//! Deterministic native animation capture utility.

mod animation;
mod animation_viewer;

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
    let exit = animation_viewer::run(args.output, asset_root, args.frames_per_sample.max(1));
    if let bevy::app::AppExit::Error(code) = exit {
        std::process::exit(code.get() as i32);
    }
}
