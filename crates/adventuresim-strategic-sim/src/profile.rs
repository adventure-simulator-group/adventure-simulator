use crate::rng::{StableRng, sub_seed};
use adventuresim_core::strategic_schedule::{DailySchedule, SkillHours};
use adventuresim_world_schema::{BestiaryHours, ReligionHours};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nerve {
    Neutral,
    Brave,
    Fearful,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Drive {
    Neutral,
    Ambitious,
    Content,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outlook {
    Neutral,
    Sanguine,
    Brooding,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sociability {
    Neutral,
    Gregarious,
    Solitary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conscience {
    Neutral,
    Compassionate,
    Callous,
    Cruel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfRegard {
    Neutral,
    Proud,
    Humble,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conviction {
    Neutral,
    Zealous,
    Irreverent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hygiene {
    Neutral,
    Slovenly,
    Cleanly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Temperance {
    Neutral,
    Temperate,
    Drunkard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    pub nerve: Nerve,
    pub drive: Drive,
    pub outlook: Outlook,
    pub sociability: Sociability,
    pub conscience: Conscience,
    pub self_regard: SelfRegard,
    pub conviction: Conviction,
    pub hygiene: Hygiene,
    pub temperance: Temperance,
}

impl Personality {
    pub fn neutral() -> Self {
        Self {
            nerve: Nerve::Neutral,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Neutral,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
            temperance: Temperance::Neutral,
        }
    }

    pub fn non_neutral_count(&self) -> usize {
        usize::from(self.nerve != Nerve::Neutral)
            + usize::from(self.drive != Drive::Neutral)
            + usize::from(self.outlook != Outlook::Neutral)
            + usize::from(self.sociability != Sociability::Neutral)
            + usize::from(self.conscience != Conscience::Neutral)
            + usize::from(self.self_regard != SelfRegard::Neutral)
            + usize::from(self.conviction != Conviction::Neutral)
            + usize::from(self.hygiene != Hygiene::Neutral)
            + usize::from(self.temperance != Temperance::Neutral)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildRole {
    FrontLine,
    Skirmisher,
    Ranged,
    Healer,
    Devout,
    Civilian,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentBuild {
    pub role: BuildRole,
    pub activity_only: bool,
    pub rationale: String,
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
    pub personality: Personality,
    pub build: AgentBuild,
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
    let personality = generated_personality(&mut rng);
    let build = derive_build(&personality, &attributes);
    let preferred_activity = if personality.conviction == Conviction::Zealous {
        ActivityPreference::Prayer
    } else if matches!(
        personality.conscience,
        Conscience::Callous | Conscience::Cruel
    ) {
        ActivityPreference::Thievery
    } else {
        match rng.next_u64() % 3 {
            0 => ActivityPreference::Labor,
            1 => ActivityPreference::Prayer,
            _ => ActivityPreference::Thievery,
        }
    };
    let style = match build.role {
        BuildRole::FrontLine => EquipmentStyle::Heavy,
        BuildRole::Ranged => EquipmentStyle::Ranged,
        BuildRole::Skirmisher | BuildRole::Healer | BuildRole::Devout => EquipmentStyle::Light,
        BuildRole::Civilian => EquipmentStyle::Unarmored,
    };
    let schedule = generated_schedule(&mut rng, preferred_activity, build.role);
    let initial = |rng: &mut StableRng| rng.range(200.0, 2_000.0);
    let mut initial_skills = SkillHours {
        polearm: initial(&mut rng),
        axe: initial(&mut rng),
        bludgeon: initial(&mut rng),
        sword: initial(&mut rng),
        knife: initial(&mut rng),
        dodge: initial(&mut rng),
        block: initial(&mut rng),
        bow: initial(&mut rng),
        crossbow: initial(&mut rng),
        firearm: initial(&mut rng),
        throw: initial(&mut rng),
        will: initial(&mut rng),
        insight: initial(&mut rng),
        self_awareness: initial(&mut rng),
        humor: initial(&mut rng),
        command: initial(&mut rng),
        deception: initial(&mut rng),
        seduction: initial(&mut rng),
        physiology: initial(&mut rng),
        cooking: initial(&mut rng),
        religion: ReligionHours {
            roman_catholic: initial(&mut rng),
            ..Default::default()
        },
        bestiary: BestiaryHours {
            beast: initial(&mut rng),
            human: initial(&mut rng),
            ..Default::default()
        },
        anatomy: initial(&mut rng),
        stealth: initial(&mut rng),
        balance: initial(&mut rng),
        tailoring: initial(&mut rng),
        smithing: initial(&mut rng),
    };
    let specialty = rng.range(2_500.0, 5_000.0);
    match build.role {
        BuildRole::FrontLine => {
            initial_skills.sword = specialty;
            initial_skills.block = specialty * 0.8;
        }
        BuildRole::Ranged => initial_skills.bow = specialty,
        BuildRole::Skirmisher => {
            initial_skills.dodge = specialty;
            initial_skills.knife = specialty * 0.7;
        }
        BuildRole::Healer => {
            initial_skills.physiology = specialty;
            initial_skills.anatomy = specialty * 0.7;
            initial_skills.knife = specialty * 0.7;
            initial_skills.tailoring = specialty * 0.7;
        }
        BuildRole::Devout => initial_skills.religion.roman_catholic = specialty,
        BuildRole::Civilian => {}
    }
    let quest_propensity = if build.activity_only {
        0.0
    } else {
        let base: f32 = rng.range(0.2, 0.7);
        (base
            + if personality.drive == Drive::Ambitious {
                0.25
            } else {
                0.0
            })
        .clamp(0.0, 1.0)
    };
    let risk_tolerance = (rng.range(0.25, 0.65)
        + if personality.nerve == Nerve::Brave {
            0.2
        } else if personality.nerve == Nerve::Fearful {
            -0.2
        } else {
            0.0
        })
    .clamp(0.0, 1.0);
    AgentProfile {
        agent_id,
        profile_seed,
        attributes,
        personality,
        build,
        initial_skills,
        schedule,
        preferred_activity,
        activity_vs_quest_propensity: quest_propensity,
        risk_tolerance,
        recovery_health_threshold: if risk_tolerance < 0.35 {
            0.9
        } else {
            rng.range(0.65, 0.85)
        },
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

fn generated_schedule(
    rng: &mut StableRng,
    preferred: ActivityPreference,
    role: BuildRole,
) -> DailySchedule {
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
    match role {
        BuildRole::FrontLine | BuildRole::Skirmisher | BuildRole::Ranged => {
            s.combat_training_minutes = training_minutes
        }
        BuildRole::Healer => {
            s.apprenticeship_minutes = training_minutes;
            s.apprenticeship_service_id =
                Some(adventuresim_core::profession::ProfessionId::Herbalist);
        }
        BuildRole::Devout => s.prayer = s.prayer.saturating_add(training_minutes),
        BuildRole::Civilian => s.carousing_minutes = training_minutes,
    }
    s
}

fn generated_personality(rng: &mut StableRng) -> Personality {
    let mut p = Personality::neutral();
    let mut axes = [0_u8, 1, 2, 3, 4, 5, 6, 7, 8];
    for index in (1..axes.len()).rev() {
        axes.swap(index, rng.next_u64() as usize % (index + 1));
    }
    let count = 2 + rng.next_u64() as usize % 3;
    for axis in axes.into_iter().take(count) {
        match axis {
            0 => {
                p.nerve = if rng.next_u64().is_multiple_of(2) {
                    Nerve::Brave
                } else {
                    Nerve::Fearful
                }
            }
            1 => {
                p.drive = if rng.next_u64().is_multiple_of(2) {
                    Drive::Ambitious
                } else {
                    Drive::Content
                }
            }
            2 => {
                p.outlook = if rng.next_u64().is_multiple_of(2) {
                    Outlook::Sanguine
                } else {
                    Outlook::Brooding
                }
            }
            3 => {
                p.sociability = if rng.next_u64().is_multiple_of(2) {
                    Sociability::Gregarious
                } else {
                    Sociability::Solitary
                }
            }
            4 => {
                p.conscience = match rng.next_u64() % 3 {
                    0 => Conscience::Compassionate,
                    1 => Conscience::Callous,
                    _ => Conscience::Cruel,
                }
            }
            5 => {
                p.self_regard = if rng.next_u64().is_multiple_of(2) {
                    SelfRegard::Proud
                } else {
                    SelfRegard::Humble
                }
            }
            6 => {
                p.conviction = if rng.next_u64().is_multiple_of(2) {
                    Conviction::Zealous
                } else {
                    Conviction::Irreverent
                }
            }
            7 => {
                p.hygiene = if rng.next_u64().is_multiple_of(2) {
                    Hygiene::Slovenly
                } else {
                    Hygiene::Cleanly
                }
            }
            _ => {
                p.temperance = if rng.next_u64().is_multiple_of(2) {
                    Temperance::Temperate
                } else {
                    Temperance::Drunkard
                }
            }
        }
    }
    p
}

pub fn derive_build(p: &Personality, a: &Attributes) -> AgentBuild {
    let arm_strength = (a.left_arm_strength + a.right_arm_strength) * 0.5;
    let frontline_viable = a.endurance >= 3.0 && arm_strength >= 3.0;
    let ranged_viable = a.precision >= 2.4 && a.eyesight >= 2.4;
    let (role, rationale) = if p.drive == Drive::Content {
        (
            BuildRole::Civilian,
            "content characters prefer settlement life",
        )
    } else if p.nerve == Nerve::Brave && frontline_viable {
        (
            BuildRole::FrontLine,
            "bravery and physical viability support heavy melee",
        )
    } else if p.nerve == Nerve::Fearful && ranged_viable {
        (
            BuildRole::Ranged,
            "fearfulness and perception support a safer ranged role",
        )
    } else if p.conscience == Conscience::Compassionate && a.intelligence >= 2.5 {
        (
            BuildRole::Healer,
            "compassion and intelligence support physiology",
        )
    } else if p.conviction == Conviction::Zealous {
        (
            BuildRole::Devout,
            "zeal supports religious study and prayer",
        )
    } else if ranged_viable {
        (BuildRole::Ranged, "perception supports ranged combat")
    } else {
        (
            BuildRole::Skirmisher,
            "light equipment avoids unsupported heavy requirements",
        )
    };
    AgentBuild {
        role,
        activity_only: p.drive == Drive::Content,
        rationale: rationale.into(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn attributes(endurance: f32, arm_strength: f32) -> Attributes {
        Attributes {
            endurance,
            immunity: 3.0,
            gut: 3.0,
            precision: 3.0,
            intelligence: 3.0,
            instinct: 3.0,
            eyesight: 3.0,
            hearing: 3.0,
            left_arm_strength: arm_strength,
            right_arm_strength: arm_strength,
            left_leg_strength: 3.0,
            right_leg_strength: 3.0,
            left_arm_agility: 3.0,
            right_arm_agility: 3.0,
            left_leg_agility: 3.0,
            right_leg_agility: 3.0,
        }
    }

    #[test]
    fn content_is_activity_only() {
        let mut p = Personality::neutral();
        p.drive = Drive::Content;
        let build = derive_build(&p, &attributes(4.0, 4.0));
        assert_eq!(build.role, BuildRole::Civilian);
        assert!(build.activity_only);
    }

    #[test]
    fn brave_front_line_requires_physical_viability() {
        let mut p = Personality::neutral();
        p.nerve = Nerve::Brave;
        assert_eq!(
            derive_build(&p, &attributes(4.0, 4.0)).role,
            BuildRole::FrontLine
        );
        assert_ne!(
            derive_build(&p, &attributes(2.0, 2.0)).role,
            BuildRole::FrontLine
        );
    }

    #[test]
    fn generated_personality_is_reproducible_and_sparse() {
        for id in 0..100 {
            let a = generate_profile(42, id);
            let b = generate_profile(42, id);
            assert_eq!(a, b);
            assert!((2..=4).contains(&a.personality.non_neutral_count()));
        }
    }
}
