//! Strategic Layer SpacetimeDB Module
//!
//! This module handles ONLY strategic (meta-game) state:
//! - Character progression (XP, level)
//! - Persistent inventory
//! - Party management
//! - Mission tracking
//! - Quest progress
//!
//! Tactical gameplay state (HP, damage, positions, enemies, loot drops) lives
//! entirely in the tactical-server game state and is NOT stored in the database.
//! Only the RESULTS of missions (XP gained, items earned) are committed to the DB.

use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

// ============================================================================
// Constants
// ============================================================================

/// Scene allowlist - validated in start_mission reducer
const VALID_SCENES: &[&str] = &["town_a", "town_b"];

/// Port range for tactical servers
const TACTICAL_PORT_START: u16 = 6000;
const TACTICAL_PORT_END: u16 = 6100;

// ============================================================================
// Tables
// ============================================================================

/// A player identity with associated character
#[derive(Clone, Debug)]
#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub character_id: String,
    pub display_name: String,
    pub online: bool,
}

/// Character progression (strategic state only)
#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    pub id: String,
    pub owner: Identity,
    pub name: String,
    pub xp: i32,
    pub level: i32,
    pub updated_at_micros: i64,
}

/// Persistent inventory (items that survive between missions)
#[derive(Clone, Debug)]
#[table(name = inventory_item, public)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: String,
    pub item_id: String,
    pub qty: i32,
}

/// Party grouping for missions
#[derive(Clone, Debug)]
#[table(name = party, public)]
pub struct Party {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub leader_id: String,
    pub created_at_micros: i64,
}

/// Party membership
#[derive(Clone, Debug)]
#[table(name = party_member, public)]
pub struct PartyMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: String,
    pub joined_at_micros: i64,
}

/// Active or completed missions
#[derive(Clone, Debug)]
#[table(name = mission, public)]
pub struct Mission {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    pub scene_key: String,
    /// "active", "completed", "failed", "cancelled"
    pub status: String,
    pub server_port: u16,
    pub join_token: String,
    pub created_at_micros: i64,
    pub completed_at_micros: i64,
}

/// Idempotent mission commit tracking
#[derive(Clone, Debug)]
#[table(name = mission_commit, public)]
pub struct MissionCommit {
    #[primary_key]
    pub mission_id: String,
    pub committed_at_micros: i64,
    pub success: bool,
    pub xp_gained: i32,
    /// JSON array of {item_id, qty}
    pub items_gained_json: String,
}

/// Quest definitions
#[derive(Clone, Debug)]
#[table(name = quest_def, public)]
pub struct QuestDef {
    #[primary_key]
    pub quest_id: String,
    pub title: String,
    pub description: String,
    pub reward_xp: i32,
    pub reward_items_json: String,
}

/// Per-character quest progress
#[derive(Clone, Debug)]
#[table(name = character_quest, public)]
pub struct CharacterQuest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: String,
    pub quest_id: String,
    /// "available", "active", "completed"
    pub status: String,
    pub updated_at_micros: i64,
}

/// Port allocation tracker (singleton)
#[derive(Clone, Debug)]
#[table(name = port_allocation, public)]
pub struct PortAllocation {
    #[primary_key]
    pub id: u64, // Always 1
    pub next_port: u16,
}

// ============================================================================
// Lifecycle Reducers
// ============================================================================

/// Initialize module state
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    log::info!("Initializing strategic module...");

    // Initialize port allocation tracker
    ctx.db.port_allocation().insert(PortAllocation {
        id: 1,
        next_port: TACTICAL_PORT_START,
    });

    // Seed some quest definitions
    seed_quests(ctx);

    log::info!("Strategic module initialized");
}

/// Handle client connection
#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    let identity = ctx.sender;
    log::info!("Client connected: {:?}", identity);

    // Mark player as online if they exist
    if let Some(mut player) = ctx.db.player().identity().find(&identity) {
        player.online = true;
        ctx.db.player().identity().update(player);
    }
}

/// Handle client disconnection
#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    let identity = ctx.sender;
    log::info!("Client disconnected: {:?}", identity);

    // Mark player as offline
    if let Some(mut player) = ctx.db.player().identity().find(&identity) {
        player.online = false;
        ctx.db.player().identity().update(player);
    }
}

// ============================================================================
// Character Reducers
// ============================================================================

/// Create or update a character
#[reducer]
pub fn upsert_character(
    ctx: &ReducerContext,
    character_id: String,
    name: String,
) -> Result<(), String> {
    let identity = ctx.sender;
    let now = now_micros(ctx);

    // Upsert player record
    match ctx.db.player().identity().find(&identity) {
        Some(mut player) => {
            player.character_id = character_id.clone();
            player.display_name = name.clone();
            ctx.db.player().identity().update(player);
        }
        None => {
            ctx.db.player().insert(Player {
                identity,
                character_id: character_id.clone(),
                display_name: name.clone(),
                online: true,
            });
        }
    }

    // Upsert character record
    match ctx.db.character().id().find(&character_id) {
        Some(mut character) => {
            character.name = name;
            character.updated_at_micros = now;
            ctx.db.character().id().update(character);
        }
        None => {
            ctx.db.character().insert(Character {
                id: character_id.clone(),
                owner: identity,
                name,
                xp: 0,
                level: 1,
                updated_at_micros: now,
            });

            // Give starter items
            add_item(ctx, &character_id, "bandage", 3)?;
            add_item(ctx, &character_id, "torch", 1)?;
        }
    }

    Ok(())
}

// ============================================================================
// Inventory Reducers
// ============================================================================

/// Add an item to a character's inventory
#[reducer]
pub fn add_item_to_inventory(
    ctx: &ReducerContext,
    character_id: String,
    item_id: String,
    qty: i32,
) -> Result<(), String> {
    if qty <= 0 {
        return Err("Quantity must be positive".to_string());
    }

    add_item(ctx, &character_id, &item_id, qty)
}

// ============================================================================
// Party Reducers
// ============================================================================

/// Create a new party
#[reducer]
pub fn create_party(
    ctx: &ReducerContext,
    party_id: String,
    party_name: String,
    leader_character_id: String,
) -> Result<(), String> {
    let now = now_micros(ctx);

    // Verify leader character exists
    if ctx.db.character().id().find(&leader_character_id).is_none() {
        return Err("Leader character not found".to_string());
    }

    // Check party doesn't already exist
    if ctx.db.party().id().find(&party_id).is_some() {
        return Err("Party ID already exists".to_string());
    }

    // Create party
    ctx.db.party().insert(Party {
        id: party_id.clone(),
        name: party_name,
        leader_id: leader_character_id.clone(),
        created_at_micros: now,
    });

    // Add leader as member
    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id,
        character_id: leader_character_id,
        joined_at_micros: now,
    });

    Ok(())
}

/// Join an existing party
#[reducer]
pub fn join_party(
    ctx: &ReducerContext,
    party_id: String,
    character_id: String,
) -> Result<(), String> {
    let now = now_micros(ctx);

    // Verify party exists
    if ctx.db.party().id().find(&party_id).is_none() {
        return Err("Party not found".to_string());
    }

    // Verify character exists
    if ctx.db.character().id().find(&character_id).is_none() {
        return Err("Character not found".to_string());
    }

    // Check if already a member
    let already_member = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .any(|m| m.character_id == character_id);

    if already_member {
        return Ok(()); // Idempotent
    }

    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id,
        character_id,
        joined_at_micros: now,
    });

    Ok(())
}

/// Leave a party
#[reducer]
pub fn leave_party(
    ctx: &ReducerContext,
    party_id: String,
    character_id: String,
) -> Result<(), String> {
    let membership: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .filter(|m| m.character_id == character_id)
        .collect();

    for m in membership {
        ctx.db.party_member().id().delete(m.id);
    }

    Ok(())
}

// ============================================================================
// Mission Reducers
// ============================================================================

/// Start a tactical mission
///
/// This reducer is called by the strategic UI to begin a mission.
/// It allocates a port, generates a join token, and records the mission.
/// The UI is responsible for spawning the tactical server process.
/// The allocated port can be retrieved by querying the mission table.
#[reducer]
pub fn start_mission(
    ctx: &ReducerContext,
    mission_id: String,
    party_id: String,
    scene_key: String,
    join_token: String,
) -> Result<(), String> {
    let now = now_micros(ctx);

    // Validate scene_key against allowlist
    if !VALID_SCENES.contains(&scene_key.as_str()) {
        return Err(format!("Invalid scene_key: {}", scene_key));
    }

    // Verify party exists
    if ctx.db.party().id().find(&party_id).is_none() {
        return Err("Party not found".to_string());
    }

    // Check mission doesn't already exist
    if ctx.db.mission().id().find(&mission_id).is_some() {
        return Err("Mission ID already exists".to_string());
    }

    // Allocate port
    let port = allocate_port(ctx)?;

    // Create mission record
    ctx.db.mission().insert(Mission {
        id: mission_id.clone(),
        party_id,
        scene_key,
        status: "active".to_string(),
        server_port: port,
        join_token,
        created_at_micros: now,
        completed_at_micros: 0,
    });

    log::info!("Mission {} started on port {}", mission_id, port);

    Ok(())
}

/// Commit mission results (called by tactical server)
///
/// This is the ONLY point where tactical gameplay results affect the database.
/// The tactical server computes everything (damage, deaths, loot) in game state,
/// then commits the final results here: XP gained and items earned.
///
/// This is idempotent - calling multiple times with the same mission_id
/// will only apply rewards once (subsequent calls are no-ops).
#[reducer]
pub fn commit_mission(
    ctx: &ReducerContext,
    mission_id: String,
    success: bool,
    xp_gained: i32,
    items_gained_json: String,
) -> Result<(), String> {
    let now = now_micros(ctx);

    // Check if already committed (idempotency)
    if ctx.db.mission_commit().mission_id().find(&mission_id).is_some() {
        log::info!("Mission {} already committed (idempotent)", mission_id);
        return Ok(());
    }

    // Get mission
    let Some(mut mission) = ctx.db.mission().id().find(&mission_id) else {
        return Err("Mission not found".to_string());
    };

    // Update mission status
    mission.status = if success {
        "completed".to_string()
    } else {
        "failed".to_string()
    };
    mission.completed_at_micros = now;
    ctx.db.mission().id().update(mission.clone());

    // Record commit
    ctx.db.mission_commit().insert(MissionCommit {
        mission_id: mission_id.clone(),
        committed_at_micros: now,
        success,
        xp_gained,
        items_gained_json: items_gained_json.clone(),
    });

    // Apply rewards to party members
    let members: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&mission.party_id)
        .collect();

    // Parse items from JSON
    let items: Vec<(String, i32)> = parse_items_json(&items_gained_json);

    for member in members {
        // Add XP
        if xp_gained > 0 {
            add_xp(ctx, &member.character_id, xp_gained);
        }

        // Add items
        for (item_id, qty) in &items {
            if *qty > 0 {
                let _ = add_item(ctx, &member.character_id, item_id, *qty);
            }
        }
    }

    log::info!(
        "Mission {} committed: success={}, xp={}, items={}",
        mission_id,
        success,
        xp_gained,
        items.len()
    );

    Ok(())
}

/// Cancel an active mission
#[reducer]
pub fn cancel_mission(ctx: &ReducerContext, mission_id: String) -> Result<(), String> {
    let now = now_micros(ctx);

    let Some(mut mission) = ctx.db.mission().id().find(&mission_id) else {
        return Err("Mission not found".to_string());
    };

    if mission.status != "active" {
        return Ok(()); // Already ended
    }

    mission.status = "cancelled".to_string();
    mission.completed_at_micros = now;
    ctx.db.mission().id().update(mission);

    Ok(())
}

// ============================================================================
// Quest Reducers
// ============================================================================

/// Start a quest for a character
#[reducer]
pub fn start_quest(
    ctx: &ReducerContext,
    character_id: String,
    quest_id: String,
) -> Result<(), String> {
    set_quest_status(ctx, &character_id, &quest_id, "active")
}

/// Complete a quest for a character
#[reducer]
pub fn complete_quest(
    ctx: &ReducerContext,
    character_id: String,
    quest_id: String,
) -> Result<(), String> {
    // Check current status
    let status = get_quest_status(ctx, &character_id, &quest_id);
    if status.as_deref() == Some("completed") {
        return Ok(()); // Already completed
    }

    // Get quest definition for rewards
    let quest_def = ctx.db.quest_def().quest_id().find(&quest_id)
        .ok_or_else(|| "Quest not found".to_string())?;

    set_quest_status(ctx, &character_id, &quest_id, "completed")?;

    // Apply rewards
    if quest_def.reward_xp > 0 {
        add_xp(ctx, &character_id, quest_def.reward_xp);
    }

    for (item_id, qty) in parse_items_json(&quest_def.reward_items_json) {
        if qty > 0 {
            add_item(ctx, &character_id, &item_id, qty)?;
        }
    }

    log::info!("Quest {} completed by {}", quest_id, character_id);

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn now_micros(ctx: &ReducerContext) -> i64 {
    ctx.timestamp.to_micros_since_unix_epoch()
}

fn allocate_port(ctx: &ReducerContext) -> Result<u16, String> {
    let Some(mut alloc) = ctx.db.port_allocation().id().find(&1) else {
        return Err("Port allocation not initialized".to_string());
    };

    let port = alloc.next_port;
    alloc.next_port = if alloc.next_port >= TACTICAL_PORT_END {
        TACTICAL_PORT_START
    } else {
        alloc.next_port + 1
    };

    ctx.db.port_allocation().id().update(alloc);
    Ok(port)
}

fn add_item(ctx: &ReducerContext, character_id: &str, item_id: &str, qty: i32) -> Result<(), String> {
    if qty <= 0 {
        return Ok(());
    }

    // Find existing item stack
    let existing = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(&character_id.to_string())
        .find(|item| item.item_id == item_id);

    match existing {
        Some(mut item) => {
            item.qty = item.qty.saturating_add(qty);
            ctx.db.inventory_item().id().update(item);
        }
        None => {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id: character_id.to_string(),
                item_id: item_id.to_string(),
                qty,
            });
        }
    }

    Ok(())
}

fn add_xp(ctx: &ReducerContext, character_id: &str, xp: i32) {
    if xp <= 0 {
        return;
    }

    if let Some(mut character) = ctx.db.character().id().find(&character_id.to_string()) {
        character.xp = character.xp.saturating_add(xp);

        // Simple level-up formula: level = 1 + xp / 100
        let new_level = 1 + (character.xp / 100);
        if new_level > character.level {
            character.level = new_level;
            log::info!("Character {} leveled up to level {}!", character_id, new_level);
        }

        character.updated_at_micros = now_micros(ctx);
        ctx.db.character().id().update(character);
    }
}

fn get_quest_status(ctx: &ReducerContext, character_id: &str, quest_id: &str) -> Option<String> {
    ctx.db
        .character_quest()
        .character_id()
        .filter(&character_id.to_string())
        .find(|q| q.quest_id == quest_id)
        .map(|q| q.status)
}

fn set_quest_status(
    ctx: &ReducerContext,
    character_id: &str,
    quest_id: &str,
    status: &str,
) -> Result<(), String> {
    let now = now_micros(ctx);

    let existing: Vec<_> = ctx
        .db
        .character_quest()
        .character_id()
        .filter(&character_id.to_string())
        .filter(|q| q.quest_id == quest_id)
        .collect();

    match existing.as_slice() {
        [] => {
            ctx.db.character_quest().insert(CharacterQuest {
                id: 0,
                character_id: character_id.to_string(),
                quest_id: quest_id.to_string(),
                status: status.to_string(),
                updated_at_micros: now,
            });
        }
        [first, rest @ ..] => {
            // Delete duplicates
            for extra in rest {
                ctx.db.character_quest().id().delete(extra.id);
            }
            // Update first
            let mut updated = first.clone();
            updated.status = status.to_string();
            updated.updated_at_micros = now;
            ctx.db.character_quest().id().update(updated);
        }
    }

    Ok(())
}

fn parse_items_json(json: &str) -> Vec<(String, i32)> {
    // Simple JSON array parser: [{"item_id":"x","qty":1},...]
    let mut items = Vec::new();

    if json.is_empty() || json == "[]" {
        return items;
    }

    // Very basic parsing - in production use serde_json
    for segment in json.split('{').skip(1) {
        let mut item_id = String::new();
        let mut qty = 0i32;

        for part in segment.split(',') {
            if part.contains("\"item_id\"") {
                if let Some(start) = part.find(':') {
                    let value = &part[start + 1..];
                    let value = value.trim().trim_matches('"').trim_matches('}');
                    item_id = value.to_string();
                }
            } else if part.contains("\"qty\"") {
                if let Some(start) = part.find(':') {
                    let value = &part[start + 1..];
                    let value = value.trim().trim_matches('}').trim_matches(']');
                    qty = value.parse().unwrap_or(0);
                }
            }
        }

        if !item_id.is_empty() && qty > 0 {
            items.push((item_id, qty));
        }
    }

    items
}

fn seed_quests(ctx: &ReducerContext) {
    // Seed some quest definitions
    if ctx.db.quest_def().quest_id().find(&"town_a_clear".to_string()).is_none() {
        ctx.db.quest_def().insert(QuestDef {
            quest_id: "town_a_clear".to_string(),
            title: "Clear Town A".to_string(),
            description: "Defeat all enemies in Town A".to_string(),
            reward_xp: 50,
            reward_items_json: r#"[{"item_id":"gold_coin","qty":10}]"#.to_string(),
        });
    }

    if ctx.db.quest_def().quest_id().find(&"town_b_clear".to_string()).is_none() {
        ctx.db.quest_def().insert(QuestDef {
            quest_id: "town_b_clear".to_string(),
            title: "Clear Town B".to_string(),
            description: "Defeat the boss in Town B".to_string(),
            reward_xp: 100,
            reward_items_json: r#"[{"item_id":"gold_coin","qty":25},{"item_id":"health_potion","qty":3}]"#.to_string(),
        });
    }
}
