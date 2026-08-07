/// Returns the living members who participate in strategic party activity.
/// Membership rows for dead characters remain durable, but corpses never
/// advance time, travel, consume provisions, affect readiness, or enter combat.
pub(crate) fn living_party_member_ids(ctx: &ReducerContext, party_id: &str) -> Vec<u64> {
    let mut character_ids: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .filter_map(|membership| {
            ctx.db
                .character()
                .id()
                .find(membership.character_id)
                .filter(|character| character.alive)
                .map(|character| character.id)
        })
        .collect();
    character_ids.sort_unstable();
    character_ids
}

pub(crate) fn require_party_ready(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let character_ids = living_party_member_ids(ctx, party_id);
    if character_ids.is_empty() {
        return Err("Party has no living members".into());
    }
    crate::condition::require_characters_ready(ctx, &character_ids)
}

pub(crate) fn character_is_publicly_ready_party_member(
    ctx: &ReducerContext,
    party: &Party,
    character_id: u64,
) -> bool {
    ctx.db
        .party_member()
        .party_id()
        .filter(&party.id)
        .any(|membership| membership.character_id == character_id)
        && ctx
            .db
            .character()
            .id()
            .find(character_id)
            .is_some_and(|character| character.alive)
        && ctx
            .db
            .character_strategic_condition()
            .character_id()
            .find(character_id)
            .is_some_and(|condition| condition.status == "ready")
        && !ctx
            .db
            .character_illness_status()
            .character_id()
            .find(character_id)
            .is_some_and(|illness| illness.symptomatic || illness.critical)
}

/// Exact server-side counterpart of the simulator's public readiness filter.
/// The result remains an aggregate; it never exposes an individual member's
/// condition or capability outside ordinary authorized projections.
pub(crate) fn publicly_ready_party_combat_power(
    ctx: &ReducerContext,
    party: &Party,
) -> Result<(u32, u64), String> {
    ctx.db
        .party_member()
        .party_id()
        .filter(&party.id)
        .filter(|member| character_is_publicly_ready_party_member(ctx, party, member.character_id))
        .try_fold((0u32, 0u64), |(count, total), member| {
            let capability = ctx
                .db
                .character_capability()
                .character_id()
                .find(member.character_id)
                .ok_or("Ready party member has no combat assessment")?;
            if !(capability.melee || capability.ranged) {
                return Ok((count, total));
            }
            let count = count
                .checked_add(1)
                .ok_or("Ready party combatant count overflow")?;
            let total = total
                .checked_add(capability.autoresolve_combat_power)
                .ok_or("Ready party combat assessment overflow")?;
            Ok((count, total))
        })
}

fn party_leader_is_publicly_ready(ctx: &ReducerContext, party: &Party) -> bool {
    let leader_is_alive = ctx
        .db
        .character()
        .id()
        .find(party.leader_id)
        .is_some_and(|character| character.alive);
    let leader_condition_ready = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(party.leader_id)
        .is_some_and(|condition| condition.status == "ready");
    let leader_is_ready = leader_is_alive
        && leader_condition_ready
        && !ctx
            .db
            .character_illness_status()
            .character_id()
            .find(party.leader_id)
            .is_some_and(|illness| illness.symptomatic || illness.critical);
    leader_is_ready
}

fn ready_companion_may_direct_recovery(
    ctx: &ReducerContext,
    party: &Party,
    character_id: u64,
) -> bool {
    character_id != party.leader_id
        && character_is_publicly_ready_party_member(ctx, party, character_id)
        && !party_leader_is_publicly_ready(ctx, party)
}

pub(crate) fn party_member_can_direct_field_rest(
    ctx: &ReducerContext,
    party: &Party,
    character_id: u64,
) -> bool {
    party.leader_id == character_id
        || (party.current_settlement_id.is_none()
            && ready_companion_may_direct_recovery(ctx, party, character_id))
}

fn authoritative_evacuation_settlement(ctx: &ReducerContext, party: &Party) -> Option<String> {
    if let Some(site_id) = party.current_case_site_id.as_ref() {
        return ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&site_id.value)
            .map(|site| site.origin_settlement_id);
    }
    let journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party.id)?;
    match (&journey.origin, &journey.destination) {
        (JourneyEndpoint::CaseSite(_), JourneyEndpoint::Settlement(destination)) => {
            Some(destination.id.clone())
        }
        (JourneyEndpoint::Settlement(origin), JourneyEndpoint::Settlement(destination))
            if origin.id == destination.id =>
        {
            Some(destination.id.clone())
        }
        (JourneyEndpoint::Camp(origin_party_id), JourneyEndpoint::Settlement(destination))
            if origin_party_id == &party.id =>
        {
            Some(destination.id.clone())
        }
        (JourneyEndpoint::Settlement(origin), _) => Some(origin.id.clone()),
        _ => None,
    }
}

fn ready_companion_may_start_evacuation(
    ctx: &ReducerContext,
    party: &Party,
    character_id: u64,
    settlement_id: &str,
) -> bool {
    party.current_settlement_id.is_none()
        && ready_companion_may_direct_recovery(ctx, party, character_id)
        && authoritative_evacuation_settlement(ctx, party).as_deref() == Some(settlement_id)
}

fn ready_companion_may_continue_evacuation(
    ctx: &ReducerContext,
    party: &Party,
    character_id: u64,
) -> bool {
    if !ready_companion_may_direct_recovery(ctx, party, character_id) {
        return false;
    }
    let Some(JourneyEndpoint::Settlement(camp_destination)) = party.camp_destination.as_ref()
    else {
        return false;
    };
    authoritative_evacuation_settlement(ctx, party).as_deref() == Some(camp_destination.id.as_str())
}
