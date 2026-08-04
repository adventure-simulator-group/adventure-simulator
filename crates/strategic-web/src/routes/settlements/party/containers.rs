#[derive(Debug, Serialize)]
pub(super) struct ContainerSnapshot {
    objects: Vec<InventoryObject>,
    edges: Vec<InventoryContainment>,
    liquids: Vec<ContainerLiquid>,
    presentations: Vec<ContainerItemPresentation>,
    tinctures: Vec<BackendTinctureStatus>,
}

#[derive(Debug, Serialize)]
pub(super) struct ContainerItemPresentation {
    object_id: u64,
    item_id: String,
    display_name: String,
    quantity: u32,
    exterior_volume_ml: u32,
    container_capacity_ml: u32,
    tincture_vessel: bool,
}

#[derive(Deserialize)]
pub(super) struct ContainerMoveForm {
    #[serde(default)] child_object_id: u64,
    #[serde(default)] child_scope: String,
    #[serde(default)] child_row_id: u64,
    #[serde(default)] parent_object_id: u64,
    #[serde(default)] parent_scope: String,
    #[serde(default)] parent_row_id: u64,
}

#[derive(Deserialize)]
pub(super) struct ContainerRemoveForm {
    child_object_id: u64,
}
#[derive(Deserialize)]
pub(super) struct ContainerWaterForm { container_object_id: u64, requested_ml: u64 }
#[derive(Deserialize)]
pub(super) struct ContainerTinctureForm { container_object_id: u64 }
#[derive(Deserialize)]
pub(super) struct ContainerTinctureDoseForm {
    container_object_id: u64,
    #[serde(default = "default_tincture_dose")]
    amount_milliunits: u32,
}
fn default_tincture_dose() -> u32 { 100 }

pub(super) async fn inventory_containers(
    State(state): State<AppState>,
    session: Session,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let fireplace_roots = state.db.query::<BackendFireplaceStation>(&format!("SELECT * FROM backend_fireplace_stations WHERE character_id = {}", actor.id))
        .await.unwrap_or_default().into_iter().filter_map(|row| row.instrument_object_id).collect::<HashSet<_>>();
    let objects = state.db.query::<InventoryObject>("SELECT * FROM inventory_object")
        .await.unwrap_or_default().into_iter().filter(|row| {
            row.location_kind == "personal" && row.location_owner == actor.id.to_string()
                || row.location_kind == "party"
                    && actor.party_id.as_deref() == Some(row.location_owner.as_str())
                || fireplace_roots.contains(&row.id)
        }).collect::<Vec<_>>();
    let ids = objects.iter().map(|row| row.id).collect::<HashSet<_>>();
    let edges = state.db.query::<InventoryContainment>("SELECT * FROM inventory_containment")
        .await.unwrap_or_default().into_iter()
        .filter(|row| ids.contains(&row.child_object_id) && ids.contains(&row.parent_object_id))
        .collect::<Vec<_>>();
    let liquids = state.db.query::<ContainerLiquid>("SELECT * FROM container_liquid")
        .await.unwrap_or_default().into_iter()
        .filter(|row| ids.contains(&row.container_object_id)).collect();
    let definitions = state.db.query::<ItemDefinition>("SELECT * FROM item").await.unwrap_or_default();
    let lots = state.db.query::<FoodLot>("SELECT * FROM food_lot").await.unwrap_or_default();
    let personal = state.db.query::<InventoryItem>(&format!("SELECT * FROM inventory_item WHERE character_id = {}", actor.id)).await.unwrap_or_default();
    let party = if let Some(party_id) = actor.party_id.as_deref() {
        state.db.query::<PartyInventoryItem>(&format!("SELECT * FROM party_inventory_item WHERE party_id = {}", sql_string_literal(party_id))).await.unwrap_or_default()
    } else { Vec::new() };
    let presentations = objects.iter().filter(|object| object.inventory_row_id > 0).filter_map(|object| {
        let quantity = if object.location_kind == "personal" {
            personal.iter().find(|row| row.id == object.inventory_row_id).map(|row| row.qty)
        } else {
            party.iter().find(|row| row.id == object.inventory_row_id).map(|row| row.quantity)
        }?;
        let definition = definitions.iter().find(|row| row.id == object.item_id)?;
        let display_name = lots.iter().find(|lot| if object.location_kind == "personal" {
            lot.inventory_item_id == Some(object.inventory_row_id)
        } else {
            lot.party_inventory_item_id == Some(object.inventory_row_id)
        })
            .map_or_else(|| object.item_id.replace('_', " "), |lot| lot.display_name.clone());
        Some(ContainerItemPresentation { object_id: object.id, item_id: object.item_id.clone(), display_name,
            quantity, exterior_volume_ml: definition.exterior_volume_ml,
            container_capacity_ml: definition.container_capacity_ml,
            tincture_vessel: adventuresim_core::item_catalog::definition(&object.item_id)
                .is_some_and(|definition| definition.tags.iter().any(|tag| tag == "tincture_vessel")) })
    }).collect();
    let existing = state.db.query::<BackendTinctureStatus>("SELECT * FROM backend_tincture_statuses")
        .await.unwrap_or_default().into_iter().filter(|row| ids.contains(&row.container_object_id)).collect::<Vec<_>>();
    for status in &existing {
        let _ = state.db.call("refresh_tincture", &[json!(actor.id), json!(status.container_object_id)]).await;
    }
    let tinctures = state.db.query::<BackendTinctureStatus>("SELECT * FROM backend_tincture_statuses")
        .await.unwrap_or_default().into_iter().filter(|row| ids.contains(&row.container_object_id)).collect();
    Json(ContainerSnapshot { objects, edges, liquids, presentations, tinctures }).into_response()
}

async fn owned_container_object(
    state: &AppState,
    actor: &Character,
    id: u64,
) -> Option<InventoryObject> {
    let row = state.db.query_one::<InventoryObject>(&format!(
        "SELECT * FROM inventory_object WHERE id = {id}"
    )).await.ok().flatten()?;
    (row.location_kind == "personal" && row.location_owner == actor.id.to_string()
        || row.location_kind == "party"
            && actor.party_id.as_deref() == Some(row.location_owner.as_str())).then_some(row)
}

pub(super) async fn move_inventory_container_item(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerMoveForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let parent = if form.parent_object_id > 0 {
        let Some(parent) = owned_container_object(&state, &actor, form.parent_object_id).await else {
            return (StatusCode::BAD_REQUEST, "Container is not in your inventory").into_response();
        };
        parent
    } else {
        InventoryObject { id: 0, item_id: String::new(), location_kind: form.parent_scope,
            location_owner: String::new(), inventory_row_id: form.parent_row_id }
    };
    let child = if form.child_object_id > 0 {
        let Some(child) = owned_container_object(&state, &actor, form.child_object_id).await else {
            return (StatusCode::BAD_REQUEST, "Item is not in your inventory").into_response();
        };
        child
    } else {
        InventoryObject { id: 0, item_id: String::new(), location_kind: form.child_scope,
            location_owner: String::new(), inventory_row_id: form.child_row_id }
    };
    match state.db.call("put_inventory_item_in_container", &[
        json!(actor.id), json!(child.location_kind), json!(child.inventory_row_id),
        json!(parent.location_kind), json!(parent.inventory_row_id),
    ]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn remove_inventory_container_item(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerRemoveForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.db.call("remove_inventory_item_from_container", &[
        json!(actor.id), json!(form.child_object_id),
    ]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn pour_inventory_container_water(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerWaterForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    let Some(container) = owned_container_object(&state, &actor, form.container_object_id).await else { return (StatusCode::BAD_REQUEST, "Container is not in your inventory").into_response(); };
    match state.db.call("pour_water_into_container", &[json!(actor.id), json!(container.location_kind), json!(container.inventory_row_id), json!(form.requested_ml)]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
pub(super) async fn drain_inventory_container_water(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerWaterForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    match state.db.call("pour_water_out_of_container", &[json!(actor.id), json!(form.container_object_id), json!(form.requested_ml)]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn pour_inventory_container_tincture_spirit(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerTinctureForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    let spirit = state.db.query::<InventoryItem>(&format!("SELECT * FROM inventory_item WHERE character_id = {}", actor.id)).await
        .unwrap_or_default().into_iter().filter(|row| row.item_id == "tincture_spirit" && row.qty > 0).min_by_key(|row| row.id);
    let Some(spirit) = spirit else { return (StatusCode::BAD_REQUEST, "No tincture spirit is carried").into_response(); };
    match state.db.call("pour_tincture_spirit_into_container", &[json!(actor.id), json!(spirit.id), json!(form.container_object_id)]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn start_inventory_container_tincture(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerTinctureForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    match state.db.call("start_poppy_tincture", &[json!(actor.id), json!(form.container_object_id)]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn refresh_inventory_container_tincture(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerTinctureForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    match state.db.call("refresh_tincture", &[json!(actor.id), json!(form.container_object_id)]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn dose_inventory_container_tincture(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerTinctureDoseForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    match state.db.call("administer_tincture_from_container", &[json!(actor.id), json!(actor.id),
        json!(form.container_object_id), json!(form.amount_milliunits)]).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(), Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod preparation_ui_tests {
    #[test]
    fn typed_liquid_and_tincture_controls_use_closed_reducers() {
        let route = include_str!("containers.rs");
        let script = include_str!("../../../../static/inventory-browser.js");
        assert!(route.contains("pour_tincture_spirit_into_container"));
        assert!(route.contains("start_poppy_tincture"));
        assert!(script.contains("data-container-spirit"));
        assert!(script.contains("data-container-tincture"));
    }
}
