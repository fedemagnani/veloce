//! `stats`: per probe counts of the instruction classes that decide branching, memory traffic and ordering

use std::collections::{BTreeMap, BTreeSet};

use crate::attribution::Inlines;
use crate::emit::{self, Build, Package};
use crate::isa::classify;
use crate::listing::{Listing, Probe};
use crate::normalize::{Instruction, Line, layout, normalize};
use crate::target::{Arch, Target};

/// Functions a failed check ends in, so referencing one means a check survived optimization
const PANIC_PATHS: [&str; 10] = [
    "core::panicking::",
    "core::panic::",
    "std::panicking::",
    "core::option::unwrap_failed",
    "core::option::expect_failed",
    "core::result::unwrap_failed",
    "core::slice::index::",
    "core::str::slice_error_fail",
    "alloc::alloc::handle_alloc_error",
    "alloc::raw_vec::",
];

/// Column headers of the table, after the probe name
pub const COLUMNS: [&str; 10] = [
    "instr", "branch", "jump", "select", "load", "store", "simd", "barrier", "call", "panic",
];

/// Prints a table of every probe's stats per target, the host when none is given
pub fn run(package: &Package, triples: &[String]) -> Result<(), String> {
    for target in emit::targets(triples)? {
        let build = package.build(&target)?;
        let listing = Listing::parse(&build.assembly, &target);
        let probes = listing.probes()?;
        let rows = rows(&probes, &target);
        println!("== {}", target.triple);
        print!("{}", table(&rows));
        if !build.watches.is_empty() {
            let watched = watched_rows(&probes, &build, &target)?;
            print!("{}", watched_table(&watched));
        }
    }
    Ok(())
}

/// A watched function's share of a probe, or why the probe couldn't be attributed
pub enum WatchedRow {
    Share {
        probe: String,
        function: String,
        stats: Stats,
    },
    Unavailable {
        probe: String,
        reason: String,
    },
}

/// One row per watched function each probe inlines, probes sorted by name
fn watched_rows(
    probes: &[Probe<'_>],
    build: &Build,
    target: &Target,
) -> Result<Vec<WatchedRow>, String> {
    let inlines = Inlines::parse(&build.object)?;
    let mut sorted: Vec<&Probe<'_>> = probes.iter().collect();
    sorted.sort_by_key(|probe| probe.name);
    let mut rows = Vec::new();
    for probe in sorted {
        // a merged probe's symbol and DWARF are those of the body it shares
        let source = probe.merged_into.unwrap_or(probe.name);
        let shares = shares(&inlines, probe, source, build, target);
        match shares {
            Ok(shares) => rows.extend(shares),
            Err(reason) => rows.push(WatchedRow::Unavailable {
                probe: probe.name.to_string(),
                reason,
            }),
        }
    }
    Ok(rows)
}

/// The share of each watched function inlined into `probe`, its instances merged
fn shares(
    inlines: &Inlines<'_>,
    probe: &Probe<'_>,
    source: &str,
    build: &Build,
    target: &Target,
) -> Result<Vec<WatchedRow>, String> {
    let watched = inlines.watched(source, &build.watches)?;
    if watched.is_empty() {
        return Ok(Vec::new());
    }
    let slots = layout(&probe.body, target);
    let offsets = inlines.offsets(source, &slots, target)?;
    let lines = normalize(&probe.body, target);
    let mut by_watch: BTreeMap<usize, (String, Vec<std::ops::Range<u64>>)> = BTreeMap::new();
    for instance in watched {
        let entry = by_watch
            .entry(instance.watch)
            .or_insert((instance.display, Vec::new()));
        entry.1.extend(instance.ranges);
    }
    let rows = by_watch.into_values().map(|(function, ranges)| {
        let covered = |index: usize| ranges.iter().any(|range| range.contains(&offsets[index]));
        let stats = Stats::of_selected(&lines, covered, target.arch);
        WatchedRow::Share {
            probe: probe.name.to_string(),
            function,
            stats,
        }
    });
    Ok(rows.collect())
}

/// The table of watched functions' shares, an unattributed probe shown with its reason
fn watched_table(rows: &[WatchedRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let label = |row: &WatchedRow| match row {
        WatchedRow::Share {
            probe, function, ..
        } => format!("{probe} > {function}"),
        WatchedRow::Unavailable { probe, .. } => probe.clone(),
    };
    let width = rows.iter().map(|row| label(row).len()).max().unwrap_or(0);
    let width = width.max("watched".len());
    let header: Vec<String> = COLUMNS
        .iter()
        .map(|column| format!("{column:>8}"))
        .collect();
    let mut out = format!("{:<width$} {}\n", "watched", header.join(""));
    for row in rows {
        let cells = match row {
            WatchedRow::Share { stats, .. } => {
                let cells: Vec<String> = stats
                    .cells()
                    .iter()
                    .map(|cell| format!("{cell:>8}"))
                    .collect();
                cells.join("")
            }
            WatchedRow::Unavailable { reason, .. } => {
                format!("  attribution unavailable: {reason}")
            }
        };
        out += &format!("{:<width$} {cells}\n", label(row));
    }
    out
}

/// Stats of every probe, sorted by name
pub fn rows<'p, 'a>(probes: &'p [Probe<'a>], target: &Target) -> Vec<(&'p Probe<'a>, Stats)> {
    let mut rows: Vec<(&Probe<'a>, Stats)> = probes
        .iter()
        .map(|probe| {
            let lines = normalize(&probe.body, target);
            (probe, Stats::of(&lines, target.arch))
        })
        .collect();
    rows.sort_by_key(|(probe, _)| probe.name);
    rows
}

/// Counts over one normalized body
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub instructions: usize,
    pub cond_branches: usize,
    pub jumps: usize,
    pub selects: usize,
    pub loads: usize,
    pub stores: usize,
    pub simd: usize,
    pub barriers: usize,
    pub calls: usize,
    /// Calls straight into a panic function
    pub panic_calls: usize,
    /// Count per ordering mnemonic, such as `ldar` or `xchgq`
    pub barrier_kinds: BTreeMap<String, usize>,
    /// Global symbols referenced, panic functions excluded
    pub symbols: BTreeSet<String>,
    /// Panic functions referenced
    pub panics: BTreeSet<String>,
}

impl Stats {
    /// Counts the instructions of `lines` on `arch`
    pub fn of(lines: &[Line], arch: Arch) -> Self {
        let mut stats = Self::default();
        for line in lines {
            if let Line::Instruction(instruction) = line {
                stats.add(instruction, arch);
            }
        }
        stats
    }

    /// Counts the instructions of `lines` whose index among instructions `keep` accepts
    pub fn of_selected(lines: &[Line], keep: impl Fn(usize) -> bool, arch: Arch) -> Self {
        let mut stats = Self::default();
        let instructions = lines.iter().filter_map(|line| match line {
            Line::Instruction(instruction) => Some(instruction),
            Line::Label(_) => None,
        });
        for (index, instruction) in instructions.enumerate() {
            if keep(index) {
                stats.add(instruction, arch);
            }
        }
        stats
    }

    /// Adds one instruction to the counts
    fn add(&mut self, instruction: &Instruction, arch: Arch) {
        let class = classify(instruction, arch);
        self.instructions += 1;
        self.cond_branches += usize::from(class.cond_branch);
        self.jumps += usize::from(class.jump);
        self.selects += usize::from(class.select);
        self.loads += usize::from(class.load);
        self.stores += usize::from(class.store);
        self.simd += usize::from(class.simd);
        self.barriers += usize::from(class.barrier);
        self.calls += usize::from(class.call);
        if class.barrier {
            let kind = self.barrier_kinds.entry(instruction.mnemonic.clone());
            *kind.or_default() += 1;
        }
        let calls_panic = instruction.symbols.iter().any(|symbol| is_panic(symbol));
        self.panic_calls += usize::from(class.call && calls_panic);
        for symbol in &instruction.symbols {
            let set = if is_panic(symbol) {
                &mut self.panics
            } else {
                &mut self.symbols
            };
            set.insert(symbol.clone());
        }
    }

    /// The table cells, in `COLUMNS` order
    pub fn cells(&self) -> [usize; 10] {
        [
            self.instructions,
            self.cond_branches,
            self.jumps,
            self.selects,
            self.loads,
            self.stores,
            self.simd,
            self.barriers,
            self.calls,
            self.panic_calls,
        ]
    }

    /// Barrier breakdown, symbols and panics, empty parts left out
    fn details(&self) -> Vec<String> {
        let mut details = Vec::new();
        if !self.barrier_kinds.is_empty() {
            let kinds: Vec<String> = self
                .barrier_kinds
                .iter()
                .map(|(mnemonic, count)| format!("{count} {mnemonic}"))
                .collect();
            details.push(format!("barriers {}", kinds.join(", ")));
        }
        if !self.symbols.is_empty() {
            let symbols: Vec<&str> = self.symbols.iter().map(String::as_str).collect();
            details.push(format!("symbols {}", symbols.join(", ")));
        }
        if !self.panics.is_empty() {
            let panics: Vec<&str> = self.panics.iter().map(String::as_str).collect();
            details.push(format!("panics {}", panics.join(", ")));
        }
        details
    }
}

/// Whether `symbol` is a function a failed check calls
fn is_panic(symbol: &str) -> bool {
    PANIC_PATHS.iter().any(|path| symbol.starts_with(path))
}

/// The table, then one line of details per probe that has any
pub fn table(rows: &[(&Probe<'_>, Stats)]) -> String {
    let name_width = rows
        .iter()
        .map(|(probe, _)| probe.name.len())
        .max()
        .unwrap_or(0);
    let name_width = name_width.max("probe".len());
    let header: Vec<String> = COLUMNS
        .iter()
        .map(|column| format!("{column:>8}"))
        .collect();
    let mut out = format!("{:<name_width$} {}\n", "probe", header.join(""));
    for (probe, stats) in rows {
        let cells: Vec<String> = stats
            .cells()
            .iter()
            .map(|cell| format!("{cell:>8}"))
            .collect();
        out += &format!("{:<name_width$} {}\n", probe.name, cells.join(""));
    }
    for (probe, stats) in rows {
        let mut details = stats.details();
        if let Some(other) = probe.merged_into {
            details.insert(0, format!("merged into {other}"));
        }
        if !details.is_empty() {
            out += &format!("{}: {}\n", probe.name, details.join(" | "));
        }
    }
    out
}

/// Cells of every probe row of a table written by [`table`], details lines skipped
pub fn parse_table(text: &str) -> BTreeMap<String, [usize; 10]> {
    let mut rows = BTreeMap::new();
    for line in text.lines().skip(1) {
        let mut words = line.split_whitespace();
        let Some(name) = words.next() else {
            continue;
        };
        let numbers: Option<Vec<usize>> = words.map(|word| word.parse().ok()).collect();
        let cells = numbers.and_then(|numbers| <[usize; 10]>::try_from(numbers).ok());
        if let Some(cells) = cells {
            rows.insert(name.to_string(), cells);
        }
    }
    rows
}

#[cfg(test)]
mod test {
    use super::*;

    /// Stats of a raw body for `triple`
    fn stats(body: &str, triple: &str) -> Stats {
        let target = Target::parse(triple).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        let normalized = normalize(&lines, &target);
        Stats::of(&normalized, target.arch)
    }

    #[test]
    fn counts_classes_and_splits_panics_from_other_symbols() {
        let body = "_probe:
\tldar\tx8, [x0]
\tcbz\tx8, LBB0_2
\tcsel\tx0, x8, x9, lo
\tbl\t_memcpy
\tret
LBB0_2:
\tbl\t__RNvNtCs3qDxio5cMcq_4core9panicking18panic_bounds_check";
        let stats = stats(body, "aarch64-apple-darwin");
        let counts = stats.cells();
        assert_eq!(counts, [6, 1, 0, 1, 1, 0, 0, 1, 2, 1]);
        let symbols: Vec<&str> = stats.symbols.iter().map(String::as_str).collect();
        let panics: Vec<&str> = stats.panics.iter().map(String::as_str).collect();
        assert_eq!(symbols, ["memcpy"]);
        assert_eq!(panics, ["core::panicking::panic_bounds_check"]);
        assert_eq!(stats.barrier_kinds.get("ldar"), Some(&1));
    }

    #[test]
    fn parse_table_reads_back_the_rendered_cells() {
        let target = Target::parse("aarch64-apple-darwin").unwrap();
        let text = "\t.globl\t_probe_a
_probe_a:
\tldar\tx8, [x0]
\tbl\t_memcpy
\t.cfi_endproc
\t.globl\t_probe_b
_probe_b = _probe_a";
        let listing = Listing::parse(text, &target);
        let probes = listing.probes().unwrap();
        let rows = rows(&probes, &target);
        let rendered = table(&rows);
        let parsed = parse_table(&rendered);
        let expected: BTreeMap<String, [usize; 10]> = rows
            .iter()
            .map(|(probe, stats)| (probe.name.to_string(), stats.cells()))
            .collect();
        assert_eq!(parsed, expected);
        assert!(
            rendered.contains("probe_b: merged into probe_a"),
            "{rendered}"
        );
    }

    #[test]
    fn panic_paths_cover_checks_but_not_user_code() {
        assert!(is_panic("core::panicking::panic_bounds_check"));
        assert!(is_panic("core::slice::index::slice_end_index_len_fail"));
        assert!(is_panic("core::option::unwrap_failed"));
        assert!(!is_panic("veloce::swmr::writer::Writer<u64>::publish"));
        assert!(!is_panic("memcpy"));
    }
}
