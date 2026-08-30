mod analyze;
mod config;
mod manifests;
mod scan;

use std::{env, path::PathBuf, process::ExitCode};

use analyze::{check_repository, print_baseline, print_census};
use config::{Baseline, Config};

fn run() -> Result<bool, String> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "check".into());
    let root = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or(env::current_dir().map_err(|error| error.to_string())?);
    if arguments.next().is_some() {
        return Err(
            "usage: fabelgeist-rust-quality [check|census|baseline] [repository-root]".into(),
        );
    }

    let config = Config::load(&root.join("rust-quality.toml"))?;
    let baseline_path = root.join("rust-quality-baseline.toml");
    let baseline = Baseline::load_optional(&baseline_path)?;
    let report = check_repository(&root, &config, &baseline)?;

    match command.as_str() {
        "check" => {
            print_census(&report.census, config.census_summary_limit);
            for diagnostic in &report.diagnostics {
                eprintln!("{diagnostic}");
            }
            Ok(report.diagnostics.is_empty())
        }
        "census" => {
            print_census(&report.census, usize::MAX);
            Ok(true)
        }
        "baseline" => {
            print_baseline(&report.snapshot)?;
            Ok(true)
        }
        _ => Err(format!("unknown command `{command}`")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("rust quality checker: {error}");
            ExitCode::FAILURE
        }
    }
}
