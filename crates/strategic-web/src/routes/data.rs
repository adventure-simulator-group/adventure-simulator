//! Shared typed reads used by more than one strategic feature.

use super::AppState;
use crate::spacetimedb::{Character, Result};

pub(crate) async fn character(state: &AppState, character_id: u64) -> Result<Option<Character>> {
    state
        .db
        .query_one(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
}

pub(crate) fn new_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
