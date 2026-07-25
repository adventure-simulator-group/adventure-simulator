//! Deterministic strategic decisions for NPC adventuring companies.
//!
//! SpacetimeDB owns scheduling and persistence. This module owns the pure,
//! replayable eligibility, strategy, party-selection, and outcome rules so the
//! server and evaluator cannot quietly diverge.

use serde::{Deserialize, Serialize};

pub const MIN_INTERVENTION_AGE_MINUTES: u64 = 5 * 1_440;
pub const PLAYER_ACTIVITY_GRACE_MINUTES: u64 = 2 * 1_440;
pub const RETRY_DELAY_MINUTES: u64 = 3 * 1_440;
pub const MIN_INTERVENTION_INCIDENTS: u16 = 2;
pub const MAX_CAPABILITY: u16 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcCaseSnapshot {
    pub case_id: String,
    pub problem_id: String,
    pub settlement_id: String,
    pub opened_at: u64,
    pub incident_count: u16,
    pub mitigation_bps: u16,
    pub open: bool,
    pub player_activity_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcPartySnapshot {
    pub party_id: String,
    pub name: String,
    pub settlement_id: String,
    pub capability: u16,
    pub available_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcInterventionStrategy {
    InvestigateCarefully,
    ProtectLocals,
    ConfrontDirectly,
    Defer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcInterventionOutcome {
    Resolved,
    Mitigated,
    Failed,
    Delayed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcInterventionDecision {
    pub strategy: NpcInterventionStrategy,
    pub outcome: NpcInterventionOutcome,
    pub mitigation_bps: u16,
    pub next_available_at: u64,
    pub roll_bps: u16,
    pub safe_summary: String,
}

pub fn eligible_at(case: &NpcCaseSnapshot) -> u64 {
    let aged = case.opened_at.saturating_add(MIN_INTERVENTION_AGE_MINUTES);
    case.player_activity_at.map_or(aged, |at| {
        aged.max(at.saturating_add(PLAYER_ACTIVITY_GRACE_MINUTES))
    })
}

pub fn case_is_eligible(case: &NpcCaseSnapshot, now: u64) -> bool {
    case.open
        && case.incident_count >= MIN_INTERVENTION_INCIDENTS
        && case.mitigation_bps < 10_000
        && now >= eligible_at(case)
}

pub fn select_party<'a>(
    case: &NpcCaseSnapshot,
    now: u64,
    parties: impl IntoIterator<Item = &'a NpcPartySnapshot>,
) -> Option<&'a NpcPartySnapshot> {
    parties
        .into_iter()
        .filter(|party| {
            party.settlement_id == case.settlement_id
                && party.available_at <= now
                && party.capability <= MAX_CAPABILITY
        })
        .max_by_key(|party| {
            (
                party.capability,
                stable_hash(&format!("{}:{}", case.case_id, party.party_id)),
            )
        })
}

pub fn scripted_strategy(
    case: &NpcCaseSnapshot,
    party: &NpcPartySnapshot,
) -> NpcInterventionStrategy {
    if party.capability >= 70 {
        NpcInterventionStrategy::InvestigateCarefully
    } else if case.incident_count >= 4 {
        NpcInterventionStrategy::ProtectLocals
    } else {
        NpcInterventionStrategy::ConfrontDirectly
    }
}

pub fn decide(
    case: &NpcCaseSnapshot,
    party: &NpcPartySnapshot,
    strategy: NpcInterventionStrategy,
    attempt: u16,
    now: u64,
) -> NpcInterventionDecision {
    let roll = (stable_hash(&format!(
        "npc-intervention-v1:{}:{}:{attempt}:{strategy:?}",
        case.case_id, party.party_id
    )) % 10_000) as u16;
    let capability = u32::from(party.capability.min(MAX_CAPABILITY));
    let incident_pressure = u32::from(case.incident_count.saturating_sub(1)).min(8) * 350;
    let strategy_bonus: i32 = match strategy {
        NpcInterventionStrategy::InvestigateCarefully => 1_100,
        NpcInterventionStrategy::ProtectLocals => 350,
        NpcInterventionStrategy::ConfrontDirectly => -250,
        NpcInterventionStrategy::Defer => {
            return decision(
                strategy,
                NpcInterventionOutcome::Delayed,
                case.mitigation_bps,
                now,
                roll,
                format!(
                    "{} postponed its investigation while other obligations took priority.",
                    party.name
                ),
            );
        }
    };
    let resolution_threshold = (1_500i32 + (capability as i32 * 65) + strategy_bonus
        - incident_pressure as i32)
        .clamp(500, 8_500) as u32;
    let mitigation_threshold = (resolution_threshold + 2_250).min(9_500);
    if u32::from(roll) < resolution_threshold {
        decision(
            strategy,
            NpcInterventionOutcome::Resolved,
            10_000,
            now,
            roll,
            format!(
                "{} investigated the local trouble and brought the incidents to an end.",
                party.name
            ),
        )
    } else if u32::from(roll) < mitigation_threshold {
        let mitigation = case
            .mitigation_bps
            .max((2_500 + capability * 50).min(8_000) as u16);
        decision(
            strategy,
            NpcInterventionOutcome::Mitigated,
            mitigation,
            now,
            roll,
            format!(
                "{} could not end the trouble, but its intervention reduced the harm to local people.",
                party.name
            ),
        )
    } else {
        decision(
            strategy,
            NpcInterventionOutcome::Failed,
            case.mitigation_bps,
            now,
            roll,
            format!(
                "{} returned without resolving the local trouble.",
                party.name
            ),
        )
    }
}

fn decision(
    strategy: NpcInterventionStrategy,
    outcome: NpcInterventionOutcome,
    mitigation_bps: u16,
    now: u64,
    roll_bps: u16,
    safe_summary: String,
) -> NpcInterventionDecision {
    NpcInterventionDecision {
        strategy,
        outcome,
        mitigation_bps,
        next_available_at: now.saturating_add(RETRY_DELAY_MINUTES),
        roll_bps,
        safe_summary,
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> NpcCaseSnapshot {
        NpcCaseSnapshot {
            case_id: "case:one".into(),
            problem_id: "problem:one".into(),
            settlement_id: "town".into(),
            opened_at: 1_000,
            incident_count: 3,
            mitigation_bps: 0,
            open: true,
            player_activity_at: None,
        }
    }

    fn party(id: &str, capability: u16) -> NpcPartySnapshot {
        NpcPartySnapshot {
            party_id: id.into(),
            name: format!("{id} Company"),
            settlement_id: "town".into(),
            capability,
            available_at: 0,
        }
    }

    #[test]
    fn eligibility_waits_for_age_incidents_and_player_grace() {
        let mut value = case();
        let aged = value.opened_at + MIN_INTERVENTION_AGE_MINUTES;
        assert!(!case_is_eligible(&value, aged - 1));
        assert!(case_is_eligible(&value, aged));
        value.incident_count = 1;
        assert!(!case_is_eligible(&value, u64::MAX));
        value.incident_count = 3;
        value.player_activity_at = Some(aged + 10);
        assert!(!case_is_eligible(
            &value,
            aged + PLAYER_ACTIVITY_GRACE_MINUTES
        ));
        assert!(case_is_eligible(
            &value,
            aged + 10 + PLAYER_ACTIVITY_GRACE_MINUTES
        ));
    }

    #[test]
    fn selection_and_outcome_are_replay_stable() {
        let value = case();
        let weak = party("weak", 40);
        let strong = party("strong", 80);
        assert_eq!(
            select_party(&value, 10_000, [&weak, &strong])
                .unwrap()
                .party_id,
            "strong"
        );
        let strategy = scripted_strategy(&value, &strong);
        assert_eq!(
            decide(&value, &strong, strategy, 1, 10_000),
            decide(&value, &strong, strategy, 1, 10_000)
        );
    }

    #[test]
    fn defer_never_resolves_or_mitigates() {
        let value = case();
        let decision = decide(
            &value,
            &party("cautious", 100),
            NpcInterventionStrategy::Defer,
            1,
            5_000,
        );
        assert_eq!(decision.outcome, NpcInterventionOutcome::Delayed);
        assert_eq!(decision.mitigation_bps, value.mitigation_bps);
    }
}
