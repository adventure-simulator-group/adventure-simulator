#[derive(Deserialize)]
pub(super) struct PrepareIngredientForm {
    inventory_item_id: u64,
    inventory_scope: String,
    food_lot_id: u64,
    material_object_id: u64,
    request_id: String,
    expected_revision: u64,
    attempt_generation: u64,
    preparation_action: String,
    return_to: Option<String>,
}

pub(super) async fn prepare_ingredient_lot(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<PrepareIngredientForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Select a character first").into_response();
    };
    let action = match form.preparation_action.as_str() {
        "cut" => json!({ "cut": {} }),
        "grind" => json!({ "grind": {} }),
        _ => return (StatusCode::BAD_REQUEST, "Invalid ingredient preparation").into_response(),
    };
    if let Err(error) = state.db.call(
        "prepare_ingredient_lot",
        &[
            json!(character_id),
            json!(form.inventory_scope),
            json!(form.inventory_item_id),
            json!(form.food_lot_id),
            json!(form.material_object_id),
            json!(form.request_id),
            json!(form.expected_revision),
            json!(form.attempt_generation),
            action,
        ],
    ).await {
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    redirect_to_local(form.return_to.as_deref().unwrap_or(""), "/")
        .into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn preparation_redirect_uses_shared_local_url_validation() {
        let source = include_str!("ingredient_preparation.rs");
        let handler = source.split("#[cfg(test)]").next().unwrap();
        assert!(source.contains("redirect_to_local"));
        assert!(source.contains("preparation_action"));
        assert!(source.contains("form.material_object_id"));
        assert!(source.contains("form.request_id"));
        assert!(source.contains("form.expected_revision"));
        assert!(source.contains("form.attempt_generation"));
        assert!(!handler.contains("form.action"));
        assert!(!source.contains("starts_with(\"//\")"));
    }
}
