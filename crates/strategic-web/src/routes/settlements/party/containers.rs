#[derive(Serialize)]
pub(super) struct ContainerSnapshot {
    objects: Vec<spacetimedb_sats::serde::SerdeWrapper<InventoryObject>>,
    edges: Vec<spacetimedb_sats::serde::SerdeWrapper<InventoryContainment>>,
    liquids: Vec<spacetimedb_sats::serde::SerdeWrapper<ContainerLiquid>>,
    presentations: Vec<ContainerItemPresentation>,
    tinctures: Vec<spacetimedb_sats::serde::SerdeWrapper<BackendTinctureStatus>>,
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
    child_object_id: u64,
    parent_object_id: u64,
}

#[derive(Clone, Copy)]
struct CarriedInventoryView {
    scope: CarriedInventoryScope,
    row_id: u64,
}

impl CarriedInventoryView {
    fn from_location(location: &InventoryLocation) -> Option<Self> {
        match location {
            InventoryLocation::Personal(location) => Some(Self {
                scope: CarriedInventoryScope::Personal,
                row_id: location.row_id,
            }),
            InventoryLocation::Party(location) => Some(Self {
                scope: CarriedInventoryScope::Party,
                row_id: location.row_id,
            }),
            InventoryLocation::Fireplace(_) | InventoryLocation::Repair(_) => None,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct ContainerRemoveForm {
    child_object_id: u64,
}
#[derive(Deserialize)]
pub(super) struct ContainerWaterForm {
    container_object_id: u64,
    requested_ml: u64,
}
#[derive(Deserialize)]
pub(super) struct ContainerTinctureForm {
    container_object_id: u64,
}
#[derive(Deserialize)]
pub(super) struct ContainerTinctureDoseForm {
    container_object_id: u64,
    #[serde(default = "default_tincture_dose")]
    dose_milliunits: u32,
}
fn default_tincture_dose() -> u32 {
    100
}

pub(super) async fn inventory_containers(
    State(state): State<AppState>,
    session: Session,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let fireplace_roots = state
        .db
        .query_sats::<BackendFireplaceStation>(&format!(
            "SELECT * FROM backend_fireplace_stations WHERE character_id = {}",
            actor.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.instrument_object_id)
        .collect::<HashSet<_>>();
    let objects = state
        .db
        .query_sats::<InventoryObject>("SELECT * FROM inventory_object")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| match &row.location {
            InventoryLocation::Personal(location) => location.character_id == actor.id,
            InventoryLocation::Party(location) => {
                actor.party_id.as_deref() == Some(location.party_id.as_str())
            }
            InventoryLocation::Fireplace(_) => fireplace_roots.contains(&row.id),
            InventoryLocation::Repair(_) => false,
        })
        .collect::<Vec<_>>();
    let ids = objects.iter().map(|row| row.id).collect::<HashSet<_>>();
    let edges = state
        .db
        .query_sats::<InventoryContainment>("SELECT * FROM inventory_containment")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| ids.contains(&row.child_object_id) && ids.contains(&row.parent_object_id))
        .collect::<Vec<_>>();
    let liquids: Vec<ContainerLiquid> = state
        .db
        .query_sats::<ContainerLiquid>("SELECT * FROM container_liquid")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| ids.contains(&row.container_object_id))
        .collect();
    let definitions = state
        .db
        .query_sats_into::<adventuresim_stdb_client::Item, CatalogItemView>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let lots = state
        .db
        .query_sats::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let personal = state
        .db
        .query_sats::<InventoryItem>(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {}",
            actor.id
        ))
        .await
        .unwrap_or_default();
    let party = if let Some(party_id) = actor.party_id.as_deref() {
        state
            .db
            .query_sats::<PartyInventoryItem>(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let presentations = objects
        .iter()
        .filter_map(|object| {
            let carried = CarriedInventoryView::from_location(&object.location)?;
            let quantity = match carried.scope {
                CarriedInventoryScope::Personal => personal
                    .iter()
                    .find(|row| row.id == carried.row_id)
                    .map(|row| row.quantity),
                CarriedInventoryScope::Party => party
                    .iter()
                    .find(|row| row.id == carried.row_id)
                    .map(|row| row.quantity),
            }?;
            let definition = definitions.iter().find(|row| row.id == object.item_id)?;
            let display_name = lots
                .iter()
                .find(|lot| match carried.scope {
                    CarriedInventoryScope::Personal => {
                        lot.inventory_item_id == Some(carried.row_id)
                    }
                    CarriedInventoryScope::Party => {
                        lot.party_inventory_item_id == Some(carried.row_id)
                    }
                })
                .map_or_else(
                    || object.item_id.replace('_', " "),
                    |lot| lot.display_name.clone(),
                );
            Some(ContainerItemPresentation {
                object_id: object.id,
                item_id: object.item_id.clone(),
                display_name,
                quantity,
                exterior_volume_ml: definition.exterior_volume_ml,
                container_capacity_ml: definition.container_capacity_ml,
                tincture_vessel: adventuresim_core::item_catalog::definition(&object.item_id)
                    .is_some_and(|definition| {
                        definition.tags.iter().any(|tag| tag == "tincture_vessel")
                    }),
            })
        })
        .collect();
    let existing = state
        .db
        .query_sats::<BackendTinctureStatus>("SELECT * FROM backend_tincture_statuses")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| ids.contains(&row.container_object_id))
        .collect::<Vec<_>>();
    for status in &existing {
        let _ = state
            .db
            .call(
                "refresh_tincture",
                &[json!(actor.id), json!(status.container_object_id)],
            )
            .await;
    }
    let tinctures: Vec<BackendTinctureStatus> = state
        .db
        .query_sats::<BackendTinctureStatus>("SELECT * FROM backend_tincture_statuses")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| ids.contains(&row.container_object_id))
        .collect();
    Json(ContainerSnapshot {
        objects: objects
            .into_iter()
            .map(spacetimedb_sats::serde::SerdeWrapper::new)
            .collect(),
        edges: edges
            .into_iter()
            .map(spacetimedb_sats::serde::SerdeWrapper::new)
            .collect(),
        liquids: liquids
            .into_iter()
            .map(spacetimedb_sats::serde::SerdeWrapper::new)
            .collect(),
        presentations,
        tinctures: tinctures
            .into_iter()
            .map(spacetimedb_sats::serde::SerdeWrapper::new)
            .collect(),
    })
    .into_response()
}

async fn owned_container_object(
    state: &AppState,
    actor: &CharacterView,
    id: u64,
) -> Option<InventoryObject> {
    let row = state
        .db
        .query_one_sats::<InventoryObject>(&crate::spacetimedb::inventory_object_by_id(id))
        .await
        .ok()
        .flatten()?;
    match &row.location {
        InventoryLocation::Personal(location) if location.character_id == actor.id => Some(row),
        InventoryLocation::Party(location)
            if actor.party_id.as_deref() == Some(location.party_id.as_str()) =>
        {
            Some(row)
        }
        InventoryLocation::Personal(_)
        | InventoryLocation::Party(_)
        | InventoryLocation::Fireplace(_)
        | InventoryLocation::Repair(_) => None,
    }
}

pub(super) async fn move_inventory_container_item(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerMoveForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(parent) = owned_container_object(&state, &actor, form.parent_object_id).await else {
        return (
            StatusCode::BAD_REQUEST,
            "Container is not in your inventory",
        )
            .into_response();
    };
    let Some(child) = owned_container_object(&state, &actor, form.child_object_id).await else {
        return (StatusCode::BAD_REQUEST, "Item is not in your inventory").into_response();
    };
    let (Some(child), Some(parent)) = (
        CarriedInventoryView::from_location(&child.location),
        CarriedInventoryView::from_location(&parent.location),
    ) else {
        return (StatusCode::BAD_REQUEST, "Item is not in carried inventory").into_response();
    };
    match state
        .db
        .call(
            "put_inventory_item_in_container",
            &[
                json!(actor.id),
                json!(child.scope.as_str()),
                json!(child.row_id),
                json!(parent.scope.as_str()),
                json!(parent.row_id),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn remove_inventory_container_item(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerRemoveForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state
        .db
        .call(
            "remove_inventory_item_from_container",
            &[json!(actor.id), json!(form.child_object_id)],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn discard_inventory_container_water(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerWaterForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state
        .db
        .call(
            "discard_container_water",
            &[
                json!(actor.id),
                json!(form.container_object_id),
                json!(form.requested_ml),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn pour_inventory_container_tincture_spirit(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerTinctureForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let spirit = state
        .db
        .query_sats::<InventoryItem>(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {}",
            actor.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.item_id == "tincture_spirit" && row.quantity > 0)
        .min_by_key(|row| row.id);
    let Some(spirit) = spirit else {
        return (StatusCode::BAD_REQUEST, "No tincture spirit is carried").into_response();
    };
    match state
        .db
        .call(
            "pour_tincture_spirit_into_container",
            &[
                json!(actor.id),
                json!(spirit.id),
                json!(form.container_object_id),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn start_inventory_container_tincture(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerTinctureForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state
        .db
        .call(
            "start_poppy_tincture",
            &[json!(actor.id), json!(form.container_object_id)],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn refresh_inventory_container_tincture(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerTinctureForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state
        .db
        .call(
            "refresh_tincture",
            &[json!(actor.id), json!(form.container_object_id)],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn dose_inventory_container_tincture(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ContainerTinctureDoseForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(dose) = adventuresim_core::physiology::DoseMilliunits::try_new(form.dose_milliunits)
    else {
        return (StatusCode::BAD_REQUEST, "Invalid tincture dose").into_response();
    };
    if dose.is_zero() || dose > adventuresim_core::physiology::DoseMilliunits::STANDARD {
        return (StatusCode::BAD_REQUEST, "Invalid tincture dose").into_response();
    }
    match state
        .db
        .call(
            "administer_tincture_from_container",
            &[
                json!(actor.id),
                json!(actor.id),
                json!(form.container_object_id),
                json!(dose.get()),
            ],
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
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
use adventuresim_core::physical_object::CarriedInventoryScope;
use crate::spacetimedb::InventoryLocation;
