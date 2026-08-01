//! Character selection and creation templates.

use maud::{Markup, html};

use super::settlement::{
    CharacterPortraitView, CharacterSheetActions, CharacterSheetView, CharacterSkillHours,
    character_portrait_overlay, character_sheet_markup,
};
use super::{entry_layout, item_display_name, item_type_icon, panel, sidebar_section};
use crate::medical::MedicalPresentation;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterPersonality,
    CharacterSkills, Conscience, Conviction, Courtship, Drive, Hygiene, Inclination, Mirth, Nerve,
    OrganizationMembership, OrganizationPresentation, Outlook, Presentation, SelfKnowledge,
    SelfRegard, Sex, Sociability, Temperance, Transparency,
};
use adventuresim_core::starting_character::{
    StartingAgeTier, StartingCharacterSpec, StartingInclination, StartingPersonalityTrait,
    StartingPresentation, StartingSex, StartingSlot,
};
use adventuresim_core::{
    equipment::weapon_skill_distribution_for_item,
    skill::{PlayerSkills, Skill},
    strategic_schedule::{CombatTrainingProfile, EquippedCombatItem},
};

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
                    p { "No adventurers have been created in this browser yet." }
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
            a href="/characters/candidates" class="btn btn-primary candidate-play-action" {
                "Create another adventurer"
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

pub fn character_switcher_options(
    characters: &[Character],
    current_character_id: Option<u64>,
) -> Markup {
    html! {
        div class="character-switcher-options" {
            @if characters.is_empty() {
                p class="character-switcher-empty" { "No remembered adventurers." }
            } @else {
                @for character in characters {
                    @let current = current_character_id == Some(character.id);
                    form action=(format!("/characters/{}/select", character.id))
                        method="post" data-hard-navigation {
                        button type="submit"
                            class=(if current { "character-switcher-option is-current" } else { "character-switcher-option" })
                            aria-current=(if current { "true" } else { "false" }) {
                            span class="character-switcher-option-portrait" aria-hidden="true" {
                                (character.name.chars().next().unwrap_or('?'))
                            }
                            span class="character-switcher-option-copy" {
                                strong { (&character.name) }
                                small {
                                    @if current { "Currently playing" }
                                    @else if character.alive { "Play this character" }
                                    @else { "View this character" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{character_switcher_options, characters_list_page};
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

    #[test]
    fn switcher_lists_remembered_characters_before_creation_link() {
        let character = |id, name: &str| Character {
            id,
            name: name.into(),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: Some("riverdale".into()),
            current_case_site_id: None,
            party_id: Some(format!("solo-{id}")),
            age_years: 22,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let markup =
            character_switcher_options(&[character(7, "Ada"), character(9, "Beatrix")], Some(9))
                .into_string();
        assert!(markup.contains("Ada"));
        assert!(markup.contains("Beatrix"));
        assert!(markup.contains("action=\"/characters/7/select\""));
        assert!(markup.contains("data-hard-navigation"));
        assert_eq!(markup.matches("aria-current=\"true\"").count(), 1);
        assert!(markup.contains("Currently playing"));
    }
}

const PROTOTYPE_NOTICE: &str = "Early prototype: All text and images are placeholders. Features and saved progress may change or be reset during development.";

pub fn character_candidates_bootstrap_page(version: u16) -> Markup {
    let content = html! {
        main class="center-content candidate-bootstrap" {
            p class="prototype-disclaimer" role="note" { (PROTOTYPE_NOTICE) }
            h2 class="page-title" { "Choose a stage of life" }
            p class="small-copy text-muted" { "Choose how established your adventurer is before meeting the candidates." }
            noscript { p role="alert" { "JavaScript is required to prepare a private candidate roster." } }
            div data-candidate-bootstrap data-generator-version=(version) {}
            nav class="candidate-age-options" aria-label="Starting age" {
                a class="candidate-age-option" data-candidate-age="young" href="#" {
                    strong { "Young" } span { "Age 16 - No profession" }
                }
                a class="candidate-age-option" data-candidate-age="adult" href="#" {
                    strong { "Adult" } span { "Age 22 - Newly qualified" }
                }
                a class="candidate-age-option" data-candidate-age="old" href="#" {
                    strong { "Old" } span { "Age 40 - Master" }
                }
            }
            script src="/static/character-candidates.js?v=3" defer {}
        }
    };
    entry_layout("Choose Your Adventurer", content)
}

pub fn character_candidates_page(
    version: u16,
    seed: &str,
    age_tier: StartingAgeTier,
    candidates: &[StartingCharacterSpec],
    selected: Option<u8>,
    show_inventory: bool,
) -> Markup {
    let presentations = candidates
        .iter()
        .map(CandidatePresentation::from)
        .collect::<Vec<_>>();
    let selected_slot = selected.unwrap_or(0) as usize;
    let candidate = &presentations[selected_slot];
    let spec = &candidates[selected_slot];
    let portraits = presentations
        .iter()
        .enumerate()
        .map(|(slot, candidate)| {
            let profile_href = format!(
                "/characters/candidates?version={version}&seed={seed}&age={}&selected={slot}",
                age_tier.as_str()
            );
            let inventory_href = format!("{profile_href}&view=inventory");
            CharacterPortraitView {
                id: candidate.character.id,
                name: &candidate.character.name,
                alive: true,
                active: false,
                selected: selected == Some(slot as u8),
                href: profile_href,
                title: format!("Inspect {}", candidate.character.name),
                aria_label: format!("Inspect {}", candidate.character.name),
                decoration: None,
                badge: None,
                actions: Some(html! {
                    span class="party-portrait-actions" aria-label=(format!("Actions for {}", candidate.character.name)) {
                        a href=(inventory_href)
                            class="party-portrait-action candidate-inventory-action"
                            title=(format!("View {}'s inventory", candidate.character.name))
                            aria-label=(format!("View {}'s inventory", candidate.character.name)) {
                            span class="party-action-icon"
                                style="--party-action-icon: url('/static/icons/game/knapsack.svg')"
                                role="img" aria-label="Inventory" {}
                        }
                    }
                }),
            }
        })
        .collect::<Vec<_>>();
    let medical = MedicalPresentation::default();
    let attributes_title = format!("{}'s attributes", candidate.character.name);
    let skills_title = format!("{}'s skills", candidate.character.name);
    let portraits = character_portrait_overlay("Candidate adventurers", None, &portraits);
    let center_before = html! {
            p class="prototype-disclaimer" role="note" { (PROTOTYPE_NOTICE) }
            span data-candidate-roster data-age-tier=(age_tier.as_str()) hidden {}
    };
    let center_after = html! {
            @if show_inventory {
                (candidate_inventory_view(spec))
            } @else {
                div class="candidate-background-summary" {
                    p { strong { "Background: " } (&spec.background) }
                }
            }
            @if let Some(selected) = selected {
                form action="/characters/candidates" method="post" class="candidate-play-action" data-candidate-confirm-form {
                    input type="hidden" name="version" value=(version);
                    input type="hidden" name="seed" value=(seed);
                    input type="hidden" name="age" value=(age_tier.as_str());
                    input type="hidden" name="slot" value=(selected);
                    button type="submit" class="btn btn-primary" {
                        "Play as " (&candidate.character.name)
                    }
                }
            }
            script src="/static/character-candidates.js?v=3" defer {}
    };
    let content = character_sheet_markup(CharacterSheetView {
        character: &candidate.character,
        capability: Some(&candidate.capability),
        attributes: Some(&candidate.attributes),
        skills: Some(&candidate.skills),
        limbs: Some(&candidate.limbs),
        personality: Some(&candidate.personality),
        medical: &medical,
        combat_profile: candidate.combat_profile,
        religion_id: candidate.religion_id.as_deref(),
        training_religion_id: None,
        fame: 0.0,
        infamy: 0.0,
        attributes_title: &attributes_title,
        skills_title: &skills_title,
        description: "Adventurer profile",
        can_renounce: false,
        organization_memberships: &candidate.organization_memberships,
        organization_presentation: candidate.organization_presentation.as_ref(),
        organization_minute: 0,
        physiology_dialog_id: None,
        surgery: None,
        injuries: &[],
        projectiles: &[],
        schedule: None,
        schedule_action: None,
        activity_preview: None,
        activity_location: None,
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

fn candidate_inventory_view(spec: &StartingCharacterSpec) -> Markup {
    html! {
        section class="candidate-inventory-view" data-candidate-inventory {
            header class="candidate-inventory-header" {
                h2 { (item_type_icon("coin")) " Starting inventory" }
                span class="candidate-inventory-purse" {
                    (item_type_icon("coin")) (spec.currency) " coins"
                }
            }
            div class="candidate-inventory-grid" {
                @for item in &spec.inventory {
                    article class="candidate-inventory-item" {
                        (item_type_icon(&item.item_id))
                        span class="candidate-inventory-copy" {
                            strong { (item_display_name(&item.item_id)) }
                            span {
                                "Quantity " (item.quantity)
                                @if let Some(slot) = &item.equipped {
                                    " · Equipped: " (starting_slot_label(*slot))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn starting_slot_label(slot: StartingSlot) -> &'static str {
    match slot {
        StartingSlot::LeftHand => "left hand",
        StartingSlot::RightHand => "right hand",
        StartingSlot::LeftArm => "left arm",
        StartingSlot::RightArm => "right arm",
        StartingSlot::LeftLeg => "left leg",
        StartingSlot::RightLeg => "right leg",
        StartingSlot::Head => "head",
        StartingSlot::Chest => "chest",
        StartingSlot::Stomach => "stomach",
    }
}

struct CandidatePresentation {
    character: Character,
    attributes: CharacterAttributes,
    capability: CharacterCapability,
    limbs: CharacterLimbs,
    personality: CharacterPersonality,
    skills: CharacterSkills,
    religion_id: Option<String>,
    organization_memberships: Vec<OrganizationMembership>,
    organization_presentation: Option<OrganizationPresentation>,
    combat_profile: CombatTrainingProfile,
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
            firearm_hours: spec.skills.firearm,
            throw_hours: spec.skills.throw,
            will_hours: spec.skills.will,
            insight_hours: spec.skills.insight,
            charm_hours: spec.skills.charm,
            command_hours: spec.skills.command,
            deception_hours: spec.skills.deception,
            physiology_hours: spec.skills.physiology,
            cooking_hours: spec.skills.cooking,
            herbalism_hours: spec.skills.herbalism,
            religion_hours: spec.skills.religion,
            oral_languages: Default::default(),
            written_languages: Default::default(),
            stealth_hours: spec.skills.stealth,
            balance_hours: spec.skills.balance,
            terrain_plains_hours: spec.skills.terrain_plains,
            terrain_forest_hours: spec.skills.terrain_forest,
            terrain_hills_hours: spec.skills.terrain_hills,
            terrain_wetlands_hours: spec.skills.terrain_wetlands,
            terrain_urban_hours: spec.skills.terrain_urban,
            terrain_snow_hours: spec.skills.terrain_snow,
            bestiary_hours: spec.skills.bestiary,
            surgery_hours: spec.skills.surgery,
            tailoring_hours: spec.skills.tailoring,
            smithing_hours: spec.skills.smithing,
        };
        let effective_skill_hours = CharacterSkillHours(&skills);
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
        let equipped_item_ids = spec
            .inventory
            .iter()
            .filter(|item| item.equipped.is_some())
            .map(|item| item.item_id.as_str())
            .collect::<Vec<_>>();
        let combat_profile = CombatTrainingProfile::from_equipped_hands(
            spec.inventory
                .iter()
                .filter(|item| {
                    matches!(
                        item.equipped,
                        Some(StartingSlot::LeftHand | StartingSlot::RightHand)
                    )
                })
                .map(|item| {
                    let shield = matches!(
                        item.item_id.as_str(),
                        "buckler" | "targe" | "heater_shield" | "round_shield" | "pavise"
                    );
                    EquippedCombatItem {
                        weapons: if shield {
                            Default::default()
                        } else {
                            weapon_skill_distribution_for_item(&item.item_id)
                        },
                        shield,
                        balance: 1.0,
                    }
                }),
        );
        let has = |choices: &[&str]| equipped_item_ids.iter().any(|item| choices.contains(item));
        let ranged = has(&[
            "self_bow",
            "longbow",
            "light_crossbow",
            "heavy_crossbow",
            "matchlock_arquebus",
            "hooked_arquebus",
        ]);
        let melee = has(&[
            "arming_sword",
            "baselard",
            "bauernwehr",
            "club",
            "flanged_mace",
            "halberd",
            "hand_axe",
            "hunting_spear",
            "katzbalger",
            "kriegsmesser",
            "longsword",
            "messer",
            "military_pike",
            "misericorde",
            "rapier",
            "rondel_dagger",
            "utility_knife",
            "walking_staff",
            "war_hammer",
            "zweihander",
        ]);
        let blunt = has(&["club", "flanged_mace", "walking_staff", "war_hammer"]);
        let slash = has(&[
            "arming_sword",
            "baselard",
            "bauernwehr",
            "hand_axe",
            "katzbalger",
            "kriegsmesser",
            "longsword",
            "messer",
            "utility_knife",
            "zweihander",
        ]);
        let pierce = ranged
            || has(&[
                "halberd",
                "hunting_spear",
                "military_pike",
                "misericorde",
                "rapier",
                "rondel_dagger",
            ]);
        let mut weapon_precision: f32 = 0.0;
        for (present, skill, hours) in [
            (
                has(&["halberd", "hunting_spear", "military_pike"]),
                Skill::Polearm,
                spec.skills.polearm,
            ),
            (has(&["hand_axe"]), Skill::Axe, spec.skills.axe),
            (
                has(&["club", "flanged_mace", "walking_staff", "war_hammer"]),
                Skill::Bludgeon,
                spec.skills.bludgeon,
            ),
            (
                has(&[
                    "arming_sword",
                    "katzbalger",
                    "kriegsmesser",
                    "longsword",
                    "messer",
                    "rapier",
                    "zweihander",
                ]),
                Skill::Sword,
                spec.skills.sword,
            ),
            (
                has(&[
                    "baselard",
                    "bauernwehr",
                    "misericorde",
                    "rondel_dagger",
                    "utility_knife",
                ]),
                Skill::Knife,
                spec.skills.knife,
            ),
            (has(&["self_bow", "longbow"]), Skill::Bow, spec.skills.bow),
            (
                has(&["light_crossbow", "heavy_crossbow"]),
                Skill::Crossbow,
                spec.skills.crossbow,
            ),
            (
                has(&["matchlock_arquebus", "hooked_arquebus"]),
                Skill::Firearm,
                spec.skills.firearm,
            ),
        ] {
            if present {
                weapon_precision = weapon_precision
                    .max(skill.capped_rank_for_aptitude(hours, spec.attributes.agility));
            }
        }
        let capability = CharacterCapability {
            character_id: spec.id,
            melee,
            ranged,
            precise: has(&["rapier", "self_bow", "longbow", "light_crossbow"]),
            heavy: has(&[
                "heavy_crossbow",
                "hooked_arquebus",
                "military_pike",
                "war_hammer",
                "zweihander",
            ]),
            quarter_armor: armor_slots >= 2,
            half_armor: armor_slots >= 4,
            three_quarter_armor: armor_slots >= 6,
            full_armor: armor_slots >= 7,
            blunt,
            slash,
            pierce,
            athletics: Skill::Dodge
                .capped_rank_for_aptitude(spec.skills.dodge, spec.attributes.agility)
                .max(
                    Skill::Balance
                        .capped_rank_for_aptitude(spec.skills.balance, spec.attributes.agility),
                ),
            endurance: spec.attributes.endurance,
            physiology: Skill::Physiology
                .capped_rank_for_aptitude(spec.skills.physiology, spec.attributes.intelligence),
            knife: Skill::Knife
                .capped_rank_for_aptitude(spec.skills.knife, spec.attributes.agility),
            tailoring: Skill::Tailoring
                .capped_rank_for_aptitude(spec.skills.tailoring, spec.attributes.agility),
            surgery: Skill::Surgery.capped_rank_for_aptitude(
                effective_skill_hours.effective_skill_hours(Skill::Surgery),
                spec.attributes.intelligence,
            ),
            command: Skill::Command
                .capped_rank_for_aptitude(spec.skills.command, spec.attributes.instinct),
            religion: Skill::Religion.capped_rank_for_aptitude(
                spec.skills.religion.maximum_effective(),
                spec.attributes.intelligence,
            ),
            weapon_precision,
            autoresolve_combat_power: 0,
        };
        let organization_memberships = spec
            .organization
            .iter()
            .map(|organization| {
                let paid_through =
                    adventuresim_core::organization::organization(&organization.organization_id)
                        .and_then(|definition| definition.dues.as_ref())
                        .map_or(u64::MAX, |dues| {
                            u64::from(dues.interval_days)
                                * adventuresim_core::strategic_time::MINUTES_PER_DAY
                        });
                OrganizationMembership {
                    id: 0,
                    character_id: spec.id,
                    organization_id: organization.organization_id.clone(),
                    rank_id: organization.rank_id.clone(),
                    joined_minute: 0,
                    dues_paid_through_minute: paid_through,
                    status: "active".into(),
                    apprenticeship_minutes_accrued: 0,
                    practice_minutes_accrued: 0,
                }
            })
            .collect();
        let organization_presentation =
            spec.organization
                .as_ref()
                .map(|organization| OrganizationPresentation {
                    character_id: spec.id,
                    organization_id: organization.organization_id.clone(),
                });
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
            religion_id: spec.religion_id.clone(),
            organization_memberships,
            organization_presentation,
            combat_profile,
        }
    }
}

fn candidate_personality(spec: &StartingCharacterSpec) -> CharacterPersonality {
    let mut personality = CharacterPersonality {
        character_id: spec.id,
        projection_character_id: spec.id,
        nerve: Nerve::Neutral,
        drive: Drive::Neutral,
        outlook: Outlook::Neutral,
        sociability: Sociability::Neutral,
        conscience: Conscience::Neutral,
        self_regard: SelfRegard::Neutral,
        conviction: Conviction::Neutral,
        hygiene: Hygiene::Neutral,
        temperance: Temperance::Neutral,
        mirth: Mirth::Neutral,
        courtship: Courtship::Neutral,
        transparency: Transparency::Neutral,
        self_knowledge: SelfKnowledge::Neutral,
        sex: match spec.personality.sex {
            StartingSex::Female => Sex::Female,
            StartingSex::Male => Sex::Male,
        },
        presentation: match spec.personality.presentation {
            StartingPresentation::Man => Presentation::Man,
            StartingPresentation::Ambiguous => Presentation::Ambiguous,
            StartingPresentation::Woman => Presentation::Woman,
        },
        inclination: match spec.personality.inclination {
            StartingInclination::Men => Inclination::Men,
            StartingInclination::Either => Inclination::Either,
            StartingInclination::Women => Inclination::Women,
            StartingInclination::Neither => Inclination::Neither,
        },
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
            StartingPersonalityTrait::Merry => personality.mirth = Mirth::Merry,
            StartingPersonalityTrait::Grave => personality.mirth = Mirth::Grave,
            StartingPersonalityTrait::Amorous => personality.courtship = Courtship::Amorous,
            StartingPersonalityTrait::Proper => personality.courtship = Courtship::Proper,
            StartingPersonalityTrait::Open => personality.transparency = Transparency::Open,
            StartingPersonalityTrait::Guarded => personality.transparency = Transparency::Guarded,
            StartingPersonalityTrait::Introspective => {
                personality.self_knowledge = SelfKnowledge::Introspective
            }
            StartingPersonalityTrait::SelfDeceiving => {
                personality.self_knowledge = SelfKnowledge::SelfDeceiving
            }
        }
    }
    personality
}

#[cfg(test)]
mod creation_tests {
    use super::{CandidatePresentation, PROTOTYPE_NOTICE, character_candidates_page};
    use adventuresim_core::organization::StartingProfession;
    use adventuresim_core::starting_character::{StartingAgeTier, StartingItem, roster};

    #[test]
    fn initial_roster_has_preview_but_no_dialog_or_customization() {
        let candidates = roster(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Young,
        )
        .unwrap();
        let markup = character_candidates_page(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Young,
            &candidates,
            None,
            false,
        )
        .into_string();
        assert_eq!(markup.matches("class=\"party-portrait\"").count(), 5);
        assert!(markup.contains(PROTOTYPE_NOTICE));
        assert!(!markup.contains("role=\"dialog\""));
        assert!(markup.contains("class=\"party-portrait-overlay\""));
        assert!(markup.contains("class=\"party-attributes-list\""));
        assert!(markup.contains("class=\"party-skills-table\""));
        assert!(markup.contains("class=\"party-portrait-actions\""));
        assert!(markup.contains("candidate-inventory-action"));
        assert!(!markup.contains("class=\"schedule-section-heading\""));
        assert!(!markup.contains("data-skill-schedule"));
        assert!(!markup.contains("data-candidate-confirm-form"));
        assert!(!markup.contains("name=\"name\""));
    }

    #[test]
    fn explicit_selection_shows_an_inline_play_action() {
        let candidates = roster(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Adult,
        )
        .unwrap();
        let markup = character_candidates_page(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Adult,
            &candidates,
            Some(2),
            false,
        )
        .into_string();
        assert!(!markup.contains("role=\"dialog\""));
        assert!(!markup.contains("aria-modal=\"true\""));
        assert!(!markup.contains("Keep looking"));
        assert!(markup.contains("class=\"candidate-play-action\""));
        assert!(markup.contains("Play as "));
        assert!(markup.contains("name=\"slot\" value=\"2\""));
        assert_eq!(markup.matches("data-character-alive=\"true\"").count(), 10);
        assert!(markup.contains("name=\"age\" value=\"adult\""));
        assert!(!markup.contains("data-candidate-package"));
        assert!(markup.contains("organization-identity-control is-readonly"));
        assert!(markup.contains("religion-identity-control"));
        assert!(markup.contains("candidate-inventory-action"));
    }

    #[test]
    fn candidate_inventory_opens_from_the_portrait_without_listing_the_package_on_profile() {
        let candidates = roster(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Adult,
        )
        .unwrap();
        let markup = character_candidates_page(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Adult,
            &candidates,
            Some(2),
            true,
        )
        .into_string();
        assert!(markup.contains("data-candidate-inventory"));
        assert!(markup.contains("Starting inventory"));
        assert!(markup.contains("candidate-inventory-item"));
        assert!(markup.contains("view=inventory"));
        assert!(!markup.contains("Package:"));
        assert!(markup.contains("party-attributes-list"));
        assert!(markup.contains("party-skills-table"));
    }

    #[test]
    fn preview_capabilities_use_equipped_items_and_professional_skill_values() {
        let mut young = roster(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Young,
        )
        .unwrap()
        .remove(0);
        young.inventory.push(StartingItem {
            item_id: "longbow".into(),
            quantity: 1,
            equipped: None,
        });
        let preview = CandidatePresentation::from(&young);
        assert!(!preview.capability.ranged);

        let mut adult = roster(
            adventuresim_core::starting_character::GENERATOR_VERSION,
            "00112233445566778899aabbccddeeff",
            StartingAgeTier::Adult,
        )
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.profession == Some(StartingProfession::Herbalist))
        .unwrap();
        adult.skills.physiology = 100.0;
        let preview = CandidatePresentation::from(&adult);
        assert!(preview.capability.physiology > 0.0);
        assert_eq!(preview.capability.surgery, 0.0);
        assert!(preview.capability.weapon_precision > 0.0);

        adult.skills.knife = 10_000.0;
        adult.skills.tailoring = 10_000.0;
        assert_eq!(
            CandidatePresentation::from(&adult).capability.surgery,
            0.0,
            "correlated crafts must not unlock a trained skill without direct Surgery study"
        );

        adult.skills.surgery = 100.0;
        let correlated = CandidatePresentation::from(&adult).capability.surgery;
        adult.skills.knife = 0.0;
        adult.skills.tailoring = 0.0;
        let direct_only = CandidatePresentation::from(&adult).capability.surgery;
        assert!(correlated > direct_only);
    }

    #[test]
    fn preview_membership_dues_use_the_same_initial_interval_semantics() {
        let candidates = [StartingAgeTier::Adult, StartingAgeTier::Old]
            .into_iter()
            .flat_map(|tier| {
                roster(
                    adventuresim_core::starting_character::GENERATOR_VERSION,
                    "00112233445566778899aabbccddeeff",
                    tier,
                )
                .unwrap()
            });
        let candidate = candidates
            .into_iter()
            .find(|candidate| {
                candidate.organization.as_ref().is_some_and(|organization| {
                    adventuresim_core::organization::organization(&organization.organization_id)
                        .is_some_and(|definition| definition.dues.is_some())
                })
            })
            .unwrap();
        let preview = CandidatePresentation::from(&candidate);
        let membership = &preview.organization_memberships[0];
        let definition =
            adventuresim_core::organization::organization(&membership.organization_id).unwrap();
        let expected = u64::from(definition.dues.as_ref().unwrap().interval_days)
            * adventuresim_core::strategic_time::MINUTES_PER_DAY;
        assert_eq!(membership.joined_minute, 0);
        assert_eq!(membership.dues_paid_through_minute, expected);
    }
}
