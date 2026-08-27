//! Authenticated, transient per-instance weapon silhouettes.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use adventuresim_core::physical_object::{CarriedInventoryScope, InventoryLocation};
use adventuresim_weapon_model::{
    DesignHash, GENERATOR_VERSION, HOLDER_GENERATOR_VERSION, ICON_RENDERER_VERSION, WeaponIconSpec,
    decode, decode_holder, design_hash, generate_holder_icon, generate_icon, holder_design_hash,
};
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::{
    routes::AppState,
    session::Session,
    spacetimedb::{BackendWeaponHolderInstance, BackendWeaponInstance, Character, InventoryObject},
};

const ICON_SIZE: u16 = 96;
const ICON_SUPERSAMPLING: u8 = 4;
const CACHE_LIMIT: usize = 512;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    source: IconSource,
    generator_version: u16,
    icon_renderer_version: u16,
    design_hash: DesignHash,
    size: u16,
    supersampling: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum IconSource {
    Weapon,
    Holder,
}

static ICON_CACHE: OnceLock<Mutex<HashMap<CacheKey, Vec<u8>>>> = OnceLock::new();

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/weapon-icons/{scope}/{filename}", get(weapon_icon))
}

async fn weapon_icon(
    State(state): State<AppState>,
    session: Session,
    Path((scope, filename)): Path<(String, String)>,
) -> Response {
    let Some(row_id) = filename
        .strip_suffix(".png")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(scope) = CarriedInventoryScope::try_from(scope.as_str()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if row_id == 0 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(actor_id) = session.character_id_u64() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let actor = match state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM backend_characters WHERE id = {actor_id}"
        ))
        .await
    {
        Ok(Some(actor)) => actor,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(error) => {
            tracing::error!(%error, actor_id, "failed to resolve weapon-icon actor");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let objects = match state
        .db
        .query::<InventoryObject>("SELECT * FROM inventory_object")
        .await
    {
        Ok(objects) => objects
            .into_iter()
            .filter(|object| carried_row_matches(&object.location, scope, row_id))
            .collect::<Vec<_>>(),
        Err(error) => {
            tracing::error!(%error, scope = scope.as_str(), row_id, "failed to resolve weapon-icon object");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let [object] = objects.as_slice() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let owner_party = match &object.location {
        InventoryLocation::Personal(location) if location.character_id != actor.id => state
            .db
            .query_one::<Character>(&format!(
                "SELECT * FROM backend_characters WHERE id = {}",
                location.character_id
            ))
            .await
            .ok()
            .flatten()
            .and_then(|owner| owner.party_id),
        InventoryLocation::Personal(_)
        | InventoryLocation::Party(_)
        | InventoryLocation::Fireplace(_)
        | InventoryLocation::Repair(_) => None,
    };
    if !custody_visible(
        actor.id,
        actor.party_id.as_deref(),
        object,
        owner_party.as_deref(),
    ) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let weapon_instance = match state
        .db
        .query_one::<BackendWeaponInstance>(&format!(
            "SELECT * FROM backend_weapon_instances WHERE physical_object_id = {}",
            object.id
        ))
        .await
    {
        Ok(instance) => instance,
        Err(error) => {
            tracing::error!(%error, object_id = object.id, "failed to resolve weapon-icon recipe");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let rendered = if let Some(instance) = weapon_instance {
        authenticated_icon(object, &instance)
    } else {
        let holder = match state
            .db
            .query_one::<BackendWeaponHolderInstance>(&format!(
                "SELECT * FROM backend_weapon_holder_instances WHERE physical_object_id = {}",
                object.id
            ))
            .await
        {
            Ok(Some(instance)) => instance,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => {
                tracing::error!(%error, object_id = object.id, "failed to resolve holder-icon recipe");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        };
        authenticated_holder_icon(object, &holder)
    };
    let (generator_version, hash, png) = match rendered {
        Ok(icon) => icon,
        Err(error) => {
            tracing::warn!(%error, object_id = object.id, "rejected equipment-icon recipe");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "private, no-store")
        .header(
            header::ETAG,
            format!(
                "\"equipment-icon-{generator_version}-{ICON_RENDERER_VERSION}-{}\"",
                hash.to_hex()
            ),
        )
        .body(Body::from(png))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn carried_row_matches(
    location: &InventoryLocation,
    scope: CarriedInventoryScope,
    row_id: u64,
) -> bool {
    match (scope, location) {
        (CarriedInventoryScope::Personal, InventoryLocation::Personal(location)) => {
            location.row_id == row_id
        }
        (CarriedInventoryScope::Party, InventoryLocation::Party(location)) => {
            location.row_id == row_id
        }
        (
            CarriedInventoryScope::Personal,
            InventoryLocation::Party(_)
            | InventoryLocation::Fireplace(_)
            | InventoryLocation::Repair(_),
        )
        | (
            CarriedInventoryScope::Party,
            InventoryLocation::Personal(_)
            | InventoryLocation::Fireplace(_)
            | InventoryLocation::Repair(_),
        ) => false,
    }
}

fn custody_visible(
    actor_id: u64,
    actor_party: Option<&str>,
    object: &InventoryObject,
    personal_owner_party: Option<&str>,
) -> bool {
    match &object.location {
        InventoryLocation::Personal(location) => {
            location.character_id == actor_id
                || actor_party.is_some_and(|party| Some(party) == personal_owner_party)
        }
        InventoryLocation::Party(location) => actor_party == Some(location.party_id.as_str()),
        InventoryLocation::Fireplace(_) | InventoryLocation::Repair(_) => false,
    }
}

fn authenticated_icon(
    object: &InventoryObject,
    instance: &BackendWeaponInstance,
) -> Result<(u16, DesignHash, Vec<u8>), String> {
    if instance.physical_object_id != object.id || instance.generator_version != GENERATOR_VERSION {
        return Err("weapon instance identity or generator version mismatch".into());
    }
    let design = decode(&instance.recipe).map_err(|error| error.to_string())?;
    if design.catalog_id != object.item_id {
        return Err("weapon recipe catalog chassis mismatch".into());
    }
    let hash = design_hash(&design);
    if instance.design_hash.as_slice() != hash.0.as_slice() {
        return Err("weapon recipe hash mismatch".into());
    }
    let key = CacheKey {
        source: IconSource::Weapon,
        generator_version: instance.generator_version,
        icon_renderer_version: ICON_RENDERER_VERSION,
        design_hash: hash,
        size: ICON_SIZE,
        supersampling: ICON_SUPERSAMPLING,
    };
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(png) = cache
        .lock()
        .map_err(|_| "weapon icon cache is unavailable")?
        .get(&key)
        .cloned()
    {
        return Ok((GENERATOR_VERSION, hash, png));
    }
    let png = generate_icon(
        &design,
        WeaponIconSpec {
            size: ICON_SIZE,
            supersampling: ICON_SUPERSAMPLING,
        },
    )
    .map_err(|error| error.to_string())?
    .encode_png()
    .map_err(|error| error.to_string())?;
    let mut cache = cache
        .lock()
        .map_err(|_| "weapon icon cache is unavailable")?;
    if cache.len() >= CACHE_LIMIT
        && let Some(evicted) = cache.keys().next().copied()
    {
        cache.remove(&evicted);
    }
    cache.insert(key, png.clone());
    Ok((GENERATOR_VERSION, hash, png))
}

fn authenticated_holder_icon(
    object: &InventoryObject,
    instance: &BackendWeaponHolderInstance,
) -> Result<(u16, DesignHash, Vec<u8>), String> {
    if instance.physical_object_id != object.id
        || instance.generator_version != HOLDER_GENERATOR_VERSION
    {
        return Err("holder instance identity or generator version mismatch".into());
    }
    let design = decode_holder(&instance.recipe).map_err(|error| error.to_string())?;
    if design.catalog_id != object.item_id {
        return Err("holder recipe catalog chassis mismatch".into());
    }
    let hash = holder_design_hash(&design);
    if instance.design_hash.as_slice() != hash.0.as_slice() {
        return Err("holder recipe hash mismatch".into());
    }
    let key = CacheKey {
        source: IconSource::Holder,
        generator_version: instance.generator_version,
        icon_renderer_version: ICON_RENDERER_VERSION,
        design_hash: hash,
        size: ICON_SIZE,
        supersampling: ICON_SUPERSAMPLING,
    };
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(png) = cache
        .lock()
        .map_err(|_| "equipment icon cache is unavailable")?
        .get(&key)
        .cloned()
    {
        return Ok((HOLDER_GENERATOR_VERSION, hash, png));
    }
    let png = generate_holder_icon(
        &design,
        WeaponIconSpec {
            size: ICON_SIZE,
            supersampling: ICON_SUPERSAMPLING,
        },
    )
    .map_err(|error| error.to_string())?
    .encode_png()
    .map_err(|error| error.to_string())?;
    let mut cache = cache
        .lock()
        .map_err(|_| "equipment icon cache is unavailable")?;
    if cache.len() >= CACHE_LIMIT
        && let Some(evicted) = cache.keys().next().copied()
    {
        cache.remove(&evicted);
    }
    cache.insert(key, png.clone());
    Ok((HOLDER_GENERATOR_VERSION, hash, png))
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_weapon_model::{default_design, default_holder_design, encode, encode_holder};

    fn object(location: InventoryLocation) -> InventoryObject {
        InventoryObject {
            id: 7,
            item_id: "longsword".into(),
            location,
        }
    }

    #[test]
    fn custody_gate_allows_self_and_party_but_not_foreign_rows() {
        assert!(custody_visible(
            3,
            None,
            &object(InventoryLocation::personal(3, 9)),
            None
        ));
        assert!(custody_visible(
            3,
            Some("party-a"),
            &object(InventoryLocation::personal(4, 9)),
            Some("party-a")
        ));
        assert!(custody_visible(
            3,
            Some("party-a"),
            &object(InventoryLocation::party("party-a", 9)),
            None
        ));
        assert!(!custody_visible(
            3,
            Some("party-a"),
            &object(InventoryLocation::personal(4, 9)),
            Some("party-b")
        ));
        assert!(!custody_visible(
            3,
            Some("party-a"),
            &object(InventoryLocation::repair("smithy", 9)),
            None
        ));
    }

    #[test]
    fn icon_cache_authenticates_before_a_warm_hit() {
        let object = object(InventoryLocation::personal(3, 9));
        let design = default_design("longsword").unwrap();
        let hash = design_hash(&design);
        let recipe = encode(&design).unwrap();
        let instance = BackendWeaponInstance {
            physical_object_id: object.id,
            generator_version: GENERATOR_VERSION,
            design_hash: hash.0.to_vec(),
            recipe: recipe.clone(),
            mass_grams: 1,
            length_mm: 1,
            grip_to_tip_mm: 1,
        };
        let (_, _, first) = authenticated_icon(&object, &instance).unwrap();
        let (_, _, second) = authenticated_icon(&object, &instance).unwrap();
        assert_eq!(first, second);

        let mut tampered = instance;
        tampered.recipe = encode(&default_design("rondel_dagger").unwrap()).unwrap();
        assert!(authenticated_icon(&object, &tampered).is_err());
    }

    #[test]
    fn holder_icon_cache_authenticates_before_a_warm_hit() {
        let mut object = object(InventoryLocation::personal(3, 9));
        object.item_id = "scabbard".into();
        let weapon = default_design("longsword").unwrap();
        let design = default_holder_design(&weapon).unwrap();
        let hash = holder_design_hash(&design);
        let instance = BackendWeaponHolderInstance {
            physical_object_id: object.id,
            generator_version: HOLDER_GENERATOR_VERSION,
            design_hash: hash.0.to_vec(),
            recipe: encode_holder(&design).unwrap(),
            mass_grams: 1,
            length_mm: 1,
            grip_to_tip_mm: 1,
        };
        let (_, _, first) = authenticated_holder_icon(&object, &instance).unwrap();
        let (_, _, second) = authenticated_holder_icon(&object, &instance).unwrap();
        assert_eq!(first, second);

        let mut tampered = instance;
        let other = default_holder_design(&default_design("rondel_dagger").unwrap()).unwrap();
        tampered.recipe = encode_holder(&other).unwrap();
        assert!(authenticated_holder_icon(&object, &tampered).is_err());
    }

    #[test]
    fn inventory_browser_progressively_replaces_only_instanced_melee_icons() {
        let script = include_str!("../../static/inventory-browser.js");
        assert!(script.contains("hydrateProceduralWeaponIcons"));
        assert!(script.contains(".inventory-item-label[data-item-melee=\"true\"]"));
        assert!(script.contains(".inventory-item-label[data-item-weapon-holder=\"true\"]"));
        assert!(script.contains("/api/weapon-icons/${scope}/${rowId}.png"));
        assert!(script.contains("authored catalog SVG remains"));
    }
}
