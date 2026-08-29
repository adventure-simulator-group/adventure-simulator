use adventuresim_world_schema::coordinates::Wgs84CoordinateMicrodegrees;
use axum::{
    Router,
    extract::{DefaultBodyLimit, RawForm, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use maud::{Markup, html};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    AppState, local_return_url, persisted_route_position, wgs84_e7,
    wgs84_latitude_longitude_degrees,
};
use crate::{
    session::Session,
    spacetimedb::{
        BackendCaseSitePin, BackendForageAttemptState, BackendForageReceipt,
        BackendOrganizationMembership, CharacterTime, CharacterView, OrganizationMembershipStatus,
        OrganizationPresentation, PartyJourney, PartyJourneyRouteView, PartyView, SettlementView,
        sql_string_literal,
    },
};

static NEXT_FORAGE_REQUEST: AtomicU64 = AtomicU64::new(1);
const FORAGE_FORM_MAX_BYTES: usize = 1_024;
const FORAGE_FORM_MAX_PAIRS: usize = 8;
const FORAGE_FORM_MAX_SOURCES: usize = 5;
const FORAGE_FORM_MAX_SOURCE_LEN: usize = 32;
const FORAGE_FORM_MAX_RETURN_TO_LEN: usize = 512;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/forage",
        post(perform).layer(DefaultBodyLimit::max(FORAGE_FORM_MAX_BYTES)),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct ForageForm {
    source: Vec<String>,
    hours: u8,
    return_to: String,
}

fn parse_forage_form(body: &[u8]) -> Result<ForageForm, ()> {
    if body.len() > FORAGE_FORM_MAX_BYTES {
        return Err(());
    }
    let mut source = Vec::new();
    let mut hours = None;
    let mut return_to = None;
    for (pair_index, (key, value)) in form_urlencoded::parse(body).enumerate() {
        if pair_index >= FORAGE_FORM_MAX_PAIRS {
            return Err(());
        }
        match key.as_ref() {
            // Repeated checkbox names are the canonical browser encoding for
            // zero, one, or many selected sources. Preserve duplicates so the
            // authoritative reducer can reject that malformed contract.
            "source" => {
                if source.len() >= FORAGE_FORM_MAX_SOURCES
                    || value.len() > FORAGE_FORM_MAX_SOURCE_LEN
                {
                    return Err(());
                }
                source.push(value.into_owned());
            }
            "hours" => {
                if hours.is_some() {
                    return Err(());
                }
                hours = Some(value.parse().map_err(|_| ())?);
            }
            "return_to" => {
                if return_to.is_some() || value.len() > FORAGE_FORM_MAX_RETURN_TO_LEN {
                    return Err(());
                }
                return_to = Some(value.into_owned());
            }
            _ => {}
        }
    }
    Ok(ForageForm {
        source,
        hours: hours.ok_or(())?,
        return_to: return_to.ok_or(())?,
    })
}

struct Vicinity {
    kind: String,
    id: String,
    latitude: f64,
    longitude: f64,
    settlement: bool,
}

#[derive(Serialize)]
struct WireForageEnvironmentAttestation<'a> {
    package_digest: &'a str,
    // SpacetimeDB codegen splits the numeric suffix in these wire names.
    latitude_e_7: i32,
    longitude_e_7: i32,
    context_kind: &'a str,
    context_id: &'a str,
    plains: u16,
    forest: u16,
    hills: u16,
    wetlands: u16,
    river_or_wet_ground: bool,
    sea_or_coast: bool,
    cultivated: bool,
}

fn source_privilege(
    source: adventuresim_core::foraging::ForageSource,
) -> Option<adventuresim_core::organization::Privilege> {
    use adventuresim_core::foraging::ForageSource;
    use adventuresim_core::organization::Privilege;
    Some(match source {
        ForageSource::HighGame => Privilege::ForageHighGame,
        ForageSource::LowGame => Privilege::ForageLowGame,
        ForageSource::Fish => Privilege::ForageFish,
        ForageSource::Plants => Privilege::ForagePlants,
        ForageSource::HarmfulBeasts => return None,
    })
}

async fn advisory_privileges(
    state: &AppState,
    character_id: u64,
) -> BTreeSet<adventuresim_core::organization::Privilege> {
    let presentation = state
        .db
        .query_one_sats::<OrganizationPresentation>(
            &crate::spacetimedb::organization_presentation_by_character_id(character_id),
        )
        .await
        .ok()
        .flatten();
    let memberships = state
        .db
        .query_sats::<BackendOrganizationMembership>(&format!(
            "SELECT * FROM backend_organization_memberships WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let minute = state
        .db
        .query_one_sats::<CharacterTime>(&crate::spacetimedb::character_time_by_character_id(
            character_id,
        ))
        .await
        .ok()
        .flatten()
        .map(|row| row.minutes);
    advisory_privileges_for(
        presentation
            .as_ref()
            .map(|presentation| presentation.organization_id.as_str()),
        &memberships,
        minute,
    )
}

fn advisory_privileges_for(
    presented_organization_id: Option<&str>,
    memberships: &[BackendOrganizationMembership],
    minute: Option<u64>,
) -> BTreeSet<adventuresim_core::organization::Privilege> {
    let Some((presented_organization_id, minute)) = presented_organization_id.zip(minute) else {
        return BTreeSet::new();
    };
    let Some(definition) = adventuresim_core::organization::organization(presented_organization_id)
    else {
        return BTreeSet::new();
    };
    let Some(membership) = memberships.iter().find(|membership| {
        membership.organization_id == presented_organization_id
            && membership.status == OrganizationMembershipStatus::Active
            && minute <= membership.dues_paid_through_minute
    }) else {
        return BTreeSet::new();
    };
    [
        adventuresim_core::organization::Privilege::ForageHighGame,
        adventuresim_core::organization::Privilege::ForageLowGame,
        adventuresim_core::organization::Privilege::ForageFish,
        adventuresim_core::organization::Privilege::ForagePlants,
    ]
    .into_iter()
    .filter(|privilege| definition.has_privilege_at_role(&membership.role_id, *privilege))
    .collect()
}

fn source_rows(
    environment: adventuresim_core::foraging::ForageEnvironment,
    privileges: &BTreeSet<adventuresim_core::organization::Privilege>,
) -> Markup {
    html! {
        @for source in adventuresim_core::foraging::ForageSource::ALL {
            @let available = adventuresim_core::foraging::source_available(source, environment);
            @let licensed = source_privilege(source)
                .is_none_or(|privilege| privileges.contains(&privilege));
            @let tooltip = (!licensed).then_some(
                "Your presented profession does not grant the hunting license required for this source. Selecting it is poaching."
            );
            label class=(format!(
                    "forage-source-row{}{}",
                    if !licensed { " forage-source-unlicensed" } else { "" },
                    if !available { " forage-source-unavailable" } else { "" }
                ))
                tabindex=[(!licensed && available).then_some("0")]
                data-strategic-tooltip=[tooltip] {
                input type="checkbox" name="source" value=(source.id()) disabled[!available];
                span class="forage-source-copy" {
                    strong { (source.name()) }
                    span { (source.description()) }
                }
                @if !available {
                    span class="forage-source-status" { "Unavailable here" }
                } @else if !licensed {
                    span class="forage-source-status" { "Unlicensed" }
                } @else if !source.requires_license() {
                    span class="forage-source-status" { "No license required" }
                } @else {
                    span class="forage-source-status" { "Licensed" }
                }
            }
        }
    }
}

async fn vicinity(state: &AppState, character: &CharacterView) -> Result<Vicinity, String> {
    if let Some(id) = character.current_settlement_id.as_deref() {
        let row = state
            .db
            .query_one_sats_into::<adventuresim_stdb_client::Settlement, SettlementView>(
                &crate::spacetimedb::settlement_by_id(id),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Current settlement not found")?;
        let coordinate = Wgs84CoordinateMicrodegrees::from_longitude_latitude_degrees(
            row.longitude,
            row.latitude,
        )
        .ok_or("persisted settlement coordinate is outside WGS84 bounds")?;
        let (longitude, latitude) = coordinate.longitude_latitude_degrees();
        return Ok(Vicinity {
            kind: "settlement".into(),
            id: id.into(),
            latitude,
            longitude,
            settlement: true,
        });
    }
    if let Some(id) = character.current_case_site_id.as_deref() {
        let row = state
            .db
            .query_one_sats::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {} AND case_site_id = {}",
                character.id,
                sql_string_literal(id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Current case site is not exact")?;
        let (latitude, longitude) =
            wgs84_latitude_longitude_degrees(row.latitude_e_7, row.longitude_e_7)
                .map_err(str::to_owned)?;
        return Ok(Vicinity {
            kind: "case_site".into(),
            id: id.into(),
            latitude,
            longitude,
            settlement: false,
        });
    }
    let party_id = character
        .party_id
        .as_deref()
        .ok_or("Character has no stationary vicinity")?;
    let party = state
        .db
        .query_one_sats_into::<adventuresim_stdb_client::Party, PartyView>(
            &crate::spacetimedb::party_by_id(party_id),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Party not found")?;
    if party.camp_destination.is_none() {
        return Err("Foraging is unavailable while moving or without a known location".into());
    }
    let journey = state
        .db
        .query_one_sats::<PartyJourney>(&crate::spacetimedb::party_journey_by_party_id(party_id))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Camp journey not found")?;
    let route = state
        .db
        .query_one_sats_into::<adventuresim_stdb_client::PartyJourneyRoute, PartyJourneyRouteView>(
            &crate::spacetimedb::party_journey_route_by_party_id(party_id),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Camp terrain route not found")?;
    let (latitude, longitude) =
        persisted_route_position(&route, journey.completed_movement_minutes)
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
        "targets" => Some("Choose valid forage sources and try again."),
        "duration" => Some("Choose a valid search duration and try again."),
        "unavailable" => Some("The search could not be completed."),
        _ => None,
    }
}

fn forage_error_code(_error: &str) -> &'static str {
    "unavailable"
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
                p { "The illegal search was noticed. Local Infamy increased." }
            }
        }
    }
}

async fn character(state: &AppState, id: u64) -> Result<CharacterView, String> {
    state
        .db
        .query_one_sats_into::<adventuresim_stdb_client::Character, CharacterView>(
            &crate::spacetimedb::character_by_id(id),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Character not found".into())
}

async fn environment(
    state: &AppState,
    character: &CharacterView,
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
        wetlands: weights.wetlands,
    };
    let environment = adventuresim_core::foraging::ForageEnvironment {
        terrain: mixture,
        river_or_wet_ground: river_or_wet,
        sea_or_coast: coastal,
        cultivated: cell.cultivated,
        settlement: location.settlement,
        license_violation: false,
    };
    let (latitude_e7, longitude_e7) =
        wgs84_e7(location.latitude, location.longitude).map_err(str::to_owned)?;
    let attestation = serde_json::to_value(WireForageEnvironmentAttestation {
        package_digest: terrain.digest(),
        latitude_e_7: latitude_e7,
        longitude_e_7: longitude_e7,
        context_kind: &location.kind,
        context_id: &location.id,
        plains: mixture.plains,
        forest: mixture.forest,
        hills: mixture.hills,
        wetlands: mixture.wetlands,
        river_or_wet_ground: environment.river_or_wet_ground,
        sea_or_coast: environment.sea_or_coast,
        cultivated: environment.cultivated,
    })
    .map_err(|error| error.to_string())?;
    Ok((environment, attestation))
}

pub(crate) async fn activity_dialog(
    state: &AppState,
    character: &CharacterView,
    return_to: &str,
    receipt_id: Option<&str>,
    error_code: Option<&str>,
) -> Markup {
    let receipt = if let Some(request_id) = receipt_id.filter(|id| valid_forage_request_id(id)) {
        state
            .db
            .query_one_sats::<BackendForageReceipt>(&forage_receipt_query(character.id, request_id))
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
    let privileges = advisory_privileges(state, character.id).await;
    let illegal =
        environment.is_some_and(|environment| environment.settlement || environment.cultivated);
    let available_source_rows = environment
        .as_ref()
        .map(|environment| source_rows(*environment, &privileges));
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
                p id="forage-description" { "Search only the character's immediate vicinity. Selected sources share the search time." }
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
                        p role="alert" class="badge badge-warning" { "Foraging here is illegal. One Stealth check is made when the search completes; failure adds local Infamy." }
                    }
                    form method="post" action="/forage" {
                        input type="hidden" name="return_to" value=(return_to);
                        p id="forage-source-help" class="text-muted small-copy" { "Choose food sources. Selected categories share one search-time budget." }
                        fieldset id="forage-targets" aria-describedby="forage-source-help" {
                            legend { "Food sources" }
                            @if let Some(rows) = available_source_rows {
                                (rows)
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

async fn perform(
    State(state): State<AppState>,
    session: Session,
    RawForm(body): RawForm,
) -> Response {
    let Ok(form) = parse_forage_form(&body) else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    let return_to = local_return_url(&form.return_to).unwrap_or("/");
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let result = async {
        let character = character(&state, character_id).await?;
        let (environment, attestation) = environment(&state, &character).await?;
        let minutes = u64::from(form.hours) * 60;
        let request_id = forage_request_id(character_id);
        let attempt_generation = state
            .db
            .query_one_sats::<BackendForageAttemptState>(
                &crate::spacetimedb::forage_attempt_state_by_character_id(character_id),
            )
            .await
            .map_err(|error| error.to_string())?
            .map_or(0, |row| row.next_generation);
        // The browser submits only selected categories and duration. This
        // session-scoped endpoint hydrates the opaque request, generation, and
        // private terrain attestation before entering the plan gateway.
        state
            .db
            .call(
                "forage_current_vicinity",
                &[
                    json!(character_id),
                    json!(&request_id),
                    json!(&form.source),
                    json!(minutes),
                    json!(attempt_generation),
                    attestation,
                ],
            )
            .await
            .map_err(|error| error.to_string())?;
        let attempt = state
            .db
            .query_one_sats::<BackendForageReceipt>(&forage_receipt_query(
                character_id,
                &request_id,
            ))
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

    fn mixed_environment() -> adventuresim_core::foraging::ForageEnvironment {
        adventuresim_core::foraging::ForageEnvironment {
            terrain: adventuresim_core::foraging::LocalTerrainMixture {
                plains: 400,
                forest: 400,
                hills: 200,
                wetlands: 0,
            },
            river_or_wet_ground: true,
            sea_or_coast: false,
            cultivated: false,
            settlement: false,
            license_violation: false,
        }
    }

    #[test]
    fn browser_checkbox_form_accepts_one_or_many_sources_without_javascript() {
        assert_eq!(
            parse_forage_form(b"return_to=%2Fcamp&source=plants&hours=1"),
            Ok(ForageForm {
                source: vec!["plants".into()],
                hours: 1,
                return_to: "/camp".into(),
            })
        );
        assert_eq!(
            parse_forage_form(b"return_to=%2Fcamp&source=high_game&source=plants&hours=24"),
            Ok(ForageForm {
                source: vec!["high_game".into(), "plants".into()],
                hours: 24,
                return_to: "/camp".into(),
            })
        );
    }

    #[test]
    fn browser_checkbox_form_preserves_none_and_duplicates_for_authoritative_validation() {
        assert_eq!(
            parse_forage_form(b"return_to=%2Fcamp&hours=1"),
            Ok(ForageForm {
                source: Vec::new(),
                hours: 1,
                return_to: "/camp".into(),
            })
        );
        assert_eq!(
            parse_forage_form(b"return_to=%2Fcamp&source=plants&source=plants&hours=1")
                .unwrap()
                .source,
            ["plants", "plants"]
        );
    }

    #[test]
    fn browser_checkbox_form_rejects_ambiguous_scalar_fields() {
        assert!(
            parse_forage_form(b"return_to=%2Fcamp&return_to=%2Fother&source=plants&hours=1")
                .is_err()
        );
        assert!(parse_forage_form(b"return_to=%2Fcamp&source=plants&hours=1&hours=2").is_err());
    }

    #[test]
    fn source_markup_is_ordered_accessible_and_keeps_poaching_enabled() {
        let markup = source_rows(mixed_environment(), &BTreeSet::new()).into_string();
        let positions = ["High Game", "Low Game", "Fish", "Harmful Beasts", "Plants"]
            .map(|label| markup.find(label).unwrap());
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(markup.matches("type=\"checkbox\"").count(), 5);
        assert_eq!(markup.matches("Unlicensed").count(), 4);
        assert!(markup.contains("No license required"));
        assert!(markup.contains("forage-source-unlicensed"));
        assert!(markup.contains("data-strategic-tooltip="));
        assert!(!markup.contains("title="));
        assert!(!markup.contains("value=\"high_game\" disabled"));
    }

    #[test]
    fn unavailable_source_remains_visible_and_disabled() {
        let mut environment = mixed_environment();
        environment.river_or_wet_ground = false;
        let markup = source_rows(environment, &BTreeSet::new()).into_string();
        assert!(markup.contains("value=\"fish\" disabled"));
        assert!(markup.contains("Unavailable here"));
    }

    fn ranger_membership(
        role_id: &str,
        status: OrganizationMembershipStatus,
        paid_through: u64,
    ) -> BackendOrganizationMembership {
        BackendOrganizationMembership {
            id: 1,
            character_id: 7,
            organization_id: "lodge_hart_king".into(),
            role_id: role_id.into(),
            joined_minute: 0,
            dues_paid_through_minute: paid_through,
            status,
            apprenticeship_minutes_accrued: 0,
            practice_minutes_accrued: 0,
        }
    }

    #[test]
    fn advisory_licenses_require_matching_current_presentation_and_role() {
        use adventuresim_core::organization::Privilege;
        let warden = ranger_membership("warden", OrganizationMembershipStatus::Active, 100);
        let common = advisory_privileges_for(
            Some("lodge_hart_king"),
            std::slice::from_ref(&warden),
            Some(100),
        );
        assert!(common.contains(&Privilege::ForageLowGame));
        assert!(common.contains(&Privilege::ForageFish));
        assert!(common.contains(&Privilege::ForagePlants));
        assert!(!common.contains(&Privilege::ForageHighGame));

        let master = ranger_membership("master", OrganizationMembershipStatus::Active, 100);
        assert!(
            advisory_privileges_for(Some("lodge_hart_king"), &[master], Some(100))
                .contains(&Privilege::ForageHighGame)
        );
        assert!(advisory_privileges_for(None, std::slice::from_ref(&warden), Some(100)).is_empty());
        assert!(
            advisory_privileges_for(
                Some("hunt_pale_lantern"),
                std::slice::from_ref(&warden),
                Some(100)
            )
            .is_empty()
        );
        let lapsed = ranger_membership("master", OrganizationMembershipStatus::Active, 99);
        assert!(advisory_privileges_for(Some("lodge_hart_king"), &[lapsed], Some(100)).is_empty());
        let suspended = ranger_membership("master", OrganizationMembershipStatus::Suspended, 100);
        assert!(
            advisory_privileges_for(Some("lodge_hart_king"), &[suspended], Some(100)).is_empty()
        );
    }

    #[test]
    fn browser_checkbox_form_is_explicitly_bounded_before_authentication() {
        assert!(parse_forage_form(&vec![b'x'; FORAGE_FORM_MAX_BYTES + 1]).is_err());

        let too_many_pairs = format!(
            "return_to=%2Fcamp&hours=1{}",
            "&ignored=x".repeat(FORAGE_FORM_MAX_PAIRS - 1)
        );
        assert!(parse_forage_form(too_many_pairs.as_bytes()).is_err());

        let too_many_sources = format!(
            "return_to=%2Fcamp&hours=1{}",
            "&source=plants".repeat(FORAGE_FORM_MAX_SOURCES + 1)
        );
        assert!(parse_forage_form(too_many_sources.as_bytes()).is_err());

        let long_source = "x".repeat(FORAGE_FORM_MAX_SOURCE_LEN + 1);
        assert!(
            parse_forage_form(format!("return_to=%2Fcamp&hours=1&source={long_source}").as_bytes())
                .is_err()
        );

        let long_return = "x".repeat(FORAGE_FORM_MAX_RETURN_TO_LEN + 1);
        assert!(
            parse_forage_form(format!("return_to=%2F{long_return}&hours=1").as_bytes()).is_err()
        );
    }

    #[test]
    fn forage_attestation_uses_generated_spacetime_wire_field_names() {
        let encoded = serde_json::to_value(WireForageEnvironmentAttestation {
            package_digest: "digest",
            latitude_e_7: 517_500_000,
            longitude_e_7: 97_500_000,
            context_kind: "settlement",
            context_id: "dev-scenario-foraging",
            plains: 0,
            forest: 1_000,
            hills: 0,
            wetlands: 0,
            river_or_wet_ground: false,
            sea_or_coast: false,
            cultivated: false,
        })
        .unwrap();
        assert_eq!(
            encoded,
            json!({
                "package_digest": "digest",
                "latitude_e_7": 517_500_000,
                "longitude_e_7": 97_500_000,
                "context_kind": "settlement",
                "context_id": "dev-scenario-foraging",
                "plains": 0,
                "forest": 1_000,
                "hills": 0,
                "wetlands": 0,
                "river_or_wet_ground": false,
                "sea_or_coast": false,
                "cultivated": false,
            })
        );
        let generated = include_str!(
            "../../../adventuresim-stdb-client/src/forage_environment_attestation_type.rs"
        );
        assert!(generated.contains("pub latitude_e_7: i32"));
        assert!(generated.contains("pub longitude_e_7: i32"));
    }

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
        assert_eq!(forage_error_code("invalid target item"), "unavailable");
        assert_eq!(
            forage_error_code("Terrain data is unavailable"),
            "unavailable"
        );
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
}
