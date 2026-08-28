#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum InvestigationActionConsequence {
    None,
    RetrieveAsset { asset_id: String, version: u32 },
    RescueSubject { subject_id: String, version: u32 },
}

fn parse_action_kind(value: &str) -> Result<action::InvestigationActionKind, String> {
    use action::InvestigationActionKind as K;
    match value {
        "inspect_site" => Ok(K::InspectSite),
        "search_area" => Ok(K::SearchArea),
        "follow_tracks" => Ok(K::FollowTracks),
        "reacquire_tracks" => Ok(K::ReacquireTracks),
        "locate_contact" => Ok(K::LocateContact),
        "watch" => Ok(K::Watch),
        "patrol" => Ok(K::Patrol),
        "lay_ambush" => Ok(K::LayAmbush),
        "approach_lead" => Ok(K::ApproachLead),
        _ => Err("Unknown investigation action method".into()),
    }
}

fn action_method(kind: action::InvestigationActionKind) -> &'static str {
    use action::InvestigationActionKind as K;
    match kind {
        K::InspectSite => "inspect_site",
        K::SearchArea => "search_area",
        K::FollowTracks => "follow_tracks",
        K::ReacquireTracks => "reacquire_tracks",
        K::LocateContact => "locate_contact",
        K::Watch => "watch",
        K::Patrol => "patrol",
        K::LayAmbush => "lay_ambush",
        K::ApproachLead => "approach_lead",
    }
}

fn parse_action_terrain(value: &str) -> Result<action::Terrain, String> {
    use action::Terrain as T;
    match value {
        "road" => Ok(T::Road),
        "settlement" => Ok(T::Settlement),
        "plains" => Ok(T::Plains),
        "forest" => Ok(T::Forest),
        "hills" => Ok(T::Hills),
        "marsh" => Ok(T::Marsh),
        "ruins" => Ok(T::Ruins),
        "underground" => Ok(T::Underground),
        _ => Err("Unknown investigation terrain".into()),
    }
}

/// Trusted generator seam. The opaque id is the only authority returned to a
/// browser. Hidden targets, seeds, and consequences remain private.
#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn validate_investigation_action_text(
    id: &str,
    case_id: &str,
    target_kind: &str,
    target_id: &str,
    safe_summary: &str,
    known_prerequisites: &str,
    safe_result_on_success: &str,
    required_action_id: &str,
    alternate_route_action_id: &str,
) -> Result<(), String> {
    for text in [
        id,
        case_id,
        target_kind,
        target_id,
        safe_summary,
        known_prerequisites,
        safe_result_on_success,
        alternate_route_action_id,
    ] {
        bounded(text)?;
    }
    // Root actions have no predecessor. Successor actions still carry the
    // observer-scoped prerequisite id and are validated as ordinary text.
    bounded_optional(required_action_id)
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub(crate) fn issue_investigation_action_capability(
    ctx: &ReducerContext,
    id: String,
    owner_character_id: u64,
    case_id: String,
    provenance_kind: InvestigationProvenanceKind,
    generated_case_id: String,
    kind: action::InvestigationActionKind,
    target_kind: String,
    target_id: String,
    target_terrain: action::Terrain,
    seed: u64,
    uncertainty_bps: u16,
    safe_summary: String,
    known_prerequisites: String,
    safe_result_on_success: String,
    consequence: InvestigationActionConsequence,
    required_action_id: String,
    alternate_route_action_id: String,
) -> Result<(), String> {
    match provenance_kind {
        InvestigationProvenanceKind::Manual if generated_case_id.is_empty() => {}
        InvestigationProvenanceKind::Generated if !generated_case_id.is_empty() => {}
        _ => return Err("Investigation capability provenance is invalid".into()),
    }
    validate_investigation_action_text(
        &id,
        &case_id,
        &target_kind,
        &target_id,
        &safe_summary,
        &known_prerequisites,
        &safe_result_on_success,
        &required_action_id,
        &alternate_route_action_id,
    )?;
    bps(uncertainty_bps)?;
    if alternate_route_action_id == id {
        return Err("A critical action needs a distinct recovery route".into());
    }
    if ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&id)
        .is_some()
    {
        return Err("Investigation action capability already exists".into());
    }
    let target_exists = match target_kind.as_str() {
        "site" => ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&target_id)
            .is_some(),
        "area" => ctx
            .db
            .investigation_area_authority()
            .id()
            .find(&target_id)
            .is_some(),
        "cohort" => ctx
            .db
            .investigation_pattern_target_authority()
            .cohort_id()
            .find(&target_id)
            .is_some_and(|target| target.case_id == case_id),
        "contact" | "route" | "tracks" => true,
        _ => false,
    };
    if !target_exists {
        return Err("Investigation action target is not authoritative".into());
    }
    ctx.db
        .investigation_action_capability()
        .insert(InvestigationActionCapability {
            id,
            owner_character_id,
            case_id,
            provenance_kind,
            generated_case_id,
            method: action_method(kind).into(),
            version: 0,
            target_kind,
            target_id,
            target_terrain: format!("{target_terrain:?}").to_ascii_lowercase(),
            seed,
            evidence_age_origin_minute: character_strategic_minute(ctx, owner_character_id),
            uncertainty_bps,
            safe_summary,
            known_prerequisites,
            safe_result_on_success,
            consequence_json: serde_json::to_string(&consequence)
                .map_err(|_| "Investigation action consequence is invalid")?,
            required_action_id,
            alternate_route_action_id,
            active: false,
        });
    Ok(())
}

fn character_strategic_minute(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or_else(|| official_minute(ctx), |time| time.minutes)
}

fn generated_observer_id(
    ctx: &ReducerContext,
    case_id: &str,
    kind: &str,
    name: &str,
) -> Option<String> {
    ctx.db
        .quest_generation_authority()
        .case_id()
        .find(case_id.to_string())
        .and_then(|authority| validate_quest_generation_authority(&authority).ok())
        .map(|validated| {
            adventuresim_core::quest_generation::observer_scoped_id(&validated.context, kind, name)
        })
}

fn set_action_active(ctx: &ReducerContext, action_id: &str, active: bool) -> Result<(), String> {
    let mut capability = ctx
        .db
        .investigation_action_capability()
        .id()
        .find(action_id.to_string())
        .ok_or("Investigation route capability is missing")?;
    capability.active = active;
    ctx.db
        .investigation_action_capability()
        .id()
        .update(capability);
    Ok(())
}

fn validate_action_route_graph_structure(
    capabilities: &[InvestigationActionCapability],
) -> Result<(), String> {
    if capabilities.len() < 2 {
        return Err("Investigation needs at least two playable routes".into());
    }
    for capability in capabilities {
        let alternate = capabilities
            .iter()
            .find(|candidate| candidate.id == capability.alternate_route_action_id)
            .ok_or("Investigation alternate route is missing")?;
        if alternate.id == capability.id
            || alternate.owner_character_id != capability.owner_character_id
            || alternate.case_id != capability.case_id
        {
            return Err("Investigation alternate route crosses authority boundaries".into());
        }
        let Ok(kind) = parse_action_kind(&capability.method) else {
            return Err("Investigation route contains an unknown action method".into());
        };
        if matches!(
            kind,
            action::InvestigationActionKind::FollowTracks
                | action::InvestigationActionKind::ReacquireTracks
        ) {
            let predecessor = capabilities
                .iter()
                .find(|candidate| candidate.id == capability.required_action_id)
                .ok_or("Investigation physical tracking predecessor is missing")?;
            let predecessor_kind = parse_action_kind(&predecessor.method)?;
            if predecessor.owner_character_id != capability.owner_character_id
                || predecessor.case_id != capability.case_id
                || !action::tracking_route_edge_is_coherent(
                    kind,
                    &capability.target_kind,
                    predecessor_kind,
                    &predecessor.target_kind,
                )
            {
                return Err("Investigation physical tracking route is incoherent".into());
            }
        }
    }
    Ok(())
}

fn validate_initial_action_frontier(
    capabilities: &[InvestigationActionCapability],
) -> Result<(), String> {
    let active = capabilities
        .iter()
        .filter(|capability| capability.active)
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Err("Investigation needs an immediately playable entry action".into());
    }
    if active.len() == 1 {
        let entry = active[0];
        if entry.method != "locate_contact"
            || entry.target_kind != "contact"
            || !entry.required_action_id.is_empty()
        {
            return Err("A single investigation entry must be an exact referred contact".into());
        }
        let successors = capabilities
            .iter()
            .filter(|candidate| candidate.required_action_id == entry.id)
            .collect::<Vec<_>>();
        if successors.len() != 2
            || successors.iter().any(|candidate| candidate.active)
            || !successors
                .iter()
                .any(|candidate| candidate.method == "approach_lead")
            || !successors
                .iter()
                .any(|candidate| candidate.method == "watch")
        {
            return Err(
                "The referred contact must unlock inactive approach and watch routes".into(),
            );
        }
    }
    Ok(())
}

fn validate_evolved_action_frontier(
    capabilities: &[InvestigationActionCapability],
    mut predecessor_succeeded: impl FnMut(&str) -> bool,
) -> Result<(), String> {
    for capability in capabilities
        .iter()
        .filter(|capability| capability.active && !capability.required_action_id.is_empty())
    {
        let predecessor = capabilities
            .iter()
            .find(|candidate| candidate.id == capability.required_action_id)
            .ok_or("Active investigation route predecessor is missing")?;
        if predecessor.owner_character_id != capability.owner_character_id
            || predecessor.case_id != capability.case_id
        {
            return Err(
                "Active investigation route predecessor crosses authority boundaries".into(),
            );
        }
        if !predecessor_succeeded(&predecessor.id) {
            return Err("Active investigation route predecessor has not succeeded".into());
        }
    }
    Ok(())
}

fn action_route_capabilities(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
) -> Vec<InvestigationActionCapability> {
    ctx.db
        .investigation_action_capability()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|capability| capability.case_id == case_id)
        .collect()
}

fn validate_action_route_graph(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
) -> Result<(), String> {
    let capabilities = action_route_capabilities(ctx, owner_character_id, case_id);
    validate_action_route_graph_structure(&capabilities)?;
    validate_evolved_action_frontier(&capabilities, |predecessor_id| {
        ctx.db
            .investigation_action_attempt()
            .capability_id()
            .filter(predecessor_id)
            .any(|attempt| attempt.success)
    })
}

fn validate_newly_issued_action_route_graph(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
) -> Result<(), String> {
    let capabilities = action_route_capabilities(ctx, owner_character_id, case_id);
    validate_action_route_graph_structure(&capabilities)?;
    validate_initial_action_frontier(&capabilities)
}

fn successful_action_successor_ids(
    capabilities: &[InvestigationActionCapability],
    capability: &InvestigationActionCapability,
) -> Vec<String> {
    capabilities
        .iter()
        .filter(|candidate| {
            candidate.owner_character_id == capability.owner_character_id
                && candidate.case_id == capability.case_id
                && candidate.required_action_id == capability.id
        })
        .map(|candidate| candidate.id.clone())
        .collect()
}

fn activate_action_successors(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    succeeded: bool,
) -> Result<bool, String> {
    let mut activate = Vec::new();
    if succeeded {
        let capabilities =
            action_route_capabilities(ctx, capability.owner_character_id, &capability.case_id);
        activate.extend(successful_action_successor_ids(&capabilities, capability));
    } else {
        use adventuresim_core::quest_generation::{
            FailedActionAlternateTransition, ReferredContactActionState,
            transition_failed_action_alternate,
        };
        let capabilities: Vec<_> = ctx
            .db
            .investigation_action_capability()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .filter(|candidate| candidate.case_id == capability.case_id)
            .collect();
        let mut states = capabilities
            .iter()
            .map(|candidate| ReferredContactActionState {
                id: candidate.id.clone(),
                owner_character_id: candidate.owner_character_id,
                case_id: candidate.case_id.clone(),
                method: candidate.method.clone(),
                target_kind: candidate.target_kind.clone(),
                target_id: candidate.target_id.clone(),
                required_action_id: candidate.required_action_id.clone(),
                active: candidate.active,
                version: candidate.version,
                successful_attempt: ctx
                    .db
                    .investigation_action_attempt()
                    .capability_id()
                    .filter(&candidate.id)
                    .any(|attempt| attempt.success),
            })
            .collect::<Vec<_>>();
        match transition_failed_action_alternate(
            &mut states,
            capability.owner_character_id,
            &capability.case_id,
            &capability.alternate_route_action_id,
        )? {
            FailedActionAlternateTransition::Activated { alternate_id } => {
                let alternate = capabilities
                    .iter()
                    .find(|candidate| candidate.id == alternate_id)
                    .ok_or("Investigation recovery route no longer exists")?;
                let kind = parse_action_kind(&alternate.method)?;
                if capability_has_live_support_reducer(ctx, alternate, kind) {
                    activate.push(alternate_id);
                }
            }
            FailedActionAlternateTransition::Unavailable => {}
        }
    }
    for id in activate {
        set_action_active(ctx, &id, true)?;
    }
    if succeeded {
        return Ok(false);
    }
    Ok(ctx
        .db
        .investigation_action_capability()
        .owner_character_id()
        .filter(capability.owner_character_id)
        .filter(|candidate| {
            candidate.case_id == capability.case_id
                && candidate.id != capability.id
                && candidate.active
                && !ctx
                    .db
                    .investigation_action_attempt()
                    .capability_id()
                    .filter(&candidate.id)
                    .any(|attempt| attempt.success)
        })
        .any(|candidate| {
            parse_action_kind(&candidate.method)
                .is_ok_and(|kind| capability_has_live_support_reducer(ctx, &candidate, kind))
        }))
}

fn capability_has_live_support_reducer(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> bool {
    let Some(observer_case_id) = reducer_action_public_case_id(ctx, capability) else {
        return false;
    };
    if !tracking_capability_chain_is_coherent(
        capability,
        kind,
        |id| {
            ctx.db
                .investigation_action_capability()
                .id()
                .find(id.to_owned())
        },
        |id| {
            ctx.db
                .investigation_action_attempt()
                .capability_id()
                .filter(id)
                .any(|attempt| attempt.success)
        },
    ) {
        return false;
    }
    if !capability.required_action_id.is_empty()
        && !ctx
            .db
            .investigation_action_attempt()
            .capability_id()
            .filter(&capability.required_action_id)
            .any(|attempt| attempt.success)
    {
        return false;
    }
    if !capability_has_live_pattern_support_reducer(ctx, capability) {
        return false;
    }
    if kind == action::InvestigationActionKind::InspectSite && capability.target_kind == "site" {
        let Some(ExactActionCaseSite {
            site,
            lead,
            generated_aliases,
        }) = exact_action_case_site_for_observer(ctx, capability)
        else {
            return false;
        };
        if !exact_site_knowledge_is_live(
            &capability.case_id,
            &capability.target_id,
            &lead.case_id,
            &lead.exact_location_id,
            lead.destination_stage,
            &lead.corrected_by,
            &site.case_id,
            &site.id.value,
            lead.latitude_e7 == site.latitude_e7 && lead.longitude_e7 == site.longitude_e7,
            generated_aliases.as_ref().map(|aliases| aliases.0.as_str()),
            generated_aliases.as_ref().map(|aliases| aliases.1.as_str()),
        ) {
            return false;
        }
    }
    let prerequisites = action::prerequisites(kind);
    if prerequisites.requires_contact_referral
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead_is_live_contact_referral(
                    &lead,
                    capability.owner_character_id,
                    &observer_case_id,
                )
            })
    {
        return false;
    }
    if prerequisites.requires_approximate_destination
        && capability.target_kind != "area"
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead.case_id == observer_case_id
                    && lead.destination_stage == DestinationKnowledgeStage::ApproximateArea
                    && lead.corrected_by.is_empty()
            })
    {
        return false;
    }
    !prerequisites.requires_tracks || !capability.required_action_id.is_empty()
}

fn capability_has_live_pattern_support_reducer(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
) -> bool {
    let Some(observer_case_id) = reducer_action_public_case_id(ctx, capability) else {
        return false;
    };
    let output = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id);
    let Ok(authority) = generated_authority_reducer(ctx, capability) else {
        return false;
    };
    let evidence_id = match generated_pattern_authority(
        capability,
        authority
            .as_ref()
            .map(|(manifest, context)| (manifest.as_str(), context.as_str())),
        output.as_ref().map(|output| output.outputs_json.as_str()),
    ) {
        GeneratedPatternAuthority::Manual | GeneratedPatternAuthority::GeneratedWithoutPattern => {
            return true;
        }
        GeneratedPatternAuthority::Pattern { evidence_id, .. } => evidence_id,
        GeneratedPatternAuthority::Invalid => return false,
    };
    observer_pattern_route_has_live_corroborated_clue(
        capability.owner_character_id,
        &observer_case_id,
        &evidence_id,
        ctx.db
            .character_time()
            .character_id()
            .find(capability.owner_character_id)
            .map_or(0, |time| time.minutes),
        ctx.db
            .investigation_evidence_knowledge()
            .owner_character_id()
            .filter(capability.owner_character_id),
    )
}

fn validate_capability_blueprint_reducer(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
) -> Result<(), String> {
    let output = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id);
    let authority = generated_authority_reducer(ctx, capability)
        .map_err(|()| "Generated action authority is ambiguous or invalid")?;
    match generated_pattern_authority(
        capability,
        authority
            .as_ref()
            .map(|(manifest, context)| (manifest.as_str(), context.as_str())),
        output.as_ref().map(|output| output.outputs_json.as_str()),
    ) {
        GeneratedPatternAuthority::Invalid => {
            Err("Investigation capability no longer matches its authored blueprint".into())
        }
        GeneratedPatternAuthority::Manual
        | GeneratedPatternAuthority::GeneratedWithoutPattern
        | GeneratedPatternAuthority::Pattern { .. } => Ok(()),
    }
}

fn validate_referred_contact_authority(
    ctx: &ReducerContext,
    owner_character_id: u64,
    canonical_case_id: &str,
    witness_resident_character_id: u64,
) -> Result<bool, String> {
    let roots = ctx
        .db
        .investigation_action_capability()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|capability| {
            capability.case_id == canonical_case_id
                && capability.method == "locate_contact"
                && capability.target_kind == "contact"
                && capability.target_id == witness_resident_character_id.to_string()
        })
        .collect::<Vec<_>>();
    if roots.len() > 1 {
        return Err("Referred contact capability authority is ambiguous".into());
    }
    let Some(root) = roots.first() else {
        return Ok(false);
    };
    if !root.required_action_id.is_empty() {
        return Err("Referred contact capability is not an authored root".into());
    }
    validate_capability_blueprint_reducer(ctx, root)?;
    if root.provenance_kind != InvestigationProvenanceKind::Generated {
        return Err("Generated referred contact root has invalid provenance".into());
    }
    let authority = generated_authority_reducer(ctx, root)
        .map_err(|()| "Generated contact authority is ambiguous or invalid")?
        .ok_or("Generated contact authority is missing")?;
    let manifest =
        serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(&authority.0)
            .map_err(|_| "Generated contact manifest is invalid")?;
    let context = serde_json::from_str::<adventuresim_core::quest_generation::GenerationContext>(
        &authority.1,
    )
    .map_err(|_| "Generated contact context is invalid")?;
    let generated_root = manifest
        .actions
        .iter()
        .find(|action| {
            adventuresim_core::quest_generation::observer_scoped_id(
                &context,
                "capability",
                &format!("{owner_character_id}:{}", action.id.0),
            ) == root.id
        })
        .ok_or("Generated contact root is absent from its manifest")?;
    let expected_successors = manifest
        .actions
        .iter()
        .filter(|action| action.prerequisite.as_ref() == Some(&generated_root.id))
        .collect::<Vec<_>>();
    let expected_successor_ids = expected_successors
        .into_iter()
        .map(|generated| {
            adventuresim_core::quest_generation::observer_scoped_id(
                &context,
                "capability",
                &format!("{owner_character_id}:{}", generated.id.0),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    let actual_successors = ctx
        .db
        .investigation_action_capability()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|capability| {
            capability.case_id == canonical_case_id && capability.required_action_id == root.id
        })
        .collect::<Vec<_>>();
    if actual_successors
        .iter()
        .map(|successor| successor.id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        != expected_successor_ids
    {
        return Err("Generated contact successor set differs from its manifest".into());
    }
    for successor_id in expected_successor_ids {
        let successor = ctx
            .db
            .investigation_action_capability()
            .id()
            .find(&successor_id)
            .ok_or("Generated contact successor is missing")?;
        validate_capability_blueprint_reducer(ctx, &successor)?;
    }
    Ok(true)
}

fn complete_referred_contact_action(
    ctx: &ReducerContext,
    owner_character_id: u64,
    canonical_case_id: &str,
    witness_resident_character_id: u64,
    dialogue_action_id: &str,
) -> Result<(), String> {
    use adventuresim_core::quest_generation::{
        ReferredContactActionState, ReferredContactTransition, transition_referred_contact_action,
    };
    if !validate_referred_contact_authority(
        ctx,
        owner_character_id,
        canonical_case_id,
        witness_resident_character_id,
    )? {
        return Ok(());
    }
    let capabilities: Vec<_> = ctx
        .db
        .investigation_action_capability()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|capability| capability.case_id == canonical_case_id)
        .collect();
    let mut states: Vec<_> = capabilities
        .iter()
        .map(|capability| ReferredContactActionState {
            id: capability.id.clone(),
            owner_character_id: capability.owner_character_id,
            case_id: capability.case_id.clone(),
            method: capability.method.clone(),
            target_kind: capability.target_kind.clone(),
            target_id: capability.target_id.clone(),
            required_action_id: capability.required_action_id.clone(),
            active: capability.active,
            version: capability.version,
            successful_attempt: ctx
                .db
                .investigation_action_attempt()
                .capability_id()
                .filter(&capability.id)
                .any(|attempt| attempt.success),
        })
        .collect();
    let transition = transition_referred_contact_action(
        &mut states,
        owner_character_id,
        canonical_case_id,
        witness_resident_character_id,
    )?;
    let ReferredContactTransition::Applied {
        root_id,
        expected_version,
        next_version,
        activated_successor_ids,
        attempt_success,
        outcome_wording,
    } = transition
    else {
        return Ok(());
    };
    let mut capability = capabilities
        .into_iter()
        .find(|capability| capability.id == root_id)
        .ok_or("Referred contact action disappeared")?;
    if capability.version != expected_version || !capability.active {
        return Err("Referred contact action changed during transition planning".into());
    }
    let completed_at = character_strategic_minute(ctx, owner_character_id);
    let attempt_name = format!("dialogue:{dialogue_action_id}:{}", capability.id);
    let attempt_id = if capability.provenance_kind == InvestigationProvenanceKind::Generated {
        generated_observer_id(ctx, canonical_case_id, "attempt", &attempt_name)
            .ok_or("Generated contact action lacks observer-id authority")?
    } else {
        inv::compound_id(&["attempt", "manual-contact", &attempt_name])
    };
    ctx.db
        .investigation_action_attempt()
        .insert(InvestigationActionAttempt {
            id: attempt_id.clone(),
            capability_id: capability.id.clone(),
            owner_character_id,
            expected_version: capability.version,
            method: capability.method.clone(),
            started_at: completed_at,
            completed_at,
            duration_minutes: 0,
            success: attempt_success,
            resulting_uncertainty_bps: capability.uncertainty_bps,
            private_resolution_json: serde_json::json!({
                "source": "exact_referred_witness_dialogue"
            })
            .to_string(),
        });
    if attempt_success {
        let resolution = successful_referred_contact_resolution(capability.uncertainty_bps);
        persist_action_result_lead(ctx, &capability, &attempt_id, &resolution)?;
    }
    capability.active = false;
    capability.version = next_version;
    ctx.db
        .investigation_action_capability()
        .id()
        .update(capability.clone());
    let outcome_id = if capability.provenance_kind == InvestigationProvenanceKind::Generated {
        generated_observer_id(ctx, canonical_case_id, "outcome", &attempt_id)
            .ok_or("Generated contact action lacks observer-id authority")?
    } else {
        inv::compound_id(&["outcome", "manual-contact", &attempt_id])
    };
    if ctx
        .db
        .investigation_action_outcome()
        .id()
        .find(&outcome_id)
        .is_none()
    {
        ctx.db
            .investigation_action_outcome()
            .insert(InvestigationActionOutcome {
                id: outcome_id,
                owner_character_id,
                case_id: canonical_case_id.into(),
                capability_id: capability.id.clone(),
                attempt_id: String::new(),
                safe_wording: outcome_wording,
                recorded_at: character_strategic_minute(ctx, owner_character_id),
                official_recorded_at: official_minute(ctx),
            });
    }
    for successor_id in activated_successor_ids {
        let successor = ctx
            .db
            .investigation_action_capability()
            .id()
            .find(&successor_id)
            .ok_or("Referred contact successor disappeared")?;
        if successor.owner_character_id != owner_character_id
            || successor.case_id != canonical_case_id
            || successor.required_action_id != capability.id
        {
            return Err("Referred contact successor changed during transition planning".into());
        }
        set_action_active(ctx, &successor_id, true)?;
    }
    Ok(())
}

fn successful_referred_contact_resolution(uncertainty_bps: u16) -> action::Resolution {
    action::Resolution {
        result: action::ActionResultKind::ContactLocated,
        success: true,
        cost: action::StrategicCost {
            minutes: 0,
            fatigue: 0,
            food_units: 0,
            water_units: 0,
        },
        resulting_uncertainty_bps: uncertainty_bps,
        risk_bps: 0,
        risk_triggered: false,
        effective_skill_bps: 0,
    }
}

fn generated_action_graph_is_complete(
    expected_ids: &[String],
    existing: &[InvestigationActionCapability],
) -> Result<bool, String> {
    if existing.is_empty() {
        return Ok(false);
    }
    if existing.len() != expected_ids.len()
        || expected_ids
            .iter()
            .any(|expected| !existing.iter().any(|capability| capability.id == *expected))
    {
        return Err("Generated investigation action graph is partial".into());
    }
    Ok(true)
}

fn generated_initially_known_site_ids(
    manifest: &adventuresim_core::quest_generation::GeneratedCase,
) -> impl Iterator<Item = &str> {
    manifest
        .sites
        .iter()
        .filter(|site| site.exact_location_initially_known)
        .map(|site| site.id.0.as_str())
}

fn disclose_generated_initial_site_knowledge(
    ctx: &ReducerContext,
    owner_character_id: u64,
    manifest: &adventuresim_core::quest_generation::GeneratedCase,
) -> Result<(), String> {
    for site_id in generated_initially_known_site_ids(manifest) {
        let site = ctx
            .db
            .case_site_authority()
            .id_key()
            .find(site_id.to_owned())
            .ok_or("Initially known generated case site is missing")?;
        disclose_exact_case_site(
            ctx,
            owner_character_id,
            &manifest.public_case_id,
            &site,
            "known when the case was accepted",
        )?;
    }
    Ok(())
}

fn issue_rumor_action_graph(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
    lead_id: &str,
    settlement_id: &str,
    contact_id: &str,
    safe_summary: &str,
) -> Result<(), String> {
    if let Some(authority) = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(case_id.to_string())
    {
        let validated = validate_quest_generation_authority(&authority)?;
        let manifest = validated.manifest;
        let generation_context = validated.context;
        let expected_capability_ids = manifest
            .actions
            .iter()
            .map(|generated| {
                adventuresim_core::quest_generation::observer_scoped_id(
                    &generation_context,
                    "capability",
                    &format!("{owner_character_id}:{}", generated.id.0),
                )
            })
            .collect::<Vec<_>>();
        let existing_capabilities = action_route_capabilities(ctx, owner_character_id, case_id);
        if generated_action_graph_is_complete(&expected_capability_ids, &existing_capabilities)? {
            for capability in &existing_capabilities {
                validate_capability_blueprint_reducer(ctx, capability)?;
            }
            disclose_generated_initial_site_knowledge(ctx, owner_character_id, &manifest)?;
            return validate_action_route_graph(ctx, owner_character_id, case_id);
        }
        for target in &manifest.pattern_targets {
            let row = InvestigationPatternTargetAuthority {
                cohort_id: target.cohort_id.clone(),
                case_id: case_id.to_string(),
                resident_character_id: target.resident_character_id,
                demographic: format!("{:?}", target.demographic).to_ascii_lowercase(),
                age_band: target.age_band.clone(),
                sex: target.sex.clone(),
                profession: target.profession.clone(),
                expected_settlement_id: target.expected_settlement_id.clone(),
                expected_location: target.expected_location.clone(),
                presence_version: target.presence_version,
            };
            if let Some(existing) = ctx
                .db
                .investigation_pattern_target_authority()
                .cohort_id()
                .find(&row.cohort_id)
            {
                if existing.case_id != row.case_id
                    || existing.resident_character_id != row.resident_character_id
                    || existing.demographic != row.demographic
                    || existing.age_band != row.age_band
                    || existing.sex != row.sex
                    || existing.profession != row.profession
                    || existing.expected_settlement_id != row.expected_settlement_id
                    || existing.expected_location != row.expected_location
                    || existing.presence_version != row.presence_version
                {
                    return Err("Generated pattern target authority conflicts".into());
                }
            } else {
                ctx.db.investigation_pattern_target_authority().insert(row);
            }
        }
        for generated in &manifest.actions {
            let capability_id = adventuresim_core::quest_generation::observer_scoped_id(
                &generation_context,
                "capability",
                &format!("{owner_character_id}:{}", generated.id.0),
            );
            let remap = |id: &adventuresim_core::quest_generation::ActionId| {
                adventuresim_core::quest_generation::observer_scoped_id(
                    &generation_context,
                    "capability",
                    &format!("{owner_character_id}:{}", id.0),
                )
            };
            let consequence = generated
                .outputs
                .iter()
                .find_map(|output| match output {
                    adventuresim_core::quest_generation::GeneratedActionOutput::Consequence {
                        consequence:
                            adventuresim_core::quest_generation::GeneratedActionConsequence::RetrieveAsset {
                                asset_id,
                                next_version,
                            },
                    } => Some(InvestigationActionConsequence::RetrieveAsset {
                        asset_id: asset_id.clone(),
                        version: *next_version,
                    }),
                    adventuresim_core::quest_generation::GeneratedActionOutput::Consequence {
                        consequence:
                            adventuresim_core::quest_generation::GeneratedActionConsequence::RescueSubject {
                                subject_id,
                                next_version,
                            },
                    } => Some(InvestigationActionConsequence::RescueSubject {
                        subject_id: subject_id.clone(),
                        version: *next_version,
                    }),
                    _ => None,
                })
                .unwrap_or(InvestigationActionConsequence::None);
            let (known_prerequisites, safe_result_on_success) =
                generated_capability_safe_text(&manifest, generated);
            issue_investigation_action_capability(
                ctx,
                capability_id,
                owner_character_id,
                case_id.to_string(),
                InvestigationProvenanceKind::Generated,
                manifest.canonical_case_id.clone(),
                generated.kind,
                generated.target_kind.clone(),
                generated.target_id.clone(),
                generated_action_terrain(&manifest, generated),
                ctx.random::<u64>(),
                7_000,
                generated.safe_summary.clone(),
                known_prerequisites,
                safe_result_on_success,
                consequence,
                generated
                    .prerequisite
                    .as_ref()
                    .map_or_else(String::new, remap),
                remap(&generated.alternate),
            )?;
            ctx.db.investigation_generated_action_output().insert(
                InvestigationGeneratedActionOutput {
                    capability_id: adventuresim_core::quest_generation::observer_scoped_id(
                        &generation_context,
                        "capability",
                        &format!("{owner_character_id}:{}", generated.id.0),
                    ),
                    outputs_json: serde_json::to_string(&generated.outputs)
                        .map_err(|_| "Could not encode generated action outputs")?,
                },
            );
        }
        for generated in manifest
            .actions
            .iter()
            .filter(|action| action.active_initially)
        {
            set_action_active(
                ctx,
                &adventuresim_core::quest_generation::observer_scoped_id(
                    &generation_context,
                    "capability",
                    &format!("{owner_character_id}:{}", generated.id.0),
                ),
                true,
            )?;
        }
        disclose_generated_initial_site_knowledge(ctx, owner_character_id, &manifest)?;
        return validate_newly_issued_action_route_graph(ctx, owner_character_id, case_id);
    }
    let area_id = inv::compound_id(&["area", lead_id]);
    if ctx
        .db
        .investigation_area_authority()
        .id()
        .find(&area_id)
        .is_none()
    {
        let settlement = ctx
            .db
            .settlement()
            .id()
            .find(settlement_id.to_string())
            .ok_or("Rumor settlement no longer exists")?;
        ctx.db
            .investigation_area_authority()
            .insert(InvestigationAreaAuthority {
                id: area_id.clone(),
                case_id: case_id.to_string(),
                origin_settlement_id: settlement_id.to_string(),
                safe_label: "the area described by local accounts".into(),
                center_longitude_e7: (settlement.coord_x * 10_000_000.0) as i32,
                center_latitude_e7: (settlement.coord_y * 10_000_000.0) as i32,
                radius_m: 5_000,
                coordinates_are_geographic: settlement.source_node_id.is_some(),
                terrain: "settlement".into(),
            });
    }
    let canonical_case = ctx
        .db
        .case_authority()
        .iter()
        .find(|case| case.id == case_id || case.investigation_case_id == case_id);
    let site = canonical_case.as_ref().and_then(|case| {
        ctx.db
            .case_site_authority()
            .case_id()
            .filter(&case.id)
            .next()
    });
    let target_id = site
        .as_ref()
        .map_or_else(|| area_id.clone(), |site| site.id.value.clone());
    let target_kind = if site.is_some() { "site" } else { "area" };
    let terrain = site
        .as_ref()
        .and_then(|site| parse_action_terrain(&site.scene_key).ok())
        .unwrap_or(action::Terrain::Settlement);
    let ids = |method: &str| inv::compound_id(&["investigate", lead_id, method]);
    let locate = ids("locate_contact");
    let watch = ids("watch");
    let approach = ids("approach_lead");
    let patrol = ids("patrol");
    let search = ids("search_area");
    let reacquire = ids("reacquire_tracks");
    let follow = ids("follow_tracks");
    let ambush = ids("lay_ambush");
    let inspect = ids("inspect_site");
    if ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&locate)
        .is_some()
    {
        return validate_action_route_graph(ctx, owner_character_id, case_id);
    }
    let none = InvestigationActionConsequence::None;
    let specs = [
        (
            locate.clone(),
            action::InvestigationActionKind::LocateContact,
            "contact",
            contact_id.to_string(),
            action::Terrain::Settlement,
            "",
            watch.clone(),
            format!("Look for {safe_summary}"),
            "You locate someone who can clarify the report.".to_string(),
            none.clone(),
        ),
        (
            watch.clone(),
            action::InvestigationActionKind::Watch,
            "contact",
            contact_id.to_string(),
            action::Terrain::Settlement,
            "",
            locate.clone(),
            "Watch the public area for a corroborating account.".into(),
            "A local observation reveals another route.".into(),
            none.clone(),
        ),
        (
            approach.clone(),
            action::InvestigationActionKind::ApproachLead,
            "area",
            area_id.clone(),
            terrain,
            locate.as_str(),
            patrol.clone(),
            "Approach the lead described by the witness.".into(),
            "The witness's directions narrow the search.".into(),
            none.clone(),
        ),
        (
            patrol.clone(),
            action::InvestigationActionKind::Patrol,
            "area",
            area_id.clone(),
            terrain,
            watch.as_str(),
            approach.clone(),
            "Patrol the area implicated by the reports.".into(),
            "The patrol reveals a repeatable pattern.".into(),
            none.clone(),
        ),
        (
            search.clone(),
            action::InvestigationActionKind::SearchArea,
            "area",
            area_id.clone(),
            terrain,
            approach.as_str(),
            reacquire.clone(),
            "Search the narrowed area for physical evidence.".into(),
            "The search reveals a trail worth following.".into(),
            none.clone(),
        ),
        (
            reacquire.clone(),
            action::InvestigationActionKind::ReacquireTracks,
            target_kind,
            target_id.clone(),
            terrain,
            patrol.as_str(),
            search.clone(),
            "Reacquire a trail from the observed pattern.".into(),
            "The party picks up the trail again.".into(),
            none.clone(),
        ),
        (
            follow.clone(),
            action::InvestigationActionKind::FollowTracks,
            target_kind,
            target_id.clone(),
            terrain,
            search.as_str(),
            ambush.clone(),
            "Follow the physical trail toward its source.".into(),
            "The trail identifies where the threat is based.".into(),
            none.clone(),
        ),
        (
            ambush.clone(),
            action::InvestigationActionKind::LayAmbush,
            target_kind,
            target_id.clone(),
            terrain,
            reacquire.as_str(),
            follow.clone(),
            "Lay an ambush along the threat's established route.".into(),
            "The ambush is prepared at the threat's likely approach.".into(),
            none.clone(),
        ),
        (
            inspect.clone(),
            action::InvestigationActionKind::InspectSite,
            target_kind,
            target_id,
            terrain,
            follow.as_str(),
            ambush.clone(),
            "Inspect the identified site directly.".into(),
            "The site yields decisive evidence.".into(),
            none,
        ),
    ];
    for (
        id,
        kind,
        kind_name,
        target,
        terrain,
        required,
        alternate,
        summary,
        success,
        consequence,
    ) in specs
    {
        issue_investigation_action_capability(
            ctx,
            id,
            owner_character_id,
            case_id.to_string(),
            InvestigationProvenanceKind::Manual,
            String::new(),
            kind,
            kind_name.into(),
            target,
            terrain,
            ctx.random::<u64>(),
            if matches!(
                kind,
                action::InvestigationActionKind::FollowTracks
                    | action::InvestigationActionKind::ReacquireTracks
                    | action::InvestigationActionKind::InspectSite
            ) {
                2_500
            } else {
                7_000
            },
            summary,
            "Complete the preceding lead and remain with your ready, co-located party.".into(),
            success,
            consequence,
            required.into(),
            alternate,
        )?;
    }
    set_action_active(ctx, &locate, true)?;
    set_action_active(ctx, &watch, true)?;
    validate_newly_issued_action_route_graph(ctx, owner_character_id, case_id)
}
