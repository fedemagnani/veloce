//! `asm`: writes one listing per probe and target, normalized unless `--raw`

use std::fs;
use std::path::Path;

use crate::emit;
use crate::listing::{Listing, Probe};
use crate::normalize::{Line, normalize};
use crate::target::Target;

/// Writes `target/codegen/<target>/<probe>.s` for every target, the host when none is given
pub fn run(triples: &[String], raw: bool) -> Result<(), String> {
    for target in emit::targets(triples)? {
        let text = emit::assembly(&target)?;
        let listing = Listing::parse(&text, &target);
        let probes = listing.probes()?;
        let out_dir = emit::output_dir(&target);
        write_probes(&probes, &target, raw, &out_dir)?;
    }
    Ok(())
}

/// Replaces the contents of `out_dir` with one listing per probe, printing a line per probe
fn write_probes(
    probes: &[Probe<'_>],
    target: &Target,
    raw: bool,
    out_dir: &Path,
) -> Result<(), String> {
    if out_dir.exists() {
        fs::remove_dir_all(out_dir)
            .map_err(|error| format!("clearing {}: {error}", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir)
        .map_err(|error| format!("creating {}: {error}", out_dir.display()))?;
    println!("== {}", target.triple);
    for probe in probes {
        let path = out_dir.join(format!("{}.s", probe.name));
        let text = listing_text(probe, target, raw);
        fs::write(&path, text).map_err(|error| format!("writing {}: {error}", path.display()))?;
        let merged = probe
            .merged_into
            .map(|other| format!(" (merged into {other})"))
            .unwrap_or_default();
        println!("{}{merged}", probe.name);
    }
    println!("   listings in {}", out_dir.display());
    Ok(())
}

/// The probe's body as emitted, or normalized
fn listing_text(probe: &Probe<'_>, target: &Target, raw: bool) -> String {
    let lines: Vec<String> = if raw {
        probe.body.iter().map(|line| line.to_string()).collect()
    } else {
        let normalized = normalize(&probe.body, target);
        normalized.iter().map(Line::render).collect()
    };
    lines.join("\n") + "\n"
}
