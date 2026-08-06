use crate::character::character_attributes__view as _;
use crate::time::character_time__view as _;
use crate::world_actor::{
    character_context_membership as _, character_context_membership__view as _,
};

const HOSTILE_NEGOTIATION_CONTEXT_REF: &str = "exact_case_context";

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HostileNegotiationOutcome {
    Refused,
    Accepted,
}

#[derive(Clone, Debug)]
#[table(accessor = hostile_negotiation_receipt)]
pub struct HostileNegotiationReceipt {
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
    pub outcome: HostileNegotiationOutcome,
    pub response: String,
    pub occurred_at_minute: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendHostileNegotiation {
    pub owner_character_id: u64,
    pub case_site_id: String,
    pub spokesman_id: u64,
    pub context_ref: String,
    pub expected_revision: u32,
    pub decision: crate::world_actor::BackendContextualDecision,
    pub latest_response: Option<String>,
}

fn view_shared_language_coefficient(
    ctx: &ViewContext,
    left_id: u64,
    right_id: u64,
) -> f32 {
    let Some(left) = ctx.db.character_skills().character_id().find(left_id) else {
        return 0.0;
    };
    let Some(right) = ctx.db.character_skills().character_id().find(right_id) else {
        return 0.0;
    };
    let left_cap = ctx
        .db
        .character_attributes()
        .character_id()
        .find(left_id)
        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    let right_cap = ctx
        .db
        .character_attributes()
        .character_id()
        .find(right_id)
        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    adventuresim_world_schema::best_common_oral_language_capped(
        left.oral_languages,
        left_cap,
        right.oral_languages,
        right_cap,
    )
    .1
}

fn exact_spokesman_for_view(
    ctx: &ViewContext,
    group: &HostileGroupAuthority,
    minute: u64,
) -> Option<crate::world_actor::CharacterContextMembership> {
    let mut rows = ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(&group.id)
        .filter(|row| {
            row.context_kind == crate::world_actor::CharacterContextKind::HostileGroup
                && row.ordinal == 0
                && crate::world_actor::context_membership_valid_at(row, minute)
        });
    let row = rows.next()?;
    rows.next().is_none().then_some(row)
}

fn current_drive_off_capability_for_view(
    ctx: &ViewContext,
    observer_character_id: u64,
    group: &HostileGroupAuthority,
) -> bool {
    ctx.db
        .mission_approach_capability()
        .observer_character_id()
        .filter(observer_character_id)
        .any(|capability| {
            capability.active
                && capability.hostile_group_id == group.id
                && capability.case_site_id == group.case_site_id
                && capability.resolution == HostileResolutionKind::DrivenOff
                && ctx
                    .db
                    .case_authority()
                    .id()
                    .find(&capability.case_id)
                    .is_some_and(|case| {
                        !case.generated_case_id.is_empty()
                            && case.resolution_status == CaseResolutionStatus::Open
                    })
        })
}

/// Gateway-only, party-scoped availability. Private group and case IDs never
/// cross the projection; the reducer re-resolves them from the exact spokesman,
/// public site, fixed context discriminator, and membership revision.
#[view(accessor = backend_hostile_negotiations, public)]
pub fn backend_hostile_negotiations(ctx: &ViewContext) -> Vec<BackendHostileNegotiation> {
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
        let profile = match parse_threat(&group.enemy_type) {
            Ok(threat) => threat.profile(),
            Err(_) => continue,
        };
        if !profile.negotiation.sapient
            || !profile.negotiation.negotiable
            || !current_drive_off_capability_for_view(ctx, party.leader_id, &group)
        {
            continue;
        }
        let minute = ctx
            .db
            .character_time()
            .character_id()
            .find(party.leader_id)
            .map_or(0, |time| time.minutes);
        let Some(spokesman) = exact_spokesman_for_view(ctx, &group, minute) else {
            continue;
        };
        if view_shared_language_coefficient(ctx, party.leader_id, spokesman.character_id) <= 0.0 {
            continue;
        }
        let latest_response = ctx
            .db
            .hostile_negotiation_receipt()
            .actor_id()
            .filter(party.leader_id)
            .filter(|receipt| receipt.hostile_group_id == group.id)
            .max_by_key(|receipt| receipt.occurred_at_minute)
            .map(|receipt| receipt.response);
        rows.push(BackendHostileNegotiation {
            owner_character_id: party.leader_id,
            case_site_id: case_site_id.value.clone(),
            spokesman_id: spokesman.character_id,
            context_ref: HOSTILE_NEGOTIATION_CONTEXT_REF.into(),
            expected_revision: spokesman.revision,
            decision: crate::world_actor::BackendContextualDecision::Request,
            latest_response,
        });
    }
    rows
}

fn exact_hostile_negotiation_authority(
    ctx: &ReducerContext,
    actor_id: u64,
    case_site_id: &str,
    spokesman_id: u64,
    context_ref: &str,
    expected_revision: u32,
) -> Result<(Party, HostileGroupAuthority), String> {
    if context_ref != HOSTILE_NEGOTIATION_CONTEXT_REF {
        return Err("Hostile negotiation context is unavailable".into());
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .filter(|actor| actor.alive)
        .ok_or("Negotiation actor is unavailable")?;
    let party_id = actor.party_id.ok_or("Negotiation requires an active party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .filter(|party| {
            party.leader_id == actor_id
                && party.current_case_site_id.as_ref().map(|site| site.value.as_str())
                    == Some(case_site_id)
        })
        .ok_or("Only the present party leader may negotiate")?;
    let group = ctx
        .db
        .hostile_group_authority()
        .case_site_id_key()
        .find(&case_site_id.to_string())
        .filter(|group| group.disposition == HostileGroupDisposition::Active)
        .ok_or("Active hostile group is unavailable")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .ok_or("Negotiation actor has no personal time")?
        .minutes;
    let mut spokesmen = ctx
        .db
        .character_context_membership()
        .character_id()
        .filter(spokesman_id)
        .filter(|membership| {
            membership.context_id == group.id
                && membership.location_id == case_site_id
                && membership.context_kind == crate::world_actor::CharacterContextKind::HostileGroup
                && membership.ordinal == 0
                && membership.revision == expected_revision
                && crate::world_actor::context_membership_valid_at(membership, minute)
        });
    let spokesman = spokesmen
        .next()
        .filter(|_| spokesmen.next().is_none())
        .ok_or("Hostile spokesman claim is stale or ambiguous")?;
    let profile = parse_threat(&group.enemy_type)?.profile();
    if !profile.negotiation.sapient || !profile.negotiation.negotiable {
        return Err("Hostile group is not available for negotiation".into());
    }
    if crate::character::shared_language_coefficient(ctx, actor_id, spokesman.character_id) <= 0.0 {
        return Err("No shared spoken language is available".into());
    }
    let eligible = ctx
        .db
        .mission_approach_capability()
        .observer_character_id()
        .filter(actor_id)
        .any(|capability| {
            capability.active
                && capability.hostile_group_id == group.id
                && capability.case_site_id == group.case_site_id
                && capability.resolution == HostileResolutionKind::DrivenOff
                && ctx
                    .db
                    .case_authority()
                    .id()
                    .find(&capability.case_id)
                    .is_some_and(|case| {
                        !case.generated_case_id.is_empty()
                            && case.resolution_status == CaseResolutionStatus::Open
                    })
        });
    eligible
        .then_some((party, group))
        .ok_or_else(|| "Hostile group has no current negotiated drive-off approach".into())
}

#[reducer]
pub fn negotiate_hostile_withdrawal(
    ctx: &ReducerContext,
    actor_id: u64,
    case_site_id: String,
    spokesman_id: u64,
    context_ref: String,
    expected_revision: u32,
    action_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Hostile negotiation action ID is invalid".into());
    }
    let receipt_id = format!("hostile-negotiation:{actor_id}:{action_id}");
    if let Some(existing) = ctx
        .db
        .hostile_negotiation_receipt()
        .id()
        .find(&receipt_id)
    {
        return if existing.actor_id == actor_id
            && existing.case_site_id.value == case_site_id
            && existing.spokesman_id == spokesman_id
            && existing.context_ref == context_ref
            && existing.expected_revision == expected_revision
        {
            Ok(())
        } else {
            Err("Conflicting hostile negotiation retry".into())
        };
    }
    let (party, group) = exact_hostile_negotiation_authority(
        ctx,
        actor_id,
        &case_site_id,
        spokesman_id,
        &context_ref,
        expected_revision,
    )?;
    let social_ability = crate::condition::mental_check(
        ctx,
        actor_id,
        adventuresim_core::skill::Skill::Charm,
    )?
    .max(crate::condition::mental_check(
        ctx,
        actor_id,
        adventuresim_core::skill::Skill::Command,
    )?);
    let language = crate::character::shared_language_coefficient(ctx, actor_id, spokesman_id);
    let affinity = crate::social::current_affinity(ctx, spokesman_id, actor_id);
    let profile = parse_threat(&group.enemy_type)?.profile();
    let assessment = adventuresim_core::strategic_action::assess_negotiated_withdrawal(
        social_ability,
        language,
        affinity,
        profile.combat.morale,
        group.base_enemy_count,
        group.enemy_count,
    );
    let (outcome, response) = if assessment.accepted {
        commit_hostile_resolution_authority(
            ctx,
            HostileResolutionCommit {
                receipt_id: &format!("{receipt_id}:resolution"),
                party_id: &party.id,
                mission_id: None,
                hostile_group_id: &group.id,
                resolution: HostileResolutionKind::DrivenOff,
                capture_subject_id: None,
            },
        )?;
        (
            HostileNegotiationOutcome::Accepted,
            "The spokesman accepts the terms and the hostile group withdraws.".to_string(),
        )
    } else {
        (
            HostileNegotiationOutcome::Refused,
            "The spokesman refuses. The hostile group remains in place; changed circumstances may alter a later answer."
                .to_string(),
        )
    };
    let occurred_at_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .map_or(0, |time| time.minutes);
    ctx.db
        .hostile_negotiation_receipt()
        .insert(HostileNegotiationReceipt {
            id: receipt_id,
            actor_id,
            party_id: party.id,
            case_site_id: group.case_site_id,
            hostile_group_id: group.id,
            spokesman_id,
            context_ref,
            expected_revision,
            outcome,
            response,
            occurred_at_minute,
        });
    Ok(())
}

#[cfg(test)]
mod hostile_negotiation_source_tests {
    #[test]
    fn refusal_is_non_terminal_and_acceptance_uses_battle_independent_drive_off() {
        let source = include_str!("hostile_negotiation.rs");
        let refusal = source
            .split("HostileNegotiationOutcome::Refused")
            .nth(1)
            .expect("refusal branch");
        assert!(!refusal.split("};").next().unwrap().contains("commit_hostile_resolution_authority"));
        assert!(source.contains("HostileResolutionKind::DrivenOff"));
        assert!(source.contains("commit_hostile_resolution_authority"));
        assert!(!source.contains("BattleResult"));
        assert!(!source.contains("battle_loot_item"));
        assert!(!source.contains("record_morale_event"));
    }

    #[test]
    fn projection_and_reducer_bind_exact_private_hostile_context() {
        let source = include_str!("hostile_negotiation.rs");
        assert!(source.contains("strategic_view_is_gateway(ctx)"));
        assert!(source.contains("HOSTILE_NEGOTIATION_CONTEXT_REF"));
        assert!(source.contains("membership.ordinal == 0"));
        assert!(source.contains("membership.revision == expected_revision"));
        assert!(source.contains("profile.negotiation.sapient"));
        assert!(source.contains("profile.negotiation.negotiable"));
        assert!(source.contains("shared_language_coefficient"));
        assert!(source.contains("CaseResolutionStatus::Open"));
    }
}
