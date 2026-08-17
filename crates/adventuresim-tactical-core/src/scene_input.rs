//! Versioned, bounded input for deterministic tactical scene generation.
//!
//! This is deliberately data-only. Production dispatchers can sample the
//! imported terrain pack into it, while tactical-only tools can serialize a
//! synthetic fixture. Short-lived servers consume the identical format and
//! never need access to the continental source pack.

use std::{fs, path::Path};

use adventuresim_core::weather::{Precipitation, WEATHER_RULES_VERSION, WeatherSnapshot};
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::scene::{GroundCover, GroundSubstrate, GroundSurface, SceneGround, SceneTerrain};

pub const TACTICAL_SCENE_SCHEMA_VERSION: u16 = 2;
pub const TACTICAL_SCENE_GENERATION_VERSION: u16 = 78;
pub const MAX_SCENE_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const TREE_TRUNK_RADIUS_METRES: f32 = 0.35;
pub const TREE_TRUNK_HEIGHT_METRES: f32 = 5.0;
/// Conservative ground footprint of the generated English-oak crown.
pub const TREE_CANOPY_GROUND_RADIUS_METRES: f32 = 5.75;
/// The trunk base is reliably leaf-covered; the outer crown uses a tapered
/// mosaic so sparse woodland does not stamp grass-free canopy discs.
const TREE_DENSE_LEAF_LITTER_RADIUS_METRES: f32 = 2.25;
pub const ROCK_RADIUS_METRES: f32 = 0.75;
const MAX_PLAYABLE_SIDE: usize = 601;
const MAX_VISTA_LEVELS: usize = 8;
const MAX_VISTA_SAMPLES: usize = 2_000_000;
const MAX_TEMPLATE_BYTES: usize = 128;
const MAX_SOURCE_ID_BYTES: usize = 128;
const MAX_PLAYABLE_GRADE: f32 = 0.65;
pub const MAX_TERRAIN_PATCH_SAMPLES: usize = 524_288;
pub const MAX_TERRAIN_PATCH_TRIANGLES: usize = 100_000;
const MAX_TERRAIN_PATCHES: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "snake_case",
    tag = "kind",
    content = "id",
    deny_unknown_fields
)]
pub enum SceneSource {
    ImportedPackage(String),
    SyntheticFixture(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentalSample {
    pub canopy_bps: u16,
    pub wetland_bps: u16,
    pub cultivation_bps: u16,
    pub water_bps: u16,
    pub hilly_bps: u16,
    pub crossing_bps: u16,
    pub surface: TacticalSurface,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalSurface {
    Road,
    #[default]
    Open,
    SparseWoods,
    DeepWoods,
    Water,
    Wetland,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSampleGrid {
    /// Vertex dimensions; samples are row-major with X varying fastest.
    pub width: u16,
    pub depth: u16,
    pub spacing_metres: f32,
    /// Relative metres around the tactical origin.
    pub heights_metres: Vec<f32>,
    /// One environment sample per height vertex.
    pub environment: Vec<EnvironmentalSample>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VistaLod {
    pub level: u8,
    pub spacing_metres: f32,
    pub width: u16,
    pub depth: u16,
    pub origin_east_metres: f64,
    pub origin_north_metres: f64,
    pub heights_metres: Vec<f32>,
    pub environment: Vec<EnvironmentalSample>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VistaSample {
    pub lods: Vec<VistaLod>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalSceneInput {
    pub schema_version: u16,
    pub generation_version: u16,
    pub seed: u64,
    pub scene_key: String,
    pub source: SceneSource,
    pub latitude_microdegrees: i32,
    pub longitude_microdegrees: i32,
    pub absolute_minute: u64,
    pub absolute_elevation_metres: i16,
    pub playable: TerrainSampleGrid,
    /// Compact authoritative landforms whose intended surface may not be a
    /// single-valued height function. Render meshes are never serialized.
    pub terrain_patches: Vec<TerrainPatchRecipe>,
    pub vista: VistaSample,
    pub weather: WeatherSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainRepresentation {
    Heightfield,
    ImplicitSurface,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandformRepresentability {
    pub representation: TerrainRepresentation,
    pub vertical_intersections: u8,
    pub heightfield_error_cm: u16,
    pub error_tolerance_cm: u16,
    pub sample_counts: [u16; 3],
    pub sample_count: u32,
    pub maximum_triangles: u32,
}

/// Heightfield fidelity is independent from traversal grade: a resolved steep
/// plane remains a heightfield, while an undercut is necessarily implicit.
pub fn classify_landform(
    vertical_intersections: u8,
    heightfield_error_cm: u16,
    error_tolerance_cm: u16,
    sample_counts: [u16; 3],
) -> Option<LandformRepresentability> {
    let sample_count = sample_counts
        .into_iter()
        .try_fold(1usize, |count, side| count.checked_mul(usize::from(side)))?;
    if sample_counts.into_iter().any(|side| side < 2) || sample_count > MAX_TERRAIN_PATCH_SAMPLES {
        return None;
    }
    let cell_count = sample_counts.into_iter().try_fold(1usize, |count, side| {
        count.checked_mul(usize::from(side - 1))
    })?;
    let maximum_triangles = cell_count
        .saturating_mul(6)
        .min(MAX_TERRAIN_PATCH_TRIANGLES);
    Some(LandformRepresentability {
        representation: if vertical_intersections > 1 || heightfield_error_cm > error_tolerance_cm {
            TerrainRepresentation::ImplicitSurface
        } else {
            TerrainRepresentation::Heightfield
        },
        vertical_intersections,
        heightfield_error_cm,
        error_tolerance_cm,
        sample_counts,
        sample_count: sample_count as u32,
        maximum_triangles: maximum_triangles as u32,
    })
}

/// A bounded inland river bluff recipe. Coordinates are centimetres in scene
/// space; the river responsible for the cut bank lies beyond the front edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiverBluffRecipe {
    pub seed: u64,
    /// Centre of the lower bench at the bluff face.
    pub center_cm: [i32; 3],
    pub yaw_milliradians: i16,
    pub face_width_cm: u16,
    pub face_height_cm: u16,
    pub rock_depth_cm: u16,
    pub curvature_cm: u16,
    pub undercut_depth_cm: u16,
    pub collapse_offset_cm: i16,
    pub collapse_radius_cm: u16,
    pub talus_depth_cm: u16,
    pub heightfield_error_cm: u16,
    pub error_tolerance_cm: u16,
    pub vertical_intersections: u8,
    pub sample_spacing_cm: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
#[serde(rename_all = "snake_case", tag = "kind", content = "recipe")]
pub enum TerrainPatchRecipe {
    RiverBluff(RiverBluffRecipe),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainPatchProxyBox {
    pub center: bevy::math::Vec3,
    pub half_extents: bevy::math::Vec3,
    pub yaw_radians: f32,
}

impl RiverBluffRecipe {
    pub fn dimensions_metres(self) -> bevy::math::Vec3 {
        bevy::math::Vec3::new(
            f32::from(self.face_width_cm),
            f32::from(self.face_height_cm),
            f32::from(self.rock_depth_cm),
        ) / 100.0
    }

    pub fn center_metres(self) -> bevy::math::Vec3 {
        bevy::math::Vec3::new(
            self.center_cm[0] as f32,
            self.center_cm[1] as f32,
            self.center_cm[2] as f32,
        ) / 100.0
    }

    pub fn yaw_radians(self) -> f32 {
        f32::from(self.yaw_milliradians) / 1_000.0
    }

    pub fn sample_counts(self) -> [u16; 3] {
        let [minimum_x, maximum_x, minimum_z, maximum_z] = self.implicit_tile_bounds_local();
        let lateral_spacing_cm = self.sample_spacing_cm.saturating_mul(5).div_ceil(4);
        let x_count =
            (((maximum_x - minimum_x) * 100.0).ceil() as u16).div_ceil(lateral_spacing_cm) + 1;
        let y_count = (self.face_height_cm + 200).div_ceil(self.sample_spacing_cm) + 1;
        let desired_z =
            (((maximum_z - minimum_z) * 100.0).ceil() as u16).div_ceil(self.sample_spacing_cm) + 1;
        let affordable_z = (MAX_TERRAIN_PATCH_SAMPLES
            / (usize::from(x_count) * usize::from(y_count)))
        .min(usize::from(u16::MAX)) as u16;
        [x_count, y_count, desired_z.min(affordable_z).max(2)]
    }

    /// Distant, axis-aligned local bounds of the continuous implicit terrain tile. Every edge is
    /// ordinary single-valued ground; the client samples the replicated heightfield there and the
    /// regular terrain renderer resumes on the same boundary.
    pub fn implicit_tile_bounds_local(self) -> [f32; 4] {
        let size = self.dimensions_metres();
        let lateral = size.x * 0.5 + 6.0;
        let front = -(f32::from(self.talus_depth_cm) / 100.0) - 5.0;
        // The weathered flanks retreat several metres behind the localized collapse face. Keep
        // the tile's distant rear edge beyond the full terrain-inheritance interval so the
        // extracted scalar is ordinary single-valued ground before reaching the perimeter.
        let back = size.z + 16.0;
        [-lateral, lateral, front, back]
    }

    pub fn implicit_tile_contains_local_xz(self, local_x: f32, local_z: f32) -> bool {
        let [minimum_x, maximum_x, minimum_z, maximum_z] = self.implicit_tile_bounds_local();
        (minimum_x..=maximum_x).contains(&local_x) && (minimum_z..=maximum_z).contains(&local_z)
    }

    pub fn representability(self) -> Option<LandformRepresentability> {
        classify_landform(
            self.vertical_intersections,
            self.heightfield_error_cm,
            self.error_tolerance_cm,
            self.sample_counts(),
        )
    }

    fn top_front_local_z(self, local_x: f32) -> f32 {
        let size = self.dimensions_metres();
        let across = local_x / (size.x * 0.5);
        let concavity =
            f32::from(self.curvature_cm) / 100.0 * (1.0 - across.powi(2)).clamp(0.0, 1.0);
        let asymmetric_sweep =
            across * 0.85 + (across * core::f32::consts::PI * 1.35 + 0.25).sin() * 0.62;
        let end_return = size.z * 0.68 * smoothstep(((across.abs() - 0.68) / 0.32).clamp(0.0, 1.0));
        concavity + asymmetric_sweep + end_return
    }

    /// Local inherited crest shared by the implicit scarp and heightfield.
    pub fn local_crest_height(self, local_x: f32) -> f32 {
        let size = self.dimensions_metres();
        let across = local_x / (size.x * 0.5);
        // Keep the collision-owned central sector coherent, then return the crest over the
        // remaining eight metres. Starting the taper inside that sector made adjacent coarse
        // heightfield columns inherit different terrace elevations at the ownership handoff.
        let end_taper = smoothstep(((across.abs() - 0.44) / 0.44).clamp(0.0, 1.0));
        let broad_crest = size.y * (1.0 - end_taper * 0.91)
            + (across * core::f32::consts::PI * 1.20 + 0.4).sin() * 0.38
            + (across * core::f32::consts::PI * 2.70 - 0.8).sin() * (0.12 + end_taper * 0.20);
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let collapse_radius = f32::from(self.collapse_radius_cm) / 100.0;
        let notch_x = (local_x - collapse_center) / collapse_radius;
        let notch_distance = if notch_x < 0.0 {
            (-notch_x / 0.72).powi(2)
        } else {
            (notch_x / 1.08).powi(2)
        };
        let notch = 1.0 - smoothstep(((notch_distance - 0.08) / 0.92).clamp(0.0, 1.0));
        let finite_side_closure = 1.0 - smoothstep(((across.abs() - 0.80) / 0.20).clamp(0.0, 1.0));
        (broad_crest - notch * (0.24 + notch_x.clamp(-0.5, 0.7) * 0.06)).max(0.0)
            * finite_side_closure
    }

    /// Half-width of the collapse-centred rock support and implicit collision.
    /// Stable flanks are ordinary heightfield terrain, not a sandstone facade.
    pub fn implicit_collision_half_width(self) -> f32 {
        3.5
    }

    pub fn rock_support_bounds_local(self) -> [f32; 2] {
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let half_width = self.implicit_collision_half_width();
        [collapse_center - half_width, collapse_center + half_width]
    }

    /// C1 lateral support shared by client geometry/material, collision, and queries.
    /// The 4.8-metre core contains the scar and undercut; the final metre fades over more than
    /// three face samples before the scalar becomes exactly authoritative terrain.
    pub fn rock_support_weight_local(self, local_x: f32) -> f32 {
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let distance = (local_x - collapse_center).abs();
        1.0 - smoothstep(((distance - 2.4) / 1.1).clamp(0.0, 1.0))
    }

    /// Authored face position at the local crest, shared by the implicit
    /// return and its heightfield-owned continuation.
    pub fn crest_brink_local_z(self, local_x: f32) -> f32 {
        self.face_surface_local_z(bevy::math::Vec3::new(
            local_x,
            self.local_crest_height(local_x),
            0.0,
        ))
    }

    fn undercut_collision_clearance_height(self) -> f32 {
        f32::from(self.undercut_depth_cm) / 100.0
    }

    /// Narrow, sheared toe-undercut weight shared by geometry, collision, and
    /// presentation. The opening is a shallow, asymmetrically feathered bed
    /// recess rather than a cave mouth; its irregular roof follows the lowest
    /// resistant-bed zone.
    pub fn undercut_weight_local(self, local: bevy::math::Vec3) -> f32 {
        if local.y < 0.0 {
            return 0.0;
        }
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let sheared_center = collapse_center
            + (local.y - 0.45) * 0.14
            + (local.y * 2.7 + collapse_center * 0.3).sin() * 0.08;
        let half_width = 1.48 + (local.y * 1.9 + self.seed as f32 * 0.001).sin() * 0.08;
        let lateral_position = (local.x - sheared_center) / half_width;
        let lateral = if lateral_position < 0.0 {
            1.0 - smoothstep((((-lateral_position) - 0.20) / 0.80).clamp(0.0, 1.0))
        } else {
            1.0 - smoothstep(((lateral_position - 0.26) / 0.58).clamp(0.0, 1.0))
        };
        let roof_x = local.x - collapse_center;
        let roof = (0.34
            + (-(roof_x / 0.72).powi(2)).exp() * 0.34
            + (roof_x * 1.35).sin() * 0.055
            + (roof_x * 2.4 + 0.6).sin() * 0.025)
            .clamp(0.30, 0.76);
        let vertical = 1.0 - smoothstep(((local.y / roof - 0.55) / 0.45).clamp(0.0, 1.0));
        lateral * vertical
    }

    /// Broad, joint-controlled weight for one continuous recessed failure
    /// plane. Its transitions deliberately span more than two scalar samples;
    /// sub-grid groove geometry aliases into holes under Surface Nets.
    /// Deterministic height contributed by the aggregated collapsed-stone fan.
    /// The ordinary terrain beneath it remains authoritative and is not
    /// flattened merely to expose the debris-free undercut flank.
    pub fn debris_fan_height_local(self, local_x: f32, local_z: f32) -> f32 {
        let collapse_x = f32::from(self.collapse_offset_cm) / 100.0;
        let talus_depth = f32::from(self.talus_depth_cm) / 100.0;
        let talus_toe_z = self.talus_toe_local_z();
        let toward_face = ((local_z - (talus_toe_z - talus_depth)) / talus_depth).clamp(0.0, 1.0);
        let across_fan = local_x - collapse_x;
        let ridge = 1.00 * (-(((across_fan + 4.4 + toward_face * 0.25) / 1.55).powi(2))).exp()
            + 0.92 * (-(((across_fan - 0.10 + toward_face * 0.18) / 1.45).powi(2))).exp()
            + 0.78 * (-(((across_fan - 4.5 - toward_face * 0.22) / 1.60).powi(2))).exp();
        // Put the aggregated mass clearly downslope of the intact toe. Besides matching how a
        // collapse spreads, this keeps its upper tail below the visible-scarp ownership band.
        let downslope_center = talus_toe_z - talus_depth * 0.55;
        let longitudinal = (-(((local_z - downslope_center) / (talus_depth * 0.42)).powi(2))).exp();
        let clear_flank = 1.0
            - (1.0 - smoothstep((((across_fan - 0.35).abs() - 0.35) / 0.50).clamp(0.0, 1.0)))
                * smoothstep(((toward_face - 0.42) / 0.30).clamp(0.0, 1.0));
        let face_clearance = 1.0 - smoothstep(((toward_face - 0.52) / 0.32).clamp(0.0, 1.0));
        // Gaussian tails settle into the inherited floodplain instead of ending at a compact
        // polygon. The old zero contour exposed the last sloped heightfield triangle as a dark
        // line around the deposit.
        let height = (longitudinal * ridge * clear_flank * face_clearance * 1.20).min(1.80);
        if height < 0.001 { 0.0 } else { height }
    }

    pub fn failure_scar_weight(self, local: bevy::math::Vec3) -> f32 {
        let size = self.dimensions_metres();
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let collapse_radius = f32::from(self.collapse_radius_cm) / 100.0;
        let scar_x = local.x - collapse_center;
        // The failure is one metre-scale sheared polygon, not a symmetric
        // decal: its sides diverge independently and its release plane points
        // obliquely toward the debris pile.
        let scar_bottom = if scar_x < -0.60 {
            size.y * 0.39 - (scar_x + 0.60) * 0.08
        } else if scar_x < 1.00 {
            size.y * 0.39 + (scar_x + 0.60) * 0.22
        } else {
            size.y * 0.39 + 0.352 - (scar_x - 1.00) * 0.06
        };
        let vertical = ((local.y - scar_bottom) / (size.y * 0.54)).clamp(0.0, 1.0);
        let piecewise = |value: f32, vertices: [f32; 4]| {
            let scaled = value.clamp(0.0, 1.0) * 3.0;
            let segment = (scaled.floor() as usize).min(2);
            let fraction = scaled - segment as f32;
            vertices[segment] + (vertices[segment + 1] - vertices[segment]) * fraction
        };
        // Four landform-scale fracture vertices per side create a single
        // angular missing wedge. The sides shear independently and change
        // direction only at these authored release vertices.
        let left_edge = piecewise(
            vertical,
            [
                -collapse_radius * 0.05,
                -collapse_radius * 0.28,
                -collapse_radius * 0.44,
                -collapse_radius * 0.65,
            ],
        );
        let right_edge = piecewise(
            vertical,
            [
                collapse_radius * 0.15,
                collapse_radius * 0.36,
                collapse_radius * 0.58,
                collapse_radius * 0.92,
            ],
        );
        let side_weight = smoothstep(((scar_x - left_edge) / 1.05).clamp(0.0, 1.0))
            .clamp(0.0, 1.0)
            .min(smoothstep(((right_edge - scar_x) / 1.05).clamp(0.0, 1.0)));
        let bottom_weight = smoothstep(((local.y - scar_bottom) / 0.72).clamp(0.0, 1.0));
        let crest_weight =
            ((self.local_crest_height(local.x) + 0.55 - local.y) / 0.80).clamp(0.0, 1.0);
        side_weight.min(bottom_weight).min(crest_weight)
    }

    fn failure_recess_local_z(self, point: bevy::math::Vec3) -> f32 {
        let scar_weight = self.failure_scar_weight(point);
        let facet_a = smoothstep(((point.x + point.y * 0.24 + 0.8) / 1.4).clamp(0.0, 1.0));
        let facet_b = smoothstep(((-point.x * 0.55 + point.y * 0.18 - 0.2) / 1.2).clamp(0.0, 1.0));
        let failure_facets = ((facet_a - 0.5) * 0.06 + (facet_b - 0.5) * 0.04) * scar_weight;
        let failure_rim = (-((scar_weight - 0.30) / 0.24).powi(2)).exp() * 0.04;
        scar_weight * 0.45 + failure_facets - failure_rim
    }

    pub fn world_to_local(self, world: bevy::math::Vec3) -> bevy::math::Vec3 {
        let relative = world - self.center_metres();
        let (sin, cos) = self.yaw_radians().sin_cos();
        bevy::math::Vec3::new(
            relative.x * cos - relative.z * sin,
            relative.y,
            relative.x * sin + relative.z * cos,
        )
    }

    pub fn local_to_world(self, local: bevy::math::Vec3) -> bevy::math::Vec3 {
        let (sin, cos) = self.yaw_radians().sin_cos();
        self.center_metres()
            + bevy::math::Vec3::new(
                local.x * cos + local.z * sin,
                local.y,
                -local.x * sin + local.z * cos,
            )
    }

    /// Front edge of the localized debris apron. The toe follows the intact
    /// lip rather than the recessed undercut surface so loose-stone semantics
    /// cannot leak rearward onto the upper brink.
    pub fn talus_toe_local_z(self) -> f32 {
        let collapse_x = f32::from(self.collapse_offset_cm) / 100.0;
        let recessed_toe =
            self.face_surface_local_z(bevy::math::Vec3::new(collapse_x, 0.25, 0.0)) - 0.35;
        let intact_lip = self.face_surface_local_z(bevy::math::Vec3::new(
            collapse_x,
            self.dimensions_metres().y * 0.40,
            0.0,
        )) - 0.15;
        recessed_toe.min(intact_lip)
    }

    /// Signed structural bed displacement: positive values recess weak
    /// interbeds and negative values project resistant sandstone beds.
    pub fn bedding_displacement_local_z(self, point: bevy::math::Vec3) -> f32 {
        let size = self.dimensions_metres();
        let height = size.y;
        let across = (point.x / (size.x * 0.5)).clamp(-1.5, 1.5);
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let collapse_radius = f32::from(self.collapse_radius_cm) / 100.0;
        let release_blend = smoothstep(
            ((point.x - (collapse_center - collapse_radius * 0.45)) / (collapse_radius * 0.90))
                .clamp(0.0, 1.0),
        );
        let end_fade = 1.0 - smoothstep(((across.abs() - 0.46) / 0.26).clamp(0.0, 1.0));
        let weak_relative_height = 0.46_f32;
        let weak_release_offset = -0.08 + release_blend * 0.20;
        let weak_warp = (point.x * 0.09 + weak_relative_height * 5.0).sin() * 0.12
            + (point.x * 0.18 - weak_relative_height * 3.0).sin() * 0.04;
        let weak_coherence = 0.82
            + (point.x * 0.16 + weak_relative_height * 7.0).sin() * 0.10
            + (point.x * 0.31 - weak_relative_height * 2.0).sin() * 0.05;
        let weak_recess = smooth_course_weight(
            point.y,
            height * weak_relative_height + weak_release_offset + weak_warp,
            0.42,
            1.0,
        ) * 0.60
            * weak_coherence
            * end_fade;
        let resistant_relative_height = 0.78_f32;
        let resistant_release_offset = -0.07 + release_blend * 0.17;
        let resistant_warp = (point.x * 0.085 + resistant_relative_height * 4.0).sin() * 0.13
            + (point.x * 0.17 + resistant_relative_height).sin() * 0.04;
        let resistant_coherence = 0.86
            + (point.x * 0.14 + resistant_relative_height * 6.0).sin() * 0.09
            + (point.x * 0.27 - resistant_relative_height * 4.0).sin() * 0.04;
        let resistant_projection = smooth_course_weight(
            point.y,
            height * resistant_relative_height + resistant_release_offset + resistant_warp,
            0.42,
            1.0,
        ) * 0.85
            * resistant_coherence
            * end_fade;
        let scar_attenuation = 1.0 - self.failure_scar_weight(point) * 0.82;
        (weak_recess - resistant_projection) * scar_attenuation
    }

    /// Metre-scale toe-to-crest retreat of the natural scarp macroform.
    ///
    /// Most of a river bluff is a steep but heightfield-readable weathered slope. Only the
    /// joint-bounded collapse sector retains a near-vertical rock face and shallow overhang. The
    /// retreat scales with the inherited local crest so the returned shoulders die into terrain
    /// instead of ending as a constant-height wall.
    fn collapse_sector_weight_local(self, local_x: f32) -> f32 {
        let collapse_center = f32::from(self.collapse_offset_cm) / 100.0;
        let collapse_distance = (local_x - collapse_center).abs();
        1.0 - smoothstep(((collapse_distance - 2.0) / 2.0).clamp(0.0, 1.0))
    }

    pub fn scarp_horizontal_run_local(self, local_x: f32) -> f32 {
        let height = self.dimensions_metres().y;
        let crest = self.local_crest_height(local_x);
        let crest_fraction = (crest / height).clamp(0.0, 1.0);
        let collapse_sector = self.collapse_sector_weight_local(local_x);
        let asymmetric_run = crest * 0.46
            + 0.58
            + (local_x * 0.19 + 0.35).sin() * 0.42
            + (local_x * 0.07 - 0.8).sin() * 0.24;
        asymmetric_run.clamp(5.2, 6.6)
            * (0.45 + crest_fraction * 0.55)
            * (1.0 - collapse_sector * 0.82)
    }

    fn scarp_retreat_local_z(self, point: bevy::math::Vec3) -> f32 {
        let crest = self.local_crest_height(point.x);
        if crest <= f32::EPSILON {
            return 0.0;
        }
        let vertical = (point.y / crest).clamp(0.0, 1.0);
        let broad_profile = vertical * 0.72 + smoothstep(vertical) * 0.28;
        let collapse_profile =
            smoothstep(((point.y - 1.55) / (crest - 1.55).max(0.5)).clamp(0.0, 1.0));
        let collapse_sector = self.collapse_sector_weight_local(point.x);
        let profile = broad_profile * (1.0 - collapse_sector) + collapse_profile * collapse_sector;
        let asymmetric_bow = (vertical * core::f32::consts::PI).sin()
            * ((point.x * 0.16 + 0.4).sin() * 0.22 + (point.x * 0.07 - 0.6).sin() * 0.12);
        self.scarp_horizontal_run_local(point.x) * profile + asymmetric_bow
    }

    /// Authored front surface in patch-local coordinates.
    ///
    /// The client and collision proxy use this same equation for the exposed
    /// multi-valued scarp inside the continuous terrain tile. Keeping the
    /// evaluator here prevents presentation or collision from guessing the
    /// authored surface from triangle normals on a deliberately curved face.
    pub fn face_surface_local_z(self, point: bevy::math::Vec3) -> f32 {
        let size = self.dimensions_metres();
        let half_width = size.x * 0.5;
        let height = size.y;
        let depth = size.z;
        let across = (point.x / half_width).clamp(-1.5, 1.5);
        let curve = self.top_front_local_z(point.x).min(depth - 0.60);
        let vertical = (point.y / height).clamp(0.0, 1.0);
        let undercut =
            f32::from(self.undercut_depth_cm) / 100.0 * self.undercut_weight_local(point);
        let phase = (self.seed >> 40) as f32 / ((1_u32 << 24) - 1) as f32 * core::f32::consts::TAU;
        let molded_relief = ((point.x * 1.31 + point.y * 0.73 + phase).sin() * 0.65
            + (point.x * 0.47 - point.y * 1.67 - phase * 0.7).sin() * 0.35)
            * 0.05;
        let detail_fade = 1.0 - smoothstep(((across.abs() - 0.46) / 0.26).clamp(0.0, 1.0));
        let face_undulation = ((vertical * core::f32::consts::PI * 0.86 + across * 1.55).sin()
            * 0.78
            + (vertical * core::f32::consts::PI * 1.48 - across * 0.68).sin() * 0.40)
            * detail_fade;
        // The collapse is a shallow release plane cut into one continuous
        // scarp, not a boolean slot dividing the mass into two lobes.
        let failure_recess = self.failure_recess_local_z(point);
        let bedding_displacement = self.bedding_displacement_local_z(point);
        let undercut_lateral = self
            .undercut_weight_local(bevy::math::Vec3::new(point.x, 0.45, 0.0))
            .clamp(0.0, 1.0);
        let lip_distance = (point.y - 1.42) / 0.30;
        let resistant_toe_lip = (-lip_distance * lip_distance).exp() * 0.56 * undercut_lateral;
        curve
            + self.scarp_retreat_local_z(point)
            + face_undulation
            + failure_recess
            + undercut
            + bedding_displacement
            + molded_relief
            - resistant_toe_lip
    }

    /// Conservative backmost extent of the authored face in one local-x
    /// column. Heightfield ownership begins behind this envelope, never at the
    /// nominal plan curve, because the undercut, broad failure plane, bedding,
    /// and metre-scale undulation can all recess the visible face.
    pub fn maximum_face_local_z(self, local_x: f32) -> f32 {
        const VERTICAL_SAMPLES: u16 = 64;
        let top = self.local_crest_height(local_x);
        let mut maximum = f32::NEG_INFINITY;
        for index in 0..=VERTICAL_SAMPLES {
            let y = top * f32::from(index) / f32::from(VERTICAL_SAMPLES);
            maximum =
                maximum.max(self.face_surface_local_z(bevy::math::Vec3::new(local_x, y, 0.0)));
        }
        // The fixed sampling is part of the compact authoritative recipe. A
        // generous sub-metre guard covers extrema between samples and remains
        // small relative to the authored rock depth.
        maximum + 0.75
    }

    /// Shared rear edge of the fully authored terrace cap. A single
    /// conservative value across the bluff prevents neighbouring heightfield
    /// columns from beginning their rear convergence at different depths and
    /// forming a lateral wall.
    pub fn rear_terrace_convergence_start_local_z(self) -> f32 {
        const LATERAL_SAMPLES: u16 = 28;
        let half_width = self.dimensions_metres().x * 0.5;
        (0..=LATERAL_SAMPLES)
            .map(|index| {
                let x =
                    -half_width + half_width * 2.0 * f32::from(index) / f32::from(LATERAL_SAMPLES);
                self.maximum_face_local_z(x)
            })
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// Monotone rear inheritance shared by authoritative heightfield collision
    /// and the client terrain-solid cap. Its softened-linear profile keeps the
    /// maximum grade below a cubic smoothstep while avoiding a hard height
    /// discontinuity at either end.
    pub fn rear_terrace_inheritance(self, local_z: f32, convergence_start: f32) -> f32 {
        let t = ((local_z - convergence_start) / 11.5).clamp(0.0, 1.0);
        let shoulder = t * (1.0 - t) * (1.0 - 2.0 * t);
        t - shoulder * 0.5
    }

    /// Conservative frontmost face extent used by the static proxy and its
    /// diagnostic overlay.
    pub fn minimum_face_local_z(self, local_x: f32) -> f32 {
        const VERTICAL_SAMPLES: u16 = 64;
        let top = self.local_crest_height(local_x);
        let mut minimum = f32::INFINITY;
        for index in 0..=VERTICAL_SAMPLES {
            let y = top * f32::from(index) / f32::from(VERTICAL_SAMPLES);
            minimum =
                minimum.min(self.face_surface_local_z(bevy::math::Vec3::new(local_x, y, 0.0)));
        }
        minimum - 0.25
    }

    /// Shared terrain-solid scalar field. Negative values are the terrace mass.
    ///
    /// This is deliberately unbounded downward and rearward. The upper surface
    /// is the authored crest and the front surface is the multi-valued scarp;
    /// their intersection produces a real terrace edge without a finite box
    /// bottom, back, or side closure. The client unions this mass with the
    /// authoritative heightfield, which already converges to the same crest at
    /// the distant implicit-tile boundary.
    pub fn signed_distance(self, world: bevy::math::Vec3) -> f32 {
        let point = self.world_to_local(world);
        (point.y - self.local_crest_height(point.x)).max(self.face_surface_local_z(point) - point.z)
    }

    fn collision_proxy_band(
        self,
        slice_start: f32,
        slice_end: f32,
        bottom: f32,
        top: f32,
    ) -> Option<TerrainPatchProxyBox> {
        let size = self.dimensions_metres();
        let x = (slice_start + slice_end) * 0.5;
        let y = (bottom + top) * 0.5;
        let sample_xs: [f32; 9] = core::array::from_fn(|sample| {
            slice_start + (slice_end - slice_start) * sample as f32 / 8.0
        });
        let sample_ys: [f32; 9] =
            core::array::from_fn(|sample| bottom + (top - bottom) * sample as f32 / 8.0);
        let intersects_failure = sample_xs.into_iter().any(|sample_x| {
            sample_ys.into_iter().any(|sample_y| {
                self.failure_scar_weight(bevy::math::Vec3::new(sample_x, sample_y, 0.0)) > 0.08
            })
        });
        let intersects_undercut = sample_xs.into_iter().any(|sample_x| {
            sample_ys.into_iter().any(|sample_y| {
                self.undercut_weight_local(bevy::math::Vec3::new(sample_x, sample_y, 0.0)) > 0.08
            })
        });
        if intersects_failure || intersects_undercut {
            return None;
        }
        let (frontmost, rearmost) = sample_xs
            .into_iter()
            .flat_map(|sample_x| {
                sample_ys.into_iter().map(move |sample_y| {
                    self.face_surface_local_z(bevy::math::Vec3::new(sample_x, sample_y, 0.0))
                })
            })
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(frontmost, rearmost), face| (frontmost.min(face), rearmost.max(face)),
            );
        if rearmost - frontmost > 0.48 {
            return None;
        }
        let front = rearmost + 0.15;
        let depth = 0.70_f32.min((size.z - front - 0.05).max(0.0));
        (depth >= 0.10).then(|| TerrainPatchProxyBox {
            center: self.local_to_world(bevy::math::Vec3::new(x, y, front + depth * 0.5)),
            half_extents: bevy::math::Vec3::new(
                (slice_end - slice_start) * 0.5 - 0.01,
                (top - bottom) * 0.5,
                depth * 0.5,
            ),
            yaw_radians: self.yaw_radians(),
        })
    }

    /// Shared bounded collision proxy, sampled into thin face-following bands.
    ///
    /// Each box begins behind the most recessed authored surface sampled in
    /// its band. Failure-scar and undercut bands are omitted, so the gameplay
    /// proxy cannot turn authored air into an enormous invisible wall.
    pub fn collision_proxy_boxes(self) -> Vec<TerrainPatchProxyBox> {
        const X_SLICES: usize = 28;
        const Y_BANDS: usize = 20;
        let size = self.dimensions_metres();
        let slice_width = size.x / X_SLICES as f32;
        let mut proxies = Vec::with_capacity(X_SLICES * Y_BANDS * 2);
        for index in 0..X_SLICES {
            let slice_start = -size.x * 0.5 + slice_width * index as f32;
            let slice_end = slice_start + slice_width;
            let x = (slice_start + slice_end) * 0.5;
            let [collision_min_x, collision_max_x] = self.rock_support_bounds_local();
            if slice_start < collision_min_x || slice_end > collision_max_x {
                // Returned shoulders are ordinary heightfield collision. Do
                // not force full-height boxes into the authored crest taper.
                continue;
            }
            let safe_crest = [slice_start, x, slice_end]
                .into_iter()
                .map(|sample_x| self.local_crest_height(sample_x))
                .fold(f32::INFINITY, f32::min);
            let center_crest = self.local_crest_height(x);
            if safe_crest < 2.8 || center_crest - safe_crest > 0.75 {
                // The sharply tapering returned shoulder is owned by the
                // heightfield. Emitting an axis-aligned face band here would
                // either miss the low edge or protrude above it.
                continue;
            }
            let slice_proxy_start = proxies.len();
            let band_height = safe_crest / Y_BANDS as f32;
            for band in 0..Y_BANDS {
                let bottom = band as f32 * band_height + 0.08;
                let top = (band + 1) as f32 * band_height - 0.08;
                if top <= bottom {
                    continue;
                }
                let clearance = self.undercut_collision_clearance_height();
                let fitted_bottom = if bottom < clearance && top > clearance {
                    clearance
                } else {
                    bottom
                };
                if let Some(proxy) =
                    self.collision_proxy_band(slice_start, slice_end, fitted_bottom, top)
                {
                    proxies.push(proxy);
                    continue;
                }
                // Preserve both sides of narrow beds and release joints by
                // fitting smaller lateral boxes. Scar and undercut sub-bands
                // remain absent because the same semantic tests are rerun for
                // every subdivision.
                let subdivision_width = slice_width / 4.0;
                for subdivision in 0..4 {
                    let sub_start = slice_start + subdivision_width * subdivision as f32;
                    let sub_end = sub_start + subdivision_width;
                    if let Some(proxy) =
                        self.collision_proxy_band(sub_start, sub_end, fitted_bottom, top)
                    {
                        proxies.push(proxy);
                    }
                }
            }
            // Bridge from the exact top of authored undercut air to the first
            // regular rock band. This lip begins at the recipe clearance, so
            // its AABB cannot occupy the empty 0..clearance interval.
            let clearance = self.undercut_collision_clearance_height();
            let lip_top = (clearance + 0.32).min(safe_crest - 0.05);
            let slice_has_undercut = [slice_start, x, slice_end].into_iter().any(|sample_x| {
                self.undercut_weight_local(bevy::math::Vec3::new(sample_x, 0.45, 0.0)) > 0.08
            });
            if slice_has_undercut && lip_top > clearance {
                if let Some(proxy) =
                    self.collision_proxy_band(slice_start, slice_end, clearance, lip_top)
                {
                    proxies.push(proxy);
                } else {
                    let subdivision_width = slice_width / 4.0;
                    for subdivision in 0..4 {
                        let sub_start = slice_start + subdivision_width * subdivision as f32;
                        let sub_end = sub_start + subdivision_width;
                        if let Some(proxy) =
                            self.collision_proxy_band(sub_start, sub_end, clearance, lip_top)
                        {
                            proxies.push(proxy);
                        }
                    }
                }
            }
            // A short overlapping cap prevents a resistant-bed transition
            // from consuming the final regular band below an otherwise solid
            // crest. It uses the identical fit and missing-volume tests, so a
            // failure-notch cap remains absent rather than filling authored
            // air.
            let cap_bottom = (safe_crest - 0.38).max(0.08);
            let cap_top = safe_crest - 0.05;
            if let Some(proxy) =
                self.collision_proxy_band(slice_start, slice_end, cap_bottom, cap_top)
            {
                proxies.push(proxy);
            } else {
                let subdivision_width = slice_width / 4.0;
                for subdivision in 0..4 {
                    let sub_start = slice_start + subdivision_width * subdivision as f32;
                    let sub_end = sub_start + subdivision_width;
                    if let Some(proxy) =
                        self.collision_proxy_band(sub_start, sub_end, cap_bottom, cap_top)
                    {
                        proxies.push(proxy);
                    }
                }
            }

            // A projecting bed or joint can reject two adjacent coarse bands
            // even though the intervening rock is solid. Detect only those
            // measured holes in this slice, then retry them as short vertical
            // bands with the same lateral subdivision and semantic air tests.
            // This preserves the explicit failure/undercut omissions while
            // keeping ordinary central collision gaps within 0.75 metres.
            let mut covered_intervals = proxies[slice_proxy_start..]
                .iter()
                .filter_map(|proxy| {
                    let local = self.world_to_local(proxy.center);
                    ((local.x - x).abs() <= proxy.half_extents.x + 0.02).then_some((
                        local.y - proxy.half_extents.y,
                        local.y + proxy.half_extents.y,
                    ))
                })
                .collect::<Vec<_>>();
            covered_intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
            let slice_has_undercut =
                self.undercut_weight_local(bevy::math::Vec3::new(x, 0.45, 0.0)) > 0.08;
            let mut covered_top = if slice_has_undercut {
                self.undercut_collision_clearance_height()
            } else {
                0.08
            };
            let mut measured_gaps = Vec::new();
            for (bottom, top) in covered_intervals {
                if bottom - covered_top > 0.75 {
                    measured_gaps.push((covered_top, bottom));
                }
                covered_top = covered_top.max(top);
            }
            for (gap_bottom, gap_top) in measured_gaps {
                let steps = ((gap_top - gap_bottom) / 0.28).ceil() as usize;
                for step in 0..steps {
                    let bottom = gap_bottom + (gap_top - gap_bottom) * step as f32 / steps as f32;
                    let top =
                        gap_bottom + (gap_top - gap_bottom) * (step + 1) as f32 / steps as f32;
                    if let Some(proxy) =
                        self.collision_proxy_band(slice_start, slice_end, bottom, top)
                    {
                        proxies.push(proxy);
                        continue;
                    }
                    // At a sheared failure margin, a quarter-slice can straddle authored air
                    // even though one side of the measured gap is intact rock. Eighth-slices
                    // fit up to that boundary without filling the missing scar volume.
                    let subdivision_width = slice_width / 8.0;
                    for subdivision in 0..8 {
                        let sub_start = slice_start + subdivision_width * subdivision as f32;
                        let sub_end = sub_start + subdivision_width;
                        if let Some(proxy) =
                            self.collision_proxy_band(sub_start, sub_end, bottom, top)
                        {
                            proxies.push(proxy);
                        }
                    }
                }
            }
            if safe_crest - covered_top > 0.75 {
                // The earlier landform-scale crest taper can put a resistant
                // bed across the final coarse cap band near the central/
                // returned-shoulder ownership boundary. Refit only that
                // measured solid crest gap with short, narrow boxes. The
                // shared scar/undercut predicates still reject authored air.
                let crest_bottom = covered_top;
                let crest_top = safe_crest - 0.05;
                let steps = ((crest_top - crest_bottom) / 0.22).ceil() as usize;
                for step in 0..steps {
                    let bottom =
                        crest_bottom + (crest_top - crest_bottom) * step as f32 / steps as f32;
                    let top = crest_bottom
                        + (crest_top - crest_bottom) * (step + 1) as f32 / steps as f32;
                    if let Some(proxy) =
                        self.collision_proxy_band(slice_start, slice_end, bottom, top)
                    {
                        proxies.push(proxy);
                        continue;
                    }
                    let subdivision_width = slice_width / 8.0;
                    for subdivision in 0..8 {
                        let sub_start = slice_start + subdivision_width * subdivision as f32;
                        let sub_end = sub_start + subdivision_width;
                        if let Some(proxy) =
                            self.collision_proxy_band(sub_start, sub_end, bottom, top)
                        {
                            proxies.push(proxy);
                        }
                    }
                }
            }

            // A parent one-metre slice can straddle the start of the returned
            // crest taper. Its low outer edge is the correct safe height for
            // full-width boxes, but must not truncate a retained inner
            // quarter-slice whose own complete footprint remains taller.
            // Fit those local caps against each quarter's conservative crest;
            // outside the explicit collision half-width remains exclusively
            // heightfield-owned.
            let quarter_width = slice_width / 4.0;
            for quarter in 0..4 {
                let quarter_start = slice_start + quarter_width * quarter as f32;
                let quarter_end = quarter_start + quarter_width;
                let quarter_center = (quarter_start + quarter_end) * 0.5;
                let quarter_safe_crest = [quarter_start, quarter_center, quarter_end]
                    .into_iter()
                    .map(|sample_x| self.local_crest_height(sample_x))
                    .fold(f32::INFINITY, f32::min);
                let quarter_top = proxies[slice_proxy_start..]
                    .iter()
                    .filter_map(|proxy| {
                        let local = self.world_to_local(proxy.center);
                        ((local.x - quarter_center).abs() <= proxy.half_extents.x + 0.02)
                            .then_some(local.y + proxy.half_extents.y)
                    })
                    .fold(f32::NEG_INFINITY, f32::max);
                if !quarter_top.is_finite() || quarter_safe_crest - quarter_top <= 0.75 {
                    continue;
                }
                let cap_top = quarter_safe_crest - 0.05;
                let steps = ((cap_top - quarter_top) / 0.22).ceil() as usize;
                for step in 0..steps {
                    let bottom = quarter_top + (cap_top - quarter_top) * step as f32 / steps as f32;
                    let top =
                        quarter_top + (cap_top - quarter_top) * (step + 1) as f32 / steps as f32;
                    if let Some(proxy) =
                        self.collision_proxy_band(quarter_start, quarter_end, bottom, top)
                    {
                        proxies.push(proxy);
                        continue;
                    }
                    let eighth_width = quarter_width * 0.5;
                    for eighth in 0..2 {
                        let eighth_start = quarter_start + eighth_width * eighth as f32;
                        let eighth_end = eighth_start + eighth_width;
                        if let Some(proxy) =
                            self.collision_proxy_band(eighth_start, eighth_end, bottom, top)
                        {
                            proxies.push(proxy);
                        }
                    }
                }
            }
        }
        proxies
    }

    pub fn upper_surface_below(self, world: bevy::math::Vec3) -> Option<f32> {
        let local = self.world_to_local(world);
        let size = self.dimensions_metres();
        let inside =
            self.rock_support_weight_local(local.x) > 0.0 && (0.0..=size.z).contains(&local.z);
        let upper = self.center_metres().y + self.local_crest_height(local.x);
        (inside && world.y >= upper).then_some(upper)
    }
}

impl TerrainPatchRecipe {
    pub fn representability(self) -> Option<LandformRepresentability> {
        match self {
            Self::RiverBluff(recipe) => recipe.representability(),
        }
    }

    pub fn nearest_surface_below(self, world: bevy::math::Vec3) -> Option<f32> {
        match self {
            Self::RiverBluff(recipe) => recipe.upper_surface_below(world),
        }
    }
}

/// Compact immutable presentation handoff. Large vista grids remain outside
/// ordinary ECS replication; this component carries only weather, provenance,
/// and broad material coverage needed by every client.
#[derive(Clone, Debug, Eq, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
#[serde(deny_unknown_fields)]
pub struct SceneEnvironment {
    pub scene_digest: String,
    pub generation_version: u16,
    pub latitude_microdegrees: i32,
    pub longitude_microdegrees: i32,
    pub absolute_minute: u64,
    pub absolute_elevation_metres: i16,
    pub weather: WeatherSnapshot,
    pub canopy_bps: u16,
    pub wetland_bps: u16,
    pub cultivation_bps: u16,
    pub water_bps: u16,
    pub hilly_bps: u16,
}

/// Broad procedural silhouette family for a collider-bearing rock.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RockArchetype {
    Rounded,
    Angular,
    Slab,
}

/// Compact material family for generated geological geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RockLithology {
    Granite,
    Limestone,
    Sandstone,
}

/// Data-only recipe for a client-generated boulder mesh.
///
/// Dimensions describe the full local-space bounds in centimetres. The
/// authoritative server uses only `collision_radius_cm` for a conservative
/// sphere proxy; it never samples the field or extracts render geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RockRecipe {
    pub seed: u64,
    pub archetype: RockArchetype,
    pub lithology: RockLithology,
    pub dimensions_cm: [u16; 3],
    pub collision_radius_cm: u16,
}

impl RockRecipe {
    pub fn collision_radius_metres(self) -> f32 {
        f32::from(self.collision_radius_cm) / 100.0
    }

    pub fn dimensions_metres(self) -> [f32; 3] {
        self.dimensions_cm
            .map(|dimension| f32::from(dimension) / 100.0)
    }
}

/// Compact replicated identity for a server-authoritative static obstacle.
/// Its Transform locates the collider center; presentation derives matching
/// proxy geometry from this recipe on each client.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
#[serde(rename_all = "snake_case")]
pub enum SceneObstacle {
    Tree,
    Rock(RockRecipe),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedObstacle {
    Tree { x: u16, z: u16 },
    Rock { x: u16, z: u16, recipe: RockRecipe },
}

#[derive(Debug)]
pub struct GeneratedTacticalScene {
    pub digest: String,
    pub terrain: SceneTerrain,
    pub ground: SceneGround,
    pub obstacles: Vec<GeneratedObstacle>,
    pub terrain_patches: Vec<TerrainPatchRecipe>,
    pub repairs: SceneRepairReport,
}

impl GeneratedTacticalScene {
    /// Highest authoritative terrain-patch or heightfield surface beneath a
    /// world-space point. This supports distinct upper and lower benches.
    pub fn nearest_surface_below(&self, world: bevy::math::Vec3) -> Option<f32> {
        self.terrain_patches
            .iter()
            .filter_map(|patch| patch.nearest_surface_below(world))
            .chain(
                self.terrain
                    .height_at(bevy::math::Vec2::new(world.x, world.z))
                    .filter(|height| *height <= world.y),
            )
            .max_by(f32::total_cmp)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneRepairReport {
    pub upsampled_height_samples: u32,
    pub microrelief_adjusted_samples: u32,
    pub adjusted_height_samples: u32,
    pub repaired_water_samples: u32,
    pub removed_corridor_obstacles: u32,
}

impl SceneRepairReport {
    pub const fn was_repaired(self) -> bool {
        self.adjusted_height_samples != 0
            || self.repaired_water_samples != 0
            || self.removed_corridor_obstacles != 0
    }
}

#[derive(Debug, Error)]
pub enum SceneInputError {
    #[error("scene input I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("scene input JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("scene input is invalid: {0}")]
    Validation(String),
}

impl TacticalSceneInput {
    pub fn load(path: &Path) -> Result<Self, SceneInputError> {
        let length = fs::metadata(path)?.len();
        if length == 0 || length > MAX_SCENE_INPUT_BYTES {
            return Err(SceneInputError::Validation(
                "file exceeds the 32 MiB bound".into(),
            ));
        }
        let input: Self = serde_json::from_slice(&fs::read(path)?)?;
        input.validate()?;
        Ok(input)
    }

    pub fn validate(&self) -> Result<(), SceneInputError> {
        if self.schema_version != TACTICAL_SCENE_SCHEMA_VERSION {
            return invalid("incompatible schema version");
        }
        if self.generation_version != TACTICAL_SCENE_GENERATION_VERSION {
            return invalid("incompatible generation version");
        }
        if self.scene_key.is_empty() || self.scene_key.len() > MAX_TEMPLATE_BYTES {
            return invalid("scene key is empty or oversized");
        }
        let source_id = match &self.source {
            SceneSource::ImportedPackage(value) | SceneSource::SyntheticFixture(value) => value,
        };
        if source_id.is_empty() || source_id.len() > MAX_SOURCE_ID_BYTES {
            return invalid("source identity is empty or oversized");
        }
        if !(-90_000_000..=90_000_000).contains(&self.latitude_microdegrees)
            || !(-180_000_000..=180_000_000).contains(&self.longitude_microdegrees)
        {
            return invalid("geographic origin is out of bounds");
        }
        validate_grid(&self.playable, MAX_PLAYABLE_SIDE, "playable")?;
        if self.terrain_patches.len() > MAX_TERRAIN_PATCHES {
            return invalid("terrain patch count exceeds its bound");
        }
        for patch in &self.terrain_patches {
            let TerrainPatchRecipe::RiverBluff(recipe) = *patch;
            let playable_half_width =
                f32::from(self.playable.width - 1) * self.playable.spacing_metres * 0.5;
            let playable_half_depth =
                f32::from(self.playable.depth - 1) * self.playable.spacing_metres * 0.5;
            let center = recipe.center_metres();
            let dimensions = recipe.dimensions_metres();
            let horizontal_radius = (dimensions.x * 0.5).hypot(dimensions.z);
            if !(800..=1_200).contains(&recipe.face_height_cm)
                || !(1_200..=6_000).contains(&recipe.face_width_cm)
                || !(400..=2_400).contains(&recipe.rock_depth_cm)
                || !(-3_142..=3_142).contains(&recipe.yaw_milliradians)
                || !(-12_000.0..=12_000.0).contains(&center.y)
                || center.x.abs() + horizontal_radius > playable_half_width
                || center.z.abs() + horizontal_radius > playable_half_depth
                || recipe.curvature_cm > 600
                || recipe.undercut_depth_cm == 0
                || recipe.undercut_depth_cm > 400
                || recipe.vertical_intersections < 1
                || recipe.error_tolerance_cm == 0
                || recipe.collapse_radius_cm < 100
                || recipe.collapse_radius_cm > recipe.face_width_cm / 2
                || recipe.collapse_offset_cm.unsigned_abs()
                    > recipe.face_width_cm / 2 - recipe.collapse_radius_cm
                || recipe.talus_depth_cm > 1_200
                || !(25..=100).contains(&recipe.sample_spacing_cm)
                || recipe.representability().is_none()
                || recipe.representability().is_some_and(|report| {
                    report.representation != TerrainRepresentation::ImplicitSurface
                })
            {
                return invalid("river bluff recipe is invalid or not implicitly representable");
            }
        }
        if self.vista.lods.len() > MAX_VISTA_LEVELS {
            return invalid("vista has too many LOD levels");
        }
        let mut previous_level = None;
        let mut previous_spacing = self.playable.spacing_metres;
        let mut vista_samples = 0usize;
        for lod in &self.vista.lods {
            if previous_level.is_some_and(|level| lod.level <= level) {
                return invalid("vista LOD levels are not strictly increasing");
            }
            if !lod.origin_east_metres.is_finite() || !lod.origin_north_metres.is_finite() {
                return invalid("vista LOD origin is not finite");
            }
            let grid = TerrainSampleGrid {
                width: lod.width,
                depth: lod.depth,
                spacing_metres: lod.spacing_metres,
                heights_metres: lod.heights_metres.clone(),
                environment: lod.environment.clone(),
            };
            validate_grid(&grid, u16::MAX as usize, "vista")?;
            if lod.spacing_metres <= previous_spacing {
                return invalid("vista LOD spacing must progressively increase");
            }
            vista_samples = vista_samples
                .checked_add(lod.heights_metres.len())
                .ok_or_else(|| SceneInputError::Validation("vista sample count overflow".into()))?;
            if vista_samples > MAX_VISTA_SAMPLES {
                return invalid("vista sample count exceeds its bound");
            }
            previous_level = Some(lod.level);
            previous_spacing = lod.spacing_metres;
        }
        validate_weather(self.weather)?;
        Ok(())
    }

    pub fn digest(&self) -> Result<String, SceneInputError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub fn generate(&self) -> Result<GeneratedTacticalScene, SceneInputError> {
        self.validate()?;
        let (grid_width, grid_depth, grid_spacing, mut heights, mut environment) =
            upsample_playable_grid(&self.playable);
        let upsampled_height_samples = heights
            .len()
            .saturating_sub(self.playable.heights_metres.len())
            as u32;
        let microrelief_adjusted_samples = add_authoritative_microrelief(
            self.seed,
            grid_width,
            grid_depth,
            grid_spacing,
            &mut heights,
            &environment,
        );
        apply_terrain_patch_heightfield_replacement(
            grid_width,
            grid_depth,
            grid_spacing,
            &mut heights,
            &mut environment,
            &self.terrain_patches,
        );
        let mut repairs = repair_playable_terrain(
            grid_width,
            grid_depth,
            grid_spacing,
            &mut heights,
            &mut environment,
        );
        repairs.upsampled_height_samples = upsampled_height_samples;
        repairs.microrelief_adjusted_samples = microrelief_adjusted_samples;
        let terrain = SceneTerrain::from_heightmap(grid_width, grid_depth, grid_spacing, heights)
            .ok_or_else(|| {
            SceneInputError::Validation("playable heightmap is invalid".into())
        })?;
        let mut obstacles = self
            .playable
            .environment
            .iter()
            .enumerate()
            .filter_map(|(index, sample)| {
                let x = (index % usize::from(self.playable.width)) as u16;
                let z = (index / usize::from(self.playable.width)) as u16;
                let coordinate = ((x as u64) << 32) ^ z as u64;
                let tree_roll = splitmix64(self.seed ^ coordinate) % 10_000;
                let rock_seed = splitmix64(self.seed ^ coordinate ^ 0x52cc_5f1b_d391_a739);
                let rock_roll = rock_seed % 10_000;
                if tree_roll < u64::from(sample.canopy_bps) / 12 {
                    Some(GeneratedObstacle::Tree { x, z })
                } else if rock_roll < u64::from(sample.hilly_bps) / 20 && sample.water_bps < 5_000 {
                    Some(GeneratedObstacle::Rock {
                        x,
                        z,
                        recipe: rock_recipe(rock_seed),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let before = obstacles.len();
        obstacles.retain(|obstacle| {
            let depth = usize::from(self.playable.depth);
            let (x, z) = match *obstacle {
                GeneratedObstacle::Tree { x, z } | GeneratedObstacle::Rock { x, z, .. } => (x, z),
            };
            let reserved = is_reserved_playability_cell(
                usize::from(x),
                usize::from(z),
                usize::from(self.playable.width),
                usize::from(self.playable.depth),
            );
            let tree_in_patch_evidence_corridor =
                matches!(obstacle, GeneratedObstacle::Tree { .. })
                    && self.terrain_patches.iter().any(|patch| {
                        let TerrainPatchRecipe::RiverBluff(recipe) = *patch;
                        let half_width =
                            f32::from(self.playable.width - 1) * self.playable.spacing_metres * 0.5;
                        let half_depth =
                            f32::from(self.playable.depth - 1) * self.playable.spacing_metres * 0.5;
                        let world = bevy::math::Vec3::new(
                            f32::from(x) * self.playable.spacing_metres - half_width,
                            0.0,
                            f32::from(z) * self.playable.spacing_metres - half_depth,
                        );
                        let local = recipe.world_to_local(world);
                        let brink = recipe.top_front_local_z(local.x.clamp(
                            -recipe.dimensions_metres().x * 0.5,
                            recipe.dimensions_metres().x * 0.5,
                        ));
                        let size = recipe.dimensions_metres();
                        local.x.abs() <= size.x * 0.5 + 7.0
                            && local.z >= brink - 20.0
                            && local.z <= size.z + 5.0
                    });
            match obstacle {
                GeneratedObstacle::Tree { .. } => {
                    !is_tree_camera_clearance_cell(usize::from(x), usize::from(z), depth)
                        && !tree_in_patch_evidence_corridor
                }
                GeneratedObstacle::Rock { .. } => !reserved,
            }
        });
        repairs.removed_corridor_obstacles = (before - obstacles.len()) as u32;
        let ground = build_scene_ground(
            grid_width,
            grid_depth,
            grid_spacing,
            &environment,
            &terrain,
            &obstacles,
            self.playable.spacing_metres,
            &self.terrain_patches,
        )?;
        Ok(GeneratedTacticalScene {
            digest: self.digest()?,
            terrain,
            ground,
            obstacles,
            terrain_patches: self.terrain_patches.clone(),
            repairs,
        })
    }

    pub fn environment_snapshot(&self, scene_digest: String) -> SceneEnvironment {
        let count = self.playable.environment.len().max(1) as u64;
        let sum = self
            .playable
            .environment
            .iter()
            .fold([0u64; 5], |mut sum, sample| {
                sum[0] += u64::from(sample.canopy_bps);
                sum[1] += u64::from(sample.wetland_bps);
                sum[2] += u64::from(sample.cultivation_bps);
                sum[3] += u64::from(sample.water_bps);
                sum[4] += u64::from(sample.hilly_bps);
                sum
            });
        SceneEnvironment {
            scene_digest,
            generation_version: self.generation_version,
            latitude_microdegrees: self.latitude_microdegrees,
            longitude_microdegrees: self.longitude_microdegrees,
            absolute_minute: self.absolute_minute,
            absolute_elevation_metres: self.absolute_elevation_metres,
            weather: self.weather,
            canopy_bps: (sum[0] / count) as u16,
            wetland_bps: (sum[1] / count) as u16,
            cultivation_bps: (sum[2] / count) as u16,
            water_bps: (sum[3] / count) as u16,
            hilly_bps: (sum[4] / count) as u16,
        }
    }
}

fn apply_terrain_patch_heightfield_replacement(
    width: usize,
    depth: usize,
    spacing: f32,
    heights: &mut [f32],
    _environment: &mut [EnvironmentalSample],
    patches: &[TerrainPatchRecipe],
) {
    let half_width = (width - 1) as f32 * spacing * 0.5;
    let half_depth = (depth - 1) as f32 * spacing * 0.5;
    for patch in patches {
        let TerrainPatchRecipe::RiverBluff(recipe) = *patch;
        let size = recipe.dimensions_metres();
        let patch_half_width = size.x * 0.5;
        let rear_convergence_start =
            recipe.rear_terrace_convergence_start_local_z() + spacing * 1.1;
        // The implicit mass owns the central non-heightfield topology. Beyond
        // its low returned-face contact, an ordinary front-facing heightfield
        // ramp replaces the face and tapers into inherited shoulder terrain.
        for z in 0..depth {
            for x in 0..width {
                let world = bevy::math::Vec3::new(
                    x as f32 * spacing - half_width,
                    recipe.center_metres().y,
                    z as f32 * spacing - half_depth,
                );
                let local = recipe.world_to_local(world);
                let talus_depth = f32::from(recipe.talus_depth_cm) / 100.0;
                let shoulder_width = 7.0;
                let within_terrain_ownership = local.x.abs() <= patch_half_width + shoulder_width;
                let neighbour_envelope = [-spacing, 0.0, spacing]
                    .into_iter()
                    .map(|offset| {
                        recipe.maximum_face_local_z(
                            (local.x + offset).clamp(-patch_half_width, patch_half_width),
                        )
                    })
                    .fold(f32::NEG_INFINITY, f32::max);
                let reserved = is_reserved_playability_cell(x, z, width, depth);
                if within_terrain_ownership && local.z >= -talus_depth - spacing * 2.0 && !reserved
                {
                    let inherited_height = heights[z * width + x];
                    let clamped_x = local.x.clamp(-patch_half_width, patch_half_width);
                    let collapse_center = f32::from(recipe.collapse_offset_cm) / 100.0;
                    let collapse_relative_x = local.x - collapse_center;
                    let returned_shoulder_weight = smoothstep(
                        ((collapse_relative_x.abs() - recipe.implicit_collision_half_width())
                            / (spacing * 0.5))
                            .clamp(0.0, 1.0),
                    );
                    let central_upper =
                        recipe.center_metres().y + recipe.local_crest_height(clamped_x);
                    let collision_half_width = recipe.implicit_collision_half_width();
                    let returned_contact_x =
                        collapse_center + collision_half_width.copysign(collapse_relative_x);
                    let returned_contact_upper =
                        recipe.center_metres().y + recipe.local_crest_height(returned_contact_x);
                    let returned_fade = 1.0
                        - smoothstep(
                            ((collapse_relative_x.abs() - collision_half_width)
                                / (patch_half_width + shoulder_width - collision_half_width))
                                .clamp(0.0, 1.0),
                        );
                    let returned_upper = returned_contact_upper * returned_fade
                        + inherited_height * (1.0 - returned_fade);
                    let authored_upper = central_upper * (1.0 - returned_shoulder_weight)
                        + returned_upper * returned_shoulder_weight;
                    // The central heightfield step stays tightly buried behind the
                    // topologically complex implicit scarp. Across the heightfield-owned
                    // returned shoulders, however, the same rise is spread over enough depth
                    // to remain an ordinary single-valued slope. The blend completes within
                    // half a coarse cell beyond the collision boundary; the central owner
                    // hides that handoff while every visible returned column uses the ramp.
                    let smoothed_returned_envelope = [
                        (-spacing * 2.0, 1.0),
                        (-spacing, 2.0),
                        (0.0, 3.0),
                        (spacing, 2.0),
                        (spacing * 2.0, 1.0),
                    ]
                    .into_iter()
                    .map(|(offset, weight)| {
                        recipe.maximum_face_local_z(
                            (local.x + offset).clamp(-patch_half_width, patch_half_width),
                        ) * weight
                    })
                    .sum::<f32>()
                        / 9.0;
                    let transition_start = neighbour_envelope * (1.0 - returned_shoulder_weight)
                        + smoothed_returned_envelope * returned_shoulder_weight
                        + spacing * (1.0 - returned_shoulder_weight * 5.0);
                    let authored_rise = (authored_upper - recipe.center_metres().y).abs();
                    let returned_transition_length =
                        (authored_rise * 2.2).clamp(spacing * 4.0, spacing * 9.0);
                    let transition_length = spacing * 0.10 * (1.0 - returned_shoulder_weight)
                        + returned_transition_length * returned_shoulder_weight;
                    let buried_transition =
                        ((local.z - transition_start) / transition_length).clamp(0.0, 1.0);
                    // The heightfield owns upper/rear collision, but it must not preserve a
                    // constant-height rectangular plateau behind the rendered scarp. Begin
                    // inheriting the native terrain immediately behind the conservative face
                    // envelope and finish over the same eight-metre landform-scale distance as
                    // the client terrain-solid cap. The visible brink remains fully authored;
                    // the remote tile perimeter is ordinary terrain again.
                    let rear_inheritance =
                        recipe.rear_terrace_inheritance(local.z, rear_convergence_start);
                    let upper = authored_upper * (1.0 - rear_inheritance)
                        + inherited_height * rear_inheritance;
                    heights[z * width + x] = recipe.center_metres().y
                        + (upper - recipe.center_metres().y) * smoothstep(buried_transition);
                }

                if !reserved {
                    let apron_height = recipe.debris_fan_height_local(local.x, local.z);
                    // The simulated collapse deposits onto the inherited lower surface. Adding
                    // its smooth height contribution avoids the hard polygon produced when the
                    // old fan replaced terrain with an absolute patch-local height.
                    heights[z * width + x] += apron_height;
                    let near_visible_face = local.z
                        >= recipe.minimum_face_local_z(local.x) - spacing * 1.5
                        && local.z <= recipe.maximum_face_local_z(local.x)
                        && recipe.rock_support_weight_local(local.x) > 0.0;
                    if near_visible_face {
                        heights[z * width + x] =
                            heights[z * width + x].min(recipe.center_metres().y + 0.48);
                    }
                }
            }
        }
    }
}

fn build_scene_ground(
    width: usize,
    depth: usize,
    spacing: f32,
    environment: &[EnvironmentalSample],
    terrain: &SceneTerrain,
    obstacles: &[GeneratedObstacle],
    obstacle_spacing: f32,
    _terrain_patches: &[TerrainPatchRecipe],
) -> Result<SceneGround, SceneInputError> {
    let mut samples = environment
        .iter()
        .copied()
        .map(base_ground_surface)
        .collect::<Vec<_>>();
    let half_width = terrain.width() * 0.5;
    let half_depth = terrain.depth() * 0.5;
    for obstacle in obstacles {
        let GeneratedObstacle::Tree { x, z } = *obstacle else {
            continue;
        };
        let tree = bevy::math::Vec2::new(
            f32::from(x) * obstacle_spacing - half_width,
            f32::from(z) * obstacle_spacing - half_depth,
        );
        for sample_z in 0..depth {
            for sample_x in 0..width {
                let position = bevy::math::Vec2::new(
                    sample_x as f32 * spacing - half_width,
                    sample_z as f32 * spacing - half_depth,
                );
                let distance = position.distance(tree);
                if distance > TREE_CANOPY_GROUND_RADIUS_METRES {
                    continue;
                }
                let sample = &mut samples[sample_z * width + sample_x];
                if matches!(
                    sample.substrate,
                    GroundSubstrate::Water | GroundSubstrate::Road
                ) {
                    continue;
                }
                let coordinate = ((u64::from(x)) << 48)
                    ^ ((u64::from(z)) << 32)
                    ^ ((sample_x as u64) << 16)
                    ^ sample_z as u64;
                let litter_roll =
                    (splitmix64(coordinate ^ 0x1eaf_1177_e2) % 10_000) as f32 / 10_000.0;
                if distance <= TREE_DENSE_LEAF_LITTER_RADIUS_METRES
                    || litter_roll < tree_leaf_litter_probability(distance)
                {
                    sample.cover = GroundCover::LeafLitter;
                    sample.cover_density_bps = 9_200;
                    sample.cover_height_cm = 6;
                }
            }
        }
    }
    SceneGround::from_samples(width, depth, spacing, samples).ok_or_else(|| {
        SceneInputError::Validation("generated ground-surface grid is invalid".into())
    })
}

fn tree_leaf_litter_probability(distance_metres: f32) -> f32 {
    if distance_metres <= TREE_DENSE_LEAF_LITTER_RADIUS_METRES {
        return 1.0;
    }
    let crown_fraction = ((distance_metres - TREE_DENSE_LEAF_LITTER_RADIUS_METRES)
        / (TREE_CANOPY_GROUND_RADIUS_METRES - TREE_DENSE_LEAF_LITTER_RADIUS_METRES))
        .clamp(0.0, 1.0);
    0.12 + (1.0 - crown_fraction).powf(1.5) * 0.60
}

fn base_ground_surface(sample: EnvironmentalSample) -> GroundSurface {
    if sample.crossing_bps >= 5_000 || matches!(sample.surface, TacticalSurface::Road) {
        return GroundSurface {
            substrate: GroundSubstrate::Road,
            cover: GroundCover::Bare,
            cover_density_bps: 0,
            cover_height_cm: 0,
        };
    }
    if sample.water_bps >= 5_000 || matches!(sample.surface, TacticalSurface::Water) {
        return GroundSurface {
            substrate: GroundSubstrate::Water,
            cover: GroundCover::Bare,
            cover_density_bps: 0,
            cover_height_cm: 0,
        };
    }
    if sample.wetland_bps >= 5_000 || matches!(sample.surface, TacticalSurface::Wetland) {
        return GroundSurface {
            substrate: GroundSubstrate::Mud,
            cover: GroundCover::Reeds,
            cover_density_bps: sample.wetland_bps.max(5_000),
            cover_height_cm: 110,
        };
    }
    if sample.hilly_bps >= 6_500 {
        return GroundSurface {
            substrate: if sample.hilly_bps >= 8_500 {
                GroundSubstrate::Stone
            } else {
                GroundSubstrate::Gravel
            },
            cover: GroundCover::LooseStone,
            cover_density_bps: (sample.hilly_bps / 2).clamp(3_250, 5_000),
            cover_height_cm: 4,
        };
    }
    GroundSurface {
        substrate: GroundSubstrate::Soil,
        cover: GroundCover::TallGrass,
        cover_density_bps: 9_600u16.saturating_sub(sample.canopy_bps / 5),
        cover_height_cm: 82,
    }
}

fn upsample_playable_grid(
    source: &TerrainSampleGrid,
) -> (usize, usize, f32, Vec<f32>, Vec<EnvironmentalSample>) {
    const TARGET_SPACING_METRES: f32 = 2.0;
    let source_width = usize::from(source.width);
    let source_depth = usize::from(source.depth);
    if source.spacing_metres <= TARGET_SPACING_METRES {
        return (
            source_width,
            source_depth,
            source.spacing_metres,
            source.heights_metres.clone(),
            source.environment.clone(),
        );
    }
    let largest_source_side = (source_width - 1).max(source_depth - 1);
    let maximum_subdivisions = ((MAX_PLAYABLE_SIDE - 1) / largest_source_side).max(1);
    let subdivisions = (source.spacing_metres / TARGET_SPACING_METRES)
        .ceil()
        .max(1.0) as usize;
    let subdivisions = subdivisions.min(maximum_subdivisions);
    let cells_x = (source_width - 1) * subdivisions;
    let cells_z = (source_depth - 1) * subdivisions;
    let width = cells_x + 1;
    let depth = cells_z + 1;
    let spacing = source.spacing_metres / subdivisions as f32;
    let mut heights = Vec::with_capacity(width * depth);
    let mut environment = Vec::with_capacity(width * depth);
    for z in 0..depth {
        for x in 0..width {
            let source_x = x as f32 / subdivisions as f32;
            let source_z = z as f32 / subdivisions as f32;
            let x0 = source_x.floor() as usize;
            let z0 = source_z.floor() as usize;
            let x1 = (x0 + 1).min(source_width - 1);
            let z1 = (z0 + 1).min(source_depth - 1);
            let tx = source_x - x0 as f32;
            let tz = source_z - z0 as f32;
            let north = lerp(
                source.heights_metres[z0 * source_width + x0],
                source.heights_metres[z0 * source_width + x1],
                tx,
            );
            let south = lerp(
                source.heights_metres[z1 * source_width + x0],
                source.heights_metres[z1 * source_width + x1],
                tx,
            );
            heights.push(lerp(north, south, tz));
            let nearest_x = source_x.round() as usize;
            let nearest_z = source_z.round() as usize;
            environment.push(source.environment[nearest_z * source_width + nearest_x]);
        }
    }
    (width, depth, spacing, heights, environment)
}

fn lerp(left: f32, right: f32, amount: f32) -> f32 {
    left + (right - left) * amount
}

fn rock_recipe(seed: u64) -> RockRecipe {
    let archetype = match seed % 3 {
        0 => RockArchetype::Rounded,
        1 => RockArchetype::Angular,
        _ => RockArchetype::Slab,
    };
    let lithology = match splitmix64(seed ^ 0x6c69_7468_6f6c_6f67) % 3 {
        0 => RockLithology::Granite,
        1 => RockLithology::Limestone,
        _ => RockLithology::Sandstone,
    };
    let base_dimensions = match archetype {
        RockArchetype::Rounded => [128_u16, 104, 120],
        RockArchetype::Angular => [136, 112, 124],
        RockArchetype::Slab => [142, 72, 132],
    };
    let dimensions_cm = core::array::from_fn(|axis| {
        let hash = splitmix64(seed ^ (axis as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let offset = (hash % 17) as i16 - 8;
        base_dimensions[axis].saturating_add_signed(offset)
    });
    RockRecipe {
        seed,
        archetype,
        lithology,
        dimensions_cm,
        collision_radius_cm: (ROCK_RADIUS_METRES * 100.0) as u16,
    }
}

/// Adds sub-source-resolution detail before constructing the shared terrain.
/// The result therefore feeds the rendered mesh, height queries, IK, and the
/// authoritative server collider instead of becoming client-only displacement.
fn add_authoritative_microrelief(
    seed: u64,
    width: usize,
    depth: usize,
    spacing: f32,
    heights: &mut [f32],
    environment: &[EnvironmentalSample],
) -> u32 {
    let mut adjusted = 0;
    for z in 0..depth {
        for x in 0..width {
            let index = z * width + x;
            let sample = environment[index];
            if is_reserved_playability_cell(x, z, width, depth)
                || sample.water_bps >= 5_000
                || sample.crossing_bps >= 5_000
                || matches!(
                    sample.surface,
                    TacticalSurface::Road | TacticalSurface::Water
                )
            {
                continue;
            }
            let hilly = f32::from(sample.hilly_bps) / 10_000.0;
            let wetland = f32::from(sample.wetland_bps) / 10_000.0;
            let amplitude = (0.055 + hilly * 0.22) * (1.0 - wetland * 0.55);
            let world_x = x as f32 * spacing;
            let world_z = z as f32 * spacing;
            let broad = value_noise(seed, world_x, world_z, 6.0);
            let fine = value_noise(seed ^ 0x8f3f_73b5_cf1c_9ade, world_x, world_z, 2.25);
            let offset = (broad * 0.72 + fine * 0.28) * amplitude;
            if offset.abs() > f32::EPSILON {
                heights[index] += offset;
                adjusted += 1;
            }
        }
    }
    adjusted
}

fn value_noise(seed: u64, x: f32, z: f32, cell_size: f32) -> f32 {
    let gx = x / cell_size;
    let gz = z / cell_size;
    let x0 = gx.floor() as i32;
    let z0 = gz.floor() as i32;
    let tx = smoothstep(gx - x0 as f32);
    let tz = smoothstep(gz - z0 as f32);
    let sample = |ix: i32, iz: i32| {
        let coordinate = (ix as u32 as u64) << 32 | iz as u32 as u64;
        let bits = splitmix64(seed ^ coordinate);
        (bits >> 40) as f32 / ((1_u32 << 24) - 1) as f32 * 2.0 - 1.0
    };
    let north = sample(x0, z0) + (sample(x0 + 1, z0) - sample(x0, z0)) * tx;
    let south = sample(x0, z0 + 1) + (sample(x0 + 1, z0 + 1) - sample(x0, z0 + 1)) * tx;
    north + (south - north) * tz
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn smooth_course_weight(
    sample_y: f32,
    center_y: f32,
    plateau_half_width: f32,
    edge_width: f32,
) -> f32 {
    1.0 - smoothstep(
        (((sample_y - center_y).abs() - plateau_half_width) / edge_width).clamp(0.0, 1.0),
    )
}

fn repair_playable_terrain(
    width: usize,
    depth: usize,
    spacing: f32,
    heights: &mut [f32],
    environment: &mut [EnvironmentalSample],
) -> SceneRepairReport {
    let original_heights = heights.to_vec();
    let maximum_step = spacing * MAX_PLAYABLE_GRADE;
    // Only the reserved combat corridor and deployment pads are repaired.
    // Deliberate landforms outside that mask must retain their authored relief.
    // Local masks can contain long one-cell corridors connected to wider
    // deployment pads. Iterate to convergence across the longest grid axis;
    // four global sweeps were insufficient once non-reserved cliff cells
    // stopped participating in repair.
    for _ in 0..width.max(depth) {
        for z in 0..depth {
            for x in 1..width {
                if is_reserved_playability_cell(x - 1, z, width, depth)
                    || is_reserved_playability_cell(x, z, width, depth)
                {
                    clamp_height_pair(heights, z * width + x - 1, z * width + x, maximum_step);
                }
            }
            for x in (0..width - 1).rev() {
                if is_reserved_playability_cell(x + 1, z, width, depth)
                    || is_reserved_playability_cell(x, z, width, depth)
                {
                    clamp_height_pair(heights, z * width + x + 1, z * width + x, maximum_step);
                }
            }
        }
        for x in 0..width {
            for z in 1..depth {
                if is_reserved_playability_cell(x, z - 1, width, depth)
                    || is_reserved_playability_cell(x, z, width, depth)
                {
                    clamp_height_pair(heights, (z - 1) * width + x, z * width + x, maximum_step);
                }
            }
            for z in (0..depth - 1).rev() {
                if is_reserved_playability_cell(x, z + 1, width, depth)
                    || is_reserved_playability_cell(x, z, width, depth)
                {
                    clamp_height_pair(heights, (z + 1) * width + x, z * width + x, maximum_step);
                }
            }
        }
    }

    // Finish each three-by-three deployment pad from the already constrained
    // corridor profile. Alternating horizontal and vertical projections can
    // otherwise leave a small residual at the pad/corridor junction when the
    // surrounding authored terrain differs sharply on opposite sides.
    let center_z = depth / 2;
    for _ in 0..4 {
        for x in 1..width {
            clamp_height_pair(
                heights,
                center_z * width + x - 1,
                center_z * width + x,
                maximum_step,
            );
        }
        for x in (0..width - 1).rev() {
            clamp_height_pair(
                heights,
                center_z * width + x + 1,
                center_z * width + x,
                maximum_step,
            );
        }
    }
    for center_x in [width / 4, width * 3 / 4] {
        for z in center_z.saturating_sub(1)..=(center_z + 1).min(depth - 1) {
            for x in center_x.saturating_sub(1)..=(center_x + 1).min(width - 1) {
                heights[z * width + x] = heights[center_z * width + x];
            }
        }
    }

    let mut repaired_water_samples = 0;
    for z in 0..depth {
        for x in 0..width {
            if is_reserved_playability_cell(x, z, width, depth)
                && environment[z * width + x].water_bps >= 8_000
            {
                let sample = &mut environment[z * width + x];
                sample.water_bps = 0;
                sample.wetland_bps = sample.wetland_bps.min(4_000);
                sample.canopy_bps = 0;
                sample.surface = TacticalSurface::Open;
                repaired_water_samples += 1;
            }
        }
    }
    SceneRepairReport {
        upsampled_height_samples: 0,
        microrelief_adjusted_samples: 0,
        adjusted_height_samples: heights
            .iter()
            .zip(original_heights)
            .filter(|(after, before)| (*after - before).abs() > f32::EPSILON)
            .count() as u32,
        repaired_water_samples,
        removed_corridor_obstacles: 0,
    }
}

fn is_reserved_playability_cell(x: usize, z: usize, width: usize, depth: usize) -> bool {
    let center_z = depth / 2;
    let party_x = width / 4;
    let enemy_x = width * 3 / 4;
    z == center_z
        || [party_x, enemy_x]
            .into_iter()
            .any(|center_x| x.abs_diff(center_x) <= 1 && z.abs_diff(center_z) <= 1)
}

fn is_tree_camera_clearance_cell(_x: usize, z: usize, depth: usize) -> bool {
    let center_z = depth / 2;
    // Players currently enter within a bounded five-metre square around the
    // scene origin. Keep only large tree crowns out of the centre row and its
    // immediate neighbours so they cannot enter the production third-person
    // camera envelope. Rocks and terrain repair retain the narrower gameplay
    // corridor contract above.
    z.abs_diff(center_z) <= 1
}

fn clamp_height_pair(heights: &mut [f32], anchor: usize, target: usize, maximum_step: f32) {
    let minimum = heights[anchor] - maximum_step;
    let maximum = heights[anchor] + maximum_step;
    heights[target] = heights[target].clamp(minimum, maximum);
}

fn validate_grid(
    grid: &TerrainSampleGrid,
    max_side: usize,
    label: &str,
) -> Result<(), SceneInputError> {
    let width = usize::from(grid.width);
    let depth = usize::from(grid.depth);
    if width < 2 || depth < 2 || width > max_side || depth > max_side {
        return invalid(format!("{label} dimensions are out of bounds"));
    }
    if !grid.spacing_metres.is_finite() || !(0.25..=2_000.0).contains(&grid.spacing_metres) {
        return invalid(format!("{label} spacing is out of bounds"));
    }
    let expected = width
        .checked_mul(depth)
        .ok_or_else(|| SceneInputError::Validation(format!("{label} dimensions overflow")))?;
    if grid.heights_metres.len() != expected || grid.environment.len() != expected {
        return invalid(format!("{label} sample counts do not match dimensions"));
    }
    if grid
        .heights_metres
        .iter()
        .any(|height| !height.is_finite() || !(-12_000.0..=12_000.0).contains(height))
    {
        return invalid(format!("{label} contains an invalid height"));
    }
    if grid.environment.iter().any(|sample| {
        [
            sample.canopy_bps,
            sample.wetland_bps,
            sample.cultivation_bps,
            sample.water_bps,
            sample.hilly_bps,
            sample.crossing_bps,
        ]
        .into_iter()
        .any(|value| value > 10_000)
    }) {
        return invalid(format!("{label} contains an invalid environment sample"));
    }
    Ok(())
}

fn validate_weather(weather: WeatherSnapshot) -> Result<(), SceneInputError> {
    if weather.rules_version != WEATHER_RULES_VERSION
        || weather.wind_speed_bps > 10_000
        || weather.intensity_bps > 10_000
        || weather.ground_moisture_bps > 10_000
        || weather.snow_cover_bps > 10_000
        || weather.atmosphere.relative_humidity_bps > 10_000
        || weather.atmosphere.dew_point_deci_c > weather.temperature_deci_c + 5
        || !(8_700..=10_850).contains(&weather.atmosphere.sea_level_pressure_deci_hpa)
        || weather.atmosphere.wind_direction_degrees >= 360
        || weather.atmosphere.wind_shear_bps > 10_000
        || weather.atmosphere.instability_bps > 10_000
        || !(-10_000..=10_000).contains(&weather.atmosphere.lift_bps)
        || weather.cloud_layers().any(|layer| {
            layer.coverage_bps > 10_000
                || layer.optical_density_bps > 10_000
                || layer.top_metres <= layer.base_metres
        })
        || (matches!(weather.precipitation, Precipitation::Clear) && weather.intensity_bps != 0)
        || (!matches!(weather.precipitation, Precipitation::Clear)
            && (weather.intensity_bps == 0
                || !weather.cloud_layers().any(|layer| {
                    matches!(
                        layer.form,
                        adventuresim_core::weather::CloudForm::Cumulonimbus
                            | adventuresim_core::weather::CloudForm::Nimbostratus
                    )
                })))
    {
        return invalid("weather snapshot is invalid");
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SceneInputError> {
    Err(SceneInputError::Validation(message.into()))
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::weather::weather_at;
    use std::time::SystemTime;

    fn fixture() -> TacticalSceneInput {
        let environment = vec![
            EnvironmentalSample {
                canopy_bps: 8_000,
                ..Default::default()
            };
            9
        ];
        TacticalSceneInput {
            schema_version: TACTICAL_SCENE_SCHEMA_VERSION,
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            seed: 42,
            scene_key: "woodland".into(),
            source: SceneSource::SyntheticFixture("dense-woodland".into()),
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 123_456,
            absolute_elevation_metres: 80,
            playable: TerrainSampleGrid {
                width: 3,
                depth: 3,
                spacing_metres: 1.0,
                heights_metres: vec![0.0, 0.1, 0.0, 0.1, 0.2, 0.1, 0.0, 0.1, 0.0],
                environment,
            },
            terrain_patches: Vec::new(),
            vista: VistaSample::default(),
            weather: weather_at(42, 123_456, 53_500_000, 10_000_000, 80),
        }
    }

    fn bluff_recipe() -> RiverBluffRecipe {
        RiverBluffRecipe {
            seed: 47_112,
            center_cm: [0, 0, 1_800],
            yaw_milliradians: 0,
            face_width_cm: 2_800,
            face_height_cm: 900,
            rock_depth_cm: 1_400,
            curvature_cm: 420,
            undercut_depth_cm: 80,
            collapse_offset_cm: 180,
            collapse_radius_cm: 300,
            talus_depth_cm: 600,
            heightfield_error_cm: 650,
            error_tolerance_cm: 75,
            vertical_intersections: 2,
            sample_spacing_cm: 28,
        }
    }

    #[test]
    fn representability_uses_topology_and_fidelity_not_grade() {
        let resolved_steep_plane = classify_landform(1, 2, 10, [8; 3]).unwrap();
        assert_eq!(
            resolved_steep_plane.representation,
            TerrainRepresentation::Heightfield
        );
        let unresolved_scarp = classify_landform(1, 80, 10, [8; 3]).unwrap();
        assert_eq!(
            unresolved_scarp.representation,
            TerrainRepresentation::ImplicitSurface
        );
        let undercut = classify_landform(2, 0, 10, [8; 3]).unwrap();
        assert_eq!(
            undercut.representation,
            TerrainRepresentation::ImplicitSurface
        );
        assert!(classify_landform(1, 0, 10, [81; 3]).is_none());
    }

    #[test]
    fn river_bluff_scalar_is_an_unbounded_terrain_mass_without_box_closures() {
        let recipe = bluff_recipe();
        let size = recipe.dimensions_metres();
        let behind = |local| recipe.signed_distance(recipe.local_to_world(local));
        assert!(
            behind(bevy::math::Vec3::new(0.0, size.y * 0.5, size.z + 50.0)) < 0.0,
            "terrace mass must not close on the obsolete finite back plane"
        );
        assert!(
            behind(bevy::math::Vec3::new(0.0, -50.0, size.z * 0.5)) < 0.0,
            "terrain mass must remain solid downward without a finite bottom plane"
        );
        let outer_x = size.x * 0.5 + 4.0;
        assert!(
            behind(bevy::math::Vec3::new(outer_x, -0.5, size.z + 4.0)) < 0.0
                && behind(bevy::math::Vec3::new(outer_x, 0.5, size.z + 4.0)) > 0.0,
            "lateral crest fade must converge to ordinary solid-below-ground terrain"
        );
        assert!(
            behind(bevy::math::Vec3::new(0.0, 0.5, -50.0)) > 0.0,
            "front floodplain must remain outside the rearward terrace mass"
        );
    }

    #[test]
    fn river_bluff_budget_proxy_and_upper_lower_queries_are_deterministic() {
        let recipe = bluff_recipe();
        let report = recipe.representability().unwrap();
        assert!(report.sample_count as usize <= MAX_TERRAIN_PATCH_SAMPLES);
        assert_eq!(
            recipe.collision_proxy_boxes(),
            recipe.collision_proxy_boxes()
        );
        let proxies = recipe.collision_proxy_boxes();
        assert!(proxies.len() <= 2_520);
        assert!(
            proxies
                .iter()
                .all(|proxy| proxy.half_extents.min_element() > 0.0)
        );
        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let collapse_radius = f32::from(recipe.collapse_radius_cm) / 100.0;
        let collision_half_width = recipe.implicit_collision_half_width();
        for proxy in &proxies {
            let local_center = recipe.world_to_local(proxy.center);
            assert!(
                local_center.x.abs() + proxy.half_extents.x <= collision_half_width + 0.001,
                "implicit collision leaked into heightfield-owned returned shoulder"
            );
            let front = local_center.z - proxy.half_extents.z;
            let authored_face = recipe.face_surface_local_z(local_center);
            let front_offset = front - authored_face;
            assert!(
                (0.0..=0.65).contains(&front_offset),
                "face proxy offset {front_offset}m exceeded the authored fit at {local_center:?}"
            );
            for sample_x in [
                local_center.x - proxy.half_extents.x,
                local_center.x,
                local_center.x + proxy.half_extents.x,
            ] {
                for sample_y in [
                    local_center.y - proxy.half_extents.y,
                    local_center.y,
                    local_center.y + proxy.half_extents.y,
                ] {
                    let sampled_offset = front
                        - recipe
                            .face_surface_local_z(bevy::math::Vec3::new(sample_x, sample_y, 0.0));
                    assert!(
                        (0.0..=0.65).contains(&sampled_offset),
                        "proxy corner offset {sampled_offset}m exceeded sampled face fit"
                    );
                    let semantic_point = bevy::math::Vec3::new(sample_x, sample_y, 0.0);
                    assert!(
                        recipe.undercut_weight_local(semantic_point) <= 0.08,
                        "proxy or cyan front-line sample entered authored undercut air"
                    );
                    assert!(
                        recipe.failure_scar_weight(semantic_point) <= 0.08,
                        "proxy or cyan front-line sample entered authored failure air"
                    );
                }
            }
            assert!(proxy.half_extents.z <= 0.35 + f32::EPSILON);
            assert!(
                recipe.undercut_weight_local(local_center) <= 0.08,
                "collision proxy filled the localized undercut"
            );
            assert!(
                recipe.failure_scar_weight(local_center) <= 0.08,
                "collision proxy filled the recessed failure scar"
            );
        }
        for proxy in &proxies {
            let local_center = recipe.world_to_local(proxy.center);
            if (local_center.x - collapse_x).abs() <= collapse_radius * 1.15 {
                continue;
            }
            let slice_top = proxies
                .iter()
                .filter_map(|candidate| {
                    let local = recipe.world_to_local(candidate.center);
                    ((local.x - local_center.x).abs() <= candidate.half_extents.x + 0.02)
                        .then_some(local.y + candidate.half_extents.y)
                })
                .fold(f32::NEG_INFINITY, f32::max);
            let slice_half_width = proxy.half_extents.x + 0.01;
            let safe_crest = [
                local_center.x - slice_half_width,
                local_center.x,
                local_center.x + slice_half_width,
            ]
            .into_iter()
            .map(|x| recipe.local_crest_height(x))
            .fold(f32::INFINITY, f32::min);
            assert!(
                safe_crest - slice_top <= 0.75,
                "collision proxy crest gap {}m exceeded the conservative slice crest {}m at x={}m (top={}m)",
                safe_crest - slice_top,
                safe_crest,
                local_center.x,
                slice_top,
            );
            assert!(
                recipe.local_crest_height(local_center.x) - safe_crest <= 0.75,
                "proxy slice at x={}m was too wide for central crest: center={}m safe={}m delta={}m",
                local_center.x,
                recipe.local_crest_height(local_center.x),
                safe_crest,
                recipe.local_crest_height(local_center.x) - safe_crest,
            );
        }
        let taper_regression_x = -6.125_f32;
        let taper_regression_half_width = 0.125_f32;
        let taper_regression_safe_crest = [
            taper_regression_x - taper_regression_half_width,
            taper_regression_x,
            taper_regression_x + taper_regression_half_width,
        ]
        .into_iter()
        .map(|x| recipe.local_crest_height(x))
        .fold(f32::INFINITY, f32::min);
        let taper_regression_top = proxies
            .iter()
            .filter_map(|proxy| {
                let local = recipe.world_to_local(proxy.center);
                ((local.x - taper_regression_x).abs() <= proxy.half_extents.x + 0.02)
                    .then_some(local.y + proxy.half_extents.y)
            })
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            taper_regression_safe_crest - taper_regression_top <= 0.75,
            "quarter-slice taper regression at x={taper_regression_x}: safe crest={taper_regression_safe_crest}, top={taper_regression_top}, gap={}",
            taper_regression_safe_crest - taper_regression_top,
        );
        let size = recipe.dimensions_metres();
        let slice_width = size.x / 28.0;
        for index in 0..28 {
            let start = -size.x * 0.5 + slice_width * index as f32;
            let end = start + slice_width;
            let center = (start + end) * 0.5;
            let safe_crest = [start, center, end]
                .into_iter()
                .map(|x| recipe.local_crest_height(x))
                .fold(f32::INFINITY, f32::min);
            if start < -collision_half_width
                || end > collision_half_width
                || (center - collapse_x).abs() <= collapse_radius * 1.15
            {
                continue;
            }
            assert!(
                recipe.local_crest_height(center) - safe_crest <= 0.75,
                "central proxy coverage policy omitted invalid slice x={center} safe={safe_crest} center_crest={}",
                recipe.local_crest_height(center),
            );
            assert!(
                proxies.iter().any(|proxy| {
                    let local_x = recipe.world_to_local(proxy.center).x;
                    (local_x - center).abs() <= proxy.half_extents.x + 0.02
                }),
                "central proxy slice x={center} has no retained collision bands"
            );
            let mut intervals = proxies
                .iter()
                .filter_map(|proxy| {
                    let local = recipe.world_to_local(proxy.center);
                    ((local.x - center).abs() <= proxy.half_extents.x + 0.02).then_some((
                        local.y - proxy.half_extents.y,
                        local.y + proxy.half_extents.y,
                    ))
                })
                .collect::<Vec<_>>();
            intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
            let mut covered_top = 0.0_f32;
            let undercut_clearance = recipe.undercut_collision_clearance_height();
            for (bottom, top) in intervals {
                let allowed_gap = if covered_top == 0.0
                    && recipe.undercut_weight_local(bevy::math::Vec3::new(center, 0.45, 0.0)) > 0.08
                {
                    undercut_clearance + 0.05
                } else {
                    0.75
                };
                assert!(
                    bottom - covered_top <= allowed_gap,
                    "central proxy slice x={center} has vertical collision gap {}m (allowed {allowed_gap}m) below band ({bottom},{top})",
                    bottom - covered_top,
                );
                covered_top = covered_top.max(top);
            }
            assert!(
                safe_crest - covered_top <= 0.75,
                "central proxy slice x={center} stops {}m below crest",
                safe_crest - covered_top,
            );
        }

        // Match the capture manifest's solid-column policy exactly. Columns intersecting the
        // authored failure air are excluded; every genuinely solid central column must receive
        // the same <=0.75-metre coverage reported by public evidence.
        for index in 0..28 {
            let x = -size.x * 0.5 + slice_width * (index as f32 + 0.5);
            if x.abs() > collision_half_width {
                continue;
            }
            let crest = recipe.local_crest_height(x);
            if !(0_u8..=20).all(|sample| {
                let y = crest * f32::from(sample) / 20.0;
                recipe.failure_scar_weight(bevy::math::Vec3::new(x, y, 0.0)) <= 0.08
            }) {
                continue;
            }
            let mut intervals = proxies
                .iter()
                .filter_map(|proxy| {
                    let local = recipe.world_to_local(proxy.center);
                    ((x - local.x).abs() <= proxy.half_extents.x + 0.02).then_some((
                        local.y - proxy.half_extents.y,
                        local.y + proxy.half_extents.y,
                    ))
                })
                .collect::<Vec<_>>();
            intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
            let mut covered_top =
                if recipe.undercut_weight_local(bevy::math::Vec3::new(x, 0.45, 0.0)) > 0.08 {
                    recipe.undercut_collision_clearance_height()
                } else {
                    0.08
                };
            for (bottom, top) in intervals {
                assert!(
                    bottom - covered_top <= 0.75,
                    "manifest solid column x={x} has vertical gap {} below ({bottom},{top})",
                    bottom - covered_top,
                );
                covered_top = covered_top.max(top);
            }
            assert!(crest - covered_top <= 0.75);
        }

        let mut input = fixture();
        input.playable = TerrainSampleGrid {
            width: 51,
            depth: 51,
            spacing_metres: 2.0,
            heights_metres: vec![0.0; 51 * 51],
            environment: vec![EnvironmentalSample::default(); 51 * 51],
        };
        input.terrain_patches = vec![TerrainPatchRecipe::RiverBluff(recipe)];
        let generated = input.generate().unwrap();
        let boundary_x = collision_half_width - 1.0;
        let returned_x = collision_half_width + 1.0;
        let returned_z =
            recipe.rear_terrace_convergence_start_local_z() + input.playable.spacing_metres * 1.1;
        let returned = recipe.local_to_world(bevy::math::Vec3::new(returned_x, 0.0, returned_z));
        let boundary = recipe.local_to_world(bevy::math::Vec3::new(boundary_x, 0.0, returned_z));
        let returned_height = generated
            .terrain
            .height_at(bevy::math::Vec2::new(returned.x, returned.z))
            .unwrap();
        let boundary_height = generated
            .terrain
            .height_at(bevy::math::Vec2::new(boundary.x, boundary.z))
            .unwrap();
        assert!(returned_height > 0.5);
        assert!(
            (boundary_height - recipe.local_crest_height(boundary_x)).abs() <= 0.75,
            "last central heightfield column must meet the implicit crest from behind: height={boundary_height}, crest={}, x={boundary_x}, z={returned_z}, rear_start={}, envelope={}",
            recipe.local_crest_height(boundary_x),
            recipe.rear_terrace_convergence_start_local_z(),
            recipe.maximum_face_local_z(boundary_x),
        );
        assert!(
            (returned_height - boundary_height).abs() <= 2.0,
            "heightfield collision must remain continuous across implicit/returned ownership boundary: returned={returned_height}, boundary={boundary_height}, delta={}, z={returned_z}, native returned crest={}, boundary crest={}, returned envelope={}",
            (returned_height - boundary_height).abs(),
            recipe.local_crest_height(returned_x),
            recipe.local_crest_height(boundary_x),
            recipe.maximum_face_local_z(returned_x),
        );
        assert_eq!(
            generated.nearest_surface_below(bevy::math::Vec3::new(0.0, 20.0, 24.0)),
            Some(recipe.local_crest_height(0.0))
        );
        let lower = generated
            .terrain
            .height_at(bevy::math::Vec2::new(0.0, 12.0))
            .unwrap();
        assert_eq!(
            generated.nearest_surface_below(bevy::math::Vec3::new(0.0, 5.0, 12.0)),
            Some(lower)
        );
        assert_eq!(input.digest().unwrap(), input.digest().unwrap());
    }

    #[test]
    fn river_bluff_undercut_and_recessed_failure_are_localized() {
        let mut recipe = bluff_recipe();
        recipe.collapse_offset_cm = 0;
        let quiet_x = -7.5;
        assert!(recipe.undercut_weight_local(bevy::math::Vec3::new(0.0, 0.35, 0.0)) > 0.9);
        assert_eq!(
            recipe.undercut_weight_local(bevy::math::Vec3::new(quiet_x, 0.35, 0.0)),
            0.0
        );
        let opening_x = (-80_i16..=80)
            .map(|step| f32::from(step) * 0.1)
            .filter(|x| recipe.undercut_weight_local(bevy::math::Vec3::new(*x, 0.35, 0.0)) > 0.08)
            .collect::<Vec<_>>();
        let opening_width = *opening_x.last().unwrap() - *opening_x.first().unwrap();
        assert!(
            (1.5..=2.6).contains(&opening_width),
            "shallow feathered mouth must remain broader than the full-depth core without reading as a cave: {opening_width}"
        );
        let full_depth_x = (-80_i16..=80)
            .map(|step| f32::from(step) * 0.05)
            .filter(|x| recipe.undercut_weight_local(bevy::math::Vec3::new(*x, 0.35, 0.0)) > 0.98)
            .collect::<Vec<_>>();
        let full_depth_width = *full_depth_x.last().unwrap() - *full_depth_x.first().unwrap();
        assert!(
            (0.5..=1.2).contains(&full_depth_width),
            "full-depth mouth must stay narrow and asymmetrically feathered: {full_depth_width}"
        );
        let opening_height = (0_u16..=160)
            .map(|step| f32::from(step) * 0.01)
            .filter(|y| recipe.undercut_weight_local(bevy::math::Vec3::new(0.0, *y, 0.0)) > 0.08)
            .fold(0.0_f32, f32::max);
        assert!((0.55..=0.82).contains(&opening_height));
        let roof_heights = [-0.8_f32, 0.0, 0.8].map(|x| {
            (0_u16..=140)
                .map(|step| f32::from(step) * 0.01)
                .filter(|y| recipe.undercut_weight_local(bevy::math::Vec3::new(x, *y, 0.0)) > 0.08)
                .fold(0.0_f32, f32::max)
        });
        let roof_min = roof_heights.into_iter().fold(f32::INFINITY, f32::min);
        let roof_max = roof_heights.into_iter().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            roof_min >= 0.25 && roof_max <= 0.78 && roof_max - roof_min >= 0.15,
            "undercut roof must stay low and visibly irregular: min={roof_min}, max={roof_max}"
        );

        let scar_span = |y: f32| {
            let xs = (-80_i16..=80)
                .map(|step| f32::from(step) * 0.05)
                .filter(|x| recipe.failure_scar_weight(bevy::math::Vec3::new(*x, y, 0.0)) > 0.5)
                .collect::<Vec<_>>();
            (*xs.first().unwrap(), *xs.last().unwrap())
        };
        let lower_span = scar_span(4.8);
        let upper_span = scar_span(8.0);
        assert!(
            lower_span.0 - upper_span.0 >= 0.6 && upper_span.1 - lower_span.1 >= 0.7,
            "scar sides must shear and diverge independently: lower={lower_span:?}, upper={upper_span:?}"
        );
        assert!(
            (upper_span.1 - upper_span.0) - (lower_span.1 - lower_span.0) >= 1.3,
            "failure polygon must widen visibly toward its open crest"
        );
        assert!(
            (upper_span.1 - upper_span.0) / (lower_span.1 - lower_span.0) >= 1.8,
            "failure must read as a strongly tapering missing wedge: lower={lower_span:?}, upper={upper_span:?}"
        );
        let lower_center = (lower_span.0 + lower_span.1) * 0.5;
        let upper_center = (upper_span.0 + upper_span.1) * 0.5;
        assert!(
            (upper_center - lower_center).abs() >= 0.15,
            "fracture wedge must traverse diagonally across the face"
        );
        let base_onset = |x: f32| {
            (250_u16..=600)
                .map(|step| f32::from(step) * 0.01)
                .find(|y| recipe.failure_scar_weight(bevy::math::Vec3::new(x, *y, 0.0)) > 0.08)
                .unwrap()
        };
        assert!(
            (base_onset(-0.8) - base_onset(0.8)).abs() >= 0.25,
            "failure release base must remain visibly oblique"
        );

        // Measure the release displacement against the same authored macro-surface. Comparing
        // signed distances at different x positions would conflate the scar with the intentional
        // multi-metre toe-to-crest retreat of the natural flanks.
        let scar_point = bevy::math::Vec3::new(0.0, 5.5, 0.0);
        let scar_point_span = scar_span(scar_point.y);
        let scar_recess = recipe.failure_recess_local_z(scar_point);
        let left_intact_recess = recipe.failure_recess_local_z(bevy::math::Vec3::new(
            scar_point_span.0 - 1.2,
            scar_point.y,
            0.0,
        ));
        let right_intact_recess = recipe.failure_recess_local_z(bevy::math::Vec3::new(
            scar_point_span.1 + 1.2,
            scar_point.y,
            0.0,
        ));
        assert!(
            scar_recess > left_intact_recess + 0.35 && scar_recess > right_intact_recess + 0.35,
            "failure plane should be materially recessed from both intact sides: scar={scar_recess}, left={left_intact_recess}, right={right_intact_recess}"
        );
        assert!(
            scar_recess <= 0.65,
            "failure plane must remain a shallow recess rather than splitting the scarp: {scar_recess}"
        );
        let mut scar_back = scar_point;
        scar_back.z = recipe.face_surface_local_z(scar_back) + 0.45;
        assert!(
            recipe.signed_distance(recipe.local_to_world(scar_back)) < 0.0,
            "recess must retain a solid rendered back plane rather than an aperture"
        );
        let mut previous_face: Option<f32> = None;
        for x_step in -9_i16..=9 {
            let x = f32::from(x_step) * 0.35;
            let point = bevy::math::Vec3::new(x, 5.5, 0.0);
            let face = recipe.face_surface_local_z(point);
            if let Some(previous) = previous_face {
                assert!(
                    (face - previous).abs() <= 0.85,
                    "scar edge changed {face_minus_previous}m in one Surface Nets sample near x={x}",
                    face_minus_previous = face - previous,
                );
            }
            previous_face = Some(face);
        }
        for x_step in -12_i16..=12 {
            let x = f32::from(x_step) * 0.25;
            for height_fraction in [0.50_f32, 0.68, 0.84] {
                let y = recipe.local_crest_height(x) * height_fraction;
                let mut behind_face = bevy::math::Vec3::new(x, y, 0.0);
                behind_face.z = recipe.face_surface_local_z(behind_face) + 0.45;
                assert!(
                    recipe.signed_distance(recipe.local_to_world(behind_face)) < 0.0,
                    "failure/bedding surface became perforated at x={x}, y={y}"
                );
            }
        }
        assert!(
            recipe.local_crest_height(0.0) < recipe.local_crest_height(4.0),
            "joint-bounded failure should lower the local crest"
        );
        assert!(
            recipe.local_crest_height(4.0) - recipe.local_crest_height(0.0) <= 1.0,
            "crest-open failure notch must not divide the bluff into separate lobes"
        );
        for x in [-6.0_f32, -4.0, 4.0, 6.0] {
            let y = recipe.local_crest_height(x) * 0.55;
            let mut local = bevy::math::Vec3::new(x, y, 0.0);
            local.z = recipe.face_surface_local_z(local);
            assert!(
                recipe.signed_distance(recipe.local_to_world(local)).abs() <= 0.000_1,
                "shared authored face evaluator must lie on the scalar field"
            );
        }
    }

    #[test]
    fn river_bluff_planform_undercut_and_shoulders_are_landform_scale() {
        let recipe = bluff_recipe();
        let centre = recipe.top_front_local_z(0.0);
        let left = recipe.top_front_local_z(-8.0);
        let right = recipe.top_front_local_z(8.0);
        assert!(centre > left + 0.5 && centre > right + 0.5);
        assert!(
            (left - right).abs() > 0.5,
            "planform must be visibly asymmetric in overhead"
        );

        for x in [-9.0_f32, -6.0, 7.0, 10.0] {
            let toe = recipe.face_surface_local_z(bevy::math::Vec3::new(x, 0.35, 0.0));
            let crest = recipe.face_surface_local_z(bevy::math::Vec3::new(
                x,
                recipe.local_crest_height(x) - 0.20,
                0.0,
            ));
            assert!(
                crest - toe >= 2.6,
                "intact river-bluff flank must retreat several metres from toe to crest: x={x}, run={}",
                crest - toe,
            );
        }
        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let collapse_run = recipe.scarp_horizontal_run_local(collapse_x);
        assert!(
            (0.45..=1.20).contains(&collapse_run),
            "localized collapse sector must remain near-vertical: {collapse_run}"
        );
        let near_vertical_samples = (-56_i16..=56)
            .map(|step| f32::from(step) * 0.25)
            .filter(|x| recipe.scarp_horizontal_run_local(*x) < 1.5)
            .collect::<Vec<_>>();
        let near_vertical_width = near_vertical_samples
            .last()
            .zip(near_vertical_samples.first())
            .map_or(0.0, |(last, first)| last - first + 0.25);
        assert!(
            (4.0..=7.0).contains(&near_vertical_width),
            "near-vertical sector must be localized to the collapse: {near_vertical_width}m"
        );
        assert!(
            recipe.scarp_horizontal_run_local(-10.0) != recipe.scarp_horizontal_run_local(10.0),
            "opposing returned shoulders must retain asymmetric macroform"
        );

        let toe = bevy::math::Vec3::new(collapse_x, 0.5, 0.0);
        let without_undercut = RiverBluffRecipe {
            undercut_depth_cm: 0,
            ..recipe
        };
        let toe_recess =
            recipe.face_surface_local_z(toe) - without_undercut.face_surface_local_z(toe);
        assert!(
            (0.45..=0.80).contains(&toe_recess),
            "localized toe undercut should be legible without becoming a cavern: {toe_recess}"
        );
        let quiet_toe = bevy::math::Vec3::new(collapse_x - 9.0, 0.5, 0.0);
        assert!(
            (recipe.face_surface_local_z(quiet_toe)
                - without_undercut.face_surface_local_z(quiet_toe))
            .abs()
                < 0.01
        );
        let review_flank_x = collapse_x + 0.35;
        let flank_lower =
            recipe.face_surface_local_z(bevy::math::Vec3::new(review_flank_x, 0.55, 0.0));
        let flank_lip =
            recipe.face_surface_local_z(bevy::math::Vec3::new(review_flank_x, 1.42, 0.0));
        assert!(
            flank_lower - flank_lip >= 0.30,
            "review flank must expose a shallow lip-to-recess setback: lower={flank_lower}, lip={flank_lip}"
        );
        let side_x = collapse_x + 1.45;
        let side_lower = recipe.face_surface_local_z(bevy::math::Vec3::new(side_x, 0.55, 0.0));
        let side_lip = recipe.face_surface_local_z(bevy::math::Vec3::new(side_x, 1.42, 0.0));
        assert!(
            (flank_lower - flank_lip) - (side_lower - side_lip) >= 0.25,
            "undercut must end in a readable shallow side silhouette rather than a tunnel mouth"
        );
        let mut strongest_projection = f32::INFINITY;
        let mut strongest_recess = f32::NEG_INFINITY;
        for x_step in -80_i16..=80 {
            for y_step in 0_u16..=180 {
                let displacement = recipe.bedding_displacement_local_z(bevy::math::Vec3::new(
                    f32::from(x_step) * 0.1,
                    f32::from(y_step) * 0.05,
                    0.0,
                ));
                strongest_projection = strongest_projection.min(displacement);
                strongest_recess = strongest_recess.max(displacement);
            }
        }
        assert!(strongest_projection <= -0.09);
        assert!(strongest_recess >= 0.07);
        assert_eq!(smooth_course_weight(4.14, 4.14, 0.42, 1.0), 1.0);
        assert_eq!(smooth_course_weight(3.74, 4.14, 0.42, 1.0), 1.0);
        assert_eq!(smooth_course_weight(4.54, 4.14, 0.42, 1.0), 1.0);
        assert!(
            1.0 / (f32::from(recipe.sample_spacing_cm) / 100.0) >= 3.5,
            "each continuous course edge must span at least three and a half authored samples"
        );
        let course_midpoint = (4.14 + 7.02) * 0.5;
        assert!(
            smooth_course_weight(course_midpoint, 4.14, 0.42, 1.0) <= 0.05
                && smooth_course_weight(course_midpoint, 7.02, 0.42, 1.0) <= 0.05,
            "finite courses must leave a low-weight interval between their full-strength interiors"
        );
        assert!(
            4.14 < 7.02,
            "weak course must remain directly beneath the resistant course"
        );
        let face_depth_separation = (strongest_recess - strongest_projection).abs();
        assert!(
            face_depth_separation >= 0.85,
            "resistant-over-weak system needs >=0.85m absolute face-depth separation: projection={strongest_projection}, recess={strongest_recess}, separation={face_depth_separation}"
        );
        let height = recipe.dimensions_metres().y;
        for x in [-6.0_f32, -4.0, 4.0, 6.0] {
            let resistant =
                recipe.bedding_displacement_local_z(bevy::math::Vec3::new(x, height * 0.72, 0.0));
            let weak =
                recipe.bedding_displacement_local_z(bevy::math::Vec3::new(x, height * 0.50, 0.0));
            assert!(
                resistant <= -0.10 && weak >= 0.05,
                "broad resistant/weak courses lost lateral coherence at x={x}: resistant={resistant}, weak={weak}"
            );
        }
        let intact_resistant =
            recipe.bedding_displacement_local_z(bevy::math::Vec3::new(4.0, height * 0.72, 0.0));
        let failed_resistant = recipe.bedding_displacement_local_z(bevy::math::Vec3::new(
            collapse_x,
            height * 0.72,
            0.0,
        ));
        assert!(
            failed_resistant >= intact_resistant + 0.07,
            "failure wedge must visibly interrupt the coherent resistant course: intact={intact_resistant}, failed={failed_resistant}"
        );
        let mut previous =
            recipe.bedding_displacement_local_z(bevy::math::Vec3::new(4.0, 0.0, 0.0));
        let mut previous_delta = 0.0_f32;
        for step in 1_u16..=26 {
            let y = f32::from(step) * 0.35;
            let displacement =
                recipe.bedding_displacement_local_z(bevy::math::Vec3::new(4.0, y, 0.0));
            let delta = displacement - previous;
            assert!(displacement.is_finite() && delta.is_finite());
            assert!(
                !(delta * previous_delta < 0.0
                    && delta.abs() > 0.20
                    && previous_delta.abs() > 0.20),
                "bedding formed a one-sample sign-flip spike around y={}: previous_delta={previous_delta}, delta={delta}",
                y - 0.35
            );
            previous_delta = delta;
            previous = displacement;
        }

        let mut input = fixture();
        input.playable = TerrainSampleGrid {
            width: 51,
            depth: 51,
            spacing_metres: 2.0,
            heights_metres: vec![0.0; 51 * 51],
            environment: vec![EnvironmentalSample::default(); 51 * 51],
        };
        input.terrain_patches = vec![TerrainPatchRecipe::RiverBluff(recipe)];
        let terrain = input.generate().unwrap().terrain;
        let flank_foreground =
            recipe.local_to_world(bevy::math::Vec3::new(review_flank_x, 0.0, flank_lip - 0.30));
        let flank_ground = terrain
            .height_at(bevy::math::Vec2::new(
                flank_foreground.x,
                flank_foreground.z,
            ))
            .unwrap();
        assert!(
            flank_ground <= 0.45,
            "lower bench/apron occluded the camera-readable undercut flank: height={flank_ground}"
        );
        // Sample the collar while it is still owned by the authored returned
        // shoulder. Farther rearward the recipe deliberately blends back into
        // the fixture's inherited broad terrace, where lateral heights should
        // converge instead of remaining monotonically tapered.
        let collar_z = recipe.center_metres().z + 17.0;
        let centre_terrace = terrain
            .height_at(bevy::math::Vec2::new(0.0, collar_z))
            .unwrap();
        let inner_shoulder = terrain
            .height_at(bevy::math::Vec2::new(10.0, collar_z))
            .unwrap();
        let returned_end = terrain
            .height_at(bevy::math::Vec2::new(14.0, collar_z))
            .unwrap();
        let outer_shoulder = terrain
            .height_at(bevy::math::Vec2::new(18.0, collar_z))
            .unwrap();
        assert!(centre_terrace > inner_shoulder);
        assert!(
            inner_shoulder > returned_end,
            "inner shoulder {inner_shoulder} must exceed returned end {returned_end}"
        );
        assert!(returned_end > outer_shoulder);
    }

    #[test]
    fn talus_apron_is_low_and_localized_to_missing_scar_volume() {
        let mut input = fixture();
        input.playable = TerrainSampleGrid {
            width: 51,
            depth: 51,
            spacing_metres: 2.0,
            heights_metres: vec![0.0; 51 * 51],
            environment: vec![EnvironmentalSample::default(); 51 * 51],
        };
        let mut recipe = bluff_recipe();
        recipe.collapse_offset_cm = 0;
        input.terrain_patches = vec![TerrainPatchRecipe::RiverBluff(recipe)];
        let generated = input.generate().unwrap();
        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let talus_toe_z = recipe.talus_toe_local_z();
        let talus_mid_z = talus_toe_z - f32::from(recipe.talus_depth_cm) / 200.0;
        let apron_world =
            recipe.local_to_world(bevy::math::Vec3::new(collapse_x, 0.0, talus_mid_z));
        let outside_world =
            recipe.local_to_world(bevy::math::Vec3::new(collapse_x + 12.0, 0.0, talus_mid_z));
        let apron = bevy::math::Vec2::new(apron_world.x, apron_world.z);
        let outside = bevy::math::Vec2::new(outside_world.x, outside_world.z);
        assert!(generated.terrain.height_at(apron).unwrap() <= 0.55);
        let mut elevated_samples = 0_usize;
        let mut maximum_apron = 0.0_f32;
        let mut previous_column: Option<f32> = None;
        for x_step in -5_i16..=5 {
            let local_x = f32::from(x_step);
            let local_z = talus_mid_z;
            let world = recipe.local_to_world(bevy::math::Vec3::new(local_x, 0.0, local_z));
            let height = generated
                .terrain
                .height_at(bevy::math::Vec2::new(world.x, world.z))
                .unwrap();
            assert!(height.is_finite() && height <= 1.85);
            if height > 0.08 {
                elevated_samples += 1;
            }
            if let Some(previous) = previous_column {
                assert!((height - previous).abs() <= 1.30);
            }
            previous_column = Some(height);
            maximum_apron = maximum_apron.max(height);
        }
        assert!(
            elevated_samples >= 5 && maximum_apron >= 0.45,
            "aggregate fan lost coarse-grid mass: elevated={elevated_samples}, maximum={maximum_apron}"
        );
        let fan_profile = [-4.0_f32, -2.0, 0.0, 2.0, 4.0].map(|local_x| {
            let world = recipe.local_to_world(bevy::math::Vec3::new(local_x, 0.0, talus_mid_z));
            generated
                .terrain
                .height_at(bevy::math::Vec2::new(world.x, world.z))
                .unwrap()
        });
        assert!(
            fan_profile[0] > fan_profile[1]
                && fan_profile[2] > fan_profile[1]
                && fan_profile[2] > fan_profile[3]
                && fan_profile[4] > fan_profile[3],
            "aggregated debris must preserve three separated coarse-grid lobes: {fan_profile:?}"
        );
        let inherited_ground = generated.ground.ground_at(outside).unwrap();
        assert_eq!(
            generated.ground.ground_at(apron).unwrap(),
            inherited_ground,
            "aggregate fan geometry must not create a categorical material patch"
        );
        let clear_flank_world = recipe.local_to_world(bevy::math::Vec3::new(
            collapse_x + 0.35,
            0.0,
            talus_toe_z - f32::from(recipe.talus_depth_cm) / 100.0 * 0.15,
        ));
        let clear_flank = bevy::math::Vec2::new(clear_flank_world.x, clear_flank_world.z);
        assert_eq!(
            recipe.debris_fan_height_local(
                collapse_x + 0.35,
                talus_toe_z - f32::from(recipe.talus_depth_cm) / 100.0 * 0.15,
            ),
            0.0,
            "the authoritative fan must contribute no height on the undercut flank"
        );
        assert!(
            generated.terrain.height_at(clear_flank).unwrap() <= 0.65,
            "the debris-free flank must remain on the ordinary lower bench"
        );
        assert_eq!(
            generated.ground.ground_at(clear_flank).unwrap(),
            inherited_ground,
            "the debris-free flank must inherit ordinary surrounding ground"
        );
        for obstacle in &generated.obstacles {
            let GeneratedObstacle::Tree { x, z } = *obstacle else {
                continue;
            };
            let world = bevy::math::Vec3::new(
                f32::from(x) * input.playable.spacing_metres - 50.0,
                0.0,
                f32::from(z) * input.playable.spacing_metres - 50.0,
            );
            let local = recipe.world_to_local(world);
            let brink = recipe.top_front_local_z(local.x.clamp(-14.0, 14.0));
            assert!(
                local.x.abs() > recipe.dimensions_metres().x * 0.5 + 7.0
                    || local.z < brink - 30.0
                    || local.z > recipe.dimensions_metres().z + 5.0,
                "generated woody obstacle entered the beauty/profile evidence corridor: {local:?}"
            );
        }
    }

    #[test]
    fn river_bluff_heightfield_triangles_do_not_cross_the_face_envelope() {
        let mut input = fixture();
        input.playable = TerrainSampleGrid {
            width: 51,
            depth: 51,
            spacing_metres: 2.0,
            heights_metres: vec![0.0; 51 * 51],
            environment: vec![EnvironmentalSample::default(); 51 * 51],
        };
        let recipe = bluff_recipe();
        input.terrain_patches = vec![TerrainPatchRecipe::RiverBluff(recipe)];
        let generated = input.generate().unwrap();
        let width = generated.terrain.grid_width();
        let depth = generated.terrain.grid_depth();
        let spacing = generated.terrain.grid_scale();
        let world_width = (width - 1) as f32 * spacing;
        let world_depth = (depth - 1) as f32 * spacing;
        let vertex = |x: usize, z: usize| {
            let world_x = x as f32 * spacing - world_width * 0.5;
            let world_z = z as f32 * spacing - world_depth * 0.5;
            let height = generated
                .terrain
                .height_at(bevy::math::Vec2::new(world_x, world_z))
                .unwrap();
            recipe.world_to_local(bevy::math::Vec3::new(world_x, height, world_z))
        };
        let half_patch_width = recipe.dimensions_metres().x * 0.5;
        let ownership_front = -f32::from(recipe.talus_depth_cm) / 100.0 - spacing * 2.0;
        let visible_toe = recipe.center_metres().y + 0.55;
        let implicit_termination =
            half_patch_width - f32::from(recipe.sample_spacing_cm) / 100.0 * 2.5;
        let outer_x = implicit_termination + spacing;
        let termination_z = recipe
            .crest_brink_local_z(-implicit_termination)
            .max(recipe.crest_brink_local_z(-outer_x))
            + spacing;
        let termination = recipe.local_to_world(bevy::math::Vec3::new(
            -implicit_termination,
            0.0,
            termination_z,
        ));
        let outer = recipe.local_to_world(bevy::math::Vec3::new(-outer_x, 0.0, termination_z));
        let termination_height = generated
            .terrain
            .height_at(bevy::math::Vec2::new(termination.x, termination.z))
            .unwrap();
        let outer_height = generated
            .terrain
            .height_at(bevy::math::Vec2::new(outer.x, outer.z))
            .unwrap();
        let returned_contact_upper = recipe.center_metres().y
            + recipe.local_crest_height(-recipe.implicit_collision_half_width());
        assert!(
            termination_height.is_finite() && termination_height <= returned_contact_upper + 0.5,
            "heightfield-owned returned shoulder exceeded its central contact elevation: height={termination_height}, contact={returned_contact_upper}, x={implicit_termination}, z={termination_z}",
        );
        assert!(
            (termination_height - outer_height).abs() <= spacing,
            "heightfield-owned returned shoulder must remain continuous toward inherited terrain: termination={termination_height}, outer={outer_height}"
        );

        for sign in [-1.0_f32, 1.0] {
            for distance in [8.0_f32, 10.0, 12.0, 14.0] {
                let local_x = sign * distance;
                let crest = recipe.local_crest_height(local_x);
                let brink = recipe.crest_brink_local_z(local_x);
                let world =
                    recipe.local_to_world(bevy::math::Vec3::new(local_x, 0.0, brink + spacing));
                let height = generated
                    .terrain
                    .height_at(bevy::math::Vec2::new(world.x, world.z))
                    .unwrap();
                assert!(
                    height <= returned_contact_upper + 0.5,
                    "heightfield-owned returned shoulder exceeded its shared contact elevation: x={local_x}, height={height}, crest={crest}, contact={returned_contact_upper}"
                );
            }
        }

        // Returned shoulders are ordinary heightfield terrain. Every coarse-grid edge in
        // their visible rise must therefore remain a slope rather than reproducing the
        // central buried near-step as a grass-topped wall or lateral slab.
        for z in 0..depth {
            for x in 0..width {
                let point = vertex(x, z);
                if !(8.0..=21.0).contains(&point.x.abs())
                    || !(ownership_front..=recipe.dimensions_metres().z + 12.0).contains(&point.z)
                {
                    continue;
                }
                for (next_x, next_z) in [(x + 1, z), (x, z + 1)] {
                    if next_x >= width || next_z >= depth {
                        continue;
                    }
                    let next = vertex(next_x, next_z);
                    if !(8.0..=21.0).contains(&next.x.abs())
                        || !(ownership_front..=recipe.dimensions_metres().z + 12.0)
                            .contains(&next.z)
                    {
                        continue;
                    }
                    let rise = (next.y - point.y).abs();
                    assert!(
                        rise <= spacing,
                        "heightfield-owned returned shoulder formed a steep wall: {point:?} -> {next:?}, rise={rise}"
                    );
                }
                if point.x.abs() >= 20.0 {
                    assert!(
                        point.y.abs() <= 0.35,
                        "returned shoulder failed to converge to inherited terrain: {point:?}"
                    );
                }
            }
        }

        for local_x in [-4.0_f32, 0.0, 4.0] {
            let crest = recipe.local_crest_height(local_x);
            let brink = recipe.crest_brink_local_z(local_x);
            let neighbour_envelope = [-spacing, 0.0, spacing]
                .into_iter()
                .map(|offset| {
                    recipe.maximum_face_local_z(
                        (local_x + offset).clamp(-half_patch_width, half_patch_width),
                    )
                })
                .fold(f32::NEG_INFINITY, f32::max);
            let transition_contact = neighbour_envelope + spacing * 1.1;
            let rear_contact = transition_contact + spacing;
            let world = recipe.local_to_world(bevy::math::Vec3::new(local_x, 0.0, rear_contact));
            let height = generated
                .terrain
                .height_at(bevy::math::Vec2::new(world.x, world.z))
                .unwrap();
            assert!(
                height >= crest - 0.75,
                "heightfield upper ground failed to meet the face boundary from behind: x={local_x}, height={height}, crest={crest}, local envelope={}, neighbour envelope={neighbour_envelope}, transition contact={transition_contact}, sample z={rear_contact}",
                recipe.maximum_face_local_z(local_x),
            );
            // There is no local render collar in the whole-tile architecture. The authoritative
            // heightfield contact instead stays one guarded coarse cell behind the deepest face
            // in the neighbouring stencil; the exhaustive triangle gate below proves that this
            // conservative contact never crosses the visible scarp.
            assert!(transition_contact >= neighbour_envelope);
            assert!(
                transition_contact - neighbour_envelope <= spacing * 1.25,
                "heightfield contact drifted beyond its conservative neighbour stencil: x={local_x}, brink={brink}, envelope={neighbour_envelope}, contact={transition_contact}"
            );
        }

        for z in 0..depth - 1 {
            for x in 0..width - 1 {
                let corners = [
                    vertex(x, z),
                    vertex(x + 1, z),
                    vertex(x, z + 1),
                    vertex(x + 1, z + 1),
                ];
                for triangle in [[0, 1, 2], [1, 3, 2]] {
                    let points = triangle.map(|index| corners[index]);
                    if !points.iter().all(|point| {
                        point.x.abs() <= recipe.implicit_collision_half_width()
                            && point.z >= ownership_front
                    }) {
                        continue;
                    }
                    if points
                        .iter()
                        .filter(|point| recipe.local_crest_height(point.x) >= 3.0)
                        .count()
                        < 2
                    {
                        // Low returned ends are deliberately buried beneath
                        // the heightfield shoulder and are not rendered.
                        continue;
                    }
                    let below_toe = points.iter().all(|point| point.y <= visible_toe);
                    let behind_face = points
                        .iter()
                        .all(|point| point.z > recipe.maximum_face_local_z(point.x));
                    let safely_in_front_of_face = points
                        .iter()
                        .all(|point| point.z < recipe.minimum_face_local_z(point.x) - 0.75);
                    assert!(
                        below_toe || behind_face || safely_in_front_of_face,
                        "heightfield triangle crossed the visible scarp: {points:?}"
                    );
                }
            }
        }

        for x in [-6.0_f32, -3.0, 0.0, 3.0, 6.0] {
            let envelope = recipe.maximum_face_local_z(x);
            for sample in 0..=64 {
                let y = recipe.local_crest_height(x) * sample as f32 / 64.0;
                assert!(
                    envelope > recipe.face_surface_local_z(bevy::math::Vec3::new(x, y, 0.0)),
                    "face envelope missed an authored failure or undercut sample"
                );
            }
        }
    }

    #[test]
    fn generation_and_digest_are_reproducible() {
        let input = fixture();
        let first = input.generate().unwrap();
        let second = input.generate().unwrap();
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.obstacles, second.obstacles);
        assert_eq!(
            first.terrain.height_at(bevy::math::Vec2::ZERO),
            second.terrain.height_at(bevy::math::Vec2::ZERO)
        );
        assert_eq!(
            first.repairs.microrelief_adjusted_samples,
            second.repairs.microrelief_adjusted_samples
        );
    }

    #[test]
    fn microrelief_is_bounded_deterministic_and_preserves_the_combat_corridor() {
        let width = 25;
        let depth = 25;
        let environment = vec![
            EnvironmentalSample {
                hilly_bps: 10_000,
                ..Default::default()
            };
            width * depth
        ];
        let mut first = vec![0.0; width * depth];
        let mut second = first.clone();
        let first_count =
            add_authoritative_microrelief(91, width, depth, 1.0, &mut first, &environment);
        let second_count =
            add_authoritative_microrelief(91, width, depth, 1.0, &mut second, &environment);
        assert_eq!(first, second);
        assert_eq!(first_count, second_count);
        assert!(first_count > 0);
        assert!(first.iter().all(|height| height.abs() <= 0.275 + 0.001));
        assert!((0..width).all(|x| first[(depth / 2) * width + x] == 0.0));
    }

    #[test]
    fn coarse_source_grid_is_upsampled_without_changing_extent() {
        let source = TerrainSampleGrid {
            width: 3,
            depth: 2,
            spacing_metres: 12.5,
            heights_metres: vec![0.0, 1.0, 2.0, 2.0, 3.0, 4.0],
            environment: vec![EnvironmentalSample::default(); 6],
        };
        let (width, depth, spacing, heights, environment) = upsample_playable_grid(&source);
        assert!(spacing <= 2.0);
        assert_eq!((width - 1) as f32 * spacing, 25.0);
        assert_eq!((depth - 1) as f32 * spacing, 12.5);
        assert_eq!(heights.len(), width * depth);
        assert_eq!(environment.len(), width * depth);
        assert!((heights[(depth - 1) * width + width - 1] - 4.0).abs() < 0.0001);
    }

    #[test]
    fn rejects_versions_bounds_and_malformed_sample_counts() {
        let mut input = fixture();
        input.schema_version += 1;
        assert!(input.validate().is_err());
        input = fixture();
        input.playable.heights_metres.pop();
        assert!(input.validate().is_err());
        input = fixture();
        input.latitude_microdegrees = 90_000_001;
        assert!(input.validate().is_err());
    }

    #[test]
    fn unknown_json_fields_fail_closed() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("tick_state".into(), 1.into());
        assert!(serde_json::from_value::<TacticalSceneInput>(value).is_err());
    }

    #[test]
    fn oversized_scene_file_fails_before_deserialization() {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "adventuresim-oversized-scene-{}-{nonce}.json",
            std::process::id()
        ));
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_SCENE_INPUT_BYTES + 1).unwrap();
        drop(file);
        let error = TacticalSceneInput::load(&path).unwrap_err();
        fs::remove_file(path).unwrap();
        assert!(error.to_string().contains("32 MiB"));
    }

    #[test]
    fn committed_synthetic_fixture_catalog_uses_the_production_format() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tactical-scenes");
        let names = [
            "flat-dry-grassland",
            "steep-open-hillside",
            "dense-woodland",
            "sparse-woodland",
            "saturated-wetland",
            "cultivated-roadside",
            "snow-covered-ground",
            "heavy-rain-high-wind",
            "valley-distant-ridge",
            "narrow-peak-lod-boundary",
            "playability-repair-required",
            "river-bluff-cliff",
        ];
        for name in names {
            let input = TacticalSceneInput::load(&root.join(format!("{name}.json"))).unwrap();
            assert_eq!(input.source, SceneSource::SyntheticFixture(name.into()));
            assert_eq!(input.absolute_minute % 1_440, 10 * 60);
            let generated = input.generate().unwrap();
            assert_eq!(generated.terrain.width(), 100.0);
            assert!(generated.terrain.grid_scale() <= 2.0);
            assert!(generated.repairs.upsampled_height_samples > 0);
            assert_eq!(input.vista.lods.len(), 3);
            let playable_center =
                input.playable.heights_metres[usize::from(input.playable.depth / 2)
                    * usize::from(input.playable.width)
                    + usize::from(input.playable.width / 2)];
            for lod in &input.vista.lods {
                let vista_center = lod.heights_metres[usize::from(lod.depth / 2)
                    * usize::from(lod.width)
                    + usize::from(lod.width / 2)];
                assert!(
                    (vista_center - playable_center).abs() < 0.001,
                    "{name} vista LOD {} must share the playable height datum",
                    lod.level
                );
            }
            let horizon = input.vista.lods.last().unwrap();
            assert_eq!(
                f32::from(horizon.width - 1) * horizon.spacing_metres,
                50_000.0
            );
        }
    }

    #[test]
    fn committed_river_bluff_has_broad_floodplain_and_rear_terrace_context() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/tactical-scenes/river-bluff-cliff.json");
        let generated = TacticalSceneInput::load(&path).unwrap().generate().unwrap();
        let lower = generated
            .terrain
            .height_at(bevy::math::Vec2::new(30.0, 0.0))
            .unwrap();
        let upper = generated
            .terrain
            .height_at(bevy::math::Vec2::new(30.0, 45.0))
            .unwrap();
        assert!(lower.abs() < 0.5, "front context must remain a floodplain");
        assert!(upper > 7.0, "rear context must read as a broad terrace");
    }

    #[test]
    fn narrow_peak_is_preserved_on_the_regional_horizon_lod_boundary() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/tactical-scenes/narrow-peak-lod-boundary.json");
        let input = TacticalSceneInput::load(&path).unwrap();
        let regional = &input.vista.lods[1];
        let horizon = &input.vista.lods[2];
        let regional_peak = regional.heights_metres
            [usize::from(regional.depth / 2) * usize::from(regional.width) + 20];
        let horizon_peak = horizon.heights_metres
            [usize::from(horizon.depth / 2) * usize::from(horizon.width) + 30];
        assert!(regional_peak >= 899.0);
        assert!((regional_peak - horizon_peak).abs() < 0.001);
    }

    #[test]
    fn committed_obstacle_fixtures_exercise_sparse_trees_and_hilly_rocks() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tactical-scenes");
        let flat = TacticalSceneInput::load(&root.join("flat-dry-grassland.json"))
            .unwrap()
            .generate()
            .unwrap();
        let sparse_input = TacticalSceneInput::load(&root.join("sparse-woodland.json")).unwrap();
        let sparse_spacing = sparse_input.playable.spacing_metres;
        let sparse = sparse_input.generate().unwrap();
        let hillside = TacticalSceneInput::load(&root.join("steep-open-hillside.json"))
            .unwrap()
            .generate()
            .unwrap();
        assert!(flat.obstacles.is_empty());
        assert!(
            sparse
                .obstacles
                .iter()
                .any(|obstacle| matches!(obstacle, GeneratedObstacle::Tree { .. }))
        );
        assert!(
            hillside
                .obstacles
                .iter()
                .any(|obstacle| matches!(obstacle, GeneratedObstacle::Rock { .. }))
        );
        assert!(
            flat.ground.cover_count(GroundCover::TallGrass)
                > flat.ground.cover_count(GroundCover::LeafLitter)
        );
        for obstacle in &sparse.obstacles {
            let GeneratedObstacle::Tree { x, z } = *obstacle else {
                continue;
            };
            let position = bevy::math::Vec2::new(
                f32::from(x) * sparse_spacing - sparse.terrain.width() * 0.5,
                f32::from(z) * sparse_spacing - sparse.terrain.depth() * 0.5,
            );
            assert_eq!(
                sparse.ground.ground_at(position).unwrap().cover,
                GroundCover::LeafLitter
            );
        }
        assert!(sparse.ground.cover_count(GroundCover::LeafLitter) > 0);
        assert!(
            sparse.ground.cover_count(GroundCover::TallGrass)
                > sparse.ground.cover_count(GroundCover::LeafLitter),
            "sparse crowns should retain a dappled grass matrix"
        );
    }

    #[test]
    fn tree_leaf_litter_tapers_from_a_dense_trunk_core() {
        assert_eq!(tree_leaf_litter_probability(0.0), 1.0);
        assert_eq!(
            tree_leaf_litter_probability(TREE_DENSE_LEAF_LITTER_RADIUS_METRES),
            1.0
        );
        let inner = tree_leaf_litter_probability(3.0);
        let middle = tree_leaf_litter_probability(4.0);
        let edge = tree_leaf_litter_probability(TREE_CANOPY_GROUND_RADIUS_METRES);
        assert!(1.0 > inner && inner > middle && middle > edge);
        assert!((edge - 0.12).abs() < f32::EPSILON);
    }

    #[test]
    fn obstacle_kind_has_a_stable_wire_round_trip() {
        let recipe = rock_recipe(42);
        for obstacle in [SceneObstacle::Tree, SceneObstacle::Rock(recipe)] {
            let bytes = postcard::to_allocvec(&obstacle).unwrap();
            assert_eq!(
                postcard::from_bytes::<SceneObstacle>(&bytes).unwrap(),
                obstacle
            );
        }
    }

    #[test]
    fn generated_rock_recipes_are_deterministic_and_fit_the_collision_proxy() {
        for seed in [0, 1, 42, u64::MAX] {
            let recipe = rock_recipe(seed);
            assert_eq!(recipe, rock_recipe(seed));
            assert_eq!(recipe.seed, seed);
            assert!(
                recipe
                    .dimensions_cm
                    .iter()
                    .all(|dimension| *dimension <= recipe.collision_radius_cm * 2)
            );
            assert_eq!(recipe.collision_radius_metres(), ROCK_RADIUS_METRES);
        }
    }

    #[test]
    fn invalid_fixture_is_repaired_deterministically_into_a_connected_battlefield() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/tactical-scenes");
        let input =
            TacticalSceneInput::load(&root.join("playability-repair-required.json")).unwrap();
        let first = input.generate().unwrap();
        let second = input.generate().unwrap();
        assert_eq!(first.repairs, second.repairs);
        assert!(first.repairs.adjusted_height_samples > 0);
        assert!(first.repairs.repaired_water_samples > 0);

        let width = first.terrain.grid_width();
        let depth = first.terrain.grid_depth();
        let spacing = first.terrain.grid_scale();
        let world_width = (width - 1) as f32 * spacing;
        let world_depth = (depth - 1) as f32 * spacing;
        let height = |x: usize, z: usize| {
            first
                .terrain
                .height_at(bevy::math::Vec2::new(
                    x as f32 * spacing - world_width * 0.5,
                    z as f32 * spacing - world_depth * 0.5,
                ))
                .unwrap()
        };
        for z in 0..depth {
            for x in 0..width {
                if x + 1 < width
                    && is_reserved_playability_cell(x, z, width, depth)
                    && is_reserved_playability_cell(x + 1, z, width, depth)
                {
                    assert!(
                        (height(x, z) - height(x + 1, z)).abs()
                            <= spacing * MAX_PLAYABLE_GRADE + 0.001,
                        "reserved horizontal edge ({x}, {z}) has {} m step over {} m",
                        (height(x, z) - height(x + 1, z)).abs(),
                        spacing
                    );
                }
                if z + 1 < depth
                    && is_reserved_playability_cell(x, z, width, depth)
                    && is_reserved_playability_cell(x, z + 1, width, depth)
                {
                    assert!(
                        (height(x, z) - height(x, z + 1)).abs()
                            <= spacing * MAX_PLAYABLE_GRADE + 0.001,
                        "reserved vertical edge ({x}, {z}) has {} m step over {} m",
                        (height(x, z) - height(x, z + 1)).abs(),
                        spacing
                    );
                }
            }
        }
        assert!(first.obstacles.iter().all(|obstacle| match *obstacle {
            GeneratedObstacle::Tree { x, z } => {
                !is_tree_camera_clearance_cell(usize::from(x), usize::from(z), depth)
            }
            GeneratedObstacle::Rock { x, z, .. } => {
                !is_reserved_playability_cell(usize::from(x), usize::from(z), width, depth)
            }
        }));
    }

    #[test]
    fn reserved_playability_corridor_covers_the_spawn_camera_envelope() {
        let width = 9;
        let depth = 9;
        for x in 0..width {
            for z in 3..=5 {
                assert!(is_tree_camera_clearance_cell(x, z, depth));
            }
            assert!(!is_tree_camera_clearance_cell(x, 2, depth));
            assert!(!is_tree_camera_clearance_cell(x, 6, depth));
        }
        assert!(!is_reserved_playability_cell(0, 3, width, depth));
        assert!(is_reserved_playability_cell(2, 3, width, depth));
    }
}
