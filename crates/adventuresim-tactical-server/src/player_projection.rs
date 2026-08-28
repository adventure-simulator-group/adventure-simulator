use std::{collections::BTreeMap, num::NonZeroU32};

use adventuresim_core::tactical_fixture::AnimationLabEnemyRole;
use adventuresim_stdb_client::*;
use adventuresim_tactical_core::animation::dive_launch_root_rotation;
use adventuresim_tactical_core::{inventory::ItemProperties, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{DisconnectReason, Disconnected},
    bevy_replicon::prelude::{FromClient, Replicated, SendTargets, ServerTriggerExt, ToClients},
    prelude::{
        DefendRequest, JoinRequest, JumpCommand, PlayerInputRequest, PostureActionRequest,
        PostureCommand, ReconnectCapability, ReconnectToken, TacticalCombatConfigSnapshot,
    },
};
use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use bevy::prelude::*;
use bevy::time::Stopwatch;

use crate::{
    Args, SceneVistaBundleResource,
    bot::{CombatantBehaviorPackages, MissionEnemy},
    combat::{
        MeleeAttackAuthority, PendingMeleeContact, RangedAttackAuthority, TacticalCombatSide,
    },
    equipment::{
        LastEquipmentSequence, PendingEquipmentActions, purge_equipment_lifecycle,
        reconnect_equipment_lifecycle,
    },
    mission::MissionState,
    stdb::{SpacetimeDb, SpacetimeDbReady},
};
use input::AccumulatedInput;

type PlayerInputStateQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static mut AccumulatedInput,
        &'static mut CharacterLook,
        &'static TacticalCombatState,
        &'static mut SkeletonState,
        &'static mut AuthoritativeMovementIntent,
        Option<&'static mut AuthoritativeInputTick>,
        &'static mut AuthoritativePostureIntent,
        &'static mut QuickstepPush,
        &'static mut MovementPace,
        &'static mut LinearVelocity,
        &'static mut Transform,
        &'static mut Rotation,
        Has<StartupInputObserved>,
    ),
    With<Player>,
>;

type MovementIntentQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static AuthoritativeMovementIntent,
        &'static SkeletonState,
        &'static AuthoritativePostureIntent,
        &'static QuickstepPush,
        Option<&'static CharacterControllerState>,
        Option<&'static Transform>,
        &'static mut AccumulatedInput,
    ),
    With<Player>,
>;

type LocomotionQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static CharacterControllerState,
        &'static LinearVelocity,
        &'static mut SkeletonState,
        &'static mut Transform,
        &'static mut Rotation,
        &'static TacticalCombatState,
        &'static MovementPace,
        &'static AuthoritativePostureIntent,
        &'static QuickstepPush,
        &'static AccumulatedInput,
        Option<&'static AttackFacing>,
    ),
    With<Player>,
>;

type QuickstepTraceQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static SkeletonState,
        &'static QuickstepPush,
        &'static Transform,
        &'static LinearVelocity,
        &'static CharacterControllerState,
    ),
    With<Player>,
>;

type CharacterMotionSnapshotQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Transform,
        &'static LinearVelocity,
        &'static CharacterControllerState,
        &'static AuthoritativeInputTick,
        &'static QuickstepPush,
        Option<&'static MeleeLungeMovement>,
        &'static mut CharacterMotionSnapshot,
    ),
    With<Player>,
>;

type ProjectedPlayerCoreQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Name,
        &'static Player,
        &'static CharacterId,
        &'static BestiaryCategories,
        &'static Skills,
        &'static Limbs,
        &'static Attributes,
        &'static Stats,
        &'static TacticalCombatState,
        &'static EquipmentActionState,
        &'static TacticalCombatSide,
    ),
>;

type ProjectedPlayerMotionQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Transform,
        &'static CharacterLook,
        &'static AuthoritativeMovementIntent,
        &'static AuthoritativeInputTick,
        &'static CharacterMotionSnapshot,
        &'static QuickstepPush,
        Option<&'static MeleeLungeMovement>,
        &'static AuthoritativePostureIntent,
        &'static MovementPace,
        &'static Mass,
        &'static LinearVelocity,
        &'static SkeletonState,
        &'static MeleeAttackAuthority,
        Option<&'static PendingMeleeContact>,
        &'static RangedAttackAuthority,
    ),
>;

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

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct StartupInputObserved;

/// Latest complete movement request accepted from a player. Unlike Ahoy's
/// per-fixed-loop accumulator, this survives missing unreliable input packets
/// until an explicit request replaces it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AuthoritativeMovementIntent(pub(crate) Option<Vec2>);

/// Temporary authoritative yaw drive created by attack target acquisition.
/// It follows the target root during windup and expires after canonical contact.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct AttackFacing {
    pub(crate) target: Entity,
    pub(crate) target_position: Vec3,
    pub(crate) contact_tick: u64,
}

pub(crate) fn begin_attack_facing(
    commands: &mut Commands,
    attacker: Entity,
    target: Option<Entity>,
    contact_tick: u64,
    transforms: &Query<&Transform>,
) {
    let Some(target) = target.filter(|target| *target != attacker) else {
        commands.entity(attacker).remove::<AttackFacing>();
        return;
    };
    let Ok(transform) = transforms.get(target) else {
        commands.entity(attacker).remove::<AttackFacing>();
        return;
    };
    commands.entity(attacker).insert(AttackFacing {
        target,
        target_position: transform.translation,
        contact_tick,
    });
}

pub(crate) fn update_attack_facing_targets(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut attackers: Query<(Entity, &mut AttackFacing)>,
    targets: Query<&Transform>,
) {
    let tick = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
    for (entity, mut facing) in &mut attackers {
        if tick > facing.contact_tick {
            commands.entity(entity).remove::<AttackFacing>();
        } else if let Ok(transform) = targets.get(facing.target) {
            facing.target_position = transform.translation;
        } else {
            commands.entity(entity).remove::<AttackFacing>();
        }
    }
}

/// Newest complete continuous-input sample accepted from the unreliable
/// channel. Wrap-aware ordering prevents a delayed packet from restoring stale
/// movement or look intent.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AuthoritativeInputTick {
    tick: u32,
    initialized: bool,
}

impl AuthoritativeInputTick {
    fn accept(&mut self, tick: u32) -> bool {
        if self.initialized && !sequence_is_newer(tick, self.tick) {
            return false;
        }
        self.tick = tick;
        self.initialized = true;
        true
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AuthoritativePostureIntent {
    facing: CameraFacingIntent,
    last_jump_sequence: u32,
    last_command_sequence: u32,
}

/// One camera-facing owner is selected per accepted input. Free downed camera
/// movement never changes body contact; only held aim owns camera-driven rolls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CameraFacingIntent {
    #[default]
    Free,
    Aim,
    DownedAlign,
}

impl CameraFacingIntent {
    fn from_input(weapon_guard: WeaponGuardState, downed_align: bool) -> Self {
        if downed_align {
            Self::DownedAlign
        } else if weapon_guard == WeaponGuardState::Raised {
            Self::Aim
        } else {
            Self::Free
        }
    }
}

#[cfg(test)]
const GROUND_POSTURE_TRANSITION_TICKS: u64 = 51;
#[cfg(test)]
const ROLL_POSTURE_TRANSITION_TICKS: u64 = GROUND_POSTURE_TRANSITION_TICKS.div_ceil(2);
#[cfg(test)]
const BACKWARD_DIVE_POSTURE_TRANSITION_TICKS: u64 = 32;

/// Durable inventory provenance retained only on the authoritative server.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TacticalInventoryItemId(pub u64);

/// Transient mission projection: durable Characters keep their strategic
/// baseline while tactical combat receives mission difficulty/escalation.
fn mission_enemy_scale(difficulty: i32, combat_scale_bps: u32, countermeasure_bps: u32) -> f32 {
    let difficulty_scale = 1.0 + (difficulty.saturating_sub(1).max(0) as f32 * 0.05);
    let whole_bps = f32::from(BASIS_POINTS_PER_WHOLE);
    difficulty_scale
        * (combat_scale_bps as f32 / whole_bps)
        * (countermeasure_bps as f32 / whole_bps)
}

fn mission_enemy_health_scale(combat_scale_bps: u32, projected_scale: f32) -> f32 {
    if combat_scale_bps == 0 {
        1.0
    } else {
        projected_scale
    }
}

#[derive(Component)]
#[expect(
    dead_code,
    reason = "opening awareness is projected for reflected tactical inspection before combat consumes it"
)]
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

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct DisconnectedPlayer {
    character_id: CharacterId,
    reconnect_token: ReconnectToken,
    remaining_secs: f32,
    claimed: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct ReconnectSession {
    character_id: CharacterId,
    token: ReconnectToken,
}

#[derive(Component, Clone)]
pub(crate) enum DisconnectedProjection {
    Loading(LoadingPlayer),
    Projected(Box<ProjectedPlayerSnapshot>),
}

#[derive(Clone)]
pub(crate) struct ProjectedPlayerSnapshot {
    name: Name,
    player: Player,
    character_id: CharacterId,
    bestiary_categories: BestiaryCategories,
    skills: Skills,
    limbs: Limbs,
    attributes: Attributes,
    stats: Stats,
    combat_state: TacticalCombatState,
    equipment_action_state: EquipmentActionState,
    combat_side: TacticalCombatSide,
    transform: Transform,
    look: CharacterLook,
    movement_intent: AuthoritativeMovementIntent,
    input_tick: AuthoritativeInputTick,
    motion_snapshot: CharacterMotionSnapshot,
    quickstep_push: QuickstepPush,
    melee_lunge: Option<MeleeLungeMovement>,
    posture_intent: AuthoritativePostureIntent,
    pace: MovementPace,
    mass: Mass,
    velocity: LinearVelocity,
    skeleton: SkeletonState,
    melee_authority: MeleeAttackAuthority,
    pending_melee_contact: Option<PendingMeleeContact>,
    ranged_authority: RangedAttackAuthority,
    collider: Collider,
    collision_margin: CollisionMargin,
    controller: CharacterController,
    accumulated_input: AccumulatedInput,
}

impl DisconnectedProjection {
    fn projected(&self) -> bool {
        matches!(self, Self::Projected(_))
    }

    fn insert(self, commands: &mut Commands, target: Entity) {
        match self {
            Self::Loading(loading) => {
                commands.entity(target).insert(loading);
            }
            Self::Projected(snapshot) => {
                let snapshot = *snapshot;
                commands.entity(target).insert((
                    snapshot.name,
                    snapshot.player,
                    snapshot.character_id,
                    snapshot.bestiary_categories,
                    snapshot.skills,
                    snapshot.limbs,
                    snapshot.attributes,
                    snapshot.stats,
                    snapshot.combat_state,
                    snapshot.equipment_action_state,
                    snapshot.combat_side,
                ));
                commands.entity(target).insert((
                    snapshot.transform,
                    snapshot.look,
                    snapshot.movement_intent,
                    snapshot.input_tick,
                    snapshot.motion_snapshot,
                    snapshot.quickstep_push,
                    snapshot.posture_intent,
                    snapshot.pace,
                    snapshot.mass,
                    snapshot.velocity,
                    snapshot.skeleton,
                    snapshot.melee_authority,
                    snapshot.ranged_authority,
                ));
                if let Some(pending) = snapshot.pending_melee_contact {
                    commands.entity(target).insert(pending);
                }
                if let Some(melee_lunge) = snapshot.melee_lunge {
                    commands.entity(target).insert(melee_lunge);
                }
                commands.entity(target).insert((
                    snapshot.collider,
                    snapshot.collision_margin,
                    snapshot.controller,
                    snapshot.accumulated_input,
                ));
            }
        }
    }
}

const RECONNECT_GRACE_SECS: f32 = 30.0;

fn tactical_equipment_location(
    location: adventuresim_stdb_client::EquipmentLocation,
) -> adventuresim_core::item_catalog::EquipmentLocation {
    use adventuresim_core::item_catalog::EquipmentLocation as Core;
    use adventuresim_stdb_client::EquipmentLocation as Durable;
    match location {
        Durable::Head => Core::Head,
        Durable::Face => Core::Face,
        Durable::Neck => Core::Neck,
        Durable::Chest => Core::Chest,
        Durable::Stomach => Core::Stomach,
        Durable::Back => Core::Back,
        Durable::LeftShoulder => Core::LeftShoulder,
        Durable::RightShoulder => Core::RightShoulder,
        Durable::LeftArm => Core::LeftArm,
        Durable::RightArm => Core::RightArm,
        Durable::LeftHand => Core::LeftHand,
        Durable::RightHand => Core::RightHand,
        Durable::LeftLeg => Core::LeftLeg,
        Durable::RightLeg => Core::RightLeg,
        Durable::LeftFoot => Core::LeftFoot,
        Durable::RightFoot => Core::RightFoot,
        Durable::LeftBelt => Core::LeftBelt,
        Durable::RightBelt => Core::RightBelt,
        Durable::FrontBelt => Core::FrontBelt,
        Durable::BackBelt => Core::BackBelt,
        Durable::LeftPocket => Core::LeftPocket,
        Durable::RightPocket => Core::RightPocket,
        Durable::BackLeftPocket => Core::BackLeftPocket,
        Durable::BackRightPocket => Core::BackRightPocket,
    }
}

fn tactical_equipment_channel(
    channel: adventuresim_stdb_client::EquipmentChannel,
) -> adventuresim_core::item_catalog::EquipmentChannel {
    use adventuresim_core::item_catalog::EquipmentChannel as Core;
    use adventuresim_stdb_client::EquipmentChannel as Durable;
    match channel {
        Durable::Held => Core::Held,
        Durable::BaseClothing => Core::BaseClothing,
        Durable::Padding => Core::Padding,
        Durable::FlexibleArmor => Core::FlexibleArmor,
        Durable::RigidArmor => Core::RigidArmor,
        Durable::Outerwear => Core::Outerwear,
        Durable::Accessory => Core::Accessory,
        Durable::Mount => Core::Mount,
        Durable::Containment => Core::Containment,
    }
}

pub(crate) fn spawn_connected_players(
    conn: Res<SpacetimeDb>,
    args: Res<Args>,
    mut cmd: Commands,
    q_loading: Query<(Entity, &LoadingPlayer)>,
    q_scene: Query<&SceneTerrain>,
    combat_config: Res<TacticalCombatConfig>,
) {
    for player in conn.take_connected_players() {
        spawn_connected_player(
            &player,
            args.enemy_combat_scale_bps,
            args.animation_behavior_lab,
            &mut cmd,
            &q_loading,
            &q_scene,
            &combat_config,
        );
    }
}

fn spawn_connected_player(
    player: &ConnectedPlayer,
    enemy_combat_scale_bps: u32,
    animation_behavior_lab: bool,
    cmd: &mut Commands,
    q_loading: &Query<(Entity, &LoadingPlayer)>,
    q_scene: &Query<&SceneTerrain>,
    combat_config: &TacticalCombatConfig,
) {
    let entity = if player.mission_side == TacticalMissionSide::Enemy {
        let packages = if animation_behavior_lab {
            match AnimationLabEnemyRole::from_name(&player.character.name) {
                Some(AnimationLabEnemyRole::ShieldBlocker) => {
                    CombatantBehaviorPackages::always_block_without_facing()
                }
                Some(AnimationLabEnemyRole::Dodger) => CombatantBehaviorPackages::always_dodge(),
                Some(AnimationLabEnemyRole::Passive | AnimationLabEnemyRole::DemiLancer) => {
                    CombatantBehaviorPackages::passive()
                }
                None => {
                    warn!(
                        name = player.character.name,
                        "Animation lab enemy has no recognized behavior role; leaving passive"
                    );
                    CombatantBehaviorPackages::passive()
                }
            }
        } else if enemy_combat_scale_bps > 0 {
            CombatantBehaviorPackages::standard_combat(combat_config)
        } else {
            CombatantBehaviorPackages::passive()
        };
        cmd.spawn((MissionEnemy, TacticalCombatSide::Enemy, packages))
            .id()
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
            starting_fear: player.strategic_fear,
            starting_fatigue: player.strategic_fatigue,
            starting_hunger: player.strategic_hunger,
            starting_thirst: player.strategic_thirst,
            starting_thermal: player.strategic_thermal,
            ..default()
        },
        EquipmentActionState::default(),
    ));
    cmd.entity(entity).insert((
        MeleeAttackAuthority::default(),
        RangedAttackAuthority::default(),
        if player.mission_side == TacticalMissionSide::Enemy {
            TacticalCombatSide::Enemy
        } else {
            TacticalCombatSide::Party
        },
        Transform::from_xyz(spawn_position.x, spawn_height, spawn_position.y)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        (
            player_collider.clone(),
            CollisionMargin(0.01),
            tactical_character_controller(),
            CharacterLook::default(),
            AuthoritativeMovementIntent::default(),
            AuthoritativePostureIntent::default(),
            MovementPace::default(),
        ),
    ));

    // Reserve every tactical entity before projecting topology so attachment
    // edges map to replicated ECS identities even when their parent appears
    // later in the durable snapshot. Durable row IDs remain server-only.
    let tactical_items: BTreeMap<u64, Entity> = player
        .items
        .iter()
        .map(|item| (item.inventory_item_id, cmd.spawn_empty().id()))
        .collect();
    for item in &player.items {
        let Some(quantity) = NonZeroU32::new(item.quantity) else {
            warn!(
                "Got item '{}' with zero quantity for Player#{}; skipped",
                item.item.id, player.character.id
            );
            continue;
        };
        let item_entity = tactical_items[&item.inventory_item_id];
        let mut item_cmd = cmd.entity(item_entity);
        let weapon_appearance = item.weapon_appearance.as_ref().and_then(|appearance| {
            let design_hash: [u8; 32] = appearance.design_hash.as_slice().try_into().ok()?;
            Some(WeaponAppearance {
                generator_version: appearance.generator_version,
                design_hash,
                recipe: appearance.recipe.clone(),
            })
        });
        let weapon_holder_appearance =
            item.weapon_holder_appearance
                .as_ref()
                .and_then(|appearance| {
                    let design_hash: [u8; 32] =
                        appearance.design_hash.as_slice().try_into().ok()?;
                    Some(WeaponHolderAppearance {
                        generator_version: appearance.generator_version,
                        design_hash,
                        recipe: appearance.recipe.clone(),
                    })
                });
        item_cmd.insert((
            Replicated,
            TacticalInventoryItemId(item.inventory_item_id),
            ItemOf(entity),
            ItemQuantity(quantity),
            ItemProperties {
                weight: item.item.weight,
                id: item.item.id.clone(),
            },
            Transform::default(),
        ));
        if let Some(definition) = adventuresim_core::item_catalog::definition(&item.item.id)
            && let Some(equipment) = &definition.equipment
        {
            let physical = equipment.physical;
            item_cmd.insert(EquipmentPhysical {
                dimensions_m: Vec3::from_array(physical.dimensions_m),
                grip_to_tip_m: physical.grip_to_tip_m,
                anchor_offset_m: Vec3::from_array(physical.anchor_offset_m),
            });
        }
        if let Some(appearance) = weapon_appearance {
            item_cmd.insert(appearance);
        }
        if let Some(appearance) = weapon_holder_appearance {
            item_cmd.insert(appearance);
        }
        item_cmd.insert(EquipmentTopology {
            placement_id: item.selected_placement_id.clone(),
            occupancies: item
                .occupancies
                .iter()
                .enumerate()
                .map(|(occupancy_index, occupancy)| EquipmentTopologyOccupancy {
                    occupancy_id: format!("tactical:{}:{occupancy_index}", item_entity.to_bits()),
                    anchor: match occupancy.anchor_kind {
                        EquipmentAnchorKind::CharacterLocation => {
                            TacticalEquipmentAnchor::CharacterLocation(tactical_equipment_location(
                                occupancy.location.expect("validated character location"),
                            ))
                        }
                        EquipmentAnchorKind::ItemAttachment => {
                            TacticalEquipmentAnchor::ItemAttachment {
                                parent: tactical_items[&occupancy
                                    .parent_inventory_item_id
                                    .expect("validated attachment parent")],
                                attachment_point_id: occupancy
                                    .attachment_point_id
                                    .clone()
                                    .expect("validated attachment point"),
                            }
                        }
                    },
                    channel: tactical_equipment_channel(occupancy.channel),
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
                let grip_to_tip_m = adventuresim_core::item_catalog::definition(&item.item.id)
                    .and_then(|definition| definition.equipment.as_ref())
                    .map_or(0.0, |equipment| equipment.physical.grip_to_tip_m);
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
                    swing_precision: item.item.swing_precision,
                    stab_precision: item.item.stab_precision,
                    prefers_stab: matches!(
                        item.item.preferred_melee_style,
                        adventuresim_stdb_client::MeleeAttackStyle::Stab
                    ),
                    penetration: item.item.penetration,
                    reach: item.item.reach,
                    grip_to_tip_m,
                    moment_of_inertia_kg_m2: item.item.moment_of_inertia_kg_m_2,
                    precise: item.item.precise,
                    melee: item.item.melee,
                    ranged: item.item.ranged,
                    blunt: item.item.blunt,
                    slash: item.item.slash,
                    pierce: item.item.pierce,
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
            occupancy.channel == adventuresim_stdb_client::EquipmentChannel::Held
                && occupancy.location == Some(adventuresim_stdb_client::EquipmentLocation::LeftHand)
        }) {
            item_cmd.insert(EquipSlot::HoldingLeft);
        } else if item.occupancies.iter().any(|occupancy| {
            occupancy.channel == adventuresim_stdb_client::EquipmentChannel::Held
                && occupancy.location
                    == Some(adventuresim_stdb_client::EquipmentLocation::RightHand)
        }) {
            item_cmd.insert(EquipSlot::HoldingRight);
        }
    }
    info!(
        temorary = player.character.temporary,
        "[startup] Player {entity:?} is fully loaded"
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each connection resource and projection query as an independent observer parameter"
)]
pub(crate) fn on_join_request(
    join: On<FromClient<JoinRequest>>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
    loading_players: Query<(), With<LoadingPlayer>>,
    players: Query<(), With<Player>>,
    mut disconnected_players: Query<(Entity, &mut DisconnectedPlayer, &DisconnectedProjection)>,
    inventory_items: Query<(Entity, &ItemOf)>,
    mut pending_actions: ResMut<PendingEquipmentActions>,
    mut action_sequences: ResMut<LastEquipmentSequence>,
    conn: Res<SpacetimeDb>,
    ready: Res<SpacetimeDbReady>,
    vista: Res<SceneVistaBundleResource>,
    combat_config: Res<TacticalCombatConfig>,
) -> Result {
    let Some(client) = join.client_id.entity() else {
        return Ok(());
    };
    if loading_players.contains(client) || players.contains(client) {
        return Ok(());
    }
    let reconnect =
        disconnected_players
            .iter_mut()
            .find_map(|(entity, mut session, projection)| {
                try_claim_reconnect(join.character_id, join.reconnect_token, &mut session)
                    .then(|| (entity, projection.clone()))
            });
    if let Some((disconnected, projection)) = reconnect {
        let token = fresh_reconnect_token();
        let projected = projection.projected();
        if projected {
            queue_replication_rebind(&mut commands, client);
        }
        projection.insert(&mut commands, client);
        commands.entity(client).insert(ReconnectSession {
            character_id: join.character_id,
            token,
        });
        send_reconnect_capability(&mut commands, join.client_id, join.character_id, token);
        send_scene_vista(&mut commands, join.client_id, &vista);
        send_combat_config(&mut commands, join.client_id, &combat_config);
        reconnect_equipment_lifecycle(
            disconnected,
            client,
            &mut pending_actions,
            &mut action_sequences,
        );
        for (item, owner) in &inventory_items {
            if owner.0 == disconnected {
                commands.entity(item).insert(ItemOf(client));
            }
        }
        if projected {
            commands.queue(move |world: &mut World| rebuild_inventory_holding_cache(world, client));
        }
        commands.entity(disconnected).despawn();
        info!(
            character_id = join.character_id.0,
            "Rebound reconnect to transient tactical state"
        );
        return Ok(());
    }
    if join.reconnect_token.is_some() {
        warn!(
            character_id = join.character_id.0,
            "Rejected invalid or consumed reconnect capability"
        );
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
    let token = fresh_reconnect_token();
    commands.entity(client).insert((
        LoadingPlayer {
            requested_character: join.character_id,
        },
        ReconnectSession {
            character_id: join.character_id,
            token,
        },
    ));
    send_reconnect_capability(&mut commands, join.client_id, join.character_id, token);
    send_scene_vista(&mut commands, join.client_id, &vista);
    send_combat_config(&mut commands, join.client_id, &combat_config);
    info!(
        "[startup] Character {} connected and entered mission, awaiting loading",
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
        "[startup] Character {} connected, awaiting binding to a dumped character",
        join.character_id.0
    );
    Ok(())
}

pub(crate) fn on_player_input(
    input: On<FromClient<PlayerInputRequest>>,
    viewer: TacticalPlayerViewer,
    mut commands: Commands,
    mut players: PlayerInputStateQuery<'_, '_>,
    combat_config: Res<TacticalCombatConfig>,
) {
    let Some(validated) = validate_player_input(**input) else {
        return;
    };
    let Some(entity) = input.client_id.entity() else {
        return;
    };
    let Ok((
        mut accumulated_input,
        mut look,
        combat_state,
        mut skeleton,
        mut movement_intent,
        input_tick,
        mut posture_intent,
        mut quickstep_push,
        mut pace,
        mut velocity,
        mut transform,
        mut physics_rotation,
        startup_input_observed,
    )) = players.get_mut(entity)
    else {
        return;
    };
    if let Some(mut newest) = input_tick
        && !newest.accept(input.simulation_tick)
    {
        return;
    }
    let jump_requested =
        sequence_is_newer(validated.jump.sequence, posture_intent.last_jump_sequence);
    if jump_requested && let Some(direction) = validated.jump.quickstep {
        info!(
            target: "quickstep_trace",
            ?entity,
            simulation_tick = input.simulation_tick,
            sequence = validated.jump.sequence,
            ?direction,
            requested_guard = ?validated.weapon_guard,
            server_guard = ?skeleton.weapon_guard(),
            server_body = ?skeleton.body(),
            server_action = ?skeleton.action_kind(),
            push_active = quickstep_push.active,
            "[quickstep][server-input] received new edge"
        );
    }
    if jump_requested {
        // Consume the edge even when the current body state cannot jump. The
        // client repeats this sequence indefinitely, so retaining it through
        // incapacitation or an airborne interval would create a stale jump
        // as soon as the player became grounded again.
        posture_intent.last_jump_sequence = validated.jump.sequence;
    }
    if combat_state.is_incapacitated() {
        if jump_requested && validated.jump.quickstep.is_some() {
            warn!(
                target: "quickstep_trace",
                ?entity,
                simulation_tick = input.simulation_tick,
                sequence = validated.jump.sequence,
                "[quickstep][server-input] rejected: incapacitated"
            );
        }
        accumulated_input.last_movement = None;
        movement_intent.0 = None;
        accumulated_input.jumped = None;
        accumulated_input.crouched = false;
        posture_intent.facing = CameraFacingIntent::Free;
        quickstep_push.cancel();
        skeleton.set_jump_anticipation(false);
        set_weapon_guard(
            &mut skeleton,
            authoritative_weapon_guard(validated.weapon_guard, true),
        );
        return;
    }
    if !startup_input_observed {
        info!("[startup] first server input received for {entity:?}");
        commands.entity(entity).insert(StartupInputObserved);
    }
    trace!(
        "DEBUG on_player_input entity={entity:?} input.look={:?} validated.yaw={}",
        input.look, validated.yaw
    );
    look.yaw = validated.yaw;
    look.pitch = validated.pitch;
    accumulated_input.last_movement = validated.movement;
    if sequence_is_newer(
        validated.posture.sequence,
        posture_intent.last_command_sequence,
    ) {
        posture_intent.last_command_sequence = validated.posture.sequence;
        if let Some(action) = validated.posture.action
            && let Some(launch) = apply_posture_action(
                action,
                &mut skeleton,
                &mut accumulated_input,
                validated.pace,
                &combat_config,
            )
        {
            if launch.trajectory == DiveTrajectory::Airborne {
                // Airborne dive travel and authored direction both capture
                // this accepted camera frame before transition facing locks.
                let launch_rotation = dive_launch_root_rotation(Quat::from_rotation_y(look.yaw));
                transform.rotation = launch_rotation;
                physics_rotation.0 = launch_rotation;
            }
            // A slide inherits an already-committed sprint heading and exact
            // velocity. Rewriting its root to camera yaw would twist the whole
            // body on the first frame, independently of its inverted animation.
            apply_dive_launch_velocity(
                &mut velocity,
                look.yaw,
                launch,
                combat_config.movement.speeds_metres_per_second.dive,
            );
        }
    }
    accumulated_input.crouched = skeleton.body().is_downed() || skeleton.is_posture_transitioning();
    movement_intent.0 = validated.movement;
    posture_intent.facing =
        CameraFacingIntent::from_input(validated.weapon_guard, validated.downed_align);
    skeleton.set_jump_anticipation(validated.jump_charge);
    *pace = validated.pace;
    set_weapon_guard(
        &mut skeleton,
        authoritative_weapon_guard(validated.weapon_guard, false),
    );
    if validated.weapon_guard == WeaponGuardState::Raised
        && let Ok(view) = viewer.get(entity)
    {
        let preferred = StrikeFamily::from_melee_style(view.weapon_preferred_melee_style());
        let requested = match validated.melee_preparation {
            MeleePreparationInput::Preferred => preferred,
            MeleePreparationInput::Alternate => preferred.alternate(),
            MeleePreparationInput::Offhand => preferred,
        };
        let preparation = if validated.melee_preparation == MeleePreparationInput::Offhand
            && skeleton.attack_animations.offhand_preparation
        {
            AttackPreparation::offhand()
        } else {
            AttackPreparation::main(
                skeleton
                    .available_strike_family(requested)
                    .unwrap_or(preferred),
            )
        };
        skeleton.set_attack_preparation(preparation);
    }
    if jump_requested {
        if skeleton.is_posture_transitioning() {
            if validated.jump.quickstep.is_some() {
                warn!(
                    target: "quickstep_trace",
                    ?entity,
                    simulation_tick = input.simulation_tick,
                    sequence = validated.jump.sequence,
                    transition = ?skeleton.posture_transition().map(|transition| transition.kind()),
                    "[quickstep][server-input] rejected: posture transition"
                );
            }
        } else if !matches!(skeleton.body(), BodyState::Grounded(_)) {
            if validated.jump.quickstep.is_some() {
                warn!(
                    target: "quickstep_trace",
                    ?entity,
                    simulation_tick = input.simulation_tick,
                    sequence = validated.jump.sequence,
                    server_body = ?skeleton.body(),
                    "[quickstep][server-input] rejected: not grounded"
                );
            }
        } else {
            let launch = match validated.jump.quickstep {
                Some(direction)
                    if validated.weapon_guard == WeaponGuardState::Raised
                        && skeleton.body() == BodyState::Grounded(GroundedPosture::Upright) =>
                {
                    let accepted = begin_authoritative_quickstep(
                        &mut skeleton,
                        &mut quickstep_push,
                        direction,
                        Quat::from_rotation_y(look.yaw),
                        transform.translation,
                        &combat_config,
                    );
                    if accepted {
                        info!(
                            target: "quickstep_trace",
                            ?entity,
                            simulation_tick = input.simulation_tick,
                            sequence = validated.jump.sequence,
                            ?direction,
                            skeleton_tick = skeleton.locomotion_sample_tick,
                            push_start_tick = quickstep_push.start_tick,
                            push_origin = ?quickstep_push.origin,
                            "[quickstep][server-input] accepted animation and actuator"
                        );
                        commands.trigger(crate::combat::DefendIntent {
                            defender: entity,
                            choice: DefendRequest::Dodge { direction },
                        });
                    } else {
                        warn!(
                            target: "quickstep_trace",
                            ?entity,
                            simulation_tick = input.simulation_tick,
                            sequence = validated.jump.sequence,
                            server_guard = ?skeleton.weapon_guard(),
                            server_body = ?skeleton.body(),
                            server_action = ?skeleton.action_kind(),
                            push_active = quickstep_push.active,
                            "[quickstep][server-input] rejected by action admission"
                        );
                    }
                    false
                }
                Some(direction) => {
                    warn!(
                        target: "quickstep_trace",
                        ?entity,
                        simulation_tick = input.simulation_tick,
                        sequence = validated.jump.sequence,
                        ?direction,
                        requested_guard = ?validated.weapon_guard,
                        server_guard = ?skeleton.weapon_guard(),
                        server_body = ?skeleton.body(),
                        "[quickstep][server-input] rejected: guard or posture"
                    );
                    false
                }
                None => true,
            };
            if launch {
                accumulated_input.jumped = Some(Stopwatch::new());
            }
        }
    }
}

pub(crate) fn begin_authoritative_quickstep(
    skeleton: &mut SkeletonState,
    quickstep_push: &mut QuickstepPush,
    direction: Vec2,
    orientation: Quat,
    origin: Vec3,
    config: &TacticalCombatConfig,
) -> bool {
    if skeleton.weapon_guard() != WeaponGuardState::Raised
        || skeleton.body() != BodyState::Grounded(GroundedPosture::Upright)
    {
        return false;
    }
    let start = skeleton.locomotion_sample_tick;
    let Some(spec) = DodgeSpec::quickstep(direction) else {
        return false;
    };
    if skeleton
        .begin_dodge(
            spec,
            start,
            start
                + quickstep_action_contact_ticks(
                    config.movement.maneuvers.quickstep_duration_seconds,
                ),
        )
        .is_err()
    {
        return false;
    }
    quickstep_push.begin(start, direction, orientation, origin);
    true
}

fn sequence_is_newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance <= u32::MAX / 2
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ValidatedPlayerInput {
    movement: Option<Vec2>,
    yaw: f32,
    pitch: f32,
    jump: JumpCommand,
    jump_charge: bool,
    downed_align: bool,
    posture: PostureCommand,
    pace: MovementPace,
    weapon_guard: WeaponGuardState,
    melee_preparation: MeleePreparationInput,
}

fn validate_player_input(input: PlayerInputRequest) -> Option<ValidatedPlayerInput> {
    let PlayerInputRequest {
        simulation_tick: _,
        look,
        movement,
        jump,
        jump_charge,
        downed_align,
        posture,
        pace,
        weapon_guard,
        melee_preparation,
    } = input;
    if !look.is_finite()
        || movement.is_some_and(|movement| !movement.is_finite())
        || jump
            .quickstep
            .is_some_and(|direction| !direction.is_finite())
    {
        return None;
    }
    let jump = JumpCommand {
        sequence: jump.sequence,
        quickstep: jump
            .quickstep
            .map(Vec2::normalize_or_zero)
            .filter(|direction| *direction != Vec2::ZERO),
    };
    Some(ValidatedPlayerInput {
        movement: movement.map(|movement| movement.clamp_length_max(1.0)),
        yaw: (look.x + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI,
        pitch: look.y.clamp(-1.5, 1.5),
        jump,
        jump_charge,
        downed_align,
        posture,
        pace,
        weapon_guard,
        melee_preparation,
    })
}

fn apply_posture_action(
    action: PostureActionRequest,
    skeleton: &mut SkeletonState,
    accumulated_input: &mut AccumulatedInput,
    pace: MovementPace,
    config: &TacticalCombatConfig,
) -> Option<DiveLaunch> {
    if action == PostureActionRequest::Toggle && skeleton.body().is_downed() {
        begin_get_up_transition_configured(skeleton, config);
        return None;
    }
    let tick = skeleton.locomotion_sample_tick;
    let mut dive_launch = None;
    let transition = match action {
        PostureActionRequest::Toggle => match skeleton.body() {
            BodyState::Grounded(_) => Some(PostureTransitionKind::UprightToProne),
            BodyState::Prone | BodyState::Supine | BodyState::Airborne | BodyState::Ragdolled => {
                None
            }
        },
        PostureActionRequest::RollLeft => roll_transition(skeleton.body(), RollDirection::Left),
        PostureActionRequest::RollRight => roll_transition(skeleton.body(), RollDirection::Right),
        PostureActionRequest::Dive {
            animation_direction,
            travel_direction,
        } => {
            let trajectory = if pace == MovementPace::Sprint {
                DiveTrajectory::GroundedSlide
            } else {
                DiveTrajectory::Airborne
            };
            let direction = if trajectory == DiveTrajectory::GroundedSlide {
                travel_direction.opposite()
            } else {
                animation_direction
            };
            dive_launch = Some(DiveLaunch {
                travel_direction,
                trajectory,
            });
            matches!(skeleton.body(), BodyState::Grounded(_)).then_some(
                PostureTransitionKind::DiveToDowned {
                    direction,
                    trajectory,
                },
            )
        }
    };
    let transition = transition?;
    let duration = match transition {
        PostureTransitionKind::DiveToDowned {
            trajectory: DiveTrajectory::GroundedSlide,
            ..
        } => combat_seconds_to_ticks(config.movement.maneuvers.slide_seconds),
        PostureTransitionKind::DiveToDowned {
            direction: DiveDirection::Backward,
            ..
        } => combat_seconds_to_ticks(config.movement.maneuvers.backward_dive_seconds),
        PostureTransitionKind::DiveToDowned { .. } => {
            combat_seconds_to_ticks(config.movement.maneuvers.dive_seconds)
        }
        PostureTransitionKind::ProneToSupine { .. }
        | PostureTransitionKind::SupineToProne { .. } => {
            combat_seconds_to_ticks(config.movement.maneuvers.roll_seconds)
        }
        _ => combat_seconds_to_ticks(config.movement.maneuvers.get_up_seconds),
    };
    if !skeleton.begin_posture_transition(transition, tick, duration) {
        return None;
    }
    if matches!(transition, PostureTransitionKind::DiveToDowned { .. }) {
        let launch = dive_launch.expect("dive transition always records its launch");
        accumulated_input.jumped =
            (launch.trajectory == DiveTrajectory::Airborne).then(Stopwatch::new);
        return Some(launch);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiveLaunch {
    travel_direction: DiveDirection,
    trajectory: DiveTrajectory,
}

pub(crate) fn begin_get_up_transition_configured(
    skeleton: &mut SkeletonState,
    config: &TacticalCombatConfig,
) -> bool {
    let transition = match skeleton.body() {
        BodyState::Prone => PostureTransitionKind::ProneToUpright,
        BodyState::Supine => PostureTransitionKind::SupineToUpright,
        _ => return false,
    };
    skeleton.begin_posture_transition(
        transition,
        skeleton.locomotion_sample_tick,
        combat_seconds_to_ticks(config.movement.maneuvers.get_up_seconds),
    )
}

fn dive_horizontal_velocity(yaw: f32, direction: DiveDirection, speed: f32) -> Vec3 {
    let local = match direction {
        DiveDirection::Forward => Vec3::NEG_Z,
        DiveDirection::Backward => Vec3::Z,
        DiveDirection::Left => Vec3::NEG_X,
        DiveDirection::Right => Vec3::X,
    };
    Quat::from_rotation_y(yaw) * local * speed
}

fn apply_dive_launch_velocity(
    velocity: &mut LinearVelocity,
    yaw: f32,
    launch: DiveLaunch,
    speed: f32,
) {
    // Sliding is a posture change, not a fresh launch. Preserve the complete
    // sprint velocity; gravity and the later body-drag phase own its evolution.
    if launch.trajectory == DiveTrajectory::GroundedSlide {
        return;
    }
    let horizontal = dive_horizontal_velocity(yaw, launch.travel_direction, speed);
    velocity.x = horizontal.x;
    velocity.z = horizontal.z;
}

fn combat_seconds_to_ticks(seconds: f32) -> u64 {
    (seconds * LOCOMOTION_SAMPLE_HZ).round().max(1.0) as u64
}

fn roll_transition(body: BodyState, direction: RollDirection) -> Option<PostureTransitionKind> {
    match body {
        BodyState::Prone => Some(PostureTransitionKind::ProneToSupine { direction }),
        BodyState::Supine => Some(PostureTransitionKind::SupineToProne { direction }),
        _ => None,
    }
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

fn downed_tank_controller_input(
    movement: Vec2,
    body: BodyState,
    body_orientation: Quat,
    controller_orientation: Quat,
    lateral_speed_scale: f32,
) -> Vec2 {
    let longitudinal_scale = if body == BodyState::Supine && movement.y < 0.0 {
        0.5
    } else {
        1.0
    };
    let body_local = Vec3::new(
        -movement.x * lateral_speed_scale,
        0.0,
        movement.y * longitudinal_scale,
    );
    body_relative_controller_input(body_local, body_orientation, controller_orientation)
}

fn body_relative_controller_input(
    body_local: Vec3,
    body_orientation: Quat,
    controller_orientation: Quat,
) -> Vec2 {
    let world_direction = controller_yaw(body_orientation) * body_local;
    let controller_local = controller_yaw(controller_orientation).inverse() * world_direction;
    Vec2::new(controller_local.x, -controller_local.z).clamp_length_max(1.0)
}

fn advance_downed_facing_for_camera(
    skeleton: &mut SkeletonState,
    facing: CameraFacingIntent,
    target: f32,
    maximum_step: f32,
) {
    if facing == CameraFacingIntent::Aim {
        skeleton.advance_downed_facing(target, true, maximum_step);
    } else if skeleton.downed_facing().is_some() {
        // Aim release resolves an already-active interpolation to its nearest
        // stable contact. A free camera cannot seed a new prone/supine roll.
        skeleton.advance_downed_facing(target, false, 1.0);
    }
}

/// Rehydrates Ahoy's disposable fixed-loop input from the latest accepted
/// complete request before movement runs. Ahoy may clear its accumulator after
/// every fixed loop without turning a missing network packet into a stop.
pub(crate) fn restore_authoritative_movement_intent(
    mut players: MovementIntentQuery<'_, '_>,
    combat_config: Res<TacticalCombatConfig>,
) {
    for (
        movement_intent,
        skeleton,
        posture,
        quickstep_push,
        controller,
        transform,
        mut accumulated_input,
    ) in &mut players
    {
        accumulated_input.last_movement = movement_intent.0;
        if quickstep_push.active {
            // The supported force phase commits to its accepted direction.
            accumulated_input.last_movement = Some(skeleton.action_direction());
        }
        if skeleton.body().is_downed()
            && let (Some(controller), Some(transform), Some(movement)) =
                (controller, transform, accumulated_input.last_movement)
        {
            accumulated_input.last_movement = Some(downed_tank_controller_input(
                movement,
                skeleton.body(),
                transform.rotation,
                controller.orientation,
                combat_config.movement.prone_lateral_speed_scale,
            ));
        }
        if skeleton.body() == BodyState::Prone && posture.facing == CameraFacingIntent::Aim {
            accumulated_input.last_movement = None;
        }
        accumulated_input.crouched =
            skeleton.body().is_downed() || skeleton.is_posture_transitioning();
        let roll_motion = skeleton.downed_lateral_motion();
        if roll_motion.abs() > f32::EPSILON {
            accumulated_input.last_movement = match (controller, transform) {
                (Some(controller), Some(transform)) => Some(body_relative_controller_input(
                    Vec3::new(-roll_motion, 0.0, 0.0),
                    transform.rotation,
                    controller.orientation,
                )),
                _ => Some(Vec2::X * roll_motion),
            };
        } else if skeleton.is_posture_transitioning() {
            accumulated_input.last_movement = None;
        }
    }
}

/// Projects authoritative controller motion into the compact presentation
/// state replicated to every client. It deliberately never evaluates bones.
pub(crate) fn update_skeleton_locomotion(
    time: Res<Time<Fixed>>,
    combat_config: Res<TacticalCombatConfig>,
    mut players: LocomotionQuery<'_, '_>,
) {
    for (
        controller,
        velocity,
        mut skeleton,
        mut transform,
        mut physics_rotation,
        combat_state,
        pace,
        posture,
        quickstep_push,
        accumulated_input,
        attack_facing,
    ) in &mut players
    {
        if combat_state.is_incapacitated() {
            let lowered = authoritative_weapon_guard(skeleton.weapon_guard(), true);
            set_weapon_guard(&mut skeleton, lowered);
        }
        let tick = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
        let posture_transitioning = posture_transition_locks_body_facing(&skeleton);
        if posture_transitioning {
            // Authored transitions own their direction relative to a fixed
            // root until a roll or get-up has reached its endpoint.
            skeleton.set_downed_turning(false);
        } else if matches!(skeleton.body(), BodyState::Prone | BodyState::Supine)
            && !skeleton.is_posture_transitioning()
        {
            let target = downed_camera_roll_target(transform.rotation, controller.orientation);
            if posture.facing == CameraFacingIntent::DownedAlign {
                let next = advance_downed_body_facing_with_speed(
                    transform.rotation,
                    controller.orientation,
                    time.delta_secs(),
                    combat_config.presentation.downed_turn_radians_per_second,
                );
                skeleton.set_downed_turning(transform.rotation.angle_between(next) > 1.0e-5);
                transform.rotation = next;
            } else {
                skeleton.set_downed_turning(false);
                advance_downed_facing_for_camera(
                    &mut skeleton,
                    posture.facing,
                    target,
                    time.delta_secs() / combat_config.movement.maneuvers.get_up_seconds,
                );
            }
        } else if skeleton.body() != BodyState::Ragdolled {
            skeleton.set_downed_turning(false);
            transform.rotation = if let Some(attack_facing) = attack_facing {
                let remaining_seconds =
                    attack_facing.contact_tick.saturating_sub(tick) as f32 / LOCOMOTION_SAMPLE_HZ;
                let desired_forward = attack_facing.target_position - transform.translation;
                let turn_speed = body_turn_speed_for_deadline(
                    transform.rotation,
                    desired_forward,
                    remaining_seconds,
                    time.delta_secs(),
                );
                advance_body_facing_toward(
                    transform.rotation,
                    desired_forward,
                    time.delta_secs(),
                    turn_speed,
                )
            } else {
                advance_body_facing_with_speed(
                    transform.rotation,
                    controller.orientation,
                    velocity.0,
                    skeleton.action_kind(),
                    skeleton.weapon_guard(),
                    time.delta_secs(),
                    std::f32::consts::PI
                        / combat_config.presentation.body_turn_seconds_per_half_turn,
                )
            };
        }
        project_skeleton_locomotion_with_intent(
            &mut skeleton,
            SkeletonLocomotionInput {
                orientation: controller.orientation,
                linear_velocity: velocity.0,
                grounded: controller.grounded.is_some() || quickstep_push.active,
                delta_seconds: time.delta_secs(),
                tick,
            },
            accumulated_input
                .last_movement
                .map(|movement| Vec2::new(movement.x, -movement.y)),
        );
        let previous_transition = skeleton.posture_transition();
        skeleton.advance_posture_transition(tick);
        advance_posture_transition_facing(
            &mut transform,
            &mut physics_rotation,
            previous_transition,
            skeleton.posture_transition(),
        );
        skeleton.set_guarded_sprint_locomotion(*pace == MovementPace::Sprint);
    }
}

/// Records the controller result after Ahoy has applied collision constraints.
/// `apply_character_motor` emits the matching pre-collision sample with the
/// same skeleton and push ticks.
pub(crate) fn trace_authoritative_quickstep_after_collision(players: QuickstepTraceQuery<'_, '_>) {
    for (entity, skeleton, push, transform, velocity, controller) in &players {
        if !push.active {
            continue;
        }
        let world_direction = (push.orientation
            * Vec3::new(push.direction.x, 0.0, -push.direction.y))
        .xz()
        .normalize_or_zero();
        let displacement_vector = (transform.translation - push.origin).xz();
        info!(
            target: "quickstep_trace",
            ?entity,
            skeleton_tick = skeleton.locomotion_sample_tick,
            push_start_tick = push.start_tick,
            elapsed_ticks = skeleton
                .locomotion_sample_tick
                .saturating_sub(push.start_tick),
            ?world_direction,
            translation = ?transform.translation,
            ?displacement_vector,
            forward_displacement = displacement_vector.dot(world_direction),
            linear_velocity = ?velocity.0,
            forward_velocity = velocity.xz().dot(world_direction),
            grounded = ?controller.grounded,
            "[quickstep][server-post-collision] resolved controller"
        );
    }
}

/// Freezes the complete controller boundary after collision resolution and
/// authoritative facing. This is deliberately distinct from replicated
/// presentation state so a client can acknowledge, restore, and replay input
/// by fixed tick.
pub(crate) fn update_character_motion_snapshots(mut players: CharacterMotionSnapshotQuery<'_, '_>) {
    for (transform, velocity, controller, input, quickstep_push, melee_lunge, mut snapshot) in
        &mut players
    {
        *snapshot = CharacterMotionSnapshot {
            acknowledged_input_tick: input.tick,
            translation: transform.translation,
            rotation: transform.rotation,
            linear_velocity: velocity.0,
            grounded: controller.grounded.is_some(),
            quickstep_push: *quickstep_push,
            melee_lunge: melee_lunge.copied(),
        };
    }
}

/// `aeronet_io`'s own `ConnectionPlugin` (part of `AdventureSimulatorNetPlugins`)
/// registers an observer on this exact same `Disconnected` trigger that
/// unconditionally despawns `entity` right afterward - see its doc comment:
/// "Immediately after this, the session will be despawned". Bevy documents
/// same-event observer ordering as unspecified. Snapshot every reconnect-owned
/// component while the source is still queryable, then queue only operations
/// whose inputs are owned values. Aeronet may therefore apply its source
/// despawn before or after these commands without invalidating them.
#[expect(
    clippy::too_many_arguments,
    reason = "Bevy injects each observer resource and query as an independent system parameter"
)]
pub(crate) fn on_client_disconnected(
    disconnected: On<Disconnected>,
    query: Query<&ReconnectSession>,
    projected_core: ProjectedPlayerCoreQuery<'_, '_>,
    projected_motion: ProjectedPlayerMotionQuery<'_, '_>,
    projected_physics: Query<(
        &Collider,
        &CollisionMargin,
        &CharacterController,
        &AccumulatedInput,
    )>,
    loading_players: Query<&LoadingPlayer>,
    inventory_items: Query<(Entity, &ItemOf)>,
    mut commands: Commands,
) -> Result {
    let entity = disconnected.event_target();
    let Ok(session) = query.get(entity) else {
        return Ok(());
    };
    let projection = if let (Ok(core), Ok(motion), Ok(physics)) = (
        projected_core.get(entity),
        projected_motion.get(entity),
        projected_physics.get(entity),
    ) {
        DisconnectedProjection::Projected(Box::new(ProjectedPlayerSnapshot {
            name: core.0.clone(),
            player: core.1.clone(),
            character_id: *core.2,
            bestiary_categories: core.3.clone(),
            skills: core.4.clone(),
            limbs: core.5.clone(),
            attributes: core.6.clone(),
            stats: core.7.clone(),
            combat_state: core.8.clone(),
            equipment_action_state: *core.9,
            combat_side: *core.10,
            transform: *motion.0,
            look: motion.1.clone(),
            movement_intent: *motion.2,
            input_tick: *motion.3,
            motion_snapshot: *motion.4,
            quickstep_push: *motion.5,
            melee_lunge: motion.6.copied(),
            posture_intent: *motion.7,
            pace: *motion.8,
            mass: *motion.9,
            velocity: *motion.10,
            skeleton: motion.11.clone(),
            melee_authority: motion.12.clone(),
            pending_melee_contact: motion.13.copied(),
            ranged_authority: motion.14.clone(),
            collider: physics.0.clone(),
            collision_margin: *physics.1,
            controller: physics.2.clone(),
            accumulated_input: physics.3.clone(),
        }))
    } else if let Ok(loading) = loading_players.get(entity) {
        DisconnectedProjection::Loading(*loading)
    } else {
        warn!(
            ?entity,
            "Disconnected reconnect session had no player projection to retain"
        );
        return Ok(());
    };
    let projected = projection.projected();
    let orphan = commands
        .spawn((
            projection,
            DisconnectedPlayer {
                character_id: session.character_id,
                reconnect_token: session.token,
                remaining_secs: RECONNECT_GRACE_SECS,
                claimed: false,
            },
        ))
        .id();
    for (item, owner) in &inventory_items {
        if owner.0 == entity {
            commands.entity(item).insert(ItemOf(orphan));
        }
    }
    if projected {
        commands.queue(move |world: &mut World| rebuild_inventory_holding_cache(world, orphan));
    }
    let character_id = session.character_id.0;
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

fn fresh_reconnect_token() -> ReconnectToken {
    ReconnectToken(rand::random())
}

fn reconnect_matches(
    requested: CharacterId,
    supplied: Option<ReconnectToken>,
    existing: &DisconnectedPlayer,
) -> bool {
    !existing.claimed
        && existing.remaining_secs > 0.0
        && requested == existing.character_id
        && supplied == Some(existing.reconnect_token)
}

fn try_claim_reconnect(
    requested: CharacterId,
    supplied: Option<ReconnectToken>,
    existing: &mut DisconnectedPlayer,
) -> bool {
    if !reconnect_matches(requested, supplied, existing) {
        return false;
    }
    // Immediate mutation serializes duplicate ordered events even though the
    // component moves below are deferred until the command queue is applied.
    existing.claimed = true;
    true
}

fn send_reconnect_capability(
    commands: &mut Commands,
    client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId,
    character_id: CharacterId,
    token: ReconnectToken,
) {
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client_id),
        message: ReconnectCapability {
            character_id,
            token,
        },
    });
}

fn send_scene_vista(
    commands: &mut Commands,
    client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId,
    vista: &SceneVistaBundleResource,
) {
    if let Some(message) = &vista.0 {
        commands.server_trigger(ToClients {
            targets: SendTargets::Single(client_id),
            message: message.clone(),
        });
    }
}

fn send_combat_config(
    commands: &mut Commands,
    client_id: adventuresim_tactical_netcode::bevy_replicon::prelude::ClientId,
    config: &TacticalCombatConfig,
) {
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client_id),
        message: TacticalCombatConfigSnapshot(config.clone()),
    });
}

fn queue_replication_rebind(commands: &mut Commands, client: Entity) {
    commands.entity(client).insert(Replicated);
}

pub(crate) fn expire_disconnected_players(
    time: Res<Time>,
    mut commands: Commands,
    mut disconnected: Query<(Entity, &mut DisconnectedPlayer)>,
    items: Query<(Entity, &ItemOf)>,
    mut pending_actions: ResMut<PendingEquipmentActions>,
    mut action_sequences: ResMut<LastEquipmentSequence>,
    conn: Res<SpacetimeDb>,
) {
    for (entity, mut grace) in &mut disconnected {
        // A successful reconnect synchronously owns this root. Its deferred
        // moves must complete before any expiry teardown can touch it.
        if grace.claimed {
            continue;
        }
        grace.remaining_secs -= time.delta_secs();
        if grace.remaining_secs > 0.0 {
            continue;
        }
        if let Err(error) = conn.reducers().leave_mission(grace.character_id.0) {
            warn!(
                character_id = grace.character_id.0,
                ?error,
                "Failed to expire disconnected mission member"
            );
            continue;
        }
        for (item, owner) in &items {
            if owner.0 == entity {
                commands.entity(item).despawn();
            }
        }
        purge_equipment_lifecycle(entity, &mut pending_actions, &mut action_sequences);
        commands.entity(entity).despawn();
    }
}

fn posture_transition_locks_body_facing(skeleton: &SkeletonState) -> bool {
    // Every authored posture transition encodes its own direction relative to
    // the current root. Turning the root toward residual/controller velocity
    // double-rotates directional dives and can reverse a get-up mid-pose.
    skeleton.is_posture_transitioning()
}

fn advance_posture_transition_facing(
    transform: &mut Transform,
    physics_rotation: &mut Rotation,
    previous_transition: Option<PostureTransitionState>,
    current_transition: Option<PostureTransitionState>,
) {
    // Directional dives transfer the downed contact pose's yaw to the root
    // during landing. A backward dive's half-turn is spread across contact
    // recovery; the supine get-up applies the inverse convention change.
    let rotation = (transform.rotation
        * dive_landing_facing_delta(previous_transition, current_transition)
        * supine_get_up_counter_yaw_delta(previous_transition, current_transition))
    .normalize();
    transform.rotation = rotation;
    physics_rotation.0 = rotation;
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
    commands.entity(event.entity).insert_if_new((
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
        AuthoritativeInputTick::default(),
        CharacterMotionSnapshot::default(),
        QuickstepPush::default(),
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
/// right where they add the other things a dump deliberately
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

        let registry = world.resource::<AppTypeRegistry>().0.clone();
        let scene = {
            let registry = registry.read();
            bevy::world_serialization::DynamicWorldBuilder::from_world(world, &registry)
                .extract_entities(core::iter::once(template_entity).chain(item_entities))
                .build()
        };
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
                // `on_player_added`'s generic bundle (fired by the `Player`
                // insert just above) adds most "always fresh, never
                // dump-captured" extras, but not these two - unlike
                // `spawn_connected_player`'s normal-join path, which inserts
                // them explicitly. Their absence doesn't fail loudly: it
                // just makes `on_player_input`'s query silently never match
                // this entity, so the joined client's ordinary per-frame
                // input (movement, look, everything) is dropped forever -
                // confirmed live, traced through many layers of downstream
                // symptoms (wrong facing, bots never reacting) before
                // finding this root cause.
                world.entity_mut(client_entity).insert((
                    AuthoritativePostureIntent::default(),
                    MovementPace::default(),
                ));
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
                crate::bot::OffensiveCombatAi::default(),
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
                    grip_to_tip_m: 0.8,
                    moment_of_inertia_kg_m2: 0.0,
                    precise: false,
                    melee: true,
                    ranged: false,
                    blunt: false,
                    slash: true,
                    pierce: false,
                    swing_precision: 0.0,
                    stab_precision: 0.0,
                    prefers_stab: false,
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
        assert!(bot_entity.contains::<crate::bot::OffensiveCombatAi>());
        assert_eq!(bot_entity.get::<CharacterId>().unwrap().0, 99);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoritativeInputTick, AuthoritativeMovementIntent, AuthoritativePostureIntent,
        BACKWARD_DIVE_POSTURE_TRANSITION_TICKS, CameraFacingIntent, DisconnectedPlayer, DiveLaunch,
        GROUND_POSTURE_TRANSITION_TICKS, MeleeAttackAuthority, Player, RECONNECT_GRACE_SECS,
        ROLL_POSTURE_TRANSITION_TICKS, RangedAttackAuthority, ReconnectSession, WeaponGuardState,
        advance_downed_facing_for_camera, advance_posture_transition_facing,
        apply_dive_launch_velocity, apply_posture_action, authoritative_weapon_guard,
        begin_authoritative_quickstep, combat_seconds_to_ticks, dive_horizontal_velocity,
        downed_tank_controller_input, input, mission_enemy_health_scale, mission_enemy_scale,
        on_client_disconnected, player_collider, posture_transition_locks_body_facing,
        queue_replication_rebind, reconnect_matches, restore_authoritative_movement_intent,
        sequence_is_newer, tactical_movement_speed_for_guard, try_claim_reconnect,
        update_character_motion_snapshots, validate_player_input,
    };
    use adventuresim_tactical_core::physics::tactical_character_controller;
    use adventuresim_tactical_core::prelude::{
        Attributes, BestiaryCategories, BodyState, CharacterControllerState, CharacterId,
        CharacterLook, CharacterMotionSnapshot, CollisionMargin, DiveDirection, DiveTrajectory,
        DodgeSpec, EquipSlot, EquipmentActionState, GroundedPosture, InventoryItems, ItemOf, Limbs,
        LinearVelocity, MeleePreparationInput, MovementPace, PostureTransitionKind, QuickstepPush,
        RollDirection, Rotation, ShieldItem, SkeletonAction, SkeletonState, Skills, Stats,
        TACTICAL_PRONE_LATERAL_SPEED_SCALE, TacticalCombatConfig, TacticalCombatSide,
        TacticalCombatState, advance_body_facing, controller_yaw, downed_camera_roll_target,
    };
    use adventuresim_tactical_netcode::aeronet::io::connection::{DisconnectReason, Disconnected};
    use adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated;
    use adventuresim_tactical_netcode::prelude::{
        JumpCommand, PlayerInputRequest, PostureActionRequest, ReconnectToken,
    };
    use bevy::prelude::*;

    #[derive(Resource)]
    struct RebindTarget(Entity);

    fn valid_player_input_request() -> PlayerInputRequest {
        PlayerInputRequest {
            weapon_guard: WeaponGuardState::Raised,
            melee_preparation: MeleePreparationInput::Preferred,
            ..default()
        }
    }

    fn mark_rebind_target(mut commands: Commands, target: Res<RebindTarget>) {
        queue_replication_rebind(&mut commands, target.0);
    }

    fn despawn_disconnected_session(event: On<Disconnected>, mut commands: Commands) {
        commands.entity(event.event_target()).try_despawn();
    }

    #[test]
    fn disconnect_snapshot_survives_competing_despawn_in_both_observer_orders() {
        for snapshot_observer_first in [true, false] {
            let mut app = App::new();
            if snapshot_observer_first {
                app.add_observer(on_client_disconnected)
                    .add_observer(despawn_disconnected_session)
                    .add_observer(super::on_player_added);
            } else {
                app.add_observer(despawn_disconnected_session)
                    .add_observer(on_client_disconnected)
                    .add_observer(super::on_player_added);
            }
            let session = ReconnectSession {
                character_id: CharacterId(7),
                token: ReconnectToken([7; 32]),
            };
            let limbs = Limbs::default();
            let mass = super::Mass(limbs.body_weight_kg);
            let client = app.world_mut().spawn(session).id();
            app.world_mut().entity_mut(client).insert((
                Name::new("snapshot-player"),
                Player::default(),
                CharacterId(7),
                BestiaryCategories::default(),
                Skills::default(),
                limbs,
                Attributes::default(),
                Stats::default(),
                TacticalCombatState::default(),
                EquipmentActionState::default(),
                TacticalCombatSide::Party,
            ));
            app.world_mut().entity_mut(client).insert((
                Transform::from_xyz(1.0, 2.0, 3.0),
                CharacterLook::default(),
                AuthoritativeMovementIntent::default(),
                AuthoritativePostureIntent::default(),
                MovementPace::default(),
                mass,
                LinearVelocity::ZERO,
                SkeletonState::default(),
                MeleeAttackAuthority::default(),
                RangedAttackAuthority::default(),
            ));
            app.world_mut().entity_mut(client).insert((
                player_collider(),
                CollisionMargin(0.01),
                tactical_character_controller(),
                input::AccumulatedInput::default(),
            ));
            let shield = app
                .world_mut()
                .spawn((
                    ItemOf(client),
                    ShieldItem { block: 1.0 },
                    EquipSlot::HoldingLeft,
                ))
                .id();

            app.world_mut().trigger(Disconnected {
                entity: client,
                reason: DisconnectReason::by_peer("test teardown"),
            });
            app.update();

            assert!(app.world().get_entity(client).is_err());
            let mut disconnected = app
                .world_mut()
                .query_filtered::<Entity, With<DisconnectedPlayer>>();
            let orphans = disconnected.iter(app.world()).collect::<Vec<_>>();
            assert_eq!(orphans.len(), 1);
            let orphan = orphans[0];
            assert_eq!(app.world().get::<ItemOf>(shield).unwrap().0, orphan);
            assert_eq!(
                app.world()
                    .get::<InventoryItems>(orphan)
                    .unwrap()
                    .holding_shield(),
                Some(shield)
            );
            let projection = app
                .world()
                .get::<super::DisconnectedProjection>(orphan)
                .unwrap()
                .clone();
            let rebound = app.world_mut().spawn_empty().id();
            projection.insert(&mut app.world_mut().commands(), rebound);
            app.world_mut().flush();
            assert_eq!(
                app.world().get::<Transform>(rebound).unwrap().translation,
                Vec3::new(1.0, 2.0, 3.0)
            );
            assert_eq!(
                app.world().get::<Name>(rebound).unwrap().as_str(),
                "snapshot-player"
            );
            assert!(app.world().get::<MeleeAttackAuthority>(rebound).is_some());
            assert!(app.world().get::<RangedAttackAuthority>(rebound).is_some());
        }
    }

    #[test]
    fn reconnect_rebind_requires_character_and_single_current_capability() {
        let current = ReconnectToken([7; 32]);
        let session = DisconnectedPlayer {
            character_id: CharacterId(7),
            reconnect_token: current,
            remaining_secs: RECONNECT_GRACE_SECS,
            claimed: false,
        };
        assert!(reconnect_matches(CharacterId(7), Some(current), &session));
        assert!(!reconnect_matches(CharacterId(8), Some(current), &session));
        assert!(!reconnect_matches(
            CharacterId(7),
            Some(ReconnectToken([8; 32])),
            &session
        ));
        assert!(!reconnect_matches(CharacterId(7), None, &session));
        const { assert!(RECONNECT_GRACE_SECS > 0.0) };
    }

    #[test]
    fn consumed_reconnect_capability_cannot_be_reused_after_rotation() {
        let old = ReconnectToken([7; 32]);
        let rotated = DisconnectedPlayer {
            character_id: CharacterId(7),
            reconnect_token: ReconnectToken([9; 32]),
            remaining_secs: RECONNECT_GRACE_SECS,
            claimed: false,
        };
        assert!(!reconnect_matches(CharacterId(7), Some(old), &rotated));
    }

    #[test]
    fn same_frame_duplicate_reconnect_is_claimed_exactly_once() {
        let token = ReconnectToken([4; 32]);
        let mut session = DisconnectedPlayer {
            character_id: CharacterId(7),
            reconnect_token: token,
            remaining_secs: 1.0,
            claimed: false,
        };
        assert!(try_claim_reconnect(
            CharacterId(7),
            Some(token),
            &mut session
        ));
        assert!(!try_claim_reconnect(
            CharacterId(7),
            Some(token),
            &mut session
        ));
    }

    #[test]
    fn reconnect_at_or_after_expiry_deadline_is_rejected() {
        for remaining_secs in [0.0, -f32::EPSILON] {
            let token = ReconnectToken([6; 32]);
            let mut session = DisconnectedPlayer {
                character_id: CharacterId(7),
                reconnect_token: token,
                remaining_secs,
                claimed: false,
            };
            assert!(!try_claim_reconnect(
                CharacterId(7),
                Some(token),
                &mut session
            ));
            assert!(!session.claimed);
        }
    }

    #[test]
    fn reconnect_rebind_marks_new_connection_replicated() {
        let mut app = App::new();
        let target = app.world_mut().spawn_empty().id();
        app.insert_resource(RebindTarget(target));
        app.add_systems(Update, mark_rebind_target);
        app.update();
        assert!(app.world().get::<Replicated>(target).is_some());
    }

    #[test]
    fn disconnected_loading_player_expiry_has_character_without_projection() {
        let marker = DisconnectedPlayer {
            character_id: CharacterId(42),
            reconnect_token: ReconnectToken([3; 32]),
            remaining_secs: RECONNECT_GRACE_SECS,
            claimed: false,
        };
        assert_eq!(marker.character_id, CharacterId(42));
    }

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
            JumpCommand::default(),
            WeaponGuardState::Lowered,
        );
        for (look, movement) in [
            (Vec2::new(f32::NAN, 0.0), Some(Vec2::ZERO)),
            (Vec2::new(0.0, f32::INFINITY), Some(Vec2::ZERO)),
            (Vec2::ZERO, Some(Vec2::new(f32::NEG_INFINITY, 0.0))),
            (Vec2::ZERO, Some(Vec2::new(0.0, f32::NAN))),
        ] {
            if let Some(validated) = validate_player_input(PlayerInputRequest {
                look,
                movement,
                jump: JumpCommand {
                    sequence: 1,
                    ..default()
                },
                pace: MovementPace::Sprint,
                ..valid_player_input_request()
            }) {
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
                JumpCommand::default(),
                WeaponGuardState::Lowered,
            )
        );
    }

    #[test]
    fn jump_sequence_accepts_each_command_once_across_loss_and_reordering() {
        assert!(sequence_is_newer(1, 0));
        assert!(!sequence_is_newer(1, 1));
        assert!(!sequence_is_newer(0, 1));
        assert!(sequence_is_newer(0, u32::MAX));
    }

    #[test]
    fn continuous_input_tick_rejects_duplicates_and_reordered_packets() {
        let mut newest = AuthoritativeInputTick::default();
        assert!(newest.accept(10));
        assert!(!newest.accept(10));
        assert!(!newest.accept(9));
        assert!(newest.accept(11));

        newest.tick = u32::MAX;
        newest.initialized = true;
        assert!(newest.accept(0));
    }

    #[test]
    fn motion_snapshot_acknowledges_the_input_that_produced_it() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Player::default(),
                Transform::from_xyz(2.0, 3.0, 4.0).with_rotation(Quat::from_rotation_y(0.4)),
                LinearVelocity(Vec3::new(1.0, -2.0, 3.0)),
                CharacterControllerState::default(),
                AuthoritativeInputTick {
                    tick: 42,
                    initialized: true,
                },
                QuickstepPush::default(),
                CharacterMotionSnapshot::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(update_character_motion_snapshots);
        schedule.run(&mut world);

        let snapshot = world.get::<CharacterMotionSnapshot>(entity).unwrap();
        assert_eq!(snapshot.acknowledged_input_tick, 42);
        assert_eq!(snapshot.translation, Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(snapshot.linear_velocity, Vec3::new(1.0, -2.0, 3.0));
        assert!(!snapshot.grounded);
        assert!(!snapshot.quickstep_push.active);
    }

    #[test]
    fn player_input_normalizes_finite_boundaries_before_controller_state() {
        let validated = validate_player_input(PlayerInputRequest {
            look: Vec2::new(std::f32::consts::TAU * 4.0 + 0.25, 99.0),
            movement: Some(Vec2::splat(10.0)),
            jump: JumpCommand {
                sequence: 7,
                ..default()
            },
            jump_charge: true,
            downed_align: true,
            pace: MovementPace::Sprint,
            ..valid_player_input_request()
        })
        .unwrap();
        assert!((validated.yaw - 0.25).abs() < 0.0001);
        assert_eq!(validated.pitch, 1.5);
        assert!(validated.movement.unwrap().length() <= 1.0001);
        assert!(validated.yaw.is_finite() && validated.pitch.is_finite());
        assert_eq!(validated.weapon_guard, WeaponGuardState::Raised);
        assert_eq!(validated.jump.sequence, 7);
        assert!(validated.jump_charge);
        assert!(validated.downed_align);
    }

    #[test]
    fn player_input_normalizes_quickstep_direction_and_rejects_non_finite_values() {
        let validated = validate_player_input(PlayerInputRequest {
            look: Vec2::ZERO,
            movement: Some(Vec2::Y),
            jump: JumpCommand {
                sequence: 3,
                quickstep: Some(Vec2::new(4.0, -3.0)),
            },
            pace: MovementPace::Walk,
            ..valid_player_input_request()
        })
        .unwrap();
        assert_eq!(validated.jump.quickstep, Some(Vec2::new(0.8, -0.6)));
        assert!(
            validate_player_input(PlayerInputRequest {
                look: Vec2::ZERO,
                movement: None,
                jump: JumpCommand {
                    sequence: 4,
                    quickstep: Some(Vec2::new(f32::NAN, 0.0)),
                },
                pace: MovementPace::Walk,
                ..valid_player_input_request()
            })
            .is_none()
        );
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
    fn camera_facing_intent_makes_overlapping_modes_unrepresentable() {
        assert_eq!(
            CameraFacingIntent::from_input(WeaponGuardState::Lowered, false),
            CameraFacingIntent::Free
        );
        assert_eq!(
            CameraFacingIntent::from_input(WeaponGuardState::Raised, false),
            CameraFacingIntent::Aim
        );
        assert_eq!(
            CameraFacingIntent::from_input(WeaponGuardState::Raised, true),
            CameraFacingIntent::DownedAlign
        );
    }

    #[test]
    fn backward_dive_and_supine_get_up_transfer_opposite_root_half_turns() {
        let mut skeleton = SkeletonState::default();
        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
                trajectory: DiveTrajectory::Airborne,
            },
            0,
            BACKWARD_DIVE_POSTURE_TRANSITION_TICKS,
        ));
        let mut transform = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI));
        let mut physics_rotation = Rotation(transform.rotation);

        skeleton.transition_body(BodyState::Airborne);
        skeleton.advance_posture_transition(1);
        skeleton.transition_body(BodyState::Grounded(GroundedPosture::Upright));
        skeleton.advance_posture_transition(2);
        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(BACKWARD_DIVE_POSTURE_TRANSITION_TICKS + 2);
        advance_posture_transition_facing(
            &mut transform,
            &mut physics_rotation,
            previous,
            skeleton.posture_transition(),
        );
        assert_eq!(skeleton.body(), BodyState::Supine);
        assert!((transform.rotation * Vec3::Z).abs_diff_eq(Vec3::Z, 0.000_01));

        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::SupineToUpright,
            BACKWARD_DIVE_POSTURE_TRANSITION_TICKS + 3,
            GROUND_POSTURE_TRANSITION_TICKS,
        ));
        assert_eq!(
            CameraFacingIntent::from_input(WeaponGuardState::Raised, false),
            CameraFacingIntent::Aim
        );
        let landing_rotation = transform.rotation;
        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(
            BACKWARD_DIVE_POSTURE_TRANSITION_TICKS + 3 + GROUND_POSTURE_TRANSITION_TICKS / 2,
        );
        advance_posture_transition_facing(
            &mut transform,
            &mut physics_rotation,
            previous,
            skeleton.posture_transition(),
        );
        assert!(
            transform.rotation.abs_diff_eq(landing_rotation, 0.000_01),
            "supine-to-midpoint recovery must not begin the counter-yaw"
        );

        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(
            BACKWARD_DIVE_POSTURE_TRANSITION_TICKS + GROUND_POSTURE_TRANSITION_TICKS + 4,
        );
        advance_posture_transition_facing(
            &mut transform,
            &mut physics_rotation,
            previous,
            skeleton.posture_transition(),
        );
        assert_eq!(
            skeleton.body(),
            BodyState::Grounded(GroundedPosture::Upright)
        );
        assert!((transform.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 0.000_01));
        let standing_rotation = transform.rotation;
        transform.rotation = advance_body_facing(
            transform.rotation,
            Quat::IDENTITY,
            Vec3::ZERO,
            SkeletonAction::None,
            WeaponGuardState::Raised,
            1.0,
        );
        assert!(!transform.rotation.abs_diff_eq(standing_rotation, 0.000_01));
        assert!(!transform.rotation.abs_diff_eq(landing_rotation, 0.000_01));
    }

    #[test]
    fn prone_get_up_still_preserves_its_root_heading() {
        let mut skeleton = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::ProneToUpright,
            0,
            GROUND_POSTURE_TRANSITION_TICKS,
        ));
        let initial = Quat::from_rotation_y(0.73);
        let mut transform = Transform::from_rotation(initial);
        let mut physics_rotation = Rotation(initial);

        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS / 2);
        advance_posture_transition_facing(
            &mut transform,
            &mut physics_rotation,
            previous,
            skeleton.posture_transition(),
        );
        assert!(transform.rotation.abs_diff_eq(initial, 0.000_01));

        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS + 1);
        advance_posture_transition_facing(
            &mut transform,
            &mut physics_rotation,
            previous,
            skeleton.posture_transition(),
        );
        assert!(transform.rotation.abs_diff_eq(initial, 0.000_01));
    }

    #[test]
    fn toggle_roll_and_supine_release_use_authoritative_transition_sequence() {
        let mut skeleton = SkeletonState::default();
        let mut input = input::AccumulatedInput::default();
        let config = TacticalCombatConfig::default();
        let _ = apply_posture_action(
            PostureActionRequest::Toggle,
            &mut skeleton,
            &mut input,
            MovementPace::Walk,
            &config,
        );
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::UprightToProne
        );
        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS);
        assert_eq!(skeleton.body(), BodyState::Prone);

        let _ = apply_posture_action(
            PostureActionRequest::RollLeft,
            &mut skeleton,
            &mut input,
            MovementPace::Walk,
            &config,
        );
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Left,
            }
        );
        assert_eq!(skeleton.downed_lateral_motion(), -1.0);
        skeleton.advance_posture_transition(ROLL_POSTURE_TRANSITION_TICKS - 1);
        assert_eq!(skeleton.body(), BodyState::Prone);
        assert!(skeleton.is_posture_transitioning());
        skeleton.advance_posture_transition(ROLL_POSTURE_TRANSITION_TICKS);
        assert_eq!(skeleton.body(), BodyState::Supine);

        let _ = apply_posture_action(
            PostureActionRequest::Toggle,
            &mut skeleton,
            &mut input,
            MovementPace::Walk,
            &config,
        );
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::SupineToUpright
        );
    }

    #[test]
    fn free_camera_cannot_reverse_a_completed_right_roll() {
        let mut skeleton = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Right,
            },
            0,
            ROLL_POSTURE_TRANSITION_TICKS,
        ));
        skeleton.advance_posture_transition(ROLL_POSTURE_TRANSITION_TICKS);
        assert_eq!(skeleton.body(), BodyState::Supine);

        // A camera looking toward the character's feet maps back toward the
        // prone sector, but it is inert without held aim.
        advance_downed_facing_for_camera(&mut skeleton, CameraFacingIntent::Free, 0.0, 1.0);
        assert_eq!(skeleton.body(), BodyState::Supine);
        assert!(skeleton.downed_facing().is_none());
    }

    #[test]
    fn dive_preserves_requested_direction_and_starts_airborne_motion() {
        let mut skeleton = SkeletonState::default();
        let mut input = input::AccumulatedInput::default();
        let config = TacticalCombatConfig::default();
        let launched = apply_posture_action(
            PostureActionRequest::Dive {
                animation_direction: DiveDirection::Forward,
                travel_direction: DiveDirection::Backward,
            },
            &mut skeleton,
            &mut input,
            MovementPace::Walk,
            &config,
        );
        assert_eq!(
            launched,
            Some(DiveLaunch {
                travel_direction: DiveDirection::Backward,
                trajectory: DiveTrajectory::Airborne,
            })
        );
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Forward,
                trajectory: DiveTrajectory::Airborne,
            }
        );
        assert!(input.jumped.is_some());
    }

    #[test]
    fn sprinting_dive_becomes_grounded_opposite_animation_slide_to_supine() {
        let mut skeleton = SkeletonState::default();
        let mut input = input::AccumulatedInput::default();
        let config = TacticalCombatConfig::default();
        let launched = apply_posture_action(
            PostureActionRequest::Dive {
                animation_direction: DiveDirection::Forward,
                travel_direction: DiveDirection::Forward,
            },
            &mut skeleton,
            &mut input,
            MovementPace::Sprint,
            &config,
        );

        assert_eq!(
            launched,
            Some(DiveLaunch {
                travel_direction: DiveDirection::Forward,
                trajectory: DiveTrajectory::GroundedSlide,
            })
        );
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
                trajectory: DiveTrajectory::GroundedSlide,
            }
        );
        assert!(input.jumped.is_none());

        let duration = combat_seconds_to_ticks(config.movement.maneuvers.slide_seconds);
        skeleton.advance_posture_transition(duration);
        assert_eq!(skeleton.body(), BodyState::Supine);
    }

    #[test]
    fn movement_intent_survives_missing_unreliable_packets_until_explicit_stop() {
        let mut world = World::new();
        world.insert_resource(TacticalCombatConfig::default());
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::X)),
                SkeletonState::default(),
                AuthoritativePostureIntent::default(),
                QuickstepPush::default(),
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

    #[test]
    fn quickstep_load_drives_the_root_before_takeoff() {
        let mut skeleton = SkeletonState::default();
        skeleton
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 20)
            .unwrap();
        assert!(!skeleton.quickstep_is_launched());
        let mut world = World::new();
        world.insert_resource(TacticalCombatConfig::default());
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(None),
                skeleton,
                AuthoritativePostureIntent::default(),
                QuickstepPush {
                    start_tick: 0,
                    direction: Vec2::X,
                    orientation: Quat::IDENTITY,
                    origin: Vec3::ZERO,
                    active: true,
                },
                input::AccumulatedInput::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(restore_authoritative_movement_intent);
        schedule.run(&mut world);

        assert_eq!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .last_movement,
            Some(Vec2::X)
        );
    }

    #[test]
    fn authoritative_quickstep_starts_animation_and_force_actuator_together() {
        let mut skeleton = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        let mut push = QuickstepPush::default();

        assert!(begin_authoritative_quickstep(
            &mut skeleton,
            &mut push,
            Vec2::X,
            Quat::IDENTITY,
            Vec3::new(2.0, 0.0, 3.0),
            &TacticalCombatConfig::default(),
        ));
        assert_eq!(skeleton.action_kind(), SkeletonAction::Dodge);
        assert_eq!(skeleton.action_direction(), Vec2::X);
        assert!(push.active);
        assert_eq!(push.direction, Vec2::X);
        assert_eq!(push.origin, Vec3::new(2.0, 0.0, 3.0));
    }

    #[test]
    fn downed_tank_input_is_body_relative_with_supine_feet_at_half_speed() {
        let body = Quat::from_rotation_y(0.4);
        for camera_yaw in [0.0, 0.9, 2.7, -1.4] {
            let controller = Quat::from_rotation_y(camera_yaw);
            for (input, expected_body_local) in [
                (Vec2::Y, Vec3::Z),
                (-Vec2::Y, Vec3::NEG_Z),
                (Vec2::X, Vec3::NEG_X * TACTICAL_PRONE_LATERAL_SPEED_SCALE),
                (-Vec2::X, Vec3::X * TACTICAL_PRONE_LATERAL_SPEED_SCALE),
            ] {
                let resolved = downed_tank_controller_input(
                    input,
                    BodyState::Prone,
                    body,
                    controller,
                    TACTICAL_PRONE_LATERAL_SPEED_SCALE,
                );
                let resolved_world =
                    controller_yaw(controller) * Vec3::new(resolved.x, 0.0, -resolved.y);
                let expected_world = controller_yaw(body) * expected_body_local;
                assert!(resolved_world.abs_diff_eq(expected_world, 0.0001));
            }
            let supine_head = downed_tank_controller_input(
                Vec2::Y,
                BodyState::Supine,
                body,
                controller,
                TACTICAL_PRONE_LATERAL_SPEED_SCALE,
            );
            let supine_feet = downed_tank_controller_input(
                -Vec2::Y,
                BodyState::Supine,
                body,
                controller,
                TACTICAL_PRONE_LATERAL_SPEED_SCALE,
            );
            assert!((supine_head.length() - 1.0).abs() < 0.0001);
            assert!((supine_feet.length() - 0.5).abs() < 0.0001);
        }
    }

    #[test]
    fn aiming_while_prone_suppresses_normal_movement() {
        let mut world = World::new();
        world.insert_resource(TacticalCombatConfig::default());
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::Y)),
                SkeletonState::default().with_body_state(BodyState::Prone),
                AuthoritativePostureIntent {
                    facing: CameraFacingIntent::Aim,
                    ..default()
                },
                QuickstepPush::default(),
                CharacterControllerState::default(),
                Transform::default(),
                input::AccumulatedInput::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(restore_authoritative_movement_intent);
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .last_movement,
            None
        );
    }

    #[test]
    fn roll_transition_overrides_normal_movement_with_lateral_controller_input() {
        let body_orientation = Quat::from_rotation_y(0.4);
        let controller_orientation = Quat::from_rotation_y(1.2);
        let mut skeleton = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Left,
            },
            0,
            GROUND_POSTURE_TRANSITION_TICKS,
        ));
        let mut world = World::new();
        world.insert_resource(TacticalCombatConfig::default());
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::Y)),
                skeleton,
                AuthoritativePostureIntent::default(),
                QuickstepPush::default(),
                CharacterControllerState {
                    orientation: controller_orientation,
                    ..default()
                },
                Transform::from_rotation(body_orientation),
                input::AccumulatedInput::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(restore_authoritative_movement_intent);
        schedule.run(&mut world);
        let resolved = world
            .get::<input::AccumulatedInput>(player)
            .unwrap()
            .last_movement
            .unwrap();
        let resolved_world =
            controller_yaw(controller_orientation) * Vec3::new(resolved.x, 0.0, -resolved.y);
        let expected_world = controller_yaw(body_orientation) * Vec3::X;
        assert!(resolved_world.abs_diff_eq(expected_world, 0.0001));
    }

    #[test]
    fn authored_roll_locks_root_facing_until_contact() {
        let mut skeleton = SkeletonState::default().with_body_state(BodyState::Prone);
        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Right,
            },
            0,
            GROUND_POSTURE_TRANSITION_TICKS,
        ));
        assert!(posture_transition_locks_body_facing(&skeleton));

        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS);
        assert!(!posture_transition_locks_body_facing(&skeleton));
        assert_eq!(skeleton.body(), BodyState::Supine);
    }

    #[test]
    fn every_authored_posture_transition_locks_root_facing() {
        for transition in [
            PostureTransitionKind::UprightToProne,
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Left,
                trajectory: DiveTrajectory::Airborne,
            },
        ] {
            let mut skeleton = SkeletonState::default();
            assert!(skeleton.begin_posture_transition(transition, 0, 10));
            assert!(posture_transition_locks_body_facing(&skeleton));
        }

        for transition in [
            PostureTransitionKind::ProneToUpright,
            PostureTransitionKind::ProneToSupine {
                direction: RollDirection::Right,
            },
        ] {
            let mut skeleton = SkeletonState::default().with_body_state(BodyState::Prone);
            assert!(skeleton.begin_posture_transition(transition, 0, 10));
            assert!(posture_transition_locks_body_facing(&skeleton));
        }

        let mut supine = SkeletonState::default().with_body_state(BodyState::Supine);
        assert!(supine.begin_posture_transition(PostureTransitionKind::SupineToUpright, 0, 10));
        assert!(posture_transition_locks_body_facing(&supine));
    }

    #[test]
    fn dive_launch_velocity_follows_the_requested_camera_relative_direction() {
        let yaw = std::f32::consts::FRAC_PI_2;
        assert!(dive_horizontal_velocity(yaw, DiveDirection::Forward, 7.0).x < -6.9);
        assert!(dive_horizontal_velocity(yaw, DiveDirection::Backward, 7.0).x > 6.9);
        assert!(dive_horizontal_velocity(0.0, DiveDirection::Left, 7.0).x < -6.9);
        assert!(dive_horizontal_velocity(0.0, DiveDirection::Right, 7.0).x > 6.9);
    }

    #[test]
    fn grounded_slide_preserves_the_complete_sprint_velocity() {
        let mut velocity = LinearVelocity(Vec3::new(1.0, 4.0, 2.0));
        apply_dive_launch_velocity(
            &mut velocity,
            0.0,
            DiveLaunch {
                travel_direction: DiveDirection::Forward,
                trajectory: DiveTrajectory::GroundedSlide,
            },
            7.0,
        );

        assert_eq!(velocity.0, Vec3::new(1.0, 4.0, 2.0));
    }

    #[test]
    fn directional_dive_handoff_preserves_its_landing_heading() {
        for (direction, expected_world_heading, expected_half_roll) in [
            (DiveDirection::Forward, Vec3::NEG_Z, None),
            (DiveDirection::Backward, Vec3::Z, None),
            (DiveDirection::Left, Vec3::NEG_X, Some(-0.5)),
            (DiveDirection::Right, Vec3::X, Some(0.5)),
        ] {
            let mut skeleton = SkeletonState::default();
            assert!(skeleton.begin_posture_transition(
                PostureTransitionKind::DiveToDowned {
                    direction,
                    trajectory: DiveTrajectory::Airborne,
                },
                0,
                10,
            ));
            let mut transform =
                Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI));
            let mut physics_rotation = Rotation(transform.rotation);

            skeleton.transition_body(BodyState::Airborne);
            skeleton.advance_posture_transition(1);
            skeleton.transition_body(BodyState::Grounded(GroundedPosture::Upright));
            skeleton.advance_posture_transition(2);

            let previous = skeleton.posture_transition();
            skeleton.advance_posture_transition(7);
            advance_posture_transition_facing(
                &mut transform,
                &mut physics_rotation,
                previous,
                skeleton.posture_transition(),
            );
            let halfway = transform.rotation * Vec3::Z;
            if direction != DiveDirection::Forward {
                assert!(
                    !halfway.abs_diff_eq(Vec3::NEG_Z, 0.000_01),
                    "{direction:?} recovery left the complete yaw handoff until its endpoint"
                );
                assert!(
                    !halfway.abs_diff_eq(expected_world_heading, 0.000_01),
                    "{direction:?} recovery applied the complete yaw handoff at first contact"
                );
            }

            let previous = skeleton.posture_transition();
            skeleton.advance_posture_transition(12);
            advance_posture_transition_facing(
                &mut transform,
                &mut physics_rotation,
                previous,
                skeleton.posture_transition(),
            );
            let facing = transform.rotation * Vec3::Z;
            assert!(
                facing.abs_diff_eq(expected_world_heading, 0.000_01),
                "{direction:?}"
            );
            assert!(
                physics_rotation.0.abs_diff_eq(transform.rotation, 0.000_01),
                "{direction:?} physics rotation diverged from replicated transform"
            );
            assert_eq!(
                skeleton.downed_facing().map(|facing| facing.half_turns()),
                expected_half_roll,
                "{direction:?}"
            );
            if let Some(expected_half_roll) = expected_half_roll {
                assert!(
                    (downed_camera_roll_target(transform.rotation, Quat::IDENTITY)
                        - expected_half_roll)
                        .abs()
                        < 0.000_01,
                    "{direction:?}"
                );
            }
        }
    }
}
