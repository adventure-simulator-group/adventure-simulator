//! Route handlers

pub mod characters;
pub mod home;
pub mod parties;
pub mod quests;
pub mod settlements;

use axum::Router;

use crate::spacetimedb::SpacetimeClient;

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub db: SpacetimeClient,
}

/// Build the complete router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(home::routes())
        .merge(characters::routes())
        .merge(settlements::routes())
        .merge(parties::routes())
        .merge(quests::routes())
        .with_state(state)
}
