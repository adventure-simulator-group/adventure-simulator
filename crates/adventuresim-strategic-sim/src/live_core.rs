//! Opt-in reducer-backed core-loop simulation.
//!
//! Unlike the native balance runner, this backend owns a disposable local
//! SpacetimeDB database and deliberately delegates every game rule to the
//! normal strategic reducers.

use crate::{
    ActivityPreference, AgentProfile, ChoiceArguments, ChoiceKind, DecisionArguments,
    DiscoveryView, EVAL_FORMAT_VERSION, EquipmentStyle, JournalView, LegalChoice, PartyView,
    PlayerFrame, QuestPolicy, generate_profile,
};
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
    backend_npc_case_intervention_type::BackendNpcCaseIntervention,
    backend_npc_case_interventions_table::BackendNpcCaseInterventionsTableAccess,
    backend_settlement_npcs_table::BackendSettlementNpcsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_death_table::CharacterDeathTableAccess,
    character_equip_table::CharacterEquipTableAccess,
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
    ensure_settlement_activity_reducer::ensure_settlement_activity, equip_item_reducer::equip_item,
    finalize_merchant_trade_reducer::finalize_merchant_trade, food_lot_table::FoodLotTableAccess,
    inventory_item_table::InventoryItemTableAccess, item_condition_table::ItemConditionTableAccess,
    item_table::ItemTableAccess, liquidate_party_inventory_reducer::liquidate_party_inventory,
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
    set_simulation_npc_intervention_strategy_reducer::set_simulation_npc_intervention_strategy,
    settlement_npc_presence_table::SettlementNpcPresenceTableAccess,
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
    world_data_import_table::WorldDataImportTableAccess,
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
    /// Concatenated server-authored stories from the authoritative NPC
    /// intervention transactions observed by this live disposable run.
    pub npc_intervention_stories_markdown: String,
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
    npc_id: String,
    name: String,
    profession: String,
    conversation_id: String,
    location_id: String,
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
            candidate.npc_id.clone(),
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

pub fn render_npc_intervention_stories(
    rows: impl IntoIterator<Item = BackendNpcCaseIntervention>,
) -> String {
    let mut rows = rows.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|row| (row.started_at, row.intervention_id.clone()));
    let mut output = String::from(
        "# Authoritative NPC adventurer quest stories\n\n\
         Each entry below was persisted by the SpacetimeDB intervention \
         transaction that applied its strategic outcome. It contains only \
         observer-safe events and exact dialogue spoken during that simulation.\n\n",
    );
    if rows.is_empty() {
        output.push_str("_No NPC intervention became eligible during this run._\n");
    } else {
        for row in rows {
            output.push_str(&row.public_story_markdown);
            if !output.ends_with("\n\n") {
                output.push('\n');
            }
        }
    }
    output
}

struct LiveRunner {
    connection: DbConnection,
    profiles: Vec<AgentProfile>,
    character_ids: Vec<u64>,
    metrics: CoreLoopMetrics,
    trace: Vec<CoreLoopEvent>,
    sequence: u64,
    dialogue_nonce: u64,
    last_semantic_event: Option<String>,
    recorded_deaths: HashSet<u64>,
    medically_paused_schedules: HashSet<u64>,
    generated_seen_cases: HashSet<(u64, String)>,
    generated_terminal_cases: HashSet<(u64, String)>,
    generated_exact_site_cases: HashSet<(u64, String)>,
    generated_traveled_cases: HashSet<(u64, String)>,
    generated_finance_blocks: HashMap<(String, u64, String), (u64, u64)>,
    npc_strategy_policy: Option<Box<dyn QuestPolicy>>,
    simulation_run_nonce: String,
    failure_recorder: FailureRecorder,
}

const SMITHING_DECISION_SCALE: f32 = 1_000.0;

fn quantize_smithing_condition(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * SMITHING_DECISION_SCALE).round() as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MedicalChoice {
    Ready,
    SuppressQuest,
    RestNaturally,
    BuyAndRest,
}

fn choose_medical_action(
    condition_status: &str,
    symptomatic: bool,
    at_settlement: bool,
    herbalist_available: bool,
    purse: u64,
    observable_quote: Option<u64>,
    natural_rest_venue: Option<bool>,
    medicated_rest_venue: Option<bool>,
) -> (MedicalChoice, &'static str) {
    if condition_status == "ready" && !symptomatic {
        return (MedicalChoice::Ready, "ready_without_symptoms");
    }
    if !at_settlement {
        return (MedicalChoice::SuppressQuest, "not_at_settlement");
    }
    if natural_rest_venue.is_none() {
        return (
            MedicalChoice::SuppressQuest,
            "rest_venue_unavailable_or_unaffordable",
        );
    }
    if !symptomatic {
        return (
            MedicalChoice::RestNaturally,
            "convalescing_without_symptoms",
        );
    }
    if !herbalist_available {
        return (MedicalChoice::RestNaturally, "herbalist_unavailable");
    }
    let Some(quote) = observable_quote else {
        return (MedicalChoice::RestNaturally, "observable_quote_unavailable");
    };
    if purse < quote || medicated_rest_venue.is_none() {
        return (MedicalChoice::RestNaturally, "observable_care_unaffordable");
    }
    (MedicalChoice::BuyAndRest, "symptomatic_and_affordable")
}

fn affordable_medical_rest_venue(
    inn_available: bool,
    temple_available: bool,
    temple_food_covers_day: bool,
    purse: u64,
    committed_cost: u64,
) -> Option<bool> {
    if temple_available && temple_food_covers_day && purse >= committed_cost {
        return Some(false);
    }
    let inn_cost = adventuresim_core::strategic_economy::inn_full_board_cost(1_440)?;
    (inn_available && purse >= committed_cost.saturating_add(inn_cost)).then_some(true)
}

fn temple_food_covers_one_day(visible_food_kcal: f32) -> bool {
    visible_food_kcal >= adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
}

fn observable_herbalist_stocks_medication(
    herbalist_available: bool,
    medication_kind: bool,
    herbs_stocked: bool,
) -> bool {
    herbalist_available && medication_kind && herbs_stocked
}

/// Keep one player-visible course of local treatment available while making
/// discretionary equipment decisions. This is a concrete emergency reserve,
/// not an arbitrary wealth target.
fn spending_budget_after_medical_reserve(purse: u64, observable_quote: Option<u64>) -> u64 {
    purse.saturating_sub(observable_quote.unwrap_or(0))
}

fn equipment_spend_is_still_affordable(
    purse: u64,
    observable_medical_quote: Option<u64>,
    equipment_cost: u64,
) -> bool {
    equipment_cost <= spending_budget_after_medical_reserve(purse, observable_medical_quote)
}

fn live_attributes(character_id: u64, profile: &AgentProfile) -> CharacterAttributes {
    let a = &profile.attributes;
    CharacterAttributes {
        character_id,
        endurance: a.endurance,
        immunity: a.immunity,
        gut: a.gut,
        intelligence: a.intelligence,
        instinct: a.instinct,
        eyesight: a.eyesight,
        hearing: a.hearing,
        left_arm_strength: a.left_arm_strength,
        right_arm_strength: a.right_arm_strength,
        left_leg_strength: a.left_leg_strength,
        right_leg_strength: a.right_leg_strength,
        left_arm_agility: a.left_arm_agility,
        right_arm_agility: a.right_arm_agility,
        left_leg_agility: a.left_leg_agility,
        right_leg_agility: a.right_leg_agility,
    }
}

fn live_skills(character_id: u64, profile: &AgentProfile) -> CharacterSkills {
    let s = profile.initial_skills;
    CharacterSkills {
        character_id,
        polearm_hours: s.polearm,
        axe_hours: s.axe,
        bludgeon_hours: s.bludgeon,
        sword_hours: s.sword,
        knife_hours: s.knife,
        dodge_hours: s.dodge,
        block_hours: s.block,
        bow_hours: s.bow,
        crossbow_hours: s.crossbow,
        firearm_hours: s.firearm,
        throw_hours: s.throw,
        will_hours: s.will,
        insight_hours: s.insight,
        charm_hours: s.charm,
        command_hours: s.command,
        deception_hours: s.deception,
        physiology_hours: s.physiology,
        cooking_hours: s.cooking,
        religion_hours: adventuresim_stdb_client::ReligionHours {
            roman_catholic: s.religion.roman_catholic,
            lutheran: s.religion.lutheran,
            reformed: s.religion.reformed,
            anglican: s.religion.anglican,
            eastern_orthodox: s.religion.eastern_orthodox,
            islamic: s.religion.islamic,
            judaism: s.religion.judaism,
        },
        bestiary_hours: adventuresim_stdb_client::BestiaryHours {
            beast: s.bestiary.beast,
            undead: s.bestiary.undead,
            human: s.bestiary.human,
            werekin: s.bestiary.werekin,
            elf: s.bestiary.elf,
            dwarf: s.bestiary.dwarf,
            fey: s.bestiary.fey,
            spirit: s.bestiary.spirit,
            greenskin: s.bestiary.greenskin,
            insectoid: s.bestiary.insectoid,
            draconid: s.bestiary.draconid,
            construct: s.bestiary.construct,
            wildmen: s.bestiary.wildmen,
        },
        anatomy_hours: s.anatomy,
        oral_languages: adventuresim_stdb_client::OralLanguageHours {
            east_central: 5_000.0,
            west_central: 0.0,
            low: 0.0,
            yiddish: 0.0,
            latin: 0.0,
            romani: 0.0,
            elven: 0.0,
            dwarfish: 0.0,
        },
        written_languages: adventuresim_stdb_client::WrittenLanguageHours {
            german: 1_000.0,
            low: 0.0,
            latin: 0.0,
            hebrew: 0.0,
            yiddish: 0.0,
            elven: 0.0,
            dwarfish: 0.0,
        },
        stealth_hours: s.stealth,
        balance_hours: s.balance,
        terrain_plains_hours: 0.0,
        terrain_forest_hours: 0.0,
        terrain_hills_hours: 0.0,
        terrain_urban_hours: 0.0,
        tailoring_hours: s.tailoring,
        smithing_hours: s.smithing,
    }
}

fn reallocate_disabled_crime_to_labor(mut schedule: ScheduleAllocation) -> ScheduleAllocation {
    let disabled_crime_minutes = schedule
        .thievery_minutes
        .checked_add(schedule.raiding_minutes)
        .expect("valid daily schedule crime allocation");
    schedule.labor_minutes = schedule
        .labor_minutes
        .checked_add(disabled_crime_minutes)
        .expect("valid daily schedule labor allocation");
    schedule.thievery_minutes = 0;
    schedule.raiding_minutes = 0;
    schedule
}

fn live_schedule(profile: &AgentProfile) -> ScheduleAllocation {
    let s = profile.schedule;
    // The live reducer accepts quarter-hour allocations. Native profiles are
    // intentionally more granular, so use the conservative lower notch and
    // leave the remainder as leisure instead of failing after medical rest.
    let quarter_hour = |minutes: u16| minutes / 15 * 15;
    reallocate_disabled_crime_to_labor(ScheduleAllocation {
        combat_training_minutes: quarter_hour(s.combat_training_minutes),
        carousing_minutes: quarter_hour(s.carousing_minutes),
        // Simulation profiles may express future profession preferences that
        // the disposable character has not learned yet. Do not submit those
        // locked activities to the authoritative schedule reducer.
        apprenticeship_minutes: 0,
        apprenticeship_organization_id: None,
        profession_practice_minutes: 0,
        practice_organization_id: None,
        labor_minutes: quarter_hour(s.labor),
        prayer_minutes: quarter_hour(s.prayer),
        // Crime activities can open a tactical incident and move the party to
        // its case site. This authoritative evaluator deliberately leaves the
        // tactical layer untouched. Preserve the authored time allocation by
        // assigning those minutes to legal subsistence labor instead.
        thievery_minutes: quarter_hour(s.thievery),
        raiding_minutes: quarter_hour(s.raiding),
    })
}

fn schedule_allocated_minutes(schedule: &ScheduleAllocation) -> u16 {
    [
        schedule.combat_training_minutes,
        schedule.carousing_minutes,
        schedule.apprenticeship_minutes,
        schedule.profession_practice_minutes,
        schedule.labor_minutes,
        schedule.prayer_minutes,
        schedule.thievery_minutes,
        schedule.raiding_minutes,
    ]
    .into_iter()
    .sum()
}

fn activity_schedule_plan(
    profile: &AgentProfile,
    temple_food_covers_day: bool,
    purse: u64,
    committed_reserve: u64,
    inn_cost: Option<u64>,
) -> (ScheduleAllocation, &'static str, &'static str) {
    let mut schedule = live_schedule(profile);
    let crime_fallback = matches!(
        profile.preferred_activity,
        ActivityPreference::Thievery | ActivityPreference::Raiding
    );
    let reserve_pressure =
        inn_cost.is_some_and(|cost| purse <= committed_reserve.saturating_add(cost));
    if schedule.labor_minutes == 0 && !temple_food_covers_day && reserve_pressure {
        let prayer_minutes = schedule.prayer_minutes;
        schedule.prayer_minutes = 0;
        if prayer_minutes > 0 {
            schedule.labor_minutes = prayer_minutes;
        } else {
            let discretionary_minutes =
                1_440_u16.saturating_sub(schedule_allocated_minutes(&schedule));
            schedule.labor_minutes = discretionary_minutes.min(480);
        }
        if schedule.labor_minutes > 0 {
            return (schedule, "Labor", "subsistence_reserve_to_labor");
        }
    }
    if crime_fallback {
        (schedule, "Labor", "crime_disabled_to_labor")
    } else {
        (
            schedule,
            match profile.preferred_activity {
                ActivityPreference::Labor => "Labor",
                ActivityPreference::Prayer => "Prayer",
                ActivityPreference::Thievery | ActivityPreference::Raiding => {
                    unreachable!("crime preferences are handled above")
                }
            },
            "none",
        )
    }
}

fn medical_rest_schedule() -> ScheduleAllocation {
    ScheduleAllocation {
        combat_training_minutes: 0,
        carousing_minutes: 0,
        apprenticeship_minutes: 0,
        apprenticeship_organization_id: None,
        profession_practice_minutes: 0,
        practice_organization_id: None,
        labor_minutes: 0,
        prayer_minutes: 0,
        thievery_minutes: 0,
        raiding_minutes: 0,
    }
}

fn live_personality(character_id: u64, p: &crate::Personality) -> CharacterPersonality {
    CharacterPersonality {
        character_id,
        projection_character_id: character_id,
        nerve: match p.nerve {
            crate::Nerve::Neutral => adventuresim_stdb_client::Nerve::Neutral,
            crate::Nerve::Brave => adventuresim_stdb_client::Nerve::Brave,
            crate::Nerve::Fearful => adventuresim_stdb_client::Nerve::Fearful,
        },
        drive: match p.drive {
            crate::Drive::Neutral => adventuresim_stdb_client::Drive::Neutral,
            crate::Drive::Ambitious => adventuresim_stdb_client::Drive::Ambitious,
            crate::Drive::Content => adventuresim_stdb_client::Drive::Content,
        },
        outlook: match p.outlook {
            crate::Outlook::Neutral => adventuresim_stdb_client::Outlook::Neutral,
            crate::Outlook::Sanguine => adventuresim_stdb_client::Outlook::Sanguine,
            crate::Outlook::Brooding => adventuresim_stdb_client::Outlook::Brooding,
        },
        sociability: match p.sociability {
            crate::Sociability::Neutral => adventuresim_stdb_client::Sociability::Neutral,
            crate::Sociability::Gregarious => adventuresim_stdb_client::Sociability::Gregarious,
            crate::Sociability::Solitary => adventuresim_stdb_client::Sociability::Solitary,
        },
        conscience: match p.conscience {
            crate::Conscience::Neutral => adventuresim_stdb_client::Conscience::Neutral,
            crate::Conscience::Compassionate => adventuresim_stdb_client::Conscience::Compassionate,
            crate::Conscience::Callous => adventuresim_stdb_client::Conscience::Callous,
            crate::Conscience::Cruel => adventuresim_stdb_client::Conscience::Cruel,
        },
        self_regard: match p.self_regard {
            crate::SelfRegard::Neutral => adventuresim_stdb_client::SelfRegard::Neutral,
            crate::SelfRegard::Proud => adventuresim_stdb_client::SelfRegard::Proud,
            crate::SelfRegard::Humble => adventuresim_stdb_client::SelfRegard::Humble,
        },
        conviction: match p.conviction {
            crate::Conviction::Neutral => adventuresim_stdb_client::Conviction::Neutral,
            crate::Conviction::Zealous => adventuresim_stdb_client::Conviction::Zealous,
            crate::Conviction::Irreverent => adventuresim_stdb_client::Conviction::Irreverent,
        },
        hygiene: match p.hygiene {
            crate::Hygiene::Neutral => adventuresim_stdb_client::Hygiene::Neutral,
            crate::Hygiene::Slovenly => adventuresim_stdb_client::Hygiene::Slovenly,
            crate::Hygiene::Cleanly => adventuresim_stdb_client::Hygiene::Cleanly,
        },
        temperance: match p.temperance {
            crate::Temperance::Neutral => adventuresim_stdb_client::Temperance::Neutral,
            crate::Temperance::Temperate => adventuresim_stdb_client::Temperance::Temperate,
            crate::Temperance::Drunkard => adventuresim_stdb_client::Temperance::Drunkard,
        },
        mirth: match p.mirth {
            crate::Mirth::Neutral => adventuresim_stdb_client::Mirth::Neutral,
            crate::Mirth::Merry => adventuresim_stdb_client::Mirth::Merry,
            crate::Mirth::Grave => adventuresim_stdb_client::Mirth::Grave,
        },
        courtship: match p.courtship {
            crate::Courtship::Neutral => adventuresim_stdb_client::Courtship::Neutral,
            crate::Courtship::Amorous => adventuresim_stdb_client::Courtship::Amorous,
            crate::Courtship::Proper => adventuresim_stdb_client::Courtship::Proper,
        },
        transparency: match p.transparency {
            crate::Transparency::Neutral => adventuresim_stdb_client::Transparency::Neutral,
            crate::Transparency::Open => adventuresim_stdb_client::Transparency::Open,
            crate::Transparency::Guarded => adventuresim_stdb_client::Transparency::Guarded,
        },
        self_knowledge: match p.self_knowledge {
            crate::SelfKnowledge::Neutral => adventuresim_stdb_client::SelfKnowledge::Neutral,
            crate::SelfKnowledge::Introspective => {
                adventuresim_stdb_client::SelfKnowledge::Introspective
            }
            crate::SelfKnowledge::SelfDeceiving => {
                adventuresim_stdb_client::SelfKnowledge::SelfDeceiving
            }
        },
        inclination: match p.inclination {
            crate::Inclination::Men => adventuresim_stdb_client::Inclination::Men,
            crate::Inclination::Either => adventuresim_stdb_client::Inclination::Either,
            crate::Inclination::Women => adventuresim_stdb_client::Inclination::Women,
            crate::Inclination::Neither => adventuresim_stdb_client::Inclination::Neither,
        },
        presentation: match p.presentation {
            crate::Presentation::Man => adventuresim_stdb_client::Presentation::Man,
            crate::Presentation::Ambiguous => adventuresim_stdb_client::Presentation::Ambiguous,
            crate::Presentation::Woman => adventuresim_stdb_client::Presentation::Woman,
        },
        sex: match p.sex {
            crate::Sex::Female => adventuresim_stdb_client::Sex::Female,
            crate::Sex::Male => adventuresim_stdb_client::Sex::Male,
        },
    }
}

macro_rules! reducer_call {
    ($runner:expr, $label:expr, $invoke:expr) => {{
        let (tx, rx) = mpsc::sync_channel(1);
        ($invoke)(
            move |_: &ReducerEventContext,
                  result: Result<
                Result<(), String>,
                adventuresim_stdb_client::spacetimedb_sdk::__codegen::InternalError,
            >| {
                let normalized = result
                    .map_err(|error| error.to_string())
                    .and_then(|module_result| module_result);
                let _ = tx.send(normalized);
            },
        )
        .map_err(|error| format!("could not send {}: {error}", $label))?;
        match rx.recv_timeout(ACTION_TIMEOUT) {
            Ok(result) => result.map_err(|error| format!("{} failed: {error}", $label)),
            Err(_) => Err(format!("{} timed out after {:?}", $label, ACTION_TIMEOUT)),
        }
    }};
}

impl LiveRunner {
    fn choose_pending_npc_strategies(&mut self) -> Result<(), String> {
        let Some(policy) = self.npc_strategy_policy.as_mut() else {
            return Ok(());
        };
        let candidates = self
            .connection
            .db
            .backend_npc_intervention_candidates()
            .iter()
            .filter(|candidate| !candidate.strategy_already_selected)
            .collect::<Vec<_>>();
        for candidate in candidates {
            let strategies: Vec<String> = serde_json::from_str(&candidate.legal_strategies_json)
                .map_err(|_| "server advertised malformed NPC strategy choices")?;
            let legal_choices = strategies
                .iter()
                .map(|strategy| LegalChoice {
                    choice_id: format!("choice:npc-strategy:{strategy}"),
                    kind: ChoiceKind::Conclude,
                    label: strategy.replace('_', " "),
                    typed_arguments: ChoiceArguments::default(),
                })
                .collect::<Vec<_>>();
            let frame = PlayerFrame {
                version: EVAL_FORMAT_VERSION,
                case_id: candidate.public_case_id.clone(),
                step: 0,
                game_minute: candidate.earliest_intervention_minute,
                discovery: DiscoveryView {
                    problem_summary: candidate.problem_summary.clone(),
                    consequence_summary: String::new(),
                    learned_at: "local reports".into(),
                    referrals: Vec::new(),
                },
                journal: JournalView::default(),
                party: PartyView {
                    members: 3,
                    terrain_skill: 0,
                    insight: 0,
                    perception: 0,
                    combat_readiness: candidate.party_capability.min(u16::from(u8::MAX)) as u8,
                    supplies: 0,
                    equipment_tags: Vec::new(),
                },
                legal_choices,
            };
            let decision = policy.decide(&frame)?;
            if decision.version != EVAL_FORMAT_VERSION
                || decision.arguments != DecisionArguments::default()
            {
                return Err("NPC strategy policy returned an unsupported decision".into());
            }
            let strategy = decision
                .choice_id
                .strip_prefix("choice:npc-strategy:")
                .filter(|choice| strategies.iter().any(|legal| legal == choice))
                .ok_or("NPC strategy policy selected a forged or stale choice")?
                .to_owned();
            let (tx, rx) = mpsc::sync_channel(1);
            self.connection
                .reducers
                .set_simulation_npc_intervention_strategy_then(
                    self.simulation_run_nonce.clone(),
                    candidate.case_id,
                    strategy,
                    move |_, result| {
                        let _ = tx.send(
                            result
                                .map_err(|error| error.to_string())
                                .and_then(|result| result),
                        );
                    },
                )
                .map_err(|error| error.to_string())?;
            rx.recv_timeout(ACTION_TIMEOUT)
                .map_err(|_| "NPC strategy reducer timed out".to_string())??;
        }
        Ok(())
    }
    fn event(&mut self, agent_id: u32, kind: CoreLoopEventKind, detail: impl Into<String>) {
        self.sequence += 1;
        let detail = detail.into();
        let semantic = format!("{agent_id}:{kind:?}:{detail}");
        let repeatable = event_is_repeatable(&kind);
        if !repeatable && self.last_semantic_event.as_ref() == Some(&semantic) {
            self.metrics.duplicate_semantic_events += 1;
        }
        self.last_semantic_event = Some(semantic);
        if self.trace.len() < MAX_CORE_TRACE_EVENTS {
            self.trace.push(CoreLoopEvent {
                sequence: self.sequence,
                agent_id,
                kind,
                detail,
            });
        }
        self.capture_failure_diagnostics();
    }

    fn call(&mut self, result: Result<(), String>) -> Result<(), String> {
        if result.is_err() {
            self.metrics.reducer_failures += 1;
            self.capture_failure_diagnostics();
        }
        result
    }

    fn capture_failure_diagnostics(&self) {
        let (trace, trace_truncated) = bounded_failure_trace(&self.trace, self.sequence);
        let final_agents = self
            .character_ids
            .iter()
            .enumerate()
            .filter_map(|(agent, character_id)| {
                self.public_failure_agent(agent as u32, *character_id)
            })
            .collect();
        self.failure_recorder.update(FailureDraft {
            metrics: self.metrics.clone(),
            total_event_count: self.sequence,
            trace_truncated,
            trace,
            final_agents,
        });
    }

    fn public_failure_agent(
        &self,
        agent_id: u32,
        character_id: u64,
    ) -> Option<CoreLoopFailureAgent> {
        let character = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)?;
        let condition = self
            .connection
            .db
            .character_strategic_condition()
            .iter()
            .find(|row| row.character_id == character_id)?;
        let illness = self
            .connection
            .db
            .character_illness_status()
            .iter()
            .find(|row| row.character_id == character_id);
        let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
        let party = character.party_id.as_deref().and_then(|party_id| {
            self.connection
                .db
                .party()
                .iter()
                .find(|row| row.id == party_id)
        });
        let current_case_site_id = party
            .as_ref()
            .and_then(|row| row.current_case_site_id.as_ref())
            .map(|site| site.value.clone());
        let journey_destination = party.as_ref().and_then(|party| {
            self.connection
                .db
                .party_journey()
                .iter()
                .find(|journey| journey.party_id == party.id)
                .map(|journey| public_journey_endpoint(&journey.destination))
        });
        let settlement = character
            .current_settlement_id
            .as_deref()
            .and_then(|settlement_id| {
                self.connection
                    .db
                    .settlement()
                    .iter()
                    .find(|row| row.id == settlement_id)
            });
        let mut settlement_services = settlement.as_ref().map_or_else(Vec::new, |row| {
            row.economy
                .services
                .iter()
                .map(|service| format!("{service:?}"))
                .collect()
        });
        settlement_services.sort();
        let settlement_id = character.current_settlement_id.clone();
        let visible_herbalist_quote = settlement_id
            .as_deref()
            .and_then(|id| self.observable_medical_quote(character_id, id));
        let visible_inn_full_board_cost = settlement
            .is_some_and(|row| row.economy.services.contains(&SettlementService::Inn))
            .then(|| adventuresim_core::strategic_economy::inn_full_board_cost(1_440))
            .flatten();
        Some(CoreLoopFailureAgent {
            agent_id,
            character_id,
            alive: character.alive,
            condition_status: condition.status,
            hunger: condition.hunger,
            thirst: condition.thirst,
            food_days: condition.food_days,
            water_days: condition.water_days,
            visible_food_kcal,
            visible_water_ml,
            personal_gold_coin: self.personal_gold(character_id),
            settlement_id,
            current_case_site_id,
            journey_destination,
            symptomatic: illness.as_ref().is_some_and(|row| row.symptomatic),
            critical: illness.as_ref().is_some_and(|row| row.critical),
            settlement_services,
            visible_herbalist_quote,
            visible_inn_full_board_cost,
        })
    }

    fn settlement_activity_venue(
        &self,
        character_id: u64,
        committed_reserve: u64,
    ) -> Result<SettlementActivityVenue, String> {
        let settlement_id = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .and_then(|character| character.current_settlement_id)
            .ok_or("simulation character is not at a settlement")?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|settlement| settlement.id == settlement_id)
            .ok_or("simulation settlement is unavailable")?;
        let inn_available = settlement
            .economy
            .services
            .contains(&SettlementService::Inn);
        let temple_available = settlement
            .economy
            .services
            .contains(&SettlementService::Temple);
        if !inn_available && !temple_available {
            return Err("simulation settlement offers neither an Inn nor a Temple".to_string());
        }
        let (visible_food_kcal, _) = self.visible_rest_supplies(character_id);
        select_settlement_activity_venue(
            inn_available,
            temple_available,
            temple_food_covers_one_day(visible_food_kcal),
            self.personal_gold(character_id),
            committed_reserve,
            adventuresim_core::strategic_economy::inn_full_board_cost(1_440),
        )
        .ok_or_else(|| {
            "simulation character cannot afford an Inn while preserving visible reserves"
                .to_string()
        })
    }

    /// Non-activity waits retain the ordinary public-service preference. Their
    /// requested duration can be shorter than the one-day activity planner's
    /// supply horizon.
    fn settlement_rest_at_inn(&self, character_id: u64) -> Result<bool, String> {
        let settlement_id = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .and_then(|character| character.current_settlement_id)
            .ok_or("simulation character is not at a settlement")?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|settlement| settlement.id == settlement_id)
            .ok_or("simulation settlement is unavailable")?;
        let service =
            adventuresim_core::settlement_economy::select_available_settlement_rest_service(
                settlement
                    .economy
                    .services
                    .contains(&SettlementService::Inn),
                settlement
                    .economy
                    .services
                    .contains(&SettlementService::Temple),
            )
            .ok_or("simulation settlement offers neither an Inn nor a Temple")?;
        Ok(adventuresim_core::settlement_economy::action_service_at_inn(service))
    }

    fn party_for(&self, character_id: u64) -> Result<Party, String> {
        let character = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .ok_or("character missing from coherent subscription")?;
        let party_id = character.party_id.ok_or("character has no party")?;
        self.connection
            .db
            .party()
            .iter()
            .find(|row| row.id == party_id)
            .ok_or_else(|| "party missing from coherent subscription".into())
    }

    fn party_by_id(&self, party_id: &str) -> Result<Party, String> {
        self.connection
            .db
            .party()
            .iter()
            .find(|row| row.id == party_id)
            .ok_or_else(|| "party missing from coherent subscription".into())
    }

    fn current_leader(&self, party_id: &str) -> Option<(u64, u32)> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|row| row.id == party_id)?;
        let leader = self.connection.db.character().iter().find(|row| {
            leader_is_actionable(
                party_id,
                party.leader_id,
                row.id,
                row.alive,
                row.party_id.as_deref(),
            )
        })?;
        let agent = self.character_ids.iter().position(|id| *id == leader.id)? as u32;
        Some((leader.id, agent))
    }

    fn public_party_elapsed_max(&self, party_id: &str) -> u64 {
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| row.character_id)
            .collect::<HashSet<_>>();
        self.connection
            .db
            .character_time()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .map(|row| row.minutes)
            .max()
            .unwrap_or(0)
    }

    fn observe_deaths(&mut self) {
        let mut newly_dead = self
            .connection
            .db
            .character()
            .iter()
            .filter(|row| !row.alive && self.character_ids.contains(&row.id))
            .filter_map(|row| self.recorded_deaths.insert(row.id).then_some(row.id))
            .collect::<Vec<_>>();
        newly_dead.sort_unstable();
        for character_id in newly_dead {
            if let Some(agent) = self.character_ids.iter().position(|id| *id == character_id) {
                let source = self
                    .connection
                    .db
                    .character_death()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .map(|row| row.source);
                if source == Some(DeathSource::Disease) {
                    self.metrics.disease_deaths += 1;
                }
                self.event(
                    agent as u32,
                    CoreLoopEventKind::Death,
                    format!("authoritative terminal state;source={source:?}"),
                );
            }
        }
    }

    fn provision_case_site_journey(
        &mut self,
        party_id: &str,
        leader: u64,
        agent: u32,
        finance_key: &str,
        distance_m: u64,
    ) -> Result<TravelProvisionDecision, String> {
        let party = self.party_by_id(party_id)?;
        let Some(settlement_id) = party.current_settlement_id.clone() else {
            return Ok(TravelProvisionDecision::Deferred(
                "provisioning_requires_settlement",
            ));
        };
        let planning_minutes =
            projected_case_site_journey_minutes(distance_m, party.walking_minutes_per_day)
                .ok_or("journey provisioning projection is incoherent")?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)
            .ok_or("journey provisioning projection is incoherent")?;
        let ration = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| {
                row.id == adventuresim_core::provisioning::STANDARD_TRAVEL_RATION_ID
                    && row.nutrition_kcal > 0.0
            })
            .ok_or("journey provisioning projection is incoherent")?;
        let waterskin = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| {
                row.id == adventuresim_core::provisioning::STANDARD_WATERSKIN_ID
                    && row.water_capacity_ml > 0
            })
            .ok_or("journey provisioning projection is incoherent")?;
        let members = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .filter_map(|membership| {
                self.connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == membership.character_id && row.alive)
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            return Err("journey provisioning projection is incoherent".into());
        }
        let member_ids = members.iter().map(|row| row.id).collect::<HashSet<_>>();
        let personal_inventory = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .collect::<Vec<_>>();
        let personal_inventory_ids = personal_inventory
            .iter()
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_inventory = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id)
            .collect::<Vec<_>>();
        let party_inventory_ids = party_inventory
            .iter()
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let food_reserve_kcal = members
            .iter()
            .filter_map(|member| {
                self.connection
                    .db
                    .character_needs()
                    .iter()
                    .find(|row| row.character_id == member.id)
            })
            .map(|needs| needs.food_balance_kcal.max(0.0))
            .sum();
        let water_reserve_ml = members
            .iter()
            .filter_map(|member| {
                self.connection
                    .db
                    .character_needs()
                    .iter()
                    .find(|row| row.character_id == member.id)
            })
            .map(|needs| needs.water_balance_ml.max(0.0))
            .sum();
        let food_lot_kcal = self
            .connection
            .db
            .food_lot()
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| personal_inventory_ids.contains(&id))
                    || lot
                        .party_inventory_item_id
                        .is_some_and(|id| party_inventory_ids.contains(&id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum();
        let count_item = |item_id: &str| {
            personal_inventory
                .iter()
                .filter(|row| row.item_id == item_id)
                .map(|row| row.quantity)
                .chain(
                    party_inventory
                        .iter()
                        .filter(|row| row.item_id == item_id)
                        .map(|row| row.quantity),
                )
                .sum::<u32>()
        };
        let inputs = adventuresim_core::provisioning::PartyProvisioningInputs {
            planning_minutes,
            target_surplus_days: TRAVEL_PROVISION_RESERVE_DAYS,
            living_members: members.len() as u32,
            food_reserve_kcal,
            food_lot_kcal,
            water_reserve_ml,
            ration_count: count_item(adventuresim_core::provisioning::STANDARD_TRAVEL_RATION_ID),
            waterskin_count: count_item(adventuresim_core::provisioning::STANDARD_WATERSKIN_ID),
            ration_kcal: ration.nutrition_kcal,
            waterskin_capacity_ml: waterskin.water_capacity_ml,
            emergency_alcohol_hydration_ml: 0,
        };
        let forecast = inputs.forecast();
        let rations_to_buy = forecast.rations_to_buy;
        let waterskins_to_buy = forecast.waterskins_to_buy;
        if rations_to_buy == 0 && waterskins_to_buy == 0 {
            self.event(
                agent,
                CoreLoopEventKind::Purchase,
                format!(
                    "journey_provisions=ready;planning_minutes={planning_minutes};reserve_days={TRAVEL_PROVISION_RESERVE_DAYS:.1};food_days={:.2};water_days={:.2}",
                    forecast.food_days, forecast.water_days,
                ),
            );
            return Ok(TravelProvisionDecision::Ready);
        }
        if rations_to_buy > MAX_TRAVEL_PROVISION_UNITS_PER_ITEM
            || waterskins_to_buy > MAX_TRAVEL_PROVISION_UNITS_PER_ITEM
        {
            return Err("journey provisioning projection is incoherent".into());
        }
        // The public storefront contract guarantees both travel staples at
        // every General storefront. Read the generated public settlement
        // projection directly rather than converting it to server schema
        // types or inspecting private merchant authority.
        let general_storefront_visible = settlement.economy.services.iter().any(|service| {
            matches!(
                service,
                SettlementService::Market | SettlementService::GeneralStore
            )
        });
        let ration_stocked = general_storefront_visible;
        let waterskin_stocked = general_storefront_visible;
        if (rations_to_buy > 0 && !ration_stocked) || (waterskins_to_buy > 0 && !waterskin_stocked)
        {
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "reason=journey_essentials_unavailable;planning_minutes={planning_minutes};rations_needed={rations_to_buy};waterskins_needed={waterskins_to_buy}"
                ),
            );
            return Ok(TravelProvisionDecision::Deferred(
                "journey_essentials_unavailable",
            ));
        }
        let party_coin = party_inventory
            .iter()
            .filter(|row| is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let upper_bound_cost_for = |buy_bps: i32| -> Option<u64> {
            let unit_price = |item: &Item| {
                let base = adventuresim_core::strategic_economy::merchant_buy_price(
                    item.base_value.unwrap_or(1),
                );
                let language_bound =
                    adventuresim_core::strategic_economy::language_adjusted_buy_price(base, 0.0);
                adventuresim_core::local_problem::adjust_price(language_bound, buy_bps)
            };
            u64::from(unit_price(&ration))
                .checked_mul(u64::from(rations_to_buy))?
                .checked_add(
                    u64::from(unit_price(&waterskin)).checked_mul(u64::from(waterskins_to_buy))?,
                )
        };
        let mut payer_options = members
            .iter()
            .filter(|member| member.current_settlement_id.as_deref() == Some(&settlement_id))
            .filter_map(|member| {
                let payer_minute = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == member.id)?
                    .minutes;
                let merchant_count = self
                    .connection
                    .db
                    .backend_settlement_npcs()
                    .iter()
                    .filter(|npc| {
                        npc.home_settlement_id == settlement_id && npc.service_id == "merchants"
                    })
                    .filter(|npc| {
                        self.connection
                            .db
                            .settlement_npc_presence()
                            .iter()
                            .any(|presence| {
                                presence.npc_id == npc.id
                                    && presence.settlement_id == settlement_id
                                    && presence.location_id == "market"
                                    && presence.is_default
                                    && npc_is_publicly_present(
                                        presence.start_minute,
                                        presence.end_minute,
                                        payer_minute,
                                    )
                            })
                    })
                    .count();
                if merchant_count != 1 {
                    return None;
                }
                let buy_bps = self
                    .connection
                    .db
                    .backend_local_problem_trade_effects()
                    .iter()
                    .find(|row| {
                        row.character_id == member.id && row.settlement_id == settlement_id
                    })?
                    .buy_bps;
                let upper_bound_cost = upper_bound_cost_for(buy_bps)?;
                let personal = self.personal_gold(member.id);
                let committed_reserve = self
                    .observable_medical_reserve(member.id, &settlement_id)
                    .unwrap_or(0);
                let spendable = party_coin
                    .saturating_add(personal)
                    .saturating_sub(committed_reserve);
                Some((
                    spendable >= upper_bound_cost,
                    spendable,
                    member.id,
                    personal,
                    committed_reserve,
                    upper_bound_cost,
                ))
            })
            .collect::<Vec<_>>();
        payer_options.sort_by_key(|option| (option.0, option.1, option.2));
        let Some((affordable, spendable, payer, personal, committed_reserve, upper_bound_cost)) =
            payer_options.pop()
        else {
            return Ok(TravelProvisionDecision::Deferred(
                "journey_payer_provider_projection_unavailable",
            ));
        };
        let stake = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == payer)
            .map_or(0, |row| row.value);
        let finance_cache_key = (party_id.to_owned(), leader, finance_key.to_owned());
        if !affordable {
            if self
                .generated_seen_cases
                .contains(&(leader, finance_key.to_owned()))
            {
                self.metrics.generated_finance_blocked_cycles = self
                    .metrics
                    .generated_finance_blocked_cycles
                    .saturating_add(1);
            }
            let public_funds = party_coin.saturating_add(personal);
            let signature = (upper_bound_cost, public_funds);
            if self.generated_finance_blocks.get(&finance_cache_key) == Some(&signature) {
                return Ok(TravelProvisionDecision::Deferred("journey_finance_backoff"));
            }
            self.generated_finance_blocks
                .insert(finance_cache_key, signature);
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "reason=journey_essentials_unaffordable;planning_minutes={planning_minutes};payer={payer};upper_bound_cost={upper_bound_cost};treasury={party_coin};payer_purse={personal};claimable_stake={stake};committed_reserve={committed_reserve};spendable={spendable};deficit={};rations_needed={rations_to_buy};waterskins_needed={waterskins_to_buy}",
                    upper_bound_cost.saturating_sub(spendable),
                ),
            );
            return Ok(TravelProvisionDecision::Deferred(
                "journey_essentials_unaffordable",
            ));
        }
        self.generated_finance_blocks.remove(&finance_cache_key);
        let mut item_ids = Vec::new();
        let mut quantities = Vec::new();
        if rations_to_buy > 0 {
            item_ids.push(ration.id.clone());
            quantities.push(rations_to_buy);
        }
        if waterskins_to_buy > 0 {
            item_ids.push(waterskin.id.clone());
            quantities.push(waterskins_to_buy);
        }
        let result = reducer_call!(self, "purchase_journey_provisions", |cb| self
            .connection
            .reducers
            .finalize_merchant_trade_then(
                payer,
                settlement_id.clone(),
                item_ids.clone(),
                quantities.clone(),
                vec![],
                vec![],
                true,
                cb,
            ));
        self.call(result)?;
        let after_party_coin = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum::<u64>();
        let actual_spent = party_coin
            .saturating_add(personal)
            .saturating_sub(after_party_coin.saturating_add(self.personal_gold(payer)));
        self.metrics.journey_provision_purchases += 1;
        self.metrics.journey_provision_party_gold_spent = self
            .metrics
            .journey_provision_party_gold_spent
            .saturating_add(actual_spent);
        self.event(
            agent,
            CoreLoopEventKind::Purchase,
            format!(
                "journey_provisions=purchased;planning_minutes={planning_minutes};reserve_days={TRAVEL_PROVISION_RESERVE_DAYS:.1};payer={payer};treasury_before={party_coin};payer_purse_before={personal};claimable_stake={stake};upper_bound_cost={upper_bound_cost};actual_spent={actual_spent};rations={rations_to_buy};waterskins={waterskins_to_buy}"
            ),
        );
        Ok(TravelProvisionDecision::Ready)
    }

    fn public_active_camp_observation(
        &self,
        party_id: &str,
    ) -> Option<PublicActiveCampObservation> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let camp_destination = party.camp_destination.as_ref()?;
        if party.current_settlement_id.is_some() || party.camp_remaining_minutes == 0 {
            return None;
        }
        let journeys = self
            .connection
            .db
            .party_journey()
            .iter()
            .filter(|journey| journey.party_id == party_id)
            .collect::<Vec<_>>();
        let itineraries = self
            .connection
            .db
            .party_journey_itinerary()
            .iter()
            .filter(|itinerary| itinerary.party_id == party_id)
            .collect::<Vec<_>>();
        let [journey] = journeys.as_slice() else {
            return None;
        };
        let [itinerary] = itineraries.as_slice() else {
            return None;
        };
        if &journey.destination != camp_destination
            || journey.completed_elapsed_minutes >= journey.total_elapsed_minutes
        {
            return None;
        }
        let (active_interval_start, active_interval_minutes) = projected_camp_rest_minutes(
            journey.completed_elapsed_minutes,
            journey.total_elapsed_minutes,
            &itinerary.forecast_camp_intervals,
        )?;
        (active_interval_minutes > 0).then_some(PublicActiveCampObservation {
            completed_elapsed_minutes: journey.completed_elapsed_minutes,
            total_elapsed_minutes: journey.total_elapsed_minutes,
            active_interval_start,
            active_interval_minutes,
        })
    }

    fn party_has_unresolved_public_encounter(&self, party_id: &str) -> bool {
        self.connection
            .db
            .strategic_encounter()
            .iter()
            .any(|row| row.party_id == party_id && row.status == "awaiting_choice")
    }

    fn travel_camps(&mut self, party_id: &str) -> Result<JourneyTravelOutcome, String> {
        for _ in 0..MAX_CAMPS_PER_LEG {
            let party = self.party_by_id(party_id)?;
            if party.camp_destination.is_none() {
                self.metrics.travel_legs += 1;
                return Ok(JourneyTravelOutcome::Completed);
            }
            let remaining_before = party.camp_remaining_minutes;
            let Some((travel_actor, _, _)) = self.expedition_recovery_actor(party_id) else {
                self.observe_deaths();
                return self.record_journey_hold(
                    party_id,
                    "journey_stalled",
                    "journey_held_no_actionable_actor",
                );
            };
            let pending_encounter = {
                let table = self.connection.db.strategic_encounter();
                table
                    .iter()
                    .find(|row| row.party_id == party_id && row.status == "awaiting_choice")
            };
            if let Some(encounter) = pending_encounter {
                self.metrics.encounters += 1;
                if encounter.run_ineligibility.is_none() {
                    self.metrics.encounter_escape_eligible += 1;
                } else {
                    self.metrics.encounter_escape_ineligible += 1;
                }
                let evacuation = self.public_journey_is_evacuation(party_id);
                let choice = select_expedition_encounter_choice(
                    &encounter.available_choices,
                    encounter.roll_index,
                    evacuation,
                )
                .ok_or("encounter offers no protective evacuation choice")?;
                match choice.as_str() {
                    "sneak" => self.metrics.encounter_sneaks += 1,
                    "detour" => self.metrics.encounter_detours += 1,
                    "attack" => self.metrics.encounter_attacks += 1,
                    "run" => self.metrics.encounter_runs += 1,
                    "surrender" => {
                        self.metrics.encounter_surrenders += 1;
                        self.metrics.encounter_surrender_items_lost =
                            self.metrics.encounter_surrender_items_lost.saturating_add(
                                encounter
                                    .loss_preview
                                    .iter()
                                    .map(|loss| loss.quantity)
                                    .sum(),
                            );
                        self.metrics.encounter_surrender_value_lost =
                            self.metrics.encounter_surrender_value_lost.saturating_add(
                                encounter
                                    .loss_preview
                                    .iter()
                                    .map(|loss| {
                                        u64::from(loss.quantity) * u64::from(loss.value_each)
                                    })
                                    .sum::<u64>(),
                            );
                    }
                    _ => return Err("encounter exposed an unknown choice".into()),
                }
                let encounter_id = encounter.encounter_id.clone();
                let result = reducer_call!(self, "resolve_strategic_encounter", |cb| self
                    .connection
                    .reducers
                    .resolve_strategic_encounter_then(travel_actor, choice.clone(), cb));
                self.call(result)?;
                self.observe_deaths();
                let resolved_outcome = {
                    let table = self.connection.db.strategic_encounter();
                    table
                        .iter()
                        .find(|row| row.encounter_id == encounter_id)
                        .ok_or("resolved encounter row disappeared")?
                        .outcome
                };
                if resolved_outcome.as_deref() == Some("defeat") {
                    self.metrics.encounter_defeats += 1;
                    if self.current_leader(party_id).is_none() {
                        self.metrics.encounter_wipes += 1;
                    }
                }
                self.event(
                    self.current_leader(party_id).map_or(0, |(_, agent)| agent),
                    CoreLoopEventKind::Encounter,
                    format!("id={encounter_id};choice={choice};outcome={resolved_outcome:?}"),
                );
                if self.current_leader(party_id).is_none() {
                    return self.record_journey_hold(
                        party_id,
                        "journey_stalled_after_encounter",
                        "journey_held_no_actionable_actor",
                    );
                }
                continue;
            }
            let camp = self
                .public_active_camp_observation(party_id)
                .ok_or("journey camp projection is incoherent: no unique active public camp")?;
            let camp_start = camp.active_interval_start;
            let rest_minutes = camp.active_interval_minutes;
            self.event(
                self.current_leader(party_id).map_or(0, |(_, agent)| agent),
                CoreLoopEventKind::Camp,
                format!(
                    "phase=pre_rest;party={};completed_elapsed={};total_elapsed={};camp_start={camp_start};rest_minutes={rest_minutes};remaining_movement={remaining_before}",
                    bounded_event_field(party_id),
                    camp.completed_elapsed_minutes,
                    camp.total_elapsed_minutes,
                ),
            );
            let camp_members_before = self.expedition_member_observations(party_id)?;
            let camp_supplies_before = self.expedition_supplies(party_id);
            let expected_completed_elapsed = camp_start.saturating_add(rest_minutes);
            let result = reducer_call!(self, "rest_at_camp", |cb| self
                .connection
                .reducers
                .rest_at_camp_then(travel_actor, rest_minutes, cb));
            self.call(result)?;
            self.observe_deaths();
            let Some((continue_actor, agent, continue_actor_role)) =
                self.expedition_recovery_actor(party_id)
            else {
                return self.record_journey_hold(
                    party_id,
                    "journey_stalled_after_rest",
                    "journey_held_no_actionable_actor",
                );
            };
            let unsafe_after_rest = self.unsafe_party_agents(&self.party_agents(continue_actor)?);
            let camp_members_after = self.expedition_member_observations(party_id)?;
            let camp_supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                party_id,
                "journey_camp",
                "rest_at_camp",
                if unsafe_after_rest.is_empty() {
                    "quest_leg_rest_complete"
                } else {
                    "quest_suppressed_member_not_ready_after_camp"
                },
                &camp_members_before,
                &camp_members_after,
                camp_supplies_before,
                camp_supplies_after,
            );
            let after_rest_party = self.party_by_id(party_id)?;
            let after_rest_journey = self
                .connection
                .db
                .party_journey()
                .iter()
                .find(|row| row.party_id == party_id)
                .ok_or("journey camp projection is incoherent: journey disappeared after rest")?;
            let after_rest_itinerary = self
                .connection
                .db
                .party_journey_itinerary()
                .iter()
                .find(|row| row.party_id == party_id)
                .ok_or("journey camp projection is incoherent: itinerary disappeared after rest")?;
            if after_rest_party.camp_destination.is_none()
                || after_rest_journey.completed_elapsed_minutes != expected_completed_elapsed
                || after_rest_journey.completed_elapsed_minutes
                    > after_rest_journey.total_elapsed_minutes
                || after_rest_itinerary.party_id != party_id
            {
                return Err(
                    "journey camp projection is incoherent: rest did not produce a safe forecast boundary"
                        .into(),
                );
            }
            self.event(
                agent,
                CoreLoopEventKind::Camp,
                format!(
                    "phase=post_rest;party={};completed_elapsed={};total_elapsed={};rest_minutes={rest_minutes};remaining_movement={}",
                    bounded_event_field(party_id),
                    after_rest_journey.completed_elapsed_minutes,
                    after_rest_journey.total_elapsed_minutes,
                    after_rest_party.camp_remaining_minutes,
                ),
            );
            let evacuation_leg = matches!(
                after_rest_party.camp_destination,
                Some(JourneyEndpoint::Settlement(_))
            );
            if !unsafe_after_rest.is_empty() && !evacuation_leg {
                self.metrics.expedition_holds = self.metrics.expedition_holds.saturating_add(1);
                for unsafe_agent in unsafe_after_rest {
                    self.metrics.quests_suppressed_for_health =
                        self.metrics.quests_suppressed_for_health.saturating_add(1);
                    self.event(
                        unsafe_agent,
                        CoreLoopEventKind::QuestSuppressed,
                        "reason=journey_camp_member_not_ready;plan=off_settlement_recovery_next_cycle",
                    );
                }
                return Ok(JourneyTravelOutcome::HeldForRecovery);
            }
            let leg_members_before = self.expedition_member_observations(party_id)?;
            let leg_supplies_before = self.expedition_supplies(party_id);
            let result = reducer_call!(self, "continue_camp_travel", |cb| self
                .connection
                .reducers
                .continue_camp_travel_then(continue_actor, cb));
            self.call(result)?;
            self.observe_deaths();
            self.metrics.camp_stops += 1;
            self.event(
                agent,
                CoreLoopEventKind::Camp,
                format!(
                    "phase=post_continue;party={};remaining_before={remaining_before};remaining_after={}",
                    bounded_event_field(party_id),
                    self.party_by_id(party_id)?.camp_remaining_minutes,
                ),
            );
            let leg_members_after = self.expedition_member_observations(party_id)?;
            let leg_supplies_after = self.expedition_supplies(party_id);
            let recovery_needed = leg_members_after
                .iter()
                .any(expedition_member_needs_recovery);
            let leg_reason = if evacuation_leg {
                format!("quest_suppressed_evacuation_continues_{continue_actor_role}")
            } else if recovery_needed {
                "quest_suppressed_member_not_ready_after_leg;plan=off_settlement_recovery_next_cycle"
                    .into()
            } else {
                "quest_leg_resumed_all_members_ready".into()
            };
            self.emit_expedition_diagnostics(
                party_id,
                "journey_leg",
                "continue_camp_travel",
                &leg_reason,
                &leg_members_before,
                &leg_members_after,
                leg_supplies_before,
                leg_supplies_after,
            );
            let after = self.party_by_id(party_id)?;
            if after.camp_destination.is_some() && after.camp_remaining_minutes >= remaining_before
            {
                self.metrics.stuck_detections += 1;
                return Err("camp continuation made no progress".into());
            }
        }
        self.metrics.stuck_detections += 1;
        Err("camp bound exhausted".into())
    }

    fn continue_public_active_journey(
        &mut self,
        party_id: &str,
    ) -> Result<Option<JourneyTravelOutcome>, String> {
        let party = self.party_by_id(party_id)?;
        let has_public_journey = party.camp_destination.is_some()
            || self
                .connection
                .db
                .party_journey()
                .iter()
                .any(|journey| journey.party_id == party_id);
        if !has_public_journey {
            return Ok(None);
        }
        let outcome = self.travel_camps(party_id)?;
        if outcome != JourneyTravelOutcome::Completed {
            return Ok(Some(outcome));
        }

        self.observe_deaths();
        let Some((leader, agent)) = self.current_leader(party_id) else {
            return self
                .record_journey_hold(
                    party_id,
                    "journey_arrival_revalidation",
                    "journey_held_arrival_not_proven",
                )
                .map(Some);
        };
        let party_agents = self.party_agents(leader)?;
        let arrived = self.party_by_id(party_id)?;
        let location_is_publicly_coherent = arrived.camp_destination.is_none()
            && (arrived.current_settlement_id.is_some() ^ arrived.current_case_site_id.is_some());
        if arrived.id != party_id
            || !self.unsafe_party_agents(&party_agents).is_empty()
            || !location_is_publicly_coherent
        {
            self.metrics.expedition_holds = self.metrics.expedition_holds.saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::QuestSuppressed,
                "reason=journey_continuation_arrival_not_actionable",
            );
            return Ok(Some(JourneyTravelOutcome::HeldForRecovery));
        }
        self.event(
            agent,
            CoreLoopEventKind::Travel,
            format!(
                "journey_continuation=completed;settlement={};case_site={}",
                arrived
                    .current_settlement_id
                    .as_deref()
                    .map_or_else(|| "none".into(), bounded_event_field),
                arrived
                    .current_case_site_id
                    .as_ref()
                    .map_or_else(|| "none".into(), |site| bounded_event_field(&site.value)),
            ),
        );
        Ok(Some(JourneyTravelOutcome::Completed))
    }

    fn choose_quest(&self, party: &Party, profile: &AgentProfile) -> Option<BackendContract> {
        let settlement = party.current_settlement_id.as_ref()?;
        let mut quests: Vec<_> = self
            .connection
            .db
            .backend_contracts()
            .iter()
            .filter(|q| q.settlement_id == *settlement && q.status == ContractStatus::Offered)
            .collect();
        quests.sort_by_key(|q| {
            let risk_target = (profile.risk_tolerance * 10.0).round() as i32;
            ((q.difficulty - risk_target).abs(), q.id.clone())
        });
        quests.into_iter().next()
    }

    fn active_direct_contract(&self, party: &Party) -> Option<BackendContract> {
        let contract_id = party.active_contract_id.as_ref()?;
        self.connection
            .db
            .backend_contracts()
            .iter()
            .find(|contract| {
                contract.id == *contract_id
                    && contract.accepted_by.as_deref() == Some(party.id.as_str())
                    && matches!(
                        contract.status,
                        ContractStatus::Accepted | ContractStatus::ReadyToReport
                    )
            })
    }

    fn personal_gold(&self, character_id: u64) -> u64 {
        self.connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum()
    }

    fn settlement_rest_sponsor(
        &self,
        patient_id: u64,
        settlement_id: &str,
        public_quote: u64,
    ) -> Option<SettlementRestSponsor> {
        let patient_purse = self.personal_gold(patient_id);
        if patient_purse >= public_quote {
            return None;
        }
        let patient_contribution = patient_purse.min(public_quote);
        let sponsor_quote = public_quote.saturating_sub(patient_contribution);
        let patient = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == patient_id && row.alive)?;
        let party_id = patient.party_id.as_deref()?;
        if patient.current_settlement_id.as_deref() != Some(settlement_id)
            || !self
                .connection
                .db
                .party_member()
                .iter()
                .any(|member| member.party_id == party_id && member.character_id == patient_id)
        {
            return None;
        }
        let party_treasury = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum();
        let mut options = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party_id && member.character_id != patient_id)
            .filter_map(|member| {
                let payer = self.connection.db.character().iter().find(|row| {
                    row.id == member.character_id
                        && row.alive
                        && row.current_settlement_id.as_deref() == Some(settlement_id)
                })?;
                let payer_agent_id =
                    self.character_ids.iter().position(|id| *id == payer.id)? as u32;
                let purse = self.personal_gold(payer.id);
                let medical_reserve = self
                    .observable_medical_reserve(payer.id, settlement_id)
                    .unwrap_or(0);
                let spendable = purse.saturating_sub(medical_reserve);
                (spendable >= sponsor_quote).then(|| SettlementRestSponsor {
                    payer_id: payer.id,
                    payer_agent_id,
                    purse,
                    medical_reserve,
                    spendable,
                    patient_contribution,
                    sponsor_quote,
                    party_treasury,
                    party_stake: self
                        .connection
                        .db
                        .party_stake()
                        .iter()
                        .find(|stake| stake.party_id == party_id && stake.character_id == payer.id)
                        .map_or(0, |stake| stake.value),
                })
            })
            .collect::<Vec<_>>();
        options.sort_by_key(|option| {
            (
                std::cmp::Reverse(option.spendable),
                option.payer_id,
                option.payer_agent_id,
            )
        });
        options.into_iter().next()
    }

    fn activity_observation(&self, character_id: u64) -> Result<ActivityObservation, String> {
        let condition = self
            .connection
            .db
            .character_strategic_condition()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing activity condition")?;
        let elapsed_minutes = self
            .connection
            .db
            .character_time()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing activity clock")?
            .minutes;
        let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
        Ok(ActivityObservation {
            personal_gold_coin: self.personal_gold(character_id),
            condition_status: condition.status,
            hunger: condition.hunger,
            thirst: condition.thirst,
            food_days: condition.food_days,
            water_days: condition.water_days,
            visible_food_kcal,
            visible_water_ml,
            elapsed_minutes,
        })
    }

    /// Total concrete food energy and water volume visible to the character
    /// for a non-inn rest. Public food lots expose nutrition, while public
    /// needs and party state expose physiological, carried, and pooled water.
    fn visible_rest_supplies(&self, character_id: u64) -> (f32, f32) {
        let Some(character) = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
        else {
            return (0.0, 0.0);
        };
        let party_id = character.party_id;
        let personal_ids = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id)
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_ids = party_id.as_deref().map_or_else(HashSet::new, |party_id| {
            self.connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party_id)
                .map(|row| row.id)
                .collect()
        });
        let stored_food_kcal = self
            .connection
            .db
            .food_lot()
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| personal_ids.contains(&id))
                    || lot
                        .party_inventory_item_id
                        .is_some_and(|id| party_ids.contains(&id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum::<f32>();
        let needs = self
            .connection
            .db
            .character_needs()
            .iter()
            .find(|row| row.character_id == character_id);
        let physiological_food = needs
            .as_ref()
            .map_or(0.0, |row| row.food_balance_kcal.max(0.0));
        let personal_water = needs.as_ref().map_or(0.0, |row| {
            row.water_balance_ml.max(0.0) + row.carried_water_ml.max(0.0)
        });
        let party_water = party_id.as_deref().map_or(0.0, |party_id| {
            self.connection
                .db
                .party()
                .iter()
                .find(|row| row.id == party_id)
                .map_or(0.0, |row| row.pooled_water_ml.max(0.0))
        });
        (
            physiological_food + stored_food_kcal,
            personal_water + party_water,
        )
    }

    /// Reproduce the herbalist storefront/reducer quote from the same item
    /// definition and gateway-projected local-problem modifier visible to a
    /// player. No local-problem authority or infection state is transported.
    fn observable_medical_quote(&self, character_id: u64, settlement_id: &str) -> Option<u64> {
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)?;
        if !settlement
            .economy
            .services
            .contains(&SettlementService::Herbalist)
        {
            return None;
        }
        let preparation = self
            .connection
            .db
            .item()
            .iter()
            .find(|row| row.id == "oral_rehydration_draught")?;
        // This is the generated-client equivalent of the public
        // `storefront_stocks(..., Herbalist, ..., Medication)` predicate:
        // the service must exist and the medication's Herbs category must be
        // present in visible settlement stock.
        if !observable_herbalist_stocks_medication(
            true,
            preparation.kind == ItemKind::Medication,
            settlement
                .economy
                .stock
                .iter()
                .any(|row| row.category == StockCategory::Herbs),
        ) {
            return None;
        }
        let buy_bps = self
            .connection
            .db
            .backend_local_problem_trade_effects()
            .iter()
            .find(|row| row.character_id == character_id && row.settlement_id == settlement_id)?
            .buy_bps;
        let base = adventuresim_core::strategic_economy::merchant_buy_price(
            preparation.base_value.unwrap_or(1),
        );
        Some(u64::from(adventuresim_core::local_problem::adjust_price(
            base, buy_bps,
        )))
    }

    fn observable_medical_reserve(&self, character_id: u64, settlement_id: &str) -> Option<u64> {
        let quote = self.observable_medical_quote(character_id, settlement_id)?;
        let settlement = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement_id)?;
        let (food_kcal, _) = self.visible_rest_supplies(character_id);
        let at_inn = affordable_medical_rest_venue(
            settlement
                .economy
                .services
                .contains(&SettlementService::Inn),
            settlement
                .economy
                .services
                .contains(&SettlementService::Temple),
            temple_food_covers_one_day(food_kcal),
            u64::MAX,
            quote,
        )?;
        if at_inn {
            quote.checked_add(adventuresim_core::strategic_economy::inn_full_board_cost(
                1_440,
            )?)
        } else {
            Some(quote)
        }
    }

    fn set_medical_rest_schedule(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        if self.medically_paused_schedules.contains(&character_id) {
            return Ok(());
        }
        let schedule = medical_rest_schedule();
        let result = reducer_call!(self, "pause_schedule_for_treatment", |cb| self
            .connection
            .reducers
            .update_training_schedule_then(
                character_id,
                schedule.clone(),
                medical_rest_schedule(),
                cb
            ));
        self.call(result)?;
        let installed = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == schedule);
        if !installed {
            return Err("medical rest schedule was not authoritatively installed".into());
        }
        self.medically_paused_schedules.insert(character_id);
        Ok(())
    }

    fn restore_profile_schedule(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        if !self.medically_paused_schedules.contains(&character_id) {
            return Ok(());
        }
        let schedule = live_schedule(&self.profiles[agent as usize]);
        let result = reducer_call!(self, "restore_schedule_after_treatment", |cb| self
            .connection
            .reducers
            .update_training_schedule_then(
                character_id,
                schedule.clone(),
                medical_rest_schedule(),
                cb
            ));
        self.call(result)?;
        let restored = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == schedule);
        if !restored {
            return Err("profile schedule was not authoritatively restored".into());
        }
        self.medically_paused_schedules.remove(&character_id);
        Ok(())
    }

    fn install_activity_schedule(
        &mut self,
        character_id: u64,
        schedule: &ScheduleAllocation,
    ) -> Result<(), String> {
        let already_installed = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == *schedule);
        if !already_installed {
            let result = reducer_call!(self, "install_activity_schedule", |cb| self
                .connection
                .reducers
                .update_training_schedule_then(
                    character_id,
                    schedule.clone(),
                    medical_rest_schedule(),
                    cb
                ));
            self.call(result)?;
        }
        let installed = self
            .connection
            .db
            .character_training_schedule()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| row.downtime == *schedule);
        if !installed {
            return Err("activity schedule was not authoritatively installed".into());
        }
        Ok(())
    }

    /// Observe only public condition plus the trusted one-shot herbalist result,
    /// filtered by the simulator-owned patient ID.
    /// Private infection episodes are deliberately absent from this policy.
    fn ensure_medically_safe(&mut self, agent: u32) -> Result<bool, String> {
        let character_id = self.character_ids[agent as usize];
        for _ in 0..MAX_RECOVERY_ACTIONS {
            let character = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing medical character")?;
            if !character.alive {
                self.medically_paused_schedules.remove(&character_id);
                if self.recorded_deaths.insert(character_id) {
                    let source = self
                        .connection
                        .db
                        .character_death()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .map(|row| row.source);
                    if source == Some(DeathSource::Disease) {
                        self.metrics.disease_deaths += 1;
                    }
                    self.event(
                        agent,
                        CoreLoopEventKind::Death,
                        format!("terminal medical state;source={source:?}"),
                    );
                }
                return Ok(false);
            }
            let condition = self
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing medical condition")?;
            let symptomatic = self
                .connection
                .db
                .character_illness_status()
                .iter()
                .find(|row| row.character_id == character_id)
                .is_some_and(|row| row.symptomatic);
            let settlement = character.current_settlement_id.clone();
            let herbalist_available = settlement.as_ref().is_some_and(|settlement| {
                self.connection
                    .db
                    .settlement()
                    .iter()
                    .find(|row| row.id == *settlement)
                    .is_some_and(|row| row.economy.services.contains(&SettlementService::Herbalist))
            });
            let purse = self.personal_gold(character_id);
            let observable_quote = settlement
                .as_deref()
                .and_then(|settlement| self.observable_medical_quote(character_id, settlement));
            let (inn_available, temple_available) =
                settlement.as_ref().map_or((false, false), |settlement_id| {
                    self.connection
                        .db
                        .settlement()
                        .iter()
                        .find(|row| row.id == *settlement_id)
                        .map_or((false, false), |row| {
                            (
                                row.economy.services.contains(&SettlementService::Inn),
                                row.economy.services.contains(&SettlementService::Temple),
                            )
                        })
                });
            let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
            let temple_food_covers_day = temple_food_covers_one_day(visible_food_kcal);
            let inn_cost = adventuresim_core::strategic_economy::inn_full_board_cost(1_440);
            let self_funded_natural_rest_venue = affordable_medical_rest_venue(
                inn_available,
                temple_available,
                temple_food_covers_day,
                purse,
                0,
            );
            let rest_sponsor = if !symptomatic && inn_available {
                settlement.as_deref().and_then(|settlement_id| {
                    inn_cost.and_then(|quote| {
                        self.settlement_rest_sponsor(character_id, settlement_id, quote)
                    })
                })
            } else {
                None
            };
            let emergency_temple_rest = self_funded_natural_rest_venue.is_none()
                && rest_sponsor.is_none()
                && temple_available;
            let natural_rest_venue = self_funded_natural_rest_venue
                .or_else(|| rest_sponsor.as_ref().map(|_| true))
                .or_else(|| emergency_temple_rest.then_some(false));
            let medicated_rest_venue = observable_quote.and_then(|quote| {
                affordable_medical_rest_venue(
                    inn_available,
                    temple_available,
                    temple_food_covers_day,
                    purse,
                    quote,
                )
            });
            let required_rest_cost = medicated_rest_venue
                .or(natural_rest_venue)
                .map(|at_inn| {
                    if at_inn {
                        adventuresim_core::strategic_economy::inn_full_board_cost(1_440)
                    } else {
                        Some(0)
                    }
                })
                .flatten();
            let observable_care_total =
                observable_quote
                    .zip(medicated_rest_venue)
                    .and_then(|(quote, at_inn)| {
                        let rest = if at_inn {
                            adventuresim_core::strategic_economy::inn_full_board_cost(1_440)?
                        } else {
                            0
                        };
                        quote.checked_add(rest)
                    });
            let (choice, reason) = choose_medical_action(
                &condition.status,
                symptomatic,
                settlement.is_some(),
                herbalist_available,
                purse,
                observable_quote,
                natural_rest_venue,
                medicated_rest_venue,
            );
            let selected_rest_venue = match choice {
                MedicalChoice::RestNaturally => natural_rest_venue,
                MedicalChoice::BuyAndRest => medicated_rest_venue,
                MedicalChoice::Ready | MedicalChoice::SuppressQuest => None,
            };
            self.event(
                agent,
                CoreLoopEventKind::MedicalDecision,
                format!(
                    "status={};symptomatic={symptomatic};settlement={};purse={purse};observable_quote={};rest_cost={};care_total={};rest_venue={};temple_food_kcal={visible_food_kcal:.0};temple_water_ml={visible_water_ml:.0};temple_food_covers_day={temple_food_covers_day};emergency_temple_rest={emergency_temple_rest};sponsor={};sponsor_purse={};sponsor_medical_reserve={};sponsor_spendable={};patient_contribution_quote={};sponsor_quote={};party_treasury={};sponsor_stake={};care_affordable={};action={choice:?};reason={reason}",
                    condition.status,
                    settlement.as_deref().unwrap_or("none"),
                    observable_quote.map_or_else(|| "unavailable".into(), |quote| quote.to_string()),
                    required_rest_cost.map_or_else(|| "unavailable".into(), |cost| cost.to_string()),
                    observable_care_total.map_or_else(|| "unavailable".into(), |cost| cost.to_string()),
                    selected_rest_venue.map_or("unavailable", |at_inn| if at_inn { "inn" } else { "temple" }),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.payer_id.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.purse.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.medical_reserve.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.spendable.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.patient_contribution.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.sponsor_quote.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.party_treasury.to_string()),
                    rest_sponsor.as_ref().map_or_else(|| "none".into(), |sponsor| sponsor.party_stake.to_string()),
                    observable_quote.is_some() && medicated_rest_venue.is_some(),
                ),
            );
            if choice == MedicalChoice::Ready {
                self.restore_profile_schedule(agent)?;
                return Ok(true);
            }
            if choice == MedicalChoice::SuppressQuest {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("status={};reason={reason}", condition.status),
                );
                return Ok(false);
            }
            let Some(settlement) = settlement else {
                unreachable!("a missing settlement is handled as quest suppression");
            };
            self.set_medical_rest_schedule(agent)?;
            if choice == MedicalChoice::RestNaturally {
                let at_inn = natural_rest_venue.expect("natural rest choice requires a venue");
                let rest_started_at = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing patient clock before natural recovery rest")?
                    .minutes;
                let actual_rest_minutes = if at_inn
                    && purse < inn_cost.expect("inn venue requires a public quote")
                {
                    let sponsor = rest_sponsor
                        .as_ref()
                        .expect("unaffordable inn venue requires a selected sponsor");
                    let payer_purse_before = sponsor.purse;
                    let patient_purse_before = purse;
                    let condition_before = condition.status.clone();
                    let public_quote = inn_cost.expect("sponsored inn rest requires a quote");
                    let result = reducer_call!(self, "sponsor_party_member_inn_rest", |cb| self
                        .connection
                        .reducers
                        .sponsor_party_member_inn_rest_then(
                            sponsor.payer_id,
                            character_id,
                            settlement.clone(),
                            public_quote,
                            cb
                        ));
                    self.call(result)?;
                    let rest_ended_at = self
                        .connection
                        .db
                        .character_time()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .ok_or("missing patient clock after sponsored recovery rest")?
                        .minutes;
                    let actual_rest_minutes = rest_ended_at.saturating_sub(rest_started_at);
                    let payer_purse_after = self.personal_gold(sponsor.payer_id);
                    let patient_purse_after = self.personal_gold(character_id);
                    let sponsor_spend = payer_purse_before.saturating_sub(payer_purse_after);
                    let patient_spend = patient_purse_before.saturating_sub(patient_purse_after);
                    let actual_spend = sponsor_spend.saturating_add(patient_spend);
                    let condition_after = self
                        .connection
                        .db
                        .character_strategic_condition()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .map_or_else(|| "unavailable".into(), |row| row.status);
                    self.metrics.sponsored_settlement_rests =
                        self.metrics.sponsored_settlement_rests.saturating_add(1);
                    self.metrics.sponsored_settlement_rest_gold_spent = self
                        .metrics
                        .sponsored_settlement_rest_gold_spent
                        .saturating_add(sponsor_spend);
                    self.metrics.sponsored_settlement_rest_requested_minutes = self
                        .metrics
                        .sponsored_settlement_rest_requested_minutes
                        .saturating_add(1_440);
                    self.metrics.sponsored_settlement_rest_elapsed_minutes = self
                        .metrics
                        .sponsored_settlement_rest_elapsed_minutes
                        .saturating_add(actual_rest_minutes);
                    self.metrics.treatment_gold_spent = self
                        .metrics
                        .treatment_gold_spent
                        .saturating_add(actual_spend);
                    self.event(
                        sponsor.payer_agent_id,
                        CoreLoopEventKind::Recover,
                        format!(
                            "sponsored_settlement_rest=completed;payer={};patient={character_id};settlement={};venue=inn;public_quote={public_quote};patient_contribution_quote={};sponsor_quote={};payer_medical_reserve={};payer_spendable={};party_treasury={};payer_party_stake={};patient_spend={patient_spend};sponsor_spend={sponsor_spend};actual_spend={actual_spend};payer_purse_before={payer_purse_before};payer_purse_after={payer_purse_after};patient_purse_before={patient_purse_before};patient_purse_after={patient_purse_after};condition_before={};condition_after={};symptomatic={symptomatic};exposure=not_publicly_projected;requested_minutes=1440;actual_elapsed_minutes={actual_rest_minutes}",
                            sponsor.payer_id,
                            bounded_event_field(&settlement),
                            sponsor.patient_contribution,
                            sponsor.sponsor_quote,
                            sponsor.medical_reserve,
                            sponsor.spendable,
                            sponsor.party_treasury,
                            sponsor.party_stake,
                            bounded_event_field(&condition_before),
                            bounded_event_field(&condition_after),
                        ),
                    );
                    actual_rest_minutes
                } else {
                    let result = reducer_call!(self, "natural_illness_recovery_rest", |cb| self
                        .connection
                        .reducers
                        .rest_at_settlement_hours_then(character_id, 1_440, at_inn, cb));
                    self.call(result)?;
                    let rest_ended_at = self
                        .connection
                        .db
                        .character_time()
                        .iter()
                        .find(|row| row.character_id == character_id)
                        .ok_or("missing patient clock after natural recovery rest")?
                        .minutes;
                    rest_ended_at.saturating_sub(rest_started_at)
                };
                self.metrics.treatment_rest_minutes = self
                    .metrics
                    .treatment_rest_minutes
                    .saturating_add(actual_rest_minutes);
                self.metrics.recovery_rests += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::Recover,
                    format!(
                        "natural_recovery_requested_minutes=1440;natural_recovery_actual_minutes={actual_rest_minutes};venue={};emergency_free_rest={emergency_temple_rest};reason={reason}",
                        if at_inn { "inn" } else { "temple" }
                    ),
                );
                continue;
            }

            // NPCs react only to the public illness status. They may purchase a
            // pre-existing preparation and administer its versioned profile;
            // Physiology never diagnoses or crafts it.
            debug_assert_eq!(choice, MedicalChoice::BuyAndRest);
            let gold_before = purse;
            let preparation_id = "oral_rehydration_draught";
            let result = reducer_call!(self, "purchase_from_herbalist", |cb| self
                .connection
                .reducers
                .purchase_from_herbalist_then(
                    character_id,
                    settlement.clone(),
                    vec![preparation_id.into()],
                    vec![1],
                    cb
                ));
            self.call(result)?;
            self.metrics.preparations_purchased += 1;
            self.event(
                agent,
                CoreLoopEventKind::BuyMedication,
                format!(
                    "item={preparation_id};observable_quote={}",
                    observable_quote.expect("purchase choice requires a quote")
                ),
            );
            let preparation = self
                .connection
                .db
                .inventory_item()
                .iter()
                .find(|row| row.character_id == character_id && row.item_id == preparation_id)
                .ok_or("preparation purchase produced no concrete item")?;
            let result = reducer_call!(self, "administer_preparation", |cb| self
                .connection
                .reducers
                .administer_preparation_then(
                    character_id,
                    character_id,
                    preparation.id,
                    1,
                    "oral".into(),
                    1_000,
                    None,
                    cb
                ));
            self.call(result)?;
            self.metrics.interventions_administered += 1;
            self.event(
                agent,
                CoreLoopEventKind::AdministerPreparation,
                format!("administered={preparation_id};profile=1;route=oral"),
            );
            self.metrics.treatment_gold_spent +=
                gold_before.saturating_sub(self.personal_gold(character_id));

            let at_inn =
                medicated_rest_venue.expect("purchase choice requires an affordable venue");
            let result = reducer_call!(self, "medical_recovery_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, at_inn, cb));
            self.call(result)?;
            self.metrics.treatment_rest_minutes += 1_440;
            self.metrics.recovery_rests += 1;
            self.event(
                agent,
                CoreLoopEventKind::Recover,
                "medical_rest_minutes=1440",
            );
            let after = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing patient after medical rest")?;
            if !after.alive {
                continue;
            }
            let status = self
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing condition after medical rest")?
                .status;
            if status == "ready" {
                let symptomatic_after = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| row.symptomatic);
                if symptomatic_after {
                    continue;
                }
                self.restore_profile_schedule(agent)?;
                self.metrics.illness_recoveries += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::IllnessRecovered,
                    format!(
                        "recovery_context=public_symptoms;condition_before={};condition_after=ready;symptomatic_before={symptomatic};symptomatic_after={symptomatic_after}",
                        condition.status,
                    ),
                );
                return Ok(true);
            }
        }
        self.metrics.stuck_detections += 1;
        Err("medical recovery bound exhausted".into())
    }

    fn settlement_activity_day(&mut self, leader_agent: u32) -> Result<(), String> {
        let leader = self.character_ids[leader_agent as usize];
        for agent in self.party_agents(leader)? {
            if !self.ensure_medically_safe(agent)? {
                continue;
            }
            self.maintain_equipment(agent)?;
            let character_id = self.character_ids[agent as usize];
            let before = self.activity_observation(character_id)?;
            let profile = self.profiles[agent as usize].clone();
            let settlement_id = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .and_then(|row| row.current_settlement_id)
                .ok_or("simulation character is not at a settlement")?;
            let inn_cost = adventuresim_core::strategic_economy::inn_full_board_cost(1_440);
            let committed_reserve = visible_activity_committed_reserve(
                before.personal_gold_coin,
                u64::from(profile.cash_reserve_target),
                self.observable_medical_reserve(character_id, &settlement_id),
                inn_cost,
            );
            let temple_food_covers_day = temple_food_covers_one_day(before.visible_food_kcal);
            let (schedule, effective_activity, fallback_reason) = activity_schedule_plan(
                &profile,
                temple_food_covers_day,
                before.personal_gold_coin,
                committed_reserve,
                inn_cost,
            );
            self.install_activity_schedule(character_id, &schedule)?;
            let venue = self.settlement_activity_venue(character_id, committed_reserve)?;
            let preferred_activity = format!("{:?}", profile.preferred_activity);
            let result = reducer_call!(self, "settlement_activity_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, venue.at_inn(), cb));
            if let Err(error) = result {
                let error_category = safe_core_loop_failure(&error).0;
                self.event(
                    agent,
                    CoreLoopEventKind::Activity,
                    format_failed_activity_detail(
                        &preferred_activity,
                        effective_activity,
                        &schedule,
                        venue,
                        fallback_reason,
                        committed_reserve,
                        &before,
                        error_category,
                    ),
                );
                return self.call(Err(error));
            }
            let after = self.activity_observation(character_id)?;
            self.event(
                agent,
                CoreLoopEventKind::Activity,
                format_activity_detail(
                    &preferred_activity,
                    effective_activity,
                    &schedule,
                    venue,
                    fallback_reason,
                    committed_reserve,
                    &before,
                    &after,
                ),
            );
            self.metrics.activity_days += 1;
            self.ensure_medically_safe(agent)?;
        }
        Ok(())
    }

    /// NPCs use the same custody/rest/retrieval reducers and stable quotes as
    /// players, reserving current personal gold before entrusting work.
    fn maintain_equipment(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        let character = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .ok_or("missing maintenance character")?;
        let Some(settlement) = character.current_settlement_id.clone() else {
            return Ok(());
        };
        if !character.alive {
            return Ok(());
        }
        let repair_service_available = self
            .connection
            .db
            .settlement()
            .iter()
            .find(|row| row.id == settlement)
            .is_some_and(|row| {
                row.economy.services.iter().any(|service| {
                    matches!(
                        service,
                        SettlementService::GeneralBlacksmith
                            | SettlementService::Weaponsmith
                            | SettlementService::Armorer
                            | SettlementService::Tailor
                    )
                })
            });
        if !repair_service_available {
            return Ok(());
        }
        let equipped = self
            .connection
            .db
            .character_equip()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing maintenance equipment state")?;
        let equipped_slots: HashMap<u64, ItemSlot> = [
            (equipped.left_hand_item_id, ItemSlot::LeftHolding),
            (equipped.right_hand_item_id, ItemSlot::RightHolding),
            (equipped.left_arm_armor_id, ItemSlot::LeftArm),
            (equipped.right_arm_armor_id, ItemSlot::RightArm),
            (equipped.left_leg_armor_id, ItemSlot::LeftLeg),
            (equipped.right_leg_armor_id, ItemSlot::RightLeg),
            (equipped.head_armor_id, ItemSlot::Head),
            (equipped.chest_armor_id, ItemSlot::Chest),
            (equipped.stomach_armor_id, ItemSlot::Stomach),
        ]
        .into_iter()
        .filter_map(|(id, slot)| id.map(|id| (id, slot)))
        .collect();
        let now = self
            .connection
            .db
            .character_time()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing maintenance clock")?
            .minutes;
        let medical_reserve = self.observable_medical_reserve(character_id, &settlement);
        let mut repair_budget = spending_budget_after_medical_reserve(
            self.personal_gold(character_id),
            medical_reserve,
        );

        let mut orders: Vec<_> = self
            .connection
            .db
            .repair_order()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.settlement_id == settlement)
            .collect();
        let mut reserved_quotes = self
            .connection
            .db
            .repair_order()
            .iter()
            .filter(|order| order.owner_character_id == character_id)
            .map(|order| (order.ready_at_minutes, order.id, order.quoted_cost))
            .collect::<Vec<_>>();
        reserved_quotes.sort_unstable();
        repair_budget = adventuresim_core::durability::repair_budget_after_reservations(
            repair_budget,
            &reserved_quotes
                .into_iter()
                .map(|(_, _, quote)| quote)
                .collect::<Vec<_>>(),
        );
        if orders.is_empty() {
            let smith = self
                .connection
                .db
                .settlement_smith()
                .iter()
                .find(|row| row.settlement_id == settlement)
                .ok_or("missing settlement smith services")?;
            let mut inventory: Vec<_> = self
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == character_id)
                .collect();
            inventory.sort_by_key(|row| row.id);
            for owned in inventory {
                let Some(definition) = self
                    .connection
                    .db
                    .item()
                    .iter()
                    .find(|row| row.id == owned.item_id)
                else {
                    continue;
                };
                let (skill, service) = match definition.kind {
                    ItemKind::Weapon | ItemKind::Shield => (smith.weaponsmith_skill, "weapons"),
                    ItemKind::Armor => (smith.armourer_skill, "armor"),
                    ItemKind::Clothing => (smith.tailor_skill, "clothing"),
                    _ => continue,
                };
                let Some(condition) = self
                    .connection
                    .db
                    .item_condition()
                    .iter()
                    .find(|row| row.inventory_item_id == owned.id)
                else {
                    continue;
                };
                let bins = [
                    condition.tier_1,
                    condition.tier_2,
                    condition.tier_3,
                    condition.tier_4,
                    condition.tier_5,
                ];
                let total = quantize_smithing_condition(bins.iter().sum());
                let red = quantize_smithing_condition(bins[2..].iter().sum());
                let repairable =
                    quantize_smithing_condition(bins.iter().take(skill as usize).sum());
                let quote = adventuresim_core::durability::repair_quote(
                    definition.base_value.unwrap_or(1),
                    repairable as f32 / SMITHING_DECISION_SCALE,
                );
                // Mild yellow wear is handled automatically by ordinary rest.
                if repairable > 0
                    && (red >= 20 || total >= 350)
                    && u64::from(quote) <= repair_budget
                {
                    let result = reducer_call!(self, "submit_item_for_repair", |cb| self
                        .connection
                        .reducers
                        .submit_item_for_repair_then(
                            character_id,
                            settlement.clone(),
                            service.to_string(),
                            owned.id,
                            cb
                        ));
                    self.call(result)?;
                    self.metrics.repair_submissions += 1;
                    repair_budget -= u64::from(quote);
                    self.event(
                        agent,
                        CoreLoopEventKind::SubmitRepair,
                        format!(
                            "item={};condition={:.3};smith={skill};quote={quote}",
                            owned.item_id,
                            1.0 - total as f32 / SMITHING_DECISION_SCALE
                        ),
                    );
                }
            }
            orders = self
                .connection
                .db
                .repair_order()
                .iter()
                .filter(|row| {
                    row.owner_character_id == character_id && row.settlement_id == settlement
                })
                .collect();
        }
        if orders.is_empty() {
            return Ok(());
        }
        orders.sort_by_key(|order| (order.ready_at_minutes, order.id));
        let mut retrieval_budget = spending_budget_after_medical_reserve(
            self.personal_gold(character_id),
            medical_reserve,
        );
        let affordable: Vec<_> = orders
            .into_iter()
            .filter(|order| {
                let cost = u64::from(order.quoted_cost);
                if cost <= retrieval_budget {
                    retrieval_budget -= cost;
                    true
                } else {
                    false
                }
            })
            .collect();
        if affordable.is_empty() {
            return Ok(());
        }
        let ready_at = affordable
            .iter()
            .map(|order| order.ready_at_minutes)
            .max()
            .unwrap_or(now);
        if ready_at > now {
            let mut remaining = ready_at - now;
            while remaining > 0 {
                let wait = remaining.min(1_440);
                let at_inn = self.settlement_rest_at_inn(character_id)?;
                let result = reducer_call!(self, "wait_for_repairs", |cb| self
                    .connection
                    .reducers
                    .rest_at_settlement_hours_then(character_id, wait, at_inn, cb));
                self.call(result)?;
                self.metrics.repair_wait_minutes += wait;
                self.event(
                    agent,
                    CoreLoopEventKind::WaitForRepair,
                    format!("minutes={wait};orders={}", affordable.len()),
                );
                self.observe_deaths();
                let alive = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == character_id)
                    .is_some_and(|row| row.alive);
                if !alive {
                    return Ok(());
                }
                if !self.ensure_medically_safe(agent)? {
                    return Ok(());
                }
                let current = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing repair wait clock")?
                    .minutes;
                remaining = ready_at.saturating_sub(current);
            }
        }
        for order in affordable {
            let retrieval_character = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == character_id)
                .ok_or("missing repair retrieval character")?;
            if retrieval_character.current_settlement_id.as_deref() != Some(&order.settlement_id) {
                return Err(format!(
                    "repair retrieval location changed: agent={agent};alive={};current={:?};origin={}",
                    retrieval_character.alive,
                    retrieval_character.current_settlement_id,
                    order.settlement_id
                ));
            }
            let current_medical_quote =
                self.observable_medical_reserve(character_id, &order.settlement_id);
            if !equipment_spend_is_still_affordable(
                self.personal_gold(character_id),
                current_medical_quote,
                u64::from(order.quoted_cost),
            ) {
                // Time and medical care can change both the purse and the
                // public local-problem quote while a smith holds the item.
                // Leave the completed order in custody for a later attempt.
                continue;
            }
            let result = reducer_call!(self, "retrieve_repaired_item", |cb| self
                .connection
                .reducers
                .retrieve_repaired_item_then(character_id, order.id, cb));
            self.call(result)?;
            self.metrics.repair_retrievals += 1;
            self.event(
                agent,
                CoreLoopEventKind::RetrieveRepair,
                format!(
                    "item={};order={};cost={}",
                    order.item_id, order.id, order.quoted_cost
                ),
            );
            if let Some(slot) = equipped_slots.get(&order.inventory_item_id).copied() {
                let result = reducer_call!(self, "reequip_repaired_item", |cb| self
                    .connection
                    .reducers
                    .equip_item_then(character_id, order.inventory_item_id, slot, cb));
                self.call(result)?;
                let verified = self
                    .connection
                    .db
                    .character_equip()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| equipped_at(&row, slot, order.inventory_item_id));
                if !verified {
                    return Err("repaired equipped item was not authoritatively re-equipped".into());
                }
                self.event(
                    agent,
                    CoreLoopEventKind::Equip,
                    format!("repaired={}", order.item_id),
                );
            }
        }
        Ok(())
    }

    fn party_agents(&self, leader: u64) -> Result<Vec<u32>, String> {
        let party = self.party_for(leader)?;
        let mut agents: Vec<_> = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party.id)
            .filter(|member| {
                self.connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == member.character_id)
                    .is_some_and(|row| row.alive)
            })
            .filter_map(|member| {
                self.character_ids
                    .iter()
                    .position(|id| *id == member.character_id)
                    .map(|index| index as u32)
            })
            .collect();
        agents.sort_unstable();
        Ok(agents)
    }

    fn unsafe_party_agents(&self, agents: &[u32]) -> Vec<u32> {
        let mut unsafe_agents = agents
            .iter()
            .copied()
            .filter(|agent| {
                let id = self.character_ids[*agent as usize];
                let alive = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == id)
                    .is_some_and(|row| row.alive);
                let ready = self
                    .connection
                    .db
                    .character_strategic_condition()
                    .iter()
                    .find(|row| row.character_id == id)
                    .is_some_and(|row| row.status == "ready")
                    && !self
                        .connection
                        .db
                        .character_illness_status()
                        .iter()
                        .find(|row| row.character_id == id)
                        .is_some_and(|row| row.symptomatic || row.critical);
                !alive || !ready
            })
            .collect::<Vec<_>>();
        unsafe_agents.sort_unstable();
        unsafe_agents
    }

    fn expedition_member_observations(
        &self,
        party_id: &str,
    ) -> Result<Vec<ExpeditionMemberObservation>, String> {
        let mut member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<Vec<_>>();
        member_ids.sort_unstable();
        member_ids
            .into_iter()
            .map(|character_id| {
                let agent_id = self
                    .character_ids
                    .iter()
                    .position(|id| *id == character_id)
                    .ok_or("expedition member is outside the simulator roster")?
                    as u32;
                let character = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == character_id)
                    .ok_or("expedition member projection is unavailable")?;
                let condition = self
                    .connection
                    .db
                    .character_strategic_condition()
                    .iter()
                    .find(|row| row.character_id == character_id);
                let illness = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id);
                let elapsed_minutes = self
                    .connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .map_or(0, |row| row.minutes);
                Ok(ExpeditionMemberObservation {
                    agent_id,
                    character_id,
                    alive: character.alive,
                    condition_status: condition
                        .as_ref()
                        .map_or_else(|| "unavailable".into(), |row| row.status.clone()),
                    hunger: condition.as_ref().map_or(0.0, |row| row.hunger),
                    thirst: condition.as_ref().map_or(0.0, |row| row.thirst),
                    food_days: condition.as_ref().map_or(0.0, |row| row.food_days),
                    water_days: condition.as_ref().map_or(0.0, |row| row.water_days),
                    symptomatic: illness.as_ref().is_some_and(|row| row.symptomatic),
                    critical: illness.as_ref().is_some_and(|row| row.critical),
                    elapsed_minutes,
                })
            })
            .collect()
    }

    fn expedition_supplies(&self, party_id: &str) -> ExpeditionSuppliesObservation {
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<HashSet<_>>();
        let personal_inventory_ids = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| member_ids.contains(&row.character_id))
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let party_inventory_ids = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let stored_food_kcal = self
            .connection
            .db
            .food_lot()
            .iter()
            .filter(|lot| {
                lot.inventory_item_id
                    .is_some_and(|id| personal_inventory_ids.contains(&id))
                    || lot
                        .party_inventory_item_id
                        .is_some_and(|id| party_inventory_ids.contains(&id))
            })
            .map(|lot| lot.nutrition_kcal.max(0.0))
            .sum();
        let carried_water_ml = self
            .connection
            .db
            .character_needs()
            .iter()
            .filter(|needs| member_ids.contains(&needs.character_id))
            .map(|needs| needs.carried_water_ml.max(0.0))
            .sum::<f32>();
        let pooled_water_ml = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)
            .map_or(0.0, |party| party.pooled_water_ml.max(0.0));
        ExpeditionSuppliesObservation {
            stored_food_kcal,
            portable_water_ml: carried_water_ml + pooled_water_ml,
        }
    }

    fn emit_expedition_diagnostics(
        &mut self,
        party_id: &str,
        phase: &str,
        action: &str,
        reason: &str,
        before: &[ExpeditionMemberObservation],
        after: &[ExpeditionMemberObservation],
        supplies_before: ExpeditionSuppliesObservation,
        supplies_after: ExpeditionSuppliesObservation,
    ) {
        for member_before in before {
            let member_after = after
                .iter()
                .find(|candidate| candidate.character_id == member_before.character_id)
                .unwrap_or(member_before);
            self.event(
                member_before.agent_id,
                CoreLoopEventKind::ExpeditionRecovery,
                format!(
                    "party={};phase={};action={};reason={};member={};alive_before={};alive_after={};condition_before={};condition_after={};hunger_before={:.3};hunger_after={:.3};thirst_before={:.3};thirst_after={:.3};food_days_before={:.2};food_days_after={:.2};water_days_before={:.2};water_days_after={:.2};symptomatic_before={};symptomatic_after={};critical_before={};critical_after={};exposure=not_publicly_projected;elapsed_before={};elapsed_after={};elapsed_delta={};stored_food_kcal_before={:.0};stored_food_kcal_after={:.0};stored_food_kcal_consumed={:.0};portable_water_ml_before={:.0};portable_water_ml_after={:.0};portable_water_ml_consumed={:.0}",
                    bounded_event_field(party_id),
                    bounded_event_field(phase),
                    bounded_event_field(action),
                    bounded_event_field(reason),
                    member_before.character_id,
                    member_before.alive,
                    member_after.alive,
                    bounded_event_field(&member_before.condition_status),
                    bounded_event_field(&member_after.condition_status),
                    member_before.hunger,
                    member_after.hunger,
                    member_before.thirst,
                    member_after.thirst,
                    member_before.food_days,
                    member_after.food_days,
                    member_before.water_days,
                    member_after.water_days,
                    member_before.symptomatic,
                    member_after.symptomatic,
                    member_before.critical,
                    member_after.critical,
                    member_before.elapsed_minutes,
                    member_after.elapsed_minutes,
                    member_after
                        .elapsed_minutes
                        .saturating_sub(member_before.elapsed_minutes),
                    supplies_before.stored_food_kcal,
                    supplies_after.stored_food_kcal,
                    (supplies_before.stored_food_kcal - supplies_after.stored_food_kcal).max(0.0),
                    supplies_before.portable_water_ml,
                    supplies_after.portable_water_ml,
                    (supplies_before.portable_water_ml - supplies_after.portable_water_ml).max(0.0),
                ),
            );
        }
    }

    fn record_journey_hold(
        &mut self,
        party_id: &str,
        phase: &str,
        reason: &str,
    ) -> Result<JourneyTravelOutcome, String> {
        let party = self.party_by_id(party_id)?;
        let members = self.expedition_member_observations(party_id)?;
        let supplies = self.expedition_supplies(party_id);
        let living_count = members.iter().filter(|member| member.alive).count() as u32;
        let required_food_kcal =
            living_count as f32 * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY;
        let required_water_ml = living_count as f32
            * adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY;
        let supplies_cover_one_day = expedition_supplies_cover_one_rest_day(&members, supplies);
        let journey = self
            .connection
            .db
            .party_journey()
            .iter()
            .find(|row| row.party_id == party_id);
        let itinerary = self
            .connection
            .db
            .party_journey_itinerary()
            .iter()
            .find(|row| row.party_id == party_id);
        let active_interval = journey.as_ref().and_then(|journey| {
            itinerary.as_ref().and_then(|itinerary| {
                projected_camp_rest_minutes(
                    journey.completed_elapsed_minutes,
                    journey.total_elapsed_minutes,
                    &itinerary.forecast_camp_intervals,
                )
            })
        });
        let journey_completed_elapsed = journey.as_ref().map_or_else(
            || "none".into(),
            |row| row.completed_elapsed_minutes.to_string(),
        );
        let journey_total_elapsed = journey.as_ref().map_or_else(
            || "none".into(),
            |row| row.total_elapsed_minutes.to_string(),
        );
        let journey_remaining_elapsed = journey.as_ref().map_or_else(
            || "none".into(),
            |row| {
                row.total_elapsed_minutes
                    .saturating_sub(row.completed_elapsed_minutes)
                    .to_string()
            },
        );
        let journey_destination = journey.as_ref().map_or_else(
            || {
                party
                    .camp_destination
                    .as_ref()
                    .map_or_else(|| "none".into(), public_journey_endpoint)
            },
            |row| public_journey_endpoint(&row.destination),
        );
        let (active_interval_start, active_interval_minutes) = active_interval.map_or_else(
            || ("none".into(), "none".into()),
            |(start, minutes)| (start.to_string(), minutes.to_string()),
        );
        self.metrics.expedition_holds = self.metrics.expedition_holds.saturating_add(1);
        let diagnostic_agent = members.first().map_or(0, |member| member.agent_id);
        self.event(
            diagnostic_agent,
            CoreLoopEventKind::ExpeditionRecovery,
            format!(
                "party={};phase={};action=hold_position;reason={};journey_completed_elapsed={journey_completed_elapsed};journey_total_elapsed={journey_total_elapsed};journey_remaining_elapsed={journey_remaining_elapsed};journey_destination={};camp_remaining_minutes={};active_forecast_interval_start={active_interval_start};active_forecast_interval_minutes={active_interval_minutes};living_count={living_count};one_day_food_kcal_required={required_food_kcal:.0};stored_food_kcal={:.0};one_day_water_ml_required={required_water_ml:.0};portable_water_ml={:.0};supplies_cover_one_rest_day={supplies_cover_one_day}",
                bounded_event_field(party_id),
                bounded_event_field(phase),
                bounded_event_field(reason),
                bounded_event_field(&journey_destination),
                party.camp_remaining_minutes,
                supplies.stored_food_kcal,
                supplies.portable_water_ml,
            ),
        );
        self.emit_expedition_diagnostics(
            party_id,
            phase,
            "hold_position",
            reason,
            &members,
            &members,
            supplies,
            supplies,
        );
        Ok(JourneyTravelOutcome::HeldNoActionableActor)
    }

    fn expedition_recovery_actor(&self, party_id: &str) -> Option<(u64, u32, &'static str)> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let mut ready = self
            .expedition_member_observations(party_id)
            .ok()?
            .into_iter()
            .filter(|member| {
                member.alive
                    && member.condition_status == "ready"
                    && !member.symptomatic
                    && !member.critical
            })
            .collect::<Vec<_>>();
        ready.sort_by_key(|member| (member.character_id != party.leader_id, member.character_id));
        if let Some(actor) = ready.into_iter().next() {
            let role = if actor.character_id == party.leader_id {
                "ready_leader"
            } else {
                "ready_companion"
            };
            return Some((actor.character_id, actor.agent_id, role));
        }
        None
    }

    fn expedition_recovery_rest_actor(
        &self,
        party_id: &str,
    ) -> Option<ExpeditionRecoveryRestActor> {
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let members = self.expedition_member_observations(party_id).ok()?;
        let supplies = self.expedition_supplies(party_id);
        if self.party_has_unresolved_public_encounter(party_id)
            || self.public_active_camp_observation(party_id).is_none()
        {
            return None;
        }
        if let Some((character_id, agent_id, role)) = self.expedition_recovery_actor(party_id) {
            return Some(ExpeditionRecoveryRestActor::Actionable(
                ActionableRecoveryRestActor {
                    character_id,
                    agent_id,
                    role,
                },
            ));
        }
        if !passive_no_actionable_rest_allowed(
            &members,
            supplies,
            party.current_settlement_id.is_none(),
            true,
            party.leader_id,
            false,
        ) {
            return None;
        }
        let leader = members
            .iter()
            .find(|member| member.character_id == party.leader_id && member.alive)?;
        Some(ExpeditionRecoveryRestActor::PassiveNoActionable(
            PassiveNoActionableRestActor {
                leader_id: leader.character_id,
                agent_id: leader.agent_id,
            },
        ))
    }

    fn perform_expedition_recovery_rest(
        &mut self,
        actor: ExpeditionRecoveryRestActor,
    ) -> Result<(), String> {
        let (character_id, operation) = match actor {
            ExpeditionRecoveryRestActor::Actionable(actor) => {
                (actor.character_id, "expedition_recovery_rest")
            }
            ExpeditionRecoveryRestActor::PassiveNoActionable(actor) => {
                (actor.leader_id, "passive_no_actionable_rest")
            }
        };
        let result = reducer_call!(self, operation, |cb| self
            .connection
            .reducers
            .rest_at_camp_then(character_id, EXPEDITION_RECOVERY_REST_MINUTES, cb));
        self.call(result)
    }

    fn public_expedition_return_settlement(&self, party_id: &str) -> Option<String> {
        if let Some(journey) = self
            .connection
            .db
            .party_journey()
            .iter()
            .find(|journey| journey.party_id == party_id)
        {
            if let JourneyEndpoint::Settlement(origin) = journey.origin {
                return Some(origin.id);
            }
            if let JourneyEndpoint::Settlement(destination) = journey.destination {
                return Some(destination.id);
            }
        }
        let party = self
            .connection
            .db
            .party()
            .iter()
            .find(|party| party.id == party_id)?;
        let current_site = party.current_case_site_id.as_ref()?.value.as_str();
        let member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|membership| membership.party_id == party_id)
            .map(|membership| membership.character_id)
            .collect::<HashSet<_>>();
        let mut origins = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|pin| {
                member_ids.contains(&pin.owner_character_id) && pin.case_site_id == current_site
            })
            .map(|pin| pin.origin_settlement_id)
            .collect::<Vec<_>>();
        origins.sort();
        origins.dedup();
        if origins.len() == 1 {
            return origins.pop();
        }
        None
    }

    fn public_journey_is_evacuation(&self, party_id: &str) -> bool {
        let Some(return_settlement) = self.public_expedition_return_settlement(party_id) else {
            return false;
        };
        self.connection
            .db
            .party_journey()
            .iter()
            .find(|journey| journey.party_id == party_id)
            .is_some_and(|journey| {
                matches!(
                    journey.destination,
                    JourneyEndpoint::Settlement(destination)
                        if destination.id == return_settlement
                )
            })
    }

    fn recover_or_evacuate_off_settlement(
        &mut self,
        party_id: &str,
        cycle: u32,
    ) -> Result<ExpeditionRecoveryOutcome, String> {
        let party = self.party_by_id(party_id)?;
        if party.current_settlement_id.is_some() {
            return Ok(ExpeditionRecoveryOutcome::None);
        }
        let mut before = self.expedition_member_observations(party_id)?;
        if !before.iter().any(expedition_member_needs_recovery) {
            return Ok(ExpeditionRecoveryOutcome::None);
        }
        let supplies_before = self.expedition_supplies(party_id);
        self.metrics.expedition_recovery_plans =
            self.metrics.expedition_recovery_plans.saturating_add(1);
        self.metrics.quests_suppressed_for_health =
            self.metrics.quests_suppressed_for_health.saturating_add(
                before
                    .iter()
                    .filter(|member| expedition_member_needs_recovery(member))
                    .count() as u32,
            );
        if self.party_has_unresolved_public_encounter(party_id) {
            self.record_journey_hold(
                party_id,
                "recovery_plan",
                "journey_held_unresolved_encounter",
            )?;
            self.emit_expedition_diagnostics(
                party_id,
                "plan",
                "hold_position",
                "journey_held_unresolved_encounter",
                &before,
                &before,
                supplies_before,
                supplies_before,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        let actionable_actor = self.expedition_recovery_actor(party_id);
        let coherent_camp = self.public_active_camp_observation(party_id);
        if party.camp_destination.is_some() && coherent_camp.is_none() {
            self.record_journey_hold(
                party_id,
                "recovery_plan",
                "journey_held_incoherent_public_camp",
            )?;
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        let plan_actor = coherent_camp
            .and_then(|_| self.expedition_recovery_rest_actor(party_id))
            .or_else(|| {
                actionable_actor.map(|(character_id, agent_id, role)| {
                    ExpeditionRecoveryRestActor::Actionable(ActionableRecoveryRestActor {
                        character_id,
                        agent_id,
                        role,
                    })
                })
            });
        let Some(plan_actor) = plan_actor else {
            self.record_journey_hold(party_id, "recovery_plan", "journey_held_no_recovery_actor")?;
            self.emit_expedition_diagnostics(
                party_id,
                "plan",
                "hold_position",
                "journey_held_no_recovery_actor",
                &before,
                &before,
                supplies_before,
                supplies_before,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        };
        let actor_id = plan_actor.character_id();
        let actor_agent = plan_actor.agent_id();
        let actor_role = plan_actor.role();
        self.emit_expedition_diagnostics(
            party_id,
            "plan",
            "field_recovery_then_evacuation",
            &format!("quest_suppressed_off_settlement_health_cycle_{cycle}_{actor_role}"),
            &before,
            &before,
            supplies_before,
            supplies_before,
        );
        self.event(
            actor_agent,
            CoreLoopEventKind::QuestSuppressed,
            format!(
                "cycle={cycle};reason=off_settlement_member_not_ready;plan=field_recovery_then_evacuation;actor={actor_id};actor_role={actor_role}"
            ),
        );

        let can_attempt_field_recovery = coherent_camp.is_some()
            && before
                .iter()
                .all(|member| !member.alive || !member.critical)
            && expedition_supplies_cover_one_rest_day(&before, supplies_before);
        if can_attempt_field_recovery {
            for rest_ordinal in 1..=MAX_EXPEDITION_RECOVERY_RESTS {
                if self.party_has_unresolved_public_encounter(party_id) {
                    self.record_journey_hold(
                        party_id,
                        "field_recovery_actor_reselection",
                        "journey_held_unresolved_encounter",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                }
                let party_before_rest = self.party_by_id(party_id)?;
                if party_before_rest.camp_destination.is_some()
                    && self.public_active_camp_observation(party_id).is_none()
                {
                    self.record_journey_hold(
                        party_id,
                        "field_recovery_actor_reselection",
                        "journey_held_incoherent_public_camp",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                }
                let Some(rest_actor) = self.expedition_recovery_rest_actor(party_id) else {
                    self.record_journey_hold(
                        party_id,
                        "field_recovery_actor_reselection",
                        "journey_held_no_recovery_actor",
                    )?;
                    return Ok(ExpeditionRecoveryOutcome::Held);
                };
                let rest_before = self.expedition_member_observations(party_id)?;
                let rest_supplies_before = self.expedition_supplies(party_id);
                if rest_actor.is_passive() {
                    self.metrics.expedition_passive_rest_attempts = self
                        .metrics
                        .expedition_passive_rest_attempts
                        .saturating_add(1);
                }
                self.perform_expedition_recovery_rest(rest_actor)?;
                self.observe_deaths();
                let rest_after = self.expedition_member_observations(party_id)?;
                let rest_supplies_after = self.expedition_supplies(party_id);
                let actual_elapsed_minutes = expedition_elapsed_delta(&rest_before, &rest_after);
                self.metrics.expedition_recovery_rests =
                    self.metrics.expedition_recovery_rests.saturating_add(1);
                self.metrics.recovery_rests = self.metrics.recovery_rests.saturating_add(1);
                if rest_actor.is_passive() {
                    self.metrics.expedition_passive_rest_minutes = self
                        .metrics
                        .expedition_passive_rest_minutes
                        .saturating_add(actual_elapsed_minutes);
                    self.event(
                        rest_actor.agent_id(),
                        CoreLoopEventKind::ExpeditionRecovery,
                        format!(
                            "party={};phase=passive_no_actionable_rest;action=rest_at_camp;rest_attempt={rest_ordinal};leader={};requested_minutes={EXPEDITION_RECOVERY_REST_MINUTES};actual_elapsed_minutes={actual_elapsed_minutes}",
                            bounded_event_field(party_id),
                            rest_actor.character_id(),
                        ),
                    );
                }
                self.emit_expedition_diagnostics(
                    party_id,
                    "field_rest",
                    "rest_at_camp",
                    &if rest_actor.is_passive() {
                        format!("passive_no_actionable_rest_attempt_{rest_ordinal}")
                    } else {
                        format!("bounded_recovery_rest_{rest_ordinal}")
                    },
                    &rest_before,
                    &rest_after,
                    rest_supplies_before,
                    rest_supplies_after,
                );
                if expedition_party_can_resume(&rest_after) {
                    self.metrics.expedition_resumes =
                        self.metrics.expedition_resumes.saturating_add(1);
                    self.emit_expedition_diagnostics(
                        party_id,
                        "resume",
                        "resume_expedition",
                        "quest_resumed_all_members_ready_and_asymptomatic",
                        &rest_after,
                        &rest_after,
                        rest_supplies_after,
                        rest_supplies_after,
                    );
                    return Ok(ExpeditionRecoveryOutcome::Resumed);
                }
                before = rest_after;
                if before.iter().any(|member| member.alive && member.critical)
                    || !expedition_supplies_cover_one_rest_day(&before, rest_supplies_after)
                {
                    break;
                }
            }
        }

        let Some(return_settlement) = self.public_expedition_return_settlement(party_id) else {
            let supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                party_id,
                "evacuation",
                "hold_position",
                "no_public_return_route",
                &before,
                &before,
                supplies_after,
                supplies_after,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        };
        let evacuation_before = self.expedition_member_observations(party_id)?;
        let evacuation_supplies_before = self.expedition_supplies(party_id);
        let Some((evacuation_actor_id, evacuation_actor_agent, evacuation_actor_role)) =
            self.expedition_recovery_actor(party_id)
        else {
            self.record_journey_hold(
                party_id,
                "evacuation_plan",
                "journey_held_no_evacuation_actor",
            )?;
            return Ok(ExpeditionRecoveryOutcome::Held);
        };
        self.emit_expedition_diagnostics(
            party_id,
            "evacuation_plan",
            "return_to_settlement",
            "quest_suppressed_recovery_incomplete",
            &evacuation_before,
            &evacuation_before,
            evacuation_supplies_before,
            evacuation_supplies_before,
        );
        let result = reducer_call!(self, "expedition_health_evacuation", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(evacuation_actor_id, return_settlement.clone(), cb));
        self.call(result)?;
        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        self.observe_deaths();
        let evacuation_after = self.expedition_member_observations(party_id)?;
        let evacuation_supplies_after = self.expedition_supplies(party_id);
        let evacuation_party = self.party_by_id(party_id)?;
        let evacuation_complete = evacuation_party.current_settlement_id.as_deref()
            == Some(return_settlement.as_str())
            && evacuation_party.camp_destination.is_none()
            && evacuation_after.iter().any(|member| member.alive);
        if !evacuation_complete {
            self.emit_expedition_diagnostics(
                party_id,
                "evacuation_stalled",
                "return_to_settlement",
                "public_state_does_not_prove_living_party_returned",
                &evacuation_before,
                &evacuation_after,
                evacuation_supplies_before,
                evacuation_supplies_after,
            );
            return Ok(ExpeditionRecoveryOutcome::Held);
        }
        self.metrics.expedition_evacuations = self.metrics.expedition_evacuations.saturating_add(1);
        self.event(
            evacuation_actor_agent,
            CoreLoopEventKind::ExpeditionRecovery,
            format!(
                "party={};phase=evacuation_authority;actor={evacuation_actor_id};actor_role={evacuation_actor_role};destination={}",
                bounded_event_field(party_id),
                bounded_event_field(&return_settlement),
            ),
        );
        self.emit_expedition_diagnostics(
            party_id,
            "evacuation_complete",
            "return_to_settlement",
            "quest_suppressed_settlement_recovery_required",
            &evacuation_before,
            &evacuation_after,
            evacuation_supplies_before,
            evacuation_supplies_after,
        );
        Ok(ExpeditionRecoveryOutcome::Evacuated)
    }

    fn owned_open_generated_cases(&self, character_id: u64) -> Vec<(String, String)> {
        stable_owned_open_cases(
            character_id,
            self.connection
                .db
                .backend_investigation_cases()
                .iter()
                .map(|row| (row.owner_character_id, row.case_id, row.subject, row.status)),
        )
    }

    fn generated_case_status(&self, character_id: u64, case_id: &str) -> Option<String> {
        self.connection
            .db
            .backend_investigation_cases()
            .iter()
            .find(|row| row.owner_character_id == character_id && row.case_id == case_id)
            .map(|row| row.status)
    }

    fn observe_generated_case_intake(
        &mut self,
        agent: u32,
        owner_character_id: u64,
        case_id: &str,
        subject: &str,
        source: &str,
    ) -> bool {
        let key = (owner_character_id, case_id.to_owned());
        if !self.generated_seen_cases.insert(key) {
            return false;
        }
        self.metrics.generated_case_intakes = self.metrics.generated_case_intakes.saturating_add(1);
        self.metrics.quests_attempted = self.metrics.quests_attempted.saturating_add(1);
        if source == "owner_projection_continuation" {
            self.metrics.generated_case_continuations =
                self.metrics.generated_case_continuations.saturating_add(1);
        }
        self.event(
            agent,
            CoreLoopEventKind::GeneratedCaseIntake,
            format!(
                "owner={owner_character_id};case={};subject={};source={}",
                bounded_event_field(case_id),
                bounded_event_field(subject),
                bounded_event_field(source),
            ),
        );
        true
    }

    fn observe_generated_case_transition(
        &mut self,
        agent: u32,
        character_id: u64,
        case_id: &str,
        title: &str,
        immediately_after_own_action: bool,
    ) {
        let key = (character_id, case_id.to_owned());
        if self.generated_terminal_cases.contains(&key) {
            return;
        }
        let attribution = generated_closure_attribution(
            "open",
            self.generated_case_status(character_id, case_id).as_deref(),
            immediately_after_own_action,
        );
        match attribution {
            GeneratedClosureAttribution::StillOpen => {}
            GeneratedClosureAttribution::OwnImmediateTransition => {
                self.generated_terminal_cases.insert(key);
                self.metrics.generated_quests_completed += 1;
                self.metrics.quests_completed += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedQuestCompleted,
                    format!(
                        "case={};subject={};attribution=own_immediate_transition",
                        bounded_event_field(case_id),
                        bounded_event_field(title)
                    ),
                );
            }
            GeneratedClosureAttribution::ExternalTransition => {
                self.generated_terminal_cases.insert(key);
                self.metrics.generated_quests_closed_externally += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedQuestClosedExternally,
                    format!(
                        "case={};subject={};attribution=external_transition",
                        bounded_event_field(case_id),
                        bounded_event_field(title)
                    ),
                );
            }
        }
    }

    fn observe_external_generated_closures(&mut self) {
        let tracked = self
            .generated_seen_cases
            .iter()
            .map(|(owner, case_id)| (case_id.clone(), *owner))
            .collect::<Vec<_>>();
        for (case_id, owner) in tracked {
            let Some(agent) = self.character_ids.iter().position(|id| *id == owner) else {
                continue;
            };
            let title = self
                .connection
                .db
                .backend_investigation_cases()
                .iter()
                .find(|row| row.owner_character_id == owner && row.case_id == case_id)
                .map_or_else(|| "Unlabelled problem".into(), |row| row.subject);
            self.observe_generated_case_transition(agent as u32, owner, &case_id, &title, false);
        }
    }

    fn visible_npc_candidates(
        &self,
        character_id: u64,
        preferred_name: Option<&str>,
        preferred_location: Option<&str>,
    ) -> Vec<PublicNpcCandidate> {
        let Some(character) = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
        else {
            return Vec::new();
        };
        let Some(settlement_id) = character.current_settlement_id else {
            return Vec::new();
        };
        let minute = self
            .connection
            .db
            .character_time()
            .iter()
            .find(|row| row.character_id == character_id)
            .map_or(720, |row| row.minutes);
        let candidates = self
            .connection
            .db
            .settlement_npc_presence()
            .iter()
            .filter(|presence| {
                presence.settlement_id == settlement_id
                    && npc_is_publicly_present(presence.start_minute, presence.end_minute, minute)
            })
            .filter_map(|presence| {
                self.connection
                    .db
                    .backend_settlement_npcs()
                    .iter()
                    .find(|npc| {
                        npc.id == presence.npc_id && npc.home_settlement_id == settlement_id
                    })
                    .map(|npc| PublicNpcCandidate {
                        npc_id: npc.id,
                        name: npc.name,
                        profession: npc.profession,
                        conversation_id: npc.conversation_id,
                        location_id: presence.location_id,
                    })
            })
            .collect();
        stable_public_npc_candidates(candidates, preferred_name, preferred_location)
    }

    fn start_public_dialogue(
        &mut self,
        character_id: u64,
        cycle: u32,
        candidate: &PublicNpcCandidate,
        purpose: &str,
    ) -> Result<String, String> {
        self.dialogue_nonce = self.dialogue_nonce.saturating_add(1);
        let session_id = format!(
            "dialogue:{character_id}:sim-{cycle}-{}-{purpose}",
            self.dialogue_nonce
        );
        let result = reducer_call!(self, "start_dialogue", |cb| self
            .connection
            .reducers
            .start_dialogue_then(
                character_id,
                session_id.clone(),
                candidate.conversation_id.clone(),
                candidate.npc_id.clone(),
                candidate.location_id.clone(),
                adventuresim_dialogue::CATALOG_DIGEST.to_owned(),
                cb,
            ));
        self.call(result)?;
        let session_is_owned = self
            .connection
            .db
            .backend_dialogue_sessions()
            .iter()
            .any(|row| row.id == session_id && row.owner_character_id == character_id);
        if !session_is_owned {
            return Err("dialogue reducer completed without an owner-scoped session".into());
        }
        Ok(session_id)
    }

    fn discover_generated_case(
        &mut self,
        character_id: u64,
        agent: u32,
        cycle: u32,
    ) -> Result<GeneratedDiscoveryOutcome, String> {
        let before = self
            .connection
            .db
            .backend_investigation_cases()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.status == "open")
            .map(|row| row.case_id)
            .collect::<HashSet<_>>();
        let candidates = self.visible_npc_candidates(character_id, None, None);
        let visible_candidate_count = candidates.len();
        let Some(candidate) = stable_discovery_action_candidate(candidates) else {
            self.metrics.generated_discovery_decisions_unproductive = self
                .metrics
                .generated_discovery_decisions_unproductive
                .saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "visible_candidate_count={visible_candidate_count};candidate_id=none;candidate_name=none;location=none;dialogue_success=false;session_success=false;open_cases_before={};open_cases_after={};new_open_cases=0;rumor_delivered=false;result=unproductive;reason=no_visible_contacts;fallback=no_visible_contacts;activity_fallback=true",
                    before.len(),
                    before.len(),
                ),
            );
            return Ok(GeneratedDiscoveryOutcome::NoVisibleContacts);
        };

        self.metrics.generated_discovery_actions_attempted = self
            .metrics
            .generated_discovery_actions_attempted
            .saturating_add(1);
        self.event(
            agent,
            CoreLoopEventKind::GeneratedDiscoveryAttempt,
            format!(
                "visible_candidate_count={visible_candidate_count};candidate_id={};candidate_name={};location={};open_cases_before={}",
                bounded_event_field(&candidate.npc_id),
                bounded_event_field(&candidate.name),
                bounded_event_field(&candidate.location_id),
                before.len(),
            ),
        );
        if let Err(error) = self.start_public_dialogue(character_id, cycle, &candidate, "discover")
        {
            let dialogue_succeeded =
                error == "dialogue reducer completed without an owner-scoped session";
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "visible_candidate_count={visible_candidate_count};candidate_id={};candidate_name={};location={};dialogue_success={dialogue_succeeded};session_success=false;open_cases_before={};open_cases_after={};new_open_cases=0;rumor_delivered=false;result=failed;reason={};fallback=none;activity_fallback=false",
                    bounded_event_field(&candidate.npc_id),
                    bounded_event_field(&candidate.name),
                    bounded_event_field(&candidate.location_id),
                    before.len(),
                    before.len(),
                    if dialogue_succeeded {
                        "session_projection_missing"
                    } else {
                        "dialogue_failed"
                    },
                ),
            );
            return Err(if dialogue_succeeded {
                "start_discovery_dialogue failed: owner-scoped dialogue session unavailable".into()
            } else {
                "start_discovery_dialogue failed: public discovery contact failed".into()
            });
        }

        // The owner-scoped open-case projection is the public postcondition of
        // receiving a generated rumor. It avoids inspecting private delivery
        // receipts or generation eligibility.
        let after = self.owned_open_generated_cases(character_id);
        let mut discovered = after
            .iter()
            .filter(|(case_id, _)| !before.contains(case_id))
            .cloned()
            .collect::<Vec<_>>();
        discovered.sort();
        let new_open_cases = discovered.len();
        if let Some((case_id, subject)) = discovered.into_iter().next() {
            self.metrics.generated_discovery_actions_fruitful = self
                .metrics
                .generated_discovery_actions_fruitful
                .saturating_add(1);
            self.event(
                agent,
                CoreLoopEventKind::GeneratedDiscoveryResult,
                format!(
                    "visible_candidate_count={visible_candidate_count};candidate_id={};candidate_name={};location={};dialogue_success=true;session_success=true;open_cases_before={};open_cases_after={};new_open_cases={new_open_cases};rumor_delivered=true;result=fruitful;reason=rumor_delivered;fallback=none;activity_fallback=false",
                    bounded_event_field(&candidate.npc_id),
                    bounded_event_field(&candidate.name),
                    bounded_event_field(&candidate.location_id),
                    before.len(),
                    after.len(),
                ),
            );
            self.observe_generated_case_intake(
                agent,
                character_id,
                &case_id,
                &subject,
                "dialogue_rumor",
            );
            self.metrics.generated_quests_discovered += 1;
            self.metrics.generated_unique_party_cases_discovered += 1;
            self.event(
                agent,
                CoreLoopEventKind::GeneratedQuestDiscovered,
                format!(
                    "case={};subject={};npc={};location={}",
                    bounded_event_field(&case_id),
                    bounded_event_field(&subject),
                    bounded_event_field(&candidate.name),
                    bounded_event_field(&candidate.location_id)
                ),
            );
            return Ok(GeneratedDiscoveryOutcome::Discovered);
        }

        self.metrics.generated_discovery_decisions_unproductive = self
            .metrics
            .generated_discovery_decisions_unproductive
            .saturating_add(1);
        self.event(
            agent,
            CoreLoopEventKind::GeneratedDiscoveryResult,
            format!(
                "visible_candidate_count={visible_candidate_count};candidate_id={};candidate_name={};location={};dialogue_success=true;session_success=true;open_cases_before={};open_cases_after={};new_open_cases=0;rumor_delivered=false;result=unproductive;reason=no_public_rumor_available;fallback=no_public_rumor_available;activity_fallback=true",
                bounded_event_field(&candidate.npc_id),
                bounded_event_field(&candidate.name),
                bounded_event_field(&candidate.location_id),
                before.len(),
                after.len(),
            ),
        );
        Ok(GeneratedDiscoveryOutcome::NoPublicRumor)
    }

    fn try_generated_dialogue_topic(
        &mut self,
        character_id: u64,
        agent: u32,
        cycle: u32,
        case_id: &str,
        subject: &str,
        topics: &[&str],
        preferred_name: Option<&str>,
        preferred_location: Option<&str>,
    ) -> Result<bool, String> {
        let mut candidates =
            self.visible_npc_candidates(character_id, preferred_name, preferred_location);
        if let Some(name) = preferred_name {
            candidates.retain(|candidate| candidate.name.eq_ignore_ascii_case(name));
        }
        for candidate in candidates.into_iter().take(8) {
            let session_id = self.start_public_dialogue(character_id, cycle, &candidate, "case")?;
            let mut options = self
                .connection
                .db
                .backend_dialogue_topic_options()
                .iter()
                .filter(|row| {
                    row.owner_character_id == character_id
                        && row.session_id == session_id
                        && topics.contains(&row.topic_id.as_str())
                        && row.public_case_id == case_id
                })
                .collect::<Vec<_>>();
            options.sort_by_key(|row| (row.topic_id.clone(), row.id.clone()));
            let Some(option) = options.into_iter().next() else {
                continue;
            };
            let session = self
                .connection
                .db
                .backend_dialogue_sessions()
                .iter()
                .find(|row| row.owner_character_id == character_id && row.id == session_id)
                .ok_or("projected dialogue session disappeared")?;
            let action_id = format!("sim-topic-{cycle}-{}", self.sequence.saturating_add(1));
            let topic_id = option.topic_id.clone();
            let result = reducer_call!(self, "choose_dialogue_topic", |cb| self
                .connection
                .reducers
                .choose_dialogue_topic_then(
                    character_id,
                    session_id.clone(),
                    topic_id.clone(),
                    action_id.clone(),
                    session.revision,
                    session.catalog_revision.clone(),
                    cb,
                ));
            self.call(result)?;
            if topic_id == "referred-testimony" {
                self.metrics.generated_witness_dialogues += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedWitnessDialogue,
                    format!(
                        "case={};subject={};npc={};location={};topic={}",
                        bounded_event_field(case_id),
                        bounded_event_field(subject),
                        bounded_event_field(&candidate.name),
                        bounded_event_field(&candidate.location_id),
                        bounded_event_field(&topic_id)
                    ),
                );
            } else {
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedInvestigationAction,
                    format!(
                        "case={};subject={};npc={};location={};topic={}",
                        bounded_event_field(case_id),
                        bounded_event_field(subject),
                        bounded_event_field(&candidate.name),
                        bounded_event_field(&candidate.location_id),
                        bounded_event_field(&topic_id)
                    ),
                );
            }
            self.observe_generated_case_transition(agent, character_id, case_id, subject, true);
            return Ok(true);
        }
        Ok(false)
    }

    fn generated_actor_ready_after_time(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        case_id: &str,
    ) -> Result<bool, String> {
        self.observe_deaths();
        let current_leader = self.current_leader(party_id).map(|(leader, _)| leader);
        if current_leader != Some(owner_character_id) {
            return Ok(false);
        }
        let current_leader = current_leader.expect("owner is the current leader");
        let party_agents = self.party_agents(current_leader)?;
        let unsafe_agents = self.unsafe_party_agents(&party_agents);
        for unsafe_agent in &unsafe_agents {
            self.metrics.quests_suppressed_for_health += 1;
            self.event(
                *unsafe_agent,
                CoreLoopEventKind::QuestSuppressed,
                format!("generated_case={case_id};after_time_advance"),
            );
        }
        Ok(generated_actor_can_continue(
            owner_character_id,
            Some(current_leader),
            unsafe_agents.len(),
        ))
    }

    fn refreshed_safe_party_for_owner(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
    ) -> Result<Option<(u32, Party)>, String> {
        self.observe_deaths();
        let Some((current_leader, current_agent)) = self.current_leader(party_id) else {
            return Ok(None);
        };
        if current_leader != owner_character_id {
            return Ok(None);
        }
        let party_agents = self.party_agents(current_leader)?;
        if !self.unsafe_party_agents(&party_agents).is_empty() {
            return Ok(None);
        }
        let party = self.party_for(current_leader)?;
        if party.id != party_id {
            return Ok(None);
        }
        Ok(Some((current_agent, party)))
    }

    fn emit_generated_investigation_attempt(
        &mut self,
        party_id: &str,
        character_id: u64,
        agent: u32,
        case_id: &str,
        subject: &str,
        action: &BackendInvestigationAction,
        attempt: &str,
    ) -> Result<(), String> {
        let actor_time = self
            .connection
            .db
            .character_time()
            .iter()
            .find(|row| row.character_id == character_id)
            .map(|row| row.minutes)
            .ok_or("projected investigation actor clock is unavailable")?;
        let party_member_ids = self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|row| row.party_id == party_id)
            .map(|row| row.character_id)
            .collect::<Vec<_>>();
        let mut party_times = party_member_ids
            .iter()
            .map(|member_id| {
                self.connection
                    .db
                    .character_time()
                    .iter()
                    .find(|row| row.character_id == *member_id)
                    .map(|row| row.minutes)
                    .ok_or("projected investigation party clock is unavailable")
            })
            .collect::<Result<Vec<_>, _>>()?;
        party_times.sort_unstable();
        let party_time_min = party_times
            .first()
            .copied()
            .ok_or("projected investigation party clock is unavailable")?;
        let party_time_max = party_times
            .last()
            .copied()
            .ok_or("projected investigation party clock is unavailable")?;
        let reason_code = if action.unavailable_reason_code.is_empty() {
            "none"
        } else {
            &action.unavailable_reason_code
        };
        self.event(
            agent,
            CoreLoopEventKind::GeneratedInvestigationAttempt,
            format!(
                "case={};subject={};action={};method={};summary={};attempt={};expected_version={};available={};unavailable_reason_code={};wait_minutes={};actor_time={actor_time};party_time_min={party_time_min};party_time_max={party_time_max}",
                bounded_event_field(case_id),
                bounded_event_field(subject),
                bounded_event_field(&action.action_id),
                bounded_event_field(&action.method),
                bounded_event_field(&action.summary),
                bounded_event_field(attempt),
                action.expected_version,
                action.available,
                bounded_event_field(reason_code),
                action.wait_minutes,
            ),
        );
        Ok(())
    }

    fn wait_for_generated_investigation_window(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        agent: u32,
        case_id: &str,
        action_id: &str,
        wait_minutes: u32,
    ) -> Result<bool, String> {
        let wait_minutes = projected_investigation_wait_minutes("night_window", wait_minutes)
            .ok_or("projected investigation wait hint was invalid")?;
        let at_settlement = self
            .party_for(owner_character_id)?
            .current_settlement_id
            .is_some();
        let settlement_venue = (at_settlement && wait_minutes >= 60)
            .then(|| self.settlement_activity_venue(owner_character_id, 0).ok())
            .flatten();
        let wait_mode = if let Some(venue) = settlement_venue {
            let result = reducer_call!(self, "wait_for_investigation_window_settlement", |cb| {
                self.connection.reducers.rest_at_settlement_hours_then(
                    owner_character_id,
                    u64::from(wait_minutes),
                    venue.at_inn(),
                    cb,
                )
            });
            self.call(result)?;
            if venue.at_inn() {
                "settlement_inn"
            } else {
                "settlement_temple"
            }
        } else {
            let result = reducer_call!(self, "wait_for_investigation_window_camp", |cb| self
                .connection
                .reducers
                .rest_at_camp_then(owner_character_id, u64::from(wait_minutes), cb));
            self.call(result)?;
            "field_rest"
        };
        self.metrics.generated_investigation_waits += 1;
        self.metrics.generated_investigation_wait_minutes = self
            .metrics
            .generated_investigation_wait_minutes
            .saturating_add(u64::from(wait_minutes));
        self.event(
            agent,
            CoreLoopEventKind::GeneratedInvestigationWait,
            format!(
                "case={};action={};reason=night_window;wait_minutes={wait_minutes};mode={wait_mode}",
                bounded_event_field(case_id),
                bounded_event_field(action_id),
            ),
        );
        self.generated_actor_ready_after_time(party_id, owner_character_id, case_id)
    }

    fn return_completed_generated_party_to_origin(
        &mut self,
        party_id: &str,
        owner_character_id: u64,
        case_id: &str,
    ) -> Result<bool, String> {
        let Some(occupied_site_id) = self
            .party_by_id(party_id)?
            .current_case_site_id
            .map(|site| site.value)
        else {
            return Ok(true);
        };
        let pin = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .find(|pin| {
                occupied_case_pin_matches(
                    owner_character_id,
                    case_id,
                    &occupied_site_id,
                    pin.owner_character_id,
                    &pin.case_id,
                    &pin.case_site_id,
                )
            })
            .ok_or("completed generated case site has no exact owner-scoped return pin")?;
        let Some((current_leader, current_agent)) = self.current_leader(party_id) else {
            return Ok(false);
        };
        let settlement_id = pin.origin_settlement_id.clone();
        let result = reducer_call!(self, "return_completed_generated_case", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(current_leader, settlement_id.clone(), cb,));
        self.call(result)?;
        self.event(
            current_agent,
            CoreLoopEventKind::Travel,
            format!("generated_case={case_id};case_completed=true;return_started={settlement_id}"),
        );
        let journey_outcome = self.travel_camps(party_id)?;
        self.observe_deaths();
        if journey_outcome == JourneyTravelOutcome::Completed {
            self.event(
                current_agent,
                CoreLoopEventKind::Travel,
                format!("generated_case={case_id};return_completed={settlement_id}"),
            );
        }
        Ok(journey_outcome == JourneyTravelOutcome::Completed)
    }

    fn advance_generated_case(
        &mut self,
        party_id: &str,
        character_id: u64,
        agent: u32,
        cycle: u32,
        case_id: &str,
        subject: &str,
    ) -> Result<bool, String> {
        for party_agent in self.party_agents(character_id)? {
            if !self.ensure_medically_safe(party_agent)? {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    party_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("generated_case={case_id};cycle={cycle}"),
                );
                return Ok(false);
            }
            self.maintain_equipment(party_agent)?;
        }
        if self
            .refreshed_safe_party_for_owner(party_id, character_id)?
            .is_none()
        {
            return Ok(false);
        }
        for _ in 0..MAX_GENERATED_CASE_STEPS_PER_CYCLE {
            if self.generated_case_status(character_id, case_id).as_deref() != Some("open") {
                return self.return_completed_generated_party_to_origin(
                    party_id,
                    character_id,
                    case_id,
                );
            }
            let at_settlement = self
                .party_for(character_id)?
                .current_settlement_id
                .is_some();
            let mut actions = self
                .connection
                .db
                .backend_investigation_actions()
                .iter()
                .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
                .collect::<Vec<_>>();
            actions.sort_by_key(|row| row.action_id.clone());
            if let Some(action) = actions.iter().find(|row| row.available).cloned() {
                let known_outcomes = self
                    .connection
                    .db
                    .backend_investigation_action_outcomes()
                    .iter()
                    .filter(|row| row.owner_character_id == character_id && row.case_id == case_id)
                    .map(|row| row.outcome_id)
                    .collect::<HashSet<_>>();
                self.emit_generated_investigation_attempt(
                    party_id,
                    character_id,
                    agent,
                    case_id,
                    subject,
                    &action,
                    "initial",
                )?;
                let result = reducer_call!(self, "perform_investigation_action", |cb| self
                    .connection
                    .reducers
                    .perform_investigation_action_then(
                        character_id,
                        action.action_id.clone(),
                        action.method.clone(),
                        action.expected_version,
                        cb,
                    ));
                if let Err(error) = self.call(result) {
                    if !victim_cohort_state_changed_failure(&error) {
                        return Err(error);
                    }
                    self.metrics.generated_investigation_replans = self
                        .metrics
                        .generated_investigation_replans
                        .saturating_add(1);
                    let refreshed = self
                        .connection
                        .db
                        .backend_investigation_actions()
                        .iter()
                        .find(|row| {
                            row.owner_character_id == character_id
                                && row.case_id == case_id
                                && row.action_id == action.action_id
                        });
                    let refreshed_version = refreshed
                        .as_ref()
                        .map_or_else(|| "none".into(), |row| row.expected_version.to_string());
                    let refreshed_available = refreshed
                        .as_ref()
                        .map_or_else(|| "none".into(), |row| row.available.to_string());
                    let refreshed_wait_minutes = refreshed
                        .as_ref()
                        .map_or_else(|| "none".into(), |row| row.wait_minutes.to_string());
                    let refreshed_reason = refreshed.as_ref().map_or("removed", |row| {
                        if row.unavailable_reason_code.is_empty() {
                            "none"
                        } else {
                            row.unavailable_reason_code.as_str()
                        }
                    });
                    let refresh_label = match refreshed.as_ref() {
                        None => "removed",
                        Some(row) if !row.available => "unavailable",
                        Some(row)
                            if row.expected_version != action.expected_version
                                || row.method != action.method =>
                        {
                            "changed"
                        }
                        Some(_) => "identical_pending_subscription",
                    };
                    self.event(
                        agent,
                        CoreLoopEventKind::GeneratedInvestigationReplan,
                        format!(
                            "case={};action={};reason=investigation_victim_cohort_state_changed;refresh={refresh_label};previous_version={};refreshed_version={};refreshed_available={};refreshed_reason_code={};refreshed_wait_minutes={}",
                            bounded_event_field(case_id),
                            bounded_event_field(&action.action_id),
                            action.expected_version,
                            bounded_event_field(&refreshed_version),
                            bounded_event_field(&refreshed_available),
                            bounded_event_field(refreshed_reason),
                            bounded_event_field(&refreshed_wait_minutes),
                        ),
                    );
                    // A failed reducer transaction cannot provide a subscription
                    // update barrier. Defer once so the next cycle chooses from
                    // a freshly applied public projection.
                    return Ok(false);
                }
                let mut outcomes = self
                    .connection
                    .db
                    .backend_investigation_action_outcomes()
                    .iter()
                    .filter(|row| {
                        row.owner_character_id == character_id
                            && row.case_id == case_id
                            && row.action_id == action.action_id
                            && !known_outcomes.contains(&row.outcome_id)
                    })
                    .collect::<Vec<_>>();
                outcomes.sort_by_key(|row| (row.recorded_at, row.outcome_id.clone()));
                let wording = outcomes
                    .last()
                    .map_or("No new public outcome wording", |row| row.wording.as_str());
                self.metrics.generated_investigation_actions += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::GeneratedInvestigationAction,
                    format!(
                        "case={};subject={};action={};method={};summary={};outcome={}",
                        bounded_event_field(case_id),
                        bounded_event_field(subject),
                        bounded_event_field(&action.action_id),
                        bounded_event_field(&action.method),
                        bounded_event_field(&action.summary),
                        bounded_event_field(wording)
                    ),
                );
                self.observe_generated_case_transition(agent, character_id, case_id, subject, true);
                if self.generated_case_status(character_id, case_id).as_deref() != Some("open") {
                    return self.return_completed_generated_party_to_origin(
                        party_id,
                        character_id,
                        case_id,
                    );
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            if let Some(action) = actions.iter().find(|row| row.can_travel_to_required_site) {
                let funnel_key = (character_id, case_id.to_owned());
                if self.generated_exact_site_cases.insert(funnel_key.clone()) {
                    self.metrics.generated_exact_site_ready += 1;
                }
                let pin = self
                    .connection
                    .db
                    .backend_case_site_pins()
                    .iter()
                    .find(|pin| {
                        pin.owner_character_id == character_id
                            && pin.case_id == case_id
                            && pin.case_site_id == action.required_case_site_id
                    })
                    .ok_or("projected action travel had no exact owner-scoped site pin")?;
                let site_id = pin.case_site_id.clone();
                let distance_m = pin.distance_m;
                if matches!(
                    self.provision_case_site_journey(
                        party_id,
                        character_id,
                        agent,
                        case_id,
                        distance_m,
                    )?,
                    TravelProvisionDecision::Deferred(_)
                ) {
                    return Ok(false);
                }
                let result = reducer_call!(self, "travel_to_generated_case_site", |cb| self
                    .connection
                    .reducers
                    .travel_to_case_site_then(
                        character_id,
                        CaseSiteId {
                            value: site_id.clone(),
                        },
                        cb,
                    ));
                self.call(result)?;
                self.event(
                    agent,
                    CoreLoopEventKind::Travel,
                    format!("generated_case={case_id};outbound={site_id}"),
                );
                let journey_outcome = self.travel_camps(party_id)?;
                if journey_outcome != JourneyTravelOutcome::Completed {
                    return Ok(false);
                }
                if self.generated_traveled_cases.insert(funnel_key) {
                    self.metrics.generated_case_site_traveled += 1;
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            if let Some((action, wait_minutes)) = actions.iter().find_map(|action| {
                projected_investigation_wait_minutes(
                    &action.unavailable_reason_code,
                    action.wait_minutes,
                )
                .map(|wait_minutes| (action, wait_minutes))
            }) {
                if !self.wait_for_generated_investigation_window(
                    party_id,
                    character_id,
                    agent,
                    case_id,
                    &action.action_id,
                    wait_minutes,
                )? {
                    return Ok(false);
                }
                // Rest may clip at a disease/injury boundary or synchronize a
                // lagging member. Re-read the projected action and its exact
                // expected version before attempting it.
                continue;
            }
            if at_settlement {
                let witness = self
                    .connection
                    .db
                    .backend_investigation_leads()
                    .iter()
                    .filter(|row| {
                        row.owner_character_id == character_id
                            && row.case_id == case_id
                            && !row.witness_name.is_empty()
                            && row.corrected_by.is_empty()
                    })
                    .max_by_key(|row| (row.recorded_at, row.lead_id.clone()));
                if let Some(witness) = witness
                    && self.try_generated_dialogue_topic(
                        character_id,
                        agent,
                        cycle,
                        case_id,
                        subject,
                        &["referred-testimony"],
                        Some(&witness.witness_name),
                        Some(if witness.current_learned_location.is_empty() {
                            &witness.expected_location
                        } else {
                            &witness.current_learned_location
                        }),
                    )?
                {
                    continue;
                }
                if self.try_generated_dialogue_topic(
                    character_id,
                    agent,
                    cycle,
                    case_id,
                    subject,
                    &["return-recovered-property", "expose-false-account"],
                    None,
                    None,
                )? {
                    continue;
                }
            }
            let party = self.party_for(character_id)?;
            let occupied_site_id = party.current_case_site_id.map(|site| site.value);
            let pin = occupied_site_id.as_deref().and_then(|occupied_site_id| {
                self.connection
                    .db
                    .backend_case_site_pins()
                    .iter()
                    .find(|pin| {
                        occupied_case_pin_matches(
                            character_id,
                            case_id,
                            occupied_site_id,
                            pin.owner_character_id,
                            &pin.case_id,
                            &pin.case_site_id,
                        )
                    })
            });
            if let Some(pin) = pin {
                if pin.combat_available {
                    let mission_id = format!(
                        "mission:sim-generated:{party_id}:{}:{}",
                        pin.case_site_id, self.sequence
                    );
                    let battle_id = format!("battle:{mission_id}");
                    let result = reducer_call!(self, "autoresolve_generated_mission", |cb| self
                        .connection
                        .reducers
                        .autoresolve_mission_then(character_id, mission_id.clone(), cb));
                    self.call(result)?;
                    let public_binding =
                        self.connection.db.backend_case_battles().iter().any(|row| {
                            row.owner_character_id == character_id
                                && row.public_case_id == case_id
                                && row.party_id == party_id
                                && row.battle_id == battle_id
                                && row.mission_id == mission_id
                                && row.case_site_id.value == pin.case_site_id
                        });
                    if !public_binding {
                        return Err(
                            "generated autoresolve had no public case-battle binding".into()
                        );
                    }
                    if self
                        .connection
                        .db
                        .battle_result()
                        .iter()
                        .any(|row| row.battle_id == battle_id)
                    {
                        self.event(
                            agent,
                            CoreLoopEventKind::AutoresolveVictory,
                            format!("generated_case={case_id};battle={battle_id}"),
                        );
                    } else {
                        self.metrics.defeats += 1;
                        self.event(
                            agent,
                            CoreLoopEventKind::AutoresolveDefeat,
                            format!("generated_case={case_id};battle={battle_id}"),
                        );
                        let settlement_id = pin.origin_settlement_id.clone();
                        let result =
                            reducer_call!(self, "generated_defeat_retreat_to_settlement", |cb| {
                                self.connection.reducers.travel_to_settlement_then(
                                    character_id,
                                    settlement_id.clone(),
                                    cb,
                                )
                            });
                        self.call(result)?;
                        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                            return Ok(false);
                        }
                        self.observe_deaths();
                        if let Some((current_leader, _)) = self.current_leader(party_id) {
                            for party_agent in self.party_agents(current_leader)? {
                                self.ensure_medically_safe(party_agent)?;
                            }
                        }
                        return Ok(false);
                    }
                    self.observe_generated_case_transition(
                        agent,
                        character_id,
                        case_id,
                        subject,
                        true,
                    );
                    if self.generated_case_status(character_id, case_id).as_deref() != Some("open")
                    {
                        return self.return_completed_generated_party_to_origin(
                            party_id,
                            character_id,
                            case_id,
                        );
                    }
                    if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                        return Ok(false);
                    }
                    continue;
                }
                let settlement_id = pin.origin_settlement_id.clone();
                let result = reducer_call!(self, "return_from_generated_case_site", |cb| self
                    .connection
                    .reducers
                    .travel_to_settlement_then(character_id, settlement_id.clone(), cb,));
                self.call(result)?;
                self.event(
                    agent,
                    CoreLoopEventKind::Travel,
                    format!("generated_case={case_id};return={settlement_id}"),
                );
                if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                    return Ok(false);
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            return Ok(false);
        }
        Ok(false)
    }

    fn turn_in_ready_direct_contract(
        &mut self,
        party_id: &str,
        leader: u64,
        leader_agent: u32,
        quest: &BackendContract,
    ) -> Result<(), String> {
        let party = self.party_by_id(party_id)?;
        let publicly_ready = party.current_settlement_id.as_deref()
            == Some(quest.settlement_id.as_str())
            && party.current_case_site_id.is_none()
            && party.camp_destination.is_none()
            && party.active_contract_id.as_deref() == Some(quest.id.as_str())
            && self
                .connection
                .db
                .backend_contracts()
                .iter()
                .any(|contract| {
                    contract.id == quest.id
                        && contract.status == ContractStatus::ReadyToReport
                        && contract.accepted_by.as_deref() == Some(party_id)
                });
        if !publicly_ready {
            self.event(
                leader_agent,
                CoreLoopEventKind::QuestSuppressed,
                format!(
                    "quest={};reason=direct_contract_report_arrival_not_proven",
                    bounded_event_field(&quest.id)
                ),
            );
            return Ok(());
        }
        let result = reducer_call!(self, "interact_report_contract", |cb| self
            .connection
            .reducers
            .simulate_contract_issuer_interaction_then(
                leader,
                quest.id.clone(),
                ContractInteractionStage::Report,
                cb,
            ));
        self.call(result)?;
        let result = reducer_call!(self, "turn_in_quest", |cb| self
            .connection
            .reducers
            .report_contract_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.metrics.quests_completed += 1;
        self.metrics.direct_contracts_completed += 1;
        self.event(leader_agent, CoreLoopEventKind::TurnIn, quest.id.clone());
        Ok(())
    }

    fn cycle(&mut self, party_id: &str, cycle: u32) -> Result<(), String> {
        let Some((quest_owner, _)) = self.current_leader(party_id) else {
            self.observe_deaths();
            return Ok(());
        };
        let party_agents = self.party_agents(quest_owner)?;
        for &agent in &party_agents {
            if !self.ensure_medically_safe(agent)? {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("cycle={cycle}"),
                );
                return Ok(());
            }
            self.maintain_equipment(agent)?;
        }
        let Some((mut leader_agent, party)) =
            self.refreshed_safe_party_for_owner(party_id, quest_owner)?
        else {
            return Ok(());
        };
        let mut leader = quest_owner;
        let active_contract = self.active_direct_contract(&party);
        let resuming_contract = active_contract.is_some();
        let quest = active_contract
            .or_else(|| self.choose_quest(&party, &self.profiles[leader_agent as usize]))
            .ok_or("no suitable available or accepted quest")?;
        if quest.status == ContractStatus::ReadyToReport {
            return self.turn_in_ready_direct_contract(party_id, leader, leader_agent, &quest);
        }
        if !resuming_contract {
            if let TravelProvisionDecision::Deferred(reason) = self.provision_case_site_journey(
                party_id,
                leader,
                leader_agent,
                &quest.case_id,
                quest.distance_m,
            )? {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!(
                        "quest={};acceptance_deferred={reason}",
                        bounded_event_field(&quest.id)
                    ),
                );
                self.settlement_activity_day(leader_agent)?;
                return Ok(());
            }
            self.metrics.quests_attempted += 1;
            self.metrics.direct_contracts_attempted += 1;
            let result = reducer_call!(self, "interact_accept_contract", |cb| self
                .connection
                .reducers
                .simulate_contract_issuer_interaction_then(
                    leader,
                    quest.id.clone(),
                    ContractInteractionStage::Accept,
                    cb,
                ));
            self.call(result)?;
            let result = reducer_call!(self, "accept_quest", |cb| self
                .connection
                .reducers
                .accept_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
        }
        let case_site = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|site| site.owner_character_id == leader && site.case_id == quest.case_id)
            .min_by_key(|site| (site.distance_m, site.case_site_id.clone()))
            .ok_or("accepted quest did not disclose an exact case site")?;
        let already_at_case_site = party
            .current_case_site_id
            .as_ref()
            .is_some_and(|site| site.value == case_site.case_site_id);
        if !already_at_case_site {
            if matches!(
                self.provision_case_site_journey(
                    party_id,
                    leader,
                    leader_agent,
                    &quest.case_id,
                    case_site.distance_m,
                )?,
                TravelProvisionDecision::Deferred(_)
            ) {
                return Err(
                    "accepted contract provisioning projection changed after disclosure".into(),
                );
            }
            if !resuming_contract {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::AcceptContract,
                    format!(
                        "cycle={cycle};quest={};title={};difficulty={};opposition={} {};distance_m={}",
                        quest.id,
                        quest.title,
                        quest.difficulty,
                        quest.opposition_count_wording,
                        quest.opposition_wording,
                        case_site.distance_m
                    ),
                );
            } else {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::Travel,
                    format!(
                        "direct_contract={};continuation=outbound;case_site={}",
                        bounded_event_field(&quest.id),
                        bounded_event_field(&case_site.case_site_id),
                    ),
                );
            }

            let outbound_before = self.expedition_member_observations(party_id)?;
            let outbound_supplies_before = self.expedition_supplies(party_id);
            let result = reducer_call!(self, "travel_to_case_site", |cb| self
                .connection
                .reducers
                .travel_to_case_site_then(
                    leader,
                    CaseSiteId {
                        value: case_site.case_site_id.clone(),
                    },
                    cb,
                ));
            self.call(result)?;
            let outbound_after = self.expedition_member_observations(party_id)?;
            let outbound_supplies_after = self.expedition_supplies(party_id);
            self.emit_expedition_diagnostics(
                party_id,
                "journey_leg",
                "travel_to_case_site",
                if outbound_after.iter().any(expedition_member_needs_recovery) {
                    "quest_suppressed_member_not_ready_after_outbound_leg"
                } else {
                    "quest_leg_outbound_all_members_ready"
                },
                &outbound_before,
                &outbound_after,
                outbound_supplies_before,
                outbound_supplies_after,
            );
            self.event(
                leader_agent,
                CoreLoopEventKind::Travel,
                format!("outbound={}", case_site.case_site_id),
            );
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
        } else {
            self.event(
                leader_agent,
                CoreLoopEventKind::Travel,
                format!(
                    "direct_contract={};continuation=arrived_case_site",
                    bounded_event_field(&quest.id)
                ),
            );
        }

        // Travel advances every member's disease clock. Re-observe public
        // life/condition state before attempting a living-only combat reducer.
        let unsafe_after_travel = self.unsafe_party_agents(&party_agents);
        if !unsafe_after_travel.is_empty() {
            for &agent in &unsafe_after_travel {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("after_travel;cycle={cycle}"),
                );
            }
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            let result = reducer_call!(self, "illness_retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
            for &agent in &party_agents {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current_agent, _)) =
                self.refreshed_safe_party_for_owner(party_id, quest_owner)?
            else {
                return Ok(());
            };
            leader = quest_owner;
            leader_agent = current_agent;
            let result = reducer_call!(self, "abandon_unsafe_quest", |cb| self
                .connection
                .reducers
                .abandon_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.event(leader_agent, CoreLoopEventKind::AbandonQuest, quest.id);
            return Ok(());
        }

        let mut victory = false;
        let mut winning_battle_id = None;
        for attempt in 0..=MAX_DEFEAT_RETRIES {
            let mission_id = format!(
                "mission:sim-autoresolve:{}:{}:{}",
                party_id, case_site.case_site_id, attempt
            );
            let battle_id = format!("battle:{mission_id}");
            let result = reducer_call!(self, "autoresolve_mission", |cb| self
                .connection
                .reducers
                .autoresolve_mission_then(leader, mission_id.clone(), cb));
            self.call(result)?;
            self.observe_deaths();
            let Some((current, current_agent)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            leader_agent = current_agent;
            let report = self
                .connection
                .db
                .autoresolve_report()
                .iter()
                .find(|r| r.battle_id == battle_id)
                .ok_or("autoresolve completed without a report")?;
            if self
                .connection
                .db
                .battle_result()
                .iter()
                .any(|r| r.battle_id == battle_id)
            {
                victory = true;
                winning_battle_id = Some(battle_id.clone());
                self.event(
                    leader_agent,
                    CoreLoopEventKind::AutoresolveVictory,
                    format!(
                        "seed={};rounds={};summary={};log={:?}",
                        report.seed, report.rounds, report.summary, report.log
                    ),
                );
                break;
            }
            self.metrics.defeats += 1;
            self.event(
                leader_agent,
                CoreLoopEventKind::AutoresolveDefeat,
                format!(
                    "seed={};rounds={};summary={};log={:?}",
                    report.seed, report.rounds, report.summary, report.log
                ),
            );
            if attempt == MAX_DEFEAT_RETRIES {
                break;
            }
            self.metrics.retries += 1;
            let result = reducer_call!(self, "retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            for agent in self.party_agents(leader)? {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current_agent, _)) =
                self.refreshed_safe_party_for_owner(party_id, quest_owner)?
            else {
                return Ok(());
            };
            leader = quest_owner;
            leader_agent = current_agent;
            if let TravelProvisionDecision::Deferred(reason) = self.provision_case_site_journey(
                party_id,
                leader,
                leader_agent,
                &quest.case_id,
                case_site.distance_m,
            )? {
                self.event(
                    leader_agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!(
                        "quest={};retry_deferred={reason}",
                        bounded_event_field(&quest.id)
                    ),
                );
                let result =
                    reducer_call!(self, "abandon_failed_unprovisioned_contract", |cb| self
                        .connection
                        .reducers
                        .abandon_contract_then(leader, quest.id.clone(), cb));
                self.call(result)?;
                self.event(
                    leader_agent,
                    CoreLoopEventKind::AbandonQuest,
                    format!(
                        "quest={};reason=failed_expedition_cannot_reprovision;detail={reason}",
                        bounded_event_field(&quest.id)
                    ),
                );
                self.settlement_activity_day(leader_agent)?;
                return Ok(());
            }
            let result = reducer_call!(self, "retry_travel_to_case_site", |cb| self
                .connection
                .reducers
                .travel_to_case_site_then(
                    leader,
                    CaseSiteId {
                        value: case_site.case_site_id.clone(),
                    },
                    cb,
                ));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
            self.observe_deaths();
            let Some((current, current_agent)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            leader_agent = current_agent;
            let retry_agents = self.party_agents(leader)?;
            if !self.unsafe_party_agents(&retry_agents).is_empty() {
                // Reuse the same post-travel health gate on retries; the next
                // iteration must never call a living-only combat reducer.
                break;
            }
        }
        if !victory {
            let result = reducer_call!(self, "defeat_retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
            self.call(result)?;
            if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
                return Ok(());
            }
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            for agent in self.party_agents(leader)? {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current_agent, _)) =
                self.refreshed_safe_party_for_owner(party_id, quest_owner)?
            else {
                return Ok(());
            };
            leader = quest_owner;
            leader_agent = current_agent;
            let result = reducer_call!(self, "abandon_defeated_quest", |cb| self
                .connection
                .reducers
                .abandon_contract_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.event(leader_agent, CoreLoopEventKind::AbandonQuest, quest.id);
            let result = reducer_call!(self, "replenish_quests_after_abandon", |cb| self
                .connection
                .reducers
                .ensure_settlement_activity_then(quest.settlement_id.clone(), cb));
            self.call(result)?;
            return Ok(());
        }
        let winning_battle_id = winning_battle_id.ok_or("victory had no battle authority")?;

        let loot: Vec<_> = self
            .connection
            .db
            .battle_loot_item()
            .iter()
            .filter(|row| row.loot_battle_id == winning_battle_id)
            .collect();
        let definitions: HashMap<_, _> = self
            .connection
            .db
            .item()
            .iter()
            .map(|item| (item.id.clone(), item))
            .collect();
        for entry in &loot {
            self.metrics.loot_items = self.metrics.loot_items.saturating_add(entry.quantity);
            self.metrics.loot_value = self.metrics.loot_value.saturating_add(
                u64::from(entry.quantity)
                    * u64::from(
                        definitions
                            .get(&entry.item_id)
                            .and_then(|i| i.base_value)
                            .unwrap_or(0),
                    ),
            );
        }
        let result = reducer_call!(self, "store_battle_loot", |cb| self
            .connection
            .reducers
            .store_battle_loot_then(leader, winning_battle_id, vec![], vec![], cb,));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::StoreLoot,
            format!("stacks={}", loot.len()),
        );

        let result = reducer_call!(self, "return_to_settlement", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(leader, quest.settlement_id.clone(), cb));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::Travel,
            format!("return={}", quest.settlement_id),
        );
        if self.travel_camps(party_id)? != JourneyTravelOutcome::Completed {
            return Ok(());
        }
        self.observe_deaths();
        let Some((current, current_agent)) = self.current_leader(party_id) else {
            return Ok(());
        };
        leader = current;
        leader_agent = current_agent;
        self.turn_in_ready_direct_contract(party_id, leader, leader_agent, &quest)?;

        let party = self.party_for(leader)?;
        let sale: Vec<_> = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party.id && !is_currency_id(&row.item_id))
            .collect();
        if !sale.is_empty() {
            let before_coins: u64 = self
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party.id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            let ids = sale.iter().map(|row| row.id).collect();
            let quantities = sale.iter().map(|row| row.quantity).collect();
            let result = reducer_call!(self, "liquidate_party_inventory", |cb| self
                .connection
                .reducers
                .liquidate_party_inventory_then(
                    leader,
                    quest.settlement_id.clone(),
                    ids,
                    quantities,
                    cb
                ));
            self.call(result)?;
            let after_coins: u64 = self
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party.id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            self.metrics.sale_proceeds += after_coins.saturating_sub(before_coins);
            self.event(
                leader_agent,
                CoreLoopEventKind::Liquidate,
                format!("stacks={}", sale.len()),
            );
        }
        // Spending priority is medical care, then repairs, then upgrades.
        for agent in self.party_agents(leader)? {
            if self.ensure_medically_safe(agent)? {
                self.maintain_equipment(agent)?;
            }
        }
        if let Some((current_agent, _)) =
            self.refreshed_safe_party_for_owner(party_id, quest_owner)?
        {
            self.try_upgrade(current_agent, &quest.settlement_id)?;
        }
        Ok(())
    }

    fn try_upgrade(&mut self, agent: u32, settlement: &str) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        let profile = self.profiles[agent as usize].clone();
        let equipped = self
            .connection
            .db
            .character_equip()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing equipment state")?;
        let equipped_ids = [
            equipped.left_hand_item_id,
            equipped.right_hand_item_id,
            equipped.left_arm_armor_id,
            equipped.right_arm_armor_id,
            equipped.left_leg_armor_id,
            equipped.right_leg_armor_id,
            equipped.head_armor_id,
            equipped.chest_armor_id,
            equipped.stomach_armor_id,
        ]
        .into_iter()
        .flatten()
        .collect::<HashSet<_>>();
        let inventories: Vec<_> = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id)
            .collect();
        let definitions: Vec<_> = self.connection.db.item().iter().collect();
        let equipped_definitions = inventories
            .iter()
            .filter(|row| equipped_ids.contains(&row.id))
            .filter_map(|row| {
                let definition = definitions.iter().find(|item| item.id == row.item_id)?;
                let condition = self
                    .connection
                    .db
                    .item_condition()
                    .iter()
                    .find(|value| value.inventory_item_id == row.id)
                    .map_or(1.0, |value| {
                        1.0 - (value.tier_1
                            + value.tier_2
                            + value.tier_3
                            + value.tier_4
                            + value.tier_5)
                            .clamp(0.0, 1.0)
                    });
                Some((definition, condition))
            })
            .collect::<Vec<_>>();
        let character = self
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .ok_or("missing upgrade character")?;
        let party_id = character.party_id.ok_or("missing upgrade party")?;
        let stake = self
            .connection
            .db
            .party_stake()
            .iter()
            .find(|row| row.party_id == party_id && row.character_id == character_id)
            .map_or(0, |row| row.value);
        let mut candidates = definitions
            .iter()
            .filter_map(|candidate| {
                let utility = equipment_utility(&profile, candidate)?;
                let armor = matches!(candidate.kind, ItemKind::Armor | ItemKind::Clothing);
                let current = equipped_definitions
                    .iter()
                    .filter(|(item, _)| {
                        if candidate.melee || candidate.ranged {
                            item.melee || item.ranged
                        } else {
                            armor
                                && matches!(item.kind, ItemKind::Armor | ItemKind::Clothing)
                                && item.slot == candidate.slot
                        }
                    })
                    .filter_map(|(item, condition)| {
                        equipment_utility(&profile, item).map(|utility| utility * *condition)
                    })
                    .fold(0.0, f32::max);
                let cost = adventuresim_core::strategic_economy::merchant_buy_price(
                    candidate.base_value.unwrap_or(1),
                );
                (utility > current && u64::from(cost) <= stake).then_some((
                    utility - current,
                    cost,
                    candidate.clone(),
                ))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.2.id.cmp(&right.2.id))
        });
        let Some((improvement, cost, candidate)) = candidates.into_iter().next() else {
            return Ok(());
        };
        let mut treasury: Vec<_> = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
            .collect();
        treasury.sort_by_key(|row| (row.item_id.clone(), row.id));
        if treasury
            .iter()
            .map(|row| u64::from(row.quantity))
            .sum::<u64>()
            < u64::from(cost)
        {
            return Ok(());
        }
        let mut remaining = cost;
        for stack in treasury {
            let quantity = remaining.min(stack.quantity);
            let result = reducer_call!(self, "withdraw_earned_upgrade_coin", |cb| self
                .connection
                .reducers
                .withdraw_party_inventory_item_then(character_id, stack.id, quantity, cb));
            self.call(result)?;
            remaining -= quantity;
            if remaining == 0 {
                break;
            }
        }
        self.metrics.earned_gold_withdrawn += u64::from(cost);
        let result = reducer_call!(self, "finalize_merchant_trade", |cb| self
            .connection
            .reducers
            .finalize_merchant_trade_then(
                character_id,
                settlement.to_string(),
                vec![candidate.id.clone()],
                vec![1],
                vec![],
                vec![],
                false,
                cb,
            ));
        self.call(result)?;
        self.metrics.equipment_purchases += 1;
        self.event(
            agent,
            CoreLoopEventKind::Purchase,
            format!(
                "item={};earned_cost={cost};utility_gain={improvement:.3}",
                candidate.id
            ),
        );
        let inventory = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && row.item_id == candidate.id)
            .max_by_key(|row| row.id)
            .ok_or("purchase succeeded but inventory was not coherent")?;
        let destination = if candidate.melee || candidate.ranged {
            ItemSlot::AnyHolding
        } else {
            candidate.slot
        };
        let result = reducer_call!(self, "equip_item", |cb| self
            .connection
            .reducers
            .equip_item_then(character_id, inventory.id, destination, cb));
        self.call(result)?;
        let verified = self
            .connection
            .db
            .character_equip()
            .iter()
            .find(|row| row.character_id == character_id)
            .is_some_and(|row| equipped_at(&row, destination, inventory.id));
        if !verified {
            return Err("equip reducer completed without the requested equipped state".into());
        }
        self.metrics.equipment_upgrades += 1;
        self.event(agent, CoreLoopEventKind::Equip, candidate.id);
        Ok(())
    }
}

fn leader_is_actionable(
    party_id: &str,
    authoritative_leader_id: u64,
    character_id: u64,
    alive: bool,
    character_party_id: Option<&str>,
) -> bool {
    alive && character_id == authoritative_leader_id && character_party_id == Some(party_id)
}

fn equipment_utility(profile: &AgentProfile, item: &Item) -> Option<f32> {
    let preference = &profile.equipment;
    let armor = matches!(item.kind, ItemKind::Armor | ItemKind::Clothing);
    let compatible = match preference.style {
        EquipmentStyle::Unarmored => !armor && item.melee && item.weight <= 2.5,
        EquipmentStyle::Ranged => !armor && item.ranged,
        EquipmentStyle::Light => (armor && item.weight <= 8.0) || (!armor && item.melee),
        EquipmentStyle::Heavy => armor || item.melee,
    };
    if !compatible || item.base_value.is_none() {
        return None;
    }
    let protection = item.coverage + item.resistance + item.padding;
    let mobility = item.flexibility + item.range_of_motion - item.weight * 0.1;
    let price = 1.0 / (1.0 + item.base_value.unwrap_or(1) as f32 / 100.0);
    Some(
        preference.protection_weight * protection
            + preference.mobility_weight * mobility
            + preference.price_weight * price
            + preference.reach_weight * item.reach,
    )
}

fn equipped_at(equip: &CharacterEquip, slot: ItemSlot, inventory_id: u64) -> bool {
    match slot {
        ItemSlot::LeftHolding => equip.left_hand_item_id == Some(inventory_id),
        ItemSlot::RightHolding | ItemSlot::AnyHolding => {
            equip.left_hand_item_id == Some(inventory_id)
                || equip.right_hand_item_id == Some(inventory_id)
        }
        ItemSlot::LeftArm => equip.left_arm_armor_id == Some(inventory_id),
        ItemSlot::RightArm | ItemSlot::AnyArm => {
            equip.left_arm_armor_id == Some(inventory_id)
                || equip.right_arm_armor_id == Some(inventory_id)
        }
        ItemSlot::LeftLeg => equip.left_leg_armor_id == Some(inventory_id),
        ItemSlot::RightLeg | ItemSlot::AnyLeg => {
            equip.left_leg_armor_id == Some(inventory_id)
                || equip.right_leg_armor_id == Some(inventory_id)
        }
        ItemSlot::Head => equip.head_armor_id == Some(inventory_id),
        ItemSlot::Chest => equip.chest_armor_id == Some(inventory_id),
        ItemSlot::Stomach => equip.stomach_armor_id == Some(inventory_id),
        ItemSlot::None => false,
    }
}

pub fn run_core_loop(config: CoreLoopConfig) -> Result<CoreLoopReport, String> {
    run_core_loop_with_npc_policy(config, None)
}

pub fn run_core_loop_with_npc_policy(
    config: CoreLoopConfig,
    npc_strategy_policy: Option<Box<dyn QuestPolicy>>,
) -> Result<CoreLoopReport, String> {
    let failure_recorder = FailureRecorder::new(config.failure_output.clone());
    let result =
        run_core_loop_with_npc_policy_inner(config, npc_strategy_policy, failure_recorder.clone());
    if let Err(error) = &result
        && let Err(diagnostic_error) = failure_recorder.write(error)
    {
        return Err(format!("{error}; {diagnostic_error}"));
    }
    result
}

fn run_core_loop_with_npc_policy_inner(
    config: CoreLoopConfig,
    npc_strategy_policy: Option<Box<dyn QuestPolicy>>,
    failure_recorder: FailureRecorder,
) -> Result<CoreLoopReport, String> {
    config.validate()?;
    let bootstrap_token =
        bootstrap_token_from_environment(std::env::var(BOOTSTRAP_TOKEN_ENV).ok())?;
    let (connected_tx, connected_rx) = mpsc::sync_channel(1);
    let connect_error_tx = connected_tx.clone();
    let connection = DbConnection::builder()
        .with_uri(&config.host)
        .with_database_name(&config.database)
        .on_connect(move |_, _, _| {
            let _ = connected_tx.send(Ok(()));
        })
        .on_connect_error(move |_, error| {
            let _ = connect_error_tx.send(Err(error.to_string()));
        })
        .build()
        .map_err(|error| error.to_string())?;
    let (subscription_tx, subscription_rx) = mpsc::sync_channel(1);
    let subscription_error_tx = subscription_tx.clone();
    connection
        .subscription_builder()
        .on_applied(move |_| {
            let _ = subscription_tx.send(Ok(()));
        })
        .on_error(move |_, error| {
            let _ = subscription_error_tx.send(Err(error.to_string()));
        })
        // Deliberately enumerate the policy observation surface. In
        // particular, never transport backend infection episodes, committed
        // cuts, or full medical examinations into the simulator process.
        .add_query(|query| query.from.autoresolve_report())
        .add_query(|query| query.from.backend_case_battles())
        .add_query(|query| query.from.backend_case_site_pins())
        .add_query(|query| query.from.backend_dialogue_sessions())
        .add_query(|query| query.from.backend_dialogue_topic_options())
        .add_query(|query| query.from.backend_investigation_action_outcomes())
        .add_query(|query| query.from.backend_investigation_actions())
        .add_query(|query| query.from.backend_investigation_cases())
        .add_query(|query| query.from.backend_investigation_journal())
        .add_query(|query| query.from.backend_investigation_leads())
        .add_query(|query| query.from.backend_npc_case_interventions())
        .add_query(|query| query.from.backend_npc_intervention_candidates())
        .add_query(|query| query.from.backend_local_problem_trade_effects())
        .add_query(|query| query.from.battle_loot_item())
        .add_query(|query| query.from.battle_result())
        .add_query(|query| query.from.character())
        .add_query(|query| query.from.character_capability())
        .add_query(|query| query.from.character_death())
        .add_query(|query| query.from.character_equip())
        .add_query(|query| query.from.character_illness_status())
        .add_query(|query| query.from.character_needs())
        .add_query(|query| query.from.character_strategic_condition())
        .add_query(|query| query.from.character_time())
        .add_query(|query| query.from.character_training_schedule())
        .add_query(|query| query.from.inventory_item())
        .add_query(|query| query.from.food_lot())
        .add_query(|query| query.from.item())
        .add_query(|query| query.from.item_condition())
        .add_query(|query| query.from.party())
        .add_query(|query| query.from.party_inventory_item())
        .add_query(|query| query.from.party_journey())
        .add_query(|query| query.from.party_journey_itinerary())
        .add_query(|query| query.from.party_join_request())
        .add_query(|query| query.from.party_member())
        .add_query(|query| query.from.party_stake())
        .add_query(|query| query.from.backend_contracts())
        .add_query(|query| query.from.backend_settlement_npcs())
        .add_query(|query| query.from.strategic_encounter())
        .add_query(|query| query.from.repair_order())
        .add_query(|query| query.from.settlement())
        .add_query(|query| query.from.settlement_npc_presence())
        .add_query(|query| query.from.settlement_smith())
        .add_query(|query| query.from.simulation_run())
        .add_query(|query| query.from.world_data_import())
        .subscribe();
    connection.run_threaded();
    connected_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "connection timed out".to_string())??;
    subscription_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "subscription timed out".to_string())??;

    let profiles = (0..config.population)
        .map(|id| generate_profile(config.seed, id))
        .collect::<Vec<_>>();
    let base_id = 0x5349_4d00_0000_0000_u64 ^ config.seed.rotate_left(17);
    let character_ids = (0..config.population)
        .map(|id| base_id ^ u64::from(id + 1))
        .collect::<Vec<_>>();
    let mut runner = LiveRunner {
        connection,
        profiles,
        character_ids,
        metrics: CoreLoopMetrics::default(),
        trace: Vec::new(),
        sequence: 0,
        dialogue_nonce: 0,
        last_semantic_event: None,
        recorded_deaths: HashSet::new(),
        medically_paused_schedules: HashSet::new(),
        generated_seen_cases: HashSet::new(),
        generated_terminal_cases: HashSet::new(),
        generated_exact_site_cases: HashSet::new(),
        generated_traveled_cases: HashSet::new(),
        generated_finance_blocks: HashMap::new(),
        npc_strategy_policy,
        simulation_run_nonce: config.run_nonce.clone(),
        failure_recorder,
    };
    if runner
        .connection
        .db
        .simulation_run()
        .iter()
        .next()
        .is_some()
        || runner.connection.db.character().iter().next().is_some()
    {
        return Err("refusing reused or populated simulation database".into());
    }
    let world_import = runner.connection.db.world_data_import().iter().next();
    if config.use_imported_world {
        let imported = world_import
            .as_ref()
            .filter(|import| import.completed)
            .ok_or("full-world mode requires a completed world_data_import")?;
        if Some(imported.manifest_digest.as_str())
            != config.expected_world_manifest_digest.as_deref()
        {
            return Err(
                "imported world manifest does not match the pinned expected manifest".into(),
            );
        }
        if imported.artifact_id.trim().is_empty()
            || imported.manifest_digest.len() != 64
            || runner.connection.db.settlement().iter().next().is_none()
        {
            return Err(
                "completed world_data_import has invalid provenance or no settlements".into(),
            );
        }
    } else if world_import.is_some() || runner.connection.db.settlement().iter().next().is_some() {
        return Err("fixture mode refuses imported or pre-existing settlement state".into());
    }
    let result = reducer_call!(runner, "claim_simulation_run", |cb| runner
        .connection
        .reducers
        .claim_simulation_run_then(
            bootstrap_token.clone(),
            config.run_nonce.clone(),
            config.seed,
            cb,
        ));
    runner.call(result)?;
    // The disposable simulation owns this otherwise-empty database, so its
    // authenticated connection is also the trusted strategic gateway.
    let result = reducer_call!(runner, "register_strategic_gateway", |cb| runner
        .connection
        .reducers
        .register_strategic_gateway_then(None, 0, cb));
    runner.call(result)?;
    // Re-subscribe the gateway-only observation surface after registration.
    // This does not rely on an already-applied subscription recomputing views
    // when gateway authority changes.
    let (gateway_subscription_tx, gateway_subscription_rx) = mpsc::sync_channel(1);
    let gateway_subscription_error_tx = gateway_subscription_tx.clone();
    runner
        .connection
        .subscription_builder()
        .on_applied(move |_| {
            let _ = gateway_subscription_tx.send(Ok(()));
        })
        .on_error(move |_, error| {
            let _ = gateway_subscription_error_tx.send(Err(error.to_string()));
        })
        .add_query(|query| query.from.backend_case_battles())
        .add_query(|query| query.from.backend_case_site_pins())
        .add_query(|query| query.from.backend_contracts())
        .add_query(|query| query.from.backend_dialogue_sessions())
        .add_query(|query| query.from.backend_dialogue_topic_options())
        .add_query(|query| query.from.backend_investigation_action_outcomes())
        .add_query(|query| query.from.backend_investigation_actions())
        .add_query(|query| query.from.backend_investigation_cases())
        .add_query(|query| query.from.backend_investigation_journal())
        .add_query(|query| query.from.backend_investigation_leads())
        .add_query(|query| query.from.backend_npc_case_interventions())
        .add_query(|query| query.from.backend_npc_intervention_candidates())
        .add_query(|query| query.from.backend_local_problem_trade_effects())
        .add_query(|query| query.from.backend_settlement_npcs())
        .add_query(|query| query.from.party())
        .add_query(|query| query.from.settlement_npc_presence())
        .subscribe();
    gateway_subscription_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "gateway subscription timed out".to_string())??;
    if !config.use_imported_world {
        let result = reducer_call!(runner, "seed_simulation_world", |cb| runner
            .connection
            .reducers
            .seed_simulation_world_then(config.run_nonce.clone(), cb));
        runner.call(result)?;
    }
    let starting_settlement_id = runner
        .connection
        .db
        .settlement()
        .iter()
        .map(|settlement| settlement.id)
        .min()
        .ok_or("simulation world has no starting settlement")?;
    for (agent, character_id) in runner.character_ids.clone().into_iter().enumerate() {
        let name = format!("sim-{}-{agent}", config.seed);
        let result = reducer_call!(runner, "create_named_character_with_id", |cb| runner
            .connection
            .reducers
            .create_named_character_with_id_then(character_id, name.clone(), cb));
        runner.call(result)?;
        let settlement = starting_settlement_id.clone();
        let profile = runner.profiles[agent].clone();
        let attributes = live_attributes(character_id, &profile);
        let skills = live_skills(character_id, &profile);
        let downtime = live_schedule(&profile);
        let personality = live_personality(character_id, &profile.personality);
        let result = reducer_call!(runner, "configure_simulation_character", |cb| runner
            .connection
            .reducers
            .configure_simulation_character_then(
                config.run_nonce.clone(),
                character_id,
                agent as u32,
                settlement.clone(),
                attributes.clone(),
                skills.clone(),
                downtime.clone(),
                personality.clone(),
                cb,
            ));
        runner.call(result)?;
        let fixture_item = runner
            .connection
            .db
            .inventory_item()
            .iter()
            .find(|row| {
                row.character_id == character_id
                    && runner
                        .connection
                        .db
                        .item()
                        .iter()
                        .find(|item| item.id == row.item_id)
                        .is_some_and(|item| {
                            matches!(
                                item.kind,
                                ItemKind::Weapon | ItemKind::Armor | ItemKind::Shield
                            )
                        })
            })
            .ok_or("simulation character has no durable fixture item")?;
        let result = reducer_call!(runner, "seed_simulation_equipment_damage", |cb| runner
            .connection
            .reducers
            .seed_simulation_equipment_damage_then(
                config.run_nonce.clone(),
                character_id,
                fixture_item.id,
                cb,
            ));
        runner.call(result)?;
        if agent == 0 {
            let result = reducer_call!(runner, "seed_simulation_disease", |cb| runner
                .connection
                .reducers
                .seed_simulation_disease_then(config.run_nonce.clone(), character_id, cb));
            runner.call(result)?;
        }
        runner.metrics.parties_formed += 1;
        runner.event(agent as u32, CoreLoopEventKind::FormParty, name);
    }

    // Joining is demonstrated with the same ordinary request/accept reducers as players.
    // The bounded bootstrap co-locates fresh sim-* solo parties before they use
    // the ordinary request/accept reducers to merge.
    let settlement = runner
        .party_for(runner.character_ids[0])?
        .current_settlement_id
        .clone()
        .ok_or("leader not at settlement")?;
    let mut party_ids = Vec::new();
    for first in (0..runner.character_ids.len()).step_by(config.party_size as usize) {
        let leader = runner.character_ids[first];
        let leader_party = runner.party_for(leader)?;
        party_ids.push(leader_party.id.clone());
        let end = (first + config.party_size as usize).min(runner.character_ids.len());
        for agent in first + 1..end {
            let member = runner.character_ids[agent];
            let result = reducer_call!(runner, "request_general_party_join", |cb| runner
                .connection
                .reducers
                .request_general_party_join_then(member, leader_party.id.clone(), cb));
            runner.call(result)?;
            runner.metrics.joins_requested += 1;
            runner.event(
                agent as u32,
                CoreLoopEventKind::RequestJoin,
                leader_party.id.clone(),
            );
            let request = runner
                .connection
                .db
                .party_join_request()
                .iter()
                .find(|row| row.character_id == member && row.party_id == leader_party.id)
                .ok_or("join reducer completed without a coherent request row")?;
            let result = reducer_call!(runner, "accept_party_join_request", |cb| runner
                .connection
                .reducers
                .accept_party_join_request_then(leader, request.id, cb));
            runner.call(result)?;
            runner.metrics.joins_accepted += 1;
            runner.event(
                agent as u32,
                CoreLoopEventKind::AcceptJoin,
                leader_party.id.clone(),
            );
        }
    }
    let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
        .connection
        .reducers
        .ensure_settlement_activity_then(settlement.clone(), cb));
    runner.call(result)?;
    runner.choose_pending_npc_strategies()?;

    let duration_minutes = u64::from(config.duration_days) * 1_440;
    for cycle in 0..config.cycles {
        let mut active = false;
        let mut held = false;
        for party_id in &party_ids {
            runner.observe_deaths();
            runner.observe_external_generated_closures();
            let party_time_before = runner.public_party_elapsed_max(party_id);
            let Some((pre_recovery_leader, _)) = runner.current_leader(party_id) else {
                continue;
            };
            let recovery_started_in_budget = runner
                .connection
                .db
                .character_time()
                .iter()
                .find(|row| row.character_id == pre_recovery_leader)
                .ok_or("missing pre-recovery leader clock")?
                .minutes
                < duration_minutes;
            if !recovery_started_in_budget {
                continue;
            }
            let recovery_outcome = runner.recover_or_evacuate_off_settlement(party_id, cycle)?;
            match recovery_outcome {
                ExpeditionRecoveryOutcome::None | ExpeditionRecoveryOutcome::Resumed => {}
                ExpeditionRecoveryOutcome::Evacuated => {
                    active = true;
                    let result = reducer_call!(
                        runner,
                        "ensure_settlement_activity_after_evacuation",
                        |cb| {
                            runner
                                .connection
                                .reducers
                                .ensure_settlement_activity_then(settlement.clone(), cb)
                        }
                    );
                    runner.call(result)?;
                    runner.choose_pending_npc_strategies()?;
                    continue;
                }
                ExpeditionRecoveryOutcome::Held => {
                    held = true;
                    if runner.public_party_elapsed_max(party_id) > party_time_before {
                        active = true;
                    }
                    continue;
                }
            }
            let Some((leader, _)) = runner.current_leader(party_id) else {
                continue;
            };
            let elapsed = runner
                .connection
                .db
                .character_time()
                .iter()
                .find(|row| row.character_id == leader)
                .ok_or("missing leader clock")?
                .minutes;
            if elapsed >= duration_minutes
                && !(recovery_outcome == ExpeditionRecoveryOutcome::Resumed
                    && recovery_started_in_budget)
            {
                continue;
            }
            match runner.continue_public_active_journey(party_id)? {
                None | Some(JourneyTravelOutcome::Completed) => {}
                Some(
                    JourneyTravelOutcome::HeldNoActionableActor
                    | JourneyTravelOutcome::HeldForRecovery,
                ) => {
                    held = true;
                    if runner.public_party_elapsed_max(party_id) > party_time_before {
                        active = true;
                    }
                    continue;
                }
            }
            let Some((leader, leader_agent)) = runner.current_leader(party_id) else {
                continue;
            };
            active = true;
            let profile = runner.profiles[leader_agent as usize].clone();
            let mixed = config.seed
                ^ u64::from(leader_agent).wrapping_mul(0x9e37_79b9_7f4a_7c15)
                ^ u64::from(cycle).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            let selector = (mixed >> 11) as f64 / ((1_u64 << 53) as f64);
            let quest_propensity = profile.activity_vs_quest_propensity;
            let wants_quest = selector < f64::from(quest_propensity);
            let party = runner.party_for(leader)?;
            let settlement_id = party.current_settlement_id.as_deref();
            let offered_contracts = settlement_id.map_or(0, |settlement_id| {
                runner
                    .connection
                    .db
                    .backend_contracts()
                    .iter()
                    .filter(|contract| {
                        contract.settlement_id == settlement_id
                            && contract.status == ContractStatus::Offered
                    })
                    .count()
            });
            let open_generated_cases = runner.owned_open_generated_cases(leader);
            for (case_id, title) in &open_generated_cases {
                runner.observe_generated_case_intake(
                    leader_agent,
                    leader,
                    case_id,
                    title,
                    "owner_projection_continuation",
                );
            }
            let projected_investigation_actions = runner
                .connection
                .db
                .backend_investigation_actions()
                .iter()
                .filter(|row| {
                    row.owner_character_id == leader
                        && open_generated_cases
                            .iter()
                            .any(|(case_id, _)| case_id == &row.case_id)
                })
                .count();
            let direct_quest_chosen =
                wants_quest && runner.choose_quest(&party, &profile).is_some();
            let active_direct_contract = runner.active_direct_contract(&party);
            let quest_path = if active_direct_contract.is_some() {
                "direct_contract_continuation"
            } else if !open_generated_cases.is_empty() {
                "generated_open_case"
            } else if direct_quest_chosen {
                "direct_contract"
            } else if wants_quest {
                "generated_discovery"
            } else {
                "activity"
            };
            let quest_selected = quest_path != "activity";
            runner.event(
                leader_agent,
                CoreLoopEventKind::QuestDecision,
                format_quest_decision_detail(
                    cycle,
                    wants_quest,
                    selector,
                    quest_propensity,
                    settlement_id,
                    offered_contracts,
                    open_generated_cases.len(),
                    projected_investigation_actions,
                    quest_path,
                    wants_quest,
                    quest_selected,
                    if quest_selected {
                        "none"
                    } else {
                        "policy_prefers_activity"
                    },
                ),
            );
            match quest_path {
                "generated_open_case" => {
                    let (case_id, title) = open_generated_cases[0].clone();
                    let progressed = runner.advance_generated_case(
                        party_id,
                        leader,
                        leader_agent,
                        cycle,
                        &case_id,
                        &title,
                    )?;
                    if !progressed && runner.party_for(leader)?.current_settlement_id.is_some() {
                        runner.settlement_activity_day(leader_agent)?;
                    }
                }
                "direct_contract" | "direct_contract_continuation" => {
                    runner.cycle(party_id, cycle)?
                }
                "generated_discovery" => {
                    let discovery = runner.discover_generated_case(leader, leader_agent, cycle)?;
                    if discovery.case_discovered() {
                        let Some((case_id, title)) =
                            runner.owned_open_generated_cases(leader).into_iter().next()
                        else {
                            continue;
                        };
                        let progressed = runner.advance_generated_case(
                            party_id,
                            leader,
                            leader_agent,
                            cycle,
                            &case_id,
                            &title,
                        )?;
                        if !progressed && runner.party_for(leader)?.current_settlement_id.is_some()
                        {
                            runner.settlement_activity_day(leader_agent)?;
                        }
                    } else {
                        runner.settlement_activity_day(leader_agent)?;
                    }
                }
                _ => runner.settlement_activity_day(leader_agent)?,
            }
            let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
                .connection
                .reducers
                .ensure_settlement_activity_then(settlement.clone(), cb));
            runner.call(result)?;
            runner.choose_pending_npc_strategies()?;
        }
        if active {
            let result = reducer_call!(runner, "advance_simulation_world_time", |cb| runner
                .connection
                .reducers
                .advance_simulation_world_time_then(
                    config.run_nonce.clone(),
                    adventuresim_core::strategic_time::MINUTES_PER_DAY,
                    cb,
                ));
            runner.call(result)?;
            let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
                .connection
                .reducers
                .ensure_settlement_activity_then(settlement.clone(), cb));
            runner.call(result)?;
            runner.choose_pending_npc_strategies()?;
        }
        if !active && held {
            break;
        }
        if !active {
            break;
        }
    }
    // One final bounded rescue pass runs even when the scenario duration or
    // cycle budget ended immediately after an off-settlement injury.
    for party_id in &party_ids {
        runner.observe_deaths();
        runner.recover_or_evacuate_off_settlement(party_id, config.cycles)?;
    }
    // Bounded final settlement cleanup prevents a duration cutoff from
    // stranding medical care or completed smith orders.
    for agent in 0..runner.character_ids.len() as u32 {
        let character_id = runner.character_ids[agent as usize];
        let at_settlement = runner
            .connection
            .db
            .character()
            .iter()
            .find(|row| row.id == character_id)
            .is_some_and(|row| row.alive && row.current_settlement_id.is_some());
        if at_settlement && runner.ensure_medically_safe(agent)? {
            runner.maintain_equipment(agent)?;
        }
    }
    runner.observe_deaths();

    let final_agents = runner
        .character_ids
        .iter()
        .enumerate()
        .map(|(agent, character_id)| {
            let character = runner
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == *character_id)
                .ok_or("missing final character")?;
            let equip = runner
                .connection
                .db
                .character_equip()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final equipment")?;
            let equipped_ids = [
                equip.left_hand_item_id,
                equip.right_hand_item_id,
                equip.left_arm_armor_id,
                equip.right_arm_armor_id,
                equip.left_leg_armor_id,
                equip.right_leg_armor_id,
                equip.head_armor_id,
                equip.chest_armor_id,
                equip.stomach_armor_id,
            ];
            let mut equipment_item_ids: Vec<String> = runner
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == *character_id)
                .filter(|row| equipped_ids.contains(&Some(row.id)))
                .map(|row| row.item_id)
                .collect();
            equipment_item_ids.sort();
            let capability = runner
                .connection
                .db
                .character_capability()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final capability")?;
            let condition = runner
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final condition")?;
            let elapsed_minutes = runner
                .connection
                .db
                .character_time()
                .iter()
                .find(|row| row.character_id == *character_id)
                .ok_or("missing final clock")?
                .minutes;
            let personal_gold_coin: u64 = runner
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == *character_id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            let worst_equipment_condition = equipped_ids
                .into_iter()
                .flatten()
                .filter_map(|id| {
                    runner
                        .connection
                        .db
                        .item_condition()
                        .iter()
                        .find(|row| row.inventory_item_id == id)
                })
                .map(|row| {
                    1.0 - (row.tier_1 + row.tier_2 + row.tier_3 + row.tier_4 + row.tier_5)
                        .clamp(0.0, 1.0)
                })
                .fold(1.0_f32, f32::min);
            let outstanding_repair_orders = runner
                .connection
                .db
                .repair_order()
                .iter()
                .filter(|row| row.owner_character_id == *character_id)
                .count() as u32;
            let party_id = character.party_id.clone().ok_or("missing final party")?;
            let party_treasury = runner
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party_id && is_currency_id(&row.item_id))
                .map(|row| u64::from(row.quantity))
                .sum();
            let party_stake = runner
                .connection
                .db
                .party_stake()
                .iter()
                .find(|row| row.party_id == party_id && row.character_id == *character_id)
                .map_or(0, |row| row.value);
            let public = runner
                .public_failure_agent(agent as u32, *character_id)
                .ok_or("missing final public diagnostic state")?;
            Ok(FinalAgentState {
                agent_id: agent as u32,
                character_id: *character_id,
                gold: personal_gold_coin.min(u64::from(u32::MAX)) as u32,
                equipment_item_ids,
                capability_summary: format!(
                    "melee={};ranged={};heavy={};athletics={:.2};endurance={:.2}",
                    capability.melee,
                    capability.ranged,
                    capability.heavy,
                    capability.athletics,
                    capability.endurance
                ),
                condition_status: condition.status,
                worst_equipment_condition,
                outstanding_repair_orders,
                alive: character.alive,
                elapsed_minutes,
                personal_gold_coin,
                party_treasury,
                party_stake,
                hunger: public.hunger,
                thirst: public.thirst,
                food_days: public.food_days,
                water_days: public.water_days,
                visible_food_kcal: public.visible_food_kcal,
                visible_water_ml: public.visible_water_ml,
                settlement_id: public.settlement_id,
                current_case_site_id: public.current_case_site_id,
                journey_destination: public.journey_destination,
                symptomatic: public.symptomatic,
                critical: public.critical,
                settlement_services: public.settlement_services,
                visible_herbalist_quote: public.visible_herbalist_quote,
                visible_inn_full_board_cost: public.visible_inn_full_board_cost,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let elapsed_game_minutes = final_agents
        .iter()
        .map(|agent| agent.elapsed_minutes)
        .max()
        .unwrap_or(0);
    let total_event_count = runner.sequence;
    let trace_truncated = total_event_count > runner.trace.len() as u64;
    let npc_intervention_stories_markdown = render_npc_intervention_stories(
        runner.connection.db.backend_npc_case_interventions().iter(),
    );
    Ok(CoreLoopReport {
        backend_kind: "spacetimedb_authoritative_core_loop".into(),
        seed: config.seed,
        server_origin: config.host.clone(),
        database: config.database,
        run_nonce: config.run_nonce,
        deployment_identity_note: "server origin, database, and claimed run nonce identify this deployment; the SDK does not expose a deployed module binary digest".into(),
        world_artifact_id: world_import.as_ref().map(|import| import.artifact_id.clone()),
        world_manifest_digest: world_import
            .as_ref()
            .map(|import| import.manifest_digest.clone()),
        starting_settlement_id,
        profiles: runner.profiles,
        metrics: runner.metrics,
        trace: runner.trace,
        trace_truncated,
        total_event_count,
        final_agents,
        elapsed_game_minutes,
        policy_seed_note: "seed controls profiles and policy choices only; authoritative autoresolve seeds are server RNG values recorded in the trace".into(),
        npc_intervention_stories_markdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_registers_and_resubscribes_gateway_before_seeding() {
        let source = include_str!("live_core.rs");
        let claim = source.find("\"claim_simulation_run\"").unwrap();
        let register = source.find("\"register_strategic_gateway\"").unwrap();
        let resubscribe = source
            .find("gateway_subscription_rx")
            .expect("post-registration gateway subscription");
        let seed = source.find("\"seed_simulation_world\"").unwrap();
        assert!(claim < register && register < resubscribe && resubscribe < seed);
        let gateway_surface = &source[resubscribe..seed];
        let contracts = gateway_surface
            .find(".backend_contracts()")
            .expect("post-registration subscription must include offered contracts");
        let subscribe = gateway_surface
            .find(".subscribe()")
            .expect("post-registration subscription must be applied");
        assert!(
            contracts < subscribe,
            "offered contracts must be part of the applied gateway subscription"
        );
    }

    #[test]
    fn live_schedule_reallocates_disabled_tactical_crime_to_legal_labor() {
        let mut profile = generate_profile(42, 0);
        profile.schedule.combat_training_minutes = 17;
        profile.schedule.apprenticeship_minutes = 60;
        profile.schedule.profession_practice_minutes = 60;
        profile.schedule.labor = 30;
        profile.schedule.prayer = 45;
        profile.schedule.thievery = 60;
        profile.schedule.raiding = 60;
        let schedule = live_schedule(&profile);
        assert_eq!(schedule.combat_training_minutes, 15);
        assert_eq!(schedule.apprenticeship_minutes, 0);
        assert_eq!(schedule.profession_practice_minutes, 0);
        assert_eq!(schedule.labor_minutes, 150);
        assert_eq!(schedule.prayer_minutes, 45);
        assert_eq!(schedule.thievery_minutes, 0);
        assert_eq!(schedule.raiding_minutes, 0);
    }

    #[test]
    fn disabled_crime_reallocation_leaves_labor_and_prayer_unchanged_without_crime() {
        let mut schedule = medical_rest_schedule();
        schedule.labor_minutes = 480;
        schedule.prayer_minutes = 60;
        assert_eq!(
            reallocate_disabled_crime_to_labor(schedule.clone()),
            schedule
        );
    }

    #[test]
    fn settlement_activity_venue_prefers_fed_temple_then_reserve_aware_inn() {
        assert_eq!(
            select_settlement_activity_venue(true, true, true, 2, 0, Some(2)),
            Some(SettlementActivityVenue::Temple)
        );
        assert_eq!(
            select_settlement_activity_venue(true, true, false, 4, 2, Some(2)),
            Some(SettlementActivityVenue::Inn)
        );
        assert_eq!(
            select_settlement_activity_venue(false, true, false, 0, 0, Some(2)),
            Some(SettlementActivityVenue::Temple)
        );
        assert_eq!(
            select_settlement_activity_venue(true, true, false, 3, 2, Some(2)),
            Some(SettlementActivityVenue::Temple)
        );
        assert_eq!(
            select_settlement_activity_venue(true, false, false, 3, 2, Some(2)),
            None
        );
    }

    #[test]
    fn temple_viability_depends_on_visible_food_not_carried_water() {
        assert!(temple_food_covers_one_day(
            adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
        ));
        assert!(!temple_food_covers_one_day(
            adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY - 1.0
        ));
    }

    #[test]
    fn committed_reserve_keeps_visible_medical_cost_and_attainable_cash_target() {
        assert_eq!(
            visible_activity_committed_reserve(9, 200, Some(6), Some(2)),
            7
        );
        assert_eq!(
            visible_activity_committed_reserve(250, 200, Some(6), Some(2)),
            206
        );
    }

    #[test]
    fn prayer_switches_to_installed_labor_plan_under_reserve_pressure() {
        let mut profile = generate_profile(42, 0);
        profile.preferred_activity = ActivityPreference::Prayer;
        profile.schedule.labor = 0;
        profile.schedule.thievery = 0;
        profile.schedule.raiding = 0;
        profile.schedule.prayer = 480;
        let (schedule, effective, fallback) =
            activity_schedule_plan(&profile, false, 2, 1, Some(2));
        assert_eq!(schedule.labor_minutes, 480);
        assert_eq!(schedule.prayer_minutes, 0);
        assert_eq!(effective, "Labor");
        assert_eq!(fallback, "subsistence_reserve_to_labor");

        let (fed_schedule, fed_effective, fed_fallback) =
            activity_schedule_plan(&profile, true, 2, 1, Some(2));
        assert_eq!(fed_schedule.prayer_minutes, 480);
        assert_eq!(fed_schedule.labor_minutes, 0);
        assert_eq!(fed_effective, "Prayer");
        assert_eq!(fed_fallback, "none");
    }

    #[test]
    fn activity_schedule_is_installed_before_the_logged_rest_attempt() {
        let source = include_str!("live_core.rs");
        let start = source
            .find("fn settlement_activity_day")
            .expect("activity policy");
        let block = &source[start
            ..source[start..]
                .find("/// NPCs use the same custody")
                .map(|offset| start + offset)
                .expect("activity policy end")];
        let install = block
            .find("install_activity_schedule")
            .expect("authoritative schedule installation");
        let rest = block
            .find("rest_at_settlement_hours_then")
            .expect("authoritative activity rest");
        assert!(install < rest);
    }

    #[test]
    fn each_active_cycle_advances_world_time_before_refreshing_npc_activity() {
        let source = include_str!("live_core.rs");
        let loop_start = source
            .find("for cycle in 0..config.cycles")
            .expect("core-loop cycle");
        let loop_end = source[loop_start..]
            .find("// Bounded final settlement cleanup")
            .map(|offset| loop_start + offset)
            .expect("core-loop cleanup");
        let active_block = &source[loop_start..loop_end];
        let advance = active_block
            .find("\"advance_simulation_world_time\"")
            .expect("simulation clock advance");
        assert!(
            active_block[advance..].contains("\"ensure_settlement_activity\""),
            "settlement activity must refresh after the simulation clock advances"
        );
    }

    #[test]
    fn quest_decision_detail_is_bounded_and_stably_formatted() {
        assert_eq!(
            format_quest_decision_detail(
                7,
                true,
                0.25,
                0.75,
                Some("lubeck"),
                2,
                1,
                3,
                "generated_open_case",
                true,
                true,
                "none",
            ),
            "cycle=7;wants_quest=true;selector=0.250000;quest_propensity=0.750000;settlement=lubeck;offered_contracts=2;open_generated_cases=1;projected_investigation_actions=3;quest_path=generated_open_case;quest_intended=true;quest_selected=true;selection_reason=none"
        );
        assert_eq!(
            format_quest_decision_detail(
                8,
                false,
                0.25,
                0.75,
                None,
                0,
                0,
                0,
                "activity",
                false,
                false,
                "policy_prefers_activity",
            ),
            "cycle=8;wants_quest=false;selector=0.250000;quest_propensity=0.750000;settlement=none;offered_contracts=0;open_generated_cases=0;projected_investigation_actions=0;quest_path=activity;quest_intended=false;quest_selected=false;selection_reason=policy_prefers_activity"
        );
    }

    #[test]
    fn quest_selection_trace_precedes_discovery_reducers() {
        let source = include_str!("live_core.rs");
        let loop_body = source
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("active core-loop body");
        let selection = loop_body
            .find("CoreLoopEventKind::QuestDecision")
            .expect("pre-action quest selection");
        let discovery = loop_body
            .find("runner.discover_generated_case")
            .expect("generated discovery reducer path");
        assert!(
            selection < discovery,
            "quest selection must be recorded before discovery dialogue"
        );
    }

    #[test]
    fn generated_case_views_filter_by_owner_and_sort_stably() {
        let rows = vec![
            (9, "case-b".into(), "B".into(), "open".into()),
            (7, "case-z".into(), "Z".into(), "open".into()),
            (9, "case-a".into(), "A".into(), "open".into()),
            (9, "case-c".into(), "C".into(), "completed".into()),
        ];
        assert_eq!(
            stable_owned_open_cases(9, rows),
            vec![
                ("case-a".to_owned(), "A".to_owned()),
                ("case-b".to_owned(), "B".to_owned())
            ]
        );
    }

    #[test]
    fn ambiguous_public_npc_candidates_remain_bounded_and_stable() {
        let candidates = vec![
            PublicNpcCandidate {
                npc_id: "npc-z".into(),
                name: "Marta".into(),
                profession: "Baker".into(),
                conversation_id: "local-resident".into(),
                location_id: "market".into(),
            },
            PublicNpcCandidate {
                npc_id: "npc-a".into(),
                name: "Marta".into(),
                profession: "Baker".into(),
                conversation_id: "local-resident".into(),
                location_id: "market".into(),
            },
        ];
        let sorted = stable_public_npc_candidates(candidates, Some("Marta"), Some("market"));
        assert_eq!(
            sorted
                .iter()
                .map(|candidate| candidate.npc_id.as_str())
                .collect::<Vec<_>>(),
            vec!["npc-a", "npc-z"]
        );
        assert_eq!(
            sorted.len(),
            2,
            "ambiguity must be resolved only by projected topic eligibility, not guessed identity"
        );
    }

    #[test]
    fn generated_discovery_uses_one_stable_representative_at_the_valid_public_location() {
        let candidate = |npc_id: &str, name: &str, location: &str| PublicNpcCandidate {
            npc_id: npc_id.into(),
            name: name.into(),
            profession: "Resident".into(),
            conversation_id: "local-resident".into(),
            location_id: location.into(),
        };
        let inn = stable_discovery_action_candidate(vec![
            candidate("npc-z", "Zelda", "inn"),
            candidate("npc-a", "Agnes", "inn"),
            candidate("npc-o", "Otto", "overview"),
            candidate("npc-m", "Marta", "market"),
        ])
        .expect("inn representative");
        assert_eq!(
            (inn.location_id.as_str(), inn.npc_id.as_str()),
            ("inn", "npc-a")
        );

        let overview = stable_discovery_action_candidate(vec![
            candidate("npc-o", "Otto", "overview"),
            candidate("npc-b", "Bertha", "overview"),
            candidate("npc-m", "Marta", "market"),
        ])
        .expect("overview fallback representative");
        assert_eq!(
            (overview.location_id.as_str(), overview.npc_id.as_str()),
            ("overview", "npc-b")
        );

        assert!(
            stable_discovery_action_candidate(vec![candidate("npc-m", "Marta", "market")])
                .is_none()
        );
    }

    #[test]
    fn generated_discovery_outcomes_do_not_conflate_selection_with_success() {
        assert!(GeneratedDiscoveryOutcome::Discovered.case_discovered());
        assert!(!GeneratedDiscoveryOutcome::NoVisibleContacts.case_discovered());
        assert!(!GeneratedDiscoveryOutcome::NoPublicRumor.case_discovered());
    }

    #[test]
    fn discovery_logging_uses_only_the_owner_visible_case_postcondition() {
        let source = include_str!("live_core.rs");
        let discovery = source
            .split("fn discover_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn try_generated_dialogue_topic").next())
            .expect("generated discovery policy");
        assert_eq!(discovery.matches("start_public_dialogue(").count(), 1);
        assert!(discovery.contains("owned_open_generated_cases(character_id)"));
        assert!(discovery.contains("rumor_delivered=true"));
        assert!(discovery.contains("reason=rumor_delivered"));
        assert!(discovery.contains("reason=no_public_rumor_available"));
        assert!(!discovery.contains("local_problem_rumor_delivery"));
        assert!(!discovery.contains("quest_generation_authority"));
    }

    #[test]
    fn generated_completion_is_attributed_only_to_the_immediate_own_transition() {
        assert_eq!(
            generated_closure_attribution("open", Some("completed"), true),
            GeneratedClosureAttribution::OwnImmediateTransition
        );
        assert_eq!(
            generated_closure_attribution("open", Some("completed"), false),
            GeneratedClosureAttribution::ExternalTransition
        );
        assert_eq!(
            generated_closure_attribution("open", Some("open"), true),
            GeneratedClosureAttribution::StillOpen
        );
    }

    #[test]
    fn generated_projection_rows_require_exact_owner_and_public_case() {
        assert!(projected_case_row_matches(7, "public-a", 7, "public-a"));
        assert!(!projected_case_row_matches(7, "public-a", 8, "public-a"));
        assert!(!projected_case_row_matches(7, "public-a", 7, "public-b"));
    }

    #[test]
    fn generated_site_selection_requires_the_exact_occupied_pin() {
        assert!(occupied_case_pin_matches(
            7, "public-a", "site-2", 7, "public-a", "site-2"
        ));
        assert!(!occupied_case_pin_matches(
            7, "public-a", "site-2", 7, "public-a", "site-1"
        ));
    }

    #[test]
    fn generated_time_gate_stops_leader_changes_and_incapacitation() {
        assert!(generated_actor_can_continue(7, Some(7), 0));
        assert!(!generated_actor_can_continue(7, Some(8), 0));
        assert!(!generated_actor_can_continue(7, Some(7), 1));
        assert!(!generated_actor_can_continue(7, None, 0));
    }

    #[test]
    fn case_site_duration_bounds_fatigue_expanded_round_trip_and_cycle16_shape() {
        assert_eq!(projected_case_site_journey_minutes(1_250, 480), Some(240));
        assert_eq!(
            projected_case_site_journey_minutes(20_000, 480),
            Some(7_680)
        );
        assert_eq!(1_503 * JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR, 6_012);
        assert!(1_503 * JOURNEY_PROVISION_ELAPSED_BOUND_FACTOR > 3_461);
        assert_eq!(projected_case_site_journey_minutes(20_000, 0), None);
        assert_eq!(projected_case_site_journey_minutes(0, 480), None);
    }

    #[test]
    fn camp_rest_uses_only_remaining_active_public_forecast_interval() {
        let intervals = vec![
            JourneyCampInterval {
                movement_minute: 480,
                elapsed_start_minute: 480,
                elapsed_minutes: 960,
                average_fatigue_start: 0.5,
                average_fatigue_end: 0.0,
                maximum_fatigue_end: 0.0,
            },
            JourneyCampInterval {
                movement_minute: 960,
                elapsed_start_minute: 1_920,
                elapsed_minutes: 960,
                average_fatigue_start: 0.5,
                average_fatigue_end: 0.0,
                maximum_fatigue_end: 0.0,
            },
        ];
        assert_eq!(
            projected_camp_rest_minutes(480, 2_500, &intervals),
            Some((480, 960))
        );
        assert_eq!(
            projected_camp_rest_minutes(1_200, 2_500, &intervals),
            Some((1_200, 240))
        );
        assert_eq!(
            projected_camp_rest_minutes(1_920, 2_500, &intervals),
            Some((1_920, 580))
        );
        assert_eq!(projected_camp_rest_minutes(1_500, 2_500, &intervals), None);
    }

    #[test]
    fn travel_driver_uses_public_itinerary_and_observer_safe_provisioning() {
        let source = include_str!("live_core.rs");
        let travel = source
            .split("fn travel_camps")
            .nth(1)
            .and_then(|tail| tail.split("fn choose_quest").next())
            .expect("travel camp driver");
        assert!(travel.contains("public_active_camp_observation(party_id)"));
        assert!(travel.contains("row.party_id == party_id"));
        assert!(!travel.contains("projected_camp_rest_minutes("));
        let coherent_camp = source
            .split("fn public_active_camp_observation")
            .nth(1)
            .and_then(|tail| {
                tail.split("fn party_has_unresolved_public_encounter")
                    .next()
            })
            .expect("shared coherent public camp helper");
        for public_projection in [".party()", ".party_journey()", ".party_journey_itinerary()"] {
            assert!(
                coherent_camp.contains(public_projection),
                "{public_projection}"
            );
        }
        assert!(coherent_camp.contains("let [journey] = journeys.as_slice()"));
        assert!(coherent_camp.contains("let [itinerary] = itineraries.as_slice()"));
        assert!(coherent_camp.contains("&journey.destination != camp_destination"));
        assert!(
            coherent_camp
                .contains("journey.completed_elapsed_minutes >= journey.total_elapsed_minutes")
        );
        assert!(coherent_camp.contains("&itinerary.forecast_camp_intervals"));
        assert!(coherent_camp.contains("projected_camp_rest_minutes("));
        assert!(
            travel.contains(
                "let Some((travel_actor, _, _)) = self.expedition_recovery_actor(party_id)"
            )
        );
        assert!(travel.contains(".rest_at_camp_then(travel_actor, rest_minutes"));
        assert!(!travel.contains(".rest_at_camp_then(travel_actor, 1_440"));
        assert!(source.contains(
            "fn travel_camps(&mut self, party_id: &str) -> Result<JourneyTravelOutcome, String>"
        ));
        for outcome in [
            "JourneyTravelOutcome::Completed",
            "JourneyTravelOutcome::HeldNoActionableActor",
            "JourneyTravelOutcome::HeldForRecovery",
        ] {
            assert!(source.contains(outcome), "{outcome}");
        }
        for hold in [
            "\"journey_stalled\"",
            "\"journey_stalled_after_encounter\"",
            "\"journey_stalled_after_rest\"",
        ] {
            assert!(travel.contains(hold), "{hold}");
        }
        assert!(
            !travel
                .contains("return Err(\"journey has no ready, asymptomatic, noncritical actor\"")
        );
        assert!(
            !travel.contains(
                "return Err(\"camp rest left no ready, asymptomatic, noncritical actor\""
            )
        );
        for phase in ["phase=pre_rest", "phase=post_rest", "phase=post_continue"] {
            assert!(travel.contains(phase));
        }

        let recovery_actor = source
            .split("fn expedition_recovery_actor")
            .nth(1)
            .and_then(|tail| tail.split("fn public_expedition_return_settlement").next())
            .expect("public recovery actor selection");
        assert!(recovery_actor.contains("expedition_member_observations(party_id)"));
        assert!(recovery_actor.contains("member.condition_status == \"ready\""));
        assert!(recovery_actor.contains("!member.symptomatic"));
        assert!(recovery_actor.contains("!member.critical"));
        assert!(recovery_actor.contains("ready.sort_by_key"));
        assert!(recovery_actor.contains("ready.into_iter().next()"));
        assert!(!recovery_actor.contains("infection_episode"));

        let provisioning = source
            .split("fn provision_case_site_journey")
            .nth(1)
            .and_then(|tail| tail.split("fn travel_camps").next())
            .expect("journey provisioner");
        for public_surface in [
            ".character_needs()",
            ".inventory_item()",
            ".party_inventory_item()",
            ".food_lot()",
            ".settlement()",
            ".backend_settlement_npcs()",
            ".settlement_npc_presence()",
            ".backend_local_problem_trade_effects()",
            ".party_stake()",
            "SettlementService::Market | SettlementService::GeneralStore",
            "finalize_merchant_trade_then(",
        ] {
            assert!(provisioning.contains(public_surface), "{public_surface}");
        }
        assert!(provisioning.contains("target_surplus_days: TRAVEL_PROVISION_RESERVE_DAYS"));
        assert!(provisioning.contains("payer_options"));
        assert!(provisioning.contains("payer_minute"));
        assert!(provisioning.contains("merchant_count != 1"));
        assert!(provisioning.contains("journey_finance_backoff"));
        assert!(provisioning.contains("(party_id.to_owned(), leader, finance_key.to_owned())"));
        assert!(!provisioning.contains(".map_or(0, |row| row.buy_bps)"));
        assert!(!provisioning.contains("party_journey_route"));
    }

    #[test]
    fn journey_holds_are_publicly_diagnosable_and_block_arrival_assumptions() {
        let source = include_str!("live_core.rs");
        let hold = source
            .split("fn record_journey_hold")
            .nth(1)
            .and_then(|tail| tail.split("fn expedition_recovery_actor").next())
            .expect("journey hold diagnostics");
        for public_field in [
            "reason={}",
            "journey_completed_elapsed=",
            "journey_total_elapsed=",
            "journey_remaining_elapsed=",
            "journey_destination=",
            "camp_remaining_minutes=",
            "active_forecast_interval_start=",
            "active_forecast_interval_minutes=",
            "living_count=",
            "one_day_food_kcal_required=",
            "stored_food_kcal=",
            "one_day_water_ml_required=",
            "portable_water_ml=",
            "supplies_cover_one_rest_day=",
        ] {
            assert!(hold.contains(public_field), "{public_field}");
        }
        assert!(hold.contains("bounded_event_field(reason)"));
        assert!(hold.contains(".party_journey()"));
        assert!(hold.contains(".party_journey_itinerary()"));
        assert!(!hold.contains("infection_episode"));
        assert!(!hold.contains("disease"));

        let generated = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        let travel_guard = generated
            .find("journey_outcome != JourneyTravelOutcome::Completed")
            .expect("typed generated travel guard");
        let traveled_marker = generated
            .find("self.generated_traveled_cases.insert(funnel_key)")
            .expect("generated traveled marker");
        assert!(travel_guard < traveled_marker);

        let recovery = source
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn generated_case_status").next())
            .expect("expedition recovery driver");
        assert!(recovery.contains("\"recovery_plan\""));
        assert!(recovery.contains("\"evacuation_plan\""));
        assert!(
            recovery.contains("self.travel_camps(party_id)? != JourneyTravelOutcome::Completed")
        );
        assert!(recovery.contains("return Ok(ExpeditionRecoveryOutcome::Held)"));
    }

    #[test]
    fn direct_contract_provisions_before_acceptance_and_never_defers_by_abandoning() {
        let source = include_str!("live_core.rs");
        let quest = source
            .split("fn cycle")
            .nth(1)
            .and_then(|tail| tail.split("fn try_upgrade").next())
            .expect("direct contract driver");
        assert!(
            quest.find("provision_case_site_journey").unwrap()
                < quest.find("accept_contract_then").unwrap()
        );
        assert!(!quest.contains("defer_unprovisioned_contract"));
        assert!(quest.contains("failed_expedition_cannot_reprovision"));
        assert!(quest.contains(".min_by_key(|site| (site.distance_m, site.case_site_id.clone()))"));
        assert!(quest.matches("provision_case_site_journey").count() >= 2);
        assert!(
            quest.contains("accepted contract provisioning projection changed after disclosure")
        );
        assert!(quest.contains("refreshed_safe_party_for_owner(party_id, quest_owner)"));
    }

    #[test]
    fn recovery_audit_separates_public_before_and_after_observations() {
        let source = include_str!("live_core.rs");
        let recovery = source
            .split("fn ensure_medically_safe")
            .nth(1)
            .and_then(|tail| tail.split("fn settlement_activity_day").next())
            .expect("medical recovery driver");
        assert!(recovery.contains("let symptomatic_after ="));
        assert!(recovery.contains("recovery_context=public_symptoms"));
        assert!(recovery.contains("symptomatic_before={symptomatic}"));
        assert!(recovery.contains("symptomatic_after={symptomatic_after}"));
        assert!(!recovery.contains("cause=public_symptomatic_illness"));
    }

    #[test]
    fn off_settlement_recovery_is_bounded_public_and_precedes_quest_selection() {
        let recovering = ExpeditionMemberObservation {
            agent_id: 0,
            character_id: 7,
            alive: true,
            condition_status: "incapacitated".into(),
            hunger: 0.1,
            thirst: 0.2,
            food_days: 3.0,
            water_days: 3.0,
            symptomatic: false,
            critical: false,
            elapsed_minutes: 1_440,
        };
        assert!(expedition_member_needs_recovery(&recovering));
        assert!(!expedition_member_needs_recovery(
            &ExpeditionMemberObservation {
                condition_status: "ready".into(),
                ..recovering.clone()
            }
        ));
        assert_eq!(MAX_EXPEDITION_RECOVERY_RESTS, 2);
        assert!(!expedition_party_can_resume(&[recovering.clone()]));
        assert!(expedition_party_can_resume(&[
            ExpeditionMemberObservation {
                condition_status: "ready".into(),
                ..recovering.clone()
            }
        ]));
        assert!(!expedition_party_can_resume(&[]));
        let one_day = ExpeditionSuppliesObservation {
            stored_food_kcal: adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY,
            portable_water_ml: adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY,
        };
        assert!(expedition_supplies_cover_one_rest_day(
            &[recovering.clone()],
            one_day
        ));
        assert!(!expedition_supplies_cover_one_rest_day(
            &[recovering.clone(), recovering.clone()],
            one_day
        ));
        assert_eq!(
            select_expedition_encounter_choice(
                &["attack".into(), "run".into(), "detour".into()],
                0,
                true,
            ),
            Some("run".into())
        );

        let source = include_str!("live_core.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let cycle = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        assert!(cycle.find("recover_or_evacuate_off_settlement") < cycle.find("QuestDecision"));
        let recovery = production
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn owned_open_generated_cases").next())
            .expect("expedition recovery policy");
        for public_input in [
            "expedition_member_observations",
            "expedition_supplies",
            "public_expedition_return_settlement",
            "party_journey()",
            "backend_case_site_pins()",
        ] {
            assert!(production.contains(public_input));
        }
        assert!(recovery.contains("MAX_EXPEDITION_RECOVERY_RESTS"));
        assert!(recovery.contains("expedition_supplies_cover_one_rest_day"));
        assert!(recovery.contains("expedition_party_can_resume"));
        assert!(recovery.contains("evacuation_stalled"));
        assert!(production.contains("\"ready_companion\""));
        assert!(recovery.contains("travel_to_settlement_then"));
        assert!(!recovery.contains("infection_episode"));
        assert!(!recovery.contains("party_journey_route"));
        let actor = production
            .split("fn expedition_recovery_actor")
            .nth(1)
            .and_then(|tail| tail.split("fn public_expedition_return_settlement").next())
            .expect("recovery actor");
        assert!(!actor.contains("current_leader("));
        assert!(production.contains("final bounded rescue pass"));
    }

    #[test]
    fn passive_no_actionable_recovery_is_camp_only_typed_and_publicly_gated() {
        let staggered_leader = ExpeditionMemberObservation {
            agent_id: 0,
            character_id: 7,
            alive: true,
            condition_status: "staggered".into(),
            hunger: 0.1,
            thirst: 0.2,
            food_days: 3.0,
            water_days: 3.0,
            symptomatic: false,
            critical: false,
            elapsed_minutes: 1_440,
        };
        let incapacitated_companion = ExpeditionMemberObservation {
            agent_id: 1,
            character_id: 8,
            condition_status: "incapacitated".into(),
            ..staggered_leader.clone()
        };
        let members = [staggered_leader.clone(), incapacitated_companion.clone()];
        let supplies = ExpeditionSuppliesObservation {
            stored_food_kcal: 2.0 * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY,
            portable_water_ml: 2.0
                * adventuresim_core::provisioning::STRATEGIC_TRAVEL_WATER_ML_PER_DAY,
        };
        assert!(passive_no_actionable_rest_allowed(
            &members, supplies, true, true, 7, false
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, false, true, 7, false
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, true, false, 7, false
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, true, true, 99, false
        ));
        assert!(passive_no_actionable_rest_allowed(
            &[
                ExpeditionMemberObservation {
                    condition_status: "ready".into(),
                    symptomatic: true,
                    ..staggered_leader.clone()
                },
                incapacitated_companion.clone(),
            ],
            supplies,
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &[
                ExpeditionMemberObservation {
                    critical: true,
                    ..staggered_leader
                },
                incapacitated_companion,
            ],
            supplies,
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &[
                ExpeditionMemberObservation {
                    condition_status: "unavailable".into(),
                    ..members[0].clone()
                },
                members[1].clone(),
            ],
            supplies,
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members,
            ExpeditionSuppliesObservation {
                stored_food_kcal: supplies.stored_food_kcal - 1.0,
                ..supplies
            },
            true,
            true,
            7,
            false,
        ));
        assert!(!passive_no_actionable_rest_allowed(
            &members, supplies, true, true, 7, true,
        ));

        let source = include_str!("live_core.rs");
        let selector = source
            .split("fn expedition_recovery_rest_actor")
            .nth(1)
            .and_then(|tail| tail.split("fn perform_expedition_recovery_rest").next())
            .expect("typed recovery-rest actor selector");
        assert!(selector.contains("ExpeditionRecoveryRestActor::Actionable"));
        assert!(selector.contains("ExpeditionRecoveryRestActor::PassiveNoActionable"));
        assert!(selector.contains("passive_no_actionable_rest_allowed"));
        assert!(selector.contains("party_has_unresolved_public_encounter"));
        assert!(selector.contains("public_active_camp_observation"));

        let camp_predicate = source
            .split("fn public_active_camp_observation")
            .nth(1)
            .and_then(|tail| {
                tail.split("fn party_has_unresolved_public_encounter")
                    .next()
            })
            .expect("coherent public active-camp predicate");
        assert!(camp_predicate.contains("let [journey] = journeys.as_slice()"));
        assert!(camp_predicate.contains("let [itinerary] = itineraries.as_slice()"));
        assert!(camp_predicate.contains("&journey.destination != camp_destination"));
        assert!(
            camp_predicate
                .contains("journey.completed_elapsed_minutes >= journey.total_elapsed_minutes")
        );
        assert!(camp_predicate.contains("projected_camp_rest_minutes("));

        let passive_call_boundary = source
            .split("fn perform_expedition_recovery_rest")
            .nth(1)
            .and_then(|tail| tail.split("fn public_expedition_return_settlement").next())
            .expect("passive recovery-rest call boundary");
        assert!(passive_call_boundary.contains("PassiveNoActionable"));
        assert!(passive_call_boundary.contains(".rest_at_camp_then("));
        for forbidden in [
            "continue_camp_travel",
            "resolve_strategic_encounter",
            "travel_to_case_site",
            "travel_to_settlement",
            "perform_investigation_action",
            "accept_contract",
            "report_contract",
            "vote_for_party_leader",
        ] {
            assert!(
                !passive_call_boundary.contains(forbidden),
                "passive actor reached {forbidden}"
            );
        }

        let recovery = source
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn generated_case_status").next())
            .expect("expedition recovery policy");
        assert!(recovery.contains("self.expedition_recovery_rest_actor(party_id)"));
        assert!(recovery.contains("self.perform_expedition_recovery_rest(rest_actor)"));
        assert!(recovery.contains("passive_no_actionable_rest_"));
        assert!(!recovery.contains(".rest_at_camp_then("));
        assert!(recovery.contains("journey_held_unresolved_encounter"));
        assert!(recovery.contains("journey_held_incoherent_public_camp"));

        let before = [
            ExpeditionMemberObservation {
                elapsed_minutes: 100,
                ..members[0].clone()
            },
            ExpeditionMemberObservation {
                elapsed_minutes: 120,
                ..members[1].clone()
            },
        ];
        let after = [
            ExpeditionMemberObservation {
                elapsed_minutes: 160,
                ..members[0].clone()
            },
            ExpeditionMemberObservation {
                elapsed_minutes: 180,
                ..members[1].clone()
            },
        ];
        assert_eq!(expedition_elapsed_delta(&before, &after), 60);
        assert!(recovery.contains("requested_minutes={EXPEDITION_RECOVERY_REST_MINUTES}"));
        assert!(recovery.contains("actual_elapsed_minutes={actual_elapsed_minutes}"));
        assert!(
            recovery.find("expedition_passive_rest_attempts").unwrap()
                < recovery
                    .find("perform_expedition_recovery_rest(rest_actor)")
                    .unwrap()
        );
        assert!(
            recovery.find("expedition_passive_rest_minutes").unwrap()
                > recovery.find("actual_elapsed_minutes =").unwrap()
        );
    }

    #[test]
    fn recovery_outcomes_resume_same_cycle_but_consume_evacuation_or_hold() {
        assert_ne!(
            ExpeditionRecoveryOutcome::Resumed,
            ExpeditionRecoveryOutcome::Evacuated
        );
        let source = include_str!("live_core.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let cycle = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        assert!(cycle.contains("let recovery_outcome ="));
        assert!(cycle.contains("let recovery_started_in_budget ="));
        assert!(cycle.contains("if !recovery_started_in_budget"));
        assert!(cycle.contains("recovery_outcome == ExpeditionRecoveryOutcome::Resumed"));
        assert!(cycle.contains("&& recovery_started_in_budget"));
        assert!(cycle.contains("ExpeditionRecoveryOutcome::Evacuated =>"));
        assert!(cycle.contains("ExpeditionRecoveryOutcome::Held =>"));
        let resumed = cycle.find("ExpeditionRecoveryOutcome::Resumed").unwrap();
        let quest_decision = cycle.find("CoreLoopEventKind::QuestDecision").unwrap();
        assert!(resumed < quest_decision);
        assert!(
            cycle.find("if !recovery_started_in_budget").unwrap()
                < cycle.find("recover_or_evacuate_off_settlement").unwrap()
        );
    }

    #[test]
    fn public_journeys_resume_generated_and_direct_state_without_duplicate_metrics() {
        let source = include_str!("live_core.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let core_loop = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        assert!(
            core_loop.find("continue_public_active_journey").unwrap()
                < core_loop.find("CoreLoopEventKind::QuestDecision").unwrap()
        );
        assert!(core_loop.contains("\"generated_open_case\""));
        assert!(core_loop.contains("\"direct_contract_continuation\""));
        assert!(core_loop.contains("runner.active_direct_contract(&party)"));

        let active_contract = production
            .split("fn active_direct_contract")
            .nth(1)
            .and_then(|tail| tail.split("fn personal_gold").next())
            .expect("public active contract selector");
        for public_identity in [
            "party.active_contract_id",
            ".backend_contracts()",
            "contract.accepted_by",
            "ContractStatus::Accepted | ContractStatus::ReadyToReport",
        ] {
            assert!(
                active_contract.contains(public_identity),
                "{public_identity}"
            );
        }

        let direct = production
            .split("fn cycle")
            .nth(1)
            .and_then(|tail| tail.split("fn try_upgrade").next())
            .expect("direct contract driver");
        let attempt_metrics = direct.find("self.metrics.quests_attempted += 1").unwrap();
        let new_contract_guard = direct.find("if !resuming_contract").unwrap();
        assert!(new_contract_guard < attempt_metrics);
        assert!(direct.contains("if quest.status == ContractStatus::ReadyToReport"));
        assert!(direct.contains("already_at_case_site"));

        let turn_in = production
            .split("fn turn_in_ready_direct_contract")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("direct contract turn-in");
        assert!(turn_in.contains("direct_contract_report_arrival_not_proven"));
        assert!(turn_in.contains("ContractStatus::ReadyToReport"));
        assert_eq!(
            turn_in
                .matches("self.metrics.quests_completed += 1")
                .count(),
            1
        );
        assert_eq!(
            turn_in
                .matches("self.metrics.direct_contracts_completed += 1")
                .count(),
            1
        );
    }

    #[test]
    fn recovery_reselects_each_rest_actor_and_held_only_cycles_do_not_advance_time() {
        let source = include_str!("live_core.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let recovery = production
            .split("fn recover_or_evacuate_off_settlement")
            .nth(1)
            .and_then(|tail| tail.split("fn owned_open_generated_cases").next())
            .expect("expedition recovery policy");
        let loop_start = recovery
            .find("for rest_ordinal in 1..=MAX_EXPEDITION_RECOVERY_RESTS")
            .unwrap();
        let reselection = recovery[loop_start..]
            .find("self.expedition_recovery_rest_actor(party_id)")
            .unwrap();
        let rest_call = recovery[loop_start..]
            .find("self.perform_expedition_recovery_rest(rest_actor)")
            .unwrap();
        assert!(reselection < rest_call);
        assert!(recovery.contains("field_recovery_actor_reselection"));

        let core_loop = production
            .split("for cycle in 0..config.cycles")
            .nth(1)
            .expect("core loop");
        let held_branch = core_loop
            .split("ExpeditionRecoveryOutcome::Held =>")
            .nth(1)
            .and_then(|tail| tail.split("let Some((leader, leader_agent))").next())
            .expect("held branch");
        assert!(held_branch.contains("held = true"));
        assert!(!held_branch.contains("active = true;\n                    continue"));
        assert!(held_branch.contains("public_party_elapsed_max(party_id) > party_time_before"));
        assert!(core_loop.contains("if active {"));
        assert!(core_loop.contains("advance_simulation_world_time"));
        assert!(core_loop.contains("if !active && held"));
    }

    #[test]
    fn generated_case_tracking_is_owner_scoped_and_intake_drives_attempts() {
        let mut seen = HashSet::new();
        assert!(seen.insert((7_u64, "same-case".to_owned())));
        assert!(seen.insert((8_u64, "same-case".to_owned())));
        assert!(!seen.insert((7_u64, "same-case".to_owned())));
        let mut finance_blocks = HashMap::new();
        finance_blocks.insert(
            ("party".to_owned(), 7_u64, "same-case".to_owned()),
            (12_u64, 3_u64),
        );
        assert!(
            finance_blocks
                .get(&("party".to_owned(), 8_u64, "same-case".to_owned()))
                .is_none()
        );

        let source = include_str!("live_core.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("generated_seen_cases: HashSet<(u64, String)>"));
        assert!(production.contains("generated_terminal_cases: HashSet<(u64, String)>"));
        assert!(
            production
                .contains("generated_finance_blocks: HashMap<(String, u64, String), (u64, u64)>")
        );
        assert!(production.contains("CoreLoopEventKind::GeneratedCaseIntake"));
        assert!(production.contains("source == \"owner_projection_continuation\""));
        assert!(production.contains(
            "self.metrics.quests_attempted = self.metrics.quests_attempted.saturating_add(1)"
        ));
        assert!(!production.contains("generated_case_owners: HashMap<String, u64>"));
    }

    #[test]
    fn quest_preparation_revalidates_public_owner_leadership_and_party_safety() {
        let source = include_str!("live_core.rs");
        let helper = source
            .split("fn refreshed_safe_party_for_owner")
            .nth(1)
            .and_then(|tail| tail.split("fn emit_generated_investigation_attempt").next())
            .expect("post-preparation safety gate");
        assert!(helper.contains("self.observe_deaths()"));
        assert!(helper.contains("current_leader != owner_character_id"));
        assert!(helper.contains("self.unsafe_party_agents(&party_agents)"));
        assert!(helper.contains("party.id != party_id"));

        let generated = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        assert!(generated.contains("refreshed_safe_party_for_owner(party_id, character_id)"));
    }

    #[test]
    fn final_agent_diagnostics_expose_public_remote_and_illness_state() {
        let value = serde_json::to_value(CoreLoopFailureAgent {
            agent_id: 0,
            character_id: 7,
            alive: true,
            condition_status: "ready".into(),
            hunger: 0.0,
            thirst: 0.0,
            food_days: 0.0,
            water_days: 0.0,
            visible_food_kcal: 0.0,
            visible_water_ml: 0.0,
            personal_gold_coin: 0,
            settlement_id: None,
            current_case_site_id: Some("site:known".into()),
            journey_destination: Some("settlement:return".into()),
            symptomatic: true,
            critical: false,
            settlement_services: Vec::new(),
            visible_herbalist_quote: None,
            visible_inn_full_board_cost: None,
        })
        .unwrap();
        for field in [
            "current_case_site_id",
            "journey_destination",
            "symptomatic",
            "critical",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
    }

    #[test]
    fn expedition_diagnostics_include_each_public_health_and_supply_boundary() {
        let source = include_str!("live_core.rs");
        let diagnostics = source
            .split("fn emit_expedition_diagnostics")
            .nth(1)
            .and_then(|tail| tail.split("fn expedition_recovery_actor").next())
            .expect("expedition diagnostics");
        for field in [
            "condition_before",
            "condition_after",
            "hunger_before",
            "hunger_after",
            "thirst_before",
            "thirst_after",
            "symptomatic_before",
            "symptomatic_after",
            "critical_before",
            "critical_after",
            "exposure=not_publicly_projected",
            "elapsed_delta",
            "stored_food_kcal_consumed",
            "portable_water_ml_consumed",
        ] {
            assert!(diagnostics.contains(field), "missing {field}");
        }
        assert!(source.contains(
            "quest_suppressed_member_not_ready_after_leg;plan=off_settlement_recovery_next_cycle"
        ));
        assert!(source.contains(".any(expedition_member_needs_recovery)"));
    }

    #[test]
    fn generated_event_fields_are_single_line_and_bounded() {
        let field = bounded_event_field(&format!("title;\n{}", "x".repeat(400)));
        assert!(!field.contains(';'));
        assert!(!field.contains('\n'));
        assert_eq!(field.chars().count(), 240);
    }

    #[test]
    fn generated_case_state_machine_is_bounded_and_precedes_direct_contracts() {
        assert!(MAX_GENERATED_CASE_STEPS_PER_CYCLE <= 32);
        let source = include_str!("live_core.rs");
        let loop_start = source.find("let quest_path = if").unwrap();
        let decision =
            &source[loop_start..source[loop_start..].find("runner.event(").unwrap() + loop_start];
        assert!(
            decision.find("!open_generated_cases.is_empty()").unwrap()
                < decision.find("direct_quest_chosen").unwrap()
        );
        let driver = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        assert!(driver.contains("action.unavailable_reason_code"));
        assert!(driver.contains("action.wait_minutes"));
        assert!(driver.contains("wait_for_generated_investigation_window"));
        assert!(!driver.contains("action.unavailable_reason.contains"));
    }

    #[test]
    fn generated_runner_subscribes_only_to_public_projection_inventory() {
        let source = include_str!("live_core.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for required in [
            ".backend_settlement_npcs()",
            ".settlement_npc_presence()",
            ".backend_dialogue_sessions()",
            ".backend_dialogue_topic_options()",
            ".backend_investigation_cases()",
            ".backend_investigation_journal()",
            ".backend_investigation_leads()",
            ".backend_investigation_actions()",
            ".backend_investigation_action_outcomes()",
            ".backend_case_site_pins()",
            ".party_journey()",
            ".party_journey_itinerary()",
        ] {
            assert!(
                production.contains(required),
                "missing safe projection {required}"
            );
        }
        for forbidden in [
            ".quest_generation_authority()",
            ".case_authority()",
            ".case_finale_authority()",
            ".hostile_group_authority()",
            ".mission_authority()",
            ".party_journey_route()",
            ".case_outcome()",
            ".case_outcome_fact()",
        ] {
            assert!(
                !production.contains(forbidden),
                "runner must not import private authority {forbidden}"
            );
        }
        assert!(!production.contains("receive_local_problem_rumor_then"));
    }

    #[test]
    fn activity_detail_exposes_public_pre_post_values_and_signed_deltas() {
        let mut schedule = medical_rest_schedule();
        schedule.labor_minutes = 480;
        let before = ActivityObservation {
            personal_gold_coin: 4,
            condition_status: "ready".into(),
            hunger: 0.125,
            thirst: 0.25,
            food_days: 1.0,
            water_days: 2.0,
            visible_food_kcal: 2_000.0,
            visible_water_ml: 4_000.0,
            elapsed_minutes: 1_440,
        };
        let after = ActivityObservation {
            personal_gold_coin: 9,
            condition_status: "recovering".into(),
            hunger: 0.5,
            thirst: 0.125,
            food_days: 0.0,
            water_days: 0.25,
            visible_food_kcal: 0.0,
            visible_water_ml: 500.0,
            elapsed_minutes: 2_880,
        };
        assert_eq!(
            format_activity_detail(
                "Thievery",
                "Labor",
                &schedule,
                SettlementActivityVenue::Temple,
                "crime_disabled_to_labor",
                2,
                &before,
                &after,
            ),
            "outcome=completed;preferred=Thievery;effective=Labor;fallback=crime_disabled_to_labor;venue=temple;committed_reserve=2;schedule=combat:0,carousing:0,apprenticeship:0,profession:0,labor:480,prayer:0,thievery:0,raiding:0;purse_before=4;purse_after=9;purse_delta=+5;condition_before=ready;condition_after=recovering;hunger_before=0.125;hunger_after=0.500;hunger_delta=+0.375;thirst_before=0.250;thirst_after=0.125;thirst_delta=-0.125;food_kcal_before=2000;food_kcal_after=0;food_kcal_delta=-2000.000;water_ml_before=4000;water_ml_after=500;water_ml_delta=-3500.000;elapsed_before=1440;elapsed_after=2880;elapsed_delta=+1440"
        );
        assert_eq!(
            format_failed_activity_detail(
                "Thievery",
                "Labor",
                &schedule,
                SettlementActivityVenue::Temple,
                "crime_disabled_to_labor",
                2,
                &before,
                "insufficient_visible_resources",
            ),
            "outcome=failed;stage=rest_at_settlement;error_category=insufficient_visible_resources;preferred=Thievery;effective=Labor;fallback=crime_disabled_to_labor;venue=temple;committed_reserve=2;schedule=combat:0,carousing:0,apprenticeship:0,profession:0,labor:480,prayer:0,thievery:0,raiding:0;requested_minutes=1440;purse_before=4;condition_before=ready;hunger_before=0.125;thirst_before=0.250;food_kcal_before=2000;water_ml_before=4000;elapsed_before=1440"
        );
    }

    #[test]
    fn effective_activity_distinguishes_authored_policy_from_safe_fallback() {
        let mut labor = generate_profile(42, 0);
        labor.preferred_activity = ActivityPreference::Labor;
        labor.schedule.labor = 480;
        labor.schedule.prayer = 0;
        let (_, effective, fallback) = activity_schedule_plan(&labor, true, 0, 0, Some(2));
        assert_eq!((effective, fallback), ("Labor", "none"));

        labor.preferred_activity = ActivityPreference::Thievery;
        labor.schedule.labor = 0;
        labor.schedule.thievery = 480;
        let (_, effective, fallback) = activity_schedule_plan(&labor, true, 0, 0, Some(2));
        assert_eq!((effective, fallback), ("Labor", "crime_disabled_to_labor"));
    }

    #[test]
    fn failed_activity_error_classification_never_echoes_raw_backend_text() {
        let raw = "Not enough coin: secret internal reducer context";
        let category = safe_core_loop_failure(raw).0;
        let detail = format_failed_activity_detail(
            "Prayer",
            "Prayer",
            &medical_rest_schedule(),
            SettlementActivityVenue::Temple,
            "none",
            0,
            &ActivityObservation {
                personal_gold_coin: 0,
                condition_status: "ready".into(),
                hunger: 0.0,
                thirst: 0.0,
                food_days: 0.0,
                water_days: 0.0,
                visible_food_kcal: 0.0,
                visible_water_ml: 0.0,
                elapsed_minutes: 0,
            },
            category,
        );
        assert!(detail.contains("error_category=insufficient_visible_resources"));
        assert!(!detail.contains("secret internal reducer context"));
        assert_eq!(
            safe_core_loop_failure("simulation settlement offers neither an Inn nor a Temple").0,
            "rest_service_unavailable"
        );
    }

    #[test]
    fn failure_artifact_version_five_serializes_safe_operation_context() {
        let artifact = CoreLoopFailureArtifact {
            schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
            category: "investigation_temporally_unavailable".into(),
            message:
                "A projected investigation action was attempted outside its learned time window."
                    .into(),
            operation: Some("perform_investigation_action".into()),
            reason_code: "investigation_night_window".into(),
            metrics: CoreLoopMetrics::default(),
            total_event_count: 1,
            trace_truncated: false,
            trace: vec![CoreLoopEvent {
                sequence: 1,
                agent_id: 0,
                kind: CoreLoopEventKind::QuestDecision,
                detail: "quest_path=generated_discovery;fallback=none".into(),
            }],
            final_agents: Vec::new(),
        };
        let value = serde_json::to_value(artifact).unwrap();
        assert_eq!(value["schema_version"], serde_json::json!(5));
        assert_eq!(
            value["operation"],
            serde_json::json!("perform_investigation_action")
        );
        assert_eq!(
            value["reason_code"],
            serde_json::json!("investigation_night_window")
        );
        assert_eq!(value["trace"][0]["kind"], "quest_decision");
    }

    #[test]
    fn expected_investigation_failure_is_allowlisted_without_raw_text() {
        let raw = "perform_investigation_action failed: The learned pattern requires acting during the nighttime window; hidden authority";
        let (category, message) = safe_core_loop_failure(raw);
        assert_eq!(category, "investigation_temporally_unavailable");
        assert_eq!(
            safe_failure_operation(raw),
            Some("perform_investigation_action")
        );
        assert_eq!(
            safe_failure_reason_code(raw, category),
            "investigation_night_window"
        );
        assert!(!message.contains("hidden authority"));
    }

    #[test]
    fn invalid_investigation_route_is_allowlisted_without_raw_text() {
        let raw = "perform_investigation_action failed: Investigation track origin no longer matches the projected route; hidden canonical action";
        let (category, message) = safe_core_loop_failure(raw);
        assert_eq!(category, "invalid_investigation_route");
        assert_eq!(
            safe_failure_operation(raw),
            Some("perform_investigation_action")
        );
        assert_eq!(
            safe_failure_reason_code(raw, category),
            "invalid_investigation_route"
        );
        assert!(!message.contains("hidden canonical action"));
        assert!(!message.contains(raw));
    }

    #[test]
    fn victim_cohort_state_changes_are_narrowly_classified_without_raw_text() {
        for detail in VICTIM_COHORT_STATE_CHANGED_DETAILS {
            let raw = format!("perform_investigation_action failed: {detail}");
            let (category, message) = safe_core_loop_failure(&raw);
            assert_eq!(category, "investigation_state_changed");
            assert_eq!(
                safe_failure_operation(&raw),
                Some("perform_investigation_action")
            );
            assert_eq!(
                safe_failure_reason_code(&raw, category),
                "investigation_victim_cohort_state_changed"
            );
            assert!(!message.contains(detail));
        }
        assert!(!victim_cohort_state_changed_failure(
            "perform_investigation_action failed: Victim cohort belongs to another case"
        ));
        assert!(!victim_cohort_state_changed_failure(
            "choose_dialogue_topic failed: Victim cohort target is unavailable"
        ));
    }

    #[test]
    fn generated_action_trace_uses_subject_and_public_attempt_evidence() {
        let source = include_str!("live_core.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production live core");
        let advance = source
            .split("fn advance_generated_case")
            .nth(1)
            .and_then(|tail| tail.split("fn cycle").next())
            .expect("generated case driver");
        assert!(advance.contains("emit_generated_investigation_attempt"));
        assert!(source.contains("actor_time={actor_time};party_time_min={party_time_min};party_time_max={party_time_max}"));
        assert!(
            advance.contains("\"case={};subject={};action={};method={};summary={};outcome={}\"")
        );
        assert!(
            !advance.contains("\"case={};title={};action={};method={};summary={};outcome={}\"")
        );
        assert!(advance.contains("identical_pending_subscription"));
        assert!(advance.contains("Defer once so the next cycle chooses"));
        assert!(!advance.contains("bounded_retry"));
        assert!(!production.contains("generated_investigation_retries"));
    }

    #[test]
    fn discovery_contact_failures_are_sanitized_without_reducer_text() {
        let raw =
            "start_discovery_dialogue failed: public discovery contact failed; hidden authority";
        let (category, message) = safe_core_loop_failure(raw);
        assert_eq!(category, "discovery_contact_failed");
        assert_eq!(
            safe_failure_operation(raw),
            Some("start_discovery_dialogue")
        );
        assert_eq!(
            safe_failure_reason_code(raw, category),
            "discovery_contact_failed"
        );
        assert!(!message.contains("hidden authority"));
    }

    #[test]
    fn journey_camp_failures_are_allowlisted_without_raw_text() {
        let temporal = "continue_camp_travel failed: Rest until the party reaches its next daylight walking window; hidden route authority";
        let (category, message) = safe_core_loop_failure(temporal);
        assert_eq!(category, "journey_temporally_unavailable");
        assert_eq!(
            safe_failure_operation(temporal),
            Some("continue_camp_travel")
        );
        assert_eq!(
            safe_failure_operation(
                "rest_at_camp failed: journey camp projection is incoherent; hidden details"
            ),
            Some("rest_at_camp")
        );
        assert_eq!(
            safe_failure_reason_code(temporal, category),
            "journey_daylight_window_rest_required"
        );
        assert!(!message.contains("hidden route authority"));

        let incoherent = "journey camp projection is incoherent: hidden itinerary implementation";
        let (category, message) = safe_core_loop_failure(incoherent);
        assert_eq!(category, "journey_projection_inconsistent");
        assert_eq!(
            safe_failure_reason_code(incoherent, category),
            "journey_projection_inconsistent"
        );
        assert!(!message.contains("hidden itinerary implementation"));

        let purchase = "purchase_journey_provisions failed: Merchant service provider is not available; hidden provider";
        let (category, message) = safe_core_loop_failure(purchase);
        assert_eq!(category, "journey_provision_purchase_failed");
        assert_eq!(
            safe_failure_operation(purchase),
            Some("purchase_journey_provisions")
        );
        assert_eq!(
            safe_failure_reason_code(purchase, category),
            "journey_provision_purchase_failed"
        );
        assert!(!message.contains("hidden provider"));

        let held = "travel_camps failed: journey has no ready, asymptomatic, noncritical actor; hidden health authority";
        let (category, message) = safe_core_loop_failure(held);
        assert_eq!(category, "journey_held_no_actionable_actor");
        assert_eq!(safe_failure_operation(held), Some("travel_camps"));
        assert_eq!(
            safe_failure_reason_code(held, category),
            "journey_held_no_actionable_actor"
        );
        assert!(!message.contains("hidden health authority"));
    }

    #[test]
    fn projected_night_wait_hints_are_strictly_bounded() {
        assert_eq!(
            projected_investigation_wait_minutes("night_window", 840),
            Some(840)
        );
        assert_eq!(
            projected_investigation_wait_minutes("travel_required", 840),
            None
        );
        assert_eq!(
            projected_investigation_wait_minutes("night_window", 0),
            None
        );
        assert_eq!(
            projected_investigation_wait_minutes("night_window", 1_441),
            None
        );
    }

    #[test]
    fn repeated_daily_quest_decisions_are_not_semantic_duplicate_failures() {
        assert!(event_is_repeatable(&CoreLoopEventKind::QuestDecision));
        assert!(!event_is_repeatable(&CoreLoopEventKind::AcceptContract));
    }

    #[test]
    fn authoritative_npc_story_renderer_preserves_server_markdown() {
        let exact = "> **Marta:** I heard the cart after midnight.\n";
        let story = render_npc_intervention_stories([BackendNpcCaseIntervention {
            intervention_id: "npc-intervention:case:1:1".into(),
            public_case_id: "journal:one".into(),
            party_name: "Marta's Company".into(),
            attempt: 1,
            started_at: 12,
            completed_at: 12,
            strategy: "InvestigateCarefully".into(),
            route: "Physical trail".into(),
            lead_summary: "Marta heard a cart after midnight.".into(),
            preparation_summary: "The company brought lanterns and rope.".into(),
            outcome: "Resolved".into(),
            safe_summary: "The incidents ended.".into(),
            public_story_markdown: format!("## Story\n\n{exact}"),
        }]);
        assert!(story.contains(exact));
        assert!(story.contains("persisted by the SpacetimeDB intervention transaction"));
        assert!(!story.contains("canonical"));
    }

    #[test]
    fn refuses_non_loopback_and_shared_database() {
        let mut config = CoreLoopConfig {
            host: "https://example.com".into(),
            database: "adventuresim-stdb-module".into(),
            seed: 1,
            population: 2,
            cycles: 1,
            duration_days: 1,
            party_size: 2,
            run_nonce: "unit-test-nonce-0001".into(),
            use_imported_world: false,
            expected_world_manifest_digest: None,
            failure_output: None,
        };
        assert!(config.validate().is_err());
        config.host = "http://127.0.0.1:3000".into();
        assert!(config.validate().is_err());
        config.database = "adventuresim-sim-test-1".into();
        assert!(config.validate().is_ok());
        for spoofed in [
            "http://localhost.example.com:3000",
            "http://127.0.0.1@evil.example:3000",
            "http://localhost:3000/path",
            "http://localhost:3000?database=shared",
            "http://user:pass@localhost:3000",
            "https://localhost:3000",
        ] {
            config.host = spoofed.into();
            assert!(config.validate().is_err(), "accepted spoofed URL {spoofed}");
        }
    }

    #[test]
    fn bootstrap_token_is_required_and_bounded() {
        assert!(bootstrap_token_from_environment(None).is_err());
        assert!(bootstrap_token_from_environment(Some("short".into())).is_err());
        assert!(bootstrap_token_from_environment(Some("z".repeat(64))).is_err());
        assert_eq!(
            bootstrap_token_from_environment(Some("a".repeat(64))).unwrap(),
            "a".repeat(64)
        );
    }

    #[test]
    fn dead_or_replaced_leader_is_never_a_policy_actor() {
        assert!(leader_is_actionable("party", 7, 7, true, Some("party")));
        assert!(!leader_is_actionable("party", 7, 7, false, Some("party")));
        assert!(!leader_is_actionable("party", 8, 7, true, Some("party")));
        assert!(!leader_is_actionable("party", 7, 7, true, Some("other")));
    }

    #[test]
    fn medical_rest_schedule_suspends_but_does_not_replace_profile_policy() {
        let profile = generate_profile(42, 0);
        let saved = live_schedule(&profile);
        let rest = medical_rest_schedule();
        assert_eq!(
            [
                rest.combat_training_minutes,
                rest.carousing_minutes,
                rest.apprenticeship_minutes,
                rest.profession_practice_minutes,
                rest.labor_minutes,
                rest.prayer_minutes,
                rest.thievery_minutes,
                rest.raiding_minutes,
            ]
            .into_iter()
            .sum::<u16>(),
            0
        );
        assert_eq!(live_schedule(&profile), saved);
        assert_ne!(saved, rest);
    }

    #[test]
    fn unaffordable_symptomatic_treatment_falls_back_to_natural_rest() {
        assert_eq!(
            choose_medical_action("recovering", true, true, true, 6, Some(5), Some(true), None),
            (MedicalChoice::RestNaturally, "observable_care_unaffordable")
        );
    }

    #[test]
    fn nonsymptomatic_convalescence_does_not_buy_medication() {
        assert_eq!(
            choose_medical_action(
                "recovering",
                false,
                true,
                true,
                100,
                Some(5),
                Some(false),
                Some(false)
            ),
            (
                MedicalChoice::RestNaturally,
                "convalescing_without_symptoms"
            )
        );
    }

    #[test]
    fn affordable_symptomatic_treatment_buys_then_rests() {
        assert_eq!(
            choose_medical_action(
                "recovering",
                true,
                true,
                true,
                7,
                Some(5),
                Some(true),
                Some(true)
            ),
            (MedicalChoice::BuyAndRest, "symptomatic_and_affordable")
        );
    }

    #[test]
    fn equipment_spending_reserves_one_observable_medical_course() {
        assert_eq!(spending_budget_after_medical_reserve(20, Some(7)), 13);
        assert_eq!(spending_budget_after_medical_reserve(5, Some(7)), 0);
        assert_eq!(spending_budget_after_medical_reserve(20, None), 20);
        assert!(equipment_spend_is_still_affordable(20, Some(7), 13));
        assert!(!equipment_spend_is_still_affordable(19, Some(7), 13));
        assert!(!equipment_spend_is_still_affordable(20, Some(8), 13));
    }

    #[test]
    fn medical_rest_venue_accounts_for_visible_inn_cost() {
        assert_eq!(
            affordable_medical_rest_venue(true, false, false, 7, 5),
            Some(true)
        );
        assert_eq!(
            affordable_medical_rest_venue(true, false, false, 6, 5),
            None
        );
        assert_eq!(
            affordable_medical_rest_venue(true, true, true, 5, 5),
            Some(false),
            "the free temple is preferred when both public venues exist"
        );
        assert_eq!(
            affordable_medical_rest_venue(true, true, false, 7, 5),
            Some(true),
            "an inn is required when visible supplies do not cover a temple rest"
        );
    }

    #[test]
    fn settlement_rest_sponsorship_is_public_bounded_and_self_payment_first() {
        let source = include_str!("live_core.rs");
        let selector = source
            .split("fn settlement_rest_sponsor")
            .nth(1)
            .and_then(|tail| tail.split("fn activity_observation").next())
            .expect("settlement rest sponsor selector");
        for public_input in [
            "personal_gold(patient_id)",
            ".party_member()",
            ".character()",
            "current_settlement_id",
            "observable_medical_reserve",
            ".party_inventory_item()",
            ".party_stake()",
        ] {
            assert!(selector.contains(public_input), "missing {public_input}");
        }
        assert!(selector.contains("spendable >= sponsor_quote"));
        assert!(selector.contains("Reverse(option.spendable)"));
        assert!(!selector.contains("infection_episode"));

        let recovery = source
            .split("fn ensure_medically_safe")
            .nth(1)
            .and_then(|tail| tail.split("fn settlement_activity_day").next())
            .expect("medical recovery driver");
        assert!(recovery.contains(".sponsor_party_member_inn_rest_then("));
        assert!(recovery.contains("sponsored_settlement_rest=completed"));
        assert!(recovery.contains("exposure=not_publicly_projected"));
        assert!(recovery.contains("emergency_temple_rest"));
        assert!(recovery.contains("actual_elapsed_minutes={actual_rest_minutes}"));
        assert!(recovery.contains("saturating_sub(rest_started_at)"));
        assert!(recovery.contains("sponsored_settlement_rest_requested_minutes"));
        assert!(recovery.contains("sponsored_settlement_rest_elapsed_minutes"));
        assert!(recovery.contains("MedicalChoice::RestNaturally => natural_rest_venue"));
        assert!(recovery.contains("MedicalChoice::BuyAndRest => medicated_rest_venue"));
        assert!(recovery.contains("selected_rest_venue.map_or(\"unavailable\""));
        assert!(!recovery.contains("infection_episode"));
    }

    #[test]
    fn medical_quote_requires_player_visible_herbalist_stock() {
        assert!(observable_herbalist_stocks_medication(true, true, true));
        assert!(!observable_herbalist_stocks_medication(false, true, true));
        assert!(!observable_herbalist_stocks_medication(true, false, true));
        assert!(!observable_herbalist_stocks_medication(true, true, false));
    }

    #[test]
    fn smithing_decisions_quantize_float_noise_at_one_thousandth() {
        assert_eq!(quantize_smithing_condition(0.020_000_1), 20);
        assert_eq!(quantize_smithing_condition(0.019_999_9), 20);
        assert_eq!(quantize_smithing_condition(f32::NAN), 0);
        assert_eq!(quantize_smithing_condition(f32::INFINITY), 1_000);
    }

    #[test]
    fn report_metrics_expose_encounter_frequency_choices_losses_and_wipes() {
        let metrics = CoreLoopMetrics {
            direct_contracts_attempted: 4,
            direct_contracts_completed: 2,
            generated_case_intakes: 3,
            generated_case_continuations: 1,
            generated_quests_discovered: 3,
            generated_quests_completed: 2,
            generated_quests_closed_externally: 1,
            generated_investigation_actions: 7,
            generated_investigation_waits: 2,
            generated_investigation_wait_minutes: 480,
            generated_investigation_replans: 2,
            generated_witness_dialogues: 4,
            generated_discovery_actions_attempted: 8,
            generated_discovery_actions_fruitful: 3,
            generated_discovery_decisions_unproductive: 2,
            expedition_recovery_plans: 2,
            expedition_recovery_rests: 3,
            expedition_evacuations: 1,
            expedition_resumes: 1,
            expedition_holds: 2,
            expedition_passive_rest_attempts: 2,
            expedition_passive_rest_minutes: 1_500,
            generated_unique_party_cases_discovered: 3,
            generated_exact_site_ready: 2,
            generated_finance_blocked_cycles: 5,
            generated_case_site_traveled: 1,
            journey_provision_purchases: 1,
            journey_provision_party_gold_spent: 115,
            sponsored_settlement_rests: 2,
            sponsored_settlement_rest_gold_spent: 4,
            sponsored_settlement_rest_requested_minutes: 2_880,
            sponsored_settlement_rest_elapsed_minutes: 2_100,
            encounters: 5,
            encounter_sneaks: 1,
            encounter_detours: 1,
            encounter_attacks: 1,
            encounter_runs: 1,
            encounter_surrenders: 1,
            encounter_escape_eligible: 3,
            encounter_escape_ineligible: 2,
            encounter_surrender_items_lost: 4,
            encounter_surrender_value_lost: 90,
            encounter_defeats: 2,
            encounter_wipes: 1,
            ..CoreLoopMetrics::default()
        };
        let value = serde_json::to_value(metrics).unwrap();
        for field in [
            "direct_contracts_attempted",
            "direct_contracts_completed",
            "generated_case_intakes",
            "generated_case_continuations",
            "generated_quests_discovered",
            "generated_quests_completed",
            "generated_quests_closed_externally",
            "generated_investigation_actions",
            "generated_investigation_waits",
            "generated_investigation_wait_minutes",
            "generated_investigation_replans",
            "generated_witness_dialogues",
            "generated_discovery_actions_attempted",
            "generated_discovery_actions_fruitful",
            "generated_discovery_decisions_unproductive",
            "expedition_recovery_plans",
            "expedition_recovery_rests",
            "expedition_evacuations",
            "expedition_resumes",
            "expedition_holds",
            "expedition_passive_rest_attempts",
            "expedition_passive_rest_minutes",
            "generated_unique_party_cases_discovered",
            "generated_exact_site_ready",
            "generated_finance_blocked_cycles",
            "generated_case_site_traveled",
            "journey_provision_purchases",
            "journey_provision_party_gold_spent",
            "sponsored_settlement_rests",
            "sponsored_settlement_rest_gold_spent",
            "sponsored_settlement_rest_requested_minutes",
            "sponsored_settlement_rest_elapsed_minutes",
            "encounters",
            "encounter_sneaks",
            "encounter_detours",
            "encounter_attacks",
            "encounter_runs",
            "encounter_surrenders",
            "encounter_escape_eligible",
            "encounter_escape_ineligible",
            "encounter_surrender_items_lost",
            "encounter_surrender_value_lost",
            "encounter_defeats",
            "encounter_wipes",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
    }
}
