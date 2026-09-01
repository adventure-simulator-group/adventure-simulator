use std::{fs, path::PathBuf};

use adventuresim_building_generator::{
    BattlementKind, BattlementRun, BuildingArchetype, BuildingDocument, BuildingEdit, BuildingPlan,
    CELL_SIZE_METRES, Cell, CrownPath, CurtainWallRun, Direction, DormerKind, FiringPosition,
    GableProfile, GateClosure, GateClosureKind, GateDefense, GatehouseLoadPath, GuardOpeningKind,
    InteriorWallFinish, Opening, OpeningKind, PlayerBuildDocument, PlayerBuildEdit,
    PlayerBuildMaterial, PlayerBuildPart, ProjectedDefenseDeployment, ProjectedDefensePath,
    ProjectedDefenseTarget, RidgeAxis, RoofAssembly, RoofDormer, RoofEnclosureFace, RoofFace,
    RoofKind, RoofMaterial, RoofPiece, RoundTower, SolidRole, SquareTower, Stair, TimberFrameStyle,
    TowerPortal, TowerPortalKind, WALL_THICKNESS_METRES, WallSegment, WallSelector, WallSourceId,
    WallStyle, WallWalk, analyse_player_build, audit_plan, audit_triangle_mesh, edit_document,
    generate, generate_document,
};
use bevy::{
    app::AppExit,
    asset::RenderAssetUsages,
    ecs::system::RunSystemOnce,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{PresentMode, WindowResolution},
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use bevy_mod_outline::{OutlineMode, OutlinePlugin, OutlineVolume};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use serde::{Deserialize, Serialize};

use crate::{ProjectedProofKind, RoofProofView, ViewerView};

#[cfg(test)]
use adventuresim_building_generator::BuildingProgram;
#[cfg(test)]
use adventuresim_building_generator::PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION;

include!("viewer/artillery_focus.rs");
include!("viewer/timber_focus.rs");
include!("viewer/church_focus.rs");
include!("viewer/roof_focus.rs");
include!("viewer/render_types.rs");
include!("viewer/editor_types.rs");
include!("viewer/editor_commands.rs");
include!("viewer/editor_input.rs");
include!("viewer/editor_ui.rs");
include!("viewer/editor_updates.rs");
include!("viewer/player_build_scene.rs");
include!("viewer/evidence_hashes.rs");
include!("viewer/source_files.rs");
include!("viewer/crown_suite.rs");
include!("viewer/projected_suite.rs");
include!("viewer/openings_suite.rs");
include!("viewer/roof_suite.rs");
include!("viewer/church_suite.rs");
include!("viewer/timber_suite.rs");
include!("viewer/focus_helpers.rs");
include!("viewer/run.rs");
include!("viewer/setup.rs");
include!("viewer/render_setup.rs");
include!("viewer/wall_rendering.rs");
include!("viewer/roof_meshes.rs");
include!("viewer/roof_rendering.rs");
include!("viewer/fortification_rendering.rs");
include!("viewer/tower_rendering.rs");
include!("viewer/walk_rendering.rs");
include!("viewer/architectural_rendering.rs");
include!("viewer/crown_rendering.rs");
include!("viewer/proof_markers.rs");
include!("viewer/battlement_rendering.rs");
include!("viewer/capture.rs");
include!("viewer/capture_metrics.rs");
include!("viewer/tests.rs");
