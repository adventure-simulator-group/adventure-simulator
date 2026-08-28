//! Authoritative measured food lots and immediate free-form cooking.

use adventuresim_core::{
    disease::{self, DiseaseId},
    durability::{DamageBins, effective_weapon_stat},
    food::{self, CookingMethod, FoodPreparation, IngredientPreparationAction},
    herbalism,
    inventory_measurement::ConsumableFractionMicros,
    material::Microliters,
    physical_object::{
        CarriedInventoryScope, InventoryLocation, OperationalCustody, PhysicalObjectId,
    },
    prelude::{PlayerSkills, Skill, apply_direct_training},
    strategic_place::{StrategicFixtureId, StrategicPlaceId},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{
        character, character__view as _, character_attributes, character_attributes__view as _,
        character_limbs, character_skills, character_skills__view as _,
    },
    condition::{character_needs, initialize_character_condition},
    container_liquid,
    inventory_container::{inventory_containment__view as _, inventory_object__view as _},
    inventory_containment, inventory_item, inventory_item_amount, inventory_object,
    item::{inventory_item__view as _, item, item__view as _},
    medicinal_component, party_item_amount,
    repair::{item_condition, item_condition__view as _},
    strategic::{
        PartyInventoryItem, StrategicEncounterStatus, party_authority, party_authority__view as _,
        party_inventory_item, party_inventory_item__view as _, party_item_condition,
        party_item_condition__view as _, party_journey_authority,
        road_challenge_authority__view as _, settlement, strategic_encounter__view as _,
    },
    time::{character_time, character_time__view as _},
};

fn preparation_skill_check(
    ctx: &ReducerContext,
    character_id: u64,
    skill: Skill,
) -> Result<f32, String> {
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
    Ok(skill.capped_training_rank(skills.effective_skill_hours(skill), &attributes))
}

fn carried_item_rows(
    ctx: &ReducerContext,
    character_id: u64,
) -> Vec<(CarriedInventoryScope, u64, String)> {
    let Some(actor) = ctx.db.character().id().find(character_id) else {
        return Vec::new();
    };
    let mut rows = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|row| {
            crate::inventory_container::object_for_row(ctx, CarriedInventoryScope::Personal, row.id)
                .ok()
                .flatten()
                .is_some_and(|object| {
                    crate::object_custody::require_actor_carried_object(ctx, &actor, &object)
                        .is_ok()
                        && !crate::inventory_container::ancestry_reaches_fireplace(ctx, object.id)
                })
        })
        .map(|row| (CarriedInventoryScope::Personal, row.id, row.item_id))
        .collect::<Vec<_>>();
    if let Some(party_id) = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|row| row.party_id)
    {
        rows.extend(
            ctx.db
                .party_inventory_item()
                .party_id()
                .filter(&party_id)
                .filter(|row| {
                    crate::inventory_container::object_for_row(
                        ctx,
                        CarriedInventoryScope::Party,
                        row.id,
                    )
                    .ok()
                    .flatten()
                    .is_some_and(|object| {
                        crate::object_custody::require_actor_carried_object(ctx, &actor, &object)
                            .is_ok()
                            && !crate::inventory_container::ancestry_reaches_fireplace(
                                ctx, object.id,
                            )
                    })
                })
                .map(|row| (CarriedInventoryScope::Party, row.id, row.item_id)),
        );
    }
    rows
}

fn qualifying_cutting_weapon_binding(ctx: &ReducerContext, character_id: u64) -> Option<String> {
    carried_item_rows(ctx, character_id)
        .into_iter()
        .filter_map(|(scope, row_id, item_id)| {
            let item = ctx.db.item().id().find(item_id)?;
            if !item.slash || item.accuracy < 0.5 {
                return None;
            }
            let damage = match scope {
                CarriedInventoryScope::Personal => ctx
                    .db
                    .item_condition()
                    .inventory_item_id()
                    .find(row_id)
                    .map(|c| c.bins()),
                CarriedInventoryScope::Party => ctx
                    .db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .find(row_id)
                    .map(|c| {
                        DamageBins([c.tier_1, c.tier_2, c.tier_3, c.tier_4, c.tier_5]).normalized()
                    }),
            }
            .unwrap_or_default();
            (effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity) >= 0.5).then(
                || {
                    format!(
                        "{}|{row_id}|{}|{}|{}|{:?}",
                        scope.as_str(),
                        item.id,
                        item.accuracy.to_bits(),
                        item.edge_sensitivity.to_bits(),
                        damage.0.map(f32::to_bits)
                    )
                },
            )
        })
        .min()
}

fn grinding_tool_binding(ctx: &ReducerContext, character_id: u64) -> String {
    carried_item_rows(ctx, character_id)
        .into_iter()
        .filter(|(_, _, item_id)| item_id == "mortar_and_pestle")
        .map(|(scope, row_id, item_id)| format!("{}|{row_id}|{item_id}", scope.as_str()))
        .min()
        .unwrap_or_else(|| "hands".into())
}

fn preparation_terminal_minute(
    ctx: &ReducerContext,
    character_id: u64,
    current_minute: u64,
    duration: u64,
) -> Result<Option<u64>, String> {
    let injury = crate::surgery::preview_injury_boundary(
        ctx,
        character_id,
        duration,
        crate::surgery::InjuryRecoveryMinutes::new(duration),
    )?;
    let (disease_safe, disease_terminal) =
        crate::disease::preview_disease_terminal_boundary(ctx, character_id, injury.elapsed, true)?;
    let safe = injury.elapsed.min(disease_safe);
    Ok((safe < duration || injury.terminal || disease_terminal)
        .then_some(current_minute.saturating_add(safe)))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the idempotency key is defined by each explicit preparation coordinate"
)]
fn next_preparation_attempt_generation(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_scope: &str,
    inventory_item_id: u64,
    food_lot_id: u64,
    material_object_id: u64,
    expected_revision: u64,
    action: IngredientPreparationAction,
) -> Result<u64, String> {
    let key = preparation_attempt_state_key(
        character_id,
        inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        expected_revision,
        action,
    );
    match ctx
        .db
        .ingredient_preparation_attempt_state()
        .key()
        .find(&key)
    {
        Some(state) if state.completed => {
            Err("Ingredient preparation was already completed".into())
        }
        Some(state) => Ok(state.next_generation),
        None => Ok(0),
    }
}

fn preparation_attempt_state_key(
    character_id: u64,
    inventory_scope: &str,
    inventory_item_id: u64,
    food_lot_id: u64,
    material_object_id: u64,
    expected_revision: u64,
    action: IngredientPreparationAction,
) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(b"ingredient-preparation-attempt-state-v1");
    hash.update(character_id.to_le_bytes());
    hash.update((inventory_scope.len() as u64).to_le_bytes());
    hash.update(inventory_scope.as_bytes());
    hash.update(inventory_item_id.to_le_bytes());
    hash.update(food_lot_id.to_le_bytes());
    hash.update(material_object_id.to_le_bytes());
    hash.update(expected_revision.to_le_bytes());
    hash.update([match action {
        IngredientPreparationAction::Cut => 1,
        IngredientPreparationAction::Grind => 2,
    }]);
    encode_digest(&hash.finalize())
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn record_preparation_attempt_state(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_scope: &str,
    inventory_item_id: u64,
    food_lot_id: u64,
    material_object_id: u64,
    expected_revision: u64,
    action: IngredientPreparationAction,
    next_generation: u64,
    completed: bool,
) {
    let state = IngredientPreparationAttemptState {
        key: preparation_attempt_state_key(
            character_id,
            inventory_scope,
            inventory_item_id,
            food_lot_id,
            material_object_id,
            expected_revision,
            action,
        ),
        next_generation,
        completed,
    };
    if ctx
        .db
        .ingredient_preparation_attempt_state()
        .key()
        .find(&state.key)
        .is_some()
    {
        ctx.db
            .ingredient_preparation_attempt_state()
            .key()
            .update(state);
    } else {
        ctx.db.ingredient_preparation_attempt_state().insert(state);
    }
}

/// Physically prepares one exact personal or party measured lot. Physical preparation
/// does not change nutrition, flavor, contamination, or value.
#[reducer]
#[expect(
    clippy::too_many_arguments,
    reason = "the reducer ABI exposes each independently validated preparation input"
)]
pub fn prepare_ingredient_lot(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_scope: String,
    inventory_item_id: u64,
    food_lot_id: u64,
    material_object_id: u64,
    request_id: String,
    expected_revision: u64,
    attempt_generation: u64,
    action: IngredientPreparationAction,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    // Exact replay is resolved solely from the immutable submitted tuple and
    // durable receipt, before consulting any mutable live state.
    if let Some(receipt) = ctx
        .db
        .ingredient_preparation_receipt()
        .request_id()
        .find(&request_id)
    {
        return if receipt.actor_character_id == character_id
            && receipt.inventory_scope == inventory_scope
            && receipt.inventory_item_id == inventory_item_id
            && receipt.food_lot_id == food_lot_id
            && receipt.material_object_id == material_object_id
            && receipt.expected_revision == expected_revision
            && receipt.attempt_generation == attempt_generation
            && receipt.action == action
        {
            Ok(())
        } else {
            Err("Ingredient preparation request id collides with a different attempt".into())
        };
    }
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Ingredient preparation is unavailable during a tactical encounter".into());
    }
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    let current_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time not found")?
        .minutes;
    let expected_generation = next_preparation_attempt_generation(
        ctx,
        character_id,
        &inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        expected_revision,
        action,
    )?;
    if attempt_generation != expected_generation {
        return Err("Ingredient preparation attempt generation is stale".into());
    }
    let authority = load_preparation_authority(
        ctx,
        &actor,
        &inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        &request_id,
        expected_revision,
        action,
        current_minute,
    )?;
    let terminal_minute = preparation_terminal_minute(
        ctx,
        character_id,
        current_minute,
        u64::from(authority.duration),
    )?;
    let authority_digest = preparation_authority_digest(
        &actor,
        &authority,
        action,
        current_minute,
        terminal_minute,
        attempt_generation,
    );
    let canonical_request = preparation_request_id(
        character_id,
        &inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        expected_revision,
        action,
        attempt_generation,
        &authority.place.to_string(),
        &authority.custody_binding,
    );
    if request_id != canonical_request {
        return Err(
            "Ingredient preparation request does not match its authoritative inputs".into(),
        );
    }
    let planned = match build_preparation_planner(
        &actor,
        &authority,
        &request_id,
        action,
        current_minute,
        terminal_minute,
        attempt_generation,
    )? {
        adventuresim_core::strategic_action::PlanningOutcome::Ready(plan) => plan,
        adventuresim_core::strategic_action::PlanningOutcome::Rejected(_) => {
            return Err("Ingredient preparation is unavailable".into());
        }
    };
    let fresh = load_preparation_authority(
        ctx,
        &actor,
        &inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        &request_id,
        expected_revision,
        action,
        current_minute,
    )?;
    let fresh_terminal =
        preparation_terminal_minute(ctx, character_id, current_minute, u64::from(fresh.duration))?;
    let replanned = build_preparation_planner(
        &actor,
        &fresh,
        &request_id,
        action,
        current_minute,
        fresh_terminal,
        attempt_generation,
    )?;
    let fresh_snapshot = match &replanned {
        adventuresim_core::strategic_action::PlanningOutcome::Ready(plan) => plan.snapshot(),
        adventuresim_core::strategic_action::PlanningOutcome::Rejected(_) => {
            return Err("Ingredient preparation prerequisites changed before commit".into());
        }
    };
    let provenance = planned.provenance();
    adventuresim_core::strategic_action::validate_commit(
        &planned,
        &replanned,
        fresh_snapshot,
        &adventuresim_core::strategic_action::CommitAttempt {
            request_id: provenance.request_id.clone(),
            action_id: provenance.action_id.clone(),
            authority_binding: provenance.authority_binding,
        },
        None,
    )
    .map_err(|_| "Ingredient preparation authority changed before commit")?;
    adventuresim_core::material::validate_material_commit(
        &authority.material_receipt,
        std::slice::from_ref(&fresh.material_snapshot),
    )
    .map_err(|_| "Ingredient material changed before commit")?;

    let mut effect_duration = None;
    let mut effect_commit = None;
    for effect in planned.effects() {
        match effect {
            adventuresim_core::strategic_action::ActionEffect::Domain(
                herbalism::PreparationPlanEffect::AttemptWait {
                    actor,
                    requested_minutes,
                },
            ) => effect_duration = Some((actor.get(), *requested_minutes)),
            adventuresim_core::strategic_action::ActionEffect::Domain(
                herbalism::PreparationPlanEffect::CommitPreparation {
                    action,
                    expected_revision,
                    next_display_name,
                },
            ) => effect_commit = Some((*action, *expected_revision, next_display_name.clone())),
            _ => return Err("Ingredient preparation planner emitted an unsupported effect".into()),
        }
    }
    let (effect_actor, duration) =
        effect_duration.ok_or("Ingredient preparation planner omitted its wait effect")?;
    let core_action = match action {
        IngredientPreparationAction::Cut => herbalism::PreparationAction::Cut,
        IngredientPreparationAction::Grind => herbalism::PreparationAction::Grind,
    };
    if effect_actor != character_id || duration != u64::from(authority.duration) {
        return Err("Ingredient preparation planner effects do not match authority".into());
    }
    if let Some((effect_action, effect_revision, _)) = &effect_commit
        && (*effect_action != core_action || *effect_revision != expected_revision)
    {
        return Err("Ingredient preparation planner effects do not match authority".into());
    }

    let survived = crate::time::advance_character_wait_time(ctx, character_id, duration)?;
    if !survived && effect_commit.is_some() {
        return Err("Ingredient preparation wait diverged from its authoritative plan".into());
    }
    // A clipped or clock-exhausted interval is a durable terminal attempt.
    // The material remains untouched; this request exact-replays while the
    // gateway publishes a distinct next server-owned generation.
    if !survived || effect_commit.is_none() {
        let next_generation = attempt_generation
            .checked_add(1)
            .ok_or("Ingredient preparation attempt generation is exhausted")?;
        record_preparation_attempt_state(
            ctx,
            character_id,
            &inventory_scope,
            inventory_item_id,
            food_lot_id,
            material_object_id,
            expected_revision,
            action,
            next_generation,
            false,
        );
        ctx.db
            .ingredient_preparation_receipt()
            .insert(IngredientPreparationReceipt {
                request_id,
                actor_character_id: character_id,
                inventory_scope,
                inventory_item_id,
                food_lot_id,
                material_object_id,
                expected_revision,
                attempt_generation,
                action,
                canonical_place: authority.place.to_string(),
                custody_binding: authority.custody_binding,
                authority_input_digest: encode_digest(&authority_digest),
                duration_minutes: authority.duration,
                interrupted: true,
                resulting_revision: expected_revision,
                material_input_digest: encode_digest(
                    authority.material_receipt.input_digest().bytes(),
                ),
            });
        return Ok(());
    }
    let (_, _, next_display_name) = effect_commit.expect("completion effect checked");
    let post_actor = crate::character::require_living_character(ctx, character_id)?;
    if post_actor.in_server {
        return Err("Ingredient preparation became unavailable during its wait".into());
    }
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    let post_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time disappeared after preparation wait")?
        .minutes;
    let post = load_preparation_authority(
        ctx,
        &post_actor,
        &inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        &request_id,
        expected_revision,
        action,
        post_minute,
    )?;
    if post.place != authority.place
        || !preparation_lot_truth_unchanged(&authority.lot, &post.lot)
        || post.custody_binding != authority.custody_binding
        || post.material_source_digest != authority.material_source_digest
        || post.object.item_id != authority.object.item_id
        || post.object.location != authority.object.location
        || post.skill != authority.skill
        || post.tool_binding != authority.tool_binding
        || post.duration != authority.duration
        || post.next != authority.next
    {
        return Err("Ingredient preparation authority changed during its wait".into());
    }
    adventuresim_core::material::validate_material_commit(
        &post.material_receipt,
        std::slice::from_ref(&post.material_snapshot),
    )
    .map_err(|_| "Ingredient material changed during preparation")?;
    let committed_material_digest = encode_digest(post.material_receipt.input_digest().bytes());
    let mut lot = post.lot;
    lot.preparation = post.next;
    lot.display_name = next_display_name;
    lot.material_revision = lot
        .material_revision
        .checked_add(1)
        .ok_or("Ingredient material revision is exhausted")?;
    let resulting_revision = lot.material_revision;
    ctx.db.food_lot().id().update(lot);
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills disappeared before preparation training")?;
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes disappeared before preparation training")?;
    let hours = match authority.skill {
        Skill::Knife => &mut skills.knife_hours,
        Skill::Bludgeon => &mut skills.bludgeon_hours,
        _ => unreachable!(),
    };
    let gain = apply_direct_training(
        authority.skill,
        hours,
        authority.duration as f32 / 60.0,
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
    record_preparation_attempt_state(
        ctx,
        character_id,
        &inventory_scope,
        inventory_item_id,
        food_lot_id,
        material_object_id,
        expected_revision,
        action,
        attempt_generation,
        true,
    );
    ctx.db
        .ingredient_preparation_receipt()
        .insert(IngredientPreparationReceipt {
            request_id,
            actor_character_id: character_id,
            inventory_scope,
            inventory_item_id,
            food_lot_id,
            material_object_id,
            expected_revision,
            attempt_generation,
            action,
            canonical_place: authority.place.to_string(),
            custody_binding: authority.custody_binding,
            authority_input_digest: encode_digest(&authority_digest),
            duration_minutes: authority.duration,
            interrupted: false,
            resulting_revision,
            material_input_digest: committed_material_digest,
        });
    Ok(())
}

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

/// Public, inspectable description of one non-fungible inventory batch.
#[derive(Clone, Debug)]
#[table(accessor = food_lot, public)]
pub struct FoodLot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub inventory_item_id: Option<u64>,
    pub party_inventory_item_id: Option<u64>,
    #[index(btree)]
    pub material_revision: u64,
    pub display_name: String,
    pub preparation: FoodPreparation,
    pub ingredient_item_ids: Vec<String>,
    /// Fractional source-unit provenance is conserved when a lot is partly eaten.
    pub ingredient_quantities: Vec<f32>,
    pub salty_kg: f32,
    pub spicy_kg: f32,
    pub sweet_kg: f32,
    pub sour_kg: f32,
    pub savory_kg: f32,
    /// Durable quality tier shared with item craftsmanship name colors.
    pub quality: u8,
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

/// Private source-material provenance carried through cooking into consumption.
#[derive(Clone, Debug)]
#[table(accessor = food_contamination_provenance)]
pub struct FoodContaminationProvenance {
    #[primary_key]
    pub food_lot_id: u64,
    pub contribution_ids: Vec<String>,
    pub contribution_loads: Vec<f32>,
    pub contribution_digest: String,
}

#[expect(
    clippy::too_many_arguments,
    reason = "stable request identity frames every preparation coordinate explicitly"
)]
fn preparation_request_id(
    character_id: u64,
    inventory_scope: &str,
    inventory_item_id: u64,
    food_lot_id: u64,
    material_object_id: u64,
    revision: u64,
    action: IngredientPreparationAction,
    attempt_generation: u64,
    canonical_place: &str,
    direct_custody_binding: &str,
) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(b"ingredient-preparation-request-v2");
    hash.update(character_id.to_le_bytes());
    hash.update((inventory_scope.len() as u64).to_le_bytes());
    hash.update(inventory_scope.as_bytes());
    hash.update(inventory_item_id.to_le_bytes());
    hash.update(food_lot_id.to_le_bytes());
    hash.update(material_object_id.to_le_bytes());
    hash.update(revision.to_le_bytes());
    hash.update([match action {
        IngredientPreparationAction::Cut => 1,
        IngredientPreparationAction::Grind => 2,
    }]);
    hash.update(attempt_generation.to_le_bytes());
    hash.update((canonical_place.len() as u64).to_le_bytes());
    hash.update(canonical_place.as_bytes());
    hash.update((direct_custody_binding.len() as u64).to_le_bytes());
    hash.update(direct_custody_binding.as_bytes());
    encode_digest(&hash.finalize())
}

fn preparation_place(
    ctx: &ReducerContext,
    actor: &crate::Character,
) -> Result<adventuresim_core::strategic_place::StrategicPlaceId, String> {
    if let Some(settlement_id) = actor.current_settlement_id.as_deref() {
        return adventuresim_core::strategic_place::StrategicPlaceId::settlement(settlement_id)
            .map_err(|_| "Ingredient preparation settlement identity is malformed".into());
    }
    if let Some(site_id) = crate::investigation::character_case_site_id(ctx, actor.id) {
        return adventuresim_core::strategic_place::StrategicPlaceId::case_site(site_id)
            .map_err(|_| "Ingredient preparation case-site identity is malformed".into());
    }
    let party_id = actor
        .party_id
        .as_deref()
        .ok_or("Ingredient preparation requires a canonical strategic place")?;
    crate::strategic::current_journey_camp_place(ctx, party_id)
}

fn encode_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn preparation_material_source_digest(ctx: &ReducerContext, food_lot_id: u64) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(b"ingredient-material-source-v1");
    if let Some(row) = ctx.db.food_contamination().food_lot_id().find(food_lot_id) {
        hash.update(row.concentration_anchor.to_bits().to_le_bytes());
        hash.update(row.growth_per_hour.to_bits().to_le_bytes());
        hash.update(row.anchor_minute.to_le_bytes());
    }
    let mut components = ctx
        .db
        .medicinal_component()
        .iter()
        .filter(|row| row.carrier_kind == "food_lot" && row.carrier_id == food_lot_id)
        .map(|row| {
            format!(
                "{}\0{}\0{}",
                row.intervention_profile_id,
                row.profile_version,
                row.potency_units.to_bits()
            )
        })
        .collect::<Vec<_>>();
    components.sort();
    for component in components {
        hash.update((component.len() as u64).to_le_bytes());
        hash.update(component.as_bytes());
    }
    encode_digest(&hash.finalize())
}

fn preparation_material_current_digest(
    ctx: &ReducerContext,
    food_lot_id: u64,
    current_minute: u64,
) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(b"ingredient-material-current-v1");
    hash.update(preparation_material_source_digest(ctx, food_lot_id).as_bytes());
    if let Some(row) = ctx.db.food_contamination().food_lot_id().find(food_lot_id) {
        let current = food::contamination_at(
            row.concentration_anchor,
            row.growth_per_hour,
            current_minute.saturating_sub(row.anchor_minute),
        );
        hash.update(current.to_bits().to_le_bytes());
    }
    encode_digest(&hash.finalize())
}

#[derive(Clone)]
struct PreparationAuthority {
    inventory_scope: String,
    inventory_item_id: u64,
    lot: FoodLot,
    object: crate::InventoryObject,
    custody_binding: String,
    place: adventuresim_core::strategic_place::StrategicPlaceId,
    skill: Skill,
    next: FoodPreparation,
    prefix: &'static str,
    tool_binding: String,
    duration: u32,
    material_source_digest: String,
    material_current_digest: String,
    material_snapshot: adventuresim_core::material::PrivateMaterialSnapshot<
        herbalism::IngredientMaterialPreparation,
        herbalism::MedicinalMaterialComponent,
        herbalism::IngredientContaminant,
    >,
    material_receipt: adventuresim_core::material::MaterialTransformationReceipt<
        herbalism::IngredientMaterialPreparation,
        herbalism::MedicinalMaterialComponent,
        herbalism::IngredientContaminant,
        herbalism::PreparationConservationPolicy,
        herbalism::PreparationMaterialReceipt,
    >,
}

fn preparation_lot_truth_unchanged(before: &FoodLot, after: &FoodLot) -> bool {
    before.id == after.id
        && before.inventory_item_id == after.inventory_item_id
        && before.party_inventory_item_id == after.party_inventory_item_id
        && before.material_revision == after.material_revision
        && before.display_name == after.display_name
        && before.preparation == after.preparation
        && before.ingredient_item_ids == after.ingredient_item_ids
        && before
            .ingredient_quantities
            .iter()
            .map(|value| value.to_bits())
            .eq(after
                .ingredient_quantities
                .iter()
                .map(|value| value.to_bits()))
        && before.salty_kg.to_bits() == after.salty_kg.to_bits()
        && before.spicy_kg.to_bits() == after.spicy_kg.to_bits()
        && before.sweet_kg.to_bits() == after.sweet_kg.to_bits()
        && before.sour_kg.to_bits() == after.sour_kg.to_bits()
        && before.savory_kg.to_bits() == after.savory_kg.to_bits()
        && before.quality == after.quality
        && before.mass_kg.to_bits() == after.mass_kg.to_bits()
        && before.nutrition_kcal.to_bits() == after.nutrition_kcal.to_bits()
        && before.total_value.to_bits() == after.total_value.to_bits()
        && before.created_at_minute == after.created_at_minute
}

fn material_snapshot(
    ctx: &ReducerContext,
    lot: &FoodLot,
    object: &crate::InventoryObject,
    custody: adventuresim_core::physical_object::OperationalCustody,
    current_minute: u64,
) -> Result<
    adventuresim_core::material::PrivateMaterialSnapshot<
        herbalism::IngredientMaterialPreparation,
        herbalism::MedicinalMaterialComponent,
        herbalism::IngredientContaminant,
    >,
    String,
> {
    use adventuresim_core::material::{
        ContaminantLoad, ExtensiveComponent, MaterialComponentMicrounits, MaterialIdentity,
        MaterialLotId, MaterialMeasure, MaterialPreparation, Milligrams, PrivateMaterialSnapshot,
        PrivateMaterialTruth,
    };
    use std::num::NonZeroU64;

    let mass_milligrams = Milligrams::try_from_kilograms(f64::from(lot.mass_kg))
        .map_err(|error| format!("Ingredient material mass is invalid: {error:?}"))?
        .get();
    let measure = MaterialMeasure::try_new(mass_milligrams, 0)
        .map_err(|error| format!("Ingredient material measure is invalid: {error:?}"))?;
    let components = ctx
        .db
        .medicinal_component()
        .iter()
        .filter(|row| row.carrier_kind == "food_lot" && row.carrier_id == lot.id)
        .map(|row| {
            MaterialComponentMicrounits::try_from_units(row.potency_units)
                .map_err(|error| format!("Ingredient medicinal potency is invalid: {error:?}"))
                .map(|magnitude| {
                    magnitude.map(|magnitude| ExtensiveComponent {
                        component: herbalism::MedicinalMaterialComponent {
                            intervention_profile_id: row.intervention_profile_id,
                            profile_version: row.profile_version,
                        },
                        magnitude: magnitude.get(),
                    })
                })
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let contaminants = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(lot.id)
        .and_then(|row| {
            let current = food::contamination_at(
                row.concentration_anchor,
                row.growth_per_hour,
                current_minute.saturating_sub(row.anchor_minute),
            );
            let load = f64::from(current.max(0.0)) * mass_milligrams as f64;
            NonZeroU64::new(load.round() as u64).map(|load| ContaminantLoad {
                contaminant: herbalism::IngredientContaminant::Microbial,
                load,
            })
        })
        .into_iter()
        .collect();
    PrivateMaterialSnapshot::try_new(
        MaterialIdentity::try_new(
            MaterialLotId::try_new(lot.id).map_err(|error| format!("Invalid lot: {error:?}"))?,
            adventuresim_core::physical_object::PhysicalObjectId::try_new(object.id)
                .map_err(|error| error.to_string())?,
            custody,
        )
        .map_err(|error| error.to_string())?,
        measure,
        match lot.preparation {
            FoodPreparation::Raw => MaterialPreparation::Raw,
            FoodPreparation::Cut => MaterialPreparation::Cut,
            FoodPreparation::Ground => MaterialPreparation::Ground,
            _ => return Err("Ingredient preparation state is unsupported".into()),
        },
        PrivateMaterialTruth::try_new(components, contaminants)
            .map_err(|error| format!("Ingredient private material is invalid: {error:?}"))?,
        lot.material_revision,
    )
    .map_err(|error| format!("Ingredient material snapshot is invalid: {error:?}"))
}

fn preparation_material_receipt(
    snapshot: &adventuresim_core::material::PrivateMaterialSnapshot<
        herbalism::IngredientMaterialPreparation,
        herbalism::MedicinalMaterialComponent,
        herbalism::IngredientContaminant,
    >,
    request_id: &str,
    action: IngredientPreparationAction,
) -> Result<
    adventuresim_core::material::MaterialTransformationReceipt<
        herbalism::IngredientMaterialPreparation,
        herbalism::MedicinalMaterialComponent,
        herbalism::IngredientContaminant,
        herbalism::PreparationConservationPolicy,
        herbalism::PreparationMaterialReceipt,
    >,
    String,
> {
    use adventuresim_core::material::{
        MaterialActionProvenance, MaterialPreparation, MaterialProcessId, MaterialRequestId,
        MaterialTransformationReceipt, Portion, ProcessConservationPolicy, ProducedMaterial,
        RoundingTolerance, SourceLotContribution,
    };
    use sha2::Digest as _;

    let request_hash: [u8; 32] = sha2::Sha256::digest(request_id.as_bytes()).into();
    let provenance = MaterialActionProvenance {
        request_id: MaterialRequestId::try_new(request_hash)
            .map_err(|error| format!("Invalid material request: {error:?}"))?,
        process_id: MaterialProcessId::try_new(snapshot.identity().lot_id().get())
            .map_err(|error| format!("Invalid material process: {error:?}"))?,
    };
    let (source, remainder) = SourceLotContribution::from_snapshot(
        snapshot,
        Portion::try_new(1, 1).map_err(|error| format!("Invalid whole portion: {error:?}"))?,
    )
    .map_err(|error| format!("Invalid preparation source: {error:?}"))?;
    if remainder.is_some() {
        return Err("Whole-lot preparation unexpectedly produced a remainder".into());
    }
    let output = adventuresim_core::material::PrivateMaterialSnapshot::try_new(
        snapshot.identity().clone(),
        snapshot.measure(),
        match action {
            IngredientPreparationAction::Cut => MaterialPreparation::Cut,
            IngredientPreparationAction::Grind => MaterialPreparation::Ground,
        },
        snapshot.private_truth().clone(),
        snapshot
            .revision()
            .checked_add(1)
            .ok_or("Ingredient material revision is exhausted")?,
    )
    .map_err(|error| format!("Invalid preparation output: {error:?}"))?;
    MaterialTransformationReceipt::try_new(
        provenance,
        vec![source],
        vec![
            ProducedMaterial::try_new(output)
                .map_err(|error| format!("Invalid preparation product: {error:?}"))?,
        ],
        ProcessConservationPolicy::try_new(
            herbalism::PreparationConservationPolicy::Exact,
            adventuresim_core::material::MaterialMeasure::ZERO,
            RoundingTolerance::exact(),
            Vec::new(),
            Vec::new(),
        )
        .map_err(|error| format!("Invalid preparation conservation: {error:?}"))?,
        herbalism::PreparationMaterialReceipt {
            action: match action {
                IngredientPreparationAction::Cut => herbalism::PreparationAction::Cut,
                IngredientPreparationAction::Grind => herbalism::PreparationAction::Grind,
            },
        },
    )
    .map_err(|error| format!("Ingredient conservation failed: {error:?}"))
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn load_preparation_authority(
    ctx: &ReducerContext,
    actor: &crate::Character,
    inventory_scope: &str,
    inventory_item_id: u64,
    food_lot_id: u64,
    material_object_id: u64,
    request_id: &str,
    expected_revision: u64,
    action: IngredientPreparationAction,
    current_minute: u64,
) -> Result<PreparationAuthority, String> {
    let inventory_scope =
        CarriedInventoryScope::try_from(inventory_scope).map_err(|error| error.to_string())?;
    let lot = ctx
        .db
        .food_lot()
        .id()
        .find(food_lot_id)
        .ok_or("Ingredient lot not found")?;
    if lot.material_revision == 0 || lot.material_revision != expected_revision {
        return Err("Ingredient preparation revision is stale".into());
    }
    let linked_row = match inventory_scope {
        CarriedInventoryScope::Personal => lot.inventory_item_id == Some(inventory_item_id),
        CarriedInventoryScope::Party => lot.party_inventory_item_id == Some(inventory_item_id),
    };
    if !linked_row {
        return Err("Ingredient lot does not match the selected inventory row".into());
    }
    let object =
        crate::inventory_container::object_for_row(ctx, inventory_scope, inventory_item_id)?
            .ok_or("Ingredient lot has no stable material object")?;
    if object.id != material_object_id {
        return Err("Ingredient material object is stale or ambiguous".into());
    }
    let resolved = crate::object_custody::require_actor_carried_object(ctx, actor, &object)?;
    if crate::inventory_container::ancestry_reaches_fireplace(ctx, object.id) {
        return Err("Ingredient lot is not in carried preparation custody".into());
    }
    let (skill, physical, next, prefix, tool_binding) = match action {
        IngredientPreparationAction::Cut => {
            if lot.preparation != FoodPreparation::Raw {
                return Err("Only a raw ingredient can be cut".into());
            }
            let tool_binding = qualifying_cutting_weapon_binding(ctx, actor.id).ok_or(
                "Cutting requires a carried edged weapon with current precision of at least 0.5",
            )?;
            (
                Skill::Knife,
                herbalism::PhysicalPreparation::Cut,
                FoodPreparation::Cut,
                "Cut",
                tool_binding,
            )
        }
        IngredientPreparationAction::Grind => {
            if !matches!(lot.preparation, FoodPreparation::Raw | FoodPreparation::Cut) {
                return Err("Only a raw or cut ingredient can be ground".into());
            }
            (
                Skill::Bludgeon,
                herbalism::PhysicalPreparation::Ground,
                FoodPreparation::Ground,
                "Ground",
                grinding_tool_binding(ctx, actor.id),
            )
        }
    };
    let duration = herbalism::physical_preparation_minutes(
        physical,
        preparation_skill_check(ctx, actor.id, skill)?,
        tool_binding != "hands",
    );
    let place = preparation_place(ctx, actor)?;
    let custody_binding =
        crate::object_custody::canonical_custody_binding(resolved.object.custody());
    let snapshot = material_snapshot(
        ctx,
        &lot,
        &object,
        resolved.object.custody().clone(),
        current_minute,
    )?;
    let material_receipt = preparation_material_receipt(&snapshot, request_id, action)?;
    Ok(PreparationAuthority {
        inventory_scope: inventory_scope.as_str().into(),
        inventory_item_id,
        lot,
        object,
        custody_binding,
        place,
        skill,
        next,
        prefix,
        tool_binding,
        duration,
        material_source_digest: preparation_material_source_digest(ctx, food_lot_id),
        material_current_digest: preparation_material_current_digest(
            ctx,
            food_lot_id,
            current_minute,
        ),
        material_snapshot: snapshot,
        material_receipt,
    })
}

fn build_preparation_planner(
    actor: &crate::Character,
    authority: &PreparationAuthority,
    request_id: &str,
    action: IngredientPreparationAction,
    current_minute: u64,
    terminal_minute: Option<u64>,
    attempt_generation: u64,
) -> Result<herbalism::PreparationPlanningOutcome, String> {
    use adventuresim_core::{
        physical_object::{CustodyCharacterId, PhysicalObjectId},
        strategic_action::{
            ActionCoordinates, ActionDefinitionId, ActionRequestId, ActionTarget,
            AuthoritativeSnapshot, AuthorityBinding, PlanProvenance, RequestedDuration,
            SnapshotDigest, SnapshotRevision,
        },
    };
    let actor_id = CustodyCharacterId::try_new(actor.id).map_err(|error| error.to_string())?;
    let object_id =
        PhysicalObjectId::try_new(authority.object.id).map_err(|error| error.to_string())?;
    let coordinates = ActionCoordinates::try_new(
        actor_id,
        ActionTarget::Object(object_id),
        authority.place.clone(),
        None,
        Vec::new(),
    )
    .map_err(|_| "Ingredient preparation coordinates are inconsistent")?;
    let rights_question =
        herbalism::preparation_rights_question(actor_id, object_id, authority.place.clone())
            .map_err(|_| "Ingredient preparation rights question is inconsistent")?;
    let rights = herbalism::decide_preparation_rights(
        &rights_question,
        true,
        authority.lot.material_revision,
    );
    let digest = preparation_authority_digest(
        actor,
        authority,
        action,
        current_minute,
        terminal_minute,
        attempt_generation,
    );
    let prefix = authority.prefix;
    let base_name = authority
        .lot
        .display_name
        .trim_start_matches("Cut ")
        .trim_start_matches("Ground ");
    Ok(herbalism::build_preparation_plan(
        herbalism::PreparationPlanAuthority {
            coordinates,
            provenance: PlanProvenance {
                request_id: ActionRequestId::try_new(request_id)
                    .map_err(|_| "Ingredient preparation request is malformed")?,
                action_id: ActionDefinitionId::try_new(match action {
                    IngredientPreparationAction::Cut => "ingredient-preparation:cut",
                    IngredientPreparationAction::Grind => "ingredient-preparation:grind",
                })
                .map_err(|_| "Ingredient preparation definition is malformed")?,
                input_digest: SnapshotDigest(digest),
                authority_binding: AuthorityBinding(digest),
            },
            snapshot: AuthoritativeSnapshot {
                revision: SnapshotRevision(authority.lot.material_revision),
                digest: SnapshotDigest(digest),
            },
            current_minute,
            duration: RequestedDuration::try_new(u64::from(authority.duration))
                .map_err(|_| "Ingredient preparation duration must be positive")?,
            terminal_minute,
            rights,
            custody_matches: true,
            revision_current: true,
            transition_allowed: true,
            required_tool_available: true,
            action: match action {
                IngredientPreparationAction::Cut => herbalism::PreparationAction::Cut,
                IngredientPreparationAction::Grind => herbalism::PreparationAction::Grind,
            },
            expected_revision: authority.lot.material_revision,
            next_display_name: format!("{prefix} {base_name}"),
        },
    ))
}

fn preparation_authority_digest(
    actor: &crate::Character,
    authority: &PreparationAuthority,
    action: IngredientPreparationAction,
    current_minute: u64,
    terminal_minute: Option<u64>,
    attempt_generation: u64,
) -> [u8; 32] {
    preparation_authority_digest_parts(
        actor,
        &authority.inventory_scope,
        authority.inventory_item_id,
        &authority.lot,
        &authority.object,
        &authority.custody_binding,
        &authority.place,
        authority.skill,
        authority.next,
        authority.prefix,
        &authority.tool_binding,
        authority.duration,
        &authority.material_source_digest,
        &authority.material_current_digest,
        action,
        current_minute,
        terminal_minute,
        attempt_generation,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn preparation_authority_digest_parts(
    actor: &crate::Character,
    inventory_scope: &str,
    inventory_item_id: u64,
    lot: &FoodLot,
    object: &crate::InventoryObject,
    custody_binding: &str,
    place: &adventuresim_core::strategic_place::StrategicPlaceId,
    skill: Skill,
    next: FoodPreparation,
    prefix: &str,
    tool_binding: &str,
    duration: u32,
    material_source_digest: &str,
    material_current_digest: &str,
    action: IngredientPreparationAction,
    current_minute: u64,
    terminal_minute: Option<u64>,
    attempt_generation: u64,
) -> [u8; 32] {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    let mut frame = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    frame(b"ingredient-preparation-plan-v2");
    frame(inventory_scope.as_bytes());
    frame(&inventory_item_id.to_le_bytes());
    frame(place.to_string().as_bytes());
    frame(custody_binding.as_bytes());
    frame(&actor.id.to_le_bytes());
    frame(&lot.id.to_le_bytes());
    frame(&object.id.to_le_bytes());
    frame(&lot.material_revision.to_le_bytes());
    frame(lot.display_name.as_bytes());
    frame(&[lot.preparation as u8]);
    frame(&[lot.quality]);
    frame(&lot.created_at_minute.to_le_bytes());
    frame(&duration.to_le_bytes());
    frame(&current_minute.to_le_bytes());
    frame(&terminal_minute.unwrap_or(u64::MAX).to_le_bytes());
    frame(&attempt_generation.to_le_bytes());
    frame(&[match skill {
        Skill::Knife => 1,
        Skill::Bludgeon => 2,
        _ => 0,
    }]);
    frame(&[next as u8]);
    frame(prefix.as_bytes());
    frame(tool_binding.as_bytes());
    frame(object.item_id.as_bytes());
    let location = serde_json::to_vec(&object.location)
        .expect("inventory location serialization is infallible");
    frame(&location);
    frame(&lot.mass_kg.to_bits().to_le_bytes());
    frame(&lot.nutrition_kcal.to_bits().to_le_bytes());
    frame(&lot.total_value.to_bits().to_le_bytes());
    for value in [
        lot.salty_kg,
        lot.spicy_kg,
        lot.sweet_kg,
        lot.sour_kg,
        lot.savory_kg,
    ] {
        frame(&value.to_bits().to_le_bytes());
    }
    for item_id in &lot.ingredient_item_ids {
        frame(item_id.as_bytes());
    }
    for quantity in &lot.ingredient_quantities {
        frame(&quantity.to_bits().to_le_bytes());
    }
    frame(material_source_digest.as_bytes());
    frame(material_current_digest.as_bytes());
    frame(&[match action {
        IngredientPreparationAction::Cut => 1,
        IngredientPreparationAction::Grind => 2,
    }]);
    hash.finalize().into()
}

#[derive(Clone, Debug)]
#[table(accessor = ingredient_preparation_receipt)]
pub struct IngredientPreparationReceipt {
    #[primary_key]
    pub request_id: String,
    pub actor_character_id: u64,
    pub inventory_scope: String,
    pub inventory_item_id: u64,
    pub food_lot_id: u64,
    pub material_object_id: u64,
    pub expected_revision: u64,
    pub attempt_generation: u64,
    pub action: IngredientPreparationAction,
    pub canonical_place: String,
    pub custody_binding: String,
    pub authority_input_digest: String,
    pub duration_minutes: u32,
    pub interrupted: bool,
    pub resulting_revision: u64,
    pub material_input_digest: String,
}

/// Minimal server-owned liveness cursor. It lets the gateway issue a fresh
/// request after a clipped terminal attempt without exposing private receipts.
#[derive(Clone, Debug)]
#[table(accessor = ingredient_preparation_attempt_state)]
pub struct IngredientPreparationAttemptState {
    #[primary_key]
    pub key: String,
    pub next_generation: u64,
    pub completed: bool,
}

/// Gateway-only public projection of the exact reducer tuple and preview. The
/// reducer still rebuilds and revalidates the private strategic/material plan
/// in its transaction; this view prevents the browser from inventing object
/// identity, revision, request identity, or duration.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendIngredientPreparationPlan {
    pub actor_character_id: u64,
    pub inventory_scope: String,
    pub inventory_item_id: u64,
    pub food_lot_id: u64,
    pub material_object_id: u64,
    pub request_id: String,
    pub expected_revision: u64,
    pub attempt_generation: u64,
    pub action: IngredientPreparationAction,
    pub duration_minutes: u32,
    pub next_display_name: String,
}

fn view_object_for_row(
    ctx: &ViewContext,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> Option<crate::InventoryObject> {
    let mut matches = ctx
        .db
        .inventory_object()
        .item_id()
        .filter(""..)
        .filter(|object| match (&object.location, scope) {
            (InventoryLocation::Personal(location), CarriedInventoryScope::Personal) => {
                location.row_id == row_id
            }
            (InventoryLocation::Party(location), CarriedInventoryScope::Party) => {
                location.row_id == row_id
            }
            _ => false,
        });
    let object = matches.next()?;
    matches.next().is_none().then_some(object)
}

fn view_ancestry_reaches_fireplace(ctx: &ViewContext, object_id: u64) -> bool {
    let mut cursor = Some(object_id);
    for _ in 0..=adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        let Some(id) = cursor else { return false };
        let Some(object) = ctx.db.inventory_object().id().find(id) else {
            return true;
        };
        if object.location.is_fireplace() {
            return true;
        }
        cursor = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(id)
            .map(|edge| edge.parent_object_id);
    }
    true
}

fn view_carried_item_rows(
    ctx: &ViewContext,
    actor: &crate::Character,
) -> Vec<(CarriedInventoryScope, u64, String)> {
    let mut rows = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(actor.id)
        .filter(|row| {
            view_object_for_row(ctx, CarriedInventoryScope::Personal, row.id).is_some_and(
                |object| {
                    !view_ancestry_reaches_fireplace(ctx, object.id)
                        && view_carried_custody_is_fully_resolved(
                            ctx,
                            actor,
                            CarriedInventoryScope::Personal,
                            &object,
                        )
                },
            )
        })
        .map(|row| (CarriedInventoryScope::Personal, row.id, row.item_id))
        .collect::<Vec<_>>();
    if let Some(party_id) = actor.party_id.as_deref() {
        rows.extend(
            ctx.db
                .party_inventory_item()
                .party_id()
                .filter(party_id)
                .filter(|row| {
                    view_object_for_row(ctx, CarriedInventoryScope::Party, row.id).is_some_and(
                        |object| {
                            !view_ancestry_reaches_fireplace(ctx, object.id)
                                && view_carried_custody_is_fully_resolved(
                                    ctx,
                                    actor,
                                    CarriedInventoryScope::Party,
                                    &object,
                                )
                        },
                    )
                })
                .map(|row| (CarriedInventoryScope::Party, row.id, row.item_id)),
        );
    }
    rows
}

fn view_cutting_weapon_binding(ctx: &ViewContext, actor: &crate::Character) -> Option<String> {
    view_carried_item_rows(ctx, actor)
        .into_iter()
        .filter_map(|(scope, row_id, item_id)| {
            let item = ctx.db.item().id().find(item_id)?;
            if !item.slash || item.accuracy < 0.5 {
                return None;
            }
            let damage = if scope == CarriedInventoryScope::Personal {
                ctx.db
                    .item_condition()
                    .inventory_item_id()
                    .find(row_id)
                    .map(|condition| condition.bins())
            } else {
                ctx.db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .find(row_id)
                    .map(|condition| {
                        DamageBins([
                            condition.tier_1,
                            condition.tier_2,
                            condition.tier_3,
                            condition.tier_4,
                            condition.tier_5,
                        ])
                        .normalized()
                    })
            }
            .unwrap_or_default();
            (effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity) >= 0.5).then(
                || {
                    format!(
                        "{}|{row_id}|{}|{}|{}|{:?}",
                        scope.as_str(),
                        item.id,
                        item.accuracy.to_bits(),
                        item.edge_sensitivity.to_bits(),
                        damage.0.map(f32::to_bits)
                    )
                },
            )
        })
        .min()
}

fn view_preparation_skill_check(ctx: &ViewContext, character_id: u64, skill: Skill) -> Option<f32> {
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)?;
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)?;
    Some(skill.capped_training_rank(skills.effective_skill_hours(skill), &attributes))
}

fn view_carried_custody_is_fully_resolved(
    ctx: &ViewContext,
    actor: &crate::Character,
    scope: CarriedInventoryScope,
    object: &crate::InventoryObject,
) -> bool {
    let mut cursor = object.clone();
    for _ in 0..=adventuresim_core::inventory_containers::MAX_CONTAINER_DEPTH {
        let row_id = match (&cursor.location, scope) {
            (InventoryLocation::Personal(location), CarriedInventoryScope::Personal)
                if location.character_id == actor.id =>
            {
                location.row_id
            }
            (InventoryLocation::Party(location), CarriedInventoryScope::Party)
                if actor.party_id.as_deref() == Some(location.party_id.as_str()) =>
            {
                location.row_id
            }
            _ => return false,
        };
        if !view_object_for_row(ctx, scope, row_id).is_some_and(|unique| unique.id == cursor.id) {
            return false;
        }
        let row_matches = match scope {
            CarriedInventoryScope::Personal => ctx
                .db
                .inventory_item()
                .id()
                .find(row_id)
                .is_some_and(|row| {
                    row.character_id == actor.id
                        && row.item_id == cursor.item_id
                        && row.quantity == 1
                }),
            CarriedInventoryScope::Party => ctx
                .db
                .party_inventory_item()
                .id()
                .find(row_id)
                .is_some_and(|row| {
                    actor.party_id.as_deref() == Some(row.party_id.as_str())
                        && row.item_id == cursor.item_id
                        && row.quantity == 1
                }),
        };
        if !row_matches {
            return false;
        }
        let parent = ctx
            .db
            .inventory_containment()
            .child_object_id()
            .find(cursor.id)
            .map(|edge| edge.parent_object_id);
        let Some(parent_id) = parent else { return true };
        let Some(parent) = ctx.db.inventory_object().id().find(parent_id) else {
            return false;
        };
        cursor = parent;
    }
    false
}

fn view_direct_custody(
    ctx: &ViewContext,
    actor: &crate::Character,
    scope: CarriedInventoryScope,
    object: &crate::InventoryObject,
) -> Option<OperationalCustody> {
    if let Some(edge) = ctx
        .db
        .inventory_containment()
        .child_object_id()
        .find(object.id)
    {
        return PhysicalObjectId::try_new(edge.parent_object_id)
            .ok()
            .map(OperationalCustody::Container);
    }
    match scope {
        CarriedInventoryScope::Personal => OperationalCustody::character(actor.id).ok(),
        CarriedInventoryScope::Party => OperationalCustody::party(actor.party_id.clone()?).ok(),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the projection mirrors the authoritative preparation identity coordinates"
)]
fn view_next_preparation_generation(
    ctx: &ViewContext,
    actor_id: u64,
    scope: &str,
    row_id: u64,
    lot_id: u64,
    object_id: u64,
    revision: u64,
    action: IngredientPreparationAction,
) -> Option<u64> {
    let key =
        preparation_attempt_state_key(actor_id, scope, row_id, lot_id, object_id, revision, action);
    match ctx
        .db
        .ingredient_preparation_attempt_state()
        .key()
        .find(&key)
    {
        Some(state) if state.completed => None,
        Some(state) => Some(state.next_generation),
        None => Some(0),
    }
}

#[view(accessor = backend_ingredient_preparation_plans, public)]
pub fn backend_ingredient_preparation_plans(
    ctx: &ViewContext,
) -> Vec<BackendIngredientPreparationPlan> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let actors = ctx
        .db
        .character()
        .scan_id()
        .filter(0u64..)
        .filter(|actor| {
            actor.alive
                && !actor.in_server
                && actor.current_settlement_id.is_some()
                && !actor.party_id.as_deref().is_some_and(|party_id| {
                    ctx.db
                        .strategic_encounter()
                        .party_id()
                        .find(party_id.to_string())
                        .is_some_and(|encounter| {
                            encounter.status == StrategicEncounterStatus::AwaitingChoice
                        })
                        || ctx
                            .db
                            .party_authority()
                            .id()
                            .find(party_id.to_string())
                            .is_some_and(|party| {
                                ctx.db
                                    .road_challenge_authority()
                                    .party_id()
                                    .filter(&party_id.to_string())
                                    .any(|challenge| {
                                        challenge.open
                                            && crate::strategic::party_at_bound_road_challenge_view(
                                                ctx, &party, &challenge,
                                            )
                                    })
                            })
                })
        })
        .collect::<Vec<_>>();
    let mut plans = Vec::new();
    for actor in actors {
        let Some(_) = ctx
            .db
            .character_time()
            .character_id()
            .find(actor.id)
            .map(|time| time.minutes)
        else {
            continue;
        };
        let Some(place) = actor.current_settlement_id.as_deref().and_then(|id| {
            adventuresim_core::strategic_place::StrategicPlaceId::settlement(id).ok()
        }) else {
            continue;
        };
        let carried = view_carried_item_rows(ctx, &actor);
        let cutting_weapon = view_cutting_weapon_binding(ctx, &actor);
        let grinding_tool = carried
            .iter()
            .filter(|(_, _, item_id)| item_id == "mortar_and_pestle")
            .map(|(scope, row_id, item_id)| format!("{}|{row_id}|{item_id}", scope.as_str()))
            .min()
            .unwrap_or_else(|| "hands".into());
        for lot in ctx
            .db
            .food_lot()
            .material_revision()
            .filter(1u64..)
            .filter(|lot| lot.material_revision > 0)
        {
            let row = if let Some(row_id) = lot.inventory_item_id
                && ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(row_id)
                    .is_some_and(|row| row.character_id == actor.id && row.quantity == 1)
            {
                Some((CarriedInventoryScope::Personal, row_id))
            } else if let Some(row_id) = lot.party_inventory_item_id
                && actor.party_id.as_deref().is_some_and(|party_id| {
                    ctx.db
                        .party_inventory_item()
                        .id()
                        .find(row_id)
                        .is_some_and(|row| row.party_id == party_id && row.quantity == 1)
                })
            {
                Some((CarriedInventoryScope::Party, row_id))
            } else {
                None
            };
            let Some((scope, row_id)) = row else { continue };
            let Some(object) = view_object_for_row(ctx, scope, row_id) else {
                continue;
            };
            if view_ancestry_reaches_fireplace(ctx, object.id)
                || !view_carried_custody_is_fully_resolved(ctx, &actor, scope, &object)
            {
                continue;
            }
            let Some(direct_custody) = view_direct_custody(ctx, &actor, scope, &object) else {
                continue;
            };
            let custody_binding = crate::object_custody::canonical_custody_binding(&direct_custody);
            let actions = match lot.preparation {
                FoodPreparation::Raw => [
                    cutting_weapon.clone().map(|tool_binding| {
                        (
                            IngredientPreparationAction::Cut,
                            Skill::Knife,
                            herbalism::PhysicalPreparation::Cut,
                            "Cut",
                            tool_binding,
                        )
                    }),
                    Some((
                        IngredientPreparationAction::Grind,
                        Skill::Bludgeon,
                        herbalism::PhysicalPreparation::Ground,
                        "Ground",
                        grinding_tool.clone(),
                    )),
                ],
                FoodPreparation::Cut => [
                    None,
                    Some((
                        IngredientPreparationAction::Grind,
                        Skill::Bludgeon,
                        herbalism::PhysicalPreparation::Ground,
                        "Ground",
                        grinding_tool.clone(),
                    )),
                ],
                _ => [None, None],
            };
            for (action, skill, physical, prefix, tool_binding) in actions.into_iter().flatten() {
                let Some(check) = view_preparation_skill_check(ctx, actor.id, skill) else {
                    continue;
                };
                let base_name = lot
                    .display_name
                    .trim_start_matches("Cut ")
                    .trim_start_matches("Ground ");
                let Some(attempt_generation) = view_next_preparation_generation(
                    ctx,
                    actor.id,
                    scope.as_str(),
                    row_id,
                    lot.id,
                    object.id,
                    lot.material_revision,
                    action,
                ) else {
                    continue;
                };
                let duration = herbalism::physical_preparation_minutes(
                    physical,
                    check,
                    tool_binding != "hands",
                );
                plans.push(BackendIngredientPreparationPlan {
                    actor_character_id: actor.id,
                    inventory_scope: scope.as_str().into(),
                    inventory_item_id: row_id,
                    food_lot_id: lot.id,
                    material_object_id: object.id,
                    request_id: preparation_request_id(
                        actor.id,
                        scope.as_str(),
                        row_id,
                        lot.id,
                        object.id,
                        lot.material_revision,
                        action,
                        attempt_generation,
                        &place.to_string(),
                        &custody_binding,
                    ),
                    expected_revision: lot.material_revision,
                    attempt_generation,
                    action,
                    duration_minutes: duration,
                    next_display_name: format!("{prefix} {base_name}"),
                });
            }
        }
    }
    plans
}

/// Private character-owned state for one exact physical fireplace context.
/// The portrait is environmental/shared, but neither its tool nor dish leaks
/// across player timelines.
#[derive(Clone, Debug)]
#[table(accessor = fireplace_station)]
pub struct FireplaceStation {
    #[primary_key]
    pub key: String,
    #[index(btree)]
    pub character_id: u64,
    /// Canonical `StrategicFixtureId::Fireplace` encoding.
    pub fireplace_fixture_id: String,
    pub instrument_item_id: Option<String>,
    /// Stable root object for a placed cooking vessel. `None` is the loose
    /// spit-roast lane or an empty station.
    pub instrument_object_id: Option<u64>,
    /// Exact immutable carried custody to which removal returns the tool.
    pub instrument_return_custody: Option<crate::PersistedOperationalCustody>,
}

fn dish_inventory_destination(
    source: &crate::PersistedOperationalCustody,
    dish_character_id: u64,
) -> Result<OperationalCustody, String> {
    crate::object_custody::carried_destination(source, dish_character_id)
}

#[derive(Clone, Debug)]
#[table(accessor = fireplace_dish)]
pub struct FireplaceDish {
    #[primary_key]
    pub station_key: String,
    #[index(btree)]
    pub character_id: u64,
    /// Canonical `StrategicFixtureId::Fireplace` encoding shared with its station.
    pub fireplace_fixture_id: String,
    /// Immutable operational return custody captured before ingredients are consumed.
    pub return_custody: crate::PersistedOperationalCustody,
    pub contributor_name: String,
    pub method: CookingMethod,
    pub cooking_check: f32,
    pub started_at_minute: u64,
    pub target_minutes: u32,
    pub display_name: String,
    pub ingredient_item_ids: Vec<String>,
    pub ingredient_quantities: Vec<f32>,
    pub salty_kg: f32,
    pub spicy_kg: f32,
    pub sweet_kg: f32,
    pub sour_kg: f32,
    pub savory_kg: f32,
    pub ready_quality: u8,
    pub mass_kg: f32,
    pub raw_nutrition_kcal: f32,
    pub ready_nutrition_retention: f32,
    pub ingredient_value: f32,
    pub raw_contamination: f32,
    pub raw_growth_per_hour: f32,
    pub cooked_growth_per_hour: f32,
    pub contamination_contribution_ids: Vec<String>,
    pub contamination_contribution_loads: Vec<f32>,
    pub contamination_contribution_digest: String,
    pub medicinal_profile_ids: Vec<String>,
    pub medicinal_profile_versions: Vec<u16>,
    pub medicinal_potency_units: Vec<f32>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFireplaceStation {
    pub key: String,
    pub character_id: u64,
    pub fireplace_fixture_id: String,
    pub instrument_item_id: Option<String>,
    pub instrument_object_id: Option<u64>,
}

#[view(accessor = backend_fireplace_stations, public)]
pub fn backend_fireplace_stations(ctx: &ViewContext) -> Vec<BackendFireplaceStation> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .fireplace_station()
        .character_id()
        .filter(0u64..)
        .map(|row| BackendFireplaceStation {
            key: row.key,
            character_id: row.character_id,
            fireplace_fixture_id: row.fireplace_fixture_id,
            instrument_item_id: row.instrument_item_id,
            instrument_object_id: row.instrument_object_id,
        })
        .collect()
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFireplaceDish {
    pub station_key: String,
    pub character_id: u64,
    pub fireplace_fixture_id: String,
    pub contributor_name: String,
    pub method: CookingMethod,
    pub started_at_minute: u64,
    pub target_minutes: u32,
    pub display_name: String,
}

#[view(accessor = backend_fireplace_dishes, public)]
pub fn backend_fireplace_dishes(ctx: &ViewContext) -> Vec<BackendFireplaceDish> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .fireplace_dish()
        .character_id()
        .filter(0u64..)
        .map(|row| BackendFireplaceDish {
            station_key: row.station_key,
            character_id: row.character_id,
            fireplace_fixture_id: row.fireplace_fixture_id,
            contributor_name: row.contributor_name,
            method: row.method,
            started_at_minute: row.started_at_minute,
            target_minutes: row.target_minutes,
            display_name: row.display_name,
        })
        .collect()
}

fn station_key(character_id: u64, fireplace_fixture_id: &str) -> String {
    format!("{character_id}|{fireplace_fixture_id}")
}

fn parse_persisted_fireplace_fixture(
    fireplace_fixture_id: &str,
) -> Result<StrategicFixtureId, String> {
    match fireplace_fixture_id
        .parse::<StrategicFixtureId>()
        .map_err(|_| "Persisted fireplace custody has an invalid canonical fixture")?
    {
        fixture @ StrategicFixtureId::Fireplace { .. } => Ok(fixture),
        _ => Err("Persisted fireplace custody names a non-fireplace fixture".into()),
    }
}

fn validate_persisted_station_fixture(
    ctx: &ReducerContext,
    station: &FireplaceStation,
) -> Result<StrategicFixtureId, String> {
    let fixture = parse_persisted_fireplace_fixture(&station.fireplace_fixture_id)?;
    let expected_key = match station.instrument_object_id {
        Some(object_id) => vessel_station_key(
            station.character_id,
            &station.fireplace_fixture_id,
            object_id,
        ),
        None => station_key(station.character_id, &station.fireplace_fixture_id),
    };
    if station.key != expected_key {
        return Err("Persisted fireplace station conflicts with its canonical fixture".into());
    }
    if station.instrument_item_id.is_some() != station.instrument_return_custody.is_some() {
        return Err("Persisted fireplace station has ambiguous return custody".into());
    }
    if let Some(custody) = station.instrument_return_custody.as_ref() {
        crate::object_custody::carried_destination(custody, station.character_id)?;
    }
    if let Some(object_id) = station.instrument_object_id {
        let object = ctx
            .db
            .inventory_object()
            .id()
            .find(object_id)
            .ok_or("Persisted fireplace station object is missing")?;
        if station.instrument_item_id.as_deref() != Some(object.item_id.as_str()) {
            return Err("Persisted fireplace station conflicts with its object identity".into());
        }
        crate::object_custody::require_object_at_fixture(ctx, &object, &fixture)?;
    }
    Ok(fixture)
}

fn validate_persisted_dish_fixture(
    ctx: &ReducerContext,
    dish: &FireplaceDish,
) -> Result<StrategicFixtureId, String> {
    let fixture = parse_persisted_fireplace_fixture(&dish.fireplace_fixture_id)?;
    let station = ctx
        .db
        .fireplace_station()
        .key()
        .find(dish.station_key.clone())
        .ok_or("Persisted fireplace dish has no station authority")?;
    let station_fixture = validate_persisted_station_fixture(ctx, &station)?;
    if station.character_id != dish.character_id || station_fixture != fixture {
        return Err("Persisted fireplace dish conflicts with its station authority".into());
    }
    crate::object_custody::carried_destination(&dish.return_custody, dish.character_id)?;
    Ok(fixture)
}

pub(crate) fn require_clear_current_camp_fireplace(
    ctx: &ReducerContext,
    camp_place: &StrategicPlaceId,
) -> Result<(), String> {
    if !matches!(camp_place, StrategicPlaceId::JourneyCamp { .. }) {
        return Err("Camp custody gate requires an exact journey camp".into());
    }
    let mut occupied = false;
    for station in ctx.db.fireplace_station().iter() {
        let fixture = validate_persisted_station_fixture(ctx, &station)?;
        occupied |= fixture.place() == camp_place && station.instrument_item_id.is_some();
    }
    for dish in ctx.db.fireplace_dish().iter() {
        let fixture = validate_persisted_dish_fixture(ctx, &dish)?;
        occupied |= fixture.place() == camp_place;
    }
    if occupied {
        Err("Retrieve every dish and remove every cooking instrument before breaking camp".into())
    } else {
        Ok(())
    }
}

pub(crate) fn require_members_clear_current_camp_fireplace(
    ctx: &ReducerContext,
    party_id: &str,
    character_ids: &[u64],
) -> Result<(), String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(party_id.to_string())
        .ok_or("Party not found")?;
    if party.camp_destination.is_none() {
        return Ok(());
    }
    let journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(party_id.to_string())
        .ok_or("Journey camp not found")?;
    if !crate::strategic::party_journey_is_current_camp(&party, &journey) {
        return Err("Party has incoherent current journey camp authority".into());
    }
    let place = crate::strategic::current_journey_camp_place(ctx, party_id)?;
    let member_ids = character_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut occupied = false;
    for station in ctx.db.fireplace_station().iter() {
        let fixture = validate_persisted_station_fixture(ctx, &station)?;
        occupied |= member_ids.contains(&station.character_id)
            && fixture.place() == &place
            && station.instrument_item_id.is_some();
    }
    for dish in ctx.db.fireplace_dish().iter() {
        let fixture = validate_persisted_dish_fixture(ctx, &dish)?;
        occupied |= member_ids.contains(&dish.character_id) && fixture.place() == &place;
    }
    if occupied {
        Err("Retrieve this member's dish and remove their cooking instrument before they leave the camp party".into())
    } else {
        Ok(())
    }
}

/// Resolves only the dead character's private station rows. Unretrieved food is
/// abandoned. Tools return to their exact recorded source when it still exists;
/// otherwise they move to the dead character's personal estate inventory. If
/// even that character row is absent, the tool is abandoned with the station.
/// A stale party reference can therefore never lock travel or leak another
/// player's dish.
pub(crate) fn cleanup_fireplace_custody_for_death(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    enum StationCleanup {
        Delete {
            station_key: String,
        },
        Abandon,
        Return {
            station_key: String,
            item_id: String,
            object_id: Option<u64>,
            destination: OperationalCustody,
        },
    }

    let personal_estate_exists = ctx.db.character().id().find(character_id).is_some();
    let stations = ctx
        .db
        .fireplace_station()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>();
    let mut cleanup = Vec::with_capacity(stations.len());

    // Resolve and validate every return before deleting a dish, changing an
    // object row, or removing a station.
    for station in &stations {
        validate_persisted_station_fixture(ctx, station)?;
        let Some(item_id) = station.instrument_item_id.as_deref() else {
            cleanup.push(StationCleanup::Delete {
                station_key: station.key.clone(),
            });
            continue;
        };
        let recorded_destination = crate::object_custody::carried_destination(
            station
                .instrument_return_custody
                .as_ref()
                .ok_or("Fireplace instrument return custody is missing")?,
            character_id,
        )?;
        let exact_party = match &recorded_destination {
            OperationalCustody::Party(party_id) => Some(party_id),
            _ => None,
        }
        .filter(|party_id| {
            ctx.db
                .party_authority()
                .id()
                .find(party_id.as_str().to_owned())
                .is_some()
        });
        let destination = if let Some(party_id) = exact_party {
            Some(OperationalCustody::party(party_id.as_str()).map_err(|error| error.to_string())?)
        } else if personal_estate_exists {
            Some(OperationalCustody::character(character_id).map_err(|error| error.to_string())?)
        } else {
            None
        };
        let Some(destination) = destination else {
            cleanup.push(StationCleanup::Abandon);
            continue;
        };
        if let Some(object_id) = station.instrument_object_id {
            let object = ctx
                .db
                .inventory_object()
                .id()
                .find(object_id)
                .ok_or("Fireplace instrument object is missing")?;
            if object.item_id != item_id {
                return Err("Fireplace instrument conflicts with its physical object".into());
            }
            crate::inventory_container::prevalidate_rehome_subtree(ctx, object_id, &destination)?;
        }
        cleanup.push(StationCleanup::Return {
            station_key: station.key.clone(),
            item_id: item_id.into(),
            object_id: station.instrument_object_id,
            destination,
        });
    }

    for dish in ctx
        .db
        .fireplace_dish()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .fireplace_dish()
            .station_key()
            .delete(dish.station_key);
    }
    for plan in cleanup {
        let StationCleanup::Return {
            station_key,
            item_id,
            object_id,
            destination,
        } = plan
        else {
            if let StationCleanup::Delete { station_key } = plan {
                ctx.db.fireplace_station().key().delete(station_key);
            }
            // Abandoned tools remain installed at their station.
            continue;
        };
        if let Some(object_id) = object_id {
            let row_id = match &destination {
                OperationalCustody::Party(party_id) => {
                    ctx.db
                        .party_inventory_item()
                        .insert(PartyInventoryItem {
                            id: 0,
                            party_id: party_id.as_str().into(),
                            item_id: item_id.clone(),
                            quantity: 1,
                        })
                        .id
                }
                OperationalCustody::Character(character) => {
                    ctx.db
                        .inventory_item()
                        .insert(crate::InventoryItem {
                            id: 0,
                            character_id: character.get(),
                            item_id: item_id.clone(),
                            quantity: 1,
                        })
                        .id
                }
                _ => return Err("Fireplace return destination is not carried inventory".into()),
            };
            let mut object = ctx
                .db
                .inventory_object()
                .id()
                .find(object_id)
                .ok_or("Fireplace instrument object is missing")?;
            object.location =
                crate::inventory_container::carried_location_for_row(&destination, row_id)?;
            ctx.db.inventory_object().id().update(object);
            crate::inventory_container::rehome_subtree(ctx, object_id, &destination)?;
        } else {
            match destination {
                OperationalCustody::Party(party_id) => {
                    crate::strategic::add_to_party_inventory_checked(
                        ctx,
                        party_id.as_str(),
                        &item_id,
                        1,
                    )?;
                }
                OperationalCustody::Character(character) => {
                    ctx.db.inventory_item().insert(crate::InventoryItem {
                        id: 0,
                        character_id: character.get(),
                        item_id,
                        quantity: 1,
                    });
                }
                _ => return Err("Fireplace return destination is not carried inventory".into()),
            }
        }
        ctx.db.fireplace_station().key().delete(station_key);
    }
    Ok(())
}

fn validate_fireplace_fixture(
    ctx: &ReducerContext,
    actor: &crate::Character,
    fireplace_fixture_id: &str,
) -> Result<(), String> {
    let fixture = fireplace_fixture_id
        .parse::<StrategicFixtureId>()
        .map_err(|_| "Invalid canonical fireplace identity")?;
    let StrategicFixtureId::Fireplace { place } = fixture else {
        return Err("Fixture is not a fireplace".into());
    };
    match place {
        StrategicPlaceId::SettlementVenue {
            settlement_id,
            kind,
        } => {
            if actor.current_settlement_id.as_deref() != Some(settlement_id.as_str()) {
                return Err("The character is not at this settlement fireplace".into());
            }
            let settlement = ctx
                .db
                .settlement()
                .id()
                .find(settlement_id.as_str().to_string())
                .ok_or("Settlement not found")?;
            let available = match kind {
                adventuresim_core::strategic_place::SettlementVenueKind::Residences => true,
                adventuresim_core::strategic_place::SettlementVenueKind::Keep => matches!(
                    settlement.category,
                    crate::strategic::SettlementCategory::Town
                        | crate::strategic::SettlementCategory::City
                        | crate::strategic::SettlementCategory::Capital
                ),
                adventuresim_core::strategic_place::SettlementVenueKind::Market => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "merchants",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Forge => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "weapons",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Armoury => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "armor",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Tailor => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "clothing",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Herbalist => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "herbalist",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Inn => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "inn",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Church => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "religion",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::Bookstore => {
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "books",
                    )
                }
                adventuresim_core::strategic_place::SettlementVenueKind::PublicSquare => false,
            };
            if !available {
                return Err("This settlement building has no fireplace".into());
            }
            Ok(())
        }
        StrategicPlaceId::ChapterVenue {
            settlement_id,
            organization_id,
            authored_location_id,
        } => {
            if actor.current_settlement_id.as_deref() != Some(settlement_id.as_str()) {
                return Err("The character is not at this settlement fireplace".into());
            }
            let settlement = ctx
                .db
                .settlement()
                .id()
                .find(settlement_id.as_str().to_string())
                .ok_or("Settlement not found")?;
            let available = adventuresim_core::organization::organization_chapter_at(
                settlement_id.as_str(),
                authored_location_id.as_str(),
            )
            .is_some_and(|(organization, chapter)| {
                organization.id == organization_id.as_str()
                    && chapter.location_id == authored_location_id.as_str()
                    && adventuresim_core::organization::chapter_has_standalone_building(
                        organization,
                        chapter,
                        &settlement.economy,
                    )
            });
            if !available {
                return Err("This settlement building has no fireplace".into());
            }
            Ok(())
        }
        StrategicPlaceId::JourneyCamp {
            party_id,
            departure_minute,
            movement_minute,
        } => {
            if actor.party_id.as_deref() != Some(party_id.as_str()) {
                return Err("The character is not in this camp's party".into());
            }
            let current = crate::strategic::current_journey_camp_place(ctx, party_id.as_str())?;
            if current
                != StrategicPlaceId::journey_camp(
                    party_id.as_str(),
                    departure_minute,
                    movement_minute,
                )
                .map_err(|_| "Invalid canonical camp identity")?
            {
                return Err("This is not the party's current journey camp".into());
            }
            Ok(())
        }
        _ => Err("Invalid fireplace place".into()),
    }
}

fn fireplace_station_for(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: &str,
) -> FireplaceStation {
    let key = station_key(character_id, fireplace_fixture_id);
    ctx.db
        .fireplace_station()
        .key()
        .find(key.clone())
        .unwrap_or(FireplaceStation {
            key,
            character_id,
            fireplace_fixture_id: fireplace_fixture_id.into(),
            instrument_item_id: None,
            instrument_object_id: None,
            instrument_return_custody: None,
        })
}

fn method_for_instrument(item_id: Option<&str>) -> Result<CookingMethod, String> {
    match item_id {
        None => Ok(CookingMethod::Roast),
        Some("cooking_pan") => Ok(CookingMethod::PanFry),
        Some("cooking_pot") => Ok(CookingMethod::Stew),
        Some("portable_oven") => Ok(CookingMethod::Bake),
        _ => Err("That item is not a cooking instrument".into()),
    }
}

fn vessel_station_key(character_id: u64, fireplace_fixture_id: &str, object_id: u64) -> String {
    format!("{character_id}|{fireplace_fixture_id}|container:{object_id}")
}

/// Places one exact vessel and its entire subtree over this exact fireplace.
/// The root carried row is removed, so ordinary inventory/trade views cannot
/// remotely transfer it. Children retain their stable object edges.
#[reducer]
pub fn place_fireplace_container(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: String,
    inventory_scope: String,
    inventory_item_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_fixture(ctx, &actor, &fireplace_fixture_id)?;
    let inventory_scope = CarriedInventoryScope::try_from(inventory_scope.as_str())
        .map_err(|error| error.to_string())?;
    let mut object = crate::inventory_container::require_object(
        ctx,
        character_id,
        inventory_scope,
        inventory_item_id,
    )?;
    if crate::inventory_container::object_is_nested(ctx, object.id) {
        return Err(
            "Remove a vessel from its parent container before placing it over a fire".into(),
        );
    }
    method_for_instrument(Some(&object.item_id))?;
    let source_custody =
        crate::object_custody::carried_scope_custody(ctx, &actor, inventory_scope)?;
    let resolved = crate::object_custody::resolve_object_custody(ctx, &object)?;
    if resolved.root != source_custody {
        return Err("Container custody conflicts with the selected inventory".into());
    }
    let persisted_source = crate::object_custody::encode_custody(&source_custody);
    match &object.location {
        InventoryLocation::Personal(location) => {
            ctx.db.inventory_item().id().delete(location.row_id);
        }
        InventoryLocation::Party(location) => {
            ctx.db.party_inventory_item().id().delete(location.row_id);
        }
        _ => return Err("Container is not in carried inventory".into()),
    }
    let key = vessel_station_key(character_id, &fireplace_fixture_id, object.id);
    object.location = InventoryLocation::fireplace(fireplace_fixture_id.clone());
    ctx.db.inventory_object().id().update(object.clone());
    ctx.db.fireplace_station().insert(FireplaceStation {
        key,
        character_id,
        fireplace_fixture_id,
        instrument_item_id: Some(object.item_id),
        instrument_object_id: Some(object.id),
        instrument_return_custody: Some(persisted_source),
    });
    Ok(())
}

#[reducer]
pub fn retrieve_fireplace_container(
    ctx: &ReducerContext,
    character_id: u64,
    fireplace_fixture_id: String,
    container_object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_fixture(ctx, &actor, &fireplace_fixture_id)?;
    let key = vessel_station_key(character_id, &fireplace_fixture_id, container_object_id);
    let station = ctx
        .db
        .fireplace_station()
        .key()
        .find(key.clone())
        .ok_or("Container is not at this fireplace")?;
    if ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(key.clone())
        .is_some()
    {
        return Err("Retrieve the cooked dish before removing its container".into());
    }
    let item_id = station
        .instrument_item_id
        .clone()
        .ok_or("Fireplace vessel is missing")?;
    let fixture = validate_persisted_station_fixture(ctx, &station)?;
    let return_custody = station
        .instrument_return_custody
        .as_ref()
        .ok_or("Container return custody is unknown")?;
    let destination = crate::object_custody::carried_destination(return_custody, character_id)?;
    crate::inventory_container::prevalidate_rehome_subtree(ctx, container_object_id, &destination)?;
    let inventory_row_id = match &destination {
        OperationalCustody::Character(character) => {
            let row = ctx.db.inventory_item().insert(crate::InventoryItem {
                id: 0,
                character_id: character.get(),
                item_id: item_id.clone(),
                quantity: 1,
            });
            row.id
        }
        OperationalCustody::Party(party_id) => {
            if ctx
                .db
                .party_authority()
                .id()
                .find(party_id.as_str().to_owned())
                .is_none()
            {
                return Err("Original party inventory is unavailable".into());
            }
            let row = ctx
                .db
                .party_inventory_item()
                .insert(crate::strategic::PartyInventoryItem {
                    id: 0,
                    party_id: party_id.as_str().into(),
                    item_id: item_id.clone(),
                    quantity: 1,
                });
            row.id
        }
        _ => return Err("Container return custody is not a carried inventory".into()),
    };
    let mut object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object is missing")?;
    crate::object_custody::require_object_at_fixture(ctx, &object, &fixture)?;
    object.location =
        crate::inventory_container::carried_location_for_row(&destination, inventory_row_id)?;
    ctx.db.inventory_object().id().update(object);
    crate::inventory_container::rehome_subtree(ctx, container_object_id, &destination)?;
    ctx.db.fireplace_station().key().delete(key);
    crate::inventory_container::merge_empty_container(ctx, container_object_id)?;
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_adapter_revalidates_and_persists_terminal_attempts() {
        let source = crate::production_source(include_str!("food.rs"));
        let reducer = source
            .split("pub fn prepare_ingredient_lot")
            .nth(1)
            .and_then(|tail| tail.split("fn cooking_method_preparation").next())
            .expect("preparation reducer");
        assert!(reducer.contains("preparation_request_id"));
        assert!(reducer.matches("load_preparation_authority").count() >= 2);
        assert!(reducer.contains("validate_commit"));
        assert!(reducer.contains("validate_material_commit"));
        assert!(reducer.contains("ingredient_preparation_receipt"));
        assert!(reducer.contains("interrupted: true"));
        assert!(reducer.contains("interrupted: false"));
        assert!(reducer.contains("receipt.inventory_scope == inventory_scope"));
        assert!(reducer.contains("receipt.inventory_item_id == inventory_item_id"));
        assert!(reducer.contains("receipt.attempt_generation == attempt_generation"));
        assert!(reducer.contains("effect_commit.is_none()"));
        assert!(reducer.contains("let post = load_preparation_authority"));
        assert!(
            reducer.contains("post.material_source_digest != authority.material_source_digest")
        );
        assert!(reducer.contains("checked_add(1)"));
        assert!(
            source.contains("#[view(accessor = backend_ingredient_preparation_plans, public)]")
        );
        assert!(source.contains("preparation_authority_digest_parts("));
        assert!(source.contains("view_carried_custody_is_fully_resolved"));
        assert!(source.contains("view_direct_custody"));
        assert!(source.contains("party_at_bound_road_challenge_view"));
        assert!(source.contains("Some((CarriedInventoryScope::Personal, row_id))"));
        assert!(source.contains("Some((CarriedInventoryScope::Party, row_id))"));
        assert!(!source.contains("view_actor_has_stable_preparation_interval"));
    }

    #[test]
    fn request_identity_binds_generation_and_submitted_locator() {
        let first = preparation_request_id(
            1,
            "personal",
            2,
            3,
            4,
            5,
            IngredientPreparationAction::Cut,
            0,
            "settlement:test",
            "character:1",
        );
        let next = preparation_request_id(
            1,
            "personal",
            2,
            3,
            4,
            5,
            IngredientPreparationAction::Cut,
            1,
            "settlement:test",
            "character:1",
        );
        let forged_row = preparation_request_id(
            1,
            "personal",
            99,
            3,
            4,
            5,
            IngredientPreparationAction::Cut,
            0,
            "settlement:test",
            "character:1",
        );
        let nested = preparation_request_id(
            1,
            "personal",
            2,
            3,
            4,
            5,
            IngredientPreparationAction::Cut,
            0,
            "settlement:test",
            "container:9",
        );
        assert_ne!(first, next);
        assert_ne!(first, forged_row);
        assert_ne!(first, nested);
    }

    #[test]
    fn grown_contamination_and_terminal_boundaries_are_planning_inputs() {
        let source = crate::production_source(include_str!("food.rs"));
        assert!(source.contains("current_minute.saturating_sub(row.anchor_minute)"));
        assert!(source.contains("preparation_terminal_minute("));
        assert!(source.contains("preview_disease_terminal_boundary"));
        assert!(source.contains("preview_injury_boundary"));
        assert!(source.contains("terminal_minute,"));
        assert!(source.contains("Ingredient preparation wait diverged"));
        let planner = source
            .split("fn preparation_terminal_minute")
            .nth(1)
            .and_then(|tail| tail.split("fn next_preparation_attempt_generation").next())
            .expect("terminal preview");
        assert!(!planner.contains("clip_elapsed_for_disease"));
        assert!(planner.contains("InjuryRecoveryMinutes::new(duration)"));
    }

    #[test]
    fn material_revision_overflow_fails_closed() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(2),
            party_inventory_item_id: None,
            material_revision: u64::MAX,
            display_name: "test".into(),
            preparation: FoodPreparation::Raw,
            ingredient_item_ids: Vec::new(),
            ingredient_quantities: Vec::new(),
            salty_kg: 0.0,
            spicy_kg: 0.0,
            sweet_kg: 0.0,
            sour_kg: 0.0,
            savory_kg: 0.0,
            quality: 1,
            mass_kg: 1.0,
            nutrition_kcal: 1.0,
            total_value: 1.0,
            created_at_minute: 0,
        };
        assert!(retain_lot_fraction(&mut lot, 0.5).is_err());
        assert_eq!(lot.material_revision, u64::MAX);
    }

    #[test]
    fn every_food_lot_constructor_establishes_stable_identity_and_revision() {
        let source = crate::production_source(include_str!("food.rs"));
        assert_eq!(
            source.matches("ctx.db.food_lot().insert(FoodLot {").count(),
            3
        );
        assert_eq!(source.matches("ctx.db.food_lot().insert(child)").count(), 3);
        assert_eq!(source.matches("material_revision: 1").count(), 3);
        assert!(source.matches("ensure_food_material_object").count() >= 8);
        assert!(!source.contains("material_revision: 0"));
    }

    #[test]
    fn every_partial_food_split_scales_full_contamination_provenance() {
        let source = crate::production_source(include_str!("food.rs"));
        assert_eq!(
            source
                .matches("split_food_contamination_provenance(ctx, source.id, child.id, ratio)")
                .count(),
            3
        );
        assert!(source.contains(".insert(FoodContaminationProvenance {"));
        assert!(source.contains("consume_food_contamination_provenance"));
    }

    #[test]
    fn container_cooking_is_distinct_from_loose_roasting() {
        let source = crate::production_source(include_str!("food.rs"));
        assert!(!source.contains("pub fn set_fireplace_instrument"));
        let cooking = source
            .split("fn add_fireplace_ingredients_at")
            .nth(1)
            .unwrap();
        assert!(cooking.contains("let is_vessel = vessel_station.is_some()"));
        assert!(cooking.contains("!is_vessel && inventory_item_ids.len() > 32"));
        assert!(cooking.contains("CookingMethod::Roast"));
    }

    #[test]
    fn vessel_selection_uses_direct_authoritative_food_lots() {
        let source = crate::production_source(include_str!("food.rs"));
        let reducer = source
            .split("pub fn start_fireplace_container_cooking")
            .nth(1)
            .unwrap()
            .split("fn add_fireplace_ingredients_at")
            .next()
            .unwrap();
        assert!(reducer.contains(".parent_object_id()"));
        assert!(reducer.contains(".filter(container_object_id)"));
        assert!(!reducer.contains("subtree_object_ids"));
        assert!(reducer.contains("InventoryLocation::Personal"));
        assert!(reducer.contains("InventoryLocation::Party"));
        assert!(reducer.contains("let (Some(lot), Some(amount))"));
        assert!(reducer.contains("A cooked meal cannot be cooked again"));
    }

    #[test]
    fn eating_and_travel_reconcile_stable_container_objects() {
        let source = crate::production_source(include_str!("food.rs"));
        assert!(source.matches("reconcile_consumed_row(").count() >= 4);
        assert!(source.contains("CarriedInventoryScope::Personal"));
        assert!(source.contains("CarriedInventoryScope::Party"));
        assert!(source.contains("row_is_fireplace_rooted"));
    }

    #[test]
    fn authoritative_preview_rejects_cooked_output_as_an_ingredient() {
        let source = crate::production_source(include_str!("food.rs"));
        let preview = source
            .split("pub fn preview_cooking")
            .nth(1)
            .and_then(|tail| tail.split("fn expose_to_dysentery").next())
            .expect("preview cooking implementation");
        assert!(preview.contains("food::is_cookable_ingredient(&inventory.item_id)"));
        assert!(preview.contains("A cooked meal cannot be cooked again"));
    }

    #[test]
    fn partial_lot_retains_quality_and_scales_every_flavor() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(2),
            party_inventory_item_id: None,
            material_revision: 1,
            display_name: "Roasted test".into(),
            preparation: FoodPreparation::Roasted,
            ingredient_item_ids: vec!["salt".into()],
            ingredient_quantities: vec![1.0],
            salty_kg: 1.0,
            spicy_kg: 0.8,
            sweet_kg: 0.6,
            sour_kg: 0.4,
            savory_kg: 0.2,
            quality: 4,
            mass_kg: 1.0,
            nutrition_kcal: 100.0,
            total_value: 10.0,
            created_at_minute: 0,
        };
        retain_lot_fraction(&mut lot, 0.25).unwrap();
        assert_eq!(lot.quality, 4);
        assert_eq!(lot.salty_kg, 0.25);
        assert_eq!(lot.spicy_kg, 0.2);
        assert_eq!(lot.sweet_kg, 0.15);
        assert_eq!(lot.sour_kg, 0.1);
        assert_eq!(lot.savory_kg, 0.05);
    }

    #[test]
    fn stew_water_and_fireplace_escrow_contract_are_explicit() {
        let source = crate::production_source(include_str!("food.rs"));
        let cook = source
            .split("pub fn add_fireplace_ingredients")
            .nth(1)
            .and_then(|tail| tail.split("pub fn retrieve_fireplace_dish").next())
            .expect("fireplace ingredient reducer source");
        assert!(cook.contains("mass += water_ml / 1_000.0"));
        assert!(cook.contains("contained_water_ml"));
        assert!(cook.contains("food::microbial_concentration(loads.iter().sum(), mass)"));
        assert!(cook.contains("if method == CookingMethod::Stew"));
        assert!(cook.contains("ctx.db.fireplace_dish().insert"));
        assert!(!cook.contains("advance_character_wait_time"));
        assert!(!cook.contains("consume_food_amount(ctx, character_id"));
        assert!(cook.contains("pan_fry_has_enough_fat"));
        assert!(cook.contains("chef_quality_tier"));
    }

    #[test]
    fn fireplace_authority_is_private_location_bound_and_race_safe() {
        let source = crate::production_source(include_str!("food.rs"));
        assert!(source.contains("#[table(accessor = fireplace_station)]"));
        assert!(source.contains("#[table(accessor = fireplace_dish)]"));
        assert!(source.contains("#[view(accessor = backend_fireplace_stations, public)]"));
        assert!(source.contains("#[view(accessor = backend_fireplace_dishes, public)]"));
        let dish_projection = source
            .split("pub struct BackendFireplaceDish")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[view(accessor = backend_fireplace_dishes")
                    .next()
            })
            .expect("gateway dish projection");
        assert!(!dish_projection.contains("raw_contamination"));
        assert!(source.contains("parse::<StrategicFixtureId>()"));
        assert!(source.contains("current_journey_camp_place"));
        assert!(source.contains("fireplace_fixture_id"));
        assert!(!source.contains("pub context_key: String"));
        let camp_custody = source
            .split("pub(crate) fn require_clear_current_camp_fireplace")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn require_members_clear_current_camp_fireplace")
                    .next()
            })
            .expect("camp fireplace custody guard");
        assert!(camp_custody.contains("validate_persisted_station_fixture"));
        assert!(camp_custody.contains("validate_persisted_dish_fixture"));
        assert!(!camp_custody.contains("ends_with"));
        assert!(source.contains("This fireplace already holds a dish"));
        assert!(source.contains("Food lot selections must be unique and positive"));
        assert!(source.contains("vessel_station_key"));
        assert!(source.contains("Retrieve the cooked dish before removing its container"));
        let container_retrieval = source
            .split("pub fn retrieve_fireplace_container")
            .nth(1)
            .and_then(|tail| tail.split("fn preparation_skill_check").next())
            .expect("container retrieval reducer");
        assert!(container_retrieval.contains("OperationalCustody::Party(party_id)"));
        assert!(container_retrieval.contains(".party_authority()"));
        assert!(container_retrieval.contains(".is_none()"));
        assert!(source.contains("instrument_return_custody"));
        assert!(!source.contains("instrument_party_id"));
    }

    #[test]
    fn camp_departure_and_retrieval_cleanup_enforce_fireplace_custody() {
        let food_source = crate::production_source(include_str!("food.rs"));
        let travel_source = crate::production_source(include_str!("strategic/travel_reducers.rs"));
        assert!(travel_source.contains("require_clear_current_camp_fireplace"));
        assert!(food_source.contains(
            "Retrieve every dish and remove every cooking instrument before breaking camp"
        ));
        let retrieval = food_source
            .split("pub fn retrieve_fireplace_dish")
            .nth(1)
            .and_then(|tail| tail.split("pub fn preview_cooking").next())
            .expect("dish retrieval reducer");
        assert!(retrieval.contains("fireplace_station().key().delete"));
        assert!(!retrieval.contains("train_skill"));
        assert!(!retrieval.contains("morale"));
    }

    #[test]
    fn dish_retrieval_is_bound_to_immutable_source_custody() {
        let party_source = crate::object_custody::encode_custody(
            &adventuresim_core::physical_object::OperationalCustody::party("party-before-transfer")
                .unwrap(),
        );
        let expected =
            OperationalCustody::party("party-before-transfer").map_err(|error| error.to_string());
        assert_eq!(dish_inventory_destination(&party_source, 7), expected);

        let personal_source = crate::object_custody::encode_custody(
            &adventuresim_core::physical_object::OperationalCustody::character(7).unwrap(),
        );
        assert!(dish_inventory_destination(&personal_source, 8).is_err());
    }

    #[test]
    fn fireplace_container_retrieval_rejects_tactical_actors() {
        let source = crate::production_source(include_str!("food.rs"));
        let retrieval = source
            .split("pub fn retrieve_fireplace_container")
            .nth(1)
            .and_then(|tail| tail.split("fn preparation_skill_check").next())
            .expect("container retrieval reducer");
        assert!(retrieval.contains("if actor.in_server"));
        assert!(retrieval.contains("Cooking is unavailable during a tactical encounter"));
    }

    #[test]
    fn party_exit_and_death_have_explicit_fireplace_custody_policy() {
        let food_source = crate::production_source(include_str!("food.rs"));
        let party_source = include_str!("strategic/inventory_trade.rs");
        let character_source = crate::production_source(include_str!("character.rs"));
        let removal = party_source
            .split("pub fn remove_party_member")
            .nth(1)
            .and_then(|tail| tail.split("pub fn disband_party").next())
            .expect("party member removal reducer");
        let disband = party_source
            .split("pub fn disband_party")
            .nth(1)
            .expect("party disband reducer");
        assert!(removal.contains("require_members_clear_current_camp_fireplace"));
        assert!(disband.contains("require_members_clear_current_camp_fireplace"));
        assert!(
            character_source
                .contains("crate::food::cleanup_fireplace_custody_for_death(ctx, character_id)")
        );

        let cleanup = food_source
            .split("pub(crate) fn cleanup_fireplace_custody_for_death")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_fireplace_fixture").next())
            .expect("death fireplace cleanup");
        assert!(cleanup.contains("fireplace_dish()"));
        assert!(cleanup.contains(".character_id()"));
        assert!(cleanup.contains("add_to_party_inventory_checked"));
        assert!(cleanup.contains("ctx.db.inventory_item().insert"));
        assert!(cleanup.contains("fireplace_station().key().delete"));
        assert!(cleanup.contains("prevalidate_rehome_subtree"));
        assert!(cleanup.contains("rehome_subtree(ctx, object_id, &destination)?"));
        assert!(!cleanup.contains("let _ ="));
        assert!(cleanup.contains("Abandoned tools remain installed at their station"));
    }

    #[test]
    fn catalog_quality_is_copied_when_lots_are_acquired() {
        let source = crate::production_source(include_str!("food.rs"));
        let constructor = source
            .split("pub fn create_personal_food_lot")
            .nth(1)
            .and_then(|tail| tail.split("pub fn create_party_food_lot").next())
            .expect("personal lot constructor");
        assert!(constructor.contains("quality: definition.default_quality.clamp(1, 5)"));
    }

    #[test]
    fn hidden_food_contamination_uses_explicit_food_water_prevention() {
        let source = crate::production_source(include_str!("food.rs"));
        let exposure = source
            .split("fn expose_to_dysentery")
            .nth(1)
            .and_then(|tail| tail.split("fn consume_food_amount").next())
            .expect("foodborne exposure source");
        assert!(exposure.contains("protected_point_exposure"));
        assert!(exposure.contains("TransmissionVector::FoodWater"));
        assert!(exposure.contains("protected_dose"));
    }

    #[test]
    fn physical_preparation_keeps_safe_prefix_and_exact_instance_tool_rules() {
        let source = crate::production_source(include_str!("food.rs"));
        let reducer = source
            .split("pub fn prepare_ingredient_lot")
            .nth(1)
            .unwrap();
        let wait = reducer.find("advance_character_wait_time").unwrap();
        assert!(wait < reducer.find("lot.preparation = post.next").unwrap());
        assert!(wait < reducer.find("apply_direct_training").unwrap());
        assert!(source.contains(
            "effective_weapon_stat(item.accuracy, damage, item.edge_sensitivity) >= 0.5"
        ));
        assert!(source.contains("row_is_fireplace_rooted"));
        assert!(source.contains("Skill::Knife"));
        assert!(source.contains("Skill::Bludgeon"));
    }

    #[test]
    fn vessel_selection_is_direct_and_preparation_shortens_safety_time() {
        let source = crate::production_source(include_str!("food.rs"));
        assert!(source.contains(".parent_object_id()"));
        assert!(source.contains(".filter(container_object_id)"));
        assert!(source.contains("CUT_COOKING_TIME_FACTOR"));
        assert!(source.contains("GROUND_COOKING_TIME_FACTOR"));
        assert!(source.contains("method_doneness_outcome"));
    }
}
