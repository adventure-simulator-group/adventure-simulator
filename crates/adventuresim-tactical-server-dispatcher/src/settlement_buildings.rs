//! Cheap deterministic settlement layout for tactical scenes.

use std::{error::Error, fmt};

use adventuresim_building_generator::{BuildingArchetype, BuildingProgram, generate};
use adventuresim_core::reputation::effective_population;
use adventuresim_tactical_core::prelude::{
    CityHouseClass, CityStreetPatch, CityYardPatch, DistantBuildingPlacement,
    TacticalBuildingPlacement, generate_city,
};
use fabelgeist_determinism::mix64;
use sha2::{Digest, Sha256};

const RECIPE_VARIANTS_PER_CLASS: u64 = 2;
const BUILDING_RECIPE_DOMAIN: u64 = 0x7265_6369_7065_0001;
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
    pub streets: Vec<CityStreetPatch>,
    pub yards: Vec<CityYardPatch>,
    pub playable: Vec<TacticalBuildingPlacement>,
    pub distant: Vec<DistantBuildingPlacement>,
}

pub fn place_settlement_buildings(
    settlement: &SettlementSceneProfile,
    playable_half_extent_metres: f32,
) -> Result<SettlementBuildingLayout, SettlementBuildingError> {
    let population = settlement.effective_population();
    let settlement_seed = settlement_seed(&settlement.id);
    let palette = CityHouseClass::ALL
        .into_iter()
        .map(|house_class| {
            let variants = (0..RECIPE_VARIANTS_PER_CLASS)
                .map(|variant| {
                    let recipe_seed = mix64(
                        settlement_seed
                            ^ BUILDING_RECIPE_DOMAIN
                            ^ (house_class as u64).rotate_left(17)
                            ^ variant,
                    );
                    valid_building_program(building_archetype(house_class), recipe_seed)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((house_class, variants))
        })
        .collect::<Result<Vec<_>, SettlementBuildingError>>()?;
    let city = generate_city(settlement_seed, population);
    let mut layout = SettlementBuildingLayout {
        streets: city.streets,
        yards: city.yards,
        ..Default::default()
    };
    for lot in city.lots {
        let selection = mix64(settlement_seed ^ BUILDING_RECIPE_DOMAIN ^ lot.id);
        let variants = &palette
            .iter()
            .find(|(house_class, _)| *house_class == lot.house_class)
            .expect("every city house class has a recipe palette")
            .1;
        let program = &variants[selection as usize % variants.len()];
        if lot.centre_metres.abs().max_element() <= playable_half_extent_metres {
            layout.playable.push(TacticalBuildingPlacement {
                id: lot.id,
                program: program.clone(),
                centre_metres: lot.centre_metres,
                orientation: lot.orientation,
            });
        } else {
            layout.distant.push(DistantBuildingPlacement {
                id: lot.id,
                archetype: program.archetype,
                seed: program.seed,
                centre_metres: lot.centre_metres,
                base_elevation_metres: 0.0,
                orientation: lot.orientation,
            });
        }
    }
    Ok(layout)
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

fn building_archetype(house_class: CityHouseClass) -> BuildingArchetype {
    match house_class {
        CityHouseClass::Cottage => BuildingArchetype::FachwerkCottage,
        CityHouseClass::CraftTownHouse => BuildingArchetype::TownHouse,
        CityHouseClass::HallHouse => BuildingArchetype::HallHouse,
        CityHouseClass::MerchantHouse => BuildingArchetype::FachwerkMerchantHouse,
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

    fn layout(id: &str, population: u32) -> SettlementBuildingLayout {
        place_settlement_buildings(&settlement(id, population), 50.0).unwrap()
    }

    fn ordered_centres(layout: &SettlementBuildingLayout) -> Vec<(u64, bevy::math::Vec2)> {
        let mut centres = layout
            .playable
            .iter()
            .map(|building| (building.id, building.centre_metres))
            .chain(
                layout
                    .distant
                    .iter()
                    .map(|building| (building.id, building.centre_metres)),
            )
            .collect::<Vec<_>>();
        centres.sort_by_key(|(id, _)| *id);
        centres
    }

    #[test]
    fn layout_is_stable_and_population_increases_occupied_cells() {
        let village = layout("lübeck", 900);
        let town = layout("lübeck", 6_500);
        let city = layout("lübeck", 40_000);
        assert_eq!(village, layout("lübeck", 900));
        assert_eq!(
            (
                village.playable.len() + village.distant.len(),
                town.playable.len() + town.distant.len(),
                city.playable.len() + city.distant.len(),
            ),
            (66, 511, 3_093)
        );
        assert_eq!(
            ordered_centres(&village),
            ordered_centres(&town)[..ordered_centres(&village).len()]
        );
        assert_eq!(
            ordered_centres(&town),
            ordered_centres(&city)[..ordered_centres(&town).len()]
        );
        assert!(city.playable.iter().all(|building| {
            building.centre_metres.abs().max_element() <= 50.0 && building.orientation.is_valid()
        }));
        assert!(
            city.distant
                .iter()
                .all(|building| building.centre_metres.abs().max_element() > 50.0)
        );
    }

    #[test]
    fn playable_boundary_only_changes_representation() {
        let compact = place_settlement_buildings(&settlement("lübeck", 40_000), 35.0).unwrap();
        let broad = place_settlement_buildings(&settlement("lübeck", 40_000), 90.0).unwrap();
        assert_eq!(ordered_centres(&compact), ordered_centres(&broad));
        assert!(compact.playable.len() < broad.playable.len());
    }

    #[test]
    fn missing_estimate_uses_the_shared_population_level_fallback() {
        let settlement = SettlementSceneProfile {
            id: "fallback-city".into(),
            population_level: 4,
            population_estimate: 0,
        };
        let population = settlement.effective_population();
        let buildings = place_settlement_buildings(&settlement, 50.0).unwrap();
        let expected = generate_city(settlement_seed(&settlement.id), population)
            .lots
            .len();
        assert_eq!(buildings.playable.len() + buildings.distant.len(), expected);
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
            landform: None,
            streets: place_settlement_buildings(&settlement("dense", 40_000), 50.0)
                .unwrap()
                .streets,
            yards: place_settlement_buildings(&settlement("dense", 40_000), 50.0)
                .unwrap()
                .yards,
            buildings: place_settlement_buildings(&settlement("dense", 40_000), 50.0)
                .unwrap()
                .playable,
            distant_buildings: Vec::new(),
            vista: VistaSample::default(),
            weather: adventuresim_core::weather::weather_at(42, 1, 53_500_000, 10_000_000, 0),
        };
        input.validate().unwrap();
        let generated = input.generate().unwrap();
        assert_eq!(generated.buildings.len(), input.buildings.len());
        assert!(!generated.buildings.is_empty());
        input.buildings.reverse();
        assert!(input.generate().is_ok());
    }

    #[test]
    fn invalid_initial_recipe_seed_advances_deterministically_to_a_valid_program() {
        let buildings = place_settlement_buildings(&settlement("massive-city-3229", 100_000), 50.0)
            .expect("the deterministic retry sequence should find valid recipes");

        assert_eq!(
            buildings.playable.len() + buildings.distant.len(),
            generate_city(settlement_seed("massive-city-3229"), 100_000)
                .lots
                .len()
        );
        assert!(
            buildings
                .playable
                .iter()
                .all(|building| generate(&building.program).is_ok())
        );
        assert_eq!(
            buildings,
            place_settlement_buildings(&settlement("massive-city-3229", 100_000), 50.0).unwrap()
        );
    }

    #[test]
    fn city_house_class_dimensions_match_generated_programmes() {
        for house_class in CityHouseClass::ALL {
            let program = BuildingProgram::fixture(building_archetype(house_class), 42);
            let (width_cells, depth_cells) = program.footprint.dimensions();
            assert_eq!(
                bevy::math::Vec2::new(
                    f32::from(width_cells) * adventuresim_building_generator::CELL_SIZE_METRES,
                    f32::from(depth_cells) * adventuresim_building_generator::CELL_SIZE_METRES,
                ),
                bevy::math::Vec2::new(
                    house_class.frontage_width_metres(),
                    house_class.depth_metres(),
                )
            );
        }
    }
}
