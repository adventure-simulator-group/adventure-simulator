use crate::{ActivityPreference, AgentProfile, EquipmentStyle, generate_profile};
use adventuresim_core::simulation_security::{
    SIM_BOOTSTRAP_TOKEN_ENV as BOOTSTRAP_TOKEN_ENV,
    SIM_BOOTSTRAP_TOKEN_HEX_LEN as BOOTSTRAP_TOKEN_HEX_LEN,
};
use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table};
use adventuresim_stdb_client::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

use adventuresim_core::strategic_currency::is_currency_id;
use url::Url;

use adventuresim_stdb_client::{
    abandon_contract_reducer::abandon_contract, accept_contract_reducer::accept_contract,
    accept_party_join_request_reducer::accept_party_join_request,
    administer_preparation_reducer::administer_preparation,
    advance_simulation_world_time_reducer::advance_simulation_world_time,
    autoresolve_mission_reducer::autoresolve_mission,
    autoresolve_report_table::AutoresolveReportTableAccess,
    backend_case_battles_table::BackendCaseBattlesTableAccess,
    backend_case_site_pins_table::BackendCaseSitePinsTableAccess,
    backend_contract_type::BackendContract, backend_contracts_table::BackendContractsTableAccess,
    backend_dialogue_sessions_table::BackendDialogueSessionsTableAccess,
    backend_dialogue_topic_options_table::BackendDialogueTopicOptionsTableAccess,
    backend_investigation_action_outcomes_table::BackendInvestigationActionOutcomesTableAccess,
    backend_investigation_actions_table::BackendInvestigationActionsTableAccess,
    backend_investigation_cases_table::BackendInvestigationCasesTableAccess,
    backend_investigation_leads_table::BackendInvestigationLeadsTableAccess,
    backend_local_problem_trade_effects_table::BackendLocalProblemTradeEffectsTableAccess,
    backend_settlement_residents_table::BackendSettlementResidentsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_death_table::CharacterDeathTableAccess,
    character_equipped_item_table::CharacterEquippedItemTableAccess,
    character_illness_status_table::CharacterIllnessStatusTableAccess,
    character_needs_table::CharacterNeedsTableAccess,
    character_strategic_condition_table::CharacterStrategicConditionTableAccess,
    character_table::CharacterTableAccess, character_time_table::CharacterTimeTableAccess,
    character_training_schedule_table::CharacterTrainingScheduleTableAccess,
    choose_dialogue_topic_reducer::choose_dialogue_topic,
    claim_simulation_run_reducer::claim_simulation_run,
    configure_simulation_character_reducer::configure_simulation_character,
    continue_camp_travel_reducer::continue_camp_travel,
    contract_interaction_stage_type::ContractInteractionStage,
    contract_status_type::ContractStatus,
    create_named_character_with_id_reducer::create_named_character_with_id,
    ensure_settlement_activity_reducer::ensure_settlement_activity,
    equip_item_at_placement_reducer::equip_item_at_placement, equip_item_reducer::equip_item,
    equipment_occupancy_table::EquipmentOccupancyTableAccess, field_shelter_type::FieldShelter,
    finalize_merchant_trade_reducer::finalize_merchant_trade, food_lot_table::FoodLotTableAccess,
    inventory_item_table::InventoryItemTableAccess, item_condition_table::ItemConditionTableAccess,
    item_table::ItemTableAccess, liquidate_party_inventory_reducer::liquidate_party_inventory,
    local_problem_symptom_table::LocalProblemSymptomTableAccess,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess,
    party_journey_itinerary_table::PartyJourneyItineraryTableAccess,
    party_journey_table::PartyJourneyTableAccess, party_member_table::PartyMemberTableAccess,
    party_stake_table::PartyStakeTableAccess, party_table::PartyTableAccess,
    perform_investigation_action_reducer::perform_investigation_action,
    purchase_from_herbalist_reducer::purchase_from_herbalist,
    register_strategic_gateway_reducer::register_strategic_gateway,
    repair_order_table::RepairOrderTableAccess, report_contract_reducer::report_contract,
    request_general_party_join_reducer::request_general_party_join,
    resolve_strategic_encounter_reducer::resolve_strategic_encounter,
    rest_at_camp_reducer::rest_at_camp, rest_at_settlement_hours_reducer::rest_at_settlement_hours,
    retrieve_repaired_item_reducer::retrieve_repaired_item,
    seed_simulation_disease_reducer::seed_simulation_disease,
    seed_simulation_equipment_damage_reducer::seed_simulation_equipment_damage,
    seed_simulation_world_reducer::seed_simulation_world,
    settlement_resident_presence_table::SettlementResidentPresenceTableAccess,
    settlement_service_type::SettlementService, settlement_smith_table::SettlementSmithTableAccess,
    settlement_table::SettlementTableAccess,
    simulate_contract_issuer_interaction_reducer::simulate_contract_issuer_interaction,
    simulation_run_table::SimulationRunTableAccess,
    sponsor_party_member_inn_rest_reducer::sponsor_party_member_inn_rest,
    start_dialogue_reducer::start_dialogue, store_battle_loot_reducer::store_battle_loot,
    strategic_encounter_table::StrategicEncounterTableAccess,
    submit_item_for_repair_reducer::submit_item_for_repair,
    travel_to_case_site_reducer::travel_to_case_site,
    travel_to_settlement_reducer::travel_to_settlement,
    update_training_schedule_reducer::update_training_schedule,
    withdraw_party_inventory_item_reducer::withdraw_party_inventory_item,
    world_clock_table::WorldClockTableAccess, world_data_import_table::WorldDataImportTableAccess,
};

const ACTION_TIMEOUT: Duration = Duration::from_secs(20);
/// Severe but non-incapacitating injuries can reduce overland pace enough for
/// a long quest leg to require many daily camps.
const MAX_CAMPS_PER_LEG: u32 = 512;
const MAX_DEFEAT_RETRIES: u32 = 2;
/// Long but survivable illnesses and injuries can require many daily rests;
/// keep the policy bounded well beyond ordinary convalescence.
const MAX_RECOVERY_ACTIONS: u32 = 128;
const MAX_CORE_LOOP_WORK: u64 = 100_000;
const MAX_CORE_TRACE_EVENTS: usize = 100_000;
const MAX_GENERATED_CASE_STEPS_PER_CYCLE: u32 = 16;
const MAX_EXPEDITION_RECOVERY_RESTS: u32 = 2;
const EXPEDITION_RECOVERY_REST_MINUTES: u64 = 1_440;
const TRAVEL_PROVISION_RESERVE_DAYS: f32 = 1.0;
const MAX_TRAVEL_PROVISION_UNITS_PER_ITEM: u32 = 512;
const MAX_PUBLIC_JOURNEY_DIAGNOSTIC_MINUTES: u64 = u32::MAX as u64;
const MAX_PUBLIC_JOURNEY_DIAGNOSTIC_INTERVALS: usize = MAX_CAMPS_PER_LEG as usize;
/// Public fail-safe bound for a disclosed one-way distance. The ordinary
/// daylight projection covers schedule downtime; four times that projection
/// covers fatigue-expanded outbound travel plus the return leg. The separate
/// reserve day remains available for delays and encounters.
const JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR: u64 = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopConfig {
    pub host: String,
    pub database: String,
    pub seed: u64,
    pub population: u32,
    pub cycles: u32,
    pub duration_days: u32,
    pub party_size: u32,
    pub run_nonce: String,
    pub use_imported_world: bool,
    pub expected_world_manifest_digest: Option<String>,
    /// Immutable, public-safe diagnostic artifact written if the run fails.
    pub failure_output: Option<PathBuf>,
}

impl CoreLoopConfig {
    pub fn validate(&self) -> Result<(), String> {
        validate_loopback_url(&self.host)?;
        if !self.database.starts_with("adventuresim-sim-")
            || !self
                .database
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("database must be a unique adventuresim-sim-* disposable name".into());
        }
        if !(2..=32).contains(&self.population)
            || !(1..=10_000).contains(&self.cycles)
            || !(1..=36_500).contains(&self.duration_days)
            || !(2..=8).contains(&self.party_size)
            || self.party_size > self.population
        {
            return Err("population 2..=32, party_size 2..=8, cycles 1..=10000, and duration_days 1..=36500 are required".into());
        }
        let work = u64::from(self.population)
            .checked_mul(u64::from(self.cycles))
            .ok_or("core-loop work overflow")?;
        if work > MAX_CORE_LOOP_WORK {
            return Err(format!(
                "population * cycles must be <= {MAX_CORE_LOOP_WORK}"
            ));
        }
        if !(16..=96).contains(&self.run_nonce.len())
            || !self
                .run_nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("run_nonce must be 16..=96 ASCII alphanumeric/dash characters".into());
        }
        if self.use_imported_world {
            let digest = self
                .expected_world_manifest_digest
                .as_deref()
                .ok_or("imported-world mode requires an expected manifest digest")?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err("expected world manifest digest must be 64 lowercase hex".into());
            }
        } else if self.expected_world_manifest_digest.is_some() {
            return Err("fixture mode cannot claim an expected world manifest".into());
        }
        Ok(())
    }
}

fn validate_loopback_url(host: &str) -> Result<(), String> {
    let parsed = Url::parse(host).map_err(|error| format!("invalid SpacetimeDB URL: {error}"))?;
    if parsed.scheme() != "http"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
    {
        return Err("host must be a credential-free http://localhost, 127.0.0.1, or [::1] origin with no path/query/fragment".into());
    }
    Ok(())
}

fn bootstrap_token_from_environment(value: Option<String>) -> Result<String, String> {
    let token = value.ok_or_else(|| {
        format!(
            "{BOOTSTRAP_TOKEN_ENV} is required; use the disposable strategic-sim-core-loop recipe"
        )
    })?;
    if token.len() != BOOTSTRAP_TOKEN_HEX_LEN || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{BOOTSTRAP_TOKEN_ENV} must contain exactly 32 random bytes encoded as hexadecimal"
        ));
    }
    Ok(token)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopMetrics {
    pub parties_formed: u32,
    pub joins_requested: u32,
    pub joins_accepted: u32,
    pub quests_attempted: u32,
    pub quests_completed: u32,
    pub direct_contracts_attempted: u32,
    pub direct_contracts_completed: u32,
    pub generated_case_intakes: u32,
    pub generated_case_continuations: u32,
    pub generated_quests_discovered: u32,
    pub generated_quests_completed: u32,
    pub generated_quests_closed_externally: u32,
    pub generated_investigation_actions: u32,
    pub generated_investigation_waits: u32,
    pub generated_investigation_wait_minutes: u64,
    pub generated_investigation_replans: u32,
    pub generated_witness_dialogues: u32,
    pub generated_discovery_actions_attempted: u32,
    pub generated_discovery_actions_fruitful: u32,
    pub generated_discovery_decisions_unproductive: u32,
    pub generated_discovery_public_backoff_suppressions: u32,
    pub expedition_recovery_plans: u32,
    pub expedition_recovery_rests: u32,
    pub expedition_evacuations: u32,
    pub expedition_resumes: u32,
    pub expedition_holds: u32,
    pub expedition_passive_rest_attempts: u32,
    pub expedition_passive_rest_minutes: u64,
    pub generated_unique_party_cases_discovered: u32,
    pub generated_exact_site_ready: u32,
    pub generated_finance_blocked_cycles: u32,
    pub generated_case_site_traveled: u32,
    pub journey_provision_purchases: u32,
    pub journey_provision_party_gold_spent: u64,
    pub defeats: u32,
    pub recovery_rests: u32,
    pub travel_legs: u32,
    pub camp_stops: u32,
    pub loot_items: u32,
    pub loot_value: u64,
    pub sale_proceeds: u64,
    pub equipment_purchases: u32,
    pub equipment_upgrades: u32,
    pub repair_submissions: u32,
    pub repair_retrievals: u32,
    pub repair_wait_minutes: u64,
    pub preparations_purchased: u32,
    pub interventions_administered: u32,
    pub treatment_gold_spent: u64,
    pub treatment_rest_minutes: u64,
    pub sponsored_settlement_rests: u32,
    pub sponsored_settlement_rest_gold_spent: u64,
    pub sponsored_settlement_rest_requested_minutes: u64,
    pub sponsored_settlement_rest_elapsed_minutes: u64,
    pub illness_recoveries: u32,
    pub disease_deaths: u32,
    pub quests_suppressed_for_health: u32,
    pub earned_gold_withdrawn: u64,
    pub activity_days: u32,
    pub reducer_failures: u32,
    pub retries: u32,
    pub duplicate_semantic_events: u32,
    pub stuck_detections: u32,
    pub encounters: u32,
    pub encounter_sneaks: u32,
    pub encounter_detours: u32,
    pub encounter_attacks: u32,
    pub encounter_runs: u32,
    pub encounter_surrenders: u32,
    pub encounter_escape_eligible: u32,
    pub encounter_escape_ineligible: u32,
    pub encounter_surrender_items_lost: u32,
    pub encounter_surrender_value_lost: u64,
    pub encounter_defeats: u32,
    pub encounter_wipes: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreLoopEventKind {
    FormParty,
    RequestJoin,
    AcceptJoin,
    AcceptContract,
    Travel,
    Camp,
    AutoresolveVictory,
    AutoresolveDefeat,
    AbandonQuest,
    Recover,
    StoreLoot,
    TurnIn,
    Liquidate,
    Purchase,
    Equip,
    SubmitRepair,
    RetrieveRepair,
    WaitForRepair,
    MedicalDecision,
    BuyMedication,
    AdministerPreparation,
    IllnessRecovered,
    QuestSuppressed,
    Death,
    QuestDecision,
    GeneratedDiscoveryAttempt,
    GeneratedDiscoveryResult,
    GeneratedCaseIntake,
    ExpeditionRecovery,
    GeneratedQuestDiscovered,
    GeneratedInvestigationAttempt,
    GeneratedInvestigationAction,
    GeneratedInvestigationWait,
    GeneratedInvestigationReplan,
    GeneratedWitnessDialogue,
    GeneratedQuestCompleted,
    GeneratedQuestClosedExternally,
    Activity,
    Encounter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopEvent {
    pub sequence: u64,
    pub agent_id: u32,
    pub kind: CoreLoopEventKind,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalAgentState {
    pub agent_id: u32,
    pub character_id: u64,
    pub gold: u32,
    pub equipment_item_ids: Vec<String>,
    pub capability_summary: String,
    pub condition_status: String,
    pub worst_equipment_condition: f32,
    pub outstanding_repair_orders: u32,
    pub alive: bool,
    pub elapsed_minutes: u64,
    pub personal_gold_coin: u64,
    pub party_treasury: u64,
    pub party_stake: u64,
    pub hunger: f32,
    pub thirst: f32,
    pub food_days: f32,
    pub water_days: f32,
    pub visible_food_kcal: f32,
    pub visible_water_ml: f32,
    pub settlement_id: Option<String>,
    pub current_case_site_id: Option<String>,
    pub journey_destination: Option<String>,
    pub symptomatic: bool,
    pub critical: bool,
    pub settlement_services: Vec<String>,
    pub visible_herbalist_quote: Option<u64>,
    pub visible_inn_full_board_cost: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopReport {
    pub backend_kind: String,
    pub seed: u64,
    pub server_origin: String,
    pub database: String,
    pub run_nonce: String,
    pub deployment_identity_note: String,
    pub world_artifact_id: Option<String>,
    pub world_manifest_digest: Option<String>,
    pub starting_settlement_id: String,
    pub profiles: Vec<AgentProfile>,
    pub metrics: CoreLoopMetrics,
    pub trace: Vec<CoreLoopEvent>,
    pub trace_truncated: bool,
    pub total_event_count: u64,
    pub final_agents: Vec<FinalAgentState>,
    pub elapsed_game_minutes: u64,
    pub policy_seed_note: String,
}

const MAX_FAILURE_TRACE_EVENTS: usize = 64;
const CORE_LOOP_FAILURE_SCHEMA_VERSION: u32 = 5;
const MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES: u32 = 1_440;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopFailureAgent {
    pub agent_id: u32,
    pub character_id: u64,
    pub alive: bool,
    pub condition_status: String,
    pub hunger: f32,
    pub thirst: f32,
    pub food_days: f32,
    pub water_days: f32,
    pub visible_food_kcal: f32,
    pub visible_water_ml: f32,
    pub personal_gold_coin: u64,
    pub settlement_id: Option<String>,
    pub current_case_site_id: Option<String>,
    pub journey_destination: Option<String>,
    pub symptomatic: bool,
    pub critical: bool,
    pub settlement_services: Vec<String>,
    pub visible_herbalist_quote: Option<u64>,
    pub visible_inn_full_board_cost: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopFailureArtifact {
    pub schema_version: u32,
    pub category: String,
    pub message: String,
    pub operation: Option<String>,
    pub reason_code: String,
    pub metrics: CoreLoopMetrics,
    pub total_event_count: u64,
    pub trace_truncated: bool,
    pub trace: Vec<CoreLoopEvent>,
    pub final_agents: Vec<CoreLoopFailureAgent>,
}

#[derive(Clone, Debug, Default)]
struct FailureDraft {
    metrics: CoreLoopMetrics,
    total_event_count: u64,
    trace_truncated: bool,
    trace: Vec<CoreLoopEvent>,
    final_agents: Vec<CoreLoopFailureAgent>,
}

#[derive(Clone, Debug, PartialEq)]
struct ActivityObservation {
    personal_gold_coin: u64,
    condition_status: String,
    hunger: f32,
    thirst: f32,
    food_days: f32,
    water_days: f32,
    visible_food_kcal: f32,
    visible_water_ml: f32,
    elapsed_minutes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettlementRestSponsor {
    payer_id: u64,
    payer_agent_id: u32,
    purse: u64,
    medical_reserve: u64,
    spendable: u64,
    patient_contribution: u64,
    sponsor_quote: u64,
    party_treasury: u64,
    party_stake: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct ExpeditionMemberObservation {
    agent_id: u32,
    character_id: u64,
    alive: bool,
    condition_status: String,
    hunger: f32,
    thirst: f32,
    food_days: f32,
    water_days: f32,
    symptomatic: bool,
    critical: bool,
    elapsed_minutes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ExpeditionSuppliesObservation {
    stored_food_kcal: f32,
    portable_water_ml: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpeditionRecoveryOutcome {
    None,
    Resumed,
    Evacuated,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JourneyTravelOutcome {
    Completed,
    HeldNoActionableActor,
    HeldForRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActionableRecoveryRestActor {
    character_id: u64,
    agent_id: u32,
    role: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PassiveNoActionableRestActor {
    leader_id: u64,
    agent_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpeditionRecoveryRestActor {
    Actionable(ActionableRecoveryRestActor),
    PassiveNoActionable(PassiveNoActionableRestActor),
}

impl ExpeditionRecoveryRestActor {
    fn character_id(self) -> u64 {
        match self {
            Self::Actionable(actor) => actor.character_id,
            Self::PassiveNoActionable(actor) => actor.leader_id,
        }
    }

    fn agent_id(self) -> u32 {
        match self {
            Self::Actionable(actor) => actor.agent_id,
            Self::PassiveNoActionable(actor) => actor.agent_id,
        }
    }

    fn role(self) -> &'static str {
        match self {
            Self::Actionable(actor) => actor.role,
            Self::PassiveNoActionable(_) => "passive_no_actionable_rest",
        }
    }

    fn is_passive(self) -> bool {
        matches!(self, Self::PassiveNoActionable(_))
    }
}

fn expedition_member_needs_recovery(member: &ExpeditionMemberObservation) -> bool {
    member.alive && (member.condition_status != "ready" || member.symptomatic || member.critical)
}

fn public_journey_endpoint(endpoint: &JourneyEndpoint) -> String {
    match endpoint {
        JourneyEndpoint::Settlement(settlement) => format!("settlement:{}", settlement.id),
        JourneyEndpoint::CaseSite(site) => format!("case_site:{}", site.id.value),
        JourneyEndpoint::Camp(camp) => format!("camp:{}", bounded_event_field(camp)),
    }
}

fn expedition_party_can_resume(members: &[ExpeditionMemberObservation]) -> bool {
    let living = members
        .iter()
        .filter(|member| member.alive)
        .collect::<Vec<_>>();
    !living.is_empty()
        && living.iter().any(|member| {
            member.condition_status == "ready" && !member.symptomatic && !member.critical
        })
        && living
            .iter()
            .all(|member| !expedition_member_needs_recovery(member))
}

fn expedition_supplies_cover_one_rest_day(
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

fn passive_no_actionable_rest_allowed(
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
        && living.iter().all(|member| {
            matches!(
                member.condition_status.as_str(),
                "ready" | "staggered" | "incapacitated"
            ) && !member.critical
        })
        && expedition_supplies_cover_one_rest_day(members, supplies)
}

fn expedition_elapsed_delta(
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicActiveCampObservation {
    completed_elapsed_minutes: u64,
    total_elapsed_minutes: u64,
    active_interval_start: u64,
    active_interval_minutes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicPostEncounterJourneyState {
    unresolved_encounter: bool,
    active_destination: bool,
    journey_count: usize,
    itinerary_count: usize,
    destination_matches: bool,
    active_interval_count: usize,
    actionable_actor: bool,
    unsafe_member_count: usize,
    evacuation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostEncounterJourneyAction {
    ReclassifyPublicState,
    HoldNoActionableActor,
    HoldForRecovery,
    HandleActiveCamp,
    ContinueTravel,
}

fn classify_post_encounter_journey(
    state: PublicPostEncounterJourneyState,
) -> Result<PostEncounterJourneyAction, &'static str> {
    if state.unresolved_encounter || !state.active_destination {
        return Ok(PostEncounterJourneyAction::ReclassifyPublicState);
    }
    if state.journey_count != 1 || state.itinerary_count != 1 || !state.destination_matches {
        return Err("post_encounter_journey_projection_mismatch");
    }
    if !state.actionable_actor {
        return Ok(PostEncounterJourneyAction::HoldNoActionableActor);
    }
    if state.unsafe_member_count > 0 && !state.evacuation {
        return Ok(PostEncounterJourneyAction::HoldForRecovery);
    }
    match state.active_interval_count {
        0 => Ok(PostEncounterJourneyAction::ContinueTravel),
        1 => Ok(PostEncounterJourneyAction::HandleActiveCamp),
        _ => Err("post_encounter_overlapping_active_camps"),
    }
}

fn select_expedition_encounter_choice(
    available_choices: &[String],
    roll_index: u64,
    evacuation: bool,
) -> Option<String> {
    let eligible = available_choices
        .iter()
        .filter(|choice| !evacuation || choice.as_str() != "attack")
        .collect::<Vec<_>>();
    (!eligible.is_empty()).then(|| (*eligible[(roll_index as usize) % eligible.len()]).clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettlementActivityVenue {
    Inn,
    Temple,
}

impl SettlementActivityVenue {
    fn at_inn(self) -> bool {
        matches!(self, Self::Inn)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Inn => "inn",
            Self::Temple => "temple",
        }
    }
}

fn select_settlement_activity_venue(
    inn_available: bool,
    temple_available: bool,
    temple_food_covers_day: bool,
    purse: u64,
    committed_reserve: u64,
    inn_cost: Option<u64>,
) -> Option<SettlementActivityVenue> {
    if temple_available && temple_food_covers_day {
        return Some(SettlementActivityVenue::Temple);
    }
    if inn_available && inn_cost.is_some_and(|cost| purse >= committed_reserve.saturating_add(cost))
    {
        return Some(SettlementActivityVenue::Inn);
    }
    temple_available.then_some(SettlementActivityVenue::Temple)
}

fn visible_activity_committed_reserve(
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

fn format_quest_decision_detail(
    cycle: u32,
    wants_quest: bool,
    selector: f64,
    quest_propensity: f32,
    settlement_id: Option<&str>,
    offered_contracts: usize,
    open_generated_cases: usize,
    projected_investigation_actions: usize,
    quest_path: &str,
    quest_intended: bool,
    quest_selected: bool,
    selection_reason: &str,
) -> String {
    format!(
        "cycle={cycle};wants_quest={wants_quest};selector={selector:.6};quest_propensity={quest_propensity:.6};settlement={};offered_contracts={offered_contracts};open_generated_cases={open_generated_cases};projected_investigation_actions={projected_investigation_actions};quest_path={quest_path};quest_intended={quest_intended};quest_selected={quest_selected};selection_reason={}",
        settlement_id.unwrap_or("none"),
        bounded_event_field(selection_reason),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicNpcCandidate {
    resident_character_id: u64,
    name: String,
    profession: String,
    conversation_id: String,
    location_id: String,
}

const PUBLIC_DISCOVERY_BACKOFF_MINUTES: u64 = 2 * 1_440;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicDiscoveryFingerprint {
    settlement_id: String,
    contacts: Vec<(u64, String, String)>,
    active_symptoms: Vec<(String, String, u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicDiscoveryBackoff {
    fingerprint: PublicDiscoveryFingerprint,
    retry_at: u64,
}

fn public_discovery_backoff_active(
    backoff: &PublicDiscoveryBackoff,
    fingerprint: &PublicDiscoveryFingerprint,
    official_minute: u64,
) -> bool {
    backoff.fingerprint == *fingerprint && official_minute < backoff.retry_at
}

fn public_symptom_age_bucket(oldest_age_minutes: Option<u64>) -> &'static str {
    match oldest_age_minutes {
        None => "none",
        Some(age) if age < 1_440 => "under_1_day",
        Some(age) if age < 4_320 => "1_to_2_days",
        Some(age) if age < 11_520 => "3_to_7_days",
        Some(_) => "8_plus_days",
    }
}

fn public_count_bucket(count: usize) -> &'static str {
    match count {
        0 => "0",
        1 => "1",
        2..=3 => "2_to_3",
        _ => "4_plus",
    }
}

fn discovery_location_class(candidate: Option<&PublicNpcCandidate>) -> &'static str {
    match candidate.map(|candidate| candidate.location_id.as_str()) {
        Some("inn") => "inn",
        Some("overview") => "overview",
        Some(_) => "other",
        None => "none",
    }
}

fn stable_discovery_action_candidate(
    candidates: Vec<PublicNpcCandidate>,
) -> Option<PublicNpcCandidate> {
    let mut candidates = stable_public_npc_candidates(candidates, None, Some("inn"));
    if candidates
        .iter()
        .any(|candidate| candidate.location_id == "inn")
    {
        candidates.retain(|candidate| candidate.location_id == "inn");
    } else {
        candidates.retain(|candidate| candidate.location_id == "overview");
    }
    candidates.into_iter().next()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedDiscoveryOutcome {
    Discovered,
    NoVisibleContacts,
    NoPublicRumor,
    PublicBackoff,
}

impl GeneratedDiscoveryOutcome {
    fn case_discovered(self) -> bool {
        self == Self::Discovered
    }
}

fn npc_is_publicly_present(start_minute: u16, end_minute: u16, minute: u64) -> bool {
    let minute = minute % 1_440;
    let start = u64::from(start_minute);
    let end = u64::from(end_minute);
    start != end
        && if start < end {
            start <= minute && minute < end
        } else {
            minute >= start || minute < end
        }
}

fn stable_public_npc_candidates(
    mut candidates: Vec<PublicNpcCandidate>,
    preferred_name: Option<&str>,
    preferred_location: Option<&str>,
) -> Vec<PublicNpcCandidate> {
    candidates.sort_by_key(|candidate| {
        (
            !preferred_name.is_some_and(|name| candidate.name.eq_ignore_ascii_case(name)),
            !preferred_location.is_some_and(|location| candidate.location_id == location),
            candidate.location_id != "inn",
            candidate.name.to_ascii_lowercase(),
            candidate.profession.to_ascii_lowercase(),
            candidate.resident_character_id.clone(),
        )
    });
    candidates
}

fn stable_owned_open_cases(
    owner_character_id: u64,
    rows: impl IntoIterator<Item = (u64, String, String, String)>,
) -> Vec<(String, String)> {
    let mut cases = rows
        .into_iter()
        .filter(|(owner, _, _, status)| *owner == owner_character_id && status == "open")
        .map(|(_, case_id, title, _)| (case_id, title))
        .collect::<Vec<_>>();
    cases.sort();
    cases
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedClosureAttribution {
    StillOpen,
    OwnImmediateTransition,
    ExternalTransition,
}

fn generated_closure_attribution(
    before_status: &str,
    after_status: Option<&str>,
    immediately_after_own_action: bool,
) -> GeneratedClosureAttribution {
    if before_status == "open" && after_status == Some("completed") {
        if immediately_after_own_action {
            GeneratedClosureAttribution::OwnImmediateTransition
        } else {
            GeneratedClosureAttribution::ExternalTransition
        }
    } else {
        GeneratedClosureAttribution::StillOpen
    }
}

fn projected_case_row_matches(
    owner_character_id: u64,
    selected_case_id: &str,
    row_owner_character_id: u64,
    row_public_case_id: &str,
) -> bool {
    row_owner_character_id == owner_character_id && row_public_case_id == selected_case_id
}

fn occupied_case_pin_matches(
    owner_character_id: u64,
    selected_case_id: &str,
    occupied_site_id: &str,
    pin_owner_character_id: u64,
    pin_public_case_id: &str,
    pin_site_id: &str,
) -> bool {
    projected_case_row_matches(
        owner_character_id,
        selected_case_id,
        pin_owner_character_id,
        pin_public_case_id,
    ) && pin_site_id == occupied_site_id
}

fn generated_actor_can_continue(
    owner_character_id: u64,
    current_leader_id: Option<u64>,
    unsafe_party_members: usize,
) -> bool {
    current_leader_id == Some(owner_character_id) && unsafe_party_members == 0
}

fn projected_investigation_wait_minutes(reason_code: &str, wait_minutes: u32) -> Option<u32> {
    (reason_code == "night_window"
        && (1..=MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES).contains(&wait_minutes))
    .then_some(wait_minutes)
}

fn projected_case_site_journey_minutes(
    distance_m: u64,
    walking_minutes_per_day: u16,
) -> Option<u64> {
    if distance_m == 0 || walking_minutes_per_day == 0 || walking_minutes_per_day > 1_440 {
        return None;
    }
    let movement_minutes = ((distance_m as f64 / 1_250.0) * 60.0).ceil() as u64;
    let walking_minutes = u64::from(walking_minutes_per_day);
    let completed_walking_days = movement_minutes.saturating_sub(1) / walking_minutes;
    Some(
        movement_minutes
            .saturating_add(
                completed_walking_days.saturating_mul(1_440_u64.saturating_sub(walking_minutes)),
            )
            .saturating_mul(JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR),
    )
}

fn projected_camp_rest_minutes(
    completed_elapsed_minutes: u64,
    total_elapsed_minutes: u64,
    intervals: &[JourneyCampInterval],
) -> Option<(u64, u64)> {
    if completed_elapsed_minutes >= total_elapsed_minutes {
        return None;
    }
    let mut active = intervals.iter().filter_map(|camp| {
        let camp_start = camp.elapsed_start_minute.max(completed_elapsed_minutes);
        let camp_end = camp
            .elapsed_start_minute
            .saturating_add(camp.elapsed_minutes)
            .min(total_elapsed_minutes);
        (camp.elapsed_start_minute <= completed_elapsed_minutes && camp_end > camp_start)
            .then(|| (camp_start, camp_end - camp_start))
    });
    let result = active.next()?;
    active.next().is_none().then_some(result)
}

fn projected_active_camp_interval_count(
    completed_elapsed_minutes: u64,
    total_elapsed_minutes: u64,
    intervals: &[JourneyCampInterval],
) -> usize {
    if completed_elapsed_minutes >= total_elapsed_minutes {
        return 0;
    }
    intervals
        .iter()
        .filter(|camp| {
            let camp_start = camp.elapsed_start_minute.max(completed_elapsed_minutes);
            let camp_end = camp
                .elapsed_start_minute
                .saturating_add(camp.elapsed_minutes)
                .min(total_elapsed_minutes);
            camp.elapsed_start_minute <= completed_elapsed_minutes && camp_end > camp_start
        })
        .count()
}

fn bounded_public_journey_diagnostic(value: u64) -> u64 {
    value.min(MAX_PUBLIC_JOURNEY_DIAGNOSTIC_MINUTES)
}

fn bounded_public_forecast_count(value: usize) -> usize {
    value.min(MAX_PUBLIC_JOURNEY_DIAGNOSTIC_INTERVALS)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TravelProvisionDecision {
    Ready,
    Deferred(&'static str),
}

fn signed_delta(after: u64, before: u64) -> String {
    if after >= before {
        format!("+{}", after - before)
    } else {
        format!("-{}", before - after)
    }
}

fn signed_float_delta(after: f32, before: f32) -> String {
    format!("{:+.3}", after - before)
}

fn bounded_event_field(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else if character == ';' {
                ','
            } else {
                character
            }
        })
        .take(240)
        .collect()
}

fn format_activity_detail(
    preferred_activity: &str,
    effective_activity: &str,
    schedule: &ScheduleAllocation,
    venue: SettlementActivityVenue,
    fallback_reason: &str,
    committed_reserve: u64,
    before: &ActivityObservation,
    after: &ActivityObservation,
) -> String {
    format!(
        "outcome=completed;preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue={};committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};purse_before={};purse_after={};purse_delta={};condition_before={};condition_after={};hunger_before={:.3};hunger_after={:.3};hunger_delta={};thirst_before={:.3};thirst_after={:.3};thirst_delta={};food_kcal_before={:.0};food_kcal_after={:.0};food_kcal_delta={};water_ml_before={:.0};water_ml_after={:.0};water_ml_delta={};elapsed_before={};elapsed_after={};elapsed_delta={}",
        venue.label(),
        schedule.combat_training_minutes,
        schedule.carousing_minutes,
        schedule.apprenticeship_minutes,
        schedule.profession_practice_minutes,
        schedule.labor_minutes,
        schedule.prayer_minutes,
        schedule.thievery_minutes,
        schedule.raiding_minutes,
        before.personal_gold_coin,
        after.personal_gold_coin,
        signed_delta(after.personal_gold_coin, before.personal_gold_coin),
        before.condition_status,
        after.condition_status,
        before.hunger,
        after.hunger,
        signed_float_delta(after.hunger, before.hunger),
        before.thirst,
        after.thirst,
        signed_float_delta(after.thirst, before.thirst),
        before.visible_food_kcal,
        after.visible_food_kcal,
        signed_float_delta(after.visible_food_kcal, before.visible_food_kcal),
        before.visible_water_ml,
        after.visible_water_ml,
        signed_float_delta(after.visible_water_ml, before.visible_water_ml),
        before.elapsed_minutes,
        after.elapsed_minutes,
        signed_delta(after.elapsed_minutes, before.elapsed_minutes),
    )
}

fn format_failed_activity_detail(
    preferred_activity: &str,
    effective_activity: &str,
    schedule: &ScheduleAllocation,
    venue: SettlementActivityVenue,
    fallback_reason: &str,
    committed_reserve: u64,
    before: &ActivityObservation,
    error_category: &str,
) -> String {
    format!(
        "outcome=failed;stage=rest_at_settlement;error_category={error_category};preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue={};committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};requested_minutes=1440;purse_before={};condition_before={};hunger_before={:.3};thirst_before={:.3};food_kcal_before={:.0};water_ml_before={:.0};elapsed_before={}",
        venue.label(),
        schedule.combat_training_minutes,
        schedule.carousing_minutes,
        schedule.apprenticeship_minutes,
        schedule.profession_practice_minutes,
        schedule.labor_minutes,
        schedule.prayer_minutes,
        schedule.thievery_minutes,
        schedule.raiding_minutes,
        before.personal_gold_coin,
        before.condition_status,
        before.hunger,
        before.thirst,
        before.visible_food_kcal,
        before.visible_water_ml,
        before.elapsed_minutes,
    )
}

fn event_is_repeatable(kind: &CoreLoopEventKind) -> bool {
    matches!(
        kind,
        CoreLoopEventKind::Camp
            | CoreLoopEventKind::Recover
            | CoreLoopEventKind::Travel
            | CoreLoopEventKind::AutoresolveDefeat
            | CoreLoopEventKind::QuestDecision
            | CoreLoopEventKind::GeneratedInvestigationWait
    )
}

const VICTIM_COHORT_STATE_CHANGED_DETAILS: [&str; 7] = [
    "Victim cohort authority no longer exists",
    "Victim cohort target is unavailable",
    "Victim cohort target moved from the learned location",
    "Victim cohort profile no longer matches its authority",
    "Victim cohort NPC no longer exists",
    "Victim cohort NPC no longer has a visible demographic",
    "Victim cohort target moved, changed, or is unavailable",
];

fn victim_cohort_state_changed_failure(error: &str) -> bool {
    error
        .strip_prefix("perform_investigation_action failed: ")
        .is_some_and(|detail| VICTIM_COHORT_STATE_CHANGED_DETAILS.contains(&detail))
}

#[derive(Clone)]
struct FailureRecorder {
    output: Option<PathBuf>,
    draft: std::sync::Arc<std::sync::Mutex<FailureDraft>>,
}

impl FailureRecorder {
    fn new(output: Option<PathBuf>) -> Self {
        Self {
            output,
            draft: Default::default(),
        }
    }

    fn update(&self, draft: FailureDraft) {
        if let Ok(mut current) = self.draft.lock() {
            *current = draft;
        }
    }

    fn write(&self, error: &str) -> Result<(), String> {
        let Some(path) = &self.output else {
            return Ok(());
        };
        let (category, message) = safe_core_loop_failure(error);
        let draft = self
            .draft
            .lock()
            .map_err(|_| "failure diagnostic state was unavailable".to_string())?
            .clone();
        let artifact = CoreLoopFailureArtifact {
            schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
            category: category.into(),
            message: message.into(),
            operation: safe_failure_operation(error).map(str::to_owned),
            reason_code: safe_failure_reason_code(error, category).into(),
            metrics: draft.metrics,
            total_event_count: draft.total_event_count,
            trace_truncated: draft.trace_truncated,
            trace: draft.trace,
            final_agents: draft.final_agents,
        };
        let bytes = serde_json::to_vec_pretty(&artifact).map_err(|error| error.to_string())?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        use std::io::Write as _;
        options
            .open(path)
            .and_then(|mut file| {
                file.write_all(&bytes)?;
                file.write_all(b"\n")
            })
            .map_err(|error| format!("could not write failure diagnostic: {error}"))
    }
}

fn safe_failure_operation(error: &str) -> Option<&'static str> {
    [
        "perform_investigation_action",
        "wait_for_investigation_window_settlement",
        "wait_for_investigation_window_camp",
        "travel_to_generated_case_site",
        "start_discovery_dialogue",
        "choose_dialogue_topic",
        "rest_at_camp",
        "continue_camp_travel",
        "travel_camps",
        "passive_no_actionable_rest",
        "sponsor_party_member_inn_rest",
        "purchase_journey_provisions",
    ]
    .into_iter()
    .find(|operation| {
        error.starts_with(&format!("{operation} failed:"))
            || error.starts_with(&format!("{operation} timed out"))
            || error.starts_with(&format!("could not send {operation}:"))
    })
}

fn safe_failure_reason_code(error: &str, category: &str) -> &'static str {
    if error.contains("journey_held_no_actionable_actor")
        || error.contains("journey has no ready, asymptomatic, noncritical actor")
        || error.contains("camp rest left no ready, asymptomatic, noncritical actor")
    {
        "journey_held_no_actionable_actor"
    } else if error.contains("Rest until the party reaches its next daylight walking window") {
        "journey_daylight_window_rest_required"
    } else if error.contains("start_discovery_dialogue") {
        "discovery_contact_failed"
    } else if error.contains("purchase_journey_provisions") {
        "journey_provision_purchase_failed"
    } else if error.contains("journey camp projection is incoherent")
        || error.contains("journey provisioning projection is incoherent")
    {
        "journey_projection_inconsistent"
    } else if error.contains("learned pattern requires acting during the nighttime window") {
        "investigation_night_window"
    } else if error.contains("Investigation track origin no longer matches the projected route") {
        "invalid_investigation_route"
    } else if error.contains("Investigation action is stale") {
        "investigation_action_stale"
    } else if error.contains("Investigation action is unavailable") {
        "investigation_action_unavailable"
    } else if victim_cohort_state_changed_failure(error) {
        "investigation_victim_cohort_state_changed"
    } else {
        match category {
            "rest_service_unavailable" => "rest_service_unavailable",
            "insufficient_visible_resources" => "insufficient_visible_resources",
            "bounded_progress_exhausted" => "bounded_progress_exhausted",
            "authoritative_backend_unavailable" => "authoritative_backend_unavailable",
            "invalid_run_environment" => "invalid_run_environment",
            _ => "unclassified_core_loop_error",
        }
    }
}

fn safe_core_loop_failure(error: &str) -> (&'static str, &'static str) {
    if error.contains("journey_held_no_actionable_actor")
        || error.contains("journey has no ready, asymptomatic, noncritical actor")
        || error.contains("camp rest left no ready, asymptomatic, noncritical actor")
    {
        (
            "journey_held_no_actionable_actor",
            "The public journey is held because no living party member is currently able to direct it.",
        )
    } else if error.contains("Rest until the party reaches its next daylight walking window") {
        (
            "journey_temporally_unavailable",
            "Camp travel was continued outside its public projected walking window.",
        )
    } else if error.contains("start_discovery_dialogue") {
        (
            "discovery_contact_failed",
            "A public discovery contact could not be completed.",
        )
    } else if error.contains("purchase_journey_provisions") {
        (
            "journey_provision_purchase_failed",
            "The public journey-provision purchase could not be completed.",
        )
    } else if error.contains("journey camp projection is incoherent")
        || error.contains("journey provisioning projection is incoherent")
    {
        (
            "journey_projection_inconsistent",
            "The public journey and camp projections were not coherent enough to continue safely.",
        )
    } else if error.contains("learned pattern requires acting during the nighttime window") {
        (
            "investigation_temporally_unavailable",
            "A projected investigation action was attempted outside its learned time window.",
        )
    } else if error.contains("Investigation track origin no longer matches the projected route") {
        (
            "invalid_investigation_route",
            "The projected investigation route no longer has a coherent completed origin.",
        )
    } else if victim_cohort_state_changed_failure(error) {
        (
            "investigation_state_changed",
            "A publicly projected investigation target changed before the action completed.",
        )
    } else if error.contains("offers neither an Inn nor a Temple") {
        (
            "rest_service_unavailable",
            "The settlement offers no player-visible rest service.",
        )
    } else if error.contains("Not enough coin") || error.contains("afford") {
        (
            "insufficient_visible_resources",
            "The NPC could not afford a player-visible action.",
        )
    } else if error.contains("bound exhausted") || error.contains("made no progress") {
        (
            "bounded_progress_exhausted",
            "The NPC exhausted a bounded action loop without making enough progress.",
        )
    } else if error.contains("timed out") || error.contains("connection") {
        (
            "authoritative_backend_unavailable",
            "The authoritative backend did not complete the requested operation.",
        )
    } else if error.contains("refusing") || error.contains("manifest") {
        (
            "invalid_run_environment",
            "The disposable simulation environment failed validation.",
        )
    } else {
        (
            "core_loop_error",
            "The authoritative core loop stopped before completion.",
        )
    }
}

fn bounded_failure_trace(
    trace: &[CoreLoopEvent],
    total_event_count: u64,
) -> (Vec<CoreLoopEvent>, bool) {
    let start = trace.len().saturating_sub(MAX_FAILURE_TRACE_EVENTS);
    (
        trace[start..].to_vec(),
        total_event_count > MAX_FAILURE_TRACE_EVENTS as u64,
    )
}
