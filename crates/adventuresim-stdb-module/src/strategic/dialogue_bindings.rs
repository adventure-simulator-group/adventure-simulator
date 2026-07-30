fn receive_referred_testimony(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    live_npc: &crate::settlement_population::SettlementNpc,
    action_id: &str,
) -> Result<(), String> {
    let delivery = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|delivery| delivery.character_id == character_id)
        .ok_or("Dialogue has no exact local-problem referral")?;
    let ReferralDeliveryAuthority::LocalProblem(receipt) =
        referral_delivery_authority(ctx, &delivery)?
    else {
        return Err("Dialogue has no exact local-problem referral".into());
    };
    if receipt.character_id != character_id || receipt.settlement_id != session.settlement_id {
        return Err("Addressed NPC is not the exact referred witness here".into());
    }
    let (generated, witness) = referred_generated_witness(
        ctx,
        character_id,
        &receipt.opaque_case_ref,
        &live_npc.id,
        &session.settlement_id,
        &session.location_id,
    )?
    .ok_or("Addressed NPC is not the exact referred witness here")?;
    crate::investigation::persist_generated_testimony(
        ctx,
        character_id,
        &generated,
        &witness,
        None,
        action_id,
        false,
    )
}

pub(crate) fn dialogue_referred_witness(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    npc_id: &str,
) -> Result<
    Option<(
        adventuresim_core::quest_generation::GeneratedCase,
        adventuresim_core::quest_generation::WitnessBinding,
    )>,
    String,
> {
    let Some(delivery) = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|delivery| delivery.character_id == character_id)
    else {
        return Ok(None);
    };
    let ReferralDeliveryAuthority::LocalProblem(receipt) =
        referral_delivery_authority(ctx, &delivery)?
    else {
        return Ok(None);
    };
    referred_generated_witness(
        ctx,
        character_id,
        &receipt.opaque_case_ref,
        npc_id,
        &session.settlement_id,
        &session.location_id,
    )
}

pub(crate) struct ReferredTestimonyClaim {
    pub proposition_id: String,
    pub displayed_text: String,
    pub charm_response: Option<String>,
    pub command_response: Option<String>,
    pub bluff_response: Option<String>,
    pub claim_is_factually_accurate: bool,
    pub demeanor_truth_signal: f32,
}

/// Recover proposition-granular presentation from private generation authority.
/// The gateway never receives proposition IDs or reliability; only the social
/// module converts these rows into opaque observer-scoped claim projections.
pub(crate) fn referred_testimony_claims(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    npc_id: &str,
    withheld: bool,
    event_sequence: u32,
) -> Result<Vec<ReferredTestimonyClaim>, String> {
    let (_, witness) = dialogue_referred_witness(ctx, character_id, session, npc_id)?
        .ok_or("Dialogue has no bound witness testimony")?;
    let claims = witness
        .testimony
        .iter()
        .filter(|draft| {
            (draft.delivery == adventuresim_core::quest_generation::TestimonyDelivery::Withheld)
                == withheld
        })
        .map(|draft| {
            // The exact interactive substring and its optional responses are
            // authored with private testimony authority. The gateway later
            // requires this substring in the persisted utterance and fails
            // closed rather than sentence-splitting.
            let displayed_text = draft.challenge_text.clone();
            let authority = adventuresim_core::quest_generation::testimony_claim_authority(draft);
            Ok(ReferredTestimonyClaim {
                proposition_id: draft.proposition_id.clone(),
                displayed_text,
                charm_response: draft.challenge_responses.charm.clone(),
                command_response: draft.challenge_responses.command.clone(),
                bluff_response: draft.challenge_responses.bluff.clone(),
                claim_is_factually_accurate: authority.factually_accurate,
                demeanor_truth_signal: authority.demeanor_truth_signal,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let event = ctx
        .db
        .dialogue_event()
        .session_id()
        .filter(&session.id)
        .find(|event| event.sequence == event_sequence)
        .ok_or("Heard witness claim event is unavailable")?;
    let fragments: Vec<adventuresim_dialogue::ResolvedFragment> =
        serde_json::from_str(&event.fragments_json)
            .map_err(|_| "Heard witness claim event is malformed")?;
    let emitted = fragments
        .iter()
        .filter_map(|fragment| match fragment {
            adventuresim_dialogue::ResolvedFragment::Claim { value, claim_order } => {
                Some((*claim_order, value.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if emitted.len() != claims.len()
        || emitted.iter().zip(&claims).enumerate().any(
            |(order, ((emitted_order, emitted_text), claim))| {
                *emitted_order != order as u32 || *emitted_text != claim.displayed_text
            },
        )
    {
        return Err("Heard witness claim projection does not match private authority".into());
    }
    Ok(claims)
}

pub(crate) fn release_referred_withheld_testimony(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    npc_id: &str,
    action_id: &str,
) -> Result<u32, String> {
    let (generated, witness) = dialogue_referred_witness(ctx, character_id, session, npc_id)?
        .ok_or("Dialogue has no bound witness testimony")?;
    let released = witness
        .testimony
        .iter()
        .filter(|draft| {
            draft.delivery == adventuresim_core::quest_generation::TestimonyDelivery::Withheld
        })
        .map(|draft| adventuresim_dialogue::TestimonyLine {
            spoken_text: draft.spoken_text.trim().to_owned(),
            claim_text: draft.challenge_text.clone(),
        })
        .filter(|line| !line.spoken_text.is_empty())
        .collect::<Vec<_>>();
    if released.is_empty() {
        return Err("Bound witness testimony is unavailable or too large".into());
    }
    crate::investigation::persist_generated_testimony(
        ctx,
        character_id,
        &generated,
        &witness,
        None,
        action_id,
        true,
    )?;
    let speaker_role = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find(|participant| participant.actor_id == npc_id)
        .map(|participant| participant.role)
        .ok_or("Witness dialogue participant disappeared")?;
    let sequence = ctx
        .db
        .dialogue_event()
        .session_id()
        .filter(&session.id)
        .count() as u32;
    let mut bindings = adventuresim_dialogue::RuntimeBindings::default();
    bindings.bind_testimony(released);
    let fragments = bindings
        .resolve(&[adventuresim_dialogue::Fragment::Runtime {
            slot: adventuresim_dialogue::RuntimeSlot::Testimony,
        }])
        .map_err(|_| "Could not resolve released witness testimony")?;
    ctx.db.dialogue_event().insert(DialogueEvent {
        id: format!("{}:event:{sequence}", session.id),
        gateway_bucket: 0,
        session_id: session.id.clone(),
        sequence,
        response_id: format!("claim-release:{action_id}"),
        speaker_role,
        fragments_json: serde_json::to_string(&fragments)
            .map_err(|_| "Could not encode released witness testimony")?,
        source_refs_json: serde_json::to_string(&vec![
            Option::<adventuresim_dialogue::SourceRef>::None;
            fragments.len()
        ])
        .map_err(|_| "Could not encode released witness testimony sources")?,
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    Ok(sequence)
}

fn resolve_dialogue_fragments(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    speaker_role: &str,
    fragments: &[adventuresim_dialogue::Fragment],
) -> Result<Vec<adventuresim_dialogue::ResolvedFragment>, String> {
    if fragments
        .iter()
        .any(|fragment| matches!(fragment, adventuresim_dialogue::Fragment::Runtime { .. }))
    {
        dialogue_runtime_bindings(ctx, session, character_id, speaker_role)?
            .resolve(fragments)
            .map_err(|_| "Dialogue runtime binding is incomplete or unsafe".into())
    } else {
        fragments
            .iter()
            .map(|fragment| {
                adventuresim_dialogue::ResolvedFragment::from_authored(fragment)
                    .ok_or_else(|| "Dialogue fragment remained unresolved".into())
            })
            .collect()
    }
}

fn resolve_dialogue_turn(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    turn: &adventuresim_dialogue::Turn,
    authored_sources: &[Option<adventuresim_dialogue::SourceRef>],
) -> Result<adventuresim_dialogue::ResolvedTurn, String> {
    let bindings = if turn
        .fragments
        .iter()
        .any(|fragment| matches!(fragment, adventuresim_dialogue::Fragment::Runtime { .. }))
    {
        dialogue_runtime_bindings(ctx, session, character_id, &turn.speaker)?
    } else {
        adventuresim_dialogue::RuntimeBindings::default()
    };
    bindings
        .resolve_turn(turn, authored_sources)
        .map_err(|_| "Dialogue turn binding or source alignment is invalid".into())
}

fn dialogue_binding_id(
    session_id: &str,
    character_id: u64,
    source_scope: &str,
    action: &adventuresim_dialogue::InvestigationAction,
    revision: u64,
) -> String {
    format!("{session_id}:{character_id}:{source_scope}:{action:?}:{revision}")
}

fn observer_case_refs(ctx: &ReducerContext, case: &CaseAuthority) -> HashSet<String> {
    let mut refs = HashSet::from([case.id.clone(), case.investigation_case_id.clone()]);
    if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(&case.id)
        && let Ok(validated) = validate_quest_generation_authority(&authority)
    {
        refs.insert(validated.manifest.public_case_id);
    }
    refs
}

fn generated_dialogue_recipient(
    ctx: &ReducerContext,
    case: &CaseAuthority,
    objective_id: &str,
    action: &adventuresim_dialogue::InvestigationAction,
    npc_ids: &HashSet<String>,
    fallback: String,
) -> Result<Option<String>, String> {
    let authority = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&case.generated_case_id);
    let Some(manifest) = validated_generated_dialogue_manifest(case, authority.as_ref())? else {
        return Ok(Some(fallback));
    };
    Ok(generated_dialogue_producer_recipient(
        &manifest,
        objective_id,
        action,
        npc_ids,
    ))
}

fn validated_generated_dialogue_manifest(
    case: &CaseAuthority,
    authority: Option<&QuestGenerationAuthority>,
) -> Result<Option<adventuresim_core::quest_generation::GeneratedCase>, String> {
    if case.provenance_kind == "manual" && case.generated_case_id.is_empty() {
        return Ok(None);
    }
    if case.provenance_kind != "generated"
        || case.generated_case_id.is_empty()
        || case.generated_case_id != case.id
    {
        return Err("Case dialogue provenance is invalid".into());
    }
    let authority = authority.ok_or("Generated dialogue authority is missing")?;
    let validated = validate_quest_generation_authority(authority)?;
    let manifest = validated.manifest;
    if authority.case_id != manifest.canonical_case_id
        || authority.public_case_id != manifest.public_case_id
        || manifest.canonical_case_id != case.generated_case_id
        || serde_json::to_string(&manifest.objectives)
            .map_err(|_| "Generated objective authority is invalid")?
            != case.objective_expression_json
    {
        return Err("Generated dialogue authority does not match case provenance".into());
    }
    Ok(Some(manifest))
}

fn generated_dialogue_producer_recipient(
    manifest: &adventuresim_core::quest_generation::GeneratedCase,
    objective_id: &str,
    action: &adventuresim_dialogue::InvestigationAction,
    npc_ids: &HashSet<String>,
) -> Option<String> {
    manifest
        .dialogue_producers
        .iter()
        .find(|producer| {
            producer.objective_id.as_str() == objective_id
                && generated_dialogue_action_matches(producer.action, action)
        })
        .filter(|producer| npc_ids.contains(&producer.recipient_npc_id))
        .map(|producer| producer.recipient_npc_id.clone())
}

fn generated_dialogue_action_matches(
    generated: adventuresim_core::quest_generation::GeneratedDialogueAction,
    action: &adventuresim_dialogue::InvestigationAction,
) -> bool {
    matches!(
        (generated, action),
        (
            adventuresim_core::quest_generation::GeneratedDialogueAction::Expose,
            adventuresim_dialogue::InvestigationAction::Expose
        ) | (
            adventuresim_core::quest_generation::GeneratedDialogueAction::ReturnAsset,
            adventuresim_dialogue::InvestigationAction::ReturnAsset
        )
    )
}

fn evidence_can_be_presented(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    case: &CaseAuthority,
    evidence_id: &str,
) -> bool {
    let observer_case_refs = observer_case_refs(ctx, case);
    let Some(evidence) = ctx
        .db
        .investigation_evidence_authority()
        .id()
        .find(&evidence_id.to_string())
    else {
        return false;
    };
    if !observer_case_refs.contains(&evidence.case_id) {
        return false;
    }
    match evidence.presentation_kind {
        EvidencePresentationKind::Physical => ctx
            .db
            .case_custody()
            .object_id()
            .find(&evidence_id.to_string())
            .is_some_and(|custody| {
                custody.case_id == case.id
                    && ((custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id)
                        || (custody.holder_kind == CustodyHolderKind::Character
                            && custody.holder_id == character_id.to_string()))
            }),
        EvidencePresentationKind::Informational => ctx
            .db
            .investigation_evidence_knowledge()
            .owner_character_id()
            .filter(character_id)
            .any(|knowledge| {
                observer_case_refs.contains(&knowledge.case_id)
                    && knowledge.evidence_id == evidence_id
            }),
    }
}

fn dialogue_objective_recipient(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    case: &CaseAuthority,
    requirement: &adventuresim_core::case::ObjectiveRequirement,
    action: &adventuresim_dialogue::InvestigationAction,
    npc_ids: &HashSet<String>,
    fallback_recipient_id: &str,
    active_contract: Option<&Contract>,
) -> Option<String> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    use adventuresim_dialogue::InvestigationAction as A;
    let observer_case_refs = observer_case_refs(ctx, case);

    match (requirement, action) {
        (R::Locate { subject_ref }, A::Locate) => ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(character_id)
            .any(|lead| {
                observer_case_refs.contains(&lead.case_id)
                    && matches!(
                        lead.destination_stage.as_str(),
                        "exact_believed" | "visited"
                    )
                    && lead.exact_location_id == subject_ref.as_str()
                    && ctx
                        .db
                        .case_site_authority()
                        .id_key()
                        .find(&lead.exact_location_id)
                        .is_some_and(|site| site.case_id == case.id)
            })
            .then(|| fallback_recipient_id.into()),
        (R::Identify { subject_ref }, A::Identify) | (R::Expose { subject_ref }, A::Expose) => ctx
            .db
            .investigation_belief()
            .owner_character_id()
            .filter(character_id)
            .any(|belief| {
                observer_case_refs.contains(&belief.case_id)
                    && belief.proposition_id == subject_ref.as_str()
            })
            .then(|| fallback_recipient_id.into()),
        (R::Negotiate { subject_ref }, A::Negotiate) => (npc_ids.contains(subject_ref.as_str())
            && ctx
                .db
                .investigation_belief()
                .owner_character_id()
                .filter(character_id)
                .any(|belief| {
                    observer_case_refs.contains(&belief.case_id)
                        && belief.proposition_id == subject_ref.as_str()
                }))
        .then(|| subject_ref.as_str().into()),
        (
            R::PresentProof {
                evidence_id,
                recipient_id,
            },
            A::PresentProof,
        ) => (npc_ids.contains(recipient_id.as_str())
            && evidence_can_be_presented(ctx, character_id, party_id, case, evidence_id.as_str()))
        .then(|| recipient_id.as_str().into()),
        (
            R::PresentTestimony {
                witness_id,
                recipient_id,
            },
            A::PresentTestimony,
        ) => (npc_ids.contains(witness_id.as_str())
            && npc_ids.contains(recipient_id.as_str())
            && ctx
                .db
                .investigation_received_testimony()
                .owner_character_id()
                .filter(character_id)
                .any(|received| {
                    observer_case_refs.contains(&received.public_case_id)
                        && received.witness_ref == witness_id.as_str()
                }))
        .then(|| recipient_id.as_str().into()),
        (
            R::Return {
                asset_id,
                custodian_id,
            },
            A::ReturnAsset,
        ) => (npc_ids.contains(custodian_id.as_str())
            && ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .is_some_and(|custody| {
                    custody.case_id == case.id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                }))
        .then(|| custodian_id.as_str().into()),
        (R::Release { subject_id }, A::ReleaseSubject) => (npc_ids.contains(subject_id.as_str())
            && ctx
                .db
                .case_custody()
                .object_id()
                .find(&subject_id.as_str().to_string())
                .is_some_and(|custody| {
                    custody.case_id == case.id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                }))
        .then(|| subject_id.as_str().into()),
        (
            R::Exchange {
                asset_id,
                recipient_id,
            },
            A::ExchangeAsset,
        ) => (npc_ids.contains(recipient_id.as_str())
            && ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .is_some_and(|custody| {
                    custody.case_id == case.id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                }))
        .then(|| recipient_id.as_str().into()),
        (R::ReportToIssuer { issuer_id }, A::ReportToIssuer) => (npc_ids
            .contains(issuer_id.as_str())
            && active_contract.is_some_and(|contract| {
                contract.case_id == case.id && contract.issuer_npc_id == issuer_id.as_str()
            }))
        .then(|| issuer_id.as_str().into()),
        _ => None,
    }
}

fn issue_dialogue_investigation_bindings(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    source_scope: &str,
    issued_revision: u64,
    effects: &[adventuresim_dialogue::Effect],
) -> Result<bool, String> {
    let actions: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            adventuresim_dialogue::Effect::InvestigationAction { action } => Some(action),
            _ => None,
        })
        .collect();
    if actions.is_empty() {
        return Ok(true);
    }
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Ok(false);
    }
    let npc_ids: HashSet<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|participant| participant.character_id.is_none())
        .map(|participant| participant.actor_id)
        .collect();
    let fallback_recipient_id = npc_ids
        .iter()
        .next()
        .ok_or("Dialogue has no NPC participant")?;
    let active_contract = party
        .active_contract_id
        .as_ref()
        .and_then(|id| ctx.db.contract_authority().id().find(id));

    // These are exact, session-relevant observer-safe provenance sources. A
    // character merely knowing some other case never makes this dialogue
    // eligible to advance it.
    let mut exact_case_refs = HashSet::new();
    for delivery in ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .filter(|delivery| delivery.character_id == character_id)
    {
        if let Some(receipt) = ctx
            .db
            .local_problem_receipt()
            .id()
            .find(&delivery.receipt_id)
        {
            exact_case_refs.insert(receipt.opaque_case_ref);
        }
    }
    for received in ctx
        .db
        .investigation_received_testimony()
        .owner_character_id()
        .filter(character_id)
        .filter(|received| npc_ids.contains(&received.witness_ref))
    {
        exact_case_refs.insert(received.public_case_id);
    }
    for action in &actions {
        match action {
            adventuresim_dialogue::InvestigationAction::PresentProof
            | adventuresim_dialogue::InvestigationAction::ReturnAsset
            | adventuresim_dialogue::InvestigationAction::ReleaseSubject
            | adventuresim_dialogue::InvestigationAction::ExchangeAsset => {
                exact_case_refs.extend(
                    ctx.db
                        .investigation_evidence_knowledge()
                        .owner_character_id()
                        .filter(character_id)
                        .map(|knowledge| knowledge.case_id),
                );
                exact_case_refs.extend(
                    ctx.db
                        .case_custody()
                        .iter()
                        .filter(|custody| {
                            (custody.holder_kind == CustodyHolderKind::Party
                                && custody.holder_id == party_id)
                                || (custody.holder_kind == CustodyHolderKind::Character
                                    && custody.holder_id == character_id.to_string())
                        })
                        .map(|custody| custody.case_id),
                );
            }
            adventuresim_dialogue::InvestigationAction::Negotiate => {
                exact_case_refs.extend(
                    ctx.db
                        .investigation_belief()
                        .owner_character_id()
                        .filter(character_id)
                        .filter(|belief| npc_ids.contains(&belief.proposition_id))
                        .map(|belief| belief.case_id),
                );
            }
            adventuresim_dialogue::InvestigationAction::Identify
            | adventuresim_dialogue::InvestigationAction::Expose => {
                exact_case_refs.extend(
                    ctx.db
                        .investigation_belief()
                        .owner_character_id()
                        .filter(character_id)
                        .map(|belief| belief.case_id),
                );
            }
            adventuresim_dialogue::InvestigationAction::ReportToIssuer => {
                if let Some(contract) = &active_contract
                    && npc_ids.contains(&contract.issuer_npc_id)
                {
                    exact_case_refs.insert(contract.case_id.clone());
                }
            }
            _ => {}
        }
    }

    let mut pending = Vec::new();
    for action in actions {
        let mut matches = Vec::new();
        for case in ctx
            .db
            .case_authority()
            .iter()
            .filter(|case| case_has_exact_dialogue_provenance(ctx, case, &exact_case_refs))
        {
            if case.resolution_status != CaseResolutionStatus::Open {
                continue;
            }
            let expression: adventuresim_core::case::ObjectiveExpression =
                serde_json::from_str(&case.objective_expression_json)
                    .map_err(|_| "Case objective authority is invalid")?;
            for objective in expression
                .alternatives
                .iter()
                .flat_map(|path| &path.objectives)
            {
                if let Some(recipient_id) = dialogue_objective_recipient(
                    ctx,
                    character_id,
                    &party_id,
                    &case,
                    &objective.requirement,
                    action,
                    &npc_ids,
                    fallback_recipient_id,
                    active_contract.as_ref(),
                ) && let Some(recipient_id) = generated_dialogue_recipient(
                    ctx,
                    &case,
                    objective.id.as_str(),
                    action,
                    &npc_ids,
                    recipient_id,
                )? {
                    let expected_custody_version = match &objective.requirement {
                        adventuresim_core::case::ObjectiveRequirement::Return {
                            asset_id, ..
                        }
                        | adventuresim_core::case::ObjectiveRequirement::Exchange {
                            asset_id,
                            ..
                        } => ctx
                            .db
                            .case_custody()
                            .object_id()
                            .find(&asset_id.as_str().to_string())
                            .map(|row| row.version),
                        adventuresim_core::case::ObjectiveRequirement::Release { subject_id } => {
                            ctx.db
                                .case_custody()
                                .object_id()
                                .find(&subject_id.as_str().to_string())
                                .map(|row| row.version)
                        }
                        _ => None,
                    };
                    matches.push((
                        case.id.clone(),
                        objective.id.as_str().to_string(),
                        recipient_id,
                        expected_custody_version,
                    ));
                }
            }
        }
        if matches.is_empty() {
            return Ok(false);
        }
        if matches.len() != 1 {
            return Err("Dialogue objective authority is ambiguous for this response".into());
        }
        let (case_id, objective_id, intended_recipient_id, expected_custody_version) =
            matches.pop().expect("exactly one binding candidate");
        pending.push((
            action,
            case_id,
            objective_id,
            intended_recipient_id,
            expected_custody_version,
        ));
    }

    for (action, case_id, objective_id, intended_recipient_id, expected_custody_version) in pending
    {
        let id = dialogue_binding_id(
            &session.id,
            character_id,
            source_scope,
            action,
            issued_revision,
        );
        if let Some(existing) = ctx.db.dialogue_investigation_binding().id().find(&id) {
            if existing.party_id != party_id
                || existing.intended_recipient_id != intended_recipient_id
                || existing.case_id != case_id
                || existing.objective_id != objective_id
                || existing.expected_custody_version != expected_custody_version
                || !existing.consumed_by.is_empty()
            {
                return Err("Dialogue investigation binding conflicts with prior authority".into());
            }
            continue;
        }
        ctx.db
            .dialogue_investigation_binding()
            .insert(DialogueInvestigationBinding {
                id,
                session_id: session.id.clone(),
                character_id,
                party_id: party_id.clone(),
                intended_recipient_id,
                action_family: format!("{action:?}"),
                source_scope: source_scope.into(),
                case_id,
                objective_id,
                expected_custody_version,
                issued_revision,
                consumed_by: String::new(),
            });
    }
    Ok(true)
}

fn case_has_exact_dialogue_provenance(
    ctx: &ReducerContext,
    case: &CaseAuthority,
    exact_case_refs: &HashSet<String>,
) -> bool {
    case_refs_have_exact_dialogue_provenance(&observer_case_refs(ctx, case), exact_case_refs)
}

fn case_refs_have_exact_dialogue_provenance(
    observer_case_refs: &HashSet<String>,
    exact_case_refs: &HashSet<String>,
) -> bool {
    observer_case_refs
        .iter()
        .any(|case_ref| exact_case_refs.contains(case_ref))
}

fn dialogue_public_case_id(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    effects: &[(&str, u64, &adventuresim_dialogue::Effect)],
) -> Result<String, String> {
    let mut public_case_ids = BTreeSet::new();
    for (source_scope, issued_revision, effect) in effects {
        match effect {
            adventuresim_dialogue::Effect::InvestigationAction { action } => {
                let binding_id = dialogue_binding_id(
                    &session.id,
                    character_id,
                    source_scope,
                    action,
                    *issued_revision,
                );
                let binding = ctx
                    .db
                    .dialogue_investigation_binding()
                    .id()
                    .find(&binding_id)
                    .ok_or("Projected dialogue action has no exact case binding")?;
                let case = ctx
                    .db
                    .case_authority()
                    .id()
                    .find(&binding.case_id)
                    .ok_or("Projected dialogue case disappeared")?;
                let authority = (!case.generated_case_id.is_empty())
                    .then(|| {
                        ctx.db
                            .quest_generation_authority()
                            .case_id()
                            .find(&case.generated_case_id)
                    })
                    .flatten();
                let public_case_id =
                    validated_generated_dialogue_manifest(&case, authority.as_ref())?
                        .map_or(case.id, |manifest| manifest.public_case_id);
                public_case_ids.insert(public_case_id);
            }
            adventuresim_dialogue::Effect::ReceiveReferredTestimony => {
                let generated = ctx
                    .db
                    .dialogue_participant()
                    .session_id()
                    .filter(&session.id)
                    .filter(|participant| participant.character_id.is_none())
                    .find_map(|participant| {
                        dialogue_referred_witness(ctx, character_id, session, &participant.actor_id)
                            .transpose()
                    })
                    .transpose()?;
                if let Some((manifest, _)) = generated {
                    public_case_ids.insert(manifest.public_case_id);
                }
            }
            _ => {}
        }
    }
    match public_case_ids.len() {
        0 => Ok(String::new()),
        1 => Ok(public_case_ids.pop_first().expect("one public case ID")),
        _ => Err("Dialogue topic would advance more than one public case".into()),
    }
}

fn refresh_dialogue_topic_options(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
) -> Result<(), String> {
    let conversation = adventuresim_dialogue::find_conversation(&session.conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    let facts = dialogue_fact_context(ctx, session, character_id)?;
    let existing: Vec<_> = ctx
        .db
        .dialogue_topic_option()
        .session_id()
        .filter(&session.id)
        .map(|row| row.id)
        .collect();
    for id in existing {
        ctx.db.dialogue_topic_option().id().delete(&id);
    }
    for topic in &conversation.topics {
        let known = topic.initially_known
            || ctx
                .db
                .character_topic_knowledge()
                .character_id()
                .filter(character_id)
                .any(|row| {
                    row.conversation_id == session.conversation_id && row.topic_id == topic.id
                });
        if known && facts.matches(&topic.conditions) {
            let response = adventuresim_dialogue::select_response(topic, &facts)
                .map_err(|_| "No unambiguous eligible dialogue response")?;
            let response_scope = format!("topic:{}:{}", topic.id, response.id);
            let response_is_bound = issue_dialogue_investigation_bindings(
                ctx,
                character_id,
                session,
                &response_scope,
                session.revision,
                &response.effects,
            )?;
            let choices_are_bound = if let Some(prompt) = &response.prompt {
                let mut bound = true;
                for choice in &prompt.choices {
                    let choice_scope =
                        format!("prompt:{}:{}:{}", prompt.id, response.id, choice.id);
                    bound &= issue_dialogue_investigation_bindings(
                        ctx,
                        character_id,
                        session,
                        &choice_scope,
                        session.revision.saturating_add(1),
                        &choice.effects,
                    )?;
                }
                bound
            } else {
                true
            };
            if !response_is_bound || !choices_are_bound {
                continue;
            }
            let mut case_effects = response
                .effects
                .iter()
                .map(|effect| (response_scope.as_str(), session.revision, effect))
                .collect::<Vec<_>>();
            let mut choice_scopes = Vec::new();
            if let Some(prompt) = &response.prompt {
                for choice in &prompt.choices {
                    choice_scopes.push(format!(
                        "prompt:{}:{}:{}",
                        prompt.id, response.id, choice.id
                    ));
                }
                for (choice, choice_scope) in prompt.choices.iter().zip(&choice_scopes) {
                    case_effects.extend(choice.effects.iter().map(|effect| {
                        (
                            choice_scope.as_str(),
                            session.revision.saturating_add(1),
                            effect,
                        )
                    }));
                }
            }
            let public_case_id =
                dialogue_public_case_id(ctx, character_id, session, &case_effects)?;
            ctx.db.dialogue_topic_option().insert(DialogueTopicOption {
                id: format!("{}:{}", session.id, topic.id),
                gateway_bucket: 0,
                session_id: session.id.clone(),
                topic_id: topic.id.clone(),
                public_case_id,
                label: topic.label.clone(),
                source_ref_json: serde_json::to_string(&adventuresim_dialogue::source_for_topic(
                    &session.conversation_id,
                    &topic.id,
                ))
                .map_err(|_| "Could not encode topic source")?,
            });
        }
    }
    if let Some(npc_id) = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find(|participant| participant.character_id.is_none())
        .map(|participant| participant.actor_id)
    {
        for (topic_id, label) in
            crate::corpse::permission_topics_for_npc(ctx, character_id, &npc_id)
        {
            ctx.db.dialogue_topic_option().insert(DialogueTopicOption {
                id: format!("{}:{topic_id}", session.id),
                gateway_bucket: 0,
                session_id: session.id.clone(),
                topic_id,
                public_case_id: String::new(),
                label,
                source_ref_json: "[]".into(),
            });
        }
    }
    Ok(())
}
