use adventuresim_tactical_core::prelude::{SceneSource, WeatherSnapshot};
use serde::Serialize;

use super::view_specs::CaptureViewSpec;

#[derive(Clone, Serialize)]
pub(super) struct CaptureRecord {
    pub(super) view: String,
    pub(super) label: String,
    pub(super) screenshot: String,
    pub(super) camera_translation: [f32; 3],
    pub(super) camera_target: [f32; 3],
    pub(super) camera_up: [f32; 3],
    pub(super) vertical_fov_degrees: f32,
    pub(super) foreground_pixel_bps: u16,
    pub(super) detail_pixel_bps: u16,
    pub(super) forced_tree_lod: Option<u8>,
    pub(super) focused_tree_lod_queued: Option<bool>,
    pub(super) diagnostic_leaf_suppression: bool,
    pub(super) diagnostic_grass_suppression: bool,
    pub(super) debris_leaf_distance_metres: Option<f32>,
    pub(super) debris_twig_distance_metres: Option<f32>,
    pub(super) lighting_luminance_samples: Vec<f32>,
    pub(super) lighting_luminance_delta: f32,
    pub(super) lighting_ready: bool,
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct RepairSummary {
    pub(super) upsampled_height_samples: u32,
    pub(super) microrelief_adjusted_samples: u32,
    pub(super) adjusted_height_samples: u32,
    pub(super) repaired_water_samples: u32,
    pub(super) removed_corridor_obstacles: u32,
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct TerrainSummary {
    pub(super) width_metres: f32,
    pub(super) depth_metres: f32,
    pub(super) source_spacing_metres: f32,
    pub(super) spacing_metres: f32,
    pub(super) source_samples: usize,
    pub(super) generated_samples: usize,
    pub(super) minimum_height_metres: f32,
    pub(super) maximum_height_metres: f32,
}

#[derive(Serialize)]
pub(super) struct ObstacleSummary {
    pub(super) generated_trees: usize,
    pub(super) generated_rocks: usize,
    pub(super) presented_trees: usize,
    pub(super) presented_rocks: usize,
    pub(super) collider_trees: usize,
    pub(super) collider_rocks: usize,
    pub(super) procedural_rock_meshes: usize,
    pub(super) rock_meshes_inside_colliders: bool,
    pub(super) tree_lods_presented: Vec<u8>,
}

#[derive(Serialize)]
pub(super) struct FoliageSummary {
    pub(super) grass_clumps: usize,
    pub(super) understory_clumps: usize,
    pub(super) dry_leaf_patches: usize,
    pub(super) twig_patches: usize,
    pub(super) loose_stone_patches: usize,
}

#[derive(Serialize)]
pub(super) struct TreeBakeSummary {
    pub(super) seed: u64,
    pub(super) lod: u8,
    pub(super) bake_version: u32,
    pub(super) source_geometry_hash: String,
    pub(super) render_method: &'static str,
    pub(super) atlas_size: [u32; 2],
    pub(super) cards: Vec<TreeBakeCardSummary>,
}

#[derive(Serialize)]
pub(super) struct TreeBakeCardSummary {
    pub(super) source_group: u16,
    pub(super) source_leaf_count: u16,
    pub(super) source_branch_count: u16,
    pub(super) view_direction: [f32; 3],
    pub(super) projected_bounds: [f32; 4],
    pub(super) atlas_region: [u32; 4],
    pub(super) opaque_pixel_count: u32,
    pub(super) silhouette_centroid: [f32; 2],
}

#[derive(Serialize)]
pub(super) struct RecursiveTreeLodSummary {
    pub(super) primary_cluster_count: usize,
    pub(super) visible_group_lods: Vec<[u8; 2]>,
    pub(super) visible_aggregate_lods: Vec<u8>,
    pub(super) mixed_lods_observed: bool,
}

#[derive(Serialize)]
pub(super) struct VistaSummary {
    pub(super) supplied_lods: usize,
    pub(super) presented_lods: Vec<u8>,
    pub(super) presented_chunks: usize,
    pub(super) diameter_metres: f32,
    pub(super) minimum_height_metres: f32,
    pub(super) peak_height_metres: f32,
    pub(super) relief_metres: f32,
    pub(super) collider_count: usize,
}

#[derive(Serialize)]
pub(super) struct ValidationSummary {
    pub(super) all_views_captured: bool,
    pub(super) requested_views_captured_exactly_once: bool,
    pub(super) requested_detail_targets_available: bool,
    pub(super) production_lighting_parity: bool,
    pub(super) lighting_readiness: bool,
    pub(super) all_views_render_content: bool,
    pub(super) foliage_detail_present: bool,
    pub(super) all_obstacles_presented: bool,
    pub(super) all_obstacles_collidable: bool,
    pub(super) procedural_rocks_fit_colliders: bool,
    pub(super) trees_have_five_lods: bool,
    pub(super) tree_detail_captured_when_expected: bool,
    pub(super) recursive_tree_lod_observed: bool,
    pub(super) terrain_material_present: bool,
    pub(super) coarse_source_terrain_upsampled: bool,
    pub(super) microrelief_present: bool,
    pub(super) grass_present_when_expected: bool,
    pub(super) forest_floor_scatter_present_when_trees: bool,
    pub(super) understory_present_when_expected: bool,
    pub(super) loose_stone_scatter_present_when_expected: bool,
    pub(super) vista_has_three_lods: bool,
    pub(super) vista_reaches_fifty_kilometres: bool,
    pub(super) vista_has_no_colliders: bool,
    pub(super) precipitation_particles_present_when_expected: bool,
    pub(super) fixture_feature_expectation_met: bool,
    pub(super) passed: bool,
    pub(super) note: &'static str,
}

#[derive(Serialize)]
pub(super) struct CaptureManifest {
    pub(super) pipeline: &'static str,
    pub(super) fixture: String,
    pub(super) source_input: String,
    pub(super) scene_digest: String,
    pub(super) seed: u64,
    pub(super) absolute_minute: u64,
    pub(super) canopy_bps: u16,
    pub(super) generation_version: u16,
    pub(super) scene_source: SceneSource,
    pub(super) capture_profile: String,
    pub(super) capture_profile_version: u16,
    pub(super) camera_version: u16,
    pub(super) requested_views: Vec<String>,
    pub(super) settle_frames: u32,
    pub(super) resolution: [u32; 2],
    pub(super) review_azimuth_degrees: f32,
    pub(super) capture_clock_strategy: &'static str,
    pub(super) capture_clock_phase_seconds: f32,
    pub(super) renderer: &'static str,
    pub(super) executable_version: &'static str,
    pub(super) revision: String,
    pub(super) source_identity: String,
    pub(super) celestial: CelestialProvenance,
    pub(super) presentation_features: PresentationFeatures,
    pub(super) weather: WeatherSnapshot,
    pub(super) repairs: RepairSummary,
    pub(super) terrain: TerrainSummary,
    pub(super) obstacles: ObstacleSummary,
    pub(super) foliage: FoliageSummary,
    pub(super) tree_impostor_bakes: Vec<TreeBakeSummary>,
    pub(super) recursive_tree_lod: RecursiveTreeLodSummary,
    pub(super) vista: VistaSummary,
    pub(super) weather_particle_count: usize,
    pub(super) captures: Vec<CaptureRecord>,
    pub(super) validation: ValidationSummary,
}

pub(super) struct PendingCaptureManifest {
    manifest: CaptureManifest,
}

impl PendingCaptureManifest {
    pub(super) fn new(manifest: CaptureManifest) -> Self {
        Self { manifest }
    }

    pub(super) fn finalize_after_screenshot(
        mut self,
        captures: &[CaptureRecord],
        views: &[CaptureViewSpec],
    ) -> (CaptureManifest, bool) {
        self.manifest.captures.clone_from_slice(captures);
        finalize_screenshot_validation(
            &self.manifest.fixture,
            &self.manifest.capture_profile,
            &self.manifest.requested_views,
            &mut self.manifest.validation,
            &self.manifest.captures,
            views,
        );
        let valid = self.manifest.validation.passed;
        (self.manifest, valid)
    }
}

fn finalize_screenshot_validation(
    fixture: &str,
    capture_profile: &str,
    requested_views: &[String],
    validation: &mut ValidationSummary,
    captures: &[CaptureRecord],
    views: &[CaptureViewSpec],
) {
    validation.all_views_render_content = captures.iter().all(|capture| {
        views
            .iter()
            .find(|view| view.slug == capture.view)
            .is_some_and(|view| capture.foreground_pixel_bps >= view.minimum_foreground_bps)
    });
    if requested_views
        .iter()
        .any(|view| view == "forest-floor-debris-detail")
    {
        validation.requested_detail_targets_available &= captures
            .iter()
            .find(|capture| capture.view == "forest-floor-debris-detail")
            .is_some_and(|capture| capture.detail_pixel_bps >= 60);
    }
    validation.foliage_detail_present = capture_profile != "semantic"
        || fixture != "flat-dry-grassland"
        || captures
            .iter()
            .find(|capture| capture.view == "beauty-overhead")
            .is_some_and(|capture| capture.detail_pixel_bps >= 100);
    if fixture == "narrow-peak-lod-boundary" {
        validation.fixture_feature_expectation_met &= captures
            .iter()
            .find(|capture| capture.view == "horizon")
            .is_some_and(|capture| capture.foreground_pixel_bps >= 200);
    }
    validation.passed = validation_passes(validation);
}

#[derive(Serialize)]
pub(super) struct CelestialProvenance {
    pub(super) sun_altitude_degrees: f32,
    pub(super) moon_altitude_degrees: f32,
    pub(super) lunar_illumination: f32,
}

#[derive(Clone, Serialize)]
pub(super) struct PresentationFeatures {
    pub(super) requested: PresentationFeatureState,
    pub(super) observed: ObservedPresentationFeatures,
    pub(super) requested_matches_observed: bool,
    pub(super) weather_iteration_in_scope: bool,
    pub(super) water_iteration_in_scope: bool,
    pub(super) cloud_iteration_in_scope: bool,
    pub(super) cave_iteration_in_scope: bool,
    pub(super) characters_present: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(super) struct PresentationFeatureState {
    pub(super) shadows: bool,
    pub(super) atmosphere: bool,
    pub(super) celestial: bool,
    pub(super) environment_light: bool,
    pub(super) environment_map_size: u32,
    pub(super) bloom: bool,
    pub(super) max_vista_lods: usize,
}

#[derive(Clone, Serialize)]
pub(super) struct ObservedPresentationFeatures {
    pub(super) settings: PresentationFeatureState,
    pub(super) camera_environment_map: bool,
    pub(super) camera_environment_map_size: Option<[u32; 2]>,
    pub(super) camera_environment_map_allocated: bool,
    pub(super) camera_environment_map_intensity: Option<f32>,
    pub(super) camera_bloom: bool,
    pub(super) camera_exposure_ev100: f32,
    pub(super) camera_tonemapping: String,
    pub(super) ambient_color: [f32; 4],
    pub(super) ambient_brightness: f32,
    pub(super) ambient_policy: &'static str,
    pub(super) expected_ambient_brightness: f32,
}

pub(super) fn validation_passes(validation: &ValidationSummary) -> bool {
    validation.all_views_captured
        && validation.requested_views_captured_exactly_once
        && validation.requested_detail_targets_available
        && validation.production_lighting_parity
        && validation.lighting_readiness
        && validation.all_views_render_content
        && validation.foliage_detail_present
        && validation.all_obstacles_presented
        && validation.all_obstacles_collidable
        && validation.procedural_rocks_fit_colliders
        && validation.trees_have_five_lods
        && validation.tree_detail_captured_when_expected
        && validation.recursive_tree_lod_observed
        && validation.terrain_material_present
        && validation.coarse_source_terrain_upsampled
        && validation.microrelief_present
        && validation.grass_present_when_expected
        && validation.forest_floor_scatter_present_when_trees
        && validation.understory_present_when_expected
        && validation.loose_stone_scatter_present_when_expected
        && validation.vista_has_three_lods
        && validation.vista_reaches_fifty_kilometres
        && validation.vista_has_no_colliders
        && validation.precipitation_particles_present_when_expected
        && validation.fixture_feature_expectation_met
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tactical_scene_viewer::view_specs::CapturePose;

    fn passing_validation() -> ValidationSummary {
        ValidationSummary {
            all_views_captured: true,
            requested_views_captured_exactly_once: true,
            requested_detail_targets_available: true,
            production_lighting_parity: true,
            lighting_readiness: true,
            all_views_render_content: false,
            foliage_detail_present: false,
            all_obstacles_presented: true,
            all_obstacles_collidable: true,
            procedural_rocks_fit_colliders: true,
            trees_have_five_lods: true,
            tree_detail_captured_when_expected: true,
            recursive_tree_lod_observed: true,
            terrain_material_present: true,
            coarse_source_terrain_upsampled: true,
            microrelief_present: true,
            grass_present_when_expected: true,
            forest_floor_scatter_present_when_trees: true,
            understory_present_when_expected: true,
            loose_stone_scatter_present_when_expected: true,
            vista_has_three_lods: true,
            vista_reaches_fifty_kilometres: true,
            vista_has_no_colliders: true,
            precipitation_particles_present_when_expected: true,
            fixture_feature_expectation_met: true,
            passed: false,
            note: "test",
        }
    }

    fn capture(view: &str, foreground_pixel_bps: u16, detail_pixel_bps: u16) -> CaptureRecord {
        CaptureRecord {
            view: view.into(),
            label: view.into(),
            screenshot: format!("{view}.png"),
            camera_translation: [0.0; 3],
            camera_target: [0.0; 3],
            camera_up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 45.0,
            foreground_pixel_bps,
            detail_pixel_bps,
            forced_tree_lod: None,
            focused_tree_lod_queued: None,
            diagnostic_leaf_suppression: false,
            diagnostic_grass_suppression: false,
            debris_leaf_distance_metres: None,
            debris_twig_distance_metres: None,
            lighting_luminance_samples: vec![10.0, 10.0],
            lighting_luminance_delta: 0.0,
            lighting_ready: true,
        }
    }

    #[test]
    fn final_screenshot_metrics_drive_manifest_validation() {
        let views = [
            CaptureViewSpec::new(
                "beauty-overhead",
                "Overhead",
                CapturePose::Overhead,
                45.0,
                200,
            ),
            CaptureViewSpec::new(
                "forest-floor-debris-detail",
                "Debris",
                CapturePose::Debris,
                45.0,
                150,
            ),
        ];
        let requested = vec![
            "beauty-overhead".into(),
            "forest-floor-debris-detail".into(),
        ];
        let mut captures = vec![
            capture("beauty-overhead", 200, 100),
            capture("forest-floor-debris-detail", 150, 60),
        ];
        let mut validation = passing_validation();
        finalize_screenshot_validation(
            "flat-dry-grassland",
            "semantic",
            &requested,
            &mut validation,
            &captures,
            &views,
        );
        assert!(validation.all_views_render_content);
        assert!(validation.requested_detail_targets_available);
        assert!(validation.foliage_detail_present);
        assert!(validation.passed);

        captures[1].detail_pixel_bps = 59;
        let mut validation = passing_validation();
        finalize_screenshot_validation(
            "flat-dry-grassland",
            "semantic",
            &requested,
            &mut validation,
            &captures,
            &views,
        );
        assert!(!validation.requested_detail_targets_available);
        assert!(!validation.passed);
    }
}
