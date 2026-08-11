//! Deterministic native capture harness for generated tactical environments.

#![allow(dead_code)]

#[cfg(not(target_family = "wasm"))]
mod presentation;
#[cfg(not(target_family = "wasm"))]
mod tactical_scene_viewer;

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use clap::Parser;

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Parser)]
#[command(version, about = "Capture and validate a tactical environment fixture")]
struct Args {
    /// Fixture name from assets/tactical-scenes, without the .json suffix.
    #[arg(long, conflicts_with = "scene_input")]
    fixture: Option<String>,

    /// Explicit TacticalSceneInput JSON path.
    #[arg(long, conflicts_with = "fixture")]
    scene_input: Option<PathBuf>,

    /// Fresh output directory. A timestamped directory is chosen when omitted.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Render frames allowed to settle between fixed camera views.
    #[arg(long, default_value_t = 12)]
    settle_frames: u32,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    if args.fixture.is_none() && args.scene_input.is_none() {
        clap::Error::raw(
            clap::error::ErrorKind::MissingRequiredArgument,
            "one of --fixture or --scene-input is required",
        )
        .exit();
    }
    tactical_scene_viewer::run(
        args.fixture,
        args.scene_input,
        args.output,
        args.settle_frames,
    );
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("tactical-scene-viewer is a native-only capture harness");
}
