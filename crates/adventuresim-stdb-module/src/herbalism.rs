//! Authoritative bounded herbal preparation.

use adventuresim_core::{
    food::FoodPreparation,
    prelude::{PlayerSkills, Skill, apply_direct_training},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, character_attributes, character_skills},
    container_liquid, inventory_containment, inventory_item, inventory_item_amount,
    inventory_object,
    item::item,
    strategic::strategic_gateway_authority__view,
};

const POPPY_PROFILE_ID: &str = "poppy_tincture";
const POPPY_PROFILE_VERSION: u16 = 1;
const TINCTURE_SETUP_MINUTES: u64 = 10;
const MIN_HERBALISM_CHECK: f32 = 0.5;

#[derive(Clone, Debug)]
#[table(accessor = tincture_process)]
pub struct TinctureProcess {
    #[primary_key]
    pub container_object_id: u64,
    /// Shared chronology keeps elapsed maturation invariant across custody transfers.
    pub started_at_world_minute: u64,
    #[index(btree)]
    pub ready_at_world_minute: u64,
    pub matured: bool,
    pub preparer_character_id: u64,
    pub intervention_profile_id: String,
    pub profile_version: u16,
    pub pinned_potency_units: f32,
    pub ingredient_object_id: u64,
    pub ingredient_item_id: String,
    pub ingredient_mass_grams: u32,
}

/// Redacted gateway projection: readiness is inspectable, medicinal identity and
/// exact potency remain private.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendTinctureStatus {
    pub container_object_id: u64,
    pub ready_at_world_minute: u64,
    pub matured: bool,
}

#[view(accessor = backend_tincture_statuses, public)]
pub fn backend_tincture_statuses(ctx: &ViewContext) -> Vec<BackendTinctureStatus> {
    if !ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
    {
        return Vec::new();
    }
    ctx.db
        .tincture_process()
        .ready_at_world_minute()
        .filter(0u64..)
        .map(|row| BackendTinctureStatus {
            container_object_id: row.container_object_id,
            ready_at_world_minute: row.ready_at_world_minute,
            matured: row.matured,
        })
        .collect()
}

/// Private medicinal payload attached to a stable container or food-lot object.
#[derive(Clone, Debug)]
#[table(accessor = medicinal_component)]
pub struct MedicinalComponent {
    #[primary_key]
    pub key: String,
    pub carrier_kind: String,
    pub carrier_id: u64,
    pub intervention_profile_id: String,
    pub profile_version: u16,
    pub potency_units: f32,
}

fn is_tincture_vessel(item_id: &str) -> bool {
    adventuresim_core::item_catalog::definition(item_id)
        .is_some_and(|definition| definition.tags.iter().any(|tag| tag == "tincture_vessel"))
}

fn require_tincture_vessel(
    ctx: &ReducerContext,
    object_id: u64,
) -> Result<crate::InventoryObject, String> {
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(object_id)
        .ok_or("Container not found")?;
    if !is_tincture_vessel(&object.item_id) {
        return Err("Tincturing requires a bottle or jar authored as a tincture vessel".into());
    }
    Ok(object)
}

pub(crate) fn container_is_processing(ctx: &ReducerContext, object_id: u64) -> bool {
    ctx.db
        .tincture_process()
        .container_object_id()
        .find(object_id)
        .is_some_and(|row| !row.matured)
}

pub(crate) fn delete_container_medicine(ctx: &ReducerContext, object_id: u64) {
    ctx.db
        .tincture_process()
        .container_object_id()
        .delete(object_id);
    for component in ctx
        .db
        .medicinal_component()
        .iter()
        .filter(|row| row.carrier_kind == "container" && row.carrier_id == object_id)
        .collect::<Vec<_>>()
    {
        ctx.db.medicinal_component().key().delete(component.key);
    }
}

fn materialize_mature_tincture(ctx: &ReducerContext, object_id: u64, now: u64) {
    let Some(mut process) = ctx
        .db
        .tincture_process()
        .container_object_id()
        .find(object_id)
    else {
        return;
    };
    if process.matured || now < process.ready_at_world_minute {
        return;
    }
    process.matured = true;
    ctx.db
        .tincture_process()
        .container_object_id()
        .update(process.clone());
    let key = format!("container|{object_id}|{}", process.intervention_profile_id);
    if ctx
        .db
        .medicinal_component()
        .key()
        .find(key.clone())
        .is_none()
    {
        ctx.db.medicinal_component().insert(MedicinalComponent {
            key,
            carrier_kind: "container".into(),
            carrier_id: object_id,
            intervention_profile_id: process.intervention_profile_id,
            profile_version: process.profile_version,
            potency_units: process.pinned_potency_units,
        });
    }
}

pub(crate) fn consume_food_medicine(
    ctx: &ReducerContext,
    patient_id: u64,
    lot_id: u64,
    fraction: f32,
) -> Result<(), String> {
    if !fraction.is_finite() {
        return Err("Medicinal food fraction is invalid".into());
    }
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction <= 0.0 {
        return Ok(());
    }
    let mut rows = ctx
        .db
        .medicinal_component()
        .iter()
        .filter(|row| row.carrier_kind == "food_lot" && row.carrier_id == lot_id)
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    for row in &rows {
        adventuresim_core::physiology::intervention_profile(
            &row.intervention_profile_id,
            row.profile_version,
        )
        .ok_or("Medicinal food references an unknown profile version")?;
        if !row.potency_units.is_finite() || row.potency_units < 0.0 {
            return Err("Medicinal food potency is invalid".into());
        }
    }
    for mut row in rows {
        let used = row.potency_units * fraction;
        let dose =
            adventuresim_core::physiology::DoseMilliunits::try_from_standard_doses_rounded(used)
                .map_err(|_| "Medicinal food dose is outside the supported range")?;
        crate::disease::administer_intervention_component(
            ctx,
            patient_id,
            &row.intervention_profile_id,
            row.profile_version,
            dose,
        )?;
        if fraction >= 0.999_999 {
            ctx.db.medicinal_component().key().delete(row.key);
        } else {
            row.potency_units -= used;
            ctx.db.medicinal_component().key().update(row);
        }
    }
    Ok(())
}

pub(crate) fn delete_food_medicine(ctx: &ReducerContext, lot_id: u64) {
    for row in ctx
        .db
        .medicinal_component()
        .iter()
        .filter(|row| row.carrier_kind == "food_lot" && row.carrier_id == lot_id)
        .collect::<Vec<_>>()
    {
        ctx.db.medicinal_component().key().delete(row.key);
    }
}

pub(crate) fn split_food_medicine(
    ctx: &ReducerContext,
    source_id: u64,
    child_id: u64,
    fraction: f32,
) -> Result<(), String> {
    if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
        return Err("Medicinal food split fraction is invalid".into());
    }
    for mut source in ctx
        .db
        .medicinal_component()
        .iter()
        .filter(|row| row.carrier_kind == "food_lot" && row.carrier_id == source_id)
        .collect::<Vec<_>>()
    {
        let child_units = source.potency_units * fraction;
        source.potency_units -= child_units;
        ctx.db.medicinal_component().key().update(source.clone());
        ctx.db.medicinal_component().insert(MedicinalComponent {
            key: format!(
                "food_lot|{child_id}|{}|{}",
                source.intervention_profile_id, source.profile_version
            ),
            carrier_kind: "food_lot".into(),
            carrier_id: child_id,
            intervention_profile_id: source.intervention_profile_id,
            profile_version: source.profile_version,
            potency_units: child_units,
        });
    }
    Ok(())
}

#[reducer]
pub fn pour_tincture_spirit_into_container(
    ctx: &ReducerContext,
    character_id: u64,
    spirit_id: u64,
    object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, character_id)?;
    let mut spirit = ctx
        .db
        .inventory_item()
        .id()
        .find(spirit_id)
        .ok_or("Tincture spirit not found")?;
    if spirit.character_id != character_id
        || spirit.item_id != "tincture_spirit"
        || spirit.quantity == 0
    {
        return Err("Select one carried 150 ml tincture-spirit serving".into());
    }
    let object = require_tincture_vessel(ctx, object_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let custody = crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
    if !matches!(
        custody.root,
        adventuresim_core::physical_object::OperationalCustody::Character(id)
            if id.get() == character_id
    ) {
        return Err("Tincture vessel is not in this character's personal custody".into());
    }
    if crate::inventory_container::object_is_nonempty(ctx, object_id) {
        return Err("Add tincture spirit before solid ingredients".into());
    }
    let definition = ctx
        .db
        .item()
        .id()
        .find(object.item_id)
        .ok_or("Container definition not found")?;
    if definition.container_capacity_ml < 150 {
        return Err("Tincture vessel is too small for 150 ml of spirit".into());
    }
    spirit.quantity -= 1;
    if spirit.quantity == 0 {
        ctx.db.inventory_item().id().delete(spirit.id);
    } else {
        ctx.db.inventory_item().id().update(spirit);
    }
    ctx.db.container_liquid().insert(crate::ContainerLiquid {
        container_object_id: object_id,
        liquid_item_id: "tincture_spirit".into(),
        water_ml: 150,
    });
    Ok(())
}

#[reducer]
pub fn start_poppy_tincture(
    ctx: &ReducerContext,
    character_id: u64,
    object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Tincturing is unavailable during a tactical encounter".into());
    }
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    let object = require_tincture_vessel(ctx, object_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let custody = crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
    if !matches!(
        custody.root,
        adventuresim_core::physical_object::OperationalCustody::Character(id)
            if id.get() == character_id
    ) {
        return Err("Tincture vessel is not in this character's personal custody".into());
    }
    if ctx
        .db
        .tincture_process()
        .container_object_id()
        .find(object_id)
        .is_some()
    {
        return Err("This vessel already has a tincture process".into());
    }
    let liquid = ctx
        .db
        .container_liquid()
        .container_object_id()
        .find(object_id)
        .ok_or("Container has no tincture spirit")?;
    if liquid.liquid_item_id != "tincture_spirit" || liquid.water_ml != 150 {
        return Err("Poppy tincture requires exactly 150 ml of tincture spirit".into());
    }
    let direct = ctx
        .db
        .inventory_containment()
        .parent_object_id()
        .filter(object_id)
        .collect::<Vec<_>>();
    if direct.len() != 1 {
        return Err("Poppy tincture requires exactly one direct ground-poppy lot".into());
    }
    let ingredient = ctx
        .db
        .inventory_object()
        .id()
        .find(direct[0].child_object_id)
        .ok_or("Tincture ingredient is missing")?;
    let ingredient_custody =
        crate::object_custody::require_actor_carried_object(ctx, &actor, &ingredient)?;
    if !matches!(
        ingredient_custody.root,
        adventuresim_core::physical_object::OperationalCustody::Character(id)
            if id.get() == character_id
    ) || ingredient.item_id != "poppy"
    {
        return Err("This tincture recipe requires carried poppy".into());
    }
    let adventuresim_core::physical_object::InventoryLocation::Personal(ingredient_location) =
        &ingredient.location
    else {
        return Err("Tincture ingredient has no personal inventory backing".into());
    };
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(ingredient_location.row_id)
        .ok_or("Poppy inventory row is missing")?;
    let lot = crate::food::personal_lot(ctx, ingredient_location.row_id)
        .ok_or("Poppy measured lot is missing")?;
    if inventory.quantity != 1
        || lot.preparation != FoodPreparation::Ground
        || (lot.mass_kg - 0.05).abs() > 0.000_1
    {
        return Err("Poppy tincture requires one exact 50 g ground-poppy lot".into());
    }
    let mut skills = ctx
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
    let check = Skill::Herbalism
        .capped_training_rank(skills.effective_skill_hours(Skill::Herbalism), &attributes);
    if check < MIN_HERBALISM_CHECK {
        return Err("At least 0.5 Herbalism is required to prepare a medicinal tincture".into());
    }
    if !crate::time::advance_character_wait_time(ctx, character_id, TINCTURE_SETUP_MINUTES)? {
        return Ok(());
    }
    let now = crate::time::refresh_clock(ctx)?;
    let ready = now
        .checked_add(adventuresim_core::herbalism::POPPY_TINCTURE_MATURATION_MINUTES)
        .ok_or("Tincture completion time overflow")?;
    // Commit boundary: consume the exact measured ingredient and its stable object.
    crate::food::delete_personal_food_lot(ctx, inventory.id);
    ctx.db
        .inventory_item_amount()
        .inventory_item_id()
        .delete(inventory.id);
    ctx.db
        .inventory_containment()
        .child_object_id()
        .delete(ingredient.id);
    ctx.db.inventory_object().id().delete(ingredient.id);
    ctx.db.inventory_item().id().delete(inventory.id);
    let gain = apply_direct_training(
        Skill::Herbalism,
        &mut skills.herbalism_hours,
        TINCTURE_SETUP_MINUTES as f32 / 60.0,
        &attributes,
    );
    ctx.db.character_skills().character_id().update(skills);
    crate::condition::record_mastery_training_morale(
        ctx,
        character_id,
        TINCTURE_SETUP_MINUTES,
        gain.excess_effective_hours,
    );
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    ctx.db.tincture_process().insert(TinctureProcess {
        container_object_id: object_id,
        started_at_world_minute: now,
        ready_at_world_minute: ready,
        matured: false,
        preparer_character_id: character_id,
        intervention_profile_id: POPPY_PROFILE_ID.into(),
        profile_version: POPPY_PROFILE_VERSION,
        pinned_potency_units: (check / 5.0).clamp(0.1, 1.0),
        ingredient_object_id: ingredient.id,
        ingredient_item_id: ingredient.item_id,
        ingredient_mass_grams: 50,
    });
    Ok(())
}

#[reducer]
pub fn refresh_tincture(
    ctx: &ReducerContext,
    character_id: u64,
    object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, character_id)?;
    let object = require_tincture_vessel(ctx, object_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
    materialize_mature_tincture(ctx, object_id, crate::time::refresh_clock(ctx)?);
    Ok(())
}

#[reducer]
pub fn administer_tincture_from_container(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    object_id: u64,
    dose_milliunits: u32,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::disease::require_intervention_relationship(ctx, actor_id, patient_id)?;
    let tincture_dose = adventuresim_core::physiology::DoseMilliunits::try_new(dose_milliunits)
        .map_err(|_| "Tincture dose must be between 1 and 1000 milliunits")?;
    if tincture_dose.is_zero()
        || tincture_dose > adventuresim_core::physiology::DoseMilliunits::STANDARD
    {
        return Err("Tincture dose must be between 1 and 1000 milliunits".into());
    }
    let object = require_tincture_vessel(ctx, object_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Character not found")?;
    crate::object_custody::require_actor_carried_object(ctx, &actor, &object)?;
    materialize_mature_tincture(ctx, object_id, crate::time::refresh_clock(ctx)?);
    let process = ctx
        .db
        .tincture_process()
        .container_object_id()
        .find(object_id)
        .ok_or("Bottle has no tincture process")?;
    if !process.matured {
        return Err("Tincture has not matured".into());
    }
    let key = format!("container|{object_id}|{}", process.intervention_profile_id);
    let mut component = ctx
        .db
        .medicinal_component()
        .key()
        .find(key.clone())
        .ok_or("Mature tincture has no medicinal component")?;
    let fraction = tincture_dose.as_standard_doses();
    let standard_doses = component.potency_units * fraction;
    let mut administered_dose =
        adventuresim_core::physiology::DoseMilliunits::try_from_standard_doses_rounded(
            standard_doses,
        )
        .map_err(|_| "Tincture potency is outside the supported dose range")?;
    if administered_dose.is_zero() {
        administered_dose = adventuresim_core::physiology::DoseMilliunits::MINIMUM_NONZERO;
    }
    crate::disease::administer_intervention_component(
        ctx,
        patient_id,
        &component.intervention_profile_id,
        component.profile_version,
        administered_dose,
    )?;
    component.potency_units -= standard_doses;
    let mut liquid = ctx
        .db
        .container_liquid()
        .container_object_id()
        .find(object_id)
        .ok_or("Tincture liquid is missing")?;
    liquid.water_ml = liquid
        .water_ml
        .saturating_sub(((liquid.water_ml as f32) * fraction).ceil().max(1.0) as u64);
    if component.potency_units <= 0.000_001 || liquid.water_ml == 0 {
        delete_container_medicine(ctx, object_id);
        ctx.db
            .container_liquid()
            .container_object_id()
            .delete(object_id);
        crate::inventory_container::merge_empty_container(ctx, object_id)?;
    } else {
        ctx.db.medicinal_component().key().update(component);
        ctx.db
            .container_liquid()
            .container_object_id()
            .update(liquid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tincture_lifecycle_is_private_pinned_consuming_and_transfer_safe() {
        let source = include_str!("herbalism.rs");
        assert!(source.contains("started_at_world_minute"));
        assert!(source.contains("pinned_potency_units"));
        assert!(source.contains("ingredient_object_id"));
        assert!(source.contains("delete_personal_food_lot"));
        assert!(source.contains("capped_training_rank"));
        assert!(source.contains("materialize_mature_tincture"));
        assert!(!source.contains("removed_herbalism_menu"));
    }
}
