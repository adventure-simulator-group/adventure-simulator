//! Tactical Server - Replicon + Aeronet websocket game server

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

use crate::{stdb::SpacetimeDb, terrain::TerrainGenerator};
use input::AccumulatedInput;

/// Default [`Args::timeout`] time.
const MISSION_TIMEOUT_SECS: f32 = 300.0;

/// Level map size.
const TERRAIN_SIZE: usize = 100;

#[derive(Parser, Debug, Clone, Resource)]
#[command(name = "adventuresim-tactical-server")]
#[command(about = "Tactical mission server for Adventure Simulator")]
struct Args {
    /// Address to listen on
    #[arg(long, default_value = "127.0.0.1:6000")]
    addr: SocketAddr,

    /// Whether or not the server should be created for the associated request.
    #[arg(long)]
    requested: bool,

    /// Unique mission instance ID
    #[arg(long)]
    mission_id: String,

    /// Scene key (e.g., "hills", "desert")
    #[arg(long)]
    scene_key: String,

    /// Scene allowed physical width (x-size).
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_width: usize,

    /// Scene allowed physical depth (z-size).
    #[arg(long, default_value_t = TERRAIN_SIZE)]
    scene_depth: usize,

    /// Amount of bots to spawn.
    #[arg(long, default_value_t = 0)]
    bots: usize,

    /// SpacetimeDB URI (e.g., http://localhost:3000)
    #[arg(long, default_value = "http://localhost:3000")]
    spacetimedb_url: String,

    /// SpacetimeDB module name
    #[arg(long, default_value = "strategic-stdb-module")]
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
        .add_plugins((
            bevy::app::PanicHandlerPlugin,
            bevy::app::TaskPoolPlugin::default(),
            bevy::log::LogPlugin {
                filter: "tactical_server=info,bevy_app=warn,bevy_ecs=warn".to_string(),
                ..default()
            },
            bevy::time::TimePlugin,
            bevy::transform::TransformPlugin,
            bevy::app::ScheduleRunnerPlugin::default(),
            #[cfg(any(all(unix, not(target_os = "horizon")), windows))]
            bevy::app::TerminalCtrlCHandlerPlugin,
        ))
        .add_plugins((
            AdventureSimulatorCorePlugins
                .build()
                .set(AdventureSimulatorPhysicsPlugin {
                    enable_simulation: true,
                }),
            AdventureSimulatorNetPlugins,
        ))
        .add_plugins((stdb::SpacetimeDbPlugin, combat::CombatPlugin))
        .insert_resource(MissionState {
            timeout: (!args.no_timeout)
                .then_some(args.timeout)
                .map(|duration| Timer::from_seconds(duration, TimerMode::Once)),
            enemies_killed: 0,
            committed: false,
        })
        .insert_resource(args)
        .add_systems(
            Update,
            (
                check_mission_timeout,
                (setup_server, setup_stdb_callbacks).run_if(resource_added::<SpacetimeDb>),
            ),
        )
        .add_systems(
            FixedPreUpdate,
            apply_networked_player_input.run_if(in_state(ServerState::Running)),
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

#[derive(Component, Debug, Default, Clone, Copy)]
pub struct NetworkPlayerState {
    pub input: PlayerInputMessage,
    pub previous_jump: bool,
}

#[derive(Resource)]
struct MissionState {
    timeout: Option<Timer>,
    enemies_killed: u32,
    committed: bool,
}

fn setup_server(mut commands: Commands, args: Res<Args>) {
    info!(
        "Starting tactical server for mission '{}'...",
        args.mission_id
    );
    info!("Scene: {}, Address: {}", args.scene_key, args.addr);

    commands.spawn(AdventureSimulatorServer { addr: args.addr });

    if !args.no_timeout {
        info!("Will timeout in {} seconds", args.timeout);
    }
}

fn setup_stdb_callbacks(mut conn: ResMut<SpacetimeDb>) {
    conn.subscribe("SELECT * FROM connected_players");
    conn.on_insert(
        RemoteTables::connected_players,
        on_stdb_insert_connected_players,
    );
}

fn on_stdb_insert_connected_players(
    InRef(player): InRef<ConnectedPlayer>,
    mut cmd: Commands,
    q_loading: Query<(Entity, &LoadingPlayer)>,
    q_scene: Query<&SceneTerrain>,
) {
    let entity = if player.character.temporary {
        cmd.spawn_empty().id()
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
        melee_hours: player.skills.melee_hours,
        dodge_hours: player.skills.dodge_hours,
        block_hours: player.skills.block_hours,
    };
    let limbs = Limbs {
        left_arm: player.limbs.left_arm,
        right_arm: player.limbs.right_arm,
        left_leg: player.limbs.left_leg,
        right_leg: player.limbs.right_leg,
        chest: player.limbs.chest,
        stomach: player.limbs.stomach,
        head: player.limbs.head,
    };
    let attributes = Attributes {
        endurance: player.attrs.endurance,
        immunity: player.attrs.immunity,
        gut: player.attrs.gut,
        strength: player.attrs.strength,
        precision: player.attrs.precision,
        agility: player.attrs.agility,
        intelligence: player.attrs.intelligence,
        instinct: player.attrs.instinct,
        eyesight: player.attrs.eyesight,
        hearing: player.attrs.hearing,
    };
    let stats = Stats {
        calories_used: player.stats.calories_used,
        focus: player.stats.focus,
    };

    let player_collider = player_collider();
    let spawn_height = q_scene
        .iter()
        .next()
        .and_then(|terrain| terrain.height_at(Vec2::ZERO))
        .unwrap_or_default()
        + player_spawn_offset(&player_collider);

    let spawn_position = Vec3::new(
        rand::random_range(-5.0..5.0),
        spawn_height,
        rand::random_range(-5.0..5.0),
    );
    cmd.entity(entity).remove::<LoadingPlayer>().insert((
        Replicated,
        Player {
            name: player.character.name.clone(),
        },
        PlayerId(player.character.id),
        skills,
        limbs,
        attributes,
        stats,
        CharacterController::default(),
        CharacterLook::default(),
        player_collider,
        CollisionMargin(0.01),
        Transform::from_translation(spawn_position),
        children![(
            Hitbox,
            Collider::cylinder(0.3, 0.6),
            Transform::from_xyz(0.0, 1.0, 0.0),
        )],
        NetworkPlayerState::default(),
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

        match item.item.kind {
            ItemKind::Simple => {}
            ItemKind::Weapon => {
                item_cmd.insert(WeaponItem {
                    accuracy: item.item.accuracy,
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
                        dodge: item.item.dodge,
                        coverage: item.item.coverage,
                        slot,
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

    let success = state.enemies_killed > 0;
    let xp_gained = (state.enemies_killed * 25) as i32;

    conn.reducers().end_tactical_server(success, xp_gained)?;

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

    if args.requested {
        conn.reducers().create_tactical_server_for_request(
            args.mission_id.clone(),
            args.addr.to_string(),
            default(),
        )?;
    } else {
        conn.reducers().create_tactical_server(
            args.mission_id.clone(),
            args.scene_key.clone(),
            args.addr.to_string(),
            default(),
        )?;
    }

    if args.bots > 0 {
        info!("Requesting {} bots...", args.bots);
        for _ in 0..args.bots {
            conn.reducers()
                .create_temporary_character(conn.identity())?;
        }
    }

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

    conn.reducers().create_character(join.player_id)?;
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
    input: On<FromClient<PlayerInputMessage>>,
    mut players: Query<&mut NetworkPlayerState>,
) {
    let Some(entity) = input.client_id.entity() else {
        return;
    };

    let Ok(mut state) = players.get_mut(entity) else {
        return;
    };

    state.input = **input;
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

fn apply_networked_player_input(
    mut players: Query<
        (
            &mut AccumulatedInput,
            &mut CharacterLook,
            &mut NetworkPlayerState,
        ),
        With<Player>,
    >,
) {
    for (mut accumulated_input, mut look, mut state) in &mut players {
        look.yaw = state.input.look.x;
        look.pitch = state.input.look.y.clamp(-1.5, 1.5);

        accumulated_input.last_movement = Some(state.input.movement.clamp_length_max(1.0));

        if state.input.jump && !state.previous_jump {
            accumulated_input.jumped = Some(Stopwatch::new());
        }

        state.previous_jump = state.input.jump;
    }
}

fn player_collider() -> Collider {
    Collider::cylinder(0.4, 1.2)
}

fn player_spawn_offset(collider: &Collider) -> f32 {
    -collider.aabb(default(), Rotation::default()).min.y
}
