//! Authoritative strategic NPC-adventurer interventions for old generated cases.

use crate::{
    investigation::{case_site_authority, investigation_action_outcome, record_journal_notice},
    local_problem::{
        LocalProblemOutcomeInput, apply_outcome, local_problem_authority,
        local_problem_authority__view, local_problem_receipt,
    },
    settlement_population::settlement_npc,
    strategic::{
        CaseResolutionStatus, HostileGroupDisposition, case_authority, case_authority__view,
        case_custody, hostile_group_authority, ingest_case_outcome_fact, party_authority,
        quest_generation_authority, quest_generation_authority__view, record_asset_retrieved,
        record_asset_returned_or_exchanged, record_subject_rescued_or_released,
        strategic_gateway_authority__view, validate_quest_generation_authority,
    },
};
use adventuresim_core::{
    case::{ObjectiveRequirement, OutcomeFactKind},
    npc_adventurer::{
        NpcApproachResolution, NpcCaseSnapshot, NpcInterventionDecision, NpcInterventionOutcome,
        NpcInterventionStrategy, NpcInvestigationApproach, NpcPartySnapshot, case_is_eligible,
        decide_after_supported_approach, resolve_investigation_approach, scripted_strategy,
        select_investigation_approach_after, select_party, supported_investigation_approaches,
        update_party_availability,
    },
    quest_generation::GeneratedCase,
    settlement_population::stable_hash,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};
use std::fmt::Write;

#[derive(Clone, Debug)]
#[table(accessor = npc_adventuring_party_authority)]
pub struct NpcAdventuringPartyAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub name: String,
    pub member_npc_ids_json: String,
    pub capability: u16,
    pub available_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = npc_case_intervention)]
pub struct NpcCaseIntervention {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub public_case_id: String,
    pub problem_id: String,
    pub party_id: String,
    pub attempt: u16,
    #[index(btree)]
    pub started_at: u64,
    pub completed_at: u64,
    pub strategy: String,
    pub route: String,
    pub lead_summary: String,
    pub preparation_summary: String,
    pub action_plan_json: String,
    pub outcome: String,
    pub mitigation_bps: u16,
    pub next_retry_at: u64,
    pub safe_summary: String,
    /// Observer-safe story emitted by the authoritative server action. This
    /// contains only dialogue and events the simulated company encountered.
    pub public_story_markdown: String,
}

#[derive(Clone, Debug)]
#[table(accessor = npc_intervention_strategy_override)]
pub struct NpcInterventionStrategyOverride {
    #[primary_key]
    pub case_id: String,
    pub strategy: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendNpcCaseIntervention {
    pub intervention_id: String,
    pub public_case_id: String,
    pub party_name: String,
    pub attempt: u16,
    pub started_at: u64,
    pub completed_at: u64,
    pub strategy: String,
    pub route: String,
    pub lead_summary: String,
    pub preparation_summary: String,
    pub outcome: String,
    pub safe_summary: String,
    pub public_story_markdown: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendNpcInterventionCandidate {
    pub case_id: String,
    pub public_case_id: String,
    pub settlement_id: String,
    pub problem_summary: String,
    pub incident_count: u16,
    pub earliest_intervention_minute: u64,
    pub party_id: String,
    pub party_name: String,
    pub party_capability: u16,
    pub strategy_already_selected: bool,
    pub legal_strategies_json: String,
}

fn view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

/// Observer-safe pending decisions for an external scripted or LLM policy.
/// The opaque simulation client may choose only one advertised strategy; it
/// receives no cause, true site, witness sincerity, weights, or factor trace.
#[view(accessor = backend_npc_intervention_candidates, public)]
pub fn backend_npc_intervention_candidates(
    ctx: &ViewContext,
) -> Vec<BackendNpcInterventionCandidate> {
    if !view_is_gateway(ctx) {
        return Vec::new();
    }
    let legal_strategies_json = serde_json::to_string(&[
        "investigate_carefully",
        "protect_locals",
        "confront_directly",
        "defer",
    ])
    .expect("static strategy names serialize");
    let mut rows = Vec::new();
    for authority in ctx.db.quest_generation_authority().seed().filter(0u64..) {
        let Ok(validated) = validate_quest_generation_authority(&authority) else {
            continue;
        };
        let Some(case) = ctx.db.case_authority().id().find(&authority.case_id) else {
            continue;
        };
        if case.resolution_status != CaseResolutionStatus::Open {
            continue;
        }
        let Some(problem) = ctx
            .db
            .local_problem_authority()
            .id()
            .find(&validated.manifest.problem_id)
        else {
            continue;
        };
        let parties = ctx
            .db
            .npc_adventuring_party_authority()
            .settlement_id()
            .filter(&authority.settlement_id)
            .map(|party| NpcPartySnapshot {
                party_id: party.id,
                name: party.name,
                settlement_id: party.settlement_id,
                capability: party.capability,
                available_at: party.available_at,
            })
            .collect::<Vec<_>>();
        let snapshot = NpcCaseSnapshot {
            case_id: case.id.clone(),
            problem_id: problem.id.clone(),
            settlement_id: authority.settlement_id.clone(),
            opened_at: problem.starts_at,
            incident_count: problem.incident_count,
            mitigation_bps: problem.mitigation_bps,
            open: true,
            player_activity_at: None,
        };
        let retry_at = ctx
            .db
            .npc_case_intervention()
            .case_id()
            .filter(&case.id)
            .max_by_key(|row| row.attempt)
            .map_or(0, |row| row.next_retry_at);
        let mut earliest_intervention_minute =
            adventuresim_core::npc_adventurer::eligible_at(&snapshot).max(retry_at);
        let Some(party) = select_party(&snapshot, u64::MAX, &parties) else {
            continue;
        };
        earliest_intervention_minute = earliest_intervention_minute.max(party.available_at);
        rows.push(BackendNpcInterventionCandidate {
            case_id: case.id,
            public_case_id: authority.public_case_id,
            settlement_id: authority.settlement_id,
            problem_summary: validated.manifest.consequence.public_summary,
            incident_count: snapshot.incident_count,
            earliest_intervention_minute,
            party_id: party.party_id.clone(),
            party_name: party.name.clone(),
            party_capability: party.capability,
            strategy_already_selected: ctx
                .db
                .npc_intervention_strategy_override()
                .case_id()
                .find(&snapshot.case_id)
                .is_some(),
            legal_strategies_json: legal_strategies_json.clone(),
        });
    }
    rows.sort_by(|left, right| left.public_case_id.cmp(&right.public_case_id));
    rows
}

/// Gateway-only observation surface for the live NPC evaluator. The Markdown
/// is generated and persisted by the authoritative intervention transaction,
/// so a simulator never reconstructs a parallel result from private truth.
#[view(accessor = backend_npc_case_interventions, public)]
pub fn backend_npc_case_interventions(ctx: &ViewContext) -> Vec<BackendNpcCaseIntervention> {
    if !view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = ctx
        .db
        .npc_case_intervention()
        .started_at()
        .filter(0u64..)
        .filter_map(|row| {
            let party = ctx
                .db
                .npc_adventuring_party_authority()
                .id()
                .find(&row.party_id)?;
            Some(BackendNpcCaseIntervention {
                intervention_id: row.id,
                public_case_id: row.public_case_id,
                party_name: party.name,
                attempt: row.attempt,
                started_at: row.started_at,
                completed_at: row.completed_at,
                strategy: row.strategy,
                route: row.route,
                lead_summary: row.lead_summary,
                preparation_summary: row.preparation_summary,
                outcome: row.outcome,
                safe_summary: row.safe_summary,
                public_story_markdown: row.public_story_markdown,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| (row.started_at, row.intervention_id.clone()));
    rows
}

/// Optional LLM policies run outside SpacetimeDB and may select only one of
/// the bounded strategic approaches. The server still owns the outcome roll
/// and all mutations. This reducer exists only in a capability-owned disposable
/// simulation database.
#[reducer]
pub fn set_simulation_npc_intervention_strategy(
    ctx: &ReducerContext,
    run_nonce: String,
    case_id: String,
    strategy: String,
) -> Result<(), String> {
    crate::simulation::owned_run(ctx, &run_nonce)?;
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id)
        .ok_or("NPC evaluation case does not exist")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("NPC evaluation case is no longer open".into());
    }
    parse_strategy(&strategy)?;
    match ctx
        .db
        .npc_intervention_strategy_override()
        .case_id()
        .find(&case_id)
    {
        Some(existing) if existing.strategy == strategy => Ok(()),
        Some(_) => Err("NPC evaluation strategy was already selected differently".into()),
        None => {
            ctx.db
                .npc_intervention_strategy_override()
                .insert(NpcInterventionStrategyOverride { case_id, strategy });
            Ok(())
        }
    }
}

pub(crate) fn ensure_npc_case_interventions(
    ctx: &ReducerContext,
    settlement_id: &str,
    now: u64,
) -> Result<(), String> {
    ensure_npc_adventuring_party(ctx, settlement_id)?;
    let mut parties = ctx
        .db
        .npc_adventuring_party_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .map(|party| NpcPartySnapshot {
            party_id: party.id,
            name: party.name,
            settlement_id: party.settlement_id,
            capability: party.capability,
            available_at: party.available_at,
        })
        .collect::<Vec<_>>();
    let mut authorities = ctx
        .db
        .quest_generation_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .collect::<Vec<_>>();
    authorities.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    for authority in authorities {
        let validated = validate_quest_generation_authority(&authority)?;
        if validated.context.settlement_id != settlement_id {
            continue;
        }
        let Some(case) = ctx.db.case_authority().id().find(&authority.case_id) else {
            continue;
        };
        let Some(problem) = ctx
            .db
            .local_problem_authority()
            .id()
            .find(&validated.manifest.problem_id)
        else {
            continue;
        };
        let last_attempt = ctx
            .db
            .npc_case_intervention()
            .case_id()
            .filter(&case.id)
            .max_by_key(|row| row.attempt);
        if last_attempt
            .as_ref()
            .is_some_and(|attempt| now < attempt.next_retry_at)
        {
            continue;
        }
        let snapshot = NpcCaseSnapshot {
            case_id: case.id.clone(),
            problem_id: problem.id.clone(),
            settlement_id: settlement_id.into(),
            opened_at: problem.starts_at,
            incident_count: problem.incident_count,
            mitigation_bps: problem.mitigation_bps,
            open: case.resolution_status == CaseResolutionStatus::Open,
            player_activity_at: latest_player_activity(ctx, &case.id, now),
        };
        if !case_is_eligible(&snapshot, now) {
            continue;
        }
        let Some(party) = select_party(&snapshot, now, &parties).cloned() else {
            continue;
        };
        let attempt = last_attempt
            .as_ref()
            .map_or(1, |row| row.attempt.saturating_add(1));
        let strategy = ctx
            .db
            .npc_intervention_strategy_override()
            .case_id()
            .find(&case.id)
            .map(|row| parse_strategy(&row.strategy))
            .transpose()?
            .unwrap_or_else(|| scripted_strategy(&snapshot, &party));
        let approaches = supported_investigation_approaches(&validated.manifest);
        let previous_route = last_attempt.as_ref().and_then(|row| {
            approaches
                .iter()
                .find(|approach| approach.route_label == row.route)
                .map(|approach| approach.route)
        });
        let approach =
            select_investigation_approach_after(&approaches, strategy, attempt, previous_route);
        let approach_resolution = approach
            .map(|plan| resolve_investigation_approach(&snapshot, &party, plan, attempt, now));
        let next_approach = select_investigation_approach_after(
            &approaches,
            strategy,
            attempt.saturating_add(1),
            approach.map(|current| current.route),
        );
        let decision = decide_after_supported_approach(
            &snapshot,
            &party,
            strategy,
            attempt,
            now,
            approach
                .zip(approach_resolution.as_ref())
                .map(|(plan, result)| {
                    (
                        plan,
                        result,
                        next_approach.map(|next| next.route_label.as_str()),
                    )
                }),
        );
        let story_started_at = public_story_started_at(
            &validated.manifest,
            approach.map_or(0, |plan| plan.step_summaries.len()),
            now,
        );
        let intervention_id = format!("npc-intervention:{}:{attempt}", case.id);
        if ctx
            .db
            .npc_case_intervention()
            .id()
            .find(&intervention_id)
            .is_some()
        {
            continue;
        }
        ctx.db.npc_case_intervention().insert(NpcCaseIntervention {
            id: intervention_id.clone(),
            case_id: case.id.clone(),
            public_case_id: authority.public_case_id.clone(),
            problem_id: problem.id.clone(),
            party_id: party.party_id.clone(),
            attempt,
            started_at: story_started_at,
            completed_at: now,
            strategy: format!("{strategy:?}"),
            route: approach.map_or_else(|| "deferred".into(), |plan| plan.route_label.clone()),
            lead_summary: approach.map_or_else(
                || "No lead was pursued.".into(),
                |plan| format!("{}: {}", plan.lead_source, plan.lead_quote),
            ),
            preparation_summary: approach.map_or_else(
                || "No preparation was undertaken.".into(),
                |plan| plan.preparation_summary.clone(),
            ),
            action_plan_json: serde_json::to_string(
                &approach.map_or_else(Vec::new, |plan| plan.step_summaries.clone()),
            )
            .map_err(|_| "Could not encode NPC investigation action plan")?,
            outcome: format!("{:?}", decision.outcome),
            mitigation_bps: decision.mitigation_bps,
            next_retry_at: decision.next_available_at,
            safe_summary: decision.safe_summary.clone(),
            public_story_markdown: render_public_story(
                &validated.manifest,
                &party.name,
                attempt,
                strategy,
                &decision,
                approach,
                approach_resolution.as_ref(),
                next_approach,
                story_started_at,
                now,
            ),
        });
        if ctx
            .db
            .npc_intervention_strategy_override()
            .case_id()
            .find(&case.id)
            .is_some()
        {
            ctx.db
                .npc_intervention_strategy_override()
                .case_id()
                .delete(&case.id);
        }

        if let Some(mut row) = ctx
            .db
            .npc_adventuring_party_authority()
            .id()
            .find(&party.party_id)
        {
            row.available_at = decision.next_available_at;
            ctx.db.npc_adventuring_party_authority().id().update(row);
        }
        update_working_party_availability(&mut parties, &party, decision.next_available_at)?;

        match decision.outcome {
            NpcInterventionOutcome::Resolved => {
                resolve_generated_case(ctx, &validated.manifest, &party.party_id, &intervention_id)?
            }
            NpcInterventionOutcome::Mitigated => apply_outcome(
                ctx,
                &problem.id,
                &LocalProblemOutcomeInput {
                    source_outcome_id: intervention_id.clone(),
                    at_minute: now,
                    mitigation_bps: decision.mitigation_bps,
                    resolve: false,
                },
            )?,
            NpcInterventionOutcome::Failed | NpcInterventionOutcome::Delayed => {}
        }
        record_news_for_informed_characters(
            ctx,
            &problem.id,
            &authority.public_case_id,
            &intervention_id,
            &decision.safe_summary,
            now,
        )?;
    }
    Ok(())
}

fn update_working_party_availability(
    parties: &mut [NpcPartySnapshot],
    selected_party: &NpcPartySnapshot,
    available_at: u64,
) -> Result<(), String> {
    if update_party_availability(parties, &selected_party.party_id, available_at) {
        Ok(())
    } else {
        Err(format!(
            "Selected NPC adventuring party `{}` was absent from the working roster",
            selected_party.party_id
        ))
    }
}

fn parse_strategy(value: &str) -> Result<NpcInterventionStrategy, String> {
    match value {
        "investigate_carefully" => Ok(NpcInterventionStrategy::InvestigateCarefully),
        "protect_locals" => Ok(NpcInterventionStrategy::ProtectLocals),
        "confront_directly" => Ok(NpcInterventionStrategy::ConfrontDirectly),
        "defer" => Ok(NpcInterventionStrategy::Defer),
        _ => Err("NPC intervention strategy is not one of the advertised choices".into()),
    }
}

fn render_public_story(
    generated: &GeneratedCase,
    party_name: &str,
    attempt: u16,
    strategy: NpcInterventionStrategy,
    decision: &NpcInterventionDecision,
    approach: Option<&NpcInvestigationApproach>,
    approach_resolution: Option<&NpcApproachResolution>,
    next_approach: Option<&NpcInvestigationApproach>,
    started_at: u64,
    completed_at: u64,
) -> String {
    let mut event_at = started_at;
    let mut story = String::new();
    writeln!(
        story,
        "## {party_name}: {}",
        generated.consequence.public_summary
    )
    .unwrap();
    writeln!(story).unwrap();
    writeln!(story, "- Case: `{}`", generated.public_case_id).unwrap();
    writeln!(story, "- Attempt: {attempt}").unwrap();
    writeln!(story, "- Began at world minute: {started_at}").unwrap();
    writeln!(story, "- Strategy: {strategy:?}").unwrap();
    writeln!(story).unwrap();
    writeln!(story, "### World minute {event_at}: tavern discovery").unwrap();
    writeln!(story).unwrap();
    writeln!(
        story,
        "{} learned: {}",
        party_name, generated.consequence.public_summary
    )
    .unwrap();
    writeln!(story).unwrap();
    writeln!(
        story,
        "> **Tavern keeper:** Locals have been saying: {}",
        generated.consequence.public_summary
    )
    .unwrap();
    event_at = event_at.saturating_add(15);
    let visible_testimony =
        adventuresim_core::quest_generation::player_visible_testimony_sequence(generated);
    let mut visible_witnesses = Vec::new();
    for (witness, _) in &visible_testimony {
        if !visible_witnesses.iter().any(
            |candidate: &&adventuresim_core::quest_generation::WitnessBinding| {
                candidate.id == witness.id
            },
        ) {
            visible_witnesses.push(*witness);
        }
    }
    for witness in visible_witnesses {
        writeln!(story).unwrap();
        writeln!(
            story,
            "### World minute {event_at}: interview with {}",
            witness.display_name
        )
        .unwrap();
        for (_, statement) in visible_testimony
            .iter()
            .filter(|(candidate, _)| candidate.id == witness.id)
        {
            writeln!(story).unwrap();
            writeln!(
                story,
                "> **{}:** {}",
                witness.display_name, statement.spoken_text
            )
            .unwrap();
        }
        event_at = event_at.saturating_add(20);
    }
    if let Some(approach) = approach {
        writeln!(story).unwrap();
        writeln!(story, "### World minute {event_at}: chosen lead").unwrap();
        writeln!(story).unwrap();
        writeln!(
            story,
            "{party_name} chose to test {} through {}.",
            approach.lead_source, approach.route_label
        )
        .unwrap();
        writeln!(story).unwrap();
        writeln!(
            story,
            "> **{}:** {}",
            approach.lead_source, approach.lead_quote
        )
        .unwrap();
        event_at = event_at.saturating_add(15);
        writeln!(story).unwrap();
        writeln!(story, "### World minute {event_at}: preparation").unwrap();
        writeln!(story).unwrap();
        writeln!(story, "{}", approach.preparation_summary).unwrap();
        for step in &approach.step_summaries {
            event_at = event_at.saturating_add(15);
            writeln!(story).unwrap();
            writeln!(story, "- World minute {event_at}: {step}").unwrap();
        }
        writeln!(story).unwrap();
        writeln!(story, "### World minute {event_at}: route result").unwrap();
        writeln!(story).unwrap();
        if approach_resolution.is_some_and(|result| result.succeeded) {
            writeln!(
                story,
                "The company completed the supported route and reached {}.",
                approach.destination_label
            )
            .unwrap();
        } else if let Some(result) = approach_resolution {
            writeln!(
                story,
                "{}",
                result
                    .failure_summary
                    .as_deref()
                    .unwrap_or("The route produced no conclusive result.")
            )
            .unwrap();
            if let Some(next) = next_approach {
                writeln!(story).unwrap();
                writeln!(
                    story,
                    "After regrouping, the company intends to try {}.",
                    next.route_label
                )
                .unwrap();
            }
        }
    } else {
        writeln!(story).unwrap();
        writeln!(story, "### World minute {event_at}: approach").unwrap();
        writeln!(story).unwrap();
        writeln!(story, "{party_name} chose {strategy:?}.").unwrap();
    }
    writeln!(story).unwrap();
    writeln!(story, "### World minute {completed_at}: result").unwrap();
    writeln!(story).unwrap();
    writeln!(story, "{}", decision.safe_summary).unwrap();
    story
}

fn public_story_started_at(
    generated: &GeneratedCase,
    planned_steps: usize,
    completed_at: u64,
) -> u64 {
    let duration = 15u64
        .saturating_add((generated.witnesses.len() as u64).saturating_mul(20))
        .saturating_add((planned_steps as u64).saturating_mul(15))
        .saturating_add(15)
        .saturating_add(30);
    completed_at.saturating_sub(duration)
}

fn ensure_npc_adventuring_party(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    if ctx
        .db
        .npc_adventuring_party_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .next()
        .is_some()
    {
        return Ok(());
    }
    let mut members = ctx
        .db
        .settlement_npc()
        .home_settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|npc| npc.service_id.is_empty())
        .collect::<Vec<_>>();
    members.sort_by_key(|npc| {
        (
            stable_hash(&format!("{settlement_id}:{}", npc.id)),
            npc.id.clone(),
        )
    });
    members.truncate(3);
    if members.len() < 2 {
        return Ok(());
    }
    let leader = &members[0];
    ctx.db
        .npc_adventuring_party_authority()
        .insert(NpcAdventuringPartyAuthority {
            id: format!("npc-party:{settlement_id}:resident-company"),
            settlement_id: settlement_id.into(),
            name: format!("{}'s Company", leader.name),
            member_npc_ids_json: serde_json::to_string(
                &members.iter().map(|npc| &npc.id).collect::<Vec<_>>(),
            )
            .map_err(|_| "Could not encode NPC adventuring-party membership")?,
            capability: 45 + (stable_hash(&format!("{settlement_id}:capability")) % 41) as u16,
            available_at: 0,
        });
    Ok(())
}

fn latest_player_activity(ctx: &ReducerContext, case_id: &str, now: u64) -> Option<u64> {
    let recent_action = ctx
        .db
        .investigation_action_outcome()
        .iter()
        .filter(|outcome| outcome.case_id == case_id)
        .map(|outcome| outcome.recorded_at)
        .max();
    let occupying = ctx
        .db
        .case_site_authority()
        .case_id()
        .filter(case_id)
        .any(|site| {
            ctx.db
                .party_authority()
                .iter()
                .any(|party| party.current_case_site_id.as_ref() == Some(&site.id))
        });
    if occupying { Some(now) } else { recent_action }
}

fn resolve_generated_case(
    ctx: &ReducerContext,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    party_id: &str,
    intervention_id: &str,
) -> Result<(), String> {
    let path = generated
        .objectives
        .alternatives
        .first()
        .ok_or("Generated case has no objective path")?;
    for (index, objective) in path.objectives.iter().enumerate() {
        apply_objective(
            ctx,
            &generated.canonical_case_id,
            party_id,
            &format!("{intervention_id}:objective:{index}"),
            &objective.requirement,
        )?;
    }
    Ok(())
}

fn apply_objective(
    ctx: &ReducerContext,
    case_id: &str,
    party_id: &str,
    source_id: &str,
    requirement: &ObjectiveRequirement,
) -> Result<(), String> {
    match requirement {
        ObjectiveRequirement::Defeat {
            hostile_group_id,
            count,
        } => {
            update_hostile_disposition(ctx, hostile_group_id, HostileGroupDisposition::Defeated)?;
            ingest_case_outcome_fact(
                ctx,
                source_id,
                case_id,
                party_id,
                OutcomeFactKind::HostilesDefeated {
                    hostile_group_id: hostile_group_id.clone(),
                    count: *count,
                },
            )
        }
        ObjectiveRequirement::DriveOff { hostile_group_id } => {
            update_hostile_disposition(ctx, hostile_group_id, HostileGroupDisposition::DrivenOff)?;
            ingest_case_outcome_fact(
                ctx,
                source_id,
                case_id,
                party_id,
                OutcomeFactKind::HostilesDrivenOff {
                    hostile_group_id: hostile_group_id.clone(),
                },
            )
        }
        ObjectiveRequirement::Rescue { subject_id } => record_subject_rescued_or_released(
            ctx,
            source_id,
            case_id,
            party_id,
            subject_id.as_str(),
            next_custody_version(ctx, subject_id.as_str()),
            false,
        )
        .map(|_| ()),
        ObjectiveRequirement::Retrieve { asset_id } => record_asset_retrieved(
            ctx,
            source_id,
            case_id,
            party_id,
            asset_id.as_str(),
            next_custody_version(ctx, asset_id.as_str()),
        )
        .map(|_| ()),
        ObjectiveRequirement::Return {
            asset_id,
            custodian_id,
        } => record_asset_returned_or_exchanged(
            ctx,
            source_id,
            case_id,
            party_id,
            asset_id.as_str(),
            custodian_id,
            next_custody_version(ctx, asset_id.as_str()),
            false,
        )
        .map(|_| ()),
        ObjectiveRequirement::Expose { subject_ref } => ingest_case_outcome_fact(
            ctx,
            source_id,
            case_id,
            party_id,
            OutcomeFactKind::Exposed {
                subject_ref: subject_ref.clone(),
            },
        ),
        _ => Err("NPC adventurers cannot yet resolve this objective type safely".into()),
    }
}

fn update_hostile_disposition(
    ctx: &ReducerContext,
    hostile_group_id: &str,
    disposition: HostileGroupDisposition,
) -> Result<(), String> {
    let mut group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&hostile_group_id.to_string())
        .ok_or("NPC intervention hostile group is missing")?;
    if group.disposition == disposition {
        return Ok(());
    }
    if group.disposition != HostileGroupDisposition::Active {
        return Err("NPC intervention hostile group is already resolved differently".into());
    }
    group.disposition = disposition;
    ctx.db.hostile_group_authority().id().update(group);
    Ok(())
}

fn next_custody_version(ctx: &ReducerContext, object_id: &str) -> u32 {
    ctx.db
        .case_custody()
        .object_id()
        .find(&object_id.to_string())
        .map_or(0, |custody| custody.version.saturating_add(1))
}

fn record_news_for_informed_characters(
    ctx: &ReducerContext,
    problem_id: &str,
    public_case_id: &str,
    source_id: &str,
    summary: &str,
    recorded_at: u64,
) -> Result<(), String> {
    let mut informed = ctx
        .db
        .local_problem_receipt()
        .iter()
        .filter(|receipt| receipt.problem_id == problem_id)
        .map(|receipt| receipt.character_id)
        .collect::<Vec<_>>();
    informed.sort_unstable();
    informed.dedup();
    for character_id in informed {
        record_journal_notice(
            ctx,
            character_id,
            public_case_id,
            source_id,
            summary,
            "local news",
            recorded_at,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_policy_is_limited_to_bounded_strategies() {
        assert_eq!(
            parse_strategy("investigate_carefully").unwrap(),
            NpcInterventionStrategy::InvestigateCarefully
        );
        assert_eq!(
            parse_strategy("defer").unwrap(),
            NpcInterventionStrategy::Defer
        );
        assert!(parse_strategy("resolve_case").is_err());
        assert!(parse_strategy("reveal_true_site").is_err());
    }

    #[test]
    fn strategy_override_is_simulation_owned_and_server_applied() {
        let source = include_str!("npc_adventurer.rs");
        let reducer = source
            .split("pub fn set_simulation_npc_intervention_strategy")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn ensure_npc_case_interventions")
            .next()
            .unwrap();
        assert!(reducer.contains("crate::simulation::owned_run(ctx, &run_nonce)?"));
        assert!(!reducer.contains("apply_outcome("));
        assert!(!reducer.contains("resolve_generated_case("));
        assert!(source.contains("let decision = decide_after_supported_approach("));
    }

    #[test]
    fn npc_interventions_use_the_settlement_authority_index() {
        let source = include_str!("npc_adventurer.rs").replace('\r', "");
        let activity = source
            .split("pub(crate) fn ensure_npc_case_interventions")
            .nth(1)
            .and_then(|tail| tail.split("fn ensure_npc_adventuring_party").next())
            .expect("NPC intervention activity");
        assert!(activity.contains("quest_generation_authority()"));
        assert!(activity.contains(".settlement_id()"));
        assert!(activity.contains(".filter(&settlement_id.to_string())"));
        assert!(!activity.contains("quest_generation_authority()\n        .iter()"));
        assert!(activity.contains("validate_quest_generation_authority"));
        assert!(activity.contains("validated.context.settlement_id != settlement_id"));
    }

    #[test]
    fn authority_loop_reserves_selected_party_until_decision_availability() {
        let selected = NpcPartySnapshot {
            party_id: "company-a".into(),
            name: "Company A".into(),
            settlement_id: "town".into(),
            capability: 80,
            available_at: 0,
        };
        let mut parties = vec![selected.clone()];
        update_working_party_availability(&mut parties, &selected, 12_345).unwrap();
        assert_eq!(parties[0].available_at, 12_345);
        assert!(update_working_party_availability(&mut [], &selected, 12_345).is_err());

        let source = include_str!("npc_adventurer.rs").replace('\r', "");
        let activity = source
            .split("pub(crate) fn ensure_npc_case_interventions")
            .nth(1)
            .and_then(|tail| tail.split("fn update_working_party_availability").next())
            .expect("NPC intervention authority loop");
        let persisted = activity
            .find("row.available_at = decision.next_available_at;")
            .expect("persisted party availability");
        let working = activity
            .find("update_working_party_availability(")
            .expect("working party availability");
        let outcome_match = activity
            .find("match decision.outcome")
            .expect("outcome application");
        let resolution = activity
            .find("resolve_generated_case(")
            .expect("resolved-case application");
        let working_call = &activity[working..outcome_match];

        assert!(
            activity.contains("let Some(party) = select_party(&snapshot, now, &parties).cloned()")
        );
        assert_eq!(
            activity
                .matches("update_working_party_availability(")
                .count(),
            1
        );
        assert!(working_call.contains("&mut parties"));
        assert!(working_call.contains("&party"));
        assert_eq!(
            working_call.matches("decision.next_available_at").count(),
            1
        );
        assert!(working_call.contains(")?;"));
        assert!(persisted < working);
        assert!(persisted < outcome_match);
        assert!(working < outcome_match);
        assert!(persisted < resolution);
        assert!(working < resolution);
    }

    #[test]
    fn evaluator_views_fail_closed_to_non_gateway_callers() {
        let source = include_str!("npc_adventurer.rs");
        assert_eq!(source.matches("if !view_is_gateway(ctx)").count(), 2);
        assert!(source.contains("legal_strategies_json"));
        assert!(!source.contains("canonical_case_id:"));
    }

    #[test]
    fn story_quotes_only_inside_explicit_chronological_interviews() {
        let source = include_str!("npc_adventurer.rs");
        let renderer = source
            .split("fn render_public_story")
            .nth(1)
            .unwrap()
            .split("fn public_story_started_at")
            .next()
            .unwrap();
        let interview = renderer.find("interview with {}").unwrap();
        let quote = renderer.find("> **{}:** {}").unwrap();
        assert!(interview < quote);
        assert!(renderer.contains("event_at = event_at.saturating_add(20)"));
        assert!(renderer.contains("chosen lead"));
        assert!(renderer.contains("preparation"));
        assert!(renderer.contains("route result"));
        assert!(renderer.contains("intends to try"));
        assert!(renderer.contains("World minute {completed_at}: result"));
        assert!(renderer.contains("player_visible_testimony_sequence(generated)"));
        assert!(!renderer.contains("for witness in &generated.witnesses"));
        for private_term in [
            "canonical_case_id",
            "canonical cause",
            "true_site",
            "sincerity",
            "factor_trace",
            "likelihood",
        ] {
            assert!(!renderer.contains(private_term));
        }
    }
}
