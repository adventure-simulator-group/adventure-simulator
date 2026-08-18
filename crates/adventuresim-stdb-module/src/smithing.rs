//! Authoritative recipe-driven smithing transactions.

use adventuresim_weapon_model::{
    MaterialClass, WeaponDesign, decode, derive_material_masses, derive_properties,
};
use spacetimedb::{ReducerContext, reducer};

use crate::character::character;
use crate::inventory_amount::inventory_item_amount;
use crate::inventory_container::inventory_object;
use crate::item::{add_inventory_item_checked, inventory_item};

pub const FORGE_MINUTES_BASE: u64 = 60;

fn stock_id(material: MaterialClass) -> &'static str {
    match material {
        MaterialClass::Steel | MaterialClass::DarkSteel => "steel_stock",
        MaterialClass::Leather | MaterialClass::DarkLeather => "leather_stock",
        MaterialClass::Brass => "brass_stock",
        MaterialClass::Wood => "wood_stock",
    }
}

fn stock_unit_kg(item_id: &str) -> f32 {
    match item_id {
        "steel_stock" => 5.0,
        "leather_stock" | "brass_stock" => 2.0,
        "wood_stock" => 4.0,
        _ => 1.0,
    }
}

pub fn forge_material_requirements(design: &WeaponDesign) -> Result<Vec<(String, u32)>, String> {
    let mut requirements = std::collections::BTreeMap::<String, u32>::new();
    for mass in derive_material_masses(design).map_err(|errors| format!("{errors:?}"))? {
        let item_id = stock_id(mass.material);
        let milliunits = (mass.mass_kg / stock_unit_kg(item_id) * 1_000_000.0).ceil();
        if !milliunits.is_finite() || milliunits > u32::MAX as f32 {
            return Err("Weapon material requirement is outside the supported range".into());
        }
        requirements
            .entry(item_id.into())
            .and_modify(|total| *total = total.saturating_add(milliunits as u32))
            .or_insert(milliunits as u32);
    }
    Ok(requirements.into_iter().collect())
}

pub fn forge_minutes(design: &WeaponDesign) -> Result<u64, String> {
    let physical = derive_properties(design).map_err(|errors| format!("{errors:?}"))?;
    Ok(FORGE_MINUTES_BASE
        + (physical.mass_kg * 120.0).ceil() as u64
        + design.components.len() as u64 * 12)
}

fn available(ctx: &ReducerContext, character_id: u64, item_id: &str) -> u32 {
    ctx.db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|row| row.item_id == item_id)
        .filter_map(|row| {
            ctx.db
                .inventory_item_amount()
                .inventory_item_id()
                .find(row.id)
                .map(|amount| amount.remaining_milliunits)
        })
        .fold(0_u32, u32::saturating_add)
}

fn consume(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    requested: u32,
) -> Result<(), String> {
    let mut rows = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|row| row.item_id == item_id)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.id);
    let mut remaining = requested;
    for row in rows {
        let consumed = crate::inventory_amount::consume_personal(ctx, row.id, remaining)?;
        remaining -= consumed;
        if remaining == 0 {
            return Ok(());
        }
    }
    Err(format!("Insufficient {item_id}"))
}

#[reducer]
pub fn forge_weapon(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    recipe: Vec<u8>,
) -> Result<(), String> {
    if recipe.len() > crate::weapon_instance::MAX_WEAPON_RECIPE_BYTES {
        return Err("Weapon recipe exceeds the smithing limit".into());
    }
    let design = decode(&recipe).map_err(|error| format!("Invalid weapon recipe: {error}"))?;
    if !adventuresim_weapon_model::MELEE_CATALOG_IDS.contains(&design.catalog_id.as_str()) {
        return Err("Weapon recipe uses an unsupported chassis".into());
    }
    let requirements = forge_material_requirements(&design)?;
    let minutes = forge_minutes(&design)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Character must be at this forge".into());
    }
    use adventuresim_world_schema::SettlementService as Service;
    let economy_has_forge =
        crate::strategic::require_settlement_service(ctx, &settlement_id, Service::Weaponsmith)
            .is_ok()
            || crate::strategic::require_settlement_service(
                ctx,
                &settlement_id,
                Service::GeneralBlacksmith,
            )
            .is_ok();
    let organization_has_forge =
        adventuresim_core::organization::organization_service_chapter(&settlement_id, "weapons")
            .is_some();
    if !economy_has_forge && !organization_has_forge {
        return Err("This settlement has no weaponsmith forge".into());
    }
    for (item_id, required) in &requirements {
        if available(ctx, character_id, item_id) < *required {
            return Err(format!(
                "Insufficient {item_id}: {required} milliunits required"
            ));
        }
    }
    for (item_id, required) in &requirements {
        consume(ctx, character_id, item_id, *required)?;
    }
    if !crate::time::advance_character_wait_time(ctx, character_id, minutes)? {
        return Err("The smithing session could not be completed".into());
    }
    let inventory_id = add_inventory_item_checked(ctx, character_id, &design.catalog_id, 1)?
        .ok_or("Could not create forged weapon")?;
    let object = ctx
        .db
        .inventory_object()
        .location_and_row()
        .filter(("personal", inventory_id))
        .next()
        .ok_or("Forged weapon has no physical identity")?;
    crate::weapon_instance::replace_design(ctx, object.id, &design)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_drive_generic_material_and_time_quotes() {
        let short = adventuresim_weapon_model::default_design("rondel_dagger").unwrap();
        let long = adventuresim_weapon_model::default_design("longsword").unwrap();
        assert_ne!(
            forge_material_requirements(&short).unwrap(),
            forge_material_requirements(&long).unwrap()
        );
        assert_ne!(
            forge_minutes(&short).unwrap(),
            forge_minutes(&long).unwrap()
        );
    }
}
