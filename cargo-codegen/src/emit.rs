//! Emission of a package's probe assembly, shared by every command: probes compile in a generated host crate

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use serde_json::Value;

use crate::host;
use crate::registry::{self, Registry, Watch};
use crate::target::Target;

/// Feature enabling an optional `codegen` dependency when neither the command line nor the metadata names one
pub const DEFAULT_FEATURE: &str = "codegen";

/// Target kinds that are not libraries, every other kind (`lib`, `rlib`, `cdylib`, ..) being one
const NON_LIBRARY_KINDS: [&str; 5] = ["bin", "example", "test", "bench", "custom-build"];

/// What the command line asks to build, each part defaulting to `[package.metadata.codegen]`
#[derive(Default)]
pub struct Request {
    pub package: Option<String>,
    pub features: Option<Vec<String>>,
    pub no_default_features: bool,
    pub feature: Option<String>,
}

/// A package's probes, resolved against `cargo metadata`
pub struct Package {
    pub name: String,
    /// The package's manifest, telling its build apart in cargo's messages
    manifest: PathBuf,
    /// Directory of the package's manifest, the host crate's path dependency
    dir: PathBuf,
    /// Crate name of the package's library, where its probe macros are exported
    library: String,
    /// Features the package is built with, the probe feature included
    features: Vec<String>,
    default_features: bool,
    /// Where the package gets `codegen` from, so the host enables `codegen/enabled` on that very crate
    codegen_source: String,
    /// Root of the package's workspace, whose `Cargo.lock` pins the host crate's dependencies
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
        let manifest = path_field(package, "manifest_path")?;
        let dir = manifest
            .parent()
            .ok_or_else(|| format!("manifest {} has no directory", manifest.display()))?
            .to_path_buf();
        let library = library_name(package, &name)?;
        let defaults = &package["metadata"]["codegen"];
        let dependency = codegen_dependency(package, &name)?;
        let codegen_source = dependency_source(dependency, &name)?;
        let optional = dependency["optional"] == Value::Bool(true);
        let features = features(request, defaults, optional);
        let default_features =
            !(request.no_default_features || defaults["no-default-features"] == Value::Bool(true));
        Ok(Self {
            name,
            manifest,
            dir,
            library,
            features,
            default_features,
            codegen_source,
            workspace_root,
            target_dir,
        })
    }

    /// Assembly of the probes built for `target`
    pub fn assembly(&self, target: &Target) -> Result<String, String> {
        let build = self.build(target)?;
        Ok(build.assembly)
    }

    /// The probes built for `target`: their assembly, the object it assembles to and the watched functions
    pub fn build(&self, target: &Target) -> Result<Build, String> {
        let (listing, watches) = self.emit(&target.triple)?;
        let assembly = fs::read_to_string(&listing)
            .map_err(|error| format!("reading {}: {error}", listing.display()))?;
        let object_path = listing.with_extension("o");
        let object = fs::read(&object_path)
            .map_err(|error| format!("reading {}: {error}", object_path.display()))?;
        Ok(Build {
            assembly,
            object,
            watches,
        })
    }

    /// `target/codegen/<package>`, where every command writes
    pub fn codegen_dir(&self) -> PathBuf {
        self.target_dir.join("codegen").join(&self.name)
    }

    /// Directory `asm` writes the listings of `target` to
    pub fn output_dir(&self, target: &Target) -> PathBuf {
        self.codegen_dir().join(&target.triple)
    }

    /// Builds the package, reads its probe registry, then builds the host expanding them with `--emit asm`
    fn emit(&self, triple: &str) -> Result<(PathBuf, Vec<Watch>), String> {
        let host_dir = self.write_host(&[])?;
        let rlib = self.build_package(&host_dir, triple)?;
        let Registry { probes, watches } = registry::read(&rlib)?;
        if probes.is_empty() {
            return Err(format!(
                "package {} registers no probes: define them with `#[codegen::probe]` or `codegen::probes!`",
                self.name
            ));
        }
        self.write_host(&probes)?;
        let mut command = self.cargo(&host_dir);
        // `release`, which `bench` inherits: `--profile bench` would build a bench harness needing a linker
        let args = ["rustc", "--quiet", "--lib", "--release", "--target", triple];
        command.args(args);
        // only the host crate is emitted, its dependencies build as usual; debuginfo changes no instruction
        command.args(["--", "--emit", "asm,obj", "-C", "debuginfo=limited"]);
        let status = command.status();
        let status = status.map_err(|error| format!("running cargo: {error}"))?;
        if !status.success() {
            return Err(format!(
                "building the probes of {} for {triple} failed",
                self.name
            ));
        }
        let release_dir = self.target_dir.join(triple).join("release");
        let crate_name = host::crate_name(&self.name);
        let listing = newest_listing(&release_dir, &crate_name)
            .ok_or_else(|| format!("no {crate_name}-*.s under {}", release_dir.display()))?;
        Ok((listing, watches))
    }

    /// Builds the host expanding no probes, returning the rlib of the package it depends on
    fn build_package(&self, host_dir: &Path, triple: &str) -> Result<PathBuf, String> {
        let mut command = self.cargo(host_dir);
        let args = ["build", "--quiet", "--lib", "--release", "--target", triple];
        // diagnostics still printed, artifacts reported as JSON on standard output
        command
            .args(args)
            .arg("--message-format=json-render-diagnostics");
        // the rendered diagnostics go to standard error, which `output` would otherwise swallow
        command.stderr(Stdio::inherit());
        let output = command.output();
        let output = output.map_err(|error| format!("running cargo: {error}"))?;
        if !output.status.success() {
            return Err(format!("building {} for {triple} failed", self.name));
        }
        let messages = String::from_utf8_lossy(&output.stdout);
        let manifest = self.manifest.to_string_lossy();
        rlib_artifact(&messages, &manifest)
            .ok_or_else(|| format!("cargo reported no rlib of {} for {triple}", self.name))
    }

    /// Cargo, run in the host crate's directory with the probed workspace's target dir
    fn cargo(&self, host_dir: &Path) -> Command {
        // the cargo running this tool, so the toolchain stays the same
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let mut command = Command::new(cargo);
        // inside the probed workspace's target dir, so its `.cargo/config.toml` and toolchain file still apply
        command.current_dir(host_dir);
        // the probed workspace's target dir, so its dependencies' builds are reused
        command.env("CARGO_TARGET_DIR", &self.target_dir);
        command
    }

    /// Writes the host crate under `target/codegen/<package>/host`, its lock file seeded from the workspace's
    fn write_host(&self, probes: &[String]) -> Result<PathBuf, String> {
        let host_dir = self.codegen_dir().join("host");
        fs::create_dir_all(&host_dir)
            .map_err(|error| format!("creating {}: {error}", host_dir.display()))?;
        let package_dir = self.dir.to_string_lossy();
        let spec = host::Spec {
            package: &self.name,
            package_dir: &package_dir,
            library: &self.library,
            features: &self.features,
            default_features: self.default_features,
            codegen_source: &self.codegen_source,
            probes,
        };
        let manifest = host::manifest(&spec);
        let library = host::library(&spec);
        write(&host_dir.join("Cargo.toml"), &manifest)?;
        write(&host_dir.join("lib.rs"), &library)?;
        // the workspace's versions, so the probes see the dependencies the package is really built with
        let lock = self.workspace_root.join("Cargo.lock");
        if lock.exists() {
            let host_lock = host_dir.join("Cargo.lock");
            fs::copy(&lock, &host_lock)
                .map_err(|error| format!("copying {}: {error}", lock.display()))?;
        }
        Ok(host_dir)
    }
}

/// The probes as built for one target
pub struct Build {
    pub assembly: String,
    /// The object file the same compilation assembled, its DWARF telling which inlined function each byte is from
    pub object: Vec<u8>,
    pub watches: Vec<Watch>,
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

/// Crate name of the package's library, the only target a downstream crate can depend on
fn library_name(package: &Value, name: &str) -> Result<String, String> {
    let library = library_target(package).ok_or_else(|| {
        format!("package {name} has no library, so no crate can expand its probes")
    })?;
    let library_name = string_field(library, "name")?;
    Ok(library_name.replace('-', "_"))
}

/// The package's normal dependency on `codegen`, under that name, as the attributes expand to `::codegen::`
fn codegen_dependency<'p>(package: &'p Value, name: &str) -> Result<&'p Value, String> {
    let dependencies = package["dependencies"].as_array();
    let found = dependencies.into_iter().flatten().find(|dependency| {
        let is_normal = dependency["kind"].is_null();
        let is_unrenamed = dependency["rename"].is_null();
        dependency["name"] == "codegen" && is_normal && is_unrenamed
    });
    found.ok_or_else(|| {
        format!("package {name} doesn't depend on `codegen` under that name, which its probes need")
    })
}

/// The dependency's source as TOML: `path = ..`, `version = ..` from crates.io, or `git = .., rev = ..`
fn dependency_source(dependency: &Value, name: &str) -> Result<String, String> {
    if let Some(path) = dependency["path"].as_str() {
        return Ok(format!("path = {}", toml_string(path)));
    }
    let source = dependency["source"].as_str().unwrap_or_default();
    let requirement = dependency["req"].as_str().unwrap_or("*");
    if source == "registry+https://github.com/rust-lang/crates.io-index"
        || source == "sparse+https://index.crates.io/"
    {
        return Ok(format!("version = {}", toml_string(requirement)));
    }
    if let Some(git) = source.strip_prefix("git+") {
        // `git+<url>?<branch or tag>#<commit>`: the commit, so the host gets the very same checkout
        let (url, commit) = git.split_once('#').unwrap_or((git, ""));
        let url = url.split('?').next().unwrap_or(url);
        let rev = if commit.is_empty() {
            String::new()
        } else {
            format!(", rev = {}", toml_string(commit))
        };
        return Ok(format!("git = {}{rev}", toml_string(url)));
    }
    Err(format!(
        "package {name} gets `codegen` from {source}, which cargo-codegen can't mirror"
    ))
}

/// A TOML basic string
fn toml_string(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// The command line's features, else the metadata's, plus the one enabling an optional `codegen` dependency
fn features(request: &Request, defaults: &Value, optional: bool) -> Vec<String> {
    let mut features = match &request.features {
        Some(features) => features.clone(),
        None => string_list(&defaults["features"]),
    };
    if !optional {
        return features;
    }
    let feature = request.feature.clone();
    let feature = feature.or_else(|| defaults["feature"].as_str().map(str::to_string));
    let feature = feature.unwrap_or_else(|| DEFAULT_FEATURE.to_string());
    if !features.contains(&feature) {
        features.push(feature);
    }
    features
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

/// The rlib cargo built for the package of `manifest`, read from its JSON messages
fn rlib_artifact(messages: &str, manifest: &str) -> Option<PathBuf> {
    messages.lines().find_map(|line| {
        let message: Value = serde_json::from_str(line).ok()?;
        let is_artifact = message["reason"] == "compiler-artifact";
        if !is_artifact || message["manifest_path"] != manifest {
            return None;
        }
        let filenames = string_list(&message["filenames"]);
        let rlib = filenames.into_iter().find(|file| file.ends_with(".rlib"))?;
        Some(PathBuf::from(rlib))
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

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| format!("writing {}: {error}", path.display()))
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

/// Newest `<crate_name>-*.s` under `root`, wherever the cargo version's build layout puts it
fn newest_listing(root: &Path, crate_name: &str) -> Option<PathBuf> {
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
            let is_listing = name.starts_with(&prefix) && name.ends_with(".s");
            if !is_listing {
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
    fn the_library_name_has_hyphens_replaced() {
        let targets = serde_json::json!([
            { "name": "my-crate", "kind": ["rlib", "cdylib"] },
            { "name": "tool", "kind": ["bin"] },
        ]);
        let demo = package(targets, Value::Null);
        let library = library_name(&demo, "demo");
        assert_eq!(library, Ok("my_crate".to_string()));
    }

    #[test]
    fn a_package_without_a_library_has_no_probes() {
        let binary_only = package(
            serde_json::json!([{ "name": "tool", "kind": ["bin"] }]),
            Value::Null,
        );
        let library = library_name(&binary_only, "demo");
        assert!(library.unwrap_err().contains("has no library"));
    }

    #[test]
    fn an_optional_codegen_gets_its_feature_enabled_once() {
        let defaults = serde_json::json!({ "features": ["fast", "codegen"] });
        let renamed = serde_json::json!({ "features": ["fast"], "feature": "probes" });
        let none = Request::default();
        let listed = features(&none, &defaults, true);
        let named = features(&none, &renamed, true);
        let default = features(&none, &Value::Null, true);
        let plain = features(&none, &renamed, false);
        assert_eq!(listed, ["fast", "codegen"]);
        assert_eq!(named, ["fast", "probes"]);
        assert_eq!(default, ["codegen"]);
        assert_eq!(plain, ["fast"]);
    }

    #[test]
    fn the_codegen_source_is_mirrored() {
        let path = serde_json::json!({ "path": "/work/codegen" });
        let registry = serde_json::json!({ "source": "registry+https://github.com/rust-lang/crates.io-index", "req": "^0.3" });
        let git = serde_json::json!({ "source": "git+https://github.com/me/codegen?branch=main#0123abc" });
        let other = serde_json::json!({ "source": "registry+https://my.registry/index" });
        let path = dependency_source(&path, "demo");
        let registry = dependency_source(&registry, "demo");
        let git = dependency_source(&git, "demo");
        let other = dependency_source(&other, "demo");
        assert_eq!(path, Ok(r#"path = "/work/codegen""#.to_string()));
        assert_eq!(registry, Ok(r#"version = "^0.3""#.to_string()));
        assert_eq!(
            git,
            Ok(r#"git = "https://github.com/me/codegen", rev = "0123abc""#.to_string())
        );
        assert!(other.unwrap_err().contains("can't mirror"));
    }

    #[test]
    fn packages_are_picked_by_name() {
        let packages = [package(serde_json::json!([]), Value::Null)];
        let found = pick_package(&packages, Some("demo")).map(|package| &package["name"]);
        let missing = pick_package(&packages, Some("other"));
        assert_eq!(found, Ok(&Value::from("demo")));
        assert!(missing.unwrap_err().contains("no package other"));
    }

    #[test]
    fn the_rlib_is_the_artifact_of_the_package_manifest() {
        let messages = [
            r#"{"reason":"compiler-artifact","manifest_path":"/dep/Cargo.toml","filenames":["/t/libdep.rlib"]}"#,
            r#"{"reason":"compiler-artifact","manifest_path":"/work/demo/Cargo.toml","filenames":["/t/libdemo.rmeta","/t/libdemo.rlib"]}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");
        let rlib = rlib_artifact(&messages, "/work/demo/Cargo.toml");
        let missing = rlib_artifact(&messages, "/other/Cargo.toml");
        assert_eq!(rlib, Some(PathBuf::from("/t/libdemo.rlib")));
        assert_eq!(missing, None);
    }
}
