//! Expedition recovery, shelter, and actionable physiology policy.

use super::*;

pub(super) const MAX_EXPEDITION_RECOVERY_RESTS: u32 = 2;
pub(super) const EXPEDITION_RECOVERY_REST_MINUTES: u64 = MINUTES_PER_DAY;
pub(super) const PARTY_TENT_ITEM_ID: &str = "field_tent";
pub(super) const MIN_ACTIONABLE_PHYSIOLOGY_CONFIDENCE_BPS: u16 = 3_000;
/// Older observations can describe a materially different disease stage.
/// One strategic day permits ordinary asynchronous party observation without
/// allowing an indefinitely cached chart to direct treatment.
pub(super) const MAX_ACTIONABLE_PHYSIOLOGY_CHART_AGE_MINUTES: u64 = MINUTES_PER_DAY;
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ActivityObservation {
    pub(super) personal_gold_coin: u64,
    pub(super) condition_status: DomainIncapacitationStatus,
    pub(super) hunger: f32,
    pub(super) thirst: f32,
    pub(super) food_days: f32,
    pub(super) water_days: f32,
    pub(super) visible_food_kcal: f32,
    pub(super) visible_water_ml: f32,
    pub(super) elapsed_minutes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SettlementRestSponsor {
    pub(super) payer_id: u64,
    pub(super) payer_agent_id: u32,
    pub(super) purse: u64,
    pub(super) medical_reserve: u64,
    pub(super) spendable: u64,
    pub(super) patient_contribution: u64,
    pub(super) sponsor_quote: u64,
    pub(super) party_treasury: u64,
    pub(super) party_stake: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ExpeditionMemberObservation {
    pub(super) agent_id: u32,
    pub(super) character_id: u64,
    pub(super) alive: bool,
    pub(super) condition_status: Option<DomainIncapacitationStatus>,
    pub(super) hunger: f32,
    pub(super) thirst: f32,
    pub(super) food_days: f32,
    pub(super) water_days: f32,
    pub(super) thermal: f32,
    pub(super) wetness_bps: u16,
    pub(super) thermal_strain: i32,
    pub(super) ammunition: u32,
    pub(super) carried_load_kg: f32,
    pub(super) carry_capacity_kg: f32,
    pub(super) encumbrance_remaining_bps: u32,
    pub(super) equipment_ready: bool,
    pub(super) party_tent_quantity: u32,
    pub(super) symptomatic: bool,
    pub(super) critical: bool,
    pub(super) elapsed_minutes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ExpeditionSuppliesObservation {
    pub(super) stored_food_kcal: f32,
    pub(super) portable_water_ml: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ExpeditionDiagnosticContext<'a> {
    pub(super) party_id: &'a str,
    pub(super) phase: &'a str,
    pub(super) action: &'a str,
    pub(super) reason: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ExpeditionObservationChange<'a> {
    pub(super) members_before: &'a [ExpeditionMemberObservation],
    pub(super) members_after: &'a [ExpeditionMemberObservation],
    pub(super) supplies_before: ExpeditionSuppliesObservation,
    pub(super) supplies_after: ExpeditionSuppliesObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpeditionRecoveryOutcome {
    None,
    Resumed,
    Returned,
    Evacuated,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum JourneyTravelOutcome {
    Completed,
    DeferredForDaylightWindow,
    HeldNoActionableActor,
    HeldForRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActionableRecoveryRestActor {
    pub(super) character_id: u64,
    pub(super) agent_id: u32,
    pub(super) role: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PassiveNoActionableRestActor {
    pub(super) leader_id: u64,
    pub(super) agent_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExpeditionRecoveryRestActor {
    Actionable(ActionableRecoveryRestActor),
    PassiveNoActionable(PassiveNoActionableRestActor),
}

impl ExpeditionRecoveryRestActor {
    pub(super) fn character_id(self) -> u64 {
        match self {
            Self::Actionable(actor) => actor.character_id,
            Self::PassiveNoActionable(actor) => actor.leader_id,
        }
    }

    pub(super) fn agent_id(self) -> u32 {
        match self {
            Self::Actionable(actor) => actor.agent_id,
            Self::PassiveNoActionable(actor) => actor.agent_id,
        }
    }

    pub(super) fn role(self) -> &'static str {
        match self {
            Self::Actionable(actor) => actor.role,
            Self::PassiveNoActionable(_) => "passive_no_actionable_rest",
        }
    }

    pub(super) fn is_passive(self) -> bool {
        matches!(self, Self::PassiveNoActionable(_))
    }
}

pub(super) fn expedition_member_needs_recovery(member: &ExpeditionMemberObservation) -> bool {
    member.alive
        && (member.condition_status != Some(DomainIncapacitationStatus::Ready)
            || member.symptomatic
            || member.critical)
}

pub(super) fn public_journey_endpoint(endpoint: &JourneyEndpoint) -> String {
    match endpoint {
        JourneyEndpoint::Settlement(settlement) => format!("settlement:{}", settlement.id),
        JourneyEndpoint::CaseSite(site) => format!("case_site:{}", site.id.value),
        JourneyEndpoint::Camp(camp) => format!("camp:{}", bounded_event_field(camp)),
    }
}

pub(super) fn expedition_party_can_resume(members: &[ExpeditionMemberObservation]) -> bool {
    let living = members
        .iter()
        .filter(|member| member.alive)
        .collect::<Vec<_>>();
    !living.is_empty()
        && living.iter().any(|member| {
            member.condition_status == Some(DomainIncapacitationStatus::Ready)
                && !member.symptomatic
                && !member.critical
        })
        && living
            .iter()
            .all(|member| !expedition_member_needs_recovery(member))
}

pub(super) fn expedition_supplies_cover_one_rest_day(
    members: &[ExpeditionMemberObservation],
    supplies: ExpeditionSuppliesObservation,
) -> bool {
    let living = members.iter().filter(|member| member.alive).count() as f32;
    living > 0.0
        && supplies.stored_food_kcal
            >= living * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
        && supplies.portable_water_ml
            >= living * adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY
}

pub(super) fn observed_activity_return_origin(
    observations: &HashMap<(String, String), String>,
    party_id: &str,
    current_case_site_id: Option<&str>,
) -> Option<String> {
    let site_id = current_case_site_id?;
    observations
        .get(&(party_id.to_owned(), site_id.to_owned()))
        .cloned()
}

pub(super) fn passive_no_actionable_rest_allowed(
    members: &[ExpeditionMemberObservation],
    supplies: ExpeditionSuppliesObservation,
    off_settlement: bool,
    persisted_camp_journey: bool,
    leader_id: u64,
    actionable_actor_exists: bool,
) -> bool {
    let living = members
        .iter()
        .filter(|member| member.alive)
        .collect::<Vec<_>>();
    off_settlement
        && persisted_camp_journey
        && !actionable_actor_exists
        && !living.is_empty()
        && living.iter().any(|member| member.character_id == leader_id)
        && living
            .iter()
            .all(|member| member.condition_status.is_some() && !member.critical)
        && expedition_supplies_cover_one_rest_day(members, supplies)
}

pub(super) fn expedition_elapsed_delta(
    before: &[ExpeditionMemberObservation],
    after: &[ExpeditionMemberObservation],
) -> u64 {
    let before_max = before
        .iter()
        .map(|member| member.elapsed_minutes)
        .max()
        .unwrap_or(0);
    let after_max = after
        .iter()
        .map(|member| member.elapsed_minutes)
        .max()
        .unwrap_or(before_max);
    after_max.saturating_sub(before_max)
}
