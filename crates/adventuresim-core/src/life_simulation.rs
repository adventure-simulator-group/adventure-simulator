//! Bounded, one-time character life simulation.
//!
//! Creation advances a handful of coarse phases analytically. It returns only
//! the current training projection; no phase, activity, or schedule history is
//! persisted. Historical training starts at age six.

use crate::attribute::PlayerAttributes;
use crate::organization::{OrganizationDefinition, Requirement, TrainingEntry, TrainingTarget};
use crate::personality::Transparency;
use crate::skill::{Skill, apply_language_training};
use crate::strategic_schedule::{
    ActivityTrainingProfile, DailySchedule, SkillHours, SocializingSociability,
    apply_curriculum_training, apply_religion_training, apply_schedule_training,
};
use crate::strategic_time::{DAYS_PER_YEAR, MINUTES_PER_DAY, MINUTES_PER_YEAR};
use adventuresim_world_schema::{
    OfficialReligion, OralLanguageHours, WrittenLanguage, WrittenLanguageHours,
};

pub const TRAINING_START_AGE: u16 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifePhaseKind {
    Childhood,
    StudentOrApprentice,
    Professional,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoricalActivity {
    ChildhoodUpbringing,
    StudentStudy,
    ProfessionalPractice,
    LiteracyStudy,
}

impl HistoricalActivity {
    pub const fn eligible(self, phase: LifePhaseKind) -> bool {
        matches!(
            (self, phase),
            (Self::ChildhoodUpbringing, LifePhaseKind::Childhood)
                | (Self::StudentStudy, LifePhaseKind::StudentOrApprentice)
                | (Self::LiteracyStudy, LifePhaseKind::StudentOrApprentice)
                | (Self::ProfessionalPractice, LifePhaseKind::Professional)
        )
    }
}

pub struct LifeSimulationInput<'a, A: PlayerAttributes> {
    pub stable_seed: u64,
    pub age_years: u16,
    pub attributes: &'a A,
    pub organization: Option<&'a OrganizationDefinition>,
    pub rank_requirements: &'a [Requirement],
    pub religion: Option<OfficialReligion>,
    pub activity_profile: ActivityTrainingProfile,
    /// Native speech is identity/acquisition, not credited study time. It is
    /// carried through the simulator so creation has one explicit language
    /// boundary while only written literacy consumes historical study.
    pub native_oral: OralLanguageHours,
    pub literacy: Option<WrittenLanguage>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LifeSimulationOutput {
    pub skills: SkillHours,
    pub written: WrittenLanguageHours,
    pub oral: OralLanguageHours,
    pub phases_advanced: u8,
}

/// Simulate at most three analytical life phases. The stable seed changes
/// authored emphasis without consuming shared RNG state.
pub fn simulate_life<A: PlayerAttributes>(
    input: LifeSimulationInput<'_, A>,
) -> LifeSimulationOutput {
    let mut output = LifeSimulationOutput {
        oral: input.native_oral,
        ..Default::default()
    };
    if input.age_years <= TRAINING_START_AGE {
        return output;
    }

    let childhood_end = input.age_years.min(12);
    if childhood_end > TRAINING_START_AGE {
        let emphasis = 45 + (domain_draw(input.stable_seed, "childhood") % 31) as u16;
        let schedule = DailySchedule {
            labor: 120,
            prayer: 30,
            carousing_minutes: emphasis,
            combat_training_minutes: 75 - emphasis / 2,
            ..Default::default()
        };
        apply_common_phase(
            &mut output,
            schedule,
            TRAINING_START_AGE,
            childhood_end,
            &input,
        );
        // Upbringing is a creation-only activity, intentionally absent from
        // the adult schedule UI.
        let upbringing = [
            TrainingEntry {
                weight: 0.25,
                target: TrainingTarget::FixedSkill {
                    skill: "balance".into(),
                },
            },
            TrainingEntry {
                weight: 0.20,
                target: TrainingTarget::FixedSkill {
                    skill: "insight".into(),
                },
            },
        ];
        run_curriculum_activity(
            &mut output,
            LifePhaseKind::Childhood,
            HistoricalActivity::ChildhoodUpbringing,
            &upbringing,
            180.0 * f32::from(childhood_end - TRAINING_START_AGE),
            &input,
        );
        output.phases_advanced += 1;
    }

    let student_end = input.age_years.min(16);
    if student_end > 12 {
        let schedule = DailySchedule {
            labor: 120,
            prayer: 30,
            combat_training_minutes: 60,
            carousing_minutes: 45,
            ..Default::default()
        };
        apply_common_phase(&mut output, schedule, 12, student_end, &input);
        let study = [
            TrainingEntry {
                weight: 0.35,
                target: TrainingTarget::FixedSkill {
                    skill: "will".into(),
                },
            },
            TrainingEntry {
                weight: 0.25,
                target: TrainingTarget::FixedSkill {
                    skill: "charm".into(),
                },
            },
        ];
        let mut student_curriculum = normalized_curriculum(&study);
        let mut daily_curriculum_hours = 1.0;
        if let Some(organization) = input.organization {
            student_curriculum.extend(
                normalized_curriculum(&organization.activity.training)
                    .into_iter()
                    .map(|mut entry| {
                        entry.weight *= 4.0;
                        entry
                    }),
            );
            daily_curriculum_hours = 5.0;
        }
        run_curriculum_activity(
            &mut output,
            LifePhaseKind::StudentOrApprentice,
            HistoricalActivity::StudentStudy,
            &student_curriculum,
            daily_curriculum_hours * DAYS_PER_YEAR as f32 * f32::from(student_end - 12),
            &input,
        );
        if let Some(language) = input.literacy {
            apply_creation_literacy(
                &mut output.written,
                input.age_years,
                language,
                input.attributes,
            );
        }
        output.phases_advanced += 1;
    }

    if input.age_years > 16 {
        let years = input.age_years - 16;
        let schedule = DailySchedule {
            labor: if input.organization.is_some() { 0 } else { 480 },
            prayer: 30,
            combat_training_minutes: 60,
            carousing_minutes: 45,
            ..Default::default()
        };
        apply_common_phase(&mut output, schedule, 16, input.age_years, &input);
        if let Some(organization) = input.organization {
            let curriculum = professional_curriculum(organization, input.rank_requirements);
            run_curriculum_activity(
                &mut output,
                LifePhaseKind::Professional,
                HistoricalActivity::ProfessionalPractice,
                &curriculum,
                8.0 * DAYS_PER_YEAR as f32 * f32::from(years),
                &input,
            );
        }
        output.phases_advanced += 1;
    }
    output
}

fn apply_common_phase<A: PlayerAttributes>(
    output: &mut LifeSimulationOutput,
    schedule: DailySchedule,
    start_age: u16,
    end_age: u16,
    input: &LifeSimulationInput<'_, A>,
) {
    debug_assert!(schedule.allocated_minutes() <= MINUTES_PER_DAY);
    let elapsed = u64::from(end_age - start_age) * MINUTES_PER_YEAR;
    apply_schedule_training(
        &mut output.skills,
        schedule,
        elapsed,
        input.activity_profile,
        SocializingSociability::Neutral,
        Transparency::Neutral,
        input.attributes,
    );
    apply_religion_training(
        &mut output.skills.religion,
        elapsed,
        input.religion,
        schedule.prayer,
        input.attributes,
    );
}

fn run_curriculum_activity<A: PlayerAttributes>(
    output: &mut LifeSimulationOutput,
    phase: LifePhaseKind,
    activity: HistoricalActivity,
    entries: &[TrainingEntry],
    work_hours: f32,
    input: &LifeSimulationInput<'_, A>,
) -> bool {
    if !activity.eligible(phase) || work_hours <= 0.0 {
        return false;
    }
    let normalized = normalized_curriculum(entries);
    let (_, written) = apply_curriculum_training(
        &mut output.skills,
        work_hours,
        &normalized,
        input.activity_profile,
        input.attributes,
    );
    apply_written(output, written, input.attributes);
    true
}

fn apply_written(
    output: &mut LifeSimulationOutput,
    awards: Vec<(adventuresim_world_schema::WrittenLanguage, f32)>,
    attributes: &impl PlayerAttributes,
) {
    let aptitude = Skill::Religion.governing_aptitude(attributes);
    for (language, hours) in awards {
        apply_language_training(output.written.direct_mut(language), hours, aptitude);
    }
}

fn credential_curriculum(requirements: &[Requirement]) -> Vec<TrainingEntry> {
    requirements
        .iter()
        .map(|requirement| match requirement {
            Requirement::ProfessedReligion { religion } => TrainingEntry {
                weight: 1.0,
                target: TrainingTarget::Religion {
                    religion: religion.clone(),
                },
            },
            Requirement::SkillRating { skill, leaf, .. } => TrainingEntry {
                weight: 1.0,
                target: match (skill.as_str(), leaf) {
                    ("religion", Some(religion)) => TrainingTarget::Religion {
                        religion: religion.clone(),
                    },
                    ("bestiary", Some(category)) => TrainingTarget::Bestiary {
                        category: category.clone(),
                    },
                    _ => TrainingTarget::FixedSkill {
                        skill: skill.clone(),
                    },
                },
            },
        })
        .collect()
}

fn professional_curriculum(
    organization: &OrganizationDefinition,
    requirements: &[Requirement],
) -> Vec<TrainingEntry> {
    let mut entries = organization.activity.training.clone();
    for credential in credential_curriculum(requirements) {
        if let Some(existing) = entries
            .iter_mut()
            .find(|entry| entry.target == credential.target)
        {
            existing.weight = existing.weight.max(credential.weight);
        } else {
            entries.push(credential);
        }
    }
    entries
}

fn normalized_curriculum(entries: &[TrainingEntry]) -> Vec<TrainingEntry> {
    let total = entries
        .iter()
        .map(|entry| entry.weight.max(0.0))
        .sum::<f32>();
    if total <= f32::EPSILON {
        return Vec::new();
    }
    entries
        .iter()
        .map(|entry| TrainingEntry {
            weight: entry.weight.max(0.0) / total,
            target: entry.target.clone(),
        })
        .collect()
}

/// Apply creation-only literacy study through ordinary aptitude-aware language
/// training. This helper is also used when role-authored literacy is resolved only
/// after the candidate's settlement is known.
pub fn apply_creation_literacy(
    written: &mut WrittenLanguageHours,
    age_years: u16,
    language: WrittenLanguage,
    attributes: &impl PlayerAttributes,
) -> bool {
    if age_years <= 12
        || !HistoricalActivity::LiteracyStudy.eligible(LifePhaseKind::StudentOrApprentice)
    {
        return false;
    }
    let study_years = age_years.min(16) - 12;
    let aptitude = Skill::Religion.governing_aptitude(attributes);
    apply_language_training(
        written.direct_mut(language),
        250.0 * f32::from(study_years),
        aptitude,
    );
    true
}

fn domain_draw(seed: u64, domain: &str) -> u64 {
    domain
        .bytes()
        .fold(seed ^ 0x6c69_6665_2d76_3100, |value, byte| {
            (value ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute::{LimbAttribute, SimpleAttribute};
    use crate::body::BodyPart;

    #[derive(Clone, Copy)]
    struct Attributes(f32);
    impl PlayerAttributes for Attributes {
        fn raw_limb_attr(&self, _: LimbAttribute, _: BodyPart) -> f32 {
            self.0
        }
        fn raw_single_body_part_attr(&self, _: SimpleAttribute) -> f32 {
            self.0
        }
    }

    fn run(seed: u64, age: u16) -> LifeSimulationOutput {
        simulate_life(LifeSimulationInput {
            stable_seed: seed,
            age_years: age,
            attributes: &Attributes(4.0),
            organization: None,
            rank_requirements: &[],
            religion: Some(OfficialReligion::RomanCatholic),
            activity_profile: ActivityTrainingProfile::default(),
            native_oral: Default::default(),
            literacy: None,
        })
    }

    #[test]
    fn replay_is_exact_and_seed_changes_emphasis() {
        assert_eq!(run(7, 25), run(7, 25));
        assert_ne!(run(7, 25), run(8, 25));
    }

    #[test]
    fn phases_are_bounded_and_age_adds_training() {
        let young = run(1, 16);
        let adult = run(1, 22);
        let old = run(1, 40);
        assert_eq!(young.phases_advanced, 2);
        assert_eq!(adult.phases_advanced, 3);
        assert_eq!(old.phases_advanced, 3);
        assert!(
            old.skills.values().into_iter().sum::<f32>() > adult.skills.values().into_iter().sum()
        );
        assert!(old.skills.is_finite());
        assert!(old.skills.values().into_iter().all(|hours| hours >= 0.0));
        assert!(Skill::Will.training_rank(old.skills.will) <= 4.0);
        assert!(Skill::Sword.training_rank(old.skills.sword) <= 4.0);
    }

    #[test]
    fn creation_only_activity_eligibility_is_explicit() {
        assert!(HistoricalActivity::ChildhoodUpbringing.eligible(LifePhaseKind::Childhood));
        assert!(HistoricalActivity::StudentStudy.eligible(LifePhaseKind::StudentOrApprentice));
        assert!(HistoricalActivity::ProfessionalPractice.eligible(LifePhaseKind::Professional));
        assert!(!HistoricalActivity::ChildhoodUpbringing.eligible(LifePhaseKind::Professional));

        let attributes = Attributes(2.5);
        let input = LifeSimulationInput {
            stable_seed: 1,
            age_years: 22,
            attributes: &attributes,
            organization: None,
            rank_requirements: &[],
            religion: None,
            activity_profile: ActivityTrainingProfile::default(),
            native_oral: Default::default(),
            literacy: None,
        };
        let mut output = LifeSimulationOutput::default();
        let entries = [TrainingEntry {
            weight: 1.0,
            target: TrainingTarget::FixedSkill {
                skill: "smithing".into(),
            },
        }];
        assert!(!run_curriculum_activity(
            &mut output,
            LifePhaseKind::Childhood,
            HistoricalActivity::ProfessionalPractice,
            &entries,
            1_000.0,
            &input,
        ));
        assert_eq!(output.skills, SkillHours::default());
    }

    #[test]
    fn shared_organization_curriculum_is_chunk_stable() {
        let definition = crate::organization::organization("herbalists_fellowship").unwrap();
        let attributes = Attributes(4.0);
        let mut bulk = SkillHours::default();
        let mut chunked = SkillHours::default();
        crate::strategic_schedule::apply_organization_training(
            &mut bulk,
            2_000.0,
            definition,
            ActivityTrainingProfile::default(),
            &attributes,
        );
        for _ in 0..2 {
            crate::strategic_schedule::apply_organization_training(
                &mut chunked,
                1_000.0,
                definition,
                ActivityTrainingProfile::default(),
                &attributes,
            );
        }
        assert!((bulk.herbalism - chunked.herbalism).abs() < 0.01);
    }

    #[test]
    fn professional_curriculum_conserves_one_budget_across_requirements() {
        let definition = crate::organization::organization("herbalists_fellowship").unwrap();
        let requirements = [
            Requirement::SkillRating {
                skill: "will".into(),
                minimum: 1.0,
                leaf: None,
            },
            Requirement::SkillRating {
                skill: "charm".into(),
                minimum: 1.0,
                leaf: None,
            },
        ];
        let entries = professional_curriculum(definition, &requirements);
        let normalized = normalized_curriculum(&entries);
        assert!((normalized.iter().map(|entry| entry.weight).sum::<f32>() - 1.0).abs() < 0.0001);
        let attributes = Attributes(2.5);
        let input = LifeSimulationInput {
            stable_seed: 9,
            age_years: 22,
            attributes: &attributes,
            organization: Some(definition),
            rank_requirements: &requirements,
            religion: None,
            activity_profile: ActivityTrainingProfile::default(),
            native_oral: Default::default(),
            literacy: None,
        };
        let mut output = LifeSimulationOutput::default();
        assert!(run_curriculum_activity(
            &mut output,
            LifePhaseKind::Professional,
            HistoricalActivity::ProfessionalPractice,
            &entries,
            1_000.0,
            &input,
        ));
        let awarded = output.skills.herbalism + output.skills.will + output.skills.charm;
        assert!((awarded - 1_000.0).abs() < 0.1);
    }

    #[test]
    fn literacy_is_aptitude_trained_and_native_speech_is_identity() {
        let mut written = WrittenLanguageHours::default();
        assert!(apply_creation_literacy(
            &mut written,
            16,
            WrittenLanguage::German,
            &Attributes(2.5),
        ));
        assert_eq!(written.german, 1_000.0);
        assert!(!apply_creation_literacy(
            &mut WrittenLanguageHours::default(),
            12,
            WrittenLanguage::German,
            &Attributes(5.0),
        ));
    }
}
