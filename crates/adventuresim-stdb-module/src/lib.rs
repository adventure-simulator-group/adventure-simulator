//! Strategic Layer SpacetimeDB Module (Minimal Demo)
//!
//! Simple Mount & Blade style architecture:
//! - Strategic layer: character, inventory, missions (SpacetimeDB)
//! - Tactical layer: real-time gameplay (Lightyear server process)
//!
//! Flow:
//! 1. Player clicks "Enter Town" → creates mission with status "pending"
//! 2. Spawner sees pending mission → starts tactical-server process
//! 3. Tactical-server writes connection info → status becomes "ready"
//! 4. Browser client sees "ready" → connects via WebTransport/WebSocket
//! 5. Mission ends → tactical-server commits results, exits

use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};
use std::hash::{DefaultHasher, Hash, Hasher};

// ============================================================================
// Types
// ============================================================================

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TacticalStatus {
    Pending,
    Ready,
    Ended,
}

// ============================================================================
// Tables
// ============================================================================

/// Character (strategic state only - no HP, that's tactical)
#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    pub id: u64,
    pub name: String,
    pub xp: i32,
    pub level: i32,
    #[index(btree)]
    pub in_server: String,
}

/// Inventory item
#[derive(Clone, Debug)]
#[table(name = inventory_item, public)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub item_id: String,
    pub qty: i32,
}

/// Active tactical server instance
/// Written by tactical-server when it starts, read by browser client
#[derive(Clone, Debug)]
#[table(name = tactical_server, public)]
pub struct TacticalServer {
    #[primary_key]
    pub mission_id: String,
    pub scene_key: String,
    pub status: TacticalStatus,
    /// Connection info (written by tactical-server)
    pub addr: String,
    pub cert_digest: String,
}

// ============================================================================
// Reducers
// ============================================================================

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

    // Starter items
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: character.id,
        item_id: "torch".into(),
        qty: 1,
    });
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: character.id,
        item_id: "bandage".into(),
        qty: 3,
    });

    Ok(())
}

/// Put character in an existing tactical server by its mission id.
#[reducer]
pub fn enter_mission(
    ctx: &ReducerContext,
    character_id: u64,
    mission_id: String,
) -> Result<(), String> {
    // Check character exists
    let mut character = ctx
        .db
        .character()
        .id()
        .find(&character_id)
        .ok_or_else(|| "Character not found".to_string())?;

    // Check not already in a mission
    if !character.in_server.is_empty() {
        if ctx
            .db
            .tactical_server()
            .mission_id()
            .find(&character.in_server)
            .is_some_and(|t| t.status != TacticalStatus::Ended)
        {
            return Err("Already in a mission".into());
        }
    }

    let server = ctx
        .db
        .tactical_server()
        .mission_id()
        .find(&mission_id)
        .ok_or_else(|| "Server not found".to_string())?;

    character.in_server = server.mission_id;
    ctx.db.character().id().update(character);

    Ok(())
}

/// Start a new server for a mission and then put a character into it.
#[reducer]
pub fn create_mission_and_enter(
    ctx: &ReducerContext,
    character_id: u64,
    scene_key: String,
) -> Result<(), String> {
    let mission_id = format!("{scene_key}-{}", ctx.timestamp.to_micros_since_unix_epoch());

    create_tactical_server(ctx, mission_id.clone(), scene_key)?;
    enter_mission(ctx, character_id, mission_id)?;

    Ok(())
}

/// Start a new tactical server, if not already started.
#[reducer]
pub fn create_tactical_server(
    ctx: &ReducerContext,
    mission_id: String,
    scene_key: String,
) -> Result<(), String> {
    if ctx
        .db
        .tactical_server()
        .mission_id()
        .find(&mission_id)
        .is_some()
    {
        return Ok(());
    }

    // Validate scene
    if scene_key != "hills" && scene_key != "desert" {
        return Err(format!("Invalid scene: {}", scene_key));
    }

    log::info!("Tactical server {mission_id} created (pending)");
    ctx.db.tactical_server().insert(TacticalServer {
        mission_id,
        scene_key,
        status: TacticalStatus::Pending,
        addr: String::new(),
        cert_digest: String::new(),
    });

    Ok(())
}

/// Called by tactical-server when it starts - provides connection info
#[reducer]
pub fn tactical_server_ready(
    ctx: &ReducerContext,
    mission_id: String,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    let Some(mut server) = ctx.db.tactical_server().mission_id().find(&mission_id) else {
        return Err("Mission not found".into());
    };

    server.status = TacticalStatus::Ready;
    server.addr = addr.clone();
    server.cert_digest = cert_digest;
    ctx.db.tactical_server().mission_id().update(server);

    log::info!("Mission {} ready on {}", mission_id, addr);
    Ok(())
}

/// Called by tactical-server when mission ends - applies rewards
#[reducer]
pub fn commit_mission(
    ctx: &ReducerContext,
    mission_id: String,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    let Some(mut server) = ctx.db.tactical_server().mission_id().find(&mission_id) else {
        return Err("Mission not found".into());
    };

    if server.status == TacticalStatus::Ended {
        return Ok(()); // Idempotent
    }

    // Apply rewards
    for mut character in ctx.db.character().in_server().filter(&server.mission_id) {
        character.in_server.clear();
        if xp_gained > 0 {
            character.xp += xp_gained;
            character.level = 1 + character.xp / 100;
        }
        if success {
            // Victory loot
            add_item(ctx, character.id, "gold_coin", 10);
            add_item(ctx, character.id, "health_potion", 2);
        }
        ctx.db.character().id().update(character);
    }

    server.status = TacticalStatus::Ended;
    ctx.db.tactical_server().mission_id().update(server);

    log::info!(
        "Mission {} ended: success={}, xp={}",
        mission_id,
        success,
        xp_gained
    );
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn add_item(ctx: &ReducerContext, character_id: u64, item_id: &str, qty: i32) {
    // Find existing stack
    let existing = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .find(|i| i.item_id == item_id);

    match existing {
        Some(mut item) => {
            item.qty += qty;
            ctx.db.inventory_item().id().update(item);
        }
        None => {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id: character_id.into(),
                item_id: item_id.into(),
                qty,
            });
        }
    }
}
