//! Command line of the codegen tool: `cargo run -p codegen -- <command>`

mod asm;
mod baseline;
mod emit;
mod isa;
mod listing;
mod normalize;
mod stats;
mod target;

use std::process::ExitCode;

/// Usage, shown on `--help` and on a malformed command line
const USAGE: &str = "usage: cargo run -p codegen -- <command> [--target <triple>]...
  asm [--raw]   write one listing per probe to target/codegen/<target>/, normalized unless --raw
  stats         print per probe counts of branches, selects, memory accesses, barriers and calls
  save [--baseline <name>]   save fresh builds as a baseline in target/codegen/baselines/, `base` by default
  diff [--baseline <name>]   compare fresh builds to a baseline, exiting 1 when a probe was added, removed or changed
save and diff default to the targets the baseline holds, save to the host when it holds none";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = run(&args);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("codegen: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the command named by the first argument
fn run(args: &[String]) -> Result<(), String> {
    match args.split_first() {
        Some((command, rest)) if command == "asm" => {
            let options = Options::parse(rest, &["--raw"])?;
            asm::run(&options.targets, options.has("--raw"))
        }
        Some((command, rest)) if command == "stats" => {
            let options = Options::parse(rest, &[])?;
            stats::run(&options.targets)
        }
        Some((command, rest)) if command == "save" => {
            let options = Options::parse(rest, &["--baseline"])?;
            let name = options
                .value("--baseline")
                .unwrap_or(baseline::DEFAULT_BASELINE);
            baseline::save(&options.targets, name)
        }
        Some((command, rest)) if command == "diff" => {
            let options = Options::parse(rest, &["--baseline"])?;
            let name = options
                .value("--baseline")
                .unwrap_or(baseline::DEFAULT_BASELINE);
            baseline::diff(&options.targets, name)
        }
        Some((flag, _)) if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Options taking a value, as `--baseline <name>`
const VALUE_OPTIONS: [&str; 1] = ["--baseline"];

/// Targets, flags and valued options following a command
struct Options {
    targets: Vec<String>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Options {
    /// Parses `--target <triple>` pairs and the flags and valued options in `allowed`, rejecting anything else
    fn parse(args: &[String], allowed: &[&str]) -> Result<Self, String> {
        let mut targets = Vec::new();
        let mut flags = Vec::new();
        let mut values = Vec::new();
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            if arg == "--target" {
                let target = rest
                    .next()
                    .ok_or_else(|| format!("--target needs a triple\n{USAGE}"))?;
                targets.push(target.clone());
            } else if allowed.contains(&arg.as_str()) && VALUE_OPTIONS.contains(&arg.as_str()) {
                let value = rest
                    .next()
                    .ok_or_else(|| format!("{arg} needs a value\n{USAGE}"))?;
                values.push((arg.clone(), value.clone()));
            } else if allowed.contains(&arg.as_str()) {
                flags.push(arg.clone());
            } else {
                return Err(format!("unexpected argument {arg:?}\n{USAGE}"));
            }
        }
        Ok(Self {
            targets,
            flags,
            values,
        })
    }

    /// Whether `flag` was given
    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|given| given == flag)
    }

    /// Value of the last `option` given
    fn value(&self, option: &str) -> Option<&str> {
        let given = self.values.iter().rev().find(|(name, _)| name == option);
        given.map(|(_, value)| value.as_str())
    }
}
