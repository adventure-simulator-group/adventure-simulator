use std::{
    f64::consts::PI,
    fs,
    path::{Path, PathBuf},
};

use adventuresim_core::weather::{WORLD_WEATHER_SEED, weather_at};
use adventuresim_tactical_core::prelude::*;
use adventuresim_terrain::{Cell, Surface, TerrainPack};
use sha2::{Digest, Sha256};

const PLAYABLE_SIDE: u16 = 101;
const PLAYABLE_SPACING_METRES: f32 = 1.0;

pub fn build_imported_scene(
    pack: &TerrainPack,
    mission_id: &str,
    scene_key: &str,
    latitude_e7: i32,
    longitude_e7: i32,
    absolute_minute: u64,
    lunar_phase_minute: u64,
) -> Result<TacticalSceneInput, String> {
    let latitude = f64::from(latitude_e7) / 10_000_000.0;
    let longitude = f64::from(longitude_e7) / 10_000_000.0;
    let center = pack
        .cell(latitude, longitude)
        .map_err(|error| error.to_string())?
        .ok_or("mission coordinate is outside the final terrain pack")?;
    let seed = deterministic_seed(mission_id);
    let playable = sample_grid(
        pack,
        latitude,
        longitude,
        PLAYABLE_SIDE,
        PLAYABLE_SIDE,
        PLAYABLE_SPACING_METRES,
        f32::from(center.elevation_m),
        false,
        seed,
    )?;
    let vista = VistaSample {
        // The near regional ring needs enough spatial frequency to preserve
        // forest boundaries, rolling ground, and the transition from geometric
        // grass. Coarser rings then expand rapidly to the 50-km horizon.
        lods: [(0, 50.0, 41), (1, 250.0, 17), (2, 1_000.0, 51)]
            .into_iter()
            .map(|(level, spacing, side)| {
                let grid = sample_grid(
                    pack,
                    latitude,
                    longitude,
                    side,
                    side,
                    spacing,
                    f32::from(center.elevation_m),
                    true,
                    seed ^ u64::from(level),
                )?;
                Ok(VistaLod {
                    level,
                    spacing_metres: spacing,
                    width: side,
                    depth: side,
                    origin_east_metres: 0.0,
                    origin_north_metres: 0.0,
                    heights_metres: grid.heights_metres,
                    environment: grid.environment,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    let input = TacticalSceneInput {
        schema_version: TACTICAL_SCENE_SCHEMA_VERSION,
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        seed,
        scene_key: scene_key.into(),
        source: SceneSource::ImportedPackage(pack.digest().into()),
        latitude_microdegrees: latitude_e7 / 10,
        longitude_microdegrees: longitude_e7 / 10,
        absolute_minute,
        lunar_phase_minute,
        absolute_elevation_metres: center.elevation_m,
        playable,
        vista,
        weather: weather_at(
            WORLD_WEATHER_SEED,
            absolute_minute,
            latitude_e7 / 10,
            longitude_e7 / 10,
            center.elevation_m,
        ),
    };
    input.validate().map_err(|error| error.to_string())?;
    Ok(input)
}

pub fn materialize_scene_input(
    directory: &Path,
    mission_id: &str,
    input: &TacticalSceneInput,
) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let key = Sha256::digest(mission_id.as_bytes())
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let path = directory.join(format!("scene-{key}.json"));
    let bytes = serde_json::to_vec(input).map_err(|error| error.to_string())?;
    if let Ok(existing) = fs::read(&path) {
        if existing == bytes {
            return Ok(path);
        }
        return Err("existing scene input does not match the deterministic request".into());
    }
    let temporary = directory.join(format!("scene-{key}.tmp"));
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn sample_grid(
    pack: &TerrainPack,
    latitude: f64,
    longitude: f64,
    width: u16,
    depth: u16,
    spacing: f32,
    center_elevation: f32,
    preserve_peaks: bool,
    seed: u64,
) -> Result<TerrainSampleGrid, String> {
    let center_x = f64::from(width - 1) * 0.5;
    let center_z = f64::from(depth - 1) * 0.5;
    let mut heights = Vec::with_capacity(usize::from(width) * usize::from(depth));
    let mut environment = Vec::with_capacity(heights.capacity());
    for z in 0..depth {
        for x in 0..width {
            let east = (f64::from(x) - center_x) * f64::from(spacing);
            let north = (f64::from(z) - center_z) * f64::from(spacing);
            let (sample_latitude, sample_longitude) =
                offset_coordinate(latitude, longitude, east, north);
            let cell = pack
                .cell(sample_latitude, sample_longitude)
                .map_err(|error| error.to_string())?
                .ok_or("requested scene window leaves the final terrain pack")?;
            let mut elevation = f32::from(cell.elevation_m);
            if preserve_peaks {
                let radius = f64::from(spacing) * 0.4;
                for (sample_east, sample_north) in [
                    (-radius, -radius),
                    (0.0, -radius),
                    (radius, -radius),
                    (-radius, 0.0),
                    (radius, 0.0),
                    (-radius, radius),
                    (0.0, radius),
                    (radius, radius),
                ] {
                    let (lat, lon) = offset_coordinate(
                        sample_latitude,
                        sample_longitude,
                        sample_east,
                        sample_north,
                    );
                    if let Some(neighbor) =
                        pack.cell(lat, lon).map_err(|error| error.to_string())?
                    {
                        elevation = elevation.max(f32::from(neighbor.elevation_m));
                    }
                }
            } else {
                let detail = deterministic_detail(seed, x, z)
                    * f32::from(cell.hilly_fraction_percent)
                    / 100.0
                    * 0.45;
                elevation += detail;
            }
            heights.push(elevation - center_elevation);
            environment.push(environment_sample(cell));
        }
    }
    Ok(TerrainSampleGrid {
        width,
        depth,
        spacing_metres: spacing,
        heights_metres: heights,
        environment,
    })
}

fn environment_sample(cell: Cell) -> EnvironmentalSample {
    EnvironmentalSample {
        canopy_bps: u16::from(cell.canopy_percent) * 100,
        wetland_bps: u16::from(cell.wetland_fraction_percent) * 100,
        cultivation_bps: if cell.cultivated { 10_000 } else { 0 },
        water_bps: if cell.surface == Surface::Water && !cell.crossing {
            10_000
        } else {
            0
        },
        hilly_bps: u16::from(cell.hilly_fraction_percent) * 100,
        crossing_bps: if cell.crossing { 10_000 } else { 0 },
        surface: match cell.surface {
            Surface::Road => TacticalSurface::Road,
            Surface::Open => TacticalSurface::Open,
            Surface::SparseWoods => TacticalSurface::SparseWoods,
            Surface::DeepWoods => TacticalSurface::DeepWoods,
            Surface::Water => TacticalSurface::Water,
            Surface::Wetland => TacticalSurface::Wetland,
        },
    }
}

fn offset_coordinate(latitude: f64, longitude: f64, east: f64, north: f64) -> (f64, f64) {
    let latitude_delta = north / 111_320.0;
    let longitude_scale = (latitude * PI / 180.0).cos().abs().max(0.01);
    let longitude_delta = east / (111_320.0 * longitude_scale);
    (latitude + latitude_delta, longitude + longitude_delta)
}

fn deterministic_seed(mission_id: &str) -> u64 {
    let digest = Sha256::digest(mission_id.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix"))
}

fn deterministic_detail(seed: u64, x: u16, z: u16) -> f32 {
    let mut value = seed ^ (u64::from(x) << 32) ^ u64::from(z);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value % 20_001) as f32 / 10_000.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, time::SystemTime};

    use adventuresim_terrain::{CHUNK_SIDE, Entry, Manifest, TerrainPurpose};
    use flate2::{Compression, write::DeflateEncoder};

    #[test]
    fn geographic_offsets_are_stable_and_axis_aligned() {
        let (north_lat, north_lon) = offset_coordinate(53.5, 10.0, 0.0, 1_000.0);
        let (east_lat, east_lon) = offset_coordinate(53.5, 10.0, 1_000.0, 0.0);
        assert!(north_lat > 53.5 && (north_lon - 10.0).abs() < 1e-12);
        assert!(east_lon > 10.0 && (east_lat - 53.5).abs() < 1e-12);
    }

    #[test]
    fn environment_projection_retains_crossing_and_independent_cover() {
        let sample = environment_sample(Cell {
            elevation_m: 4,
            surface: Surface::Water,
            crossing: true,
            cultivated: true,
            canopy_percent: 37,
            hilly_fraction_percent: 28,
            wetland_fraction_percent: 19,
        });
        assert_eq!(sample.water_bps, 0);
        assert_eq!(sample.crossing_bps, 10_000);
        assert_eq!(
            (sample.canopy_bps, sample.hilly_bps, sample.wetland_bps),
            (3_700, 2_800, 1_900)
        );
    }

    #[test]
    fn imported_scene_samples_a_known_final_pack_coordinate() {
        let (pack, directory) = constant_final_pack();
        let input = build_imported_scene(
            &pack,
            "mission:known-coordinate",
            "known-coordinate",
            505_000_000,
            105_000_000,
            123_456,
        )
        .expect("known coordinate should produce a tactical scene");

        assert_eq!(input.absolute_elevation_metres, 321);
        assert_eq!(input.playable.heights_metres.len(), 101 * 101);
        assert_eq!(input.playable.environment.len(), 101 * 101);
        assert_eq!(input.vista.lods.len(), 3);
        assert_eq!(input.vista.lods[0].spacing_metres, 50.0);
        assert_eq!(input.vista.lods[0].width, 41);
        assert_eq!(input.vista.lods[2].spacing_metres, 1_000.0);
        assert_eq!(input.vista.lods[2].width, 51);
        assert!(
            input
                .playable
                .heights_metres
                .iter()
                .all(|height| height.abs() <= 0.45)
        );
        let center = input.playable.environment[50 * 101 + 50];
        assert_eq!(center.surface, TacticalSurface::DeepWoods);
        assert_eq!((center.canopy_bps, center.hilly_bps), (7_700, 10_000));
        assert_eq!(
            (center.crossing_bps, center.cultivation_bps),
            (10_000, 10_000)
        );
        assert_eq!(
            input.weather,
            weather_at(WORLD_WEATHER_SEED, 123_456, 50_500_000, 10_500_000, 321)
        );

        drop(pack);
        fs::remove_dir_all(directory).expect("remove terrain fixture directory");
    }

    fn constant_final_pack() -> (TerrainPack, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "adventuresim-tactical-scene-pack-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create terrain fixture directory");
        let manifest_path = directory.join("terrain.manifest.json");
        let pack_path = directory.join("terrain.pack");
        let mut pack_bytes = Vec::new();
        let mut entries = Vec::new();
        let tile_width = 1_800_u16;
        let tile_height = 3_600_u16;
        for chunk_y in 0..tile_height.div_ceil(CHUNK_SIDE) {
            for chunk_x in 0..tile_width.div_ceil(CHUNK_SIDE) {
                let width = CHUNK_SIDE.min(tile_width - chunk_x * CHUNK_SIDE);
                let height = CHUNK_SIDE.min(tile_height - chunk_y * CHUNK_SIDE);
                let cell = [65, 1, 3, 0b1111, 77];
                let decoded = cell.repeat(usize::from(width) * usize::from(height));
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(&decoded).expect("encode fixture chunk");
                let compressed = encoder.finish().expect("finish fixture chunk");
                let offset = pack_bytes.len() as u64;
                pack_bytes.extend_from_slice(&compressed);
                entries.push(Entry {
                    south: 50,
                    west: 10,
                    tile_width,
                    tile_height,
                    chunk_x,
                    chunk_y,
                    width,
                    height,
                    offset,
                    length: compressed.len() as u32,
                    decoded_sha256: format!("{:x}", Sha256::digest(&decoded)),
                });
            }
        }
        fs::write(&pack_path, &pack_bytes).expect("write terrain fixture pack");
        let mut manifest = Manifest {
            schema: adventuresim_terrain::SCHEMA,
            purpose: TerrainPurpose::Final,
            bounds: [10.0, 50.0, 11.0, 51.0],
            source_resolution_m: 30,
            content_sha256: format!("{:x}", Sha256::digest(&pack_bytes)),
            road_geometry_sha256: "1".repeat(64),
            wetland_source_sha256: "2".repeat(64),
            wetland_cells: 1,
            cultivation_grid_crs: "EPSG:3035".into(),
            cultivation_grid_resolution_m: 1_000,
            cultivation_rules_version: 1,
            cultivation_source_sha256: "3".repeat(64),
            cultivated_square_count: 1,
            cultivated_native_cells: 1,
            entries,
            package_sha256: "0".repeat(64),
        };
        manifest.package_sha256 = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&manifest).expect("serialize unsigned manifest"))
        );
        fs::write(
            &manifest_path,
            serde_json::to_vec(&manifest).expect("serialize terrain manifest"),
        )
        .expect("write terrain fixture manifest");
        let pack =
            TerrainPack::load(&manifest_path, &pack_path).expect("load final terrain fixture");
        (pack, directory)
    }
}
