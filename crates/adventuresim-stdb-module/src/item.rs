use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};
use strum::{EnumCount, VariantArray};

/// [`Item`] that is in the inventory
#[derive(Clone, Debug)]
#[table(
    name = inventory_item, public,
    index(name = character_and_item_id, btree(columns = [character_id, item_id])),
    index(name = character_and_id, btree(columns = [character_id, id])),
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
#[table(name = item, public)]
pub struct Item {
    #[primary_key]
    pub id: String,
    pub weight: f32,
    pub slot: ItemSlot,
    pub kind: ItemKind,
    pub accuracy: f32,
    pub block: f32,
    pub dodge: f32,
    pub coverage: f32,
}

#[reducer(init)]
fn init_items(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Populating items...");

    define_item(ctx, "torch", 0.5);
    define_item(ctx, "bandage", 0.05);

    define_shield(ctx, "buckler", 1.0, 1.0);

    define_weapon(ctx, "short_sword", 1.5, 1.5);
    define_weapon(ctx, "knife", 0.5, 2.0);
    define_weapon(ctx, "zweihander", 6.0, 0.6);

    define_armor(ctx, "leather_armguard", 0.5, ItemSlot::AnyArm, 1.0, 0.2);
    define_armor(ctx, "leather_helmet", 0.5, ItemSlot::Head, 1.0, 0.3);
    define_armor(ctx, "leather_vest", 0.5, ItemSlot::Chest, 1.0, 0.3);
    define_armor(ctx, "leather_belt", 0.5, ItemSlot::Stomach, 1.0, 0.2);
    define_armor(ctx, "leather_cuisse", 0.5, ItemSlot::AnyLeg, 1.0, 0.4);
    define_armor(ctx, "steel_sallet", 2.0, ItemSlot::Head, 0.6, 0.7);

    Ok(())
}

#[reducer]
pub fn define_item(ctx: &ReducerContext, item_id: &str, weight: f32) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        kind: ItemKind::Simple,
        ..Item::default()
    });
}

#[reducer]
pub fn define_weapon(ctx: &ReducerContext, item_id: &str, weight: f32, accuracy: f32) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        accuracy,
        kind: ItemKind::Weapon,
        ..Item::default()
    });
}

#[reducer]
pub fn define_shield(ctx: &ReducerContext, item_id: &str, weight: f32, block: f32) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        block,
        kind: ItemKind::Shield,
        ..Item::default()
    });
}

#[reducer]
pub fn define_armor(
    ctx: &ReducerContext,
    item_id: &str,
    weight: f32,
    slot: ItemSlot,
    dodge: f32,
    coverage: f32,
) {
    ctx.db.item().insert(Item {
        id: item_id.to_string(),
        weight,
        slot,
        dodge,
        coverage,
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
