#[derive(Deserialize)]
pub(super) struct InventoryTargetForm {
    item_id: String,
    quantity: u32,
    #[serde(default)]
    party_scope: bool,
}

pub(super) async fn set_inventory_target(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<InventoryTargetForm>,
) -> impl IntoResponse {
    let Some(character_id) = session.character_id_u64() else {
        return (axum::http::StatusCode::UNAUTHORIZED, "Choose a character");
    };
    let args = vec![
        json!(character_id),
        json!(form.party_scope),
        json!(form.item_id),
        json!(form.quantity),
    ];
    let result = if form.party_scope {
        super::execute_or_request_party_action(
            &state,
            character_id,
            super::PartyAction::SetInventoryQuantityTarget {
                item_id: form.item_id,
                quantity: form.quantity,
            },
        )
        .await
        .map(|_| ())
    } else {
        state
            .db
            .call("set_inventory_quantity_target", &args)
            .await
            .map_err(|error| error.to_string())
    };
    match result {
        Ok(()) => (axum::http::StatusCode::NO_CONTENT, ""),
        Err(error) => {
            tracing::warn!("Failed to save inventory target: {error}");
            (axum::http::StatusCode::BAD_REQUEST, "Could not save target")
        }
    }
}

#[derive(Deserialize)]
pub(super) struct EquipmentForm {
    inventory_item_id: u64,
    equipped: bool,
    placement_index: Option<u16>,
    attachment_targets: Option<String>,
    #[serde(default)]
    replace_occupied: bool,
}

#[derive(Deserialize, Serialize)]
pub(super) struct EquipmentAttachmentTargetForm {
    requirement_index: u16,
    parent_inventory_item_id: u64,
    attachment_point_id: String,
}

pub(super) async fn character_equipment_graph(
    state: &AppState,
    character_id: u64,
) -> Vec<CharacterEquipmentGraph> {
    let worn_sql =
        format!("SELECT * FROM character_equipped_item WHERE character_id = {character_id}");
    let occupancy_sql =
        format!("SELECT * FROM equipment_occupancy WHERE character_id = {character_id}");
    let inventory_sql = format!("SELECT * FROM inventory_item WHERE character_id = {character_id}");
    let (worn, occupancies, inventory, definitions) = tokio::join!(
        state.db.query::<CharacterEquippedItem>(&worn_sql),
        state.db.query::<EquipmentOccupancy>(&occupancy_sql),
        state.db.query::<InventoryItem>(&inventory_sql),
        state.db.query::<ItemDefinition>("SELECT * FROM item"),
    );
    let mut worn = worn.unwrap_or_default();
    let worn_ids = worn
        .iter()
        .map(|row| row.inventory_item_id)
        .collect::<Vec<_>>();
    let occupancies = occupancies.unwrap_or_default();
    let inventory = inventory.unwrap_or_default();
    let definitions = definitions.unwrap_or_default();
    for node in &mut worn {
        node.item_name = inventory
            .iter()
            .find(|item| item.id == node.inventory_item_id)
            .map_or_else(
                || format!("#{}", node.inventory_item_id),
                |item| item.item_id.clone(),
            );
    }
    let used_capacity = occupancies
        .iter()
        .filter(|row| row.anchor_kind == EquipmentAnchorKind::ItemAttachment)
        .fold(HashMap::<(u64, String), u16>::new(), |mut counts, row| {
            if let (Some(parent_id), Some(point_id)) = (
                row.parent_inventory_item_id,
                row.attachment_point_id.as_ref(),
            ) {
                *counts.entry((parent_id, point_id.clone())).or_default() += 1;
            }
            counts
        });
    let mut attachment_targets = Vec::new();
    for node in &worn {
        let Some(carried) = inventory
            .iter()
            .find(|item| item.id == node.inventory_item_id)
        else {
            continue;
        };
        let Some(definition) = definitions
            .iter()
            .find(|definition| definition.id == carried.item_id)
        else {
            continue;
        };
        for point in &definition.attachment_points {
            let used = used_capacity
                .get(&(node.inventory_item_id, point.id.clone()))
                .copied()
                .unwrap_or(0);
            attachment_targets.push(EquipmentAttachmentTarget {
                parent_inventory_item_id: node.inventory_item_id,
                parent_item_name: carried.item_id.clone(),
                attachment_point_id: point.id.clone(),
                channel: point.channel,
                accepts_tags: point.accepts_tags.clone(),
                free_capacity: point.capacity.saturating_sub(used),
                order: point.order,
            });
        }
    }
    attachment_targets.sort_by_key(|target| {
        (
            target.parent_inventory_item_id,
            target.order,
            target.attachment_point_id.clone(),
        )
    });
    vec![CharacterEquipmentGraph {
        _character_id: character_id,
        worn_item_ids: worn_ids,
        equipment_nodes: worn,
        equipment_occupancies: occupancies,
        attachment_targets,
    }]
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct StandardMedicationAdministration {
    actor_id: u64,
    patient_id: u64,
    inventory_item_id: u64,
    profile_version: u16,
    route: String,
    amount_milliunits: u32,
    region: Option<String>,
}

impl StandardMedicationAdministration {
    fn reducer_args(&self) -> [serde_json::Value; 7] {
        [
            json!(self.actor_id),
            json!(self.patient_id),
            json!(self.inventory_item_id),
            json!(self.profile_version),
            json!(&self.route),
            json!(self.amount_milliunits),
            json!(&self.region),
        ]
    }
}

pub(super) fn standard_medication_administration(
    session_character_id: u64,
    inventory_item_id: u64,
    preparation_id: &str,
    checked: bool,
) -> Result<StandardMedicationAdministration, &'static str> {
    if !checked {
        return Err("A preparation cannot be unchecked after it has been administered.");
    }
    let profile = adventuresim_core::physiology::current_intervention_profile(preparation_id)
        .ok_or("This medication has no current preparation profile.")?;
    Ok(StandardMedicationAdministration {
        actor_id: session_character_id,
        patient_id: session_character_id,
        inventory_item_id,
        profile_version: profile.version,
        route: format!("{:?}", profile.route).to_ascii_lowercase(),
        amount_milliunits: 1_000,
        region: None,
    })
}

pub(super) async fn set_equipment(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<EquipmentForm>,
) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return (StatusCode::UNAUTHORIZED, "Choose a character").into_response();
    };
    let inventory: Option<InventoryItem> = match state
        .db
        .query_one(&format!(
            "SELECT * FROM inventory_item WHERE id = {} AND character_id = {character_id}",
            form.inventory_item_id
        ))
        .await
    {
        Ok(inventory) => inventory,
        Err(error) => {
            tracing::warn!(%error, character_id, "failed to load equipment inventory row");
            return (StatusCode::SERVICE_UNAVAILABLE, "Inventory is unavailable").into_response();
        }
    };
    let Some(inventory) = inventory else {
        return (StatusCode::NOT_FOUND, "Item is not in this inventory").into_response();
    };
    let definition: Option<ItemDefinition> = match state
        .db
        .query_one(&format!(
            "SELECT * FROM item WHERE id = {}",
            sql_string_literal(&inventory.item_id)
        ))
        .await
    {
        Ok(definition) => definition,
        Err(error) => {
            tracing::warn!(%error, character_id, "failed to load equipment definition");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Equipment catalog is unavailable",
            )
                .into_response();
        }
    };
    let Some(definition) = definition else {
        return (StatusCode::NOT_FOUND, "Item definition is missing").into_response();
    };
    if definition.kind == crate::spacetimedb::ItemKind::Medication {
        let administration = match standard_medication_administration(
            character_id,
            form.inventory_item_id,
            &inventory.item_id,
            form.equipped,
        ) {
            Ok(administration) => administration,
            Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
        };
        if let Err(error) = state
            .db
            .call("administer_preparation", &administration.reducer_args())
            .await
        {
            tracing::warn!(
                %error,
                character_id,
                inventory_item_id = form.inventory_item_id,
                "preparation administration rejected"
            );
            return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
        return (StatusCode::NO_CONTENT, "").into_response();
    }
    if form.equipped && !definition.equipment_placements.is_empty() {
        let Some(placement_index) = form
            .placement_index
            .or_else(|| (definition.equipment_placements.len() == 1).then_some(0))
        else {
            return (
                StatusCode::BAD_REQUEST,
                "Choose a complete placement for this item",
            )
                .into_response();
        };
        let Some(placement) = definition
            .equipment_placements
            .get(usize::from(placement_index))
        else {
            return (StatusCode::BAD_REQUEST, "Invalid equipment placement").into_response();
        };
        let targets = match form.attachment_targets.as_deref() {
            Some(value) => match serde_json::from_str::<Vec<EquipmentAttachmentTargetForm>>(value) {
                Ok(targets) => targets,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        "Invalid attachment target selection",
                    )
                        .into_response();
                }
            },
            None => Vec::new(),
        };
        let result = if form.replace_occupied {
            state
                .db
                .call(
                    "replace_item_at_placement",
                    &[
                        json!(character_id),
                        json!(form.inventory_item_id),
                        json!(placement_index),
                        json!(targets),
                    ],
                )
                .await
        } else if !placement.parents.is_empty() {
            state
                .db
                .call(
                    "attach_item_at_placement",
                    &[
                        json!(character_id),
                        json!(form.inventory_item_id),
                        json!(placement_index),
                        json!(targets),
                    ],
                )
                .await
        } else {
            state
                .db
                .call(
                    "equip_item_at_placement",
                    &[
                        json!(character_id),
                        json!(form.inventory_item_id),
                        json!(placement_index),
                    ],
                )
                .await
        };
        if let Err(error) = result {
            tracing::warn!(%error, character_id, "failed to equip item");
            return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
        return (StatusCode::NO_CONTENT, "").into_response();
    }
    let destination = if form.equipped {
        definition.slot
    } else {
        ItemSlot::None
    };
    if form.equipped && destination == ItemSlot::None {
        return (StatusCode::BAD_REQUEST, "This item cannot be equipped").into_response();
    }
    if let Err(error) = state
        .db
        .call(
            "equip_item",
            &[
                json!(character_id),
                json!(form.inventory_item_id),
                destination.sats_json(),
            ],
        )
        .await
    {
        tracing::warn!(%error, character_id, "failed to update equipment");
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    for reducer in ["refresh_capabilities", "refresh_strategic_condition"] {
        if let Err(error) = state.db.call(reducer, &[json!(character_id)]).await {
            tracing::warn!(%error, character_id, reducer, "failed to refresh equipment projection");
        }
    }
    (StatusCode::NO_CONTENT, "").into_response()
}

pub(super) async fn deposit_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        for (item_id, quantity) in transfer_entries(&form) {
            let _ = state
                .db
                .call(
                    "deposit_party_inventory_item",
                    &[json!(character_id), json!(item_id), json!(quantity)],
                )
                .await;
        }
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party-inventory")))
}

pub(super) async fn withdraw_party_inventory(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        for (item_id, quantity) in transfer_entries(&form) {
            let _ = state
                .db
                .call(
                    "withdraw_party_inventory_item",
                    &[json!(character_id), json!(item_id), json!(quantity)],
                )
                .await;
        }
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party-inventory")))
}

pub(super) fn transfer_entries(form: &PartyPoolTransferForm) -> Vec<(u64, u32)> {
    match form.entries() {
        Ok(entries) => entries
            .into_iter()
            .map(|entry| (entry.id, entry.quantity))
            .collect(),
        Err(error) => {
            tracing::warn!(error, "invalid party inventory transfer form");
            Vec::new()
        }
    }
}

pub(super) async fn liquidate_party_assets(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<PartyPoolTransferForm>,
) -> Redirect {
    if kind == "settlement"
        && let Some(character_id) = session.character_id_u64()
    {
        let entries = transfer_entries(&form);
        let _ = state
            .db
            .call(
                "liquidate_party_inventory",
                &[
                    json!(character_id),
                    json!(id.clone()),
                    json!(entries.iter().map(|entry| entry.0).collect::<Vec<_>>()),
                    json!(entries.iter().map(|entry| entry.1).collect::<Vec<_>>()),
                ],
            )
            .await;
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party-inventory")))
}

pub(super) async fn remove_party_member(
    State(state): State<AppState>,
    Path((kind, id, member_character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Redirect {
    if let Some(actor_character_id) = session.character_id_u64() {
        let result = if actor_character_id == member_character_id {
            state
                .db
                .call("leave_party", &[json!(actor_character_id)])
                .await
                .map_err(|error| error.to_string())
        } else {
            super::execute_or_request_party_action(
                &state,
                actor_character_id,
                super::PartyAction::RemovePartyMember {
                    character_id: member_character_id,
                },
            )
            .await
            .map(|_| ())
        };
        if let Err(error) = result {
            tracing::error!("Failed to remove party member: {error:?}");
        }
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}")))
}

pub(super) async fn party_stats(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Html<String> {
    render_party_stats(
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
pub(super) async fn render_party_stats(
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
    let Some((active_character, _)) =
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
                "SELECT * FROM character WHERE id = {character_id}"
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
    let active_party = match active_character.party_id.as_deref() {
        Some(party_id) => state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next(),
        None => None,
    };
    let selected_party = match selected.party_id.as_deref() {
        Some(party_id) => state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next(),
        None => None,
    };
    let selected_attributes: Vec<CharacterAttributes> = state
        .db
        .query(&format!(
            "SELECT * FROM character_attributes WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let selected_skills: Vec<CharacterSkills> = state
        .db
        .query(&format!(
            "SELECT * FROM character_skills WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let selected_limbs: Vec<CharacterLimbs> = state
        .db
        .query(&format!(
            "SELECT * FROM character_limbs WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    let capability = get_character_capability(&state, character_id).await;
    let combat_profile = get_combat_training_profile(&state, character_id).await;
    let can_examine = false;
    let condition = get_strategic_condition(&state, character_id).await;
    let morale_sources = get_morale_sources(&state, character_id).await;
    let religion = query_single::<CharacterCondition>(&state, "character_condition", character_id)
        .await
        .and_then(|condition| condition.religion_id);
    let reputation = query_local_reputation(&state, character_id, &location.id).await;
    let fame = reputation
        .as_ref()
        .map_or(0.0, |value| value.fame as f32 / 100.0);
    let infamy = reputation
        .as_ref()
        .map_or(0.0, |value| value.infamy as f32 / 100.0);
    let personality: Option<CharacterPersonality> = None;
    let medical = medical_presentation(&state, active_character.id, character_id).await;
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
    let filth = state
        .db
        .query::<CharacterFilth>(&format!(
            "SELECT * FROM character_filth WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    Html(
        party_stats_page(
            &location,
            &selected,
            &active_character,
            &party_members,
            capability.as_ref(),
            selected_attributes.first(),
            selected_skills.first(),
            selected_limbs.first(),
            combat_profile,
            condition.as_ref(),
            &morale_sources,
            religion.as_deref(),
            active_party.as_ref(),
            selected_party.as_ref(),
            fame,
            infamy,
            personality.as_ref(),
            &medical,
            can_examine,
            &injuries,
            &projectiles,
            &filth,
            dialog,
            surgery_open,
            social_open,
        )
        .into_string(),
    )
}

pub(crate) async fn medical_presentation(
    state: &AppState,
    viewer_id: u64,
    target_id: u64,
) -> crate::medical::MedicalPresentation {
    let rows = match state
        .db
        .query::<BackendPhysiologyChart>(&format!(
            "SELECT * FROM backend_physiology_charts WHERE observer_id = {viewer_id} AND patient_id = {target_id}"
        ))
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error,target_id,"private medical query failed closed");
            return crate::medical::MedicalPresentation {
                unavailable: true,
                ..Default::default()
            };
        }
    };
    let administrations = if administration_history_visible(viewer_id, target_id, !rows.is_empty())
    {
        state
            .db
            .query::<BackendPhysiologyAdministration>(&format!(
                "SELECT * FROM backend_physiology_administrations WHERE patient_id = {target_id}"
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let current_minute = match state
        .db
        .query_one::<CharacterTime>(&format!(
            "SELECT * FROM character_time WHERE character_id = {target_id}"
        ))
        .await
    {
        Ok(Some(time)) => time.minutes,
        Ok(None) => {
            tracing::error!(
                target_id,
                "patient time missing; medical presentation unavailable"
            );
            return crate::medical::MedicalPresentation {
                unavailable: true,
                ..Default::default()
            };
        }
        Err(error) => {
            tracing::error!(%error, target_id, "patient time query failed; medical presentation unavailable");
            return crate::medical::MedicalPresentation {
                unavailable: true,
                ..Default::default()
            };
        }
    };
    crate::medical::sanitize(&rows, &administrations, current_minute)
}

pub(super) fn administration_history_visible(
    viewer_id: u64,
    target_id: u64,
    has_authorized_observations: bool,
) -> bool {
    viewer_id == target_id || has_authorized_observations
}

#[cfg(test)]
mod physiology_privacy_tests {
    use super::{administration_history_visible, standard_medication_administration};
    use serde_json::json;

    #[test]
    fn administration_history_requires_self_or_an_authorized_observation() {
        assert!(administration_history_visible(7, 7, false));
        assert!(administration_history_visible(7, 8, true));
        assert!(!administration_history_visible(7, 8, false));
    }

    #[test]
    fn standard_medication_mapping_is_self_only_versioned_and_parameter_free() {
        let action =
            standard_medication_administration(7, 42, "oral_rehydration_draught", true).unwrap();
        assert_eq!(action.actor_id, 7);
        assert_eq!(action.patient_id, 7);
        assert_eq!(action.inventory_item_id, 42);
        assert_eq!(action.profile_version, 1);
        assert_eq!(action.route, "oral");
        assert_eq!(action.amount_milliunits, 1_000);
        assert_eq!(action.region, None);
        assert_eq!(
            action.reducer_args(),
            [
                json!(7),
                json!(7),
                json!(42),
                json!(1),
                json!("oral"),
                json!(1_000),
                json!(null),
            ]
        );
    }

    #[test]
    fn standard_medication_mapping_rejects_uncheck_and_unknown_profile() {
        assert_eq!(
            standard_medication_administration(7, 42, "oral_rehydration_draught", false),
            Err("A preparation cannot be unchecked after it has been administered.")
        );
        assert_eq!(
            standard_medication_administration(7, 42, "unknown_medication", true),
            Err("This medication has no current preparation profile.")
        );
    }
}

pub(super) async fn stop_preparation(
    State(state): State<AppState>,
    Path((kind, id, patient_id, administration_id)): Path<(String, String, u64, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Err(error) = state
        .db
        .call(
            "stop_preparation",
            &[json!(actor_id), json!(administration_id)],
        )
        .await
    {
        tracing::warn!(%error, actor_id, patient_id, administration_id, "preparation stop rejected");
    }
    Redirect::to(&building.append_to(format!("/locations/{kind}/{id}/party/{patient_id}")))
}

pub(super) async fn get_strategic_condition(
    state: &AppState,
    character_id: u64,
) -> Option<CharacterStrategicCondition> {
    if let Err(error) = state
        .db
        .call("refresh_strategic_condition", &[json!(character_id)])
        .await
    {
        tracing::warn!(%error, character_id, "failed to refresh strategic condition");
        return None;
    }
    query_single(state, "character_strategic_condition", character_id).await
}

pub(super) async fn get_morale_sources(state: &AppState, character_id: u64) -> Vec<CharacterMoraleSource> {
    let mut sources: Vec<CharacterMoraleSource> = state
        .db
        .query(&format!(
            "SELECT * FROM character_morale_source WHERE character_id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    sources.sort_by(|left, right| right.magnitude.abs().total_cmp(&left.magnitude.abs()));
    sources
}
