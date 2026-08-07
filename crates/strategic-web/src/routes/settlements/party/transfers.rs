#[derive(Deserialize)]
pub(super) struct PartyTransferForm {
    from_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
}

pub(super) async fn discard_inventory_items(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<DiscardInventoryForm>,
) -> Redirect {
    if session.character_id_u64() == Some(character_id) {
        if let Ok(entries) = form.entries() {
            let (item_ids, quantities): (Vec<_>, Vec<_>) = entries
                .into_iter()
                .map(|entry| (entry.id, entry.quantity))
                .unzip();
            if let Err(error) = state
                .db
                .call(
                    "discard_inventory_items",
                    &[json!(character_id), json!(item_ids), json!(quantities)],
                )
                .await
            {
                tracing::warn!("Inventory discard failed: {error}");
            }
        }
    }
    Redirect::to(&building.append_to(&state, &kind, &id, format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    )).await)
}

pub(super) async fn finalize_party_offer(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PartyOfferForm>,
) -> Redirect {
    if let Some((active, _)) = get_active_character(&state, session.character_id_u64()).await {
        if let Ok(entries) = form.entries() {
            let from_ids = entries.iter().map(|entry| entry.from).collect::<Vec<_>>();
            let to_ids = entries.iter().map(|entry| entry.to).collect::<Vec<_>>();
            let item_ids = entries
                .iter()
                .map(|entry| entry.inventory_id)
                .collect::<Vec<_>>();
            let quantities = entries
                .iter()
                .map(|entry| entry.quantity)
                .collect::<Vec<_>>();
            if from_ids
                .iter()
                .all(|id| *id == active.id || *id == character_id)
                && to_ids
                    .iter()
                    .all(|id| *id == active.id || *id == character_id)
            {
                let _ = state
                    .db
                    .call(
                        "finalize_party_offer",
                        &[
                            json!(from_ids),
                            json!(to_ids),
                            json!(item_ids),
                            json!(quantities),
                        ],
                    )
                    .await;
            }
        }
    }
    Redirect::to(&building.append_to(&state, &kind, &id, format!(
        "/locations/{kind}/{id}/party/{character_id}/inventory"
    )).await)
}

pub(super) async fn transfer_party_item(
    State(state): State<AppState>,
    Path((kind, id, recipient_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PartyTransferForm>,
) -> Redirect {
    let Some((active_character, _)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    if form.from_character_id != active_character.id && recipient_id != active_character.id {
        return Redirect::to(&building.append_to(&state, &kind, &id, format!("/locations/{kind}/{id}")).await);
    }
    let to_character_id = if form.from_character_id == active_character.id {
        recipient_id
    } else {
        active_character.id
    };
    if let Err(error) = state
        .db
        .call(
            "transfer_party_item",
            &[
                json!(form.from_character_id),
                json!(to_character_id),
                json!(form.inventory_item_id),
                json!(form.quantity),
            ],
        )
        .await
    {
        tracing::warn!("Party item transfer failed: {error}");
    }
    let comparison_character_id = if form.from_character_id == active_character.id {
        recipient_id
    } else {
        form.from_character_id
    };
    Redirect::to(&building.append_to(&state, &kind, &id, format!(
        "/locations/{kind}/{id}/party/{comparison_character_id}/inventory"
    )).await)
}

pub(super) async fn merchants(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::General).await
}

pub(super) async fn finalize_merchant_offer(
    State(state): State<AppState>,
    Path((id, service_id)): Path<(String, String)>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    let Some(location_id) = merchant_service_location(&service_id) else {
        return Redirect::to(&format!("/settlements/{id}/merchants"));
    };
    let fallback = format!("/settlements/{id}/{service_id}");
    let mut trade_completed = false;
    if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await {
        if let (Ok(buys), Ok(sells)) = (form.buys(), form.sells()) {
            let (items, quantities): (Vec<_>, Vec<_>) = buys
                .into_iter()
                .map(|entry| (entry.id, entry.quantity))
                .unzip();
            let (sell_ids, sell_quantities): (Vec<_>, Vec<_>) = sells
                .into_iter()
                .map(|entry| (entry.id, entry.quantity))
                .unzip();
            let provider_resident_character_id =
                merchant_provider_id(&state, &id, &service_id, location_id).await;
            if items.is_empty() && sell_ids.is_empty() {
                trade_completed = true;
            } else if let Some(provider_resident_character_id) = provider_resident_character_id {
                match state
                    .db
                    .call(
                        "finalize_storefront_trade",
                        &[
                            json!(character.id),
                            json!(&id),
                            json!(&service_id),
                            json!(provider_resident_character_id),
                            json!(items),
                            json!(quantities),
                            json!(sell_ids),
                            json!(sell_quantities),
                            json!(form.inventory_scope == "party"),
                        ],
                    )
                    .await
                {
                    Ok(()) => trade_completed = true,
                    Err(error) => {
                        tracing::warn!(%error, settlement_id = %id, "merchant offer was rejected");
                    }
                }
            }
        }
    }
    if trade_completed {
        redirect_to_local(&form.return_to, &fallback)
    } else {
        Redirect::to(&fallback)
    }
}
