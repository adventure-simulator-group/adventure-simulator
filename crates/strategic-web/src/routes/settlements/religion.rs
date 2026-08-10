#[derive(Deserialize)]
pub(super) struct ReligionForm {
    religion_id: String,
}

#[derive(Serialize)]
pub(super) struct ReligionDialogue {
    religion_id: Option<String>,
    priest_religion_id: String,
    represented_religion_ids: Vec<String>,
    can_choose: bool,
}

pub(super) async fn religion_dialogue(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<ReligionDialogue> {
    let settlement = state
        .db
        .query::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let priest_religion_id = settlement
        .as_ref()
        .filter(|settlement| {
            settlement_action_service_available(
                &settlement.economy,
                adventuresim_core::settlement_economy::SettlementActionService::Temple,
            )
        })
        .map(|settlement| settlement.religion_id.clone())
        .unwrap_or_default();
    let represented_religion_ids = settlement
        .as_ref()
        .filter(|settlement| {
            settlement_action_service_available(
                &settlement.economy,
                adventuresim_core::settlement_economy::SettlementActionService::Temple,
            )
        })
        .map(|s| {
            s.religious_status
                .represented_religions()
                .into_iter()
                .map(|r| r.religion_id().to_string())
                .collect()
        })
        .unwrap_or_default();
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ReligionDialogue {
            religion_id: None,
            priest_religion_id,
            represented_religion_ids,
            can_choose: false,
        });
    };
    let can_choose = settlement.as_ref().is_some_and(|settlement| {
        settlement_action_service_available(
            &settlement.economy,
            adventuresim_core::settlement_economy::SettlementActionService::Temple,
        )
    }) && character.current_settlement_id.as_deref() == Some(id.as_str());
    let condition = state
        .db
        .query::<CharacterCondition>(&format!(
            "SELECT * FROM backend_character_conditions WHERE character_id = {}",
            character.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    Json(ReligionDialogue {
        religion_id: condition.and_then(|condition| condition.religion_id),
        priest_religion_id,
        represented_religion_ids,
        can_choose,
    })
}

#[derive(Serialize)]
pub(super) struct ReligionChange {
    changed: bool,
    religion_id: Option<String>,
    message: &'static str,
}

pub(super) async fn set_religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<ReligionForm>,
) -> Json<ReligionChange> {
    let religion_id = form.religion_id.trim();
    let settlement = state
        .db
        .query::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(settlement) = settlement else {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "There is no church here to receive your profession.",
        });
    };
    if !settlement_action_service_available(
        &settlement.economy,
        adventuresim_core::settlement_economy::SettlementActionService::Temple,
    ) {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "There is no church here to receive your profession.",
        });
    }
    if !settlement
        .religious_status
        .represented_religions()
        .iter()
        .any(|religion| religion.religion_id() == religion_id)
    {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "This priest can receive you only into his own faith.",
        });
    }
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "Choose a character before speaking with the priest.",
        });
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return Json(ReligionChange {
            changed: false,
            religion_id: None,
            message: "You must be at this church to make a profession of faith.",
        });
    }
    match state
        .db
        .call(
            "set_character_religion",
            &[json!(character.id), json!(religion_id)],
        )
        .await
    {
        Ok(()) => Json(ReligionChange {
            changed: true,
            religion_id: (!religion_id.is_empty()).then(|| religion_id.to_string()),
            message: "Your profession has been recorded.",
        }),
        Err(error) => {
            tracing::warn!(%error, character_id = character.id, "failed to set character religion");
            Json(ReligionChange {
                changed: false,
                religion_id: None,
                message: "The priest cannot receive your profession just now.",
            })
        }
    }
}

pub(super) async fn renounce_religion(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Redirect {
    if session.character_id_u64() == Some(character_id)
        && let Err(error) = state
            .db
            .call("set_character_religion", &[json!(character_id), json!("")])
            .await
        {
            tracing::warn!(%error, character_id, "failed to renounce character religion");
        }
    Redirect::to(&building.append_to(&state, &kind, &id, format!("/locations/{kind}/{id}/party/{character_id}")).await)
}
