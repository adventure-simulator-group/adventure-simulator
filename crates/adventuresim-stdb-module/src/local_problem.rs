//! Private local-problem authority and safe discovery/consequence projections.
use crate::{
    character::character,
    settlement_population::{settlement_npc, settlement_npc_presence},
    time::character_time,
};
use adventuresim_core::local_problem as lp;
use serde::{Deserialize, Serialize};
use spacetimedb::{ReducerContext, Table, table};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
#[table(accessor = local_problem_authority)]
pub struct LocalProblemAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub scope_key: String,
    pub scope_json: String,
    pub cause: String,
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
    pub resolved_at: Option<u64>,
    pub opaque_case_ref: String,
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

/// Bounded, non-causal explanation for consequences visible in the UI.
#[derive(Clone, Debug)]
#[table(accessor = local_problem_consequence, public)]
pub struct LocalProblemConsequence {
    #[primary_key]
    pub problem_id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
    pub encounter_frequency_bps: u16,
    pub disease_exposure_intensity: u16,
    pub starts_at: u64,
    pub ends_at: u64,
    pub mitigation_bps: u16,
    pub resolved_at: Option<u64>,
}

/// Private, source-attributed knowledge receipt and the narrow #183 seam.
#[derive(Clone, Debug)]
#[table(accessor = local_problem_receipt)]
pub struct LocalProblemReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub problem_id: String,
    pub opaque_case_ref: String,
    pub source_npc_id: String,
    pub contact_npc_id: String,
    pub expected_location_id: String,
    pub safe_summary: String,
    pub learned_at: u64,
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
fn cause_name(value: lp::Cause) -> &'static str {
    match value {
        lp::Cause::Bandits => "bandits",
        lp::Cause::Goblins => "goblins",
        lp::Cause::Ghouls => "ghouls",
        lp::Cause::ContaminatedWell => "contaminated_well",
        lp::Cause::Smugglers => "smugglers",
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

pub fn ensure_settlement_problems(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let scope = lp::Scope::Settlement {
        settlement_id: settlement_id.into(),
    };
    let key = scope_key(&scope);
    if ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&key)
        .next()
        .is_some()
    {
        return Ok(());
    }
    let context = lp::GenerationContext {
        seed: format!("local-problems:{settlement_id}"),
        scope: scope.clone(),
        allowed_bridges: BTreeSet::from(["secret_riverside_meeting".into()]),
    };
    let (problem, explanation) = lp::generate(&context, 0, 0)?;
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
            cause: cause_name(problem.cause).into(),
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
    ctx.db
        .local_problem_consequence()
        .insert(LocalProblemConsequence {
            problem_id: problem.id.0.clone(),
            settlement_id: settlement_id.into(),
            buy_bps: problem.effects.buy_bps,
            sell_penalty_bps: problem.effects.sell_penalty_bps,
            encounter_frequency_bps: problem.effects.encounter_frequency_bps,
            disease_exposure_intensity: problem.effects.disease_intensity,
            starts_at: problem.starts_at,
            ends_at: problem.ends_at,
            mitigation_bps: 0,
            resolved_at: None,
        });
    Ok(())
}

pub fn ensure_route_problem(ctx: &ReducerContext, left: &str, right: &str) -> Result<(), String> {
    let scope = lp::Scope::route(left, right);
    let key = scope_key(&scope);
    if ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&key)
        .next()
        .is_some()
    {
        return Ok(());
    }
    let context = lp::GenerationContext {
        seed: format!("local-problems:{key}"),
        scope: scope.clone(),
        allowed_bridges: BTreeSet::new(),
    };
    let (problem, explanation) = lp::generate(&context, 0, 0)?;
    ctx.db
        .local_problem_authority()
        .insert(LocalProblemAuthority {
            id: problem.id.0.clone(),
            scope_key: key,
            scope_json: serde_json::to_string(&scope)
                .map_err(|_| "Could not encode route scope")?,
            cause: cause_name(problem.cause).into(),
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
#[allow(dead_code, reason = "typed internal boundary consumed by issue #186")]
pub(crate) fn apply_outcome(
    ctx: &ReducerContext,
    problem_id: &str,
    source_outcome_id: &str,
    minute: u64,
    mitigation_bps: u16,
    resolve: bool,
) -> Result<(), String> {
    if source_outcome_id.is_empty() || source_outcome_id.len() > 160 {
        return Err("Invalid source outcome ID".into());
    }
    let receipt_id = format!("{problem_id}:{source_outcome_id}");
    if ctx
        .db
        .local_problem_outcome_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Ok(());
    }
    let mut problem = ctx
        .db
        .local_problem_authority()
        .id()
        .find(&problem_id.to_owned())
        .ok_or("Local problem not found")?;
    problem.mitigation_bps = problem.mitigation_bps.max(mitigation_bps.min(10_000));
    if resolve {
        problem.resolved_at = Some(problem.resolved_at.map_or(minute, |old| old.min(minute)));
    }
    ctx.db
        .local_problem_authority()
        .id()
        .update(problem.clone());
    if let Some(mut public) = ctx
        .db
        .local_problem_consequence()
        .problem_id()
        .find(&problem_id.to_owned())
    {
        public.mitigation_bps = problem.mitigation_bps;
        public.resolved_at = problem.resolved_at;
        ctx.db
            .local_problem_consequence()
            .problem_id()
            .update(public);
    }
    ctx.db
        .local_problem_outcome_receipt()
        .insert(LocalProblemOutcomeReceipt {
            id: receipt_id,
            problem_id: problem_id.into(),
            source_outcome_id: source_outcome_id.into(),
            applied_at: minute,
            mitigation_bps: mitigation_bps.min(10_000),
            resolved: resolve,
        });
    Ok(())
}

/// Surface at most one unknown active problem. Inns are preferred by callers;
/// overview dialogue is the fallback. The return is safe authored text only.
pub fn surface_problem(
    ctx: &ReducerContext,
    character_id: u64,
    source_npc_id: &str,
    location_id: &str,
) -> Result<Option<String>, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Problem discovery requires a settlement")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let mut known: Vec<_> = ctx
        .db
        .local_problem_receipt()
        .character_id()
        .filter(character_id)
        .filter_map(|receipt| {
            ctx.db
                .local_problem_authority()
                .id()
                .find(&receipt.problem_id)
                .filter(|p| is_active(p, minute))
                .map(|_| receipt)
        })
        .collect();
    known.sort_by(|a, b| a.problem_id.cmp(&b.problem_id));
    if let Some(receipt) = known.into_iter().next() {
        if let Some(contact) = ctx.db.settlement_npc().id().find(&receipt.contact_npc_id) {
            return Ok(Some(format!(
                "{} {}—the {}—is usually at the {} and can tell you more.",
                receipt.safe_summary,
                contact.name,
                contact.profession,
                receipt.expected_location_id
            )));
        }
    }
    let inn_available = ctx
        .db
        .settlement_npc_presence()
        .settlement_id()
        .filter(&settlement_id)
        .any(|p| {
            p.location_id == "inn" && crate::settlement_population::npc_is_present(&p, minute)
        });
    if lp::discovery_action(location_id, inn_available, false) != lp::DiscoveryAction::NewRumor {
        return Ok(None);
    }
    let mut rows: Vec<_> = ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&format!("settlement:{settlement_id}"))
        .filter(|p| is_active(p, minute))
        .filter(|p| {
            ctx.db
                .local_problem_receipt()
                .character_id()
                .filter(character_id)
                .all(|r| r.problem_id != p.id)
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    let Some(problem) = rows.into_iter().next() else {
        return Ok(None);
    };
    let mut contacts: Vec<_> = ctx
        .db
        .settlement_npc_presence()
        .settlement_id()
        .filter(&settlement_id)
        .filter(|p| p.location_id != "inn")
        .filter_map(|p| ctx.db.settlement_npc().id().find(&p.npc_id).map(|n| (p, n)))
        .collect();
    contacts.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    let (presence, contact) = contacts
        .into_iter()
        .next()
        .ok_or("Local problem has no reliable referral contact")?;
    let symptom = ctx
        .db
        .local_problem_symptom()
        .problem_id()
        .find(&problem.id)
        .ok_or("Problem symptom projection missing")?;
    let description = format!(
        "{}, {}, with {} hair",
        contact.height, contact.build, contact.hair
    );
    let text = format!(
        "{} Ask {}—the {}, {}, usually found at the {}.",
        symptom.public_summary, contact.name, contact.profession, description, presence.location_id
    );
    ctx.db.local_problem_receipt().insert(LocalProblemReceipt {
        id: format!("{character_id}:{}", problem.id),
        character_id,
        problem_id: problem.id,
        opaque_case_ref: problem.opaque_case_ref,
        source_npc_id: source_npc_id.into(),
        contact_npc_id: contact.id,
        expected_location_id: presence.location_id,
        safe_summary: symptom.public_summary,
        learned_at: minute,
    });
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
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
}
