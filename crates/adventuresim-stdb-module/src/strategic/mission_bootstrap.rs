const MISSION_CANDIDATE_ENTROPY_STRIDE: u64 = 0x9e37_79b9_7f4a_7c15;

#[reducer]
pub fn report_contract(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.active_contract_id.as_deref() != Some(&contract_id) {
        return Err("This is not the party's active quest".into());
    }
    let mut quest = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Quest not found")?;
    quest.parsed_state()?;
    if quest.status != ContractStatus::ReadyToReport
        || quest.accepted_by.as_ref() != Some(&party_id)
    {
        return Err("The quest has not been completed by this party".into());
    }
    if quest.paid_at_minute.is_some() {
        return Err("This contract has already been paid".into());
    }
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Return to the questgiver's settlement to claim the reward".into());
    }
    consume_contract_interaction(
        ctx,
        &contract_id,
        &party_id,
        ContractInteractionStage::Report,
    )?;
    let reported_at_minute = crate::time::refresh_clock(ctx)?;

    let reward = quest.gold_reward.max(0) as u64;
    if reward > 0 {
        credit_party_currency(ctx, &party_id, &quest.settlement_id, reward as u32)?;
        let recipients = living_party_member_ids(ctx, &party_id);
        let recipient_count = recipients.len().max(1) as u64;
        let share = reward / recipient_count;
        for recipient in recipients {
            credit_party_stake(ctx, &party_id, recipient, share)?;
        }
        credit_party_reserve(ctx, &party_id, reward % recipient_count)?;
    }
    let total_xp = quest.xp_reward.max(0) as u32;
    let members = living_party_member_ids(ctx, &party_id);
    let xp_per_member = total_xp / members.len().max(1) as u32;
    for member_id in members {
        if let Some(mut member) = ctx.db.character().id().find(member_id) {
            member.xp = member.xp.saturating_add(xp_per_member);
            member.level = 1 + member.xp / 100;
            ctx.db.character().id().update(member);
        }
    }
    quest.status = ContractStatus::Paid;
    quest.paid_at_minute = Some(reported_at_minute);
    ctx.db.contract_authority().id().update(quest.clone());

    party.active_contract_id = None;
    ctx.db.party_authority().id().update(party);
    let obsolete_requests: Vec<u64> = ctx
        .db
        .party_action_request_authority()
        .party_id()
        .filter(&party_id)
        .filter(|request| request.action_kind == "report_contract")
        .map(|request| request.id)
        .collect();
    for request_id in obsolete_requests {
        ctx.db
            .party_action_request_authority()
            .id()
            .delete(request_id);
    }
    ensure_settlement_activity_inner(ctx, &quest.settlement_id)?;
    Ok(())
}

#[reducer]
pub fn autoresolve_mission(
    ctx: &ReducerContext,
    character_id: u64,
    mission_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id else {
        return Err("Must be in a party".into());
    };
    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can autoresolve".into());
    }
    let case_site = party
        .current_case_site_id
        .as_ref()
        .and_then(|id| ctx.db.case_site_authority().id_key().find(id.to_string()))
        .ok_or("Party is not at a case site")?;
    if crate::investigation::character_case_site_id(ctx, character_id).as_deref()
        != Some(case_site.id.as_str())
    {
        return Err("Character and party case-site occupancy do not agree".into());
    }
    require_party_ready(ctx, &party_id)?;

    let mission = ensure_bound_mission_authority(
        ctx,
        &mission_id,
        &party_id,
        character_id,
        &case_site,
        &case_site.scene_key,
    )?;
    if mission.status != MissionAttemptStatus::Bound {
        return Err("Autoresolve requires a newly bound mission attempt".into());
    }
    let hostile_group_id = mission
        .hostile_group_id
        .as_deref()
        .ok_or("Quest mission must bind a hostile group")?;
    let battle_id = format!("battle:{mission_id}");
    if ctx
        .db
        .autoresolve_report()
        .battle_id()
        .find(&battle_id)
        .is_some()
    {
        return Ok(());
    }
    let hostile_group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(hostile_group_id.to_string())
        .ok_or("Hostile group not found")?;
    if hostile_group.disposition != HostileGroupDisposition::Active {
        return Err("Hostile group is already resolved".into());
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
                .is_some_and(|authority| {
                    authority.hostile_group_id.as_deref() == Some(hostile_group_id)
                })
        })
        || ctx.db.tactical_server_authority().iter().any(|server| {
            ctx.db
                .mission_authority()
                .id()
                .find(&server.mission_id)
                .is_some_and(|authority| {
                    authority.hostile_group_id.as_deref() == Some(hostile_group_id)
                })
        })
    {
        return Err("Hostile group already has a tactical resolution in progress".into());
    }

    let member_ids = living_party_member_ids(ctx, &party_id);
    let allies = member_ids
        .iter()
        .map(|member_id| {
            let condition =
                crate::condition::refresh_character_strategic_condition(ctx, *member_id)?;
            crate::capability::load_combatant(
                ctx,
                *member_id,
                condition.incapacitation,
                condition.pain,
                condition.blood_loss,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let enemy_ids = mission.enemy_character_ids.clone();
    if enemy_ids.len() != mission.enemy_count as usize {
        return Err("Mission hostile roster no longer matches its bound snapshot".into());
    }
    let enemies = enemy_ids
        .into_iter()
        .map(|enemy_id| {
            autoresolve_enemy_with_countermeasure(
                enemy_id,
                &hostile_group.enemy_type,
                mission.enemy_difficulty,
                mission.enemy_combat_scale_bps,
                u32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let seed = ctx.random();
    let opening = if mission.contacted_before_combat {
        BattleOpening::Normal
    } else {
        BattleOpening::AlliesSurprise
    };
    let outcome = resolve_battle(allies, enemies, seed, opening);
    let morale_difficulty = mission.enemy_difficulty.max(
        mission
            .normalized_combat_power
            .div_ceil(adventuresim_core::threat_escalation::BASELINE_ORC_POWER)
            .min(i32::MAX as u32) as i32,
    );
    commit_autoresolve_outcome(
        ctx,
        &battle_id,
        &party_id,
        &member_ids,
        5.0 + morale_difficulty.max(0) as f32,
        &outcome,
    )?;

    if outcome.victor != BattleVictor::Allies {
        fail_bound_mission_attempt(ctx, &mission_id)?;
        return Ok(());
    }

    crate::corpse::persist_autoresolve_enemy_corpses(
        ctx,
        &battle_id,
        &party_id,
        &case_site.origin_settlement_id,
        Some(&case_site.id),
        &hostile_group.enemy_type,
        &outcome,
    )?;
    complete_bound_mission_success(ctx, &mission_id)?;
    Ok(())
}

#[reducer]
pub fn cancel_mission_request(
    ctx: &ReducerContext,
    character_id: u64,
    mission_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
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
        return Err("Only the party leader can cancel a mission request".into());
    }
    let mut mission = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id)
        .ok_or("Mission authority not found")?;
    mission.parsed_state()?;
    if mission.party_id != party_id {
        return Err("Mission request belongs to another party".into());
    }
    match mission.status {
        MissionAttemptStatus::Cancelled => return Ok(()),
        MissionAttemptStatus::Bound => {}
        MissionAttemptStatus::Committed | MissionAttemptStatus::Failed => {
            return Err("Mission request is already terminal".into());
        }
    }
    let request = ctx
        .db
        .tactical_server_request_authority()
        .mission_id()
        .find(&mission_id)
        .ok_or("Mission request not found")?;
    if request.party_id != party_id {
        return Err("Mission request belongs to another party".into());
    }
    ctx.db
        .tactical_server_request_authority()
        .mission_id()
        .delete(&mission_id);
    ctx.db
        .tactical_server_claim()
        .mission_id()
        .delete(&mission_id);
    mission.status = MissionAttemptStatus::Cancelled;
    mission.parsed_state()?;
    ctx.db.mission_authority().id().update(mission);
    Ok(())
}

/// Seed the local demonstration world only when this module binary was built
/// with a matching high-entropy development capability. Normal builds contain
/// no usable token, so ordinary database and tactical identities cannot seed.
#[reducer]
pub fn bootstrap_development_world(
    ctx: &ReducerContext,
    bootstrap_token: String,
) -> Result<(), String> {
    if !adventuresim_core::simulation_security::simulation_bootstrap_authorized(
        COMPILED_DEV_BOOTSTRAP_TOKEN,
        &bootstrap_token,
    ) {
        return Err("Development bootstrap is disabled or unauthorized".into());
    }
    seed_world(ctx, true)?;
    crate::disease::seed_sick_character(ctx)?;
    crate::character::seed_damaged_character(ctx)?;
    crate::character::seed_religion_scholar_character(ctx)?;
    crate::character::seed_bestiary_scholar_character(ctx)?;
    crate::social::seed_social_demo(ctx)?;
    crate::character::seed_herbalism_demo_character(ctx)?;
    materialize_development_scenario_gallery(ctx)?;
    Ok(())
}

/// Reject a dev-bootstrap call whose token does not match the compiled
/// capability. Shared by every split bootstrap reducer so the granular seed
/// path is gated exactly like [`bootstrap_development_world`].
fn require_dev_bootstrap_token(bootstrap_token: &str) -> Result<(), String> {
    if !adventuresim_core::simulation_security::simulation_bootstrap_authorized(
        COMPILED_DEV_BOOTSTRAP_TOKEN,
        bootstrap_token,
    ) {
        return Err("Development bootstrap is disabled or unauthorized".into());
    }
    Ok(())
}

/// Settlement ids in a stable, id-sorted order so a caller can drive
/// per-settlement seeding by index across separate transactions.
fn sorted_settlement_ids(ctx: &ReducerContext) -> Vec<String> {
    let mut ids: Vec<String> = ctx.db.settlement().iter().map(|s| s.id).collect();
    ids.sort();
    ids
}

/// First split of [`bootstrap_development_world`]: seed geography, settlements,
/// and the errantry demo chapter, WITHOUT materializing per-settlement
/// strategic activity (that expensive work is done by
/// [`dev_bootstrap_settlement_activity`]). Kept under the per-reducer compute
/// budget so it can run as its own transaction.
#[reducer]
pub fn dev_bootstrap_base(ctx: &ReducerContext, bootstrap_token: String) -> Result<(), String> {
    require_dev_bootstrap_token(&bootstrap_token)?;
    seed_world_rows(ctx, true, false)?;
    Ok(())
}

/// Second split of [`bootstrap_development_world`]: materialize strategic
/// activity for a single settlement, addressed by its index into the
/// id-sorted settlement list. Out-of-range indices are a no-op so the caller
/// can loop `0..count` without racing the exact settlement total. Each call is
/// its own transaction, keeping the heavy per-settlement work under budget.
///
/// `quest_batch` caps how many NEW quests are generated on this call (0 means
/// "no cap"). The cheap idempotent ensures (smith, residence, population,
/// incidents, recruiting) run every call; passing a small `quest_batch` and
/// repeating the call lets a settlement whose full quest generation would
/// exceed the compute budget be filled across several transactions.
#[reducer]
pub fn dev_bootstrap_settlement_activity(
    ctx: &ReducerContext,
    bootstrap_token: String,
    index: u32,
) -> Result<(), String> {
    require_dev_bootstrap_token(&bootstrap_token)?;
    let ids = sorted_settlement_ids(ctx);
    let Some(settlement_id) = ids.get(index as usize) else {
        return Ok(());
    };
    // One settlement's full activity is well under the per-reducer budget, so a
    // single call materializes all of its quests (`quest_batch == 0` = no cap);
    // this avoids re-running the idempotent population/incident work per quest.
    ensure_settlement_activity_batched(ctx, settlement_id, 0)?;
    crate::repair::ensure_settlement_smith(ctx, settlement_id);
    Ok(())
}

/// The number of discrete finalize steps [`dev_bootstrap_finalize`] exposes;
/// a caller drives `0..DEV_BOOTSTRAP_FINALIZE_STEPS` to complete the world.
pub const DEV_BOOTSTRAP_FINALIZE_STEPS: u32 = 7;

/// Final split of [`bootstrap_development_world`]: everything after the world
/// and its strategic activity - the demo characters, social demo, and the
/// development scenario gallery. Each `step` runs one sub-seed as its own
/// transaction so no single call exceeds the per-reducer compute budget; the
/// union of steps `0..DEV_BOOTSTRAP_FINALIZE_STEPS` reproduces the tail of
/// `bootstrap_development_world` exactly. Every sub-seed is idempotent, and an
/// out-of-range `step` is a no-op so the caller can loop without racing.
#[reducer]
pub fn dev_bootstrap_finalize(
    ctx: &ReducerContext,
    bootstrap_token: String,
) -> Result<(), String> {
    require_dev_bootstrap_token(&bootstrap_token)?;
    // The demo characters are individually cheap, so one call seeds them all.
    // The development scenario gallery stays separate (via `dev_bootstrap_gallery`
    // / `dev_bootstrap_gallery_validate`) because it exceeds the budget alone.
    crate::disease::seed_sick_character(ctx)?;
    crate::character::seed_damaged_character(ctx)?;
    crate::character::seed_religion_scholar_character(ctx)?;
    crate::character::seed_bestiary_scholar_character(ctx)?;
    crate::social::seed_social_demo(ctx)?;
    crate::character::seed_herbalism_demo_character(ctx)?;
    Ok(())
}

/// Materialize one development-gallery item (see `materialize_gallery_item`).
/// The whole gallery is too heavy for a single reducer transaction, so callers
/// invoke this with `index` 0, 1, 2, ... until a call materializes nothing new,
/// then run `dev_bootstrap_gallery_validate` once. Idempotent per item.
#[reducer]
pub fn dev_bootstrap_gallery(
    ctx: &ReducerContext,
    bootstrap_token: String,
    offset: u32,
    count: u32,
) -> Result<(), String> {
    require_dev_bootstrap_token(&bootstrap_token)?;
    // Materialize a contiguous batch of gallery items in one transaction (a
    // batch of several stays well under the budget); stop early once past the
    // end so an over-long final batch is harmless.
    let start = offset as usize;
    for index in start..start.saturating_add(count as usize) {
        if !materialize_gallery_item(ctx, index)? {
            break;
        }
    }
    Ok(())
}

/// Run the gallery postcondition check once every item has been materialized by
/// `dev_bootstrap_gallery`. Fails if any scenario lacks its primary character.
#[reducer]
pub fn dev_bootstrap_gallery_validate(
    ctx: &ReducerContext,
    bootstrap_token: String,
) -> Result<(), String> {
    require_dev_bootstrap_token(&bootstrap_token)?;
    validate_gallery_postcondition(ctx)
}

/// Seed a standalone tactical mission: a solo party occupying a typed case
/// site with a bound hostile group, mission, tactical-server request, and
/// its authorized claim.
///
/// Lets an isolated, strategic-layer-free SpacetimeDB instance host a
/// standalone tactical server/client test without hand-authoring party and
/// quest state, or running the trusted dispatcher to authorize a claim.
/// `tactical_claim` is the plaintext one-use secret the caller will later
/// launch the tactical server with (as `ADVENTURESIM_TACTICAL_CLAIM`); only
/// its hash is stored, mirroring [`crate::tactical::authorize_tactical_server_claim`].
/// Gated by the same development capability as [`bootstrap_development_world`].
fn configure_animation_lab_enemies(
    ctx: &ReducerContext,
    hostile_group_id: &str,
) -> Result<(), String> {
    use adventuresim_core::starting_character::StartingSlot;
    use adventuresim_core::tactical_fixture::AnimationLabEnemyRole;

    const PADDED_BASE: &[(&str, StartingSlot)] = &[
        ("quilted_sleeve", StartingSlot::LeftArm),
        ("quilted_sleeve", StartingSlot::RightArm),
        ("arming_cap", StartingSlot::Head),
        ("arming_doublet", StartingSlot::Chest),
        ("padded_skirt", StartingSlot::Stomach),
        ("padded_chausses", StartingSlot::LeftLeg),
        ("padded_chausses", StartingSlot::RightLeg),
    ];

    let enemy_ids = crate::world_actor::context_character_ids(ctx, hostile_group_id);
    if enemy_ids.len() != AnimationLabEnemyRole::ALL.len() {
        return Err(format!(
            "Animation lab requires exactly {} enemies, got {}",
            AnimationLabEnemyRole::ALL.len(),
            enemy_ids.len()
        ));
    }
    for (character_id, role) in enemy_ids.into_iter().zip(AnimationLabEnemyRole::ALL) {
        let mut character = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or("Animation lab enemy disappeared")?;
        character.name = role.name().into();
        ctx.db.character().id().update(character);

        let mut loadout = PADDED_BASE.to_vec();
        match role {
            AnimationLabEnemyRole::Passive | AnimationLabEnemyRole::Dodger => {}
            AnimationLabEnemyRole::ShieldBlocker => {
                loadout.push(("buckler", StartingSlot::LeftHand));
            }
            AnimationLabEnemyRole::DemiLancer => {
                loadout.extend([
                    ("morion", StartingSlot::Head),
                    ("cuirass", StartingSlot::Chest),
                    ("tassets", StartingSlot::Stomach),
                    ("vambrace", StartingSlot::LeftArm),
                    ("vambrace", StartingSlot::RightArm),
                ]);
            }
        }
        crate::character::replace_development_loadout(ctx, character_id, &loadout)?;
        crate::character::add_and_equip_basic_clothing(ctx, character_id)?;
    }
    Ok(())
}

/// Gives the native diagnostic the same two-handed weapon path that its
/// scripted attack is intended to validate. The standalone database is
/// disposable, so this may replace the seeded character's equipped roots
/// without affecting strategic state or player data.
fn configure_diagnostic_player(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    use adventuresim_core::starting_character::StartingSlot;

    const LONGSWORD_LOADOUT: &[(&str, StartingSlot)] = &[
        ("linen_tunic", StartingSlot::Chest),
        ("linen_breeches", StartingSlot::LeftLeg),
        ("leather_boot", StartingSlot::LeftFoot),
        ("leather_boot", StartingSlot::RightFoot),
        ("morion", StartingSlot::Head),
        ("breastplate", StartingSlot::Chest),
        ("vambrace", StartingSlot::LeftArm),
        ("vambrace", StartingSlot::RightArm),
        ("longsword", StartingSlot::RightHand),
    ];

    crate::character::replace_development_loadout(ctx, character_id, LONGSWORD_LOADOUT)
}

#[reducer]
pub fn seed_standalone_tactical_mission(
    ctx: &ReducerContext,
    bootstrap_token: String,
    character_id: u64,
    mission_id: String,
    scene_key: String,
    required_enemy_kills: u32,
    tactical_claim: String,
) -> Result<(), String> {
    if !adventuresim_core::simulation_security::simulation_bootstrap_authorized(
        COMPILED_DEV_BOOTSTRAP_TOKEN,
        &bootstrap_token,
    ) {
        return Err("Development bootstrap is disabled or unauthorized".into());
    }
    if ctx
        .db
        .tactical_server_request_authority()
        .mission_id()
        .find(&mission_id)
        .is_some()
        || ctx
            .db
            .tactical_server_authority()
            .mission_id()
            .find(&mission_id)
            .is_some()
    {
        return Ok(());
    }
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
    let standalone_mission_family = standalone_mission_family(&mission_id)?;

    if !ctx
        .db
        .settlement()
        .iter()
        .any(|settlement| settlement.scene_key == scene_key)
    {
        seed_tactical_host_world(ctx)?;
    }
    if ctx.db.character().id().find(character_id).is_none() {
        let default =
            adventuresim_core::starting_character::default_character_with_id(character_id);
        crate::character::insert_starting_character(ctx, &default)?;
    }
    if standalone_mission_family == StandaloneMissionFamily::Diagnostic {
        configure_diagnostic_player(ctx, character_id)?;
    }
    let party_id = create_solo_party_for_character(ctx, character_id)?;
    crate::tactical::retire_interrupted_standalone_server_for_character(ctx, character_id)?;
    retire_interrupted_standalone_requests(ctx, character_id, &party_id, &mission_id)?;

    let settlement = ctx
        .db
        .settlement()
        .iter()
        .find(|settlement| settlement.scene_key == scene_key)
        .ok_or_else(|| {
            format!("No settlement with scene_key '{scene_key}' to host a debug mission")
        })?;
    let case_site_id = CaseSiteId::from(format!("case-site:standalone:{mission_id}"));
    let case_site = if let Some(existing) = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(case_site_id.to_string())
    {
        existing
    } else {
        let coordinates_are_geographic = settlement.source_node_id.is_some();
        let coordinate = encode_position_e7(
            settlement.coord_x,
            settlement.coord_y,
            coordinates_are_geographic,
        )
        .ok_or("Standalone tactical site is not a valid WGS84 coordinate")?;
        ctx.db.case_site_authority().insert(CaseSiteAuthority {
            id_key: case_site_id.as_str().to_owned(),
            id: case_site_id.clone(),
            case_id: standalone_case_id(&mission_id),
            origin_settlement_id: settlement.id.clone(),
            name: "Standalone Tactical Test".into(),
            description: "Seeded for isolated tactical testing".into(),
            scene_key: scene_key.clone(),
            longitude_e7: coordinate.longitude_e7,
            latitude_e7: coordinate.latitude_e7,
            coordinates_are_geographic,
            distance_m: 0,
        })
    };
    let hostile_group_id = format!("hostile-group:standalone:{mission_id}");
    if ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&hostile_group_id)
        .is_none()
    {
        ctx.db
            .hostile_group_authority()
            .insert(HostileGroupAuthority {
                id: hostile_group_id.clone(),
                case_site_id_key: case_site.id.as_str().to_owned(),
                case_site_id: case_site.id.clone(),
                enemy_type: "bandit".into(),
                base_enemy_count: required_enemy_kills
                    .clamp(1, adventuresim_core::threat_escalation::MAX_MOB_COUNT),
                base_difficulty: 1,
                baseline_enemy_power: 10_000,
                enemy_count: required_enemy_kills
                    .clamp(1, adventuresim_core::threat_escalation::MAX_MOB_COUNT),
                difficulty: 1,
                escalation_incident_ordinal: 1,
                escalation_progress_bps: 0,
                combat_scale_bps: u32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE),
                normalized_combat_power: required_enemy_kills
                    .clamp(1, adventuresim_core::threat_escalation::MAX_MOB_COUNT)
                    .saturating_mul(10_000),
                drop_item_id: autoresolve_drop("bandit")?.map(str::to_string),
                drop_quantity: required_enemy_kills,
                disposition: HostileGroupDisposition::Active,
            });
    }
    let case_id = standalone_case_id(&mission_id);
    let group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&hostile_group_id)
        .ok_or("Standalone hostile group disappeared")?;
    crate::world_actor::materialize_context_roster(
        ctx,
        crate::world_actor::CharacterContextKind::HostileGroup,
        &group.id,
        group.case_site_id.as_str(),
        &group.enemy_type,
        group.enemy_count,
    )?;
    if standalone_mission_family == StandaloneMissionFamily::Animation {
        configure_animation_lab_enemies(ctx, &hostile_group_id)?;
    }
    let objective_id = format!("objective:standalone:{mission_id}");
    if ctx.db.case_authority().id().find(&case_id).is_none() {
        use adventuresim_core::case::{
            Objective, ObjectiveExpression, ObjectiveId, ObjectivePath, ObjectiveRequirement,
        };
        let expression = ObjectiveExpression {
            alternatives: vec![ObjectivePath {
                objectives: vec![Objective {
                    id: ObjectiveId::new(objective_id.clone())
                        .map_err(|_| "Standalone tactical objective ID is invalid".to_string())?,
                    requirement: ObjectiveRequirement::Defeat {
                        hostile_group_id: hostile_group_id.clone(),
                        count: required_enemy_kills.max(1),
                    },
                }],
            }],
        };
        ctx.db.case_authority().insert(CaseAuthority {
            id: case_id.clone(),
            provenance_kind: InvestigationProvenanceKind::Manual,
            generated_case_id: String::new(),
            investigation_case_id: format!("investigation:standalone:{mission_id}"),
            local_problem_id: None,
            objective_expression_json: serde_json::to_string(&expression)
                .map_err(|_| "Standalone tactical objective serialization failed")?,
            resolution_status: CaseStatus::Open,
            resolved_by_party_id: None,
        });
    }

    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    party.current_settlement_id = None;
    party.current_case_site_id = Some(case_site.id.clone());
    if party.wilderness_canonical_anchor_minute.is_none() {
        party.wilderness_canonical_anchor_minute = Some(crate::time::refresh_clock(ctx)?);
        party.wilderness_elapsed_minutes = 0;
    }
    ctx.db.party_authority().id().update(party.clone());
    crate::investigation::set_character_case_site(
        ctx,
        character_id,
        Some(case_site.id.as_str().to_owned()),
    )?;
    let capability_id = format!("mission-approach:standalone:{mission_id}");
    if ctx
        .db
        .mission_approach_capability()
        .id()
        .find(&capability_id)
        .is_none()
    {
        ctx.db
            .mission_approach_capability()
            .insert(MissionApproachCapability {
                id: capability_id.clone(),
                observer_character_id: character_id,
                hostile_group_id: hostile_group_id.clone(),
                case_id: case_id.clone(),
                case_site_id: case_site.id.clone(),
                path_index: 0,
                objective_id: objective_id.clone(),
                resolution: HostileResolutionKind::Defeated,
                weight: 1,
                capture_subject_id: None,
                capture_custody_version: None,
                active: true,
            });
    }
    let mission = if let Some(existing) = ctx.db.mission_authority().id().find(&mission_id) {
        existing
    } else {
        let enemy_combat_scale_bps = group.combat_scale_bps;
        let normalized_combat_power = group.normalized_combat_power;
        ctx.db.mission_authority().insert(MissionAuthority {
            id: mission_id.clone(),
            party_id: party_id.clone(),
            case_site_id: Some(case_site.id.clone()),
            hostile_group_id: Some(hostile_group_id.clone()),
            observer_character_id: character_id,
            case_id: case_id.clone(),
            outcome_entropy: ctx.random(),
            status: MissionAttemptStatus::Bound,
            committed_resolution: None,
            committed_capture_subject_id: None,
            committed_capture_custody_version: None,
            scene_key: scene_key.clone(),
            hostile_version: group.escalation_incident_ordinal,
            enemy_count: group.enemy_count,
            enemy_character_ids: crate::world_actor::context_character_ids(ctx, &hostile_group_id),
            contacted_before_combat: crate::world_actor::party_contacted_context(
                ctx,
                &party_id,
                &hostile_group_id,
            ),
            enemy_difficulty: group.base_difficulty,
            enemy_combat_scale_bps,
            normalized_combat_power,
            drop_item_id: group.drop_item_id.clone(),
            drop_quantity: group.drop_quantity,
        })
    };
    if mission.hostile_group_id.as_deref() != Some(&hostile_group_id) {
        return Err("Standalone mission resolved to an unexpected hostile group".into());
    }
    if ctx
        .db
        .mission_outcome_candidate()
        .mission_id()
        .filter(&mission_id)
        .next()
        .is_none()
    {
        ctx.db
            .mission_outcome_candidate()
            .insert(MissionOutcomeCandidate {
                id: format!("{mission_id}:candidate:000"),
                mission_id: mission_id.clone(),
                capability_id,
                case_id,
                case_site_id: case_site.id,
                hostile_group_id,
                path_index: 0,
                objective_id,
                resolution: HostileResolutionKind::Defeated,
                weight: 1,
                capture_subject_id: None,
                capture_custody_version: None,
            });
    }
    let authorized_party_member_ids = crate::strategic::living_party_member_ids(ctx, &party_id);
    let expected_party_members = u32::try_from(authorized_party_member_ids.len())
        .map_err(|_| "Party is too large for tactical enrollment")?;
    if expected_party_members == 0 {
        return Err("A tactical mission requires at least one living party member".into());
    }
    if expected_party_members as usize
        > adventuresim_core::mission::MAX_TACTICAL_RECEIPT_PARTICIPANTS
    {
        return Err("Party exceeds the tactical receipt participant limit".into());
    }
    ctx.db
        .tactical_server_request_authority()
        .insert(crate::tactical::TacticalServerRequest {
            mission_id: mission_id.clone(),
            gateway_bucket: 0,
            scene_key,
            party_id,
            requested_by: character_id,
            longitude_e7: case_site.longitude_e7,
            latitude_e7: case_site.latitude_e7,
            absolute_minute: party_wilderness_environment_minutes(&party)
                .map_or(adventuresim_core::strategic_time::WORLD_START_MINUTE, |value| value.0),
            lunar_phase_minute: party_wilderness_environment_minutes(&party)
                .map_or(adventuresim_core::strategic_time::WORLD_START_MINUTE, |value| value.1),
            expected_party_members,
            authorized_party_member_ids,
            required_enemy_kills,
            enemy_difficulty: mission.enemy_difficulty,
            enemy_combat_scale_bps: mission.enemy_combat_scale_bps,
            countermeasure_multiplier_bps: u32::from(
                adventuresim_world_schema::BASIS_POINTS_PER_WHOLE,
            ),
            normalized_combat_power: mission.normalized_combat_power,
            enemy_character_ids: mission.enemy_character_ids.clone(),
            party_has_surprise: !mission.contacted_before_combat,
        });
    ctx.db
        .tactical_server_claim()
        .insert(crate::tactical::TacticalServerClaim {
            mission_id,
            claim_hash: Sha256::digest(tactical_claim.as_bytes()).to_vec(),
        });
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum StandaloneMissionFamily {
    Animation,
    Diagnostic,
    General,
}

fn standalone_mission_family(mission_id: &str) -> Result<StandaloneMissionFamily, String> {
    let (domain, coordinate) = mission_id
        .split_once(':')
        .ok_or("Standalone mission ID has no domain")?;
    if domain != "mission" || coordinate.is_empty() {
        return Err("Standalone mission ID has an invalid domain coordinate".into());
    }
    if let Some(animation_coordinate) = coordinate.strip_prefix("animation-") {
        if animation_coordinate.is_empty() {
            return Err("Animation mission ID has no coordinate".into());
        }
        Ok(StandaloneMissionFamily::Animation)
    } else if let Some(diagnostic_coordinate) = coordinate.strip_prefix("diagnostic-") {
        if diagnostic_coordinate.is_empty() {
            return Err("Diagnostic mission ID has no coordinate".into());
        }
        Ok(StandaloneMissionFamily::Diagnostic)
    } else {
        Ok(StandaloneMissionFamily::General)
    }
}

fn standalone_case_id(mission_id: &str) -> String {
    format!("case:standalone:{mission_id}")
}

fn retire_interrupted_standalone_requests(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    next_mission_id: &str,
) -> Result<(), String> {
    let requests: Vec<_> = ctx
        .db
        .tactical_server_request_authority()
        .iter()
        .filter(|request| {
            request.requested_by == character_id
                && request.party_id == party_id
                && request.mission_id != next_mission_id
        })
        .collect();
    for request in requests {
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&request.mission_id)
            .ok_or("Interrupted tactical request has no mission authority")?;
        if mission.case_id != standalone_case_id(&mission.id) {
            return Err("Refusing to retire a non-standalone tactical request".into());
        }
        ctx.db
            .tactical_server_request_authority()
            .mission_id()
            .delete(&request.mission_id);
        ctx.db
            .tactical_server_claim()
            .mission_id()
            .delete(&request.mission_id);
        fail_bound_mission_attempt(ctx, &request.mission_id)?;
    }
    Ok(())
}

pub(crate) fn seed_world(
    ctx: &ReducerContext,
    include_errantry_demo_chapter: bool,
) -> Result<(), String> {
    seed_world_rows(ctx, include_errantry_demo_chapter, true)
}

fn seed_tactical_host_world(ctx: &ReducerContext) -> Result<(), String> {
    seed_world_rows(ctx, false, false)
}

fn seed_world_rows(
    ctx: &ReducerContext,
    include_errantry_demo_chapter: bool,
    materialize_strategic_activity: bool,
) -> Result<(), String> {
    const DEMO_SOURCES: &str = "- **Fabelgeist renderer demo:** Hand-authored geographic fixture for exercising map and terrain-routing UI.";

    for (id, latitude, longitude) in [
        (RIVERDALE_RENDERER_DEMO_NODE, 53.50, 10.00),
        (IRONFORGE_RENDERER_DEMO_NODE, 53.62, 10.20),
    ] {
        if ctx.db.world_node().id().find(id).is_none() {
            ctx.db.world_node().insert(WorldNode {
                id,
                parent_node_id: None,
                latitude,
                longitude,
                is_settlement: true,
                is_town: true,
                is_ferry: false,
                is_harbour: false,
                sources: DEMO_SOURCES.into(),
            });
        }
    }
    if ctx.db.travel_edge().id().find(RENDERER_DEMO_EDGE).is_none() {
        ctx.db.travel_edge().insert(TravelEdge {
            id: RENDERER_DEMO_EDGE,
            from_node_id: RIVERDALE_RENDERER_DEMO_NODE,
            to_node_id: IRONFORGE_RENDERER_DEMO_NODE,
            route: TravelRoute::Land(LandRoute {
                bridge: None,
                water_crossings: vec![],
            }),
            provenance: TravelEdgeProvenance::DocumentedViabundus,
            toll_at: None,
            length_m: 19_000,
            slope_multiplier: 1.0,
            terrain: RouteTerrain::stage_placeholder(),
            certainty: 1,
            section: "renderer-demo".into(),
            sources: DEMO_SOURCES.into(),
        });
    }

    let mut settlements = vec![
        (
            "riverdale",
            "Riverdale",
            10.00,
            53.50,
            Some(RIVERDALE_RENDERER_DEMO_NODE),
            3,
            "woodland",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::RomanCatholic,
            },
        ),
        (
            "ironforge",
            "Ironforge",
            10.20,
            53.62,
            Some(IRONFORGE_RENDERER_DEMO_NODE),
            4,
            "desert",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::Reformed,
            },
        ),
        (
            "willowmere",
            "Willowmere",
            -50.0,
            75.0,
            None,
            2,
            "hills",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::EasternOrthodox,
            },
        ),
    ];
    if include_errantry_demo_chapter {
        let errantry_chapter_settlement_id =
            adventuresim_core::organization::organization(ERRANTRY_ISSUER_ORGANIZATION_ID)
                .filter(|organization| organization.errantry_issuance)
                .and_then(|organization| organization.chapters.first())
                .map(|chapter| chapter.settlement_id.as_str())
                .ok_or("Order errantry demo has no canonical authored chapter")?;
        settlements.push((
            errantry_chapter_settlement_id,
            "St. George Chapter (Development Demo)",
            10.10,
            53.55,
            None,
            3,
            "hills",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::RomanCatholic,
            },
        ));
    }

    for (id, name, x, y, source_node_id, pop, scene, religious_status) in settlements {
        if ctx.db.settlement().id().find(id.to_string()).is_none() {
            let languages = match id {
                "oakenshire" => adventuresim_world_schema::SettlementLanguageProfile {
                    east_central_bp: 1_500,
                    west_central_bp: 7_500,
                    low_bp: 1_000,
                    yiddish_incidence_bp: 75,
                },
                "ravenmoor" => adventuresim_world_schema::SettlementLanguageProfile {
                    east_central_bp: 7_500,
                    west_central_bp: 1_500,
                    low_bp: 1_000,
                    yiddish_incidence_bp: 75,
                },
                _ => adventuresim_world_schema::SettlementLanguageProfile {
                    east_central_bp: 1_000,
                    west_central_bp: 1_000,
                    low_bp: 8_000,
                    yiddish_incidence_bp: 75,
                },
            };
            ctx.db.settlement().insert(Settlement {
                id: id.into(),
                name: name.into(),
                coord_x: x,
                coord_y: y,
                population_level: pop,
                population_estimate: 0,
                category: settlement_category(0, pop),
                elevation: ElevationMeters::new(100).unwrap(),
                land_use: LandUseProfile::new(
                    LandUseFraction::new(2_500).unwrap(),
                    LandUseFraction::new(2_000).unwrap(),
                    LandUseFraction::new(100).unwrap(),
                    LandUseFraction::new(5_400).unwrap(),
                )
                .unwrap(),
                forest_cover: ForestCover::Wooded(Woodland {
                    density: CanopyDensity::new(35).unwrap(),
                    dominant: DominantLeafType::Mixed,
                }),
                potential_vegetation: PotentialVegetation::Inferred(
                    PotentialVegetationClass::WoodlandAndForest,
                ),
                historical_vegetation: HistoricalVegetation::Fallback(adventuresim_world_schema::FallbackHistoricalVegetation {
                    cover: adventuresim_world_schema::FallbackHistoricalVegetationCover::Woodland(adventuresim_world_schema::HistoricalWoodland {
                        canopy: CanopyDensity::new(35).unwrap(),
                        dominant: DominantLeafType::Mixed,
                    }),
                    method: adventuresim_world_schema::FallbackHistoricalVegetationMethod::PotentialEnvelopeV4,
                }),
                tree_species: TreeSpeciesProfile::Inferred(
                    InferredTreeSpeciesProfile::new(vec![
                        TreeSpeciesId::new("Quercus_robur").unwrap(),
                    ])
                    .unwrap(),
                ),
                soil: SoilProfile {
                    wrb_group: adventuresim_world_schema::WrbReferenceGroup::Cambisol,
                    parent_material: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                    properties: SoilProperties {
                    substrate: SoilSubstrate::Mineral(MineralSoil {
                        texture: MineralSoilTexture::Medium,
                        depth: SoilDepth::Deep,
                        available_water: AvailableWaterCapacity::Medium,
                        organic_carbon: TopsoilOrganicCarbon::Medium,
                        stones: StoneContentPercent::new(10).unwrap(),
                    }),
                    water_regime: SoilWaterRegime::SeasonallyWet,
                    agricultural_limitation: AgriculturalLimitation::None,
                    },
                    acidity: SoilAcidity::Acid,
                    cation_exchange_capacity: CationExchangeCapacity::Medium,
                    fertility: SoilFertility::Medium,
                    confidence: SoilBasisPoints::new(2_500).unwrap(),
                    evidence: SoilEvidence::DeterministicInference,
                },
                geology: SurfaceGeology::Inferred(InferredGeologicSetting {
                    lithology: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                    age: GeologicEra::Quaternary,
                }),
                religious_status,
                languages,
                drought: DroughtProfile::Inferred(
                    DroughtHistory::new(
                        PalmerDroughtSeverityIndex::new(0).unwrap(),
                        PalmerDroughtSeverityIndex::new(0).unwrap(),
                        0,
                        0,
                    )
                    .unwrap(),
                ),
                hydrology: SettlementHydrology::default(),
                industries: InferredIndustryProfile::new(vec![
                    adventuresim_world_schema::IndustryEvidence::Fallback(
                        adventuresim_world_schema::FallbackIndustry::WoodlandFuelwood,
                    ),
                ]).unwrap(),
                economy: {
                    let mut economy = SettlementEconomyProfile::stage_placeholder();
                    if id == "ironforge" {
                        economy
                            .services
                            .push(adventuresim_world_schema::SettlementService::Weaponsmith);
                    }
                    economy
                },
                scene_key: scene.into(),
                religion_id: religious_status.church().religion_id().into(),
                currency_id: crate::item::settlement_currency_id(id).into(),
                source_node_id,
                sources: "- **Fabelgeist demo data:** Hand-authored settlement and deterministic placeholder environment; no external world-data source was imported.".into(),
            });
        }
    }

    if !materialize_strategic_activity {
        return Ok(());
    }

    let settlement_ids: Vec<_> = ctx
        .db
        .settlement()
        .iter()
        .map(|settlement| settlement.id)
        .collect();
    for settlement_id in settlement_ids {
        ensure_settlement_activity_inner(ctx, &settlement_id)?;
        crate::repair::ensure_settlement_smith(ctx, &settlement_id);
    }

    Ok(())
}

#[reducer]
pub fn ensure_settlement_activity(
    ctx: &ReducerContext,
    settlement_id: String,
) -> Result<(), String> {
    ensure_settlement_activity_inner(ctx, &settlement_id)
}

fn settlement_activity_target(settlement_id: &str) -> usize {
    MIN_QUESTS_PER_SETTLEMENT
        + settlement_id.bytes().map(usize::from).sum::<usize>()
            % (MAX_QUESTS_PER_SETTLEMENT - MIN_QUESTS_PER_SETTLEMENT + 1)
}

fn settlement_activity_stage_error(
    settlement_id: &str,
    stage: &str,
    error: impl std::fmt::Display,
) -> String {
    format!("Settlement activity for {settlement_id} failed during {stage}: {error}")
}

fn ensure_settlement_activity_inner(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    ensure_settlement_activity_batched(ctx, settlement_id, 0)
}

/// Like [`ensure_settlement_activity_inner`], but generates at most
/// `quest_batch` new quests when `quest_batch > 0` (0 means "no cap", the
/// behavior [`ensure_settlement_activity_inner`] relies on). The cheap
/// idempotent ensures run unconditionally; only quest generation is throttled,
/// letting a heavy settlement be filled across several transactions while the
/// `active..target` loop still converges on the same final quest count.
fn ensure_settlement_activity_batched(
    ctx: &ReducerContext,
    settlement_id: &str,
    quest_batch: u32,
) -> Result<(), String> {
    // World import writes only canonical settlement facts. These derived
    // service rows are instead materialized when settlement activity is used.
    crate::repair::ensure_settlement_smith(ctx, settlement_id);
    crate::residence::ensure_settlement_residence_offers(ctx, settlement_id).map_err(|error| {
        settlement_activity_stage_error(settlement_id, "residence offers", error)
    })?;
    crate::settlement_population::ensure_settlement_population(ctx, settlement_id).map_err(
        |error| settlement_activity_stage_error(settlement_id, "settlement population", error),
    )?;
    let official_minute = crate::time::refresh_clock(ctx).map_err(|error| {
        settlement_activity_stage_error(settlement_id, "official clock refresh", error)
    })?;
    for mut contract in ctx
        .db
        .contract_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|contract| {
            matches!(
                contract.status,
                ContractStatus::Offered | ContractStatus::Accepted
            ) && ctx
                .db
                .case_authority()
                .id()
                .find(&contract.case_id)
                .is_some_and(|case| case.resolution_status != CaseStatus::Open)
        })
        .collect::<Vec<_>>()
    {
        contract.status = ContractStatus::Withdrawn;
        ctx.db.contract_authority().id().update(contract);
    }
    let tracked_quests: HashSet<String> = ctx
        .db
        .party_authority()
        .iter()
        .filter_map(|party| party.active_contract_id)
        .collect();
    let active_contracts = ctx
        .db
        .contract_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|quest| {
            matches!(
                quest.status,
                ContractStatus::Offered | ContractStatus::Accepted
            ) || (quest.status == ContractStatus::ReadyToReport
                && tracked_quests.contains(&quest.id))
        })
        .count();
    let active_generated_cases = ctx
        .db
        .quest_generation_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .try_fold(0usize, |count, authority| {
            let validated = validate_quest_generation_authority(&authority).map_err(|error| {
                settlement_activity_stage_error(
                    settlement_id,
                    "generated activity validation",
                    error,
                )
            })?;
            if validated.context.settlement_id != settlement_id {
                return Ok(count);
            }
            Ok::<_, String>(
                count
                    + usize::from(
                        ctx.db
                            .case_authority()
                            .id()
                            .find(&authority.case_id)
                            .is_some_and(|case| {
                                case.resolution_status == CaseStatus::Open
                            }),
                    ),
            )
        })?;
    let active = active_contracts.saturating_add(active_generated_cases);
    let target = settlement_activity_target(settlement_id);
    for (generated, _) in (active..target).enumerate() {
        if quest_batch != 0 && generated as u32 >= quest_batch {
            break;
        }
        generate_quest_for_settlement(ctx, settlement_id).map_err(|error| {
            settlement_activity_stage_error(settlement_id, "quest generation", error)
        })?;
    }
    crate::local_problem::ensure_generated_incidents(ctx, settlement_id, official_minute).map_err(
        |error| settlement_activity_stage_error(settlement_id, "generated incidents", error),
    )?;
    ensure_npc_recruiting_parties(ctx, settlement_id).map_err(|error| {
        settlement_activity_stage_error(settlement_id, "NPC recruiting parties", error)
    })?;
    Ok(())
}

fn ensure_npc_recruiting_parties(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let target = 1 + settlement_id.bytes().map(usize::from).sum::<usize>() % 2;
    let now = crate::time::refresh_clock(ctx)?;
    for mut offer in ctx.db.recruitment_offer().iter().collect::<Vec<_>>() {
        if offer.status == RecruitmentOfferStatus::Open && now >= offer.expires_at_minute {
            if recruitment_offer_bindings_are_live(ctx, &offer, now) {
                // Reuse the canonical offer identity instead of allowing
                // historical rows to exhaust the finite NPC population.
                offer.created_at_minute = now;
                offer.expires_at_minute = renewed_recruitment_offer_expiry(now);
            } else {
                offer.status = RecruitmentOfferStatus::Closed;
            }
            ctx.db.recruitment_offer().id_key().update(offer);
        }
    }
    let existing = ctx
        .db
        .recruitment_offer()
        .iter()
        .filter(|offer| offer.settlement_id == settlement_id)
        .filter(|offer| offer.status == RecruitmentOfferStatus::Open)
        .count();
    for _ in existing..target {
        let used_npcs: HashSet<_> = ctx
            .db
            .recruitment_offer()
            .settlement_id()
            .filter(&settlement_id.to_string())
            .filter(|offer| offer.status == RecruitmentOfferStatus::Open)
            .map(|offer| offer.settlement_resident_id)
            .collect();
        let Some((npc, presence)) = ctx
            .db
            .settlement_resident_profile()
            .home_settlement_id()
            .filter(&settlement_id.to_string())
            .filter(|npc| !used_npcs.contains(&npc.character_id))
            .filter_map(|npc| {
                ctx.db
                    .settlement_resident_presence()
                    .character_id()
                    .find(npc.character_id)
                    .filter(|presence| {
                        crate::settlement_population::npc_is_present(ctx, presence, now)
                    })
                    .map(|presence| (npc, presence))
            })
            .min_by_key(|(npc, _)| (!npc.service_id.is_empty(), npc.character_id))
        else {
            break;
        };
        let leader_id = npc.character_id;
        let mut leader = ctx
            .db
            .character()
            .id()
            .find(leader_id)
            .ok_or("Recruiting resident has no Character")?;
        let leader_name = leader.name.clone();
        if leader.party_id.is_none() {
            crate::strategic::create_solo_party_for_character(ctx, leader_id)?;
            leader = ctx
                .db
                .character()
                .id()
                .find(leader_id)
                .ok_or("Recruiting resident disappeared")?;
        }
        leader.current_settlement_id = Some(settlement_id.to_string());
        ctx.db.character().id().update(leader.clone());
        crate::character::set_character_languages_for_settlement(
            ctx,
            leader_id,
            settlement_id,
            true,
        )?;
        let party_id = leader.party_id.clone().ok_or("NPC leader has no party")?;
        let mut party = ctx
            .db
            .party_authority()
            .id()
            .find(&party_id)
            .ok_or("NPC party disappeared after leader assignment")?;
        party.name = format!("{}'s company", leader_name);
        party.current_settlement_id = Some(settlement_id.to_string());
        party.physiology_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.command_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.religion_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        ctx.db.party_authority().id().update(party);

        let mut requirements = RoleRequirements::default();
        if ctx.random::<u64>().is_multiple_of(2) {
            requirements.melee = true;
        } else {
            requirements.ranged = true;
        }
        requirements.athletics = (ctx.random::<u64>() % 4) as u8;
        requirements.endurance = (ctx.random::<u64>() % 4) as u8;
        let armor = ctx.random::<u64>() % 3;
        requirements.quarter_armor = armor == 1;
        requirements.half_armor = armor == 2;
        requirements.weapon_precision = (ctx.random::<u64>() % 4) as f32 * 0.5;
        ctx.db
            .party_recruitment_role()
            .insert(PartyRecruitmentRole {
                id: 0,
                party_id: party_id.clone(),
                purpose: RecruitmentRolePurpose::Specialized,
                name: if requirements.ranged {
                    "Ranged support".into()
                } else {
                    "Vanguard".into()
                },
                requirements,
                quantity: 3,
            });
        let source_key = format!("settlement-recruiter:{}", npc.character_id);
        let offer_key = format!("recruitment-offer:{source_key}");
        ctx.db.recruitment_offer().insert(RecruitmentOffer {
            id_key: offer_key.clone(),
            id: RecruitmentOfferId { value: offer_key },
            source_id: RecruitmentSourceId { value: source_key },
            recruiting_party_id: party_id,
            settlement_id: settlement_id.to_string(),
            settlement_resident_id: npc.character_id,
            location_id: presence.location_id,
            leader_id,
            status: RecruitmentOfferStatus::Open,
            created_at_minute: now,
            expires_at_minute: now
                .saturating_add(7 * adventuresim_core::strategic_time::MINUTES_PER_DAY),
        });
    }
    Ok(())
}

fn renewed_recruitment_offer_expiry(now: u64) -> u64 {
    now.saturating_add(7 * adventuresim_core::strategic_time::MINUTES_PER_DAY)
}

fn generated_witness_visible_description(
    height: &str,
    build: &str,
    hair: &str,
    clothing: &str,
) -> String {
    format!("{height}, {build}, with {hair}, wearing {clothing}")
}

fn generated_witness_candidates(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Vec<adventuresim_core::quest_generation::WitnessCandidate> {
    use adventuresim_core::{
        quest_generation::{
            Circumstance, WitnessCandidate, WitnessDemographic, retain_navigable_witnesses,
        },
        settlement_economy::player_visible_npc_tabs,
    };
    let Some(settlement) = ctx.db.settlement().id().find(settlement_id.to_owned()) else {
        return Vec::new();
    };
    let has_keep = matches!(
        settlement.category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    );
    let visible_tabs = player_visible_npc_tabs(&settlement.economy, has_keep, settlement_id);
    let mut candidates = ctx
        .db
        .settlement_resident_profile()
        .home_settlement_id()
        .filter(&settlement_id.to_string())
        .filter_map(|profile| {
            let npc = crate::settlement_population::resolve_settlement_resident(
                ctx,
                profile.character_id,
            )?;
            let presence = ctx
                .db
                .settlement_resident_presence()
                .character_id()
                .find(npc.character_id)?;
            let demographic = generated_npc_demographic(&npc);
            let mut circumstances = BTreeSet::from([
                Circumstance::NightWindow,
                Circumstance::RoadJourney,
                Circumstance::LivestockWatch,
            ]);
            if presence.location_id == "church" {
                circumstances.insert(Circumstance::GraveDuty);
            }
            if presence.location_id == "adult_venue"
                || !matches!(demographic, WitnessDemographic::Child)
            {
                circumstances.insert(Circumstance::AdultVenue);
            }
            if !matches!(demographic, WitnessDemographic::Child) {
                circumstances.insert(Circumstance::SecretRiversideMeeting);
            }
            let age_band = npc.age_band.stable_id().to_owned();
            let sex = npc.sex.stable_id().to_owned();
            let presence_version = generated_npc_presence_version(&npc, &presence);
            Some(WitnessCandidate {
                resident_character_id: npc.character_id,
                display_name: npc.name.clone(),
                demographic,
                age_band,
                sex,
                profession: npc.profession.clone(),
                visible_description: generated_witness_visible_description(
                    &npc.height,
                    &npc.build,
                    &npc.hair,
                    &npc.clothing,
                ),
                expected_location: presence.location_id,
                expected_location_label: String::new(),
                presence_version,
                allowed_circumstances: circumstances,
            })
        })
        .collect::<Vec<_>>();
    candidates = retain_navigable_witnesses(candidates, &visible_tabs);
    candidates.sort_by_key(|left| left.resident_character_id);
    candidates
}

/// Developer quest authority must compile from the same player-visible NPC
/// facts as the gateway preview. Automatic generation intentionally continues
/// to use `generated_witness_candidates` and its private demographic truth.
fn developer_witness_candidates(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Vec<adventuresim_core::quest_generation::WitnessCandidate> {
    use adventuresim_core::{
        quest_generation::retain_navigable_witnesses, settlement_economy::player_visible_npc_tabs,
    };
    let Some(settlement) = ctx.db.settlement().id().find(settlement_id.to_owned()) else {
        return Vec::new();
    };
    let has_keep = matches!(
        settlement.category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    );
    let visible_tabs = player_visible_npc_tabs(&settlement.economy, has_keep, settlement_id);
    let mut candidates = ctx
        .db
        .settlement_resident_profile()
        .home_settlement_id()
        .filter(&settlement_id.to_string())
        .filter_map(|profile| {
            let npc = crate::settlement_population::resolve_settlement_resident(
                ctx,
                profile.character_id,
            )?;
            let presence = ctx
                .db
                .settlement_resident_presence()
                .character_id()
                .find(npc.character_id)?;
            developer_npc_witness_candidate(&npc, &presence)
        })
        .collect::<Vec<_>>();
    candidates = retain_navigable_witnesses(candidates, &visible_tabs);
    candidates.sort_by_key(|left| left.resident_character_id);
    candidates
}

pub(crate) fn developer_npc_witness_candidate(
    npc: &crate::settlement_population::ResolvedSettlementResident,
    presence: &crate::settlement_population::SettlementResidentPresence,
) -> Option<adventuresim_core::quest_generation::WitnessCandidate> {
    use adventuresim_core::quest_generation::{
        VisibleWitnessCandidateInput, visible_witness_candidate,
    };
    let age_band = npc.age_band.stable_variant_id();
    let presentation = npc.presentation.stable_variant_id();
    visible_witness_candidate(VisibleWitnessCandidateInput {
        resident_character_id: npc.character_id,
        display_name: &npc.name,
        age_band,
        presentation,
        height: &npc.height,
        build: &npc.build,
        hair: &npc.hair,
        clothing: &npc.clothing,
        profession: &npc.profession,
        local_role: &npc.local_role,
        settlement_id: &presence.settlement_id,
        location_id: &presence.location_id,
        start_minute: presence.start_minute,
        end_minute: presence.end_minute,
        is_default: presence.is_default,
    })
}

pub(crate) fn generated_npc_demographic(
    npc: &crate::settlement_population::ResolvedSettlementResident,
) -> adventuresim_core::quest_generation::WitnessDemographic {
    let age_band = npc.age_band.stable_id();
    let sex = npc.sex.stable_id();
    let authored = adventuresim_core::quest_catalog::catalog()
        .witness_demographic_for(age_band, sex, &npc.profession, &npc.local_role)
        .expect("validated demographic catalog has one fallback");
    adventuresim_core::quest_generation::WitnessDemographic::try_new(&authored.id)
        .expect("validated open demographic ID")
}

pub(crate) fn generated_npc_presence_version(
    npc: &crate::settlement_population::ResolvedSettlementResident,
    presence: &crate::settlement_population::SettlementResidentPresence,
) -> u64 {
    let mut hash = 1_469_598_103_934_665_603_u64;
    let character_id = npc.character_id.to_le_bytes();
    let start_minute = presence.start_minute.to_le_bytes();
    let end_minute = presence.end_minute.to_le_bytes();
    let is_default = [u8::from(presence.is_default)];
    for value in [
        b"adventuresim.generated-witness-presence.v1".as_slice(),
        character_id.as_slice(),
        npc.age_band.stable_variant_id().as_bytes(),
        npc.sex.stable_variant_id().as_bytes(),
        npc.profession.as_bytes(),
        presence.settlement_id.as_bytes(),
        presence.location_id.as_bytes(),
        start_minute.as_slice(),
        end_minute.as_slice(),
        is_default.as_slice(),
    ] {
        for byte in (value.len() as u64)
            .to_le_bytes()
            .into_iter()
            .chain(value.iter().copied())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
        }
    }
    hash
}

fn generated_scene_key(kind: adventuresim_core::quest_generation::SiteKind) -> &'static str {
    let site = adventuresim_core::quest_catalog::catalog()
        .site(kind.as_str())
        .expect("generated site exists in startup catalog");
    match site.terrain.as_str() {
        "underground" => "cave",
        "forest" => "woods",
        "settlement" => "ruins",
        "road" => "camp",
        _ => unreachable!("catalog validation rejects unknown terrain mechanics"),
    }
}

fn generate_quest_for_settlement(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    use adventuresim_core::quest_generation as qg;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_string())
        .ok_or("Settlement not found")?;
    let ordinal = ctx
        .db
        .quest_generation_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .try_fold(0u16, |count, row| {
            let validated = validate_quest_generation_authority(&row)?;
            Ok::<_, String>(count + u16::from(validated.context.settlement_id == settlement_id))
        })?;
    let seed = ctx.random::<u64>();
    let observer_entropy_hi = ctx.random::<u64>();
    let observer_entropy_lo = ctx.random::<u64>();
    let now_minute = crate::time::refresh_clock(ctx)?;
    let weather_coordinate = adventuresim_world_schema::coordinates::Wgs84CoordinateMicrodegrees::from_longitude_latitude_degrees(
        settlement.coord_x,
        settlement.coord_y,
    )
    .ok_or("Settlement has invalid WGS84 coordinates")?;
    let incident_weather = adventuresim_core::weather::weather_at(
        adventuresim_core::weather::WORLD_WEATHER_SEED,
        now_minute.saturating_sub(180),
        weather_coordinate.latitude().get(),
        weather_coordinate.longitude().get(),
        0,
    )
    .precipitation;
    let context = qg::GenerationContext {
        seed,
        observer_entropy_hi,
        observer_entropy_lo,
        settlement_id: settlement_id.into(),
        settlement_name: settlement.name.clone(),
        scope: adventuresim_core::local_problem::Scope::Settlement {
            settlement_id: settlement_id.into(),
        },
        ordinal,
        now_minute,
        incident_weather,
        requested_family: (ordinal < 2).then_some(if ordinal == 0 {
            qg::TemplateFamily::RecurringDepredation
        } else {
            qg::TemplateFamily::DisappearanceOrLoss
        }),
        witness_candidates: generated_witness_candidates(ctx, settlement_id),
    };
    let generated = qg::generate(&context)
        .map_err(|error| format!("Quest generator exhausted its bounded search: {error:?}"))?;
    qg::validate(&generated)
        .map_err(|errors| format!("Generated quest manifest is invalid: {}", errors.join("; ")))?;

    materialize_generated_quest(ctx, &settlement, &context, &generated, None, None)
}

fn materialize_preferred_generated_fixture(
    ctx: &ReducerContext,
    character_id: u64,
    family: adventuresim_core::quest_generation::TemplateFamily,
    seed_salt: u64,
) -> Result<String, String> {
    use adventuresim_core::quest_generation as qg;

    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .clone()
        .ok_or("Load the outbreak demo while in a settlement")?;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Current settlement not found")?;

    let now_minute = crate::time::refresh_clock(ctx)?.max(4_000);
    let entropy = character_id ^ seed_salt;
    let initial_context = qg::GenerationContext {
        seed: preferred_fixture_seed(entropy, family),
        observer_entropy_hi: entropy.rotate_left(23),
        observer_entropy_lo: entropy.rotate_right(17),
        settlement_id: settlement_id.clone(),
        settlement_name: settlement.name.clone(),
        scope: adventuresim_core::local_problem::Scope::Settlement {
            settlement_id: settlement_id.clone(),
        },
        ordinal: (seed_salt as u16).max(1),
        now_minute,
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(family),
        witness_candidates: generated_witness_candidates(ctx, &settlement_id),
    };
    let (context, generated) = (0..64_u64)
        .find_map(|offset| {
            let mut candidate = initial_context.clone();
            candidate.seed = candidate.seed.wrapping_add(offset);
            let generated = qg::generate(&candidate).ok()?;
            let suitable = family != qg::TemplateFamily::RecurringDepredation
                || generated.hostile_groups.iter().any(|(_, _, threat, _)| {
                    *threat == adventuresim_core::bestiary::ThreatId::Bandit
                        || *threat == adventuresim_core::bestiary::ThreatId::Smuggler
                });
            suitable.then_some((candidate, generated))
        })
        .ok_or("Development quest fixture exhausted negotiable generated threats")?;
    let witness = generated
        .witnesses
        .first()
        .ok_or("Generated quest fixture has no first witness")?;
    if !context
        .witness_candidates
        .iter()
        .any(|candidate| candidate.resident_character_id == witness.resident_character_id)
    {
        return Err("Generated quest fixture first witness is not navigable".into());
    }
    if ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&generated.canonical_case_id)
        .is_none()
    {
        qg::validate(&generated).map_err(|errors| {
            format!(
                "Development quest fixture manifest is invalid: {}",
                errors.join("; ")
            )
        })?;
        let fixture_site_distance_m = preferred_fixture_site_distance_m(family);
        materialize_generated_quest(
            ctx,
            &settlement,
            &context,
            &generated,
            None,
            fixture_site_distance_m,
        )?;
    }
    crate::local_problem::prefer_next_rumor(
        ctx,
        character_id,
        &settlement_id,
        &generated.problem_id,
    );
    let problem = ctx
        .db
        .local_problem_authority()
        .id()
        .find(&generated.problem_id)
        .ok_or("Development quest fixture did not materialize its exact local problem")?;
    if problem.opaque_case_ref != generated.canonical_case_id
        || problem.scope_key != format!("settlement:{settlement_id}")
    {
        return Err("Development quest fixture local-problem binding is inconsistent".into());
    }
    Ok(generated.problem_id)
}

fn preferred_fixture_seed(
    entropy: u64,
    family: adventuresim_core::quest_generation::TemplateFamily,
) -> u64 {
    let seed = entropy.rotate_left(11);
    if family == adventuresim_core::quest_generation::TemplateFamily::Outbreak {
        seed - seed % 6
    } else {
        seed
    }
}

fn ordinary_generated_site_distance_m(seed: u64, index: usize) -> u64 {
    4_000 + (seed.rotate_left(index as u32) % 17_000)
}

fn materialize_simulation_acceptance_outbreak(
    ctx: &ReducerContext,
    character_id: u64,
    policy_seed: u64,
) -> Result<String, String> {
    use adventuresim_core::quest_generation as qg;

    const MAX_CANDIDATES: u16 = 32_768;
    const MAX_REQUIRED_ROUTE_DISTANCE_M: u64 = 5_500;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .clone()
        .ok_or("Quest acceptance outbreak leader must be in a settlement")?;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Quest acceptance outbreak settlement not found")?;
    let now_minute = crate::time::refresh_clock(ctx)?.max(4_000);
    let entropy = character_id ^ policy_seed ^ 0x4143_4345_5054_414e;
    for candidate in 0..MAX_CANDIDATES {
        let candidate_entropy =
            entropy ^ u64::from(candidate).wrapping_mul(MISSION_CANDIDATE_ENTROPY_STRIDE);
        let context = qg::GenerationContext {
            seed: candidate_entropy.rotate_left(11),
            observer_entropy_hi: candidate_entropy.rotate_left(23),
            observer_entropy_lo: candidate_entropy.rotate_right(17),
            settlement_id: settlement_id.clone(),
            settlement_name: settlement.name.clone(),
            scope: adventuresim_core::local_problem::Scope::Settlement {
                settlement_id: settlement_id.clone(),
            },
            ordinal: candidate,
            now_minute,
            incident_weather: adventuresim_core::weather::Precipitation::Clear,
            requested_family: Some(qg::TemplateFamily::Outbreak),
            witness_candidates: generated_witness_candidates(ctx, &settlement_id),
        };
        let generated = qg::generate(&context)
            .map_err(|error| format!("Acceptance outbreak generation failed: {error:?}"))?;
        if generated.sites.is_empty()
            || generated.sites.iter().enumerate().any(|(index, _)| {
                ordinary_generated_site_distance_m(context.seed, index)
                    > MAX_REQUIRED_ROUTE_DISTANCE_M
            })
        {
            continue;
        }
        qg::validate(&generated).map_err(|errors| {
            format!(
                "Acceptance outbreak manifest is invalid: {}",
                errors.join("; ")
            )
        })?;
        materialize_generated_quest(ctx, &settlement, &context, &generated, None, None)?;
        crate::local_problem::prefer_next_rumor(
            ctx,
            character_id,
            &settlement_id,
            &generated.problem_id,
        );
        return Ok(generated.canonical_case_id);
    }
    Err("No ordinary generated outbreak met the bounded acceptance route constraint".into())
}

fn seed_outbreak_demo(ctx: &ReducerContext, character_id: u64) -> Result<String, String> {
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Outbreak demo character has no skills")?;
    skills.physiology_hours = skills.physiology_hours.max(20_000.0);
    skills.surgery_hours = skills.surgery_hours.max(20_000.0);
    skills.knife_hours = skills.knife_hours.max(20_000.0);
    skills.insight_hours = skills.insight_hours.max(8_000.0);
    skills.charm_hours = skills.charm_hours.max(8_000.0);
    skills.terrain_urban_hours = skills.terrain_urban_hours.max(8_000.0);
    skills.bestiary_hours = adventuresim_world_schema::BestiaryHours {
        beast: 8_000.0,
        undead: 8_000.0,
        human: 8_000.0,
        werekin: 8_000.0,
        elf: 8_000.0,
        dwarf: 8_000.0,
        fey: 8_000.0,
        spirit: 8_000.0,
        greenskin: 8_000.0,
        insectoid: 8_000.0,
        draconid: 8_000.0,
        construct: 8_000.0,
        wildmen: 8_000.0,
    };
    ctx.db.character_skills().character_id().update(skills);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    if !ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .any(|row| row.item_id == "surgery_kit")
    {
        crate::add_inventory_item(ctx, character_id, "surgery_kit", 1);
    }
    if !ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .any(|row| row.item_id == "cooking_pot")
    {
        crate::add_inventory_item(ctx, character_id, "cooking_pot", 1);
    }
    let cooking_pot = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .find(|row| row.item_id == "cooking_pot")
        .ok_or("Outbreak demo cooking pot was not materialized")?;
    crate::inventory_container::require_object(
        ctx,
        character_id,
        adventuresim_core::physical_object::CarriedInventoryScope::Personal,
        cooking_pot.id,
    )?;

    materialize_preferred_generated_fixture(
        ctx,
        character_id,
        adventuresim_core::quest_generation::TemplateFamily::Outbreak,
        0x4f55_5442_5245_414b,
    )
}

pub(crate) struct SimulationQuestFixtureSeed {
    pub direct_contract_id: String,
    pub generated_canonical_case_id: String,
    pub direct_party_id: String,
    pub generated_party_id: String,
}

const SIMULATION_QUEST_ENEMY_TYPE: &str = "cultist";
const SIMULATION_QUEST_ENEMY_DIFFICULTY: i32 = 1;
const SIMULATION_QUEST_ENEMY_COMBAT_SCALE_BPS: u32 =
    adventuresim_world_schema::BASIS_POINTS_PER_WHOLE as u32;
pub(crate) fn simulation_quest_fixture_enemy_power() -> Result<u64, String> {
    autoresolve_enemy(
        u64::MAX,
        SIMULATION_QUEST_ENEMY_TYPE,
        SIMULATION_QUEST_ENEMY_DIFFICULTY,
        SIMULATION_QUEST_ENEMY_COMBAT_SCALE_BPS,
    )
    .map(|enemy| adventuresim_core::autoresolve::autoresolve_combat_power(&enemy))
}

fn simulation_quest_provisioning_economy(
    mut economy: SettlementEconomyProfile,
) -> Result<SettlementEconomyProfile, String> {
    use adventuresim_world_schema::{
        ProfileFactProvenance, SettlementService, SettlementStock, StockCategory,
    };

    if !economy.services.contains(&SettlementService::GeneralStore) {
        economy.services.push(SettlementService::GeneralStore);
    }
    if !economy.services.contains(&SettlementService::Temple) {
        economy.services.push(SettlementService::Temple);
    }
    economy.services.sort();
    if !economy
        .stock
        .iter()
        .any(|stock| stock.category == StockCategory::GeneralGoods)
    {
        economy.stock.push(SettlementStock {
            category: StockCategory::GeneralGoods,
            abundance: 1,
            provenance: ProfileFactProvenance::DeterministicGapFill,
        });
        economy.stock.sort_by_key(|stock| stock.category);
    }
    economy.validate()?;
    Ok(economy)
}

fn ensure_simulation_quest_provisioning_environment(
    ctx: &ReducerContext,
    leader_id: u64,
) -> Result<String, String> {
    let character = crate::character::require_living_character(ctx, leader_id)?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Quest fixture leader must be in a settlement")?;
    let mut settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Quest fixture settlement is unavailable")?;
    settlement.economy = simulation_quest_provisioning_economy(settlement.economy)?;
    ctx.db.settlement().id().update(settlement);

    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(leader_id)
        .map_or(720, |time| time.minutes);
    let provider_id = default_merchant_provider(ctx, &settlement_id, "merchants", "market")
        .map_err(|error| error.to_string())?
        .get();
    let provider = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(provider_id)
        .ok_or("Quest fixture general merchant has no presence")?;
    if !crate::settlement_population::npc_is_present(ctx, &provider, minute) {
        return Err("Quest fixture general merchant is not presently available".into());
    }
    Ok(settlement_id)
}

pub(crate) fn seed_simulation_quest_fixture_inner(
    ctx: &ReducerContext,
    policy_seed: u64,
    direct_leader_id: u64,
    generated_leader_id: u64,
) -> Result<SimulationQuestFixtureSeed, String> {
    use adventuresim_core::case::{
        Objective, ObjectiveExpression, ObjectiveId, ObjectivePath, ObjectiveRequirement,
    };
    use adventuresim_core::settlement_economy::npc_location_is_navigable;

    let suffix = format!("{policy_seed:016x}");
    let contract_id = format!("contract:simulation-acceptance-direct:{suffix}");
    if ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .is_some()
    {
        return Err("Simulation direct quest fixture ID is already in use".into());
    }

    let settlement_id = ensure_simulation_quest_provisioning_environment(ctx, direct_leader_id)?;
    ensure_simulation_quest_provisioning_environment(ctx, generated_leader_id)?;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Direct quest fixture settlement is unavailable")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(direct_leader_id)
        .map_or(720, |time| time.minutes);
    let has_keep = matches!(
        settlement.category,
        SettlementCategory::Town | SettlementCategory::City | SettlementCategory::Capital
    );
    let mut issuers = ctx
        .db
        .settlement_resident_profile()
        .home_settlement_id()
        .filter(&settlement_id)
        .filter(|profile| {
            !profile.service_id.is_empty()
                && crate::settlement_population::resident_is_dialogue_capable(profile)
                && ctx
                    .db
                    .settlement_resident_presence()
                    .character_id()
                    .find(profile.character_id)
                    .is_some_and(|presence| {
                        presence.settlement_id == settlement_id
                            && crate::settlement_population::npc_is_present(ctx, &presence, minute)
                            && npc_location_is_navigable(
                                &settlement.economy,
                                has_keep,
                                &settlement_id,
                                &presence.location_id,
                            )
                    })
        })
        .collect::<Vec<_>>();
    issuers.sort_by_key(|profile| profile.character_id);
    let issuer = issuers
        .into_iter()
        .next()
        .ok_or("Direct quest fixture has no present navigable dialogue-capable issuer")?;

    // Generate and privately materialize the second party's local problem
    // before publishing the direct contract marker. No skill or item boosts
    // are part of this acceptance fixture.
    let generated_canonical_case_id =
        materialize_simulation_acceptance_outbreak(ctx, generated_leader_id, policy_seed)?;
    let direct_party_id = ctx
        .db
        .character()
        .id()
        .find(direct_leader_id)
        .and_then(|character| character.party_id)
        .ok_or("Direct quest fixture leader has no party")?;
    let generated_party_id = ctx
        .db
        .character()
        .id()
        .find(generated_leader_id)
        .and_then(|character| character.party_id)
        .ok_or("Generated quest fixture leader has no party")?;

    let case_id = format!("case:simulation-acceptance-direct:{suffix}");
    let case_site_id = format!("case-site:simulation-acceptance-direct:{suffix}");
    let hostile_group_id = format!("hostile-group:simulation-acceptance-direct:{suffix}");
    let objective = ObjectiveExpression::new(vec![ObjectivePath {
        objectives: vec![Objective {
            id: ObjectiveId::new(format!("objective:simulation-acceptance-direct:{suffix}"))
                .map_err(|_| "Direct quest fixture objective ID is invalid")?,
            requirement: ObjectiveRequirement::Defeat {
                hostile_group_id: hostile_group_id.clone(),
                count: 1,
            },
        }],
    }])
    .map_err(|_| "Direct quest fixture objective is invalid")?;
    ctx.db.case_authority().insert(CaseAuthority {
        id: case_id.clone(),
        investigation_case_id: format!("investigation:simulation-acceptance-direct:{suffix}"),
        provenance_kind: InvestigationProvenanceKind::Manual,
        generated_case_id: String::new(),
        local_problem_id: None,
        objective_expression_json: serde_json::to_string(&objective)
            .map_err(|_| "Could not encode direct quest fixture objective")?,
        resolution_status: CaseStatus::Open,
        resolved_by_party_id: None,
    });
    let geographic = settlement.source_node_id.is_some();
    let (offset_x, offset_y) = if geographic {
        (0.0, 2.0 / 111.0)
    } else {
        (0.0, 2.0)
    };
    let site_coordinate = encode_position_e7(
        settlement.coord_x + offset_x,
        settlement.coord_y + offset_y,
        geographic,
    )
    .ok_or("Simulation quest site is not a valid WGS84 coordinate")?;
    let site = CaseSiteAuthority {
        id_key: case_site_id.clone(),
        id: CaseSiteId::from(case_site_id),
        case_id: case_id.clone(),
        origin_settlement_id: settlement_id.clone(),
        name: "A Nearby Robbers' Camp".into(),
        description: "A small bandit camp lies a short march from the settlement.".into(),
        scene_key: "forest-clearing".into(),
        longitude_e7: site_coordinate.longitude_e7,
        latitude_e7: site_coordinate.latitude_e7,
        coordinates_are_geographic: geographic,
        distance_m: 2_000,
    };
    ctx.db.case_site_authority().insert(site.clone());
    // The acceptance opponent is a normal, authored, unarmored novice threat.
    // It still uses ordinary hostile materialization and autoresolve; unlike an
    // armored bandit, it does not silently turn a difficulty-one fixture into
    // a shield-and-armor proficiency check for starting adventurers.
    materialize_hostile_group(
        ctx,
        &hostile_group_id,
        &site,
        SIMULATION_QUEST_ENEMY_TYPE.into(),
        1,
        SIMULATION_QUEST_ENEMY_DIFFICULTY,
    )?;
    ctx.db.contract_authority().insert(ContractAuthority {
        id: contract_id.clone(),
        gateway_bucket: 0,
        case_id,
        title: "Trouble on the Near Road".into(),
        description: "Drive one knife-wielding troublemaker from a camp near the settlement."
            .into(),
        difficulty: 1,
        gold_reward: 12,
        xp_reward: 20,
        settlement_id,
        service_id: issuer.service_id,
        issuer_resident_character_id: issuer.character_id,
        status: ContractStatus::Offered,
        accepted_by: None,
        opposition_wording: "unarmored troublemaker".into(),
        opposition_count_wording: "one".into(),
        accepted_at_minute: None,
        paid_at_minute: None,
    });
    Ok(SimulationQuestFixtureSeed {
        direct_contract_id: contract_id,
        generated_canonical_case_id,
        direct_party_id,
        generated_party_id,
    })
}

fn materialize_generated_quest(
    ctx: &ReducerContext,
    settlement: &Settlement,
    context: &adventuresim_core::quest_generation::GenerationContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    context_snapshot_override: Option<&str>,
    fixture_site_distance_m: Option<u64>,
) -> Result<(), String> {
    let settlement_id = context.settlement_id.as_str();
    let seed = context.seed;
    if ctx
        .db
        .case_authority()
        .id()
        .find(&generated.canonical_case_id)
        .is_some()
    {
        return Err(format!(
            "Generated case ID collision: {}",
            generated.canonical_case_id
        ));
    }
    ctx.db.case_authority().insert(CaseAuthority {
        id: generated.canonical_case_id.clone(),
        investigation_case_id: generated.canonical_case_id.clone(),
        provenance_kind: InvestigationProvenanceKind::Generated,
        generated_case_id: generated.canonical_case_id.clone(),
        local_problem_id: Some(generated.problem_id.clone()),
        objective_expression_json: serde_json::to_string(&generated.objectives)
            .map_err(|_| "Could not encode generated objectives")?,
        resolution_status: CaseStatus::Open,
        resolved_by_party_id: None,
    });
    ctx.db.investigation_case_authority().insert(
        crate::investigation::InvestigationCaseAuthority {
            id: generated.canonical_case_id.clone(),
            problem_id: generated.problem_id.clone(),
            hidden_target_json: serde_json::to_string(&generated.cause)
                .map_err(|_| "Could not encode canonical generated cause")?,
            generation_explanation_json: serde_json::to_string(&generated.factor_trace)
                .map_err(|_| "Could not encode generated factor trace")?,
        },
    );
    for event in &generated.canonical_events {
        if ctx
            .db
            .investigation_event_authority()
            .id()
            .find(&event.id)
            .is_some()
        {
            return Err(format!("Generated event ID collision: {}", event.id));
        }
        ctx.db.investigation_event_authority().insert(
            crate::investigation::InvestigationEventAuthority {
                id: event.id.clone(),
                case_id: generated.canonical_case_id.clone(),
                canonical_propositions_json: serde_json::to_string(&[serde_json::json!({
                    "id": event.proposition_id,
                    "subject": event.subject,
                    "predicate": event.predicate,
                    "object": event.object,
                })])
                .map_err(|_| "Could not encode generated event")?,
                occurred_at: event.occurred_at,
            },
        );
    }

    let geographic = settlement.source_node_id.is_some();
    let settlement_coordinate =
        encode_position_e7(settlement.coord_x, settlement.coord_y, geographic)
            .ok_or("Generated investigation center is not a valid WGS84 coordinate")?;
    let mut site_rows = BTreeMap::new();
    for (index, site) in generated.sites.iter().enumerate() {
        let distance_m = fixture_site_distance_m
            .unwrap_or_else(|| ordinary_generated_site_distance_m(seed, index));
        let angle_seed = seed.rotate_left((index as u32).saturating_mul(11));
        let angle = (angle_seed as f64 / u64::MAX as f64) * std::f64::consts::TAU;
        let distance_km = distance_m as f64 / 1_000.0;
        let (offset_x, offset_y) = if geographic {
            let latitude_scale = 111.0;
            let longitude_scale =
                latitude_scale * settlement.coord_y.to_radians().cos().abs().max(0.1);
            (
                angle.cos() * distance_km / longitude_scale,
                angle.sin() * distance_km / latitude_scale,
            )
        } else {
            (angle.cos() * distance_km, angle.sin() * distance_km)
        };
        let site_coordinate = encode_position_e7(
            settlement.coord_x + offset_x,
            settlement.coord_y + offset_y,
            geographic,
        )
        .ok_or("Generated case site is not a valid WGS84 coordinate")?;
        let row = CaseSiteAuthority {
            id_key: site.id.0.clone(),
            id: CaseSiteId::from(site.id.0.clone()),
            case_id: generated.canonical_case_id.clone(),
            origin_settlement_id: settlement_id.into(),
            name: site.safe_label.clone(),
            description: format!("You arrive at {}.", site.safe_label),
            scene_key: generated_scene_key(site.kind).into(),
            longitude_e7: site_coordinate.longitude_e7,
            latitude_e7: site_coordinate.latitude_e7,
            coordinates_are_geographic: geographic,
            distance_m,
        };
        if let Some(existing) = ctx.db.case_site_authority().id_key().find(&row.id_key) {
            return Err(format!(
                "Generated case-site ID collision: {} for {} ({}) already belongs to {} ({})",
                row.id_key, generated.canonical_case_id, row.name, existing.case_id, existing.name
            ));
        }
        ctx.db.case_site_authority().insert(row.clone());
        site_rows.insert(site.id.clone(), row);
    }
    for area in &generated.areas {
        if ctx
            .db
            .investigation_area_authority()
            .id()
            .find(&area.id)
            .is_some()
        {
            return Err(format!("Generated area ID collision: {}", area.id));
        }
        ctx.db.investigation_area_authority().insert(
            crate::investigation::InvestigationAreaAuthority {
                id: area.id.clone(),
                case_id: generated.canonical_case_id.clone(),
                origin_settlement_id: settlement_id.into(),
                safe_label: area.safe_label.clone(),
                center_longitude_e7: settlement_coordinate.longitude_e7,
                center_latitude_e7: settlement_coordinate.latitude_e7,
                radius_m: 5_000,
                coordinates_are_geographic: geographic,
                terrain: serde_json::to_string(&area.terrain)
                    .map_err(|_| "Could not encode generated area terrain")?
                    .trim_matches('"')
                    .into(),
            },
        );
    }
    for evidence in &generated.evidence {
        if ctx
            .db
            .investigation_evidence_authority()
            .id()
            .find(&evidence.id.0)
            .is_some()
        {
            return Err(format!(
                "Generated evidence ID collision: {}",
                evidence.id.0
            ));
        }
        ctx.db.investigation_evidence_authority().insert(
            crate::investigation::InvestigationEvidenceAuthority {
                id: evidence.id.0.clone(),
                case_id: generated.canonical_case_id.clone(),
                proposition_id: evidence.proposition_id.clone(),
                presentation_kind: crate::investigation::EvidencePresentationKind::Physical,
                authority_json: serde_json::to_string(evidence)
                    .map_err(|_| "Could not encode generated evidence")?,
                hidden_coordinates_json: serde_json::to_string(&evidence.site_id)
                    .map_err(|_| "Could not encode generated evidence site")?,
            },
        );
    }
    for witness in &generated.witnesses {
        if ctx
            .db
            .investigation_testimony_bundle()
            .id()
            .find(&witness.id.0)
            .is_some()
        {
            return Err(format!(
                "Generated testimony ID collision: {}",
                witness.id.0
            ));
        }
        ctx.db.investigation_testimony_bundle().insert(
            crate::investigation::InvestigationTestimonyBundle {
                id: witness.id.0.clone(),
                case_id: generated.canonical_case_id.clone(),
                witness_ref: witness.resident_character_id.to_string(),
                reliability_json: serde_json::to_string(
                    &witness
                        .testimony
                        .iter()
                        .map(|draft| draft.reliability)
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| "Could not encode testimony reliability")?,
                stages_json: serde_json::to_string(&witness.testimony)
                    .map_err(|_| "Could not encode testimony drafts")?,
            },
        );
    }
    for (group_id, site_id, threat, count) in &generated.hostile_groups {
        let site = site_rows
            .get(site_id)
            .ok_or("Generated hostile group references a missing site")?;
        materialize_hostile_group(ctx, group_id, site, threat.as_str().into(), *count, 2)?;
    }
    seed_case_custody(
        ctx,
        &generated.canonical_case_id,
        &generated.objectives,
        &generated.custody,
    )?;
    for (path_index, _) in generated.objectives.alternatives.iter().enumerate() {
        ctx.db.case_finale_authority().insert(CaseFinaleAuthority {
            id: format!("finale:{}:{path_index}", generated.canonical_case_id),
            case_id: generated.canonical_case_id.clone(),
            kind: FinaleExecutionKind::RecordResolution,
            resolution_status: CaseStatus::Resolved,
            eligible_path_index: Some(path_index as u16),
            priority: 100u16.saturating_sub(path_index as u16),
            status: FinaleStatus::Available,
        });
    }
    ctx.db.case_finale_authority().insert(CaseFinaleAuthority {
        id: format!("finale:{}:problem", generated.canonical_case_id),
        case_id: generated.canonical_case_id.clone(),
        kind: FinaleExecutionKind::ResolveLocalProblem,
        resolution_status: CaseStatus::Resolved,
        eligible_path_index: None,
        priority: 1,
        status: FinaleStatus::Available,
    });
    crate::local_problem::materialize_generated_problem(ctx, generated, settlement_id)?;
    crate::outbreak::materialize_generated_outbreak(
        ctx,
        generated,
        settlement_id,
        context.now_minute,
    )?;
    let context_snapshot_json = context_snapshot_override.map(str::to_owned).map_or_else(
        || serde_json::to_string(&context).map_err(|_| "Could not encode quest generation context"),
        Ok,
    )?;
    ctx.db
        .quest_generation_authority()
        .insert(QuestGenerationAuthority {
            case_id: generated.canonical_case_id.clone(),
            public_case_id: generated.public_case_id.clone(),
            settlement_id: context.settlement_id.clone(),
            settlement_name: context.settlement_name.clone(),
            seed,
            catalog_revision: generated.catalog_revision.clone(),
            context_commitment: quest_generation_context_commitment(&context_snapshot_json)?,
            context_snapshot_json,
            manifest_json: serde_json::to_string(&generated)
                .map_err(|_| "Could not encode quest generation manifest")?,
            factor_trace_json: serde_json::to_string(&generated.factor_trace)
                .map_err(|_| "Could not encode quest generation trace")?,
        });
    Ok(())
}

/// Developer-only authoring reducer.
///
/// There is intentionally no gameplay authorization in this pre-launch tool:
/// strategic-web hides the control unless browser-local developer mode is on,
/// but any caller able to invoke reducers can call this directly. The caller
/// supplies a character, never a settlement; current location is derived from
/// authoritative character state.
#[reducer]
pub fn spawn_developer_quest(
    ctx: &ReducerContext,
    character_id: u64,
    definition_json: String,
    allow_implausible: bool,
) -> Result<(), String> {
    require_development_gateway(ctx)?;
    use adventuresim_core::{developer_quest as dq, quest_generation as qg};
    require_strategic_character_authority(ctx, character_id)?;
    let definition = dq::parse_definition_json(&definition_json).map_err(|diagnostics| {
        serde_json::to_string(&diagnostics).unwrap_or_else(|_| "Invalid developer quest".into())
    })?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let settlement_id = character
        .current_settlement_id
        .clone()
        .ok_or("Developer quests can only be spawned while the character is in a settlement")?;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.clone())
        .ok_or("Current settlement not found")?;
    let ordinal = ctx
        .db
        .quest_generation_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .try_fold(0u16, |count, row| {
            let validated = validate_quest_generation_authority(&row)?;
            Ok::<_, String>(count + u16::from(validated.context.settlement_id == settlement_id))
        })?;
    let now_minute = crate::time::refresh_clock(ctx)?;
    let weather_coordinate = adventuresim_world_schema::coordinates::Wgs84CoordinateMicrodegrees::from_longitude_latitude_degrees(
        settlement.coord_x,
        settlement.coord_y,
    )
    .ok_or("Settlement has invalid WGS84 coordinates")?;
    let incident_weather = adventuresim_core::weather::weather_at(
        adventuresim_core::weather::WORLD_WEATHER_SEED,
        now_minute.saturating_sub(180),
        weather_coordinate.latitude().get(),
        weather_coordinate.longitude().get(),
        0,
    )
    .precipitation;
    let base = qg::GenerationContext {
        seed: ctx.random(),
        observer_entropy_hi: ctx.random(),
        observer_entropy_lo: ctx.random(),
        settlement_id: settlement_id.clone(),
        settlement_name: settlement.name.clone(),
        scope: adventuresim_core::local_problem::Scope::Settlement {
            settlement_id: settlement_id.clone(),
        },
        ordinal,
        now_minute,
        incident_weather,
        requested_family: Some(definition.family),
        witness_candidates: developer_witness_candidates(ctx, &settlement_id),
    };
    let developer_context = dq::DeveloperGenerationContext {
        base: base.clone(),
        definition,
        allow_implausible,
    };
    let generated = dq::compile(&developer_context).map_err(|diagnostics| {
        serde_json::to_string(&diagnostics).unwrap_or_else(|_| "Invalid developer quest".into())
    })?;
    let context_snapshot_json = serde_json::to_string(&developer_context)
        .map_err(|_| "Could not encode developer quest generation context")?;
    materialize_generated_quest(
        ctx,
        &settlement,
        &base,
        &generated,
        Some(&context_snapshot_json),
        None,
    )
}

fn preferred_fixture_site_distance_m(
    family: adventuresim_core::quest_generation::TemplateFamily,
) -> Option<u64> {
    (family == adventuresim_core::quest_generation::TemplateFamily::RecurringDepredation)
        .then_some(1_000)
}

#[cfg(test)]
mod developer_quest_source_tests {
    use super::*;

    #[test]
    fn standalone_mission_family_uses_an_exact_mission_tag() {
        assert!(
            standalone_mission_family("mission:animation-demo").unwrap()
                == StandaloneMissionFamily::Animation
        );
        assert!(
            standalone_mission_family("mission:ordinary").unwrap()
                == StandaloneMissionFamily::General
        );
        assert!(
            standalone_mission_family("mission:diagnostic-demo").unwrap()
                == StandaloneMissionFamily::Diagnostic
        );
        for invalid in [
            "mission:",
            "mission:animation-",
            "mission:diagnostic-",
            "case:animation-demo",
            "animation-demo",
        ] {
            assert!(standalone_mission_family(invalid).is_err());
        }
    }

    #[test]
    fn outbreak_demo_seed_selects_the_waterborne_fixture() {
        let entropy = 9_999_999_999_999_959u64 ^ 0x4f55_5442_5245_414b;
        let seed = preferred_fixture_seed(
            entropy,
            adventuresim_core::quest_generation::TemplateFamily::Outbreak,
        );
        assert_eq!(seed % 6, 0);
        assert_eq!(
            preferred_fixture_seed(
                entropy,
                adventuresim_core::quest_generation::TemplateFamily::RecurringDepredation,
            ),
            entropy.rotate_left(11)
        );
    }

    #[test]
    fn only_the_recurring_threat_gallery_fixture_gets_the_nearby_site_override() {
        use adventuresim_core::quest_generation::TemplateFamily;
        assert_eq!(
            preferred_fixture_site_distance_m(TemplateFamily::RecurringDepredation),
            Some(1_000)
        );
        assert_eq!(
            preferred_fixture_site_distance_m(TemplateFamily::Outbreak),
            None
        );
        assert!(ordinary_generated_site_distance_m(0, 0) >= 4_000);
    }

    #[test]
    fn fresh_development_world_seeds_an_exact_order_errantry_issuer() {
        let order = adventuresim_core::organization::organization(ERRANTRY_ISSUER_ORGANIZATION_ID)
            .expect("Order catalog entry");
        assert!(order.errantry_issuance);
        let chapter = order.chapters.first().expect("authored Order chapter");
        let expected_representative =
            adventuresim_core::organization::organization_representative_id(
                &chapter.settlement_id,
                ERRANTRY_ISSUER_ORGANIZATION_ID,
            );
        assert_eq!(
            expected_representative,
            adventuresim_core::organization::organization_representative_id(
                &chapter.settlement_id,
                ERRANTRY_ISSUER_ORGANIZATION_ID,
            )
        );
        assert_ne!(
            expected_representative,
            adventuresim_core::organization::organization_representative_id(
                "different-settlement",
                ERRANTRY_ISSUER_ORGANIZATION_ID,
            )
        );

        let source = crate::production_source(include_str!("mission_bootstrap.rs"));
        let seed = source
            .split("fn seed_world_rows")
            .nth(1)
            .unwrap()
            .split("pub fn ensure_settlement_activity")
            .next()
            .unwrap();
        let canonical_chapter = seed
            .find("organization.chapters.first()")
            .expect("canonical Order chapter lookup");
        let settlement_insert = seed
            .find("ctx.db.settlement().insert")
            .expect("demo settlement insertion");
        let population = seed
            .find("ensure_settlement_activity_inner")
            .expect("canonical representative population seeding");
        assert!(canonical_chapter < settlement_insert && settlement_insert < population);
        let population_source = crate::production_source(include_str!("../settlement_population.rs"));
        let representative_seed = population_source
            .split("for organization in adventuresim_core::organization::organizations_for_chapter")
            .nth(1)
            .unwrap()
            .split("pub fn npc_is_present")
            .next()
            .unwrap();
        assert!(representative_seed.contains("organization_representative_id"));
        assert!(representative_seed.contains("representative.organization_id = organization.id"));
        assert!(representative_seed.contains("\"organization-representative\""));

        let standalone = source
            .split("pub fn seed_standalone_tactical_mission")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn seed_world")
            .next()
            .unwrap();
        assert!(standalone.contains("seed_tactical_host_world(ctx)"));
        let create_party = standalone
            .find("create_solo_party_for_character")
            .expect("standalone party creation");
        let retire_server = standalone
            .find("retire_interrupted_standalone_server_for_character")
            .expect("interrupted server retirement");
        let retire_request = standalone
            .find("retire_interrupted_standalone_requests")
            .expect("interrupted request retirement");
        let new_case_site = standalone
            .find("let case_site_id")
            .expect("fresh standalone case site");
        assert!(create_party < retire_server);
        assert!(retire_server < retire_request);
        assert!(retire_request < new_case_site);
        assert!(
            standalone.find("seed_tactical_host_world(ctx)").unwrap()
                < standalone.find("ctx.db.character().id()").unwrap()
        );
        let tactical_host = source
            .split("fn seed_tactical_host_world")
            .nth(1)
            .unwrap()
            .split("fn seed_world_rows")
            .next()
            .unwrap();
        assert!(tactical_host.contains("seed_world_rows(ctx, false, false)"));

        let bootstrap = source
            .split("pub fn bootstrap_development_world")
            .nth(1)
            .unwrap()
            .split("pub fn seed_standalone_tactical_mission")
            .next()
            .unwrap();
        assert!(
            bootstrap.find("seed_world(ctx, true)").unwrap()
                < bootstrap.find("seed_social_demo(ctx)").unwrap()
        );
        assert!(
            bootstrap.find("seed_social_demo(ctx)").unwrap()
                < bootstrap
                    .find("seed_herbalism_demo_character(ctx)")
                    .unwrap()
        );
        assert!(
            bootstrap
                .find("seed_herbalism_demo_character(ctx)")
                .unwrap()
                < bootstrap
                    .find("materialize_development_scenario_gallery(ctx)")
                    .unwrap()
        );
        assert!(
            include_str!("../simulation.rs").contains("crate::strategic::seed_world(ctx, false)")
        );

        let challenges = crate::production_source(include_str!("challenges.rs"));
        let gallery = crate::production_source(include_str!("development_scenarios.rs"));
        assert!(!challenges.contains("pub fn load_puzzle_demo"));
        assert!(gallery.contains("ErrantryLaunch::DirectDemoCamp(kind)"));
        assert!(gallery.contains("PuzzleKind::ResourceAllocation"));
        let materializer = challenges
            .split("fn materialize_order_errantry")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(materializer.contains("order_errantry_issuer(ctx)"));
        assert!(
            materializer.contains("issuer_organization_id: ERRANTRY_ISSUER_ORGANIZATION_ID.into()")
        );
    }

    #[test]
    fn debug_and_automatic_generation_share_materialization_without_disclosure() {
        let source = STRATEGIC_SOURCE;
        let automatic = source
            .split("fn generate_quest_for_settlement")
            .nth(1)
            .unwrap()
            .split("fn materialize_generated_quest")
            .next()
            .unwrap();
        let debug = source
            .split("pub fn spawn_developer_quest")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(automatic.contains("materialize_generated_quest"));
        assert!(automatic.contains("generated_witness_candidates"));
        assert!(debug.contains("materialize_generated_quest"));
        assert!(debug.contains("developer_witness_candidates"));
        assert!(!debug.contains("generated_witness_candidates"));
        assert!(debug.contains("current_settlement_id"));
        assert!(!debug.contains("rumor_receipt"));
        assert!(!debug.contains("referral"));
        assert!(!debug.contains("journal"));
        assert!(!debug.contains("case_site_pin"));
    }

    #[test]
    fn replay_compiles_the_persisted_developer_context() {
        let source = STRATEGIC_SOURCE;
        let validator = source
            .split("pub(crate) fn validate_quest_generation_authority")
            .nth(1)
            .unwrap()
            .split("/// A separately accepted agreement")
            .next()
            .unwrap();
        assert!(validator.contains("DeveloperGenerationContext"));
        assert!(validator.contains("developer_quest::compile"));
        assert!(validator.contains("regenerated != manifest"));
    }

    #[test]
    fn developer_witness_projection_matches_core_for_every_presentation() {
        use crate::personality::{Presentation, Sex};
        use crate::settlement_population::{
            NpcAgeBand, ResolvedSettlementResident, SettlementResidentPresence,
            SettlementResidentProfile,
        };
        use adventuresim_core::quest_generation::{
            VisibleWitnessCandidateInput, visible_witness_candidate,
        };

        let presence = SettlementResidentPresence {
            character_id: 42,
            settlement_id: "settlement:visible".into(),
            location_id: "market".into(),
            start_minute: 480,
            end_minute: 1_020,
            is_default: true,
            context_suppressed: false,
            health_suppressed: false,
        };
        for presentation in [
            Presentation::Man,
            Presentation::Woman,
            Presentation::Ambiguous,
        ] {
            let npc = ResolvedSettlementResident {
                profile: SettlementResidentProfile {
                    character_id: 42,
                    projection_id: 42,
                    home_settlement_id: "settlement:visible".into(),
                    height: "average height".into(),
                    build: "sturdy".into(),
                    hair: "brown hair".into(),
                    facial_hair: "none visible".into(),
                    complexion: "weathered".into(),
                    visible_features: "a scar".into(),
                    clothing: "a wool coat".into(),
                    profession: "laborer".into(),
                    household: "market household".into(),
                    local_role: "resident".into(),
                    service_id: String::new(),
                    organization_id: String::new(),
                    conversation_id: "local-resident".into(),
                },
                name: "Visible Witness".into(),
                age_band: NpcAgeBand::Adult,
                sex: Sex::Female,
                presentation,
            };
            let age_band = npc.age_band.stable_variant_id();
            let presentation = npc.presentation.stable_variant_id();
            let direct = visible_witness_candidate(VisibleWitnessCandidateInput {
                resident_character_id: npc.character_id,
                display_name: &npc.name,
                age_band,
                presentation,
                height: &npc.height,
                build: &npc.build,
                hair: &npc.hair,
                clothing: &npc.clothing,
                profession: &npc.profession,
                local_role: &npc.local_role,
                settlement_id: &presence.settlement_id,
                location_id: &presence.location_id,
                start_minute: presence.start_minute,
                end_minute: presence.end_minute,
                is_default: presence.is_default,
            })
            .unwrap();
            let authoritative = developer_npc_witness_candidate(&npc, &presence).unwrap();
            assert_eq!(authoritative, direct);
            assert!(authoritative.sex.is_empty());
        }
    }

    #[test]
    fn materializer_uses_exact_authored_custody_tuples() {
        let source = STRATEGIC_SOURCE;
        let custody = source
            .split("fn seed_case_custody")
            .nth(1)
            .unwrap()
            .split("fn party_strategic_minute")
            .next()
            .unwrap();
        assert!(custody.contains("authored_custody"));
        assert!(custody.contains("for (object_id, site_id) in authored_custody"));
        assert!(custody.contains("&site_id.0"));
        assert!(!custody.contains("SiteRole::Finale"));
        let materializer = source
            .split("fn materialize_generated_quest")
            .nth(1)
            .unwrap()
            .split("pub fn spawn_developer_quest")
            .next()
            .unwrap();
        assert!(materializer.contains("&generated.custody"));
    }

    #[test]
    fn hearing_referred_testimony_triggers_passive_insight_after_persistence() {
        let source = crate::production_source(include_str!("dialogue_effects.rs"));
        let effect = source
            .split("adventuresim_dialogue::Effect::ReceiveReferredTestimony =>")
            .nth(1)
            .and_then(|tail| {
                tail.split("adventuresim_dialogue::Effect::InvestigationAction")
                    .next()
            })
            .expect("referred testimony dialogue effect");
        let receive = effect
            .find("receive_referred_testimony")
            .expect("authoritative testimony persistence");
        let assess = effect
            .find("passively_assess_dialogue_witness")
            .expect("passive Insight assessment");
        assert!(receive < assess);
        assert!(effect.contains("?;"));
    }

    #[test]
    fn released_testimony_aligns_one_source_entry_per_resolved_fragment() {
        let source = STRATEGIC_SOURCE;
        let release = source
            .split("pub(crate) fn release_referred_withheld_testimony")
            .nth(1)
            .and_then(|tail| tail.split("fn resolve_dialogue_fragments").next())
            .expect("released testimony emitter");
        assert!(release.contains("fragments.len()"));
        assert!(release.contains("Option::<adventuresim_dialogue::SourceRef>::None"));
        assert!(!release.contains("source_refs_json: \"[null]\""));
    }

    #[test]
    fn gateway_battle_and_dialogue_options_expose_only_observer_case_ids() {
        let source = STRATEGIC_SOURCE;
        let battle = source
            .split("pub struct BackendCaseBattle")
            .nth(1)
            .and_then(|tail| tail.split("pub struct MissionAuthority").next())
            .expect("battle gateway projection");
        assert!(battle.contains("owner_character_id"));
        assert!(battle.contains("public_case_id"));
        assert!(!battle.contains("pub case_id:"));

        let options = source
            .split("pub struct BackendDialogueTopicOption")
            .nth(1)
            .and_then(|tail| tail.split("fn player_participant_ids").next())
            .expect("dialogue topic projection");
        assert!(options.contains("public_case_id"));
        let refresh = source
            .split("fn refresh_dialogue_topic_options")
            .nth(1)
            .and_then(|tail| tail.split("pub fn choose_dialogue_topic").next())
            .expect("dialogue topic refresh");
        assert!(refresh.contains("dialogue_public_case_id"));
    }

    #[test]
    fn organization_effects_resolve_only_from_the_bound_live_representative() {
        let source = crate::production_source(include_str!("dialogue_effects.rs"));
        let resolver = source
            .split("fn exact_organization_representative")
            .nth(1)
            .and_then(|tail| tail.split("fn dialogue_service_id").next())
            .expect("organization dialogue authority");
        assert!(resolver.contains("npc.organization_id"));
        assert!(resolver.contains("chapter_effective_location_id"));
        assert!(resolver.contains("organization_representative_id"));
        assert!(resolver.contains("session.settlement_id"));
        assert!(resolver.contains("session.location_id"));
        assert!(resolver.contains("exact_representative_fields_match"));

        let effects = source
            .split("adventuresim_dialogue::Effect::JoinOrganization =>")
            .nth(1)
            .and_then(|tail| {
                tail.split("adventuresim_dialogue::Effect::ReceiveReferredTestimony")
                    .next()
            })
            .expect("organization dialogue effects");
        assert!(effects.contains("dialogue_organization_id(ctx, session, &live_npc)"));
        assert!(!effects.contains("organization_id: String"));
    }

    #[test]
    fn committed_join_and_dues_answers_are_retry_safe_after_prompt_closes() {
        for (action_id, prompt_row_id) in [
            ("join-lost-response", "session:prompt:join"),
            ("dues-lost-response", "session:prompt:dues"),
        ] {
            let receipt = DialogueAction {
                id: format!("session:{action_id}"),
                session_id: "session".into(),
                action_id: action_id.into(),
                action_kind: format!("answer:{prompt_row_id}"),
                resulting_revision: 3,
            };
            assert!(
                dialogue_answer_is_committed_retry(Some(&receipt), "resolved", prompt_row_id)
                    .unwrap()
            );
        }
    }

    #[test]
    fn new_or_conflicting_action_against_closed_prompt_fails() {
        assert!(dialogue_answer_is_committed_retry(None, "resolved", "prompt").is_err());
        let conflicting = DialogueAction {
            id: "session:collision".into(),
            session_id: "session".into(),
            action_id: "collision".into(),
            action_kind: "topic".into(),
            resulting_revision: 2,
        };
        assert!(
            dialogue_answer_is_committed_retry(Some(&conflicting), "resolved", "prompt").is_err()
        );
        let other_prompt = DialogueAction {
            action_kind: "answer:other-prompt".into(),
            ..conflicting
        };
        assert!(
            dialogue_answer_is_committed_retry(Some(&other_prompt), "resolved", "prompt").is_err()
        );
    }

    #[test]
    fn prompt_retry_receipt_is_checked_only_after_session_and_role_scope() {
        let source = STRATEGIC_SOURCE;
        let answer = source
            .split("pub fn answer_dialogue_prompt")
            .nth(1)
            .and_then(|tail| tail.split("fn apply_dialogue_effect").next())
            .expect("answer dialogue reducer");
        let session_scope = answer.find("require_session_member").unwrap();
        let participant_scope = answer
            .find("Character is not an eligible respondent")
            .unwrap();
        let retry = answer.find("dialogue_answer_is_committed_retry").unwrap();
        assert!(session_scope < retry);
        assert!(participant_scope < retry);
    }
}
