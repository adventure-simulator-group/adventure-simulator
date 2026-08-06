//! Deterministic native animation capture utility.

mod animation;
mod animation_viewer;

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "Capture deterministic tactical animation phases")]
struct Args {
    /// Directory receiving PNG frames and manifest.json.
    #[arg(long, default_value = "target/animation-captures/walk")]
    output: PathBuf,

    /// Repository asset directory containing animations/.
    #[arg(long, default_value = "assets")]
    asset_root: PathBuf,

    /// Rendered frames allowed for each phase to settle before capture.
    #[arg(long, default_value_t = 6)]
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
    animation_viewer::run(args.output, asset_root, args.frames_per_sample.max(1));
}
