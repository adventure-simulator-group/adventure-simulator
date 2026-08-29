// Owns foodborne exposure, measured consumption, travel use, and eating reducers.
fn expose_to_dysentery(
    ctx: &ReducerContext,
    character_id: u64,
    lot_id: u64,
    minute: u64,
    dose: f32,
    consumed_fraction_bps: u16,
) -> Result<(), String> {
    let contribution_digest = ctx
        .db
        .food_contamination_provenance()
        .food_lot_id()
        .find(lot_id)
        .map_or_else(
            || format!("food-lot:{lot_id}"),
            |row| row.contribution_digest,
        );
    expose_food_water_dysentery(
        ctx,
        character_id,
        &format!("food:{lot_id}:{minute}"),
        lot_id,
        minute,
        dose,
        &contribution_digest,
        consumed_fraction_bps,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the exposure boundary records each source and dose coordinate explicitly"
)]
pub(crate) fn expose_food_water_dysentery(
    ctx: &ReducerContext,
    character_id: u64,
    exposure_id: &str,
    carrier_id: u64,
    minute: u64,
    dose: f32,
    contribution_digest: &str,
    consumed_fraction_bps: u16,
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
    let seed = disease::outbreak_exposure_seed(character_id, exposure_id);
    let protected_dose = crate::disease::protected_point_exposure(
        ctx,
        character_id,
        minute,
        adventuresim_core::disease::TransmissionVector::FoodWater,
        dose,
    )?;
    if disease::acquisition_succeeds(
        seed,
        disease::definition(DiseaseId::Dysentery),
        immunity,
        prior,
        protected_dose,
    ) {
        let episode_id = seed.max(1);
        let place = crate::foraging::current_strategic_place(ctx, character_id)?;
        crate::world_event::commit_food_water_infection(
            ctx,
            exposure_id,
            character_id,
            &place.to_string(),
            carrier_id,
            contribution_digest,
            dose,
            protected_dose,
            immunity,
            prior,
            consumed_fraction_bps,
            "dysentery",
            episode_id,
            minute,
        )?;
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
    crate::inventory_container::reconcile_consumed_row(
        ctx,
        CarriedInventoryScope::Personal,
        inventory_id,
        false,
    )?;
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
        (ratio * f32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)).round() as u16,
    )?;
    crate::herbalism::consume_food_medicine(ctx, character_id, lot.id, ratio)?;
    consume_food_contamination_provenance(ctx, lot.id, ratio);
    needs.food_balance_kcal += wanted;
    ctx.db.character_needs().character_id().update(needs);
    if ratio >= 0.999_999 {
        crate::inventory_container::reconcile_consumed_row(
            ctx,
            CarriedInventoryScope::Personal,
            inventory.id,
            true,
        )?;
        ctx.db
            .inventory_item_amount()
            .inventory_item_id()
            .delete(inventory.id);
        ctx.db.inventory_item().id().delete(inventory.id);
        delete_personal_food_lot(ctx, inventory.id);
    } else {
        let retained = 1.0 - ratio;
        let state = ctx
            .db
            .inventory_item_amount()
            .inventory_item_id()
            .find(inventory.id)
            .ok_or("Food amount state is missing")?;
        retain_lot_fraction(&mut lot, retained)?;
        ctx.db.food_lot().id().update(lot);
        let current = ConsumableFractionMicros::try_new(state.remaining_fraction_micros)
            .expect("persisted consumable fraction must not exceed one whole");
        let mut remaining = current
            .try_scaled_floor(retained)
            .map_err(|_| "Retained food fraction is invalid")?;
        if remaining.is_zero() {
            remaining = ConsumableFractionMicros::MINIMUM_NONZERO;
        }
        ctx.db
            .inventory_item_amount()
            .inventory_item_id()
            .update(crate::InventoryItemAmount {
                inventory_item_id: inventory.id,
                remaining_fraction_micros: remaining.get(),
            });
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
            .filter(|inventory| {
                !crate::inventory_container::row_is_fireplace_rooted(
                    ctx,
                    CarriedInventoryScope::Party,
                    inventory.id,
                )
            })
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
            crate::inventory_container::reconcile_consumed_row(
                ctx,
                CarriedInventoryScope::Party,
                inventory.id,
                false,
            )?;
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
                (ratio * f32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)).round()
                    as u16,
            )?;
            crate::herbalism::consume_food_medicine(ctx, character_id, lot.id, ratio)?;
            consume_food_contamination_provenance(ctx, lot.id, ratio);
            let mut needs = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .unwrap();
            needs.food_balance_kcal = (needs.food_balance_kcal + wanted).min(0.0);
            ctx.db.character_needs().character_id().update(needs);
            if ratio >= 0.999_999 {
                crate::inventory_container::reconcile_consumed_row(
                    ctx,
                    CarriedInventoryScope::Party,
                    inventory.id,
                    true,
                )?;
                ctx.db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .delete(inventory.id);
                ctx.db.party_inventory_item().id().delete(inventory.id);
                ctx.db.food_contamination().food_lot_id().delete(lot.id);
                ctx.db.food_lot().id().delete(lot.id);
            } else {
                let retained = 1.0 - ratio;
                let state = ctx
                    .db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .find(inventory.id)
                    .ok_or("Party food amount state is missing")?;
                retain_lot_fraction(&mut lot, retained)?;
                ctx.db.food_lot().id().update(lot);
                let current = ConsumableFractionMicros::try_new(state.remaining_fraction_micros)
                    .expect("persisted consumable fraction must not exceed one whole");
                let mut remaining = current
                    .try_scaled_floor(retained)
                    .map_err(|_| "Retained party food fraction is invalid")?;
                if remaining.is_zero() {
                    remaining = ConsumableFractionMicros::MINIMUM_NONZERO;
                }
                ctx.db.party_item_amount().party_inventory_item_id().update(
                    crate::PartyItemAmount {
                        party_inventory_item_id: inventory.id,
                        remaining_fraction_micros: remaining.get(),
                    },
                );
            }
        }
    }
    let mut personal: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|inventory| {
            !crate::inventory_container::row_is_fireplace_rooted(
                ctx,
                CarriedInventoryScope::Personal,
                inventory.id,
            )
        })
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
