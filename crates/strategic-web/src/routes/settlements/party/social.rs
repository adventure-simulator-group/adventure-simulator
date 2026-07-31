pub(super) async fn party_social(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
    let Some((active, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let selected = if target_id == active.id {
        active.clone()
    } else {
        match crate::routes::data::character_as_observed(&state, target_id, active.id)
            .await
            .ok()
            .flatten()
        {
            Some(value) => value,
            None => return Html("<h1>Party member not found</h1>".into()),
        }
    };
    let same_party = target_id == active.id
        || (active.party_id.is_some() && active.party_id == selected.party_id);
    let colocated = active.current_settlement_id == selected.current_settlement_id
        && active.current_case_site_id == selected.current_case_site_id;
    if !same_party
        || !colocated
        || !active.alive
        || !selected.alive
        || !character_is_at_location(&active, &location)
    {
        return Html("<h1>Social actions require a living, co-located party member</h1>".into());
    }
    let party_members = get_active_party_members(&state, Some(&active)).await;
    let sources = get_morale_sources(&state, target_id).await;
    let actor_sources = get_morale_sources(&state, active.id).await;
    let mut shared_concerns = actor_sources
        .iter()
        .filter(|source| {
            adventuresim_core::social::social_source_eligible(&source.kind, source.magnitude)
        })
        .filter_map(|source| adventuresim_core::social::topic_for_source_kind(&source.kind))
        .collect::<Vec<_>>();
    shared_concerns.sort_by_key(|topic| format!("{topic:?}"));
    shared_concerns.dedup();
    let target_condition_result = state
        .db
        .query_one::<CharacterCondition>(&format!(
            "SELECT * FROM backend_character_conditions WHERE character_id = {target_id}"
        ))
        .await;
    let religion_id = target_condition_result
        .as_ref()
        .ok()
        .and_then(|value| value.as_ref())
        .and_then(|value| value.religion_id.clone());
    let reputation = query_local_reputation(&state, target_id, &location.id).await;
    let fame = reputation
        .as_ref()
        .map_or(0.0, |value| value.fame as f32 / 100.0);
    let infamy = reputation
        .as_ref()
        .map_or(0.0, |value| value.infamy as f32 / 100.0);
    let target_minute = query_single::<CharacterTime>(&state, "backend_character_times", target_id)
        .await
        .map_or(0, |v| v.minutes);
    let affinity_id = format!("{target_id}:{}", active.id);
    let affinity_result = state
        .db
        .query_one::<CharacterAffinity>(&format!(
            "SELECT * FROM backend_character_affinities WHERE id = {}",
            sql_string_literal(&affinity_id)
        ))
        .await;
    let affinity_available = affinity_result.is_ok();
    let affinity = affinity_result.ok().flatten().map_or(0.0, |v| {
        adventuresim_core::social::settle_affinity(
            v.anchor,
            target_minute.saturating_sub(v.anchor_minute),
        )
    });
    let (low, high) = (active.id.min(target_id), active.id.max(target_id));
    let familiarity_id = format!("{low}:{high}");
    let familiarity_result = state
        .db
        .query_one::<CharacterFamiliarity>(&format!(
            "SELECT * FROM backend_character_familiarities WHERE id = {}",
            sql_string_literal(&familiarity_id)
        ))
        .await;
    let familiarity_available = familiarity_result.is_ok();
    let shared_minutes = familiarity_result
        .ok()
        .flatten()
        .map_or(0, |v| v.shared_minutes);
    let beliefs_result = state
        .db
        .query::<SocialBelief>(&format!(
            "SELECT * FROM backend_social_beliefs WHERE observer_id = {}",
            active.id
        ))
        .await;
    let beliefs_available = beliefs_result.is_ok();
    let beliefs = match beliefs_result {
        Ok(rows) => rows
            .into_iter()
            .filter(|row| row.subject_id == target_id)
            .collect(),
        Err(error) => {
            tracing::error!(%error, observer_id=active.id, target_id, "private social belief query failed closed");
            Vec::new()
        }
    };
    let addressed_source_ids = state
        .db
        .query::<SocialAddress>(&format!(
            "SELECT * FROM backend_social_addresses WHERE actor_id = {}",
            active.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.target_id == target_id)
        .map(|row| row.source_id)
        .collect();
    let automatic_chat_enabled = if target_id == active.id {
        false
    } else {
        state
            .db
            .query_one::<AutomaticSocialChat>(&format!(
                "SELECT * FROM backend_automatic_social_chats WHERE id = {}",
                sql_string_literal(&format!("{}:{target_id}", active.id))
            ))
            .await
            .ok()
            .flatten()
            .is_some_and(|row| row.enabled)
    };
    let actor_personality_result = state
        .db
        .query::<CharacterPersonality>(&format!(
            "SELECT * FROM backend_character_personalities WHERE character_id = {}",
            active.id
        ))
        .await;
    let actor_personality_available = actor_personality_result.is_ok();
    let actor_personality = match actor_personality_result {
        Ok(rows) => rows.into_iter().next(),
        Err(error) => {
            tracing::error!(
                %error,
                actor_id = active.id,
                "private actor personality query failed closed"
            );
            None
        }
    };
    let actor_skills_result = state
        .db
        .query_one::<CharacterSkills>(&format!(
            "SELECT * FROM backend_character_skills WHERE character_id = {}",
            active.id
        ))
        .await;
    let prayer_disabled_reason = if target_id == active.id {
        None
    } else if !actor_personality_available || actor_personality.is_none() {
        Some("Prayer eligibility is unavailable right now.".to_owned())
    } else if actor_personality.as_ref().is_some_and(|personality| {
        personality.conviction == crate::spacetimedb::Conviction::Zealous
    }) {
        Some("Your Zealous conviction prevents you from leading a companion's prayer.".to_owned())
    } else {
        match &target_condition_result {
            Err(error) => {
                tracing::error!(%error, target_id, "target religion query failed closed");
                Some("Their religion is unavailable right now.".to_owned())
            }
            Ok(None) => Some("Their religion is unavailable right now.".to_owned()),
            Ok(Some(condition)) => match condition.religion_id.as_deref() {
                None => Some("They profess no religion.".to_owned()),
                Some(religion_id) => {
                    match adventuresim_world_schema::OfficialReligion::from_id(religion_id) {
                        None => Some("Their religion is unknown.".to_owned()),
                        Some(religion) => match &actor_skills_result {
                            Err(error) => {
                                tracing::error!(%error, actor_id=active.id, "private Religion knowledge query failed closed");
                                Some("Your Religion knowledge is unavailable right now.".to_owned())
                            }
                            Ok(None) => {
                                Some("Your Religion knowledge is unavailable right now.".to_owned())
                            }
                            Ok(Some(skills))
                                if !skills.religion_hours.direct(religion).is_finite()
                                    || skills.religion_hours.direct(religion) <= 0.0 =>
                            {
                                Some(format!(
                                    "You have not directly studied {}.",
                                    religion.label()
                                ))
                            }
                            Ok(Some(_)) => None,
                        },
                    }
                }
            },
        }
    };
    let social = SocialPresentation {
        affinity,
        familiarity_hours: adventuresim_core::social::effective_familiarity_hours(
            shared_minutes,
            party_members.iter().filter(|v| v.alive).count(),
            true,
        ),
        religion_id,
        fame,
        infamy,
        beliefs,
        shared_concerns,
        addressed_source_ids,
        automatic_chat_enabled,
        joke_blocked: social_action_blocked_by_actor(
            actor_personality_available,
            actor_personality.as_ref(),
            adventuresim_core::social::SocialActionKind::LightenMood,
        ),
        flirt_blocked: social_action_blocked_by_actor(
            actor_personality_available,
            actor_personality.as_ref(),
            adventuresim_core::social::SocialActionKind::Flirt,
        ),
        prayer_disabled_reason,
        feedback: social_feedback(building.social_feedback.as_deref()),
        unavailable: !beliefs_available || !affinity_available || !familiarity_available,
    };
    let dialog = party_social_dialog(&location, &selected, &active, &sources, &social);
    if target_id == active.id {
        render_party_personal(
            &state,
            &kind,
            &id,
            target_id,
            building,
            &session,
            Some(dialog),
            None,
            true,
        )
        .await
    } else {
        render_party_stats(
            &state,
            &kind,
            &id,
            target_id,
            building,
            &session,
            Some(dialog),
            None,
            true,
        )
        .await
    }
}

#[derive(Deserialize)]
pub(super) struct SocialActionForm {
    source_id: String,
    action_kind: String,
}

#[derive(Deserialize)]
pub(super) struct CasualChatForm {
    requested_minutes: SocialDuration,
    action_id: SocialActionId,
}

#[derive(Deserialize)]
pub(super) struct BackendSocialChatReceiptRow {
    #[serde(deserialize_with = "crate::spacetimedb::deserialize_social_chat_outcome")]
    outcome: SocialChatOutcome,
}

#[derive(Deserialize)]
pub(super) struct AutomaticSocialChatForm {
    enabled: Option<String>,
}

pub(super) async fn set_automatic_social_chat(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<AutomaticSocialChatForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character first").into_response();
    };
    if let Err(error) = state
        .db
        .call(
            "set_automatic_social_chat",
            &[
                json!(actor_id),
                json!(target_id),
                json!(form.enabled.is_some()),
            ],
        )
        .await
    {
        tracing::warn!(%error, actor_id, target_id, "automatic social chat preference rejected");
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{target_id}/social")))
        .into_response()
}

pub(super) async fn perform_social_action(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<SocialActionForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character first").into_response();
    };
    // The actor is derived exclusively from the signed session, never form input.
    let result = state
        .db
        .call(
            "perform_social_action",
            &[
                json!(actor_id),
                json!(target_id),
                json!(form.source_id),
                json!(form.action_kind),
            ],
        )
        .await;
    let feedback = match result {
        Ok(()) => {
            let address_id = format!("{actor_id}:{target_id}:{}", form.source_id);
            match state
                .db
                .query_one::<SocialAddress>(&format!(
                    "SELECT * FROM backend_social_addresses WHERE id = {}",
                    sql_string_literal(&address_id)
                ))
                .await
            {
                Ok(Some(_)) => "addressed",
                Ok(None) => "not_addressed",
                Err(error) => {
                    tracing::warn!(%error, actor_id, target_id, "social action result unavailable");
                    "unavailable"
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, actor_id, target_id, "social action rejected");
            social_action_error_feedback(&error.to_string())
        }
    };
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{target_id}/social?social_feedback={feedback}"
    )))
    .into_response()
}

pub(super) async fn chat_with_party_member(
    State(state): State<AppState>,
    Path((kind, id, target_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<CasualChatForm>,
) -> Response {
    let Some(actor_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character first").into_response();
    };
    let result = state
        .db
        .call(
            "chat_with_party_member",
            &[
                json!(actor_id),
                json!(target_id),
                json!(form.requested_minutes.minutes()),
                json!(form.action_id.as_str()),
            ],
        )
        .await;
    let feedback = match result {
        Ok(()) => state
            .db
            .query_one::<BackendSocialChatReceiptRow>(&format!(
                "SELECT * FROM backend_social_chat_receipts WHERE id = {} AND actor_id = {actor_id}",
                sql_string_literal(&format!("{actor_id}:{}", form.action_id.as_str()))
            ))
            .await
            .ok()
            .flatten()
            .map_or("chat_unavailable", |row| match row.outcome {
                SocialChatOutcome::Positive => "chat_positive",
                SocialChatOutcome::Mixed => "chat_mixed",
                SocialChatOutcome::Negative => "chat_negative",
            }),
        Err(error) => {
            tracing::warn!(%error, actor_id, target_id, "casual party chat rejected");
            "chat_unavailable"
        }
    };
    Redirect::to(&building.append_to(format!(
        "/locations/{kind}/{id}/party/{target_id}/social?social_feedback={feedback}"
    )))
    .into_response()
}

pub(super) fn social_action_error_feedback(error: &str) -> &'static str {
    if error.contains("needs time before it can be tried again") {
        "cooldown"
    } else if error.contains("Morale source is stale")
        || error.contains("Only current, negative, recognized morale sources")
        || error.contains("Morale source is not actionable")
    {
        "stale"
    } else {
        "unavailable"
    }
}

pub(super) fn social_action_blocked_by_actor(
    personality_available: bool,
    personality: Option<&CharacterPersonality>,
    action: adventuresim_core::social::SocialActionKind,
) -> bool {
    use adventuresim_core::social::{
        Courtship as CoreCourtship, Mirth as CoreMirth, actor_allows_social_action,
    };

    if !personality_available {
        return true;
    }
    let Some(personality) = personality else {
        return true;
    };
    let mirth = match personality.mirth {
        crate::spacetimedb::Mirth::Neutral => CoreMirth::Neutral,
        crate::spacetimedb::Mirth::Merry => CoreMirth::Merry,
        crate::spacetimedb::Mirth::Grave => CoreMirth::Grave,
    };
    let courtship = match personality.courtship {
        crate::spacetimedb::Courtship::Neutral => CoreCourtship::Neutral,
        crate::spacetimedb::Courtship::Amorous => CoreCourtship::Amorous,
        crate::spacetimedb::Courtship::Proper => CoreCourtship::Proper,
    };
    !actor_allows_social_action(action, mirth, courtship)
}

pub(super) fn social_feedback(value: Option<&str>) -> Option<crate::templates::settlement::SocialFeedback> {
    use crate::templates::settlement::SocialFeedback;
    match value {
        Some("addressed") => Some(SocialFeedback {
            message: "This concern is addressed.",
            is_error: false,
        }),
        Some("not_addressed") => Some(SocialFeedback {
            message: "This concern remains unresolved.",
            is_error: false,
        }),
        Some("cooldown") => Some(SocialFeedback {
            message: "That approach needs time before it can be tried again.",
            is_error: true,
        }),
        Some("stale") => Some(SocialFeedback {
            message: "That morale concern has changed. Choose a current concern.",
            is_error: true,
        }),
        Some("unavailable") => Some(SocialFeedback {
            message: "The social action could not be completed right now.",
            is_error: true,
        }),
        Some("chat_positive") => Some(SocialFeedback {
            message: "The conversation brings you closer.",
            is_error: false,
        }),
        Some("chat_mixed") => Some(SocialFeedback {
            message: "The conversation has warm moments and awkward ones.",
            is_error: false,
        }),
        Some("chat_negative") => Some(SocialFeedback {
            message: "The conversation leaves some friction between you.",
            is_error: false,
        }),
        Some("chat_unavailable") => Some(SocialFeedback {
            message: "The conversation could not be completed right now.",
            is_error: true,
        }),
        _ => None,
    }
}
