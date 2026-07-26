use adventuresim_core::strategic_schedule::CombatTrainingProfile;
use maud::{Markup, html};

use super::{
    character_health::{
        party_attributes_rail, physiology_controls, physiology_dialog, strategic_condition_rail,
    },
    character_skills::{ActivityPreviewRates, CharacterSheetActions, party_skills_rail},
    chrome::{party_portrait_overlay, visual_stage},
    context::LocationView,
    social::{player_chat_area, settlement_chat_area},
    trade::{cooking_activity_dialog, religious_demand_rail},
};
use crate::medical::MedicalPresentation;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterSkills,
    CharacterStrategicCondition, CharacterTrainingSchedule, FoodLot, InventoryItem, ItemDefinition,
    LimbInjury, Party, RetainedProjectile,
};
use crate::templates::{decorative_game_icon, sidebar_section};

fn character_summary_rail(capability: Option<&CharacterCapability>) -> Markup {
    let tags = capability
        .map(CharacterCapability::summary_tags)
        .unwrap_or_default();
    html! {
        (sidebar_section("Summary", html! {
            @if tags.is_empty() {
                p class="text-muted small-copy" { "No notable capabilities." }
            } @else {
                div class="character-summary-tags" aria-label="Character capability summary" {
                    @for tag in tags { span class="character-summary-tag" { (tag) } }
                }
            }
        }))
    }
}

pub(crate) fn character_stats_panel(
    character: &Character,
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    medical: &MedicalPresentation,
) -> Markup {
    html! {
        (character_summary_rail(capability))
        (party_attributes_rail(
            &format!("{}'s attributes", character.name),
            attributes,
            limbs,
            medical,
            Some("physiology-chart-dialog"),
            None,
            &[],
            &[],
        ))
        (party_skills_rail(
            &format!("{}'s skills", character.name), attributes, skills, limbs, None, None, None,
            false, 0.0, None, CombatTrainingProfile::default(), CharacterSheetActions::default(),
        ))
        (physiology_dialog(medical, "physiology-chart-dialog", &character.name))
    }
}

pub(crate) struct CharacterSheetView<'a> {
    pub character: &'a Character,
    pub capability: Option<&'a CharacterCapability>,
    pub attributes: Option<&'a CharacterAttributes>,
    pub skills: Option<&'a CharacterSkills>,
    pub limbs: Option<&'a CharacterLimbs>,
    pub personality: Option<&'a crate::spacetimedb::CharacterPersonality>,
    pub medical: &'a MedicalPresentation,
    pub combat_profile: CombatTrainingProfile,
    pub religion_id: Option<&'a str>,
    pub training_religion_id: Option<&'a str>,
    pub virtue: f32,
    pub attributes_title: &'a str,
    pub skills_title: &'a str,
    pub description: &'a str,
    pub can_renounce: bool,
    pub physiology_dialog_id: Option<&'a str>,
    pub surgery: Option<(&'a str, Option<&'a str>)>,
    pub injuries: &'a [LimbInjury],
    pub projectiles: &'a [RetainedProjectile],
    pub schedule: Option<&'a CharacterTrainingSchedule>,
    pub schedule_action: Option<&'a str>,
    pub activity_preview: Option<ActivityPreviewRates>,
    pub professes_religion: bool,
    pub prayer_religion_check: f32,
    pub skill_actions: CharacterSheetActions<'a>,
    pub location_path: &'a str,
    pub center_before: Markup,
    pub portraits: Markup,
    pub center_after: Markup,
    pub left_after: Markup,
    pub right_after: Markup,
    pub after: Markup,
}

/// The single selected-character sheet used by live party members and
/// preview-only starting candidates. Callers supply party-only controls in the
/// extension slots; the attributes, limbs, bio, skills, and activities markup
/// remains owned here.
pub(crate) fn character_sheet_markup(view: CharacterSheetView<'_>) -> Markup {
    html! {
        aside class="left-sidebar" {
            (party_attributes_rail(
                view.attributes_title,
                view.attributes,
                view.limbs,
                view.medical,
                view.physiology_dialog_id,
                view.surgery,
                view.injuries,
                view.projectiles,
            ))
            (view.left_after)
        }
        main class="center-content settlement-main party-member-stage" {
            (view.center_before)
            (view.portraits)
            (visual_stage("character", &view.character.name, view.description))
            (view.center_after)
        }
        aside class="right-sidebar" {
            (character_summary_rail(view.capability))
            (character_bio_rail(
                view.character,
                view.religion_id,
                view.virtue,
                view.personality,
                view.can_renounce,
                view.location_path,
            ))
            (party_skills_rail(
                view.skills_title,
                view.attributes,
                view.skills,
                view.limbs,
                view.schedule,
                view.schedule_action,
                view.activity_preview,
                view.professes_religion,
                view.prayer_religion_check,
                view.training_religion_id,
                view.combat_profile,
                view.skill_actions,
            ))
            (view.right_after)
        }
        (view.after)
    }
}

pub(crate) fn character_visual_preview(character: &Character) -> Markup {
    visual_stage("character", &character.name, "Adventurer profile")
}

/// Active character's combined strategic view.
pub fn party_personal_page(
    location: &LocationView,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    prayer_religion_check: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    combat_profile: CombatTrainingProfile,
    activity_preview: ActivityPreviewRates,
    religious_demand: Option<&crate::spacetimedb::ReligiousDemand>,
    virtue: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    _can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    cooking: bool,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    item_definitions: &[ItemDefinition],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
    foraging_dialog: Option<Markup>,
) -> Markup {
    let cooking_href = location.preserve_building(format!(
        "{}/party/{}?cook=true",
        location.base_path(),
        active_character.id
    ));
    let cooking_open = cooking;
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        active_character.id
    ));
    let location_path = location.base_path();
    let foraging_href = location.preserve_building(format!(
        "{location_path}/party/{}?forage=true",
        active_character.id,
    ));
    let schedule_action = format!("{location_path}/party/{}/schedule", active_character.id);
    let social_path = location.preserve_building(format!(
        "{location_path}/party/{}/social",
        active_character.id
    ));
    let left_after = html! {
        (strategic_condition_rail(condition, morale_sources, filth, &social_path, social_open))
        (physiology_controls(
            medical,
            &format!("{location_path}/party/{}", active_character.id),
            inventory,
            item_definitions,
        ))
        @if let Some(demand) = religious_demand {
            (religious_demand_rail(demand, &location_path, active_character.id))
        }
    };
    let portraits = party_portrait_overlay(
        party_members,
        Some(active_character),
        &location_path,
        Some(active_character.id),
        false,
    );
    let center_after = settlement_chat_area(&active_character.name, Some(active_character));
    let foraging_open = foraging_dialog.is_some();
    let after = html! {
        (physiology_dialog(medical, "physiology-chart-dialog", &active_character.name))
        @if cooking_open {
            (cooking_activity_dialog(location, active_character, inventory, food_lots, item_definitions))
        } @else if let Some(dialog) = foraging_dialog {
            (dialog)
        } @else {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
    };
    let content = character_sheet_markup(CharacterSheetView {
        character: active_character,
        capability,
        attributes,
        skills,
        limbs,
        personality,
        medical,
        combat_profile,
        religion_id,
        training_religion_id: religion_id.or(location.religion_id.as_deref()),
        virtue,
        attributes_title: "Your attributes",
        skills_title: "Your skills",
        description: "Your identity, condition, and capabilities",
        can_renounce: true,
        physiology_dialog_id: Some("physiology-chart-dialog"),
        surgery: Some((&surgery_path_template, surgery_open)),
        injuries,
        projectiles,
        schedule,
        schedule_action: Some(&schedule_action),
        activity_preview: Some(activity_preview),
        professes_religion: religion_id.is_some(),
        prayer_religion_check,
        skill_actions: CharacterSheetActions {
            cooking_href: Some(&cooking_href),
            cooking_open,
            foraging_href: Some(&foraging_href),
            foraging_open,
        },
        location_path: &location_path,
        center_before: html! {},
        portraits,
        center_after,
        left_after,
        right_after: html! {},
        after,
    });
    location.render_layout("Party", content, Some(&active_character.name))
}

pub fn party_stats_page(
    location: &LocationView,
    selected: &Character,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    selected_attributes: Option<&CharacterAttributes>,
    selected_skills: Option<&CharacterSkills>,
    selected_limbs: Option<&CharacterLimbs>,
    combat_profile: CombatTrainingProfile,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    active_party: Option<&Party>,
    selected_party: Option<&Party>,
    virtue: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    _can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        selected.id
    ));
    let location_path = location.base_path();
    let social_path =
        location.preserve_building(format!("{location_path}/party/{}/social", selected.id));
    let left_after =
        strategic_condition_rail(condition, morale_sources, filth, &social_path, social_open);
    let portraits = party_portrait_overlay(
        party_members,
        Some(active_character),
        &location_path,
        Some(selected.id),
        false,
    );
    let center_after = player_chat_area(selected, active_character);
    let right_after = html! {
        @if selected.id != active_character.id {
                @if active_character.party_id == selected.party_id {
                    @if active_party.is_some_and(|party| party.leader_id == selected.id) {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{location_path}/party/{}/remove", active_character.id)) {
                                button type="submit" class="btn btn-danger btn-block" { "Leave party" }
                            }
                        }))
                    } @else {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{location_path}/party/{}/remove", selected.id)) {
                                button type="submit" class="btn btn-danger btn-block" {
                                    @if active_party.is_some_and(|party| party.leader_id == active_character.id) { "Kick from party" }
                                    @else { "Request kick" }
                                }
                            }
                        }))
                    }
                } @else if let Some(party) = selected_party {
                    (sidebar_section("Party", html! {
                        p { (&party.name) }
                        form method="post" action=(format!("/parties/{}/join-general", party.id)) {
                            button type="submit" class="btn btn-primary btn-block" { "Request to join party" }
                        }
                    }))
                }
            }
    };
    let after = html! {
        @if let Some(dialog) = character_action_dialog { (dialog) }
        (physiology_dialog(medical, "physiology-chart-dialog", &selected.name))
    };
    let content = character_sheet_markup(CharacterSheetView {
        character: selected,
        capability,
        attributes: selected_attributes,
        skills: selected_skills,
        limbs: selected_limbs,
        personality,
        medical,
        combat_profile,
        religion_id,
        training_religion_id: religion_id.or(location.religion_id.as_deref()),
        virtue,
        attributes_title: &selected_attributes_title,
        skills_title: &selected_skills_title,
        description: "Party member identity and capabilities",
        can_renounce: selected.id == active_character.id,
        physiology_dialog_id: Some("physiology-chart-dialog"),
        surgery: Some((&surgery_path_template, surgery_open)),
        injuries,
        projectiles,
        schedule: None,
        schedule_action: None,
        activity_preview: None,
        professes_religion: religion_id.is_some(),
        prayer_religion_check: 0.0,
        skill_actions: CharacterSheetActions::default(),
        location_path: &location_path,
        center_before: html! {},
        portraits,
        center_after,
        left_after,
        right_after,
        after,
    });
    location.render_layout("Party stats", content, Some(&active_character.name))
}

pub(super) fn religion_name(religion_id: Option<&str>) -> &'static str {
    match religion_id {
        Some("western_church") => "Western Church",
        Some("roman_catholic") => "Roman Catholic",
        Some("lutheran") => "Lutheran",
        Some("reformed") => "Reformed",
        Some("anglican") => "Anglican",
        Some("eastern_orthodox") => "Eastern Orthodox",
        Some("islamic") => "Islamic",
        Some("judaism") => "Jewish",
        Some("old_faith") => "Old Faith",
        Some(_) => "Unknown faith",
        None => "None",
    }
}

fn character_bio_rail(
    character: &Character,
    religion_id: Option<&str>,
    virtue: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    can_renounce: bool,
    location_path: &str,
) -> Markup {
    html! {
        (sidebar_section("Bio", html! {
            dl class="character-bio" {
                div { dt class="metric-label" { (decorative_game_icon("calendar")) span { "Age" } } dd { (character.age_years) " years" } }
                div { dt class="metric-label" { (decorative_game_icon("spiked-halo")) span { "Virtue" } } dd title="Immoral activities reduce Virtue; consequences will be added later." { (format!("{virtue:+.1}")) } }
                @if let Some(personality) = personality {
                    @let tags = personality_tags(personality);
                    @if !tags.is_empty() {
                        div { dt { "Personality" } dd class="personality-tags" {
                            @for (name, description) in tags {
                                span class="personality-tag" title=(description) { (name) }
                            }
                        } }
                    }
                }
                div class="character-religion" {
                    dt class="metric-label" { (decorative_game_icon("holy-symbol")) span { "Religion" } }
                    dd {
                        (religion_name(religion_id))
                        @if can_renounce && religion_id.is_some() {
                            form method="post" action=(format!("{location_path}/party/{}/religion/renounce", character.id)) class="character-religion-action" {
                                button type="submit" class="btn btn-danger" title="Renounce this faith" { "Renounce" }
                            }
                        }
                    }
                }
                @if character.current_settlement_id.is_some() {
                    div {
                        dt class="metric-label" { (decorative_game_icon("shield")) span { "Organizations" } }
                        dd {
                            a class="btn" href=(format!("{location_path}/party/{}/organizations", character.id)) {
                                "Manage memberships"
                            }
                        }
                    }
                }
            }
        }))
    }
}

fn personality_tags(
    personality: &crate::spacetimedb::CharacterPersonality,
) -> Vec<(&'static str, &'static str)> {
    use crate::spacetimedb::{
        Conscience::*, Conviction::*, Drive::*, Hygiene::*, Nerve::*, Outlook::*, SelfRegard::*,
        Sociability::*, Temperance::*,
    };
    let mut tags = Vec::new();
    match personality.nerve {
        Brave => tags.push(("Brave", "Morale loss from being outmatched ×0.5.")),
        Fearful => tags.push(("Fearful", "Morale loss from being outmatched ×2.")),
        _ => {}
    }
    match personality.drive {
        Ambitious => tags.push(("Ambitious", "Morale from victories and defeats ×1.5.")),
        Content => tags.push(("Content", "Morale from victories and defeats ×0.5.")),
        _ => {}
    }
    match personality.outlook {
        Sanguine => tags.push((
            "Sanguine",
            "Positive morale ×1.25; negative morale ×0.75; negative-event duration ×0.5.",
        )),
        Brooding => tags.push((
            "Brooding",
            "Positive morale ×0.75; negative morale ×1.25; negative-event duration ×2.",
        )),
        _ => {}
    }
    match personality.sociability {
        Gregarious => tags.push(("Gregarious", "Morale restored by allies ×1.5.")),
        Solitary => tags.push(("Solitary", "Morale restored by allies ×0.5.")),
        _ => {}
    }
    match personality.conscience {
        Compassionate => tags.push((
            "Compassionate",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        Callous => tags.push((
            "Callous",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        Cruel => tags.push((
            "Cruel",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        _ => {}
    }
    match personality.self_regard {
        Proud => tags.push(("Proud", "Morale from victory ×1.5; morale from defeat ×3.")),
        Humble => tags.push(("Humble", "Morale from victories and defeats ×0.75.")),
        _ => {}
    }
    match personality.conviction {
        Zealous => tags.push(("Zealous", "Morale from religious sources and events ×1.5.")),
        Irreverent => tags.push((
            "Irreverent",
            "Morale from religious sources and events ×0.5.",
        )),
        _ => {}
    }
    match personality.hygiene {
        Slovenly => tags.push(("Slovenly", "Filth morale penalty ×0.")),
        Cleanly => tags.push((
            "Cleanly",
            "Filth morale penalty ×2.5; +2 morale while completely clean.",
        )),
        _ => {}
    }
    match personality.temperance {
        Temperate => tags.push((
            "Temperate",
            "Automatic alcohol morale bonus +0; missed-drink morale penalty -0.",
        )),
        Drunkard => tags.push((
            "Drunkard",
            "Wants a heavy drink every evening: +5 morale when satisfied, -5 when missed.",
        )),
        _ => {}
    }
    tags
}

#[cfg(test)]
mod personality_tests {
    use super::*;
    use crate::spacetimedb::*;

    #[test]
    fn neutral_axes_are_omitted_from_bio_tags() {
        let personality = CharacterPersonality {
            character_id: 1,
            nerve: Nerve::Brave,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Cruel,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
            hygiene: Hygiene::Neutral,
            temperance: Temperance::Neutral,
        };
        let tags = personality_tags(&personality);
        assert_eq!(
            tags.iter().map(|tag| tag.0).collect::<Vec<_>>(),
            ["Brave", "Cruel"]
        );
    }

    #[test]
    fn every_visible_tag_explains_its_numeric_morale_effect() {
        let profiles = [
            CharacterPersonality {
                character_id: 1,
                nerve: Nerve::Brave,
                drive: Drive::Ambitious,
                outlook: Outlook::Sanguine,
                sociability: Sociability::Gregarious,
                conscience: Conscience::Compassionate,
                self_regard: SelfRegard::Proud,
                conviction: Conviction::Zealous,
                hygiene: Hygiene::Cleanly,
                temperance: Temperance::Temperate,
            },
            CharacterPersonality {
                character_id: 2,
                nerve: Nerve::Fearful,
                drive: Drive::Content,
                outlook: Outlook::Brooding,
                sociability: Sociability::Solitary,
                conscience: Conscience::Callous,
                self_regard: SelfRegard::Humble,
                conviction: Conviction::Irreverent,
                hygiene: Hygiene::Slovenly,
                temperance: Temperance::Drunkard,
            },
            CharacterPersonality {
                character_id: 3,
                nerve: Nerve::Neutral,
                drive: Drive::Neutral,
                outlook: Outlook::Neutral,
                sociability: Sociability::Neutral,
                conscience: Conscience::Cruel,
                self_regard: SelfRegard::Neutral,
                conviction: Conviction::Neutral,
                hygiene: Hygiene::Neutral,
                temperance: Temperance::Neutral,
            },
        ];

        for profile in &profiles {
            for (tag, description) in personality_tags(profile) {
                assert!(
                    description
                        .chars()
                        .any(|character| character.is_ascii_digit()),
                    "{tag} tooltip lacks a numeric morale effect: {description}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_imported_religions_have_ui_labels() {
        for (id, label) in [
            ("roman_catholic", "Roman Catholic"),
            ("lutheran", "Lutheran"),
            ("reformed", "Reformed"),
            ("anglican", "Anglican"),
            ("eastern_orthodox", "Eastern Orthodox"),
            ("islamic", "Islamic"),
            ("judaism", "Jewish"),
        ] {
            assert_eq!(religion_name(Some(id)), label);
        }
    }
}
