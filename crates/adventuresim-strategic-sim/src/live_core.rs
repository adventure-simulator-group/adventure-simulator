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
use url::Url;

use adventuresim_stdb_client::{
    abandon_quest_reducer::abandon_quest,
    accept_party_join_request_reducer::accept_party_join_request,
    accept_quest_reducer::accept_quest, autoresolve_quest_reducer::autoresolve_quest,
    autoresolve_report_table::AutoresolveReportTableAccess,
    backend_herbalist_examinations_table::BackendHerbalistExaminationsTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_equip_table::CharacterEquipTableAccess,
    character_limbs_table::CharacterLimbsTableAccess,
    character_personality_table::CharacterPersonalityTableAccess,
    character_strategic_condition_table::CharacterStrategicConditionTableAccess,
    character_table::CharacterTableAccess, character_time_table::CharacterTimeTableAccess,
    claim_simulation_run_reducer::claim_simulation_run,
    configure_simulation_character_reducer::configure_simulation_character,
    continue_camp_travel_reducer::continue_camp_travel, craft_medication_reducer::craft_medication,
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
    quest_status_type::QuestStatus, quest_table::QuestTableAccess,
    repair_order_table::RepairOrderTableAccess,
    request_general_party_join_reducer::request_general_party_join,
    rest_at_camp_reducer::rest_at_camp, rest_at_settlement_hours_reducer::rest_at_settlement_hours,
    retrieve_repaired_item_reducer::retrieve_repaired_item,
    seed_simulation_disease_reducer::seed_simulation_disease,
    seed_simulation_equipment_damage_reducer::seed_simulation_equipment_damage,
    seed_world_reducer::seed_world, settlement_smith_table::SettlementSmithTableAccess,
    simulation_run_table::SimulationRunTableAccess, store_battle_loot_reducer::store_battle_loot,
    submit_item_for_repair_reducer::submit_item_for_repair,
    travel_to_quest_reducer::travel_to_quest, travel_to_settlement_reducer::travel_to_settlement,
    turn_in_quest_reducer::turn_in_quest,
    withdraw_party_inventory_item_reducer::withdraw_party_inventory_item,
};

const ACTION_TIMEOUT: Duration = Duration::from_secs(20);
/// Severe but non-incapacitating injuries can reduce overland pace enough for
/// a long quest leg to require many daily camps.
const MAX_CAMPS_PER_LEG: u32 = 512;
const MAX_DEFEAT_RETRIES: u32 = 2;
/// Natural recovery is one percent per day without medicine, so a character
/// reduced to zero may legitimately need roughly one hundred days.
const MAX_RECOVERY_ACTIONS: u32 = 32;

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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreLoopEventKind {
    FormParty,
    RequestJoin,
    AcceptJoin,
    AcceptQuest,
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
        melee_hours: s.melee,
        dodge_hours: s.dodge,
        block_hours: s.block,
        ranged_hours: s.ranged,
        will_hours: s.will,
        charisma_hours: s.charisma,
        medicine_hours: s.medicine,
        faith_hours: s.faith,
        stealth_hours: s.stealth,
        balance_hours: s.balance,
        surgeon_hours: s.surgeon,
        smithing_hours: s.smithing,
    }
}

fn live_schedule(profile: &AgentProfile) -> ScheduleAllocation {
    let s = profile.schedule;
    ScheduleAllocation {
        melee_minutes: s.melee,
        dodge_minutes: s.dodge,
        block_minutes: s.block,
        ranged_minutes: s.ranged,
        will_minutes: s.will,
        charisma_minutes: s.charisma,
        medicine_minutes: s.medicine,
        faith_minutes: s.faith,
        stealth_minutes: s.stealth,
        balance_minutes: s.balance,
        surgeon_minutes: s.surgeon,
        smithing_minutes: s.smithing,
        labor_minutes: s.labor,
        prayer_minutes: s.prayer,
        thievery_minutes: s.thievery,
        raiding_minutes: s.raiding,
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
        self.trace.push(CoreLoopEvent {
            sequence: self.sequence,
            agent_id,
            kind,
            detail,
        });
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

    fn travel_camps(&mut self, leader: u64, agent: u32) -> Result<(), String> {
        for _ in 0..MAX_CAMPS_PER_LEG {
            let party = self.party_for(leader)?;
            if party.camp_destination_id.is_none() {
                self.metrics.travel_legs += 1;
                return Ok(());
            }
            let remaining_before = party.camp_remaining_minutes;
            let result = reducer_call!(self, "rest_at_camp", |cb| self
                .connection
                .reducers
                .rest_at_camp_then(leader, 1_440, cb));
            self.call(result)?;
            let result = reducer_call!(self, "continue_camp_travel", |cb| self
                .connection
                .reducers
                .continue_camp_travel_then(leader, cb));
            self.call(result)?;
            self.metrics.camp_stops += 1;
            self.event(
                agent,
                CoreLoopEventKind::Camp,
                format!("remaining_before={remaining_before}"),
            );
            let after = self.party_for(leader)?;
            if after.camp_destination_id.is_some()
                && after.camp_remaining_minutes >= remaining_before
            {
                self.metrics.stuck_detections += 1;
                return Err("camp continuation made no progress".into());
            }
        }
        self.metrics.stuck_detections += 1;
        Err("camp bound exhausted".into())
    }

    fn choose_quest(&self, party: &Party, profile: &AgentProfile) -> Option<Quest> {
        let settlement = party.current_settlement_id.as_ref()?;
        let mut quests: Vec<_> = self
            .connection
            .db
            .quest()
            .iter()
            .filter(|q| q.settlement_id == *settlement && q.status == QuestStatus::Available)
            .collect();
        quests.sort_by_key(|q| {
            let risk_target = (profile.risk_tolerance * 10.0).round() as i32;
            ((q.difficulty - risk_target).abs(), q.id.clone())
        });
        quests.into_iter().next()
    }

    fn recover_at_settlement(&mut self, agent: u32) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        for _ in 0..MAX_RECOVERY_ACTIONS {
            let limbs = self
                .connection
                .db
                .character_limbs()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing limb state")?;
            let minimum = [
                limbs.left_arm_health,
                limbs.right_arm_health,
                limbs.left_leg_health,
                limbs.right_leg_health,
                limbs.head_health,
                limbs.chest_health,
                limbs.stomach_health,
            ]
            .into_iter()
            .fold(1.0_f32, f32::min);
            let condition = self
                .connection
                .db
                .character_strategic_condition()
                .iter()
                .find(|row| row.character_id == character_id)
                .ok_or("missing strategic condition")?;
            if minimum >= self.profiles[agent as usize].recovery_health_threshold
                && condition.status == "ready"
            {
                return Ok(());
            }
            let result = reducer_call!(self, "rest_at_settlement_hours", |cb| self
                .connection
                .reducers
                .rest_at_settlement_hours_then(character_id, 7 * 1440, false, cb));
            self.call(result)?;
            self.metrics.recovery_rests += 1;
            self.event(
                agent,
                CoreLoopEventKind::Recover,
                format!("minimum_health={minimum:.3}"),
            );
        }
        self.metrics.stuck_detections += 1;
        Err("recovery bound exhausted".into())
    }

    fn personal_gold(&self, character_id: u64) -> u64 {
        self.connection
            .db
            .inventory_item()
            .iter()
            .filter(|row| row.character_id == character_id && row.item_id == "gold_coin")
            .map(|row| u64::from(row.quantity))
            .sum()
    }

    /// Observe only public condition plus the caller-scoped herbalist result.
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
                self.metrics.disease_deaths += 1;
                self.event(agent, CoreLoopEventKind::Death, "terminal disease state");
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
            if condition.status == "ready" && !treatment_active {
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
                if ready && !still_treated {
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
                .ok_or("herbalist examination reducer returned no caller-scoped result")?;
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
                if capability.medicine >= f32::from(recipe.medicine_dc) && has_ingredients {
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
            .filter(|row| row.character_id == character_id && row.item_id == "gold_coin")
            .map(|row| u64::from(row.quantity))
            .sum();

        let mut orders: Vec<_> = self
            .connection
            .db
            .repair_order()
            .iter()
            .filter(|row| row.owner_character_id == character_id && row.settlement_id == settlement)
            .collect();
        repair_budget = adventuresim_core::durability::repair_budget_after_reservations(
            repair_budget,
            &self
                .connection
                .db
                .repair_order()
                .iter()
                .filter(|order| order.owner_character_id == character_id)
                .map(|order| order.quoted_cost)
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
            let inventory: Vec<_> = self
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == character_id)
                .collect();
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
                let skill = match definition.kind {
                    ItemKind::Weapon | ItemKind::Shield => smith.weaponsmith_skill,
                    ItemKind::Armor => smith.armourer_skill,
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
                let total: f32 = bins.iter().sum();
                let red: f32 = bins[2..].iter().sum();
                let repairable: f32 = bins.iter().take(skill as usize).sum();
                let quote = adventuresim_core::durability::repair_quote(
                    definition.base_value.unwrap_or(1),
                    repairable,
                );
                // Mild yellow wear is handled automatically by ordinary rest.
                if repairable > f32::EPSILON
                    && (red >= 0.02 || total >= 0.35)
                    && u64::from(quote) <= repair_budget
                {
                    let result = reducer_call!(self, "submit_item_for_repair", |cb| self
                        .connection
                        .reducers
                        .submit_item_for_repair_then(
                            character_id,
                            settlement.clone(),
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
                            1.0 - total
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
            .filter(|row| row.character_id == character_id && row.item_id == "gold_coin")
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
            let wait = ready_at - now;
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
        }
        for order in affordable {
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
        }
        Ok(())
    }

    fn party_agents(&self, leader: u64) -> Result<Vec<u32>, String> {
        let party = self.party_for(leader)?;
        Ok(self
            .connection
            .db
            .party_member()
            .iter()
            .filter(|member| member.party_id == party.id)
            .filter_map(|member| {
                self.character_ids
                    .iter()
                    .position(|id| *id == member.character_id)
                    .map(|index| index as u32)
            })
            .collect())
    }

    fn cycle(&mut self, leader_agent: u32, cycle: u32) -> Result<(), String> {
        let leader = self.character_ids[leader_agent as usize];
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
        let result = reducer_call!(self, "accept_quest", |cb| self
            .connection
            .reducers
            .accept_quest_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::AcceptQuest,
            format!(
                "cycle={cycle};quest={};title={};difficulty={};enemy={}x{};distance_m={}",
                quest.id,
                quest.title,
                quest.difficulty,
                quest.enemy_type,
                quest.enemy_count,
                quest.distance_m
            ),
        );

        let result = reducer_call!(self, "travel_to_quest", |cb| self
            .connection
            .reducers
            .travel_to_quest_then(leader, quest.id.clone(), true, cb));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::Travel,
            format!("outbound={}", quest.id),
        );
        self.travel_camps(leader, leader_agent)?;

        // Travel advances every member's disease clock. Re-observe public
        // life/condition state before attempting a living-only combat reducer.
        let unsafe_after_travel = party_agents
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
        if !unsafe_after_travel.is_empty() {
            for &agent in &unsafe_after_travel {
                self.metrics.quests_suppressed_for_health += 1;
                self.event(
                    agent,
                    CoreLoopEventKind::QuestSuppressed,
                    format!("after_travel;cycle={cycle}"),
                );
            }
            let leader_alive = self
                .connection
                .db
                .character()
                .iter()
                .find(|row| row.id == leader)
                .is_some_and(|row| row.alive);
            if !leader_alive {
                self.metrics.disease_deaths += 1;
                self.event(
                    leader_agent,
                    CoreLoopEventKind::Death,
                    "leader died during travel",
                );
                return Ok(());
            }
            let result = reducer_call!(self, "illness_retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
            self.call(result)?;
            self.travel_camps(leader, leader_agent)?;
            for agent in party_agents {
                self.ensure_medically_safe(agent)?;
            }
            let result = reducer_call!(self, "abandon_unsafe_quest", |cb| self
                .connection
                .reducers
                .abandon_quest_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.event(leader_agent, CoreLoopEventKind::AbandonQuest, quest.id);
            return Ok(());
        }

        let mut victory = false;
        for attempt in 0..=MAX_DEFEAT_RETRIES {
            let result = reducer_call!(self, "autoresolve_quest", |cb| self
                .connection
                .reducers
                .autoresolve_quest_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            let report = self
                .connection
                .db
                .autoresolve_report()
                .iter()
                .find(|r| r.quest_id == quest.id)
                .ok_or("autoresolve completed without a report")?;
            if self
                .connection
                .db
                .battle_result()
                .iter()
                .any(|r| r.quest_id == quest.id)
            {
                victory = true;
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
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
            self.call(result)?;
            self.travel_camps(leader, leader_agent)?;
            for agent in self.party_agents(leader)? {
                self.recover_at_settlement(agent)?;
            }
            let result = reducer_call!(self, "retry_travel_to_quest", |cb| self
                .connection
                .reducers
                .travel_to_quest_then(leader, quest.id.clone(), true, cb));
            self.call(result)?;
            self.travel_camps(leader, leader_agent)?;
        }
        if !victory {
            let result = reducer_call!(self, "defeat_retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
            self.call(result)?;
            self.travel_camps(leader, leader_agent)?;
            for agent in self.party_agents(leader)? {
                self.recover_at_settlement(agent)?;
            }
            let result = reducer_call!(self, "abandon_defeated_quest", |cb| self
                .connection
                .reducers
                .abandon_quest_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.event(leader_agent, CoreLoopEventKind::AbandonQuest, quest.id);
            let result = reducer_call!(self, "replenish_quests_after_abandon", |cb| self
                .connection
                .reducers
                .ensure_settlement_activity_then(quest.settlement_id.clone(), cb));
            self.call(result)?;
            return Ok(());
        }

        let loot: Vec<_> = self
            .connection
            .db
            .battle_loot_item()
            .iter()
            .filter(|row| row.quest_id == quest.id)
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
            .store_battle_loot_then(leader, quest.id.clone(), vec![], vec![], cb));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::StoreLoot,
            format!("stacks={}", loot.len()),
        );

        let result = reducer_call!(self, "return_to_settlement", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
        self.call(result)?;
        self.event(
            leader_agent,
            CoreLoopEventKind::Travel,
            format!("return={}", quest.settlement_id),
        );
        self.travel_camps(leader, leader_agent)?;
        let result = reducer_call!(self, "turn_in_quest", |cb| self
            .connection
            .reducers
            .turn_in_quest_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.metrics.quests_completed += 1;
        self.event(leader_agent, CoreLoopEventKind::TurnIn, quest.id.clone());

        let party = self.party_for(leader)?;
        let sale: Vec<_> = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .filter(|row| row.party_id == party.id && row.item_id != "gold_coin")
            .collect();
        if !sale.is_empty() {
            let before_coins: u64 = self
                .connection
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| row.party_id == party.id && row.item_id == "gold_coin")
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
                .filter(|row| row.party_id == party.id && row.item_id == "gold_coin")
                .map(|row| u64::from(row.quantity))
                .sum();
            self.metrics.sale_proceeds += after_coins.saturating_sub(before_coins);
            self.event(
                leader_agent,
                CoreLoopEventKind::Liquidate,
                format!("stacks={}", sale.len()),
            );
        }
        self.try_upgrade(leader_agent, &quest.settlement_id)?;
        // A successful but costly victory may still leave someone incapacitated.
        // Recover before the next policy cycle instead of discovering that only
        // by repeatedly failing the next autoresolve reducer.
        for agent in self.party_agents(leader)? {
            self.recover_at_settlement(agent)?;
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
        candidates.sort_by(|left, right| right.0.total_cmp(&left.0));
        let Some((improvement, cost, candidate)) = candidates.into_iter().next() else {
            return Ok(());
        };
        let treasury = self
            .connection
            .db
            .party_inventory_item()
            .iter()
            .find(|row| row.party_id == party_id && row.item_id == "gold_coin")
            .ok_or("earned party treasury is missing")?;
        if treasury.quantity < cost {
            return Ok(());
        }
        let result = reducer_call!(self, "withdraw_earned_upgrade_gold", |cb| self
            .connection
            .reducers
            .withdraw_party_inventory_item_then(character_id, treasury.id, cost, cb));
        self.call(result)?;
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
        .subscribe_to_all_tables();
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
    let result = reducer_call!(runner, "seed_world", |cb| runner
        .connection
        .reducers
        .seed_world_then(cb));
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
        let authoritative_personality = runner
            .connection
            .db
            .character_personality()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("configured character has no authoritative personality")?;
        if authoritative_personality != personality {
            return Err("authoritative personality does not match deterministic profile".into());
        }
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
            .seed_simulation_equipment_damage_then(character_id, fixture_item.id, cb));
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
    let mut leader_agents = Vec::new();
    for first in (0..runner.character_ids.len()).step_by(config.party_size as usize) {
        let leader = runner.character_ids[first];
        leader_agents.push(first as u32);
        let leader_party = runner.party_for(leader)?;
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
        for &leader_agent in &leader_agents {
            let leader = runner.character_ids[leader_agent as usize];
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
                runner.cycle(leader_agent, cycle)?;
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
            let equipment_item_ids = runner
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == *character_id)
                .filter(|row| equipped_ids.contains(&Some(row.id)))
                .map(|row| row.item_id)
                .collect();
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
            let personal_gold_coin = runner
                .connection
                .db
                .inventory_item()
                .iter()
                .filter(|row| row.character_id == *character_id && row.item_id == "gold_coin")
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
                .filter(|row| row.party_id == party_id && row.item_id == "gold_coin")
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
                gold: character.gold,
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
        final_agents,
        elapsed_game_minutes,
        policy_seed_note: "seed controls profiles and policy choices only; authoritative autoresolve seeds are server RNG values recorded in the trace".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
