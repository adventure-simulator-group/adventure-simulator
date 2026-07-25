//! Private local-problem authority and safe discovery/consequence projections.
use crate::{
    character::{character, character__view},
    investigation::{
        EvidencePresentationKind, InvestigationEventAuthority, InvestigationEvidenceAuthority,
        InvestigationLead, investigation_event_authority, investigation_evidence_authority,
        investigation_lead,
    },
    settlement_population::{settlement_npc, settlement_npc_presence},
    strategic::{
        ValidatedQuestGenerationAuthority, quest_generation_authority, settlement,
        strategic_gateway_authority__view, validate_quest_generation_authority,
    },
    time::{character_time, character_time__view, world_clock},
};
use adventuresim_core::local_problem as lp;
use serde::{Deserialize, Serialize};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
#[table(accessor = local_problem_authority)]
pub struct LocalProblemAuthority {
    #[primary_key]
    pub id: String,
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
    pub encounter_archetype: String,
    pub disease_intensity: u16,
    pub disease_id: String,
    pub starts_at: u64,
    pub ends_at: u64,
    pub mitigation_bps: u16,
    /// Includes the original offence represented by the generated case.
    pub incident_count: u16,
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
    pub witness_npc_id: String,
    pub victim_npc_id: String,
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
    pub source_npc_id: String,
    pub discovery_session_id: String,
    pub contact_npc_id: String,
    pub expected_location_id: String,
    pub safe_summary: String,
    pub learned_at: u64,
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
fn archetype_name(value: Option<lp::EncounterArchetype>) -> &'static str {
    match value {
        Some(lp::EncounterArchetype::Bandits) => "bandits",
        Some(lp::EncounterArchetype::Goblins) => "goblins",
        Some(lp::EncounterArchetype::Undead) => "undead",
        None => "",
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

fn official_minute(ctx: &ReducerContext) -> u64 {
    ctx.db
        .world_clock()
        .id()
        .find(0)
        .map_or(0, |r| r.official_minutes)
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
    let ends_at = starts_at.saturating_add(30 * 1_440);
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
            scope_key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode generated problem scope")?,
            consequence_mechanism: mechanism.into(),
            symptom: symptom_name(consequence.symptom).into(),
            buy_bps: consequence.effects.buy_bps,
            sell_penalty_bps: consequence.effects.sell_penalty_bps,
            encounter_frequency_bps: consequence.effects.encounter_frequency_bps,
            encounter_archetype: archetype_name(consequence.effects.encounter_archetype).into(),
            disease_intensity: consequence.effects.disease_intensity,
            disease_id: if consequence.effects.disease_intensity > 0 {
                "influenza".into()
            } else {
                String::new()
            },
            starts_at,
            ends_at,
            mitigation_bps: 0,
            incident_count: 1,
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
    let scope = format!("settlement:{settlement_id}");
    let problems: Vec<_> = ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&scope)
        .filter(|problem| is_active(problem, minute))
        .collect();
    for mut problem in problems {
        let Some(validated) = validated_problem_generation(ctx, &problem, settlement_id) else {
            continue;
        };
        let due = lp::due_incident_count(problem.starts_at, minute);
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
            let occurred_at = problem.starts_at.saturating_add(
                u64::from(ordinal.saturating_sub(1)).saturating_mul(lp::INCIDENT_INTERVAL_MINUTES),
            );
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
                witness_npc_id: witness.npc_id.clone(),
                victim_npc_id: victim.npc_id.clone(),
                circumstance: format!("{circumstance:?}").to_ascii_lowercase(),
                site_id: site.id.0.clone(),
                evidence_id: evidence_id.clone(),
                evidence_kind: format!("{evidence_kind:?}").to_ascii_lowercase(),
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
                        "subject": victim.npc_id,
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
    let cycle = minute / (30 * 1_440);
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
            scope_key: key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode problem scope")?,
            consequence_mechanism: route_mechanism(problem.symptom).into(),
            symptom: symptom_name(problem.symptom).into(),
            buy_bps: problem.effects.buy_bps,
            sell_penalty_bps: problem.effects.sell_penalty_bps,
            encounter_frequency_bps: problem.effects.encounter_frequency_bps,
            encounter_archetype: archetype_name(problem.effects.encounter_archetype).into(),
            disease_intensity: problem.effects.disease_intensity,
            disease_id: disease_id.into(),
            starts_at: problem.starts_at,
            ends_at: problem.ends_at,
            mitigation_bps: 0,
            incident_count: 1,
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
    let cycle = minute / (30 * 1_440);
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
            scope_key: key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode route scope")?,
            consequence_mechanism: route_mechanism(problem.symptom).into(),
            symptom: symptom_name(problem.symptom).into(),
            buy_bps: 0,
            sell_penalty_bps: 0,
            encounter_frequency_bps: problem.effects.encounter_frequency_bps,
            encounter_archetype: archetype_name(problem.effects.encounter_archetype).into(),
            disease_intensity: 0,
            disease_id: String::new(),
            starts_at: problem.starts_at,
            ends_at: problem.ends_at,
            mitigation_bps: 0,
            incident_count: 1,
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
        && minute < row.ends_at
        && row.resolved_at.is_none_or(|at| minute < at)
        && row.mitigation_bps < 10_000
}
fn scaled(value: i32, mitigation: u16) -> i32 {
    (i64::from(value) * i64::from(10_000u16.saturating_sub(mitigation.min(10_000))) / 10_000) as i32
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
        .filter(|r| r.encounter_frequency_bps > 0)
        .find_map(|r| match r.encounter_archetype.as_str() {
            "bandits" => Some(adventuresim_core::encounter::EncounterArchetype::Bandits),
            "goblins" => Some(adventuresim_core::encounter::EncounterArchetype::Goblins),
            "undead" => Some(adventuresim_core::encounter::EncounterArchetype::Undead),
            _ => None,
        });
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
#[allow(dead_code, reason = "typed internal boundary consumed by issue #186")]
pub(crate) fn apply_outcome(
    ctx: &ReducerContext,
    problem_id: &str,
    input: &LocalProblemOutcomeInput,
) -> Result<(), String> {
    if input.source_outcome_id.is_empty()
        || input.source_outcome_id.len() > 160
        || input.mitigation_bps > 10_000
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
        .find(&problem_id.to_owned())
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
    if (input.resolve || problem.mitigation_bps == 10_000)
        && let Some(mut symptom) = ctx
            .db
            .local_problem_symptom()
            .problem_id()
            .find(&problem_id.to_owned())
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
    let settlement = ctx.db.settlement().id().find(&settlement_id.to_string())?;
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
            witness.npc_id == receipt.contact_npc_id
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

/// Surface at most one unknown active problem. Inns are preferred by callers;
/// overview dialogue is the fallback. The return is safe authored text only.
pub fn surface_problem(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: &str,
    source_npc_id: &str,
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
    let source_npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&source_npc_id.to_owned())
        .filter(|npc| npc.home_settlement_id == settlement_id);
    let minute = ctx
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
            .filter(|problem| is_active(problem, minute)),
        lp::MAX_ACTIVE_PER_SCOPE,
        |problem| validated_problem_generation(ctx, problem, &settlement_id).is_some(),
        |problem| (problem.id.clone(), problem.opaque_case_ref.clone()),
    );
    for problem in &active_problems {
        let Some(receipt) = ctx
            .db
            .local_problem_receipt()
            .id()
            .find(&format!("{character_id}:{}", problem.id))
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
                    .find(&format!("{character_id}:{}", incident.id))
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
                    learned_at: minute,
                });
            ctx.db.investigation_lead().insert(InvestigationLead {
                id: format!("lead:{incident_receipt_id}"),
                owner_character_id: character_id,
                case_id: validated.manifest.public_case_id,
                proposition_id: incident.proposition_id.clone(),
                summary: incident.public_summary.clone(),
                source_label: "local report".into(),
                confidence_bps: 5_000,
                destination_stage: "unknown".into(),
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
                recorded_at: minute,
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
        let Some(contact) = ctx.db.settlement_npc().id().find(&receipt.contact_npc_id) else {
            continue;
        };
        let Some(location_label) = referral_location_label(ctx, problem, &receipt) else {
            continue;
        };
        let present = ctx
            .db
            .settlement_npc_presence()
            .npc_id()
            .find(&receipt.contact_npc_id)
            .is_some_and(|presence| {
                presence.settlement_id == settlement_id
                    && presence.location_id == receipt.expected_location_id
            });
        if !present {
            continue;
        }
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
                    source_npc
                        .as_ref()
                        .map(|npc| (npc.id.as_str(), npc.name.as_str())),
                    &contact.id,
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
            .settlement_npc_presence()
            .settlement_id()
            .filter(&settlement_id)
            .any(|p| {
                p.location_id == "inn" && crate::settlement_population::npc_is_present(&p, minute)
            });
    if lp::discovery_action(location_id, inn_available, false) != lp::DiscoveryAction::NewRumor {
        return Ok(());
    }
    for problem in active_problems {
        if ctx
            .db
            .local_problem_receipt()
            .id()
            .find(&format!("{character_id}:{}", problem.id))
            .is_some()
        {
            continue;
        }
        let Some(validated) = validated_problem_generation(ctx, &problem, &settlement_id) else {
            continue;
        };
        let Some(witness) = validated.manifest.witnesses.first() else {
            continue;
        };
        let Some(contact) = ctx.db.settlement_npc().id().find(&witness.npc_id) else {
            continue;
        };
        let Some(presence) = ctx
            .db
            .settlement_npc_presence()
            .npc_id()
            .find(&witness.npc_id)
            .filter(|presence| {
                presence.settlement_id == settlement_id
                    && presence.location_id == witness.expected_location
            })
        else {
            continue;
        };
        let Some(symptom) = ctx
            .db
            .local_problem_symptom()
            .problem_id()
            .find(&problem.id)
        else {
            continue;
        };
        let location_label =
            adventuresim_core::quest_generation::referral_display_location(witness);
        if location_label.is_empty() {
            continue;
        }
        let presentation = lp::referral_presentation(
            &symptom.public_summary,
            source_npc
                .as_ref()
                .map(|npc| (npc.id.as_str(), npc.name.as_str())),
            &contact.id,
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
            settlement_id: settlement_id.clone(),
            problem_id: problem.id,
            opaque_case_ref: problem.opaque_case_ref,
            source_npc_id: source_npc_id.into(),
            discovery_session_id: session_id.into(),
            contact_npc_id: contact.id,
            expected_location_id: presence.location_id,
            safe_summary: symptom.public_summary,
            learned_at: minute,
        });
        ctx.db
            .local_problem_rumor_delivery()
            .insert(LocalProblemRumorDelivery {
                id: format!("{session_id}:rumor"),
                character_id,
                settlement_id,
                session_id: session_id.into(),
                receipt_id,
                fragments_json: referral_fragments_json(presentation)?,
            });
        return Ok(());
    }
    Ok(())
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
    use crate::local_problem::{referral_fragments_json, stable_eligible_candidates};

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
    fn rumor_consumers_use_validated_generation_provenance_before_disclosure() {
        let source = include_str!("local_problem.rs");
        let authority = source
            .split("fn validated_problem_generation")
            .nth(1)
            .and_then(|tail| tail.split("fn referral_location_label").next())
            .unwrap();
        let referral = source
            .split("fn referral_location_label")
            .nth(1)
            .and_then(|tail| tail.split("pub fn surface_problem").next())
            .unwrap();
        let surface = source
            .split("pub fn surface_problem")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
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
            "witness.npc_id == receipt.contact_npc_id",
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
        assert!(surface.contains("presence.location_id == witness.expected_location"));
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
        assert!(surface.matches("continue;").count() >= 8);
        assert!(!surface.contains("case:opaque:"));
        assert!(!surface.contains("Manual problem"));
    }

    #[test]
    fn public_schema_has_no_hidden_fields() {
        let source = include_str!("local_problem.rs");
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
        let source = include_str!("local_problem.rs");
        assert!(!source.contains("accessor = local_problem_consequence, public"));
        assert!(source.contains("backend_local_problem_trade_effects"));
        assert!(source.contains("backend_local_problem_rumors"));
        assert!(source.matches("if !is_gateway(ctx)").count() >= 2);
        assert!(source.contains("#[table(accessor = local_problem_rumor_delivery)]"));
        let strategic = include_str!("strategic.rs");
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
    }
    #[test]
    fn authoritative_purchase_seams_apply_problem_price_after_base_quote() {
        let disease = include_str!("disease.rs");
        let purchase = disease
            .split("pub fn purchase_from_herbalist")
            .nth(1)
            .unwrap()
            .split("fn advance_medical_participants")
            .next()
            .unwrap();
        assert!(purchase.contains("character_time()"));
        assert!(purchase.contains("settlement_effects"));
        assert!(purchase.contains("adjust_price(base_price, problem_effects.buy_bps)"));
        let strategic = include_str!("strategic.rs");
        let trade = strategic
            .split("pub fn finalize_merchant_trade")
            .nth(1)
            .unwrap()
            .split("pub fn ")
            .next()
            .unwrap();
        assert!(trade.contains("character_time()"));
        assert!(trade.matches("local_problem::adjust_price").count() >= 3);
    }
    #[test]
    fn discovery_and_outcome_boundaries_are_bounded() {
        let source = include_str!("local_problem.rs");
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
        let local = include_str!("local_problem.rs");
        let surface = local.split("pub fn surface_problem").nth(1).unwrap();
        assert!(surface.contains("validated.manifest.witnesses.first()"));
        assert!(surface.contains("settlement_npc().id().find(&witness.npc_id)"));
        assert!(surface.contains("presence.location_id == witness.expected_location"));
        assert!(surface.contains("contact_npc_id: contact.id"));
        assert!(surface.contains("discovery_session_id: session_id.into()"));

        let strategic = include_str!("strategic.rs");
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
        assert!(receive.contains("&live_npc.id"));
        assert!(receive.contains("&session.settlement_id"));
        assert!(receive.contains("&session.location_id"));
        assert!(!receive.contains("manifest.witnesses"));
        assert!(receive.contains("persist_generated_testimony("));
        assert!(!start.contains("accept_contract("));
    }
}
