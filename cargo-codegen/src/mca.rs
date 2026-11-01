//! `stats --mca`: llvm-mca's static cost estimate of each probe on a CPU model

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::listing::Probe;
use crate::target::Target;

/// Where Homebrew installs `llvm`, which it doesn't link onto `PATH`
const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew/opt/llvm/bin", "/usr/local/opt/llvm/bin"];

/// Column headers of the table, after the probe name
const COLUMNS: [&str; 6] = ["cycles", "rthru", "deps%", "press%", "unit", "critical"];

/// The table of every probe's estimate, sorted by name, on `mcpu`, else the host CPU for the host target
pub fn table(
    probes: &[Probe<'_>],
    target: &Target,
    mcpu: Option<&str>,
    host: bool,
) -> Result<String, String> {
    let llvm_mca = llvm_mca()?;
    let mcpu = mcpu.or(host.then_some("native"));
    let mut sorted: Vec<&Probe<'_>> = probes.iter().collect();
    sorted.sort_by_key(|probe| probe.name);
    let mut rows = Vec::new();
    for probe in sorted {
        let estimate = estimate(&llvm_mca, probe, target, mcpu)?;
        rows.push((probe.name, estimate));
    }
    Ok(render(&rows, mcpu))
}

/// llvm-mca's view of one probe body, run as a loop of 100 iterations
#[derive(Debug, PartialEq)]
struct Estimate {
    /// Simulated cycles per iteration, dependencies between iterations included
    cycles: f64,
    /// Block reciprocal throughput: cycles per iteration if only execution resources limited it
    rthroughput: f64,
    /// Share of cycles a register or memory dependency stalled dispatch
    dependency_share: f64,
    /// Share of cycles an execution unit's pressure stalled dispatch
    pressure_share: f64,
    /// The unit under the most pressure, if any
    unit: Option<String>,
    /// Distinct instructions on the critical dependency sequence
    critical: usize,
}

/// Runs llvm-mca on the body of `probe`, its directives stripped
fn estimate(
    llvm_mca: &PathBuf,
    probe: &Probe<'_>,
    target: &Target,
    mcpu: Option<&str>,
) -> Result<Estimate, String> {
    let source = instructions_only(&probe.body);
    let mut command = Command::new(llvm_mca);
    command.args(["-json", "-bottleneck-analysis", "-mtriple", &target.triple]);
    if let Some(mcpu) = mcpu {
        command.args(["-mcpu", mcpu]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let spawned = command.spawn();
    let mut child = spawned.map_err(|error| format!("running {}: {error}", llvm_mca.display()))?;
    let stdin = child.stdin.take();
    stdin
        .expect("stdin is piped")
        .write_all(source.as_bytes())
        .map_err(|error| format!("writing to llvm-mca: {error}"))?;
    let output = child.wait_with_output();
    let output = output.map_err(|error| format!("running llvm-mca: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("llvm-mca failed on {}: {stderr}", probe.name));
    }
    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parsing llvm-mca's output for {}: {error}", probe.name))?;
    parse(&json).ok_or_else(|| format!("llvm-mca printed no summary for {}", probe.name))
}

/// Labels and instructions of a body: llvm-mca rejects `.loc` without the file table, and skips the rest anyway
fn instructions_only(body: &[&str]) -> String {
    let kept = body
        .iter()
        .filter(|line| !line.trim_start().starts_with('.'));
    let lines: Vec<&str> = kept.copied().collect();
    lines.join("\n") + "\n"
}

/// The estimate in llvm-mca's JSON, its first and only code region
fn parse(json: &Value) -> Option<Estimate> {
    let region = &json["CodeRegions"][0];
    let summary = &region["SummaryView"];
    let bottleneck = &region["BottleneckAnalysis"];
    let iterations = summary["Iterations"].as_f64()?;
    let total = summary["TotalCycles"].as_f64()?;
    let share = |field: &str| bottleneck[field].as_f64().unwrap_or(0.0) / total * 100.0;
    let pressures = bottleneck["ResourcePressure"].as_array();
    let units = pressures
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .flatten();
    let busiest = units.max_by_key(|(_, cycles)| cycles.as_u64().unwrap_or(0));
    let edges = bottleneck["DependencyEdge"].as_array();
    let mut critical: Vec<u64> = edges
        .into_iter()
        .flatten()
        .flat_map(|edge| [edge["FromID"].as_u64(), edge["ToID"].as_u64()])
        .flatten()
        .collect();
    critical.sort_unstable();
    critical.dedup();
    Some(Estimate {
        cycles: total / iterations,
        rthroughput: summary["BlockRThroughput"].as_f64()?,
        dependency_share: share("DataDependencyCycles"),
        pressure_share: share("ResourcePressureCycles"),
        unit: busiest.map(|(unit, _)| unit.clone()),
        critical: critical.len(),
    })
}

/// `LLVM_MCA`, else `llvm-mca` on `PATH`, else Homebrew's
fn llvm_mca() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("LLVM_MCA") {
        return Ok(PathBuf::from(path));
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    dirs.extend(HOMEBREW_PREFIXES.iter().map(PathBuf::from));
    let found = dirs
        .into_iter()
        .map(|dir| dir.join("llvm-mca"))
        .find(|path| path.is_file());
    found.ok_or_else(|| {
        "llvm-mca not found: `brew install llvm`, or set LLVM_MCA to its path".to_string()
    })
}

/// The table, headed by the CPU model the estimates are for
fn render(rows: &[(&str, Estimate)], mcpu: Option<&str>) -> String {
    let model = mcpu.unwrap_or("default");
    let title = format!("llvm-mca -mcpu={model}");
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let width = width.max(title.len());
    let header: Vec<String> = COLUMNS
        .iter()
        .map(|column| format!("{column:>9}"))
        .collect();
    let mut out = format!("{title:<width$} {}\n", header.join(""));
    for (name, estimate) in rows {
        let unit = estimate.unit.as_deref().unwrap_or("-");
        out += &format!(
            "{name:<width$} {:>9.1}{:>9.1}{:>9.0}{:>9.0}{unit:>9}{:>9}\n",
            estimate.cycles,
            estimate.rthroughput,
            estimate.dependency_share,
            estimate.pressure_share,
            estimate.critical
        );
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn instructions_only_drops_directives_and_keeps_labels() {
        let body = [
            "_probe:",
            "\t.cfi_startproc",
            "\t.loc\t1 2 3",
            "\tcbz\tx1, LBB0_2",
            "LBB0_2:",
            "\tret",
        ];
        let source = instructions_only(&body);
        assert_eq!(source, "_probe:\n\tcbz\tx1, LBB0_2\nLBB0_2:\n\tret\n");
    }

    #[test]
    fn parse_reads_the_summary_and_the_bottleneck() {
        let json: Value = serde_json::from_str(
            r#"{"CodeRegions":[{"SummaryView":{"Iterations":100,"TotalCycles":2800,"BlockRThroughput":15.5},
            "BottleneckAnalysis":{"DataDependencyCycles":2100,"ResourcePressureCycles":700,
            "ResourcePressure":[{"CyUnitBR":2},{"CyUnitIS":785}],
            "DependencyEdge":[{"FromID":59,"ToID":72},{"FromID":72,"ToID":74}]}}]}"#,
        )
        .unwrap();
        let estimate = parse(&json).unwrap();
        let expected = Estimate {
            cycles: 28.0,
            rthroughput: 15.5,
            dependency_share: 75.0,
            pressure_share: 25.0,
            unit: Some("CyUnitIS".to_string()),
            critical: 3,
        };
        assert_eq!(estimate, expected);
    }

    #[test]
    fn parse_tolerates_a_missing_bottleneck() {
        let json: Value = serde_json::from_str(
            r#"{"CodeRegions":[{"SummaryView":{"Iterations":100,"TotalCycles":300,"BlockRThroughput":2.0}}]}"#,
        )
        .unwrap();
        let estimate = parse(&json).unwrap();
        assert_eq!(estimate.unit, None);
        assert_eq!(estimate.critical, 0);
        assert_eq!(estimate.dependency_share, 0.0);
    }
}
