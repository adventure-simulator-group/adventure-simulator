//! Stable inventory-object identity and generic litre-based containment.
//!
//! Inventory rows provide carried custody and storage. Discrete items receive
//! their stable object identity when the row is created, and containment edges
//! use that identity so a whole subtree can travel atomically.

use adventuresim_core::{
    inventory_containers::{ContainmentGraph, Object},
    item_references::STANDARD_WATERSKIN_ID,
    material::Microliters,
    physical_object::{
        CarriedInventoryScope, InventoryLocation, OperationalCustody, PhysicalObjectId,
    },
};
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::character::character as _;
use crate::item::item as _;
use crate::strategic::{party_authority, party_inventory_item, party_item_condition};
use crate::{
    fireplace_dish, fireplace_station, inventory_item, inventory_item_amount, item_condition,
    party_item_amount,
};

pub const WATER_ITEM_ID: &str = "water";
fn empty_container_can_stack(has_contents: bool, is_nested: bool, has_condition: bool) -> bool {
    !has_contents && !is_nested && !has_condition
}

#[derive(Clone, Debug)]
#[table(accessor = inventory_object, public)]
pub struct InventoryObject {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub item_id: String,
    pub location: InventoryLocation,
}

#[derive(Clone, Debug)]
#[table(accessor = inventory_containment, public)]
pub struct InventoryContainment {
    #[primary_key]
    pub child_object_id: u64,
    #[index(btree)]
    pub parent_object_id: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = container_liquid, public)]
pub struct ContainerLiquid {
    #[primary_key]
    pub container_object_id: u64,
    /// Authored substance identity; liquids in one container never mix
    /// implicitly. The volume is counted exactly once by capacity checks.
    pub liquid_item_id: String,
    pub water_ml: u64,
}

pub(crate) fn object_for_row(
    ctx: &ReducerContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Result<Option<InventoryObject>, String> {
    let mut matches =
        ctx.db
            .inventory_object()
            .iter()
            .filter(|object| match (&object.location, scope) {
                (InventoryLocation::Personal(location), CarriedInventoryScope::Personal) => {
                    location.row_id == row_id
                }
                (InventoryLocation::Party(location), CarriedInventoryScope::Party) => {
                    location.row_id == row_id
                }
                _ => false,
            });
    let first = matches.next();
    if matches.next().is_some() {
        return Err("Inventory row has duplicate physical object identities".into());
    }
    Ok(first)
}

fn insert_carried_object(
    ctx: &ReducerContext,
    item_id: &str,
    location: InventoryLocation,
) -> Result<InventoryObject, String> {
    let scope = location
        .scope()
        .ok_or("Stable physical objects require carried custody at insertion")?;
    let row_id = location
        .row_id()
        .ok_or("Stable physical object has no backing inventory row")?;
    if object_for_row(ctx, scope, row_id)?.is_some() {
        return Err("Inventory row already has a stable physical object identity".into());
    }
    Ok(ctx.db.inventory_object().insert(InventoryObject {
        id: 0,
        item_id: item_id.into(),
        location,
    }))
}

pub(crate) fn insert_personal_object(
    ctx: &ReducerContext,
    row: &crate::InventoryItem,
) -> Result<InventoryObject, String> {
    if row.quantity != 1 {
        return Err("Stable personal inventory objects require quantity-one rows".into());
    }
    insert_carried_object(
        ctx,
        &row.item_id,
        InventoryLocation::personal(row.character_id, row.id),
    )
}

pub(crate) fn insert_party_object(
    ctx: &ReducerContext,
    row: &crate::strategic::PartyInventoryItem,
) -> Result<InventoryObject, String> {
    if row.quantity != 1 {
        return Err("Stable party inventory objects require quantity-one rows".into());
    }
    insert_carried_object(
        ctx,
        &row.item_id,
        InventoryLocation::party(row.party_id.clone(), row.id),
    )
}

pub(crate) fn object_is_nonempty(ctx: &ReducerContext, object_id: u64) -> bool {
    ctx.db
        .inventory_containment()
        .parent_object_id()
        .filter(object_id)
        .next()
        .is_some()
        || liquid_used(ctx, object_id) > 0
}

pub(crate) fn ancestry_reaches_fireplace(ctx: &ReducerContext, object_id: u64) -> bool {
    let mut cursor = Some(object_id);
    for _ in 0..=adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        let Some(id) = cursor else { return false };
        let Some(object) = ctx.db.inventory_object().id().find(id) else {
            return false;
        };
        if object.location.is_fireplace() {
            return true;
        }
        cursor = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(id)
            .map(|edge| edge.parent_object_id);
    }
    true
}

pub(crate) fn row_is_fireplace_rooted(
    ctx: &ReducerContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> bool {
    object_for_row(ctx, scope, row_id)
        .map(|object| object.is_some_and(|object| ancestry_reaches_fireplace(ctx, object.id)))
        .unwrap_or(true)
}

pub(crate) fn detach_if_nested(ctx: &ReducerContext, object_id: u64) -> Result<bool, String> {
    if ancestry_reaches_fireplace(ctx, object_id) {
        return Err("Retrieve the container from its fireplace before moving its contents".into());
    }
    let parent = ctx
        .db
        .inventory_containment()
        .child_object_id()
        .find(object_id)
        .map(|edge| edge.parent_object_id);
    let removed = ctx
        .db
        .inventory_containment()
        .child_object_id()
        .delete(object_id);
    if let Some(parent_id) = parent {
        merge_empty_container(ctx, parent_id)?;
    }
    Ok(removed)
}

pub(crate) fn object_is_nested(ctx: &ReducerContext, object_id: u64) -> bool {
    ctx.db
        .inventory_containment()
        .child_object_id()
        .find(object_id)
        .is_some()
}

/// Delete a carried physical object and its complete backing subtree exactly
/// once. Callers that otherwise delete an inventory row directly must use this
/// first so per-object capability rows cannot be orphaned.
pub(crate) fn delete_carried_object_for_row(
    ctx: &ReducerContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Result<bool, String> {
    let Some(object) = object_for_row(ctx, scope, row_id)? else {
        return Ok(false);
    };
    delete_subtree(ctx, object.id)?;
    Ok(true)
}

/// Final deletion for an item in settlement repair escrow. Repair objects are
/// intentionally outside operational carried custody and cannot contain
/// children.
pub(crate) fn delete_repair_object_for_row(
    ctx: &ReducerContext,
    row_id: u64,
) -> Result<bool, String> {
    let matches = ctx
        .db
        .inventory_object()
        .iter()
        .filter(|object| {
            matches!(
                object.location,
                InventoryLocation::Repair(ref location) if location.row_id == row_id
            )
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err("Repair inventory row has duplicate physical objects".into());
    }
    let Some(object) = matches.into_iter().next() else {
        return Ok(false);
    };
    crate::object_custody::resolve_object_custody(ctx, &object)?;
    if object_is_nonempty(ctx, object.id) || object_is_nested(ctx, object.id) {
        return Err("Repair escrow object has an invalid containment edge".into());
    }
    crate::weapon_instance::delete_for_object(ctx, object.id);
    ctx.db.inventory_object().id().delete(object.id);
    Ok(true)
}

pub(crate) fn reconcile_consumed_row(
    ctx: &ReducerContext,
    scope: CarriedInventoryScope,
    row_id: u64,
    fully_consumed: bool,
) -> Result<(), String> {
    let Some(object) = object_for_row(ctx, scope, row_id)? else {
        return Ok(());
    };
    if ancestry_reaches_fireplace(ctx, object.id) {
        return Err("Retrieve the container from its fireplace before using its contents".into());
    }
    if fully_consumed {
        let parent = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(object.id)
            .map(|edge| edge.parent_object_id);
        ctx.db
            .inventory_containment()
            .child_object_id()
            .delete(object.id);
        crate::weapon_instance::delete_for_object(ctx, object.id);
        ctx.db.inventory_object().id().delete(object.id);
        if let Some(parent_id) = parent {
            merge_empty_container(ctx, parent_id)?;
        }
    }
    Ok(())
}

pub(crate) fn detach_row_for_action(
    ctx: &ReducerContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Result<(), String> {
    if let Some(object) = object_for_row(ctx, scope, row_id)? {
        detach_if_nested(ctx, object.id)?;
    }
    Ok(())
}

pub(crate) fn subtree_object_ids(
    ctx: &ReducerContext,
    root_object_id: u64,
) -> Result<Vec<u64>, String> {
    let root_object_id =
        PhysicalObjectId::try_new(root_object_id).map_err(|error| error.to_string())?;
    graph(ctx)?
        .subtree(root_object_id)
        .map(|ids| ids.into_iter().map(PhysicalObjectId::get).collect())
        .map_err(str::to_owned)
}

/// Validates an entire custody move before its caller creates a replacement
/// root row or changes an installed fixture. This keeps death cleanup and
/// other exceptional returns fail-closed even before reducer rollback applies.
pub(crate) fn prevalidate_rehome_subtree(
    ctx: &ReducerContext,
    root_object_id: u64,
    destination: &OperationalCustody,
) -> Result<(), String> {
    match destination {
        OperationalCustody::Character(character_id) => {
            if ctx.db.character().id().find(character_id.get()).is_none() {
                return Err("Destination character custody is unavailable".into());
            }
        }
        OperationalCustody::Party(party_id) => {
            if ctx
                .db
                .party_authority()
                .id()
                .find(party_id.as_str().to_owned())
                .is_none()
            {
                return Err("Destination party custody is unavailable".into());
            }
        }
        _ => return Err("Invalid subtree destination".into()),
    }

    for id in subtree_object_ids(ctx, root_object_id)? {
        let object = ctx
            .db
            .inventory_object()
            .id()
            .find(id)
            .ok_or("Inventory subtree object is missing")?;
        crate::object_custody::resolve_object_custody(ctx, &object)?;
        match object.location {
            InventoryLocation::Personal(location) => {
                let row = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(location.row_id)
                    .ok_or("Inventory subtree row is missing")?;
                if row.quantity != 1 {
                    return Err("Stable inventory objects must be quantity one".into());
                }
            }
            InventoryLocation::Party(location) => {
                let row = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(location.row_id)
                    .ok_or("Party inventory subtree row is missing")?;
                if row.quantity != 1 {
                    return Err("Stable inventory objects must be quantity one".into());
                }
            }
            InventoryLocation::Fireplace(_) if id == root_object_id => {}
            _ => return Err("Inventory subtree has an unsupported location transition".into()),
        }
    }
    Ok(())
}

pub(crate) fn carried_location_for_row(
    destination: &OperationalCustody,
    row_id: u64,
) -> Result<InventoryLocation, String> {
    match destination {
        OperationalCustody::Character(character_id) => {
            Ok(InventoryLocation::personal(character_id.get(), row_id))
        }
        OperationalCustody::Party(party_id) => {
            Ok(InventoryLocation::party(party_id.as_str(), row_id))
        }
        OperationalCustody::Container(_)
        | OperationalCustody::Place(_)
        | OperationalCustody::Fixture(_) => {
            Err("Inventory row destination must be a character or party".into())
        }
    }
}

/// Rehomes every carried row and linked measured/food/condition state in one
/// reducer transaction. Descendant object owners are never left behind.
pub(crate) fn rehome_subtree(
    ctx: &ReducerContext,
    root_object_id: u64,
    destination: &OperationalCustody,
) -> Result<(), String> {
    prevalidate_rehome_subtree(ctx, root_object_id, destination)?;
    let ids = subtree_object_ids(ctx, root_object_id)?;
    for id in ids {
        let mut object = ctx
            .db
            .inventory_object()
            .id()
            .find(id)
            .ok_or("Inventory subtree object is missing")?;
        match (&object.location, destination) {
            (
                InventoryLocation::Personal(location),
                OperationalCustody::Character(character_id),
            ) => {
                let row_id = location.row_id;
                let mut row = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(row_id)
                    .ok_or("Inventory subtree row is missing")?;
                row.character_id = character_id.get();
                ctx.db.inventory_item().id().update(row);
                object.location = InventoryLocation::personal(character_id.get(), row_id);
            }
            (InventoryLocation::Party(location), OperationalCustody::Party(party_id)) => {
                let row_id = location.row_id;
                let mut row = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(row_id)
                    .ok_or("Party inventory subtree row is missing")?;
                row.party_id = party_id.as_str().into();
                ctx.db.party_inventory_item().id().update(row);
                object.location = InventoryLocation::party(party_id.as_str(), row_id);
            }
            (InventoryLocation::Personal(location), OperationalCustody::Party(party_id)) => {
                let source = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(location.row_id)
                    .ok_or("Inventory subtree row is missing")?;
                if source.quantity != 1 {
                    return Err("Stable inventory objects must be quantity one".into());
                }
                let destination_row =
                    ctx.db
                        .party_inventory_item()
                        .insert(crate::strategic::PartyInventoryItem {
                            id: 0,
                            party_id: party_id.as_str().into(),
                            item_id: source.item_id.clone(),
                            quantity: 1,
                        });
                if crate::food::personal_lot(ctx, source.id).is_some() {
                    crate::food::move_or_split_to_party(ctx, source.id, destination_row.id, 1, 1)?;
                }
                if crate::inventory_amount::personal_fraction(ctx, source.id).is_some() {
                    crate::inventory_amount::move_personal_to_party(
                        ctx,
                        source.id,
                        destination_row.id,
                    )?;
                }
                if let Some(condition) = ctx.db.item_condition().inventory_item_id().find(source.id)
                {
                    ctx.db
                        .item_condition()
                        .inventory_item_id()
                        .delete(source.id);
                    ctx.db
                        .party_item_condition()
                        .insert(crate::strategic::PartyItemCondition {
                            party_inventory_item_id: destination_row.id,
                            tier_1: condition.tier_1,
                            tier_2: condition.tier_2,
                            tier_3: condition.tier_3,
                            tier_4: condition.tier_4,
                            tier_5: condition.tier_5,
                        });
                }
                ctx.db.inventory_item().id().delete(source.id);
                object.location = InventoryLocation::party(party_id.as_str(), destination_row.id);
            }
            (InventoryLocation::Party(location), OperationalCustody::Character(character_id)) => {
                let source = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(location.row_id)
                    .ok_or("Party inventory subtree row is missing")?;
                if source.quantity != 1 {
                    return Err("Stable inventory objects must be quantity one".into());
                }
                let destination_row = ctx.db.inventory_item().insert(crate::InventoryItem {
                    id: 0,
                    character_id: character_id.get(),
                    item_id: source.item_id.clone(),
                    quantity: 1,
                });
                if crate::food::party_lot(ctx, source.id).is_some() {
                    crate::food::move_or_split_to_personal(
                        ctx,
                        source.id,
                        destination_row.id,
                        1,
                        1,
                    )?;
                }
                if crate::inventory_amount::party_fraction(ctx, source.id).is_some() {
                    crate::inventory_amount::move_party_to_personal(
                        ctx,
                        source.id,
                        destination_row.id,
                    )?;
                }
                if let Some(condition) = ctx
                    .db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .find(source.id)
                {
                    ctx.db
                        .party_item_condition()
                        .party_inventory_item_id()
                        .delete(source.id);
                    ctx.db
                        .item_condition()
                        .insert(crate::repair::ItemCondition {
                            inventory_item_id: destination_row.id,
                            tier_1: condition.tier_1,
                            tier_2: condition.tier_2,
                            tier_3: condition.tier_3,
                            tier_4: condition.tier_4,
                            tier_5: condition.tier_5,
                        });
                }
                ctx.db.party_inventory_item().id().delete(source.id);
                object.location =
                    InventoryLocation::personal(character_id.get(), destination_row.id);
            }
            _ => return Err("Inventory subtree has an unsupported location transition".into()),
        }
        ctx.db.inventory_object().id().update(object);
    }
    Ok(())
}

pub(crate) fn delete_subtree(ctx: &ReducerContext, root_object_id: u64) -> Result<(), String> {
    let ids = subtree_object_ids(ctx, root_object_id)?;
    let root = ctx
        .db
        .inventory_object()
        .id()
        .find(root_object_id)
        .ok_or("Inventory subtree root object is missing")?;
    let root_custody = crate::object_custody::resolve_object_custody(ctx, &root)?.root;
    if !matches!(
        root_custody,
        OperationalCustody::Character(_) | OperationalCustody::Party(_)
    ) {
        return Err("A fixture-held subtree must be retrieved before deletion".into());
    }
    let mut objects = Vec::with_capacity(ids.len());
    for id in ids {
        let object = ctx
            .db
            .inventory_object()
            .id()
            .find(id)
            .ok_or("Inventory subtree object is missing")?;
        require_exact_carried_backing(ctx, &object, &root_custody)?;
        if !matches!(
            &object.location,
            InventoryLocation::Personal(_) | InventoryLocation::Party(_)
        ) {
            return Err("Inventory subtree has an unsupported deletion location".into());
        }
        objects.push(object);
    }

    // Only mutate after every descendant, including aliases outside the
    // subtree, has passed strict custody and backing-row validation.
    objects.reverse();
    for object in objects {
        let id = object.id;
        match &object.location {
            InventoryLocation::Personal(location) => {
                ctx.db
                    .inventory_item_amount()
                    .inventory_item_id()
                    .delete(location.row_id);
                ctx.db
                    .item_condition()
                    .inventory_item_id()
                    .delete(location.row_id);
                crate::food::delete_personal_food_lot(ctx, location.row_id);
                ctx.db.inventory_item().id().delete(location.row_id);
            }
            InventoryLocation::Party(location) => {
                ctx.db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .delete(location.row_id);
                ctx.db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .delete(location.row_id);
                crate::food::delete_party_food_lot(ctx, location.row_id);
                ctx.db.party_inventory_item().id().delete(location.row_id);
            }
            _ => unreachable!("preflight accepts only carried deletion locations"),
        }
        ctx.db.container_liquid().container_object_id().delete(id);
        crate::outbreak::delete_container_water_contributions(ctx, id);
        crate::herbalism::delete_container_medicine(ctx, id);
        ctx.db.inventory_containment().child_object_id().delete(id);
        crate::weapon_instance::delete_for_object(ctx, id);
        ctx.db.inventory_object().id().delete(id);
    }
    Ok(())
}

fn require_exact_carried_backing(
    ctx: &ReducerContext,
    object: &InventoryObject,
    expected_root: &OperationalCustody,
) -> Result<(), String> {
    let resolved = crate::object_custody::resolve_object_custody(ctx, object)?;
    if &resolved.root != expected_root {
        return Err("Physical object has conflicting authenticated root custody".into());
    }
    let scope = object
        .location
        .scope()
        .ok_or("Physical object backing is not a carried inventory")?;
    let row_id = object
        .location
        .row_id()
        .ok_or("Physical object has no backing inventory row")?;
    let exact =
        object_for_row(ctx, scope, row_id)?.ok_or("Physical object has no backing identity")?;
    if exact.id != object.id {
        return Err("Physical object conflicts with its backing identity".into());
    }
    Ok(())
}

pub(crate) fn merge_empty_container(ctx: &ReducerContext, emptied_id: u64) -> Result<(), String> {
    let Some(emptied) = ctx.db.inventory_object().id().find(emptied_id) else {
        return Ok(());
    };
    let expected = crate::object_custody::carried_location_custody(&emptied.location)?;
    require_exact_carried_backing(ctx, &emptied, &expected)?;
    if !empty_container_can_stack(
        object_is_nonempty(ctx, emptied_id),
        object_is_nested(ctx, emptied_id),
        false,
    ) {
        return Ok(());
    }
    let scope = emptied
        .location
        .scope()
        .ok_or("Only carried containers can merge into inventory stacks")?;
    let mergeable_object = |row_id| -> Result<bool, String> {
        match object_for_row(ctx, scope, row_id)? {
            Some(object) => {
                require_exact_carried_backing(ctx, &object, &expected)?;
                Ok(!object_is_nonempty(ctx, object.id) && !object_is_nested(ctx, object.id))
            }
            None => Ok(true),
        }
    };
    let merged_object_id;
    match &emptied.location {
        InventoryLocation::Personal(location) => {
            let source = ctx
                .db
                .inventory_item()
                .id()
                .find(location.row_id)
                .ok_or("Emptied container row is missing")?;
            let mut targets = ctx
                .db
                .inventory_item()
                .iter()
                .filter(|row| {
                    row.id != source.id
                        && row.character_id == source.character_id
                        && row.item_id == source.item_id
                })
                .collect::<Vec<_>>();
            targets.sort_by_key(|row| row.id);
            let mut target = None;
            for row in targets {
                if mergeable_object(row.id)? {
                    target = Some(row);
                    break;
                }
            }
            let Some(mut target) = target else {
                return Ok(());
            };
            if ctx
                .db
                .item_condition()
                .inventory_item_id()
                .find(target.id)
                .is_some()
                || ctx
                    .db
                    .item_condition()
                    .inventory_item_id()
                    .find(source.id)
                    .is_some()
            {
                return Ok(());
            }
            merged_object_id =
                object_for_row(ctx, CarriedInventoryScope::Personal, target.id)?.map(|row| row.id);
            target.quantity = target
                .quantity
                .checked_add(source.quantity)
                .ok_or("Container stack quantity overflow")?;
            ctx.db.inventory_item().id().update(target);
            ctx.db.inventory_item().id().delete(source.id);
        }
        InventoryLocation::Party(location) => {
            let source = ctx
                .db
                .party_inventory_item()
                .id()
                .find(location.row_id)
                .ok_or("Emptied party container row is missing")?;
            let mut targets = ctx
                .db
                .party_inventory_item()
                .iter()
                .filter(|row| {
                    row.id != source.id
                        && row.party_id == source.party_id
                        && row.item_id == source.item_id
                })
                .collect::<Vec<_>>();
            targets.sort_by_key(|row| row.id);
            let mut target = None;
            for row in targets {
                if mergeable_object(row.id)? {
                    target = Some(row);
                    break;
                }
            }
            let Some(mut target) = target else {
                return Ok(());
            };
            if ctx
                .db
                .party_item_condition()
                .party_inventory_item_id()
                .find(target.id)
                .is_some()
                || ctx
                    .db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .find(source.id)
                    .is_some()
            {
                return Ok(());
            }
            merged_object_id =
                object_for_row(ctx, CarriedInventoryScope::Party, target.id)?.map(|row| row.id);
            target.quantity = target
                .quantity
                .checked_add(source.quantity)
                .ok_or("Container stack quantity overflow")?;
            ctx.db.party_inventory_item().id().update(target);
            ctx.db.party_inventory_item().id().delete(source.id);
        }
        _ => return Ok(()),
    }
    if let Some(other_id) = merged_object_id {
        ctx.db.inventory_object().id().delete(other_id);
    }
    ctx.db.inventory_object().id().delete(emptied.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::empty_container_can_stack;

    #[test]
    fn only_empty_root_containers_without_instance_condition_stack() {
        assert!(empty_container_can_stack(false, false, false));
        assert!(!empty_container_can_stack(true, false, false));
        assert!(!empty_container_can_stack(false, true, false));
        assert!(!empty_container_can_stack(false, false, true));
    }

    #[test]
    fn generic_solid_containers_are_authoring_driven() {
        let source = include_str!("inventory_container.rs")
            .rsplit_once("pub(crate) fn require_object")
            .expect("post-test production section")
            .1;
        assert!(source.contains("parent_definition.container_capacity_ml == 0"));
        assert!(!source.contains("GENERIC_CONTAINER_IDS"));
    }

    #[test]
    fn physical_object_row_lookup_matches_the_bespoke_location() {
        let lookup = include_str!("inventory_container.rs")
            .split("pub(crate) fn object_for_row")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn object_is_nonempty").next())
            .expect("physical object row lookup");
        assert!(lookup.contains("InventoryLocation::Personal"));
        assert!(lookup.contains("InventoryLocation::Party"));
        assert!(!lookup.contains("location_kind"));
        assert!(!lookup.contains("location_owner"));
    }

    #[test]
    fn empty_container_reconciliation_covers_removal_and_automatic_drain() {
        let source = include_str!("inventory_container.rs")
            .rsplit_once("pub(crate) fn require_object")
            .expect("post-test production section")
            .1;
        let remove = source
            .split("pub fn remove_inventory_item_from_container")
            .nth(1)
            .unwrap()
            .split("pub fn discard_container_water")
            .next()
            .unwrap();
        assert!(remove.contains("merge_empty_container(ctx, child_object_id)"));
        assert!(remove.contains("merge_empty_container(ctx, former_parent)"));
        let drain = source
            .split("pub(crate) fn consume_contained_water")
            .nth(1)
            .unwrap()
            .split("fn require_mutable")
            .next()
            .unwrap();
        assert!(drain.contains("resolve_object_custody"));
        assert!(drain.contains("resolved.root == *expected_custody"));
        assert!(drain.contains("merge_empty_container(ctx, liquid.container_object_id)"));
    }

    #[test]
    fn direct_object_mutators_require_resolved_actor_custody() {
        let source = include_str!("inventory_container.rs")
            .rsplit_once("pub(crate) fn require_object")
            .expect("post-test production section")
            .1;
        for reducer in [
            "pub fn remove_inventory_item_from_container",
            "pub fn discard_container_water",
        ] {
            let body = source.split(reducer).nth(1).expect("direct object reducer");
            assert!(body.contains("require_actor_carried_object"));
        }
        let lookup = include_str!("inventory_container.rs")
            .split("pub(crate) fn object_for_row")
            .nth(1)
            .and_then(|tail| tail.split("fn insert_carried_object").next())
            .expect("physical object lookup");
        assert!(lookup.contains("if matches.next().is_some()"));
        assert!(lookup.contains("duplicate physical object identities"));
    }

    #[test]
    fn empty_container_merge_authenticates_exact_root_before_mutation() {
        let source = crate::production_source(include_str!("inventory_container.rs"));
        let merge = source
            .split("pub(crate) fn merge_empty_container")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .expect("empty-container merge");
        let source_validation = merge
            .find("require_exact_carried_backing(ctx, &emptied, &expected)?")
            .unwrap();
        let target_validation = merge
            .find("require_exact_carried_backing(ctx, &object, &expected)?")
            .unwrap();
        let first_delete = merge.find(".delete(source.id)").unwrap();
        assert!(source_validation < first_delete);
        assert!(target_validation < first_delete);
    }

    #[test]
    fn subtree_alias_failure_is_preflighted_before_every_delete() {
        let deletion = include_str!("inventory_container.rs")
            .split("pub(crate) fn delete_subtree")
            .nth(1)
            .and_then(|tail| tail.split("fn require_exact_carried_backing").next())
            .expect("subtree deletion");
        let descendant_validation = deletion
            .find("require_exact_carried_backing(ctx, &object, &root_custody)?")
            .unwrap();
        let completed_preflight = deletion.find("objects.reverse()").unwrap();
        let first_delete = deletion.find(".delete(").unwrap();
        assert!(descendant_validation < completed_preflight);
        assert!(completed_preflight < first_delete);
    }

    #[test]
    fn water_exists_only_in_physical_containers() {
        let source = include_str!("inventory_container.rs")
            .rsplit_once("pub(crate) fn require_object")
            .expect("post-test production section")
            .1;
        assert!(!source.contains("carried_water_ml"));
        assert!(!source.contains("pooled_water_ml"));
        assert!(!source.contains("pour_water_into_container"));
        assert!(source.contains("take_container_water_contributions"));
        let deletion = include_str!("inventory_container.rs")
            .split("pub(crate) fn delete_subtree")
            .nth(1)
            .unwrap()
            .split("fn require_exact_carried_backing")
            .next()
            .unwrap();
        assert_eq!(
            deletion
                .matches("delete_container_water_contributions")
                .count(),
            1
        );
    }
}

pub(crate) fn require_object(
    ctx: &ReducerContext,
    character_id: u64,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Result<InventoryObject, String> {
    let object = object_for_row(ctx, scope, row_id)?
        .ok_or("Inventory row has no stable physical object identity")?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
    Ok(object)
}

fn measured_volume_ml(
    ctx: &ReducerContext,
    object: &InventoryObject,
    definition: &crate::Item,
) -> u64 {
    let amount = match &object.location {
        InventoryLocation::Personal(location) => ctx
            .db
            .inventory_item_amount()
            .inventory_item_id()
            .find(location.row_id)
            .map(|row| row.remaining_fraction_micros),
        InventoryLocation::Party(location) => ctx
            .db
            .party_item_amount()
            .party_inventory_item_id()
            .find(location.row_id)
            .map(|row| row.remaining_fraction_micros),
        _ => None,
    };
    let Some(fraction_micros) = amount else {
        return 0;
    };
    let fraction = adventuresim_core::inventory_measurement::ConsumableFractionMicros::try_new(
        fraction_micros,
    )
    .expect("persisted consumable fraction must not exceed one whole");
    let full_ml = if definition.alcohol_serving_ml > 0 {
        definition.alcohol_serving_ml
    } else {
        definition.exterior_volume_ml
    };
    fraction.scale_floor(u64::from(full_ml))
}

fn graph(ctx: &ReducerContext) -> Result<ContainmentGraph, String> {
    let objects = ctx
        .db
        .inventory_object()
        .iter()
        .map(|object| {
            let definition = ctx
                .db
                .item()
                .id()
                .find(object.item_id.clone())
                .ok_or_else(|| format!("Unknown item {}", object.item_id))?;
            Ok(Object {
                id: PhysicalObjectId::try_new(object.id).map_err(|error| error.to_string())?,
                exterior_volume_ml: u64::from(definition.exterior_volume_ml),
                capacity_ml: u64::from(definition.container_capacity_ml),
                measured_volume_ml: measured_volume_ml(ctx, &object, &definition),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut graph = ContainmentGraph::new(objects).map_err(str::to_owned)?;
    for edge in ctx.db.inventory_containment().iter() {
        graph
            .insert(
                PhysicalObjectId::try_new(edge.child_object_id)
                    .map_err(|error| error.to_string())?,
                PhysicalObjectId::try_new(edge.parent_object_id)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(str::to_owned)?;
    }
    Ok(graph)
}

fn liquid_used(ctx: &ReducerContext, container_object_id: u64) -> u64 {
    ctx.db
        .container_liquid()
        .container_object_id()
        .find(container_object_id)
        .map_or(0, |row| row.water_ml)
}

pub(crate) fn contained_water_ml(
    ctx: &ReducerContext,
    expected_custody: &OperationalCustody,
) -> Result<u64, String> {
    let mut total = 0_u64;
    for liquid in ctx
        .db
        .container_liquid()
        .iter()
        .filter(|liquid| liquid.liquid_item_id == WATER_ITEM_ID)
    {
        let object = ctx
            .db
            .inventory_object()
            .id()
            .find(liquid.container_object_id)
            .ok_or("Contained water has no physical container object")?;
        if crate::object_custody::resolve_object_custody(ctx, &object)?.root == *expected_custody {
            total = total
                .checked_add(liquid.water_ml)
                .ok_or("Contained water volume overflow")?;
        }
    }
    Ok(total)
}

fn remaining_container_capacity_ml(
    ctx: &ReducerContext,
    container_object_id: u64,
) -> Result<u64, String> {
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object not found")?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(object.item_id)
        .ok_or("Container definition not found")?;
    let capacity = u64::from(definition.container_capacity_ml);
    let used = graph(ctx)?
        .used_ml(PhysicalObjectId::try_new(container_object_id).map_err(|error| error.to_string())?)
        .map_err(str::to_owned)?
        .checked_add(liquid_used(ctx, container_object_id))
        .ok_or("Container volume overflow")?;
    capacity
        .checked_sub(used)
        .ok_or_else(|| "Container contents exceed authored capacity".into())
}

pub(crate) fn fill_carried_waterskins(
    ctx: &ReducerContext,
    custody: &OperationalCustody,
) -> Result<(), String> {
    let mut waterskins = ctx
        .db
        .inventory_object()
        .item_id()
        .filter(STANDARD_WATERSKIN_ID)
        .collect::<Vec<_>>();
    waterskins.sort_by_key(|object| object.id);
    for object in waterskins {
        if crate::object_custody::resolve_object_custody(ctx, &object)?.root != *custody {
            continue;
        }
        require_mutable(ctx, object.id)?;
        let existing = ctx
            .db
            .container_liquid()
            .container_object_id()
            .find(object.id);
        if existing
            .as_ref()
            .is_some_and(|liquid| liquid.liquid_item_id != WATER_ITEM_ID)
        {
            continue;
        }
        let added_ml = remaining_container_capacity_ml(ctx, object.id)?;
        if added_ml == 0 {
            continue;
        }
        if let Some(mut liquid) = existing {
            liquid.water_ml = liquid
                .water_ml
                .checked_add(added_ml)
                .ok_or("Water volume overflow")?;
            ctx.db
                .container_liquid()
                .container_object_id()
                .update(liquid);
        } else {
            ctx.db.container_liquid().insert(ContainerLiquid {
                container_object_id: object.id,
                liquid_item_id: WATER_ITEM_ID.into(),
                water_ml: added_ml,
            });
        }
    }
    Ok(())
}

pub(crate) fn consume_contained_water(
    ctx: &ReducerContext,
    consumer_character_id: u64,
    expected_custody: &OperationalCustody,
    requested_ml: u64,
) -> Result<u64, String> {
    let mut liquids = Vec::new();
    for liquid in ctx
        .db
        .container_liquid()
        .iter()
        .filter(|liquid| liquid.liquid_item_id == WATER_ITEM_ID)
    {
        let object = ctx
            .db
            .inventory_object()
            .id()
            .find(liquid.container_object_id)
            .ok_or("Contained water has no physical container object")?;
        let resolved = crate::object_custody::resolve_object_custody(ctx, &object)?;
        if resolved.root == *expected_custody {
            liquids.push(liquid);
        }
    }
    liquids.sort_by_key(|liquid| liquid.container_object_id);
    let mut remaining = requested_ml;
    for mut liquid in liquids {
        let consumed = remaining.min(liquid.water_ml);
        crate::outbreak::consume_container_water_contributions(
            ctx,
            liquid.container_object_id,
            liquid.water_ml,
            consumed,
            consumer_character_id,
        )?;
        liquid.water_ml -= consumed;
        remaining -= consumed;
        if liquid.water_ml == 0 {
            ctx.db
                .container_liquid()
                .container_object_id()
                .delete(liquid.container_object_id);
            merge_empty_container(ctx, liquid.container_object_id)?;
        } else {
            ctx.db
                .container_liquid()
                .container_object_id()
                .update(liquid);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(requested_ml - remaining)
}

pub(crate) fn require_mutable(ctx: &ReducerContext, object_id: u64) -> Result<(), String> {
    let mut cursor = Some(object_id);
    for _ in 0..=adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        let Some(id) = cursor else { return Ok(()) };
        if ctx
            .db
            .inventory_object()
            .id()
            .find(id)
            .is_some_and(|object| object.location.is_fireplace())
        {
            return Err(
                "Retrieve the container from its fireplace before changing its contents".into(),
            );
        }
        if ctx.db.fireplace_station().iter().any(|station| {
            station.instrument_object_id == Some(id)
                && ctx
                    .db
                    .fireplace_dish()
                    .station_key()
                    .find(station.key)
                    .is_some()
        }) {
            return Err("Container contents are locked while cooking".into());
        }
        if crate::herbalism::container_is_processing(ctx, id) {
            return Err("Container contents are locked while a tincture is macerating".into());
        }
        cursor = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(id)
            .map(|edge| edge.parent_object_id);
    }
    Err("Container ancestry exceeds the maximum depth".into())
}

pub(crate) fn require_container_capacity(
    ctx: &ReducerContext,
    container_object_id: u64,
    additional_ml: u64,
) -> Result<(), String> {
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object not found")?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(object.item_id)
        .ok_or("Container definition not found")?;
    if definition.container_capacity_ml == 0 {
        return Err("That item is not a container".into());
    }
    let container_object_id =
        PhysicalObjectId::try_new(container_object_id).map_err(|error| error.to_string())?;
    let used = graph(ctx)?
        .used_ml(container_object_id)
        .map_err(str::to_owned)?
        .checked_add(liquid_used(ctx, container_object_id.get()))
        .ok_or("Container volume overflow")?;
    if used
        .checked_add(additional_ml)
        .is_none_or(|next| next > u64::from(definition.container_capacity_ml))
    {
        return Err(format!(
            "Container capacity exceeded: {used} ml used of {} ml",
            definition.container_capacity_ml
        ));
    }
    Ok(())
}

#[reducer]
pub fn put_inventory_item_in_container(
    ctx: &ReducerContext,
    character_id: u64,
    child_scope: String,
    child_row_id: u64,
    parent_scope: String,
    parent_row_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, character_id)?;
    let parent_scope = CarriedInventoryScope::try_from(parent_scope.as_str())
        .map_err(|error| error.to_string())?;
    let child_scope =
        CarriedInventoryScope::try_from(child_scope.as_str()).map_err(|error| error.to_string())?;
    let parent = require_object(ctx, character_id, parent_scope, parent_row_id)?;
    let parent_definition = ctx
        .db
        .item()
        .id()
        .find(parent.item_id.clone())
        .ok_or("Container definition not found")?;
    if parent_definition.container_capacity_ml == 0 {
        return Err("That item has no authored container capability".into());
    }
    let child = require_object(ctx, character_id, child_scope, child_row_id)?;
    require_mutable(ctx, parent.id)?;
    require_mutable(ctx, child.id)?;
    if !parent.location.has_same_carried_custody(&child.location) {
        return Err("Move the item to the container owner's inventory first".into());
    }
    if ctx
        .db
        .inventory_containment()
        .child_object_id()
        .find(child.id)
        .is_some()
    {
        ctx.db
            .inventory_containment()
            .child_object_id()
            .delete(child.id);
    }
    let mut model = graph(ctx)?;
    model
        .insert(
            PhysicalObjectId::try_new(child.id).map_err(|error| error.to_string())?,
            PhysicalObjectId::try_new(parent.id).map_err(|error| error.to_string())?,
        )
        .map_err(str::to_owned)?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(parent.item_id.clone())
        .ok_or("Container definition not found")?;
    let used = model
        .used_ml(PhysicalObjectId::try_new(parent.id).map_err(|error| error.to_string())?)
        .map_err(str::to_owned)?
        .checked_add(liquid_used(ctx, parent.id))
        .ok_or("Container volume overflow")?;
    if used > u64::from(definition.container_capacity_ml) {
        return Err(format!(
            "Container capacity exceeded: {used} ml used of {} ml",
            definition.container_capacity_ml
        ));
    }
    ctx.db.inventory_containment().insert(InventoryContainment {
        child_object_id: child.id,
        parent_object_id: parent.id,
    });
    Ok(())
}

#[reducer]
pub fn remove_inventory_item_from_container(
    ctx: &ReducerContext,
    character_id: u64,
    child_object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, character_id)?;
    let child = ctx
        .db
        .inventory_object()
        .id()
        .find(child_object_id)
        .ok_or("Contained object not found")?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    crate::object_custody::require_actor_carried_object(ctx, &actor, &child)?;
    require_mutable(ctx, child_object_id)?;
    let former_parent = ctx
        .db
        .inventory_containment()
        .child_object_id()
        .find(child_object_id)
        .ok_or("Object is not inside a container")?
        .parent_object_id;
    ctx.db
        .inventory_containment()
        .child_object_id()
        .delete(child_object_id);
    merge_empty_container(ctx, child_object_id)?;
    merge_empty_container(ctx, former_parent)?;
    Ok(())
}

#[reducer]
pub fn discard_container_water(
    ctx: &ReducerContext,
    character_id: u64,
    container_object_id: u64,
    requested_ml: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    if requested_ml == 0 {
        return Err("Water amount must be positive".into());
    }
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object not found")?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
    require_mutable(ctx, container_object_id)?;
    let mut liquid = ctx
        .db
        .container_liquid()
        .container_object_id()
        .find(container_object_id)
        .ok_or("Container has no water")?;
    if liquid.liquid_item_id != WATER_ITEM_ID {
        return Err("Container does not hold water".into());
    }
    if liquid.water_ml < requested_ml {
        return Err("Container does not hold that much water".into());
    }
    crate::outbreak::take_container_water_contributions(
        ctx,
        container_object_id,
        Microliters::try_from_milliliters(liquid.water_ml)
            .map_err(|_| "Container water volume is too large")?,
        Microliters::try_from_milliliters(requested_ml)
            .map_err(|_| "Requested water volume is too large")?,
    )?;
    liquid.water_ml -= requested_ml;
    if liquid.water_ml == 0 {
        ctx.db
            .container_liquid()
            .container_object_id()
            .delete(container_object_id);
        merge_empty_container(ctx, container_object_id)?;
    } else {
        ctx.db
            .container_liquid()
            .container_object_id()
            .update(liquid);
    }
    Ok(())
}
