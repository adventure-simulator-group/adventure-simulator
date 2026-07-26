use super::{
    DeveloperCaseAnalysis, EVAL_FORMAT_VERSION, EvalCaseConfig, InvestigationEnvironment,
    PolicyClassification, PolicyDecision, PolicyRunMetadata, PublicQuestTrace, QuestPolicy,
    Termination, TerminationErrorCode, semantic_digest,
};
use adventuresim_core::quest_generation::{RouteClass, TemplateFamily};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalLimits {
    pub max_cases: u32,
    pub max_steps_per_case: u32,
    pub max_wall_time_ms: u64,
    pub max_total_events: u32,
    pub max_output_bytes: u64,
    pub max_total_output_bytes: u64,
}

impl Default for EvalLimits {
    fn default() -> Self {
        Self {
            max_cases: 64,
            max_steps_per_case: 64,
            max_wall_time_ms: 60_000,
            max_total_events: 16_384,
            max_output_bytes: 32 * 1024 * 1024,
            max_total_output_bytes: 64 * 1024 * 1024,
        }
    }
}

impl EvalLimits {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1_000).contains(&self.max_cases)
            || !(1..=1_000).contains(&self.max_steps_per_case)
            || !(1..=3_600_000).contains(&self.max_wall_time_ms)
            || !(1..=100_000).contains(&self.max_total_events)
            || !(1..=100 * 1024 * 1024).contains(&self.max_output_bytes)
            || !(1..=300 * 1024 * 1024).contains(&self.max_total_output_bytes)
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
    pub solve_rate_denominator: u32,
    pub contract_rate: Measurement<f64>,
    pub mean_steps: f64,
    pub mean_game_minutes: f64,
    pub mean_resource_cost: f64,
    pub route_counts: BTreeMap<String, u32>,
    pub unique_path_fingerprints: u32,
    pub path_fingerprint_counts: BTreeMap<String, u32>,
    pub path_diversity_rate: f64,
    pub dominant_path_share: f64,
    pub action_kind_counts: BTreeMap<String, u32>,
    pub action_fingerprint_counts: BTreeMap<String, u32>,
    pub dominant_action_share: f64,
    pub dominant_route_share: f64,
    pub repeated_policy_choices: u32,
    pub dead_ends: u32,
    pub loops: u32,
    pub false_hypothesis_corrections: u32,
    pub corrected_case_count: u32,
    pub mean_false_belief_persistence_steps: Measurement<f64>,
    pub preparation_rate: f64,
    pub prepared_cases: u32,
    pub prepared_solve_rate: Measurement<f64>,
    pub unprepared_solve_rate: Measurement<f64>,
    pub policy_errors: u32,
    pub termination_counts: BTreeMap<String, u32>,
    pub accidental_discovery_rate: Measurement<f64>,
    pub initial_template_classification_coverage: f64,
    pub initial_threat_classification_coverage: f64,
    pub terrain_benefit: Measurement<f64>,
    pub insight_benefit: Measurement<f64>,
    pub language_benefit: Measurement<f64>,
    pub perception_benefit: Measurement<f64>,
    pub combat_benefit: Measurement<f64>,
    pub counterfactual_fingerprint_rate: Measurement<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationProvenance {
    pub surface: String,
    pub authority: String,
    pub observation_source: String,
    pub format_revision: u32,
    pub limits: EvalLimits,
    pub policy: PolicyRunMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicEvaluationReport {
    pub version: u32,
    pub policy: String,
    pub provenance: EvaluationProvenance,
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
    pub classification_audit: ClassificationAudit,
    pub counterfactual_audit: CounterfactualAudit,
    pub privacy_audit: PrivacyAudit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarginalAudit {
    pub family_counts: BTreeMap<String, u32>,
    pub cause_counts: BTreeMap<String, u32>,
    pub true_site_counts: BTreeMap<String, u32>,
    pub factor_count: u64,
    pub bridge_count: u64,
    pub catalog_revisions: BTreeSet<String>,
    pub factor_id_counts: BTreeMap<String, u32>,
    pub bridge_id_counts: BTreeMap<String, u32>,
    pub accepted_factor_rows: u64,
    pub rejected_factor_rows: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationAudit {
    pub cases: u32,
    pub template_guesses: u32,
    pub correct_template_guesses: u32,
    pub template_accuracy: Measurement<f64>,
    pub threat_accuracy: Measurement<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualAudit {
    pub cases: u32,
    pub naturally_matched_groups: u32,
    pub matched_cases: u32,
    pub comparisons: u32,
    pub divergent_comparisons: u32,
    pub fingerprint_divergence_rate: Measurement<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyAudit {
    pub structural_public_private_type_split: bool,
    pub private_canary_occurrences_in_public_json: u32,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct EvaluationBundle {
    pub public: PublicEvaluationReport,
    pub developer: DeveloperEvaluationReport,
    pub artifacts: EvaluationArtifacts,
}

#[derive(Clone, Debug)]
pub struct EvaluationArtifacts {
    pub public_json: Vec<u8>,
    pub developer_json: Vec<u8>,
    pub stories_markdown: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayCase {
    pub version: u32,
    pub catalog_revision: String,
    pub generator_manifest_digest: String,
    pub seed: u64,
    pub family: TemplateFamily,
    pub decisions: Vec<PolicyDecision>,
    pub expected: ReplayExpectations,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayExpectations {
    pub solved: bool,
    pub termination: Termination,
    pub route: Option<RouteClass>,
    pub event_count: u32,
    pub semantic_digest: Option<String>,
}

pub const MAX_REPLAY_DECISIONS: usize = 1_000;

/// Render the player-visible half of an evaluation as a chronological anthology.
///
/// This deliberately accepts only the public report, so canonical cause, true
/// site, weights, and other developer-only generator authority cannot leak into
/// the story artifact.
pub fn render_markdown_stories(report: &PublicEvaluationReport) -> String {
    let mut output = String::from(
        "# Quest evaluation stories\n\n\
         These stories contain only information exposed to the simulated player. \
         Dialogue is reproduced exactly as emitted by the evaluation environment.\n\n",
    );
    for (index, trace) in report.traces.iter().enumerate() {
        let outcome = if trace.solved {
            "completed"
        } else {
            match trace.termination {
                Termination::StepLimit => "unfinished (step limit)",
                Termination::DeadEnd => "unfinished (dead end)",
                Termination::Loop => "unfinished (loop detected)",
                Termination::PolicyError => "unfinished (policy error)",
                Termination::BudgetExceeded => "unfinished (budget exceeded)",
                Termination::Solved => "completed",
            }
        };
        writeln!(output, "## Story {}: {}", index + 1, trace.title).unwrap();
        writeln!(output).unwrap();
        writeln!(output, "- Case: `{}`", trace.case_id).unwrap();
        writeln!(output, "- Policy: {}", trace.policy).unwrap();
        writeln!(output, "- Outcome: {outcome}").unwrap();
        writeln!(output, "- Problem: {}", trace.problem_summary).unwrap();

        for event in &trace.events {
            let (day, hour, minute) = story_time(event.game_minute);
            writeln!(output).unwrap();
            writeln!(
                output,
                "### Day {day}, {hour:02}:{minute:02} — {}",
                event.location
            )
            .unwrap();
            writeln!(output).unwrap();
            writeln!(output, "**Player action:** {}", event.action_label).unwrap();

            for line in &event.dialogue {
                writeln!(output).unwrap();
                writeln!(output, "> **{}:** {}", line.speaker, line.text).unwrap();
            }

            let non_dialogue_discoveries = event
                .learned
                .iter()
                .filter(|learned| {
                    !event
                        .dialogue
                        .iter()
                        .any(|line| line.text.contains(learned.as_str()))
                })
                .collect::<Vec<_>>();
            if !non_dialogue_discoveries.is_empty() {
                writeln!(output).unwrap();
                writeln!(output, "**Recorded:**").unwrap();
                for learned in non_dialogue_discoveries {
                    writeln!(output, "- {learned}").unwrap();
                }
            }

            writeln!(output).unwrap();
            writeln!(output, "**Result:** {}", event.result).unwrap();
            if event.game_minutes > 0 || event.resource_cost > 0 {
                write!(output, "**Cost:** {} in-game minutes", event.game_minutes).unwrap();
                if event.resource_cost > 0 {
                    write!(output, "; {} supplies", event.resource_cost).unwrap();
                }
                writeln!(output).unwrap();
            }
        }
        writeln!(output).unwrap();
    }
    output
}

fn story_time(game_minute: u64) -> (u64, u64, u64) {
    (
        game_minute / (24 * 60) + 1,
        game_minute % (24 * 60) / 60,
        game_minute % 60,
    )
}

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
    let mut total_events = 0_u32;
    for config in configs {
        if Instant::now() >= deadline {
            return Err("quest evaluator wall-time budget exceeded".into());
        }
        let mut environment = InvestigationEnvironment::generate(config.clone())?;
        private.push(environment.developer_analysis().clone());
        let mut termination = Termination::StepLimit;
        let mut termination_error = None;
        let mut initial_classification = PolicyClassification::default();
        let mut initial_observation_digest = observable_state_digest(environment.frame())?;
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
                    termination_error = Some(if termination == Termination::BudgetExceeded {
                        TerminationErrorCode::BudgetExceeded
                    } else {
                        TerminationErrorCode::PolicyFailure
                    });
                    break;
                }
            };
            if environment.apply(&decision).is_err() {
                termination = Termination::PolicyError;
                termination_error = Some(TerminationErrorCode::InvalidDecision);
                break;
            }
            total_events = total_events.saturating_add(1);
            if total_events > limits.max_total_events {
                return Err("quest evaluator total event budget exceeded".into());
            }
            if environment.frame().step == 1 {
                initial_observation_digest = observable_state_digest(environment.frame())?;
                initial_classification = policy.classify(environment.frame())?;
                initial_classification.validate()?;
            }
            if Instant::now() >= deadline {
                termination = Termination::BudgetExceeded;
                break;
            }
        }
        if environment.is_solved() {
            termination = Termination::Solved;
        }
        traces.push(environment.public_trace(
            policy.name().into(),
            initial_observation_digest,
            initial_classification,
            termination,
            termination_error,
        )?);
    }
    let metrics = metrics(&traces);
    let policy_metadata = policy.run_metadata();
    let mut public = PublicEvaluationReport {
        version: EVAL_FORMAT_VERSION,
        policy: policy.name().into(),
        provenance: EvaluationProvenance {
            surface: "offline_projection".into(),
            authority: "generator_content_analysis_only".into(),
            observation_source: "synthetic_player_frame".into(),
            format_revision: EVAL_FORMAT_VERSION,
            limits: limits.clone(),
            policy: policy_metadata,
        },
        traces,
        metrics,
        semantic_digest: String::new(),
    };
    public.semantic_digest = semantic_digest(&public)?;
    let public_json = serde_json::to_vec_pretty(&public).map_err(|error| error.to_string())?;
    let privacy_audit = privacy_audit(&public_json, &private)?;
    let developer = DeveloperEvaluationReport {
        version: EVAL_FORMAT_VERSION,
        public_report_digest: public.semantic_digest.clone(),
        marginal_audit: marginal_audit(&private),
        classification_audit: classification_audit(&public.traces, &private),
        counterfactual_audit: counterfactual_audit(&public.traces, &private),
        privacy_audit,
        cases: private,
    };
    let developer_json =
        serde_json::to_vec_pretty(&developer).map_err(|error| error.to_string())?;
    let stories_markdown = render_markdown_stories(&public).into_bytes();
    validate_artifact_sizes(
        [
            ("public JSON", public_json.len()),
            ("developer JSON", developer_json.len()),
            ("Markdown stories", stories_markdown.len()),
        ],
        limits,
    )?;
    Ok(EvaluationBundle {
        public,
        developer,
        artifacts: EvaluationArtifacts {
            public_json,
            developer_json,
            stories_markdown,
        },
    })
}

pub fn replay_case(recorded: &ReplayCase) -> Result<PublicQuestTrace, String> {
    if recorded.version != EVAL_FORMAT_VERSION {
        return Err("replay fixture schema version mismatch".into());
    }
    if recorded.decisions.len() > MAX_REPLAY_DECISIONS {
        return Err("recorded action replay exceeds decision cap".into());
    }
    let mut environment = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
        recorded.seed,
        recorded.family,
    ))?;
    if environment.developer_analysis().catalog_revision != recorded.catalog_revision
        || environment.developer_analysis().generator_manifest_digest
            != recorded.generator_manifest_digest
    {
        return Err("replay fixture generator revision or manifest mismatch".into());
    }
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
    let trace = environment.public_trace(
        "recorded-action-replay".into(),
        observable_state_digest(environment.frame())?,
        PolicyClassification::default(),
        termination,
        None,
    )?;
    if trace.solved != recorded.expected.solved
        || trace.termination != recorded.expected.termination
        || trace.route != recorded.expected.route
        || trace.events.len() as u32 != recorded.expected.event_count
    {
        return Err("recorded action replay stable expectation mismatch".into());
    }
    if recorded
        .expected
        .semantic_digest
        .as_ref()
        .is_some_and(|expected| expected != &trace.semantic_digest)
    {
        return Err("recorded action replay optional digest mismatch".into());
    }
    Ok(trace)
}

pub fn promote_replay_candidate(
    bundle: &EvaluationBundle,
    case_index: usize,
) -> Result<ReplayCase, String> {
    let trace = bundle
        .public
        .traces
        .get(case_index)
        .ok_or("promotion case index is out of range")?;
    let developer = bundle
        .developer
        .cases
        .get(case_index)
        .ok_or("promotion developer join is out of range")?;
    if trace.solved {
        return Err("promotion accepts only an observed non-solving case".into());
    }
    if !matches!(
        trace.termination,
        Termination::StepLimit | Termination::DeadEnd
    ) {
        return Err(
            "promotion accepts only step-limit or dead-end failures reproducible by action replay"
                .into(),
        );
    }
    Ok(ReplayCase {
        version: EVAL_FORMAT_VERSION,
        catalog_revision: developer.catalog_revision.clone(),
        generator_manifest_digest: developer.generator_manifest_digest.clone(),
        seed: developer.generation_seed,
        family: developer.family,
        decisions: trace
            .events
            .iter()
            .map(|event| PolicyDecision {
                version: EVAL_FORMAT_VERSION,
                choice_id: event.choice_id.clone(),
                arguments: Default::default(),
            })
            .collect(),
        expected: ReplayExpectations {
            solved: trace.solved,
            termination: trace.termination,
            route: trace.route,
            event_count: trace.events.len() as u32,
            semantic_digest: None,
        },
    })
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

fn validate_artifact_sizes(
    artifacts: [(&str, usize); 3],
    limits: &EvalLimits,
) -> Result<(), String> {
    let mut total = 0_u64;
    for (name, size) in artifacts {
        let size = size as u64;
        if size > limits.max_output_bytes {
            return Err(format!(
                "quest evaluator {name} exceeds per-artifact byte budget"
            ));
        }
        total = total
            .checked_add(size)
            .ok_or("quest evaluator artifact byte count overflow")?;
    }
    if total > limits.max_total_output_bytes {
        return Err("quest evaluator artifacts exceed total output byte budget".into());
    }
    Ok(())
}

fn metrics(traces: &[PublicQuestTrace]) -> QuestEvalMetrics {
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
    let mut path_fingerprint_counts = BTreeMap::new();
    let mut action_kind_counts = BTreeMap::new();
    let mut action_fingerprint_counts = BTreeMap::new();
    for trace in traces {
        let path = trace
            .events
            .iter()
            .map(observer_safe_action_identity)
            .collect::<Vec<_>>()
            .join(">");
        let fingerprint = blake3::hash(path.as_bytes()).to_hex()[..16].to_string();
        *path_fingerprint_counts.entry(fingerprint).or_default() += 1;
        for event in &trace.events {
            *action_kind_counts
                .entry(format!("{:?}", event.choice_kind))
                .or_default() += 1;
            *action_fingerprint_counts
                .entry(observer_safe_action_identity(event))
                .or_default() += 1;
        }
    }
    let dominant_path = path_fingerprint_counts.values().copied().max().unwrap_or(0);
    let dominant_action = action_fingerprint_counts
        .values()
        .copied()
        .max()
        .unwrap_or(0);
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
    let corrected_case_count = traces
        .iter()
        .filter(|trace| {
            trace
                .events
                .iter()
                .any(|event| !event.corrected_proposition_ids.is_empty())
        })
        .count() as u32;
    let persistence = correction_persistence(traces);
    let prepared = traces
        .iter()
        .filter(|trace| {
            trace
                .events
                .iter()
                .any(|event| event.choice_kind == super::ChoiceKind::Prepare)
        })
        .count() as u32;
    let mut termination_counts = BTreeMap::new();
    for trace in traces {
        *termination_counts
            .entry(format!("{:?}", trace.termination))
            .or_default() += 1;
    }
    QuestEvalMetrics {
        cases,
        solved,
        solve_rate: ratio(solved, cases),
        solve_rate_denominator: cases,
        contract_rate: Measurement::NotMeasured(
            "modular #187 cases intentionally have no contract".into(),
        ),
        mean_steps: mean(total_steps, cases),
        mean_game_minutes: mean(total_minutes, cases),
        mean_resource_cost: mean(total_cost, cases),
        route_counts,
        unique_path_fingerprints: path_fingerprint_counts.len() as u32,
        path_diversity_rate: ratio(path_fingerprint_counts.len() as u32, cases),
        dominant_path_share: ratio(dominant_path, cases),
        path_fingerprint_counts,
        dominant_action_share: ratio(dominant_action, total_steps as u32),
        action_kind_counts,
        action_fingerprint_counts,
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
        corrected_case_count,
        mean_false_belief_persistence_steps: persistence,
        preparation_rate: ratio(prepared, cases),
        prepared_cases: prepared,
        prepared_solve_rate: subgroup_solve_rate(traces, true),
        unprepared_solve_rate: subgroup_solve_rate(traces, false),
        policy_errors: traces
            .iter()
            .filter(|trace| trace.termination == Termination::PolicyError)
            .count() as u32,
        termination_counts,
        accidental_discovery_rate: Measurement::NotMeasured(
            "offline graph has source provenance but no runtime perception roll".into(),
        ),
        initial_template_classification_coverage: ratio(
            traces
                .iter()
                .filter(|trace| trace.initial_classification.template_guess.is_some())
                .count() as u32,
            cases,
        ),
        initial_threat_classification_coverage: ratio(
            traces
                .iter()
                .filter(|trace| trace.initial_classification.threat_guess.is_some())
                .count() as u32,
            cases,
        ),
        terrain_benefit: Measurement::NotMeasured(
            "offline projection displays terrain skill but does not apply it causally".into(),
        ),
        insight_benefit: Measurement::NotMeasured(
            "offline projection displays insight but does not apply it causally".into(),
        ),
        language_benefit: Measurement::NotMeasured(
            "language checks are absent from the generated investigation graph".into(),
        ),
        perception_benefit: Measurement::NotMeasured(
            "offline projection has no causal perception roll".into(),
        ),
        combat_benefit: Measurement::NotMeasured(
            "evaluator does not duplicate tactical combat".into(),
        ),
        counterfactual_fingerprint_rate: Measurement::NotMeasured(
            "counterfactuals are developer-side and require naturally matched visible prefixes"
                .into(),
        ),
    }
}

fn subgroup_solve_rate(traces: &[PublicQuestTrace], prepared: bool) -> Measurement<f64> {
    let subgroup = traces
        .iter()
        .filter(|trace| {
            trace
                .events
                .iter()
                .any(|event| !event.preparation_tags.is_empty())
                == prepared
        })
        .collect::<Vec<_>>();
    if subgroup.is_empty() {
        Measurement::NotMeasured(format!(
            "no {} cases in this run",
            if prepared { "prepared" } else { "unprepared" }
        ))
    } else {
        Measurement::Measured(
            subgroup.iter().filter(|trace| trace.solved).count() as f64 / subgroup.len() as f64,
        )
    }
}

fn correction_persistence(traces: &[PublicQuestTrace]) -> Measurement<f64> {
    let mut durations = Vec::new();
    for trace in traces {
        let mut first_seen = BTreeMap::new();
        for event in &trace.events {
            for corrected in &event.corrected_proposition_ids {
                if let Some(first_step) = first_seen.get(corrected) {
                    durations.push(event.step.saturating_sub(*first_step));
                }
            }
            for learned in &event.learned_claim_ids {
                first_seen.entry(learned.clone()).or_insert(event.step);
            }
        }
    }
    if durations.is_empty() {
        Measurement::NotMeasured(
            "no correction had a measurable prior public observation in this run".into(),
        )
    } else {
        Measurement::Measured(
            durations.iter().map(|value| u64::from(*value)).sum::<u64>() as f64
                / durations.len() as f64,
        )
    }
}

fn observer_safe_action_identity(event: &super::PublicTraceEvent) -> String {
    let label_digest = blake3::hash(event.action_label.as_bytes()).to_hex();
    format!("{:?}:{}", event.choice_kind, &label_digest[..16])
}

fn marginal_audit(cases: &[DeveloperCaseAnalysis]) -> MarginalAudit {
    let mut family_counts = BTreeMap::new();
    let mut cause_counts = BTreeMap::new();
    let mut true_site_counts = BTreeMap::new();
    let mut factor_id_counts = BTreeMap::new();
    let mut bridge_id_counts = BTreeMap::new();
    for case in cases {
        *family_counts
            .entry(format!("{:?}", case.family))
            .or_default() += 1;
        *cause_counts
            .entry(case.canonical_cause.clone())
            .or_default() += 1;
        *true_site_counts.entry(case.true_site.clone()).or_default() += 1;
        for row in &case.factor_trace {
            for factor in &row.factor_ids {
                *factor_id_counts.entry(factor.0.clone()).or_default() += 1;
            }
        }
        for bridge in &case.bridges {
            *bridge_id_counts.entry(bridge.id.0.clone()).or_default() += 1;
        }
    }
    MarginalAudit {
        family_counts,
        cause_counts,
        true_site_counts,
        factor_count: cases
            .iter()
            .flat_map(|case| &case.factor_trace)
            .map(|row| row.factor_ids.len() as u64)
            .sum(),
        bridge_count: cases.iter().map(|case| case.bridges.len() as u64).sum(),
        catalog_revisions: cases
            .iter()
            .map(|case| case.catalog_revision.clone())
            .collect(),
        factor_id_counts,
        bridge_id_counts,
        accepted_factor_rows: cases
            .iter()
            .flat_map(|case| &case.factor_trace)
            .filter(|row| row.accepted)
            .count() as u64,
        rejected_factor_rows: cases
            .iter()
            .flat_map(|case| &case.factor_trace)
            .filter(|row| !row.accepted)
            .count() as u64,
    }
}

fn classification_audit(
    traces: &[PublicQuestTrace],
    cases: &[DeveloperCaseAnalysis],
) -> ClassificationAudit {
    let mut guesses = 0_u32;
    let mut correct = 0_u32;
    for (trace, case) in traces.iter().zip(cases) {
        if let Some(guess) = &trace.initial_classification.template_guess {
            guesses += 1;
            let truth = format!("{:?}", case.family).to_ascii_lowercase();
            let normalized_guess = guess.replace('_', "").to_ascii_lowercase();
            if truth == normalized_guess {
                correct += 1;
            }
        }
    }
    ClassificationAudit {
        cases: traces.len() as u32,
        template_guesses: guesses,
        correct_template_guesses: correct,
        template_accuracy: if guesses == 0 {
            Measurement::NotMeasured("policy emitted no template classifications".into())
        } else {
            Measurement::Measured(ratio(correct, guesses))
        },
        threat_accuracy: Measurement::NotMeasured(
            "generated player frame exposes no stable player-facing threat taxonomy".into(),
        ),
    }
}

fn counterfactual_audit(
    traces: &[PublicQuestTrace],
    cases: &[DeveloperCaseAnalysis],
) -> CounterfactualAudit {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, trace) in traces.iter().enumerate() {
        groups
            .entry(trace.initial_observation_digest.as_str())
            .or_default()
            .push(index);
    }
    let coherent = groups
        .values()
        .filter(|indices| {
            indices.len() > 1
                && indices.iter().any(|left| {
                    indices
                        .iter()
                        .any(|right| cases[*left].canonical_cause != cases[*right].canonical_cause)
                })
        })
        .collect::<Vec<_>>();
    let matched_cases = coherent.iter().map(|group| group.len() as u32).sum();
    let mut comparisons = 0_u32;
    let mut divergent = 0_u32;
    for group in &coherent {
        for (left_position, left_index) in group.iter().enumerate() {
            for right_index in group.iter().skip(left_position + 1) {
                if cases[*left_index].canonical_cause == cases[*right_index].canonical_cause {
                    continue;
                }
                comparisons += 1;
                let left_path = traces[*left_index]
                    .events
                    .iter()
                    .map(observer_safe_action_identity)
                    .collect::<Vec<_>>();
                let right_path = traces[*right_index]
                    .events
                    .iter()
                    .map(observer_safe_action_identity)
                    .collect::<Vec<_>>();
                if left_path != right_path {
                    divergent += 1;
                }
            }
        }
    }
    CounterfactualAudit {
        cases: traces.len() as u32,
        naturally_matched_groups: coherent.len() as u32,
        matched_cases,
        comparisons,
        divergent_comparisons: divergent,
        fingerprint_divergence_rate: if comparisons == 0 {
            Measurement::NotMeasured(
                "no different hidden causes shared an identical player-visible initial prefix"
                    .into(),
            )
        } else {
            Measurement::Measured(ratio(divergent, comparisons))
        },
    }
}

fn privacy_audit(
    public_json: &[u8],
    cases: &[DeveloperCaseAnalysis],
) -> Result<PrivacyAudit, String> {
    let public = String::from_utf8_lossy(public_json);
    let mut occurrences = 0_u32;
    for case in cases {
        // Use high-entropy authority values that have no legitimate public
        // rendering. Factor candidate IDs can intentionally name public
        // catalog concepts, so treating every candidate as a canary creates
        // false positives rather than strengthening this heuristic.
        for private in [
            case.canonical_case_id.as_str(),
            case.generator_manifest_digest.as_str(),
        ] {
            if !private.is_empty() && public.contains(private) {
                occurrences = occurrences.saturating_add(1);
            }
        }
    }
    let structural_public_private_type_split = occurrences == 0;
    if !structural_public_private_type_split {
        return Err("private authority canary detected in public evaluation artifact".into());
    }
    Ok(PrivacyAudit {
        structural_public_private_type_split,
        private_canary_occurrences_in_public_json: occurrences,
        note:
            "Heuristic canary scan plus separate serialization types; not a formal privacy proof."
                .into(),
    })
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
    .enumerate()
    .flat_map(|(family_index, family)| {
        (0..cases_per_family).map(move |offset| {
            let suite_offset = (family_index as u64)
                .wrapping_mul(u64::from(cases_per_family))
                .wrapping_add(u64::from(offset));
            EvalCaseConfig::fixture(seed.wrapping_add(suite_offset), family)
        })
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
    fn golden_suite_uses_distinct_public_case_ids_across_families() {
        let bundle = evaluate_cases(
            &golden_suite(41, 2),
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        let ids = bundle
            .public
            .traces
            .iter()
            .map(|trace| trace.case_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), bundle.public.traces.len());
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
        assert_eq!(
            bundle
                .developer
                .privacy_audit
                .private_canary_occurrences_in_public_json,
            0
        );
        assert!(!public.contains("catalog_revision"));
        assert!(!public.contains("factor_trace"));
        assert!(!public.contains("\"bridges\""));
    }

    #[test]
    fn player_frames_use_run_local_handles_not_generator_ids() {
        let mut environment = InvestigationEnvironment::generate(EvalCaseConfig::fixture(
            11,
            TemplateFamily::DisappearanceOrLoss,
        ))
        .unwrap();
        let enter = environment.frame().legal_choices[0].choice_id.clone();
        environment
            .apply(&PolicyDecision {
                version: EVAL_FORMAT_VERSION,
                choice_id: enter,
                arguments: Default::default(),
            })
            .unwrap();
        let public = serde_json::to_string(environment.frame()).unwrap();
        assert!(public.contains("witness:observed-"));
        for factor in &environment.developer_analysis().factor_trace {
            assert!(!public.contains(&factor.candidate_id));
        }
    }

    #[test]
    fn correction_metric_uses_structured_ids_not_prose() {
        let trace = PublicQuestTrace {
            version: EVAL_FORMAT_VERSION,
            case_id: "case:public".into(),
            policy: "fixture".into(),
            title: "Fixture investigation".into(),
            problem_summary: "Something happened.".into(),
            initial_observation_digest: "initial".into(),
            initial_classification: PolicyClassification::default(),
            events: vec![super::super::PublicTraceEvent {
                step: 0,
                game_minute: 0,
                location: "the market".into(),
                observation_provenance: "offline_projection/player_frame".into(),
                pre_observation_digest: "pre".into(),
                post_observation_digest: "post".into(),
                choice_id: "choice:fixture".into(),
                choice_kind: super::super::ChoiceKind::InterviewWitness,
                action_label: "Speak with Marta.".into(),
                dialogue: vec![super::super::PublicDialogueLine {
                    speaker: "Marta".into(),
                    text: "I heard iron scrape against stone.".into(),
                }],
                result: "ordinary testimony".into(),
                learned: vec!["no correction wording here".into()],
                learned_claim_ids: vec!["claim:wrong".into()],
                corrected_proposition_ids: vec!["claim:wrong".into()],
                preparation_tags: Vec::new(),
                game_minutes: 1,
                resource_cost: 0,
            }],
            solved: false,
            exhausted: false,
            termination: Termination::StepLimit,
            termination_error: None,
            route: None,
            semantic_digest: "fixture".into(),
        };
        let measured = metrics(&[trace]);
        assert_eq!(measured.false_hypothesis_corrections, 1);
        assert!(matches!(
            measured.mean_false_belief_persistence_steps,
            Measurement::NotMeasured(_)
        ));
    }

    #[test]
    fn fingerprints_distinguish_same_kind_actions_by_visible_identity() {
        let bundle = evaluate_cases(
            &golden_suite(5, 1),
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        let mut left = bundle.public.traces[0].clone();
        let mut right = left.clone();
        left.events[0].choice_kind = super::super::ChoiceKind::Investigate;
        right.events[0].choice_kind = super::super::ChoiceKind::Investigate;
        left.events[0].action_label = "Inspect the eastern tracks.".into();
        right.events[0].action_label = "Inspect the western tracks.".into();
        let measured = metrics(&[left, right]);
        assert_eq!(measured.unique_path_fingerprints, 2);
        assert!(measured.action_fingerprint_counts.len() >= 2);
    }

    #[test]
    fn markdown_story_preserves_exact_dialogue_and_public_chronology() {
        let bundle = evaluate_cases(
            &golden_suite(5, 1),
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        let story = render_markdown_stories(&bundle.public);
        let first_dialogue = &bundle.public.traces[0].events[0].dialogue[0];
        assert!(story.contains(&format!(
            "> **{}:** {}",
            first_dialogue.speaker, first_dialogue.text
        )));
        assert!(story.contains("**Player action:**"));
        assert!(story.contains("### Day 1, 00:00"));
        assert!(!story.to_ascii_lowercase().contains("true site"));
        assert!(!story.to_ascii_lowercase().contains("unspecified"));
        for case in &bundle.developer.cases {
            assert!(!story.contains(&case.canonical_case_id));
            assert!(!story.contains(&case.generator_manifest_digest));
            assert!(!story.contains(&case.true_site));
        }
    }

    #[test]
    fn replay_rejects_unbounded_decision_lists_before_execution() {
        let recorded = ReplayCase {
            version: EVAL_FORMAT_VERSION,
            catalog_revision: "fixture".into(),
            generator_manifest_digest: "fixture".into(),
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
            expected: ReplayExpectations {
                solved: false,
                termination: Termination::StepLimit,
                route: None,
                event_count: 0,
                semantic_digest: None,
            },
        };
        assert!(replay_case(&recorded).is_err());
    }

    #[test]
    fn promoted_failure_replays_against_stable_expectations() {
        let limits = EvalLimits {
            max_cases: 1,
            max_steps_per_case: 1,
            ..EvalLimits::default()
        };
        let bundle = evaluate_cases(
            &[EvalCaseConfig::fixture(
                41,
                TemplateFamily::RecurringDepredation,
            )],
            &mut MockLlmPolicy,
            &limits,
        )
        .unwrap();
        let fixture = promote_replay_candidate(&bundle, 0).unwrap();
        assert!(!fixture.expected.solved);
        let first = replay_case(&fixture).unwrap();
        let second = replay_case(&fixture).unwrap();
        assert_eq!(first.termination, fixture.expected.termination);
        assert_eq!(first.semantic_digest, second.semantic_digest);

        for termination in [
            Termination::Loop,
            Termination::PolicyError,
            Termination::BudgetExceeded,
        ] {
            let mut rejected = bundle.clone();
            rejected.public.traces[0].termination = termination;
            assert!(promote_replay_candidate(&rejected, 0).is_err());
        }
    }

    #[test]
    fn checked_in_failure_fixture_replays_deterministically() {
        let fixture: ReplayCase = serde_json::from_str(include_str!(
            "../../fixtures/quest-analysis-failure-v3.json"
        ))
        .unwrap();
        let first = replay_case(&fixture).unwrap();
        let second = replay_case(&fixture).unwrap();
        assert!(!first.solved);
        assert_eq!(first.termination, Termination::StepLimit);
        assert_eq!(first.semantic_digest, second.semantic_digest);
    }

    #[test]
    fn counterfactual_pairs_are_complete_and_order_invariant() {
        let bundle = evaluate_cases(
            &golden_suite(71, 2),
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        let mut traces = bundle.public.traces[..3].to_vec();
        let mut cases = bundle.developer.cases[..3].to_vec();
        for (index, trace) in traces.iter_mut().enumerate() {
            trace.initial_observation_digest = "shared-visible-prefix".into();
            trace.events[0].action_label = format!("Visible action {index}");
            cases[index].canonical_cause = format!("private-cause-{index}");
        }
        let forward = counterfactual_audit(&traces, &cases);
        traces.reverse();
        cases.reverse();
        let reversed = counterfactual_audit(&traces, &cases);
        assert_eq!(forward.comparisons, 3);
        assert_eq!(forward.divergent_comparisons, 3);
        assert_eq!(forward.comparisons, reversed.comparisons);
        assert_eq!(
            forward.fingerprint_divergence_rate,
            reversed.fingerprint_divergence_rate
        );
    }

    #[test]
    fn artifact_byte_budgets_cover_pretty_json_and_markdown() {
        let limits = EvalLimits {
            max_cases: 2,
            max_output_bytes: 64,
            max_total_output_bytes: 192,
            ..EvalLimits::default()
        };
        assert!(
            evaluate_cases(&golden_suite(5, 1), &mut ScriptedPolicy::default(), &limits)
                .unwrap_err()
                .contains("per-artifact")
        );

        let limits = EvalLimits {
            max_cases: 2,
            max_output_bytes: 100 * 1024 * 1024,
            max_total_output_bytes: 128,
            ..EvalLimits::default()
        };
        assert!(
            evaluate_cases(&golden_suite(5, 1), &mut ScriptedPolicy::default(), &limits)
                .unwrap_err()
                .contains("total output")
        );
    }

    #[test]
    fn privacy_canaries_fail_closed() {
        let bundle = evaluate_cases(
            &golden_suite(5, 1),
            &mut ScriptedPolicy::default(),
            &EvalLimits::default(),
        )
        .unwrap();
        let leaked = format!(
            "{{\"secret\":\"{}\"}}",
            bundle.developer.cases[0].canonical_case_id
        );
        assert!(privacy_audit(leaked.as_bytes(), &bundle.developer.cases).is_err());
    }

    struct SecretErrorPolicy;

    impl QuestPolicy for SecretErrorPolicy {
        fn name(&self) -> &str {
            "secret-error-fixture"
        }

        fn decide(&mut self, _frame: &super::super::PlayerFrame) -> Result<PolicyDecision, String> {
            Err("provider failed at https://secret.internal.example/v1?token=CANARY".into())
        }
    }

    #[test]
    fn public_policy_errors_are_typed_and_do_not_echo_provider_details() {
        let bundle = evaluate_cases(
            &golden_suite(5, 1),
            &mut SecretErrorPolicy,
            &EvalLimits::default(),
        )
        .unwrap();
        let public = String::from_utf8(bundle.artifacts.public_json).unwrap();
        assert!(!public.contains("secret.internal"));
        assert!(!public.contains("CANARY"));
        assert_eq!(
            bundle.public.traces[0].termination_error,
            Some(TerminationErrorCode::PolicyFailure)
        );
    }

    struct InvalidClassificationPolicy;

    impl QuestPolicy for InvalidClassificationPolicy {
        fn name(&self) -> &str {
            "invalid-classification-fixture"
        }

        fn decide(&mut self, frame: &super::super::PlayerFrame) -> Result<PolicyDecision, String> {
            ScriptedPolicy::default().decide(frame)
        }

        fn classify(
            &mut self,
            _frame: &super::super::PlayerFrame,
        ) -> Result<PolicyClassification, String> {
            Ok(PolicyClassification {
                template_guess: Some("X".repeat(65)),
                threat_guess: None,
                confidence_percent: Some(101),
            })
        }
    }

    #[test]
    fn invalid_classifications_fail_before_publication() {
        assert!(
            evaluate_cases(
                &golden_suite(5, 1),
                &mut InvalidClassificationPolicy,
                &EvalLimits::default()
            )
            .is_err()
        );
        assert!(
            PolicyClassification {
                template_guess: Some("ValidButUppercase".into()),
                threat_guess: None,
                confidence_percent: Some(50),
            }
            .validate()
            .is_err()
        );
    }
}
