#[derive(Deserialize)]
pub(crate) struct RestForm {
    pub(crate) duration: String,
    pub(crate) unit: String,
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    pub(crate) requested_minutes: Option<u64>,
    #[serde(default = "default_field_shelter")]
    pub(crate) shelter: String,
}

fn default_field_shelter() -> String {
    String::new()
}

pub(crate) fn field_shelter_argument(form: &RestForm) -> Result<serde_json::Value, &'static str> {
    match form.shelter.as_str() {
        "bivouac" => Ok(json!({"bivouac": {}})),
        "tent" => Ok(json!({"tent": {}})),
        _ => Err("Shelter must be bivouac or tent"),
    }
}

pub(crate) fn settlement_action_service_argument(
    service: adventuresim_world_schema::SettlementActionService,
) -> serde_json::Value {
    match service {
        adventuresim_world_schema::SettlementActionService::Inn => json!({ "inn": {} }),
        adventuresim_world_schema::SettlementActionService::Temple => json!({ "temple": {} }),
    }
}

pub(super) fn deserialize_optional_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.parse().map_err(serde::de::Error::custom))
        .transpose()
}

pub(super) use adventuresim_core::strategic_time::MAX_SETTLEMENT_REST_MINUTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RestDurationError {
    SettlementBelowMinimum,
    SettlementAboveMaximum,
    TravelBelowMinimum,
    TravelAboveMaximum,
    MinutesNotWhole,
    ClockFormat,
    ClockMinuteOutOfRange,
    DurationOverflow,
    WakeTimeMismatch,
    DaysNotWhole,
    UnknownUnit,
}

impl std::fmt::Display for RestDurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use adventuresim_core::strategic_time::DAYS_PER_YEAR;

        match self {
            Self::SettlementBelowMinimum => {
                formatter.write_str("Settlement rest must last at least one day")
            }
            Self::SettlementAboveMaximum => write!(
                formatter,
                "Settlement rest cannot exceed {DAYS_PER_YEAR} days"
            ),
            Self::TravelBelowMinimum => formatter.write_str("Rest must last at least one minute"),
            Self::TravelAboveMaximum => {
                write!(formatter, "Rest cannot exceed {DAYS_PER_YEAR} days")
            }
            Self::MinutesNotWhole => formatter.write_str("Rest minutes must be a whole number"),
            Self::ClockFormat => formatter.write_str("Rest duration must use HH:MM"),
            Self::ClockMinuteOutOfRange => {
                formatter.write_str("Rest duration minutes must be between 00 and 59")
            }
            Self::DurationOverflow => formatter.write_str("Rest duration is too large"),
            Self::WakeTimeMismatch => {
                formatter.write_str("Rest duration does not match the selected wake time")
            }
            Self::DaysNotWhole => formatter.write_str("Rest days must be a whole number"),
            Self::UnknownUnit => formatter.write_str("Unknown rest duration unit"),
        }
    }
}

impl std::error::Error for RestDurationError {}

pub(super) fn settlement_rest_minutes(form: &RestForm) -> Result<u64, RestDurationError> {
    let minutes = parsed_rest_minutes(form)?;
    if minutes < adventuresim_core::strategic_time::MINUTES_PER_DAY {
        return Err(RestDurationError::SettlementBelowMinimum);
    }
    if minutes > MAX_SETTLEMENT_REST_MINUTES {
        return Err(RestDurationError::SettlementAboveMaximum);
    }
    Ok(minutes)
}

pub(crate) fn travel_rest_minutes(form: &RestForm) -> Result<u64, RestDurationError> {
    let minutes = parsed_rest_minutes(form)?;
    if minutes == 0 {
        return Err(RestDurationError::TravelBelowMinimum);
    }
    if minutes > MAX_SETTLEMENT_REST_MINUTES {
        return Err(RestDurationError::TravelAboveMaximum);
    }
    Ok(minutes)
}

pub(super) fn parsed_rest_minutes(form: &RestForm) -> Result<u64, RestDurationError> {
    Ok(match form.unit.as_str() {
        "minutes" => form
            .duration
            .parse::<u64>()
            .map_err(|_| RestDurationError::MinutesNotWhole)?,
        "hours" => {
            let (hours, minutes) = form
                .duration
                .split_once(':')
                .ok_or(RestDurationError::ClockFormat)?;
            if minutes.len() != 2 || !minutes.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(RestDurationError::ClockFormat);
            }
            let hours = hours
                .parse::<u64>()
                .map_err(|_| RestDurationError::ClockFormat)?;
            let minutes = minutes
                .parse::<u64>()
                .map_err(|_| RestDurationError::ClockFormat)?;
            if minutes >= 60 {
                return Err(RestDurationError::ClockMinuteOutOfRange);
            }
            let duration_minutes = hours
                .checked_mul(60)
                .and_then(|value| value.checked_add(minutes))
                .ok_or(RestDurationError::DurationOverflow)?;
            if let Some(requested_minutes) = form.requested_minutes
                && requested_minutes != duration_minutes
            {
                return Err(RestDurationError::WakeTimeMismatch);
            }
            form.requested_minutes.unwrap_or(duration_minutes)
        }
        "days" => {
            let days = form
                .duration
                .parse::<u64>()
                .map_err(|_| RestDurationError::DaysNotWhole)?;
            days.saturating_mul(adventuresim_core::strategic_time::MINUTES_PER_DAY)
        }
        _ => return Err(RestDurationError::UnknownUnit),
    })
}

pub(super) fn safe_rest_error(_error: &str) -> &'static str {
    "The rest could not be completed. Review the duration and try again."
}

pub(super) async fn rest(
    State(state): State<AppState>,
    Path((id, kind)): Path<(String, String)>,
    session: Session,
    form: Result<Form<RestForm>, FormRejection>,
) -> Response {
    let Some(service_kind) = RestServiceKind::parse(&kind) else {
        return Html("<h1>Rest service not found</h1>".to_string()).into_response();
    };
    let public_service = service_kind.public_service();
    let Some(character_id) = session.character_id_u64() else {
        return Html("<h1>Choose a character first</h1>".to_string()).into_response();
    };
    let form = match form {
        Ok(Form(form)) => form,
        Err(error) => {
            tracing::warn!(
                character_id,
                requested_settlement_id_length = id.len(),
                service = service_kind.tag(),
                rejection_status = %error.status(),
                error = %error,
                "settlement rest form extraction rejected request"
            );
            return error.into_response();
        }
    };
    let settlement_query = settlement_by_id(&id);
    let settlements: Vec<SettlementView> = state
        .db
        .query_sats_into::<adventuresim_stdb_client::Settlement, SettlementView>(
            settlement_query.as_str(),
        )
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string()).into_response();
    };
    if public_service
        .is_some_and(|service| !settlement_action_service_available(&settlement.economy, service))
    {
        return Html("<h1>Rest service unavailable</h1>".to_string()).into_response();
    }
    let requested_minutes = match settlement_rest_minutes(&form) {
        Ok(minutes) => minutes,
        Err(error) => {
            let message = error.to_string();
            let unit = match form.unit.as_str() {
                "hours" => "hours",
                "days" => "days",
                _ => "unknown",
            };
            tracing::warn!(
                character_id,
                requested_settlement_id = %id,
                requested_minutes = ?form.requested_minutes,
                public_service = ?public_service,
                route_service = service_kind.tag(),
                unit,
                duration_length = form.duration.len(),
                reason = message.as_str(),
                "settlement rest duration validation rejected request"
            );
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Html(
                    crate::templates::strategic_notice_page(
                        "Unable to rest",
                        &message,
                        &format!("/settlements/{id}/{}", service_kind.page_path()),
                        "Return to rest service",
                        None,
                    )
                    .into_string(),
                ),
            )
                .into_response();
        }
    };
    let before_character = get_active_character(&state, Some(character_id)).await;
    let before_limbs = query_single::<CharacterLimbs>(
        &state,
        crate::spacetimedb::character_limbs_by_character_id(character_id),
    )
    .await;
    let before_skills = query_single::<CharacterSkills>(
        &state,
        crate::spacetimedb::character_skills_by_character_id(character_id),
    )
    .await;
    let before_time = query_single::<crate::spacetimedb::CharacterTime>(
        &state,
        crate::spacetimedb::character_time_by_character_id(character_id),
    )
    .await;
    let before_reputation = query_local_reputation(&state, character_id, &id).await;
    let character_settlement_id = before_character
        .as_ref()
        .and_then(|(character, _)| character.current_settlement_id.as_deref())
        .unwrap_or("<none>");
    let reducer = match service_kind {
        RestServiceKind::Inn | RestServiceKind::Temple => "rest_at_settlement_hours",
        RestServiceKind::Residence => "rest_at_residence_hours",
    };
    let reducer_arguments = match public_service {
        Some(service) => vec![
            json!(character_id),
            json!(requested_minutes),
            settlement_action_service_argument(service),
        ],
        None => vec![json!(character_id), json!(requested_minutes)],
    };
    if let Err(error) = state.db.call(reducer, &reducer_arguments).await {
        tracing::warn!(
            character_id,
            requested_settlement_id = %id,
            character_settlement_id,
            requested_minutes,
            public_service = ?public_service,
            route_service = service_kind.tag(),
            error = %error,
            "settlement rest reducer rejected request"
        );
        return (
            StatusCode::BAD_REQUEST,
            Html(
                crate::templates::strategic_notice_page(
                    "Unable to rest",
                    safe_rest_error(&error.to_string()),
                    &format!("/settlements/{id}/{}", service_kind.page_path()),
                    "Return to rest service",
                    None,
                )
                .into_string(),
            ),
        )
            .into_response();
    }

    let active_character = get_active_character(&state, Some(character_id)).await;
    if let Some(case_site_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.current_case_site_id.as_deref())
    {
        return Redirect::to(&format!("/locations/case-site/{case_site_id}")).into_response();
    }
    let party_members = get_active_party_members(
        &state,
        active_character.as_ref().map(|(character, _)| character),
    )
    .await;
    let after_limbs = query_single::<CharacterLimbs>(
        &state,
        crate::spacetimedb::character_limbs_by_character_id(character_id),
    )
    .await;
    let after_skills = query_single::<CharacterSkills>(
        &state,
        crate::spacetimedb::character_skills_by_character_id(character_id),
    )
    .await;
    let after_time = query_single::<crate::spacetimedb::CharacterTime>(
        &state,
        crate::spacetimedb::character_time_by_character_id(character_id),
    )
    .await;
    let after_reputation = query_local_reputation(&state, character_id, &id).await;
    let summary = rest_summary(RestSummaryObservation {
        before_inventory: before_character
            .as_ref()
            .map_or(&[], |(_, inventory)| inventory.as_slice()),
        after_inventory: active_character
            .as_ref()
            .map_or(&[], |(_, inventory)| inventory.as_slice()),
        before_limbs: before_limbs.as_ref(),
        after_limbs: after_limbs.as_ref(),
        before_skills: before_skills.as_ref(),
        after_skills: after_skills.as_ref(),
        before_time: before_time.as_ref(),
        after_time: after_time.as_ref(),
        before_reputation: before_reputation.as_ref(),
        after_reputation: after_reputation.as_ref(),
        public_service,
        requested_minutes,
    });
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let items = state
        .db
        .query_sats_into::<adventuresim_stdb_client::Item, CatalogItemView>("SELECT * FROM item")
        .await
        .unwrap_or_default();
    let food_lots = state
        .db
        .query_sats::<FoodLot>("SELECT * FROM food_lot")
        .await
        .unwrap_or_default();
    let soap_preview = soap_rest_preview(
        &state,
        active_character
            .as_ref()
            .map_or(&[][..], |(character, _)| std::slice::from_ref(character)),
        active_character
            .as_ref()
            .and_then(|(character, _)| character.party_id.as_deref()),
    )
    .await;
    Html(
        rest_result_page(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            active_character
                .as_ref()
                .map_or(&[], |(_, inventory)| inventory.as_slice()),
            &items,
            &food_lots,
            &party_members,
            logged_in_as.as_deref(),
            public_service,
            &summary,
            soap_preview,
        )
        .into_string(),
    )
    .into_response()
}

pub(super) async fn query_single<T: spacetimedb_sats::de::DeserializeOwned>(
    state: &AppState,
    query: crate::spacetimedb::SqlQuery,
) -> Option<T> {
    state.db.query_one_sats(&query).await.ok().flatten()
}

pub(super) async fn query_local_reputation(
    state: &AppState,
    character_id: u64,
    settlement_id: &str,
) -> Option<CharacterSettlementReputation> {
    state
        .db
        .query_sats(&format!(
            "SELECT * FROM character_settlement_reputation WHERE character_id = {character_id} AND settlement_id = {}",
            crate::spacetimedb::sql_string_literal(settlement_id)
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
}

pub(super) struct RestSummaryObservation<'a> {
    before_inventory: &'a [InventoryItem],
    after_inventory: &'a [InventoryItem],
    before_limbs: Option<&'a CharacterLimbs>,
    after_limbs: Option<&'a CharacterLimbs>,
    before_skills: Option<&'a CharacterSkills>,
    after_skills: Option<&'a CharacterSkills>,
    before_time: Option<&'a crate::spacetimedb::CharacterTime>,
    after_time: Option<&'a crate::spacetimedb::CharacterTime>,
    before_reputation: Option<&'a CharacterSettlementReputation>,
    after_reputation: Option<&'a CharacterSettlementReputation>,
    public_service: Option<adventuresim_world_schema::SettlementActionService>,
    requested_minutes: u64,
}

pub(super) fn rest_summary(observation: RestSummaryObservation<'_>) -> RestSummary {
    let RestSummaryObservation {
        before_inventory,
        after_inventory,
        before_limbs,
        after_limbs,
        before_skills,
        after_skills,
        before_time,
        after_time,
        before_reputation,
        after_reputation,
        public_service,
        requested_minutes,
    } = observation;
    let minutes = before_time.zip(after_time).map_or(0, |(before, after)| {
        after.minutes.saturating_sub(before.minutes)
    });
    let currency_total = |inventory: &[InventoryItem]| -> u32 {
        inventory
            .iter()
            .filter(|item| adventuresim_core::strategic_currency::is_currency_id(&item.item_id))
            .map(|item| item.quantity)
            .sum()
    };
    let before_currency = currency_total(before_inventory);
    let after_currency = currency_total(after_inventory);
    let gold_spent = before_currency.saturating_sub(after_currency);
    let (full_board_gold_spent, additional_gold_spent) =
        rest_spending_breakdown(gold_spent, public_service, requested_minutes);
    let gold_earned = after_currency.saturating_sub(before_currency);
    let fame_gained = after_reputation.map_or(0.0, |after| {
        (after.fame - before_reputation.map_or(0, |before| before.fame)) as f32 / 100.0
    });
    let infamy_gained = after_reputation.map_or(0.0, |after| {
        (after.infamy - before_reputation.map_or(0, |before| before.infamy)) as f32 / 100.0
    });
    let healed = match (before_limbs, after_limbs) {
        (Some(before), Some(after)) => limb_deltas(before, after),
        _ => vec![],
    };
    let trained = match (before_skills, after_skills) {
        (Some(before), Some(after)) => skill_deltas(before, after),
        _ => vec![],
    };
    RestSummary {
        minutes,
        full_board_gold_spent,
        additional_gold_spent,
        gold_earned,
        fame_gained,
        infamy_gained,
        healed,
        trained,
    }
}

pub(super) fn rest_spending_breakdown(
    total_gold_spent: u32,
    public_service: Option<adventuresim_world_schema::SettlementActionService>,
    requested_minutes: u64,
) -> (u32, u32) {
    let full_board = if matches!(
        public_service,
        Some(adventuresim_world_schema::SettlementActionService::Inn)
    ) {
        adventuresim_core::strategic_economy::inn_full_board_cost(requested_minutes)
            .and_then(|cost| u32::try_from(cost).ok())
            .unwrap_or(u32::MAX)
    } else {
        0
    };
    (full_board, total_gold_spent.saturating_sub(full_board))
}

pub(super) fn limb_deltas(before: &CharacterLimbs, after: &CharacterLimbs) -> Vec<(String, f32)> {
    [
        ("Left arm", before.left_arm_health, after.left_arm_health),
        ("Right arm", before.right_arm_health, after.right_arm_health),
        ("Left leg", before.left_leg_health, after.left_leg_health),
        ("Right leg", before.right_leg_health, after.right_leg_health),
        ("Head", before.head_health, after.head_health),
        ("Chest", before.chest_health, after.chest_health),
        ("Stomach", before.stomach_health, after.stomach_health),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = (after - before) * 100.0;
        (delta > 0.01).then(|| (name.to_string(), delta))
    })
    .collect()
}

pub(super) fn skill_deltas(
    before: &CharacterSkills,
    after: &CharacterSkills,
) -> Vec<(String, f32)> {
    [
        ("Polearm", before.polearm_hours, after.polearm_hours),
        ("Axe", before.axe_hours, after.axe_hours),
        ("Bludgeon", before.bludgeon_hours, after.bludgeon_hours),
        ("Sword", before.sword_hours, after.sword_hours),
        ("Knife", before.knife_hours, after.knife_hours),
        ("Dodge", before.dodge_hours, after.dodge_hours),
        ("Block", before.block_hours, after.block_hours),
        ("Bow", before.bow_hours, after.bow_hours),
        ("Crossbow", before.crossbow_hours, after.crossbow_hours),
        ("Firearm", before.firearm_hours, after.firearm_hours),
        ("Throw", before.throw_hours, after.throw_hours),
        ("Will", before.will_hours, after.will_hours),
        ("Insight", before.insight_hours, after.insight_hours),
        ("Charm", before.charm_hours, after.charm_hours),
        ("Command", before.command_hours, after.command_hours),
        ("Deception", before.deception_hours, after.deception_hours),
        (
            "Physiology",
            before.physiology_hours,
            after.physiology_hours,
        ),
        ("Cooking", before.cooking_hours, after.cooking_hours),
        ("Herbalism", before.herbalism_hours, after.herbalism_hours),
        (
            "Religion",
            before.religion_hours.total_direct(),
            after.religion_hours.total_direct(),
        ),
        (
            "Bestiary",
            before.bestiary_hours.total_direct(),
            after.bestiary_hours.total_direct(),
        ),
        ("Stealth", before.stealth_hours, after.stealth_hours),
        ("Balance", before.balance_hours, after.balance_hours),
        ("Surgery", before.surgery_hours, after.surgery_hours),
        ("Tailoring", before.tailoring_hours, after.tailoring_hours),
        ("Smithing", before.smithing_hours, after.smithing_hours),
    ]
    .into_iter()
    .filter_map(|(name, before, after)| {
        let delta = after - before;
        (delta > 0.001).then(|| (name.to_string(), delta))
    })
    .collect()
}

#[cfg(test)]
mod surgery_skill_delta_tests {
    use super::skill_deltas;
    use crate::spacetimedb::CharacterSkills;

    #[test]
    fn surgery_training_is_reported_as_a_leaf_skill_delta() {
        let before = CharacterSkills {
            surgery_hours: 12.0,
            ..crate::spacetimedb::generated_character_skills_fixture()
        };
        let after = CharacterSkills {
            surgery_hours: 13.5,
            ..crate::spacetimedb::generated_character_skills_fixture()
        };
        assert_eq!(skill_deltas(&before, &after), vec![("Surgery".into(), 1.5)]);
    }
}

pub(super) async fn travel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(_form): Form<TravelForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };

    let outcome = super::execute_or_request_party_action(
        &state,
        character_id,
        super::PartyAction::TravelToSettlement {
            settlement_id: id.clone(),
        },
    )
    .await;
    match outcome {
        // The live navigation stream routes every party member after the
        // reducer's committed state is visible.
        Ok(super::PartyActionOutcome::Executed) => StatusCode::NO_CONTENT.into_response(),
        Ok(super::PartyActionOutcome::Requested) => StatusCode::ACCEPTED.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
}

pub(super) async fn weapons(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Weapons).await
}

#[derive(Deserialize)]
pub(super) struct ForgeWeaponForm {
    recipe: String,
}

pub(super) async fn forge_weapon(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<ForgeWeaponForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters").into_response();
    };
    let recipe = match form
        .recipe
        .split(',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(recipe) if !recipe.is_empty() => recipe,
        _ => return (StatusCode::BAD_REQUEST, "Invalid weapon recipe").into_response(),
    };
    match state
        .db
        .call(
            "forge_weapon",
            &[json!(character_id), json!(id), json!(recipe)],
        )
        .await
    {
        Ok(()) => Redirect::to(&format!("/settlements/{id}/weapons")).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn armor(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Armor).await
}

pub(super) async fn clothing(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Clothing).await
}

pub(super) async fn herbalist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Herbalist).await
}

pub(super) async fn bookstore(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    merchant_shop(state, id, session, MerchantShop::Books).await
}

pub(super) async fn purchase_from_herbalist(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
    Form(form): Form<MerchantOfferForm>,
) -> Redirect {
    let fallback = format!("/settlements/{id}/herbalist");
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Redirect::to("/characters");
    };
    let mut purchase_completed = false;
    // Prepared courses are individual equipment-like items, so this storefront
    // intentionally has no party-scope purchase path.
    if form.inventory_scope == "player"
        && let Ok(buys) = form.buys()
        && !buys.is_empty()
    {
        let (items, quantities): (Vec<_>, Vec<_>) = buys
            .into_iter()
            .map(|entry| (entry.id, entry.quantity))
            .unzip();
        match state
            .db
            .call(
                "purchase_from_herbalist",
                &[
                    json!(character.id),
                    json!(id),
                    json!(items),
                    json!(quantities),
                ],
            )
            .await
        {
            Ok(()) => purchase_completed = true,
            Err(error) => {
                tracing::warn!(%error, character_id = character.id, "herbalist purchase rejected");
            }
        }
    }
    if purchase_completed {
        redirect_to_local(&form.return_to, &fallback)
    } else {
        Redirect::to(&fallback)
    }
}

pub(super) async fn religion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    render_service_page(
        state,
        id,
        session,
        adventuresim_world_schema::SettlementActionService::Temple,
        religion_page,
    )
    .await
}

pub(super) fn settlement_action_service_available(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    service: adventuresim_world_schema::SettlementActionService,
) -> bool {
    adventuresim_core::settlement_economy::action_service_available(profile, service)
}

#[cfg(test)]
mod service_availability_tests {
    use super::{SETTLEMENTS_SOURCE, settlement_action_service_available};
    use adventuresim_core::settlement_economy::{player_visible_npc_tabs, visible_npc_tab};
    use adventuresim_world_schema::{
        SettlementActionService, SettlementEconomyProfile, SettlementService,
    };

    #[test]
    fn direct_routes_reject_unadvertised_church_inn_and_armoury() {
        let mut profile = SettlementEconomyProfile::stage_placeholder();
        profile.services.clear();
        assert!(!settlement_action_service_available(
            &profile,
            SettlementActionService::Temple
        ));
        assert!(!settlement_action_service_available(
            &profile,
            SettlementActionService::Inn
        ));
        let tabs = player_visible_npc_tabs(&profile, false, "fixture-no-orgs");
        assert!(visible_npc_tab(&tabs, "church").is_none());
        assert!(visible_npc_tab(&tabs, "inn").is_none());
        assert!(visible_npc_tab(&tabs, "armoury").is_none());

        profile.services = vec![SettlementService::Inn, SettlementService::Temple];
        profile.services.sort();
        assert!(settlement_action_service_available(
            &profile,
            SettlementActionService::Temple
        ));
        assert!(settlement_action_service_available(
            &profile,
            SettlementActionService::Inn
        ));
    }

    #[test]
    fn organization_management_page_and_business_routes_are_removed() {
        let source = SETTLEMENTS_SOURCE;
        let routes = source
            .split("pub fn routes()")
            .nth(1)
            .and_then(|tail| tail.split("#[derive").next())
            .expect("settlement router");
        let production = source
            .split("mod service_availability_tests")
            .next()
            .expect("production settlement routes");
        assert!(!routes.contains("character_organizations"));
        assert!(!production.contains("fn character_organizations"));
        assert!(!routes.contains("/organizations/{organization_id}/{action}"));
        assert!(!production.contains("\"join\" => \"join_organization\""));
        assert!(!production.contains("\"pay\" => \"pay_organization_dues\""));
        assert!(!production.contains("\"promote\" => \"promote_organization_membership\""));
        assert!(routes.contains("organization-presentation/{organization_id}"));
        assert!(production.contains("organization_chapter_at(&id, &place)"));
    }

    #[test]
    fn service_apprenticeship_fallback_defers_to_colocated_representatives() {
        let source = SETTLEMENTS_SOURCE;
        let handler = source
            .split("async fn begin_service_apprenticeship")
            .nth(1)
            .and_then(|tail| {
                tail.split("async fn update_organization_presentation")
                    .next()
            })
            .expect("service apprenticeship handler");
        assert!(handler.contains("organizations_for_chapter(&id)"));
        assert!(handler.contains("organization.service_id"));
        assert!(handler.contains("exact_apprenticeship_representative_present"));
        assert!(handler.contains("settlement_resident_presence"));
        assert!(handler.contains("chapter_effective_location_id"));
        assert!(handler.contains("Speak to the local organization representative"));
        assert!(handler.contains("\"join_organization\""));
        for forbidden in [
            "pay_organization_dues",
            "promote_organization_membership",
            "present_organization",
        ] {
            assert!(!handler.contains(forbidden));
        }
    }
}
