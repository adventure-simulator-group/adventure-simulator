//! Character selection and creation templates.

use maud::{Markup, html};

use super::settlement::{
    CharacterPortraitView, CharacterSheetActions, CharacterSheetView, character_portrait_overlay,
    character_sheet_markup,
};
use super::{entry_layout, panel, sidebar_section};
use crate::medical::MedicalPresentation;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterPersonality,
    CharacterSkills, Conscience, Conviction, Drive, Hygiene, Nerve, Outlook, SelfRegard,
    Sociability, Temperance,
};
use adventuresim_core::starting_character::{
    StartingCharacterSpec, StartingPersonalityTrait, StartingSlot,
};
use adventuresim_core::strategic_schedule::CombatTrainingProfile;

/// List all characters and select the adventurer who enters the strategic layer.
pub fn characters_list_page(characters: &[Character], current_character_id: Option<u64>) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Choose an adventurer", html! {
                p class="small-copy text-muted" { "A character must be selected before entering the strategic layer." }
            }))
        }

        main class="center-content" {
            h2 class="page-title" { "Select your adventurer" }
            @if characters.is_empty() {
                div class="center-welcome" {
                    p { "No persisted adventurers are available." }
                }
            } @else {
                div class="character-select-grid" {
                    @for character in characters {
                        @let is_current = current_character_id == Some(character.id);
                        (panel(&character.name, html! {
                            div class="stat-grid" {
                                div class="stat-item" {
                                    span class="stat-label" { "Status" }
                                    span class="stat-value" {
                                        @if character.alive { "Alive" } @else { span class="badge badge-danger" { "Dead" } }
                                    }
                                }
                            }
                            @if is_current {
                                p class="text-accent small-copy" {
                                    @if character.alive { "Currently selected" } @else { "Currently viewed" }
                                }
                            }
                            form action=(format!("/characters/{}/select", character.id)) method="post" class="mt-1" {
                                button type="submit" class="btn btn-primary btn-block character-select-action" {
                                    @if !character.alive { "View " (&character.name) }
                                    @else if is_current { "Continue" }
                                    @else { "Play as " (&character.name) }
                                }
                            }
                        }))
                    }
                }
            }
        }

        aside class="right-sidebar" {
            (sidebar_section("Starting settlement", html! {
                p class="small-copy text-muted" { "New adventurers begin at a random settlement with basic supplies." }
            }))
        }
    };

    entry_layout("Select Adventurer", content)
}

#[cfg(test)]
mod tests {
    use super::characters_list_page;
    use crate::spacetimedb::Character;

    #[test]
    fn dead_character_is_labeled_and_uses_view_wording() {
        let character = Character {
            id: 7,
            name: "Fallen Adventurer".into(),
            xp: 0,
            level: 1,
            gold: 100,
            current_settlement_id: Some("ironforge".into()),
            current_case_site_id: None,
            party_id: Some("solo-7".into()),
            age_years: 30,
            alive: false,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };

        let markup = characters_list_page(&[character], Some(7)).into_string();
        assert!(markup.contains("Dead"));
        assert!(markup.contains("Currently viewed"));
        assert!(markup.contains("View Fallen Adventurer"));
        assert!(!markup.contains("Play as Fallen Adventurer"));
        assert!(!markup.contains(">Continue<"));
    }
}

const PROTOTYPE_NOTICE: &str = "Early prototype: All text and images are placeholders. Features and saved progress may change or be reset during development.";

pub fn character_candidates_bootstrap_page(version: u16) -> Markup {
    let content = html! {
        main class="center-content candidate-bootstrap" {
            p class="prototype-disclaimer" role="note" { (PROTOTYPE_NOTICE) }
            h2 class="page-title" { "Gathering candidates…" }
            p class="small-copy text-muted" { "Preparing your first company." }
            noscript { p role="alert" { "JavaScript is required to prepare a private candidate roster." } }
            div data-candidate-bootstrap data-generator-version=(version) {}
            script src="/static/character-candidates.js?v=2" defer {}
        }
    };
    entry_layout("Choose Your Adventurer", content)
}

pub fn character_candidates_page(
    version: u16,
    seed: &str,
    candidates: &[StartingCharacterSpec],
    selected: Option<u8>,
) -> Markup {
    let presentations = candidates
        .iter()
        .map(CandidatePresentation::from)
        .collect::<Vec<_>>();
    let selected_slot = selected.unwrap_or(0) as usize;
    let candidate = &presentations[selected_slot];
    let portraits = presentations
        .iter()
        .enumerate()
        .map(|(slot, candidate)| CharacterPortraitView {
            id: candidate.character.id,
            name: &candidate.character.name,
            alive: true,
            active: false,
            selected: selected == Some(slot as u8),
            href: format!("/characters/candidates?version={version}&seed={seed}&selected={slot}"),
            title: format!("Inspect {}", candidate.character.name),
            aria_label: format!("Inspect {}", candidate.character.name),
            decoration: None,
            badge: None,
            actions: None,
        })
        .collect::<Vec<_>>();
    let medical = MedicalPresentation::default();
    let attributes_title = format!("{}'s attributes", candidate.character.name);
    let skills_title = format!("{}'s skills", candidate.character.name);
    let portraits = character_portrait_overlay("Candidate adventurers", None, &portraits);
    let center_before = html! {
            p class="prototype-disclaimer" role="note" { (PROTOTYPE_NOTICE) }
    };
    let center_after = html! {
            @if let Some(selected) = selected {
                form action="/characters/candidates" method="post" class="candidate-play-action" data-candidate-confirm-form {
                    input type="hidden" name="version" value=(version);
                    input type="hidden" name="seed" value=(seed);
                    input type="hidden" name="slot" value=(selected);
                    button type="submit" class="btn btn-primary" {
                        "Play as " (&candidate.character.name)
                    }
                }
            }
            script src="/static/character-candidates.js?v=2" defer {}
    };
    let content = character_sheet_markup(CharacterSheetView {
        character: &candidate.character,
        capability: Some(&candidate.capability),
        attributes: Some(&candidate.attributes),
        skills: Some(&candidate.skills),
        limbs: Some(&candidate.limbs),
        personality: Some(&candidate.personality),
        medical: &medical,
        combat_profile: CombatTrainingProfile::default(),
        religion_id: None,
        training_religion_id: None,
        virtue: 0.0,
        attributes_title: &attributes_title,
        skills_title: &skills_title,
        description: "Adventurer profile",
        can_renounce: false,
        physiology_dialog_id: None,
        surgery: None,
        injuries: &[],
        projectiles: &[],
        schedule: None,
        schedule_action: None,
        activity_preview: None,
        professes_religion: false,
        prayer_religion_check: 0.0,
        skill_actions: CharacterSheetActions::default(),
        location_path: "",
        center_before,
        portraits,
        center_after,
        left_after: html! {},
        right_after: html! {},
        after: html! {},
    });

    entry_layout("Choose Your Adventurer", content)
}

struct CandidatePresentation {
    character: Character,
    attributes: CharacterAttributes,
    capability: CharacterCapability,
    limbs: CharacterLimbs,
    personality: CharacterPersonality,
    skills: CharacterSkills,
}

impl From<&StartingCharacterSpec> for CandidatePresentation {
    fn from(spec: &StartingCharacterSpec) -> Self {
        let character = Character {
            id: spec.id,
            name: spec.name.clone(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: None,
            age_years: spec.age_years,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let attributes = CharacterAttributes {
            character_id: spec.id,
            endurance: spec.attributes.endurance,
            immunity: spec.attributes.immunity,
            gut: spec.attributes.gut,
            intelligence: spec.attributes.intelligence,
            instinct: spec.attributes.instinct,
            eyesight: spec.attributes.eyesight,
            hearing: spec.attributes.hearing,
            left_arm_strength: spec.attributes.strength,
            right_arm_strength: spec.attributes.strength,
            left_leg_strength: spec.attributes.strength,
            right_leg_strength: spec.attributes.strength,
            left_arm_agility: spec.attributes.agility,
            right_arm_agility: spec.attributes.agility,
            left_leg_agility: spec.attributes.agility,
            right_leg_agility: spec.attributes.agility,
        };
        let skills = CharacterSkills {
            character_id: spec.id,
            polearm_hours: spec.skills.polearm,
            axe_hours: spec.skills.axe,
            bludgeon_hours: spec.skills.bludgeon,
            sword_hours: spec.skills.sword,
            knife_hours: spec.skills.knife,
            dodge_hours: spec.skills.dodge,
            block_hours: spec.skills.block,
            bow_hours: spec.skills.bow,
            crossbow_hours: spec.skills.crossbow,
            firearm_hours: 1000.0,
            throw_hours: spec.skills.throw,
            will_hours: spec.skills.will,
            insight_hours: spec.skills.insight,
            self_awareness_hours: 1000.0,
            humor_hours: 1000.0,
            command_hours: spec.skills.command,
            deception_hours: 1000.0,
            seduction_hours: 1000.0,
            physiology_hours: spec.skills.physiology,
            cooking_hours: spec.skills.cooking,
            religion_hours: adventuresim_world_schema::ReligionHours {
                roman_catholic: 1000.0,
                ..Default::default()
            },
            oral_languages: Default::default(),
            written_languages: Default::default(),
            stealth_hours: spec.skills.stealth,
            balance_hours: spec.skills.balance,
            terrain_plains_hours: 0.0,
            terrain_forest_hours: 0.0,
            terrain_hills_hours: 0.0,
            terrain_urban_hours: 0.0,
            bestiary_hours: spec.skills.bestiary,
            anatomy_hours: spec.skills.anatomy,
            tailoring_hours: 1000.0,
            smithing_hours: 1000.0,
        };
        let armor_slots = spec
            .inventory
            .iter()
            .filter(|item| {
                matches!(
                    item.equipped,
                    Some(
                        StartingSlot::LeftArm
                            | StartingSlot::RightArm
                            | StartingSlot::LeftLeg
                            | StartingSlot::RightLeg
                            | StartingSlot::Head
                            | StartingSlot::Chest
                            | StartingSlot::Stomach
                    )
                )
            })
            .count();
        let item_ids = spec
            .inventory
            .iter()
            .map(|item| item.item_id.as_str())
            .collect::<Vec<_>>();
        let ranged = item_ids
            .iter()
            .any(|item| item.contains("bow") || item.contains("crossbow"));
        let melee = item_ids.iter().any(|item| {
            ["sword", "spear", "axe", "mace", "club", "knife", "dagger"]
                .iter()
                .any(|weapon| item.contains(weapon))
        });
        let capability = CharacterCapability {
            character_id: spec.id,
            melee,
            ranged,
            precise: false,
            heavy: false,
            quarter_armor: armor_slots >= 2,
            half_armor: armor_slots >= 4,
            three_quarter_armor: armor_slots >= 6,
            full_armor: armor_slots >= 7,
            blunt: false,
            slash: melee,
            pierce: ranged,
            athletics: 0.0,
            endurance: spec.attributes.endurance,
            physiology: 0.0,
            anatomy: 0.0,
            knife: 0.0,
            tailoring: 0.0,
            surgery: 0.0,
            command: 0.0,
            religion: 0.0,
            weapon_precision: 0.0,
        };
        Self {
            character,
            attributes,
            capability,
            limbs: CharacterLimbs {
                character_id: spec.id,
                left_arm_health: 1.0,
                right_arm_health: 1.0,
                left_leg_health: 1.0,
                right_leg_health: 1.0,
                head_health: 1.0,
                chest_health: 1.0,
                stomach_health: 1.0,
            },
            personality: candidate_personality(spec),
            skills,
        }
    }
}

fn candidate_personality(spec: &StartingCharacterSpec) -> CharacterPersonality {
    let mut personality = CharacterPersonality {
        character_id: spec.id,
        nerve: Nerve::Neutral,
        drive: Drive::Neutral,
        outlook: Outlook::Neutral,
        sociability: Sociability::Neutral,
        conscience: Conscience::Neutral,
        self_regard: SelfRegard::Neutral,
        conviction: Conviction::Neutral,
        hygiene: Hygiene::Neutral,
        temperance: Temperance::Neutral,
    };
    for personality_trait in &spec.personality.traits {
        match personality_trait {
            StartingPersonalityTrait::Brave => personality.nerve = Nerve::Brave,
            StartingPersonalityTrait::Fearful => personality.nerve = Nerve::Fearful,
            StartingPersonalityTrait::Ambitious => personality.drive = Drive::Ambitious,
            StartingPersonalityTrait::Content => personality.drive = Drive::Content,
            StartingPersonalityTrait::Sanguine => personality.outlook = Outlook::Sanguine,
            StartingPersonalityTrait::Brooding => personality.outlook = Outlook::Brooding,
            StartingPersonalityTrait::Gregarious => {
                personality.sociability = Sociability::Gregarious
            }
            StartingPersonalityTrait::Solitary => personality.sociability = Sociability::Solitary,
            StartingPersonalityTrait::Compassionate => {
                personality.conscience = Conscience::Compassionate
            }
            StartingPersonalityTrait::Callous => personality.conscience = Conscience::Callous,
            StartingPersonalityTrait::Cruel => personality.conscience = Conscience::Cruel,
            StartingPersonalityTrait::Proud => personality.self_regard = SelfRegard::Proud,
            StartingPersonalityTrait::Humble => personality.self_regard = SelfRegard::Humble,
            StartingPersonalityTrait::Zealous => personality.conviction = Conviction::Zealous,
            StartingPersonalityTrait::Irreverent => personality.conviction = Conviction::Irreverent,
            StartingPersonalityTrait::Slovenly => personality.hygiene = Hygiene::Slovenly,
            StartingPersonalityTrait::Cleanly => personality.hygiene = Hygiene::Cleanly,
            StartingPersonalityTrait::Temperate => personality.temperance = Temperance::Temperate,
            StartingPersonalityTrait::Drunkard => personality.temperance = Temperance::Drunkard,
        }
    }
    personality
}

#[cfg(test)]
mod creation_tests {
    use super::{PROTOTYPE_NOTICE, character_candidates_page};
    use adventuresim_core::starting_character::roster;

    #[test]
    fn initial_roster_has_preview_but_no_dialog_or_customization() {
        let candidates = roster(1, "00112233445566778899aabbccddeeff").unwrap();
        let markup =
            character_candidates_page(1, "00112233445566778899aabbccddeeff", &candidates, None)
                .into_string();
        assert_eq!(markup.matches("class=\"party-portrait\"").count(), 5);
        assert!(markup.contains(PROTOTYPE_NOTICE));
        assert!(!markup.contains("role=\"dialog\""));
        assert!(markup.contains("class=\"party-portrait-overlay\""));
        assert!(markup.contains("class=\"party-attributes-list\""));
        assert!(markup.contains("class=\"party-skills-table\""));
        assert!(!markup.contains("class=\"party-portrait-actions\""));
        assert!(!markup.contains("class=\"schedule-section-heading\""));
        assert!(!markup.contains("data-skill-schedule"));
        assert!(!markup.contains("data-candidate-confirm-form"));
        assert!(!markup.contains("name=\"name\""));
    }

    #[test]
    fn explicit_selection_shows_an_inline_play_action() {
        let candidates = roster(1, "00112233445566778899aabbccddeeff").unwrap();
        let markup =
            character_candidates_page(1, "00112233445566778899aabbccddeeff", &candidates, Some(2))
                .into_string();
        assert!(!markup.contains("role=\"dialog\""));
        assert!(!markup.contains("aria-modal=\"true\""));
        assert!(!markup.contains("Keep looking"));
        assert!(markup.contains("class=\"candidate-play-action\""));
        assert!(markup.contains("Play as "));
        assert!(markup.contains("name=\"slot\" value=\"2\""));
    }
}
