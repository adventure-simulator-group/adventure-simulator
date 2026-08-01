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

#[derive(Default, Deserialize)]
pub(super) struct ResidencePageQuery {
    residence_notice: Option<String>,
}

#[derive(Default, Deserialize)]
pub(super) struct ResidenceActionForm {
    holding_id: Option<String>,
}

fn residence_notice(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("rented") => Some("The residence is now rented and ready to use."),
        Some("bought") => Some("You bought the residence."),
        Some("relinquished") => Some("You relinquished the residence."),
        Some("designated") => Some("This residence is now your designated home."),
        Some("recovered") => Some("The owned residence is active again."),
        Some("funds") => Some("You do not have enough coin for that."),
        Some("location") => Some("You must be in this settlement to do that."),
        Some("overdue") => Some("Settle the overdue housing cost before doing that."),
        Some("unavailable") => Some("That housing change is not available."),
        _ => None,
    }
}

fn relationship_date_label(minute: u64) -> String {
    let day = minute / adventuresim_core::strategic_time::MINUTES_PER_DAY;
    let year = 1544 + day / 365;
    let day_of_year = day % 365 + 1;
    format!("year {year}, day {day_of_year}")
}

fn housing_error_code(error: &str) -> &'static str {
    if error.contains("coin") || error.contains("fund") {
        "funds"
    } else if error.contains("settlement") || error.contains("co-location") {
        "location"
    } else if error.contains("dormant") || error.contains("due") {
        "overdue"
    } else {
        "unavailable"
    }
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
    location.active_building = building.valid_for(&location).map(str::to_owned);
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
        return Redirect::to(&building.append_to(&state, &kind, &id, destination).await);
    };
    if parse_surgery_limb(&limb).is_none() {
        return Redirect::to(&building.append_to(&state, &kind, &id, destination).await);
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
    Redirect::to(&building.append_to(&state, &kind, &id, destination).await)
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

pub(super) async fn settlement_resident_place(
    State(state): State<AppState>,
    Path((id, place)): Path<(String, String)>,
    Query(page_query): Query<ResidencePageQuery>,
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
    if let Some((organization, chapter)) = organization_chapter
        && !adventuresim_core::organization::chapter_has_standalone_building(
            organization,
            chapter,
            &settlement.economy,
        )
    {
        return Html("<h1>Settlement place not found</h1>".into());
    }
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
    if place == "residences" {
        let settlement_literal = sql_string_literal(&settlement.id);
        let offers_sql = format!(
            "SELECT * FROM settlement_residence_offer WHERE settlement_id = {settlement_literal}"
        );
        let residence_sql = format!(
            "SELECT * FROM backend_character_residence_statuses WHERE character_id = {}",
            character.id,
        );
        let relationship_sql = format!(
            "SELECT * FROM backend_character_relationship_statuses WHERE character_id = {}",
            character.id,
        );
        let owner_key = session.owner_key().unwrap_or_default();
        let family_sql = format!(
            "SELECT * FROM backend_family_children WHERE owner_key = {} AND observer_character_id = {}",
            sql_string_literal(owner_key),
            character.id,
        );
        let (offers, residences, relationship, children) = tokio::join!(
            state.db.query::<SettlementResidenceOffer>(&offers_sql),
            state.db.query::<BackendCharacterResidenceStatus>(&residence_sql),
            state
                .db
                .query_one::<BackendCharacterRelationshipStatus>(&relationship_sql),
            state.db.query::<BackendFamilyChild>(&family_sql),
        );
        let mut offers = offers.unwrap_or_default();
        offers.sort_by_key(|offer| match offer.tier {
            ResidenceTier::Cheap => 0,
            ResidenceTier::Moderate => 1,
            ResidenceTier::Fancy => 2,
        });
        let mut residences = residences.unwrap_or_default();
        residences.retain(|holding| holding.character_id == character.id);
        residences.sort_by(|left, right| {
            (
                !left.primary,
                left.settlement_id != settlement.id,
                left.holding_id.as_str(),
            )
                .cmp(&(
                    !right.primary,
                    right.settlement_id != settlement.id,
                    right.holding_id.as_str(),
                ))
        });
        let can_rest_at_home = residences.iter().any(|home| {
            home.active && home.occupied && home.settlement_id == settlement.id
        });
        let relationship = relationship.ok().flatten();
        let mut children = children.unwrap_or_default();
        children.retain(|child| {
            child.owner_key == owner_key && child.observer_character_id == character.id
        });
        children.sort_by_key(|child| child.child_id);
        let related_ids = relationship
            .iter()
            .flat_map(|status| {
                [status.spouse_id, status.courtship_partner_id]
            })
            .flatten()
            .collect::<Vec<_>>();
        let mut related_characters = Vec::new();
        for related_id in related_ids {
            if let Ok(Some(related)) = state
                .db
                .query_one::<Character>(&format!(
                    "SELECT * FROM backend_characters WHERE id = {related_id}"
                ))
                .await
            {
                related_characters.push(related);
            }
        }
        let character_minute = state
            .db
            .query_one::<CharacterTime>(&format!(
                "SELECT * FROM backend_character_times WHERE character_id = {}",
                character.id
            ))
            .await
            .ok()
            .flatten()
            .map_or(0, |time| time.minutes);
        let wedding = relationship
            .as_ref()
            .and_then(|row| row.wedding_effective_minute)
            .map(|effective_minute| WeddingPresentation {
                days_remaining: effective_minute
                    .saturating_sub(character_minute)
                    .div_ceil(adventuresim_core::strategic_time::MINUTES_PER_DAY),
                date_label: relationship_date_label(effective_minute),
            });
        let presentation = relationship.as_ref().map(|status| {
            let name = |id: Option<u64>| {
                id.and_then(|id| {
                    related_characters
                        .iter()
                        .find(|character| character.id == id)
                        .map(|character| character.name.clone())
                })
            };
            RelationshipPresentation {
                spouse_name: name(status.spouse_id),
                courtship_partner_name: name(status.courtship_partner_id),
                courtship_kind: status.courtship_kind.clone(),
                courtship_exposed: status.courtship_exposed,
                wedding,
                pregnancy_due_days: status.pregnancy_due_minute.map(|due| {
                    due.saturating_sub(character_minute)
                        .div_ceil(adventuresim_core::strategic_time::MINUTES_PER_DAY)
                }),
                children: children
                    .iter()
                    .map(|child| ChildPresentation {
                        name: child.child_name.clone(),
                        stage: child.stage,
                        focus: child.focus,
                        maturity_basis_points: child.maturity_basis_points,
                        adult_playable: child.adult_playable,
                        alive: child.alive,
                    })
                    .collect(),
            }
        });
        return Html(
            settlement_residence_page(
                &settlement,
                character,
                &party_members,
                Some(&character.name),
                &offers,
                &residences,
                presentation.as_ref(),
                can_rest_at_home,
                residence_notice(page_query.residence_notice.as_deref()),
            )
            .into_string(),
        );
    }
    Html(
        settlement_resident_location_page(
            &settlement,
            character,
            &party_members,
            &place,
            Some(&character.name),
        )
        .into_string(),
    )
}

pub(super) async fn change_residence(
    State(state): State<AppState>,
    Path((id, action, tier)): Path<(String, String, String)>,
    session: Session,
    Form(form): Form<ResidenceActionForm>,
) -> Redirect {
    let fallback = format!("/settlements/{id}/places/residences");
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let tier_argument = match tier.as_str() {
        "cheap" => json!({ "cheap": [] }),
        "moderate" => json!({ "moderate": [] }),
        "fancy" => json!({ "fancy": [] }),
        "current" => serde_json::Value::Null,
        _ => return Redirect::to(&format!("{fallback}?residence_notice=unavailable")),
    };
    let selected_holding = form
        .holding_id
        .filter(|holding_id| !holding_id.trim().is_empty());
    let (reducer, args, success) = match action.as_str() {
        "rent" => (
            "rent_residence",
            vec![json!(character_id), json!(id), tier_argument],
            "rented",
        ),
        "buy" => (
            "buy_residence",
            vec![json!(character_id), json!(id), tier_argument],
            "bought",
        ),
        "relinquish" => (
            "relinquish_residence",
            vec![
                json!(character_id),
                json!(match selected_holding.as_ref() {
                    Some(holding_id) if tier == "current" => holding_id,
                    _ => {
                        return Redirect::to(&format!(
                            "{fallback}?residence_notice=unavailable"
                        ));
                    }
                }),
            ],
            "relinquished",
        ),
        "designate" => (
            "designate_residence",
            vec![
                json!(character_id),
                json!(match selected_holding.as_ref() {
                    Some(holding_id) if tier == "current" => holding_id,
                    _ => {
                        return Redirect::to(&format!(
                            "{fallback}?residence_notice=unavailable"
                        ));
                    }
                }),
            ],
            "designated",
        ),
        "recover" => (
            "recover_owned_residence",
            vec![
                json!(character_id),
                json!(match selected_holding.as_ref() {
                    Some(holding_id) if tier == "current" => holding_id,
                    _ => {
                        return Redirect::to(&format!(
                            "{fallback}?residence_notice=unavailable"
                        ));
                    }
                }),
            ],
            "recovered",
        ),
        _ => return Redirect::to(&format!("{fallback}?residence_notice=unavailable")),
    };
    match state.db.call(reducer, &args).await {
        Ok(()) => Redirect::to(&format!("{fallback}?residence_notice={success}")),
        Err(error) => {
            tracing::warn!(character_id, action, tier, %error, "residence acquisition rejected");
            Redirect::to(&format!(
                "{fallback}?residence_notice={}",
                housing_error_code(&error.to_string())
            ))
        }
    }
}

#[cfg(test)]
mod residence_route_tests {
    #[test]
    fn portfolio_reads_and_management_mutations_keep_explicit_holding_ids() {
        let source = include_str!("medical.rs");
        let residence_page = source
            .split("if place == \"residences\"")
            .nth(1)
            .unwrap()
            .split("pub(super) async fn change_residence")
            .next()
            .unwrap();
        assert!(residence_page.contains("query::<BackendCharacterResidenceStatus>"));
        assert!(residence_page.contains("residences.retain"));
        assert!(residence_page.contains("home.active && home.occupied"));
        assert!(residence_page.contains("query::<BackendFamilyChild>"));
        assert!(residence_page.contains("WHERE owner_key = {} AND observer_character_id = {}"));
        assert!(residence_page.contains(
            "child.owner_key == owner_key && child.observer_character_id == character.id"
        ));

        let change = source
            .split("pub(super) async fn change_residence")
            .nth(1)
            .unwrap()
            .split("pub(super) async fn show_settlement_location")
            .next()
            .unwrap();
        assert!(change.contains("Form(form): Form<ResidenceActionForm>"));
        assert!(change.contains("let selected_holding = form"));
        assert!(change.contains(".holding_id"));
        for reducer in [
            "relinquish_residence",
            "designate_residence",
            "recover_owned_residence",
        ] {
            assert!(change.contains(reducer));
        }
        assert!(change.matches("json!(character_id)").count() >= 5);
        assert!(change.matches("json!(match selected_holding.as_ref()").count() == 3);
    }
}

pub(super) async fn show_settlement_location(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<BuildingQuery>,
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
    let mut corpses = if let Some((character, _)) = &active_character {
        state
            .db
            .query::<BackendCorpse>(&format!(
                "SELECT * FROM backend_corpses WHERE owner_character_id = {}",
                character.id
            ))
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, settlement_id = %id, "failed to load settlement corpses");
                Vec::new()
            })
            .into_iter()
            .filter(|corpse| corpse.settlement_id == id && corpse.location != "scene")
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    corpses.sort_by(|left, right| left.corpse_id.cmp(&right.corpse_id));
    let selected_corpse = query.corpse.as_deref().and_then(|corpse_id| {
        corpses
            .iter()
            .position(|corpse| corpse.corpse_id == corpse_id)
            .map(|index| {
                (
                    index,
                    if query.medical.as_deref() == Some("surgery") {
                        "surgery"
                    } else {
                        "physiology"
                    },
                )
            })
    });
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
            &corpses,
            selected_corpse.map(|(index, window)| (&corpses[index], window)),
        )
        .into_string(),
    )
}
