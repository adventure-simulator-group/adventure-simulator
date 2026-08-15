//! Deterministic native capture harness for generated tactical environments.

#![allow(dead_code)]

#[cfg(not(target_family = "wasm"))]
mod presentation;
#[cfg(not(target_family = "wasm"))]
mod tactical_scene_viewer;

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use clap::{Parser, ValueEnum};

#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum CaptureProfile {
    /// Existing exhaustive semantic presentation suite (23 recorded views).
    #[default]
    Semantic,
    /// Compact environment-art review suite; intended for matrix review.
    EnvironmentReview,
}

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

    /// Test-only world canopy override in basis points (0..=10000).
    #[arg(long, value_parser = clap::value_parser!(u16).range(0..=10_000))]
    canopy_bps: Option<u16>,

    /// Test-only celestial time override, in absolute world minutes.
    #[arg(long)]
    absolute_minute: Option<u64>,

    /// Benchmark each leaf representation for this many frames in dense woodland.
    #[arg(long, value_parser = clap::value_parser!(u32).range(30..))]
    leaf_benchmark_frames: Option<u32>,

    /// Benchmark baseline, canopy AO, shadows, and combined tree lighting.
    #[arg(long, value_parser = clap::value_parser!(u32).range(30..), conflicts_with = "leaf_benchmark_frames")]
    tree_lighting_benchmark_frames: Option<u32>,

    /// Benchmark natural and isolated tree/LOD costs for this many frames per mode.
    #[arg(long, value_parser = clap::value_parser!(u32).range(30..), conflicts_with_all = ["leaf_benchmark_frames", "tree_lighting_benchmark_frames"])]
    scene_performance_benchmark_frames: Option<u32>,

    /// Collect GPU timestamps and render diagnostics during the scene benchmark.
    #[arg(long, requires = "scene_performance_benchmark_frames")]
    scene_performance_render_diagnostics: bool,

    /// Azimuth around the review tree for locked leaf-LOD comparison views.
    #[arg(long, default_value_t = 45.0)]
    tree_review_azimuth_degrees: f32,

    /// Named capture profile. The default preserves the exhaustive semantic suite.
    #[arg(long, value_enum, default_value_t)]
    profile: CaptureProfile,

    /// Capture only these named views (repeatable). Unknown or unavailable views fail closed.
    #[arg(long = "view")]
    views: Vec<String>,
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
        args.canopy_bps,
        args.absolute_minute,
        args.leaf_benchmark_frames,
        args.tree_lighting_benchmark_frames,
        args.scene_performance_benchmark_frames,
        args.scene_performance_render_diagnostics,
        args.tree_review_azimuth_degrees,
        match args.profile {
            CaptureProfile::Semantic => "semantic",
            CaptureProfile::EnvironmentReview => "environment-review",
        },
        args.views,
    );
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("tactical-scene-viewer is a native-only capture harness");
}
