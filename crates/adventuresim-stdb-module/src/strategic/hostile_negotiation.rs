use crate::character::character_attributes__view as _;
use crate::character::character__view as _;
use crate::time::character_time__view as _;
use crate::world_actor::character_context_membership as _;

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
    let rows = ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(&group.id)
        .filter(|row| {
            row.context_kind == crate::world_actor::CharacterContextKind::HostileGroup
                && row.role == crate::world_actor::CharacterContextRole::Counterparty
                && row.location_id == group.case_site_id.value
                && crate::world_actor::context_membership_valid_at(row, minute)
                && ctx
                    .db
                    .character()
                    .id()
                    .find(row.character_id)
                    .is_some_and(|character| character.alive)
                && crate::world_actor::character_alive_at_for_view(ctx, row.character_id, minute)
        })
        .collect();
    unique_lowest_ordinal_spokesman(rows)
}

fn unique_lowest_ordinal_spokesman(
    rows: Vec<crate::world_actor::CharacterContextMembership>,
) -> Option<crate::world_actor::CharacterContextMembership> {
    unique_lowest_ordinal_value(rows.into_iter().map(|row| (row.ordinal, row)))
}

fn unique_lowest_ordinal_value<T>(rows: impl IntoIterator<Item = (u16, T)>) -> Option<T> {
    let mut selected: Option<(u16, T)> = None;
    let mut ambiguous = false;
    for (ordinal, value) in rows {
        match selected.as_ref().map(|(selected_ordinal, _)| *selected_ordinal) {
            None => selected = Some((ordinal, value)),
            Some(selected_ordinal) if ordinal < selected_ordinal => {
                selected = Some((ordinal, value));
                ambiguous = false;
            }
            Some(selected_ordinal) if ordinal == selected_ordinal => ambiguous = true,
            Some(_) => {}
        }
    }
    (!ambiguous).then(|| selected.map(|(_, value)| value)).flatten()
}

fn bound_mission_for_view(
    ctx: &ViewContext,
    party_id: &str,
    hostile_group_id: &str,
) -> bool {
    ctx.db
        .mission_authority()
        .party_id()
        .filter(&party_id.to_string())
        .any(|mission| {
            mission.status == MissionAttemptStatus::Bound
                && mission.hostile_group_id.as_deref() == Some(hostile_group_id)
        })
}

fn bound_mission(
    ctx: &ReducerContext,
    party_id: &str,
    hostile_group_id: &str,
) -> bool {
    ctx.db
        .mission_authority()
        .party_id()
        .filter(&party_id.to_string())
        .any(|mission| {
            mission.status == MissionAttemptStatus::Bound
                && mission.hostile_group_id.as_deref() == Some(hostile_group_id)
        })
}

fn current_drive_off_capability_for_view(
    ctx: &ViewContext,
    observer_character_id: u64,
    party_id: &str,
    group: &HostileGroupAuthority,
) -> bool {
    let Some(site) = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&group.case_site_id.value)
        .filter(|site| site.id == group.case_site_id)
    else {
        return false;
    };
    ctx.db
        .mission_approach_capability()
        .observer_character_id()
        .filter(observer_character_id)
        .any(|capability| {
            capability.active
                && capability.hostile_group_id == group.id
                && capability.case_id == site.case_id
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
                            && view_capability_objective_is_pending(
                                ctx,
                                &case,
                                &capability,
                                party_id,
                            )
                    })
        })
}

fn view_capability_objective_is_pending(
    ctx: &ViewContext,
    case: &CaseAuthority,
    capability: &MissionApproachCapability,
    party_id: &str,
) -> bool {
    let Ok(expression) = serde_json::from_str::<adventuresim_core::case::ObjectiveExpression>(
        &case.objective_expression_json,
    ) else {
        return false;
    };
    let Ok(core_case_id) = adventuresim_core::case::CaseId::new(case.id.clone()) else {
        return false;
    };
    let Some(facts) = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| serde_json::from_str(&row.fact_json).ok())
        .collect::<Option<Vec<adventuresim_core::case::OutcomeFact>>>()
    else {
        return false;
    };
    let Some(path) = expression.alternatives.get(usize::from(capability.path_index)) else {
        return false;
    };
    let Some(objective_index) = path
        .objectives
        .iter()
        .position(|objective| objective.id.as_str() == capability.objective_id)
    else {
        return false;
    };
    expression
        .evaluate(&core_case_id, party_id, &facts)
        .alternatives
        .get(usize::from(capability.path_index))
        .and_then(|path| path.get(objective_index))
        .is_some_and(|progress| progress.state == adventuresim_core::case::EvaluationState::Pending)
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
            || bound_mission_for_view(ctx, &party.id, &group.id)
            || !current_drive_off_capability_for_view(ctx, party.leader_id, &party.id, &group)
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
) -> Result<(Party, HostileGroupAuthority, CaseSiteAuthority), String> {
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
    if bound_mission(ctx, &party.id, &group.id) {
        return Err("Combat is already in flight for this hostile group".into());
    }
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&case_site_id.to_string())
        .filter(|site| site.id == group.case_site_id)
        .ok_or("Hostile case-site authority is unavailable")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .ok_or("Negotiation actor has no personal time")?
        .minutes;
    let spokesman = unique_lowest_ordinal_spokesman(
        ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(&group.id)
        .filter(|membership| {
            membership.location_id == case_site_id
                && membership.context_kind == crate::world_actor::CharacterContextKind::HostileGroup
                && membership.role == crate::world_actor::CharacterContextRole::Counterparty
                && crate::world_actor::context_membership_valid_at(membership, minute)
                && ctx
                    .db
                    .character()
                    .id()
                    .find(membership.character_id)
                    .is_some_and(|character| character.alive)
                && crate::relationship::character_alive_at(ctx, membership.character_id, minute)
        })
        .collect(),
    )
        .filter(|membership| {
            membership.character_id == spokesman_id && membership.revision == expected_revision
        })
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
                && capability.case_id == site.case_id
                && capability.hostile_group_id == group.id
                && capability.case_site_id == group.case_site_id
                && capability.resolution == HostileResolutionKind::DrivenOff
                && crate::strategic::inventory_trade::mission_approach_capability_is_pending(
                    ctx,
                    &capability,
                    &party.id,
                )
                .unwrap_or(false)
        });
    eligible
        .then_some((party, group, site))
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
    let (party, group, _site) = exact_hostile_negotiation_authority(
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
    );
    let (outcome, response) = if assessment.accepted {
        // Re-resolve all mutable pre-combat authority immediately before the
        // shared transition. This closes the projection/assessment race.
        let (_, current_group, current_site) = exact_hostile_negotiation_authority(
            ctx,
            actor_id,
            &case_site_id,
            spokesman_id,
            &context_ref,
            expected_revision,
        )?;
        commit_hostile_resolution_authority(
            ctx,
            HostileResolutionCommit {
                receipt_id: &format!("{receipt_id}:resolution"),
                party_id: &party.id,
                mission_id: None,
                hostile_group_id: &current_group.id,
                observer_character_id: actor_id,
                case_id: &current_site.case_id,
                case_site_id: &current_site.id,
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
        .ok_or("Negotiation actor has no personal time")?
        .minutes;
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
    use super::unique_lowest_ordinal_value;

    #[test]
    fn spokesman_selection_skips_ineligible_rows_before_choosing_and_rejects_ties() {
        // Living/role/context filtering happens before this shared pure selector.
        assert_eq!(
            unique_lowest_ordinal_value([(4, "living-four"), (7, "living-seven")]),
            Some("living-four")
        );
        assert_eq!(unique_lowest_ordinal_value([(4, "a"), (4, "b")]), None);
        assert_eq!(unique_lowest_ordinal_value::<&str>([]), None);
    }

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
        assert!(source.contains("CharacterContextRole::Counterparty"));
        assert!(source.contains("unique_lowest_ordinal_spokesman"));
        assert!(source.contains("character_alive_at"));
        assert!(source.contains("membership.revision == expected_revision"));
        assert!(source.contains("profile.negotiation.sapient"));
        assert!(source.contains("profile.negotiation.negotiable"));
        assert!(source.contains("shared_language_coefficient"));
        assert!(source.contains("CaseResolutionStatus::Open"));
        assert!(source.contains("MissionAttemptStatus::Bound"));
        assert!(source.contains("mission_approach_capability_is_pending"));
        assert!(source.contains("case_id == site.case_id"));
    }
}
