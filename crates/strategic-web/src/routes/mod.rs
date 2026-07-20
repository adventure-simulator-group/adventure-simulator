//! Route handlers

pub mod characters;
mod data;
pub mod home;
mod inventory_forms;
pub mod local_chat;
pub mod missions;
pub mod parties;
mod party_actions;
pub mod quests;
pub mod settlements;
pub(crate) mod travel;

use axum::{
    Router,
    extract::{Request, State},
    http::Uri,
    middleware::{self, Next},
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::live::LiveState;
use crate::session::{CHARACTER_COOKIE, Session};
use crate::spacetimedb::{
    Character, CharacterStrategicCondition, CharacterTime, Party, PartyActionRequest, PartyMember,
    SpacetimeClient, WorldClock,
};

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub db: SpacetimeClient,
    pub live: LiveState,
}

pub(crate) use party_actions::PartyAction;

pub(crate) enum PartyActionOutcome {
    Executed,
    Requested,
}

/// Accept a return destination only when it is a local absolute-path URL.
///
/// Workflows may carry this value through query strings and hidden form fields,
/// but every completion handler must validate it before emitting a redirect.
pub(crate) fn local_return_url(value: &str) -> Option<&str> {
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return None;
    }
    let uri = value.parse::<Uri>().ok()?;
    (uri.scheme().is_none() && uri.authority().is_none() && uri.path().starts_with('/'))
        .then_some(value)
}

pub(crate) fn redirect_to_local(return_to: &str, fallback: &str) -> Redirect {
    Redirect::to(local_return_url(return_to).unwrap_or(fallback))
}

#[cfg(test)]
mod return_url_tests {
    use super::local_return_url;

    #[test]
    fn return_urls_are_local_paths_with_optional_query_and_fragment() {
        assert_eq!(
            local_return_url(
                "/locations/settlement/riverdale/map?destination=quest-1&target_surplus=1.5#plan"
            ),
            Some("/locations/settlement/riverdale/map?destination=quest-1&target_surplus=1.5#plan")
        );
        assert_eq!(local_return_url("https://example.com/steal"), None);
        assert_eq!(local_return_url("//example.com/steal"), None);
        assert_eq!(local_return_url("/\\example.com/steal"), None);
        assert_eq!(local_return_url("merchants"), None);
        assert_eq!(local_return_url("/safe\nLocation: /unsafe"), None);
    }

    #[test]
    fn party_purchase_funding_uses_shared_coin_before_personal_coin() {
        use adventuresim_core::strategic_economy::split_party_purchase_payment;

        assert_eq!(split_party_purchase_payment(8, 20, 15), Some((8, 7)));
        assert_eq!(split_party_purchase_payment(20, 8, 15), Some((15, 0)));
        assert_eq!(split_party_purchase_payment(4, 5, 10), None);
    }
}

/// Corpses remain party members for rendering and history, but do not
/// participate in readiness checks that gate survivor actions.
pub(crate) fn participates_in_party_readiness(alive: bool) -> bool {
    alive
}

/// Execute a leader action immediately, or persist the same validated intent for
/// the party leader when a member attempts it.
pub(crate) async fn execute_or_request_party_action(
    state: &AppState,
    actor_id: u64,
    action: PartyAction,
) -> Result<PartyActionOutcome, String> {
    let character = state
        .db
        .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {actor_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = state
        .db
        .query_one::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Party not found")?;
    if action.requires_ready_party() {
        let members = state
            .db
            .query::<PartyMember>(&format!(
                "SELECT * FROM party_member WHERE party_id = '{}'",
                party.id
            ))
            .await
            .map_err(|error| error.to_string())?;
        for membership in members {
            let member = state
                .db
                .query_one::<Character>(&format!(
                    "SELECT * FROM character WHERE id = {}",
                    membership.character_id
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Party member not found")?;
            if !participates_in_party_readiness(member.alive) {
                continue;
            }
            state
                .db
                .call("refresh_strategic_condition", &[json!(member.id)])
                .await
                .map_err(|error| error.to_string())?;
            let condition = state
                .db
                .query_one::<CharacterStrategicCondition>(&format!(
                    "SELECT * FROM character_strategic_condition WHERE character_id = {}",
                    member.id
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Party member condition not found")?;
            if condition.status == "incapacitated" {
                return Err(
                    "An incapacitated party member must recover before the party can act".into(),
                );
            }
        }
    }
    if party.leader_id == actor_id {
        let (reducer, args) = action.reducer_call(actor_id);
        state
            .db
            .call(reducer, &args)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(PartyActionOutcome::Executed);
    }
    let kind = action.kind();
    let summary = action.summary();
    let payload = serde_json::to_string(&action).map_err(|e| e.to_string())?;
    state
        .db
        .call(
            "request_party_action",
            &[
                json!(actor_id),
                json!(&kind),
                json!(summary),
                json!(payload),
            ],
        )
        .await
        .map_err(|e| e.to_string())?;

    // Temporary NPC captains always approve after a short, visible delay.
    let leader = state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {}",
            party.leader_id
        ))
        .await
        .map_err(|e| e.to_string())?;
    if leader.is_some_and(|leader| leader.temporary) {
        let state = state.clone();
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
                if let Err(error) = approve_party_action(&state, party.leader_id, &request).await {
                    tracing::warn!(%error, "temporary captain could not approve party action");
                }
            }
        });
    }
    Ok(PartyActionOutcome::Requested)
}

#[cfg(test)]
mod readiness_tests {
    use super::participates_in_party_readiness;

    #[test]
    fn corpses_do_not_participate_in_party_readiness() {
        assert!(participates_in_party_readiness(true));
        assert!(!participates_in_party_readiness(false));
    }
}

pub(crate) async fn approve_party_action(
    state: &AppState,
    leader_id: u64,
    request: &PartyActionRequest,
) -> Result<(), String> {
    let action: PartyAction = serde_json::from_str(&request.payload)
        .map_err(|e| format!("Invalid party action payload: {e}"))?;
    let (reducer, args) = action.reducer_call(leader_id);
    state
        .db
        .call(reducer, &args)
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
        .merge(
            Router::new()
                .merge(home::routes())
                .merge(local_chat::routes())
                .merge(settlements::routes())
                .merge(parties::routes())
                .merge(quests::routes())
                .merge(missions::routes())
                .merge(crate::live::routes())
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

async fn current_time(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Json(CurrentTime {
            character_minutes: 0,
            official_minutes: 0,
        })
        .into_response();
    };
    let character_time_sql =
        format!("SELECT * FROM character_time WHERE character_id = {character_id}");
    let (character_time, world_clock) = tokio::join!(
        state.db.query::<CharacterTime>(&character_time_sql),
        state
            .db
            .query::<WorldClock>("SELECT * FROM world_clock WHERE id = 0"),
    );
    let character_time = match character_time {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to load character time");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic time is unavailable",
            )
                .into_response();
        }
    };
    let world_clock = match world_clock {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to load world clock");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic time is unavailable",
            )
                .into_response();
        }
    };
    let official_minutes = world_clock.first().map_or(0, |clock| {
        let now_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        let elapsed_micros = now_micros.saturating_sub(clock.epoch_micros.max(0) as u128);
        (elapsed_micros.saturating_mul(73) / 84_000_000) as u64
    });
    Json(CurrentTime {
        character_minutes: character_time.first().map_or(0, |time| time.minutes),
        official_minutes,
    })
    .into_response()
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
