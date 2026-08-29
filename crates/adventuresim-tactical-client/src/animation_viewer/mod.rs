//! A deterministic gameplay-presentation fixture for tactical animation review.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use adventuresim_tactical_core::animation::dive_launch_root_rotation;
use adventuresim_tactical_core::physics::{
    AdventureSimulatorPhysicsPlugin, TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND,
    quickstep_force_curve, quickstep_motion_target, quickstep_peak_horizontal_force_newtons,
    quickstep_push_seconds, quickstep_target_displacement_metres, quickstep_tracking_force_newtons,
};
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::client::WeaponGuardInputState;
use bevy::{
    animation::AnimationTargetId,
    app::AppExit,
    asset::io::AssetSourceBuilder,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::PresentMode,
};
use serde::Serialize;

use crate::animation::jitter::{self, JitterBone, JitterFrame, JitterValidationSummary};
use crate::animation::pose_buffer::PoseBufferMetrics;
use crate::animation::{
    AnimationPlayback, AnimationRuntime, ArmIkState, AuthoredBindTransform, BoneRole, HumanoidBone,
    LegIkDiagnostics, LegIkState, LocomotionBodyResponseState, LocomotionHeightState,
    LocomotionPresentationEvent, LocomotionPresentationEventKind,
    MEASURED_ANKLE_SOLE_OFFSET_METRES, PresentedSkeleton, ProceduralAnimationClock,
    RaisedFootworkState, SOLE_CONTACT_TOLERANCE_METRES, TacticalAnimationPlugin, TerrainIkEnabled,
    capture_animation_target_id, capture_entity_id, locomotion_support_weights,
    secondary_physics::SecondaryPhysicsTelemetry,
    semantic_route::{SemanticRoutePath, SemanticRouteTrace},
};
use crate::{
    camera::{
        CameraMode, TacticalCameraPlugin, TacticalCameraSet, animation_capture_camera_offset,
    },
    player::{LocalCharacterId, PlayerPlugin},
    presentation::{TacticalGameplayCamera, TacticalPresentationPlugin},
};

mod capture;
mod report;
mod scenarios;

use capture::*;
use report::*;
use scenarios::*;

pub(crate) fn run(
    output: PathBuf,
    asset_root: PathBuf,
    settle_frames: u32,
    scenario: Option<&str>,
) -> AppExit {
    fs::create_dir_all(&output).unwrap_or_else(|error| {
        panic!("failed to create animation capture directory {output:?}: {error}")
    });
    invalidate_previous_report(&output);
    let initial_terrain_ik = scenario.is_some_and(|name| {
        scenario_metadata(name).kind == ScenarioKind::Terrain || name.contains("terrain")
    });
    let default_character_id = default_tactical_character_id();

    let workspace_asset_source =
        AssetSourceBuilder::platform_default(&asset_root.to_string_lossy(), None);
    App::new()
        .register_asset_source("workspace", workspace_asset_source)
        // The live debug client registers the same default through
        // `DebugPlugin`. The fixture does not install that input/network
        // plugin, so mirror its presentation default explicitly.
        .register_required_components_with::<Collider, _>(DebugRender::none)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Fabelgeist Animation Review Capture".into(),
                        resolution: (960, 720).into(),
                        present_mode: PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: false,
                    enable_presentation_simulation: true,
                }),
            EnhancedInputPlugin,
        ))
        .add_plugins((
            PlayerPlugin,
            TacticalAnimationPlugin,
            TacticalCameraPlugin,
            TacticalPresentationPlugin::default(),
        ))
        .insert_resource(LocalCharacterId(default_character_id))
        .insert_resource(CameraMode { third_person: true })
        .insert_resource(WeaponGuardInputState::default())
        .insert_resource(Time::<Fixed>::from_hz(SAMPLE_HZ as f64))
        // Individual scenarios select terrain conformity explicitly so the
        // viewer can retain FK-only controls after the live default changed.
        .insert_resource(TerrainIkEnabled(initial_terrain_ik))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .insert_resource(CaptureSequence::new(output, settle_frames, scenario))
        .add_systems(Startup, setup_viewer)
        .add_systems(PreUpdate, (drive_sequence, freeze_capture_look).chain())
        .add_systems(
            PostUpdate,
            position_capture_camera
                .after(TacticalCameraSet::Offset)
                .before(TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            (draw_flat_ground_grid, draw_skeleton_overlay)
                .chain()
                .after(TransformSystems::Propagate),
        )
        .add_systems(
            Last,
            (collect_locomotion_presentation_events, capture_frame).chain(),
        )
        .run()
}
