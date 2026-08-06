//! Private generated-outbreak authority and patient materialization.
//!
//! Canonical disease, source and remediation facts never cross a public view.

use spacetimedb::{ReducerContext, Table, ViewContext, reducer, table};
use std::str::FromStr;

use adventuresim_core::strategic_place::{StrategicFixtureId, StrategicPlaceId};

use crate::{
    character::{character, character_attributes, character_death},
    corpse::strategic_corpse,
    disease::infection_episode,
    inventory_container::{container_liquid, inventory_object},
    investigation::investigation_action_capability,
    local_problem::{local_problem_receipt, local_problem_receipt__view},
    relationship::character_kinship,
    settlement_population::{settlement_resident_presence, settlement_resident_profile},
    time::character_time,
    world_actor::character_context_membership,
};

const MAX_OUTBREAK_PATIENTS: usize = 8;

#[derive(Clone, Debug)]
#[table(accessor = outbreak_authority)]
pub struct OutbreakAuthority {
    #[primary_key]
    pub case_id: String,
    #[unique]
    pub problem_id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub disease_id: String,
    pub transmission_route: String,
    pub source_kind: String,
    pub source_json: String,
    /// Canonical `StrategicFixtureId::OutbreakSource` encoding.
    pub physical_source_fixture_id: String,
    /// Canonical `StrategicPlaceId::CaseSite` encoding.
    pub patient_presentation_place_id: String,
    pub responsible_resident_character_id: Option<u64>,
    pub culpability: Option<String>,
    pub carrier_threat_id: Option<String>,
    pub chronology_json: String,
    pub remediation_id: String,
    pub remediation_json: String,
    pub remediated_at: Option<u64>,
    pub remediated_by_party_id: Option<String>,
    pub remediation_source_id: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = outbreak_patient_authority)]
pub struct OutbreakPatientAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub patient_character_id: u64,
    #[unique]
    pub episode_id: u64,
    pub context_active: bool,
    pub health_active: bool,
    pub corpse_id: Option<String>,
    pub autopsy_evidence_id: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = outbreak_source_presence_span)]
pub struct OutbreakSourcePresenceSpan {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    /// Canonical `StrategicPlaceId::CaseSite` encoding.
    pub source_place_id: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

/// Private material truth for one ordinary collectible water source.
#[derive(Clone, Debug)]
#[table(accessor = water_material_lot)]
pub struct WaterMaterialLot {
    #[primary_key]
    pub id: u64,
    #[unique]
    pub source_fixture_id: String,
    pub outbreak_case_id: String,
    pub liquid_item_id: String,
    pub concentration_anchor: f32,
    pub growth_per_hour: f32,
    pub anchor_minute: u64,
}

/// Private fixture stock. Public views expose neither contamination nor its
/// relationship to an outbreak.
#[derive(Clone, Debug)]
#[table(accessor = outbreak_water_source)]
pub struct OutbreakWaterSource {
    #[primary_key]
    pub fixture_id: String,
    #[unique]
    pub material_lot_id: u64,
    pub available_ml: u64,
    pub revision: u64,
    pub disabled_at: Option<u64>,
}

/// Private immutable output identity for one successful fixture draw.
#[derive(Clone, Debug)]
#[table(accessor = water_output_lot)]
pub struct WaterOutputLot {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub container_object_id: u64,
    pub source_material_lot_id: u64,
    pub amount_ml: u64,
    pub contaminant_load_microunits: u64,
    pub concentration_anchor: f32,
    pub growth_per_hour: f32,
    pub anchor_minute: u64,
}

/// Private exact material contributions in containers and legacy water pools.
#[derive(Clone, Debug)]
#[table(accessor = water_holding_contribution)]
pub struct WaterHoldingContribution {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub holding_key: String,
    pub material_lot_id: u64,
    pub amount_microliters: u64,
    pub contaminant_load_microunits: u64,
    pub collected_at: u64,
}

fn water_holding_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

fn water_holding_contribution_id(kind: &str, id: &str, material_lot_id: u64) -> String {
    format!("{}:{material_lot_id}", water_holding_key(kind, id))
}

pub(crate) fn delete_water_holding_contributions(ctx: &ReducerContext, kind: &str, id: &str) {
    let key = water_holding_key(kind, id);
    for row in ctx
        .db
        .water_holding_contribution()
        .holding_key()
        .filter(&key)
        .collect::<Vec<_>>()
    {
        ctx.db.water_holding_contribution().id().delete(row.id);
    }
}

pub(crate) fn move_water_holding_contributions(
    ctx: &ReducerContext,
    source_kind: &str,
    source_id: &str,
    destination_kind: &str,
    destination_id: &str,
    source_total_microliters: u64,
    moved_water_microliters: u64,
) -> Result<Vec<(u64, u64, u64)>, String> {
    if moved_water_microliters > source_total_microliters {
        return Err("Water material transfer exceeds public volume".into());
    }
    let source_key = water_holding_key(source_kind, source_id);
    let mut rows = ctx
        .db
        .water_holding_contribution()
        .holding_key()
        .filter(&source_key)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.material_lot_id);
    let mut moved = Vec::new();
    for mut row in rows {
        let amount_before = row.amount_microliters;
        let load_before = row.contaminant_load_microunits;
        let (amount, moved_load) = adventuresim_core::water_source::proportional_material_transfer(
            source_total_microliters,
            moved_water_microliters,
            amount_before,
            load_before,
        )
        .ok_or("Water material transfer exceeds public volume")?;
        if amount == 0 {
            continue;
        }
        row.amount_microliters -= amount;
        row.contaminant_load_microunits -= moved_load;
        if row.amount_microliters == 0 {
            ctx.db.water_holding_contribution().id().delete(row.id);
        } else {
            ctx.db.water_holding_contribution().id().update(row.clone());
        }
        let destination_row_id =
            water_holding_contribution_id(destination_kind, destination_id, row.material_lot_id);
        if let Some(mut destination) = ctx
            .db
            .water_holding_contribution()
            .id()
            .find(&destination_row_id)
        {
            destination.amount_microliters = destination
                .amount_microliters
                .checked_add(amount)
                .ok_or("Water material amount overflow")?;
            destination.contaminant_load_microunits = destination
                .contaminant_load_microunits
                .checked_add(moved_load)
                .ok_or("Water contaminant load overflow")?;
            ctx.db.water_holding_contribution().id().update(destination);
        } else {
            ctx.db
                .water_holding_contribution()
                .insert(WaterHoldingContribution {
                    id: destination_row_id,
                    holding_key: water_holding_key(destination_kind, destination_id),
                    material_lot_id: row.material_lot_id,
                    amount_microliters: amount,
                    contaminant_load_microunits: moved_load,
                    collected_at: row.collected_at,
                });
        }
        moved.push((row.material_lot_id, amount, moved_load));
    }
    Ok(moved)
}

pub(crate) fn consume_water_holding_contributions(
    ctx: &ReducerContext,
    source_kind: &str,
    source_id: &str,
    source_total_ml: f32,
    consumed_water_ml: f32,
    consumer_character_id: u64,
) -> Result<(), String> {
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(consumer_character_id)
        .map_or(0, |row| row.minutes);
    let sink = format!("consume:{consumer_character_id}:{}", minute);
    let moved = move_water_holding_contributions(
        ctx,
        source_kind,
        source_id,
        "consumed",
        &sink,
        (source_total_ml.max(0.0) * 1_000.0).round() as u64,
        (consumed_water_ml.max(0.0) * 1_000.0).round() as u64,
    )?;
    delete_water_holding_contributions(ctx, "consumed", &sink);
    if !moved.is_empty() {
        expose_to_water_contributions(ctx, consumer_character_id, &moved)?;
    }
    Ok(())
}

fn expose_to_water_contributions(
    ctx: &ReducerContext,
    character_id: u64,
    moved: &[(u64, u64, u64)],
) -> Result<(), String> {
    use sha2::{Digest as _, Sha256};
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |row| row.minutes);
    let mut digest = Sha256::new();
    let mut dose = 0.0_f32;
    for &(output_lot_id, amount_microliters, anchor_load) in moved {
        let lot = ctx
            .db
            .water_output_lot()
            .id()
            .find(output_lot_id)
            .ok_or("Consumed water material provenance is incomplete")?;
        digest.update(output_lot_id.to_le_bytes());
        digest.update(amount_microliters.to_le_bytes());
        digest.update(anchor_load.to_le_bytes());
        let amount_ml = amount_microliters as f32 / 1_000.0;
        let anchor_concentration = anchor_load as f32 / (amount_ml.max(0.001) * 1_000.0);
        let current = adventuresim_core::food::contamination_at(
            anchor_concentration,
            lot.growth_per_hour,
            minute.saturating_sub(lot.anchor_minute),
        );
        dose += current * amount_ml / 1_000.0;
    }
    let bytes: [u8; 32] = digest.finalize().into();
    let contribution_digest = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let carrier_id = u64::from_le_bytes(bytes[..8].try_into().unwrap()).max(1);
    crate::food::expose_food_water_dysentery(
        ctx,
        character_id,
        &format!("water:{minute}:{contribution_digest}"),
        carrier_id,
        minute,
        dose,
        &contribution_digest,
        10_000,
    )
}

#[derive(Clone, Debug)]
#[table(accessor = water_collection_receipt)]
pub struct WaterCollectionReceipt {
    #[primary_key]
    pub request_id: String,
    pub character_id: u64,
    pub capability_id: String,
    pub capability_version: u32,
    pub source_fixture_id: String,
    pub container_object_id: u64,
    pub material_lot_id: u64,
    pub source_material_lot_id: u64,
    pub source_revision_before: u64,
    pub source_amount_before_ml: u64,
    pub source_amount_after_ml: u64,
    pub contaminant_load_microunits: u64,
    pub amount_ml: u64,
    pub collected_at: u64,
}

#[reducer]
pub fn collect_fixture_water_into_container(
    ctx: &ReducerContext,
    request_id: String,
    character_id: u64,
    capability_id: String,
    expected_capability_version: u32,
    container_object_id: u64,
    requested_ml: u64,
) -> Result<(), String> {
    use adventuresim_core::{
        material::MaterialLotId,
        physical_object::{CustodyCharacterId, PhysicalObjectId},
        rights::RightsDecisionKind,
        strategic_action::{
            ActionCoordinates, ActionTarget, AuthoritativeSnapshot, AuthorityBinding, PlanInput,
            PlanProvenance, PlanningOutcome, PublicRejection, RequestedDuration, SnapshotDigest,
            SnapshotRevision, TimeBoundaries, ToolReference,
        },
        water_source::{
            WaterCollectionAuthority, build_water_collection_plan, conserved_collection,
            decide_public_water_collection, water_collection_question,
            water_container_alter_question,
        },
    };
    use sha2::{Digest as _, Sha256};

    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    if request_id.is_empty() || request_id.len() > 192 || requested_ml == 0 {
        return Err("Invalid water collection request".into());
    }
    if let Some(existing) = ctx
        .db
        .water_collection_receipt()
        .request_id()
        .find(&request_id)
    {
        return if existing.character_id == character_id
            && existing.capability_id == capability_id
            && existing.capability_version == expected_capability_version
            && existing.container_object_id == container_object_id
            && existing.amount_ml == requested_ml
        {
            Ok(())
        } else {
            Err("Water collection request conflicts with an earlier action".into())
        };
    }
    let capability = ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&capability_id)
        .filter(|capability| {
            capability.owner_character_id == character_id
                && capability.active
                && capability.version == expected_capability_version
                && capability.provenance_kind == "generated"
                && capability.target_kind == "site"
        })
        .ok_or("Water collection action is unavailable")?;
    let authority = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&capability.generated_case_id)
        .ok_or("Water collection action is unavailable")?;
    let source_fixture_id = authority.physical_source_fixture_id.clone();
    let mut source = ctx
        .db
        .outbreak_water_source()
        .fixture_id()
        .find(&source_fixture_id)
        .ok_or("Water collection is unavailable")?;
    let lot = ctx
        .db
        .water_material_lot()
        .id()
        .find(&source.material_lot_id)
        .ok_or("Water collection is unavailable")?;
    if lot.outbreak_case_id != authority.case_id {
        return Err("Water collection action is unavailable".into());
    }
    let fixture = parse_outbreak_source_fixture(&source_fixture_id, &authority.case_id)
        .ok_or("Water collection is unavailable")?;
    if fixture.place().case_site_id() != Some(capability.target_id.as_str()) {
        return Err("Water collection action is unavailable".into());
    }
    let exact_presence = crate::investigation::character_case_site_id(ctx, character_id).as_deref()
        == fixture.place().case_site_id();
    let actor = crate::character::require_living_character(ctx, character_id)?;
    let object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object not found")?;
    let resolved_custody =
        crate::object_custody::require_actor_carried_object(ctx, &actor, &object);
    let container_custody = resolved_custody.is_ok();
    let mutable = crate::inventory_container::require_mutable(ctx, object.id).is_ok();
    let capacity_available =
        crate::inventory_container::require_container_capacity(ctx, object.id, requested_ml)
            .is_ok();
    let existing_liquid = ctx
        .db
        .container_liquid()
        .container_object_id()
        .find(object.id);
    let material_compatible = existing_liquid
        .as_ref()
        .is_none_or(|liquid| liquid.liquid_item_id == crate::inventory_container::WATER_ITEM_ID);
    let container_before = existing_liquid.as_ref().map_or(0, |liquid| liquid.water_ml);
    let conserved = conserved_collection(source.available_ml, container_before, requested_ml);
    let question = water_collection_question(
        CustodyCharacterId::try_new(character_id)
            .map_err(|_| "Water collector identity is malformed")?,
        fixture.clone(),
    )
    .map_err(|_| "Water collection rights question is malformed")?;
    let rights = decide_public_water_collection(
        &question,
        source.disabled_at.is_none(),
        source.disabled_at.unwrap_or(lot.anchor_minute),
    );
    let container_question = water_container_alter_question(
        CustodyCharacterId::try_new(character_id).map_err(|_| "Invalid water collector")?,
        PhysicalObjectId::try_new(object.id).map_err(|_| "Invalid container identity")?,
        fixture.place().clone(),
    )
    .map_err(|_| "Water collection is unavailable")?;
    let container_rights =
        decide_public_water_collection(&container_question, container_custody, lot.anchor_minute);
    let mut hash = Sha256::new();
    hash.update(b"water-collection-snapshot-v1");
    hash.update(request_id.as_bytes());
    hash.update(source_fixture_id.as_bytes());
    hash.update(container_object_id.to_le_bytes());
    hash.update(requested_ml.to_le_bytes());
    hash.update(source.available_ml.to_le_bytes());
    hash.update(container_before.to_le_bytes());
    hash.update(lot.id.to_le_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let coordinates = ActionCoordinates::try_new(
        CustodyCharacterId::try_new(character_id).map_err(|_| "Invalid water collector")?,
        ActionTarget::Fixture(fixture.clone()),
        fixture.place().clone(),
        Some(fixture.clone()),
        resolved_custody
            .as_ref()
            .ok()
            .and_then(|custody| {
                ToolReference::try_new(
                    PhysicalObjectId::try_new(object.id).ok()?,
                    custody.root.clone(),
                )
                .ok()
            })
            .into_iter()
            .collect(),
    )
    .map_err(|_| "Water collection coordinates are inconsistent")?;
    let snapshot = AuthoritativeSnapshot {
        revision: SnapshotRevision(lot.anchor_minute),
        digest: SnapshotDigest(digest),
    };
    let action_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let plan = build_water_collection_plan(WaterCollectionAuthority {
        coordinates: coordinates.clone(),
        plan: PlanInput {
            coordinates,
            provenance: PlanProvenance {
                request_id: adventuresim_core::strategic_action::ActionRequestId::try_new(
                    request_id.clone(),
                )
                .map_err(|_| "Invalid water request identity")?,
                action_id: adventuresim_core::strategic_action::ActionDefinitionId::try_new(
                    "collect-fixture-water",
                )
                .unwrap(),
                input_digest: SnapshotDigest(digest),
                authority_binding: AuthorityBinding(digest),
            },
            snapshot,
            current_minute: action_minute,
            duration: RequestedDuration::try_new(5).unwrap(),
            boundaries: TimeBoundaries {
                terminal_minute: None,
                interruption: None,
            },
            requirements: Vec::new(),
            sanitized_rejection: PublicRejection::Unavailable,
        },
        container_object_id: PhysicalObjectId::try_new(object.id)
            .map_err(|_| "Container identity is malformed")?,
        material_lot_id: MaterialLotId::try_new(lot.id)
            .map_err(|_| "Water material identity is malformed")?,
        amount_ml: requested_ml,
        rights_allowed: rights.kind() == RightsDecisionKind::Allowed
            && container_rights.kind() == RightsDecisionKind::Allowed,
        exact_presence,
        source_available: source.disabled_at.is_none() && conserved.is_some(),
        container_custody,
        mutable,
        capacity_available,
        material_compatible,
    });
    let PlanningOutcome::Ready(plan) = plan else {
        return Err("Water collection is unavailable".into());
    };
    let output_lot_id = water_output_lot_id(&request_id, lot.id, object.id);
    if ctx.db.water_output_lot().id().find(output_lot_id).is_some() {
        return Err("Water collection output identity conflicts".into());
    }
    crate::time::advance_character_time(ctx, character_id, plan.time().elapsed_minutes)?;
    let (source_after, container_after) = conserved.unwrap();
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if crate::investigation::character_case_site_id(ctx, character_id).as_deref()
        != fixture.place().case_site_id()
        || crate::object_custody::require_actor_carried_object(ctx, &actor, &object).is_err()
        || crate::inventory_container::require_mutable(ctx, object.id).is_err()
        || crate::inventory_container::require_container_capacity(ctx, object.id, requested_ml)
            .is_err()
        || ctx
            .db
            .outbreak_water_source()
            .fixture_id()
            .find(&source_fixture_id)
            .is_none_or(|current| {
                current.available_ml != source.available_ml
                    || current.revision != source.revision
                    || current.material_lot_id != source.material_lot_id
                    || current.disabled_at.is_some()
            })
        || ctx
            .db
            .water_material_lot()
            .id()
            .find(lot.id)
            .is_none_or(|current| {
                current.source_fixture_id != lot.source_fixture_id
                    || current.outbreak_case_id != lot.outbreak_case_id
                    || current.concentration_anchor != lot.concentration_anchor
                    || current.growth_per_hour != lot.growth_per_hour
                    || current.anchor_minute != lot.anchor_minute
            })
        || ctx
            .db
            .investigation_action_capability()
            .id()
            .find(&capability_id)
            .is_none_or(|current| {
                current.owner_character_id != character_id
                    || !current.active
                    || current.version != expected_capability_version
                    || current.generated_case_id != authority.case_id
                    || current.target_id != capability.target_id
            })
        || ctx
            .db
            .container_liquid()
            .container_object_id()
            .find(object.id)
            .map(|liquid| (liquid.liquid_item_id, liquid.water_ml))
            != existing_liquid
                .as_ref()
                .map(|liquid| (liquid.liquid_item_id.clone(), liquid.water_ml))
    {
        return Err("Water collection is unavailable".into());
    }
    let source_before = source.available_ml;
    let source_revision_before = source.revision;
    let current_concentration = adventuresim_core::food::contamination_at(
        lot.concentration_anchor,
        lot.growth_per_hour,
        plan.time().end_minute.saturating_sub(lot.anchor_minute),
    );
    let contaminant_load_microunits =
        (current_concentration.max(0.0) * requested_ml as f32 * 1_000.0).round() as u64;
    source.available_ml = source_after;
    source.revision = source
        .revision
        .checked_add(1)
        .ok_or("Water source revision overflow")?;
    ctx.db.outbreak_water_source().fixture_id().update(source);
    if let Some(mut liquid) = existing_liquid {
        liquid.water_ml = container_after;
        ctx.db
            .container_liquid()
            .container_object_id()
            .update(liquid);
    } else {
        ctx.db.container_liquid().insert(crate::ContainerLiquid {
            container_object_id: object.id,
            liquid_item_id: crate::inventory_container::WATER_ITEM_ID.into(),
            water_ml: container_after,
        });
    }
    ctx.db
        .water_holding_contribution()
        .insert(WaterHoldingContribution {
            id: water_holding_contribution_id("container", &object.id.to_string(), output_lot_id),
            holding_key: water_holding_key("container", &object.id.to_string()),
            material_lot_id: output_lot_id,
            amount_microliters: requested_ml.saturating_mul(1_000),
            contaminant_load_microunits,
            collected_at: plan.time().end_minute,
        });
    ctx.db.water_output_lot().insert(WaterOutputLot {
        id: output_lot_id,
        container_object_id: object.id,
        source_material_lot_id: lot.id,
        amount_ml: requested_ml,
        contaminant_load_microunits,
        concentration_anchor: current_concentration,
        growth_per_hour: lot.growth_per_hour,
        anchor_minute: plan.time().end_minute,
    });
    ctx.db
        .water_collection_receipt()
        .insert(WaterCollectionReceipt {
            request_id,
            character_id,
            capability_id,
            capability_version: expected_capability_version,
            source_fixture_id,
            container_object_id: object.id,
            material_lot_id: output_lot_id,
            source_material_lot_id: lot.id,
            source_revision_before,
            source_amount_before_ml: source_before,
            source_amount_after_ml: source_after,
            contaminant_load_microunits,
            amount_ml: requested_ml,
            collected_at: plan.time().end_minute,
        });
    Ok(())
}

pub(crate) fn contained_water_contamination(
    ctx: &ReducerContext,
    container_object_id: u64,
    minute: u64,
) -> Result<Vec<(u64, f32, f32, u64)>, String> {
    water_holding_contamination(
        ctx,
        "container",
        &container_object_id.to_string(),
        ctx.db
            .container_liquid()
            .container_object_id()
            .find(container_object_id)
            .map_or(0, |liquid| liquid.water_ml.saturating_mul(1_000)),
        u64::MAX,
        minute,
    )
}

pub(crate) fn water_holding_contamination(
    ctx: &ReducerContext,
    kind: &str,
    id: &str,
    source_total_microliters: u64,
    moved_microliters: u64,
    minute: u64,
) -> Result<Vec<(u64, f32, f32, u64)>, String> {
    let key = water_holding_key(kind, id);
    let mut contributions = ctx
        .db
        .water_holding_contribution()
        .holding_key()
        .filter(&key)
        .collect::<Vec<_>>();
    contributions.sort_by_key(|row| row.material_lot_id);
    let mut result = Vec::new();
    for contribution in contributions {
        let lot = ctx
            .db
            .water_output_lot()
            .id()
            .find(contribution.material_lot_id)
            .ok_or("Contained water material provenance is incomplete")?;
        if lot.id != contribution.material_lot_id {
            return Err("Contained water material provenance conflicts".into());
        }
        if contribution.amount_microliters == 0 {
            return Err("Contained water material has zero measure".into());
        }
        let (amount_microliters, selected_load) =
            adventuresim_core::water_source::proportional_material_transfer(
                source_total_microliters,
                moved_microliters.min(source_total_microliters),
                contribution.amount_microliters,
                contribution.contaminant_load_microunits,
            )
            .ok_or("Water material preview exceeds public volume")?;
        if amount_microliters == 0 {
            continue;
        }
        let amount_ml = amount_microliters as f32 / 1_000.0;
        let held_anchor_concentration = selected_load as f32 / (amount_ml as f32 * 1_000.0);
        let current = adventuresim_core::food::contamination_at(
            held_anchor_concentration,
            lot.growth_per_hour,
            minute.saturating_sub(lot.anchor_minute),
        );
        result.push((lot.id, current, lot.growth_per_hour, amount_microliters));
    }
    result.sort_by_key(|row| row.0);
    Ok(result)
}

pub(crate) fn source_material_knowledge_provenance(
    ctx: &ReducerContext,
    case_id: &str,
    site_id: &str,
) -> Result<Option<String>, String> {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
    else {
        return Ok(None);
    };
    let outbreak_source: adventuresim_core::quest_generation::OutbreakSource =
        serde_json::from_str(&authority.source_json)
            .map_err(|_| "Generated outbreak source authority is malformed")?;
    if !matches!(
        outbreak_source,
        adventuresim_core::quest_generation::OutbreakSource::Sanitation {
            practice:
                adventuresim_core::quest_generation::OutbreakSanitationPractice::ContaminatedWell
        }
    ) {
        return Ok(None);
    }
    let fixture = parse_outbreak_source_fixture(&authority.physical_source_fixture_id, case_id)
        .ok_or("Generated outbreak source fixture is malformed")?;
    if fixture.place().case_site_id() != Some(site_id) {
        return Ok(None);
    }
    let source = ctx
        .db
        .outbreak_water_source()
        .fixture_id()
        .find(&authority.physical_source_fixture_id)
        .ok_or("Generated outbreak water material is missing")?;
    let lot = ctx
        .db
        .water_material_lot()
        .id()
        .find(source.material_lot_id)
        .ok_or("Generated outbreak water material is missing")?;
    if lot.source_fixture_id != authority.physical_source_fixture_id
        || lot.outbreak_case_id != authority.case_id
    {
        return Err("Generated outbreak water material conflicts".into());
    }
    use sha2::Digest as _;
    let digest: [u8; 32] = sha2::Sha256::digest(
        format!(
            "{}:{}",
            authority.physical_source_fixture_id, source.material_lot_id
        )
        .as_bytes(),
    )
    .into();
    let short = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(Some(format!("source-material:{short}")))
}

/// Case-site Patient rows are visible only to a party whose leader has learned
/// the underlying local problem. Exact physical co-presence is checked by the
/// shared context projection before this predicate is called.
pub(crate) fn case_patient_visible_to_character_view(
    ctx: &ViewContext,
    character_id: u64,
    case_id: &str,
    minute: u64,
) -> bool {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
    else {
        return false;
    };
    ctx.db
        .local_problem_receipt()
        .character_id()
        .filter(character_id)
        .any(|receipt| {
            receipt.problem_id == authority.problem_id
                && receipt.settlement_id == authority.settlement_id
                && receipt.learned_at <= minute
        })
}

pub(crate) fn case_patient_visible_to_character(
    ctx: &ReducerContext,
    character_id: u64,
    case_id: &str,
    minute: u64,
) -> bool {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
    else {
        return false;
    };
    ctx.db
        .local_problem_receipt()
        .character_id()
        .filter(character_id)
        .any(|receipt| {
            receipt.problem_id == authority.problem_id
                && receipt.settlement_id == authority.settlement_id
                && receipt.learned_at <= minute
        })
}

fn materialize_patient_corpse(
    ctx: &ReducerContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    exposure: &adventuresim_core::quest_generation::OutbreakExposure,
    settlement_id: &str,
    death_minute: u64,
) -> Result<String, String> {
    use adventuresim_core::{
        autopsy::SystemicPathologySnapshot,
        disease::{InfectionEpisode, Symptom, combined_state},
        physiology::Meter,
        quest_generation::OutbreakPatientDeathKind,
    };
    let outbreak = generated
        .outbreak
        .as_ref()
        .ok_or("Outbreak truth missing")?;
    let cause = match exposure.death_kind {
        Some(OutbreakPatientDeathKind::CarrierAttack) => crate::character::DeathCause::Combat,
        Some(OutbreakPatientDeathKind::Disease) => crate::character::DeathCause::Disease,
        None => return Err("Living outbreak patient cannot materialize a corpse".into()),
    };
    let source = match exposure.death_kind {
        Some(OutbreakPatientDeathKind::CarrierAttack) => crate::character::DeathSource::Autoresolve,
        _ => crate::character::DeathSource::Disease,
    };
    crate::investigation::set_character_case_site(
        ctx,
        exposure.patient_character_id,
        Some(outbreak.patient_presentation_site.0.clone()),
    )?;
    let death_source_id = format!("outbreak-victim:{}", generated.canonical_case_id);
    if let Some(existing) = ctx
        .db
        .character_death()
        .character_id()
        .find(exposure.patient_character_id)
    {
        if existing.strategic_minute != death_minute
            || existing.source_id.as_deref() != Some(death_source_id.as_str())
        {
            return Err("Outbreak patient death provenance collision".into());
        }
    }
    crate::character::transition_character_to_dead_at(
        ctx,
        exposure.patient_character_id,
        cause,
        source,
        Some(death_source_id),
        death_minute,
    )?;
    let corpse_id = format!("corpse:character:{}", exposure.patient_character_id);
    let episode = InfectionEpisode {
        id: exposure.episode_id,
        character_id: exposure.patient_character_id,
        disease_id: outbreak.disease,
        contracted_at: exposure.exposed_at,
        ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
        phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
    };
    let (_, vitals, symptoms, _) = combined_state(
        &[episode],
        death_minute,
        ctx.db
            .character_attributes()
            .character_id()
            .find(exposure.patient_character_id)
            .map_or(3.0, |attributes| attributes.immunity),
    );
    let physiology_key = crate::disease::physiology_key(ctx)?;
    if physiology_key.version != adventuresim_core::physiology::PHENOTYPE_KEY_VERSION {
        return Err("Patient phenotype version does not match private key material".into());
    }
    let meters = adventuresim_core::disease::private_meter_state(
        episode,
        death_minute,
        ctx.db
            .character_attributes()
            .character_id()
            .find(exposure.patient_character_id)
            .map_or(3.0, |attributes| attributes.immunity),
        &physiology_key.key,
    );
    let bps = |value: f32| (value.clamp(0.0, 1.0) * 10_000.0).round() as u16;
    crate::corpse::persist_pathology_snapshot(
        ctx,
        &corpse_id,
        &SystemicPathologySnapshot {
            respiratory_bps: bps(vitals.phlegmatic.max(meters.get(Meter::Oxygenation))),
            circulatory_bps: bps(vitals.sanguine.max(meters.get(Meter::Perfusion))),
            homeostatic_bps: bps(vitals
                .choleric
                .max(meters.get(Meter::Hydration))
                .max(meters.get(Meter::Temperature))
                .max(meters.get(Meter::Inflammation))),
            neurologic_bps: bps(vitals.melancholic.max(meters.get(Meter::Neurologic))),
            feverish: symptoms.contains(&Symptom::Feverish),
            air_hunger: symptoms.contains(&Symptom::AirHunger),
            wasting: symptoms.contains(&Symptom::Wasting),
        },
    )?;
    let canonical_family = ctx
        .db
        .character_kinship()
        .subject_id()
        .filter(exposure.patient_character_id)
        .filter_map(|edge| {
            ctx.db
                .settlement_resident_profile()
                .character_id()
                .find(edge.related_id)
                .filter(|profile| profile.home_settlement_id == settlement_id)
                .map(|_| edge.related_id)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    crate::corpse::materialize_corpse_family_bindings(
        ctx,
        &corpse_id,
        settlement_id,
        &canonical_family,
    )?;
    Ok(corpse_id)
}

pub(crate) fn remediation_id(
    generated: &adventuresim_core::quest_generation::GeneratedCase,
) -> Result<String, String> {
    generated
        .objectives
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .find_map(|objective| match &objective.requirement {
            adventuresim_core::case::ObjectiveRequirement::RemediateSource { remediation_id } => {
                Some(remediation_id.clone())
            }
            _ => None,
        })
        .ok_or("Outbreak has no exact remediation objective".into())
}

fn outbreak_source_fixture(case_id: &str, site_id: &str) -> Result<StrategicFixtureId, String> {
    StrategicFixtureId::outbreak_source(
        StrategicPlaceId::case_site(site_id.to_owned())
            .map_err(|_| "Outbreak source case-site identity is malformed")?,
        case_id.to_owned(),
    )
    .map_err(|_| "Outbreak source fixture identity is malformed".into())
}

fn water_material_lot_id(case_id: &str) -> u64 {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(format!("outbreak-water-material-v1\0{case_id}").as_bytes());
    u64::from_le_bytes(digest[..8].try_into().unwrap()).max(1)
}

fn water_output_lot_id(request_id: &str, source_lot_id: u64, container_object_id: u64) -> u64 {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(b"water-output-lot-v1");
    hash.update(request_id.as_bytes());
    hash.update(source_lot_id.to_le_bytes());
    hash.update(container_object_id.to_le_bytes());
    let bytes: [u8; 32] = hash.finalize().into();
    u64::from_le_bytes(bytes[..8].try_into().expect("eight bytes")).max(1)
}

fn parse_outbreak_source_fixture(value: &str, case_id: &str) -> Option<StrategicFixtureId> {
    let fixture = StrategicFixtureId::from_str(value).ok()?;
    (fixture.outbreak_id() == Some(case_id)).then_some(fixture)
}

pub(crate) fn materialize_generated_outbreak(
    ctx: &ReducerContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    settlement_id: &str,
    now_minute: u64,
) -> Result<(), String> {
    use adventuresim_core::quest_generation::OutbreakSource;

    let Some(outbreak) = &generated.outbreak else {
        return Ok(());
    };
    if outbreak.exposure_chronology.len() > MAX_OUTBREAK_PATIENTS {
        return Err("Generated outbreak exceeds bounded patient materialization".into());
    }
    let exact_remediation_id = remediation_id(generated)?;
    let (source_kind, carrier) = match &outbreak.source {
        OutbreakSource::Sanitation { .. } => ("sanitation", None),
        OutbreakSource::Behavior { .. } => ("behavior", None),
        OutbreakSource::ThreatVector { threat } => ("threat_vector", Some(threat.as_str())),
        OutbreakSource::Environmental { .. } => ("environmental", None),
    };
    let responsible = outbreak.responsible_npc.as_ref();
    let physical_source_fixture = outbreak_source_fixture(
        &generated.canonical_case_id,
        &outbreak.physical_source_site.0,
    )?;
    let patient_presentation_place =
        StrategicPlaceId::case_site(outbreak.patient_presentation_site.0.clone())
            .map_err(|_| "Outbreak patient case-site identity is malformed")?;
    let disease_id = crate::disease::disease_key(outbreak.disease).to_string();
    let transmission_route = format!("{:?}", outbreak.transmission_route).to_ascii_lowercase();
    let source_json =
        serde_json::to_string(&outbreak.source).map_err(|_| "Could not encode outbreak source")?;
    let responsible_resident_character_id =
        responsible.map(|value| value.resident_character_id.clone());
    let culpability =
        responsible.map(|value| format!("{:?}", value.culpability).to_ascii_lowercase());
    let carrier_threat_id = carrier.map(str::to_owned);
    let chronology_json = serde_json::to_string(&outbreak.exposure_chronology)
        .map_err(|_| "Could not encode outbreak chronology")?;
    let remediation_json = serde_json::to_string(&outbreak.remediation)
        .map_err(|_| "Could not encode outbreak remediation")?;
    if let Some(existing) = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&generated.canonical_case_id)
    {
        let exact_patients = outbreak.exposure_chronology.iter().all(|exposure| {
            ctx.db
                .outbreak_patient_authority()
                .id()
                .find(&exposure.patient_ref)
                .is_some_and(|patient| {
                    patient.case_id == generated.canonical_case_id
                        && patient.patient_character_id == exposure.patient_character_id
                        && patient.episode_id == exposure.episode_id
                })
                && ctx
                    .db
                    .infection_episode()
                    .id()
                    .find(exposure.episode_id)
                    .is_some_and(|episode| {
                        episode.character_id == exposure.patient_character_id
                            && episode.contracted_at == exposure.exposed_at
                    })
        });
        let expects_water = outbreak.disease == adventuresim_core::disease::DiseaseId::Dysentery
            && matches!(
            outbreak.source,
            adventuresim_core::quest_generation::OutbreakSource::Sanitation {
                practice: adventuresim_core::quest_generation::OutbreakSanitationPractice::ContaminatedWell
            }
        );
        let material_lot_id = water_material_lot_id(&generated.canonical_case_id);
        let exact_water = ctx
            .db
            .outbreak_water_source()
            .fixture_id()
            .find(&physical_source_fixture.to_string())
            .is_some_and(|source| {
                source.material_lot_id == material_lot_id
                    && ctx
                        .db
                        .water_material_lot()
                        .id()
                        .find(&source.material_lot_id)
                        .is_some_and(|lot| {
                            lot.source_fixture_id == source.fixture_id
                                && lot.outbreak_case_id == generated.canonical_case_id
                                && lot.liquid_item_id == crate::inventory_container::WATER_ITEM_ID
                        })
            });
        return if existing.problem_id == generated.problem_id
            && existing.settlement_id == settlement_id
            && existing.disease_id == disease_id
            && existing.transmission_route == transmission_route
            && existing.source_kind == source_kind
            && existing.source_json == source_json
            && existing.physical_source_fixture_id == physical_source_fixture.to_string()
            && existing.patient_presentation_place_id == patient_presentation_place.to_string()
            && existing.responsible_resident_character_id == responsible_resident_character_id
            && existing.culpability == culpability
            && existing.carrier_threat_id == carrier_threat_id
            && existing.chronology_json == chronology_json
            && existing.remediation_id == exact_remediation_id
            && existing.remediation_json == remediation_json
            && exact_patients
            && exact_water == expects_water
        {
            Ok(())
        } else {
            Err("Generated outbreak provenance collision".into())
        };
    }
    if ctx
        .db
        .outbreak_authority()
        .problem_id()
        .find(&generated.problem_id)
        .is_some()
    {
        return Err("Generated outbreak authority ID collision".into());
    }
    ctx.db.outbreak_authority().insert(OutbreakAuthority {
        case_id: generated.canonical_case_id.clone(),
        problem_id: generated.problem_id.clone(),
        settlement_id: settlement_id.into(),
        disease_id,
        transmission_route,
        source_kind: source_kind.into(),
        source_json,
        physical_source_fixture_id: physical_source_fixture.to_string(),
        patient_presentation_place_id: patient_presentation_place.to_string(),
        responsible_resident_character_id,
        culpability,
        carrier_threat_id,
        chronology_json,
        remediation_id: exact_remediation_id,
        remediation_json,
        remediated_at: None,
        remediated_by_party_id: None,
        remediation_source_id: None,
    });

    if outbreak.disease == adventuresim_core::disease::DiseaseId::Dysentery
        && matches!(
        outbreak.source,
        adventuresim_core::quest_generation::OutbreakSource::Sanitation {
            practice:
                adventuresim_core::quest_generation::OutbreakSanitationPractice::ContaminatedWell
        }
    ) {
        let fixture_id = physical_source_fixture.to_string();
        let material_lot_id = water_material_lot_id(&generated.canonical_case_id);
        ctx.db.water_material_lot().insert(WaterMaterialLot {
            id: material_lot_id.clone(),
            source_fixture_id: fixture_id.clone(),
            outbreak_case_id: generated.canonical_case_id.clone(),
            liquid_item_id: crate::inventory_container::WATER_ITEM_ID.into(),
            concentration_anchor: 12.0,
            growth_per_hour: 0.0,
            anchor_minute: now_minute,
        });
        ctx.db.outbreak_water_source().insert(OutbreakWaterSource {
            fixture_id,
            material_lot_id,
            available_ml: 500_000,
            revision: 1,
            disabled_at: None,
        });
    }

    for exposure in &outbreak.exposure_chronology {
        if ctx
            .db
            .outbreak_patient_authority()
            .id()
            .find(&exposure.patient_ref)
            .is_some()
            || ctx
                .db
                .outbreak_patient_authority()
                .episode_id()
                .find(exposure.episode_id)
                .is_some()
        {
            return Err(format!(
                "Generated outbreak patient ID collision: {}",
                exposure.patient_ref
            ));
        }
        let npc = crate::settlement_population::resolve_settlement_resident(
            ctx,
            exposure.patient_character_id,
        )
        .ok_or("Outbreak patient NPC no longer exists")?;
        if npc.home_settlement_id != settlement_id {
            return Err("Outbreak presentation NPC is not local to its patient".into());
        }
        let mut patient_time = ctx
            .db
            .character_time()
            .character_id()
            .find(exposure.patient_character_id)
            .ok_or("Outbreak patient Character has no ordinary clock")?;
        if patient_time.minutes < now_minute {
            patient_time.minutes = now_minute;
            ctx.db.character_time().character_id().update(patient_time);
            crate::time::settle_lifecycle_after_character_time_write(
                ctx,
                exposure.patient_character_id,
                now_minute,
            )?;
        }
        let episode = adventuresim_core::disease::InfectionEpisode {
            id: exposure.episode_id,
            character_id: exposure.patient_character_id,
            disease_id: outbreak.disease,
            contracted_at: exposure.exposed_at,
            ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
        };
        let immunity = ctx
            .db
            .character_attributes()
            .character_id()
            .find(exposure.patient_character_id)
            .ok_or("Outbreak patient Character has no ordinary attributes")?
            .immunity;
        if let Some(existing) = ctx.db.infection_episode().id().find(exposure.episode_id) {
            if existing.character_id != exposure.patient_character_id
                || existing.disease_id != crate::disease::disease_key(outbreak.disease)
                || existing.contracted_at != exposure.exposed_at
                || existing.ruleset_version
                    != adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION
                || existing.phenotype_key_version
                    != adventuresim_core::physiology::PHENOTYPE_KEY_VERSION
            {
                return Err("Outbreak infection provenance collision".into());
            }
        } else {
            ctx.db
                .infection_episode()
                .insert(crate::disease::InfectionEpisodeRow {
                    id: exposure.episode_id,
                    character_id: exposure.patient_character_id,
                    disease_id: crate::disease::disease_key(outbreak.disease).into(),
                    contracted_at: exposure.exposed_at,
                    ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
                    phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
                });
        }
        let definition = adventuresim_core::disease::definition(outbreak.disease);
        let course_end = exposure
            .exposed_at
            .saturating_add(definition.incubation_minutes)
            .saturating_add(definition.rise_minutes)
            .saturating_add(definition.peak_minutes)
            .saturating_add(definition.recovery_minutes);
        let private_terminal = crate::disease::first_private_terminal(
            ctx,
            exposure.patient_character_id,
            &[episode],
            exposure.exposed_at,
            course_end,
            immunity,
        )?;
        let mut resolved_exposure = exposure.clone();
        match exposure.death_kind {
            Some(adventuresim_core::quest_generation::OutbreakPatientDeathKind::Disease) => {
                resolved_exposure.died_at = private_terminal.map(|value| value.0);
                resolved_exposure.death_kind = private_terminal.map(|_| {
                    adventuresim_core::quest_generation::OutbreakPatientDeathKind::Disease
                });
            }
            Some(adventuresim_core::quest_generation::OutbreakPatientDeathKind::CarrierAttack) => {
                let latest_attack = private_terminal
                    .map(|(terminal_at, _)| terminal_at.saturating_sub(1))
                    .unwrap_or(now_minute)
                    .min(now_minute);
                let attack_at = exposure
                    .died_at
                    .unwrap_or(latest_attack)
                    .min(latest_attack)
                    .max(exposure.became_symptomatic_at);
                if attack_at <= latest_attack {
                    resolved_exposure.died_at = Some(attack_at);
                } else {
                    resolved_exposure.died_at = None;
                    resolved_exposure.death_kind = None;
                }
            }
            None => {}
        }
        let row_id = resolved_exposure.patient_ref.clone();
        let corpse_id = resolved_exposure
            .died_at
            .filter(|death_minute| *death_minute <= now_minute)
            .map(|death_minute| {
                materialize_patient_corpse(
                    ctx,
                    generated,
                    &resolved_exposure,
                    settlement_id,
                    death_minute,
                )
            })
            .transpose()?;
        let autopsy_evidence_id = corpse_id.as_ref().and_then(|_| {
            generated
                .evidence
                .iter()
                .find(|evidence| {
                    evidence.kind
                        == adventuresim_core::quest_generation::EvidenceKind::BloodlessCorpse
                })
                .map(|evidence| evidence.id.0.clone())
        });
        let membership_id = format!(
            "context:{}:patient:{}",
            generated.canonical_case_id, exposure.patient_character_id
        );
        let patient_active = resolved_exposure.died_at.is_none() && now_minute < course_end;
        let membership = crate::world_actor::CharacterContextMembership {
            id: membership_id.clone(),
            context_id: generated.canonical_case_id.clone(),
            location_id: outbreak.patient_presentation_site.0.clone(),
            character_id: exposure.patient_character_id,
            context_kind: crate::world_actor::CharacterContextKind::CaseSite,
            role: crate::world_actor::CharacterContextRole::Patient,
            ordinal: u16::try_from(
                outbreak
                    .exposure_chronology
                    .iter()
                    .position(|candidate| candidate.patient_ref == exposure.patient_ref)
                    .ok_or("Outbreak patient lost its authored ordinal")?,
            )
            .map_err(|_| "Outbreak patient ordinal exceeds its bounded roster")?,
            active: patient_active,
            entered_at: exposure.became_symptomatic_at,
            left_at: (!patient_active).then_some(
                resolved_exposure
                    .died_at
                    .unwrap_or(course_end)
                    .min(course_end),
            ),
            revision: 1,
            contact_decision: crate::world_actor::ContextualDecisionState::Allowed,
            treatment_decision: crate::world_actor::ContextualDecisionState::Allowed,
        };
        if ctx
            .db
            .character_context_membership()
            .character_id()
            .filter(exposure.patient_character_id)
            .any(|existing| {
                existing.active
                    && existing.role == crate::world_actor::CharacterContextRole::Patient
                    && existing.context_id != generated.canonical_case_id
            })
        {
            return Err(
                "Canonical Character is already an active Patient in another context".into(),
            );
        }
        if let Some(existing) = ctx
            .db
            .character_context_membership()
            .id()
            .find(&membership_id)
        {
            if existing.context_id != membership.context_id
                || existing.location_id != membership.location_id
                || existing.character_id != membership.character_id
                || existing.context_kind != membership.context_kind
                || existing.role != membership.role
                || existing.ordinal != membership.ordinal
                || existing.entered_at != membership.entered_at
                || existing.contact_decision != membership.contact_decision
                || existing.treatment_decision != membership.treatment_decision
            {
                return Err("Outbreak Patient context provenance collision".into());
            }
        } else {
            ctx.db.character_context_membership().insert(membership);
        }
        if let Some(mut presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(exposure.patient_character_id)
        {
            presence.context_suppressed = patient_active;
            presence.health_suppressed = patient_active
                || ctx
                    .db
                    .character()
                    .id()
                    .find(exposure.patient_character_id)
                    .is_none_or(|character| !character.alive);
            ctx.db
                .settlement_resident_presence()
                .character_id()
                .update(presence);
        }
        ctx.db
            .outbreak_patient_authority()
            .insert(OutbreakPatientAuthority {
                id: row_id,
                case_id: generated.canonical_case_id.clone(),
                patient_character_id: exposure.patient_character_id,
                episode_id: exposure.episode_id,
                context_active: patient_active,
                health_active: patient_active,
                corpse_id,
                autopsy_evidence_id,
            });
    }
    Ok(())
}

pub(crate) fn commit_source_remediation(
    ctx: &ReducerContext,
    case_id: &str,
    party_id: &str,
    source_id: &str,
    remediation_id: &str,
    source_site_id: &str,
    at_minute: u64,
) -> Result<(), String> {
    let mut authority = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
        .ok_or("Outbreak authority not found")?;
    let source_fixture = outbreak_source_fixture(case_id, source_site_id)?;
    if authority.remediation_id != remediation_id
        || authority.physical_source_fixture_id != source_fixture.to_string()
    {
        return Err("Intervention does not match the authoritative outbreak source".into());
    }
    if authority.source_kind == "threat_vector" {
        return Err("A carrier outbreak must be remediated through its hostile outcome".into());
    }
    let outbreak_source: adventuresim_core::quest_generation::OutbreakSource =
        serde_json::from_str(&authority.source_json)
            .map_err(|_| "Outbreak source authority is malformed")?;
    let expects_water = authority.disease_id == "dysentery"
        && matches!(
        outbreak_source,
        adventuresim_core::quest_generation::OutbreakSource::Sanitation {
            practice:
                adventuresim_core::quest_generation::OutbreakSanitationPractice::ContaminatedWell
        }
    );
    let mut water_source = ctx
        .db
        .outbreak_water_source()
        .fixture_id()
        .find(&authority.physical_source_fixture_id);
    if expects_water {
        let source = water_source
            .as_ref()
            .ok_or("Outbreak water source authority is missing")?;
        let lot = ctx
            .db
            .water_material_lot()
            .id()
            .find(source.material_lot_id)
            .ok_or("Outbreak water material authority is missing")?;
        if lot.source_fixture_id != authority.physical_source_fixture_id
            || lot.outbreak_case_id != authority.case_id
        {
            return Err("Outbreak water material authority conflicts".into());
        }
    } else if water_source.is_some() {
        return Err("Outbreak source has unexpected water authority".into());
    }
    if authority.remediated_at.is_some() {
        return if authority.remediation_source_id.as_deref() == Some(source_id)
            && authority.remediated_by_party_id.as_deref() == Some(party_id)
            && authority.remediated_at == Some(at_minute)
            && water_source
                .as_ref()
                .is_none_or(|source| source.disabled_at == authority.remediated_at)
        {
            Ok(())
        } else {
            Err("Outbreak source was already remediated by different authority".into())
        };
    }
    if water_source
        .as_ref()
        .is_some_and(|source| source.disabled_at.is_some())
    {
        return Err("Outbreak water source state conflicts with remediation".into());
    }
    authority.remediated_at = Some(at_minute);
    authority.remediated_by_party_id = Some(party_id.into());
    authority.remediation_source_id = Some(source_id.into());
    ctx.db.outbreak_authority().case_id().update(authority);
    if let Some(mut source) = water_source.take() {
        source.disabled_at = Some(at_minute);
        ctx.db.outbreak_water_source().fixture_id().update(source);
    }
    deactivate_outbreak_patient_contexts(ctx, case_id, at_minute);
    Ok(())
}

fn deactivate_outbreak_patient_contexts(ctx: &ReducerContext, case_id: &str, at_minute: u64) {
    crate::world_actor::deactivate_context_roster_at(ctx, case_id, at_minute);
    for mut patient in ctx
        .db
        .outbreak_patient_authority()
        .case_id()
        .filter(&case_id.to_string())
        .collect::<Vec<_>>()
    {
        patient.context_active = false;
        if let Some(mut presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(patient.patient_character_id)
        {
            presence.context_suppressed = false;
            ctx.db
                .settlement_resident_presence()
                .character_id()
                .update(presence);
        }
        ctx.db.outbreak_patient_authority().id().update(patient);
    }
}

/// Reconstructs settlement-presence suppression at an observer-relative
/// historical minute. Mutable materialized patient flags describe only the
/// latest world state and must never be read for a lagging observer.
pub(crate) fn patient_presence_suppression_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Option<adventuresim_core::strategic_presence::PresenceSuppression> {
    let alive_at_observer = crate::relationship::character_alive_at(ctx, character_id, minute);
    let mut aggregate = adventuresim_core::strategic_presence::PresenceSuppression {
        context_suppressed: false,
        health_suppressed: !alive_at_observer,
    };
    for patient in ctx
        .db
        .outbreak_patient_authority()
        .patient_character_id()
        .filter(character_id)
    {
        let episode = ctx.db.infection_episode().id().find(patient.episode_id)?;
        let authority = ctx
            .db
            .outbreak_authority()
            .case_id()
            .find(&patient.case_id)?;
        if episode.character_id != character_id || episode.disease_id != authority.disease_id {
            return None;
        }
        let disease_id = crate::disease::parse_id(&episode.disease_id).ok()?;
        let definition = adventuresim_core::disease::definition(disease_id);
        let recovery_minute = episode
            .contracted_at
            .checked_add(definition.incubation_minutes)?
            .checked_add(definition.rise_minutes)?
            .checked_add(definition.peak_minutes)?
            .checked_add(definition.recovery_minutes)?;
        let suppression = adventuresim_core::strategic_presence::outbreak_patient_suppression_at(
            episode.contracted_at,
            recovery_minute,
            authority.remediated_at,
            minute,
            alive_at_observer,
        )
        .ok()?;
        aggregate.context_suppressed |= suppression.context_suppressed;
        aggregate.health_suppressed |= suppression.health_suppressed;
    }
    Some(aggregate)
}

#[cfg(test)]
mod water_integration_contract_tests {
    #[test]
    fn collection_input_is_observer_safe_and_replay_precedes_private_reads() {
        let source = include_str!("outbreak.rs");
        let reducer = source
            .split("pub fn collect_fixture_water_into_container")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn contained_water_contamination")
            .next()
            .unwrap();
        let signature = reducer.split(") -> Result<(), String>").next().unwrap();
        assert!(signature.contains("capability_id: String"));
        assert!(signature.contains("expected_capability_version: u32"));
        assert!(!signature.contains("source_fixture_id"));
        assert!(
            reducer.find("water_collection_receipt()").unwrap()
                < reducer.find("outbreak_water_source()").unwrap()
        );
    }

    #[test]
    fn private_truth_does_not_control_public_fixture_handling() {
        let container = include_str!("inventory_container.rs");
        let public_row = container
            .split("pub struct ContainerLiquid")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap();
        assert!(!public_row.contains("fixture_drawn"));
        assert!(!public_row.contains("material_lot"));
        assert!(!public_row.contains("contamin"));
    }
}

/// Release a recovered or dead patient from case-site presentation whenever
/// their ordinary Character clock advances past the standard episode course.
pub(crate) fn refresh_patient_context_after_time_write(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) {
    let alive = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|character| character.alive);
    let mut released_any = false;
    for mut patient in ctx
        .db
        .outbreak_patient_authority()
        .patient_character_id()
        .filter(character_id)
        .filter(|patient| patient.health_active)
        .collect::<Vec<_>>()
    {
        let recovered = ctx
            .db
            .infection_episode()
            .id()
            .find(patient.episode_id)
            .and_then(|episode| {
                crate::disease::parse_id(&episode.disease_id)
                    .ok()
                    .map(|id| (episode, id))
            })
            .is_some_and(|(episode, disease_id)| {
                let definition = adventuresim_core::disease::definition(disease_id);
                minute
                    >= episode
                        .contracted_at
                        .saturating_add(definition.incubation_minutes)
                        .saturating_add(definition.rise_minutes)
                        .saturating_add(definition.peak_minutes)
                        .saturating_add(definition.recovery_minutes)
            });
        if alive && !recovered {
            continue;
        }
        patient.context_active = false;
        patient.health_active = false;
        let membership_id = format!(
            "context:{}:patient:{}",
            patient.case_id, patient.patient_character_id
        );
        if let Some(mut membership) = ctx
            .db
            .character_context_membership()
            .id()
            .find(&membership_id)
        {
            membership.active = false;
            membership.left_at = Some(minute.max(membership.entered_at));
            membership.revision = membership.revision.saturating_add(1);
            ctx.db
                .character_context_membership()
                .id()
                .update(membership);
        }
        ctx.db.outbreak_patient_authority().id().update(patient);
        released_any = true;
    }
    if released_any
        && let Some(mut presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(character_id)
    {
        presence.context_suppressed = false;
        presence.health_suppressed = !alive;
        ctx.db
            .settlement_resident_presence()
            .character_id()
            .update(presence);
    }
}

pub(crate) fn commit_carrier_remediation(
    ctx: &ReducerContext,
    case_id: &str,
    party_id: &str,
    source_id: &str,
    remediation_id: &str,
    at_minute: u64,
) -> Result<(), String> {
    let mut authority = ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
        .ok_or("Outbreak authority not found")?;
    if authority.source_kind != "threat_vector" || authority.remediation_id != remediation_id {
        return Err("Hostile outcome does not match the authoritative carrier source".into());
    }
    if authority.remediated_at.is_some() {
        return if authority.remediation_source_id.as_deref() == Some(source_id)
            && authority.remediated_by_party_id.as_deref() == Some(party_id)
        {
            Ok(())
        } else {
            Err("Carrier source was already remediated by different authority".into())
        };
    }
    authority.remediated_at = Some(at_minute);
    authority.remediated_by_party_id = Some(party_id.into());
    authority.remediation_source_id = Some(source_id.into());
    ctx.db.outbreak_authority().case_id().update(authority);
    deactivate_outbreak_patient_contexts(ctx, case_id, at_minute);
    Ok(())
}

pub(crate) fn accepted_hostile_remediation(
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    fact: &adventuresim_core::case::OutcomeFactKind,
) -> Option<String> {
    use adventuresim_core::{
        case::OutcomeFactKind,
        quest_generation::{OutbreakCarrierOutcome, OutbreakRemediation},
    };
    let outbreak = generated.outbreak.as_ref()?;
    let OutbreakRemediation::ResolveCarrierThreat {
        hostile_group_id,
        accepted_outcomes,
    } = &outbreak.remediation
    else {
        return None;
    };
    let accepted = match fact {
        OutcomeFactKind::HostilesDefeated {
            hostile_group_id: actual,
            ..
        } if actual == hostile_group_id => {
            accepted_outcomes.contains(&OutbreakCarrierOutcome::Defeated)
        }
        OutcomeFactKind::HostilesDrivenOff {
            hostile_group_id: actual,
        } if actual == hostile_group_id => {
            accepted_outcomes.contains(&OutbreakCarrierOutcome::DrivenOff)
        }
        _ => false,
    };
    accepted.then(|| remediation_id(generated).ok()).flatten()
}

/// Scope generated disease pressure to the authority that physically produces
/// it. Community bedding/behavior affects settlement presence; reservoirs and
/// carriers require occupancy at the exact source site.
pub(crate) fn exposure_windows(
    ctx: &ReducerContext,
    problem_id: &str,
    character_id: u64,
    from: u64,
    to: u64,
) -> Vec<(String, u64, u64)> {
    let Some(authority) = ctx
        .db
        .outbreak_authority()
        .problem_id()
        .find(&problem_id.to_owned())
    else {
        return vec![(problem_id.to_owned(), from, to)];
    };
    let exposure_to = to.min(authority.remediated_at.unwrap_or(to));
    if exposure_to <= from {
        return Vec::new();
    }
    match authority.source_kind.as_str() {
        "sanitation" | "behavior" => {
            ctx.db
                .character()
                .id()
                .find(character_id)
                .map_or_else(Vec::new, |character| {
                    (character.current_settlement_id.as_deref()
                        == Some(authority.settlement_id.as_str()))
                    .then_some((problem_id.to_owned(), from, exposure_to))
                    .into_iter()
                    .collect()
                })
        }
        "environmental" | "threat_vector" => ctx
            .db
            .outbreak_source_presence_span()
            .character_id()
            .filter(character_id)
            .filter(|span| {
                parse_outbreak_source_fixture(
                    &authority.physical_source_fixture_id,
                    &authority.case_id,
                )
                .is_some_and(|fixture| fixture.place().to_string() == span.source_place_id)
            })
            .filter_map(|span| {
                let low = from.max(span.started_at);
                let high = exposure_to.min(span.ended_at.unwrap_or(exposure_to));
                (low < high).then_some((span.id, low, high))
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn record_case_site_presence_transition(
    ctx: &ReducerContext,
    character_id: u64,
    destination_site_id: Option<&str>,
) {
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |row| row.minutes);
    for mut span in ctx
        .db
        .outbreak_source_presence_span()
        .character_id()
        .filter(character_id)
        .filter(|span| span.ended_at.is_none())
        .collect::<Vec<_>>()
    {
        span.ended_at = Some(minute);
        ctx.db.outbreak_source_presence_span().id().update(span);
    }
    let Some(site_id) = destination_site_id else {
        return;
    };
    let Some(destination_place) = crate::investigation::canonical_case_site_place(site_id) else {
        return;
    };
    if !ctx.db.outbreak_authority().iter().any(|authority| {
        parse_outbreak_source_fixture(&authority.physical_source_fixture_id, &authority.case_id)
            .is_some_and(|fixture| {
                fixture.place() == &destination_place && authority.remediated_at.is_none()
            })
    }) {
        return;
    }
    let id = format!("outbreak-presence:{character_id}:{site_id}:{minute}");
    if ctx
        .db
        .outbreak_source_presence_span()
        .id()
        .find(&id)
        .is_none()
    {
        ctx.db
            .outbreak_source_presence_span()
            .insert(OutbreakSourcePresenceSpan {
                id,
                character_id,
                source_place_id: destination_place.to_string(),
                started_at: minute,
                ended_at: None,
            });
    }
}

pub(crate) fn discover_case_corpses(
    ctx: &ReducerContext,
    case_id: &str,
    character_id: u64,
    discovered_at: u64,
) -> Result<(), String> {
    if ctx
        .db
        .outbreak_authority()
        .case_id()
        .find(&case_id.to_owned())
        .is_none()
    {
        return Ok(());
    }
    let party_id = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id)
        .ok_or("Outbreak corpse discovery requires a party")?;
    for patient in ctx
        .db
        .outbreak_patient_authority()
        .case_id()
        .filter(&case_id.to_owned())
    {
        let Some(corpse_id) = patient.corpse_id else {
            continue;
        };
        let Some(mut corpse) = ctx.db.strategic_corpse().id().find(&corpse_id) else {
            return Err("Outbreak patient corpse authority is missing".into());
        };
        if corpse.discovering_party_id.is_empty() {
            corpse.discovering_party_id = party_id.clone();
            corpse.discovered_minute = discovered_at;
            ctx.db.strategic_corpse().id().update(corpse);
        } else if corpse.discovering_party_id != party_id {
            // Knowledge remains party-scoped; another party's discovery is not
            // silently transferred.
            continue;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn outbreak_authority_and_patients_are_private_and_real() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("#[table(accessor = outbreak_authority)]"));
        assert!(production.contains("#[table(accessor = outbreak_patient_authority)]"));
        assert!(!production.contains("accessor = outbreak_authority, public"));
        assert!(!production.contains("accessor = outbreak_patient_authority, public"));
        assert!(production.contains("infection_episode()"));
        assert!(production.contains("CharacterContextRole::Patient"));
        assert!(!production.contains("insert_character_with_origin"));
        assert!(production.contains("patient_character_id"));
        assert!(!production.contains("OutbreakPatientExamination"));
        assert!(!production.contains("examine_outbreak_patient"));
        assert!(!production.contains(&["settlement_outbreak()", ".insert"].concat()));
    }

    #[test]
    fn outbreak_corpses_use_ordinary_character_death_and_generic_pathology() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("transition_character_to_dead_at"));
        assert!(production.contains("corpse:character:"));
        assert!(production.contains("persist_pathology_snapshot"));
        assert!(production.contains("CharacterContextRole::Patient"));
        assert!(production.contains("context_suppressed"));
    }

    #[test]
    fn patient_context_visibility_requires_problem_knowledge() {
        let source = include_str!("outbreak.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("case_patient_visible_to_character_view"));
        assert!(production.contains("local_problem_receipt()"));
        assert!(production.contains("receipt.learned_at <= minute"));
    }

    #[test]
    fn patient_problem_knowledge_is_frontier_bounded() {
        let visible = |learned_at: u64, minute: u64| learned_at <= minute;
        assert!(!visible(101, 100));
        assert!(visible(100, 100));
    }

    #[test]
    fn remediation_is_exact_idempotent_and_uses_normal_outcome_authority() {
        let source = include_str!("outbreak.rs");
        assert!(source.contains("authority.remediation_id != remediation_id"));
        assert!(source.contains("physical_source_fixture_id != source_fixture.to_string()"));
        assert!(source.contains("StrategicFixtureId::outbreak_source"));
        assert!(source.contains("parse_outbreak_source_fixture"));
        assert!(source.contains("remediation_source_id.as_deref() == Some(source_id)"));
        let actions = include_str!("investigation/actions.rs");
        assert!(actions.contains("OutcomeFactKind::SourceRemediated"));
        let objectives = include_str!("strategic/custody_objectives.rs");
        assert!(objectives.contains("accepted_hostile_remediation"));
    }

    #[test]
    fn generated_outbreak_retry_checks_every_immutable_authority_field() {
        let source = include_str!("outbreak.rs");
        let retry = source
            .split("pub(crate) fn materialize_generated_outbreak")
            .nth(1)
            .and_then(|tail| tail.split("Generated outbreak provenance collision").next())
            .expect("generated outbreak retry comparison");
        for field in [
            "problem_id",
            "settlement_id",
            "disease_id",
            "transmission_route",
            "source_kind",
            "source_json",
            "physical_source_fixture_id",
            "patient_presentation_place_id",
            "responsible_resident_character_id",
            "culpability",
            "carrier_threat_id",
            "chronology_json",
            "remediation_id",
            "remediation_json",
        ] {
            assert!(retry.contains(field), "retry omitted {field}");
        }
    }

    #[test]
    fn remediation_releases_context_without_curing_and_family_is_canonical() {
        let source = include_str!("outbreak.rs");
        let deactivate = source
            .split("fn deactivate_outbreak_patient_contexts")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn refresh_patient_context_after_time_write")
                    .next()
            })
            .expect("context deactivation");
        assert!(deactivate.contains("patient.context_active = false"));
        assert!(!deactivate.contains("patient.health_active = false"));
        assert!(!deactivate.contains("health_suppressed = false"));
        assert!(source.contains("character_kinship()"));
        assert!(!source.contains("family_resident_character_id: Option"));
    }
}
