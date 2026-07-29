#[derive(Deserialize)]
pub(super) struct ReligiousDemandForm {
    choice: String,
}

pub(super) async fn resolve_religious_demand(
    State(state): State<AppState>,
    Path((kind, id, character_id, demand_id)): Path<(String, String, u64, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<ReligiousDemandForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id)
        && let Err(error) = state
            .db
            .call(
                "resolve_religious_demand",
                &[json!(demand_id), json!(form.choice)],
            )
            .await
    {
        tracing::warn!(%error, character_id, demand_id, "failed to resolve religious demand");
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")))
}
