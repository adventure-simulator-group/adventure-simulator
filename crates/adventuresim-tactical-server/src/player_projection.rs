use std::{collections::BTreeMap, num::NonZeroU32};

use adventuresim_stdb_client::*;
use adventuresim_tactical_core::physics::TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND;
use adventuresim_tactical_core::{inventory::ItemProperties, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{DisconnectReason, Disconnected},
    bevy_replicon::prelude::{FromClient, Replicated, SendTargets, ServerTriggerExt, ToClients},
    prelude::{
        JoinRequest, JumpCommand, PlayerInputRequest, PostureActionRequest, PostureCommand,
        ReconnectCapability, ReconnectToken,
    },
};
use bevy::prelude::*;
use bevy::time::Stopwatch;

use crate::{
    Args, SceneVistaBundleResource,
    bot::{MissionEnemy, OffensiveCombatAi},
    combat::{MeleeAttackAuthority, RangedAttackAuthority, TacticalCombatSide},
    equipment::{
        LastEquipmentSequence, PendingEquipmentActions, purge_equipment_lifecycle,
        reconnect_equipment_lifecycle,
    },
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
pub(crate) struct AuthoritativeMovementIntent(pub(crate) Option<Vec2>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AuthoritativePostureIntent {
    crouch: bool,
    facing: CameraFacingIntent,
    last_jump_sequence: u32,
    last_command_sequence: u32,
}

/// One camera-facing owner is selected per accepted input. In particular,
/// aim-following and modifier-driven body alignment cannot both be active, and
/// there is no persistent "suspended while upright" combination to leak out
/// of an authored posture transition.
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

const GROUND_POSTURE_TRANSITION_TICKS: u64 = 51;
const ROLL_POSTURE_TRANSITION_TICKS: u64 = GROUND_POSTURE_TRANSITION_TICKS.div_ceil(2);
// Five authored frames at 30 FPS, rounded up to the 64 Hz fixed simulation.
const DIVE_POSTURE_TRANSITION_TICKS: u64 = 20;
const BACKWARD_DIVE_POSTURE_TRANSITION_TICKS: u64 = 32;

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
            enemy.insert(OffensiveCombatAi::default());
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

    cmd.entity(entity).insert(MissionOpeningAwareness {
        party_has_surprise: player.party_has_surprise,
    });
    cmd.entity(entity).remove::<LoadingPlayer>().insert((
        Name::new(name),
        Replicated,
        Player {
            name: player.character.name.clone(),
        },
        CharacterId(player.character.id),
        BestiaryCategories::default(),
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
                    prefers_stab: item.item.prefers_stab,
                    penetration: item.item.penetration,
                    reach: item.item.reach,
                    balance: item.item.balance,
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
        "Player {entity:?} is fully loaded"
    );
}

pub(crate) fn on_join_request(
    join: On<FromClient<JoinRequest>>,
    mut commands: Commands,
    mut state: ResMut<MissionState>,
    loading_players: Query<(), With<LoadingPlayer>>,
    players: Query<(), With<Player>>,
    mut disconnected_players: Query<(
        Entity,
        &mut DisconnectedPlayer,
        Has<Player>,
        Has<LoadingPlayer>,
    )>,
    inventory_items: Query<(Entity, &ItemOf)>,
    mut pending_actions: ResMut<PendingEquipmentActions>,
    mut action_sequences: ResMut<LastEquipmentSequence>,
    conn: Res<SpacetimeDb>,
    ready: Res<SpacetimeDbReady>,
    vista: Res<SceneVistaBundleResource>,
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
            .find_map(|(entity, mut session, projected, loading)| {
                try_claim_reconnect(join.character_id, join.reconnect_token, &mut session)
                    .then_some((entity, projected, loading))
            });
    if let Some((disconnected, projected, loading)) = reconnect {
        let token = fresh_reconnect_token();
        if projected {
            queue_replication_rebind(&mut commands, client);
        }
        commands.entity(disconnected).move_components::<(
            Name,
            Player,
            CharacterId,
            BestiaryCategories,
            Skills,
            Limbs,
            Attributes,
            Stats,
            TacticalCombatState,
            EquipmentActionState,
            TacticalCombatSide,
        )>(client);
        commands.entity(disconnected).move_components::<(
            Transform,
            CharacterLook,
            AuthoritativeMovementIntent,
            AuthoritativePostureIntent,
            MovementPace,
            LinearVelocity,
            SkeletonState,
            MeleeAttackAuthority,
            RangedAttackAuthority,
        )>(client);
        commands.entity(disconnected).move_components::<(
            Collider,
            CollisionMargin,
            CharacterController,
            AccumulatedInput,
        )>(client);
        if projected {
            commands
                .entity(disconnected)
                .move_components::<InventoryItems>(client);
        } else if loading {
            commands
                .entity(disconnected)
                .move_components::<LoadingPlayer>(client);
        }
        commands.entity(client).insert(ReconnectSession {
            character_id: join.character_id,
            token,
        });
        send_reconnect_capability(&mut commands, join.client_id, join.character_id, token);
        send_scene_vista(&mut commands, join.client_id, &vista);
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
    info!(
        "Character {} connected and entered mission, awaiting loading",
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
            &mut AuthoritativePostureIntent,
            &mut MovementPace,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
) {
    let Some(validated) = validate_player_input(
        input.look,
        input.movement,
        input.jump,
        input.crouch,
        input.jump_charge,
        input.downed_align,
        input.posture,
        input.pace,
        input.weapon_guard,
    ) else {
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
        mut posture_intent,
        mut pace,
        mut velocity,
    )) = players.get_mut(entity)
    else {
        return;
    };
    let jump_requested =
        sequence_is_newer(validated.jump.sequence, posture_intent.last_jump_sequence);
    if jump_requested {
        // Consume the edge even when the current body state cannot jump. The
        // client repeats this sequence indefinitely, so retaining it through
        // incapacitation or an airborne interval would create a stale jump
        // as soon as the player became grounded again.
        posture_intent.last_jump_sequence = validated.jump.sequence;
    }
    if combat_state.is_incapacitated() {
        accumulated_input.last_movement = None;
        movement_intent.0 = None;
        accumulated_input.jumped = None;
        accumulated_input.crouched = false;
        posture_intent.crouch = false;
        posture_intent.facing = CameraFacingIntent::Free;
        skeleton.set_jump_anticipation(false);
        set_weapon_guard(
            &mut skeleton,
            authoritative_weapon_guard(validated.weapon_guard, true),
        );
        return;
    }
    look.yaw = validated.yaw;
    look.pitch = validated.pitch;
    accumulated_input.last_movement = validated.movement;
    if sequence_is_newer(
        validated.posture.sequence,
        posture_intent.last_command_sequence,
    ) {
        posture_intent.last_command_sequence = validated.posture.sequence;
        if let Some(action) = validated.posture.action
            && let Some(direction) =
                apply_posture_action(action, &mut skeleton, &mut accumulated_input)
        {
            let horizontal = dive_horizontal_velocity(look.yaw, direction);
            velocity.x = horizontal.x;
            velocity.z = horizontal.z;
        }
    }
    accumulated_input.crouched =
        validated.crouch || skeleton.body().is_downed() || skeleton.is_posture_transitioning();
    movement_intent.0 = validated.movement;
    posture_intent.crouch = validated.crouch;
    posture_intent.facing =
        CameraFacingIntent::from_input(validated.weapon_guard, validated.downed_align);
    skeleton.set_jump_anticipation(validated.jump_charge);
    *pace = validated.pace;
    set_weapon_guard(
        &mut skeleton,
        authoritative_weapon_guard(validated.weapon_guard, false),
    );
    if jump_requested
        && !skeleton.is_posture_transitioning()
        && matches!(skeleton.body(), BodyState::Grounded(_))
    {
        accumulated_input.jumped = Some(Stopwatch::new());
    }
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
    crouch: bool,
    jump_charge: bool,
    downed_align: bool,
    posture: PostureCommand,
    pace: MovementPace,
    weapon_guard: WeaponGuardState,
}

fn validate_player_input(
    look: Vec2,
    movement: Option<Vec2>,
    jump: JumpCommand,
    crouch: bool,
    jump_charge: bool,
    downed_align: bool,
    posture: PostureCommand,
    pace: MovementPace,
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
        crouch,
        jump_charge,
        downed_align,
        posture,
        pace,
        weapon_guard,
    })
}

fn apply_posture_action(
    action: PostureActionRequest,
    skeleton: &mut SkeletonState,
    accumulated_input: &mut AccumulatedInput,
) -> Option<DiveDirection> {
    let tick = skeleton.locomotion_sample_tick;
    let transition = match action {
        PostureActionRequest::Toggle => match skeleton.body() {
            BodyState::Grounded(_) => Some(PostureTransitionKind::UprightToProne),
            BodyState::Prone => Some(PostureTransitionKind::ProneToUpright),
            BodyState::Supine => Some(PostureTransitionKind::SupineToUpright),
            BodyState::Airborne | BodyState::Ragdolled => None,
        },
        PostureActionRequest::RollLeft => roll_transition(skeleton.body(), RollDirection::Left),
        PostureActionRequest::RollRight => roll_transition(skeleton.body(), RollDirection::Right),
        PostureActionRequest::Dive { direction } => {
            matches!(skeleton.body(), BodyState::Grounded(_))
                .then_some(PostureTransitionKind::DiveToDowned { direction })
        }
    };
    let transition = transition?;
    let duration = match transition {
        PostureTransitionKind::DiveToDowned {
            direction: DiveDirection::Backward,
        } => BACKWARD_DIVE_POSTURE_TRANSITION_TICKS,
        PostureTransitionKind::DiveToDowned { .. } => DIVE_POSTURE_TRANSITION_TICKS,
        PostureTransitionKind::ProneToSupine { .. }
        | PostureTransitionKind::SupineToProne { .. } => ROLL_POSTURE_TRANSITION_TICKS,
        _ => GROUND_POSTURE_TRANSITION_TICKS,
    };
    if !skeleton.begin_posture_transition(transition, tick, duration) {
        return None;
    }
    if let PostureTransitionKind::DiveToDowned { direction } = transition {
        accumulated_input.jumped = Some(Stopwatch::new());
        return Some(direction);
    }
    None
}

fn dive_horizontal_velocity(yaw: f32, direction: DiveDirection) -> Vec3 {
    let local = match direction {
        DiveDirection::Forward => Vec3::NEG_Z,
        DiveDirection::Backward => Vec3::Z,
        DiveDirection::Left => Vec3::NEG_X,
        DiveDirection::Right => Vec3::X,
    };
    Quat::from_rotation_y(yaw) * local * TACTICAL_DIVE_HORIZONTAL_SPEED_METRES_PER_SECOND
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

fn prone_tank_controller_input(
    movement: Vec2,
    body_orientation: Quat,
    controller_orientation: Quat,
) -> Vec2 {
    let body_local = Vec3::new(
        -movement.x * TACTICAL_PRONE_LATERAL_SPEED_SCALE,
        0.0,
        movement.y,
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

/// Rehydrates Ahoy's disposable fixed-loop input from the latest accepted
/// complete request before movement runs. Ahoy may clear its accumulator after
/// every fixed loop without turning a missing network packet into a stop.
pub(crate) fn restore_authoritative_movement_intent(
    mut players: Query<
        (
            &AuthoritativeMovementIntent,
            &SkeletonState,
            &AuthoritativePostureIntent,
            Option<&CharacterControllerState>,
            Option<&Transform>,
            &mut AccumulatedInput,
        ),
        With<Player>,
    >,
) {
    for (movement_intent, skeleton, posture, controller, transform, mut accumulated_input) in
        &mut players
    {
        accumulated_input.last_movement =
            if let Some((direction, speed)) = skeleton.attack_movement() {
                let cap = match skeleton.weapon_guard() {
                    WeaponGuardState::Lowered => TACTICAL_RUN_SPEED_METRES_PER_SECOND,
                    WeaponGuardState::Raised => TACTICAL_GUARD_SPEED_METRES_PER_SECOND,
                };
                // A moving attack owns its captured movement through the
                // completed switching action. Releasing or reversing input
                // cannot stop the controller underneath the attack step; the
                // latest player intent resumes once the end guard commits.
                (speed > 0.01 && direction != Vec2::ZERO)
                    .then_some(direction * (speed / cap).clamp(0.0, 1.0))
            } else {
                movement_intent.0
            };
        if skeleton.body() == BodyState::Prone
            && skeleton.attack_movement().is_none()
            && let (Some(controller), Some(transform), Some(movement)) =
                (controller, transform, accumulated_input.last_movement)
        {
            accumulated_input.last_movement = Some(prone_tank_controller_input(
                movement,
                transform.rotation,
                controller.orientation,
            ));
        }
        accumulated_input.crouched =
            posture.crouch || skeleton.body().is_downed() || skeleton.is_posture_transitioning();
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
    mut players: Query<
        (
            &CharacterControllerState,
            &LinearVelocity,
            &mut SkeletonState,
            &mut Transform,
            &TacticalCombatState,
            &MovementPace,
            &AuthoritativePostureIntent,
        ),
        With<Player>,
    >,
) {
    for (controller, velocity, mut skeleton, mut transform, combat_state, pace, posture) in
        &mut players
    {
        if combat_state.is_incapacitated() {
            let lowered = authoritative_weapon_guard(skeleton.weapon_guard(), true);
            set_weapon_guard(&mut skeleton, lowered);
        }
        let tick = (time.elapsed_secs_f64() * LOCOMOTION_SAMPLE_HZ as f64).round() as u64;
        let posture_transitioning = posture_transition_locks_body_facing(&skeleton);
        if posture_transitioning {
            // Authored transitions own their direction relative to a fixed
            // root. This also suspends held downed alignment until a roll or
            // get-up has reached its endpoint.
            skeleton.set_downed_turning(false);
        } else if skeleton.body().is_downed() && !skeleton.is_posture_transitioning() {
            let target = downed_camera_roll_target(transform.rotation, controller.orientation);
            if posture.facing == CameraFacingIntent::DownedAlign {
                let next = advance_downed_body_facing(
                    transform.rotation,
                    controller.orientation,
                    time.delta_secs(),
                );
                skeleton.set_downed_turning(transform.rotation.angle_between(next) > 1.0e-5);
                transform.rotation = next;
                skeleton.advance_downed_facing(
                    target,
                    false,
                    time.delta_secs() * LOCOMOTION_SAMPLE_HZ
                        / GROUND_POSTURE_TRANSITION_TICKS as f32,
                );
            } else {
                skeleton.set_downed_turning(false);
                skeleton.advance_downed_facing(
                    target,
                    posture.facing == CameraFacingIntent::Aim,
                    time.delta_secs() * LOCOMOTION_SAMPLE_HZ
                        / GROUND_POSTURE_TRANSITION_TICKS as f32,
                );
            }
        } else {
            skeleton.set_downed_turning(false);
            transform.rotation = advance_body_facing(
                transform.rotation,
                controller.orientation,
                velocity.0,
                skeleton.action_kind(),
                skeleton.weapon_guard(),
                time.delta_secs(),
            );
        }
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
        let previous_transition = skeleton.posture_transition();
        skeleton.advance_posture_transition(tick);
        advance_posture_transition_facing(
            &mut transform,
            previous_transition,
            skeleton.posture_transition(),
        );
        skeleton.set_guarded_sprint_locomotion(*pace == MovementPace::Sprint);
    }
}

pub(crate) fn on_client_disconnected(
    disconnected: On<Disconnected>,
    query: Query<&ReconnectSession>,
    mut commands: Commands,
) -> Result {
    let entity = disconnected.event_target();
    let Ok(session) = query.get(entity) else {
        return Ok(());
    };
    commands.entity(entity).insert(DisconnectedPlayer {
        character_id: session.character_id,
        reconnect_token: session.token,
        remaining_secs: RECONNECT_GRACE_SECS,
        claimed: false,
    });
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
    previous_transition: Option<PostureTransitionState>,
    current_transition: Option<PostureTransitionState>,
) {
    // Directional dives transfer their authored yaw to the root during
    // landing. Supine get-up applies an inverse half-turn that cancels the
    // authored pose's implicit convention change in world space. Prone get-up
    // receives neither correction.
    transform.rotation = (transform.rotation
        * dive_landing_facing_delta(previous_transition, current_transition)
        * supine_get_up_counter_yaw_delta(previous_transition, current_transition))
    .normalize();
}

fn player_collider() -> Collider {
    Collider::cylinder(0.4, 1.9)
}

fn player_spawn_offset(collider: &Collider) -> f32 {
    -collider.aabb(default(), Rotation::default()).min.y
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoritativeMovementIntent, AuthoritativePostureIntent,
        BACKWARD_DIVE_POSTURE_TRANSITION_TICKS, CameraFacingIntent, DisconnectedPlayer,
        GROUND_POSTURE_TRANSITION_TICKS, Player, RECONNECT_GRACE_SECS,
        ROLL_POSTURE_TRANSITION_TICKS, WeaponGuardState, advance_posture_transition_facing,
        apply_posture_action, authoritative_weapon_guard, dive_horizontal_velocity, input,
        mission_enemy_health_scale, mission_enemy_scale, posture_transition_locks_body_facing,
        prone_tank_controller_input, queue_replication_rebind, reconnect_matches,
        restore_authoritative_movement_intent, sequence_is_newer,
        tactical_movement_speed_for_guard, try_claim_reconnect, validate_player_input,
    };
    use adventuresim_tactical_core::prelude::{
        AttackSpec, BodyState, CharacterControllerState, CharacterId, DiveDirection,
        GroundedPosture, MovementPace, PostureTransitionKind, RollDirection, SkeletonAction,
        SkeletonState, TACTICAL_PRONE_LATERAL_SPEED_SCALE, advance_body_facing, controller_yaw,
        downed_camera_roll_target,
    };
    use adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated;
    use adventuresim_tactical_netcode::prelude::{
        JumpCommand, PostureActionRequest, PostureCommand, ReconnectToken,
    };
    use bevy::prelude::*;

    #[derive(Resource)]
    struct RebindTarget(Entity);

    fn mark_rebind_target(mut commands: Commands, target: Res<RebindTarget>) {
        queue_replication_rebind(&mut commands, target.0);
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
        assert!(RECONNECT_GRACE_SECS > 0.0);
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
            if let Some(validated) = validate_player_input(
                look,
                movement,
                JumpCommand { sequence: 1 },
                false,
                false,
                false,
                PostureCommand::default(),
                MovementPace::Sprint,
                WeaponGuardState::Raised,
            ) {
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
    fn player_input_normalizes_finite_boundaries_before_controller_state() {
        let validated = validate_player_input(
            Vec2::new(std::f32::consts::TAU * 4.0 + 0.25, 99.0),
            Some(Vec2::splat(10.0)),
            JumpCommand { sequence: 7 },
            false,
            true,
            true,
            PostureCommand::default(),
            MovementPace::Sprint,
            WeaponGuardState::Raised,
        )
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
    fn guarded_backward_dive_then_get_up_commits_the_supine_counter_yaw() {
        let mut skeleton = SkeletonState::default();
        assert!(skeleton.begin_posture_transition(
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
            },
            0,
            BACKWARD_DIVE_POSTURE_TRANSITION_TICKS,
        ));
        let mut transform = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI));

        skeleton.transition_body(BodyState::Airborne);
        skeleton.advance_posture_transition(1);
        skeleton.transition_body(BodyState::Grounded(GroundedPosture::Crouched));
        skeleton.advance_posture_transition(2);
        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(BACKWARD_DIVE_POSTURE_TRANSITION_TICKS + 2);
        advance_posture_transition_facing(&mut transform, previous, skeleton.posture_transition());
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
        advance_posture_transition_facing(&mut transform, previous, skeleton.posture_transition());
        assert!(
            transform.rotation.abs_diff_eq(landing_rotation, 0.000_01),
            "supine-to-midpoint recovery must not begin the counter-yaw"
        );

        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(
            BACKWARD_DIVE_POSTURE_TRANSITION_TICKS + GROUND_POSTURE_TRANSITION_TICKS + 4,
        );
        advance_posture_transition_facing(&mut transform, previous, skeleton.posture_transition());
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

        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS / 2);
        advance_posture_transition_facing(&mut transform, previous, skeleton.posture_transition());
        assert!(transform.rotation.abs_diff_eq(initial, 0.000_01));

        let previous = skeleton.posture_transition();
        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS + 1);
        advance_posture_transition_facing(&mut transform, previous, skeleton.posture_transition());
        assert!(transform.rotation.abs_diff_eq(initial, 0.000_01));
    }

    #[test]
    fn toggle_roll_and_supine_release_use_authoritative_transition_sequence() {
        let mut skeleton = SkeletonState::default();
        let mut input = input::AccumulatedInput::default();
        let _ = apply_posture_action(PostureActionRequest::Toggle, &mut skeleton, &mut input);
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::UprightToProne
        );
        skeleton.advance_posture_transition(GROUND_POSTURE_TRANSITION_TICKS);
        assert_eq!(skeleton.body(), BodyState::Prone);

        let _ = apply_posture_action(PostureActionRequest::RollLeft, &mut skeleton, &mut input);
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

        let _ = apply_posture_action(PostureActionRequest::Toggle, &mut skeleton, &mut input);
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::SupineToUpright
        );
    }

    #[test]
    fn dive_preserves_requested_direction_and_starts_airborne_motion() {
        let mut skeleton = SkeletonState::default();
        let mut input = input::AccumulatedInput::default();
        let launched = apply_posture_action(
            PostureActionRequest::Dive {
                direction: DiveDirection::Backward,
            },
            &mut skeleton,
            &mut input,
        );
        assert_eq!(launched, Some(DiveDirection::Backward));
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Backward,
            }
        );
        assert!(input.jumped.is_some());
    }

    #[test]
    fn movement_intent_survives_missing_unreliable_packets_until_explicit_stop() {
        let mut world = World::new();
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::X)),
                SkeletonState::default(),
                AuthoritativePostureIntent::default(),
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
    fn prone_tank_input_is_body_relative_and_lateral_travel_is_three_eighths_speed() {
        let body = Quat::from_rotation_y(0.4);
        for camera_yaw in [0.0, 0.9, 2.7, -1.4] {
            let controller = Quat::from_rotation_y(camera_yaw);
            for (input, expected_body_local) in [
                (Vec2::Y, Vec3::Z),
                (-Vec2::Y, Vec3::NEG_Z),
                (Vec2::X, Vec3::NEG_X * TACTICAL_PRONE_LATERAL_SPEED_SCALE),
                (-Vec2::X, Vec3::X * TACTICAL_PRONE_LATERAL_SPEED_SCALE),
            ] {
                let resolved = prone_tank_controller_input(input, body, controller);
                let resolved_world =
                    controller_yaw(controller) * Vec3::new(resolved.x, 0.0, -resolved.y);
                let expected_world = controller_yaw(body) * expected_body_local;
                assert!(resolved_world.abs_diff_eq(expected_world, 0.0001));
            }
        }
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
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::Y)),
                skeleton,
                AuthoritativePostureIntent::default(),
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
    fn moving_attack_holds_captured_velocity_until_the_end_guard_commits() {
        let mut skeleton = SkeletonState::default().with_weapon_guard(WeaponGuardState::Raised);
        skeleton.begin_attack(
            AttackSpec::melee_from_local_velocity(Vec3::new(0.0, 0.0, -2.0)),
            0,
            10,
        );
        let mut world = World::new();
        let player = world
            .spawn((
                Player::default(),
                AuthoritativeMovementIntent(Some(Vec2::X)),
                skeleton,
                AuthoritativePostureIntent::default(),
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
            Some(Vec2::Y)
        );

        world
            .get_mut::<AuthoritativeMovementIntent>(player)
            .unwrap()
            .0 = None;
        world
            .get_mut::<SkeletonState>(player)
            .unwrap()
            .advance_action(10);
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .last_movement,
            Some(Vec2::Y)
        );

        world
            .get_mut::<AuthoritativeMovementIntent>(player)
            .unwrap()
            .0 = Some(Vec2::X);
        world
            .get_mut::<SkeletonState>(player)
            .unwrap()
            .advance_action(20);
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .last_movement,
            Some(Vec2::Y)
        );

        world
            .get_mut::<SkeletonState>(player)
            .unwrap()
            .advance_action(21);
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
        assert!(dive_horizontal_velocity(yaw, DiveDirection::Forward).x < -6.9);
        assert!(dive_horizontal_velocity(yaw, DiveDirection::Backward).x > 6.9);
        assert!(dive_horizontal_velocity(0.0, DiveDirection::Left).x < -6.9);
        assert!(dive_horizontal_velocity(0.0, DiveDirection::Right).x > 6.9);
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
                PostureTransitionKind::DiveToDowned { direction },
                0,
                10,
            ));
            let mut transform =
                Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI));

            skeleton.transition_body(BodyState::Airborne);
            skeleton.advance_posture_transition(1);
            skeleton.transition_body(BodyState::Grounded(GroundedPosture::Crouched));
            skeleton.advance_posture_transition(2);

            let previous = skeleton.posture_transition();
            skeleton.advance_posture_transition(7);
            advance_posture_transition_facing(
                &mut transform,
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
                previous,
                skeleton.posture_transition(),
            );
            let facing = transform.rotation * Vec3::Z;
            assert!(
                facing.abs_diff_eq(expected_world_heading, 0.000_01),
                "{direction:?}"
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
