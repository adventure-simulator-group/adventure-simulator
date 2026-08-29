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
        BackendBestiaryDeduction, BackendInvestigationCaseSummary,
        BackendInvestigationJournalEntry, BackendInvestigationLead, CharacterView,
    },
    templates::investigation::journal_page,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/quests", get(journal))
        .route("/quests/actions", post(perform_action))
}

async fn journal(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let character = match state
        .db
        .query_one_sats_into::<adventuresim_stdb_client::Character, CharacterView>(
            &crate::spacetimedb::character_by_id(character_id),
        )
        .await
    {
        Ok(Some(character)) => character,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(error) => {
            tracing::error!(%error, "journal character lookup failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    journal_response(&state, &character, None, StatusCode::OK).await
}

async fn journal_response(
    state: &AppState,
    character: &CharacterView,
    feedback: Option<&str>,
    status: StatusCode,
) -> Response {
    let character_id = character.id;
    // Defense in depth: the gateway view is already sanitized, and SSR still
    // scopes every query to the selected session character.
    let entries_sql = format!(
        "SELECT * FROM backend_investigation_journal WHERE owner_character_id = {character_id}"
    );
    let leads_sql = format!(
        "SELECT * FROM backend_investigation_leads WHERE owner_character_id = {character_id}"
    );
    let cases_sql = format!(
        "SELECT * FROM backend_investigation_cases WHERE owner_character_id = {character_id}"
    );
    let deductions_sql = format!(
        "SELECT * FROM backend_bestiary_deductions WHERE owner_character_id = {character_id}"
    );
    let (entries, leads, cases, deductions) = tokio::join!(
        state
            .db
            .query_sats::<BackendInvestigationJournalEntry>(&entries_sql),
        state.db.query_sats::<BackendInvestigationLead>(&leads_sql),
        state
            .db
            .query_sats::<BackendInvestigationCaseSummary>(&cases_sql),
        state
            .db
            .query_sats::<BackendBestiaryDeduction>(&deductions_sql)
    );
    match (entries, leads, cases, deductions) {
        (Ok(mut entries), Ok(mut leads), Ok(cases), Ok(deductions)) => {
            entries.sort_by_key(|row| (row.case_id.clone(), row.recorded_at));
            leads.sort_by_key(|row| (row.case_id.clone(), row.recorded_at));
            (
                status,
                Html(
                    journal_page(
                        &entries,
                        &leads,
                        &cases,
                        &deductions,
                        &character.name,
                        feedback,
                    )
                    .into_string(),
                ),
            )
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

fn safe_investigation_action_error(_error: &str) -> &'static str {
    "That investigation route is no longer available. The journal now shows the routes supported by your current leads."
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
        Ok(_) => Redirect::to("/quests").into_response(),
        Err(error) => {
            tracing::warn!(%error, character_id, "investigation action rejected");
            let feedback = safe_investigation_action_error(&error);
            match state
                .db
                .query_one_sats_into::<adventuresim_stdb_client::Character, CharacterView>(
                    &crate::spacetimedb::character_by_id(character_id),
                )
                .await
            {
                Ok(Some(character)) => {
                    journal_response(&state, &character, Some(feedback), StatusCode::CONFLICT).await
                }
                _ => (
                    StatusCode::CONFLICT,
                    Html(
                        crate::templates::strategic_notice_page(
                            "Investigation route unavailable",
                            feedback,
                            "/quests",
                            "Return to journal",
                            None,
                        )
                        .into_string(),
                    ),
                )
                    .into_response(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::safe_investigation_action_error;

    #[test]
    fn rejected_actions_map_to_visible_player_safe_feedback() {
        let generic = "That investigation route is no longer available. The journal now shows the routes supported by your current leads.";
        assert_eq!(
            safe_investigation_action_error(
                "An incapacitated party member must recover before the party can act"
            ),
            generic
        );
        assert_eq!(
            safe_investigation_action_error(
                "The party must occupy the action's authoritative site"
            ),
            generic
        );
        assert!(!safe_investigation_action_error("private target id 123").contains("123"));
    }

    #[test]
    fn route_filters_all_safe_views_to_the_session_observer() {
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
            production
                .contains("backend_investigation_cases WHERE owner_character_id = {character_id}")
        );
        assert!(
            production
                .contains("backend_bestiary_deductions WHERE owner_character_id = {character_id}")
        );
        assert!(!production.contains("target_id"));
        assert!(!production.contains("seed"));
        assert!(!production.contains("investigation_case_authority"));
        assert!(!production.contains("investigation_evidence_authority"));
        assert!(production.contains(".route(\"/quests\", get(journal))"));
        assert!(production.contains(".route(\"/quests/actions\", post(perform_action))"));
        assert!(production.contains("Redirect::to(\"/quests\")"));
        assert!(production.contains("journal_response("));
        assert!(production.contains("safe_investigation_action_error"));
        assert!(!production.contains(".route(\"/journal"));
        assert!(!production.contains("Redirect::to(\"/journal\")"));
        assert!(!production.contains(".route(\"/investigations"));
        assert!(!production.contains("Redirect::to(\"/investigations\")"));
    }
}
