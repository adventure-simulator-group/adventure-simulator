use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use crate::{
    Character, CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, Item,
    ItemSlot, character::character, character__view, character_attributes__view, character_equip,
    character_equip__view, character_limbs__view, character_skills__view, character_stats__view,
    complete_quest, inventory_item, inventory_item__view, item__view, party, record_battle_result,
};
use std::collections::{HashMap, HashSet};
use strum::VariantArray;

/// Request to start a new [`TacticalServer`]
#[derive(Clone, Debug)]
#[table(accessor = tactical_server_request, public)]
pub struct TacticalServerRequest {
    #[primary_key]
    #[unique]
    pub mission_id: String,
    pub scene_key: String,
}

/// Active tactical server instance
#[derive(Clone, Debug)]
#[table(accessor = tactical_server, public)]
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
    pub items: Vec<ConnectedPlayerItem>,
    pub skills: CharacterSkills,
    pub stats: CharacterStats,
    pub attrs: CharacterAttributes,
    pub limbs: CharacterLimbs,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct ConnectedPlayerItem {
    pub quantity: u32,
    pub item: Item,
    pub equipped: Option<ItemSlot>,
}

/// View of [`ConnectedPlayer`] for this [`TacticalServer`].
#[view(accessor = connected_players, public)]
pub fn connected_players(ctx: &ViewContext) -> Vec<ConnectedPlayer> {
    ctx.db
        .character()
        .server()
        .filter(ctx.sender())
        .filter_map(|character| {
            let limbs = ctx.db.character_limbs().character_id().find(character.id)?;
            let attrs = ctx
                .db
                .character_attributes()
                .character_id()
                .find(character.id)?;
            let skills = ctx
                .db
                .character_skills()
                .character_id()
                .find(character.id)?;
            let stats = ctx.db.character_stats().character_id().find(character.id)?;
            let items = connected_player_items(ctx, character.id).collect();

            Some(ConnectedPlayer {
                character,
                items,
                skills,
                limbs,
                attrs,
                stats,
            })
        })
        .collect()
}

fn connected_player_items(
    ctx: &ViewContext,
    character_id: u64,
) -> impl Iterator<Item = ConnectedPlayerItem> + use<'_> {
    let equip = ctx.db.character_equip().character_id().find(character_id);

    ctx.db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter_map(move |inventory_item| {
            let item = ctx.db.item().id().find(inventory_item.item_id)?;
            let equipped = equip.as_ref().and_then(|e| e.is_equiped(inventory_item.id));

            Some(ConnectedPlayerItem {
                quantity: inventory_item.quantity,
                item,
                equipped,
            })
        })
}

/// Put a character in an existing [`TacticalServer`].
#[reducer]
pub fn enter_mission(
    ctx: &ReducerContext,
    character_id: u64,
    server: Identity,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
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

    if character.temporary {
        log::info!("Leaving mission for character #{character_id}: removing temporary character..");
        ctx.db.character().delete(character);
    } else {
        log::info!("Leaving mission for character #{character_id}: resetting server info..");
        character.in_server = false;
        character.server = Identity::ZERO;
        ctx.db.character().id().update(character);
    }

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
    )
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
) -> Result<(), String> {
    if let Some(_previous) = ctx.db.tactical_server().identity().find(ctx.sender()) {
        return Err(format!(
            "{} already hosting a tactical server !",
            ctx.sender()
        ));
    }
    if let Some(previous) = ctx.db.tactical_server().mission_id().find(&mission_id) {
        log::info!("Ending previous server for mission '{mission_id}'...");
        end_tactical_server_by_instance(ctx, previous, false, 0)?;
    }

    log::info!("Tactical server for mission '{mission_id}' is ready on {addr}");
    let server = TacticalServer {
        identity: ctx.sender(),
        mission_id,
        scene_key,
        addr,
        cert_digest,
    };
    ctx.db.tactical_server().insert(server);
    Ok(())
}

/// End a [`TacticallServer`] associated with the caller.
#[reducer]
pub fn end_tactical_server(
    ctx: &ReducerContext,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    let Some(server) = ctx.db.tactical_server().identity().find(ctx.sender()) else {
        return Err(format!(
            "Can't end tactical server: sender's server with identity {} not found",
            ctx.sender()
        ));
    };

    end_tactical_server_by_instance(ctx, server, success, xp_gained)
}

/// End a [`TacticallServer`].
#[reducer]
fn end_tactical_server_by_instance(
    ctx: &ReducerContext,
    server: TacticalServer,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    if ctx
        .db
        .tactical_server()
        .identity()
        .find(&server.identity)
        .is_none()
    {
        return Err(format!(
            "Can't end tactical server: server with identity {} not found",
            server.identity
        ));
    }

    let connected: Vec<_> = ctx
        .db
        .character()
        .server()
        .filter(server.identity)
        .collect();
    if success {
        if let Some(adventurer) = connected.iter().find(|character| !character.temporary) {
            if let Some(party_id) = adventurer.party_id.as_ref() {
                if let Some(party) = ctx.db.party().id().find(party_id) {
                    if let Some(quest_id) = party.active_quest_id.as_ref() {
                        let mut drops: HashMap<String, u32> = HashMap::new();
                        for enemy in connected.iter().filter(|character| character.temporary) {
                            let Some(equip) =
                                ctx.db.character_equip().character_id().find(enemy.id)
                            else {
                                continue;
                            };
                            let mut seen = HashSet::new();
                            for slot in ItemSlot::VARIANTS {
                                let Some(inventory_id) = equip.get(*slot) else {
                                    continue;
                                };
                                if !seen.insert(inventory_id) {
                                    continue;
                                }
                                if let Some(inventory) =
                                    ctx.db.inventory_item().id().find(inventory_id)
                                {
                                    *drops.entry(inventory.item_id).or_default() += 1;
                                }
                            }
                        }
                        record_battle_result(
                            ctx,
                            party_id,
                            quest_id,
                            &server.mission_id,
                            drops.into_iter().collect(),
                            true,
                        )?;
                        complete_quest(ctx, quest_id.clone())?;
                    }
                }
            }
        }
    }

    // Apply persistent character progression. Loot is handled by the battle result.
    for mut character in connected {
        if xp_gained > 0 {
            character.xp = character.xp.saturating_add_signed(xp_gained);
            character.level = 1 + character.xp / 100;
        }

        let id = character.id;
        ctx.db.character().id().update(character);
        leave_mission(ctx, id)?;
    }

    ctx.db.tactical_server().identity().delete(server.identity);

    log::info!(
        "Tactical server for mission '{}' ended: success={success}, xp={xp_gained}",
        server.mission_id
    );
    Ok(())
}
