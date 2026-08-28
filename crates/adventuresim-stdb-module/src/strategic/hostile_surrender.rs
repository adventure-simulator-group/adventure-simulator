#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HostileSurrenderMode {
    Demand,
    Offer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HostileSurrenderOutcome {
    Refused,
    Declined,
    Accepted,
}

#[derive(Clone, Debug)]
#[table(accessor = hostile_surrender_receipt)]
pub struct HostileSurrenderReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub actor_id: u64,
    pub party_id: String,
    pub case_site_id: CaseSiteId,
    pub hostile_group_id: String,
    pub spokesman_id: u64,
    pub context_ref: String,
    pub expected_revision: u32,
    pub mode: HostileSurrenderMode,
    pub player_accepted_offer: Option<bool>,
    pub outcome: HostileSurrenderOutcome,
    pub response: String,
    pub occurred_at_minute: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendHostileSurrender {
    pub owner_character_id: u64,
    pub case_site_id: String,
    pub spokesman_id: u64,
    pub context_ref: String,
    pub expected_revision: u32,
    pub mode: HostileSurrenderMode,
    pub latest_response: Option<String>,
}

fn surrender_awareness_for_case(ctx: &ReducerContext, case: &CaseAuthority) -> u16 {
    case.local_problem_id
        .as_ref()
        .and_then(|id| ctx.db.local_problem_authority().id().find(id))
        .map_or(0, |problem| problem.public_awareness_bps)
}

fn surrender_awareness_for_group_view(
    ctx: &ViewContext,
    group: &HostileGroupAuthority,
) -> u16 {
    ctx.db
        .case_site_authority()
        .id_key()
        .find(&group.case_site_id.value)
        .and_then(|site| ctx.db.case_authority().id().find(&site.case_id))
        .and_then(|case| case.local_problem_id)
        .and_then(|id| ctx.db.local_problem_authority().id().find(&id))
        .map_or(0, |problem| problem.public_awareness_bps)
}

fn surrender_offer_elected(
    language: f32,
    morale: u8,
    awareness_bps: u16,
) -> bool {
    adventuresim_core::strategic_action::assess_hostile_surrender(
        0.0,
        language,
        0.0,
        morale,
        awareness_bps,
    )
    .offers_surrender
}

fn elected_surrender_mode(offers_surrender: bool) -> HostileSurrenderMode {
    if offers_surrender {
        HostileSurrenderMode::Offer
    } else {
        HostileSurrenderMode::Demand
    }
}

#[view(accessor = backend_hostile_surrenders, public)]
pub fn backend_hostile_surrenders(ctx: &ViewContext) -> Vec<BackendHostileSurrender> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for party in ctx.db.party_authority().gateway_bucket().filter(0u8) {
        let Some(case_site_id) = party.current_case_site_id.as_ref() else {
            continue;
        };
        let Some(group) = ctx
            .db
            .hostile_group_authority()
            .case_site_id_key()
            .find(&case_site_id.value)
            .filter(|group| group.disposition == HostileGroupDisposition::Active)
        else {
            continue;
        };
        let Ok(threat) = parse_threat(&group.enemy_type) else {
            continue;
        };
        let profile = threat.profile();
        if !profile.negotiation.sapient
            || !profile.negotiation.negotiable
            || bound_mission_for_view(ctx, &party.id, &group.id)
            || !current_drive_off_capability_for_view(
                ctx,
                party.leader_id,
                &party.id,
                &group,
                HostileResolutionKind::Surrendered,
            )
        {
            continue;
        }
        let Some(minute) = ctx
            .db
            .character_time()
            .character_id()
            .find(party.leader_id)
            .map(|time| time.minutes)
        else {
            continue;
        };
        let Some(spokesman) = exact_spokesman_for_view(ctx, &group, minute) else {
            continue;
        };
        let language = view_shared_language_coefficient(ctx, party.leader_id, spokesman.character_id);
        if language <= 0.0 {
            continue;
        }
        let mode = elected_surrender_mode(surrender_offer_elected(
            language,
            profile.combat.morale,
            surrender_awareness_for_group_view(ctx, &group),
        ));
        let latest_response = ctx
            .db
            .hostile_surrender_receipt()
            .actor_id()
            .filter(party.leader_id)
            .filter(|receipt| receipt.hostile_group_id == group.id)
            .max_by_key(|receipt| receipt.occurred_at_minute)
            .map(|receipt| receipt.response);
        rows.push(BackendHostileSurrender {
            owner_character_id: party.leader_id,
            case_site_id: case_site_id.value.clone(),
            spokesman_id: spokesman.character_id,
            context_ref: HOSTILE_NEGOTIATION_CONTEXT_REF.into(),
            expected_revision: spokesman.revision,
            mode,
            latest_response,
        });
    }
    rows
}

struct SurrenderRequest {
    actor_id: u64,
    case_site_id: String,
    spokesman_id: u64,
    context_ref: String,
    expected_revision: u32,
    action_id: String,
    mode: HostileSurrenderMode,
    player_accepted_offer: Option<bool>,
}

fn resolve_hostile_surrender(ctx: &ReducerContext, request: SurrenderRequest) -> Result<(), String> {
    require_strategic_character_authority(ctx, request.actor_id)?;
    if request.action_id.is_empty() || request.action_id.len() > 160 {
        return Err("Hostile surrender action ID is invalid".into());
    }
    let receipt_id = format!("hostile-surrender:{}:{}", request.actor_id, request.action_id);
    if let Some(existing) = ctx.db.hostile_surrender_receipt().id().find(&receipt_id) {
        return if existing.actor_id == request.actor_id
            && existing.case_site_id.value == request.case_site_id
            && existing.spokesman_id == request.spokesman_id
            && existing.context_ref == request.context_ref
            && existing.expected_revision == request.expected_revision
            && existing.mode == request.mode
            && existing.player_accepted_offer == request.player_accepted_offer
        {
            Ok(())
        } else {
            Err("Conflicting hostile surrender retry".into())
        };
    }
    let (party, group, site) = exact_hostile_negotiation_authority(
        ctx,
        request.actor_id,
        &request.case_site_id,
        request.spokesman_id,
        &request.context_ref,
        request.expected_revision,
        HostileResolutionKind::Surrendered,
    )?;
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&site.case_id)
        .ok_or("Surrender case authority is unavailable")?;
    let profile = parse_threat(&group.enemy_type)?.profile();
    let language = crate::character::shared_language_coefficient(
        ctx,
        request.actor_id,
        request.spokesman_id,
    );
    let affinity = crate::social::current_affinity(ctx, request.spokesman_id, request.actor_id);
    let social_ability = crate::condition::mental_check(
        ctx,
        request.actor_id,
        adventuresim_core::skill::Skill::Charm,
    )?
    .max(crate::condition::mental_check(
        ctx,
        request.actor_id,
        adventuresim_core::skill::Skill::Command,
    )?);
    let assessment = adventuresim_core::strategic_action::assess_hostile_surrender(
        social_ability,
        language,
        affinity,
        profile.combat.morale,
        surrender_awareness_for_case(ctx, &case),
    );
    let elected_mode = elected_surrender_mode(assessment.offers_surrender);
    if request.mode != elected_mode {
        return Err("Hostile surrender mode is stale or forged".into());
    }
    let accepted = match request.mode {
        HostileSurrenderMode::Demand => {
            if request.player_accepted_offer.is_some() {
                return Err("A surrender demand cannot include an offer answer".into());
            }
            assessment.accepts_demand
        }
        HostileSurrenderMode::Offer => {
            if !assessment.offers_surrender {
                return Err("The hostile surrender offer is no longer available".into());
            }
            request
                .player_accepted_offer
                .ok_or("A surrender offer requires an answer")?
        }
    };
    let (outcome, response) = if accepted {
        let (_, current_group, current_site) = exact_hostile_negotiation_authority(
            ctx,
            request.actor_id,
            &request.case_site_id,
            request.spokesman_id,
            &request.context_ref,
            request.expected_revision,
            HostileResolutionKind::Surrendered,
        )?;
        commit_hostile_resolution_authority(
            ctx,
            HostileResolutionCommit {
                receipt_id: &format!("{receipt_id}:resolution"),
                party_id: &party.id,
                mission_id: None,
                hostile_group_id: &current_group.id,
                observer_character_id: request.actor_id,
                case_id: &current_site.case_id,
                case_site_id: &current_site.id,
                resolution: HostileResolutionKind::Surrendered,
                capture_subject_id: None,
            },
        )?;
        (
            HostileSurrenderOutcome::Accepted,
            "The hostile group yields as a whole and submits without battle.".into(),
        )
    } else if request.mode == HostileSurrenderMode::Offer {
        (
            HostileSurrenderOutcome::Declined,
            "You decline the hostile group's offer. The group and every available approach remain unchanged."
                .into(),
        )
    } else {
        crate::social::put_affinity(
            ctx,
            request.spokesman_id,
            request.actor_id,
            affinity - 1.0,
        );
        (
            HostileSurrenderOutcome::Refused,
            "The hostile spokesman refuses your surrender demand. The group remains active and every approach remains available."
                .into(),
        )
    };
    let occurred_at_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(request.actor_id)
        .ok_or("Surrender actor has no personal time")?
        .minutes;
    ctx.db.hostile_surrender_receipt().insert(HostileSurrenderReceipt {
        id: receipt_id,
        actor_id: request.actor_id,
        party_id: party.id,
        case_site_id: group.case_site_id,
        hostile_group_id: group.id,
        spokesman_id: request.spokesman_id,
        context_ref: request.context_ref,
        expected_revision: request.expected_revision,
        mode: request.mode,
        player_accepted_offer: request.player_accepted_offer,
        outcome,
        response,
        occurred_at_minute,
    });
    Ok(())
}

#[reducer]
pub fn demand_hostile_surrender(
    ctx: &ReducerContext,
    actor_id: u64,
    case_site_id: String,
    spokesman_id: u64,
    context_ref: String,
    expected_revision: u32,
    action_id: String,
) -> Result<(), String> {
    resolve_hostile_surrender(
        ctx,
        SurrenderRequest {
            actor_id,
            case_site_id,
            spokesman_id,
            context_ref,
            expected_revision,
            action_id,
            mode: HostileSurrenderMode::Demand,
            player_accepted_offer: None,
        },
    )
}

#[reducer]
#[expect(
    clippy::too_many_arguments,
    reason = "the reducer ABI exposes each independently validated surrender field"
)]
pub fn answer_hostile_surrender_offer(
    ctx: &ReducerContext,
    actor_id: u64,
    case_site_id: String,
    spokesman_id: u64,
    context_ref: String,
    expected_revision: u32,
    accept: bool,
    action_id: String,
) -> Result<(), String> {
    resolve_hostile_surrender(
        ctx,
        SurrenderRequest {
            actor_id,
            case_site_id,
            spokesman_id,
            context_ref,
            expected_revision,
            action_id,
            mode: HostileSurrenderMode::Offer,
            player_accepted_offer: Some(accept),
        },
    )
}

#[cfg(test)]
mod hostile_surrender_source_tests {
    use super::{HostileSurrenderMode, elected_surrender_mode, surrender_offer_elected};

    #[test]
    fn one_live_mode_is_elected_from_authored_offer_policy() {
        assert_eq!(elected_surrender_mode(false), HostileSurrenderMode::Demand);
        assert_eq!(elected_surrender_mode(true), HostileSurrenderMode::Offer);
        assert!(surrender_offer_elected(1.0, 50, 6_000));
        assert!(!surrender_offer_elected(1.0, 70, 6_000));
    }

    #[test]
    fn surrender_is_exact_precombat_and_battle_independent() {
        let source = crate::production_source(include_str!("hostile_surrender.rs"));
        assert!(source.contains("HostileResolutionKind::Surrendered"));
        assert!(source.contains("exact_hostile_negotiation_authority"));
        assert!(source.contains("expected_revision"));
        assert!(source.contains("assessment.offers_surrender"));
        assert!(source.contains("request.mode != elected_mode"));
        assert!(source.contains("HostileSurrenderOutcome::Declined"));
        let declined = source
            .split("HostileSurrenderOutcome::Declined")
            .next()
            .expect("player-declined offer branch");
        assert!(!declined.rsplit("} else if").next().unwrap().contains("put_affinity"));
        assert!(source.contains("every available approach remain unchanged"));
        assert!(source.contains("put_affinity"));
        assert!(!source.contains("BattleResult"));
        assert!(!source.contains("battle_loot_item"));
        assert!(!source.contains("record_morale_event"));
        assert!(!source.contains("case_custody"));
    }
}
