//! Strategic Layer SpacetimeDB Module
//!
//! Mount & Blade style architecture with full auth and strategic features:
//! - Auth: User/Account/OidcIdentity with WorkOS integration
//! - Characters: Full stats, skills, body parts, attributes
//! - Strategic: Settlements, quests, parties
//! - Tactical integration: Mission lifecycle with tactical-server
//!
//! Flow:
//! 1. Client connects → User record created (guest or linked to account)
//! 2. Player creates/selects character → joins settlement
//! 3. Player forms party → accepts quest → enters mission
//! 4. Spawner starts tactical-server → players connect via WebTransport
//! 5. Mission ends → rewards applied → party returns to settlement

use spacetimedb::{reducer, table, Identity, ReducerContext, SpacetimeType, Table, Timestamp};

// ============================================================================
// Enums
// ============================================================================

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TacticalStatus {
    Pending,
    Ready,
    Ended,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Race {
    // Mortal races
    Human,
    Orc,
    Goblin,
    Dwarf,
    Halfling,
    Ratling,
    // Immortal races (fey-blooded)
    Elf,
    Dragon,
    Beastman,
    HalfBlood,
    VileBlood,
    Undead,
}

impl Race {
    pub fn is_immortal(&self) -> bool {
        matches!(
            self,
            Race::Elf | Race::Dragon | Race::Beastman | Race::HalfBlood | Race::VileBlood | Race::Undead
        )
    }
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillCategory {
    // Mental
    Will,
    Charisma,
    Medicine,
    Faith,
    // Physical
    Melee,
    Ranged,
    Block,
    Stealth,
    Balance,
    Surgeon,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyPartType {
    Head,
    Torso,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeType {
    Strength,
    Agility,
    Endurance,
    Immunity,
    Gut,
    Intelligence,
    Instinct,
    Eyesight,
    Hearing,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestStatus {
    Available,
    Accepted,
    InProgress,
    Completed,
    Failed,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartyStatus {
    Forming,
    Traveling,
    InMission,
    Disbanded,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyType {
    Orc,
    Goblin,
    Bandit,
    Wolf,
    Skeleton,
}

// ============================================================================
// Auth Tables
// ============================================================================

/// User - per-connection session state
#[derive(Clone, Debug)]
#[table(name = user, public)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    /// Optional link to persistent account
    pub account_id: Option<u64>,
    /// Display name
    pub display_name: String,
    /// Currently connected
    pub online: bool,
    pub created_at: Timestamp,
    pub last_seen: Timestamp,
}

/// Account - persistent user data (optional login)
#[derive(Clone, Debug)]
#[table(name = account, public)]
pub struct Account {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub username: Option<String>,
    pub username_lower: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// OidcIdentity - WorkOS identity link
#[derive(Clone, Debug)]
#[table(name = oidc_identity, public)]
pub struct OidcIdentity {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub workos_user_id: String,
    #[index(btree)]
    pub account_id: u64,
    pub email: String,
    pub linked_at: Timestamp,
}

// ============================================================================
// Character Tables
// ============================================================================

/// Character - strategic state (HP/damage lives in tactical memory)
#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner_identity: Identity,
    pub name: String,
    pub race: Race,
    pub is_immortal: bool,
    pub age_days: u32,
    pub favor_invested: u64,
    pub xp: i64,
    pub level: i32,
    pub current_settlement_id: Option<u64>,
    pub party_id: Option<u64>,
    pub gold: i64,
    pub created_at: Timestamp,
}

/// Character skills
#[derive(Clone, Debug)]
#[table(name = character_skill, public)]
pub struct CharacterSkill {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub skill: SkillCategory,
    pub hours_trained: u32,
    pub training_allocation: f32,
    pub last_used: Timestamp,
}

/// Character body parts
#[derive(Clone, Debug)]
#[table(name = character_body_part, public)]
pub struct CharacterBodyPart {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub part_type: BodyPartType,
    pub health: f32,
    pub bandaged_damage: f32,
    pub scarred_damage: f32,
}

/// Character attributes
#[derive(Clone, Debug)]
#[table(name = character_attribute, public)]
pub struct CharacterAttribute {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub attribute_type: AttributeType,
    pub limb: Option<BodyPartType>,
    pub genetic_max: f32,
    pub current_value: f32,
    pub conditioning_hours: u32,
}

// ============================================================================
// Strategic Tables
// ============================================================================

/// Settlement
#[derive(Clone, Debug)]
#[table(name = settlement, public)]
pub struct Settlement {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: u8,
    pub scene_key: String,
}

/// Quest
#[derive(Clone, Debug)]
#[table(name = quest, public)]
pub struct Quest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub settlement_id: u64,
    pub title: String,
    pub difficulty: u8,
    pub reward_gold: i64,
    pub reward_xp: i64,
    pub enemy_type: EnemyType,
    pub enemy_count: u32,
    pub distance: f32,
    pub status: QuestStatus,
    pub scene_key: String,
    pub created_at: Timestamp,
}

/// Party
#[derive(Clone, Debug)]
#[table(name = party, public)]
pub struct Party {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub leader_id: u64,
    pub quest_id: Option<u64>,
    pub status: PartyStatus,
    pub settlement_id: Option<u64>,
    pub created_at: Timestamp,
}

/// Party membership
#[derive(Clone, Debug)]
#[table(name = party_member, public)]
pub struct PartyMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub joined_at: Timestamp,
}

// ============================================================================
// Inventory & Tactical Tables
// ============================================================================

/// Inventory item
#[derive(Clone, Debug)]
#[table(name = inventory_item, public)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub item_type: String,
    pub quantity: i32,
}

/// Active tactical server instance
#[derive(Clone, Debug)]
#[table(name = tactical_server, public)]
pub struct TacticalServer {
    #[primary_key]
    pub mission_id: String,
    pub scene_key: String,
    pub status: TacticalStatus,
    pub addr: String,
    pub cert_digest: String,
    pub party_id: u64,
    pub quest_id: Option<u64>,
    pub created_at: Timestamp,
}

// ============================================================================
// Lifecycle Reducers
// ============================================================================

#[reducer(client_connected)]
pub fn on_client_connected(ctx: &ReducerContext) {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    if let Some(mut user) = ctx.db.user().identity().find(&identity) {
        user.online = true;
        user.last_seen = now;
        ctx.db.user().identity().update(user);
        log::info!("User reconnected: {:?}", identity);
    } else {
        let display_name = format!("Adventurer_{}", &identity.to_hex()[..8]);
        ctx.db.user().insert(User {
            identity,
            account_id: None,
            display_name,
            online: true,
            created_at: now,
            last_seen: now,
        });
        log::info!("New user connected: {:?}", identity);
    }
}

#[reducer(client_disconnected)]
pub fn on_client_disconnected(ctx: &ReducerContext) {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    if let Some(mut user) = ctx.db.user().identity().find(&identity) {
        user.online = false;
        user.last_seen = now;
        ctx.db.user().identity().update(user);
        log::info!("User disconnected: {:?}", identity);
    }
}

// ============================================================================
// Auth Reducers
// ============================================================================

/// Link a WorkOS identity to the current user's account
#[reducer]
pub fn link_account(ctx: &ReducerContext, workos_user_id: String, email: String) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    let mut user = ctx.db.user().identity().find(&identity)
        .ok_or("User not found - connect first")?;

    // Check if this OIDC identity is already linked
    if let Some(existing_oidc) = ctx.db.oidc_identity().workos_user_id().find(&workos_user_id) {
        // Already linked - just update user's account_id
        user.account_id = Some(existing_oidc.account_id);
        user.display_name = email.split('@').next().unwrap_or("User").to_string();
        ctx.db.user().identity().update(user);
        log::info!("User re-linked to existing account {}", existing_oidc.account_id);
        return Ok(());
    }

    // Create new account if user doesn't have one
    let account_id = match user.account_id {
        Some(id) => id,
        None => {
            let account = ctx.db.account().insert(Account {
                id: 0,
                username: None,
                username_lower: None,
                created_at: now,
                updated_at: now,
            });
            account.id
        }
    };

    // Create OIDC identity link
    ctx.db.oidc_identity().insert(OidcIdentity {
        id: 0,
        workos_user_id,
        account_id,
        email: email.clone(),
        linked_at: now,
    });

    // Update user
    user.account_id = Some(account_id);
    user.display_name = email.split('@').next().unwrap_or("User").to_string();
    ctx.db.user().identity().update(user);

    log::info!("Account {} linked", account_id);
    Ok(())
}

/// Unlink from account (revert to guest)
#[reducer]
pub fn unlink_account(ctx: &ReducerContext) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    let mut user = ctx.db.user().identity().find(&identity)
        .ok_or("User not found")?;

    user.account_id = None;
    user.display_name = format!("Adventurer_{}", &identity.to_hex()[..8]);
    user.last_seen = now;
    ctx.db.user().identity().update(user);

    log::info!("User unlinked from account");
    Ok(())
}

/// Set username for current account
#[reducer]
pub fn set_username(ctx: &ReducerContext, username: String) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    if username.len() < 3 || username.len() > 32 {
        return Err("Username must be 3-32 characters".into());
    }

    if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Username can only contain letters, numbers, and underscores".into());
    }

    let user = ctx.db.user().identity().find(&identity)
        .ok_or("User not found")?;

    let account_id = user.account_id
        .ok_or("Link an account first")?;

    let username_lower = username.to_lowercase();

    // Check uniqueness by iterating through accounts
    for existing in ctx.db.account().iter() {
        if existing.id != account_id {
            if let Some(ref existing_lower) = existing.username_lower {
                if existing_lower == &username_lower {
                    return Err("Username already taken".into());
                }
            }
        }
    }

    let mut account = ctx.db.account().id().find(&account_id)
        .ok_or("Account not found")?;

    account.username = Some(username);
    account.username_lower = Some(username_lower);
    account.updated_at = now;
    ctx.db.account().id().update(account);

    Ok(())
}

// ============================================================================
// Character Reducers
// ============================================================================

/// Create a new character
#[reducer]
pub fn create_character(ctx: &ReducerContext, name: String, race: Race) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    if name.len() < 2 || name.len() > 32 {
        return Err("Name must be 2-32 characters".into());
    }

    if ctx.db.user().identity().find(&identity).is_none() {
        return Err("User not found - connect first".into());
    }

    // Check character limit (max 5)
    let existing_count = ctx.db.character()
        .owner_identity()
        .filter(&identity)
        .count();

    if existing_count >= 5 {
        return Err("Maximum 5 characters per account".into());
    }

    let is_immortal = race.is_immortal();

    // Get first settlement
    let first_settlement = ctx.db.settlement().iter().next();
    let settlement_id = first_settlement.map(|s| s.id);

    let character = ctx.db.character().insert(Character {
        id: 0,
        owner_identity: identity,
        name: name.clone(),
        race,
        is_immortal,
        age_days: if is_immortal { 0 } else { 6570 }, // ~18 years for mortals
        favor_invested: 0,
        xp: 0,
        level: 1,
        current_settlement_id: settlement_id,
        party_id: None,
        gold: 100,
        created_at: now,
    });

    let character_id = character.id;

    // Initialize body parts
    for part_type in [
        BodyPartType::Head,
        BodyPartType::Torso,
        BodyPartType::LeftArm,
        BodyPartType::RightArm,
        BodyPartType::LeftLeg,
        BodyPartType::RightLeg,
    ] {
        ctx.db.character_body_part().insert(CharacterBodyPart {
            id: 0,
            character_id,
            part_type,
            health: 1.0,
            bandaged_damage: 0.0,
            scarred_damage: 0.0,
        });
    }

    // Initialize skills
    for skill in [
        SkillCategory::Will,
        SkillCategory::Charisma,
        SkillCategory::Medicine,
        SkillCategory::Faith,
        SkillCategory::Melee,
        SkillCategory::Ranged,
        SkillCategory::Block,
        SkillCategory::Stealth,
        SkillCategory::Balance,
        SkillCategory::Surgeon,
    ] {
        let allocation = if skill == SkillCategory::Melee { 0.2 } else { 0.08 };
        ctx.db.character_skill().insert(CharacterSkill {
            id: 0,
            character_id,
            skill,
            hours_trained: 0,
            training_allocation: allocation,
            last_used: now,
        });
    }

    // Initialize core attributes
    for attr in [
        AttributeType::Endurance,
        AttributeType::Immunity,
        AttributeType::Gut,
        AttributeType::Intelligence,
        AttributeType::Instinct,
        AttributeType::Eyesight,
        AttributeType::Hearing,
    ] {
        ctx.db.character_attribute().insert(CharacterAttribute {
            id: 0,
            character_id,
            attribute_type: attr,
            limb: None,
            genetic_max: 3.0,
            current_value: 2.5,
            conditioning_hours: 0,
        });
    }

    // Initialize limb attributes
    for limb in [
        BodyPartType::LeftArm,
        BodyPartType::RightArm,
        BodyPartType::LeftLeg,
        BodyPartType::RightLeg,
    ] {
        for attr in [AttributeType::Strength, AttributeType::Agility] {
            ctx.db.character_attribute().insert(CharacterAttribute {
                id: 0,
                character_id,
                attribute_type: attr,
                limb: Some(limb),
                genetic_max: 3.0,
                current_value: 2.5,
                conditioning_hours: 0,
            });
        }
    }

    // Starter items
    add_item(ctx, character_id, "torch", 1);
    add_item(ctx, character_id, "bandage", 3);
    add_item(ctx, character_id, "rations", 7);
    add_item(ctx, character_id, "sword_iron", 1);

    log::info!("Character {} '{}' created", character_id, name);
    Ok(())
}

// ============================================================================
// Party Reducers
// ============================================================================

/// Create a new party
#[reducer]
pub fn create_party(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    let character = ctx.db.character().id().find(&character_id)
        .ok_or("Character not found")?;

    if character.owner_identity != identity {
        return Err("Not your character".into());
    }

    if character.party_id.is_some() {
        return Err("Character already in a party".into());
    }

    let settlement_id = character.current_settlement_id
        .ok_or("Character must be in a settlement")?;

    let party = ctx.db.party().insert(Party {
        id: 0,
        leader_id: character_id,
        quest_id: None,
        status: PartyStatus::Forming,
        settlement_id: Some(settlement_id),
        created_at: now,
    });

    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: party.id,
        character_id,
        joined_at: now,
    });

    // Update character
    let mut character = character;
    character.party_id = Some(party.id);
    ctx.db.character().id().update(character);

    log::info!("Party {} created", party.id);
    Ok(())
}

/// Join an existing party
#[reducer]
pub fn join_party(ctx: &ReducerContext, character_id: u64, party_id: u64) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    let mut character = ctx.db.character().id().find(&character_id)
        .ok_or("Character not found")?;

    if character.owner_identity != identity {
        return Err("Not your character".into());
    }

    if character.party_id.is_some() {
        return Err("Character already in a party".into());
    }

    let party = ctx.db.party().id().find(&party_id)
        .ok_or("Party not found")?;

    if party.status != PartyStatus::Forming {
        return Err("Party is not accepting members".into());
    }

    if character.current_settlement_id != party.settlement_id {
        return Err("Must be in same settlement".into());
    }

    let member_count = ctx.db.party_member().party_id().filter(&party_id).count();
    if member_count >= 6 {
        return Err("Party is full".into());
    }

    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id,
        character_id,
        joined_at: now,
    });

    character.party_id = Some(party_id);
    ctx.db.character().id().update(character);

    log::info!("Character {} joined party {}", character_id, party_id);
    Ok(())
}

/// Leave current party
#[reducer]
pub fn leave_party(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let identity = ctx.sender;

    let mut character = ctx.db.character().id().find(&character_id)
        .ok_or("Character not found")?;

    if character.owner_identity != identity {
        return Err("Not your character".into());
    }

    let party_id = character.party_id.ok_or("Character not in a party")?;

    let party = ctx.db.party().id().find(&party_id)
        .ok_or("Party not found")?;

    if party.status == PartyStatus::InMission {
        return Err("Cannot leave during mission".into());
    }

    // Remove membership
    if let Some(membership) = ctx.db.party_member()
        .party_id()
        .filter(&party_id)
        .find(|m| m.character_id == character_id)
    {
        ctx.db.party_member().id().delete(&membership.id);
    }

    character.party_id = None;
    ctx.db.character().id().update(character);

    // Handle leader leaving
    if party.leader_id == character_id {
        let remaining: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();

        if remaining.is_empty() {
            let mut party = party;
            party.status = PartyStatus::Disbanded;
            ctx.db.party().id().update(party);
        } else {
            let mut party = party;
            party.leader_id = remaining[0].character_id;
            ctx.db.party().id().update(party);
        }
    }

    log::info!("Character {} left party {}", character_id, party_id);
    Ok(())
}

// ============================================================================
// Quest Reducers
// ============================================================================

/// Accept a quest for a party
#[reducer]
pub fn accept_quest(ctx: &ReducerContext, party_id: u64, quest_id: u64) -> Result<(), String> {
    let identity = ctx.sender;

    let mut party = ctx.db.party().id().find(&party_id)
        .ok_or("Party not found")?;

    let leader = ctx.db.character().id().find(&party.leader_id)
        .ok_or("Party leader not found")?;

    if leader.owner_identity != identity {
        return Err("Only party leader can accept quests".into());
    }

    if party.status != PartyStatus::Forming {
        return Err("Party must be forming".into());
    }

    let mut quest = ctx.db.quest().id().find(&quest_id)
        .ok_or("Quest not found")?;

    if quest.status != QuestStatus::Available {
        return Err("Quest not available".into());
    }

    if quest.settlement_id != party.settlement_id.unwrap_or(0) {
        return Err("Quest is from different settlement".into());
    }

    quest.status = QuestStatus::Accepted;
    ctx.db.quest().id().update(quest);

    party.quest_id = Some(quest_id);
    ctx.db.party().id().update(party);

    log::info!("Party {} accepted quest {}", party_id, quest_id);
    Ok(())
}

// ============================================================================
// Mission Reducers
// ============================================================================

/// Party leader initiates tactical mission
#[reducer]
pub fn enter_mission(ctx: &ReducerContext, party_id: u64) -> Result<(), String> {
    let identity = ctx.sender;
    let now = ctx.timestamp;

    let mut party = ctx.db.party().id().find(&party_id)
        .ok_or("Party not found")?;

    let leader = ctx.db.character().id().find(&party.leader_id)
        .ok_or("Party leader not found")?;

    if leader.owner_identity != identity {
        return Err("Only party leader can start mission".into());
    }

    if party.status != PartyStatus::Forming {
        return Err("Party must be forming".into());
    }

    let quest_id = party.quest_id.ok_or("Accept a quest first")?;

    let quest = ctx.db.quest().id().find(&quest_id)
        .ok_or("Quest not found")?;

    let mission_id = format!("mission-{}-{}", party_id, now.to_micros_since_unix_epoch());

    ctx.db.tactical_server().insert(TacticalServer {
        mission_id: mission_id.clone(),
        scene_key: quest.scene_key.clone(),
        status: TacticalStatus::Pending,
        addr: String::new(),
        cert_digest: String::new(),
        party_id,
        quest_id: Some(quest_id),
        created_at: now,
    });

    party.status = PartyStatus::InMission;
    party.settlement_id = None;
    ctx.db.party().id().update(party);

    // Update quest status
    let mut quest = quest;
    quest.status = QuestStatus::InProgress;
    ctx.db.quest().id().update(quest);

    // Update party member characters
    for member in ctx.db.party_member().party_id().filter(&party_id) {
        if let Some(mut character) = ctx.db.character().id().find(&member.character_id) {
            character.current_settlement_id = None;
            ctx.db.character().id().update(character);
        }
    }

    log::info!("Mission {} started", mission_id);
    Ok(())
}

/// Called by tactical-server when ready
#[reducer]
pub fn tactical_server_ready(
    ctx: &ReducerContext,
    mission_id: String,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    let mut server = ctx.db.tactical_server().mission_id().find(&mission_id)
        .ok_or("Mission not found")?;

    if server.status != TacticalStatus::Pending {
        return Err("Mission not pending".into());
    }

    server.status = TacticalStatus::Ready;
    server.addr = addr.clone();
    server.cert_digest = cert_digest;
    ctx.db.tactical_server().mission_id().update(server);

    log::info!("Mission {} ready on {}", mission_id, addr);
    Ok(())
}

/// Called by tactical-server when mission ends
#[reducer]
pub fn commit_mission(
    ctx: &ReducerContext,
    mission_id: String,
    success: bool,
    xp_gained: i64,
    gold_gained: i64,
) -> Result<(), String> {
    let mut server = ctx.db.tactical_server().mission_id().find(&mission_id)
        .ok_or("Mission not found")?;

    if server.status == TacticalStatus::Ended {
        return Ok(()); // Idempotent
    }

    let party = ctx.db.party().id().find(&server.party_id)
        .ok_or("Party not found")?;

    // Get origin settlement from quest
    let settlement_id = server.quest_id
        .and_then(|qid| ctx.db.quest().id().find(&qid))
        .map(|q| q.settlement_id);

    // Apply rewards to party members
    let members: Vec<_> = ctx.db.party_member().party_id().filter(&server.party_id).collect();
    let member_count = members.len() as i64;
    let xp_per = xp_gained / member_count.max(1);
    let gold_per = gold_gained / member_count.max(1);

    for member in &members {
        if let Some(mut character) = ctx.db.character().id().find(&member.character_id) {
            if success {
                character.xp += xp_per;
                character.level = 1 + (character.xp / 100) as i32;
                character.gold += gold_per;
            }
            character.current_settlement_id = settlement_id;
            ctx.db.character().id().update(character);
        }
    }

    // Update quest status
    if let Some(quest_id) = server.quest_id {
        if let Some(mut quest) = ctx.db.quest().id().find(&quest_id) {
            quest.status = if success { QuestStatus::Completed } else { QuestStatus::Available };
            ctx.db.quest().id().update(quest);
        }
    }

    // Update party
    let mut party = party;
    party.status = PartyStatus::Forming;
    party.settlement_id = settlement_id;
    party.quest_id = None;
    ctx.db.party().id().update(party);

    // Mark mission ended
    server.status = TacticalStatus::Ended;
    ctx.db.tactical_server().mission_id().update(server);

    log::info!("Mission {} ended: success={}, xp={}, gold={}", mission_id, success, xp_gained, gold_gained);
    Ok(())
}

/// Leave/cancel mission
#[reducer]
pub fn leave_mission(ctx: &ReducerContext, mission_id: String) -> Result<(), String> {
    commit_mission(ctx, mission_id, false, 0, 0)
}

// ============================================================================
// Init Reducer
// ============================================================================

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let now = ctx.timestamp;

    // Create starter settlements
    let settlement1 = ctx.db.settlement().insert(Settlement {
        id: 0,
        name: "Millbrook".into(),
        coord_x: 0.0,
        coord_y: 0.0,
        population_level: 3,
        scene_key: "town_a".into(),
    });

    let settlement2 = ctx.db.settlement().insert(Settlement {
        id: 0,
        name: "Ironford".into(),
        coord_x: 50.0,
        coord_y: 30.0,
        population_level: 3,
        scene_key: "town_b".into(),
    });

    // Generate quests for each settlement
    for settlement in [&settlement1, &settlement2] {
        for difficulty in 1..=3u8 {
            let (enemy_type, enemy_count, reward_gold, reward_xp) = match difficulty {
                1 => (EnemyType::Goblin, 5, 50, 30),
                2 => (EnemyType::Orc, 4, 100, 60),
                _ => (EnemyType::Bandit, 6, 150, 90),
            };

            ctx.db.quest().insert(Quest {
                id: 0,
                settlement_id: settlement.id,
                title: format!("Hunt {} {:?}s", enemy_count, enemy_type),
                difficulty,
                reward_gold,
                reward_xp,
                enemy_type,
                enemy_count,
                distance: (difficulty as f32) * 5.0,
                status: QuestStatus::Available,
                scene_key: settlement.scene_key.clone(),
                created_at: now,
            });
        }
    }

    log::info!("Module initialized with 2 settlements and 6 quests");
}

// ============================================================================
// Helpers
// ============================================================================

fn add_item(ctx: &ReducerContext, character_id: u64, item_type: &str, quantity: i32) {
    let existing = ctx.db.inventory_item()
        .character_id()
        .filter(&character_id)
        .find(|i| i.item_type == item_type);

    match existing {
        Some(mut item) => {
            item.quantity += quantity;
            ctx.db.inventory_item().id().update(item);
        }
        None => {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id,
                item_type: item_type.into(),
                quantity,
            });
        }
    }
}
