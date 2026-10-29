//! Emission of a package's probe assembly, shared by every command

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde_json::Value;

use crate::target::Target;

/// `--cfg` the probes are compiled under when neither the command line nor the metadata names one
pub const DEFAULT_CFG: &str = "codegen";

/// Target kinds that are not libraries, every other kind (`lib`, `rlib`, `cdylib`, ..) being one
const NON_LIBRARY_KINDS: [&str; 5] = ["bin", "example", "test", "bench", "custom-build"];

/// What the command line asks to build, each part defaulting to `[package.metadata.codegen]`
#[derive(Default)]
pub struct Request {
    pub package: Option<String>,
    pub selection: Option<Selection>,
    pub features: Option<Vec<String>>,
    pub no_default_features: bool,
    pub cfg: Option<String>,
}

/// The target of a package holding the probes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Lib,
    /// An example built as a library, `crate-type = ["lib"]`, so it needs no `main`
    Example(String),
}

/// A package's probe target, resolved against `cargo metadata`
pub struct Package {
    pub name: String,
    selection: Selection,
    /// Name of the probe target's crate, the stem of the assembly file rustc emits
    crate_name: String,
    features: Vec<String>,
    no_default_features: bool,
    /// Passed to the probed crate alone as `--cfg`, so modules gated on it compile only for the tool
    cfg: String,
    workspace_root: PathBuf,
    target_dir: PathBuf,
}

impl Package {
    /// Resolves `request` in the workspace of the current directory
    pub fn resolve(request: &Request) -> Result<Self, String> {
        let metadata = cargo_metadata()?;
        let workspace_root = path_field(&metadata, "workspace_root")?;
        let target_dir = path_field(&metadata, "target_directory")?;
        let packages = metadata["packages"]
            .as_array()
            .ok_or("cargo metadata listed no packages")?;
        let package = pick_package(packages, request.package.as_deref())?;
        let name = string_field(package, "name")?;
        let defaults = &package["metadata"]["codegen"];
        let selection = selection(request, defaults, package, &name)?;
        let crate_name = crate_name(package, &selection, &name)?;
        let features = match &request.features {
            Some(features) => features.clone(),
            None => string_list(&defaults["features"]),
        };
        let no_default_features =
            request.no_default_features || defaults["no-default-features"] == Value::Bool(true);
        let cfg = request.cfg.clone();
        let cfg = cfg.or_else(|| defaults["cfg"].as_str().map(str::to_string));
        let cfg = cfg.unwrap_or_else(|| DEFAULT_CFG.to_string());
        Ok(Self {
            name,
            selection,
            crate_name,
            features,
            no_default_features,
            cfg,
            workspace_root,
            target_dir,
        })
    }

    /// Assembly of the probe target built for `target`
    pub fn assembly(&self, target: &Target) -> Result<String, String> {
        let path = self.emit(&target.triple)?;
        fs::read_to_string(&path).map_err(|error| format!("reading {}: {error}", path.display()))
    }

    /// `target/codegen/<package>`, where every command writes
    pub fn codegen_dir(&self) -> PathBuf {
        self.target_dir.join("codegen").join(&self.name)
    }

    /// Directory `asm` writes the listings of `target` to
    pub fn output_dir(&self, target: &Target) -> PathBuf {
        self.codegen_dir().join(&target.triple)
    }

    /// Builds the probe target for `triple` with `--emit asm`, returning the emitted file
    fn emit(&self, triple: &str) -> Result<PathBuf, String> {
        // the cargo running this tool, so the toolchain stays the same
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let mut command = Command::new(&cargo);
        command.current_dir(&self.workspace_root);
        command.args(["rustc", "--quiet", "-p", &self.name]);
        match &self.selection {
            Selection::Lib => command.arg("--lib"),
            Selection::Example(example) => command.args(["--example", example]),
        };
        // `release`, which `bench` inherits: `--profile bench` would build a bench harness needing a linker
        command.args(["--release", "--target", triple]);
        if !self.features.is_empty() {
            command.args(["--features", &self.features.join(",")]);
        }
        if self.no_default_features {
            command.arg("--no-default-features");
        }
        // after `--`, so only the probed crate sees the cfg and its dependencies build as usual
        command.args(["--", "--cfg", &self.cfg, "--emit", "asm"]);
        let status = command.status();
        let status = status.map_err(|error| format!("running {cargo}: {error}"))?;
        if !status.success() {
            return Err(format!(
                "building the probes of {} for {triple} failed",
                self.name
            ));
        }
        let release_dir = self.target_dir.join(triple).join("release");
        let in_examples = matches!(self.selection, Selection::Example(_));
        newest_listing(&release_dir, &self.crate_name, in_examples).ok_or_else(|| {
            let dir = release_dir.display();
            format!("no {}-*.s under {dir}", self.crate_name)
        })
    }
}

/// The targets requested, the host when none is
pub fn targets(requested: &[String]) -> Result<Vec<Target>, String> {
    if requested.is_empty() {
        let host = host_triple()?;
        let target = Target::parse(&host)?;
        return Ok(vec![target]);
    }
    requested
        .iter()
        .map(|triple| Target::parse(triple))
        .collect()
}

/// `rustc -V` of the active toolchain, the compiler `assembly` builds with
pub fn rustc_version() -> Result<String, String> {
    let version = rustc("-V")?;
    Ok(version.trim().to_string())
}

/// `cargo metadata` of the workspace of the current directory, its own packages only
fn cargo_metadata() -> Result<Value, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let args = ["metadata", "--format-version", "1", "--no-deps"];
    let output = Command::new(&cargo).args(args).output();
    let output = output.map_err(|error| format!("running {cargo} metadata: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{cargo} metadata failed: {}", stderr.trim()));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parsing cargo metadata: {error}"))
}

/// The package named `requested`, else the one whose directory holds the current directory most closely
fn pick_package<'a>(packages: &'a [Value], requested: Option<&str>) -> Result<&'a Value, String> {
    let names: Vec<&str> = packages
        .iter()
        .filter_map(|package| package["name"].as_str())
        .collect();
    if let Some(requested) = requested {
        return packages
            .iter()
            .find(|package| package["name"] == requested)
            .ok_or_else(|| {
                format!(
                    "no package {requested} in the workspace: {}",
                    names.join(", ")
                )
            });
    }
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let enclosing = packages.iter().filter_map(|package| {
        let manifest = package["manifest_path"].as_str()?;
        let dir = Path::new(manifest).parent()?;
        cwd.starts_with(dir)
            .then_some((dir.components().count(), package))
    });
    let closest = enclosing.max_by_key(|(depth, _)| *depth);
    closest.map(|(_, package)| package).ok_or_else(|| {
        let names = names.join(", ");
        format!("the current directory is in no package: pass --package, one of {names}")
    })
}

/// The command line's target, else the metadata's `example` or `lib = true`, else the package's library
fn selection(
    request: &Request,
    defaults: &Value,
    package: &Value,
    name: &str,
) -> Result<Selection, String> {
    if let Some(selection) = &request.selection {
        return Ok(selection.clone());
    }
    if let Some(example) = defaults["example"].as_str() {
        return Ok(Selection::Example(example.to_string()));
    }
    if defaults["lib"] == Value::Bool(true) || library_target(package).is_some() {
        return Ok(Selection::Lib);
    }
    Err(format!(
        "package {name} has no library: pass --example, or set `example` in [package.metadata.codegen]"
    ))
}

/// Crate name of the selected target, checking the package has it
fn crate_name(package: &Value, selection: &Selection, name: &str) -> Result<String, String> {
    let target = match selection {
        Selection::Lib => library_target(package)
            .ok_or_else(|| format!("package {name} has no library target"))?,
        Selection::Example(example) => example_target(package, example)
            .ok_or_else(|| format!("package {name} has no example {example}"))?,
    };
    let target_name = string_field(target, "name")?;
    Ok(target_name.replace('-', "_"))
}

/// The package's library target
fn library_target(package: &Value) -> Option<&Value> {
    let targets = package["targets"].as_array()?;
    targets.iter().find(|target| {
        let kinds = string_list(&target["kind"]);
        let is_library = |kind: &String| !NON_LIBRARY_KINDS.contains(&kind.as_str());
        !kinds.is_empty() && kinds.iter().all(is_library)
    })
}

/// The package's example `example`
fn example_target<'a>(package: &'a Value, example: &str) -> Option<&'a Value> {
    let targets = package["targets"].as_array()?;
    targets.iter().find(|target| {
        let kinds = string_list(&target["kind"]);
        target["name"] == example && kinds.iter().any(|kind| kind == "example")
    })
}

/// Strings of a JSON array, empty when the value is missing or holds something else
fn string_list(value: &Value) -> Vec<String> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn string_field(value: &Value, field: &str) -> Result<String, String> {
    let text = value[field].as_str();
    text.map(str::to_string)
        .ok_or_else(|| format!("cargo metadata has no {field}"))
}

fn path_field(value: &Value, field: &str) -> Result<PathBuf, String> {
    string_field(value, field).map(PathBuf::from)
}

/// Host triple of the active toolchain
fn host_triple() -> Result<String, String> {
    let version = rustc("-vV")?;
    let host = version.lines().find_map(|line| line.strip_prefix("host: "));
    host.map(str::to_string)
        .ok_or_else(|| "rustc -vV printed no host".to_string())
}

/// Standard output of `rustc <flag>`
fn rustc(flag: &str) -> Result<String, String> {
    let output = Command::new("rustc").arg(flag).output();
    let output = output.map_err(|error| format!("running rustc {flag}: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Newest `<crate_name>-*.s` under `root`, inside or outside an `examples` directory, wherever the build layout puts it
fn newest_listing(root: &Path, crate_name: &str, in_examples: bool) -> Option<PathBuf> {
    let prefix = format!("{crate_name}-");
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // below `root` only, so a checkout under some `examples` directory doesn't count
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let under_examples = relative
                .components()
                .any(|part| part.as_os_str() == "examples");
            let is_listing = name.starts_with(&prefix) && name.ends_with(".s");
            if !is_listing || under_examples != in_examples {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
                continue;
            };
            let is_newer = newest.as_ref().is_none_or(|(time, _)| modified > *time);
            if is_newer {
                newest = Some((modified, path));
            }
        }
    }
    newest.map(|(_, path)| path)
}

#[cfg(test)]
mod test {
    use super::*;

    /// A package as `cargo metadata` lists it, with the given targets and codegen metadata
    fn package(targets: Value, codegen: Value) -> Value {
        serde_json::json!({
            "name": "demo",
            "manifest_path": "/work/demo/Cargo.toml",
            "targets": targets,
            "metadata": { "codegen": codegen },
        })
    }

    #[test]
    fn selection_prefers_the_command_line_then_the_metadata_then_the_library() {
        let targets = serde_json::json!([
            { "name": "demo", "kind": ["lib"] },
            { "name": "probes", "kind": ["example"] },
        ]);
        let with_example = package(targets.clone(), serde_json::json!({ "example": "probes" }));
        let without = package(targets, Value::Null);
        let given = Request {
            selection: Some(Selection::Lib),
            ..Request::default()
        };
        let none = Request::default();
        let from_line = selection(
            &given,
            &with_example["metadata"]["codegen"],
            &with_example,
            "demo",
        );
        let from_metadata = selection(
            &none,
            &with_example["metadata"]["codegen"],
            &with_example,
            "demo",
        );
        let from_lib = selection(&none, &without["metadata"]["codegen"], &without, "demo");
        assert_eq!(from_line, Ok(Selection::Lib));
        assert_eq!(from_metadata, Ok(Selection::Example("probes".to_string())));
        assert_eq!(from_lib, Ok(Selection::Lib));
    }

    #[test]
    fn a_package_without_a_library_needs_an_example() {
        let binary_only = package(
            serde_json::json!([{ "name": "tool", "kind": ["bin"] }]),
            Value::Null,
        );
        let none = Request::default();
        let picked = selection(&none, &Value::Null, &binary_only, "tool");
        assert!(picked.unwrap_err().contains("has no library"));
    }

    #[test]
    fn crate_names_come_from_the_target_with_hyphens_replaced() {
        let targets = serde_json::json!([
            { "name": "my-crate", "kind": ["rlib", "cdylib"] },
            { "name": "my-probes", "kind": ["example"] },
        ]);
        let demo = package(targets, Value::Null);
        let lib = crate_name(&demo, &Selection::Lib, "demo");
        let example = crate_name(&demo, &Selection::Example("my-probes".to_string()), "demo");
        let missing = crate_name(&demo, &Selection::Example("other".to_string()), "demo");
        assert_eq!(lib, Ok("my_crate".to_string()));
        assert_eq!(example, Ok("my_probes".to_string()));
        assert!(missing.unwrap_err().contains("no example other"));
    }

    #[test]
    fn packages_are_picked_by_name() {
        let packages = [package(serde_json::json!([]), Value::Null)];
        let found = pick_package(&packages, Some("demo")).map(|package| &package["name"]);
        let missing = pick_package(&packages, Some("other"));
        assert_eq!(found, Ok(&Value::from("demo")));
        assert!(missing.unwrap_err().contains("no package other"));
    }
}
