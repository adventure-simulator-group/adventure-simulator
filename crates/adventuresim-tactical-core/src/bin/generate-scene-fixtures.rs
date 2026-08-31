use std::{fs, path::PathBuf};

use adventuresim_building_generator::{BuildingArchetype, BuildingProgram};
use adventuresim_core::weather::{Precipitation, WEATHER_RULES_VERSION, WeatherSnapshot};
use adventuresim_tactical_core::prelude::*;
use bevy::math::Vec2;
use fabelgeist_determinism::splitmix64;

#[path = "generate_scene_fixtures/fault.rs"]
mod fault;

const DEFAULT_TEST_MINUTE: u64 = 339_840 + 10 * 60;
const MASSIVE_CITY_GRID_SIDE: u16 = 21;
const MASSIVE_CITY_CELL_SIDE_METRES: f32 = 30.0;
const MASSIVE_CITY_DISTANT_BUILDINGS: usize = 334;
const MASSIVE_CITY_LEVEL_HALF_EXTENT_METRES: f32 = 350.0;
const MASSIVE_CITY_LEVEL_BLEND_METRES: f32 = 100.0;

#[derive(Clone, Copy)]
struct Fixture {
    name: &'static str,
    scene_key: &'static str,
    seed: u64,
    terrain: fn(f32, f32) -> f32,
    environment: fn(f32, f32) -> EnvironmentalSample,
    weather: WeatherSnapshot,
    vista: VistaKind,
    fault_scarp: Option<FaultScarpRecipe>,
    buildings: BuildingFixture,
}

#[derive(Clone, Copy)]
enum VistaKind {
    Ordinary,
    ValleyRidge,
    BoundaryPeak,
}

#[derive(Clone, Copy)]
enum BuildingFixture {
    Empty,
    Cottage,
    MassiveCity,
}

fn main() {
    let check = std::env::args().any(|argument| argument == "--check");
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = repository.join("assets/tactical-scenes");
    if !check {
        fs::create_dir_all(&output).expect("create fixture directory");
    }
    for fixture in fixtures() {
        let input = build_fixture(fixture);
        input.validate().expect(fixture.name);
        let json = serde_json::to_string_pretty(&input).expect("serialize fixture") + "\n";
        let path = output.join(format!("{}.json", fixture.name));
        if check {
            assert_eq!(
                fs::read_to_string(path).expect("read fixture"),
                json,
                "fixture {} is stale",
                fixture.name
            );
        } else {
            fs::write(path, json).expect("write fixture");
        }
    }
}

fn fixtures() -> [Fixture; 15] {
    [
        Fixture {
            buildings: BuildingFixture::Cottage,
            ..fixture(
                "flat-dry-grassland",
                "grassland",
                47_101,
                flat,
                dry_open,
                clear(),
            )
        },
        Fixture {
            buildings: BuildingFixture::MassiveCity,
            ..fixture("massive-city", "city", 47_114, flat, dry_open, clear())
        },
        fixture(
            "steep-open-hillside",
            "hillside",
            47_102,
            hillside,
            rocky_open,
            clear(),
        ),
        fault::fixture(),
        fixture(
            "dense-woodland",
            "woodland",
            42,
            rolling,
            dense_woods,
            clear(),
        ),
        fixture(
            "sparse-woodland",
            "woodland",
            47_104,
            rolling,
            sparse_woods,
            clear(),
        ),
        fixture(
            "saturated-wetland",
            "wetland",
            47_105,
            wetland,
            saturated,
            rain(7_500, 4_000),
        ),
        fixture(
            "cultivated-roadside",
            "roadside",
            47_106,
            roadside,
            cultivated_road,
            clear(),
        ),
        fixture(
            "snow-covered-ground",
            "snowfield",
            47_107,
            snowfield,
            snow_open,
            snow(6_500, 2_500),
        ),
        fixture(
            "light-rain-low-wind",
            "rain",
            47_112,
            rolling,
            wet_open,
            rain(2_500, 2_000),
        ),
        fixture(
            "heavy-rain-high-wind",
            "storm",
            47_108,
            rolling,
            wet_open,
            rain(9_500, 9_000),
        ),
        fixture(
            "severe-downpour",
            "severe-storm",
            47_113,
            rolling,
            wet_open,
            rain(10_000, 10_000),
        ),
        Fixture {
            vista: VistaKind::ValleyRidge,
            ..fixture(
                "valley-distant-ridge",
                "valley",
                47_109,
                valley,
                dry_open,
                clear(),
            )
        },
        Fixture {
            vista: VistaKind::BoundaryPeak,
            ..fixture(
                "narrow-peak-lod-boundary",
                "mountain",
                47_110,
                rolling,
                dry_open,
                clear(),
            )
        },
        fixture(
            "playability-repair-required",
            "floodplain",
            47_111,
            blocked,
            water_dominated,
            rain(8_000, 3_000),
        ),
    ]
}

const fn fixture(
    name: &'static str,
    scene_key: &'static str,
    seed: u64,
    terrain: fn(f32, f32) -> f32,
    environment: fn(f32, f32) -> EnvironmentalSample,
    weather: WeatherSnapshot,
) -> Fixture {
    Fixture {
        name,
        scene_key,
        seed,
        terrain,
        environment,
        weather,
        vista: VistaKind::Ordinary,
        fault_scarp: None,
        buildings: BuildingFixture::Empty,
    }
}

fn build_fixture(fixture: Fixture) -> TacticalSceneInput {
    let (buildings, distant_buildings) = fixture_buildings(fixture.buildings);
    let mut vista = vista(
        fixture.vista,
        fixture.environment,
        (fixture.terrain)(0.0, 0.0),
    );
    level_city_vista(&mut vista, !distant_buildings.is_empty());
    TacticalSceneInput {
        schema_version: TACTICAL_SCENE_SCHEMA_VERSION,
        generation_version: TACTICAL_SCENE_GENERATION_VERSION,
        seed: fixture.seed,
        scene_key: fixture.scene_key.into(),
        source: SceneSource::SyntheticFixture(fixture.name.into()),
        latitude_microdegrees: 53_500_000,
        longitude_microdegrees: 10_000_000,
        absolute_minute: fixture.weather.interval_start_minute,
        lunar_phase_minute: fixture.weather.interval_start_minute,
        absolute_elevation_metres: 42,
        playable: grid(9, 9, 12.5, fixture.terrain, fixture.environment),
        fault_scarp: fixture.fault_scarp,
        buildings,
        distant_buildings,
        vista,
        weather: fixture.weather,
    }
}

fn fixture_buildings(
    buildings: BuildingFixture,
) -> (
    Vec<TacticalBuildingPlacement>,
    Vec<DistantBuildingPlacement>,
) {
    match buildings {
        BuildingFixture::Empty => (Vec::new(), Vec::new()),
        BuildingFixture::Cottage => (
            vec![building(
                1,
                BuildingArchetype::FachwerkCottage,
                42,
                Vec2::new(12.0, 4.0),
                1,
            )],
            Vec::new(),
        ),
        BuildingFixture::MassiveCity => (playable_city_buildings(), distant_city_buildings()),
    }
}

fn playable_city_buildings() -> Vec<TacticalBuildingPlacement> {
    [
        (
            BuildingArchetype::FachwerkMerchantHouse,
            42,
            -30.0,
            -30.0,
            0,
        ),
        (BuildingArchetype::TownHouse, 47, 0.0, -30.0, 1),
        (BuildingArchetype::FachwerkCottage, 101, 30.0, -30.0, 2),
        (BuildingArchetype::FachwerkCottage, 47, -30.0, 30.0, 3),
        (BuildingArchetype::FachwerkMerchantHouse, 101, 0.0, 30.0, 2),
        (BuildingArchetype::TownHouse, 42, 30.0, 30.0, 1),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (archetype, seed, x, z, quarter_turns))| {
        building(
            index as u64 + 1,
            archetype,
            seed,
            Vec2::new(x, z),
            quarter_turns,
        )
    })
    .collect()
}

fn distant_city_buildings() -> Vec<DistantBuildingPlacement> {
    let offset = i32::from(MASSIVE_CITY_GRID_SIDE) / 2;
    let mut cells = (0..MASSIVE_CITY_GRID_SIDE * MASSIVE_CITY_GRID_SIDE)
        .filter(|cell| {
            let x = i32::from(*cell % MASSIVE_CITY_GRID_SIDE) - offset;
            let z = i32::from(*cell / MASSIVE_CITY_GRID_SIDE) - offset;
            x.abs() > 1 || z.abs() > 1
        })
        .collect::<Vec<_>>();
    cells.sort_by_key(|cell| splitmix64(47_114 ^ u64::from(*cell)));
    let palette = [
        (BuildingArchetype::FachwerkMerchantHouse, 42),
        (BuildingArchetype::TownHouse, 47),
        (BuildingArchetype::FachwerkCottage, 101),
        (BuildingArchetype::TownHouse, 42),
        (BuildingArchetype::FachwerkCottage, 47),
        (BuildingArchetype::FachwerkMerchantHouse, 101),
    ];
    cells
        .into_iter()
        .take(MASSIVE_CITY_DISTANT_BUILDINGS)
        .map(|cell| {
            let selection = splitmix64(47_114 ^ 0x6469_7374_616e_7401 ^ u64::from(cell));
            let (archetype, seed) = palette[selection as usize % palette.len()];
            let x = i32::from(cell % MASSIVE_CITY_GRID_SIDE) - offset;
            let z = i32::from(cell / MASSIVE_CITY_GRID_SIDE) - offset;
            DistantBuildingPlacement {
                id: u64::from(cell) + 1,
                archetype,
                seed,
                centre_metres: Vec2::new(
                    x as f32 * MASSIVE_CITY_CELL_SIDE_METRES,
                    z as f32 * MASSIVE_CITY_CELL_SIDE_METRES,
                ),
                base_elevation_metres: 0.0,
                quarter_turns: (selection >> 32) as u8 % 4,
            }
        })
        .collect()
}

fn level_city_vista(vista: &mut VistaSample, has_city: bool) {
    if !has_city {
        return;
    }
    for lod in &mut vista.lods {
        let centre = Vec2::new(
            (f32::from(lod.width) - 1.0) * 0.5,
            (f32::from(lod.depth) - 1.0) * 0.5,
        );
        for (index, height) in lod.heights_metres.iter_mut().enumerate() {
            let grid = Vec2::new(
                (index % usize::from(lod.width)) as f32,
                (index / usize::from(lod.width)) as f32,
            );
            let point = (grid - centre) * lod.spacing_metres;
            let outside = (point.abs() - Vec2::splat(MASSIVE_CITY_LEVEL_HALF_EXTENT_METRES))
                .max(Vec2::ZERO)
                .length();
            let weight = (1.0 - outside / MASSIVE_CITY_LEVEL_BLEND_METRES).clamp(0.0, 1.0);
            let smooth_weight = weight * weight * (3.0 - 2.0 * weight);
            *height *= 1.0 - smooth_weight;
        }
    }
}

fn building(
    id: u64,
    archetype: BuildingArchetype,
    seed: u64,
    centre_metres: Vec2,
    quarter_turns: u8,
) -> TacticalBuildingPlacement {
    TacticalBuildingPlacement {
        id,
        program: BuildingProgram::fixture(archetype, seed),
        centre_metres,
        quarter_turns,
    }
}

fn grid(
    width: u16,
    depth: u16,
    spacing: f32,
    height: fn(f32, f32) -> f32,
    environment: fn(f32, f32) -> EnvironmentalSample,
) -> TerrainSampleGrid {
    let center_x = f32::from(width - 1) * 0.5;
    let center_z = f32::from(depth - 1) * 0.5;
    let points = (0..depth).flat_map(|z| {
        (0..width).map(move |x| {
            (
                (f32::from(x) - center_x) * spacing,
                (f32::from(z) - center_z) * spacing,
            )
        })
    });
    let (heights_metres, environment) = points
        .map(|(x, z)| (height(x, z), environment(x, z)))
        .unzip();
    TerrainSampleGrid {
        width,
        depth,
        spacing_metres: spacing,
        heights_metres,
        environment,
    }
}

fn vista(
    kind: VistaKind,
    environment: fn(f32, f32) -> EnvironmentalSample,
    playable_center_height: f32,
) -> VistaSample {
    // The former 250 m first vista tier exposed broad planar facets directly
    // beyond the 100 m playable field. Keep the same one-kilometre near-vista
    // reach with a 100 m lattice. A 250 m odd middle grid preserves the same
    // exact 10 km extent, center datum, and boundary landmarks without showing
    // 500 m triangular facets across isolated peaks and valley walls.
    let specs = [(0, 100.0, 21), (1, 250.0, 41), (2, 1_000.0, 51)];
    VistaSample {
        lods: specs
            .into_iter()
            .map(|(level, spacing, side)| {
                let terrain = match kind {
                    VistaKind::Ordinary => distant_rolling,
                    VistaKind::ValleyRidge => distant_valley_ridge,
                    VistaKind::BoundaryPeak => distant_boundary_peak,
                };
                let mut sample = grid(side, side, spacing, terrain, environment);
                let vista_center_height = terrain(0.0, 0.0);
                for height in &mut sample.heights_metres {
                    *height += playable_center_height - vista_center_height;
                }
                VistaLod {
                    level,
                    spacing_metres: sample.spacing_metres,
                    width: sample.width,
                    depth: sample.depth,
                    origin_east_metres: 0.0,
                    origin_north_metres: 0.0,
                    heights_metres: sample.heights_metres,
                    environment: sample.environment,
                }
            })
            .collect(),
    }
}

fn sample(
    surface: TacticalSurface,
    canopy: u16,
    wetland: u16,
    cultivation: u16,
    water: u16,
) -> EnvironmentalSample {
    EnvironmentalSample {
        canopy_bps: canopy,
        wetland_bps: wetland,
        cultivation_bps: cultivation,
        water_bps: water,
        hilly_bps: 0,
        crossing_bps: 0,
        surface,
    }
}
fn dry_open(_: f32, _: f32) -> EnvironmentalSample {
    sample(TacticalSurface::Open, 300, 0, 0, 0)
}
fn rocky_open(x: f32, z: f32) -> EnvironmentalSample {
    EnvironmentalSample {
        hilly_bps: 8_000,
        ..dry_open(x, z)
    }
}
fn wet_open(_: f32, _: f32) -> EnvironmentalSample {
    sample(TacticalSurface::Open, 500, 2_000, 0, 0)
}
fn dense_woods(_: f32, _: f32) -> EnvironmentalSample {
    sample(TacticalSurface::DeepWoods, 9_000, 500, 0, 0)
}
fn sparse_woods(_: f32, _: f32) -> EnvironmentalSample {
    sample(TacticalSurface::SparseWoods, 3_500, 300, 0, 0)
}
fn saturated(x: f32, z: f32) -> EnvironmentalSample {
    // Alternating pools and hummocks give the wetland both visible structure
    // and enough woody cover to support its required understory community.
    let basin = ((x / 8.0).sin() * 0.35
        + (z / 11.0).cos() * 0.28
        + ((x + z) / 6.0).sin() * 0.22
        + ((x - z) / 15.0).cos() * 0.15
        + 1.0)
        * 0.5;
    if basin > 0.62 {
        sample(TacticalSurface::Wetland, 1_400, 9_800, 0, 6_500)
    } else {
        sample(TacticalSurface::Wetland, 2_800, 8_800, 0, 1_200)
    }
}
fn cultivated_road(x: f32, _: f32) -> EnvironmentalSample {
    if x.abs() <= 11.0 {
        sample(TacticalSurface::Road, 0, 0, 7_000, 0)
    } else {
        sample(TacticalSurface::Open, 500, 0, 9_000, 0)
    }
}
fn water_dominated(x: f32, z: f32) -> EnvironmentalSample {
    if x.abs() < 42.0 || z.abs() < 42.0 {
        sample(TacticalSurface::Water, 0, 8_000, 0, 10_000)
    } else {
        dry_open(x, z)
    }
}

fn flat(_: f32, _: f32) -> f32 {
    0.0
}
fn rolling(x: f32, z: f32) -> f32 {
    (x / 22.0).sin() * 1.8 + (z / 31.0).cos() * 1.2
}
fn hillside(x: f32, z: f32) -> f32 {
    x * 0.36 + (z / 18.0).sin() * 1.35 + ((x + z) / 11.0).sin() * 0.45
}
fn wetland(x: f32, z: f32) -> f32 {
    (x / 18.0).sin() * 0.24 + (z / 20.0).cos() * 0.18 + ((x - z) / 12.0).sin() * 0.11
}
fn roadside(x: f32, z: f32) -> f32 {
    let across = x.abs();
    if across <= 10.0 {
        // Shallow camber keeps the route readable without making its centre a
        // perfectly flat strip.
        -0.22 + across * 0.012
    } else if across <= 16.0 {
        // Bounded drainage ditches distinguish the road edge from the field.
        -0.34 + (across - 13.0).abs() * 0.035
    } else {
        0.16 + (z / 30.0).sin() * 0.4
    }
}
fn snowfield(x: f32, z: f32) -> f32 {
    rolling(x, z) * 0.62 + (x / 9.0).sin() * (z / 13.0).cos() * 0.34
}
fn snow_open(x: f32, z: f32) -> EnvironmentalSample {
    EnvironmentalSample {
        hilly_bps: 1_200,
        ..dry_open(x, z)
    }
}
fn valley(x: f32, z: f32) -> f32 {
    x.abs() * 0.08 + (z / 26.0).sin() * 0.5
}
fn blocked(x: f32, z: f32) -> f32 {
    if x.abs() < 20.0 && z.abs() < 20.0 {
        18.0
    } else {
        0.0
    }
}
fn distant_rolling(x: f32, z: f32) -> f32 {
    (x / 3_000.0).sin() * 45.0 + (z / 4_000.0).cos() * 30.0
}
fn distant_valley_ridge(x: f32, z: f32) -> f32 {
    let valley_walls = (x.abs() * 0.014).min(310.0);
    let ridge_offset = (z - 4_400.0) / 1_650.0;
    valley_walls + (-0.5 * ridge_offset * ridge_offset).exp() * 430.0 + (z / 5_000.0).sin() * 38.0
}
fn distant_boundary_peak(x: f32, z: f32) -> f32 {
    let distance = ((x - 5_000.0).powi(2) + z.powi(2)).sqrt();
    let summit = (-0.5 * (distance / 1_150.0).powi(2)).exp() * 720.0;
    let shoulder = (-0.5 * (distance / 2_450.0).powi(2)).exp() * 180.0;
    let origin_profile = (-0.5_f32 * (5_000.0_f32 / 1_150.0).powi(2)).exp() * 720.0
        + (-0.5_f32 * (5_000.0_f32 / 2_450.0).powi(2)).exp() * 180.0;
    let ridge = (summit + shoulder - origin_profile) * 900.0 / (900.0 - origin_profile);
    ridge.max(distant_rolling(x, z) - distant_rolling(0.0, 0.0))
}

const fn clear() -> WeatherSnapshot {
    weather(Precipitation::Clear, 0, 1_200, 100, 0, 120)
}
const fn rain(intensity: u16, wind: u16) -> WeatherSnapshot {
    weather(Precipitation::Rain, intensity, wind, 8_000, 0, 75)
}
const fn snow(intensity: u16, wind: u16) -> WeatherSnapshot {
    weather(Precipitation::Snow, intensity, wind, 2_000, 8_500, -40)
}
const fn weather(
    precipitation: Precipitation,
    intensity_bps: u16,
    wind_speed_bps: u16,
    ground_moisture_bps: u16,
    snow_cover_bps: u16,
    temperature_deci_c: i32,
) -> WeatherSnapshot {
    WeatherSnapshot {
        rules_version: WEATHER_RULES_VERSION,
        interval_start_minute: DEFAULT_TEST_MINUTE,
        cell_latitude: 214,
        cell_longitude: 40,
        temperature_deci_c,
        wind_speed_bps,
        precipitation,
        intensity_bps,
        ground_moisture_bps,
        snow_cover_bps,
        atmosphere: match precipitation {
            Precipitation::Clear => AtmosphericSnapshot {
                relative_humidity_bps: 5_800,
                dew_point_deci_c: temperature_deci_c - 84,
                sea_level_pressure_deci_hpa: 10_180,
                wind_direction_degrees: 245,
                wind_shear_bps: 2_500,
                instability_bps: 4_200,
                lift_bps: -500,
                low_cloud: Some(CloudLayerSnapshot {
                    form: CloudForm::Cumulus,
                    coverage_bps: 2_800,
                    optical_density_bps: 4_000,
                    base_metres: 1_050,
                    top_metres: 2_700,
                }),
                middle_cloud: None,
                high_cloud: None,
            },
            Precipitation::Rain => AtmosphericSnapshot {
                relative_humidity_bps: 9_500,
                dew_point_deci_c: temperature_deci_c - 10,
                sea_level_pressure_deci_hpa: 9_920,
                wind_direction_degrees: 70,
                wind_shear_bps: 7_500,
                instability_bps: 8_200,
                lift_bps: 7_000,
                low_cloud: Some(CloudLayerSnapshot {
                    form: CloudForm::Cumulonimbus,
                    coverage_bps: 8_800,
                    optical_density_bps: 9_000,
                    base_metres: 500,
                    top_metres: 10_500,
                }),
                middle_cloud: None,
                high_cloud: Some(CloudLayerSnapshot {
                    form: CloudForm::Cirrus,
                    coverage_bps: 3_500,
                    optical_density_bps: 2_000,
                    base_metres: 6_500,
                    top_metres: 10_500,
                }),
            },
            Precipitation::Snow => AtmosphericSnapshot {
                relative_humidity_bps: 9_200,
                dew_point_deci_c: temperature_deci_c - 16,
                sea_level_pressure_deci_hpa: 10_020,
                wind_direction_degrees: 110,
                wind_shear_bps: 3_500,
                instability_bps: 2_500,
                lift_bps: 4_000,
                low_cloud: Some(CloudLayerSnapshot {
                    form: CloudForm::Stratocumulus,
                    coverage_bps: 8_200,
                    optical_density_bps: 6_500,
                    base_metres: 550,
                    top_metres: 1_800,
                }),
                middle_cloud: Some(CloudLayerSnapshot {
                    form: CloudForm::Nimbostratus,
                    coverage_bps: 8_800,
                    optical_density_bps: 8_000,
                    base_metres: 1_800,
                    top_metres: 5_500,
                }),
                high_cloud: None,
            },
        },
    }
}
