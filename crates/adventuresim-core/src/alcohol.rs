//! Deterministic fixed-point alcohol rules shared by strategic simulation and UI.

pub const MINUTES_PER_DAY: u64 = 1_440;
pub const EVENING_BOUNDARY_MINUTE: u64 = 18 * 60;
pub const MODEST_ETHANOL_ML: u32 = 15;
pub const HEAVY_ETHANOL_ML: u32 = 45;
pub const ROLLING_WEEK_DAYS: u64 = 7;
pub const LOW_MORALE_THRESHOLD: f32 = -10.0;
pub const MAX_ALCOHOL_INTERVAL_MINUTES: u64 = 365 * MINUTES_PER_DAY;
pub const NIGHT_END_MINUTE: u64 = 8 * 60;
pub const ALCOHOL_SURGERY_CONTROL_DIVISOR: f32 = 25.0;
pub const NIGHTLY_MORALE_SOURCE_ID: &str = "alcohol-nightly";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NightlyMoraleEffect {
    pub kind: &'static str,
    pub magnitude: i8,
    pub occurred_at_minute: u64,
}

pub const fn nightly_morale_effect(
    evening: u64,
    preference: TemperancePreference,
    had_recent_heavy: bool,
    target_satisfied: bool,
) -> Option<NightlyMoraleEffect> {
    let magnitude = morale_change(preference, had_recent_heavy, target_satisfied);
    if magnitude == 0 {
        return None;
    }
    let Some(occurred_at_minute) = evening_boundary(evening) else {
        return None;
    };
    Some(NightlyMoraleEffect {
        kind: if magnitude > 0 {
            "alcohol_satisfied"
        } else {
            "alcohol_unsatisfied"
        },
        magnitude,
        occurred_at_minute,
    })
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
    if properties.abv_basis_points > 10_000 {
        return 0;
    }
    properties
        .serving_ml
        .saturating_mul(properties.abv_basis_points as u32)
        / 10_000
}

pub const fn water_content_ml(properties: AlcoholProperties) -> u32 {
    if properties.abv_basis_points > 10_000 {
        return 0;
    }
    properties
        .serving_ml
        .saturating_mul(10_000_u32.saturating_sub(properties.abv_basis_points as u32))
        / 10_000
}

pub const fn properties_valid(properties: AlcoholProperties) -> bool {
    properties.abv_basis_points <= 10_000
        && properties.net_hydration_ml <= water_content_ml(properties)
}

pub const fn emergency_hydration_ml(properties: AlcoholProperties) -> u32 {
    if properties.potable && properties.abv_basis_points <= 10_000 {
        let water = water_content_ml(properties);
        if properties.net_hydration_ml < water {
            properties.net_hydration_ml
        } else {
            water
        }
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

pub const fn consumable_units(total: u32, target: u32, settled: bool) -> u32 {
    if settled {
        total.saturating_sub(target)
    } else {
        total
    }
}

pub fn tavern_units_affordable(
    remaining_ethanol_ml: u32,
    ethanol_per_unit_ml: u32,
    price_per_unit: u64,
    personal_coin: u64,
) -> u32 {
    if remaining_ethanol_ml == 0 || ethanol_per_unit_ml == 0 || price_per_unit == 0 {
        return 0;
    }
    let required = units_for_target(remaining_ethanol_ml, ethanol_per_unit_ml);
    let affordable = personal_coin / price_per_unit;
    required.min(if affordable > u64::from(u32::MAX) {
        u32::MAX
    } else {
        affordable as u32
    })
}

/// Stable absolute identity for the evening whose boundary occurs at `minute`.
pub const fn evening_id(minute: u64) -> u64 {
    minute.saturating_sub(EVENING_BOUNDARY_MINUTE) / MINUTES_PER_DAY
}

pub const fn evening_boundary(evening: u64) -> Option<u64> {
    match evening.checked_mul(MINUTES_PER_DAY) {
        Some(days) => EVENING_BOUNDARY_MINUTE.checked_add(days),
        None => None,
    }
}

pub const fn next_evening_boundary_after(minute: u64) -> Option<u64> {
    let next_id = if minute < EVENING_BOUNDARY_MINUTE {
        0
    } else {
        match evening_id(minute).checked_add(1) {
            Some(id) => id,
            None => return None,
        }
    };
    evening_boundary(next_id)
}

#[derive(Clone, Debug)]
pub struct EveningIds {
    next: u64,
    last: Option<u64>,
}

impl Iterator for EveningIds {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        let last = self.last?;
        if self.next > last {
            self.last = None;
            return None;
        }
        let value = self.next;
        self.next = self.next.saturating_add(1);
        Some(value)
    }
}

fn checked_interval(start: u64, end: u64) -> Result<(), &'static str> {
    let elapsed = end
        .checked_sub(start)
        .ok_or("Alcohol interval ends before it starts")?;
    if elapsed > MAX_ALCOHOL_INTERVAL_MINUTES {
        return Err("Alcohol interval cannot exceed one year");
    }
    Ok(())
}

/// Every evening boundary in `(start, end]`, lazily and with an independent
/// one-year work bound.
pub fn crossed_evenings(start: u64, end: u64) -> Result<EveningIds, &'static str> {
    checked_interval(start, end)?;
    let first = if start < EVENING_BOUNDARY_MINUTE {
        0
    } else {
        evening_id(start)
            .checked_add(1)
            .ok_or("Evening identity overflow")?
    };
    let last = (end >= EVENING_BOUNDARY_MINUTE).then(|| evening_id(end));
    Ok(EveningIds { next: first, last })
}

#[derive(Clone, Debug)]
pub struct RestEvenings {
    next: u64,
    last: u64,
    start: u64,
    end: u64,
}

impl Iterator for RestEvenings {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next <= self.last {
            let evening = self.next;
            self.next = self.next.saturating_add(1);
            let boundary = evening_boundary(evening)?;
            let sleep_end = boundary
                .checked_add(MINUTES_PER_DAY - EVENING_BOUNDARY_MINUTE + NIGHT_END_MINUTE)?;
            if self.start < sleep_end && self.end >= boundary {
                return Some(evening);
            }
        }
        None
    }
}

/// Nightly opportunities overlapped by a rest interval. This includes the
/// current unprocessed evening for rests beginning after 18:00 or before 08:00.
pub fn rest_evenings(start: u64, end: u64) -> Result<RestEvenings, &'static str> {
    checked_interval(start, end)?;
    let first = (start / MINUTES_PER_DAY).saturating_sub(1);
    let last = end / MINUTES_PER_DAY;
    Ok(RestEvenings {
        next: first,
        last,
        start,
        end,
    })
}

#[derive(Clone, Debug)]
pub struct TravelEveningSegments {
    cursor: u64,
    end: u64,
}

impl Iterator for TravelEveningSegments {
    type Item = (u64, u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end {
            return None;
        }
        let start = self.cursor;
        let end =
            next_evening_boundary_after(start).map_or(self.end, |boundary| boundary.min(self.end));
        self.cursor = end;
        Some((start, end, end.saturating_sub(1)))
    }
}

/// Travel need intervals split at evening boundaries so emergency servings
/// are attributed to the same nightly history independent of reducer chunking.
pub fn travel_evening_segments(
    start: u64,
    end: u64,
) -> Result<TravelEveningSegments, &'static str> {
    checked_interval(start, end)?;
    Ok(TravelEveningSegments { cursor: start, end })
}

pub const fn qualifying_heavy(ethanol_ml: u32) -> bool {
    ethanol_ml >= HEAVY_ETHANOL_ML
}

/// Forecast morale drinking against a stable inventory ordering, then sum the
/// useful hydration left in concrete whole units. Protected medical alcohol is
/// excluded because ordinary planned drinking and hydration will not use it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedAlcoholSupply {
    pub properties: AlcoholProperties,
    pub quantity: u32,
    pub item_id: String,
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
    supplies.sort_by(|a, b| {
        (
            a.owner.is_some(),
            a.properties.disinfectant_effectiveness,
            &a.item_id,
            a.stable_id,
        )
            .cmp(&(
                b.owner.is_some(),
                b.properties.disinfectant_effectiveness,
                &b.item_id,
                b.stable_id,
            ))
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

pub const fn surgery_control_bonus(soap_used: bool, alcohol_effectiveness: Option<u16>) -> f32 {
    let soap = if soap_used { 2.0 } else { 0.0 };
    let alcohol = match alcohol_effectiveness {
        Some(effectiveness) => effectiveness as f32 / ALCOHOL_SURGERY_CONTROL_DIVISOR,
        None => 0.0,
    };
    soap + alcohol
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
        let whole: Vec<_> = crossed_evenings(0, 4_000).unwrap().collect();
        let split: Vec<_> = crossed_evenings(0, 2_000)
            .unwrap()
            .chain(crossed_evenings(2_000, 4_000).unwrap())
            .collect();
        assert_eq!(whole, split);
        assert_eq!(whole, vec![0, 1, 2]);
    }

    #[test]
    fn rest_after_evening_boundary_includes_the_current_night() {
        assert_eq!(
            rest_evenings(19 * 60, MINUTES_PER_DAY + 7 * 60)
                .unwrap()
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            rest_evenings(MINUTES_PER_DAY + 2 * 60, MINUTES_PER_DAY + 8 * 60)
                .unwrap()
                .collect::<Vec<_>>(),
            vec![0]
        );
    }

    #[test]
    fn nightly_rest_enumeration_is_chunk_idempotent() {
        let whole: Vec<_> = rest_evenings(19 * 60, 3 * MINUTES_PER_DAY + 7 * 60)
            .unwrap()
            .collect();
        let mut split: Vec<_> = rest_evenings(19 * 60, MINUTES_PER_DAY + 7 * 60)
            .unwrap()
            .chain(rest_evenings(MINUTES_PER_DAY + 7 * 60, 3 * MINUTES_PER_DAY + 7 * 60).unwrap())
            .collect();
        split.dedup();
        assert_eq!(whole, split);
        assert_eq!(whole, vec![0, 1, 2]);
    }

    #[test]
    fn evening_iterators_reject_reversed_or_unbounded_work() {
        assert!(crossed_evenings(2, 1).is_err());
        assert!(rest_evenings(0, MAX_ALCOHOL_INTERVAL_MINUTES + 1).is_err());
        assert_eq!(
            crossed_evenings(0, MAX_ALCOHOL_INTERVAL_MINUTES)
                .unwrap()
                .count(),
            365
        );
    }

    #[test]
    fn long_rest_and_daily_chunks_choose_the_same_latest_absolute_event() {
        fn latest(intervals: &[(u64, u64)]) -> Option<(u64, i8)> {
            let mut evaluated = std::collections::BTreeSet::new();
            let mut result = None;
            for (start, end) in intervals {
                for evening in rest_evenings(*start, *end).unwrap() {
                    if evaluated.insert(evening) {
                        result = Some((
                            evening_boundary(evening).unwrap(),
                            morale_change(TemperancePreference::Drunkard, true, false),
                        ));
                    }
                }
            }
            result
        }
        let start = 19 * 60;
        let end = start + 30 * MINUTES_PER_DAY;
        let daily: Vec<_> = (0..30)
            .map(|day| {
                (
                    start + day * MINUTES_PER_DAY,
                    start + (day + 1) * MINUTES_PER_DAY,
                )
            })
            .collect();
        assert_eq!(latest(&[(start, end)]), latest(&daily));
        assert_eq!(
            latest(&[(start, end)]).unwrap().0,
            evening_boundary(30).unwrap()
        );
    }

    #[test]
    fn nightly_morale_is_one_refreshable_nonzero_source() {
        assert_eq!(NIGHTLY_MORALE_SOURCE_ID, "alcohol-nightly");
        assert_eq!(
            nightly_morale_effect(0, TemperancePreference::Temperate, false, false),
            None
        );
        let missed = nightly_morale_effect(2, TemperancePreference::Drunkard, true, false).unwrap();
        let satisfied =
            nightly_morale_effect(3, TemperancePreference::Drunkard, true, true).unwrap();
        // Upserting by the stable source leaves only the latest value.
        let mut sources = std::collections::BTreeMap::new();
        sources.insert(NIGHTLY_MORALE_SOURCE_ID, missed);
        sources.insert(NIGHTLY_MORALE_SOURCE_ID, satisfied);
        assert_eq!(sources.len(), 1);
        let source = sources[NIGHTLY_MORALE_SOURCE_ID];
        assert_eq!(source.kind, "alcohol_satisfied");
        assert_eq!(source.magnitude, 5);
        assert_eq!(source.occurred_at_minute, evening_boundary(3).unwrap());
    }

    #[test]
    fn emergency_travel_segments_preserve_evening_history_when_chunked() {
        let start = 17 * 60;
        let end = start + 3 * MINUTES_PER_DAY;
        let mut whole: Vec<_> = travel_evening_segments(start, end)
            .unwrap()
            .map(|(_, _, minute)| evening_id(minute))
            .collect();
        whole.dedup();
        assert_eq!(whole, vec![0, 1, 2]);
        let mut daily = Vec::new();
        for day in 0..3 {
            daily.extend(
                travel_evening_segments(
                    start + day * MINUTES_PER_DAY,
                    start + (day + 1) * MINUTES_PER_DAY,
                )
                .unwrap()
                .map(|(_, _, minute)| evening_id(minute)),
            );
        }
        daily.dedup();
        assert_eq!(whole, daily);
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
    fn hydration_is_clamped_to_physical_water_content_and_rejects_bad_abv() {
        let impossible = AlcoholProperties {
            serving_ml: 100,
            abv_basis_points: 5_000,
            net_hydration_ml: 90,
            potable: true,
            ..AlcoholProperties::default()
        };
        assert!(!properties_valid(impossible));
        assert_eq!(water_content_ml(impossible), 50);
        assert_eq!(emergency_hydration_ml(impossible), 50);
        let invalid_abv = AlcoholProperties {
            abv_basis_points: 10_001,
            ..impossible
        };
        assert!(!properties_valid(invalid_abv));
        assert_eq!(ethanol_ml(invalid_abv), 0);
        assert_eq!(emergency_hydration_ml(invalid_abv), 0);
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
                    item_id: "beer".into(),
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
                item_id: "beer".into(),
                stable_id: 1,
                owner: Some(1),
            },
            ScopedAlcoholSupply {
                properties: beer,
                quantity: 2,
                item_id: "beer".into(),
                stable_id: 2,
                owner: Some(2),
            },
        ];
        assert_eq!(hydration_after_expected_drinking(supplies, &[(1, 45)]), 800);
    }

    #[test]
    fn forecast_rounds_each_evening_and_uses_runtime_member_order() {
        let wine = AlcoholProperties {
            serving_ml: 250,
            abv_basis_points: 1_200,
            net_hydration_ml: 150,
            potable: true,
            ..AlcoholProperties::default()
        };
        let supplies = vec![ScopedAlcoholSupply {
            properties: wine,
            quantity: 3,
            item_id: "table_wine".into(),
            stable_id: 1,
            owner: None,
        }];
        // Two 15 ml evenings each consume a concrete 30 ml serving. Aggregating
        // them into one 30 ml request would incorrectly leave one extra unit.
        assert_eq!(
            hydration_after_expected_drinking(supplies.clone(), &[(1, 15), (1, 15)]),
            150
        );
        assert_eq!(hydration_after_expected_drinking(supplies, &[(1, 30)]), 300);
    }

    #[test]
    fn settlement_reserves_and_tavern_purchase_are_whole_unit_and_coin_bounded() {
        assert_eq!(consumable_units(5, 3, true), 2);
        assert_eq!(consumable_units(2, 3, true), 0);
        assert_eq!(consumable_units(2, 3, false), 2);
        assert_eq!(tavern_units_affordable(45, 30, 2, 3), 1);
        assert_eq!(tavern_units_affordable(45, 30, 2, 4), 2);
        assert_eq!(tavern_units_affordable(45, 0, 2, 100), 0);
    }

    #[test]
    fn best_disinfectant_prefers_effectiveness_then_lowest_stable_id() {
        assert_eq!(best_disinfectant(&[(20, 9), (80, 7), (80, 3)]), Some(2));
        assert_eq!(best_disinfectant(&[(0, 1)]), None);
    }

    #[test]
    fn surgery_alcohol_and_soap_bonuses_stack_and_absence_is_safe() {
        assert_eq!(surgery_control_bonus(false, None), 0.0);
        assert_eq!(surgery_control_bonus(true, None), 2.0);
        assert_eq!(surgery_control_bonus(false, Some(100)), 4.0);
        assert_eq!(surgery_control_bonus(true, Some(100)), 6.0);
    }
}
