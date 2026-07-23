use axum::{
    Router,
    extract::{Form, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use super::{AppState, PartyAction, execute_or_request_party_action};
use crate::{
    session::Session,
    spacetimedb::{
        BackendInvestigationAction, BackendInvestigationActionOutcome,
        BackendInvestigationJournalEntry, BackendInvestigationLead, Character,
    },
    templates::investigation::journal_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/journal", get(journal))
        .route("/journal/actions", post(perform_action))
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
    let actions_sql = format!(
        "SELECT * FROM backend_investigation_actions WHERE owner_character_id = {character_id}"
    );
    let outcomes_sql = format!(
        "SELECT * FROM backend_investigation_action_outcomes WHERE owner_character_id = {character_id}"
    );
    let (entries, leads, actions, outcomes) = tokio::join!(
        state
            .db
            .query::<BackendInvestigationJournalEntry>(&entries_sql),
        state.db.query::<BackendInvestigationLead>(&leads_sql),
        state.db.query::<BackendInvestigationAction>(&actions_sql),
        state
            .db
            .query::<BackendInvestigationActionOutcome>(&outcomes_sql)
    );
    match (entries, leads, actions, outcomes) {
        (Ok(mut entries), Ok(mut leads), Ok(mut actions), Ok(mut outcomes)) => {
            entries.sort_by_key(|row| (row.case_id.clone(), row.recorded_at));
            leads.sort_by_key(|row| (row.case_id.clone(), row.recorded_at));
            actions.sort_by_key(|row| (row.summary.clone(), row.action_id.clone()));
            outcomes.sort_by_key(|row| row.recorded_at);
            Html(journal_page(&entries, &leads, &actions, &outcomes, &character.name).into_string())
                .into_response()
        }
        (Err(error), _, _, _)
        | (_, Err(error), _, _)
        | (_, _, Err(error), _)
        | (_, _, _, Err(error)) => {
            tracing::error!(%error, character_id, "sanitized investigation projection failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[derive(Deserialize)]
struct InvestigationActionForm {
    action_id: String,
    method: String,
    expected_version: u32,
}

async fn perform_action(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<InvestigationActionForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match execute_or_request_party_action(
        &state,
        character_id,
        PartyAction::PerformInvestigation {
            action_id: form.action_id,
            method: form.method,
            expected_version: form.expected_version,
        },
    )
    .await
    {
        Ok(_) => Redirect::to("/journal").into_response(),
        Err(error) => {
            tracing::warn!(%error, character_id, "investigation action rejected");
            (StatusCode::CONFLICT, error).into_response()
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
        assert!(
            production.contains(
                "backend_investigation_actions WHERE owner_character_id = {character_id}"
            )
        );
        assert!(!production.contains("target_id"));
        assert!(!production.contains("seed"));
        assert!(!production.contains("investigation_case_authority"));
        assert!(!production.contains("investigation_evidence_authority"));
    }
}
