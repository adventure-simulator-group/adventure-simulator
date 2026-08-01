use crate::{ActivityPreference, AgentProfile, BuildRole, EquipmentStyle, generate_profile};
use adventuresim_core::simulation_security::{
    SIM_BOOTSTRAP_TOKEN_ENV as BOOTSTRAP_TOKEN_ENV,
    SIM_BOOTSTRAP_TOKEN_HEX_LEN as BOOTSTRAP_TOKEN_HEX_LEN,
};
use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table};
use adventuresim_stdb_client::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
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
    backend_character_attributes_table::BackendCharacterAttributesTableAccess,
    backend_character_capabilities_table::BackendCharacterCapabilitiesTableAccess,
    backend_character_conditions_table::BackendCharacterConditionsTableAccess,
    backend_character_deaths_table::BackendCharacterDeathsTableAccess,
    backend_character_limbs_table::BackendCharacterLimbsTableAccess,
    backend_character_needs_table::BackendCharacterNeedsTableAccess,
    backend_character_strategic_conditions_table::BackendCharacterStrategicConditionsTableAccess,
    backend_character_times_table::BackendCharacterTimesTableAccess,
    backend_character_training_schedules_table::BackendCharacterTrainingSchedulesTableAccess,
    backend_characters_table::BackendCharactersTableAccess, backend_contract_type::BackendContract,
    backend_contracts_table::BackendContractsTableAccess,
    backend_dialogue_sessions_table::BackendDialogueSessionsTableAccess,
    backend_dialogue_topic_options_table::BackendDialogueTopicOptionsTableAccess,
    backend_investigation_action_outcomes_table::BackendInvestigationActionOutcomesTableAccess,
    backend_investigation_actions_table::BackendInvestigationActionsTableAccess,
    backend_investigation_cases_table::BackendInvestigationCasesTableAccess,
    backend_investigation_leads_table::BackendInvestigationLeadsTableAccess,
    backend_local_problem_trade_effects_table::BackendLocalProblemTradeEffectsTableAccess,
    backend_physiology_charts_table::BackendPhysiologyChartsTableAccess,
    backend_settlement_residents_table::BackendSettlementResidentsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_equipped_item_table::CharacterEquippedItemTableAccess,
    character_illness_status_table::CharacterIllnessStatusTableAccess,
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
    purchase_personal_storefront_with_party_stake_reducer::purchase_personal_storefront_with_party_stake,
    register_strategic_gateway_reducer::register_strategic_gateway,
    repair_order_table::RepairOrderTableAccess, report_contract_reducer::report_contract,
    request_general_party_join_reducer::request_general_party_join,
    resolve_strategic_encounter_reducer::resolve_strategic_encounter,
    rest_at_camp_reducer::rest_at_camp, rest_at_settlement_hours_reducer::rest_at_settlement_hours,
    retrieve_repaired_item_reducer::retrieve_repaired_item,
    seed_simulation_disease_reducer::seed_simulation_disease,
    seed_simulation_equipment_damage_reducer::seed_simulation_equipment_damage,
    seed_simulation_quest_fixture_reducer::seed_simulation_quest_fixture,
    seed_simulation_world_reducer::seed_simulation_world,
    settlement_resident_presence_table::SettlementResidentPresenceTableAccess,
    settlement_service_type::SettlementService, settlement_smith_table::SettlementSmithTableAccess,
    settlement_table::SettlementTableAccess,
    simulate_contract_issuer_interaction_reducer::simulate_contract_issuer_interaction,
    simulation_quest_fixture_table::SimulationQuestFixtureTableAccess,
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
const PARTY_TENT_ITEM_ID: &str = "field_tent";
const RANGED_AMMUNITION_ITEM_ID: &str = "arrow";
/// One ordinary autoresolve can consume several arrows. Twenty leaves a
/// conservative reserve for an encounter plus the disclosed quest fight.
const RANGED_AMMUNITION_FLOOR: u32 = 20;
/// Keep a material movement margin rather than departing at the point where
/// the authoritative linear encumbrance rule reaches zero.
const MIN_DEPARTURE_ENCUMBRANCE_REMAINING_BPS: u32 = 2_000;
const MAX_DEPARTURE_WETNESS_BPS: u16 = 8_000;
const MAX_DEPARTURE_ABS_THERMAL_STRAIN: u32 = 2_500;
const MIN_ACTIONABLE_PHYSIOLOGY_CONFIDENCE_BPS: u16 = 3_000;
/// Older observations can describe a materially different disease stage.
/// One strategic day permits ordinary asynchronous party observation without
/// allowing an indefinitely cached chart to direct treatment.
const MAX_ACTIONABLE_PHYSIOLOGY_CHART_AGE_MINUTES: u64 = 1_440;
const DEFAULT_SIMULATION_DISEASE: &str = "influenza";
const SIMULATION_DISEASE_SCENARIOS: [&str; 9] = [
    "influenza",
    "dysentery",
    "tetanus",
    "erysipelas",
    "consumption",
    "mahrdruck",
    "shroud_fever",
    "bilwisschuss",
    "kobeldunst",
];

fn default_simulation_disease() -> String {
    DEFAULT_SIMULATION_DISEASE.to_owned()
}

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
    /// Validated disease identity used only by the disposable fixture.
    #[serde(default = "default_simulation_disease")]
    pub fixture_disease: String,
    /// Install and require the deterministic two-party quest acceptance fixture.
    #[serde(default)]
    pub require_quest_coverage: bool,
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
        if !SIMULATION_DISEASE_SCENARIOS.contains(&self.fixture_disease.as_str()) {
            return Err(format!(
                "fixture_disease must be one of {}",
                SIMULATION_DISEASE_SCENARIOS.join(", ")
            ));
        }
        if self.use_imported_world {
            if self.require_quest_coverage {
                return Err("quest coverage fixture cannot use an imported world".into());
            }
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
        if self.require_quest_coverage && self.population <= self.party_size {
            return Err("quest coverage fixture requires at least two parties".into());
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
    pub direct_contracts_safely_abandoned: u32,
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
    pub party_tents_purchased: u32,
    pub party_tent_gold_spent: u64,
    pub tent_provider_unavailable_bivouac_departures: u32,
    pub tent_field_rests: u32,
    pub tent_field_rest_failures: u32,
    pub bivouac_field_rests: u32,
    pub ammunition_purchases: u32,
    pub ammunition_units_purchased: u32,
    pub ammunition_gold_spent: u64,
    pub ammunition_shortage_suppressions: u32,
    pub load_readiness_suppressions: u32,
    pub current_condition_readiness_suppressions: u32,
    pub route_weather_projection_unavailable_departures: u32,
    pub survival_observations: u32,
    pub max_party_carried_load_grams: u64,
    pub max_party_carry_capacity_grams: u64,
    pub min_party_encumbrance_remaining_bps: u32,
    pub max_observed_wetness_bps: u16,
    pub max_observed_abs_thermal_strain: u32,
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
    pub thermal: f32,
    pub wetness_bps: u16,
    pub thermal_strain: i32,
    pub ammunition: u32,
    pub carried_load_kg: f32,
    pub carry_capacity_kg: f32,
    pub encumbrance_remaining_bps: u32,
    pub equipment_ready: bool,
    pub party_tent_quantity: u32,
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
    pub format_version: u32,
    pub backend_kind: String,
    pub seed: u64,
    pub server_origin: String,
    pub database: String,
    pub run_nonce: String,
    pub fixture_disease: String,
    pub deployment_identity_note: String,
    pub world_artifact_id: Option<String>,
    pub world_manifest_digest: Option<String>,
    pub starting_settlement_id: String,
    pub profiles: Vec<AgentProfile>,
    pub metrics: CoreLoopMetrics,
    pub quest_coverage: Option<QuestCoverageEvidence>,
    pub trace: Vec<CoreLoopEvent>,
    pub trace_truncated: bool,
    pub total_event_count: u64,
    pub final_agents: Vec<FinalAgentState>,
    pub elapsed_game_minutes: u64,
    pub policy_seed_note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestCoverageEvidence {
    pub direct_contract_id: String,
    pub generated_case_id: String,
    pub direct_leader_id: u64,
    pub generated_leader_id: u64,
    pub direct_party_id: String,
    pub generated_party_id: String,
    pub direct_accepted: bool,
    pub direct_traveled: bool,
    pub direct_encountered: bool,
    pub direct_reported: bool,
    pub direct_safely_abandoned: bool,
    pub generated_intake: bool,
    pub generated_discovered: bool,
    pub generated_completed: bool,
}

/// Strict acceptance contract for the deterministic two-party quest fixture.
/// Each error names the first unmet metric so CI output is actionable.
pub fn validate_quest_coverage(report: &CoreLoopReport) -> Result<(), String> {
    let metrics = &report.metrics;
    let coverage = report
        .quest_coverage
        .as_ref()
        .ok_or("quest coverage acceptance failed: metric=fixture_provenance")?;
    let checks = [
        ("reducer_failures", metrics.reducer_failures == 0),
        (
            "duplicate_semantic_events",
            metrics.duplicate_semantic_events == 0,
        ),
        ("stuck_detections", metrics.stuck_detections == 0),
        ("encounter_wipes", metrics.encounter_wipes == 0),
        ("fixture_direct_accepted", coverage.direct_accepted),
        ("fixture_direct_traveled", coverage.direct_traveled),
        ("fixture_direct_encountered", coverage.direct_encountered),
        ("fixture_direct_reported", coverage.direct_reported),
        ("fixture_generated_intake", coverage.generated_intake),
        (
            "fixture_generated_discovered",
            coverage.generated_discovered,
        ),
        ("fixture_generated_completed", coverage.generated_completed),
        (
            "fixture_successful_completion",
            coverage.direct_reported || coverage.generated_completed,
        ),
        ("quests_attempted", metrics.quests_attempted >= 2),
        (
            "quests_attempted_consistency",
            metrics.quests_attempted
                == metrics
                    .direct_contracts_attempted
                    .saturating_add(metrics.generated_case_intakes),
        ),
    ];
    if let Some((metric, _)) = checks.into_iter().find(|(_, passed)| !passed) {
        return Err(format!("quest coverage acceptance failed: metric={metric}"));
    }
    if report.final_agents.iter().any(|agent| !agent.alive) {
        return Err("quest coverage acceptance failed: metric=final_agents_alive".into());
    }
    if report.final_agents.iter().any(|agent| agent.critical) {
        return Err("quest coverage acceptance failed: metric=final_agents_not_critical".into());
    }
    if report.final_agents.iter().any(|agent| {
        agent.settlement_id.is_none()
            || agent.current_case_site_id.is_some()
            || agent.journey_destination.is_some()
    }) {
        return Err("quest coverage acceptance failed: metric=final_agents_not_stranded".into());
    }
    Ok(())
}

/// Persist the same public-safe diagnostic shape used by reducer failures
/// when the completed report fails the stricter quest-coverage contract.
pub fn write_quest_coverage_failure(
    report: &CoreLoopReport,
    path: &Path,
    error: &str,
) -> Result<(), String> {
    let reason_code = error
        .strip_prefix("quest coverage acceptance failed: metric=")
        .unwrap_or("unknown_metric");
    let (trace, trace_truncated) = bounded_failure_trace(&report.trace, report.total_event_count);
    let final_agents = report
        .final_agents
        .iter()
        .map(|agent| CoreLoopFailureAgent {
            agent_id: agent.agent_id,
            character_id: agent.character_id,
            alive: agent.alive,
            condition_status: agent.condition_status.clone(),
            thermal: agent.thermal,
            wetness_bps: agent.wetness_bps,
            thermal_strain: agent.thermal_strain,
            ammunition: agent.ammunition,
            carried_load_kg: agent.carried_load_kg,
            carry_capacity_kg: agent.carry_capacity_kg,
            encumbrance_remaining_bps: agent.encumbrance_remaining_bps,
            equipment_ready: agent.equipment_ready,
            party_tent_quantity: agent.party_tent_quantity,
            hunger: agent.hunger,
            thirst: agent.thirst,
            food_days: agent.food_days,
            water_days: agent.water_days,
            visible_food_kcal: agent.visible_food_kcal,
            visible_water_ml: agent.visible_water_ml,
            personal_gold_coin: agent.personal_gold_coin,
            settlement_id: agent.settlement_id.clone(),
            current_case_site_id: agent.current_case_site_id.clone(),
            journey_destination: agent.journey_destination.clone(),
            symptomatic: agent.symptomatic,
            critical: agent.critical,
            settlement_services: agent.settlement_services.clone(),
            visible_herbalist_quote: agent.visible_herbalist_quote,
            visible_inn_full_board_cost: agent.visible_inn_full_board_cost,
        })
        .collect();
    let artifact = CoreLoopFailureArtifact {
        schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
        category: "quest_coverage_acceptance".into(),
        message: error.into(),
        operation: None,
        reason_code: reason_code.into(),
        fixture_disease: report.fixture_disease.clone(),
        metrics: report.metrics.clone(),
        quest_coverage: report.quest_coverage.clone(),
        total_event_count: report.total_event_count,
        trace_truncated,
        trace,
        final_agents,
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
        .map_err(|error| format!("could not write quest coverage diagnostic: {error}"))
}

const MAX_FAILURE_TRACE_EVENTS: usize = 64;
const CORE_LOOP_FAILURE_SCHEMA_VERSION: u32 = 9;
const MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES: u32 = 1_440;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopFailureAgent {
    pub agent_id: u32,
    pub character_id: u64,
    pub alive: bool,
    pub condition_status: String,
    pub thermal: f32,
    pub wetness_bps: u16,
    pub thermal_strain: i32,
    pub ammunition: u32,
    pub carried_load_kg: f32,
    pub carry_capacity_kg: f32,
    pub encumbrance_remaining_bps: u32,
    pub equipment_ready: bool,
    pub party_tent_quantity: u32,
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
    pub fixture_disease: String,
    pub metrics: CoreLoopMetrics,
    pub quest_coverage: Option<QuestCoverageEvidence>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PublicSurvivalObservation {
    thermal: f32,
    wetness_bps: u16,
    thermal_strain: i32,
    ammunition: u32,
    carried_load_kg: f32,
    carry_capacity_kg: f32,
    encumbrance_remaining_bps: u32,
    equipment_ready: bool,
    party_tent_quantity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DepartureReadiness {
    Ready,
    Deferred(&'static str),
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
    thermal: f32,
    wetness_bps: u16,
    thermal_strain: i32,
    ammunition: u32,
    carried_load_kg: f32,
    carry_capacity_kg: f32,
    encumbrance_remaining_bps: u32,
    equipment_ready: bool,
    party_tent_quantity: u32,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct EncounterPolicyChoice {
    choice: String,
    reason: &'static str,
}

fn select_expedition_encounter_choice(
    available_choices: &[String],
    evacuation: bool,
) -> Option<EncounterPolicyChoice> {
    let has = |candidate: &str| available_choices.iter().any(|choice| choice == candidate);
    if has("detour") {
        return Some(EncounterPolicyChoice {
            choice: "detour".into(),
            reason: "guaranteed_party_aware_detour",
        });
    }
    if has("run") {
        return Some(EncounterPolicyChoice {
            choice: "run".into(),
            reason: "public_speed_check_allows_escape",
        });
    }
    if has("surrender") {
        return Some(EncounterPolicyChoice {
            choice: "surrender".into(),
            reason: "bandit_surrender_is_only_protective_choice",
        });
    }
    (!evacuation && has("attack")).then(|| EncounterPolicyChoice {
        choice: "attack".into(),
        reason: "no_protective_response_available",
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PublicCombatFingerprint {
    members: Vec<(u64, bool, bool, bool, bool, u32, u32, u32, u64)>,
}

#[derive(Clone, Debug)]
struct PublicPartyCombatant {
    capability: CharacterCapability,
    ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicContractAssessment {
    eligible: bool,
    reason: &'static str,
    enemy_count: Option<u32>,
    ready_combatants: u32,
    party_power_milli: u64,
    enemy_power_milli: u64,
}

fn public_opposition_count(wording: &str) -> Option<u32> {
    let normalized = wording
        .trim()
        .trim_end_matches(|character| character == '.' || character == ',')
        .to_ascii_lowercase();
    let (estimate, value) = normalized
        .strip_prefix("perhaps ")
        .map_or((false, normalized.as_str()), |value| (true, value));
    let count = match value {
        "one" | "a lone" | "a single" => 1,
        "two" | "a pair" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        value => value.parse::<u32>().ok()?,
    };
    (count > 0).then_some(if estimate {
        count.saturating_add(1)
    } else {
        count
    })
}

fn public_contract_assessment(
    difficulty: i32,
    opposition_count_wording: &str,
    opposition_combat_power: u64,
    members: &[PublicPartyCombatant],
) -> PublicContractAssessment {
    let Some(enemy_count) = public_opposition_count(opposition_count_wording) else {
        return PublicContractAssessment {
            eligible: false,
            reason: "unknown_public_opposition_count",
            enemy_count: None,
            ready_combatants: 0,
            party_power_milli: 0,
            enemy_power_milli: 0,
        };
    };
    if difficulty <= 0 {
        return PublicContractAssessment {
            eligible: false,
            reason: "invalid_public_difficulty",
            enemy_count: Some(enemy_count),
            ready_combatants: 0,
            party_power_milli: 0,
            enemy_power_milli: 0,
        };
    }
    if opposition_combat_power == 0 {
        return PublicContractAssessment {
            eligible: false,
            reason: "missing_authoritative_opposition_power",
            enemy_count: Some(enemy_count),
            ready_combatants: 0,
            party_power_milli: 0,
            enemy_power_milli: 0,
        };
    }
    let ready = members
        .iter()
        .filter(|member| member.ready && (member.capability.melee || member.capability.ranged))
        .collect::<Vec<_>>();
    let Some(party_power_milli) = ready.iter().try_fold(0u64, |total, member| {
        total.checked_add(member.capability.autoresolve_combat_power)
    }) else {
        return PublicContractAssessment {
            eligible: false,
            reason: "public_party_power_overflow",
            enemy_count: Some(enemy_count),
            ready_combatants: ready.len().min(u32::MAX as usize) as u32,
            party_power_milli: 0,
            enemy_power_milli: opposition_combat_power,
        };
    };
    let enemy_power_milli = opposition_combat_power;
    let margin = adventuresim_core::autoresolve::combat_power_meets_safety_margin(
        party_power_milli,
        enemy_power_milli,
    );
    let eligible = !ready.is_empty() && margin == Some(true);
    PublicContractAssessment {
        eligible,
        reason: if ready.is_empty() {
            "no_ready_public_combatants"
        } else if party_power_milli == 0 {
            "missing_authoritative_party_power"
        } else if margin.is_none() {
            "public_combat_margin_overflow"
        } else if eligible {
            "public_matchup_with_safety_margin"
        } else {
            "public_matchup_below_safety_margin"
        },
        enemy_count: Some(enemy_count),
        ready_combatants: ready.len() as u32,
        party_power_milli,
        enemy_power_milli,
    }
}

fn public_combat_fingerprint(
    mut capabilities: Vec<CharacterCapability>,
) -> PublicCombatFingerprint {
    capabilities.sort_by_key(|row| row.character_id);
    PublicCombatFingerprint {
        members: capabilities
            .into_iter()
            .map(|row| {
                (
                    row.character_id,
                    row.melee,
                    row.ranged,
                    row.heavy || row.half_armor,
                    row.precise,
                    (row.endurance.max(0.0) * 100.0).round() as u32,
                    (row.athletics.max(0.0) * 100.0).round() as u32,
                    (row.weapon_precision.max(0.0) * 100.0).round() as u32,
                    row.autoresolve_combat_power,
                )
            })
            .collect(),
    }
}

fn generated_method_skill_fit(profile: &AgentProfile, method: &str) -> u32 {
    let skills = &profile.initial_skills;
    let hours = match method {
        "inspect_site" | "search_area" | "locate_contact" | "watch" | "patrol"
        | "approach_lead" => skills.insight,
        // The public action projection does not expose target terrain.
        "follow_tracks" | "reacquire_tracks" => 0.0,
        "lay_ambush" => (skills.insight + skills.stealth) / 2.0,
        _ => 0.0,
    };
    hours.max(0.0).min(100_000.0).round() as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedDefeatDecision {
    Proceed,
    SuppressUnchanged,
}

fn generated_defeat_decision(
    combat_available: bool,
    previous: Option<&PublicCombatFingerprint>,
    current: &PublicCombatFingerprint,
) -> GeneratedDefeatDecision {
    if combat_available && previous.is_some_and(|previous| previous == current) {
        GeneratedDefeatDecision::SuppressUnchanged
    } else {
        GeneratedDefeatDecision::Proceed
    }
}

fn generated_action_score(
    profile: &AgentProfile,
    action: &BackendInvestigationAction,
) -> (u8, u32, u16, u32, u32) {
    let progress = if action.available {
        3
    } else if action.can_travel_to_required_site {
        2
    } else if projected_investigation_wait_minutes(
        &action.unavailable_reason_code,
        action.wait_minutes,
    )
    .is_some()
    {
        1
    } else {
        0
    };
    (
        progress,
        generated_method_skill_fit(profile, &action.method),
        10_000_u16.saturating_sub(action.uncertainty_bps),
        u32::MAX.saturating_sub(action.duration_max_minutes),
        u32::MAX.saturating_sub(action.wait_minutes),
    )
}

fn sort_generated_actions(profile: &AgentProfile, actions: &mut [BackendInvestigationAction]) {
    actions.sort_by(|left, right| {
        generated_action_score(profile, right)
            .cmp(&generated_action_score(profile, left))
            .then_with(|| left.action_id.cmp(&right.action_id))
    });
}

fn role_rank(role: BuildRole) -> u8 {
    match role {
        BuildRole::FrontLine => 0,
        BuildRole::Skirmisher => 1,
        BuildRole::Ranged => 2,
        BuildRole::Healer => 3,
        BuildRole::Devout => 4,
        BuildRole::Civilian => 5,
    }
}

pub(crate) fn balanced_party_groups(
    profiles: &[AgentProfile],
    party_size: usize,
) -> Vec<Vec<usize>> {
    let group_count = profiles.len().div_ceil(party_size);
    if group_count == 0 {
        return Vec::new();
    }
    let mut targets = vec![profiles.len() / group_count; group_count];
    for target in targets.iter_mut().take(profiles.len() % group_count) {
        *target += 1;
    }
    let mut order = (0..profiles.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| {
        (
            role_rank(profiles[index].build.role),
            profiles[index].agent_id,
        )
    });
    let mut groups = vec![Vec::new(); group_count];
    let mut cursor = 0;
    for index in order {
        let group = (0..group_count)
            .map(|offset| (cursor + offset) % group_count)
            .find(|&group| groups[group].len() < targets[group])
            .expect("party target capacity covers every profile");
        groups[group].push(index);
        cursor = (group + 1) % group_count;
    }
    for group in &mut groups {
        group.sort_by_key(|&index| {
            (
                profiles[index].build.activity_only,
                profiles[index].agent_id,
            )
        });
    }
    groups
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
    safe_offered_contracts: usize,
    open_generated_cases: usize,
    projected_investigation_actions: usize,
    quest_path: &str,
    quest_intended: bool,
    quest_selected: bool,
    selection_reason: &str,
) -> String {
    format!(
        "cycle={cycle};wants_quest={wants_quest};selector={selector:.6};quest_propensity={quest_propensity:.6};settlement={};offered_contracts={offered_contracts};safe_offered_contracts={safe_offered_contracts};open_generated_cases={open_generated_cases};projected_investigation_actions={projected_investigation_actions};quest_path={quest_path};quest_intended={quest_intended};quest_selected={quest_selected};selection_reason={}",
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

fn public_settlement_economy_profile(
    profile: &SettlementEconomyProfile,
) -> Option<adventuresim_world_schema::SettlementEconomyProfile> {
    use adventuresim_world_schema as world;

    // NPC tab visibility depends only on the canonical service set. Build the
    // smallest shared profile that preserves those inputs instead of trying to
    // serde-bridge SpacetimeDB's SATS-only generated client types.
    let mut navigability_profile = world::SettlementEconomyProfile::stage_placeholder();
    navigability_profile.rules_version = profile.rules_version;
    navigability_profile.prosperity_score = profile.prosperity_score;
    navigability_profile.services = profile
        .services
        .iter()
        .map(|service| match service {
            SettlementService::GeneralStore => world::SettlementService::GeneralStore,
            SettlementService::Inn => world::SettlementService::Inn,
            SettlementService::GeneralBlacksmith => world::SettlementService::GeneralBlacksmith,
            SettlementService::Market => world::SettlementService::Market,
            SettlementService::Weaponsmith => world::SettlementService::Weaponsmith,
            SettlementService::Armorer => world::SettlementService::Armorer,
            SettlementService::Tailor => world::SettlementService::Tailor,
            SettlementService::Herbalist => world::SettlementService::Herbalist,
            SettlementService::Temple => world::SettlementService::Temple,
            SettlementService::Bookstore => world::SettlementService::Bookstore,
        })
        .collect();
    navigability_profile.specializations = profile
        .specializations
        .iter()
        .copied()
        .map(public_stock_category)
        .collect();
    navigability_profile.stock = profile
        .stock
        .iter()
        .map(|stock| adventuresim_world_schema::SettlementStock {
            category: public_stock_category(stock.category),
            abundance: stock.abundance,
            provenance: adventuresim_world_schema::ProfileFactProvenance::DeterministicGapFill,
        })
        .collect();

    navigability_profile
        .validate()
        .ok()
        .map(|()| navigability_profile)
}

fn public_stock_category(category: StockCategory) -> adventuresim_world_schema::StockCategory {
    use adventuresim_world_schema::StockCategory as World;
    match category {
        StockCategory::Grain => World::Grain,
        StockCategory::Dairy => World::Dairy,
        StockCategory::Meat => World::Meat,
        StockCategory::Fish => World::Fish,
        StockCategory::Cloth => World::Cloth,
        StockCategory::Hides => World::Hides,
        StockCategory::Timber => World::Timber,
        StockCategory::Fuel => World::Fuel,
        StockCategory::Stone => World::Stone,
        StockCategory::Pottery => World::Pottery,
        StockCategory::Salt => World::Salt,
        StockCategory::Metalwares => World::Metalwares,
        StockCategory::Weapons => World::Weapons,
        StockCategory::Armor => World::Armor,
        StockCategory::Herbs => World::Herbs,
        StockCategory::GeneralGoods => World::GeneralGoods,
        StockCategory::Books => World::Books,
    }
}

fn public_economy_catalog_kind(
    kind: ItemKind,
) -> adventuresim_core::settlement_economy::CatalogKind {
    use adventuresim_core::settlement_economy::CatalogKind as Catalog;
    match kind {
        ItemKind::Simple | ItemKind::Container => Catalog::Simple,
        ItemKind::Weapon => Catalog::Weapon,
        ItemKind::Armor => Catalog::Armor,
        ItemKind::Shield => Catalog::Shield,
        ItemKind::Clothing => Catalog::Clothing,
        ItemKind::Currency => Catalog::Currency,
        ItemKind::Ingredient => Catalog::Ingredient,
        ItemKind::Medication => Catalog::Medication,
        ItemKind::Food => Catalog::Food,
    }
}

fn public_storefront_available(
    profile: &SettlementEconomyProfile,
    storefront: adventuresim_core::settlement_economy::Storefront,
) -> bool {
    public_settlement_economy_profile(profile).is_some_and(|profile| {
        adventuresim_core::settlement_economy::storefront_available(&profile, storefront)
    })
}

fn public_storefront_stocks(
    profile: &SettlementEconomyProfile,
    storefront: adventuresim_core::settlement_economy::Storefront,
    item: &Item,
) -> bool {
    public_settlement_economy_profile(profile).is_some_and(|profile| {
        adventuresim_core::settlement_economy::storefront_stocks(
            &profile,
            storefront,
            &item.id,
            public_economy_catalog_kind(item.kind),
        )
    })
}

fn storefront_offer_unchanged(
    selected: &(String, u64, u64),
    current: Option<(String, u64, u64)>,
) -> bool {
    current.as_ref() == Some(selected)
}

fn visible_unique_default_provider(providers: &[(u64, u16, u16)], minute: u64) -> Option<u64> {
    let [(provider, start_minute, end_minute)] = providers else {
        return None;
    };
    npc_is_publicly_present(*start_minute, *end_minute, minute).then_some(*provider)
}

fn retain_navigable_public_npc_candidates(
    candidates: Vec<PublicNpcCandidate>,
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    has_keep: bool,
    settlement_id: &str,
) -> Vec<PublicNpcCandidate> {
    candidates
        .into_iter()
        .filter(|candidate| {
            adventuresim_core::settlement_economy::npc_location_is_navigable(
                profile,
                has_keep,
                settlement_id,
                &candidate.location_id,
            )
        })
        .collect()
}

const PUBLIC_DISCOVERY_BACKOFF_MINUTES: u64 = 2 * 1_440;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PublicDiscoveryContactIdentity {
    resident_character_id: u64,
    conversation_id: String,
    location_id: String,
}

fn public_discovery_contact_identity(
    candidate: &PublicNpcCandidate,
) -> PublicDiscoveryContactIdentity {
    PublicDiscoveryContactIdentity {
        resident_character_id: candidate.resident_character_id,
        conversation_id: candidate.conversation_id.clone(),
        location_id: candidate.location_id.clone(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicDiscoveryFingerprint {
    settlement_id: String,
    contacts: Vec<PublicDiscoveryContactIdentity>,
    active_symptoms: Vec<(String, String, u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicDiscoveryBackoff {
    fingerprint: PublicDiscoveryFingerprint,
    last_contact: PublicDiscoveryContactIdentity,
    retry_at: u64,
}

fn public_discovery_backoff_active(
    backoff: &PublicDiscoveryBackoff,
    fingerprint: &PublicDiscoveryFingerprint,
    official_minute: u64,
) -> bool {
    backoff.fingerprint == *fingerprint && official_minute < backoff.retry_at
}

fn public_discovery_previous_contact<'a>(
    backoff: Option<&'a PublicDiscoveryBackoff>,
    fingerprint: &PublicDiscoveryFingerprint,
) -> Option<&'a PublicDiscoveryContactIdentity> {
    backoff
        .filter(|backoff| backoff.fingerprint == *fingerprint)
        .map(|backoff| &backoff.last_contact)
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
    previous_contact: Option<&PublicDiscoveryContactIdentity>,
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
    let next_index = previous_contact
        .and_then(|previous| {
            candidates
                .iter()
                .position(|candidate| public_discovery_contact_identity(candidate) == *previous)
        })
        .map_or(0, |index| (index + 1) % candidates.len());
    candidates.into_iter().nth(next_index)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicDiscoveryReferral {
    owner_character_id: u64,
    case_id: String,
    lead_id: String,
    summary: String,
    witness_name: String,
    expected_location: String,
    current_learned_location: String,
    corrected_by: String,
    recorded_at: u64,
}

impl From<BackendInvestigationLead> for PublicDiscoveryReferral {
    fn from(lead: BackendInvestigationLead) -> Self {
        Self {
            owner_character_id: lead.owner_character_id,
            case_id: lead.case_id,
            lead_id: lead.lead_id,
            summary: lead.summary,
            witness_name: lead.witness_name,
            expected_location: lead.expected_location,
            current_learned_location: lead.current_learned_location,
            corrected_by: lead.corrected_by,
            recorded_at: lead.recorded_at,
        }
    }
}

fn new_or_updated_public_discovery_referral(
    owner_character_id: u64,
    before: &HashMap<String, PublicDiscoveryReferral>,
    after: impl IntoIterator<Item = PublicDiscoveryReferral>,
) -> Option<PublicDiscoveryReferral> {
    after
        .into_iter()
        .filter(|lead| {
            lead.owner_character_id == owner_character_id
                && !lead.case_id.is_empty()
                && !lead.witness_name.is_empty()
                && lead.corrected_by.is_empty()
                && before.get(&lead.lead_id) != Some(lead)
        })
        .max_by_key(|lead| (lead.recorded_at, lead.lead_id.clone()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicDialogueProgressFingerprint {
    cases: Vec<(String, String, u64)>,
    leads: Vec<(String, u64, String, String, String, String, String)>,
    actions: Vec<(String, u32, bool, bool, String, u32)>,
    outcomes: Vec<(String, String, u64)>,
    sites: Vec<(String, String, bool, bool, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PublicDialogueAttemptKey {
    owner_character_id: u64,
    case_id: String,
    topic_id: String,
    contact: PublicDiscoveryContactIdentity,
}

fn public_dialogue_topic_attempt_allowed(
    last_no_progress: Option<&PublicDialogueProgressFingerprint>,
    current: &PublicDialogueProgressFingerprint,
) -> bool {
    last_no_progress != Some(current)
}

fn public_dialogue_topic_made_progress(
    before: &PublicDialogueProgressFingerprint,
    after: &PublicDialogueProgressFingerprint,
) -> bool {
    before != after
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
    fixture_disease: String,
    draft: std::sync::Arc<std::sync::Mutex<FailureDraft>>,
}

impl FailureRecorder {
    fn new(output: Option<PathBuf>, fixture_disease: String) -> Self {
        Self {
            output,
            fixture_disease,
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
            fixture_disease: self.fixture_disease.clone(),
            metrics: draft.metrics,
            quest_coverage: None,
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

const SAFE_TRAVEL_FAILURE_OPERATIONS: [&str; 11] = [
    "travel_to_case_site",
    "travel_to_generated_case_site",
    "unsafe_contract_retreat_to_settlement",
    "illness_retreat_to_settlement",
    "defeat_retreat_to_settlement",
    "return_to_settlement",
    "return_completed_generated_case",
    "generated_unchanged_defeat_retreat",
    "generated_defeat_retreat_to_settlement",
    "return_from_generated_case_site",
    "expedition_health_evacuation",
];

fn safe_failure_operation(error: &str) -> Option<&'static str> {
    [
        "perform_investigation_action",
        "wait_for_investigation_window_settlement",
        "wait_for_investigation_window_camp",
        "start_discovery_dialogue",
        "choose_dialogue_topic",
        "rest_at_camp",
        "continue_camp_travel",
        "travel_camps",
        "passive_no_actionable_rest",
        "sponsor_party_member_inn_rest",
        "purchase_journey_provisions",
        "purchase_party_tent",
        "purchase_ammunition",
        "withdraw_purchase_coin",
        "purchase_from_herbalist",
        "finalize_storefront_trade",
        "purchase_personal_storefront_with_party_stake",
        "administer_preparation",
    ]
    .into_iter()
    .chain(SAFE_TRAVEL_FAILURE_OPERATIONS)
    .find(|operation| {
        error.starts_with(&format!("{operation} failed:"))
            || error.starts_with(&format!("{operation} timed out"))
            || error.starts_with(&format!("could not send {operation}:"))
    })
}

fn safe_travel_failure(error: &str) -> bool {
    safe_failure_operation(error)
        .is_some_and(|operation| SAFE_TRAVEL_FAILURE_OPERATIONS.contains(&operation))
}

fn safe_failure_reason_code(error: &str, category: &str) -> &'static str {
    if error.contains("journey_held_no_actionable_actor")
        || error.contains("journey has no ready, asymptomatic, noncritical actor")
        || error.contains("camp rest left no ready, asymptomatic, noncritical actor")
    {
        "journey_held_no_actionable_actor"
    } else if error.contains("Rest until the party reaches its next daylight walking window") {
        "journey_daylight_window_rest_required"
    } else if safe_travel_failure(error) {
        "journey_travel_reducer_failed"
    } else if error.contains("start_discovery_dialogue") {
        "discovery_contact_failed"
    } else if error.contains("purchase_journey_provisions") {
        "journey_provision_purchase_failed"
    } else if error.contains("purchase_party_tent") {
        "party_tent_purchase_failed"
    } else if error.contains("purchase_ammunition") || error.contains("withdraw_purchase_coin") {
        "ammunition_purchase_failed"
    } else if error.contains("purchase_from_herbalist") {
        "medical_purchase_failed"
    } else if error.contains("finalize_storefront_trade")
        || error.contains("purchase_personal_storefront_with_party_stake")
    {
        "equipment_storefront_trade_failed"
    } else if error.contains("administer_preparation") {
        "medical_intervention_failed"
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
    } else if safe_travel_failure(error) {
        (
            "journey_travel_failed",
            "The authoritative journey transition could not be completed.",
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
    } else if error.contains("purchase_party_tent") {
        (
            "survival_purchase_failed",
            "The public party-shelter purchase could not be completed.",
        )
    } else if error.contains("purchase_ammunition") || error.contains("withdraw_purchase_coin") {
        (
            "survival_purchase_failed",
            "The public ammunition preparation could not be completed.",
        )
    } else if error.contains("purchase_from_herbalist") {
        (
            "medical_purchase_failed",
            "The selected public herbalist preparation could not be purchased.",
        )
    } else if error.contains("finalize_storefront_trade")
        || error.contains("purchase_personal_storefront_with_party_stake")
    {
        (
            "equipment_purchase_failed",
            "The revalidated public equipment purchase was rejected by authoritative storefront rules.",
        )
    } else if error.contains("administer_preparation") {
        (
            "medical_intervention_failed",
            "The selected public preparation was rejected by authoritative intervention rules.",
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
