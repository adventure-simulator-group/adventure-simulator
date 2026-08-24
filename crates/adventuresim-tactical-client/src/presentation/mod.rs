//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

mod clouds;
mod environment;
mod ground_scatter;
mod obstacles;
mod procedural;
mod procedural_assets;
mod sky;
mod terrain;
mod vista;
mod volumetric;
mod weather;

use clouds::*;
use environment::*;
use ground_scatter::*;
use obstacles::on_scene_obstacle_added;
use obstacles::rock::TacticalRockMaterial;
use obstacles::tree::*;
use procedural::*;
#[cfg(test)]
use procedural_assets::generate_procedural_environment_assets;
use procedural_assets::{LeafTextureSet, setup_procedural_environment_assets};
use sky::*;
use terrain::*;
use vista::*;
use volumetric::*;
use weather::*;

// This facade is compiled independently by several binaries, so each binary
// uses only the subset of the stable presentation interface that it needs.
#[allow(unused_imports)]
pub(crate) use clouds::TacticalCloudLayer;
#[allow(unused_imports)]
pub(crate) use clouds::{
    TacticalCloudAnimationStatus, TacticalCloudBenchmarkIsolation, TacticalCloudCaptureOverride,
    TacticalCloudCaptureProfile,
};
#[allow(unused_imports)]
pub(crate) use environment::{
    TacticalCameraSetup, scene_ambient_light, scene_ibl_visibility_floor,
};
#[allow(unused_imports)]
pub(crate) use ground_scatter::{
    GrassInteractor, GroundLitterCaptureAnchors, GroundLitterCapturePair, GroundLitterDiagnostics,
    GroundScatterLayer, LooseStonePebblePatch,
};
#[allow(unused_imports)]
pub(crate) use obstacles::oak_review_terminal_specimen;
#[allow(unused_imports)]
pub(crate) use obstacles::rock::ProceduralRockVisual;
#[allow(unused_imports)]
pub(crate) use obstacles::tree::TreeImpostorProvenance;
#[allow(unused_imports)]
pub(crate) use obstacles::tree::{
    PlayableTreeAggregateWood, PlayableTreeBuds, PlayableTreeCanopyCard,
    PlayableTreeDetailedLeaves, PlayableTreeDetailedTrunk, PlayableTreeDetailedWood,
    PlayableTreeMidTrunk, PlayableTreeTrunk, PresentedTree, TacticalTreeAggregateBarkMaterial,
    TacticalTreeBarkMaterial, TacticalTreeBenchmarkIsolation, TacticalTreeLeafCardMaterial,
    TreeAssetResidencyDiagnostics, TreeLeafRepresentation, TreeLeafTriangleCount, TreeLod,
    TreeLodCluster, TreeLodRenderOverride, TreeTrunkLod, oak_aggregate_bark_material,
    oak_bark_material, oak_leaf_material,
};
pub(crate) use procedural_assets::ProceduralEnvironmentAssets;
pub(crate) use sky::AtmosphereIblAmbientHandoff;
#[allow(unused_imports)]
pub(crate) use sky::{TacticalMoon, TacticalMoonlight, TacticalStars, TacticalSunlight};
#[allow(unused_imports)]
pub(crate) use terrain::{
    TerrainDetailPatch, TerrainMaterialPresentation, terrain_heightmap_image,
};
#[allow(unused_imports)]
pub(crate) use vista::{VistaTerrain, VistaTreePresentation};
#[allow(unused_imports)]
pub(crate) use weather::WeatherParticle;

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
#[cfg(test)]
use bevy::mesh::VertexAttributeValues;
use bevy::{
    asset::RenderAssetUsages,
    camera::{
        Exposure,
        visibility::{NoFrustumCulling, VisibilityRange},
    },
    core_pipeline::tonemapping::Tonemapping,
    image::ImageSampler,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, DirectionalLightShadowMap, EnvironmentMapLight,
        NotShadowCaster, atmosphere::ScatteringMedium, light_consts::lux,
    },
    mesh::{Indices, MeshVertexAttribute, PrimitiveTopology},
    pbr::{AtmosphereSettings, ExtendedMaterial, MaterialExtension},
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
            max_vista_lods: 3,
        }
    }
}

impl Plugin for TacticalPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<TacticalTerrainMaterial>::default(),
            MaterialPlugin::<TacticalVistaMaterial>::default(),
            MaterialPlugin::<TacticalRockMaterial>::default(),
            MaterialPlugin::<TacticalFoliageMaterial>::default(),
            MaterialPlugin::<TacticalPebbleBillboardMaterial>::default(),
            MaterialPlugin::<TacticalTreeBarkMaterial>::default(),
            MaterialPlugin::<TacticalTreeAggregateBarkMaterial>::default(),
            MaterialPlugin::<TacticalTreeLeafCardMaterial>::default(),
            MaterialPlugin::<TacticalTreeImpostorMaterial>::default(),
            MaterialPlugin::<TacticalMoonMaterial>::default(),
            MaterialPlugin::<TacticalStarMaterial>::default(),
            MaterialPlugin::<TacticalCloudMaterial>::default(),
            MaterialPlugin::<TacticalWeatherMaterial>::default(),
        ))
        // Tactical play uses one compact close-range cascade for whichever
        // celestial light is active. Keep the map allocation identical in the
        // game and all tactical review viewers.
        .insert_resource(DirectionalLightShadowMap {
            size: sky::TACTICAL_DIRECTIONAL_SHADOW_MAP_SIZE,
        })
        .insert_resource(TacticalGraphicsSettings {
            shadows_enabled: self.shadows_enabled,
            atmosphere_enabled: self.atmosphere_enabled,
            celestial_enabled: self.celestial_enabled,
            environment_light_enabled: self.environment_light_enabled,
            environment_map_size: self.environment_map_size,
            max_vista_lods: self.max_vista_lods,
        })
        .init_resource::<TacticalCameraSetup>()
        // The sky observer preserves this low, cool floor at night and restores
        // physically scaled diffuse sky irradiance during daylight.
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.36, 0.48, 0.72),
            brightness: 0.6,
            ..default()
        })
        .add_systems(
            Startup,
            (
                setup_procedural_environment_assets,
                setup_tactical_presentation,
                setup_tactical_sky,
                setup_tactical_clouds,
            )
                .chain(),
        )
        .init_resource::<GrassInteractionState>()
        .init_resource::<WoodyUnderstoryPresentationCache>()
        .init_resource::<GroundFoliagePresentationCache>()
        .init_resource::<TreePresentationCache>()
        .init_resource::<TreeAssetResidencyDiagnostics>()
        .init_resource::<VistaTreePresentationCache>()
        .init_resource::<ActiveVistaSurface>()
        .init_resource::<TreeLodRenderOverride>()
        .init_resource::<TacticalTreeBenchmarkIsolation>()
        .init_resource::<ActiveTacticalScene>()
        .init_resource::<PresentedCelestialLighting>()
        .init_resource::<AtmosphereIblAmbientHandoff>()
        .init_resource::<TacticalCloudCaptureOverride>()
        .init_resource::<TacticalCloudBenchmarkIsolation>()
        .init_resource::<WeatherOcclusionState>()
        .add_systems(
            Update,
            (
                update_grass_interaction,
                update_tree_leaf_wind,
                (
                    present_pending_terrain,
                    update_terrain_detail_patch,
                    present_ground_scatter,
                )
                    .chain(),
                (
                    refresh_active_tactical_scene,
                    update_presented_celestial_lighting,
                    apply_presented_celestial_lighting,
                )
                    .chain(),
                update_celestial_material_lighting
                    .after(update_presented_celestial_lighting)
                    .after(present_pending_trees),
                (
                    present_pending_trees,
                    stream_tree_lod_children,
                    update_tree_projected_lod_ranges,
                )
                    .chain(),
                keep_celestial_visuals_centered.after(update_presented_celestial_lighting),
                update_tactical_clouds.after(update_presented_celestial_lighting),
                update_global_ambient_policy.after(apply_presented_celestial_lighting),
                apply_active_environment_fog.after(refresh_active_tactical_scene),
                apply_active_scene_weather
                    .after(refresh_active_tactical_scene)
                    .after(present_pending_trees),
                update_weather_occlusion_map
                    .after(apply_active_scene_weather)
                    .after(present_pending_trees),
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
    pub(crate) max_vista_lods: usize,
}
