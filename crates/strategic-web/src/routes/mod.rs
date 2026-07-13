//! Route handlers

pub mod characters;
pub mod home;
pub mod missions;
pub mod parties;
pub mod quests;
pub mod settlements;

use axum::{
    extract::{Path, Request},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use axum_extra::extract::CookieJar;

use crate::session::{set_theme_cookie, CHARACTER_COOKIE};
use crate::spacetimedb::SpacetimeClient;

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
                .layer(middleware::from_fn(require_active_character)),
        )
        .with_state(state)
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
