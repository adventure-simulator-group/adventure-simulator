use std::num::NonZeroU32;

use adventuresim_stdb_client::*;
use adventuresim_tactical_core::{inventory::ItemProperties, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{DisconnectReason, Disconnected},
    bevy_replicon::prelude::{FromClient, Replicated},
    prelude::{JoinRequest, PlayerInputRequest},
};
use bevy::prelude::*;
use bevy::time::Stopwatch;

use crate::{
    Args,
    bot::{DefenseChances, MissionEnemy, OffensiveCombatAi},
    combat::{MeleeAttackAuthority, RangedAttackAuthority, TacticalCombatSide},
    mission::MissionState,
    stdb::{SpacetimeDb, SpacetimeDbReady},
};
use input::AccumulatedInput;

/// Player projection completes before condition derivation, so a newly loaded
/// strategically incapacitated character cannot act for one simulation tick
/// with default tactical readiness.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PlayerProjectionSet {
    Spawn,
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct LoadingPlayer {
    pub(crate) requested_character: CharacterId,
}

/// Latest complete movement request accepted from a player. Unlike Ahoy's
/// per-fixed-loop accumulator, this survives missing unreliable input packets
/// until an explicit request replaces it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AuthoritativeMovementIntent(Option<Vec2>);

/// Durable inventory provenance retained only on the authoritative server.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TacticalInventoryItemId(pub u64);

/// Transient mission projection: durable Characters keep their strategic
/// baseline while tactical combat receives mission difficulty/escalation.
fn mission_enemy_scale(difficulty: i32, combat_scale_bps: u32, countermeasure_bps: u32) -> f32 {
    let difficulty_scale = 1.0 + (difficulty.saturating_sub(1).max(0) as f32 * 0.05);
    difficulty_scale * (combat_scale_bps as f32 / 10_000.0) * (countermeasure_bps as f32 / 10_000.0)
}

fn mission_enemy_health_scale(combat_scale_bps: u32, projected_scale: f32) -> f32 {
    if combat_scale_bps == 0 {
        1.0
    } else {
        projected_scale
    }
}

#[derive(Component)]
#[allow(dead_code)]
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

pub(crate) fn spawn_connected_players(
    conn: Res<SpacetimeDb>,
    args: Res<Args>,
    mut cmd: Commands,
    q_loading: Query<(Entity, &LoadingPlayer)>,
    q_scene: Query<&SceneTerrain>,
) {
    for player in conn.take_connected_players() {
        spawn_connected_player(
            &player,
            args.enemy_combat_scale_bps,
            &mut cmd,
            &q_loading,
            &q_scene,
        );
    }
}

fn spawn_connected_player(
    player: &ConnectedPlayer,
    enemy_combat_scale_bps: u32,
    cmd: &mut Commands,
    q_loading: &Query<(Entity, &LoadingPlayer)>,
    q_scene: &Query<&SceneTerrain>,
) {
    let entity = if player.mission_side == TacticalMissionSide::Enemy {
        let mut enemy = cmd.spawn((MissionEnemy, TacticalCombatSide::Enemy));
        if enemy_combat_scale_bps > 0 {
            enemy.insert((OffensiveCombatAi::default(), DefenseChances::default()));
        }
        enemy.id()
    } else {
        let Some((entity, _)) = q_loading
            .iter()
            .find(|(_, id)| id.requested_character.0 == player.character.id)
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
        body_weight_kg: player.body_weight_kg,
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
            enemy_combat_scale_bps,
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
        let health_scale = mission_enemy_health_scale(enemy_combat_scale_bps, scale);
        for health in [
            &mut limbs.left_arm,
            &mut limbs.right_arm,
            &mut limbs.left_leg,
            &mut limbs.right_leg,
            &mut limbs.chest,
            &mut limbs.stomach,
            &mut limbs.head,
        ] {
            *health *= health_scale;
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

    let (starting_incapacitation, starting_blood_fraction) = derive_combat_starting_condition(
        player.strategic_incapacitation,
        player.strategic_pain,
        player.strategic_blood_loss,
        player.current_blood_ml,
        player.maximum_blood_ml,
    );

    cmd.entity(entity).insert(MissionOpeningAwareness {
        party_has_surprise: player.party_has_surprise,
    });
    // Only "core" character data goes here - the same data a world dump
    // carries. Everything else that a character always needs regardless of
    // where its core data came from (physics/controller bundle, replication
    // marker, mid-action authority state) is added by `on_player_added`,
    // triggered the moment `Player` lands on this entity below.
    cmd.entity(entity).remove::<LoadingPlayer>().insert((
        Player {
            name: player.character.name.clone(),
        },
        CharacterId(player.character.id),
        skills,
        limbs,
        attributes,
        stats,
        TacticalCombatState {
            starting_incapacitation,
            starting_blood_fraction,
            ..default()
        },
        if player.mission_side == TacticalMissionSide::Enemy {
            TacticalCombatSide::Enemy
        } else {
            TacticalCombatSide::Party
        },
        Transform::from_xyz(spawn_position.x, spawn_height, spawn_position.y)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
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
                    melee: item.item.melee,
                    ranged: item.item.ranged,
                    blunt: item.item.blunt,
                    slash: item.item.slash,
                    pierce: item.item.pierce,
                    // The strategic item schema (adventuresim-stdb-module)
                    // doesn't author a per-weapon windup yet, unlike
                    // accuracy/reach/balance/etc above - falls back to the
                    // same default `PlayerEquipment::weapon_windup_secs`
                    // uses. Extending the schema to author this per-weapon
                    // is future work, not done here.
                    windup_secs: 0.3,
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

pub(crate) fn on_join_request(
    join: On<FromClient<JoinRequest>>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
    loading_players: Query<(), With<LoadingPlayer>>,
    players: Query<(), With<Player>>,
    conn: Res<SpacetimeDb>,
    ready: Res<SpacetimeDbReady>,
) -> Result {
    let Some(client) = join.client_id.entity() else {
        return Ok(());
    };
    if loading_players.contains(client) || players.contains(client) {
        return Ok(());
    }
    if !state.allows_party_join(join.character_id) {
        warn!(
            character_id = join.character_id.0,
            "Rejected unseen Party join after enrollment sealed"
        );
        return Ok(());
    }
    conn.reducers()
        .enter_mission(join.character_id.0, ready.identity())?;
    state.begin_enrollment();
    commands.entity(client).insert(LoadingPlayer {
        requested_character: join.character_id,
    });
    info!(
        "Character {} connected and entered mission, awaiting loading",
        join.character_id.0
    );
    Ok(())
}

/// Standalone-mode counterpart to [`on_join_request`]: no SpacetimeDB
/// authorization round-trip. [`bind_dumped_character_on_join`] is what turns
/// the resulting [`LoadingPlayer`] into a real character, by matching it
/// against an already-loaded world dump.
#[cfg(feature = "debug")]
pub(crate) fn on_join_request_standalone(
    join: On<FromClient<JoinRequest>>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
    loading_players: Query<(), With<LoadingPlayer>>,
    players: Query<(), With<Player>>,
) -> Result {
    let Some(client) = join.client_id.entity() else {
        return Ok(());
    };
    if loading_players.contains(client) || players.contains(client) {
        return Ok(());
    }
    if !state.allows_party_join(join.character_id) {
        warn!(
            character_id = join.character_id.0,
            "Rejected unseen Party join after enrollment sealed"
        );
        return Ok(());
    }
    state.begin_enrollment();
    commands.entity(client).insert(LoadingPlayer {
        requested_character: join.character_id,
    });
    info!(
        "Character {} connected, awaiting binding to a dumped character",
        join.character_id.0
    );
    Ok(())
}

pub(crate) fn on_player_input(
    input: On<FromClient<PlayerInputRequest>>,
    mut players: Query<
        (
            &mut AccumulatedInput,
            &mut CharacterLook,
            &TacticalCombatState,
            &mut SkeletonState,
            &mut AuthoritativeMovementIntent,
        ),
        With<Player>,
    >,
) {
    let Some(validated) =
        validate_player_input(input.look, input.movement, input.jump, input.weapon_guard)
    else {
        return;
    };
    let Some(entity) = input.client_id.entity() else {
        return;
    };
    let Ok((mut accumulated_input, mut look, combat_state, mut skeleton, mut movement_intent)) =
        players.get_mut(entity)
    else {
        return;
    };
    if combat_state.is_incapacitated() {
        accumulated_input.last_movement = None;
        movement_intent.0 = None;
        accumulated_input.jumped = None;
        set_weapon_guard(
            &mut skeleton,
            authoritative_weapon_guard(validated.weapon_guard, true),
        );
        return;
    }
    look.yaw = validated.yaw;
    look.pitch = validated.pitch;
    accumulated_input.last_movement = validated.movement;
    movement_intent.0 = validated.movement;
    set_weapon_guard(
        &mut skeleton,
        authoritative_weapon_guard(validated.weapon_guard, false),
    );
    if validated.jump {
        accumulated_input.jumped = Some(Stopwatch::new());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ValidatedPlayerInput {
    movement: Option<Vec2>,
    yaw: f32,
    pitch: f32,
    jump: bool,
    weapon_guard: WeaponGuardState,
}

fn validate_player_input(
    look: Vec2,
    movement: Option<Vec2>,
    jump: bool,
    weapon_guard: WeaponGuardState,
) -> Option<ValidatedPlayerInput> {
    if !look.is_finite() || movement.is_some_and(|movement| !movement.is_finite()) {
        return None;
    }
    Some(ValidatedPlayerInput {
        movement: movement.map(|movement| movement.clamp_length_max(1.0)),
        yaw: (look.x + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI,
        pitch: look.y.clamp(-1.5, 1.5),
        jump,
        weapon_guard,
    })
}

fn authoritative_weapon_guard(
    requested: WeaponGuardState,
    incapacitated: bool,
) -> WeaponGuardState {
    if incapacitated {
        WeaponGuardState::Lowered
    } else {
        requested
    }
}

/// Rehydrates Ahoy's disposable fixed-loop input from the latest accepted
/// complete request before movement runs. Ahoy may clear its accumulator after
/// every fixed loop without turning a missing network packet into a stop.
pub(crate) fn restore_authoritative_movement_intent(
    mut players: Query<(&AuthoritativeMovementIntent, &mut AccumulatedInput), With<Player>>,
) {
    for (movement_intent, mut accumulated_input) in &mut players {
        accumulated_input.last_movement = movement_intent.0;
    }
}

/// Projects authoritative controller motion into the compact presentation
/// state replicated to every client. It deliberately never evaluates bones.
pub(crate) fn update_skeleton_locomotion(
    time: Res<Time<Fixed>>,
    mut players: Query<
        (
            &CharacterControllerState,
            &LinearVelocity,
            &mut SkeletonState,
            &mut Transform,
            &TacticalCombatState,
        ),
        With<Player>,
    >,
) {
    for (controller, velocity, mut skeleton, mut transform, combat_state) in &mut players {
        if combat_state.is_incapacitated() {
            let lowered = authoritative_weapon_guard(skeleton.weapon_guard(), true);
            set_weapon_guard(&mut skeleton, lowered);
        }
        let tick = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
        transform.rotation = advance_body_facing(
            transform.rotation,
            controller.orientation,
            velocity.0,
            skeleton.action_kind(),
            skeleton.weapon_guard(),
            time.delta_secs(),
        );
        project_skeleton_locomotion(
            &mut skeleton,
            SkeletonLocomotionInput {
                orientation: controller.orientation,
                linear_velocity: velocity.0,
                grounded: controller.grounded.is_some(),
                crouching: controller.crouching,
                delta_seconds: time.delta_secs(),
                tick,
            },
        );
    }
}

pub(crate) fn on_client_disconnected(
    disconnected: On<Disconnected>,
    query: Query<(Option<&CharacterId>, Option<&LoadingPlayer>)>,
    conn: Res<SpacetimeDb>,
) -> Result {
    let entity = disconnected.event_target();
    let Ok((player_id, loading)) = query.get(entity) else {
        return Ok(());
    };
    let Some(character_id) = player_id
        .map(|id| id.0)
        .or_else(|| loading.map(|id| id.requested_character.0))
    else {
        return Ok(());
    };
    conn.reducers().leave_mission(character_id)?;
    match &disconnected.reason {
        DisconnectReason::ByUser(reason) => {
            info!("Character {character_id} disconnected by server request: {reason}")
        }
        DisconnectReason::ByPeer(reason) => {
            info!("Character {character_id} disconnected by peer: {reason}")
        }
        DisconnectReason::ByError(error) => {
            warn!("Character {character_id} disconnected due to error: {error:#}")
        }
    }
    Ok(())
}

/// Standalone-mode counterpart to [`on_client_disconnected`]: no SpacetimeDB
/// `leave_mission` call.
#[cfg(feature = "debug")]
pub(crate) fn on_client_disconnected_standalone(
    disconnected: On<Disconnected>,
    query: Query<(Option<&CharacterId>, Option<&LoadingPlayer>)>,
) -> Result {
    let entity = disconnected.event_target();
    let Ok((player_id, loading)) = query.get(entity) else {
        return Ok(());
    };
    let Some(character_id) = player_id
        .map(|id| id.0)
        .or_else(|| loading.map(|id| id.requested_character.0))
    else {
        return Ok(());
    };
    match &disconnected.reason {
        DisconnectReason::ByUser(reason) => {
            info!("Character {character_id} disconnected by server request: {reason}")
        }
        DisconnectReason::ByPeer(reason) => {
            info!("Character {character_id} disconnected by peer: {reason}")
        }
        DisconnectReason::ByError(error) => {
            warn!("Character {character_id} disconnected due to error: {error:#}")
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

/// Fires whenever `Player` lands on any entity - via the normal
/// SpacetimeDB-driven [`spawn_connected_player`], a loaded world dump
/// (`load_world_dump`), or a dump-to-live-client merge
/// (`bind_dumped_character_on_join`). Adds everything a character always
/// needs that is never part of its actual data: mid-action authority/
/// cooldown state (correctly starts neutral regardless of source), the
/// physics/controller bundle (colliders aren't reflectable - see
/// `avian3d::Collider`), and the replication marker. This is what lets both
/// SpacetimeDB rows and world dumps carry only "core" reflectable character
/// data (`Player`, `CharacterId`, `Skills`, `Limbs`, `Attributes`, `Stats`,
/// `TacticalCombatState`, `TacticalCombatSide`, `Transform`) instead of a
/// full component-for-component bundle.
pub(crate) fn on_player_added(
    event: On<Add, Player>,
    mut commands: Commands,
    query: Query<(&Player, &CharacterId)>,
) -> Result {
    let (player, character_id) = query.get(event.entity)?;
    commands.entity(event.entity).insert((
        Name::new(format!("Character#{} {}", character_id.0, player.name)),
        Replicated,
        BestiaryCategories::default(),
        MeleeAttackAuthority::default(),
        RangedAttackAuthority::default(),
        player_collider(),
        CollisionMargin(0.01),
        tactical_character_controller(),
        CharacterLook::default(),
        AuthoritativeMovementIntent::default(),
    ));
    Ok(())
}

/// Marks every scene-written inventory item as [`Replicated`]. `Replicated`
/// is deliberately outside the dump's core-data allowlist (a replication-
/// transport concern, not character/level data - see
/// `dump_request_excludes_reflected_components_outside_the_core_allowlist`),
/// so a dump-loaded item never carries it - without this, such an item
/// exists server-side but never replicates to any client. Called by the two
/// scene-writing paths (`load_world_dump`, [`bind_dumped_character_on_join`])
/// right where they already backfill the other things a dump deliberately
/// omits (see `insert_fresh_combatant_extras`); the live SpacetimeDB spawn
/// path inserts `Replicated` explicitly at spawn like everything else it
/// spawns.
#[cfg(feature = "debug")]
pub(crate) fn mark_loaded_items_replicated<'a>(
    world: &mut World,
    loaded: impl Iterator<Item = &'a Entity>,
) {
    for &entity in loaded {
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        if entity_mut.contains::<ItemOf>() {
            entity_mut.insert(Replicated);
        }
    }
}

/// Standalone-mode-only: turns a [`LoadingPlayer`] into a real character by
/// finding an existing entity with a matching [`CharacterId`] (the world
/// dump's placeholder for that character) and transplanting its reflected
/// components directly onto the joining client's connection entity, in one
/// atomic operation - no other system observes a half-merged entity.
///
/// Reuses the same `DynamicSceneBuilder`/`EntityHashMap` trick as
/// `load_world_dump`, just pre-seeding the map with `template -> client`
/// instead of letting it allocate a fresh entity.
#[cfg(feature = "debug")]
pub(crate) fn bind_dumped_character_on_join(world: &mut World) {
    let loading: Vec<(Entity, CharacterId)> = world
        .query::<(Entity, &LoadingPlayer)>()
        .iter(world)
        .map(|(entity, loading)| (entity, loading.requested_character))
        .collect();
    if loading.is_empty() {
        return;
    }
    let mut templates: Vec<(Entity, CharacterId)> = world
        .query_filtered::<(Entity, &CharacterId), Without<LoadingPlayer>>()
        .iter(world)
        .map(|(entity, id)| (entity, *id))
        .collect();

    for (client_entity, character_id) in loading {
        let Some(index) = templates.iter().position(|(_, id)| *id == character_id) else {
            warn!(
                character_id = character_id.0,
                "No dumped character found for joining client; join stays pending"
            );
            continue;
        };
        let (template_entity, _) = templates.swap_remove(index);

        // Inventory items are separate entities linked via `ItemOf`, not
        // part of the character entity itself - they must be extracted
        // alongside it or they're silently left behind.
        let item_entities: Vec<Entity> = world
            .query::<(Entity, &ItemOf)>()
            .iter(world)
            .filter_map(|(entity, item_of)| (item_of.0 == template_entity).then_some(entity))
            .collect();

        let scene = bevy::scene::DynamicSceneBuilder::from_world(world)
            .extract_entities(core::iter::once(template_entity).chain(item_entities))
            .build();
        let mut entity_map = bevy::ecs::entity::EntityHashMap::default();
        entity_map.insert(template_entity, client_entity);
        match scene.write_to_world(world, &mut entity_map) {
            Ok(()) => {
                world.despawn(template_entity);
                world.entity_mut(client_entity).remove::<LoadingPlayer>();
                // `write_to_world` inserted `Player` directly (not via
                // `Commands`), which still triggers `on_player_added`
                // synchronously, but that observer's own `Commands` calls
                // need an explicit flush to actually land before anything
                // else in this same exclusive system reads the entity.
                world.flush();
                mark_loaded_items_replicated(world, entity_map.values());
                info!(
                    character_id = character_id.0,
                    "Bound joining client to dumped character"
                );
            }
            Err(error) => error!(
                ?error,
                character_id = character_id.0,
                "Failed to bind dumped character to joining client"
            ),
        }
    }
}

#[cfg(feature = "debug")]
#[cfg(test)]
mod standalone_join_tests {
    use super::*;

    #[test]
    fn bind_dumped_character_on_join_transplants_reflected_state_and_leaves_bots_alone() {
        let mut app = App::new();
        app.add_observer(on_player_added);
        let world = app.world_mut();

        // The dump's placeholder for the connecting player: distinct
        // Transform/TacticalCombatState from any fresh-join default.
        let template = world
            .spawn((
                Player {
                    name: "Dumped Party Member".to_string(),
                },
                CharacterId(7),
                TacticalCombatSide::Party,
                TacticalCombatState {
                    imbalance: 0.42,
                    ..default()
                },
                Transform::from_xyz(12.0, 0.0, -5.0),
            ))
            .id();

        // A bot that should be left completely untouched.
        let bot = world
            .spawn((
                Player {
                    name: "Bandit".to_string(),
                },
                CharacterId(99),
                MissionEnemy,
                TacticalCombatSide::Enemy,
                OffensiveCombatAi::default(),
            ))
            .id();

        // An inventory item is a separate entity linked via `ItemOf`, not a
        // component on the character itself - it must travel with the
        // merge, with its `ItemOf` reference updated to the new owner. (The
        // original entity id is not what to check afterward: `write_to_world`
        // spawns a *new* entity for anything not pre-seeded in the entity
        // map, same as it does for every other dump-loaded entity.)
        // `ItemOf` first, then `EquipSlot`, matching every live equip path,
        // so the equip hook derives the template's `holding_weapon`.
        let sword = world
            .spawn((
                ItemOf(template),
                ItemQuantity::default(),
                ItemProperties {
                    id: "sword".to_string(),
                    weight: 1.2,
                },
                WeaponItem {
                    skill_weights: [0.0; 9],
                    accuracy: 1.0,
                    penetration: 1.0,
                    reach: 0.8,
                    balance: 0.0,
                    precise: false,
                    melee: true,
                    ranged: false,
                    blunt: false,
                    slash: true,
                    pierce: false,
                    windup_secs: 0.3,
                },
            ))
            .id();
        world.entity_mut(sword).insert(EquipSlot::HoldingRight);

        // The joining client's connection entity.
        let client_entity = world
            .spawn(LoadingPlayer {
                requested_character: CharacterId(7),
            })
            .id();

        bind_dumped_character_on_join(world);

        assert!(
            world.get_entity(template).is_err(),
            "the dump's placeholder should be despawned once merged"
        );
        assert!(!world.entity(client_entity).contains::<LoadingPlayer>());

        let mut items = world.query::<(Entity, &ItemOf, &ItemProperties)>();
        let (merged_sword, item_of, _) = items
            .iter(world)
            .find(|(_, _, properties)| properties.id == "sword")
            .expect("the merged character's inventory item should still exist");
        assert_eq!(
            item_of.0, client_entity,
            "the item's owner reference should be remapped to the joined client, not the despawned template"
        );
        assert!(
            world.entity(merged_sword).contains::<Replicated>(),
            "the merged item must be marked Replicated (dumps deliberately never carry the marker)"
        );

        let merged = world.entity(client_entity);
        // The scene write carries the template's `InventoryItems` verbatim
        // (relationship hooks are silenced during scene application, so
        // nothing would rebuild it) - its refs must be remapped to the
        // merged item entity.
        let inventory = merged
            .get::<InventoryItems>()
            .expect("the merged character's InventoryItems should travel with the merge");
        assert!(
            inventory.iter().any(|item| item == merged_sword),
            "the merged InventoryItems should reference the remapped item entity"
        );
        assert_eq!(
            inventory.holding_weapon(),
            Some(merged_sword),
            "holding_weapon should be remapped to the merged item entity"
        );
        assert_eq!(merged.get::<Player>().unwrap().name, "Dumped Party Member");
        assert_eq!(merged.get::<CharacterId>().unwrap().0, 7);
        assert_eq!(
            merged.get::<TacticalCombatSide>().unwrap(),
            &TacticalCombatSide::Party
        );
        assert_eq!(merged.get::<TacticalCombatState>().unwrap().imbalance, 0.42);
        assert_eq!(
            merged.get::<Transform>().unwrap().translation,
            Vec3::new(12.0, 0.0, -5.0)
        );
        assert!(
            merged.contains::<MeleeAttackAuthority>(),
            "merge should insert fresh combatant extras"
        );

        let bot_entity = world.entity(bot);
        assert!(bot_entity.contains::<MissionEnemy>());
        assert!(bot_entity.contains::<OffensiveCombatAi>());
        assert_eq!(bot_entity.get::<CharacterId>().unwrap().0, 99);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoritativeMovementIntent, Player, WeaponGuardState, authoritative_weapon_guard, input,
        mission_enemy_health_scale, mission_enemy_scale, restore_authoritative_movement_intent,
        tactical_movement_speed_for_guard, validate_player_input,
    };
    use bevy::prelude::*;

    #[test]
    fn same_durable_enemy_identity_projects_different_mission_strength() {
        let baseline = mission_enemy_scale(1, 10_000, 10_000);
        let escalated = mission_enemy_scale(4, 13_000, 10_000);
        let countered = mission_enemy_scale(4, 13_000, 7_500);
        assert_eq!(baseline, 1.0);
        assert!(escalated > baseline);
        assert!(countered < escalated);
    }

    #[test]
    fn zero_combat_scale_keeps_test_enemy_alive() {
        assert_eq!(mission_enemy_health_scale(0, 0.0), 1.0);
        assert_eq!(mission_enemy_health_scale(5_000, 0.5), 0.5);
    }

    #[test]
    fn player_input_rejects_non_finite_look_or_movement_as_one_update() {
        let mut controller_input = (
            Vec2::new(0.4, -0.2),
            Some(Vec2::new(0.25, 0.5)),
            false,
            WeaponGuardState::Lowered,
        );
        for (look, movement) in [
            (Vec2::new(f32::NAN, 0.0), Some(Vec2::ZERO)),
            (Vec2::new(0.0, f32::INFINITY), Some(Vec2::ZERO)),
            (Vec2::ZERO, Some(Vec2::new(f32::NEG_INFINITY, 0.0))),
            (Vec2::ZERO, Some(Vec2::new(0.0, f32::NAN))),
        ] {
            if let Some(validated) =
                validate_player_input(look, movement, true, WeaponGuardState::Raised)
            {
                controller_input = (
                    Vec2::new(validated.yaw, validated.pitch),
                    validated.movement,
                    validated.jump,
                    validated.weapon_guard,
                );
            }
        }
        assert_eq!(
            controller_input,
            (
                Vec2::new(0.4, -0.2),
                Some(Vec2::new(0.25, 0.5)),
                false,
                WeaponGuardState::Lowered,
            )
        );
    }

    #[test]
    fn player_input_normalizes_finite_boundaries_before_controller_state() {
        let validated = validate_player_input(
            Vec2::new(std::f32::consts::TAU * 4.0 + 0.25, 99.0),
            Some(Vec2::splat(10.0)),
            true,
            WeaponGuardState::Raised,
        )
        .unwrap();
        assert!((validated.yaw - 0.25).abs() < 0.0001);
        assert_eq!(validated.pitch, 1.5);
        assert!(validated.movement.unwrap().length() <= 1.0001);
        assert!(validated.yaw.is_finite() && validated.pitch.is_finite());
        assert_eq!(validated.weapon_guard, WeaponGuardState::Raised);
    }

    #[test]
    fn authoritative_guard_accepts_active_input_but_incapacitation_forces_lowered() {
        assert_eq!(
            authoritative_weapon_guard(WeaponGuardState::Raised, false),
            WeaponGuardState::Raised
        );
        assert_eq!(
            authoritative_weapon_guard(WeaponGuardState::Raised, true),
            WeaponGuardState::Lowered
        );
    }

    #[test]
    fn movement_intent_survives_missing_unreliable_packets_until_explicit_stop() {
        let mut world = World::new();
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::X)),
                input::AccumulatedInput::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(restore_authoritative_movement_intent);

        for _missing_packet_tick in 0..4 {
            schedule.run(&mut world);
            let accumulated = world.get::<input::AccumulatedInput>(player).unwrap();
            assert_eq!(accumulated.last_movement, Some(Vec2::X));
            assert_eq!(
                tactical_movement_speed_for_guard(
                    accumulated.last_movement,
                    WeaponGuardState::Lowered
                ),
                5.5
            );
            world
                .get_mut::<input::AccumulatedInput>(player)
                .unwrap()
                .last_movement = None;
        }

        world
            .get_mut::<AuthoritativeMovementIntent>(player)
            .unwrap()
            .0 = None;
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .last_movement,
            None
        );
        assert_eq!(
            tactical_movement_speed_for_guard(None, WeaponGuardState::Lowered),
            0.0
        );
    }
}
