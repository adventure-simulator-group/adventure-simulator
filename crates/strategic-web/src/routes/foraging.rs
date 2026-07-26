use axum::{
    Form, Router,
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::{Markup, html};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{AppState, local_return_url, persisted_route_position};
use crate::{
    session::Session,
    spacetimedb::{
        BackendCaseSitePin, BackendForageReceipt, Character, Party, PartyJourney,
        PartyJourneyRoute, Settlement, sql_string_literal,
    },
};

static NEXT_FORAGE_REQUEST: AtomicU64 = AtomicU64::new(1);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/forage", get(menu))
        .route("/forage", post(perform))
}

#[derive(Default, Deserialize)]
struct ForageQuery {
    return_to: Option<String>,
}

#[derive(Deserialize)]
struct ForageForm {
    #[serde(default)]
    target: Vec<String>,
    hours: u8,
    return_to: String,
}

struct Vicinity {
    kind: String,
    id: String,
    latitude: f64,
    longitude: f64,
    settlement: bool,
}

async fn vicinity(state: &AppState, character: &Character) -> Result<Vicinity, String> {
    if let Some(id) = character.current_settlement_id.as_deref() {
        let row = state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Current settlement not found")?;
        return Ok(Vicinity {
            kind: "settlement".into(),
            id: id.into(),
            latitude: row.coord_y,
            longitude: row.coord_x,
            settlement: true,
        });
    }
    if let Some(id) = character.current_case_site_id.as_deref() {
        let row = state
            .db
            .query_one::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {} AND case_site_id = {}",
                character.id,
                sql_string_literal(id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Current case site is not exact")?;
        return Ok(Vicinity {
            kind: "case_site".into(),
            id: id.into(),
            latitude: f64::from(row.latitude_e7) / 10_000_000.0,
            longitude: f64::from(row.longitude_e7) / 10_000_000.0,
            settlement: false,
        });
    }
    let party_id = character
        .party_id
        .as_deref()
        .ok_or("Character has no stationary vicinity")?;
    let party = state
        .db
        .query_one::<Party>(&format!(
            "SELECT * FROM party WHERE id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Party not found")?;
    if party.camp_destination.is_none() {
        return Err("Foraging is unavailable while moving or without a known location".into());
    }
    let journey = state
        .db
        .query_one::<PartyJourney>(&format!(
            "SELECT * FROM party_journey WHERE party_id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Camp journey not found")?;
    let route = state
        .db
        .query_one::<PartyJourneyRoute>(&format!(
            "SELECT * FROM party_journey_route WHERE party_id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Camp terrain route not found")?;
    let (latitude, longitude) = persisted_route_position(&route, journey.completed_minutes)
        .ok_or("Camp terrain position is unavailable")?;
    Ok(Vicinity {
        kind: "camp".into(),
        id: party_id.into(),
        latitude,
        longitude,
        settlement: false,
    })
}

fn forage_request_id(character_id: u64) -> String {
    let counter = NEXT_FORAGE_REQUEST.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{:x}",
        Sha256::digest(
            [
                b"forage-request-v1".as_slice(),
                &character_id.to_le_bytes(),
                &counter.to_le_bytes(),
                &nanos.to_le_bytes(),
            ]
            .concat()
        )
    )
}

fn forage_receipt_query(character_id: u64, request_id: &str) -> String {
    format!(
        "SELECT * FROM backend_forage_receipts WHERE character_id = {character_id} AND request_id = {}",
        sql_string_literal(request_id)
    )
}

fn valid_forage_request_id(request_id: &str) -> bool {
    request_id.len() == 64 && request_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn forage_result_href(return_to: &str, request_id: &str) -> String {
    format!(
        "{return_to}{}forage=true&forage_receipt={request_id}",
        if return_to.contains('?') { "&" } else { "?" }
    )
}

fn forage_error_message(code: &str) -> Option<&'static str> {
    match code {
        "location" => Some("Foraging is unavailable at this location."),
        "targets" => Some("Choose valid forage targets and try again."),
        "duration" => Some("Choose a valid search duration and try again."),
        "unavailable" => Some("The search could not be completed."),
        _ => None,
    }
}

fn forage_error_code(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("target") || error.contains("resource") {
        "targets"
    } else if error.contains("hour") || error.contains("minute") || error.contains("duration") {
        "duration"
    } else if [
        "terrain",
        "water",
        "vicinity",
        "location",
        "settlement",
        "case site",
        "camp",
        "journey",
        "moving",
    ]
    .iter()
    .any(|term| error.contains(term))
    {
        "location"
    } else {
        "unavailable"
    }
}

fn forage_error_href(return_to: &str, code: &str) -> String {
    let code = forage_error_message(code)
        .map(|_| code)
        .unwrap_or("unavailable");
    format!(
        "{return_to}{}forage=true&forage_error={code}",
        if return_to.contains('?') { "&" } else { "?" }
    )
}

fn forage_receipt_status(receipt: &BackendForageReceipt) -> Markup {
    html! {
        div role="status" aria-live="polite" {
            @if receipt.interrupted {
                p { "The search was interrupted after " (receipt.elapsed_minutes / 60) " hour(s). Nothing was gathered." }
            } @else if receipt.yielded_item_ids.is_empty() {
                p { "The search found nothing." }
            } @else {
                h3 { "Gathered" }
                ul {
                    @for (item, quantity) in receipt.yielded_item_ids.iter().zip(&receipt.yielded_quantities) {
                        li { (quantity) " × " (adventuresim_core::foraging::resource(item).map_or(item.as_str(), |resource| resource.name)) }
                    }
                }
            }
            @if receipt.legal_outcome == "unnoticed" {
                p { "The illegal search went unnoticed." }
            } @else if receipt.legal_outcome == "noticed" {
                p { "The illegal search was noticed. Virtue -1.0." }
            }
        }
    }
}

async fn character(state: &AppState, id: u64) -> Result<Character, String> {
    state
        .db
        .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {id}"))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Character not found".into())
}

async fn environment(
    state: &AppState,
    character: &Character,
) -> Result<
    (
        adventuresim_core::foraging::ForageEnvironment,
        serde_json::Value,
    ),
    String,
> {
    let location = vicinity(state, character).await?;
    let terrain = state
        .terrain
        .as_deref()
        .ok_or("Terrain data is unavailable")?;
    let (cell, river_or_wet, coastal) =
        terrain.forage_environment(location.latitude, location.longitude)?;
    if cell.surface == adventuresim_terrain::Surface::Water && !cell.crossing {
        return Err("Foraging is unavailable on open water".into());
    }
    let weights = cell.terrain_weights();
    let mixture = adventuresim_core::foraging::LocalTerrainMixture {
        plains: weights.plains + weights.urban,
        forest: weights.forest,
        hills: weights.hills,
    };
    let environment = adventuresim_core::foraging::ForageEnvironment {
        terrain: mixture,
        river_or_wet_ground: river_or_wet,
        sea_or_coast: coastal,
        cultivated: cell.cultivated,
        settlement: location.settlement,
    };
    let attestation = json!({
        "package_digest": terrain.digest(),
        "latitude_e7": (location.latitude * 10_000_000.0).round() as i32,
        "longitude_e7": (location.longitude * 10_000_000.0).round() as i32,
        "context_kind": location.kind,
        "context_id": location.id,
        "plains": mixture.plains,
        "forest": mixture.forest,
        "hills": mixture.hills,
        "river_or_wet_ground": environment.river_or_wet_ground,
        "sea_or_coast": environment.sea_or_coast,
        "cultivated": environment.cultivated,
    });
    Ok((environment, attestation))
}

pub(crate) async fn activity_dialog(
    state: &AppState,
    character: &Character,
    return_to: &str,
    receipt_id: Option<&str>,
    error_code: Option<&str>,
) -> Markup {
    let receipt = if let Some(request_id) = receipt_id.filter(|id| valid_forage_request_id(id)) {
        state
            .db
            .query_one::<BackendForageReceipt>(&forage_receipt_query(character.id, request_id))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let error_message = error_code.and_then(forage_error_message);
    let outcome = environment(state, character).await;
    let (environment, unavailable) = match outcome {
        Ok((environment, _)) => (Some(environment), None),
        Err(error) => (
            None,
            Some(
                forage_error_message(forage_error_code(&error))
                    .unwrap_or("The search could not be completed."),
            ),
        ),
    };
    let resources = environment
        .as_ref()
        .map(|environment| {
            adventuresim_core::foraging::FORAGE_RESOURCES
                .iter()
                .filter(|resource| adventuresim_core::foraging::available(resource, *environment))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let illegal =
        environment.is_some_and(|environment| environment.settlement || environment.cultivated);
    html! {
        div class="character-action-overlay" data-character-action-dialog
            data-initial-focus=(if receipt.is_some() { ".modal-actions .btn" } else { "#forage-targets input:not(:disabled)" }) {
            a class="character-action-backdrop" href=(return_to) aria-label="Close foraging dialog" {}
            section class="character-action-dialog forage-dialog" role="dialog" aria-modal="true"
                aria-labelledby="forage-title" aria-describedby="forage-description" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="forage-title" { "Forage nearby" }
                    a class="character-action-dialog-close" href=(return_to) aria-label="Close foraging dialog" { "×" }
                }
                p id="forage-description" { "Search only the character's immediate vicinity. Multiple targets share the selected time." }
                @if let Some(receipt) = receipt.as_ref() {
                    (forage_receipt_status(receipt))
                    div class="modal-actions" {
                        a class="btn btn-primary character-action-dialog-close" href=(return_to) { "Return" }
                    }
                } @else if let Some(reason) = unavailable {
                    p role="alert" class="badge badge-danger" { (reason) }
                    div class="modal-actions" {
                        a class="btn btn-secondary character-action-dialog-close" href=(return_to) { "Return" }
                    }
                } @else {
                    @if let Some(message) = error_message {
                        p role="alert" class="badge badge-danger" { (message) }
                    }
                    @if illegal {
                        p role="alert" class="badge badge-warning" { "Foraging here is illegal. One Stealth check is made when the search completes; failure costs 1 Virtue." }
                    }
                    form method="post" action="/forage" {
                        input type="hidden" name="return_to" value=(return_to);
                        p id="forage-target-limit" class="text-muted small-copy" { "Choose at most eight targets. Every selected target shares the same search time." }
                        fieldset id="forage-targets" aria-describedby="forage-target-limit" {
                            legend { "Targets" }
                            @for resource in resources {
                                label class="inventory-row" {
                                    input type="checkbox" name="target" value=(resource.item_id);
                                    span { (resource.name) " · " (format!("{:?}", resource.rarity)) }
                                }
                            }
                        }
                        label for="forage-hours" { "Search plan" }
                        input id="forage-hours" name="hours" type="range" min="1" max="24" value="4"
                            oninput="this.nextElementSibling.value=this.value + ' hours'";
                        output { "4 hours" }
                        div class="modal-actions" {
                            button class="btn btn-primary" type="submit" { "Begin search" }
                            a class="btn btn-secondary character-action-dialog-close" href=(return_to) { "Cancel" }
                        }
                    }
                }
            }
        }
    }
}

async fn menu(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<ForageQuery>,
) -> Response {
    let return_to = query
        .return_to
        .as_deref()
        .and_then(local_return_url)
        .unwrap_or("/");
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let Ok(character) = character(&state, character_id).await else {
        return Redirect::to("/characters").into_response();
    };
    Redirect::to(&integrated_forage_href(&character, return_to)).into_response()
}

fn integrated_forage_href(character: &Character, return_to: &str) -> String {
    if return_to == "/camp" {
        return "/camp?forage=true".into();
    }
    if return_to.starts_with("/locations/") && return_to.contains("/party/") {
        return format!(
            "{return_to}{}forage=true",
            if return_to.contains('?') { "&" } else { "?" }
        );
    }
    if let Some(id) = character.current_settlement_id.as_deref() {
        return format!(
            "/locations/settlement/{id}/party/{}?forage=true",
            character.id
        );
    }
    if let Some(id) = character.current_case_site_id.as_deref() {
        return format!(
            "/locations/case-site/{id}/party/{}?forage=true",
            character.id
        );
    }
    "/camp?forage=true".into()
}

async fn perform(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ForageForm>,
) -> Response {
    let return_to = local_return_url(&form.return_to).unwrap_or("/");
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let result = async {
        let character = character(&state, character_id).await?;
        let (environment, attestation) = environment(&state, &character).await?;
        let minutes = u64::from(form.hours) * 60;
        let request_id = forage_request_id(character_id);
        state
            .db
            .call(
                "forage_current_vicinity",
                &[
                    json!(character_id),
                    json!(&request_id),
                    json!(&form.target),
                    json!(minutes),
                    attestation,
                ],
            )
            .await
            .map_err(|error| error.to_string())?;
        let attempt = state
            .db
            .query_one::<BackendForageReceipt>(&forage_receipt_query(character_id, &request_id))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Foraging completed but its result is not visible yet")?;
        Ok::<_, String>((environment, attempt, request_id))
    }
    .await;
    match result {
        Ok((_environment, _attempt, request_id)) => {
            Redirect::to(&forage_result_href(return_to, &request_id)).into_response()
        }
        Err(error) => {
            Redirect::to(&forage_error_href(return_to, forage_error_code(&error))).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_lookup_is_exact_for_character_and_opaque_request() {
        let request = "a".repeat(64);
        assert_eq!(
            forage_receipt_query(17, &request),
            format!(
                "SELECT * FROM backend_forage_receipts WHERE character_id = 17 AND request_id = '{request}'"
            )
        );
    }

    #[test]
    fn request_ids_are_unique_and_opaque() {
        let first = forage_request_id(17);
        let second = forage_request_id(17);
        assert_eq!(first.len(), 64);
        assert_ne!(first, second);
    }

    #[test]
    fn result_redirects_reopen_authoritative_receipts_in_the_integrated_dialog() {
        let request = "a".repeat(64);
        assert!(valid_forage_request_id(&request));
        assert!(!valid_forage_request_id("../client-feedback"));
        assert_eq!(
            forage_result_href("/camp", &request),
            format!("/camp?forage=true&forage_receipt={request}")
        );
        assert_eq!(
            forage_result_href(
                "/locations/settlement/lubeck/party/17?building=public-square",
                &request
            ),
            format!(
                "/locations/settlement/lubeck/party/17?building=public-square&forage=true&forage_receipt={request}"
            )
        );
    }

    #[test]
    fn failures_reopen_the_integrated_dialog_with_allowlisted_feedback() {
        assert_eq!(forage_error_code("invalid target item"), "targets");
        assert_eq!(forage_error_code("Terrain data is unavailable"), "location");
        assert_eq!(forage_error_code("database exploded"), "unavailable");
        assert_eq!(forage_error_message("../raw-error"), None);
        assert_eq!(
            forage_error_href("/camp", "../raw-error"),
            "/camp?forage=true&forage_error=unavailable"
        );
        assert_eq!(
            forage_error_href(
                "/locations/settlement/lubeck/party/17?building=public-square",
                "targets"
            ),
            "/locations/settlement/lubeck/party/17?building=public-square&forage=true&forage_error=targets"
        );
    }

    #[test]
    fn legacy_forage_routes_reopen_the_integrated_location_dialog() {
        let mut character = Character {
            id: 17,
            name: "Ada".into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("lubeck".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 24,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        assert_eq!(
            integrated_forage_href(&character, "/"),
            "/locations/settlement/lubeck/party/17?forage=true"
        );
        assert_eq!(
            integrated_forage_href(&character, "/camp"),
            "/camp?forage=true"
        );
        character.current_settlement_id = None;
        character.current_case_site_id = Some("site".into());
        assert_eq!(
            integrated_forage_href(&character, "/"),
            "/locations/case-site/site/party/17?forage=true"
        );
    }
}
