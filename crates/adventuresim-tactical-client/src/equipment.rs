//! Tactical grab input, QWERTY slot HUD, and placeholder item presentation.

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    prelude::{EquipmentActionRequest, EquipmentHand},
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::{
    animation::{HandSide, HeldWeaponConstraint},
    player::ClientPlayer,
};

const ITEM_LAYER: LayerMask = LayerMask(1 << 4);
const PICKUP_RANGE_M: f32 = 2.0;
const INVALID_FLASH_SECS: f32 = 0.18;

pub(crate) struct TacticalEquipmentPlugin;

impl Plugin for TacticalEquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrabSession>()
            .add_systems(PreUpdate, update_grab_input.after(bevy::input::InputSystems))
            .add_systems(Update, (spawn_item_placeholders, update_item_placeholders))
            .add_systems(EguiPrimaryContextPass, draw_slot_hud);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrabSelection {
    Slot { location: EquipmentLocation, depth: u16 },
    Hand(EquipmentHand),
    Scene(Entity),
}

#[derive(Resource, Default)]
struct GrabSession {
    active: Option<EquipmentHand>,
    selection: Option<GrabSelection>,
    repeated_input: Option<(&'static str, u16)>,
    invalid_flash_remaining: f32,
}

#[derive(Component)]
struct ItemPlaceholder(Entity);

fn held_item(
    actor: Entity,
    hand: EquipmentHand,
    items: &Query<(Entity, &ItemOf, Option<&EquipSlot>)>,
) -> Option<Entity> {
    items.iter().find_map(|(entity, owner, slot)| {
        (owner.0 == actor && slot.is_some_and(|slot| *slot == hand.slot())).then_some(entity)
    })
}

fn key_code(input: &str) -> Option<KeyCode> {
    Some(match input {
        "q" => KeyCode::KeyQ,
        "e" => KeyCode::KeyE,
        "f" => KeyCode::KeyF,
        "x" => KeyCode::KeyX,
        "tab" => KeyCode::Tab,
        "r" => KeyCode::KeyR,
        "g" => KeyCode::KeyG,
        "y" => KeyCode::KeyY,
        "h" => KeyCode::KeyH,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4,
        "5" => KeyCode::Digit5,
        "t" => KeyCode::KeyT,
        "`" => KeyCode::Backquote,
        "v" => KeyCode::KeyV,
        "b" => KeyCode::KeyB,
        "z" => KeyCode::KeyZ,
        "c" => KeyCode::KeyC,
        _ => return None,
    })
}

fn update_grab_input(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    player: Single<Entity, With<ClientPlayer>>,
    item_owners: Query<(Entity, &ItemOf, Option<&EquipSlot>)>,
    topologies: Query<(&ItemOf, &EquipmentTopology)>,
    properties: Query<&ItemProperties>,
    mut session: ResMut<GrabSession>,
) {
    session.invalid_flash_remaining =
        (session.invalid_flash_remaining - time.delta_secs()).max(0.0);
    let actor = *player;
    if session.active.is_none() && !mouse.pressed(MouseButton::Right) {
        session.active = if mouse.just_pressed(MouseButton::Left) {
            Some(EquipmentHand::Right)
        } else if mouse.just_pressed(MouseButton::Middle) {
            Some(EquipmentHand::Left)
        } else {
            None
        };
    }
    let Some(hand) = session.active else { return };
    let held = held_item(actor, hand, &item_owners);

    for mapping in INPUT_ADDRESS_MAPPINGS {
        let Some(key) = key_code(mapping.input) else { continue };
        if !keys.just_pressed(key) {
            continue;
        }
        let repeat = if session.repeated_input.is_some_and(|(input, _)| input == mapping.input) {
            session.repeated_input.unwrap().1.saturating_add(1)
        } else {
            0
        };
        let location_index = repeat as usize % mapping.locations.len();
        let depth = repeat / mapping.locations.len() as u16;
        let location = mapping.locations[location_index];
        let valid = if let Some(held) = held {
            properties.get(held).ok().is_some_and(|properties| {
                item_catalog::definition(&properties.id)
                    .and_then(|definition| definition.equipment.as_ref())
                    .is_some_and(|equipment| {
                        equipment.placements.iter().any(|placement| {
                            placement.parents.is_empty()
                                && placement
                                    .occupancy
                                    .iter()
                                    .any(|occupancy| occupancy.location == location)
                        })
                    })
            })
        } else {
            topologies.iter().any(|(owner, topology)| {
                owner.0 == actor
                    && topology.occupancies.iter().any(|occupancy| {
                        matches!(occupancy.anchor, TacticalEquipmentAnchor::CharacterLocation(found) if found == location)
                    })
            })
        };
        if valid {
            session.repeated_input = Some((mapping.input, repeat));
            session.selection = Some(GrabSelection::Slot { location, depth });
        } else {
            session.invalid_flash_remaining = INVALID_FLASH_SECS;
        }
    }

    let released = match hand {
        EquipmentHand::Right => mouse.just_released(MouseButton::Left),
        EquipmentHand::Left => mouse.just_released(MouseButton::Middle),
    };
    if !released {
        return;
    }
    let request = match session.selection {
        Some(GrabSelection::Slot { location, depth }) => EquipmentActionRequest::Slot {
            actor,
            hand,
            location,
            depth,
        },
        Some(GrabSelection::Hand(destination)) => EquipmentActionRequest::Hand {
            actor,
            hand,
            destination,
        },
        Some(GrabSelection::Scene(item)) => EquipmentActionRequest::Pickup {
            actor,
            hand,
            item,
        },
        None if held.is_some() => EquipmentActionRequest::Drop { actor, hand },
        None => {
            session.active = None;
            session.repeated_input = None;
            return;
        }
    };
    commands.client_trigger(request);
    session.active = None;
    session.selection = None;
    session.repeated_input = None;
}

fn slot_label(location: EquipmentLocation) -> String {
    format!("{location:?}")
}

fn draw_slot_hud(
    mut contexts: EguiContexts,
    player: Single<Entity, With<ClientPlayer>>,
    items: Query<(Entity, &ItemOf, Option<&EquipSlot>, &ItemProperties, &EquipmentTopology)>,
    scene_items: Query<(Entity, &ItemProperties), With<TacticalSceneItem>>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    spatial: SpatialQuery,
    mut session: ResMut<GrabSession>,
) {
    let Some(hand) = session.active else { return };
    let actor = *player;
    let held = items.iter().find(|(_, owner, slot, _, _)| {
        owner.0 == actor && slot.is_some_and(|slot| *slot == hand.slot())
    });
    let Ok(context) = contexts.ctx_mut() else { return };
    let invalid = session.invalid_flash_remaining > 0.0;
    egui::Window::new(match hand {
        EquipmentHand::Left => "Left-hand grab",
        EquipmentHand::Right => "Right-hand grab",
    })
    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 80.0))
    .title_bar(false)
    .resizable(false)
    .show(context, |ui| {
        ui.label(held.map_or("Empty hand", |(_, _, _, item, _)| item.id.as_str()));
        let size = egui::vec2(86.0, 55.0);
        egui::Grid::new("tactical-slot-qwerty")
            .num_columns(7)
            .spacing(egui::vec2(5.0, 5.0))
            .show(ui, |ui| {
                for row in 0..4 {
                    for column in 0..7 {
                        if let Some(mapping) = INPUT_ADDRESS_MAPPINGS.iter().find(|mapping| {
                            mapping.keyboard_row == row && mapping.keyboard_column == column
                        }) {
                            let location = mapping.locations[0];
                            let occupant = items.iter().find(|(_, owner, _, _, topology)| {
                                owner.0 == actor
                                    && topology.occupancies.iter().any(|occupancy| {
                                        matches!(occupancy.anchor, TacticalEquipmentAnchor::CharacterLocation(found) if found == location)
                                    })
                            });
                            let valid = if let Some((_, _, _, held, _)) = held {
                                item_catalog::definition(&held.id)
                                    .and_then(|definition| definition.equipment.as_ref())
                                    .is_some_and(|equipment| equipment.placements.iter().any(|placement| {
                                        placement.parents.is_empty() && placement.occupancy.iter().any(|occupancy| occupancy.location == location)
                                    }))
                            } else {
                                occupant.is_some()
                            };
                            let selected = matches!(session.selection, Some(GrabSelection::Slot { location: found, .. }) if found == location);
                            let text = format!(
                                "{}\n{}\n{}",
                                mapping.input.to_uppercase(),
                                slot_label(location),
                                occupant.map_or("", |(_, _, _, item, _)| item.id.as_str())
                            );
                            let button = egui::Button::new(text)
                                .selected(selected)
                                .fill(if invalid { egui::Color32::from_rgb(90, 20, 20) } else { egui::Color32::TRANSPARENT });
                            if ui.add_enabled(valid, button).clicked() {
                                session.selection = Some(GrabSelection::Slot { location, depth: 0 });
                            }
                        } else {
                            ui.allocate_space(size);
                        }
                    }
                    ui.end_row();
                }
            });
        let other = match hand { EquipmentHand::Left => EquipmentHand::Right, EquipmentHand::Right => EquipmentHand::Left };
        if ui.button(format!("{:?} hand", other)).clicked() {
            session.selection = Some(GrabSelection::Hand(other));
        }
        if held.is_none()
            && let Ok(camera) = cameras.single()
            && let Some(hit) = spatial.cast_ray(
                camera.translation(),
                camera.forward(),
                PICKUP_RANGE_M,
                true,
                &SpatialQueryFilter::from_mask(ITEM_LAYER),
            )
            && let Ok((entity, item)) = scene_items.get(hit.entity)
            && ui.button(format!("Pick up {}", item.id)).clicked()
        {
            session.selection = Some(GrabSelection::Scene(entity));
        }
        if let Some((_, depth)) = session.repeated_input {
            ui.label(format!("Depth {}", depth + 1));
        }
    });
}

fn spawn_item_placeholders(
    mut commands: Commands,
    added: Query<(Entity, &EquipmentPhysical), Added<EquipmentPhysical>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (item, physical) in &added {
        if !physical.is_valid() {
            continue;
        }
        let root = commands
            .spawn((
                Name::new("Tactical item placeholder"),
                ItemPlaceholder(item),
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        commands.entity(root).with_child((
            Mesh3d(meshes.add(Cuboid::new(
                physical.dimensions_m.x,
                physical.dimensions_m.y,
                physical.dimensions_m.z,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.48, 0.34, 0.18),
                perceptual_roughness: 0.8,
                ..default()
            })),
            // The root is the authored grip. Box centre is offset from it;
            // local +Y remains the weapon-tip direction.
            Transform::from_translation(-physical.grip_offset_m),
        ));
    }
}

fn update_item_placeholders(
    mut commands: Commands,
    items: Query<(
        &Transform,
        Option<&ItemOf>,
        Option<&EquipSlot>,
        &EquipmentTopology,
        Has<TacticalSceneItem>,
    )>,
    owners: Query<&GlobalTransform, With<Player>>,
    mut placeholders: Query<(Entity, &ItemPlaceholder, &mut Transform)>,
) {
    for (entity, placeholder, mut transform) in &mut placeholders {
        let Ok((item_transform, owner, slot, topology, scene)) = items.get(placeholder.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        if scene {
            *transform = *item_transform;
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        } else if let (Some(owner), Some(slot)) = (owner, slot) {
            let side = match slot {
                EquipSlot::HoldingLeft => Some(HandSide::Left),
                EquipSlot::HoldingRight => Some(HandSide::Right),
                _ => None,
            };
            if let Some(primary_hand) = side {
                commands.entity(entity).insert(HeldWeaponConstraint {
                    owner: owner.0,
                    primary_hand,
                    secondary_grip_local: None,
                });
            }
        } else if let Some(owner) = owner.and_then(|owner| owners.get(owner.0).ok()) {
            let offset = topology.occupancies.first().map_or(Vec3::Y, |occupancy| match occupancy.anchor {
                TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::Head) => Vec3::Y * 1.7,
                TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftArm) => Vec3::new(-0.45, 1.0, 0.0),
                TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::RightArm) => Vec3::new(0.45, 1.0, 0.0),
                TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftLeg) => Vec3::new(-0.2, 0.45, 0.0),
                TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::RightLeg) => Vec3::new(0.2, 0.45, 0.0),
                _ => Vec3::Y,
            });
            *transform = Transform::from_translation(owner.transform_point(offset));
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_keys_are_not_slot_addresses() {
        for key in ["w", "a", "s", "d"] {
            assert!(INPUT_ADDRESS_MAPPINGS.iter().all(|mapping| mapping.input != key));
        }
    }

    #[test]
    fn repeated_input_walks_location_alternatives_then_depth() {
        let mapping = INPUT_ADDRESS_MAPPINGS.iter().find(|mapping| mapping.input == "t").unwrap();
        assert!(mapping.locations.len() > 1);
        let repeat = mapping.locations.len() as u16;
        assert_eq!(repeat as usize % mapping.locations.len(), 0);
        assert_eq!(repeat / mapping.locations.len() as u16, 1);
    }
}
