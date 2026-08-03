//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

use adventuresim_tactical_core::prelude::*;
use bevy::{
    camera::Exposure,
    core_pipeline::tonemapping::Tonemapping,
    light::{AtmosphereEnvironmentMapLight, light_consts::lux},
    pbr::{Atmosphere, ScatteringMedium, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
};

pub struct TacticalPresentationPlugin;

impl Plugin for TacticalPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tactical_presentation)
            .add_observer(on_game_scene_added);
    }
}

fn setup_tactical_presentation(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
) {
    commands.spawn((
        Name::new("Tactical sunlight"),
        Transform::from_xyz(200.0, 1000.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: lux::DIRECT_SUNLIGHT,
            ..default()
        },
    ));

    commands.spawn((
        Name::new("Tactical gameplay camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 80.0_f32.to_radians(),
            ..default()
        }),
        Atmosphere::earthlike(scattering_mediums.add(ScatteringMedium::default())),
        AtmosphereEnvironmentMapLight::default(),
        Exposure::SUNLIGHT,
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
        Msaa::Off,
        ScreenSpaceAmbientOcclusion::default(),
    ));
}

fn on_game_scene_added(
    event: On<Add, SceneId>,
    mut commands: Commands,
    query: Query<(&SceneId, &SceneTerrain)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let (id, terrain) = query.get(event.entity)?;
    info!(entity = ?event.entity, "Spawning a scene {id:?}");

    let floor_color = match id.0.as_str() {
        "hills" => Color::srgb_u8(96, 108, 56),
        "desert" => Color::srgb_u8(221, 161, 94),
        id => {
            warn!("Unknown scene: {id}");
            Color::BLACK
        }
    };

    commands.spawn((
        Name::new(format!("{} terrain mesh", id.0)),
        Mesh3d(meshes.add(terrain.mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: floor_color,
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        })),
    ));
    Ok(())
}
