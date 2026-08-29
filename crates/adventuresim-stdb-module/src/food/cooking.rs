// Owns cooking instruments, fireplace dish assembly, retrieval, and previews.
fn cooking_method_preparation(method: CookingMethod) -> FoodPreparation {
    match method {
        CookingMethod::PanFry => FoodPreparation::PanFried,
        CookingMethod::Stew => FoodPreparation::Stewed,
        CookingMethod::Roast => FoodPreparation::Roasted,
        CookingMethod::Bake => FoodPreparation::Baked,
    }
}

fn cooking_method_name(method: CookingMethod) -> &'static str {
    match method {
        CookingMethod::PanFry => "Pan-fried",
        CookingMethod::Stew => "Stewed",
        CookingMethod::Roast => "Roasted",
        CookingMethod::Bake => "Baked",
    }
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

fn cooking_check(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    Ok(Skill::Cooking.capped_rank_for_aptitude(
        skills.effective_skill_hours(Skill::Cooking),
        Skill::Cooking.governing_aptitude(&attributes),
    ) * limbs.head_health.clamp(0.0, 1.0))
}

fn parse_consumable_fractions(
    fractions_micros: Vec<u32>,
) -> Result<Vec<ConsumableFractionMicros>, String> {
    fractions_micros
        .into_iter()
        .map(|value| {
            ConsumableFractionMicros::try_new(value)
                .map_err(|_| "Ingredient fraction cannot exceed one whole".to_owned())
        })
        .collect()
}

#[reducer]
pub fn add_fireplace_ingredients(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: String,
    inventory_scope: String,
    inventory_item_ids: Vec<u64>,
    fractions_micros: Vec<u32>,
) -> Result<(), String> {
    add_fireplace_ingredients_at(
        ctx,
        character_id,
        fireplace_fixture_id,
        inventory_scope,
        inventory_item_ids,
        parse_consumable_fractions(fractions_micros)?,
        None,
    )
}

/// Starts the independent dish lane belonging to one placed vessel. Every
/// contained cookable food lot at any nesting depth is consumed in full;
/// non-food solids and nested containers remain in place. Container water is used by the cooking evaluator and is
/// mandatory for pots.
#[reducer]
pub fn start_fireplace_container_cooking(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: String,
    container_object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    validate_fireplace_fixture(ctx, &actor, &fireplace_fixture_id)?;
    let key = vessel_station_key(character_id, &fireplace_fixture_id, container_object_id);
    let station = ctx
        .db
        .fireplace_station()
        .key()
        .find(key.clone())
        .ok_or("Container is not over this fireplace")?;
    let fixture = validate_persisted_station_fixture(ctx, &station)?;
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object is missing")?;
    crate::object_custody::require_object_at_fixture(ctx, &object, &fixture)?;
    if ctx.db.fireplace_dish().station_key().find(key).is_some() {
        return Err("This container is already cooking".into());
    }
    let return_custody = station
        .instrument_return_custody
        .as_ref()
        .ok_or("Container return custody is unknown")?;
    let destination = crate::object_custody::carried_destination(return_custody, character_id)?;
    let scope = match destination {
        OperationalCustody::Character(_) => CarriedInventoryScope::Personal,
        OperationalCustody::Party(_) => CarriedInventoryScope::Party,
        _ => return Err("Container return custody is not carried inventory".into()),
    };
    let mut ids = Vec::new();
    let mut amounts = Vec::new();
    let mut consumed_objects = Vec::new();
    // Cooking intentionally sees only direct contents. A hidden nested lot is
    // not a selected ingredient and remains inside its child container.
    for edge in ctx
        .db
        .inventory_containment()
        .parent_object_id()
        .filter(container_object_id)
    {
        let object_id = edge.child_object_id;
        let child = ctx
            .db
            .inventory_object()
            .id()
            .find(object_id)
            .ok_or("Contained object is missing")?;
        if !food::is_cookable_ingredient(&child.item_id) {
            continue;
        }
        let (row_id, lot, amount) = match (&child.location, scope) {
            (InventoryLocation::Personal(location), CarriedInventoryScope::Personal) => (
                location.row_id,
                personal_lot(ctx, location.row_id),
                crate::inventory_amount::personal_fraction(ctx, location.row_id),
            ),
            (InventoryLocation::Party(location), CarriedInventoryScope::Party) => (
                location.row_id,
                party_lot(ctx, location.row_id),
                crate::inventory_amount::party_fraction(ctx, location.row_id),
            ),
            _ => return Err("Contained food custody conflicts with its return inventory".into()),
        };
        let (Some(lot), Some(amount)) = (lot, amount) else {
            continue;
        };
        if !matches!(
            lot.preparation,
            FoodPreparation::Raw
                | FoodPreparation::Cut
                | FoodPreparation::Ground
                | FoodPreparation::Preserved
        ) {
            return Err("A cooked meal cannot be cooked again".into());
        }
        ids.push(row_id);
        amounts.push(amount);
        consumed_objects.push(child.id);
    }
    if ids.is_empty() {
        return Err("Put at least one uncooked food lot in the container".into());
    }
    add_fireplace_ingredients_at(
        ctx,
        character_id,
        fireplace_fixture_id,
        scope.as_str().into(),
        ids,
        amounts,
        Some(station),
    )?;
    for object_id in consumed_objects {
        ctx.db
            .inventory_containment()
            .child_object_id()
            .delete(object_id);
        ctx.db.inventory_object().id().delete(object_id);
    }
    Ok(())
}

fn add_fireplace_ingredients_at(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: String,
    inventory_scope: String,
    inventory_item_ids: Vec<u64>,
    fractions: Vec<ConsumableFractionMicros>,
    vessel_station: Option<FireplaceStation>,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_fixture(ctx, &actor, &fireplace_fixture_id)?;
    let inventory_scope = CarriedInventoryScope::try_from(inventory_scope.as_str())
        .map_err(|error| error.to_string())?;
    let is_vessel = vessel_station.is_some();
    let station = vessel_station
        .unwrap_or_else(|| fireplace_station_for(ctx, character_id, &fireplace_fixture_id));
    let station_fixture = validate_persisted_station_fixture(ctx, &station)?;
    if station_fixture.to_string() != fireplace_fixture_id {
        return Err("Fireplace station conflicts with the requested canonical fixture".into());
    }
    let return_custody = if is_vessel {
        let custody = station
            .instrument_return_custody
            .clone()
            .ok_or("Container return custody is unknown")?;
        let destination = crate::object_custody::carried_destination(&custody, character_id)?;
        let destination_scope = match &destination {
            OperationalCustody::Character(_) => CarriedInventoryScope::Personal,
            OperationalCustody::Party(_) => CarriedInventoryScope::Party,
            _ => return Err("Container return custody is not carried inventory".into()),
        };
        if destination_scope != inventory_scope {
            return Err("Container return custody conflicts with the cooking scope".into());
        }
        if let OperationalCustody::Party(party_id) = destination
            && ctx
                .db
                .party_authority()
                .id()
                .find(party_id.as_str().to_owned())
                .is_none()
        {
            return Err("Original party inventory is unavailable".into());
        }
        custody
    } else {
        let custody = crate::object_custody::carried_scope_custody(ctx, &actor, inventory_scope)?;
        crate::object_custody::encode_custody(&custody)
    };
    if ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(station.key.clone())
        .is_some()
    {
        return Err("This fireplace already holds a dish".into());
    }
    if inventory_item_ids.is_empty()
        || inventory_item_ids.len() != fractions.len()
        || !is_vessel && inventory_item_ids.len() > 32
    {
        return Err("Select between one and 32 food lots".into());
    }
    let method = if is_vessel {
        method_for_instrument(station.instrument_item_id.as_deref())?
    } else {
        CookingMethod::Roast
    };
    let check = cooking_check(ctx, character_id)?;
    let herbalism_check = preparation_skill_check(ctx, character_id, Skill::Herbalism)?;
    initialize_character_condition(ctx, character_id)?;
    let minute = current_minute(ctx, character_id);
    let mut seen = std::collections::BTreeSet::new();
    let mut selected = Vec::new();
    let mut safety = Vec::new();
    let mut name_parts = Vec::new();
    let mut ingredient_ids = Vec::new();
    let mut ingredient_quantities = Vec::new();
    let mut flavors = food::FlavorProfile::default();
    let mut mass = 0.0;
    let mut kcal = 0.0;
    let mut value = 0.0;
    let mut culinary_fat_mass = 0.0;
    let mut growth = Vec::new();
    let mut growth_mass = 0.0;
    let mut loads = Vec::new();
    let mut contamination_contribution_ids = Vec::new();
    let mut contamination_contribution_loads = Vec::new();
    let mut medicinal = std::collections::BTreeMap::<String, f32>::new();
    for (&id, &fraction) in inventory_item_ids.iter().zip(&fractions) {
        if fraction.is_zero() || !seen.insert(id) {
            return Err("Food lot selections must be unique and positive".into());
        }
        let (item_id, available, lot) = match inventory_scope {
            CarriedInventoryScope::Personal => {
                let row = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(id)
                    .ok_or("Ingredient inventory row not found")?;
                if row.character_id != character_id
                    || crate::character::wearable_is_equipped(ctx, id)
                {
                    return Err("Ingredient is equipped or not in this inventory".into());
                }
                (
                    row.item_id,
                    crate::inventory_amount::personal_fraction(ctx, id)
                        .ok_or("Ingredient amount state is missing")?,
                    personal_lot(ctx, id).ok_or("Food lot metadata not found")?,
                )
            }
            CarriedInventoryScope::Party => {
                let party_id = actor
                    .party_id
                    .as_deref()
                    .ok_or("Character has no party inventory")?;
                let row = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(id)
                    .ok_or("Ingredient inventory row not found")?;
                if row.party_id != party_id {
                    return Err("Ingredient is not in this party inventory".into());
                }
                (
                    row.item_id,
                    crate::inventory_amount::party_fraction(ctx, id)
                        .ok_or("Ingredient amount state is missing")?,
                    party_lot(ctx, id).ok_or("Food lot metadata not found")?,
                )
            }
        };
        if fraction > available {
            return Err("Ingredient is not available in that amount".into());
        }
        if !food::is_cookable_ingredient(&item_id) {
            return Err("A cooked meal cannot be cooked again".into());
        }
        let ratio = fraction.get() as f32 / available.get() as f32;
        if ![
            lot.mass_kg,
            lot.nutrition_kcal,
            lot.total_value,
            lot.salty_kg,
            lot.spicy_kg,
            lot.sweet_kg,
            lot.sour_kg,
            lot.savory_kg,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0)
        {
            return Err("Ingredient lot contains invalid food values".into());
        }
        let (cont, current) = contamination(ctx, &lot, minute)?;
        let raw_safety = food::definition(&item_id).map_or(5, |d| d.cooking_minutes);
        let preparation_factor = match lot.preparation {
            FoodPreparation::Cut => food::CUT_COOKING_TIME_FACTOR,
            FoodPreparation::Ground => food::GROUND_COOKING_TIME_FACTOR,
            _ => 1.0,
        };
        safety.push(
            food::preparation_safety_minutes(raw_safety, preparation_factor)
                .ok_or("Ingredient preparation has an invalid cooking-time factor")?,
        );
        name_parts.push(lot.display_name.clone());
        ingredient_ids.extend(lot.ingredient_item_ids.clone());
        ingredient_quantities.extend(
            lot.ingredient_quantities
                .iter()
                .map(|q| food::retained_component(*q, ratio)),
        );
        if herbalism_check >= 1.0 {
            for (component_id, quantity) in lot
                .ingredient_item_ids
                .iter()
                .zip(&lot.ingredient_quantities)
            {
                let profile = match component_id.as_str() {
                    "willow_bark" => Some("cooling_willow_draught"),
                    "sage" => Some("sage_infusion"),
                    // Comfrey is heat-sensitive and loses its useful topical
                    // component. Poppy requires passive alcoholic extraction.
                    "comfrey" | "poppy" => None,
                    _ => None,
                };
                if let Some(profile) = profile {
                    *medicinal.entry(profile.into()).or_default() +=
                        food::retained_component(*quantity, ratio)
                            * (0.5 + 0.1 * herbalism_check.clamp(1.0, 5.0));
                }
            }
        }
        let selected_mass = lot.mass_kg * ratio;
        mass += selected_mass;
        kcal += lot.nutrition_kcal * ratio;
        value += lot.total_value * ratio;
        flavors.add_assign(
            food::FlavorProfile::new(
                lot.salty_kg,
                lot.spicy_kg,
                lot.sweet_kg,
                lot.sour_kg,
                lot.savory_kg,
            )
            .scaled(ratio),
        );
        if lot
            .ingredient_item_ids
            .iter()
            .any(|i| food::definition(i).is_some_and(|d| d.culinary_fat))
        {
            culinary_fat_mass += selected_mass;
        }
        growth.push(cont.growth_per_hour);
        growth_mass += cont.growth_per_hour.max(0.0) * selected_mass;
        let selected_load = current * selected_mass;
        loads.push(selected_load);
        contamination_contribution_ids.push(format!("food-lot:{}", lot.id));
        contamination_contribution_loads.push(selected_load);
        selected.push((id, fraction, available, lot));
    }
    let ingredient_mass = mass;
    let contained_water_ml = station
        .instrument_object_id
        .and_then(|object_id| {
            ctx.db
                .container_liquid()
                .container_object_id()
                .find(object_id)
        })
        .filter(|liquid| liquid.liquid_item_id == crate::inventory_container::WATER_ITEM_ID)
        .map_or(0, |liquid| liquid.water_ml);
    let contained_water_materials = match station.instrument_object_id {
        Some(object_id) if contained_water_ml > 0 => {
            crate::outbreak::contained_water_contamination(ctx, object_id, minute)?
        }
        _ => Vec::new(),
    };
    if method == CookingMethod::Stew && contained_water_ml == 0 {
        return Err("Stew requires water inside the cooking pot".into());
    }
    let water_ml = contained_water_ml as f32;
    mass += water_ml / 1_000.0;
    let contributed_water_microliters = contained_water_materials
        .iter()
        .try_fold(Microliters::ZERO, |total, row| total.checked_add(row.3))
        .ok_or("Contained water material volume overflow")?;
    let public_water_microliters = Microliters::try_from_nonnegative_milliliters_rounded(water_ml)
        .ok_or("Cooking water volume is invalid")?;
    if contributed_water_microliters > public_water_microliters {
        return Err("Contained water material exceeds its public volume".into());
    }
    for (material_lot_id, current, water_growth, amount_microliters) in &contained_water_materials {
        let water_mass_kg = amount_microliters.as_water_kilograms_f32();
        let water_load = current * water_mass_kg;
        loads.push(water_load);
        growth.push(*water_growth);
        growth_mass += water_growth.max(0.0) * water_mass_kg;
        contamination_contribution_ids.push(format!("water-output-lot:{material_lot_id}"));
        contamination_contribution_loads.push(water_load);
    }
    let target = food::cooking_duration_minutes_for_check(method, &safety, mass, check)
        .ok_or("Cooking duration could not be calculated")?;
    let flavor_quality = food::aggregate_flavor_quality(method, flavors, mass);
    let quality = food::cooked_quality(
        food::chef_quality_tier(check),
        flavor_quality,
        method == CookingMethod::PanFry
            && !food::pan_fry_has_enough_fat(culinary_fat_mass, ingredient_mass),
    );
    // Everything above is preflight. Mutation starts here and remains atomic.
    if let Some(instrument_object_id) = station.instrument_object_id
        && contained_water_ml > 0
    {
        ctx.db
            .container_liquid()
            .container_object_id()
            .delete(instrument_object_id);
        crate::outbreak::delete_container_water_contributions(ctx, instrument_object_id);
    }
    for (id, fraction, available, mut lot) in selected {
        if fraction == available {
            match inventory_scope {
                CarriedInventoryScope::Personal => {
                    ctx.db
                        .inventory_item_amount()
                        .inventory_item_id()
                        .delete(id);
                    ctx.db.inventory_item().id().delete(id);
                    delete_personal_food_lot(ctx, id);
                }
                CarriedInventoryScope::Party => {
                    ctx.db
                        .party_item_amount()
                        .party_inventory_item_id()
                        .delete(id);
                    ctx.db.party_inventory_item().id().delete(id);
                    delete_party_food_lot(ctx, id);
                }
            }
        } else {
            retain_lot_fraction(
                &mut lot,
                1.0 - fraction.get() as f32 / available.get() as f32,
            )?;
            let remaining = available
                .checked_sub(fraction)
                .expect("selected ingredient fraction cannot exceed availability");
            ctx.db.food_lot().id().update(lot);
            match inventory_scope {
                CarriedInventoryScope::Personal => {
                    ctx.db.inventory_item_amount().inventory_item_id().update(
                        crate::InventoryItemAmount {
                            inventory_item_id: id,
                            remaining_fraction_micros: remaining.get(),
                        },
                    );
                }
                CarriedInventoryScope::Party => {
                    ctx.db.party_item_amount().party_inventory_item_id().update(
                        crate::PartyItemAmount {
                            party_inventory_item_id: id,
                            remaining_fraction_micros: remaining.get(),
                        },
                    );
                }
            };
        }
    }
    name_parts.sort();
    name_parts.dedup();
    let raw_contamination = food::microbial_concentration(loads.iter().sum(), mass);
    let raw_growth_per_hour = if mass > 0.0 { growth_mass / mass } else { 0.0 };
    let ready_nutrition_retention =
        food::cooked_nutrition_retention(check) * food::method_nutrition_retention(method);
    ctx.db.fireplace_dish().insert(FireplaceDish {
        station_key: station.key.clone(),
        character_id,
        fireplace_fixture_id: fireplace_fixture_id.clone(),
        return_custody,
        contributor_name: actor.name,
        method,
        cooking_check: check,
        started_at_minute: minute,
        target_minutes: target,
        display_name: format!("{} {}", cooking_method_name(method), name_parts.join(", ")),
        ingredient_item_ids: ingredient_ids,
        ingredient_quantities,
        salty_kg: flavors.salty,
        spicy_kg: flavors.spicy,
        sweet_kg: flavors.sweet,
        sour_kg: flavors.sour,
        savory_kg: flavors.savory,
        ready_quality: quality,
        mass_kg: mass,
        raw_nutrition_kcal: kcal,
        ready_nutrition_retention,
        ingredient_value: value,
        raw_contamination,
        raw_growth_per_hour,
        cooked_growth_per_hour: food::cooked_growth_per_hour(&growth, method),
        contamination_contribution_digest: contamination_provenance_digest(
            &contamination_contribution_ids,
            &contamination_contribution_loads,
        ),
        contamination_contribution_ids,
        contamination_contribution_loads,
        medicinal_profile_ids: medicinal.keys().cloned().collect(),
        medicinal_profile_versions: vec![1; medicinal.len()],
        medicinal_potency_units: medicinal.values().copied().collect(),
    });
    if ctx
        .db
        .fireplace_station()
        .key()
        .find(station.key.clone())
        .is_none()
    {
        ctx.db.fireplace_station().insert(station);
    }
    Ok(())
}

#[reducer]
pub fn retrieve_fireplace_dish(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: String,
    container_object_id: Option<u64>,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_fixture(ctx, &actor, &fireplace_fixture_id)?;
    let key = container_object_id.map_or_else(
        || station_key(character_id, &fireplace_fixture_id),
        |object_id| vessel_station_key(character_id, &fireplace_fixture_id, object_id),
    );
    let vessel_station = ctx.db.fireplace_station().key().find(key.clone());
    let dish = ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(key)
        .ok_or("No dish is in this fireplace")?;
    if validate_persisted_dish_fixture(ctx, &dish)?.to_string() != fireplace_fixture_id {
        return Err("Dish custody conflicts with the requested canonical fixture".into());
    }
    let destination = dish_inventory_destination(&dish.return_custody, character_id)?;
    if let OperationalCustody::Party(party_id) = &destination
        && ctx
            .db
            .party_authority()
            .id()
            .find(party_id.as_str().to_owned())
            .is_none()
    {
        return Err("Dish's original party inventory is unavailable".into());
    }
    if let Some(object_id) = container_object_id
        && vessel_station
            .as_ref()
            .and_then(|station| station.instrument_object_id)
            != Some(object_id)
    {
        return Err("Dish selector conflicts with its fireplace container".into());
    }
    let minute = current_minute(ctx, character_id);
    let elapsed = minute.saturating_sub(dish.started_at_minute);
    let doneness = food::method_doneness_outcome(dish.method, elapsed, dish.target_minutes);
    let quality = dish
        .ready_quality
        .saturating_sub(doneness.quality_penalty)
        .max(1);
    let kcal = dish.raw_nutrition_kcal
        * food::doneness_nutrition_factor(dish.ready_nutrition_retention, doneness);
    let value =
        dish.ingredient_value * food::quality_value_multiplier(quality) * doneness.calorie_factor;
    let cooked_contamination = food::partially_cooked_contamination(
        dish.raw_contamination,
        dish.method,
        doneness.contamination_kill_progress,
    );
    let cooked_contribution_loads = food::scale_contamination_contributions(
        dish.raw_contamination,
        cooked_contamination,
        &dish.contamination_contribution_loads,
    );
    let cooked_contribution_digest = contamination_provenance_digest(
        &dish.contamination_contribution_ids,
        &cooked_contribution_loads,
    );
    let surviving_load = cooked_contribution_loads.iter().sum::<f32>();
    let expected_surviving_load = cooked_contamination * dish.mass_kg;
    if (surviving_load - expected_surviving_load).abs()
        > expected_surviving_load.abs().max(1.0) * 1e-5
    {
        return Err("Cooked contamination contribution loads do not conserve".into());
    }

    let (personal_id, party_id) = match &destination {
        OperationalCustody::Character(character) => {
            let row = ctx.db.inventory_item().insert(crate::InventoryItem {
                id: 0,
                character_id: character.get(),
                item_id: "cooked_meal".into(),
                quantity: 1,
            });
            crate::inventory_amount::initialize_personal(ctx, row.id);
            (Some(row.id), None)
        }
        OperationalCustody::Party(party) => {
            let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party.as_str().into(),
                item_id: "cooked_meal".into(),
                quantity: 1,
            });
            crate::inventory_amount::initialize_party(ctx, row.id);
            (None, Some(row.id))
        }
        _ => return Err("Invalid retrieval inventory".into()),
    };
    if let Some(parent_object_id) = container_object_id {
        let row_id = personal_id.or(party_id).expect("cooked meal inventory row");
        let meal = ctx.db.inventory_object().insert(crate::InventoryObject {
            id: 0,
            item_id: "cooked_meal".into(),
            location: crate::inventory_container::carried_location_for_row(&destination, row_id)?,
        });
        ctx.db
            .inventory_containment()
            .insert(crate::InventoryContainment {
                child_object_id: meal.id,
                parent_object_id,
            });
    }
    if let Some(row_id) = personal_id {
        ensure_food_material_object(ctx, CarriedInventoryScope::Personal, row_id)?;
    }
    if let Some(row_id) = party_id {
        ensure_food_material_object(ctx, CarriedInventoryScope::Party, row_id)?;
    }
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: personal_id,
        party_inventory_item_id: party_id,
        material_revision: 1,
        display_name: dish.display_name,
        preparation: if dish.method == CookingMethod::Roast
            && elapsed > u64::from(dish.target_minutes)
        {
            FoodPreparation::DriedSmoked
        } else {
            cooking_method_preparation(dish.method)
        },
        ingredient_item_ids: dish.ingredient_item_ids,
        ingredient_quantities: dish.ingredient_quantities,
        salty_kg: dish.salty_kg,
        spicy_kg: dish.spicy_kg,
        sweet_kg: dish.sweet_kg,
        sour_kg: dish.sour_kg,
        savory_kg: dish.savory_kg,
        quality,
        mass_kg: dish.mass_kg,
        nutrition_kcal: kcal,
        total_value: value,
        created_at_minute: minute,
    });
    if !dish.contamination_contribution_ids.is_empty() {
        ctx.db
            .food_contamination_provenance()
            .insert(FoodContaminationProvenance {
                food_lot_id: lot.id,
                contribution_ids: dish.contamination_contribution_ids,
                contribution_loads: cooked_contribution_loads,
                contribution_digest: cooked_contribution_digest,
            });
    }
    let medicinal_heat_factor = if doneness.progress < 1.0 {
        doneness.progress
    } else if matches!(dish.method, CookingMethod::PanFry | CookingMethod::Bake) {
        doneness.calorie_factor
    } else {
        1.0
    };
    for ((profile_id, profile_version), potency_units) in dish
        .medicinal_profile_ids
        .iter()
        .zip(&dish.medicinal_profile_versions)
        .zip(&dish.medicinal_potency_units)
    {
        let potency = potency_units * medicinal_heat_factor;
        if potency > 0.0 {
            ctx.db
                .medicinal_component()
                .insert(crate::herbalism::MedicinalComponent {
                    key: format!("food_lot|{}|{profile_id}|{profile_version}", lot.id),
                    carrier_kind: "food_lot".into(),
                    carrier_id: lot.id,
                    intervention_profile_id: profile_id.clone(),
                    profile_version: *profile_version,
                    potency_units: potency,
                });
        }
    }
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: lot.id,
        concentration_anchor: cooked_contamination,
        growth_per_hour: food::partially_cooked_growth(
            dish.raw_growth_per_hour,
            dish.cooked_growth_per_hour,
            doneness.contamination_kill_progress,
        ),
        anchor_minute: minute,
    });
    ctx.db
        .fireplace_dish()
        .station_key()
        .delete(dish.station_key.clone());
    if let Some(station) = ctx.db.fireplace_station().key().find(dish.station_key)
        && station.instrument_item_id.is_none()
    {
        ctx.db.fireplace_station().key().delete(station.key);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

pub fn preview_cooking(
    ctx: &ReducerContext,
    character_id: u64,
    method: CookingMethod,
    inventory_ids: &[u64],
    fractions: &[ConsumableFractionMicros],
) -> Result<u32, String> {
    if inventory_ids.is_empty()
        || inventory_ids.len() != fractions.len()
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
    for (&id, &fraction) in inventory_ids.iter().zip(fractions) {
        if fraction.is_zero() || !seen.insert(id) {
            return Err("Food lot selections must be unique and positive".into());
        }
        let inventory = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .ok_or("Ingredient inventory row not found")?;
        let available = crate::inventory_amount::personal_fraction(ctx, id).unwrap_or_default();
        if inventory.character_id != character_id || fraction > available {
            return Err("Ingredient is not available in that amount".into());
        }
        if !food::is_cookable_ingredient(&inventory.item_id) {
            return Err("A cooked meal cannot be cooked again".into());
        }
        let lot = lot_for_inventory(ctx, id)?;
        safety.push(
            food::definition(&inventory.item_id).map_or(5, |definition| definition.cooking_minutes),
        );
        mass += lot.mass_kg * fraction.get() as f32 / available.get() as f32;
    }
    food::cooking_duration_minutes_for_check(
        method,
        &safety,
        mass,
        cooking_check(ctx, character_id)?,
    )
    .ok_or("Cooking duration could not be calculated".into())
}
