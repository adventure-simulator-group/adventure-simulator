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

#[derive(Debug, Clone, Copy)]
pub struct TacticalPresentationPlugin {
    pub shadows_enabled: bool,
    pub atmosphere_enabled: bool,
    pub environment_light_enabled: bool,
    pub environment_map_size: u32,
    pub bloom_enabled: bool,
    pub ssao_enabled: bool,
}

impl Default for TacticalPresentationPlugin {
    fn default() -> Self {
        Self {
            shadows_enabled: true,
            atmosphere_enabled: true,
            environment_light_enabled: true,
            environment_map_size: 64,
            bloom_enabled: true,
            ssao_enabled: true,
        }
    }
}

impl Plugin for TacticalPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TacticalGraphicsSettings {
            shadows_enabled: self.shadows_enabled,
            atmosphere_enabled: self.atmosphere_enabled,
            environment_light_enabled: self.environment_light_enabled,
            environment_map_size: self.environment_map_size,
            bloom_enabled: self.bloom_enabled,
            ssao_enabled: self.ssao_enabled,
        })
        .add_systems(Startup, setup_tactical_presentation)
        .add_observer(on_game_scene_added);
    }
}

#[derive(Resource, Debug, Clone, Copy)]
struct TacticalGraphicsSettings {
    shadows_enabled: bool,
    atmosphere_enabled: bool,
    environment_light_enabled: bool,
    environment_map_size: u32,
    bloom_enabled: bool,
    ssao_enabled: bool,
}

fn setup_tactical_presentation(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    settings: Res<TacticalGraphicsSettings>,
) {
    commands.spawn((
        Name::new("Tactical sunlight"),
        Transform::from_xyz(200.0, 1000.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            shadows_enabled: settings.shadows_enabled,
            illuminance: lux::DIRECT_SUNLIGHT,
            ..default()
        },
    ));

    let mut camera = commands.spawn((
        Name::new("Tactical gameplay camera"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 80.0_f32.to_radians(),
            ..default()
        }),
        Exposure::SUNLIGHT,
        Tonemapping::AcesFitted,
        // Gameplay MSAA is deliberately off even in the full preset.
        Msaa::Off,
    ));
    if settings.atmosphere_enabled {
        camera.insert(Atmosphere::earthlike(
            scattering_mediums.add(ScatteringMedium::default()),
        ));
        if settings.environment_light_enabled {
            camera.insert(AtmosphereEnvironmentMapLight {
                size: UVec2::splat(settings.environment_map_size),
                ..default()
            });
        }
    }
    if settings.bloom_enabled {
        camera.insert(Bloom::NATURAL);
    }
    if settings.ssao_enabled {
        camera.insert(ScreenSpaceAmbientOcclusion::default());
    }
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
