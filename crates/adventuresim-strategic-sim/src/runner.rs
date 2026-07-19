use crate::{
    AgentProfile, FORMAT_VERSION, Objective, ParetoPoint, SimulationConfig, generate_profile,
    nondominated,
};
use adventuresim_core::{
    attribute::{LimbAttribute, PlayerAttributes, SimpleAttribute},
    body::{BodyPart, LimbWeights},
    skill::{PlayerSkills, Skill},
    strategic_schedule::*,
    strategic_time::MINUTES_PER_DAY,
};
use adventuresim_world_schema::OfficialReligion;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("invariant failed: {message}; recent trace: {recent_trace:?}")]
    Invariant {
        message: String,
        recent_trace: Vec<DecisionEvent>,
    },
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub const MAX_INITIAL_SKILL_HOURS: f32 = 1_000_000.0;

/// Observation/intent seam for a future reducer-backed implementation. The native
/// backend currently supports settlement downtime only; no quest model is invented here.
pub trait StrategicBackend {
    type State;
    fn observe(&self, state: &Self::State) -> SettlementObservation;
    fn apply_intent(
        &self,
        state: &mut Self::State,
        intent: StrategicIntent,
    ) -> Result<DayResult, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementObservation {
    pub day: u32,
    pub gold: u32,
    pub notoriety: f32,
    pub skills: SkillHours,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicIntent {
    FollowDowntimeScheduleOneDay,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DayResult {
    pub gold_earned: u32,
    pub notoriety_gained: f32,
    pub risk_exposure: f32,
}

#[derive(Clone, Debug)]
pub struct NativeAgentState {
    profile: AgentProfile,
    day: u32,
    gold: u32,
    notoriety: f32,
    skills: SkillHours,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSettlementBackend {
    pub population_scale: f32,
}

impl StrategicBackend for NativeSettlementBackend {
    type State = NativeAgentState;
    fn observe(&self, state: &Self::State) -> SettlementObservation {
        SettlementObservation {
            day: state.day,
            gold: state.gold,
            notoriety: state.notoriety,
            skills: state.skills,
        }
    }
    fn apply_intent(
        &self,
        state: &mut Self::State,
        _: StrategicIntent,
    ) -> Result<DayResult, String> {
        let before = state.day;
        apply_schedule_training(
            &mut state.skills,
            state.profile.schedule,
            MINUTES_PER_DAY,
            ActivityTrainingProfile::default(),
        );
        apply_religion_training(
            &mut state.skills.religion,
            state.profile.schedule.religions,
            MINUTES_PER_DAY,
            None,
            state.profile.schedule.prayer,
        );
        let checks = Checks {
            attrs: &state.profile.attributes,
            skills: state.skills,
        };
        let outcome = settlement_activity_outcome(
            state.profile.schedule,
            MINUTES_PER_DAY,
            ActivityOutcomeInputs {
                strength_check: checks.strength(),
                endurance_check: state.profile.attributes.endurance,
                stealth_check: checks.skill(Skill::Stealth),
                combat_check: 0.0,
                population_scale: self.population_scale,
            },
        );
        state.gold = state
            .gold
            .checked_add(outcome.gold_earned)
            .ok_or("gold overflow")?;
        state.notoriety += outcome.notoriety_gained;
        state.day = state.day.checked_add(1).ok_or("day overflow")?;
        if state.day <= before {
            return Err("time did not advance".into());
        }
        Ok(DayResult {
            gold_earned: outcome.gold_earned,
            notoriety_gained: outcome.notoriety_gained,
            risk_exposure: outcome.thievery_discovery_chance + outcome.raiding_retaliation_chance,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunManifest {
    pub version: u32,
    pub config: SimulationConfig,
    pub profiles: Vec<AgentProfile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionEvent {
    pub sequence: u64,
    pub day: u32,
    pub agent_id: u32,
    pub intent: StrategicIntent,
    pub gold_earned: u32,
    pub notoriety_gained: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub day: u32,
    pub agent_id: u32,
    pub gold: u32,
    pub notoriety: f32,
    pub total_skill_hours: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    Completed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMetrics {
    pub agent_id: u32,
    pub wealth: u32,
    pub skill_hours: SkillHours,
    pub total_skill_hours_gained: f64,
    pub activity_minutes: u64,
    pub leisure_minutes: u64,
    pub notoriety: f32,
    pub cumulative_risk_exposure: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportMetric {
    Wealth,
    SkillHoursGained,
    Notoriety,
    RiskExposure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParetoObjective {
    pub metric: ReportMetric,
    pub direction: Objective,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportFrontier {
    pub objectives: Vec<ParetoObjective>,
    pub agent_ids: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationReport {
    pub version: u32,
    pub manifest: RunManifest,
    pub trace: Vec<DecisionEvent>,
    pub trace_truncated: bool,
    pub snapshots: Vec<Snapshot>,
    pub snapshots_truncated: bool,
    pub terminal_reason: TerminalReason,
    pub metrics: Vec<AgentMetrics>,
    pub pareto_frontier: ReportFrontier,
    pub canonical_digest: String,
}

pub fn run(config: SimulationConfig) -> Result<SimulationReport, SimulationError> {
    config.validate().map_err(SimulationError::InvalidConfig)?;
    let profiles = (0..config.population)
        .map(|id| generate_profile(config.seed, id))
        .collect();
    run_manifest(RunManifest {
        version: FORMAT_VERSION,
        config,
        profiles,
    })
}

pub fn run_profiles(
    config: SimulationConfig,
    profiles: Vec<AgentProfile>,
) -> Result<SimulationReport, SimulationError> {
    config.validate().map_err(SimulationError::InvalidConfig)?;
    if profiles.len() != config.population as usize {
        return Err(SimulationError::InvalidConfig(
            "profile count differs from population".into(),
        ));
    }
    run_manifest(RunManifest {
        version: FORMAT_VERSION,
        config,
        profiles,
    })
}

pub fn replay(manifest: RunManifest) -> Result<SimulationReport, SimulationError> {
    run_manifest(manifest)
}

fn run_manifest(mut manifest: RunManifest) -> Result<SimulationReport, SimulationError> {
    manifest
        .config
        .validate()
        .map_err(SimulationError::InvalidConfig)?;
    if manifest.version != FORMAT_VERSION {
        return Err(SimulationError::InvalidConfig(
            "unsupported manifest version".into(),
        ));
    }
    if manifest.profiles.len() != manifest.config.population as usize {
        return Err(SimulationError::InvalidConfig(
            "profile count differs from population".into(),
        ));
    }
    manifest.profiles.sort_by_key(|p| p.agent_id);
    for (expected, profile) in manifest.profiles.iter().enumerate() {
        if profile.agent_id as usize != expected {
            return Err(SimulationError::InvalidConfig(
                "agent ids must be contiguous and canonical".into(),
            ));
        }
        if profile.schedule.allocated_minutes() > MINUTES_PER_DAY {
            return Err(SimulationError::InvalidConfig(format!(
                "agent {} schedule exceeds 1440 minutes",
                profile.agent_id
            )));
        }
        if profile.schedule.raiding > 0 {
            return Err(SimulationError::InvalidConfig(format!(
                "agent {} assigns raiding minutes, but native raiding execution is unsupported until equipped capabilities are authoritative",
                profile.agent_id
            )));
        }
        if profile.schedule.religion_auto_train || profile.schedule.religion > 0 {
            return Err(SimulationError::InvalidConfig(format!(
                "agent {} uses automatic Religion training; simulator profiles require explicit per-tradition allocations",
                profile.agent_id
            )));
        }
        validate_profile(profile).map_err(SimulationError::InvalidConfig)?;
    }
    let config = manifest.config.clone();
    let backend = NativeSettlementBackend {
        population_scale: config.population_scale,
    };
    let mut states: Vec<_> = manifest
        .profiles
        .iter()
        .cloned()
        .map(|profile| NativeAgentState {
            skills: profile.initial_skills,
            profile,
            day: 0,
            gold: 100,
            notoriety: 0.0,
        })
        .collect();
    let mut trace = Vec::with_capacity((config.max_trace_events as usize).min(10_000));
    let mut recent = VecDeque::with_capacity(8);
    let mut snapshots = Vec::new();
    let mut risks = vec![0.0_f32; states.len()];
    let mut sequence = 0_u64;
    for day in 0..config.days {
        for state in &mut states {
            // canonical ascending agent id
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| invariant("decision sequence overflow", &recent))?;
            if sequence > config.max_decisions {
                return Err(invariant("decision cap reached", &recent));
            }
            let result = backend
                .apply_intent(state, StrategicIntent::FollowDowntimeScheduleOneDay)
                .map_err(|message| invariant(message, &recent))?;
            let event = DecisionEvent {
                sequence,
                day,
                agent_id: state.profile.agent_id,
                intent: StrategicIntent::FollowDowntimeScheduleOneDay,
                gold_earned: result.gold_earned,
                notoriety_gained: result.notoriety_gained,
            };
            if recent.len() == 8 {
                recent.pop_front();
            }
            recent.push_back(event.clone());
            if trace.len() < config.max_trace_events as usize {
                trace.push(event);
            }
            risks[state.profile.agent_id as usize] += result.risk_exposure;
            let obs = backend.observe(state);
            if !obs.notoriety.is_finite()
                || !obs.skills.is_finite()
                || !risks[state.profile.agent_id as usize].is_finite()
            {
                return Err(invariant("nonfinite state or metrics", &recent));
            }
        }
        let at_interval = (day + 1) % config.snapshot_interval_days == 0 || day + 1 == config.days;
        if at_interval {
            for state in &states {
                if snapshots.len() < config.max_snapshots as usize {
                    snapshots.push(Snapshot {
                        day: day + 1,
                        agent_id: state.profile.agent_id,
                        gold: state.gold,
                        notoriety: state.notoriety,
                        total_skill_hours: total_skill_hours(state.skills),
                    });
                }
            }
        }
    }
    let metrics = states
        .iter()
        .map(|state| {
            let activity_daily = [
                state.profile.schedule.labor,
                state.profile.schedule.prayer,
                state.profile.schedule.thievery,
                state.profile.schedule.raiding,
            ]
            .into_iter()
            .map(u64::from)
            .sum::<u64>();
            let allocated = state.profile.schedule.allocated_minutes();
            AgentMetrics {
                agent_id: state.profile.agent_id,
                wealth: state.gold,
                skill_hours: state.skills,
                total_skill_hours_gained: total_skill_hours(state.skills)
                    - total_skill_hours(state.profile.initial_skills),
                activity_minutes: activity_daily * u64::from(config.days),
                leisure_minutes: (MINUTES_PER_DAY - allocated) * u64::from(config.days),
                notoriety: state.notoriety,
                cumulative_risk_exposure: risks[state.profile.agent_id as usize],
            }
        })
        .collect::<Vec<_>>();
    let pareto_frontier = report_frontier(&metrics)?;
    let trace_truncated = sequence > u64::from(config.max_trace_events);
    let expected_snapshots = ((config.days - 1) / config.snapshot_interval_days + 1) as u64
        * u64::from(config.population);
    let mut report = SimulationReport {
        version: FORMAT_VERSION,
        manifest,
        trace,
        trace_truncated,
        snapshots,
        snapshots_truncated: expected_snapshots > u64::from(config.max_snapshots),
        terminal_reason: TerminalReason::Completed,
        metrics,
        pareto_frontier,
        canonical_digest: String::new(),
    };
    report.canonical_digest = digest(&report)?;
    Ok(report)
}

pub fn digest(report: &SimulationReport) -> Result<String, serde_json::Error> {
    validate_report(report).map_err(|message| {
        serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        ))
    })?;
    let mut canonical = report.clone();
    canonical.canonical_digest.clear();
    quantize_canonical_floats(&mut canonical);
    Ok(blake3::hash(&serde_json::to_vec(&canonical)?)
        .to_hex()
        .to_string())
}

const DIGEST_DECIMAL_SCALE: f64 = 10_000.0;

fn q32(value: f32) -> f32 {
    ((f64::from(value) * DIGEST_DECIMAL_SCALE).round() / DIGEST_DECIMAL_SCALE) as f32
}

fn q64(value: f64) -> f64 {
    (value * DIGEST_DECIMAL_SCALE).round() / DIGEST_DECIMAL_SCALE
}

/// Quantization affects only the canonical digest view. Stored metrics retain
/// their full precision while sub-ULP JSON/platform differences hash identically.
fn quantize_canonical_floats(report: &mut SimulationReport) {
    report.manifest.config.population_scale = q32(report.manifest.config.population_scale);
    for profile in &mut report.manifest.profiles {
        let a = &mut profile.attributes;
        a.endurance = q32(a.endurance);
        a.immunity = q32(a.immunity);
        a.gut = q32(a.gut);
        a.precision = q32(a.precision);
        a.intelligence = q32(a.intelligence);
        a.instinct = q32(a.instinct);
        a.eyesight = q32(a.eyesight);
        a.hearing = q32(a.hearing);
        a.left_arm_strength = q32(a.left_arm_strength);
        a.right_arm_strength = q32(a.right_arm_strength);
        a.left_leg_strength = q32(a.left_leg_strength);
        a.right_leg_strength = q32(a.right_leg_strength);
        a.left_arm_agility = q32(a.left_arm_agility);
        a.right_arm_agility = q32(a.right_arm_agility);
        a.left_leg_agility = q32(a.left_leg_agility);
        a.right_leg_agility = q32(a.right_leg_agility);
        quantize_skills(&mut profile.initial_skills);
        profile.activity_vs_quest_propensity = q32(profile.activity_vs_quest_propensity);
        profile.risk_tolerance = q32(profile.risk_tolerance);
        profile.recovery_health_threshold = q32(profile.recovery_health_threshold);
        profile.equipment.protection_weight = q32(profile.equipment.protection_weight);
        profile.equipment.mobility_weight = q32(profile.equipment.mobility_weight);
        profile.equipment.price_weight = q32(profile.equipment.price_weight);
        profile.equipment.reach_weight = q32(profile.equipment.reach_weight);
        profile.spending_propensity = q32(profile.spending_propensity);
    }
    for event in &mut report.trace {
        event.notoriety_gained = q32(event.notoriety_gained);
    }
    for snapshot in &mut report.snapshots {
        snapshot.notoriety = q32(snapshot.notoriety);
        snapshot.total_skill_hours = q64(snapshot.total_skill_hours);
    }
    for metric in &mut report.metrics {
        quantize_skills(&mut metric.skill_hours);
        metric.total_skill_hours_gained = q64(metric.total_skill_hours_gained);
        metric.notoriety = q32(metric.notoriety);
        metric.cumulative_risk_exposure = q32(metric.cumulative_risk_exposure);
    }
}

fn quantize_skills(skills: &mut SkillHours) {
    skills.melee = q32(skills.melee);
    skills.dodge = q32(skills.dodge);
    skills.block = q32(skills.block);
    skills.ranged = q32(skills.ranged);
    skills.will = q32(skills.will);
    skills.charisma = q32(skills.charisma);
    skills.medicine = q32(skills.medicine);
    for religion in OfficialReligion::ALL {
        *skills.religion.direct_mut(religion) = q32(skills.religion.direct(religion));
    }
    skills.stealth = q32(skills.stealth);
    skills.balance = q32(skills.balance);
    skills.surgeon = q32(skills.surgeon);
}

fn total_skill_hours(skills: SkillHours) -> f64 {
    skills.values().into_iter().map(f64::from).sum()
}

fn frontier_objectives() -> Vec<ParetoObjective> {
    vec![
        ParetoObjective {
            metric: ReportMetric::Wealth,
            direction: Objective::Maximize,
        },
        ParetoObjective {
            metric: ReportMetric::SkillHoursGained,
            direction: Objective::Maximize,
        },
        ParetoObjective {
            metric: ReportMetric::Notoriety,
            direction: Objective::Minimize,
        },
        ParetoObjective {
            metric: ReportMetric::RiskExposure,
            direction: Objective::Minimize,
        },
    ]
}

fn report_frontier(metrics: &[AgentMetrics]) -> Result<ReportFrontier, SimulationError> {
    let objectives = frontier_objectives();
    let points = metrics
        .iter()
        .map(|metric| ParetoPoint {
            id: metric.agent_id,
            values: vec![
                f64::from(metric.wealth),
                q64(metric.total_skill_hours_gained),
                f64::from(q32(metric.notoriety)),
                f64::from(q32(metric.cumulative_risk_exposure)),
            ],
        })
        .collect::<Vec<_>>();
    let directions = objectives
        .iter()
        .map(|objective| objective.direction)
        .collect::<Vec<_>>();
    let agent_ids = nondominated(&points, &directions).map_err(|error| {
        SimulationError::InvalidConfig(format!("Pareto analysis failed: {error}"))
    })?;
    Ok(ReportFrontier {
        objectives,
        agent_ids,
    })
}

/// Validate all report-controlled vector and numeric bounds before canonical hashing.
pub fn validate_report(report: &SimulationReport) -> Result<(), String> {
    if report.version != FORMAT_VERSION || report.manifest.version != FORMAT_VERSION {
        return Err("unsupported report or manifest version".into());
    }
    report.manifest.config.validate()?;
    let config = &report.manifest.config;
    if report.manifest.profiles.len() != config.population as usize
        || report.metrics.len() != config.population as usize
    {
        return Err("report population vector length mismatch".into());
    }
    if report.trace.len() > config.max_trace_events as usize
        || report.trace.len() > crate::MAX_TRACE_EVENTS as usize
        || report.snapshots.len() > config.max_snapshots as usize
        || report.snapshots.len() > crate::MAX_SNAPSHOTS as usize
        || report.pareto_frontier.agent_ids.len() > config.population as usize
    {
        return Err("report vector exceeds configured or global bounds".into());
    }
    if report.pareto_frontier.objectives != frontier_objectives() {
        return Err("report Pareto objectives do not match format contract".into());
    }
    for (expected, profile) in report.manifest.profiles.iter().enumerate() {
        if profile.agent_id as usize != expected
            || profile.schedule.allocated_minutes() > MINUTES_PER_DAY
        {
            return Err("report profile order or schedule allocation is invalid".into());
        }
        validate_profile(profile)?;
        if profile.schedule.raiding > 0 {
            return Err("report contains unsupported raiding schedule".into());
        }
    }
    if report
        .trace
        .iter()
        .any(|event| !event.notoriety_gained.is_finite())
        || report.snapshots.iter().any(|snapshot| {
            !snapshot.notoriety.is_finite() || !snapshot.total_skill_hours.is_finite()
        })
        || report.metrics.iter().any(|metric| {
            !metric.skill_hours.is_finite()
                || !metric.total_skill_hours_gained.is_finite()
                || !metric.notoriety.is_finite()
                || !metric.cumulative_risk_exposure.is_finite()
        })
    {
        return Err("report contains nonfinite canonical metrics".into());
    }
    if report
        .pareto_frontier
        .agent_ids
        .iter()
        .any(|id| *id >= config.population)
    {
        return Err("report Pareto frontier contains an invalid agent id".into());
    }
    Ok(())
}

fn invariant(message: impl Into<String>, recent: &VecDeque<DecisionEvent>) -> SimulationError {
    SimulationError::Invariant {
        message: message.into(),
        recent_trace: recent.iter().cloned().collect(),
    }
}

fn validate_profile(p: &AgentProfile) -> Result<(), String> {
    if !(2..=4).contains(&p.personality.non_neutral_count()) {
        return Err(format!(
            "agent {} personality must have 2..=4 active axes",
            p.agent_id
        ));
    }
    if p.build.activity_only != (p.personality.drive == crate::Drive::Content) {
        return Err(format!(
            "agent {} activity-only build disagrees with personality",
            p.agent_id
        ));
    }
    if p.build.role == crate::BuildRole::FrontLine {
        let arm_strength = (p.attributes.left_arm_strength + p.attributes.right_arm_strength) * 0.5;
        if p.personality.nerve != crate::Nerve::Brave
            || p.attributes.endurance < 3.0
            || arm_strength < 3.0
        {
            return Err(format!(
                "agent {} has a non-viable front-line build",
                p.agent_id
            ));
        }
    }
    let finite = [
        p.activity_vs_quest_propensity,
        p.risk_tolerance,
        p.recovery_health_threshold,
        p.equipment.protection_weight,
        p.equipment.mobility_weight,
        p.equipment.price_weight,
        p.equipment.reach_weight,
        p.spending_propensity,
    ]
    .into_iter()
    .all(f32::is_finite);
    if !finite || !p.initial_skills.is_finite() {
        return Err(format!(
            "agent {} has nonfinite preference or skill",
            p.agent_id
        ));
    }
    if !(0.0..=1.0).contains(&p.activity_vs_quest_propensity)
        || !(0.0..=1.0).contains(&p.risk_tolerance)
        || !(0.0..=1.0).contains(&p.recovery_health_threshold)
        || !(0.0..=1.0).contains(&p.equipment.protection_weight)
        || !(0.0..=1.0).contains(&p.equipment.mobility_weight)
        || !(0.0..=1.0).contains(&p.equipment.price_weight)
        || !(0.0..=1.0).contains(&p.equipment.reach_weight)
        || !(0.0..=1.0).contains(&p.spending_propensity)
    {
        return Err(format!(
            "agent {} probability or weight outside 0..=1",
            p.agent_id
        ));
    }
    let a = &p.attributes;
    let attributes = [
        a.endurance,
        a.immunity,
        a.gut,
        a.precision,
        a.intelligence,
        a.instinct,
        a.eyesight,
        a.hearing,
        a.left_arm_strength,
        a.right_arm_strength,
        a.left_leg_strength,
        a.right_leg_strength,
        a.left_arm_agility,
        a.right_arm_agility,
        a.left_leg_agility,
        a.right_leg_agility,
    ];
    if attributes
        .into_iter()
        .any(|value| !value.is_finite() || !(0.5..=5.0).contains(&value))
    {
        return Err(format!(
            "agent {} attribute is nonfinite or outside 0.5..=5",
            p.agent_id
        ));
    }
    if p.initial_skills
        .values()
        .into_iter()
        .any(|hours| !(0.0..=MAX_INITIAL_SKILL_HOURS).contains(&hours))
    {
        return Err(format!(
            "agent {} skill hours are outside 0..={MAX_INITIAL_SKILL_HOURS}",
            p.agent_id
        ));
    }
    if !p
        .initial_skills
        .religion
        .direct_fields_valid(MAX_INITIAL_SKILL_HOURS)
    {
        return Err(format!(
            "agent {} Religion hours contain a nonfinite or out-of-range direct field",
            p.agent_id
        ));
    }
    if !total_skill_hours(p.initial_skills).is_finite() {
        return Err(format!("agent {} has a nonfinite skill total", p.agent_id));
    }
    Ok(())
}

struct Checks<'a> {
    attrs: &'a crate::Attributes,
    skills: SkillHours,
}
impl Checks<'_> {
    fn strength(&self) -> f32 {
        self.attrs.limb_attr_by_weight_by_parts(
            LimbAttribute::Strength,
            &adventuresim_core::stub::StubBody,
            LimbWeights::all_equal(),
        )
    }
    fn skill(&self, skill: Skill) -> f32 {
        SimSkills(self.skills).skill_check_by_parts(
            skill,
            self.attrs,
            &adventuresim_core::stub::StubBody,
            &adventuresim_core::stub::StubEssentials,
            &adventuresim_core::stub::StubEquipment,
            LimbWeights::all_equal(),
        )
    }
}

struct SimSkills(SkillHours);
impl PlayerSkills for SimSkills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Melee => self.0.melee,
            Skill::Dodge => self.0.dodge,
            Skill::Block => self.0.block,
            Skill::Ranged => self.0.ranged,
            Skill::Will => self.0.will,
            Skill::Charisma => self.0.charisma,
            Skill::Medicine => self.0.medicine,
            Skill::Religion => self.0.religion.maximum_effective(),
            Skill::Stealth => self.0.stealth,
            Skill::Balance => self.0.balance,
            Skill::Surgeon => self.0.surgeon,
            Skill::Smithing => self.0.smithing,
        }
    }
}

impl PlayerAttributes for crate::Attributes {
    fn raw_limb_attr(&self, attr: LimbAttribute, limb: BodyPart) -> f32 {
        match (attr, limb) {
            (LimbAttribute::Strength, BodyPart::LeftArm) => self.left_arm_strength,
            (LimbAttribute::Strength, BodyPart::RightArm) => self.right_arm_strength,
            (LimbAttribute::Strength, BodyPart::LeftLeg) => self.left_leg_strength,
            (LimbAttribute::Strength, BodyPart::RightLeg) => self.right_leg_strength,
            (LimbAttribute::Agility, BodyPart::LeftArm) => self.left_arm_agility,
            (LimbAttribute::Agility, BodyPart::RightArm) => self.right_arm_agility,
            (LimbAttribute::Agility, BodyPart::LeftLeg) => self.left_leg_agility,
            (LimbAttribute::Agility, BodyPart::RightLeg) => self.right_leg_agility,
            _ => 0.0,
        }
    }
    fn raw_single_body_part_attr(&self, attr: SimpleAttribute) -> f32 {
        match attr {
            SimpleAttribute::Endurance => self.endurance,
            SimpleAttribute::Immunity => self.immunity,
            SimpleAttribute::Gut => self.gut,
            SimpleAttribute::Intelligence => self.intelligence,
            SimpleAttribute::Instinct => self.instinct,
            SimpleAttribute::Eyesight => self.eyesight,
            SimpleAttribute::Hearing => self.hearing,
        }
    }

    fn raw_precision(&self) -> f32 {
        self.precision
    }

    fn has_dedicated_precision(&self) -> bool {
        true
    }
}

pub fn human_summary(report: &SimulationReport) -> String {
    let n = report.metrics.len().max(1) as f64;
    let wealth = report
        .metrics
        .iter()
        .map(|m| f64::from(m.wealth))
        .sum::<f64>()
        / n;
    let notoriety = report
        .metrics
        .iter()
        .map(|m| f64::from(m.notoriety))
        .sum::<f64>()
        / n;
    format!(
        "{} agents for {} days; mean wealth {:.1}; mean notoriety {:.2}; Pareto frontier {:?}; digest {}",
        report.metrics.len(),
        report.manifest.config.days,
        wealth,
        notoriety,
        report.pareto_frontier.agent_ids,
        report.canonical_digest
    )
}
