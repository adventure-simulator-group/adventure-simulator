//! Route handlers

pub mod characters;
pub mod home;
pub mod missions;
pub mod parties;
pub mod quests;
pub mod settlements;

use axum::{
    extract::Path,
    response::Response,
    routing::get,
    Router,
};

use crate::edgegap::EdgegapClient;
use crate::session::{set_theme_cookie, Session};
use crate::spacetimedb::SpacetimeClient;

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub db: SpacetimeClient,
    pub edgegap: Option<EdgegapClient>,
    pub spacetimedb_host: String,
    pub spacetimedb_database: String,
}

/// Build the complete router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(home::routes())
        .merge(characters::routes())
        .merge(settlements::routes())
        .merge(parties::routes())
        .merge(quests::routes())
        .merge(missions::routes())
        .route("/theme/{name}", get(set_theme))
        .with_state(state)
}

async fn set_theme(Path(name): Path<String>) -> Response {
    set_theme_cookie(&name, "/")
}
