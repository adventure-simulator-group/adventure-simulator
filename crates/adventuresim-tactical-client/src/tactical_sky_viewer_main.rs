//! Deterministic native capture harness for tactical sky presentation.

#![allow(dead_code)]

#[cfg(not(target_family = "wasm"))]
mod presentation;
#[cfg(not(target_family = "wasm"))]
mod tactical_sky_viewer;

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use clap::{Parser, ValueEnum};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, ValueEnum)]
enum SkyView {
    Sun,
    SunDetail,
    Twilight,
    Moon,
    Stars,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Parser)]
#[command(version, about = "Capture one deterministic tactical sky view")]
struct Args {
    /// Celestial feature and canonical time to render.
    #[arg(long, value_enum)]
    view: SkyView,

    /// PNG output path.
    #[arg(long)]
    output: PathBuf,

    /// Render frames allowed for atmosphere and custom pipelines to settle.
    #[arg(long, default_value_t = 24)]
    settle_frames: u32,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    tactical_sky_viewer::run(args.view, args.output, args.settle_frames);
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("tactical-sky-viewer is a native-only capture harness");
}
