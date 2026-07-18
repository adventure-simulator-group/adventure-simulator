use adventuresim_strategic_sim::*;
use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf};

#[derive(Parser)]
#[command(about = "Deterministic strategic NPC balance simulator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate profiles and run canonical settlement downtime.
    Run {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 100)]
        population: u32,
        #[arg(long, default_value_t = 1095)]
        days: u32,
    },
    /// Rerun a report's recorded manifest and verify its digest.
    Replay {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run a matched labor/thievery pair with common initial circumstances.
    Matched {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 365)]
        days: u32,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let report = match cli.command {
        Command::Run {
            config,
            output,
            seed,
            population,
            days,
        } => {
            let config = if let Some(path) = config {
                serde_json::from_slice(&fs::read(path)?)?
            } else {
                SimulationConfig {
                    seed,
                    population,
                    days,
                    max_decisions: u64::from(population)
                        .checked_mul(u64::from(days))
                        .ok_or("decision count overflow")?,
                    ..SimulationConfig::default()
                }
            };
            emit(run(config)?, output)?
        }
        Command::Replay { report, output } => {
            let recorded: SimulationReport = serde_json::from_slice(&fs::read(report)?)?;
            if digest(&recorded)? != recorded.canonical_digest {
                return Err("recorded report digest is invalid".into());
            }
            let rerun = replay(recorded.manifest.clone())?;
            if rerun.canonical_digest != recorded.canonical_digest {
                return Err("replay digest differs".into());
            }
            emit(rerun, output)?
        }
        Command::Matched { seed, days, output } => {
            let (a, b) = matched_activity_pair(
                seed,
                0,
                ActivityPreference::Labor,
                ActivityPreference::Thievery,
            );
            let mut b = b;
            b.agent_id = 1;
            let config = SimulationConfig {
                seed,
                population: 2,
                days,
                max_decisions: 2_u64
                    .checked_mul(u64::from(days))
                    .ok_or("decision count overflow")?,
                ..SimulationConfig::default()
            };
            emit(run_profiles(config, vec![a, b])?, output)?
        }
    };
    eprintln!("{}", human_summary(&report));
    Ok(())
}

fn emit(
    report: SimulationReport,
    output: Option<PathBuf>,
) -> Result<SimulationReport, Box<dyn std::error::Error>> {
    let json = serde_json::to_vec_pretty(&report)?;
    if let Some(path) = output {
        fs::write(path, json)?;
    } else {
        println!("{}", String::from_utf8(json)?);
    }
    Ok(report)
}
