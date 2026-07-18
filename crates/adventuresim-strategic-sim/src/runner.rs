use crate::{AgentProfile, EquipmentStyle, FORMAT_VERSION, SimulationConfig, generate_profile};
use adventuresim_core::{
    attribute::{LimbAttribute, PlayerAttributes, SimpleAttribute},
    body::{BodyPart, LimbWeights},
    skill::{PlayerSkills, Skill},
    strategic_schedule::*,
    strategic_time::MINUTES_PER_DAY,
};
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
        let training_profile = training_profile(state.profile.equipment.style);
        apply_schedule_training(
            &mut state.skills,
            state.profile.schedule,
            MINUTES_PER_DAY,
            training_profile,
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
                combat_check: checks.skill(Skill::Melee).max(checks.skill(Skill::Ranged)),
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
    pub total_skill_hours: f32,
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
    pub total_skill_hours_gained: f32,
    pub activity_minutes: u64,
    pub leisure_minutes: u64,
    pub notoriety: f32,
    pub cumulative_risk_exposure: f32,
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
                        total_skill_hours: state.skills.values().into_iter().sum(),
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
                total_skill_hours_gained: state.skills.values().into_iter().sum::<f32>()
                    - state
                        .profile
                        .initial_skills
                        .values()
                        .into_iter()
                        .sum::<f32>(),
                activity_minutes: activity_daily * u64::from(config.days),
                leisure_minutes: (MINUTES_PER_DAY - allocated) * u64::from(config.days),
                notoriety: state.notoriety,
                cumulative_risk_exposure: risks[state.profile.agent_id as usize],
            }
        })
        .collect();
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
        canonical_digest: String::new(),
    };
    report.canonical_digest = digest(&report)?;
    Ok(report)
}

pub fn digest(report: &SimulationReport) -> Result<String, serde_json::Error> {
    let mut canonical = report.clone();
    canonical.canonical_digest.clear();
    Ok(blake3::hash(&serde_json::to_vec(&canonical)?)
        .to_hex()
        .to_string())
}

fn invariant(message: impl Into<String>, recent: &VecDeque<DecisionEvent>) -> SimulationError {
    SimulationError::Invariant {
        message: message.into(),
        recent_trace: recent.iter().cloned().collect(),
    }
}

fn validate_profile(p: &AgentProfile) -> Result<(), String> {
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
        .any(|hours| hours < 0.0)
    {
        return Err(format!("agent {} has negative skill hours", p.agent_id));
    }
    Ok(())
}

fn training_profile(style: EquipmentStyle) -> ActivityTrainingProfile {
    ActivityTrainingProfile {
        raiding_melee: style != EquipmentStyle::Ranged,
        raiding_ranged: style == EquipmentStyle::Ranged,
        raiding_block: style == EquipmentStyle::Heavy,
        raiding_dodge: style != EquipmentStyle::Heavy,
    }
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
            Skill::Faith => self.0.faith,
            Skill::Stealth => self.0.stealth,
            Skill::Balance => self.0.balance,
            Skill::Surgeon => self.0.surgeon,
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
        "{} agents for {} days; mean wealth {:.1}; mean notoriety {:.2}; digest {}",
        report.metrics.len(),
        report.manifest.config.days,
        wealth,
        notoriety,
        report.canonical_digest
    )
}
