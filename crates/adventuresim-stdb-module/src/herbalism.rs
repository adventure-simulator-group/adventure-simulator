//! Authoritative bounded herbal preparation.

use crate::{
    character::{character_attributes, character_skills},
    item::{InventoryItem, ItemKind, inventory_item, item},
};
use adventuresim_core::{
    attribute::PlayerAttributes,
    herbalism::{self, CraftOutcome},
    skill::{PlayerSkills, Skill, apply_direct_training},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HerbalPreparationMethod {
    DryGrind,
    InfuseDecoct,
    Tincture,
}

impl HerbalPreparationMethod {
    fn core(self) -> herbalism::PreparationMethod {
        match self {
            Self::DryGrind => herbalism::PreparationMethod::DryGrind,
            Self::InfuseDecoct => herbalism::PreparationMethod::InfuseDecoct,
            Self::Tincture => herbalism::PreparationMethod::Tincture,
        }
    }
}

fn capability(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    Ok(Skill::Herbalism.capped_rank_for_aptitude(
        skills.effective_skill_hours(Skill::Herbalism),
        attributes
            .raw_single_body_part_attr(adventuresim_core::attribute::SimpleAttribute::Intelligence),
    ))
}

/// Stable, aggregate consumption plan across any number of fungible personal
/// stacks. Each tuple contains the original row and its post-craft quantity.
fn consumable_plan(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    required_units: u32,
) -> Result<Vec<(InventoryItem, u32)>, String> {
    let mut rows = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|row| row.item_id == item_id)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.id);
    let available = rows.iter().try_fold(0u32, |total, row| {
        total
            .checked_add(row.quantity)
            .ok_or_else(|| "Consumable quantity could not be calculated".to_string())
    })?;
    if available < required_units {
        return Err(format!(
            "Preparation requires {required_units} unit(s) of {item_id}"
        ));
    }
    let mut needed = required_units;
    let mut plan = Vec::new();
    for row in rows {
        if needed == 0 {
            break;
        }
        let used = row.quantity.min(needed);
        let remaining = row
            .quantity
            .checked_sub(used)
            .ok_or("Consumable quantity could not be calculated")?;
        needed = needed
            .checked_sub(used)
            .ok_or("Consumable requirement could not be calculated")?;
        plan.push((row, remaining));
    }
    Ok(plan)
}

#[reducer]
pub fn prepare_herbal_remedy(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    method: HerbalPreparationMethod,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Herbalism is unavailable during a tactical encounter".into());
    }
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;

    // Every fallible lookup and calculation is completed before strategic time
    // advances. A terminal safe-prefix interruption can therefore return Ok
    // without consuming the ingredient, producing output, or training.
    let input = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Medicinal ingredient not found")?;
    if input.character_id != character_id {
        return Err("Medicinal ingredient is not in this character's inventory".into());
    }
    let definition = ctx
        .db
        .item()
        .id()
        .find(&input.item_id)
        .ok_or("Medicinal ingredient definition not found")?;
    if definition.kind != ItemKind::Ingredient {
        return Err("Selected inventory row is not an ingredient".into());
    }
    let herbalism_capability = capability(ctx, character_id)?;
    let preview = herbalism::preview(&input.item_id, method.core(), herbalism_capability)
        .ok_or("That ingredient and preparation method are not an authored remedy")?;
    if input.quantity < preview.input_units {
        return Err(format!(
            "Preparation requires {} whole ingredient unit(s)",
            preview.input_units
        ));
    }
    let output_id = match preview.outcome {
        CraftOutcome::Medication(id) | CraftOutcome::DegradedWaste(id) => id,
    };
    let output_definition = ctx
        .db
        .item()
        .id()
        .find(output_id.to_owned())
        .ok_or("Herbal preparation output is not in the item catalogue")?;
    match preview.outcome {
        CraftOutcome::Medication(_) if output_definition.kind != ItemKind::Medication => {
            return Err("Authored herbal output is not medication".into());
        }
        CraftOutcome::DegradedWaste(_) if output_definition.kind != ItemKind::Simple => {
            return Err("Authored degraded output is not waste".into());
        }
        _ => {}
    }
    let remaining = input
        .quantity
        .checked_sub(preview.input_units)
        .ok_or("Ingredient quantity could not be calculated")?;
    let consumables = if let Some(required) = preview.required_consumable {
        let consumable_definition = ctx
            .db
            .item()
            .id()
            .find(required.item_id.to_owned())
            .ok_or("Herbal preparation consumable is not in the item catalogue")?;
        if consumable_definition.kind != ItemKind::Ingredient {
            return Err("Authored herbal consumable is not an ingredient".into());
        }
        consumable_plan(ctx, character_id, required.item_id, required.units)?
    } else {
        Vec::new()
    };
    let duration = u64::from(preview.duration_minutes);

    if !crate::time::advance_character_wait_time(ctx, character_id, duration)? {
        return Ok(());
    }

    if remaining == 0 {
        ctx.db.inventory_item().id().delete(input.id);
    } else {
        ctx.db.inventory_item().id().update(InventoryItem {
            quantity: remaining,
            ..input
        });
    }
    for (row, remaining) in consumables {
        if remaining == 0 {
            ctx.db.inventory_item().id().delete(row.id);
        } else {
            ctx.db.inventory_item().id().update(InventoryItem {
                quantity: remaining,
                ..row
            });
        }
    }
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id,
        item_id: output_id.to_owned(),
        // Medication identity and transfer/admin lifecycle require an
        // individual, nonstacking row. Degraded waste is also one concrete row.
        quantity: 1,
    });
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills disappeared before herbalism training")?;
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes disappeared before herbalism training")?;
    let gain = apply_direct_training(
        Skill::Herbalism,
        &mut skills.herbalism_hours,
        duration as f32 / 60.0,
        &attributes,
    );
    ctx.db.character_skills().character_id().update(skills);
    crate::condition::record_mastery_training_morale(
        ctx,
        character_id,
        duration,
        gain.excess_effective_hours,
    );
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reducer_keeps_all_mutation_after_terminal_wait_boundary() {
        let source = include_str!("herbalism.rs");
        let wait = source.find("advance_character_wait_time").unwrap();
        let unresolved = source
            .find("require_character_no_unresolved_encounter(ctx, character_id)?")
            .unwrap();
        let delete = source.find("delete(input.id)").unwrap();
        let insert = source.find("insert(InventoryItem").unwrap();
        let training = source.find("apply_direct_training(").unwrap();
        assert!(unresolved < wait);
        assert!(wait < delete && wait < insert && wait < training);
        assert!(source.contains("require_strategic_gateway(ctx)?"));
        assert!(source.contains("require_living_character(ctx, character_id)?"));
        assert!(source.contains("require_character_no_unresolved_encounter(ctx, character_id)?"));
        assert!(source.contains("definition.kind != ItemKind::Ingredient"));
    }

    #[test]
    fn reducer_accepts_only_the_closed_typed_method() {
        let source = include_str!("herbalism.rs");
        assert!(source.contains("method: HerbalPreparationMethod"));
        assert!(!source.contains("method: String"));
    }

    #[test]
    fn tincture_consumable_is_aggregated_and_only_mutated_after_wait() {
        let source = include_str!("herbalism.rs");
        let plan = source.find("fn consumable_plan").unwrap();
        let aggregate = source[plan..].find("checked_add(row.quantity)").unwrap() + plan;
        let wait = source.find("advance_character_wait_time").unwrap();
        let consume = source.find("for (row, remaining) in consumables").unwrap();
        assert!(aggregate < wait && wait < consume);
        assert!(source.contains("rows.sort_by_key(|row| row.id)"));
        assert!(source.contains("available < required_units"));
    }
}
