use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, reducer, table};
use std::hash::{DefaultHasher, Hash, Hasher};
use strum::VariantArray;

use crate::{
    ItemSlot, Settlement, add_inventory_item, enter_mission, inventory_item,
    item::item,
    repair::item_condition,
    strategic::{party, settlement},
    time::character_time,
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

#[derive(Clone, Debug, SpacetimeType)]
pub enum DeathCause {
    Combat,
    Injury,
    Disease,
    RespiratoryFailure,
    CirculatoryFailure,
    HomeostaticFailure,
    NeurologicFailure,
    Starvation,
    Dehydration,
    Other,
    DevTest,
}

#[derive(Clone, Debug, SpacetimeType)]
pub enum DeathSource {
    Tactical,
    Autoresolve,
    Strategic,
    Disease,
    DevTest,
}

/// Immutable first-known death context. Tactical state is deliberately absent;
/// committed outcomes pass only their durable cause/source into this boundary.
#[derive(Clone, Debug)]
#[table(accessor = character_death, public)]
pub struct CharacterDeath {
    #[primary_key]
    pub character_id: u64,
    pub cause: DeathCause,
    pub source: DeathSource,
    pub source_id: Option<String>,
    pub strategic_minute: u64,
}

pub fn require_living_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Character, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.alive {
        return Err("Dead characters cannot perform this action".into());
    }
    Ok(character)
}

/// Authoritative, idempotent life-state transition shared by strategic disease
/// and committed tactical/autoresolve outcomes. Repeated calls retain the first
/// recorded cause, source, and strategic minute.
pub fn transition_character_to_dead(
    ctx: &ReducerContext,
    character_id: u64,
    cause: DeathCause,
    source: DeathSource,
    source_id: Option<String>,
) -> Result<CharacterDeath, String> {
    if let Some(death) = ctx.db.character_death().character_id().find(character_id) {
        return Ok(death);
    }
    let mut character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let strategic_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let death = ctx.db.character_death().insert(CharacterDeath {
        character_id,
        cause,
        source,
        source_id,
        strategic_minute,
    });
    character.alive = false;
    character.in_server = false;
    character.server = Identity::ZERO;
    let party_id = character.party_id.clone();
    ctx.db.character().id().update(character);
    if let Some(party_id) = party_id {
        crate::strategic::normalize_and_elect_party_leader(ctx, &party_id)?;
    }
    Ok(death)
}

/// Non-destructive upgrade path for legacy rows that predate durable death
/// context and standing vote normalization.
#[reducer]
pub fn backfill_character_deaths_and_leadership(ctx: &ReducerContext) -> Result<(), String> {
    let dead_ids: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|character| !character.alive)
        .map(|character| character.id)
        .collect();
    for character_id in dead_ids {
        transition_character_to_dead(
            ctx,
            character_id,
            DeathCause::Other,
            DeathSource::Strategic,
            Some("legacy-backfill".into()),
        )?;
    }
    let party_ids: Vec<_> = ctx.db.party().iter().map(|party| party.id).collect();
    for party_id in party_ids {
        crate::strategic::normalize_and_elect_party_leader(ctx, &party_id)?;
    }
    Ok(())
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
    pub religion_hours: adventuresim_world_schema::ReligionHours,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub surgeon_hours: f32,
    pub smithing_hours: f32,
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

    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(DAMAGED_CHARACTER_ID)
        .ok_or_else(|| "Damaged demo character is missing equipment data".to_string())?;

    // Exercise field, settlement, and beyond-smith repair states across both
    // specialist screens. Equipped pieces make the durability column useful,
    // while the two spares also exercise the combined sell/repair row actions.
    for (inventory_item_id, bins) in [
        (equip.left_hand_item_id, [0.20, 0.0, 0.0, 0.0, 0.0]),
        (equip.right_hand_item_id, [0.08, 0.17, 0.0, 0.0, 0.0]),
        (equip.left_arm_armor_id, [0.18, 0.0, 0.0, 0.0, 0.0]),
        (equip.right_arm_armor_id, [0.0, 0.10, 0.10, 0.0, 0.0]),
        (equip.left_leg_armor_id, [0.10, 0.0, 0.08, 0.0, 0.0]),
        (equip.right_leg_armor_id, [0.0, 0.0, 0.0, 0.12, 0.08]),
        (equip.head_armor_id, [0.0, 0.0, 0.15, 0.05, 0.0]),
        (equip.chest_armor_id, [0.05, 0.10, 0.08, 0.07, 0.0]),
        (equip.stomach_armor_id, [0.0, 0.14, 0.0, 0.0, 0.0]),
    ] {
        if let Some(inventory_item_id) = inventory_item_id {
            set_demo_item_damage(ctx, inventory_item_id, bins)?;
        }
    }

    for (item_id, bins) in [
        ("arming_sword", [0.10, 0.08, 0.12, 0.05, 0.0]),
        ("brigandine", [0.06, 0.10, 0.08, 0.08, 0.04]),
    ] {
        let inventory_item_id = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(DAMAGED_CHARACTER_ID)
            .find(|inventory| inventory.item_id == item_id)
            .map(|inventory| inventory.id)
            .or_else(|| add_inventory_item(ctx, DAMAGED_CHARACTER_ID, item_id, 1))
            .ok_or_else(|| format!("Failed to add {item_id} to damaged demo inventory"))?;
        set_demo_item_damage(ctx, inventory_item_id, bins)?;
    }

    Ok(())
}

fn set_demo_item_damage(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    bins: [f32; 5],
) -> Result<(), String> {
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or_else(|| format!("Damaged demo item {inventory_item_id} is missing"))?;
    let quality = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .map_or(1, |definition| definition.quality.clamp(1, 5));
    let bins = adventuresim_core::durability::DamageBins(bins)
        .capped_to_quality(quality)
        .0;
    let mut condition = ctx
        .db
        .item_condition()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or_else(|| format!("Damaged demo item {inventory_item_id} has no condition row"))?;
    [
        condition.tier_1,
        condition.tier_2,
        condition.tier_3,
        condition.tier_4,
        condition.tier_5,
    ] = bins;
    ctx.db
        .item_condition()
        .inventory_item_id()
        .update(condition);
    Ok(())
}

#[reducer]
pub(crate) fn insert_new_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    temporary: bool,
) -> Result<(), String> {
    insert_character_with_origin(ctx, name, id, temporary, temporary)
}

pub(crate) fn insert_new_npc_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    temporary: bool,
) -> Result<(), String> {
    insert_character_with_origin(ctx, name, id, temporary, true)
}

fn insert_character_with_origin(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    temporary: bool,
    npc: bool,
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
        religion_hours: adventuresim_world_schema::ReligionHours {
            roman_catholic: 1000.0,
            ..Default::default()
        },
        stealth_hours: 1000.0,
        balance_hours: 1000.0,
        surgeon_hours: 1000.0,
        smithing_hours: 1000.0,
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
    crate::personality::initialize_personality(ctx, id, npc);

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
    require_living_character(ctx, character_id)?;
    let inventory = ctx
        .db
        .inventory_item()
        .character_and_id()
        .filter((character_id, inventory_item_id))
        .next()
        .ok_or_else(|| {
            format!(
            "Can't equip item: InventoryItem@{inventory_item_id} doesn't exist for Character@{character_id}"
            )
        })?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .ok_or_else(|| format!("Can't equip unknown item {}", inventory.item_id))?;
    if destination != ItemSlot::None && !item_slot_accepts(definition.slot, destination) {
        return Err(format!(
            "Can't equip {} in {:?}; its equipment slot is {:?}",
            inventory.item_id, destination, definition.slot
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
        ItemSlot::None => crate::repair::unequip(&mut equip, inventory_item_id),
    }

    ctx.db.character_equip().character_id().update(equip);
    Ok(())
}

fn item_slot_accepts(catalog_slot: ItemSlot, destination: ItemSlot) -> bool {
    match catalog_slot {
        ItemSlot::AnyHolding => matches!(
            destination,
            ItemSlot::AnyHolding | ItemSlot::LeftHolding | ItemSlot::RightHolding
        ),
        ItemSlot::AnyArm => matches!(
            destination,
            ItemSlot::AnyArm | ItemSlot::LeftArm | ItemSlot::RightArm
        ),
        ItemSlot::AnyLeg => matches!(
            destination,
            ItemSlot::AnyLeg | ItemSlot::LeftLeg | ItemSlot::RightLeg
        ),
        slot => slot == destination,
    }
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
