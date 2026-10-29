//! Command line of the codegen tool: `cargo run -p codegen -- <command>`

mod asm;
mod listing;

use std::process::ExitCode;

/// Usage, shown on `--help` and on a malformed command line
const USAGE: &str = "usage: cargo run -p codegen -- asm [--target <triple>]...";

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
            let targets = parse_targets(rest)?;
            asm::run(&targets)
        }
        Some((flag, _)) if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Triples given by `--target` flags, in order
fn parse_targets(args: &[String]) -> Result<Vec<String>, String> {
    let mut targets = Vec::new();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        if flag != "--target" {
            return Err(format!("unexpected argument {flag:?}\n{USAGE}"));
        }
        let target = rest
            .next()
            .ok_or_else(|| format!("--target needs a triple\n{USAGE}"))?;
        targets.push(target.clone());
    }
    Ok(targets)
}
