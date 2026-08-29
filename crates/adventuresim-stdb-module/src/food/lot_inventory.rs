// Owns food-lot creation, splitting, transfer, and contamination provenance.
fn current_minute(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |row| row.minutes)
}

fn ensure_food_material_object(
    ctx: &ReducerContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Result<crate::InventoryObject, String> {
    let (item_id, location, quantity) = match scope {
        CarriedInventoryScope::Personal => {
            let row = ctx
                .db
                .inventory_item()
                .id()
                .find(row_id)
                .ok_or("Food inventory row is missing")?;
            (
                row.item_id,
                InventoryLocation::personal(row.character_id, row.id),
                row.quantity,
            )
        }
        CarriedInventoryScope::Party => {
            let row = ctx
                .db
                .party_inventory_item()
                .id()
                .find(row_id)
                .ok_or("Party food inventory row is missing")?;
            (
                row.item_id,
                InventoryLocation::party(row.party_id, row.id),
                row.quantity,
            )
        }
    };
    if quantity != 1 {
        return Err("Every food lot requires a quantity-one stable inventory object".into());
    }
    if let Some(object) = crate::inventory_container::object_for_row(ctx, scope, row_id)? {
        if object.item_id != item_id || object.location != location {
            return Err("Food inventory row has a mismatched stable object identity".into());
        }
        return Ok(object);
    }
    Ok(ctx.db.inventory_object().insert(crate::InventoryObject {
        id: 0,
        item_id,
        location,
    }))
}

pub fn create_personal_food_lot(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    item_id: &str,
    quantity: u32,
) -> Result<FoodLot, String> {
    let definition = food::definition(item_id).ok_or("Food definition not found")?;
    let minute = current_minute(ctx, character_id);
    ensure_food_material_object(ctx, CarriedInventoryScope::Personal, inventory_item_id)?;
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: Some(inventory_item_id),
        party_inventory_item_id: None,
        material_revision: 1,
        display_name: definition.name.into(),
        preparation: if definition.class == food::FoodClass::Ration {
            FoodPreparation::Preserved
        } else {
            FoodPreparation::Raw
        },
        ingredient_item_ids: vec![item_id.into()],
        ingredient_quantities: vec![quantity as f32],
        salty_kg: definition.flavors_per_unit.salty * quantity as f32,
        spicy_kg: definition.flavors_per_unit.spicy * quantity as f32,
        sweet_kg: definition.flavors_per_unit.sweet * quantity as f32,
        sour_kg: definition.flavors_per_unit.sour * quantity as f32,
        savory_kg: definition.flavors_per_unit.savory * quantity as f32,
        quality: definition.default_quality.clamp(1, 5),
        mass_kg: definition.mass_kg_per_unit * quantity as f32,
        nutrition_kcal: definition.kcal_per_unit * quantity as f32,
        total_value: definition.value_per_unit * quantity as f32,
        created_at_minute: minute,
    });
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: lot.id,
        concentration_anchor: food::deterministic_initial_contamination(
            ctx.random::<u64>() ^ lot.id ^ character_id,
        ),
        growth_per_hour: definition.growth_per_hour,
        anchor_minute: minute,
    });
    Ok(lot)
}

pub fn create_party_food_lot(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    item_id: &str,
    quantity: u32,
    minute: u64,
) -> Option<FoodLot> {
    let definition = food::definition(item_id)?;
    ensure_food_material_object(ctx, CarriedInventoryScope::Party, inventory_item_id).ok()?;
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: None,
        party_inventory_item_id: Some(inventory_item_id),
        material_revision: 1,
        display_name: definition.name.into(),
        preparation: if definition.class == food::FoodClass::Ration {
            FoodPreparation::Preserved
        } else {
            FoodPreparation::Raw
        },
        ingredient_item_ids: vec![item_id.into()],
        ingredient_quantities: vec![quantity as f32],
        salty_kg: definition.flavors_per_unit.salty * quantity as f32,
        spicy_kg: definition.flavors_per_unit.spicy * quantity as f32,
        sweet_kg: definition.flavors_per_unit.sweet * quantity as f32,
        sour_kg: definition.flavors_per_unit.sour * quantity as f32,
        savory_kg: definition.flavors_per_unit.savory * quantity as f32,
        quality: definition.default_quality.clamp(1, 5),
        mass_kg: definition.mass_kg_per_unit * quantity as f32,
        nutrition_kcal: definition.kcal_per_unit * quantity as f32,
        total_value: definition.value_per_unit * quantity as f32,
        created_at_minute: minute,
    });
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: lot.id,
        concentration_anchor: food::deterministic_initial_contamination(
            ctx.random::<u64>() ^ lot.id,
        ),
        growth_per_hour: definition.growth_per_hour,
        anchor_minute: minute,
    });
    Some(lot)
}

pub fn delete_personal_food_lot(ctx: &ReducerContext, inventory_item_id: u64) {
    for lot in ctx
        .db
        .food_lot()
        .iter()
        .filter(|lot| lot.inventory_item_id == Some(inventory_item_id))
        .collect::<Vec<_>>()
    {
        crate::herbalism::delete_food_medicine(ctx, lot.id);
        ctx.db
            .food_contamination_provenance()
            .food_lot_id()
            .delete(lot.id);
        ctx.db.food_contamination().food_lot_id().delete(lot.id);
        ctx.db.food_lot().id().delete(lot.id);
    }
}

pub fn delete_party_food_lot(ctx: &ReducerContext, inventory_item_id: u64) {
    for lot in ctx
        .db
        .food_lot()
        .iter()
        .filter(|lot| lot.party_inventory_item_id == Some(inventory_item_id))
        .collect::<Vec<_>>()
    {
        crate::herbalism::delete_food_medicine(ctx, lot.id);
        ctx.db
            .food_contamination_provenance()
            .food_lot_id()
            .delete(lot.id);
        ctx.db.food_contamination().food_lot_id().delete(lot.id);
        ctx.db.food_lot().id().delete(lot.id);
    }
}

pub fn remove_party_lot_quantity(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    removed: u32,
    original: u32,
) -> Result<(), String> {
    if removed == original {
        delete_party_food_lot(ctx, inventory_item_id);
        return Ok(());
    }
    let mut lot = ctx
        .db
        .food_lot()
        .iter()
        .find(|lot| lot.party_inventory_item_id == Some(inventory_item_id))
        .ok_or("Food lot metadata not found")?;
    let keep = 1.0 - removed as f32 / original as f32;
    retain_lot_fraction(&mut lot, keep)?;
    ctx.db.food_lot().id().update(lot);
    Ok(())
}

fn split_ingredient_quantities(
    quantities: &[f32],
    taken: u32,
    original: u32,
) -> (Vec<f32>, Vec<f32>) {
    let ratio = taken as f32 / original as f32;
    let child = quantities
        .iter()
        .map(|quantity| food::retained_component(*quantity, ratio))
        .collect::<Vec<_>>();
    let source = quantities
        .iter()
        .zip(&child)
        .map(|(quantity, child_quantity)| (quantity - child_quantity).max(0.0))
        .collect();
    (source, child)
}

fn retain_lot_fraction(lot: &mut FoodLot, retained: f32) -> Result<(), String> {
    lot.material_revision = lot
        .material_revision
        .checked_add(1)
        .ok_or("Food material revision is exhausted")?;
    lot.mass_kg = food::retained_component(lot.mass_kg, retained);
    lot.nutrition_kcal = food::retained_component(lot.nutrition_kcal, retained);
    lot.total_value = food::retained_component(lot.total_value, retained);
    lot.salty_kg = food::retained_component(lot.salty_kg, retained);
    lot.spicy_kg = food::retained_component(lot.spicy_kg, retained);
    lot.sweet_kg = food::retained_component(lot.sweet_kg, retained);
    lot.sour_kg = food::retained_component(lot.sour_kg, retained);
    lot.savory_kg = food::retained_component(lot.savory_kg, retained);
    for quantity in &mut lot.ingredient_quantities {
        *quantity = food::retained_component(*quantity, retained);
    }
    Ok(())
}

pub fn personal_lot(ctx: &ReducerContext, inventory_item_id: u64) -> Option<FoodLot> {
    ctx.db
        .food_lot()
        .iter()
        .find(|lot| lot.inventory_item_id == Some(inventory_item_id))
}

pub fn party_lot(ctx: &ReducerContext, inventory_item_id: u64) -> Option<FoodLot> {
    ctx.db
        .food_lot()
        .iter()
        .find(|lot| lot.party_inventory_item_id == Some(inventory_item_id))
}

fn lot_for_inventory(ctx: &ReducerContext, inventory_item_id: u64) -> Result<FoodLot, String> {
    personal_lot(ctx, inventory_item_id).ok_or("Food lot metadata not found".into())
}

fn contamination(
    ctx: &ReducerContext,
    lot: &FoodLot,
    minute: u64,
) -> Result<(FoodContamination, f32), String> {
    let row = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(lot.id)
        .ok_or("Food contamination state not found")?;
    let current = food::contamination_at(
        row.concentration_anchor,
        row.growth_per_hour,
        minute.saturating_sub(row.anchor_minute),
    );
    Ok((row, current))
}

pub fn split_lot(
    ctx: &ReducerContext,
    source_inventory_id: u64,
    destination_inventory_id: u64,
    taken: u32,
    original: u32,
) -> Result<(), String> {
    if taken == 0 || original == 0 || taken > original {
        return Err("Invalid food lot split".into());
    }
    let mut source = lot_for_inventory(ctx, source_inventory_id)?;
    if taken == original {
        source.inventory_item_id = Some(destination_inventory_id);
        ctx.db.food_lot().id().update(source);
        return Ok(());
    }
    let ratio = taken as f32 / original as f32;
    let mut child = source.clone();
    child.id = 0;
    child.inventory_item_id = Some(destination_inventory_id);
    retain_lot_fraction(&mut child, ratio)?;
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    child.ingredient_quantities = child_ingredients;
    retain_lot_fraction(&mut source, 1.0 - ratio)?;
    source.ingredient_quantities = source_ingredients;
    let contamination = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    ensure_food_material_object(
        ctx,
        CarriedInventoryScope::Personal,
        destination_inventory_id,
    )?;
    let child = ctx.db.food_lot().insert(child);
    split_food_contamination_provenance(ctx, source.id, child.id, ratio)?;
    crate::herbalism::split_food_medicine(ctx, source.id, child.id, ratio)?;
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..contamination
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

pub fn remove_lot_quantity(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    removed: u32,
    original: u32,
) -> Result<(), String> {
    if removed == 0 || original == 0 || removed > original {
        return Err("Invalid food lot quantity change".into());
    }
    if removed == original {
        delete_personal_food_lot(ctx, inventory_item_id);
        return Ok(());
    }
    let mut lot = lot_for_inventory(ctx, inventory_item_id)?;
    let keep = 1.0 - removed as f32 / original as f32;
    retain_lot_fraction(&mut lot, keep)?;
    ctx.db.food_lot().id().update(lot);
    Ok(())
}

pub fn move_or_split_to_party(
    ctx: &ReducerContext,
    source_inventory_id: u64,
    destination_party_id: u64,
    taken: u32,
    original: u32,
) -> Result<(), String> {
    let mut source = lot_for_inventory(ctx, source_inventory_id)?;
    if taken == original {
        source.inventory_item_id = None;
        source.party_inventory_item_id = Some(destination_party_id);
        ctx.db.food_lot().id().update(source);
        return Ok(());
    }
    let ratio = taken as f32 / original as f32;
    let mut child = source.clone();
    child.id = 0;
    child.inventory_item_id = None;
    child.party_inventory_item_id = Some(destination_party_id);
    retain_lot_fraction(&mut child, ratio)?;
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    child.ingredient_quantities = child_ingredients;
    retain_lot_fraction(&mut source, 1.0 - ratio)?;
    source.ingredient_quantities = source_ingredients;
    let hidden = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    ensure_food_material_object(ctx, CarriedInventoryScope::Party, destination_party_id)?;
    let child = ctx.db.food_lot().insert(child);
    split_food_contamination_provenance(ctx, source.id, child.id, ratio)?;
    crate::herbalism::split_food_medicine(ctx, source.id, child.id, ratio)?;
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..hidden
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

pub fn move_or_split_to_personal(
    ctx: &ReducerContext,
    source_party_id: u64,
    destination_inventory_id: u64,
    taken: u32,
    original: u32,
) -> Result<(), String> {
    let mut source = ctx
        .db
        .food_lot()
        .iter()
        .find(|lot| lot.party_inventory_item_id == Some(source_party_id))
        .ok_or("Food lot metadata not found")?;
    if taken == original {
        source.party_inventory_item_id = None;
        source.inventory_item_id = Some(destination_inventory_id);
        ctx.db.food_lot().id().update(source);
        return Ok(());
    }
    let ratio = taken as f32 / original as f32;
    let mut child = source.clone();
    child.id = 0;
    child.party_inventory_item_id = None;
    child.inventory_item_id = Some(destination_inventory_id);
    retain_lot_fraction(&mut child, ratio)?;
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    child.ingredient_quantities = child_ingredients;
    retain_lot_fraction(&mut source, 1.0 - ratio)?;
    source.ingredient_quantities = source_ingredients;
    let hidden = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    ensure_food_material_object(
        ctx,
        CarriedInventoryScope::Personal,
        destination_inventory_id,
    )?;
    let child = ctx.db.food_lot().insert(child);
    split_food_contamination_provenance(ctx, source.id, child.id, ratio)?;
    crate::herbalism::split_food_medicine(ctx, source.id, child.id, ratio)?;
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..hidden
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

fn contamination_provenance_digest(ids: &[String], loads: &[f32]) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(b"food-contamination-provenance-v1");
    for (id, load) in ids.iter().zip(loads) {
        hash.update((id.len() as u64).to_le_bytes());
        hash.update(id.as_bytes());
        hash.update(load.to_bits().to_le_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn split_food_contamination_provenance(
    ctx: &ReducerContext,
    source_food_lot_id: u64,
    destination_food_lot_id: u64,
    child_ratio: f32,
) -> Result<(), String> {
    if let Some(mut provenance) = ctx
        .db
        .food_contamination_provenance()
        .food_lot_id()
        .find(source_food_lot_id)
    {
        let child_loads = provenance
            .contribution_loads
            .iter()
            .map(|load| load * child_ratio)
            .collect::<Vec<_>>();
        provenance.contribution_loads = provenance
            .contribution_loads
            .iter()
            .map(|load| load * (1.0 - child_ratio))
            .collect();
        provenance.contribution_digest = contamination_provenance_digest(
            &provenance.contribution_ids,
            &provenance.contribution_loads,
        );
        ctx.db
            .food_contamination_provenance()
            .food_lot_id()
            .update(provenance.clone());
        ctx.db
            .food_contamination_provenance()
            .insert(FoodContaminationProvenance {
                food_lot_id: destination_food_lot_id,
                contribution_digest: contamination_provenance_digest(
                    &provenance.contribution_ids,
                    &child_loads,
                ),
                contribution_ids: provenance.contribution_ids,
                contribution_loads: child_loads,
            });
    }
    Ok(())
}

fn consume_food_contamination_provenance(
    ctx: &ReducerContext,
    food_lot_id: u64,
    consumed_ratio: f32,
) {
    if let Some(mut provenance) = ctx
        .db
        .food_contamination_provenance()
        .food_lot_id()
        .find(food_lot_id)
    {
        if consumed_ratio >= 0.999_999 {
            ctx.db
                .food_contamination_provenance()
                .food_lot_id()
                .delete(food_lot_id);
        } else {
            for load in &mut provenance.contribution_loads {
                *load *= 1.0 - consumed_ratio;
            }
            provenance.contribution_digest = contamination_provenance_digest(
                &provenance.contribution_ids,
                &provenance.contribution_loads,
            );
            ctx.db
                .food_contamination_provenance()
                .food_lot_id()
                .update(provenance);
        }
    }
}
