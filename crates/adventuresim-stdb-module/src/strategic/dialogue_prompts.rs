#[reducer]
pub fn choose_dialogue_topic(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: String,
    topic_id: String,
    action_id: String,
    expected_revision: u64,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    validate_dialogue_action_id(&action_id)?;
    let action_row_id = format!("{session_id}:{action_id}");
    let mut session = require_session_member(ctx, &session_id, character_id)?;
    if ctx.db.dialogue_action().id().find(&action_row_id).is_some() {
        return Ok(());
    }
    if session.catalog_revision != catalog_revision {
        return Err("Dialogue session revision is stale".into());
    }
    if session.revision != expected_revision {
        return Err("Dialogue action used a stale session revision".into());
    }
    let conversation = adventuresim_dialogue::find_conversation(&session.conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    let topic = conversation
        .topics
        .iter()
        .find(|topic| topic.id == topic_id)
        .ok_or("Unknown dialogue topic")?;
    let known = topic.initially_known
        || ctx
            .db
            .character_topic_knowledge()
            .character_id()
            .filter(character_id)
            .any(|row| row.conversation_id == session.conversation_id && row.topic_id == topic.id);
    if !known {
        return Err("Dialogue topic is not known by this character".into());
    }
    validate_dialogue_cardinality(ctx, &session, conversation)?;
    let facts = dialogue_fact_context(ctx, &session, character_id)?;
    if !facts.matches(&topic.conditions) {
        return Err("Dialogue topic is not eligible in this context".into());
    }
    let response = adventuresim_dialogue::select_response(topic, &facts)
        .map_err(|_| "No unambiguous eligible dialogue response")?;
    let sequence = ctx
        .db
        .dialogue_event()
        .session_id()
        .filter(&session_id)
        .count() as u32;
    let mut testimony_event_sequence = None;
    for (offset, turn) in response.turns.iter().enumerate() {
        let authored_sources = turn
            .fragments
            .iter()
            .enumerate()
            .map(|(fragment, authored)| {
                let field = match authored {
                    adventuresim_dialogue::Fragment::Text { .. } => "value",
                    adventuresim_dialogue::Fragment::Topic { .. } => "label",
                    adventuresim_dialogue::Fragment::PeriodClaim { .. } => "value",
                    adventuresim_dialogue::Fragment::AuthoritativeExplanation { .. } => "value",
                    adventuresim_dialogue::Fragment::Runtime { .. } => "slot",
                };
                adventuresim_dialogue::source_for_fragment(
                    &session.conversation_id,
                    &topic.id,
                    &response.id,
                    offset,
                    fragment,
                    field,
                )
                .cloned()
            })
            .collect::<Vec<_>>();
        let resolved_turn =
            resolve_dialogue_turn(ctx, &session, character_id, turn, &authored_sources)?;
        let resolved = resolved_turn.fragments;
        let source_refs = resolved_turn.source_refs;
        if resolved.iter().any(|fragment| {
            matches!(
                fragment,
                adventuresim_dialogue::ResolvedFragment::Claim { .. }
            )
        }) {
            let emitted_sequence = sequence + offset as u32;
            if testimony_event_sequence.replace(emitted_sequence).is_some() {
                return Err("Testimony binding expanded into more than one event".into());
            }
        }
        ctx.db.dialogue_event().insert(DialogueEvent {
            id: format!("{session_id}:event:{}", sequence + offset as u32),
            gateway_bucket: 0,
            session_id: session_id.clone(),
            sequence: sequence + offset as u32,
            response_id: response.id.clone(),
            speaker_role: turn.speaker.clone(),
            fragments_json: serde_json::to_string(&resolved)
                .map_err(|_| "Could not encode dialogue turn")?,
            source_refs_json: serde_json::to_string(&source_refs)
                .map_err(|_| "Could not encode dialogue sources")?,
            created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
    for effect in &response.effects {
        let source_scope = format!("topic:{}:{}", topic.id, response.id);
        apply_dialogue_effect(
            ctx,
            character_id,
            &session,
            &source_scope,
            &action_id,
            session.revision.saturating_add(1),
            effect,
            testimony_event_sequence,
        )?;
    }
    if let Some(prompt) = &response.prompt {
        let id = format!("{session_id}:prompt:{}:{action_id}", prompt.id);
        if ctx.db.dialogue_prompt().id().find(&id).is_none() {
            ctx.db.dialogue_prompt().insert(DialoguePrompt {
                id,
                gateway_bucket: 0,
                session_id: session_id.clone(),
                prompt_id: prompt.id.clone(),
                mode: format!("{:?}", prompt.mode),
                respondent_role: prompt.respondent.clone(),
                resolution_policy: format!("{:?}", prompt.resolution),
                choices_json: serde_json::to_string(&prompt.choices)
                    .map_err(|_| "Could not encode dialogue choices")?,
                min_choices: prompt.min_choices as u32,
                max_choices: prompt.max_choices as u32,
                state: "open".into(),
                resolved_choice_ids_json: "[]".into(),
                source_refs_json: serde_json::to_string(
                    &prompt
                        .choices
                        .iter()
                        .map(|choice| {
                            adventuresim_dialogue::source_for_choice(
                                &session.conversation_id,
                                &topic.id,
                                &response.id,
                                &choice.id,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| "Could not encode prompt sources")?,
            });
        }
    }
    session.revision += 1;
    ctx.db.dialogue_session().id().update(session.clone());
    ctx.db.dialogue_action().insert(DialogueAction {
        id: action_row_id,
        session_id,
        action_id,
        action_kind: "topic".into(),
        resulting_revision: session.revision,
    });
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}

fn dialogue_answer_is_committed_retry(
    receipt: Option<&DialogueAction>,
    prompt_state: &str,
    prompt_row_id: &str,
) -> Result<bool, String> {
    if let Some(receipt) = receipt {
        return if receipt.action_kind == format!("answer:{prompt_row_id}") {
            Ok(true)
        } else {
            Err("Dialogue action ID conflicts with another action".into())
        };
    }
    if prompt_state != "open" {
        return Err("Dialogue prompt is closed".into());
    }
    Ok(false)
}

#[reducer]
pub fn answer_dialogue_prompt(
    ctx: &ReducerContext,
    character_id: u64,
    prompt_row_id: String,
    choice_ids_json: String,
    action_id: String,
    expected_revision: u64,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    validate_dialogue_action_id(&action_id)?;
    let prompt = ctx
        .db
        .dialogue_prompt()
        .id()
        .find(&prompt_row_id)
        .ok_or("Dialogue prompt not found")?;
    let action_row_id = format!("{}:{action_id}", prompt.session_id);
    let mut session = require_session_member(ctx, &prompt.session_id, character_id)?;
    let participant = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&prompt.session_id)
        .find(|participant| participant.character_id == Some(character_id))
        .ok_or("Character is not a dialogue participant")?;
    if participant.role != prompt.respondent_role {
        return Err("Character is not an eligible respondent for this prompt".into());
    }
    let receipt = ctx.db.dialogue_action().id().find(&action_row_id);
    if dialogue_answer_is_committed_retry(receipt.as_ref(), &prompt.state, &prompt_row_id)? {
        return Ok(());
    }
    if session.catalog_revision != catalog_revision {
        return Err("Dialogue session revision is stale".into());
    }
    if session.revision != expected_revision {
        return Err("Dialogue answer used a stale session revision".into());
    }
    let chosen: Vec<String> =
        serde_json::from_str(&choice_ids_json).map_err(|_| "Invalid dialogue choices")?;
    let allowed: Vec<adventuresim_dialogue::Choice> =
        serde_json::from_str(&prompt.choices_json).map_err(|_| "Invalid authoritative choices")?;
    let unique: std::collections::BTreeSet<_> = chosen.iter().collect();
    if chosen.len() != unique.len()
        || chosen.len() < prompt.min_choices as usize
        || chosen.len() > prompt.max_choices as usize
        || chosen
            .iter()
            .any(|id| !allowed.iter().any(|choice| &choice.id == id))
        || (!prompt.mode.contains("Multi") && chosen.len() != 1)
    {
        return Err("Invalid dialogue answer".into());
    }
    let id = format!("{}:{character_id}", prompt.id);
    if ctx.db.dialogue_answer().id().find(&id).is_some() {
        return Err("Dialogue prompt was already answered by this character".into());
    }
    ctx.db.dialogue_answer().insert(DialogueAnswer {
        id,
        prompt_row_id: prompt_row_id.clone(),
        character_id,
        choice_ids_json: serde_json::to_string(&chosen).unwrap(),
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    let answer_count = ctx
        .db
        .dialogue_answer()
        .prompt_row_id()
        .filter(&prompt.id)
        .count();
    let respondent_count = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&prompt.session_id)
        .filter(|participant| participant.role == prompt.respondent_role)
        .count();
    let answers: Vec<_> = ctx
        .db
        .dialogue_answer()
        .prompt_row_id()
        .filter(&prompt.id)
        .collect();
    let ballots: Vec<Vec<String>> = answers
        .iter()
        .filter_map(|answer| serde_json::from_str(&answer.choice_ids_json).ok())
        .collect();
    let mut vote_counts = std::collections::BTreeMap::<String, usize>::new();
    for ballot in &ballots {
        for choice in ballot {
            *vote_counts.entry(choice.clone()).or_default() += 1;
        }
    }
    let winning = if prompt.resolution_policy.contains("FirstResponse") {
        ballots.first().cloned()
    } else if prompt.resolution_policy.contains("Majority") {
        vote_counts
            .iter()
            .filter(|(_, count)| **count > respondent_count / 2)
            .map(|(choice, _)| vec![choice.clone()])
            .next()
    } else if prompt.resolution_policy.contains("Unanimous")
        && answer_count >= respondent_count
        && ballots.windows(2).all(|pair| pair[0] == pair[1])
    {
        ballots.first().cloned()
    } else if prompt.resolution_policy.contains("AllRespondents")
        && answer_count >= respondent_count
    {
        vote_counts
            .iter()
            .max_by_key(|(choice, count)| (**count, std::cmp::Reverse((*choice).clone())))
            .map(|(choice, _)| vec![choice.clone()])
    } else {
        None
    };
    if let Some(winning) = winning {
        let topic = adventuresim_dialogue::find_conversation(&session.conversation_id)
            .and_then(|conversation| {
                conversation.topics.iter().find(|topic| {
                    topic.responses.iter().any(|response| {
                        response
                            .prompt
                            .as_ref()
                            .is_some_and(|authored| authored.id == prompt.prompt_id)
                    })
                })
            })
            .ok_or("Dialogue prompt topic is no longer authored")?;
        let response = topic
            .responses
            .iter()
            .find(|response| {
                response
                    .prompt
                    .as_ref()
                    .is_some_and(|authored| authored.id == prompt.prompt_id)
            })
            .ok_or("Dialogue prompt response is no longer authored")?;
        for choice in allowed.iter().filter(|choice| winning.contains(&choice.id)) {
            for effect in &choice.effects {
                let source_scope =
                    format!("prompt:{}:{}:{}", prompt.prompt_id, response.id, choice.id);
                apply_dialogue_effect(
                    ctx,
                    character_id,
                    &session,
                    &source_scope,
                    &action_id,
                    session.revision.saturating_add(1),
                    effect,
                    None,
                )?;
            }
            let mut sequence = ctx
                .db
                .dialogue_event()
                .session_id()
                .filter(&session.id)
                .count() as u32;
            for (turn_index, turn) in choice.result_turns.iter().enumerate() {
                let source_refs: Vec<_> = turn
                    .fragments
                    .iter()
                    .enumerate()
                    .map(|(fragment_index, fragment)| {
                        let field = match fragment {
                            adventuresim_dialogue::Fragment::Text { .. } => "value",
                            adventuresim_dialogue::Fragment::Topic { .. } => "label",
                            adventuresim_dialogue::Fragment::PeriodClaim { .. } => "value",
                            adventuresim_dialogue::Fragment::AuthoritativeExplanation {
                                ..
                            } => "value",
                            adventuresim_dialogue::Fragment::Runtime { .. } => "slot",
                        };
                        adventuresim_dialogue::source_for_choice_fragment(
                            &session.conversation_id,
                            &topic.id,
                            &response.id,
                            &choice.id,
                            turn_index,
                            fragment_index,
                            field,
                        )
                    })
                    .collect();
                ctx.db.dialogue_event().insert(DialogueEvent {
                    id: format!("{}:event:{sequence}", session.id),
                    gateway_bucket: 0,
                    session_id: session.id.clone(),
                    sequence,
                    response_id: format!("{}:{}", response.id, choice.id),
                    speaker_role: turn.speaker.clone(),
                    fragments_json: serde_json::to_string(&resolve_dialogue_fragments(
                        ctx,
                        &session,
                        character_id,
                        &turn.speaker,
                        &turn.fragments,
                    )?)
                    .map_err(|_| "Could not encode dialogue result")?,
                    source_refs_json: serde_json::to_string(&source_refs)
                        .map_err(|_| "Could not encode dialogue result sources")?,
                    created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
                });
                sequence += 1;
            }
        }
        let mut prompt = prompt;
        prompt.state = "resolved".into();
        prompt.resolved_choice_ids_json = serde_json::to_string(&winning).unwrap();
        ctx.db.dialogue_prompt().id().update(prompt);
    }
    session.revision += 1;
    ctx.db.dialogue_session().id().update(session.clone());
    ctx.db.dialogue_action().insert(DialogueAction {
        id: action_row_id,
        session_id: session.id.clone(),
        action_id,
        action_kind: format!("answer:{prompt_row_id}"),
        resulting_revision: session.revision,
    });
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}
