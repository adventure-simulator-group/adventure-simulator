//! Shared typed reads used by more than one strategic feature.

use super::AppState;
use crate::spacetimedb::{BackendCharacterCaseSiteLocation, Character, Result};

pub(crate) async fn character(state: &AppState, character_id: u64) -> Result<Option<Character>> {
    let (character, case_site) = tokio::join!(
        state
            .db
            .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {character_id}")),
        state
            .db
            .query_one::<BackendCharacterCaseSiteLocation>(&format!(
                "SELECT * FROM backend_character_case_site_locations WHERE character_id = {character_id}"
            ))
    );
    let mut character = character?;
    let case_site = case_site?;
    if let Some(character) = character.as_mut() {
        character.current_case_site_id = case_site.map(|location| location.case_site_id.value);
    }
    Ok(character)
}

#[cfg(test)]
mod tests {
    #[test]
    fn character_loader_uses_authoritative_case_site_projection() {
        let source = include_str!("data.rs");
        let loader = source
            .split("pub(crate) async fn character")
            .nth(1)
            .unwrap();
        assert!(loader.contains("backend_character_case_site_locations"));
        assert!(loader.contains("character.current_case_site_id"));
        assert!(loader.contains("location.case_site_id.value"));
        assert!(!loader.contains("current_case_site_id.unwrap"));
    }
}

pub(crate) fn new_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
