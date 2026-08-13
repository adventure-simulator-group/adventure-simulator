//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

mod environment;
mod foliage;
mod obstacles;
mod procedural;
mod sky;
mod terrain;
mod vista;
mod volumetric;
mod weather;

use environment::*;
use foliage::*;
use obstacles::rock::{TacticalRockMaterial, procedural_rock_mesh, rock_color};
use obstacles::tree::*;
use obstacles::{on_scene_obstacle_added, present_pending_trees};
use procedural::*;
use sky::*;
use terrain::*;
use vista::*;
use volumetric::*;
use weather::*;

// This facade is compiled independently by several binaries, so each binary
// uses only the subset of the stable presentation interface that it needs.
#[allow(unused_imports)]
pub(crate) use environment::{scene_ambient_light, scene_ibl_visibility_floor};
#[allow(unused_imports)]
pub(crate) use foliage::{GrassInteractor, GroundScatterLayer};
#[allow(unused_imports)]
pub(crate) use obstacles::oak_review_terminal_specimen;
#[allow(unused_imports)]
pub(crate) use obstacles::rock::ProceduralRockVisual;
#[allow(unused_imports)]
pub(crate) use obstacles::tree::impostor::TreeImpostorProvenance;
#[allow(unused_imports)]
pub(crate) use obstacles::tree::{
    TacticalTreeLeafCardMaterial, TreeLeafRepresentation, TreeLod, TreeLodCluster,
    TreeLodRenderOverride, TreeTrunkLod, oak_bark_material, oak_leaf_material,
};
pub(crate) use sky::AtmosphereIblAmbientHandoff;
#[allow(unused_imports)]
pub(crate) use sky::{TacticalMoon, TacticalMoonlight, TacticalStars, TacticalSunlight};
#[allow(unused_imports)]
pub(crate) use terrain::TerrainMaterialPresentation;
#[allow(unused_imports)]
pub(crate) use vista::VistaTerrain;
#[allow(unused_imports)]
pub(crate) use weather::WeatherParticle;

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::{
    asset::RenderAssetUsages,
    camera::{Exposure, visibility::VisibilityRange},
    core_pipeline::tonemapping::Tonemapping,
    image::ImageSampler,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, EnvironmentMapLight, NotShadowCaster,
        atmosphere::ScatteringMedium, light_consts::lux,
    },
    mesh::{Indices, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues},
    pbr::{AtmosphereSettings, ExtendedMaterial, MaterialExtension, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat, VertexFormat,
    },
    shader::ShaderRef,
};

#[derive(Debug, Clone, Copy)]
pub struct TacticalPresentationPlugin {
    pub shadows_enabled: bool,
    pub atmosphere_enabled: bool,
    pub celestial_enabled: bool,
    pub environment_light_enabled: bool,
    pub environment_map_size: u32,
    pub bloom_enabled: bool,
    pub ssao_enabled: bool,
    pub max_vista_lods: usize,
}

impl Default for TacticalPresentationPlugin {
    fn default() -> Self {
        Self {
            shadows_enabled: true,
            atmosphere_enabled: true,
            celestial_enabled: true,
            environment_light_enabled: true,
            environment_map_size: 64,
            bloom_enabled: true,
            ssao_enabled: true,
            max_vista_lods: 3,
        }
    }
}

impl Plugin for TacticalPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<TacticalTerrainMaterial>::default(),
            MaterialPlugin::<TacticalRockMaterial>::default(),
            MaterialPlugin::<TacticalFoliageMaterial>::default(),
            MaterialPlugin::<TacticalTreeLeafCardMaterial>::default(),
            MaterialPlugin::<TacticalTreeImpostorMaterial>::default(),
            MaterialPlugin::<TacticalMoonMaterial>::default(),
            MaterialPlugin::<TacticalStarMaterial>::default(),
        ))
        .insert_resource(TacticalGraphicsSettings {
            shadows_enabled: self.shadows_enabled,
            atmosphere_enabled: self.atmosphere_enabled,
            celestial_enabled: self.celestial_enabled,
            environment_light_enabled: self.environment_light_enabled,
            environment_map_size: self.environment_map_size,
            bloom_enabled: self.bloom_enabled,
            ssao_enabled: self.ssao_enabled,
            max_vista_lods: self.max_vista_lods,
        })
        // The sky observer preserves this low, cool floor at night and restores
        // physically scaled diffuse sky irradiance during daylight.
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.36, 0.48, 0.72),
            brightness: 0.6,
            ..default()
        })
        .add_systems(
            Startup,
            (setup_tactical_presentation, setup_tactical_sky).chain(),
        )
        .init_resource::<GrassInteractionState>()
        .init_resource::<HazelPresentationCache>()
        .init_resource::<GroundFoliagePresentationCache>()
        .init_resource::<TreePresentationCache>()
        .init_resource::<TreeLodRenderOverride>()
        .init_resource::<ActiveTacticalScene>()
        .init_resource::<PresentedCelestialLighting>()
        .init_resource::<AtmosphereIblAmbientHandoff>()
        .add_systems(
            Update,
            (
                update_grass_interaction,
                update_tree_leaf_wind,
                present_ground_scatter,
                (
                    refresh_active_tactical_scene,
                    update_presented_celestial_lighting,
                    apply_presented_celestial_lighting,
                )
                    .chain(),
                update_celestial_material_lighting.after(update_presented_celestial_lighting),
                (present_pending_trees, update_tree_projected_lod_ranges).chain(),
                keep_celestial_visuals_centered.after(update_presented_celestial_lighting),
                update_global_ambient_policy.after(update_presented_celestial_lighting),
                apply_active_environment_fog.after(refresh_active_tactical_scene),
                (apply_active_scene_weather, advance_weather_particles)
                    .chain()
                    .after(refresh_active_tactical_scene),
            ),
        )
        .add_observer(on_game_scene_added)
        .add_observer(activate_tactical_scene)
        .add_observer(terrain::on_environment_added)
        .add_observer(terrain::on_ground_added)
        .add_observer(on_scene_obstacle_added)
        .add_observer(on_scene_vista_bundle);
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct TacticalGraphicsSettings {
    pub(crate) shadows_enabled: bool,
    pub(crate) atmosphere_enabled: bool,
    pub(crate) celestial_enabled: bool,
    pub(crate) environment_light_enabled: bool,
    pub(crate) environment_map_size: u32,
    pub(crate) bloom_enabled: bool,
    pub(crate) ssao_enabled: bool,
    pub(crate) max_vista_lods: usize,
}
