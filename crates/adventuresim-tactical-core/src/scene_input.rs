//! Versioned, bounded input for deterministic tactical scene generation.
//!
//! This is deliberately data-only. Production dispatchers can sample the
//! imported terrain pack into it, while tactical-only tools can serialize a
//! synthetic fixture. Short-lived servers consume the identical format and
//! never need access to the continental source pack.

use std::{fs, path::Path};

use adventuresim_core::weather::{Precipitation, WeatherSnapshot};
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::scene::SceneTerrain;

pub const TACTICAL_SCENE_SCHEMA_VERSION: u16 = 1;
pub const TACTICAL_SCENE_GENERATION_VERSION: u16 = 2;
pub const MAX_SCENE_INPUT_BYTES: u64 = 32 * 1024 * 1024;
pub const TREE_TRUNK_RADIUS_METRES: f32 = 0.35;
pub const TREE_TRUNK_HEIGHT_METRES: f32 = 5.0;
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
    pub weather: WeatherSnapshot,
    pub canopy_bps: u16,
    pub wetland_bps: u16,
    pub cultivation_bps: u16,
    pub water_bps: u16,
    pub hilly_bps: u16,
}

/// Compact replicated identity for a server-authoritative static obstacle.
/// Its Transform locates the collider center; presentation derives matching
/// proxy geometry from these shared dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component, Serialize, Deserialize)]
#[component(immutable)]
#[serde(rename_all = "snake_case")]
pub enum SceneObstacle {
    Tree,
    Rock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedObstacle {
    Tree { x: u16, z: u16 },
    Rock { x: u16, z: u16 },
}

#[derive(Debug)]
pub struct GeneratedTacticalScene {
    pub digest: String,
    pub terrain: SceneTerrain,
    pub obstacles: Vec<GeneratedObstacle>,
    pub repairs: SceneRepairReport,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneRepairReport {
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
        let mut heights = self.playable.heights_metres.clone();
        let mut environment = self.playable.environment.clone();
        let mut repairs = repair_playable_terrain(
            usize::from(self.playable.width),
            usize::from(self.playable.depth),
            self.playable.spacing_metres,
            &mut heights,
            &mut environment,
        );
        let terrain = SceneTerrain::from_heightmap(
            self.playable.width.into(),
            self.playable.depth.into(),
            self.playable.spacing_metres,
            heights,
        )
        .ok_or_else(|| SceneInputError::Validation("playable heightmap is invalid".into()))?;
        let mut obstacles = environment
            .iter()
            .enumerate()
            .filter_map(|(index, sample)| {
                let x = (index % usize::from(self.playable.width)) as u16;
                let z = (index / usize::from(self.playable.width)) as u16;
                let coordinate = ((x as u64) << 32) ^ z as u64;
                let tree_roll = splitmix64(self.seed ^ coordinate) % 10_000;
                let rock_roll = splitmix64(self.seed ^ coordinate ^ 0x52cc_5f1b_d391_a739) % 10_000;
                if tree_roll < u64::from(sample.canopy_bps) / 12 {
                    Some(GeneratedObstacle::Tree { x, z })
                } else if rock_roll < u64::from(sample.hilly_bps) / 20 && sample.water_bps < 5_000 {
                    Some(GeneratedObstacle::Rock { x, z })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let before = obstacles.len();
        obstacles.retain(|obstacle| {
            let (x, z) = match *obstacle {
                GeneratedObstacle::Tree { x, z } | GeneratedObstacle::Rock { x, z } => (x, z),
            };
            !is_reserved_playability_cell(
                usize::from(x),
                usize::from(z),
                usize::from(self.playable.width),
                usize::from(self.playable.depth),
            )
        });
        repairs.removed_corridor_obstacles = (before - obstacles.len()) as u32;
        Ok(GeneratedTacticalScene {
            digest: self.digest()?,
            terrain,
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
            weather: self.weather,
            canopy_bps: (sum[0] / count) as u16,
            wetland_bps: (sum[1] / count) as u16,
            cultivation_bps: (sum[2] / count) as u16,
            water_bps: (sum[3] / count) as u16,
            hilly_bps: (sum[4] / count) as u16,
        }
    }
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
    if weather.wind_speed_bps > 10_000
        || weather.intensity_bps > 10_000
        || weather.ground_moisture_bps > 10_000
        || weather.snow_cover_bps > 10_000
        || (matches!(weather.precipitation, Precipitation::Clear) && weather.intensity_bps != 0)
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
            assert_eq!(input.generate().unwrap().terrain.width(), 100.0);
            assert_eq!(input.vista.lods.len(), 3);
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
        let sparse = TacticalSceneInput::load(&root.join("sparse-woodland.json"))
            .unwrap()
            .generate()
            .unwrap();
        let hillside = TacticalSceneInput::load(&root.join("steep-open-hillside.json"))
            .unwrap()
            .generate()
            .unwrap();
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
    }

    #[test]
    fn obstacle_kind_has_a_stable_wire_round_trip() {
        for obstacle in [SceneObstacle::Tree, SceneObstacle::Rock] {
            let bytes = postcard::to_allocvec(&obstacle).unwrap();
            assert_eq!(
                postcard::from_bytes::<SceneObstacle>(&bytes).unwrap(),
                obstacle
            );
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
        assert!(first.obstacles.iter().all(|obstacle| {
            let (x, z) = match *obstacle {
                GeneratedObstacle::Tree { x, z } | GeneratedObstacle::Rock { x, z } => (x, z),
            };
            !is_reserved_playability_cell(usize::from(x), usize::from(z), width, depth)
        }));
    }
}
