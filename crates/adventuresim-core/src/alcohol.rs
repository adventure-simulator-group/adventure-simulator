//! Deterministic fixed-point alcohol rules shared by strategic simulation and UI.

pub const MINUTES_PER_DAY: u64 = 1_440;
pub const EVENING_BOUNDARY_MINUTE: u64 = 18 * 60;
pub const MODEST_ETHANOL_ML: u32 = 15;
pub const HEAVY_ETHANOL_ML: u32 = 45;
pub const ROLLING_WEEK_DAYS: u64 = 7;
pub const LOW_MORALE_THRESHOLD: f32 = -10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemperancePreference {
    Neutral,
    Temperate,
    Drunkard,
}

pub const fn evening_target(preference: TemperancePreference, had_recent_heavy: bool) -> u32 {
    match preference {
        TemperancePreference::Temperate => 0,
        TemperancePreference::Drunkard => HEAVY_ETHANOL_ML,
        TemperancePreference::Neutral if !had_recent_heavy => HEAVY_ETHANOL_ML,
        TemperancePreference::Neutral => MODEST_ETHANOL_ML,
    }
}

pub const fn morale_change(
    preference: TemperancePreference,
    had_recent_heavy: bool,
    target_satisfied: bool,
) -> i8 {
    match preference {
        TemperancePreference::Temperate => 0,
        TemperancePreference::Drunkard if target_satisfied => 5,
        TemperancePreference::Drunkard => -5,
        TemperancePreference::Neutral if !had_recent_heavy && target_satisfied => 3,
        TemperancePreference::Neutral if !had_recent_heavy => -3,
        TemperancePreference::Neutral if target_satisfied => 1,
        TemperancePreference::Neutral => -1,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AlcoholProperties {
    pub serving_ml: u32,
    /// Alcohol by volume in basis points: 500 = 5% ABV.
    pub abv_basis_points: u16,
    /// Useful hydration after alcohol's diuretic cost, in millilitres.
    pub net_hydration_ml: u32,
    /// Zero means unsuitable for wound disinfection.
    pub disinfectant_effectiveness: u16,
    /// Protected from ordinary morale drinking because it is primarily medical.
    pub disinfectant_focused: bool,
    pub potable: bool,
}

pub const fn ethanol_ml(properties: AlcoholProperties) -> u32 {
    properties
        .serving_ml
        .saturating_mul(properties.abv_basis_points as u32)
        / 10_000
}

pub const fn emergency_hydration_ml(properties: AlcoholProperties) -> u32 {
    if properties.potable {
        properties.net_hydration_ml
    } else {
        0
    }
}

pub const fn units_for_target(target_ethanol_ml: u32, per_unit_ethanol_ml: u32) -> u32 {
    if target_ethanol_ml == 0 || per_unit_ethanol_ml == 0 {
        0
    } else {
        target_ethanol_ml.div_ceil(per_unit_ethanol_ml)
    }
}

/// Stable absolute identity for the evening whose boundary occurs at `minute`.
pub const fn evening_id(minute: u64) -> u64 {
    minute.saturating_sub(EVENING_BOUNDARY_MINUTE) / MINUTES_PER_DAY
}

/// Every evening boundary in `(start, end]`, independent of caller chunking.
pub fn crossed_evenings(start: u64, end: u64) -> impl Iterator<Item = u64> {
    let first_boundary = if start < EVENING_BOUNDARY_MINUTE {
        EVENING_BOUNDARY_MINUTE
    } else {
        EVENING_BOUNDARY_MINUTE
            + (start - EVENING_BOUNDARY_MINUTE)
                .div_euclid(MINUTES_PER_DAY)
                .saturating_add(1)
                * MINUTES_PER_DAY
    };
    let mut ids = Vec::new();
    let mut boundary = first_boundary;
    while boundary <= end {
        ids.push((boundary - EVENING_BOUNDARY_MINUTE) / MINUTES_PER_DAY);
        boundary = boundary.saturating_add(MINUTES_PER_DAY);
        if boundary == u64::MAX {
            break;
        }
    }
    ids.into_iter()
}

pub const fn qualifying_heavy(ethanol_ml: u32) -> bool {
    ethanol_ml >= HEAVY_ETHANOL_ML
}

/// Forecast morale drinking against a stable inventory ordering, then sum the
/// useful hydration left in concrete whole units. Protected medical alcohol is
/// excluded because ordinary planned drinking and hydration will not use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopedAlcoholSupply {
    pub properties: AlcoholProperties,
    pub quantity: u32,
    pub stable_id: u64,
    /// `None` is shared inventory; `Some(id)` is that character's inventory.
    pub owner: Option<u64>,
}

pub fn hydration_after_expected_drinking(
    mut supplies: Vec<ScopedAlcoholSupply>,
    demands: &[(u64, u32)],
) -> u32 {
    supplies.retain(|s| {
        s.properties.potable
            && !s.properties.disinfectant_focused
            && s.quantity > 0
            && ethanol_ml(s.properties) > 0
    });
    supplies.sort_by_key(|s| {
        (
            s.owner.is_some(),
            s.properties.disinfectant_effectiveness,
            s.stable_id,
        )
    });
    for (character_id, requested) in demands {
        let mut remaining = *requested;
        for supply in supplies
            .iter_mut()
            .filter(|s| s.owner.is_none() || s.owner == Some(*character_id))
        {
            let each = ethanol_ml(supply.properties);
            while supply.quantity > 0 && remaining > 0 {
                supply.quantity -= 1;
                remaining = remaining.saturating_sub(each);
            }
        }
    }
    supplies.into_iter().fold(0_u32, |total, supply| {
        total.saturating_add(
            emergency_hydration_ml(supply.properties).saturating_mul(supply.quantity),
        )
    })
}

/// Index of the strongest eligible concrete disinfectant; lower stable IDs
/// win exact ties so previews and reducers agree.
pub fn best_disinfectant(candidates: &[(u16, u64)]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, (effectiveness, _))| *effectiveness > 0)
        .max_by_key(|(_, (effectiveness, stable_id))| {
            (*effectiveness, std::cmp::Reverse(*stable_id))
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_equivalence_uses_pure_ethanol() {
        let beer = AlcoholProperties {
            serving_ml: 500,
            abv_basis_points: 500,
            potable: true,
            ..AlcoholProperties::default()
        };
        let wine = AlcoholProperties {
            serving_ml: 125,
            abv_basis_points: 2_000,
            potable: true,
            ..AlcoholProperties::default()
        };
        assert_eq!(ethanol_ml(beer), 25);
        assert_eq!(ethanol_ml(wine), 25);
        assert_eq!(units_for_target(HEAVY_ETHANOL_ML, ethanol_ml(beer)), 2);
    }

    #[test]
    fn evening_enumeration_is_chunk_invariant() {
        let whole: Vec<_> = crossed_evenings(0, 4_000).collect();
        let split: Vec<_> = crossed_evenings(0, 2_000)
            .chain(crossed_evenings(2_000, 4_000))
            .collect();
        assert_eq!(whole, split);
        assert_eq!(whole, vec![0, 1, 2]);
    }

    #[test]
    fn temperance_controls_evening_target_and_morale() {
        assert_eq!(evening_target(TemperancePreference::Temperate, false), 0);
        assert_eq!(
            morale_change(TemperancePreference::Temperate, false, false),
            0
        );
        assert_eq!(
            evening_target(TemperancePreference::Neutral, false),
            HEAVY_ETHANOL_ML
        );
        assert_eq!(morale_change(TemperancePreference::Neutral, false, true), 3);
        assert_eq!(
            morale_change(TemperancePreference::Neutral, false, false),
            -3
        );
        assert_eq!(
            evening_target(TemperancePreference::Neutral, true),
            MODEST_ETHANOL_ML
        );
        assert_eq!(morale_change(TemperancePreference::Neutral, true, true), 1);
        assert_eq!(
            morale_change(TemperancePreference::Neutral, true, false),
            -1
        );
        assert_eq!(
            evening_target(TemperancePreference::Drunkard, true),
            HEAVY_ETHANOL_ML
        );
        assert_eq!(morale_change(TemperancePreference::Drunkard, true, true), 5);
        assert_eq!(
            morale_change(TemperancePreference::Drunkard, true, false),
            -5
        );
    }

    #[test]
    fn non_potable_never_hydrates() {
        assert_eq!(
            emergency_hydration_ml(AlcoholProperties {
                net_hydration_ml: 400,
                potable: false,
                ..AlcoholProperties::default()
            }),
            0
        );
    }

    #[test]
    fn forecast_subtracts_whole_morale_servings_before_hydration() {
        let beer = AlcoholProperties {
            serving_ml: 500,
            abv_basis_points: 500,
            net_hydration_ml: 400,
            potable: true,
            ..AlcoholProperties::default()
        };
        assert_eq!(
            hydration_after_expected_drinking(
                vec![ScopedAlcoholSupply {
                    properties: beer,
                    quantity: 3,
                    stable_id: 1,
                    owner: None,
                }],
                &[(7, 45)],
            ),
            400
        );
    }

    #[test]
    fn forecast_never_spends_another_characters_personal_drink() {
        let beer = AlcoholProperties {
            serving_ml: 500,
            abv_basis_points: 500,
            net_hydration_ml: 400,
            potable: true,
            ..AlcoholProperties::default()
        };
        let supplies = vec![
            ScopedAlcoholSupply {
                properties: beer,
                quantity: 1,
                stable_id: 1,
                owner: Some(1),
            },
            ScopedAlcoholSupply {
                properties: beer,
                quantity: 2,
                stable_id: 2,
                owner: Some(2),
            },
        ];
        assert_eq!(hydration_after_expected_drinking(supplies, &[(1, 45)]), 800);
    }

    #[test]
    fn best_disinfectant_prefers_effectiveness_then_lowest_stable_id() {
        assert_eq!(best_disinfectant(&[(20, 9), (80, 7), (80, 3)]), Some(2));
        assert_eq!(best_disinfectant(&[(0, 1)]), None);
    }
}
