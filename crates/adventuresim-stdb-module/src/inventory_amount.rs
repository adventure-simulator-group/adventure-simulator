//! Authoritative fixed-point remaining amounts for divisible inventory rows.

use spacetimedb::{ReducerContext, Table, table};

use crate::item::item;
use crate::{inventory_item, party_inventory_item};

pub use adventuresim_core::inventory_measurement::FULL_AMOUNT_MILLIUNITS;

pub const SMITHING_MATERIAL_IDS: [&str; 4] =
    ["steel_stock", "leather_stock", "brass_stock", "wood_stock"];

#[derive(Clone, Debug)]
#[table(accessor = inventory_item_amount, public)]
pub struct InventoryItemAmount {
    #[primary_key]
    pub inventory_item_id: u64,
    pub remaining_milliunits: u32,
}

#[derive(Clone, Debug)]
#[table(accessor = party_item_amount, public)]
pub struct PartyItemAmount {
    #[primary_key]
    pub party_inventory_item_id: u64,
    pub remaining_milliunits: u32,
}

pub fn is_measured_definition(definition: &crate::Item) -> bool {
    definition.kind == crate::ItemKind::Food
        || definition.alcohol_serving_ml > 0
        || definition.id == crate::filth::SOAP_ITEM_ID
        || SMITHING_MATERIAL_IDS.contains(&definition.id.as_str())
}

pub fn is_measured_item(ctx: &ReducerContext, item_id: &str) -> bool {
    ctx.db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|definition| is_measured_definition(&definition))
        || adventuresim_core::food::definition(item_id).is_some()
}

pub fn initialize_personal(ctx: &ReducerContext, inventory_item_id: u64) {
    ctx.db.inventory_item_amount().insert(InventoryItemAmount {
        inventory_item_id,
        remaining_milliunits: FULL_AMOUNT_MILLIUNITS,
    });
}

pub fn initialize_party(ctx: &ReducerContext, party_inventory_item_id: u64) {
    ctx.db.party_item_amount().insert(PartyItemAmount {
        party_inventory_item_id,
        remaining_milliunits: FULL_AMOUNT_MILLIUNITS,
    });
}

pub fn personal_amount(ctx: &ReducerContext, inventory_item_id: u64) -> Option<u32> {
    ctx.db
        .inventory_item_amount()
        .inventory_item_id()
        .find(inventory_item_id)
        .map(|row| row.remaining_milliunits)
}

pub fn party_amount(ctx: &ReducerContext, party_inventory_item_id: u64) -> Option<u32> {
    ctx.db
        .party_item_amount()
        .party_inventory_item_id()
        .find(party_inventory_item_id)
        .map(|row| row.remaining_milliunits)
}

pub fn consume_personal(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    requested_milliunits: u32,
) -> Result<u32, String> {
    if requested_milliunits == 0 {
        return Ok(0);
    }
    let mut state = ctx
        .db
        .inventory_item_amount()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or("Measured personal item state is missing")?;
    let consumed = requested_milliunits.min(state.remaining_milliunits);
    state.remaining_milliunits -= consumed;
    if state.remaining_milliunits == 0 {
        ctx.db
            .inventory_item_amount()
            .inventory_item_id()
            .delete(inventory_item_id);
        ctx.db.inventory_item().id().delete(inventory_item_id);
    } else {
        ctx.db
            .inventory_item_amount()
            .inventory_item_id()
            .update(state);
    }
    Ok(consumed)
}

pub fn consume_party(
    ctx: &ReducerContext,
    party_inventory_item_id: u64,
    requested_milliunits: u32,
) -> Result<u32, String> {
    if requested_milliunits == 0 {
        return Ok(0);
    }
    let mut state = ctx
        .db
        .party_item_amount()
        .party_inventory_item_id()
        .find(party_inventory_item_id)
        .ok_or("Measured party item state is missing")?;
    let consumed = requested_milliunits.min(state.remaining_milliunits);
    state.remaining_milliunits -= consumed;
    if state.remaining_milliunits == 0 {
        ctx.db
            .party_item_amount()
            .party_inventory_item_id()
            .delete(party_inventory_item_id);
        ctx.db
            .party_inventory_item()
            .id()
            .delete(party_inventory_item_id);
    } else {
        ctx.db
            .party_item_amount()
            .party_inventory_item_id()
            .update(state);
    }
    Ok(consumed)
}

pub fn move_personal_to_party(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    party_inventory_item_id: u64,
) -> Result<(), String> {
    let state = ctx
        .db
        .inventory_item_amount()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or("Measured personal item state is missing")?;
    ctx.db
        .inventory_item_amount()
        .inventory_item_id()
        .delete(inventory_item_id);
    ctx.db.party_item_amount().insert(PartyItemAmount {
        party_inventory_item_id,
        remaining_milliunits: state.remaining_milliunits,
    });
    Ok(())
}

pub fn move_party_to_personal(
    ctx: &ReducerContext,
    party_inventory_item_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    let state = ctx
        .db
        .party_item_amount()
        .party_inventory_item_id()
        .find(party_inventory_item_id)
        .ok_or("Measured party item state is missing")?;
    ctx.db
        .party_item_amount()
        .party_inventory_item_id()
        .delete(party_inventory_item_id);
    ctx.db.inventory_item_amount().insert(InventoryItemAmount {
        inventory_item_id,
        remaining_milliunits: state.remaining_milliunits,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_divisible_consumable_definitions_are_measured() {
        let food = crate::Item {
            kind: crate::ItemKind::Food,
            ..crate::Item::default()
        };
        let alcohol = crate::Item {
            alcohol_serving_ml: 500,
            ..crate::Item::default()
        };
        let soap = crate::Item {
            id: crate::filth::SOAP_ITEM_ID.into(),
            ..crate::Item::default()
        };
        let sword = crate::Item {
            id: "sword".into(),
            kind: crate::ItemKind::Weapon,
            ..crate::Item::default()
        };
        let steel = crate::Item {
            id: "steel_stock".into(),
            ..crate::Item::default()
        };

        assert!(is_measured_definition(&food));
        assert!(is_measured_definition(&alcohol));
        assert!(is_measured_definition(&soap));
        assert!(!is_measured_definition(&sword));
        assert!(is_measured_definition(&steel));
    }
}
