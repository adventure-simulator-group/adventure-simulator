//! Persistent strategic equipment condition and settlement repair custody.

use adventuresim_core::durability::DamageBins;
use adventuresim_core::durability::{DurabilityProfile, damage_from_impact};
use adventuresim_core::physical_object::{CarriedInventoryScope, InventoryLocation};
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::character::{character, character_equipped_item, equipment_occupancy};
use crate::item::{inventory_item, item};
use crate::simulation::simulation_character;
use crate::time::character_time;
use crate::{InventoryItem, PersistedItemKind, inventory_object};

pub const REPAIR_MINUTES_PER_FULL_ITEM: u64 = 2 * MINUTES_PER_DAY;

#[derive(Clone, Debug)]
#[table(accessor = item_condition, public)]
pub struct ItemCondition {
    #[primary_key]
    pub inventory_item_id: u64,
    pub tier_1: f32,
    pub tier_2: f32,
    pub tier_3: f32,
    pub tier_4: f32,
    pub tier_5: f32,
}

impl ItemCondition {
    pub fn bins(&self) -> DamageBins {
        DamageBins([
            self.tier_1,
            self.tier_2,
            self.tier_3,
            self.tier_4,
            self.tier_5,
        ])
        .normalized()
    }
    fn set_bins(&mut self, bins: DamageBins) {
        [
            self.tier_1,
            self.tier_2,
            self.tier_3,
            self.tier_4,
            self.tier_5,
        ] = bins.normalized().0;
    }
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_smith, public)]
pub struct SettlementSmith {
    #[primary_key]
    pub settlement_id: String,
    pub weaponsmith_skill: u8,
    pub armourer_skill: u8,
    pub tailor_skill: u8,
}

#[derive(Clone, Debug)]
#[table(
    accessor = repair_order, public,
    index(accessor = owner_id, btree(columns = [owner_character_id])),
    index(accessor = settlement_id, btree(columns = [settlement_id]))
)]
pub struct RepairOrder {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner_character_id: u64,
    #[unique]
    pub inventory_item_id: u64,
    pub item_id: String,
    pub settlement_id: String,
    pub smith_skill: u8,
    pub submitted_at_minutes: u64,
    pub ready_at_minutes: u64,
    pub target_condition: f32,
    pub equipped_placement_id: Option<String>,
    pub attachment_targets: Vec<crate::character::EquipmentAttachmentTargetSelection>,
    /// Stable quote charged when the completed job is retrieved.
    #[default(0)]
    pub quoted_cost: u32,
}

fn repair_service(value: &str) -> Result<adventuresim_core::durability::RepairService, String> {
    adventuresim_core::durability::RepairService::parse(value)
        .ok_or_else(|| "Unknown repair service".into())
}

fn repair_kind(kind: PersistedItemKind) -> Option<adventuresim_core::durability::RepairItemKind> {
    use adventuresim_core::durability::RepairItemKind;
    match kind {
        PersistedItemKind::Weapon => Some(RepairItemKind::Weapon),
        PersistedItemKind::Shield => Some(RepairItemKind::Shield),
        PersistedItemKind::Armor => Some(RepairItemKind::Armor),
        PersistedItemKind::Clothing => Some(RepairItemKind::Clothing),
        _ => None,
    }
}

fn service_matches(
    service: adventuresim_core::durability::RepairService,
    kind: PersistedItemKind,
) -> bool {
    repair_kind(kind).is_some_and(|kind| service.matches(kind))
}

pub(crate) fn initialize_item_condition(ctx: &ReducerContext, inventory: &InventoryItem) {
    let Some(definition) = ctx.db.item().id().find(&inventory.item_id) else {
        return;
    };
    if !definition.repairable
        || ctx
            .db
            .item_condition()
            .inventory_item_id()
            .find(inventory.id)
            .is_some()
    {
        return;
    }
    ctx.db.item_condition().insert(ItemCondition {
        inventory_item_id: inventory.id,
        tier_1: 0.0,
        tier_2: 0.0,
        tier_3: 0.0,
        tier_4: 0.0,
        tier_5: 0.0,
    });
}

fn stable_skill(settlement_id: &str, salt: u64) -> u8 {
    let mut hash = 0xcbf29ce484222325_u64 ^ salt;
    for byte in settlement_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    3 + (hash % 3) as u8
}

pub(crate) fn ensure_settlement_smith(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> SettlementSmith {
    if let Some(row) = ctx
        .db
        .settlement_smith()
        .settlement_id()
        .find(settlement_id.to_owned())
    {
        return row;
    }
    ctx.db.settlement_smith().insert(SettlementSmith {
        settlement_id: settlement_id.to_owned(),
        weaponsmith_skill: stable_skill(settlement_id, 0x5745_4150),
        armourer_skill: stable_skill(settlement_id, 0x4152_4d52),
        tailor_skill: stable_skill(settlement_id, 0x5441_494c),
    })
}

fn service_skill(
    ctx: &ReducerContext,
    settlement_id: &str,
    kind: PersistedItemKind,
) -> Result<u8, String> {
    use adventuresim_world_schema::SettlementService as S;
    let specialist = match kind {
        PersistedItemKind::Weapon | PersistedItemKind::Shield => S::Weaponsmith,
        PersistedItemKind::Armor => S::Armorer,
        PersistedItemKind::Clothing => S::Tailor,
        _ => return Err("This service does not repair that item kind".into()),
    };
    if crate::strategic::require_settlement_service(ctx, settlement_id, specialist).is_err() {
        crate::strategic::require_settlement_service(ctx, settlement_id, S::GeneralBlacksmith)?;
    }
    let service = ensure_settlement_smith(ctx, settlement_id);
    match kind {
        PersistedItemKind::Weapon | PersistedItemKind::Shield => Ok(service.weaponsmith_skill),
        PersistedItemKind::Armor => Ok(service.armourer_skill),
        PersistedItemKind::Clothing => Ok(service.tailor_skill),
        _ => Err("This service does not repair that item kind".into()),
    }
}

fn submit(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    service: adventuresim_core::durability::RepairService,
    inventory_item_id: u64,
) -> Result<u64, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(settlement_id) {
        return Err("Character must be at the originating settlement".into());
    }
    let mut inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Inventory item not found")?;
    if ctx
        .db
        .repair_order()
        .inventory_item_id()
        .find(inventory_item_id)
        .is_some()
    {
        return Err("Item already has an active repair order".into());
    }
    if inventory.character_id != character_id || inventory.quantity != 1 {
        return Err("Repair custody requires one owned equipment instance".into());
    }
    let definition = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .ok_or("Item definition not found")?;
    if !service_matches(service, definition.kind) {
        return Err("That item does not belong to this repair service".into());
    }
    let skill = service_skill(ctx, settlement_id, definition.kind)?;
    let condition = ctx
        .db
        .item_condition()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or("Equipment condition not found")?;
    let bins = condition.bins();
    let repairable = bins.repairable(skill);
    if repairable <= f32::EPSILON {
        return Err(if bins.total() <= f32::EPSILON {
            "Item is not damaged"
        } else {
            "All damage is beyond this craftsperson's skill"
        }
        .into());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|v| v.minutes)
        .unwrap_or(0);
    let minutes = (repairable * REPAIR_MINUTES_PER_FULL_ITEM as f32).ceil() as u64;
    let quoted_cost =
        adventuresim_core::durability::repair_quote(definition.base_value.unwrap_or(1), repairable);
    let equipped = ctx
        .db
        .character_equipped_item()
        .inventory_item_id()
        .find(inventory_item_id);
    let attachment_targets = if equipped.is_some() {
        crate::character::require_no_equipped_children(ctx, inventory_item_id)?;
        saved_attachment_targets(
            ctx.db
                .equipment_occupancy()
                .inventory_item_id()
                .filter(inventory_item_id),
        )
    } else {
        Vec::new()
    };
    if let Some(mut object) = crate::inventory_container::object_for_row(
        ctx,
        CarriedInventoryScope::Personal,
        inventory_item_id,
    )? {
        crate::inventory_container::detach_if_nested(ctx, object.id)?;
        object.location = InventoryLocation::repair(settlement_id, inventory_item_id);
        ctx.db.inventory_object().id().update(object);
    } else {
        crate::inventory_container::reconcile_consumed_row(
            ctx,
            CarriedInventoryScope::Personal,
            inventory_item_id,
            true,
        )?;
    }
    crate::character::unequip_wearable(ctx, inventory_item_id);
    // Zero is reserved for smith custody and is excluded by every owner-scoped inventory path.
    inventory.character_id = 0;
    ctx.db.inventory_item().id().update(inventory.clone());
    let target_condition = (bins.condition() + repairable).min(1.0);
    let order = ctx.db.repair_order().insert(RepairOrder {
        id: 0,
        owner_character_id: character_id,
        inventory_item_id,
        item_id: inventory.item_id,
        settlement_id: settlement_id.to_owned(),
        smith_skill: skill,
        submitted_at_minutes: now,
        ready_at_minutes: now.saturating_add(minutes.max(1)),
        target_condition,
        equipped_placement_id: equipped.map(|row| row.placement_id),
        attachment_targets,
        quoted_cost,
    });
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(order.id)
}

fn saved_attachment_targets(
    rows: impl IntoIterator<Item = crate::character::EquipmentOccupancy>,
) -> Vec<crate::character::EquipmentAttachmentTargetSelection> {
    let mut targets = rows
        .into_iter()
        .filter_map(|row| {
            Some(crate::character::EquipmentAttachmentTargetSelection {
                requirement_index: row.requirement_index,
                parent_inventory_item_id: row.parent_inventory_item_id?,
                attachment_point_id: row.attachment_point_id?,
            })
        })
        .collect::<Vec<_>>();
    targets.sort_by_key(|target| target.requirement_index);
    targets
}

#[reducer]
pub fn submit_item_for_repair(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    service: String,
    inventory_item_id: u64,
) -> Result<(), String> {
    submit(
        ctx,
        character_id,
        &settlement_id,
        repair_service(&service)?,
        inventory_item_id,
    )
    .map(|_| ())
}

#[reducer]
pub fn submit_all_repairable_items(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    service: String,
) -> Result<(), String> {
    let service = repair_service(&service)?;
    let ids: Vec<u64> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter_map(|inventory| {
            let kind = ctx.db.item().id().find(&inventory.item_id)?.kind;
            service_matches(service, kind).then_some(inventory.id)
        })
        .collect();
    let mut submitted = 0;
    for id in ids {
        if submit(ctx, character_id, &settlement_id, service, id).is_ok() {
            submitted += 1;
        }
    }
    if submitted == 0 {
        return Err("No matching item has damage this service can repair".into());
    }
    Ok(())
}

#[reducer]
pub fn retrieve_repaired_item(
    ctx: &ReducerContext,
    character_id: u64,
    order_id: u64,
) -> Result<(), String> {
    retrieve(ctx, character_id, order_id)
}

fn retrieve(ctx: &ReducerContext, character_id: u64, order_id: u64) -> Result<(), String> {
    let order = ctx
        .db
        .repair_order()
        .id()
        .find(order_id)
        .ok_or("Repair order not found")?;
    if order.owner_character_id != character_id {
        return Err("Only the owner may retrieve this item".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(&order.settlement_id) {
        return Err("Return to the originating settlement to retrieve this item".into());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|v| v.minutes)
        .unwrap_or(0);
    if now < order.ready_at_minutes {
        return Err("Repair is not complete yet".into());
    }
    crate::strategic::consume_personal_gold(ctx, character_id, u64::from(order.quoted_cost))?;
    let mut inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(order.inventory_item_id)
        .ok_or("Escrowed item not found")?;
    if !adventuresim_core::durability::valid_repair_escrow_row(
        inventory.character_id,
        inventory.quantity,
        &order.item_id,
        &inventory.item_id,
    ) {
        return Err("Repair escrow state is inconsistent".into());
    }
    let mut condition = ctx
        .db
        .item_condition()
        .inventory_item_id()
        .find(order.inventory_item_id)
        .ok_or("Equipment condition not found")?;
    let mut bins = condition.bins();
    bins.repair_through(order.smith_skill);
    condition.set_bins(bins);
    ctx.db
        .item_condition()
        .inventory_item_id()
        .update(condition);
    inventory.character_id = character_id;
    ctx.db.inventory_item().id().update(inventory);
    let mut escrow_objects = ctx
        .db
        .inventory_object()
        .iter()
        .filter(|object| {
            matches!(
                &object.location,
                InventoryLocation::Repair(location)
                    if location.row_id == order.inventory_item_id
            )
        })
        .collect::<Vec<_>>();
    if escrow_objects.len() > 1 {
        return Err("Repair escrow has duplicate physical object identities".into());
    }
    if let Some(mut object) = escrow_objects.pop() {
        let correct_escrow = matches!(
            &object.location,
            InventoryLocation::Repair(location)
                if location.settlement_id == order.settlement_id
        );
        if !correct_escrow || object.item_id != order.item_id {
            return Err("Repair escrow physical object does not match its order".into());
        }
        object.location = InventoryLocation::personal(character_id, order.inventory_item_id);
        ctx.db.inventory_object().id().update(object);
    }
    if let Some(placement_id) = order.equipped_placement_id.as_deref() {
        crate::character::restore_equipment_placement(
            ctx,
            character_id,
            order.inventory_item_id,
            placement_id,
            order.attachment_targets.clone(),
        )
        .map_err(|error| {
            format!("Repair is complete, but its saved equipment graph cannot be restored: {error}")
        })?;
    }
    ctx.db.repair_order().id().delete(order_id);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn retrieve_repaired_items(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    service: String,
    item_id: Option<String>,
    limit: u32,
) -> Result<(), String> {
    let service = repair_service(&service)?;
    if limit == 0 {
        return Err("Retrieve count must be positive".into());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|value| value.minutes)
        .unwrap_or(0);
    let mut ids: Vec<_> = ctx
        .db
        .repair_order()
        .owner_id()
        .filter(character_id)
        .filter(|order| {
            order.settlement_id == settlement_id
                && order.ready_at_minutes <= now
                && item_id
                    .as_ref()
                    .is_none_or(|item_id| item_id == &order.item_id)
                && ctx
                    .db
                    .item()
                    .id()
                    .find(&order.item_id)
                    .is_some_and(|item| service_matches(service, item.kind))
        })
        .map(|order| (order.submitted_at_minutes, order.id, order.quoted_cost))
        .collect();
    ids.sort_unstable();
    ids.truncate(limit as usize);
    if ids.is_empty() {
        return Err("No completed matching repairs are ready to retrieve".into());
    }
    let available_gold = crate::item::personal_currency_total(ctx, character_id);
    let affordable = adventuresim_core::durability::affordable_repair_prefix(
        available_gold,
        &ids.iter().map(|(_, _, cost)| *cost).collect::<Vec<_>>(),
    );
    if affordable == 0 {
        return Err("Not enough personal coin to retrieve the next completed repair".into());
    }
    for (_, order_id, _) in ids.into_iter().take(affordable) {
        retrieve(ctx, character_id, order_id)?;
    }
    Ok(())
}

/// Field maintenance is automatic after bodily convalescence. It repairs only
/// yellow bins and uses Tailoring for clothing, Smithing for other equipment.
pub(crate) fn field_repair(
    ctx: &ReducerContext,
    character_id: u64,
    smithing: u8,
    tailoring: u8,
    available_minutes: u64,
) -> u64 {
    let mut remaining = available_minutes;
    let ids: Vec<u64> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .map(|v| v.id)
        .collect();
    for id in ids {
        if remaining == 0 {
            break;
        }
        let Some(mut condition) = ctx.db.item_condition().inventory_item_id().find(id) else {
            continue;
        };
        let mut bins = condition.bins();
        let item_kind = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .and_then(|inventory| ctx.db.item().id().find(&inventory.item_id))
            .map(|item| item.kind);
        let eligible_skill = if item_kind == Some(PersistedItemKind::Clothing) {
            tailoring
        } else {
            smithing
        }
        .min(2);
        let eligible = bins.repairable(eligible_skill);
        let possible = remaining as f32 / REPAIR_MINUTES_PER_FULL_ITEM as f32;
        let repaired = eligible.min(possible);
        let mut left = repaired;
        for bin in bins.0.iter_mut().take(eligible_skill as usize) {
            let take = (*bin).min(left);
            *bin -= take;
            left -= take;
        }
        let used = (repaired * REPAIR_MINUTES_PER_FULL_ITEM as f32).ceil() as u64;
        remaining = remaining.saturating_sub(used);
        condition.set_bins(bins);
        ctx.db
            .item_condition()
            .inventory_item_id()
            .update(condition);
    }
    available_minutes - remaining
}

/// Applies one localized combat stress to the persisted physical instance.
/// Autoresolve calls this only at its final strategic commit boundary.
pub(crate) fn apply_impact(ctx: &ReducerContext, inventory_item_id: u64, stress: f32) {
    let Some(inventory) = ctx.db.inventory_item().id().find(inventory_item_id) else {
        return;
    };
    let Some(definition) = ctx.db.item().id().find(&inventory.item_id) else {
        return;
    };
    let Some(mut condition) = ctx
        .db
        .item_condition()
        .inventory_item_id()
        .find(inventory_item_id)
    else {
        return;
    };
    let event = damage_from_impact(
        DurabilityProfile {
            yield_threshold: definition.durability_yield,
            catastrophic_threshold: definition.durability_fracture,
            wear_rate: definition.durability_wear,
            failure_share: definition.durability_failure_share,
            quality: definition.quality.clamp(1, 5),
        },
        stress,
    );
    let quality_tier = definition.quality.clamp(1, 5);
    let mut bins = condition.bins().capped_to_quality(quality_tier);
    let event_tier = event.tier.min(quality_tier);
    if !event.catastrophic && quality_tier > event.tier {
        // Most routine wear remains approachable, but restoring the final fit,
        // temper, and finish requires a craftsperson matching the item's quality.
        bins.add_to_tier(event_tier, event.amount * 0.8);
        bins.add_to_tier(quality_tier, event.amount * 0.2);
    } else {
        bins.add_to_tier(event_tier, event.amount);
    }
    condition.set_bins(bins);
    ctx.db
        .item_condition()
        .inventory_item_id()
        .update(condition);
}

/// Disposable-live-simulator fixture. Policy actions still use the ordinary
/// submit/rest/retrieve reducers; this only supplies deterministic initial wear.
#[reducer]
pub fn seed_simulation_equipment_damage(
    ctx: &ReducerContext,
    nonce: String,
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    let run = crate::simulation::owned_run(ctx, &nonce)?;
    let simulation_character = ctx
        .db
        .simulation_character()
        .character_id()
        .find(character_id)
        .ok_or("Only claimed simulation characters may use this fixture")?;
    if simulation_character.run_id != run.id {
        return Err("Simulation character belongs to a different run".into());
    }
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Inventory item not found")?;
    if inventory.character_id != character_id {
        return Err("Simulation character does not own that item".into());
    }
    let mut condition = ctx
        .db
        .item_condition()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or("Durable condition not found")?;
    condition.tier_2 = 0.08;
    condition.tier_3 = 0.24;
    ctx.db
        .item_condition()
        .inventory_item_id()
        .update(condition);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::item_catalog::EquipmentChannel;

    #[test]
    fn repair_service_kinds_remain_explicit() {
        assert!(repair_kind(PersistedItemKind::Clothing).is_some());
        assert!(repair_kind(PersistedItemKind::Weapon).is_some());
        assert!(repair_kind(PersistedItemKind::Medication).is_none());
    }

    #[test]
    fn repair_services_are_strict_three_way_item_filters() {
        let weapons = repair_service("weapons").unwrap();
        let armor = repair_service("armor").unwrap();
        let clothing = repair_service("clothing").unwrap();
        assert!(service_matches(weapons, PersistedItemKind::Weapon));
        assert!(service_matches(weapons, PersistedItemKind::Shield));
        assert!(!service_matches(weapons, PersistedItemKind::Armor));
        assert!(!service_matches(weapons, PersistedItemKind::Clothing));
        assert!(service_matches(armor, PersistedItemKind::Armor));
        assert!(!service_matches(armor, PersistedItemKind::Clothing));
        assert!(service_matches(clothing, PersistedItemKind::Clothing));
        assert!(!service_matches(clothing, PersistedItemKind::Weapon));
        assert!(repair_service("smith").is_err());
    }

    #[test]
    fn repair_snapshot_preserves_every_parent_requirement_in_order() {
        let occupancy =
            |requirement_index, parent, point: &str| crate::character::EquipmentOccupancy {
                id: format!("{parent}:{point}"),
                character_id: 1,
                inventory_item_id: 99,
                anchor_kind: crate::character::EquipmentAnchorKind::ItemAttachment,
                location: None,
                parent_inventory_item_id: Some(parent),
                attachment_point_id: Some(point.into()),
                channel: EquipmentChannel::Mount,
                order: requirement_index,
                requirement_index,
                capacity_index: 0,
            };
        let targets =
            saved_attachment_targets([occupancy(1, 11, "right"), occupancy(0, 10, "left")]);
        assert_eq!(
            targets,
            vec![
                crate::character::EquipmentAttachmentTargetSelection {
                    requirement_index: 0,
                    parent_inventory_item_id: 10,
                    attachment_point_id: "left".into(),
                },
                crate::character::EquipmentAttachmentTargetSelection {
                    requirement_index: 1,
                    parent_inventory_item_id: 11,
                    attachment_point_id: "right".into(),
                },
            ]
        );
    }

    #[test]
    fn repair_submission_preserves_stable_object_in_explicit_escrow() {
        let source = crate::production_source(include_str!("repair.rs"));
        let submit = source
            .split("fn submit(")
            .nth(1)
            .unwrap()
            .split("fn saved_attachment_targets")
            .next()
            .unwrap();
        assert!(submit.contains("InventoryLocation::repair"));
        assert!(submit.contains("detach_if_nested"));
    }
}
