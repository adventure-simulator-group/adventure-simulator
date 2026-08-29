//! Authoritative fixed-point remaining amounts for divisible inventory rows.

use spacetimedb::{ReducerContext, Table, table};

use adventuresim_core::inventory_measurement::ConsumableFractionMicros;

use crate::item::item;
use crate::{inventory_item, party_inventory_item};

pub const SMITHING_MATERIAL_IDS: [&str; 4] =
    ["steel_stock", "leather_stock", "brass_stock", "wood_stock"];

#[derive(Clone, Debug)]
#[table(accessor = inventory_item_amount, public)]
pub struct InventoryItemAmount {
    #[primary_key]
    pub inventory_item_id: u64,
    pub remaining_fraction_micros: u32,
}

#[derive(Clone, Debug)]
#[table(accessor = party_item_amount, public)]
pub struct PartyItemAmount {
    #[primary_key]
    pub party_inventory_item_id: u64,
    pub remaining_fraction_micros: u32,
}

pub fn is_measured_definition(definition: &crate::Item) -> bool {
    definition.kind == crate::PersistedItemKind::Food
        || definition.alcohol_serving_ml > 0
        || definition.id == adventuresim_core::item_references::SOFT_SOAP_ID
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
        remaining_fraction_micros: ConsumableFractionMicros::WHOLE.get(),
    });
}

pub fn initialize_party(ctx: &ReducerContext, party_inventory_item_id: u64) {
    ctx.db.party_item_amount().insert(PartyItemAmount {
        party_inventory_item_id,
        remaining_fraction_micros: ConsumableFractionMicros::WHOLE.get(),
    });
}

fn persisted_fraction(fraction_micros: u32) -> ConsumableFractionMicros {
    ConsumableFractionMicros::try_new(fraction_micros)
        .expect("persisted consumable fraction must not exceed one whole")
}

pub fn personal_fraction(
    ctx: &ReducerContext,
    inventory_item_id: u64,
) -> Option<ConsumableFractionMicros> {
    ctx.db
        .inventory_item_amount()
        .inventory_item_id()
        .find(inventory_item_id)
        .map(|row| persisted_fraction(row.remaining_fraction_micros))
}

pub fn party_fraction(
    ctx: &ReducerContext,
    party_inventory_item_id: u64,
) -> Option<ConsumableFractionMicros> {
    ctx.db
        .party_item_amount()
        .party_inventory_item_id()
        .find(party_inventory_item_id)
        .map(|row| persisted_fraction(row.remaining_fraction_micros))
}

pub fn consume_personal(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    requested_fraction: ConsumableFractionMicros,
) -> Result<ConsumableFractionMicros, String> {
    if requested_fraction.is_zero() {
        return Ok(ConsumableFractionMicros::ZERO);
    }
    let mut state = ctx
        .db
        .inventory_item_amount()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or("Measured personal item state is missing")?;
    let available = persisted_fraction(state.remaining_fraction_micros);
    let consumed = requested_fraction.min(available);
    let remaining = available
        .checked_sub(consumed)
        .expect("consumed fraction cannot exceed available fraction");
    state.remaining_fraction_micros = remaining.get();
    if remaining.is_zero() {
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
    requested_fraction: ConsumableFractionMicros,
) -> Result<ConsumableFractionMicros, String> {
    if requested_fraction.is_zero() {
        return Ok(ConsumableFractionMicros::ZERO);
    }
    let mut state = ctx
        .db
        .party_item_amount()
        .party_inventory_item_id()
        .find(party_inventory_item_id)
        .ok_or("Measured party item state is missing")?;
    let available = persisted_fraction(state.remaining_fraction_micros);
    let consumed = requested_fraction.min(available);
    let remaining = available
        .checked_sub(consumed)
        .expect("consumed fraction cannot exceed available fraction");
    state.remaining_fraction_micros = remaining.get();
    if remaining.is_zero() {
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
        remaining_fraction_micros: state.remaining_fraction_micros,
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
        remaining_fraction_micros: state.remaining_fraction_micros,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_divisible_consumable_definitions_are_measured() {
        let food = crate::Item {
            kind: crate::PersistedItemKind::Food,
            ..crate::Item::default()
        };
        let alcohol = crate::Item {
            alcohol_serving_ml: 500,
            ..crate::Item::default()
        };
        let soap = crate::Item {
            id: adventuresim_core::item_references::SOFT_SOAP_ID.into(),
            ..crate::Item::default()
        };
        let sword = crate::Item {
            id: "sword".into(),
            kind: crate::PersistedItemKind::Weapon,
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
