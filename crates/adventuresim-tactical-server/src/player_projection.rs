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
    bot::{MissionEnemy, OffensiveCombatAi},
    combat::{MeleeAttackAuthority, RangedAttackAuthority, TacticalCombatSide},
    mission::MissionState,
    stdb::SpacetimeDb,
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

/// Durable inventory provenance retained only on the authoritative server.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct TacticalInventoryItemId(pub u64);

pub(crate) fn spawn_connected_players(
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
            OffensiveCombatAi::default(),
            TacticalCombatSide::Enemy,
        ))
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
        CharacterId(player.character.id),
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
        RangedAttackAuthority::default(),
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
                    melee: item.item.melee,
                    ranged: item.item.ranged,
                    blunt: item.item.blunt,
                    slash: item.item.slash,
                    pierce: item.item.pierce,
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

pub(crate) fn on_join_request(
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
    if !state.allows_party_join(join.character_id) {
        warn!(
            character_id = join.character_id.0,
            "Rejected unseen Party join after enrollment sealed"
        );
        return Ok(());
    }
    conn.reducers()
        .enter_mission(join.character_id.0, conn.identity())?;
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

pub(crate) fn on_player_input(
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
    if combat_state.is_incapacitated() {
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

fn player_collider() -> Collider {
    Collider::cylinder(0.4, 1.9)
}

fn player_spawn_offset(collider: &Collider) -> f32 {
    -collider.aabb(default(), Rotation::default()).min.y
}
