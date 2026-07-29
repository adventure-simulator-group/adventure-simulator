//! Bounded, deterministic escalation, normalized combat power, and public
//! public-awareness math for unresolved recurring hostile cases.

use serde::{Deserialize, Serialize};

pub const PUBLIC_THRESHOLD_BPS: u16 = 6_500;
pub const AWARENESS_GROWTH_BPS: u16 = 3_500;
pub const COMBAT_SCALE_BPS: u32 = 10_000;
/// One baseline orc is 10,000 normalized power. No generated hostile group may
/// escalate past thirty baseline-orc equivalents.
pub const BASELINE_ORC_POWER: u32 = 10_000;
pub use crate::threat_escalation_limits::{MAX_ORC_EQUIVALENT_POWER, MIN_BASELINE_ENEMY_POWER};
/// Maximum per-enemy scale needed for the weakest valid single threat to reach
/// the thirty-orc normalized-power ceiling.
pub const MAX_COMBAT_SCALE_BPS: u32 =
    MAX_ORC_EQUIVALENT_POWER * COMBAT_SCALE_BPS / MIN_BASELINE_ENEMY_POWER;
pub const MAX_MOB_COUNT: u32 = 30;
pub const MAX_PUBLIC_THREAT_CANDIDATES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationMode {
    Mob,
    Single,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationProfile {
    pub mode: EscalationMode,
    pub growth_rate_bps: u16,
    /// Normalized power of one unscaled member of this authored threat type.
    pub baseline_enemy_power: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EscalatedCombat {
    pub progress_bps: u16,
    pub enemy_count: u32,
    pub difficulty: i32,
    /// Per-enemy multiplier relative to the immutable base snapshot.
    pub combat_scale_bps: u32,
    /// Comparable total group power in baseline-orc units.
    pub normalized_combat_power: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferralDeliveryAuthorityKind {
    LocalProblem,
    PublicThreat,
    Missing,
    Conflict,
}

pub const fn referral_delivery_authority_kind(
    has_local_problem_receipt: bool,
    has_public_threat_disclosure: bool,
) -> ReferralDeliveryAuthorityKind {
    match (has_local_problem_receipt, has_public_threat_disclosure) {
        (true, false) => ReferralDeliveryAuthorityKind::LocalProblem,
        (false, true) => ReferralDeliveryAuthorityKind::PublicThreat,
        (false, false) => ReferralDeliveryAuthorityKind::Missing,
        (true, true) => ReferralDeliveryAuthorityKind::Conflict,
    }
}

/// One integer asymptotic step. Ceil division guarantees progress while below
/// the cap, including near the integer plateau.
pub fn asymptotic_step(current: u16, cap: u16, rate_bps: u16) -> u16 {
    if current >= cap || rate_bps == 0 {
        return current.min(cap);
    }
    let remaining = u32::from(cap - current);
    let increase = remaining
        .saturating_mul(u32::from(rate_bps.min(10_000)))
        .saturating_add(9_999)
        / 10_000;
    current.saturating_add(increase as u16).min(cap)
}

pub fn progress_for_follow_ups(follow_up_count: u16, growth_rate_bps: u16) -> u16 {
    (0..follow_up_count).fold(0, |progress, _| {
        asymptotic_step(progress, 10_000, growth_rate_bps)
    })
}

pub fn awareness_for_incident(investigability: u8, incident_ordinal: u16) -> u16 {
    let cap = u16::from(investigability.min(100)) * 100;
    (1..incident_ordinal).fold(0, |awareness, _| {
        asymptotic_step(awareness, cap, AWARENESS_GROWTH_BPS)
    })
}

pub fn first_public_incident(investigability: u8) -> Option<u16> {
    let cap = u16::from(investigability.min(100)) * 100;
    let mut ordinal = 1_u16;
    let mut awareness = 0_u16;
    loop {
        if is_public(awareness) {
            return Some(ordinal);
        }
        if awareness >= cap || ordinal == u16::MAX {
            return None;
        }
        awareness = asymptotic_step(awareness, cap, AWARENESS_GROWTH_BPS);
        ordinal += 1;
    }
}

pub fn scheduled_public_since_minute(
    starts_at: u64,
    incident_interval_minutes: u64,
    investigability: u8,
) -> Option<u64> {
    first_public_incident(investigability).map(|ordinal| {
        starts_at.saturating_add(
            u64::from(ordinal.saturating_sub(1)).saturating_mul(incident_interval_minutes),
        )
    })
}

pub const fn is_public(public_awareness_bps: u16) -> bool {
    public_awareness_bps >= PUBLIC_THRESHOLD_BPS
}

fn interpolated_u32(base: u32, cap: u32, progress_bps: u16) -> u32 {
    if base >= cap {
        return base;
    }
    base.saturating_add(
        (u64::from(cap - base) * u64::from(progress_bps) / 10_000).min(u64::from(u32::MAX)) as u32,
    )
}

fn mob_count_cap(base_count: u32, baseline_enemy_power: u32) -> u32 {
    let mut cap = base_count;
    while cap < MAX_MOB_COUNT {
        let candidate = cap + 1;
        // Balance count and stat growth: candidate/base_count must not exceed
        // the remaining per-enemy scale available at the global power ceiling.
        let balanced = u128::from(candidate)
            .saturating_mul(u128::from(candidate))
            .saturating_mul(u128::from(baseline_enemy_power))
            <= u128::from(MAX_ORC_EQUIVALENT_POWER).saturating_mul(u128::from(base_count));
        if !balanced {
            break;
        }
        cap = candidate;
    }
    cap
}

pub fn combat_for_incident(
    initial_count: u32,
    initial_difficulty: i32,
    incident_ordinal: u16,
    profile: EscalationProfile,
) -> EscalatedCombat {
    let base_count = initial_count.clamp(1, MAX_MOB_COUNT);
    let base_difficulty = initial_difficulty.max(1);
    let baseline_enemy_power = profile
        .baseline_enemy_power
        .clamp(MIN_BASELINE_ENEMY_POWER, MAX_ORC_EQUIVALENT_POWER);
    let base_power = u64::from(base_count)
        .saturating_mul(u64::from(baseline_enemy_power))
        .min(u64::from(MAX_ORC_EQUIVALENT_POWER)) as u32;
    let progress = if base_power >= MAX_ORC_EQUIVALENT_POWER {
        10_000
    } else {
        progress_for_follow_ups(incident_ordinal.saturating_sub(1), profile.growth_rate_bps)
    };
    let count_cap = match profile.mode {
        EscalationMode::Mob => mob_count_cap(base_count, baseline_enemy_power),
        EscalationMode::Single => base_count,
    };
    let enemy_count = interpolated_u32(base_count, count_cap, progress).max(base_count);
    let scale_denominator = u64::from(count_cap)
        .saturating_mul(u64::from(baseline_enemy_power))
        .max(1);
    let scale_cap = (u64::from(MAX_ORC_EQUIVALENT_POWER)
        .saturating_mul(u64::from(COMBAT_SCALE_BPS))
        .div_ceil(scale_denominator))
    .clamp(u64::from(COMBAT_SCALE_BPS), u64::from(MAX_COMBAT_SCALE_BPS)) as u32;
    let combat_scale_bps =
        interpolated_u32(COMBAT_SCALE_BPS, scale_cap, progress).max(COMBAT_SCALE_BPS);
    let normalized_combat_power = u64::from(enemy_count)
        .saturating_mul(u64::from(baseline_enemy_power))
        .saturating_mul(u64::from(combat_scale_bps))
        .div_ceil(u64::from(COMBAT_SCALE_BPS))
        .min(u64::from(MAX_ORC_EQUIVALENT_POWER)) as u32;
    let scaled_difficulty = i64::from(base_difficulty).saturating_mul(i64::from(combat_scale_bps));
    let difficulty = (scaled_difficulty.saturating_add(i64::from(COMBAT_SCALE_BPS) - 1)
        / i64::from(COMBAT_SCALE_BPS))
    .clamp(i64::from(base_difficulty), i64::from(i32::MAX)) as i32;
    EscalatedCombat {
        progress_bps: progress,
        enemy_count,
        difficulty,
        combat_scale_bps,
        normalized_combat_power: normalized_combat_power.max(base_power),
    }
}

pub fn approximate_count_band(count: u32) -> &'static str {
    match count {
        0 | 1 => "one",
        2..=4 => "a few (2–4)",
        5..=9 => "several (5–9)",
        10..=19 => "a warband (10–19)",
        _ => "a horde (20+)",
    }
}

/// Shared tactical/autoresolve consumer equation. Physical capability and limb
/// health grow with the square root of per-enemy power; damage-relevant
/// training grows linearly. Callers apply authored base difficulty separately.
pub fn combat_physical_multiplier(combat_scale_bps: u32) -> f32 {
    (combat_scale_bps.clamp(COMBAT_SCALE_BPS, MAX_COMBAT_SCALE_BPS) as f32
        / COMBAT_SCALE_BPS as f32)
        .sqrt()
}

pub fn combat_training_multiplier(combat_scale_bps: u32) -> f32 {
    combat_scale_bps.clamp(COMBAT_SCALE_BPS, MAX_COMBAT_SCALE_BPS) as f32 / COMBAT_SCALE_BPS as f32
}

/// Investigability 50 is neutral. The bounded modifier is used consistently
/// by route, physical-inspection, and lore checks.
pub fn check_modifier_milli(investigability: u8) -> i16 {
    ((i16::from(investigability.min(100)) - 50) * 30).clamp(-1_500, 1_500)
}

pub fn adjusted_difficulty_milli(difficulty: u16, investigability: u8) -> u16 {
    (i32::from(difficulty) - i32::from(check_modifier_milli(investigability))).clamp(100, 10_000)
        as u16
}

/// Bounded hearing allowance in 25 km road-distance units. Local and adjacent
/// settlements are handled separately as a minimum.
pub fn hearing_radius(
    population: u32,
    normalized_combat_power: u32,
    public_awareness_bps: u16,
) -> u32 {
    if !is_public(public_awareness_bps) {
        return 0;
    }
    let population_factor = if population == 0 {
        0
    } else {
        population.min(100_000).ilog2().saturating_sub(7).min(9)
    };
    let danger_factor = normalized_combat_power.min(MAX_ORC_EQUIVALENT_POWER) / 50_000;
    let post_threshold = u32::from(public_awareness_bps.saturating_sub(PUBLIC_THRESHOLD_BPS)) / 700;
    1u32.saturating_add(population_factor)
        .saturating_add(danger_factor)
        .saturating_add(post_threshold)
        .min(18)
}

pub fn public_referral_source(
    local_source: bool,
    innkeeper_at_inn: bool,
    exact_capable_representative: bool,
    current_member: bool,
) -> Option<&'static str> {
    if !local_source {
        return None;
    }
    if innkeeper_at_inn {
        return Some("innkeeper");
    }
    (exact_capable_representative && current_member).then_some("organization")
}

pub fn hearing_allows(
    same_settlement: bool,
    adjacent_settlement: bool,
    road_distance_m: Option<u64>,
    population: u32,
    normalized_combat_power: u32,
    public_awareness_bps: u16,
) -> bool {
    same_settlement
        || adjacent_settlement
        || road_distance_m.is_some_and(|distance| {
            distance
                <= u64::from(hearing_radius(
                    population,
                    normalized_combat_power,
                    public_awareness_bps,
                ))
                .saturating_mul(25_000)
        })
}

pub fn public_threat_summary(threat_name: &str, site_label: &str, count_band: &str) -> String {
    format!("{threat_name} at {site_label}; reported number: {count_band}.")
}

pub fn public_threat_journal_id(owner_character_id: u64, public_case_id: &str) -> String {
    format!("public-threat-journal:{owner_character_id}:{public_case_id}")
}

/// Select the oldest public cases first, breaking ties by stable settlement and
/// problem identity so a crowded world cannot make referral work unbounded or
/// starve an older case nondeterministically.
pub fn bounded_public_threat_candidates<T>(
    mut candidates: Vec<(u64, String, String, T)>,
) -> Vec<T> {
    candidates
        .sort_by(|left, right| (&left.0, &left.1, &left.2).cmp(&(&right.0, &right.1, &right.2)));
    candidates.truncate(MAX_PUBLIC_THREAT_CANDIDATES);
    candidates
        .into_iter()
        .map(|(_, _, _, candidate)| candidate)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORC_MOB: EscalationProfile = EscalationProfile {
        mode: EscalationMode::Mob,
        growth_rate_bps: 2_000,
        baseline_enemy_power: BASELINE_ORC_POWER,
    };

    #[test]
    fn awareness_is_monotonic_bounded_and_has_authored_crossing_behavior() {
        let orc = (1..=12)
            .map(|ordinal| awareness_for_incident(80, ordinal))
            .collect::<Vec<_>>();
        assert!(orc.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(orc[0], 0);
        assert_eq!(first_public_incident(80), Some(5));
        assert_eq!(
            scheduled_public_since_minute(100, 60, 80),
            Some(340),
            "catch-up and incremental refreshes retain the scheduled crossing"
        );
        assert!(is_public(orc[4]));
        assert_eq!(first_public_incident(25), None);
        assert!(orc.into_iter().all(|value| value <= 8_000));
    }

    #[test]
    fn every_pre_saturation_follow_up_strictly_increases_effective_power() {
        let states = (1..=50)
            .map(|ordinal| combat_for_incident(2, 2, ordinal, ORC_MOB))
            .collect::<Vec<_>>();
        for pair in states.windows(2) {
            if pair[0].normalized_combat_power < MAX_ORC_EQUIVALENT_POWER {
                assert!(
                    pair[1].normalized_combat_power > pair[0].normalized_combat_power,
                    "{pair:?}"
                );
                assert!(
                    pair[1].enemy_count > pair[0].enemy_count
                        || pair[1].combat_scale_bps > pair[0].combat_scale_bps,
                    "integer count plateaus must grow enemy stats"
                );
            }
            assert!(pair[1].enemy_count >= pair[0].enemy_count);
            assert!(pair[1].combat_scale_bps >= pair[0].combat_scale_bps);
            assert!(pair[1].difficulty >= pair[0].difficulty);
        }
    }

    #[test]
    fn global_power_is_comparable_for_weak_mobs_and_strong_solos() {
        let weak = EscalationProfile {
            baseline_enemy_power: 5_000,
            ..ORC_MOB
        };
        let strong = EscalationProfile {
            mode: EscalationMode::Single,
            baseline_enemy_power: 150_000,
            ..ORC_MOB
        };
        let weak_late = combat_for_incident(2, 2, u16::MAX, weak);
        let strong_late = combat_for_incident(1, 2, u16::MAX, strong);
        assert_eq!(weak_late.normalized_combat_power, MAX_ORC_EQUIVALENT_POWER);
        assert_eq!(
            strong_late.normalized_combat_power,
            MAX_ORC_EQUIVALENT_POWER
        );
        assert!(weak_late.enemy_count > strong_late.enemy_count);
        assert!(strong_late.combat_scale_bps > COMBAT_SCALE_BPS);
    }

    #[test]
    fn hostile_count_boundaries_are_defined_and_never_weaken_the_base() {
        for count in [0, 1, 30, 31, u32::MAX] {
            let first = combat_for_incident(count, 2, 1, ORC_MOB);
            let base = count.clamp(1, MAX_MOB_COUNT);
            assert_eq!(first.enemy_count, base);
            assert!(first.difficulty >= 2);
            assert!(first.combat_scale_bps >= COMBAT_SCALE_BPS);
            assert!(first.normalized_combat_power <= MAX_ORC_EQUIVALENT_POWER);
        }
    }

    #[test]
    fn tactical_and_autoresolve_multipliers_consume_scale_once_and_monotonically() {
        assert_eq!(combat_physical_multiplier(10_000), 1.0);
        assert_eq!(combat_training_multiplier(10_000), 1.0);
        let earlier = combat_for_incident(2, 2, 2, ORC_MOB);
        let later = combat_for_incident(2, 2, 3, ORC_MOB);
        assert!(
            combat_physical_multiplier(later.combat_scale_bps)
                >= combat_physical_multiplier(earlier.combat_scale_bps)
        );
        assert!(
            combat_training_multiplier(later.combat_scale_bps)
                >= combat_training_multiplier(earlier.combat_scale_bps)
        );

        let weakest_single = combat_for_incident(
            1,
            1,
            u16::MAX,
            EscalationProfile {
                mode: EscalationMode::Single,
                growth_rate_bps: 2_000,
                baseline_enemy_power: MIN_BASELINE_ENEMY_POWER,
            },
        );
        assert_eq!(weakest_single.combat_scale_bps, MAX_COMBAT_SCALE_BPS);
        assert_eq!(
            weakest_single.normalized_combat_power,
            MAX_ORC_EQUIVALENT_POWER
        );
        let consumed_power = (MIN_BASELINE_ENEMY_POWER as f32
            * combat_training_multiplier(weakest_single.combat_scale_bps))
        .round() as u32;
        assert_eq!(consumed_power, weakest_single.normalized_combat_power);
        assert!(combat_physical_multiplier(weakest_single.combat_scale_bps).is_finite());
        assert_eq!(
            combat_training_multiplier(u32::MAX),
            MAX_COMBAT_SCALE_BPS as f32 / COMBAT_SCALE_BPS as f32
        );
    }

    #[test]
    fn count_bands_checks_and_hearing_boundaries_are_closed() {
        assert_eq!(approximate_count_band(1), "one");
        assert_eq!(approximate_count_band(4), "a few (2–4)");
        assert_eq!(approximate_count_band(20), "a horde (20+)");
        assert!(adjusted_difficulty_milli(3_000, 80) < 3_000);
        assert!(adjusted_difficulty_milli(3_000, 20) > 3_000);
        assert_eq!(
            hearing_radius(0, MAX_ORC_EQUIVALENT_POWER, PUBLIC_THRESHOLD_BPS - 1),
            0
        );
        let zero = hearing_radius(0, BASELINE_ORC_POWER, PUBLIC_THRESHOLD_BPS);
        let one = hearing_radius(1, BASELINE_ORC_POWER, PUBLIC_THRESHOLD_BPS);
        let saturated = hearing_radius(u32::MAX, u32::MAX, u16::MAX);
        assert_eq!(zero, one);
        assert!((1..=18).contains(&saturated));
    }

    #[test]
    fn referral_access_range_and_payload_are_closed_observer_safe_behavior() {
        assert_eq!(
            public_referral_source(true, true, false, false),
            Some("innkeeper")
        );
        assert_eq!(public_referral_source(true, false, true, false), None);
        assert_eq!(
            public_referral_source(true, false, true, true),
            Some("organization")
        );
        assert_eq!(public_referral_source(false, true, true, true), None);
        assert!(hearing_allows(false, true, None, 0, 10_000, 6_500));
        assert!(hearing_allows(false, false, Some(25_000), 0, 10_000, 6_500));
        assert!(!hearing_allows(
            false,
            false,
            Some(25_001),
            0,
            10_000,
            6_500
        ));
        assert!(!hearing_allows(
            false,
            false,
            None,
            u32::MAX,
            300_000,
            10_000
        ));
        let summary = public_threat_summary("Orcs", "Old Quarry", "a few (2–4)");
        assert_eq!(summary, "Orcs at Old Quarry; reported number: a few (2–4).");
        for secret in ["evidence", "testimony", "preparation", "manifest"] {
            assert!(!summary.to_ascii_lowercase().contains(secret));
        }
        assert_eq!(
            referral_delivery_authority_kind(true, false),
            ReferralDeliveryAuthorityKind::LocalProblem
        );
        assert_eq!(
            referral_delivery_authority_kind(false, true),
            ReferralDeliveryAuthorityKind::PublicThreat
        );
        assert_eq!(
            referral_delivery_authority_kind(false, false),
            ReferralDeliveryAuthorityKind::Missing
        );
        assert_eq!(
            referral_delivery_authority_kind(true, true),
            ReferralDeliveryAuthorityKind::Conflict
        );
    }

    #[test]
    fn journal_identity_is_stable_across_referral_refreshes() {
        let first = public_threat_journal_id(42, "public-case:orc-quarry");
        assert_eq!(
            first,
            public_threat_journal_id(42, "public-case:orc-quarry")
        );
        assert_ne!(
            first,
            public_threat_journal_id(43, "public-case:orc-quarry")
        );
        assert_ne!(first, public_threat_journal_id(42, "public-case:other"));
    }

    #[test]
    fn public_candidate_selection_is_bounded_oldest_first_and_stable() {
        let mut inputs = (0..100_u64)
            .rev()
            .map(|ordinal| {
                (
                    ordinal / 2,
                    format!("settlement-{}", ordinal % 2),
                    format!("problem-{ordinal:03}"),
                    ordinal,
                )
            })
            .collect::<Vec<_>>();
        inputs.push((0, "settlement-0".into(), "problem-000".into(), 999));
        let selected = bounded_public_threat_candidates(inputs);
        assert_eq!(selected.len(), MAX_PUBLIC_THREAT_CANDIDATES);
        assert_eq!(selected[0], 0);
        assert_eq!(selected[1], 999);
        assert!(selected.contains(&1));
        assert!(!selected.contains(&99));
    }
}
