//! Tactical Server - Replicon + Aeronet websocket game server

mod bot;
mod combat;
mod stdb;
mod terrain;

use std::{collections::HashSet, net::SocketAddr, num::NonZeroU32, time::Duration};

use adventuresim_stdb_client::*;
use adventuresim_tactical_core::{
    inventory::ItemProperties, physics::AdventureSimulatorPhysicsPlugin, prelude::*,
};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{DisconnectReason, Disconnected, LocalAddr},
    bevy_replicon::prelude::*,
    prelude::*,
};
use bevy::prelude::*;
use bevy::time::Stopwatch;
use clap::{ArgAction, Parser};

use crate::{
    bot::{MissionEnemy, OffensiveMeleeAi},
    combat::{MeleeAttackAuthority, TacticalCombatSide, TacticalConsequenceAccumulator},
    stdb::SpacetimeDb,
    terrain::TerrainGenerator,
};
use input::AccumulatedInput;

/// Default [`Args::timeout`] time.
const MISSION_TIMEOUT_SECS: f32 = 300.0;
/// Retry interval after a synchronous terminal reducer submission error.
const TERMINAL_RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Time a sealed, empty Party has to reconnect before the mission is abandoned.
const PARTY_RECONNECT_GRACE: Duration = Duration::from_secs(10);

/// Level map size.
const TERRAIN_SIZE: usize = 100;

#[derive(Parser, Debug, Clone, Resource)]
#[command(name = "adventuresim-tactical-server")]
#[command(about = "Tactical mission server for Adventure Simulator")]
struct Args {
    /// Address to listen on
    #[arg(long, default_value = "127.0.0.1:6000")]
    addr: SocketAddr,

    /// Unique mission instance ID
    #[arg(long)]
    mission_id: String,

    /// One-use dispatcher claim, supplied only through the child environment.
    #[arg(long, env = "ADVENTURESIM_TACTICAL_CLAIM", hide_env_values = true)]
    tactical_claim: String,

    /// Scene key (e.g., "hills", "desert")
    #[arg(long)]
    scene_key: String,

    /// Scene allowed physical width (x-size).
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_width: usize,

    /// Scene allowed physical depth (z-size).
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_depth: usize,

    /// Authoritative number of quest enemies that must be defeated.
    #[arg(long)]
    required_enemy_kills: u32,

    /// Living Party members bound by strategic authority for this mission.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    expected_party_members: u32,

    /// Observer-safe combat scale copied from the trusted mission request.
    /// The strategic reducer independently derives the authoritative value.
    #[arg(long)]
    enemy_combat_scale_bps: u32,

    /// SpacetimeDB URI (e.g., http://localhost:3000)
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "adventuresim-stdb-module")]
    spacetimedb_module: String,

    /// Mission timeout in seconds (how long the server stays up waiting for players)
    #[arg(long, default_value_t = MISSION_TIMEOUT_SECS)]
    timeout: f32,

    /// Disable the timeout entirely
    #[arg(
        long,
        action = ArgAction::SetTrue,
        conflicts_with = "timeout"
    )]
    no_timeout: bool,
}

fn main() {
    let args = Args::parse();

    App::new()
        .add_plugins((DefaultPlugins.set(bevy::log::LogPlugin {
            filter: "tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
            ..default()
        }),))
        .add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: true,
                }),
            AdventureSimulatorNetPlugins,
        ))
        .add_plugins((
            stdb::SpacetimeDbPlugin,
            combat::CombatPlugin,
            bot::BotPlugin,
        ))
        .insert_resource(MissionState {
            timeout: (!args.no_timeout)
                .then_some(args.timeout)
                .map(|duration| Timer::from_seconds(duration, TimerMode::Once)),
            enemies_defeated: 0,
            required_enemy_defeats: args.required_enemy_kills,
            expected_party_members: args.expected_party_members,
            seen_party_members: HashSet::new(),
            enrollment_begun: false,
            enrollment_sealed: false,
            abandonment_elapsed: Duration::ZERO,
            terminal_retry_not_before: Duration::ZERO,
            pending_resolution: None,
            pending_receipt: None,
            committed: false,
        })
        .insert_resource(args)
        .add_systems(
            Update,
            (
                (check_terminal_combat_outcome, check_mission_timeout)
                    .chain()
                    .after(combat::update_tactical_combat_state)
                    .after(spawn_connected_players),
                spawn_connected_players.after(stdb::update_spacetimedb),
                (setup_server, setup_stdb_callbacks).run_if(resource_added::<SpacetimeDb>),
            ),
        )
        .add_systems(OnEnter(ServerState::Running), on_server_started)
        .add_observer(on_join_request)
        .add_observer(on_player_input)
        .add_observer(on_client_disconnected)
        .run();
}

#[derive(Component, Debug, Clone, Copy)]
struct LoadingPlayer {
    requested_player_id: u64,
}

/// Durable inventory provenance retained only on the authoritative server.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TacticalInventoryItemId(pub u64);

#[derive(Resource)]
pub struct MissionState {
    timeout: Option<Timer>,
    pub enemies_defeated: u32,
    required_enemy_defeats: u32,
    expected_party_members: u32,
    seen_party_members: HashSet<u64>,
    enrollment_begun: bool,
    enrollment_sealed: bool,
    abandonment_elapsed: Duration,
    terminal_retry_not_before: Duration,
    pending_resolution: Option<TacticalMissionResolution>,
    pending_receipt: Option<TacticalConsequenceReceipt>,
    committed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalCombatSnapshot {
    pub required_enemies: u32,
    pub loaded_enemies: u32,
    pub defeated_enemies: u32,
    pub loaded_party: u32,
    pub incapacitated_party: u32,
    pub enrollment_sealed: bool,
}

pub(crate) fn terminal_resolution(
    snapshot: TerminalCombatSnapshot,
) -> Option<TacticalMissionResolution> {
    if snapshot.required_enemies == 0
        || snapshot.loaded_enemies < snapshot.required_enemies
        || !snapshot.enrollment_sealed
        || snapshot.loaded_party == 0
    {
        return None;
    }
    let enemies_defeated = snapshot.defeated_enemies >= snapshot.required_enemies;
    let party_defeated = snapshot.incapacitated_party >= snapshot.loaded_party;
    match (enemies_defeated, party_defeated) {
        // Simultaneous defeat fails closed, matching autoresolve's lack of an
        // allied victory when both sides are unable to continue.
        (_, true) => Some(TacticalMissionResolution::Failed),
        (true, false) => Some(TacticalMissionResolution::Defeated),
        (false, false) => None,
    }
}

fn abandonment_due(
    elapsed: &mut Duration,
    enrollment_begun: bool,
    loaded_party: u32,
    has_loading_player: bool,
    delta: Duration,
) -> bool {
    if enrollment_begun && loaded_party == 0 && !has_loading_player {
        *elapsed = elapsed.saturating_add(delta);
    } else {
        *elapsed = Duration::ZERO;
    }
    *elapsed >= PARTY_RECONNECT_GRACE
}

fn enrollment_ready(expected: u32, seen: usize, has_loading_player: bool) -> bool {
    expected > 0 && seen >= expected as usize && !has_loading_player
}

impl MissionState {
    fn submit_terminal<E>(
        &mut self,
        resolution: TacticalMissionResolution,
        receipt: TacticalConsequenceReceipt,
        now: Duration,
        mut send: impl FnMut(TacticalMissionResolution, TacticalConsequenceReceipt) -> Result<(), E>,
    ) -> Result<bool, E> {
        if self.committed || now < self.terminal_retry_not_before {
            return Ok(false);
        }
        let resolution = *self.pending_resolution.get_or_insert(resolution);
        let receipt = self.pending_receipt.get_or_insert(receipt).clone();
        if let Err(error) = send(resolution, receipt) {
            self.terminal_retry_not_before = now.saturating_add(TERMINAL_RETRY_BACKOFF);
            return Err(error);
        }
        self.committed = true;
        self.pending_resolution = None;
        self.pending_receipt = None;
        Ok(true)
    }
}

fn setup_server(mut commands: Commands, args: Res<Args>) {
    info!(
        "Starting tactical server for mission '{}'...",
        args.mission_id
    );
    info!("Scene: {}, Address: {}", args.scene_key, args.addr);
    info!(
        "Enemy objective: count={}, scale={} bps",
        args.required_enemy_kills, args.enemy_combat_scale_bps
    );

    commands.spawn(AdventureSimulatorServer { addr: args.addr });

    if !args.no_timeout {
        info!("Will timeout in {} seconds", args.timeout);
    }
}

fn setup_stdb_callbacks(conn: Res<SpacetimeDb>) {
    conn.subscribe_connected_players();
}

fn spawn_connected_players(
    conn: Res<SpacetimeDb>,
    mut cmd: Commands,
    q_loading: Query<(Entity, &LoadingPlayer)>,
    q_scene: Query<&SceneTerrain>,
) {
    for player in conn.take_connected_players() {
        spawn_connected_player(&player, &mut cmd, &q_loading, &q_scene);
    }
}

fn spawn_connected_player(
    player: &ConnectedPlayer,
    cmd: &mut Commands,
    q_loading: &Query<(Entity, &LoadingPlayer)>,
    q_scene: &Query<&SceneTerrain>,
) {
    let entity = if player.character.temporary {
        cmd.spawn((
            MissionEnemy,
            OffensiveMeleeAi::default(),
            TacticalCombatSide::Enemy,
        ))
        .id()
    } else {
        let Some((entity, _)) = q_loading
            .iter()
            .find(|(_, id)| id.requested_player_id == player.character.id)
        else {
            warn!(
                "Got new ConnectedPlayer from stdb, but there is no LoadingPlayer for it: {}#{}",
                player.character.name, player.character.id
            );
            return;
        };
        entity
    };

    let skills = Skills {
        polearm_hours: player.skills.polearm_hours,
        axe_hours: player.skills.axe_hours,
        bludgeon_hours: player.skills.bludgeon_hours,
        sword_hours: player.skills.sword_hours,
        knife_hours: player.skills.knife_hours,
        dodge_hours: player.skills.dodge_hours,
        block_hours: player.skills.block_hours,
        bow_hours: player.skills.bow_hours,
        crossbow_hours: player.skills.crossbow_hours,
        firearm_hours: player.skills.firearm_hours,
        throw_hours: player.skills.throw_hours,
        will_hours: player.skills.will_hours,
        insight_hours: player.skills.insight_hours,
        charm_hours: player.skills.charm_hours,
        command_hours: player.skills.command_hours,
        deception_hours: player.skills.deception_hours,
        physiology_hours: player.skills.physiology_hours,
        religion_hours: {
            let religion = &player.skills.religion_hours;
            [
                religion.roman_catholic,
                religion.lutheran,
                religion.reformed,
                religion.anglican,
                religion.eastern_orthodox,
                religion.islamic,
                religion.judaism,
            ]
            .into_iter()
            .filter(|hours| hours.is_finite())
            .map(|hours| hours.max(0.0))
            .sum()
        },
        bestiary_beast_hours: player.skills.bestiary_hours.beast,
        bestiary_undead_hours: player.skills.bestiary_hours.undead,
        bestiary_human_hours: player.skills.bestiary_hours.human,
        bestiary_werekin_hours: player.skills.bestiary_hours.werekin,
        bestiary_elf_hours: player.skills.bestiary_hours.elf,
        bestiary_dwarf_hours: player.skills.bestiary_hours.dwarf,
        bestiary_fey_hours: player.skills.bestiary_hours.fey,
        bestiary_spirit_hours: player.skills.bestiary_hours.spirit,
        bestiary_greenskin_hours: player.skills.bestiary_hours.greenskin,
        bestiary_insectoid_hours: player.skills.bestiary_hours.insectoid,
        bestiary_draconid_hours: player.skills.bestiary_hours.draconid,
        bestiary_construct_hours: player.skills.bestiary_hours.construct,
        bestiary_wildmen_hours: player.skills.bestiary_hours.wildmen,
        surgery_hours: player.skills.surgery_hours,
        stealth_hours: player.skills.stealth_hours,
        balance_hours: player.skills.balance_hours,
        tailoring_hours: player.skills.tailoring_hours,
        smithing_hours: player.skills.smithing_hours,
    };
    let limbs = Limbs {
        body_weight_kg: player.body_weight_kg,
        left_arm: player.limbs.left_arm_health,
        right_arm: player.limbs.right_arm_health,
        left_leg: player.limbs.left_leg_health,
        right_leg: player.limbs.right_leg_health,
        chest: player.limbs.chest_health,
        stomach: player.limbs.stomach_health,
        head: player.limbs.head_health,
    };
    let attributes = Attributes {
        endurance: player.attrs.endurance,
        immunity: player.attrs.immunity,
        gut: player.attrs.gut,
        intelligence: player.attrs.intelligence,
        instinct: player.attrs.instinct,
        eyesight: player.attrs.eyesight,
        hearing: player.attrs.hearing,
        left_arm_strength: player.attrs.left_arm_strength,
        right_arm_strength: player.attrs.right_arm_strength,
        left_leg_strength: player.attrs.left_leg_strength,
        right_leg_strength: player.attrs.right_leg_strength,
        left_arm_agility: player.attrs.left_arm_agility,
        right_arm_agility: player.attrs.right_arm_agility,
        left_leg_agility: player.attrs.left_leg_agility,
        right_leg_agility: player.attrs.right_leg_agility,
    };
    let stats = Stats {
        calories_used: player.stats.calories_used,
        focus: player.stats.focus,
    };

    let player_collider = player_collider();
    let spawn_position = Vec2::new(rand::random_range(-5.0..5.0), rand::random_range(-5.0..5.0));
    let spawn_height = q_scene
        .iter()
        .next()
        .and_then(|terrain| terrain.height_at(spawn_position))
        .unwrap_or_default()
        + player_spawn_offset(&player_collider);

    let tag = if player.character.temporary {
        "Bot"
    } else {
        "Player"
    };
    let name = format!("{tag}#{} {}", player.character.id, player.character.name);

    let (starting_incapacitation, starting_blood_fraction) = derive_combat_starting_condition(
        player.strategic_incapacitation,
        player.strategic_pain,
        player.strategic_blood_loss,
        player.current_blood_ml,
        player.maximum_blood_ml,
    );

    cmd.entity(entity).remove::<LoadingPlayer>().insert((
        Name::new(name),
        Replicated,
        Player {
            name: player.character.name.clone(),
        },
        PlayerId(player.character.id),
        BestiaryCategories::default(),
        skills,
        limbs,
        attributes,
        stats,
        TacticalCombatState {
            starting_incapacitation,
            starting_blood_fraction,
            ..default()
        },
        MeleeAttackAuthority::default(),
        if player.character.temporary {
            TacticalCombatSide::Enemy
        } else {
            TacticalCombatSide::Party
        },
        Transform::from_xyz(spawn_position.x, spawn_height, spawn_position.y),
        (
            player_collider.clone(),
            CollisionMargin(0.01),
            CharacterController::default(),
            CharacterLook::default(),
        ),
    ));

    for item in &player.items {
        let Some(quantity) = NonZeroU32::new(item.quantity) else {
            warn!(
                "Got item '{}' with zero quantity for Player#{}; skipped",
                item.item.id, player.character.id
            );
            continue;
        };

        let mut item_cmd = cmd.spawn((
            Replicated,
            TacticalInventoryItemId(item.inventory_item_id),
            ItemOf(entity),
            ItemQuantity(quantity),
            ItemProperties {
                weight: item.item.weight,
                id: item.item.id.clone(),
            },
        ));

        match item.item.kind {
            ItemKind::Simple
            | ItemKind::Container
            | ItemKind::Clothing
            | ItemKind::Currency
            | ItemKind::Ingredient
            | ItemKind::Medication
            | ItemKind::Food => {}
            ItemKind::Weapon => {
                item_cmd.insert(WeaponItem {
                    skill_weights: [
                        item.item.weapon_skills.polearm,
                        item.item.weapon_skills.axe,
                        item.item.weapon_skills.bludgeon,
                        item.item.weapon_skills.sword,
                        item.item.weapon_skills.knife,
                        item.item.weapon_skills.bow,
                        item.item.weapon_skills.crossbow,
                        item.item.weapon_skills.firearm,
                        item.item.weapon_skills.throw_skill,
                    ],
                    accuracy: item.item.accuracy,
                    penetration: item.item.penetration,
                    reach: item.item.reach,
                    balance: item.item.balance,
                    precise: item.item.precise,
                });
            }
            ItemKind::Armor => {
                if let Some(slot) = match item.item.slot {
                    ItemSlot::LeftArm => Some(ArmorSlot::Arms(Some(ArmorSide::Left))),
                    ItemSlot::RightArm => Some(ArmorSlot::Arms(Some(ArmorSide::Right))),
                    ItemSlot::AnyArm => Some(ArmorSlot::Arms(None)),
                    ItemSlot::LeftLeg => Some(ArmorSlot::Legs(Some(ArmorSide::Left))),
                    ItemSlot::RightLeg => Some(ArmorSlot::Legs(Some(ArmorSide::Right))),
                    ItemSlot::AnyLeg => Some(ArmorSlot::Legs(None)),
                    ItemSlot::Head => Some(ArmorSlot::Head),
                    ItemSlot::Chest => Some(ArmorSlot::Chest),
                    ItemSlot::Stomach => Some(ArmorSlot::Stomach),
                    slot => {
                        warn!(
                            "Got armor item '{}' with an invalid slot {slot:?} for Player#{}",
                            item.item.id, player.character.id
                        );
                        None
                    }
                } {
                    item_cmd.insert(ArmorItem {
                        range_of_motion: item.item.range_of_motion,
                        coverage: item.item.coverage,
                        slot,
                        resistance: item.item.resistance,
                        padding: item.item.padding,
                        flexibility: item.item.flexibility,
                    });
                }
            }
            ItemKind::Shield => {
                item_cmd.insert(ShieldItem {
                    block: item.item.block,
                });
            }
        }

        match item.equipped {
            Some(ItemSlot::LeftHolding) => {
                item_cmd.insert(EquipSlot::HoldingLeft);
            }
            Some(ItemSlot::RightHolding) => {
                item_cmd.insert(EquipSlot::HoldingRight);
            }
            Some(ItemSlot::LeftArm) => {
                item_cmd.insert(EquipSlot::ArmorLeftArm);
            }
            Some(ItemSlot::RightArm) => {
                item_cmd.insert(EquipSlot::ArmorRightArm);
            }
            Some(ItemSlot::LeftLeg) => {
                item_cmd.insert(EquipSlot::ArmorLeftLeg);
            }
            Some(ItemSlot::RightLeg) => {
                item_cmd.insert(EquipSlot::ArmorRightLeg);
            }
            Some(ItemSlot::Chest) => {
                item_cmd.insert(EquipSlot::ArmorChest);
            }
            Some(ItemSlot::Stomach) => {
                item_cmd.insert(EquipSlot::ArmorStomach);
            }
            Some(ItemSlot::Head) => {
                item_cmd.insert(EquipSlot::ArmorHead);
            }
            slot @ Some(
                ItemSlot::None | ItemSlot::AnyHolding | ItemSlot::AnyArm | ItemSlot::AnyLeg,
            ) => {
                warn!(
                    "Got equipped item '{}' with an invalid equip slot {slot:?} for Player#{}",
                    item.item.id, player.character.id
                );
            }
            _ => {}
        }
    }

    info!(
        temorary = player.character.temporary,
        "Player {entity:?} is fully loaded"
    );
}

fn commit_terminal_resolution(
    resolution: TacticalMissionResolution,
    now: Duration,
    conn: Res<SpacetimeDb>,
    consequences: Res<TacticalConsequenceAccumulator>,
    mut state: ResMut<MissionState>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    let receipt = tactical_consequence_receipt(&consequences);
    let submitted = match state.submit_terminal(resolution, receipt, now, |resolution, receipt| {
        conn.reducers().end_tactical_server(resolution, receipt)
    }) {
        Ok(submitted) => submitted,
        Err(error) => {
            warn!(
                "Terminal result submission failed; retrying in {}s: {error}",
                TERMINAL_RETRY_BACKOFF.as_secs()
            );
            return Ok(());
        }
    };
    if !submitted {
        return Ok(());
    }
    info!(
        ?resolution,
        "Mission terminal result committed; shutting down"
    );
    exit.write(AppExit::Success);
    Ok(())
}

fn receipt_body_part(body_part: BodyPart) -> TacticalReceiptBodyPart {
    match body_part {
        BodyPart::LeftArm => TacticalReceiptBodyPart::LeftArm,
        BodyPart::RightArm => TacticalReceiptBodyPart::RightArm,
        BodyPart::LeftLeg => TacticalReceiptBodyPart::LeftLeg,
        BodyPart::RightLeg => TacticalReceiptBodyPart::RightLeg,
        BodyPart::Chest => TacticalReceiptBodyPart::Chest,
        BodyPart::Stomach => TacticalReceiptBodyPart::Stomach,
        BodyPart::Head => TacticalReceiptBodyPart::Head,
    }
}

fn tactical_consequence_receipt(
    accumulated: &TacticalConsequenceAccumulator,
) -> TacticalConsequenceReceipt {
    let mut party: Vec<_> = accumulated
        .party
        .iter()
        .map(|(character_id, consequence)| TacticalCharacterConsequence {
            character_id: *character_id,
            injuries: consequence
                .injuries
                .iter()
                .map(|injury| TacticalHitInjury {
                    body_part: receipt_body_part(injury.body_part),
                    cut_damage: injury.cut_damage,
                    blunt_damage: injury.blunt_damage,
                    max_single_hit_blunt_damage: injury.max_single_hit_blunt_damage,
                })
                .collect(),
            blood_loss_fraction: consequence.blood_loss_fraction,
            ammunition_used: 0,
        })
        .collect();
    for contact in &accumulated.equipment_contacts {
        if !party
            .iter()
            .any(|consequence| consequence.character_id == contact.character_id)
        {
            party.push(TacticalCharacterConsequence {
                character_id: contact.character_id,
                injuries: Vec::new(),
                blood_loss_fraction: 0.0,
                ammunition_used: 0,
            });
        }
    }
    party.sort_by_key(|consequence| consequence.character_id);
    party.truncate(adventuresim_core::mission::MAX_TACTICAL_RECEIPT_PARTICIPANTS);
    TacticalConsequenceReceipt {
        party,
        equipment_contacts: accumulated
            .equipment_contacts
            .iter()
            .map(|contact| TacticalEquipmentContact {
                character_id: contact.character_id,
                inventory_item_id: contact.inventory_item_id,
                contact_stress: contact.contact_stress,
                role: if contact.defender_equipment {
                    TacticalEquipmentContactRole::DefenderEquipment
                } else {
                    TacticalEquipmentContactRole::AttackerWeapon
                },
            })
            .collect(),
    }
}

fn empty_tactical_consequence_receipt() -> TacticalConsequenceReceipt {
    TacticalConsequenceReceipt {
        party: Vec::new(),
        equipment_contacts: Vec::new(),
    }
}

fn check_terminal_combat_outcome(
    time: Res<Time>,
    conn: Res<SpacetimeDb>,
    consequences: Res<TacticalConsequenceAccumulator>,
    mut state: ResMut<MissionState>,
    exit: MessageWriter<AppExit>,
    enemies: Query<(), (With<bot::MissionEnemy>, With<Player>)>,
    combatants: Query<(&TacticalCombatSide, &TacticalCombatState, &PlayerId), With<Player>>,
    loading_players: Query<(), With<LoadingPlayer>>,
) -> Result {
    if state.committed {
        return Ok(());
    }
    // A result whose first submission failed is already decided. Retry it
    // before observing recoveries, disconnects, or any newly terminal state.
    if let Some(resolution) = state.pending_resolution {
        return commit_terminal_resolution(
            resolution,
            time.elapsed(),
            conn,
            consequences,
            state,
            exit,
        );
    }
    let mut loaded_party = 0;
    let mut incapacitated_party = 0;
    for (side, combat_state, player_id) in &combatants {
        if *side == TacticalCombatSide::Party {
            loaded_party += 1;
            incapacitated_party += u32::from(combat_state.incapacitated);
            state.seen_party_members.insert(player_id.0);
        }
    }
    let has_loading_player = !loading_players.is_empty();
    state.enrollment_begun |= has_loading_player || !state.seen_party_members.is_empty();
    if !state.enrollment_sealed
        && enrollment_ready(
            state.expected_party_members,
            state.seen_party_members.len(),
            has_loading_player,
        )
    {
        state.enrollment_sealed = true;
        info!(
            expected = state.expected_party_members,
            "Party enrollment sealed"
        );
    }
    let enrollment_begun = state.enrollment_begun;
    if abandonment_due(
        &mut state.abandonment_elapsed,
        enrollment_begun,
        loaded_party,
        has_loading_player,
        time.delta(),
    ) {
        return commit_terminal_resolution(
            TacticalMissionResolution::Failed,
            time.elapsed(),
            conn,
            consequences,
            state,
            exit,
        );
    }
    let snapshot = TerminalCombatSnapshot {
        required_enemies: state.required_enemy_defeats,
        loaded_enemies: enemies.iter().count() as u32,
        defeated_enemies: state.enemies_defeated,
        loaded_party,
        incapacitated_party,
        enrollment_sealed: state.enrollment_sealed && !has_loading_player,
    };
    let Some(resolution) = terminal_resolution(snapshot) else {
        return Ok(());
    };
    commit_terminal_resolution(resolution, time.elapsed(), conn, consequences, state, exit)
}

fn check_mission_timeout(
    time: Res<Time>,
    conn: Res<SpacetimeDb>,
    consequences: Res<TacticalConsequenceAccumulator>,
    mut state: ResMut<MissionState>,
    exit: MessageWriter<AppExit>,
) -> Result {
    let is_timeout = match state.timeout {
        Some(ref mut timer) => {
            timer.tick(time.delta());
            timer.is_finished()
        }
        None => false,
    };

    if !is_timeout || state.committed {
        return Ok(());
    }

    info!("Mission timeout, committing bounded failure fallback");
    commit_terminal_resolution(
        TacticalMissionResolution::Failed,
        time.elapsed(),
        conn,
        consequences,
        state,
        exit,
    )
}

fn on_server_started(
    args: Res<Args>,
    conn: Res<SpacetimeDb>,
    mut commands: Commands,
    server_addr: Single<&LocalAddr, With<AdventureSimulatorServer>>,
) -> Result {
    info!("Server opened on {:?}", **server_addr);
    info!("Creating a game scene for {}", args.scene_key);

    let mut generator = TerrainGenerator::from_hash((&args.mission_id, &args.scene_key));
    let (scene_height, gen_period) = match args.scene_key.as_str() {
        "hills" => (30, 200.0),
        "desert" => (2, 30.0),
        id => {
            warn!("Unknown scene: {id}");
            (0, 1.0)
        }
    };
    generator.period = gen_period;
    let terrain = generator.generate(args.scene_width, scene_height, args.scene_depth);
    let terrain_collider = terrain.collider();

    commands.spawn((
        Replicated,
        SceneId(args.scene_key.clone()),
        terrain,
        RigidBody::Static,
        terrain_collider,
        Transform::default(),
    ));

    let scene_width = args.scene_width as f32;
    let scene_depth = args.scene_depth as f32;
    commands.spawn((
        RigidBody::Static,
        Transform::default(),
        children![
            (
                Collider::half_space(Vec3::X),
                Transform::from_xyz(-scene_width * 0.5, 0.0, 0.0),
            ),
            (
                Collider::half_space(Vec3::NEG_X),
                Transform::from_xyz(scene_width * 0.5, 0.0, 0.0),
            ),
            (
                Collider::half_space(Vec3::Z),
                Transform::from_xyz(0.0, 0.0, -scene_depth * 0.5),
            ),
            (
                Collider::half_space(Vec3::NEG_Z),
                Transform::from_xyz(0.0, 0.0, scene_depth * 0.5),
            )
        ],
    ));

    info!("Creating tactical server in stdb...");

    conn.reducers().create_tactical_server_for_request(
        args.mission_id.clone(),
        args.tactical_claim.clone(),
        args.addr.to_string(),
        default(),
    )?;

    if args.required_enemy_kills > 0 {
        info!(
            "Requesting {} mission enemies...",
            args.required_enemy_kills
        );
        for _ in 0..args.required_enemy_kills {
            conn.reducers()
                .create_temporary_character(conn.identity())?;
        }
    }

    Ok(())
}

fn on_join_request(
    join: On<FromClient<JoinRequest>>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
    loading_players: Query<(), With<LoadingPlayer>>,
    players: Query<(), With<Player>>,
    conn: Res<SpacetimeDb>,
) -> Result {
    let Some(client) = join.client_id.entity() else {
        return Ok(());
    };

    if loading_players.contains(client) || players.contains(client) {
        return Ok(());
    }

    // JoinRequest carries a character id chosen by the client. The strategic
    // reducer therefore treats it only as a request to enroll an existing
    // member of this mission's authoritative party; it never creates a row.
    // Until the netcode authenticates character ownership, deployments must
    // keep tactical clients within the trusted mission boundary.
    conn.reducers()
        .enter_mission(join.player_id, conn.identity())?;

    state.enrollment_begun = true;
    commands.entity(client).insert(LoadingPlayer {
        requested_player_id: join.player_id,
    });

    info!(
        "Character {} connected and entered mission, awaiting loading",
        join.player_id
    );

    Ok(())
}

fn on_player_input(
    input: On<FromClient<PlayerInputRequest>>,
    mut players: Query<
        (
            &mut AccumulatedInput,
            &mut CharacterLook,
            &TacticalCombatState,
        ),
        With<Player>,
    >,
) {
    let Some(entity) = input.client_id.entity() else {
        return;
    };

    let Ok((mut accumulated_input, mut look, combat_state)) = players.get_mut(entity) else {
        return;
    };
    if combat_state.incapacitated {
        accumulated_input.last_movement = None;
        accumulated_input.jumped = None;
        return;
    }

    look.yaw = input.look.x;
    look.pitch = input.look.y.clamp(-1.5, 1.5);

    accumulated_input.last_movement = input.movement.map(|m| m.clamp_length_max(1.0));

    if input.jump {
        accumulated_input.jumped = Some(Stopwatch::new());
    }
}

fn on_client_disconnected(
    disconnected: On<Disconnected>,
    query: Query<(Option<&PlayerId>, Option<&LoadingPlayer>)>,
    conn: Res<SpacetimeDb>,
) -> Result {
    let entity = disconnected.event_target();
    let Ok((player_id, loading)) = query.get(entity) else {
        return Ok(());
    };

    let Some(character_id) = player_id
        .map(|player_id| player_id.0)
        .or_else(|| loading.map(|loading| loading.requested_player_id))
    else {
        return Ok(());
    };

    conn.reducers().leave_mission(character_id)?;

    match &disconnected.reason {
        DisconnectReason::ByUser(reason) => {
            info!("Character {character_id} disconnected by server request: {reason}");
        }
        DisconnectReason::ByPeer(reason) => {
            info!("Character {character_id} disconnected by peer: {reason}");
        }
        DisconnectReason::ByError(error) => {
            warn!("Character {character_id} disconnected due to error: {error:#}");
        }
    }

    Ok(())
}

fn player_collider() -> Collider {
    Collider::cylinder(0.4, 1.9)
}

fn player_spawn_offset(collider: &Collider) -> f32 {
    -collider.aabb(default(), Rotation::default()).min.y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> TerminalCombatSnapshot {
        TerminalCombatSnapshot {
            required_enemies: 2,
            loaded_enemies: 2,
            defeated_enemies: 0,
            loaded_party: 2,
            incapacitated_party: 0,
            enrollment_sealed: true,
        }
    }

    #[test]
    fn terminal_resolution_waits_for_both_sides_to_load() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                loaded_enemies: 1,
                ..snapshot()
            }),
            None
        );
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                loaded_party: 0,
                ..snapshot()
            }),
            None
        );
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                enrollment_sealed: false,
                ..snapshot()
            }),
            None
        );
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                required_enemies: 0,
                loaded_enemies: 0,
                ..snapshot()
            }),
            None
        );
    }

    #[test]
    fn terminal_resolution_reports_immediate_victory() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                defeated_enemies: 2,
                ..snapshot()
            }),
            Some(TacticalMissionResolution::Defeated)
        );
    }

    #[test]
    fn terminal_resolution_reports_immediate_party_defeat() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                incapacitated_party: 2,
                ..snapshot()
            }),
            Some(TacticalMissionResolution::Failed)
        );
    }

    #[test]
    fn simultaneous_defeat_deterministically_fails() {
        assert_eq!(
            terminal_resolution(TerminalCombatSnapshot {
                defeated_enemies: 2,
                incapacitated_party: 2,
                ..snapshot()
            }),
            Some(TacticalMissionResolution::Failed)
        );
    }

    #[test]
    fn no_timeout_mission_can_claim_exactly_one_terminal_result() {
        let mut state = MissionState {
            timeout: None,
            enemies_defeated: 2,
            required_enemy_defeats: 2,
            expected_party_members: 1,
            seen_party_members: HashSet::from([7]),
            enrollment_begun: true,
            enrollment_sealed: true,
            abandonment_elapsed: Duration::ZERO,
            terminal_retry_not_before: Duration::ZERO,
            pending_resolution: None,
            pending_receipt: None,
            committed: false,
        };
        let resolution = terminal_resolution(TerminalCombatSnapshot {
            defeated_enemies: state.enemies_defeated,
            ..snapshot()
        })
        .unwrap();

        assert_eq!(
            state.submit_terminal(
                resolution,
                empty_tactical_consequence_receipt(),
                Duration::ZERO,
                |_, _| Ok::<_, ()>(()),
            ),
            Ok(true)
        );
        assert_eq!(
            state.submit_terminal(
                resolution,
                empty_tactical_consequence_receipt(),
                Duration::ZERO,
                |_, _| Ok::<_, ()>(()),
            ),
            Ok(false)
        );
    }

    #[test]
    fn failed_submission_retries_frozen_result_after_predicate_clears() {
        let mut state = MissionState {
            timeout: None,
            enemies_defeated: 1,
            required_enemy_defeats: 1,
            expected_party_members: 1,
            seen_party_members: HashSet::from([7]),
            enrollment_begun: true,
            enrollment_sealed: true,
            abandonment_elapsed: Duration::ZERO,
            terminal_retry_not_before: Duration::ZERO,
            pending_resolution: None,
            pending_receipt: None,
            committed: false,
        };
        let mut attempts = 0;
        let mut reports = Vec::new();
        let mut receipts = Vec::new();
        let frozen_receipt = TacticalConsequenceReceipt {
            party: vec![TacticalCharacterConsequence {
                character_id: 7,
                injuries: vec![TacticalHitInjury {
                    body_part: TacticalReceiptBodyPart::Chest,
                    cut_damage: 0.2,
                    blunt_damage: 0.0,
                    max_single_hit_blunt_damage: 0.0,
                }],
                blood_loss_fraction: 0.1,
                ammunition_used: 0,
            }],
            equipment_contacts: Vec::new(),
        };
        let mut sender = |resolution, receipt| {
            attempts += 1;
            reports.push(resolution);
            receipts.push(receipt);
            (attempts > 1).then_some(()).ok_or("offline")
        };

        assert_eq!(
            state.submit_terminal(
                TacticalMissionResolution::Defeated,
                frozen_receipt.clone(),
                Duration::ZERO,
                &mut sender
            ),
            Err("offline")
        );
        let current_resolution = terminal_resolution(TerminalCombatSnapshot {
            defeated_enemies: 0,
            ..snapshot()
        });
        assert_eq!(current_resolution, None, "the original predicate recovered");
        let retry_resolution = state
            .pending_resolution
            .or(current_resolution)
            .expect("failed submission remains pending");
        assert_eq!(
            state.submit_terminal(
                retry_resolution,
                empty_tactical_consequence_receipt(),
                Duration::from_millis(999),
                &mut sender,
            ),
            Ok(false)
        );
        assert_eq!(
            state.submit_terminal(
                retry_resolution,
                empty_tactical_consequence_receipt(),
                Duration::from_secs(1),
                &mut sender,
            ),
            Ok(true)
        );
        assert_eq!(
            state.submit_terminal(
                TacticalMissionResolution::Defeated,
                empty_tactical_consequence_receipt(),
                Duration::from_secs(2),
                &mut sender
            ),
            Ok(false)
        );
        assert_eq!(attempts, 2);
        assert_eq!(
            reports,
            vec![
                TacticalMissionResolution::Defeated,
                TacticalMissionResolution::Defeated
            ]
        );
        assert_eq!(receipts, vec![frozen_receipt.clone(), frozen_receipt]);
    }

    #[test]
    fn sealed_empty_party_fails_only_after_reconnection_grace() {
        let mut elapsed = Duration::ZERO;
        assert!(!abandonment_due(
            &mut elapsed,
            true,
            0,
            false,
            Duration::from_secs(9)
        ));
        assert!(!abandonment_due(
            &mut elapsed,
            true,
            0,
            true,
            Duration::from_secs(1)
        ));
        assert_eq!(elapsed, Duration::ZERO);
        assert!(abandonment_due(
            &mut elapsed,
            true,
            0,
            false,
            PARTY_RECONNECT_GRACE
        ));
    }

    #[test]
    fn pending_second_party_member_prevents_enrollment_seal() {
        assert!(!enrollment_ready(2, 1, false));
        assert!(!enrollment_ready(2, 2, true));
        assert!(enrollment_ready(2, 2, false));
    }

    #[test]
    fn partially_enrolled_party_abandons_after_every_client_disconnects() {
        let expected = 2;
        let seen = HashSet::from([7]);
        assert!(!enrollment_ready(expected, seen.len(), true));

        let enrollment_begun = !seen.is_empty();
        let mut elapsed = Duration::ZERO;
        // B is still represented by LoadingPlayer, so its disconnect resets
        // rather than advances the grace period.
        assert!(!abandonment_due(
            &mut elapsed,
            enrollment_begun,
            1,
            true,
            Duration::from_secs(5)
        ));
        // B's LoadingPlayer and then A's loaded entity disappear. Enrollment
        // was already begun, so the same bounded abandonment policy applies.
        assert!(abandonment_due(
            &mut elapsed,
            enrollment_begun,
            0,
            false,
            PARTY_RECONNECT_GRACE
        ));
    }

    #[test]
    fn never_joined_timeout_disabled_server_does_not_abandon() {
        let mut elapsed = Duration::ZERO;
        assert!(!abandonment_due(
            &mut elapsed,
            false,
            0,
            false,
            PARTY_RECONNECT_GRACE.saturating_mul(2)
        ));
        assert_eq!(elapsed, Duration::ZERO);
    }

    #[test]
    fn weapon_contact_owner_is_included_without_an_injury() {
        let mut accumulated = TacticalConsequenceAccumulator::default();
        accumulated
            .equipment_contacts
            .push(combat::AccumulatedEquipmentContact {
                character_id: 7,
                inventory_item_id: 99,
                contact_stress: 12.0,
                defender_equipment: false,
            });

        let receipt = tactical_consequence_receipt(&accumulated);
        assert_eq!(receipt.party.len(), 1);
        assert_eq!(receipt.party[0].character_id, 7);
        assert!(receipt.party[0].injuries.is_empty());
        assert_eq!(receipt.equipment_contacts.len(), 1);
    }
}
