//! Emission of the probe library's assembly, shared by every command

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::target::Target;

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

/// Assembly of the probe library built for `target`
pub fn assembly(target: &Target) -> Result<String, String> {
    let root = workspace_root();
    let path = emit(&root, &target.triple)?;
    fs::read_to_string(&path).map_err(|error| format!("reading {}: {error}", path.display()))
}

/// Directory `asm` writes the listings of `target` to
pub fn output_dir(target: &Target) -> PathBuf {
    codegen_dir().join(&target.triple)
}

/// `target/codegen`, where every command writes
pub fn codegen_dir() -> PathBuf {
    let root = workspace_root();
    target_dir(&root).join("codegen")
}

/// `rustc -V` of the active toolchain, the compiler `assembly` builds with
pub fn rustc_version() -> Result<String, String> {
    let version = rustc("-V")?;
    Ok(version.trim().to_string())
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

/// Workspace root, the parent of this crate
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("codegen sits inside the workspace")
        .to_path_buf()
}

/// Builds the probe library for `triple` with `--emit asm`, returning the emitted file
fn emit(root: &Path, triple: &str) -> Result<PathBuf, String> {
    // the cargo running this tool, so the toolchain stays the same
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    // `release`, which `bench` inherits: `--profile bench` would build the lib as a bench harness needing a linker
    let args = [
        "rustc",
        "--quiet",
        "-p",
        "codegen",
        "--lib",
        "--release",
        "--target",
        triple,
    ];
    let status = Command::new(&cargo)
        .current_dir(root)
        .args(args)
        .args(["--", "--emit", "asm"])
        .status();
    let status = status.map_err(|error| format!("running {cargo}: {error}"))?;
    if !status.success() {
        return Err(format!("building the probes for {triple} failed"));
    }
    let release_dir = target_dir(root).join(triple).join("release");
    newest_listing(&release_dir)
        .ok_or_else(|| format!("no codegen-*.s under {}", release_dir.display()))
}

/// Target directory of the workspace
fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => root.join("target"),
    }
}

/// Newest `codegen-*.s` under `dir`, its location depending on the cargo version's build directory layout
fn newest_listing(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    let mut pending = vec![dir.to_path_buf()];
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
            if !(name.starts_with("codegen-") && name.ends_with(".s")) {
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
