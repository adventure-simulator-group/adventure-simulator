//! Tactical Server - Replicon + Aeronet websocket game server

mod bot;
mod combat;
mod stdb;
mod terrain;

use std::{net::SocketAddr, num::NonZeroU32};

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

use crate::{bot::MissionEnemy, stdb::SpacetimeDb, terrain::TerrainGenerator};
use input::AccumulatedInput;

/// Default [`Args::timeout`] time.
const MISSION_TIMEOUT_SECS: f32 = 300.0;

/// Level map size.
const TERRAIN_SIZE: usize = 100;

/// Transient mission projection: durable Characters keep their strategic
/// baseline while tactical combat receives mission difficulty/escalation.
fn mission_enemy_scale(difficulty: i32, combat_scale_bps: u32, countermeasure_bps: u32) -> f32 {
    let difficulty_scale = 1.0 + (difficulty.saturating_sub(1).max(0) as f32 * 0.05);
    difficulty_scale * (combat_scale_bps as f32 / 10_000.0) * (countermeasure_bps as f32 / 10_000.0)
}

#[derive(Component)]
struct MissionOpeningAwareness {
    party_has_surprise: bool,
}

fn tactical_covered_parts(parts: &[EquipmentBodyPart]) -> [bool; 7] {
    let mut covered = [false; 7];
    for part in parts {
        let index = match part {
            EquipmentBodyPart::LeftArm => 0,
            EquipmentBodyPart::RightArm => 1,
            EquipmentBodyPart::LeftLeg => 2,
            EquipmentBodyPart::RightLeg => 3,
            EquipmentBodyPart::Chest => 4,
            EquipmentBodyPart::Stomach => 5,
            EquipmentBodyPart::Head => 6,
        };
        covered[index] = true;
    }
    covered
}

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
            enemies_killed: 0,
            required_enemy_kills: args.required_enemy_kills,
            committed: false,
        })
        .insert_resource(args)
        .add_systems(
            Update,
            (
                check_mission_timeout,
                spawn_connected_players.after(stdb::update_spacetimedb),
                exclude_surrendered_participants.after(stdb::update_spacetimedb),
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

#[derive(Resource)]
pub struct MissionState {
    timeout: Option<Timer>,
    pub enemies_killed: u32,
    required_enemy_kills: u32,
    committed: bool,
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

fn exclude_surrendered_participants(
    conn: Res<SpacetimeDb>,
    mut cmd: Commands,
    participants: Query<(Entity, &PlayerId), With<MissionEnemy>>,
) {
    for exclusion in conn.take_participant_exclusions() {
        if let Some((entity, _)) = participants
            .iter()
            .find(|(_, id)| id.0 == exclusion.character_id)
        {
            info!(
                "Removing surrendered tactical participant {}",
                exclusion.character_id
            );
            cmd.entity(entity).despawn();
        }
    }
}

fn spawn_connected_player(
    player: &ConnectedPlayer,
    cmd: &mut Commands,
    q_loading: &Query<(Entity, &LoadingPlayer)>,
    q_scene: &Query<&SceneTerrain>,
) {
    let entity = if player.mission_side == TacticalMissionSide::Enemy {
        cmd.spawn(MissionEnemy).id()
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

    let mut skills = Skills {
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
    let mut limbs = Limbs {
        left_arm: player.limbs.left_arm_health,
        right_arm: player.limbs.right_arm_health,
        left_leg: player.limbs.left_leg_health,
        right_leg: player.limbs.right_leg_health,
        chest: player.limbs.chest_health,
        stomach: player.limbs.stomach_health,
        head: player.limbs.head_health,
    };
    let mut attributes = Attributes {
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
    if player.mission_side == TacticalMissionSide::Enemy {
        let scale = mission_enemy_scale(
            player.enemy_difficulty,
            player.enemy_combat_scale_bps,
            player.countermeasure_multiplier_bps,
        );
        for hours in [
            &mut skills.polearm_hours,
            &mut skills.axe_hours,
            &mut skills.bludgeon_hours,
            &mut skills.sword_hours,
            &mut skills.knife_hours,
            &mut skills.dodge_hours,
            &mut skills.block_hours,
            &mut skills.bow_hours,
            &mut skills.crossbow_hours,
            &mut skills.firearm_hours,
            &mut skills.throw_hours,
        ] {
            *hours *= scale;
        }
        for attribute in [
            &mut attributes.endurance,
            &mut attributes.gut,
            &mut attributes.instinct,
            &mut attributes.eyesight,
            &mut attributes.hearing,
            &mut attributes.left_arm_strength,
            &mut attributes.right_arm_strength,
            &mut attributes.left_leg_strength,
            &mut attributes.right_leg_strength,
            &mut attributes.left_arm_agility,
            &mut attributes.right_arm_agility,
            &mut attributes.left_leg_agility,
            &mut attributes.right_leg_agility,
        ] {
            *attribute *= scale;
        }
        for health in [
            &mut limbs.left_arm,
            &mut limbs.right_arm,
            &mut limbs.left_leg,
            &mut limbs.right_leg,
            &mut limbs.chest,
            &mut limbs.stomach,
            &mut limbs.head,
        ] {
            *health *= scale;
        }
    }
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
        MissionOpeningAwareness {
            party_has_surprise: player.party_has_surprise,
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
            ItemOf(entity),
            ItemQuantity(quantity),
            ItemProperties {
                weight: item.item.weight,
                id: item.item.id.clone(),
            },
        ));
        item_cmd.insert(EquipmentTopology {
            placement_id: item.selected_placement_id.clone(),
            occupancies: item
                .occupancies
                .iter()
                .map(|occupancy| EquipmentTopologyOccupancy {
                    occupancy_id: occupancy.id.clone(),
                    anchor_kind: format!("{:?}", occupancy.anchor_kind),
                    location: occupancy.location.map(|location| format!("{location:?}")),
                    parent_inventory_item_id: occupancy.parent_inventory_item_id,
                    attachment_point_id: occupancy.attachment_point_id.clone(),
                    channel: format!("{:?}", occupancy.channel),
                    order: occupancy.order,
                    requirement_index: occupancy.requirement_index,
                    capacity_index: occupancy.capacity_index,
                })
                .collect(),
        });

        match item.item.kind {
            ItemKind::Simple
            | ItemKind::Container
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
            ItemKind::Armor | ItemKind::Clothing => {}
            ItemKind::Shield => {
                item_cmd.insert(ShieldItem {
                    block: item.item.block,
                });
            }
        }
        if let Some(part) = item.protected_body_parts.first() {
            let slot = match part {
                EquipmentBodyPart::LeftArm => ArmorSlot::Arms(Some(ArmorSide::Left)),
                EquipmentBodyPart::RightArm => ArmorSlot::Arms(Some(ArmorSide::Right)),
                EquipmentBodyPart::LeftLeg => ArmorSlot::Legs(Some(ArmorSide::Left)),
                EquipmentBodyPart::RightLeg => ArmorSlot::Legs(Some(ArmorSide::Right)),
                EquipmentBodyPart::Head => ArmorSlot::Head,
                EquipmentBodyPart::Chest => ArmorSlot::Chest,
                EquipmentBodyPart::Stomach => ArmorSlot::Stomach,
            };
            item_cmd.insert(ArmorItem {
                range_of_motion: item.item.range_of_motion,
                coverage: item.item.coverage,
                slot,
                resistance: item.item.resistance,
                padding: item.item.padding,
                flexibility: item.item.flexibility,
                covered_parts: tactical_covered_parts(&item.protected_body_parts),
            });
        }

        if item.occupancies.iter().any(|occupancy| {
            occupancy.channel == EquipmentChannel::Held
                && occupancy.location == Some(EquipmentLocation::LeftHand)
        }) {
            item_cmd.insert(EquipSlot::HoldingLeft);
        } else if item.occupancies.iter().any(|occupancy| {
            occupancy.channel == EquipmentChannel::Held
                && occupancy.location == Some(EquipmentLocation::RightHand)
        }) {
            item_cmd.insert(EquipSlot::HoldingRight);
        }
    }

    info!(
        temorary = player.character.temporary,
        "Player {entity:?} is fully loaded"
    );
}

fn check_mission_timeout(
    time: Res<Time>,
    conn: Res<SpacetimeDb>,
    mut state: ResMut<MissionState>,
    mut exit: MessageWriter<AppExit>,
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

    info!("Mission timeout, committing results...");
    state.committed = true;

    let success =
        bot::mission_objective_satisfied(state.required_enemy_kills, state.enemies_killed);
    let xp_gained = (state.enemies_killed * 25) as i32;

    let resolution = if success {
        TacticalMissionResolution::Defeated
    } else {
        TacticalMissionResolution::Failed
    };
    conn.reducers().end_tactical_server(resolution, xp_gained)?;

    info!("Mission ended successfully");
    info!("Shutting down");
    exit.write(AppExit::Success);
    Ok(())
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

    // Strategic authority enrolls the mission's exact durable enemy roster as
    // part of server creation. ConnectedPlayer delivery spawns those rows.

    Ok(())
}

fn on_join_request(
    join: On<FromClient<JoinRequest>>,
    mut commands: Commands,
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
    mut players: Query<(&mut AccumulatedInput, &mut CharacterLook), With<Player>>,
) {
    let Some(entity) = input.client_id.entity() else {
        return;
    };

    let Ok((mut accumulated_input, mut look)) = players.get_mut(entity) else {
        return;
    };

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
mod mission_projection_tests {
    use super::mission_enemy_scale;

    #[test]
    fn same_durable_enemy_identity_projects_different_mission_strength() {
        let baseline = mission_enemy_scale(1, 10_000, 10_000);
        let escalated = mission_enemy_scale(4, 13_000, 10_000);
        let countered = mission_enemy_scale(4, 13_000, 7_500);
        assert_eq!(baseline, 1.0);
        assert!(escalated > baseline);
        assert!(countered < escalated);
    }
}
