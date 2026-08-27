//! Tactical grab input, QWERTY slot HUD, and placeholder item presentation.

use std::{
    collections::{BTreeMap, HashMap},
    sync::LazyLock,
};

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    prelude::{EquipmentAction, EquipmentActionRequest, EquipmentHand},
};
use adventuresim_weapon_model::{
    ICON_RENDERER_VERSION, MaterialClass, WeaponIconSpec, decode, generate, generate_holder_icon,
    generate_icon,
};
use bevy::{
    asset::{LoadState, RenderAssetUsages},
    camera::visibility::NoFrustumCulling,
    gltf::{Gltf, GltfAssetLabel, GltfMesh, GltfNode, GltfSkin},
    mesh::{
        Indices, PrimitiveTopology,
        skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    },
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, EguiTextureHandle, egui};
use bevy_mod_outline::{OutlineMode, OutlinePlugin, OutlineVolume};
use serde::Deserialize;

use crate::{
    animation::{
        AuthoredBindTransform, BoneRole, HandSide, HeldWeaponConstraint, HumanoidRig, MhrBone,
        authored_bind_global,
    },
    player::ClientPlayer,
    presentation::TacticalGameplayCamera,
    targeting::auto_aim_candidate,
};

const PICKUP_RANGE_M: f32 = 2.0;
const INVALID_FLASH_SECS: f32 = 0.18;
const TACTICAL_WEAPON_ICON_SIZE: u16 = 64;
const TACTICAL_WEAPON_ICON_SUPERSAMPLING: u8 = 4;
const EQUIPMENT_SOCKET_NODE_PREFIX: &str = "equipment_socket_";
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
            .init_resource::<WeaponMeshCache>()
            .init_resource::<WeaponIconCache>()
            .add_systems(
                PreUpdate,
                update_grab_input.after(bevy::input::InputSystems),
            )
            .add_systems(
                Update,
                (
                    spawn_item_placeholders,
                    request_procedural_equipment_models,
                    resolve_procedural_equipment_models,
                    sync_procedural_equipment_skins,
                    update_item_placeholders,
                    update_pickup_outlines,
                )
                    .chain(),
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

#[derive(Clone)]
struct CachedWeaponPart {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Clone)]
struct CachedWeapon {
    parts: Vec<CachedWeaponPart>,
    grip: Vec3,
}

#[derive(Resource, Default)]
struct WeaponMeshCache {
    weapons: HashMap<(u16, [u8; 32]), CachedWeapon>,
    holders: HashMap<(u16, [u8; 32]), CachedWeapon>,
    materials: HashMap<MaterialClass, Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
struct WeaponIconCache {
    icons: HashMap<(IconSource, u16, u16, [u8; 32], u16, u8), Handle<Image>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum IconSource {
    Weapon,
    Holder,
}

#[derive(Component)]
struct ItemPlaceholder(Entity);

#[derive(Component)]
struct PickupOutline(Entity);

#[derive(Deserialize)]
struct ProceduralEquipmentManifest {
    assets: Vec<ProceduralEquipmentManifestAsset>,
}

#[derive(Deserialize)]
struct ProceduralEquipmentManifestAsset {
    item_id: String,
    placement_id: String,
    file: String,
}

static PROCEDURAL_EQUIPMENT_ASSETS: LazyLock<BTreeMap<String, BTreeMap<String, String>>> =
    LazyLock::new(|| {
        let manifest: ProceduralEquipmentManifest = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/equipment/procedural/manifest.json"
        )))
        .expect("committed procedural equipment manifest must be valid");
        let mut assets = BTreeMap::<String, BTreeMap<String, String>>::new();
        for asset in manifest.assets {
            let replaced = assets
                .entry(asset.item_id)
                .or_default()
                .insert(asset.placement_id, asset.file);
            assert!(
                replaced.is_none(),
                "procedural equipment manifest keys must be unique"
            );
        }
        assets
    });

fn procedural_equipment_file(item_id: &str, placement_id: Option<&str>) -> Option<&'static str> {
    let placements = PROCEDURAL_EQUIPMENT_ASSETS.get(item_id)?;
    match placement_id {
        Some(placement_id) => placements.get(placement_id),
        None => placements.values().next(),
    }
    .map(String::as_str)
}

fn procedural_equipment_asset_path(file: &str) -> String {
    let path = format!("equipment/procedural/{file}");
    #[cfg(not(target_family = "wasm"))]
    {
        format!("workspace://{path}")
    }
    #[cfg(target_family = "wasm")]
    {
        path
    }
}

#[derive(Component)]
struct ProceduralEquipmentPresentation {
    asset_path: String,
}

#[derive(Component)]
struct ProceduralEquipmentRequest(Handle<Gltf>);

#[derive(Component)]
struct ProceduralEquipmentResolved;

#[derive(Component)]
struct ProceduralEquipmentFailed;

#[derive(Component)]
struct ItemFallback(Entity);

#[derive(Component)]
struct ProceduralEquipmentPart {
    item: Entity,
    inverse_bindposes: Handle<SkinnedMeshInverseBindposes>,
    joint_names: Vec<String>,
}

#[derive(Component, Default)]
struct EquipmentAttachmentSockets(BTreeMap<String, Transform>);

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
    cameras: Query<&GlobalTransform, With<TacticalGameplayCamera>>,
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
    cameras: &Query<&GlobalTransform, With<TacticalGameplayCamera>>,
    scene_items: &Query<(Entity, &GlobalTransform, &EquipmentPhysical), With<TacticalSceneItem>>,
    spatial: &SpatialQuery,
) -> Option<Entity> {
    let camera = cameras.single().ok()?;
    let origin = actor.translation() + Vec3::Y * 0.6;
    auto_aim_candidate(
        camera.translation(),
        camera.forward().as_vec3(),
        actor.translation(),
        PICKUP_RANGE_M,
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
                visible.then_some((entity, position, entity.to_bits()))
            }),
    )
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
        append_preview(entity, location, topologies, &mut visited, &mut output);
    }
    output
}

fn append_preview(
    entity: Entity,
    location: EquipmentLocation,
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
    for point in points
        .into_iter()
        .filter(|point| point.locations.is_empty() || point.locations.contains(&location))
    {
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
                append_preview(child, location, items, visited, output);
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
        location: EquipmentLocation,
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
        for point in points
            .into_iter()
            .filter(|point| point.locations.is_empty() || point.locations.contains(&location))
        {
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
                    append(child, location, items, visited, output);
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
        append(root, location, items, &mut visited, &mut output);
    }
    output
}

fn cached_weapon_icon(
    appearance: &WeaponAppearance,
    cache: &mut WeaponIconCache,
    images: &mut Assets<Image>,
) -> Option<Handle<Image>> {
    if appearance.recipe.len() > 16 * 1024
        || appearance.generator_version != adventuresim_weapon_model::GENERATOR_VERSION
    {
        return None;
    }
    let design = decode(&appearance.recipe).ok()?;
    if adventuresim_weapon_model::design_hash(&design).0 != appearance.design_hash {
        return None;
    }
    let key = (
        IconSource::Weapon,
        appearance.generator_version,
        ICON_RENDERER_VERSION,
        appearance.design_hash,
        TACTICAL_WEAPON_ICON_SIZE,
        TACTICAL_WEAPON_ICON_SUPERSAMPLING,
    );
    if let Some(cached) = cache.icons.get(&key) {
        return Some(cached.clone());
    }
    let icon = generate_icon(
        &design,
        WeaponIconSpec {
            size: TACTICAL_WEAPON_ICON_SIZE,
            supersampling: TACTICAL_WEAPON_ICON_SUPERSAMPLING,
        },
    )
    .ok()?;
    let rgba = icon
        .alpha
        .into_iter()
        .flat_map(|alpha| [255, 255, 255, alpha])
        .collect();
    let handle = images.add(Image::new(
        Extent3d {
            width: u32::from(TACTICAL_WEAPON_ICON_SIZE),
            height: u32::from(TACTICAL_WEAPON_ICON_SIZE),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    cache.icons.insert(key, handle.clone());
    Some(handle)
}

fn cached_holder_icon(
    appearance: &WeaponHolderAppearance,
    cache: &mut WeaponIconCache,
    images: &mut Assets<Image>,
) -> Option<Handle<Image>> {
    if appearance.recipe.len() > 16 * 1024
        || appearance.generator_version != adventuresim_weapon_model::HOLDER_GENERATOR_VERSION
    {
        return None;
    }
    let design = adventuresim_weapon_model::decode_holder(&appearance.recipe).ok()?;
    if adventuresim_weapon_model::holder_design_hash(&design).0 != appearance.design_hash {
        return None;
    }
    let key = (
        IconSource::Holder,
        appearance.generator_version,
        ICON_RENDERER_VERSION,
        appearance.design_hash,
        TACTICAL_WEAPON_ICON_SIZE,
        TACTICAL_WEAPON_ICON_SUPERSAMPLING,
    );
    if let Some(cached) = cache.icons.get(&key) {
        return Some(cached.clone());
    }
    let icon = generate_holder_icon(
        &design,
        WeaponIconSpec {
            size: TACTICAL_WEAPON_ICON_SIZE,
            supersampling: TACTICAL_WEAPON_ICON_SUPERSAMPLING,
        },
    )
    .ok()?;
    let rgba = icon
        .alpha
        .into_iter()
        .flat_map(|alpha| [255, 255, 255, alpha])
        .collect();
    let handle = images.add(Image::new(
        Extent3d {
            width: u32::from(TACTICAL_WEAPON_ICON_SIZE),
            height: u32::from(TACTICAL_WEAPON_ICON_SIZE),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    cache.icons.insert(key, handle.clone());
    Some(handle)
}

fn equipment_icon_image(
    entity: Option<Entity>,
    fallback_slug: &str,
    size: egui::Vec2,
    procedural: &HashMap<Entity, egui::TextureId>,
    atlas: egui::TextureId,
) -> egui::Image<'static> {
    if let Some(texture) = entity.and_then(|entity| procedural.get(&entity)).copied() {
        egui::Image::new((texture, size))
    } else {
        egui::Image::new((atlas, size)).uv(icon_uv(fallback_slug))
    }
}

fn draw_slot_hud(
    mut contexts: EguiContexts,
    asset_server: Res<AssetServer>,
    mut icon_atlas: Local<Option<Handle<Image>>>,
    mut weapon_icon_cache: ResMut<WeaponIconCache>,
    mut images: ResMut<Assets<Image>>,
    player: Single<Entity, With<ClientPlayer>>,
    items: Query<(
        Entity,
        &ItemOf,
        Option<&EquipSlot>,
        &ItemProperties,
        &EquipmentTopology,
    )>,
    scene_items: Query<(Entity, &ItemProperties), With<TacticalSceneItem>>,
    weapon_appearances: Query<(Entity, &WeaponAppearance)>,
    holder_appearances: Query<(Entity, &WeaponHolderAppearance)>,
    mut session: ResMut<GrabSession>,
) {
    let Some(hand) = session.active else { return };
    let actor = *player;
    let held = items.iter().find(|(_, owner, slot, _, _)| {
        owner.0 == actor && slot.is_some_and(|slot| *slot == hand.slot())
    });
    let atlas = icon_atlas.get_or_insert_with(|| asset_server.load("tactical-equipment-icons.png"));
    let atlas_texture = contexts.add_image(EguiTextureHandle::Weak(atlas.id()));
    let mut procedural_textures = weapon_appearances
        .iter()
        .filter_map(|(entity, appearance)| {
            let handle = cached_weapon_icon(appearance, &mut weapon_icon_cache, &mut images)?;
            let texture = contexts.add_image(EguiTextureHandle::Weak(handle.id()));
            Some((entity, texture))
        })
        .collect::<HashMap<_, _>>();
    procedural_textures.extend(
        holder_appearances
            .iter()
            .filter_map(|(entity, appearance)| {
                let handle = cached_holder_icon(appearance, &mut weapon_icon_cache, &mut images)?;
                let texture = contexts.add_image(EguiTextureHandle::Weak(handle.id()));
                Some((entity, texture))
            }),
    );
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
                                        equipment_icon_image(
                                            Some(layer.entity),
                                            icon,
                                            egui::vec2(27.0, 27.0),
                                            &procedural_textures,
                                            atlas_texture,
                                        )
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
            let active_entity = held.map(|(entity, _, _, _, _)| entity);
            let active_icon = held
                .and_then(|(_, _, _, item, _)| item_catalog::definition(&item.id))
                .map_or("mailed-fist", |definition| {
                    definition.presentation.icon.as_str()
                });
            ui.add(
                equipment_icon_image(
                    active_entity,
                    active_icon,
                    egui::vec2(28.0, 28.0),
                    &procedural_textures,
                    atlas_texture,
                ),
            )
            .on_hover_text(match hand {
                EquipmentHand::Left => "Active left hand",
                EquipmentHand::Right => "Active right hand",
            });

            let other = match hand {
                EquipmentHand::Left => EquipmentHand::Right,
                EquipmentHand::Right => EquipmentHand::Left,
            };
            let other_item = items
                .iter()
                .find(|(_, owner, slot, _, _)| {
                    owner.0 == actor && slot.is_some_and(|slot| *slot == other.slot())
                });
            let other_entity = other_item.map(|(entity, _, _, _, _)| entity);
            let other_icon = other_item
                .and_then(|(_, _, _, item, _)| item_catalog::definition(&item.id))
                .map_or("mailed-fist", |definition| definition.presentation.icon.as_str());
            let other_button = egui::Button::image(
                equipment_icon_image(
                    other_entity,
                    other_icon,
                    egui::vec2(24.0, 24.0),
                    &procedural_textures,
                    atlas_texture,
                ),
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
                ui.add(equipment_icon_image(
                    Some(entity),
                    icon,
                    egui::vec2(24.0, 24.0),
                    &procedural_textures,
                    atlas_texture,
                ))
                .on_hover_text(format!("Release to pick up {}", item.id));
            }
        });
    });
}

fn weapon_material(
    class: MaterialClass,
    cache: &mut WeaponMeshCache,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    cache
        .materials
        .entry(class)
        .or_insert_with(|| {
            let base_color = match class {
                MaterialClass::Wood => Color::srgb(0.30, 0.18, 0.09),
                MaterialClass::Leather => Color::srgb(0.16, 0.09, 0.05),
                MaterialClass::DarkLeather => Color::srgb(0.055, 0.045, 0.038),
                MaterialClass::Brass => Color::srgb(0.58, 0.42, 0.13),
                MaterialClass::Steel => Color::srgb(0.55, 0.58, 0.60),
                MaterialClass::DarkSteel => Color::srgb(0.22, 0.24, 0.26),
            };
            materials.add(StandardMaterial {
                base_color,
                metallic: if matches!(
                    class,
                    MaterialClass::Brass | MaterialClass::Steel | MaterialClass::DarkSteel
                ) {
                    0.82
                } else {
                    0.0
                },
                perceptual_roughness: if matches!(
                    class,
                    MaterialClass::Brass | MaterialClass::Steel | MaterialClass::DarkSteel
                ) {
                    0.34
                } else {
                    0.76
                },
                ..default()
            })
        })
        .clone()
}

fn cached_weapon(
    appearance: &WeaponAppearance,
    cache: &mut WeaponMeshCache,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Option<CachedWeapon> {
    if appearance.recipe.len() > 16 * 1024 {
        return None;
    }
    if appearance.generator_version != adventuresim_weapon_model::GENERATOR_VERSION {
        return None;
    }
    let design = decode(&appearance.recipe).ok()?;
    if adventuresim_weapon_model::design_hash(&design).0 != appearance.design_hash {
        return None;
    }
    let key = (appearance.generator_version, appearance.design_hash);
    if let Some(cached) = cache.weapons.get(&key) {
        return Some(cached.clone());
    }
    let generated = generate(&design).ok()?;
    let grip = Vec3::from_array(
        generated
            .anchors
            .iter()
            .find(|anchor| anchor.name == "weapon.grip")?
            .position,
    );
    let parts = generated
        .parts
        .into_iter()
        .map(|part| {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, part.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals);
            mesh.insert_indices(Indices::U32(part.indices));
            CachedWeaponPart {
                mesh: meshes.add(mesh),
                material: weapon_material(part.material, cache, materials),
            }
        })
        .collect();
    let cached = CachedWeapon { parts, grip };
    cache.weapons.insert(key, cached.clone());
    Some(cached)
}

fn cached_holder(
    appearance: &WeaponHolderAppearance,
    cache: &mut WeaponMeshCache,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Option<CachedWeapon> {
    if appearance.recipe.len() > 16 * 1024
        || appearance.generator_version != adventuresim_weapon_model::HOLDER_GENERATOR_VERSION
    {
        return None;
    }
    let design = adventuresim_weapon_model::decode_holder(&appearance.recipe).ok()?;
    if adventuresim_weapon_model::holder_design_hash(&design).0 != appearance.design_hash {
        return None;
    }
    let key = (appearance.generator_version, appearance.design_hash);
    if let Some(cached) = cache.holders.get(&key) {
        return Some(cached.clone());
    }
    let generated = adventuresim_weapon_model::generate_holder(&design).ok()?;
    let parts = generated
        .parts
        .into_iter()
        .map(|part| {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, part.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals);
            mesh.insert_indices(Indices::U32(part.indices));
            CachedWeaponPart {
                mesh: meshes.add(mesh),
                material: weapon_material(part.material, cache, materials),
            }
        })
        .collect();
    let cached = CachedWeapon {
        parts,
        grip: Vec3::from_array(generated.grip),
    };
    cache.holders.insert(key, cached.clone());
    Some(cached)
}

fn spawn_item_placeholders(
    mut commands: Commands,
    added: Query<
        (
            Entity,
            &EquipmentPhysical,
            Option<&EquipmentTopology>,
            Option<&ItemProperties>,
            Option<&WeaponAppearance>,
            Option<&WeaponHolderAppearance>,
        ),
        Or<(
            Added<EquipmentPhysical>,
            Added<EquipmentTopology>,
            Changed<EquipmentTopology>,
            Added<ItemProperties>,
            Changed<ItemProperties>,
            Added<WeaponAppearance>,
            Changed<WeaponAppearance>,
            Added<WeaponHolderAppearance>,
            Changed<WeaponHolderAppearance>,
        )>,
    >,
    existing: Query<(Entity, &ItemPlaceholder)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<WeaponMeshCache>,
) {
    for (item, physical, topology, properties, appearance, holder_appearance) in &added {
        if !physical.is_valid() {
            continue;
        }
        for (root, placeholder) in &existing {
            if placeholder.0 == item {
                commands.entity(root).despawn();
            }
        }
        commands.entity(item).remove::<EquipmentAttachmentSockets>();
        let mut root_commands = commands.spawn((
            Name::new("Tactical item placeholder"),
            ItemPlaceholder(item),
            Transform::default(),
            // Attachment is resolved on the following update. Keeping the
            // root hidden avoids a one-frame flash at the world origin.
            Visibility::Hidden,
        ));
        if let Some(file) = properties.and_then(|properties| {
            procedural_equipment_file(
                &properties.id,
                topology.and_then(|topology| topology.placement_id.as_deref()),
            )
        }) {
            root_commands.insert(ProceduralEquipmentPresentation {
                asset_path: procedural_equipment_asset_path(file),
            });
        }
        let root = root_commands.id();
        let (generated, part_name) = if let Some(holder) =
            holder_appearance.and_then(|appearance| {
                cached_holder(appearance, &mut cache, &mut meshes, &mut materials)
            }) {
            (Some(holder), "Procedural weapon holder part")
        } else if properties.is_some_and(|properties| {
            matches!(
                properties.id.as_str(),
                "scabbard" | "sword_sheath" | "boot_sheath" | "forearm_holster" | "weapon_loop"
            )
        }) {
            // Holder catalog rows are semantic chassis, not renderable generic
            // boxes. An absent, corrupt, or unsupported holder instance stays
            // visually absent until a valid first-class recipe arrives.
            continue;
        } else {
            (
                appearance.and_then(|appearance| {
                    cached_weapon(appearance, &mut cache, &mut meshes, &mut materials)
                }),
                "Procedural weapon part",
            )
        };
        if let Some(generated) = generated {
            commands.entity(root).with_children(|parent| {
                for part in generated.parts {
                    parent.spawn((
                        Name::new(part_name),
                        Mesh3d(part.mesh),
                        MeshMaterial3d(part.material),
                        Transform::from_translation(-generated.grip),
                        PickupOutline(item),
                        OutlineVolume {
                            visible: false,
                            colour: Color::WHITE,
                            width: 4.0,
                        },
                        OutlineMode::FloodFlat,
                    ));
                }
            });
        } else {
            commands.entity(root).with_child((
                Name::new("Tactical item fallback"),
                ItemFallback(item),
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
}

fn request_procedural_equipment_models(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pending: Query<(Entity, &ProceduralEquipmentPresentation), Without<ProceduralEquipmentRequest>>,
) {
    for (entity, presentation) in &pending {
        commands.entity(entity).insert(ProceduralEquipmentRequest(
            asset_server.load(&presentation.asset_path),
        ));
    }
}

fn mark_procedural_equipment_failed(
    commands: &mut Commands,
    entity: Entity,
    path: &str,
    reason: &str,
) {
    warn!(asset = path, %reason, "Procedural equipment asset is unusable; retaining cuboid fallback");
    commands.entity(entity).insert(ProceduralEquipmentFailed);
}

#[allow(clippy::too_many_arguments)]
fn resolve_procedural_equipment_models(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_skins: Res<Assets<GltfSkin>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    pending: Query<
        (
            Entity,
            &ItemPlaceholder,
            &ProceduralEquipmentPresentation,
            &ProceduralEquipmentRequest,
        ),
        (
            Without<ProceduralEquipmentResolved>,
            Without<ProceduralEquipmentFailed>,
        ),
    >,
    fallbacks: Query<(Entity, &ItemFallback)>,
) {
    for (root, placeholder, presentation, request) in &pending {
        if matches!(
            asset_server.load_state(request.0.id()),
            LoadState::Failed(_)
        ) {
            mark_procedural_equipment_failed(
                &mut commands,
                root,
                &presentation.asset_path,
                "root glTF failed to load",
            );
            continue;
        }
        let Some(gltf) = gltfs.get(&request.0) else {
            continue;
        };
        let Some(gltf_mesh) = gltf.meshes.first().and_then(|mesh| gltf_meshes.get(mesh)) else {
            mark_procedural_equipment_failed(
                &mut commands,
                root,
                &presentation.asset_path,
                "missing mesh zero",
            );
            continue;
        };
        let Some(primitive) = gltf_mesh.primitives.first() else {
            mark_procedural_equipment_failed(
                &mut commands,
                root,
                &presentation.asset_path,
                "mesh zero has no primitive",
            );
            continue;
        };
        let Some(skin) = gltf.skins.first().and_then(|skin| gltf_skins.get(skin)) else {
            continue;
        };
        let Some(joint_names) = skin
            .joints
            .iter()
            .map(|joint| gltf_nodes.get(joint).map(|node| node.name.clone()))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let Some(attachment_sockets) = gltf
            .named_nodes
            .iter()
            .filter_map(|(name, node)| {
                name.strip_prefix(EQUIPMENT_SOCKET_NODE_PREFIX)
                    .map(|attachment_point_id| (attachment_point_id, node))
            })
            .map(|(attachment_point_id, node)| {
                gltf_nodes
                    .get(node)
                    .map(|node| (attachment_point_id.to_owned(), node.transform))
            })
            .collect::<Option<BTreeMap<_, _>>>()
        else {
            continue;
        };
        let Some(material) = primitive.material.as_ref() else {
            mark_procedural_equipment_failed(
                &mut commands,
                root,
                &presentation.asset_path,
                "primitive has no material",
            );
            continue;
        };
        let Some(material_index) = gltf
            .materials
            .iter()
            .position(|candidate| candidate.id() == material.id())
        else {
            mark_procedural_equipment_failed(
                &mut commands,
                root,
                &presentation.asset_path,
                "primitive material is absent from the glTF material table",
            );
            continue;
        };
        let material_label = format!(
            "{}/std",
            GltfAssetLabel::Material {
                index: material_index,
                is_scale_inverted: false,
            }
        );
        let material: Handle<StandardMaterial> =
            asset_server.load(format!("{}#{material_label}", presentation.asset_path));
        commands.entity(root).with_child((
            Name::new("Procedural armor or clothing"),
            Mesh3d(primitive.mesh.clone()),
            MeshMaterial3d(material),
            Transform::IDENTITY,
            NoFrustumCulling,
            ProceduralEquipmentPart {
                item: placeholder.0,
                inverse_bindposes: skin.inverse_bind_matrices.clone(),
                joint_names,
            },
            PickupOutline(placeholder.0),
            OutlineVolume {
                visible: false,
                colour: Color::WHITE,
                width: 4.0,
            },
            OutlineMode::FloodFlat,
        ));
        commands
            .entity(placeholder.0)
            .insert(EquipmentAttachmentSockets(attachment_sockets));
        for (fallback, item) in &fallbacks {
            if item.0 == placeholder.0 {
                commands.entity(fallback).despawn();
            }
        }
        commands.entity(root).insert(ProceduralEquipmentResolved);
    }
}

fn sync_procedural_equipment_skins(
    mut commands: Commands,
    parts: Query<(Entity, &ProceduralEquipmentPart, Option<&SkinnedMesh>)>,
    items: Query<(Option<&ItemOf>, Has<TacticalSceneItem>)>,
    bones: Query<(Entity, &MhrBone, &Name)>,
) {
    let mut rig_bones = HashMap::<Entity, HashMap<String, Entity>>::new();
    for (entity, bone, name) in &bones {
        rig_bones
            .entry(bone.owner)
            .or_default()
            .insert(name.as_str().to_owned(), entity);
    }
    for (entity, part, current_skin) in &parts {
        let desired_joints = items
            .get(part.item)
            .ok()
            .and_then(|(owner, scene)| (!scene).then_some(owner?.0))
            .and_then(|owner| {
                let bones = rig_bones.get(&owner)?;
                part.joint_names
                    .iter()
                    .map(|name| bones.get(name).copied())
                    .collect::<Option<Vec<_>>>()
            });
        if let Some(joints) = desired_joints {
            if current_skin.is_none_or(|skin| skin.joints != joints) {
                commands.entity(entity).insert(SkinnedMesh {
                    inverse_bindposes: part.inverse_bindposes.clone(),
                    joints,
                });
            }
        } else if current_skin.is_some() {
            commands.entity(entity).remove::<SkinnedMesh>();
        }
    }
}

fn semantic_attachment_axis(role: BoneRole) -> Option<(BoneRole, BoneRole)> {
    Some(match role {
        BoneRole::Pelvis => (BoneRole::Pelvis, BoneRole::StomachOne),
        BoneRole::StomachOne => (BoneRole::StomachOne, BoneRole::StomachTwo),
        BoneRole::StomachTwo => (BoneRole::StomachTwo, BoneRole::StomachThree),
        BoneRole::StomachThree => (BoneRole::StomachThree, BoneRole::Chest),
        BoneRole::Chest => (BoneRole::Chest, BoneRole::NeckOne),
        BoneRole::NeckOne => (BoneRole::NeckOne, BoneRole::Head),
        BoneRole::Head => (BoneRole::NeckOne, BoneRole::Head),
        BoneRole::ClavicleLeft => (BoneRole::ClavicleLeft, BoneRole::UpperArmLeft),
        BoneRole::ClavicleRight => (BoneRole::ClavicleRight, BoneRole::UpperArmRight),
        BoneRole::UpperArmLeft => (BoneRole::UpperArmLeft, BoneRole::ForearmLeft),
        BoneRole::UpperArmRight => (BoneRole::UpperArmRight, BoneRole::ForearmRight),
        BoneRole::ForearmLeft | BoneRole::HandLeft => (BoneRole::ForearmLeft, BoneRole::HandLeft),
        BoneRole::ForearmRight | BoneRole::HandRight => {
            (BoneRole::ForearmRight, BoneRole::HandRight)
        }
        BoneRole::ThighLeft => (BoneRole::ThighLeft, BoneRole::ShinLeft),
        BoneRole::ThighRight => (BoneRole::ThighRight, BoneRole::ShinRight),
        BoneRole::ShinLeft => (BoneRole::ShinLeft, BoneRole::FootLeft),
        BoneRole::ShinRight => (BoneRole::ShinRight, BoneRole::FootRight),
        BoneRole::FootLeft | BoneRole::ToeLeft => (BoneRole::FootLeft, BoneRole::ToeLeft),
        BoneRole::FootRight | BoneRole::ToeRight => (BoneRole::FootRight, BoneRole::ToeRight),
        BoneRole::Root | BoneRole::Camera | BoneRole::WeaponLeft | BoneRole::WeaponRight => {
            return None;
        }
    })
}

fn bind_space_attachment_correction(
    bind: GlobalTransform,
    desired_bind_rotation: Quat,
) -> Transform {
    GlobalTransform::from(Transform {
        translation: bind.translation(),
        rotation: desired_bind_rotation,
        scale: Vec3::ONE,
    })
    .reparented_to(&bind)
}

fn equipment_bind_correction(
    role: BoneRole,
    rig: &HumanoidRig,
    bind_nodes: &Query<(&AuthoredBindTransform, Option<&ChildOf>)>,
) -> Option<Transform> {
    let &bone = rig.get(&role)?;
    let bind = authored_bind_global(bone, bind_nodes.get(bone).ok()?.0.owner, bind_nodes)?;
    let desired_rotation = semantic_attachment_axis(role)
        .and_then(|(from_role, to_role)| {
            let &from = rig.get(&from_role)?;
            let &to = rig.get(&to_role)?;
            let owner = bind_nodes.get(from).ok()?.0.owner;
            let from = authored_bind_global(from, owner, bind_nodes)?.translation();
            let to = authored_bind_global(to, owner, bind_nodes)?.translation();
            Some(Quat::from_rotation_arc(
                Vec3::Y,
                (to - from).try_normalize()?,
            ))
        })
        .unwrap_or(Quat::IDENTITY);
    Some(bind_space_attachment_correction(bind, desired_rotation))
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
    topologies: Query<
        (&EquipmentTopology, Option<&EquipmentAttachmentSockets>),
        Without<ItemPlaceholder>,
    >,
    rigs: Query<&HumanoidRig, With<Player>>,
    bind_nodes: Query<(&AuthoredBindTransform, Option<&ChildOf>)>,
    mut placeholders: Query<(
        Entity,
        &ItemPlaceholder,
        &mut Transform,
        &mut Visibility,
        Option<&ChildOf>,
        Has<ProceduralEquipmentResolved>,
    )>,
) {
    for (entity, placeholder, mut transform, mut visibility, parent, procedural) in
        &mut placeholders
    {
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
        } else if procedural {
            let rig_scene = resolve_character_location(topology, &topologies)
                .and(owner)
                .and_then(|owner| rigs.get(owner.0).ok())
                .and_then(HumanoidRig::rig_scene);
            if let Some(rig_scene) = rig_scene {
                if parent.is_none_or(|parent| parent.parent() != rig_scene) {
                    commands.entity(entity).insert(ChildOf(rig_scene));
                }
                *transform = Transform::IDENTITY;
                *visibility = Visibility::Inherited;
            } else {
                *visibility = Visibility::Hidden;
            }
            commands.entity(entity).remove::<HeldWeaponConstraint>();
        } else if let (Some(owner), Some(primary_hand)) = (owner, holding_side(slot)) {
            if parent.is_some() {
                commands.entity(entity).remove::<ChildOf>();
            }
            let constraint = rigs.get(owner.0).ok().and_then(|rig| {
                let role = match primary_hand {
                    HandSide::Left => BoneRole::WeaponLeft,
                    HandSide::Right => BoneRole::WeaponRight,
                };
                rig.get(&role)?;
                Some(HeldWeaponConstraint {
                    owner: owner.0,
                    primary_hand,
                    secondary_grip_local: None,
                })
            });
            if let Some(constraint) = constraint {
                *visibility = Visibility::Inherited;
                commands.entity(entity).insert(constraint);
            } else {
                *visibility = Visibility::Hidden;
                commands.entity(entity).remove::<HeldWeaponConstraint>();
            }
        } else if let Some((bone, correction)) = owner.and_then(|owner| {
            let rig = rigs.get(owner.0).ok()?;
            let role = equipment_location_bone(resolve_character_location(topology, &topologies)?);
            let bone = rig.get(&role).copied()?;
            let correction =
                if let Some(socket) = resolve_equipment_attachment_socket(topology, &topologies) {
                    // Generated attachment sockets are authored pelvis-local, so
                    // they can be consumed directly as children of the pelvis.
                    socket
                } else {
                    equipment_bind_correction(role, rig, &bind_nodes)?
                };
            Some((bone, correction))
        }) {
            if parent.is_none_or(|parent| parent.parent() != bone) {
                commands.entity(entity).insert(ChildOf(bone));
            }
            *transform = correction;
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
    topologies: &Query<
        (&EquipmentTopology, Option<&EquipmentAttachmentSockets>),
        Without<ItemPlaceholder>,
    >,
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
        topology = topologies.get(parent).ok()?.0;
    }
    None
}

fn resolve_equipment_attachment_socket(
    topology: &EquipmentTopology,
    topologies: &Query<
        (&EquipmentTopology, Option<&EquipmentAttachmentSockets>),
        Without<ItemPlaceholder>,
    >,
) -> Option<Transform> {
    let mut topology = topology;
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..32 {
        let (parent, attachment_point_id) =
            topology
                .occupancies
                .iter()
                .find_map(|occupancy| match &occupancy.anchor {
                    TacticalEquipmentAnchor::ItemAttachment {
                        parent,
                        attachment_point_id,
                    } => Some((*parent, attachment_point_id.as_str())),
                    TacticalEquipmentAnchor::CharacterLocation(_) => None,
                })?;
        if !visited.insert(parent) {
            return None;
        }
        let (parent_topology, sockets) = topologies.get(parent).ok()?;
        if let Some(socket) = sockets.and_then(|sockets| sockets.0.get(attachment_point_id)) {
            return Some(*socket);
        }
        topology = parent_topology;
    }
    None
}

fn equipment_location_bone(location: EquipmentLocation) -> BoneRole {
    match location {
        EquipmentLocation::Head | EquipmentLocation::Face => BoneRole::Head,
        EquipmentLocation::Neck => BoneRole::NeckOne,
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
    fn procedural_equipment_manifest_selects_exact_and_dropped_variants() {
        assert_eq!(
            procedural_equipment_file("mail_sleeve", Some("right")),
            Some("mail_sleeve--right.glb")
        );
        assert_eq!(
            procedural_equipment_file("mail_sleeve", None),
            Some("mail_sleeve--left.glb")
        );
        assert_eq!(
            procedural_equipment_file("mail_sleeve", Some("left_hand")),
            None
        );
        assert_eq!(
            procedural_equipment_file("leather_belt", Some("worn")),
            Some("leather_belt--worn.glb")
        );
        assert_eq!(
            procedural_equipment_file("leather_belt", None),
            Some("leather_belt--worn.glb")
        );
        assert_eq!(
            procedural_equipment_file("linen_breeches", Some("worn")),
            Some("linen_breeches--worn.glb")
        );
        assert_eq!(
            procedural_equipment_file("leather_boot", Some("right")),
            Some("leather_boot--right.glb")
        );
        assert_eq!(
            procedural_equipment_file("leather_boot", None),
            Some("leather_boot--left.glb")
        );
        assert_eq!(procedural_equipment_file("arming_sword", None), None);

        let asset_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for placements in PROCEDURAL_EQUIPMENT_ASSETS.values() {
            for file in placements.values() {
                assert!(
                    asset_root.join("equipment/procedural").join(file).is_file(),
                    "manifest asset {file} should exist"
                );
            }
        }
    }

    #[test]
    fn procedural_equipment_uses_live_rig_while_worn_and_rest_pose_when_dropped() {
        let mut world = World::new();
        let owner = world.spawn_empty().id();
        let root = world.spawn((MhrBone { owner }, Name::new("root"))).id();
        let spine = world.spawn((MhrBone { owner }, Name::new("c_spine0"))).id();
        let item = world.spawn(ItemOf(owner)).id();
        let part = world
            .spawn(ProceduralEquipmentPart {
                item,
                inverse_bindposes: Handle::default(),
                joint_names: vec!["root".into(), "c_spine0".into()],
            })
            .id();

        world
            .run_system_once(sync_procedural_equipment_skins)
            .unwrap();
        assert_eq!(
            world.get::<SkinnedMesh>(part).unwrap().joints,
            [root, spine]
        );

        world.entity_mut(item).insert(TacticalSceneItem);
        world
            .run_system_once(sync_procedural_equipment_skins)
            .unwrap();
        assert!(world.get::<SkinnedMesh>(part).is_none());
    }

    #[test]
    fn armor_starts_with_fallback_while_requesting_its_procedural_model() {
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.init_resource::<WeaponMeshCache>();
        let item = world
            .spawn((
                valid_physical(),
                EquipmentTopology {
                    placement_id: Some("worn".into()),
                    ..default()
                },
                ItemProperties {
                    weight: 2.5,
                    id: "arming_doublet".into(),
                },
            ))
            .id();

        world.run_system_once(spawn_item_placeholders).unwrap();

        let presentation = world
            .query::<(&ItemPlaceholder, &ProceduralEquipmentPresentation)>()
            .iter(&world)
            .find(|(placeholder, _)| placeholder.0 == item)
            .map(|(_, presentation)| presentation)
            .unwrap();
        assert!(
            presentation
                .asset_path
                .ends_with("equipment/procedural/arming_doublet--worn.glb")
        );
        assert!(
            world
                .query::<&ItemFallback>()
                .iter(&world)
                .any(|fallback| fallback.0 == item)
        );
    }

    #[test]
    fn identical_weapon_recipes_share_cached_mesh_handles() {
        let design = adventuresim_weapon_model::default_design("longsword").unwrap();
        let appearance = WeaponAppearance {
            generator_version: adventuresim_weapon_model::GENERATOR_VERSION,
            design_hash: adventuresim_weapon_model::design_hash(&design).0,
            recipe: adventuresim_weapon_model::encode(&design).unwrap(),
        };
        let mut cache = WeaponMeshCache::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let first = cached_weapon(&appearance, &mut cache, &mut meshes, &mut materials).unwrap();
        let second = cached_weapon(&appearance, &mut cache, &mut meshes, &mut materials).unwrap();
        assert_eq!(first.parts.len(), second.parts.len());
        assert!(
            first
                .parts
                .iter()
                .zip(&second.parts)
                .all(|(left, right)| left.mesh == right.mesh)
        );
        assert_eq!(cache.weapons.len(), 1);
    }

    #[test]
    fn corrupt_weapon_recipe_falls_back_instead_of_panicking() {
        let appearance = WeaponAppearance {
            generator_version: adventuresim_weapon_model::GENERATOR_VERSION,
            design_hash: [7; 32],
            recipe: vec![0xff, 0x00],
        };
        assert!(
            cached_weapon(
                &appearance,
                &mut WeaponMeshCache::default(),
                &mut Assets::<Mesh>::default(),
                &mut Assets::<StandardMaterial>::default(),
            )
            .is_none()
        );
    }

    fn longsword_appearance() -> WeaponAppearance {
        let design = adventuresim_weapon_model::default_design("longsword").unwrap();
        WeaponAppearance {
            generator_version: adventuresim_weapon_model::GENERATOR_VERSION,
            design_hash: adventuresim_weapon_model::design_hash(&design).0,
            recipe: adventuresim_weapon_model::encode(&design).unwrap(),
        }
    }

    #[test]
    fn identical_weapon_recipes_share_cached_tactical_icon_handles() {
        let appearance = longsword_appearance();
        let mut cache = WeaponIconCache::default();
        let mut images = Assets::<Image>::default();
        let first = cached_weapon_icon(&appearance, &mut cache, &mut images).unwrap();
        let second = cached_weapon_icon(&appearance, &mut cache, &mut images).unwrap();
        assert_eq!(first, second);
        assert_eq!(cache.icons.len(), 1);
        let image = images.get(&first).unwrap();
        assert_eq!(
            image.texture_descriptor.size,
            Extent3d {
                width: u32::from(TACTICAL_WEAPON_ICON_SIZE),
                height: u32::from(TACTICAL_WEAPON_ICON_SIZE),
                depth_or_array_layers: 1,
            }
        );
    }

    #[test]
    fn tactical_icon_cache_reauthenticates_recipe_before_a_warm_hit() {
        let valid = longsword_appearance();
        let mut cache = WeaponIconCache::default();
        let mut images = Assets::<Image>::default();
        assert!(cached_weapon_icon(&valid, &mut cache, &mut images).is_some());

        let mut borrowed_hash = valid;
        let other = adventuresim_weapon_model::default_design("rondel_dagger").unwrap();
        borrowed_hash.recipe = adventuresim_weapon_model::encode(&other).unwrap();
        assert!(cached_weapon_icon(&borrowed_hash, &mut cache, &mut images).is_none());
        assert_eq!(cache.icons.len(), 1);
    }

    #[test]
    fn distinct_weapon_designs_get_distinct_tactical_icon_handles() {
        let longsword = longsword_appearance();
        let rondel = adventuresim_weapon_model::default_design("rondel_dagger").unwrap();
        let rondel = WeaponAppearance {
            generator_version: adventuresim_weapon_model::GENERATOR_VERSION,
            design_hash: adventuresim_weapon_model::design_hash(&rondel).0,
            recipe: adventuresim_weapon_model::encode(&rondel).unwrap(),
        };
        let mut cache = WeaponIconCache::default();
        let mut images = Assets::<Image>::default();
        let longsword = cached_weapon_icon(&longsword, &mut cache, &mut images).unwrap();
        let rondel = cached_weapon_icon(&rondel, &mut cache, &mut images).unwrap();
        assert_ne!(longsword, rondel);
        assert_eq!(cache.icons.len(), 2);
    }

    fn longsword_holder_appearance() -> WeaponHolderAppearance {
        let design = adventuresim_weapon_model::default_design("longsword").unwrap();
        let holder = adventuresim_weapon_model::default_holder_design(&design).unwrap();
        WeaponHolderAppearance {
            generator_version: adventuresim_weapon_model::HOLDER_GENERATOR_VERSION,
            design_hash: adventuresim_weapon_model::holder_design_hash(&holder).0,
            recipe: adventuresim_weapon_model::encode_holder(&holder).unwrap(),
        }
    }

    #[test]
    fn identical_holder_recipes_share_cached_tactical_icon_handles() {
        let appearance = longsword_holder_appearance();
        let mut cache = WeaponIconCache::default();
        let mut images = Assets::<Image>::default();
        let first = cached_holder_icon(&appearance, &mut cache, &mut images).unwrap();
        let second = cached_holder_icon(&appearance, &mut cache, &mut images).unwrap();
        assert_eq!(first, second);
        assert_eq!(cache.icons.len(), 1);
    }

    #[test]
    fn tactical_holder_icon_cache_reauthenticates_before_a_warm_hit() {
        let valid = longsword_holder_appearance();
        let mut cache = WeaponIconCache::default();
        let mut images = Assets::<Image>::default();
        assert!(cached_holder_icon(&valid, &mut cache, &mut images).is_some());
        let mut tampered = valid;
        tampered.recipe[5] ^= 0x3c;
        assert!(cached_holder_icon(&tampered, &mut cache, &mut images).is_none());
        assert_eq!(cache.icons.len(), 1);
    }

    #[test]
    fn identical_holder_recipes_share_a_separate_cache() {
        let appearance = longsword_holder_appearance();
        let mut cache = WeaponMeshCache::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let first = cached_holder(&appearance, &mut cache, &mut meshes, &mut materials).unwrap();
        let second = cached_holder(&appearance, &mut cache, &mut meshes, &mut materials).unwrap();
        assert!(
            first
                .parts
                .iter()
                .zip(&second.parts)
                .all(|(left, right)| left.mesh == right.mesh)
        );
        assert_eq!(cache.holders.len(), 1);
        assert!(cache.weapons.is_empty());
    }

    #[test]
    fn warm_holder_cache_reauthenticates_recipe_and_hash() {
        let valid = longsword_holder_appearance();
        let mut cache = WeaponMeshCache::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        assert!(cached_holder(&valid, &mut cache, &mut meshes, &mut materials).is_some());
        let mut tampered = valid;
        tampered.recipe[5] ^= 0x3c;
        assert!(cached_holder(&tampered, &mut cache, &mut meshes, &mut materials).is_none());
    }

    #[test]
    fn holder_chassis_without_an_instance_never_renders_a_generic_box() {
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.init_resource::<WeaponMeshCache>();
        world.spawn((
            valid_physical(),
            ItemProperties {
                weight: 0.3,
                id: "scabbard".into(),
            },
        ));
        world.run_system_once(spawn_item_placeholders).unwrap();
        assert_eq!(world.query::<&Mesh3d>().iter(&world).count(), 0);
    }

    #[test]
    fn warm_cache_does_not_accept_a_recipe_with_a_borrowed_hash() {
        let valid = longsword_appearance();
        let mut cache = WeaponMeshCache::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        assert!(cached_weapon(&valid, &mut cache, &mut meshes, &mut materials).is_some());
        let mut tampered = valid;
        tampered.recipe[4] ^= 0x5a;
        assert!(cached_weapon(&tampered, &mut cache, &mut meshes, &mut materials).is_none());
    }

    fn valid_physical() -> EquipmentPhysical {
        EquipmentPhysical {
            dimensions_m: Vec3::new(0.25, 1.4, 0.08),
            grip_to_tip_m: 1.15,
            anchor_offset_m: Vec3::new(0.0, -0.45, 0.0),
        }
    }

    #[test]
    fn either_replication_arrival_order_builds_one_weapon_presentation() {
        for appearance_first in [false, true] {
            let mut world = World::new();
            world.insert_resource(Assets::<Mesh>::default());
            world.insert_resource(Assets::<StandardMaterial>::default());
            world.init_resource::<WeaponMeshCache>();
            let item = if appearance_first {
                world.spawn(longsword_appearance()).id()
            } else {
                world.spawn(valid_physical()).id()
            };
            world.run_system_once(spawn_item_placeholders).unwrap();
            if appearance_first {
                world.entity_mut(item).insert(valid_physical());
            } else {
                world.entity_mut(item).insert(longsword_appearance());
            }
            world.run_system_once(spawn_item_placeholders).unwrap();
            let roots = world
                .query::<&ItemPlaceholder>()
                .iter(&world)
                .filter(|placeholder| placeholder.0 == item)
                .count();
            assert_eq!(roots, 1, "appearance_first={appearance_first}");
        }
    }

    #[test]
    fn either_replication_arrival_order_builds_one_holder_presentation() {
        for appearance_first in [false, true] {
            let mut world = World::new();
            world.insert_resource(Assets::<Mesh>::default());
            world.insert_resource(Assets::<StandardMaterial>::default());
            world.init_resource::<WeaponMeshCache>();
            let item = if appearance_first {
                world.spawn(longsword_holder_appearance()).id()
            } else {
                world.spawn(valid_physical()).id()
            };
            world.run_system_once(spawn_item_placeholders).unwrap();
            if appearance_first {
                world.entity_mut(item).insert(valid_physical());
            } else {
                world.entity_mut(item).insert(longsword_holder_appearance());
            }
            world.run_system_once(spawn_item_placeholders).unwrap();
            let roots = world
                .query::<&ItemPlaceholder>()
                .iter(&world)
                .filter(|placeholder| placeholder.0 == item)
                .count();
            assert_eq!(roots, 1, "appearance_first={appearance_first}");
            assert_eq!(world.resource::<WeaponMeshCache>().holders.len(), 1);
        }
    }

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
    fn preview_traversal_keeps_belt_attachment_points_in_their_slots() {
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
            ))
            .id();
        let right_sheath = world
            .spawn((
                ItemOf(actor),
                ItemProperties {
                    id: "scabbard".into(),
                    weight: 0.3,
                },
                EquipmentTopology {
                    placement_id: Some("right".into()),
                    occupancies: vec![EquipmentTopologyOccupancy {
                        occupancy_id: "right-sheath".into(),
                        anchor: TacticalEquipmentAnchor::ItemAttachment {
                            parent: belt,
                            attachment_point_id: "right".into(),
                        },
                        channel: EquipmentChannel::Mount,
                        order: 1,
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
        let right_preview = world
            .run_system_once(
                move |items: Query<(Entity, &ItemOf, &EquipmentTopology, &ItemProperties)>| {
                    ordered_preview_at_location(actor, EquipmentLocation::RightBelt, &items)
                },
            )
            .unwrap();
        assert_eq!(
            right_preview.first().map(|target| target.entity),
            Some(right_sheath)
        );
        assert!(
            right_preview.iter().all(|target| target.entity != sheath),
            "the left sheath must not appear in the right belt slot"
        );
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
            (EquipmentLocation::Neck, BoneRole::NeckOne),
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
    fn attachment_correction_cancels_mhr_bind_roll_but_keeps_position() {
        let bind = GlobalTransform::from(Transform {
            translation: Vec3::new(0.4, 1.2, -0.3),
            rotation: Quat::from_euler(EulerRot::XYZ, 0.7, -0.4, 1.1),
            scale: Vec3::splat(1.25),
        });
        let desired = Quat::from_rotation_arc(Vec3::Y, Vec3::X);
        let correction = bind_space_attachment_correction(bind, desired);
        let resolved = bind.mul_transform(correction);

        assert!(resolved.translation().abs_diff_eq(bind.translation(), 1e-5));
        assert!(resolved.rotation().dot(desired).abs() > 1.0 - 1e-5);
        assert!(
            resolved
                .to_scale_rotation_translation()
                .0
                .abs_diff_eq(Vec3::ONE, 1e-5)
        );
    }

    #[test]
    fn limb_placeholders_use_semantic_joint_axes_not_local_mhr_y() {
        assert_eq!(
            semantic_attachment_axis(BoneRole::UpperArmLeft),
            Some((BoneRole::UpperArmLeft, BoneRole::ForearmLeft))
        );
        assert_eq!(
            semantic_attachment_axis(BoneRole::ThighRight),
            Some((BoneRole::ThighRight, BoneRole::ShinRight))
        );
        assert_eq!(
            semantic_attachment_axis(BoneRole::FootLeft),
            Some((BoneRole::FootLeft, BoneRole::ToeLeft))
        );
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
        let socket = Transform::from_translation(Vec3::new(-0.31, 1.02, -0.04));
        let belt = world
            .spawn((
                EquipmentTopology {
                    placement_id: Some("worn".into()),
                    occupancies: vec![occupancy(TacticalEquipmentAnchor::CharacterLocation(
                        EquipmentLocation::LeftBelt,
                    ))],
                },
                EquipmentAttachmentSockets(BTreeMap::from([("mount".into(), socket)])),
            ))
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
                move |topologies: Query<
                    (&EquipmentTopology, Option<&EquipmentAttachmentSockets>),
                    Without<ItemPlaceholder>,
                >| {
                    let topology = topologies.get(weapon).unwrap().0;
                    (
                        resolve_character_location(topology, &topologies),
                        resolve_equipment_attachment_socket(topology, &topologies),
                    )
                },
            )
            .unwrap();
        assert_eq!(location.0, Some(EquipmentLocation::LeftBelt));
        assert_eq!(location.1, Some(socket));
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
