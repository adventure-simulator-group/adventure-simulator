//! Deterministic strategic investigation actions.
//!
//! This module deliberately accepts no canonical case truth. The server binds a
//! private target to an opaque capability and supplies only authoritative
//! environmental and party inputs.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationActionKind {
    InspectSite,
    SearchArea,
    FollowTracks,
    ReacquireTracks,
    LocateContact,
    Watch,
    Patrol,
    LayAmbush,
    ApproachLead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    Road,
    Settlement,
    Plains,
    Forest,
    Hills,
    Marsh,
    Ruins,
    Underground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Day,
    Night,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherAuthority {
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillContribution {
    /// The best relevant terrain skill in basis points.
    pub terrain_bps: u16,
    /// Perception/awareness contribution in basis points.
    pub awareness_bps: u16,
    /// Stealth contribution used by watch and ambush actions.
    pub stealth_bps: u16,
    /// Bounded contribution from the rest of the party.
    pub assistance_bps: u16,
    /// Familiarity with this locality, in basis points.
    pub familiarity_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionPrerequisites {
    pub required_terrain: Option<Terrain>,
    pub minimum_party_members: u8,
    pub requires_tracks: bool,
    pub requires_contact_referral: bool,
    pub requires_approximate_destination: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategicCost {
    pub minutes: u32,
    pub fatigue: u16,
    pub food_units: u16,
    pub water_units: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionInput {
    pub seed: u64,
    pub attempt_index: u32,
    pub kind: InvestigationActionKind,
    pub terrain: Terrain,
    pub target_terrain: Terrain,
    pub time_of_day: TimeOfDay,
    pub evidence_age_minutes: u64,
    pub current_uncertainty_bps: u16,
    pub skills: SkillContribution,
    pub weather: WeatherAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionResultKind {
    EvidenceFound,
    AreaNarrowed,
    TracksFollowed,
    TracksReacquired,
    ContactLocated,
    ObservationMade,
    AmbushPrepared,
    LeadApproached,
    NoNewInformation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub result: ActionResultKind,
    pub success: bool,
    pub cost: StrategicCost,
    pub resulting_uncertainty_bps: u16,
    pub risk_bps: u16,
    pub risk_triggered: bool,
    pub effective_skill_bps: u16,
}

/// A valid generated physical investigation route always resolves by this
/// attempt when its uninterrupted failure history remains intact.
pub const GENERATED_PHYSICAL_ATTEMPT_BOUND: u32 = 6;
pub const GENERATED_PHYSICAL_PROGRESS_BPS_PER_FAILURE: u16 = 1_900;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedProgressResolution {
    pub resolution: Resolution,
    pub attempt_number: u32,
    pub persistent_progress_bps: u16,
    pub success_threshold_bps: u16,
    pub guaranteed_by_attempt: u32,
}

pub fn prerequisites(kind: InvestigationActionKind) -> ActionPrerequisites {
    use InvestigationActionKind as K;
    ActionPrerequisites {
        required_terrain: None,
        minimum_party_members: if kind == K::Patrol { 2 } else { 1 },
        requires_tracks: matches!(kind, K::FollowTracks | K::ReacquireTracks),
        requires_contact_referral: kind == K::LocateContact,
        requires_approximate_destination: matches!(kind, K::SearchArea | K::ApproachLead),
    }
}

pub fn base_cost(kind: InvestigationActionKind) -> StrategicCost {
    use InvestigationActionKind as K;
    match kind {
        K::InspectSite => StrategicCost {
            minutes: 45,
            fatigue: 80,
            food_units: 0,
            water_units: 1,
        },
        K::SearchArea => StrategicCost {
            minutes: 180,
            fatigue: 240,
            food_units: 1,
            water_units: 2,
        },
        K::FollowTracks | K::ReacquireTracks => StrategicCost {
            minutes: 120,
            fatigue: 200,
            food_units: 1,
            water_units: 2,
        },
        K::LocateContact => StrategicCost {
            minutes: 60,
            fatigue: 40,
            food_units: 0,
            water_units: 0,
        },
        K::Watch | K::LayAmbush => StrategicCost {
            minutes: 240,
            fatigue: 160,
            food_units: 1,
            water_units: 1,
        },
        K::Patrol => StrategicCost {
            minutes: 180,
            fatigue: 260,
            food_units: 1,
            water_units: 2,
        },
        K::ApproachLead => StrategicCost {
            minutes: 90,
            fatigue: 140,
            food_units: 0,
            water_units: 1,
        },
    }
}

/// Resolve with a domain-separated deterministic roll. Assistance is capped at
/// 2,000 bps so one specialist remains important.
pub fn resolve(input: ResolutionInput) -> Resolution {
    let terrain_match = if input.terrain == input.target_terrain {
        1_500
    } else {
        0
    };
    let age_penalty = (input.evidence_age_minutes / 60).min(3_000) as i32;
    let night = match (input.kind, input.time_of_day) {
        (InvestigationActionKind::Watch | InvestigationActionKind::LayAmbush, TimeOfDay::Night) => {
            500
        }
        (_, TimeOfDay::Night) => -700,
        _ => 0,
    };
    let primary = match input.kind {
        InvestigationActionKind::Watch
        | InvestigationActionKind::Patrol
        | InvestigationActionKind::LocateContact => input.skills.awareness_bps,
        InvestigationActionKind::LayAmbush => {
            (u32::from(input.skills.awareness_bps) + u32::from(input.skills.stealth_bps)) as u16 / 2
        }
        _ => input.skills.terrain_bps,
    };
    let assistance = input.skills.assistance_bps.min(2_000);
    let effective = (i32::from(primary)
        + i32::from(assistance)
        + i32::from(input.skills.familiarity_bps) / 3
        + terrain_match
        + night
        - age_penalty)
        .clamp(500, 9_500) as u16;
    let roll = domain_roll(input.seed, input.attempt_index, input.kind);
    let success = roll < effective;
    let base = base_cost(input.kind);
    let skill_time_reduction = u32::from(effective) * base.minutes / 20_000;
    let mismatch_penalty = if input.terrain == input.target_terrain {
        0
    } else {
        base.minutes / 2
    };
    let cost = StrategicCost {
        minutes: base
            .minutes
            .saturating_sub(skill_time_reduction)
            .saturating_add(mismatch_penalty)
            .max(15),
        ..base
    };
    let change = if success {
        1_800u16.saturating_add(effective / 5)
    } else {
        // A failed search still maps ground but may broaden an overconfident area.
        300
    };
    let resulting_uncertainty_bps = if success {
        input.current_uncertainty_bps.saturating_sub(change)
    } else {
        input
            .current_uncertainty_bps
            .saturating_add(change)
            .min(10_000)
    };
    let risk_bps = if success { 500 } else { 1_500 };
    Resolution {
        result: result_kind(input.kind, success),
        success,
        cost,
        resulting_uncertainty_bps,
        risk_bps,
        risk_triggered: domain_roll(
            input.seed ^ 0x5249_534b_5f52_4f4c,
            input.attempt_index,
            input.kind,
        ) < risk_bps,
        effective_skill_bps: effective,
    }
}

/// Preserve the ordinary physical resolution and costs while allowing repeated
/// work on one generated route to accumulate bounded, non-transferable progress.
pub fn resolve_with_bounded_progress(
    input: ResolutionInput,
    consecutive_failures: u32,
) -> BoundedProgressResolution {
    let mut resolution = resolve(input);
    let prior = consecutive_failures.min(GENERATED_PHYSICAL_ATTEMPT_BOUND - 1);
    let progress = u32::from(GENERATED_PHYSICAL_PROGRESS_BPS_PER_FAILURE).saturating_mul(prior);
    let success_threshold_bps = u32::from(resolution.effective_skill_bps)
        .saturating_add(progress)
        .min(10_000) as u16;
    let success = domain_roll(input.seed, input.attempt_index, input.kind) < success_threshold_bps;
    if success != resolution.success {
        resolution.success = success;
        resolution.result = result_kind(input.kind, success);
        resolution.risk_bps = 500;
        resolution.risk_triggered = domain_roll(
            input.seed ^ 0x5249_534b_5f52_4f4c,
            input.attempt_index,
            input.kind,
        ) < resolution.risk_bps;
    }
    resolution.resulting_uncertainty_bps = if success {
        input
            .current_uncertainty_bps
            .saturating_sub(1_800u16.saturating_add(resolution.effective_skill_bps / 5))
    } else {
        // Even an inconclusive pass maps ground for this exact route.
        input.current_uncertainty_bps.saturating_sub(300)
    };
    BoundedProgressResolution {
        resolution,
        attempt_number: prior.saturating_add(1),
        persistent_progress_bps: progress.min(10_000) as u16,
        success_threshold_bps,
        guaranteed_by_attempt: GENERATED_PHYSICAL_ATTEMPT_BOUND,
    }
}

fn result_kind(kind: InvestigationActionKind, success: bool) -> ActionResultKind {
    if !success {
        return ActionResultKind::NoNewInformation;
    }
    use ActionResultKind as R;
    use InvestigationActionKind as K;
    match kind {
        K::InspectSite => R::EvidenceFound,
        K::SearchArea => R::AreaNarrowed,
        K::FollowTracks => R::TracksFollowed,
        K::ReacquireTracks => R::TracksReacquired,
        K::LocateContact => R::ContactLocated,
        K::Watch | K::Patrol => R::ObservationMade,
        K::LayAmbush => R::AmbushPrepared,
        K::ApproachLead => R::LeadApproached,
    }
}

fn domain_roll(seed: u64, attempt: u32, kind: InvestigationActionKind) -> u16 {
    let mut value =
        seed ^ 0x494e_5645_5354_4143 ^ (u64::from(attempt) << 17) ^ ((kind as u64) << 41);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) % 10_000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: InvestigationActionKind) -> ResolutionInput {
        ResolutionInput {
            seed: 42,
            attempt_index: 0,
            kind,
            terrain: Terrain::Forest,
            target_terrain: Terrain::Forest,
            time_of_day: TimeOfDay::Day,
            evidence_age_minutes: 60,
            current_uncertainty_bps: 8_000,
            skills: SkillContribution {
                terrain_bps: 7_000,
                awareness_bps: 6_000,
                stealth_bps: 5_000,
                assistance_bps: 9_000,
                familiarity_bps: 3_000,
            },
            weather: WeatherAuthority::Unavailable,
        }
    }

    #[test]
    fn resolution_is_deterministic_and_domain_separated() {
        let a = resolve(input(InvestigationActionKind::SearchArea));
        assert_eq!(a, resolve(input(InvestigationActionKind::SearchArea)));
        assert_ne!(a, resolve(input(InvestigationActionKind::FollowTracks)));
    }

    #[test]
    fn bounded_progress_preserves_first_chance_and_guarantees_sixth_attempt() {
        for (high_skill, expected_effective) in [(false, 500), (true, 9_500)] {
            let mut candidate = input(InvestigationActionKind::ReacquireTracks);
            candidate.skills.terrain_bps = if high_skill { 9_500 } else { 0 };
            candidate.skills.assistance_bps = if high_skill { 2_000 } else { 0 };
            candidate.skills.familiarity_bps = if high_skill { 3_000 } else { 0 };
            candidate.terrain = if high_skill {
                Terrain::Forest
            } else {
                Terrain::Road
            };
            candidate.target_terrain = Terrain::Forest;
            candidate.evidence_age_minutes = if high_skill { 0 } else { 600_000 };
            let mut saw_ordinary_early_success = false;
            for seed in 0..128 {
                candidate.seed = seed;
                let ordinary = resolve(candidate);
                assert_eq!(ordinary.effective_skill_bps, expected_effective);
                saw_ordinary_early_success |= ordinary.success;
                let first = resolve_with_bounded_progress(candidate, 0);
                assert_eq!(first.resolution.success, ordinary.success);
                assert_eq!(first.success_threshold_bps, ordinary.effective_skill_bps);
                assert_eq!(first.persistent_progress_bps, 0);
                let sixth = resolve_with_bounded_progress(candidate, 5);
                assert!(sixth.resolution.success);
                assert_eq!(sixth.persistent_progress_bps, 9_500);
                assert_eq!(sixth.success_threshold_bps, 10_000);
                assert_eq!(
                    sixth.guaranteed_by_attempt,
                    GENERATED_PHYSICAL_ATTEMPT_BOUND
                );
            }
            if high_skill {
                assert!(saw_ordinary_early_success);
            }
        }
    }

    #[test]
    fn failed_bounded_attempt_truthfully_reduces_uncertainty() {
        let mut candidate = input(InvestigationActionKind::ReacquireTracks);
        candidate.skills.terrain_bps = 0;
        candidate.skills.assistance_bps = 0;
        candidate.skills.familiarity_bps = 0;
        candidate.terrain = Terrain::Road;
        candidate.evidence_age_minutes = 600_000;
        let failing_seed = (0..u64::MAX)
            .find(|seed| {
                !resolve_with_bounded_progress(
                    ResolutionInput {
                        seed: *seed,
                        ..candidate
                    },
                    0,
                )
                .resolution
                .success
            })
            .unwrap();
        candidate.seed = failing_seed;
        let failed = resolve_with_bounded_progress(candidate, 0);
        assert!(!failed.resolution.success);
        assert_eq!(failed.resolution.resulting_uncertainty_bps, 7_700);
        assert_eq!(failed.attempt_number, 1);
    }

    #[test]
    fn terrain_and_age_change_skill_time_and_uncertainty() {
        let matched = resolve(input(InvestigationActionKind::SearchArea));
        let mut poor = input(InvestigationActionKind::SearchArea);
        poor.terrain = Terrain::Road;
        poor.evidence_age_minutes = 600_000;
        let poor = resolve(poor);
        assert!(matched.effective_skill_bps > poor.effective_skill_bps);
        assert!(matched.cost.minutes < poor.cost.minutes);
    }

    #[test]
    fn assistance_is_bounded_and_weather_is_explicitly_unavailable() {
        let capped = resolve(input(InvestigationActionKind::InspectSite));
        let mut exact_cap = input(InvestigationActionKind::InspectSite);
        exact_cap.skills.assistance_bps = 2_000;
        assert_eq!(capped, resolve(exact_cap));
        assert_eq!(exact_cap.weather, WeatherAuthority::Unavailable);
    }

    #[test]
    fn failures_never_delete_the_route() {
        let mut hard = input(InvestigationActionKind::ReacquireTracks);
        hard.seed = 1;
        hard.skills.terrain_bps = 0;
        hard.skills.assistance_bps = 0;
        hard.skills.familiarity_bps = 0;
        hard.evidence_age_minutes = u64::MAX;
        hard.current_uncertainty_bps = 9_800;
        let result = resolve(hard);
        assert!(!result.success);
        assert_eq!(result.result, ActionResultKind::NoNewInformation);
        assert!(result.resulting_uncertainty_bps <= 10_000);
        assert!(result.cost.minutes > 0);
    }

    #[test]
    fn risk_is_deterministic_and_distinct_from_action_success() {
        let input = input(InvestigationActionKind::Watch);
        let first = resolve(input);
        let second = resolve(input);
        assert_eq!(first.risk_triggered, second.risk_triggered);
        assert!(first.risk_bps <= 10_000);
    }
}
