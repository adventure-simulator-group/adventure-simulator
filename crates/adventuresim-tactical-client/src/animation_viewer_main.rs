//! Deterministic native animation capture utility.

// This binary reuses the full gameplay presentation modules while installing
// only the deterministic capture path.
#![expect(
    dead_code,
    reason = "the deterministic viewer compiles the gameplay module graph but exercises only its capture path"
)]

mod animation;
mod animation_viewer;
mod camera;
mod player;
mod presentation;
mod targeting;

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Capture deterministic tactical locomotion review sequences"
)]
struct Args {
    /// Directory receiving per-view PNGs, manifest.json, and index.html.
    #[arg(long, default_value = "target/animation-captures/locomotion-review")]
    output: PathBuf,

    /// Repository asset directory containing animations/.
    #[arg(long, default_value = "assets")]
    asset_root: PathBuf,

    /// Rendered frames allowed for each deterministic sample to settle.
    #[arg(long, default_value_t = 1)]
    frames_per_sample: u32,

    /// Capture only one named scenario (for example `steady-walk-2.0`).
    #[arg(long)]
    scenario: Option<String>,

    /// Runtime tactical combat and animation tuning YAML.
    #[arg(long, default_value = "content/tactical/combat.yaml")]
    combat_config: PathBuf,
}

fn main() {
    let args = Args::parse();
    let combat_config_path = if args.combat_config.is_absolute() {
        args.combat_config.clone()
    } else {
        std::env::current_dir()
            .expect("animation viewer needs a working directory")
            .join(&args.combat_config)
    };
    let combat_config_text = std::fs::read_to_string(&combat_config_path).unwrap_or_else(|error| {
        panic!(
            "could not read animation tuning {}: {error}",
            combat_config_path.display()
        )
    });
    let combat_config: adventuresim_tactical_core::combat_config::TacticalCombatConfig =
        serde_saphyr::from_str(&combat_config_text).unwrap_or_else(|error| {
            panic!(
                "{} is not valid tactical combat YAML: {error}",
                combat_config_path.display()
            )
        });
    combat_config
        .install_runtime_snapshot()
        .unwrap_or_else(|error| panic!("{}: {error}", combat_config_path.display()));
    let asset_root = if args.asset_root.is_absolute() {
        args.asset_root
    } else {
        std::env::current_dir()
            .expect("animation viewer needs a working directory")
            .join(args.asset_root)
    };
    let exit = animation_viewer::run(
        args.output,
        asset_root,
        args.frames_per_sample.max(1),
        args.scenario.as_deref(),
        combat_config,
    );
    if let bevy::app::AppExit::Error(code) = exit {
        std::process::exit(code.get() as i32);
    }
}
