//! Focused native presentation/physics fixture for the Cascadeur humanoid.

#[cfg(not(target_family = "wasm"))]
mod animation;
#[cfg(not(target_family = "wasm"))]
mod animation_graph_nodes;
#[cfg(not(target_family = "wasm"))]
mod camera;
#[cfg(not(target_family = "wasm"))]
mod player;
#[cfg(not(target_family = "wasm"))]
mod presentation;
#[cfg(not(target_family = "wasm"))]
mod ragdoll_viewer;

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(not(target_family = "wasm"))]
use clap::Parser;

#[cfg(not(target_family = "wasm"))]
#[derive(Debug, Parser)]
#[command(version, about = "Review the Adventure Simulator humanoid ragdoll")]
struct Args {
    #[arg(long, default_value = "assets")]
    asset_root: PathBuf,
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    let args = Args::parse();
    let asset_root = if args.asset_root.is_absolute() {
        args.asset_root
    } else {
        std::env::current_dir()
            .expect("ragdoll viewer needs a working directory")
            .join(args.asset_root)
    };
    ragdoll_viewer::run(asset_root);
}

#[cfg(target_family = "wasm")]
fn main() {
    panic!("ragdoll-viewer is a native-only Avian physics fixture");
}
