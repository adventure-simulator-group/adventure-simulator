//! Opt-in reducer-backed core-loop simulation.
//!
//! Unlike the native balance runner, this backend owns a disposable local
//! SpacetimeDB database and deliberately delegates every game rule to the
//! normal strategic reducers.

use crate::{AgentProfile, EquipmentStyle, generate_profile};
use adventuresim_core::simulation_security::{
    SIM_BOOTSTRAP_TOKEN_ENV as BOOTSTRAP_TOKEN_ENV,
    SIM_BOOTSTRAP_TOKEN_HEX_LEN as BOOTSTRAP_TOKEN_HEX_LEN,
};
use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table};
use adventuresim_stdb_client::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
    time::Duration,
};

use adventuresim_core::strategic_currency::is_currency_id;
use url::Url;

use adventuresim_stdb_client::{
    abandon_contract_reducer::abandon_contract, accept_contract_reducer::accept_contract,
    accept_party_join_request_reducer::accept_party_join_request,
    autoresolve_mission_reducer::autoresolve_mission,
    autoresolve_report_table::AutoresolveReportTableAccess,
    backend_case_site_pins_table::BackendCaseSitePinsTableAccess,
    backend_contract_type::BackendContract, backend_contracts_table::BackendContractsTableAccess,
    backend_herbalist_examinations_table::BackendHerbalistExaminationsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_death_table::CharacterDeathTableAccess,
    character_equip_table::CharacterEquipTableAccess,
    character_illness_status_table::CharacterIllnessStatusTableAccess,
    character_strategic_condition_table::CharacterStrategicConditionTableAccess,
    character_table::CharacterTableAccess, character_time_table::CharacterTimeTableAccess,
    character_training_schedule_table::CharacterTrainingScheduleTableAccess,
    claim_simulation_run_reducer::claim_simulation_run,
    configure_simulation_character_reducer::configure_simulation_character,
    continue_camp_travel_reducer::continue_camp_travel,
    contract_interaction_stage_type::ContractInteractionStage,
    contract_status_type::ContractStatus, craft_medication_reducer::craft_medication,
    create_named_character_with_id_reducer::create_named_character_with_id,
    dismiss_herbalist_examination_reducer::dismiss_herbalist_examination,
    ensure_settlement_activity_reducer::ensure_settlement_activity, equip_item_reducer::equip_item,
    equip_medication_reducer::equip_medication,
    equipped_medication_table::EquippedMedicationTableAccess,
    examine_by_herbalist_reducer::examine_by_herbalist,
    finalize_merchant_trade_reducer::finalize_merchant_trade,
    inventory_item_table::InventoryItemTableAccess, item_condition_table::ItemConditionTableAccess,
    item_table::ItemTableAccess, liquidate_party_inventory_reducer::liquidate_party_inventory,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess,
    party_member_table::PartyMemberTableAccess, party_stake_table::PartyStakeTableAccess,
    party_table::PartyTableAccess, purchase_from_herbalist_reducer::purchase_from_herbalist,
    register_strategic_gateway_reducer::register_strategic_gateway,
    repair_order_table::RepairOrderTableAccess, report_contract_reducer::report_contract,
    request_general_party_join_reducer::request_general_party_join,
    resolve_strategic_encounter_reducer::resolve_strategic_encounter,
    rest_at_camp_reducer::rest_at_camp, rest_at_settlement_hours_reducer::rest_at_settlement_hours,
    retrieve_repaired_item_reducer::retrieve_repaired_item,
    seed_simulation_disease_reducer::seed_simulation_disease,
    seed_simulation_equipment_damage_reducer::seed_simulation_equipment_damage,
    seed_simulation_world_reducer::seed_simulation_world,
    settlement_smith_table::SettlementSmithTableAccess,
    simulate_contract_issuer_interaction_reducer::simulate_contract_issuer_interaction,
    simulation_run_table::SimulationRunTableAccess, store_battle_loot_reducer::store_battle_loot,
    strategic_encounter_table::StrategicEncounterTableAccess,
    submit_item_for_repair_reducer::submit_item_for_repair,
    travel_to_case_site_reducer::travel_to_case_site,
    travel_to_settlement_reducer::travel_to_settlement,
    update_training_schedule_reducer::update_training_schedule,
    withdraw_party_inventory_item_reducer::withdraw_party_inventory_item,
};

const ACTION_TIMEOUT: Duration = Duration::from_secs(20);
/// Severe but non-incapacitating injuries can reduce overland pace enough for
/// a long quest leg to require many daily camps.
const MAX_CAMPS_PER_LEG: u32 = 512;
const MAX_DEFEAT_RETRIES: u32 = 2;
/// Natural recovery is one percent per day without medicine, so a character
/// reduced to zero may legitimately need roughly one hundred days.
const MAX_RECOVERY_ACTIONS: u32 = 128;
const MAX_CORE_LOOP_WORK: u64 = 100_000;
const MAX_CORE_TRACE_EVENTS: usize = 100_000;

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
    pub diagnoses_attempted: u32,
    pub diagnoses_confirmed: u32,
    pub medications_crafted: u32,
    pub medications_purchased: u32,
    pub medications_equipped: u32,
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
    Diagnose,
    CraftMedication,
    BuyMedication,
    EquipMedication,
    IllnessRecovered,
    QuestSuppressed,
    Death,
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
    pub equipped_medication_courses: u32,
    pub elapsed_minutes: u64,
    pub personal_gold_coin: u64,
    pub party_treasury: u64,
    pub party_stake: u64,
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
    pub profiles: Vec<AgentProfile>,
    pub metrics: CoreLoopMetrics,
    pub trace: Vec<CoreLoopEvent>,
    pub trace_truncated: bool,
    pub total_event_count: u64,
    pub final_agents: Vec<FinalAgentState>,
    pub elapsed_game_minutes: u64,
    pub policy_seed_note: String,
}

struct LiveRunner {
    connection: DbConnection,
    profiles: Vec<AgentProfile>,
    character_ids: Vec<u64>,
    metrics: CoreLoopMetrics,
    trace: Vec<CoreLoopEvent>,
    sequence: u64,
    last_semantic_event: Option<String>,
    recorded_deaths: HashSet<u64>,
    medically_paused_schedules: HashSet<u64>,
}

const SMITHING_DECISION_SCALE: f32 = 1_000.0;

fn quantize_smithing_condition(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * SMITHING_DECISION_SCALE).round() as u32
}

fn live_attributes(character_id: u64, profile: &AgentProfile) -> CharacterAttributes {
    let a = &profile.attributes;
    CharacterAttributes {
        character_id,
        endurance: a.endurance,
        immunity: a.immunity,
        gut: a.gut,
        precision: a.precision,
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
        self_awareness_hours: s.self_awareness,
        humor_hours: s.humor,
        command_hours: s.command,
        deception_hours: s.deception,
        seduction_hours: s.seduction,
        medicine_hours: s.medicine,
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
        anatomy_hours: s.anatomy,
        tailoring_hours: s.tailoring,
        smithing_hours: s.smithing,
    }
}

fn live_schedule(profile: &AgentProfile) -> ScheduleAllocation {
    let s = profile.schedule;
    ScheduleAllocation {
        combat_training_minutes: s.combat_training_minutes,
        carousing_minutes: s.carousing_minutes,
        apprenticeship_minutes: s.apprenticeship_minutes,
        apprenticeship_service_id: s
            .apprenticeship_service_id
            .map(|id| id.service_id().to_string()),
        profession_practice_minutes: s.profession_practice_minutes,
        profession_service_id: s
            .profession_service_id
            .map(|id| id.service_id().to_string()),
        labor_minutes: s.labor,
        prayer_minutes: s.prayer,
        thievery_minutes: s.thievery,
        raiding_minutes: s.raiding,
    }
}

fn medical_rest_schedule() -> ScheduleAllocation {
    ScheduleAllocation {
        combat_training_minutes: 0,
        carousing_minutes: 0,
        apprenticeship_minutes: 0,
        apprenticeship_service_id: None,
        profession_practice_minutes: 0,
        profession_service_id: None,
        labor_minutes: 0,
        prayer_minutes: 0,
        thievery_minutes: 0,
        raiding_minutes: 0,
    }
}

fn live_personality(character_id: u64, p: &crate::Personality) -> CharacterPersonality {
    CharacterPersonality {
        character_id,
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
    fn event(&mut self, agent_id: u32, kind: CoreLoopEventKind, detail: impl Into<String>) {
        self.sequence += 1;
        let detail = detail.into();
        let semantic = format!("{agent_id}:{kind:?}:{detail}");
        let repeatable = matches!(
            kind,
            CoreLoopEventKind::Camp
                | CoreLoopEventKind::Recover
                | CoreLoopEventKind::Travel
                | CoreLoopEventKind::AutoresolveDefeat
        );
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
    }

    fn call(&mut self, result: Result<(), String>) -> Result<(), String> {
        if result.is_err() {
            self.metrics.reducer_failures += 1;
        }
        result
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
            let result = reducer_call!(self, "rest_at_camp", |cb| self
                .connection
                .reducers
                .rest_at_camp_then(leader, 1_440, cb));
            self.call(result)?;
            self.observe_deaths();
            let Some((leader, agent)) = self.current_leader(party_id) else {
                return Ok(());
            };
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
                format!("remaining_before={remaining_before}"),
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
            let treatment_active = self
                .connection
                .db
                .equipped_medication()
                .iter()
                .any(|row| row.character_id == character_id);
            let symptomatic = self
                .connection
                .db
                .character_illness_status()
                .iter()
                .find(|row| row.character_id == character_id)
                .is_some_and(|row| row.symptomatic);
            if condition.status == "ready" && !treatment_active && !symptomatic {
                self.restore_profile_schedule(agent)?;
                return Ok(true);
            }
            let Some(settlement) = character.current_settlement_id.clone() else {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("status={}", condition.status),
                );
                return Ok(false);
            };

            if treatment_active {
                self.set_medical_rest_schedule(agent)?;
                let result = reducer_call!(self, "ongoing_treatment_rest", |cb| self
                    .connection
                    .reducers
                    .rest_at_settlement_hours_then(character_id, 1_440, false, cb));
                self.call(result)?;
                self.metrics.treatment_rest_minutes += 1_440;
                self.metrics.recovery_rests += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::Recover,
                    "ongoing_treatment_minutes=1440",
                );
                let alive = self
                    .connection
                    .db
                    .character()
                    .iter()
                    .find(|row| row.id == character_id)
                    .is_some_and(|row| row.alive);
                if !alive {
                    continue;
                }
                let ready = self
                    .connection
                    .db
                    .character_strategic_condition()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| row.status == "ready");
                let still_treated = self
                    .connection
                    .db
                    .equipped_medication()
                    .iter()
                    .any(|row| row.character_id == character_id);
                let still_symptomatic = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| row.symptomatic);
                if ready && !still_treated && !still_symptomatic {
                    self.restore_profile_schedule(agent)?;
                    self.metrics.illness_recoveries += 1;
                    self.event(
                        agent,
                        CoreLoopEventKind::IllnessRecovered,
                        "status=ready;treatment=complete",
                    );
                    return Ok(true);
                }
                continue;
            }

            let gold_before = self.personal_gold(character_id);
            let result = reducer_call!(self, "examine_by_herbalist", |cb| self
                .connection
                .reducers
                .examine_by_herbalist_then(character_id, settlement.clone(), cb));
            self.call(result)?;
            self.metrics.diagnoses_attempted += 1;
            let examination = self
                .connection
                .db
                .backend_herbalist_examinations()
                .iter()
                .find(|row| row.patient_id == character_id)
                .ok_or("herbalist examination reducer returned no patient-filtered result")?;
            self.event(
                agent,
                CoreLoopEventKind::Diagnose,
                format!(
                    "possibilities={};confirmed={}",
                    examination.disease_names.len(),
                    examination.medication_names.len()
                ),
            );
            if !examination.medication_names.is_empty() {
                self.metrics.diagnoses_confirmed += 1;
            }
            for medication_name in examination.medication_names.clone() {
                let recipe = adventuresim_core::disease::MEDICATION_RECIPES
                    .iter()
                    .find(|recipe| recipe.name == medication_name)
                    .ok_or_else(|| {
                        format!("herbalist returned unknown medication {medication_name}")
                    })?;
                let capability = self
                    .connection
                    .db
                    .character_capability()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .ok_or("missing medicine capability")?;
                let has_ingredients = recipe.ingredients.iter().all(|ingredient| {
                    self.connection
                        .db
                        .inventory_item()
                        .iter()
                        .filter(|row| {
                            row.character_id == character_id && row.item_id == ingredient.item_id
                        })
                        .map(|row| row.quantity)
                        .sum::<u32>()
                        >= ingredient.quantity
                });
                if adventuresim_core::disease::can_prepare_medication(capability.medicine, recipe)
                    && has_ingredients
                {
                    let disease_id = format!("{:?}", recipe.disease_id).to_ascii_lowercase();
                    let result = reducer_call!(self, "craft_medication", |cb| self
                        .connection
                        .reducers
                        .craft_medication_then(character_id, disease_id.clone(), false, cb));
                    self.call(result)?;
                    self.metrics.medications_crafted += 1;
                    self.event(
                        agent,
                        CoreLoopEventKind::CraftMedication,
                        format!("item={}", recipe.item_id),
                    );
                } else {
                    let result = reducer_call!(self, "purchase_from_herbalist", |cb| self
                        .connection
                        .reducers
                        .purchase_from_herbalist_then(
                            character_id,
                            settlement.clone(),
                            vec![recipe.item_id.into()],
                            vec![1],
                            cb
                        ));
                    self.call(result)?;
                    self.metrics.medications_purchased += 1;
                    self.event(
                        agent,
                        CoreLoopEventKind::BuyMedication,
                        format!("item={}", recipe.item_id),
                    );
                }
                let course = self
                    .connection
                    .db
                    .inventory_item()
                    .iter()
                    .find(|row| row.character_id == character_id && row.item_id == recipe.item_id)
                    .ok_or("medication acquisition produced no course")?;
                let course_id = course.id;
                let result = reducer_call!(self, "equip_medication", |cb| self
                    .connection
                    .reducers
                    .equip_medication_then(character_id, course_id, cb));
                self.call(result)?;
                if !self.connection.db.equipped_medication().iter().any(|row| {
                    row.character_id == character_id && row.inventory_item_id == course_id
                }) {
                    return Err(
                        "medication equip completed without authoritative equipped row".into(),
                    );
                }
                self.metrics.medications_equipped += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::EquipMedication,
                    format!("item={}", recipe.item_id),
                );
            }
            self.set_medical_rest_schedule(agent)?;
            let examination_id = examination.id;
            let result = reducer_call!(self, "dismiss_herbalist_examination", |cb| self
                .connection
                .reducers
                .dismiss_herbalist_examination_then(character_id, examination_id, cb));
            self.call(result)?;
            self.metrics.treatment_gold_spent +=
                gold_before.saturating_sub(self.personal_gold(character_id));

            let result = reducer_call!(self, "medical_recovery_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, false, cb));
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
            let treatment_active = self
                .connection
                .db
                .equipped_medication()
                .iter()
                .any(|row| row.character_id == character_id);
            if status == "ready" && !treatment_active {
                let symptomatic = self
                    .connection
                    .db
                    .character_illness_status()
                    .iter()
                    .find(|row| row.character_id == character_id)
                    .is_some_and(|row| row.symptomatic);
                if symptomatic {
                    continue;
                }
                self.restore_profile_schedule(agent)?;
                self.metrics.illness_recoveries += 1;
                self.event(agent, CoreLoopEventKind::IllnessRecovered, "status=ready");
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
            let result = reducer_call!(self, "settlement_activity_rest", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 1_440, false, cb));
            self.call(result)?;
            self.event(
                agent,
                CoreLoopEventKind::Activity,
                format!(
                    "preferred={:?}",
                    self.profiles[agent as usize].preferred_activity
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
        let mut repair_budget: u64 = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum();

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
        let mut retrieval_budget: u64 = self
            .connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && is_currency_id(&row.item_id))
            .map(|row| u64::from(row.quantity))
            .sum();
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
                let result = reducer_call!(self, "wait_for_repairs", |cb| self
                    .connection
                    .reducers
                    .rest_at_settlement_hours_then(character_id, wait, false, cb));
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
                self.ensure_medically_safe(agent)?;
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
            .find(|site| site.owner_character_id == leader && site.case_id == quest.case_id)
            .ok_or("accepted quest did not disclose an exact case site")?;
        self.event(
            leader_agent,
            CoreLoopEventKind::AcceptContract,
            format!(
                "cycle={cycle};quest={};title={};difficulty={};enemy={}x{};distance_m={}",
                quest.id,
                quest.title,
                quest.difficulty,
                quest.enemy_type,
                quest.enemy_count,
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
        .add_query(|query| query.from.backend_case_site_pins())
        .add_query(|query| query.from.backend_herbalist_examinations())
        .add_query(|query| query.from.battle_loot_item())
        .add_query(|query| query.from.battle_result())
        .add_query(|query| query.from.character())
        .add_query(|query| query.from.character_capability())
        .add_query(|query| query.from.character_death())
        .add_query(|query| query.from.character_equip())
        .add_query(|query| query.from.character_illness_status())
        .add_query(|query| query.from.character_strategic_condition())
        .add_query(|query| query.from.character_time())
        .add_query(|query| query.from.character_training_schedule())
        .add_query(|query| query.from.equipped_medication())
        .add_query(|query| query.from.inventory_item())
        .add_query(|query| query.from.item())
        .add_query(|query| query.from.item_condition())
        .add_query(|query| query.from.party())
        .add_query(|query| query.from.party_inventory_item())
        .add_query(|query| query.from.party_join_request())
        .add_query(|query| query.from.party_member())
        .add_query(|query| query.from.party_stake())
        .add_query(|query| query.from.backend_contracts())
        .add_query(|query| query.from.strategic_encounter())
        .add_query(|query| query.from.repair_order())
        .add_query(|query| query.from.settlement())
        .add_query(|query| query.from.settlement_smith())
        .add_query(|query| query.from.simulation_run())
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
        last_semantic_event: None,
        recorded_deaths: HashSet::new(),
        medically_paused_schedules: HashSet::new(),
    };
    if runner
        .connection
        .db
        .simulation_run()
        .iter()
        .next()
        .is_some()
        || runner.connection.db.character().iter().next().is_some()
        || runner.connection.db.settlement().iter().next().is_some()
    {
        return Err("refusing reused or populated simulation database".into());
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
        .add_query(|query| query.from.backend_case_site_pins())
        .add_query(|query| query.from.backend_herbalist_examinations())
        .add_query(|query| query.from.party())
        .subscribe();
    gateway_subscription_rx
        .recv_timeout(ACTION_TIMEOUT)
        .map_err(|_| "gateway subscription timed out".to_string())??;
    let result = reducer_call!(runner, "seed_simulation_world", |cb| runner
        .connection
        .reducers
        .seed_simulation_world_then(config.run_nonce.clone(), cb));
    runner.call(result)?;
    let mut shared_settlement: Option<String> = None;
    for (agent, character_id) in runner.character_ids.clone().into_iter().enumerate() {
        let name = format!("sim-{}-{agent}", config.seed);
        let result = reducer_call!(runner, "create_named_character_with_id", |cb| runner
            .connection
            .reducers
            .create_named_character_with_id_then(character_id, name.clone(), cb));
        runner.call(result)?;
        let settlement = match &shared_settlement {
            Some(settlement) => settlement.clone(),
            None => {
                let settlement = runner
                    .party_for(character_id)?
                    .current_settlement_id
                    .ok_or("fresh character has no settlement")?;
                shared_settlement = Some(settlement.clone());
                settlement
            }
        };
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

    let duration_minutes = u64::from(config.duration_days) * 1_440;
    for cycle in 0..config.cycles {
        let mut active = false;
        for party_id in &party_ids {
            runner.observe_deaths();
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
            let wants_quest = selector < f64::from(profile.activity_vs_quest_propensity);
            if wants_quest
                && runner
                    .choose_quest(&runner.party_for(leader)?, profile)
                    .is_some()
            {
                runner.cycle(party_id, cycle)?;
            } else {
                runner.settlement_activity_day(leader_agent)?;
            }
            let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
                .connection
                .reducers
                .ensure_settlement_activity_then(settlement.clone(), cb));
            runner.call(result)?;
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
            let equipped_medication_courses = runner
                .connection
                .db
                .equipped_medication()
                .iter()
                .filter(|row| row.character_id == *character_id)
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
                equipped_medication_courses,
                elapsed_minutes,
                personal_gold_coin,
                party_treasury,
                party_stake,
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
    Ok(CoreLoopReport {
        backend_kind: "spacetimedb_authoritative_core_loop".into(),
        seed: config.seed,
        server_origin: config.host.clone(),
        database: config.database,
        run_nonce: config.run_nonce,
        deployment_identity_note: "server origin, database, and claimed run nonce identify this deployment; the SDK does not expose a deployed module binary digest".into(),
        profiles: runner.profiles,
        metrics: runner.metrics,
        trace: runner.trace,
        trace_truncated,
        total_event_count,
        final_agents,
        elapsed_game_minutes,
        policy_seed_note: "seed controls profiles and policy choices only; authoritative autoresolve seeds are server RNG values recorded in the trace".into(),
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
    fn crafting_uses_authoritative_medicine_boundary() {
        let recipe = &adventuresim_core::disease::MEDICATION_RECIPES[0];
        let dc = f32::from(recipe.medicine_dc);
        assert!(!adventuresim_core::disease::can_prepare_medication(
            dc - 0.51,
            recipe
        ));
        assert!(adventuresim_core::disease::can_prepare_medication(
            dc - 0.49,
            recipe
        ));
        assert!(adventuresim_core::disease::can_prepare_medication(
            dc, recipe
        ));
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
    fn smithing_decisions_quantize_float_noise_at_one_thousandth() {
        assert_eq!(quantize_smithing_condition(0.020_000_1), 20);
        assert_eq!(quantize_smithing_condition(0.019_999_9), 20);
        assert_eq!(quantize_smithing_condition(f32::NAN), 0);
        assert_eq!(quantize_smithing_condition(f32::INFINITY), 1_000);
    }

    #[test]
    fn report_metrics_expose_encounter_frequency_choices_losses_and_wipes() {
        let metrics = CoreLoopMetrics {
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
