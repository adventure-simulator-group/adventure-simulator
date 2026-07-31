#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct TrainingScheduleForm {
    reading_minutes: u16,
    combat_training_minutes: u16,
    carousing_minutes: u16,
    socializing_minutes: u16,
    apprenticeship_minutes: u16,
    apprenticeship_organization_id: Option<String>,
    profession_practice_minutes: u16,
    practice_organization_id: Option<String>,
    labor_minutes: u16,
    prayer_minutes: u16,
    thievery_minutes: u16,
    raiding_minutes: u16,
}

#[cfg(test)]
mod training_schedule_form_tests {
    use super::TrainingScheduleForm;
    use serde_json::json;

    #[test]
    fn schedule_form_contains_only_activity_allocations() {
        let form: TrainingScheduleForm = serde_json::from_value(json!({
            "combat_training_minutes": 90, "labor_minutes": 15,
            "prayer_minutes": 30, "thievery_minutes": 0, "raiding_minutes": 0
        }))
        .unwrap();
        assert_eq!(form.combat_training_minutes, 90);
        assert_eq!(form.labor_minutes, 15);
        assert_eq!(form.prayer_minutes, 30);
    }

    #[test]
    fn immediate_route_checks_resolved_location_before_calling_reducer() {
        let source = include_str!("training_activity.rs");
        let handler = source
            .split_once("pub(super) async fn perform_immediate_activity(\n")
            .map(|(_, tail)| tail)
            .and_then(|tail| tail.split("pub(super) async fn party_member").next())
            .expect("immediate activity handler");
        let resolve = handler.find("resolve_location").unwrap();
        let co_location = handler.find("character_is_at_location").unwrap();
        let reducer_call = handler.find("\"perform_immediate_activity\"").unwrap();
        assert!(resolve < co_location && co_location < reducer_call);
    }
}

pub(super) async fn update_training_schedule(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<TrainingScheduleForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Select this character before editing their schedule",
        )
            .into_response();
    }
    let downtime = ScheduleAllocation {
        reading_minutes: form.reading_minutes,
        combat_training_minutes: form.combat_training_minutes,
        carousing_minutes: form.carousing_minutes,
        socializing_minutes: form.socializing_minutes,
        apprenticeship_minutes: form.apprenticeship_minutes,
        apprenticeship_organization_id: form.apprenticeship_organization_id,
        profession_practice_minutes: form.profession_practice_minutes,
        practice_organization_id: form.practice_organization_id,
        labor_minutes: form.labor_minutes,
        prayer_minutes: form.prayer_minutes,
        thievery_minutes: form.thievery_minutes,
        raiding_minutes: form.raiding_minutes,
    };
    match state
        .db
        .call(
            "update_training_schedule",
            &[
                json!(character_id),
                schedule_allocation_reducer_arg(&downtime),
                schedule_allocation_reducer_arg(&ScheduleAllocation::default()),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to(
            &building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")),
        )
        .into_response(),
        Err(error) => {
            tracing::warn!(%error, character_id, "failed to update training schedule");
            (StatusCode::BAD_REQUEST, error.to_string()).into_response()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct ImmediateActivityForm {
    activity: String,
    requested_minutes: u64,
    #[serde(default)]
    service_id: Option<String>,
}

pub(super) fn immediate_activity_arg(activity: &str) -> Option<serde_json::Value> {
    let tag = match activity {
        "reading" => "reading",
        "prayer" => "prayer",
        "combat_training" => "combatTraining",
        "carousing" => "carousing",
        "apprenticeship" => "apprenticeship",
        "profession_practice" => "professionPractice",
        "labor" => "labor",
        "thievery" => "thievery",
        "raiding" => "raiding",
        _ => return None,
    };
    Some(json!({ (tag): {} }))
}

pub(super) async fn perform_immediate_activity(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<ImmediateActivityForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Select this character before performing an activity",
        )
            .into_response();
    }
    let location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => {
            return (StatusCode::NOT_FOUND, "Location not found").into_response();
        }
        LocationLookup::Unavailable => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Strategic location data is unavailable",
            )
                .into_response();
        }
    };
    let Some((character, _)) = get_active_character(&state, Some(character_id)).await else {
        return (StatusCode::FORBIDDEN, "Select this character first").into_response();
    };
    if !character_is_at_location(&character, &location) {
        return (
            StatusCode::CONFLICT,
            "Character is no longer at this location",
        )
            .into_response();
    }
    let Some(activity) = immediate_activity_arg(&form.activity) else {
        return (StatusCode::BAD_REQUEST, "Unknown activity").into_response();
    };
    if form.requested_minutes < 60
        || form.requested_minutes > 1_440
        || form.requested_minutes % 60 != 0
    {
        return (StatusCode::BAD_REQUEST, "Choose one to 24 whole hours").into_response();
    }
    let service_id = form.service_id.filter(|value| !value.is_empty());
    match state
        .db
        .call(
            "perform_immediate_activity",
            &[
                json!(character_id),
                activity,
                json!(form.requested_minutes),
                json!(service_id),
            ],
        )
        .await
    {
        Ok(()) => {
            if let Some((character, _)) = get_active_character(&state, Some(character_id)).await
                && let Some(case_site_id) = character.current_case_site_id
            {
                return Redirect::to(&format!("/locations/case-site/{case_site_id}"))
                    .into_response();
            }
            Redirect::to(
                &building.append_to(format!("/locations/{kind}/{id}/party/{character_id}")),
            )
            .into_response()
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn party_member(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".to_string()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".to_string());
        }
    };
    location.active_building = building.valid().map(str::to_owned);

    let Some((active_character, active_inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".to_string());
    };
    if !character_is_at_location(&active_character, &location) {
        return Html("<h1>Your party is not at this location</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;

    let selected = if character_id == active_character.id {
        active_character.clone()
    } else {
        let characters: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM backend_characters WHERE id = {character_id}"
            ))
            .await
            .unwrap_or_default();
        match characters.into_iter().next() {
            Some(character) => character,
            None => return Html("<h1>Party member not found</h1>".to_string()),
        }
    };
    if selected.current_settlement_id != active_character.current_settlement_id
        || selected.current_case_site_id != active_character.current_case_site_id
    {
        return Html("<h1>Character is not at this location</h1>".to_string());
    }
    if selected.id != active_character.id
        && (active_character.party_id.is_none() || selected.party_id != active_character.party_id)
    {
        return Html("<h1>Party member not found</h1>".to_string());
    }
    let selected_inventory: Vec<InventoryItem> = if character_id == active_character.id {
        active_inventory.clone()
    } else {
        state
            .db
            .query(&format!(
                "SELECT * FROM inventory_item WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
    };

    let selected_equip = character_equipment_graph(&state, character_id).await;
    let active_equip: Vec<CharacterEquipmentGraph> = if character_id == active_character.id {
        selected_equip.clone()
    } else {
        character_equipment_graph(&state, active_character.id).await
    };
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let selected_targets = personal_inventory_targets(&state, selected.id).await;
    let active_targets = personal_inventory_targets(&state, active_character.id).await;
    let encumbrance_rows =
        EncumbranceRows::query(&state, &[selected.id, active_character.id]).await;
    let selected_encumbrance = personal_encumbrance(
        selected.id,
        &selected_inventory,
        &items,
        &food_lots,
        &encumbrance_rows,
    );
    let active_encumbrance = personal_encumbrance(
        active_character.id,
        &active_inventory,
        &items,
        &food_lots,
        &encumbrance_rows,
    );

    if character_id == active_character.id {
        return Html(
            party_discard_page(
                &location,
                &active_character,
                &active_inventory,
                &items,
                &food_lots,
                &party_members,
                active_equip.first(),
                active_encumbrance,
            )
            .into_string(),
        );
    }

    Html(
        party_inventory_page(
            &location,
            &selected,
            &selected_inventory,
            &active_character,
            &active_inventory,
            &items,
            &food_lots,
            &party_members,
            selected_equip.first(),
            active_equip.first(),
            &selected_targets,
            &active_targets,
            selected_encumbrance,
            active_encumbrance,
        )
        .into_string(),
    )
}

pub(super) async fn party_pool_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
    let Some((character, inventory)) =
        get_active_character(&state, session.character_id_u64()).await
    else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    if !character_is_at_location(&character, &location) {
        return Html("<h1>Your party is not at this location</h1>".into());
    }
    let Some(party_id) = character.party_id.as_ref() else {
        return Html("<h1>Character has no party</h1>".into());
    };
    let pooled: Vec<PartyInventoryItem> = state
        .db
        .query(&format!(
            "SELECT * FROM party_inventory_item WHERE party_id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .unwrap_or_default();
    let stakes: Vec<PartyStake> = state
        .db
        .query(&format!(
            "SELECT * FROM party_stake WHERE party_id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .unwrap_or_default();
    let food_lots: Vec<FoodLot> = state
        .db
        .query("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let items: Vec<ItemDefinition> = state
        .db
        .query("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let equip = character_equipment_graph(&state, character.id).await;
    let members = get_active_party_members(&state, Some(&character)).await;
    let encumbrance = inventory_encumbrance_summaries(
        &state, &character, &inventory, &members, &pooled, &items, true,
    )
    .await;
    let stake = stakes
        .iter()
        .find(|stake| stake.character_id == character.id)
        .map_or(0, |stake| stake.value);
    let (personal_targets, party_targets, _) = inventory_trade_context(&state, &character).await;
    Html(
        party_pool_page(
            &location,
            &character,
            &inventory,
            &pooled,
            stake,
            &items,
            &food_lots,
            &members,
            equip.first(),
            &personal_targets,
            &party_targets,
            encumbrance.party,
            encumbrance.personal,
        )
        .into_string(),
    )
}
