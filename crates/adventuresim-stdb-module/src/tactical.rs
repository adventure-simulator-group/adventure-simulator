use spacetimedb::{
    reducer, table, view, Identity, ReducerContext, SpacetimeType, Table, ViewContext,
};

use crate::{
    change_inventory_item, character::character, character__view, character_equip__view,
    character_limbs__view, character_skills__view, Character, CharacterEquip, CharacterLimbs,
    CharacterSkills,
};

/// Request to start a new [`TacticalServer`]
#[derive(Clone, Debug)]
#[table(name = tactical_server_request, public)]
pub struct TacticalServerRequest {
    #[primary_key]
    #[unique]
    pub mission_id: String,
    pub scene_key: String,
}

/// Active tactical server instance
#[derive(Clone, Debug)]
#[table(name = tactical_server, public)]
pub struct TacticalServer {
    #[primary_key]
    pub identity: Identity,
    #[index(btree)]
    #[unique]
    pub mission_id: String,
    pub scene_key: String,
    pub addr: String,
    pub cert_digest: String,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct ConnectedPlayer {
    pub character: Character,
    pub equip: CharacterEquip,
    pub skills: CharacterSkills,
    pub limbs: CharacterLimbs,
}

/// View of [`ConnectedPlayer`] for this [`TacticalServer`].
#[view(name = connected_players, public)]
fn connected_players(ctx: &ViewContext) -> Vec<ConnectedPlayer> {
    ctx.db
        .character()
        .server()
        .filter(ctx.sender)
        .filter_map(|character| {
            let limbs = ctx.db.character_limbs().character_id().find(character.id)?;
            let skills = ctx
                .db
                .character_skills()
                .character_id()
                .find(character.id)?;
            let equip = ctx.db.character_equip().character_id().find(character.id)?;

            Some(ConnectedPlayer {
                character,
                equip,
                skills,
                limbs,
            })
        })
        .collect()
}

/// Put a character in an existing [`TacticalServer`].
#[reducer]
pub fn enter_mission(
    ctx: &ReducerContext,
    character_id: u64,
    server: Identity,
) -> Result<(), String> {
    // Check character exists
    let mut character = ctx
        .db
        .character()
        .id()
        .find(&character_id)
        .ok_or_else(|| format!("Character {character_id} not found"))?;

    // Check not already in a mission
    if character.in_server {
        if ctx
            .db
            .tactical_server()
            .identity()
            .find(character.server)
            .is_some()
        {
            log::warn!("Character {character_id} is already in a mission, ejecting..");
        }
    }

    let server = ctx
        .db
        .tactical_server()
        .identity()
        .find(&server)
        .ok_or_else(|| format!("Server {server} not found"))?;

    character.server = server.identity;
    character.in_server = true;
    ctx.db.character().id().update(character);

    Ok(())
}

/// Take out character from an existing [`TacticalServer`].
#[reducer]
pub fn leave_mission(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let mut character = ctx
        .db
        .character()
        .id()
        .find(&character_id)
        .ok_or_else(|| format!("Character {character_id} not found"))?;

    character.in_server = false;
    character.server = Identity::ZERO;
    ctx.db.character().id().update(character);

    Ok(())
}

/// Request a new [`TacticalServer`] with generated *mission_id* from
/// the *scene_key* and the current timestamp.
#[reducer]
pub fn request_tactical_server_for_scene(
    ctx: &ReducerContext,
    scene_key: String,
) -> Result<(), String> {
    let mission_id = format!("{scene_key}-{}", ctx.timestamp.to_micros_since_unix_epoch());
    request_tactical_server(ctx, mission_id, scene_key)
}

/// Request a new [`TacticalServer`], unless there is already
/// a server with the same *mission_id*.
#[reducer]
pub fn request_tactical_server(
    ctx: &ReducerContext,
    mission_id: String,
    scene_key: String,
) -> Result<(), String> {
    if let Some(server) = ctx.db.tactical_server().mission_id().find(&mission_id) {
        return Err(format!(
            "Server for mission '{mission_id}' already exist: {}",
            server.identity
        ));
    }

    log::info!("Tactical server for '{mission_id}' requested");
    ctx.db
        .tactical_server_request()
        .insert(TacticalServerRequest {
            mission_id,
            scene_key,
        });

    Ok(())
}

/// Creates a new [`TacticalServer`], fulfilling the associated [`TacticalServerRequest`].
///
/// It will fail if there is no request for the mission.
///
/// Should be called by a server instance, since its identity will be
/// the identity of the [`TacticalServer`] in DB.
#[reducer]
pub fn create_tactical_server_for_request(
    ctx: &ReducerContext,
    mission_id: String,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    let Some(request) = ctx
        .db
        .tactical_server_request()
        .mission_id()
        .find(&mission_id)
    else {
        return Err(format!(
            "Tactical server request for mission '{mission_id}' not found"
        ));
    };
    ctx.db
        .tactical_server_request()
        .mission_id()
        .delete(&mission_id);

    create_tactical_server(
        ctx,
        request.mission_id,
        request.scene_key,
        addr,
        cert_digest,
    );
    Ok(())
}

/// Creates a new [`TacticalServer`].
///
/// Should be called by a server instance, since its identity will be
/// the identity of the [`TacticalServer`] in DB.
#[reducer]
pub fn create_tactical_server(
    ctx: &ReducerContext,
    mission_id: String,
    scene_key: String,
    addr: String,
    cert_digest: String,
) {
    log::info!("Tactical server for mission '{mission_id}' is ready on {addr}");
    let server = TacticalServer {
        identity: ctx.sender,
        mission_id,
        scene_key,
        addr,
        cert_digest,
    };
    ctx.db.tactical_server().insert(server);
}

/// End a [`TacticallServer`] associated with the caller.
#[reducer]
pub fn end_tactical_server(
    ctx: &ReducerContext,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    let Some(server) = ctx.db.tactical_server().identity().find(ctx.sender) else {
        return Err(format!("Tactical server {} not found", ctx.sender));
    };

    // Apply rewards
    for mut character in ctx.db.character().server().filter(ctx.sender) {
        leave_mission(ctx, character.id)?;

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

    ctx.db.tactical_server().identity().delete(ctx.sender);

    log::info!(
        "Tactical server for mission '{}' ended: success={success}, xp={xp_gained}",
        server.mission_id
    );
    Ok(())
}
