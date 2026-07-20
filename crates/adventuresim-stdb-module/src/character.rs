use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, reducer, table};
use std::hash::{DefaultHasher, Hash, Hasher};
use strum::VariantArray;

use crate::{
    ItemSlot, Settlement, add_inventory_item,
    capability::character_capability,
    condition::{
        character_condition, character_morale_source, character_needs,
        character_strategic_condition, morale_event, religious_demand,
    },
    disease::{
        character_illness_status, committed_cut, disease_notice, equipped_medication,
        herbalist_examination, infection_episode, medical_examination,
    },
    enter_mission, inventory_item,
    item::item,
    personality::character_personality,
    repair::{item_condition, repair_order},
    strategic::{inventory_quantity_target, party, party_member, settlement},
    surgery::{limb_injury, retained_projectile},
    tactical::tactical_server,
    time::{character_notoriety, character_time, character_training_schedule},
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
    // Keep an active tactical assignment until that server commits or removes
    // the character. This lets mission teardown find and cascade temporary
    // combatants that died in transient tactical state.
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

    if ctx.sender() != server || ctx.db.tactical_server().identity().find(server).is_none() {
        return Err("Only a registered tactical server can create its temporary characters".into());
    }

    let name = petname::Petnames::default()
        .generate(&mut ctx.rng(), 1, " ")
        .ok_or_else(|| format!("Can't generate a name for a temporary character"))?;
    let name = format!("bot-{name}");

    let mut id = ctx.random();
    id |= 0b1000_1000_1000_1000;

    insert_new_character(ctx, name, id, true)?;
    enter_mission(ctx, id, server)
}

/// Transactionally delete a temporary tactical character and every durable
/// strategic row that is owned by it. This must run before deleting Character
/// so no orphan can survive a successful reducer commit.
pub(crate) fn delete_temporary_character(
    ctx: &ReducerContext,
    character: Character,
) -> Result<(), String> {
    if !character.temporary {
        return Err("Refusing to cascade-delete a persistent character".into());
    }
    if let Some(party_id) = character.party_id.as_deref() {
        crate::strategic::delete_temporary_character_party(ctx, character.id, party_id)?;
    } else {
        for membership in ctx
            .db
            .party_member()
            .character_id()
            .filter(character.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_member().id().delete(membership.id);
        }
    }

    // Repair custody changes InventoryItem.character_id to zero while retaining
    // the real owner on RepairOrder. Remove those custody rows before scanning
    // ordinary owned inventory so a temporary character cannot leave orphaned
    // smith inventory behind.
    for order in ctx
        .db
        .repair_order()
        .owner_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        if ctx
            .db
            .item_condition()
            .inventory_item_id()
            .find(order.inventory_item_id)
            .is_some()
        {
            ctx.db
                .item_condition()
                .inventory_item_id()
                .delete(order.inventory_item_id);
        }
        ctx.db.inventory_item().id().delete(order.inventory_item_id);
        ctx.db.repair_order().id().delete(order.id);
    }

    let inventory = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>();
    for row in inventory {
        if ctx
            .db
            .item_condition()
            .inventory_item_id()
            .find(row.id)
            .is_some()
        {
            ctx.db.item_condition().inventory_item_id().delete(row.id);
        }
        if ctx
            .db
            .equipped_medication()
            .inventory_item_id()
            .find(row.id)
            .is_some()
        {
            ctx.db
                .equipped_medication()
                .inventory_item_id()
                .delete(row.id);
        }
        for repair in ctx
            .db
            .repair_order()
            .iter()
            .filter(|repair| repair.inventory_item_id == row.id)
            .collect::<Vec<_>>()
        {
            ctx.db.repair_order().id().delete(repair.id);
        }
        ctx.db.inventory_item().id().delete(row.id);
    }

    for row in ctx
        .db
        .infection_episode()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.infection_episode().id().delete(row.id);
    }
    for row in ctx
        .db
        .committed_cut()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.committed_cut().id().delete(row.id);
    }
    for row in ctx
        .db
        .disease_notice()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.disease_notice().id().delete(&row.id);
    }
    for row in ctx
        .db
        .medical_examination()
        .iter()
        .filter(|row| row.doctor_id == character.id || row.target_id == character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.medical_examination().id().delete(row.id);
    }
    for row in ctx
        .db
        .herbalist_examination()
        .patient_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.herbalist_examination().id().delete(row.id);
    }
    for row in ctx
        .db
        .morale_event()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.morale_event().id().delete(row.id);
    }
    for row in ctx
        .db
        .character_morale_source()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.character_morale_source().id().delete(&row.id);
    }
    for row in ctx
        .db
        .religious_demand()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.religious_demand().id().delete(row.id);
    }
    for row in ctx
        .db
        .inventory_quantity_target()
        .owner_character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.inventory_quantity_target().id().delete(&row.id);
    }

    ctx.db.character_stats().character_id().delete(character.id);
    ctx.db
        .character_skills()
        .character_id()
        .delete(character.id);
    ctx.db.character_time().character_id().delete(character.id);
    ctx.db
        .character_training_schedule()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_notoriety()
        .character_id()
        .delete(character.id);
    ctx.db.character_limbs().character_id().delete(character.id);
    ctx.db.character_equip().character_id().delete(character.id);
    ctx.db
        .character_attributes()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_personality()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_capability()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_condition()
        .character_id()
        .delete(character.id);
    ctx.db.character_needs().character_id().delete(character.id);
    ctx.db
        .character_strategic_condition()
        .character_id()
        .delete(character.id);
    if ctx
        .db
        .character_illness_status()
        .character_id()
        .find(character.id)
        .is_some()
    {
        ctx.db
            .character_illness_status()
            .character_id()
            .delete(character.id);
    }
    if ctx
        .db
        .character_death()
        .character_id()
        .find(character.id)
        .is_some()
    {
        ctx.db.character_death().character_id().delete(character.id);
    }
    ctx.db.character().delete(character);
    Ok(())
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
pub(crate) fn seed_damaged_character(ctx: &ReducerContext) -> Result<(), String> {
    const DAMAGED_CHARACTER_ID: u64 = 9_999_999_999_999_999;

    if ctx.db.character().id().find(DAMAGED_CHARACTER_ID).is_none() {
        insert_new_character(ctx, "Wounded Demo".to_string(), DAMAGED_CHARACTER_ID, false)?;
    }
    for (id, name, role) in [
        (9_000_001, "Mara", "Backup surgeon"),
        (9_000_002, "Orrin", "Critical patient"),
    ] {
        if ctx.db.character().id().find(id).is_none() {
            insert_new_npc_character(ctx, name.into(), id, false)?;
        }
        crate::strategic::attach_seeded_party_member(ctx, DAMAGED_CHARACTER_ID, id, role)?;
    }

    let mut limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(DAMAGED_CHARACTER_ID)
        .ok_or_else(|| "Damaged demo character is missing limb data".to_string())?;

    // Surgery's durable injury rows are authoritative over the projection.
    // Start clean so this reducer remains idempotent during UI iteration.
    limbs.left_arm_health = 1.0;
    limbs.right_arm_health = 1.0;
    limbs.left_leg_health = 1.0;
    limbs.right_leg_health = 1.0;
    limbs.head_health = 1.0;
    limbs.chest_health = 1.0;
    limbs.stomach_health = 1.0;
    ctx.db.character_limbs().character_id().update(limbs);

    for character_id in [DAMAGED_CHARACTER_ID, 9_000_001, 9_000_002] {
        for injury in ctx
            .db
            .limb_injury()
            .character_id()
            .filter(character_id)
            .collect::<Vec<_>>()
        {
            ctx.db.limb_injury().id().delete(injury.id);
        }
        for projectile in ctx
            .db
            .retained_projectile()
            .character_id()
            .filter(character_id)
            .collect::<Vec<_>>()
        {
            ctx.db.retained_projectile().id().delete(projectile.id);
        }
    }
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::LeftArm,
        0.22,
        0.05,
        Some(crate::surgery::ProjectileKind::Arrowhead),
    );
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::RightArm,
        0.18,
        0.0,
        None,
    );
    let mut bandaged = crate::surgery::injury_for(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::RightArm,
    );
    bandaged.bandaged = true;
    bandaged.stitched = true;
    bandaged.stitch_quality = 4.0;
    ctx.db.limb_injury().id().update(bandaged);
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::LeftLeg,
        0.0,
        0.42,
        None,
    );
    let mut splinted = crate::surgery::injury_for(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::LeftLeg,
    );
    splinted.splint_owner_id = Some(DAMAGED_CHARACTER_ID);
    ctx.db.limb_injury().id().update(splinted);
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::Chest,
        0.15,
        0.08,
        Some(crate::surgery::ProjectileKind::Ball),
    );

    crate::add_inventory_item(ctx, DAMAGED_CHARACTER_ID, "bandage", 8);
    crate::add_inventory_item(ctx, DAMAGED_CHARACTER_ID, "surgery_kit", 1);
    crate::add_inventory_item(ctx, DAMAGED_CHARACTER_ID, "splint", 3);

    // A wounded primary surgeon, a less-skilled backup, and a second critical
    // patient make clock alignment and parallel triage visible in one fixture.
    let fixture_now = ctx
        .db
        .character_time()
        .character_id()
        .find(DAMAGED_CHARACTER_ID)
        .ok_or("Surgery demo primary surgeon is missing time")?
        .minutes
        .max(200);
    for (id, surgeon_hours, lag) in [
        (DAMAGED_CHARACTER_ID, 20_000.0, 0),
        (9_000_001, 3_333.0, 100),
        (9_000_002, 500.0, 200),
    ] {
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(id)
            .ok_or("Surgery demo character is missing skills")?;
        skills.surgeon_hours = surgeon_hours;
        ctx.db.character_skills().character_id().update(skills);
        let mut time = ctx
            .db
            .character_time()
            .character_id()
            .find(id)
            .ok_or("Surgery demo character is missing time")?;
        time.minutes = fixture_now.saturating_sub(lag);
        ctx.db.character_time().character_id().update(time);
        crate::capability::refresh_character_capability(ctx, id)?;
    }
    crate::add_inventory_item(ctx, 9_000_001, "bandage", 6);
    crate::add_inventory_item(ctx, 9_000_001, "surgery_kit", 1);
    crate::add_inventory_item(ctx, 9_000_001, "splint", 2);
    crate::surgery::commit_hit_injury(
        ctx,
        9_000_002,
        crate::surgery::LimbRegion::RightLeg,
        0.36,
        0.08,
        Some(crate::surgery::ProjectileKind::Arrowhead),
    );
    crate::surgery::commit_hit_injury(
        ctx,
        9_000_002,
        crate::surgery::LimbRegion::LeftArm,
        0.04,
        0.50,
        None,
    );

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

/// Seed a character with direct training in every religion for local UI development.
pub(crate) fn seed_religion_scholar_character(ctx: &ReducerContext) -> Result<(), String> {
    const RELIGION_SCHOLAR_CHARACTER_ID: u64 = 9_999_999_999_999_988;

    if ctx
        .db
        .character()
        .id()
        .find(RELIGION_SCHOLAR_CHARACTER_ID)
        .is_none()
    {
        insert_new_character(
            ctx,
            "Religion Scholar Demo".to_string(),
            RELIGION_SCHOLAR_CHARACTER_ID,
            false,
        )?;
    }

    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(RELIGION_SCHOLAR_CHARACTER_ID)
        .ok_or_else(|| "Religion scholar demo is missing skill data".to_string())?;
    skills.religion_hours = adventuresim_world_schema::ReligionHours {
        roman_catholic: 100.0,
        lutheran: 200.0,
        reformed: 300.0,
        anglican: 400.0,
        eastern_orthodox: 500.0,
        islamic: 600.0,
        judaism: 700.0,
    };
    ctx.db.character_skills().character_id().update(skills);

    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(RELIGION_SCHOLAR_CHARACTER_ID)
        .ok_or_else(|| "Religion scholar demo is missing condition data".to_string())?;
    condition.religion_id = Some(
        adventuresim_world_schema::OfficialReligion::RomanCatholic
            .religion_id()
            .to_string(),
    );
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);

    crate::capability::refresh_character_capability(ctx, RELIGION_SCHOLAR_CHARACTER_ID)?;
    crate::condition::refresh_character_strategic_condition(ctx, RELIGION_SCHOLAR_CHARACTER_ID)?;
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
        // Legacy scalar retained only for schema compatibility. Currency is
        // authoritative in inventory.
        gold: 0,
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
    crate::item::credit_personal_currency(ctx, character.id, &start_settlement.id, 100)?;
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

    // One inventory instance may occupy at most one slot. Clear any previous
    // occurrence before selecting the new destination.
    crate::repair::unequip(&mut equip, inventory_item_id);

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
    crate::capability::refresh_character_capability(ctx, character_id)?;
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
