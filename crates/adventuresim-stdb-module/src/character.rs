use spacetimedb::{Identity, ReducerContext, Table, reducer, table};
use std::hash::{DefaultHasher, Hash, Hasher};
use strum::VariantArray;

use crate::{ItemSlot, add_inventory_item, enter_mission, inventory_item};

/// General character info
#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    pub id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    pub gold: u32,
    pub current_settlement_id: Option<String>,
    pub party_id: Option<String>,
    #[index(btree)]
    pub server: Identity,
    pub in_server: bool,
    pub temporary: bool,
}

/// [`Character`] attributes
#[derive(Clone, Debug)]
#[table(name = character_attributes, public)]
pub struct CharacterAttributes {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

/// [`Character`] stats
#[derive(Clone, Debug)]
#[table(name = character_stats, public)]
pub struct CharacterStats {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    pub calories_used: f32,
    pub focus: f32,
}

/// [`Character`] skills
#[derive(Clone, Debug)]
#[table(name = character_skills, public)]
pub struct CharacterSkills {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    pub melee_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub ranged_hours: f32,
    pub will_hours: f32,
    pub charisma_hours: f32,
    pub medicine_hours: f32,
    pub faith_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub surgeon_hours: f32,
}

/// [`Character`] limbs
#[derive(Clone, Debug)]
#[table(name = character_limbs, public)]
pub struct CharacterLimbs {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    pub left_arm_health: f32,
    pub right_arm_health: f32,
    pub left_leg_health: f32,
    pub right_leg_health: f32,
    pub head_health: f32,
    pub chest_health: f32,
    pub stomach_health: f32,
}

/// [`Character`] equipment
#[derive(Clone, Debug)]
#[table(name = character_equip, public)]
pub struct CharacterEquip {
    #[index(direct)]
    #[unique]
    pub character_id: u64,
    // weapon or shield
    pub left_hand_item_id: Option<u64>,
    pub right_hand_item_id: Option<u64>,
    // armor
    pub left_arm_armor_id: Option<u64>,
    pub right_arm_armor_id: Option<u64>,
    pub left_leg_armor_id: Option<u64>,
    pub right_leg_armor_id: Option<u64>,
    pub head_armor_id: Option<u64>,
    pub chest_armor_id: Option<u64>,
    pub stomach_armor_id: Option<u64>,
}

impl CharacterEquip {
    pub fn is_equiped(&self, inventory_item_id: u64) -> Option<ItemSlot> {
        ItemSlot::VARIANTS
            .iter()
            .find(|slot| self.get(**slot) == Some(inventory_item_id))
            .copied()
    }

    pub fn get(&self, slot: ItemSlot) -> Option<u64> {
        match slot {
            ItemSlot::LeftHolding => self.left_hand_item_id,
            ItemSlot::RightHolding => self.right_hand_item_id,
            ItemSlot::LeftArm => self.left_arm_armor_id,
            ItemSlot::RightArm => self.right_arm_armor_id,
            ItemSlot::LeftLeg => self.left_leg_armor_id,
            ItemSlot::RightLeg => self.right_leg_armor_id,
            ItemSlot::Head => self.head_armor_id,
            ItemSlot::Chest => self.chest_armor_id,
            ItemSlot::Stomach => self.stomach_armor_id,
            _ => None,
        }
    }
}

/// Create a new random temporary character for the server.
#[reducer]
pub fn create_temporary_character(ctx: &ReducerContext, server: Identity) -> Result<(), String> {
    use petname::Generator;

    let name = petname::Petnames::default()
        .generate(&mut ctx.rng(), 1, " ")
        .ok_or_else(|| format!("Can't generate a name for a temporary character"))?;
    let name = format!("bot-{name}");

    let mut id = ctx.random();
    id |= 0b1000_1000_1000_1000;

    insert_new_character(ctx, name, id, true)?;
    enter_mission(ctx, id, server)
}

/// Create a new character with generated name and add initial items to it
#[reducer]
pub fn create_character(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    use petname::Generator;

    let name = petname::Petnames::default()
        .generate(&mut ctx.rng(), 2, " ")
        .ok_or_else(|| format!("Can't generate a name for a character with id {id}"))?;

    insert_new_character(ctx, name, id, false)
}

/// Create a new character with name and add initial items to it
#[reducer]
pub fn create_named_character(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    ctx.timestamp.hash(&mut hasher);
    let id = hasher.finish();

    insert_new_character(ctx, name, id, false)
}

/// Create a named character with a caller-supplied ID.
#[reducer]
pub fn create_named_character_with_id(
    ctx: &ReducerContext,
    id: u64,
    name: String,
) -> Result<(), String> {
    insert_new_character(ctx, name, id, false)
}

/// Seed an injured character for local UI development and visual verification.
#[reducer]
pub fn seed_damaged_character(ctx: &ReducerContext) -> Result<(), String> {
    const DAMAGED_CHARACTER_ID: u64 = 9_999_999_999_999_999;

    if ctx
        .db
        .character()
        .id()
        .find(DAMAGED_CHARACTER_ID)
        .is_none()
    {
        insert_new_character(
            ctx,
            "Wounded Demo".to_string(),
            DAMAGED_CHARACTER_ID,
            false,
        )?;
    }

    let mut limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(DAMAGED_CHARACTER_ID)
        .ok_or_else(|| "Damaged demo character is missing limb data".to_string())?;

    limbs.left_arm_health = 0.55;
    limbs.right_arm_health = 0.80;
    limbs.left_leg_health = 0.65;
    limbs.right_leg_health = 0.90;
    limbs.head_health = 0.75;
    limbs.chest_health = 0.85;
    limbs.stomach_health = 0.70;
    ctx.db.character_limbs().character_id().update(limbs);

    Ok(())
}

#[reducer]
pub(crate) fn insert_new_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    temporary: bool,
) -> Result<(), String> {
    log::info!("New character created: {name} (ID: {id})");

    let character = ctx.db.character().insert(Character {
        id,
        name,
        xp: 0,
        level: 1,
        gold: 100,
        current_settlement_id: Some("riverdale".into()),
        party_id: None,
        server: Identity::ZERO,
        in_server: false,
        temporary,
    });
    let _character_stats = ctx.db.character_stats().insert(CharacterStats {
        character_id: id,
        calories_used: 0.0,
        focus: 1.0,
    });
    let _character_skills = ctx.db.character_skills().insert(CharacterSkills {
        character_id: id,
        melee_hours: 2000.0,
        dodge_hours: 1000.0,
        block_hours: 1000.0,
        ranged_hours: 1000.0,
        will_hours: 1000.0,
        charisma_hours: 1000.0,
        medicine_hours: 1000.0,
        faith_hours: 1000.0,
        stealth_hours: 1000.0,
        balance_hours: 1000.0,
        surgeon_hours: 1000.0,
    });
    let _character_limbs = ctx.db.character_limbs().insert(CharacterLimbs {
        character_id: id,
        left_arm_health: 1.0,
        right_arm_health: 1.0,
        left_leg_health: 1.0,
        right_leg_health: 1.0,
        head_health: 1.0,
        chest_health: 1.0,
        stomach_health: 1.0,
    });
    let _character_equip = ctx.db.character_equip().insert(CharacterEquip {
        character_id: id,
        left_hand_item_id: None,
        right_hand_item_id: None,
        left_arm_armor_id: None,
        right_arm_armor_id: None,
        left_leg_armor_id: None,
        right_leg_armor_id: None,
        head_armor_id: None,
        chest_armor_id: None,
        stomach_armor_id: None,
    });
    let _character_attrs = ctx.db.character_attributes().insert(CharacterAttributes {
        character_id: id,
        endurance: 2.0,
        immunity: 2.0,
        gut: 2.0,
        precision: 2.0,
        intelligence: 2.0,
        instinct: 2.0,
        eyesight: 2.0,
        hearing: 2.0,
        left_arm_strength: 3.0,
        right_arm_strength: 3.0,
        left_leg_strength: 3.0,
        right_leg_strength: 3.0,
        left_arm_agility: 3.0,
        right_arm_agility: 3.0,
        left_leg_agility: 3.0,
        right_leg_agility: 3.0,
    });

    // Starter items
    add_inventory_item(ctx, character.id, "torch", 1);
    add_inventory_item(ctx, character.id, "bandage", 3);

    // Starter equip
    add_and_equip_item(ctx, character.id, "buckler", ItemSlot::LeftHolding)?;
    add_and_equip_item(ctx, character.id, "short_sword", ItemSlot::RightHolding)?;
    add_and_equip_item(ctx, character.id, "leather_armguard", ItemSlot::LeftArm)?;
    add_and_equip_item(ctx, character.id, "leather_armguard", ItemSlot::RightArm)?;
    add_and_equip_item(ctx, character.id, "leather_helmet", ItemSlot::Head)?;
    add_and_equip_item(ctx, character.id, "leather_vest", ItemSlot::Chest)?;
    add_and_equip_item(ctx, character.id, "leather_vest", ItemSlot::Stomach)?;
    add_and_equip_item(ctx, character.id, "leather_cuisse", ItemSlot::LeftLeg)?;
    add_and_equip_item(ctx, character.id, "leather_cuisse", ItemSlot::RightLeg)?;

    Ok(())
}

#[reducer]
pub fn equip_item(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    destination: ItemSlot,
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
        ItemSlot::AnyHolding if equip.left_hand_item_id.is_none() => {
            equip.left_hand_item_id = Some(inventory_item_id)
        }
        ItemSlot::AnyHolding if equip.right_hand_item_id.is_none() => {
            equip.right_hand_item_id = Some(inventory_item_id)
        }
        ItemSlot::RightHolding | ItemSlot::AnyHolding => {
            equip.right_hand_item_id = Some(inventory_item_id)
        }
        ItemSlot::LeftHolding => equip.left_hand_item_id = Some(inventory_item_id),
        ItemSlot::AnyArm if equip.left_arm_armor_id.is_none() => {
            equip.left_arm_armor_id = Some(inventory_item_id)
        }
        ItemSlot::AnyArm if equip.right_arm_armor_id.is_none() => {
            equip.right_arm_armor_id = Some(inventory_item_id)
        }
        ItemSlot::RightArm | ItemSlot::AnyArm => equip.right_arm_armor_id = Some(inventory_item_id),
        ItemSlot::LeftArm => equip.left_arm_armor_id = Some(inventory_item_id),
        ItemSlot::AnyLeg if equip.left_leg_armor_id.is_none() => {
            equip.left_leg_armor_id = Some(inventory_item_id)
        }
        ItemSlot::AnyLeg if equip.right_leg_armor_id.is_none() => {
            equip.right_leg_armor_id = Some(inventory_item_id)
        }
        ItemSlot::RightLeg | ItemSlot::AnyLeg => equip.right_leg_armor_id = Some(inventory_item_id),
        ItemSlot::LeftLeg => equip.left_leg_armor_id = Some(inventory_item_id),
        ItemSlot::Head => equip.head_armor_id = Some(inventory_item_id),
        ItemSlot::Chest => equip.chest_armor_id = Some(inventory_item_id),
        ItemSlot::Stomach => equip.stomach_armor_id = Some(inventory_item_id),
        ItemSlot::None => {}
    }

    ctx.db.character_equip().character_id().update(equip);
    Ok(())
}

#[reducer]
pub fn add_and_equip_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    destination: ItemSlot,
) -> Result<(), String> {
    let id = add_inventory_item(ctx, character_id, item_id, 1)
        .ok_or_else(|| "Can't add item to inventory".to_string())?;
    equip_item(ctx, character_id, id, destination)
}
