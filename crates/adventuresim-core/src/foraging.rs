//! Pure, deterministic rules for personal current-vicinity foraging.
//!
//! Resources are intentionally year-round until a strategic season model is
//! authoritative. Displayed market prices never participate in discovery.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MIN_FORAGE_MINUTES: u64 = 60;
pub const MAX_FORAGE_MINUTES: u64 = 24 * 60;
pub const ILLEGAL_FORAGE_VIRTUE_LOSS: f32 = 1.0;
pub const CULTIVATED_STEALTH_DC_MILLIRANK: u16 = 1_750;
pub const SETTLEMENT_STEALTH_DC_MILLIRANK: u16 = 2_500;
pub const DURATION_STEALTH_DC_PER_HOUR: u16 = 75;
pub const MAX_TARGETS: usize = 8;

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
    },
    ForageResource {
        item_id: "root_vegetables",
        name: "Wild roots",
        rarity: ForageRarity::Common,
        biomes: &[Plains, Forest, Hills, RiverWetGround],
        yield_min: 1,
        yield_max: 3,
    },
    ForageResource {
        item_id: "hazelnuts",
        name: "Hazelnuts",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, Hills],
        yield_min: 1,
        yield_max: 3,
    },
    ForageResource {
        item_id: "wild_mushrooms",
        name: "Wild mushrooms",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 3,
    },
    ForageResource {
        item_id: "garlic",
        name: "Wild garlic",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 2,
    },
    ForageResource {
        item_id: "sage",
        name: "Sage",
        rarity: ForageRarity::Rare,
        biomes: &[Plains, Hills],
        yield_min: 1,
        yield_max: 2,
    },
    ForageResource {
        item_id: "willow_bark",
        name: "Willow bark",
        rarity: ForageRarity::Uncommon,
        biomes: &[Forest, RiverWetGround],
        yield_min: 1,
        yield_max: 2,
    },
    ForageResource {
        item_id: "poppy",
        name: "Poppy",
        rarity: ForageRarity::Uncommon,
        biomes: &[Plains, Hills],
        yield_min: 1,
        yield_max: 2,
    },
    ForageResource {
        item_id: "comfrey",
        name: "Comfrey",
        rarity: ForageRarity::Uncommon,
        biomes: &[Plains, RiverWetGround],
        yield_min: 1,
        yield_max: 2,
    },
    ForageResource {
        item_id: "watercress",
        name: "Watercress",
        rarity: ForageRarity::Common,
        biomes: &[RiverWetGround],
        yield_min: 1,
        yield_max: 4,
    },
    ForageResource {
        item_id: "seaweed",
        name: "Seaweed",
        rarity: ForageRarity::Common,
        biomes: &[SeaCoast],
        yield_min: 1,
        yield_max: 4,
    },
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalTerrainMixture {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
}

impl LocalTerrainMixture {
    pub const TOTAL: u16 = 1_000;
    pub const fn is_normalized(self) -> bool {
        self.plains <= Self::TOTAL
            && self.forest <= Self::TOTAL
            && self.hills <= Self::TOTAL
            && self.plains + self.forest + self.hills == Self::TOTAL
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForageEnvironment {
    pub terrain: LocalTerrainMixture,
    pub river_or_wet_ground: bool,
    pub sea_or_coast: bool,
    pub cultivated: bool,
    pub settlement: bool,
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

pub fn available(resource: &ForageResource, environment: ForageEnvironment) -> bool {
    resource.biomes.iter().any(|biome| match biome {
        Plains => environment.terrain.plains > 0,
        Forest => environment.terrain.forest > 0,
        Hills => environment.terrain.hills > 0,
        RiverWetGround => environment.river_or_wet_ground,
        SeaCoast => environment.sea_or_coast,
    })
}

pub fn stealth_dc_millirank(environment: ForageEnvironment, minutes: u64) -> Option<u16> {
    if !environment.cultivated && !environment.settlement {
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

/// Resolve one shared search budget. Splitting it over N targets divides each
/// target's attempts by N rather than manufacturing N complete searches.
/// Terrain checks contribute at most +50% discovery and +50% yield.
pub fn resolve(
    seed: u64,
    environment: ForageEnvironment,
    target_ids: &[String],
    minutes: u64,
    terrain_check_millirank: u16,
    stealth_check_millirank: u16,
) -> Result<ForageResolution, &'static str> {
    validate_duration(minutes)?;
    if !environment.terrain.is_normalized() {
        return Err("Foraging terrain mixture is not normalized");
    }
    if target_ids.is_empty() || target_ids.len() > MAX_TARGETS {
        return Err("Choose between one and eight forage targets");
    }
    let mut unique = std::collections::BTreeSet::new();
    for id in target_ids {
        if !unique.insert(id.as_str()) {
            return Err("Forage targets must be unique");
        }
    }
    let mut targets = Vec::with_capacity(unique.len());
    for id in unique {
        let target = resource(id).ok_or("Unknown forage target")?;
        if !available(target, environment) {
            return Err("A forage target is unavailable in this vicinity");
        }
        targets.push(target);
    }
    let skill_bonus_permille = 1_000 + u32::from(terrain_check_millirank.min(5_000)) / 10;
    let target_count = targets.len() as u64;
    let mut yields = Vec::new();
    for (index, target) in targets.into_iter().enumerate() {
        let expected_permille = u64::from(target.rarity.discoveries_per_eight_hours_permille())
            * minutes
            * u64::from(skill_bonus_permille)
            / (8 * 60 * 1_000 * target_count);
        let guaranteed = expected_permille / 1_000;
        let remainder = expected_permille % 1_000;
        let discovered =
            guaranteed + u64::from(random_below(seed, index as u64, 0, 1_000) < remainder);
        if discovered == 0 {
            continue;
        }
        let range = u64::from(target.yield_max - target.yield_min + 1);
        let base_yield = u64::from(target.yield_min) + random_below(seed, index as u64, 1, range);
        let yield_bonus_permille = 1_000 + u64::from(terrain_check_millirank.min(5_000)) / 10;
        let quantity = discovered
            .saturating_mul(base_yield)
            .saturating_mul(yield_bonus_permille)
            / 1_000;
        yields.push(ForageYield {
            item_id: target.item_id,
            quantity: u16::try_from(quantity.max(1)).unwrap_or(u16::MAX),
        });
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
pub fn training_hours(mixture: LocalTerrainMixture, elapsed_minutes: u64) -> [f32; 3] {
    if !mixture.is_normalized() {
        return [0.0; 3];
    }
    let hours = elapsed_minutes as f32 / 60.0;
    [
        hours * f32::from(mixture.plains) / 1_000.0,
        hours * f32::from(mixture.forest) / 1_000.0,
        hours * f32::from(mixture.hills) / 1_000.0,
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
            },
            ..Default::default()
        }
    }

    #[test]
    fn shared_budget_and_resolution_are_deterministic() {
        let one = resolve(7, legal(), &["wild_berries".into()], 8 * 60, 2_000, 0).unwrap();
        let again = resolve(7, legal(), &["wild_berries".into()], 8 * 60, 2_000, 0).unwrap();
        let split = resolve(
            7,
            legal(),
            &["wild_berries".into(), "sage".into()],
            8 * 60,
            2_000,
            0,
        )
        .unwrap();
        assert_eq!(one, again);
        assert!(
            split
                .yields
                .iter()
                .map(|row| u32::from(row.quantity))
                .sum::<u32>()
                <= one
                    .yields
                    .iter()
                    .map(|row| u32::from(row.quantity))
                    .sum::<u32>()
                    + 4
        );
    }

    #[test]
    fn target_order_cannot_change_resolution() {
        let first = resolve(
            91,
            legal(),
            &["sage".into(), "wild_berries".into()],
            8 * 60,
            2_000,
            0,
        )
        .unwrap();
        let reversed = resolve(
            91,
            legal(),
            &["wild_berries".into(), "sage".into()],
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
}
