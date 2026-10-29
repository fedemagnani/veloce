//! `asm`: emits the probe library's assembly per target and writes one listing per probe

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::listing::Listing;

/// Writes `target/codegen/<target>/<probe>.s` for every target, the host when none is given
pub fn run(targets: &[String]) -> Result<(), String> {
    let root = workspace_root();
    let targets = if targets.is_empty() {
        vec![host_target()?]
    } else {
        targets.to_vec()
    };
    for target in &targets {
        let asm_path = emit(&root, target)?;
        let text = read(&asm_path)?;
        let listing = Listing::parse(&text, target);
        let out_dir = target_dir(&root).join("codegen").join(target);
        write_probes(&listing, &out_dir)?;
    }
    Ok(())
}

/// Workspace root, the parent of this crate
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("codegen sits inside the workspace")
        .to_path_buf()
}

/// Host triple of the active toolchain
fn host_target() -> Result<String, String> {
    let output = Command::new("rustc").arg("-vV").output();
    let output = output.map_err(|error| format!("running rustc -vV: {error}"))?;
    let version = String::from_utf8_lossy(&output.stdout);
    let host = version.lines().find_map(|line| line.strip_prefix("host: "));
    host.map(str::to_string)
        .ok_or_else(|| "rustc -vV printed no host".to_string())
}

/// Builds the probe library for `target` with `--emit asm`, returning the emitted file
fn emit(root: &Path, target: &str) -> Result<PathBuf, String> {
    // the cargo running this tool, so the toolchain stays the same
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    // the release profile `cargo bench` inherits; assembly only, so no linker is needed for the target
    let args = [
        "rustc",
        "--quiet",
        "-p",
        "codegen",
        "--lib",
        "--release",
        "--target",
        target,
    ];
    let status = Command::new(&cargo)
        .current_dir(root)
        .args(args)
        .args(["--", "--emit", "asm"])
        .status();
    let status = status.map_err(|error| format!("running {cargo}: {error}"))?;
    if !status.success() {
        return Err(format!("building the probes for {target} failed"));
    }
    let release_dir = target_dir(root).join(target).join("release");
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

/// Reads a file to a string
fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("reading {}: {error}", path.display()))
}

/// Replaces the contents of `out_dir` with one listing per probe, printing a line per probe
fn write_probes(listing: &Listing, out_dir: &Path) -> Result<(), String> {
    let probes = listing.probes()?;
    if out_dir.exists() {
        fs::remove_dir_all(out_dir)
            .map_err(|error| format!("clearing {}: {error}", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir)
        .map_err(|error| format!("creating {}: {error}", out_dir.display()))?;
    println!("== {}", listing.target());
    for probe in &probes {
        let path = out_dir.join(format!("{}.s", probe.name));
        let body = probe.body.join("\n") + "\n";
        fs::write(&path, body).map_err(|error| format!("writing {}: {error}", path.display()))?;
        let merged = probe
            .merged_into
            .map(|other| format!(" (merged into {other})"))
            .unwrap_or_default();
        println!("{}{merged}", probe.name);
    }
    println!("   listings in {}", out_dir.display());
    Ok(())
}
