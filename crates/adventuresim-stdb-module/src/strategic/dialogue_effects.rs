fn apply_dialogue_effect(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    source_scope: &str,
    action_id: &str,
    resulting_revision: u64,
    effect: &adventuresim_dialogue::Effect,
    testimony_event_sequence: Option<u32>,
) -> Result<(), String> {
    let live_npc = require_live_dialogue_presence(ctx, session, character_id)?;
    match effect {
        adventuresim_dialogue::Effect::LearnTopic { topic } => {
            let id = format!("{character_id}:{}:{topic}", session.conversation_id);
            if ctx.db.character_topic_knowledge().id().find(&id).is_none() {
                ctx.db
                    .character_topic_knowledge()
                    .insert(CharacterTopicKnowledge {
                        id,
                        character_id,
                        conversation_id: session.conversation_id.clone(),
                        topic_id: topic.clone(),
                        learned_micros: ctx.timestamp.to_micros_since_unix_epoch(),
                    });
            }
            Ok(())
        }
        adventuresim_dialogue::Effect::AcceptContract { contract }
            if contract != "selected-service-contract" =>
        {
            Err("Dialogue contracts must use the session-bound selection".into())
        }
        adventuresim_dialogue::Effect::ReportContract { contract }
            if contract != "selected-service-contract" =>
        {
            Err("Dialogue reports must use the session-bound active contract".into())
        }
        adventuresim_dialogue::Effect::BeginApprenticeship { profession } => {
            let service = if profession == "selected-service" {
                dialogue_service_id(ctx, session)?
            } else {
                profession.clone()
            };
            let organization_id =
                adventuresim_core::organization::organizations_for_chapter(&session.settlement_id)
                    .find(|organization| {
                        organization.service_id.as_deref() == Some(service.as_str())
                    })
                    .map(|organization| organization.id.clone())
                    .ok_or("No organization chapter offers that professional activity here")?;
            let entry_role_id = adventuresim_core::organization::organization(&organization_id)
                .and_then(|definition| definition.entry_role_ids.first())
                .cloned()
                .ok_or("Organization has no admission role")?;
            crate::organization::join_organization(
                ctx,
                character_id,
                organization_id,
                entry_role_id,
            )
        }
        adventuresim_dialogue::Effect::JoinOrganization => {
            let organization_id = dialogue_organization_id(ctx, session, &live_npc)?;
            let entry_role_id = adventuresim_core::organization::organization(&organization_id)
                .and_then(|definition| definition.entry_role_ids.first())
                .cloned()
                .ok_or("Organization has no admission role")?;
            crate::organization::join_organization(
                ctx,
                character_id,
                organization_id,
                entry_role_id,
            )
        }
        adventuresim_dialogue::Effect::PayOrganizationDues => {
            let organization_id = dialogue_organization_id(ctx, session, &live_npc)?;
            crate::organization::pay_organization_dues(ctx, character_id, organization_id)
        }
        adventuresim_dialogue::Effect::RequestOrganizationPromotion { to_role_id } => {
            let organization_id = dialogue_organization_id(ctx, session, &live_npc)?;
            let definition = adventuresim_core::organization::organization(&organization_id)
                .ok_or("Unknown organization")?;
            let current = crate::social_roles::assigned_organization_role(
                ctx,
                character_id,
                &organization_id,
            )?;
            let target = if let Some(to_role_id) = to_role_id {
                definition
                    .promotion_targets(&current.role_id)
                    .find(|role| role.id == *to_role_id)
                    .ok_or("The selected role is not a direct authored promotion")?
            } else {
                let mut targets = definition.promotion_targets(&current.role_id);
                let target = targets.next().ok_or("No promotion role is available")?;
                if targets.next().is_some() {
                    return Err("Choose a specific promotion role".into());
                }
                target
            };
            crate::organization::promote_organization_membership(
                ctx,
                character_id,
                organization_id,
                target.id.clone(),
            )
        }
        adventuresim_dialogue::Effect::PresentOrganization => {
            let organization_id = dialogue_organization_id(ctx, session, &live_npc)?;
            crate::organization::present_organization(ctx, character_id, organization_id)
        }
        adventuresim_dialogue::Effect::ReceiveReferredTestimony => {
            let event_sequence = testimony_event_sequence
                .ok_or("Testimony effect has no structured emitted claim event")?;
            receive_referred_testimony(ctx, character_id, session, &live_npc, action_id)?;
            crate::social::passively_assess_dialogue_witness(
                ctx,
                character_id,
                &session.id,
                event_sequence,
            )
        }
        adventuresim_dialogue::Effect::InvestigationAction { action } => {
            apply_dialogue_investigation_action(
                ctx,
                character_id,
                session,
                action,
                source_scope,
                action_id,
            )
        }
        adventuresim_dialogue::Effect::AcceptContract { .. } => {
            let service = dialogue_service_id(ctx, session)?;
            let contract_id = ctx
                .db
                .contract_authority()
                .service_id()
                .filter(&service)
                .find(|contract| {
                    contract.settlement_id == session.settlement_id
                        && contract.issuer_resident_character_id == live_npc.character_id
                        && contract.status == ContractStatus::Offered
                })
                .map(|contract| contract.id)
                .ok_or("This issuer has no available contract")?;
            record_dialogue_contract_issuer_interaction(
                ctx,
                character_id,
                contract_id.clone(),
                ContractInteractionStage::Accept,
                session,
                action_id,
                resulting_revision,
            )?;
            accept_contract(ctx, character_id, contract_id)
        }
        adventuresim_dialogue::Effect::ReportContract { .. } => {
            let character = ctx
                .db
                .character()
                .id()
                .find(character_id)
                .ok_or("Character not found")?;
            let party_id = character.party_id.ok_or("Character has no party")?;
            let contract_id = ctx
                .db
                .party_authority()
                .id()
                .find(&party_id)
                .and_then(|party| party.active_contract_id)
                .ok_or("Party has no active contract")?;
            let service = dialogue_service_id(ctx, session)?;
            let local_issuer = ctx
                .db
                .contract_authority()
                .id()
                .find(&contract_id)
                .is_some_and(|contract| {
                    contract.service_id == service
                        && contract.settlement_id == session.settlement_id
                        && contract.issuer_resident_character_id == live_npc.character_id
                });
            if !local_issuer {
                return Err("This NPC did not issue the active contract".into());
            }
            record_dialogue_contract_issuer_interaction(
                ctx,
                character_id,
                contract_id.clone(),
                ContractInteractionStage::Report,
                session,
                action_id,
                resulting_revision,
            )?;
            report_contract(ctx, character_id, contract_id)
        }
        adventuresim_dialogue::Effect::SetFlag { flag, value } if flag == "profess-local-faith" => {
            let religion_id = if *value {
                ctx.db
                    .settlement()
                    .id()
                    .find(&session.settlement_id)
                    .ok_or("Dialogue settlement not found")?
                    .religion_id
            } else {
                String::new()
            };
            crate::condition::set_character_religion(ctx, character_id, religion_id)
        }
        adventuresim_dialogue::Effect::SetFlag { .. } => Err("Unknown dialogue flag".into()),
    }
}

/// Resolve organization business only from the trusted persistent NPC and its
/// exact authored chapter. Dialogue content and clients never supply this ID.
pub(crate) fn exact_organization_representative(
    ctx: &ReducerContext,
    npc: &crate::settlement_population::SettlementResidentProfile,
    settlement_id: &str,
    location_id: &str,
) -> Option<String> {
    let organization = adventuresim_core::organization::organization(&npc.organization_id)?;
    let chapter = organization.chapter(settlement_id)?;
    let settlement = ctx.db.settlement().id().find(&settlement_id.to_owned())?;
    let observed_place =
        crate::settlement_population::canonical_npc_place(settlement_id, location_id)?;
    let effective_location = adventuresim_core::organization::chapter_effective_location_id(
        organization,
        chapter,
        &settlement.economy,
    );
    let effective_place =
        crate::settlement_population::canonical_npc_place(settlement_id, effective_location)?;
    if observed_place != effective_place {
        return None;
    }
    adventuresim_core::strategic_place::StrategicFixtureId::chapter(
        observed_place,
        &organization.id,
        &chapter.location_id,
    )
    .ok()?;
    let expected_id = adventuresim_core::organization::organization_representative_id(
        settlement_id,
        &organization.id,
    );
    if !adventuresim_core::organization::exact_representative_fields_match(
        npc.character_id,
        expected_id,
        &npc.home_settlement_id,
        settlement_id,
        &npc.organization_id,
        &organization.id,
        &npc.conversation_id,
    ) {
        return None;
    }
    Some(organization.id.clone())
}

fn dialogue_organization_id(
    ctx: &ReducerContext,
    session: &DialogueSession,
    npc: &crate::settlement_population::SettlementResidentProfile,
) -> Result<String, String> {
    exact_organization_representative(ctx, npc, &session.settlement_id, &session.location_id)
        .ok_or_else(|| "Dialogue NPC is not the representative of this chapter".into())
}

fn dialogue_service_id(ctx: &ReducerContext, session: &DialogueSession) -> Result<String, String> {
    ctx.db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find_map(|participant| {
            participant
                .character_id
                .is_none()
                .then(|| {
                    let resident_character_id = participant.actor_id.parse::<u64>().ok()?;
                    ctx.db
                        .settlement_resident_profile()
                        .character_id()
                        .find(resident_character_id)
                        .map(|npc| npc.service_id)
                })
                .flatten()
        })
        .ok_or("Dialogue has no service actor".into())
}

fn apply_dialogue_investigation_action(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    action: &adventuresim_dialogue::InvestigationAction,
    source_scope: &str,
    action_id: &str,
) -> Result<(), String> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    use adventuresim_core::case::OutcomeFactKind as F;

    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can perform case objectives".into());
    }
    let binding_id = dialogue_binding_id(
        &session.id,
        character_id,
        source_scope,
        action,
        session.revision,
    );
    let binding = ctx
        .db
        .dialogue_investigation_binding()
        .id()
        .find(&binding_id)
        .ok_or("Dialogue investigation action has no pre-issued binding")?;
    if binding.session_id != session.id
        || binding.character_id != character_id
        || binding.party_id != party_id
        || binding.action_family != format!("{action:?}")
        || binding.source_scope != source_scope
        || binding.issued_revision != session.revision
        || !binding.consumed_by.is_empty()
    {
        return Err("Dialogue investigation binding is stale, replayed, or conflicting".into());
    }
    let resident_character_ids: HashSet<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|row| row.character_id.is_none())
        .map(|row| row.actor_id)
        .collect();
    if !resident_character_ids.contains(&binding.intended_recipient_id) {
        return Err("Dialogue investigation recipient is no longer in this session".into());
    }
    let active_contract = party
        .active_contract_id
        .as_ref()
        .and_then(|id| ctx.db.contract_authority().id().find(id));
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&binding.case_id)
        .ok_or("Dialogue investigation case no longer exists")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Dialogue investigation case is no longer open".into());
    }
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    let objective = expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .find(|objective| objective.id.as_str() == binding.objective_id)
        .ok_or("Pre-issued dialogue objective no longer exists")?;
    let recipient = dialogue_objective_recipient(
        ctx,
        character_id,
        &party_id,
        &case,
        &objective.requirement,
        action,
        &resident_character_ids,
        &binding.intended_recipient_id,
        active_contract.as_ref(),
    )
    .ok_or("Pre-issued dialogue objective is no longer authorized")?;
    let recipient = generated_dialogue_recipient(
        ctx,
        &case,
        objective.id.as_str(),
        action,
        &resident_character_ids,
        recipient,
    )?
    .ok_or("Generated dialogue producer is no longer authorized")?;
    if recipient != binding.intended_recipient_id {
        return Err("Pre-issued dialogue recipient no longer matches".into());
    }
    let source_id = format!(
        "dialogue-objective:{}:{action_id}:{}",
        session.id,
        objective.id.as_str()
    );
    match &objective.requirement {
        R::Return {
            asset_id,
            custodian_id,
        } => {
            let current = ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .ok_or("Returned asset has no custody authority")?;
            if Some(current.version) != binding.expected_custody_version
                || current.case_id != case.id
                || current.holder_kind != CustodyHolderKind::Party
                || current.holder_id != party_id
            {
                return Err("Returned asset custody is stale or belongs elsewhere".into());
            }
            record_asset_returned_or_exchanged(
                ctx,
                &source_id,
                &case.id,
                &party_id,
                asset_id.as_str(),
                custodian_id,
                current.version.saturating_add(1),
                false,
            )?;
            let mut binding = binding;
            binding.consumed_by = action_id.into();
            ctx.db.dialogue_investigation_binding().id().update(binding);
            return Ok(());
        }
        R::Release { subject_id } => {
            let current = ctx
                .db
                .case_custody()
                .object_id()
                .find(&subject_id.as_str().to_string())
                .ok_or("Released subject has no custody authority")?;
            if Some(current.version) != binding.expected_custody_version
                || current.case_id != case.id
                || current.holder_kind != CustodyHolderKind::Party
                || current.holder_id != party_id
            {
                return Err("Released subject custody is stale or belongs elsewhere".into());
            }
            record_subject_rescued_or_released(
                ctx,
                &source_id,
                &case.id,
                &party_id,
                subject_id.as_str(),
                current.version.saturating_add(1),
                true,
            )?;
            let mut binding = binding;
            binding.consumed_by = action_id.into();
            ctx.db.dialogue_investigation_binding().id().update(binding);
            return Ok(());
        }
        R::Exchange {
            asset_id,
            recipient_id,
        } => {
            let current = ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .ok_or("Exchanged asset has no custody authority")?;
            if Some(current.version) != binding.expected_custody_version
                || current.case_id != case.id
                || current.holder_kind != CustodyHolderKind::Party
                || current.holder_id != party_id
            {
                return Err("Exchanged asset custody is stale or belongs elsewhere".into());
            }
            record_asset_returned_or_exchanged(
                ctx,
                &source_id,
                &case.id,
                &party_id,
                asset_id.as_str(),
                recipient_id,
                current.version.saturating_add(1),
                true,
            )?;
            let mut binding = binding;
            binding.consumed_by = action_id.into();
            ctx.db.dialogue_investigation_binding().id().update(binding);
            return Ok(());
        }
        _ => {}
    }
    let fact = match &objective.requirement {
        R::Locate { subject_ref } => F::Located {
            subject_ref: subject_ref.clone(),
        },
        R::Identify { subject_ref } => F::Identified {
            subject_ref: subject_ref.clone(),
        },
        R::Expose { subject_ref } => F::Exposed {
            subject_ref: subject_ref.clone(),
        },
        R::PresentProof {
            evidence_id,
            recipient_id,
        } => F::ProofPresented {
            evidence_id: evidence_id.clone(),
            recipient_id: recipient_id.clone(),
        },
        R::PresentTestimony {
            witness_id,
            recipient_id,
        } => F::TestimonyPresented {
            witness_id: witness_id.clone(),
            recipient_id: recipient_id.clone(),
        },
        R::Negotiate { subject_ref } => F::Negotiated {
            subject_ref: subject_ref.clone(),
        },
        R::ReportToIssuer { issuer_id } => F::Reported {
            issuer_id: issuer_id.clone(),
        },
        _ => return Err("Dialogue action selected the wrong objective family".into()),
    };
    ingest_case_outcome_fact(ctx, &source_id, &case.id, &party_id, fact)?;
    let mut binding = binding;
    binding.consumed_by = action_id.into();
    ctx.db.dialogue_investigation_binding().id().update(binding);
    Ok(())
}

fn same_location(ctx: &ReducerContext, left: &crate::Character, right: &crate::Character) -> bool {
    let left_site = crate::investigation::character_case_site_id(ctx, left.id);
    let right_site = crate::investigation::character_case_site_id(ctx, right.id);
    left.current_settlement_id == right.current_settlement_id
        && left_site == right_site
        && (left.current_settlement_id.is_some() || left_site.is_some())
}

fn player_conversation_parties(
    ctx: &ReducerContext,
    sender: &crate::Character,
    subject_id: u64,
) -> Result<(String, String), String> {
    let subject = ctx
        .db
        .character()
        .id()
        .find(subject_id)
        .ok_or("Conversation subject not found")?;
    if !same_location(ctx, sender, &subject) {
        return Err("Local conversations require a shared location".into());
    }
    let sender_party = sender.party_id.as_deref().ok_or("Sender has no party")?;
    let subject_party = subject.party_id.as_deref().ok_or("Subject has no party")?;
    Ok((sender_party.to_string(), subject_party.to_string()))
}

fn npc_conversation_authority_matches(
    settlement_id: &str,
    npc_home_settlement_id: &str,
    resident_character_id: u64,
    presence_resident_character_id: u64,
    presence_settlement_id: &str,
    presence_location_id: &str,
    requested_location_id: &str,
    presence_start_minute: u16,
    presence_end_minute: u16,
    minute: u64,
) -> bool {
    let minute = (minute % 1_440) as u16;
    npc_home_settlement_id == settlement_id
        && presence_resident_character_id == resident_character_id
        && presence_settlement_id == settlement_id
        && presence_location_id == requested_location_id
        && !requested_location_id.is_empty()
        && presence_start_minute <= minute
        && minute < presence_end_minute
}

fn npc_conversation_party(
    ctx: &ReducerContext,
    sender: &crate::Character,
    subject_id: &str,
    location_id: &str,
) -> Result<String, String> {
    let party_id = sender.party_id.as_deref().ok_or("Sender has no party")?;
    let settlement_id = sender
        .current_settlement_id
        .as_deref()
        .ok_or("NPC conversations require a settlement")?;
    require_navigable_npc_location(ctx, settlement_id, location_id)?;
    let resident_character_id = subject_id
        .parse::<u64>()
        .map_err(|_| "NPC is not at the sender's settlement")?;
    let npc = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(resident_character_id)
        .ok_or("NPC is not at the sender's settlement")?;
    let presence = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(resident_character_id)
        .ok_or("NPC is not at the sender's settlement")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(sender.id)
        .map_or(720, |time| time.minutes);
    if !npc_conversation_authority_matches(
        settlement_id,
        &npc.home_settlement_id,
        npc.character_id,
        presence.character_id,
        &presence.settlement_id,
        &presence.location_id,
        location_id,
        presence.start_minute,
        presence.end_minute,
        minute,
    ) || !crate::settlement_population::npc_is_present(ctx, &presence, minute)
    {
        return Err("NPC is not at the sender's settlement".into());
    }
    Ok(party_id.to_string())
}
