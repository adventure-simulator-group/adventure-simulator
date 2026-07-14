//! Quest route handlers

use axum::{
    Form, Json, Router,
    extract::{Path, Query, State},
    response::{Html, Redirect},
    routing::{get, post},
};
use serde::Serialize;
use serde_json::json;

use super::{
    AppState, PartyActionOutcome, execute_or_request_party_action,
    settlements::{TravelDestination, get_active_party_members, settlement_destination},
};
use crate::session::Session;
use crate::spacetimedb::{
    BattleLootItem, BattleResult, Character, InventoryQuantityTarget, ItemDefinition, Party,
    PartyInventoryItem, PartyStake, Quest, Settlement,
};
use crate::templates::quest::{
    post_battle_page, quest_location_base_page, quest_location_map_page, quest_location_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/current-quest", get(current_quest))
        .route("/quests/{id}/accept", post(accept_quest))
        .route("/api/quests/{id}/accept", post(accept_quest_api))
        .route("/api/quests/{id}/turn-in", post(turn_in_quest_api))
        .route("/quests/{id}/abandon", post(abandon_quest))
        .route("/quests/{id}/travel", post(travel_to_quest))
        .route("/quests/{id}/location", get(quest_location_redirect))
        .route("/locations/quest/{id}", get(quest_location_base))
        .route("/locations/quest/{id}/map", get(quest_location_map))
        .route("/locations/quest/{id}/loot", get(quest_location_loot))
        .route("/quests/{id}/autoresolve", post(autoresolve_quest))
        .route("/quests/{id}/loot", get(post_battle_loot))
        .route("/quests/{id}/loot/store", post(store_battle_loot))
}

#[derive(Serialize)]
struct CurrentQuestSummary {
    id: String,
    title: String,
    can_abandon: bool,
}

async fn current_quest(
    State(state): State<AppState>,
    session: Session,
) -> Json<Option<CurrentQuestSummary>> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(None);
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
    let Some(character) = character else {
        return Json(None);
    };
    let Some(party_id) = character.party_id.as_ref() else {
        return Json(None);
    };
    let party = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let Some(party) = party else {
        return Json(None);
    };
    let Some(active_quest_id) = party.active_quest_id.as_ref() else {
        return Json(None);
    };
    let quest = state
        .db
        .query::<Quest>(&format!(
            "SELECT * FROM quest WHERE id = '{}'",
            active_quest_id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    Json(quest.map(|quest| CurrentQuestSummary {
        id: quest.id,
        title: quest.title,
        can_abandon: quest.status.eq_ignore_ascii_case("accepted")
            && character.current_quest_location_id.is_none(),
    }))
}

async fn accept_quest(
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
    let outcome = accept_quest_for_character(&state, character_id, &id).await;

    if matches!(outcome, Ok(PartyActionOutcome::Requested)) {
        return Redirect::to("/?party-requested=accept_quest");
    }
    settlement_id.map_or_else(
        || Redirect::to("/"),
        |settlement_id| Redirect::to(&format!("/locations/settlement/{settlement_id}")),
    )
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
                "Quest added to your tracker."
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
        "accept_quest",
        &format!("Accept quest {quest_id}"),
        "accept_quest",
        vec![json!(character_id), json!(quest_id)],
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
        Some(character_id) => {
            execute_or_request_party_action(
                &state,
                character_id,
                "turn_in_quest",
                &format!("Turn in quest {id}"),
                "turn_in_quest",
                vec![json!(character_id), json!(id)],
            )
            .await
        }
        None => Err("Choose a character first".to_string()),
    };
    match result {
        Ok(outcome) => Json(TurnInQuestResponse {
            claimed: matches!(outcome, PartyActionOutcome::Executed),
            reward: if matches!(outcome, PartyActionOutcome::Executed) {
                reward
            } else {
                0
            },
            message: if matches!(outcome, PartyActionOutcome::Executed) {
                "Quest reward added to the party inventory."
            } else {
                "Requested that the party turn in this quest."
            }
            .to_string(),
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
        "abandon_quest",
        &format!("Abandon quest {id}"),
        "abandon_quest",
        vec![json!(character_id), json!(id.clone())],
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
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let outcome = execute_or_request_party_action(
        &state,
        character_id,
        "travel",
        &format!("Travel to quest {id}"),
        "travel_to_quest",
        vec![json!(character_id), json!(id.clone())],
    )
    .await;
    if let Err(ref error) = outcome {
        tracing::error!("Failed to travel to quest: {error:?}");
        return Redirect::to("/");
    }
    match outcome.unwrap() {
        PartyActionOutcome::Executed => Redirect::to(&format!("/locations/quest/{id}")),
        PartyActionOutcome::Requested => Redirect::to("/?party-requested=travel"),
    }
}

async fn post_battle_loot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let Some(character_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let Some(character) = characters.first() else {
        return Html("<h1>Character not found</h1>".to_string());
    };
    let results: Vec<BattleResult> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_result WHERE quest_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let Some(result) = results.first() else {
        return Html("<h1>Battle result not found</h1>".to_string());
    };
    if character.party_id.as_deref() != Some(&result.party_id) {
        return Html("<h1>This battle belongs to another party</h1>".to_string());
    }
    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(quest) = quests.first() else {
        return Html("<h1>Quest not found</h1>".to_string());
    };
    let loot: Vec<BattleLootItem> = state
        .db
        .query(&format!(
            "SELECT * FROM battle_loot_item WHERE quest_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();
    let pooled: Vec<PartyInventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM party_inventory_item WHERE party_id = '{}'",
            result.party_id
        ))
        .await
        .unwrap_or_default();
    let stakes: Vec<PartyStake> = state
        .db
        .query(&format!(
            "SELECT * FROM party_stake WHERE party_id = '{}'",
            result.party_id
        ))
        .await
        .unwrap_or_default();
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let stake = stakes
        .iter()
        .find(|stake| stake.character_id == character.id)
        .map_or(0, |stake| stake.value);
    let targets = party_targets(&state, &result.party_id).await;
    Html(
        post_battle_page(
            quest,
            character,
            &loot,
            &pooled,
            stake,
            &items,
            &targets,
            session.theme(),
        )
        .into_string(),
    )
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

async fn quest_location_redirect(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/locations/quest/{id}"))
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

async fn render_quest_location(
    state: AppState,
    id: String,
    session: Session,
    tab: QuestLocationTab,
) -> Html<String> {
    let quests: Vec<Quest> = state
        .db
        .query(&format!("SELECT * FROM quest WHERE id = '{}'", id))
        .await
        .unwrap_or_default();
    let Some(quest) = quests.first() else {
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
    let mut nearby: Vec<TravelDestination> = settlements
        .into_iter()
        .map(|settlement| {
            let distance_m = straight_line_distance_m(quest, &settlement);
            settlement_destination(settlement, distance_m, offroad_journey_minutes(distance_m))
        })
        .collect();
    if quest.status.eq_ignore_ascii_case("completed") {
        for destination in &mut nearby {
            destination.turn_in_ready = destination.id == quest.settlement_id;
        }
    }
    nearby.sort_by_key(|destination| destination.distance_m);
    nearby.truncate(5);
    let can_control = character.as_ref().zip(party.as_ref()).is_some();
    let can_fight = can_control
        && quest.status.eq_ignore_ascii_case("accepted")
        && party
            .as_ref()
            .is_some_and(|party| party.active_quest_id.as_deref() == Some(&quest.id));
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
    let logged_in_as = character.as_ref().map(|c| c.name.as_str());
    let page = match tab {
        QuestLocationTab::Base => quest_location_base_page(
            quest,
            character.as_ref(),
            &party_members,
            can_fight,
            resolved,
            logged_in_as,
            session.theme(),
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
            logged_in_as,
            session.theme(),
        ),
        QuestLocationTab::Loot => quest_location_page(
            quest,
            character.as_ref(),
            &party_members,
            can_fight,
            resolved,
            &loot,
            &pooled,
            stake,
            &items,
            &targets,
            logged_in_as,
            session.theme(),
        ),
    };
    Html(page.into_string())
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
        "autoresolve",
        &format!("Autoresolve quest {id}"),
        "autoresolve_quest",
        vec![json!(character_id), json!(id.clone())],
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
