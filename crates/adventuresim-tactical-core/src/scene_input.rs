//! Versioned, bounded input for deterministic tactical scene generation.
//!
//! This is deliberately data-only. Production dispatchers can sample the
//! imported terrain pack into it, while tactical-only tools can serialize a
//! synthetic fixture. Short-lived servers consume the identical format and
//! never need access to the continental source pack.

use std::{fs, path::Path};

use adventuresim_core::{
    strategic_time::MINUTES_PER_DAY,
    weather::{Precipitation, WEATHER_RULES_VERSION, WeatherSnapshot},
};
use adventuresim_world_schema::{BASIS_POINTS_PER_WHOLE, UnitBasisPoints};
use bevy::prelude::Component;
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    city_layout::{CityStreetPatch, CityYardPatch, MAX_CITY_STREET_PATCHES, MAX_CITY_YARD_PATCHES},
    scene::{GroundCover, GroundSubstrate, GroundSurface, SceneGround, SceneTerrain},
    volumetric_terrain::{SceneTerrainPatch, TerrainLandformRecipe},
};

use crate::scene_ground::build_scene_ground;
#[cfg(test)]
use crate::scene_ground::tree_leaf_litter_probability;

pub(crate) mod buildings;
mod generation;

pub use buildings::{
    BuildingOrientation, DistantBuildingPlacement, GeneratedBuilding, SceneBuilding, SceneDoor,
    SceneWindow, TacticalBuildingPlacement,
};

pub const TACTICAL_SCENE_SCHEMA_VERSION: u16 = 11;
pub const TACTICAL_SCENE_GENERATION_VERSION: u16 = 27;
pub const MAX_SCENE_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const TREE_TRUNK_RADIUS_METRES: f32 = 0.35;
pub const TREE_TRUNK_HEIGHT_METRES: f32 = 5.0;
/// Conservative ground footprint of the generated English-oak crown.
pub const TREE_CANOPY_GROUND_RADIUS_METRES: f32 = 5.75;
/// The trunk base is reliably leaf-covered; the outer crown uses a tapered
/// mosaic so sparse woodland does not stamp grass-free canopy discs.
pub(crate) const TREE_DENSE_LEAF_LITTER_RADIUS_METRES: f32 = 2.25;
pub const ROCK_RADIUS_METRES: f32 = 0.75;
const MAX_PLAYABLE_SIDE: usize = 601;
const MAX_VISTA_LEVELS: usize = 8;
const MAX_VISTA_SAMPLES: usize = 2_000_000;
const MAX_TEMPLATE_BYTES: usize = 128;
const MAX_SOURCE_ID_BYTES: usize = 128;
const MAX_PLAYABLE_GRADE: f32 = 0.65;
const ROCK_PLACEMENT_DOMAIN: u64 = 0x52cc_5f1b_d391_a739;
const ROCK_LITHOLOGY_DOMAIN: u64 = 0x6c69_7468_6f6c_6f67;
const ROCK_DIMENSION_AXIS_STRIDE: u64 = 0x9e37_79b9_7f4a_7c15;
const MICRORELIEF_FINE_DOMAIN: u64 = 0x8f3f_73b5_cf1c_9ade;
pub(crate) const TREE_LEAF_LITTER_DOMAIN: u64 = 0x001e_af11_77e2;
const AUTHORITATIVE_DETAIL_SPACING_METRES: f32 = 0.5;
const DETAIL_RELIEF_MINIMUM_METRES: f32 = -0.075;
const DETAIL_RELIEF_MAXIMUM_METRES: f32 = 0.105;

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
    pub lunar_phase_minute: u64,
    pub absolute_elevation_metres: i16,
    pub playable: TerrainSampleGrid,
    pub landform: Option<TerrainLandformRecipe>,
    pub streets: Vec<CityStreetPatch>,
    pub yards: Vec<CityYardPatch>,
    pub buildings: Vec<TacticalBuildingPlacement>,
    pub distant_buildings: Vec<DistantBuildingPlacement>,
    pub vista: VistaSample,
    pub weather: WeatherSnapshot,
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
    pub lunar_phase_minute: u64,
    pub absolute_elevation_metres: i16,
    pub weather: WeatherSnapshot,
    pub canopy_bps: u16,
    pub wetland_bps: u16,
    pub cultivation_bps: u16,
    pub water_bps: u16,
    pub hilly_bps: u16,
}

/// Explicit environment profiles for deterministic tactical-only fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneEnvironmentFixture {
    TemperateHills,
}

impl SceneEnvironmentFixture {
    pub fn snapshot(self, scene_digest: impl Into<String>) -> SceneEnvironment {
        match self {
            Self::TemperateHills => SceneEnvironment {
                scene_digest: scene_digest.into(),
                generation_version: TACTICAL_SCENE_GENERATION_VERSION,
                latitude_microdegrees: 53_500_000,
                longitude_microdegrees: 10_000_000,
                absolute_minute: MINUTES_PER_DAY / 2,
                lunar_phase_minute: MINUTES_PER_DAY / 2,
                absolute_elevation_metres: 20,
                weather: WeatherSnapshot {
                    rules_version: WEATHER_RULES_VERSION,
                    interval_start_minute: 0,
                    cell_latitude: 0,
                    cell_longitude: 0,
                    temperature_deci_c: 100,
                    wind_speed_bps: 1_500,
                    precipitation: Precipitation::Clear,
                    intensity_bps: 0,
                    ground_moisture_bps: 0,
                    snow_cover_bps: 0,
                    atmosphere: Default::default(),
                },
                canopy_bps: 1_200,
                wetland_bps: 0,
                cultivation_bps: 0,
                water_bps: 0,
                hilly_bps: 7_000,
            },
        }
    }
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
    pub terrain_patch: Option<SceneTerrainPatch>,
    pub buildings: Vec<GeneratedBuilding>,
    pub repairs: SceneRepairReport,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneRepairReport {
    pub upsampled_height_samples: u32,
    pub microrelief_adjusted_samples: u32,
    pub adjusted_height_samples: u32,
    pub repaired_water_samples: u32,
    pub removed_corridor_obstacles: u32,
    pub levelled_building_samples: u32,
    pub removed_building_obstacles: u32,
}

impl SceneRepairReport {
    pub const fn was_repaired(self) -> bool {
        self.adjusted_height_samples != 0
            || self.repaired_water_samples != 0
            || self.removed_corridor_obstacles != 0
            || self.levelled_building_samples != 0
            || self.removed_building_obstacles != 0
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
        crate::scene_fault::validate(self.landform, &self.playable)?;
        if self.streets.len() > MAX_CITY_STREET_PATCHES
            || self.streets.iter().any(|street| !street.is_valid())
        {
            return invalid("scene street surfaces are invalid or exceed their bound");
        }
        if self.yards.len() > MAX_CITY_YARD_PATCHES
            || self.yards.iter().any(|yard| !yard.is_valid())
        {
            return invalid("scene yard surfaces are invalid or exceed their bound");
        }
        buildings::validate_building_placements(&self.buildings)?;
        buildings::validate_distant_building_placements(&self.distant_buildings)?;
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
            lunar_phase_minute: self.lunar_phase_minute,
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

#[derive(Clone, Copy)]
struct TerrainShapeSample {
    downhill: bevy::math::Vec2,
    slope: f32,
    concavity: f32,
}

#[derive(Clone, Copy)]
struct TerrainRockInfluence {
    centre: bevy::math::Vec2,
    radius: f32,
}

/// Builds the one high-resolution surface used by height queries, IK,
/// rendering, and the server heightfield collider. The original grid is kept
/// implicitly as `SceneTerrain`'s coarse render LOD.
fn refine_authoritative_terrain(
    seed: u64,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    obstacles: &[GeneratedObstacle],
    obstacle_spacing: f32,
    moisture_bps: u16,
    building_pads: &[buildings::BuildingPad],
) -> Result<SceneTerrain, SceneInputError> {
    let half_extent = bevy::math::Vec2::new(terrain.width(), terrain.depth()) * 0.5;
    let mut trees = Vec::new();
    let mut rocks = Vec::new();
    for obstacle in obstacles {
        match *obstacle {
            GeneratedObstacle::Tree { x, z } => trees.push(
                bevy::math::Vec2::new(
                    f32::from(x) * obstacle_spacing,
                    f32::from(z) * obstacle_spacing,
                ) - half_extent,
            ),
            GeneratedObstacle::Rock { x, z, recipe } => rocks.push(TerrainRockInfluence {
                centre: bevy::math::Vec2::new(
                    f32::from(x) * obstacle_spacing,
                    f32::from(z) * obstacle_spacing,
                ) - half_extent,
                radius: recipe.collision_radius_metres(),
            }),
        }
    }
    let detail_seed = seed ^ 0x7465_7272_6169_6e64;
    let mut terrain = terrain
        .refined(AUTHORITATIVE_DETAIL_SPACING_METRES, |point, base_height| {
            if let Some(pad) = building_pads
                .iter()
                .find(|pad| pad.contains_level_ground(point))
            {
                pad.elevation_metres
            } else if building_pads.iter().any(|pad| pad.contains_apron(point)) {
                base_height
            } else {
                base_height
                    + authoritative_surface_relief(
                        detail_seed,
                        point,
                        terrain,
                        ground,
                        moisture_bps,
                        &trees,
                        &rocks,
                    )
            }
        })
        .ok_or_else(|| {
            SceneInputError::Validation("authoritative detail terrain is invalid".into())
        })?;
    terrain.constrain_max_grade(MAX_PLAYABLE_GRADE);
    terrain.rewrite_heights(|point, height| {
        building_pads
            .iter()
            .find(|pad| pad.contains_level_ground(point))
            .map_or(height, |pad| pad.elevation_metres)
    });
    Ok(terrain)
}

fn authoritative_surface_relief(
    seed: u64,
    point: bevy::math::Vec2,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    moisture_bps: u16,
    tree_positions: &[bevy::math::Vec2],
    rock_influences: &[TerrainRockInfluence],
) -> f32 {
    let surface = ground.ground_at(point);
    if surface.is_some_and(|surface| surface.substrate == GroundSubstrate::Water) {
        return 0.0;
    }
    if surface.is_some_and(|surface| surface.substrate == GroundSubstrate::Road) {
        return road_surface_relief(seed, point, ground);
    }

    let broad = signed_ground_noise(seed ^ 0x6272_6f61_645f_0001, point / 3.2) * 0.024;
    let fine = signed_ground_noise(seed ^ 0x6669_6e65_5f00_0002, point / 0.92) * 0.009;
    let clod_strength = match surface.map(|surface| surface.substrate) {
        Some(GroundSubstrate::Stone) => 0.0,
        Some(GroundSubstrate::Gravel) => 0.25,
        Some(GroundSubstrate::Mud | GroundSubstrate::Soil) => 1.0,
        Some(GroundSubstrate::Road | GroundSubstrate::Water) => unreachable!(),
        None => 1.0,
    };
    let clods = terrain_clod_relief(seed, point) * clod_strength;
    let shape = terrain_shape_sample(terrain, point, AUTHORITATIVE_DETAIL_SPACING_METRES * 3.0);
    let process_relief = shape.map_or(0.0, |shape| {
        drainage_relief(seed, point, shape, moisture_bps)
            + soil_creep_relief(seed, point, shape)
            + shape.concavity.clamp(-0.018, 0.024)
    });
    let roots = tree_root_relief(seed, point, tree_positions);
    let rock_contact = boulder_ground_relief(seed, point, shape, rock_influences);
    let rock_strata = match surface.map(|surface| surface.substrate) {
        Some(GroundSubstrate::Stone) => rocky_substrate_relief(seed, point, shape, 1.0),
        Some(GroundSubstrate::Gravel) => rocky_substrate_relief(seed, point, shape, 0.62),
        _ => 0.0,
    };
    let substrate_strength = match surface.map(|surface| surface.substrate) {
        Some(GroundSubstrate::Stone) => 0.54,
        Some(GroundSubstrate::Gravel) => 0.68,
        Some(GroundSubstrate::Mud) => 0.78,
        Some(GroundSubstrate::Soil) => 1.0,
        Some(GroundSubstrate::Road | GroundSubstrate::Water) => unreachable!(),
        None => 0.62,
    };
    let cover_strength = match surface.map(|surface| surface.cover) {
        Some(GroundCover::Reeds) => 0.35,
        Some(GroundCover::LooseStone) => 0.78,
        _ => 1.0,
    };
    ((broad + fine + clods + process_relief) * substrate_strength * cover_strength
        + roots
        + rock_contact
        + rock_strata)
        .clamp(DETAIL_RELIEF_MINIMUM_METRES, DETAIL_RELIEF_MAXIMUM_METRES)
}

fn terrain_shape_sample(
    terrain: &SceneTerrain,
    point: bevy::math::Vec2,
    radius: f32,
) -> Option<TerrainShapeSample> {
    use bevy::math::Vec2;
    let centre = terrain.height_at(point)?;
    let east = terrain.height_at(point + Vec2::X * radius)?;
    let west = terrain.height_at(point - Vec2::X * radius)?;
    let north = terrain.height_at(point + Vec2::Y * radius)?;
    let south = terrain.height_at(point - Vec2::Y * radius)?;
    let gradient = Vec2::new(east - west, north - south) / (radius * 2.0);
    let slope = gradient.length();
    let downhill = if slope > 0.000_1 {
        -gradient / slope
    } else {
        Vec2::X
    };
    Some(TerrainShapeSample {
        downhill,
        slope,
        concavity: ((east + west + north + south) * 0.25 - centre) * 0.18,
    })
}

fn signed_ground_noise(seed: u64, point: bevy::math::Vec2) -> f32 {
    ground_mask_noise(seed, point) * 2.0 - 1.0
}

fn terrain_clod_relief(seed: u64, point: bevy::math::Vec2) -> f32 {
    let field = ground_mask_noise(seed ^ 0x636c_6f64_5f66_6c64, point / 0.58);
    detail_smoothstep(0.69, 0.91, field) * 0.022 - 0.003
}

fn drainage_relief(
    seed: u64,
    point: bevy::math::Vec2,
    shape: TerrainShapeSample,
    moisture_bps: u16,
) -> f32 {
    use bevy::math::Vec2;
    let slope_weight = detail_smoothstep(0.012, 0.16, shape.slope);
    if slope_weight <= 0.0 {
        return 0.0;
    }
    let normal = Vec2::new(-shape.downhill.y, shape.downhill.x);
    let warp = signed_ground_noise(seed ^ 0x7269_6c6c_5f77_6172, point / 5.5) * 0.85;
    let spacing = 2.6 + ground_mask_noise(seed ^ 0x7269_6c6c_5f73_7063, point / 11.0) * 1.4;
    let distance = periodic_distance(point.dot(normal) + warp, spacing);
    let channel = 1.0 - detail_smoothstep(0.08, 0.34, distance);
    let shoulder =
        detail_smoothstep(0.18, 0.42, distance) * (1.0 - detail_smoothstep(0.42, 0.72, distance));
    let moisture = UnitBasisPoints::saturating(moisture_bps).as_unit_f32();
    (-channel * (0.026 + moisture * 0.012) + shoulder * 0.009) * slope_weight
}

fn soil_creep_relief(seed: u64, point: bevy::math::Vec2, shape: TerrainShapeSample) -> f32 {
    let slope_weight = detail_smoothstep(0.035, 0.22, shape.slope);
    let warp = signed_ground_noise(seed ^ 0x6372_6565_705f_7772, point / 7.0) * 0.55;
    let distance = periodic_distance(point.dot(shape.downhill) + warp, 3.1);
    (1.0 - detail_smoothstep(0.12, 0.52, distance)) * 0.019 * slope_weight
}

fn rocky_substrate_relief(
    seed: u64,
    point: bevy::math::Vec2,
    shape: Option<TerrainShapeSample>,
    strength: f32,
) -> f32 {
    use bevy::math::Vec2;
    let fallback_angle = inclusive_unit_f32(seed ^ 0x7374_7261_7461_6469) * core::f32::consts::TAU;
    let downhill = shape
        .map(|shape| shape.downhill)
        .unwrap_or(Vec2::new(fallback_angle.cos(), fallback_angle.sin()));
    let across = Vec2::new(-downhill.y, downhill.x);
    let slope_weight = shape
        .map(|shape| detail_smoothstep(0.018, 0.18, shape.slope))
        .unwrap_or(0.35);
    let contour =
        point.dot(downhill) + signed_ground_noise(seed ^ 0x7374_7261_7461_7772, point / 6.5) * 0.72;
    let shelf = (1.0 - detail_smoothstep(0.08, 0.48, periodic_distance(contour, 2.15)))
        * (0.019 + slope_weight * 0.029);
    let fracture_a = periodic_distance(
        point.dot(across) + signed_ground_noise(seed ^ 0x6672_6163_7475_7261, point / 4.8) * 0.4,
        3.7,
    );
    let diagonal = (across * 0.72 + downhill * 0.69).normalize_or_zero();
    let fracture_b = periodic_distance(
        point.dot(diagonal) + signed_ground_noise(seed ^ 0x6672_6163_7475_7262, point / 5.6) * 0.34,
        5.3,
    );
    let crack = (1.0 - detail_smoothstep(0.035, 0.17, fracture_a))
        .max((1.0 - detail_smoothstep(0.035, 0.15, fracture_b)) * 0.72);
    (shelf - crack * 0.019) * strength
}

fn boulder_ground_relief(
    seed: u64,
    point: bevy::math::Vec2,
    shape: Option<TerrainShapeSample>,
    rocks: &[TerrainRockInfluence],
) -> f32 {
    use bevy::math::Vec2;
    let mut relief = 0.0;
    for rock in rocks {
        let offset = point - rock.centre;
        let distance = offset.length();
        let radius = rock.radius.max(0.35);
        if distance > radius * 5.0 {
            continue;
        }
        let rock_seed = splitmix64(
            seed ^ u64::from(rock.centre.x.to_bits()).rotate_left(29)
                ^ u64::from(rock.centre.y.to_bits()),
        );
        let fallback_angle = inclusive_unit_f32(rock_seed) * core::f32::consts::TAU;
        let downhill = shape
            .map(|shape| shape.downhill)
            .unwrap_or(Vec2::new(fallback_angle.cos(), fallback_angle.sin()));
        let across_axis = Vec2::new(-downhill.y, downhill.x);
        let socket = (1.0 - detail_smoothstep(radius * 0.48, radius * 1.08, distance)) * -0.042;
        let apron = detail_smoothstep(radius * 0.72, radius * 1.04, distance)
            * (1.0 - detail_smoothstep(radius * 1.04, radius * 1.72, distance))
            * 0.033;
        let downstream = offset.dot(downhill);
        let across = offset.dot(across_axis).abs();
        let tail_length = radius * (3.2 + inclusive_unit_f32(splitmix64(rock_seed)) * 1.1);
        let longitudinal = detail_smoothstep(radius * 0.45, radius * 0.95, downstream)
            * (1.0 - detail_smoothstep(tail_length * 0.62, tail_length, downstream));
        let tail_width = radius * 0.42 + downstream.max(0.0) * 0.24;
        let lateral = 1.0 - detail_smoothstep(tail_width * 0.42, tail_width, across);
        let granular = 0.72
            + ground_mask_noise(
                rock_seed ^ 0x6465_6272_6973_746c,
                Vec2::new(downstream / 1.7, across / 0.8),
            ) * 0.28;
        relief += socket + apron + longitudinal * lateral * granular * 0.034;
    }
    relief.clamp(-0.055, 0.07)
}

fn tree_root_relief(
    seed: u64,
    point: bevy::math::Vec2,
    tree_positions: &[bevy::math::Vec2],
) -> f32 {
    let mut relief = 0.0;
    for &tree in tree_positions {
        let offset = point - tree;
        let radius = offset.length();
        if radius > 8.0 {
            continue;
        }
        let tree_seed = splitmix64(
            seed ^ u64::from(tree.x.to_bits()).rotate_left(23) ^ u64::from(tree.y.to_bits()),
        );
        let mound = (-(radius / 1.35).powi(2)).exp() * 0.045;
        let basin = detail_smoothstep(0.9, 1.8, radius)
            * (1.0 - detail_smoothstep(5.2, 7.7, radius))
            * -0.012;
        let angle = offset.y.atan2(offset.x);
        let mut ridges = 0.0_f32;
        for root in 0..7_u64 {
            let root_seed = splitmix64(tree_seed ^ root.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let origin = inclusive_unit_f32(root_seed) * core::f32::consts::TAU;
            let phase = inclusive_unit_f32(splitmix64(root_seed)) * core::f32::consts::TAU;
            let length = 4.8 + inclusive_unit_f32(splitmix64(root_seed ^ 0x6c65_6e67)) * 2.9;
            if radius > length || radius < 0.28 {
                continue;
            }
            let curved_angle = origin + (radius * 0.9 + phase).sin() * 0.11;
            let angular_distance = wrapped_angle_difference(angle, curved_angle).abs() * radius;
            let width = 0.16 + radius * 0.045;
            let ridge = 1.0 - detail_smoothstep(width * 0.3, width, angular_distance);
            let taper = 1.0 - detail_smoothstep(length * 0.58, length, radius);
            ridges = ridges.max(ridge * taper * (0.082 - radius * 0.007).max(0.025));
        }
        relief += mound + basin + ridges;
    }
    relief.clamp(-0.02, 0.115)
}

fn road_surface_relief(seed: u64, point: bevy::math::Vec2, ground: &SceneGround) -> f32 {
    use bevy::math::Vec2;
    let is_road = |sample: Vec2| {
        ground
            .ground_at(sample)
            .is_some_and(|surface| surface.substrate == GroundSubstrate::Road)
    };
    let mut tangent = Vec2::X;
    let mut best_score = -1_i32;
    for direction_index in 0..8 {
        let angle = direction_index as f32 * core::f32::consts::PI / 8.0;
        let candidate = Vec2::new(angle.cos(), angle.sin());
        let score = [0.75_f32, 1.5, 2.5]
            .into_iter()
            .map(|distance| {
                i32::from(is_road(point + candidate * distance))
                    + i32::from(is_road(point - candidate * distance))
            })
            .sum();
        if score > best_score {
            best_score = score;
            tangent = candidate;
        }
    }
    let normal = Vec2::new(-tangent.y, tangent.x);
    let edge_distance = |direction: f32| {
        (1..=24)
            .map(|step| step as f32 * 0.25)
            .find(|distance| !is_road(point + normal * direction * *distance))
            .unwrap_or(6.0)
    };
    let positive_edge = edge_distance(1.0);
    let negative_edge = edge_distance(-1.0);
    let half_width = ((positive_edge + negative_edge) * 0.5).max(0.6);
    let across = (negative_edge - positive_edge) * 0.5;
    let rut_offset = (half_width * 0.46).clamp(0.38, 0.82);
    let rut_width = (half_width * 0.12).clamp(0.14, 0.27);
    let gaussian = |distance: f32| (-(distance / rut_width).powi(2) * 1.7).exp();
    let ruts = gaussian(across - rut_offset) + gaussian(across + rut_offset);
    let crown = (1.0 - (across / half_width).powi(2)).max(0.0) * 0.026;
    let travelled = point.dot(tangent);
    let irregularity = signed_ground_noise(
        seed ^ 0x726f_6164_5f72_7574,
        Vec2::new(travelled / 2.4, across / 1.1),
    ) * 0.004;
    (crown - ruts * 0.038 + irregularity).clamp(-0.048, 0.032)
}

fn ground_mask_noise(seed: u64, point: bevy::math::Vec2) -> f32 {
    use bevy::math::Vec2;
    let cell = point.floor();
    let local = point - cell;
    let curve = local * local * (Vec2::splat(3.0) - local * 2.0);
    let hash = |offset: Vec2| {
        let coordinate = cell + offset;
        let x = i64::from(coordinate.x as i32) as u64;
        let y = i64::from(coordinate.y as i32) as u64;
        inclusive_unit_f32(splitmix64(
            seed ^ x.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ y.wrapping_mul(0xbf58_476d_1ce4_e5b9),
        ))
    };
    let bottom = lerp(hash(Vec2::ZERO), hash(Vec2::X), curve.x);
    let top = lerp(hash(Vec2::Y), hash(Vec2::ONE), curve.x);
    lerp(bottom, top, curve.y)
}

fn periodic_distance(value: f32, period: f32) -> f32 {
    let wrapped = value.rem_euclid(period);
    wrapped.min(period - wrapped)
}

fn wrapped_angle_difference(left: f32, right: f32) -> f32 {
    (left - right + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU)
        - core::f32::consts::PI
}

fn detail_smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn base_ground_surface(sample: EnvironmentalSample) -> GroundSurface {
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
    let lithology = match splitmix64(seed ^ ROCK_LITHOLOGY_DOMAIN) % 3 {
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
        let hash = splitmix64(seed ^ (axis as u64).wrapping_mul(ROCK_DIMENSION_AXIS_STRIDE));
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
            let hilly = f32::from(sample.hilly_bps) / f32::from(BASIS_POINTS_PER_WHOLE);
            let wetland = f32::from(sample.wetland_bps) / f32::from(BASIS_POINTS_PER_WHOLE);
            let amplitude = (0.055 + hilly * 0.22) * (1.0 - wetland * 0.55);
            let world_x = x as f32 * spacing;
            let world_z = z as f32 * spacing;
            let broad = value_noise(seed, world_x, world_z, 6.0);
            let fine = value_noise(seed ^ MICRORELIEF_FINE_DOMAIN, world_x, world_z, 2.25);
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
        inclusive_unit_f32(bits) * 2.0 - 1.0
    };
    let north = sample(x0, z0) + (sample(x0 + 1, z0) - sample(x0, z0)) * tx;
    let south = sample(x0, z0 + 1) + (sample(x0 + 1, z0 + 1) - sample(x0, z0 + 1)) * tx;
    north + (south - north) * tz
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
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
    // Alternating deterministic sweeps constrain every cardinal edge without
    // choosing a random repair direction around isolated spikes or pits.
    for _ in 0..4 {
        for z in 0..depth {
            for x in 1..width {
                clamp_height_pair(heights, z * width + x - 1, z * width + x, maximum_step);
            }
            for x in (0..width - 1).rev() {
                clamp_height_pair(heights, z * width + x + 1, z * width + x, maximum_step);
            }
        }
        for x in 0..width {
            for z in 1..depth {
                clamp_height_pair(heights, (z - 1) * width + x, z * width + x, maximum_step);
            }
            for z in (0..depth - 1).rev() {
                clamp_height_pair(heights, (z + 1) * width + x, z * width + x, maximum_step);
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
        levelled_building_samples: 0,
        removed_building_obstacles: 0,
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
        .any(|value| value > BASIS_POINTS_PER_WHOLE)
    }) {
        return invalid(format!("{label} contains an invalid environment sample"));
    }
    Ok(())
}

fn validate_weather(weather: WeatherSnapshot) -> Result<(), SceneInputError> {
    if weather.rules_version != WEATHER_RULES_VERSION
        || weather.wind_speed_bps > BASIS_POINTS_PER_WHOLE
        || weather.intensity_bps > BASIS_POINTS_PER_WHOLE
        || weather.ground_moisture_bps > BASIS_POINTS_PER_WHOLE
        || weather.snow_cover_bps > BASIS_POINTS_PER_WHOLE
        || weather.atmosphere.relative_humidity_bps > BASIS_POINTS_PER_WHOLE
        || weather.atmosphere.dew_point_deci_c > weather.temperature_deci_c + 5
        || !(8_700..=10_850).contains(&weather.atmosphere.sea_level_pressure_deci_hpa)
        || weather.atmosphere.wind_direction_degrees >= 360
        || weather.atmosphere.wind_shear_bps > BASIS_POINTS_PER_WHOLE
        || weather.atmosphere.instability_bps > BASIS_POINTS_PER_WHOLE
        || !(-i16::try_from(BASIS_POINTS_PER_WHOLE).unwrap()
            ..=i16::try_from(BASIS_POINTS_PER_WHOLE).unwrap())
            .contains(&weather.atmosphere.lift_bps)
        || weather.cloud_layers().any(|layer| {
            layer.coverage_bps > BASIS_POINTS_PER_WHOLE
                || layer.optical_density_bps > BASIS_POINTS_PER_WHOLE
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

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_building_generator::{BuildingArchetype, BuildingProgram};
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
            lunar_phase_minute: 123_456,
            absolute_elevation_metres: 80,
            playable: TerrainSampleGrid {
                width: 3,
                depth: 3,
                spacing_metres: 1.0,
                heights_metres: vec![0.0, 0.1, 0.0, 0.1, 0.2, 0.1, 0.0, 0.1, 0.0],
                environment,
            },
            landform: None,
            streets: Vec::new(),
            yards: Vec::new(),
            buildings: Vec::new(),
            distant_buildings: Vec::new(),
            vista: VistaSample::default(),
            weather: weather_at(42, 123_456, 53_500_000, 10_000_000, 80),
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
    fn generated_building_gets_static_collision_and_a_level_clear_pad() {
        let mut input = fixture();
        let width = 41usize;
        let depth = 41usize;
        input.playable = TerrainSampleGrid {
            width: width as u16,
            depth: depth as u16,
            spacing_metres: 1.0,
            heights_metres: (0..width * depth)
                .map(|index| (index % width) as f32 * 0.04)
                .collect(),
            environment: vec![
                EnvironmentalSample {
                    canopy_bps: 10_000,
                    ..Default::default()
                };
                width * depth
            ],
        };
        input.buildings.push(TacticalBuildingPlacement {
            id: 7,
            program: BuildingProgram::fixture(BuildingArchetype::FachwerkCottage, 42),
            centre_metres: bevy::math::Vec2::ZERO,
            orientation: BuildingOrientation::from_radians(core::f32::consts::FRAC_PI_2).unwrap(),
        });

        let generated = input.generate().unwrap();
        let building = &generated.buildings[0];
        assert!(!building.collision.cuboids.is_empty());
        assert!(generated.repairs.levelled_building_samples > 0);
        let centre_height = generated.terrain.height_at(bevy::math::Vec2::ZERO).unwrap();
        assert!((centre_height - building.pad_elevation_metres).abs() < 0.0001);
        assert_eq!(
            generated.ground.ground_at(bevy::math::Vec2::ZERO),
            Some(GroundSurface {
                substrate: GroundSubstrate::Stone,
                cover: GroundCover::Bare,
                cover_density_bps: 0,
                cover_height_cm: 0,
            })
        );
        let exclusion =
            building.collision.bounds.plan_half_extents() + bevy::math::Vec2::splat(5.5);
        assert!(generated.obstacles.iter().all(|obstacle| {
            let (x, z) = match *obstacle {
                GeneratedObstacle::Tree { x, z } | GeneratedObstacle::Rock { x, z, .. } => (x, z),
            };
            let point =
                bevy::math::Vec2::new(f32::from(x), f32::from(z)) - bevy::math::Vec2::splat(20.0);
            let building_local = bevy::math::Vec2::new(point.y, -point.x).abs();
            !building_local.cmple(exclusion).all()
        }));
    }

    #[test]
    fn city_streets_replace_grass_with_their_authored_substrate() {
        let mut input = fixture();
        input.streets = vec![
            CityStreetPatch::Corridor {
                start_metres: bevy::math::Vec2::new(-2.0, 0.0),
                end_metres: bevy::math::Vec2::new(2.0, 0.0),
                half_width_metres: 1.0,
                surface: crate::city_layout::CityStreetSurface::CompactedEarth,
            },
            CityStreetPatch::Corridor {
                start_metres: bevy::math::Vec2::new(-1.0, -2.0),
                end_metres: bevy::math::Vec2::new(-1.0, 2.0),
                half_width_metres: 1.0,
                surface: crate::city_layout::CityStreetSurface::Fieldstone,
            },
        ];

        let generated = input.generate().unwrap();
        assert_eq!(
            generated.ground.ground_at(bevy::math::Vec2::new(1.0, 0.0)),
            Some(GroundSurface {
                substrate: GroundSubstrate::Soil,
                cover: GroundCover::Bare,
                cover_density_bps: 0,
                cover_height_cm: 0,
            })
        );
        assert_eq!(
            generated.ground.ground_at(bevy::math::Vec2::new(-1.0, 0.0)),
            Some(GroundSurface {
                substrate: GroundSubstrate::Road,
                cover: GroundCover::Bare,
                cover_density_bps: 0,
                cover_height_cm: 0,
            })
        );
    }

    #[test]
    fn developed_city_yards_replace_meadow_cover_with_bare_soil() {
        let mut input = fixture();
        input.yards.push(CityYardPatch {
            corners_metres: [
                bevy::math::Vec2::new(-1.0, -1.0),
                bevy::math::Vec2::new(1.0, -1.0),
                bevy::math::Vec2::new(1.0, 1.0),
                bevy::math::Vec2::new(-1.0, 1.0),
            ],
            surface: crate::city_layout::CityYardSurface::PackedEarth,
        });

        let generated = input.generate().unwrap();
        assert_eq!(
            generated.ground.ground_at(bevy::math::Vec2::ZERO),
            Some(GroundSurface {
                substrate: GroundSubstrate::Soil,
                cover: GroundCover::Bare,
                cover_density_bps: 0,
                cover_height_cm: 0,
            })
        );
    }

    #[test]
    fn distant_buildings_affect_scene_identity_without_entering_tactical_generation() {
        let mut input = fixture();
        let empty_digest = input.digest().unwrap();
        input.distant_buildings.push(DistantBuildingPlacement {
            id: 1,
            archetype: BuildingArchetype::TownHouse,
            seed: 42,
            centre_metres: bevy::math::Vec2::new(120.0, -90.0),
            base_elevation_metres: 0.0,
            orientation: BuildingOrientation::from_radians(core::f32::consts::PI).unwrap(),
        });

        assert_ne!(input.digest().unwrap(), empty_digest);
        assert!(input.generate().unwrap().buildings.is_empty());
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
    fn narrow_peak_is_preserved_on_the_regional_horizon_lod_boundary() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/tactical-scenes/narrow-peak-lod-boundary.json");
        let input = TacticalSceneInput::load(&path).unwrap();
        let regional = &input.vista.lods[1];
        let horizon = &input.vista.lods[2];
        let peak_column = |lod: &VistaLod| {
            usize::from(lod.width / 2) + (5_000.0 / lod.spacing_metres).round() as usize
        };
        let regional_peak = regional.heights_metres
            [usize::from(regional.depth / 2) * usize::from(regional.width) + peak_column(regional)];
        let horizon_peak = horizon.heights_metres
            [usize::from(horizon.depth / 2) * usize::from(horizon.width) + peak_column(horizon)];
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
        assert!(flat.terrain.grid_scale() <= AUTHORITATIVE_DETAIL_SPACING_METRES);
        assert!(flat.terrain.coarse_grid_scale() > flat.terrain.grid_scale());
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

        let width = usize::from(input.playable.width);
        let depth = usize::from(input.playable.depth);
        let spacing = input.playable.spacing_metres;
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
                if x + 1 < width {
                    assert!(
                        (height(x, z) - height(x + 1, z)).abs()
                            <= spacing * MAX_PLAYABLE_GRADE + 0.001
                    );
                }
                if z + 1 < depth {
                    assert!(
                        (height(x, z) - height(x, z + 1)).abs()
                            <= spacing * MAX_PLAYABLE_GRADE + 0.001
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
