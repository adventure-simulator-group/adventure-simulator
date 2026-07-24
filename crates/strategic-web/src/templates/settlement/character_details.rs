use super::*;

pub(super) fn character_summary_rail(capability: Option<&CharacterCapability>) -> Markup {
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

pub(super) fn character_bio_rail(
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

pub(super) fn personality_tags(
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
