//! Opt-in reducer-backed core-loop simulation.
//!
//! Unlike the native balance runner, this backend owns a disposable local
//! SpacetimeDB database and deliberately delegates every game rule to the
//! normal strategic reducers.

use crate::{AgentProfile, EquipmentStyle, generate_profile};
use adventuresim_stdb_client::spacetimedb_sdk::{DbContext, Table};
use adventuresim_stdb_client::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
    time::Duration,
};

use adventuresim_stdb_client::{
    abandon_quest_reducer::abandon_quest,
    accept_party_join_request_reducer::accept_party_join_request,
    accept_quest_reducer::accept_quest, autoresolve_quest_reducer::autoresolve_quest,
    autoresolve_report_table::AutoresolveReportTableAccess,
    battle_loot_item_table::BattleLootItemTableAccess,
    battle_result_table::BattleResultTableAccess,
    character_capability_table::CharacterCapabilityTableAccess,
    character_equip_table::CharacterEquipTableAccess,
    character_limbs_table::CharacterLimbsTableAccess,
    character_strategic_condition_table::CharacterStrategicConditionTableAccess,
    character_table::CharacterTableAccess,
    configure_simulation_character_reducer::configure_simulation_character,
    continue_camp_travel_reducer::continue_camp_travel,
    create_named_character_with_id_reducer::create_named_character_with_id,
    ensure_settlement_activity_reducer::ensure_settlement_activity, equip_item_reducer::equip_item,
    finalize_merchant_trade_reducer::finalize_merchant_trade,
    inventory_item_table::InventoryItemTableAccess, item_table::ItemTableAccess,
    liquidate_party_inventory_reducer::liquidate_party_inventory,
    party_inventory_item_table::PartyInventoryItemTableAccess,
    party_join_request_table::PartyJoinRequestTableAccess, party_table::PartyTableAccess,
    quest_status_type::QuestStatus, quest_table::QuestTableAccess,
    request_general_party_join_reducer::request_general_party_join,
    rest_at_camp_reducer::rest_at_camp, rest_at_settlement_hours_reducer::rest_at_settlement_hours,
    seed_world_reducer::seed_world, store_battle_loot_reducer::store_battle_loot,
    travel_to_quest_reducer::travel_to_quest, travel_to_settlement_reducer::travel_to_settlement,
    turn_in_quest_reducer::turn_in_quest,
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
}

impl CoreLoopConfig {
    pub fn validate(&self) -> Result<(), String> {
        let local = self.host.starts_with("http://127.0.0.1:")
            || self.host.starts_with("http://localhost:")
            || self.host.starts_with("http://[::1]:");
        if !local {
            return Err("core-loop backend only accepts an explicit loopback HTTP host".into());
        }
        if !self.database.starts_with("adventuresim-sim-")
            || !self
                .database
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("database must be a unique adventuresim-sim-* disposable name".into());
        }
        if !(2..=32).contains(&self.population) || !(1..=100).contains(&self.cycles) {
            return Err("population must be 2..=32 and cycles 1..=100".into());
        }
        Ok(())
    }
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopReport {
    pub backend_kind: String,
    pub seed: u64,
    pub database: String,
    pub profiles: Vec<AgentProfile>,
    pub metrics: CoreLoopMetrics,
    pub trace: Vec<CoreLoopEvent>,
    pub final_agents: Vec<FinalAgentState>,
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
        labor_minutes: s.labor,
        prayer_minutes: s.prayer,
        thievery_minutes: s.thievery,
        raiding_minutes: s.raiding,
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

    fn cycle(&mut self, cycle: u32) -> Result<(), String> {
        let leader = self.character_ids[0];
        let party = self.party_for(leader)?;
        let quest = self
            .choose_quest(&party, &self.profiles[0])
            .ok_or("no suitable available quest")?;
        self.metrics.quests_attempted += 1;
        let result = reducer_call!(self, "accept_quest", |cb| self
            .connection
            .reducers
            .accept_quest_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.event(
            0,
            CoreLoopEventKind::AcceptQuest,
            format!("cycle={cycle};quest={}", quest.id),
        );

        let result = reducer_call!(self, "travel_to_quest", |cb| self
            .connection
            .reducers
            .travel_to_quest_then(leader, quest.id.clone(), true, cb));
        self.call(result)?;
        self.event(
            0,
            CoreLoopEventKind::Travel,
            format!("outbound={}", quest.id),
        );
        self.travel_camps(leader, 0)?;

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
                self.event(0, CoreLoopEventKind::AutoresolveVictory, report.summary);
                break;
            }
            self.metrics.defeats += 1;
            self.event(0, CoreLoopEventKind::AutoresolveDefeat, report.summary);
            if attempt == MAX_DEFEAT_RETRIES {
                break;
            }
            self.metrics.retries += 1;
            let result = reducer_call!(self, "retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
            self.call(result)?;
            self.travel_camps(leader, 0)?;
            for agent in 0..self.character_ids.len() as u32 {
                self.recover_at_settlement(agent)?;
            }
            let result = reducer_call!(self, "retry_travel_to_quest", |cb| self
                .connection
                .reducers
                .travel_to_quest_then(leader, quest.id.clone(), true, cb));
            self.call(result)?;
            self.travel_camps(leader, 0)?;
        }
        if !victory {
            let result = reducer_call!(self, "defeat_retreat_to_settlement", |cb| self
                .connection
                .reducers
                .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
            self.call(result)?;
            self.travel_camps(leader, 0)?;
            for agent in 0..self.character_ids.len() as u32 {
                self.recover_at_settlement(agent)?;
            }
            let result = reducer_call!(self, "abandon_defeated_quest", |cb| self
                .connection
                .reducers
                .abandon_quest_then(leader, quest.id.clone(), cb));
            self.call(result)?;
            self.event(0, CoreLoopEventKind::AbandonQuest, quest.id);
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
            0,
            CoreLoopEventKind::StoreLoot,
            format!("stacks={}", loot.len()),
        );

        let result = reducer_call!(self, "return_to_settlement", |cb| self
            .connection
            .reducers
            .travel_to_settlement_then(leader, quest.settlement_id.clone(), false, cb));
        self.call(result)?;
        self.event(
            0,
            CoreLoopEventKind::Travel,
            format!("return={}", quest.settlement_id),
        );
        self.travel_camps(leader, 0)?;
        let result = reducer_call!(self, "turn_in_quest", |cb| self
            .connection
            .reducers
            .turn_in_quest_then(leader, quest.id.clone(), cb));
        self.call(result)?;
        self.metrics.quests_completed += 1;
        self.event(0, CoreLoopEventKind::TurnIn, quest.id.clone());

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
                0,
                CoreLoopEventKind::Liquidate,
                format!("stacks={}", sale.len()),
            );
        }
        self.try_upgrade(0, &quest.settlement_id)?;
        // A successful but costly victory may still leave someone incapacitated.
        // Recover before the next policy cycle instead of discovering that only
        // by repeatedly failing the next autoresolve reducer.
        for agent in 0..self.character_ids.len() as u32 {
            self.recover_at_settlement(agent)?;
        }
        Ok(())
    }

    fn try_upgrade(&mut self, agent: u32, settlement: &str) -> Result<(), String> {
        let character_id = self.character_ids[agent as usize];
        let equipped = self
            .connection
            .db
            .character_equip()
            .iter()
            .find(|row| row.character_id == character_id)
            .ok_or("missing equipment state")?;
        let equipped_ids = [equipped.left_hand_item_id, equipped.right_hand_item_id]
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
        let current_value = inventories
            .iter()
            .filter(|row| equipped_ids.contains(&row.id))
            .filter_map(|row| {
                self.connection
                    .db
                    .item()
                    .iter()
                    .find(|i| i.id == row.item_id)
            })
            .filter_map(|item| item.base_value)
            .max()
            .unwrap_or(0);
        let style = self.profiles[agent as usize].equipment.style;
        let mut candidates: Vec<_> = self
            .connection
            .db
            .item()
            .iter()
            .filter(|item| item.base_value.unwrap_or(0) > current_value)
            .filter(|item| match style {
                EquipmentStyle::Ranged => item.ranged,
                EquipmentStyle::Unarmored => item.melee && item.weight <= 2.0,
                EquipmentStyle::Light | EquipmentStyle::Heavy => item.melee,
            })
            .collect();
        candidates.sort_by_key(|item| item.base_value.unwrap_or(u32::MAX));
        for candidate in candidates {
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
            if self.call(result).is_err() {
                continue;
            }
            self.metrics.equipment_purchases += 1;
            self.event(agent, CoreLoopEventKind::Purchase, candidate.id.clone());
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
            self.metrics.equipment_upgrades += 1;
            self.event(agent, CoreLoopEventKind::Equip, candidate.id);
            return Ok(());
        }
        Ok(())
    }
}

pub fn run_core_loop(config: CoreLoopConfig) -> Result<CoreLoopReport, String> {
    config.validate()?;
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
        let result = reducer_call!(runner, "configure_simulation_character", |cb| runner
            .connection
            .reducers
            .configure_simulation_character_then(
                character_id,
                settlement.clone(),
                attributes.clone(),
                skills.clone(),
                downtime.clone(),
                cb,
            ));
        runner.call(result)?;
        runner.metrics.parties_formed += 1;
        runner.event(agent as u32, CoreLoopEventKind::FormParty, name);
    }

    // Joining is demonstrated with the same ordinary request/accept reducers as players.
    // The bounded bootstrap co-locates fresh sim-* solo parties before they use
    // the ordinary request/accept reducers to merge.
    let leader = runner.character_ids[0];
    let leader_party = runner.party_for(leader)?;
    let settlement = leader_party
        .current_settlement_id
        .clone()
        .ok_or("leader not at settlement")?;
    for agent in 1..runner.character_ids.len() {
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
    let result = reducer_call!(runner, "ensure_settlement_activity", |cb| runner
        .connection
        .reducers
        .ensure_settlement_activity_then(settlement.clone(), cb));
    runner.call(result)?;

    for cycle in 0..config.cycles {
        runner.cycle(cycle)?;
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
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(CoreLoopReport {
        backend_kind: "spacetimedb_authoritative_core_loop".into(),
        seed: config.seed,
        database: config.database,
        profiles: runner.profiles,
        metrics: runner.metrics,
        trace: runner.trace,
        final_agents,
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
        };
        assert!(config.validate().is_err());
        config.host = "http://127.0.0.1:3000".into();
        assert!(config.validate().is_err());
        config.database = "adventuresim-sim-test-1".into();
        assert!(config.validate().is_ok());
    }
}
