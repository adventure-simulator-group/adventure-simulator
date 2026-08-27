//! Pure deterministic policy choices for autonomous full Characters.
//!
//! These helpers deliberately know nothing about database state. The
//! authoritative scheduler supplies a bounded snapshot, records the selected
//! outcome, and only then advances personal time.

use fabelgeist_determinism::mix64;

use crate::strategic_schedule::DailySchedule;

const NPC_POLICY_ACTOR_ID_STRIDE: u64 = 0x9e37_79b9_7f4a_7c15;
const NPC_POLICY_DAY_STRIDE: u64 = 0xbf58_476d_1ce4_e5b9;
const NPC_POLICY_CANDIDATE_STRIDE: u64 = 0x94d0_49bb_1331_11eb;

pub const NPC_ROMANCE_CANDIDATE_CAP: usize = 16;
pub const NPC_SCHEDULE_QUANTUM_MINUTES: u16 = 15;
pub const NPC_HOUSING_RESERVE_PERIODS: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcCandidate {
    pub character_id: u64,
    pub policy_seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NpcHouseOffer {
    /// Increasing comfort rank. A higher rank is preferred.
    pub rank: u8,
    pub initial_cost: u64,
    pub recurring_cost: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcCourtshipRoute {
    Formal,
    Informal,
    Ineligible,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NpcCourtshipEligibility {
    pub both_npc: bool,
    pub co_located: bool,
    pub living_adults: bool,
    pub mutually_attracted: bool,
    pub nonkin: bool,
    pub conflict_free: bool,
    /// The direction eligible for the currently implemented formal route:
    /// man suitor and woman partner.
    pub formal_pair: bool,
    pub father_approves: bool,
    pub formal_affinity_met: bool,
    pub informal_affinity_met: bool,
}

pub const fn npc_courtship_route(inputs: NpcCourtshipEligibility) -> NpcCourtshipRoute {
    if !inputs.both_npc
        || !inputs.co_located
        || !inputs.living_adults
        || !inputs.mutually_attracted
        || !inputs.nonkin
        || !inputs.conflict_free
    {
        return NpcCourtshipRoute::Ineligible;
    }
    if inputs.formal_pair && inputs.father_approves && inputs.formal_affinity_met {
        NpcCourtshipRoute::Formal
    } else if inputs.informal_affinity_met {
        NpcCourtshipRoute::Informal
    } else {
        NpcCourtshipRoute::Ineligible
    }
}

/// Stable mixer used only for policy ordering. It has no platform or
/// collection iteration dependency.
pub const fn npc_policy_hash(seed: u64, actor_id: u64, day: u64, value: u64) -> u64 {
    mix64(
        seed ^ actor_id.wrapping_mul(NPC_POLICY_ACTOR_ID_STRIDE)
            ^ day.wrapping_mul(NPC_POLICY_DAY_STRIDE)
            ^ value.wrapping_mul(NPC_POLICY_CANDIDATE_STRIDE),
    )
}

/// Returns a bounded permutation-independent candidate order.
pub fn stable_candidate_order(
    actor_id: u64,
    actor_seed: u64,
    day: u64,
    candidates: impl IntoIterator<Item = NpcCandidate>,
) -> Vec<NpcCandidate> {
    let mut candidates: Vec<_> = candidates.into_iter().collect();
    candidates.sort_by_key(|candidate| {
        (
            npc_policy_hash(
                actor_seed ^ candidate.policy_seed,
                actor_id,
                day,
                candidate.character_id,
            ),
            candidate.character_id,
        )
    });
    candidates.dedup_by_key(|candidate| candidate.character_id);
    candidates.truncate(NPC_ROMANCE_CANDIDATE_CAP);
    candidates
}

/// Conservative saved plan used exactly once for a new NPC policy. Work
/// produces income, conversation exercises ordinary Socializing target
/// priority, and all remaining time is Leisure.
pub fn initial_npc_schedule(character_id: u64, policy_seed: u64) -> DailySchedule {
    let socializing_minutes =
        60 + 15 * (npc_policy_hash(policy_seed, character_id, 0, 1) % 5) as u16;
    DailySchedule {
        socializing_minutes,
        labor: 6 * 60,
        ..DailySchedule::default()
    }
}

/// Chooses the nicest affordable house while retaining one recurring payment
/// as a reserve. No choice can increase the caller's funds.
pub fn best_affordable_house(
    available: u64,
    offers: impl IntoIterator<Item = NpcHouseOffer>,
) -> Option<NpcHouseOffer> {
    offers
        .into_iter()
        .filter(|offer| {
            offer.initial_cost.saturating_add(
                offer
                    .recurring_cost
                    .saturating_mul(NPC_HOUSING_RESERVE_PERIODS),
            ) <= available
        })
        .max_by_key(|offer| (offer.rank, offer.initial_cost, offer.recurring_cost))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_order_is_permutation_independent_unique_and_capped() {
        let forward: Vec<_> = (1..=40)
            .map(|character_id| NpcCandidate {
                character_id,
                policy_seed: character_id * 7,
            })
            .collect();
        let mut reverse = forward.clone();
        reverse.reverse();
        reverse.push(forward[0]);
        let first = stable_candidate_order(99, 123, 45, forward);
        let second = stable_candidate_order(99, 123, 45, reverse);
        assert_eq!(first, second);
        assert_eq!(first.len(), NPC_ROMANCE_CANDIDATE_CAP);
    }

    #[test]
    fn initial_schedule_has_income_socializing_and_quarter_hour_units() {
        for character_id in 1..20 {
            let schedule = initial_npc_schedule(character_id, character_id * 11);
            assert!(schedule.labor > 0);
            assert!((60..=120).contains(&schedule.socializing_minutes));
            assert_eq!(schedule.labor % NPC_SCHEDULE_QUANTUM_MINUTES, 0);
            assert_eq!(
                schedule.socializing_minutes % NPC_SCHEDULE_QUANTUM_MINUTES,
                0
            );
            assert!(schedule.allocated_minutes() < 24 * 60);
        }
    }

    #[test]
    fn housing_choice_keeps_reserve_and_never_fabricates_money() {
        let offers = [
            NpcHouseOffer {
                rank: 0,
                initial_cost: 10,
                recurring_cost: 10,
            },
            NpcHouseOffer {
                rank: 1,
                initial_cost: 30,
                recurring_cost: 20,
            },
            NpcHouseOffer {
                rank: 2,
                initial_cost: 80,
                recurring_cost: 30,
            },
        ];
        assert_eq!(best_affordable_house(19, offers), None);
        assert_eq!(best_affordable_house(20, offers).unwrap().rank, 0);
        let selected = best_affordable_house(50, offers).unwrap();
        assert_eq!(selected.rank, 1);
        assert!(selected.initial_cost <= 50);
        assert!(selected.initial_cost + selected.recurring_cost <= 50);
    }

    fn eligible() -> NpcCourtshipEligibility {
        NpcCourtshipEligibility {
            both_npc: true,
            co_located: true,
            living_adults: true,
            mutually_attracted: true,
            nonkin: true,
            conflict_free: true,
            formal_pair: true,
            father_approves: true,
            formal_affinity_met: true,
            informal_affinity_met: true,
        }
    }

    #[test]
    fn formal_and_informal_routes_are_explicit() {
        assert_eq!(npc_courtship_route(eligible()), NpcCourtshipRoute::Formal);
        assert_eq!(
            npc_courtship_route(NpcCourtshipEligibility {
                father_approves: false,
                ..eligible()
            }),
            NpcCourtshipRoute::Informal
        );
        assert_eq!(
            npc_courtship_route(NpcCourtshipEligibility {
                formal_pair: false,
                ..eligible()
            }),
            NpcCourtshipRoute::Informal
        );
    }

    #[test]
    fn player_targets_kin_and_unattracted_pairs_are_rejected() {
        for inputs in [
            NpcCourtshipEligibility {
                both_npc: false,
                ..eligible()
            },
            NpcCourtshipEligibility {
                nonkin: false,
                ..eligible()
            },
            NpcCourtshipEligibility {
                mutually_attracted: false,
                ..eligible()
            },
        ] {
            assert_eq!(npc_courtship_route(inputs), NpcCourtshipRoute::Ineligible);
        }
    }
}
