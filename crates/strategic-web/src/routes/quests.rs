//! Quest route handlers

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Serialize;
use serde_json::json;

use super::{
    AppState, PartyAction, PartyActionOutcome, execute_or_request_party_action,
    participates_in_party_readiness,
    settlements::{RestForm, get_active_party_members, living_party_members, travel_rest_minutes},
    travel::{
        QuestMapMarkers, TravelDestination, TravelForm, active_quest_tooltip,
        populate_itinerary_forecasts, settlement_destination,
    },
};
use crate::session::Session;
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    AutoresolveReport, BattleLootItem, BattleResult, Character, CharacterAttributes,
    CharacterLimbs, CharacterStats, CharacterTime, CharacterTrainingSchedule,
    InventoryQuantityTarget, ItemDefinition, Party, PartyInventoryItem, PartyStake, Quest,
    QuestStatus, Settlement,
};
use crate::templates::quest::{
    quest_location_base_page, quest_location_map_page, quest_location_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/active-quest-marker", get(active_quest_marker))
        .route("/api/quests/{id}/accept", post(accept_quest_api))
        .route("/api/quests/{id}/turn-in", post(turn_in_quest_api))
        .route("/quests/{id}/abandon", post(abandon_quest))
        .route("/quests/{id}/travel", post(travel_to_quest))
        .route("/locations/quest/{id}", get(quest_location_base))
        .route("/locations/quest/{id}/map", get(quest_location_map))
        .route("/locations/quest/{id}/loot", get(quest_location_loot))
        .route("/locations/quest/{id}/rest", post(rest_at_quest_location))
        .route(
            "/locations/quest/{id}/map/rest",
            post(rest_at_quest_location_map),
        )
        .route("/quests/{id}/autoresolve", post(autoresolve_quest))
        .route("/quests/{id}/loot/store", post(store_battle_loot))
}

#[derive(Serialize)]
struct ActiveQuestMarker {
    active: bool,
    description: Option<String>,
}

async fn active_quest_marker(
    State(state): State<AppState>,
    session: Session,
) -> Json<ActiveQuestMarker> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(ActiveQuestMarker {
            active: false,
            description: None,
        });
    };
    let character = state
        .db
        .query::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(party_id) = character.and_then(|character| character.party_id) else {
        return Json(ActiveQuestMarker {
            active: false,
            description: None,
        });
    };
    let active_quest_id = state
        .db
        .query::<Party>(&format!(
            "SELECT * FROM party WHERE id = {}",
            sql_string_literal(&party_id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
        .and_then(|party| party.active_quest_id);
    let description = if let Some(quest_id) = active_quest_id.as_deref() {
        state
            .db
            .query::<Quest>(&format!(
                "SELECT * FROM quest WHERE id = {}",
                sql_string_literal(quest_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
            .map(|quest| active_quest_tooltip(&quest))
    } else {
        None
    };
    Json(ActiveQuestMarker {
        active: active_quest_id.is_some(),
        description,
    })
}

#[derive(Serialize)]
struct AcceptQuestResponse {
    accepted: bool,
    quest_id: String,
    title: String,
    message: String,
}

async fn accept_quest_api(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<AcceptQuestResponse> {
    let title = state
        .db
        .query::<Quest>(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|quest| quest.title)
        .unwrap_or_else(|| "Quest".to_string());
    let result = match session.character_id_u64() {
        Some(character_id) => accept_quest_for_character(&state, character_id, &id).await,
        None => Err("Choose a character first".to_string()),
    };
    match result {
        Ok(outcome) => Json(AcceptQuestResponse {
            accepted: matches!(outcome, PartyActionOutcome::Executed),
            quest_id: id,
            title,
            message: if matches!(outcome, PartyActionOutcome::Executed) {
                "Quest accepted."
            } else {
                "Requested that the party accept this quest."
            }
            .to_string(),
        }),
        Err(error) => Json(AcceptQuestResponse {
            accepted: false,
            quest_id: id,
            title,
            message: error,
        }),
    }
}

async fn accept_quest_for_character(
    state: &AppState,
    character_id: u64,
    quest_id: &str,
) -> Result<PartyActionOutcome, String> {
    execute_or_request_party_action(
        state,
        character_id,
        PartyAction::AcceptQuest {
            quest_id: quest_id.into(),
        },
    )
    .await
}

#[derive(Serialize)]
struct TurnInQuestResponse {
    claimed: bool,
    reward: i32,
    message: String,
}

async fn turn_in_quest_api(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<TurnInQuestResponse> {
    let reward = state
        .db
        .query::<Quest>(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
        .map_or(0, |quest| quest.gold_reward);
    let result = match session.character_id_u64() {
        Some(character_id) => state
            .db
            .call("turn_in_quest", &[json!(character_id), json!(id)])
            .await
            .map_err(|error| error.to_string()),
        None => Err("Choose a character first".to_string()),
    };
    match result {
        Ok(()) => Json(TurnInQuestResponse {
            claimed: true,
            reward,
            message: "Quest reward added to the party inventory.".to_string(),
        }),
        Err(error) => Json(TurnInQuestResponse {
            claimed: false,
            reward: 0,
            message: error,
        }),
    }
}

async fn abandon_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let settlement_id = quests.first().map(|quest| quest.settlement_id.clone());
    let _ = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::AbandonQuest {
            quest_id: id.clone(),
        },
    )
    .await;

    settlement_id.map_or_else(
        || Redirect::to("/"),
        |settlement_id| Redirect::to(&format!("/locations/settlement/{settlement_id}")),
    )
}

async fn travel_to_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    axum::Form(_form): axum::Form<TravelForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let outcome = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::TravelToQuest {
            quest_id: id.clone(),
        },
    )
    .await;
    if let Err(ref error) = outcome {
        tracing::error!("Failed to travel to quest: {error:?}");
        return (StatusCode::BAD_REQUEST, error.clone()).into_response();
    }
    match outcome.unwrap() {
        PartyActionOutcome::Executed => StatusCode::NO_CONTENT.into_response(),
        PartyActionOutcome::Requested => StatusCode::ACCEPTED.into_response(),
    }
}

#[derive(Default, serde::Deserialize)]
struct StoreLootForm {
    #[serde(default)]
    item_ids: String,
    #[serde(default)]
    quantities: String,
}

async fn store_battle_loot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<StoreLootForm>,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "store_battle_loot",
            &[
                json!(character_id),
                json!(id.clone()),
                json!(
                    form.item_ids
                        .split(',')
                        .filter_map(|v| v.parse::<u64>().ok())
                        .collect::<Vec<_>>()
                ),
                json!(
                    form.quantities
                        .split(',')
                        .filter_map(|v| v.parse::<u32>().ok())
                        .collect::<Vec<_>>()
                ),
            ],
        )
        .await
    {
        tracing::error!("Failed to store battle loot: {error:?}");
    }
    Redirect::to(&format!("/locations/quest/{id}/loot"))
}

#[derive(Default, serde::Deserialize)]
struct QuestMapQuery {
    destination: Option<String>,
}

enum QuestLocationTab {
    Base,
    Map(Option<String>),
    Loot,
}

async fn quest_location_base(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    render_quest_location(state, id, session, QuestLocationTab::Base).await
}

async fn quest_location_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<QuestMapQuery>,
    session: Session,
) -> Html<String> {
    render_quest_location(state, id, session, QuestLocationTab::Map(query.destination)).await
}

async fn quest_location_loot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    render_quest_location(state, id, session, QuestLocationTab::Loot).await
}

async fn rest_at_quest_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    rest_at_quest_location_with_redirect(
        state,
        id.clone(),
        session,
        form,
        &format!("/locations/quest/{id}"),
    )
    .await
}

async fn rest_at_quest_location_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    rest_at_quest_location_with_redirect(
        state,
        id.clone(),
        session,
        form,
        &format!("/locations/quest/{id}/map"),
    )
    .await
}

async fn rest_at_quest_location_with_redirect(
    state: AppState,
    id: String,
    session: Session,
    form: RestForm,
    redirect_path: &str,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let character = state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .ok()
        .flatten();
    if character
        .as_ref()
        .and_then(|row| row.current_quest_location_id.as_deref())
        != Some(id.as_str())
    {
        return (
            StatusCode::BAD_REQUEST,
            "The party is not at this quest location",
        )
            .into_response();
    }
    let requested_minutes = match travel_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match state
        .db
        .call(
            "rest_at_camp",
            &[json!(character_id), json!(requested_minutes)],
        )
        .await
    {
        Ok(()) => Redirect::to(redirect_path).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn render_quest_location(
    state: AppState,
    id: String,
    session: Session,
    tab: QuestLocationTab,
) -> Html<String> {
    let matching_quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(quest) = matching_quests.first() else {
        return Html("<h1>Quest location not found</h1>".to_string());
    };
    let character = match session.character_id_u64() {
        Some(character_id) => {
            let characters: Vec<Character> = state
                .db
                .query(&format!(
                    "SELECT * FROM character WHERE id = {character_id}"
                ))
                .await
                .unwrap_or_default();
            characters.into_iter().next()
        }
        None => None,
    };
    let party = if let Some(party_id) = character.as_ref().and_then(|c| c.party_id.as_ref()) {
        state
            .db
            .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let is_at_location = character
        .as_ref()
        .is_some_and(|c| c.current_quest_location_id.as_deref() == Some(&quest.id));
    if !is_at_location {
        return Html("<h1>Your party is not at this quest location</h1>".to_string());
    }
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let quests: Vec<Quest> = state
        .db
        .query("SELECT * FROM quest")
        .await
        .unwrap_or_default();
    let markers = QuestMapMarkers::new(
        &quests,
        party
            .as_ref()
            .and_then(|party| party.active_quest_id.as_deref()),
    );
    let mut nearby: Vec<TravelDestination> = settlements
        .into_iter()
        .map(|settlement| {
            let distance_m = straight_line_distance_m(quest, &settlement);
            settlement_destination(settlement, distance_m, offroad_journey_minutes(distance_m))
        })
        .collect();
    for destination in &mut nearby {
        markers.decorate_settlement(destination);
    }
    nearby.sort_by_key(|destination| destination.distance_m);
    nearby.truncate(5);
    let can_control = character.as_ref().zip(party.as_ref()).is_some();
    let can_configure_travel = character
        .as_ref()
        .zip(party.as_ref())
        .is_some_and(|(character, party)| party.leader_id == character.id);
    let results: Vec<BattleResult> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_result WHERE quest_id = '{}'",
            quest.id
        ))
        .await
        .unwrap_or_default();
    let resolved = party
        .as_ref()
        .is_some_and(|party| results.iter().any(|result| result.party_id == party.id));
    let autoresolve_report = if let Some(party) = party.as_ref() {
        state
            .db
            .query::<AutoresolveReport>(&format!(
                "SELECT * FROM autoresolve_report WHERE quest_id = '{}' AND party_id = '{}'",
                quest.id, party.id
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let loot: Vec<BattleLootItem> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_loot_item WHERE quest_id = '{}'",
            quest.id
        ))
        .await
        .unwrap_or_default();
    let pooled: Vec<PartyInventoryItem> = if let Some(party) = party.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
                party.id
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let stakes: Vec<PartyStake> = if let Some(party) = party.as_ref() {
        state
            .db
            .query(&format!(
                "SELECT * FROM party_stake WHERE party_id = '{}'",
                party.id
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let stake = character.as_ref().map_or(0, |character| {
        stakes
            .iter()
            .find(|stake| stake.character_id == character.id)
            .map_or(0, |stake| stake.value)
    });
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let targets = if let Some(party) = party.as_ref() {
        party_targets(&state, &party.id).await
    } else {
        Vec::new()
    };
    let party_members = get_active_party_members(&state, character.as_ref()).await;
    let living_party_members = living_party_members(&party_members);
    let stats: Vec<CharacterStats> = state
        .db
        .query("SELECT * FROM character_stats")
        .await
        .unwrap_or_default();
    let default_rest_minutes = living_party_members
        .iter()
        .filter_map(|member| stats.iter().find(|row| row.character_id == member.id))
        .map(|row| {
            (row.calories_used.max(0.0)
                / adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
                * adventuresim_core::strategic_time::MINUTES_PER_DAY as f32)
                .ceil() as u64
        })
        .max()
        .unwrap_or(0)
        .max(1);
    if let Some(party) = party.as_ref() {
        let attributes: Vec<CharacterAttributes> = state
            .db
            .query("SELECT * FROM character_attributes")
            .await
            .unwrap_or_default();
        let limbs: Vec<CharacterLimbs> = state
            .db
            .query("SELECT * FROM character_limbs")
            .await
            .unwrap_or_default();
        let times: Vec<CharacterTime> = state
            .db
            .query("SELECT * FROM character_time")
            .await
            .unwrap_or_default();
        let schedules: Vec<CharacterTrainingSchedule> = state
            .db
            .query("SELECT * FROM character_training_schedule")
            .await
            .unwrap_or_default();
        let member_ids: Vec<_> = living_party_members
            .iter()
            .map(|member| member.id)
            .collect();
        populate_itinerary_forecasts(
            &mut nearby,
            &member_ids,
            &attributes,
            &limbs,
            &stats,
            &times,
            &schedules,
            party,
        );
        for destination in &mut nearby {
            destination.provision_forecast = super::settlements::travel_provision_forecast(
                &state,
                Some(party),
                &living_party_members,
                destination,
                false,
            )
            .await
            .ok()
            .flatten();
        }
    }
    let party_ready = party_is_ready(&state, &party_members).await;
    let can_fight = can_control
        && party_ready
        && quest.status == QuestStatus::Accepted
        && party
            .as_ref()
            .is_some_and(|party| party.active_quest_id.as_deref() == Some(&quest.id));
    let logged_in_as = character.as_ref().map(|c| c.name.as_str());
    let page = match tab {
        QuestLocationTab::Base => quest_location_base_page(
            quest,
            character.as_ref(),
            &party_members,
            can_fight,
            resolved,
            autoresolve_report.as_ref(),
            party.as_ref(),
            can_configure_travel,
            default_rest_minutes,
            logged_in_as,
        ),
        QuestLocationTab::Map(selected) => quest_location_map_page(
            quest,
            &nearby,
            selected.as_deref(),
            character.as_ref(),
            &party_members,
            can_control,
            can_fight,
            resolved,
            autoresolve_report.as_ref(),
            party.as_ref(),
            can_configure_travel,
            default_rest_minutes,
            logged_in_as,
        ),
        QuestLocationTab::Loot => quest_location_page(
            quest,
            character.as_ref(),
            &party_members,
            can_fight,
            resolved,
            autoresolve_report.as_ref(),
            &loot,
            &pooled,
            stake,
            &items,
            &targets,
            logged_in_as,
        ),
    };
    Html(page.into_string())
}

async fn party_is_ready(state: &AppState, members: &[Character]) -> bool {
    for member in members
        .iter()
        .filter(|member| participates_in_party_readiness(member.alive))
    {
        if state
            .db
            .call(
                "refresh_strategic_condition",
                &[serde_json::json!(member.id)],
            )
            .await
            .is_err()
        {
            return false;
        }
        let condition = state
            .db
            .query_one::<crate::spacetimedb::CharacterStrategicCondition>(&format!(
                "SELECT * FROM character_strategic_condition WHERE character_id = {}",
                member.id
            ))
            .await;
        if !matches!(condition, Ok(Some(condition)) if condition.status != "incapacitated") {
            return false;
        }
    }
    true
}

async fn party_targets(state: &AppState, party_id: &str) -> Vec<InventoryQuantityTarget> {
    let party = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(party) = party else {
        return Vec::new();
    };
    state.db.query(&format!("SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = true", party.leader_id)).await.unwrap_or_default()
}

async fn autoresolve_quest(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let outcome = execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::AutoresolveQuest {
            quest_id: id.clone(),
        },
    )
    .await;
    if let Err(ref error) = outcome {
        tracing::error!("Failed to autoresolve quest: {error:?}");
    }
    match outcome {
        Ok(PartyActionOutcome::Executed) => Redirect::to(&format!("/locations/quest/{id}/loot")),
        Ok(PartyActionOutcome::Requested) => Redirect::to("/?party-requested=autoresolve"),
        Err(_) => Redirect::to(&format!("/locations/quest/{id}")),
    }
}

pub(crate) fn offroad_journey_minutes(distance_m: u64) -> u64 {
    ((distance_m as f64 / 1_250.0) * 60.0).ceil() as u64
}

pub(crate) fn straight_line_distance_m(quest: &Quest, settlement: &Settlement) -> u64 {
    if quest.coordinates_are_geographic && settlement.source_node_id.is_some() {
        let lat1 = quest.location_coord_y.to_radians();
        let lat2 = settlement.coord_y.to_radians();
        let delta_lat = (settlement.coord_y - quest.location_coord_y).to_radians();
        let delta_lon = (settlement.coord_x - quest.location_coord_x).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        (6_371_000.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())).round() as u64
    } else {
        (((quest.location_coord_x - settlement.coord_x).powi(2)
            + (quest.location_coord_y - settlement.coord_y).powi(2))
        .sqrt()
            * 1_000.0)
            .round() as u64
    }
}
