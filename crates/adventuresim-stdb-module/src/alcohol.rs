//! Discrete alcohol consumption, durable evening history, and shared selection.

use adventuresim_core::alcohol::{
    AlcoholProperties, HEAVY_ETHANOL_ML, LOW_MORALE_THRESHOLD, NIGHTLY_MORALE_SOURCE_ID,
    ROLLING_WEEK_DAYS, TemperancePreference, emergency_hydration_ml, ethanol_ml, evening_target,
    nightly_morale_effect, rest_evenings,
};
use spacetimedb::{ReducerContext, Table, table};

use crate::character::character;
use crate::condition::character_strategic_condition;
use crate::item::item;
use crate::{inventory_item, inventory_quantity_target, party, party_inventory_item};

pub const TAVERN_DRINK_ITEM_ID: &str = "table_wine";
#[derive(Clone, Debug)]
#[table(
    accessor = alcohol_consumption, public,
    index(accessor = by_character, btree(columns = [character_id])),
)]
pub struct AlcoholConsumption {
    #[primary_key]
    pub id: String,
    pub character_id: u64,
    pub evening_id: u64,
    pub ethanol_ml: u32,
    pub morale_evaluated: bool,
}

pub fn properties(item: &crate::Item) -> AlcoholProperties {
    AlcoholProperties {
        serving_ml: item.alcohol_serving_ml,
        abv_basis_points: item.alcohol_abv_basis_points,
        net_hydration_ml: item.alcohol_net_hydration_ml,
        disinfectant_effectiveness: item.alcohol_disinfectant_effectiveness,
        disinfectant_focused: item.alcohol_disinfectant_focused,
        potable: item.alcohol_potable,
    }
}

fn ledger_id(character_id: u64, evening_id: u64) -> String {
    format!("{character_id}:{evening_id}")
}

pub fn record_consumed_ethanol(ctx: &ReducerContext, character_id: u64, minute: u64, amount: u32) {
    if amount == 0 {
        return;
    }
    let evening_id = adventuresim_core::alcohol::evening_id(minute);
    let id = ledger_id(character_id, evening_id);
    if let Some(mut row) = ctx.db.alcohol_consumption().id().find(&id) {
        row.ethanol_ml = row.ethanol_ml.saturating_add(amount);
        ctx.db.alcohol_consumption().id().update(row);
    } else {
        ctx.db.alcohol_consumption().insert(AlcoholConsumption {
            id,
            character_id,
            evening_id,
            ethanol_ml: amount,
            morale_evaluated: false,
        });
    }
}

fn target_for(ctx: &ReducerContext, owner: u64, party_scope: bool, item_id: &str) -> u32 {
    ctx.db
        .inventory_quantity_target()
        .owner_and_scope()
        .filter((owner, party_scope))
        .find(|row| row.item_id == item_id)
        .map_or(0, |row| row.quantity)
}

#[derive(Clone)]
enum Stack {
    Party(crate::PartyInventoryItem),
    Personal(crate::InventoryItem),
}

fn available_stacks(
    ctx: &ReducerContext,
    character_id: u64,
    settled: bool,
    allow_medical: bool,
    hydration: bool,
) -> Vec<(Stack, crate::Item)> {
    let character = match ctx.db.character().id().find(character_id) {
        Some(row) => row,
        None => return Vec::new(),
    };
    let mut rows = Vec::new();
    if let Some(party_id) = character.party_id.as_ref()
        && let Some(party) = ctx.db.party().id().find(party_id)
    {
        for stack in ctx.db.party_inventory_item().party_id().filter(party_id) {
            if let Some(def) = ctx.db.item().id().find(&stack.item_id) {
                let p = properties(&def);
                if p.potable
                    && ethanol_ml(p) > 0
                    && (!hydration || emergency_hydration_ml(p) > 0)
                    && (allow_medical || !p.disinfectant_focused)
                    && adventuresim_core::alcohol::consumable_units(
                        ctx.db
                            .party_inventory_item()
                            .party_id()
                            .filter(party_id)
                            .filter(|row| row.item_id == stack.item_id)
                            .map(|row| row.quantity)
                            .sum::<u32>(),
                        target_for(ctx, party.leader_id, true, &stack.item_id),
                        settled,
                    ) > 0
                {
                    rows.push((Stack::Party(stack), def));
                }
            }
        }
    }
    for stack in ctx.db.inventory_item().character_id().filter(character_id) {
        if let Some(def) = ctx.db.item().id().find(&stack.item_id) {
            let p = properties(&def);
            if p.potable
                && ethanol_ml(p) > 0
                && (!hydration || emergency_hydration_ml(p) > 0)
                && (allow_medical || !p.disinfectant_focused)
                && adventuresim_core::alcohol::consumable_units(
                    ctx.db
                        .inventory_item()
                        .character_id()
                        .filter(character_id)
                        .filter(|row| row.item_id == stack.item_id)
                        .map(|row| row.quantity)
                        .sum::<u32>(),
                    target_for(ctx, character_id, false, &stack.item_id),
                    settled,
                ) > 0
            {
                rows.push((Stack::Personal(stack), def));
            }
        }
    }
    rows.sort_by(|(a, ad), (b, bd)| {
        let scope = |s: &Stack| if matches!(s, Stack::Party(_)) { 0 } else { 1 };
        let id = |s: &Stack| match s {
            Stack::Party(x) => x.id,
            Stack::Personal(x) => x.id,
        };
        (
            scope(a),
            ad.alcohol_disinfectant_effectiveness,
            &ad.id,
            id(a),
        )
            .cmp(&(
                scope(b),
                bd.alcohol_disinfectant_effectiveness,
                &bd.id,
                id(b),
            ))
    });
    rows
}

fn consume_stack(ctx: &ReducerContext, stack: Stack) {
    match stack {
        Stack::Party(mut row) => {
            row.quantity -= 1;
            if row.quantity == 0 {
                ctx.db.party_inventory_item().id().delete(row.id);
            } else {
                ctx.db.party_inventory_item().id().update(row);
            }
        }
        Stack::Personal(mut row) => {
            row.quantity -= 1;
            if row.quantity == 0 {
                ctx.db.inventory_item().id().delete(row.id);
            } else {
                ctx.db.inventory_item().id().update(row);
            }
        }
    }
}

fn consume_for_ethanol(
    ctx: &ReducerContext,
    character_id: u64,
    target: u32,
    settled: bool,
    allow_medical: bool,
) -> u32 {
    let mut total = 0_u32;
    while total < target {
        let Some((stack, def)) = available_stacks(ctx, character_id, settled, allow_medical, false)
            .into_iter()
            .next()
        else {
            break;
        };
        total = total.saturating_add(ethanol_ml(properties(&def)));
        consume_stack(ctx, stack);
    }
    total
}

fn current_morale(ctx: &ReducerContext, character_id: u64) -> f32 {
    ctx.db
        .character_strategic_condition()
        .character_id()
        .find(character_id)
        .map_or(0.0, |row| row.morale)
}

fn ordinary_potable_exists(ctx: &ReducerContext, character_id: u64) -> bool {
    let personal = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .any(|row| {
            row.quantity > 0
                && ctx.db.item().id().find(&row.item_id).is_some_and(|def| {
                    def.alcohol_potable
                        && def.alcohol_abv_basis_points > 0
                        && !def.alcohol_disinfectant_focused
                })
        });
    personal
        || ctx
            .db
            .character()
            .id()
            .find(character_id)
            .and_then(|row| row.party_id)
            .is_some_and(|party_id| {
                ctx.db
                    .party_inventory_item()
                    .party_id()
                    .filter(&party_id)
                    .any(|row| {
                        row.quantity > 0
                            && ctx.db.item().id().find(&row.item_id).is_some_and(|def| {
                                def.alcohol_potable
                                    && def.alcohol_abv_basis_points > 0
                                    && !def.alcohol_disinfectant_focused
                            })
                    })
            })
}

fn had_recent_heavy(ctx: &ReducerContext, character_id: u64, evening: u64) -> bool {
    ctx.db
        .alcohol_consumption()
        .by_character()
        .filter(character_id)
        .any(|row| {
            row.evening_id < evening
                && evening - row.evening_id < ROLLING_WEEK_DAYS
                && row.ethanol_ml >= HEAVY_ETHANOL_ML
        })
}

fn tavern_purchase(ctx: &ReducerContext, character_id: u64, target: u32) -> u32 {
    let Some(def) = ctx.db.item().id().find(TAVERN_DRINK_ITEM_ID.to_string()) else {
        return 0;
    };
    let each = ethanol_ml(properties(&def));
    let price = u64::from(def.base_value.unwrap_or(0));
    let mut bought = 0_u32;
    let units = adventuresim_core::alcohol::tavern_units_affordable(
        target,
        each,
        price,
        crate::item::personal_currency_total(ctx, character_id),
    );
    for _ in 0..units {
        // Affordability was computed from the same personal-only balance. Keep
        // the reducer transaction authoritative if inventory changed.
        if crate::item::consume_personal_currency(ctx, character_id, price).is_err() {
            break;
        }
        bought = bought.saturating_add(each);
    }
    bought
}

pub fn process_rest_evenings(
    ctx: &ReducerContext,
    character_id: u64,
    start: u64,
    end: u64,
    settled: bool,
) -> Result<(), String> {
    use crate::personality::Temperance;
    let temperament = crate::personality::personality_or_neutral(ctx, character_id).temperance;
    if temperament == Temperance::Temperate {
        return Ok(());
    }
    let mut morale_changed = false;
    for evening in rest_evenings(start, end).map_err(str::to_string)? {
        let id = ledger_id(character_id, evening);
        if ctx
            .db
            .alcohol_consumption()
            .id()
            .find(&id)
            .is_some_and(|row| row.morale_evaluated)
        {
            continue;
        }
        let weekly_heavy =
            temperament == Temperance::Neutral && !had_recent_heavy(ctx, character_id, evening);
        let preference = match temperament {
            Temperance::Neutral => TemperancePreference::Neutral,
            Temperance::Temperate => TemperancePreference::Temperate,
            Temperance::Drunkard => TemperancePreference::Drunkard,
        };
        let had_recent_heavy = !weekly_heavy;
        let target = evening_target(preference, had_recent_heavy);
        let existing = ctx
            .db
            .alcohol_consumption()
            .id()
            .find(&id)
            .map_or(0, |row| row.ethanol_ml);
        let mut consumed = existing;
        if consumed < target {
            consumed = consumed.saturating_add(consume_for_ethanol(
                ctx,
                character_id,
                target - consumed,
                settled,
                false,
            ));
        }
        if consumed < target
            && temperament == Temperance::Drunkard
            && current_morale(ctx, character_id) < LOW_MORALE_THRESHOLD
            && !ordinary_potable_exists(ctx, character_id)
        {
            consumed = consumed.saturating_add(consume_for_ethanol(
                ctx,
                character_id,
                target - consumed,
                settled,
                true,
            ));
        }
        if settled && consumed < target {
            consumed =
                consumed.saturating_add(tavern_purchase(ctx, character_id, target - consumed));
        }
        let mut row = ctx
            .db
            .alcohol_consumption()
            .id()
            .find(&id)
            .unwrap_or(AlcoholConsumption {
                id: id.clone(),
                character_id,
                evening_id: evening,
                ethanol_ml: 0,
                morale_evaluated: false,
            });
        row.ethanol_ml = consumed;
        row.morale_evaluated = true;
        if ctx.db.alcohol_consumption().id().find(&id).is_some() {
            ctx.db.alcohol_consumption().id().update(row);
        } else {
            ctx.db.alcohol_consumption().insert(row);
        }
        let effect =
            nightly_morale_effect(evening, preference, had_recent_heavy, consumed >= target)
                .ok_or("Alcohol morale effect unexpectedly absent")?;
        crate::condition::upsert_refreshable_morale_event_at_without_refresh(
            ctx,
            character_id,
            effect.kind,
            f32::from(effect.magnitude),
            effect.occurred_at_minute,
            NIGHTLY_MORALE_SOURCE_ID,
        )?;
        morale_changed = true;
    }
    if morale_changed {
        crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    }
    Ok(())
}

pub fn consume_emergency_hydration(
    ctx: &ReducerContext,
    character_id: u64,
    requested_ml: f32,
    minute: u64,
) -> u32 {
    let mut supplied = 0_u32;
    while supplied as f32 + f32::EPSILON < requested_ml {
        let Some((stack, def)) = available_stacks(ctx, character_id, false, false, true)
            .into_iter()
            .next()
        else {
            break;
        };
        let p = properties(&def);
        supplied = supplied.saturating_add(emergency_hydration_ml(p));
        record_consumed_ethanol(ctx, character_id, minute, ethanol_ml(p));
        consume_stack(ctx, stack);
    }
    supplied
}

pub fn best_disinfectant(ctx: &ReducerContext, character_id: u64) -> Option<(u64, String, u16)> {
    let candidates = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter_map(|row| ctx.db.item().id().find(&row.item_id).map(|def| (row, def)))
        .filter(|(row, def)| row.quantity > 0 && def.alcohol_disinfectant_effectiveness > 0)
        .collect::<Vec<_>>();
    let ranking = candidates
        .iter()
        .map(|(row, def)| (def.alcohol_disinfectant_effectiveness, row.id))
        .collect::<Vec<_>>();
    adventuresim_core::alcohol::best_disinfectant(&ranking).map(|index| {
        let (row, def) = &candidates[index];
        (
            row.id,
            def.id.clone(),
            def.alcohol_disinfectant_effectiveness,
        )
    })
}

pub fn disinfectant_count(ctx: &ReducerContext, character_id: u64) -> u32 {
    ctx.db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|row| {
            ctx.db
                .item()
                .id()
                .find(&row.item_id)
                .is_some_and(|def| def.alcohol_disinfectant_effectiveness > 0)
        })
        .map(|row| row.quantity)
        .sum()
}

pub fn consume_inventory_row(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let row = ctx
        .db
        .inventory_item()
        .id()
        .find(id)
        .ok_or("Selected alcohol is no longer available")?;
    consume_stack(ctx, Stack::Personal(row));
    Ok(())
}
