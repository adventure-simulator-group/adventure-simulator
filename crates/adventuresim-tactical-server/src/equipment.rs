//! Authoritative, transient tactical equipment mutations.

use adventuresim_core::item_catalog;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient,
    prelude::{EquipmentAction, EquipmentActionRequest, EquipmentHand},
};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

const PICKUP_RANGE_M: f32 = 2.0;

pub(crate) struct TacticalEquipmentPlugin;

impl Plugin for TacticalEquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingEquipmentActions>()
            .init_resource::<LastEquipmentSequence>()
            .add_observer(queue_equipment_action)
            .add_systems(Update, process_equipment_actions);
    }
}

#[derive(Resource, Default)]
pub(crate) struct PendingEquipmentActions(VecDeque<(Entity, EquipmentActionRequest)>);

#[derive(Resource, Default)]
pub(crate) struct LastEquipmentSequence(HashMap<Entity, u32>);

pub(crate) fn reconnect_equipment_lifecycle(
    old: Entity,
    new: Entity,
    pending: &mut PendingEquipmentActions,
    sequences: &mut LastEquipmentSequence,
) {
    pending
        .0
        .retain(|(actor, _)| *actor != old && *actor != new);
    sequences.0.remove(&new);
    if let Some(sequence) = sequences.0.remove(&old) {
        sequences.0.insert(new, sequence);
    }
}

pub(crate) fn purge_equipment_lifecycle(
    actor: Entity,
    pending: &mut PendingEquipmentActions,
    sequences: &mut LastEquipmentSequence,
) {
    pending.0.retain(|(queued, _)| *queued != actor);
    sequences.0.remove(&actor);
}

const MAX_PENDING_PER_ACTOR: usize = 4;

type ItemView<'a> = (
    Entity,
    &'a ItemProperties,
    Option<&'a ItemOf>,
    &'a EquipmentTopology,
    Option<&'a EquipSlot>,
    Option<&'a TacticalEquipmentPhysical>,
    Has<TacticalSceneItem>,
    Option<&'a Transform>,
);

fn queue_equipment_action(
    request: On<FromClient<EquipmentActionRequest>>,
    mut pending: ResMut<PendingEquipmentActions>,
) {
    let Some(controlled) = request.client_id.entity() else {
        warn!(
            client_id = ?request.client_id,
            actor = ?request.actor,
            sequence = request.sequence,
            "Rejected tactical equipment action from a client without a controlled entity"
        );
        return;
    };
    if request.actor != controlled {
        warn!(
            ?controlled,
            requested_actor = ?request.actor,
            sequence = request.sequence,
            action = ?request.action,
            "Rejected tactical equipment action for an uncontrolled actor"
        );
        return;
    }
    if !can_enqueue(&pending.0, controlled) {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            action = ?request.action,
            max_pending = MAX_PENDING_PER_ACTOR,
            "Rejected tactical equipment action because the actor queue is full"
        );
        return;
    }
    pending.0.push_back((controlled, **request));
}

fn can_enqueue(queue: &VecDeque<(Entity, EquipmentActionRequest)>, actor: Entity) -> bool {
    queue.iter().filter(|(queued, _)| *queued == actor).count() < MAX_PENDING_PER_ACTOR
}

fn process_equipment_actions(
    mut commands: Commands,
    mut pending: ResMut<PendingEquipmentActions>,
    mut sequences: ResMut<LastEquipmentSequence>,
    players: Query<(&Transform, &CharacterLook), With<Player>>,
    mut action_states: Query<&mut EquipmentActionState>,
    items: Query<ItemView<'_>>,
    spatial: SpatialQuery,
) {
    // Exactly one action is validated and committed per frame. Deferred ECS
    // mutations are therefore applied before another queued action can read
    // the actor's topology.
    let Some((controlled, request)) = pending.0.pop_front() else {
        return;
    };
    if request.actor != controlled {
        warn!(
            ?controlled,
            requested_actor = ?request.actor,
            sequence = request.sequence,
            action = ?request.action,
            "Rejected queued tactical equipment action for an uncontrolled actor"
        );
        return;
    }
    if players.get(controlled).is_err() {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            action = ?request.action,
            "Rejected tactical equipment action because the controlled player is unavailable"
        );
        return;
    }
    let Ok(mut state) = action_states.get_mut(controlled) else {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            action = ?request.action,
            "Rejected tactical equipment action because authoritative action state is unavailable"
        );
        return;
    };
    let last = sequences.0.get(&controlled).copied().unwrap_or(0);
    if !sequence_is_newer(request.sequence, last) {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            last_sequence = last,
            action = ?request.action,
            "Rejected stale or replayed tactical equipment action"
        );
        return;
    }
    if request.expected_revision != state.revision {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            expected_revision = request.expected_revision,
            authoritative_revision = state.revision,
            action = ?request.action,
            "Rejected tactical equipment action with a stale revision"
        );
        return;
    }
    let authoritative_hand_item = hand_item(controlled, request.hand, &items);
    if authoritative_hand_item != request.expected_hand_item {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            hand = ?request.hand,
            expected_hand_item = ?request.expected_hand_item,
            ?authoritative_hand_item,
            action = ?request.action,
            "Rejected tactical equipment action because the held item changed"
        );
        return;
    }

    let accepted = match request.action {
        EquipmentAction::Slot {
            location,
            depth,
            expected_destination,
        } => {
            let authoritative_destination = ordered_at_location(controlled, location, &items)
                .get(depth as usize)
                .map(ReachableTarget::expected_entity);
            if authoritative_destination != expected_destination {
                warn!(
                    actor = ?controlled,
                    sequence = request.sequence,
                    ?location,
                    depth,
                    ?expected_destination,
                    ?authoritative_destination,
                    "Rejected tactical equipment slot action because the destination changed"
                );
                false
            } else {
                transfer_slot(
                    &mut commands,
                    controlled,
                    request.hand,
                    location,
                    depth,
                    &items,
                )
            }
        }
        EquipmentAction::Hand {
            destination,
            expected_destination,
        } => {
            if hand_item(controlled, destination, &items) != expected_destination {
                false
            } else {
                transfer_hand(&mut commands, controlled, request.hand, destination, &items)
            }
        }
        EquipmentAction::Drop => drop_hand(
            &mut commands,
            controlled,
            request.hand,
            &players,
            &items,
            &spatial,
        ),
        EquipmentAction::Pickup { item } => pickup(
            &mut commands,
            controlled,
            request.hand,
            item,
            &players,
            &items,
            &spatial,
        ),
    };
    if accepted {
        sequences.0.insert(controlled, request.sequence);
        state.revision = state.revision.wrapping_add(1);
        info!(
            actor = ?controlled,
            sequence = request.sequence,
            revision = state.revision,
            action = ?request.action,
            "Committed tactical equipment action"
        );
    } else {
        warn!(
            actor = ?controlled,
            sequence = request.sequence,
            action = ?request.action,
            "Rejected tactical equipment action during authoritative transfer validation"
        );
    }
}

fn sequence_is_newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance <= u32::MAX / 2
}

fn hand_item(actor: Entity, hand: EquipmentHand, items: &Query<ItemView<'_>>) -> Option<Entity> {
    items
        .iter()
        .find_map(|(entity, _, owner, _, slot, _, scene, _)| {
            (!scene
                && owner.is_some_and(|owner| owner.0 == actor)
                && slot.is_some_and(|slot| *slot == hand.slot()))
            .then_some(entity)
        })
}

fn ordered_at_location(
    actor: Entity,
    location: EquipmentLocation,
    items: &Query<ItemView<'_>>,
) -> Vec<ReachableTarget> {
    let mut found: Vec<_> = items
        .iter()
        .filter(|(_, _, owner, _, _, _, scene, _)| {
            !scene && owner.is_some_and(|owner| owner.0 == actor)
        })
        .filter_map(|(entity, _, _, topology, _, _, _, _)| {
            topology
                .occupancies
                .iter()
                .filter_map(|occupancy| match occupancy.anchor {
                    TacticalEquipmentAnchor::CharacterLocation(found) if found == location => {
                        Some((occupancy.channel.order(), occupancy.order, entity.to_bits()))
                    }
                    _ => None,
                })
                .max()
                .map(|key| (key, entity))
        })
        .collect();
    found.sort_by_key(|left| std::cmp::Reverse(left.0));
    let mut reachable = Vec::new();
    let mut visited = HashSet::new();
    for (_, entity) in found {
        append_reachable(entity, location, items, &mut visited, &mut reachable);
    }
    reachable
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ReachableTarget {
    Occupied(Entity),
    EmptyAttachment {
        parent: Entity,
        attachment_point_id: String,
        channel: EquipmentChannel,
        capacity_index: u16,
    },
}

impl ReachableTarget {
    fn expected_entity(&self) -> Entity {
        match self {
            Self::Occupied(entity) => *entity,
            Self::EmptyAttachment { parent, .. } => *parent,
        }
    }
}

fn append_reachable(
    entity: Entity,
    location: EquipmentLocation,
    items: &Query<ItemView<'_>>,
    visited: &mut HashSet<Entity>,
    output: &mut Vec<ReachableTarget>,
) {
    if !visited.insert(entity) {
        return;
    }
    let Ok((_, properties, _, _, _, _, _, _)) = items.get(entity) else {
        output.push(ReachableTarget::Occupied(entity));
        return;
    };
    let Some(equipment) = item_catalog::definition(&properties.id)
        .and_then(|definition| definition.equipment.as_ref())
    else {
        output.push(ReachableTarget::Occupied(entity));
        return;
    };
    let mut points: Vec<_> = equipment.attachment_points.iter().collect();
    points.sort_by_key(|point| (point.order, point.id.as_str()));
    for point in points
        .into_iter()
        .filter(|point| point.locations.is_empty() || point.locations.contains(&location))
    {
        for capacity_index in 0..point.capacity {
            let child = items
                .iter()
                .find_map(|(child, _, _, topology, _, _, _, _)| {
                    topology.occupancies.iter().any(|occupancy| {
                    matches!(
                        &occupancy.anchor,
                        TacticalEquipmentAnchor::ItemAttachment { parent, attachment_point_id }
                            if *parent == entity
                                && attachment_point_id == &point.id
                                && occupancy.capacity_index == capacity_index
                    )
                }).then_some(child)
                });
            if let Some(child) = child {
                append_reachable(child, location, items, visited, output);
            } else {
                output.push(ReachableTarget::EmptyAttachment {
                    parent: entity,
                    attachment_point_id: point.id.clone(),
                    channel: point.channel,
                    capacity_index,
                });
            }
        }
    }
    output.push(ReachableTarget::Occupied(entity));
}

fn has_children(entity: Entity, items: &Query<ItemView<'_>>) -> bool {
    items.iter().any(|(_, _, _, topology, _, _, _, _)| {
        topology.occupancies.iter().any(|occupancy| {
            matches!(
                occupancy.anchor,
                TacticalEquipmentAnchor::ItemAttachment { parent, .. } if parent == entity
            )
        })
    })
}

fn hand_topology(hand: EquipmentHand) -> EquipmentTopology {
    EquipmentTopology {
        placement_id: Some(
            match hand {
                EquipmentHand::Left => "left_hand",
                EquipmentHand::Right => "right_hand",
            }
            .into(),
        ),
        occupancies: vec![EquipmentTopologyOccupancy {
            occupancy_id: format!("tactical:{}:held", hand.id()),
            anchor: TacticalEquipmentAnchor::CharacterLocation(hand.location()),
            channel: EquipmentChannel::Held,
            order: 0,
            requirement_index: 0,
            capacity_index: 0,
        }],
    }
}

fn placement_topology(item_id: &str, location: EquipmentLocation) -> Option<EquipmentTopology> {
    let definition = item_catalog::definition(item_id)?;
    let equipment = definition.equipment.as_ref()?;
    let placement = equipment.placements.iter().find(|placement| {
        placement.parents.is_empty()
            && placement
                .occupancy
                .iter()
                .any(|requirement| requirement.location == location)
    })?;
    if !root_placement_allowed(item_id, placement) {
        return None;
    }
    Some(EquipmentTopology {
        placement_id: Some(placement.id.clone()),
        occupancies: placement
            .occupancy
            .iter()
            .enumerate()
            .map(|(index, requirement)| EquipmentTopologyOccupancy {
                occupancy_id: format!("tactical:{item_id}:{}:{index}", placement.id),
                anchor: TacticalEquipmentAnchor::CharacterLocation(requirement.location),
                channel: requirement.channel,
                order: requirement.order,
                requirement_index: index as u16,
                capacity_index: 0,
            })
            .collect(),
    })
}

fn root_placement_allowed(item_id: &str, placement: &item_catalog::EquipmentPlacement) -> bool {
    if item_catalog::weapon_carry(item_id) != Some(item_catalog::WeaponCarry::HandOnly) {
        return true;
    }
    placement.parents.is_empty()
        && placement.occupancy.len() == 1
        && matches!(
            placement.occupancy[0].location,
            EquipmentLocation::LeftHand | EquipmentLocation::RightHand
        )
        && placement.occupancy[0].channel == EquipmentChannel::Held
        && placement.occupancy[0].order == 0
}

#[derive(Clone)]
struct AttachmentTarget {
    parent: Entity,
    attachment_point_id: String,
    channel: EquipmentChannel,
    capacity_index: u16,
}

fn attachment_target_accepts(
    moving_tags: &[String],
    target: &AttachmentTarget,
    items: &Query<ItemView<'_>>,
) -> bool {
    let Ok((_, parent_properties, _, _, _, _, _, _)) = items.get(target.parent) else {
        return false;
    };
    item_catalog::definition(&parent_properties.id)
        .and_then(|definition| definition.equipment.as_ref())
        .and_then(|equipment| {
            equipment
                .attachment_points
                .iter()
                .find(|point| point.id == target.attachment_point_id)
        })
        .is_some_and(|point| {
            point.channel == target.channel
                && (point.accepts_tags.is_empty()
                    || point
                        .accepts_tags
                        .iter()
                        .any(|accepted| moving_tags.contains(accepted)))
        })
}

fn attachment_topology(
    item_id: &str,
    selected: &ReachableTarget,
    actor: Entity,
    items: &Query<ItemView<'_>>,
) -> Option<EquipmentTopology> {
    if !parent_placement_allowed(item_id) {
        return None;
    }
    let moving = item_catalog::definition(item_id)?.equipment.as_ref()?;
    let mut available = Vec::<AttachmentTarget>::new();
    let mut explicitly_selected = Vec::<(Entity, String, u16)>::new();
    match selected {
        ReachableTarget::EmptyAttachment {
            parent,
            attachment_point_id,
            channel,
            capacity_index,
        } => {
            explicitly_selected.push((*parent, attachment_point_id.clone(), *capacity_index));
            available.push(AttachmentTarget {
                parent: *parent,
                attachment_point_id: attachment_point_id.clone(),
                channel: *channel,
                capacity_index: *capacity_index,
            });
        }
        ReachableTarget::Occupied(entity) => {
            let (_, _, _, topology, _, _, _, _) = items.get(*entity).ok()?;
            available.extend(topology.occupancies.iter().filter_map(|occupancy| {
                match &occupancy.anchor {
                    TacticalEquipmentAnchor::ItemAttachment {
                        parent,
                        attachment_point_id,
                    } => {
                        explicitly_selected.push((
                            *parent,
                            attachment_point_id.clone(),
                            occupancy.capacity_index,
                        ));
                        Some(AttachmentTarget {
                            parent: *parent,
                            attachment_point_id: attachment_point_id.clone(),
                            channel: occupancy.channel,
                            capacity_index: occupancy.capacity_index,
                        })
                    }
                    _ => None,
                }
            }));
        }
    }
    // Additional empty points are selected deterministically for multi-parent
    // placements. Existing occupied capacities are never silently displaced.
    for (parent, parent_properties, owner, _, _, _, scene, _) in items.iter() {
        if scene || owner.is_none_or(|owner| owner.0 != actor) {
            continue;
        }
        let Some(parent_equipment) = item_catalog::definition(&parent_properties.id)
            .and_then(|definition| definition.equipment.as_ref())
        else {
            continue;
        };
        for point in &parent_equipment.attachment_points {
            for capacity_index in 0..point.capacity {
                let occupied = items.iter().any(|(_, _, _, topology, _, _, _, _)| {
                    topology.occupancies.iter().any(|occupancy| {
                        matches!(
                            &occupancy.anchor,
                            TacticalEquipmentAnchor::ItemAttachment { parent: found, attachment_point_id }
                                if *found == parent
                                    && attachment_point_id == &point.id
                                    && occupancy.capacity_index == capacity_index
                        )
                    })
                });
                if !occupied
                    && !available.iter().any(|target| {
                        target.parent == parent
                            && target.attachment_point_id == point.id
                            && target.capacity_index == capacity_index
                    })
                {
                    available.push(AttachmentTarget {
                        parent,
                        attachment_point_id: point.id.clone(),
                        channel: point.channel,
                        capacity_index,
                    });
                }
            }
        }
    }
    available.sort_by(|left, right| {
        let left_selected = explicitly_selected.iter().any(|selected| {
            selected
                == &(
                    left.parent,
                    left.attachment_point_id.clone(),
                    left.capacity_index,
                )
        });
        let right_selected = explicitly_selected.iter().any(|selected| {
            selected
                == &(
                    right.parent,
                    right.attachment_point_id.clone(),
                    right.capacity_index,
                )
        });
        right_selected.cmp(&left_selected).then(
            left.parent
                .to_bits()
                .cmp(&right.parent.to_bits())
                .then(left.attachment_point_id.cmp(&right.attachment_point_id))
                .then(left.capacity_index.cmp(&right.capacity_index)),
        )
    });
    for placement in &moving.placements {
        if placement.occupancy.is_empty() && !placement.parents.is_empty() {
            let mut chosen = Vec::new();
            for requirement in &placement.parents {
                let Some(target) = available.iter().find(|target| {
                    target.channel == requirement.channel
                        && attachment_target_accepts(&moving.attachment_tags, target, items)
                        && !chosen.iter().any(|chosen: &&AttachmentTarget| {
                            chosen.parent == target.parent
                                && chosen.attachment_point_id == target.attachment_point_id
                                && chosen.capacity_index == target.capacity_index
                        })
                }) else {
                    break;
                };
                chosen.push(target);
            }
            if chosen.len() != placement.parents.len() {
                continue;
            }
            if !explicitly_selected.iter().all(|selected| {
                chosen.iter().any(|target| {
                    selected
                        == &(
                            target.parent,
                            target.attachment_point_id.clone(),
                            target.capacity_index,
                        )
                })
            }) {
                continue;
            }
            return Some(EquipmentTopology {
                placement_id: Some(placement.id.clone()),
                occupancies: chosen
                    .into_iter()
                    .enumerate()
                    .map(|(index, target)| EquipmentTopologyOccupancy {
                        occupancy_id: format!(
                            "tactical:{}:{}:{}",
                            target.parent.to_bits(),
                            target.attachment_point_id,
                            target.capacity_index
                        ),
                        anchor: TacticalEquipmentAnchor::ItemAttachment {
                            parent: target.parent,
                            attachment_point_id: target.attachment_point_id.clone(),
                        },
                        channel: target.channel,
                        order: placement.parents[index].order,
                        requirement_index: index as u16,
                        capacity_index: target.capacity_index,
                    })
                    .collect(),
            });
        }
    }
    None
}

fn parent_placement_allowed(item_id: &str) -> bool {
    item_catalog::weapon_carry(item_id) != Some(item_catalog::WeaponCarry::HandOnly)
}

fn topology_conflicts(
    actor: Entity,
    proposed: &EquipmentTopology,
    ignored: &[Entity],
    items: &Query<ItemView<'_>>,
) -> bool {
    items
        .iter()
        .any(|(entity, _, owner, topology, _, _, scene, _)| {
            !scene
                && !ignored.contains(&entity)
                && owner.is_some_and(|owner| owner.0 == actor)
                && proposed.occupancies.iter().any(|candidate| {
                    topology
                        .occupancies
                        .iter()
                        .any(|current| occupancies_conflict(candidate, current))
                })
        })
}

fn occupancies_conflict(
    candidate: &EquipmentTopologyOccupancy,
    current: &EquipmentTopologyOccupancy,
) -> bool {
    match (&candidate.anchor, &current.anchor) {
        (
            TacticalEquipmentAnchor::ItemAttachment {
                parent: left,
                attachment_point_id: left_point,
            },
            TacticalEquipmentAnchor::ItemAttachment {
                parent: right,
                attachment_point_id: right_point,
            },
        ) => {
            left == right
                && left_point == right_point
                && candidate.capacity_index == current.capacity_index
        }
        (
            TacticalEquipmentAnchor::CharacterLocation(left),
            TacticalEquipmentAnchor::CharacterLocation(right),
        ) => {
            left == right
                && candidate.channel == current.channel
                && (candidate.channel.singleton_per_location() || candidate.order == current.order)
        }
        _ => false,
    }
}

fn transfer_slot(
    commands: &mut Commands,
    actor: Entity,
    hand: EquipmentHand,
    location: EquipmentLocation,
    depth: u16,
    items: &Query<ItemView<'_>>,
) -> bool {
    if matches!(
        location,
        EquipmentLocation::LeftHand | EquipmentLocation::RightHand
    ) {
        return false;
    }
    let held = hand_item(actor, hand, items);
    let destination = ordered_at_location(actor, location, items)
        .get(depth as usize)
        .cloned();
    match held {
        None => {
            let Some(ReachableTarget::Occupied(item)) = destination else {
                return false;
            };
            if has_children(item, items) {
                return false;
            }
            commands
                .entity(item)
                .insert((hand_topology(hand), hand.slot()));
            true
        }
        Some(moving) => {
            let Ok((_, properties, _, _, _, _, _, _)) = items.get(moving) else {
                return false;
            };
            if has_children(moving, items) {
                return false;
            }
            let selected_entity = destination.as_ref().map(ReachableTarget::expected_entity);
            let proposed = match destination.as_ref() {
                Some(ReachableTarget::EmptyAttachment { .. }) => {
                    attachment_topology(&properties.id, destination.as_ref().unwrap(), actor, items)
                }
                Some(ReachableTarget::Occupied(item))
                    if items
                        .get(*item)
                        .is_ok_and(|(_, _, _, topology, _, _, _, _)| {
                            topology.occupancies.iter().any(|occupancy| {
                                matches!(
                                    occupancy.anchor,
                                    TacticalEquipmentAnchor::ItemAttachment { .. }
                                )
                            })
                        }) =>
                {
                    attachment_topology(&properties.id, destination.as_ref().unwrap(), actor, items)
                }
                _ => placement_topology(&properties.id, location),
            };
            let Some(proposed) = proposed else {
                return false;
            };
            if destination.as_ref().is_some_and(|target| match target {
                ReachableTarget::Occupied(item) => has_children(*item, items),
                ReachableTarget::EmptyAttachment { .. } => false,
            }) || topology_conflicts(
                actor,
                &proposed,
                &selected_entity.map_or(vec![moving], |item| vec![moving, item]),
                items,
            ) {
                return false;
            }
            // Every condition is proven before these deferred mutations: a
            // multi-location placement or occupied swap commits as one batch.
            commands
                .entity(moving)
                .insert(proposed)
                .remove::<EquipSlot>();
            if let Some(ReachableTarget::Occupied(swapped)) = destination {
                commands
                    .entity(swapped)
                    .insert((hand_topology(hand), hand.slot()));
            }
            true
        }
    }
}

fn transfer_hand(
    commands: &mut Commands,
    actor: Entity,
    source: EquipmentHand,
    destination: EquipmentHand,
    items: &Query<ItemView<'_>>,
) -> bool {
    if source == destination {
        return false;
    }
    let moving = hand_item(actor, source, items);
    let swapped = hand_item(actor, destination, items);
    if let Some(moving) = moving {
        commands
            .entity(moving)
            .insert((hand_topology(destination), destination.slot()));
    }
    if let Some(swapped) = swapped {
        commands
            .entity(swapped)
            .insert((hand_topology(source), source.slot()));
    }
    moving.is_some() || swapped.is_some()
}

fn drop_hand(
    commands: &mut Commands,
    actor: Entity,
    hand: EquipmentHand,
    players: &Query<(&Transform, &CharacterLook), With<Player>>,
    items: &Query<ItemView<'_>>,
    spatial: &SpatialQuery,
) -> bool {
    let Some(item) = hand_item(actor, hand, items) else {
        return false;
    };
    if has_children(item, items) {
        return false;
    }
    let Ok((_, _, _, _, _, physical, _, _)) = items.get(item) else {
        return false;
    };
    let Some(physical) = physical.copied().filter(|physical| physical.is_valid()) else {
        return false;
    };
    let Ok((actor_transform, _)) = players.get(actor) else {
        return false;
    };
    let shape = Collider::cuboid(
        physical.dimensions_m.x,
        physical.dimensions_m.y,
        physical.dimensions_m.z,
    );
    let Some(position) = [0.9_f32, 1.2, 1.5].into_iter().find_map(|distance| {
        let grip =
            actor_transform.translation + *actor_transform.forward() * distance + Vec3::Y * 0.65;
        let centre = item_box_center(grip, &physical);
        spatial
            .shape_intersections(
                &shape,
                centre,
                Quat::IDENTITY,
                &SpatialQueryFilter::from_excluded_entities([item]),
            )
            .is_empty()
            .then_some(grip)
    }) else {
        return false;
    };
    let collider = Collider::compound(vec![(-physical.anchor_offset_m, Quat::IDENTITY, shape)]);
    commands
        .entity(item)
        .remove::<ItemOf>()
        .remove::<EquipSlot>()
        .insert((
            EquipmentTopology::default(),
            TacticalSceneItem,
            Transform::from_translation(position),
            RigidBody::Dynamic,
            collider,
            CollisionLayers::new(TACTICAL_ITEM_LAYER, TACTICAL_TERRAIN_LAYER),
        ));
    true
}

fn item_box_center(grip: Vec3, physical: &TacticalEquipmentPhysical) -> Vec3 {
    grip - physical.anchor_offset_m
}

fn pickup(
    commands: &mut Commands,
    actor: Entity,
    hand: EquipmentHand,
    requested: Entity,
    players: &Query<(&Transform, &CharacterLook), With<Player>>,
    items: &Query<ItemView<'_>>,
    spatial: &SpatialQuery,
) -> bool {
    if hand_item(actor, hand, items).is_some() {
        return false;
    }
    let Ok((actor_transform, _)) = players.get(actor) else {
        return false;
    };
    let Ok((_, _, _, _, _, physical, scene, item_transform)) = items.get(requested) else {
        return false;
    };
    let Some((physical, item_transform)) = physical
        .filter(|physical| physical.is_valid())
        .zip(item_transform)
        .filter(|_| scene)
    else {
        return false;
    };
    let item_position = item_box_center(item_transform.translation, physical);
    if item_position.distance_squared(actor_transform.translation) > PICKUP_RANGE_M * PICKUP_RANGE_M
    {
        return false;
    }
    let origin = actor_transform.translation + Vec3::Y * 0.6;
    let sight = item_position - origin;
    let distance = sight.length();
    let Ok(direction) = Dir3::new(sight) else {
        return false;
    };
    // Terrain/support LOS is evaluated separately from item pointing; item
    // boxes therefore do not become combat/visibility blockers.
    let blocker_filter = SpatialQueryFilter::from_mask(TACTICAL_TERRAIN_LAYER);
    if spatial
        .cast_ray(origin, direction, distance, true, &blocker_filter)
        .is_some()
    {
        return false;
    }
    commands
        .entity(requested)
        .remove::<TacticalSceneItem>()
        .remove::<RigidBody>()
        .remove::<Collider>()
        .remove::<CollisionLayers>()
        .insert((
            ItemOf(actor),
            hand_topology(hand),
            hand.slot(),
            Transform::default(),
        ));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn multi_location_catalog_placement_is_planned_as_one_topology() {
        let topology = placement_topology("linen_tunic", EquipmentLocation::Chest)
            .expect("tunic chest placement");
        assert!(topology.occupancies.len() > 1);
        assert!(topology.occupancies.iter().any(|occupancy| {
            occupancy.anchor
                == TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftArm)
        }));
        assert!(topology.occupancies.iter().any(|occupancy| {
            occupancy.anchor
                == TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::RightArm)
        }));
    }

    #[test]
    fn hand_topology_contains_only_the_selected_mapped_hand() {
        let topology = hand_topology(EquipmentHand::Left);
        assert_eq!(topology.occupancies.len(), 1);
        assert_eq!(
            topology.occupancies[0].anchor,
            TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftHand)
        );
    }

    #[test]
    fn parent_only_stack_destination_fails_closed_in_body_planner() {
        assert!(placement_topology("arming_sword", EquipmentLocation::Chest).is_none());
    }

    #[test]
    fn hand_only_weapons_fail_closed_for_parent_placement() {
        for item_id in ["halberd", "hunting_spear", "military_pike", "spear"] {
            assert!(!parent_placement_allowed(item_id), "{item_id}");
            let equipment = item_catalog::definition(item_id)
                .unwrap()
                .equipment
                .as_ref()
                .unwrap();
            assert!(
                equipment
                    .placements
                    .iter()
                    .all(|placement| placement.parents.is_empty())
            );
        }
        assert!(parent_placement_allowed("arming_sword"));
    }

    #[test]
    fn hand_only_root_authority_accepts_only_one_held_hand() {
        let authored = item_catalog::definition("halberd")
            .unwrap()
            .equipment
            .as_ref()
            .unwrap()
            .placements[0]
            .clone();
        assert!(root_placement_allowed("halberd", &authored));
        assert!(
            placement_topology("halberd", authored.occupancy[0].location).is_some(),
            "authored hand placement remains usable"
        );

        let mut wrong_location = authored.clone();
        wrong_location.occupancy[0].location = EquipmentLocation::Chest;
        assert!(!root_placement_allowed("halberd", &wrong_location));

        let mut wrong_channel = authored.clone();
        wrong_channel.occupancy[0].channel = EquipmentChannel::Accessory;
        assert!(!root_placement_allowed("halberd", &wrong_channel));

        let mut multiple = authored.clone();
        multiple.occupancy.push(authored.occupancy[0]);
        assert!(!root_placement_allowed("halberd", &multiple));
    }

    #[test]
    fn sequence_replay_and_old_half_range_are_rejected() {
        assert!(sequence_is_newer(2, 1));
        assert!(!sequence_is_newer(1, 1));
        assert!(!sequence_is_newer(1, 2));
        assert!(sequence_is_newer(0, u32::MAX));
    }

    #[test]
    fn per_actor_pending_work_is_rate_bounded() {
        let actor = Entity::from_bits(7);
        let request = EquipmentActionRequest {
            actor,
            sequence: 1,
            expected_revision: 0,
            hand: EquipmentHand::Right,
            expected_hand_item: None,
            action: EquipmentAction::Drop,
        };
        let mut queue = VecDeque::new();
        for _ in 0..MAX_PENDING_PER_ACTOR {
            assert!(can_enqueue(&queue, actor));
            queue.push_back((actor, request));
        }
        assert!(!can_enqueue(&queue, actor));
        assert!(can_enqueue(&queue, Entity::from_bits(8)));
    }

    #[test]
    fn reconnect_rekeys_sequence_and_purges_old_pending_actions() {
        let old = Entity::from_bits(7);
        let new = Entity::from_bits(8);
        let request = EquipmentActionRequest {
            actor: old,
            sequence: 4,
            expected_revision: 0,
            hand: EquipmentHand::Right,
            expected_hand_item: None,
            action: EquipmentAction::Drop,
        };
        let mut pending = PendingEquipmentActions(VecDeque::from([(old, request)]));
        let mut sequences = LastEquipmentSequence(HashMap::from([(old, 3)]));
        reconnect_equipment_lifecycle(old, new, &mut pending, &mut sequences);
        assert!(pending.0.is_empty());
        assert_eq!(sequences.0.get(&new), Some(&3));
        purge_equipment_lifecycle(new, &mut pending, &mut sequences);
        assert!(!sequences.0.contains_key(&new));
    }

    #[test]
    fn attachment_capacity_indices_are_distinct_even_with_same_requirement_order() {
        let parent = Entity::from_bits(11);
        let occupancy = |capacity_index| EquipmentTopologyOccupancy {
            occupancy_id: format!("capacity-{capacity_index}"),
            anchor: TacticalEquipmentAnchor::ItemAttachment {
                parent,
                attachment_point_id: "loops".into(),
            },
            channel: EquipmentChannel::Mount,
            order: 0,
            requirement_index: 0,
            capacity_index,
        };
        assert!(occupancies_conflict(&occupancy(0), &occupancy(0)));
        assert!(!occupancies_conflict(&occupancy(0), &occupancy(1)));
    }

    #[test]
    fn pickup_box_center_accounts_for_nonzero_anchor_offset() {
        let physical = TacticalEquipmentPhysical {
            dimensions_m: Vec3::splat(0.2),
            grip_to_tip_m: 0.4,
            striking_head_length_m: 0.2,
            anchor_offset_m: Vec3::new(0.15, -0.05, 0.1),
        };
        assert_eq!(
            item_box_center(Vec3::new(2.0, 1.0, -3.0), &physical),
            Vec3::new(1.85, 1.05, -3.1)
        );
    }

    fn spawn_test_item(
        world: &mut World,
        actor: Entity,
        id: &str,
        topology: EquipmentTopology,
    ) -> Entity {
        world
            .spawn((
                ItemProperties {
                    id: id.into(),
                    weight: 1.0,
                },
                ItemOf(actor),
                topology,
                Transform::default(),
            ))
            .id()
    }

    #[test]
    fn belt_locations_traverse_only_their_authored_attachment_points() {
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let belt = spawn_test_item(
            &mut world,
            actor,
            "leather_belt",
            EquipmentTopology {
                placement_id: Some("worn".into()),
                occupancies: [
                    EquipmentLocation::LeftBelt,
                    EquipmentLocation::RightBelt,
                    EquipmentLocation::FrontBelt,
                    EquipmentLocation::BackBelt,
                ]
                .into_iter()
                .enumerate()
                .map(|(index, location)| EquipmentTopologyOccupancy {
                    occupancy_id: format!("belt-{index}"),
                    anchor: TacticalEquipmentAnchor::CharacterLocation(location),
                    channel: EquipmentChannel::Accessory,
                    order: 0,
                    requirement_index: index as u16,
                    capacity_index: 0,
                })
                .collect(),
            },
        );
        for (location, expected_point) in [
            (EquipmentLocation::LeftBelt, "left"),
            (EquipmentLocation::RightBelt, "right"),
            (EquipmentLocation::FrontBelt, "front"),
            (EquipmentLocation::BackBelt, "back"),
        ] {
            let reachable = world
                .run_system_once(move |items: Query<ItemView<'_>>| {
                    ordered_at_location(actor, location, &items)
                })
                .unwrap();
            assert!(
                matches!(reachable.first(), Some(ReachableTarget::EmptyAttachment { attachment_point_id, .. }) if attachment_point_id == expected_point)
            );
            assert_eq!(reachable.len(), 2);
            assert_eq!(reachable.last(), Some(&ReachableTarget::Occupied(belt)));
        }
    }

    #[test]
    fn multi_parent_attachment_is_atomic_and_capacity_fail_closed() {
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let belt = spawn_test_item(
            &mut world,
            actor,
            "leather_belt",
            EquipmentTopology {
                placement_id: Some("worn".into()),
                occupancies: vec![EquipmentTopologyOccupancy {
                    occupancy_id: "belt".into(),
                    anchor: TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftBelt),
                    channel: EquipmentChannel::Accessory,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                }],
            },
        );
        let selected = ReachableTarget::EmptyAttachment {
            parent: belt,
            attachment_point_id: "left".into(),
            channel: EquipmentChannel::Mount,
            capacity_index: 0,
        };
        let planned = world
            .run_system_once(move |items: Query<ItemView<'_>>| {
                attachment_topology("sword_sheath", &selected, actor, &items)
            })
            .unwrap()
            .expect("two belt mounts are available");
        assert_eq!(planned.occupancies.len(), 2);
        assert!(planned.occupancies.iter().all(|occupancy| {
            matches!(occupancy.anchor, TacticalEquipmentAnchor::ItemAttachment { parent, .. } if parent == belt)
        }));
        for (index, point) in ["left", "right"].into_iter().enumerate() {
            spawn_test_item(
                &mut world,
                actor,
                "leather_satchel",
                EquipmentTopology {
                    placement_id: Some("occupied".into()),
                    occupancies: vec![EquipmentTopologyOccupancy {
                        occupancy_id: format!("occupied-{point}"),
                        anchor: TacticalEquipmentAnchor::ItemAttachment {
                            parent: belt,
                            attachment_point_id: point.into(),
                        },
                        channel: EquipmentChannel::Mount,
                        order: index as u16,
                        requirement_index: 0,
                        capacity_index: 0,
                    }],
                },
            );
        }
        let incompatible = ReachableTarget::EmptyAttachment {
            parent: belt,
            attachment_point_id: "front".into(),
            channel: EquipmentChannel::Mount,
            capacity_index: 0,
        };
        assert!(
            world
                .run_system_once(move |items: Query<ItemView<'_>>| {
                    attachment_topology("sword_sheath", &incompatible, actor, &items)
                })
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn descendants_block_drop_and_graph_reparenting() {
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let parent = spawn_test_item(
            &mut world,
            actor,
            "leather_belt",
            EquipmentTopology::default(),
        );
        spawn_test_item(
            &mut world,
            actor,
            "leather_satchel",
            EquipmentTopology {
                placement_id: Some("belt_left".into()),
                occupancies: vec![EquipmentTopologyOccupancy {
                    occupancy_id: "edge".into(),
                    anchor: TacticalEquipmentAnchor::ItemAttachment {
                        parent,
                        attachment_point_id: "left".into(),
                    },
                    channel: EquipmentChannel::Mount,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                }],
            },
        );
        assert!(
            world
                .run_system_once(move |items: Query<ItemView<'_>>| has_children(parent, &items))
                .unwrap()
        );
    }

    #[test]
    fn item_and_terrain_layers_are_isolated() {
        let item = CollisionLayers::new(TACTICAL_ITEM_LAYER, TACTICAL_TERRAIN_LAYER);
        let terrain = CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL);
        assert!(item.interacts_with(terrain));
        assert!(!item.interacts_with(CollisionLayers::DEFAULT));
    }
}
