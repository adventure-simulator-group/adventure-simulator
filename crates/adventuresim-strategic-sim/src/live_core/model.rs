use crate::{ActivityPreference, AgentProfile, BuildRole, EquipmentStyle, generate_profile};
use adventuresim_core::case::CaseStatus as DomainCaseStatus;
use adventuresim_core::dialogue_boundary::{PublicDialogueStartError, PublicDialogueStartOutcome};
use adventuresim_core::investigation::DestinationKnowledgeStage as CoreDestinationKnowledgeStage;
use adventuresim_core::morale::IncapacitationStatus as DomainIncapacitationStatus;
use adventuresim_core::personality::{
    self as core_personality, Conscience, Conviction, Drive, Nerve, SelfRegard, Sociability,
    Transparency,
};
use adventuresim_core::physical_object::{CarriedInventoryScope, OperationalCustody};
use adventuresim_core::reducer_error::{ReducerErrorCode, parse_reducer_error};
use adventuresim_core::simulation_security::{
    SIM_BOOTSTRAP_TOKEN_ENV as BOOTSTRAP_TOKEN_ENV,
    SIM_BOOTSTRAP_TOKEN_HEX_LEN as BOOTSTRAP_TOKEN_HEX_LEN,
};
use adventuresim_core::strategic_presence::DailyPresenceWindow;
use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table};
use adventuresim_stdb_client::*;
use adventuresim_world_schema::{
    SettlementActionService as DomainSettlementActionService,
    coordinates::{LatitudeE7, LatitudeMicrodegrees, LongitudeE7, LongitudeMicrodegrees},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

use adventuresim_core::strategic_currency::is_currency_id;
use adventuresim_core::strategic_time::{
    DEFAULT_JOURNEY_START_MINUTE_OF_DAY, DEFAULT_NIGHT_JOURNEY_START_MINUTE_OF_DAY, MINUTES_PER_DAY,
};
use url::Url;

use adventuresim_stdb_client::{
    abandon_contract_reducer::abandon_contract, accept_contract_reducer::accept_contract,
    accept_party_join_request_reducer::accept_party_join_request,
    administer_preparation_reducer::administer_preparation,
    advance_simulation_world_time_reducer::advance_simulation_world_time,
    autoresolve_mission_reducer::autoresolve_mission,
    autoresolve_report_table::AutoresolveReportTableAccess,
    backend_authority_arrest_actions_table::BackendAuthorityArrestActionsTableAccess,
    backend_case_battles_table::BackendCaseBattlesTableAccess,
    backend_case_site_pins_table::BackendCaseSitePinsTableAccess,
    backend_character_attributes_table::BackendCharacterAttributesTableAccess,
    backend_character_capabilities_table::BackendCharacterCapabilitiesTableAccess,
    backend_character_deaths_table::BackendCharacterDeathsTableAccess,
    backend_character_limbs_table::BackendCharacterLimbsTableAccess,
    backend_character_needs_table::BackendCharacterNeedsTableAccess,
    backend_character_stats_table::BackendCharacterStatsTableAccess,
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
    backend_road_challenges_table::BackendRoadChallengesTableAccess,
    backend_settlement_residents_table::BackendSettlementResidentsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_equipped_item_table::CharacterEquippedItemTableAccess,
    character_illness_status_table::CharacterIllnessStatusTableAccess,
    choose_dialogue_topic_reducer::choose_dialogue_topic,
    claim_simulation_run_reducer::claim_simulation_run,
    configure_simulation_character_reducer::configure_simulation_character,
    container_liquid_table::ContainerLiquidTableAccess,
    continue_camp_travel_reducer::continue_camp_travel,
    contract_interaction_stage_type::ContractInteractionStage,
    contract_status_type::ContractStatus,
    create_named_character_with_id_reducer::create_named_character_with_id,
    deposit_party_inventory_item_reducer::deposit_party_inventory_item,
    ensure_settlement_activity_reducer::ensure_settlement_activity,
    equipment_occupancy_table::EquipmentOccupancyTableAccess, field_shelter_type::FieldShelter,
    finalize_merchant_trade_reducer::finalize_merchant_trade, food_lot_table::FoodLotTableAccess,
    inventory_containment_table::InventoryContainmentTableAccess,
    inventory_item_amount_table::InventoryItemAmountTableAccess,
    inventory_item_table::InventoryItemTableAccess,
    inventory_object_table::InventoryObjectTableAccess,
    item_condition_table::ItemConditionTableAccess, item_table::ItemTableAccess,
    limb_injury_table::LimbInjuryTableAccess,
    liquidate_party_inventory_reducer::liquidate_party_inventory,
    local_problem_symptom_table::LocalProblemSymptomTableAccess,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_item_amount_table::PartyItemAmountTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess,
    party_journey_table::PartyJourneyTableAccess, party_member_table::PartyMemberTableAccess,
    party_stake_table::PartyStakeTableAccess, party_table::PartyTableAccess,
    perform_investigation_action_reducer::perform_investigation_action,
    purchase_from_herbalist_reducer::purchase_from_herbalist,
    purchase_personal_storefront_with_party_stake_reducer::purchase_personal_storefront_with_party_stake,
    register_strategic_gateway_reducer::register_strategic_gateway,
    repair_order_table::RepairOrderTableAccess,
    replace_item_at_placement_reducer::replace_item_at_placement,
    report_contract_reducer::report_contract,
    request_general_party_join_reducer::request_general_party_join,
    resolve_errantry_road_challenge_reducer::resolve_errantry_road_challenge,
    resolve_strategic_encounter_reducer::resolve_strategic_encounter,
    rest_at_camp_reducer::rest_at_camp, retrieve_repaired_item_reducer::retrieve_repaired_item,
    seed_simulation_disease_reducer::seed_simulation_disease,
    seed_simulation_equipment_damage_reducer::seed_simulation_equipment_damage,
    seed_simulation_quest_fixture_reducer::seed_simulation_quest_fixture,
    seed_simulation_world_reducer::seed_simulation_world,
    set_party_travel_itinerary_reducer::set_party_travel_itinerary,
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
    surrender_to_authority_reducer::surrender_to_authority,
    synchronize_party_for_activity_reducer::synchronize_party_for_activity,
    travel_to_case_site_reducer::travel_to_case_site,
    travel_to_settlement_reducer::travel_to_settlement, treat_limb_reducer::treat_limb,
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
const EXPEDITION_RECOVERY_REST_MINUTES: u64 = MINUTES_PER_DAY;
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
/// Keep the public minute-by-minute weather projection bounded. A route beyond
/// sixty days is not a credible case-site leg and fails closed instead of
/// consuming unbounded simulator work.
const MAX_CASE_SITE_THERMAL_FORECAST_MINUTES: u64 = 60 * MINUTES_PER_DAY;
const MIN_ACTIONABLE_PHYSIOLOGY_CONFIDENCE_BPS: u16 = 3_000;
/// Older observations can describe a materially different disease stage.
/// One strategic day permits ordinary asynchronous party observation without
/// allowing an indefinitely cached chart to direct treatment.
const MAX_ACTIONABLE_PHYSIOLOGY_CHART_AGE_MINUTES: u64 = MINUTES_PER_DAY;
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

fn core_destination_knowledge_stage(
    stage: DestinationKnowledgeStage,
) -> CoreDestinationKnowledgeStage {
    match stage {
        DestinationKnowledgeStage::Unknown => CoreDestinationKnowledgeStage::Unknown,
        DestinationKnowledgeStage::Textual => CoreDestinationKnowledgeStage::Textual,
        DestinationKnowledgeStage::Landmark => CoreDestinationKnowledgeStage::Landmark,
        DestinationKnowledgeStage::ApproximateArea => {
            CoreDestinationKnowledgeStage::ApproximateArea
        }
        DestinationKnowledgeStage::RouteSegment => CoreDestinationKnowledgeStage::RouteSegment,
        DestinationKnowledgeStage::ExactBelieved => CoreDestinationKnowledgeStage::ExactBelieved,
        DestinationKnowledgeStage::Visited => CoreDestinationKnowledgeStage::Visited,
    }
}

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
        const MAX_DURATION_YEARS: u32 = 100;
        const MAX_DURATION_DAYS: u32 =
            adventuresim_core::strategic_time::DAYS_PER_YEAR as u32 * MAX_DURATION_YEARS;

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
            || !(1..=MAX_DURATION_DAYS).contains(&self.duration_days)
            || !(2..=8).contains(&self.party_size)
            || self.party_size > self.population
        {
            return Err(format!(
                "population 2..=32, party_size 2..=8, cycles 1..=10000, and duration_days 1..={MAX_DURATION_DAYS} are required"
            ));
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
    pub authority_surrenders: u32,
    pub authority_fines_paid: u64,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    SafeDepartureWait,
    SafeDepartureWaitRelocated,
    AuthoritySurrender,
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum CoreLoopEventSubject {
    Agent,
    Character {
        character_id: u64,
    },
    DirectContract {
        party_id: String,
        contract_id: String,
    },
    GeneratedCase {
        party_id: String,
        case_id: String,
    },
    InvestigationAction {
        case_id: String,
        action_id: String,
    },
    Item {
        inventory_item_id: u64,
    },
    Encounter {
        party_id: String,
        encounter_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoreLoopEventSemanticKey {
    agent_id: u32,
    kind: CoreLoopEventKind,
    subject: CoreLoopEventSubject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoreLoopEventPayload {
    kind: CoreLoopEventKind,
    subject: CoreLoopEventSubject,
    detail: String,
}

impl CoreLoopEventPayload {
    fn agent(kind: CoreLoopEventKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Agent,
            detail: detail.into(),
        }
    }

    fn direct_contract(
        kind: CoreLoopEventKind,
        party_id: impl Into<String>,
        contract_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::DirectContract {
                party_id: party_id.into(),
                contract_id: contract_id.into(),
            },
            detail: detail.into(),
        }
    }

    fn character(kind: CoreLoopEventKind, character_id: u64, detail: impl Into<String>) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Character { character_id },
            detail: detail.into(),
        }
    }

    fn generated_case(
        kind: CoreLoopEventKind,
        party_id: impl Into<String>,
        case_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::GeneratedCase {
                party_id: party_id.into(),
                case_id: case_id.into(),
            },
            detail: detail.into(),
        }
    }

    fn investigation_action(
        kind: CoreLoopEventKind,
        case_id: impl Into<String>,
        action_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::InvestigationAction {
                case_id: case_id.into(),
                action_id: action_id.into(),
            },
            detail: detail.into(),
        }
    }

    fn item(kind: CoreLoopEventKind, inventory_item_id: u64, detail: impl Into<String>) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Item { inventory_item_id },
            detail: detail.into(),
        }
    }

    fn encounter(
        kind: CoreLoopEventKind,
        party_id: impl Into<String>,
        encounter_id: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            subject: CoreLoopEventSubject::Encounter {
                party_id: party_id.into(),
                encounter_id: encounter_id.into(),
            },
            detail: detail.into(),
        }
    }

    fn semantic_key(&self, agent_id: u32) -> CoreLoopEventSemanticKey {
        CoreLoopEventSemanticKey {
            agent_id,
            kind: self.kind.clone(),
            subject: self.subject.clone(),
        }
    }

    fn into_public(self, sequence: u64, agent_id: u32) -> CoreLoopEvent {
        CoreLoopEvent {
            sequence,
            agent_id,
            kind: self.kind,
            detail: self.detail,
        }
    }
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
    pub condition_status: DomainIncapacitationStatus,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestCoverageMetric {
    DuplicateSemanticEvents,
    EncounterWipes,
    FinalAgentsAlive,
    FinalAgentsNotCritical,
    FinalAgentsNotStranded,
    FixtureDirectAccepted,
    FixtureDirectEncountered,
    FixtureDirectReported,
    FixtureDirectTraveled,
    FixtureGeneratedCompleted,
    FixtureGeneratedDiscovered,
    FixtureGeneratedIntake,
    FixtureProvenance,
    FixtureSuccessfulCompletion,
    QuestsAttempted,
    QuestsAttemptedConsistency,
    ReducerFailures,
    StuckDetections,
}

impl QuestCoverageMetric {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateSemanticEvents => "duplicate_semantic_events",
            Self::EncounterWipes => "encounter_wipes",
            Self::FinalAgentsAlive => "final_agents_alive",
            Self::FinalAgentsNotCritical => "final_agents_not_critical",
            Self::FinalAgentsNotStranded => "final_agents_not_stranded",
            Self::FixtureDirectAccepted => "fixture_direct_accepted",
            Self::FixtureDirectEncountered => "fixture_direct_encountered",
            Self::FixtureDirectReported => "fixture_direct_reported",
            Self::FixtureDirectTraveled => "fixture_direct_traveled",
            Self::FixtureGeneratedCompleted => "fixture_generated_completed",
            Self::FixtureGeneratedDiscovered => "fixture_generated_discovered",
            Self::FixtureGeneratedIntake => "fixture_generated_intake",
            Self::FixtureProvenance => "fixture_provenance",
            Self::FixtureSuccessfulCompletion => "fixture_successful_completion",
            Self::QuestsAttempted => "quests_attempted",
            Self::QuestsAttemptedConsistency => "quests_attempted_consistency",
            Self::ReducerFailures => "reducer_failures",
            Self::StuckDetections => "stuck_detections",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestCoverageFailure {
    metric: QuestCoverageMetric,
}

impl QuestCoverageFailure {
    const fn new(metric: QuestCoverageMetric) -> Self {
        Self { metric }
    }

    pub const fn metric(self) -> QuestCoverageMetric {
        self.metric
    }
}

impl std::fmt::Display for QuestCoverageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "quest coverage acceptance failed: metric={}",
            self.metric.as_str()
        )
    }
}

impl std::error::Error for QuestCoverageFailure {}

pub fn validate_quest_coverage(report: &CoreLoopReport) -> Result<(), QuestCoverageFailure> {
    let metrics = &report.metrics;
    let coverage = report
        .quest_coverage
        .as_ref()
        .ok_or_else(|| QuestCoverageFailure::new(QuestCoverageMetric::FixtureProvenance))?;
    let checks = [
        (
            QuestCoverageMetric::ReducerFailures,
            metrics.reducer_failures == 0,
        ),
        (
            QuestCoverageMetric::DuplicateSemanticEvents,
            metrics.duplicate_semantic_events == 0,
        ),
        (
            QuestCoverageMetric::StuckDetections,
            metrics.stuck_detections == 0,
        ),
        (
            QuestCoverageMetric::EncounterWipes,
            metrics.encounter_wipes == 0,
        ),
        (
            QuestCoverageMetric::FixtureDirectAccepted,
            coverage.direct_accepted,
        ),
        (
            QuestCoverageMetric::FixtureDirectTraveled,
            coverage.direct_traveled,
        ),
        (
            QuestCoverageMetric::FixtureDirectEncountered,
            coverage.direct_encountered,
        ),
        (
            QuestCoverageMetric::FixtureDirectReported,
            coverage.direct_reported || coverage.direct_safely_abandoned,
        ),
        (
            QuestCoverageMetric::FixtureGeneratedIntake,
            coverage.generated_intake,
        ),
        (
            QuestCoverageMetric::FixtureGeneratedDiscovered,
            coverage.generated_discovered,
        ),
        (
            QuestCoverageMetric::FixtureGeneratedCompleted,
            coverage.generated_completed,
        ),
        (
            QuestCoverageMetric::FixtureSuccessfulCompletion,
            coverage.direct_reported || coverage.generated_completed,
        ),
        (
            QuestCoverageMetric::QuestsAttempted,
            metrics.quests_attempted >= 2,
        ),
        (
            QuestCoverageMetric::QuestsAttemptedConsistency,
            metrics.quests_attempted
                == metrics
                    .direct_contracts_attempted
                    .saturating_add(metrics.generated_case_intakes),
        ),
    ];
    if let Some((metric, _)) = checks.into_iter().find(|(_, passed)| !passed) {
        return Err(QuestCoverageFailure::new(metric));
    }
    if report.final_agents.iter().any(|agent| !agent.alive) {
        return Err(QuestCoverageFailure::new(
            QuestCoverageMetric::FinalAgentsAlive,
        ));
    }
    if report.final_agents.iter().any(|agent| agent.critical) {
        return Err(QuestCoverageFailure::new(
            QuestCoverageMetric::FinalAgentsNotCritical,
        ));
    }
    if report.final_agents.iter().any(|agent| {
        agent.settlement_id.is_none()
            || agent.current_case_site_id.is_some()
            || agent.journey_destination.is_some()
    }) {
        return Err(QuestCoverageFailure::new(
            QuestCoverageMetric::FinalAgentsNotStranded,
        ));
    }
    Ok(())
}

/// Persist the same public-safe diagnostic shape used by reducer failures
/// when the completed report fails the stricter quest-coverage contract.
pub fn write_quest_coverage_failure(
    report: &CoreLoopReport,
    path: &Path,
    error: &QuestCoverageFailure,
) -> Result<(), String> {
    let reason_code = error.metric().as_str();
    let (trace, trace_truncated) = bounded_failure_trace(&report.trace, report.total_event_count);
    let final_agents = report
        .final_agents
        .iter()
        .map(|agent| CoreLoopFailureAgent {
            agent_id: agent.agent_id,
            character_id: agent.character_id,
            alive: agent.alive,
            condition_status: agent.condition_status,
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
        message: error.to_string(),
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
const MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES: u32 = MINUTES_PER_DAY as u32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopFailureAgent {
    pub agent_id: u32,
    pub character_id: u64,
    pub alive: bool,
    pub condition_status: DomainIncapacitationStatus,
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
enum DepartureDeferralReason {
    AmmunitionProviderProjectionUnavailable,
    AmmunitionUnaffordable,
    AmmunitionWouldOverload,
    EquipmentNotReady,
    PartyLoadUnsafe,
    PartyTentQuoteUnavailable,
    PartyTentUnaffordable,
    PartyTentWouldOverload,
    RouteActionNotSurvivable,
    RouteActionSiteMismatch,
    RouteFatigueRecoveryRequired,
    RouteThermalRisk,
    RouteThermalUnsafeAllPublicWindows,
    RouteWeatherProjectionUnavailable,
    SafePublicRouteWindow,
    WaitTowardSafePublicRouteWindow,
    SurvivalProjectionUnavailable,
    SurvivalReadinessRequiresSettlement,
    ThermalRecoveryRequired,
}

impl DepartureDeferralReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AmmunitionProviderProjectionUnavailable => {
                "ammunition_provider_projection_unavailable"
            }
            Self::AmmunitionUnaffordable => "ammunition_unaffordable",
            Self::AmmunitionWouldOverload => "ammunition_would_overload",
            Self::EquipmentNotReady => "equipment_not_ready",
            Self::PartyLoadUnsafe => "party_load_unsafe",
            Self::PartyTentQuoteUnavailable => "party_tent_quote_unavailable",
            Self::PartyTentUnaffordable => "party_tent_unaffordable",
            Self::PartyTentWouldOverload => "party_tent_would_overload",
            Self::RouteActionNotSurvivable => "route_action_not_survivable",
            Self::RouteActionSiteMismatch => "route_action_site_mismatch",
            Self::RouteFatigueRecoveryRequired => "route_fatigue_recovery_required",
            Self::RouteThermalRisk => "route_thermal_risk",
            Self::RouteThermalUnsafeAllPublicWindows => "route_thermal_unsafe_all_public_windows",
            Self::RouteWeatherProjectionUnavailable => "route_weather_projection_unavailable",
            Self::SafePublicRouteWindow => "safe_public_route_window",
            Self::WaitTowardSafePublicRouteWindow => "wait_toward_safe_public_route_window",
            Self::SurvivalProjectionUnavailable => "survival_projection_unavailable",
            Self::SurvivalReadinessRequiresSettlement => "survival_readiness_requires_settlement",
            Self::ThermalRecoveryRequired => "thermal_recovery_required",
        }
    }
}

impl std::fmt::Display for DepartureDeferralReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DepartureReadiness {
    Ready,
    ReadyWithItinerary {
        walking_minutes_per_day: u16,
        travel_at_night: bool,
        case_site_recovery_minutes: u64,
    },
    WaitForSafeDeparture {
        reason: DepartureDeferralReason,
        wait_minutes: u64,
        walking_minutes_per_day: u16,
        travel_at_night: bool,
        case_site_recovery_minutes: u64,
    },
    Deferred(DepartureDeferralReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SettlementDepartureWait<'a> {
    character_id: u64,
    agent: u32,
    case_id: &'a str,
    reason: DepartureDeferralReason,
    wait_minutes: u64,
    walking_minutes_per_day: u16,
    travel_at_night: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct SelectedCaseSitePlan {
    walking_minutes_per_day: u16,
    travel_at_night: bool,
    departure_wait_minutes: u64,
    outbound: adventuresim_core::strategic_time::ItineraryForecast,
    returned: adventuresim_core::strategic_time::ItineraryForecast,
    minimum_insulation_bps: u16,
    case_site_recovery_minutes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnSiteActionDecision {
    Ready,
    RestThenRetry(u64),
    ReturnNow,
    Hold,
}

fn classify_on_site_action_decision(
    action_return_safe: bool,
    rest_action_return_safe: bool,
    recovery_minutes: u64,
    return_now_safe: bool,
) -> OnSiteActionDecision {
    if action_return_safe {
        OnSiteActionDecision::Ready
    } else if rest_action_return_safe && (1..=MINUTES_PER_DAY).contains(&recovery_minutes) {
        OnSiteActionDecision::RestThenRetry(recovery_minutes)
    } else if return_now_safe {
        OnSiteActionDecision::ReturnNow
    } else {
        OnSiteActionDecision::Hold
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedAdvanceResult {
    Progressed,
    RecoveryCommitted,
    NoProgress,
}

fn classify_generated_advance(
    public_progressed: bool,
    elapsed_advanced: bool,
) -> GeneratedAdvanceResult {
    if public_progressed {
        GeneratedAdvanceResult::Progressed
    } else if elapsed_advanced {
        GeneratedAdvanceResult::RecoveryCommitted
    } else {
        GeneratedAdvanceResult::NoProgress
    }
}

fn calories_after_strenuous_action(calories_used: f32, action_minutes: u64) -> f32 {
    calories_used
        + action_minutes as f32 / MINUTES_PER_DAY as f32
            * adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
}

fn projected_action_ready(
    nonfatigue_incapacitation: f32,
    calories_after_action: f32,
    fatigue_capacity: f32,
) -> bool {
    projected_action_status(
        nonfatigue_incapacitation,
        calories_after_action,
        fatigue_capacity,
    ) == adventuresim_core::morale::IncapacitationStatus::Ready
}

fn projected_action_survivable(
    nonfatigue_incapacitation: f32,
    calories_after_action: f32,
    fatigue_capacity: f32,
) -> bool {
    projected_action_status(
        nonfatigue_incapacitation,
        calories_after_action,
        fatigue_capacity,
    ) != adventuresim_core::morale::IncapacitationStatus::Incapacitated
}

fn projected_itinerary_survivable(
    nonfatigue_incapacitation: f32,
    itinerary: &adventuresim_core::strategic_time::ItineraryForecast,
    member_index: usize,
    fatigue_capacity: f32,
) -> bool {
    itinerary
        .member_maximum_fatigue
        .get(member_index)
        .is_some_and(|fatigue| {
            projected_action_survivable(
                nonfatigue_incapacitation,
                fatigue * fatigue_capacity,
                fatigue_capacity,
            )
        })
}

fn projected_action_status(
    nonfatigue_incapacitation: f32,
    calories_after_action: f32,
    fatigue_capacity: f32,
) -> adventuresim_core::morale::IncapacitationStatus {
    adventuresim_core::morale::StrategicIncapacitation {
        pain: nonfatigue_incapacitation.max(0.0),
        fatigue: adventuresim_core::morale::fatigue_incapacitation(
            calories_after_action / fatigue_capacity.max(0.01),
        ),
        ..Default::default()
    }
    .status()
}

fn round_trip_walking_window_minutes(
    current_walking_minutes: u16,
    movement_minutes: u64,
    action_minutes: u64,
) -> Option<u16> {
    let required = movement_minutes
        .checked_mul(2)?
        .checked_add(action_minutes)?;
    let required_breakpoint = required.checked_add(59)?.checked_div(60)?.checked_mul(60)?;
    u16::try_from(required_breakpoint.max(u64::from(current_walking_minutes)))
        .ok()
        .filter(|minutes| u64::from(*minutes) <= MINUTES_PER_DAY)
}

fn generated_action_walking_windows(
    current_walking_minutes: u16,
    movement_minutes: u64,
    action_minutes: u64,
) -> Vec<u16> {
    let mut windows = Vec::new();
    let mut push = |minutes: u64| {
        if let Ok(minutes) = u16::try_from(minutes)
            && (adventuresim_core::strategic_time::MIN_WALKING_MINUTES_PER_DAY
                ..=adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY)
                .contains(&minutes)
            && !windows.contains(&minutes)
        {
            windows.push(minutes);
        }
    };
    push(u64::from(current_walking_minutes));
    if let Some(widened) =
        round_trip_walking_window_minutes(current_walking_minutes, movement_minutes, action_minutes)
    {
        push(u64::from(widened));
    }
    let exact_action_breakpoint = movement_minutes.saturating_add(action_minutes);
    push(exact_action_breakpoint);
    push(movement_minutes);
    let descent_start = exact_action_breakpoint.min(u64::from(
        adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY,
    ));
    for minutes in (u64::from(adventuresim_core::strategic_time::MIN_WALKING_MINUTES_PER_DAY)
        ..descent_start)
        .rev()
    {
        push(minutes);
    }
    windows
}

fn select_generated_case_site_plan<T>(
    current_walking_minutes: u16,
    movement_minutes: u64,
    action_minutes: u64,
    current_travel_at_night: bool,
    starting_minute: u64,
    mut evaluate: impl FnMut(u16, bool, u64) -> Option<T>,
) -> Option<T> {
    let windows =
        generated_action_walking_windows(current_walking_minutes, movement_minutes, action_minutes);
    for travel_at_night in [current_travel_at_night, !current_travel_at_night] {
        for (window_index, &walking_minutes) in windows.iter().enumerate() {
            if adventuresim_core::strategic_time::is_walking_time(
                starting_minute,
                walking_minutes,
                travel_at_night,
            ) && let Some(plan) = evaluate(walking_minutes, travel_at_night, 0)
            {
                return Some(plan);
            }
            let candidate_waits = if window_index < 4 {
                generated_safe_departure_waits(starting_minute, walking_minutes, travel_at_night)
            } else {
                generated_daily_walking_start_waits(
                    starting_minute,
                    walking_minutes,
                    travel_at_night,
                )
            };
            for wait_minutes in candidate_waits {
                if let Some(plan) = evaluate(walking_minutes, travel_at_night, wait_minutes) {
                    return Some(plan);
                }
            }
        }
    }
    None
}

fn joint_case_site_plan_failure_reason(
    complete_candidate_count: u32,
    thermally_safe_complete_candidate_count: u32,
    candidate_projection_unavailable: bool,
    candidate_fatigue_unsafe: bool,
    candidate_site_mismatch: bool,
) -> DepartureDeferralReason {
    if complete_candidate_count > 0 && thermally_safe_complete_candidate_count == 0 {
        DepartureDeferralReason::RouteThermalUnsafeAllPublicWindows
    } else if thermally_safe_complete_candidate_count > 0 && candidate_fatigue_unsafe {
        DepartureDeferralReason::RouteFatigueRecoveryRequired
    } else if candidate_site_mismatch {
        DepartureDeferralReason::RouteActionSiteMismatch
    } else if complete_candidate_count > 0 || !candidate_projection_unavailable {
        DepartureDeferralReason::RouteActionNotSurvivable
    } else {
        DepartureDeferralReason::RouteWeatherProjectionUnavailable
    }
}

fn safe_departure_wait_minutes(
    immediate_safe: bool,
    delayed_safe: bool,
    wait_minutes: Option<u64>,
) -> Option<u64> {
    (!immediate_safe && delayed_safe)
        .then_some(wait_minutes?)
        .filter(|minutes| (60..=MINUTES_PER_DAY).contains(minutes))
}

const MAX_CASE_SITE_SAFE_WINDOW_SEARCH_DAYS: u64 = 7;
const MAX_CASE_SITE_SAFE_WINDOW_SEARCH_MINUTES: u64 =
    MAX_CASE_SITE_SAFE_WINDOW_SEARCH_DAYS * MINUTES_PER_DAY;

fn generated_safe_departure_waits(
    starting_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Vec<u64> {
    let mut waits = (60..=MAX_CASE_SITE_SAFE_WINDOW_SEARCH_MINUTES)
        .step_by(60)
        .filter(|wait| {
            adventuresim_core::strategic_time::is_walking_time(
                starting_minute.saturating_add(*wait),
                walking_minutes,
                travel_at_night,
            )
        })
        .collect::<Vec<_>>();
    waits.extend(generated_daily_walking_start_waits(
        starting_minute,
        walking_minutes,
        travel_at_night,
    ));
    waits.sort_unstable();
    waits.dedup();
    waits
}

fn generated_daily_walking_start_waits(
    starting_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Vec<u64> {
    (0..=MAX_CASE_SITE_SAFE_WINDOW_SEARCH_DAYS)
        .filter_map(|day_offset| {
            let day_wait = day_offset * MINUTES_PER_DAY;
            adventuresim_core::strategic_time::minutes_until_next_walking_start(
                starting_minute.saturating_add(day_wait),
                walking_minutes,
                travel_at_night,
            )
            .and_then(|wait| forecast_safe_departure_wait_minutes(day_wait.saturating_add(wait)))
        })
        .collect()
}

fn forecast_safe_departure_wait_minutes(next_walking_start: u64) -> Option<u64> {
    (next_walking_start <= MAX_CASE_SITE_SAFE_WINDOW_SEARCH_MINUTES)
        .then_some(next_walking_start.max(60))
}

fn representable_safe_departure_wait_minutes(next_walking_start: u64) -> Option<u64> {
    (next_walking_start <= MINUTES_PER_DAY).then_some(next_walking_start.max(60))
}

#[derive(Clone, Debug, PartialEq)]
struct ActivityObservation {
    personal_gold_coin: u64,
    condition_status: DomainIncapacitationStatus,
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
    condition_status: Option<DomainIncapacitationStatus>,
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

#[derive(Clone, Copy, Debug)]
struct ExpeditionDiagnosticContext<'a> {
    party_id: &'a str,
    phase: &'a str,
    action: &'a str,
    reason: &'a str,
}

#[derive(Clone, Copy, Debug)]
struct ExpeditionObservationChange<'a> {
    members_before: &'a [ExpeditionMemberObservation],
    members_after: &'a [ExpeditionMemberObservation],
    supplies_before: ExpeditionSuppliesObservation,
    supplies_after: ExpeditionSuppliesObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpeditionRecoveryOutcome {
    None,
    Resumed,
    Returned,
    Evacuated,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JourneyTravelOutcome {
    Completed,
    DeferredForDaylightWindow,
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
    member.alive
        && (member.condition_status != Some(DomainIncapacitationStatus::Ready)
            || member.symptomatic
            || member.critical)
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
            member.condition_status == Some(DomainIncapacitationStatus::Ready)
                && !member.symptomatic
                && !member.critical
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

fn observed_activity_return_origin(
    observations: &HashMap<(String, String), String>,
    party_id: &str,
    current_case_site_id: Option<&str>,
) -> Option<String> {
    let site_id = current_case_site_id?;
    observations
        .get(&(party_id.to_owned(), site_id.to_owned()))
        .cloned()
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
        && living
            .iter()
            .all(|member| member.condition_status.is_some() && !member.critical)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicRoutePoint {
    latitude: LatitudeMicrodegrees,
    longitude: LongitudeMicrodegrees,
    elevation_m: i16,
}

impl PublicRoutePoint {
    fn from_degrees(latitude: f64, longitude: f64, elevation_m: i16) -> Option<Self> {
        Some(Self {
            latitude: LatitudeMicrodegrees::from_degrees(latitude)?,
            longitude: LongitudeMicrodegrees::from_degrees(longitude)?,
            elevation_m,
        })
    }

    fn from_e7(latitude: i32, longitude: i32, elevation_m: i16) -> Option<Self> {
        Some(Self {
            latitude: LatitudeE7::new(latitude)?.to_microdegrees(),
            longitude: LongitudeE7::new(longitude)?.to_microdegrees(),
            elevation_m,
        })
    }
}

fn public_straight_line_distance_m(
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    geographic: bool,
) -> u64 {
    let longitude_delta = (i64::from(destination.longitude.get())
        - i64::from(origin.longitude.get())) as f64
        / f64::from(LongitudeMicrodegrees::UNITS_PER_DEGREE);
    let latitude_delta = (i64::from(destination.latitude.get()) - i64::from(origin.latitude.get()))
        as f64
        / f64::from(LatitudeMicrodegrees::UNITS_PER_DEGREE);
    let distance_m = if geographic {
        let origin_latitude = origin.latitude.degrees();
        let destination_latitude = destination.latitude.degrees();
        let lat1 = origin_latitude.to_radians();
        let lat2 = destination_latitude.to_radians();
        let delta_lat = latitude_delta.to_radians();
        let delta_lon = longitude_delta.to_radians();
        let a = ((delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2))
        .clamp(0.0, 1.0);
        6_371_000.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
    } else {
        longitude_delta.hypot(latitude_delta) * 1_000.0
    };
    distance_m.round().max(1.0) as u64
}

fn case_site_movement_minutes(distance_m: u64) -> Option<u64> {
    (distance_m > 0).then(|| ((distance_m as f64 / 1_250.0) * 60.0).ceil() as u64)
}

fn projected_itinerary_thermal_safe(
    starting_minute: u64,
    itinerary: &adventuresim_core::strategic_time::ItineraryForecast,
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
) -> Option<bool> {
    projected_itinerary_thermal_state(
        starting_minute,
        itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        has_tent,
    )
    .map(|projection| projection.safe)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PublicThermalProjection {
    state: adventuresim_core::survival::SurvivalState,
    safe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicRoundTripRoute {
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicThermalTraveler {
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
}

#[derive(Clone, Copy, Debug)]
struct RoundTripThermalProjection<'a> {
    starting_minute: u64,
    outbound_itinerary: &'a adventuresim_core::strategic_time::ItineraryForecast,
    return_itinerary: &'a adventuresim_core::strategic_time::ItineraryForecast,
    action_minutes: u64,
    route: PublicRoundTripRoute,
    traveler: PublicThermalTraveler,
}

fn projected_itinerary_thermal_state(
    starting_minute: u64,
    itinerary: &adventuresim_core::strategic_time::ItineraryForecast,
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
) -> Option<PublicThermalProjection> {
    if itinerary.truncated
        || itinerary.total_elapsed_minutes == 0
        || itinerary.total_elapsed_minutes > MAX_CASE_SITE_THERMAL_FORECAST_MINUTES
        || itinerary.total_movement_minutes == 0
    {
        return None;
    }
    let clothing = adventuresim_core::survival::ClothingExposure {
        insulation_bps,
        // Public equipped definitions are sufficient to reproduce insulation.
        // Layer ordering is not projected here, so rain protection deliberately
        // fails safe at zero rather than assuming an advantageous outer shell.
        weatherproofing_bps: 0,
        peripheral_protection_bps: [0; 4],
    };
    let mut state = starting_state;
    for segment in &itinerary.segments {
        for local_offset in 0..segment.elapsed_minutes {
            let offset = segment.elapsed_start.saturating_add(local_offset);
            let movement_offset = segment.movement_start.saturating_add(
                if segment.kind == adventuresim_core::strategic_time::ItinerarySegmentKind::Walking
                {
                    local_offset.min(segment.movement_minutes)
                } else {
                    0
                },
            );
            let interpolate = |start: i32, end: i32| {
                let delta = i64::from(end) - i64::from(start);
                (i64::from(start)
                    + delta.saturating_mul(movement_offset as i64)
                        / itinerary.total_movement_minutes as i64)
                    .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
            };
            let elevation = interpolate(
                i32::from(origin.elevation_m),
                i32::from(destination.elevation_m),
            )
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            let weather = adventuresim_core::weather::weather_at(
                adventuresim_core::weather::WORLD_WEATHER_SEED,
                starting_minute.saturating_add(offset),
                interpolate(origin.latitude.get(), destination.latitude.get()),
                interpolate(origin.longitude.get(), destination.longitude.get()),
                elevation,
            );
            let shelter = if segment.kind
                == adventuresim_core::strategic_time::ItinerarySegmentKind::Camp
                && has_tent
            {
                adventuresim_core::survival::ExposureShelter::Field(
                    adventuresim_core::survival::FieldShelter::Tent,
                )
            } else {
                adventuresim_core::survival::ExposureShelter::Field(
                    adventuresim_core::survival::FieldShelter::Bivouac,
                )
            };
            state = adventuresim_core::survival::advance_exposure(
                state,
                std::iter::once(weather),
                clothing,
                shelter,
            )
            .state;
            if state.thermal_strain <= adventuresim_core::survival::COLD_STAGGER_STRAIN
                || state.thermal_strain >= adventuresim_core::survival::HEAT_STAGGER_STRAIN
            {
                return Some(PublicThermalProjection { state, safe: false });
            }
        }
    }
    Some(PublicThermalProjection { state, safe: true })
}

fn projected_stationary_outdoor_thermal_state(
    starting_minute: u64,
    duration_minutes: u64,
    location: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
) -> Option<PublicThermalProjection> {
    projected_stationary_field_thermal_state(
        starting_minute,
        duration_minutes,
        location,
        starting_state,
        insulation_bps,
        false,
    )
}

fn projected_stationary_field_thermal_state(
    starting_minute: u64,
    duration_minutes: u64,
    location: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
) -> Option<PublicThermalProjection> {
    if duration_minutes == 0 {
        return Some(PublicThermalProjection {
            state: starting_state,
            safe: true,
        });
    }
    let itinerary = adventuresim_core::strategic_time::ItineraryForecast {
        segments: vec![adventuresim_core::strategic_time::ItinerarySegment {
            kind: adventuresim_core::strategic_time::ItinerarySegmentKind::Camp,
            elapsed_start: 0,
            elapsed_minutes: duration_minutes,
            movement_start: 0,
            movement_minutes: 0,
            average_fatigue_start: 0.0,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
            required_rest_minutes: 0,
        }],
        member_final_fatigue: vec![0.0],
        member_maximum_fatigue: vec![0.0],
        total_elapsed_minutes: duration_minutes,
        // The shared itinerary projector requires a nonzero movement bound;
        // identical endpoints keep this stationary despite that sentinel.
        total_movement_minutes: 1,
        truncated: false,
    };
    projected_itinerary_thermal_state(
        starting_minute,
        &itinerary,
        location,
        location,
        starting_state,
        insulation_bps,
        has_tent,
    )
}

fn projected_round_trip_thermal_safe(projection: RoundTripThermalProjection<'_>) -> Option<bool> {
    let RoundTripThermalProjection {
        starting_minute,
        outbound_itinerary,
        return_itinerary,
        action_minutes,
        route: PublicRoundTripRoute {
            origin,
            destination,
        },
        traveler:
            PublicThermalTraveler {
                starting_state,
                insulation_bps,
                has_tent,
            },
    } = projection;
    let outbound = projected_itinerary_thermal_state(
        starting_minute,
        outbound_itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        has_tent,
    )?;
    let action_start = starting_minute.saturating_add(outbound_itinerary.total_elapsed_minutes);
    let action = projected_stationary_outdoor_thermal_state(
        action_start,
        action_minutes,
        destination,
        outbound.state,
        insulation_bps,
    )?;
    let returned = projected_itinerary_thermal_state(
        action_start.saturating_add(action_minutes),
        return_itinerary,
        destination,
        origin,
        action.state,
        insulation_bps,
        has_tent,
    )?;
    Some(outbound.safe && action.safe && returned.safe)
}

fn projected_recovery_round_trip_thermal_safe(
    projection: RoundTripThermalProjection<'_>,
    recovery_minutes: u64,
) -> Option<bool> {
    let RoundTripThermalProjection {
        starting_minute,
        outbound_itinerary,
        return_itinerary,
        action_minutes,
        route: PublicRoundTripRoute {
            origin,
            destination,
        },
        traveler:
            PublicThermalTraveler {
                starting_state,
                insulation_bps,
                has_tent,
            },
    } = projection;
    let outbound = projected_itinerary_thermal_state(
        starting_minute,
        outbound_itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        has_tent,
    )?;
    let recovery_start = starting_minute.saturating_add(outbound_itinerary.total_elapsed_minutes);
    let recovery = projected_stationary_field_thermal_state(
        recovery_start,
        recovery_minutes,
        destination,
        outbound.state,
        insulation_bps,
        has_tent,
    )?;
    let action_start = recovery_start.saturating_add(recovery_minutes);
    let action = projected_stationary_outdoor_thermal_state(
        action_start,
        action_minutes,
        destination,
        recovery.state,
        insulation_bps,
    )?;
    let returned = projected_itinerary_thermal_state(
        action_start.saturating_add(action_minutes),
        return_itinerary,
        destination,
        origin,
        action.state,
        insulation_bps,
        has_tent,
    )?;
    Some(outbound.safe && recovery.safe && action.safe && returned.safe)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicJourneyCampState {
    BetweenCamps,
    ActiveCamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostRestProgress {
    Exact { actual_rest_minutes: u64 },
    InterruptedShort { actual_rest_minutes: u64 },
    TerminalBoundary { actual_rest_minutes: u64 },
}

impl PostRestProgress {
    fn actual_rest_minutes(self) -> u64 {
        match self {
            Self::Exact {
                actual_rest_minutes,
            }
            | Self::InterruptedShort {
                actual_rest_minutes,
            }
            | Self::TerminalBoundary {
                actual_rest_minutes,
            } => actual_rest_minutes,
        }
    }
}

fn classify_post_rest_progress(
    before_completed_elapsed: u64,
    requested_rest_minutes: u64,
    after_completed_elapsed: u64,
    after_total_elapsed: u64,
    interrupted: bool,
    terminal_state_change: bool,
) -> Result<PostRestProgress, &'static str> {
    if after_completed_elapsed > after_total_elapsed {
        return Err("post_rest_completed_after_total");
    }
    let actual_rest_minutes = after_completed_elapsed
        .checked_sub(before_completed_elapsed)
        .ok_or("post_rest_progress_regressed")?;
    if actual_rest_minutes > requested_rest_minutes {
        return Err("post_rest_overshot_request");
    }
    if terminal_state_change {
        return Ok(PostRestProgress::TerminalBoundary {
            actual_rest_minutes,
        });
    }
    if actual_rest_minutes == 0 {
        return Err("post_rest_zero_progress");
    }
    if actual_rest_minutes < requested_rest_minutes {
        return if interrupted {
            Ok(PostRestProgress::InterruptedShort {
                actual_rest_minutes,
            })
        } else {
            Err("post_rest_short_without_interruption")
        };
    }
    Ok(PostRestProgress::Exact {
        actual_rest_minutes,
    })
}

fn public_alive_to_dead_ids(before: &[(u64, bool)], after: &[(u64, bool)]) -> Vec<u64> {
    let mut deaths = before
        .iter()
        .filter(|(_, alive)| *alive)
        .filter_map(|(character_id, _)| {
            after
                .iter()
                .find(|(after_id, _)| after_id == character_id)
                .is_some_and(|(_, alive)| !*alive)
                .then_some(*character_id)
        })
        .collect::<Vec<_>>();
    deaths.sort_unstable();
    deaths
}

fn public_terminal_rest_elapsed(
    terminal_ids: &[u64],
    before: &[(u64, u64)],
    after: &[(u64, u64)],
) -> Option<u64> {
    terminal_ids
        .iter()
        .map(|character_id| {
            let before_elapsed = before
                .iter()
                .find(|(before_id, _)| before_id == character_id)?
                .1;
            let after_elapsed = after
                .iter()
                .find(|(after_id, _)| after_id == character_id)?
                .1;
            after_elapsed.checked_sub(before_elapsed)
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min()
}

fn classify_public_journey_camp_state(
    active_interval_count: usize,
) -> Result<PublicJourneyCampState, &'static str> {
    match active_interval_count {
        0 => Ok(PublicJourneyCampState::BetweenCamps),
        1 => Ok(PublicJourneyCampState::ActiveCamp),
        _ => Err("overlapping_active_public_camps"),
    }
}

fn classify_post_encounter_journey(
    state: PublicPostEncounterJourneyState,
) -> Result<PostEncounterJourneyAction, &'static str> {
    if state.unresolved_encounter || !state.active_destination {
        return Ok(PostEncounterJourneyAction::ReclassifyPublicState);
    }
    if state.journey_count != 1 || !state.destination_matches {
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

fn simulation_elapsed_minutes(starting_minute: u64, current_minute: u64) -> u64 {
    current_minute.saturating_sub(starting_minute)
}

fn public_effective_inventory_quantity(quantity: u32, fraction_micros: Option<u32>) -> f32 {
    fraction_micros.map_or(quantity as f32, |value| {
        adventuresim_core::inventory_measurement::ConsumableFractionMicros::try_new(value)
            .expect("public consumable fraction must not exceed one whole")
            .as_unit_f32()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NarrativeEncounterPolicyChoice {
    choice: String,
    reason: &'static str,
    visible_alternatives: Vec<String>,
    eligible_meaningful_alternatives: Vec<String>,
}

fn narrative_axis_fit(
    profile: &AgentProfile,
    development: &adventuresim_core::road_encounter_catalog::PersonalityDevelopment,
) -> i32 {
    use adventuresim_core::road_encounter_catalog::PersonalityAxisId;
    let preferred_sign = match development.axis {
        PersonalityAxisId::Nerve => match profile.personality.nerve {
            Nerve::Brave => 1,
            Nerve::Fearful => -1,
            Nerve::Neutral => 0,
        },
        PersonalityAxisId::Drive => match profile.personality.drive {
            Drive::Ambitious => 1,
            Drive::Content => -1,
            Drive::Neutral => 0,
        },
        PersonalityAxisId::Sociability => match profile.personality.sociability {
            Sociability::Gregarious => 1,
            Sociability::Solitary => -1,
            Sociability::Neutral => 0,
        },
        PersonalityAxisId::Conscience => match profile.personality.conscience {
            Conscience::Compassionate => 1,
            Conscience::Callous | Conscience::Cruel => -1,
            Conscience::Neutral => 0,
        },
        PersonalityAxisId::SelfRegard => match profile.personality.self_regard {
            SelfRegard::Proud => 1,
            SelfRegard::Humble => -1,
            SelfRegard::Neutral => 0,
        },
        PersonalityAxisId::Conviction => match profile.personality.conviction {
            Conviction::Zealous => 1,
            Conviction::Irreverent => -1,
            Conviction::Neutral => 0,
        },
        PersonalityAxisId::Transparency => match profile.personality.transparency {
            Transparency::Open => 1,
            Transparency::Guarded => -1,
            Transparency::Neutral => 0,
        },
        PersonalityAxisId::Courtship => 0,
    };
    preferred_sign * i32::from(development.delta.signum())
}

fn select_public_narrative_encounter_choice(
    presentation_json: &str,
    profile: &AgentProfile,
) -> Result<Option<NarrativeEncounterPolicyChoice>, serde_json::Error> {
    let presentation: adventuresim_core::road_encounter_catalog::EncounterPresentation =
        serde_json::from_str(presentation_json)?;
    let mut visible_alternatives = presentation
        .choices
        .iter()
        .filter(|choice| choice.available)
        .map(|choice| choice.id.clone())
        .collect::<Vec<_>>();
    visible_alternatives.sort();
    let ignore = visible_alternatives.iter().any(|choice| choice == "ignore");
    let mut meaningful = presentation
        .choices
        .iter()
        .filter(|choice| choice.available && choice.id != "ignore")
        .filter_map(|presented| {
            let mut authored = adventuresim_core::road_encounter_catalog::definitions()
                .iter()
                .flat_map(|definition| definition.choices.iter())
                .filter(|choice| choice.id == presented.id);
            let choice = authored.next()?;
            if authored.next().is_some()
                || !choice.checks.is_empty()
                || matches!(
                    choice.transition.as_ref(),
                    Some(adventuresim_core::road_encounter_catalog::EncounterTransition::StartCombat { .. })
                )
            {
                return None;
            }
            let personality_fit = choice
                .personality
                .iter()
                .map(|development| narrative_axis_fit(profile, development))
                .sum::<i32>();
            // Public availability and authored requirements prove only that
            // a choice is legal. They do not establish that spending its
            // resources is preferable to continuing safely.
            (personality_fit > 0).then_some((presented.id.clone(), personality_fit))
        })
        .collect::<Vec<_>>();
    meaningful.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let eligible_meaningful_alternatives = meaningful
        .iter()
        .map(|(choice, _)| choice.clone())
        .collect::<Vec<_>>();
    if let Some((choice, _)) = meaningful.into_iter().next() {
        return Ok(Some(NarrativeEncounterPolicyChoice {
            choice,
            reason: "personality_aligned_check_free_noncombat",
            visible_alternatives,
            eligible_meaningful_alternatives,
        }));
    }
    Ok(ignore.then_some(NarrativeEncounterPolicyChoice {
        choice: "ignore".into(),
        reason: "unconditional_check_free_noncombat_fallback",
        visible_alternatives,
        eligible_meaningful_alternatives,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PublicCombatFingerprint {
    members: Vec<PublicCombatMemberFingerprint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PublicCombatMemberFingerprint {
    character_id: u64,
    melee: bool,
    ranged: bool,
    armored: bool,
    precise: bool,
    endurance_centipoints: u32,
    athletics_centipoints: u32,
    weapon_precision_centipoints: u32,
    autoresolve_combat_power: u64,
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

fn public_contract_assessment(
    difficulty: i32,
    opposition_count: u32,
    opposition_combat_power: u64,
    members: &[PublicPartyCombatant],
) -> PublicContractAssessment {
    let Some(enemy_count) = (opposition_count > 0).then_some(opposition_count) else {
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
    const SKILL_CENTIPOINTS_PER_POINT: f32 = 100.0;

    capabilities.sort_by_key(|row| row.character_id);
    PublicCombatFingerprint {
        members: capabilities
            .into_iter()
            .map(|row| PublicCombatMemberFingerprint {
                character_id: row.character_id,
                melee: row.melee,
                ranged: row.ranged,
                armored: row.heavy || row.half_armor,
                precise: row.precise,
                endurance_centipoints: (row.endurance.max(0.0) * SKILL_CENTIPOINTS_PER_POINT)
                    .round() as u32,
                athletics_centipoints: (row.athletics.max(0.0) * SKILL_CENTIPOINTS_PER_POINT)
                    .round() as u32,
                weapon_precision_centipoints: (row.weapon_precision.max(0.0)
                    * SKILL_CENTIPOINTS_PER_POINT)
                    .round() as u32,
                autoresolve_combat_power: row.autoresolve_combat_power,
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
    hours.clamp(0.0, 100_000.0).round() as u32
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
    let progress = match projected_investigation_action_state(&action.availability) {
        ProjectedInvestigationActionState::Available => 3,
        ProjectedInvestigationActionState::Travel => 2,
        ProjectedInvestigationActionState::Wait(_) => 1,
        ProjectedInvestigationActionState::Blocked => 0,
    };
    let wait_minutes = match &action.availability {
        InvestigationActionAvailability::Available => 0,
        InvestigationActionAvailability::Unavailable(unavailable) => unavailable.wait_minutes,
    };
    (
        progress,
        generated_method_skill_fit(profile, &action.method),
        10_000_u16.saturating_sub(action.uncertainty_bps),
        u32::MAX.saturating_sub(action.duration_max_minutes),
        u32::MAX.saturating_sub(wait_minutes),
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

fn settlement_action_service_label(service: DomainSettlementActionService) -> &'static str {
    match service {
        DomainSettlementActionService::Inn => "inn",
        DomainSettlementActionService::Temple => "temple",
    }
}

fn settlement_service_key(service: SettlementService) -> &'static str {
    match service {
        SettlementService::GeneralStore => "GeneralStore",
        SettlementService::Inn => "Inn",
        SettlementService::GeneralBlacksmith => "GeneralBlacksmith",
        SettlementService::Market => "Market",
        SettlementService::Weaponsmith => "Weaponsmith",
        SettlementService::Armorer => "Armorer",
        SettlementService::Tailor => "Tailor",
        SettlementService::Herbalist => "Herbalist",
        SettlementService::Temple => "Temple",
        SettlementService::Bookstore => "Bookstore",
    }
}

fn death_cause_key(cause: DeathCause) -> &'static str {
    match cause {
        DeathCause::Combat => "Combat",
        DeathCause::Injury => "Injury",
        DeathCause::Disease => "Disease",
        DeathCause::RespiratoryFailure => "RespiratoryFailure",
        DeathCause::CirculatoryFailure => "CirculatoryFailure",
        DeathCause::HomeostaticFailure => "HomeostaticFailure",
        DeathCause::NeurologicFailure => "NeurologicFailure",
        DeathCause::Starvation => "Starvation",
        DeathCause::Dehydration => "Dehydration",
        DeathCause::Other => "Other",
        DeathCause::DevTest => "DevTest",
    }
}

fn activity_preference_key(preference: ActivityPreference) -> &'static str {
    match preference {
        ActivityPreference::Labor => "Labor",
        ActivityPreference::Prayer => "Prayer",
        ActivityPreference::Thievery => "Thievery",
        ActivityPreference::Raiding => "Raiding",
    }
}

fn stdb_settlement_action_service(
    service: DomainSettlementActionService,
) -> adventuresim_stdb_client::SettlementActionService {
    match service {
        DomainSettlementActionService::Inn => {
            adventuresim_stdb_client::SettlementActionService::Inn
        }
        DomainSettlementActionService::Temple => {
            adventuresim_stdb_client::SettlementActionService::Temple
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
) -> Option<DomainSettlementActionService> {
    if temple_available && temple_food_covers_day {
        return Some(DomainSettlementActionService::Temple);
    }
    if inn_available && inn_cost.is_some_and(|cost| purse >= committed_reserve.saturating_add(cost))
    {
        return Some(DomainSettlementActionService::Inn);
    }
    None
}

fn select_generated_travel_action<'a>(
    profile: &AgentProfile,
    actions: &'a mut [BackendInvestigationAction],
    mut action_safe: impl FnMut(&BackendInvestigationAction) -> bool,
) -> Option<&'a BackendInvestigationAction> {
    sort_generated_actions(profile, actions);
    actions.iter().find(|action| {
        projected_investigation_action_state(&action.availability)
            == ProjectedInvestigationActionState::Travel
            && action_safe(action)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FixturePartyIdentity {
    leader_id: u64,
    party_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FixturePartyCandidate {
    identity: FixturePartyIdentity,
    assessment: PublicContractAssessment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FixturePartySelection {
    direct: FixturePartyIdentity,
    generated: FixturePartyIdentity,
}

fn select_strongest_fixture_party(
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
enum FixtureQuestLane {
    Direct,
    Generated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FixtureLanePlan {
    direct_contract_id: String,
    generated_case_id: Option<String>,
    direct_leader_id: u64,
    generated_leader_id: u64,
    direct_party_id: String,
    generated_party_id: String,
}

fn fixture_quest_lane(
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct QuestDecisionObservation<'a> {
    cycle: u32,
    wants_quest: bool,
    selector: f64,
    quest_propensity: f32,
    settlement_id: Option<&'a str>,
    offered_contracts: usize,
    safe_offered_contracts: usize,
    open_generated_cases: usize,
    projected_investigation_actions: usize,
    quest_path: &'a str,
    quest_intended: bool,
    quest_selected: bool,
    selection_reason: &'a str,
}

fn format_quest_decision_detail(decision: QuestDecisionObservation<'_>) -> String {
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
    kind: PersistedItemKind,
) -> adventuresim_core::settlement_economy::CatalogKind {
    use adventuresim_core::settlement_economy::CatalogKind as Catalog;
    match kind {
        PersistedItemKind::Simple | PersistedItemKind::Container => Catalog::Simple,
        PersistedItemKind::Weapon => Catalog::Weapon,
        PersistedItemKind::Armor => Catalog::Armor,
        PersistedItemKind::Shield => Catalog::Shield,
        PersistedItemKind::Clothing => Catalog::Clothing,
        PersistedItemKind::Currency => Catalog::Currency,
        PersistedItemKind::Ingredient => Catalog::Ingredient,
        PersistedItemKind::Medication => Catalog::Medication,
        PersistedItemKind::Food => Catalog::Food,
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

fn visible_unique_default_provider(
    providers: &[(u64, u16, u16, bool, bool)],
    minute: u64,
) -> Option<u64> {
    let [(provider, start_minute, end_minute, context_suppressed, health_suppressed)] = providers
    else {
        return None;
    };
    npc_is_publicly_present(
        *start_minute,
        *end_minute,
        *context_suppressed,
        *health_suppressed,
        minute,
    )
    .then_some(*provider)
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

const PUBLIC_DISCOVERY_BACKOFF_MINUTES: u64 = 2 * MINUTES_PER_DAY;

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
        Some(age) if age < MINUTES_PER_DAY => "under_1_day",
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

fn public_discovery_referral_to_follow(
    owner_character_id: u64,
    before: &HashMap<String, PublicDiscoveryReferral>,
    open_cases: &HashSet<String>,
    after: impl IntoIterator<Item = PublicDiscoveryReferral>,
) -> Option<PublicDiscoveryReferral> {
    let mut newest_changed: Option<PublicDiscoveryReferral> = None;
    let mut newest_unresolved: Option<PublicDiscoveryReferral> = None;
    for lead in after.into_iter().filter(|lead| {
        lead.owner_character_id == owner_character_id
            && !lead.case_id.is_empty()
            && !lead.witness_name.is_empty()
            && lead.corrected_by.is_empty()
    }) {
        let later_than_changed = newest_changed.as_ref().is_none_or(|current| {
            (lead.recorded_at, &lead.lead_id) > (current.recorded_at, &current.lead_id)
        });
        let later_than_unresolved = newest_unresolved.as_ref().is_none_or(|current| {
            (lead.recorded_at, &lead.lead_id) > (current.recorded_at, &current.lead_id)
        });
        if before.get(&lead.lead_id) != Some(&lead) && later_than_changed {
            newest_changed = Some(lead.clone());
        }
        if !open_cases.contains(&lead.case_id) && later_than_unresolved {
            newest_unresolved = Some(lead);
        }
    }
    newest_changed.or(newest_unresolved)
}

#[derive(Clone, Debug, PartialEq)]
struct PublicDialogueProgressFingerprint {
    cases: Vec<(String, String)>,
    leads: Vec<PublicDialogueLeadSemantic>,
    actions: Vec<PublicDialogueActionSemantic>,
    outcomes: Vec<(String, String)>,
    sites: Vec<(String, CoreDestinationKnowledgeStage, bool, bool, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PublicDialogueLeadSemantic {
    summary: String,
    source_label: String,
    confidence_bps: u16,
    destination_stage: CoreDestinationKnowledgeStage,
    directions: String,
    exact_location_id: String,
    latitude_e7: i32,
    longitude_e7: i32,
    witness_name: String,
    witness_description: String,
    witness_occupation_or_relationship: String,
    expected_location: String,
    current_learned_location: String,
    contradiction_group: String,
    corrected_by: String,
}

#[derive(Clone, Debug, PartialEq)]
struct PublicDialogueActionSemantic {
    action_id: String,
    method: String,
    summary: String,
    known_prerequisites: String,
    duration_min_minutes: u32,
    duration_max_minutes: u32,
    uncertainty_bps: u16,
    skill_contributions: String,
    weather_available: bool,
    required_case_site_id: Option<CaseSiteId>,
    availability: InvestigationActionAvailability,
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

fn npc_is_publicly_present(
    start_minute: u16,
    end_minute: u16,
    context_suppressed: bool,
    health_suppressed: bool,
    minute: u64,
) -> bool {
    if context_suppressed || health_suppressed {
        return false;
    }
    let minute = minute % MINUTES_PER_DAY;
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
            candidate.resident_character_id,
        )
    });
    candidates
}

fn stable_owned_open_cases(
    owner_character_id: u64,
    rows: impl IntoIterator<Item = (u64, String, String, DomainCaseStatus, u64)>,
) -> Vec<(String, String)> {
    let mut cases = rows
        .into_iter()
        .filter(|(owner, _, _, status, _)| {
            *owner == owner_character_id && *status == DomainCaseStatus::Open
        })
        .map(|(_, case_id, title, _, latest_update_at)| (latest_update_at, case_id, title))
        .collect::<Vec<_>>();
    cases.sort();
    cases
        .into_iter()
        .map(|(_, case_id, title)| (case_id, title))
        .collect()
}

fn fair_open_case_index(
    cases: &[(String, String)],
    active_case_id: Option<&str>,
    active_is_actionable: bool,
    cursor_case_id: Option<&str>,
) -> usize {
    if active_is_actionable
        && let Some(index) = active_case_id
            .and_then(|active| cases.iter().position(|(case_id, _)| case_id == active))
    {
        return index;
    }
    cursor_case_id
        .and_then(|cursor| cases.iter().position(|(case_id, _)| case_id == cursor))
        .map_or(0, |index| (index + 1) % cases.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedClosureAttribution {
    StillOpen,
    OwnImmediateTransition,
    ExternalTransition,
}

fn generated_closure_attribution(
    before_status: DomainCaseStatus,
    after_status: Option<DomainCaseStatus>,
    immediately_after_own_action: bool,
) -> GeneratedClosureAttribution {
    if before_status == DomainCaseStatus::Open && after_status == Some(DomainCaseStatus::Resolved) {
        if immediately_after_own_action {
            GeneratedClosureAttribution::OwnImmediateTransition
        } else {
            GeneratedClosureAttribution::ExternalTransition
        }
    } else {
        GeneratedClosureAttribution::StillOpen
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedCaseIntakeSource {
    OwnerProjectionContinuation,
    DialogueRumor,
}

impl GeneratedCaseIntakeSource {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::OwnerProjectionContinuation => "owner_projection_continuation",
            Self::DialogueRumor => "dialogue_rumor",
        }
    }

    const fn is_continuation(self) -> bool {
        matches!(self, Self::OwnerProjectionContinuation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedDialoguePurpose {
    Discovery,
    Case,
}

impl GeneratedDialoguePurpose {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Discovery => "discover",
            Self::Case => "case",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeneratedDialogueTopic {
    ReferredTestimony,
    ReturnRecoveredProperty,
    ExposeFalseAccount,
}

impl GeneratedDialogueTopic {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::ReferredTestimony => "referred-testimony",
            Self::ReturnRecoveredProperty => "return-recovered-property",
            Self::ExposeFalseAccount => "expose-false-account",
        }
    }

    fn from_stable_id(value: &str) -> Option<Self> {
        match value {
            "referred-testimony" => Some(Self::ReferredTestimony),
            "return-recovered-property" => Some(Self::ReturnRecoveredProperty),
            "expose-false-account" => Some(Self::ExposeFalseAccount),
            _ => None,
        }
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
    occupied_site_id: &CaseSiteId,
    pin_owner_character_id: u64,
    pin_public_case_id: &str,
    pin_site_id: &CaseSiteId,
) -> bool {
    projected_case_row_matches(
        owner_character_id,
        selected_case_id,
        pin_owner_character_id,
        pin_public_case_id,
    ) && pin_site_id == occupied_site_id
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectedInvestigationActionState {
    Available,
    Travel,
    Wait(u32),
    Blocked,
}

fn projected_investigation_action_state(
    availability: &InvestigationActionAvailability,
) -> ProjectedInvestigationActionState {
    match availability {
        InvestigationActionAvailability::Available => ProjectedInvestigationActionState::Available,
        InvestigationActionAvailability::Unavailable(unavailable)
            if unavailable.can_travel_to_required_site =>
        {
            ProjectedInvestigationActionState::Travel
        }
        InvestigationActionAvailability::Unavailable(unavailable) => {
            projected_investigation_wait_minutes(unavailable.reason, unavailable.wait_minutes)
                .map_or(ProjectedInvestigationActionState::Blocked, |minutes| {
                    ProjectedInvestigationActionState::Wait(minutes)
                })
        }
    }
}

fn projected_investigation_wait_minutes(
    reason: InvestigationActionUnavailableReason,
    wait_minutes: u32,
) -> Option<u32> {
    match reason {
        InvestigationActionUnavailableReason::NightWindow
        | InvestigationActionUnavailableReason::ContactScheduleWindow => (1
            ..=MAX_PROJECTED_INVESTIGATION_WAIT_MINUTES)
            .contains(&wait_minutes)
            .then_some(wait_minutes),
        InvestigationActionUnavailableReason::PartyNotReady
        | InvestigationActionUnavailableReason::TravelRequired
        | InvestigationActionUnavailableReason::TargetChanged
        | InvestigationActionUnavailableReason::ContactNotPresent
        | InvestigationActionUnavailableReason::CharacterUnavailable
        | InvestigationActionUnavailableReason::PartyRequired => None,
    }
}

fn current_contact_schedule_wait_minutes(
    action: &BackendInvestigationAction,
    presences: impl IntoIterator<Item = SettlementResidentPresence>,
    actor_minute: u64,
) -> Option<u32> {
    let contact_character_id = action.contact_character_id?;
    let presence = presences
        .into_iter()
        .find(|presence| presence.character_id == contact_character_id)?;
    if presence.context_suppressed || presence.health_suppressed {
        return None;
    }
    DailyPresenceWindow {
        start_minute: presence.start_minute,
        end_minute: presence.end_minute,
    }
    .minutes_until_start(actor_minute)
    .ok()
}

fn investigation_unavailable_reason_key(
    reason: InvestigationActionUnavailableReason,
) -> &'static str {
    match reason {
        InvestigationActionUnavailableReason::PartyNotReady => "party_not_ready",
        InvestigationActionUnavailableReason::TravelRequired => "travel_required",
        InvestigationActionUnavailableReason::NightWindow => "night_window",
        InvestigationActionUnavailableReason::TargetChanged => "target_changed",
        InvestigationActionUnavailableReason::ContactScheduleWindow => "contact_schedule_window",
        InvestigationActionUnavailableReason::ContactNotPresent => "contact_not_present",
        InvestigationActionUnavailableReason::CharacterUnavailable => "character_unavailable",
        InvestigationActionUnavailableReason::PartyRequired => "party_required",
    }
}

fn dialogue_contact_presence_changed(error: &CoreLoopError) -> bool {
    error.reducer_code() == Some(ReducerErrorCode::DialogueContactUnavailable)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InvestigationActionReplanReason {
    Unavailable,
    Stale,
}

impl InvestigationActionReplanReason {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
        }
    }
}

fn investigation_action_replan_reason(
    error: &CoreLoopError,
) -> Option<InvestigationActionReplanReason> {
    match error.reducer_code()? {
        ReducerErrorCode::InvestigationActionUnavailable => {
            Some(InvestigationActionReplanReason::Unavailable)
        }
        ReducerErrorCode::InvestigationActionStale => Some(InvestigationActionReplanReason::Stale),
        _ => None,
    }
}

fn projected_case_site_journey_minutes(
    distance_m: u64,
    walking_minutes_per_day: u16,
) -> Option<u64> {
    if distance_m == 0
        || walking_minutes_per_day == 0
        || u64::from(walking_minutes_per_day) > MINUTES_PER_DAY
    {
        return None;
    }
    let movement_minutes = case_site_movement_minutes(distance_m)?;
    let walking_minutes = u64::from(walking_minutes_per_day);
    let completed_walking_days = movement_minutes.saturating_sub(1) / walking_minutes;
    Some(
        movement_minutes
            .saturating_add(
                completed_walking_days
                    .saturating_mul(MINUTES_PER_DAY.saturating_sub(walking_minutes)),
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
enum TravelProvisionDeferralReason {
    ContributionRevalidationFailed,
    EssentialsUnaffordable,
    EssentialsUnavailable,
    FinanceBackoff,
    PayerProviderProjectionUnavailable,
    RequiresSettlement,
}

impl TravelProvisionDeferralReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ContributionRevalidationFailed => "journey_contribution_revalidation_failed",
            Self::EssentialsUnaffordable => "journey_essentials_unaffordable",
            Self::EssentialsUnavailable => "journey_essentials_unavailable",
            Self::FinanceBackoff => "journey_finance_backoff",
            Self::PayerProviderProjectionUnavailable => {
                "journey_payer_provider_projection_unavailable"
            }
            Self::RequiresSettlement => "provisioning_requires_settlement",
        }
    }
}

impl std::fmt::Display for TravelProvisionDeferralReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TravelProvisionDecision {
    Ready,
    Deferred(TravelProvisionDeferralReason),
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

#[derive(Clone, Copy, Debug)]
struct ActivityPlanDiagnostic<'a> {
    preferred_activity: &'a str,
    effective_activity: &'a str,
    schedule: &'a ScheduleAllocation,
    fallback_reason: &'a str,
    committed_reserve: u64,
}

#[derive(Clone, Copy, Debug)]
struct ActivityExecutionDiagnostic<'a> {
    plan: ActivityPlanDiagnostic<'a>,
    venue: DomainSettlementActionService,
}

fn format_activity_detail(
    diagnostic: ActivityExecutionDiagnostic<'_>,
    before: &ActivityObservation,
    after: &ActivityObservation,
) -> String {
    let ActivityExecutionDiagnostic {
        plan:
            ActivityPlanDiagnostic {
                preferred_activity,
                effective_activity,
                schedule,
                fallback_reason,
                committed_reserve,
            },
        venue,
    } = diagnostic;
    format!(
        "outcome=completed;preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue={};committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};purse_before={};purse_after={};purse_delta={};condition_before={};condition_after={};hunger_before={:.3};hunger_after={:.3};hunger_delta={};thirst_before={:.3};thirst_after={:.3};thirst_delta={};food_kcal_before={:.0};food_kcal_after={:.0};food_kcal_delta={};water_ml_before={:.0};water_ml_after={:.0};water_ml_delta={};elapsed_before={};elapsed_after={};elapsed_delta={}",
        settlement_action_service_label(venue),
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
    diagnostic: ActivityExecutionDiagnostic<'_>,
    before: &ActivityObservation,
    error_category: &str,
) -> String {
    let ActivityExecutionDiagnostic {
        plan:
            ActivityPlanDiagnostic {
                preferred_activity,
                effective_activity,
                schedule,
                fallback_reason,
                committed_reserve,
            },
        venue,
    } = diagnostic;
    format!(
        "outcome=failed;stage=rest_at_settlement;error_category={error_category};preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue={};committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};requested_minutes={MINUTES_PER_DAY};purse_before={};condition_before={};hunger_before={:.3};thirst_before={:.3};food_kcal_before={:.0};water_ml_before={:.0};elapsed_before={}",
        settlement_action_service_label(venue),
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

fn format_deferred_activity_detail(
    diagnostic: ActivityPlanDiagnostic<'_>,
    before: &ActivityObservation,
) -> String {
    let ActivityPlanDiagnostic {
        preferred_activity,
        effective_activity,
        schedule,
        fallback_reason,
        committed_reserve,
    } = diagnostic;
    format!(
        "outcome=deferred;reason=insufficient_visible_resources;preferred={preferred_activity};effective={effective_activity};fallback={fallback_reason};venue=unavailable;committed_reserve={committed_reserve};schedule=combat:{},carousing:{},apprenticeship:{},profession:{},labor:{},prayer:{},thievery:{},raiding:{};purse_before={};condition_before={};hunger_before={:.3};thirst_before={:.3};food_kcal_before={:.0};water_ml_before={:.0};elapsed_before={}",
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
