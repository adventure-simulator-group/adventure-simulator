//! Persistent strategic equipment condition and settlement repair custody.

use adventuresim_core::durability::DamageBins;
use adventuresim_core::durability::{DurabilityProfile, damage_from_impact};
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::character::{character, character_equip};
use crate::item::{inventory_item, item};
use crate::simulation::simulation_character;
use crate::strategic::{
    PartyInventoryItem, PartyItemCondition, party_inventory_item, party_item_condition, settlement,
};
use crate::time::character_time;
use crate::{CharacterEquip, InventoryItem, ItemKind};

pub const REPAIR_MINUTES_PER_FULL_ITEM: u64 = 2 * 1_440;

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
    /// Stable quote charged when the completed job is retrieved.
    #[default(0)]
    pub quoted_cost: u32,
}

fn durable(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Weapon | ItemKind::Armor | ItemKind::Shield)
}

pub(crate) fn initialize_item_condition(ctx: &ReducerContext, inventory: &InventoryItem) {
    let Some(definition) = ctx.db.item().id().find(&inventory.item_id) else {
        return;
    };
    if !durable(definition.kind)
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

fn initialize_party_item_condition(ctx: &ReducerContext, inventory: &PartyInventoryItem) {
    let Some(definition) = ctx.db.item().id().find(&inventory.item_id) else {
        return;
    };
    if !durable(definition.kind)
        || ctx
            .db
            .party_item_condition()
            .party_inventory_item_id()
            .find(inventory.id)
            .is_some()
    {
        return;
    }
    ctx.db.party_item_condition().insert(PartyItemCondition {
        party_inventory_item_id: inventory.id,
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
    })
}

#[reducer]
pub fn backfill_equipment_condition_and_smiths(ctx: &ReducerContext) {
    for mut inventory in ctx.db.inventory_item().iter().collect::<Vec<_>>() {
        let durable = ctx
            .db
            .item()
            .id()
            .find(&inventory.item_id)
            .is_some_and(|item| durable(item.kind));
        if durable
            && adventuresim_core::durability::legacy_durable_row_is_invalid(inventory.quantity)
        {
            if let Some(mut equip) = ctx
                .db
                .character_equip()
                .character_id()
                .find(inventory.character_id)
            {
                unequip(&mut equip, inventory.id);
                ctx.db.character_equip().character_id().update(equip);
                let _ =
                    crate::capability::refresh_character_capability(ctx, inventory.character_id);
            }
            ctx.db
                .item_condition()
                .inventory_item_id()
                .delete(inventory.id);
            ctx.db.inventory_item().id().delete(inventory.id);
            continue;
        }
        let additional = if durable {
            adventuresim_core::durability::durable_stack_split_count(inventory.quantity)
        } else {
            0
        };
        if additional > 0 {
            // Keep this ID so any CharacterEquip reference remains valid.
            inventory.quantity = 1;
            ctx.db.inventory_item().id().update(inventory.clone());
            for _ in 0..additional {
                let split = ctx.db.inventory_item().insert(InventoryItem {
                    id: 0,
                    character_id: inventory.character_id,
                    item_id: inventory.item_id.clone(),
                    quantity: 1,
                });
                initialize_item_condition(ctx, &split);
            }
        }
        initialize_item_condition(ctx, &inventory);
    }
    for mut inventory in ctx.db.party_inventory_item().iter().collect::<Vec<_>>() {
        let durable = ctx
            .db
            .item()
            .id()
            .find(&inventory.item_id)
            .is_some_and(|item| durable(item.kind));
        if durable
            && adventuresim_core::durability::legacy_durable_row_is_invalid(inventory.quantity)
        {
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(inventory.id);
            ctx.db.party_inventory_item().id().delete(inventory.id);
            continue;
        }
        let additional = if durable {
            adventuresim_core::durability::durable_stack_split_count(inventory.quantity)
        } else {
            0
        };
        if additional > 0 {
            inventory.quantity = 1;
            ctx.db.party_inventory_item().id().update(inventory.clone());
            for _ in 0..additional {
                let split = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                    id: 0,
                    party_id: inventory.party_id.clone(),
                    item_id: inventory.item_id.clone(),
                    quantity: 1,
                });
                initialize_party_item_condition(ctx, &split);
            }
        }
        initialize_party_item_condition(ctx, &inventory);
    }
    for settlement in ctx.db.settlement().iter() {
        ensure_settlement_smith(ctx, &settlement.id);
    }
}

fn service_skill(ctx: &ReducerContext, settlement_id: &str, kind: ItemKind) -> Result<u8, String> {
    let service = ensure_settlement_smith(ctx, settlement_id);
    match kind {
        ItemKind::Weapon | ItemKind::Shield => Ok(service.weaponsmith_skill),
        ItemKind::Armor => Ok(service.armourer_skill),
        _ => Err("This service does not repair that item kind".into()),
    }
}

pub(crate) fn is_equipped(equip: &CharacterEquip, id: u64) -> bool {
    equip.left_hand_item_id == Some(id)
        || equip.right_hand_item_id == Some(id)
        || equip.left_arm_armor_id == Some(id)
        || equip.right_arm_armor_id == Some(id)
        || equip.left_leg_armor_id == Some(id)
        || equip.right_leg_armor_id == Some(id)
        || equip.head_armor_id == Some(id)
        || equip.chest_armor_id == Some(id)
        || equip.stomach_armor_id == Some(id)
}

pub(crate) fn unequip(equip: &mut CharacterEquip, id: u64) {
    if equip.left_hand_item_id == Some(id) {
        equip.left_hand_item_id = None;
    }
    if equip.right_hand_item_id == Some(id) {
        equip.right_hand_item_id = None;
    }
    if equip.left_arm_armor_id == Some(id) {
        equip.left_arm_armor_id = None;
    }
    if equip.right_arm_armor_id == Some(id) {
        equip.right_arm_armor_id = None;
    }
    if equip.left_leg_armor_id == Some(id) {
        equip.left_leg_armor_id = None;
    }
    if equip.right_leg_armor_id == Some(id) {
        equip.right_leg_armor_id = None;
    }
    if equip.head_armor_id == Some(id) {
        equip.head_armor_id = None;
    }
    if equip.chest_armor_id == Some(id) {
        equip.chest_armor_id = None;
    }
    if equip.stomach_armor_id == Some(id) {
        equip.stomach_armor_id = None;
    }
}

fn submit(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
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
            "All damage is beyond this smith's skill"
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
    if let Some(mut equip) = ctx.db.character_equip().character_id().find(character_id) {
        unequip(&mut equip, inventory_item_id);
        ctx.db.character_equip().character_id().update(equip);
    }
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
        quoted_cost,
    });
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(order.id)
}

#[reducer]
pub fn submit_item_for_repair(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    inventory_item_id: u64,
) -> Result<(), String> {
    submit(ctx, character_id, &settlement_id, inventory_item_id).map(|_| ())
}

#[reducer]
pub fn submit_all_repairable_items(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    armourer: bool,
) -> Result<(), String> {
    let ids: Vec<u64> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter_map(|inventory| {
            let kind = ctx.db.item().id().find(&inventory.item_id)?.kind;
            let matching = if armourer {
                kind == ItemKind::Armor
            } else {
                matches!(kind, ItemKind::Weapon | ItemKind::Shield)
            };
            matching.then_some(inventory.id)
        })
        .collect();
    let mut submitted = 0;
    for id in ids {
        if submit(ctx, character_id, &settlement_id, id).is_ok() {
            submitted += 1;
        }
    }
    if submitted == 0 {
        return Err("No matching item has damage this smith can repair".into());
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
    ctx.db.repair_order().id().delete(order_id);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn retrieve_repaired_items(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    armourer: bool,
    item_id: Option<String>,
    limit: u32,
) -> Result<(), String> {
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
                && ctx.db.item().id().find(&order.item_id).is_some_and(|item| {
                    if armourer {
                        item.kind == ItemKind::Armor
                    } else {
                        matches!(item.kind, ItemKind::Weapon | ItemKind::Shield)
                    }
                })
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
/// yellow bins and only through the character's trained Smithing rating.
pub(crate) fn field_repair(
    ctx: &ReducerContext,
    character_id: u64,
    smithing: u8,
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
        let eligible_skill = smithing.min(2);
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
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    if ctx
        .db
        .simulation_character()
        .character_id()
        .find(character_id)
        .is_none()
    {
        return Err("Only claimed simulation characters may use this fixture".into());
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
    Ok(())
}
