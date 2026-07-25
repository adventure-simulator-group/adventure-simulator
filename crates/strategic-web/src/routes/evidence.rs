use super::AppState;
use crate::{
    session::Session,
    spacetimedb::{
        BackendPhysicalEvidence, BackendPhysicalEvidenceInspection, BestiaryResultView,
        bestiary_result_views, sql_string_literal,
    },
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/evidence/case-sites/{case_site_id}",
            get(case_site_evidence),
        )
        .route("/api/evidence/inspect", post(inspect))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct EvidenceTopicView {
    id: String,
    label: String,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceView {
    id: String,
    label: String,
    portrait_icon: String,
    description: String,
    topics: Vec<EvidenceTopicView>,
    inspections: Vec<EvidenceInspectionView>,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceInspectionView {
    attempt_id: String,
    topic_id: String,
    stat_label: String,
    passed: bool,
    narration: String,
    bestiary_results: Vec<BestiaryResultView>,
}

async fn evidence_at_site(
    state: &AppState,
    character_id: u64,
    case_site_id: &str,
) -> Result<Vec<EvidenceView>, StatusCode> {
    let character = super::data::character(state, character_id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if character.current_case_site_id.as_deref() != Some(case_site_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut evidence = state
        .db
        .query::<BackendPhysicalEvidence>(&format!(
            "SELECT * FROM backend_physical_evidence WHERE owner_character_id = {character_id} AND case_site_id = {}",
            sql_string_literal(case_site_id)
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let inspections = state
        .db
        .query::<BackendPhysicalEvidenceInspection>(&format!(
            "SELECT * FROM backend_physical_evidence_inspections WHERE owner_character_id = {character_id}"
        ))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    evidence.sort_by(|left, right| {
        (&left.label, &left.evidence_id).cmp(&(&right.label, &right.evidence_id))
    });
    Ok(evidence
        .into_iter()
        .map(|item| {
            let mut item_inspections = inspections
                .iter()
                .filter(|attempt| attempt.evidence_id == item.evidence_id)
                .cloned()
                .collect::<Vec<_>>();
            item_inspections
                .sort_by_key(|attempt| (attempt.attempted_at, attempt.attempt_id.clone()));
            EvidenceView {
                id: item.evidence_id,
                label: item.label,
                portrait_icon: item.portrait_icon,
                description: item.description,
                topics: serde_json::from_str(&item.topics_json).unwrap_or_default(),
                inspections: item_inspections
                    .into_iter()
                    .map(|attempt| EvidenceInspectionView {
                        attempt_id: attempt.attempt_id,
                        topic_id: attempt.topic_id,
                        stat_label: attempt.stat_label,
                        passed: attempt.passed,
                        narration: attempt.narration,
                        bestiary_results: bestiary_result_views(&attempt.bestiary_results_json),
                    })
                    .collect(),
            }
        })
        .collect())
}

async fn case_site_evidence(
    State(state): State<AppState>,
    Path(case_site_id): Path<String>,
    session: Session,
) -> Result<Json<Vec<EvidenceView>>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(
        evidence_at_site(&state, character_id, &case_site_id).await?,
    ))
}

#[derive(Deserialize)]
struct InspectRequest {
    evidence_id: String,
    topic_id: String,
    action_id: String,
    case_site_id: String,
}

async fn inspect(
    State(state): State<AppState>,
    session: Session,
    Json(request): Json<InspectRequest>,
) -> Result<Json<EvidenceView>, StatusCode> {
    let character_id = session.character_id_u64().ok_or(StatusCode::UNAUTHORIZED)?;
    // Validate location and observer-safe availability before invoking private
    // authority. The reducer repeats the position and topic checks.
    if !evidence_at_site(&state, character_id, &request.case_site_id)
        .await?
        .iter()
        .any(|item| item.id == request.evidence_id)
    {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .db
        .call(
            "inspect_physical_evidence",
            &[
                json!(character_id),
                json!(&request.evidence_id),
                json!(&request.topic_id),
                json!(&request.action_id),
            ],
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                %error,
                character_id,
                evidence_id = %request.evidence_id,
                topic_id = %request.topic_id,
                "physical-evidence inspection was rejected"
            );
            StatusCode::CONFLICT
        })?;
    evidence_at_site(&state, character_id, &request.case_site_id)
        .await?
        .into_iter()
        .find(|item| item.id == request.evidence_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use crate::spacetimedb::{BestiaryResultView, StoredBestiaryResult, bestiary_support_label};
    use adventuresim_world_schema::BestiaryCategory;

    #[test]
    fn evidence_api_never_serializes_check_difficulty() {
        let source = include_str!("evidence.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("inspect_physical_evidence"));
        assert!(!production.contains("difficulty_milli"));
        assert!(!production.contains("current_value"));
    }

    #[test]
    fn bestiary_support_labels_cover_exact_probability_anchors() {
        assert_eq!(bestiary_support_label(0), "rules out");
        assert_eq!(bestiary_support_label(5_000), "inconclusive");
        assert_eq!(bestiary_support_label(10_000), "conclusive support");
    }

    #[test]
    fn bestiary_api_result_contains_accessible_lore_but_no_hidden_truth() {
        let view = BestiaryResultView::from(StoredBestiaryResult {
            category: BestiaryCategory::Spirit,
            support_bps: 5_000,
            interpretation: "The impression may not have been fully physical.".into(),
        });
        let serialized = serde_json::to_string(&view).unwrap();

        assert!(serialized.contains("\"support_bps\":5000"));
        assert!(serialized.contains("\"support_label\":\"inconclusive\""));
        assert!(serialized.contains("\"strengths\":["));
        assert!(serialized.contains("\"weaknesses\":["));
        assert!(!serialized.contains("confirmed_mechanics"));
        assert!(!serialized.contains("folklore"));
        assert!(!serialized.contains("combat modifier"));
        assert!(!serialized.contains("difficulty"));
        assert!(!serialized.contains("threat"));
        assert!(!serialized.contains("canonical"));
    }
}
