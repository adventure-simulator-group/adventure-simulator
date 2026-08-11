//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

mod environment;
mod foliage;
mod obstacles;
mod procedural;
mod terrain;
mod vista;
mod weather;

use environment::*;
use foliage::*;
use obstacles::tree::*;
use obstacles::{on_scene_obstacle_added, present_pending_trees};
use procedural::*;
use terrain::*;
use vista::*;
use weather::*;

// This facade is compiled independently by several binaries, so each binary
// uses only the subset of the stable presentation interface that it needs.
#[allow(unused_imports)]
pub(crate) use environment::TacticalSunlight;
#[allow(unused_imports)]
pub(crate) use foliage::{FoliageLayer, GrassInteractor};
#[allow(unused_imports)]
pub(crate) use obstacles::oak_review_terminal_specimen;
#[allow(unused_imports)]
pub(crate) use obstacles::rock::ProceduralRockVisual;
#[allow(unused_imports)]
pub(crate) use obstacles::tree::TreeLod;
#[allow(unused_imports)]
pub(crate) use obstacles::tree::impostor::TreeImpostorProvenance;
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
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, NotShadowCaster, atmosphere::ScatteringMedium,
        light_consts::lux,
    },
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::{AtmosphereSettings, ExtendedMaterial, MaterialExtension, ScreenSpaceAmbientOcclusion},
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};

#[derive(Debug, Clone, Copy)]
pub struct TacticalPresentationPlugin {
    pub shadows_enabled: bool,
    pub atmosphere_enabled: bool,
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
            MaterialPlugin::<TacticalFoliageMaterial>::default(),
            MaterialPlugin::<TacticalTreeImpostorMaterial>::default(),
        ))
        .insert_resource(TacticalGraphicsSettings {
            shadows_enabled: self.shadows_enabled,
            atmosphere_enabled: self.atmosphere_enabled,
            environment_light_enabled: self.environment_light_enabled,
            environment_map_size: self.environment_map_size,
            bloom_enabled: self.bloom_enabled,
            ssao_enabled: self.ssao_enabled,
            max_vista_lods: self.max_vista_lods,
        })
        .add_systems(Startup, setup_tactical_presentation)
        .init_resource::<GrassInteractionState>()
        .init_resource::<TreePresentationCache>()
        .add_systems(
            Update,
            (
                advance_weather_particles,
                update_grass_interaction,
                present_pending_trees,
            ),
        )
        .add_observer(on_game_scene_added)
        .add_observer(environment::on_environment_added)
        .add_observer(terrain::on_environment_added)
        .add_observer(foliage::on_environment_added)
        .add_observer(weather::on_environment_added)
        .add_observer(on_scene_obstacle_added)
        .add_observer(on_scene_vista_bundle);
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
    max_vista_lods: usize,
}
