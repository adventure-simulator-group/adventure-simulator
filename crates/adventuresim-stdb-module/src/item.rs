use crate::{character::character_equip, repair::item_condition};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};
use strum::{EnumCount, VariantArray};

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
    Currency,
    Ingredient,
    Medication,
}

#[derive(SpacetimeType, Default, Clone, Copy, Debug, PartialEq, EnumCount, VariantArray)]
pub enum ItemSlot {
    #[default]
    None,
    // Holding for whats in character hands
    LeftHolding,
    RightHolding,
    // Armor
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
    // Any slots for equip targets
    AnyHolding,
    AnyArm,
    AnyLeg,
}

/// Item stats
#[derive(Clone, Debug, Default)]
#[table(accessor = item, public)]
pub struct Item {
    #[primary_key]
    pub id: String,
    pub weight: f32,
    pub slot: ItemSlot,
    pub kind: ItemKind,
    pub accuracy: f32,
    pub reach: f32,
    pub block: f32,
    pub coverage: f32,
    pub penetration: f32,
    pub resistance: f32,
    pub padding: f32,
    pub flexibility: f32,
    pub range_of_motion: f32,
    pub precise: bool,
    pub balance: f32,
    pub melee: bool,
    pub ranged: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub base_value: Option<u32>,
    /// Metabolizable energy supplied when this item is automatically eaten.
    pub nutrition_kcal: f32,
    /// Water capacity contributed while this item is in personal inventory.
    pub water_capacity_ml: u32,
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

/// A static item definition used to seed the strategic item table.
///
/// The values use the combat quantities documented in `wiki/tactical/Combat.md`.
/// `base_value` is a relative gameplay price, not a claim about a historical
/// market price; the historical basis and gameplay inferences are recorded in
/// `docs/EQUIPMENT.md`.
#[derive(Clone, Copy, Debug)]
struct EquipmentDefinition {
    id: &'static str,
    weight: f32,
    base_value: u32,
    slot: ItemSlot,
    kind: ItemKind,
    accuracy: f32,
    reach: f32,
    block: f32,
    coverage: f32,
    penetration: f32,
    resistance: f32,
    padding: f32,
    flexibility: f32,
    range_of_motion: f32,
    precise: bool,
    balance: f32,
    melee: bool,
    ranged: bool,
    blunt: bool,
    slash: bool,
    pierce: bool,
}

const fn weapon(
    id: &'static str,
    weight: f32,
    base_value: u32,
    accuracy: f32,
    penetration: f32,
    reach: f32,
    balance: f32,
    precise: bool,
    blunt: bool,
    slash: bool,
    pierce: bool,
    ranged: bool,
) -> EquipmentDefinition {
    EquipmentDefinition {
        id,
        weight,
        base_value,
        slot: ItemSlot::AnyHolding,
        kind: ItemKind::Weapon,
        accuracy,
        reach,
        block: 0.0,
        coverage: 0.0,
        penetration,
        resistance: 0.0,
        padding: 0.0,
        flexibility: 0.0,
        range_of_motion: 0.0,
        precise,
        balance,
        melee: !ranged,
        ranged,
        blunt,
        slash,
        pierce,
    }
}

const fn armor(
    id: &'static str,
    weight: f32,
    base_value: u32,
    slot: ItemSlot,
    coverage: f32,
    resistance: f32,
    padding: f32,
    flexibility: f32,
    range_of_motion: f32,
) -> EquipmentDefinition {
    EquipmentDefinition {
        id,
        weight,
        base_value,
        slot,
        kind: ItemKind::Armor,
        accuracy: 0.0,
        reach: 0.0,
        block: 0.0,
        coverage,
        penetration: 0.0,
        resistance,
        padding,
        flexibility,
        range_of_motion,
        precise: false,
        balance: 0.0,
        melee: false,
        ranged: false,
        blunt: false,
        slash: false,
        pierce: false,
    }
}

const fn shield(id: &'static str, weight: f32, base_value: u32, block: f32) -> EquipmentDefinition {
    EquipmentDefinition {
        id,
        weight,
        base_value,
        slot: ItemSlot::AnyHolding,
        kind: ItemKind::Shield,
        accuracy: 0.0,
        reach: 0.0,
        block,
        coverage: 0.0,
        penetration: 0.0,
        resistance: 0.0,
        padding: 0.0,
        flexibility: 0.0,
        range_of_motion: 0.0,
        precise: false,
        balance: 0.0,
        melee: false,
        ranged: false,
        blunt: false,
        slash: false,
        pierce: false,
    }
}

// Common civilian, militia, professional, elite, and older-serviceable arms.
// Ranged reach is the current autoresolver range in metres; melee reach is in
// metres. Weapon precision and penetration follow the Combat.md calibration.
const WEAPONS: &[EquipmentDefinition] = &[
    weapon(
        "club", 1.2, 1, 0.5, 0.1, 0.7, 0.7, false, true, false, false, false,
    ),
    weapon(
        "walking_staff",
        1.5,
        1,
        0.8,
        0.1,
        1.8,
        0.3,
        false,
        true,
        false,
        false,
        false,
    ),
    weapon(
        "hand_axe", 0.9, 4, 1.0, 1.0, 0.7, 0.65, false, false, true, false, false,
    ),
    weapon(
        "flanged_mace",
        1.2,
        10,
        0.7,
        0.5,
        0.75,
        0.65,
        false,
        true,
        false,
        false,
        false,
    ),
    weapon(
        "war_hammer",
        1.4,
        14,
        0.7,
        0.5,
        0.8,
        0.55,
        false,
        true,
        false,
        true,
        false,
    ),
    weapon(
        "utility_knife",
        0.2,
        2,
        1.5,
        1.0,
        0.3,
        0.8,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "baselard", 0.4, 6, 1.5, 1.0, 0.4, 0.75, false, false, true, true, false,
    ),
    weapon(
        "rondel_dagger",
        0.45,
        12,
        2.0,
        4.0,
        0.4,
        0.8,
        true,
        false,
        false,
        true,
        false,
    ),
    weapon(
        "misericorde",
        0.35,
        14,
        2.0,
        4.0,
        0.4,
        0.75,
        true,
        false,
        false,
        true,
        false,
    ),
    weapon(
        "bauernwehr",
        0.7,
        8,
        1.2,
        1.0,
        0.65,
        0.7,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "katzbalger",
        1.1,
        18,
        1.5,
        1.0,
        0.8,
        0.55,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "arming_sword",
        1.3,
        28,
        1.5,
        1.0,
        0.95,
        0.5,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "longsword",
        1.5,
        40,
        1.5,
        1.0,
        1.25,
        0.45,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "messer", 1.2, 20, 1.2, 1.0, 1.0, 0.6, false, false, true, true, false,
    ),
    weapon(
        "kriegsmesser",
        1.6,
        35,
        1.0,
        1.0,
        1.25,
        0.65,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "rapier", 1.1, 45, 2.0, 4.0, 1.05, 0.45, true, false, false, true, false,
    ),
    weapon(
        "zweihander",
        2.8,
        60,
        0.7,
        1.0,
        1.8,
        0.5,
        false,
        false,
        true,
        true,
        false,
    ),
    weapon(
        "hunting_spear",
        1.5,
        6,
        1.5,
        2.0,
        2.2,
        0.45,
        false,
        false,
        false,
        true,
        false,
    ),
    weapon(
        "military_pike",
        4.0,
        9,
        1.3,
        2.0,
        4.2,
        0.85,
        false,
        false,
        false,
        true,
        false,
    ),
    weapon(
        "halberd", 2.4, 24, 1.1, 2.0, 2.0, 0.75, false, true, true, true, false,
    ),
    weapon(
        "self_bow", 0.8, 8, 1.5, 2.0, 60.0, 0.35, false, false, false, true, true,
    ),
    weapon(
        "longbow", 1.0, 18, 1.8, 2.0, 120.0, 0.4, false, false, false, true, true,
    ),
    weapon(
        "light_crossbow",
        2.2,
        28,
        2.0,
        4.0,
        80.0,
        0.45,
        true,
        false,
        false,
        true,
        true,
    ),
    weapon(
        "heavy_crossbow",
        3.5,
        50,
        2.0,
        4.0,
        110.0,
        0.5,
        true,
        false,
        false,
        true,
        true,
    ),
    weapon(
        "matchlock_arquebus",
        4.5,
        55,
        1.5,
        1.0,
        90.0,
        0.55,
        false,
        false,
        false,
        true,
        true,
    ),
    weapon(
        "hooked_arquebus",
        7.0,
        80,
        1.2,
        1.0,
        130.0,
        0.65,
        false,
        false,
        false,
        true,
        true,
    ),
];

const SHIELDS: &[EquipmentDefinition] = &[
    shield("buckler", 1.0, 5, 1.5),
    shield("targe", 2.5, 8, 2.5),
    shield("heater_shield", 3.0, 10, 3.0),
    shield("round_shield", 3.5, 10, 3.0),
    shield("pavise", 8.0, 20, 5.0),
];

// Helmets deliberately receive more entries than other armor slots: helmets
// remained independently useful, and armories retained older patterns.
const ARMOR: &[EquipmentDefinition] = &[
    armor(
        "arming_cap",
        0.35,
        2,
        ItemSlot::Head,
        0.15,
        20.0,
        35.0,
        0.4,
        0.95,
    ),
    armor(
        "mail_coif",
        1.5,
        18,
        ItemSlot::Head,
        0.65,
        70.0,
        30.0,
        0.8,
        0.85,
    ),
    armor(
        "kettle_hat",
        1.8,
        20,
        ItemSlot::Head,
        0.6,
        90.0,
        25.0,
        0.2,
        0.75,
    ),
    armor(
        "barbute",
        2.5,
        35,
        ItemSlot::Head,
        0.75,
        95.0,
        30.0,
        0.15,
        0.65,
    ),
    armor(
        "sallet",
        2.6,
        40,
        ItemSlot::Head,
        0.8,
        100.0,
        30.0,
        0.2,
        0.65,
    ),
    armor(
        "visored_sallet",
        3.0,
        50,
        ItemSlot::Head,
        0.9,
        105.0,
        35.0,
        0.15,
        0.55,
    ),
    armor(
        "burgonet",
        2.3,
        55,
        ItemSlot::Head,
        0.75,
        100.0,
        30.0,
        0.2,
        0.7,
    ),
    armor(
        "close_helmet",
        3.0,
        75,
        ItemSlot::Head,
        0.9,
        110.0,
        35.0,
        0.15,
        0.55,
    ),
    armor(
        "quilted_sleeve",
        0.6,
        5,
        ItemSlot::AnyArm,
        0.45,
        50.0,
        40.0,
        0.35,
        0.9,
    ),
    armor(
        "mail_sleeve",
        1.8,
        28,
        ItemSlot::AnyArm,
        0.65,
        70.0,
        30.0,
        0.8,
        0.8,
    ),
    armor(
        "vambrace",
        0.9,
        35,
        ItemSlot::AnyArm,
        0.65,
        95.0,
        25.0,
        0.15,
        0.8,
    ),
    armor(
        "padded_chausses",
        1.2,
        8,
        ItemSlot::AnyLeg,
        0.45,
        50.0,
        40.0,
        0.35,
        0.9,
    ),
    armor(
        "mail_chausses",
        3.5,
        45,
        ItemSlot::AnyLeg,
        0.65,
        70.0,
        30.0,
        0.8,
        0.8,
    ),
    armor(
        "greave",
        1.4,
        45,
        ItemSlot::AnyLeg,
        0.65,
        95.0,
        25.0,
        0.15,
        0.78,
    ),
    armor(
        "arming_doublet",
        2.5,
        12,
        ItemSlot::Chest,
        0.6,
        60.0,
        45.0,
        0.3,
        0.88,
    ),
    armor(
        "jack_of_plates",
        5.0,
        35,
        ItemSlot::Chest,
        0.7,
        85.0,
        35.0,
        0.45,
        0.8,
    ),
    armor(
        "brigandine",
        5.5,
        50,
        ItemSlot::Chest,
        0.8,
        100.0,
        40.0,
        0.4,
        0.75,
    ),
    armor(
        "mail_shirt",
        6.0,
        55,
        ItemSlot::Chest,
        0.75,
        70.0,
        35.0,
        0.8,
        0.82,
    ),
    armor(
        "breastplate",
        3.5,
        70,
        ItemSlot::Chest,
        0.85,
        120.0,
        45.0,
        0.05,
        0.72,
    ),
    armor(
        "cuirass",
        6.0,
        110,
        ItemSlot::Chest,
        0.9,
        120.0,
        50.0,
        0.08,
        0.65,
    ),
    armor(
        "padded_skirt",
        0.8,
        5,
        ItemSlot::Stomach,
        0.45,
        50.0,
        40.0,
        0.35,
        0.92,
    ),
    armor(
        "mail_skirt",
        2.5,
        30,
        ItemSlot::Stomach,
        0.65,
        70.0,
        30.0,
        0.8,
        0.85,
    ),
    armor(
        "fauld",
        1.6,
        40,
        ItemSlot::Stomach,
        0.7,
        95.0,
        35.0,
        0.2,
        0.78,
    ),
    armor(
        "tassets",
        2.0,
        55,
        ItemSlot::Stomach,
        0.75,
        100.0,
        35.0,
        0.18,
        0.75,
    ),
];

fn equipment_quality(item_id: &str) -> u8 {
    match item_id {
        "buckler" => 1,
        "katzbalger" | "padded_skirt" => 2,
        "arming_cap" | "arming_doublet" | "arming_sword" => 4,
        "padded_chausses" | "brigandine" => 5,
        _ => 3,
    }
}

#[reducer(init)]
fn init_items(ctx: &ReducerContext) -> Result<(), String> {
    crate::time::initialize_time(ctx);
    log::info!("Populating items...");

    define_item(ctx, "torch", 0.5);
    define_item(ctx, "arrow", 0.05);
    ctx.db.item().insert(Item {
        id: "gold_coin".into(),
        weight: 0.01,
        base_value: Some(1),
        kind: ItemKind::Currency,
        ..Item::default()
    });
    define_item(ctx, "bandage", 0.05);
    for (id, weight) in [
        ("honey", 0.25),
        ("sage", 0.05),
        ("dried_mint", 0.05),
        ("charcoal", 0.15),
        ("willow_bark", 0.10),
        ("vinegar", 0.30),
        ("poppy", 0.05),
        ("comfrey", 0.08),
        ("garlic", 0.10),
        ("oatmeal", 0.25),
        ("rosewater", 0.20),
    ] {
        ctx.db.item().insert(Item {
            id: id.into(),
            weight,
            base_value: adventuresim_core::strategic_economy::medicinal_ingredient_value(id),
            kind: ItemKind::Ingredient,
            ..Item::default()
        });
    }
    for recipe in adventuresim_core::disease::MEDICATION_RECIPES {
        ctx.db.item().insert(Item {
            id: recipe.item_id.into(),
            weight: 0.25,
            base_value: Some(1),
            kind: ItemKind::Medication,
            ..Item::default()
        });
    }
    ctx.db.item().insert(Item {
        id: "travel_ration".into(),
        weight: 1.0,
        base_value: Some(3),
        nutrition_kcal: 6_000.0,
        ..Item::default()
    });
    ctx.db.item().insert(Item {
        id: "waterskin".into(),
        weight: 0.5,
        base_value: Some(2),
        water_capacity_ml: 4_000,
        ..Item::default()
    });
    define_clothing(ctx, "linen_tunic", 0.6);

    for definition in WEAPONS.iter().chain(SHIELDS).chain(ARMOR) {
        ctx.db.item().insert(Item {
            id: definition.id.into(),
            weight: definition.weight,
            base_value: Some(definition.base_value),
            slot: definition.slot,
            kind: definition.kind,
            accuracy: definition.accuracy,
            reach: definition.reach,
            block: definition.block,
            coverage: definition.coverage,
            penetration: definition.penetration,
            resistance: definition.resistance,
            padding: definition.padding,
            flexibility: definition.flexibility,
            range_of_motion: definition.range_of_motion,
            precise: definition.precise,
            balance: definition.balance,
            melee: definition.melee,
            ranged: definition.ranged,
            blunt: definition.blunt,
            slash: definition.slash,
            pierce: definition.pierce,
            quality: equipment_quality(definition.id),
            durability_yield: if definition.kind == ItemKind::Armor {
                35.0
            } else {
                65.0
            },
            durability_fracture: if definition.kind == ItemKind::Armor {
                130.0
            } else {
                100.0
            },
            durability_wear: if definition.kind == ItemKind::Armor {
                0.18
            } else {
                0.10
            },
            durability_failure_share: if matches!(definition.id, "brigandine" | "jack_of_plates") {
                0.08
            } else {
                0.65
            },
            edge_sensitivity: if definition.kind == ItemKind::Weapon && !definition.blunt {
                0.8
            } else {
                0.2
            },
            handling_sensitivity: if definition.kind == ItemKind::Armor {
                0.7
            } else {
                0.5
            },
            ..Item::default()
        });
    }

    Ok(())
}

/// Applies recruitment precision calibration to databases created before the
/// numeric weapon-precision scale was introduced.
#[reducer]
pub fn calibrate_weapon_precision(ctx: &ReducerContext) {
    for (item_id, accuracy) in [("club", 0.5), ("halberd", 1.1)] {
        if let Some(mut item) = ctx.db.item().id().find(item_id.to_string()) {
            item.accuracy = accuracy;
            ctx.db.item().id().update(item);
        }
    }
}

#[reducer]
pub fn define_item(ctx: &ReducerContext, item_id: &str, weight: f32) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        base_value: Some((weight * 10.0).ceil() as u32),
        kind: ItemKind::Simple,
        ..Item::default()
    });
}

/// Backfill base values for item records created before values were added to
/// the item schema. New records receive these values in their definition
/// reducers; this migration intentionally leaves existing explicit values
/// untouched.
#[reducer]
pub fn backfill_item_values(ctx: &ReducerContext) {
    for mut item in ctx.db.item().iter() {
        if item.base_value.is_some() {
            continue;
        }

        let multiplier = match item.kind {
            ItemKind::Simple => 10.0,
            ItemKind::Weapon => 15.0,
            ItemKind::Armor | ItemKind::Clothing => 25.0,
            ItemKind::Shield => 8.0,
            ItemKind::Currency => 1.0,
            ItemKind::Ingredient => 10.0,
            ItemKind::Medication => 1.0,
        };
        item.base_value = Some((item.weight * multiplier).ceil() as u32);
        ctx.db.item().id().update(item);
    }
}

#[reducer]
pub fn define_weapon(
    ctx: &ReducerContext,
    item_id: &str,
    weight: f32,
    accuracy: f32,
    penetration: f32,
    reach: f32,
    balance: f32,
    precise: bool,
    melee: bool,
    ranged: bool,
    blunt: bool,
    slash: bool,
    pierce: bool,
) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        base_value: Some((weight * 15.0).ceil() as u32),
        accuracy,
        penetration,
        reach,
        balance,
        precise,
        melee,
        ranged,
        blunt,
        slash,
        pierce,
        kind: ItemKind::Weapon,
        quality: 1,
        durability_yield: 55.0,
        durability_fracture: 95.0,
        durability_wear: 0.1,
        durability_failure_share: 0.65,
        edge_sensitivity: if slash || pierce { 0.8 } else { 0.2 },
        handling_sensitivity: 0.5,
        ..Item::default()
    });
}

#[reducer]
pub fn define_shield(ctx: &ReducerContext, item_id: &str, weight: f32, block: f32) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        base_value: Some((weight * 8.0).ceil() as u32),
        block,
        kind: ItemKind::Shield,
        quality: 1,
        durability_yield: 35.0,
        durability_fracture: 100.0,
        durability_wear: 0.15,
        durability_failure_share: 0.4,
        handling_sensitivity: 0.8,
        ..Item::default()
    });
}

#[reducer]
pub fn define_clothing(ctx: &ReducerContext, item_id: &str, weight: f32) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        base_value: Some((weight * 25.0).ceil() as u32),
        kind: ItemKind::Clothing,
        ..Item::default()
    });
}

#[reducer]
pub fn define_armor(
    ctx: &ReducerContext,
    item_id: &str,
    weight: f32,
    slot: ItemSlot,
    coverage: f32,
    resistance: f32,
    padding: f32,
    flexibility: f32,
    range_of_motion: f32,
) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        base_value: Some((weight * 25.0).ceil() as u32),
        slot,
        coverage,
        resistance,
        padding,
        flexibility,
        range_of_motion,
        kind: ItemKind::Armor,
        quality: 1,
        durability_yield: 30.0,
        durability_fracture: 125.0,
        durability_wear: 0.18,
        durability_failure_share: 0.65,
        handling_sensitivity: 0.7,
        ..Item::default()
    });
}

pub fn add_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    quantity: u32,
) -> Option<u64> {
    if quantity == 0 {
        return None;
    }

    let kind = ctx
        .db
        .item()
        .id()
        .find(item_id.to_owned())
        .map(|definition| definition.kind);
    let durable = kind
        .is_some_and(|kind| matches!(kind, ItemKind::Weapon | ItemKind::Armor | ItemKind::Shield));
    let individual = durable || kind == Some(ItemKind::Medication);
    let count = if individual { quantity } else { 1 };
    let mut first = None;
    for _ in 0..count {
        let item = ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id,
            item_id: item_id.to_string(),
            quantity: if individual { 1 } else { quantity },
        });
        if durable {
            crate::repair::initialize_item_condition(ctx, &item);
        }
        first.get_or_insert(item.id);
    }
    first
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
        .is_some_and(|definition| {
            matches!(
                definition.kind,
                ItemKind::Weapon | ItemKind::Armor | ItemKind::Shield
            )
        });
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
            let mut equip = ctx.db.character_equip().character_id().find(character_id);
            let removal_ids = adventuresim_core::durability::durable_removal_ids(
                instances
                    .iter()
                    .map(|item| {
                        (
                            item.id,
                            equip
                                .as_ref()
                                .is_some_and(|equip| crate::repair::is_equipped(equip, item.id)),
                        )
                    })
                    .collect(),
                remove,
            );
            let mut equipment_changed = false;
            for id in removal_ids {
                if let Some(equip) = equip.as_mut()
                    && crate::repair::is_equipped(equip, id)
                {
                    crate::repair::unequip(equip, id);
                    equipment_changed = true;
                }
                ctx.db.inventory_item().id().delete(id);
                ctx.db.item_condition().inventory_item_id().delete(id);
            }
            if equipment_changed {
                ctx.db
                    .character_equip()
                    .character_id()
                    .update(equip.expect("equipment exists when it changed"));
                crate::capability::refresh_character_capability(ctx, character_id)?;
            }
        }
        return Ok(());
    }
    let mut is_found = false;

    for mut item in ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
    {
        item.quantity = item.quantity.saturating_add_signed(by_quantity);
        ctx.db.inventory_item().id().update(item);
        is_found = true;
    }

    if !is_found && by_quantity > 0 {
        add_inventory_item(ctx, character_id, item_id, by_quantity as u32);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn historical_equipment_catalog_is_well_formed() {
        let definitions: Vec<_> = WEAPONS.iter().chain(SHIELDS).chain(ARMOR).collect();
        let mut ids = HashSet::new();

        assert!(WEAPONS.len() > ARMOR.len());
        assert_eq!(
            ARMOR
                .iter()
                .filter(|definition| definition.slot == ItemSlot::Head)
                .count(),
            8
        );

        for definition in definitions {
            assert!(
                ids.insert(definition.id),
                "duplicate item id: {}",
                definition.id
            );
            assert!(definition.weight > 0.0, "{} has no weight", definition.id);
            assert!(definition.base_value > 0, "{} has no value", definition.id);
            assert!(
                !definition.id.starts_with("bot_"),
                "{} is a placeholder rather than historical equipment",
                definition.id
            );

            match definition.kind {
                ItemKind::Weapon => {
                    assert_eq!(definition.slot, ItemSlot::AnyHolding);
                    assert!(definition.accuracy > 0.0);
                    assert!(definition.reach > 0.0);
                    assert!(definition.blunt || definition.slash || definition.pierce);
                    assert_ne!(definition.melee, definition.ranged);
                }
                ItemKind::Armor => {
                    assert!(matches!(
                        definition.slot,
                        ItemSlot::AnyArm
                            | ItemSlot::AnyLeg
                            | ItemSlot::Chest
                            | ItemSlot::Stomach
                            | ItemSlot::Head
                    ));
                    assert!((0.0..=1.0).contains(&definition.coverage));
                    assert!(definition.resistance > 0.0);
                    assert!(definition.padding > 0.0);
                    assert!((0.0..=1.0).contains(&definition.flexibility));
                    assert!((0.0..=1.0).contains(&definition.range_of_motion));
                }
                ItemKind::Shield => {
                    assert_eq!(definition.slot, ItemSlot::AnyHolding);
                    assert!((1.0..=5.0).contains(&definition.block));
                }
                _ => unreachable!("equipment catalog contains a non-equipment item"),
            }
        }
    }

    #[test]
    fn development_catalog_exercises_every_quality_level() {
        let qualities: HashSet<_> = WEAPONS
            .iter()
            .chain(SHIELDS)
            .chain(ARMOR)
            .map(|definition| equipment_quality(definition.id))
            .collect();
        assert_eq!(qualities, HashSet::from([1, 2, 3, 4, 5]));
    }
}
