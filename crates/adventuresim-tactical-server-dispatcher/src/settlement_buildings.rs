//! Cheap deterministic settlement layout for tactical scenes.

use std::{error::Error, fmt};

use adventuresim_building_generator::{BuildingArchetype, BuildingProgram, generate};
use adventuresim_core::reputation::effective_population;
use adventuresim_tactical_core::prelude::{DistantBuildingPlacement, TacticalBuildingPlacement};
use bevy::math::Vec2;
use fabelgeist_determinism::mix64;
use sha2::{Digest, Sha256};

const GRID_SIDE: u8 = 3;
const GRID_CELL_SIDE_METRES: f32 = 30.0;
const BUILDING_CELLS: [u8; 6] = [0, 1, 2, 6, 7, 8];
const RESIDENTS_PER_REPRESENTED_BUILDING: u32 = 2_000;
const CITY_GRID_SIDE: u16 = 21;
const DISTANT_RESIDENTS_PER_BUILDING: u32 = 120;
const DISTANT_RECIPE_VARIANTS: u64 = 6;
const BUILDING_SELECTION_DOMAIN: u64 = 0x6275_696c_6469_6e67;
const BUILDING_RECIPE_DOMAIN: u64 = 0x7265_6369_7065_0001;
const DISTANT_SELECTION_DOMAIN: u64 = 0x6469_7374_616e_7401;
const DISTANT_RECIPE_DOMAIN: u64 = 0x6469_7374_7265_6301;
const VALID_RECIPE_ATTEMPTS: u8 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettlementBuildingError {
    archetype: BuildingArchetype,
    initial_seed: u64,
}

impl fmt::Display for SettlementBuildingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "no valid {:?} recipe was found from deterministic seed {} in {} attempts",
            self.archetype, self.initial_seed, VALID_RECIPE_ATTEMPTS
        )
    }
}

impl Error for SettlementBuildingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementSceneProfile {
    pub id: String,
    pub population_level: i32,
    pub population_estimate: u32,
}

impl SettlementSceneProfile {
    fn effective_population(&self) -> u32 {
        effective_population(self.population_level, self.population_estimate)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettlementBuildingLayout {
    pub playable: Vec<TacticalBuildingPlacement>,
    pub distant: Vec<DistantBuildingPlacement>,
}

pub fn place_settlement_buildings(
    settlement: &SettlementSceneProfile,
) -> Result<SettlementBuildingLayout, SettlementBuildingError> {
    let population = settlement.effective_population();
    let occupied_cells = population
        .div_ceil(RESIDENTS_PER_REPRESENTED_BUILDING)
        .clamp(1, BUILDING_CELLS.len() as u32) as usize;
    let settlement_seed = settlement_seed(&settlement.id);
    let mut cells = BUILDING_CELLS.to_vec();
    cells.sort_by_key(|cell| mix64(settlement_seed ^ BUILDING_SELECTION_DOMAIN ^ u64::from(*cell)));
    let playable = cells
        .into_iter()
        .take(occupied_cells)
        .map(|cell| placement(settlement_seed, population, cell))
        .collect::<Result<Vec<_>, _>>()?;
    let distant = distant_placements(settlement_seed, population)?;
    Ok(SettlementBuildingLayout { playable, distant })
}

fn distant_placements(
    settlement_seed: u64,
    population: u32,
) -> Result<Vec<DistantBuildingPlacement>, SettlementBuildingError> {
    let playable_offset = i32::from(GRID_SIDE) / 2;
    let city_offset = i32::from(CITY_GRID_SIDE) / 2;
    let mut cells = (0..CITY_GRID_SIDE * CITY_GRID_SIDE)
        .filter(|cell| {
            let x = i32::from(*cell % CITY_GRID_SIDE) - city_offset;
            let z = i32::from(*cell / CITY_GRID_SIDE) - city_offset;
            x.abs() > playable_offset || z.abs() > playable_offset
        })
        .collect::<Vec<_>>();
    cells.sort_by_key(|cell| mix64(settlement_seed ^ DISTANT_SELECTION_DOMAIN ^ u64::from(*cell)));
    let building_count = population
        .div_ceil(DISTANT_RESIDENTS_PER_BUILDING)
        .min(cells.len() as u32) as usize;
    let palette = (0..DISTANT_RECIPE_VARIANTS)
        .map(|variant| {
            let recipe_seed = mix64(settlement_seed ^ DISTANT_RECIPE_DOMAIN ^ variant);
            let archetype = building_archetype(population, recipe_seed);
            valid_building_program(archetype, recipe_seed)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(cells
        .into_iter()
        .take(building_count)
        .map(|cell| {
            let grid_x = i32::from(cell % CITY_GRID_SIDE) - city_offset;
            let grid_z = i32::from(cell / CITY_GRID_SIDE) - city_offset;
            let selection = mix64(settlement_seed ^ DISTANT_RECIPE_DOMAIN ^ u64::from(cell));
            let program = &palette[selection as usize % palette.len()];
            DistantBuildingPlacement {
                id: u64::from(cell) + 1,
                archetype: program.archetype,
                seed: program.seed,
                centre_metres: Vec2::new(
                    grid_x as f32 * GRID_CELL_SIDE_METRES,
                    grid_z as f32 * GRID_CELL_SIDE_METRES,
                ),
                base_elevation_metres: 0.0,
                quarter_turns: (selection >> 32) as u8 % 4,
            }
        })
        .collect())
}

fn placement(
    settlement_seed: u64,
    population: u32,
    cell: u8,
) -> Result<TacticalBuildingPlacement, SettlementBuildingError> {
    let recipe_seed = mix64(settlement_seed ^ BUILDING_RECIPE_DOMAIN ^ u64::from(cell));
    let grid_x = cell % GRID_SIDE;
    let grid_z = cell / GRID_SIDE;
    let offset = (f32::from(GRID_SIDE) - 1.0) * 0.5;
    let archetype = building_archetype(population, recipe_seed);
    Ok(TacticalBuildingPlacement {
        id: u64::from(cell) + 1,
        program: valid_building_program(archetype, recipe_seed)?,
        centre_metres: Vec2::new(
            (f32::from(grid_x) - offset) * GRID_CELL_SIDE_METRES,
            (f32::from(grid_z) - offset) * GRID_CELL_SIDE_METRES,
        ),
        quarter_turns: (recipe_seed >> 32) as u8 % 4,
    })
}

fn valid_building_program(
    archetype: BuildingArchetype,
    initial_seed: u64,
) -> Result<BuildingProgram, SettlementBuildingError> {
    for attempt in 0..VALID_RECIPE_ATTEMPTS {
        let seed = if attempt == 0 {
            initial_seed
        } else {
            mix64(initial_seed ^ u64::from(attempt))
        };
        let program = BuildingProgram::fixture(archetype, seed);
        if generate(&program).is_ok() {
            return Ok(program);
        }
    }
    Err(SettlementBuildingError {
        archetype,
        initial_seed,
    })
}

fn building_archetype(population: u32, recipe_seed: u64) -> BuildingArchetype {
    match population {
        0..3_000 => BuildingArchetype::FachwerkCottage,
        3_000..10_000 => match recipe_seed % 3 {
            0 => BuildingArchetype::TownHouse,
            _ => BuildingArchetype::FachwerkCottage,
        },
        _ => match recipe_seed % 4 {
            0 => BuildingArchetype::FachwerkMerchantHouse,
            1 => BuildingArchetype::TownHouse,
            _ => BuildingArchetype::FachwerkCottage,
        },
    }
}

fn settlement_seed(id: &str) -> u64 {
    let digest = Sha256::digest(id.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_tactical_core::prelude::*;

    fn settlement(id: &str, population: u32) -> SettlementSceneProfile {
        SettlementSceneProfile {
            id: id.into(),
            population_level: 1,
            population_estimate: population,
        }
    }

    #[test]
    fn layout_is_stable_and_population_increases_occupied_cells() {
        let village = place_settlement_buildings(&settlement("lübeck", 900)).unwrap();
        let town = place_settlement_buildings(&settlement("lübeck", 6_500)).unwrap();
        let city = place_settlement_buildings(&settlement("lübeck", 40_000)).unwrap();
        assert_eq!(
            village,
            place_settlement_buildings(&settlement("lübeck", 900)).unwrap()
        );
        assert_eq!(
            (
                village.playable.len(),
                town.playable.len(),
                city.playable.len()
            ),
            (1, 4, 6)
        );
        assert_eq!(
            (
                village.distant.len(),
                town.distant.len(),
                city.distant.len()
            ),
            (8, 55, 334)
        );
        assert_eq!(
            village.playable[0].centre_metres,
            town.playable[0].centre_metres
        );
        assert_eq!(
            town.playable
                .iter()
                .map(|building| building.centre_metres)
                .collect::<Vec<_>>(),
            city.playable[..town.playable.len()]
                .iter()
                .map(|building| building.centre_metres)
                .collect::<Vec<_>>()
        );
        assert!(
            city.playable
                .iter()
                .all(|building| building.centre_metres.y.abs() == GRID_CELL_SIDE_METRES)
        );
    }

    #[test]
    fn missing_estimate_uses_the_shared_population_level_fallback() {
        let settlement = SettlementSceneProfile {
            id: "fallback-city".into(),
            population_level: 4,
            population_estimate: 0,
        };
        assert_eq!(
            place_settlement_buildings(&settlement)
                .unwrap()
                .playable
                .len(),
            5
        );
    }

    #[test]
    fn dense_city_layout_passes_tactical_pad_validation() {
        let mut input = TacticalSceneInput {
            schema_version: TACTICAL_SCENE_SCHEMA_VERSION,
            generation_version: TACTICAL_SCENE_GENERATION_VERSION,
            seed: 42,
            scene_key: "city".into(),
            source: SceneSource::SyntheticFixture("city".into()),
            latitude_microdegrees: 53_500_000,
            longitude_microdegrees: 10_000_000,
            absolute_minute: 1,
            lunar_phase_minute: 1,
            absolute_elevation_metres: 0,
            playable: TerrainSampleGrid {
                width: 101,
                depth: 101,
                spacing_metres: 1.0,
                heights_metres: vec![0.0; 101 * 101],
                environment: vec![EnvironmentalSample::default(); 101 * 101],
            },
            fault_scarp: None,
            buildings: place_settlement_buildings(&settlement("dense", 40_000))
                .unwrap()
                .playable,
            distant_buildings: Vec::new(),
            vista: VistaSample::default(),
            weather: adventuresim_core::weather::weather_at(42, 1, 53_500_000, 10_000_000, 0),
        };
        input.validate().unwrap();
        let generated = input.generate().unwrap();
        assert_eq!(generated.buildings.len(), 6);
        input.buildings.reverse();
        assert!(input.generate().is_ok());
    }

    #[test]
    fn invalid_initial_recipe_seed_advances_deterministically_to_a_valid_program() {
        let buildings = place_settlement_buildings(&settlement("massive-city-3229", 100_000))
            .expect("the deterministic retry sequence should find valid recipes");

        assert_eq!(buildings.playable.len(), BUILDING_CELLS.len());
        assert_eq!(buildings.distant.len(), 432);
        assert!(
            buildings
                .playable
                .iter()
                .all(|building| generate(&building.program).is_ok())
        );
        assert_eq!(
            buildings,
            place_settlement_buildings(&settlement("massive-city-3229", 100_000)).unwrap()
        );
    }
}
