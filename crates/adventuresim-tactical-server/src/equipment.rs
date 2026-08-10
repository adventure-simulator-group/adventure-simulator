//! Authoritative, transient tactical equipment mutations.

use adventuresim_core::item_catalog;
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::FromClient,
    prelude::{EquipmentActionRequest, EquipmentHand},
};
use bevy::prelude::*;

pub(crate) const ITEM_LAYER: LayerMask = LayerMask(1 << 4);
const PICKUP_RANGE_M: f32 = 2.0;

pub(crate) struct TacticalEquipmentPlugin;

impl Plugin for TacticalEquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_equipment_action);
    }
}

type ItemView<'a> = (
    Entity,
    &'a ItemProperties,
    Option<&'a ItemOf>,
    &'a EquipmentTopology,
    Option<&'a EquipSlot>,
    Option<&'a EquipmentPhysical>,
    Has<TacticalSceneItem>,
    Option<&'a Transform>,
);

fn on_equipment_action(
    request: On<FromClient<EquipmentActionRequest>>,
    mut commands: Commands,
    players: Query<(&Transform, &CharacterLook), With<Player>>,
    items: Query<ItemView<'_>>,
    spatial: SpatialQuery,
) {
    let Some(controlled) = request.client_id.entity() else {
        return;
    };
    let actor = match **request {
        EquipmentActionRequest::Slot { actor, .. }
        | EquipmentActionRequest::Hand { actor, .. }
        | EquipmentActionRequest::Drop { actor, .. }
        | EquipmentActionRequest::Pickup { actor, .. } => actor,
    };
    if actor != controlled || players.get(actor).is_err() {
        warn!(?actor, ?controlled, "rejected equipment request for uncontrolled actor");
        return;
    }

    match **request {
        EquipmentActionRequest::Slot {
            hand,
            location,
            depth,
            ..
        } => transfer_slot(&mut commands, actor, hand, location, depth, &items),
        EquipmentActionRequest::Hand {
            hand, destination, ..
        } => transfer_hand(&mut commands, actor, hand, destination, &items),
        EquipmentActionRequest::Drop { hand, .. } => {
            drop_hand(&mut commands, actor, hand, &players, &items)
        }
        EquipmentActionRequest::Pickup { hand, item, .. } => pickup(
            &mut commands,
            actor,
            hand,
            item,
            &players,
            &items,
            &spatial,
        ),
    }
}

fn hand_item(actor: Entity, hand: EquipmentHand, items: &Query<ItemView<'_>>) -> Option<Entity> {
    items.iter().find_map(|(entity, _, owner, _, slot, _, scene, _)| {
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
) -> Vec<Entity> {
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
    found.sort_by(|left, right| right.0.cmp(&left.0));
    found.into_iter().map(|(_, entity)| entity).collect()
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
        placement_id: Some(match hand {
            EquipmentHand::Left => "left_hand",
            EquipmentHand::Right => "right_hand",
        }
        .into()),
        occupancies: vec![EquipmentTopologyOccupancy {
            occupancy_id: format!("tactical:{:?}:held", hand),
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

fn topology_conflicts(
    actor: Entity,
    proposed: &EquipmentTopology,
    ignored: &[Entity],
    items: &Query<ItemView<'_>>,
) -> bool {
    items.iter().any(|(entity, _, owner, topology, _, _, scene, _)| {
        !scene
            && !ignored.contains(&entity)
            && owner.is_some_and(|owner| owner.0 == actor)
            && proposed.occupancies.iter().any(|candidate| {
                topology.occupancies.iter().any(|current| {
                    candidate.anchor == current.anchor
                        && candidate.channel == current.channel
                        && (candidate.channel.singleton_per_location()
                            || candidate.order == current.order)
                })
            })
    })
}

fn transfer_slot(
    commands: &mut Commands,
    actor: Entity,
    hand: EquipmentHand,
    location: EquipmentLocation,
    depth: u16,
    items: &Query<ItemView<'_>>,
) {
    if matches!(location, EquipmentLocation::LeftHand | EquipmentLocation::RightHand) {
        return;
    }
    let held = hand_item(actor, hand, items);
    let destination = ordered_at_location(actor, location, items)
        .get(depth as usize)
        .copied();
    match held {
        None => {
            let Some(item) = destination else { return };
            if has_children(item, items) {
                return;
            }
            commands.entity(item).insert((hand_topology(hand), hand.slot()));
        }
        Some(moving) => {
            let Ok((_, properties, _, _, _, _, _, _)) = items.get(moving) else {
                return;
            };
            let Some(proposed) = placement_topology(&properties.id, location) else {
                return;
            };
            if destination.is_some_and(|item| has_children(item, items))
                || topology_conflicts(
                    actor,
                    &proposed,
                    &destination.map_or(vec![moving], |item| vec![moving, item]),
                    items,
                )
            {
                return;
            }
            // Every condition is proven before these deferred mutations: a
            // multi-location placement or occupied swap commits as one batch.
            commands.entity(moving).insert(proposed).remove::<EquipSlot>();
            if let Some(swapped) = destination {
                commands.entity(swapped).insert((hand_topology(hand), hand.slot()));
            }
        }
    }
}

fn transfer_hand(
    commands: &mut Commands,
    actor: Entity,
    source: EquipmentHand,
    destination: EquipmentHand,
    items: &Query<ItemView<'_>>,
) {
    if source == destination {
        return;
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
}

fn drop_hand(
    commands: &mut Commands,
    actor: Entity,
    hand: EquipmentHand,
    players: &Query<(&Transform, &CharacterLook), With<Player>>,
    items: &Query<ItemView<'_>>,
) {
    let Some(item) = hand_item(actor, hand, items) else { return };
    let Ok((_, _, _, _, _, physical, _, _)) = items.get(item) else { return };
    let Some(physical) = physical.copied().filter(|physical| physical.is_valid()) else {
        return;
    };
    let Ok((actor_transform, _)) = players.get(actor) else { return };
    let position = actor_transform.translation + *actor_transform.forward() * 0.8 + Vec3::Y * 0.5;
    commands
        .entity(item)
        .remove::<ItemOf>()
        .remove::<EquipSlot>()
        .insert((
            EquipmentTopology::default(),
            TacticalSceneItem,
            Transform::from_translation(position),
            RigidBody::Dynamic,
            Collider::cuboid(
                physical.dimensions_m.x,
                physical.dimensions_m.y,
                physical.dimensions_m.z,
            ),
            CollisionLayers::new(ITEM_LAYER, LayerMask::ALL),
        ));
}

fn pickup(
    commands: &mut Commands,
    actor: Entity,
    hand: EquipmentHand,
    requested: Entity,
    players: &Query<(&Transform, &CharacterLook), With<Player>>,
    items: &Query<ItemView<'_>>,
    spatial: &SpatialQuery,
) {
    if hand_item(actor, hand, items).is_some() {
        return;
    }
    let Ok((actor_transform, look)) = players.get(actor) else { return };
    let origin = actor_transform.translation + Vec3::Y * 0.6;
    let direction = Dir3::new(
        Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0) * Vec3::NEG_Z,
    )
    .unwrap_or(Dir3::NEG_Z);
    let mut candidates: Vec<_> = items
        .iter()
        .filter(|(_, _, _, _, _, physical, scene, transform)| {
            *scene && physical.is_some() && transform.is_some()
        })
        .filter_map(|(entity, _, _, _, _, physical, _, transform)| {
            let transform = transform?;
            let half_width = physical?.dimensions_m.xz().length() * 0.5;
            let delta = transform.translation - origin;
            let ray_distance = delta.dot(*direction);
            let off_axis = (delta - *direction * ray_distance).length();
            (ray_distance >= 0.0
                && ray_distance <= PICKUP_RANGE_M
                && off_axis <= half_width)
                .then_some((ray_distance, entity.to_bits(), entity))
        })
        .collect();
    candidates.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
    if candidates.first().map(|candidate| candidate.2) != Some(requested) {
        return;
    }
    let target = candidates[0];
    let blocker_filter = SpatialQueryFilter::from_excluded_entities([actor, requested]);
    if spatial
        .cast_ray(origin, direction, target.0, true, &blocker_filter)
        .is_some()
    {
        return;
    }
    commands
        .entity(requested)
        .remove::<TacticalSceneItem>()
        .remove::<RigidBody>()
        .remove::<Collider>()
        .remove::<CollisionLayers>()
        .insert((ItemOf(actor), hand_topology(hand), hand.slot(), Transform::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
