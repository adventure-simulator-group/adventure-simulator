//! Private local-problem authority and safe discovery/consequence projections.
use crate::{
    character::{character, character__view},
    investigation::{
        EvidencePresentationKind, InvestigationEventAuthority, InvestigationEvidenceAuthority,
        InvestigationLead, case_site_authority, investigation_event_authority,
        investigation_evidence_authority, investigation_lead,
    },
    settlement_population::{settlement_resident_presence, settlement_resident_profile},
    strategic::{
        ValidatedQuestGenerationAuthority, hostile_group_authority, quest_generation_authority,
        settlement, strategic_gateway_authority__view, travel_edge,
        validate_quest_generation_authority,
    },
    time::{character_time, character_time__view, world_clock},
};
use adventuresim_core::strategic_place::CaseSiteId;
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use adventuresim_core::threat_escalation::bounded_public_threat_candidates as bounded_public_candidates;
use adventuresim_core::{encounter::EncounterArchetype, local_problem as lp};
use serde::{Deserialize, Serialize};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};

#[derive(Clone, Debug)]
#[table(accessor = local_problem_authority)]
pub struct LocalProblemAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub scope_key: String,
    pub scope_json: String,
    /// Symptom-to-effect mechanism only. Canonical cause belongs exclusively
    /// to the linked generated case manifest.
    pub consequence_mechanism: String,
    pub symptom: String,
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
    pub encounter_frequency_bps: u16,
    pub encounter_archetype: Option<EncounterArchetype>,
    pub disease_intensity: u16,
    pub disease_id: String,
    pub starts_at: u64,
    pub ends_at: u64,
    pub mitigation_bps: u16,
    /// Includes the original offence represented by the generated case.
    pub incident_count: u16,
    pub recurring_hostile: bool,
    pub public_awareness_bps: u16,
    pub public_since_minute: Option<u64>,
    pub resolved_at: Option<u64>,
    pub opaque_case_ref: String,
}

/// An immutable follow-up offence appended to a generated case after creation.
/// This remains private authority until an observer discovers its account or
/// physical evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[table(accessor = generated_problem_incident)]
pub struct GeneratedProblemIncident {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub problem_id: String,
    pub ordinal: u16,
    pub occurred_at: u64,
    pub event_id: String,
    pub proposition_id: String,
    pub witness_resident_character_id: u64,
    pub victim_resident_character_id: u64,
    pub circumstance: String,
    pub site_id: String,
    pub evidence_id: String,
    pub evidence_kind: String,
    pub public_summary: String,
    pub witness_account: String,
}

#[derive(Clone, Debug)]
#[table(accessor = local_problem_generation_explanation)]
pub struct LocalProblemGenerationExplanation {
    #[primary_key]
    pub problem_id: String,
    pub explanation_json: String,
}

/// Safe world projection: a symptom and bounded current effects, never its cause.
#[derive(Clone, Debug)]
#[table(accessor = local_problem_symptom, public)]
pub struct LocalProblemSymptom {
    #[primary_key]
    pub problem_id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub symptom: String,
    pub public_summary: String,
    pub active_from: u64,
    pub active_until: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendLocalProblemTradeEffect {
    pub character_id: u64,
    pub settlement_id: String,
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
}

/// Private, source-attributed knowledge receipt and the narrow #183 seam.
#[derive(Clone, Debug)]
#[table(accessor = local_problem_receipt)]
pub struct LocalProblemReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    pub problem_id: String,
    pub opaque_case_ref: String,
    pub source_resident_character_id: u64,
    pub discovery_session_id: String,
    pub contact_resident_character_id: u64,
    pub expected_location_id: String,
    pub safe_summary: String,
    /// Observer chronology used by owner-facing journal projections.
    pub learned_at: u64,
    /// Authoritative world chronology used only by server-side fairness rules.
    pub official_learned_at: u64,
}

/// Private, one-shot discovery ordering used by explicit development demos.
///
/// The preference does not disclose the problem or create observer knowledge.
/// It is consumed only when ordinary rumor dialogue successfully creates the
/// corresponding receipt.
#[derive(Clone, Debug)]
#[table(accessor = local_problem_rumor_preference)]
pub struct LocalProblemRumorPreference {
    #[primary_key]
    pub character_id: u64,
    pub settlement_id: String,
    pub problem_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor=local_problem_rumor_delivery)]
pub struct LocalProblemRumorDelivery {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    #[index(btree)]
    pub session_id: String,
    /// The private observer receipt consumed by investigation authority.
    /// This is intentionally not derived from the delivery row ID.
    pub receipt_id: String,
    pub fragments_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = local_problem_incident_receipt)]
pub struct LocalProblemIncidentReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub problem_id: String,
    pub incident_id: String,
    pub learned_at: u64,
}

/// Observer-scoped canonical disclosure for a publicly notorious hostile case.
/// It deliberately contains no evidence, testimony, trace, or preparation data.
#[derive(Clone, Debug, PartialEq, Eq)]
#[table(accessor = public_threat_disclosure)]
pub struct PublicThreatDisclosure {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub public_case_id: String,
    pub threat_type: String,
    pub exact_site_id: CaseSiteId,
    pub approximate_count: String,
    pub source_kind: String,
    pub source_resident_character_id: u64,
    pub learned_at: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendLocalProblemRumor {
    pub receipt_id: String,
    pub character_id: u64,
    pub settlement_id: String,
    pub session_id: String,
    pub fragments_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = local_problem_outcome_receipt)]
pub struct LocalProblemOutcomeReceipt {
    #[primary_key]
    pub id: String,
    pub problem_id: String,
    pub source_outcome_id: String,
    pub applied_at: u64,
    pub mitigation_bps: u16,
    pub resolved: bool,
    pub payload_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PrivateExplanation {
    context: lp::GenerationContext,
    explanation: lp::GenerationExplanation,
}

fn scope_key(scope: &lp::Scope) -> String {
    match scope {
        lp::Scope::Settlement { settlement_id } => format!("settlement:{settlement_id}"),
        lp::Scope::Route {
            endpoint_a,
            endpoint_b,
        } => format!("route:{endpoint_a}:{endpoint_b}"),
    }
}
fn symptom_name(value: lp::Symptom) -> &'static str {
    match value {
        lp::Symptom::MissingCaravans => "missing_caravans",
        lp::Symptom::NightScreams => "night_screams",
        lp::Symptom::SickLocals => "sick_locals",
        lp::Symptom::EmptyStalls => "empty_stalls",
        lp::Symptom::VanishedLivestock => "vanished_livestock",
    }
}
fn safe_summary(value: lp::Symptom) -> &'static str {
    match value {
        lp::Symptom::MissingCaravans => "Several expected caravans have not arrived.",
        lp::Symptom::NightScreams => "Locals report troubling sounds after dark.",
        lp::Symptom::SickLocals => "An unusual number of locals have fallen ill.",
        lp::Symptom::EmptyStalls => "Everyday goods have become harder to obtain.",
        lp::Symptom::VanishedLivestock => "Livestock have been disappearing from nearby holdings.",
    }
}
fn route_mechanism(value: lp::Symptom) -> &'static str {
    match value {
        lp::Symptom::MissingCaravans | lp::Symptom::EmptyStalls => "route_supply_disruption",
        lp::Symptom::NightScreams => "route_nighttime_insecurity",
        lp::Symptom::SickLocals => "route_illness_pressure",
        lp::Symptom::VanishedLivestock => "route_livestock_losses",
    }
}
fn is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

#[view(accessor = backend_local_problem_trade_effects, public)]
pub fn backend_local_problem_trade_effects(
    ctx: &ViewContext,
) -> Vec<BackendLocalProblemTradeEffect> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let mut characters: Vec<_> = ctx
        .db
        .character_time()
        .minutes()
        .filter(0u64..)
        .filter_map(|time| {
            ctx.db
                .character()
                .id()
                .find(time.character_id)
                .and_then(|c| c.current_settlement_id.map(|s| (c.id, s, time.minutes)))
        })
        .collect();
    characters.sort();
    characters
        .into_iter()
        .map(|(character_id, settlement_id, minute)| {
            let key = format!("settlement:{settlement_id}");
            let rows: Vec<_> = ctx
                .db
                .local_problem_authority()
                .scope_key()
                .filter(&key)
                .map(|r| lp::ConsequenceInput {
                    id: r.id,
                    buy_bps: r.buy_bps,
                    sell_penalty_bps: r.sell_penalty_bps,
                    encounter_frequency_bps: 0,
                    disease_intensity: 0,
                    starts_at: r.starts_at,
                    ends_at: r.ends_at,
                    mitigation_bps: r.mitigation_bps,
                    incident_count: r.incident_count,
                    resolved_at: r.resolved_at,
                })
                .collect();
            let effects = lp::aggregate_consequences(&rows, minute);
            BackendLocalProblemTradeEffect {
                character_id,
                settlement_id,
                buy_bps: effects.buy_bps,
                sell_penalty_bps: effects.sell_penalty_bps,
            }
        })
        .collect()
}

#[view(accessor = backend_local_problem_rumors, public)]
pub fn backend_local_problem_rumors(ctx: &ViewContext) -> Vec<BackendLocalProblemRumor> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .local_problem_rumor_delivery()
        .character_id()
        .filter(0u64..)
        .map(|r| BackendLocalProblemRumor {
            receipt_id: r.receipt_id,
            character_id: r.character_id,
            settlement_id: r.settlement_id,
            session_id: r.session_id,
            fragments_json: r.fragments_json,
        })
        .collect()
}

pub(crate) fn official_minute(ctx: &ReducerContext) -> u64 {
    ctx.db
        .world_clock()
        .id()
        .find(0)
        .map_or(0, |r| r.official_minutes)
}

pub(crate) fn prefer_next_rumor(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    problem_id: &str,
) {
    let preference = LocalProblemRumorPreference {
        character_id,
        settlement_id: settlement_id.into(),
        problem_id: problem_id.into(),
    };
    if ctx
        .db
        .local_problem_rumor_preference()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .local_problem_rumor_preference()
            .character_id()
            .update(preference);
    } else {
        ctx.db.local_problem_rumor_preference().insert(preference);
    }
}

/// Materialize only symptom and consequence authority from a fully validated
/// canonical generated case. The caller owns the surrounding transaction.
pub(crate) fn materialize_generated_problem(
    ctx: &ReducerContext,
    case: &adventuresim_core::quest_generation::GeneratedCase,
    settlement_id: &str,
) -> Result<(), String> {
    let consequence = &case.consequence;
    let scope = lp::Scope::Settlement {
        settlement_id: settlement_id.into(),
    };
    let scope_key = scope_key(&scope);
    let starts_at = official_minute(ctx);
    let recurring_hostile = case.family
        == adventuresim_core::quest_generation::TemplateFamily::RecurringDepredation
        && matches!(
            case.cause,
            adventuresim_core::quest_generation::CanonicalCause::Hostile(_)
        );
    let ends_at = if recurring_hostile {
        u64::MAX
    } else {
        starts_at.saturating_add(30 * MINUTES_PER_DAY)
    };
    let mechanism = match consequence.symptom {
        lp::Symptom::MissingCaravans => "supply_disruption",
        lp::Symptom::NightScreams => "nighttime_insecurity",
        lp::Symptom::SickLocals => "community_illness",
        lp::Symptom::EmptyStalls => "trade_disruption",
        lp::Symptom::VanishedLivestock => "livestock_losses",
    };
    ctx.db
        .local_problem_authority()
        .insert(LocalProblemAuthority {
            id: case.problem_id.clone(),
            gateway_bucket: 0,
            scope_key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode generated problem scope")?,
            consequence_mechanism: mechanism.into(),
            symptom: symptom_name(consequence.symptom).into(),
            buy_bps: consequence.effects.buy_bps,
            sell_penalty_bps: consequence.effects.sell_penalty_bps,
            encounter_frequency_bps: consequence.effects.encounter_frequency_bps,
            encounter_archetype: consequence.effects.encounter_archetype,
            disease_intensity: consequence.effects.disease_intensity,
            disease_id: case.outbreak.as_ref().map_or_else(
                || {
                    if consequence.effects.disease_intensity > 0 {
                        "influenza".into()
                    } else {
                        String::new()
                    }
                },
                |outbreak| crate::disease::disease_key(outbreak.disease).into(),
            ),
            starts_at,
            ends_at,
            mitigation_bps: 0,
            incident_count: 1,
            recurring_hostile,
            public_awareness_bps: 0,
            public_since_minute: None,
            resolved_at: None,
            opaque_case_ref: case.canonical_case_id.clone(),
        });
    ctx.db.local_problem_symptom().insert(LocalProblemSymptom {
        problem_id: case.problem_id.clone(),
        settlement_id: settlement_id.into(),
        symptom: symptom_name(consequence.symptom).into(),
        public_summary: consequence.public_summary.clone(),
        active_from: starts_at,
        active_until: ends_at,
    });
    Ok(())
}

fn incident_circumstance_label(
    value: adventuresim_core::quest_generation::Circumstance,
) -> &'static str {
    adventuresim_core::quest_catalog::catalog()
        .circumstance(value.as_str())
        .expect("generated circumstance exists in startup catalog")
        .statement
        .as_str()
}

fn incident_evidence_description(
    kind: adventuresim_core::quest_generation::EvidenceKind,
) -> &'static str {
    adventuresim_core::quest_catalog::catalog()
        .evidence(kind.as_str())
        .expect("generated evidence exists in startup catalog")
        .base_description
        .as_str()
}

fn follow_up_summary(symptom: &str) -> &'static str {
    match symptom {
        "missing_caravans" => "Another expected caravan has failed to arrive.",
        "night_screams" => "More troubling sounds were reported after dark.",
        "sick_locals" => "More local people have fallen ill.",
        "empty_stalls" => "Further shortages have left more market stalls empty.",
        "vanished_livestock" => "More livestock have disappeared from nearby holdings.",
        _ => "A further incident affecting local people was reported.",
    }
}

/// Append every follow-up incident that is due for an unresolved generated
/// problem. IDs and modular choices derive from immutable generation inputs,
/// so retries and delayed refreshes materialize the same bounded history.
pub(crate) fn ensure_generated_incidents(
    ctx: &ReducerContext,
    settlement_id: &str,
    minute: u64,
) -> Result<(), String> {
    ensure_generated_incidents_inner(ctx, settlement_id, minute, None, None)
}

pub(crate) fn trigger_next_generated_incident(
    ctx: &ReducerContext,
    problem_id: &str,
    occurred_at: u64,
) -> Result<u16, String> {
    let problem = ctx
        .db
        .local_problem_authority()
        .id()
        .find(problem_id.to_owned())
        .ok_or("Generated problem not found")?;
    if problem.resolved_at.is_some() {
        return Err("Resolved generated problems cannot receive another incident".into());
    }
    let scope: lp::Scope = serde_json::from_str(&problem.scope_json)
        .map_err(|_| "Generated problem scope authority is invalid")?;
    let lp::Scope::Settlement { settlement_id } = scope else {
        return Err("Only settlement generated problems support manual incidents".into());
    };
    let validated = validated_problem_generation(ctx, &problem, &settlement_id)
        .ok_or("Generated problem has no valid private generation authority")?;
    if !problem.recurring_hostile && problem.incident_count >= validated.manifest.maximum_incidents
    {
        return Err("Generated problem has reached its incident maximum".into());
    }
    let next = problem.incident_count.saturating_add(1);
    ensure_generated_incidents_inner(
        ctx,
        &settlement_id,
        occurred_at,
        Some(problem_id),
        Some(occurred_at),
    )?;
    Ok(next)
}

fn ensure_generated_incidents_inner(
    ctx: &ReducerContext,
    settlement_id: &str,
    minute: u64,
    target_problem_id: Option<&str>,
    forced_occurred_at: Option<u64>,
) -> Result<(), String> {
    let scope = format!("settlement:{settlement_id}");
    let problems: Vec<_> = ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&scope)
        .filter(|problem| is_active(problem, minute))
        .filter(|problem| target_problem_id.is_none_or(|id| problem.id == id))
        .collect();
    for mut problem in problems {
        let Some(validated) = validated_problem_generation(ctx, &problem, settlement_id) else {
            continue;
        };
        let configured_maximum = if problem.recurring_hostile {
            u16::MAX
        } else {
            validated.manifest.maximum_incidents
        };
        let total_due = if target_problem_id.is_some() {
            problem
                .incident_count
                .saturating_add(1)
                .min(configured_maximum)
        } else {
            lp::due_incident_count_configured(
                problem.starts_at,
                minute,
                validated.manifest.incident_interval_minutes,
                configured_maximum,
            )
        };
        // Bound one transaction's catch-up work. Repeated settlement refreshes
        // deterministically continue from the persisted ordinal.
        let due = total_due.min(problem.incident_count.saturating_add(16));
        if due <= problem.incident_count {
            continue;
        }
        let candidates = &validated.context.witness_candidates;
        let sites = &validated.manifest.sites;
        if candidates.is_empty() || sites.is_empty() {
            return Err("Generated incident has no persistent witness or case site".into());
        }
        for ordinal in problem.incident_count.saturating_add(1)..=due {
            let choice = adventuresim_core::settlement_population::stable_hash(&format!(
                "generated-incident-v1:{}:{ordinal}",
                validated.manifest.canonical_case_id
            ));
            let witness = &candidates[choice as usize % candidates.len()];
            let victim = &candidates[(choice.rotate_left(17) as usize) % candidates.len()];
            let site = &sites[(choice.rotate_left(31) as usize) % sites.len()];
            let circumstances: Vec<_> = witness.allowed_circumstances.iter().copied().collect();
            if circumstances.is_empty() {
                return Err("Generated incident witness has no valid circumstance".into());
            }
            let circumstance =
                circumstances[(choice.rotate_left(7) as usize) % circumstances.len()];
            let evidence_kind = adventuresim_core::quest_generation::select_follow_up_evidence(
                validated.manifest.cause,
                site.kind,
                choice.rotate_left(43),
            )
            .ok_or("Generated incident has no valid evidence relation")?;
            let evidence_description = incident_evidence_description(evidence_kind);
            let id = format!(
                "incident:{:016x}",
                adventuresim_core::settlement_population::stable_hash(&format!(
                    "{}:{ordinal}:incident",
                    validated.manifest.canonical_case_id
                ))
            );
            let event_id = format!("{id}:event");
            let proposition_id = format!("{id}:proposition");
            let evidence_id = format!("{id}:evidence");
            let occurred_at = forced_occurred_at.unwrap_or_else(|| {
                problem.starts_at.saturating_add(
                    u64::from(ordinal.saturating_sub(1))
                        .saturating_mul(validated.manifest.incident_interval_minutes),
                )
            });
            let public_summary = follow_up_summary(&problem.symptom).to_owned();
            let witness_account = format!(
                "{} reported seeing signs near {} while {}.",
                witness.display_name,
                site.safe_label,
                incident_circumstance_label(circumstance)
            );
            let incident = GeneratedProblemIncident {
                id: id.clone(),
                case_id: validated.manifest.canonical_case_id.clone(),
                problem_id: problem.id.clone(),
                ordinal,
                occurred_at,
                event_id: event_id.clone(),
                proposition_id: proposition_id.clone(),
                witness_resident_character_id: witness.resident_character_id,
                victim_resident_character_id: victim.resident_character_id,
                circumstance: circumstance.as_str().to_owned(),
                site_id: site.id.0.clone(),
                evidence_id: evidence_id.clone(),
                evidence_kind: evidence_kind.as_str().to_owned(),
                public_summary,
                witness_account,
            };
            if let Some(existing) = ctx.db.generated_problem_incident().id().find(&id) {
                if existing.ordinal != ordinal
                    || existing.case_id != incident.case_id
                    || existing.problem_id != incident.problem_id
                {
                    return Err("Generated incident identity conflicts with authority".into());
                }
                continue;
            }
            ctx.db
                .investigation_event_authority()
                .insert(InvestigationEventAuthority {
                    id: event_id,
                    case_id: incident.case_id.clone(),
                    canonical_propositions_json: serde_json::to_string(&[serde_json::json!({
                        "id": proposition_id,
                        "subject": victim.resident_character_id,
                        "predicate": "was affected by a further incident near",
                        "object": site.id.0,
                    })])
                    .map_err(|_| "Could not encode generated incident event")?,
                    occurred_at,
                });
            ctx.db
                .investigation_evidence_authority()
                .insert(InvestigationEvidenceAuthority {
                    id: evidence_id,
                    case_id: incident.case_id.clone(),
                    proposition_id: incident.proposition_id.clone(),
                    presentation_kind: EvidencePresentationKind::Physical,
                    authority_json: serde_json::to_string(&serde_json::json!({
                        "kind": incident.evidence_kind,
                        "safe_description": evidence_description,
                        "incident_id": incident.id,
                    }))
                    .map_err(|_| "Could not encode generated incident evidence")?,
                    hidden_coordinates_json: serde_json::to_string(&incident.site_id)
                        .map_err(|_| "Could not encode generated incident evidence site")?,
                });
            ctx.db.generated_problem_incident().insert(incident);
        }
        problem.incident_count = due;
        if problem.recurring_hostile {
            let adventuresim_core::quest_generation::CanonicalCause::Hostile(threat) =
                validated.manifest.cause
            else {
                return Err("Recurring-hostile authority has a non-hostile manifest".into());
            };
            let profile = adventuresim_core::bestiary::profile(threat);
            let next_awareness = adventuresim_core::threat_escalation::awareness_for_incident(
                profile.investigation.investigability,
                due,
            );
            problem.public_awareness_bps = problem.public_awareness_bps.max(next_awareness);
            if problem.public_since_minute.is_none()
                && adventuresim_core::threat_escalation::is_public(problem.public_awareness_bps)
            {
                problem.public_since_minute = forced_occurred_at.or_else(|| {
                    adventuresim_core::threat_escalation::scheduled_public_since_minute(
                        problem.starts_at,
                        validated.manifest.incident_interval_minutes,
                        profile.investigation.investigability,
                    )
                });
                if problem.public_since_minute.is_none() {
                    return Err("Public awareness crossed without a crossing ordinal".into());
                }
            }
            let Some((group_id, _, _, _)) = validated.manifest.hostile_groups.first() else {
                return Err("Recurring hostile case has no hostile group".into());
            };
            let mut group = ctx
                .db
                .hostile_group_authority()
                .id()
                .find(group_id)
                .ok_or("Recurring hostile group authority is missing")?;
            if group.disposition == crate::strategic::HostileGroupDisposition::Active
                && due > group.escalation_incident_ordinal
            {
                let escalated = adventuresim_core::threat_escalation::combat_for_incident(
                    group.base_enemy_count,
                    group.base_difficulty,
                    due,
                    profile.combat.escalation,
                );
                group.enemy_count = escalated.enemy_count;
                group.difficulty = escalated.difficulty;
                group.escalation_incident_ordinal = due;
                group.escalation_progress_bps = escalated.progress_bps;
                group.combat_scale_bps = escalated.combat_scale_bps;
                group.normalized_combat_power = escalated.normalized_combat_power;
                group.drop_quantity = if profile.combat.escalation.mode
                    == adventuresim_core::threat_escalation::EscalationMode::Mob
                {
                    escalated.enemy_count
                } else {
                    group.base_enemy_count
                };
                ctx.db.hostile_group_authority().id().update(group);
            }
        }
        ctx.db.local_problem_authority().id().update(problem);
    }
    Ok(())
}

pub fn ensure_settlement_problems(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let scope = lp::Scope::Settlement {
        settlement_id: settlement_id.into(),
    };
    let key = scope_key(&scope);
    let minute = official_minute(ctx);
    if ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&key)
        .filter(|p| is_active(p, minute))
        .count()
        >= 1
    {
        return Ok(());
    }
    let cycle = minute / (30 * MINUTES_PER_DAY);
    let private_entropy = ctx.random::<u64>();
    let context = lp::GenerationContext {
        seed: format!("private:{private_entropy:016x}:cycle:{cycle}"),
        scope: scope.clone(),
        allowed_bridges: BTreeSet::from(["secret_riverside_meeting".into()]),
    };
    let (problem, explanation) = lp::generate(&context, 0, minute)?;
    let disease_id = if problem.effects.disease_intensity > 0 {
        "influenza"
    } else {
        ""
    };
    ctx.db
        .local_problem_authority()
        .insert(LocalProblemAuthority {
            id: problem.id.0.clone(),
            gateway_bucket: 0,
            scope_key: key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode problem scope")?,
            consequence_mechanism: route_mechanism(problem.symptom).into(),
            symptom: symptom_name(problem.symptom).into(),
            buy_bps: problem.effects.buy_bps,
            sell_penalty_bps: problem.effects.sell_penalty_bps,
            encounter_frequency_bps: problem.effects.encounter_frequency_bps,
            encounter_archetype: problem.effects.encounter_archetype,
            disease_intensity: problem.effects.disease_intensity,
            disease_id: disease_id.into(),
            starts_at: problem.starts_at,
            ends_at: problem.ends_at,
            mitigation_bps: 0,
            incident_count: 1,
            recurring_hostile: false,
            public_awareness_bps: 0,
            public_since_minute: None,
            resolved_at: None,
            opaque_case_ref: format!("case:opaque:{}", problem.id.0),
        });
    ctx.db
        .local_problem_generation_explanation()
        .insert(LocalProblemGenerationExplanation {
            problem_id: problem.id.0.clone(),
            explanation_json: serde_json::to_string(&PrivateExplanation {
                context,
                explanation,
            })
            .map_err(|_| "Could not encode problem explanation")?,
        });
    ctx.db.local_problem_symptom().insert(LocalProblemSymptom {
        problem_id: problem.id.0.clone(),
        settlement_id: settlement_id.into(),
        symptom: symptom_name(problem.symptom).into(),
        public_summary: safe_summary(problem.symptom).into(),
        active_from: problem.starts_at,
        active_until: problem.ends_at,
    });
    Ok(())
}

pub fn ensure_route_problem(
    ctx: &ReducerContext,
    left: &str,
    right: &str,
    minute: u64,
) -> Result<(), String> {
    let scope = lp::Scope::route(left, right);
    let key = scope_key(&scope);
    if ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&key)
        .filter(|p| is_active(p, minute))
        .count()
        >= 1
    {
        return Ok(());
    }
    let cycle = minute / (30 * MINUTES_PER_DAY);
    let private_entropy = ctx.random::<u64>();
    let context = lp::GenerationContext {
        seed: format!("private:{private_entropy:016x}:route-cycle:{cycle}"),
        scope: scope.clone(),
        allowed_bridges: BTreeSet::new(),
    };
    let (problem, explanation) = lp::generate(&context, 0, minute)?;
    ctx.db
        .local_problem_authority()
        .insert(LocalProblemAuthority {
            id: problem.id.0.clone(),
            gateway_bucket: 0,
            scope_key: key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode route scope")?,
            consequence_mechanism: route_mechanism(problem.symptom).into(),
            symptom: symptom_name(problem.symptom).into(),
            buy_bps: 0,
            sell_penalty_bps: 0,
            encounter_frequency_bps: problem.effects.encounter_frequency_bps,
            encounter_archetype: problem.effects.encounter_archetype,
            disease_intensity: 0,
            disease_id: String::new(),
            starts_at: problem.starts_at,
            ends_at: problem.ends_at,
            mitigation_bps: 0,
            incident_count: 1,
            recurring_hostile: false,
            public_awareness_bps: 0,
            public_since_minute: None,
            resolved_at: None,
            opaque_case_ref: format!("case:opaque:{}", problem.id.0),
        });
    ctx.db
        .local_problem_generation_explanation()
        .insert(LocalProblemGenerationExplanation {
            problem_id: problem.id.0,
            explanation_json: serde_json::to_string(&PrivateExplanation {
                context,
                explanation,
            })
            .map_err(|_| "Could not encode route explanation")?,
        });
    Ok(())
}

fn is_active(row: &LocalProblemAuthority, minute: u64) -> bool {
    minute >= row.starts_at
        && (row.recurring_hostile || minute < row.ends_at)
        && row.resolved_at.is_none_or(|at| minute < at)
        && row.mitigation_bps < adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
}
fn scaled(value: i32, mitigation: u16) -> i32 {
    (i64::from(value)
        * i64::from(
            adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
                .saturating_sub(mitigation.min(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)),
        )
        / i64::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)) as i32
}
pub fn settlement_effects(
    ctx: &ReducerContext,
    settlement_id: &str,
    minute: u64,
) -> lp::AggregateEffects {
    let key = format!("settlement:{settlement_id}");
    let rows: Vec<_> = ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&key)
        .map(|r| lp::ConsequenceInput {
            id: r.id,
            buy_bps: r.buy_bps,
            sell_penalty_bps: r.sell_penalty_bps,
            encounter_frequency_bps: r.encounter_frequency_bps,
            disease_intensity: r.disease_intensity,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            mitigation_bps: r.mitigation_bps,
            incident_count: r.incident_count,
            resolved_at: r.resolved_at,
        })
        .collect();
    lp::aggregate_consequences(rows.iter(), minute)
}

pub fn route_encounter_influence(
    ctx: &ReducerContext,
    left: &str,
    right: &str,
    minute: u64,
) -> Option<adventuresim_core::encounter::LocalProblemInfluence> {
    let scope = lp::Scope::route(left, right);
    let key = scope_key(&scope);
    let mut rows: Vec<_> = ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&key)
        .filter(|r| is_active(r, minute))
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.truncate(lp::MAX_ACTIVE_PER_SCOPE);
    let frequency = rows
        .iter()
        .map(|r| scaled(i32::from(r.encounter_frequency_bps), r.mitigation_bps).max(0) as u32)
        .sum::<u32>()
        .min(u32::from(lp::MAX_ENCOUNTER_BPS)) as u16;
    let archetype = rows
        .iter()
        .filter(|row| row.encounter_frequency_bps > 0)
        .find_map(|row| row.encounter_archetype);
    (frequency > 0).then_some(adventuresim_core::encounter::LocalProblemInfluence {
        frequency_bonus_basis_points: frequency,
        archetype,
    })
}

/// Internal, monotonic and idempotent future-outcome boundary for #186.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LocalProblemOutcomeInput {
    pub source_outcome_id: String,
    pub at_minute: u64,
    pub mitigation_bps: u16,
    pub resolve: bool,
}
pub(crate) fn apply_outcome(
    ctx: &ReducerContext,
    problem_id: &str,
    input: &LocalProblemOutcomeInput,
) -> Result<(), String> {
    if input.source_outcome_id.is_empty()
        || input.source_outcome_id.len() > 160
        || input.mitigation_bps > adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
    {
        return Err("Invalid source outcome ID".into());
    }
    let fingerprint =
        serde_json::to_string(input).map_err(|_| "Could not encode outcome payload")?;
    let receipt_id = format!("{problem_id}:{}", input.source_outcome_id);
    if let Some(existing) = ctx
        .db
        .local_problem_outcome_receipt()
        .id()
        .find(&receipt_id)
    {
        return if existing.payload_fingerprint == fingerprint {
            Ok(())
        } else {
            Err("Conflicting retry for source outcome ID".into())
        };
    }
    if input.at_minute != official_minute(ctx) {
        return Err("Outcome minute is not the authoritative strategic minute".into());
    }
    let mut problem = ctx
        .db
        .local_problem_authority()
        .id()
        .find(problem_id.to_owned())
        .ok_or("Local problem not found")?;
    problem.mitigation_bps = problem.mitigation_bps.max(input.mitigation_bps);
    if input.resolve {
        problem.resolved_at = Some(
            problem
                .resolved_at
                .map_or(input.at_minute, |old| old.min(input.at_minute)),
        );
    }
    ctx.db
        .local_problem_authority()
        .id()
        .update(problem.clone());
    if (input.resolve
        || problem.mitigation_bps == adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)
        && let Some(mut symptom) = ctx
            .db
            .local_problem_symptom()
            .problem_id()
            .find(problem_id.to_owned())
    {
        symptom.active_until = symptom.active_until.min(input.at_minute);
        ctx.db.local_problem_symptom().problem_id().update(symptom);
    }
    ctx.db
        .local_problem_outcome_receipt()
        .insert(LocalProblemOutcomeReceipt {
            id: receipt_id,
            problem_id: problem_id.into(),
            source_outcome_id: input.source_outcome_id.clone(),
            applied_at: input.at_minute,
            mitigation_bps: input.mitigation_bps,
            resolved: input.resolve,
            payload_fingerprint: fingerprint,
        });
    Ok(())
}

fn validated_problem_generation(
    ctx: &ReducerContext,
    problem: &LocalProblemAuthority,
    settlement_id: &str,
) -> Option<ValidatedQuestGenerationAuthority> {
    let mut candidates = Vec::new();
    if let Some(authority) = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&problem.opaque_case_ref)
    {
        candidates.push(authority);
    }
    candidates.extend(
        ctx.db
            .quest_generation_authority()
            .public_case_id()
            .filter(&problem.opaque_case_ref),
    );
    candidates.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    candidates.dedup_by(|left, right| left.case_id == right.case_id);
    if candidates.len() != 1 {
        return None;
    }
    let validated = validate_quest_generation_authority(&candidates[0]).ok()?;
    let settlement = ctx.db.settlement().id().find(settlement_id.to_string())?;
    if validated.manifest.canonical_case_id != problem.opaque_case_ref
        || validated.manifest.problem_id != problem.id
        || validated.context.settlement_id != settlement_id
        || validated.context.settlement_name != settlement.name
        || problem.scope_key != format!("settlement:{settlement_id}")
    {
        return None;
    }
    Some(validated)
}

fn referral_location_label(
    ctx: &ReducerContext,
    problem: &LocalProblemAuthority,
    receipt: &LocalProblemReceipt,
) -> Option<String> {
    let validated = validated_problem_generation(ctx, problem, &receipt.settlement_id)?;
    if problem.opaque_case_ref != receipt.opaque_case_ref || problem.id != receipt.problem_id {
        return None;
    }
    validated
        .manifest
        .witnesses
        .iter()
        .find(|witness| {
            witness.resident_character_id == receipt.contact_resident_character_id
                && witness.expected_location == receipt.expected_location_id
        })
        .map(adventuresim_core::quest_generation::referral_display_location)
        .map(str::to_owned)
        .filter(|label| !label.is_empty())
}

fn stable_eligible_candidates<T, K: Ord>(
    candidates: impl IntoIterator<Item = T>,
    limit: usize,
    mut eligible: impl FnMut(&T) -> bool,
    mut stable_key: impl FnMut(&T) -> K,
) -> Vec<T> {
    let mut eligible_candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| eligible(candidate))
        .collect();
    eligible_candidates.sort_by_key(|candidate| stable_key(candidate));
    eligible_candidates.truncate(limit);
    eligible_candidates
}

const MAX_HEARING_GRAPH_NODES: usize = 4_096;
const MAX_HEARING_GRAPH_EDGES: usize = 16_384;
const MAX_HEARING_DISTANCE_M: u64 = 18 * 25_000;

#[derive(Default)]
struct BoundedHearingGraph {
    distances: HashMap<u64, u64>,
    adjacent_settlements: HashSet<u64>,
    visited_nodes: usize,
    inspected_edges: usize,
}

fn bounded_hearing_graph(ctx: &ReducerContext, listener_node: u64) -> BoundedHearingGraph {
    let settlements = ctx
        .db
        .settlement()
        .iter()
        .filter_map(|settlement| settlement.source_node_id)
        .collect::<HashSet<_>>();
    let mut graph = BoundedHearingGraph {
        distances: HashMap::from([(listener_node, 0)]),
        ..Default::default()
    };
    // `crossed_settlement` lets this one bounded shortest-path traversal also
    // identify the first settlement reached along each road branch.
    let mut state_distances = HashMap::from([((listener_node, false), 0_u64)]);
    let mut pending = BinaryHeap::from([Reverse((0_u64, listener_node, false))]);
    while let Some(Reverse((distance, node, crossed_settlement))) = pending.pop() {
        if distance > MAX_HEARING_DISTANCE_M
            || graph.visited_nodes >= MAX_HEARING_GRAPH_NODES
            || graph.inspected_edges >= MAX_HEARING_GRAPH_EDGES
        {
            continue;
        }
        if state_distances
            .get(&(node, crossed_settlement))
            .is_some_and(|known| *known != distance)
        {
            continue;
        }
        graph.visited_nodes += 1;
        let reached_settlement = node != listener_node && settlements.contains(&node);
        if reached_settlement && !crossed_settlement {
            graph.adjacent_settlements.insert(node);
        }
        let next_crossed = crossed_settlement || reached_settlement;
        let mut neighbors = ctx
            .db
            .travel_edge()
            .from_node_id()
            .filter(&node)
            .map(|edge| (edge.to_node_id, edge.length_m))
            .collect::<Vec<_>>();
        neighbors.extend(
            ctx.db
                .travel_edge()
                .to_node_id()
                .filter(&node)
                .map(|edge| (edge.from_node_id, edge.length_m)),
        );
        neighbors.sort_unstable();
        neighbors.dedup();
        for (next, length) in neighbors {
            graph.inspected_edges += 1;
            if graph.inspected_edges > MAX_HEARING_GRAPH_EDGES {
                break;
            }
            let next_distance = distance.saturating_add(u64::from(length));
            if next_distance > MAX_HEARING_DISTANCE_M {
                continue;
            }
            graph
                .distances
                .entry(next)
                .and_modify(|known| *known = (*known).min(next_distance))
                .or_insert(next_distance);
            let state = (next, next_crossed);
            if state_distances
                .get(&state)
                .is_none_or(|known| next_distance < *known)
            {
                state_distances.insert(state, next_distance);
                pending.push(Reverse((next_distance, next, next_crossed)));
            }
        }
    }
    graph
}

fn source_may_disclose_public_threat(
    ctx: &ReducerContext,
    character_id: u64,
    source_npc: &crate::settlement_population::SettlementResidentProfile,
    listener_settlement_id: &str,
    location_id: &str,
    minute: u64,
) -> Option<&'static str> {
    let organization_id = crate::strategic::exact_organization_representative(
        ctx,
        source_npc,
        listener_settlement_id,
        location_id,
    );
    let organization = organization_id
        .as_deref()
        .and_then(adventuresim_core::organization::organization);
    let current_member = organization.and_then(|organization| {
        crate::organization::membership(ctx, character_id, &organization.id)
            .filter(|membership| crate::organization::membership_is_current(membership, minute))
    });
    adventuresim_core::threat_escalation::public_referral_source(
        source_npc.home_settlement_id == listener_settlement_id,
        source_npc.service_id == "inn" && location_id == "inn",
        organization.is_some_and(|organization| organization.public_threat_referrals),
        current_member.is_some(),
    )
}

fn public_threat_in_hearing_range(
    graph: &BoundedHearingGraph,
    listener_settlement_id: &str,
    afflicted_settlement_id: &str,
    afflicted_node: Option<u64>,
    listener_population: u32,
    public_awareness_bps: u16,
    normalized_combat_power: u32,
) -> bool {
    adventuresim_core::threat_escalation::hearing_allows(
        listener_settlement_id == afflicted_settlement_id,
        afflicted_node.is_some_and(|node| graph.adjacent_settlements.contains(&node)),
        afflicted_node.and_then(|node| graph.distances.get(&node).copied()),
        listener_population,
        normalized_combat_power,
        public_awareness_bps,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "threat publication keeps each authority and timing input explicit"
)]
fn surface_public_threat(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: &str,
    source_npc: &crate::settlement_population::SettlementResidentProfile,
    listener_settlement_id: &str,
    location_id: &str,
    observer_minute: u64,
    official_world_minute: u64,
) -> Result<bool, String> {
    let Some(source_kind) = source_may_disclose_public_threat(
        ctx,
        character_id,
        source_npc,
        listener_settlement_id,
        location_id,
        observer_minute,
    ) else {
        return Ok(false);
    };
    let listener = ctx
        .db
        .settlement()
        .id()
        .find(listener_settlement_id.to_string())
        .ok_or("Listener settlement is missing")?;
    let graph = listener
        .source_node_id
        .map(|node| bounded_hearing_graph(ctx, node))
        .unwrap_or_default();
    // The one listener-centric graph is independent of case count. Use it to
    // discard remote scopes cheaply before the deterministic candidate cap, so
    // old remote cases cannot starve a nearby referral.
    let public_problems = ctx
        .db
        .local_problem_authority()
        .iter()
        .filter(|problem| {
            problem.recurring_hostile
                && problem.public_since_minute.is_some()
                && is_active(problem, official_world_minute)
        })
        .filter_map(|problem| {
            let lp::Scope::Settlement {
                settlement_id: afflicted,
            } = serde_json::from_str(&problem.scope_json).ok()?
            else {
                return None;
            };
            let settlement = ctx.db.settlement().id().find(&afflicted)?;
            let plausibly_local = afflicted == listener_settlement_id
                || settlement.source_node_id.is_some_and(|node| {
                    graph.adjacent_settlements.contains(&node)
                        || graph.distances.contains_key(&node)
                });
            plausibly_local.then_some((
                problem.public_since_minute.unwrap_or(u64::MAX),
                afflicted.clone(),
                problem.id.clone(),
                (problem, afflicted, settlement.source_node_id),
            ))
        })
        .collect::<Vec<_>>();
    let public_problems = bounded_public_candidates(public_problems);
    let mut candidates = public_problems
        .into_iter()
        .filter_map(|(problem, afflicted, afflicted_node)| {
            let validated = validated_problem_generation(ctx, &problem, &afflicted)?;
            let (group_id, site_id, threat, _) = validated.manifest.hostile_groups.first()?;
            let group_id = group_id.clone();
            let site_id = site_id.clone();
            let threat = *threat;
            let group = ctx.db.hostile_group_authority().id().find(&group_id)?;
            public_threat_in_hearing_range(
                &graph,
                listener_settlement_id,
                &afflicted,
                afflicted_node,
                listener.population_estimate,
                problem.public_awareness_bps,
                group.normalized_combat_power,
            )
            .then_some((validated, site_id, threat, group))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(validated, _, _, _)| {
        let already_known = ctx
            .db
            .public_threat_disclosure()
            .id()
            .find(format!(
                "public-threat:{character_id}:{}",
                validated.manifest.public_case_id
            ))
            .is_some();
        let problem = ctx
            .db
            .local_problem_authority()
            .id()
            .find(&validated.manifest.problem_id);
        (
            already_known,
            problem
                .and_then(|problem| problem.public_since_minute)
                .unwrap_or(u64::MAX),
            validated.manifest.public_case_id.clone(),
        )
    });
    candidates.truncate(lp::MAX_ACTIVE_PER_SCOPE);
    let Some((validated, site_id, threat, group)) = candidates.into_iter().next() else {
        return Ok(false);
    };
    let site = validated
        .manifest
        .sites
        .iter()
        .find(|candidate| candidate.id == site_id && candidate.is_true_location)
        .ok_or("Public hostile case has no canonical true site")?;
    let case_site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&site.id.0)
        .ok_or("Public hostile case site authority is missing")?;
    let threat_name = adventuresim_core::bestiary::profile(threat).display_name;
    let count_band =
        adventuresim_core::threat_escalation::approximate_count_band(group.enemy_count);
    let disclosure_id = format!(
        "public-threat:{character_id}:{}",
        validated.manifest.public_case_id
    );
    let disclosure = PublicThreatDisclosure {
        id: disclosure_id.clone(),
        character_id,
        public_case_id: validated.manifest.public_case_id.clone(),
        threat_type: threat.as_str().into(),
        exact_site_id: case_site.id.clone(),
        approximate_count: count_band.into(),
        source_kind: source_kind.into(),
        source_resident_character_id: source_npc.character_id,
        learned_at: observer_minute,
    };
    if ctx
        .db
        .public_threat_disclosure()
        .id()
        .find(&disclosure_id)
        .as_ref()
        != Some(&disclosure)
    {
        if ctx
            .db
            .public_threat_disclosure()
            .id()
            .find(&disclosure_id)
            .is_some()
        {
            ctx.db
                .public_threat_disclosure()
                .id()
                .update(disclosure.clone());
        } else {
            ctx.db.public_threat_disclosure().insert(disclosure);
        }
    }
    for mut lead in ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(&character_id)
        .filter(|lead| {
            lead.case_id == validated.manifest.public_case_id
                && lead.exact_location_id != site.id.0
                && lead.corrected_by.is_empty()
        })
        .collect::<Vec<_>>()
    {
        lead.corrected_by = disclosure_id.clone();
        ctx.db.investigation_lead().id().update(lead);
    }
    crate::investigation::disclose_exact_case_site(
        ctx,
        character_id,
        &validated.manifest.public_case_id,
        &case_site,
        source_kind,
    )?;
    crate::investigation::upsert_public_threat_journal_notice(
        ctx,
        character_id,
        &validated.manifest.public_case_id,
        &adventuresim_core::threat_escalation::public_threat_summary(
            threat_name,
            &site.safe_label,
            count_band,
        ),
        source_kind,
        observer_minute,
    )?;
    ctx.db
        .local_problem_rumor_delivery()
        .insert(LocalProblemRumorDelivery {
            id: format!("{session_id}:rumor"),
            character_id,
            settlement_id: listener_settlement_id.into(),
            session_id: session_id.into(),
            receipt_id: disclosure_id,
            fragments_json: serde_json::to_string(&vec![
                adventuresim_dialogue::Fragment::Text {
                    value: format!(
                        "{threat_name} are publicly known to be at {}. Reports put their number at {count_band}.",
                        site.safe_label
                    ),
                },
            ])
            .map_err(|_| "Could not encode public threat referral")?,
        });
    Ok(true)
}

#[expect(
    clippy::too_many_arguments,
    reason = "problem publication keeps each authority and timing input explicit"
)]
fn surface_new_problem(
    ctx: &ReducerContext,
    problem: &LocalProblemAuthority,
    character_id: u64,
    session_id: &str,
    source_resident_character_id: u64,
    source_npc: Option<&crate::settlement_population::ResolvedSettlementResident>,
    settlement_id: &str,
    observer_minute: u64,
    official_world_minute: u64,
) -> Result<bool, String> {
    if ctx
        .db
        .local_problem_receipt()
        .id()
        .find(format!("{character_id}:{}", problem.id))
        .is_some()
    {
        return Ok(false);
    }
    let Some(validated) = validated_problem_generation(ctx, problem, settlement_id) else {
        return Ok(false);
    };
    let Some(witness) = validated.manifest.witnesses.first() else {
        return Ok(false);
    };
    let Some(contact) = crate::settlement_population::resolve_settlement_resident(
        ctx,
        witness.resident_character_id,
    ) else {
        return Ok(false);
    };
    let Some(settlement) = ctx.db.settlement().id().find(settlement_id.to_owned()) else {
        return Ok(false);
    };
    let has_keep = matches!(
        settlement.category,
        crate::strategic::SettlementCategory::Town
            | crate::strategic::SettlementCategory::City
            | crate::strategic::SettlementCategory::Capital
    );
    let expected_location_is_navigable =
        adventuresim_core::settlement_economy::npc_location_is_navigable(
            &settlement.economy,
            has_keep,
            settlement_id,
            &witness.expected_location,
        );
    if !rumor_contact_is_valid(
        &contact.home_settlement_id,
        settlement_id,
        expected_location_is_navigable,
    ) {
        return Ok(false);
    }
    let Some(symptom) = ctx
        .db
        .local_problem_symptom()
        .problem_id()
        .find(&problem.id)
    else {
        return Ok(false);
    };
    let location_label = adventuresim_core::quest_generation::referral_display_location(witness);
    if location_label.is_empty() {
        return Ok(false);
    }
    let source_identity = source_npc.map(|npc| npc.character_id.to_string());
    let source = source_npc
        .zip(source_identity.as_deref())
        .map(|(npc, id)| (id, npc.name.as_str()));
    let contact_identity = contact.character_id.to_string();
    let presentation = lp::referral_presentation(
        &symptom.public_summary,
        source,
        &contact_identity,
        &contact.name,
        &contact.profession,
        &contact.height,
        &contact.build,
        &contact.hair,
        location_label,
    );
    let receipt_id = format!("{character_id}:{}", problem.id);
    ctx.db.local_problem_receipt().insert(LocalProblemReceipt {
        id: receipt_id.clone(),
        character_id,
        settlement_id: settlement_id.into(),
        problem_id: problem.id.clone(),
        opaque_case_ref: problem.opaque_case_ref.clone(),
        source_resident_character_id,
        discovery_session_id: session_id.into(),
        contact_resident_character_id: contact.character_id,
        expected_location_id: witness.expected_location.clone(),
        safe_summary: symptom.public_summary,
        learned_at: observer_minute,
        official_learned_at: official_world_minute,
    });
    ctx.db
        .local_problem_rumor_delivery()
        .insert(LocalProblemRumorDelivery {
            id: format!("{session_id}:rumor"),
            character_id,
            settlement_id: settlement_id.into(),
            session_id: session_id.into(),
            receipt_id,
            fragments_json: referral_fragments_json(presentation)?,
        });
    Ok(true)
}

fn rumor_contact_is_valid(
    contact_home_settlement_id: &str,
    problem_settlement_id: &str,
    expected_location_is_navigable: bool,
) -> bool {
    contact_home_settlement_id == problem_settlement_id && expected_location_is_navigable
}

/// Development-gallery seam that establishes the same receipt, referral, and
/// journal-visible action graph as a completed local rumor interaction, plus
/// the dry journal notice required to index the case immediately. It accepts
/// only one exact already-materialized problem in the selected character's
/// current settlement.
pub(crate) fn discover_development_problem(
    ctx: &ReducerContext,
    character_id: u64,
    problem_id: &str,
    scenario_slug: &str,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Development quest discovery requires a settlement")?;
    let problem = ctx
        .db
        .local_problem_authority()
        .id()
        .find(problem_id.to_owned())
        .ok_or("Development quest problem is missing")?;
    if problem.scope_key != format!("settlement:{settlement_id}") {
        return Err("Development quest problem is outside its scenario settlement".into());
    }
    let validated = validated_problem_generation(ctx, &problem, &settlement_id)
        .ok_or("Development quest problem has invalid generation authority")?;
    let witness = validated
        .manifest
        .witnesses
        .first()
        .ok_or("Development quest problem has no rumor witness")?;
    let source = crate::settlement_population::resolve_settlement_resident(
        ctx,
        witness.resident_character_id,
    )
    .ok_or("Development quest rumor witness is missing")?;
    let observer_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Development quest character time is missing")?
        .minutes;
    let official_world_minute = official_minute(ctx);
    let session_id = format!("development-scenario:{scenario_slug}:rumor");
    surface_new_problem(
        ctx,
        &problem,
        character_id,
        &session_id,
        source.character_id,
        Some(&source),
        &settlement_id,
        observer_minute,
        official_world_minute,
    )?;
    let receipt_id = format!("{character_id}:{}", problem.id);
    crate::investigation::receive_local_problem_rumor_for_development_bootstrap(
        ctx,
        character_id,
        receipt_id.clone(),
        format!("development-scenario:{scenario_slug}:receive-rumor"),
    )?;
    let receipt = ctx
        .db
        .local_problem_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Development quest rumor receipt is missing")?;
    crate::investigation::record_journal_notice(
        ctx,
        character_id,
        &validated.manifest.public_case_id,
        &format!("development-scenario:{scenario_slug}:discovered-rumor"),
        &receipt.safe_summary,
        "local rumor",
        receipt.learned_at,
    )?;
    ctx.db
        .local_problem_rumor_preference()
        .character_id()
        .delete(character_id);
    Ok(())
}

/// Surface at most one active problem. A private one-shot preference may order
/// an explicit development demo first, but disclosure still occurs through
/// ordinary eligible rumor dialogue and creates the normal observer receipt.
pub fn surface_problem(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: &str,
    source_resident_character_id: u64,
    location_id: &str,
) -> Result<(), String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Problem discovery requires a settlement")?;
    let source_npc = crate::settlement_population::resolve_settlement_resident(
        ctx,
        source_resident_character_id,
    )
    .filter(|npc| npc.home_settlement_id == settlement_id);
    // Problem authority is anchored to the official world clock. A character's
    // elapsed clock is an observer timeline and may be ahead of or behind it
    // after travel, treatment, or bulk settlement activity.
    let official_world_minute = official_minute(ctx);
    let observer_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let scope = format!("settlement:{settlement_id}");
    let active_problems = stable_eligible_candidates(
        ctx.db
            .local_problem_authority()
            .scope_key()
            .filter(&scope)
            .filter(|problem| is_active(problem, official_world_minute)),
        lp::MAX_ACTIVE_PER_SCOPE,
        |problem| validated_problem_generation(ctx, problem, &settlement_id).is_some(),
        |problem| (problem.id.clone(), problem.opaque_case_ref.clone()),
    );
    let inn_service = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .is_some_and(|s| {
            s.economy
                .has_service(adventuresim_world_schema::SettlementService::Inn)
        });
    let inn_available = inn_service
        && ctx
            .db
            .settlement_resident_presence()
            .settlement_id()
            .filter(&settlement_id)
            .any(|presence| {
                let npc = ctx
                    .db
                    .settlement_resident_profile()
                    .character_id()
                    .find(presence.character_id);
                dialogue_capable_inn_contact(
                    &presence.settlement_id,
                    &settlement_id,
                    &presence.location_id,
                    crate::settlement_population::npc_is_present(ctx, &presence, observer_minute),
                    npc.as_ref().is_some_and(|npc| {
                        npc.home_settlement_id == settlement_id
                            && crate::settlement_population::resident_is_dialogue_capable(npc)
                    }),
                )
            });
    let can_discover_new =
        lp::discovery_action(location_id, inn_available, false) == lp::DiscoveryAction::NewRumor;
    if can_discover_new
        && let Some(preference) = ctx
            .db
            .local_problem_rumor_preference()
            .character_id()
            .find(character_id)
        && preference.settlement_id == settlement_id
    {
        let preferred_problem = ctx
            .db
            .local_problem_authority()
            .id()
            .find(&preference.problem_id)
            .filter(|problem| {
                problem.scope_key == scope
                    && is_active(problem, official_world_minute)
                    && validated_problem_generation(ctx, problem, &settlement_id).is_some()
            });
        let already_known = preferred_problem.as_ref().is_some_and(|problem| {
            ctx.db
                .local_problem_receipt()
                .id()
                .find(format!("{character_id}:{}", problem.id))
                .is_some()
        });
        let surfaced = if let Some(problem) = preferred_problem.as_ref()
            && !already_known
        {
            surface_new_problem(
                ctx,
                problem,
                character_id,
                session_id,
                source_resident_character_id,
                source_npc.as_ref(),
                &settlement_id,
                observer_minute,
                official_world_minute,
            )?
        } else {
            false
        };
        if surfaced || preferred_problem.is_none() || already_known {
            ctx.db
                .local_problem_rumor_preference()
                .character_id()
                .delete(character_id);
        }
        if surfaced {
            return Ok(());
        }
    }
    if let Some(source_npc) = source_npc.as_ref()
        && surface_public_threat(
            ctx,
            character_id,
            session_id,
            source_npc,
            &settlement_id,
            location_id,
            observer_minute,
            official_world_minute,
        )?
    {
        return Ok(());
    }
    for problem in &active_problems {
        let Some(receipt) = ctx
            .db
            .local_problem_receipt()
            .id()
            .find(format!("{character_id}:{}", problem.id))
        else {
            continue;
        };
        if receipt.character_id != character_id || receipt.settlement_id != settlement_id {
            continue;
        }
        let Some(validated) = validated_problem_generation(ctx, problem, &settlement_id) else {
            continue;
        };
        let mut pending_incidents: Vec<_> = ctx
            .db
            .generated_problem_incident()
            .problem_id()
            .filter(&problem.id)
            .filter(|incident| {
                ctx.db
                    .local_problem_incident_receipt()
                    .id()
                    .find(format!("{character_id}:{}", incident.id))
                    .is_none()
            })
            .collect();
        pending_incidents.sort_by_key(|incident| (incident.ordinal, incident.id.clone()));
        if let Some(incident) = pending_incidents.first() {
            let incident_receipt_id = format!("{character_id}:{}", incident.id);
            ctx.db
                .local_problem_incident_receipt()
                .insert(LocalProblemIncidentReceipt {
                    id: incident_receipt_id.clone(),
                    character_id,
                    problem_id: problem.id.clone(),
                    incident_id: incident.id.clone(),
                    learned_at: observer_minute,
                });
            ctx.db.investigation_lead().insert(InvestigationLead {
                id: format!("lead:{incident_receipt_id}"),
                owner_character_id: character_id,
                case_id: validated.manifest.public_case_id,
                proposition_id: incident.proposition_id.clone(),
                summary: incident.public_summary.clone(),
                source_label: "local report".into(),
                confidence_bps: 5_000,
                destination_stage:
                    adventuresim_core::investigation::DestinationKnowledgeStage::Unknown,
                directions: String::new(),
                exact_location_id: String::new(),
                latitude_e7: 0,
                longitude_e7: 0,
                witness_name: String::new(),
                witness_description: String::new(),
                witness_occupation_or_relationship: String::new(),
                expected_location: String::new(),
                current_learned_location: String::new(),
                contradiction_group: format!("incident:{}", incident.id),
                corrected_by: String::new(),
                recorded_at: observer_minute,
            });
            ctx.db
                .local_problem_rumor_delivery()
                .insert(LocalProblemRumorDelivery {
                    id: format!("{session_id}:rumor"),
                    character_id,
                    settlement_id,
                    session_id: session_id.into(),
                    receipt_id: receipt.id,
                    fragments_json: serde_json::to_string(&vec![
                        adventuresim_dialogue::Fragment::Text {
                            value: incident.public_summary.clone(),
                        },
                    ])
                    .map_err(|error| {
                        format!("failed to serialize follow-up incident dialogue: {error}")
                    })?,
                });
            return Ok(());
        }
        let Some(contact) = crate::settlement_population::resolve_settlement_resident(
            ctx,
            receipt.contact_resident_character_id,
        ) else {
            continue;
        };
        let Some(location_label) = referral_location_label(ctx, problem, &receipt) else {
            continue;
        };
        let present = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(receipt.contact_resident_character_id)
            .is_some_and(|presence| {
                presence.settlement_id == settlement_id
                    && presence.location_id == receipt.expected_location_id
            });
        if !present {
            continue;
        }
        let source_identity = source_npc.as_ref().map(|npc| npc.character_id.to_string());
        let source = source_npc
            .as_ref()
            .zip(source_identity.as_deref())
            .map(|(npc, id)| (id, npc.name.as_str()));
        let contact_identity = contact.character_id.to_string();
        ctx.db
            .local_problem_rumor_delivery()
            .insert(LocalProblemRumorDelivery {
                id: format!("{session_id}:rumor"),
                character_id,
                settlement_id,
                session_id: session_id.into(),
                receipt_id: receipt.id,
                fragments_json: referral_fragments_json(lp::referral_presentation(
                    &receipt.safe_summary,
                    source,
                    &contact_identity,
                    &contact.name,
                    &contact.profession,
                    &contact.height,
                    &contact.build,
                    &contact.hair,
                    &location_label,
                ))?,
            });
        return Ok(());
    }
    if !can_discover_new {
        return Ok(());
    }
    for problem in active_problems {
        if surface_new_problem(
            ctx,
            &problem,
            character_id,
            session_id,
            source_resident_character_id,
            source_npc.as_ref(),
            &settlement_id,
            observer_minute,
            official_world_minute,
        )? {
            return Ok(());
        }
    }
    Ok(())
}

fn dialogue_capable_inn_contact(
    presence_settlement_id: &str,
    settlement_id: &str,
    location_id: &str,
    present_now: bool,
    has_dialogue_capable_npc: bool,
) -> bool {
    presence_settlement_id == settlement_id
        && location_id == "inn"
        && present_now
        && has_dialogue_capable_npc
}

fn referral_fragments_json(presentation: lp::ReferralPresentation) -> Result<String, String> {
    let mut fragments = vec![adventuresim_dialogue::Fragment::Text {
        value: presentation.lead,
    }];
    if let Some((label, topic)) = presentation.topic {
        fragments.push(adventuresim_dialogue::Fragment::Topic { topic, label });
    }
    if !presentation.trailing.is_empty() {
        fragments.push(adventuresim_dialogue::Fragment::Text {
            value: presentation.trailing,
        });
    }
    serde_json::to_string(&fragments)
        .map_err(|error| format!("failed to serialize referral dialogue: {error}"))
}

#[cfg(test)]
mod tests {
    use crate::local_problem::{
        BoundedHearingGraph, MAX_HEARING_DISTANCE_M, MAX_HEARING_GRAPH_EDGES,
        MAX_HEARING_GRAPH_NODES, dialogue_capable_inn_contact, public_threat_in_hearing_range,
        referral_fragments_json, rumor_contact_is_valid, stable_eligible_candidates,
    };
    use adventuresim_core::threat_escalation::{
        MAX_PUBLIC_THREAT_CANDIDATES, bounded_public_threat_candidates as bounded_public_candidates,
    };
    use std::collections::{HashMap, HashSet};

    #[test]
    fn referral_access_is_closed_and_presentation_independent() {
        assert_eq!(
            adventuresim_core::threat_escalation::public_referral_source(true, true, false, false),
            Some("innkeeper")
        );
        for current in [false, true] {
            assert_eq!(
                adventuresim_core::threat_escalation::public_referral_source(
                    false, false, true, current
                ),
                None,
                "wrong settlement is always rejected"
            );
        }
        assert_eq!(
            adventuresim_core::threat_escalation::public_referral_source(true, false, true, false),
            None,
            "nonmember and suspended member are rejected"
        );
        assert_eq!(
            adventuresim_core::threat_escalation::public_referral_source(true, false, true, true),
            Some("organization")
        );
        assert_eq!(
            adventuresim_core::threat_escalation::public_referral_source(true, false, false, true),
            None,
            "wrong speaker or chapter location is rejected"
        );
    }

    #[test]
    fn hearing_boundaries_and_candidate_budget_are_deterministic() {
        let graph = BoundedHearingGraph {
            distances: HashMap::from([(2, 25_000), (3, 25_001)]),
            adjacent_settlements: HashSet::from([4]),
            visited_nodes: MAX_HEARING_GRAPH_NODES,
            inspected_edges: MAX_HEARING_GRAPH_EDGES,
        };
        assert!(public_threat_in_hearing_range(
            &graph, "home", "home", None, 0, 6_500, 10_000
        ));
        assert!(public_threat_in_hearing_range(
            &graph,
            "home",
            "adjacent",
            Some(4),
            0,
            6_500,
            10_000
        ));
        assert!(public_threat_in_hearing_range(
            &graph,
            "home",
            "edge",
            Some(2),
            0,
            6_500,
            10_000
        ));
        assert!(!public_threat_in_hearing_range(
            &graph,
            "home",
            "beyond",
            Some(3),
            0,
            6_500,
            10_000
        ));
        assert!(!public_threat_in_hearing_range(
            &graph,
            "home",
            "disconnected",
            Some(5),
            u32::MAX,
            10_000,
            300_000
        ));
        assert_eq!(MAX_HEARING_DISTANCE_M, 450_000);

        let inputs = (0..100)
            .rev()
            .map(|index| {
                (
                    index,
                    format!("settlement-{index:03}"),
                    format!("problem-{index:03}"),
                    index,
                )
            })
            .collect();
        let selected = bounded_public_candidates(inputs);
        assert_eq!(selected.len(), MAX_PUBLIC_THREAT_CANDIDATES);
        assert_eq!(
            selected,
            (0..MAX_PUBLIC_THREAT_CANDIDATES as u64).collect::<Vec<_>>()
        );
    }

    #[test]
    fn durable_public_summary_contains_only_the_disclosed_triplet() {
        let summary = adventuresim_core::threat_escalation::public_threat_summary(
            "Orcs",
            "Old Quarry",
            "a few (2–4)",
        );
        assert_eq!(summary, "Orcs at Old Quarry; reported number: a few (2–4).");
        for secret in [
            "evidence",
            "testimony",
            "preparation",
            "manifest",
            "canonical",
        ] {
            assert!(!summary.to_ascii_lowercase().contains(secret));
        }
    }

    #[test]
    fn public_referrals_share_canonical_disclosure_and_closed_authorization() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        let authorization = source
            .split("fn source_may_disclose_public_threat")
            .nth(1)
            .and_then(|tail| tail.split("fn public_threat_in_hearing_range").next())
            .expect("public referral authorization");
        assert!(authorization.contains("source_npc.service_id == \"inn\""));
        assert!(authorization.contains("public_threat_referrals"));
        assert!(authorization.contains("membership_is_current"));
        assert!(!authorization.contains("organization_presentation"));
        let disclosure = source
            .split("fn surface_public_threat")
            .nth(1)
            .and_then(|tail| tail.split("pub fn surface_problem").next())
            .expect("shared public disclosure");
        for canonical in [
            "threat_type",
            "exact_site_id",
            "approximate_count",
            "disclose_exact_case_site",
        ] {
            assert!(disclosure.contains(canonical), "{canonical}");
        }
        for secret in ["preparation_advice", "witness_account", "factor_trace"] {
            assert!(!disclosure.contains(secret), "{secret}");
        }
    }

    #[test]
    fn recurring_hostiles_are_unbounded_but_transaction_catchup_is_bounded() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        let incidents = source
            .split("pub(crate) fn ensure_generated_incidents")
            .nth(1)
            .and_then(|tail| tail.split("pub fn ensure_settlement_problems").next())
            .expect("incident materializer");
        assert!(incidents.contains("u16::MAX"));
        assert!(incidents.contains("saturating_add(16)"));
        assert!(incidents.contains("awareness_for_incident"));
        assert!(incidents.contains("combat_for_incident"));
    }

    #[test]
    fn self_referral_serializes_an_inline_testimony_topic() {
        let json =
            referral_fragments_json(adventuresim_core::local_problem::ReferralPresentation {
                lead: "I am the witness. Ask me about ".into(),
                topic: Some(("what I saw".into(), "referred-testimony".into())),
                trailing: ".".into(),
            })
            .unwrap();
        let fragments: Vec<adventuresim_dialogue::Fragment> = serde_json::from_str(&json).unwrap();
        assert_eq!(
            fragments,
            vec![
                adventuresim_dialogue::Fragment::Text {
                    value: "I am the witness. Ask me about ".into()
                },
                adventuresim_dialogue::Fragment::Topic {
                    topic: "referred-testimony".into(),
                    label: "what I saw".into()
                },
                adventuresim_dialogue::Fragment::Text { value: ".".into() }
            ]
        );
    }

    #[test]
    fn deterministic_rumor_selection_skips_unbacked_or_invalid_candidates() {
        let candidates = [
            ("valid-d", true),
            ("unbacked-a", false),
            ("valid-b", true),
            ("invalid-c", false),
            ("valid-a", true),
        ];
        assert_eq!(
            stable_eligible_candidates(
                candidates,
                2,
                |(_, eligible)| *eligible,
                |(id, _)| id.to_string(),
            ),
            vec![("valid-a", true), ("valid-b", true)]
        );
        assert!(
            stable_eligible_candidates(
                [("unbacked", false), ("ambiguous", false)],
                2,
                |(_, eligible)| *eligible,
                |(id, _)| id.to_string(),
            )
            .is_empty()
        );
    }

    #[test]
    fn initial_rumor_uses_persistent_local_identity_and_authored_navigable_location() {
        assert!(rumor_contact_is_valid("ironforge", "ironforge", true));
        assert!(!rumor_contact_is_valid("lubeck", "ironforge", true));
        assert!(!rumor_contact_is_valid("ironforge", "ironforge", false));
    }

    #[test]
    fn discovery_uses_world_time_for_problem_windows_and_observer_time_for_records() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        let surface = source
            .split("pub fn surface_problem")
            .nth(1)
            .and_then(|tail| tail.split("fn referral_fragments_json").next())
            .expect("problem discovery implementation");
        assert!(surface.contains("let official_world_minute = official_minute(ctx);"));
        assert!(surface.contains("let observer_minute = ctx"));
        assert!(surface.contains("is_active(problem, official_world_minute)"));
        assert!(!surface.contains("is_active(problem, observer_minute)"));
        assert!(surface.contains("npc_is_present(ctx, &presence, observer_minute)"));
        let new_problem = source
            .split("fn surface_new_problem")
            .nth(1)
            .and_then(|tail| tail.split("fn rumor_contact_is_valid").next())
            .expect("new-problem disclosure helper");
        assert!(new_problem.contains("learned_at: observer_minute"));
        assert!(new_problem.contains("official_learned_at: official_world_minute"));
        assert!(surface.contains("recorded_at: observer_minute"));
    }

    #[test]
    fn preferred_demo_rumor_is_private_one_shot_and_preempts_existing_followups() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        assert!(source.contains("#[table(accessor = local_problem_rumor_preference)]"));
        assert!(!source.contains("#[table(accessor = local_problem_rumor_preference, public)]"));
        let surface = source
            .split("pub fn surface_problem")
            .nth(1)
            .and_then(|tail| tail.split("fn dialogue_capable_inn_contact").next())
            .expect("problem discovery implementation");
        let preference = surface
            .find("local_problem_rumor_preference()")
            .expect("preferred rumor lookup");
        let public_threat = surface
            .find("surface_public_threat(")
            .expect("public threat fallback");
        let existing_followups = surface
            .find("for problem in &active_problems")
            .expect("existing problem followups");
        assert!(preference < public_threat);
        assert!(preference < existing_followups);
        assert!(surface.contains("if can_discover_new"));
        assert!(surface.contains("if surfaced || preferred_problem.is_none() || already_known"));
        assert!(surface.contains(".delete(character_id);"));

        let bootstrap = crate::strategic::STRATEGIC_SOURCE;
        let demo = bootstrap
            .split("fn seed_outbreak_demo")
            .nth(1)
            .and_then(|tail| tail.split("fn materialize_generated_quest").next())
            .expect("outbreak demo materialization");
        assert!(demo.contains("materialize_preferred_generated_fixture("));
        let preferred = bootstrap
            .split("fn materialize_preferred_generated_fixture")
            .nth(1)
            .and_then(|tail| tail.split("fn preferred_fixture_seed").next())
            .expect("preferred generated fixture helper");
        assert_eq!(preferred.matches("prefer_next_rumor(").count(), 1);
        assert!(
            preferred.find("materialize_generated_quest(").unwrap()
                < preferred.find("prefer_next_rumor(").unwrap()
        );
    }

    #[test]
    fn development_discovery_uses_bootstrap_safe_rumor_transition() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        let discovery = source
            .split("pub(crate) fn discover_development_problem")
            .nth(1)
            .and_then(|tail| tail.split("pub fn surface_problem").next())
            .expect("development problem discovery implementation");
        assert!(discovery.contains("receive_local_problem_rumor_for_development_bootstrap"));
        assert!(!discovery.contains("crate::investigation::receive_local_problem_rumor("));
        assert!(discovery.contains("record_journal_notice"));

        let investigation = crate::investigation::INVESTIGATION_SOURCE;
        let external = investigation
            .split("pub fn receive_local_problem_rumor(")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn receive_local_problem_rumor_for_development_bootstrap")
                    .next()
            })
            .expect("external rumor reducer");
        assert!(external.contains("require_actor(ctx, character_id)?"));
        let bootstrap = investigation
            .split("pub(crate) fn receive_local_problem_rumor_for_development_bootstrap")
            .nth(1)
            .and_then(|tail| tail.split("fn receive_local_problem_rumor_impl").next())
            .expect("bootstrap rumor transition");
        assert!(bootstrap.contains("development_capability_enabled()"));
        assert!(!bootstrap.contains("require_strategic_gateway"));
    }

    #[test]
    fn overview_fallback_counts_only_present_dialogue_capable_inn_contacts() {
        assert!(dialogue_capable_inn_contact(
            "lubeck", "lubeck", "inn", true, true,
        ));
        assert!(!dialogue_capable_inn_contact(
            "lubeck", "lubeck", "overview", true, true,
        ));
        assert!(!dialogue_capable_inn_contact(
            "lubeck", "lubeck", "inn", true, false,
        ));
        assert!(!dialogue_capable_inn_contact(
            "lubeck", "lubeck", "inn", false, true,
        ));
        assert!(!dialogue_capable_inn_contact(
            "hamburg", "lubeck", "inn", true, true,
        ));
    }

    #[test]
    fn rumor_consumers_use_validated_generation_provenance_before_disclosure() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        let authority = source
            .split("fn validated_problem_generation")
            .nth(1)
            .and_then(|tail| tail.split("fn referral_location_label").next())
            .unwrap();
        let referral = source
            .split("fn referral_location_label")
            .nth(1)
            .and_then(|tail| tail.split("fn stable_eligible_candidates").next())
            .unwrap();
        let surface = source
            .split("pub fn surface_problem")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .unwrap();
        let new_problem = source
            .split("fn surface_new_problem")
            .nth(1)
            .and_then(|tail| tail.split("pub fn surface_problem").next())
            .unwrap();
        assert!(authority.contains("validate_quest_generation_authority"));
        assert!(authority.contains("candidates.len() != 1"));
        assert!(authority.contains(".case_id()"));
        assert!(authority.contains(".public_case_id()"));
        assert!(referral.contains("validated_problem_generation"));
        assert!(surface.contains("validated_problem_generation"));
        assert!(!referral.contains("serde_json::from_str"));
        assert!(!surface.contains("serde_json::from_str"));
        for binding in [
            "problem.opaque_case_ref != receipt.opaque_case_ref",
            "problem.id != receipt.problem_id",
            "witness.resident_character_id == receipt.contact_resident_character_id",
            "witness.expected_location == receipt.expected_location_id",
        ] {
            assert!(referral.contains(binding), "{binding}");
        }
        for binding in [
            "manifest.canonical_case_id != problem.opaque_case_ref",
            "manifest.problem_id != problem.id",
            "context.settlement_id != settlement_id",
            "context.settlement_name != settlement.name",
        ] {
            assert!(authority.contains(binding), "{binding}");
        }
        assert!(new_problem.contains("contact.home_settlement_id"));
        assert!(new_problem.contains("npc_location_is_navigable"));
        assert!(new_problem.contains("expected_location_id: witness.expected_location.clone()"));
        assert!(!new_problem.contains("settlement_resident_presence()"));
        assert!(surface.contains("stable_eligible_candidates"));
        let selector = source
            .split("fn stable_eligible_candidates")
            .nth(1)
            .and_then(|tail| tail.split("pub fn surface_problem").next())
            .unwrap();
        assert!(
            selector.find(".filter(").unwrap() < selector.find(".sort_by_key(").unwrap()
                && selector.find(".sort_by_key(").unwrap() < selector.find(".truncate(").unwrap()
        );
        assert!(surface.matches("continue;").count() >= 6);
        assert!(new_problem.matches("return Ok(false);").count() >= 7);
        assert!(!surface.contains("case:opaque:"));
        assert!(!surface.contains("Manual problem"));
    }

    #[test]
    fn public_schema_has_no_hidden_fields() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        let public = source
            .split("pub struct LocalProblemSymptom")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap();
        for forbidden in [
            "cause",
            "threat",
            "disease_id",
            "destination",
            "case_ref",
            "weight",
            "bridge",
            "json",
        ] {
            assert!(!public.contains(forbidden), "{forbidden} leaked");
        }
        assert!(!source.contains("#[reducer]\npub fn apply_outcome"));
    }
    #[test]
    fn public_handles_are_gateway_filtered_and_dialogue_delivery_is_private() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        assert!(!source.contains("accessor = local_problem_consequence, public"));
        assert!(source.contains("backend_local_problem_trade_effects"));
        assert!(source.contains("backend_local_problem_rumors"));
        assert!(source.matches("if !is_gateway(ctx)").count() >= 2);
        assert!(source.contains("#[table(accessor=local_problem_rumor_delivery)]"));
        let strategic = crate::strategic::STRATEGIC_SOURCE;
        let start = strategic
            .split("pub fn start_dialogue")
            .nth(1)
            .unwrap()
            .split("pub fn join_dialogue_session")
            .next()
            .unwrap();
        assert!(!start.contains("local-problem-rumor"));
        assert!(!start.contains("fragments_json: serde_json::to_string(&fragments)"));
        assert!(
            start.find("local_problem_rumor_delivery()").unwrap()
                < start.find("receive_local_problem_rumor").unwrap()
        );
        let referral_event = start.find("response_id: \"generated-referral\"").unwrap();
        assert!(start.find("surface_problem(").unwrap() < referral_event);
        assert!(referral_event < start.find("receive_local_problem_rumor").unwrap());
    }
    #[test]
    fn authoritative_purchase_seams_apply_problem_price_after_base_quote() {
        let disease = crate::production_source(include_str!("disease.rs"));
        let purchase = disease
            .split("pub fn purchase_from_herbalist")
            .nth(1)
            .unwrap()
            .split("fn advance_medical_participants")
            .next()
            .unwrap();
        assert!(purchase.contains("character_time()"));
        assert!(purchase.contains("settlement_effects"));
        assert!(purchase.contains("local_problem::adjust_price(base, problem_effects.buy_bps)"));
        // This module intentionally keeps a few source-boundary tests ahead of
        // its implementation, so select the final (production) definition.
        let trade = include_str!("strategic/inventory_trade.rs")
            .rsplit("fn finalize_storefront_trade_impl")
            .next()
            .unwrap();
        assert!(trade.contains("character_time()"));
        assert!(trade.matches("local_problem::adjust_price").count() >= 3);
    }
    #[test]
    fn discovery_and_outcome_boundaries_are_bounded() {
        let source = crate::production_source(include_str!("local_problem.rs"));
        assert!(source.contains("has_service(adventuresim_world_schema::SettlementService::Inn)"));
        assert!(source.contains("stable_eligible_candidates"));
        assert!(source.contains("eligible_candidates.truncate(limit)"));
        let discovery = source.split("pub fn surface_problem").nth(1).unwrap();
        assert!(!discovery.contains("local_problem_receipt()\n        .character_id()"));
        assert!(source.contains("Conflicting retry for source outcome ID"));
        assert!(source.contains("input.at_minute != official_minute(ctx)"));
    }

    #[test]
    fn generated_referrals_bind_persistent_npcs_without_revealing_testimony() {
        let local = crate::production_source(include_str!("local_problem.rs"));
        let surface = local
            .split("fn surface_new_problem")
            .nth(1)
            .and_then(|tail| tail.split("fn rumor_contact_is_valid").next())
            .expect("new-problem disclosure helper");
        assert!(surface.contains("validated.manifest.witnesses.first()"));
        assert!(surface.contains("resolve_settlement_resident("));
        assert!(surface.contains("contact.home_settlement_id"));
        assert!(surface.contains("npc_location_is_navigable"));
        assert!(surface.contains("expected_location_id: witness.expected_location.clone()"));
        assert!(surface.contains("contact_resident_character_id: contact.character_id"));
        assert!(surface.contains("discovery_session_id: session_id.into()"));

        let strategic = crate::strategic::STRATEGIC_SOURCE;
        let start = strategic
            .split("pub fn start_dialogue")
            .nth(1)
            .and_then(|tail| tail.split("pub fn join_dialogue_session").next())
            .unwrap();
        assert!(!start.contains("persist_generated_testimony("));
        let receive = strategic
            .split("fn receive_referred_testimony")
            .nth(1)
            .and_then(|tail| tail.split("fn resolve_dialogue_fragments").next())
            .unwrap();
        assert!(receive.contains("referred_generated_witness"));
        assert!(receive.contains("&receipt.opaque_case_ref"));
        assert!(receive.contains("live_npc.character_id"));
        assert!(receive.contains("&session.settlement_id"));
        assert!(receive.contains("&session.location_id"));
        assert!(!receive.contains("manifest.witnesses"));
        assert!(receive.contains("persist_generated_testimony("));
        assert!(!start.contains("accept_contract("));
    }
}
