//! Quest-coverage evidence, validation, and failure artifact writing.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestCoverageEvidence {
    pub direct_contract_id: String,
    pub generated_case_id: String,
    pub direct_leader_id: u64,
    pub generated_leader_id: u64,
    pub direct_party_id: String,
    pub generated_party_id: String,
    pub direct_accepted: bool,
    pub direct_traveled: bool,
    pub direct_encountered: bool,
    pub direct_reported: bool,
    pub direct_safely_abandoned: bool,
    pub generated_intake: bool,
    pub generated_discovered: bool,
    pub generated_completed: bool,
}

/// Strict acceptance contract for the deterministic two-party quest fixture.
/// Each error names the first unmet metric so CI output is actionable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestCoverageMetric {
    DuplicateSemanticEvents,
    EncounterWipes,
    FinalAgentsAlive,
    FinalAgentsNotCritical,
    FinalAgentsNotStranded,
    FixtureDirectAccepted,
    FixtureDirectEncountered,
    FixtureDirectReported,
    FixtureDirectTraveled,
    FixtureGeneratedCompleted,
    FixtureGeneratedDiscovered,
    FixtureGeneratedIntake,
    FixtureProvenance,
    FixtureSuccessfulCompletion,
    QuestsAttempted,
    QuestsAttemptedConsistency,
    ReducerFailures,
    StuckDetections,
}

impl QuestCoverageMetric {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateSemanticEvents => "duplicate_semantic_events",
            Self::EncounterWipes => "encounter_wipes",
            Self::FinalAgentsAlive => "final_agents_alive",
            Self::FinalAgentsNotCritical => "final_agents_not_critical",
            Self::FinalAgentsNotStranded => "final_agents_not_stranded",
            Self::FixtureDirectAccepted => "fixture_direct_accepted",
            Self::FixtureDirectEncountered => "fixture_direct_encountered",
            Self::FixtureDirectReported => "fixture_direct_reported",
            Self::FixtureDirectTraveled => "fixture_direct_traveled",
            Self::FixtureGeneratedCompleted => "fixture_generated_completed",
            Self::FixtureGeneratedDiscovered => "fixture_generated_discovered",
            Self::FixtureGeneratedIntake => "fixture_generated_intake",
            Self::FixtureProvenance => "fixture_provenance",
            Self::FixtureSuccessfulCompletion => "fixture_successful_completion",
            Self::QuestsAttempted => "quests_attempted",
            Self::QuestsAttemptedConsistency => "quests_attempted_consistency",
            Self::ReducerFailures => "reducer_failures",
            Self::StuckDetections => "stuck_detections",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestCoverageFailure {
    metric: QuestCoverageMetric,
}

impl QuestCoverageFailure {
    const fn new(metric: QuestCoverageMetric) -> Self {
        Self { metric }
    }

    pub const fn metric(self) -> QuestCoverageMetric {
        self.metric
    }
}

impl std::fmt::Display for QuestCoverageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "quest coverage acceptance failed: metric={}",
            self.metric.as_str()
        )
    }
}

impl std::error::Error for QuestCoverageFailure {}

pub fn validate_quest_coverage(report: &CoreLoopReport) -> Result<(), QuestCoverageFailure> {
    let metrics = &report.metrics;
    let coverage = report
        .quest_coverage
        .as_ref()
        .ok_or_else(|| QuestCoverageFailure::new(QuestCoverageMetric::FixtureProvenance))?;
    let checks = [
        (
            QuestCoverageMetric::ReducerFailures,
            metrics.reducer_failures == 0,
        ),
        (
            QuestCoverageMetric::DuplicateSemanticEvents,
            metrics.duplicate_semantic_events == 0,
        ),
        (
            QuestCoverageMetric::StuckDetections,
            metrics.stuck_detections == 0,
        ),
        (
            QuestCoverageMetric::EncounterWipes,
            metrics.encounter_wipes == 0,
        ),
        (
            QuestCoverageMetric::FixtureDirectAccepted,
            coverage.direct_accepted,
        ),
        (
            QuestCoverageMetric::FixtureDirectTraveled,
            coverage.direct_traveled,
        ),
        (
            QuestCoverageMetric::FixtureDirectEncountered,
            coverage.direct_encountered,
        ),
        (
            QuestCoverageMetric::FixtureDirectReported,
            coverage.direct_reported || coverage.direct_safely_abandoned,
        ),
        (
            QuestCoverageMetric::FixtureGeneratedIntake,
            coverage.generated_intake,
        ),
        (
            QuestCoverageMetric::FixtureGeneratedDiscovered,
            coverage.generated_discovered,
        ),
        (
            QuestCoverageMetric::FixtureGeneratedCompleted,
            coverage.generated_completed,
        ),
        (
            QuestCoverageMetric::FixtureSuccessfulCompletion,
            coverage.direct_reported || coverage.generated_completed,
        ),
        (
            QuestCoverageMetric::QuestsAttempted,
            metrics.quests_attempted >= 2,
        ),
        (
            QuestCoverageMetric::QuestsAttemptedConsistency,
            metrics.quests_attempted
                == metrics
                    .direct_contracts_attempted
                    .saturating_add(metrics.generated_case_intakes),
        ),
    ];
    if let Some((metric, _)) = checks.into_iter().find(|(_, passed)| !passed) {
        return Err(QuestCoverageFailure::new(metric));
    }
    if report.final_agents.iter().any(|agent| !agent.alive) {
        return Err(QuestCoverageFailure::new(
            QuestCoverageMetric::FinalAgentsAlive,
        ));
    }
    if report.final_agents.iter().any(|agent| agent.critical) {
        return Err(QuestCoverageFailure::new(
            QuestCoverageMetric::FinalAgentsNotCritical,
        ));
    }
    if report.final_agents.iter().any(|agent| {
        agent.settlement_id.is_none()
            || agent.current_case_site_id.is_some()
            || agent.journey_destination.is_some()
    }) {
        return Err(QuestCoverageFailure::new(
            QuestCoverageMetric::FinalAgentsNotStranded,
        ));
    }
    Ok(())
}

/// Persist the same public-safe diagnostic shape used by reducer failures
/// when the completed report fails the stricter quest-coverage contract.
pub fn write_quest_coverage_failure(
    report: &CoreLoopReport,
    path: &Path,
    error: &QuestCoverageFailure,
) -> Result<(), String> {
    let reason_code = error.metric().as_str();
    let (trace, trace_truncated) = bounded_failure_trace(&report.trace, report.total_event_count);
    let final_agents = report
        .final_agents
        .iter()
        .map(|agent| CoreLoopFailureAgent {
            agent_id: agent.agent_id,
            character_id: agent.character_id,
            alive: agent.alive,
            condition_status: agent.condition_status,
            thermal: agent.thermal,
            wetness_bps: agent.wetness_bps,
            thermal_strain: agent.thermal_strain,
            ammunition: agent.ammunition,
            carried_load_kg: agent.carried_load_kg,
            carry_capacity_kg: agent.carry_capacity_kg,
            encumbrance_remaining_bps: agent.encumbrance_remaining_bps,
            equipment_ready: agent.equipment_ready,
            party_tent_quantity: agent.party_tent_quantity,
            hunger: agent.hunger,
            thirst: agent.thirst,
            food_days: agent.food_days,
            water_days: agent.water_days,
            visible_food_kcal: agent.visible_food_kcal,
            visible_water_ml: agent.visible_water_ml,
            personal_gold_coin: agent.personal_gold_coin,
            settlement_id: agent.settlement_id.clone(),
            current_case_site_id: agent.current_case_site_id.clone(),
            journey_destination: agent.journey_destination.clone(),
            symptomatic: agent.symptomatic,
            critical: agent.critical,
            settlement_services: agent.settlement_services.clone(),
            visible_herbalist_quote: agent.visible_herbalist_quote,
            visible_inn_full_board_cost: agent.visible_inn_full_board_cost,
        })
        .collect();
    let artifact = CoreLoopFailureArtifact {
        schema_version: CORE_LOOP_FAILURE_SCHEMA_VERSION,
        category: "quest_coverage_acceptance".into(),
        message: error.to_string(),
        operation: None,
        reason_code: reason_code.into(),
        fixture_disease: report.fixture_disease.clone(),
        metrics: report.metrics.clone(),
        quest_coverage: report.quest_coverage.clone(),
        total_event_count: report.total_event_count,
        trace_truncated,
        trace,
        final_agents,
    };
    let bytes = serde_json::to_vec_pretty(&artifact).map_err(|error| error.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    use std::io::Write as _;
    options
        .open(path)
        .and_then(|mut file| {
            file.write_all(&bytes)?;
            file.write_all(b"\n")
        })
        .map_err(|error| format!("could not write quest coverage diagnostic: {error}"))
}
