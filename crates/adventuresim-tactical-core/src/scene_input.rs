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
pub const TACTICAL_SCENE_GENERATION_VERSION: u16 = 9;
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
    pub repairs: SceneRepairReport,
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
            let width = usize::from(self.playable.width);
            let depth = usize::from(self.playable.depth);
            match *obstacle {
                GeneratedObstacle::Tree { x, z } => {
                    !is_tree_camera_clearance_cell(usize::from(x), usize::from(z), depth)
                }
                GeneratedObstacle::Rock { x, z, .. } => {
                    !is_reserved_playability_cell(usize::from(x), usize::from(z), width, depth)
                }
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
        )?;
        Ok(GeneratedTacticalScene {
            digest: self.digest()?,
            terrain,
            ground,
            obstacles,
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

fn build_scene_ground(
    width: usize,
    depth: usize,
    spacing: f32,
    environment: &[EnvironmentalSample],
    terrain: &SceneTerrain,
    obstacles: &[GeneratedObstacle],
    obstacle_spacing: f32,
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
                    (splitmix64(coordinate ^ 0x001e_af11_77e2) % 10_000) as f32 / 10_000.0;
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
