//! Route handlers

pub mod characters;
pub mod home;
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
use serde::Serialize;

use crate::session::{CHARACTER_COOKIE, Session, set_theme_cookie};
use crate::spacetimedb::{CharacterTime, SpacetimeClient, WorldClock};

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub db: SpacetimeClient,
}

/// Build the complete router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(characters::routes())
        .route("/theme/{name}", get(set_theme))
        .merge(
            Router::new()
                .merge(home::routes())
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
