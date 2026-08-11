#[reducer]
pub fn start_dialogue(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: String,
    conversation_id: String,
    npc_actor_id: String,
    location_id: String,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let owner_party_id = character
        .party_id
        .clone()
        .ok_or("Dialogue requires a party")?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Dialogue requires a settlement")?;
    // The route value selects a candidate only; server-side settlement
    // navigability must resolve it before it becomes exact actor context.
    let exact_place = require_navigable_npc_place(ctx, &settlement_id, &location_id)?;
    let npc_character_id = npc_actor_id
        .parse::<u64>()
        .map_err(|_| "Dialogue NPC identity is invalid")?;
    let minute = crate::relationship::enforce_temporal_scope(
        ctx,
        character_id,
        Some(npc_character_id),
        crate::relationship::TemporalScope::PairwiseSoft,
    )?;
    let actor_settlement_presence =
        adventuresim_core::strategic_presence::StrategicPresence::settlement_membership(
            character_id,
            settlement_id.clone(),
            adventuresim_core::strategic_presence::PresenceFrontier {
                observer_character_id: character_id,
                personal_minute: minute,
            },
        )
        .map_err(|_| "Dialogue settlement identity is invalid")?;
    let actor_presence =
        adventuresim_core::strategic_presence::StrategicPresence::validated_venue_selection(
            &actor_settlement_presence,
            exact_place,
        )
        .map_err(|_| "Dialogue location is not in the actor's settlement")?;
    let npc = crate::settlement_population::resolve_settlement_resident(ctx, npc_character_id)
        .ok_or("Dialogue actor is not a persistent settlement NPC")?;
    if npc.home_settlement_id != settlement_id {
        return Err("Dialogue actor is not at this settlement".into());
    }
    if (npc.organization_id.is_empty() && npc.conversation_id == "organization-representative")
        || (!npc.organization_id.is_empty()
            && exact_organization_representative(ctx, &npc, &settlement_id, &location_id).is_none())
    {
        return Err("Dialogue actor has no exact authority at this location".into());
    }
    let presence = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(npc_character_id)
        .ok_or("Dialogue actor has no authoritative presence")?;
    let npc_presence = crate::settlement_population::npc_strategic_presence_at(
        ctx,
        &presence,
        character_id,
        minute,
    )
    .ok_or("Dialogue actor is not present at this time")?;
    if !adventuresim_core::strategic_presence::are_co_present(
        &actor_presence,
        npc_presence.presence(),
    ) {
        return Err("Dialogue actor is not present at this time".into());
    }
    if conversation_id != npc.conversation_id {
        return Err("Dialogue conversation is not valid for this NPC".into());
    }
    let conversation = adventuresim_dialogue::find_conversation(&conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    adventuresim_dialogue::validate(adventuresim_dialogue::catalog())
        .map_err(|_| "Dialogue catalog is invalid")?;
    let player_role = conversation
        .roles
        .iter()
        .find(|(_, role)| role.kind == adventuresim_dialogue::ParticipantKind::Player)
        .map(|(name, _)| name.clone())
        .ok_or("Conversation has no player role")?;
    if !conversation
        .roles
        .values()
        .any(|role| role.kind == adventuresim_dialogue::ParticipantKind::Npc)
    {
        return Err("Conversation has no NPC role".into());
    }
    if !session_id.starts_with(&format!("dialogue:{character_id}:"))
        || session_id.len() > 160
        || session_id.chars().any(char::is_control)
    {
        return Err("Invalid dialogue session ID".into());
    }
    if let Some(existing) = ctx.db.dialogue_session().id().find(&session_id) {
        return if existing.conversation_id == conversation_id
            && existing.settlement_id == settlement_id
            && existing.location_id == location_id
            && existing.catalog_revision == catalog_revision
        {
            require_live_dialogue_presence(ctx, &existing, character_id).map(|_| ())
        } else {
            Err("Dialogue session ID conflicts with another request".into())
        };
    }
    let id = session_id;
    ctx.db.dialogue_session().insert(DialogueSession {
        id: id.clone(),
        gateway_bucket: 0,
        conversation_id,
        catalog_revision,
        settlement_id: settlement_id.clone(),
        location_id: location_id.clone(),
        owner_character_id: character_id,
        owner_party_id,
        state: "active".into(),
        revision: 0,
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    ctx.db.dialogue_participant().insert(DialogueParticipant {
        id: format!("{id}:character:{character_id}"),
        gateway_bucket: 0,
        session_id: id.clone(),
        role: player_role,
        character_id: Some(character_id),
        actor_id: format!("character:{character_id}"),
        display_name: character.name.clone(),
    });
    let available_npcs: Vec<_> = ctx
        .db
        .settlement_resident_presence()
        .settlement_id()
        .filter(&settlement_id)
        .filter(|candidate| {
            crate::settlement_population::npc_strategic_presence_at(
                ctx,
                candidate,
                character_id,
                minute,
            )
                .is_some_and(|candidate_presence| {
                    adventuresim_core::strategic_presence::are_co_present(
                        &actor_presence,
                        candidate_presence.presence(),
                    )
                })
        })
        .filter_map(|presence| {
            crate::settlement_population::resolve_settlement_resident(ctx, presence.character_id)
        })
        .collect();
    let mut used_npcs = HashSet::new();
    for (index, (role_name, role)) in conversation
        .roles
        .iter()
        .filter(|(_, role)| role.kind == adventuresim_dialogue::ParticipantKind::Npc)
        .enumerate()
    {
        if role.min > 1 {
            return Err("Dialogue currently supports one persistent actor per NPC role".into());
        }
        let bound = if index == 0 {
            npc.clone()
        } else {
            available_npcs
                .iter()
                .find(|candidate| {
                    candidate.character_id != npc_character_id
                        && !used_npcs.contains(&candidate.character_id)
                })
                .cloned()
                .ok_or("Required NPC role has no persistent actor at this location")?
        };
        used_npcs.insert(bound.character_id);
        ctx.db.dialogue_participant().insert(DialogueParticipant {
            id: format!("{id}:npc:{role_name}"),
            gateway_bucket: 0,
            session_id: id.clone(),
            role: role_name.clone(),
            character_id: None,
            display_name: bound.name.clone(),
            actor_id: bound.character_id.to_string(),
        });
    }
    let session = ctx
        .db
        .dialogue_session()
        .id()
        .find(&id)
        .ok_or("Dialogue session not found")?;
    validate_dialogue_cardinality(ctx, &session, conversation)?;
    require_live_dialogue_presence(ctx, &session, character_id)?;
    if !conversation.on_start.is_empty() {
        let facts = dialogue_fact_context(ctx, &session, character_id)?;
        let response = adventuresim_dialogue::select_start_response(conversation, &facts)
            .map_err(|_| "No unambiguous eligible conversation greeting")?;
        for (turn_index, turn) in response.turns.iter().enumerate() {
            let source_refs: Vec<_> = turn
                .fragments
                .iter()
                .enumerate()
                .map(|(fragment_index, authored)| {
                    let field = match authored {
                        adventuresim_dialogue::Fragment::Text { .. } => "value",
                        adventuresim_dialogue::Fragment::Topic { .. } => "label",
                        adventuresim_dialogue::Fragment::PeriodClaim { .. } => "value",
                        adventuresim_dialogue::Fragment::AuthoritativeExplanation { .. } => "value",
                        adventuresim_dialogue::Fragment::Runtime { .. } => "slot",
                    };
                    adventuresim_dialogue::source_for_start_fragment(
                        &session.conversation_id,
                        &response.id,
                        turn_index,
                        fragment_index,
                        field,
                    )
                })
                .collect();
            ctx.db.dialogue_event().insert(DialogueEvent {
                id: format!("{}:event:{turn_index}", session.id),
                gateway_bucket: 0,
                session_id: session.id.clone(),
                sequence: turn_index as u32,
                response_id: response.id.clone(),
                speaker_role: turn.speaker.clone(),
                fragments_json: serde_json::to_string(&resolve_dialogue_fragments(
                    ctx,
                    &session,
                    character_id,
                    &turn.speaker,
                    Some(&turn.addressee),
                    &turn.fragments,
                )?)
                .map_err(|_| "Could not encode dialogue greeting")?,
                source_refs_json: serde_json::to_string(&source_refs)
                    .map_err(|_| "Could not encode dialogue greeting sources")?,
                created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
            });
        }
        for effect in &response.effects {
            let source_scope = format!("start:{}", response.id);
            apply_dialogue_effect(
                ctx,
                character_id,
                &session,
                &source_scope,
                "start",
                session.revision,
                effect,
                None,
            )?;
        }
    }
    crate::local_problem::surface_problem(
        ctx,
        character_id,
        &session.id,
        npc_character_id,
        &session.location_id,
    )?;
    // Entering a tavern (or the overview fallback) is itself the reliable,
    // markerless discovery action. Persist the observer-safe journal lead
    // immediately; it does not accept a contract or disclose a hidden cause.
    if let Some(delivery) = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|row| row.character_id == character_id)
    {
        let authority = referral_delivery_authority(ctx, &delivery)?;
        let speaker_role = ctx
            .db
            .dialogue_participant()
            .session_id()
            .filter(&session.id)
            .find(|participant| participant.actor_id == npc_actor_id)
            .map(|participant| participant.role)
            .ok_or("Rumor delivery speaker disappeared")?;
        let sequence = ctx
            .db
            .dialogue_event()
            .session_id()
            .filter(&session.id)
            .count() as u32;
        let (fragments_json, source_refs_json) = match &authority {
            ReferralDeliveryAuthority::PublicThreat => (
                delivery.fragments_json.clone(),
                serde_json::to_string(std::slice::from_ref(&delivery.receipt_id))
                    .map_err(|_| "Could not encode public referral source")?,
            ),
            ReferralDeliveryAuthority::LocalProblem(_) => render_quest_referral_variant(
                ctx,
                &session,
                character_id,
                npc_character_id,
                &delivery,
            )?,
        };
        ctx.db.dialogue_event().insert(DialogueEvent {
            id: format!("{}:event:{sequence}", session.id),
            gateway_bucket: 0,
            session_id: session.id.clone(),
            sequence,
            response_id: "generated-referral".into(),
            speaker_role,
            fragments_json,
            source_refs_json,
            created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
        if let ReferralDeliveryAuthority::LocalProblem(receipt) = authority {
            crate::investigation::receive_local_problem_rumor(
                ctx,
                character_id,
                receipt.id.clone(),
                format!("receive-rumor:{character_id}:{}", receipt.id),
            )?;
        }
    }
    crate::social::ensure_dialogue_witness_capability(
        ctx,
        &session,
        character_id,
        npc_character_id,
    )?;
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}
