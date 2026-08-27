//! Shared tactical scene and gameplay-camera presentation.
//!
//! Both the networked client and deterministic animation capture install this
//! plugin so screenshots cannot drift to a different camera, terrain mesh,
//! lighting, or post-processing setup.

mod atmosphere;
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

use atmosphere::*;
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

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TerrainTriangleCount(pub(crate) usize);

fn mesh_triangle_count(mesh: &Mesh) -> usize {
    mesh.indices()
        .map_or_else(|| mesh.count_vertices() / 3, |indices| indices.len() / 3)
}

// This facade is compiled independently by several binaries, so each binary
// uses only the subset of the stable presentation interface that it needs.
#[allow(unused_imports)]
pub(crate) use clouds::{
    TacticalCloudAnimationStatus, TacticalCloudBenchmarkIsolation, TacticalCloudCaptureOverride,
    TacticalCloudCaptureProfile, TacticalCloudLayer, TacticalCloudOffscreenCamera,
};
#[allow(unused_imports)]
pub(crate) use environment::{
    TacticalCameraSetup, TacticalGameplayCamera, scene_ambient_light, scene_ibl_visibility_floor,
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
    DETAIL_PATCH_SPACING_METRES, TerrainDetailPatch, TerrainMaterialPresentation,
    terrain_heightmap_image,
};
#[allow(unused_imports)]
pub(crate) use vista::{VistaTerrain, VistaTerrainMesh, VistaTreePresentation};
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
    post_process::bloom::Bloom,
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat, VertexFormat,
    },
    shader::ShaderRef,
};
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

#[derive(Debug, Clone, Copy)]
pub struct TacticalPresentationPlugin {
    pub shadows_enabled: bool,
    pub atmosphere_enabled: bool,
    pub celestial_enabled: bool,
    pub environment_light_enabled: bool,
    pub environment_map_size: u32,
    pub bloom_enabled: bool,
    pub max_vista_lods: usize,
    /// Scales the per-tuft shoot density of the instanced sward; presets use
    /// it to trade meadow fullness against vertex throughput on weak GPUs.
    pub grass_density_scale: f32,
    /// Uniformly contracts the instanced grass LOD fade bands. Near-tier
    /// vertex cost scales with the band radius squared, so this is the
    /// primary grass frame-rate lever; 1.0 reproduces the legacy reach.
    pub grass_range_scale: f32,
    /// Scales the volumetric cloud ray-march sample budget (clamped to
    /// 0.35..=1.0 in the shader). The march burns a near-constant ~20 ms per
    /// frame at QHD reference quality, so gameplay presets lower it; 1.0
    /// keeps the full-fidelity march for capture tooling.
    pub cloud_quality_scale: f32,
    /// Resolution fraction for the offscreen volumetric cloud pass (clamped
    /// to 0.25..=1.0). Below 1.0 the shells render into a reduced target and
    /// composite through one dome, cutting march cost with the square of the
    /// scale; 1.0 keeps the legacy full-resolution in-view path.
    pub cloud_resolution_scale: f32,
    /// MSAA sample count for the gameplay camera (1, 2, or 4). Coverage
    /// bandwidth at QHD scales with it; AlphaToCoverage foliage keeps
    /// working at 2, and at 1 the cutout falls back to hard discards.
    pub msaa_samples: u8,
    /// Directional shadow cascade count; 0 keeps the engine default (4).
    pub shadow_cascade_count: usize,
    /// Directional shadow reach in metres; 0.0 keeps the engine default.
    /// Cascade texel density rises as this shrinks, so gameplay presets can
    /// trade distant contact shadows for cheaper, sharper near ones.
    pub shadow_maximum_distance: f32,
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
            max_vista_lods: 3,
            grass_density_scale: 1.0,
            grass_range_scale: 1.0,
            cloud_quality_scale: 1.0,
            cloud_resolution_scale: 1.0,
            msaa_samples: 4,
            shadow_cascade_count: 0,
            shadow_maximum_distance: 0.0,
        }
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
        .add_plugins(MaterialPlugin::<TacticalWeatherMaterial>::default())
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
            bloom_enabled: self.bloom_enabled,
            max_vista_lods: self.max_vista_lods,
            grass_density_scale: self.grass_density_scale,
            grass_range_scale: self.grass_range_scale,
            cloud_quality_scale: self.cloud_quality_scale,
            cloud_resolution_scale: self.cloud_resolution_scale,
            msaa_samples: self.msaa_samples,
            shadow_cascade_count: self.shadow_cascade_count,
            shadow_maximum_distance: self.shadow_maximum_distance,
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
        .add_observer(on_scene_vista_bundle);
    }

    fn finish(&self, app: &mut App) {
        // Install this after every plugin has built so the RenderApp and
        // Bevy's atmosphere extractor exist regardless of plugin order.
        install_atmosphere_cleanup_backport(app);
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
    pub(crate) max_vista_lods: usize,
    pub(crate) grass_density_scale: f32,
    pub(crate) grass_range_scale: f32,
    pub(crate) cloud_quality_scale: f32,
    pub(crate) cloud_resolution_scale: f32,
    pub(crate) msaa_samples: u8,
    pub(crate) shadow_cascade_count: usize,
    pub(crate) shadow_maximum_distance: f32,
}
