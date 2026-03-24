use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

use crate::{change_inventory_item, character::character};

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TacticalStatus {
    Pending,
    Ready,
    Ended,
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
            character.xp = character.xp.saturating_add_signed(xp_gained);
            character.level = 1 + character.xp / 100;
        }
        if success {
            // Victory loot
            change_inventory_item(ctx, character.id, "gold_coin", 10);
            change_inventory_item(ctx, character.id, "health_potion", 2);
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
