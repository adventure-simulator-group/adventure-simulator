//! Pure, deterministic rules for personal current-vicinity foraging.
//!
//! Resources are intentionally year-round until a strategic season model is
//! authoritative. Displayed market prices never participate in discovery.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MIN_FORAGE_MINUTES: u64 = 60;
pub const MAX_FORAGE_MINUTES: u64 = 24 * 60;
pub const ILLEGAL_FORAGE_INFAMY: f32 = 1.0;
pub const CULTIVATED_STEALTH_DC_MILLIRANK: u16 = 1_750;
pub const SETTLEMENT_STEALTH_DC_MILLIRANK: u16 = 2_500;
pub const DURATION_STEALTH_DC_PER_HOUR: u16 = 75;
pub const MAX_SOURCES: usize = 5;
/// Food searches are calibrated so an eight-hour low-skill search in an ideal
/// habitat can approximately replace that interval's metabolic expenditure.
pub const FOOD_DISCOVERY_RATE_PERMILLE: u64 = 1_750;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForageSeasonalityPolicy {
    YearRoundUntilStrategicSeasons,
}

pub const SEASONALITY_POLICY: ForageSeasonalityPolicy =
    ForageSeasonalityPolicy::YearRoundUntilStrategicSeasons;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForageBiome {
    Plains,
    Forest,
    Hills,
    RiverWetGround,
    SeaCoast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForageRarity {
    Common,
    Uncommon,
    Rare,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForageSource {
    HighGame,
    LowGame,
    Fish,
    HarmfulBeasts,
    Plants,
}

impl ForageSource {
    pub const ALL: [Self; 5] = [
        Self::HighGame,
        Self::LowGame,
        Self::Fish,
        Self::HarmfulBeasts,
        Self::Plants,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::HighGame => "high_game",
            Self::LowGame => "low_game",
            Self::Fish => "fish",
            Self::HarmfulBeasts => "harmful_beasts",
            Self::Plants => "plants",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::HighGame => "High Game",
            Self::LowGame => "Low Game",
            Self::Fish => "Fish",
            Self::HarmfulBeasts => "Harmful Beasts",
            Self::Plants => "Plants",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::HighGame => "Deer and other prized large quarry",
            Self::LowGame => "Fowl and other lesser quarry",
            Self::Fish => "River, wet-ground, and coastal fish",
            Self::HarmfulBeasts => "Predators and vermin; no license required",
            Self::Plants => "Berries, roots, nuts, fungi, and herbs",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }

    pub const fn requires_license(self) -> bool {
        !matches!(self, Self::HarmfulBeasts)
    }
}

impl ForageRarity {
    const fn discoveries_per_eight_hours_permille(self) -> u32 {
        match self {
            Self::Common => 2_300,
            Self::Uncommon => 900,
            Self::Rare => 300,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForageResource {
    pub item_id: &'static str,
    pub name: &'static str,
    pub rarity: ForageRarity,
    pub biomes: &'static [ForageBiome],
    pub yield_min: u16,
    pub yield_max: u16,
    pub source: ForageSource,
}

use ForageBiome::{Forest, Hills, Plains, RiverWetGround, SeaCoast};

pub const FORAGE_RESOURCES: &[ForageResource] = &[
    ForageResource {
        item_id: "wild_berries",
        name: "Wild berries",
        rarity: ForageRarity::Common,
        biomes: &[Plains, Forest, Hills],
        yield_min: 1,
        yield_max: 4,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "root_vegetables",
        name: "Wild roots",
        rarity: ForageRarity::Common,
        biomes: &[Plains, Forest, Hills, RiverWetGround],
        yield_min: 1,
        yield_max: 3,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "hazelnuts",
        name: "Hazelnuts",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, Hills],
        yield_min: 1,
        yield_max: 3,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "wild_mushrooms",
        name: "Wild mushrooms",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 3,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "garlic",
        name: "Wild garlic",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 2,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "sage",
        name: "Sage",
        rarity: ForageRarity::Rare,
        biomes: &[Plains, Hills],
        yield_min: 1,
        yield_max: 2,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "willow_bark",
        name: "Willow bark",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 2,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "poppy",
        name: "Poppy",
        rarity: ForageRarity::Uncommon,
        biomes: &[Plains, Hills],
        yield_min: 1,
        yield_max: 2,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "comfrey",
        name: "Comfrey",
        rarity: ForageRarity::Uncommon,
        biomes: &[Plains, RiverWetGround],
        yield_min: 1,
        yield_max: 2,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "watercress",
        name: "Watercress",
        rarity: ForageRarity::Common,
        biomes: &[RiverWetGround],
        yield_min: 1,
        yield_max: 4,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "seaweed",
        name: "Seaweed",
        rarity: ForageRarity::Common,
        biomes: &[SeaCoast],
        yield_min: 1,
        yield_max: 4,
        source: ForageSource::Plants,
    },
    ForageResource {
        item_id: "raw_venison",
        name: "Raw venison",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, Hills, Plains],
        yield_min: 2,
        yield_max: 6,
        source: ForageSource::HighGame,
    },
    ForageResource {
        item_id: "raw_fowl",
        name: "Raw fowl",
        rarity: ForageRarity::Common,
        biomes: &[Plains, Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 3,
        source: ForageSource::LowGame,
    },
    ForageResource {
        item_id: "raw_fish",
        name: "Raw fish",
        rarity: ForageRarity::Common,
        biomes: &[RiverWetGround, SeaCoast],
        yield_min: 1,
        yield_max: 4,
        source: ForageSource::Fish,
    },
    ForageResource {
        item_id: "raw_beast_meat",
        name: "Raw beast meat",
        rarity: ForageRarity::Uncommon,
        biomes: &[Plains, Forest, Hills],
        yield_min: 1,
        yield_max: 3,
        source: ForageSource::HarmfulBeasts,
    },
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalTerrainMixture {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub wetlands: u16,
}

impl LocalTerrainMixture {
    pub const TOTAL: u16 = 1_000;
    pub const fn is_normalized(self) -> bool {
        self.plains <= Self::TOTAL
            && self.forest <= Self::TOTAL
            && self.hills <= Self::TOTAL
            && self.wetlands <= Self::TOTAL
            && self.plains + self.forest + self.hills + self.wetlands == Self::TOTAL
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForageEnvironment {
    pub terrain: LocalTerrainMixture,
    pub river_or_wet_ground: bool,
    pub sea_or_coast: bool,
    pub cultivated: bool,
    pub settlement: bool,
    /// Authoritative license evaluation, never supplied by the browser.
    pub license_violation: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForageYield {
    pub item_id: &'static str,
    pub quantity: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForageResolution {
    pub yields: Vec<ForageYield>,
    pub stealth_dc_millirank: Option<u16>,
    pub stealth_succeeded: Option<bool>,
}

pub fn validate_duration(minutes: u64) -> Result<(), &'static str> {
    if (MIN_FORAGE_MINUTES..=MAX_FORAGE_MINUTES).contains(&minutes) && minutes % 60 == 0 {
        Ok(())
    } else {
        Err("Foraging duration must use whole hours from one to 24 hours")
    }
}

pub fn resource(item_id: &str) -> Option<&'static ForageResource> {
    FORAGE_RESOURCES
        .iter()
        .find(|entry| entry.item_id == item_id)
}

pub fn source_available(source: ForageSource, environment: ForageEnvironment) -> bool {
    FORAGE_RESOURCES
        .iter()
        .any(|resource| resource.source == source && available(resource, environment))
}

fn search_budget_divisor(selected_source_count: u64, available_resource_count: u64) -> u64 {
    selected_source_count.saturating_mul(available_resource_count)
}

pub fn available(resource: &ForageResource, environment: ForageEnvironment) -> bool {
    habitat_share_permille(resource, environment) > 0
}

pub fn habitat_share_permille(resource: &ForageResource, environment: ForageEnvironment) -> u16 {
    resource
        .biomes
        .iter()
        .map(|biome| match biome {
            Plains => environment.terrain.plains,
            Forest => environment.terrain.forest,
            Hills => environment.terrain.hills,
            RiverWetGround if environment.river_or_wet_ground => 1_000,
            SeaCoast if environment.sea_or_coast => 1_000,
            _ => 0,
        })
        .fold(0_u16, u16::saturating_add)
        .min(1_000)
}

pub fn stealth_dc_millirank(environment: ForageEnvironment, minutes: u64) -> Option<u16> {
    if !environment.cultivated && !environment.settlement && !environment.license_violation {
        return None;
    }
    let base = if environment.settlement {
        SETTLEMENT_STEALTH_DC_MILLIRANK
    } else {
        CULTIVATED_STEALTH_DC_MILLIRANK
    };
    let hours_after_first = minutes.saturating_sub(60) / 60;
    Some(
        base.saturating_add(
            u16::try_from(hours_after_first)
                .unwrap_or(u16::MAX)
                .saturating_mul(DURATION_STEALTH_DC_PER_HOUR),
        )
        .min(4_500),
    )
}

/// Resolve one shared search budget. Time is divided first over selected
/// categories, then over the locally available resources in each category.
/// Terrain checks contribute at most +50% discovery and +50% yield.
pub fn resolve(
    seed: u64,
    environment: ForageEnvironment,
    source_ids: &[String],
    minutes: u64,
    terrain_check_millirank: u16,
    stealth_check_millirank: u16,
) -> Result<ForageResolution, &'static str> {
    validate_duration(minutes)?;
    if !environment.terrain.is_normalized() {
        return Err("Foraging terrain mixture is not normalized");
    }
    if source_ids.is_empty() || source_ids.len() > MAX_SOURCES {
        return Err("Choose between one and five forage sources");
    }
    let mut unique = std::collections::BTreeSet::new();
    for id in source_ids {
        if !unique.insert(id.as_str()) {
            return Err("Forage sources must be unique");
        }
    }
    let mut sources = Vec::with_capacity(unique.len());
    for id in unique {
        let source = ForageSource::from_id(id).ok_or("Unknown forage source")?;
        let resources = FORAGE_RESOURCES
            .iter()
            .filter(|resource| resource.source == source && available(resource, environment))
            .collect::<Vec<_>>();
        if resources.is_empty() {
            return Err("A forage source is unavailable in this vicinity");
        }
        sources.push((source, resources));
    }
    let skill_bonus_permille = 1_000 + u32::from(terrain_check_millirank.min(5_000)) / 10;
    let source_count = sources.len() as u64;
    let mut yields = Vec::new();
    for (source_index, (_, resources)) in sources.into_iter().enumerate() {
        let resource_count = resources.len() as u64;
        for (resource_index, target) in resources.into_iter().enumerate() {
            let habitat = u64::from(habitat_share_permille(target, environment));
            let food_rate = if crate::food::definition(target.item_id).is_some() {
                FOOD_DISCOVERY_RATE_PERMILLE
            } else {
                1_000
            };
            let expected_permille = u64::from(target.rarity.discoveries_per_eight_hours_permille())
                * minutes
                * u64::from(skill_bonus_permille)
                * habitat
                * food_rate
                / (8 * 60
                    * 1_000
                    * 1_000
                    * 1_000
                    * search_budget_divisor(source_count, resource_count));
            let random_key = source_index as u64 * 256 + resource_index as u64;
            let guaranteed = expected_permille / 1_000;
            let remainder = expected_permille % 1_000;
            let discovered =
                guaranteed + u64::from(random_below(seed, random_key, 0, 1_000) < remainder);
            if discovered == 0 {
                continue;
            }
            let range = u64::from(target.yield_max - target.yield_min + 1);
            let base_yield = u64::from(target.yield_min) + random_below(seed, random_key, 1, range);
            let yield_bonus_permille = 1_000 + u64::from(terrain_check_millirank.min(5_000)) / 10;
            let quantity = discovered
                .saturating_mul(base_yield)
                .saturating_mul(yield_bonus_permille)
                / 1_000;
            yields.push(ForageYield {
                item_id: target.item_id,
                quantity: u16::try_from(quantity).unwrap_or(u16::MAX),
            });
        }
    }
    let (dc, stealth_succeeded) =
        resolve_stealth(seed, environment, minutes, stealth_check_millirank);
    Ok(ForageResolution {
        yields,
        stealth_dc_millirank: dc,
        stealth_succeeded,
    })
}

/// Resolve the single completion/exposure check independently from yield.
/// This accepts a partial elapsed duration so interrupted illegal work can
/// still be noticed without granting partial forage yields.
pub fn resolve_stealth(
    seed: u64,
    environment: ForageEnvironment,
    elapsed_minutes: u64,
    stealth_check_millirank: u16,
) -> (Option<u16>, Option<bool>) {
    let dc = stealth_dc_millirank(environment, elapsed_minutes);
    let succeeded = dc.map(|dc| {
        let roll = random_below(seed, 0, 2, 1_001) as u16;
        stealth_check_millirank.saturating_add(roll) >= dc
    });
    (dc, succeeded)
}

fn random_below(seed: u64, target: u64, stream: u64, upper: u64) -> u64 {
    let digest = Sha256::digest(
        [
            seed.to_le_bytes().as_slice(),
            target.to_le_bytes().as_slice(),
            stream.to_le_bytes().as_slice(),
        ]
        .concat(),
    );
    u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes")) % upper.max(1)
}

/// Conserve actual elapsed search time over concrete Terrain leaf skills.
pub fn training_hours(mixture: LocalTerrainMixture, elapsed_minutes: u64) -> [f32; 4] {
    if !mixture.is_normalized() {
        return [0.0; 4];
    }
    let hours = elapsed_minutes as f32 / 60.0;
    [
        hours * f32::from(mixture.plains) / 1_000.0,
        hours * f32::from(mixture.forest) / 1_000.0,
        hours * f32::from(mixture.hills) / 1_000.0,
        hours * f32::from(mixture.wetlands) / 1_000.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legal() -> ForageEnvironment {
        ForageEnvironment {
            terrain: LocalTerrainMixture {
                plains: 500,
                forest: 300,
                hills: 200,
                wetlands: 0,
            },
            ..Default::default()
        }
    }

    #[test]
    fn shared_budget_and_resolution_are_deterministic() {
        let one = resolve(7, legal(), &["plants".into()], 8 * 60, 2_000, 0).unwrap();
        let again = resolve(7, legal(), &["plants".into()], 8 * 60, 2_000, 0).unwrap();
        let split = resolve(
            7,
            legal(),
            &["plants".into(), "low_game".into()],
            8 * 60,
            2_000,
            0,
        )
        .unwrap();
        assert_eq!(one, again);
        assert_eq!(
            split,
            resolve(
                7,
                legal(),
                &["plants".into(), "low_game".into()],
                8 * 60,
                2_000,
                0,
            )
            .unwrap()
        );
    }

    #[test]
    fn category_then_resource_budget_is_exactly_conserved() {
        let plant_count = FORAGE_RESOURCES
            .iter()
            .filter(|resource| {
                resource.source == ForageSource::Plants && available(resource, legal())
            })
            .count() as u64;
        let low_game_count = FORAGE_RESOURCES
            .iter()
            .filter(|resource| {
                resource.source == ForageSource::LowGame && available(resource, legal())
            })
            .count() as u64;
        assert!(plant_count > 1);
        assert_eq!(low_game_count, 1);

        // Use 2 * plant_count as a common exact budget unit. Plants alone
        // receive the whole budget. With Plants + Low Game, each category
        // receives exactly half, irrespective of how many Plant resources
        // exist inside its half.
        let common = 2 * plant_count;
        let plants_alone = plant_count * (common / search_budget_divisor(1, plant_count));
        let split_plants = plant_count * (common / search_budget_divisor(2, plant_count));
        let split_low_game = low_game_count * (common / search_budget_divisor(2, low_game_count));
        assert_eq!(plants_alone, common);
        assert_eq!(split_plants, common / 2);
        assert_eq!(split_low_game, common / 2);
        assert_eq!(split_plants + split_low_game, common);
    }

    #[test]
    fn target_order_cannot_change_resolution() {
        let first = resolve(
            91,
            legal(),
            &["low_game".into(), "plants".into()],
            8 * 60,
            2_000,
            0,
        )
        .unwrap();
        let reversed = resolve(
            91,
            legal(),
            &["plants".into(), "low_game".into()],
            8 * 60,
            2_000,
            0,
        )
        .unwrap();
        assert_eq!(first, reversed);
    }

    #[test]
    fn illegality_uses_one_bounded_duration_scaled_check() {
        let mut environment = legal();
        environment.cultivated = true;
        assert_eq!(stealth_dc_millirank(environment, 60), Some(1_750));
        environment.settlement = true;
        assert_eq!(stealth_dc_millirank(environment, 24 * 60), Some(4_225));
    }

    #[test]
    fn interrupted_illegal_elapsed_time_still_resolves_exposure() {
        let mut environment = legal();
        environment.cultivated = true;
        let (dc, noticed) = resolve_stealth(7, environment, 30, 0);
        assert_eq!(dc, Some(CULTIVATED_STEALTH_DC_MILLIRANK));
        assert_eq!(noticed, Some(false));
    }

    #[test]
    fn training_is_conserved_across_leaf_skills() {
        let gains = training_hours(legal().terrain, 120);
        assert!((gains.iter().sum::<f32>() - 2.0).abs() < 0.0001);
    }

    #[test]
    fn authoritative_wetland_share_trains_wetlands() {
        let gains = training_hours(
            LocalTerrainMixture {
                wetlands: 1_000,
                ..Default::default()
            },
            60,
        );
        assert_eq!(gains, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn habitat_share_scales_mixed_biome_resources() {
        assert_eq!(
            habitat_share_permille(resource("hazelnuts").unwrap(), legal()),
            500
        );
        assert_eq!(
            habitat_share_permille(resource("wild_berries").unwrap(), legal()),
            1_000
        );
    }

    #[test]
    fn skill_is_monotonic_for_category_searches() {
        let forest = ForageEnvironment {
            terrain: LocalTerrainMixture {
                plains: 0,
                forest: 1_000,
                hills: 0,
                wetlands: 0,
            },
            ..Default::default()
        };
        let calories = |check| {
            (0..256_u64)
                .map(|seed| {
                    resolve(seed, forest, &["plants".into()], 8 * 60, check, 0)
                        .unwrap()
                        .yields
                        .iter()
                        .map(|row| f32::from(row.quantity) * 630.0)
                        .sum::<f32>()
                })
                .sum::<f32>()
                / 256.0
        };
        let novice = calories(0);
        let expert = calories(5_000);
        assert!(novice > 0.0);
        assert!(expert > novice);
    }

    #[test]
    fn source_ids_and_order_are_stable_and_resources_are_classified() {
        assert_eq!(
            ForageSource::ALL.map(ForageSource::id),
            ["high_game", "low_game", "fish", "harmful_beasts", "plants"]
        );
        assert_eq!(
            resource("raw_venison").unwrap().source,
            ForageSource::HighGame
        );
        assert_eq!(resource("raw_fowl").unwrap().source, ForageSource::LowGame);
        assert_eq!(resource("raw_fish").unwrap().source, ForageSource::Fish);
        assert_eq!(
            resource("raw_beast_meat").unwrap().source,
            ForageSource::HarmfulBeasts
        );
        assert!(
            FORAGE_RESOURCES
                .iter()
                .filter(|resource| resource.item_id != "raw_venison"
                    && resource.item_id != "raw_fowl"
                    && resource.item_id != "raw_fish"
                    && resource.item_id != "raw_beast_meat")
                .all(|resource| resource.source == ForageSource::Plants)
        );
    }

    #[test]
    fn unavailable_sources_are_rejected_and_harmful_beasts_need_no_license() {
        assert!(!source_available(ForageSource::Fish, legal()));
        assert!(resolve(7, legal(), &["fish".into()], 60, 0, 0).is_err());
        assert!(!ForageSource::HarmfulBeasts.requires_license());
        assert!(ForageSource::Plants.requires_license());
    }

    #[test]
    fn license_violation_uses_cultivated_base_and_combines_into_one_check() {
        let mut environment = legal();
        environment.license_violation = true;
        assert_eq!(
            stealth_dc_millirank(environment, 60),
            Some(CULTIVATED_STEALTH_DC_MILLIRANK)
        );
        environment.cultivated = true;
        environment.settlement = true;
        assert_eq!(
            stealth_dc_millirank(environment, 60),
            Some(SETTLEMENT_STEALTH_DC_MILLIRANK)
        );
    }
}
