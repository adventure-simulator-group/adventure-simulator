//! Deterministic native animation capture utility.

// This binary reuses the full gameplay presentation modules while installing
// only the deterministic capture path.
#![allow(dead_code)]

mod animation;
mod animation_viewer;
mod camera;
mod player;
mod presentation;

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
}

fn main() {
    let args = Args::parse();
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
    );
    if let bevy::app::AppExit::Error(code) = exit {
        std::process::exit(code.get() as i32);
    }
}
