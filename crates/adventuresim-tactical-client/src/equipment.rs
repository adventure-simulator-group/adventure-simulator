//! Tactical grab input, QWERTY slot HUD, and placeholder item presentation.

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    prelude::{EquipmentAction, EquipmentActionRequest, EquipmentHand},
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, EguiTextureHandle, egui};
use bevy_mod_outline::{OutlineMode, OutlinePlugin, OutlineVolume};

use crate::{
    animation::{BoneRole, HandSide, HeldWeaponConstraint, HumanoidRig},
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
        app.add_plugins(OutlinePlugin::JUMP_FLOOD)
            .init_resource::<GrabSession>()
            .add_systems(
                PreUpdate,
                update_grab_input.after(bevy::input::InputSystems),
            )
            .add_systems(
                Update,
                (
                    spawn_item_placeholders,
                    update_item_placeholders,
                    update_pickup_outlines,
                ),
            )
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

#[derive(Component)]
struct PickupOutline(Entity);

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
    player: Single<(Entity, &GlobalTransform), With<ClientPlayer>>,
    item_owners: Query<(Entity, &ItemOf, Option<&EquipSlot>)>,
    topologies: Query<(Entity, &ItemOf, &EquipmentTopology, &ItemProperties)>,
    properties: Query<&ItemProperties>,
    action_states: Query<&EquipmentActionState>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    scene_items: Query<(Entity, &GlobalTransform, &EquipmentPhysical), With<TacticalSceneItem>>,
    spatial: SpatialQuery,
    mut session: ResMut<GrabSession>,
) {
    session.invalid_flash_remaining =
        (session.invalid_flash_remaining - time.delta_secs()).max(0.0);
    let (actor, actor_transform) = *player;
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
    session.selection = scene_grab_selection(
        session.selection,
        held.is_some(),
        auto_aim_scene_item(actor_transform, &cameras, &scene_items, &spatial),
    );

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
        let location = mapping.locations[location_index];
        let layers = ordered_preview_at_location(actor, location, &topologies);
        let depth = if let Some(held) = held {
            properties
                .get(held)
                .ok()
                .and_then(|properties| eligible_slot_depth(&properties.id, location, &layers))
        } else {
            outermost_occupied_depth(&layers)
        };
        if let Some(depth) = depth {
            session.repeated_input = Some((mapping.input, repeat));
            session.selection = Some(GrabSelection::Slot {
                location,
                depth: depth as u16,
            });
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

fn auto_aim_scene_item(
    actor: &GlobalTransform,
    cameras: &Query<&GlobalTransform, With<Camera3d>>,
    scene_items: &Query<(Entity, &GlobalTransform, &EquipmentPhysical), With<TacticalSceneItem>>,
    spatial: &SpatialQuery,
) -> Option<Entity> {
    let camera = cameras.single().ok()?;
    let origin = actor.translation() + Vec3::Y * 0.6;
    auto_aim_candidate(
        camera.translation(),
        camera.forward().as_vec3(),
        actor.translation(),
        scene_items
            .iter()
            .filter_map(|(entity, transform, physical)| {
                let position = transform.transform_point(-physical.anchor_offset_m);
                let sight = position - origin;
                let distance = sight.length();
                let visible = distance > f32::EPSILON
                    && spatial
                        .cast_ray(
                            origin,
                            Dir3::new(sight / distance).ok()?,
                            distance,
                            true,
                            &SpatialQueryFilter::from_mask(TACTICAL_TERRAIN_LAYER),
                        )
                        .is_none();
                visible.then_some((entity, position))
            }),
    )
}

fn auto_aim_candidate(
    camera_origin: Vec3,
    camera_forward: Vec3,
    actor_position: Vec3,
    candidates: impl IntoIterator<Item = (Entity, Vec3)>,
) -> Option<Entity> {
    let camera_forward = camera_forward.try_normalize()?;
    candidates
        .into_iter()
        .filter_map(|(entity, position)| {
            let actor_distance_squared = position.distance_squared(actor_position);
            if actor_distance_squared > PICKUP_RANGE_M * PICKUP_RANGE_M {
                return None;
            }
            let camera_delta = position - camera_origin;
            let alignment = camera_delta
                .try_normalize()
                .map_or(-1.0, |direction| direction.dot(camera_forward));
            let angular_error = 1.0 - alignment.clamp(-1.0, 1.0);
            Some((entity, angular_error, actor_distance_squared))
        })
        .min_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then(left.2.total_cmp(&right.2))
                .then(left.0.to_bits().cmp(&right.0.to_bits()))
        })
        .map(|(entity, _, _)| entity)
}

fn scene_grab_selection(
    selection: Option<GrabSelection>,
    hand_occupied: bool,
    pointed: Option<Entity>,
) -> Option<GrabSelection> {
    if hand_occupied || !matches!(selection, None | Some(GrabSelection::Scene(_))) {
        return selection;
    }
    pointed.map(GrabSelection::Scene)
}

fn update_pickup_outlines(
    session: Res<GrabSession>,
    mut outlines: Query<(&PickupOutline, &mut OutlineVolume)>,
) {
    for (outline, mut volume) in &mut outlines {
        volume.visible = pickup_outline_selected(outline.0, session.selection);
    }
}

fn pickup_outline_selected(item: Entity, selection: Option<GrabSelection>) -> bool {
    matches!(selection, Some(GrabSelection::Scene(selected)) if selected == item)
}

#[derive(Clone, Copy)]
struct PreviewTarget {
    entity: Entity,
    occupied: bool,
    attached: bool,
}

fn outermost_occupied_depth(layers: &[PreviewTarget]) -> Option<usize> {
    layers.iter().position(|target| target.occupied)
}

fn eligible_slot_depth(
    held_item_id: &str,
    location: EquipmentLocation,
    layers: &[PreviewTarget],
) -> Option<usize> {
    let equipment = item_catalog::definition(held_item_id)?.equipment.as_ref()?;
    if equipment.placements.iter().any(|placement| {
        placement
            .occupancy
            .iter()
            .any(|occupancy| occupancy.location == location)
    }) {
        return Some(0);
    }
    equipment
        .placements
        .iter()
        .any(|placement| !placement.parents.is_empty())
        .then(|| layers.iter().position(|target| target.attached))?
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
    let Ok((_, _, _, properties)) = items.get(entity) else {
        output.push(PreviewTarget {
            entity,
            occupied: true,
            attached: false,
        });
        return;
    };
    let Some(equipment) = item_catalog::definition(&properties.id)
        .and_then(|definition| definition.equipment.as_ref())
    else {
        output.push(PreviewTarget {
            entity,
            occupied: true,
            attached: false,
        });
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
                    attached: true,
                });
            }
        }
    }
    output.push(PreviewTarget {
        entity,
        occupied: true,
        attached: items.get(entity).is_ok_and(|(_, _, topology, _)| {
            topology.occupancies.iter().any(|occupancy| {
                matches!(
                    occupancy.anchor,
                    TacticalEquipmentAnchor::ItemAttachment { .. }
                )
            })
        }),
    });
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
        let Ok((_, _, _, properties, _)) = items.get(entity) else {
            output.push(PreviewTarget {
                entity,
                occupied: true,
                attached: false,
            });
            return;
        };
        let Some(equipment) = item_catalog::definition(&properties.id)
            .and_then(|definition| definition.equipment.as_ref())
        else {
            output.push(PreviewTarget {
                entity,
                occupied: true,
                attached: false,
            });
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
                        attached: true,
                    });
                }
            }
        }
        output.push(PreviewTarget {
            entity,
            occupied: true,
            attached: items.get(entity).is_ok_and(|(_, _, _, _, topology)| {
                topology.occupancies.iter().any(|occupancy| {
                    matches!(
                        occupancy.anchor,
                        TacticalEquipmentAnchor::ItemAttachment { .. }
                    )
                })
            }),
        });
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
    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
    .title_bar(false)
    .resizable(false)
    .frame(egui::Frame::NONE)
    .show(context, |ui| {
        let size = egui::vec2(64.0, 48.0);
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
                            let outermost = outermost_occupied_depth(&layers)
                                .and_then(|depth| Some((depth, *layers.get(depth)?)));
                            let selection_depth = if let Some((_, _, _, held, _)) = held {
                                eligible_slot_depth(&held.id, location, &layers)
                            } else {
                                outermost.map(|(depth, _)| depth)
                            };
                            let eligible = selection_depth.is_some();
                            let selected = matches!(
                                session.selection,
                                Some(GrabSelection::Slot { location: found, .. }) if found == location
                            );
                            let fill = if selected {
                                egui::Color32::from_rgba_unmultiplied(45, 105, 170, 105)
                            } else if eligible {
                                egui::Color32::from_white_alpha(22)
                            } else {
                                egui::Color32::from_black_alpha(90)
                            };
                            let stroke = if invalid && !eligible {
                                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(190, 45, 45))
                            } else if selected {
                                egui::Stroke::new(
                                    2.0_f32,
                                    egui::Color32::from_rgb(105, 175, 255),
                                )
                            } else if eligible {
                                egui::Stroke::new(1.5_f32, egui::Color32::from_white_alpha(180))
                            } else {
                                egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(28))
                            };
                            let frame = egui::Frame::NONE
                                .fill(fill)
                                .stroke(stroke)
                                .corner_radius(5.0)
                                .inner_margin(4.0)
                                .show(ui, |ui| {
                                    ui.set_min_size(size);
                                    ui.label(
                                        egui::RichText::new(mapping.input.to_uppercase())
                                            .small()
                                            .strong()
                                            .color(if eligible {
                                                egui::Color32::WHITE
                                            } else {
                                                egui::Color32::from_white_alpha(75)
                                            }),
                                    );
                                    let Some((_, layer)) = outermost else {
                                        return;
                                    };
                                    let Ok((_, _, _, item, topology)) = items.get(layer.entity)
                                    else {
                                        return;
                                    };
                                    let icon = item_catalog::definition(&item.id)
                                        .map(|definition| definition.presentation.icon.as_str())
                                        .unwrap_or("help");
                                    let response = ui.add(
                                        egui::Image::new((
                                            atlas_texture,
                                            egui::vec2(27.0, 27.0),
                                        ))
                                        .uv(icon_uv(icon))
                                        .tint(if eligible {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::from_white_alpha(75)
                                        }),
                                    );
                                    let multi = topology
                                        .occupancies
                                        .iter()
                                        .filter(|occupancy| {
                                            matches!(
                                                occupancy.anchor,
                                                TacticalEquipmentAnchor::CharacterLocation(_)
                                            )
                                        })
                                        .count()
                                        > 1;
                                    if multi {
                                        joined_cells.push((response.rect, layer.entity));
                                    }
                                });
                            let tooltip = if let Some((_, _, _, held, _)) = held {
                                if eligible {
                                    format!(
                                        "{}: place or swap {}",
                                        mapping.input.to_uppercase(),
                                        held.id
                                    )
                                } else {
                                    format!(
                                        "{}: {} cannot be placed here",
                                        mapping.input.to_uppercase(),
                                        held.id
                                    )
                                }
                            } else if let Some((_, layer)) = outermost {
                                if let Ok((_, _, _, item, _)) = items.get(layer.entity) {
                                    format!(
                                        "{}: draw {}",
                                        mapping.input.to_uppercase(),
                                        item.id
                                    )
                                } else {
                                    mapping.input.to_uppercase()
                                }
                            } else {
                                format!("{}: empty", mapping.input.to_uppercase())
                            };
                            let response = ui
                                .interact(
                                    frame.response.rect,
                                    egui::Id::new(("tactical-slot", mapping.input)),
                                    egui::Sense::click(),
                                )
                                .on_hover_text(tooltip);
                            if response.clicked()
                                && let Some(depth) = selection_depth
                            {
                                session.selection = Some(GrabSelection::Slot {
                                    location,
                                    depth: depth as u16,
                                });
                            }
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
        ui.horizontal(|ui| {
            let active_icon = held
                .and_then(|(_, _, _, item, _)| item_catalog::definition(&item.id))
                .map_or("mailed-fist", |definition| {
                    definition.presentation.icon.as_str()
                });
            ui.add(
                egui::Image::new((atlas_texture, egui::vec2(28.0, 28.0)))
                    .uv(icon_uv(active_icon)),
            )
            .on_hover_text(match hand {
                EquipmentHand::Left => "Active left hand",
                EquipmentHand::Right => "Active right hand",
            });

            let other = match hand {
                EquipmentHand::Left => EquipmentHand::Right,
                EquipmentHand::Right => EquipmentHand::Left,
            };
            let other_icon = items
                .iter()
                .find(|(_, owner, slot, _, _)| {
                    owner.0 == actor && slot.is_some_and(|slot| *slot == other.slot())
                })
                .and_then(|(_, _, _, item, _)| item_catalog::definition(&item.id))
                .map_or("mailed-fist", |definition| {
                    definition.presentation.icon.as_str()
                });
            let other_button = egui::Button::image(
                egui::Image::new((atlas_texture, egui::vec2(24.0, 24.0)))
                    .uv(icon_uv(other_icon)),
            )
            .min_size(egui::vec2(32.0, 32.0))
            .fill(egui::Color32::TRANSPARENT);
            if ui
                .add(other_button)
                .on_hover_text("Move or swap with the other hand")
                .clicked()
            {
                session.selection = Some(GrabSelection::Hand(other));
            }

            if held.is_none()
                && let Some(GrabSelection::Scene(entity)) = session.selection
                && let Ok((_, item)) = scene_items.get(entity)
            {
                let icon = item_catalog::definition(&item.id)
                    .map(|definition| definition.presentation.icon.as_str())
                    .unwrap_or("help");
                ui.add(
                    egui::Image::new((atlas_texture, egui::vec2(24.0, 24.0)))
                        .uv(icon_uv(icon)),
                )
                .on_hover_text(format!("Release to pick up {}", item.id));
            }
        });
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
                // Attachment is resolved on the following update. Keeping the
                // root hidden avoids a one-frame flash at the world origin.
                Visibility::Hidden,
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
            Transform::from_translation(-physical.anchor_offset_m),
            PickupOutline(item),
            OutlineVolume {
                visible: false,
                colour: Color::WHITE,
                width: 4.0,
            },
            OutlineMode::FloodFlat,
        ));
    }
}

fn update_item_placeholders(
    mut commands: Commands,
    items: Query<
        (
            &Transform,
            Option<&ItemOf>,
            Option<&EquipSlot>,
            &EquipmentTopology,
            Has<TacticalSceneItem>,
        ),
        Without<ItemPlaceholder>,
    >,
    topologies: Query<&EquipmentTopology, Without<ItemPlaceholder>>,
    rigs: Query<&HumanoidRig, With<Player>>,
    mut placeholders: Query<(
        Entity,
        &ItemPlaceholder,
        &mut Transform,
        &mut Visibility,
        Option<&ChildOf>,
    )>,
) {
    for (entity, placeholder, mut transform, mut visibility, parent) in &mut placeholders {
        let Ok((item_transform, owner, slot, topology, scene)) = items.get(placeholder.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        if scene {
            if parent.is_some() {
                commands.entity(entity).remove::<ChildOf>();
            }
            *transform = *item_transform;
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        } else if let (Some(owner), Some(primary_hand)) = (owner, holding_side(slot)) {
            if parent.is_some() {
                commands.entity(entity).remove::<ChildOf>();
            }
            *visibility = Visibility::Inherited;
            commands.entity(entity).insert(HeldWeaponConstraint {
                owner: owner.0,
                primary_hand,
                secondary_grip_local: None,
            });
        } else if let Some(bone) = owner.and_then(|owner| {
            let rig = rigs.get(owner.0).ok()?;
            let role = equipment_location_bone(resolve_character_location(topology, &topologies)?);
            rig.get(&role).copied()
        }) {
            if parent.is_none_or(|parent| parent.parent() != bone) {
                commands.entity(entity).insert(ChildOf(bone));
            }
            *transform = Transform::IDENTITY;
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        } else {
            *visibility = Visibility::Hidden;
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        }
    }
}

fn resolve_character_location(
    topology: &EquipmentTopology,
    topologies: &Query<&EquipmentTopology, Without<ItemPlaceholder>>,
) -> Option<EquipmentLocation> {
    let mut topology = topology;
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..32 {
        if let Some(location) =
            topology
                .occupancies
                .iter()
                .find_map(|occupancy| match occupancy.anchor {
                    TacticalEquipmentAnchor::CharacterLocation(location) => Some(location),
                    TacticalEquipmentAnchor::ItemAttachment { .. } => None,
                })
        {
            return Some(location);
        }
        let parent = topology
            .occupancies
            .iter()
            .find_map(|occupancy| match occupancy.anchor {
                TacticalEquipmentAnchor::ItemAttachment { parent, .. } => Some(parent),
                TacticalEquipmentAnchor::CharacterLocation(_) => None,
            })?;
        if !visited.insert(parent) {
            return None;
        }
        topology = topologies.get(parent).ok()?;
    }
    None
}

fn equipment_location_bone(location: EquipmentLocation) -> BoneRole {
    match location {
        EquipmentLocation::Head | EquipmentLocation::Face => BoneRole::Head,
        EquipmentLocation::Neck => BoneRole::NeckTwo,
        EquipmentLocation::Chest | EquipmentLocation::Back => BoneRole::Chest,
        EquipmentLocation::Stomach => BoneRole::StomachTwo,
        EquipmentLocation::LeftShoulder => BoneRole::ClavicleLeft,
        EquipmentLocation::RightShoulder => BoneRole::ClavicleRight,
        EquipmentLocation::LeftArm => BoneRole::UpperArmLeft,
        EquipmentLocation::RightArm => BoneRole::UpperArmRight,
        EquipmentLocation::LeftHand => BoneRole::HandLeft,
        EquipmentLocation::RightHand => BoneRole::HandRight,
        EquipmentLocation::LeftLeg => BoneRole::ThighLeft,
        EquipmentLocation::RightLeg => BoneRole::ThighRight,
        EquipmentLocation::LeftFoot => BoneRole::FootLeft,
        EquipmentLocation::RightFoot => BoneRole::FootRight,
        EquipmentLocation::LeftBelt
        | EquipmentLocation::RightBelt
        | EquipmentLocation::FrontBelt
        | EquipmentLocation::BackBelt
        | EquipmentLocation::BackLeftPocket
        | EquipmentLocation::BackRightPocket => BoneRole::Pelvis,
        EquipmentLocation::LeftPocket => BoneRole::ThighLeft,
        EquipmentLocation::RightPocket => BoneRole::ThighRight,
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
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn pickup_outline_is_visible_only_for_the_selected_scene_item() {
        let selected = Entity::from_bits(1);
        let other = Entity::from_bits(2);

        assert!(pickup_outline_selected(
            selected,
            Some(GrabSelection::Scene(selected))
        ));
        assert!(!pickup_outline_selected(
            other,
            Some(GrabSelection::Scene(selected))
        ));
        assert!(!pickup_outline_selected(selected, None));
    }

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
    fn outermost_display_skips_empty_attachment_capacity() {
        let parent = Entity::from_bits(1);
        let child = Entity::from_bits(2);
        let layers = [
            PreviewTarget {
                entity: parent,
                occupied: false,
                attached: true,
            },
            PreviewTarget {
                entity: child,
                occupied: true,
                attached: true,
            },
            PreviewTarget {
                entity: parent,
                occupied: true,
                attached: false,
            },
        ];
        assert_eq!(outermost_occupied_depth(&layers), Some(1));
    }

    #[test]
    fn empty_hand_tracks_the_pointed_scene_item_for_release() {
        let first = Entity::from_bits(21);
        let second = Entity::from_bits(22);
        let selection = scene_grab_selection(None, false, Some(first));
        assert_eq!(selection, Some(GrabSelection::Scene(first)));
        assert_eq!(
            scene_grab_selection(selection, false, Some(second)),
            Some(GrabSelection::Scene(second))
        );
        assert_eq!(scene_grab_selection(selection, false, None), None);
    }

    #[test]
    fn pointed_scene_item_does_not_override_an_explicit_slot_selection() {
        let selection = Some(GrabSelection::Slot {
            location: EquipmentLocation::LeftBelt,
            depth: 0,
        });
        assert_eq!(
            scene_grab_selection(selection, false, Some(Entity::from_bits(23))),
            selection
        );
    }

    #[test]
    fn auto_aim_prefers_cursor_alignment_over_character_distance() {
        let pointed = Entity::from_bits(31);
        let nearby_side = Entity::from_bits(32);
        assert_eq!(
            auto_aim_candidate(
                Vec3::Y,
                Vec3::NEG_Z,
                Vec3::ZERO,
                [
                    (nearby_side, Vec3::new(0.5, 0.0, 0.0)),
                    (pointed, Vec3::new(0.0, 0.0, -1.8)),
                ],
            ),
            Some(pointed)
        );
    }

    #[test]
    fn auto_aim_has_no_cursor_cone_and_falls_back_to_an_item_behind() {
        let behind = Entity::from_bits(33);
        assert_eq!(
            auto_aim_candidate(
                Vec3::Y,
                Vec3::NEG_Z,
                Vec3::ZERO,
                [(behind, Vec3::new(0.0, 0.0, 1.5))],
            ),
            Some(behind)
        );
    }

    #[test]
    fn auto_aim_excludes_items_outside_character_pickup_range() {
        assert_eq!(
            auto_aim_candidate(
                Vec3::Y,
                Vec3::NEG_Z,
                Vec3::ZERO,
                [(Entity::from_bits(34), Vec3::new(0.0, 0.0, -2.01))],
            ),
            None
        );
    }

    #[test]
    fn preview_traversal_addresses_deepest_attachment_before_parent() {
        let mut world = World::new();
        let actor = world.spawn_empty().id();
        let belt = world
            .spawn((
                ItemOf(actor),
                ItemProperties {
                    id: "leather_belt".into(),
                    weight: 0.35,
                },
                EquipmentTopology {
                    placement_id: Some("worn".into()),
                    occupancies: vec![EquipmentTopologyOccupancy {
                        occupancy_id: "belt".into(),
                        anchor: TacticalEquipmentAnchor::CharacterLocation(
                            EquipmentLocation::LeftBelt,
                        ),
                        channel: EquipmentChannel::Accessory,
                        order: 0,
                        requirement_index: 0,
                        capacity_index: 0,
                    }],
                },
            ))
            .id();
        let sheath = world
            .spawn((
                ItemOf(actor),
                ItemProperties {
                    id: "sword_sheath".into(),
                    weight: 0.3,
                },
                EquipmentTopology {
                    placement_id: Some("attached".into()),
                    occupancies: vec![EquipmentTopologyOccupancy {
                        occupancy_id: "sheath".into(),
                        anchor: TacticalEquipmentAnchor::ItemAttachment {
                            parent: belt,
                            attachment_point_id: "left".into(),
                        },
                        channel: EquipmentChannel::Mount,
                        order: 0,
                        requirement_index: 0,
                        capacity_index: 0,
                    }],
                },
            ))
            .id();
        let preview = world
            .run_system_once(
                move |items: Query<(Entity, &ItemOf, &EquipmentTopology, &ItemProperties)>| {
                    ordered_preview_at_location(actor, EquipmentLocation::LeftBelt, &items)
                },
            )
            .unwrap();
        assert_eq!(preview.first().map(|target| target.entity), Some(sheath));
        assert!(
            !preview.first().unwrap().occupied,
            "deepest empty blade is first"
        );
        assert_eq!(preview.last().map(|target| target.entity), Some(belt));
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
    fn every_equipment_location_has_a_semantic_bone() {
        for (location, expected) in [
            (EquipmentLocation::Head, BoneRole::Head),
            (EquipmentLocation::Face, BoneRole::Head),
            (EquipmentLocation::Neck, BoneRole::NeckTwo),
            (EquipmentLocation::Chest, BoneRole::Chest),
            (EquipmentLocation::Stomach, BoneRole::StomachTwo),
            (EquipmentLocation::Back, BoneRole::Chest),
            (EquipmentLocation::LeftShoulder, BoneRole::ClavicleLeft),
            (EquipmentLocation::RightShoulder, BoneRole::ClavicleRight),
            (EquipmentLocation::LeftArm, BoneRole::UpperArmLeft),
            (EquipmentLocation::RightArm, BoneRole::UpperArmRight),
            (EquipmentLocation::LeftHand, BoneRole::HandLeft),
            (EquipmentLocation::RightHand, BoneRole::HandRight),
            (EquipmentLocation::LeftLeg, BoneRole::ThighLeft),
            (EquipmentLocation::RightLeg, BoneRole::ThighRight),
            (EquipmentLocation::LeftFoot, BoneRole::FootLeft),
            (EquipmentLocation::RightFoot, BoneRole::FootRight),
            (EquipmentLocation::LeftBelt, BoneRole::Pelvis),
            (EquipmentLocation::RightBelt, BoneRole::Pelvis),
            (EquipmentLocation::FrontBelt, BoneRole::Pelvis),
            (EquipmentLocation::BackBelt, BoneRole::Pelvis),
            (EquipmentLocation::LeftPocket, BoneRole::ThighLeft),
            (EquipmentLocation::RightPocket, BoneRole::ThighRight),
            (EquipmentLocation::BackLeftPocket, BoneRole::Pelvis),
            (EquipmentLocation::BackRightPocket, BoneRole::Pelvis),
        ] {
            assert_eq!(equipment_location_bone(location), expected, "{location:?}");
        }
    }

    #[test]
    fn attached_items_resolve_the_body_bone_through_their_parent_chain() {
        fn occupancy(anchor: TacticalEquipmentAnchor) -> EquipmentTopologyOccupancy {
            EquipmentTopologyOccupancy {
                occupancy_id: String::new(),
                anchor,
                channel: EquipmentChannel::Containment,
                order: 0,
                requirement_index: 0,
                capacity_index: 0,
            }
        }

        let mut world = World::new();
        let belt = world
            .spawn(EquipmentTopology {
                placement_id: Some("worn".into()),
                occupancies: vec![occupancy(TacticalEquipmentAnchor::CharacterLocation(
                    EquipmentLocation::LeftBelt,
                ))],
            })
            .id();
        let sheath = world
            .spawn(EquipmentTopology {
                placement_id: Some("attached".into()),
                occupancies: vec![occupancy(TacticalEquipmentAnchor::ItemAttachment {
                    parent: belt,
                    attachment_point_id: "mount".into(),
                })],
            })
            .id();
        let weapon = world
            .spawn(EquipmentTopology {
                placement_id: Some("contained".into()),
                occupancies: vec![occupancy(TacticalEquipmentAnchor::ItemAttachment {
                    parent: sheath,
                    attachment_point_id: "blade".into(),
                })],
            })
            .id();

        let location = world
            .run_system_once(
                move |topologies: Query<&EquipmentTopology, Without<ItemPlaceholder>>| {
                    resolve_character_location(topologies.get(weapon).unwrap(), &topologies)
                },
            )
            .unwrap();
        assert_eq!(location, Some(EquipmentLocation::LeftBelt));
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
