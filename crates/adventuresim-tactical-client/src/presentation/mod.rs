//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

#![expect(
    unused_imports,
    reason = "the gameplay client and capture binaries consume different parts of this shared presentation facade"
)]

mod atmosphere;
mod buildings;
mod clouds;
mod config;
mod doors;
mod environment;
pub(crate) mod ground_scatter;
mod obstacles;
mod procedural;
mod procedural_assets;
mod sky;
mod terrain;
mod terrain_blood;
mod vista;
mod volumetric;
mod weather;

use atmosphere::*;
use buildings::*;
use clouds::*;
pub(crate) use doors::{DoorPresentationPlugin, GrabTargetOutline};
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

pub use config::{
    AntiAliasingConfig, PresentModeConfig, ShadowFiltering, SmaaQuality, TacticalGraphicsConfig,
    TonemappingConfig, WindowModeConfig,
};

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TerrainTriangleCount(pub(crate) usize);

fn mesh_triangle_count(mesh: &Mesh) -> usize {
    mesh.indices()
        .map_or_else(|| mesh.count_vertices() / 3, |indices| indices.len() / 3)
}

// This facade is compiled independently by several binaries, so each binary
// uses only the subset of the stable presentation interface that it needs.
pub(crate) use buildings::PresentedBuildingMesh;
pub(crate) use clouds::{
    TacticalCloudAnimationStatus, TacticalCloudBenchmarkIsolation, TacticalCloudCaptureOverride,
    TacticalCloudCaptureProfile, TacticalCloudLayer, TacticalCloudOffscreenCamera,
};
pub(crate) use environment::{
    TacticalCameraSetup, TacticalGameplayCamera, scene_ambient_light, scene_ibl_visibility_floor,
};
pub(crate) use ground_scatter::{
    GrassInteractor, GroundLitterCaptureAnchors, GroundLitterCapturePair, GroundLitterDiagnostics,
    GroundScatterLayer, LooseStonePebblePatch,
};
pub(crate) use obstacles::oak_review_terminal_specimen;
pub(crate) use obstacles::rock::ProceduralRockVisual;
pub(crate) use obstacles::tree::TreeImpostorProvenance;
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
pub(crate) use sky::{TacticalMoon, TacticalMoonlight, TacticalStars, TacticalSunlight};
pub(crate) use terrain::{
    DETAIL_PATCH_SPACING_METRES, TerrainDetailPatch, TerrainMaterialPresentation,
    terrain_heightmap_image,
};
pub(crate) use vista::{VistaTerrain, VistaTerrainMesh, VistaTreePresentation};
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
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat, VertexFormat,
    },
    shader::ShaderRef,
};
pub(crate) use fabelgeist_determinism::splitmix64;
use web_time::Instant;

#[derive(Resource)]
pub(crate) struct ClientStartupTiming {
    started_at: Instant,
    terrain_preparation_reported: bool,
}

impl ClientStartupTiming {
    pub(crate) fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            terrain_preparation_reported: false,
        }
    }

    pub(crate) fn mark(&self, phase: &str) {
        let elapsed_ms = self.started_at.elapsed().as_millis();
        info!(phase, elapsed_ms, "[startup] tactical client phase");
        #[cfg(not(target_family = "wasm"))]
        eprintln!("[startup] native client phase={phase:?} elapsed_ms={elapsed_ms}");
    }

    pub(crate) fn mark_terrain_prepared_once(&mut self) {
        if self.terrain_preparation_reported {
            return;
        }
        self.terrain_preparation_reported = true;
        self.mark("first tactical terrain prepared");
    }
}

#[derive(Debug, Clone)]
pub struct TacticalPresentationPlugin {
    pub config: TacticalGraphicsConfig,
}

impl Default for TacticalPresentationPlugin {
    fn default() -> Self {
        Self {
            config: TacticalGraphicsConfig::parse(include_str!(
                "../../../../assets/config/tactical-graphics.yaml"
            ))
            .expect("shipped tactical graphics configuration must be valid"),
        }
    }
}

struct TacticalWeatherAndDoorPlugin;

impl Plugin for TacticalWeatherAndDoorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TacticalWeatherMaterial>::default())
            .add_plugins(DoorPresentationPlugin);
    }
}

fn tactical_global_ambient_light() -> GlobalAmbientLight {
    GlobalAmbientLight {
        color: Color::srgb(0.36, 0.48, 0.72),
        brightness: 0.6,
        ..default()
    }
}

impl Plugin for TacticalPresentationPlugin {
    fn build(&self, app: &mut App) {
        // GPU-instanced grass renders through bevy_eidolon on native builds;
        // the browser bundle keeps the legacy patch renderer until the wasm
        // indirect-draw fallback lands.
        #[cfg(all(feature = "instanced-grass", not(target_family = "wasm")))]
        app.add_plugins(ground_scatter::InstancedGrassPlugin);
        app.add_plugins((
            MaterialPlugin::<TacticalTerrainMaterial>::default(),
            MaterialPlugin::<TacticalVistaMaterial>::default(),
            MaterialPlugin::<TacticalRockMaterial>::default(),
            MaterialPlugin::<TacticalFoliageMaterial>::default(),
            MaterialPlugin::<TacticalPebbleMaterial>::default(),
            MaterialPlugin::<TacticalPebbleBillboardMaterial>::default(),
            MaterialPlugin::<TacticalTreeBarkMaterial>::default(),
            MaterialPlugin::<TacticalTreeAggregateBarkMaterial>::default(),
            MaterialPlugin::<TacticalTreeLeafCardMaterial>::default(),
            MaterialPlugin::<TacticalTreeImpostorMaterial>::default(),
            MaterialPlugin::<TacticalMoonMaterial>::default(),
            MaterialPlugin::<TacticalSunMaterial>::default(),
            MaterialPlugin::<TacticalStarMaterial>::default(),
            MaterialPlugin::<TacticalCloudMaterial>::default(),
            MaterialPlugin::<TacticalCloudCompositeMaterial>::default(),
        ))
        // Split from the tuple above so the material-plugin group stays within
        // Bevy's 15-element `Plugins` tuple arity limit.
        .add_plugins(TacticalWeatherAndDoorPlugin)
        // Tactical play uses one compact close-range cascade for whichever
        // celestial light is active. Keep the map allocation identical in the
        // game and all tactical review viewers.
        .insert_resource(DirectionalLightShadowMap {
            size: self.config.rendering.shadows.map_size,
        })
        .insert_resource(TacticalGraphicsSettings {
            config: self.config.clone(),
        })
        .init_resource::<TacticalCameraSetup>()
        // The sky observer preserves this low, cool floor at night and restores
        // physically scaled diffuse sky irradiance during daylight.
        .insert_resource(tactical_global_ambient_light())
        .add_systems(
            Startup,
            (
                setup_procedural_environment_assets,
                setup_tactical_building_materials,
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
        .init_resource::<FrozenAtmosphereStatus>()
        .init_resource::<AtmosphereIblAmbientHandoff>()
        .init_resource::<TacticalCloudCaptureOverride>()
        .init_resource::<TacticalCloudBenchmarkIsolation>()
        .init_resource::<WeatherOcclusionState>()
        .add_systems(
            Update,
            (
                update_grass_interaction,
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
                update_tactical_cloud_offscreen_target,
                update_global_ambient_policy.after(apply_presented_celestial_lighting),
                freeze_initialized_atmosphere
                    .after(update_global_ambient_policy)
                    .after(update_presented_celestial_lighting),
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
        .add_observer(on_scene_building_added)
        .add_observer(on_scene_vista_buildings)
        .add_observer(on_scene_vista_bundle);
    }

    fn finish(&self, app: &mut App) {
        // Install this after every plugin has built so the RenderApp and
        // Bevy's atmosphere extractor exist regardless of plugin order.
        install_atmosphere_cleanup_backport(app);
    }
}

#[derive(Resource, Debug, Clone)]
pub(crate) struct TacticalGraphicsSettings {
    pub(crate) config: TacticalGraphicsConfig,
}

impl Default for TacticalGraphicsSettings {
    fn default() -> Self {
        Self {
            config: TacticalPresentationPlugin::default().config,
        }
    }
}
