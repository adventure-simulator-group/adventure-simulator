//! Durable parametric weapon recipes keyed by stable physical-object identity.

use adventuresim_weapon_model::{
    GENERATOR_VERSION, WeaponDesign, decode, default_design, derive_properties, design_hash, encode,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table};

use crate::inventory_container::inventory_object__view;
use crate::item::item;
use crate::strategic::PartyInventoryItem;
use crate::{InventoryItem, InventoryObject, ItemKind, inventory_object};

pub const MAX_WEAPON_RECIPE_BYTES: usize = 16 * 1024;

fn checked_scaled_u32(value: f32, scale: f32, label: &str) -> Result<u32, String> {
    let scaled = value * scale;
    if !scaled.is_finite() || scaled < 0.0 || scaled > u32::MAX as f32 {
        return Err(format!("Weapon {label} is outside the persisted range"));
    }
    Ok(scaled.round() as u32)
}

#[derive(Clone, Debug, PartialEq)]
#[table(accessor = weapon_instance)]
pub struct WeaponInstance {
    #[primary_key]
    pub physical_object_id: u64,
    pub generator_version: u16,
    pub design_hash: Vec<u8>,
    pub recipe: Vec<u8>,
    pub mass_grams: u32,
    pub length_mm: u32,
    pub grip_to_tip_mm: u32,
}

/// Sender-scoped tactical transport. The strategic inventory UI does not
/// subscribe to the private instance table or expose its smithing parameters.
#[derive(SpacetimeType, Clone, Debug)]
pub struct ConnectedWeaponAppearance {
    pub generator_version: u16,
    pub design_hash: Vec<u8>,
    pub recipe: Vec<u8>,
    pub mass_kg: f32,
    pub length_m: f32,
    pub grip_to_tip_m: f32,
}

fn instance_for_design(
    physical_object_id: u64,
    design: &WeaponDesign,
) -> Result<WeaponInstance, String> {
    let derived = derive_properties(design).map_err(|errors| format!("{errors:?}"))?;
    let recipe = encode(design).map_err(|error| error.to_string())?;
    if recipe.len() > MAX_WEAPON_RECIPE_BYTES {
        return Err("Weapon recipe exceeds the tactical transport limit".into());
    }
    let mass_grams = checked_scaled_u32(derived.mass_kg, 1_000.0, "mass")?.max(1);
    let length_mm = checked_scaled_u32(derived.length_m, 1_000.0, "length")?.max(1);
    let grip_to_tip_mm =
        checked_scaled_u32(derived.grip_to_tip_m, 1_000.0, "grip-to-tip distance")?;
    Ok(WeaponInstance {
        physical_object_id,
        generator_version: GENERATOR_VERSION,
        design_hash: design_hash(design).0.to_vec(),
        recipe,
        mass_grams,
        length_mm,
        grip_to_tip_mm,
    })
}

fn insert_object(
    ctx: &ReducerContext,
    item_id: &str,
    location_kind: &str,
    location_owner: String,
    inventory_row_id: u64,
) -> InventoryObject {
    ctx.db.inventory_object().insert(InventoryObject {
        id: 0,
        item_id: item_id.into(),
        location_kind: location_kind.into(),
        location_owner,
        inventory_row_id,
    })
}

pub(crate) fn initialize_personal_weapon(
    ctx: &ReducerContext,
    inventory: &InventoryItem,
) -> Result<(), String> {
    let Some(definition) = ctx.db.item().id().find(inventory.item_id.clone()) else {
        return Err(format!("Unknown weapon definition {}", inventory.item_id));
    };
    if definition.kind != ItemKind::Weapon || !definition.melee {
        return Ok(());
    }
    let Some(design) = default_design(&inventory.item_id) else {
        return Ok(());
    };
    if inventory.quantity != 1 {
        return Err("Parametric weapons must be individual inventory rows".into());
    }
    let object = insert_object(
        ctx,
        &inventory.item_id,
        "personal",
        inventory.character_id.to_string(),
        inventory.id,
    );
    replace_design(ctx, object.id, &design)?;
    Ok(())
}

pub(crate) fn initialize_party_weapon(
    ctx: &ReducerContext,
    inventory: &PartyInventoryItem,
) -> Result<(), String> {
    let Some(definition) = ctx.db.item().id().find(inventory.item_id.clone()) else {
        return Err(format!("Unknown weapon definition {}", inventory.item_id));
    };
    if definition.kind != ItemKind::Weapon || !definition.melee {
        return Ok(());
    }
    let Some(design) = default_design(&inventory.item_id) else {
        return Ok(());
    };
    if inventory.quantity != 1 {
        return Err("Parametric weapons must be individual party inventory rows".into());
    }
    let object = insert_object(
        ctx,
        &inventory.item_id,
        "party",
        inventory.party_id.clone(),
        inventory.id,
    );
    replace_design(ctx, object.id, &design)?;
    Ok(())
}

pub(crate) fn delete_for_object(ctx: &ReducerContext, physical_object_id: u64) {
    ctx.db
        .weapon_instance()
        .physical_object_id()
        .delete(physical_object_id);
}

/// Atomic design replacement seam for the future smithing reducer. Authority,
/// material consumption, skill checks, price, and elapsed time belong to that
/// reducer; this function owns recipe validation and derived-property refresh.
pub(crate) fn replace_design(
    ctx: &ReducerContext,
    physical_object_id: u64,
    design: &WeaponDesign,
) -> Result<(), String> {
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(physical_object_id)
        .ok_or("Weapon physical object not found")?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(object.item_id.clone())
        .ok_or("Weapon catalog definition not found")?;
    if definition.kind != ItemKind::Weapon || !definition.melee {
        return Err("Only melee weapon objects accept parametric designs".into());
    }
    if design.catalog_id != object.item_id {
        return Err("Weapon design chassis does not match its inventory object".into());
    }
    let instance = instance_for_design(physical_object_id, design)?;
    if ctx
        .db
        .weapon_instance()
        .physical_object_id()
        .find(physical_object_id)
        .is_some()
    {
        ctx.db
            .weapon_instance()
            .physical_object_id()
            .update(instance);
    } else {
        ctx.db.weapon_instance().insert(instance);
    }
    Ok(())
}

fn valid_instance(instance: &WeaponInstance, expected_catalog_id: &str) -> bool {
    if instance.generator_version != GENERATOR_VERSION
        || instance.design_hash.len() != 32
        || instance.recipe.len() > MAX_WEAPON_RECIPE_BYTES
    {
        return false;
    }
    let Ok(design) = decode(&instance.recipe) else {
        return false;
    };
    if design.catalog_id != expected_catalog_id {
        return false;
    }
    instance_for_design(instance.physical_object_id, &design)
        .is_ok_and(|expected| expected == *instance)
}

pub(crate) fn connected_appearance(
    ctx: &ViewContext,
    inventory_row_id: u64,
    item_id: &str,
) -> Option<ConnectedWeaponAppearance> {
    let mut objects = ctx
        .db
        .inventory_object()
        .location_and_row()
        .filter(("personal", inventory_row_id))
        .filter(|object| object.item_id == item_id);
    let object = objects.next()?;
    if objects.next().is_some() {
        return None;
    }
    let instance = ctx
        .db
        .weapon_instance()
        .physical_object_id()
        .find(object.id)?;
    if !valid_instance(&instance, item_id) {
        return None;
    }
    Some(ConnectedWeaponAppearance {
        generator_version: instance.generator_version,
        design_hash: instance.design_hash,
        recipe: instance.recipe,
        mass_kg: instance.mass_grams as f32 / 1_000.0,
        length_m: instance.length_mm as f32 / 1_000.0,
        grip_to_tip_m: instance.grip_to_tip_mm as f32 / 1_000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_projection_round_trips_and_rejects_tampering() {
        let design = default_design("longsword").expect("longsword recipe");
        let mut instance = instance_for_design(42, &design).expect("instance");
        assert!(valid_instance(&instance, "longsword"));
        instance.recipe[0] ^= 0x55;
        assert!(!valid_instance(&instance, "longsword"));
    }

    #[test]
    fn same_catalog_weapon_rows_can_retain_distinct_recipes() {
        let first = default_design("longsword").expect("longsword recipe");
        let mut second = first.clone();
        let blade = second
            .components
            .iter_mut()
            .find_map(|component| match &mut component.shape {
                adventuresim_weapon_model::ComponentShape::SectionBlade(blade) => Some(blade),
                _ => None,
            })
            .expect("longsword should have a section blade");
        blade.length.0 += 25;
        let first = instance_for_design(10, &first).unwrap();
        let second = instance_for_design(11, &second).unwrap();
        assert_ne!(first.physical_object_id, second.physical_object_id);
        assert_ne!(first.design_hash, second.design_hash);
        assert_ne!(first.recipe, second.recipe);
    }
}
