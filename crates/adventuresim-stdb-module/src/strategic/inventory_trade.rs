/// Transfer a stack of items between two members of the same party.
#[reducer]
pub fn transfer_party_item(
    ctx: &ReducerContext,
    from_character_id: u64,
    to_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, from_character_id)?;
    crate::character::require_living_character(ctx, from_character_id)?;
    crate::character::require_living_character(ctx, to_character_id)?;
    if quantity == 0 || from_character_id == to_character_id {
        return Err("Transfer quantity must be positive and between different characters".into());
    }
    let Some(from) = ctx.db.character().id().find(from_character_id) else {
        return Err("Source character not found".into());
    };
    let Some(to) = ctx.db.character().id().find(to_character_id) else {
        return Err("Recipient character not found".into());
    };
    if from.party_id.is_none() || from.party_id != to.party_id {
        return Err("Characters must belong to the same party".into());
    }
    let Some(source_item) = ctx.db.inventory_item().id().find(inventory_item_id) else {
        return Err("Inventory item not found".into());
    };
    if source_item.character_id != from_character_id || source_item.quantity < quantity {
        return Err("Source character does not have that quantity".into());
    }
    if crate::character::inventory_item_is_equipped(ctx, from_character_id, inventory_item_id) {
        return Err("Unequip an item before transferring it".into());
    }

    let measured = crate::inventory_amount::personal_amount(ctx, source_item.id).is_some();
    if measured {
        if quantity != 1 || source_item.quantity != 1 {
            return Err("Measured items must be transferred as complete rows".into());
        }
        let mut transferred = source_item;
        transferred.character_id = to_character_id;
        ctx.db.inventory_item().id().update(transferred);
        crate::capability::refresh_character_capability(ctx, from_character_id)?;
        crate::capability::refresh_character_capability(ctx, to_character_id)?;
        return Ok(());
    }

    if item_is_medication(ctx, &source_item.item_id) {
        if quantity != 1 || source_item.quantity != 1 {
            return Err("Medication must be transferred as an individual course".into());
        }
        let mut transferred = source_item;
        transferred.character_id = to_character_id;
        ctx.db.inventory_item().id().update(transferred);
        return Ok(());
    }

    let durable = item_is_durable(ctx, &source_item.item_id);
    if durable {
        if quantity != 1 || source_item.quantity != 1 {
            return Err("Equipment instances must be transferred individually".into());
        }
        let mut transferred = source_item;
        transferred.character_id = to_character_id;
        ctx.db.inventory_item().id().update(transferred);
        return Ok(());
    }

    let food = ctx
        .db
        .item()
        .id()
        .find(&source_item.item_id)
        .is_some_and(|row| row.kind == crate::ItemKind::Food)
        || adventuresim_core::food::definition(&source_item.item_id).is_some();
    if food {
        if source_item.quantity == quantity {
            let mut moved = source_item;
            moved.character_id = to_character_id;
            ctx.db.inventory_item().id().update(moved);
        } else {
            let original_quantity = source_item.quantity;
            let item_id = source_item.item_id.clone();
            let mut remaining = source_item;
            remaining.quantity -= quantity;
            ctx.db.inventory_item().id().update(remaining);
            let destination = ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id: to_character_id,
                item_id,
                quantity,
            });
            crate::food::split_lot(
                ctx,
                inventory_item_id,
                destination.id,
                quantity,
                original_quantity,
            )?;
        }
        crate::capability::refresh_character_capability(ctx, from_character_id)?;
        crate::capability::refresh_character_capability(ctx, to_character_id)?;
        return Ok(());
    }

    let destination_item = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((to_character_id, &source_item.item_id))
        .next();
    let merged_quantity = destination_item
        .as_ref()
        .and_then(|destination| destination.quantity.checked_add(quantity));

    if source_item.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory_item_id);
    } else {
        let mut updated = source_item.clone();
        updated.quantity -= quantity;
        ctx.db.inventory_item().id().update(updated);
    }
    if let (Some(mut destination_item), Some(merged_quantity)) = (destination_item, merged_quantity)
    {
        destination_item.quantity = merged_quantity;
        ctx.db.inventory_item().id().update(destination_item);
    } else {
        ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id: to_character_id,
            item_id: source_item.item_id,
            quantity,
        });
    }
    Ok(())
}

/// Permanently removes staged quantities from a character's unequipped inventory.
fn objective_item_value(ctx: &ReducerContext, item_id: &str) -> Result<u64, String> {
    ctx.db
        .item()
        .id()
        .find(&item_id.to_string())
        .and_then(|item| item.base_value)
        .map(u64::from)
        .ok_or_else(|| format!("Item {item_id} has no objective value"))
}

fn food_lot_value(value: f32) -> Result<u64, String> {
    if !value.is_finite() || value < 0.0 {
        return Err("Food lot has invalid value".into());
    }
    Ok(value.floor() as u64)
}

fn personal_inventory_value(
    ctx: &ReducerContext,
    inventory: &InventoryItem,
    quantity: u32,
) -> Result<u64, String> {
    if let Some(lot) = crate::food::personal_lot(ctx, inventory.id) {
        if quantity != inventory.quantity {
            return Err("Food batches must move as complete lots".into());
        }
        food_lot_value(lot.total_value)
    } else if let Some(amount) = crate::inventory_amount::personal_amount(ctx, inventory.id) {
        if quantity != 1 || inventory.quantity != 1 {
            return Err("Measured inventory must be valued as a complete row".into());
        }
        Ok(adventuresim_core::inventory_measurement::scaled_by_amount(
            objective_item_value(ctx, &inventory.item_id)?,
            amount,
        ))
    } else {
        objective_item_value(ctx, &inventory.item_id)?
            .checked_mul(u64::from(quantity))
            .ok_or_else(|| "Party asset liquidation line value overflow".into())
    }
}

fn party_inventory_value(
    ctx: &ReducerContext,
    inventory: &PartyInventoryItem,
    quantity: u32,
) -> Result<u64, String> {
    if let Some(lot) = crate::food::party_lot(ctx, inventory.id) {
        if quantity != inventory.quantity {
            return Err("Food batches must move as complete lots".into());
        }
        food_lot_value(lot.total_value)
    } else if let Some(amount) = crate::inventory_amount::party_amount(ctx, inventory.id) {
        if quantity != 1 || inventory.quantity != 1 {
            return Err("Measured inventory must be valued as a complete row".into());
        }
        Ok(adventuresim_core::inventory_measurement::scaled_by_amount(
            objective_item_value(ctx, &inventory.item_id)?,
            amount,
        ))
    } else {
        objective_item_value(ctx, &inventory.item_id)?
            .checked_mul(u64::from(quantity))
            .ok_or_else(|| "Inventory value overflow".into())
    }
}

fn item_is_durable(ctx: &ReducerContext, item_id: &str) -> bool {
    ctx.db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|definition| definition.repairable)
}

fn item_is_medication(ctx: &ReducerContext, item_id: &str) -> bool {
    ctx.db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|definition| definition.kind == crate::ItemKind::Medication)
}

#[cfg(test)]
mod durable_custody_tests {
    #[test]
    fn clothing_uses_repairable_capability_for_every_custody_path() {
        let clothing = crate::Item {
            id: "linen_tunic".into(),
            kind: crate::ItemKind::Clothing,
            repairable: true,
            ..crate::Item::default()
        };
        assert!(clothing.repairable);

        let source = STRATEGIC_SOURCE;
        let durable_policy = source
            .split("fn item_is_durable")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(durable_policy.contains("definition.repairable"));
        assert!(!durable_policy.contains("ItemKind::"));
        for (start, end) in [
            ("fn add_to_party_inventory_checked", "fn credit_party_stake"),
            (
                "pub fn deposit_party_inventory_item",
                "pub(crate) fn consume_personal_gold",
            ),
            (
                "pub fn withdraw_party_inventory_item",
                "pub fn liquidate_party_inventory",
            ),
        ] {
            let custody_path = source
                .split(start)
                .nth(1)
                .unwrap()
                .split(end)
                .next()
                .unwrap();
            assert!(
                custody_path.contains("item_is_durable"),
                "{start} must preserve condition custody for repairable clothing"
            );
        }
    }
}

#[cfg(test)]
mod medication_custody_tests {
    #[test]
    fn medication_stays_in_quantity_one_rows_across_custody_paths() {
        let source = STRATEGIC_SOURCE;
        let direct_transfer = source
            .split("pub fn transfer_party_item")
            .nth(1)
            .unwrap()
            .split("fn objective_item_value")
            .next()
            .unwrap();
        assert!(direct_transfer.contains("item_is_medication"));
        assert!(direct_transfer.contains("source_item.quantity != 1"));
        assert!(direct_transfer.contains("transferred.character_id = to_character_id"));

        let party_add = source
            .rsplit("fn add_to_party_inventory_checked")
            .next()
            .unwrap()
            .split("fn credit_party_stake")
            .next()
            .unwrap();
        assert!(party_add.contains("kind == Some(crate::ItemKind::Medication)"));
        assert!(party_add.contains("for _ in 0..quantity"));
        assert!(party_add.contains("quantity: 1"));

        let withdrawal = source
            .rsplit("pub fn withdraw_party_inventory_item")
            .next()
            .unwrap()
            .split("pub fn liquidate_party_inventory")
            .next()
            .unwrap();
        assert!(withdrawal.contains("item_is_medication"));
        assert!(withdrawal.contains("Medication must be withdrawn as an individual course"));
        assert!(withdrawal.contains("crate::add_inventory_item"));

        let deposit = source
            .rsplit("pub fn deposit_party_inventory_item")
            .next()
            .unwrap()
            .split("pub(crate) fn consume_personal_gold")
            .next()
            .unwrap();
        assert!(deposit.contains("item_is_medication"));
        assert!(deposit.contains("Medication must be deposited as an individual course"));
        assert!(deposit.contains("add_to_party_inventory"));
    }
}

pub(crate) fn add_to_party_inventory(
    ctx: &ReducerContext,
    party_id: &str,
    item_id: &str,
    quantity: u32,
) {
    let _ = add_to_party_inventory_checked(ctx, party_id, item_id, quantity);
}

fn add_to_party_inventory_checked(
    ctx: &ReducerContext,
    party_id: &str,
    item_id: &str,
    quantity: u32,
) -> Result<(), String> {
    if quantity == 0 {
        return Ok(());
    }
    let kind = ctx
        .db
        .item()
        .id()
        .find(item_id.to_string())
        .map(|row| row.kind);
    let food_definition = crate::item::inventory_food_definition(kind, item_id)?;
    let measured = crate::inventory_amount::is_measured_item(ctx, item_id);
    if kind == Some(crate::ItemKind::Medication) {
        for _ in 0..quantity {
            ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.into(),
                item_id: item_id.into(),
                quantity: 1,
            });
        }
        return Ok(());
    }
    if measured {
        let minute = ctx
            .db
            .party_authority()
            .id()
            .find(&party_id.to_string())
            .and_then(|party| ctx.db.character_time().character_id().find(party.leader_id))
            .map_or(0, |time| time.minutes);
        for _ in 0..quantity {
            let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.into(),
                item_id: item_id.into(),
                quantity: 1,
            });
            crate::inventory_amount::initialize_party(ctx, row.id);
            if food_definition.is_some() {
                crate::food::create_party_food_lot(ctx, row.id, item_id, 1, minute)
                    .ok_or_else(|| format!("Could not create party food lot for {item_id}"))?;
            }
        }
        return Ok(());
    }
    if item_is_durable(ctx, item_id) {
        for _ in 0..quantity {
            let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.to_string(),
                item_id: item_id.to_string(),
                quantity: 1,
            });
            ctx.db.party_item_condition().insert(PartyItemCondition {
                party_inventory_item_id: row.id,
                tier_1: 0.0,
                tier_2: 0.0,
                tier_3: 0.0,
                tier_4: 0.0,
                tier_5: 0.0,
            });
        }
        return Ok(());
    }
    if let Some(mut stack) = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .find(|stack| stack.item_id == item_id)
    {
        if let Some(merged) = stack.quantity.checked_add(quantity) {
            stack.quantity = merged;
            ctx.db.party_inventory_item().id().update(stack);
        } else {
            ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.to_string(),
                item_id: item_id.to_string(),
                quantity,
            });
        }
    } else {
        ctx.db.party_inventory_item().insert(PartyInventoryItem {
            id: 0,
            party_id: party_id.to_string(),
            item_id: item_id.to_string(),
            quantity,
        });
    }
    Ok(())
}

fn credit_party_stake(
    ctx: &ReducerContext,
    party_id: &str,
    character_id: u64,
    value: u64,
) -> Result<(), String> {
    if value == 0 {
        return Ok(());
    }
    if let Some(mut stake) = ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .find(|stake| stake.character_id == character_id)
    {
        stake.value = stake
            .value
            .checked_add(value)
            .ok_or("Party stake overflow")?;
        ctx.db.party_stake().id().update(stake);
    } else {
        ctx.db.party_stake().insert(PartyStake {
            id: 0,
            party_id: party_id.to_string(),
            character_id,
            value,
        });
    }
    Ok(())
}

fn credit_party_reserve(ctx: &ReducerContext, party_id: &str, value: u64) -> Result<(), String> {
    if value == 0 {
        return Ok(());
    }
    if let Some(mut state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
    {
        state.reserve_value = state
            .reserve_value
            .checked_add(value)
            .ok_or("Party reserve overflow")?;
        ctx.db.party_inventory_state().party_id().update(state);
    } else {
        ctx.db.party_inventory_state().insert(PartyInventoryState {
            party_id: party_id.to_string(),
            reserve_value: value,
        });
    }
    Ok(())
}

fn mission_candidate_is_current(
    ctx: &ReducerContext,
    mission: &MissionAuthority,
    candidate: &MissionOutcomeCandidate,
) -> Result<bool, String> {
    if mission.status != MissionAttemptStatus::Bound
        || candidate.mission_id != mission.id
        || candidate.case_id != mission.case_id
        || mission.case_site_id.as_ref() != Some(&candidate.case_site_id)
        || mission.hostile_group_id.as_deref() != Some(&candidate.hostile_group_id)
    {
        return Ok(false);
    }
    let Some(capability) = ctx
        .db
        .mission_approach_capability()
        .id()
        .find(&candidate.capability_id)
    else {
        return Ok(false);
    };
    if !capability.active
        || capability.observer_character_id != mission.observer_character_id
        || capability.case_id != candidate.case_id
        || capability.case_site_id != candidate.case_site_id
        || capability.hostile_group_id != candidate.hostile_group_id
        || capability.path_index != candidate.path_index
        || capability.objective_id != candidate.objective_id
        || capability.resolution != candidate.resolution
        || capability.weight != candidate.weight
        || capability.capture_subject_id != candidate.capture_subject_id
        || capability.capture_custody_version != candidate.capture_custody_version
    {
        return Ok(false);
    }
    let Some(case) = ctx.db.case_authority().id().find(&candidate.case_id) else {
        return Ok(false);
    };
    if case.resolution_status != CaseResolutionStatus::Open {
        return Ok(false);
    }
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| {
            serde_json::from_str::<adventuresim_core::case::OutcomeFact>(&row.fact_json)
                .map_err(|_| "Stored outcome fact is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let core_case_id =
        adventuresim_core::case::CaseId::new(case.id.clone()).map_err(|_| "Case ID is invalid")?;
    let evaluation = expression.evaluate(&core_case_id, &mission.party_id, &facts);
    let Some(path) = expression
        .alternatives
        .get(usize::from(candidate.path_index))
    else {
        return Ok(false);
    };
    let Some(objective_index) = path
        .objectives
        .iter()
        .position(|objective| objective.id.as_str() == candidate.objective_id)
    else {
        return Ok(false);
    };
    if evaluation
        .alternatives
        .get(usize::from(candidate.path_index))
        .and_then(|path| path.get(objective_index))
        .is_none_or(|progress| progress.state != adventuresim_core::case::EvaluationState::Pending)
    {
        return Ok(false);
    }
    if candidate.resolution != HostileResolutionKind::Captured {
        return Ok(true);
    }
    let (Some(subject_id), Some(version)) = (
        candidate.capture_subject_id.as_ref(),
        candidate.capture_custody_version,
    ) else {
        return Ok(false);
    };
    Ok(ctx
        .db
        .case_custody()
        .object_id()
        .find(subject_id)
        .is_some_and(|custody| {
            custody.case_id == candidate.case_id
                && custody.object_kind == CustodyObjectKind::Subject
                && custody.holder_kind == CustodyHolderKind::Site
                && custody.holder_id == candidate.case_site_id.value
                && custody.version == version
        }))
}

fn mission_outcome_draw(mission: &MissionAuthority, candidates: &[MissionOutcomeCandidate]) -> u64 {
    let mut candidates = candidates.to_vec();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let mut hasher = Sha256::new();
    hasher.update(b"adventuresim:strategic-mission-outcome:v1\0");
    hasher.update(mission.party_id.as_bytes());
    hasher.update(mission.outcome_entropy.to_le_bytes());
    for candidate in &candidates {
        hasher.update([0]);
        hasher.update(candidate.id.as_bytes());
        hasher.update([0]);
        hasher.update(candidate.capability_id.as_bytes());
        hasher.update(candidate.weight.to_le_bytes());
        hasher.update([candidate.resolution as u8]);
    }
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes)
}

#[allow(clippy::too_many_arguments)]
fn mission_approach_capability_id(
    observer_character_id: u64,
    case_id: &str,
    site_id: &str,
    hostile_group_id: &str,
    path_index: u16,
    objective_id: &str,
    resolution: HostileResolutionKind,
    capture_subject_id: Option<&str>,
    capture_custody_version: Option<u32>,
) -> String {
    format!(
        "mission-approach:{observer_character_id}:{case_id}:{site_id}:{hostile_group_id}:{path_index}:{objective_id}:{}:{}:{}",
        resolution as u8,
        capture_subject_id.unwrap_or("-"),
        capture_custody_version.map_or_else(|| "-".into(), |version| version.to_string()),
    )
}

fn sample_mission_candidate(
    mission: &MissionAuthority,
    mut candidates: Vec<MissionOutcomeCandidate>,
) -> Option<MissionOutcomeCandidate> {
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let total_weight = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(u64::from(candidate.weight))
    });
    if total_weight == 0 {
        return None;
    }
    let mut draw = mission_outcome_draw(mission, &candidates) % total_weight;
    for candidate in candidates {
        let weight = u64::from(candidate.weight);
        if draw < weight {
            return Some(candidate);
        }
        draw -= weight;
    }
    None
}

/// Commit the strategic meaning of an authenticated successful combat
/// session. The child reports only success; the exact result is sampled here.
pub(crate) fn complete_bound_mission_success(
    ctx: &ReducerContext,
    mission_id: &str,
) -> Result<bool, String> {
    let mut mission = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id.to_string())
        .ok_or("Mission authority not found")?;
    if mission.status == MissionAttemptStatus::Committed {
        return Ok(false);
    }
    if mission.status != MissionAttemptStatus::Bound {
        return Err("Mission attempt is no longer eligible for completion".into());
    }
    let mut candidates = Vec::new();
    for candidate in ctx
        .db
        .mission_outcome_candidate()
        .mission_id()
        .filter(&mission.id)
    {
        if mission_candidate_is_current(ctx, &mission, &candidate)? {
            candidates.push(candidate);
        }
    }
    let Some(selected) = sample_mission_candidate(&mission, candidates) else {
        mission.status = MissionAttemptStatus::Failed;
        ctx.db.mission_authority().id().update(mission);
        return Ok(false);
    };
    let group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&selected.hostile_group_id)
        .ok_or("Bound mission hostile group no longer exists")?;
    if group.disposition != HostileGroupDisposition::Active {
        return Err("Bound hostile group is already resolved".into());
    }
    let dropped_items = if selected.resolution == HostileResolutionKind::Defeated {
        mission
            .drop_item_id
            .clone()
            .map(|item| vec![(item, mission.drop_quantity)])
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let battle_id = format!("battle:{mission_id}");
    let outcome_source_id = format!("outcome:{mission_id}");
    let committed = commit_hostile_battle_resolution(
        ctx,
        &outcome_source_id,
        &battle_id,
        &mission.party_id,
        Some(mission_id),
        Some(&selected.hostile_group_id),
        selected.resolution,
        selected.capture_subject_id.as_deref(),
        dropped_items,
        selected.resolution == HostileResolutionKind::Defeated,
    )?;
    mission.status = MissionAttemptStatus::Committed;
    mission.committed_resolution = Some(selected.resolution);
    mission.committed_capture_subject_id = selected.capture_subject_id;
    ctx.db.mission_authority().id().update(mission);
    for mut capability in ctx
        .db
        .mission_approach_capability()
        .hostile_group_id()
        .filter(&selected.hostile_group_id)
        .filter(|capability| capability.case_site_id == selected.case_site_id)
        .collect::<Vec<_>>()
    {
        capability.active = false;
        ctx.db.mission_approach_capability().id().update(capability);
    }
    if committed {
        finish_incident_for_hostile_group(ctx, &selected.hostile_group_id)?;
    }
    Ok(committed)
}

pub(crate) fn fail_bound_mission_attempt(
    ctx: &ReducerContext,
    mission_id: &str,
) -> Result<(), String> {
    let Some(mut mission) = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id.to_string())
    else {
        return Ok(());
    };
    match mission.status {
        MissionAttemptStatus::Bound => {
            mission.status = MissionAttemptStatus::Failed;
            ctx.db.mission_authority().id().update(mission);
            Ok(())
        }
        MissionAttemptStatus::Failed => Ok(()),
        MissionAttemptStatus::Committed | MissionAttemptStatus::Cancelled => {
            Err("Conflicting terminal mission retry".into())
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_hostile_battle_resolution(
    ctx: &ReducerContext,
    outcome_source_id: &str,
    battle_id: &str,
    party_id: &str,
    mission_id: Option<&str>,
    hostile_group_id: Option<&str>,
    resolution: HostileResolutionKind,
    capture_subject_id: Option<&str>,
    dropped_items: Vec<(String, u32)>,
    include_random_gold: bool,
) -> Result<bool, String> {
    validate_hostile_resolution_contract(
        None,
        None,
        resolution,
        capture_subject_id,
        !dropped_items.is_empty() || include_random_gold,
    )
    .map_err(str::to_string)?;
    adventuresim_core::mission::OutcomeSourceId::new(outcome_source_id).map_err(str::to_string)?;
    adventuresim_core::mission::BattleId::new(battle_id).map_err(str::to_string)?;
    if let Some(id) = mission_id {
        adventuresim_core::mission::MissionId::new(id).map_err(str::to_string)?;
    }
    if let Some(id) = hostile_group_id {
        adventuresim_core::mission::HostileGroupId::new(id).map_err(str::to_string)?;
    }
    if let Some(existing) = ctx
        .db
        .outcome_source_authority()
        .id()
        .find(&outcome_source_id.to_string())
    {
        return if existing.battle_id == battle_id
            && existing.party_id == party_id
            && existing.mission_id.as_deref() == mission_id
            && existing.hostile_group_id.as_deref() == hostile_group_id
            && existing.resolution == resolution
        {
            Ok(false)
        } else {
            Err("Conflicting retry for strategic battle outcome source".into())
        };
    }
    let group = hostile_group_id
        .map(|id| {
            ctx.db
                .hostile_group_authority()
                .id()
                .find(&id.to_string())
                .ok_or_else(|| "Hostile group not found".to_string())
        })
        .transpose()?;
    if let Some(mission_id) = mission_id {
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&mission_id.to_string())
            .ok_or("Mission authority not found")?;
        if mission.party_id != party_id || mission.hostile_group_id.as_deref() != hostile_group_id {
            return Err("Battle attribution does not match mission authority".into());
        }
        if mission.status != MissionAttemptStatus::Bound {
            return Err("Mission attempt is not bound for strategic completion".into());
        }
        let mut candidates = Vec::new();
        for candidate in ctx
            .db
            .mission_outcome_candidate()
            .mission_id()
            .filter(&mission.id)
        {
            if mission_candidate_is_current(ctx, &mission, &candidate)? {
                candidates.push(candidate);
            }
        }
        let selected = sample_mission_candidate(&mission, candidates)
            .ok_or("Mission has no current strategic outcome candidate")?;
        if selected.resolution != resolution
            || selected.capture_subject_id.as_deref() != capture_subject_id
        {
            return Err("Strategic result is not an exact current mission candidate".into());
        }
    }
    ctx.db
        .outcome_source_authority()
        .insert(OutcomeSourceAuthority {
            id: outcome_source_id.to_string(),
            battle_id: battle_id.to_string(),
            mission_id: mission_id.map(str::to_string),
            hostile_group_id: hostile_group_id.map(str::to_string),
            resolution,
            party_id: party_id.to_string(),
        });
    ctx.db.battle_result().insert(BattleResult {
        battle_id: battle_id.to_string(),
        party_id: party_id.to_string(),
    });
    if let Some(mission_id) = mission_id {
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&mission_id.to_string())
            .ok_or("Mission authority not found")?;
        if let Some(ref site_id) = mission.case_site_id {
            let site = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&site_id.value)
                .ok_or("Case site authority not found")?;
            let public_case_id = mission_public_case_id(ctx, &mission)?;
            ctx.db
                .backend_case_battle_authority()
                .insert(BackendCaseBattle {
                    gateway_bucket: 0,
                    owner_character_id: mission.observer_character_id,
                    public_case_id,
                    party_id: party_id.to_string(),
                    battle_id: battle_id.to_string(),
                    mission_id: mission_id.to_string(),
                    case_site_id: site.id,
                });
        }
    }
    // Bound missions own an immutable encounter snapshot. Later recurring
    // incidents may strengthen the live hostile group while this mission is
    // still in flight, so rewards and morale use its bound normalized power.
    let difficulty = mission_id
        .and_then(|id| {
            ctx.db
                .mission_authority()
                .id()
                .find(&id.to_string())
                .map(|mission| {
                    let normalized = mission
                        .normalized_combat_power
                        .div_ceil(adventuresim_core::threat_escalation::BASELINE_ORC_POWER)
                        .min(i32::MAX as u32) as i32;
                    mission.enemy_difficulty.max(normalized)
                })
        })
        .unwrap_or_else(|| group.as_ref().map_or(1, |group| group.difficulty));
    for member_id in living_party_member_ids(ctx, party_id) {
        ctx.db.battle_participant().insert(BattleParticipant {
            id: 0,
            participant_battle_id: battle_id.to_string(),
            character_id: member_id,
        });
        crate::condition::record_morale_event(
            ctx,
            member_id,
            "victory",
            5.0 + difficulty.max(0) as f32,
            Some(outcome_source_id.to_string()),
        )?;
    }
    let mut combined: HashMap<String, u32> = HashMap::new();
    for (item_id, quantity) in dropped_items {
        if quantity > 0 && ctx.db.item().id().find(&item_id).is_some() {
            *combined.entry(item_id).or_default() = combined
                .get(&item_id)
                .copied()
                .unwrap_or_default()
                .saturating_add(quantity);
        }
    }
    if include_random_gold && ctx.random::<u64>().is_multiple_of(2) {
        let maximum_gold = difficulty.max(1) as u32 * 10;
        let gold = 1 + (ctx.random::<u64>() % u64::from(maximum_gold)) as u32;
        if gold > 0
            && let Some(group) = &group
            && let Some(site) = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&group.case_site_id.value)
        {
            *combined
                .entry(crate::item::currency_id_for_settlement(
                    ctx,
                    &site.origin_settlement_id,
                )?)
                .or_default() += gold;
        }
    }
    for (item_id, quantity) in combined {
        ctx.db.battle_loot_item().insert(BattleLootItem {
            id: 0,
            loot_battle_id: battle_id.to_string(),
            item_id,
            quantity,
        });
    }
    if let Some(mut group) = group {
        group.disposition = match resolution {
            HostileResolutionKind::Defeated => HostileGroupDisposition::Defeated,
            HostileResolutionKind::DrivenOff => HostileGroupDisposition::DrivenOff,
            HostileResolutionKind::Captured => HostileGroupDisposition::Captured,
            HostileResolutionKind::CaptureTargetKilled => unreachable!(),
        };
        ctx.db.hostile_group_authority().id().update(group.clone());
        match resolution {
            HostileResolutionKind::Defeated => ingest_hostile_group_defeat_fact(
                ctx,
                outcome_source_id,
                party_id,
                &group,
                group.enemy_count,
            )?,
            HostileResolutionKind::DrivenOff => {
                let site = ctx
                    .db
                    .case_site_authority()
                    .id_key()
                    .find(&group.case_site_id.value)
                    .ok_or("Hostile group case site not found")?;
                ingest_case_outcome_fact(
                    ctx,
                    &format!("{outcome_source_id}:drive-off"),
                    &site.case_id,
                    party_id,
                    adventuresim_core::case::OutcomeFactKind::HostilesDrivenOff {
                        hostile_group_id: group.id.clone(),
                    },
                )?;
            }
            HostileResolutionKind::Captured => {
                let subject_id =
                    capture_subject_id.ok_or("Capture result has no mission-bound subject")?;
                let current = ctx
                    .db
                    .case_custody()
                    .object_id()
                    .find(&subject_id.to_string())
                    .ok_or("Captured subject has no custody authority")?;
                if current.case_id
                    != ctx
                        .db
                        .case_site_authority()
                        .id_key()
                        .find(&group.case_site_id.value)
                        .ok_or("Hostile group case site not found")?
                        .case_id
                    || current.holder_kind != CustodyHolderKind::Site
                    || current.holder_id != group.case_site_id.value
                {
                    return Err("Capture subject is not bound to this mission site and case".into());
                }
                transition_case_custody(
                    ctx,
                    &format!("{outcome_source_id}:capture"),
                    &current.case_id,
                    party_id,
                    CustodyObjectKind::Subject,
                    subject_id,
                    CustodyHolderKind::Party,
                    party_id,
                    current.version.saturating_add(1),
                    Some(adventuresim_core::case::OutcomeFactKind::SubjectCaptured {
                        subject_id: adventuresim_core::case::SubjectId::new(subject_id)
                            .map_err(|_| "Capture subject ID is invalid")?,
                    }),
                )?;
            }
            HostileResolutionKind::CaptureTargetKilled => unreachable!(),
        }
    }
    Ok(true)
}

#[reducer]
pub fn store_battle_loot(
    ctx: &ReducerContext,
    character_id: u64,
    battle_id: String,
    loot_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    adventuresim_core::mission::BattleId::new(battle_id.clone()).map_err(str::to_string)?;
    crate::character::require_living_character(ctx, character_id)?;
    if loot_item_ids.len() != quantities.len() {
        return Err("Loot entries must be aligned".into());
    }
    if loot_item_ids.iter().copied().collect::<HashSet<_>>().len() != loot_item_ids.len() {
        return Err("Duplicate battle loot IDs are not allowed".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let result = ctx
        .db
        .battle_result()
        .battle_id()
        .find(&battle_id)
        .ok_or("Battle result not found")?;
    if result.party_id != party_id {
        return Err("Battle loot belongs to another party".into());
    }
    let available: Vec<_> = ctx
        .db
        .battle_loot_item()
        .loot_battle_id()
        .filter(&battle_id)
        .collect();
    let loot: Vec<_> = if loot_item_ids.is_empty() {
        available
    } else {
        loot_item_ids
            .iter()
            .zip(&quantities)
            .map(|(id, quantity)| {
                let mut entry = available
                    .iter()
                    .find(|entry| entry.id == *id)
                    .cloned()
                    .ok_or("Loot item not found")?;
                if *quantity == 0 || *quantity > entry.quantity {
                    return Err("Invalid loot quantity".into());
                }
                entry.quantity = *quantity;
                Ok(entry)
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    let mut total_value = 0_u64;
    for entry in &loot {
        let entry_value = objective_item_value(ctx, &entry.item_id)?
            .checked_mul(u64::from(entry.quantity))
            .ok_or("Battle loot value overflow")?;
        total_value = total_value
            .checked_add(entry_value)
            .ok_or("Battle loot value overflow")?;
    }
    let recorded_participants: Vec<_> = ctx
        .db
        .battle_participant()
        .participant_battle_id()
        .filter(&battle_id)
        .map(|participant| participant.character_id)
        .collect();
    let living_recorded: Vec<_> = recorded_participants
        .iter()
        .copied()
        .filter(|participant_id| {
            ctx.db
                .character()
                .id()
                .find(*participant_id)
                .is_some_and(|character| character.alive)
        })
        .collect();
    let participants = adventuresim_core::battle_rewards::living_participant_ids(
        &recorded_participants,
        &living_recorded,
    );
    if participants.is_empty() {
        return Err("Battle has no eligible participants".into());
    }
    for entry in loot {
        add_to_party_inventory(ctx, &party_id, &entry.item_id, entry.quantity);
        let original = ctx
            .db
            .battle_loot_item()
            .id()
            .find(entry.id)
            .ok_or("Battle loot changed during transfer")?;
        if original.quantity == entry.quantity {
            ctx.db.battle_loot_item().id().delete(entry.id);
        } else {
            let mut original = original;
            original.quantity = original
                .quantity
                .checked_sub(entry.quantity)
                .ok_or("Battle loot quantity underflow")?;
            ctx.db.battle_loot_item().id().update(original);
        }
    }
    let participant_count = participants.len() as u64;
    let share = total_value / participant_count;
    for participant_id in participants {
        credit_party_stake(ctx, &party_id, participant_id, share)?;
    }
    credit_party_reserve(ctx, &party_id, total_value % participant_count)?;
    Ok(())
}

#[reducer]
pub fn deposit_party_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let mut inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Inventory item not found")?;
    if quantity == 0 || inventory.character_id != character_id || inventory.quantity < quantity {
        return Err("Invalid party inventory deposit".into());
    }
    if crate::character::inventory_item_is_equipped(ctx, character_id, inventory_item_id) {
        return Err("Unequip an item before depositing it".into());
    }
    let medication = item_is_medication(ctx, &inventory.item_id);
    if medication && (quantity != 1 || inventory.quantity != 1) {
        return Err("Medication must be deposited as an individual course".into());
    }
    let value = personal_inventory_value(ctx, &inventory, quantity)?;
    let durable = item_is_durable(ctx, &inventory.item_id);
    if durable && (quantity != 1 || inventory.quantity != 1) {
        return Err("Equipment instances must be deposited individually".into());
    }
    let preserved_condition = if durable {
        ctx.db
            .item_condition()
            .inventory_item_id()
            .find(inventory.id)
    } else {
        None
    };
    let food = crate::food::personal_lot(ctx, inventory.id).is_some();
    let measured = crate::inventory_amount::personal_amount(ctx, inventory.id).is_some();
    let inserted_party_row = if measured {
        if quantity != 1 || inventory.quantity != 1 {
            return Err("Measured items must be deposited as complete rows".into());
        }
        let party_row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
            id: 0,
            party_id: party_id.clone(),
            item_id: inventory.item_id.clone(),
            quantity,
        });
        if food {
            crate::food::move_or_split_to_party(
                ctx,
                inventory.id,
                party_row.id,
                quantity,
                inventory.quantity,
            )?;
        }
        crate::inventory_amount::move_personal_to_party(ctx, inventory.id, party_row.id)?;
        Some(party_row)
    } else {
        add_to_party_inventory(ctx, &party_id, &inventory.item_id, quantity);
        None
    };
    if let Some(condition) = preserved_condition {
        let party_row = inserted_party_row
            .or_else(|| {
                ctx.db
                    .party_inventory_item()
                    .party_id()
                    .filter(&party_id)
                    .filter(|row| row.item_id == inventory.item_id)
                    .max_by_key(|row| row.id)
            })
            .expect("durable party row was just inserted");
        ctx.db
            .party_item_condition()
            .party_inventory_item_id()
            .update(PartyItemCondition {
                party_inventory_item_id: party_row.id,
                tier_1: condition.tier_1,
                tier_2: condition.tier_2,
                tier_3: condition.tier_3,
                tier_4: condition.tier_4,
                tier_5: condition.tier_5,
            });
        ctx.db
            .item_condition()
            .inventory_item_id()
            .delete(inventory.id);
    }
    credit_party_stake(ctx, &party_id, character_id, value)?;
    if inventory.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory.id);
    } else {
        inventory.quantity -= quantity;
        ctx.db.inventory_item().id().update(inventory);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

pub(crate) fn consume_personal_gold(
    ctx: &ReducerContext,
    character_id: u64,
    amount: u64,
) -> Result<(), String> {
    crate::item::consume_personal_currency(ctx, character_id, amount)
}

pub(crate) fn party_currency_total(ctx: &ReducerContext, party_id: &str) -> u64 {
    ctx.db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .map(|stack| u64::from(stack.quantity))
        .sum()
}

pub(crate) fn consume_party_currency(
    ctx: &ReducerContext,
    party_id: &str,
    amount: u64,
) -> Result<(), String> {
    let mut stacks: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    if stacks
        .iter()
        .map(|stack| u64::from(stack.quantity))
        .sum::<u64>()
        < amount
    {
        return Err("Not enough party coin to cover this payment".into());
    }
    stacks.sort_by(|a, b| (&a.item_id, a.id).cmp(&(&b.item_id, b.id)));
    let mut remaining = amount;
    for mut stack in stacks {
        let taken = remaining.min(u64::from(stack.quantity)) as u32;
        stack.quantity -= taken;
        remaining -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.party_inventory_item().id().delete(stack.id);
        } else {
            ctx.db.party_inventory_item().id().update(stack);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

pub(crate) fn credit_party_currency(
    ctx: &ReducerContext,
    party_id: &str,
    settlement_id: &str,
    amount: u32,
) -> Result<(), String> {
    let currency_id = crate::item::currency_id_for_settlement(ctx, settlement_id)?;
    add_to_party_inventory(ctx, party_id, &currency_id, amount);
    Ok(())
}

fn transfer_personal_currency_to_party(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    amount: u64,
) -> Result<(), String> {
    let mut stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    if stacks.iter().map(|s| u64::from(s.quantity)).sum::<u64>() < amount {
        return Err("Not enough personal coin".into());
    }
    stacks.sort_by(|a, b| (&a.item_id, a.id).cmp(&(&b.item_id, b.id)));
    let mut remaining = amount;
    for mut stack in stacks {
        let taken = remaining.min(u64::from(stack.quantity)) as u32;
        add_to_party_inventory(ctx, party_id, &stack.item_id, taken);
        stack.quantity -= taken;
        remaining -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

fn transfer_party_currency_to_personal(
    ctx: &ReducerContext,
    party_id: &str,
    character_id: u64,
    amount: u64,
) -> Result<(), String> {
    let mut stacks: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    if stacks.iter().map(|s| u64::from(s.quantity)).sum::<u64>() < amount {
        return Err("The party has insufficient coin".into());
    }
    stacks.sort_by(|a, b| (&a.item_id, a.id).cmp(&(&b.item_id, b.id)));
    let mut remaining = amount;
    for mut stack in stacks {
        let taken = remaining.min(u64::from(stack.quantity)) as u32;
        crate::add_inventory_item(ctx, character_id, &stack.item_id, taken);
        stack.quantity -= taken;
        remaining -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.party_inventory_item().id().delete(stack.id);
        } else {
            ctx.db.party_inventory_item().id().update(stack);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

#[reducer]
pub fn withdraw_party_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    party_inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let mut inventory = ctx
        .db
        .party_inventory_item()
        .id()
        .find(party_inventory_item_id)
        .ok_or("Party inventory item not found")?;
    if quantity == 0 || inventory.party_id != party_id || inventory.quantity < quantity {
        return Err("Invalid party inventory withdrawal".into());
    }
    let cost = party_inventory_value(ctx, &inventory, quantity)?;
    let mut stake = ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .find(|stake| stake.character_id == character_id);
    let stake_value = stake.as_ref().map_or(0, |stake| stake.value);
    if cost > stake_value {
        let top_up = cost - stake_value;
        transfer_personal_currency_to_party(ctx, character_id, &party_id, top_up)?;
    }
    if let Some(ref mut stake) = stake {
        stake.value = stake.value.saturating_sub(cost);
        ctx.db.party_stake().id().update(stake.clone());
    }
    let durable = item_is_durable(ctx, &inventory.item_id);
    let medication = item_is_medication(ctx, &inventory.item_id);
    if medication && (quantity != 1 || inventory.quantity != 1) {
        return Err("Medication must be withdrawn as an individual course".into());
    }
    if durable && (quantity != 1 || inventory.quantity != 1) {
        return Err("Equipment instances must be withdrawn individually".into());
    }
    let preserved_condition = ctx
        .db
        .party_item_condition()
        .party_inventory_item_id()
        .find(inventory.id);
    let food = crate::food::party_lot(ctx, inventory.id).is_some();
    let measured = crate::inventory_amount::party_amount(ctx, inventory.id).is_some();
    let new_inventory_id = if measured {
        if quantity != 1 || inventory.quantity != 1 {
            return Err("Measured items must be withdrawn as complete rows".into());
        }
        let row = ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id,
            item_id: inventory.item_id.clone(),
            quantity,
        });
        if food {
            crate::food::move_or_split_to_personal(
                ctx,
                inventory.id,
                row.id,
                quantity,
                inventory.quantity,
            )?;
        }
        crate::inventory_amount::move_party_to_personal(ctx, inventory.id, row.id)?;
        Some(row.id)
    } else {
        crate::add_inventory_item(ctx, character_id, &inventory.item_id, quantity)
    };
    if let (Some(condition), Some(new_id)) = (preserved_condition, new_inventory_id) {
        ctx.db
            .item_condition()
            .inventory_item_id()
            .update(crate::repair::ItemCondition {
                inventory_item_id: new_id,
                tier_1: condition.tier_1,
                tier_2: condition.tier_2,
                tier_3: condition.tier_3,
                tier_4: condition.tier_4,
                tier_5: condition.tier_5,
            });
        ctx.db
            .party_item_condition()
            .party_inventory_item_id()
            .delete(inventory.id);
    }
    if inventory.quantity == quantity {
        ctx.db.party_inventory_item().id().delete(inventory.id);
    } else {
        inventory.quantity -= quantity;
        ctx.db.party_inventory_item().id().update(inventory);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn liquidate_party_inventory(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    party_inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if party_inventory_item_ids.is_empty() || party_inventory_item_ids.len() != quantities.len() {
        return Err("Liquidation entries must be non-empty and aligned".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Character must be at this settlement to liquidate party assets".into());
    }
    if require_settlement_service(
        ctx,
        &settlement_id,
        adventuresim_world_schema::SettlementService::Market,
    )
    .is_err()
    {
        require_settlement_service(
            ctx,
            &settlement_id,
            adventuresim_world_schema::SettlementService::GeneralStore,
        )?;
    }
    let party_id = character.party_id.ok_or("Character has no party")?;
    let mut staged = Vec::new();
    let mut proceeds = 0_u64;
    let mut seen = HashSet::new();
    for (&id, &quantity) in party_inventory_item_ids.iter().zip(&quantities) {
        if !seen.insert(id) {
            return Err("Party liquidation item IDs must be unique".into());
        }
        let entry = ctx
            .db
            .party_inventory_item()
            .id()
            .find(id)
            .ok_or("Party inventory item not found")?;
        if quantity == 0
            || entry.party_id != party_id
            || entry.quantity < quantity
            || crate::item::is_currency(ctx, &entry.item_id)
        {
            return Err("Invalid party asset liquidation".into());
        }
        let line_value = party_inventory_value(ctx, &entry, quantity)?;
        proceeds = proceeds
            .checked_add(line_value)
            .ok_or("Party asset liquidation total overflow")?;
        staged.push((entry, quantity));
    }
    let proceeds =
        u32::try_from(proceeds).map_err(|_| "Party asset liquidation exceeds currency limits")?;
    for (mut entry, quantity) in staged {
        let is_food = crate::food::party_lot(ctx, entry.id).is_some();
        if is_food {
            crate::food::remove_party_lot_quantity(ctx, entry.id, quantity, entry.quantity)?;
        }
        if entry.quantity == quantity {
            ctx.db
                .party_item_amount()
                .party_inventory_item_id()
                .delete(entry.id);
            ctx.db.party_inventory_item().id().delete(entry.id);
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(entry.id);
        } else {
            entry.quantity -= quantity;
            ctx.db.party_inventory_item().id().update(entry);
        }
    }
    credit_party_currency(ctx, &party_id, &settlement_id, proceeds)?;
    Ok(())
}

#[reducer]
pub fn discard_inventory_items(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    if inventory_item_ids.is_empty() || inventory_item_ids.len() != quantities.len() {
        return Err("Discarded item IDs and quantities must be non-empty and aligned".into());
    }
    if ctx.db.character().id().find(character_id).is_none() {
        return Err("Character not found".into());
    }
    let mut seen = HashSet::new();
    let mut staged = Vec::with_capacity(inventory_item_ids.len());
    for (&inventory_item_id, &quantity) in inventory_item_ids.iter().zip(&quantities) {
        if quantity == 0 || !seen.insert(inventory_item_id) {
            return Err("Discard quantities must be positive and item IDs unique".into());
        }
        let item = ctx
            .db
            .inventory_item()
            .id()
            .find(inventory_item_id)
            .ok_or("Inventory item not found")?;
        if item.character_id != character_id || item.quantity < quantity {
            return Err("Character does not have the staged quantity".into());
        }
        if crate::character::inventory_item_is_equipped(ctx, character_id, inventory_item_id) {
            return Err("Unequip an item before discarding it".into());
        }
        staged.push((item, quantity));
    }

    for (mut item, quantity) in staged {
        if item.quantity == quantity {
            ctx.db
                .inventory_item_amount()
                .inventory_item_id()
                .delete(item.id);
            ctx.db.inventory_item().id().delete(item.id);
            ctx.db.item_condition().inventory_item_id().delete(item.id);
            crate::food::delete_personal_food_lot(ctx, item.id);
        } else {
            crate::food::remove_lot_quantity(ctx, item.id, quantity, item.quantity)?;
            item.quantity -= quantity;
            ctx.db.inventory_item().id().update(item);
        }
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn finalize_party_offer(
    ctx: &ReducerContext,
    from_character_ids: Vec<u64>,
    to_character_ids: Vec<u64>,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    for character_id in from_character_ids.iter().chain(&to_character_ids) {
        require_character_no_unresolved_encounter(ctx, *character_id)?;
        crate::character::require_living_character(ctx, *character_id)?;
    }
    if from_character_ids.len() != to_character_ids.len()
        || from_character_ids.len() != inventory_item_ids.len()
        || from_character_ids.len() != quantities.len()
        || from_character_ids.is_empty()
    {
        return Err("Offer entries must be non-empty and aligned".into());
    }
    for index in 0..from_character_ids.len() {
        let from_id = from_character_ids[index];
        let to_id = to_character_ids[index];
        let quantity = quantities[index];
        let Some(from) = ctx.db.character().id().find(from_id) else {
            return Err("Source character not found".into());
        };
        let Some(to) = ctx.db.character().id().find(to_id) else {
            return Err("Recipient character not found".into());
        };
        let Some(item) = ctx.db.inventory_item().id().find(inventory_item_ids[index]) else {
            return Err("Inventory item not found".into());
        };
        if quantity == 0
            || from_id == to_id
            || from.party_id.is_none()
            || from.party_id != to.party_id
            || item.character_id != from_id
            || item.quantity < quantity
        {
            return Err("Invalid party trade offer".into());
        }
        if crate::character::inventory_item_is_equipped(ctx, from_id, item.id) {
            return Err("Unequip an item before offering it".into());
        }
    }
    for index in 0..from_character_ids.len() {
        transfer_party_item(
            ctx,
            from_character_ids[index],
            to_character_ids[index],
            inventory_item_ids[index],
            quantities[index],
        )?;
    }
    Ok(())
}

#[reducer]
pub fn finalize_merchant_trade(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    buy_item_ids: Vec<String>,
    buy_quantities: Vec<u32>,
    sell_inventory_ids: Vec<u64>,
    sell_quantities: Vec<u32>,
    party_scope: bool,
) -> Result<(), String> {
    let provider_npc_id = default_merchant_provider(ctx, &settlement_id, "merchants", "market")?;
    finalize_storefront_trade_impl(
        ctx,
        character_id,
        settlement_id,
        "merchants".into(),
        provider_npc_id,
        buy_item_ids,
        buy_quantities,
        sell_inventory_ids,
        sell_quantities,
        party_scope,
    )
}

#[reducer]
pub fn finalize_storefront_trade(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    service_id: String,
    provider_npc_id: String,
    buy_item_ids: Vec<String>,
    buy_quantities: Vec<u32>,
    sell_inventory_ids: Vec<u64>,
    sell_quantities: Vec<u32>,
    party_scope: bool,
) -> Result<(), String> {
    finalize_storefront_trade_impl(
        ctx,
        character_id,
        settlement_id,
        service_id,
        provider_npc_id,
        buy_item_ids,
        buy_quantities,
        sell_inventory_ids,
        sell_quantities,
        party_scope,
    )
}

fn merchant_storefront(
    service_id: &str,
) -> Result<
    (
        adventuresim_core::settlement_economy::Storefront,
        &'static str,
    ),
    String,
> {
    use adventuresim_core::settlement_economy::Storefront;
    match service_id {
        "merchants" => Ok((Storefront::General, "market")),
        "weapons" => Ok((Storefront::Weapons, "forge")),
        "armor" => Ok((Storefront::Armor, "armoury")),
        "clothing" => Ok((Storefront::Clothing, "tailor")),
        "inn" => Ok((Storefront::Inn, "inn")),
        "books" => Ok((Storefront::Books, "bookstore")),
        _ => Err("Unknown merchant storefront".into()),
    }
}

fn default_merchant_provider(
    ctx: &ReducerContext,
    settlement_id: &str,
    service_id: &str,
    location_id: &str,
) -> Result<String, String> {
    unique_default_merchant_provider(
        ctx.db
            .settlement_npc()
            .iter()
            .filter(|npc| npc.home_settlement_id == settlement_id && npc.service_id == service_id)
            .filter_map(|npc| {
                ctx.db
                    .settlement_npc_presence()
                    .npc_id()
                    .find(&npc.id)
                    .filter(|presence| {
                        presence.settlement_id == settlement_id
                            && presence.location_id == location_id
                            && presence.is_default
                    })
                    .map(|_| npc.id)
            }),
    )
}

fn unique_default_merchant_provider(
    providers: impl IntoIterator<Item = String>,
) -> Result<String, String> {
    let providers = providers.into_iter().collect::<Vec<_>>();
    match providers.as_slice() {
        [provider] => Ok(provider.clone()),
        [] => Err("Merchant service provider not found".into()),
        _ => Err("Merchant service provider is ambiguous".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn finalize_storefront_trade_impl(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    service_id: String,
    provider_npc_id: String,
    buy_item_ids: Vec<String>,
    buy_quantities: Vec<u32>,
    sell_inventory_ids: Vec<u64>,
    sell_quantities: Vec<u32>,
    party_scope: bool,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let (storefront, location_id) = merchant_storefront(&service_id)?;
    if buy_item_ids.len() != buy_quantities.len()
        || sell_inventory_ids.len() != sell_quantities.len()
    {
        return Err("Trade entries must be aligned".into());
    }
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    if character.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Character must be at this settlement to trade".into());
    }
    let party_id = character.party_id.clone();
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Settlement not found")?;
    if !adventuresim_core::settlement_economy::storefront_available(&settlement.economy, storefront)
    {
        return Err("This settlement does not offer that service".into());
    }
    let provider = ctx
        .db
        .settlement_npc()
        .id()
        .find(&provider_npc_id)
        .ok_or("Merchant service provider not found")?;
    let provider_presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&provider_npc_id)
        .ok_or("Merchant service provider has no presence")?;
    let problem_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    if provider.home_settlement_id != settlement_id
        || provider.service_id != service_id
        || provider_presence.settlement_id != settlement_id
        || provider_presence.location_id != location_id
        || !provider_presence.is_default
        || !crate::settlement_population::npc_is_present(&provider_presence, problem_minute)
    {
        return Err("Merchant service provider is not available".into());
    }
    if default_merchant_provider(ctx, &settlement_id, &service_id, location_id)? != provider_npc_id
    {
        return Err("Merchant service provider does not match this storefront".into());
    }
    let speaker = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?
        .oral_languages;
    let mut merchant = adventuresim_world_schema::OralLanguageHours::default();
    *merchant.direct_mut(settlement.languages.dominant_german()) =
        adventuresim_world_schema::ORAL_FLUENCY_HOURS;
    let speaker_cap = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    let (_, shared_language) = adventuresim_world_schema::best_common_oral_language_capped(
        speaker,
        speaker_cap,
        merchant,
        adventuresim_world_schema::ORAL_FLUENCY_HOURS,
    );
    let settlement_economy = settlement.economy.clone();
    let problem_effects =
        crate::local_problem::settlement_effects(ctx, &settlement_id, problem_minute);
    // Sales are inventory-instance operations. Preserve each submitted stack
    // and quantity rather than netting by item ID, which can assign the whole
    // net sale to every matching stack.
    let mut seen_sale_ids = HashSet::new();
    if !sell_inventory_ids
        .iter()
        .all(|inventory_id| seen_sale_ids.insert(*inventory_id))
    {
        return Err("Merchant sale inventory IDs must be unique".into());
    }
    let mut cost = 0_u64;
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        let Some(item) = ctx.db.item().id().find(item_id) else {
            return Err("Merchant item not found".into());
        };
        if matches!(
            item.kind,
            crate::ItemKind::Currency | crate::ItemKind::Medication
        ) || *quantity == 0
        {
            return Err("Invalid merchant purchase".into());
        }
        crate::item::inventory_food_definition(Some(item.kind), item_id)?;
        let catalog_kind = crate::item::economy_catalog_kind(item.kind);
        if !adventuresim_core::settlement_economy::storefront_stocks(
            &settlement_economy,
            storefront,
            item_id,
            catalog_kind,
        ) || (storefront == adventuresim_core::settlement_economy::Storefront::Books
            && adventuresim_core::item_catalog::definition(item_id)
                .and_then(|definition| definition.capabilities.book.as_ref())
                .is_none_or(|book| {
                    !book.settlement_allowlist.is_empty()
                        && !book.settlement_allowlist.contains(&settlement_id)
                }))
        {
            return Err("This settlement does not stock that merchant item".into());
        }
        let quoted = adventuresim_core::strategic_economy::language_adjusted_buy_price(
            adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1)),
            shared_language,
        );
        let quoted =
            adventuresim_core::local_problem::adjust_price(quoted, problem_effects.buy_bps);
        let line =
            adventuresim_core::strategic_economy::checked_merchant_line_total(quoted, *quantity)
                .ok_or("Merchant purchase total overflow")?;
        cost = adventuresim_core::strategic_economy::checked_add_merchant_total(cost, line)
            .ok_or("Merchant purchase total overflow")?;
    }
    let mut proceeds = 0_u64;
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let (item_id, available, food_value) = if party_scope {
            let inventory = ctx
                .db
                .party_inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Party inventory item not found")?;
            if Some(&inventory.party_id) != party_id.as_ref() {
                return Err("Invalid party inventory sale".into());
            }
            let food_value = crate::food::party_lot(ctx, inventory.id).map(|lot| lot.total_value);
            (inventory.item_id, inventory.quantity, food_value)
        } else {
            let inventory = ctx
                .db
                .inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Inventory item not found")?;
            if inventory.character_id != character_id {
                return Err("Invalid merchant sale".into());
            }
            let food_value =
                crate::food::personal_lot(ctx, inventory.id).map(|lot| lot.total_value);
            (inventory.item_id, inventory.quantity, food_value)
        };
        let Some(item) = ctx.db.item().id().find(&item_id) else {
            return Err("Item definition not found".into());
        };
        if available < *quantity
            || *quantity == 0
            || matches!(
                item.kind,
                crate::ItemKind::Currency | crate::ItemKind::Medication
            )
        {
            return Err("Invalid merchant sale".into());
        }
        if !party_scope
            && crate::character::inventory_item_is_equipped(ctx, character_id, *inventory_id)
        {
            return Err("Unequip an item before selling it".into());
        }
        let line = if let Some(value) = food_value {
            if *quantity != available || !value.is_finite() || value < 0.0 {
                return Err("Food batches must be sold as complete valid lots".into());
            }
            let base = adventuresim_core::strategic_economy::merchant_sell_food_lot_value(value)
                .ok_or("Food lot has invalid value")?;
            let quoted = adventuresim_core::strategic_economy::language_adjusted_sell_price(
                u32::try_from(base).map_err(|_| "Food lot quote overflow")?,
                shared_language,
            );
            u64::from(adventuresim_core::local_problem::adjust_price(
                quoted,
                -problem_effects.sell_penalty_bps,
            ))
        } else {
            let measured_amount = if party_scope {
                crate::inventory_amount::party_amount(ctx, *inventory_id)
            } else {
                crate::inventory_amount::personal_amount(ctx, *inventory_id)
            };
            if measured_amount.is_some() && *quantity != available {
                return Err("Measured inventory must be sold as a complete row".into());
            }
            let intrinsic = measured_amount.map_or(item.base_value.unwrap_or(1), |amount| {
                adventuresim_core::inventory_measurement::scaled_by_amount(
                    u64::from(item.base_value.unwrap_or(1)),
                    amount,
                )
                .min(u64::from(u32::MAX)) as u32
            });
            let merchant_value = if measured_amount.is_some() && intrinsic == 0 {
                0
            } else {
                adventuresim_core::strategic_economy::merchant_sell_price(intrinsic)
            };
            let quoted = adventuresim_core::strategic_economy::language_adjusted_sell_price(
                merchant_value,
                shared_language,
            );
            let quoted = adventuresim_core::local_problem::adjust_price(
                quoted,
                -problem_effects.sell_penalty_bps,
            );
            adventuresim_core::strategic_economy::checked_merchant_line_total(quoted, *quantity)
                .ok_or("Merchant sale total overflow")?
        };
        proceeds = adventuresim_core::strategic_economy::checked_add_merchant_total(proceeds, line)
            .ok_or("Merchant sale total overflow")?;
    }
    let coins = if party_scope {
        party_currency_total(ctx, party_id.as_ref().ok_or("Character has no party")?)
            .checked_add(crate::item::personal_currency_total(ctx, character_id))
            .ok_or("Merchant balance overflow")?
    } else {
        crate::item::personal_currency_total(ctx, character_id)
    };
    if coins
        .checked_add(proceeds)
        .ok_or("Merchant balance overflow")?
        < cost
    {
        return Err("Not enough coin".into());
    }
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        if party_scope {
            let mut inventory = ctx
                .db
                .party_inventory_item()
                .id()
                .find(*inventory_id)
                .unwrap();
            if inventory.quantity == *quantity {
                crate::food::delete_party_food_lot(ctx, *inventory_id);
                ctx.db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .delete(*inventory_id);
                ctx.db.party_inventory_item().id().delete(*inventory_id);
                ctx.db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .delete(*inventory_id);
            } else {
                if crate::food::party_lot(ctx, inventory.id).is_some() {
                    crate::food::remove_party_lot_quantity(
                        ctx,
                        *inventory_id,
                        *quantity,
                        inventory.quantity,
                    )?;
                }
                inventory.quantity -= quantity;
                ctx.db.party_inventory_item().id().update(inventory);
            }
        } else {
            let mut inventory = ctx.db.inventory_item().id().find(*inventory_id).unwrap();
            if inventory.quantity == *quantity {
                crate::food::delete_personal_food_lot(ctx, *inventory_id);
                ctx.db
                    .inventory_item_amount()
                    .inventory_item_id()
                    .delete(*inventory_id);
                ctx.db.inventory_item().id().delete(*inventory_id);
                ctx.db
                    .item_condition()
                    .inventory_item_id()
                    .delete(*inventory_id);
            } else {
                if crate::food::personal_lot(ctx, inventory.id).is_some() {
                    crate::food::remove_lot_quantity(
                        ctx,
                        *inventory_id,
                        *quantity,
                        inventory.quantity,
                    )?;
                }
                inventory.quantity -= quantity;
                ctx.db.inventory_item().id().update(inventory);
            }
        }
    }
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        if party_scope {
            add_to_party_inventory_checked(ctx, party_id.as_ref().unwrap(), item_id, *quantity)?;
            continue;
        }
        // Never add purchases to an equipped stack. An equipped item must stay
        // independently sellable from an otherwise identical spare item.
        let durable = ctx.db.item().id().find(item_id).is_some_and(|definition| {
            matches!(
                definition.kind,
                crate::ItemKind::Weapon
                    | crate::ItemKind::Armor
                    | crate::ItemKind::Shield
                    | crate::ItemKind::Clothing
            )
        });
        let food = ctx
            .db
            .item()
            .id()
            .find(item_id)
            .is_some_and(|definition| definition.kind == crate::ItemKind::Food)
            || adventuresim_core::food::definition(item_id).is_some();
        if !durable
            && !food
            && let Some(mut stack) = ctx
                .db
                .inventory_item()
                .character_and_item_id()
                .filter((character_id, item_id))
                .find(|stack| {
                    !crate::character::inventory_item_is_equipped(ctx, character_id, stack.id)
                })
        {
            if let Some(merged) = stack.quantity.checked_add(*quantity) {
                stack.quantity = merged;
                ctx.db.inventory_item().id().update(stack);
            } else {
                crate::item::add_inventory_item_checked(ctx, character_id, item_id, *quantity)?
                    .ok_or("Merchant purchase created no inventory item")?;
            }
        } else {
            crate::item::add_inventory_item_checked(ctx, character_id, item_id, *quantity)?
                .ok_or("Merchant purchase created no inventory item")?;
        }
    }
    let (owes, receives) = if cost >= proceeds {
        (cost - proceeds, 0)
    } else {
        (0, proceeds - cost)
    };
    if party_scope && receives > 0 {
        let party_id = party_id.as_ref().unwrap();
        credit_party_currency(
            ctx,
            party_id,
            &settlement_id,
            u32::try_from(receives).map_err(|_| "Merchant proceeds exceed inventory capacity")?,
        )?;
    } else if party_scope && owes > 0 {
        let party_id = party_id.as_ref().unwrap();
        let party_coins = party_currency_total(ctx, party_id);
        let personal_coins = crate::item::personal_currency_total(ctx, character_id);
        let (pooled, personal) =
            adventuresim_core::strategic_economy::split_party_purchase_payment(
                party_coins,
                personal_coins,
                owes,
            )
            .ok_or("Not enough coin")?;
        consume_party_currency(ctx, party_id, pooled)?;
        consume_personal_gold(ctx, character_id, personal)?;
        if personal > 0 {
            credit_party_stake(ctx, party_id, character_id, personal)?;
        }
    } else if owes > 0 {
        consume_personal_gold(ctx, character_id, owes)?;
    } else if receives > 0 {
        crate::item::credit_personal_currency(
            ctx,
            character_id,
            &settlement_id,
            u32::try_from(receives).map_err(|_| "Merchant proceeds exceed inventory capacity")?,
        )?;
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn leave_party(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    remove_party_member(ctx, character_id, character_id)
}

/// Removes a non-leader member. Leaders may remove their members and non-leaders
/// may remove themselves; a leader must disband rather than remove themselves.
#[reducer]
pub fn remove_party_member(
    ctx: &ReducerContext,
    actor_character_id: u64,
    member_character_id: u64,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, actor_character_id)?;
    crate::character::require_living_character(ctx, actor_character_id)?;
    let Some(actor) = ctx.db.character().id().find(actor_character_id) else {
        return Err("Acting character not found".into());
    };
    let Some(mut character) = ctx.db.character().id().find(member_character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Character is not in a party".into());
    };

    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if actor.party_id.as_deref() != Some(&party_id) {
        return Err("Characters are not in the same party".into());
    }
    if party.leader_id == member_character_id {
        return Err("Party leader cannot leave. Use disband_party instead.".into());
    }
    if actor_character_id != member_character_id && party.leader_id != actor_character_id {
        return Err("Only the party leader may remove another member".into());
    }
    if actor_character_id == party.leader_id && character.temporary {
        settle_temporary_member_stake(ctx, &party_id, member_character_id)?;
    }
    if ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .any(|stake| stake.character_id == member_character_id && stake.value > 0)
    {
        return Err("Withdraw this character's party stake before leaving".into());
    }

    if let Some(membership) = ctx
        .db
        .party_member()
        .character_id()
        .filter(member_character_id)
        .find(|m| m.party_id == party_id)
    {
        ctx.db.party_member().id().delete(membership.id);
    }

    crate::social::settle_shared_party_time(ctx, member_character_id);
    crate::social::close_physiology_presence(ctx, member_character_id);
    character.party_id = None;
    ctx.db.character().id().update(character);
    for vote in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        if vote.voter_id == member_character_id || vote.candidate_id == member_character_id {
            ctx.db.party_leader_vote().id().delete(&vote.id);
        }
    }
    normalize_and_elect_party_leader(ctx, &party_id)?;
    create_solo_party_for_character(ctx, member_character_id)?;
    crate::social::prune_invalid_automatic_social_chats(ctx);
    Ok(())
}

/// Generated companions retain the value they contributed to the shared pool
/// when the leader dismisses them. Use the normal gold-withdrawal path before
/// removing them, rather than silently deleting their stake.
fn settle_temporary_member_stake(
    ctx: &ReducerContext,
    party_id: &str,
    member_character_id: u64,
) -> Result<(), String> {
    let stake_value = ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .find(|stake| stake.character_id == member_character_id)
        .map_or(0, |stake| stake.value);
    if stake_value == 0 {
        return Ok(());
    }
    transfer_party_currency_to_personal(ctx, party_id, member_character_id, stake_value)
}

#[reducer]
pub fn disband_party(ctx: &ReducerContext, leader_id: u64, party_id: String) -> Result<(), String> {
    require_no_unresolved_encounter(ctx, &party_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can disband the party".into());
    }
    if party
        .active_contract_id
        .as_ref()
        .is_some_and(|contract_id| {
            ctx.db
                .contract_authority()
                .id()
                .find(contract_id)
                .is_some_and(|contract| contract.status == ContractStatus::ReadyToReport)
        })
    {
        return Err("Report the completed contract before disbanding the party".into());
    }
    if party.current_case_site_id.is_some() {
        return Err("Travel to a settlement before disbanding the party".into());
    }
    if ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .any(|stake| stake.value > 0)
    {
        return Err("Settle every member's party stake before disbanding".into());
    }
    let pooled_items: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(&party_id)
        .collect();
    let reserve = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id)
        .map_or(0, |state| state.reserve_value);
    if pooled_items
        .iter()
        .any(|entry| !crate::item::is_currency(ctx, &entry.item_id))
        || pooled_items
            .iter()
            .map(|entry| u64::from(entry.quantity))
            .sum::<u64>()
            != reserve
    {
        return Err("Liquidate and distribute the party inventory before disbanding".into());
    }
    if reserve > 0 {
        transfer_party_currency_to_personal(ctx, &party_id, party.leader_id, reserve)?;
    }
    for entry in pooled_items {
        ctx.db.party_inventory_item().id().delete(entry.id);
    }
    if ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db.party_inventory_state().party_id().delete(&party_id);
    }
    for stake in ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_stake().id().delete(stake.id);
    }

    let members: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();
    let member_ids: Vec<_> = members.iter().map(|member| member.character_id).collect();
    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(member.character_id) {
            crate::social::settle_shared_party_time(ctx, member.character_id);
            crate::social::close_physiology_presence(ctx, member.character_id);
            character.party_id = None;
            ctx.db.character().id().update(character);
        }
        ctx.db.party_member().id().delete(member.id);
    }

    let requests: Vec<_> = ctx
        .db
        .party_join_request()
        .party_id()
        .filter(&party_id)
        .collect();
    for request in requests {
        ctx.db.party_join_request().id().delete(request.id);
    }
    for role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_recruitment_role().id().delete(role.id);
    }

    if let Some(contract_id) = party.active_contract_id
        && let Some(mut contract) = ctx.db.contract_authority().id().find(&contract_id)
    {
        contract.status = ContractStatus::Withdrawn;
        ctx.db.contract_authority().id().update(contract);
    }

    ctx.db.party_authority().id().delete(&party_id);
    for character_id in member_ids {
        create_solo_party_for_character(ctx, character_id)?;
    }
    crate::social::prune_invalid_automatic_social_chats(ctx);
    Ok(())
}
