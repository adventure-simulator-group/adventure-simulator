use super::{
    DeveloperCaseAnalysis, EVAL_FORMAT_VERSION, EvalCaseConfig, InvestigationEnvironment,
    PolicyDecision, PublicQuestTrace, QuestPolicy, Termination, semantic_digest,
};
use adventuresim_core::quest_generation::TemplateFamily;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalLimits {
    pub max_cases: u32,
    pub max_steps_per_case: u32,
    pub max_wall_time_ms: u64,
}

impl Default for EvalLimits {
    fn default() -> Self {
        Self {
            max_cases: 64,
            max_steps_per_case: 64,
            max_wall_time_ms: 60_000,
        }
    }
}

impl EvalLimits {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1_000).contains(&self.max_cases)
            || !(1..=1_000).contains(&self.max_steps_per_case)
            || !(1..=3_600_000).contains(&self.max_wall_time_ms)
        {
            return Err("quest evaluator bounds are invalid".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Measurement<T> {
    Measured(T),
    NotMeasured(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestEvalMetrics {
    pub cases: u32,
    pub solved: u32,
    pub solve_rate: f64,
    pub contract_rate: Measurement<f64>,
    pub mean_steps: f64,
    pub mean_game_minutes: f64,
    pub mean_resource_cost: f64,
    pub route_counts: BTreeMap<String, u32>,
    pub dominant_route_share: f64,
    pub repeated_policy_choices: u32,
    pub dead_ends: u32,
    pub loops: u32,
    pub false_hypothesis_corrections: u32,
    pub preparation_rate: f64,
    pub accidental_discovery_rate: Measurement<f64>,
    pub initial_template_classification: Measurement<f64>,
    pub initial_threat_classification: Measurement<f64>,
    pub terrain_benefit: Measurement<f64>,
    pub insight_benefit: Measurement<f64>,
    pub language_benefit: Measurement<f64>,
    pub perception_benefit: Measurement<f64>,
    pub combat_benefit: Measurement<f64>,
    pub counterfactual_fingerprint_rate: Measurement<f64>,
    pub generator_factor_count: u64,
    pub generator_bridge_count: u64,
    pub catalog_revisions: BTreeSet<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicEvaluationReport {
    pub version: u32,
    pub policy: String,
    pub traces: Vec<PublicQuestTrace>,
    pub metrics: QuestEvalMetrics,
    pub semantic_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeveloperEvaluationReport {
    pub version: u32,
    pub public_report_digest: String,
    pub cases: Vec<DeveloperCaseAnalysis>,
    pub marginal_audit: MarginalAudit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarginalAudit {
    pub family_counts: BTreeMap<String, u32>,
    pub cause_counts: BTreeMap<String, u32>,
    pub true_site_counts: BTreeMap<String, u32>,
    pub factor_count: u64,
    pub bridge_count: u64,
}

#[derive(Clone, Debug)]
pub struct EvaluationBundle {
    pub public: PublicEvaluationReport,
    pub developer: DeveloperEvaluationReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayCase {
    pub seed: u64,
    pub family: TemplateFamily,
    pub decisions: Vec<PolicyDecision>,
    pub expected_digest: String,
}

pub const MAX_REPLAY_DECISIONS: usize = 1_000;

pub fn evaluate_cases(
    configs: &[EvalCaseConfig],
    policy: &mut dyn QuestPolicy,
    limits: &EvalLimits,
) -> Result<EvaluationBundle, String> {
    limits.validate()?;
    if configs.is_empty() || configs.len() > limits.max_cases as usize {
        return Err("case count exceeds evaluator bounds".into());
    }
    let started = Instant::now();
    let deadline = started + Duration::from_millis(limits.max_wall_time_ms);
    let mut traces = Vec::new();
    let mut private = Vec::new();
    for config in configs {
        if Instant::now() >= deadline {
            return Err("quest evaluator wall-time budget exceeded".into());
        }
        let mut environment = InvestigationEnvironment::generate(config.clone())?;
        private.push(environment.developer_analysis().clone());
        let mut termination = Termination::StepLimit;
        let mut state_visits: BTreeMap<String, u8> = BTreeMap::new();
        for _ in 0..limits.max_steps_per_case {
            if environment.is_solved() {
                termination = Termination::Solved;
                break;
            }
            if environment.frame().legal_choices.is_empty() {
                termination = Termination::DeadEnd;
                break;
            }
            let state = observable_state_digest(environment.frame())?;
            let visits = state_visits.entry(state).or_default();
            *visits = visits.saturating_add(1);
            if *visits >= 4 {
                termination = Termination::Loop;
                break;
            }
            if Instant::now() >= deadline {
                termination = Termination::BudgetExceeded;
                break;
            }
            let decision = match policy.decide_before(environment.frame(), deadline) {
                Ok(decision) => decision,
                Err(error) => {
                    termination = if error.contains("wall-time budget") {
                        Termination::BudgetExceeded
                    } else {
                        Termination::PolicyError
                    };
                    break;
                }
            };
            if environment.apply(&decision).is_err() {
                termination = Termination::PolicyError;
                break;
            }
            if Instant::now() >= deadline {
                termination = Termination::BudgetExceeded;
                break;
            }
        }
        if environment.is_solved() {
            termination = Termination::Solved;
        }
        traces.push(environment.public_trace(policy.name().into(), termination)?);
    }
    let metrics = metrics(&traces, &private);
    let mut public = PublicEvaluationReport {
        version: EVAL_FORMAT_VERSION,
        policy: policy.name().into(),
        traces,
        metrics,
        semantic_digest: String::new(),
    };
    public.semantic_digest = semantic_digest(&public)?;
    let developer = DeveloperEvaluationReport {
        version: EVAL_FORMAT_VERSION,
        public_report_digest: public.semantic_digest.clone(),
        marginal_audit: marginal_audit(&private),
        cases: private,
    };
    Ok(EvaluationBundle { public, developer })
}

pub fn replay_case(recorded: &ReplayCase) -> Result<PublicQuestTrace, String> {
    if recorded.decisions.len() > MAX_REPLAY_DECISIONS {
        return Err("recorded action replay exceeds decision cap".into());
    }
    let mut environment = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
        recorded.seed,
        recorded.family,
    ))?;
    for decision in recorded.decisions.iter().take(MAX_REPLAY_DECISIONS) {
        environment.apply(decision)?;
    }
    let termination = if environment.is_solved() {
        Termination::Solved
    } else if environment.frame().legal_choices.is_empty() {
        Termination::DeadEnd
    } else {
        Termination::StepLimit
    };
    let trace = environment.public_trace("recorded-action-replay".into(), termination)?;
    if trace.semantic_digest != recorded.expected_digest {
        return Err("recorded action replay digest mismatch".into());
    }
    Ok(trace)
}

fn observable_state_digest(frame: &super::PlayerFrame) -> Result<String, String> {
    semantic_digest(&(
        &frame.journal,
        &frame.discovery,
        &frame.party,
        frame
            .legal_choices
            .iter()
            .map(|choice| (choice.kind, &choice.label))
            .collect::<Vec<_>>(),
    ))
}

fn metrics(traces: &[PublicQuestTrace], private: &[DeveloperCaseAnalysis]) -> QuestEvalMetrics {
    let cases = traces.len() as u32;
    let solved = traces.iter().filter(|trace| trace.solved).count() as u32;
    let total_steps = traces
        .iter()
        .map(|trace| trace.events.len() as u64)
        .sum::<u64>();
    let total_minutes = traces
        .iter()
        .flat_map(|trace| &trace.events)
        .map(|event| u64::from(event.game_minutes))
        .sum::<u64>();
    let total_cost = traces
        .iter()
        .flat_map(|trace| &trace.events)
        .map(|event| u64::from(event.resource_cost))
        .sum::<u64>();
    let mut route_counts = BTreeMap::new();
    for route in traces.iter().filter_map(|trace| trace.route) {
        *route_counts.entry(format!("{route:?}")).or_default() += 1;
    }
    let dominant = route_counts.values().copied().max().unwrap_or(0);
    let repeated = traces
        .iter()
        .map(|trace| {
            trace
                .events
                .windows(2)
                .filter(|pair| pair[0].choice_kind == pair[1].choice_kind)
                .count() as u32
        })
        .sum();
    let corrections = traces
        .iter()
        .flat_map(|trace| &trace.events)
        .map(|event| event.corrected_proposition_ids.len() as u32)
        .sum();
    let prepared = traces
        .iter()
        .filter(|trace| {
            trace
                .events
                .iter()
                .any(|event| event.choice_kind == super::ChoiceKind::Prepare)
        })
        .count() as u32;
    QuestEvalMetrics {
        cases,
        solved,
        solve_rate: ratio(solved, cases),
        contract_rate: Measurement::NotMeasured(
            "modular #187 cases intentionally have no contract".into(),
        ),
        mean_steps: mean(total_steps, cases),
        mean_game_minutes: mean(total_minutes, cases),
        mean_resource_cost: mean(total_cost, cases),
        route_counts,
        dominant_route_share: ratio(dominant, solved.max(1)),
        repeated_policy_choices: repeated,
        dead_ends: traces
            .iter()
            .filter(|trace| trace.termination == Termination::DeadEnd)
            .count() as u32,
        loops: traces
            .iter()
            .filter(|trace| trace.termination == Termination::Loop)
            .count() as u32,
        false_hypothesis_corrections: corrections,
        preparation_rate: ratio(prepared, cases),
        accidental_discovery_rate: Measurement::NotMeasured(
            "offline graph has source provenance but no runtime perception roll".into(),
        ),
        initial_template_classification: Measurement::NotMeasured(
            "policy decision schema does not ask for a template guess".into(),
        ),
        initial_threat_classification: Measurement::NotMeasured(
            "policy decision schema does not ask for a threat guess".into(),
        ),
        terrain_benefit: Measurement::NotMeasured("requires a matched skill ablation suite".into()),
        insight_benefit: Measurement::NotMeasured("requires a matched skill ablation suite".into()),
        language_benefit: Measurement::NotMeasured("language checks are not in #187 graph".into()),
        perception_benefit: Measurement::NotMeasured(
            "requires a matched skill ablation suite".into(),
        ),
        combat_benefit: Measurement::NotMeasured(
            "evaluator does not duplicate tactical combat".into(),
        ),
        counterfactual_fingerprint_rate: Measurement::NotMeasured(
            "run `quest-eval-suite` with matched policies to populate".into(),
        ),
        generator_factor_count: private
            .iter()
            .map(|case| case.factor_ids.len() as u64)
            .sum(),
        generator_bridge_count: private
            .iter()
            .map(|case| case.bridge_ids.len() as u64)
            .sum(),
        catalog_revisions: private
            .iter()
            .map(|case| case.catalog_revision.clone())
            .collect(),
    }
}

fn marginal_audit(cases: &[DeveloperCaseAnalysis]) -> MarginalAudit {
    let mut family_counts = BTreeMap::new();
    let mut cause_counts = BTreeMap::new();
    let mut true_site_counts = BTreeMap::new();
    for case in cases {
        *family_counts
            .entry(format!("{:?}", case.family))
            .or_default() += 1;
        *cause_counts
            .entry(case.canonical_cause.clone())
            .or_default() += 1;
        *true_site_counts.entry(case.true_site.clone()).or_default() += 1;
    }
    MarginalAudit {
        family_counts,
        cause_counts,
        true_site_counts,
        factor_count: cases.iter().map(|case| case.factor_ids.len() as u64).sum(),
        bridge_count: cases.iter().map(|case| case.bridge_ids.len() as u64).sum(),
    }
}

fn ratio(numerator: u32, denominator: u32) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        f64::from(numerator) / f64::from(denominator)
    }
}

fn mean(total: u64, cases: u32) -> f64 {
    if cases == 0 {
        0.0
    } else {
        total as f64 / f64::from(cases)
    }
}

pub fn golden_suite(seed: u64, cases_per_family: u32) -> Vec<EvalCaseConfig> {
    [
        TemplateFamily::RecurringDepredation,
        TemplateFamily::DisappearanceOrLoss,
    ]
    .into_iter()
    .flat_map(|family| {
        (0..cases_per_family)
            .map(move |offset| EvalCaseConfig::fixture(seed + u64::from(offset), family))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investigation_eval::{MockLlmPolicy, ScriptedPolicy};
    use adventuresim_core::quest_generation::RouteClass;

    #[test]
    fn scripted_and_mock_solve_both_templates_offline() {
        for mut policy in [
            Box::new(ScriptedPolicy::default()) as Box<dyn QuestPolicy>,
            Box::new(MockLlmPolicy) as Box<dyn QuestPolicy>,
        ] {
            let bundle = evaluate_cases(
                &golden_suite(20, 2),
                policy.as_mut(),
                &EvalLimits::default(),
            )
            .unwrap();
            assert_eq!(bundle.public.metrics.solve_rate, 1.0);
            assert!(bundle.public.traces.iter().all(|trace| {
                trace
                    .events
                    .first()
                    .is_some_and(|event| event.choice_kind == super::super::ChoiceKind::EnterTavern)
            }));
            assert!(
                !serde_json::to_string(&bundle.public)
                    .unwrap()
                    .contains("canonical_case_id")
            );
        }
    }

    #[test]
    fn alternate_baselines_cover_two_route_classes() {
        let configs = golden_suite(33, 4);
        let a = evaluate_cases(
            &configs,
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        let b = evaluate_cases(
            &configs,
            &mut ScriptedPolicy {
                prefer_alternate_route: true,
            },
            &EvalLimits::default(),
        )
        .unwrap();
        let routes = a
            .public
            .traces
            .iter()
            .chain(&b.public.traces)
            .filter_map(|trace| trace.route)
            .collect::<BTreeSet<RouteClass>>();
        assert!(routes.len() >= 2);
    }

    #[test]
    fn public_and_private_artifacts_have_one_way_digest_join() {
        let bundle = evaluate_cases(
            &golden_suite(5, 1),
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        assert_eq!(
            bundle.developer.public_report_digest,
            bundle.public.semantic_digest
        );
        let public = serde_json::to_string(&bundle.public).unwrap();
        for case in &bundle.developer.cases {
            assert!(!public.contains(&case.canonical_case_id));
            assert!(!public.contains(&case.generator_manifest_digest));
        }
    }

    #[test]
    fn correction_metric_uses_structured_ids_not_prose() {
        let trace = PublicQuestTrace {
            version: EVAL_FORMAT_VERSION,
            case_id: "case:public".into(),
            policy: "fixture".into(),
            events: vec![super::super::PublicTraceEvent {
                step: 0,
                frame_digest: "digest".into(),
                choice_id: "choice:fixture".into(),
                choice_kind: super::super::ChoiceKind::InterviewWitness,
                result: "ordinary testimony".into(),
                learned: vec!["no correction wording here".into()],
                corrected_proposition_ids: vec!["claim:wrong".into()],
                game_minutes: 1,
                resource_cost: 0,
            }],
            solved: false,
            exhausted: false,
            termination: Termination::StepLimit,
            route: None,
            semantic_digest: "fixture".into(),
        };
        assert_eq!(metrics(&[trace], &[]).false_hypothesis_corrections, 1);
    }

    #[test]
    fn replay_rejects_unbounded_decision_lists_before_execution() {
        let recorded = ReplayCase {
            seed: 1,
            family: TemplateFamily::RecurringDepredation,
            decisions: vec![
                PolicyDecision {
                    version: EVAL_FORMAT_VERSION,
                    choice_id: "choice:fixture".into(),
                    arguments: Default::default(),
                };
                MAX_REPLAY_DECISIONS + 1
            ],
            expected_digest: "unused".into(),
        };
        assert!(replay_case(&recorded).is_err());
    }
}
