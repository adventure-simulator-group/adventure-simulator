use adventuresim_core::{
    activity::{PRAYER_MORALE_LIMIT, PRAYER_MORALE_SCALE_MINUTES, settlement_population_scale},
    prelude::{PlayerSkills, Skill},
    skill::aptitude_training_multiplier,
    strategic_schedule::{
        BASELINE_FATIGUE_PER_DAY, CombatTrainingProfile, DailySchedule,
        FATIGUE_RESERVOIR_PER_PREVIEW_POINT, LABOR_FATIGUE_PER_HOUR,
        LEISURE_FATIGUE_RECOVERY_PER_HOUR, LEISURE_MORALE_LIMIT, LEISURE_MORALE_SCALE_FATIGUE,
        LeisureOutcome, settlement_leisure_outcome,
    },
    strategic_time::MINUTES_PER_DAY,
};
use adventuresim_world_schema::{
    BestiaryCategory, OfficialReligion, OralLanguage, WrittenLanguage,
};
use maud::{Markup, html};

use super::character_health::stat_icon;
use crate::spacetimedb::{
    CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterSkills, CharacterStats,
    CharacterTrainingSchedule, OrganizationMembership, ScheduleAllocation, Settlement,
};
use crate::templates::{
    game_icon, religion_icon, religion_icon_path, sidebar_section, stat_icon_path,
};

const SUMMARY_SKILL_THRESHOLD: f32 = 3.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SummaryIconKind {
    Mask(String),
    Monogram {
        text: &'static str,
        germanic_style: bool,
        written: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SummaryIcon {
    pub(super) label: String,
    pub(super) tooltip: String,
    pub(super) rank: f32,
    pub(super) kind: SummaryIconKind,
}

fn finite_rank(rank: f32) -> f32 {
    if rank.is_finite() {
        rank.clamp(0.0, 5.0)
    } else {
        0.0
    }
}

pub(super) fn skill_rank_tier(rank: f32) -> u8 {
    let rank = finite_rank(rank);
    if rank <= 0.0 {
        0
    } else {
        rank.ceil().clamp(1.0, 5.0) as u8
    }
}

fn social_family_rank(skills: &CharacterSkills, aptitude: f32) -> f32 {
    [
        (Skill::Insight, skills.insight_hours),
        (Skill::Charm, skills.charm_hours),
        (Skill::Command, skills.command_hours),
        (Skill::Deception, skills.deception_hours),
    ]
    .into_iter()
    .map(|(skill, hours)| skill.capped_rank_for_aptitude(hours, aptitude))
    .map(finite_rank)
    .sum::<f32>()
        / 4.0
}

fn terrain_family_rank(skills: &CharacterSkills, aptitude: f32) -> f32 {
    [
        Skill::TerrainPlains,
        Skill::TerrainForest,
        Skill::TerrainHills,
        Skill::TerrainUrban,
    ]
    .into_iter()
    .map(|skill| {
        skill.capped_rank_for_aptitude(
            CharacterSkillHours(skills).effective_skill_hours(skill),
            aptitude,
        )
    })
    .map(finite_rank)
    .sum::<f32>()
        / 4.0
}

fn strongest_oral_language(skills: &CharacterSkills) -> OralLanguage {
    OralLanguage::ALL
        .into_iter()
        .max_by(|left, right| {
            skills
                .oral_languages
                .effective(*left)
                .total_cmp(&skills.oral_languages.effective(*right))
        })
        .unwrap_or(OralLanguage::EastCentral)
}

fn strongest_written_language(skills: &CharacterSkills) -> WrittenLanguage {
    WrittenLanguage::ALL
        .into_iter()
        .max_by(|left, right| {
            skills
                .written_languages
                .effective(*left)
                .total_cmp(&skills.written_languages.effective(*right))
        })
        .unwrap_or(WrittenLanguage::German)
}

fn primary_religion(
    skills: &CharacterSkills,
    context: Option<OfficialReligion>,
) -> OfficialReligion {
    context.unwrap_or_else(|| {
        OfficialReligion::ALL
            .into_iter()
            .max_by(|left, right| {
                skills
                    .religion_hours
                    .effective(*left)
                    .total_cmp(&skills.religion_hours.effective(*right))
            })
            .unwrap_or(OfficialReligion::RomanCatholic)
    })
}

fn summary_mask_icon(
    label: impl Into<String>,
    tooltip: impl Into<String>,
    rank: f32,
    category: &str,
    icon: &str,
) -> SummaryIcon {
    SummaryIcon {
        label: label.into(),
        tooltip: tooltip.into(),
        rank: finite_rank(rank),
        kind: SummaryIconKind::Mask(stat_icon_path(category, icon)),
    }
}

fn qualifying_tooltip(family: &str, entries: &[(&str, f32)]) -> String {
    let mut tooltip = family.to_owned();
    for (label, rank) in entries {
        tooltip.push_str(&format!("\n{label} — {:.1}", finite_rank(*rank)));
    }
    tooltip
}

fn push_standalone_summary_icon(
    icons: &mut Vec<SummaryIcon>,
    skills: &CharacterSkills,
    label: &'static str,
    icon: &'static str,
    skill: Skill,
    aptitude: f32,
) {
    let rank = finite_rank(skill.capped_rank_for_aptitude(
        CharacterSkillHours(skills).effective_skill_hours(skill),
        aptitude,
    ));
    if rank >= SUMMARY_SKILL_THRESHOLD {
        let text = format!("{label} — {rank:.1}");
        icons.push(summary_mask_icon(&text, &text, rank, "skills", icon));
    }
}

/// Compact character-sheet summary based on healthy aptitude-capped ranks.
/// Injuries are deliberately ignored, matching recruitment-summary semantics.
/// The equipped profile selects combat leaves, while the shared family helpers
/// keep aggregation identical to the expandable skill rail.
pub(super) fn character_summary_icons(
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    combat_profile: CombatTrainingProfile,
    religion_context: Option<OfficialReligion>,
) -> Vec<SummaryIcon> {
    let (Some(attributes), Some(skills)) = (attributes, skills) else {
        return Vec::new();
    };
    let intelligence = finite_rank(attributes.intelligence);
    let instinct = finite_rank(attributes.instinct);
    let arm_agility =
        finite_rank((attributes.left_arm_agility + attributes.right_arm_agility) * 0.5);
    let leg_agility =
        finite_rank((attributes.left_leg_agility + attributes.right_leg_agility) * 0.5);
    let all_agility = finite_rank(
        (attributes.left_arm_agility
            + attributes.right_arm_agility
            + attributes.left_leg_agility
            + attributes.right_leg_agility)
            * 0.25,
    );
    let view = CharacterSkillHours(skills);
    let mut icons = Vec::new();

    for (weight, (label, icon, skill)) in combat_profile.weights()[..9].iter().copied().zip([
        ("Polearm", "spear-hook", Skill::Polearm),
        ("Axe", "battle-axe", Skill::Axe),
        ("Bludgeon", "flanged-mace", Skill::Bludgeon),
        ("Sword", "sword", Skill::Sword),
        ("Knife", "bowie-knife", Skill::Knife),
        ("Bow", "bow-arrow", Skill::Bow),
        ("Crossbow", "crossbow", Skill::Crossbow),
        ("Firearm", "musket", Skill::Firearm),
        ("Throw", "throwing-ball", Skill::Throw),
    ]) {
        if weight > 0.0 {
            let rank = finite_rank(
                skill.capped_rank_for_aptitude(view.effective_skill_hours(skill), arm_agility),
            );
            let label = format!("{label} — {rank:.1}");
            icons.push(summary_mask_icon(&label, &label, rank, "skills", icon));
        }
    }

    if let Some(capability) = capability {
        let armor = if capability.full_armor {
            Some(("Full armor", "breastplate"))
        } else if capability.three_quarter_armor {
            Some(("Three-quarter armor", "layered-armor"))
        } else if capability.half_armor {
            Some(("Half armor", "chain-mail"))
        } else if capability.quarter_armor {
            Some(("Quarter armor", "armor-vest"))
        } else {
            None
        };
        if let Some((armor_label, icon)) = armor {
            let dodge =
                finite_rank(Skill::Dodge.capped_rank_for_aptitude(
                    view.effective_skill_hours(Skill::Dodge),
                    leg_agility,
                ));
            let block =
                finite_rank(Skill::Block.capped_rank_for_aptitude(
                    view.effective_skill_hours(Skill::Block),
                    arm_agility,
                ));
            let (defense, rank) = if block > dodge {
                ("Block", block)
            } else {
                ("Dodge", dodge)
            };
            let label = format!("{armor_label} — {defense} {rank:.1}");
            let tooltip = format!("{label}\nDodge — {dodge:.1}\nBlock — {block:.1}");
            icons.push(SummaryIcon {
                label,
                tooltip,
                rank,
                kind: SummaryIconKind::Mask(format!("/static/icons/game/{icon}.svg")),
            });
        }
    }

    let social_entries = [
        (
            "Insight",
            finite_rank(
                Skill::Insight
                    .capped_rank_for_aptitude(view.effective_skill_hours(Skill::Insight), instinct),
            ),
        ),
        (
            "Charm",
            finite_rank(
                Skill::Charm
                    .capped_rank_for_aptitude(view.effective_skill_hours(Skill::Charm), instinct),
            ),
        ),
        (
            "Command",
            finite_rank(
                Skill::Command
                    .capped_rank_for_aptitude(view.effective_skill_hours(Skill::Command), instinct),
            ),
        ),
        (
            "Deception",
            finite_rank(
                Skill::Deception.capped_rank_for_aptitude(
                    view.effective_skill_hours(Skill::Deception),
                    instinct,
                ),
            ),
        ),
    ];
    let qualifying_social = social_entries
        .into_iter()
        .filter(|(_, rank)| *rank >= SUMMARY_SKILL_THRESHOLD)
        .collect::<Vec<_>>();
    if !qualifying_social.is_empty() {
        let rank = finite_rank(social_family_rank(skills, instinct));
        icons.push(summary_mask_icon(
            format!("Social — {rank:.1}"),
            qualifying_tooltip("Social", &qualifying_social),
            rank,
            "skills",
            "social",
        ));
    }

    push_standalone_summary_icon(
        &mut icons,
        skills,
        "Physiology",
        "physiology",
        Skill::Physiology,
        intelligence,
    );
    push_standalone_summary_icon(
        &mut icons,
        skills,
        "Cooking",
        "cooking",
        Skill::Cooking,
        intelligence,
    );

    let primary = primary_religion(skills, religion_context);
    let religion_entries = OfficialReligion::ALL
        .into_iter()
        .map(|religion| {
            (
                religion.label(),
                finite_rank(Skill::Religion.capped_rank_for_aptitude(
                    skills.religion_hours.effective(religion),
                    intelligence,
                )),
            )
        })
        .filter(|(_, rank)| *rank >= SUMMARY_SKILL_THRESHOLD)
        .collect::<Vec<_>>();
    if !religion_entries.is_empty() {
        let rank = finite_rank(
            Skill::Religion
                .capped_rank_for_aptitude(skills.religion_hours.effective(primary), intelligence),
        );
        icons.push(SummaryIcon {
            label: format!("{} religion — {rank:.1}", primary.label()),
            tooltip: qualifying_tooltip("Religion", &religion_entries),
            rank,
            kind: SummaryIconKind::Mask(religion_icon_path(Some(primary.religion_id())).into()),
        });
    }

    let bestiary_entries = BestiaryCategory::ALL
        .into_iter()
        .map(|category| {
            (
                category.label(),
                finite_rank(Skill::Bestiary.capped_rank_for_aptitude(
                    skills.bestiary_hours.effective(category),
                    intelligence,
                )),
            )
        })
        .filter(|(_, rank)| *rank >= SUMMARY_SKILL_THRESHOLD)
        .collect::<Vec<_>>();
    if !bestiary_entries.is_empty() {
        let rank =
            finite_rank(Skill::Bestiary.capped_rank_for_aptitude(
                skills.bestiary_hours.aggregate_effective(),
                intelligence,
            ));
        icons.push(summary_mask_icon(
            format!("Bestiary — {rank:.1}"),
            qualifying_tooltip("Bestiary", &bestiary_entries),
            rank,
            "bestiary",
            "bestiary",
        ));
    }

    let oral_entries = OralLanguage::ALL
        .into_iter()
        .map(|language| {
            (
                language.descriptor().english,
                finite_rank((skills.oral_languages.effective(language) / 1000.0).min(instinct)),
            )
        })
        .filter(|(_, rank)| *rank >= SUMMARY_SKILL_THRESHOLD)
        .collect::<Vec<_>>();
    if !oral_entries.is_empty() {
        let strongest = strongest_oral_language(skills);
        let rank = finite_rank((skills.oral_languages.effective(strongest) / 1000.0).min(instinct));
        icons.push(SummaryIcon {
            label: format!("Oral languages — {rank:.1}"),
            tooltip: qualifying_tooltip("Oral languages", &oral_entries),
            rank,
            kind: SummaryIconKind::Monogram {
                text: "O",
                germanic_style: false,
                written: false,
            },
        });
    }

    let written_entries = WrittenLanguage::ALL
        .into_iter()
        .map(|language| {
            (
                language.descriptor().english,
                finite_rank(
                    (skills.written_languages.effective(language) / 1000.0).min(intelligence),
                ),
            )
        })
        .filter(|(_, rank)| *rank >= SUMMARY_SKILL_THRESHOLD)
        .collect::<Vec<_>>();
    if !written_entries.is_empty() {
        let strongest = strongest_written_language(skills);
        let rank =
            finite_rank((skills.written_languages.effective(strongest) / 1000.0).min(intelligence));
        icons.push(SummaryIcon {
            label: format!("Written languages — {rank:.1}"),
            tooltip: qualifying_tooltip("Written languages", &written_entries),
            rank,
            kind: SummaryIconKind::Monogram {
                text: "W",
                germanic_style: false,
                written: true,
            },
        });
    }

    push_standalone_summary_icon(
        &mut icons,
        skills,
        "Stealth",
        "stealth",
        Skill::Stealth,
        all_agility,
    );

    let terrain_entries = [
        ("Plains", Skill::TerrainPlains),
        ("Forest", Skill::TerrainForest),
        ("Hills", Skill::TerrainHills),
        ("Urban", Skill::TerrainUrban),
    ]
    .into_iter()
    .map(|(label, skill)| {
        (
            label,
            finite_rank(
                skill.capped_rank_for_aptitude(view.effective_skill_hours(skill), intelligence),
            ),
        )
    })
    .filter(|(_, rank)| *rank >= SUMMARY_SKILL_THRESHOLD)
    .collect::<Vec<_>>();
    if !terrain_entries.is_empty() {
        let rank = finite_rank(terrain_family_rank(skills, intelligence));
        icons.push(summary_mask_icon(
            format!("Terrain — {rank:.1}"),
            qualifying_tooltip("Terrain", &terrain_entries),
            rank,
            "terrain",
            "terrain",
        ));
    }

    push_standalone_summary_icon(
        &mut icons,
        skills,
        "Anatomy",
        "surgeon",
        Skill::Anatomy,
        intelligence,
    );
    push_standalone_summary_icon(
        &mut icons,
        skills,
        "Tailoring",
        "sewing-needle",
        Skill::Tailoring,
        arm_agility,
    );
    push_standalone_summary_icon(
        &mut icons,
        skills,
        "Smithing",
        "smithing",
        Skill::Smithing,
        arm_agility,
    );
    icons
}

#[derive(Clone, Debug, Default)]
pub struct ActivityPreviewRates {
    labor_gold_per_hour: f32,
    thievery_gold_per_hour: f32,
    thievery_virtue_per_hour: f32,
    raiding_gold_per_hour: f32,
    raiding_virtue_per_hour: f32,
    current_fatigue: f32,
    profession: std::collections::BTreeMap<String, ProfessionActivityPreview>,
}

#[derive(Clone, Debug)]
struct ProfessionActivityPreview {
    training_rates: Vec<(String, f32)>,
    apprenticeship_accrued: u64,
    practice_accrued: u64,
    practice_threshold: u64,
    practice_weight: u64,
    practice_reward: &'static str,
    tier_label: String,
    practice_allowed: bool,
}

const PROFESSION_ACCRUAL_SCALE: u64 = MINUTES_PER_DAY;

impl ProfessionActivityPreview {
    fn reward_delta(&self, allocation_name: &str, minutes: u16) -> [f32; 2] {
        let (accrued, threshold, sign, reward) = match allocation_name {
            "apprenticeship_minutes" => return [0.0, 0.0],
            "profession_practice_minutes" => (
                self.practice_accrued,
                self.practice_threshold,
                1.0,
                self.practice_reward,
            ),
            _ => return [0.0, 0.0],
        };
        let weight = if allocation_name == "profession_practice_minutes" {
            self.practice_weight
        } else {
            1
        };
        let after = accrued.saturating_add(
            u64::from(minutes)
                .saturating_mul(PROFESSION_ACCRUAL_SCALE)
                .saturating_mul(weight),
        );
        let delta = (after / threshold).saturating_sub(accrued / threshold) as f32 * sign;
        if reward == "virtue" {
            [0.0, delta]
        } else {
            [delta, 0.0]
        }
    }
}

impl ActivityPreviewRates {
    pub fn from_character(
        attributes: Option<&CharacterAttributes>,
        skills: Option<&CharacterSkills>,
        limbs: Option<&CharacterLimbs>,
        capability: Option<&CharacterCapability>,
        settlement: Option<&Settlement>,
        stats: Option<&CharacterStats>,
    ) -> Self {
        let current_fatigue = stats.map_or(0.0, |stats| stats.calories_used.max(0.0));
        let (Some(attributes), Some(skills), Some(limbs), Some(capability), Some(settlement)) =
            (attributes, skills, limbs, capability, settlement)
        else {
            return Self {
                current_fatigue,
                ..Self::default()
            };
        };
        let limb_health = [
            limbs.left_arm_health,
            limbs.right_arm_health,
            limbs.left_leg_health,
            limbs.right_leg_health,
        ];
        let strength = [
            attributes.left_arm_strength,
            attributes.right_arm_strength,
            attributes.left_leg_strength,
            attributes.right_leg_strength,
        ]
        .into_iter()
        .zip(limb_health)
        .map(|(value, health)| value * health.clamp(0.0, 1.0) * 0.25)
        .sum::<f32>();
        let raw_agility = [
            attributes.left_arm_agility,
            attributes.right_arm_agility,
            attributes.left_leg_agility,
            attributes.right_leg_agility,
        ]
        .into_iter()
        .map(|value| value * 0.25)
        .sum::<f32>();
        let usable_limbs = limb_health
            .into_iter()
            .map(|health| health.clamp(0.0, 1.0) * 0.25)
            .sum::<f32>();
        let stealth = Skill::Stealth.capped_rank_for_aptitude(skills.stealth_hours, raw_agility)
            * usable_limbs;
        let endurance = attributes.endurance * limbs.chest_health.clamp(0.0, 1.0);
        let population = settlement_population_scale(
            settlement.population_level,
            settlement.population_estimate,
        );
        let combat = capability
            .weapon_precision
            .max(capability.athletics)
            .max(capability.endurance);
        Self {
            labor_gold_per_hour: (strength.max(0.0) + endurance.max(0.0)) / 8.0,
            thievery_gold_per_hour: population.max(0.0) * (1.0 + stealth.max(0.0)) / 8.0,
            thievery_virtue_per_hour: -population.max(0.0) * 0.5 / (1.0 + stealth.max(0.0)),
            raiding_gold_per_hour: (2.0 + combat.max(0.0)) / 6.0,
            raiding_virtue_per_hour: -1.5,
            current_fatigue,
            profession: Default::default(),
        }
    }

    pub fn with_professions(
        mut self,
        attributes: Option<&CharacterAttributes>,
        _skills: Option<&CharacterSkills>,
        memberships: &[OrganizationMembership],
        settlement_id: &str,
        minute: u64,
    ) -> Self {
        let Some(attributes) = attributes else {
            return self;
        };
        for row in memberships {
            let Some(definition) =
                adventuresim_core::organization::organization(&row.organization_id)
            else {
                continue;
            };
            if row.status != "active"
                || minute > row.dues_paid_through_minute
                || !definition.has_chapter(settlement_id)
            {
                continue;
            }
            let Some(rank) = definition.rank(&row.rank_id) else {
                continue;
            };
            let practice_threshold =
                u64::from(rank.practice_reward_interval_minutes) * MINUTES_PER_DAY;
            let practice_reward = match definition.activity.reward {
                adventuresim_core::organization::ActivityReward::Gold => "gold",
                adventuresim_core::organization::ActivityReward::Virtue => "virtue",
            };
            self.profession.insert(
                row.organization_id.clone(),
                ProfessionActivityPreview {
                    training_rates: definition
                        .activity
                        .training
                        .iter()
                        .map(|entry| {
                            let multiplier =
                                training_target_skill(&entry.target).map_or(1.0, |skill| {
                                    aptitude_training_multiplier(character_aptitude(
                                        attributes, skill,
                                    ))
                                });
                            (
                                training_target_label(&entry.target),
                                entry.weight * multiplier,
                            )
                        })
                        .collect(),
                    apprenticeship_accrued: row.apprenticeship_minutes_accrued,
                    practice_accrued: row.practice_minutes_accrued,
                    practice_threshold,
                    practice_weight: 1,
                    practice_reward,
                    tier_label: definition
                        .rank(&row.rank_id)
                        .map_or_else(|| row.rank_id.clone(), |rank| rank.name.clone()),
                    practice_allowed: rank.practice_allowed,
                },
            );
        }
        self
    }
}

fn training_target_label(target: &adventuresim_core::organization::TrainingTarget) -> String {
    use adventuresim_core::organization::TrainingTarget;
    match target {
        TrainingTarget::FixedSkill { skill } => skill.replace('_', " "),
        TrainingTarget::Religion { religion } => format!("{religion} Religion"),
        TrainingTarget::Bestiary { category } => format!("{category} Bestiary"),
        TrainingTarget::Terrain { terrain } => format!("{terrain} Terrain"),
        TrainingTarget::EquippedWeaponSkills => "equipped weapon skills".into(),
    }
}

fn training_target_skill(
    target: &adventuresim_core::organization::TrainingTarget,
) -> Option<Skill> {
    use adventuresim_core::organization::TrainingTarget;
    match target {
        TrainingTarget::FixedSkill { skill } => match skill.as_str() {
            "will" => Some(Skill::Will),
            "insight" => Some(Skill::Insight),
            "charm" => Some(Skill::Charm),
            "command" => Some(Skill::Command),
            "deception" => Some(Skill::Deception),
            "physiology" => Some(Skill::Physiology),
            "cooking" => Some(Skill::Cooking),
            "anatomy" => Some(Skill::Anatomy),
            "polearm" => Some(Skill::Polearm),
            "axe" => Some(Skill::Axe),
            "bludgeon" => Some(Skill::Bludgeon),
            "sword" => Some(Skill::Sword),
            "knife" => Some(Skill::Knife),
            "bow" => Some(Skill::Bow),
            "crossbow" => Some(Skill::Crossbow),
            "firearm" => Some(Skill::Firearm),
            "throw" => Some(Skill::Throw),
            "block" => Some(Skill::Block),
            "dodge" => Some(Skill::Dodge),
            "stealth" => Some(Skill::Stealth),
            "balance" => Some(Skill::Balance),
            "terrain_plains" => Some(Skill::TerrainPlains),
            "terrain_forest" => Some(Skill::TerrainForest),
            "terrain_hills" => Some(Skill::TerrainHills),
            "terrain_urban" => Some(Skill::TerrainUrban),
            "tailoring" => Some(Skill::Tailoring),
            "smithing" => Some(Skill::Smithing),
            _ => None,
        },
        TrainingTarget::Religion { .. } => Some(Skill::Religion),
        TrainingTarget::Bestiary { .. } => Some(Skill::Bestiary),
        TrainingTarget::Terrain { terrain } => match terrain.as_str() {
            "plains" => Some(Skill::TerrainPlains),
            "forest" => Some(Skill::TerrainForest),
            "hills" => Some(Skill::TerrainHills),
            "urban" => Some(Skill::TerrainUrban),
            _ => None,
        },
        TrainingTarget::EquippedWeaponSkills => Some(Skill::Polearm),
    }
}

fn character_aptitude(attributes: &CharacterAttributes, skill: Skill) -> f32 {
    match skill.governing_aptitude_kind() {
        adventuresim_core::skill::GoverningAptitude::Intelligence => attributes.intelligence,
        adventuresim_core::skill::GoverningAptitude::Instinct => attributes.instinct,
        adventuresim_core::skill::GoverningAptitude::Agility(weights) => {
            attributes.left_arm_agility * weights.left_arm
                + attributes.right_arm_agility * weights.right_arm
                + attributes.left_leg_agility * weights.left_leg
                + attributes.right_leg_agility * weights.right_leg
        }
    }
}
#[derive(Clone, Copy, Default)]
pub(crate) struct CharacterSheetActions<'a> {
    pub(super) cooking_href: Option<&'a str>,
    pub(super) cooking_open: bool,
    pub(super) foraging_href: Option<&'a str>,
    pub(super) foraging_open: bool,
}

#[derive(Clone, Copy)]
pub(super) enum SkillAction<'a> {
    Get {
        href: &'a str,
        label: &'a str,
        open: bool,
    },
}

pub(super) fn skill_action_icon(
    name: &str,
    icon: &str,
    action: SkillAction<'_>,
    _inside_form: bool,
) -> Markup {
    match action {
        SkillAction::Get { href, label, open } => html! {
            a class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                href=(href) title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                data-dialog-opener=(href) {
                span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                @if open { span class="sr-only" { " (open)" } }
            }
            span class="sr-only" { (name) }
        },
    }
}

pub(super) fn party_skills_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    schedule: Option<&CharacterTrainingSchedule>,
    schedule_action: Option<&str>,
    activity_preview: Option<ActivityPreviewRates>,
    professes_religion: bool,
    prayer_religion_check: f32,
    training_religion_id: Option<&str>,
    combat_profile: CombatTrainingProfile,
    actions: CharacterSheetActions<'_>,
) -> Markup {
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let upper_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_arm_health + limbs.right_arm_health) / 2.0
    });
    let lower_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_leg_health + limbs.right_leg_health) / 2.0
    });
    html! {
        (sidebar_section("", html! {
            @if let Some(skills) = skills {
                h3 class="sr-only" { (title) }
                @if let (Some(schedule), Some(action)) = (schedule, schedule_action) {
                    form class="skill-schedule" data-skill-schedule action=(action) method="post" {
                        (skills_table(
                            title, attributes, skills, head_health, upper_health, lower_health, Some(schedule),
                            activity_preview, professes_religion, prayer_religion_check,
                            training_religion_id.and_then(OfficialReligion::from_id),
                            combat_profile, action.starts_with("/locations/settlement/"),
                            actions,
                        ))
                        div class="schedule-save-status" data-schedule-save-status role="status" aria-live="polite" hidden {
                            span { "Schedule could not be saved." }
                            button type="button" class="btn btn-secondary btn-small" data-schedule-retry { "Retry" }
                        }
                    }
                    @if action.starts_with("/locations/settlement/") {
                        (immediate_activity_dialog(&action.replace("/schedule", "/activity")))
                    }
                } @else {
                    (skills_table(
                        title, attributes, skills, head_health, upper_health, lower_health, None, None,
                        professes_religion, prayer_religion_check,
                        training_religion_id.and_then(OfficialReligion::from_id),
                        combat_profile, false,
                        actions,
                    ))
                }
            } @else {
                h3 class="sidebar-header" { (title) }
                p class="text-muted small-copy" { "Skill records have not been created yet." }
            }
        }))
    }
}

fn skills_table(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    skills: &CharacterSkills,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    activity_preview: Option<ActivityPreviewRates>,
    professes_religion: bool,
    prayer_religion_check: f32,
    training_religion: Option<OfficialReligion>,
    combat_profile: CombatTrainingProfile,
    immediate_actions: bool,
    actions: CharacterSheetActions<'_>,
) -> Markup {
    let intelligence = attributes.map_or(0.0, |value| value.intelligence);
    let instinct = attributes.map_or(0.0, |value| value.instinct);
    let arm_agility = attributes.map_or(0.0, |value| {
        (value.left_arm_agility + value.right_arm_agility) * 0.5
    });
    let leg_agility = attributes.map_or(0.0, |value| {
        (value.left_leg_agility + value.right_leg_agility) * 0.5
    });
    let all_agility = (arm_agility + leg_agility) * 0.5;
    let instinct_training = aptitude_training_multiplier(instinct);
    let intelligence_training = aptitude_training_multiplier(intelligence);
    let arm_training = aptitude_training_multiplier(arm_agility);
    let leg_training = aptitude_training_multiplier(leg_agility);
    let all_training = aptitude_training_multiplier(all_agility);
    let combat_weights = combat_profile.weights();
    let combat_weight_total = combat_weights.iter().sum::<f32>();
    let combat_training = if combat_weight_total > 0.0 {
        combat_weights
            .into_iter()
            .zip([
                arm_training,
                arm_training,
                arm_training,
                arm_training,
                arm_training,
                arm_training,
                arm_training,
                arm_training,
                arm_training,
                leg_training,
                arm_training,
                leg_training,
                instinct_training,
            ])
            .map(|(weight, multiplier)| weight * multiplier)
            .sum::<f32>()
            / combat_weight_total
    } else {
        0.0
    };
    html! {
            table class="party-skills-table" {
                colgroup {
                    col class="party-skill-name-column";
                    @if schedule.is_some() {
                        col class="schedule-effect-column";
                        col class="schedule-effect-column schedule-training-column";
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                    } @else {
                        col class="party-skill-meter-column";
                    }
                }
                @if schedule.is_some() {
                    colgroup {
                        col class="religion-auto-column";
                        col class="party-skill-time-column";
                        col class="religion-expand-column";
                    }
                } @else {
                    colgroup { col class="religion-expand-column"; }
                }
                thead { tr class="schedule-context-heading" {
                        th scope="colgroup" colspan=(if schedule.is_some() { "8" } else { "2" }) class="schedule-table-title" { (title) }
                    th scope="col" aria-label="Skill details" {}
                } }
                tbody {
                    @if skills.will_hours > 0.0 { (party_skill_row(skills, "Will", "will", Skill::Will, instinct, head_health, schedule.is_some(), None)) }
                    (social_skill_rows(skills, instinct, head_health, schedule))
                    @if skills.physiology_hours > 0.0 {
                        (party_skill_row(
                            skills,
                            "Physiology",
                            "physiology",
                            Skill::Physiology,
                            intelligence,
                            head_health,
                            schedule.is_some(),
                            None,
                        ))
                    }
                    (party_skill_row(skills, "Cooking", "cooking", Skill::Cooking, intelligence, head_health, schedule.is_some(), actions.cooking_href.map(|href| SkillAction::Get { href, label: "Open cooking menu", open: actions.cooking_open })))
                    (religion_skill_rows(skills, intelligence, head_health, schedule, training_religion))
                    (bestiary_skill_rows(skills, intelligence, head_health, schedule.is_some()))
                    (language_skill_rows(skills, instinct, intelligence, schedule.is_some()))
                    (combat_skill_rows(skills, instinct, arm_agility, leg_agility, head_health, upper_health, lower_health, schedule, combat_profile))
                    @if skills.stealth_hours > 0.0 { (party_skill_row(skills, "Stealth", "stealth", Skill::Stealth, all_agility, (upper_health + lower_health) * 0.5, schedule.is_some(), None)) }
                    (terrain_skill_rows(
                        skills,
                        intelligence,
                        schedule.is_some(),
                        actions.foraging_href.map(|href| SkillAction::Get {
                            href,
                            label: "Forage in the immediate vicinity",
                            open: actions.foraging_open,
                        }),
                    ))
                    @if skills.anatomy_hours > 0.0 { (party_skill_row(skills, "Anatomy", "surgeon", Skill::Anatomy, intelligence, head_health, schedule.is_some(), None)) }
                    @if skills.tailoring_hours > 0.0 { (party_skill_row(skills, "Tailoring", "sewing-needle", Skill::Tailoring, arm_agility, upper_health, schedule.is_some(), None)) }
                    @if skills.smithing_hours > 0.0 { (party_skill_row(skills, "Smithing", "smithing", Skill::Smithing, arm_agility, upper_health, schedule.is_some(), None)) }
                    @if let Some(schedule) = schedule {
                        @let preview = activity_preview.unwrap_or_default();
                        tr class="schedule-divider" { td colspan="9" {} }
                        tr class="schedule-section-heading" {
                            th { span class="sr-only" { "Activities" } }
                            th scope="col" title="Currency" { (schedule_header_icon("coins", "Currency")) }
                            th scope="col" title="Virtue" { (schedule_header_icon("scales", "Virtue")) }
                            th scope="col" title="Morale" { (schedule_header_icon("sun", "Morale")) }
                            th scope="col" title="Fatigue" { (schedule_header_icon("night-sleep", "Fatigue")) }
                            th scope="col" title="Effective skill-hours gained at the current daily allocation" { (schedule_header_icon("open-book", "Skill-hours")) }
                            th scope="col" {}
                            th scope="col" title="Daily allocation" { (schedule_header_icon("duration", "Daily allocation")) }
                            th scope="col" aria-label="Skill details" {}
                        }
                        (schedule_special_row(
                            if professes_religion { "Prayer" } else { "Meditate" },
                            if professes_religion { "prayer" } else { "inner-self" },
                            "prayer_minutes", schedule.downtime.prayer_minutes, true, immediate_actions,
                            if professes_religion { ActivityEffectRates::prayer(prayer_religion_check / 5.0) } else { ActivityEffectRates::meditation() }, None,
                            None, intelligence_training,
                            if professes_religion {
                                "Prayer trains the professed Religion at 25% speed; morale depends on party knowledge and satisfies Fervor-driven needs."
                            } else {
                                "Meditation gives modest morale independently of party Religion knowledge and does not train Religion or create Fervor."
                            },
                        ))
                        (schedule_special_row("Combat Training", "crossed-swords", "combat_training_minutes", schedule.downtime.combat_training_minutes, true, immediate_actions, ActivityEffectRates::default(), None, None, combat_training, "Sparring and target practice train equipped Combat skills together with Will and Balance."))
                        (schedule_special_row("Carousing", "beer-stein", "carousing_minutes", schedule.downtime.carousing_minutes, true, immediate_actions, ActivityEffectRates::carousing(), None, None, instinct_training, "Drink and socialize to improve morale and train Charm at 25% speed, at a small cost to Virtue."))
                        @let apprenticeship_id = schedule.downtime.apprenticeship_organization_id.as_deref().filter(|id| preview.profession.contains_key(*id)).or_else(|| preview.profession.keys().next().map(String::as_str));
                        @if let Some(service_id) = apprenticeship_id {
                            (schedule_organization_selection("Training organization", "apprenticeship_organization_id", service_id, preview.profession.iter().map(|(id, entry)| (id.as_str(), entry.tier_label.as_str())).collect()))
                            (schedule_special_row(&format!("Organization training — {}", profession_label(service_id)), "open-book", "apprenticeship_minutes", schedule.downtime.apprenticeship_minutes, true, immediate_actions && preview.profession.contains_key(service_id), ActivityEffectRates::default(), None, preview.profession.get(service_id), 1.0, "Train according to this organization's YAML-defined curriculum. Any dues are assessed separately."))
                        }
                        @let practice_choices: Vec<(&str, &str)> = preview.profession.iter().filter(|(_, entry)| entry.practice_allowed).map(|(id, entry)| (id.as_str(), entry.tier_label.as_str())).collect();
                        @let practice_id = schedule.downtime.practice_organization_id.as_deref().filter(|id| practice_choices.iter().any(|(candidate, _)| candidate == id)).or_else(|| practice_choices.first().map(|(id, _)| *id));
                        @if let Some(service_id) = practice_id {
                            (schedule_organization_selection("Activity organization", "practice_organization_id", service_id, practice_choices))
                            @if let Some(profession) = preview.profession.get(service_id) {
                                (schedule_special_row(&format!("Organization activity — {}", profession_label(service_id)), "shield", "profession_practice_minutes", schedule.downtime.profession_practice_minutes, true, immediate_actions, ActivityEffectRates::default(), None, Some(profession), 1.0, "Conduct the activity associated with the awarded rank. Training and rewards come from the organization's YAML definition."))
                            }
                        }
                        (schedule_special_row("Labor", "hammer-sickle", "labor_minutes", schedule.downtime.labor_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.labor_gold_per_hour, 0.0, 0.0, LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT), None, None, instinct_training, "Earn coin during settlement downtime from Strength and Endurance checks; trains Will at 25% speed and generates fatigue."))
                        (schedule_special_row("Thievery", "lockpicks", "thievery_minutes", schedule.downtime.thievery_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.thievery_gold_per_hour, preview.thievery_virtue_per_hour, 0.0, 0.0), None, None, all_training, "Settlement downtime can earn coin and risk discovery while training Stealth at 25% speed."))
                        (schedule_special_row("Raiding", "mounted-knight", "raiding_minutes", schedule.downtime.raiding_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.raiding_gold_per_hour, preview.raiding_virtue_per_hour, 0.0, 0.0), None, None, combat_training, "Settlement downtime can earn coin and risk retaliation while feeding the equipment-derived Combat training distribution at 25% speed."))
                        @let leisure = leisure_preview(&schedule.downtime, preview.current_fatigue);
                        (schedule_special_row("Leisure", "bed", "leisure_minutes", 0, false, false, ActivityEffectRates::default(), Some(leisure), None, 1.0, "Unallocated downtime first offsets baseline and activity fatigue; only surplus recovery improves morale."))
                    }
            }
        }
    }
}

fn terrain_skill_rows(
    skills: &CharacterSkills,
    aptitude: f32,
    schedule_context: bool,
    action: Option<SkillAction<'_>>,
) -> Markup {
    let entries = [
        (
            "Plains",
            "plains",
            Skill::TerrainPlains,
            skills.terrain_plains_hours,
        ),
        (
            "Forest",
            "forest",
            Skill::TerrainForest,
            skills.terrain_forest_hours,
        ),
        (
            "Hills",
            "hills",
            Skill::TerrainHills,
            skills.terrain_hills_hours,
        ),
        (
            "Urban",
            "urban",
            Skill::TerrainUrban,
            skills.terrain_urban_hours,
        ),
    ];
    let rank = terrain_family_rank(skills, aptitude);
    let average_hours = entries
        .iter()
        .map(|entry| finite_hours(entry.3))
        .sum::<f32>()
        / 4.0;
    let average_effective_hours = entries
        .iter()
        .map(|entry| CharacterSkillHours(skills).effective_skill_hours(entry.2))
        .sum::<f32>()
        / 4.0;
    html! {
        tr class="party-skill-row terrain-primary-row" data-terrain-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                @if let Some(action) = action {
                    (skill_action_icon("Terrain", "terrain", action, false))
                } @else {
                    (stat_icon("Terrain", "terrain", "terrain", false))
                }
            }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar_with_tooltip(
                    rank,
                    rank,
                    &SkillTooltip::aggregate(
                        "Terrain",
                        "Intelligence",
                        average_hours,
                        average_effective_hours,
                    ),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-terrain-expand aria-expanded="false" aria-label="Expand Terrain skills" title="Expand Terrain" {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for (name, icon, skill, _hours) in entries {
            tr class="party-skill-row terrain-detail-row" data-terrain-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    (stat_icon(name, "terrain", icon, false))
                }
                td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                    @let effective_hours = CharacterSkillHours(skills).effective_skill_hours(skill);
                    @let uncapped = skill.training_rank(effective_hours);
                    @let sub_rank = skill.capped_rank_for_aptitude(effective_hours, aptitude);
                    (skill_rank_bar_with_tooltip(uncapped, sub_rank, &SkillTooltip::ordinary(skills, skill), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn language_skill_rows(
    skills: &CharacterSkills,
    oral_aptitude: f32,
    written_aptitude: f32,
    schedule_context: bool,
) -> Markup {
    use adventuresim_world_schema::{OralLanguage, WrittenLanguage};
    let oral_effective = OralLanguage::ALL
        .into_iter()
        .map(|language| skills.oral_languages.effective(language))
        .fold(0.0, f32::max);
    let written_effective = WrittenLanguage::ALL
        .into_iter()
        .map(|language| skills.written_languages.effective(language))
        .fold(0.0, f32::max);
    html! {
        @for (family, effective, kind) in [("Oral",oral_effective,"oral"),("Written",written_effective,"written")] {
            @if effective.is_finite() && effective > 0.0 {
                tr class=(format!("party-skill-row language-primary-row language-{kind}")) {
                    th scope="row" class="party-skill-name party-skill-icon-cell" { span class=(format!("language-monogram language-{kind}")) title=(format!("{family} languages")) aria-hidden="true" { (if kind=="oral" {"O"} else {"W"}) } span class="sr-only" { (family) } }
                    td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let aptitude=if kind=="oral" {oral_aptitude} else {written_aptitude}; @let tooltip=if kind=="oral" { oral_language_family_tooltip(skills) } else { written_language_family_tooltip(skills) }; @let rank=(effective/1000.0).clamp(0.0,5.0); (skill_rank_bar_with_tooltip(rank,rank.min(aptitude.clamp(0.0,5.0)),&tooltip,skill_rail_bar_options())) }
                    td class="religion-expand-cell" { button type="button" class="religion-expand-button" data-language-expand=(kind) aria-expanded="false" aria-label=(format!("Expand {family} languages")) { span class="religion-expand-chevron" aria-hidden="true" { "›" } } }
                }
                @if kind=="oral" { @for language in OralLanguage::ALL { @let descriptor=language.descriptor(); @let effective=skills.oral_languages.effective(language);
                    @if effective.is_finite() && effective > 0.0 {
                        tr class="party-skill-row language-detail-row" data-language-detail="oral" hidden { th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" { span class=(if descriptor.germanic_style {"language-monogram language-oral language-blackletter"} else {"language-monogram language-oral"}) title=(format!("{} — {}",descriptor.english,descriptor.native)) aria-hidden="true" { (descriptor.monogram) } span class="sr-only" { (descriptor.english) } } td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let rank=(effective/1000.0).clamp(0.0,5.0); (skill_rank_bar_with_tooltip(rank,rank.min(oral_aptitude.clamp(0.0,5.0)),&oral_language_tooltip(skills, language),skill_rail_bar_options())) } td class="religion-expand-cell" {} }
                    }
                }} @else { @for language in WrittenLanguage::ALL { @let descriptor=language.descriptor(); @let effective=skills.written_languages.effective(language);
                    @if effective.is_finite() && effective > 0.0 {
                        tr class="party-skill-row language-detail-row" data-language-detail="written" hidden { th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" { span class=(if descriptor.germanic_style {"language-monogram language-written language-blackletter"} else {"language-monogram language-written"}) title=(format!("{} — {}",descriptor.english,descriptor.native)) aria-hidden="true" { (descriptor.monogram) } span class="sr-only" { (descriptor.english) } } td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let rank=(effective/1000.0).clamp(0.0,5.0); (skill_rank_bar_with_tooltip(rank,rank.min(written_aptitude.clamp(0.0,5.0)),&written_language_tooltip(skills, language),skill_rail_bar_options())) } td class="religion-expand-cell" {} }
                    }
                }}
            }
        }
    }
}

fn oral_language_tooltip(
    skills: &CharacterSkills,
    language: adventuresim_world_schema::OralLanguage,
) -> SkillTooltip {
    use adventuresim_world_schema::OralLanguage;
    SkillTooltip::new(
        language.descriptor().english,
        "Instinct",
        skills.oral_languages.direct(language),
        skills.oral_languages.effective(language),
        OralLanguage::ALL
            .into_iter()
            .filter(move |source| *source != language)
            .map(move |source| (source.descriptor().english, language.correlation(source))),
    )
}

fn oral_language_family_tooltip(skills: &CharacterSkills) -> SkillTooltip {
    let strongest = strongest_oral_language(skills);
    let mut tooltip = oral_language_tooltip(skills, strongest);
    tooltip.name = "Oral languages".into();
    tooltip
}

fn written_language_tooltip(
    skills: &CharacterSkills,
    language: adventuresim_world_schema::WrittenLanguage,
) -> SkillTooltip {
    use adventuresim_world_schema::WrittenLanguage;
    SkillTooltip::new(
        language.descriptor().english,
        "Intelligence",
        skills.written_languages.direct(language),
        skills.written_languages.effective(language),
        WrittenLanguage::ALL
            .into_iter()
            .filter(move |source| *source != language)
            .map(move |source| (source.descriptor().english, language.correlation(source))),
    )
}

fn written_language_family_tooltip(skills: &CharacterSkills) -> SkillTooltip {
    let strongest = strongest_written_language(skills);
    let mut tooltip = written_language_tooltip(skills, strongest);
    tooltip.name = "Written languages".into();
    tooltip
}

fn religion_skill_rows(
    skills: &CharacterSkills,
    aptitude: f32,
    health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    training_religion: Option<OfficialReligion>,
) -> Markup {
    if !OfficialReligion::ALL.into_iter().any(|religion| {
        let direct = skills.religion_hours.direct(religion);
        direct.is_finite() && direct > 0.0
    }) {
        return html! {};
    }
    let primary = primary_religion(skills, training_religion);
    let primary_id = primary.religion_id();
    let primary_effective = skills.religion_hours.effective(primary);
    let has_details = OfficialReligion::ALL.into_iter().any(|religion| {
        let direct = skills.religion_hours.direct(religion);
        religion != primary && direct.is_finite() && direct > 0.0
    });
    html! {
        tr class="party-skill-row religion-primary-row" data-religion-primary=(primary_id) {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                span class="religion-tradition-icon" title=(primary.label()) {
                    (religion_icon(primary.label(), Some(primary_id), false))
                }
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar_with_tooltip(
                    Skill::Religion.training_rank(primary_effective),
                    Skill::Religion.capped_rank_for_aptitude(primary_effective, aptitude) * health.clamp(0.0, 1.0),
                    &religion_tooltip(skills, primary),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {
                @if has_details {
                    (religion_expand_button(primary))
                }
            }
        }
        @for religion in OfficialReligion::ALL {
          @let direct = skills.religion_hours.direct(religion);
          @if religion != primary && direct.is_finite() && direct > 0.0 {
            @let id = religion.religion_id();
            @let effective = skills.religion_hours.effective(religion);
            tr class="party-skill-row religion-detail-row" data-religion-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    span class="religion-tradition-icon" {
                        (religion_icon(religion.label(), Some(id), false))
                    }
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    (skill_rank_bar_with_tooltip(
                        Skill::Religion.training_rank(effective),
                        Skill::Religion.capped_rank_for_aptitude(effective, aptitude) * health.clamp(0.0, 1.0),
                        &religion_tooltip(skills, religion),
                        skill_rail_bar_options(),
                    ))
                }
                td class="religion-expand-cell" {}
            }
          }
        }
    }
}

fn religion_tooltip(skills: &CharacterSkills, religion: OfficialReligion) -> SkillTooltip {
    SkillTooltip::new(
        religion.label(),
        Skill::Religion.governing_aptitude_kind().label(),
        skills.religion_hours.direct(religion),
        skills.religion_hours.effective(religion),
        OfficialReligion::ALL
            .into_iter()
            .filter(move |source| *source != religion)
            .map(move |source| (source.label(), religion.correlation(source))),
    )
}

fn bestiary_category_enemies(category: BestiaryCategory) -> (String, String) {
    let enemies = crate::spacetimedb::bestiary_enemy_lore(category);
    let primary_names = enemies
        .iter()
        .filter(|enemy| enemy.is_primary)
        .map(|enemy| enemy.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let secondary_names = enemies
        .iter()
        .filter(|enemy| !enemy.is_primary)
        .map(|enemy| enemy.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut relationships = Vec::new();
    if !primary_names.is_empty() {
        relationships.push(format!("Main type for: {primary_names}."));
    }
    if !secondary_names.is_empty() {
        relationships.push(format!("Secondary type for: {secondary_names}."));
    }
    (
        serde_json::to_string(&enemies).expect("Bestiary enemy lore serializes"),
        if relationships.is_empty() {
            "No current enemy types".into()
        } else {
            relationships.join(" ")
        },
    )
}

fn bestiary_skill_rows(
    skills: &CharacterSkills,
    aptitude: f32,
    health: f32,
    schedule_context: bool,
) -> Markup {
    if !BestiaryCategory::ALL
        .into_iter()
        .any(|category| skills.bestiary_hours.direct(category) > 0.0)
    {
        return html! {};
    }
    let aggregate_effective = skills.bestiary_hours.aggregate_effective();
    html! {
        tr class="party-skill-row skill-family-primary-row bestiary-primary-row"
            data-skill-family="bestiary" data-bestiary-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon("Bestiary", "bestiary", "bestiary", false))
            }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar_with_tooltip(
                    Skill::Bestiary.training_rank(aggregate_effective),
                    Skill::Bestiary.capped_rank_for_aptitude(aggregate_effective, aptitude) * health.clamp(0.0, 1.0),
                    &bestiary_family_tooltip(skills),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-bestiary-expand
                    aria-expanded="false" aria-label="Expand Bestiary skills" title="Expand Bestiary" {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for category in BestiaryCategory::ALL {
            @let effective = skills.bestiary_hours.effective(category);
            @if effective.is_finite() && effective > 0.0 {
                @let (enemies, applies_to) = bestiary_category_enemies(category);
                tr class="party-skill-row bestiary-detail-row" data-bestiary-detail hidden {
                    th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                        span class="bestiary-lore-trigger" data-strategic-tooltip=(category.label())
                            data-tooltip-pinnable data-bestiary-enemies=(&enemies)
                            tabindex="0" role="button" aria-pressed="false"
                            data-bestiary-name=(category.label())
                            aria-label=(format!("{} knowledge. {}", category.label(), applies_to)) {
                            (stat_icon(category.label(), "bestiary", category.id(), true))
                            span class="sr-only" { (category.label()) }
                        }
                    }
                    td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                        (skill_rank_bar_with_tooltip(
                            Skill::Bestiary.training_rank(effective),
                            Skill::Bestiary.capped_rank_for_aptitude(effective, aptitude) * health.clamp(0.0, 1.0),
                            &bestiary_tooltip(skills, category),
                            skill_rail_bar_options(),
                        ))
                    }
                    td class="religion-expand-cell" {}
                }
            }
        }
    }
}

fn bestiary_family_tooltip(skills: &CharacterSkills) -> SkillTooltip {
    SkillTooltip::aggregate(
        Skill::Bestiary.label(),
        Skill::Bestiary.governing_aptitude_kind().label(),
        skills.bestiary_hours.total_direct() / BestiaryCategory::ALL.len() as f32,
        skills.bestiary_hours.aggregate_effective(),
    )
}

fn bestiary_tooltip(skills: &CharacterSkills, category: BestiaryCategory) -> SkillTooltip {
    SkillTooltip::new(
        category.label(),
        Skill::Bestiary.governing_aptitude_kind().label(),
        skills.bestiary_hours.direct(category),
        skills.bestiary_hours.effective(category),
        BestiaryCategory::ALL
            .into_iter()
            .filter(move |source| *source != category)
            .map(move |source| (source.label(), category.correlation(source))),
    )
}

fn social_skill_rows(
    skills: &CharacterSkills,
    aptitude: f32,
    health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
) -> Markup {
    let entries = [
        ("Insight", "insight", Skill::Insight, skills.insight_hours),
        ("Charm", "charm", Skill::Charm, skills.charm_hours),
        ("Command", "command", Skill::Command, skills.command_hours),
        (
            "Deception",
            "deception",
            Skill::Deception,
            skills.deception_hours,
        ),
    ];
    if entries.iter().all(|entry| entry.3 <= 0.0) {
        return html! {};
    }
    let rank = social_family_rank(skills, aptitude);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    let average_hours = entries
        .iter()
        .map(|entry| finite_hours(entry.3))
        .sum::<f32>()
        / entries.len() as f32;
    html! {
        tr class="party-skill-row social-primary-row" data-social-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon("Social", "skills", "social", false))
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar_with_tooltip(
                    rank,
                    effective_rank,
                    &SkillTooltip::aggregate(
                        "Social",
                        "Instinct",
                        average_hours,
                        average_hours,
                    ),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-social-expand
                    aria-expanded="false" aria-label="Expand Social skills" title="Expand Social" {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for (name, icon, skill, hours) in entries {
            tr class="party-skill-row social-detail-row" data-social-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    (stat_icon(name, "skills", icon, false))
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    @let uncapped = skill.training_rank(hours);
                    @let sub_rank = skill.capped_rank_for_aptitude(hours, aptitude);
                    (skill_rank_bar_with_tooltip(uncapped, sub_rank * health.clamp(0.0, 1.0), &SkillTooltip::direct(skill, hours), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn combat_skill_rows(
    skills: &CharacterSkills,
    instinct: f32,
    arm_agility: f32,
    leg_agility: f32,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    profile: CombatTrainingProfile,
) -> Markup {
    let weights = profile.weights();
    html! {
        (combat_meta_group(skills, "Melee", "crossed-swords", schedule, &[
            ("Polearm", "spear-hook", Skill::Polearm, skills.polearm_hours, arm_agility, upper_health, weights[0]),
            ("Axe", "battle-axe", Skill::Axe, skills.axe_hours, arm_agility, upper_health, weights[1]),
            ("Bludgeon", "flanged-mace", Skill::Bludgeon, skills.bludgeon_hours, arm_agility, upper_health, weights[2]),
            ("Sword", "sword", Skill::Sword, skills.sword_hours, arm_agility, upper_health, weights[3]),
            ("Knife", "bowie-knife", Skill::Knife, skills.knife_hours, arm_agility, upper_health, weights[4]),
        ]))
        (combat_meta_group(skills, "Ranged", "archery-target", schedule, &[
            ("Bow", "bow-arrow", Skill::Bow, skills.bow_hours, arm_agility, upper_health, weights[5]),
            ("Crossbow", "crossbow", Skill::Crossbow, skills.crossbow_hours, arm_agility, upper_health, weights[6]),
            ("Firearm", "musket", Skill::Firearm, skills.firearm_hours, arm_agility, upper_health, weights[7]),
            ("Throw", "throwing-ball", Skill::Throw, skills.throw_hours, arm_agility, upper_health, weights[8]),
        ]))
        (combat_meta_group(skills, "Defense", "shield", schedule, &[
            ("Dodge", "dodge", Skill::Dodge, skills.dodge_hours, leg_agility, lower_health, weights[9]),
            ("Block", "block", Skill::Block, skills.block_hours, arm_agility, upper_health, weights[10]),
            ("Balance", "balance", Skill::Balance, skills.balance_hours, leg_agility, lower_health, weights[11]),
            ("Will", "will", Skill::Will, skills.will_hours, instinct, head_health, weights[12]),
        ]))
    }
}

fn combat_meta_group(
    skills: &CharacterSkills,
    name: &str,
    icon: &str,
    schedule: Option<&CharacterTrainingSchedule>,
    entries: &[(&str, &str, Skill, f32, f32, f32, f32)],
) -> Markup {
    let relevant: Vec<_> = entries.iter().filter(|entry| entry.6 > 0.0).collect();
    let rank = relevant
        .iter()
        .map(|entry| {
            entry.2.capped_rank_for_aptitude(
                CharacterSkillHours(skills).effective_skill_hours(entry.2),
                entry.4,
            )
        })
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let effective_rank = relevant
        .iter()
        .map(|entry| {
            entry.2.capped_rank_for_aptitude(
                CharacterSkillHours(skills).effective_skill_hours(entry.2),
                entry.4,
            ) * entry.5.clamp(0.0, 1.0)
        })
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let average_hours = relevant
        .iter()
        .map(|entry| finite_hours(entry.3))
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let average_effective_hours = relevant
        .iter()
        .map(|entry| CharacterSkillHours(skills).effective_skill_hours(entry.2))
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let governed_by = if relevant
        .iter()
        .all(|entry| entry.2.governing_aptitude_kind().label() == "Agility")
    {
        "Agility"
    } else {
        "Agility and Instinct"
    };
    html! {
        tr class="party-skill-row combat-primary-row" data-combat-primary=(name.to_ascii_lowercase()) {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon(name, "skills", icon, false))
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar_with_tooltip(
                    rank,
                    effective_rank,
                    &SkillTooltip::aggregate(
                        name,
                        governed_by,
                        average_hours,
                        average_effective_hours,
                    ),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-combat-expand=(name.to_ascii_lowercase())
                    aria-expanded="false" aria-label=(format!("Expand {name} skills")) title=(format!("Expand {name}")) {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for &(leaf_name, leaf_icon, skill, _hours, aptitude, health, weight) in entries {
            tr class="party-skill-row combat-detail-row" data-combat-detail=(name.to_ascii_lowercase()) data-combat-weight=(weight) hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    span title=[(skill == Skill::Knife).then_some("Knife means short weapons: knives, daggers, and short blades.")] {
                        (stat_icon(leaf_name, "skills", leaf_icon, false))
                    }
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    @let effective_hours = CharacterSkillHours(skills).effective_skill_hours(skill);
                    @let uncapped = skill.training_rank(effective_hours);
                    @let sub_rank = skill.capped_rank_for_aptitude(effective_hours, aptitude);
                    (skill_rank_bar_with_tooltip(uncapped, sub_rank * health.clamp(0.0, 1.0), &SkillTooltip::ordinary(skills, skill), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn religion_expand_button(primary: OfficialReligion) -> Markup {
    html! {
        button type="button" class="religion-expand-button" data-religion-expand
            aria-expanded="false"
            aria-label=(format!("Expand {} Religion skill", primary.label()))
            title=(format!("Expand {}", primary.label())) {
            span class="religion-expand-chevron" aria-hidden="true" { "›" }
        }
    }
}

fn schedule_header_icon(icon: &str, label: &str) -> Markup {
    html! { span class="schedule-header-icon" { (game_icon(label, icon)) } }
}

fn party_skill_row(
    skills: &CharacterSkills,
    name: &str,
    icon: &str,
    skill: Skill,
    aptitude: f32,
    health: f32,
    schedule_context: bool,
    action: Option<SkillAction<'_>>,
) -> Markup {
    let view = CharacterSkillHours(skills);
    let effective_hours = view.effective_skill_hours(skill);
    let uncapped_rank = skill.training_rank(effective_hours);
    let rank = skill.capped_rank_for_aptitude(effective_hours, aptitude);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    html! {
        tr class="party-skill-row" {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                @if let Some(action) = action {
                    (skill_action_icon(name, icon, action, schedule_context))
                } @else {
                    (stat_icon(name, "skills", icon, false))
                }
            }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar_with_tooltip(
                    uncapped_rank,
                    effective_rank,
                    &SkillTooltip::ordinary(skills, skill),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {}
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct SkillCorrelationTooltip {
    name: String,
    percent: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
struct SkillTooltip {
    name: String,
    governed_by: String,
    trained_hours: f32,
    correlated_hours: f32,
    correlations: Vec<SkillCorrelationTooltip>,
}

impl SkillTooltip {
    fn new(
        name: impl Into<String>,
        governed_by: impl Into<String>,
        trained_hours: f32,
        effective_hours: f32,
        correlations: impl IntoIterator<Item = (impl Into<String>, f32)>,
    ) -> Self {
        let trained_hours = finite_hours(trained_hours);
        let effective_hours = finite_hours(effective_hours);
        Self {
            name: name.into(),
            governed_by: governed_by.into(),
            trained_hours,
            correlated_hours: (effective_hours - trained_hours).max(0.0),
            correlations: correlations
                .into_iter()
                .filter_map(|(name, rate)| {
                    (rate.is_finite() && rate > 0.0).then(|| SkillCorrelationTooltip {
                        name: name.into(),
                        percent: rate * 100.0,
                    })
                })
                .collect(),
        }
    }

    fn direct(skill: Skill, trained_hours: f32) -> Self {
        Self::new(
            skill.label(),
            skill.governing_aptitude_kind().label(),
            trained_hours,
            trained_hours,
            std::iter::empty::<(&str, f32)>(),
        )
    }

    fn ordinary(skills: &CharacterSkills, skill: Skill) -> Self {
        let view = CharacterSkillHours(skills);
        Self::new(
            skill.label(),
            skill.governing_aptitude_kind().label(),
            view.skill_hours_trained(skill),
            view.effective_skill_hours(skill),
            skill
                .ordinary_correlations()
                .iter()
                .map(|(source, rate)| (source.label(), *rate)),
        )
    }

    fn aggregate(
        name: impl Into<String>,
        governed_by: impl Into<String>,
        trained_hours: f32,
        effective_hours: f32,
    ) -> Self {
        Self::new(
            name,
            governed_by,
            trained_hours,
            effective_hours,
            std::iter::empty::<(&str, f32)>(),
        )
    }

    fn accessible_description(&self) -> String {
        let mut description = format!(
            "{}\nGoverned by {}\n{:.1} direct hours trained\n{:.1} effective hours trained\n{:.1} hours from correlated skills:",
            self.name,
            self.governed_by,
            self.trained_hours,
            self.trained_hours + self.correlated_hours,
            self.correlated_hours
        );
        for correlation in &self.correlations {
            description.push_str(&format!(
                "\n{} | {:.0}%",
                correlation.name, correlation.percent
            ));
        }
        description
    }
}

fn finite_hours(hours: f32) -> f32 {
    if hours.is_finite() {
        hours.max(0.0)
    } else {
        0.0
    }
}

struct CharacterSkillHours<'a>(&'a CharacterSkills);

impl PlayerSkills for CharacterSkillHours<'_> {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        let skills = self.0;
        match skill {
            Skill::Polearm => skills.polearm_hours,
            Skill::Axe => skills.axe_hours,
            Skill::Bludgeon => skills.bludgeon_hours,
            Skill::Sword => skills.sword_hours,
            Skill::Knife => skills.knife_hours,
            Skill::Dodge => skills.dodge_hours,
            Skill::Block => skills.block_hours,
            Skill::Bow => skills.bow_hours,
            Skill::Crossbow => skills.crossbow_hours,
            Skill::Firearm => skills.firearm_hours,
            Skill::Throw => skills.throw_hours,
            Skill::Will => skills.will_hours,
            Skill::Insight => skills.insight_hours,
            Skill::Charm => skills.charm_hours,
            Skill::Command => skills.command_hours,
            Skill::Deception => skills.deception_hours,
            Skill::Physiology => skills.physiology_hours,
            Skill::Cooking => skills.cooking_hours,
            Skill::Stealth => skills.stealth_hours,
            Skill::Balance => skills.balance_hours,
            Skill::TerrainPlains => skills.terrain_plains_hours,
            Skill::TerrainForest => skills.terrain_forest_hours,
            Skill::TerrainHills => skills.terrain_hills_hours,
            Skill::TerrainUrban => skills.terrain_urban_hours,
            Skill::Anatomy => skills.anatomy_hours,
            Skill::Tailoring => skills.tailoring_hours,
            Skill::Smithing => skills.smithing_hours,
            Skill::Religion | Skill::Bestiary => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SkillRankBarOptions<'a> {
    pub(super) show_value: bool,
    pub(super) extra_class: Option<&'a str>,
    pub(super) aria_label: Option<&'a str>,
}

impl Default for SkillRankBarOptions<'_> {
    fn default() -> Self {
        Self {
            show_value: true,
            extra_class: None,
            aria_label: None,
        }
    }
}

fn skill_rail_bar_options() -> SkillRankBarOptions<'static> {
    SkillRankBarOptions {
        show_value: false,
        ..SkillRankBarOptions::default()
    }
}

pub(super) fn skill_rank_bar(
    rank: f32,
    effective_rank: f32,
    title: &str,
    options: SkillRankBarOptions<'_>,
) -> Markup {
    skill_rank_bar_markup(rank, effective_rank, Some(title), None, options)
}

fn skill_rank_bar_with_tooltip(
    rank: f32,
    effective_rank: f32,
    tooltip: &SkillTooltip,
    options: SkillRankBarOptions<'_>,
) -> Markup {
    skill_rank_bar_markup(rank, effective_rank, None, Some(tooltip), options)
}

fn skill_rank_bar_markup(
    rank: f32,
    effective_rank: f32,
    title: Option<&str>,
    skill_tooltip: Option<&SkillTooltip>,
    options: SkillRankBarOptions<'_>,
) -> Markup {
    let rank = rank.clamp(0.0, 5.0);
    let effective_rank = effective_rank.clamp(0.0, rank);
    let class = options.extra_class.map_or_else(
        || "skill-rank-bar".to_owned(),
        |extra| format!("skill-rank-bar {extra}"),
    );
    let aria_label = options
        .aria_label
        .map_or_else(|| format!("{effective_rank:.1} out of 5"), str::to_owned);
    let tooltip_description = skill_tooltip.map(SkillTooltip::accessible_description);
    let tooltip_json = skill_tooltip
        .map(|tooltip| serde_json::to_string(tooltip).expect("skill tooltip data serializes"));
    html! {
        div class=(class) title=[title] aria-label=(aria_label)
            data-strategic-tooltip=[tooltip_description]
            data-skill-tooltip=[tooltip_json]
            tabindex=[skill_tooltip.map(|_| "0")]
            role="meter" aria-valuemin="0" aria-valuemax="5" aria-valuenow=(format!("{effective_rank:.1}")) {
            span class="skill-rank-track" aria-hidden="true" {
                @for tier in 1..=5 {
                    @let offset = (tier - 1) as f32;
                    @let current = (effective_rank - offset).clamp(0.0, 1.0) * 100.0;
                    @let trained = (rank - offset).clamp(0.0, 1.0) * 100.0;
                    @let damaged = (trained - current).max(0.0);
                    span class=(format!("skill-rank-segment skill-rank-segment-{tier}")) {
                        span class="rank-current" style=(format!("width:{current:.1}%")) {}
                        span class="rank-damage" style=(format!("left:{current:.1}%;width:{damaged:.1}%")) {}
                    }
                }
            }
            @if options.show_value {
                span class="skill-rank-value" aria-hidden="true" { (format!("{effective_rank:.1}")) }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ActivityEffectRates {
    gold_per_hour: f32,
    virtue_per_hour: f32,
    morale_per_hour: f32,
    fatigue_per_hour: f32,
    prayer_morale: bool,
    prayer_morale_multiplier: f32,
    morale_limit: f32,
    morale_scale_minutes: f32,
}

#[derive(Clone, Copy, Debug)]
struct LeisurePreview {
    current_fatigue: f32,
    outcome: LeisureOutcome,
    fatigue_display: f32,
}

fn core_daily_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        combat_training_minutes: schedule.combat_training_minutes,
        carousing_minutes: schedule.carousing_minutes,
        apprenticeship_minutes: schedule.apprenticeship_minutes,
        profession_practice_minutes: schedule.profession_practice_minutes,
        labor: schedule.labor_minutes,
        prayer: schedule.prayer_minutes,
        thievery: schedule.thievery_minutes,
        raiding: schedule.raiding_minutes,
    }
}

fn leisure_preview(schedule: &ScheduleAllocation, current_fatigue: f32) -> LeisurePreview {
    let outcome = settlement_leisure_outcome(
        core_daily_schedule(schedule),
        MINUTES_PER_DAY,
        current_fatigue,
    );
    let labor_fatigue = f32::from(schedule.labor_minutes) / 60.0 * LABOR_FATIGUE_PER_HOUR;
    LeisurePreview {
        current_fatigue,
        outcome,
        fatigue_display: (outcome.fatigue_delta - labor_fatigue)
            / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
    }
}

impl ActivityEffectRates {
    const fn linear(gold: f32, virtue: f32, morale: f32, fatigue: f32) -> Self {
        Self {
            gold_per_hour: gold,
            virtue_per_hour: virtue,
            morale_per_hour: morale,
            fatigue_per_hour: fatigue,
            prayer_morale: false,
            prayer_morale_multiplier: 1.0,
            morale_limit: PRAYER_MORALE_LIMIT,
            morale_scale_minutes: PRAYER_MORALE_SCALE_MINUTES,
        }
    }

    fn prayer(multiplier: f32) -> Self {
        Self {
            prayer_morale: true,
            prayer_morale_multiplier: multiplier.clamp(0.0, 1.0),
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    const fn meditation() -> Self {
        Self {
            prayer_morale_multiplier: 0.25,
            prayer_morale: true,
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    const fn carousing() -> Self {
        Self {
            gold_per_hour: 0.0,
            virtue_per_hour: -0.125,
            prayer_morale: true,
            prayer_morale_multiplier: 1.0,
            morale_limit: adventuresim_core::activity::CAROUSING_MORALE_LIMIT,
            morale_scale_minutes: adventuresim_core::activity::CAROUSING_MORALE_SCALE_MINUTES,
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    fn values(self, minutes: u16) -> [f32; 4] {
        let hours = f32::from(minutes) / 60.0;
        let morale = if self.prayer_morale {
            self.prayer_morale_multiplier
                * self.morale_limit
                * (1.0 - (-f32::from(minutes) / self.morale_scale_minutes).exp())
        } else {
            self.morale_per_hour * hours
        };
        [
            (self.gold_per_hour * hours).round(),
            self.virtue_per_hour * hours,
            morale,
            self.fatigue_per_hour * hours,
        ]
    }
}

fn activity_effect_cell(kind: &str, value: f32) -> Markup {
    let rounded = if kind == "gold" {
        value.round()
    } else {
        (value * 10.0).round() / 10.0
    };
    let state = if rounded > 0.0 {
        "positive"
    } else if rounded < 0.0 {
        "negative"
    } else {
        "neutral"
    };
    let display = if state == "neutral" {
        "0".to_string()
    } else if kind == "gold" {
        format!("{rounded:+.0}")
    } else {
        format!("{rounded:+.1}")
    };
    html! {
        td class=(format!("schedule-effect schedule-effect-{state}")) data-activity-effect=(kind) {
            (display)
        }
    }
}

fn activity_training_cell(
    label: &str,
    allocation_name: &str,
    minutes: u16,
    profession: Option<&ProfessionActivityPreview>,
    training_multiplier: f32,
) -> Markup {
    let hours = f32::from(minutes) / 60.0;
    let rates: Vec<(String, f32)> = match allocation_name {
        "combat_training_minutes" => {
            vec![("Relevant combat skills".into(), training_multiplier)]
        }
        "carousing_minutes" => vec![("Charm".into(), 0.25 * training_multiplier)],
        "labor_minutes" => vec![("Will".into(), 0.25 * training_multiplier)],
        "thievery_minutes" => vec![("Stealth".into(), 0.25 * training_multiplier)],
        "raiding_minutes" => vec![("Relevant combat skills".into(), 0.25 * training_multiplier)],
        "prayer_minutes" if label == "Prayer" => {
            vec![("Religion".into(), 0.25 * training_multiplier)]
        }
        "apprenticeship_minutes" | "profession_practice_minutes" => profession
            .map(|preview| preview.training_rates.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let breakdown = rates
        .iter()
        .map(|(skill, rate)| (skill.clone(), hours * rate))
        .collect::<Vec<_>>();
    let total = breakdown.iter().map(|(_, value)| value).sum::<f32>();
    let title = if breakdown.is_empty() {
        "No skill training".to_string()
    } else {
        breakdown
            .iter()
            .map(|(skill, value)| format!("{skill}: +{value:.2}h"))
            .collect::<Vec<_>>()
            .join("; ")
    };
    html! {
        td class="schedule-effect schedule-training-effect" data-activity-effect="training"
            data-training-rates=(rates.iter().map(|(skill, rate)| format!("{skill}={rate}")).collect::<Vec<_>>().join("|"))
            title=(title) aria-label=(format!("Effective skill training: {total:.2} hours")) {
            @if total > 0.0 { (format!("+{total:.2}h")) } @else { "—" }
        }
    }
}

fn schedule_special_row(
    label: &str,
    icon: &str,
    allocation_name: &str,
    allocation_minutes: u16,
    editable: bool,
    actionable: bool,
    effects: ActivityEffectRates,
    leisure: Option<LeisurePreview>,
    profession: Option<&ProfessionActivityPreview>,
    training_multiplier: f32,
    description: &str,
) -> Markup {
    let mut values = leisure.map_or_else(
        || effects.values(allocation_minutes),
        |preview| [0.0, 0.0, preview.outcome.morale, preview.fatigue_display],
    );
    if let Some(profession) = profession {
        let reward = profession.reward_delta(allocation_name, allocation_minutes);
        values[0] = reward[0];
        values[1] = reward[1];
    }
    html! {
        tr class="party-skill-row schedule-special-row" title=(description)
            data-activity-row data-activity-allocation=(allocation_name)
            data-gold-rate=(effects.gold_per_hour)
            data-virtue-rate=(effects.virtue_per_hour)
            data-morale-rate=(effects.morale_per_hour)
            data-fatigue-rate=(effects.fatigue_per_hour)
            data-prayer-morale=[effects.prayer_morale.then_some("true")]
            data-prayer-morale-limit=[effects.prayer_morale.then_some(effects.morale_limit)]
            data-prayer-morale-scale=[effects.prayer_morale.then_some(effects.morale_scale_minutes)]
            data-prayer-morale-multiplier=[effects.prayer_morale.then_some(effects.prayer_morale_multiplier)]
            data-profession-accrued=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { preview.apprenticeship_accrued } else { preview.practice_accrued })]
            data-profession-threshold=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { u64::MAX } else { preview.practice_threshold })]
            data-profession-reward=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { "gold" } else { preview.practice_reward })]
            data-profession-sign=[profession.map(|_| if allocation_name == "apprenticeship_minutes" { -1 } else { 1 })]
            data-profession-tier=[profession.map(|preview| preview.tier_label.as_str())]
            data-leisure-current-fatigue=[leisure.map(|preview| preview.current_fatigue)]
            data-leisure-baseline-fatigue=[leisure.map(|_| BASELINE_FATIGUE_PER_DAY)]
            data-leisure-labor-fatigue-rate=[leisure.map(|_| LABOR_FATIGUE_PER_HOUR)]
            data-leisure-recovery-rate=[leisure.map(|_| LEISURE_FATIGUE_RECOVERY_PER_HOUR)]
            data-leisure-morale-limit=[leisure.map(|_| LEISURE_MORALE_LIMIT)]
            data-leisure-morale-scale=[leisure.map(|_| LEISURE_MORALE_SCALE_FATIGUE)]
            data-leisure-fatigue-preview-divisor=[leisure.map(|_| FATIGUE_RESERVOIR_PER_PREVIEW_POINT)] {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (schedule_icon(label, icon, actionable, allocation_name))
                span class="sr-only" { (label) }
            }
            (activity_effect_cell("gold", values[0]))
            (activity_effect_cell("virtue", values[1]))
            (activity_effect_cell("morale", values[2]))
            (activity_effect_cell("fatigue", values[3]))
            (activity_training_cell(
                label,
                allocation_name,
                allocation_minutes,
                profession,
                training_multiplier,
            ))
            td class="religion-auto-toggle-cell" {}
            (schedule_allocation_cell(allocation_name, allocation_minutes, editable))
            td class="religion-expand-cell" {}
        }
    }
}

fn schedule_organization_selection(
    label: &str,
    name: &str,
    selected_id: &str,
    choices: Vec<(&str, &str)>,
) -> Markup {
    html! {
        tr class="schedule-organization-selection" {
            th scope="row" { (label) }
            td colspan="8" {
                select name=(name) data-organization-schedule-select aria-label=(label) {
                    @for (organization_id, rank_name) in choices {
                        option value=(organization_id) selected[organization_id == selected_id] {
                            (profession_label(organization_id)) " — " (rank_name)
                        }
                    }
                }
            }
        }
    }
}

fn profession_label(organization_id: &str) -> &str {
    adventuresim_core::organization::organization(organization_id)
        .map_or("organization", |organization| organization.name.as_str())
}

fn schedule_allocation_cell(name: &str, minutes: u16, editable: bool) -> Markup {
    html! {
        td class="party-skill-allocation" data-schedule-value=(name) {
            @if editable {
                input type="hidden" name=(name) value=(minutes) data-schedule-input;
                span data-schedule-display tabindex="0" role="button" title="Click to enter a time such as 8, 8:30, or 830" {
                    (format_schedule_hours(minutes))
                }
            } @else {
                span data-schedule-display { "0h" }
            }
        }
    }
}

fn schedule_icon(label: &str, icon: &str, actionable: bool, activity: &str) -> Markup {
    html! {
        @if actionable {
            button type="button" class="schedule-activity-button" data-activity-open=(activity)
                aria-label=(format!("Perform {label} now")) title=(format!("Perform {label} now"))
                aria-haspopup="dialog" aria-expanded="false" {
                span class="stat-icon schedule-special-icon"
                    style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')"))
                    aria-hidden="true" {}
            }
        } @else {
            span class="stat-icon schedule-special-icon"
                style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')"))
                aria-hidden="true" {}
        }
    }
}

fn immediate_activity_dialog(action: &str) -> Markup {
    html! {
        div class="activity-modal" data-activity-modal hidden {
            button type="button" class="activity-modal-backdrop" data-activity-close
                aria-label="Close activity dialog" {}
            form class="activity-modal-panel" action=(action) method="post" role="dialog"
                aria-modal="true" aria-labelledby="activity-modal-title" tabindex="-1"
                data-activity-form {
                header class="activity-modal-header" {
                    h3 id="activity-modal-title" data-activity-title { "Perform activity" }
                    button type="button" class="activity-modal-close" data-activity-close
                        aria-label="Close activity dialog" { "×" }
                }
                input type="hidden" name="activity" data-activity-kind;
                input type="hidden" name="service_id" data-activity-service;
                input type="hidden" name="requested_minutes" value="60" data-activity-minutes;
                div class="activity-duration-control" {
                    label for="immediate-activity-duration" { "Duration" }
                    input id="immediate-activity-duration" type="range" min="1" max="24"
                        step="1" value="1" data-activity-duration;
                    p class="activity-duration-summary" aria-live="polite" data-activity-duration-summary {
                        span data-activity-end { "Ends at --:--" }
                        span aria-hidden="true" { " / " }
                        span data-activity-hours { "1 h spent" }
                    }
                }
                table class="party-skills-table activity-preview-table" aria-label="Activity result preview" {
                    thead { tr {
                        th scope="col" { "Activity" }
                        th scope="col" { (schedule_header_icon("coins", "Currency")) }
                        th scope="col" { (schedule_header_icon("scales", "Virtue")) }
                        th scope="col" { (schedule_header_icon("sun", "Morale")) }
                        th scope="col" { (schedule_header_icon("night-sleep", "Fatigue")) }
                        th scope="col" { (schedule_header_icon("open-book", "Skill-hours")) }
                    } }
                    tbody { tr class="party-skill-row schedule-special-row" data-activity-preview-row {
                        th scope="row" data-activity-preview-label { "Activity" }
                        @for kind in ["gold", "virtue", "morale", "fatigue"] {
                            td class="schedule-effect schedule-effect-neutral" data-activity-effect=(kind) { "0" }
                        }
                        td class="schedule-effect schedule-training-effect" data-activity-effect="training" { "--" }
                    } }
                }
                button type="submit" class="btn btn-primary activity-submit" data-activity-submit { "Spend 1 hour" }
            }
        }
    }
}

fn format_schedule_hours(minutes: u16) -> String {
    let rounded = ((u32::from(minutes) + 7) / 15) * 15;
    let hours = rounded / 60;
    let fraction = match rounded % 60 {
        0 => "",
        15 => "¼",
        30 => "½",
        45 => "¾",
        _ => unreachable!("rounded schedule minute must be a quarter hour"),
    };
    format!("{hours}{fraction}h")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;

    fn test_attributes(value: f32) -> CharacterAttributes {
        CharacterAttributes {
            character_id: 1,
            endurance: value,
            immunity: value,
            gut: value,
            intelligence: value,
            instinct: value,
            eyesight: value,
            hearing: value,
            left_arm_strength: value,
            right_arm_strength: value,
            left_leg_strength: value,
            right_leg_strength: value,
            left_arm_agility: value,
            right_arm_agility: value,
            left_leg_agility: value,
            right_leg_agility: value,
        }
    }

    #[test]
    fn healthy_summary_orders_unique_weapon_leaves_before_armor_and_noncombat() {
        let skills = CharacterSkills {
            sword_hours: 50_000.0,
            knife_hours: 50_000.0,
            block_hours: 50_000.0,
            command_hours: 50_000.0,
            ..Default::default()
        };
        let profile = CombatTrainingProfile {
            weapons: adventuresim_core::equipment::WeaponSkillDistribution {
                sword: 1.0,
                knife: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let capability = CharacterCapability {
            full_armor: true,
            ..Default::default()
        };
        let icons = character_summary_icons(
            Some(&capability),
            Some(&test_attributes(5.0)),
            Some(&skills),
            profile,
            None,
        );
        assert!(icons[0].label.starts_with("Sword —"));
        assert!(icons[1].label.starts_with("Knife —"));
        assert!(icons[2].label.starts_with("Full armor — Block"));
        assert!(icons[3].label.starts_with("Social —"));
        assert_eq!(
            icons
                .iter()
                .filter(|icon| icon.label.starts_with("Sword —"))
                .count(),
            1
        );
    }

    #[test]
    fn summary_armor_uses_the_stronger_healthy_defense() {
        let skills = CharacterSkills {
            dodge_hours: 100.0,
            block_hours: 50_000.0,
            ..Default::default()
        };
        let icons = character_summary_icons(
            Some(&CharacterCapability {
                quarter_armor: true,
                ..Default::default()
            }),
            Some(&test_attributes(5.0)),
            Some(&skills),
            CombatTrainingProfile::default(),
            None,
        );
        let armor = icons
            .iter()
            .find(|icon| icon.label.starts_with("Quarter armor"))
            .unwrap();
        assert!(armor.label.contains("Block"));
        assert!(armor.tooltip.contains("Dodge —"));
        assert!(armor.tooltip.contains("Block —"));
    }

    #[test]
    fn summary_groups_only_qualifying_noncombat_leaves() {
        let skills = CharacterSkills {
            insight_hours: 50_000.0,
            charm_hours: 1.0,
            command_hours: 50_000.0,
            ..Default::default()
        };
        let icons = character_summary_icons(
            None,
            Some(&test_attributes(5.0)),
            Some(&skills),
            CombatTrainingProfile::default(),
            None,
        );
        let social = icons
            .iter()
            .find(|icon| icon.label.starts_with("Social —"))
            .unwrap();
        assert!(social.tooltip.contains("Insight —"));
        assert!(social.tooltip.contains("Command —"));
        assert!(!social.tooltip.contains("Charm —"));
        assert_eq!(social.rank, social_family_rank(&skills, 5.0));
    }

    #[test]
    fn summary_religion_uses_the_contextual_primary_icon_and_shared_score() {
        let skills = CharacterSkills {
            religion_hours: adventuresim_world_schema::ReligionHours {
                roman_catholic: 50_000.0,
                lutheran: 50_000.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let icons = character_summary_icons(
            None,
            Some(&test_attributes(5.0)),
            Some(&skills),
            CombatTrainingProfile::default(),
            Some(OfficialReligion::Lutheran),
        );
        let religion = icons
            .iter()
            .find(|icon| icon.label.contains("religion"))
            .unwrap();
        assert!(religion.label.starts_with("Lutheran"));
        assert!(matches!(
            &religion.kind,
            SummaryIconKind::Mask(path) if path.contains("luther-rose")
        ));
        assert_eq!(
            primary_religion(&skills, Some(OfficialReligion::Lutheran)),
            OfficialReligion::Lutheran
        );
    }

    #[test]
    fn summary_missing_or_nonfinite_inputs_degrade_safely() {
        assert!(
            character_summary_icons(None, None, None, CombatTrainingProfile::default(), None)
                .is_empty()
        );
        let skills = CharacterSkills {
            sword_hours: f32::NAN,
            command_hours: f32::INFINITY,
            ..Default::default()
        };
        let icons = character_summary_icons(
            None,
            Some(&test_attributes(f32::NAN)),
            Some(&skills),
            CombatTrainingProfile {
                weapons: adventuresim_core::equipment::WeaponSkillDistribution {
                    sword: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            },
            None,
        );
        assert_eq!(icons.len(), 1);
        assert_eq!(icons[0].rank, 0.0);
        assert!(!icons[0].label.contains("NaN"));
        assert!(!icons[0].label.contains("inf"));
    }

    #[test]
    fn social_skill_family_has_an_average_and_four_expandable_icon_rows() {
        let skills = CharacterSkills {
            character_id: 7,
            insight_hours: 100.0,
            charm_hours: 80.0,
            command_hours: 40.0,
            deception_hours: 20.0,
            ..CharacterSkills::default()
        };
        let markup = social_skill_rows(&skills, 5.0, 1.0, None).into_string();
        assert!(markup.contains("data-social-primary"));
        assert!(markup.contains("data-skill-tooltip"));
        assert!(markup.contains("Social"));
        assert!(markup.contains("Governed by Instinct"));
        assert_eq!(markup.matches("data-social-detail").count(), 4);
        for icon in ["conversation.svg", "awareness.svg", "crown.svg", "rose.svg"] {
            assert!(markup.contains(icon), "missing social icon {icon}");
        }
    }

    #[test]
    fn skill_meter_and_schedule_use_segmented_rank_and_text_time_controls() {
        let meter =
            skill_rank_bar(3.5, 2.75, "Skill test", SkillRankBarOptions::default()).into_string();
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(meter.contains("role=\"meter\""));
        assert!(meter.contains("aria-valuenow=\"2.8\""));
        assert!(meter.contains("class=\"skill-rank-value\""));
        assert!(!meter.contains("tabindex"));
        let allocation = schedule_allocation_cell("smithing_minutes", 75, true).into_string();
        assert!(allocation.contains("data-schedule-input"));
        assert!(allocation.contains("data-schedule-display"));
        assert!(allocation.contains("Click to enter a time such as 8, 8:30, or 830"));
        assert!(!allocation.contains("data-schedule-step"));
        assert!(!allocation.contains("type=\"range\""));
        assert!(!allocation.contains("schedule-handle"));
    }

    #[test]
    fn forage_action_replaces_the_terrain_skill_icon_not_the_expand_control() {
        let rendered = terrain_skill_rows(
            &CharacterSkills::default(),
            1.0,
            false,
            Some(SkillAction::Get {
                href: "/forage",
                label: "Forage in the immediate vicinity",
                open: false,
            }),
        )
        .into_string();
        let primary = rendered.find("data-terrain-primary").unwrap();
        let action = rendered.find("href=\"/forage\"").unwrap();
        let expand = rendered.find("data-terrain-expand").unwrap();

        assert!(primary < action && action < expand);
        assert!(rendered.contains("class=\"religion-expand-cell\"><button"));
        assert!(!rendered.contains("class=\"religion-expand-cell\"><a"));
    }

    #[test]
    fn schedule_table_uses_compact_accessible_icon_headers() {
        let skills = CharacterSkills {
            character_id: 1,
            polearm_hours: 0.0,
            axe_hours: 0.0,
            bludgeon_hours: 0.0,
            sword_hours: 0.0,
            knife_hours: 0.0,
            dodge_hours: 0.0,
            block_hours: 0.0,
            bow_hours: 0.0,
            crossbow_hours: 0.0,
            firearm_hours: 0.0,
            throw_hours: 0.0,
            will_hours: 0.0,
            insight_hours: 0.0,
            charm_hours: 0.0,
            command_hours: 0.0,
            deception_hours: 0.0,
            physiology_hours: 0.0,
            cooking_hours: 0.0,
            religion_hours: adventuresim_world_schema::ReligionHours {
                roman_catholic: 1_000.0,
                ..Default::default()
            },
            bestiary_hours: Default::default(),
            oral_languages: Default::default(),
            written_languages: Default::default(),
            stealth_hours: 0.0,
            balance_hours: 0.0,
            terrain_plains_hours: 0.0,
            terrain_forest_hours: 0.0,
            terrain_hills_hours: 0.0,
            terrain_urban_hours: 0.0,
            anatomy_hours: 0.0,
            tailoring_hours: 0.0,
            smithing_hours: 0.0,
        };
        let schedule = CharacterTrainingSchedule {
            character_id: 1,
            downtime: crate::spacetimedb::ScheduleAllocation {
                combat_training_minutes: 90,
                prayer_minutes: 120,
                ..Default::default()
            },
            travel: crate::spacetimedb::ScheduleAllocation::default(),
        };
        let attributes = test_attributes(5.0);
        let rendered = skills_table(
            "Your skills",
            Some(&attributes),
            &skills,
            1.0,
            1.0,
            1.0,
            Some(&schedule),
            None,
            false,
            0.0,
            Some(OfficialReligion::Judaism),
            CombatTrainingProfile::default(),
            false,
            CharacterSheetActions::default(),
        )
        .into_string();

        assert!(rendered.contains(
            "scope=\"colgroup\" colspan=\"8\" class=\"schedule-table-title\">Your skills"
        ));
        assert_eq!(rendered.matches("<colgroup>").count(), 2);
        assert!(rendered.contains(
            "<col class=\"religion-auto-column\"><col class=\"party-skill-time-column\"><col class=\"religion-expand-column\">"
        ));
        assert_eq!(
            rendered.matches("aria-label=\"Daily allocation\"").count(),
            1
        );
        assert!(!rendered.contains("aria-label=\"Automatic training\""));
        for label in ["Currency", "Virtue", "Morale", "Fatigue"] {
            assert!(rendered.contains(&format!("aria-label=\"{label}\"")));
        }
        assert!(rendered.contains("data-religion-expand"));
        assert!(!rendered.contains("class=\"skill-rank-value\""));
        assert_eq!(
            rendered.matches("class=\"party-skill-row").count(),
            rendered.matches("class=\"religion-expand-cell\"").count(),
        );
        assert!(rendered.contains("aria-expanded=\"false\""));
        assert!(rendered.contains("data-religion-primary=\"judaism\""));
        assert!(rendered.contains("Expand Judaism Religion skill"));
        assert!(rendered.contains("title=\"Judaism\""));
        assert!(rendered.contains("/static/icons/religion/fontawesome-star-of-david.svg"));
        assert!(!rendered.contains("data-combat-auto-toggle"));
        for group in ["melee", "ranged", "defense"] {
            assert!(rendered.contains(&format!("data-combat-expand=\"{group}\"")));
            assert!(rendered.contains(&format!("data-combat-detail=\"{group}\"")));
        }
        assert!(!rendered.contains("aria-label=\"Religion details\""));
        assert!(rendered.contains("aria-label=\"Skill details\""));
        assert!(rendered.contains("Sparring and target practice"));
        assert!(rendered.contains("Carousing"));
        assert_eq!(rendered.matches("data-religion-detail").count(), 1);
        assert!(!rendered.contains("title=\"Lutheranism\""));
        assert!(!rendered.contains("religion_judaism_minutes"));
        assert!(!rendered.contains("effective /"));
        assert!(rendered.contains("Governed by Intelligence"));
        assert!(rendered.contains("0.0 effective hours trained"));
        let primary_icon = rendered
            .find("/static/icons/religion/fontawesome-star-of-david.svg")
            .unwrap();
        let expand = rendered.find("data-religion-expand").unwrap();
        assert!(primary_icon < expand);
        assert!(rendered.contains("class=\"religion-expand-cell\"><button"));
        assert!(rendered.contains("aria-label=\"Will\""));
        assert!(!rendered.contains("data-religion-auto-budget disabled"));
        assert!(!rendered.contains("data-religion-manual-budget disabled"));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
        assert!(!rendered.contains(">Gold</th>"));
        assert!(!rendered.contains(">Virt.</th>"));

        let rail = party_skills_rail(
            "Your skills",
            None,
            Some(&skills),
            None,
            Some(&schedule),
            Some("/schedule"),
            None,
            false,
            0.0,
            Some("judaism"),
            CombatTrainingProfile::default(),
            CharacterSheetActions::default(),
        )
        .into_string();
        assert!(!rail.contains("class=\"sidebar-header\">Your skills"));
        assert!(rail.contains("<h3 class=\"sr-only\">Your skills</h3>"));
        assert!(rail.contains("data-schedule-save-status"));
        assert!(rail.contains("role=\"status\" aria-live=\"polite\" hidden"));
        assert!(rail.contains("data-schedule-retry>Retry</button>"));
        assert!(rail.contains("class=\"btn btn-secondary btn-small\""));
        assert!(!rail.contains("data-activity-modal"));
        assert!(!rail.contains("data-activity-open"));
        let settlement_rail = party_skills_rail(
            "Your skills",
            None,
            Some(&skills),
            None,
            Some(&schedule),
            Some("/locations/settlement/lubeck/party/1/schedule"),
            None,
            false,
            0.0,
            Some("judaism"),
            CombatTrainingProfile::default(),
            CharacterSheetActions::default(),
        )
        .into_string();
        assert!(settlement_rail.contains("data-activity-modal"));
        assert!(settlement_rail.contains("data-activity-open"));
        assert!(!rail.contains(">⚙</span>"));
        assert!(!rail.contains("aria-label=\"Automatic training\""));
    }

    #[test]
    fn defense_will_uses_head_health_for_its_injury_adjusted_rank() {
        let skills = CharacterSkills {
            will_hours: 5_000.0,
            ..Default::default()
        };
        let rendered = combat_skill_rows(
            &skills,
            5.0,
            5.0,
            5.0,
            0.2,
            0.8,
            1.0,
            None,
            CombatTrainingProfile::default(),
        )
        .into_string();
        let will = rendered.find("aria-label=\"Will\"").unwrap();
        let start = rendered[..will].rfind("<tr").unwrap();
        let end = will + rendered[will..].find("</tr>").unwrap() + "</tr>".len();
        let will_row = &rendered[start..end];
        let rank = Skill::Will.training_rank(5_000.0);
        assert!(will_row.contains(&format!("aria-valuenow=\"{:.1}\"", rank * 0.2)));
        assert!(will_row.contains("data-skill-tooltip"));
        assert!(will_row.contains("Will"));
        assert!(will_row.contains("Governed by Instinct"));
        assert!(will_row.contains("5000.0 effective hours trained"));
    }

    #[test]
    fn cooking_and_knife_rows_render_projected_hours_without_losing_injury_caps() {
        let skills = CharacterSkills {
            cooking_hours: 100.0,
            knife_hours: 1_000.0,
            ..Default::default()
        };
        let cooking = party_skill_row(
            &skills,
            "Cooking",
            "cooking",
            Skill::Cooking,
            5.0,
            0.5,
            false,
            None,
        )
        .into_string();
        assert!(cooking.contains("100.0 direct hours trained"));
        assert!(cooking.contains("200.0 effective hours trained"));
        assert!(cooking.contains("Knife | 15%"));
        let cooking_rank = Skill::Cooking.training_rank(200.0) * 0.5;
        assert!(cooking.contains(&format!("aria-valuenow=\"{cooking_rank:.1}\"")));

        let combat = combat_skill_rows(
            &skills,
            5.0,
            5.0,
            5.0,
            1.0,
            0.5,
            1.0,
            None,
            CombatTrainingProfile::default(),
        )
        .into_string();
        assert!(combat.contains("1000.0 direct hours trained"));
        assert!(combat.contains("1015.0 effective hours trained"));
        assert!(combat.contains("Cooking | 15%"));
    }

    #[test]
    fn terrain_rows_render_intuitive_cross_habitat_projection() {
        let skills = CharacterSkills {
            terrain_forest_hours: 1_000.0,
            ..Default::default()
        };
        let rendered = terrain_skill_rows(&skills, 5.0, false, None).into_string();
        assert!(rendered.contains("0.0 direct hours trained"));
        assert!(rendered.contains("200.0 effective hours trained"));
        assert!(rendered.contains("Forest | 20%"));
        assert!(rendered.contains("1000.0 direct hours trained"));
        assert!(rendered.contains("Plains | 20%"));
    }

    #[test]
    fn language_families_are_expandable_accessible_and_color_coded() {
        let skills = CharacterSkills {
            oral_languages: adventuresim_world_schema::OralLanguageHours {
                east_central: 5_000.0,
                west_central: 1_000.0,
                ..Default::default()
            },
            written_languages: adventuresim_world_schema::WrittenLanguageHours {
                german: 1_000.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let rendered = language_skill_rows(&skills, 5.0, 5.0, false).into_string();
        assert!(rendered.contains("Expand Oral languages"));
        assert!(rendered.contains("Expand Written languages"));
        assert!(rendered.contains("language-oral language-blackletter"));
        assert!(rendered.contains("language-written language-blackletter"));
        assert!(rendered.contains("Oral languages"));
        assert!(rendered.contains("Written languages"));
        assert!(rendered.contains("Governed by Instinct"));
        assert!(rendered.contains("Governed by Intelligence"));
        assert!(rendered.contains("title=\"East-central — Ostmitteldeutsch\""));
        let oral_tooltip = oral_language_family_tooltip(&skills);
        assert_eq!(oral_tooltip.trained_hours, 5_000.0);
        assert!(oral_tooltip.correlated_hours > 0.0);
        assert!(!oral_tooltip.correlations.is_empty());
        assert!(rendered.contains(&format!(
            "{:.1} effective hours trained",
            oral_tooltip.trained_hours + oral_tooltip.correlated_hours
        )));
        assert!(!rendered.contains("title=\"Latin — Latine\""));
        assert!(!rendered.contains("title=\"Romani — Romani\""));
        assert_eq!(rendered.matches("data-language-detail=\"oral\"").count(), 4);
        assert_eq!(
            rendered.matches("data-language-detail=\"written\"").count(),
            1
        );
    }

    #[test]
    fn language_families_are_hidden_without_effective_hours() {
        let rendered =
            language_skill_rows(&CharacterSkills::default(), 5.0, 5.0, false).into_string();
        assert!(!rendered.contains("Expand Oral languages"));
        assert!(!rendered.contains("Expand Written languages"));
        assert!(!rendered.contains("data-language-detail"));
    }

    #[test]
    fn activity_rows_show_signed_daily_effects_instead_of_allocation_bars() {
        let rendered = schedule_special_row(
            "Thievery",
            "market",
            "thievery_minutes",
            120,
            true,
            true,
            ActivityEffectRates::linear(2.0, -1.0, 0.0, 0.0),
            None,
            None,
            1.0,
            "Test activity",
        )
        .into_string();
        for effect in ["gold", "virtue", "morale", "fatigue"] {
            assert!(rendered.contains(&format!("data-activity-effect=\"{effect}\"")));
        }
        assert!(rendered.contains("schedule-effect-positive"));
        assert!(rendered.contains(">+4</td>"));
        assert!(rendered.contains("schedule-effect-negative"));
        assert!(rendered.contains(">-2.0</td>"));
        assert!(rendered.contains("<span class=\"sr-only\">Thievery</span>"));
        assert!(!rendered.contains("<strong>Thievery</strong>"));
        assert!(!rendered.contains("schedule-allocation-fill"));
        assert!(!rendered.contains("schedule-special-track"));
    }

    #[test]
    fn activity_training_column_totals_and_explains_effective_skill_hours() {
        let combat =
            activity_training_cell("Combat Training", "combat_training_minutes", 120, None, 1.0)
                .into_string();
        assert!(combat.contains(">+2.00h<"));
        assert!(combat.contains("Relevant combat skills: +2.00h"));

        let carousing =
            activity_training_cell("Carousing", "carousing_minutes", 120, None, 1.0).into_string();
        assert!(carousing.contains(">+0.50h<"));
        assert!(carousing.contains("Charm: +0.50h"));

        let profession = ProfessionActivityPreview {
            training_rates: vec![
                ("Physiology".into(), 0.5),
                ("Anatomy".into(), 1.0 / 6.0),
                ("Knife".into(), 1.0 / 6.0),
                ("Tailoring".into(), 1.0 / 6.0),
            ],
            apprenticeship_accrued: 0,
            practice_accrued: 0,
            practice_threshold: 8 * 60 * PROFESSION_ACCRUAL_SCALE,
            practice_weight: 1,
            practice_reward: "gold",
            tier_label: "Apprentice".into(),
            practice_allowed: false,
        };
        let apprenticeship = activity_training_cell(
            "Apprenticeship — herbalist",
            "apprenticeship_minutes",
            120,
            Some(&profession),
            1.0,
        )
        .into_string();
        assert!(apprenticeship.contains(">+2.00h<"));
        assert!(apprenticeship.contains("Physiology: +1.00h"));
        assert!(apprenticeship.contains("Anatomy: +0.33h"));
        assert!(apprenticeship.contains("Knife: +0.33h"));
        assert!(apprenticeship.contains("Tailoring: +0.33h"));

        let leisure =
            activity_training_cell("Leisure", "leisure_minutes", 480, None, 1.0).into_string();
        assert!(leisure.contains(">—<"));
        assert!(leisure.contains("No skill training"));
    }

    #[test]
    fn server_rendered_effects_normalize_negative_zero() {
        let rendered = activity_effect_cell("fatigue", -0.0006).into_string();
        assert!(rendered.contains("schedule-effect-neutral"));
        assert!(rendered.contains(">0</td>"));
        assert!(!rendered.contains("-0.0"));

        let negative = activity_effect_cell("fatigue", -0.06).into_string();
        assert!(negative.contains("schedule-effect-negative"));
        assert!(negative.contains(">-0.1</td>"));
    }

    #[test]
    fn prayer_preview_uses_zero_partial_and_full_party_religion_checks() {
        let minutes = 240;
        let full = ActivityEffectRates::prayer(1.0).values(minutes)[2];
        assert_eq!(ActivityEffectRates::prayer(0.0).values(minutes)[2], 0.0);
        assert!((ActivityEffectRates::prayer(0.5).values(minutes)[2] - full * 0.5).abs() < 0.001);
        assert!(full > 0.0);
        assert!((ActivityEffectRates::meditation().values(minutes)[2] - full * 0.25).abs() < 0.001);
    }

    #[test]
    fn leisure_and_labor_previews_decompose_the_shared_fatigue_outcome() {
        let schedule = ScheduleAllocation {
            labor_minutes: 240,
            combat_training_minutes: 720,
            ..Default::default()
        };
        let leisure = leisure_preview(&schedule, 0.0);
        assert_eq!(leisure.outcome.leisure_hours, 8.0);
        assert_eq!(leisure.outcome.fatigue_delta, 0.0);
        assert_eq!(leisure.outcome.morale, 0.0);
        assert_eq!(leisure.fatigue_display, -2.0);
        assert_eq!(
            ActivityEffectRates::linear(
                0.0,
                0.0,
                0.0,
                LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
            )
            .values(schedule.labor_minutes)[3],
            2.0
        );
        let rendered = schedule_special_row(
            "Leisure",
            "inn",
            "leisure_minutes",
            0,
            false,
            false,
            ActivityEffectRates::default(),
            Some(leisure),
            None,
            1.0,
            "Test leisure",
        )
        .into_string();
        for attribute in [
            "data-leisure-baseline-fatigue",
            "data-leisure-labor-fatigue-rate",
            "data-leisure-recovery-rate",
            "data-leisure-morale-limit",
            "data-leisure-morale-scale",
            "data-leisure-fatigue-preview-divisor",
        ] {
            assert!(rendered.contains(attribute));
        }
        assert!(rendered.contains(">-2.0</td>"));
    }

    #[test]
    fn schedule_and_equipment_scripts_use_the_new_interactions() {
        let schedule = include_str!("../../../static/training-schedule.js");
        let numeric = include_str!("../../../static/numeric-editor.js");
        let equipment = include_str!("../../../static/equipment-toggle.js");
        let live_regions = include_str!("../../../static/live-regions.js");
        let immediate_activity = include_str!("../../../static/immediate-activity.js");
        let css = include_str!("../../../static/css/strategic.css");
        assert!(schedule.contains("function parseClock(value)"));
        assert!(schedule.contains("window.StrategicNumericEditor.open"));
        assert!(numeric.contains("input.type = 'text'"));
        assert!(numeric.contains("confirm.addEventListener('click', () => finish(true))"));
        assert!(numeric.contains("cancel.addEventListener('click', () => finish(false))"));
        assert!(numeric.contains("input.addEventListener('wheel'"));
        assert!(!numeric.contains("document.addEventListener('wheel'"));
        assert!(schedule.contains("/^\\d{3,4}$/"));
        assert!(schedule.contains("Math.round(wanted / STEP) * STEP"));
        assert!(schedule.contains("function renderActivityPreview(row, minutes)"));
        assert!(schedule.contains("function calculateLeisurePreview"));
        assert!(schedule.contains("row.dataset.leisureFatiguePreviewDivisor"));
        assert!(schedule.contains("function mountSchedules(root = document)"));
        assert!(schedule.contains("[data-social-expand]"));
        assert!(schedule.contains(".social-detail-row"));
        assert!(schedule.contains("[data-bestiary-expand]"));
        assert!(schedule.contains(".bestiary-detail-row"));
        assert!(schedule.contains("'strategic-live-regions-refreshed'"));
        assert!(schedule.contains("event.detail.regions.includes('left-sidebar')"));
        assert!(schedule.contains("function createLatestSaveQueue(send"));
        assert!(schedule.contains("data-schedule-pending"));
        assert!(schedule.contains("retry()"));
        assert!(schedule.contains("data-schedule-save-status"));
        assert!(schedule.contains("data-schedule-retry"));
        assert!(schedule.contains("strategic-live-refresh-requested"));
        assert!(schedule.contains("schedule-effect-positive"));
        assert!(!schedule.contains("scheduleDrag"));
        assert!(!schedule.contains("travel_"));
        assert!(equipment.contains("'/api/equipment'"));
        assert!(equipment.contains("strategicSubmitMutation"));
        assert!(!equipment.contains("window.location.reload()"));
        assert!(!equipment.contains("strategic-live-refresh-requested"));
        assert!(live_regions.contains("const scrollOffsets = (selector)"));
        assert!(live_regions.contains("region.scrollTop = offsets.top"));
        assert!(live_regions.contains("replaced.includes(\"left-sidebar\")"));
        assert!(live_regions.contains("document.querySelector('.numeric-editor')"));
        assert!(live_regions.contains("[data-activity-modal]:not([hidden])"));
        assert!(live_regions.contains("scheduleEditorIsPending"));
        assert!(live_regions.contains("const schedulePendingAtStart = scheduleEditorIsPending()"));
        assert!(live_regions.contains("!schedulePendingAtStart && !scheduleEditorIsPending()"));
        assert!(immediate_activity.contains("typeof window === 'undefined'"));
        assert!(immediate_activity.contains("input:not([type=\"hidden\"]):not(:disabled)"));
        assert!(immediate_activity.contains("wrappedFocusTarget"));
        assert!(immediate_activity.contains("strategic-editor-idle"));
        assert!(immediate_activity.contains("'strategic-page-mounted'"));
        assert!(css.contains(".numeric-editor-input {"));
        assert!(css.contains("position: fixed;"));
        assert!(css.contains("z-index: 80;"));
        assert!(css.contains(".numeric-editor {"));
        assert!(css.contains("right: auto;"));
        assert!(css.contains("left: 50%;"));
        assert!(css.contains("transform: translate(-50%, -50%);"));
        assert!(css.contains(".numeric-editor-input::selection {"));
        assert!(numeric.contains("document.body.append(editor)"));
        assert!(numeric.contains("display.style.visibility = 'hidden'"));
        assert!(!numeric.contains("display.hidden = true"));
        assert!(numeric.contains("window.addEventListener('resize', positionEditor)"));
        assert!(!css.contains(".party-skill-icon-column"));
        assert!(css.contains(".numeric-editor-action {"));
        assert!(css.contains(".numeric-editor-confirm { background: #2f7d3d; }"));
        assert!(css.contains(".numeric-editor-cancel { background: #9c3434; }"));
        assert!(css.contains(".schedule-save-status"));
    }

    #[test]
    fn bestiary_skill_family_lists_correlated_categories_and_accessible_lore() {
        let css = include_str!("../../../static/css/strategic.css");
        let skills = CharacterSkills {
            bestiary_hours: adventuresim_world_schema::BestiaryHours {
                human: 1_000.0,
                wildmen: 500.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let rendered = bestiary_skill_rows(&skills, 5.0, 1.0, false).into_string();
        let family_tooltip = bestiary_family_tooltip(&skills);
        let expected_direct =
            skills.bestiary_hours.total_direct() / BestiaryCategory::ALL.len() as f32;
        assert!((family_tooltip.trained_hours - expected_direct).abs() < f32::EPSILON);
        assert!(
            (family_tooltip.trained_hours + family_tooltip.correlated_hours
                - skills.bestiary_hours.aggregate_effective())
            .abs()
                < f32::EPSILON
        );
        assert!(rendered.contains("data-skill-family=\"bestiary\""));
        assert!(rendered.contains("/static/icons/stats/bestiary/bestiary.png"));
        assert!(rendered.contains("/static/icons/stats/bestiary/wildmen.png"));
        assert!(rendered.contains("data-bestiary-expand"));
        assert!(rendered.contains("Expand Bestiary skills"));
        assert!(rendered.contains("Wildmen"));
        assert!(rendered.contains("effective hours trained"));
        assert!(rendered.contains("hours from correlated skills"));
        assert!(rendered.contains("data-bestiary-enemies"));
        assert!(rendered.contains("data-tooltip-pinnable"));
        assert!(rendered.contains("Wild man"));
        assert!(rendered.contains("Main type for:"));
        assert!(rendered.contains("Secondary type for:"));
        assert!(!rendered.contains("data-bestiary-strengths"));
        assert!(!rendered.contains("data-bestiary-weaknesses"));
        assert!(!rendered.contains("Great strength and endurance"));
        assert!(!rendered.contains("Limited armour"));
        assert!(!rendered.contains("combat modifier"));
        assert!(!rendered.contains("no effect"));
        assert!(rendered.contains("data-strategic-tooltip"));
        assert_eq!(rendered.matches("data-bestiary-detail").count(), 2);
        assert!(css.contains(".bestiary-primary-row .stat-icon,"));
        assert!(css.contains("--stat-icon-color: var(--info);"));
        assert!(css.contains("cursor: help;"));
    }
}
