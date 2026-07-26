//! Stable service-profession metadata shared by progression and presentation.

use crate::skill::Skill;

/// Stable identifier used by schedules and persisted service mappings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfessionId {
    Merchant,
    Weaponsmith,
    Armourer,
    Tailor,
    Herbalist,
    Cook,
    Religion,
}

impl ProfessionId {
    pub const fn service_id(self) -> &'static str {
        match self {
            Self::Merchant => "merchants",
            Self::Weaponsmith => "weapons",
            Self::Armourer => "armor",
            Self::Tailor => "clothing",
            Self::Herbalist => "herbalist",
            Self::Cook => "inn",
            Self::Religion => "religion",
        }
    }

    pub fn from_service_id(service_id: &str) -> Option<Self> {
        match service_id {
            "merchants" => Some(Self::Merchant),
            "weapons" => Some(Self::Weaponsmith),
            "armor" => Some(Self::Armourer),
            "clothing" => Some(Self::Tailor),
            "herbalist" => Some(Self::Herbalist),
            "inn" => Some(Self::Cook),
            "religion" => Some(Self::Religion),
            _ => None,
        }
    }
}

/// One skill's share of the training awarded by a profession activity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfessionSkillWeight {
    pub skill: Skill,
    pub weight: f32,
}

/// The profession represented by a settlement service.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfessionDefinition {
    pub id: ProfessionId,
    pub service_id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub skills: &'static [ProfessionSkillWeight],
    pub religious: bool,
    pub practice_reward: PracticeReward,
}

/// Strategic reward produced by independent professional practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeReward {
    Gold,
    Virtue,
}

/// Written exposure per hour of profession work. Religious language depends
/// on the enrolled denomination and is resolved by the strategic server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfessionLiteracyProfile {
    pub vernacular: f32,
    pub latin: f32,
    pub religious: bool,
}

pub fn profession_literacy_profile(service_id: &str) -> ProfessionLiteracyProfile {
    match service_id {
        "merchants" => ProfessionLiteracyProfile {
            vernacular: 0.40,
            latin: 0.0,
            religious: false,
        },
        "herbalist" => ProfessionLiteracyProfile {
            vernacular: 0.05,
            latin: 0.25,
            religious: false,
        },
        "weapons" | "armor" => ProfessionLiteracyProfile {
            vernacular: 0.03,
            latin: 0.0,
            religious: false,
        },
        "clothing" => ProfessionLiteracyProfile {
            vernacular: 0.08,
            latin: 0.0,
            religious: false,
        },
        "inn" => ProfessionLiteracyProfile {
            vernacular: 0.02,
            latin: 0.0,
            religious: false,
        },
        "religion" => ProfessionLiteracyProfile {
            vernacular: 0.0,
            latin: 0.0,
            religious: true,
        },
        _ => ProfessionLiteracyProfile {
            vernacular: 0.0,
            latin: 0.0,
            religious: false,
        },
    }
}

/// Progression earned within a profession. Enrollment is stored separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfessionTier {
    Apprentice,
    Journeyman,
    Master,
}

impl ProfessionTier {
    /// Player-facing title, using tradition-neutral religious vocabulary.
    pub const fn title(self, religious: bool) -> &'static str {
        match (self, religious) {
            (Self::Apprentice, false) => "apprentice",
            (Self::Journeyman, false) => "journeyman",
            (Self::Master, false) => "master",
            (Self::Apprentice, true) => "novice",
            (Self::Journeyman, true) => "cleric",
            (Self::Master, true) => "teacher",
        }
    }
}

const CHARISMA: &[ProfessionSkillWeight] = &[ProfessionSkillWeight {
    skill: Skill::Command,
    weight: 1.0,
}];
const SMITHING: &[ProfessionSkillWeight] = &[ProfessionSkillWeight {
    skill: Skill::Smithing,
    weight: 1.0,
}];
const TAILORING: &[ProfessionSkillWeight] = &[ProfessionSkillWeight {
    skill: Skill::Tailoring,
    weight: 1.0,
}];
const PHYSIOLOGY: &[ProfessionSkillWeight] = &[
    ProfessionSkillWeight {
        skill: Skill::Physiology,
        weight: 0.5,
    },
    ProfessionSkillWeight {
        skill: Skill::Anatomy,
        weight: 1.0 / 6.0,
    },
    ProfessionSkillWeight {
        skill: Skill::Knife,
        weight: 1.0 / 6.0,
    },
    ProfessionSkillWeight {
        skill: Skill::Tailoring,
        weight: 1.0 / 6.0,
    },
];
const COOKING: &[ProfessionSkillWeight] = &[ProfessionSkillWeight {
    skill: Skill::Cooking,
    weight: 1.0,
}];
const RELIGION: &[ProfessionSkillWeight] = &[ProfessionSkillWeight {
    skill: Skill::Religion,
    weight: 1.0,
}];

/// Canonical order follows the settlement service navigation.
pub const PROFESSIONS: &[ProfessionDefinition] = &[
    ProfessionDefinition {
        id: ProfessionId::Merchant,
        service_id: "merchants",
        label: "merchant",
        description: "Merchants appraise goods, negotiate exchanges, and keep a market's trade moving.",
        skills: CHARISMA,
        religious: false,
        practice_reward: PracticeReward::Gold,
    },
    ProfessionDefinition {
        id: ProfessionId::Weaponsmith,
        service_id: "weapons",
        label: "weaponsmith",
        description: "Weaponsmiths forge, fit, and repair weapons and shields.",
        skills: SMITHING,
        religious: false,
        practice_reward: PracticeReward::Gold,
    },
    ProfessionDefinition {
        id: ProfessionId::Armourer,
        service_id: "armor",
        label: "armourer",
        description: "Armourers shape, fit, and repair protective equipment.",
        skills: SMITHING,
        religious: false,
        practice_reward: PracticeReward::Gold,
    },
    ProfessionDefinition {
        id: ProfessionId::Tailor,
        service_id: "clothing",
        label: "tailor",
        description: "Tailors cut, fit, and repair clothing for work, travel, and display.",
        skills: TAILORING,
        religious: false,
        practice_reward: PracticeReward::Gold,
    },
    ProfessionDefinition {
        id: ProfessionId::Herbalist,
        service_id: "herbalist",
        label: "physician",
        description: "Physicians study changing bodily function, keep observation notebooks, administer existing preparations, and support wound recovery. Herbalists own remedy preparation.",
        skills: PHYSIOLOGY,
        religious: false,
        practice_reward: PracticeReward::Gold,
    },
    ProfessionDefinition {
        id: ProfessionId::Cook,
        service_id: "inn",
        label: "cook",
        description: "Cooks prepare safe, nourishing meals and manage a busy tavern kitchen.",
        skills: COOKING,
        religious: false,
        practice_reward: PracticeReward::Gold,
    },
    ProfessionDefinition {
        id: ProfessionId::Religion,
        service_id: "religion",
        label: "religious teacher",
        description: "Religious teachers study, preserve, and explain the teachings of their own tradition.",
        skills: RELIGION,
        religious: true,
        practice_reward: PracticeReward::Virtue,
    },
];

pub fn profession_for_service(service_id: &str) -> Option<&'static ProfessionDefinition> {
    ProfessionId::from_service_id(service_id).and_then(profession_definition)
}

pub fn profession_definition(id: ProfessionId) -> Option<&'static ProfessionDefinition> {
    PROFESSIONS.iter().find(|profession| profession.id == id)
}

/// Derive standing from the least-developed skill used by the profession.
///
/// Multi-skill professions therefore require competence in every part of their
/// practice. Whether the character has joined the profession is an independent
/// concern and must not be inferred from skill hours.
pub fn profession_tier(
    profession: &ProfessionDefinition,
    mut training_hours: impl FnMut(Skill) -> f32,
    mut aptitude: impl FnMut(Skill) -> f32,
) -> ProfessionTier {
    let lowest_rank = profession
        .skills
        .iter()
        .map(|entry| {
            entry
                .skill
                .capped_rank_for_aptitude(training_hours(entry.skill), aptitude(entry.skill))
        })
        .fold(f32::INFINITY, f32::min);
    if lowest_rank >= 4.0 {
        ProfessionTier::Master
    } else if lowest_rank >= 2.0 {
        ProfessionTier::Journeyman
    } else {
        ProfessionTier::Apprentice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_every_service_once_and_normalized_skill_weights() {
        let expected = [
            "merchants",
            "weapons",
            "armor",
            "clothing",
            "herbalist",
            "inn",
            "religion",
        ];
        assert_eq!(
            PROFESSIONS
                .iter()
                .map(|entry| entry.service_id)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            PROFESSIONS
                .iter()
                .map(|entry| entry.service_id)
                .collect::<HashSet<_>>()
                .len(),
            PROFESSIONS.len()
        );
        for profession in PROFESSIONS {
            assert!(!profession.label.is_empty());
            assert!(!profession.description.is_empty());
            assert!(!profession.skills.is_empty());
            let total: f32 = profession.skills.iter().map(|entry| entry.weight).sum();
            assert!((total - 1.0).abs() < f32::EPSILON);
            assert!(profession.skills.iter().all(|entry| entry.weight > 0.0));
        }
    }

    #[test]
    fn lookup_uses_stable_service_ids() {
        assert_eq!(
            profession_for_service("weapons").unwrap().label,
            "weaponsmith"
        );
        assert_eq!(profession_for_service("religion").unwrap().religious, true);
        assert_eq!(
            profession_for_service("inn").unwrap().id,
            ProfessionId::Cook
        );
        assert_eq!(profession_for_service("inn").unwrap().skills, COOKING);
        assert!(profession_for_service("smith").is_none());
        for profession in PROFESSIONS {
            assert_eq!(profession.id.service_id(), profession.service_id);
            assert_eq!(
                ProfessionId::from_service_id(profession.service_id),
                Some(profession.id)
            );
            assert_eq!(profession_definition(profession.id), Some(profession));
        }
    }

    #[test]
    fn standing_changes_at_exact_rank_boundaries() {
        let smith = profession_for_service("weapons").unwrap();
        // Smithing max_hours is 10,000: ranks two and four occur at max/3 and 2*max.
        assert_eq!(
            profession_tier(smith, |_| 0.0, |_| 5.0),
            ProfessionTier::Apprentice
        );
        assert_eq!(
            profession_tier(smith, |_| 10_000.0 / 3.0, |_| 5.0),
            ProfessionTier::Journeyman
        );
        assert_eq!(
            profession_tier(smith, |_| 20_000.0, |_| 5.0),
            ProfessionTier::Master
        );
    }

    #[test]
    fn multi_skill_profession_uses_its_lowest_rank() {
        let herbalist = profession_for_service("herbalist").unwrap();
        assert_eq!(
            profession_tier(
                herbalist,
                |skill| match skill {
                    Skill::Physiology => 20_000.0,
                    Skill::Anatomy => 0.0,
                    Skill::Knife | Skill::Tailoring => 20_000.0,
                    _ => unreachable!("unexpected medical skill: {skill:?}"),
                },
                |_| 5.0,
            ),
            ProfessionTier::Apprentice
        );
    }

    #[test]
    fn religious_titles_are_distinct_and_neutral() {
        assert_eq!(ProfessionTier::Apprentice.title(false), "apprentice");
        assert_eq!(ProfessionTier::Journeyman.title(false), "journeyman");
        assert_eq!(ProfessionTier::Master.title(false), "master");
        assert_eq!(ProfessionTier::Apprentice.title(true), "novice");
        assert_eq!(ProfessionTier::Journeyman.title(true), "cleric");
        assert_eq!(ProfessionTier::Master.title(true), "teacher");
        assert_eq!(
            profession_for_service("religion").unwrap().practice_reward,
            PracticeReward::Virtue
        );
        assert!(
            PROFESSIONS
                .iter()
                .filter(|profession| !profession.religious)
                .all(|profession| profession.practice_reward == PracticeReward::Gold)
        );
    }
}
