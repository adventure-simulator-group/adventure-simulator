//! Durable parametric weapon recipes keyed by stable physical-object identity.

use adventuresim_core::physical_object::{CarriedInventoryScope, InventoryLocation};
use adventuresim_weapon_model::{
    GENERATOR_VERSION, HOLDER_GENERATOR_VERSION, WeaponDesign, WeaponHolderDesign, decode,
    decode_holder, default_design, default_holder_design, derive_holder_properties,
    derive_properties, design_hash, encode, encode_holder, holder_design_hash,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};

use crate::inventory_container::inventory_object__view;
use crate::item::item;
use crate::strategic::PartyInventoryItem;
use crate::strategic::strategic_gateway_authority__view;
use crate::{InventoryItem, ItemKind, inventory_object};

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

#[derive(Clone, Debug, PartialEq)]
#[table(accessor = weapon_holder_instance)]
pub struct WeaponHolderInstance {
    #[primary_key]
    pub physical_object_id: u64,
    pub generator_version: u16,
    pub design_hash: Vec<u8>,
    pub recipe: Vec<u8>,
    pub mass_grams: u32,
    pub length_mm: u32,
    pub grip_to_tip_mm: u32,
}

/// Trusted strategic-backend projection used to produce per-instance inventory
/// icons. Recipes remain absent for ordinary player subscriptions and HTML.
#[view(accessor = backend_weapon_instances, public)]
pub fn backend_weapon_instances(ctx: &ViewContext) -> Vec<WeaponInstance> {
    let gateway = ctx.db.strategic_gateway_authority().id().find(0);
    if gateway.is_none_or(|authority| authority.identity != ctx.sender()) {
        return Vec::new();
    }
    let mut instances = Vec::new();
    for object in ctx
        .db
        .inventory_object()
        .item_id()
        .filter(""..)
        .filter(|object| {
            matches!(
                &object.location,
                InventoryLocation::Personal(_) | InventoryLocation::Party(_)
            )
        })
    {
        if let Some(instance) = ctx
            .db
            .weapon_instance()
            .physical_object_id()
            .find(object.id)
        {
            instances.push(instance);
        }
    }
    instances
}

/// Trusted strategic-backend projection for independently persisted scabbard
/// and haft-loop recipes. Ordinary players still receive holder recipes only
/// through the sender-scoped tactical projection.
#[view(accessor = backend_weapon_holder_instances, public)]
pub fn backend_weapon_holder_instances(ctx: &ViewContext) -> Vec<WeaponHolderInstance> {
    let gateway = ctx.db.strategic_gateway_authority().id().find(0);
    if gateway.is_none_or(|authority| authority.identity != ctx.sender()) {
        return Vec::new();
    }
    let mut instances = Vec::new();
    for object in ctx
        .db
        .inventory_object()
        .item_id()
        .filter(""..)
        .filter(|object| {
            matches!(
                &object.location,
                InventoryLocation::Personal(_) | InventoryLocation::Party(_)
            )
        })
    {
        if let Some(instance) = ctx
            .db
            .weapon_holder_instance()
            .physical_object_id()
            .find(object.id)
        {
            instances.push(instance);
        }
    }
    instances
}

/// Sender-scoped tactical transport. Strategic HTML likewise never embeds the
/// full smithing recipe; its trusted icon endpoint consumes the backend view.
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

fn holder_instance_for_design(
    physical_object_id: u64,
    design: &WeaponHolderDesign,
) -> Result<WeaponHolderInstance, String> {
    let recipe = encode_holder(design).map_err(|error| error.to_string())?;
    if recipe.len() > MAX_WEAPON_RECIPE_BYTES {
        return Err("Weapon holder recipe exceeds the tactical transport limit".into());
    }
    let derived = derive_holder_properties(design).map_err(|errors| format!("{errors:?}"))?;
    Ok(WeaponHolderInstance {
        physical_object_id,
        generator_version: HOLDER_GENERATOR_VERSION,
        design_hash: holder_design_hash(design).0.to_vec(),
        recipe,
        mass_grams: checked_scaled_u32(derived.mass_kg, 1_000.0, "holder mass")?.max(1),
        length_mm: checked_scaled_u32(derived.length_m, 1_000.0, "holder length")?.max(1),
        grip_to_tip_mm: checked_scaled_u32(
            derived.grip_to_tip_m,
            1_000.0,
            "holder anchor-to-tip distance",
        )?,
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
    let object = crate::inventory_container::object_for_row(
        ctx,
        CarriedInventoryScope::Personal,
        inventory.id,
    )?
    .ok_or("Parametric weapon has no stable physical object identity")?;
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
    let object = crate::inventory_container::object_for_row(
        ctx,
        CarriedInventoryScope::Party,
        inventory.id,
    )?
    .ok_or("Parametric party weapon has no stable physical object identity")?;
    replace_design(ctx, object.id, &design)?;
    Ok(())
}

pub(crate) fn delete_for_object(ctx: &ReducerContext, physical_object_id: u64) {
    ctx.db
        .weapon_instance()
        .physical_object_id()
        .delete(physical_object_id);
    ctx.db
        .weapon_holder_instance()
        .physical_object_id()
        .delete(physical_object_id);
}

pub(crate) fn fit_personal_holder(
    ctx: &ReducerContext,
    character_id: u64,
    holder_inventory_row_id: u64,
    weapon_inventory_row_id: u64,
) -> Result<(), String> {
    let holder_object = crate::inventory_container::require_object(
        ctx,
        character_id,
        CarriedInventoryScope::Personal,
        holder_inventory_row_id,
    )?;
    let weapon_object = crate::inventory_container::object_for_row(
        ctx,
        CarriedInventoryScope::Personal,
        weapon_inventory_row_id,
    )?
    .ok_or("Fitted weapon has no physical identity")?;
    let weapon = ctx
        .db
        .weapon_instance()
        .physical_object_id()
        .find(weapon_object.id)
        .ok_or("Fitted weapon has no parametric recipe")?;
    if !valid_instance(&weapon, &weapon_object.item_id) {
        return Err("Fitted weapon recipe is invalid".into());
    }
    let expected_holder =
        match adventuresim_weapon_model::recommended_holder(&weapon_object.item_id) {
            Some(adventuresim_weapon_model::WeaponHolderKind::BladeSheath) => "scabbard",
            Some(adventuresim_weapon_model::WeaponHolderKind::HaftLoop) => "weapon_loop",
            None => return Err("Polearms cannot be fitted to a body-mounted holder".into()),
        };
    if holder_object.item_id != expected_holder {
        return Err(format!(
            "{} requires a {expected_holder}",
            weapon_object.item_id
        ));
    }
    let weapon_design = decode(&weapon.recipe).map_err(|error| error.to_string())?;
    let holder_design =
        default_holder_design(&weapon_design).ok_or("Weapon has no procedural holder template")?;
    replace_holder_design(ctx, holder_object.id, &holder_design)?;
    Ok(())
}

/// Atomic holder-design replacement seam for a future leatherworking/smithing
/// reducer. Holder parameters and the fitted weapon snapshot are independently
/// versioned; changing the weapon does not silently rewrite an existing holder.
pub(crate) fn replace_holder_design(
    ctx: &ReducerContext,
    physical_object_id: u64,
    design: &WeaponHolderDesign,
) -> Result<(), String> {
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(physical_object_id)
        .ok_or("Holder physical object not found")?;
    if object.item_id != design.catalog_id {
        return Err("Holder design chassis does not match its inventory object".into());
    }
    let fit = holder_instance_for_design(physical_object_id, design)?;
    if ctx
        .db
        .weapon_holder_instance()
        .physical_object_id()
        .find(physical_object_id)
        .is_some()
    {
        ctx.db
            .weapon_holder_instance()
            .physical_object_id()
            .update(fit);
    } else {
        ctx.db.weapon_holder_instance().insert(fit);
    }
    Ok(())
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
            .update(instance.clone());
    } else {
        ctx.db.weapon_instance().insert(instance.clone());
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
        .item_id()
        .filter(""..)
        .filter(|object| {
            matches!(
                &object.location,
                InventoryLocation::Personal(location) if location.row_id == inventory_row_id
            )
        })
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

pub(crate) fn connected_holder_appearance(
    ctx: &ViewContext,
    inventory_row_id: u64,
    item_id: &str,
) -> Option<ConnectedWeaponAppearance> {
    if !matches!(item_id, "scabbard" | "weapon_loop") {
        return None;
    }
    let mut objects = ctx
        .db
        .inventory_object()
        .item_id()
        .filter(""..)
        .filter(|object| {
            matches!(
                &object.location,
                InventoryLocation::Personal(location) if location.row_id == inventory_row_id
            )
        })
        .filter(|object| object.item_id == item_id);
    let object = objects.next()?;
    if objects.next().is_some() {
        return None;
    }
    let holder = ctx
        .db
        .weapon_holder_instance()
        .physical_object_id()
        .find(object.id)?;
    if holder.generator_version != HOLDER_GENERATOR_VERSION
        || holder.design_hash.len() != 32
        || holder.recipe.len() > MAX_WEAPON_RECIPE_BYTES
    {
        return None;
    }
    let design = decode_holder(&holder.recipe).ok()?;
    if design.catalog_id != item_id
        || holder_design_hash(&design).0.as_slice() != holder.design_hash
    {
        return None;
    }
    let derived = derive_holder_properties(&design).ok()?;
    if checked_scaled_u32(derived.mass_kg, 1_000.0, "holder mass")
        .ok()?
        .max(1)
        != holder.mass_grams
        || checked_scaled_u32(derived.length_m, 1_000.0, "holder length")
            .ok()?
            .max(1)
            != holder.length_mm
        || checked_scaled_u32(
            derived.grip_to_tip_m,
            1_000.0,
            "holder anchor-to-tip distance",
        )
        .ok()?
            != holder.grip_to_tip_mm
    {
        return None;
    }
    Some(ConnectedWeaponAppearance {
        generator_version: holder.generator_version,
        design_hash: holder.design_hash,
        recipe: holder.recipe,
        mass_kg: holder.mass_grams as f32 / 1_000.0,
        length_m: holder.length_mm as f32 / 1_000.0,
        grip_to_tip_m: holder.grip_to_tip_mm as f32 / 1_000.0,
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
                adventuresim_weapon_model::ComponentShape::Blade(blade) => Some(blade),
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

    #[test]
    fn same_holder_template_can_retain_distinct_per_object_recipes() {
        let weapon = default_design("longsword").unwrap();
        let first = default_holder_design(&weapon).unwrap();
        let mut second = first.clone();
        second.clearance.0 += 2;
        second.chape_length.0 += 6;
        let first = holder_instance_for_design(20, &first).unwrap();
        let second = holder_instance_for_design(21, &second).unwrap();
        assert_ne!(first.physical_object_id, second.physical_object_id);
        assert_ne!(first.design_hash, second.design_hash);
        assert_ne!(first.recipe, second.recipe);
        assert_ne!(first.length_mm, second.length_mm);
    }
}
