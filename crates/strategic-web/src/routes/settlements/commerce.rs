pub(super) fn merchant_service_location(service_id: &str) -> Option<&'static str> {
    match service_id {
        "merchants" => Some("market"),
        "weapons" => Some("forge"),
        "armor" => Some("armoury"),
        "clothing" => Some("tailor"),
        "inn" => Some("inn"),
        "books" => Some("bookstore"),
        _ => None,
    }
}

pub(super) async fn merchant_provider_id(
    state: &AppState,
    settlement_id: &str,
    service_id: &str,
    location_id: &str,
) -> Option<String> {
    let settlement_literal = sql_string_literal(settlement_id);
    let providers_sql = format!(
        "SELECT * FROM backend_settlement_residents WHERE home_settlement_id = {settlement_literal}"
    );
    let presences_sql = format!(
        "SELECT * FROM settlement_resident_presence WHERE settlement_id = {settlement_literal}"
    );
    let (providers, presences) = tokio::join!(
        state
            .db
            .query_sats::<crate::spacetimedb::BackendSettlementResident>(&providers_sql),
        state
            .db
            .query_sats::<crate::spacetimedb::SettlementResidentPresence>(&presences_sql),
    );
    let providers = providers.ok()?;
    let presences = presences.ok()?;
    let mut matches = providers.into_iter().filter_map(|provider| {
        (provider.home_settlement_id == settlement_id && provider.service_id == service_id)
            .then_some(provider)
            .and_then(|provider| {
                presences
                    .iter()
                    .any(|presence| {
                        presence.character_id == provider.character_id
                            && presence.settlement_id == settlement_id
                            && presence.location_id == location_id
                            && presence.is_default
                    })
                    .then_some(provider.character_id)
            })
    });
    let provider = matches.next()?;
    matches.next().is_none().then(|| provider.to_string())
}

pub(super) async fn provisioning_storefront_path(
    state: &AppState,
    settlement: &SettlementView,
) -> Option<String> {
    use adventuresim_core::settlement_economy::{Storefront, storefront_available};

    for (storefront, service_id, location_id) in [
        (Storefront::General, "merchants", "market"),
        (Storefront::Inn, "inn", "inn"),
    ] {
        if storefront_available(&settlement.economy, storefront)
            && merchant_provider_id(state, &settlement.id, service_id, location_id)
                .await
                .is_some()
        {
            return Some(format!("/settlements/{}/{service_id}", settlement.id));
        }
    }
    None
}

pub(super) async fn rest_at_settlement_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<RestForm>,
) -> Response {
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters").into_response();
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            "The party is not at this settlement",
        )
            .into_response();
    }
    let requested_minutes = match travel_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    let shelter = match field_shelter_argument(&form) {
        Ok(shelter) => shelter,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match state
        .db
        .call(
            "rest_at_camp",
            &[json!(character.id), json!(requested_minutes), shelter],
        )
        .await
    {
        Ok(()) => Redirect::to(&format!("/locations/settlement/{id}/map")).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn inn(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Inn).await
}
