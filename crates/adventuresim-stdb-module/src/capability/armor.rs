use super::*;

pub(super) fn wearable_protection_for_part(
    ctx: &ReducerContext,
    character_id: u64,
    part: BodyPart,
) -> Vec<adventuresim_core::equipment::WearableProtection> {
    ctx.db
        .character_equipped_item()
        .character_id()
        .filter(character_id)
        .filter_map(|equipped| {
            let inventory = ctx
                .db
                .inventory_item()
                .id()
                .find(equipped.inventory_item_id)?;
            let item = effective_item_definition(ctx, Some(inventory.id))?;
            let placement = item
                .equipment_placements
                .iter()
                .find(|placement| placement.id == equipped.placement_id)?;
            if !placement
                .protection
                .iter()
                .any(|target| runtime_body_part(*target) == part)
            {
                return None;
            }
            let (channel, order) = ctx
                .db
                .equipment_occupancy()
                .inventory_item_id()
                .filter(inventory.id)
                .max_by_key(|row| (row.channel.order(), row.order))
                .map_or((EquipmentChannel::Containment, 0), |row| {
                    (row.channel, row.order)
                });
            Some(adventuresim_core::equipment::WearableProtection {
                inventory_item_id: inventory.id,
                body_part: part,
                channel,
                order,
                coverage: item.coverage,
                resistance: item.resistance,
                padding: item.padding,
                flexibility: item.flexibility,
                range_of_motion: item.range_of_motion,
            })
        })
        .collect()
}

pub(super) fn effective_item_definition(
    ctx: &ReducerContext,
    inventory_id: Option<u64>,
) -> Option<Item> {
    let id = inventory_id?;
    let inventory = ctx.db.inventory_item().id().find(id)?;
    let mut item = ctx.db.item().id().find(&inventory.item_id)?;
    if let Some(condition) = ctx.db.item_condition().inventory_item_id().find(id) {
        let damage = condition.bins();
        item.accuracy = effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity);
        item.penetration =
            effective_weapon_stat(item.penetration, damage, item.edge_sensitivity * 0.6);
        item.block = effective_weapon_stat(item.block, damage, item.handling_sensitivity);
        item.range_of_motion =
            effective_handling(item.range_of_motion, damage, item.handling_sensitivity);
        item.resistance = effective_weapon_stat(item.resistance, damage, 0.1);
    }
    Some(item)
}

pub(super) fn armor_material(
    ctx: &ReducerContext,
    inventory_item_id: u64,
) -> Option<adventuresim_core::item_catalog::EquipmentMaterial> {
    ctx.db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .and_then(|inventory| adventuresim_core::item_catalog::definition(&inventory.item_id))
        .and_then(|definition| definition.equipment.as_ref())
        .and_then(|equipment| equipment.material)
}

pub(super) fn armor_coverage_span(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    part: BodyPart,
    fallback_coverage: f32,
) -> Option<adventuresim_core::combat::ArmorCoverageSpan> {
    let inventory = ctx.db.inventory_item().id().find(inventory_item_id)?;
    let equipped = ctx
        .db
        .character_equipped_item()
        .inventory_item_id()
        .find(inventory_item_id)?;
    let definition = adventuresim_core::item_catalog::definition(&inventory.item_id)?;
    let equipment = definition.equipment.as_ref()?;
    let placement = equipment
        .placements
        .iter()
        .find(|placement| placement.id == equipped.placement_id)?;
    Some(adventuresim_core::combat::authored_armor_coverage_span(
        placement,
        part,
        fallback_coverage,
    ))
}
