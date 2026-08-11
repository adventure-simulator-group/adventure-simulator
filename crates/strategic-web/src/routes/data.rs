//! Shared typed reads used by more than one strategic feature.

use super::AppState;
use crate::spacetimedb::{BackendCharacterCaseSiteLocation, Character, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct BackendCharacterDeathMinute {
    strategic_minute: u64,
}

async fn character_minute(state: &AppState, character_id: u64) -> Result<Option<u64>> {
    state
        .db
        .query_one::<crate::spacetimedb::CharacterTime>(&format!(
            "SELECT * FROM backend_character_times WHERE character_id = {character_id}"
        ))
        .await
        .map(|time| time.map(|time| time.minutes))
}

/// Mutable Character columns have no effective-dated history yet. They are
/// safe to use cross-character only when the subject has not advanced beyond
/// the observer. Callers that need co-location or another shared-current fact
/// should additionally require equal frontiers.
pub(crate) async fn character_not_ahead_of_observer(
    state: &AppState,
    character_id: u64,
    observer_character_id: u64,
) -> Result<bool> {
    if character_id == observer_character_id {
        return Ok(true);
    }
    let (subject, observer) = tokio::join!(
        character_minute(state, character_id),
        character_minute(state, observer_character_id),
    );
    Ok(matches!((subject?, observer?), (Some(subject), Some(observer)) if subject <= observer))
}

pub(crate) async fn characters_share_frontier(
    state: &AppState,
    first_character_id: u64,
    second_character_id: u64,
) -> Result<bool> {
    let (first, second) = tokio::join!(
        character_minute(state, first_character_id),
        character_minute(state, second_character_id),
    );
    Ok(matches!((first?, second?), (Some(first), Some(second)) if first == second))
}

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
            let character_sql =
                format!("SELECT * FROM backend_characters WHERE id = {character_id}");
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

/// Reconstruct mutable life state at the selected observer's authoritative
/// personal minute. The trusted gateway can read the broad current Character
/// projection, but must not disclose a death from another character's future.
pub(crate) async fn project_alive_as_observed(
    state: &AppState,
    observer_character_id: u64,
    characters: &mut [Character],
) -> Result<()> {
    let observer_time = match state
        .db
        .query_one::<crate::spacetimedb::CharacterTime>(&format!(
            "SELECT * FROM backend_character_times WHERE character_id = {observer_character_id}"
        ))
        .await
    {
        Ok(time) => time,
        Err(error) => {
            tracing::warn!(%error, observer_character_id, "could not read observer chronology");
            for character in characters.iter_mut().filter(|character| !character.alive) {
                character.alive = true;
            }
            return Ok(());
        }
    };
    let Some(observer_minute) = observer_time.map(|time| time.minutes) else {
        // Without an observer frontier the gateway cannot safely decide that a
        // broad current death is already knowable. Preserve availability and
        // leave authoritative reducers to reject actions when appropriate.
        for character in characters.iter_mut().filter(|character| !character.alive) {
            character.alive = true;
        }
        return Ok(());
    };
    for character in characters.iter_mut().filter(|character| !character.alive) {
        let death = match state
            .db
            .query_one::<BackendCharacterDeathMinute>(&format!(
                "SELECT * FROM backend_character_deaths WHERE character_id = {}",
                character.id
            ))
            .await
        {
            Ok(death) => death,
            Err(error) => {
                tracing::warn!(%error, character_id = character.id, observer_character_id, "could not read death chronology");
                character.alive = true;
                continue;
            }
        };
        character.alive = death.is_none_or(|death| death.strategic_minute > observer_minute);
    }
    Ok(())
}

pub(crate) async fn character_as_observed(
    state: &AppState,
    character_id: u64,
    observer_character_id: u64,
) -> Result<Option<Character>> {
    if !character_not_ahead_of_observer(state, character_id, observer_character_id).await? {
        // We cannot reconstruct location, party, age, wealth, or progression
        // at the observer's earlier date. Fail closed instead of returning a
        // row containing mutable facts from the subject's future.
        return Ok(None);
    }
    let mut character = character(state, character_id).await?;
    if let Some(character) = character.as_mut() {
        project_alive_as_observed(
            state,
            observer_character_id,
            std::slice::from_mut(character),
        )
        .await?;
    }
    Ok(character)
}

pub(crate) async fn character_is_alive_as_observed(
    state: &AppState,
    character_id: u64,
    observer_character_id: u64,
) -> Result<bool> {
    // Life state has its own effective-dated history, so callers that need
    // only this fact must not inherit the fail-closed policy for unrelated
    // mutable Character fields. This keeps asynchronous NPC dialogue
    // available without exposing the NPC's future location, wealth, or party.
    let mut character = character(state, character_id).await?;
    let Some(character) = character.as_mut() else {
        return Ok(false);
    };
    project_alive_as_observed(
        state,
        observer_character_id,
        std::slice::from_mut(character),
    )
    .await?;
    Ok(character.alive)
}

pub(crate) fn new_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
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
        assert!(prefer_complete_cache(Some(None), Some(character(8))).is_none());
        assert_eq!(
            prefer_complete_cache(None, Some(character(8))).unwrap().id,
            8
        );
    }

    #[test]
    fn character_loader_uses_authoritative_case_site_projection() {
        let source = include_str!("data.rs");
        let loader = source
            .split("pub(crate) async fn character(")
            .nth(1)
            .unwrap()
            .split("pub(crate) async fn project_alive_as_observed")
            .next()
            .unwrap();
        assert!(loader.contains("backend_character_case_site_locations"));
        assert!(loader.contains("character.current_case_site_id"));
        assert!(loader.contains("location.case_site_id.value"));
        assert!(!loader.contains("current_case_site_id.unwrap"));
    }

    #[test]
    fn observed_character_projection_uses_observer_time_and_private_death_view() {
        let source = include_str!("data.rs");
        let projection = source
            .split("pub(crate) async fn project_alive_as_observed")
            .nth(1)
            .unwrap()
            .split("pub(crate) async fn character_as_observed")
            .next()
            .unwrap();
        assert!(projection.contains("backend_character_times"));
        assert!(projection.contains("backend_character_deaths"));
        assert!(projection.contains("death.strategic_minute > observer_minute"));
        assert!(projection.contains("let Some(observer_minute)"));
        assert!(projection.contains("character.alive = true"));
        assert!(!projection.contains("character_death WHERE"));
    }

    #[test]
    fn cross_character_loader_fails_closed_before_reading_future_mutable_state() {
        let source = include_str!("data.rs");
        let loader = source
            .split("pub(crate) async fn character_as_observed")
            .nth(1)
            .unwrap()
            .split("pub(crate) async fn character_is_alive_as_observed")
            .next()
            .unwrap();
        let chronology = loader.find("character_not_ahead_of_observer").unwrap();
        let mutable_read = loader.find("character(state, character_id)").unwrap();
        assert!(chronology < mutable_read);
        assert!(loader.contains("return Ok(None)"));
    }

    #[test]
    fn life_only_projection_does_not_require_mutable_frontier_alignment() {
        let source = include_str!("data.rs");
        let loader = source
            .split("pub(crate) async fn character_is_alive_as_observed")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(loader.contains("project_alive_as_observed"));
        assert!(!loader.contains("character_as_observed"));
        assert!(!loader.contains("character_not_ahead_of_observer"));
    }
}
