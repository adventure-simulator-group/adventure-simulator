//! Route handlers

pub mod characters;
pub mod home;
pub mod local_chat;
pub mod missions;
pub mod parties;
pub mod quests;
pub mod settlements;

use axum::{
    Router,
    extract::{Path, Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::session::{CHARACTER_COOKIE, Session, set_theme_cookie};
use crate::spacetimedb::{
    Character, CharacterTime, Party, PartyActionRequest, SpacetimeClient, WorldClock,
};

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub db: SpacetimeClient,
}

#[derive(Serialize, Deserialize)]
struct RequestedActionPayload {
    reducer: String,
    args: Vec<Value>,
}

pub(crate) enum PartyActionOutcome {
    Executed,
    Requested,
}

/// Execute a leader action immediately, or persist the same validated intent for
/// the party leader when a member attempts it.
pub(crate) async fn execute_or_request_party_action(
    state: &AppState,
    actor_id: u64,
    kind: &str,
    summary: &str,
    reducer: &str,
    args: Vec<Value>,
) -> Result<PartyActionOutcome, String> {
    let character = state
        .db
        .query::<Character>(&format!("SELECT * FROM character WHERE id = {actor_id}"))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or("Party not found")?;
    if party.leader_id == actor_id {
        state
            .db
            .call(reducer, &args)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(PartyActionOutcome::Executed);
    }
    let payload = serde_json::to_string(&RequestedActionPayload {
        reducer: reducer.into(),
        args,
    })
    .map_err(|e| e.to_string())?;
    state
        .db
        .call(
            "request_party_action",
            &[json!(actor_id), json!(kind), json!(summary), json!(payload)],
        )
        .await
        .map_err(|e| e.to_string())?;

    // Temporary NPC captains always approve after a short, visible delay.
    let leaders = state
        .db
        .query::<Character>(&format!(
            "SELECT * FROM character WHERE id = {}",
            party.leader_id
        ))
        .await
        .unwrap_or_default();
    if leaders.first().is_some_and(|leader| leader.temporary) {
        let state = state.clone();
        let kind = kind.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let requests = state
                .db
                .query::<PartyActionRequest>(&format!(
                    "SELECT * FROM party_action_request WHERE party_id = '{}'",
                    party_id
                ))
                .await
                .unwrap_or_default();
            for request in requests
                .into_iter()
                .filter(|request| request.requester_id == actor_id && request.action_kind == kind)
            {
                let _ = approve_party_action(&state, party.leader_id, &request).await;
            }
        });
    }
    Ok(PartyActionOutcome::Requested)
}

pub(crate) async fn approve_party_action(
    state: &AppState,
    leader_id: u64,
    request: &PartyActionRequest,
) -> Result<(), String> {
    let payload: RequestedActionPayload =
        serde_json::from_str(&request.payload).map_err(|e| e.to_string())?;
    const ALLOWED: &[&str] = &[
        "travel_to_settlement",
        "travel_to_quest",
        "remove_party_member",
        "create_recruitment_role",
        "accept_party_join_request",
        "reject_party_join_request",
        "accept_quest",
        "abandon_quest",
        "turn_in_quest",
        "autoresolve_quest",
        "update_party_check_targets",
        "set_inventory_quantity_target",
        "disband_party",
        "request_tactical_server",
        "cancel_mission_request",
    ];
    if !ALLOWED.contains(&payload.reducer.as_str()) {
        return Err("Requested reducer is not approvable".into());
    }
    let mut args = payload.args;
    if !matches!(
        payload.reducer.as_str(),
        "request_tactical_server" | "cancel_mission_request"
    ) {
        if let Some(first) = args.first_mut() {
            *first = json!(leader_id);
        }
    }
    state
        .db
        .call(&payload.reducer, &args)
        .await
        .map_err(|e| e.to_string())?;
    state
        .db
        .call(
            "dismiss_party_action_request",
            &[json!(leader_id), json!(request.id)],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Build the complete router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(characters::routes())
        .route("/theme/{name}", get(set_theme))
        .merge(
            Router::new()
                .merge(home::routes())
                .merge(local_chat::routes())
                .merge(settlements::routes())
                .merge(parties::routes())
                .merge(quests::routes())
                .merge(missions::routes())
                .route("/time", get(current_time))
                .layer(middleware::from_fn(require_active_character)),
        )
        .with_state(state)
}

#[derive(Serialize)]
struct CurrentTime {
    character_minutes: u64,
    official_minutes: u64,
}

async fn current_time(State(state): State<AppState>, session: Session) -> Json<CurrentTime> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(CurrentTime {
            character_minutes: 0,
            official_minutes: 0,
        });
    };
    let character_time: Vec<CharacterTime> = state
        .db
        .query(&format!(
            "SELECT * FROM character_time WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let world_clock: Vec<WorldClock> = state
        .db
        .query("SELECT * FROM world_clock WHERE id = 0")
        .await
        .unwrap_or_default();
    Json(CurrentTime {
        character_minutes: character_time.first().map_or(0, |time| time.minutes),
        official_minutes: world_clock
            .first()
            .map_or(0, |clock| clock.official_minutes),
    })
}

async fn set_theme(Path(name): Path<String>) -> Response {
    set_theme_cookie(&name, "/")
}

/// Strategic screens have no anonymous mode. Character creation and selection
/// remain public entry screens; every other route requires a selected character.
async fn require_active_character(request: Request, next: Next) -> Response {
    let cookies = CookieJar::from_headers(request.headers());
    if cookies.get(CHARACTER_COOKIE).is_none() {
        return Redirect::to("/characters").into_response();
    }
    next.run(request).await
}
