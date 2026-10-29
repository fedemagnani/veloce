//! Command line of `cargo codegen <command> [options]`, run inside the workspace of the probed package
//!
//! A probe is any `#[unsafe(no_mangle)]` function of the generated host crate, found by symbol in its emitted assembly.

mod asm;
mod baseline;
mod emit;
mod host;
mod isa;
mod listing;
mod normalize;
mod registry;
mod stats;
mod target;

use std::process::ExitCode;

use emit::{Package, Request};

/// Usage, shown on `--help` and on a malformed command line
const USAGE: &str = "usage: cargo codegen <command> [build options] [--target <triple>]...
commands:
  asm [--raw]                write one listing per probe to target/codegen/<package>/<target>/, normalized unless --raw
  stats                      print per probe counts of branches, selects, memory accesses, barriers and calls
  save [--baseline <name>]   save fresh builds as a baseline in target/codegen/<package>/baselines/, `base` by default
  diff [--baseline <name>]   compare fresh builds to a baseline, exiting 1 when a probe was added, removed or changed
build options, each defaulting to [package.metadata.codegen] of the package:
  -p, --package <name>       package holding the probes, else the one containing the current directory
  --features <list>          features to build with, replacing the metadata's `features`
  --no-default-features      build without the package's default features
  --feature <name>           feature enabling the probes, else the metadata's `feature`, else `codegen`
save and diff default to the targets the baseline holds, save to the host when it holds none";

/// Options taking a value; the build options and `--target` are parsed apart
const VALUE_OPTIONS: [&str; 1] = ["--baseline"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `cargo codegen ..` runs `cargo-codegen codegen ..`, `cargo run -p cargo-codegen -- ..` passes no subcommand name
    let args = match args.split_first() {
        Some((subcommand, rest)) if subcommand == "codegen" => rest.to_vec(),
        _ => args,
    };
    let result = run(&args);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("cargo-codegen: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the command named by the first argument
fn run(args: &[String]) -> Result<(), String> {
    match args.split_first() {
        Some((command, rest)) if command == "asm" => {
            let options = Options::parse(rest, &["--raw"])?;
            let package = Package::resolve(&options.request)?;
            asm::run(&package, &options.targets, options.has("--raw"))
        }
        Some((command, rest)) if command == "stats" => {
            let options = Options::parse(rest, &[])?;
            let package = Package::resolve(&options.request)?;
            stats::run(&package, &options.targets)
        }
        Some((command, rest)) if command == "save" => {
            let options = Options::parse(rest, &["--baseline"])?;
            let package = Package::resolve(&options.request)?;
            let name = options.baseline();
            baseline::save(&package, &options.targets, name)
        }
        Some((command, rest)) if command == "diff" => {
            let options = Options::parse(rest, &["--baseline"])?;
            let package = Package::resolve(&options.request)?;
            let name = options.baseline();
            baseline::diff(&package, &options.targets, name)
        }
        Some((flag, _)) if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Build request, targets, flags and valued options following a command
struct Options {
    request: Request,
    targets: Vec<String>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Options {
    /// Parses the build options, `--target <triple>` pairs and the command's own options in `allowed`
    fn parse(args: &[String], allowed: &[&str]) -> Result<Self, String> {
        let mut options = Self {
            request: Request::default(),
            targets: Vec::new(),
            flags: Vec::new(),
            values: Vec::new(),
        };
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            let mut value = || {
                let value = rest.next();
                value
                    .cloned()
                    .ok_or_else(|| format!("{arg} needs a value\n{USAGE}"))
            };
            match arg.as_str() {
                "--target" => options.targets.push(value()?),
                "-p" | "--package" => options.request.package = Some(value()?),
                "--features" => options.add_features(&value()?),
                "--no-default-features" => options.request.no_default_features = true,
                "--feature" => options.request.feature = Some(value()?),
                option if allowed.contains(&option) && VALUE_OPTIONS.contains(&option) => {
                    options.values.push((arg.clone(), value()?));
                }
                flag if allowed.contains(&flag) => options.flags.push(arg.clone()),
                _ => return Err(format!("unexpected argument {arg:?}\n{USAGE}")),
            }
        }
        Ok(options)
    }

    /// Adds features separated by commas or spaces, as cargo accepts them
    fn add_features(&mut self, given: &str) {
        let features = self.request.features.get_or_insert_with(Vec::new);
        features.extend(list(given));
    }

    /// Whether `flag` was given
    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|given| given == flag)
    }

    /// The baseline named by the last `--baseline`, else the default one
    fn baseline(&self) -> &str {
        let given = self
            .values
            .iter()
            .rev()
            .find(|(name, _)| name == "--baseline");
        let name = given.map(|(_, value)| value.as_str());
        name.unwrap_or(baseline::DEFAULT_BASELINE)
    }
}

/// Items separated by commas or spaces, as cargo accepts feature lists
fn list(text: &str) -> Vec<String> {
    let items = text.split([',', ' ']).filter(|item| !item.is_empty());
    items.map(str::to_string).collect()
}
