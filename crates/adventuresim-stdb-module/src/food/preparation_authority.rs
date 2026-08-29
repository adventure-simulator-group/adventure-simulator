// Owns immutable request identity, custody/material validation, and authority digests.
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
