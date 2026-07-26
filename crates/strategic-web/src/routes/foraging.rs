use axum::{
    Form, Router,
    extract::{Query, State},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use maud::{DOCTYPE, Markup, html};
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

fn page(
    return_to: &str,
    environment: Option<(adventuresim_core::foraging::ForageEnvironment, String)>,
    unavailable: Option<&str>,
    result: Option<&BackendForageReceipt>,
) -> String {
    let resources = environment
        .as_ref()
        .map(|(environment, _)| {
            adventuresim_core::foraging::FORAGE_RESOURCES
                .iter()
                .filter(|resource| adventuresim_core::foraging::available(resource, *environment))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let illegal = environment
        .as_ref()
        .is_some_and(|(environment, _)| environment.settlement || environment.cultivated);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Forage nearby" }
                link rel="stylesheet" href="/static/style.css";
            }
            body class="entry-page activity-modal-open" {
                main class="modal-backdrop" {
                    section class="modal-content forage-dialog" role="dialog" aria-modal="true"
                        aria-labelledby="forage-title" aria-describedby="forage-description" {
                        h1 id="forage-title" { "Forage nearby" }
                        p id="forage-description" {
                            "Search only the character's immediate vicinity. Multiple targets share the selected time."
                        }
                        @if let Some(reason) = unavailable {
                            p role="alert" class="badge badge-danger" { (reason) }
                        } @else if let Some(result) = result {
                            div role="status" aria-live="polite" {
                                @if result.interrupted {
                                    p { "The search was interrupted after " (result.elapsed_minutes / 60) " hour(s). Nothing was gathered." }
                                } @else if result.yielded_item_ids.is_empty() {
                                    p { "The search found nothing." }
                                } @else {
                                    h2 { "Gathered" }
                                    ul {
                                        @for (item, quantity) in result.yielded_item_ids.iter().zip(&result.yielded_quantities) {
                                            li { (quantity) " × " (adventuresim_core::foraging::resource(item).map_or(item.as_str(), |resource| resource.name)) }
                                        }
                                    }
                                }
                                @if result.legal_outcome == "unnoticed" {
                                    p { "The illegal search went unnoticed." }
                                } @else if result.legal_outcome == "noticed" {
                                    p { "The illegal search was noticed. Virtue -1.0." }
                                }
                            }
                        } @else {
                            @if illegal {
                                p role="alert" class="badge badge-warning" {
                                    "Foraging here is illegal. One Stealth check is made when the search completes; failure costs 1 Virtue."
                                }
                            }
                            form method="post" action="/forage" {
                                input type="hidden" name="return_to" value=(return_to);
                                p id="forage-target-limit" class="text-muted small-copy" {
                                    "Choose at most eight targets. Every selected target shares the same search time."
                                }
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
                                    a class="btn btn-secondary" href=(return_to) { "Cancel" }
                                }
                                script {
                                    "document.getElementById('forage-targets').addEventListener('change',function(){const c=[...this.querySelectorAll('input[type=checkbox]')],n=c.filter(x=>x.checked).length;c.forEach(x=>x.disabled=!x.checked&&n>=8);});"
                                }
                            }
                        }
                        @if result.is_some() || unavailable.is_some() {
                            a class="btn btn-secondary" href=(return_to) { "Return" }
                        }
                    }
                }
            }
        }
    }
    .into_string()
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
) -> Markup {
    let outcome = environment(state, character).await;
    let (environment, unavailable) = match outcome {
        Ok((environment, _)) => (Some(environment), None),
        Err(error) => (None, Some(error)),
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
        div class="character-action-overlay" data-character-action-dialog data-initial-focus="#forage-targets" {
            a class="character-action-backdrop" href=(return_to) aria-label="Close foraging dialog" {}
            section class="character-action-dialog forage-dialog" role="dialog" aria-modal="true"
                aria-labelledby="forage-title" aria-describedby="forage-description" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="forage-title" { "Forage nearby" }
                    a class="character-action-dialog-close" href=(return_to) aria-label="Close foraging dialog" { "×" }
                }
                p id="forage-description" { "Search only the character's immediate vicinity. Multiple targets share the selected time." }
                @if let Some(reason) = unavailable {
                    p role="alert" class="badge badge-danger" { (reason) }
                } @else {
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
        return Html(page(
            return_to,
            None,
            Some("Choose a character first."),
            None,
        ))
        .into_response();
    };
    let result = match character(&state, character_id).await {
        Ok(character) if !character.alive => Err("Dead characters cannot forage.".into()),
        Ok(character) => environment(&state, &character)
            .await
            .map(|value| (value.0, "ready".into())),
        Err(error) => Err(error),
    };
    match result {
        Ok(environment) => Html(page(return_to, Some(environment), None, None)).into_response(),
        Err(error) => Html(page(return_to, None, Some(&error), None)).into_response(),
    }
}

async fn perform(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<ForageForm>,
) -> Response {
    let return_to = local_return_url(&form.return_to).unwrap_or("/");
    let Some(character_id) = session.character_id_u64() else {
        return Html(page(
            return_to,
            None,
            Some("Choose a character first."),
            None,
        ))
        .into_response();
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
        Ok::<_, String>((environment, attempt))
    }
    .await;
    match result {
        Ok((environment, attempt)) => Html(page(
            return_to,
            Some((environment, "ready".into())),
            None,
            Some(&attempt),
        ))
        .into_response(),
        Err(error) => Html(page(return_to, None, Some(&error), None)).into_response(),
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
}
