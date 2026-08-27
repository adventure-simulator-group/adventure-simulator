#[derive(Deserialize)]
pub(super) struct CookFoodForm {
    #[serde(default = "default_personal_scope")]
    inventory_scope: String,
    inventory_item_ids: String,
    fractions_micros: String,
}

fn default_personal_scope() -> String {
    "personal".into()
}

#[derive(Deserialize)]
pub(super) struct FireplaceQuery {
    #[serde(default)]
    building: String,
    #[serde(default = "default_personal_scope")]
    inventory_scope: String,
}

#[derive(Deserialize)]
pub(super) struct FireplaceRetrieveForm {
    #[serde(default)]
    container_object_id: Option<u64>,
}

#[derive(Deserialize)]
pub(super) struct FireplaceContainerPlaceForm {
    inventory_scope: String,
    inventory_item_id: u64,
}
#[derive(Deserialize)]
pub(super) struct FireplaceContainerForm {
    container_object_id: u64,
}

fn settlement_fireplace_context(
    settlement: &Settlement,
    building: &str,
) -> Result<String, &'static str> {
    if building.is_empty()
        || matches!(building, "public-square" | "overview" | "map")
        || !building
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err("This settlement page has no fireplace");
    }
    let standard_available = match building {
        "residences" => Some(true),
        "keep" => Some(matches!(
            settlement.category,
            crate::spacetimedb::SettlementCategory::Town
                | crate::spacetimedb::SettlementCategory::City
                | crate::spacetimedb::SettlementCategory::Capital
        )),
        "market" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "merchants",
            ),
        ),
        "forge" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "weapons",
            ),
        ),
        "armoury" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "armor",
            ),
        ),
        "tailor" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "clothing",
            ),
        ),
        "herbalist" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "herbalist",
            ),
        ),
        "inn" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "inn",
            ),
        ),
        "church" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "religion",
            ),
        ),
        "bookstore" => Some(
            adventuresim_core::organization::service_npc_location_available(
                &settlement.economy,
                "books",
            ),
        ),
        _ => None,
    };
    let available = standard_available.unwrap_or_else(|| {
        adventuresim_core::organization::organization_chapter_at(&settlement.id, building)
            .is_some_and(|(organization, chapter)| {
                adventuresim_core::organization::chapter_has_standalone_building(
                    organization,
                    chapter,
                    &settlement.economy,
                )
            })
    });
    if !available {
        return Err("This settlement building has no fireplace");
    }
    let place = if let Some(kind) =
        adventuresim_core::strategic_place::SettlementVenueKind::from_id(building)
    {
        adventuresim_core::strategic_place::StrategicPlaceId::settlement_venue(&settlement.id, kind)
    } else {
        let (organization, chapter) =
            adventuresim_core::organization::organization_chapter_at(&settlement.id, building)
                .ok_or("This settlement page has no canonical fireplace place")?;
        adventuresim_core::strategic_place::StrategicPlaceId::chapter_venue(
            &settlement.id,
            &organization.id,
            &chapter.location_id,
        )
    }
    .map_err(|_| "This settlement page has no canonical fireplace place")?;
    adventuresim_core::strategic_place::StrategicFixtureId::fireplace(place)
        .map(|fixture| fixture.to_string())
        .map_err(|_| "This settlement page has no fireplace")
}

fn party_journey_is_current_camp(party: &Party, journey: &PartyJourney) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.as_ref() == Some(&journey.destination)
        && journey.completed_movement_minutes < journey.total_movement_minutes
        && journey
            .reached_camp_movement_minutes
            .contains(&journey.completed_movement_minutes)
}

async fn camp_fireplace_context(
    state: &AppState,
    actor: &Character,
) -> Result<String, &'static str> {
    let party_id = actor
        .party_id
        .as_deref()
        .ok_or("Character has no active camp")?;
    let party = state
        .db
        .query_one::<Party>(&crate::spacetimedb::party_by_id(party_id))
        .await
        .map_err(|_| "Party state unavailable")?
        .ok_or("Party state unavailable")?;
    let journey = state
        .db
        .query_one::<PartyJourney>(&format!(
            "SELECT * FROM party_journey WHERE party_id = {}",
            sql_string_literal(party_id)
        ))
        .await
        .map_err(|_| "Journey state unavailable")?
        .ok_or("Journey state unavailable")?;
    if !party_journey_is_current_camp(&party, &journey) {
        return Err("This is not the party's current journey camp");
    }
    let place = adventuresim_core::strategic_place::StrategicPlaceId::journey_camp(
        party_id,
        journey.departure_minute,
        journey.completed_movement_minutes,
    )
    .map_err(|_| "This journey camp has no canonical identity")?;
    adventuresim_core::strategic_place::StrategicFixtureId::fireplace(place)
        .map(|fixture| fixture.to_string())
        .map_err(|_| "This journey camp has no fireplace")
}

async fn fireplace_rows(
    state: &AppState,
    actor: &Character,
    fireplace_fixture_id: &str,
) -> (
    Vec<InventoryItem>,
    Vec<PartyInventoryItem>,
    Vec<InventoryItemAmount>,
    Vec<PartyItemAmount>,
    Vec<FoodLot>,
    Vec<ItemDefinition>,
    Option<BackendFireplaceStation>,
    Option<BackendFireplaceDish>,
    Vec<BackendFireplaceStation>,
    Vec<BackendFireplaceDish>,
    u64,
) {
    let personal = state
        .db
        .query::<InventoryItem>(&format!(
            "SELECT * FROM inventory_item WHERE character_id = {}",
            actor.id
        ))
        .await
        .unwrap_or_default();
    let party = if let Some(party_id) = actor.party_id.as_deref() {
        state
            .db
            .query::<PartyInventoryItem>(&format!(
                "SELECT * FROM party_inventory_item WHERE party_id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let personal_amounts = state
        .db
        .query::<InventoryItemAmount>("SELECT * FROM inventory_item_amount")
        .await
        .unwrap_or_default();
    let party_amounts = state
        .db
        .query::<PartyItemAmount>("SELECT * FROM party_item_amount")
        .await
        .unwrap_or_default();
    let lots = state
        .db
        .query::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let definitions = state
        .db
        .query::<ItemDefinition>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let key = format!("{}|{}", actor.id, fireplace_fixture_id);
    let station = state
        .db
        .query_one::<BackendFireplaceStation>(&format!(
            "SELECT * FROM backend_fireplace_stations WHERE key = {}",
            sql_string_literal(&key)
        ))
        .await
        .ok()
        .flatten();
    let dish = state
        .db
        .query_one::<BackendFireplaceDish>(&format!(
            "SELECT * FROM backend_fireplace_dishes WHERE station_key = {}",
            sql_string_literal(&key)
        ))
        .await
        .ok()
        .flatten();
    let vessel_stations = state
        .db
        .query::<BackendFireplaceStation>(&format!(
            "SELECT * FROM backend_fireplace_stations WHERE character_id = {}",
            actor.id
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| {
            row.fireplace_fixture_id == fireplace_fixture_id && row.instrument_object_id.is_some()
        })
        .collect::<Vec<_>>();
    let vessel_keys = vessel_stations
        .iter()
        .map(|row| row.key.as_str())
        .collect::<HashSet<_>>();
    let vessel_dishes = state
        .db
        .query::<BackendFireplaceDish>("SELECT * FROM backend_fireplace_dishes")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|row| vessel_keys.contains(row.station_key.as_str()))
        .collect::<Vec<_>>();
    let minute = query_single::<CharacterTime>(state, "backend_character_times", actor.id)
        .await
        .map_or(0, |row| row.minutes);
    (
        personal,
        party,
        personal_amounts,
        party_amounts,
        lots,
        definitions,
        station,
        dish,
        vessel_stations,
        vessel_dishes,
        minute,
    )
}

pub(super) async fn settlement_fireplace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    if actor.current_settlement_id.as_deref() != Some(id.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            "The character is not at this settlement",
        )
            .into_response();
    }
    let Some(settlement) = state
        .db
        .query_one::<Settlement>(&crate::spacetimedb::settlement_by_id(&id))
        .await
        .ok()
        .flatten()
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let context = match settlement_fireplace_context(&settlement, &query.building) {
        Ok(v) => v,
        Err(e) => return (StatusCode::NOT_FOUND, e).into_response(),
    };
    let rows = fireplace_rows(&state, &actor, &context).await;
    let action_base = format!(
        "/locations/settlement/{id}/fireplace?building={}",
        query.building
    );
    let post_base = format!("/locations/settlement/{id}/fireplace");
    let back = format!("/locations/settlement/{id}");
    let active = query.building.clone();
    Html(
        crate::templates::settlement::fireplace_page(
            "Fireplace",
            &back,
            &action_base,
            &format!("/locations/settlement/{id}/map/rest"),
            &actor,
            if query.inventory_scope == "party" {
                "party"
            } else {
                "personal"
            },
            &rows.0,
            &rows.1,
            &rows.2,
            &rows.3,
            &rows.4,
            &rows.5,
            rows.6.as_ref(),
            rows.7.as_ref(),
            &rows.8,
            &rows.9,
            rows.10,
            |content| {
                crate::templates::settlement_layout_with_session(
                    "Fireplace",
                    &settlement.name,
                    &settlement.id,
                    &settlement.category,
                    &active,
                    Some(&settlement.religion_id),
                    Some(&settlement.economy),
                    content,
                    Some(&actor.name),
                )
            },
        )
        .into_string()
        .replace(
            &format!("{action_base}/ingredients"),
            &format!("{post_base}/ingredients?building={}", query.building),
        )
        .replace(
            &format!("{action_base}/instrument"),
            &format!("{post_base}/instrument?building={}", query.building),
        )
        .replace(
            &format!("{action_base}/retrieve"),
            &format!("{post_base}/retrieve?building={}", query.building),
        )
        .replace(
            &format!("{action_base}/container/place"),
            &format!("{post_base}/container/place?building={}", query.building),
        )
        .replace(
            &format!("{action_base}/container/start"),
            &format!("{post_base}/container/start?building={}", query.building),
        )
        .replace(
            &format!("{action_base}/container/remove"),
            &format!("{post_base}/container/remove?building={}", query.building),
        ),
    )
    .into_response()
}

pub(super) async fn camp_fireplace_page(
    State(state): State<AppState>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match camp_fireplace_context(&state, &actor).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let rows = fireplace_rows(&state, &actor, &context).await;
    Html(
        crate::templates::settlement::fireplace_page(
            "Campfire",
            "/camp",
            "/camp/fireplace",
            "/camp/rest",
            &actor,
            if query.inventory_scope == "party" {
                "party"
            } else {
                "personal"
            },
            &rows.0,
            &rows.1,
            &rows.2,
            &rows.3,
            &rows.4,
            &rows.5,
            rows.6.as_ref(),
            rows.7.as_ref(),
            &rows.8,
            &rows.9,
            rows.10,
            |content| {
                crate::templates::camp_location_layout_with_session(
                    "Campfire",
                    "Camp",
                    actor.party_id.as_deref().unwrap_or("camp"),
                    true,
                    content,
                    Some(&actor.name),
                )
            },
        )
        .into_string(),
    )
    .into_response()
}

async fn fireplace_post_context(
    state: &AppState,
    actor: &Character,
    settlement: Option<(&str, &str)>,
) -> Result<String, &'static str> {
    match settlement {
        Some((id, building)) => {
            if actor.current_settlement_id.as_deref() != Some(id) {
                return Err("The character is not at this settlement");
            }
            let settlement = state
                .db
                .query_one::<Settlement>(&crate::spacetimedb::settlement_by_id(id))
                .await
                .map_err(|_| "Settlement state unavailable")?
                .ok_or("Settlement not found")?;
            settlement_fireplace_context(&settlement, building)
        }
        None => camp_fireplace_context(state, actor).await,
    }
}

async fn post_fireplace_ingredients(
    state: AppState,
    actor: Character,
    context: String,
    form: CookFoodForm,
    redirect: String,
) -> Response {
    let parse_ids = form
        .inventory_item_ids
        .split(',')
        .filter(|v| !v.is_empty())
        .map(str::parse)
        .collect::<Result<Vec<u64>, _>>();
    let parse_amounts = form
        .fractions_micros
        .split(',')
        .filter(|v| !v.is_empty())
        .map(str::parse)
        .collect::<Result<Vec<u32>, _>>();
    let (Ok(ids), Ok(amounts)) = (parse_ids, parse_amounts) else {
        return (StatusCode::BAD_REQUEST, "Invalid ingredient selection").into_response();
    };
    match state
        .db
        .call(
            "add_fireplace_ingredients",
            &[
                json!(actor.id),
                json!(context),
                json!(form.inventory_scope),
                json!(ids),
                json!(amounts),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to(&redirect).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

pub(super) async fn settlement_fireplace_ingredients(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
    Form(form): Form<CookFoodForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, Some((&id, &query.building))).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_ingredients(
        state,
        actor,
        context,
        form,
        format!(
            "/locations/settlement/{id}/fireplace?building={}",
            query.building
        ),
    )
    .await
}

pub(super) async fn camp_fireplace_ingredients(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<CookFoodForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, None).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_ingredients(state, actor, context, form, "/camp/fireplace".into()).await
}

async fn post_fireplace_retrieve(
    state: AppState,
    actor: Character,
    context: String,
    form: FireplaceRetrieveForm,
    redirect: String,
) -> Response {
    match state
        .db
        .call(
            "retrieve_fireplace_dish",
            &[
                json!(actor.id),
                json!(context),
                crate::spacetimedb::sats_option(form.container_object_id),
            ],
        )
        .await
    {
        Ok(()) => Redirect::to(&redirect).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
pub(super) async fn settlement_fireplace_retrieve(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
    Form(form): Form<FireplaceRetrieveForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, Some((&id, &query.building))).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_retrieve(
        state,
        actor,
        context,
        form,
        format!(
            "/locations/settlement/{id}/fireplace?building={}",
            query.building
        ),
    )
    .await
}
pub(super) async fn camp_fireplace_retrieve(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<FireplaceRetrieveForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, None).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_retrieve(state, actor, context, form, "/camp/fireplace".into()).await
}

async fn post_fireplace_container(
    state: AppState,
    actor: Character,
    context: String,
    reducer: &str,
    args: Vec<serde_json::Value>,
    redirect: String,
) -> Response {
    let mut reducer_args = vec![json!(actor.id), json!(context)];
    reducer_args.extend(args);
    match state.db.call(reducer, &reducer_args).await {
        Ok(()) => Redirect::to(&redirect).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn settlement_fireplace_container_place(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
    Form(form): Form<FireplaceContainerPlaceForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, Some((&id, &query.building))).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_container(
        state,
        actor,
        context,
        "place_fireplace_container",
        vec![json!(form.inventory_scope), json!(form.inventory_item_id)],
        format!(
            "/locations/settlement/{id}/fireplace?building={}",
            query.building
        ),
    )
    .await
}
pub(super) async fn camp_fireplace_container_place(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<FireplaceContainerPlaceForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, None).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_container(
        state,
        actor,
        context,
        "place_fireplace_container",
        vec![json!(form.inventory_scope), json!(form.inventory_item_id)],
        "/camp/fireplace".into(),
    )
    .await
}
pub(super) async fn settlement_fireplace_container_start(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
    Form(form): Form<FireplaceContainerForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, Some((&id, &query.building))).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_container(
        state,
        actor,
        context,
        "start_fireplace_container_cooking",
        vec![json!(form.container_object_id)],
        format!(
            "/locations/settlement/{id}/fireplace?building={}",
            query.building
        ),
    )
    .await
}
pub(super) async fn camp_fireplace_container_start(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<FireplaceContainerForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, None).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_container(
        state,
        actor,
        context,
        "start_fireplace_container_cooking",
        vec![json!(form.container_object_id)],
        "/camp/fireplace".into(),
    )
    .await
}
pub(super) async fn settlement_fireplace_container_remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FireplaceQuery>,
    session: Session,
    Form(form): Form<FireplaceContainerForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, Some((&id, &query.building))).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_container(
        state,
        actor,
        context,
        "retrieve_fireplace_container",
        vec![json!(form.container_object_id)],
        format!(
            "/locations/settlement/{id}/fireplace?building={}",
            query.building
        ),
    )
    .await
}
pub(super) async fn camp_fireplace_container_remove(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<FireplaceContainerForm>,
) -> Response {
    let Some((actor, _)) = get_active_character(&state, session.character_id_u64()).await else {
        return Redirect::to("/characters").into_response();
    };
    let context = match fireplace_post_context(&state, &actor, None).await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    post_fireplace_container(
        state,
        actor,
        context,
        "retrieve_fireplace_container",
        vec![json!(form.container_object_id)],
        "/camp/fireplace".into(),
    )
    .await
}

pub(super) async fn party_religion_knowledge_check(
    state: &AppState,
    party_members: &[Character],
    religion_id: &str,
) -> f32 {
    let Some(religion) = OfficialReligion::from_id(religion_id) else {
        return 0.0;
    };
    let mut checks = Vec::with_capacity(party_members.len());
    for member in living_party_member_refs(party_members) {
        let skills =
            query_single::<CharacterSkills>(state, "backend_character_skills", member.id).await;
        let attributes =
            query_single::<CharacterAttributes>(state, "backend_character_attributes", member.id)
                .await;
        let limbs =
            query_single::<CharacterLimbs>(state, "backend_character_limbs", member.id).await;
        let stats =
            query_single::<CharacterStats>(state, "backend_character_stats", member.id).await;
        if let (Some(skills), Some(attributes), Some(limbs), Some(stats)) =
            (skills, attributes, limbs, stats)
        {
            checks.push(adventuresim_core::capability::religion_knowledge_check(
                skills.religion_hours.effective(religion),
                attributes.instinct,
                attributes.intelligence,
                stats.focus,
                limbs.head_health,
            ));
        }
    }
    adventuresim_core::capability::aggregate_party_check(checks).clamp(0.0, 5.0)
}

pub(super) fn living_party_member_refs(
    party_members: &[Character],
) -> impl Iterator<Item = &Character> {
    party_members.iter().filter(|member| member.alive)
}

#[cfg(test)]
mod party_religion_knowledge_tests {
    use super::living_party_member_refs;
    use crate::spacetimedb::Character;

    fn party_member(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    #[test]
    fn prayer_preview_knowledge_excludes_dead_party_members() {
        let members = [party_member(1, true), party_member(2, false)];
        let ids = living_party_member_refs(&members)
            .map(|member| member.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![1]);
    }
}
