//! Equipment placement, occupancy, keyboard mapping, and controls.

use super::*;

pub(super) fn equipment_target_is_self_or_descendant(
    equip: &CharacterEquipmentGraph,
    moving_inventory_item_id: u64,
    candidate_parent_id: u64,
) -> bool {
    let mut ancestors = vec![candidate_parent_id];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(item_id) = ancestors.pop() {
        if item_id == moving_inventory_item_id {
            return true;
        }
        if visited.insert(item_id) {
            ancestors.extend(
                equip
                    .equipment_occupancies
                    .iter()
                    .filter(|row| row.inventory_item_id == item_id)
                    .filter_map(|row| row.parent_inventory_item_id),
            );
        }
    }
    false
}

pub(super) fn web_equipment_location(location: CoreEquipmentLocation) -> EquipmentLocation {
    match location {
        CoreEquipmentLocation::Head => EquipmentLocation::Head,
        CoreEquipmentLocation::Face => EquipmentLocation::Face,
        CoreEquipmentLocation::Neck => EquipmentLocation::Neck,
        CoreEquipmentLocation::Chest => EquipmentLocation::Chest,
        CoreEquipmentLocation::Stomach => EquipmentLocation::Stomach,
        CoreEquipmentLocation::Back => EquipmentLocation::Back,
        CoreEquipmentLocation::LeftShoulder => EquipmentLocation::LeftShoulder,
        CoreEquipmentLocation::RightShoulder => EquipmentLocation::RightShoulder,
        CoreEquipmentLocation::LeftArm => EquipmentLocation::LeftArm,
        CoreEquipmentLocation::RightArm => EquipmentLocation::RightArm,
        CoreEquipmentLocation::LeftHand => EquipmentLocation::LeftHand,
        CoreEquipmentLocation::RightHand => EquipmentLocation::RightHand,
        CoreEquipmentLocation::LeftLeg => EquipmentLocation::LeftLeg,
        CoreEquipmentLocation::RightLeg => EquipmentLocation::RightLeg,
        CoreEquipmentLocation::LeftFoot => EquipmentLocation::LeftFoot,
        CoreEquipmentLocation::RightFoot => EquipmentLocation::RightFoot,
        CoreEquipmentLocation::LeftBelt => EquipmentLocation::LeftBelt,
        CoreEquipmentLocation::RightBelt => EquipmentLocation::RightBelt,
        CoreEquipmentLocation::FrontBelt => EquipmentLocation::FrontBelt,
        CoreEquipmentLocation::BackBelt => EquipmentLocation::BackBelt,
        CoreEquipmentLocation::LeftPocket => EquipmentLocation::LeftPocket,
        CoreEquipmentLocation::RightPocket => EquipmentLocation::RightPocket,
        CoreEquipmentLocation::BackLeftPocket => EquipmentLocation::BackLeftPocket,
        CoreEquipmentLocation::BackRightPocket => EquipmentLocation::BackRightPocket,
    }
}

pub(super) fn equipment_input_display(input: &str) -> String {
    if input == "tab" {
        "Tab".to_owned()
    } else {
        input.to_ascii_uppercase()
    }
}

pub(super) fn equipment_location_display(location: CoreEquipmentLocation) -> &'static str {
    match location {
        CoreEquipmentLocation::Head => "Head",
        CoreEquipmentLocation::Face => "Face",
        CoreEquipmentLocation::Neck => "Neck",
        CoreEquipmentLocation::Chest => "Chest",
        CoreEquipmentLocation::Stomach => "Stomach",
        CoreEquipmentLocation::Back => "Back",
        CoreEquipmentLocation::LeftShoulder => "Left shoulder",
        CoreEquipmentLocation::RightShoulder => "Right shoulder",
        CoreEquipmentLocation::LeftArm => "Left arm",
        CoreEquipmentLocation::RightArm => "Right arm",
        CoreEquipmentLocation::LeftHand => "Left hand",
        CoreEquipmentLocation::RightHand => "Right hand",
        CoreEquipmentLocation::LeftLeg => "Left leg",
        CoreEquipmentLocation::RightLeg => "Right leg",
        CoreEquipmentLocation::LeftFoot => "Left foot",
        CoreEquipmentLocation::RightFoot => "Right foot",
        CoreEquipmentLocation::LeftBelt => "Left belt",
        CoreEquipmentLocation::RightBelt => "Right belt",
        CoreEquipmentLocation::FrontBelt => "Front belt",
        CoreEquipmentLocation::BackBelt => "Back belt",
        CoreEquipmentLocation::LeftPocket => "Left pocket",
        CoreEquipmentLocation::RightPocket => "Right pocket",
        CoreEquipmentLocation::BackLeftPocket => "Back-left pocket",
        CoreEquipmentLocation::BackRightPocket => "Back-right pocket",
    }
}

pub(super) fn equipment_location_wire_label(location: CoreEquipmentLocation) -> &'static str {
    match location {
        CoreEquipmentLocation::Head => "Head",
        CoreEquipmentLocation::Face => "Face",
        CoreEquipmentLocation::Neck => "Neck",
        CoreEquipmentLocation::Chest => "Chest",
        CoreEquipmentLocation::Stomach => "Stomach",
        CoreEquipmentLocation::Back => "Back",
        CoreEquipmentLocation::LeftShoulder => "LeftShoulder",
        CoreEquipmentLocation::RightShoulder => "RightShoulder",
        CoreEquipmentLocation::LeftArm => "LeftArm",
        CoreEquipmentLocation::RightArm => "RightArm",
        CoreEquipmentLocation::LeftHand => "LeftHand",
        CoreEquipmentLocation::RightHand => "RightHand",
        CoreEquipmentLocation::LeftLeg => "LeftLeg",
        CoreEquipmentLocation::RightLeg => "RightLeg",
        CoreEquipmentLocation::LeftFoot => "LeftFoot",
        CoreEquipmentLocation::RightFoot => "RightFoot",
        CoreEquipmentLocation::LeftBelt => "LeftBelt",
        CoreEquipmentLocation::RightBelt => "RightBelt",
        CoreEquipmentLocation::FrontBelt => "FrontBelt",
        CoreEquipmentLocation::BackBelt => "BackBelt",
        CoreEquipmentLocation::LeftPocket => "LeftPocket",
        CoreEquipmentLocation::RightPocket => "RightPocket",
        CoreEquipmentLocation::BackLeftPocket => "BackLeftPocket",
        CoreEquipmentLocation::BackRightPocket => "BackRightPocket",
    }
}

pub(super) fn item_kind_tag(kind: crate::spacetimedb::CatalogItemKind) -> &'static str {
    use crate::spacetimedb::CatalogItemKind;

    match kind {
        CatalogItemKind::Simple => "simple",
        CatalogItemKind::Weapon => "weapon",
        CatalogItemKind::Armor => "armor",
        CatalogItemKind::Shield => "shield",
        CatalogItemKind::Clothing => "clothing",
        CatalogItemKind::Container => "container",
        CatalogItemKind::Currency => "currency",
        CatalogItemKind::Ingredient => "ingredient",
        CatalogItemKind::Medication => "medication",
        CatalogItemKind::Food => "food",
    }
}

pub(super) fn slot_wire_label(slot: crate::spacetimedb::Slot) -> &'static str {
    use crate::spacetimedb::Slot;

    match slot {
        Slot::None => "None",
        Slot::LeftHolding => "LeftHolding",
        Slot::RightHolding => "RightHolding",
        Slot::LeftArm => "LeftArm",
        Slot::RightArm => "RightArm",
        Slot::LeftLeg => "LeftLeg",
        Slot::RightLeg => "RightLeg",
        Slot::Chest => "Chest",
        Slot::Stomach => "Stomach",
        Slot::Head => "Head",
        Slot::AnyHolding => "AnyHolding",
        Slot::AnyArm => "AnyArm",
        Slot::AnyLeg => "AnyLeg",
    }
}

pub(super) fn equipped_location_display(location: EquipmentLocation) -> &'static str {
    match location {
        EquipmentLocation::Head => "Head",
        EquipmentLocation::Face => "Face",
        EquipmentLocation::Neck => "Neck",
        EquipmentLocation::Chest => "Chest",
        EquipmentLocation::Stomach => "Stomach",
        EquipmentLocation::Back => "Back",
        EquipmentLocation::LeftShoulder => "Left shoulder",
        EquipmentLocation::RightShoulder => "Right shoulder",
        EquipmentLocation::LeftArm => "Left arm",
        EquipmentLocation::RightArm => "Right arm",
        EquipmentLocation::LeftHand => "Left hand",
        EquipmentLocation::RightHand => "Right hand",
        EquipmentLocation::LeftLeg => "Left leg",
        EquipmentLocation::RightLeg => "Right leg",
        EquipmentLocation::LeftFoot => "Left foot",
        EquipmentLocation::RightFoot => "Right foot",
        EquipmentLocation::LeftBelt => "Left belt",
        EquipmentLocation::RightBelt => "Right belt",
        EquipmentLocation::FrontBelt => "Front belt",
        EquipmentLocation::BackBelt => "Back belt",
        EquipmentLocation::LeftPocket => "Left pocket",
        EquipmentLocation::RightPocket => "Right pocket",
        EquipmentLocation::BackLeftPocket => "Back-left pocket",
        EquipmentLocation::BackRightPocket => "Back-right pocket",
    }
}

pub(super) fn equipment_body_part_display(part: EquipmentBodyPart) -> &'static str {
    match part {
        EquipmentBodyPart::LeftArm => "Left Arm",
        EquipmentBodyPart::RightArm => "Right Arm",
        EquipmentBodyPart::LeftLeg => "Left Leg",
        EquipmentBodyPart::RightLeg => "Right Leg",
        EquipmentBodyPart::Chest => "Chest",
        EquipmentBodyPart::Stomach => "Stomach",
        EquipmentBodyPart::Head => "Head",
    }
}

pub(super) fn core_equipment_channel(channel: SatsEquipmentChannel) -> CoreEquipmentChannel {
    match channel {
        SatsEquipmentChannel::Held => CoreEquipmentChannel::Held,
        SatsEquipmentChannel::BaseClothing => CoreEquipmentChannel::BaseClothing,
        SatsEquipmentChannel::Padding => CoreEquipmentChannel::Padding,
        SatsEquipmentChannel::FlexibleArmor => CoreEquipmentChannel::FlexibleArmor,
        SatsEquipmentChannel::RigidArmor => CoreEquipmentChannel::RigidArmor,
        SatsEquipmentChannel::Outerwear => CoreEquipmentChannel::Outerwear,
        SatsEquipmentChannel::Accessory => CoreEquipmentChannel::Accessory,
        SatsEquipmentChannel::Mount => CoreEquipmentChannel::Mount,
        SatsEquipmentChannel::Containment => CoreEquipmentChannel::Containment,
    }
}

pub(super) fn equipment_channel_rank(channel: CoreEquipmentChannel) -> u64 {
    u64::from(channel.order())
}

pub(super) fn equipment_channel_label(channel: CoreEquipmentChannel) -> &'static str {
    match channel {
        CoreEquipmentChannel::Held => "Held",
        CoreEquipmentChannel::BaseClothing => "Base clothing",
        CoreEquipmentChannel::Padding => "Padding",
        CoreEquipmentChannel::FlexibleArmor => "Flexible armor",
        CoreEquipmentChannel::RigidArmor => "Rigid armor",
        CoreEquipmentChannel::Outerwear => "Outerwear",
        CoreEquipmentChannel::Accessory => "Accessory",
        CoreEquipmentChannel::Mount => "Mount",
        CoreEquipmentChannel::Containment => "Contents",
    }
}

pub(super) fn equipment_binding_contains(
    binding: &InputAddressMapping,
    location: EquipmentLocation,
) -> bool {
    binding
        .locations
        .iter()
        .copied()
        .map(web_equipment_location)
        .any(|candidate| candidate == location)
}

pub(super) fn equipment_item_roots(
    equip: &CharacterEquipmentGraph,
    inventory_item_id: u64,
) -> Vec<(EquipmentLocation, CoreEquipmentChannel, u16, usize)> {
    fn visit(
        equip: &CharacterEquipmentGraph,
        inventory_item_id: u64,
        attachment_depth: usize,
        path: &mut std::collections::BTreeSet<u64>,
        roots: &mut Vec<(EquipmentLocation, CoreEquipmentChannel, u16, usize)>,
    ) {
        if !path.insert(inventory_item_id) {
            return;
        }
        for row in equip
            .equipment_occupancies
            .iter()
            .filter(|row| row.inventory_item_id == inventory_item_id)
        {
            if let Some(location) = row.location {
                roots.push((
                    crate::spacetimedb::core_equipment_location(location),
                    core_equipment_channel(row.channel),
                    row.order,
                    attachment_depth,
                ));
            } else if let Some(parent_id) = row.parent_inventory_item_id {
                visit(equip, parent_id, attachment_depth + 1, path, roots);
            }
        }
        path.remove(&inventory_item_id);
    }

    let mut roots = Vec::new();
    visit(
        equip,
        inventory_item_id,
        0,
        &mut std::collections::BTreeSet::new(),
        &mut roots,
    );
    roots
}

pub(super) fn equipment_binding_rank(
    binding: &InputAddressMapping,
    roots: &[(EquipmentLocation, CoreEquipmentChannel, u16, usize)],
) -> Option<u64> {
    roots
        .iter()
        .filter(|(location, _, _, _)| equipment_binding_contains(binding, *location))
        .map(|(_, channel, order, attachment_depth)| {
            (*attachment_depth as u64 * 100_000)
                + (equipment_channel_rank(*channel) * 1_000)
                + u64::from(*order)
        })
        .max()
}

pub(super) fn equipment_input_ranks_for_item(
    equip: &CharacterEquipmentGraph,
    inventory_item_id: u64,
) -> serde_json::Value {
    let roots = equipment_item_roots(equip, inventory_item_id);
    serde_json::Value::Object(
        INPUT_ADDRESS_MAPPINGS
            .iter()
            .filter_map(|binding| {
                equipment_binding_rank(binding, &roots)
                    .map(|rank| (binding.input.to_owned(), serde_json::json!(rank)))
            })
            .collect(),
    )
}

pub(super) fn equipment_input_map_json() -> String {
    let inputs = INPUT_ADDRESS_MAPPINGS
        .iter()
        .map(|binding| {
            serde_json::json!({
                "input": binding.input,
                "label": equipment_input_display(binding.input),
                "row": binding.keyboard_row,
                "column": binding.keyboard_column,
                "locations": binding.locations
                    .iter()
                    .copied()
                    .map(equipment_location_display)
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&inputs).unwrap_or_else(|_| "[]".to_owned())
}

pub(super) fn equipment_occupant_json(
    equip: &CharacterEquipmentGraph,
    inventory_item_id: u64,
) -> serde_json::Value {
    let item_id = equip
        .equipment_nodes
        .iter()
        .find(|node| node.inventory_item_id == inventory_item_id)
        .map(|node| node.item_name.as_str())
        .unwrap_or("unknown_item");
    serde_json::json!({
        "inventoryItemId": inventory_item_id,
        "itemName": item_display_name(item_id),
        "icon": format!("/static/icons/game/{}.svg", item_icon_name(item_id))
    })
}

pub(super) fn equipped_input_badges(
    equip: Option<&CharacterEquipmentGraph>,
    inventory_item_id: u64,
) -> Vec<(String, usize, String)> {
    let Some(equip) = equip else {
        return Vec::new();
    };
    INPUT_ADDRESS_MAPPINGS
        .iter()
        .filter_map(|binding| {
            let roots = equipment_item_roots(equip, inventory_item_id);
            equipment_binding_rank(binding, &roots)?;
            let mut stack = equip
                .equipment_nodes
                .iter()
                .filter_map(|node| {
                    let roots = equipment_item_roots(equip, node.inventory_item_id);
                    equipment_binding_rank(binding, &roots)
                        .map(|rank| (std::cmp::Reverse(rank), node.inventory_item_id))
                })
                .collect::<Vec<_>>();
            stack.sort_unstable();
            let depth = stack
                .iter()
                .position(|(_, item_id)| *item_id == inventory_item_id)
                .unwrap_or_default();
            let locations = binding
                .locations
                .iter()
                .copied()
                .map(equipment_location_display)
                .collect::<Vec<_>>()
                .join(" / ");
            Some((equipment_input_display(binding.input), depth, locations))
        })
        .collect()
}

pub(in crate::templates::settlement) fn equipment_control(
    inventory: &InventoryItem,
    definition: Option<&crate::spacetimedb::CatalogItemView>,
    equipped: bool,
    medication_is_self: bool,
    equip: Option<&CharacterEquipmentGraph>,
) -> Markup {
    let medication = definition.is_some_and(|definition| {
        definition.kind == crate::spacetimedb::CatalogItemKind::Medication
    });
    let equippable = definition.is_some_and(|definition| {
        !definition.equipment_placements.is_empty() || (medication && medication_is_self)
    });
    let placement_labels = definition
        .map(|definition| {
            definition
                .equipment_placements
                .iter()
                .map(|placement| {
                    let anchors = placement
                        .occupancy
                        .iter()
                        .map(|requirement| {
                            format!(
                                "{} · {} · depth {}",
                                equipment_location_wire_label(requirement.location),
                                equipment_channel_label(requirement.channel),
                                requirement.order
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let parent = placement
                        .parents
                        .iter()
                        .map(|parent| {
                            format!(
                                "attached via {} · depth {}",
                                equipment_channel_label(parent.channel),
                                parent.order
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let protection = placement
                        .protection
                        .iter()
                        .copied()
                        .map(equipment_body_part_display)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let conflicts = equip
                        .into_iter()
                        .flat_map(|equip| equip.equipment_occupancies.iter())
                        .filter(|occupied| occupied.inventory_item_id != inventory.id)
                        .filter(|occupied| {
                            placement.occupancy.iter().any(|requirement| {
                                occupied
                                    .location
                                    .map(crate::spacetimedb::core_equipment_location)
                                    == Some(requirement.location)
                                    && core_equipment_channel(occupied.channel)
                                        == requirement.channel
                                    && occupied.order == requirement.order
                            })
                        })
                        .map(|occupied| format!("#{}", occupied.inventory_item_id))
                        .collect::<Vec<_>>();
                    let anchor_or_parent = if parent.is_empty() { anchors } else { parent };
                    format!(
                        "{}: {}{}{}",
                        placement.id,
                        anchor_or_parent,
                        if !protection.is_empty() {
                            {
                                format!(
                                    "; protects {protection} ({:.0}% coverage)",
                                    definition.coverage * 100.0
                                )
                            }
                        } else {
                            Default::default()
                        },
                        if !conflicts.is_empty() {
                            format!("; conflict with {}", conflicts.join(", "))
                        } else {
                            Default::default()
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .unwrap_or_default();
    let wear_layer = definition
        .and_then(|definition| {
            definition
                .equipment_placements
                .iter()
                .flat_map(|placement| placement.occupancy.iter())
                .map(|requirement| requirement.channel)
                .max_by_key(|channel| equipment_channel_rank(*channel))
        })
        .map(equipment_channel_label)
        .unwrap_or("Attachment");
    let placement_options = definition
        .map(|definition| {
            definition
                .equipment_placements
                .iter()
                .enumerate()
                .map(|(placement_index, placement)| {
                    let requirements = placement
                        .parents
                        .iter()
                        .enumerate()
                        .map(|(requirement_index, requirement)| {
                            let targets = equip
                                .into_iter()
                                .flat_map(|equip| {
                                    equip.attachment_targets.iter().filter(move |target| {
                                        target.channel == requirement.channel
                                            && target.order == requirement.order
                                            && (target.accepts_tags.is_empty()
                                                || definition
                                                    .attachment_tags
                                                    .iter()
                                                    .any(|tag| target.accepts_tags.contains(tag)))
                                            && !equipment_target_is_self_or_descendant(
                                                equip,
                                                inventory.id,
                                                target.parent_inventory_item_id,
                                            )
                                    })
                                })
                                .map(|target| {
                                    let mut occupants = equip
                                        .into_iter()
                                        .flat_map(|equip| equip.equipment_occupancies.iter())
                                        .filter(|row| {
                                            row.parent_inventory_item_id
                                                == Some(target.parent_inventory_item_id)
                                                && row.attachment_point_id.as_deref()
                                                    == Some(target.attachment_point_id.as_str())
                                        })
                                        .collect::<Vec<_>>();
                                    occupants.sort_by_key(|row| row.capacity_index);
                                    serde_json::json!({
                                        "parentInventoryItemId": target.parent_inventory_item_id,
                                        "attachmentPointId": target.attachment_point_id,
                                        "freeCapacity": target.free_capacity,
                                        "occupants": occupants
                                            .into_iter()
                                            .filter_map(|row| equip.map(|equip| {
                                                equipment_occupant_json(equip, row.inventory_item_id)
                                            }))
                                            .collect::<Vec<_>>(),
                                        "inputRanks": equip
                                            .map(|equip| equipment_input_ranks_for_item(
                                                equip,
                                                target.parent_inventory_item_id,
                                            ))
                                            .unwrap_or_else(|| serde_json::json!({})),
                                        "label": format!(
                                            "{} / {} ({} free)",
                                            item_display_name(&target.parent_item_name),
                                            target.attachment_point_id,
                                            target.free_capacity
                                        )
                                    })
                                })
                                .collect::<Vec<_>>();
                            serde_json::json!({
                                "requirementIndex": requirement_index,
                                "channel": equipment_channel_label(requirement.channel),
                                "targets": targets
                            })
                        })
                        .collect::<Vec<_>>();
                    let input_ranks = serde_json::Value::Object(
                        INPUT_ADDRESS_MAPPINGS
                            .iter()
                            .filter_map(|binding| {
                                placement
                                    .occupancy
                                    .iter()
                                    .filter(|requirement| {
                                        equipment_binding_contains(binding, requirement.location)
                                    })
                                    .map(|requirement| {
                                        (equipment_channel_rank(requirement.channel) * 1_000)
                                            + u64::from(requirement.order)
                                    })
                                    .max()
                                    .map(|rank| (binding.input.to_owned(), serde_json::json!(rank)))
                            })
                            .collect(),
                    );
                    let input_occupants = serde_json::Value::Object(
                        INPUT_ADDRESS_MAPPINGS
                            .iter()
                            .filter_map(|binding| {
                                let occupant = placement
                                    .occupancy
                                    .iter()
                                    .filter(|requirement| {
                                        equipment_binding_contains(binding, requirement.location)
                                    })
                                    .filter_map(|requirement| {
                                        equip
                                            .into_iter()
                                            .flat_map(|equip| {
                                                equip.equipment_occupancies.iter().map(move |row| {
                                                    (equip, row)
                                                })
                                            })
                                            .find(|(_, occupied)| {
                                        occupied.location.map(crate::spacetimedb::core_equipment_location)
                                            == Some(requirement.location)
                                                    && core_equipment_channel(occupied.channel)
                                                        == requirement.channel
                                                    && occupied.order == requirement.order
                                            })
                                    })
                                    .max_by_key(|(_, occupied)| {
                                        (
                                                        equipment_channel_rank(core_equipment_channel(occupied.channel)),
                                            occupied.order,
                                            occupied.inventory_item_id,
                                        )
                                    });
                                occupant.map(|(equip, occupied)| {
                                    (
                                        binding.input.to_owned(),
                                        equipment_occupant_json(
                                            equip,
                                            occupied.inventory_item_id,
                                        ),
                                    )
                                })
                            })
                            .collect(),
                    );
                    serde_json::json!({
                        "placementIndex": placement_index,
                        "label": placement_labels
                            .split('|')
                            .nth(placement_index)
                            .unwrap_or(&placement.id),
                        "hasBody": !placement.occupancy.is_empty(),
                        "inputRanks": input_ranks,
                        "inputOccupants": input_occupants,
                        "requirements": requirements
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let placement_options_json =
        serde_json::to_string(&placement_options).unwrap_or_else(|_| "[]".to_owned());
    let input_map_json = equipment_input_map_json();
    let input_badges = equipped_input_badges(equip, inventory.id);
    let equipped_node = equip.and_then(|equip| {
        equip
            .equipment_nodes
            .iter()
            .find(|node| node.inventory_item_id == inventory.id)
    });
    let attachment_summary = equip.and_then(|equip| {
        let parents = equip
            .equipment_occupancies
            .iter()
            .filter(|row| row.inventory_item_id == inventory.id)
            .filter_map(|row| {
                let parent_id = row.parent_inventory_item_id?;
                let parent_name = equip
                    .equipment_nodes
                    .iter()
                    .find(|parent| parent.inventory_item_id == parent_id)
                    .map(|parent| item_display_name(&parent.item_name))
                    .unwrap_or_else(|| format!("#{parent_id}"));
                Some(format!(
                    "{parent_name} / {}",
                    row.attachment_point_id.as_deref()?
                ))
            })
            .collect::<Vec<_>>();
        (!parents.is_empty()).then(|| format!("Attached: {}", parents.join(" + ")))
    });
    let mut equipped_context = Vec::new();
    if let Some(equip) = equip.filter(|_| equipped) {
        let mut locations = Vec::new();
        for (location, _, _, _) in equipment_item_roots(equip, inventory.id) {
            let location = equipped_location_display(location);
            if !locations.contains(&location) {
                locations.push(location);
            }
        }
        if !locations.is_empty() {
            equipped_context.push(format!("Slots: {}", locations.join(", ")));
        }
    }
    if let Some(protection) = definition
        .zip(equipped_node)
        .and_then(|(definition, node)| {
            definition
                .equipment_placements
                .iter()
                .find(|placement| placement.id == node.placement_id)
                .filter(|placement| !placement.protection.is_empty())
                .map(|placement| {
                    format!(
                        "Protects {} ({:.0}% coverage)",
                        placement
                            .protection
                            .iter()
                            .copied()
                            .map(equipment_body_part_display)
                            .collect::<Vec<_>>()
                            .join(", "),
                        definition.coverage * 100.0
                    )
                })
        })
    {
        equipped_context.push(protection);
    }
    if let Some(attachment) = &attachment_summary {
        equipped_context.push(attachment.clone());
    }
    if !input_badges.is_empty() {
        equipped_context.push(format!(
            "Keyboard: {}",
            input_badges
                .iter()
                .map(|(input, depth, locations)| {
                    format!("{input} = {locations}, layer {}", depth + 1)
                })
                .collect::<Vec<_>>()
                .join("; ")
        ));
    } else if equipped {
        equipped_context.push("Equipped; no keyboard binding".into());
    }
    let item_name = item_display_name(&inventory.item_id);
    let base_label = if medication && medication_is_self {
        format!("Administer {item_name}")
    } else if medication {
        format!("Only {item_name}'s owner can administer it")
    } else if equipped {
        format!("Unequip {item_name}")
    } else {
        format!("Equip {item_name}")
    };
    let base_title = if medication && medication_is_self {
        "Administer one standard course of this preparation"
    } else if medication {
        "Select this character to administer their preparation"
    } else if equipped {
        "Click to unassign this item from all equipped slots"
    } else if equippable {
        "Click to choose an available keyboard slot"
    } else {
        "This item cannot be equipped"
    };
    let label = if equipped_context.is_empty() {
        base_label
    } else {
        format!("{base_label}. {}", equipped_context.join(". "))
    };
    let title = if equipped_context.is_empty() {
        base_title.to_owned()
    } else {
        format!("{base_title}. {}", equipped_context.join(". "))
    };
    html! {
        @if medication {
            input type="checkbox"
                checked[equipped]
                disabled[!equippable]
                data-equipment-toggle
                data-equipment-medication
                data-inventory-item-id=(inventory.id)
                aria-describedby=(format!("equipment-status-{}", inventory.id))
                aria-label=(label)
                title=(title);
        } @else if equippable {
            button type="button"
                class="equipment-slot-control"
                data-equipment-toggle
                data-equipment-equipped=(equipped)
                data-inventory-item-id=(inventory.id)
                data-wear-layer=(wear_layer)
                data-wear-placements=(placement_labels)
                data-equipment-input-map=(input_map_json)
                data-equipment-placement-options=(placement_options_json)
                aria-haspopup="dialog"
                aria-describedby=(format!("equipment-status-{}", inventory.id))
                aria-label=(label)
                data-strategic-tooltip=(title) {
                @if equipped {
                    @if input_badges.is_empty() {
                        span class="equipment-slot-unbound"
                            aria-hidden="true" { (decorative_game_icon("check-mark")) }
                    } @else {
                        @for (input, depth, locations) in &input_badges {
                            @let lightness = 88usize.saturating_sub((*depth).min(5) * 9);
                            kbd class="equipment-slot-key"
                                data-equipment-layer-depth=(depth)
                                data-strategic-tooltip=(format!("{input}: {locations}, layer {}", depth + 1))
                                style=(format!("--equipment-layer-lightness: {lightness}%")) {
                                (input)
                            }
                        }
                    }
                } @else {
                    span class="equipment-slot-empty" aria-hidden="true" { "+" }
                }
            }
        } @else {
            span class="equipment-unavailable" role="img" tabindex="0"
                aria-label="Not equippable"
                data-strategic-tooltip="This item cannot be equipped" {}
        }
        span id=(format!("equipment-status-{}", inventory.id))
            class="equipment-toggle-status"
            data-equipment-status
            role="status"
            aria-live="polite"
            hidden {}
        @if let Some(attachment_summary) = attachment_summary {
            span class="equipment-graph-summary" { (attachment_summary) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use crate::spacetimedb::{EquipmentAnchorKind, EquipmentLocation};

    #[test]
    fn equipment_slot_dialog_is_centered_in_the_viewport() {
        let css = include_str!("../../../../static/css/strategic.css");
        let rule = css
            .split(".equipment-placement-modal {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("equipment placement dialog rule");
        assert!(rule.contains("position: fixed"));
        assert!(rule.contains("inset: 0"));
        assert!(rule.contains("margin: auto"));
        assert!(rule.contains("max-block-size: calc(100dvh - 2rem)"));
    }

    #[test]
    fn equipment_slot_control_is_enabled_only_for_authored_placements() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "sword".into(),
            quantity: 1,
        };
        let mut definition = crate::spacetimedb::CatalogItemView {
            id: "sword".into(),
            weight: 1.0,
            slot: Slot::AnyHolding,
            kind: crate::spacetimedb::CatalogItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 3,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            equipment_placements: vec![CatalogEquipmentPlacement {
                id: "left_hand".into(),
                occupancy: vec![OccupancyRequirement {
                    location: EquipmentLocation::LeftHand,
                    channel: CoreEquipmentChannel::Held,
                    order: 0,
                }],
                parents: Vec::new(),
                protection: Vec::new(),
            }],
            ..Default::default()
        };
        let enabled =
            equipment_control(&inventory, Some(&definition), false, true, None).into_string();
        assert!(enabled.contains("data-equipment-toggle"));
        assert!(enabled.contains("equipment-slot-control"));
        assert!(enabled.contains("data-equipment-input-map"));
        assert!(!enabled.contains(" disabled"));
        definition.equipment_placements.clear();
        let disabled =
            equipment_control(&inventory, Some(&definition), false, true, None).into_string();
        assert!(disabled.contains("class=\"equipment-unavailable\""));
        assert!(disabled.contains("aria-label=\"Not equippable\""));
        assert!(!disabled.contains("equipment-slot-control"));

        definition
            .equipment_placements
            .push(CatalogEquipmentPlacement {
                id: "left_hand".into(),
                occupancy: vec![OccupancyRequirement {
                    location: EquipmentLocation::LeftHand,
                    channel: CoreEquipmentChannel::Held,
                    order: 0,
                }],
                parents: Vec::new(),
                protection: Vec::new(),
            });
        let unbound =
            equipment_control(&inventory, Some(&definition), true, true, None).into_string();
        assert!(unbound.contains("check-mark.svg"));
        assert!(unbound.contains("Equipped; no keyboard binding"));
        assert!(!unbound.contains(">?</kbd>"));
    }

    #[test]
    fn equipped_slot_control_lists_all_keys_and_darkens_inner_layers() {
        let outer = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "cloak".into(),
            quantity: 1,
        };
        let inner = InventoryItem {
            id: 8,
            character_id: 9,
            item_id: "tunic".into(),
            quantity: 1,
        };
        let placement = |id: &str, channel| CatalogEquipmentPlacement {
            id: id.into(),
            occupancy: vec![
                OccupancyRequirement {
                    location: EquipmentLocation::Chest,
                    channel,
                    order: 0,
                },
                OccupancyRequirement {
                    location: EquipmentLocation::Stomach,
                    channel,
                    order: 0,
                },
            ],
            parents: Vec::new(),
            protection: vec![EquipmentBodyPart::Chest],
        };
        let definition = crate::spacetimedb::CatalogItemView {
            id: outer.item_id.clone(),
            kind: crate::spacetimedb::CatalogItemKind::Clothing,
            coverage: 0.8,
            equipment_placements: vec![placement("worn", CoreEquipmentChannel::Outerwear)],
            ..Default::default()
        };
        let occupancy = |inventory_item_id, location, channel| EquipmentOccupancy {
            id: format!(
                "{inventory_item_id}:{}",
                equipment_location_wire_label(crate::spacetimedb::core_equipment_location(
                    location
                ))
            ),
            character_id: 9,
            inventory_item_id,
            anchor_kind: EquipmentAnchorKind::CharacterLocation,
            location: Some(location),
            parent_inventory_item_id: None,
            attachment_point_id: None,
            channel,
            order: 0,
            requirement_index: 0,
            capacity_index: 0,
        };
        let graph = CharacterEquipmentGraph {
            _character_id: 9,
            worn_item_ids: vec![outer.id, inner.id],
            equipment_nodes: vec![
                EquippedItemView {
                    inventory_item_id: outer.id,
                    character_id: 9,
                    placement_id: "worn".into(),
                    item_name: outer.item_id.clone(),
                },
                EquippedItemView {
                    inventory_item_id: inner.id,
                    character_id: 9,
                    placement_id: "worn".into(),
                    item_name: inner.item_id.clone(),
                },
            ],
            equipment_occupancies: vec![
                occupancy(
                    outer.id,
                    adventuresim_stdb_client::EquipmentLocation::Chest,
                    SatsEquipmentChannel::Outerwear,
                ),
                occupancy(
                    outer.id,
                    adventuresim_stdb_client::EquipmentLocation::Stomach,
                    SatsEquipmentChannel::Outerwear,
                ),
                occupancy(
                    inner.id,
                    adventuresim_stdb_client::EquipmentLocation::Chest,
                    SatsEquipmentChannel::BaseClothing,
                ),
            ],
            attachment_targets: Vec::new(),
        };

        let outer_control =
            equipment_control(&outer, Some(&definition), true, true, Some(&graph)).into_string();
        assert!(outer_control.contains(">G</kbd>"));
        assert!(outer_control.contains(">Y</kbd>"));
        assert!(outer_control.contains("--equipment-layer-lightness: 88%"));
        assert!(outer_control.contains("aria-label=\"Unequip Cloak. Slots: Chest, Stomach. Protects Chest (80% coverage). Keyboard: G = Chest, layer 1; Y = Stomach, layer 1\""));
        assert!(outer_control.contains("data-strategic-tooltip=\"G: Chest, layer 1\""));
        assert!(outer_control.contains("protects Chest (80% coverage)"));

        let inner_definition = crate::spacetimedb::CatalogItemView {
            id: inner.item_id.clone(),
            kind: crate::spacetimedb::CatalogItemKind::Clothing,
            equipment_placements: vec![placement("worn", CoreEquipmentChannel::BaseClothing)],
            ..Default::default()
        };
        let inner_control =
            equipment_control(&inner, Some(&inner_definition), true, true, Some(&graph))
                .into_string();
        assert!(inner_control.contains(">G</kbd>"));
        assert!(inner_control.contains("--equipment-layer-lightness: 79%"));
    }

    #[test]
    fn attached_item_names_its_parent_on_the_inventory_row() {
        let parent = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "belt".into(),
            quantity: 1,
        };
        let child = InventoryItem {
            id: 8,
            character_id: 9,
            item_id: "pouch".into(),
            quantity: 1,
        };
        let definition = crate::spacetimedb::CatalogItemView {
            id: child.item_id.clone(),
            kind: crate::spacetimedb::CatalogItemKind::Container,
            equipment_placements: vec![CatalogEquipmentPlacement {
                id: "hung".into(),
                occupancy: Vec::new(),
                parents: Vec::new(),
                protection: Vec::new(),
            }],
            ..Default::default()
        };
        let graph = CharacterEquipmentGraph {
            _character_id: 9,
            worn_item_ids: vec![parent.id, child.id],
            equipment_nodes: vec![
                EquippedItemView {
                    inventory_item_id: parent.id,
                    character_id: 9,
                    placement_id: "worn".into(),
                    item_name: parent.item_id.clone(),
                },
                EquippedItemView {
                    inventory_item_id: child.id,
                    character_id: 9,
                    placement_id: "hung".into(),
                    item_name: child.item_id.clone(),
                },
            ],
            equipment_occupancies: vec![
                EquipmentOccupancy {
                    id: "belt:front".into(),
                    character_id: 9,
                    inventory_item_id: parent.id,
                    anchor_kind: EquipmentAnchorKind::CharacterLocation,
                    location: Some(adventuresim_stdb_client::EquipmentLocation::FrontBelt),
                    parent_inventory_item_id: None,
                    attachment_point_id: None,
                    channel: SatsEquipmentChannel::Mount,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                },
                EquipmentOccupancy {
                    id: "pouch:belt".into(),
                    character_id: 9,
                    inventory_item_id: child.id,
                    anchor_kind: EquipmentAnchorKind::ItemAttachment,
                    location: None,
                    parent_inventory_item_id: Some(parent.id),
                    attachment_point_id: Some("front-loop".into()),
                    channel: SatsEquipmentChannel::Containment,
                    order: 0,
                    requirement_index: 0,
                    capacity_index: 0,
                },
            ],
            attachment_targets: Vec::new(),
        };

        let rendered =
            equipment_control(&child, Some(&definition), true, true, Some(&graph)).into_string();
        assert!(rendered.contains(">F</kbd>"));
        assert!(rendered.contains(
            "aria-label=\"Unequip Pouch. Slots: Front belt. Attached: Belt / front-loop. Keyboard:"
        ));
        assert!(rendered.contains("F = Front belt"));
        assert!(rendered.contains(
            "<span class=\"equipment-graph-summary\">Attached: Belt / front-loop</span>"
        ));
    }

    #[test]
    fn medication_checkbox_describes_administration_instead_of_equipping() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "oral_rehydration_draught".into(),
            quantity: 1,
        };
        let definition = crate::spacetimedb::CatalogItemView {
            id: inventory.item_id.clone(),
            slot: Slot::None,
            kind: crate::spacetimedb::CatalogItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            equipment_control(&inventory, Some(&definition), false, true, None).into_string();
        assert!(!rendered.contains(" disabled"));
        assert!(rendered.contains("data-equipment-medication"));
        assert!(rendered.contains("type=\"checkbox\""));
        assert!(rendered.contains("aria-label=\"Administer Oral rehydration draught\""));
        assert!(rendered.contains("title=\"Administer one standard course of this preparation\""));
        assert!(!rendered.contains("Equip Oral rehydration draught"));
    }

    #[test]
    fn companion_medication_checkbox_is_disabled_with_honest_copy() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 10,
            item_id: "oral_rehydration_draught".into(),
            quantity: 1,
        };
        let definition = crate::spacetimedb::CatalogItemView {
            id: inventory.item_id.clone(),
            slot: Slot::None,
            kind: crate::spacetimedb::CatalogItemKind::Medication,
            ..Default::default()
        };
        let rendered =
            equipment_control(&inventory, Some(&definition), false, false, None).into_string();
        assert!(rendered.contains(" disabled"));
        assert!(
            rendered
                .contains("aria-label=\"Only Oral rehydration draught's owner can administer it\"")
        );
        assert!(
            rendered.contains("title=\"Select this character to administer their preparation\"")
        );
        assert!(!rendered.contains("aria-label=\"Administer Oral rehydration draught\""));
    }

    #[test]
    fn attachment_target_filter_rejects_self_and_descendants() {
        let edge = |child, parent| EquipmentOccupancy {
            id: format!("{parent}->{child}"),
            character_id: 1,
            inventory_item_id: child,
            anchor_kind: EquipmentAnchorKind::ItemAttachment,
            location: None,
            parent_inventory_item_id: Some(parent),
            attachment_point_id: Some("point".into()),
            channel: SatsEquipmentChannel::Mount,
            order: 0,
            requirement_index: 0,
            capacity_index: 0,
        };
        let equip = CharacterEquipmentGraph {
            _character_id: 1,
            worn_item_ids: vec![10, 20, 30],
            equipment_nodes: Vec::new(),
            equipment_occupancies: vec![edge(20, 10), edge(30, 20)],
            attachment_targets: Vec::new(),
        };
        assert!(equipment_target_is_self_or_descendant(&equip, 10, 10));
        assert!(equipment_target_is_self_or_descendant(&equip, 10, 30));
        assert!(!equipment_target_is_self_or_descendant(&equip, 30, 10));
    }
}
