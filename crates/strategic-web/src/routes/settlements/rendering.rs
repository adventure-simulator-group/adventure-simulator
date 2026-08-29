pub(super) type ServiceRenderer = fn(
    &SettlementView,
    Option<&CharacterView>,
    &[InventoryItem],
    &[CatalogItemView],
    &[FoodLot],
    &[CharacterView],
    Option<&CharacterLimbs>,
    Option<&CharacterStats>,
    Option<&CharacterCondition>,
    u64,
    u64,
    SoapRestPreview,
    Option<&str>,
) -> maud::Markup;

pub(super) async fn merchant_shop(
    state: AppState,
    id: String,
    session: Session,
    shop: MerchantShop,
) -> Html<String> {
    let settlement_literal = sql_string_literal(&id);
    let settlement_sql = settlement_by_id(&id);
    let (settlements, active_character) = tokio::join!(
        state.db.query_sats_into::<adventuresim_stdb_client::Settlement, SettlementView>(settlement_sql.as_str()),
        get_active_character(&state, session.character_id_u64()),
    );
    let settlements = settlements.unwrap_or_default();
    let Some(settlement) = settlements.first() else {
        return Html("<h1>Settlement not found</h1>".to_string());
    };
    if !shop.available_at(settlement) {
        return Html(
            crate::templates::strategic_notice_page(
                "Service unavailable",
                "This settlement does not offer that service.",
                &format!("/locations/settlement/{}", settlement.id),
                "Return to settlement",
                None,
            )
            .into_string(),
        );
    }
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());
    let Some((character, inventory)) = active_character.as_ref() else {
        let party_members = get_active_party_members(&state, None).await;
        return Html(
            merchants_page(
                settlement,
                None,
                &[],
                &[],
                &party_members,
                logged_in_as.as_deref(),
            )
            .into_string(),
        );
    };
    let condition_sql = "SELECT * FROM item_condition".to_string();
    let smith_sql = crate::spacetimedb::settlement_smith_by_settlement_id(&id);
    let order_sql = format!(
        "SELECT * FROM repair_order WHERE owner_character_id = {} AND settlement_id = {settlement_literal}",
        character.id
    );
    let time_sql = crate::spacetimedb::character_time_by_character_id(character.id);
    let consequence_sql = format!(
        "SELECT * FROM backend_local_problem_trade_effects WHERE character_id = {}",
        character.id
    );
    let amount_sql = "SELECT * FROM inventory_item_amount";
    let (
        party_members,
        items,
        food_lots,
        equip,
        trade_context,
        conditions,
        smiths,
        orders,
        times,
        consequences,
        personal_amounts,
    ) = tokio::join!(
        get_active_party_members(&state, Some(character)),
        state.db.query_sats_into::<adventuresim_stdb_client::Item, CatalogItemView>("SELECT * FROM item"),
        state.db.query_sats::<FoodLot>("SELECT * FROM food_lot"),
        party::character_equipment_graph(&state, character.id),
        inventory_trade_context(&state, character),
        state.db.query_sats::<ItemCondition>(&condition_sql),
        state.db.query_sats::<SettlementSmith>(&smith_sql),
        state.db.query_sats::<RepairOrder>(&order_sql),
        state.db.query_sats::<CharacterTime>(&time_sql),
        state
            .db
            .query_sats::<BackendLocalProblemTradeEffect>(&consequence_sql),
        state.db.query_sats::<InventoryItemAmount>(amount_sql),
    );
    let items = items.unwrap_or_default();
    let (personal_targets, party_targets, pooled) = trade_context;
    let encumbrance = inventory_encumbrance_summaries(
        &state,
        character,
        inventory,
        &party_members,
        &pooled,
        &items,
        !matches!(shop, MerchantShop::Herbalist),
    )
    .await;
    let (inn_rest_default, inn_soap_preview) = if matches!(shop, MerchantShop::Inn) {
        let (limbs, stats, condition) = tokio::join!(
            query_single::<CharacterLimbs>(
                &state,
                crate::spacetimedb::character_limbs_by_character_id(character.id),
            ),
            query_single::<CharacterStats>(
                &state,
                crate::spacetimedb::character_stats_by_character_id(character.id),
            ),
            query_single::<CharacterCondition>(
                &state,
                crate::spacetimedb::character_condition_by_character_id(character.id),
            ),
        );
        let (field_repair_minutes, smith_wait_minutes) =
            equipment_rest_recommendation(&state, character.id, &id, inventory).await;
        let soap = soap_rest_preview(
            &state,
            std::slice::from_ref(character),
            character.party_id.as_deref(),
        )
        .await;
        (
            rest_default_minutes(
                limbs.as_ref(),
                stats.as_ref(),
                condition.as_ref(),
                field_repair_minutes,
                smith_wait_minutes,
            ),
            soap,
        )
    } else {
        (None, SoapRestPreview::default())
    };
    let speaker = query_single::<CharacterSkills>(
        &state,
        crate::spacetimedb::character_skills_by_character_id(character.id),
    )
    .await
    .map_or_else(adventuresim_world_schema::OralLanguageHours::default, |skills| {
        crate::spacetimedb::core_oral_language_hours(&skills.oral_languages)
    });
    let speaker_cap = query_single::<CharacterAttributes>(
        &state,
        crate::spacetimedb::character_attributes_by_character_id(character.id),
    )
    .await
    .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    let mut merchant_languages = adventuresim_world_schema::OralLanguageHours::default();
    *merchant_languages.direct_mut(settlement.languages.dominant_german()) =
        adventuresim_world_schema::ORAL_FLUENCY_HOURS;
    let (_, shared_language) = adventuresim_world_schema::best_common_oral_language_capped(
        speaker,
        speaker_cap,
        merchant_languages,
        adventuresim_world_schema::ORAL_FLUENCY_HOURS,
    );
    let now_minutes = times
        .as_ref()
        .ok()
        .and_then(|rows| rows.first())
        .map_or(0, |time| time.minutes);
    let problem_effects = consequences
        .unwrap_or_default()
        .into_iter()
        .find(|row| row.character_id == character.id && row.settlement_id == id)
        .unwrap_or(BackendLocalProblemTradeEffect {
            character_id: character.id,
            settlement_id: id.clone(),
            buy_bps: 0,
            sell_penalty_bps: 0,
        });
    Html(
        live_merchant_shop_page(
            settlement,
            character,
            inventory,
            &personal_amounts.unwrap_or_default(),
            &items,
            &food_lots.unwrap_or_default(),
            &party_members,
            equip.first(),
            &personal_targets,
            &party_targets,
            &pooled,
            shop,
            shared_language,
            problem_effects.buy_bps,
            problem_effects.sell_penalty_bps,
            &conditions.unwrap_or_default(),
            smiths.unwrap_or_default().first(),
            &orders.unwrap_or_default(),
            now_minutes,
            encumbrance.personal,
            encumbrance.party,
            inn_rest_default,
            inn_soap_preview,
        )
        .into_string(),
    )
}

pub(super) async fn inventory_trade_context(
    state: &AppState,
    character: &CharacterView,
) -> (
    Vec<InventoryQuantityTarget>,
    Vec<InventoryQuantityTarget>,
    Vec<PartyInventoryItem>,
) {
    let personal_sql = format!(
        "SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = false",
        character.id
    );
    let Some(party_id) = character.party_id.as_ref() else {
        let personal = state.db.query_sats(&personal_sql).await.unwrap_or_default();
        return (personal, Vec::new(), Vec::new());
    };
    let party_sql = crate::spacetimedb::party_by_id(party_id);
    let (personal, party) = tokio::join!(
        state.db.query_sats(&personal_sql),
        state.db.query_sats_into::<adventuresim_stdb_client::Party, PartyView>(&party_sql),
    );
    let personal = personal.unwrap_or_default();
    let party = party.unwrap_or_default().into_iter().next();
    let Some(party) = party else {
        return (personal, Vec::new(), Vec::new());
    };
    let party_targets_sql = format!(
        "SELECT * FROM inventory_quantity_target WHERE owner_character_id = {} AND party_scope = true",
        party.leader_id
    );
    let pooled_sql = format!(
        "SELECT * FROM party_inventory_item WHERE party_id = {}",
        sql_string_literal(party_id)
    );
    let (party_targets, pooled) = tokio::join!(
        state.db.query_sats(&party_targets_sql),
        state.db.query_sats(&pooled_sql),
    );
    (
        personal,
        party_targets.unwrap_or_default(),
        pooled.unwrap_or_default(),
    )
}

pub(super) async fn personal_inventory_targets(
    state: &AppState,
    character_id: u64,
) -> Vec<InventoryQuantityTarget> {
    state.db.query_sats(&format!("SELECT * FROM inventory_quantity_target WHERE owner_character_id = {character_id} AND party_scope = false")).await.unwrap_or_default()
}

pub(super) async fn render_service_page(
    state: AppState,
    id: String,
    session: Session,
    required_service: adventuresim_world_schema::SettlementActionService,
    render: ServiceRenderer,
) -> Html<String> {
    let settlement_sql = crate::spacetimedb::settlement_by_id(&id);
    let (settlements, active_character) = tokio::join!(
        state.db.query_sats_into::<adventuresim_stdb_client::Settlement, SettlementView>(&settlement_sql),
        get_active_character(&state, session.character_id_u64()),
    );
    let settlements = settlements.unwrap_or_default();
    let settlement = match settlements.first() {
        Some(settlement) => settlement,
        None => return Html("<h1>Settlement not found</h1>".to_string()),
    };
    if !settlement_action_service_available(&settlement.economy, required_service) {
        return Html(
            crate::templates::strategic_notice_page(
                "Service unavailable",
                "This settlement does not offer that service.",
                &format!("/locations/settlement/{}", settlement.id),
                "Return to settlement",
                None,
            )
            .into_string(),
        );
    }

    let active_character_ref = active_character.as_ref().map(|(character, _)| character);
    let limbs_lookup = async {
        match active_character_ref {
            Some(character) => {
                query_single::<CharacterLimbs>(
                    &state,
                    crate::spacetimedb::character_limbs_by_character_id(character.id),
                )
                .await
            }
            None => None,
        }
    };
    let stats_lookup = async {
        match active_character_ref {
            Some(character) => {
                query_single::<CharacterStats>(
                    &state,
                    crate::spacetimedb::character_stats_by_character_id(character.id),
                )
                .await
            }
            None => None,
        }
    };
    let condition_lookup = async {
        match active_character_ref {
            Some(character) => {
                query_single::<CharacterCondition>(
                    &state,
                    crate::spacetimedb::character_condition_by_character_id(character.id),
                )
                .await
            }
            None => None,
        }
    };
    let equipment_lookup = async {
        match active_character.as_ref() {
            Some((character, inventory)) => {
                equipment_rest_recommendation(&state, character.id, &id, inventory).await
            }
            None => (0, 0),
        }
    };
    let (party_members, items, food_lots, limbs, stats, condition, equipment_recovery) = tokio::join!(
        get_active_party_members(&state, active_character_ref),
        state.db.query_sats_into::<adventuresim_stdb_client::Item, CatalogItemView>("SELECT * FROM item"),
        state.db.query_sats::<FoodLot>("SELECT * FROM food_lot"),
        limbs_lookup,
        stats_lookup,
        condition_lookup,
        equipment_lookup,
    );
    let soap_preview = soap_rest_preview(
        &state,
        active_character_ref.map_or(&[][..], std::slice::from_ref),
        active_character_ref.and_then(|character| character.party_id.as_deref()),
    )
    .await;
    let logged_in_as = active_character
        .as_ref()
        .map(|(character, _)| character.name.clone());

    let inventory = active_character
        .as_ref()
        .map_or_else(Vec::new, |(_, inventory)| inventory.clone());
    Html(
        render(
            settlement,
            active_character.as_ref().map(|(character, _)| character),
            &inventory,
            &items.unwrap_or_default(),
            &food_lots.unwrap_or_default(),
            &party_members,
            limbs.as_ref(),
            stats.as_ref(),
            condition.as_ref(),
            equipment_recovery.0,
            equipment_recovery.1,
            soap_preview,
            logged_in_as.as_deref(),
        )
        .into_string(),
    )
}

pub(super) async fn equipment_rest_recommendation(
    state: &AppState,
    character_id: u64,
    settlement_id: &str,
    inventory: &[InventoryItem],
) -> (u64, u64) {
    let skills_sql = crate::spacetimedb::character_skills_by_character_id(character_id);
    let attributes_sql = crate::spacetimedb::character_attributes_by_character_id(character_id);
    let settlement_literal = sql_string_literal(settlement_id);
    let orders_sql = format!(
        "SELECT * FROM repair_order WHERE owner_character_id = {character_id} AND settlement_id = {settlement_literal}"
    );
    let time_sql = crate::spacetimedb::character_time_by_character_id(character_id);
    let (conditions, skills, attributes, orders, times) = tokio::join!(
        state
            .db
            .query_sats::<ItemCondition>("SELECT * FROM item_condition"),
        state.db.query_sats::<CharacterSkills>(&skills_sql),
        state.db.query_sats::<CharacterAttributes>(&attributes_sql),
        state.db.query_sats::<RepairOrder>(&orders_sql),
        state.db.query_sats::<CharacterTime>(&time_sql),
    );
    let skills = skills.unwrap_or_default();
    let attributes = attributes.unwrap_or_default();
    let skill = skills
        .first()
        .zip(attributes.first())
        .map(|(skills, attributes)| {
            let arm_agility = (attributes.left_arm_agility + attributes.right_arm_agility) * 0.5;
            Skill::Smithing
                .capped_rank_for_aptitude(skills.smithing_hours, arm_agility)
                .floor() as u8
        })
        .unwrap_or_default()
        .min(2);
    let owned: std::collections::HashSet<u64> = inventory.iter().map(|item| item.id).collect();
    let yellow: f32 = conditions
        .unwrap_or_default()
        .iter()
        .filter(|condition| owned.contains(&condition.inventory_item_id))
        .map(|condition| condition.bins().iter().take(skill as usize).sum::<f32>())
        .sum();
    let field_minutes = (yellow * 2_880.0).ceil() as u64;
    let now = times
        .unwrap_or_default()
        .first()
        .map_or(0, |time| time.minutes);
    let smith_wait = orders
        .unwrap_or_default()
        .iter()
        .map(|order| order.ready_at_minutes.saturating_sub(now))
        .max()
        .unwrap_or(0);
    (field_minutes, smith_wait)
}
