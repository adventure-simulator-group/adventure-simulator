use adventuresim_core::strategic_schedule::CombatTrainingProfile;
use maud::{Markup, html};

use super::{
    character_health::{
        medical_examination_popup, medical_rail, party_attributes_rail, strategic_condition_rail,
    },
    character_skills::{ActivityPreviewRates, CharacterSkillActions, party_skills_rail},
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
        (party_attributes_rail(&format!("{}'s attributes", character.name), attributes, limbs, medical, None, &[], &[]))
        (party_skills_rail(
            &format!("{}'s skills", character.name), skills, limbs, None, None, None,
            false, 0.0, None, CombatTrainingProfile::default(), CharacterSkillActions::default(),
        ))
        (medical_rail(medical, "", 0, character.id, false))
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
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    can_examine: bool,
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
) -> Markup {
    let cooking_href = location.preserve_building(format!(
        "{}/party/{}?cook=true",
        location.base_path(),
        active_character.id
    ));
    let examination_action = location.preserve_building(format!(
        "{}/party/{}/examine",
        location.base_path(),
        active_character.id
    ));
    let cooking_open = cooking && medical.examination_id.is_none();
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        active_character.id
    ));
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail("Your attributes", attributes, limbs, medical, Some((&surgery_path_template, surgery_open)), injuries, projectiles))
            (strategic_condition_rail(condition, morale_sources, filth, &location.preserve_building(format!("{}/party/{}/social", location.base_path(), active_character.id)), social_open))
            (medical_rail(medical, &location.base_path(), active_character.id, active_character.id, true))
            @if let Some(demand) = religious_demand {
                (religious_demand_rail(demand, &location.base_path(), active_character.id))
            }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(
                party_members,
                Some(active_character),
                &location.base_path(),
                Some(active_character.id),
                can_examine,
            ))
            (visual_stage("character", &active_character.name, "Your identity, condition, and capabilities"))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            (medical_examination_popup(medical, location, active_character.id, limbs, injuries, projectiles))
        }
        aside class="right-sidebar" {
            (character_summary_rail(capability))
            (character_bio_rail(active_character, religion_id, notoriety, personality, true, &location.base_path()))
            @let schedule_action = format!("{}/party/{}/schedule", location.base_path(), active_character.id);
            (party_skills_rail(
                "Your skills", skills, limbs, schedule, Some(&schedule_action),
                Some(activity_preview), religion_id.is_some(), prayer_religion_check,
                religion_id.or(location.religion_id.as_deref()),
                combat_profile,
                CharacterSkillActions {
                    cooking_href: Some(&cooking_href),
                    cooking_open,
                    examination_action: can_examine.then_some(examination_action.as_str()),
                    examination_open: medical.examination_id.is_some(),
                },
            ))
        }
        @if cooking_open {
            (cooking_activity_dialog(location, active_character, inventory, food_lots, item_definitions))
        } @else if medical.examination_id.is_none() {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
    };
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
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let examination_action = location.preserve_building(format!(
        "{}/party/{}/examine",
        location.base_path(),
        selected.id
    ));
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        selected.id
    ));
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail(&selected_attributes_title, selected_attributes, selected_limbs, medical, Some((&surgery_path_template, surgery_open)), injuries, projectiles))
            (strategic_condition_rail(condition, morale_sources, filth, &location.preserve_building(format!("{}/party/{}/social", location.base_path(), selected.id)), social_open))
            (medical_rail(medical, &location.base_path(), active_character.id, selected.id, true))
        }
        @if medical.examination_id.is_none() {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(
                party_members,
                Some(active_character),
                &location.base_path(),
                Some(selected.id),
                can_examine,
            ))
            (visual_stage("character", &selected.name, "Party member identity and capabilities"))
            (player_chat_area(selected, active_character))
            (medical_examination_popup(medical, location, selected.id, selected_limbs, injuries, projectiles))
        }
        aside class="right-sidebar" {
            (character_summary_rail(capability))
            (character_bio_rail(
                selected,
                religion_id,
                notoriety,
                personality,
                selected.id == active_character.id,
                &location.base_path(),
            ))
            (party_skills_rail(
                &selected_skills_title, selected_skills, selected_limbs, None, None, None,
                religion_id.is_some(), 0.0, religion_id.or(location.religion_id.as_deref()),
                combat_profile,
                CharacterSkillActions {
                    examination_action: can_examine.then_some(examination_action.as_str()),
                    examination_open: medical.examination_id.is_some(),
                    ..Default::default()
                },
            ))
            @if selected.id != active_character.id {
                @if active_character.party_id == selected.party_id {
                    @if active_party.is_some_and(|party| party.leader_id == selected.id) {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), active_character.id)) {
                                button type="submit" class="btn btn-danger btn-block" { "Leave party" }
                            }
                        }))
                    } @else {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), selected.id)) {
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
        }
    };
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
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    can_renounce: bool,
    location_path: &str,
) -> Markup {
    let virtue = if notoriety.abs() < 0.0005 {
        0.0
    } else {
        -notoriety
    };
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
