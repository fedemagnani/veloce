//! `save` and `diff`: named baselines of normalized listings and stats under `target/codegen/<package>/baselines/`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use similar::TextDiff;

use crate::asm::listing_text;
use crate::emit::{self, Package};
use crate::listing::Listing;
use crate::stats::{self, COLUMNS};
use crate::target::Target;

/// Baseline `save` and `diff` use when none is named
pub const DEFAULT_BASELINE: &str = "base";
/// File holding the stats table of a target's baseline, the source of the deltas `diff` reports
const STATS_FILE: &str = "stats.txt";
/// File holding the `rustc -V` line of the compiler that emitted a target's baseline
const TOOLCHAIN_FILE: &str = "toolchain";
/// Lines of context around each change in a listing diff
const CONTEXT: usize = 3;

/// Saves fresh builds as `baseline`, for the given targets, else those it already holds, else the host
pub fn save(package: &Package, triples: &[String], baseline: &str) -> Result<(), String> {
    for target in baseline_targets(package, triples, baseline, Fallback::Host)? {
        let snapshot = Snapshot::build(package, &target)?;
        let dir = baseline_dir(package, baseline).join(&target.triple);
        snapshot.write(&dir)?;
        let count = snapshot.listings.len();
        println!(
            "saved {count} probes for {} as baseline `{baseline}`",
            target.triple
        );
    }
    Ok(())
}

/// Compares fresh builds to `baseline`, failing when a probe was added, removed or changed
pub fn diff(package: &Package, triples: &[String], baseline: &str) -> Result<(), String> {
    let mut drifted = Vec::new();
    for target in baseline_targets(package, triples, baseline, Fallback::Error)? {
        let dir = baseline_dir(package, baseline).join(&target.triple);
        let old = Snapshot::read(&dir, &target.triple, baseline)?;
        let new = Snapshot::build(package, &target)?;
        let drift = Drift::between(&old, &new);
        print!("{}", drift.report(&target.triple, baseline, &old, &new));
        if !drift.is_empty() {
            drifted.push(target.triple);
        }
    }
    if drifted.is_empty() {
        return Ok(());
    }
    let targets = drifted.join(", ");
    Err(format!("drifted from baseline `{baseline}` for {targets}"))
}

/// What to fall back to when no target is given and the baseline holds none
enum Fallback {
    Host,
    Error,
}

/// The given targets, else every target `baseline` holds
fn baseline_targets(
    package: &Package,
    triples: &[String],
    baseline: &str,
    fallback: Fallback,
) -> Result<Vec<Target>, String> {
    if !triples.is_empty() {
        return emit::targets(triples);
    }
    let saved = saved_triples(package, baseline);
    if !saved.is_empty() {
        return emit::targets(&saved);
    }
    match fallback {
        Fallback::Host => emit::targets(&[]),
        Fallback::Error => Err(format!(
            "no baseline `{baseline}`: save one first with `save --baseline {baseline}`"
        )),
    }
}

/// Triples with a directory in `baseline`, sorted
fn saved_triples(package: &Package, baseline: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(baseline_dir(package, baseline)) else {
        return Vec::new();
    };
    let mut triples: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    triples.sort();
    triples
}

/// `target/codegen/<package>/baselines/<baseline>`, local to the checkout as criterion's baselines are
fn baseline_dir(package: &Package, baseline: &str) -> PathBuf {
    package.codegen_dir().join("baselines").join(baseline)
}

/// Normalized listings, stats and compiler of one target
struct Snapshot {
    listings: BTreeMap<String, String>,
    stats: BTreeMap<String, [usize; 10]>,
    /// The stats table as `stats` prints it, written verbatim
    table: String,
    toolchain: String,
}

impl Snapshot {
    /// The probes of a fresh build for `target`
    fn build(package: &Package, target: &Target) -> Result<Self, String> {
        let text = package.assembly(target)?;
        let listing = Listing::parse(&text, target);
        let probes = listing.probes()?;
        let rows = stats::rows(&probes, target);
        let listings = probes
            .iter()
            .map(|probe| (probe.name.to_string(), listing_text(probe, target, false)))
            .collect();
        let stats = rows
            .iter()
            .map(|(probe, stats)| (probe.name.to_string(), stats.cells()))
            .collect();
        let table = stats::table(&rows);
        let toolchain = emit::rustc_version()?;
        Ok(Self {
            listings,
            stats,
            table,
            toolchain,
        })
    }

    /// The snapshot saved in `dir`, for `triple` in `baseline`
    fn read(dir: &Path, triple: &str, baseline: &str) -> Result<Self, String> {
        let entries = fs::read_dir(dir).map_err(|error| {
            format!(
                "no {triple} in baseline `{baseline}` ({}): {error}",
                dir.display()
            )
        })?;
        let mut listings = BTreeMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let is_listing = path.extension().is_some_and(|extension| extension == "s");
            let Some(name) = path.file_stem().filter(|_| is_listing) else {
                continue;
            };
            let text = read_file(&path)?;
            listings.insert(name.to_string_lossy().into_owned(), text);
        }
        let table = read_file(&dir.join(STATS_FILE))?;
        let stats = stats::parse_table(&table);
        let toolchain = read_file(&dir.join(TOOLCHAIN_FILE))?;
        Ok(Self {
            listings,
            stats,
            table,
            toolchain: toolchain.trim().to_string(),
        })
    }

    /// Replaces `dir` with one listing per probe, the stats table and the compiler version
    fn write(&self, dir: &Path) -> Result<(), String> {
        if dir.exists() {
            fs::remove_dir_all(dir)
                .map_err(|error| format!("clearing {}: {error}", dir.display()))?;
        }
        fs::create_dir_all(dir).map_err(|error| format!("creating {}: {error}", dir.display()))?;
        for (name, text) in &self.listings {
            write_file(&dir.join(format!("{name}.s")), text)?;
        }
        write_file(&dir.join(STATS_FILE), &self.table)?;
        write_file(&dir.join(TOOLCHAIN_FILE), &format!("{}\n", self.toolchain))
    }
}

/// Probes added, removed or with a different listing, each list sorted
#[derive(Debug, Default, PartialEq, Eq)]
struct Drift {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<String>,
}

impl Drift {
    /// Differences from the baseline `old` to the fresh build `new`
    fn between(old: &Snapshot, new: &Snapshot) -> Self {
        let mut drift = Self::default();
        for (name, text) in &new.listings {
            match old.listings.get(name) {
                None => drift.added.push(name.clone()),
                Some(old_text) if old_text != text => drift.changed.push(name.clone()),
                Some(_) => {}
            }
        }
        for name in old.listings.keys() {
            if !new.listings.contains_key(name) {
                drift.removed.push(name.clone());
            }
        }
        drift
    }

    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Header, compiler note, delta table of drifted probes and a unified diff per changed listing
    fn report(&self, triple: &str, baseline: &str, old: &Snapshot, new: &Snapshot) -> String {
        let mut out = format!("== {triple} against baseline `{baseline}`\n");
        if old.toolchain != new.toolchain {
            let note = "drift may come from the compiler, not the code";
            out += &format!(
                "note: baseline from {}, built with {}: {note}\n",
                old.toolchain, new.toolchain
            );
        }
        if self.is_empty() {
            out += &format!("no drift in {} probes\n", new.listings.len());
            return out;
        }
        out += &self.delta_table(old, new);
        for name in &self.changed {
            let old_text = &old.listings[name];
            let new_text = &new.listings[name];
            let old_header = format!("baseline {baseline}/{triple}/{name}.s");
            let diff = TextDiff::from_lines(old_text, new_text);
            let unified = diff
                .unified_diff()
                .context_radius(CONTEXT)
                .header(&old_header, "fresh build")
                .to_string();
            out += &unified;
        }
        out
    }

    /// One row per drifted probe: its status, then each count as `old→new` where it moved
    fn delta_table(&self, old: &Snapshot, new: &Snapshot) -> String {
        let mut rows: Vec<(&str, &str)> = Vec::new();
        rows.extend(self.changed.iter().map(|name| (name.as_str(), "changed")));
        rows.extend(self.added.iter().map(|name| (name.as_str(), "added")));
        rows.extend(self.removed.iter().map(|name| (name.as_str(), "removed")));
        rows.sort();
        let cells: Vec<Vec<String>> = rows
            .iter()
            .map(|(name, _)| {
                let before = old.stats.get(*name);
                let after = new.stats.get(*name);
                (0..COLUMNS.len())
                    .map(|i| delta_cell(before.map(|c| c[i]), after.map(|c| c[i])))
                    .collect()
            })
            .collect();
        let name_width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
        let name_width = name_width.max("probe".len());
        let widths: Vec<usize> = COLUMNS
            .iter()
            .enumerate()
            .map(|(i, column)| {
                let widest = cells
                    .iter()
                    .map(|row| row[i].chars().count())
                    .max()
                    .unwrap_or(0);
                widest.max(column.len()) + 1
            })
            .collect();
        let header: Vec<String> = COLUMNS
            .iter()
            .zip(&widths)
            .map(|(column, width)| format!("{column:>width$}"))
            .collect();
        let mut out = format!(
            "{:<name_width$} {:<8}{}\n",
            "probe",
            "status",
            header.join("")
        );
        for ((name, status), row) in rows.iter().zip(&cells) {
            let row: Vec<String> = row
                .iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:>width$}"))
                .collect();
            out += &format!("{name:<name_width$} {status:<8}{}\n", row.join(""));
        }
        out
    }
}

/// `n` when unchanged, `a→b` when moved, `-` standing for a probe missing on one side
fn delta_cell(before: Option<usize>, after: Option<usize>) -> String {
    match (before, after) {
        (Some(a), Some(b)) if a == b => a.to_string(),
        (Some(a), Some(b)) => format!("{a}→{b}"),
        (None, Some(b)) => format!("-→{b}"),
        (Some(a), None) => format!("{a}→-"),
        (None, None) => "-".to_string(),
    }
}

fn read_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("reading {}: {error}", path.display()))
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| format!("writing {}: {error}", path.display()))
}

#[cfg(test)]
mod test {
    use super::*;

    /// Snapshot of in-memory listings, each probe's stats being its listing's line count in every column
    fn snapshot(listings: &[(&str, &str)], toolchain: &str) -> Snapshot {
        let listings: BTreeMap<String, String> = listings
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect();
        let stats = listings
            .iter()
            .map(|(name, text)| (name.clone(), [text.lines().count(); 10]))
            .collect();
        Snapshot {
            listings,
            stats,
            table: String::new(),
            toolchain: toolchain.to_string(),
        }
    }

    #[test]
    fn drift_sorts_probes_into_added_removed_and_changed() {
        let old = snapshot(
            &[("kept", "ret\n"), ("edited", "ret\n"), ("gone", "ret\n")],
            "rustc 1",
        );
        let new = snapshot(
            &[
                ("kept", "ret\n"),
                ("edited", "nop\nret\n"),
                ("fresh", "ret\n"),
            ],
            "rustc 1",
        );
        let drift = Drift::between(&old, &new);
        let expected = Drift {
            added: vec!["fresh".to_string()],
            removed: vec!["gone".to_string()],
            changed: vec!["edited".to_string()],
        };
        assert_eq!(drift, expected);
    }

    #[test]
    fn unchanged_snapshots_report_no_drift() {
        let old = snapshot(&[("probe", "ret\n")], "rustc 1");
        let new = snapshot(&[("probe", "ret\n")], "rustc 1");
        let drift = Drift::between(&old, &new);
        let report = drift.report("aarch64-apple-darwin", "base", &old, &new);
        assert!(drift.is_empty());
        assert_eq!(
            report,
            "== aarch64-apple-darwin against baseline `base`\nno drift in 1 probes\n"
        );
    }

    #[test]
    fn report_shows_moved_counts_a_unified_diff_and_a_compiler_note() {
        let old = snapshot(&[("probe", "add\nret\n")], "rustc 1");
        let new = snapshot(&[("probe", "add\nbl\tpanic\nret\n")], "rustc 2");
        let drift = Drift::between(&old, &new);
        let report = drift.report("aarch64-apple-darwin", "base", &old, &new);
        assert!(
            report.contains("note: baseline from rustc 1, built with rustc 2"),
            "{report}"
        );
        assert!(report.contains("probe changed"), "{report}");
        assert!(report.contains("2→3"), "{report}");
        assert!(report.contains("+bl\tpanic"), "{report}");
    }

    #[test]
    fn delta_cells_mark_moves_and_missing_sides() {
        let unchanged = delta_cell(Some(4), Some(4));
        let moved = delta_cell(Some(4), Some(6));
        let added = delta_cell(None, Some(6));
        let removed = delta_cell(Some(4), None);
        assert_eq!(
            [unchanged, moved, added, removed],
            ["4", "4→6", "-→6", "4→-"]
        );
    }

    #[test]
    fn written_snapshots_read_back_identically() {
        let dir = std::env::temp_dir().join(format!("codegen-snapshot-{}", std::process::id()));
        let mut written = snapshot(&[("probe_a", "probe_a:\n\tret\n")], "rustc 1");
        written.table = "probe      instr\nprobe_a        1\n".to_string();
        written.write(&dir).unwrap();
        let read = Snapshot::read(&dir, "test", "base").unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(read.listings, written.listings);
        assert_eq!(read.table, written.table);
        assert_eq!(read.toolchain, "rustc 1");
    }
}
