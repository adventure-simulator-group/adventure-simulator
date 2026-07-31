use sha2::{Digest, Sha256};
use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use crate::repair::{ItemCondition, item_condition__view};
use crate::{
    Character, CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, Item,
    character::character,
    character__view, character_attributes__view, character_equipped_item__view,
    character_limbs__view, character_skills__view, character_stats__view,
    equipment_occupancy__view, inventory_item__view,
    investigation::case_site_authority,
    item__view, party_authority,
    strategic::{
        autoresolve_report, complete_bound_mission_success, ensure_bound_mission_authority,
        fail_bound_mission_attempt, hostile_group_authority, mission_authority,
        outcome_source_authority, strategic_gateway_authority__view,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum TacticalMissionResolution {
    Failed,
    Defeated,
    DrivenOff,
    Captured,
    CaptureTargetKilled,
}

fn tactical_session_succeeded(reported: TacticalMissionResolution) -> bool {
    match reported {
        TacticalMissionResolution::Failed | TacticalMissionResolution::CaptureTargetKilled => false,
        TacticalMissionResolution::Defeated
        | TacticalMissionResolution::DrivenOff
        | TacticalMissionResolution::Captured => true,
    }
}

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
    pub enemy_difficulty: i32,
    pub enemy_combat_scale_bps: u32,
    pub countermeasure_multiplier_bps: u32,
    pub normalized_combat_power: u32,
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
    pub enemy_difficulty: i32,
    pub enemy_combat_scale_bps: u32,
    pub countermeasure_multiplier_bps: u32,
    pub normalized_combat_power: u32,
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
    pub selected_placement_id: Option<String>,
    pub occupancies: Vec<ConnectedEquipmentOccupancy>,
    pub protected_body_parts: Vec<crate::item::EquipmentBodyPart>,
    pub condition: Option<ItemCondition>,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct ConnectedEquipmentOccupancy {
    pub id: String,
    pub anchor_kind: crate::character::EquipmentAnchorKind,
    pub location: Option<crate::item::EquipmentLocation>,
    pub parent_inventory_item_id: Option<u64>,
    pub attachment_point_id: Option<String>,
    pub channel: crate::item::EquipmentChannel,
    pub order: u16,
    pub requirement_index: u16,
    pub capacity_index: u16,
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
            let worn = ctx
                .db
                .character_equipped_item()
                .inventory_item_id()
                .find(inventory_item.id);
            let placement = worn.as_ref().and_then(|worn| {
                item.equipment_placements
                    .iter()
                    .find(|placement| placement.id == worn.placement_id)
            });
            let occupancies = ctx
                .db
                .equipment_occupancy()
                .inventory_item_id()
                .filter(inventory_item.id)
                .map(|row| ConnectedEquipmentOccupancy {
                    id: row.id,
                    anchor_kind: row.anchor_kind,
                    location: row.location,
                    parent_inventory_item_id: row.parent_inventory_item_id,
                    attachment_point_id: row.attachment_point_id,
                    channel: row.channel,
                    order: row.order,
                    requirement_index: row.requirement_index,
                    capacity_index: row.capacity_index,
                })
                .collect();
            let protected_body_parts = placement
                .map(|placement| placement.protection.clone())
                .unwrap_or_default();
            Some(ConnectedPlayerItem {
                inventory_item_id: inventory_item.id,
                quantity: inventory_item.quantity,
                item,
                selected_placement_id: worn.map(|row| row.placement_id),
                condition,
                occupancies,
                protected_body_parts,
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
    match crate::investigation::case_site_provenance_reducer(ctx, &case_site) {
        Some(None) => {}
        Some(Some(_)) => {
            return Err(
                "Generated quest finales support strategic autoresolve, not tactical entry".into(),
            );
        }
        None => return Err("Case-site combat provenance is invalid or ambiguous".into()),
    }
    let mission = ensure_bound_mission_authority(
        ctx,
        &mission_id,
        &party_id,
        character_id,
        &case_site,
        &scene_key,
    )?;
    if mission.status != crate::strategic::MissionAttemptStatus::Bound {
        return Err("Tactical request requires a newly bound mission attempt".into());
    }
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
    if group.disposition != crate::strategic::HostileGroupDisposition::Active {
        return Err("Hostile group is already resolved".into());
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
            required_enemy_kills: mission.enemy_count,
            enemy_difficulty: mission.enemy_difficulty,
            enemy_combat_scale_bps: mission.enemy_combat_scale_bps,
            countermeasure_multiplier_bps: mission.countermeasure_multiplier_bps,
            normalized_combat_power: mission.normalized_combat_power,
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
        request.enemy_difficulty,
        request.enemy_combat_scale_bps,
        request.countermeasure_multiplier_bps,
        request.normalized_combat_power,
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
    enemy_difficulty: i32,
    enemy_combat_scale_bps: u32,
    countermeasure_multiplier_bps: u32,
    normalized_combat_power: u32,
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
        enemy_difficulty,
        enemy_combat_scale_bps,
        countermeasure_multiplier_bps,
        normalized_combat_power,
    };
    ctx.db.tactical_server_authority().insert(server);
    Ok(())
}

/// End a [`TacticallServer`] associated with the caller.
#[reducer]
pub fn end_tactical_server(
    ctx: &ReducerContext,
    resolution: TacticalMissionResolution,
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
    end_tactical_server_by_instance(ctx, server, resolution)
}

/// End a [`TacticallServer`].
fn end_tactical_server_by_instance(
    ctx: &ReducerContext,
    server: TacticalServer,
    resolution: TacticalMissionResolution,
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
    if tactical_session_succeeded(resolution) {
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
                complete_bound_mission_success(ctx, &server.mission_id)?;
            } else {
                fail_bound_mission_attempt(ctx, &server.mission_id)?;
            }
        } else {
            fail_bound_mission_attempt(ctx, &server.mission_id)?;
        }
    } else {
        fail_bound_mission_attempt(ctx, &server.mission_id)?;
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
        "Tactical server for mission '{}' ended: resolution={resolution:?}",
        server.mission_id,
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
            assert!(!body.contains("expected_resolution"));
            assert!(!body.contains("capture_subject"));
            assert!(!body.contains("outcome_entropy"));
        }
    }

    #[test]
    fn tactical_result_variants_cannot_choose_the_strategic_outcome() {
        use super::TacticalMissionResolution as T;
        assert!(!super::tactical_session_succeeded(T::Failed));
        assert!(
            !super::tactical_session_succeeded(T::CaptureTargetKilled),
            "typed contradictory terminal evidence must fail without sampling"
        );
        for report in [T::Defeated, T::DrivenOff, T::Captured] {
            assert!(super::tactical_session_succeeded(report));
        }
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
    fn tactical_entry_is_manual_only_and_scene_wrapper_cannot_bypass_it() {
        let source = include_str!("tactical.rs");
        let wrapper = source
            .split("pub fn request_tactical_server_for_scene")
            .nth(1)
            .and_then(|tail| tail.split("/// Request a new").next())
            .expect("scene request wrapper");
        assert!(
            wrapper.contains("request_tactical_server(ctx, character_id, mission_id, scene_key)")
        );

        let request = source
            .split("pub fn request_tactical_server(")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("request reducer");
        let provenance = request
            .find("case_site_provenance_reducer")
            .expect("manual-only provenance guard");
        let binding = request
            .find("ensure_bound_mission_authority")
            .expect("mission binding");
        assert!(provenance < binding);
        assert!(request.contains("Some(None) => {}"));
        assert!(request.contains("Some(Some(_))"));
        assert!(request.contains("strategic autoresolve, not tactical entry"));
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

    #[test]
    fn tactical_completion_is_an_opaque_signal_to_strategic_authority() {
        let source = include_str!("tactical.rs");
        let completion = source
            .split("fn end_tactical_server_by_instance")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .expect("tactical completion body");
        assert!(completion.contains("complete_bound_mission_success"));
        assert!(!completion.contains("commit_hostile_battle_resolution"));
        assert!(!completion.contains("HostileResolutionKind"));
        assert!(!completion.contains("capture_subject"));
    }
}
