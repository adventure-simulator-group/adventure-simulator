use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::{add_inventory_item, inventory_item};

/// General character info
#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    pub id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    #[index(btree)]
    pub in_server: String,
}

/// [`Character`] skills
#[derive(Clone, Debug)]
#[table(name = character_skills, public)]
pub struct CharacterSkills {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    pub melee: f32,
    pub dodge: f32,
    pub block: f32,
}

/// [`Character`] limbs
#[derive(Clone, Debug)]
#[table(name = character_limbs, public)]
pub struct CharacterLimbs {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    pub left_arm: f32,
    pub right_arm: f32,
    pub left_leg: f32,
    pub right_leg: f32,
    pub head: f32,
    pub torso: f32,
}

/// [`Character`] equipment
#[derive(Clone, Debug)]
#[table(name = character_equip, public)]
pub struct CharacterEquip {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    // weapon or shield
    pub left_arm_item_id: Option<u64>,
    pub right_arm_item_id: Option<u64>,
    // armor
    pub left_arm_armor_id: Option<u64>,
    pub right_arm_armor_id: Option<u64>,
    pub left_leg_armor_id: Option<u64>,
    pub right_leg_armor_id: Option<u64>,
    pub head_armor_id: Option<u64>,
    pub torso_armor_id: Option<u64>,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq)]
pub enum EquipDst {
    LeftArm,
    RightArm,
    ArmorLeftArm,
    ArmorRightArm,
    ArmorLeftLeg,
    ArmorRightLeg,
    ArmorHead,
    ArmorTorso,
}

/// Create a new character with generated name and add initial items to it
#[reducer]
pub fn create_character(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    use petname::Generator;
    use rand::SeedableRng;

    let mut rng = rand::rngs::SmallRng::seed_from_u64(id);
    let name = petname::Petnames::default()
        .generate(&mut rng, 2, " ")
        .ok_or_else(|| format!("Can't generate a name for a character with id {id}"))?;

    insert_new_character(ctx, name, id)
}

/// Create a new character with name and add initial items to it
#[reducer]
pub fn create_named_character(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    ctx.timestamp.hash(&mut hasher);
    let id = hasher.finish();

    insert_new_character(ctx, name, id)
}

#[reducer]
fn insert_new_character(ctx: &ReducerContext, name: String, id: u64) -> Result<(), String> {
    log::info!("New character created: {name} (ID: {id})");

    let character = ctx.db.character().insert(Character {
        id,
        name,
        xp: 0,
        level: 1,
        in_server: String::new(),
    });
    let _character_stats = ctx.db.character_skills().insert(CharacterSkills {
        character_id: id,
        melee: 1.0,
        dodge: 1.0,
        block: 1.0,
    });
    let _character_limbs = ctx.db.character_limbs().insert(CharacterLimbs {
        character_id: id,
        left_arm: 1.0,
        right_arm: 1.0,
        left_leg: 1.0,
        right_leg: 1.0,
        head: 1.0,
        torso: 1.0,
    });
    let _character_equip = ctx.db.character_equip().insert(CharacterEquip {
        character_id: id,
        left_arm_item_id: None,
        right_arm_item_id: None,
        left_arm_armor_id: None,
        right_arm_armor_id: None,
        left_leg_armor_id: None,
        right_leg_armor_id: None,
        head_armor_id: None,
        torso_armor_id: None,
    });

    // Starter items
    add_inventory_item(ctx, character.id, "torch", 1);
    add_inventory_item(ctx, character.id, "bandage", 3);

    // Starter equip
    add_and_equip_item(ctx, character.id, "buckler", EquipDst::LeftArm)?;
    add_and_equip_item(ctx, character.id, "short_sword", EquipDst::RightArm)?;
    add_and_equip_item(
        ctx,
        character.id,
        "leather_armguard",
        EquipDst::ArmorLeftArm,
    )?;
    add_and_equip_item(
        ctx,
        character.id,
        "leather_armguard",
        EquipDst::ArmorRightArm,
    )?;
    add_and_equip_item(ctx, character.id, "leather_helmet", EquipDst::ArmorHead)?;
    add_and_equip_item(ctx, character.id, "leather_vest", EquipDst::ArmorTorso)?;
    add_and_equip_item(ctx, character.id, "leather_cuisse", EquipDst::ArmorLeftLeg)?;
    add_and_equip_item(ctx, character.id, "leather_cuisse", EquipDst::ArmorRightLeg)?;

    Ok(())
}

#[reducer]
pub fn equip_item(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    destination: EquipDst,
) -> Result<(), String> {
    if ctx
        .db
        .inventory_item()
        .character_and_id()
        .filter((character_id, inventory_item_id))
        .next()
        .is_none()
    {
        return Err(format!(
            "Can't equip item: InventoryItem@{inventory_item_id} doesn't exist for Character@{character_id}"
        ));
    }

    let mut equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Can't find character".to_string())?;

    match destination {
        EquipDst::LeftArm => equip.left_arm_item_id = Some(inventory_item_id),
        EquipDst::RightArm => equip.right_arm_item_id = Some(inventory_item_id),
        EquipDst::ArmorLeftArm => equip.left_arm_armor_id = Some(inventory_item_id),
        EquipDst::ArmorRightArm => equip.right_arm_armor_id = Some(inventory_item_id),
        EquipDst::ArmorLeftLeg => equip.left_leg_armor_id = Some(inventory_item_id),
        EquipDst::ArmorRightLeg => equip.right_leg_armor_id = Some(inventory_item_id),
        EquipDst::ArmorHead => equip.head_armor_id = Some(inventory_item_id),
        EquipDst::ArmorTorso => equip.torso_armor_id = Some(inventory_item_id),
    }

    ctx.db.character_equip().character_id().update(equip);
    Ok(())
}

#[reducer]
pub fn add_and_equip_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    destination: EquipDst,
) -> Result<(), String> {
    let id = add_inventory_item(ctx, character_id, item_id, 1)
        .ok_or_else(|| "Can't add item to inventory".to_string())?;
    equip_item(ctx, character_id, id, destination)
}
