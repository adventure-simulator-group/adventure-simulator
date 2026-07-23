use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use crate::repair::{ItemCondition, item_condition__view};
use crate::{
    Character, CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, Item,
    ItemSlot, character::character, character__view, character_attributes__view, character_equip,
    character_equip__view, character_limbs__view, character_skills__view, character_stats__view,
    complete_quest, inventory_item, inventory_item__view, investigation::case_site_authority,
    item__view, party_authority, record_battle_result, strategic::quest,
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
    #[index(btree)]
    pub quest_id: String,
    pub party_id: String,
    pub requested_by: u64,
    pub required_enemy_kills: u32,
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
    #[index(btree)]
    pub quest_id: String,
    pub party_id: String,
    pub addr: String,
    pub cert_digest: String,
    pub required_enemy_kills: u32,
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
    pub inventory_item_id: u64,
    pub quantity: u32,
    pub item: Item,
    pub equipped: Option<ItemSlot>,
    pub condition: Option<ItemCondition>,
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
            let mut item = ctx.db.item().id().find(inventory_item.item_id)?;
            let condition = ctx
                .db
                .item_condition()
                .inventory_item_id()
                .find(inventory_item.id);
            if let Some(condition) = &condition {
                let damage = condition.bins();
                item.accuracy = adventuresim_core::durability::effective_weapon_stat(
                    item.accuracy,
                    damage,
                    item.edge_sensitivity,
                );
                item.penetration = adventuresim_core::durability::effective_weapon_stat(
                    item.penetration,
                    damage,
                    item.edge_sensitivity * 0.6,
                );
                item.block = adventuresim_core::durability::effective_weapon_stat(
                    item.block,
                    damage,
                    item.handling_sensitivity,
                );
                item.range_of_motion = adventuresim_core::durability::effective_handling(
                    item.range_of_motion,
                    damage,
                    item.handling_sensitivity,
                );
            }
            let equipped = equip.as_ref().and_then(|e| e.is_equiped(inventory_item.id));

            Some(ConnectedPlayerItem {
                inventory_item_id: inventory_item.id,
                quantity: inventory_item.quantity,
                item,
                equipped,
                condition,
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
    if ctx.sender() != server {
        return Err("Only the owning tactical server can enroll characters".into());
    }
    crate::character::require_living_character(ctx, character_id)?;
    // Check character exists
    let mut character = ctx
        .db
        .character()
        .id()
        .find(&character_id)
        .ok_or_else(|| format!("Character {character_id} not found"))?;
    // An unassigned character may join. An assignment to this server is an
    // idempotent retry, and a stale assignment whose server no longer exists
    // may be reclaimed. Never steal a character from another active server.
    if character.in_server
        && character.server != server
        && ctx
            .db
            .tactical_server()
            .identity()
            .find(character.server)
            .is_some()
    {
        return Err("Character is already assigned to another active tactical server".into());
    }

    let server = ctx
        .db
        .tactical_server()
        .identity()
        .find(&server)
        .ok_or_else(|| format!("Server {server} not found"))?;

    if !character.temporary {
        let character_party = character
            .party_id
            .as_deref()
            .ok_or("Character has no party")?;
        if character_party != server.party_id {
            return Err("Character is not a member of this mission's party".into());
        }
        let party = ctx
            .db
            .party_authority()
            .id()
            .find(&server.party_id)
            .ok_or("Mission party no longer exists")?;
        let quest = ctx
            .db
            .quest()
            .id()
            .find(&server.quest_id)
            .ok_or("Mission quest no longer exists")?;
        if party.active_quest_id.as_deref() != Some(server.quest_id.as_str())
            || quest.accepted_by.as_deref() != Some(server.party_id.as_str())
        {
            return Err("Mission party and quest binding changed before enrollment".into());
        }
    }

    character.server = server.identity;
    character.in_server = true;
    ctx.db.character().id().update(character);

    Ok(())
}

/// Take out character from an existing [`TacticalServer`].
#[reducer]
pub fn leave_mission(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let server = ctx
        .db
        .tactical_server()
        .identity()
        .find(ctx.sender())
        .ok_or("Only a registered tactical server can remove characters")?;
    let character = ctx
        .db
        .character()
        .id()
        .find(&character_id)
        .ok_or_else(|| format!("Character {character_id} not found"))?;
    leave_mission_for_server(ctx, character, server.identity)
}

fn leave_mission_for_server(
    ctx: &ReducerContext,
    mut character: Character,
    server: Identity,
) -> Result<(), String> {
    if !character.in_server || character.server != server {
        return Err("Only the character's owning tactical server can remove it".into());
    }
    let character_id = character.id;
    if character.temporary {
        log::info!("Leaving mission for character #{character_id}: removing temporary character..");
        crate::character::delete_temporary_character(ctx, character)?;
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
    character_id: u64,
    scene_key: String,
) -> Result<(), String> {
    let mission_id = format!("{scene_key}-{}", ctx.timestamp.to_micros_since_unix_epoch());
    request_tactical_server(ctx, character_id, mission_id, scene_key)
}

/// Request a new [`TacticalServer`], unless there is already
/// a server with the same *mission_id*.
#[reducer]
pub fn request_tactical_server(
    ctx: &ReducerContext,
    character_id: u64,
    mission_id: String,
    scene_key: String,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can request a tactical server".into());
    }
    let quest_id = party
        .active_quest_id
        .clone()
        .ok_or("Party has no active quest")?;
    let case_site = party
        .current_case_site_id
        .as_ref()
        .and_then(|site_id| ctx.db.case_site_authority().id().find(site_id))
        .filter(|site| site.case_id == quest_id)
        .ok_or("Party must be at its active quest site")?;
    if party.current_case_site_id.as_deref() != Some(case_site.id.as_str()) {
        return Err("Party must be at its active quest location".into());
    }
    let quest = ctx
        .db
        .quest()
        .id()
        .find(&quest_id)
        .ok_or("Quest not found")?;
    if quest.accepted_by.as_deref() != Some(&party_id) || case_site.scene_key != scene_key {
        return Err("Tactical scene does not match the party's active quest".into());
    }
    if let Some(server) = ctx.db.tactical_server().mission_id().find(&mission_id) {
        return Err(format!(
            "Server for mission '{mission_id}' already exist: {}",
            server.identity
        ));
    }
    if ctx
        .db
        .tactical_server_request()
        .iter()
        .any(|request| request.quest_id == quest_id)
        || ctx
            .db
            .tactical_server()
            .iter()
            .any(|server| server.quest_id == quest_id)
    {
        return Err("Party quest already has a pending or active tactical server".into());
    }

    log::info!("Tactical server for '{mission_id}' requested");
    ctx.db
        .tactical_server_request()
        .insert(TacticalServerRequest {
            mission_id,
            scene_key,
            quest_id,
            party_id,
            requested_by: character_id,
            required_enemy_kills: u32::try_from(quest.enemy_count.max(0))
                .map_err(|_| "Quest enemy count exceeds tactical limits")?,
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

    insert_tactical_server(
        ctx,
        request.mission_id,
        request.scene_key,
        request.quest_id,
        request.party_id,
        request.required_enemy_kills,
        addr,
        cert_digest,
    )
}

/// Creates a new [`TacticalServer`].
///
/// Should be called by a server instance, since its identity will be
/// the identity of the [`TacticalServer`] in DB.
fn insert_tactical_server(
    ctx: &ReducerContext,
    mission_id: String,
    scene_key: String,
    quest_id: String,
    party_id: String,
    required_enemy_kills: u32,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    if let Some(_previous) = ctx.db.tactical_server().identity().find(ctx.sender()) {
        return Err(format!(
            "{} already hosting a tactical server !",
            ctx.sender()
        ));
    }
    if ctx
        .db
        .tactical_server()
        .mission_id()
        .find(&mission_id)
        .is_some()
    {
        return Err(format!("Server for mission '{mission_id}' already exists"));
    }

    log::info!("Tactical server for mission '{mission_id}' is ready on {addr}");
    let server = TacticalServer {
        identity: ctx.sender(),
        mission_id,
        scene_key,
        quest_id,
        party_id,
        addr,
        cert_digest,
        required_enemy_kills,
    };
    ctx.db.tactical_server().insert(server);
    Ok(())
}

/// End a [`TacticallServer`] associated with the caller.
#[reducer]
pub fn end_tactical_server(
    ctx: &ReducerContext,
    success: bool,
    _reported_xp_gained: i32,
) -> Result<(), String> {
    let Some(server) = ctx.db.tactical_server().identity().find(ctx.sender()) else {
        return Err(format!(
            "Can't end tactical server: sender's server with identity {} not found",
            ctx.sender()
        ));
    };

    // Persistent progression is derived from the authoritative quest reward
    // in complete_quest. Tactical processes cannot mint arbitrary XP.
    end_tactical_server_by_instance(ctx, server, success)
}

/// End a [`TacticallServer`].
fn end_tactical_server_by_instance(
    ctx: &ReducerContext,
    server: TacticalServer,
    success: bool,
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
        let party = ctx
            .db
            .party_authority()
            .id()
            .find(&server.party_id)
            .ok_or("Mission party no longer exists")?;
        let quest = ctx
            .db
            .quest()
            .id()
            .find(&server.quest_id)
            .ok_or("Mission quest no longer exists")?;
        if party.active_quest_id.as_deref() != Some(server.quest_id.as_str())
            || quest.accepted_by.as_deref() != Some(server.party_id.as_str())
        {
            return Err("Mission party and quest binding changed before completion".into());
        }
        if let Some(adventurer) = connected.iter().find(|character| !character.temporary) {
            if adventurer.party_id.as_deref() == Some(server.party_id.as_str()) {
                let mut drops: HashMap<String, u32> = HashMap::new();
                for enemy in connected.iter().filter(|character| character.temporary) {
                    let Some(equip) = ctx.db.character_equip().character_id().find(enemy.id) else {
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
                        if let Some(inventory) = ctx.db.inventory_item().id().find(inventory_id) {
                            *drops.entry(inventory.item_id).or_default() += 1;
                        }
                    }
                }
                record_battle_result(
                    ctx,
                    &server.party_id,
                    &server.quest_id,
                    &server.mission_id,
                    drops.into_iter().collect(),
                    true,
                )?;
                complete_quest(ctx, server.quest_id.clone())?;
            }
        }
    }

    // Apply persistent character progression. Loot is handled by the battle result.
    for character in connected {
        leave_mission_for_server(ctx, character, server.identity)?;
    }

    ctx.db.tactical_server().identity().delete(server.identity);

    log::info!(
        "Tactical server for mission '{}' ended: success={success}",
        server.mission_id
    );
    Ok(())
}
