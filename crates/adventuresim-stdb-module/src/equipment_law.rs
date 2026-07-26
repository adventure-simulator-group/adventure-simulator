//! Centralized coarse settlement arms-and-armor compliance.

use adventuresim_core::organization::{Privilege, settlement_policy};
use spacetimedb::ReducerContext;

use crate::{
    ItemKind,
    character::{character, character_equip},
    item::{inventory_item, item},
};

pub fn require_item_legal(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(settlement_id) = character.current_settlement_id else {
        return Ok(());
    };
    let Some(policy) = settlement_policy(&settlement_id) else {
        return Ok(());
    };
    let inventory = ctx
        .db
        .inventory_item()
        .character_and_id()
        .filter((character_id, inventory_item_id))
        .next()
        .ok_or("Inventory item not found")?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .ok_or("Item definition not found")?;
    let privilege = match definition.kind {
        ItemKind::Weapon | ItemKind::Shield if policy.restrict_arms => Some(Privilege::BearArms),
        ItemKind::Armor if policy.restrict_armor => Some(Privilege::WearArmor),
        _ => None,
    };
    if privilege.is_some_and(|privilege| {
        !crate::organization::presented_privilege(ctx, character_id, &settlement_id, privilege)
    }) {
        return Err(match definition.kind {
            ItemKind::Armor => format!(
                "{} restricts armor; present a recognized organization with the right to wear armor",
                settlement_id
            ),
            _ => format!(
                "{} restricts arms; present a recognized organization with the right to bear arms",
                settlement_id
            ),
        });
    }
    Ok(())
}

/// Auto-unequip prohibited items. Inventory ownership is unchanged.
pub fn enforce_equipment_compliance(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Vec<u64>, String> {
    let Some(mut equip) = ctx.db.character_equip().character_id().find(character_id) else {
        return Ok(Vec::new());
    };
    let equipped = [
        equip.left_hand_item_id,
        equip.right_hand_item_id,
        equip.left_arm_armor_id,
        equip.right_arm_armor_id,
        equip.left_leg_armor_id,
        equip.right_leg_armor_id,
        equip.head_armor_id,
        equip.chest_armor_id,
        equip.stomach_armor_id,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let mut removed = Vec::new();
    for inventory_item_id in equipped {
        if require_item_legal(ctx, character_id, inventory_item_id).is_err() {
            crate::repair::unequip(&mut equip, inventory_item_id);
            removed.push(inventory_item_id);
        }
    }
    if !removed.is_empty() {
        ctx.db.character_equip().character_id().update(equip);
        crate::capability::refresh_character_capability(ctx, character_id)?;
    }
    Ok(removed)
}
