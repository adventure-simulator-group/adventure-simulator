//! Authoritative non-fungible food lots and immediate free-form cooking.

use adventuresim_core::{
    disease::{self, DiseaseId},
    food,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    character::{character, character_attributes},
    condition::{character_needs, initialize_character_condition},
    disease::{InfectionEpisodeRow, infection_episode},
    inventory_item,
    strategic::{party, party_inventory_item},
    time::character_time,
};

#[derive(Clone, Copy, Debug, PartialEq, SpacetimeType)]
pub enum FoodPreparation {
    Raw,
    Preserved,
    PanFried,
    Stewed,
    Roasted,
    Baked,
}

#[derive(Clone, Copy, Debug, PartialEq, SpacetimeType)]
pub enum CookingMethod {
    PanFry,
    Stew,
    Roast,
    Bake,
}

impl CookingMethod {
    fn core(self) -> food::CookingMethod {
        match self {
            Self::PanFry => food::CookingMethod::PanFry,
            Self::Stew => food::CookingMethod::Stew,
            Self::Roast => food::CookingMethod::Roast,
            Self::Bake => food::CookingMethod::Bake,
        }
    }
    fn preparation(self) -> FoodPreparation {
        match self {
            Self::PanFry => FoodPreparation::PanFried,
            Self::Stew => FoodPreparation::Stewed,
            Self::Roast => FoodPreparation::Roasted,
            Self::Bake => FoodPreparation::Baked,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::PanFry => "Pan-fried",
            Self::Stew => "Stewed",
            Self::Roast => "Roasted",
            Self::Bake => "Baked",
        }
    }
}

/// Public, inspectable description of one non-fungible inventory batch.
#[derive(Clone, Debug)]
#[table(accessor = food_lot, public)]
pub struct FoodLot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub inventory_item_id: Option<u64>,
    pub party_inventory_item_id: Option<u64>,
    pub display_name: String,
    pub preparation: FoodPreparation,
    pub ingredient_item_ids: Vec<String>,
    /// Fractional source-unit provenance is conserved when a lot is partly eaten.
    pub ingredient_quantities: Vec<f32>,
    pub mass_kg: f32,
    pub nutrition_kcal: f32,
    pub total_value: f32,
    pub created_at_minute: u64,
}

/// Hidden microbial state. The browser can inspect provenance, never pathogen load.
#[derive(Clone, Debug)]
#[table(accessor = food_contamination)]
pub struct FoodContamination {
    #[primary_key]
    pub food_lot_id: u64,
    pub concentration_anchor: f32,
    pub growth_per_hour: f32,
    pub anchor_minute: u64,
}

fn current_minute(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |row| row.minutes)
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
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: Some(inventory_item_id),
        party_inventory_item_id: None,
        display_name: definition.name.into(),
        preparation: if definition.class == food::FoodClass::Ration {
            FoodPreparation::Preserved
        } else {
            FoodPreparation::Raw
        },
        ingredient_item_ids: vec![item_id.into()],
        ingredient_quantities: vec![quantity as f32],
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
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: None,
        party_inventory_item_id: Some(inventory_item_id),
        display_name: definition.name.into(),
        preparation: if definition.class == food::FoodClass::Ration {
            FoodPreparation::Preserved
        } else {
            FoodPreparation::Raw
        },
        ingredient_item_ids: vec![item_id.into()],
        ingredient_quantities: vec![quantity as f32],
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
    lot.mass_kg *= keep;
    lot.nutrition_kcal *= keep;
    lot.total_value *= keep;
    lot.ingredient_quantities = retained_ingredient_quantities(
        &lot.ingredient_quantities,
        original.saturating_sub(removed),
        original,
    );
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

fn retained_ingredient_quantities(quantities: &[f32], retained: u32, original: u32) -> Vec<f32> {
    let ratio = retained as f32 / original as f32;
    quantities
        .iter()
        .map(|quantity| food::retained_component(*quantity, ratio))
        .collect()
}

fn retain_lot_fraction(lot: &mut FoodLot, retained: f32) {
    lot.mass_kg = food::retained_component(lot.mass_kg, retained);
    lot.nutrition_kcal = food::retained_component(lot.nutrition_kcal, retained);
    lot.total_value = food::retained_component(lot.total_value, retained);
    for quantity in &mut lot.ingredient_quantities {
        *quantity = food::retained_component(*quantity, retained);
    }
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
    child.mass_kg = source.mass_kg * ratio;
    child.nutrition_kcal = source.nutrition_kcal * ratio;
    child.total_value = source.total_value * ratio;
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    source.ingredient_quantities = source_ingredients;
    child.ingredient_quantities = child_ingredients;
    source.mass_kg -= child.mass_kg;
    source.nutrition_kcal -= child.nutrition_kcal;
    source.total_value -= child.total_value;
    let contamination = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    let child = ctx.db.food_lot().insert(child);
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
    lot.mass_kg *= keep;
    lot.nutrition_kcal *= keep;
    lot.total_value *= keep;
    lot.ingredient_quantities = retained_ingredient_quantities(
        &lot.ingredient_quantities,
        original.saturating_sub(removed),
        original,
    );
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
    child.mass_kg *= ratio;
    child.nutrition_kcal *= ratio;
    child.total_value *= ratio;
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    source.ingredient_quantities = source_ingredients;
    child.ingredient_quantities = child_ingredients;
    source.mass_kg -= child.mass_kg;
    source.nutrition_kcal -= child.nutrition_kcal;
    source.total_value -= child.total_value;
    let hidden = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    let child = ctx.db.food_lot().insert(child);
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
    child.mass_kg *= ratio;
    child.nutrition_kcal *= ratio;
    child.total_value *= ratio;
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    source.ingredient_quantities = source_ingredients;
    child.ingredient_quantities = child_ingredients;
    source.mass_kg -= child.mass_kg;
    source.nutrition_kcal -= child.nutrition_kcal;
    source.total_value -= child.total_value;
    let hidden = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    let child = ctx.db.food_lot().insert(child);
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..hidden
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

fn item_quantity(ctx: &ReducerContext, character_id: u64, item_id: &str) -> u32 {
    ctx.db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .map(|row| row.quantity)
        .sum()
}

fn equipment_reason(
    ctx: &ReducerContext,
    character_id: u64,
    method: CookingMethod,
) -> Option<&'static str> {
    match method {
        CookingMethod::PanFry if item_quantity(ctx, character_id, "cooking_pan") == 0 => {
            Some("A pan is required")
        }
        CookingMethod::Stew if item_quantity(ctx, character_id, "cooking_pot") == 0 => {
            Some("A pot is required")
        }
        CookingMethod::Bake if item_quantity(ctx, character_id, "portable_oven") == 0 => {
            Some("A portable oven is required")
        }
        _ => None,
    }
}

pub fn preview_cooking(
    ctx: &ReducerContext,
    character_id: u64,
    method: CookingMethod,
    inventory_ids: &[u64],
    quantities: &[u32],
) -> Result<u32, String> {
    if inventory_ids.is_empty()
        || inventory_ids.len() != quantities.len()
        || inventory_ids.len() > 32
    {
        return Err("Select between one and 32 food lots".into());
    }
    if let Some(reason) = equipment_reason(ctx, character_id, method) {
        return Err(reason.into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut safety = Vec::new();
    let mut mass = 0.0;
    for (&id, &quantity) in inventory_ids.iter().zip(quantities) {
        if quantity == 0 || !seen.insert(id) {
            return Err("Food lot selections must be unique and positive".into());
        }
        let inventory = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .ok_or("Ingredient inventory row not found")?;
        if inventory.character_id != character_id || quantity > inventory.quantity {
            return Err("Ingredient is not available in that quantity".into());
        }
        let lot = lot_for_inventory(ctx, id)?;
        safety.push(
            food::definition(&inventory.item_id).map_or(5, |definition| definition.cooking_minutes),
        );
        mass += lot.mass_kg * quantity as f32 / inventory.quantity as f32;
    }
    food::cooking_duration_minutes(method.core(), &safety, mass)
        .ok_or("Cooking duration could not be calculated".into())
}

fn expose_to_dysentery(
    ctx: &ReducerContext,
    character_id: u64,
    lot_id: u64,
    minute: u64,
    dose: f32,
) -> Result<(), String> {
    if dose <= 0.0 {
        return Ok(());
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |row| row.immunity);
    let episodes = crate::disease::character_episodes(ctx, character_id)?;
    if disease::has_unresolved_disease(&episodes, DiseaseId::Dysentery, minute, immunity) {
        return Ok(());
    }
    let prior = disease::acquired_immunity(&episodes, DiseaseId::Dysentery, minute, immunity);
    let seed =
        disease::outbreak_exposure_seed(character_id, &format!("food:{lot_id}:{}", minute / 1));
    if disease::acquisition_succeeds(
        seed,
        disease::definition(DiseaseId::Dysentery),
        immunity,
        prior,
        dose,
    ) {
        ctx.db.infection_episode().insert(InfectionEpisodeRow {
            id: 0,
            character_id,
            disease_id: "dysentery".into(),
            contracted_at: minute,
            treated_at: None,
        });
    }
    Ok(())
}

fn consume_food_amount(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_id: u64,
    kcal: f32,
    explicit: bool,
) -> Result<f32, String> {
    initialize_character_condition(ctx, character_id)?;
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_id)
        .ok_or("Food inventory row not found")?;
    if inventory.character_id != character_id {
        return Err("Food is not in this inventory".into());
    }
    let mut lot = lot_for_inventory(ctx, inventory_id)?;
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    let wanted = if explicit {
        food::explicit_meal_consumption(needs.food_balance_kcal, lot.nutrition_kcal)
    } else {
        food::travel_consumption(needs.food_balance_kcal, lot.nutrition_kcal)
    }
    .min(kcal.max(0.0));
    if wanted <= 0.0 {
        return Ok(0.0);
    }
    let ratio = (wanted / lot.nutrition_kcal).clamp(0.0, 1.0);
    let minute = current_minute(ctx, character_id);
    let (_, current) = contamination(ctx, &lot, minute)?;
    expose_to_dysentery(
        ctx,
        character_id,
        lot.id,
        minute,
        current * ratio * lot.mass_kg,
    )?;
    needs.food_balance_kcal += wanted;
    ctx.db.character_needs().character_id().update(needs);
    if ratio >= 0.999_999 {
        ctx.db.inventory_item().id().delete(inventory.id);
        delete_personal_food_lot(ctx, inventory.id);
    } else {
        retain_lot_fraction(&mut lot, 1.0 - ratio);
        ctx.db.food_lot().id().update(lot);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(wanted)
}

pub fn consume_travel_food_to_zero(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if let Some(party_id) = actor.party_id.as_deref() {
        let mut candidates: Vec<_> = ctx
            .db
            .party_inventory_item()
            .party_id()
            .filter(party_id)
            .filter_map(|inventory| {
                let lot = ctx
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.party_inventory_item_id == Some(inventory.id))?;
                Some((lot.created_at_minute, inventory.id, inventory, lot))
            })
            .collect();
        candidates.sort_by_key(|row| (row.0, row.1));
        for (_, _, inventory, mut lot) in candidates {
            let deficit = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .map_or(0.0, |n| n.food_balance_kcal);
            let wanted = food::travel_consumption(deficit, lot.nutrition_kcal);
            if wanted <= 0.0 {
                break;
            }
            let ratio = (wanted / lot.nutrition_kcal).clamp(0.0, 1.0);
            let minute = current_minute(ctx, character_id);
            let (_, current) = contamination(ctx, &lot, minute)?;
            expose_to_dysentery(
                ctx,
                character_id,
                lot.id,
                minute,
                current * ratio * lot.mass_kg,
            )?;
            let mut needs = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .unwrap();
            needs.food_balance_kcal = (needs.food_balance_kcal + wanted).min(0.0);
            ctx.db.character_needs().character_id().update(needs);
            if ratio >= 0.999_999 {
                ctx.db.party_inventory_item().id().delete(inventory.id);
                ctx.db.food_contamination().food_lot_id().delete(lot.id);
                ctx.db.food_lot().id().delete(lot.id);
            } else {
                retain_lot_fraction(&mut lot, 1.0 - ratio);
                ctx.db.food_lot().id().update(lot);
            }
        }
    }
    let mut personal: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter_map(|inventory| {
            lot_for_inventory(ctx, inventory.id)
                .ok()
                .map(|lot| (lot.created_at_minute, inventory.id))
        })
        .collect();
    personal.sort_unstable();
    for (_, id) in personal {
        if ctx
            .db
            .character_needs()
            .character_id()
            .find(character_id)
            .is_some_and(|n| n.food_balance_kcal >= 0.0)
        {
            break;
        }
        consume_food_amount(ctx, character_id, id, f32::MAX, false)?;
    }
    Ok(())
}

pub fn clear_stomach_fullness(ctx: &ReducerContext, character_id: u64) {
    if let Some(mut needs) = ctx.db.character_needs().character_id().find(character_id) {
        needs.food_balance_kcal = needs.food_balance_kcal.min(0.0);
        ctx.db.character_needs().character_id().update(needs);
    }
}

#[reducer]
pub fn eat_food(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, character_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if actor.in_server {
        return Err("Eating is unavailable during a tactical encounter".into());
    }
    consume_food_amount(ctx, character_id, inventory_item_id, f32::MAX, true)?;
    Ok(())
}

#[reducer]
pub fn cook_food(
    ctx: &ReducerContext,
    character_id: u64,
    method: CookingMethod,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    let duration = preview_cooking(ctx, character_id, method, &inventory_item_ids, &quantities)?;
    initialize_character_condition(ctx, character_id)?;
    let needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    if method == CookingMethod::Stew {
        let required = 500.0 + quantities.iter().map(|q| *q as f32).sum::<f32>() * 100.0;
        let pooled = actor
            .party_id
            .as_deref()
            .and_then(|id| ctx.db.party().id().find(id.to_string()))
            .map_or(0.0, |row| row.pooled_water_ml);
        if pooled + needs.carried_water_ml < required {
            return Err("Stew requires enough pooled or carried water".into());
        }
    }
    // Advance the safe strategic-time prefix before consuming anything. A
    // terminal disease/injury boundary commits through the successful reducer,
    // but leaves ingredients and water untouched and produces no meal.
    if !crate::time::advance_character_wait_time(ctx, character_id, duration as u64)? {
        return Ok(());
    }
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found after cooking time")?;
    let minute = current_minute(ctx, character_id);
    let mut name_parts = Vec::new();
    let mut ingredients = Vec::new();
    let mut ingredient_quantities = Vec::new();
    let mut mass = 0.0;
    let mut kcal = 0.0;
    let mut value = 0.0;
    let mut growth = Vec::new();
    let mut loads = Vec::new();
    for (&id, &quantity) in inventory_item_ids.iter().zip(&quantities) {
        let inventory = ctx.db.inventory_item().id().find(id).unwrap();
        let lot = lot_for_inventory(ctx, id)?;
        let (cont, current) = contamination(ctx, &lot, minute)?;
        let ratio = quantity as f32 / inventory.quantity as f32;
        name_parts.push(lot.display_name.clone());
        ingredients.extend(lot.ingredient_item_ids.clone());
        ingredient_quantities.extend(
            lot.ingredient_quantities
                .iter()
                .map(|q| food::retained_component(*q, ratio)),
        );
        mass += lot.mass_kg * ratio;
        kcal += lot.nutrition_kcal * ratio;
        value += lot.total_value * ratio;
        growth.push(cont.growth_per_hour);
        loads.push(current * lot.mass_kg * ratio);
    }
    // Ingredient and water mutation begins only after the wait completed.
    if method == CookingMethod::Stew {
        let required = 500.0 + quantities.iter().map(|q| *q as f32).sum::<f32>() * 100.0;
        if let Some(party_id) = actor.party_id.as_deref()
            && let Some(mut party) = ctx.db.party().id().find(party_id.to_string())
        {
            let used = required.min(party.pooled_water_ml);
            party.pooled_water_ml -= used;
            ctx.db.party().id().update(party);
            needs.carried_water_ml -= required - used;
        } else {
            needs.carried_water_ml -= required;
        }
        ctx.db.character_needs().character_id().update(needs);
    }
    for (&id, &quantity) in inventory_item_ids.iter().zip(&quantities) {
        let mut inv = ctx.db.inventory_item().id().find(id).unwrap();
        let lot = lot_for_inventory(ctx, id)?;
        if quantity == inv.quantity {
            ctx.db.inventory_item().id().delete(id);
            delete_personal_food_lot(ctx, id);
        } else {
            let ratio = quantity as f32 / inv.quantity as f32;
            inv.quantity -= quantity;
            ctx.db.inventory_item().id().update(inv);
            let mut remaining = lot;
            retain_lot_fraction(&mut remaining, 1.0 - ratio);
            ctx.db.food_lot().id().update(remaining);
        }
    }
    let output = ctx.db.inventory_item().insert(crate::InventoryItem {
        id: 0,
        character_id,
        item_id: "cooked_meal".into(),
        quantity: 1,
    });
    name_parts.sort();
    name_parts.dedup();
    let display = format!("{} {}", method.name(), name_parts.join(", "));
    let out_minute = current_minute(ctx, character_id);
    let out_lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: Some(output.id),
        party_inventory_item_id: None,
        display_name: display,
        preparation: method.preparation(),
        ingredient_item_ids: ingredients,
        ingredient_quantities,
        mass_kg: mass,
        nutrition_kcal: kcal * 0.97,
        total_value: value,
        created_at_minute: out_minute,
    });
    let weighted = if mass > 0.0 {
        loads.iter().sum::<f32>() / mass
    } else {
        0.0
    };
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: out_lot.id,
        concentration_anchor: food::cooked_contamination(weighted, method.core()),
        growth_per_hour: food::cooked_growth_per_hour(&growth, method.core()),
        anchor_minute: out_minute,
    });
    consume_food_amount(ctx, character_id, output.id, f32::MAX, true)?;
    // A full character consumes zero calories, so the helper may return before
    // its mutation refresh. The retained output mass must still be persisted.
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn method_mapping_is_stable() {
        assert_eq!(CookingMethod::Roast.core(), food::CookingMethod::Roast);
    }
}
