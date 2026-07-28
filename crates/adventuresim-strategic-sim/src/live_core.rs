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
    simulation_run_table::SimulationRunTableAccess, start_dialogue_reducer::start_dialogue,
    store_battle_loot_reducer::store_battle_loot,
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
const CORE_LOOP_FAILURE_SCHEMA_VERSION: u32 = 4;
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
    visible_food_kcal: f32,
    visible_water_ml: f32,
    elapsed_minutes: u64,
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
    if error.contains("Rest until the party reaches its next daylight walking window") {
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
    if error.contains("Rest until the party reaches its next daylight walking window") {
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
    generated_case_owners: HashMap<String, u64>,
    generated_terminal_cases: HashSet<String>,
    generated_exact_site_cases: HashSet<String>,
    generated_traveled_cases: HashSet<String>,
    generated_finance_blocks: HashMap<String, (u64, u64)>,
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
        let (visible_food_kcal, visible_water_ml) = self.visible_rest_supplies(character_id);
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
        _leader: u64,
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
        if !affordable {
            if self.generated_case_owners.contains_key(finance_key) {
                self.metrics.generated_finance_blocked_cycles = self
                    .metrics
                    .generated_finance_blocked_cycles
                    .saturating_add(1);
            }
            let public_funds = party_coin.saturating_add(personal);
            let signature = (upper_bound_cost, public_funds);
            let finance_key = format!("{party_id}:{finance_key}");
            if self.generated_finance_blocks.get(&finance_key) == Some(&signature) {
                return Ok(TravelProvisionDecision::Deferred("journey_finance_backoff"));
            }
            self.generated_finance_blocks.insert(finance_key, signature);
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
        self.generated_finance_blocks
            .remove(&format!("{party_id}:{finance_key}"));
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

    fn travel_camps(&mut self, party_id: &str) -> Result<(), String> {
        for _ in 0..MAX_CAMPS_PER_LEG {
            let party = self.party_by_id(party_id)?;
            if party.camp_destination.is_none() {
                self.metrics.travel_legs += 1;
                return Ok(());
            }
            let remaining_before = party.camp_remaining_minutes;
            let Some((leader, _)) = self.current_leader(party_id) else {
                self.observe_deaths();
                return Ok(());
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
                let choice = encounter.available_choices
                    [(encounter.roll_index as usize) % encounter.available_choices.len()]
                .clone();
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
                    .resolve_strategic_encounter_then(leader, choice.clone(), cb));
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
                    return Ok(());
                }
                continue;
            }
            let journey = self
                .connection
                .db
                .party_journey()
                .iter()
                .find(|row| row.party_id == party_id)
                .ok_or("journey camp projection is incoherent: active journey missing")?;
            let itinerary = self
                .connection
                .db
                .party_journey_itinerary()
                .iter()
                .find(|row| row.party_id == party_id)
                .ok_or("journey camp projection is incoherent: itinerary missing")?;
            let (camp_start, rest_minutes) = projected_camp_rest_minutes(
                journey.completed_elapsed_minutes,
                journey.total_elapsed_minutes,
                &itinerary.forecast_camp_intervals,
            )
            .ok_or("journey camp projection is incoherent: no active forecast interval")?;
            self.event(
                self.current_leader(party_id).map_or(0, |(_, agent)| agent),
                CoreLoopEventKind::Camp,
                format!(
                    "phase=pre_rest;party={};completed_elapsed={};total_elapsed={};camp_start={camp_start};rest_minutes={rest_minutes};remaining_movement={remaining_before}",
                    bounded_event_field(party_id),
                    journey.completed_elapsed_minutes,
                    journey.total_elapsed_minutes,
                ),
            );
            let expected_completed_elapsed = camp_start.saturating_add(rest_minutes);
            let result = reducer_call!(self, "rest_at_camp", |cb| self
                .connection
                .reducers
                .rest_at_camp_then(leader, rest_minutes, cb));
            self.call(result)?;
            self.observe_deaths();
            let Some((leader, agent)) = self.current_leader(party_id) else {
                return Ok(());
            };
            let unsafe_after_rest = self.unsafe_party_agents(&self.party_agents(leader)?);
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
                || !unsafe_after_rest.is_empty()
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
            let result = reducer_call!(self, "continue_camp_travel", |cb| self
                .connection
                .reducers
                .continue_camp_travel_then(leader, cb));
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

    fn personal_gold(&self, character_id: u64) -> u64 {
        self.connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum()
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
            let natural_rest_venue = affordable_medical_rest_venue(
                inn_available,
                temple_available,
                temple_food_covers_day,
                purse,
                0,
            );
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
            self.event(
                agent,
                CoreLoopEventKind::MedicalDecision,
                format!(
                    "status={};symptomatic={symptomatic};settlement={};purse={purse};observable_quote={};rest_cost={};care_total={};rest_venue={};temple_food_kcal={visible_food_kcal:.0};temple_water_ml={visible_water_ml:.0};temple_food_covers_day={temple_food_covers_day};care_affordable={};action={choice:?};reason={reason}",
                    condition.status,
                    settlement.as_deref().unwrap_or("none"),
                    observable_quote.map_or_else(|| "unavailable".into(), |quote| quote.to_string()),
                    required_rest_cost.map_or_else(|| "unavailable".into(), |cost| cost.to_string()),
                    observable_care_total.map_or_else(|| "unavailable".into(), |cost| cost.to_string()),
                    medicated_rest_venue.map_or("unavailable", |at_inn| if at_inn { "inn" } else { "temple" }),
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
                let result = reducer_call!(self, "natural_illness_recovery_rest", |cb| self
                    .connection
                    .reducers
                    .rest_at_settlement_hours_then(character_id, 1_440, at_inn, cb));
                self.call(result)?;
                self.metrics.treatment_rest_minutes += 1_440;
                self.metrics.recovery_rests += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::Recover,
                    format!("natural_recovery_minutes=1440;reason={reason}"),
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
                    .is_some_and(|row| row.status == "ready");
                !alive || !ready
            })
            .collect::<Vec<_>>();
        unsafe_agents.sort_unstable();
        unsafe_agents
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

    fn observe_generated_case_transition(
        &mut self,
        agent: u32,
        character_id: u64,
        case_id: &str,
        title: &str,
        immediately_after_own_action: bool,
    ) {
        if self.generated_terminal_cases.contains(case_id) {
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
                self.generated_terminal_cases.insert(case_id.to_owned());
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
                self.generated_terminal_cases.insert(case_id.to_owned());
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
            .generated_case_owners
            .iter()
            .map(|(case_id, owner)| (case_id.clone(), *owner))
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
            self.generated_case_owners
                .insert(case_id.clone(), character_id);
            self.metrics.generated_quests_discovered += 1;
            self.metrics.generated_unique_party_cases_discovered += 1;
            self.metrics.quests_attempted += 1;
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
    ) -> Result<(), String> {
        let Some(occupied_site_id) = self
            .party_by_id(party_id)?
            .current_case_site_id
            .map(|site| site.value)
        else {
            return Ok(());
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
            return Ok(());
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
            format!("generated_case={case_id};completed_return={settlement_id}"),
        );
        self.travel_camps(party_id)?;
        self.observe_deaths();
        Ok(())
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
        for _ in 0..MAX_GENERATED_CASE_STEPS_PER_CYCLE {
            if self.generated_case_status(character_id, case_id).as_deref() != Some("open") {
                self.return_completed_generated_party_to_origin(party_id, character_id, case_id)?;
                return Ok(true);
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
                    self.return_completed_generated_party_to_origin(
                        party_id,
                        character_id,
                        case_id,
                    )?;
                    return Ok(true);
                }
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            if let Some(action) = actions.iter().find(|row| row.can_travel_to_required_site) {
                let funnel_key = format!("{character_id}:{case_id}");
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
                if self.generated_traveled_cases.insert(funnel_key) {
                    self.metrics.generated_case_site_traveled += 1;
                }
                self.event(
                    agent,
                    CoreLoopEventKind::Travel,
                    format!("generated_case={case_id};outbound={site_id}"),
                );
                self.travel_camps(party_id)?;
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
                        self.travel_camps(party_id)?;
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
                        self.return_completed_generated_party_to_origin(
                            party_id,
                            character_id,
                            case_id,
                        )?;
                        return Ok(true);
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
                self.travel_camps(party_id)?;
                if !self.generated_actor_ready_after_time(party_id, character_id, case_id)? {
                    return Ok(false);
                }
                continue;
            }
            return Ok(false);
        }
        Ok(false)
    }

    fn cycle(&mut self, party_id: &str, cycle: u32) -> Result<(), String> {
        let Some((mut leader, authoritative_agent)) = self.current_leader(party_id) else {
            self.observe_deaths();
            return Ok(());
        };
        let mut leader_agent = authoritative_agent;
        let party_agents = self.party_agents(leader)?;
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
        let party = self.party_for(leader)?;
        let quest = self
            .choose_quest(&party, &self.profiles[leader_agent as usize])
            .ok_or("no suitable available quest")?;
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
        let case_site = self
            .connection
            .db
            .backend_case_site_pins()
            .iter()
            .filter(|site| site.owner_character_id == leader && site.case_id == quest.case_id)
            .min_by_key(|site| (site.distance_m, site.case_site_id.clone()))
            .ok_or("accepted quest did not disclose an exact case site")?;
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
        self.event(
            leader_agent,
            CoreLoopEventKind::Travel,
            format!("outbound={}", case_site.case_site_id),
        );
        self.travel_camps(party_id)?;

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
            self.travel_camps(party_id)?;
            for agent in party_agents {
                self.ensure_medically_safe(agent)?;
            }
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
            self.travel_camps(party_id)?;
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            for agent in self.party_agents(leader)? {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
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
            self.travel_camps(party_id)?;
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
            self.travel_camps(party_id)?;
            self.observe_deaths();
            let Some((current, _)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
            for agent in self.party_agents(leader)? {
                self.ensure_medically_safe(agent)?;
            }
            let Some((current, current_agent)) = self.current_leader(party_id) else {
                return Ok(());
            };
            leader = current;
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
        self.travel_camps(party_id)?;
        self.observe_deaths();
        let Some((current, current_agent)) = self.current_leader(party_id) else {
            return Ok(());
        };
        leader = current;
        leader_agent = current_agent;
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
        self.event(leader_agent, CoreLoopEventKind::TurnIn, quest.id.clone());

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
        if let Some((_, current_agent)) = self.current_leader(party_id) {
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
        generated_case_owners: HashMap::new(),
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
        for party_id in &party_ids {
            runner.observe_deaths();
            runner.observe_external_generated_closures();
            let Some((leader, leader_agent)) = runner.current_leader(party_id) else {
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
            if elapsed >= duration_minutes {
                continue;
            }
            active = true;
            let profile = &runner.profiles[leader_agent as usize];
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
            let direct_quest_chosen = wants_quest && runner.choose_quest(&party, profile).is_some();
            let quest_path = if !open_generated_cases.is_empty() {
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
                    runner
                        .generated_case_owners
                        .entry(case_id.clone())
                        .or_insert(leader);
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
                "direct_contract" => runner.cycle(party_id, cycle)?,
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
        if !active {
            break;
        }
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
        assert!(travel.contains(".party_journey()"));
        assert!(travel.contains(".party_journey_itinerary()"));
        assert!(travel.contains("row.party_id == party_id"));
        assert!(travel.contains("projected_camp_rest_minutes("));
        assert!(travel.contains(".rest_at_camp_then(leader, rest_minutes"));
        assert!(!travel.contains(".rest_at_camp_then(leader, 1_440"));
        for phase in ["phase=pre_rest", "phase=post_rest", "phase=post_continue"] {
            assert!(travel.contains(phase));
        }

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
        assert!(!provisioning.contains(".map_or(0, |row| row.buy_bps)"));
        assert!(!provisioning.contains("party_journey_route"));
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
            visible_food_kcal: 2_000.0,
            visible_water_ml: 4_000.0,
            elapsed_minutes: 1_440,
        };
        let after = ActivityObservation {
            personal_gold_coin: 9,
            condition_status: "recovering".into(),
            hunger: 0.5,
            thirst: 0.125,
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
    fn failure_artifact_version_four_serializes_safe_operation_context() {
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
        assert_eq!(value["schema_version"], serde_json::json!(4));
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
            generated_unique_party_cases_discovered: 3,
            generated_exact_site_ready: 2,
            generated_finance_blocked_cycles: 5,
            generated_case_site_traveled: 1,
            journey_provision_purchases: 1,
            journey_provision_party_gold_spent: 115,
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
            "generated_unique_party_cases_discovered",
            "generated_exact_site_ready",
            "generated_finance_blocked_cycles",
            "generated_case_site_traveled",
            "journey_provision_purchases",
            "journey_provision_party_gold_spent",
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
