use spacetimedb::{Identity, ReducerContext, Table, reducer, table};
use std::hash::{DefaultHasher, Hash, Hasher};
use strum::VariantArray;

use crate::{
    CharacterTrainingSchedule, ItemSlot, ScheduleAllocation, Settlement, add_inventory_item,
    character_training_schedule, enter_mission, inventory_item,
    strategic::{party, settlement},
};

/// General character info
#[derive(Clone, Debug)]
#[table(accessor = character, public)]
pub struct Character {
    #[primary_key]
    pub id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    pub gold: u32,
    pub current_settlement_id: Option<String>,
    /// The quest location occupied by this character, mutually exclusive with a settlement.
    pub current_quest_location_id: Option<String>,
    pub party_id: Option<String>,
    #[index(btree)]
    pub server: Identity,
    pub in_server: bool,
    pub temporary: bool,
    #[default(25)]
    pub age_years: u16,
    /// Strategic life state. Death transitions are intentionally deferred to the
    /// future death system, but parties already use this to govern succession.
    #[default(true)]
    pub alive: bool,
}

/// [`Character`] attributes
#[derive(Clone, Debug)]
#[table(accessor = character_attributes, public)]
pub struct CharacterAttributes {
    #[primary_key]
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
#[table(accessor = character_stats, public)]
pub struct CharacterStats {
    #[primary_key]
    pub character_id: u64,
    pub calories_used: f32,
    pub focus: f32,
}

/// [`Character`] skills
#[derive(Clone, Debug)]
#[table(accessor = character_skills, public)]
pub struct CharacterSkills {
    #[primary_key]
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
#[table(accessor = character_limbs, public)]
pub struct CharacterLimbs {
    #[primary_key]
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
#[table(accessor = character_equip, public)]
pub struct CharacterEquip {
    #[primary_key]
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

/// Configure a freshly-created `sim-*` character for an isolated strategic
/// simulation. This reducer does not accept any combat seed or outcome. Its
/// fixed starting purse and profile bounds keep it useful for reproducible
/// local experiments without becoming a general-purpose character editor.
#[reducer]
pub fn configure_simulation_character(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    attributes: CharacterAttributes,
    skills: CharacterSkills,
    downtime: ScheduleAllocation,
) -> Result<(), String> {
    let mut character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Simulation character not found")?;
    if !character.name.starts_with("sim-") || character.temporary {
        return Err("Only a fresh sim-* character may be configured".into());
    }
    if attributes.character_id != character_id || skills.character_id != character_id {
        return Err("Simulation profile character IDs must match".into());
    }
    let bounded_attributes = [
        attributes.endurance,
        attributes.immunity,
        attributes.gut,
        attributes.precision,
        attributes.intelligence,
        attributes.instinct,
        attributes.eyesight,
        attributes.hearing,
        attributes.left_arm_strength,
        attributes.right_arm_strength,
        attributes.left_leg_strength,
        attributes.right_leg_strength,
        attributes.left_arm_agility,
        attributes.right_arm_agility,
        attributes.left_leg_agility,
        attributes.right_leg_agility,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.5..=5.0).contains(&value));
    let bounded_skills = [
        skills.melee_hours,
        skills.dodge_hours,
        skills.block_hours,
        skills.ranged_hours,
        skills.will_hours,
        skills.charisma_hours,
        skills.medicine_hours,
        skills.faith_hours,
        skills.stealth_hours,
        skills.balance_hours,
        skills.surgeon_hours,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.0..=1_000_000.0).contains(&value));
    if !bounded_attributes || !bounded_skills || downtime.allocated_minutes() > 1_440 {
        return Err("Simulation profile is outside bounded gameplay ranges".into());
    }
    if ctx.db.settlement().id().find(&settlement_id).is_none() {
        return Err("Simulation settlement not found".into());
    }
    let party_id = character
        .party_id
        .clone()
        .ok_or("Simulation character has no party")?;
    let mut solo_party = ctx
        .db
        .party()
        .id()
        .find(&party_id)
        .ok_or("Simulation party not found")?;
    if solo_party.leader_id != character_id || !solo_party.is_solo {
        return Err("Simulation character must still lead its fresh solo party".into());
    }
    character.current_settlement_id = Some(settlement_id.clone());
    character.current_quest_location_id = None;
    character.temporary = true;
    character.gold = 500;
    ctx.db.character().id().update(character);
    solo_party.current_settlement_id = Some(settlement_id);
    solo_party.current_quest_location_id = None;
    ctx.db.party().id().update(solo_party);
    ctx.db
        .character_attributes()
        .character_id()
        .update(attributes);
    ctx.db.character_skills().character_id().update(skills);
    let mut schedule: CharacterTrainingSchedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or("Simulation schedule not found")?;
    schedule.downtime = downtime;
    ctx.db
        .character_training_schedule()
        .character_id()
        .update(schedule);
    add_inventory_item(ctx, character_id, "gold_coin", 400);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

/// Seed an injured character for local UI development and visual verification.
#[reducer]
pub fn seed_damaged_character(ctx: &ReducerContext) -> Result<(), String> {
    const DAMAGED_CHARACTER_ID: u64 = 9_999_999_999_999_999;

    if ctx.db.character().id().find(DAMAGED_CHARACTER_ID).is_none() {
        insert_new_character(ctx, "Wounded Demo".to_string(), DAMAGED_CHARACTER_ID, false)?;
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

    let settlements: Vec<Settlement> = ctx.db.settlement().iter().collect();
    if settlements.is_empty() {
        return Err("Cannot create a character before at least one settlement is loaded".into());
    }
    let start_settlement = &settlements[ctx.random::<u64>() as usize % settlements.len()];

    let character = ctx.db.character().insert(Character {
        id,
        name,
        xp: 0,
        level: 1,
        gold: 100,
        current_settlement_id: Some(start_settlement.id.clone()),
        current_quest_location_id: None,
        party_id: None,
        server: Identity::ZERO,
        in_server: false,
        temporary,
        age_years: 25,
        alive: true,
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
    crate::time::initialize_character_time(ctx, id)?;
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
    add_inventory_item(ctx, character.id, "gold_coin", 100);
    add_inventory_item(ctx, character.id, "torch", 1);
    add_inventory_item(ctx, character.id, "bandage", 3);

    // Starter equip
    add_and_equip_item(ctx, character.id, "buckler", ItemSlot::LeftHolding)?;
    add_and_equip_item(ctx, character.id, "katzbalger", ItemSlot::RightHolding)?;
    add_and_equip_item(ctx, character.id, "quilted_sleeve", ItemSlot::LeftArm)?;
    add_and_equip_item(ctx, character.id, "quilted_sleeve", ItemSlot::RightArm)?;
    add_and_equip_item(ctx, character.id, "arming_cap", ItemSlot::Head)?;
    add_and_equip_item(ctx, character.id, "arming_doublet", ItemSlot::Chest)?;
    add_and_equip_item(ctx, character.id, "padded_skirt", ItemSlot::Stomach)?;
    add_and_equip_item(ctx, character.id, "padded_chausses", ItemSlot::LeftLeg)?;
    add_and_equip_item(ctx, character.id, "padded_chausses", ItemSlot::RightLeg)?;

    crate::strategic::create_solo_party_for_character(ctx, character.id)?;
    crate::capability::refresh_character_capability(ctx, character.id)?;
    crate::condition::initialize_character_condition(ctx, character.id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character.id)?;

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
