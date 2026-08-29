use crate::{
    inventory_amount::inventory_item_amount, repair::item_condition, strategic::settlement,
};
use adventuresim_core::{
    combat_style::MeleeAttackStyle,
    item_catalog::{EquipmentBodyPart, EquipmentChannel, EquipmentLocation, Slot},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

pub use adventuresim_core::strategic_currency::CURRENCY_IDS;

pub fn settlement_currency_id(settlement_id: &str) -> &'static str {
    adventuresim_core::strategic_currency::assigned_currency_id(settlement_id)
}

pub fn currency_id_for_settlement(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<String, String> {
    match ctx.db.settlement().id().find(settlement_id.to_string()) {
        Some(settlement) if CURRENCY_IDS.contains(&settlement.currency_id.as_str()) => {
            Ok(settlement.currency_id)
        }
        Some(settlement) => Err(format!(
            "Settlement {} has invalid currency {}",
            settlement.id, settlement.currency_id
        )),
        None => Ok(settlement_currency_id(settlement_id).to_string()),
    }
}

/// [`Item`] that is in the inventory
#[derive(Clone, Debug)]
#[table(
    accessor = inventory_item, public,
    index(accessor = character_and_item_id, btree(columns = [character_id, item_id])),
    index(accessor = character_and_id, btree(columns = [character_id, id])),
)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub item_id: String,
    pub quantity: u32,
}

#[derive(SpacetimeType, Default, Clone, Copy, Debug, PartialEq)]
pub enum ItemKind {
    #[default]
    Simple,
    Weapon,
    Armor,
    Shield,
    Clothing,
    Container,
    Currency,
    Ingredient,
    Medication,
    Food,
}

pub(crate) const fn economy_catalog_kind(
    kind: ItemKind,
) -> adventuresim_core::settlement_economy::CatalogKind {
    use adventuresim_core::settlement_economy::CatalogKind as C;
    match kind {
        ItemKind::Simple => C::Simple,
        ItemKind::Weapon => C::Weapon,
        ItemKind::Armor => C::Armor,
        ItemKind::Shield => C::Shield,
        ItemKind::Clothing => C::Clothing,
        ItemKind::Container => C::Simple,
        ItemKind::Currency => C::Currency,
        ItemKind::Ingredient => C::Ingredient,
        ItemKind::Medication => C::Medication,
        ItemKind::Food => C::Food,
    }
}

#[derive(SpacetimeType, Default, Clone, Copy, Debug, PartialEq)]
pub struct WeaponSkillDistribution {
    pub polearm: f32,
    pub axe: f32,
    pub bludgeon: f32,
    pub sword: f32,
    pub knife: f32,
    pub bow: f32,
    pub crossbow: f32,
    pub firearm: f32,
    pub throw_skill: f32,
}

impl WeaponSkillDistribution {
    pub fn core(self) -> adventuresim_core::equipment::WeaponSkillDistribution {
        adventuresim_core::equipment::WeaponSkillDistribution {
            polearm: self.polearm,
            axe: self.axe,
            bludgeon: self.bludgeon,
            sword: self.sword,
            knife: self.knife,
            bow: self.bow,
            crossbow: self.crossbow,
            firearm: self.firearm,
            throw: self.throw_skill,
        }
    }
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentOccupancyRequirement {
    pub location: EquipmentLocation,
    pub channel: EquipmentChannel,
    pub order: u16,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentParentRequirement {
    pub channel: EquipmentChannel,
    pub order: u16,
}

#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct EquipmentPlacement {
    pub id: String,
    pub occupancy: Vec<EquipmentOccupancyRequirement>,
    pub parents: Vec<EquipmentParentRequirement>,
    pub protection: Vec<EquipmentBodyPart>,
}

#[derive(SpacetimeType, Clone, Debug, PartialEq)]
pub struct EquipmentAttachmentPoint {
    pub id: String,
    pub channel: EquipmentChannel,
    pub capacity: u16,
    pub order: u16,
    pub accepts_tags: Vec<String>,
}

/// Item stats
#[derive(Clone, Debug, Default)]
#[table(accessor = item, public)]
pub struct Item {
    #[primary_key]
    pub id: String,
    pub weight: f32,
    /// Exterior displacement for generic inventory containment.
    pub exterior_volume_ml: u32,
    pub slot: Slot,
    pub kind: ItemKind,
    pub equipment_placements: Vec<EquipmentPlacement>,
    pub attachment_tags: Vec<String>,
    pub attachment_points: Vec<EquipmentAttachmentPoint>,
    /// Whether this definition has authored durability and receives condition rows.
    pub repairable: bool,
    pub accuracy: f32,
    pub swing_precision: f32,
    pub stab_precision: f32,
    pub preferred_melee_style: MeleeAttackStyle,
    pub reach: f32,
    pub block: f32,
    pub coverage: f32,
    pub penetration: f32,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
    pub precise: bool,
    /// Rotational inertia around the weapon grip, in kg*m^2.
    pub moment_of_inertia_kg_m2: f32,
    /// Derived user-facing radius-of-gyration coefficient. Lower is easier to
    /// redirect; the authored source of truth is `moment_of_inertia_kg_m2`.
    pub balance: f32,
    pub melee: bool,
    pub ranged: bool,
    pub weapon_skills: WeaponSkillDistribution,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub base_value: Option<u32>,
    /// Metabolizable energy supplied when this item is automatically eaten.
    pub nutrition_kcal: f32,
    /// Water capacity contributed while this item is in personal inventory.
    pub water_capacity_ml: u32,
    /// Generic interior inventory capacity. This remains independent of
    /// equipment attachment points and their slot counts.
    pub container_capacity_ml: u32,
    /// Potable liquid in one discrete serving.
    pub alcohol_serving_ml: u32,
    /// Alcohol by volume in basis points (500 = 5%).
    pub alcohol_abv_basis_points: u16,
    /// Useful emergency hydration supplied by one unit while travelling.
    pub alcohol_net_hydration_ml: u32,
    /// Additive hidden infection-control value; zero means ineligible.
    pub alcohol_disinfectant_effectiveness: u16,
    /// Preserve for medical use during ordinary morale drinking.
    pub alcohol_disinfectant_focused: bool,
    pub alcohol_potable: bool,
    /// Craftsmanship and maintenance target, on the shared 1..5 skill scale.
    pub quality: u8,
    /// Explicit construction/material inputs; never inferred from market value.
    pub durability_yield: f32,
    pub durability_fracture: f32,
    pub durability_wear: f32,
    pub durability_failure_share: f32,
    pub edge_sensitivity: f32,
    pub handling_sensitivity: f32,
}

/// Projects a typed authored definition into the authoritative strategic
/// persistence schema.
fn project_definition(definition: &adventuresim_core::item_catalog::ItemDefinition) -> Item {
    use adventuresim_core::item_catalog::{DamageType, ItemKind as K};
    let mut item = Item {
        id: definition.id.clone(),
        weight: definition.weight_kg,
        exterior_volume_ml: definition.exterior_volume_ml,
        base_value: Some(definition.base_value),
        ..Item::default()
    };
    if let Some(equipment) = &definition.equipment {
        item.attachment_tags = equipment.attachment_tags.clone();
        item.equipment_placements = equipment
            .placements
            .iter()
            .map(|placement| EquipmentPlacement {
                id: placement.id.clone(),
                occupancy: placement
                    .occupancy
                    .iter()
                    .map(|requirement| EquipmentOccupancyRequirement {
                        location: requirement.location,
                        channel: requirement.channel,
                        order: requirement.order,
                    })
                    .collect(),
                parents: placement
                    .parents
                    .iter()
                    .map(|parent| EquipmentParentRequirement {
                        channel: parent.channel,
                        order: parent.order,
                    })
                    .collect(),
                protection: placement.protection.clone(),
            })
            .collect();
        item.attachment_points = equipment
            .attachment_points
            .iter()
            .map(|point| EquipmentAttachmentPoint {
                id: point.id.clone(),
                channel: point.channel,
                capacity: point.capacity,
                order: point.order,
                accepts_tags: point.accepts_tags.clone(),
            })
            .collect();
        if let Some(protection) = equipment.protection {
            item.coverage = protection.coverage;
            item.resistance = protection.resistance;
            item.padding = protection.padding;
            item.flexibility = protection.flexibility;
            item.range_of_motion = protection.range_of_motion;
        }
    }
    match &definition.kind {
        K::Simple => item.kind = ItemKind::Simple,
        K::Container {
            slot: authored_slot,
        } => {
            item.kind = ItemKind::Container;
            item.slot = *authored_slot;
        }
        K::Currency => item.kind = ItemKind::Currency,
        K::Ingredient => item.kind = ItemKind::Ingredient,
        K::Medication => item.kind = ItemKind::Medication,
        K::Clothing => {
            item.kind = ItemKind::Clothing;
        }
        K::Food => item.kind = ItemKind::Food,
        K::Shield {
            slot: authored_slot,
            block,
        } => {
            item.kind = ItemKind::Shield;
            item.slot = *authored_slot;
            item.block = *block;
        }
        K::Armor {
            slot: authored_slot,
            coverage,
            resistance,
            padding,
            flexibility,
            range_of_motion,
        } => {
            item.kind = ItemKind::Armor;
            item.slot = *authored_slot;
            item.coverage = *coverage;
            item.resistance = *resistance;
            item.padding = *padding;
            item.flexibility = *flexibility;
            item.range_of_motion = *range_of_motion;
        }
        K::Weapon {
            slot: authored_slot,
            handling: _,
            animation_pack: _,
            carry: _,
            preferred_attack,
            swing_precision,
            stab_precision,
            accuracy,
            reach_m,
            penetration,
            moment_of_inertia_kg_m2,
            precise,
            melee,
            ranged,
            damage_types,
            skills,
        } => {
            item.kind = ItemKind::Weapon;
            item.slot = *authored_slot;
            item.accuracy = *accuracy;
            item.swing_precision = *swing_precision;
            item.stab_precision = *stab_precision;
            item.preferred_melee_style = *preferred_attack;
            item.reach = *reach_m;
            item.penetration = *penetration;
            item.moment_of_inertia_kg_m2 = *moment_of_inertia_kg_m2;
            let grip_to_tip_m = definition
                .equipment
                .as_ref()
                .map_or(0.0, |equipment| equipment.physical.grip_to_tip_m);
            item.balance = adventuresim_core::equipment::weapon_balance_from_moment(
                *moment_of_inertia_kg_m2,
                definition.weight_kg,
                grip_to_tip_m,
            );
            item.precise = *precise;
            item.melee = *melee;
            item.ranged = *ranged;
            item.blunt = damage_types.contains(&DamageType::Blunt);
            item.slash = damage_types.contains(&DamageType::Slash);
            item.pierce = damage_types.contains(&DamageType::Pierce);
            item.weapon_skills = WeaponSkillDistribution {
                polearm: skills.polearm,
                axe: skills.axe,
                bludgeon: skills.bludgeon,
                sword: skills.sword,
                knife: skills.knife,
                bow: skills.bow,
                crossbow: skills.crossbow,
                firearm: skills.firearm,
                throw_skill: skills.throw,
            };
        }
    }
    if let Some(food) = &definition.capabilities.food {
        item.nutrition_kcal = food.nutrition_kcal;
        item.quality = food.quality;
    }
    if let Some(book) = &definition.capabilities.book {
        item.quality = book.quality;
    }
    if let Some(container) = definition.capabilities.container {
        item.water_capacity_ml = if definition.id == "waterskin" {
            container.capacity_ml
        } else {
            0
        };
        item.container_capacity_ml = container.capacity_ml;
    }
    if let Some(alcohol) = definition.capabilities.alcohol {
        item.alcohol_serving_ml = alcohol.serving_ml;
        item.alcohol_abv_basis_points = alcohol.abv_basis_points;
        item.alcohol_net_hydration_ml = alcohol.net_hydration_ml;
        item.alcohol_disinfectant_effectiveness = alcohol.disinfectant_effectiveness;
        item.alcohol_disinfectant_focused = alcohol.disinfectant_focused;
        item.alcohol_potable = alcohol.potable;
    }
    if let Some(durable) = definition.capabilities.durability {
        item.repairable = true;
        item.quality = durable.quality;
        item.durability_yield = durable.yield_j;
        item.durability_fracture = durable.fracture_j;
        item.durability_wear = durable.wear;
        item.durability_failure_share = durable.failure_share;
        item.edge_sensitivity = durable.edge_sensitivity;
        item.handling_sensitivity = durable.handling_sensitivity;
    }
    item
}

#[reducer(init)]
fn init_items(ctx: &ReducerContext) -> Result<(), String> {
    crate::time::initialize_time(ctx);
    crate::npc_causal::initialize_npc_causal_schedule(ctx);
    crate::disease::initialize_physiology_key(ctx);
    log::info!(
        "Populating items from catalog revision {}",
        adventuresim_core::item_catalog::revision()
    );
    for definition in adventuresim_core::item_catalog::catalog() {
        ctx.db.item().insert(project_definition(definition));
    }
    Ok(())
}

pub(crate) fn upsert_surgery_items(ctx: &ReducerContext) {
    for id in [
        "surgery_kit",
        "splint",
        adventuresim_core::item_references::SOFT_SOAP_ID,
    ] {
        let definition = adventuresim_core::item_catalog::definition(id)
            .expect("validated surgery item reference");
        let item = project_definition(definition);
        if ctx.db.item().id().find(id.to_owned()).is_some() {
            ctx.db.item().id().update(item);
        } else {
            ctx.db.item().insert(item);
        }
    }
}

pub(crate) fn inventory_food_definition(
    kind: Option<ItemKind>,
    item_id: &str,
) -> Result<Option<&'static adventuresim_core::food::FoodDefinition>, String> {
    let definition = adventuresim_core::food::definition(item_id);
    if kind == Some(ItemKind::Food) || definition.is_some() {
        definition
            .map(Some)
            .ok_or_else(|| format!("Food definition not found for {item_id}"))
    } else {
        Ok(None)
    }
}

pub(crate) fn requires_stable_object(
    definition: Option<&Item>,
    food: bool,
    measured: bool,
) -> bool {
    food || measured
        || definition.is_some_and(|definition| {
            definition.repairable
                || definition.kind == ItemKind::Medication
                || (definition.kind == ItemKind::Weapon && definition.melee)
                || definition.container_capacity_ml > 0
                || !definition.attachment_points.is_empty()
        })
}

pub(crate) fn add_inventory_item_checked(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    quantity: u32,
) -> Result<Option<u64>, String> {
    if quantity == 0 {
        return Ok(None);
    }

    let definition = ctx.db.item().id().find(item_id.to_owned());
    let kind = definition.as_ref().map(|definition| definition.kind);
    let food_definition = inventory_food_definition(kind, item_id)?;
    let durable = definition
        .as_ref()
        .is_some_and(|definition| definition.repairable);
    let food = food_definition.is_some();
    let measured = crate::inventory_amount::is_measured_item(ctx, item_id);
    // Every food unit is its own non-fungible batch. A partly consumed unit
    // remains quantity one while its authoritative lot mass/value/provenance
    // shrink, so it can never be merged back into fresh stock.
    let individual = requires_stable_object(definition.as_ref(), food, measured);
    let count = if individual { quantity } else { 1 };
    let mut first = None;
    for _ in 0..count {
        let item = ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id,
            item_id: item_id.to_string(),
            quantity: if individual { 1 } else { quantity },
        });
        if individual {
            crate::inventory_container::insert_personal_object(ctx, &item)?;
        }
        if durable {
            crate::repair::initialize_item_condition(ctx, &item);
        }
        crate::weapon_instance::initialize_personal_weapon(ctx, &item)?;
        if measured {
            crate::inventory_amount::initialize_personal(ctx, item.id);
        }
        if food {
            crate::food::create_personal_food_lot(
                ctx,
                character_id,
                item.id,
                item_id,
                if individual { 1 } else { quantity },
            )
            .map_err(|error| format!("Could not create food lot: {error}"))?;
        }
        first.get_or_insert(item.id);
    }
    if food {
        let _ = crate::capability::refresh_character_capability(ctx, character_id);
    }
    Ok(first)
}

/// Foraging receipts bind every concrete harvested unit to an object and
/// material lot. Preserve the shared grant helper's stacking semantics for
/// every other caller while intentionally issuing one validated unit here.
pub(crate) fn add_foraged_inventory_item_checked_rows(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    quantity: u32,
) -> Result<Vec<u64>, String> {
    let mut rows = Vec::with_capacity(quantity as usize);
    for _ in 0..quantity {
        rows.push(
            add_inventory_item_checked(ctx, character_id, item_id, 1)?
                .ok_or("Foraged inventory insertion returned no row")?,
        );
    }
    Ok(rows)
}

pub fn add_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    quantity: u32,
) -> Option<u64> {
    add_inventory_item_checked(ctx, character_id, item_id, quantity)
        .ok()
        .flatten()
}

pub fn is_currency(ctx: &ReducerContext, item_id: &str) -> bool {
    ctx.db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|item| item.kind == ItemKind::Currency)
}

pub fn personal_currency_total(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|stack| is_currency(ctx, &stack.item_id))
        .map(|stack| u64::from(stack.quantity))
        .sum()
}

fn currency_withdrawal_plan(
    mut stacks: Vec<(String, u64, u32)>,
    amount: u64,
) -> Option<Vec<(u64, u32)>> {
    if stacks
        .iter()
        .map(|(_, _, quantity)| u64::from(*quantity))
        .sum::<u64>()
        < amount
    {
        return None;
    }
    stacks.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut remaining = amount;
    let mut plan = Vec::new();
    for (_, id, quantity) in stacks {
        if remaining == 0 {
            break;
        }
        let taken = remaining.min(u64::from(quantity)) as u32;
        remaining -= u64::from(taken);
        plan.push((id, taken));
    }
    Some(plan)
}

/// Atomically consumes equal-value currency in a stable denomination/stack
/// order.  The preflight makes an insufficient payment a no-op.
pub fn consume_personal_currency(
    ctx: &ReducerContext,
    character_id: u64,
    amount: u64,
) -> Result<(), String> {
    let stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|stack| is_currency(ctx, &stack.item_id))
        .collect();
    let plan = currency_withdrawal_plan(
        stacks
            .iter()
            .map(|stack| (stack.item_id.clone(), stack.id, stack.quantity))
            .collect(),
        amount,
    )
    .ok_or_else(|| "Not enough coin to cover this payment".to_string())?;
    for (id, taken) in plan {
        let mut stack = stacks.iter().find(|stack| stack.id == id).cloned().unwrap();
        stack.quantity -= taken;
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
    }
    Ok(())
}

pub fn credit_personal_currency(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    amount: u32,
) -> Result<(), String> {
    if amount == 0 {
        return Ok(());
    }
    let currency_id = currency_id_for_settlement(ctx, settlement_id)?;
    if let Some(mut stack) = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, &currency_id))
        .next()
    {
        if let Some(quantity) = merged_currency_quantity(stack.quantity, amount) {
            stack.quantity = quantity;
            ctx.db.inventory_item().id().update(stack);
        } else {
            add_inventory_item(ctx, character_id, &currency_id, amount);
        }
    } else {
        add_inventory_item(ctx, character_id, &currency_id, amount);
    }
    Ok(())
}

pub fn validate_personal_currency_credit(
    ctx: &ReducerContext,
    settlement_id: &str,
    amount: u32,
) -> Result<(), String> {
    if amount == 0 {
        Ok(())
    } else {
        currency_id_for_settlement(ctx, settlement_id).map(|_| ())
    }
}

fn merged_currency_quantity(existing: u32, credit: u32) -> Option<u32> {
    existing.checked_add(credit)
}

#[reducer]
pub fn change_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    by_quantity: i32,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let durable = ctx
        .db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|definition| definition.repairable);
    if durable {
        let (add, remove) = adventuresim_core::durability::bounded_durable_change(by_quantity)
            .map_err(str::to_owned)?;
        if add > 0 {
            add_inventory_item(ctx, character_id, item_id, add);
            return Ok(());
        }
        if remove > 0 {
            let instances: Vec<_> = ctx
                .db
                .inventory_item()
                .character_and_item_id()
                .filter((character_id, item_id))
                .collect();
            if instances.len() < remove as usize {
                return Err("not enough durable instances to remove".into());
            }
            let removal_ids = adventuresim_core::durability::durable_removal_ids(
                instances
                    .iter()
                    .map(|item| {
                        (
                            item.id,
                            crate::character::wearable_is_equipped(ctx, item.id),
                        )
                    })
                    .collect(),
                remove,
            );
            let mut equipment_changed = false;
            for id in removal_ids {
                if crate::character::wearable_is_equipped(ctx, id) {
                    crate::character::unequip_wearable(ctx, id);
                    equipment_changed = true;
                }
                if !crate::inventory_container::delete_carried_object_for_row(
                    ctx,
                    adventuresim_core::physical_object::CarriedInventoryScope::Personal,
                    id,
                )? {
                    ctx.db.inventory_item().id().delete(id);
                    ctx.db.item_condition().inventory_item_id().delete(id);
                }
            }
            if equipment_changed {
                crate::capability::refresh_character_capability(ctx, character_id)?;
                crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
            }
        }
        return Ok(());
    }
    let food = ctx
        .db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|definition| definition.kind == ItemKind::Food)
        || adventuresim_core::food::definition(item_id).is_some();
    let measured = crate::inventory_amount::is_measured_item(ctx, item_id);
    if measured {
        if by_quantity > 0 {
            add_inventory_item(ctx, character_id, item_id, by_quantity as u32)
                .ok_or("food definition not found")?;
            return Ok(());
        }
        if by_quantity < 0 {
            let mut remaining = by_quantity.unsigned_abs();
            let mut items: Vec<_> = ctx
                .db
                .inventory_item()
                .character_and_item_id()
                .filter((character_id, item_id))
                .collect();
            items.sort_by_key(|row| row.id);
            if items.iter().map(|row| row.quantity as u64).sum::<u64>() < remaining as u64 {
                return Err("not enough inventory quantity to remove".into());
            }
            for mut row in items {
                let take = row.quantity.min(remaining);
                if food {
                    crate::food::remove_lot_quantity(ctx, row.id, take, row.quantity)?;
                }
                row.quantity -= take;
                remaining -= take;
                if row.quantity == 0 {
                    ctx.db
                        .inventory_item_amount()
                        .inventory_item_id()
                        .delete(row.id);
                    ctx.db.inventory_item().id().delete(row.id);
                } else {
                    ctx.db.inventory_item().id().update(row);
                }
                if remaining == 0 {
                    break;
                }
            }
        }
        return Ok(());
    }
    let mut items = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.id);
    if by_quantity > 0 {
        let addition = by_quantity as u32;
        if let Some(mut item) = items.into_iter().next() {
            if let Some(quantity) = item.quantity.checked_add(addition) {
                item.quantity = quantity;
                ctx.db.inventory_item().id().update(item);
            } else {
                add_inventory_item(ctx, character_id, item_id, addition);
            }
        } else {
            add_inventory_item(ctx, character_id, item_id, addition);
        }
    } else if by_quantity < 0 {
        let available = items
            .iter()
            .map(|item| u64::from(item.quantity))
            .sum::<u64>();
        let mut remaining = u64::from(by_quantity.unsigned_abs());
        if available < remaining {
            return Err("not enough inventory quantity to remove".into());
        }
        for mut item in items {
            let taken = remaining.min(u64::from(item.quantity)) as u32;
            item.quantity -= taken;
            remaining -= u64::from(taken);
            if item.quantity == 0 {
                ctx.db.inventory_item().id().delete(item.id);
            } else {
                ctx.db.inventory_item().id().update(item);
            }
            if remaining == 0 {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn food_inventory_is_prevalidated_before_rows_can_be_inserted() {
        let cooked = inventory_food_definition(Some(ItemKind::Food), "cooked_meal")
            .unwrap()
            .expect("cooked meal definition");
        assert!(cooked.kcal_per_unit > 0.0);
        assert!(inventory_food_definition(Some(ItemKind::Food), "missing_food").is_err());
        assert_eq!(
            inventory_food_definition(Some(ItemKind::Simple), "torch").unwrap(),
            None
        );
        let source = crate::production_source(include_str!("item.rs"));
        assert_eq!(
            source.matches("id: \"cooked_meal\".into()").count(),
            0,
            "the standard food catalog must be the sole cooked-meal item seed"
        );
        let checked = source
            .split("pub(crate) fn add_inventory_item_checked")
            .nth(1)
            .and_then(|tail| tail.split("pub fn add_inventory_item").next())
            .expect("checked inventory insertion");
        assert!(
            checked.find("inventory_food_definition").unwrap()
                < checked.find("inventory_item().insert").unwrap()
        );
        assert!(checked.contains("for _ in 0..count"));
        assert!(checked.contains("create_personal_food_lot("));
    }

    #[test]
    fn kind_aware_insertion_keeps_ingredients_fungible_and_medication_individual() {
        let source = crate::production_source(include_str!("item.rs"));
        let checked = source
            .split("pub(crate) fn add_inventory_item_checked")
            .nth(1)
            .and_then(|tail| tail.split("pub fn add_inventory_item").next())
            .expect("checked inventory insertion");
        assert!(checked.contains("requires_stable_object(definition.as_ref(), food, measured)"));
        let stable_object_policy = source
            .split("pub(crate) fn requires_stable_object")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn add_inventory_item_checked")
                    .next()
            })
            .expect("stable-object policy");
        assert!(stable_object_policy.contains("definition.kind == ItemKind::Medication"));
        assert!(checked.contains("let count = if individual { quantity } else { 1 }"));
        assert!(checked.contains("quantity: if individual { 1 } else { quantity }"));
        assert_eq!(
            adventuresim_core::item_catalog::definition("tincture_spirit")
                .unwrap()
                .kind,
            adventuresim_core::item_catalog::ItemKind::Ingredient
        );
    }

    #[test]
    fn foraging_specific_insertion_issues_each_harvested_unit_separately() {
        let source = crate::production_source(include_str!("item.rs"));
        let helper = source
            .split("pub(crate) fn add_foraged_inventory_item_checked_rows")
            .nth(1)
            .and_then(|tail| tail.split("pub fn add_inventory_item").next())
            .expect("foraging-specific insertion helper");
        assert!(helper.contains("for _ in 0..quantity"));
        assert!(helper.contains("add_inventory_item_checked(ctx, character_id, item_id, 1)"));
    }

    #[test]
    fn catalog_weapon_skill_distributions_cover_hybrids_and_ranged_families() {
        let halberd =
            project_definition(adventuresim_core::item_catalog::definition("halberd").unwrap())
                .weapon_skills
                .core();
        assert_eq!(halberd.polearm, 1.0 / 3.0);
        assert_eq!(halberd.axe, 1.0 / 3.0);
        assert_eq!(halberd.bludgeon, 1.0 / 3.0);
        assert!(halberd.validate(true, false));

        let hand_axe =
            project_definition(adventuresim_core::item_catalog::definition("hand_axe").unwrap())
                .weapon_skills
                .core();
        assert_eq!(hand_axe.axe, 0.5);
        assert_eq!(hand_axe.knife, 0.5);

        let crossbow = project_definition(
            adventuresim_core::item_catalog::definition("heavy_crossbow").unwrap(),
        )
        .weapon_skills
        .core();
        assert_eq!(crossbow.crossbow, 1.0);
        assert!(crossbow.validate(false, true));
    }

    #[test]
    fn settlement_currency_assignment_is_stable_and_uses_the_fixed_catalog() {
        let first = settlement_currency_id("viabundus-12345");
        assert_eq!(first, settlement_currency_id("viabundus-12345"));
        assert!(CURRENCY_IDS.contains(&first));
        assert_eq!(
            CURRENCY_IDS.len(),
            CURRENCY_IDS.iter().copied().collect::<HashSet<_>>().len()
        );
        assert!(
            (0..128)
                .map(|id| settlement_currency_id(&format!("demo-{id}")))
                .collect::<HashSet<_>>()
                .len()
                > 1
        );
    }

    #[test]
    fn mixed_currency_withdrawal_plan_is_deterministic_and_atomic() {
        let stacks = vec![("lubeck_mark".into(), 4, 3), ("danish_mark".into(), 9, 2)];
        assert_eq!(
            currency_withdrawal_plan(stacks.clone(), 4),
            Some(vec![(9, 2), (4, 2)])
        );
        assert_eq!(currency_withdrawal_plan(stacks, 6), None);
    }

    #[test]
    fn repeated_same_denomination_credits_merge_safely() {
        assert_eq!(merged_currency_quantity(40, 2), Some(42));
        assert_eq!(merged_currency_quantity(u32::MAX, 1), None);
    }

    #[test]
    fn historical_equipment_catalog_is_well_formed() {
        let projected: Vec<_> = adventuresim_core::item_catalog::catalog()
            .iter()
            .map(project_definition)
            .filter(|item| {
                matches!(
                    item.kind,
                    ItemKind::Weapon | ItemKind::Armor | ItemKind::Shield
                )
            })
            .collect();
        assert!(projected.iter().any(|definition| {
            definition.kind == ItemKind::Armor && definition.slot == Slot::Head
        }));
        for definition in projected {
            assert!(definition.weight > 0.0, "{} has no weight", definition.id);
            assert!(
                definition.base_value.is_some(),
                "{} has no value",
                definition.id
            );
            assert!(
                !definition.id.starts_with("bot_"),
                "{} is a placeholder rather than historical equipment",
                definition.id
            );

            match definition.kind {
                ItemKind::Weapon => {
                    assert_eq!(definition.slot, Slot::AnyHolding);
                    assert!(definition.accuracy > 0.0);
                    assert!(definition.reach > 0.0);
                    assert!(definition.blunt || definition.slash || definition.pierce);
                    assert_ne!(definition.melee, definition.ranged);
                }
                ItemKind::Armor => {
                    assert!(matches!(
                        definition.slot,
                        Slot::AnyArm | Slot::AnyLeg | Slot::Chest | Slot::Stomach | Slot::Head
                    ));
                    assert!((0.0..=1.0).contains(&definition.coverage));
                    assert!(definition.resistance > 0.0);
                    assert!(definition.padding > 0.0);
                    assert!((0.0..=1.0).contains(&definition.flexibility));
                    assert!((0.0..=1.0).contains(&definition.range_of_motion));
                }
                ItemKind::Shield => {
                    assert_eq!(definition.slot, Slot::AnyHolding);
                    assert!((1.0..=5.0).contains(&definition.block));
                }
                _ => unreachable!("equipment catalog contains a non-equipment item"),
            }
        }
    }

    #[test]
    fn projection_preserves_container_and_authored_repairability() {
        let waterskin =
            project_definition(adventuresim_core::item_catalog::definition("waterskin").unwrap());
        assert_eq!(waterskin.kind, ItemKind::Container);
        assert_eq!(waterskin.slot, Slot::None);
        assert!(!waterskin.repairable);

        let sword = project_definition(
            adventuresim_core::item_catalog::definition("arming_sword").unwrap(),
        );
        assert!(sword.repairable);
    }

    #[test]
    fn projection_exposes_book_quality_for_inventory_presentation() {
        let book = project_definition(
            adventuresim_core::item_catalog::definition("human_anatomy").unwrap(),
        );
        assert_eq!(book.kind, ItemKind::Simple);
        assert_eq!(book.quality, 4);
        assert!(!book.repairable);
    }

    #[test]
    fn round_and_heater_shields_have_a_weight_block_tradeoff() {
        let round = project_definition(
            adventuresim_core::item_catalog::definition("round_shield").unwrap(),
        );
        let heater = project_definition(
            adventuresim_core::item_catalog::definition("heater_shield").unwrap(),
        );
        assert!(round.weight < heater.weight);
        assert!(round.block < heater.block);
        assert!(!(round.weight <= heater.weight && round.block >= heater.block));
        assert!(!(heater.weight <= round.weight && heater.block >= round.block));
    }

    #[test]
    fn development_catalog_exercises_every_quality_level() {
        let qualities: HashSet<_> = adventuresim_core::item_catalog::catalog()
            .iter()
            .filter_map(|definition| definition.capabilities.durability)
            .map(|durability| durability.quality)
            .collect();
        assert_eq!(qualities, HashSet::from([1, 2, 3, 4, 5]));
    }
}
