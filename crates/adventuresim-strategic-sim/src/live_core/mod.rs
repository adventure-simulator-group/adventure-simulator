//! Opt-in reducer-backed core-loop simulation.
//!
//! Storage, policy, failure projection, travel, settlement, expedition,
//! generated-case, cycle, and bootstrap behavior are kept in focused
//! source units while retaining the established crate-root API.

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
#[cfg(test)]
pub(crate) const LIVE_CORE_SOURCE: &str = concat!(
    include_str!("config.rs"),
    include_str!("events.rs"),
    include_str!("report.rs"),
    include_str!("quest_coverage.rs"),
    include_str!("departure_policy.rs"),
    include_str!("thermal_projection.rs"),
    include_str!("expedition_policy.rs"),
    include_str!("journey_policy.rs"),
    include_str!("encounter_policy.rs"),
    include_str!("fixture_policy.rs"),
    include_str!("discovery_policy.rs"),
    include_str!("investigation_policy.rs"),
    include_str!("settlement_activity.rs"),
    include_str!("error.rs"),
    include_str!("failure_projection.rs"),
    include_str!("policy.rs"),
    include_str!("survival.rs"),
    include_str!("failure.rs"),
    include_str!("travel.rs"),
    include_str!("settlement.rs"),
    include_str!("expedition.rs"),
    include_str!("generated_cases.rs"),
    include_str!("cycle.rs"),
    include_str!("bootstrap.rs"),
    include_str!("tests.rs"),
);

mod config;
mod departure_policy;
mod discovery_policy;
mod encounter_policy;
mod events;
mod expedition_policy;
mod fixture_policy;
mod investigation_policy;
mod journey_policy;
mod quest_coverage;
mod report;
mod settlement_activity;
mod thermal_projection;

use config::*;
use departure_policy::*;
use discovery_policy::*;
use encounter_policy::*;
use events::*;
use expedition_policy::*;
use fixture_policy::*;
use investigation_policy::*;
use journey_policy::*;
use settlement_activity::*;
use thermal_projection::*;

pub use config::CoreLoopConfig;
pub use events::{CoreLoopEvent, CoreLoopEventKind};
pub(crate) use investigation_policy::balanced_party_groups;
pub use quest_coverage::{
    QuestCoverageEvidence, QuestCoverageFailure, QuestCoverageMetric, validate_quest_coverage,
    write_quest_coverage_failure,
};
pub use report::{CoreLoopMetrics, CoreLoopReport, FinalAgentState};
include!("error.rs");
include!("failure_projection.rs");
include!("policy.rs");
include!("survival.rs");

mod schema_types;
use schema_types::{
    domain_body_region, domain_incapacitation_status, reducer_intervention_route,
    reducer_surgery_procedure,
};

mod failure {
    use super::*;
    include!("failure.rs");
}

mod travel {
    use super::*;
    include!("travel.rs");
}

mod settlement {
    use super::*;
    include!("settlement.rs");
}

mod expedition {
    use super::*;
    include!("expedition.rs");
}

mod generated_cases {
    use super::*;
    include!("generated_cases.rs");
}

mod cycle {
    use super::*;
    include!("cycle.rs");
}

mod bootstrap {
    use super::*;
    include!("bootstrap.rs");
}

pub use bootstrap::run_core_loop;
use bootstrap::{equipment_utility, leader_is_actionable, root_requirement_matches_slot};
#[cfg(test)]
use bootstrap::{select_public_quest_fixture, select_public_quest_fixture_if_present};
include!("tests.rs");
