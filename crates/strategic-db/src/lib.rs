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
    pub id: String,
    pub name: String,
    pub xp: i32,
    pub level: i32,
}

/// Inventory item
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
    /// Character in this mission
    pub character_id: String,
}

// ============================================================================
// Reducers
// ============================================================================

/// Create or update a character
#[reducer]
pub fn create_character(ctx: &ReducerContext, id: String, name: String) -> Result<(), String> {
    if ctx.db.character().id().find(&id).is_some() {
        return Ok(()); // Already exists
    }

    ctx.db.character().insert(Character {
        id: id.clone(),
        name,
        xp: 0,
        level: 1,
    });

    // Starter items
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: id.clone(),
        item_id: "torch".into(),
        qty: 1,
    });
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: id,
        item_id: "bandage".into(),
        qty: 3,
    });

    Ok(())
}

/// Player wants to enter a tactical mission
/// Creates a "pending" entry - spawner will see this and start server
#[reducer]
pub fn enter_mission(
    ctx: &ReducerContext,
    character_id: String,
    scene_key: String,
) -> Result<(), String> {
    // Validate scene
    if scene_key != "town_a" && scene_key != "town_b" {
        return Err(format!("Invalid scene: {}", scene_key));
    }

    // Check character exists
    if ctx.db.character().id().find(&character_id).is_none() {
        return Err("Character not found".into());
    }

    // Check not already in a mission
    if ctx
        .db
        .tactical_server()
        .iter()
        .any(|t| t.character_id == character_id && t.status != TacticalStatus::Ended)
    {
        return Err("Already in a mission".into());
    }

    let mission_id = format!(
        "{}-{}",
        character_id,
        ctx.timestamp.to_micros_since_unix_epoch()
    );

    ctx.db.tactical_server().insert(TacticalServer {
        mission_id: mission_id.clone(),
        scene_key,
        status: TacticalStatus::Pending,
        addr: String::new(),
        cert_digest: String::new(),
        character_id,
    });

    log::info!("Mission {} created (pending)", mission_id);
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
    if let Some(mut character) = ctx.db.character().id().find(&server.character_id) {
        if success && xp_gained > 0 {
            character.xp += xp_gained;
            character.level = 1 + character.xp / 100;
            ctx.db.character().id().update(character.clone());

            // Victory loot
            add_item(ctx, &server.character_id, "gold_coin", 10);
            add_item(ctx, &server.character_id, "health_potion", 2);
        }
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

/// Leave/cancel a mission
#[reducer]
pub fn leave_mission(ctx: &ReducerContext, mission_id: String) -> Result<(), String> {
    if let Some(mut server) = ctx.db.tactical_server().mission_id().find(&mission_id) {
        server.status = TacticalStatus::Ended;
        ctx.db.tactical_server().mission_id().update(server);
    }
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn add_item(ctx: &ReducerContext, character_id: &str, item_id: &str, qty: i32) {
    // Find existing stack
    let existing = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(&character_id.to_string())
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
