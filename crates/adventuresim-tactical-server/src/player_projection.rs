use std::{collections::BTreeMap, num::NonZeroU32};

use adventuresim_core::tactical_fixture::AnimationLabEnemyRole;
use adventuresim_stdb_client::*;
use adventuresim_tactical_core::animation::dive_launch_root_rotation;
use adventuresim_tactical_core::{inventory::ItemProperties, prelude::*};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{DisconnectReason, Disconnected},
    bevy_replicon::prelude::{FromClient, Replicated, SendTargets, ServerTriggerExt, ToClients},
    prelude::{
        JoinRequest, JumpCommand, PlayerInputRequest, PostureActionRequest, PostureCommand,
        ReconnectCapability, ReconnectToken, TacticalCombatConfigSnapshot,
    },
};
use bevy::prelude::*;
use bevy::time::Stopwatch;

use crate::{
    Args, SceneVistaBundleResource,
    bot::{CombatantBehaviorPackages, MissionEnemy},
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

#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct StartupInputObserved;

/// Latest complete movement request accepted from a player. Unlike Ahoy's
/// per-fixed-loop accumulator, this survives missing unreliable input packets
/// until an explicit request replaces it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct AuthoritativeMovementIntent(pub(crate) Option<Vec2>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AuthoritativePostureIntent {
    facing: CameraFacingIntent,
    last_jump_sequence: u32,
    last_command_sequence: u32,
    quickstep_launch_tick: Option<u64>,
    quickstep_landing_braking: bool,
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
                    prefers_stab: item.item.prefers_stab,
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
            &mut Transform,
            &mut Rotation,
            Has<StartupInputObserved>,
        ),
        With<Player>,
    >,
    combat_config: Res<TacticalCombatConfig>,
) {
    let Some(validated) = validate_player_input(
        input.look,
        input.movement,
        input.jump,
        input.jump_charge,
        input.downed_align,
        input.posture,
        input.pace,
        input.weapon_guard,
        input.melee_preparation,
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
        mut transform,
        mut physics_rotation,
        startup_input_observed,
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
        posture_intent.facing = CameraFacingIntent::Free;
        posture_intent.quickstep_launch_tick = None;
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
            && let Some(direction) = apply_posture_action(
                action,
                &mut skeleton,
                &mut accumulated_input,
                &combat_config,
            )
        {
            // The authored direction and physical launch are both relative to
            // this accepted camera frame. Commit the root before transition
            // facing locks, rather than retaining a stale pre-aim heading.
            let launch_rotation = dive_launch_root_rotation(Quat::from_rotation_y(look.yaw));
            transform.rotation = launch_rotation;
            physics_rotation.0 = launch_rotation;
            let horizontal = dive_horizontal_velocity(
                look.yaw,
                direction,
                combat_config.movement.speeds_metres_per_second.dive,
            );
            velocity.x = horizontal.x;
            velocity.z = horizontal.z;
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
    if jump_requested
        && !skeleton.is_posture_transitioning()
        && matches!(skeleton.body(), BodyState::Grounded(_))
    {
        let launch = match validated.jump.quickstep {
            Some(direction)
                if validated.weapon_guard == WeaponGuardState::Raised
                    && skeleton.body() == BodyState::Grounded(GroundedPosture::Upright) =>
            {
                begin_authoritative_quickstep(
                    &mut skeleton,
                    &mut posture_intent,
                    direction,
                    &combat_config,
                );
                false
            }
            Some(_) => false,
            None => true,
        };
        if launch {
            accumulated_input.jumped = Some(Stopwatch::new());
        }
    }
}

pub(crate) fn begin_authoritative_quickstep(
    skeleton: &mut SkeletonState,
    posture_intent: &mut AuthoritativePostureIntent,
    direction: Vec2,
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
            start + combat_seconds_to_ticks(config.movement.maneuvers.quickstep_contact_seconds),
        )
        .is_err()
    {
        return false;
    }
    posture_intent.quickstep_launch_tick = Some(
        start + combat_seconds_to_ticks(config.movement.maneuvers.quickstep_preparation_seconds),
    );
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

fn validate_player_input(
    look: Vec2,
    movement: Option<Vec2>,
    jump: JumpCommand,
    jump_charge: bool,
    downed_align: bool,
    posture: PostureCommand,
    pace: MovementPace,
    weapon_guard: WeaponGuardState,
    melee_preparation: MeleePreparationInput,
) -> Option<ValidatedPlayerInput> {
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
    config: &TacticalCombatConfig,
) -> Option<DiveDirection> {
    if action == PostureActionRequest::Toggle && skeleton.body().is_downed() {
        begin_get_up_transition_configured(skeleton, config);
        return None;
    }
    let tick = skeleton.locomotion_sample_tick;
    let mut dive_travel_direction = None;
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
            dive_travel_direction = Some(travel_direction);
            matches!(skeleton.body(), BodyState::Grounded(_)).then_some(
                PostureTransitionKind::DiveToDowned {
                    direction: animation_direction,
                },
            )
        }
    };
    let transition = transition?;
    let duration = match transition {
        PostureTransitionKind::DiveToDowned {
            direction: DiveDirection::Backward,
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
        accumulated_input.jumped = Some(Stopwatch::new());
        return dive_travel_direction;
    }
    None
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
    combat_config: Res<TacticalCombatConfig>,
) {
    for (movement_intent, skeleton, posture, controller, transform, mut accumulated_input) in
        &mut players
    {
        accumulated_input.last_movement = movement_intent.0;
        if skeleton.action_kind() == SkeletonAction::Dodge
            && skeleton.action_direction() != Vec2::ZERO
            && skeleton.quickstep_is_launched()
            && skeleton.body() == BodyState::Airborne
        {
            accumulated_input.last_movement = Some(skeleton.action_direction());
        } else if skeleton.action_kind() == SkeletonAction::Dodge
            && skeleton.action_direction() != Vec2::ZERO
        {
            accumulated_input.last_movement = None;
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

pub(crate) fn launch_pending_quicksteps(
    mut players: Query<
        (
            &SkeletonState,
            &mut AuthoritativePostureIntent,
            &mut AccumulatedInput,
            &CharacterControllerState,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
    combat_config: Res<TacticalCombatConfig>,
) {
    for (skeleton, mut posture, mut input, controller, mut velocity) in &mut players {
        let Some(launch_tick) = posture.quickstep_launch_tick else {
            continue;
        };
        if skeleton.action_kind() != SkeletonAction::Dodge {
            posture.quickstep_launch_tick = None;
        } else if skeleton.locomotion_sample_tick >= launch_tick {
            let direction = skeleton.action_direction();
            let world_direction =
                controller_yaw(controller.orientation) * Vec3::new(direction.x, 0.0, -direction.y);
            let speed = combat_config.movement.speeds_metres_per_second.quickstep;
            velocity.x = world_direction.x * speed;
            velocity.z = world_direction.z * speed;
            input.jumped = Some(Stopwatch::new());
            posture.quickstep_launch_tick = None;
            posture.quickstep_landing_braking = false;
        }
    }
}

/// Residual quickstep momentum outlives the visual dodge action. Apply drag
/// over several grounded ticks while ordinary raised guard presentation has
/// already resumed, rather than snapping velocity to zero at contact.
pub(crate) fn brake_quickstep_landing(
    time: Res<Time<Fixed>>,
    combat_config: Res<TacticalCombatConfig>,
    mut players: Query<
        (
            &SkeletonState,
            &CharacterControllerState,
            &mut AuthoritativePostureIntent,
            &mut LinearVelocity,
        ),
        With<Player>,
    >,
) {
    for (skeleton, controller, mut posture, mut velocity) in &mut players {
        brake_quickstep_horizontal_velocity(
            skeleton,
            controller.grounded.is_some(),
            time.delta_secs(),
            &mut posture,
            &mut velocity,
            combat_config
                .movement
                .quickstep_landing_brake_metres_per_second_squared,
        );
    }
}

fn brake_quickstep_horizontal_velocity(
    skeleton: &SkeletonState,
    grounded: bool,
    delta_seconds: f32,
    posture: &mut AuthoritativePostureIntent,
    velocity: &mut LinearVelocity,
    brake_metres_per_second_squared: f32,
) {
    if skeleton.action_kind() == SkeletonAction::Dodge
        && skeleton.action_direction() != Vec2::ZERO
        && skeleton.body() == BodyState::Airborne
        && grounded
    {
        posture.quickstep_landing_braking = true;
    }
    if !grounded {
        posture.quickstep_landing_braking = false;
        return;
    }
    if !posture.quickstep_landing_braking {
        return;
    }

    let horizontal = velocity.xz();
    let speed = horizontal.length();
    let next_speed = (speed - brake_metres_per_second_squared * delta_seconds.max(0.0)).max(0.0);
    if speed <= f32::EPSILON || next_speed <= f32::EPSILON {
        velocity.x = 0.0;
        velocity.z = 0.0;
        posture.quickstep_landing_braking = false;
    } else {
        let scale = next_speed / speed;
        velocity.x *= scale;
        velocity.z *= scale;
    }
}

/// Projects authoritative controller motion into the compact presentation
/// state replicated to every client. It deliberately never evaluates bones.
pub(crate) fn update_skeleton_locomotion(
    time: Res<Time<Fixed>>,
    combat_config: Res<TacticalCombatConfig>,
    mut players: Query<
        (
            &CharacterControllerState,
            &LinearVelocity,
            &mut SkeletonState,
            &mut Transform,
            &mut Rotation,
            &TacticalCombatState,
            &MovementPace,
            &AuthoritativePostureIntent,
        ),
        With<Player>,
    >,
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
            transform.rotation = advance_body_facing_with_speed(
                transform.rotation,
                controller.orientation,
                velocity.0,
                skeleton.action_kind(),
                skeleton.weapon_guard(),
                time.delta_secs(),
                std::f32::consts::PI / combat_config.presentation.body_turn_seconds_per_half_turn,
            );
        }
        project_skeleton_locomotion(
            &mut skeleton,
            SkeletonLocomotionInput {
                orientation: controller.orientation,
                linear_velocity: velocity.0,
                grounded: controller.grounded.is_some(),
                delta_seconds: time.delta_secs(),
                tick,
            },
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

/// `aeronet_io`'s own `ConnectionPlugin` (part of `AdventureSimulatorNetPlugins`)
/// registers an observer on this exact same `Disconnected` trigger that
/// unconditionally despawns `entity` right afterward - see its doc comment:
/// "Immediately after this, the session will be despawned". Bevy documents
/// same-event observer ordering as unspecified, and per-observer commands
/// from a single trigger dispatch are all applied together afterward in
/// enqueue order - so if that despawn command happened to apply before any
/// command queued here, every one of them would panic trying to touch an
/// already-despawned entity (confirmed live: this is exactly what an abrupt
/// disconnect used to do before this got split onto a fresh entity).
/// `main()` registers this observer before `AdventureSimulatorNetPlugins` is
/// added specifically so its commands enqueue - and therefore apply - first.
/// Moving components onto a brand-new entity here (mirroring the exact
/// component lists [`on_join_request`]'s reconnect branch already expects to
/// move back off of), rather than trying to keep `entity` itself alive,
/// means the grace-period state no longer depends on outliving `aeronet_io`'s
/// despawn at all - only on running before it, which the registration order
/// above guarantees.
pub(crate) fn on_client_disconnected(
    disconnected: On<Disconnected>,
    query: Query<&ReconnectSession>,
    inventory_items: Query<(Entity, &ItemOf)>,
    mut commands: Commands,
) -> Result {
    let entity = disconnected.event_target();
    let Ok(session) = query.get(entity) else {
        return Ok(());
    };
    let orphan = commands.spawn_empty().id();
    commands.entity(entity).move_components::<(
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
    )>(orphan);
    commands.entity(entity).move_components::<(
        Transform,
        CharacterLook,
        AuthoritativeMovementIntent,
        AuthoritativePostureIntent,
        MovementPace,
        LinearVelocity,
        SkeletonState,
        MeleeAttackAuthority,
        RangedAttackAuthority,
    )>(orphan);
    commands.entity(entity).move_components::<(
        Collider,
        CollisionMargin,
        CharacterController,
        AccumulatedInput,
    )>(orphan);
    commands
        .entity(entity)
        .move_components::<InventoryItems>(orphan);
    commands
        .entity(entity)
        .move_components::<LoadingPlayer>(orphan);
    for (item, owner) in &inventory_items {
        if owner.0 == entity {
            commands.entity(item).insert(ItemOf(orphan));
        }
    }
    commands.entity(orphan).insert(DisconnectedPlayer {
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
    // during landing. Supine get-up applies an inverse half-turn that cancels the
    // authored pose's implicit convention change in world space. Prone get-up
    // receives neither correction.
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
                // insert just above) backfills most "always fresh, never
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
        assert!(bot_entity.contains::<OffensiveCombatAi>());
        assert_eq!(bot_entity.get::<CharacterId>().unwrap().0, 99);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthoritativeMovementIntent, AuthoritativePostureIntent,
        BACKWARD_DIVE_POSTURE_TRANSITION_TICKS, CameraFacingIntent, DisconnectedPlayer,
        GROUND_POSTURE_TRANSITION_TICKS, Player, RECONNECT_GRACE_SECS,
        ROLL_POSTURE_TRANSITION_TICKS, WeaponGuardState, advance_downed_facing_for_camera,
        advance_posture_transition_facing, apply_posture_action, authoritative_weapon_guard,
        brake_quickstep_horizontal_velocity, dive_horizontal_velocity,
        downed_tank_controller_input, input, launch_pending_quicksteps, mission_enemy_health_scale,
        mission_enemy_scale, posture_transition_locks_body_facing, queue_replication_rebind,
        reconnect_matches, restore_authoritative_movement_intent, sequence_is_newer,
        tactical_movement_speed_for_guard, try_claim_reconnect, validate_player_input,
    };
    use adventuresim_tactical_core::physics::TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND;
    use adventuresim_tactical_core::prelude::{
        BodyState, CharacterControllerState, CharacterId, DiveDirection, DodgeSpec,
        GroundedPosture, LinearVelocity, MeleePreparationInput, MovementPace,
        PostureTransitionKind, RollDirection, Rotation, SkeletonAction, SkeletonState,
        TACTICAL_PRONE_LATERAL_SPEED_SCALE, TacticalCombatConfig, advance_body_facing,
        controller_yaw, downed_camera_roll_target,
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
                JumpCommand {
                    sequence: 1,
                    ..default()
                },
                false,
                false,
                PostureCommand::default(),
                MovementPace::Sprint,
                WeaponGuardState::Raised,
                MeleePreparationInput::Preferred,
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
            JumpCommand {
                sequence: 7,
                ..default()
            },
            true,
            true,
            PostureCommand::default(),
            MovementPace::Sprint,
            WeaponGuardState::Raised,
            MeleePreparationInput::Preferred,
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
    fn player_input_normalizes_quickstep_direction_and_rejects_non_finite_values() {
        let validated = validate_player_input(
            Vec2::ZERO,
            Some(Vec2::Y),
            JumpCommand {
                sequence: 3,
                quickstep: Some(Vec2::new(4.0, -3.0)),
            },
            false,
            false,
            PostureCommand::default(),
            MovementPace::Walk,
            WeaponGuardState::Raised,
            MeleePreparationInput::Preferred,
        )
        .unwrap();
        assert_eq!(validated.jump.quickstep, Some(Vec2::new(0.8, -0.6)));
        assert!(
            validate_player_input(
                Vec2::ZERO,
                None,
                JumpCommand {
                    sequence: 4,
                    quickstep: Some(Vec2::new(f32::NAN, 0.0)),
                },
                false,
                false,
                PostureCommand::default(),
                MovementPace::Walk,
                WeaponGuardState::Raised,
                MeleePreparationInput::Preferred,
            )
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
            &config,
        );
        assert_eq!(launched, Some(DiveDirection::Backward));
        assert_eq!(
            skeleton.posture_transition().unwrap().kind(),
            PostureTransitionKind::DiveToDowned {
                direction: DiveDirection::Forward,
            }
        );
        assert!(input.jumped.is_some());
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
    fn pending_quickstep_launches_only_after_the_procedural_load() {
        let mut skeleton = SkeletonState::default();
        skeleton
            .begin_dodge(DodgeSpec::quickstep(Vec2::Y).unwrap(), 0, 20)
            .unwrap();
        skeleton.advance_action(4);
        skeleton.locomotion_sample_tick = 4;
        let mut world = World::new();
        world.insert_resource(TacticalCombatConfig::default());
        let player = world
            .spawn((
                Player::default(),
                skeleton,
                AuthoritativePostureIntent {
                    quickstep_launch_tick: Some(5),
                    ..default()
                },
                input::AccumulatedInput::default(),
                CharacterControllerState::default(),
                LinearVelocity::default(),
            ))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(launch_pending_quicksteps);
        schedule.run(&mut world);
        assert!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .jumped
                .is_none()
        );

        {
            let mut skeleton = world.get_mut::<SkeletonState>(player).unwrap();
            skeleton.advance_action(5);
            skeleton.locomotion_sample_tick = 5;
        }
        schedule.run(&mut world);
        assert!(
            world
                .get::<input::AccumulatedInput>(player)
                .unwrap()
                .jumped
                .is_some()
        );
        assert_eq!(
            world.get::<LinearVelocity>(player).unwrap().xz(),
            Vec2::new(0.0, -TACTICAL_QUICKSTEP_SPEED_METRES_PER_SECOND)
        );
        assert_eq!(
            world
                .get::<AuthoritativePostureIntent>(player)
                .unwrap()
                .quickstep_launch_tick,
            None
        );
    }

    #[test]
    fn quickstep_brakes_horizontal_velocity_over_multiple_grounded_ticks() {
        let mut skeleton = SkeletonState::default();
        skeleton
            .begin_dodge(DodgeSpec::quickstep(Vec2::X).unwrap(), 0, 20)
            .unwrap();
        skeleton.advance_action(10);
        skeleton.transition_body(BodyState::Airborne);
        let mut velocity = LinearVelocity(Vec3::new(5.0, -1.0, 0.0));
        let mut posture = AuthoritativePostureIntent::default();
        let initial_horizontal_speed = velocity.xz().length();
        brake_quickstep_horizontal_velocity(
            &skeleton,
            true,
            1.0 / 64.0,
            &mut posture,
            &mut velocity,
            20.0,
        );

        assert!(velocity.xz().length() < initial_horizontal_speed);
        assert!(velocity.xz().length() > 0.0);
        assert_eq!(velocity.y, -1.0);
        assert!(posture.quickstep_landing_braking);

        for _ in 0..7 {
            brake_quickstep_horizontal_velocity(
                &skeleton,
                true,
                1.0 / 64.0,
                &mut posture,
                &mut velocity,
                20.0,
            );
        }
        assert!(velocity.xz().length() > 0.0);

        for _ in 0..8 {
            brake_quickstep_horizontal_velocity(
                &skeleton,
                true,
                1.0 / 64.0,
                &mut posture,
                &mut velocity,
                20.0,
            );
        }
        assert_eq!(velocity.xz(), Vec2::ZERO);
        assert!(!posture.quickstep_landing_braking);
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
