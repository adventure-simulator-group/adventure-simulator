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
}

#[reducer(init)]
fn init_items(ctx: &ReducerContext) -> Result<(), String> {
    crate::time::initialize_time(ctx);
    log::info!("Populating items...");

    define_item(ctx, "torch", 0.5);
    ctx.db.item().insert(Item {
        id: "gold_coin".into(),
        weight: 0.01,
        base_value: Some(1),
        kind: ItemKind::Currency,
        ..Item::default()
    });
    define_item(ctx, "bandage", 0.05);
    define_clothing(ctx, "linen_tunic", 0.6);

    define_shield(ctx, "buckler", 1.0, 1.0);

    define_weapon(
        ctx,
        "short_sword",
        1.5,
        1.5,
        1.0,
        1.0,
        0.5,
        false,
        true,
        false,
        false,
        true,
        true,
    );
    define_weapon(
        ctx, "knife", 0.5, 2.0, 1.0, 1.0, 0.5, false, true, false, false, true, true,
    );
    define_weapon(
        ctx,
        "zweihander",
        6.0,
        0.6,
        1.0,
        2.0,
        0.3,
        false,
        true,
        false,
        false,
        true,
        false,
    );
    define_weapon(
        ctx, "club", 2.0, 0.5, 0.5, 1.0, 0.3, false, true, false, true, false, false,
    );
    define_weapon(
        ctx,
        "short_bow",
        1.0,
        1.2,
        1.0,
        20.0,
        0.4,
        false,
        false,
        true,
        false,
        false,
        true,
    );
    define_weapon(
        ctx,
        "bot_multirole_weapon",
        4.0,
        2.0,
        1.0,
        2.0,
        0.5,
        true,
        true,
        true,
        true,
        true,
        true,
    );
    define_weapon(
        ctx, "rapier", 1.2, 2.0, 1.2, 1.8, 0.7, true, true, false, false, false, true,
    );
    define_weapon(
        ctx, "rondel", 0.5, 2.0, 1.0, 0.6, 0.8, true, true, false, false, false, true,
    );
    define_weapon(
        ctx,
        "misericorde",
        0.5,
        2.0,
        1.0,
        0.7,
        0.8,
        true,
        true,
        false,
        false,
        false,
        true,
    );

    define_armor(
        ctx,
        "leather_armguard",
        0.5,
        ItemSlot::AnyArm,
        0.2,
        60.0,
        40.0,
        1.0,
        0.9,
    );
    define_armor(
        ctx,
        "leather_helmet",
        0.5,
        ItemSlot::Head,
        0.3,
        60.0,
        40.0,
        1.0,
        0.9,
    );
    define_armor(
        ctx,
        "leather_vest",
        0.5,
        ItemSlot::Chest,
        0.3,
        60.0,
        40.0,
        1.0,
        0.9,
    );
    define_armor(
        ctx,
        "leather_belt",
        0.5,
        ItemSlot::Stomach,
        0.2,
        60.0,
        40.0,
        1.0,
        0.9,
    );
    define_armor(
        ctx,
        "leather_cuisse",
        0.5,
        ItemSlot::AnyLeg,
        0.4,
        60.0,
        40.0,
        1.0,
        0.9,
    );
    define_armor(
        ctx,
        "steel_sallet",
        2.0,
        ItemSlot::Head,
        0.7,
        70.0,
        40.0,
        0.4,
        0.3,
    );
    define_armor(
        ctx,
        "bot_plate_arm",
        1.5,
        ItemSlot::AnyArm,
        0.9,
        80.0,
        60.0,
        0.4,
        0.4,
    );
    define_armor(
        ctx,
        "bot_plate_leg",
        2.0,
        ItemSlot::AnyLeg,
        0.9,
        80.0,
        60.0,
        0.4,
        0.4,
    );
    define_armor(
        ctx,
        "bot_plate_chest",
        3.0,
        ItemSlot::Chest,
        0.9,
        80.0,
        60.0,
        0.4,
        0.4,
    );
    define_armor(
        ctx,
        "bot_plate_stomach",
        2.0,
        ItemSlot::Stomach,
        0.9,
        80.0,
        60.0,
        0.4,
        0.4,
    );
    define_armor(
        ctx,
        "bot_plate_helmet",
        2.0,
        ItemSlot::Head,
        0.9,
        80.0,
        60.0,
        0.4,
        0.4,
    );

    Ok(())
}

/// Applies recruitment precision calibration to databases created before the
/// numeric weapon-precision scale was introduced.
#[reducer]
pub fn calibrate_weapon_precision(ctx: &ReducerContext) {
    for (item_id, accuracy) in [("club", 0.5), ("bot_multirole_weapon", 2.0)] {
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

    let item = ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: character_id,
        item_id: item_id.to_string(),
        quantity,
    });

    Some(item.id)
}

#[reducer]
pub fn change_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    by_quantity: i32,
) {
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

    if !is_found {
        add_inventory_item(ctx, character_id, item_id, by_quantity as u32);
    }
}
