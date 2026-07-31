fn safe_correction_label(
    row: &InvestigationLead,
    correction: Option<&InvestigationLead>,
) -> String {
    if row.corrected_by.is_empty() {
        return String::new();
    }
    let Some(correction) = correction.filter(|correction| {
        correction.owner_character_id == row.owner_character_id && correction.case_id == row.case_id
    }) else {
        return "a later account".into();
    };
    if !correction.witness_name.is_empty() {
        correction.witness_name.clone()
    } else if !correction.source_label.is_empty() {
        correction.source_label.clone()
    } else {
        "a later account".into()
    }
}

fn safe_superseded_revision_label(
    row: &InvestigationBeliefRevision,
    superseded: Option<&InvestigationBeliefRevision>,
) -> String {
    if row.supersedes.is_empty() {
        return String::new();
    }
    let Some(superseded) = superseded.filter(|superseded| {
        superseded.owner_character_id == row.owner_character_id
            && superseded.belief_id == row.belief_id
    }) else {
        return "an earlier account".into();
    };
    if superseded.provenance_label.is_empty() {
        format!("revision {}", superseded.revision)
    } else {
        format!(
            "revision {} from {}",
            superseded.revision, superseded.provenance_label
        )
    }
}

fn sanitize_lead(
    row: InvestigationLead,
    correction: Option<&InvestigationLead>,
) -> BackendInvestigationLead {
    let exact = matches!(row.destination_stage.as_str(), "exact_believed" | "visited");
    let corrected_by = safe_correction_label(&row, correction);
    BackendInvestigationLead {
        owner_character_id: row.owner_character_id,
        case_id: row.case_id,
        lead_id: row.id,
        summary: row.summary,
        source_label: row.source_label,
        confidence_bps: row.confidence_bps,
        destination_stage: row.destination_stage,
        directions: row.directions,
        exact_location_id: if exact {
            row.exact_location_id
        } else {
            String::new()
        },
        latitude_e7: if exact { row.latitude_e7 } else { 0 },
        longitude_e7: if exact { row.longitude_e7 } else { 0 },
        witness_name: row.witness_name,
        witness_description: row.witness_description,
        witness_occupation_or_relationship: row.witness_occupation_or_relationship,
        expected_location: row.expected_location,
        current_learned_location: row.current_learned_location,
        contradiction_group: row.contradiction_group,
        corrected_by,
        recorded_at: row.recorded_at,
    }
}

fn bounded(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_TEXT || value.chars().any(char::is_control) {
        Err("Investigation text must be non-empty, bounded, and printable".into())
    } else {
        Ok(())
    }
}
fn bounded_optional(value: &str) -> Result<(), String> {
    if value.len() > MAX_TEXT || value.chars().any(char::is_control) {
        Err("Investigation text must be bounded and printable".into())
    } else {
        Ok(())
    }
}
fn bps(value: u16) -> Result<(), String> {
    (value <= 10_000)
        .then_some(())
        .ok_or_else(|| "Confidence must be at most 10000 basis points".into())
}
fn validate_destination(stage: &str, id: &str, lat: i32, lon: i32) -> Result<(), String> {
    let exact = matches!(stage, "exact_believed" | "visited");
    if exact
        && (id.is_empty()
            || !(-900_000_000..=900_000_000).contains(&lat)
            || !(-1_800_000_000..=1_800_000_000).contains(&lon))
    {
        return Err("Exact destination requires an id and valid E7 coordinates".into());
    }
    if !exact && (!id.is_empty() || lat != 0 || lon != 0) {
        return Err("Non-exact destination knowledge may not carry a pin".into());
    }
    Ok(())
}
fn official_minute(ctx: &ReducerContext) -> u64 {
    ctx.db
        .world_clock()
        .id()
        .find(0)
        .map_or(0, |clock| clock.official_minutes)
}
fn require_actor(ctx: &ReducerContext, actor_id: u64) -> Result<crate::Character, String> {
    require_strategic_gateway(ctx)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Character not found")?;
    if !actor.alive {
        return Err("Dead characters cannot update investigation knowledge".into());
    }
    Ok(actor)
}
fn canonical_payload(parts: &[&str]) -> Result<String, String> {
    let payload = inv::compound_id(parts);
    if payload.len() > 4_096 {
        Err("Canonical investigation payload is too large".into())
    } else {
        Ok(payload)
    }
}
fn idempotent(
    ctx: &ReducerContext,
    action_id: &str,
    actor_id: u64,
    kind: &str,
    payload: &str,
) -> Result<bool, String> {
    bounded(action_id)?;
    if let Some(existing) = ctx
        .db
        .investigation_action_receipt()
        .id()
        .find(action_id.to_string())
    {
        if existing.actor_id != actor_id
            || existing.action_kind != kind
            || existing.canonical_payload != payload
        {
            return Err("Investigation action id was reused with a different payload".into());
        }
        return Ok(true);
    }
    Ok(false)
}
fn record_action(
    ctx: &ReducerContext,
    action_id: String,
    actor_id: u64,
    kind: &str,
    payload: String,
) {
    ctx.db
        .investigation_action_receipt()
        .insert(InvestigationActionReceipt {
            id: action_id,
            actor_id,
            action_kind: kind.into(),
            canonical_payload: payload,
            applied_at: official_minute(ctx),
        });
}

fn witness_referral_id(character_id: u64, canonical_case_id: &str, witness_npc_id: &str) -> String {
    inv::compound_id(&[
        "generated-witness-referral",
        &character_id.to_string(),
        canonical_case_id,
        witness_npc_id,
    ])
}

fn witness_referral_context_matches(
    referral: &InvestigationWitnessReferral,
    character_id: u64,
    canonical_case_id: &str,
    witness_npc_id: &str,
    settlement_id: &str,
    location_id: &str,
) -> bool {
    referral.owner_character_id == character_id
        && referral.canonical_case_id == canonical_case_id
        && referral.witness_npc_id == witness_npc_id
        && referral.expected_settlement_id == settlement_id
        && referral.expected_location_id == location_id
}

fn validate_referral_manifest_provenance(
    referral: &InvestigationWitnessReferral,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    witness: &adventuresim_core::quest_generation::WitnessBinding,
) -> Result<(), String> {
    match referral.grant_kind.as_str() {
        "initial_rumor" => {
            if referral.source_receipt_id.is_empty()
                || !referral.source_witness_id.is_empty()
                || referral.source_testimony_index != 0
                || !referral.source_proposition_id.is_empty()
                || generated.witnesses.first() != Some(witness)
            {
                return Err("Initial witness referral provenance is malformed".into());
            }
        }
        "testimony" => {
            if referral.source_receipt_id.is_empty() {
                return Err("Testimony witness referral provenance is malformed".into());
            }
            let source = generated
                .witnesses
                .iter()
                .find(|candidate| {
                    candidate.id.0 == referral.source_witness_id
                        && candidate.npc_id == referral.source_witness_npc_id
                })
                .ok_or("Testimony witness referral source disappeared")?;
            let source_draft = source
                .testimony
                .get(referral.source_testimony_index as usize)
                .filter(|draft| draft.proposition_id == referral.source_proposition_id)
                .ok_or("Testimony witness referral draft disappeared")?;
            if source_draft
                .referred_witness_ids
                .iter()
                .filter(|target| **target == witness.id)
                .count()
                != 1
            {
                return Err("Testimony no longer authors one exact witness referral".into());
            }
        }
        _ => return Err("Unknown witness referral provenance".into()),
    }
    Ok(())
}

fn authored_witness_referrals<'a>(
    generated: &'a adventuresim_core::quest_generation::GeneratedCase,
    witness: &adventuresim_core::quest_generation::WitnessBinding,
    draft: &adventuresim_core::quest_generation::TestimonyDraft,
) -> Result<Vec<&'a adventuresim_core::quest_generation::WitnessBinding>, String> {
    if !generated
        .witnesses
        .iter()
        .any(|authoritative| authoritative == witness)
    {
        return Err("Generated testimony witness is absent from the authoritative manifest".into());
    }
    draft
        .referred_witness_ids
        .iter()
        .map(|referred_id| {
            generated
                .witnesses
                .iter()
                .find(|candidate| candidate.id == *referred_id)
                .ok_or_else(|| "Authored testimony referral witness disappeared".into())
        })
        .collect()
}

enum WitnessReferralProvenance<'a> {
    InitialRumor(&'a crate::local_problem::LocalProblemReceipt),
    Testimony {
        source_witness: &'a adventuresim_core::quest_generation::WitnessBinding,
        testimony_index: usize,
        source_draft: &'a adventuresim_core::quest_generation::TestimonyDraft,
        source_receipt_id: &'a str,
    },
}

fn grant_generated_witness_referral(
    ctx: &ReducerContext,
    character_id: u64,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    witness: &adventuresim_core::quest_generation::WitnessBinding,
    expected_settlement_id: &str,
    provenance: WitnessReferralProvenance<'_>,
) -> Result<(), String> {
    if !generated
        .witnesses
        .iter()
        .any(|authoritative| authoritative == witness)
    {
        return Err("Generated witness referral is absent from its manifest".into());
    }
    let (
        grant_kind,
        source_receipt_id,
        source_witness_id,
        source_witness_npc_id,
        source_testimony_index,
        source_proposition_id,
    ) = match provenance {
        WitnessReferralProvenance::InitialRumor(receipt) => {
            if generated.witnesses.first() != Some(witness)
                || receipt.character_id != character_id
                || receipt.opaque_case_ref != generated.canonical_case_id
                || receipt.problem_id != generated.problem_id
                || receipt.settlement_id != expected_settlement_id
                || receipt.contact_npc_id != witness.npc_id
                || receipt.expected_location_id != witness.expected_location
            {
                return Err("Initial rumor does not grant the exact primary witness".into());
            }
            (
                "initial_rumor".to_owned(),
                receipt.id.clone(),
                String::new(),
                receipt.source_npc_id.clone(),
                0,
                String::new(),
            )
        }
        WitnessReferralProvenance::Testimony {
            source_witness,
            testimony_index,
            source_draft,
            source_receipt_id,
        } => {
            if generated
                .witnesses
                .iter()
                .filter(|candidate| *candidate == source_witness)
                .count()
                != 1
                || source_witness.testimony.get(testimony_index) != Some(source_draft)
                || !source_draft.referred_witness_ids.contains(&witness.id)
            {
                return Err("Testimony does not author the exact witness referral".into());
            }
            let receipt = ctx
                .db
                .investigation_safe_claim_receipt()
                .id()
                .find(source_receipt_id.to_owned())
                .ok_or("Testimony witness referral receipt is missing")?;
            if receipt.owner_character_id != character_id
                || receipt.public_case_id != generated.public_case_id
                || receipt.proposition_id != source_draft.proposition_id
                || receipt.consumed_by.is_empty()
            {
                return Err("Testimony witness referral receipt is inconsistent".into());
            }
            (
                "testimony".to_owned(),
                source_receipt_id.to_owned(),
                source_witness.id.0.clone(),
                source_witness.npc_id.clone(),
                u32::try_from(testimony_index)
                    .map_err(|_| "Testimony referral index is too large")?,
                source_draft.proposition_id.clone(),
            )
        }
    };
    let id = witness_referral_id(character_id, &generated.canonical_case_id, &witness.npc_id);
    let row = InvestigationWitnessReferral {
        id: id.clone(),
        owner_character_id: character_id,
        canonical_case_id: generated.canonical_case_id.clone(),
        public_case_id: generated.public_case_id.clone(),
        witness_npc_id: witness.npc_id.clone(),
        expected_settlement_id: expected_settlement_id.into(),
        expected_location_id: witness.expected_location.clone(),
        grant_kind,
        source_receipt_id,
        source_witness_id,
        source_witness_npc_id,
        source_testimony_index,
        source_proposition_id,
        catalog_revision: generated.catalog_revision.clone(),
        granted_at: official_minute(ctx),
    };
    if let Some(existing) = ctx.db.investigation_witness_referral().id().find(&id) {
        return if existing.owner_character_id == row.owner_character_id
            && existing.canonical_case_id == row.canonical_case_id
            && existing.public_case_id == row.public_case_id
            && existing.witness_npc_id == row.witness_npc_id
            && existing.expected_settlement_id == row.expected_settlement_id
            && existing.expected_location_id == row.expected_location_id
            && existing.grant_kind == row.grant_kind
            && existing.source_receipt_id == row.source_receipt_id
            && existing.source_witness_id == row.source_witness_id
            && existing.source_witness_npc_id == row.source_witness_npc_id
            && existing.source_testimony_index == row.source_testimony_index
            && existing.source_proposition_id == row.source_proposition_id
            && existing.catalog_revision == row.catalog_revision
        {
            Ok(())
        } else {
            Err("Generated witness referral conflicts with existing authority".into())
        };
    }
    ctx.db.investigation_witness_referral().insert(row);
    let npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&witness.npc_id)
        .ok_or("Referred generated witness is no longer persistent")?;
    let lead_id = inv::compound_id(&["lead", "generated-witness-referral", &id]);
    if ctx.db.investigation_lead().id().find(&lead_id).is_none() {
        let location =
            adventuresim_core::quest_generation::referral_display_location(witness).to_owned();
        ctx.db.investigation_lead().insert(InvestigationLead {
            id: lead_id,
            owner_character_id: character_id,
            case_id: generated.public_case_id.clone(),
            proposition_id: String::new(),
            summary: format!(
                "Ask {}—{}, usually found at the {}.",
                npc.name, witness.visible_description, location
            ),
            source_label: "witness referral".into(),
            confidence_bps: 10_000,
            destination_stage: "textual".into(),
            directions: location.clone(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: npc.name,
            witness_description: witness.visible_description.clone(),
            witness_occupation_or_relationship: npc.profession,
            expected_location: location,
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: official_minute(ctx),
        });
    }
    Ok(())
}

pub(crate) fn referred_generated_witness(
    ctx: &ReducerContext,
    character_id: u64,
    canonical_case_id: &str,
    witness_npc_id: &str,
    settlement_id: &str,
    location_id: &str,
) -> Result<
    Option<(
        adventuresim_core::quest_generation::GeneratedCase,
        adventuresim_core::quest_generation::WitnessBinding,
    )>,
    String,
> {
    let id = witness_referral_id(character_id, canonical_case_id, witness_npc_id);
    let Some(referral) = ctx.db.investigation_witness_referral().id().find(&id) else {
        return Ok(None);
    };
    if !witness_referral_context_matches(
        &referral,
        character_id,
        canonical_case_id,
        witness_npc_id,
        settlement_id,
        location_id,
    ) {
        return Ok(None);
    }
    let authority = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&referral.canonical_case_id)
        .ok_or("Witness referral generation authority disappeared")?;
    let validated = validate_quest_generation_authority(&authority)?;
    if validated.manifest.public_case_id != referral.public_case_id
        || validated.manifest.catalog_revision != referral.catalog_revision
        || validated.context.settlement_id != referral.expected_settlement_id
    {
        return Err("Witness referral no longer matches generated authority".into());
    }
    let witness = validated
        .manifest
        .witnesses
        .iter()
        .find(|witness| {
            witness.npc_id == referral.witness_npc_id
                && witness.expected_location == referral.expected_location_id
        })
        .cloned()
        .ok_or("Witness referral is absent from generated authority")?;
    validate_referral_manifest_provenance(&referral, &validated.manifest, &witness)?;
    match referral.grant_kind.as_str() {
        "initial_rumor" => {
            let receipt = ctx
                .db
                .local_problem_receipt()
                .id()
                .find(&referral.source_receipt_id)
                .ok_or("Initial witness referral receipt disappeared")?;
            if receipt.character_id != character_id
                || receipt.opaque_case_ref != referral.canonical_case_id
                || receipt.problem_id != validated.manifest.problem_id
                || receipt.settlement_id != referral.expected_settlement_id
                || receipt.contact_npc_id != referral.witness_npc_id
                || receipt.expected_location_id != referral.expected_location_id
                || receipt.source_npc_id != referral.source_witness_npc_id
            {
                return Err("Initial witness referral no longer matches its receipt".into());
            }
        }
        "testimony" => {
            let receipt = ctx
                .db
                .investigation_safe_claim_receipt()
                .id()
                .find(&referral.source_receipt_id)
                .ok_or("Testimony witness referral receipt disappeared")?;
            if receipt.owner_character_id != character_id
                || receipt.public_case_id != referral.public_case_id
                || receipt.proposition_id != referral.source_proposition_id
                || receipt.consumed_by.is_empty()
            {
                return Err("Testimony witness referral no longer matches its receipt".into());
            }
        }
        _ => unreachable!("validated referral kind"),
    }
    Ok(Some((validated.manifest, witness)))
}

/// A known outbreak's patients and explicit family/carers may speak about
/// their authored testimony without first being unlocked by another witness.
/// The observer must still possess the ordinary rumor receipt, and the NPC
/// must be an exact generated witness at their authoritative location.
pub(crate) fn known_outbreak_witness(
    ctx: &ReducerContext,
    character_id: u64,
    witness_npc_id: &str,
    settlement_id: &str,
    location_id: &str,
) -> Result<
    Option<(
        adventuresim_core::quest_generation::GeneratedCase,
        adventuresim_core::quest_generation::WitnessBinding,
    )>,
    String,
> {
    let mut matches = Vec::new();
    for receipt in ctx
        .db
        .local_problem_receipt()
        .character_id()
        .filter(character_id)
        .filter(|receipt| receipt.settlement_id == settlement_id)
    {
        let authority = ctx
            .db
            .quest_generation_authority()
            .case_id()
            .find(&receipt.opaque_case_ref)
            .ok_or("Known outbreak receipt lost its generation authority")?;
        let validated = validate_quest_generation_authority(&authority)?;
        let generated = validated.manifest;
        if generated.problem_id != receipt.problem_id {
            return Err("Known outbreak receipt no longer matches its problem".into());
        }
        let Some(outbreak) = generated.outbreak.as_ref() else {
            continue;
        };
        let has_explicit_relationship = outbreak.exposure_chronology.iter().any(|patient| {
            patient.presentation_npc_id == witness_npc_id
                || patient.family_npc_id.as_deref() == Some(witness_npc_id)
        });
        if !has_explicit_relationship {
            continue;
        }
        let Some(witness) = generated
            .witnesses
            .iter()
            .find(|witness| {
                witness.npc_id == witness_npc_id
                    && witness.expected_location == location_id
                    && validated.context.settlement_id == settlement_id
            })
            .cloned()
        else {
            return Err("Known outbreak patient or carer has no authored testimony".into());
        };
        matches.push((generated, witness));
        if matches.len() > 1 {
            return Err("This NPC is connected to more than one known outbreak".into());
        }
    }
    Ok(matches.pop())
}

/// Converts #182's private safe receipt to owner knowledge without consulting
/// or exposing the local problem's hidden cause.
#[reducer]
pub fn receive_local_problem_rumor(
    ctx: &ReducerContext,
    character_id: u64,
    receipt_id: String,
    action_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    bounded(&receipt_id)?;
    let payload = canonical_payload(&[&receipt_id])?;
    if idempotent(ctx, &action_id, character_id, "receive_rumor", &payload)? {
        return Ok(());
    }
    let receipt = ctx
        .db
        .local_problem_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Rumor receipt not found")?;
    if receipt.character_id != character_id {
        return Err("Rumor receipt belongs to another observer".into());
    }
    let contact = ctx.db.settlement_npc().id().find(&receipt.contact_npc_id);
    let visible_description = contact.as_ref().map_or_else(String::new, |npc| {
        format!(
            "{}, {}, {}, with {}; {}",
            npc.height, npc.build, npc.complexion, npc.hair, npc.visible_features
        )
    });
    // Never expose the private opaque case seam. This observer-facing stable ID
    // derives only from the already-public problem identifier.
    let canonical_case_id = receipt.opaque_case_ref.clone();
    let generation = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&canonical_case_id)
        .ok_or("Rumor is not linked to a real generated case")?;
    let generated = validate_quest_generation_authority(&generation)?.manifest;
    let referred_witness = generated
        .witnesses
        .iter()
        .find(|witness| {
            witness.npc_id == receipt.contact_npc_id
                && witness.expected_location == receipt.expected_location_id
        })
        .ok_or("Generated rumor referral has no authoritative witness")?;
    let referral_location_label = Some(referred_witness)
        .map(adventuresim_core::quest_generation::referral_display_location)
        .map(str::to_owned)
        .filter(|label| !label.is_empty())
        .ok_or("Generated rumor referral has no player-visible tab label")?;
    grant_generated_witness_referral(
        ctx,
        character_id,
        &generated,
        referred_witness,
        &receipt.settlement_id,
        WitnessReferralProvenance::InitialRumor(&receipt),
    )?;
    let case_id = generated.public_case_id;
    let lead_id = inv::compound_id(&["lead", "rumor", &receipt.id]);
    if ctx.db.investigation_lead().id().find(&lead_id).is_none() {
        ctx.db.investigation_lead().insert(InvestigationLead {
            id: lead_id.clone(),
            owner_character_id: character_id,
            case_id: case_id.clone(),
            proposition_id: String::new(),
            summary: receipt.safe_summary.clone(),
            source_label: "local rumor".into(),
            confidence_bps: 5_000,
            destination_stage: if receipt.expected_location_id.is_empty() {
                "unknown"
            } else {
                "textual"
            }
            .into(),
            directions: referral_location_label.clone(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: contact
                .as_ref()
                .map_or_else(String::new, |npc| npc.name.clone()),
            witness_description: visible_description,
            witness_occupation_or_relationship: contact
                .map_or_else(String::new, |npc| npc.profession),
            expected_location: referral_location_label,
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: receipt.learned_at,
        });
    }
    issue_rumor_action_graph(
        ctx,
        character_id,
        &canonical_case_id,
        &lead_id,
        &receipt.settlement_id,
        &receipt.contact_npc_id,
        &receipt.safe_summary,
    )?;
    crate::outbreak::discover_case_corpses(
        ctx,
        &canonical_case_id,
        character_id,
        receipt.learned_at,
    )?;
    record_action(ctx, action_id, character_id, "receive_rumor", payload);
    Ok(())
}

/// Trusted authority seam for #184/generation. `pipeline_json` is private
/// server-authored material and must never originate in or be projected to a
/// browser; only the registered SSR gateway can invoke this temporary seam.
fn process_investigation_pipeline(
    pipeline: inv::PipelineInput,
) -> Result<(inv::Observation, inv::Recollection, Option<inv::Claim>), String> {
    inv::process_report(pipeline)
        .map_err(|error| format!("Invalid investigation pipeline at report processing: {error:?}"))
}

pub(crate) fn stage_investigation_claim(
    ctx: &ReducerContext,
    character_id: u64,
    receipt_id: String,
    pipeline_json: String,
    public_case_id: String,
    safe_source_label: String,
    conflict_group: String,
    correction_of_belief_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    for value in [&receipt_id, &public_case_id, &safe_source_label] {
        bounded(value)?;
    }
    bounded_optional(&conflict_group)?;
    bounded_optional(&correction_of_belief_id)?;
    if pipeline_json.len() > 8_192 {
        return Err("Pipeline payload is too large".into());
    }
    let pipeline: inv::PipelineInput = serde_json::from_str(&pipeline_json)
        .map_err(|_| "Invalid investigation pipeline at payload decoding")?;
    let proposition = pipeline.proposition.clone();
    let (observation, recollection, claim) = process_investigation_pipeline(pipeline)?;
    let claim = claim.ok_or("An omitted proposition cannot create a receivable claim")?;
    if ctx
        .db
        .investigation_safe_claim_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Err("Safe claim receipt already exists".into());
    }
    let event_id = observation.event_id.as_str().to_string();
    let event_payload = serde_json::to_string(&proposition).map_err(|e| e.to_string())?;
    if let Some(existing) = ctx.db.investigation_event_authority().id().find(&event_id) {
        if existing.case_id != claim.case_id.as_str()
            || existing.canonical_propositions_json != event_payload
        {
            return Err("Event id does not match its existing authority payload".into());
        }
    } else {
        ctx.db
            .investigation_event_authority()
            .insert(InvestigationEventAuthority {
                id: event_id,
                case_id: claim.case_id.as_str().into(),
                canonical_propositions_json: event_payload,
                occurred_at: claim.received_at,
            });
    }
    let observation_row = InvestigationObservation {
        id: observation.id.as_str().into(),
        event_id: observation.event_id.as_str().into(),
        observer_ref: observation.observer_ref.clone(),
        proposition_id: observation.proposition_id.as_str().into(),
        stage_json: serde_json::to_string(&observation).map_err(|e| e.to_string())?,
    };
    if let Some(existing) = ctx
        .db
        .investigation_observation()
        .id()
        .find(&observation_row.id)
    {
        if existing.event_id != observation_row.event_id
            || existing.observer_ref != observation_row.observer_ref
            || existing.proposition_id != observation_row.proposition_id
            || existing.stage_json != observation_row.stage_json
        {
            return Err("Observation id does not match existing authority".into());
        }
    } else {
        ctx.db.investigation_observation().insert(observation_row);
    }
    let recollection_row = InvestigationRecollection {
        id: recollection.id.as_str().into(),
        observation_id: recollection.observation_id.as_str().into(),
        witness_ref: claim.speaker_ref.clone(),
        proposition_id: claim.proposition_id.as_str().into(),
        stage_json: serde_json::to_string(&recollection).map_err(|e| e.to_string())?,
    };
    if let Some(existing) = ctx
        .db
        .investigation_recollection()
        .id()
        .find(&recollection_row.id)
    {
        if existing.observation_id != recollection_row.observation_id
            || existing.witness_ref != recollection_row.witness_ref
            || existing.proposition_id != recollection_row.proposition_id
            || existing.stage_json != recollection_row.stage_json
        {
            return Err("Recollection id does not match existing authority".into());
        }
    } else {
        ctx.db.investigation_recollection().insert(recollection_row);
    }
    let claim_row = InvestigationClaim {
        id: claim.id.as_str().into(),
        case_id: claim.case_id.as_str().into(),
        proposition_id: claim.proposition_id.as_str().into(),
        hidden_speaker_ref: claim.speaker_ref,
        statement: claim.statement.clone(),
        confidence_bps: claim.confidence.get(),
        disclosure_stage: format!("{:?}", claim.disclosure),
        transmission_stage: format!("{:?}", claim.transmission),
        received_at: claim.received_at,
        public_case_id: public_case_id.clone(),
        safe_source_label: safe_source_label.clone(),
        conflict_group: conflict_group.clone(),
    };
    if let Some(existing) = ctx.db.investigation_claim().id().find(&claim_row.id) {
        if existing.case_id != claim_row.case_id
            || existing.proposition_id != claim_row.proposition_id
            || existing.statement != claim_row.statement
            || existing.public_case_id != claim_row.public_case_id
        {
            return Err("Claim id does not match existing authority".into());
        }
    } else {
        ctx.db.investigation_claim().insert(claim_row);
    }
    ctx.db
        .investigation_safe_claim_receipt()
        .insert(InvestigationSafeClaimReceipt {
            id: receipt_id,
            owner_character_id: character_id,
            claim_id: claim.id.as_str().into(),
            public_case_id,
            proposition_id: claim.proposition_id.as_str().into(),
            statement: claim.statement,
            safe_source_label,
            confidence_bps: claim.confidence.get(),
            conflict_group,
            correction_of_belief_id,
            consumed_by: String::new(),
        });
    Ok(())
}

fn validate_generated_testimony_site(
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    draft: &adventuresim_core::quest_generation::TestimonyDraft,
    site: Option<&CaseSiteAuthority>,
) -> Result<(), &'static str> {
    if draft.destination_stage != "exact_believed" {
        return Ok(());
    }
    let site_id = draft
        .site_id
        .as_ref()
        .filter(|site_id| !site_id.0.is_empty())
        .ok_or("Exact generated testimony has no site identity")?;
    let generated_site = generated
        .sites
        .iter()
        .find(|site| site.id == *site_id)
        .ok_or("Exact generated testimony site is absent from the manifest")?;
    let site = site.ok_or("Exact generated testimony site authority is missing")?;
    if site.case_id != generated.canonical_case_id
        || site.id.value != site_id.0
        || site.id_key != site_id.0
        || site.name != generated_site.safe_label
        || site.distance_m == 0
        || (site.coordinates_are_geographic
            && (!(-1_800_000_000..=1_800_000_000).contains(&site.longitude_e7)
                || !(-900_000_000..=900_000_000).contains(&site.latitude_e7)))
    {
        return Err("Exact generated testimony site authority is inconsistent");
    }
    Ok(())
}

fn record_generated_bestiary_report(
    ctx: &ReducerContext,
    character_id: u64,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    witness: &adventuresim_core::quest_generation::WitnessBinding,
    received_at: u64,
) -> Result<(), String> {
    let id = inv::compound_id(&[
        "bestiary-report",
        &character_id.to_string(),
        &generated.public_case_id,
        &witness.id.0,
    ]);
    if let Some(existing) = ctx
        .db
        .investigation_bestiary_report_receipt()
        .id()
        .find(&id)
    {
        if existing.owner_character_id != character_id
            || existing.public_case_id != generated.public_case_id
            || existing.description_id != witness.description.as_str()
        {
            return Err("Bestiary report receipt conflicts with its generated authority".into());
        }
    } else {
        ctx.db
            .investigation_bestiary_report_receipt()
            .insert(InvestigationBestiaryReportReceipt {
                id,
                owner_character_id: character_id,
                public_case_id: generated.public_case_id.clone(),
                description_id: witness.description.as_str().into(),
                source_label: witness.display_name.clone(),
                received_at,
            });
    }
    rebuild_bestiary_deductions(ctx, character_id, &generated.public_case_id, received_at)
}

pub(crate) fn persist_generated_testimony(
    ctx: &ReducerContext,
    character_id: u64,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    witness: &adventuresim_core::quest_generation::WitnessBinding,
    presentation_texts: Option<&[String]>,
    dialogue_action_id: &str,
    withheld_only: bool,
) -> Result<(), String> {
    if !generated
        .witnesses
        .iter()
        .any(|authoritative| authoritative == witness)
    {
        return Err("Generated testimony witness is absent from the authoritative manifest".into());
    }
    let projection_plan =
        adventuresim_core::quest_generation::generated_testimony_projection_plan(witness)
            .map_err(str::to_string)?;
    let authority = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&generated.canonical_case_id)
        .ok_or("Generated testimony case authority is missing")?;
    let validated_authority = validate_quest_generation_authority(&authority)?;
    if validated_authority.manifest != *generated {
        return Err("Generated testimony manifest does not match private authority".into());
    }
    if let Some(texts) = presentation_texts {
        if texts.len() != witness.testimony.len()
            || texts.iter().any(|text| {
                text.is_empty()
                    || text.chars().count() > 1_024
                    || text.chars().any(char::is_control)
            })
        {
            return Err("Generated testimony presentation text is invalid".into());
        }
    }
    for draft in &projection_plan {
        let site = draft
            .site_id
            .as_ref()
            .and_then(|site_id| ctx.db.case_site_authority().id_key().find(&site_id.0));
        validate_generated_testimony_site(generated, draft, site.as_ref())
            .map_err(str::to_string)?;
    }
    let generation_context = validated_authority.context;
    let mut corrected_capability_ids = BTreeSet::new();
    for (index, draft) in projection_plan.iter().enumerate() {
        let is_withheld =
            draft.delivery == adventuresim_core::quest_generation::TestimonyDelivery::Withheld;
        if is_withheld != withheld_only {
            continue;
        }
        let (receipt_id, mut pipeline) =
            adventuresim_core::quest_generation::generated_testimony_pipeline(
                &generation_context,
                character_id,
                generated,
                witness,
                index,
                official_minute(ctx),
            )
            .map_err(|error| format!("Invalid generated testimony pipeline: {error:?}"))?;
        let presentation_text = presentation_texts
            .and_then(|texts| texts.get(index))
            .unwrap_or(&draft.spoken_text);
        pipeline.recalled_text = presentation_text.clone();
        pipeline.disclosed_text = Some(presentation_text.clone());
        pipeline.transmitted_text = presentation_text.clone();
        if ctx
            .db
            .investigation_safe_claim_receipt()
            .id()
            .find(&receipt_id)
            .is_some()
        {
            for referred in authored_witness_referrals(generated, witness, draft)? {
                grant_generated_witness_referral(
                    ctx,
                    character_id,
                    generated,
                    referred,
                    &generation_context.settlement_id,
                    WitnessReferralProvenance::Testimony {
                        source_witness: witness,
                        testimony_index: index,
                        source_draft: draft,
                        source_receipt_id: &receipt_id,
                    },
                )?;
            }
            continue;
        }
        let correction_belief_id = draft
            .corrects_proposition_id
            .as_ref()
            .and_then(|proposition_id| {
                ctx.db
                    .investigation_belief()
                    .owner_character_id()
                    .filter(character_id)
                    .find(|belief| {
                        belief.case_id == generated.public_case_id
                            && belief.proposition_id == *proposition_id
                    })
                    .map(|belief| belief.id)
            })
            .unwrap_or_default();
        stage_investigation_claim(
            ctx,
            character_id,
            receipt_id.clone(),
            serde_json::to_string(&pipeline)
                .map_err(|_| "Could not encode generated testimony pipeline")?,
            generated.public_case_id.clone(),
            "the referred local witness".into(),
            inv::compound_id(&["conflict", &generated.public_case_id, &draft.proposition_id]),
            correction_belief_id,
        )?;
        receive_investigation_claim(
            ctx,
            character_id,
            inv::compound_id(&["receive-generated-testimony", &receipt_id]),
            receipt_id.clone(),
        )?;
        record_generated_bestiary_report(
            ctx,
            character_id,
            generated,
            witness,
            official_minute(ctx),
        )?;

        let site = draft
            .site_id
            .as_ref()
            .and_then(|site_id| ctx.db.case_site_authority().id_key().find(&site_id.0));
        let exact = draft.destination_stage == "exact_believed";
        let lead_id = inv::compound_id(&[
            "lead",
            "generated-testimony",
            &character_id.to_string(),
            &witness.id.0,
            &index.to_string(),
        ]);
        if ctx.db.investigation_lead().id().find(&lead_id).is_none() {
            let npc = ctx
                .db
                .settlement_npc()
                .id()
                .find(&witness.npc_id)
                .ok_or("Generated witness is no longer persistent")?;
            ctx.db.investigation_lead().insert(InvestigationLead {
                id: lead_id.clone(),
                owner_character_id: character_id,
                case_id: generated.public_case_id.clone(),
                proposition_id: draft.proposition_id.clone(),
                summary: presentation_text.clone(),
                source_label: "the referred local witness".into(),
                // Confidence describes provenance quality, never sincerity.
                // Generated testimony uses one observer-facing band for every
                // hidden reliability state.
                confidence_bps: 6_000,
                destination_stage: draft.destination_stage.clone(),
                directions: if exact {
                    String::new()
                } else {
                    presentation_text.clone()
                },
                exact_location_id: site
                    .as_ref()
                    .filter(|_| exact)
                    .map_or_else(String::new, |site| site.id.value.clone()),
                latitude_e7: site
                    .as_ref()
                    .filter(|_| exact)
                    .map_or(0, |site| site.latitude_e7),
                longitude_e7: site
                    .as_ref()
                    .filter(|_| exact)
                    .map_or(0, |site| site.longitude_e7),
                witness_name: npc.name,
                witness_description: witness.visible_description.clone(),
                witness_occupation_or_relationship: npc.profession,
                expected_location: adventuresim_core::quest_generation::referral_display_location(
                    witness,
                )
                .to_owned(),
                current_learned_location: site.as_ref().filter(|_| exact).map_or_else(
                    || {
                        adventuresim_core::quest_generation::referral_display_location(witness)
                            .to_owned()
                    },
                    |site| site.name.clone(),
                ),
                contradiction_group: format!("generated-location:{}", generated.public_case_id),
                corrected_by: String::new(),
                recorded_at: official_minute(ctx),
            });
            if let Some(corrected_proposition) = &draft.corrects_proposition_id {
                for mut prior in ctx
                    .db
                    .investigation_lead()
                    .owner_character_id()
                    .filter(character_id)
                    .filter(|prior| {
                        prior.case_id == generated.public_case_id
                            && prior.proposition_id == *corrected_proposition
                            && prior.id != lead_id
                            && prior.corrected_by.is_empty()
                    })
                    .collect::<Vec<_>>()
                {
                    corrected_capability_ids
                        .extend(dependent_capability_ids_for_exact_lead(ctx, &prior));
                    prior.corrected_by = lead_id.clone();
                    ctx.db.investigation_lead().id().update(prior);
                }
            }
        }
        for referred in authored_witness_referrals(generated, witness, draft)? {
            grant_generated_witness_referral(
                ctx,
                character_id,
                generated,
                referred,
                &generation_context.settlement_id,
                WitnessReferralProvenance::Testimony {
                    source_witness: witness,
                    testimony_index: index,
                    source_draft: draft,
                    source_receipt_id: &receipt_id,
                },
            )?;
        }
    }
    reset_unsupported_capability_progress(ctx, corrected_capability_ids)?;
    complete_referred_contact_action(
        ctx,
        character_id,
        &generated.canonical_case_id,
        &witness.npc_id,
        dialogue_action_id,
    )
}

#[reducer]
pub fn receive_investigation_claim(
    ctx: &ReducerContext,
    character_id: u64,
    action_id: String,
    receipt_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    bounded(&receipt_id)?;
    let payload = canonical_payload(&[&receipt_id])?;
    if idempotent(ctx, &action_id, character_id, "receive_claim", &payload)? {
        return Ok(());
    }
    let mut receipt = ctx
        .db
        .investigation_safe_claim_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Safe claim receipt not found")?;
    if receipt.owner_character_id != character_id || !receipt.consumed_by.is_empty() {
        return Err("Safe claim receipt is stale or belongs to another observer".into());
    }
    let authority = ctx
        .db
        .investigation_claim()
        .id()
        .find(&receipt.claim_id)
        .ok_or("Claim authority missing")?;
    if authority.public_case_id != receipt.public_case_id
        || authority.proposition_id != receipt.proposition_id
        || authority.statement != receipt.statement
        || authority.safe_source_label != receipt.safe_source_label
        || authority.confidence_bps != receipt.confidence_bps
    {
        return Err("Safe claim receipt no longer matches authority".into());
    }
    let previous = if receipt.correction_of_belief_id.is_empty() {
        None
    } else {
        let belief = ctx
            .db
            .investigation_belief()
            .id()
            .find(&receipt.correction_of_belief_id)
            .ok_or("Correction target belief not found")?;
        if belief.owner_character_id != character_id
            || belief.case_id != receipt.public_case_id
            || belief.proposition_id != receipt.proposition_id
        {
            return Err("Correction target does not match observer and proposition".into());
        }
        Some(belief)
    };
    let belief_id = previous.as_ref().map_or_else(
        || inv::compound_id(&["belief", &character_id.to_string(), &receipt.claim_id]),
        |belief| belief.id.clone(),
    );
    let now = official_minute(ctx);
    let revision = previous.as_ref().map_or(1, |_| {
        ctx.db
            .investigation_belief_revision()
            .owner_character_id()
            .filter(character_id)
            .filter(|r| r.belief_id == belief_id)
            .count()
            .saturating_add(1) as u16
    });
    let revision_id = inv::compound_id(&["revision", &belief_id, &revision.to_string()]);
    ctx.db
        .investigation_belief_revision()
        .insert(InvestigationBeliefRevision {
            id: revision_id.clone(),
            owner_character_id: character_id,
            belief_id: belief_id.clone(),
            revision,
            statement: receipt.statement.clone(),
            confidence_bps: receipt.confidence_bps,
            provenance_kind: "received_claim".into(),
            provenance_label: receipt.safe_source_label.clone(),
            supersedes: previous
                .as_ref()
                .map_or_else(String::new, |b| b.current_revision_id.clone()),
            recorded_at: now,
        });
    let belief = InvestigationBelief {
        id: belief_id.clone(),
        owner_character_id: character_id,
        case_id: receipt.public_case_id.clone(),
        proposition_id: receipt.proposition_id.clone(),
        current_revision_id: revision_id,
        statement: receipt.statement.clone(),
        confidence_bps: receipt.confidence_bps,
        conflict_group: receipt.conflict_group.clone(),
    };
    if previous.is_some() {
        ctx.db.investigation_belief().id().update(belief);
    } else {
        ctx.db.investigation_belief().insert(belief);
    }
    let testimony_id = inv::compound_id(&[
        "received-testimony",
        &character_id.to_string(),
        &receipt.claim_id,
        &authority.hidden_speaker_ref,
    ]);
    if ctx
        .db
        .investigation_received_testimony()
        .id()
        .find(&testimony_id)
        .is_none()
    {
        ctx.db
            .investigation_received_testimony()
            .insert(InvestigationReceivedTestimony {
                id: testimony_id,
                owner_character_id: character_id,
                public_case_id: receipt.public_case_id.clone(),
                claim_id: receipt.claim_id.clone(),
                witness_ref: authority.hidden_speaker_ref,
                source_receipt_id: receipt.id.clone(),
                received_at: now,
            });
    }
    receipt.consumed_by = action_id.clone();
    ctx.db
        .investigation_safe_claim_receipt()
        .id()
        .update(receipt);
    record_action(ctx, action_id, character_id, "receive_claim", payload);
    Ok(())
}

#[reducer]
pub fn stage_investigation_lead(
    ctx: &ReducerContext,
    character_id: u64,
    receipt_id: String,
    public_case_id: String,
    summary: String,
    safe_source_label: String,
    confidence_bps: u16,
    destination_stage: String,
    directions: String,
    exact_location_id: String,
    latitude_e7: i32,
    longitude_e7: i32,
    conflict_group: String,
    correction_of_lead_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    for value in [
        &receipt_id,
        &public_case_id,
        &summary,
        &safe_source_label,
        &destination_stage,
    ] {
        bounded(value)?;
    }
    bps(confidence_bps)?;
    bounded_optional(&directions)?;
    bounded_optional(&exact_location_id)?;
    bounded_optional(&conflict_group)?;
    bounded_optional(&correction_of_lead_id)?;
    if !matches!(
        destination_stage.as_str(),
        "unknown"
            | "textual"
            | "landmark"
            | "approximate_area"
            | "route_segment"
            | "exact_believed"
            | "visited"
    ) {
        return Err("Unknown destination knowledge stage".into());
    }
    validate_destination(
        &destination_stage,
        &exact_location_id,
        latitude_e7,
        longitude_e7,
    )?;
    if matches!(destination_stage.as_str(), "exact_believed" | "visited") {
        let site = ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&exact_location_id)
            .ok_or("Exact lead must name a server-issued case site")?;
        if site.case_id != public_case_id
            || site.latitude_e7 != latitude_e7
            || site.longitude_e7 != longitude_e7
        {
            return Err("Exact lead does not match the case-site authority".into());
        }
    }
    if ctx
        .db
        .investigation_safe_lead_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Err("Safe lead receipt already exists".into());
    }
    ctx.db
        .investigation_safe_lead_receipt()
        .insert(InvestigationSafeLeadReceipt {
            id: receipt_id,
            owner_character_id: character_id,
            public_case_id,
            summary,
            safe_source_label,
            confidence_bps,
            destination_stage,
            directions,
            exact_location_id,
            latitude_e7,
            longitude_e7,
            conflict_group,
            correction_of_lead_id,
            consumed_by: String::new(),
        });
    Ok(())
}

#[reducer]
pub fn discover_investigation_lead(
    ctx: &ReducerContext,
    character_id: u64,
    action_id: String,
    receipt_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    bounded(&receipt_id)?;
    let payload = canonical_payload(&[&receipt_id])?;
    if idempotent(ctx, &action_id, character_id, "discover_lead", &payload)? {
        return Ok(());
    }
    let mut receipt = ctx
        .db
        .investigation_safe_lead_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Safe lead receipt not found")?;
    if receipt.owner_character_id != character_id || !receipt.consumed_by.is_empty() {
        return Err("Safe lead receipt is stale or belongs to another observer".into());
    }
    if matches!(
        receipt.destination_stage.as_str(),
        "exact_believed" | "visited"
    ) {
        let site = ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&receipt.exact_location_id)
            .ok_or("Exact lead must name a server-issued case site")?;
        if site.case_id != receipt.public_case_id
            || site.latitude_e7 != receipt.latitude_e7
            || site.longitude_e7 != receipt.longitude_e7
        {
            return Err("Exact lead no longer matches the case-site authority".into());
        }
    }
    let lead_id = inv::compound_id(&["lead", &character_id.to_string(), &receipt_id]);
    let mut corrected_capability_ids = BTreeSet::new();
    if !receipt.correction_of_lead_id.is_empty() {
        let mut prior = ctx
            .db
            .investigation_lead()
            .id()
            .find(&receipt.correction_of_lead_id)
            .ok_or("Correction target lead not found")?;
        if prior.owner_character_id != character_id || prior.case_id != receipt.public_case_id {
            return Err("Correction target does not match observer and case".into());
        }
        let invalidated_live_support = prior.corrected_by.is_empty();
        if invalidated_live_support {
            corrected_capability_ids.extend(dependent_capability_ids_for_exact_lead(ctx, &prior));
        }
        prior.corrected_by = lead_id.clone();
        ctx.db.investigation_lead().id().update(prior);
    }
    ctx.db.investigation_lead().insert(InvestigationLead {
        id: lead_id,
        owner_character_id: character_id,
        case_id: receipt.public_case_id.clone(),
        proposition_id: String::new(),
        summary: receipt.summary.clone(),
        source_label: receipt.safe_source_label.clone(),
        confidence_bps: receipt.confidence_bps,
        destination_stage: receipt.destination_stage.clone(),
        directions: receipt.directions.clone(),
        exact_location_id: receipt.exact_location_id.clone(),
        latitude_e7: receipt.latitude_e7,
        longitude_e7: receipt.longitude_e7,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: String::new(),
        contradiction_group: receipt.conflict_group.clone(),
        corrected_by: String::new(),
        recorded_at: official_minute(ctx),
    });
    reset_unsupported_capability_progress(ctx, corrected_capability_ids)?;
    receipt.consumed_by = action_id.clone();
    ctx.db
        .investigation_safe_lead_receipt()
        .id()
        .update(receipt);
    record_action(ctx, action_id, character_id, "discover_lead", payload);
    Ok(())
}

fn same_place(ctx: &ReducerContext, left: &crate::Character, right: &crate::Character) -> bool {
    let left_site = character_case_site_id(ctx, left.id);
    let right_site = character_case_site_id(ctx, right.id);
    (left.current_settlement_id.is_some()
        && left.current_settlement_id == right.current_settlement_id)
        || (left_site.is_some() && left_site == right_site)
}

#[reducer]
pub fn share_investigation_lead(
    ctx: &ReducerContext,
    sender_id: u64,
    recipient_id: u64,
    source_lead_id: String,
    action_id: String,
) -> Result<(), String> {
    let sender = require_actor(ctx, sender_id)?;
    bounded(&source_lead_id)?;
    let recipient = ctx
        .db
        .character()
        .id()
        .find(recipient_id)
        .ok_or("Recipient not found")?;
    if !recipient.alive
        || sender.party_id.is_none()
        || sender.party_id != recipient.party_id
        || !same_place(ctx, &sender, &recipient)
    {
        return Err("Recipient must be a living, co-located member of the sender's party".into());
    }
    let source = ctx
        .db
        .investigation_lead()
        .id()
        .find(&source_lead_id)
        .ok_or("Source lead not found")?;
    if source.owner_character_id != sender_id {
        return Err("Cannot share another observer's lead".into());
    }
    let payload = canonical_payload(&[
        &recipient_id.to_string(),
        &source_lead_id,
        &source.summary,
        &source.source_label,
        &source.confidence_bps.to_string(),
        &source.destination_stage,
        &source.directions,
        &source.exact_location_id,
        &source.latitude_e7.to_string(),
        &source.longitude_e7.to_string(),
        &source.witness_name,
        &source.witness_description,
        &source.witness_occupation_or_relationship,
        &source.expected_location,
        &source.current_learned_location,
        &source.contradiction_group,
        &source.corrected_by,
    ])?;
    if idempotent(ctx, &action_id, sender_id, "share_lead", &payload)? {
        return Ok(());
    }
    let receipt_id = inv::compound_id(&[
        "share-lead",
        &sender_id.to_string(),
        &recipient_id.to_string(),
        &source_lead_id,
        &payload,
    ]);
    if let Some(existing) = ctx
        .db
        .investigation_sharing_receipt()
        .id()
        .find(&receipt_id)
    {
        if existing.payload_fingerprint != payload {
            return Err("Semantic share receipt payload mismatch".into());
        }
        record_action(ctx, action_id, sender_id, "share_lead", payload);
        return Ok(());
    }
    let copy_id = inv::compound_id(&["shared-lead", &recipient_id.to_string(), &receipt_id]);
    ctx.db.investigation_lead().insert(InvestigationLead {
        id: copy_id,
        owner_character_id: recipient_id,
        source_label: format!("shared by {}", sender.name),
        ..source
    });
    ctx.db
        .investigation_sharing_receipt()
        .insert(InvestigationSharingReceipt {
            id: receipt_id,
            sender_id,
            recipient_id,
            source_record_id: source_lead_id,
            payload_fingerprint: payload.clone(),
            shared_at: official_minute(ctx),
        });
    record_action(ctx, action_id, sender_id, "share_lead", payload);
    Ok(())
}

#[reducer]
pub fn share_investigation_belief(
    ctx: &ReducerContext,
    sender_id: u64,
    recipient_id: u64,
    source_belief_id: String,
    action_id: String,
) -> Result<(), String> {
    let sender = require_actor(ctx, sender_id)?;
    bounded(&source_belief_id)?;
    let recipient = ctx
        .db
        .character()
        .id()
        .find(recipient_id)
        .ok_or("Recipient not found")?;
    if !recipient.alive
        || sender.party_id.is_none()
        || sender.party_id != recipient.party_id
        || !same_place(ctx, &sender, &recipient)
    {
        return Err("Recipient must be a living, co-located member of the sender's party".into());
    }
    let source = ctx
        .db
        .investigation_belief()
        .id()
        .find(&source_belief_id)
        .ok_or("Source belief not found")?;
    if source.owner_character_id != sender_id {
        return Err("Cannot share another observer's belief".into());
    }
    let payload = canonical_payload(&[
        &recipient_id.to_string(),
        &source_belief_id,
        &source.current_revision_id,
        &source.case_id,
        &source.proposition_id,
        &source.statement,
        &source.confidence_bps.to_string(),
    ])?;
    if idempotent(ctx, &action_id, sender_id, "share_belief", &payload)? {
        return Ok(());
    }
    let recipient_belief_id = inv::compound_id(&[
        "belief",
        &recipient_id.to_string(),
        &source_belief_id,
        &source.current_revision_id,
    ]);
    let existing = ctx
        .db
        .investigation_belief()
        .id()
        .find(&recipient_belief_id);
    let revision = existing.as_ref().map_or(1, |_| {
        ctx.db
            .investigation_belief_revision()
            .owner_character_id()
            .filter(recipient_id)
            .filter(|r| r.belief_id == recipient_belief_id)
            .count()
            .saturating_add(1) as u16
    });
    let revision_id = inv::compound_id(&["revision", &recipient_belief_id, &revision.to_string()]);
    let receipt_id = inv::compound_id(&[
        "share-belief",
        &sender_id.to_string(),
        &recipient_id.to_string(),
        &source_belief_id,
        &source.current_revision_id,
        &payload,
    ]);
    if let Some(existing_share) = ctx
        .db
        .investigation_sharing_receipt()
        .id()
        .find(&receipt_id)
    {
        if existing_share.payload_fingerprint != payload {
            return Err("Semantic share receipt payload mismatch".into());
        }
        record_action(ctx, action_id, sender_id, "share_belief", payload);
        return Ok(());
    }
    ctx.db
        .investigation_belief_revision()
        .insert(InvestigationBeliefRevision {
            id: revision_id.clone(),
            owner_character_id: recipient_id,
            belief_id: recipient_belief_id.clone(),
            revision,
            statement: source.statement.clone(),
            confidence_bps: source.confidence_bps,
            provenance_kind: "shared_by".into(),
            provenance_label: format!("shared by {}", sender.name),
            supersedes: existing
                .as_ref()
                .map_or_else(String::new, |belief| belief.current_revision_id.clone()),
            recorded_at: official_minute(ctx),
        });
    let copy = InvestigationBelief {
        id: recipient_belief_id,
        owner_character_id: recipient_id,
        case_id: source.case_id,
        proposition_id: source.proposition_id,
        current_revision_id: revision_id,
        statement: source.statement,
        confidence_bps: source.confidence_bps,
        conflict_group: source.conflict_group,
    };
    if existing.is_some() {
        ctx.db.investigation_belief().id().update(copy);
    } else {
        ctx.db.investigation_belief().insert(copy);
    }
    ctx.db
        .investigation_sharing_receipt()
        .insert(InvestigationSharingReceipt {
            id: receipt_id,
            sender_id,
            recipient_id,
            source_record_id: source_belief_id,
            payload_fingerprint: payload.clone(),
            shared_at: official_minute(ctx),
        });
    record_action(ctx, action_id, sender_id, "share_belief", payload);
    Ok(())
}
