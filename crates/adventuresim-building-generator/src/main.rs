//! Deterministic native viewer and screenshot harness for building prototypes.

#[cfg(not(target_family = "wasm"))]
mod viewer;

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use adventuresim_building_generator::BuildingArchetype;
#[cfg(not(target_family = "wasm"))]
use clap::{Parser, ValueEnum};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ViewerView {
    Exterior,
    Defenses,
    Cutaway,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Render a deterministic procedural-building prototype"
)]
struct Args {
    /// Curated high-level building program to generate.
    #[arg(long, value_enum)]
    fixture: BuildingArchetype,

    /// Exterior massing, elevated rear defenses, or a cutaway exposing rooms and stairs.
    #[arg(long, value_enum, default_value_t = ViewerView::Exterior)]
    view: ViewerView,

    /// Deterministic generation seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// PNG output path. Omit to leave the interactive viewer open.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Frames allowed for render pipelines to settle before capture.
    #[arg(long, default_value_t = 240)]
    settle_frames: u32,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    viewer::run(
        args.fixture,
        args.view,
        args.seed,
        args.output,
        args.settle_frames,
    );
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("building-viewer is a native-only prototype");
}
