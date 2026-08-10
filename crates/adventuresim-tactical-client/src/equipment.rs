//! Tactical grab input, QWERTY slot HUD, and placeholder item presentation.

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    prelude::{EquipmentAction, EquipmentActionRequest, EquipmentHand},
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, EguiTextureHandle, egui};

use crate::{
    animation::{HandSide, HeldWeaponConstraint},
    player::ClientPlayer,
};

const PICKUP_RANGE_M: f32 = 2.0;
const INVALID_FLASH_SECS: f32 = 0.18;
const EQUIPMENT_ICON_SLUGS: [&str; 56] = [
    "ancient-sword",
    "arm-bandage",
    "armor-cuisses",
    "armor-vest",
    "barbute",
    "belt-armor",
    "bo",
    "bordered-shield",
    "bow-arrow",
    "bowie-knife",
    "bracer",
    "breastplate",
    "broad-dagger",
    "broadsword",
    "brodie-helmet",
    "chain-mail",
    "chest-armor",
    "crested-helmet",
    "crossbow",
    "daggers",
    "flanged-mace",
    "greaves",
    "halberd",
    "heavy-helm",
    "helmet",
    "knapsack",
    "layered-armor",
    "light-helm",
    "mailed-fist",
    "mail-shirt",
    "metal-skirt",
    "musket",
    "piercing-sword",
    "plain-dagger",
    "pocket-bow",
    "pteruges",
    "relic-blade",
    "rifle",
    "roman-shield",
    "round-shield",
    "saber-slash",
    "shield",
    "shirt",
    "skirt",
    "sleeveless-jacket",
    "spear-hook",
    "spears",
    "stiletto",
    "sword-hilt",
    "templar-shield",
    "trousers",
    "two-handed-sword",
    "visored-helm",
    "warhammer",
    "wood-axe",
    "wood-club",
];

fn icon_uv(slug: &str) -> egui::Rect {
    let index = EQUIPMENT_ICON_SLUGS
        .iter()
        .position(|candidate| *candidate == slug)
        .unwrap_or(0);
    let column = (index % 8) as f32;
    let row = (index / 8) as f32;
    egui::Rect::from_min_max(
        egui::pos2(column / 8.0, row / 7.0),
        egui::pos2((column + 1.0) / 8.0, (row + 1.0) / 7.0),
    )
}

pub(crate) struct TacticalEquipmentPlugin;

impl Plugin for TacticalEquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrabSession>()
            .add_systems(
                PreUpdate,
                update_grab_input.after(bevy::input::InputSystems),
            )
            .add_systems(Update, (spawn_item_placeholders, update_item_placeholders))
            .add_systems(EguiPrimaryContextPass, draw_slot_hud);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrabSelection {
    Slot {
        location: EquipmentLocation,
        depth: u16,
    },
    Hand(EquipmentHand),
    Scene(Entity),
}

#[derive(Resource, Default)]
struct GrabSession {
    active: Option<EquipmentHand>,
    selection: Option<GrabSelection>,
    repeated_input: Option<(&'static str, u16)>,
    invalid_flash_remaining: f32,
    next_sequence: u32,
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
    topologies: Query<(Entity, &ItemOf, &EquipmentTopology, &ItemProperties)>,
    properties: Query<&ItemProperties>,
    action_states: Query<&EquipmentActionState>,
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
        let Some(key) = key_code(mapping.input) else {
            continue;
        };
        if !keys.just_pressed(key) {
            continue;
        }
        let repeat = if session
            .repeated_input
            .is_some_and(|(input, _)| input == mapping.input)
        {
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
                            placement
                                .occupancy
                                .iter()
                                .any(|occupancy| occupancy.location == location)
                                || (!placement.parents.is_empty()
                                    && ordered_preview_at_location(actor, location, &topologies)
                                        .get(depth as usize)
                                        .is_some())
                        })
                    })
            })
        } else {
            ordered_preview_at_location(actor, location, &topologies)
                .get(depth as usize)
                .is_some_and(|target| target.occupied)
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
    let action = match session.selection {
        Some(GrabSelection::Slot { location, depth }) => EquipmentAction::Slot {
            location,
            depth,
            expected_destination: ordered_preview_at_location(actor, location, &topologies)
                .get(depth as usize)
                .map(|target| target.entity),
        },
        Some(GrabSelection::Hand(destination)) => EquipmentAction::Hand {
            destination,
            expected_destination: held_item(actor, destination, &item_owners),
        },
        Some(GrabSelection::Scene(item)) => EquipmentAction::Pickup { item },
        None if held.is_some() => EquipmentAction::Drop,
        None => {
            session.active = None;
            session.repeated_input = None;
            return;
        }
    };
    session.next_sequence = session.next_sequence.wrapping_add(1);
    commands.client_trigger(EquipmentActionRequest {
        actor,
        sequence: session.next_sequence,
        expected_revision: action_states.get(actor).map_or(0, |state| state.revision),
        hand,
        expected_hand_item: held,
        action,
    });
    session.active = None;
    session.selection = None;
    session.repeated_input = None;
}

#[derive(Clone, Copy)]
struct PreviewTarget {
    entity: Entity,
    occupied: bool,
}

fn ordered_preview_at_location(
    actor: Entity,
    location: EquipmentLocation,
    topologies: &Query<(Entity, &ItemOf, &EquipmentTopology, &ItemProperties)>,
) -> Vec<PreviewTarget> {
    let mut found: Vec<_> = topologies
        .iter()
        .filter(|(_, owner, _, _)| owner.0 == actor)
        .filter_map(|(entity, _, topology, _)| {
            topology
                .occupancies
                .iter()
                .find_map(|occupancy| match occupancy.anchor {
                    TacticalEquipmentAnchor::CharacterLocation(found) if found == location => {
                        Some(((occupancy.channel.order(), occupancy.order), entity))
                    }
                    _ => None,
                })
        })
        .collect();
    found.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.to_bits().cmp(&right.1.to_bits()))
    });
    let mut output = Vec::new();
    let mut visited = std::collections::HashSet::new();
    for (_, entity) in found {
        append_preview(entity, topologies, &mut visited, &mut output);
    }
    output
}

fn append_preview(
    entity: Entity,
    items: &Query<(Entity, &ItemOf, &EquipmentTopology, &ItemProperties)>,
    visited: &mut std::collections::HashSet<Entity>,
    output: &mut Vec<PreviewTarget>,
) {
    if !visited.insert(entity) {
        return;
    }
    output.push(PreviewTarget {
        entity,
        occupied: true,
    });
    let Ok((_, _, _, properties)) = items.get(entity) else {
        return;
    };
    let Some(equipment) = item_catalog::definition(&properties.id)
        .and_then(|definition| definition.equipment.as_ref())
    else {
        return;
    };
    let mut points: Vec<_> = equipment.attachment_points.iter().collect();
    points.sort_by_key(|point| (point.order, point.id.as_str()));
    for point in points {
        for capacity_index in 0..point.capacity {
            let child =
                items.iter().find_map(|(child, _, topology, _)| {
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
                append_preview(child, items, visited, output);
            } else {
                // Empty attachment points address their mapped parent. Depth
                // disambiguates the exact authored point/capacity on server.
                output.push(PreviewTarget {
                    entity,
                    occupied: false,
                });
            }
        }
    }
}

fn slot_label(location: EquipmentLocation) -> String {
    format!("{location:?}")
}

fn hud_layers(
    actor: Entity,
    location: EquipmentLocation,
    items: &Query<(
        Entity,
        &ItemOf,
        Option<&EquipSlot>,
        &ItemProperties,
        &EquipmentTopology,
    )>,
) -> Vec<PreviewTarget> {
    let mut roots: Vec<_> = items
        .iter()
        .filter(|(_, owner, _, _, _)| owner.0 == actor)
        .filter_map(|(entity, _, _, _, topology)| {
            topology
                .occupancies
                .iter()
                .find_map(|occupancy| match occupancy.anchor {
                    TacticalEquipmentAnchor::CharacterLocation(found) if found == location => {
                        Some(((occupancy.channel.order(), occupancy.order), entity))
                    }
                    _ => None,
                })
        })
        .collect();
    roots.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.to_bits().cmp(&right.1.to_bits()))
    });
    let mut output = Vec::new();
    let mut visited = std::collections::HashSet::new();
    fn append(
        entity: Entity,
        items: &Query<(
            Entity,
            &ItemOf,
            Option<&EquipSlot>,
            &ItemProperties,
            &EquipmentTopology,
        )>,
        visited: &mut std::collections::HashSet<Entity>,
        output: &mut Vec<PreviewTarget>,
    ) {
        if !visited.insert(entity) {
            return;
        }
        output.push(PreviewTarget {
            entity,
            occupied: true,
        });
        let Ok((_, _, _, properties, _)) = items.get(entity) else {
            return;
        };
        let Some(equipment) = item_catalog::definition(&properties.id)
            .and_then(|definition| definition.equipment.as_ref())
        else {
            return;
        };
        let mut points: Vec<_> = equipment.attachment_points.iter().collect();
        points.sort_by_key(|point| (point.order, point.id.as_str()));
        for point in points {
            for capacity_index in 0..point.capacity {
                let child = items.iter().find_map(|(child, _, _, _, topology)| {
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
                    append(child, items, visited, output);
                } else {
                    output.push(PreviewTarget {
                        entity,
                        occupied: false,
                    });
                }
            }
        }
    }
    for (_, root) in roots {
        append(root, items, &mut visited, &mut output);
    }
    output
}

fn draw_slot_hud(
    mut contexts: EguiContexts,
    asset_server: Res<AssetServer>,
    mut icon_atlas: Local<Option<Handle<Image>>>,
    player: Single<Entity, With<ClientPlayer>>,
    items: Query<(
        Entity,
        &ItemOf,
        Option<&EquipSlot>,
        &ItemProperties,
        &EquipmentTopology,
    )>,
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
    let atlas = icon_atlas.get_or_insert_with(|| asset_server.load("tactical-equipment-icons.png"));
    let atlas_texture = contexts.add_image(EguiTextureHandle::Weak(atlas.id()));
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
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
        let mut joined_cells = Vec::new();
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
                            let layers = hud_layers(actor, location, &items);
                            let valid = if let Some((_, _, _, held, _)) = held {
                                item_catalog::definition(&held.id)
                                    .and_then(|definition| definition.equipment.as_ref())
                                    .is_some_and(|equipment| equipment.placements.iter().any(|placement| {
                                        placement.occupancy.iter().any(|occupancy| occupancy.location == location)
                                            || (!placement.parents.is_empty() && !layers.is_empty())
                                    }))
                            } else {
                                !layers.is_empty()
                            };
                            ui.vertical(|ui| {
                                ui.set_min_width(size.x);
                                ui.label(egui::RichText::new(format!(
                                    "{}  {}",
                                    mapping.input.to_uppercase(),
                                    slot_label(location)
                                )).strong());
                                if layers.is_empty() {
                                    let button = egui::Button::new("empty").fill(if invalid {
                                        egui::Color32::from_rgb(90, 20, 20)
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    });
                                    if ui.add_enabled(valid, button).clicked() {
                                        session.selection = Some(GrabSelection::Slot { location, depth: 0 });
                                    }
                                }
                                for (depth, layer) in layers.iter().copied().enumerate() {
                                    let Ok((_, _, _, item, topology)) = items.get(layer.entity) else { continue };
                                    let icon = item_catalog::definition(&item.id)
                                        .map(|definition| definition.presentation.icon.as_str())
                                        .unwrap_or("help");
                                    let multi = topology.occupancies.iter().filter(|occupancy| {
                                        matches!(occupancy.anchor, TacticalEquipmentAnchor::CharacterLocation(_))
                                    }).count() > 1;
                                    let suffix = if held.is_some() { "swap → hand" } else { "draw → hand" };
                                    let text = if layer.occupied {
                                        format!(
                                            "{}\n{} · {}",
                                            item.id,
                                            depth + 1,
                                            suffix
                                        )
                                    } else {
                                        format!("＋ attachment\n{} · place", depth + 1)
                                    };
                                    let shade = (235_i32 - depth as i32 * 28).max(95) as u8;
                                    let selected = matches!(
                                        session.selection,
                                        Some(GrabSelection::Slot { location: found, depth: found_depth })
                                            if found == location && found_depth as usize == depth
                                    );
                                    let button = egui::Button::new(
                                        egui::RichText::new(text).color(egui::Color32::from_gray(shade)),
                                    )
                                    .selected(selected)
                                    .fill(if invalid {
                                        egui::Color32::from_rgb(90, 20, 20)
                                    } else if selected {
                                        egui::Color32::from_rgb(45, 70, 105)
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    });
                                    let response = ui.horizontal(|ui| {
                                        ui.add(egui::Image::new((atlas_texture, egui::vec2(22.0, 22.0))).uv(icon_uv(icon)));
                                        ui.add_enabled(valid && (held.is_some() || layer.occupied), button)
                                    }).inner;
                                    if multi {
                                        joined_cells.push((response.rect, layer.entity));
                                    }
                                    if response.clicked() {
                                        session.selection = Some(GrabSelection::Slot {
                                            location,
                                            depth: depth as u16,
                                        });
                                    }
                                }
                            });
                        } else {
                            ui.allocate_space(size);
                        }
                    }
                    ui.end_row();
                }
            });
        for (index, (left, entity)) in joined_cells.iter().enumerate() {
            for (right, other) in joined_cells.iter().skip(index + 1) {
                if entity != other { continue; }
                let horizontally_adjacent = (left.center().y - right.center().y).abs() < 8.0
                    && (left.right() - right.left()).abs().min((right.right() - left.left()).abs()) < 16.0;
                let vertically_adjacent = (left.center().x - right.center().x).abs() < 8.0
                    && (left.bottom() - right.top()).abs().min((right.bottom() - left.top()).abs()) < 16.0;
                if horizontally_adjacent || vertically_adjacent {
                    let segment = if horizontally_adjacent {
                        let (first, second) = if left.center().x < right.center().x { (left, right) } else { (right, left) };
                        [egui::pos2(first.right(), first.center().y), egui::pos2(second.left(), second.center().y)]
                    } else {
                        let (first, second) = if left.center().y < right.center().y { (left, right) } else { (right, left) };
                        [egui::pos2(first.center().x, first.bottom()), egui::pos2(second.center().x, second.top())]
                    };
                    context.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("equipment-cell-joins")))
                        .line_segment(segment, egui::Stroke::new(5.0_f32, egui::Color32::from_rgb(75, 115, 165)));
                }
            }
        }
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
                &SpatialQueryFilter::from_mask(TACTICAL_ITEM_LAYER),
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
        } else if let (Some(owner), Some(primary_hand)) = (owner, holding_side(slot)) {
            commands.entity(entity).insert(HeldWeaponConstraint {
                owner: owner.0,
                primary_hand,
                secondary_grip_local: None,
            });
        } else if let Some(owner) = owner.and_then(|owner| owners.get(owner.0).ok()) {
            let offset = topology
                .occupancies
                .first()
                .map_or(Vec3::Y, |occupancy| match occupancy.anchor {
                    TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::Head) => {
                        Vec3::Y * 1.7
                    }
                    TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftArm) => {
                        Vec3::new(-0.45, 1.0, 0.0)
                    }
                    TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::RightArm) => {
                        Vec3::new(0.45, 1.0, 0.0)
                    }
                    TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::LeftLeg) => {
                        Vec3::new(-0.2, 0.45, 0.0)
                    }
                    TacticalEquipmentAnchor::CharacterLocation(EquipmentLocation::RightLeg) => {
                        Vec3::new(0.2, 0.45, 0.0)
                    }
                    _ => Vec3::Y,
                });
            *transform = Transform::from_translation(owner.transform_point(offset));
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        }
    }
}

fn holding_side(slot: Option<&EquipSlot>) -> Option<HandSide> {
    match slot {
        Some(EquipSlot::HoldingLeft) => Some(HandSide::Left),
        Some(EquipSlot::HoldingRight) => Some(HandSide::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_keys_are_not_slot_addresses() {
        for key in ["w", "a", "s", "d"] {
            assert!(
                INPUT_ADDRESS_MAPPINGS
                    .iter()
                    .all(|mapping| mapping.input != key)
            );
        }
    }

    #[test]
    fn repeated_input_walks_location_alternatives_then_depth() {
        let mapping = INPUT_ADDRESS_MAPPINGS
            .iter()
            .find(|mapping| mapping.input == "t")
            .unwrap();
        assert!(mapping.locations.len() > 1);
        let repeat = mapping.locations.len() as u16;
        assert_eq!(repeat as usize % mapping.locations.len(), 0);
        assert_eq!(repeat / mapping.locations.len() as u16, 1);
    }

    #[test]
    fn armor_slots_use_body_anchor_instead_of_stale_hand_constraint() {
        assert_eq!(holding_side(Some(&EquipSlot::ArmorChest)), None);
        assert_eq!(
            holding_side(Some(&EquipSlot::HoldingLeft)),
            Some(HandSide::Left)
        );
    }

    #[test]
    fn every_equipment_catalog_icon_is_present_in_tactical_atlas() {
        for definition in item_catalog::catalog()
            .iter()
            .filter(|definition| definition.equipment.is_some())
        {
            assert!(
                EQUIPMENT_ICON_SLUGS.contains(&definition.presentation.icon.as_str()),
                "missing tactical atlas icon {}",
                definition.presentation.icon
            );
        }
    }
}
