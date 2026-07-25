use adventuresim_core::{
    activity::{PRAYER_MORALE_LIMIT, PRAYER_MORALE_SCALE_MINUTES, settlement_population_scale},
    prelude::Skill,
    strategic_schedule::{
        BASELINE_FATIGUE_PER_DAY, CombatTrainingProfile, DailySchedule,
        FATIGUE_RESERVOIR_PER_PREVIEW_POINT, LABOR_FATIGUE_PER_HOUR,
        LEISURE_FATIGUE_RECOVERY_PER_HOUR, LEISURE_MORALE_LIMIT, LEISURE_MORALE_SCALE_FATIGUE,
        LeisureOutcome, settlement_leisure_outcome,
    },
    strategic_time::MINUTES_PER_DAY,
};
use adventuresim_world_schema::{BestiaryCategory, OfficialReligion};
use maud::{Markup, html};

use super::character_health::stat_icon;
use crate::spacetimedb::{
    CharacterApprenticeship, CharacterAttributes, CharacterCapability, CharacterLimbs,
    CharacterSkills, CharacterStats, CharacterTrainingSchedule, ScheduleAllocation, Settlement,
};
use crate::templates::{game_icon, religion_icon, sidebar_section};

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
    practice_reward: &'static str,
    tier_label: &'static str,
}

const PROFESSION_ACCRUAL_SCALE: u64 = MINUTES_PER_DAY;
const APPRENTICESHIP_REWARD_THRESHOLD: u64 = 8 * 60 * PROFESSION_ACCRUAL_SCALE;

impl ProfessionActivityPreview {
    fn reward_delta(&self, allocation_name: &str, minutes: u16) -> [f32; 2] {
        let (accrued, threshold, sign, reward) = match allocation_name {
            "apprenticeship_minutes" => (
                self.apprenticeship_accrued,
                APPRENTICESHIP_REWARD_THRESHOLD,
                -1.0,
                "gold",
            ),
            "profession_practice_minutes" => (
                self.practice_accrued,
                self.practice_threshold,
                1.0,
                self.practice_reward,
            ),
            _ => return [0.0, 0.0],
        };
        let after = accrued.saturating_add(u64::from(minutes) * PROFESSION_ACCRUAL_SCALE);
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
        let agility = [
            attributes.left_arm_agility,
            attributes.right_arm_agility,
            attributes.left_leg_agility,
            attributes.right_leg_agility,
        ]
        .into_iter()
        .zip(limb_health)
        .map(|(value, health)| value * health.clamp(0.0, 1.0) * 0.25)
        .sum::<f32>();
        let usable_limbs = limb_health
            .into_iter()
            .map(|health| health.clamp(0.0, 1.0) * 0.25)
            .sum::<f32>();
        let precision = attributes.precision * usable_limbs;
        let stealth =
            (Skill::Stealth.training_rank(skills.stealth_hours) + agility + precision) * 0.5;
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
        skills: Option<&CharacterSkills>,
        apprenticeships: &[CharacterApprenticeship],
    ) -> Self {
        let Some(skills) = skills else { return self };
        for row in apprenticeships {
            let Some(definition) =
                adventuresim_core::profession::profession_for_service(&row.service_id)
            else {
                continue;
            };
            let hours = |skill: Skill| match skill {
                Skill::Command => skills.command_hours,
                Skill::Smithing => skills.smithing_hours,
                Skill::Tailoring => skills.tailoring_hours,
                Skill::Medicine => skills.medicine_hours,
                Skill::Bestiary => skills.bestiary_hours.direct(BestiaryCategory::Human),
                Skill::Surgery => skills.surgery_hours,
                Skill::Knife => skills.knife_hours,
                Skill::Cooking => skills.cooking_hours,
                Skill::Religion => row
                    .religion_id
                    .as_deref()
                    .and_then(OfficialReligion::from_id)
                    .map_or(0.0, |religion| skills.religion_hours.direct(religion)),
                _ => 0.0,
            };
            let tier = adventuresim_core::profession::profession_tier(definition, hours);
            let practice_threshold = match tier {
                adventuresim_core::profession::ProfessionTier::Master => 2 * 60 * MINUTES_PER_DAY,
                _ => 8 * 60 * MINUTES_PER_DAY,
            };
            let practice_reward = match definition.practice_reward {
                adventuresim_core::profession::PracticeReward::Gold => "gold",
                adventuresim_core::profession::PracticeReward::Virtue => "virtue",
            };
            self.profession.insert(
                row.service_id.clone(),
                ProfessionActivityPreview {
                    training_rates: definition
                        .skills
                        .iter()
                        .map(|entry| {
                            (
                                if entry.skill == Skill::Bestiary {
                                    "Human knowledge".into()
                                } else {
                                    format!("{:?}", entry.skill)
                                },
                                entry.weight,
                            )
                        })
                        .collect(),
                    apprenticeship_accrued: row.apprenticeship_minutes_accrued,
                    practice_accrued: row.practice_minutes_accrued,
                    practice_threshold,
                    practice_reward,
                    tier_label: tier.title(definition.religious),
                },
            );
        }
        self
    }
}
#[derive(Clone, Copy, Default)]
pub(crate) struct CharacterSheetActions<'a> {
    pub(super) cooking_href: Option<&'a str>,
    pub(super) cooking_open: bool,
    pub(super) examination_action: Option<&'a str>,
    pub(super) examination_open: bool,
}

#[derive(Clone, Copy)]
pub(super) enum SkillAction<'a> {
    Get {
        href: &'a str,
        label: &'a str,
        open: bool,
    },
    Post {
        href: &'a str,
        label: &'a str,
        open: bool,
    },
}

pub(super) fn skill_action_icon(
    name: &str,
    icon: &str,
    action: SkillAction<'_>,
    inside_form: bool,
) -> Markup {
    let (href, label, open) = match action {
        SkillAction::Get { href, label, open } | SkillAction::Post { href, label, open } => {
            (href, label, open)
        }
    };
    html! {
        @match action {
            SkillAction::Get { .. } => {
                a class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                    href=(href) title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                    data-dialog-opener=(href) {
                    span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                    @if open { span class="sr-only" { " (open)" } }
                }
            }
            SkillAction::Post { .. } => {
                @if inside_form {
                    button type="submit" class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                        formaction=(href) formmethod="post"
                        title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                        data-dialog-opener=(href) {
                        span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                        @if open { span class="sr-only" { " (open)" } }
                    }
                } @else {
                    form method="post" action=(href) class="character-menu-button-form" {
                        button type="submit" class=(if open { "character-menu-button is-open" } else { "character-menu-button" })
                            title=(label) aria-label=(label) aria-haspopup="dialog" aria-expanded=(open)
                            data-dialog-opener=(href) {
                            span class="stat-icon" style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')")) aria-hidden="true" {}
                            @if open { span class="sr-only" { " (open)" } }
                        }
                    }
                }
            }
        }
        span class="sr-only" { (name) }
    }
}

pub(super) fn party_skills_rail(
    title: &str,
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
                            title, skills, head_health, upper_health, lower_health, Some(schedule),
                            activity_preview, professes_religion, prayer_religion_check,
                            training_religion_id.and_then(OfficialReligion::from_id),
                            combat_profile, action.starts_with("/locations/settlement/"),
                            actions,
                        ))
                        div class="schedule-save-status" data-schedule-save-status role="status" aria-live="polite" hidden {
                            span { "Schedule could not be saved." }
                            button type="button" data-schedule-retry { "Retry" }
                        }
                    }
                    @if action.starts_with("/locations/settlement/") {
                        (immediate_activity_dialog(&action.replace("/schedule", "/activity")))
                    }
                    script src="/static/training-schedule.js?v=apprentice-system-1" {}
                    script src="/static/immediate-activity.js?v=manual-activities-1" {}
                } @else {
                    (skills_table(
                        title, skills, head_health, upper_health, lower_health, None, None,
                        professes_religion, prayer_religion_check,
                        training_religion_id.and_then(OfficialReligion::from_id),
                        combat_profile, false,
                        actions,
                    ))
                    script src="/static/training-schedule.js?v=apprentice-system-1" {}
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
                    @if skills.will_hours > 0.0 { (party_skill_row("Will", "will", Skill::Will, skills.will_hours, head_health, schedule.is_some(), None)) }
                    (social_skill_rows(skills, head_health, schedule))
                    @if skills.medicine_hours > 0.0 { (party_skill_row("Medicine", "medicine", Skill::Medicine, skills.medicine_hours, head_health, schedule.is_some(), actions.examination_action.map(|href| SkillAction::Post { href, label: "Perform medical examination (15 minutes)", open: actions.examination_open }))) }
                    (party_skill_row("Cooking", "cooking", Skill::Cooking, skills.cooking_hours, head_health, schedule.is_some(), actions.cooking_href.map(|href| SkillAction::Get { href, label: "Open cooking menu", open: actions.cooking_open })))
                    (religion_skill_rows(skills, head_health, schedule, training_religion))
                    (bestiary_skill_rows(skills, head_health, schedule.is_some()))
                    (surgery_skill_rows(skills, upper_health, schedule.is_some()))
                    (language_skill_rows(skills, schedule.is_some()))
                    (combat_skill_rows(skills, head_health, upper_health, lower_health, schedule, combat_profile))
                    @if skills.stealth_hours > 0.0 { (party_skill_row("Stealth", "stealth", Skill::Stealth, skills.stealth_hours, upper_health, schedule.is_some(), None)) }
                    (terrain_skill_rows(skills, schedule.is_some()))
                    @if skills.tailoring_hours > 0.0 { (party_skill_row("Tailoring", "sewing-needle", Skill::Tailoring, skills.tailoring_hours, upper_health, schedule.is_some(), None)) }
                    @if skills.smithing_hours > 0.0 { (party_skill_row("Smithing", "smithing", Skill::Smithing, skills.smithing_hours, upper_health, schedule.is_some(), None)) }
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
                            None,
                            if professes_religion {
                                "Prayer trains the professed Religion at 25% speed; morale depends on party knowledge and satisfies Fervor-driven needs."
                            } else {
                                "Meditation gives modest morale independently of party Religion knowledge and does not train Religion or create Fervor."
                            },
                        ))
                        (schedule_special_row("Combat Training", "crossed-swords", "combat_training_minutes", schedule.downtime.combat_training_minutes, true, immediate_actions, ActivityEffectRates::default(), None, None, "Sparring and target practice train equipped Combat skills together with Will and Balance."))
                        (schedule_special_row("Carousing", "beer-stein", "carousing_minutes", schedule.downtime.carousing_minutes, true, immediate_actions, ActivityEffectRates::carousing(), None, None, "Drink and socialize to improve morale and train Humor at 25% speed, at a small cost to Virtue."))
                        @if let Some(service_id) = schedule.downtime.apprenticeship_service_id.as_deref() {
                            (schedule_service_selection("apprenticeship_service_id", service_id))
                            (schedule_special_row(&format!("Apprenticeship — {}", profession_label(service_id)), "open-book", "apprenticeship_minutes", schedule.downtime.apprenticeship_minutes, true, immediate_actions && preview.profession.contains_key(service_id), ActivityEffectRates::default(), None, preview.profession.get(service_id), "Pay one coin per completed eight hours of instruction in an enrolled profession. Religious students are called novices."))
                        }
                        @if let Some(service_id) = schedule.downtime.profession_service_id.as_deref() {
                            (schedule_service_selection("profession_service_id", service_id))
                            @if let Some(profession) = preview.profession.get(service_id) {
                                @if profession.tier_label != "apprentice" && profession.tier_label != "novice" {
                                    @let religious = service_id == "religion";
                                    (schedule_special_row(&format!("Profession Practice — {}", profession_label(service_id)), if religious { "holy-symbol" } else { "anvil" }, "profession_practice_minutes", schedule.downtime.profession_practice_minutes, true, immediate_actions, ActivityEffectRates::default(), None, Some(profession), if religious { "Practice as a cleric or teacher to serve the community and earn Virtue; teachers earn faster than clerics." } else { "Practice an enrolled profession independently. Journeymen earn one coin per eight hours; masters earn one per two hours." }))
                                }
                            }
                        }
                        (schedule_special_row("Labor", "hammer-sickle", "labor_minutes", schedule.downtime.labor_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.labor_gold_per_hour, 0.0, 0.0, LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT), None, None, "Earn coin during settlement downtime from Strength and Endurance checks; trains Will at 25% speed and generates fatigue."))
                        (schedule_special_row("Thievery", "lockpicks", "thievery_minutes", schedule.downtime.thievery_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.thievery_gold_per_hour, preview.thievery_virtue_per_hour, 0.0, 0.0), None, None, "Settlement downtime can earn coin and risk discovery while training Stealth at 25% speed."))
                        (schedule_special_row("Raiding", "mounted-knight", "raiding_minutes", schedule.downtime.raiding_minutes, true, immediate_actions, ActivityEffectRates::linear(preview.raiding_gold_per_hour, preview.raiding_virtue_per_hour, 0.0, 0.0), None, None, "Settlement downtime can earn coin and risk retaliation while feeding the equipment-derived Combat training distribution at 25% speed."))
                        @let leisure = leisure_preview(&schedule.downtime, preview.current_fatigue);
                        (schedule_special_row("Leisure", "bed", "leisure_minutes", 0, false, false, ActivityEffectRates::default(), Some(leisure), None, "Unallocated downtime first offsets baseline and activity fatigue; only surplus recovery improves morale."))
                    }
            }
        }
    }
}

fn terrain_skill_rows(skills: &CharacterSkills, schedule_context: bool) -> Markup {
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
    let rank = entries
        .iter()
        .map(|entry| entry.2.training_rank(entry.3))
        .sum::<f32>()
        / 4.0;
    html! {
        tr class="party-skill-row terrain-primary-row" data-terrain-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" { (stat_icon("Terrain", "terrain", "terrain", false)) }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar(rank, rank, "Unweighted mean; route previews use the local terrain mixture", skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-terrain-expand aria-expanded="false" aria-label="Expand Terrain skills" title="Expand Terrain" {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for (name, icon, skill, hours) in entries {
            tr class="party-skill-row terrain-detail-row" data-terrain-detail hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    (stat_icon(name, "terrain", icon, false))
                }
                td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                    @let sub_rank = skill.training_rank(hours);
                    (skill_rank_bar(sub_rank, sub_rank, &format!("{:.1} hours invested", hours.max(0.0)), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn language_skill_rows(skills: &CharacterSkills, schedule_context: bool) -> Markup {
    use adventuresim_world_schema::{OralLanguage, WrittenLanguage};
    let oral_effective = OralLanguage::ALL
        .into_iter()
        .map(|language| skills.oral_languages.effective(language))
        .fold(0.0, f32::max);
    let oral_direct = OralLanguage::ALL
        .into_iter()
        .map(|language| skills.oral_languages.direct(language).max(0.0))
        .sum::<f32>();
    let written_effective = WrittenLanguage::ALL
        .into_iter()
        .map(|language| skills.written_languages.effective(language))
        .fold(0.0, f32::max);
    let written_direct = WrittenLanguage::ALL
        .into_iter()
        .map(|language| skills.written_languages.direct(language).max(0.0))
        .sum::<f32>();
    html! {
        @for (family, effective, direct, kind) in [("Oral",oral_effective,oral_direct,"oral"),("Written",written_effective,written_direct,"written")] {
            @if effective.is_finite() && effective > 0.0 {
                tr class=(format!("party-skill-row language-primary-row language-{kind}")) {
                    th scope="row" class="party-skill-name party-skill-icon-cell" { span class=(format!("language-monogram language-{kind}")) title=(format!("{family} languages")) aria-hidden="true" { (if kind=="oral" {"O"} else {"W"}) } span class="sr-only" { (family) } }
                    td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { (skill_rank_bar((effective/1000.0).clamp(0.0,5.0),(effective/1000.0).clamp(0.0,5.0),&format!("{effective:.1} effective hours; {direct:.1} directly studied hours across {family} languages"),skill_rail_bar_options())) }
                    td class="religion-expand-cell" { button type="button" class="religion-expand-button" data-language-expand=(kind) aria-expanded="false" aria-label=(format!("Expand {family} languages")) { span class="religion-expand-chevron" aria-hidden="true" { "›" } } }
                }
                @if kind=="oral" { @for language in OralLanguage::ALL { @let descriptor=language.descriptor(); @let effective=skills.oral_languages.effective(language);
                    @if effective.is_finite() && effective > 0.0 {
                        tr class="party-skill-row language-detail-row" data-language-detail="oral" hidden { th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" { span class=(if descriptor.germanic_style {"language-monogram language-oral language-blackletter"} else {"language-monogram language-oral"}) title=(format!("{} — {}",descriptor.english,descriptor.native)) aria-hidden="true" { (descriptor.monogram) } span class="sr-only" { (descriptor.english) } } td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let direct=skills.oral_languages.direct(language).max(0.0); (skill_rank_bar((effective/1000.0).clamp(0.0,5.0),(effective/1000.0).clamp(0.0,5.0),&format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),skill_rail_bar_options())) } td class="religion-expand-cell" {} }
                    }
                }} @else { @for language in WrittenLanguage::ALL { @let descriptor=language.descriptor(); @let effective=skills.written_languages.effective(language);
                    @if effective.is_finite() && effective > 0.0 {
                        tr class="party-skill-row language-detail-row" data-language-detail="written" hidden { th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" { span class=(if descriptor.germanic_style {"language-monogram language-written language-blackletter"} else {"language-monogram language-written"}) title=(format!("{} — {}",descriptor.english,descriptor.native)) aria-hidden="true" { (descriptor.monogram) } span class="sr-only" { (descriptor.english) } } td class="party-skill-meter" colspan=[schedule_context.then_some("7")] { @let direct=skills.written_languages.direct(language).max(0.0); (skill_rank_bar((effective/1000.0).clamp(0.0,5.0),(effective/1000.0).clamp(0.0,5.0),&format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),skill_rail_bar_options())) } td class="religion-expand-cell" {} }
                    }
                }}
            }
        }
    }
}

fn religion_skill_rows(
    skills: &CharacterSkills,
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
    let primary = training_religion.unwrap_or_else(|| {
        OfficialReligion::ALL
            .into_iter()
            .max_by(|left, right| {
                skills
                    .religion_hours
                    .effective(*left)
                    .total_cmp(&skills.religion_hours.effective(*right))
            })
            .unwrap_or(OfficialReligion::RomanCatholic)
    });
    let primary_id = primary.religion_id();
    let primary_effective = skills.religion_hours.effective(primary);
    let primary_direct = skills.religion_hours.direct(primary);
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
                (skill_rank_bar(
                    Skill::Religion.training_rank(primary_effective),
                    Skill::Religion.training_rank(primary_effective) * health.clamp(0.0, 1.0),
                    &format!("{primary_effective:.1} effective hours; {primary_direct:.1} directly studied hours"),
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
                    (skill_rank_bar(
                        Skill::Religion.training_rank(effective),
                        Skill::Religion.training_rank(effective) * health.clamp(0.0, 1.0),
                        &format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),
                        skill_rail_bar_options(),
                    ))
                }
                td class="religion-expand-cell" {}
            }
          }
        }
    }
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

fn bestiary_skill_rows(skills: &CharacterSkills, health: f32, schedule_context: bool) -> Markup {
    if !BestiaryCategory::ALL
        .into_iter()
        .any(|category| skills.bestiary_hours.direct(category) > 0.0)
    {
        return html! {};
    }
    let aggregate_effective = skills.bestiary_hours.aggregate_effective();
    let total_direct = skills.bestiary_hours.total_direct();
    html! {
        tr class="party-skill-row skill-family-primary-row bestiary-primary-row"
            data-skill-family="bestiary" data-bestiary-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon("Bestiary", "bestiary", "bestiary", false))
            }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar(
                    Skill::Bestiary.training_rank(aggregate_effective),
                    Skill::Bestiary.training_rank(aggregate_effective) * health.clamp(0.0, 1.0),
                    &format!("{aggregate_effective:.1} average effective hours across all Bestiary categories; {total_direct:.1} total directly studied hours"),
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
            @let direct = skills.bestiary_hours.direct(category);
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
                        (skill_rank_bar(
                            Skill::Bestiary.training_rank(effective),
                            Skill::Bestiary.training_rank(effective) * health.clamp(0.0, 1.0),
                            &format!("{effective:.1} effective hours; {direct:.1} directly studied hours"),
                            skill_rail_bar_options(),
                        ))
                    }
                    td class="religion-expand-cell" {}
                }
            }
        }
    }
}

fn surgery_skill_rows(
    skills: &CharacterSkills,
    upper_health: f32,
    schedule_context: bool,
) -> Markup {
    let knife_transfer_percent = adventuresim_core::surgery::KNIFE_SURGERY_CORRELATION * 100.0;
    let tailoring_transfer_percent =
        adventuresim_core::surgery::TAILORING_SURGERY_CORRELATION * 100.0;
    let effective_hours = adventuresim_core::surgery::effective_surgery_hours(
        skills.surgery_hours,
        skills.knife_hours,
        skills.tailoring_hours,
    );
    if effective_hours <= 0.0 {
        return html! {};
    }
    let rank = Skill::Surgery.training_rank(effective_hours);
    html! {
        tr class="party-skill-row surgery-skill-row" data-skill="surgery" {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon("Surgery", "skills", "surgeon", false))
            }
            td class="party-skill-meter" colspan=[schedule_context.then_some("7")] {
                (skill_rank_bar(
                    rank,
                    rank * upper_health.clamp(0.0, 1.0),
                    &format!(
                        "{effective_hours:.1} effective hours; {:.1} directly trained hours; Knife transfers at {knife_transfer_percent:.0}% and Tailoring transfers at {tailoring_transfer_percent:.0}%",
                        skills.surgery_hours.max(0.0),
                    ),
                    skill_rail_bar_options(),
                ))
            }
            td class="religion-expand-cell" {}
        }
    }
}

fn social_skill_rows(
    skills: &CharacterSkills,
    health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
) -> Markup {
    let entries = [
        ("Insight", "insight", Skill::Insight, skills.insight_hours),
        (
            "Self-awareness",
            "self-awareness",
            Skill::SelfAwareness,
            skills.self_awareness_hours,
        ),
        ("Humor", "humor", Skill::Humor, skills.humor_hours),
        ("Command", "command", Skill::Command, skills.command_hours),
        (
            "Deception",
            "deception",
            Skill::Deception,
            skills.deception_hours,
        ),
        (
            "Seduction",
            "seduction",
            Skill::Seduction,
            skills.seduction_hours,
        ),
    ];
    if entries.iter().all(|entry| entry.3 <= 0.0) {
        return html! {};
    }
    let rank = entries
        .iter()
        .map(|entry| entry.2.training_rank(entry.3))
        .sum::<f32>()
        / entries.len() as f32;
    let effective_rank = rank * health.clamp(0.0, 1.0);
    html! {
        tr class="party-skill-row social-primary-row" data-social-primary {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon("Social", "skills", "social", false))
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar(rank, effective_rank, "Average of all six Social skills", skill_rail_bar_options()))
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
                    @let sub_rank = skill.training_rank(hours);
                    (skill_rank_bar(sub_rank, sub_rank * health.clamp(0.0, 1.0), &format!("{:.0} hours invested", hours.max(0.0)), skill_rail_bar_options()))
                }
                td class="religion-expand-cell" {}
            }
        }
    }
}

fn combat_skill_rows(
    skills: &CharacterSkills,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    profile: CombatTrainingProfile,
) -> Markup {
    let weights = profile.weights();
    html! {
        (combat_meta_group("Melee", "crossed-swords", schedule, &[
            ("Polearm", "spear-hook", Skill::Polearm, skills.polearm_hours, upper_health, weights[0]),
            ("Axe", "battle-axe", Skill::Axe, skills.axe_hours, upper_health, weights[1]),
            ("Bludgeon", "flanged-mace", Skill::Bludgeon, skills.bludgeon_hours, upper_health, weights[2]),
            ("Sword", "sword", Skill::Sword, skills.sword_hours, upper_health, weights[3]),
            ("Knife", "bowie-knife", Skill::Knife, skills.knife_hours, upper_health, weights[4]),
        ]))
        (combat_meta_group("Ranged", "archery-target", schedule, &[
            ("Bow", "bow-arrow", Skill::Bow, skills.bow_hours, upper_health, weights[5]),
            ("Crossbow", "crossbow", Skill::Crossbow, skills.crossbow_hours, upper_health, weights[6]),
            ("Firearm", "musket", Skill::Firearm, skills.firearm_hours, upper_health, weights[7]),
            ("Throw", "throwing-ball", Skill::Throw, skills.throw_hours, upper_health, weights[8]),
        ]))
        (combat_meta_group("Defense", "shield", schedule, &[
            ("Dodge", "dodge", Skill::Dodge, skills.dodge_hours, lower_health, weights[9]),
            ("Block", "block", Skill::Block, skills.block_hours, upper_health, weights[10]),
            ("Balance", "balance", Skill::Balance, skills.balance_hours, lower_health, weights[11]),
            ("Will", "will", Skill::Will, skills.will_hours, head_health, weights[12]),
        ]))
    }
}

fn combat_meta_group(
    name: &str,
    icon: &str,
    schedule: Option<&CharacterTrainingSchedule>,
    entries: &[(&str, &str, Skill, f32, f32, f32)],
) -> Markup {
    let relevant: Vec<_> = entries.iter().filter(|entry| entry.5 > 0.0).collect();
    let rank = relevant
        .iter()
        .map(|entry| entry.2.training_rank(entry.3))
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let effective_rank = relevant
        .iter()
        .map(|entry| entry.2.training_rank(entry.3) * entry.4.clamp(0.0, 1.0))
        .sum::<f32>()
        / relevant.len().max(1) as f32;
    let included = relevant
        .iter()
        .map(|entry| entry.0)
        .collect::<Vec<_>>()
        .join(", ");
    html! {
        tr class="party-skill-row combat-primary-row" data-combat-primary=(name.to_ascii_lowercase()) {
            th scope="row" class="party-skill-name party-skill-icon-cell" {
                (stat_icon(name, "skills", icon, false))
            }
            td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                (skill_rank_bar(rank, effective_rank, &format!("Relevant skills: {included}"), skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {
                button type="button" class="religion-expand-button" data-combat-expand=(name.to_ascii_lowercase())
                    aria-expanded="false" aria-label=(format!("Expand {name} skills")) title=(format!("Expand {name}")) {
                    span class="religion-expand-chevron" aria-hidden="true" { "›" }
                }
            }
        }
        @for &(leaf_name, leaf_icon, skill, hours, health, weight) in entries {
            tr class="party-skill-row combat-detail-row" data-combat-detail=(name.to_ascii_lowercase()) data-combat-weight=(weight) hidden {
                th scope="row" class="party-skill-name party-skill-icon-cell religion-subskill-name" {
                    span title=[(skill == Skill::Knife).then_some("Knife means short weapons: knives, daggers, and short blades.")] {
                        (stat_icon(leaf_name, "skills", leaf_icon, false))
                    }
                }
                td class="party-skill-meter" colspan=[schedule.map(|_| "7")] {
                    @let sub_rank = skill.training_rank(hours);
                    (skill_rank_bar(sub_rank, sub_rank * health.clamp(0.0, 1.0), &format!("{:.0} hours invested", hours.max(0.0)), skill_rail_bar_options()))
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
    name: &str,
    icon: &str,
    skill: Skill,
    hours: f32,
    health: f32,
    schedule_context: bool,
    action: Option<SkillAction<'_>>,
) -> Markup {
    let rank = skill.training_rank(hours);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    let invested_hours = hours.max(0.0).floor() as u64;
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
                (skill_rank_bar(rank, effective_rank, &format!("{invested_hours} hours invested"), skill_rail_bar_options()))
            }
            td class="religion-expand-cell" {}
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
    let rank = rank.clamp(0.0, 5.0);
    let effective_rank = effective_rank.clamp(0.0, rank);
    let class = options.extra_class.map_or_else(
        || "skill-rank-bar".to_owned(),
        |extra| format!("skill-rank-bar {extra}"),
    );
    let aria_label = options
        .aria_label
        .map_or_else(|| format!("{effective_rank:.1} out of 5"), str::to_owned);
    html! {
        div class=(class) title=(title) aria-label=(aria_label)
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
        apprenticeship_service_id: schedule
            .apprenticeship_service_id
            .as_deref()
            .and_then(adventuresim_core::profession::ProfessionId::from_service_id),
        profession_practice_minutes: schedule.profession_practice_minutes,
        profession_service_id: schedule
            .profession_service_id
            .as_deref()
            .and_then(adventuresim_core::profession::ProfessionId::from_service_id),
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
) -> Markup {
    let hours = f32::from(minutes) / 60.0;
    let rates: Vec<(String, f32)> = match allocation_name {
        "combat_training_minutes" => vec![("Relevant combat skills".into(), 1.0)],
        "carousing_minutes" => vec![("Humor".into(), 0.25)],
        "labor_minutes" => vec![("Will".into(), 0.25)],
        "thievery_minutes" => vec![("Stealth".into(), 0.25)],
        "raiding_minutes" => vec![("Relevant combat skills".into(), 0.25)],
        "prayer_minutes" if label == "Prayer" => vec![("Religion".into(), 0.25)],
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
            data-profession-threshold=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { APPRENTICESHIP_REWARD_THRESHOLD } else { preview.practice_threshold })]
            data-profession-reward=[profession.map(|preview| if allocation_name == "apprenticeship_minutes" { "gold" } else { preview.practice_reward })]
            data-profession-sign=[profession.map(|_| if allocation_name == "apprenticeship_minutes" { -1 } else { 1 })]
            data-profession-tier=[profession.map(|preview| preview.tier_label)]
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
            (activity_training_cell(label, allocation_name, allocation_minutes, profession))
            td class="religion-auto-toggle-cell" {}
            (schedule_allocation_cell(allocation_name, allocation_minutes, editable))
            td class="religion-expand-cell" {}
        }
    }
}

fn schedule_service_selection(name: &str, service_id: &str) -> Markup {
    html! {
        tr hidden aria-hidden="true" {
            td colspan="9" { input type="hidden" name=(name) value=(service_id); }
        }
    }
}

fn profession_label(service_id: &str) -> &'static str {
    adventuresim_core::profession::profession_for_service(service_id)
        .map_or("profession", |profession| profession.label)
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
                        aria-label="Close activity dialog" { "x" }
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
                button type="submit" class="activity-submit" data-activity-submit { "Spend 1 hour" }
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

    #[test]
    fn social_skill_family_has_an_average_and_six_expandable_icon_rows() {
        let skills = CharacterSkills {
            character_id: 7,
            insight_hours: 100.0,
            self_awareness_hours: 80.0,
            humor_hours: 60.0,
            command_hours: 40.0,
            deception_hours: 20.0,
            seduction_hours: 10.0,
            ..CharacterSkills::default()
        };
        let markup = social_skill_rows(&skills, 1.0, None).into_string();
        assert!(markup.contains("data-social-primary"));
        assert!(markup.contains("Average of all six Social skills"));
        assert_eq!(markup.matches("data-social-detail").count(), 6);
        for icon in [
            "conversation.svg",
            "awareness.svg",
            "inner-self.svg",
            "juggler.svg",
            "crown.svg",
            "rose.svg",
        ] {
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
            self_awareness_hours: 0.0,
            humor_hours: 0.0,
            command_hours: 0.0,
            deception_hours: 0.0,
            seduction_hours: 0.0,
            medicine_hours: 0.0,
            cooking_hours: 0.0,
            religion_hours: adventuresim_world_schema::ReligionHours {
                roman_catholic: 1_000.0,
                ..Default::default()
            },
            bestiary_hours: Default::default(),
            surgery_hours: 0.0,
            oral_languages: Default::default(),
            written_languages: Default::default(),
            stealth_hours: 0.0,
            balance_hours: 0.0,
            terrain_plains_hours: 0.0,
            terrain_forest_hours: 0.0,
            terrain_hills_hours: 0.0,
            terrain_urban_hours: 0.0,
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
        let rendered = skills_table(
            "Your skills",
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
        assert!(rendered.contains("100.0 effective hours; 0.0 directly studied hours"));
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
        assert!(!rail.contains("data-activity-modal"));
        assert!(!rail.contains("data-activity-open"));
        let settlement_rail = party_skills_rail(
            "Your skills",
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
        let expected = skill_rank_bar(
            rank,
            rank * 0.2,
            "5000 hours invested",
            skill_rail_bar_options(),
        )
        .into_string();
        assert!(will_row.contains(&expected));
    }

    #[test]
    fn language_families_are_expandable_accessible_and_color_coded() {
        let skills = CharacterSkills {
            oral_languages: adventuresim_world_schema::OralLanguageHours {
                east_central: 5_000.0,
                ..Default::default()
            },
            written_languages: adventuresim_world_schema::WrittenLanguageHours {
                german: 1_000.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let rendered = language_skill_rows(&skills, false).into_string();
        assert!(rendered.contains("Expand Oral languages"));
        assert!(rendered.contains("Expand Written languages"));
        assert!(rendered.contains("language-oral language-blackletter"));
        assert!(rendered.contains("language-written language-blackletter"));
        assert!(rendered.contains(
            "5000.0 effective hours; 5000.0 directly studied hours across Oral languages"
        ));
        assert!(rendered.contains(
            "1000.0 effective hours; 1000.0 directly studied hours across Written languages"
        ));
        assert!(rendered.contains("title=\"East-central — Ostmitteldeutsch\""));
        assert!(rendered.contains("5000.0 effective hours; 5000.0 directly studied hours"));
        assert!(rendered.contains("title=\"Latin — Latine\""));
        assert!(!rendered.contains("title=\"Romani — Romani\""));
        assert_eq!(rendered.matches("data-language-detail=\"oral\"").count(), 4);
        assert_eq!(
            rendered.matches("data-language-detail=\"written\"").count(),
            3
        );
    }

    #[test]
    fn language_families_are_hidden_without_effective_hours() {
        let rendered = language_skill_rows(&CharacterSkills::default(), false).into_string();
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
            activity_training_cell("Combat Training", "combat_training_minutes", 120, None)
                .into_string();
        assert!(combat.contains(">+2.00h<"));
        assert!(combat.contains("Relevant combat skills: +2.00h"));

        let carousing =
            activity_training_cell("Carousing", "carousing_minutes", 120, None).into_string();
        assert!(carousing.contains(">+0.50h<"));
        assert!(carousing.contains("Humor: +0.50h"));

        let profession = ProfessionActivityPreview {
            training_rates: vec![
                ("Medicine".into(), 0.5),
                ("Human knowledge".into(), 1.0 / 6.0),
                ("Surgery".into(), 1.0 / 3.0),
            ],
            apprenticeship_accrued: 0,
            practice_accrued: 0,
            practice_threshold: 8 * 60 * PROFESSION_ACCRUAL_SCALE,
            practice_reward: "gold",
            tier_label: "apprentice",
        };
        let apprenticeship = activity_training_cell(
            "Apprenticeship — herbalist",
            "apprenticeship_minutes",
            120,
            Some(&profession),
        )
        .into_string();
        assert!(apprenticeship.contains(">+2.00h<"));
        assert!(apprenticeship.contains("Medicine: +1.00h"));
        assert!(apprenticeship.contains("Human knowledge: +0.33h"));
        assert!(apprenticeship.contains("Surgery: +0.67h"));
        assert!(!apprenticeship.contains("Knife:"));
        assert!(!apprenticeship.contains("Tailoring:"));

        let leisure = activity_training_cell("Leisure", "leisure_minutes", 480, None).into_string();
        assert!(leisure.contains(">—<"));
        assert!(leisure.contains("No skill training"));
    }

    #[test]
    fn profession_preview_uses_accrual_tier_reward_and_training_distribution() {
        let threshold = APPRENTICESHIP_REWARD_THRESHOLD;
        let row = CharacterApprenticeship {
            id: 1,
            character_id: 7,
            service_id: "weapons".into(),
            religion_id: None,
            started_minute: 0,
            apprenticeship_minutes_accrued: threshold - 60 * PROFESSION_ACCRUAL_SCALE,
            practice_minutes_accrued: 0,
        };
        let journeyman = CharacterSkills {
            smithing_hours: 4_000.0,
            ..Default::default()
        };
        let preview = ActivityPreviewRates::default()
            .with_professions(Some(&journeyman), std::slice::from_ref(&row));
        let smith = preview.profession.get("weapons").unwrap();
        assert_eq!(smith.tier_label, "journeyman");
        assert_eq!(smith.practice_threshold, 8 * 60 * PROFESSION_ACCRUAL_SCALE);
        assert_eq!(
            smith.reward_delta("apprenticeship_minutes", 60),
            [-1.0, 0.0]
        );
        assert_eq!(smith.training_rates, vec![("Smithing".into(), 1.0)]);

        let master = CharacterSkills {
            smithing_hours: 25_000.0,
            ..Default::default()
        };
        let preview = ActivityPreviewRates::default().with_professions(Some(&master), &[row]);
        let smith = preview.profession.get("weapons").unwrap();
        assert_eq!(smith.tier_label, "master");
        assert_eq!(smith.practice_threshold, 2 * 60 * PROFESSION_ACCRUAL_SCALE);
        assert_eq!(
            smith.reward_delta("profession_practice_minutes", 240),
            [2.0, 0.0]
        );
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
        assert!(equipment.contains("window.location.reload()"));
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
        let rendered = bestiary_skill_rows(&skills, 1.0, false).into_string();
        assert!(rendered.contains("data-skill-family=\"bestiary\""));
        assert!(rendered.contains("/static/icons/stats/bestiary/bestiary.png"));
        assert!(rendered.contains("/static/icons/stats/bestiary/wildmen.png"));
        assert!(rendered.contains("data-bestiary-expand"));
        assert!(rendered.contains("Expand Bestiary skills"));
        assert!(rendered.contains("Wildmen"));
        assert!(rendered.contains("directly studied hours"));
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
        assert_eq!(rendered.matches("data-bestiary-detail").count(), 13);
        assert!(css.contains(".bestiary-primary-row .stat-icon,"));
        assert!(css.contains("--stat-icon-color: var(--info);"));
        assert!(css.contains("cursor: help;"));
    }

    #[test]
    fn surgery_is_a_single_leaf_row_with_direct_and_correlated_hours() {
        let skills = CharacterSkills {
            surgery_hours: 500.0,
            knife_hours: 750.0,
            tailoring_hours: 250.0,
            ..Default::default()
        };
        let rendered = surgery_skill_rows(&skills, 1.0, false).into_string();
        assert!(rendered.contains("data-skill=\"surgery\""));
        assert!(rendered.contains("750.0 effective hours"));
        assert!(rendered.contains("500.0 directly trained hours"));
        let knife_percent = adventuresim_core::surgery::KNIFE_SURGERY_CORRELATION * 100.0;
        let tailoring_percent = adventuresim_core::surgery::TAILORING_SURGERY_CORRELATION * 100.0;
        assert!(rendered.contains(&format!(
            "Knife transfers at {knife_percent:.0}% and Tailoring transfers at {tailoring_percent:.0}%"
        )));
        assert!(!rendered.contains("data-surgery-expand"));
        assert!(!rendered.contains("data-surgery-detail"));
    }

    #[test]
    fn surgery_row_uses_upper_body_health_for_its_injury_adjusted_rank() {
        let skills = CharacterSkills {
            surgery_hours: 5_000.0,
            ..Default::default()
        };
        let impaired = skills_table(
            "Skills",
            &skills,
            1.0,
            0.25,
            1.0,
            None,
            None,
            false,
            0.0,
            None,
            CombatTrainingProfile::default(),
            false,
            CharacterSheetActions::default(),
        )
        .into_string();
        let rank = Skill::Surgery.training_rank(5_000.0);

        let surgery = impaired.find("data-skill=\"surgery\"").unwrap();
        let row_start = impaired[..surgery].rfind("<tr").unwrap();
        let row_end = surgery + impaired[surgery..].find("</tr>").unwrap() + "</tr>".len();
        let surgery_row = &impaired[row_start..row_end];
        assert!(surgery_row.contains(&format!("aria-valuenow=\"{:.1}\"", rank * 0.25)));
        assert!(surgery_row.contains("class=\"rank-damage\""));
    }
}
