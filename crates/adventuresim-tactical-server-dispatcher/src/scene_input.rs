use std::{
    fs,
    path::{Path, PathBuf},
};

use adventuresim_core::weather::{WORLD_WEATHER_SEED, weather_at};
use adventuresim_tactical_core::prelude::*;
use adventuresim_terrain::{Cell, Surface, TerrainPack};
use adventuresim_world_schema::{
    BASIS_POINTS_PER_WHOLE, TerrainFeature, coordinates::Wgs84CoordinateE7,
};
use bevy::math::Vec2;
use fabelgeist_determinism::mix64;
use sha2::{Digest, Sha256};

use crate::settlement_buildings::{
    SettlementBuildingLayout, SettlementSceneProfile, place_settlement_buildings,
};

mod geological_landforms;

const PLAYABLE_SIDE: u16 = 101;
const PLAYABLE_SPACING_METRES: f32 = 1.0;
const PERCENT_PER_WHOLE: u16 = 100;
const BASIS_POINTS_PER_PERCENT: u16 = BASIS_POINTS_PER_WHOLE / PERCENT_PER_WHOLE;
const VISTA_LOD_SPECS: [VistaLodSpec; 3] = [
    VistaLodSpec::new(0, 50.0, 41),
    VistaLodSpec::new(1, 250.0, 17),
    VistaLodSpec::new(2, 1_000.0, 51),
];
const METRES_PER_LATITUDE_DEGREE: f64 = 111_320.0;
const MIN_LONGITUDE_SCALE: f64 = 0.01;
const PEAK_SAMPLE_RADIUS_FACTOR: f64 = 0.4;
const HILLY_DETAIL_AMPLITUDE_METRES: f32 = 0.45;
const DISTANT_CITY_PAD_MARGIN_METRES: f32 = 50.0;
const DISTANT_CITY_LEVEL_BLEND_METRES: f32 = 100.0;
const RANDOM_DETAIL_SCALE: u64 = 10_000;
const RANDOM_DETAIL_BUCKETS: u64 = RANDOM_DETAIL_SCALE * 2 + 1;
const SCARP_DEFAULT_THROW_CM: u16 = 800;
const SCARP_DEFAULT_HALF_LENGTH_CM: u16 = 4_500;
const SCARP_DEFAULT_HALF_WIDTH_CM: u16 = 1_800;
const SCARP_DEFAULT_COLLAR_CM: u16 = 400;

#[derive(Clone, Copy)]
struct VistaLodSpec {
    level: u8,
    spacing_metres: f32,
    side: u16,
}

impl VistaLodSpec {
    const fn new(level: u8, spacing_metres: f32, side: u16) -> Self {
        Self {
            level,
            spacing_metres,
            side,
        }
    }
}

#[derive(Clone, Copy)]
struct GridDimensions {
    width: u16,
    depth: u16,
}

impl GridDimensions {
    const fn square(side: u16) -> Self {
        Self {
            width: side,
            depth: side,
        }
    }
}

#[derive(Clone, Copy)]
enum ElevationSampling {
    AddLocalDetail,
    PreservePeaks,
}

#[derive(Clone, Copy)]
struct GridSampleRequest {
    center: Wgs84CoordinateE7,
    dimensions: GridDimensions,
    spacing_metres: f32,
    center_elevation_metres: f32,
    elevation_sampling: ElevationSampling,
    seed: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn build_imported_scene(
    pack: &TerrainPack,
    mission_id: &str,
    scene_key: &str,
    latitude_e7: i32,
    longitude_e7: i32,
    absolute_minute: u64,
    lunar_phase_minute: u64,
    settlement: Option<&SettlementSceneProfile>,
) -> Result<TacticalSceneInput, String> {
    let coordinates = Wgs84CoordinateE7::new(latitude_e7, longitude_e7)
        .ok_or("mission coordinate is outside the WGS84 bounds")?;
    let center = pack
        .cell(
            coordinates.latitude().degrees(),
            coordinates.longitude().degrees(),
        )
        .map_err(|error| error.to_string())?
        .ok_or("mission coordinate is outside the final terrain pack")?;
    let seed = deterministic_seed(mission_id);
    let playable = sample_grid(
        pack,
        GridSampleRequest {
            center: coordinates,
            dimensions: GridDimensions::square(PLAYABLE_SIDE),
            spacing_metres: PLAYABLE_SPACING_METRES,
            center_elevation_metres: f32::from(center.elevation_m),
            elevation_sampling: ElevationSampling::AddLocalDetail,
            seed,
        },
    )?;
    let mut vista = VistaSample {
        // The near regional ring needs enough spatial frequency to preserve
        // forest boundaries, rolling ground, and the transition from geometric
        // grass. Coarser rings then expand rapidly to the 50-km horizon.
        lods: VISTA_LOD_SPECS
            .into_iter()
            .map(|spec| {
                let grid = sample_grid(
                    pack,
                    GridSampleRequest {
                        center: coordinates,
                        dimensions: GridDimensions::square(spec.side),
                        spacing_metres: spec.spacing_metres,
                        center_elevation_metres: f32::from(center.elevation_m),
                        elevation_sampling: ElevationSampling::PreservePeaks,
                        seed: seed ^ u64::from(spec.level),
                    },
                )?;
                Ok(VistaLod {
                    level: spec.level,
                    spacing_metres: spec.spacing_metres,
                    width: spec.side,
                    depth: spec.side,
                    origin_east_metres: 0.0,
                    origin_north_metres: 0.0,
                    heights_metres: grid.heights_metres,
                    environment: grid.environment,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    let city_elevation_metres = f32::from(center.elevation_m);
    let building_layout =
        settlement_building_layout(settlement, city_elevation_metres, &mut vista)?;
    let landform = nearest_fault_scarp(pack.terrain_features(), coordinates, seed).or_else(|| {
        settlement
            .is_none()
            .then(|| {
                geological_landforms::select(pack.terrain_features(), coordinates, &playable, seed)
            })
            .flatten()
    });
    let input = TacticalSceneInput {
        schema_version: TACTICAL_SCENE_SCHEMA_VERSION,
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        seed,
        scene_key: scene_key.into(),
        source: SceneSource::ImportedPackage(pack.digest().into()),
        latitude_microdegrees: coordinates.latitude().to_microdegrees().get(),
        longitude_microdegrees: coordinates.longitude().to_microdegrees().get(),
        absolute_minute,
        lunar_phase_minute,
        absolute_elevation_metres: center.elevation_m,
        playable,
        landform,
        streets: building_layout.streets,
        yards: building_layout.yards,
        buildings: building_layout.playable,
        distant_buildings: building_layout.distant,
        vista,
        weather: weather_at(
            WORLD_WEATHER_SEED,
            absolute_minute,
            coordinates.latitude().to_microdegrees().get(),
            coordinates.longitude().to_microdegrees().get(),
            center.elevation_m,
        ),
    };
    input.validate().map_err(|error| error.to_string())?;
    Ok(input)
}

fn settlement_building_layout(
    settlement: Option<&SettlementSceneProfile>,
    elevation_metres: f32,
    vista: &mut VistaSample,
) -> Result<SettlementBuildingLayout, String> {
    let playable_half_extent_metres = f32::from(PLAYABLE_SIDE - 1) * PLAYABLE_SPACING_METRES * 0.5;
    let mut layout = settlement
        .map(|profile| place_settlement_buildings(profile, playable_half_extent_metres))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    for building in &mut layout.distant {
        building.base_elevation_metres = elevation_metres;
    }
    level_distant_city_vista(vista, &layout.distant, elevation_metres);
    Ok(layout)
}

fn nearest_fault_scarp(
    terrain_features: &[TerrainFeature],
    center: Wgs84CoordinateE7,
    seed: u64,
) -> Option<TerrainLandformRecipe> {
    let latitude = center.latitude().degrees();
    let longitude = center.longitude().degrees();
    let longitude_scale =
        (METRES_PER_LATITUDE_DEGREE * latitude.to_radians().cos()).max(MIN_LONGITUDE_SCALE);
    let playable_half_extent =
        f64::from(PLAYABLE_SIDE - 1) * f64::from(PLAYABLE_SPACING_METRES) * 0.5;
    let mut nearest: Option<(f64, [f64; 2], [i32; 2], TerrainLandformLod)> = None;
    for feature in terrain_features {
        let TerrainFeature::MappedFault(fault) = feature else {
            continue;
        };
        for segment in fault.trace.windows(2) {
            let a = [
                (segment[0].longitude() - longitude) * longitude_scale,
                (segment[0].latitude() - latitude) * METRES_PER_LATITUDE_DEGREE,
            ];
            let b = [
                (segment[1].longitude() - longitude) * longitude_scale,
                (segment[1].latitude() - latitude) * METRES_PER_LATITUDE_DEGREE,
            ];
            let tangent = [b[0] - a[0], b[1] - a[1]];
            let length_squared = tangent[0] * tangent[0] + tangent[1] * tangent[1];
            if length_squared < 1.0 {
                continue;
            }
            let fraction =
                (-(a[0] * tangent[0] + a[1] * tangent[1]) / length_squared).clamp(0.0, 1.0);
            let closest = [a[0] + tangent[0] * fraction, a[1] + tangent[1] * fraction];
            let origin_cm = [
                (closest[0] * 100.0).round() as i32,
                (closest[1] * 100.0).round() as i32,
            ];
            let distance_squared = closest[0] * closest[0] + closest[1] * closest[1];
            let Some(lod) = scarp_lod(closest, tangent, playable_half_extent) else {
                continue;
            };
            if nearest.is_none_or(|(best, _, _, _)| distance_squared < best) {
                nearest = Some((distance_squared, tangent, origin_cm, lod));
            }
        }
    }
    let (_, tangent, origin_cm, lod) = nearest?;
    let length = (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
    Some(TerrainLandformRecipe {
        kind: TerrainLandformKind::FaultScarp,
        seed,
        // The closest point is the canonical mapped trace projected into the
        // scene's east/north frame. LOD changes sampling resolution only; the
        // feature's position and physical dimensions remain canonical.
        origin_cm,
        tangent_permyriad: [
            (tangent[0] / length * 10_000.0).round() as i16,
            (tangent[1] / length * 10_000.0).round() as i16,
        ],
        relief_cm: SCARP_DEFAULT_THROW_CM,
        half_length_cm: SCARP_DEFAULT_HALF_LENGTH_CM,
        half_width_cm: SCARP_DEFAULT_HALF_WIDTH_CM,
        collar_cm: SCARP_DEFAULT_COLLAR_CM,
        lod,
    })
}

fn scarp_lod(
    origin_metres: [f64; 2],
    tangent: [f64; 2],
    playable_half_extent: f64,
) -> Option<TerrainLandformLod> {
    if origin_metres
        .into_iter()
        .all(|coordinate| coordinate.abs() <= playable_half_extent)
    {
        return Some(TerrainLandformLod::Detail);
    }
    let length = (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
    let unit = [tangent[0] / length, tangent[1] / length];
    let half_length = f64::from(SCARP_DEFAULT_HALF_LENGTH_CM) / 100.0;
    let half_width = f64::from(SCARP_DEFAULT_HALF_WIDTH_CM) / 100.0;
    let extent = [
        unit[0].abs() * half_length + unit[1].abs() * half_width,
        unit[1].abs() * half_length + unit[0].abs() * half_width,
    ];
    origin_metres
        .into_iter()
        .zip(extent)
        .all(|(coordinate, footprint_extent)| {
            coordinate.abs() <= playable_half_extent + footprint_extent
        })
        .then_some(TerrainLandformLod::Fringe)
}

fn level_distant_city_vista(
    vista: &mut VistaSample,
    buildings: &[DistantBuildingPlacement],
    elevation_metres: f32,
) {
    let Some(city_half_extent) = buildings
        .iter()
        .map(|building| building.centre_metres.abs())
        .reduce(Vec2::max)
        .map(|extent| extent + Vec2::splat(DISTANT_CITY_PAD_MARGIN_METRES))
    else {
        return;
    };
    for lod in &mut vista.lods {
        let grid_centre = Vec2::new(
            (f32::from(lod.width) - 1.0) * 0.5,
            (f32::from(lod.depth) - 1.0) * 0.5,
        );
        for (index, height) in lod.heights_metres.iter_mut().enumerate() {
            let grid = Vec2::new(
                (index % usize::from(lod.width)) as f32,
                (index / usize::from(lod.width)) as f32,
            );
            let point = (grid - grid_centre) * lod.spacing_metres;
            let outside = (point.abs() - city_half_extent).max(Vec2::ZERO).length();
            let weight = (1.0 - outside / DISTANT_CITY_LEVEL_BLEND_METRES).clamp(0.0, 1.0);
            let smooth_weight = weight * weight * (3.0 - 2.0 * weight);
            *height += (elevation_metres - *height) * smooth_weight;
        }
    }
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
    request: GridSampleRequest,
) -> Result<TerrainSampleGrid, String> {
    let GridDimensions { width, depth } = request.dimensions;
    let center_x = f64::from(width - 1) * 0.5;
    let center_z = f64::from(depth - 1) * 0.5;
    let mut heights = Vec::with_capacity(usize::from(width) * usize::from(depth));
    let mut environment = Vec::with_capacity(heights.capacity());
    for z in 0..depth {
        for x in 0..width {
            let east = (f64::from(x) - center_x) * f64::from(request.spacing_metres);
            let north = (f64::from(z) - center_z) * f64::from(request.spacing_metres);
            let (sample_latitude, sample_longitude) = offset_coordinate(
                request.center.latitude().degrees(),
                request.center.longitude().degrees(),
                east,
                north,
            );
            let cell = pack
                .cell(sample_latitude, sample_longitude)
                .map_err(|error| error.to_string())?
                .ok_or("requested scene window leaves the final terrain pack")?;
            let mut elevation = f32::from(cell.elevation_m);
            if matches!(request.elevation_sampling, ElevationSampling::PreservePeaks) {
                let radius = f64::from(request.spacing_metres) * PEAK_SAMPLE_RADIUS_FACTOR;
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
                let detail = deterministic_detail(request.seed, x, z)
                    * f32::from(cell.hilly_fraction_percent)
                    / f32::from(PERCENT_PER_WHOLE)
                    * HILLY_DETAIL_AMPLITUDE_METRES;
                elevation += detail;
            }
            heights.push(elevation - request.center_elevation_metres);
            environment.push(environment_sample(cell));
        }
    }
    Ok(TerrainSampleGrid {
        width,
        depth,
        spacing_metres: request.spacing_metres,
        heights_metres: heights,
        environment,
    })
}

fn environment_sample(cell: Cell) -> EnvironmentalSample {
    EnvironmentalSample {
        canopy_bps: u16::from(cell.canopy_percent) * BASIS_POINTS_PER_PERCENT,
        wetland_bps: u16::from(cell.wetland_fraction_percent) * BASIS_POINTS_PER_PERCENT,
        cultivation_bps: if cell.cultivated {
            BASIS_POINTS_PER_WHOLE
        } else {
            0
        },
        water_bps: if cell.surface == Surface::Water && !cell.crossing {
            BASIS_POINTS_PER_WHOLE
        } else {
            0
        },
        hilly_bps: u16::from(cell.hilly_fraction_percent) * BASIS_POINTS_PER_PERCENT,
        crossing_bps: if cell.crossing {
            BASIS_POINTS_PER_WHOLE
        } else {
            0
        },
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
    let latitude_delta = north / METRES_PER_LATITUDE_DEGREE;
    let longitude_scale = latitude.to_radians().cos().abs().max(MIN_LONGITUDE_SCALE);
    let longitude_delta = east / (METRES_PER_LATITUDE_DEGREE * longitude_scale);
    (latitude + latitude_delta, longitude + longitude_delta)
}

fn deterministic_seed(mission_id: &str) -> u64 {
    let digest = Sha256::digest(mission_id.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix"))
}

fn deterministic_detail(seed: u64, x: u16, z: u16) -> f32 {
    let value = mix64(seed ^ (u64::from(x) << 32) ^ u64::from(z));
    (value % RANDOM_DETAIL_BUCKETS) as f32 / RANDOM_DETAIL_SCALE as f32 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_world_schema::{MappedFault, TravelGeometryPoint};
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
            123_456,
            None,
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
        let landform = input
            .landform
            .expect("terrain-pack feature should produce a scarp");
        assert!((1_050..=1_180).contains(&landform.origin_cm[1]));
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

    #[test]
    fn nearby_fault_keeps_its_canonical_offset_and_source_orientation() {
        let center = Wgs84CoordinateE7::new(510_000_000, 100_000_000).unwrap();
        let faults = vec![TerrainFeature::MappedFault(MappedFault {
            id: "DE:test".into(),
            local_name: Some("fixture".into()),
            classification: None,
            mapped_active: false,
            mapped_capable: false,
            trace: vec![
                TravelGeometryPoint::new(9.99, 51.0001).unwrap(),
                TravelGeometryPoint::new(10.01, 51.0001).unwrap(),
            ],
        })];
        let recipe = nearest_fault_scarp(&faults, center, 42).unwrap();
        assert!(recipe.tangent_permyriad[0] > 9_900);
        assert!(recipe.tangent_permyriad[1].abs() < 100);
        assert_eq!(recipe.origin_cm[0], 0);
        assert!((1_050..=1_180).contains(&recipe.origin_cm[1]));
        assert_eq!(recipe.relief_cm, 800);
        assert_eq!(recipe.half_length_cm, SCARP_DEFAULT_HALF_LENGTH_CM);
        assert_eq!(recipe.half_width_cm, SCARP_DEFAULT_HALF_WIDTH_CM);
        assert_eq!(recipe.lod, TerrainLandformLod::Detail);
    }

    #[test]
    fn overlapping_fault_uses_coarser_lod_without_changing_its_extent() {
        let center = Wgs84CoordinateE7::new(510_000_000, 100_000_000).unwrap();
        let latitude_offset = 60.0 / METRES_PER_LATITUDE_DEGREE;
        let faults = vec![TerrainFeature::MappedFault(MappedFault {
            id: "DE:fringe".into(),
            local_name: None,
            classification: None,
            mapped_active: false,
            mapped_capable: false,
            trace: vec![
                TravelGeometryPoint::new(9.99, 51.0 + latitude_offset).unwrap(),
                TravelGeometryPoint::new(10.01, 51.0 + latitude_offset).unwrap(),
            ],
        })];

        let recipe = nearest_fault_scarp(&faults, center, 42).unwrap();

        assert!((5_950..=6_050).contains(&recipe.origin_cm[1]));
        assert_eq!(recipe.half_length_cm, SCARP_DEFAULT_HALF_LENGTH_CM);
        assert_eq!(recipe.half_width_cm, SCARP_DEFAULT_HALF_WIDTH_CM);
        assert_eq!(recipe.lod, TerrainLandformLod::Fringe);
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
            terrain_features: vec![TerrainFeature::MappedFault(MappedFault {
                id: "DE:pack-fixture".into(),
                local_name: Some("pack fixture".into()),
                classification: None,
                mapped_active: false,
                mapped_capable: false,
                trace: vec![
                    TravelGeometryPoint::new(10.49, 50.5001).unwrap(),
                    TravelGeometryPoint::new(10.51, 50.5001).unwrap(),
                ],
            })],
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
