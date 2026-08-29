//! Deterministic fixture-party identity and lane assignment policy.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FixturePartyIdentity {
    pub(super) leader_id: u64,
    pub(super) party_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FixturePartyCandidate {
    pub(super) identity: FixturePartyIdentity,
    pub(super) assessment: PublicContractAssessment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FixturePartySelection {
    pub(super) direct: FixturePartyIdentity,
    pub(super) generated: FixturePartyIdentity,
}

pub(super) fn select_strongest_fixture_party(
    mut candidates: Vec<FixturePartyCandidate>,
) -> Result<FixturePartySelection, String> {
    if candidates.len() != 2 {
        return Err("quest fixture designation requires exactly two parties".into());
    }
    candidates.sort_by(|left, right| {
        right
            .assessment
            .party_power_milli
            .cmp(&left.assessment.party_power_milli)
            .then_with(|| left.identity.party_id.cmp(&right.identity.party_id))
            .then_with(|| left.identity.leader_id.cmp(&right.identity.leader_id))
    });
    if !candidates[0].assessment.eligible {
        return Err("quest fixture has no publicly safe direct party".into());
    }
    let generated = candidates
        .pop()
        .expect("the fixture candidate count was checked");
    let direct = candidates
        .pop()
        .expect("the fixture candidate count was checked");
    Ok(FixturePartySelection {
        direct: direct.identity,
        generated: generated.identity,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FixtureQuestLane {
    Direct,
    Generated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FixtureLanePlan {
    pub(super) direct_contract_id: String,
    pub(super) generated_case_id: Option<String>,
    pub(super) direct_leader_id: u64,
    pub(super) generated_leader_id: u64,
    pub(super) direct_party_id: String,
    pub(super) generated_party_id: String,
}

pub(super) fn fixture_quest_lane(
    fixture: Option<&FixtureLanePlan>,
    leader_id: u64,
    party_id: &str,
) -> Option<FixtureQuestLane> {
    let fixture = fixture?;
    if fixture.direct_leader_id == leader_id && fixture.direct_party_id == party_id {
        Some(FixtureQuestLane::Direct)
    } else if fixture.generated_leader_id == leader_id && fixture.generated_party_id == party_id {
        Some(FixtureQuestLane::Generated)
    } else {
        None
    }
}

pub(super) fn visible_activity_committed_reserve(
    purse: u64,
    profile_cash_reserve_target: u64,
    observable_medical_reserve: Option<u64>,
    inn_cost: Option<u64>,
) -> u64 {
    let medical = observable_medical_reserve.unwrap_or(0);
    let spendable_after_medical_and_inn =
        inn_cost.map_or(0, |cost| purse.saturating_sub(medical.saturating_add(cost)));
    medical.saturating_add(profile_cash_reserve_target.min(spendable_after_medical_and_inn))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct QuestDecisionObservation<'a> {
    pub(super) cycle: u32,
    pub(super) wants_quest: bool,
    pub(super) selector: f64,
    pub(super) quest_propensity: f32,
    pub(super) settlement_id: Option<&'a str>,
    pub(super) offered_contracts: usize,
    pub(super) safe_offered_contracts: usize,
    pub(super) open_generated_cases: usize,
    pub(super) projected_investigation_actions: usize,
    pub(super) quest_path: &'a str,
    pub(super) quest_intended: bool,
    pub(super) quest_selected: bool,
    pub(super) selection_reason: &'a str,
}

pub(super) fn format_quest_decision_detail(decision: QuestDecisionObservation<'_>) -> String {
    let QuestDecisionObservation {
        cycle,
        wants_quest,
        selector,
        quest_propensity,
        settlement_id,
        offered_contracts,
        safe_offered_contracts,
        open_generated_cases,
        projected_investigation_actions,
        quest_path,
        quest_intended,
        quest_selected,
        selection_reason,
    } = decision;
    format!(
        "cycle={cycle};wants_quest={wants_quest};selector={selector:.6};quest_propensity={quest_propensity:.6};settlement={};offered_contracts={offered_contracts};safe_offered_contracts={safe_offered_contracts};open_generated_cases={open_generated_cases};projected_investigation_actions={projected_investigation_actions};quest_path={quest_path};quest_intended={quest_intended};quest_selected={quest_selected};selection_reason={}",
        settlement_id.unwrap_or("none"),
        bounded_event_field(selection_reason),
    )
}
