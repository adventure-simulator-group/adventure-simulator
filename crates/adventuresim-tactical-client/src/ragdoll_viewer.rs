use std::path::PathBuf;

use adventuresim_tactical_core::{physics::AdventureSimulatorPhysicsPlugin, prelude::*};
use adventuresim_tactical_netcode::client::WeaponGuardInputState;
use bevy::{asset::io::AssetSourceBuilder, prelude::*, window::PresentMode};

use crate::{
    animation::{
        TacticalAnimationPlugin,
        ragdoll::{
            HumanoidRagdollPlugin, RAGDOLL_LAYER, RagdollMode, RagdollPresentationFocus,
            RagdollReset, TERRAIN_LAYER,
        },
    },
    camera::{CameraMode, TacticalCameraPlugin, TacticalCameraSet},
    player::{LocalCharacterId, PlayerPlugin},
    presentation::TacticalPresentationPlugin,
};

#[derive(Component)]
struct RagdollViewerSubject;

#[derive(Component)]
struct RagdollViewerLabel;

pub(crate) fn run(asset_root: PathBuf) {
    let source = AssetSourceBuilder::platform_default(&asset_root.to_string_lossy(), None);
    App::new()
        .register_asset_source("workspace", source)
        .register_required_components_with::<Collider, _>(DebugRender::none)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Adventure Simulator Ragdoll Viewer".into(),
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
                    enable_simulation: true,
                }),
            EnhancedInputPlugin,
        ))
        .add_plugins((
            PlayerPlugin,
            TacticalAnimationPlugin,
            HumanoidRagdollPlugin,
            TacticalCameraPlugin,
            TacticalPresentationPlugin::default(),
        ))
        .insert_resource(LocalCharacterId(0))
        .insert_resource(CameraMode { third_person: true })
        .insert_resource(WeaponGuardInputState::default())
        .insert_resource(ClearColor(Color::srgb(0.08, 0.1, 0.13)))
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_controls, update_label))
        .add_systems(
            PostUpdate,
            position_viewer_camera
                .after(TacticalCameraSet::Offset)
                .before(TransformSystems::Propagate),
        )
        .run();
}

fn viewer_camera_transform(focus: Vec3) -> Transform {
    let target = focus + Vec3::new(0.0, -0.15, 0.0);
    Transform::from_translation(focus + Vec3::new(2.0, 1.0, 3.2)).looking_at(target, Vec3::Y)
}

fn position_viewer_camera(
    subject: Single<(&Transform, Option<&RagdollPresentationFocus>), With<RagdollViewerSubject>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<RagdollViewerSubject>)>,
) {
    let focus = subject.1.map_or(subject.0.translation, |focus| focus.0);
    let framed = viewer_camera_transform(focus);
    for mut camera in &mut cameras {
        *camera = framed;
    }
}

fn setup(mut commands: Commands) {
    // Keep the focused physics fixture level so capture measures ragdoll
    // settling rather than downhill travel on the gameplay hills profile.
    let terrain = TerrainGenerator::new(0xA11C_E5E1).generate(80, 0, 80);
    let height = terrain.height_at(Vec2::ZERO).unwrap_or_default() + 0.95;
    let terrain_collider = terrain.collider();
    commands.spawn((
        Name::new("Ragdoll viewer terrain"),
        SceneId("hills".to_owned()),
        terrain,
        RigidBody::Static,
        terrain_collider,
        terrain_collision_layers(),
        Transform::default(),
    ));
    commands.spawn((
        Name::new("Ragdoll viewer subject"),
        RagdollViewerSubject,
        Player {
            name: "Ragdoll review".into(),
        },
        CharacterId(0),
        CharacterLook::default(),
        SkeletonState::default(),
        Transform::from_xyz(0.0, height, 0.0),
    ));
    commands.spawn((
        RagdollViewerLabel,
        Text::new("Loading Cascadeur humanoid..."),
        TextFont::from_font_size(22.0),
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
}

fn terrain_collision_layers() -> CollisionLayers {
    CollisionLayers::new(TERRAIN_LAYER, RAGDOLL_LAYER)
}

fn keyboard_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<RagdollMode>,
    mut reset: ResMut<RagdollReset>,
) {
    if keyboard.just_pressed(KeyCode::KeyT) {
        *mode = match *mode {
            RagdollMode::Animated => RagdollMode::Passive,
            RagdollMode::Passive => RagdollMode::Animated,
        };
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        reset.0 = true;
        *mode = RagdollMode::Animated;
    }
}

fn update_label(mode: Res<RagdollMode>, mut labels: Query<&mut Text, With<RagdollViewerLabel>>) {
    if !mode.is_changed() {
        return;
    }
    for mut label in &mut labels {
        label.0 = format!("Mode: {}\nT: animated/passive | R: reset", mode.label());
    }
}

#[cfg(test)]
mod tests {
    use super::{terrain_collision_layers, viewer_camera_transform};
    use crate::animation::ragdoll::{RAGDOLL_LAYER, TERRAIN_LAYER};
    use avian3d::prelude::CollisionLayers;
    use bevy::prelude::*;

    #[test]
    fn fixture_terrain_and_ragdoll_layers_interact_bidirectionally() {
        let terrain = terrain_collision_layers();
        let ragdoll = CollisionLayers::new(RAGDOLL_LAYER, TERRAIN_LAYER);

        assert!(terrain.interacts_with(ragdoll));
        assert!(!terrain.interacts_with(terrain));
        assert!(!ragdoll.interacts_with(ragdoll));
    }

    #[test]
    fn deterministic_camera_faces_subject_from_stable_three_quarter_offset() {
        let subject = Vec3::new(1.0, 2.0, -3.0);
        let camera = viewer_camera_transform(subject);
        let target = subject + Vec3::new(0.0, -0.15, 0.0);
        let expected = (target - camera.translation).normalize();
        assert!(camera.forward().as_vec3().dot(expected) > 0.9999);
        let distance = (camera.translation - subject).length();
        assert!(distance > 3.5 && distance < 4.0);
    }
}
