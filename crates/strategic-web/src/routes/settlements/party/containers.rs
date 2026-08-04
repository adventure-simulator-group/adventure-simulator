#[derive(Debug, Serialize)]
pub(super) struct ContainerSnapshot {
    objects: Vec<InventoryObject>,
    edges: Vec<InventoryContainment>,
    liquids: Vec<ContainerLiquid>,
}

#[derive(Deserialize)]
pub(super) struct ContainerMoveForm {
    #[serde(default)] child_object_id: u64,
    #[serde(default)] child_scope: String,
    #[serde(default)] child_row_id: u64,
    parent_object_id: u64,
}

#[derive(Deserialize)]
pub(super) struct ContainerRemoveForm {
    child_object_id: u64,
}
#[derive(Deserialize)]
pub(super) struct ContainerOpenForm { inventory_scope: String, inventory_row_id: u64 }
#[derive(Deserialize)]
pub(super) struct ContainerWaterForm { container_object_id: u64, requested_ml: u64 }

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
    Json(ContainerSnapshot { objects, edges, liquids }).into_response()
}

pub(super) async fn open_inventory_container(
    State(state): State<AppState>, session: Session, Form(form): Form<ContainerOpenForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let item_id = match form.inventory_scope.as_str() {
        "personal" => state.db.query_one::<InventoryItem>(&format!("SELECT * FROM inventory_item WHERE id = {}", form.inventory_row_id)).await.ok().flatten().map(|row| row.item_id),
        "party" => state.db.query_one::<PartyInventoryItem>(&format!("SELECT * FROM party_inventory_item WHERE id = {}", form.inventory_row_id)).await.ok().flatten().map(|row| row.item_id),
        _ => None,
    };
    let Some(item_id) = item_id else { return (StatusCode::BAD_REQUEST, "Inventory row not found").into_response(); };
    if let Err(error) = state.db.call("open_inventory_container", &[
        json!(actor.id), json!(form.inventory_scope), json!(form.inventory_row_id),
    ]).await {
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    let owner = if form.inventory_scope == "personal" { actor.id.to_string() }
        else { actor.party_id.unwrap_or_default() };
    let object = state.db.query::<InventoryObject>("SELECT * FROM inventory_object").await
        .unwrap_or_default().into_iter().filter(|row| row.location_kind == form.inventory_scope
            && row.location_owner == owner && row.item_id == item_id).max_by_key(|row| row.id);
    match object { Some(row) => Json(row).into_response(), None => StatusCode::NOT_FOUND.into_response() }
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
    let Some(parent) = owned_container_object(&state, &actor, form.parent_object_id).await else {
        return (StatusCode::BAD_REQUEST, "Container is not in your inventory").into_response();
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
