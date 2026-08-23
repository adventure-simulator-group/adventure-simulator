//! Stable inventory-object identity and generic litre-based containment.
//!
//! Inventory rows remain the existing custody/storage ABI. This module gives
//! a row a stable object identity before it participates in containment and
//! records edges by object identity, so a whole subtree can travel atomically.

use adventuresim_core::{
    inventory_containers::{ContainmentGraph, Object},
    physical_object::{OperationalCustody, PhysicalObjectId},
};
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::character::character as _;
use crate::item::item as _;
use crate::strategic::{party_authority, party_inventory_item, party_item_condition};
use crate::{
    character_needs, fireplace_dish, fireplace_station, inventory_item, inventory_item_amount,
    item_condition, party_item_amount,
};

pub const WATER_ITEM_ID: &str = "water";
fn empty_container_can_stack(has_contents: bool, is_nested: bool, has_condition: bool) -> bool {
    !has_contents && !is_nested && !has_condition
}

#[derive(Clone, Debug)]
#[table(
    accessor = inventory_object,
    public,
    index(accessor = location_and_row, btree(columns = [location_kind, inventory_row_id]))
)]
pub struct InventoryObject {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub item_id: String,
    /// `personal`, `party`, `fireplace`, or temporary `repair` escrow.
    #[index(btree)]
    pub location_kind: String,
    /// Character ID, party ID, canonical fixture ID, or repair settlement place ID.
    pub location_owner: String,
    /// Current inventory row while carried or in repair; zero at a fireplace.
    pub inventory_row_id: u64,
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
    kind: &str,
    row_id: u64,
) -> Result<Option<InventoryObject>, String> {
    if !matches!(kind, "personal" | "party") {
        return Err("Physical object row lookup requires carried custody".into());
    }
    let mut matches = ctx
        .db
        .inventory_object()
        .location_and_row()
        .filter((kind, row_id));
    let first = matches.next();
    if matches.next().is_some() {
        return Err("Inventory row has duplicate physical object identities".into());
    }
    Ok(first)
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
        if object.location_kind == "fireplace" {
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

pub(crate) fn row_is_fireplace_rooted(ctx: &ReducerContext, kind: &str, row_id: u64) -> bool {
    object_for_row(ctx, kind, row_id)
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
    kind: &str,
    row_id: u64,
) -> Result<bool, String> {
    let Some(object) = object_for_row(ctx, kind, row_id)? else {
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
        .location_and_row()
        .filter(("repair", row_id))
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
    kind: &str,
    row_id: u64,
    fully_consumed: bool,
) -> Result<(), String> {
    let Some(object) = object_for_row(ctx, kind, row_id)? else {
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
    kind: &str,
    row_id: u64,
) -> Result<(), String> {
    if let Some(object) = object_for_row(ctx, kind, row_id)? {
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
    destination_kind: &str,
    destination_owner: &str,
) -> Result<(), String> {
    match destination_kind {
        "personal" => {
            let character_id = destination_owner
                .parse::<u64>()
                .map_err(|_| "Invalid destination character")?;
            if destination_owner != character_id.to_string()
                || ctx.db.character().id().find(character_id).is_none()
            {
                return Err("Destination character custody is unavailable".into());
            }
        }
        "party" => {
            if ctx
                .db
                .party_authority()
                .id()
                .find(destination_owner.to_string())
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
        match object.location_kind.as_str() {
            "personal" => {
                let row = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(object.inventory_row_id)
                    .ok_or("Inventory subtree row is missing")?;
                if row.quantity != 1 {
                    return Err("Stable inventory objects must be quantity one".into());
                }
            }
            "party" => {
                let row = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(object.inventory_row_id)
                    .ok_or("Party inventory subtree row is missing")?;
                if row.quantity != 1 {
                    return Err("Stable inventory objects must be quantity one".into());
                }
            }
            "fireplace" if id == root_object_id && object.inventory_row_id == 0 => {}
            _ => return Err("Inventory subtree has an unsupported location transition".into()),
        }
    }
    Ok(())
}

/// Rehomes every legacy row and linked measured/food/condition state in one
/// reducer transaction. Descendant object owners are never left behind.
pub(crate) fn rehome_subtree(
    ctx: &ReducerContext,
    root_object_id: u64,
    destination_kind: &str,
    destination_owner: &str,
) -> Result<(), String> {
    prevalidate_rehome_subtree(ctx, root_object_id, destination_kind, destination_owner)?;
    let ids = subtree_object_ids(ctx, root_object_id)?;
    for id in ids {
        let mut object = ctx
            .db
            .inventory_object()
            .id()
            .find(id)
            .ok_or("Inventory subtree object is missing")?;
        if object.location_kind == destination_kind {
            match destination_kind {
                "personal" => {
                    let character_id = destination_owner
                        .parse::<u64>()
                        .map_err(|_| "Invalid destination character")?;
                    let mut row = ctx
                        .db
                        .inventory_item()
                        .id()
                        .find(object.inventory_row_id)
                        .ok_or("Inventory subtree row is missing")?;
                    row.character_id = character_id;
                    ctx.db.inventory_item().id().update(row);
                }
                "party" => {
                    let mut row = ctx
                        .db
                        .party_inventory_item()
                        .id()
                        .find(object.inventory_row_id)
                        .ok_or("Party inventory subtree row is missing")?;
                    row.party_id = destination_owner.into();
                    ctx.db.party_inventory_item().id().update(row);
                }
                _ => unreachable!(),
            }
        } else {
            match (object.location_kind.as_str(), destination_kind) {
                ("personal", "party") => {
                    let source = ctx
                        .db
                        .inventory_item()
                        .id()
                        .find(object.inventory_row_id)
                        .ok_or("Inventory subtree row is missing")?;
                    if source.quantity != 1 {
                        return Err("Stable inventory objects must be quantity one".into());
                    }
                    let destination = ctx.db.party_inventory_item().insert(
                        crate::strategic::PartyInventoryItem {
                            id: 0,
                            party_id: destination_owner.into(),
                            item_id: source.item_id.clone(),
                            quantity: 1,
                        },
                    );
                    if crate::food::personal_lot(ctx, source.id).is_some() {
                        crate::food::move_or_split_to_party(ctx, source.id, destination.id, 1, 1)?;
                    }
                    if crate::inventory_amount::personal_amount(ctx, source.id).is_some() {
                        crate::inventory_amount::move_personal_to_party(
                            ctx,
                            source.id,
                            destination.id,
                        )?;
                    }
                    if let Some(condition) =
                        ctx.db.item_condition().inventory_item_id().find(source.id)
                    {
                        ctx.db
                            .item_condition()
                            .inventory_item_id()
                            .delete(source.id);
                        ctx.db.party_item_condition().insert(
                            crate::strategic::PartyItemCondition {
                                party_inventory_item_id: destination.id,
                                tier_1: condition.tier_1,
                                tier_2: condition.tier_2,
                                tier_3: condition.tier_3,
                                tier_4: condition.tier_4,
                                tier_5: condition.tier_5,
                            },
                        );
                    }
                    ctx.db.inventory_item().id().delete(source.id);
                    object.inventory_row_id = destination.id;
                }
                ("party", "personal") => {
                    let character_id = destination_owner
                        .parse::<u64>()
                        .map_err(|_| "Invalid destination character")?;
                    let source = ctx
                        .db
                        .party_inventory_item()
                        .id()
                        .find(object.inventory_row_id)
                        .ok_or("Party inventory subtree row is missing")?;
                    if source.quantity != 1 {
                        return Err("Stable inventory objects must be quantity one".into());
                    }
                    let destination = ctx.db.inventory_item().insert(crate::InventoryItem {
                        id: 0,
                        character_id,
                        item_id: source.item_id.clone(),
                        quantity: 1,
                    });
                    if crate::food::party_lot(ctx, source.id).is_some() {
                        crate::food::move_or_split_to_personal(
                            ctx,
                            source.id,
                            destination.id,
                            1,
                            1,
                        )?;
                    }
                    if crate::inventory_amount::party_amount(ctx, source.id).is_some() {
                        crate::inventory_amount::move_party_to_personal(
                            ctx,
                            source.id,
                            destination.id,
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
                                inventory_item_id: destination.id,
                                tier_1: condition.tier_1,
                                tier_2: condition.tier_2,
                                tier_3: condition.tier_3,
                                tier_4: condition.tier_4,
                                tier_5: condition.tier_5,
                            });
                    }
                    ctx.db.party_inventory_item().id().delete(source.id);
                    object.inventory_row_id = destination.id;
                }
                _ => return Err("Inventory subtree has an unsupported location transition".into()),
            }
        }
        object.location_kind = destination_kind.into();
        object.location_owner = destination_owner.into();
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
        if !matches!(object.location_kind.as_str(), "personal" | "party") {
            return Err("Inventory subtree has an unsupported deletion location".into());
        }
        objects.push(object);
    }

    // Only mutate after every descendant, including aliases outside the
    // subtree, has passed strict custody and backing-row validation.
    objects.reverse();
    for object in objects {
        let id = object.id;
        match object.location_kind.as_str() {
            "personal" => {
                ctx.db
                    .inventory_item_amount()
                    .inventory_item_id()
                    .delete(object.inventory_row_id);
                ctx.db
                    .item_condition()
                    .inventory_item_id()
                    .delete(object.inventory_row_id);
                crate::food::delete_personal_food_lot(ctx, object.inventory_row_id);
                ctx.db.inventory_item().id().delete(object.inventory_row_id);
            }
            "party" => {
                ctx.db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .delete(object.inventory_row_id);
                ctx.db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .delete(object.inventory_row_id);
                crate::food::delete_party_food_lot(ctx, object.inventory_row_id);
                ctx.db
                    .party_inventory_item()
                    .id()
                    .delete(object.inventory_row_id);
            }
            _ => unreachable!("preflight accepts only carried deletion locations"),
        }
        ctx.db.container_liquid().container_object_id().delete(id);
        crate::outbreak::delete_water_holding_contributions(ctx, "container", &id.to_string());
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
    let exact = object_for_row(ctx, &object.location_kind, object.inventory_row_id)?
        .ok_or("Physical object has no backing identity")?;
    if exact.id != object.id {
        return Err("Physical object conflicts with its backing identity".into());
    }
    Ok(())
}

fn require_owned_row(
    ctx: &ReducerContext,
    character_id: u64,
    kind: &str,
    row_id: u64,
) -> Result<(String, String, u32), String> {
    match kind {
        "personal" => {
            let row = ctx
                .db
                .inventory_item()
                .id()
                .find(row_id)
                .ok_or("Inventory item not found")?;
            if row.character_id != character_id
                || crate::character::inventory_item_is_equipped(ctx, character_id, row_id)
            {
                return Err("Item is equipped or belongs to another character".into());
            }
            Ok((row.item_id, character_id.to_string(), row.quantity))
        }
        "party" => {
            let actor = ctx
                .db
                .character()
                .id()
                .find(character_id)
                .ok_or("Character not found")?;
            let party_id = actor.party_id.ok_or("Character has no party inventory")?;
            let row = ctx
                .db
                .party_inventory_item()
                .id()
                .find(row_id)
                .ok_or("Party inventory item not found")?;
            if row.party_id != party_id {
                return Err("Item belongs to another party".into());
            }
            Ok((row.item_id, party_id, row.quantity))
        }
        _ => Err("Invalid inventory scope".into()),
    }
}

fn split_one(ctx: &ReducerContext, kind: &str, row_id: u64) -> Result<u64, String> {
    // Companion rows describe this exact legacy row, rather than one arbitrary
    // member of its stack. Splitting without a domain-specific proportional
    // split would silently duplicate or strand food, measured amount, or
    // condition state. Valid authored creation already keeps these instances
    // quantity-one; reject corrupt/legacy stacks atomically.
    let has_companion = match kind {
        "personal" => {
            crate::food::personal_lot(ctx, row_id).is_some()
                || crate::inventory_amount::personal_amount(ctx, row_id).is_some()
                || ctx
                    .db
                    .item_condition()
                    .inventory_item_id()
                    .find(row_id)
                    .is_some()
        }
        "party" => {
            crate::food::party_lot(ctx, row_id).is_some()
                || crate::inventory_amount::party_amount(ctx, row_id).is_some()
                || ctx
                    .db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .find(row_id)
                    .is_some()
        }
        _ => return Err("Invalid inventory scope".into()),
    };
    if has_companion {
        return Err("Cannot split a stacked item with measured, food, or condition state".into());
    }
    match kind {
        "personal" => {
            let mut row = ctx
                .db
                .inventory_item()
                .id()
                .find(row_id)
                .ok_or("Inventory item not found")?;
            if row.quantity <= 1 {
                return Ok(row_id);
            }
            row.quantity -= 1;
            let character_id = row.character_id;
            let item_id = row.item_id.clone();
            ctx.db.inventory_item().id().update(row);
            Ok(ctx
                .db
                .inventory_item()
                .insert(crate::InventoryItem {
                    id: 0,
                    character_id,
                    item_id,
                    quantity: 1,
                })
                .id)
        }
        "party" => {
            let mut row = ctx
                .db
                .party_inventory_item()
                .id()
                .find(row_id)
                .ok_or("Party inventory item not found")?;
            if row.quantity <= 1 {
                return Ok(row_id);
            }
            row.quantity -= 1;
            let party_id = row.party_id.clone();
            let item_id = row.item_id.clone();
            ctx.db.party_inventory_item().id().update(row);
            Ok(ctx
                .db
                .party_inventory_item()
                .insert(crate::strategic::PartyInventoryItem {
                    id: 0,
                    party_id,
                    item_id,
                    quantity: 1,
                })
                .id)
        }
        _ => Err("Invalid inventory scope".into()),
    }
}

pub(crate) fn merge_empty_container(ctx: &ReducerContext, emptied_id: u64) -> Result<(), String> {
    let Some(emptied) = ctx.db.inventory_object().id().find(emptied_id) else {
        return Ok(());
    };
    let expected = crate::object_custody::carried_location_custody(
        &emptied.location_kind,
        &emptied.location_owner,
    )?;
    require_exact_carried_backing(ctx, &emptied, &expected)?;
    if !empty_container_can_stack(
        object_is_nonempty(ctx, emptied_id),
        object_is_nested(ctx, emptied_id),
        false,
    ) {
        return Ok(());
    }
    let mergeable_object = |row_id| -> Result<bool, String> {
        match object_for_row(ctx, &emptied.location_kind, row_id)? {
            Some(object) => {
                require_exact_carried_backing(ctx, &object, &expected)?;
                Ok(!object_is_nonempty(ctx, object.id) && !object_is_nested(ctx, object.id))
            }
            None => Ok(true),
        }
    };
    let merged_object_id;
    match emptied.location_kind.as_str() {
        "personal" => {
            let source = ctx
                .db
                .inventory_item()
                .id()
                .find(emptied.inventory_row_id)
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
            merged_object_id = object_for_row(ctx, "personal", target.id)?.map(|row| row.id);
            target.quantity = target
                .quantity
                .checked_add(source.quantity)
                .ok_or("Container stack quantity overflow")?;
            ctx.db.inventory_item().id().update(target);
            ctx.db.inventory_item().id().delete(source.id);
        }
        "party" => {
            let source = ctx
                .db
                .party_inventory_item()
                .id()
                .find(emptied.inventory_row_id)
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
            merged_object_id = object_for_row(ctx, "party", target.id)?.map(|row| row.id);
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
        let source = include_str!("inventory_container.rs");
        assert!(source.contains("parent_definition.container_capacity_ml == 0"));
        assert!(!source.contains("GENERIC_CONTAINER_IDS"));
    }

    #[test]
    fn physical_object_row_lookup_uses_the_compound_index() {
        let lookup = include_str!("inventory_container.rs")
            .split("pub(crate) fn object_for_row")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn object_is_nonempty").next())
            .expect("physical object row lookup");
        assert!(lookup.contains(".location_and_row()"));
        assert!(!lookup.contains(".inventory_object()\n        .iter()"));
    }

    #[test]
    fn empty_container_reconciliation_covers_removal_and_automatic_drain() {
        let source = include_str!("inventory_container.rs");
        let remove = source
            .split("pub fn remove_inventory_item_from_container")
            .nth(1)
            .unwrap()
            .split("pub fn pour_water_into_container")
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
        assert!(drain.contains("carried_location_custody"));
        assert!(drain.contains("resolve_object_custody"));
        assert!(drain.contains("merge_empty_container(ctx, liquid.container_object_id)"));
    }

    #[test]
    fn direct_object_mutators_require_resolved_actor_custody() {
        let source = include_str!("inventory_container.rs");
        for reducer in [
            "pub fn remove_inventory_item_from_container",
            "pub fn pour_water_out_of_container",
        ] {
            let body = source.split(reducer).nth(1).expect("direct object reducer");
            assert!(body.contains("require_actor_carried_object"));
        }
        assert!(source.contains("duplicate physical object identities"));
    }

    #[test]
    fn empty_container_merge_authenticates_exact_root_before_mutation() {
        let source = include_str!("inventory_container.rs");
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
        let source = include_str!("inventory_container.rs");
        let deletion = source
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
    fn public_water_handling_is_provenance_agnostic() {
        let source = include_str!("inventory_container.rs");
        assert!(!source.contains("pub fixture_drawn: bool"));
        assert!(!source.contains("Fixture-collected water cannot be mixed with pooled water"));
        assert!(source.contains("move_water_holding_contributions"));
        let deletion = source
            .split("pub(crate) fn delete_subtree")
            .nth(1)
            .unwrap()
            .split("fn require_exact_carried_backing")
            .next()
            .unwrap();
        assert_eq!(
            deletion
                .matches("delete_water_holding_contributions")
                .count(),
            1
        );
    }
}

pub(crate) fn ensure_object(
    ctx: &ReducerContext,
    character_id: u64,
    kind: &str,
    row_id: u64,
    split_stack: bool,
) -> Result<InventoryObject, String> {
    if let Some(object) = object_for_row(ctx, kind, row_id)? {
        let actor = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or("Character not found")?;
        crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
        return Ok(object);
    }
    let (item_id, owner, quantity) = require_owned_row(ctx, character_id, kind, row_id)?;
    let row_id = if split_stack && quantity > 1 {
        split_one(ctx, kind, row_id)?
    } else {
        row_id
    };
    let object = ctx.db.inventory_object().insert(InventoryObject {
        id: 0,
        item_id,
        location_kind: kind.into(),
        location_owner: owner,
        inventory_row_id: row_id,
    });
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
    let amount = match object.location_kind.as_str() {
        "personal" => ctx
            .db
            .inventory_item_amount()
            .inventory_item_id()
            .find(object.inventory_row_id)
            .map(|row| row.remaining_milliunits),
        "party" => ctx
            .db
            .party_item_amount()
            .party_inventory_item_id()
            .find(object.inventory_row_id)
            .map(|row| row.remaining_milliunits),
        _ => None,
    };
    let Some(amount) = amount else { return 0 };
    let full_ml = if definition.alcohol_serving_ml > 0 {
        definition.alcohol_serving_ml
    } else {
        definition.exterior_volume_ml
    };
    u64::from(full_ml) * u64::from(amount)
        / u64::from(crate::inventory_amount::FULL_AMOUNT_MILLIUNITS)
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

pub(crate) fn consume_contained_water(
    ctx: &ReducerContext,
    consumer_character_id: u64,
    kind: &str,
    owner: &str,
    requested_ml: u64,
) -> Result<u64, String> {
    let expected_custody = crate::object_custody::carried_location_custody(kind, owner)?;
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
        if resolved.root == expected_custody {
            liquids.push(liquid);
        }
    }
    liquids.sort_by_key(|liquid| liquid.container_object_id);
    let mut remaining = requested_ml;
    for mut liquid in liquids {
        let consumed = remaining.min(liquid.water_ml);
        crate::outbreak::consume_water_holding_contributions(
            ctx,
            "container",
            &liquid.container_object_id.to_string(),
            liquid.water_ml as f32,
            consumed as f32,
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
            .is_some_and(|object| object.location_kind == "fireplace")
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
    let parent = ensure_object(ctx, character_id, &parent_scope, parent_row_id, true)?;
    let parent_definition = ctx
        .db
        .item()
        .id()
        .find(parent.item_id.clone())
        .ok_or("Container definition not found")?;
    if parent_definition.container_capacity_ml == 0 {
        return Err("That item has no authored container capability".into());
    }
    let child = ensure_object(ctx, character_id, &child_scope, child_row_id, true)?;
    require_mutable(ctx, parent.id)?;
    require_mutable(ctx, child.id)?;
    if parent.location_kind != child.location_kind || parent.location_owner != child.location_owner
    {
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
pub fn pour_water_into_container(
    ctx: &ReducerContext,
    character_id: u64,
    parent_scope: String,
    parent_row_id: u64,
    requested_ml: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    if requested_ml == 0 {
        return Err("Water amount must be positive".into());
    }
    let parent = ensure_object(ctx, character_id, &parent_scope, parent_row_id, true)?;
    require_mutable(ctx, parent.id)?;
    require_container_capacity(ctx, parent.id, requested_ml)?;
    let available = match parent.location_kind.as_str() {
        "personal" => ctx
            .db
            .character_needs()
            .character_id()
            .find(character_id)
            .map_or(0, |row| row.carried_water_ml.max(0.0).floor() as u64),
        "party" => ctx
            .db
            .party_authority()
            .id()
            .find(parent.location_owner.clone())
            .map_or(0, |row| row.pooled_water_ml.max(0.0).floor() as u64),
        _ => 0,
    };
    if requested_ml > available {
        return Err("Not enough carried water".into());
    }
    let source_total_ml = match parent.location_kind.as_str() {
        "personal" => {
            let mut row = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .ok_or("Character needs not initialized")?;
            let total = row.carried_water_ml;
            row.carried_water_ml -= requested_ml as f32;
            ctx.db.character_needs().character_id().update(row);
            total
        }
        "party" => {
            let mut row = ctx
                .db
                .party_authority()
                .id()
                .find(parent.location_owner.clone())
                .ok_or("Party not found")?;
            let total = row.pooled_water_ml;
            row.pooled_water_ml -= requested_ml as f32;
            ctx.db.party_authority().id().update(row);
            total
        }
        _ => return Err("Cannot pour water into a container at this location".into()),
    };
    crate::outbreak::move_water_holding_contributions(
        ctx,
        parent.location_kind.as_str(),
        &parent.location_owner,
        "container",
        &parent.id.to_string(),
        (source_total_ml * 1_000.0).round() as u64,
        requested_ml.saturating_mul(1_000),
    )?;
    if let Some(mut liquid) = ctx
        .db
        .container_liquid()
        .container_object_id()
        .find(parent.id)
    {
        if liquid.liquid_item_id != WATER_ITEM_ID {
            return Err("Container already holds another liquid".into());
        }
        liquid.water_ml = liquid
            .water_ml
            .checked_add(requested_ml)
            .ok_or("Water volume overflow")?;
        ctx.db
            .container_liquid()
            .container_object_id()
            .update(liquid);
    } else {
        ctx.db.container_liquid().insert(ContainerLiquid {
            container_object_id: parent.id,
            liquid_item_id: WATER_ITEM_ID.into(),
            water_ml: requested_ml,
        });
    }
    Ok(())
}

#[reducer]
pub fn pour_water_out_of_container(
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
    let resolved = crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
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
    let (destination_kind, destination_id) = match &resolved.root {
        adventuresim_core::physical_object::OperationalCustody::Character(character) => {
            ("personal", character.get().to_string())
        }
        adventuresim_core::physical_object::OperationalCustody::Party(party) => {
            ("party", party.as_str().to_owned())
        }
        _ => return Err("Container belongs to another inventory".into()),
    };
    match &resolved.root {
        adventuresim_core::physical_object::OperationalCustody::Character(_) => {
            let mut row = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .ok_or("Character needs not initialized")?;
            if row.carried_water_ml + requested_ml as f32
                > crate::condition::water_capacity_ml(ctx, character_id) as f32
            {
                return Err("Personal waterskins do not have enough free capacity".into());
            }
            row.carried_water_ml += requested_ml as f32;
            ctx.db.character_needs().character_id().update(row);
        }
        adventuresim_core::physical_object::OperationalCustody::Party(party_id) => {
            let mut row = ctx
                .db
                .party_authority()
                .id()
                .find(party_id.as_str().to_string())
                .ok_or("Party not found")?;
            if row.pooled_water_ml + requested_ml as f32
                > crate::condition::party_water_capacity_ml(ctx, party_id.as_str()) as f32
            {
                return Err("Party waterskins do not have enough free capacity".into());
            }
            row.pooled_water_ml += requested_ml as f32;
            ctx.db.party_authority().id().update(row);
        }
        _ => return Err("Container belongs to another inventory".into()),
    }
    crate::outbreak::move_water_holding_contributions(
        ctx,
        "container",
        &container_object_id.to_string(),
        destination_kind,
        &destination_id,
        liquid.water_ml.saturating_mul(1_000),
        requested_ml.saturating_mul(1_000),
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
