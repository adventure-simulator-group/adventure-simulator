//! Shared typed reads used by more than one strategic feature.

use super::AppState;
use crate::spacetimedb::{BackendCharacterCaseSiteLocation, Character, Result};

fn prefer_complete_cache<T>(cache: Option<Option<T>>, fallback: Option<T>) -> Option<T> {
    cache.unwrap_or(fallback)
}

pub(crate) async fn character(state: &AppState, character_id: u64) -> Result<Option<Character>> {
    let case_site_sql = format!(
        "SELECT * FROM backend_character_case_site_locations WHERE character_id = {character_id}"
    );
    // Character is a public mutable projection and is served from the SDK
    // cache once its explicit subscription is complete. The case-site view is
    // intentionally kept on HTTP SQL: owner-scoped/private projections are
    // never treated as an authorization boundary by the shared cache.
    let cached_character = state.live.cached_character(character_id);
    let mut character = match cached_character {
        Some(character) => Ok(prefer_complete_cache(Some(character), None)),
        None => {
            let character_sql = format!("SELECT * FROM character WHERE id = {character_id}");
            state.db.query_one::<Character>(&character_sql).await
        }
    }?;
    let case_site = state
        .db
        .query_one::<BackendCharacterCaseSiteLocation>(&case_site_sql)
        .await?;
    if let Some(character) = character.as_mut() {
        character.current_case_site_id = case_site.map(|location| location.case_site_id.value);
    }
    Ok(character)
}

#[cfg(test)]
mod tests {
    use super::prefer_complete_cache;
    use crate::spacetimedb::Character;

    fn character(id: u64) -> Character {
        Character {
            id,
            name: format!("character-{id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: None,
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    #[test]
    fn complete_cache_hit_wins_and_cache_miss_falls_back() {
        assert_eq!(
            prefer_complete_cache(Some(Some(character(7))), Some(character(8)))
                .unwrap()
                .id,
            7
        );
        assert_eq!(
            prefer_complete_cache(Some(None), Some(character(8))).is_none(),
            true
        );
        assert_eq!(
            prefer_complete_cache(None, Some(character(8))).unwrap().id,
            8
        );
    }

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
