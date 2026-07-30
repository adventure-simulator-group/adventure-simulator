pub(super) fn parse_surgery_limb(slug: &str) -> Option<LimbRegion> {
    Some(match slug {
        "left-arm" => LimbRegion::LeftArm,
        "right-arm" => LimbRegion::RightArm,
        "left-leg" => LimbRegion::LeftLeg,
        "right-leg" => LimbRegion::RightLeg,
        "chest" => LimbRegion::Chest,
        "stomach" => LimbRegion::Stomach,
        "head" => LimbRegion::Head,
        _ => return None,
    })
}

pub(super) async fn required_surgery_rows<T>(
    state: &AppState,
    sql: &str,
    data_kind: &'static str,
) -> Result<Vec<T>, Html<String>>
where
    T: serde::de::DeserializeOwned,
{
    state.db.query(sql).await.map_err(|error| {
        tracing::error!(%error, data_kind, "failed to load surgery data");
        Html("<h1>Strategic medical data is unavailable</h1>".into())
    })
}

pub(super) async fn surgery(
    State(state): State<AppState>,
    Path((kind, id, patient_id, limb)): Path<(String, String, u64, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    let Some(actor_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let Some(selected_limb) = parse_surgery_limb(&limb) else {
        return Html("<h1>Limb not found</h1>".into());
    };
    let mut location = match resolve_location(&state, &kind, &id).await {
        LocationLookup::Found(location) => location,
        LocationLookup::NotFound => return Html("<h1>Location not found</h1>".into()),
        LocationLookup::Unavailable => {
            return Html("<h1>Strategic data is unavailable</h1>".into());
        }
    };
    location.active_building = building.valid().map(str::to_owned);
    let Some((active, _)) = get_active_character(&state, Some(actor_id)).await else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    let party_members = get_active_party_members(&state, Some(&active)).await;
    let Some(patient) = party_members
        .iter()
        .find(|member| member.id == patient_id)
        .cloned()
    else {
        return Html("<h1>Party member not found</h1>".into());
    };
    if !character_is_at_location(&active, &location)
        || !character_is_at_location(&patient, &location)
    {
        return Html("<h1>Surgeon and patient must be together</h1>".into());
    }
    let injuries = match required_surgery_rows::<LimbInjury>(
        &state,
        &format!("SELECT * FROM limb_injury WHERE character_id = {patient_id}"),
        "patient injuries",
    )
    .await
    {
        Ok(rows) => rows,
        Err(response) => return response,
    };
    let projectiles = match required_surgery_rows::<RetainedProjectile>(
        &state,
        &format!("SELECT * FROM retained_projectile WHERE character_id = {patient_id}"),
        "retained projectiles",
    )
    .await
    {
        Ok(rows) => rows,
        Err(response) => return response,
    };
    let inventory = match required_surgery_rows::<InventoryItem>(
        &state,
        &format!("SELECT * FROM inventory_item WHERE character_id = {actor_id}"),
        "surgeon inventory",
    )
    .await
    {
        Ok(rows) => rows,
        Err(response) => return response,
    };
    let item_definitions = match required_surgery_rows::<ItemDefinition>(
        &state,
        "SELECT * FROM item",
        "item definitions",
    )
    .await
    {
        Ok(rows) => rows,
        Err(response) => return response,
    };
    let alcohol_count = inventory
        .iter()
        .filter(|entry| {
            item_definitions
                .iter()
                .any(|def| def.id == entry.item_id && def.alcohol_disinfectant_effectiveness > 0)
        })
        .map(|entry| entry.qty)
        .sum();
    let disinfectants = inventory
        .iter()
        .filter_map(|entry| {
            item_definitions
                .iter()
                .find(|def| def.id == entry.item_id && def.alcohol_disinfectant_effectiveness > 0)
                .map(|def| {
                    (
                        def.alcohol_disinfectant_effectiveness,
                        entry.id,
                        def.id.as_str(),
                    )
                })
        })
        .collect::<Vec<_>>();
    let selected_alcohol = adventuresim_core::alcohol::best_disinfectant(
        &disinfectants
            .iter()
            .map(|(effectiveness, id, _)| (*effectiveness, *id))
            .collect::<Vec<_>>(),
    )
    .map(|index| disinfectants[index].2);
    let actor_injuries = if actor_id == patient_id {
        injuries.clone()
    } else {
        match required_surgery_rows::<LimbInjury>(
            &state,
            &format!("SELECT * FROM limb_injury WHERE character_id = {actor_id}"),
            "surgeon injuries",
        )
        .await
        {
            Ok(rows) => rows,
            Err(response) => return response,
        }
    };
    let quantity = |item_id: &str| {
        inventory
            .iter()
            .filter(|item| item.item_id == item_id)
            .map(|item| item.qty)
            .sum()
    };
    let surgery_check = get_character_capability(&state, actor_id)
        .await
        .map_or(0.0, |capability| capability.surgery);
    let available_splints = inventory
        .iter()
        .filter(|item| {
            item.item_id == "splint"
                && !actor_injuries
                    .iter()
                    .any(|injury| injury.splint_inventory_item_id == Some(item.id))
        })
        .map(|item| item.qty)
        .sum();
    let dialog = surgery_dialog(
        &location,
        &active,
        &patient,
        &injuries,
        &projectiles,
        selected_limb,
        quantity("bandage"),
        quantity("surgery_kit"),
        available_splints,
        quantity("soft_soap"),
        alcohol_count,
        selected_alcohol,
        surgery_check,
    );
    if patient_id == active.id {
        render_party_personal(
            &state,
            &kind,
            &id,
            patient_id,
            building,
            &session,
            Some(dialog),
            Some(&limb),
            false,
        )
        .await
    } else {
        render_party_stats(
            &state,
            &kind,
            &id,
            patient_id,
            building,
            &session,
            Some(dialog),
            Some(&limb),
            false,
        )
        .await
    }
}

#[derive(Deserialize)]
pub(super) struct SurgeryProcedureForm {
    procedure: String,
    projectile_id: Option<u64>,
    #[serde(default)]
    use_soap: bool,
}

pub(super) fn schedule_allocation_reducer_arg(schedule: &ScheduleAllocation) -> serde_json::Value {
    let mut value = json!(schedule);
    value["apprenticeship_organization_id"] =
        crate::spacetimedb::sats_option(schedule.apprenticeship_organization_id.as_deref());
    value["practice_organization_id"] =
        crate::spacetimedb::sats_option(schedule.practice_organization_id.as_deref());
    value
}

#[cfg(test)]
mod surgery_reducer_argument_tests {
    use super::schedule_allocation_reducer_arg;
    use crate::spacetimedb::ScheduleAllocation;
    use serde_json::json;

    #[test]
    fn schedule_profession_ids_use_spacetime_option_encoding() {
        let encoded = schedule_allocation_reducer_arg(&ScheduleAllocation {
            apprenticeship_organization_id: Some("armourers_guild".into()),
            practice_organization_id: None,
            ..Default::default()
        });
        assert_eq!(
            encoded["apprenticeship_organization_id"],
            json!({ "some": "armourers_guild" })
        );
        assert_eq!(encoded["practice_organization_id"], json!({ "none": [] }));
    }
}

pub(super) async fn perform_surgery(
    State(state): State<AppState>,
    Path((kind, id, patient_id, limb)): Path<(String, String, u64, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<SurgeryProcedureForm>,
) -> Redirect {
    let destination = format!("/locations/{kind}/{id}/party/{patient_id}/surgery/{limb}");
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to(&building.append_to(destination));
    };
    if parse_surgery_limb(&limb).is_none() {
        return Redirect::to(&building.append_to(destination));
    }
    if let Err(error) = state
        .db
        .call(
            "treat_limb",
            &[
                json!(actor_id),
                json!(patient_id),
                json!(limb),
                json!(form.procedure),
                crate::spacetimedb::sats_option(form.projectile_id),
                json!(form.use_soap),
            ],
        )
        .await
    {
        tracing::warn!(?error, "Manual surgery procedure failed");
    }
    Redirect::to(&building.append_to(destination))
}

pub(super) async fn alchemy(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Response {
    if session.character_id_u64().is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Html(crate::templates::strategic_notice_page(
        "Alchemy is not yet modelled",
        "Physiology observes patients and administers existing preparations; it does not craft them. Herbalism issue #214 owns preparations, and chemistry issue #215 owns Alchemy.",
        &format!("/locations/settlement/{id}"),
        "Return to the settlement",
        None,
    ).into_string())
    .into_response()
}
#[derive(Deserialize)]
pub(super) struct RepairItemForm {
    inventory_item_id: u64,
}

#[cfg(test)]
mod repair_route_tests {
    use adventuresim_core::durability::RepairService;

    #[test]
    fn repair_routes_dispatch_all_and_only_the_three_authoritative_services() {
        assert_eq!(
            RepairService::parse("weapons"),
            Some(RepairService::Weapons)
        );
        assert_eq!(RepairService::parse("armor"), Some(RepairService::Armor));
        assert_eq!(
            RepairService::parse("clothing"),
            Some(RepairService::Clothing)
        );
        assert_eq!(RepairService::parse("merchants"), None);
        assert_eq!(RepairService::parse("smith"), None);
    }
}

pub(super) async fn submit_repair(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RepairItemForm>,
) -> Redirect {
    if let Some(service) = RepairService::parse(&shop) {
        if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
        {
            if let Err(error) = state
                .db
                .call(
                    "submit_item_for_repair",
                    &[
                        json!(character.id),
                        json!(id),
                        json!(service.as_str()),
                        json!(form.inventory_item_id),
                    ],
                )
                .await
            {
                tracing::warn!(%error, character_id = character.id, settlement_id = %id, shop = service.as_str(), "failed to submit item for repair");
            }
        }
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

pub(super) async fn submit_all_repairs(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
) -> Redirect {
    if let Some(service) = RepairService::parse(&shop) {
        if let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
        {
            if let Err(error) = state
                .db
                .call(
                    "submit_all_repairable_items",
                    &[json!(character.id), json!(id), json!(service.as_str())],
                )
                .await
            {
                tracing::warn!(%error, character_id = character.id, settlement_id = %id, shop = service.as_str(), "failed to submit repairable items");
            }
        }
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

pub(super) async fn retrieve_repair(
    State(state): State<AppState>,
    Path((id, shop, order_id)): Path<(String, String, u64)>,
    session: Session,
) -> Redirect {
    if RepairService::parse(&shop).is_some()
        && let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    {
        if let Err(error) = state
            .db
            .call(
                "retrieve_repaired_item",
                &[json!(character.id), json!(order_id)],
            )
            .await
        {
            tracing::warn!(%error, character_id = character.id, settlement_id = %id, order_id, "failed to retrieve repaired item");
        }
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

#[derive(Deserialize)]
pub(super) struct RetrieveRepairsForm {
    item_id: Option<String>,
    limit: u32,
}

pub(super) async fn retrieve_repairs(
    State(state): State<AppState>,
    Path((id, shop)): Path<(String, String)>,
    session: Session,
    Form(form): Form<RetrieveRepairsForm>,
) -> Redirect {
    if let Some(service) = RepairService::parse(&shop)
        && let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    {
        if let Err(error) = state
            .db
            .call(
                "retrieve_repaired_items",
                &[
                    json!(character.id),
                    json!(id),
                    json!(service.as_str()),
                    json!(form.item_id),
                    json!(form.limit),
                ],
            )
            .await
        {
            tracing::warn!(%error, character_id = character.id, settlement_id = %id, shop = service.as_str(), "failed to retrieve repaired items");
        }
    }
    Redirect::to(&format!("/settlements/{id}/{shop}"))
}

pub(super) async fn show_settlement(Path(id): Path<String>) -> Redirect {
    Redirect::to(&format!("/locations/settlement/{id}"))
}

pub(super) async fn settlement_npc_place(
    State(state): State<AppState>,
    Path((id, place)): Path<(String, String)>,
    session: Session,
) -> Html<String> {
    let organization_chapter =
        adventuresim_core::organization::organization_chapter_at(&id, &place);
    if !matches!(place.as_str(), "overview" | "residences" | "keep")
        && organization_chapter.is_none()
    {
        return Html("<h1>Settlement place not found</h1>".into());
    }
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .ok()
        .flatten();
    let Some(settlement) = settlement else {
        return Html("<h1>Settlement not found</h1>".into());
    };
    let active = get_active_character(&state, session.character_id_u64()).await;
    let Some((character, _)) = active.as_ref() else {
        return Html("<h1>Choose a character first</h1>".into());
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return Html("<h1>You are not in this settlement</h1>".into());
    }
    if place == "keep"
        && !matches!(
            settlement.category,
            crate::spacetimedb::SettlementCategory::Town
                | crate::spacetimedb::SettlementCategory::City
                | crate::spacetimedb::SettlementCategory::Capital
        )
    {
        return Html("<h1>This settlement has no keep</h1>".into());
    }
    let party_members = get_active_party_members(&state, Some(character)).await;
    Html(
        settlement_npc_location_page(
            &settlement,
            character,
            &party_members,
            &place,
            Some(&character.name),
        )
        .into_string(),
    )
}

pub(super) async fn show_settlement_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let settlement_literal = sql_string_literal(&id);
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {settlement_literal}"
        ))
        .await;
    let settlement = match settlement {
        Ok(Some(settlement)) => settlement,
        Ok(None) => return Html("<h1>Settlement not found</h1>".to_string()),
        Err(error) => {
            tracing::error!(%error, settlement_id = %id, "failed to load settlement");
            return Html("<h1>Settlement data unavailable</h1>".to_string());
        }
    };
    if let Err(error) = state
        .db
        .call("ensure_settlement_activity", &[json!(id.clone())])
        .await
    {
        tracing::warn!(%error, settlement_id = %id, "failed to activate settlement activity");
    }
    let alias_sql =
        format!("SELECT * FROM settlement_alias WHERE settlement_id = {settlement_literal}");
    let description_sql =
        format!("SELECT * FROM settlement_description WHERE settlement_id = {settlement_literal}");
    let (aliases, descriptions, active_character) = tokio::join!(
        state.db.query::<SettlementAlias>(&alias_sql),
        state.db.query::<SettlementDescription>(&description_sql),
        get_active_character(&state, session.character_id_u64()),
    );
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let aliases = aliases.unwrap_or_else(|error| {
        tracing::warn!(%error, settlement_id = %id, "failed to load settlement aliases");
        Vec::new()
    });
    let descriptions = descriptions.unwrap_or_else(|error| {
        tracing::warn!(%error, settlement_id = %id, "failed to load settlement descriptions");
        Vec::new()
    });
    let mut aliases: Vec<_> = aliases
        .into_iter()
        .filter(|alias| alias.settlement_id == id)
        .collect();
    aliases.sort_by(|left, right| left.id.cmp(&right.id));
    let mut descriptions: Vec<_> = descriptions
        .into_iter()
        .filter(|description| description.settlement_id == id)
        .collect();
    descriptions.sort_by(|left, right| left.id.cmp(&right.id));
    Html(
        settlement_overview_page(
            &settlement,
            &aliases,
            &descriptions,
            active_character.as_ref().map(|(character, _)| character),
            &party_members,
            logged_in_as.as_deref(),
        )
        .into_string(),
    )
}
