#[reducer]
pub fn send_local_chat_message(
    ctx: &ReducerContext,
    sender_id: u64,
    subject_kind: String,
    subject_id: String,
    location_id: String,
    body: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, sender_id)?;
    let sender = ctx
        .db
        .character()
        .id()
        .find(sender_id)
        .ok_or("Sender not found")?;
    let body = body.trim();
    if body.is_empty() || body.chars().count() > 500 {
        return Err("Messages must contain 1 to 500 characters".into());
    }
    let (audience_party_id, other_party_id, npc_id) = match subject_kind.as_str() {
        "player" => {
            if !location_id.is_empty() {
                return Err("Player conversations do not accept an NPC location".into());
            }
            let (audience_party_id, other_party_id) = player_conversation_parties(
                ctx,
                &sender,
                subject_id.parse().map_err(|_| "Invalid player subject")?,
            )?;
            (audience_party_id, other_party_id, String::new())
        }
        "npc" => (
            npc_conversation_party(ctx, &sender, &subject_id, &location_id)?,
            String::new(),
            subject_id,
        ),
        _ => return Err("Unknown Local conversation subject".into()),
    };
    ctx.db.local_chat_message().insert(LocalChatMessage {
        id: 0,
        gateway_bucket: 0,
        audience_party_id,
        other_party_id,
        npc_id,
        sender_id,
        sender_name: sender.name,
        body: body.to_string(),
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    Ok(())
}

#[reducer]
pub fn request_party_action(
    ctx: &ReducerContext,
    requester_id: u64,
    action_kind: String,
    summary: String,
    payload: String,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, requester_id)?;
    let requester = ctx
        .db
        .character()
        .id()
        .find(requester_id)
        .ok_or("Character not found")?;
    let party_id = requester.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id == requester_id {
        return Err("The party leader does not need to request permission".into());
    }
    let allowed = [
        "travel",
        "kick",
        "add_role",
        "edit_role",
        "delete_role",
        "accept_join",
        "reject_join",
        "accept_contract",
        "abandon_contract",
        "report_contract",
        "autoresolve",
        "party_checks",
        "party_inventory",
        "disband_party",
        "initiate_combat",
        "cancel_mission",
        "investigate",
    ];
    if !allowed.contains(&action_kind.as_str()) {
        return Err("Unknown party action request".into());
    }
    // Travel destinations supersede one another. Inventory target/staging edits
    // are intentionally coalesced to one notification per requesting member.
    if action_kind == "travel" || action_kind == "party_inventory" {
        let old: Vec<_> = ctx
            .db
            .party_action_request_authority()
            .requester_id()
            .filter(requester_id)
            .filter(|request| request.party_id == party_id && request.action_kind == action_kind)
            .map(|request| request.id)
            .collect();
        for id in old {
            ctx.db.party_action_request_authority().id().delete(id);
        }
    }
    ctx.db
        .party_action_request_authority()
        .insert(PartyActionRequest {
            id: 0,
            gateway_bucket: 0,
            party_id,
            requester_id,
            action_kind,
            summary: summary.trim().to_string(),
            payload,
        });
    Ok(())
}

#[reducer]
pub fn dismiss_party_action_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, leader_id)?;
    let request = ctx
        .db
        .party_action_request_authority()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can resolve requests".into());
    }
    ctx.db
        .party_action_request_authority()
        .id()
        .delete(request_id);
    Ok(())
}

/// Atomically execute and resolve a member's approved action. SpacetimeDB
/// reducers are transactional, so a failed action leaves the request intact;
/// a committed request id is recorded to make retries idempotent.
#[reducer]
pub fn approve_party_action_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if let Some(resolved) = ctx.db.resolved_party_action().id().find(request_id) {
        if resolved.approved_by != leader_id {
            return Err("Only the party leader can approve requests".into());
        }
        return Ok(());
    }
    let request = ctx
        .db
        .party_action_request_authority()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can approve requests".into());
    }
    let action: ApprovedPartyAction = serde_json::from_str(&request.payload)
        .map_err(|error| format!("Invalid party action payload: {error}"))?;
    if action.kind() != request.action_kind {
        return Err("Party action kind does not match its typed payload".into());
    }
    action.execute(ctx, leader_id, request.requester_id)?;
    ctx.db.resolved_party_action().insert(ResolvedPartyAction {
        id: request.id,
        party_id: request.party_id,
        approved_by: leader_id,
    });
    ctx.db
        .party_action_request_authority()
        .id()
        .delete(request_id);
    Ok(())
}

#[reducer]
pub fn approve_party_action_request_planned(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
    route: JourneyRoutePlan,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if let Some(resolved) = ctx.db.resolved_party_action().id().find(request_id) {
        if resolved.approved_by != leader_id {
            return Err("Only the party leader can approve requests".into());
        }
        return Ok(());
    }
    let request = ctx
        .db
        .party_action_request_authority()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can resolve requests".into());
    }
    let action: ApprovedPartyAction = serde_json::from_str(&request.payload)
        .map_err(|error| format!("Invalid party action payload: {error}"))?;
    if action.kind() != request.action_kind {
        return Err("Party action kind does not match its typed payload".into());
    }
    match action {
        ApprovedPartyAction::TravelToSettlement { settlement_id } => {
            travel_to_settlement_impl(ctx, leader_id, settlement_id, Some(route))?
        }
        ApprovedPartyAction::TravelToCaseSite { case_site_id } => {
            travel_to_case_site_impl(ctx, leader_id, case_site_id, Some(route))?
        }
        _ => return Err("A planned approval is only valid for travel".into()),
    }
    ctx.db.resolved_party_action().insert(ResolvedPartyAction {
        id: request.id,
        party_id: request.party_id,
        approved_by: leader_id,
    });
    ctx.db
        .party_action_request_authority()
        .id()
        .delete(request_id);
    Ok(())
}

#[reducer]
pub fn vote_for_party_leader(
    ctx: &ReducerContext,
    voter_id: u64,
    candidate_id: u64,
) -> Result<(), String> {
    let voter = ctx
        .db
        .character()
        .id()
        .find(voter_id)
        .ok_or("Voter not found")?;
    if !voter.alive {
        return Err("Dead characters cannot vote".into());
    }
    let party_id = voter.party_id.ok_or("Voter has no party")?;
    require_no_unresolved_encounter(ctx, &party_id)?;
    ctx.db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    let candidate = ctx
        .db
        .character()
        .id()
        .find(candidate_id)
        .ok_or("Candidate not found")?;
    if !candidate.alive || candidate.party_id.as_deref() != Some(&party_id) {
        return Err("Candidate must be a living member of this party".into());
    }
    let id = format!("{party_id}:{voter_id}");
    let vote = PartyLeaderVote {
        id: id.clone(),
        party_id: party_id.clone(),
        voter_id,
        candidate_id,
    };
    if ctx.db.party_leader_vote().id().find(&id).is_some() {
        ctx.db.party_leader_vote().id().update(vote);
    } else {
        ctx.db.party_leader_vote().insert(vote);
    }
    normalize_and_elect_party_leader(ctx, &party_id)?;
    Ok(())
}

fn put_leader_vote(ctx: &ReducerContext, party_id: &str, voter_id: u64, candidate_id: u64) {
    let id = format!("{party_id}:{voter_id}");
    let row = PartyLeaderVote {
        id: id.clone(),
        party_id: party_id.to_string(),
        voter_id,
        candidate_id,
    };
    if ctx.db.party_leader_vote().id().find(&id).is_some() {
        ctx.db.party_leader_vote().id().update(row);
    } else {
        ctx.db.party_leader_vote().insert(row);
    }
}

/// Retire only strategic party state that can otherwise keep an all-dead
/// company travelling or accepting input. Tactical mission/server authority
/// and pooled party assets intentionally remain untouched.
pub(crate) fn teardown_all_dead_strategic_party(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    if !living_party_member_ids(ctx, party_id).is_empty() {
        return Err("Cannot retire strategic state for a party with living members".into());
    }
    let party_key = party_id.to_string();
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_key)
        .ok_or("Party not found")?;
    party.camp_destination = None;
    party.camp_remaining_minutes = 0;
    ctx.db.party_authority().id().update(party);

    finish_party_journey(ctx, party_id);
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db.strategic_encounter().party_id().delete(&party_key);
    }
    for row in ctx
        .db
        .party_action_request_authority()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_action_request_authority().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_leader_vote().id().delete(&row.id);
    }
    let member_ids: HashSet<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .map(|member| member.character_id)
        .collect();
    for role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        delete_recruitment_role_authority(ctx, role.id);
    }
    for row in ctx
        .db
        .party_join_request()
        .iter()
        .filter(|request| {
            request.party_id == party_id || member_ids.contains(&request.character_id)
        })
        .collect::<Vec<_>>()
    {
        ctx.db.party_join_request().id().delete(row.id);
    }
    if let Some(mut offer) = ctx
        .db
        .recruitment_offer()
        .recruiting_party_id()
        .find(&party_key)
    {
        offer.status = RecruitmentOfferStatus::Closed;
        ctx.db.recruitment_offer().id_key().update(offer);
    }
    Ok(())
}

/// Lazily backfills standing votes and discards stale legacy succession rows.
/// This is intentionally safe to call after every membership or life-state
/// transition, preserving non-destructive compatibility with existing parties.
pub(crate) fn normalize_and_elect_party_leader(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let living = living_party_member_ids(ctx, party_id);
    if living.is_empty() {
        if let Some(contract_id) = party.active_contract_id.take()
            && let Some(mut contract) = ctx.db.contract_authority().id().find(&contract_id)
            && matches!(
                contract.status,
                ContractStatus::Accepted | ContractStatus::ReadyToReport
            )
        {
            contract.status = ContractStatus::Withdrawn;
            ctx.db.contract_authority().id().update(contract);
        }
        party.current_case_site_id = None;
        ctx.db.party_authority().id().update(party);
        teardown_all_dead_strategic_party(ctx, party_id)?;
        return Ok(());
    }
    let living_set: std::collections::HashSet<_> = living.iter().copied().collect();
    for vote in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        if !living_set.contains(&vote.voter_id) || !living_set.contains(&vote.candidate_id) {
            ctx.db.party_leader_vote().id().delete(&vote.id);
        }
    }
    if living_set.contains(&party.leader_id) {
        for voter_id in &living {
            let id = format!("{party_id}:{voter_id}");
            if ctx.db.party_leader_vote().id().find(&id).is_none() {
                // New and legacy members begin by supporting the incumbent.
                put_leader_vote(ctx, party_id, *voter_id, party.leader_id);
            }
        }
    } else if let [sole_survivor] = living.as_slice() {
        // Ensure a sole survivor can complete succession without deadlocking.
        put_leader_vote(ctx, party_id, *sole_survivor, *sole_survivor);
    }
    let leader_alive = living_set.contains(&party.leader_id);
    let ballots: Vec<_> = ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .map(|vote| (vote.voter_id, vote.candidate_id))
        .collect();
    if let Some(next) = adventuresim_core::leadership::elect_leader(
        party.leader_id,
        leader_alive,
        &living,
        &ballots,
    ) {
        party.leader_id = next;
        party.is_solo = living.len() == 1;
        ctx.db.party_authority().id().update(party);
    }
    Ok(())
}

#[reducer]
pub fn update_character(ctx: &ReducerContext, id: u64, name: String) -> Result<(), String> {
    crate::character::require_living_character(ctx, id)?;
    let Some(mut character) = ctx.db.character().id().find(id) else {
        return Err("Character not found".into());
    };

    character.name = name;
    ctx.db.character().id().update(character);
    Ok(())
}

pub(crate) fn create_solo_party_for_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<String, String> {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let party_id = format!("solo-{character_id}");
    if ctx.db.party_authority().id().find(&party_id).is_none() {
        ctx.db.party_authority().insert(Party {
            id: party_id.clone(),
            gateway_bucket: 0,
            name: format!("{}'s party", character.name),
            leader_id: character_id,
            current_settlement_id: character.current_settlement_id.clone(),
            current_case_site_id: crate::investigation::character_case_site_id(ctx, character_id)
                .map(CaseSiteId::from),
            active_contract_id: None,
            is_solo: true,
            camp_fatigue_percent: 50,
            walking_minutes_per_day: DEFAULT_WALKING_MINUTES_PER_DAY,
            travel_at_night: false,
            camp_duration_mode: CampDurationMode::Auto,
            fixed_camp_minutes: 0,
            camp_destination: None,
            camp_remaining_minutes: 0,
            pooled_water_ml: 0.0,
            physiology_target: 0.0,
            command_target: 0.0,
            religion_target: 0.0,
        });
        ctx.db.party_member().insert(PartyMember {
            id: 0,
            party_id: party_id.clone(),
            character_id,
            role: Some("Leader".into()),
            recruitment_role_id: None,
        });
        put_leader_vote(ctx, &party_id, character_id, character_id);
    }
    character.party_id = Some(party_id.clone());
    ctx.db.character().id().update(character);
    crate::social::reset_familiarity_after_join(ctx, character_id);
    normalize_and_elect_party_leader(ctx, &party_id)?;
    Ok(party_id)
}

/// Remove the isolated party created for a temporary tactical character.
/// Refuse to delete a party that has acquired any other member.
pub(crate) fn delete_temporary_character_party(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
) -> Result<(), String> {
    let party_key = party_id.to_string();
    let members: Vec<_> = ctx.db.party_member().party_id().filter(party_id).collect();
    if members
        .iter()
        .any(|member| member.character_id != character_id)
    {
        return Err("Temporary character party contains another member".into());
    }
    for member in members {
        ctx.db.party_member().id().delete(member.id);
    }
    for row in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_leader_vote().id().delete(&row.id);
    }
    for row in ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_stake().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        if let Some(condition) = ctx
            .db
            .party_item_condition()
            .party_inventory_item_id()
            .find(row.id)
        {
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(condition.party_inventory_item_id);
        }
        ctx.db.party_inventory_item().id().delete(row.id);
    }
    if ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db.party_inventory_state().party_id().delete(&party_key);
    }
    if ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db
            .party_journey_authority()
            .party_id()
            .delete(&party_key);
    }
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party_key);
    }
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db
            .party_journey_itinerary()
            .party_id()
            .delete(&party_key);
    }
    for row in ctx
        .db
        .party_action_request_authority()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_action_request_authority().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_join_request()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_join_request().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_recruitment_role().id().delete(row.id);
    }
    ctx.db.party_authority().id().delete(&party_key);
    Ok(())
}

/// Move a deterministic development fixture into another fixture's party
/// without going through the player-facing recruitment workflow.
pub(crate) fn attach_seeded_party_member(
    ctx: &ReducerContext,
    leader_id: u64,
    member_id: u64,
    role: &str,
) -> Result<(), String> {
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Seed party leader not found")?;
    let party_id = leader
        .party_id
        .clone()
        .ok_or("Seed party leader has no party")?;
    let mut member = ctx
        .db
        .character()
        .id()
        .find(member_id)
        .ok_or("Seed party member not found")?;

    if member.party_id.as_deref() == Some(&party_id) {
        if let Some(mut membership) = ctx
            .db
            .party_member()
            .character_id()
            .filter(member_id)
            .find(|membership| membership.party_id == party_id)
        {
            membership.role = Some(role.into());
            ctx.db.party_member().id().update(membership);
        }
        return Ok(());
    }

    if let Some(source_party_id) = member.party_id.clone() {
        let source_members: Vec<_> = ctx
            .db
            .party_member()
            .party_id()
            .filter(&source_party_id)
            .collect();
        if source_members
            .iter()
            .any(|membership| membership.character_id != member_id)
        {
            return Err("Seed party member belongs to a non-solo party".into());
        }
        for membership in source_members {
            ctx.db.party_member().id().delete(membership.id);
        }
        for vote in ctx
            .db
            .party_leader_vote()
            .party_id()
            .filter(&source_party_id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_leader_vote().id().delete(&vote.id);
        }
        ctx.db.party_authority().id().delete(&source_party_id);
    }

    member.party_id = Some(party_id.clone());
    member.current_settlement_id = leader.current_settlement_id.clone();
    ctx.db.character().id().update(member);
    crate::investigation::set_character_case_site(
        ctx,
        member_id,
        crate::investigation::character_case_site_id(ctx, leader_id),
    );
    crate::social::reset_familiarity_after_join(ctx, member_id);
    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: party_id.clone(),
        character_id: member_id,
        role: Some(role.into()),
        recruitment_role_id: None,
    });
    put_leader_vote(ctx, &party_id, member_id, leader_id);
    if let Some(mut party) = ctx.db.party_authority().id().find(&party_id) {
        party.is_solo = false;
        ctx.db.party_authority().id().update(party);
    }
    normalize_and_elect_party_leader(ctx, &party_id)?;
    Ok(())
}

#[reducer]
pub fn backfill_solo_parties(ctx: &ReducerContext) -> Result<(), String> {
    let ids: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|c| c.party_id.is_none())
        .map(|c| c.id)
        .collect();
    for id in ids {
        create_solo_party_for_character(ctx, id)?;
    }
    Ok(())
}

#[reducer]
pub fn create_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    name: String,
    quantity: u32,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
    save_role: bool,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if quantity == 0 || quantity > 8 {
        return Err("Role quantity must be between 1 and 8".into());
    }
    if !(0.0..=adventuresim_core::capability::WEAPON_PRECISION_RAPIER).contains(&weapon_precision)
        || (weapon_precision * 2.0).fract() != 0.0
    {
        return Err("Weapon precision must use a 0.5 step between 0 and 2".into());
    }
    if [
        requirements.athletics,
        requirements.endurance,
        requirements.physiology,
        requirements.surgery,
        requirements.command,
        requirements.religion,
    ]
    .iter()
    .any(|v| *v > 5)
    {
        return Err("Role ratings must be between 0 and 5".into());
    }
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can create roles".into());
    }
    let role_name = if name.trim().is_empty() {
        "Any adventurer".to_string()
    } else {
        name.trim().to_string()
    };
    ctx.db
        .party_recruitment_role()
        .insert(PartyRecruitmentRole {
            id: 0,
            party_id,
            name: role_name.clone(),
            requirements,
            quantity,
            weapon_precision,
        });
    if save_role {
        if name.trim().is_empty() {
            return Err("Name a role before saving it".into());
        }
        ctx.db
            .saved_recruitment_role()
            .insert(SavedRecruitmentRole {
                id: 0,
                owner_character_id: leader_id,
                name: role_name,
                requirements,
                weapon_precision,
            });
    }
    Ok(())
}

#[reducer]
pub fn update_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    role_id: u64,
    name: String,
    quantity: u32,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if quantity > 8 {
        return Err("Role quantity must be between 0 and 8".into());
    }
    validate_recruitment_requirements(requirements, weapon_precision)?;
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can edit roles".into());
    }
    let mut role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Recruitment role not found")?;
    if role.party_id != party_id {
        return Err("Cannot edit another party's role".into());
    }
    let filled = filled_role_slots(ctx, role_id);
    if quantity < filled {
        return Err(format!("This role already has {filled} filled slots"));
    }
    let role_name = if name.trim().is_empty() {
        "Any adventurer".to_string()
    } else {
        name.trim().to_string()
    };
    role.name = role_name.clone();
    role.quantity = quantity;
    role.requirements = requirements;
    role.weapon_precision = weapon_precision;
    ctx.db.party_recruitment_role().id().update(role);
    for mut member in ctx
        .db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .collect::<Vec<_>>()
    {
        member.role = Some(role_name.clone());
        ctx.db.party_member().id().update(member);
    }
    Ok(())
}

#[reducer]
pub fn delete_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    role_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can delete roles".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Recruitment role not found")?;
    if role.party_id != party_id {
        return Err("Cannot delete another party's role".into());
    }
    delete_recruitment_role_authority(ctx, role_id);
    Ok(())
}

fn delete_recruitment_role_authority(ctx: &ReducerContext, role_id: u64) {
    for request in ctx
        .db
        .party_join_request()
        .recruitment_role_id()
        .filter(role_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_join_request().id().delete(request.id);
    }
    for mut member in ctx
        .db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .collect::<Vec<_>>()
    {
        member.role = None;
        member.recruitment_role_id = None;
        ctx.db.party_member().id().update(member);
    }
    ctx.db.party_recruitment_role().id().delete(role_id);
}

#[reducer]
pub fn save_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    name: String,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, owner_id)?;
    crate::character::require_living_character(ctx, owner_id)?;
    if ctx.db.character().id().find(owner_id).is_none() {
        return Err("Character not found".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("Saved roles must have a name".into());
    }
    validate_recruitment_requirements(requirements, weapon_precision)?;
    ctx.db
        .saved_recruitment_role()
        .insert(SavedRecruitmentRole {
            id: 0,
            owner_character_id: owner_id,
            name: name.to_string(),
            requirements,
            weapon_precision,
        });
    Ok(())
}

#[reducer]
pub fn rename_saved_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    role_id: u64,
    name: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, owner_id)?;
    crate::character::require_living_character(ctx, owner_id)?;
    let mut role = ctx
        .db
        .saved_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Saved role not found")?;
    if role.owner_character_id != owner_id {
        return Err("Cannot rename another character's saved role".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("Saved roles must have a name".into());
    }
    role.name = name.to_string();
    ctx.db.saved_recruitment_role().id().update(role);
    Ok(())
}

fn validate_recruitment_requirements(
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
) -> Result<(), String> {
    if !(0.0..=adventuresim_core::capability::WEAPON_PRECISION_RAPIER).contains(&weapon_precision)
        || (weapon_precision * 2.0).fract() != 0.0
    {
        return Err("Weapon precision must use a 0.5 step between 0 and 2".into());
    }
    if [requirements.athletics, requirements.endurance]
        .iter()
        .any(|value| *value > 5)
    {
        return Err("Role ratings must be between 0 and 5".into());
    }
    Ok(())
}

#[reducer]
pub fn update_party_check_targets(
    ctx: &ReducerContext,
    leader_id: u64,
    physiology: f32,
    command: f32,
    religion: f32,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if [physiology, command, religion]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=5.0).contains(&value) || value.fract() != 0.0)
    {
        return Err("Party check targets must be whole numbers between 0 and 5".into());
    }
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can configure party checks".into());
    }
    party.physiology_target = physiology;
    party.command_target = command;
    party.religion_target = religion;
    ctx.db.party_authority().id().update(party);
    Ok(())
}

#[reducer]
pub fn delete_saved_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    role_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, owner_id)?;
    crate::character::require_living_character(ctx, owner_id)?;
    let role = ctx
        .db
        .saved_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Saved role not found")?;
    if role.owner_character_id != owner_id {
        return Err("Cannot delete another character's saved role".into());
    }
    ctx.db.saved_recruitment_role().id().delete(role_id);
    Ok(())
}

fn filled_role_slots(ctx: &ReducerContext, role_id: u64) -> u32 {
    ctx.db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .count() as u32
}

fn role_requirements(
    role: &PartyRecruitmentRole,
) -> adventuresim_core::capability::RoleRequirements {
    let mut requirements = adventuresim_core::capability::RoleRequirements::from(role.requirements);
    requirements.weapon_precision = requirements.weapon_precision.max(role.weapon_precision);
    requirements.physiology = 0;
    requirements.surgery = 0;
    requirements.command = 0;
    requirements.religion = 0;
    requirements
}

#[derive(Clone, Copy)]
struct RecruitmentOfferBindingFields<'a> {
    party_leader_id: u64,
    party_settlement_id: Option<&'a str>,
    leader_alive: bool,
    leader_party_id: Option<&'a str>,
    leader_settlement_id: Option<&'a str>,
    npc_home_settlement_id: Option<&'a str>,
    presence_settlement_id: Option<&'a str>,
    presence_location_id: Option<&'a str>,
    presence_is_current: bool,
}

fn recruitment_offer_binding_fields_are_live(
    offer: &RecruitmentOffer,
    fields: RecruitmentOfferBindingFields<'_>,
) -> bool {
    offer.leader_id == fields.party_leader_id
        && fields.leader_alive
        && fields.leader_party_id == Some(offer.recruiting_party_id.as_str())
        && fields.party_settlement_id == Some(offer.settlement_id.as_str())
        && fields.leader_settlement_id == Some(offer.settlement_id.as_str())
        && fields.npc_home_settlement_id == Some(offer.settlement_id.as_str())
        && fields.presence_settlement_id == Some(offer.settlement_id.as_str())
        && fields.presence_location_id == Some(offer.location_id.as_str())
        && fields.presence_is_current
}

fn recruitment_offer_bindings_are_live(
    ctx: &ReducerContext,
    offer: &RecruitmentOffer,
    now: u64,
) -> bool {
    let Some(party) = ctx
        .db
        .party_authority()
        .id()
        .find(&offer.recruiting_party_id)
    else {
        return false;
    };
    let Some(leader) = ctx.db.character().id().find(offer.leader_id) else {
        return false;
    };
    let npc = ctx.db.settlement_npc().id().find(&offer.settlement_npc_id);
    let presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&offer.settlement_npc_id);
    recruitment_offer_binding_fields_are_live(
        offer,
        RecruitmentOfferBindingFields {
            party_leader_id: party.leader_id,
            party_settlement_id: party.current_settlement_id.as_deref(),
            leader_alive: leader.alive,
            leader_party_id: leader.party_id.as_deref(),
            leader_settlement_id: leader.current_settlement_id.as_deref(),
            npc_home_settlement_id: npc.as_ref().map(|row| row.home_settlement_id.as_str()),
            presence_settlement_id: presence.as_ref().map(|row| row.settlement_id.as_str()),
            presence_location_id: presence.as_ref().map(|row| row.location_id.as_str()),
            presence_is_current: presence
                .as_ref()
                .is_some_and(|row| crate::settlement_population::npc_is_present(row, now)),
        },
    )
}

fn require_open_recruitment_offer(
    ctx: &ReducerContext,
    party: &Party,
) -> Result<Option<RecruitmentOffer>, String> {
    let Some(leader) = ctx.db.character().id().find(party.leader_id) else {
        return Err("Recruiting party leader not found".into());
    };
    let offer = ctx
        .db
        .recruitment_offer()
        .recruiting_party_id()
        .find(&party.id);
    let Some(mut offer) = offer else {
        return if leader.temporary {
            Err("NPC company has no recruitment authority".into())
        } else {
            Ok(None)
        };
    };
    let now = crate::time::refresh_clock(ctx)?;
    if offer.status != RecruitmentOfferStatus::Open {
        return Err("This recruitment offer is no longer open".into());
    }
    let bindings_are_live = recruitment_offer_bindings_are_live(ctx, &offer, now);
    let refreshed = refreshed_recruitment_offer_status(
        offer.status,
        now,
        offer.expires_at_minute,
        bindings_are_live,
    );
    if refreshed != RecruitmentOfferStatus::Open {
        // A valid generated offer remains Open-but-expired until settlement
        // activity renews it in place. Persisting Expired here would strand
        // its unique NPC/party identity and prevent that renewal.
        if refreshed == RecruitmentOfferStatus::Closed {
            offer.status = refreshed;
            ctx.db.recruitment_offer().id_key().update(offer);
        }
        return Err(if refreshed == RecruitmentOfferStatus::Expired {
            "This recruitment offer has expired".into()
        } else {
            "Recruiting company's advertised identity or presence is stale".into()
        });
    }
    Ok(Some(offer))
}

fn refreshed_recruitment_offer_status(
    current: RecruitmentOfferStatus,
    now: u64,
    expires_at: u64,
    bindings_are_live: bool,
) -> RecruitmentOfferStatus {
    if current != RecruitmentOfferStatus::Open {
        current
    } else if !bindings_are_live {
        RecruitmentOfferStatus::Closed
    } else if now >= expires_at {
        RecruitmentOfferStatus::Expired
    } else {
        RecruitmentOfferStatus::Open
    }
}

fn require_living_recruitment_target(ctx: &ReducerContext, party: &Party) -> Result<(), String> {
    let leader = ctx
        .db
        .character()
        .id()
        .find(party.leader_id)
        .ok_or("Recruiting party leader not found")?;
    if !leader.alive
        || leader.party_id.as_deref() != Some(party.id.as_str())
        || !living_party_member_ids(ctx, &party.id).contains(&party.leader_id)
    {
        return Err("Cannot join a party without a living leader".into());
    }
    Ok(())
}

#[reducer]
pub fn request_to_join_party(
    ctx: &ReducerContext,
    character_id: u64,
    recruitment_role_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let current_party_id = character.party_id.clone().ok_or("Character has no party")?;
    let current_party = ctx
        .db
        .party_authority()
        .id()
        .find(&current_party_id)
        .ok_or("Current party not found")?;
    if current_party.leader_id != character_id {
        return Err("Only a party leader may request a party merge".into());
    }
    if current_party.active_contract_id.is_some() {
        return Err("Abandon the current quest before joining another party".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    let party_id = role.party_id.clone();
    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if current_party_id == party_id {
        return Err("Cannot join your own party".into());
    }
    require_living_recruitment_target(ctx, &party)?;
    require_open_recruitment_offer(ctx, &party)?;
    if !crate::simulation::same_simulation_scope(ctx, character_id, party.leader_id) {
        return Err("Simulation and ordinary parties cannot merge".into());
    }
    if current_party.current_settlement_id != party.current_settlement_id
        || current_party.current_case_site_id != party.current_case_site_id
    {
        return Err("Parties must be in the same location to merge".into());
    }
    if role.quantity > 0 && filled_role_slots(ctx, role.id) >= role.quantity {
        return Err("Recruitment role is full".into());
    }
    if ctx
        .db
        .party_join_request()
        .character_id()
        .filter(character_id)
        .any(|request| request.recruitment_role_id == recruitment_role_id)
    {
        return Ok(());
    }
    let capabilities = crate::capability::refresh_character_capability(ctx, character_id)?;
    ctx.db.party_join_request().insert(PartyJoinRequest {
        id: 0,
        party_id,
        recruitment_role_id,
        character_id,
        meets_requirements: capabilities.meets(role_requirements(&role)),
    });
    Ok(())
}

#[reducer]
pub fn request_general_party_join(
    ctx: &ReducerContext,
    character_id: u64,
    target_party_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    let target_party = ctx
        .db
        .party_authority()
        .id()
        .find(&target_party_id)
        .ok_or("Party not found")?;
    require_living_recruitment_target(ctx, &target_party)?;
    let role = ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&target_party_id)
        .find(|role| role.quantity == 0 && role.name == "Unassigned")
        .unwrap_or_else(|| {
            ctx.db
                .party_recruitment_role()
                .insert(PartyRecruitmentRole {
                    id: 0,
                    party_id: target_party_id.clone(),
                    name: "Unassigned".into(),
                    requirements: RecruitmentRequirements::default(),
                    quantity: 0,
                    weapon_precision: 0.0,
                })
        });
    request_to_join_party(ctx, character_id, role.id)
}

#[reducer]
pub fn accept_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party_authority().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    let recruitment_offer = require_open_recruitment_offer(ctx, &party)?;
    require_no_unresolved_encounter(ctx, &request.party_id)?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can accept join requests".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(request.recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    if role.quantity > 0 && filled_role_slots(ctx, role.id) >= role.quantity {
        return Err("Recruitment role is full".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(request.character_id)
        .ok_or("Applicant not found")?;
    let source_party_id = character.party_id.clone().ok_or("Applicant has no party")?;
    require_no_unresolved_encounter(ctx, &source_party_id)?;
    let source_party = ctx
        .db
        .party_authority()
        .id()
        .find(&source_party_id)
        .ok_or("Applicant party not found")?;
    if source_party.leader_id != request.character_id {
        return Err("Applicant is no longer their party leader".into());
    }
    if !crate::simulation::same_simulation_scope(ctx, request.character_id, leader_id) {
        return Err("Simulation and ordinary parties cannot merge".into());
    }
    if source_party.active_contract_id.is_some() {
        return Err("Applicant's party must abandon its current quest first".into());
    }
    if source_party.current_settlement_id != party.current_settlement_id
        || source_party.current_case_site_id != party.current_case_site_id
    {
        return Err("Parties must be in the same location to merge".into());
    }

    // Preserve the source party's jointly-owned assets and each member's absolute
    // stake. Combining the ledgers does not dilute either party; only future loot
    // is shared among the newly combined membership.
    for mut entry in ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        if item_is_durable(ctx, &entry.item_id) {
            entry.party_id = request.party_id.clone();
            ctx.db.party_inventory_item().id().update(entry);
        } else {
            add_to_party_inventory(ctx, &request.party_id, &entry.item_id, entry.quantity);
            ctx.db.party_inventory_item().id().delete(entry.id);
        }
    }
    for stake in ctx
        .db
        .party_stake()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        credit_party_stake(ctx, &request.party_id, stake.character_id, stake.value)?;
        ctx.db.party_stake().id().delete(stake.id);
    }
    if let Some(state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&source_party_id)
    {
        credit_party_reserve(ctx, &request.party_id, state.reserve_value)?;
        ctx.db
            .party_inventory_state()
            .party_id()
            .delete(&source_party_id);
    }

    let source_members: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&source_party_id)
        .collect();
    let source_member_ids: Vec<_> = source_members
        .iter()
        .map(|member| member.character_id)
        .collect();
    if source_member_ids.iter().any(|member_id| {
        ctx.db
            .character()
            .id()
            .find(*member_id)
            .is_some_and(|character| !character.alive)
    }) {
        return Err("A party containing dead members cannot merge".into());
    }
    for member in source_members {
        ctx.db.party_member().id().delete(member.id);
        ctx.db.party_member().insert(PartyMember {
            id: 0,
            party_id: request.party_id.clone(),
            character_id: member.character_id,
            role: if member.character_id == request.character_id {
                Some(role.name.clone())
            } else {
                member.role
            },
            recruitment_role_id: (member.character_id == request.character_id).then_some(role.id),
        });
        if let Some(mut source_character) = ctx.db.character().id().find(member.character_id) {
            source_character.party_id = Some(request.party_id.clone());
            source_character.current_settlement_id = party.current_settlement_id.clone();
            ctx.db.character().id().update(source_character);
            crate::investigation::set_character_case_site(
                ctx,
                member.character_id,
                party.current_case_site_id.clone().map(|id| id.value),
            );
            crate::social::reset_familiarity_after_join(ctx, member.character_id);
        }
    }

    // Incoming applications and recruitment roles belonged to the source party,
    // so they cannot survive after its leader relinquishes command.
    for source_role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        for pending in ctx
            .db
            .party_join_request()
            .recruitment_role_id()
            .filter(source_role.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
        ctx.db.party_recruitment_role().id().delete(source_role.id);
    }
    for member_id in &source_member_ids {
        for pending in ctx
            .db
            .party_join_request()
            .character_id()
            .filter(*member_id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
    }
    ctx.db.party_authority().id().delete(&source_party_id);
    for old_vote in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_leader_vote().id().delete(&old_vote.id);
    }
    for member_id in &source_member_ids {
        put_leader_vote(ctx, &request.party_id, *member_id, party.leader_id);
    }
    if party.is_solo {
        let mut party = party;
        party.is_solo = false;
        ctx.db.party_authority().id().update(party);
    }
    let requests: Vec<_> = ctx
        .db
        .party_join_request()
        .character_id()
        .filter(request.character_id)
        .collect();
    for pending in requests {
        ctx.db.party_join_request().id().delete(pending.id);
    }
    if role.quantity > 0 && filled_role_slots(ctx, role.id) >= role.quantity {
        for pending in ctx
            .db
            .party_join_request()
            .recruitment_role_id()
            .filter(role.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
    }
    if let Some(mut offer) = recruitment_offer {
        let has_open_role = ctx
            .db
            .party_recruitment_role()
            .party_id()
            .filter(&request.party_id)
            .any(|candidate| {
                candidate.quantity == 0 || filled_role_slots(ctx, candidate.id) < candidate.quantity
            });
        if !has_open_role {
            offer.status = RecruitmentOfferStatus::Closed;
            ctx.db.recruitment_offer().id_key().update(offer);
        }
    }
    normalize_and_elect_party_leader(ctx, &request.party_id)?;
    Ok(())
}

#[reducer]
pub fn reject_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party_authority().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can reject join requests".into());
    }
    ctx.db.party_join_request().id().delete(request_id);
    Ok(())
}
