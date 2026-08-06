//! Native, opt-in Adventure Simulator animation-graph authoring tool.

// This bin reuses the tactical animation/catalog modules solely for project
// validation; their runtime systems are intentionally not installed here.
#![allow(dead_code)]

#[cfg(not(target_family = "wasm"))]
mod native {
    use std::path::PathBuf;

    use bevy::prelude::*;
    use bevy_animation_graph_editor::AnimationGraphEditorPlugin;

    use crate::{
        animation::catalog::validate_editor_asset_root,
        animation::semantic_graph::{SemanticGraphLibrary, editor_graph_preflight},
        animation_graph_nodes::SparseSemanticBlendNode,
    };

    pub(super) fn run() {
        let asset_root = asset_source_argument().unwrap_or_else(|| {
            eprintln!(
                "error: Adventure Simulator's graph editor requires --asset-source <assets-dir>"
            );
            std::process::exit(2);
        });
        let report = validate_editor_asset_root(&asset_root).unwrap_or_else(|errors| {
            eprintln!("Adventure Simulator animation graph validation failed:");
            for error in errors {
                eprintln!("  - {error}");
            }
            std::process::exit(2);
        });

        eprintln!(
            "Validated {} semantic pack(s), {} motion source(s), and {} deterministic route endpoint(s); {} optional motion file(s) are absent.",
            report.pack_count,
            report.motion_count,
            report.route_resolutions.len(),
            report.missing_motion_count
        );
        for warning in &report.warnings {
            eprintln!("  warning: {warning}");
        }
        for resolution in report.route_resolutions {
            let fallback = if resolution.mirrored {
                "same-pack mirrored fallback"
            } else {
                "exact"
            };
            eprintln!(
                "  {}: {} -> {}:{} frame {} ({fallback}) [{}]",
                resolution.route,
                resolution.requested.as_str(),
                resolution.resolved_pack,
                resolution.resolved_pose.as_str(),
                resolution.frame,
                resolution.motion_path.display()
            );
        }

        let mut graph_preflight = App::new();
        graph_preflight
            .add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins(bevy_animation_graph::AnimationGraphPlugin::default())
            .register_type::<SparseSemanticBlendNode>()
            .init_resource::<SemanticGraphLibrary>();
        let routes = graph_preflight
            .world_mut()
            .run_system_cached(editor_graph_preflight)
            .unwrap_or_else(|error| {
                eprintln!("Adventure Simulator graph preflight system failed: {error}");
                std::process::exit(2);
            })
            .unwrap_or_else(|errors| {
                eprintln!("Adventure Simulator animation graph validation failed:");
                for error in errors {
                    eprintln!("  - {error}");
                }
                std::process::exit(2);
            });
        for route in routes {
            eprintln!(
                "  queried {}: {} -> {} ({} semantic sample(s))",
                route.label,
                route.requested_path.as_str(),
                route.selected_path.as_str(),
                route.sample_count
            );
        }

        let mut app = App::new();
        app.add_plugins(AnimationGraphEditorPlugin)
            .register_type::<SparseSemanticBlendNode>();
        app.run();
    }

    fn asset_source_argument() -> Option<PathBuf> {
        let args = std::env::args_os().skip(1).collect::<Vec<_>>();
        args.windows(2).find_map(|pair| {
            matches!(pair[0].to_str(), Some("--asset-source" | "-a"))
                .then(|| PathBuf::from(&pair[1]))
        })
    }
}

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
fn main() {
    native::run();
}

#[cfg(target_family = "wasm")]
fn main() {}
