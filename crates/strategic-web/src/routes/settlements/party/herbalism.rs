#[derive(Deserialize)]
pub(super) struct PrepareHerbForm {
    inventory_item_id: u64,
    method: String,
}

pub(super) async fn prepare_herbal_remedy(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PrepareHerbForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Only the selected character can prepare herbs",
        )
            .into_response();
    }
    let method = match form.method.as_str() {
        "dry_grind" => json!({ "dryGrind": {} }),
        "infuse_decoct" => json!({ "infuseDecoct": {} }),
        "tincture" => json!({ "tincture": {} }),
        _ => return (StatusCode::BAD_REQUEST, "Invalid herbal preparation method").into_response(),
    };
    if let Err(error) = state
        .db
        .call(
            "prepare_herbal_remedy",
            &[json!(character_id), json!(form.inventory_item_id), method],
        )
        .await
    {
        tracing::warn!(%error, character_id, "herbal preparation failed");
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    Redirect::to(&building.append_to(&state, &kind, &id, format!(
        "/locations/{kind}/{id}/party/{character_id}?herbalism=true"
    )).await)
    .into_response()
}
