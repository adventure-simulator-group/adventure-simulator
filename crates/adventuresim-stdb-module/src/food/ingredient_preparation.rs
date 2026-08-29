// Owns ingredient preparation tools, timing, mutations, and reducer coordination.
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

fn cutting_weapon_binding(
    scope: CarriedInventoryScope,
    row_id: u64,
    item_id: &str,
    accuracy: f32,
    edge_sensitivity: f32,
    damage: DamageBins,
) -> String {
    use sha2::Digest as _;

    let mut hash = sha2::Sha256::new();
    for value in [
        b"adventuresim.ingredient-preparation.cutting-weapon".as_slice(),
        1u16.to_le_bytes().as_slice(),
        scope.as_str().as_bytes(),
        row_id.to_le_bytes().as_slice(),
        item_id.as_bytes(),
        accuracy.to_bits().to_le_bytes().as_slice(),
        edge_sensitivity.to_bits().to_le_bytes().as_slice(),
    ] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value);
    }
    for damage_bits in damage.0.map(f32::to_bits) {
        let value = damage_bits.to_le_bytes();
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value);
    }
    format!("cutting-weapon:v1:{:x}", hash.finalize())
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
                    cutting_weapon_binding(
                        scope,
                        row_id,
                        &item.id,
                        item.accuracy,
                        item.edge_sensitivity,
                        damage,
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
