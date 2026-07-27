//! Deterministic strategic decisions for NPC adventuring companies.
//!
//! SpacetimeDB owns scheduling and persistence. This module owns the pure,
//! replayable eligibility, strategy, party-selection, and outcome rules so the
//! server and evaluator cannot quietly diverge.

use serde::{Deserialize, Serialize};

use crate::{
    investigation_action::{
        InvestigationActionKind, ResolutionInput, SkillContribution, TimeOfDay, WeatherAuthority,
        resolve,
    },
    quest_generation::{FinaleKind, GeneratedCase, RouteClass},
};

pub const MIN_INTERVENTION_AGE_MINUTES: u64 = 5 * 1_440;
pub const PLAYER_ACTIVITY_GRACE_MINUTES: u64 = 2 * 1_440;
pub const RETRY_DELAY_MINUTES: u64 = 3 * 1_440;
pub const MIN_INTERVENTION_INCIDENTS: u16 = 2;
pub const MAX_CAPABILITY: u16 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcCaseSnapshot {
    pub case_id: String,
    pub problem_id: String,
    pub settlement_id: String,
    pub opened_at: u64,
    pub incident_count: u16,
    pub mitigation_bps: u16,
    pub open: bool,
    pub player_activity_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcPartySnapshot {
    pub party_id: String,
    pub name: String,
    pub settlement_id: String,
    pub capability: u16,
    pub available_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcInterventionStrategy {
    InvestigateCarefully,
    ProtectLocals,
    ConfrontDirectly,
    Defer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NpcInterventionOutcome {
    Resolved,
    Mitigated,
    Failed,
    Delayed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcInterventionDecision {
    pub strategy: NpcInterventionStrategy,
    pub outcome: NpcInterventionOutcome,
    pub mitigation_bps: u16,
    pub next_available_at: u64,
    pub roll_bps: u16,
    pub safe_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcInvestigationApproach {
    pub route: RouteClass,
    pub route_label: String,
    pub lead_source: String,
    pub lead_quote: String,
    pub preparation_summary: String,
    pub step_summaries: Vec<String>,
    pub decisive_action: InvestigationActionKind,
    pub destination_label: String,
    pub target_terrain: crate::investigation_action::Terrain,
    pub finale_kind: FinaleKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcApproachResolution {
    pub succeeded: bool,
    pub effective_skill_bps: u16,
    pub risk_triggered: bool,
    pub failure_summary: Option<String>,
}

pub fn eligible_at(case: &NpcCaseSnapshot) -> u64 {
    let aged = case.opened_at.saturating_add(MIN_INTERVENTION_AGE_MINUTES);
    case.player_activity_at.map_or(aged, |at| {
        aged.max(at.saturating_add(PLAYER_ACTIVITY_GRACE_MINUTES))
    })
}

pub fn case_is_eligible(case: &NpcCaseSnapshot, now: u64) -> bool {
    case.open
        && case.incident_count >= MIN_INTERVENTION_INCIDENTS
        && case.mitigation_bps < 10_000
        && now >= eligible_at(case)
}

pub fn select_party<'a>(
    case: &NpcCaseSnapshot,
    now: u64,
    parties: impl IntoIterator<Item = &'a NpcPartySnapshot>,
) -> Option<&'a NpcPartySnapshot> {
    parties
        .into_iter()
        .filter(|party| {
            party.settlement_id == case.settlement_id
                && party.available_at <= now
                && party.capability <= MAX_CAPABILITY
        })
        .max_by_key(|party| {
            (
                party.capability,
                stable_hash(&format!("{}:{}", case.case_id, party.party_id)),
            )
        })
}

pub fn scripted_strategy(
    case: &NpcCaseSnapshot,
    party: &NpcPartySnapshot,
) -> NpcInterventionStrategy {
    if party.capability >= 70 {
        NpcInterventionStrategy::InvestigateCarefully
    } else if case.incident_count >= 4 {
        NpcInterventionStrategy::ProtectLocals
    } else {
        NpcInterventionStrategy::ConfrontDirectly
    }
}

pub fn decide(
    case: &NpcCaseSnapshot,
    party: &NpcPartySnapshot,
    strategy: NpcInterventionStrategy,
    attempt: u16,
    now: u64,
) -> NpcInterventionDecision {
    decide_after_supported_approach(case, party, strategy, attempt, now, None)
}

pub fn decide_after_supported_approach(
    case: &NpcCaseSnapshot,
    party: &NpcPartySnapshot,
    strategy: NpcInterventionStrategy,
    attempt: u16,
    now: u64,
    approach: Option<(
        &NpcInvestigationApproach,
        &NpcApproachResolution,
        Option<&str>,
    )>,
) -> NpcInterventionDecision {
    if let Some((plan, resolution, next_route)) = approach
        && !resolution.succeeded
    {
        let mut summary = format!(
            "{} tested {} but {}",
            party.name,
            plan.route_label,
            resolution
                .failure_summary
                .as_deref()
                .unwrap_or("the attempt produced no conclusive result.")
        );
        if let Some(next_route) = next_route {
            summary.push_str(&format!(
                " The case remains open; after regrouping, the company intends to try {next_route}."
            ));
        } else {
            summary.push_str(" The case remains open.");
        }
        return decision(
            strategy,
            NpcInterventionOutcome::Failed,
            case.mitigation_bps,
            now,
            stable_hash(&format!(
                "npc-investigation-route-failure:{}:{}:{attempt}:{:?}",
                case.case_id, party.party_id, plan.route
            )) as u16
                % 10_000,
            summary,
        );
    }
    let roll = (stable_hash(&format!(
        "npc-intervention-v1:{}:{}:{attempt}:{strategy:?}",
        case.case_id, party.party_id
    )) % 10_000) as u16;
    let capability = u32::from(party.capability.min(MAX_CAPABILITY));
    let incident_pressure = u32::from(case.incident_count.saturating_sub(1)).min(8) * 350;
    let strategy_bonus: i32 = match strategy {
        NpcInterventionStrategy::InvestigateCarefully => 1_100,
        NpcInterventionStrategy::ProtectLocals => 350,
        NpcInterventionStrategy::ConfrontDirectly => -250,
        NpcInterventionStrategy::Defer => {
            return decision(
                strategy,
                NpcInterventionOutcome::Delayed,
                case.mitigation_bps,
                now,
                roll,
                format!(
                    "{} postponed its investigation while other obligations took priority.",
                    party.name
                ),
            );
        }
    };
    let approach_bonus = approach.map_or(0, |(_, result, _)| {
        i32::from(result.effective_skill_bps) / 5
    });
    let resolution_threshold =
        (1_500i32 + (capability as i32 * 65) + strategy_bonus + approach_bonus
            - incident_pressure as i32)
            .clamp(500, 8_500) as u32;
    let mitigation_threshold = (resolution_threshold + 2_250).min(9_500);
    if u32::from(roll) < resolution_threshold {
        let safe_summary = approach.map_or_else(
            || {
                format!(
                    "{} investigated the local trouble and brought the incidents to an end.",
                    party.name
                )
            },
            |(plan, _, _)| {
                format!(
                    "{} followed {} to {} and {}",
                    party.name,
                    plan.route_label,
                    plan.destination_label,
                    resolved_finale_summary(plan.finale_kind)
                )
            },
        );
        decision(
            strategy,
            NpcInterventionOutcome::Resolved,
            10_000,
            now,
            roll,
            safe_summary,
        )
    } else if u32::from(roll) < mitigation_threshold {
        let mitigation = case
            .mitigation_bps
            .max((2_500 + capability * 50).min(8_000) as u16);
        let safe_summary = approach.map_or_else(
            || {
                format!(
                    "{} could not end the trouble, but its intervention reduced the harm to local people.",
                    party.name
                )
            },
            |(plan, _, next_route)| {
                let retry = next_route.map_or_else(
                    String::new,
                    |route| format!(" The company intends to investigate {route} next."),
                );
                format!(
                    "{} reached {} by following {}, but could not finish the case. Its intervention reduced the immediate harm to local people.{retry}",
                    party.name, plan.destination_label, plan.route_label
                )
            },
        );
        decision(
            strategy,
            NpcInterventionOutcome::Mitigated,
            mitigation,
            now,
            roll,
            safe_summary,
        )
    } else {
        let safe_summary = approach.map_or_else(
            || format!("{} returned without resolving the local trouble.", party.name),
            |(plan, _, next_route)| {
                let retry = next_route.map_or_else(
                    String::new,
                    |route| {
                        format!(
                            " The case remains open; after regrouping, the company intends to try {route}."
                        )
                    },
                );
                format!(
                    "{} followed {} to {}, but {}{retry}",
                    party.name,
                    plan.route_label,
                    plan.destination_label,
                    unresolved_finale_summary(plan.finale_kind)
                )
            },
        );
        decision(
            strategy,
            NpcInterventionOutcome::Failed,
            case.mitigation_bps,
            now,
            roll,
            safe_summary,
        )
    }
}

pub fn supported_investigation_approaches(
    generated: &GeneratedCase,
) -> Vec<NpcInvestigationApproach> {
    let Some(finale) = generated.finales.first() else {
        return Vec::new();
    };
    let destination_label = generated
        .sites
        .iter()
        .find(|site| site.id == finale.site_id)
        .map_or_else(
            || "the reported destination".to_owned(),
            |site| site.safe_label.clone(),
        );
    let target_terrain = generated
        .sites
        .iter()
        .find(|site| site.id == finale.site_id)
        .map_or(crate::investigation_action::Terrain::Settlement, |site| {
            site.terrain
        });
    let mut routes = generated
        .actions
        .iter()
        .map(|action| action.route)
        .collect::<Vec<_>>();
    let visible_contact_ids = crate::quest_generation::player_visible_testimony_sequence(generated)
        .into_iter()
        .map(|(witness, _)| witness.npc_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    routes.sort();
    routes.dedup();
    routes
        .into_iter()
        .filter_map(|route| {
            let terminal = generated.actions.iter().find(|candidate| {
                candidate.route == route
                    && !generated.actions.iter().any(|other| {
                        other.route == route && other.prerequisite.as_ref() == Some(&candidate.id)
                    })
            })?;
            let mut chain = Vec::new();
            let mut cursor = Some(terminal);
            while let Some(action) = cursor {
                chain.push(action);
                cursor = action.prerequisite.as_ref().and_then(|required| {
                    generated
                        .actions
                        .iter()
                        .find(|candidate| &candidate.id == required)
                });
            }
            chain.reverse();
            if chain.iter().any(|action| {
                action.target_kind == "contact"
                    && !visible_contact_ids.contains(action.target_id.as_str())
            }) {
                return None;
            }
            let (lead_source, lead_quote) = testimony_for_route(generated, route)
                .unwrap_or_else(|| ("the local accounts".into(), terminal.safe_summary.clone()));
            Some(NpcInvestigationApproach {
                route,
                route_label: route_label(route).into(),
                lead_source,
                lead_quote,
                preparation_summary: preparation_summary(route, terminal.kind, target_terrain),
                step_summaries: chain
                    .iter()
                    .map(|action| action.safe_summary.clone())
                    .collect(),
                decisive_action: terminal.kind,
                destination_label: destination_label.clone(),
                target_terrain,
                finale_kind: finale.kind,
            })
        })
        .collect()
}

pub fn select_investigation_approach(
    approaches: &[NpcInvestigationApproach],
    strategy: NpcInterventionStrategy,
    attempt: u16,
) -> Option<&NpcInvestigationApproach> {
    select_investigation_approach_after(approaches, strategy, attempt, None)
}

pub fn select_investigation_approach_after(
    approaches: &[NpcInvestigationApproach],
    strategy: NpcInterventionStrategy,
    attempt: u16,
    previous_route: Option<RouteClass>,
) -> Option<&NpcInvestigationApproach> {
    let preference = match strategy {
        NpcInterventionStrategy::InvestigateCarefully => [
            RouteClass::PhysicalTrail,
            RouteClass::SocialInquiry,
            RouteClass::PatternSurveillance,
        ],
        NpcInterventionStrategy::ProtectLocals => [
            RouteClass::PatternSurveillance,
            RouteClass::SocialInquiry,
            RouteClass::PhysicalTrail,
        ],
        NpcInterventionStrategy::ConfrontDirectly => [
            RouteClass::PhysicalTrail,
            RouteClass::PatternSurveillance,
            RouteClass::SocialInquiry,
        ],
        NpcInterventionStrategy::Defer => return None,
    };
    let mut ordered = preference
        .into_iter()
        .filter_map(|route| approaches.iter().find(|approach| approach.route == route))
        .collect::<Vec<_>>();
    if ordered.len() > 1
        && let Some(previous_route) = previous_route
    {
        ordered.retain(|approach| approach.route != previous_route);
    }
    if ordered.is_empty() {
        None
    } else {
        Some(ordered[usize::from(attempt.saturating_sub(1)) % ordered.len()])
    }
}

pub fn resolve_investigation_approach(
    case: &NpcCaseSnapshot,
    party: &NpcPartySnapshot,
    approach: &NpcInvestigationApproach,
    attempt: u16,
    now: u64,
) -> NpcApproachResolution {
    let capability_bps = party.capability.min(MAX_CAPABILITY).saturating_mul(100);
    let resolution = resolve(ResolutionInput {
        seed: stable_hash(&format!(
            "npc-supported-route-v1:{}:{}:{:?}",
            case.case_id, party.party_id, approach.route
        )),
        attempt_index: u32::from(attempt.saturating_sub(1)),
        kind: approach.decisive_action,
        terrain: approach.target_terrain,
        target_terrain: approach.target_terrain,
        time_of_day: if matches!(
            approach.decisive_action,
            InvestigationActionKind::Watch | InvestigationActionKind::LayAmbush
        ) {
            TimeOfDay::Night
        } else {
            TimeOfDay::Day
        },
        evidence_age_minutes: now.saturating_sub(case.opened_at),
        current_uncertainty_bps: 8_000,
        skills: SkillContribution {
            terrain_bps: capability_bps,
            awareness_bps: capability_bps,
            stealth_bps: capability_bps.saturating_sub(1_000),
            assistance_bps: 1_500,
            familiarity_bps: case.incident_count.saturating_mul(350).min(3_000),
        },
        weather: WeatherAuthority::Unavailable,
    });
    NpcApproachResolution {
        succeeded: resolution.success,
        effective_skill_bps: resolution.effective_skill_bps,
        risk_triggered: resolution.risk_triggered,
        failure_summary: (!resolution.success)
            .then(|| route_failure_summary(approach.decisive_action, resolution.risk_triggered)),
    }
}

fn testimony_for_route(generated: &GeneratedCase, route: RouteClass) -> Option<(String, String)> {
    let statements = crate::quest_generation::player_visible_testimony_sequence(generated);
    let selected = match route {
        RouteClass::PhysicalTrail => statements
            .iter()
            .copied()
            .filter(|(_, statement)| statement.site_id.is_some())
            .next(),
        RouteClass::PatternSurveillance => statements
            .iter()
            .copied()
            .filter(|(_, statement)| statement.site_id.is_none())
            .next(),
        RouteClass::SocialInquiry => statements.first().copied(),
    }?;
    Some((
        selected.0.display_name.clone(),
        selected.1.spoken_text.clone(),
    ))
}

fn route_label(route: RouteClass) -> &'static str {
    match route {
        RouteClass::PhysicalTrail => "the physical trail",
        RouteClass::PatternSurveillance => "the reported pattern of incidents",
        RouteClass::SocialInquiry => "the witness-and-contact route",
    }
}

fn preparation_summary(
    route: RouteClass,
    decisive_action: InvestigationActionKind,
    terrain: crate::investigation_action::Terrain,
) -> String {
    match route {
        RouteClass::PhysicalTrail => format!(
            "The company packed provisions, light, and field tools for tracking through {terrain:?} terrain."
        ),
        RouteClass::PatternSurveillance => {
            if decisive_action == InvestigationActionKind::LayAmbush {
                "The company prepared concealed positions, rotating watches, and provisions for a long ambush.".into()
            } else {
                "The company assigned rotating watches and carried provisions for a prolonged patrol.".into()
            }
        }
        RouteClass::SocialInquiry => {
            "The company compared the accounts, divided the interviews among its members, and prepared to verify identities and locations.".into()
        }
    }
}

fn route_failure_summary(kind: InvestigationActionKind, risk_triggered: bool) -> String {
    let mut summary = match kind {
        InvestigationActionKind::InspectSite => {
            "the inspection produced no conclusive evidence before the company had to withdraw."
        }
        InvestigationActionKind::SearchArea => {
            "the search covered the reported ground without finding a usable trail."
        }
        InvestigationActionKind::FollowTracks => {
            "the trail became unreadable before it reached a destination."
        }
        InvestigationActionKind::ReacquireTracks => {
            "the old traces could not be distinguished from newer traffic."
        }
        InvestigationActionKind::LocateContact => {
            "the referred contact could not be found or independently verified."
        }
        InvestigationActionKind::Watch => {
            "nothing matching the reports appeared during the watched period."
        }
        InvestigationActionKind::Patrol => {
            "the patrol saw no repeat incident during the reported window."
        }
        InvestigationActionKind::LayAmbush => {
            "nothing matching the reports entered the ambush before supplies ran low."
        }
        InvestigationActionKind::ApproachLead => {
            "the reported landmarks did not identify a defensible destination."
        }
    }
    .to_owned();
    if risk_triggered {
        summary.push_str(" The attempt also exposed the company to danger.");
    }
    summary
}

fn resolved_finale_summary(kind: FinaleKind) -> &'static str {
    match kind {
        FinaleKind::Defeat => "defeated the attackers, ending the incidents.",
        FinaleKind::DriveOff => "drove the attackers away from the area.",
        FinaleKind::Capture => "captured the responsible party.",
        FinaleKind::Rescue => "brought the missing person to safety.",
        FinaleKind::RetrieveReturn => "recovered the missing property and returned it.",
        FinaleKind::Expose => "secured enough proof to expose the fabrication.",
        FinaleKind::Negotiate => "reached an agreement that ended the trouble.",
    }
}

fn unresolved_finale_summary(kind: FinaleKind) -> &'static str {
    match kind {
        FinaleKind::Defeat | FinaleKind::DriveOff | FinaleKind::Capture => {
            "the opposition forced the company to retreat."
        }
        FinaleKind::Rescue => "the missing person could not be extracted safely.",
        FinaleKind::RetrieveReturn => "the missing property could not be recovered.",
        FinaleKind::Expose => "the available proof did not substantiate the accusation.",
        FinaleKind::Negotiate => "the parties would not accept workable terms.",
    }
}

fn decision(
    strategy: NpcInterventionStrategy,
    outcome: NpcInterventionOutcome,
    mitigation_bps: u16,
    now: u64,
    roll_bps: u16,
    safe_summary: String,
) -> NpcInterventionDecision {
    NpcInterventionDecision {
        strategy,
        outcome,
        mitigation_bps,
        next_available_at: now.saturating_add(RETRY_DELAY_MINUTES),
        roll_bps,
        safe_summary,
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        local_problem::Scope,
        quest_generation::{GenerationContext, TemplateFamily, generate, test_witnesses},
    };

    fn case() -> NpcCaseSnapshot {
        NpcCaseSnapshot {
            case_id: "case:one".into(),
            problem_id: "problem:one".into(),
            settlement_id: "town".into(),
            opened_at: 1_000,
            incident_count: 3,
            mitigation_bps: 0,
            open: true,
            player_activity_at: None,
        }
    }

    fn party(id: &str, capability: u16) -> NpcPartySnapshot {
        NpcPartySnapshot {
            party_id: id.into(),
            name: format!("{id} Company"),
            settlement_id: "town".into(),
            capability,
            available_at: 0,
        }
    }

    fn generated(family: TemplateFamily) -> GeneratedCase {
        generate(&GenerationContext {
            seed: 17,
            observer_entropy_hi: 1,
            observer_entropy_lo: 2,
            settlement_id: "lubeck".into(),
            settlement_name: "Lubeck".into(),
            scope: Scope::Settlement {
                settlement_id: "lubeck".into(),
            },
            ordinal: 0,
            now_minute: 1_000,
            requested_family: Some(family),
            witness_candidates: test_witnesses(),
        })
        .unwrap()
    }

    #[test]
    fn eligibility_waits_for_age_incidents_and_player_grace() {
        let mut value = case();
        let aged = value.opened_at + MIN_INTERVENTION_AGE_MINUTES;
        assert!(!case_is_eligible(&value, aged - 1));
        assert!(case_is_eligible(&value, aged));
        value.incident_count = 1;
        assert!(!case_is_eligible(&value, u64::MAX));
        value.incident_count = 3;
        value.player_activity_at = Some(aged + 10);
        assert!(!case_is_eligible(
            &value,
            aged + PLAYER_ACTIVITY_GRACE_MINUTES
        ));
        assert!(case_is_eligible(
            &value,
            aged + 10 + PLAYER_ACTIVITY_GRACE_MINUTES
        ));
    }

    #[test]
    fn selection_and_outcome_are_replay_stable() {
        let value = case();
        let weak = party("weak", 40);
        let strong = party("strong", 80);
        assert_eq!(
            select_party(&value, 10_000, [&weak, &strong])
                .unwrap()
                .party_id,
            "strong"
        );
        let strategy = scripted_strategy(&value, &strong);
        assert_eq!(
            decide(&value, &strong, strategy, 1, 10_000),
            decide(&value, &strong, strategy, 1, 10_000)
        );
    }

    #[test]
    fn defer_never_resolves_or_mitigates() {
        let value = case();
        let decision = decide(
            &value,
            &party("cautious", 100),
            NpcInterventionStrategy::Defer,
            1,
            5_000,
        );
        assert_eq!(decision.outcome, NpcInterventionOutcome::Delayed);
        assert_eq!(decision.mitigation_bps, value.mitigation_bps);
    }

    #[test]
    fn generated_routes_use_authored_action_chains_and_retries_change_leads() {
        let generated = generated(TemplateFamily::RecurringDepredation);
        let approaches = supported_investigation_approaches(&generated);
        assert_eq!(approaches.len(), 2);
        assert!(
            approaches
                .iter()
                .all(|approach| !approach.step_summaries.is_empty())
        );
        assert!(approaches.iter().all(|approach| {
            generated.witnesses.iter().any(|witness| {
                witness.display_name == approach.lead_source
                    && witness
                        .testimony
                        .iter()
                        .any(|statement| statement.spoken_text == approach.lead_quote)
            })
        }));
        let first = select_investigation_approach(
            &approaches,
            NpcInterventionStrategy::ConfrontDirectly,
            1,
        )
        .unwrap();
        let retry = select_investigation_approach(
            &approaches,
            NpcInterventionStrategy::ConfrontDirectly,
            2,
        )
        .unwrap();
        assert_ne!(first.route, retry.route);
        let changed_strategy_retry = select_investigation_approach_after(
            &approaches,
            NpcInterventionStrategy::ProtectLocals,
            2,
            Some(first.route),
        )
        .unwrap();
        assert_ne!(first.route, changed_strategy_retry.route);
    }

    #[test]
    fn generated_routes_never_use_withheld_or_unreferred_testimony() {
        let mut generated = generated(TemplateFamily::RecurringDepredation);
        generated.witnesses[0]
            .testimony
            .push(crate::quest_generation::TestimonyDraft {
                proposition_id: "withheld-canary-proposition".into(),
                reliability: crate::quest_generation::Reliability::Truthful,
                delivery: crate::quest_generation::TestimonyDelivery::Withheld,
                truthful_text: "WITHHELD_CANARY".into(),
                spoken_text: "WITHHELD_CANARY".into(),
                challenge_text: "WITHHELD_CANARY".into(),
                challenge_responses: crate::quest_generation::TestimonyChallengeResponses {
                    charm: Some("CANARY_CHARM".into()),
                    command: Some("CANARY_COMMAND".into()),
                    bluff: Some("CANARY_BLUFF".into()),
                },
                destination_stage: "textual".into(),
                site_id: None,
                corrects_proposition_id: None,
                referred_witness_ids: vec![],
            });
        for statement in &mut generated.witnesses[0].testimony {
            statement.referred_witness_ids.clear();
        }
        generated.witnesses[1].display_name = "UNREFERRED_CANARY".into();
        generated.witnesses[1].testimony[0].spoken_text = "UNREFERRED_CANARY".into();
        generated
            .actions
            .iter_mut()
            .find(|action| action.target_kind == "contact")
            .unwrap()
            .target_id = generated.witnesses[1].npc_id.clone();
        let hidden_contact_routes = generated
            .actions
            .iter()
            .filter(|action| {
                action.target_kind == "contact" && action.target_id == generated.witnesses[1].npc_id
            })
            .map(|action| action.route)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(!hidden_contact_routes.is_empty());
        let approaches = supported_investigation_approaches(&generated);
        let rendered = format!("{approaches:?}");
        assert!(!rendered.contains("WITHHELD_CANARY"));
        assert!(!rendered.contains("UNREFERRED_CANARY"));
        assert!(
            approaches
                .iter()
                .all(|approach| !hidden_contact_routes.contains(&approach.route))
        );
    }

    #[test]
    fn failed_supported_route_names_the_setback_and_next_route() {
        let generated = generated(TemplateFamily::DisappearanceOrLoss);
        let approaches = supported_investigation_approaches(&generated);
        let first = select_investigation_approach(
            &approaches,
            NpcInterventionStrategy::InvestigateCarefully,
            1,
        )
        .unwrap();
        let retry = select_investigation_approach(
            &approaches,
            NpcInterventionStrategy::InvestigateCarefully,
            2,
        )
        .unwrap();
        let resolution = NpcApproachResolution {
            succeeded: false,
            effective_skill_bps: 4_000,
            risk_triggered: false,
            failure_summary: Some("the trail ended at a flooded crossing.".into()),
        };
        let decision = decide_after_supported_approach(
            &case(),
            &party("company", 60),
            NpcInterventionStrategy::InvestigateCarefully,
            1,
            10_000,
            Some((first, &resolution, Some(&retry.route_label))),
        );
        assert_eq!(decision.outcome, NpcInterventionOutcome::Failed);
        assert!(decision.safe_summary.contains("flooded crossing"));
        assert!(decision.safe_summary.contains(&retry.route_label));
        assert!(!decision.safe_summary.contains("returned without resolving"));
    }
}
