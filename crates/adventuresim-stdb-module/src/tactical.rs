use sha2::{Digest, Sha256};
use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use crate::repair::{ItemCondition, item_condition__view};
use crate::{
    Character, CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, Item,
    ItemSlot,
    character::character,
    character__view, character_attributes__view, character_equip__view, character_limbs__view,
    character_skills__view, character_stats__view, inventory_item__view,
    investigation::case_site_authority,
    item__view, party_authority,
    strategic::{
        autoresolve_report, commit_victorious_battle, ensure_bound_mission_authority,
        hostile_group_authority, mission_authority, outcome_source_authority,
        strategic_gateway_authority__view,
    },
};

/// Request to start a new [`TacticalServer`]
#[derive(Clone, Debug)]
#[table(accessor = tactical_server_request_authority)]
pub struct TacticalServerRequest {
    #[primary_key]
    #[unique]
    pub mission_id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub scene_key: String,
    pub party_id: String,
    pub requested_by: u64,
    pub required_enemy_kills: u32,
}

/// Active tactical server instance
#[derive(Clone, Debug)]
#[table(accessor = tactical_server_authority)]
pub struct TacticalServer {
    #[primary_key]
    pub identity: Identity,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    #[unique]
    pub mission_id: String,
    pub scene_key: String,
    pub party_id: String,
    pub addr: String,
    pub cert_digest: String,
    pub required_enemy_kills: u32,
}

#[view(accessor = tactical_server_request, public)]
pub fn tactical_server_request(ctx: &ViewContext) -> Vec<TacticalServerRequest> {
    let is_gateway = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender());
    is_gateway
        .then(|| {
            ctx.db
                .tactical_server_request_authority()
                .gateway_bucket()
                .filter(0u8)
                .collect()
        })
        .unwrap_or_default()
}

#[view(accessor = tactical_server, public)]
pub fn tactical_server(ctx: &ViewContext) -> Vec<TacticalServer> {
    let is_gateway = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender());
    if is_gateway {
        ctx.db
            .tactical_server_authority()
            .gateway_bucket()
            .filter(0u8)
            .collect()
    } else {
        ctx.db
            .tactical_server_authority()
            .identity()
            .find(ctx.sender())
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Debug)]
#[table(accessor = tactical_server_claim)]
pub struct TacticalServerClaim {
    #[primary_key]
    pub mission_id: String,
    pub claim_hash: Vec<u8>,
}

#[reducer]
pub fn authorize_tactical_server_claim(
    ctx: &ReducerContext,
    mission_id: String,
    claim_hash: Vec<u8>,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    if claim_hash.len() != 32 {
        return Err("Tactical claim hash must be SHA-256".into());
    }
    if ctx
        .db
        .tactical_server_request_authority()
        .mission_id()
        .find(&mission_id)
        .is_none()
    {
        return Err("Tactical server request not found".into());
    }
    if ctx
        .db
        .tactical_server_claim()
        .mission_id()
        .find(&mission_id)
        .is_some()
    {
        return Err("Tactical server request is already claimed".into());
    }
    ctx.db.tactical_server_claim().insert(TacticalServerClaim {
        mission_id,
        claim_hash,
    });
    Ok(())
}

/// Release an unconsumed claim after the trusted dispatcher fails to start
/// its child process. Consumed claims and active servers have no row to revoke.
#[reducer]
pub fn revoke_tactical_server_claim(
    ctx: &ReducerContext,
    mission_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
    if ctx
        .db
        .tactical_server_request_authority()
        .mission_id()
        .find(&mission_id)
        .is_none()
    {
        return Err("Tactical server request is no longer pending".into());
    }
    ctx.db
        .tactical_server_claim()
        .mission_id()
        .delete(&mission_id);
    Ok(())
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
            .tactical_server_authority()
            .identity()
            .find(character.server)
            .is_some()
    {
        return Err("Character is already assigned to another active tactical server".into());
    }

    let server = ctx
        .db
        .tactical_server_authority()
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
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&server.mission_id)
            .ok_or("Mission authority no longer exists")?;
        if mission.party_id != party.id {
            return Err("Mission authority changed before enrollment".into());
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
        .tactical_server_authority()
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
    let mission_id = format!(
        "mission:{scene_key}-{}",
        ctx.timestamp.to_micros_since_unix_epoch()
    );
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
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
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
    let case_site = party
        .current_case_site_id
        .as_ref()
        .and_then(|site_id| ctx.db.case_site_authority().id_key().find(&site_id.value))
        .ok_or("Party must be at a case site")?;
    if party.current_case_site_id.as_deref() != Some(case_site.id.as_str()) {
        return Err("Party must be at its active quest location".into());
    }
    if case_site.scene_key != scene_key {
        return Err("Tactical scene does not match the occupied case site".into());
    }
    let mission =
        ensure_bound_mission_authority(ctx, &mission_id, &party_id, &case_site, &scene_key)?;
    let hostile_group_id = mission
        .hostile_group_id
        .clone()
        .ok_or("Case-site mission has no hostile group")?;
    let group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&hostile_group_id)
        .ok_or("Hostile group not found")?;
    if group.defeated {
        return Err("Hostile group is already defeated".into());
    }
    let battle_id = format!("battle:{mission_id}");
    let outcome_source_id = format!("outcome:{mission_id}");
    if ctx
        .db
        .autoresolve_report()
        .battle_id()
        .find(&battle_id)
        .is_some()
        || ctx
            .db
            .outcome_source_authority()
            .id()
            .find(&outcome_source_id)
            .is_some()
    {
        return Err("Mission attempt already has a strategic resolution".into());
    }
    if let Some(server) = ctx
        .db
        .tactical_server_authority()
        .mission_id()
        .find(&mission_id)
    {
        return Err(format!(
            "Server for mission '{mission_id}' already exist: {}",
            server.identity
        ));
    }
    if ctx
        .db
        .tactical_server_request_authority()
        .iter()
        .any(|request| {
            ctx.db
                .mission_authority()
                .id()
                .find(&request.mission_id)
                .is_some_and(|mission| {
                    mission.hostile_group_id.as_deref() == Some(&hostile_group_id)
                })
        })
        || ctx.db.tactical_server_authority().iter().any(|server| {
            ctx.db
                .mission_authority()
                .id()
                .find(&server.mission_id)
                .is_some_and(|mission| {
                    mission.hostile_group_id.as_deref() == Some(&hostile_group_id)
                })
        })
    {
        return Err("Party quest already has a pending or active tactical server".into());
    }

    log::info!("Tactical server for '{mission_id}' requested");
    ctx.db
        .tactical_server_request_authority()
        .insert(TacticalServerRequest {
            mission_id,
            gateway_bucket: 0,
            scene_key,
            party_id,
            requested_by: character_id,
            required_enemy_kills: group.enemy_count,
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
    claim: String,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
    let claim_row = ctx
        .db
        .tactical_server_claim()
        .mission_id()
        .find(&mission_id)
        .ok_or("Tactical server request has no authorized claim")?;
    let actual = Sha256::digest(claim.as_bytes());
    if actual.len() != claim_row.claim_hash.len()
        || actual
            .iter()
            .zip(&claim_row.claim_hash)
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            != 0
    {
        return Err("Tactical server claim is invalid".into());
    }
    let Some(request) = ctx
        .db
        .tactical_server_request_authority()
        .mission_id()
        .find(&mission_id)
    else {
        return Err(format!(
            "Tactical server request for mission '{mission_id}' not found"
        ));
    };
    ctx.db
        .tactical_server_request_authority()
        .mission_id()
        .delete(&mission_id);
    ctx.db
        .tactical_server_claim()
        .mission_id()
        .delete(&mission_id);

    insert_tactical_server(
        ctx,
        request.mission_id,
        request.scene_key,
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
    party_id: String,
    required_enemy_kills: u32,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    if let Some(_previous) = ctx
        .db
        .tactical_server_authority()
        .identity()
        .find(ctx.sender())
    {
        return Err(format!(
            "{} already hosting a tactical server !",
            ctx.sender()
        ));
    }
    if ctx
        .db
        .tactical_server_authority()
        .mission_id()
        .find(&mission_id)
        .is_some()
    {
        return Err(format!("Server for mission '{mission_id}' already exists"));
    }

    log::info!("Tactical server for mission '{mission_id}' is ready on {addr}");
    let server = TacticalServer {
        identity: ctx.sender(),
        gateway_bucket: 0,
        mission_id,
        scene_key,
        party_id,
        addr,
        cert_digest,
        required_enemy_kills,
    };
    ctx.db.tactical_server_authority().insert(server);
    Ok(())
}

/// End a [`TacticallServer`] associated with the caller.
#[reducer]
pub fn end_tactical_server(
    ctx: &ReducerContext,
    success: bool,
    _reported_xp_gained: i32,
) -> Result<(), String> {
    let Some(server) = ctx
        .db
        .tactical_server_authority()
        .identity()
        .find(ctx.sender())
    else {
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
        .tactical_server_authority()
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
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&server.mission_id)
            .ok_or("Mission authority no longer exists")?;
        if mission.party_id != server.party_id {
            return Err("Mission authority changed before completion".into());
        }
        if let Some(adventurer) = connected.iter().find(|character| !character.temporary) {
            if adventurer.party_id.as_deref() == Some(server.party_id.as_str()) {
                let group = mission
                    .hostile_group_id
                    .as_ref()
                    .and_then(|id| ctx.db.hostile_group_authority().id().find(id))
                    .ok_or("Bound mission hostile group no longer exists")?;
                let drops = group
                    .drop_item_id
                    .clone()
                    .map(|item| vec![(item, group.drop_quantity)])
                    .unwrap_or_default();
                let battle_id = format!("battle:{}", server.mission_id);
                let outcome_source_id = format!("outcome:{}", server.mission_id);
                let committed = commit_victorious_battle(
                    ctx,
                    &outcome_source_id,
                    &battle_id,
                    &server.party_id,
                    Some(&server.mission_id),
                    mission.hostile_group_id.as_deref(),
                    drops,
                    true,
                )?;
                if committed
                    && let Some(group_id) = mission.hostile_group_id.as_deref()
                    && !crate::strategic::finish_incident_for_hostile_group(ctx, group_id)?
                    && let Some(group) = ctx
                        .db
                        .hostile_group_authority()
                        .id()
                        .find(&group_id.to_string())
                    && let Some(site) = ctx
                        .db
                        .case_site_authority()
                        .id_key()
                        .find(&group.case_site_id.value)
                    && ctx
                        .db
                        .party_authority()
                        .id()
                        .find(&server.party_id)
                        .is_some_and(|party| {
                            party.active_quest_id.as_deref() == Some(site.case_id.as_str())
                        })
                {
                    crate::complete_quest(ctx, site.case_id)?;
                }
            }
        }
    }

    // Apply persistent character progression. Loot is handled by the battle result.
    for character in connected {
        leave_mission_for_server(ctx, character, server.identity)?;
    }

    ctx.db
        .tactical_server_authority()
        .identity()
        .delete(server.identity);

    log::info!(
        "Tactical server for mission '{}' ended: success={success}",
        server.mission_id
    );
    Ok(())
}

#[cfg(test)]
mod authority_tests {
    #[test]
    fn tactical_schema_has_no_quest_keyed_mission_authority() {
        let source = include_str!("tactical.rs");
        for schema in ["TacticalServerRequest", "TacticalServer"] {
            let body = source
                .split(&format!("pub struct {schema}"))
                .nth(1)
                .and_then(|tail| tail.split('}').next())
                .expect("schema body");
            assert!(!body.contains("quest_id"));
            assert!(!body.contains("hostile_group_id"));
            assert!(!body.contains("case_site_id"));
        }
    }

    #[test]
    fn incident_outcomes_are_consumed_before_legacy_quest_projection() {
        let source = include_str!("tactical.rs");
        let completion = source
            .split("fn end_tactical_server_by_instance")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .expect("tactical completion");
        let incident = completion
            .find("finish_incident_for_hostile_group")
            .expect("typed incident completion");
        let quest = completion
            .find("complete_quest")
            .expect("legacy quest projection");
        assert!(incident < quest);
    }

    #[test]
    fn only_success_routes_through_the_shared_victory_commit() {
        let source = include_str!("tactical.rs");
        let end = source
            .split("fn end_tactical_server_by_instance")
            .nth(1)
            .expect("tactical end path");
        let success = end.split("if success").nth(1).expect("success branch");
        assert!(success.contains("commit_victorious_battle("));
        assert!(success.contains("mission_authority()"));
        assert!(!source.contains("record_battle_result("));
    }

    #[test]
    fn gateway_entry_points_require_character_authority() {
        let source = include_str!("tactical.rs");
        let request = source
            .split("pub fn request_tactical_server(")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("request reducer");
        assert!(request.contains("require_strategic_character_authority(ctx, character_id)?"));
    }

    #[test]
    fn claim_is_gateway_authorized_hashed_and_consumed_once() {
        let source = include_str!("tactical.rs");
        assert!(source.contains("require_strategic_gateway(ctx)?"));
        assert!(source.contains("Sha256::digest(claim.as_bytes())"));
        assert!(source.contains(".tactical_server_claim()"));
        assert!(source.contains(".delete(&mission_id)"));
        assert!(!source.contains("pub claim:"));
    }
}
