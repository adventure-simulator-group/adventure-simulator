use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};

use super::AppState;
use crate::{
    session::Session,
    spacetimedb::{BackendInvestigationJournalEntry, BackendInvestigationLead, Character},
    templates::investigation::journal_page,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/journal", get(journal))
}

async fn journal(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let character = match state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
    {
        Ok(Some(character)) => character,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(error) => {
            tracing::error!(%error, "journal character lookup failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    // Defense in depth: the gateway view is already sanitized, and SSR still
    // scopes every query to the selected session character.
    let entries_sql = format!(
        "SELECT * FROM backend_investigation_journal WHERE owner_character_id = {character_id}"
    );
    let leads_sql = format!(
        "SELECT * FROM backend_investigation_leads WHERE owner_character_id = {character_id}"
    );
    let (entries, leads) = tokio::join!(
        state
            .db
            .query::<BackendInvestigationJournalEntry>(&entries_sql),
        state.db.query::<BackendInvestigationLead>(&leads_sql)
    );
    match (entries, leads) {
        (Ok(mut entries), Ok(mut leads)) => {
            entries.sort_by_key(|row| (row.case_id.clone(), row.recorded_at));
            leads.sort_by_key(|row| (row.case_id.clone(), row.recorded_at));
            Html(journal_page(&entries, &leads, &character.name).into_string()).into_response()
        }
        (Err(error), _) | (_, Err(error)) => {
            tracing::error!(%error, character_id, "sanitized investigation projection failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn route_filters_both_safe_views_to_the_session_observer() {
        let source = include_str!("investigation.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(
            production.contains(
                "backend_investigation_journal WHERE owner_character_id = {character_id}"
            )
        );
        assert!(
            production
                .contains("backend_investigation_leads WHERE owner_character_id = {character_id}")
        );
        assert!(!production.contains("investigation_case_authority"));
        assert!(!production.contains("investigation_evidence_authority"));
    }
}
