//! Durable strategic filth and automatic washing.

use adventuresim_core::filth::{self, Deposit, DiseaseSnapshot, Substance, WashStack};
use spacetimedb::{ReducerContext, SpacetimeType, Table, table};

use crate::character::character;
use crate::personality::character_personality;
use crate::{character_time, infection_episode, inventory_item, limb_injury, party_inventory_item};

pub const SOAP_ITEM_ID: &str = "soft_soap";

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum FilthSubstance {
    Dirt,
    Blood,
}

/// Sanitized visible deposit. Disease identity is never copied into this table.
#[derive(Clone, Debug)]
#[table(accessor = character_filth, public)]
pub struct CharacterFilth {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub substance: FilthSubstance,
    pub source_character_id: Option<u64>,
    pub amount: u16,
    pub deposited_at: u64,
}

/// Private snapshot of source infection provenance at the instant of deposition.
#[derive(Clone, Debug)]
#[table(accessor = filth_disease_snapshot)]
pub struct FilthDiseaseSnapshot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub filth_id: u64,
    pub disease_id: String,
    pub episode_id: u64,
}

fn total(ctx: &ReducerContext, character_id: u64) -> u16 {
    ctx.db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|d| d.amount)
        .fold(0u16, u16::saturating_add)
        .min(filth::MAX_FILTH)
}

pub fn dirt_total(ctx: &ReducerContext, character_id: u64) -> u16 {
    ctx.db
        .character_filth()
        .character_id()
        .filter(character_id)
        .filter(|d| d.substance == FilthSubstance::Dirt)
        .map(|d| d.amount)
        .fold(0u16, u16::saturating_add)
        .min(filth::MAX_FILTH)
}

/// Reusable strategic boundary: callers persist only final dirt/blood outcomes.
pub fn deposit(
    ctx: &ReducerContext,
    character_id: u64,
    substance: FilthSubstance,
    source_character_id: Option<u64>,
    amount: u16,
    at: u64,
) -> Option<u64> {
    let amount = filth::bounded_deposit_amount(total(ctx, character_id), amount);
    if amount == 0 {
        return None;
    }
    let row = ctx.db.character_filth().insert(CharacterFilth {
        id: 0,
        character_id,
        substance,
        source_character_id,
        amount,
        deposited_at: at,
    });
    if substance == FilthSubstance::Blood {
        if let Some(source) = source_character_id {
            for episode in ctx.db.infection_episode().character_id().filter(source) {
                ctx.db
                    .filth_disease_snapshot()
                    .insert(FilthDiseaseSnapshot {
                        id: 0,
                        filth_id: row.id,
                        disease_id: episode.disease_id,
                        episode_id: episode.id,
                    });
            }
        }
    }
    Some(row.id)
}

pub fn deposit_now(
    ctx: &ReducerContext,
    character_id: u64,
    substance: FilthSubstance,
    source_character_id: Option<u64>,
    amount: u16,
) -> Option<u64> {
    let at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    deposit(
        ctx,
        character_id,
        substance,
        source_character_id,
        amount,
        at,
    )
}

fn deposits(ctx: &ReducerContext, character_id: u64) -> Vec<Deposit> {
    ctx.db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|row| {
            let diseases = ctx
                .db
                .filth_disease_snapshot()
                .filth_id()
                .filter(row.id)
                .filter_map(|s| {
                    crate::disease::parse_id(&s.disease_id)
                        .ok()
                        .map(|disease_id| DiseaseSnapshot {
                            disease_id,
                            episode_id: s.episode_id,
                        })
                })
                .collect();
            Deposit {
                id: row.id,
                character_id,
                substance: match row.substance {
                    FilthSubstance::Dirt => Substance::Dirt,
                    FilthSubstance::Blood => Substance::Blood,
                },
                source_character_id: row.source_character_id,
                amount: row.amount,
                deposited_at: row.deposited_at,
                diseases,
            }
        })
        .collect()
}

fn has_cut(ctx: &ReducerContext, character_id: u64) -> bool {
    ctx.db
        .limb_injury()
        .character_id()
        .filter(character_id)
        .any(|i| i.cut_damage > 0.0)
}

fn consume_personal(ctx: &ReducerContext, stack_id: u64, quantity: u32) {
    if let Some(mut stack) = ctx.db.inventory_item().id().find(stack_id) {
        stack.quantity -= quantity;
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
    }
}
fn consume_party(ctx: &ReducerContext, stack_id: u64, quantity: u32) {
    if let Some(mut stack) = ctx.db.party_inventory_item().id().find(stack_id) {
        stack.quantity -= quantity;
        if stack.quantity == 0 {
            ctx.db.party_inventory_item().id().delete(stack.id);
        } else {
            ctx.db.party_inventory_item().id().update(stack);
        }
    }
}

/// Washes one living character, drawing stable personal stacks before the party pool.
pub fn wash_before_explicit_rest(ctx: &ReducerContext, character_id: u64) -> Result<u32, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.alive {
        return Ok(0);
    }
    let dirty = deposits(ctx, character_id);
    if dirty.is_empty() {
        return Ok(0);
    }
    let personal: Vec<_> = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, SOAP_ITEM_ID))
        .map(|s| WashStack {
            id: s.id,
            quantity: s.quantity,
            personal: true,
        })
        .collect();
    let shared: Vec<_> = character
        .party_id
        .as_ref()
        .map_or_else(Vec::new, |party_id| {
            ctx.db
                .party_inventory_item()
                .iter()
                .filter(|s| &s.party_id == party_id && s.item_id == SOAP_ITEM_ID)
                .map(|s| WashStack {
                    id: s.id,
                    quantity: s.quantity,
                    personal: false,
                })
                .collect()
        });
    // Plan in two stages so colliding IDs in independent tables remain unambiguous.
    let personal_plan = filth::plan_wash(&dirty, &personal, has_cut(ctx, character_id));
    let personal_units: u32 = personal_plan.soap_stacks.iter().map(|(_, q)| *q).sum();
    let total_needed = dirty
        .iter()
        .map(|d| u32::from(d.amount))
        .sum::<u32>()
        .div_ceil(u32::from(filth::SOAP_CLEANSING_CAPACITY));
    let shared_needed = total_needed.saturating_sub(personal_units);
    let mut shared_limited = shared;
    let mut remaining = shared_needed;
    shared_limited.sort_by_key(|s| s.id);
    for stack in &mut shared_limited {
        let keep = stack.quantity.min(remaining);
        stack.quantity = keep;
        remaining -= keep;
    }
    shared_limited.retain(|s| s.quantity > 0);
    let all = personal
        .iter()
        .copied()
        .chain(shared_limited.iter().copied())
        .collect::<Vec<_>>();
    let plan = filth::plan_wash(&dirty, &all, has_cut(ctx, character_id));
    for (id, qty) in &plan.soap_stacks {
        if personal.iter().any(|s| s.id == *id) {
            consume_personal(ctx, *id, *qty);
        } else {
            consume_party(ctx, *id, *qty);
        }
    }
    for (id, removed) in plan.cleaned_deposits {
        if let Some(mut row) = ctx.db.character_filth().id().find(id) {
            row.amount -= removed;
            if row.amount == 0 {
                for snapshot in ctx
                    .db
                    .filth_disease_snapshot()
                    .filth_id()
                    .filter(id)
                    .collect::<Vec<_>>()
                {
                    ctx.db.filth_disease_snapshot().id().delete(snapshot.id);
                }
                ctx.db.character_filth().id().delete(id);
            } else {
                ctx.db.character_filth().id().update(row);
            }
        }
    }
    Ok(plan.soap_stacks.iter().map(|(_, q)| *q).sum())
}

pub fn preview_soap_units(ctx: &ReducerContext, character_id: u64) -> u32 {
    let amount: u32 = ctx
        .db
        .character_filth()
        .character_id()
        .filter(character_id)
        .map(|d| u32::from(d.amount))
        .sum();
    if amount == 0 {
        return 0;
    }
    let available: u32 = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, SOAP_ITEM_ID))
        .map(|s| s.quantity)
        .sum::<u32>()
        + ctx
            .db
            .character()
            .id()
            .find(character_id)
            .and_then(|c| c.party_id)
            .map_or(0, |p| {
                ctx.db
                    .party_inventory_item()
                    .iter()
                    .filter(|s| s.party_id == p && s.item_id == SOAP_ITEM_ID)
                    .map(|s| s.quantity)
                    .sum()
            });
    amount
        .div_ceil(u32::from(filth::SOAP_CLEANSING_CAPACITY))
        .min(available)
}

pub(crate) fn seed_demo(ctx: &ReducerContext, character_id: u64, foreign_source_id: u64) {
    for row in ctx
        .db
        .character_filth()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        for snapshot in ctx
            .db
            .filth_disease_snapshot()
            .filth_id()
            .filter(row.id)
            .collect::<Vec<_>>()
        {
            ctx.db.filth_disease_snapshot().id().delete(snapshot.id);
        }
        ctx.db.character_filth().id().delete(row.id);
    }
    deposit(ctx, character_id, FilthSubstance::Dirt, None, 38, 86_000);
    deposit(
        ctx,
        character_id,
        FilthSubstance::Blood,
        Some(foreign_source_id),
        27,
        86_200,
    );
    if let Some(mut personality) = ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
    {
        personality.hygiene = crate::personality::Hygiene::Cleanly;
        ctx.db
            .character_personality()
            .character_id()
            .update(personality);
    }
    let existing = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, SOAP_ITEM_ID))
        .map(|row| row.quantity)
        .sum::<u32>();
    if existing < 3 {
        crate::add_inventory_item(ctx, character_id, SOAP_ITEM_ID, 3 - existing);
    }
}
