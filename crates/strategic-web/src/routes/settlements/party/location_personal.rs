pub(super) enum LocationLookup {
    Found(LocationView),
    NotFound,
    Unavailable,
}

pub(super) async fn resolve_location(state: &AppState, kind: &str, id: &str) -> LocationLookup {
    let Ok(kind) = kind.parse::<LocationKind>() else {
        return LocationLookup::NotFound;
    };
    let location = match kind {
        LocationKind::Settlement => state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(id)
            ))
            .await
            .map(|row| {
                row.map(|settlement| {
                    (
                        settlement.name,
                        Some(settlement.category),
                        Some(settlement.religion_id),
                    )
                })
            }),
        LocationKind::CaseSite => state
            .db
            .query_one::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE case_site_id = {}",
                sql_string_literal(id)
            ))
            .await
            .map(|row| row.map(|site| (site.display_title, None, None))),
    };
    let (name, category, religion_id) = match location {
        Ok(Some(location)) => location,
        Ok(None) => return LocationLookup::NotFound,
        Err(error) => {
            tracing::error!(%error, "failed to resolve location");
            return LocationLookup::Unavailable;
        }
    };
    LocationLookup::Found(LocationView {
        kind,
        id: id.to_string(),
        name,
        religion_id,
        category,
        active_building: None,
    })
}

pub(super) fn character_is_at_location(character: &Character, location: &LocationView) -> bool {
    match location.kind {
        LocationKind::Settlement => {
            character.current_settlement_id.as_deref() == Some(location.id.as_str())
        }
        LocationKind::CaseSite => {
            character.current_case_site_id.as_deref() == Some(location.id.as_str())
        }
    }
}

pub(super) async fn party_personal(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    render_party_personal(
        &state,
        &kind,
        &id,
        character_id,
        building,
        &session,
        None,
        None,
        false,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_party_personal(
    state: &AppState,
    kind: &str,
    id: &str,
    character_id: u64,
    building: BuildingQuery,
    session: &Session,
    dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
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
    if character_id != active_character.id {
        return Html("<h1>Party member not found</h1>".to_string());
    }
    let party_members = get_active_party_members(&state, Some(&active_character)).await;
    let attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_character_attributes WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_character_skills WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_character_limbs WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let schedule: Vec<CharacterTrainingSchedule> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_character_training_schedules WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let apprenticeships: Vec<crate::spacetimedb::OrganizationMembership> = state
        .db
        .query(&format!(
            "SELECT * FROM organization_membership WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let organization_presentation = state
        .db
        .query_one::<crate::spacetimedb::OrganizationPresentation>(&format!(
            "SELECT * FROM organization_presentation WHERE character_id = {character_id}"
        ))
        .await
        .ok()
        .flatten();
    let character_minute = query_single::<CharacterTime>(&state, "backend_character_times", character_id)
        .await
        .map_or(0, |time| time.minutes);
    let capability = get_character_capability(&state, character_id).await;
    let combat_profile = get_combat_training_profile(&state, character_id).await;
    let can_examine = false;
    let stats = query_single::<CharacterStats>(&state, "backend_character_stats", character_id).await;
    let case_site = if location.kind == LocationKind::CaseSite {
        state
            .db
            .query_one::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE case_site_id = {}",
                sql_string_literal(&location.id)
            ))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let settlement = if location.kind == LocationKind::Settlement {
        state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(&location.id)
            ))
            .await
            .ok()
            .flatten()
    } else if let Some(site) = case_site.as_ref() {
        state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(&site.origin_settlement_id)
            ))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let activity_preview = ActivityPreviewRates::from_character(
        attributes.first(),
        skills.first(),
        limbs.first(),
        capability.as_ref(),
        settlement.as_ref(),
        stats.as_ref(),
    )
    .with_professions(
        attributes.first(),
        skills.first(),
        &apprenticeships,
        &location.id,
        character_minute,
    )
    .with_reading(
        attributes.first(),
        skills.first(),
        settlement.as_ref(),
        active_inventory.iter().filter(|item| item.qty > 0).map(|item| item.item_id.as_str()),
    );
    let activity_location = match location.kind {
        LocationKind::Settlement => adventuresim_core::activity::ActivityLocation::Settlement {
            has_inn: settlement
                .as_ref()
                .is_some_and(|settlement| settlement.economy
                .has_service(adventuresim_world_schema::SettlementService::Inn)),
        },
        LocationKind::CaseSite
            if case_site
                .as_ref()
                .is_some_and(|site| site.distance_m > 0 && !site.case_id.starts_with("incident:")) =>
        {
            adventuresim_core::activity::ActivityLocation::NamedOutdoorLocation
        }
        LocationKind::CaseSite => {
            adventuresim_core::activity::ActivityLocation::IneligibleNamedLocation
        }
    };
    let condition = get_strategic_condition(&state, character_id).await;
    let morale_sources = get_morale_sources(&state, character_id).await;
    let religion = query_single::<CharacterCondition>(&state, "backend_character_conditions", character_id)
        .await
        .and_then(|condition| condition.religion_id);
    let prayer_religion_check = match religion.as_deref() {
        Some(religion_id) => {
            party_religion_knowledge_check(&state, &party_members, religion_id).await
        }
        None => 0.0,
    };
    let reputation_location_id = settlement
        .as_ref()
        .map_or(location.id.as_str(), |settlement| settlement.id.as_str());
    let reputation = query_local_reputation(&state, character_id, reputation_location_id).await;
    let fame = reputation
        .as_ref()
        .map_or(0.0, |value| value.fame as f32 / 100.0);
    let infamy = reputation
        .as_ref()
        .map_or(0.0, |value| value.infamy as f32 / 100.0);
    // Authoritative personality is private. Ordinary pages render only
    // observer-specific beliefs through the dedicated social route.
    let personality: Option<CharacterPersonality> = None;
    let medical = medical_presentation(&state, character_id, character_id).await;
    let injuries = state
        .db
        .query::<LimbInjury>(&format!(
            "SELECT * FROM limb_injury WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let projectiles = state
        .db
        .query::<RetainedProjectile>(&format!(
            "SELECT * FROM retained_projectile WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let religious_demand = state
        .db
        .query::<ReligiousDemand>(&format!(
            "SELECT * FROM religious_demand WHERE character_id = {character_id} AND status = 'pending'"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    let filth = state
        .db
        .query::<crate::spacetimedb::CharacterFilth>(&format!(
            "SELECT * FROM character_filth WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let food_lots = state
        .db
        .query::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let inventory_amounts = state
        .db
        .query::<InventoryItemAmount>("SELECT * FROM inventory_item_amount")
        .await
        .unwrap_or_default();
    let item_definitions = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let foraging_dialog = if building.forage.unwrap_or(false) {
        Some(
            crate::routes::foraging::activity_dialog(
                state,
                &active_character,
                &location
                    .preserve_building(format!("{}/party/{character_id}", location.base_path(),)),
                building.forage_receipt.as_deref(),
                building.forage_error.as_deref(),
            )
            .await,
        )
    } else {
        None
    };
    Html(
        party_personal_page(
            &location,
            &active_character,
            &party_members,
            capability.as_ref(),
            attributes.first(),
            skills.first(),
            limbs.first(),
            condition.as_ref(),
            &morale_sources,
            religion.as_deref(),
            &apprenticeships,
            organization_presentation.as_ref(),
            character_minute,
            prayer_religion_check,
            schedule.first(),
            combat_profile,
            activity_preview,
            activity_location,
            religious_demand.as_ref(),
            fame,
            infamy,
            personality.as_ref(),
            &medical,
            can_examine,
            &injuries,
            &projectiles,
            &filth,
            building.cooking(),
            building.herbalism(),
            &active_inventory,
            &inventory_amounts,
            &food_lots,
            &item_definitions,
            dialog,
            surgery_open,
            social_open,
            foraging_dialog,
        )
        .into_string(),
    )
}

#[cfg(test)]
mod location_activity_tests {
    #[test]
    fn case_site_preview_uses_origin_settlement_and_positive_distance_policy() {
        let source = include_str!("location_personal.rs");
        let origin = source.find("site.origin_settlement_id").unwrap();
        let preview = source.find("ActivityPreviewRates::from_character").unwrap();
        assert!(origin < preview);
        assert!(source.contains("site.distance_m > 0 && !site.case_id.starts_with"));
        assert!(source.contains("ActivityLocation::IneligibleNamedLocation"));
    }
}
