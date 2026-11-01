//! cargo-codegen run on `tests/fixture`, whose probes have known panics and a watched function

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn stats_count_the_panic_a_surviving_bounds_check_calls() {
    let output = run("stats_panics", &["stats"]);
    let checked = row(&output, "fixture_checked");
    let unchecked = row(&output, "fixture_unchecked");
    let checked_panics = checked.last().copied();
    let unchecked_panics = unchecked.last().copied();
    assert_eq!(checked_panics, Some(1), "{output}");
    assert_eq!(unchecked_panics, Some(0), "{output}");
}

#[test]
fn stats_attribute_instructions_to_the_watched_function() {
    let output = run("stats_watch", &["stats"]);
    let attributed = output
        .lines()
        .any(|line| line.starts_with("fixture_watched_rotated > watched_sum"));
    assert!(attributed, "{output}");
}

// `fixture_watched_sum` and `fixture_plain_sum` are identical, and LLVM keeps the DWARF of either
#[test]
fn stats_report_a_merged_probe_in_the_watched_table() {
    let output = run("stats_merged", &["stats"]);
    let merged = output.lines().find_map(|line| {
        let (probe, details) = line.split_once(": ")?;
        let merged = probe.ends_with("_sum") && details.starts_with("merged into");
        merged.then_some(probe)
    });
    let merged =
        merged.unwrap_or_else(|| panic!("the identical sum probes weren't merged\n{output}"));
    let shared_rows = format!("{merged} (merged into ");
    let reported = output.lines().any(|line| {
        let unavailable = line.split_whitespace().next() == Some(merged)
            && line.contains("attribution unavailable: merged into");
        line.starts_with(&shared_rows) || unavailable
    });
    assert!(reported, "{output}");
}

// identical bodies may also be merged into one symbol, which the listings then share
#[test]
fn watching_a_function_leaves_its_code_unchanged() {
    let target_dir = target_dir("asm_watch");
    run_in(&target_dir, &["asm"]);
    let listings = target_dir
        .join("codegen/codegen-fixture")
        .join(host_triple());
    let watched = body(&listings.join("fixture_watched_sum.s"));
    let plain = body(&listings.join("fixture_plain_sum.s"));
    assert!(!watched.is_empty());
    assert_eq!(watched, plain);
}

/// Runs cargo-codegen on the fixture with its own target dir, returning standard output
fn run(name: &str, args: &[&str]) -> String {
    let target_dir = target_dir(name);
    run_in(&target_dir, args)
}

/// Runs cargo-codegen on the fixture with `target_dir`, failing the test on a non-zero exit
fn run_in(target_dir: &Path, args: &[&str]) -> String {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-codegen"))
        .args(args)
        .current_dir(&fixture)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .expect("running cargo-codegen");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    stdout
}

/// A target dir per test, so concurrent tests never rewrite each other's host crate
fn target_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

/// The numbers of the table row named `name`
fn row(output: &str, name: &str) -> Vec<u64> {
    let line = output
        .lines()
        .find(|line| line.split_whitespace().next() == Some(name));
    let line = line.unwrap_or_else(|| panic!("no row {name} in\n{output}"));
    let numbers = line.split_whitespace().skip(1);
    numbers.map_while(|word| word.parse().ok()).collect()
}

/// A listing without its first line, the label naming the probe
fn body(path: &Path) -> Vec<String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines().skip(1).map(str::to_string).collect()
}

/// The host triple, the target cargo-codegen builds for when given none
fn host_triple() -> String {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("running rustc");
    let text = String::from_utf8_lossy(&output.stdout);
    let host = text.lines().find_map(|line| line.strip_prefix("host: "));
    host.expect("rustc -vV printed no host").to_string()
}
