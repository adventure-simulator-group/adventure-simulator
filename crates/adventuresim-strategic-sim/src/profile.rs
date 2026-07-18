use crate::rng::{StableRng, sub_seed};
use adventuresim_core::strategic_schedule::{DailySchedule, SkillHours};
use serde::{Deserialize, Serialize};

const PROFILE_DOMAIN: u64 = 0x5052_4f46_494c_4501;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityPreference {
    Labor,
    Prayer,
    Thievery,
    Raiding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentStyle {
    Unarmored,
    Light,
    Heavy,
    Ranged,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentPreferences {
    pub style: EquipmentStyle,
    pub protection_weight: f32,
    pub mobility_weight: f32,
    pub price_weight: f32,
    pub reach_weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProfile {
    pub agent_id: u32,
    pub profile_seed: u64,
    pub attributes: Attributes,
    pub initial_skills: SkillHours,
    pub schedule: DailySchedule,
    pub preferred_activity: ActivityPreference,
    pub activity_vs_quest_propensity: f32,
    pub risk_tolerance: f32,
    pub recovery_health_threshold: f32,
    pub equipment: EquipmentPreferences,
    pub provision_days_target: u16,
    pub cash_reserve_target: u32,
    pub spending_propensity: f32,
}

fn bounded(base: f32, spread: f32, rng: &mut StableRng) -> f32 {
    (base + rng.range(-spread, spread)).clamp(0.5, 5.0)
}

pub fn generate_profile(seed: u64, agent_id: u32) -> AgentProfile {
    let profile_seed = sub_seed(seed, PROFILE_DOMAIN, u64::from(agent_id));
    let mut rng = StableRng::new(profile_seed);
    // Shared latent factors create plausible correlations while limb-specific noise
    // prevents profiles from being merely scalar copies of one another.
    let physique = rng.range(1.3, 4.4);
    let coordination = rng.range(1.2, 4.5);
    let cognition = rng.range(1.0, 4.6);
    let resilience = rng.range(1.0, 4.6);
    let attributes = Attributes {
        endurance: bounded((physique + resilience) * 0.5, 0.35, &mut rng),
        immunity: bounded(resilience, 0.45, &mut rng),
        gut: bounded(resilience, 0.5, &mut rng),
        precision: bounded(coordination, 0.4, &mut rng),
        intelligence: bounded(cognition, 0.4, &mut rng),
        instinct: bounded((cognition + coordination) * 0.5, 0.4, &mut rng),
        eyesight: bounded(coordination, 0.6, &mut rng),
        hearing: bounded(coordination, 0.6, &mut rng),
        left_arm_strength: bounded(physique, 0.35, &mut rng),
        right_arm_strength: bounded(physique, 0.35, &mut rng),
        left_leg_strength: bounded(physique + 0.2, 0.35, &mut rng),
        right_leg_strength: bounded(physique + 0.2, 0.35, &mut rng),
        left_arm_agility: bounded(coordination, 0.35, &mut rng),
        right_arm_agility: bounded(coordination, 0.35, &mut rng),
        left_leg_agility: bounded(coordination, 0.35, &mut rng),
        right_leg_agility: bounded(coordination, 0.35, &mut rng),
    };
    let preferred_activity = match rng.next_u64() % 4 {
        0 => ActivityPreference::Labor,
        1 => ActivityPreference::Prayer,
        2 => ActivityPreference::Thievery,
        _ => ActivityPreference::Raiding,
    };
    let style = match rng.next_u64() % 4 {
        0 => EquipmentStyle::Unarmored,
        1 => EquipmentStyle::Light,
        2 => EquipmentStyle::Heavy,
        _ => EquipmentStyle::Ranged,
    };
    let schedule = generated_schedule(&mut rng, preferred_activity);
    let initial = |rng: &mut StableRng| rng.range(200.0, 2_000.0);
    AgentProfile {
        agent_id,
        profile_seed,
        attributes,
        initial_skills: SkillHours {
            melee: initial(&mut rng),
            dodge: initial(&mut rng),
            block: initial(&mut rng),
            ranged: initial(&mut rng),
            will: initial(&mut rng),
            charisma: initial(&mut rng),
            medicine: initial(&mut rng),
            faith: initial(&mut rng),
            stealth: initial(&mut rng),
            balance: initial(&mut rng),
            surgeon: initial(&mut rng),
        },
        schedule,
        preferred_activity,
        activity_vs_quest_propensity: rng.unit(),
        risk_tolerance: rng.unit(),
        recovery_health_threshold: rng.range(0.55, 0.95),
        equipment: EquipmentPreferences {
            style,
            protection_weight: rng.unit(),
            mobility_weight: rng.unit(),
            price_weight: rng.unit(),
            reach_weight: rng.unit(),
        },
        provision_days_target: 1 + (rng.next_u64() % 31) as u16,
        cash_reserve_target: (rng.next_u64() % 501) as u32,
        spending_propensity: rng.unit(),
    }
}

fn generated_schedule(rng: &mut StableRng, preferred: ActivityPreference) -> DailySchedule {
    // Ten-minute units make profiles readable and keep allocation exact.
    let activity_minutes = 240 + (rng.next_u64() % 49) as u16 * 10;
    let training_minutes = 120 + (rng.next_u64() % 37) as u16 * 10;
    let mut s = DailySchedule::default();
    match preferred {
        ActivityPreference::Labor => s.labor = activity_minutes,
        ActivityPreference::Prayer => s.prayer = activity_minutes,
        ActivityPreference::Thievery => s.thievery = activity_minutes,
        ActivityPreference::Raiding => s.raiding = activity_minutes,
    }
    match rng.next_u64() % 11 {
        0 => s.melee = training_minutes,
        1 => s.dodge = training_minutes,
        2 => s.block = training_minutes,
        3 => s.ranged = training_minutes,
        4 => s.will = training_minutes,
        5 => s.charisma = training_minutes,
        6 => s.medicine = training_minutes,
        7 => s.faith = training_minutes,
        8 => s.stealth = training_minutes,
        9 => s.balance = training_minutes,
        _ => s.surgeon = training_minutes,
    }
    s
}

/// A matched pair preserves the generated profile and circumstances, changing
/// only the named activity preference and its schedule allocation.
pub fn matched_activity_pair(
    seed: u64,
    agent_id: u32,
    left: ActivityPreference,
    right: ActivityPreference,
) -> (AgentProfile, AgentProfile) {
    let mut a = generate_profile(seed, agent_id);
    set_activity(&mut a, left);
    let mut b = a.clone();
    set_activity(&mut b, right);
    (a, b)
}

fn set_activity(profile: &mut AgentProfile, preference: ActivityPreference) {
    let minutes = profile.schedule.labor
        + profile.schedule.prayer
        + profile.schedule.thievery
        + profile.schedule.raiding;
    profile.schedule.labor = 0;
    profile.schedule.prayer = 0;
    profile.schedule.thievery = 0;
    profile.schedule.raiding = 0;
    match preference {
        ActivityPreference::Labor => profile.schedule.labor = minutes,
        ActivityPreference::Prayer => profile.schedule.prayer = minutes,
        ActivityPreference::Thievery => profile.schedule.thievery = minutes,
        ActivityPreference::Raiding => profile.schedule.raiding = minutes,
    }
    profile.preferred_activity = preference;
}
