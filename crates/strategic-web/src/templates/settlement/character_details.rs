use adventuresim_core::{
    organization::{OrganizationDefinition, OrganizationRank, organization},
    strategic_schedule::CombatTrainingProfile,
};
use adventuresim_world_schema::OfficialReligion;
use maud::{Markup, html};

use super::{
    character_health::{
        party_attributes_rail, physiology_controls, physiology_dialog, strategic_condition_rail,
    },
    character_skills::{
        ActivityPreviewRates, CharacterSheetActions, SummaryIconKind, character_summary_icons,
        party_skills_rail, skill_rank_tier,
    },
    chrome::{party_portrait_overlay, visual_stage},
    context::LocationView,
    social::{player_chat_area, settlement_chat_area},
    trade::{cooking_activity_dialog, religious_demand_rail},
};
use crate::medical::MedicalPresentation;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterSkills,
    CharacterStrategicCondition, CharacterTrainingSchedule, FoodLot, InventoryItem,
    InventoryItemAmount, ItemDefinition, LimbInjury, OrganizationMembership,
    OrganizationPresentation, Party, RetainedProjectile,
};
use crate::templates::{decorative_game_icon, religion_icon, sidebar_section};

fn character_summary_rail(
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    combat_profile: CombatTrainingProfile,
    religion_context: Option<OfficialReligion>,
) -> Markup {
    let icons = character_summary_icons(
        capability,
        attributes,
        skills,
        combat_profile,
        religion_context,
    );
    html! {
        (sidebar_section("Summary", html! {
            @if icons.is_empty() {
                p class="text-muted small-copy" { "No notable capabilities." }
            } @else {
                div class="character-summary-icons" role="list"
                    aria-label="Character capability summary" {
                    @for icon in icons {
                        span class=(format!(
                                "character-summary-icon skill-rank-tier-{}",
                                skill_rank_tier(icon.rank)
                            ))
                            role="listitem" tabindex="0" aria-label=(&icon.label)
                            data-strategic-tooltip=(&icon.tooltip) {
                            @match icon.kind {
                                SummaryIconKind::Mask(path) => {
                                    span class="character-summary-icon-mask"
                                        style=(format!("--summary-icon: url('{path}')"))
                                        aria-hidden="true" {}
                                }
                                SummaryIconKind::Monogram { text, germanic_style, written } => {
                                    span class=(format!(
                                            "character-summary-monogram language-{}{}",
                                            if written { "written" } else { "oral" },
                                            if germanic_style { " language-blackletter" } else { "" },
                                        ))
                                        aria-hidden="true" { (text) }
                                }
                            }
                        }
                    }
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
    combat_profile: CombatTrainingProfile,
    religion_context: Option<OfficialReligion>,
) -> Markup {
    html! {
        (character_summary_rail(
            capability, attributes, skills, combat_profile, religion_context,
        ))
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
            false, 0.0, None, combat_profile, CharacterSheetActions::default(),
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
    pub fame: f32,
    pub infamy: f32,
    pub attributes_title: &'a str,
    pub skills_title: &'a str,
    pub description: &'a str,
    pub can_renounce: bool,
    pub organization_memberships: &'a [OrganizationMembership],
    pub organization_presentation: Option<&'a OrganizationPresentation>,
    pub organization_minute: u64,
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
            (character_summary_rail(
                view.capability,
                view.attributes,
                view.skills,
                view.combat_profile,
                view.training_religion_id.and_then(OfficialReligion::from_id),
            ))
            (character_bio_rail(
                view.character,
                view.religion_id,
                view.fame,
                view.infamy,
                view.personality,
                view.can_renounce,
                view.location_path,
                view.organization_memberships,
                view.organization_presentation,
                view.organization_minute,
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
    organization_memberships: &[OrganizationMembership],
    organization_presentation: Option<&OrganizationPresentation>,
    organization_minute: u64,
    prayer_religion_check: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    combat_profile: CombatTrainingProfile,
    activity_preview: ActivityPreviewRates,
    religious_demand: Option<&crate::spacetimedb::ReligiousDemand>,
    fame: f32,
    infamy: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    _can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    cooking: bool,
    inventory: &[InventoryItem],
    inventory_amounts: &[InventoryItemAmount],
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
            (cooking_activity_dialog(
                location,
                active_character,
                inventory,
                inventory_amounts,
                food_lots,
                item_definitions,
            ))
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
        fame,
        infamy,
        attributes_title: "Your attributes",
        skills_title: "Your skills",
        description: "Your identity, condition, and capabilities",
        can_renounce: true,
        organization_memberships,
        organization_presentation,
        organization_minute,
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
    fame: f32,
    infamy: f32,
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
        fame,
        infamy,
        attributes_title: &selected_attributes_title,
        skills_title: &selected_skills_title,
        description: "Party member identity and capabilities",
        can_renounce: selected.id == active_character.id,
        organization_memberships: &[],
        organization_presentation: None,
        organization_minute: 0,
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
    fame: f32,
    infamy: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    can_renounce: bool,
    location_path: &str,
    organization_memberships: &[OrganizationMembership],
    organization_presentation: Option<&OrganizationPresentation>,
    organization_minute: u64,
) -> Markup {
    html! {
        (sidebar_section("Bio", html! {
            dl class="character-bio" {
                div { dt class="metric-label" { (decorative_game_icon("calendar")) span { "Age" } } dd { (character.age_years) " years" } }
                div { dt class="metric-label" { (decorative_game_icon("spiked-halo")) span { "Fame" } } dd title="Favorable stories known in this settlement." { (format!("{fame:.1}")) } }
                div { dt class="metric-label" { (decorative_game_icon("scales")) span { "Infamy" } } dd title="Crimes and scandals known in this settlement." { (format!("{infamy:.1}")) } }
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
                div class="character-identity-controls" {
                    @if can_renounce {
                        (religion_identity_button(character, religion_id, location_path))
                        @if let Some(settlement_id) = character.current_settlement_id.as_deref() {
                            (organization_identity_picker(
                                character,
                                settlement_id,
                                location_path,
                                organization_memberships,
                                organization_presentation,
                                organization_minute,
                            ))
                        }
                    } @else {
                        (religion_identity_display(religion_id))
                        (organization_identity_display(
                            organization_memberships,
                            organization_presentation,
                        ))
                    }
                }
            }
        }))
    }
}

fn religion_identity_display(religion_id: Option<&str>) -> Markup {
    let name = religion_name(religion_id);
    let class = if religion_id.is_some() {
        "identity-control religion-identity-control is-readonly"
    } else {
        "identity-control religion-identity-control is-empty is-readonly"
    };
    html! {
        div class=(class) {
            (religion_icon(name, religion_id, true))
            span class="religion-control-copy" {
                span class="religion-control-name" { (name) }
            }
        }
    }
}

fn organization_identity_display(
    memberships: &[OrganizationMembership],
    presentation: Option<&OrganizationPresentation>,
) -> Markup {
    let selected = presentation.and_then(|presentation| {
        let membership = memberships.iter().find(|membership| {
            membership.status == "active"
                && membership.organization_id == presentation.organization_id
        })?;
        let definition = organization(&membership.organization_id)?;
        let rank = definition.rank(&membership.rank_id)?;
        Some((definition, rank))
    });
    let class = if selected.is_some() {
        "identity-control organization-identity-control is-readonly"
    } else {
        "identity-control organization-identity-control is-empty is-readonly"
    };
    html! {
        div class=(class) {
            @if let Some((definition, rank)) = selected {
                (organization_crest(definition))
                (organization_identity_copy(
                    definition.name.as_str(),
                    &profession_name(definition, rank),
                ))
            } @else {
                (empty_organization_crest())
                (organization_identity_copy("No organization", "No Profession"))
            }
        }
    }
}

fn religion_identity_button(
    character: &Character,
    religion_id: Option<&str>,
    location_path: &str,
) -> Markup {
    let name = religion_name(religion_id);
    html! {
        @if religion_id.is_some() {
            form method="post"
                action=(format!("{location_path}/party/{}/religion/renounce", character.id))
                class="identity-control-form" {
                button type="submit" class="identity-control religion-identity-control"
                    title=(format!("Renounce {name}")) {
                    (religion_icon(name, religion_id, true))
                    span class="religion-control-copy" {
                        span class="religion-control-name" { (name) }
                        span class="religion-control-renounce" aria-hidden="true" { "Renounce" }
                    }
                }
            }
        } @else {
            button type="button" class="identity-control religion-identity-control is-empty"
                disabled aria-label="No Religion" {
                (religion_icon("No Religion", None, true))
                span { "No Religion" }
            }
        }
    }
}

fn organization_identity_picker(
    character: &Character,
    settlement_id: &str,
    _location_path: &str,
    memberships: &[OrganizationMembership],
    presentation: Option<&OrganizationPresentation>,
    minute: u64,
) -> Markup {
    let choices = memberships
        .iter()
        .filter_map(|membership| {
            let definition = organization(&membership.organization_id)?;
            let rank = definition.rank(&membership.rank_id)?;
            (membership.status == "active"
                && minute <= membership.dues_paid_through_minute
                && definition.recognition.includes(settlement_id))
            .then_some((membership, definition, rank))
        })
        .collect::<Vec<_>>();
    let selected = presentation.and_then(|presentation| {
        choices
            .iter()
            .copied()
            .find(|(_, definition, _)| definition.id == presentation.organization_id)
    });
    let base = format!(
        "/locations/settlement/{settlement_id}/party/{}",
        character.id
    );
    let summary_class = if selected.is_none() {
        "identity-control organization-identity-control is-empty"
    } else {
        "identity-control organization-identity-control"
    };

    html! {
        details class="organization-identity-picker" {
            summary class=(summary_class) {
                @if let Some((_, definition, rank)) = selected {
                    (organization_crest(definition))
                    (organization_identity_copy(definition.name.as_str(), &profession_name(definition, rank)))
                } @else {
                    (empty_organization_crest())
                    (organization_identity_copy("No organization", "No Profession"))
                }
                span class="organization-picker-arrow" aria-hidden="true" {}
            }
            div class="organization-picker-menu" role="menu" {
                form method="post" action=(format!("{base}/organization-presentation-none")) {
                    button type="submit" class=(if selected.is_none() { "organization-picker-option is-selected" } else { "organization-picker-option" })
                        role="menuitem" {
                        (empty_organization_crest())
                        (organization_identity_copy("No organization", "No Profession"))
                    }
                }
                @for (_, definition, rank) in choices {
                    @let is_selected = selected.is_some_and(|(_, selected_definition, _)| selected_definition.id == definition.id);
                    form method="post" action=(format!("{base}/organization-presentation/{}", definition.id)) {
                        button type="submit" class=(if is_selected { "organization-picker-option is-selected" } else { "organization-picker-option" })
                            role="menuitem" {
                            (organization_crest(definition))
                            (organization_identity_copy(definition.name.as_str(), &profession_name(definition, rank)))
                        }
                    }
                }
            }
        }
    }
}

fn organization_identity_copy(organization_name: &str, profession: &str) -> Markup {
    html! {
        span class="organization-control-copy" {
            span class="organization-control-name" { (organization_name) }
            span class="organization-control-profession" { (profession) }
        }
    }
}

fn profession_name(definition: &OrganizationDefinition, rank: &OrganizationRank) -> String {
    let profession = match definition.service_id.as_deref() {
        Some("merchants") => Some("Merchant"),
        Some("weapons") => Some("Weaponsmith"),
        Some("armor") => Some("Armourer"),
        Some("clothing") => Some("Tailor"),
        Some("herbalist") => Some("Herbalist"),
        Some("inn") => Some("Cook"),
        _ => None,
    };
    profession.map_or_else(
        || rank.name.clone(),
        |profession| match rank.id.as_str() {
            "apprentice" | "journeyman" | "master" => format!("{} {profession}", rank.name),
            _ => profession.to_string(),
        },
    )
}

fn organization_crest(definition: &OrganizationDefinition) -> Markup {
    let (field, accent) = organization_colors(&definition.id);
    let charge = organization_charge(definition);
    html! {
        span class="organization-crest"
            style=(format!(
                "--crest-field: {field}; --crest-accent: {accent}; --crest-charge: url('/static/icons/game/{charge}.svg')"
            ))
            role="img" aria-label=(format!("{} heraldry", definition.name)) {
            span class="organization-crest-charge" aria-hidden="true" {}
        }
    }
}

fn empty_organization_crest() -> Markup {
    html! {
        span class="organization-crest organization-crest-empty" aria-hidden="true" {
            span class="organization-crest-charge" {}
        }
    }
}

fn organization_colors(id: &str) -> (&'static str, &'static str) {
    const PALETTES: &[(&str, &str)] = &[
        ("#7f1d1d", "#f5d77b"),
        ("#173f5f", "#d9edf7"),
        ("#285943", "#f0cf65"),
        ("#4c2a63", "#e6c9ff"),
        ("#7a4b12", "#f6e7c1"),
        ("#1e4d4f", "#f1b24a"),
        ("#5a2333", "#f3d9a5"),
        ("#243b67", "#d8c89b"),
    ];
    let hash = id.bytes().fold(0usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(usize::from(byte))
    });
    PALETTES[hash % PALETTES.len()]
}

fn organization_charge(definition: &OrganizationDefinition) -> &'static str {
    match definition.id.as_str() {
        "order_saint_george" => return "mounted-knight",
        "lodge_hart_king" => return "wood-axe",
        "hunt_pale_lantern" => return "eye-target",
        _ => {}
    }
    match definition.service_id.as_deref() {
        Some("merchants") => "coins",
        Some("weapons") => "anvil",
        Some("armor") => "breastplate",
        Some("clothing") => "clothes",
        Some("herbalist") => "caduceus",
        Some("inn") => "meal",
        _ if definition.id.contains("forester") => "wood-axe",
        _ if definition.id.contains("saint_george")
            || definition.id.contains("royal")
            || definition.id.contains("knight") =>
        {
            "mounted-knight"
        }
        _ if definition.id.contains("witch") || definition.id.contains("watchful") => "eye-target",
        _ if definition.id.contains("religion") || definition.id.contains("theolog") => {
            "gothic-cross"
        }
        _ if definition.id.contains("scholar") || definition.id.contains("college") => "open-book",
        _ => "shield",
    }
}

fn personality_tags(
    personality: &crate::spacetimedb::CharacterPersonality,
) -> Vec<(&'static str, &'static str)> {
    use crate::spacetimedb::{
        Conscience::*, Conviction::*, Courtship::*, Drive::*, Hygiene::*, Inclination::*, Mirth::*,
        Nerve::*, Outlook::*, Presentation::*, SelfKnowledge::*, SelfRegard::*, Sociability::*,
        Temperance::*, Transparency::*,
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
    match personality.mirth {
        Merry => tags.push((
            "Merry",
            "Charm through humor lands best with Merry or Neutral company.",
        )),
        Grave => tags.push((
            "Grave",
            "Cannot use Joke; serious reserve adds +0.35 to Rally Command.",
        )),
        _ => {}
    }
    match personality.courtship {
        Amorous => tags.push(("Amorous", "Responds strongly to compatible flirtation.")),
        Proper => tags.push((
            "Proper",
            "Cannot use Flirt; decorous reserve adds +0.35 to Rally Command.",
        )),
        _ => {}
    }
    match personality.transparency {
        Open => tags.push((
            "Open",
            "Involuntarily reveals temperament; discovery trains Insight.",
        )),
        Guarded => tags.push((
            "Guarded",
            "Involuntarily conceals temperament; discovery trains Deception.",
        )),
        _ => {}
    }
    match personality.self_knowledge {
        Introspective => tags.push(("Introspective", "Usually reads their own motives clearly.")),
        SelfDeceiving => tags.push(("Self-deceiving", "Habitually obscures their own motives.")),
        _ => {}
    }
    tags.push(match personality.presentation {
        Man => ("Man", "Apparently a man; normally apparent on contact."),
        Ambiguous => (
            "Ambiguous",
            "Ambiguous presentation; compatible with attraction to men and women.",
        ),
        Woman => ("Woman", "Apparently a woman; normally apparent on contact."),
    });
    tags.push(match personality.inclination {
        Men => ("Attracted to men", "Romantic interest favors men."),
        Either => (
            "Attracted to men and women",
            "Romantic interest includes men and women.",
        ),
        Women => ("Attracted to women", "Romantic interest favors women."),
        Neither => (
            "Attracted to neither",
            "No gender-directed romantic interest.",
        ),
    });
    tags
}

#[cfg(test)]
mod personality_tests {
    use super::*;
    use crate::spacetimedb::*;

    #[test]
    fn neutral_behavioral_axes_are_omitted_but_demographics_remain_visible_to_owner() {
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
            ..CharacterPersonality::neutral(1)
        };
        let tags = personality_tags(&personality);
        assert_eq!(
            tags.iter().map(|tag| tag.0).collect::<Vec<_>>(),
            ["Brave", "Cruel", "Man", "Attracted to women"]
        );
    }

    #[test]
    fn every_visible_tag_has_an_accessible_explanation() {
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
                ..CharacterPersonality::neutral(1)
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
                ..CharacterPersonality::neutral(2)
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
                ..CharacterPersonality::neutral(3)
            },
        ];

        for profile in &profiles {
            for (tag, description) in personality_tags(profile) {
                assert!(!description.is_empty(), "{tag} tooltip is empty");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary_attributes() -> CharacterAttributes {
        CharacterAttributes {
            character_id: 1,
            endurance: 5.0,
            immunity: 5.0,
            gut: 5.0,
            intelligence: 5.0,
            instinct: 5.0,
            eyesight: 5.0,
            hearing: 5.0,
            left_arm_strength: 5.0,
            right_arm_strength: 5.0,
            left_leg_strength: 5.0,
            right_leg_strength: 5.0,
            left_arm_agility: 5.0,
            right_arm_agility: 5.0,
            left_leg_agility: 5.0,
            right_leg_agility: 5.0,
        }
    }

    #[test]
    fn summary_icons_expose_instant_tooltips_and_keyboard_names() {
        let skills = CharacterSkills {
            sword_hours: 50_000.0,
            ..Default::default()
        };
        let profile = CombatTrainingProfile {
            weapons: adventuresim_core::equipment::WeaponSkillDistribution {
                sword: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let markup = character_summary_rail(
            None,
            Some(&summary_attributes()),
            Some(&skills),
            profile,
            None,
        )
        .into_string();
        assert!(markup.contains("class=\"character-summary-icons\" role=\"list\""));
        assert!(markup.contains("role=\"listitem\" tabindex=\"0\""));
        assert!(markup.contains("aria-label=\"Sword —"));
        assert!(markup.contains("data-strategic-tooltip=\"Sword —"));
        assert!(!markup.contains("character-summary-tag"));
        assert!(!markup.contains(" title="));
    }

    #[test]
    fn summary_without_required_profile_data_keeps_the_empty_state() {
        let markup =
            character_summary_rail(None, None, None, CombatTrainingProfile::default(), None)
                .into_string();
        assert!(markup.contains("No notable capabilities."));
    }

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

    #[test]
    fn every_catalog_organization_has_stable_heraldry() {
        for definition in &adventuresim_core::organization::catalog().organizations {
            let first = organization_crest(definition).into_string();
            let second = organization_crest(definition).into_string();
            assert_eq!(
                first, second,
                "{} crest changed within a render",
                definition.id
            );
            assert!(first.contains("organization-crest"));
            assert!(first.contains("/static/icons/game/"));
            assert!(first.contains(&format!("{} heraldry", definition.name)));
        }
    }

    #[test]
    fn consolidated_adventurer_organizations_have_specific_charges() {
        for (id, charge) in [
            ("order_saint_george", "mounted-knight"),
            ("lodge_hart_king", "wood-axe"),
            ("hunt_pale_lantern", "eye-target"),
        ] {
            let definition = organization(id).unwrap();
            assert_eq!(organization_charge(definition), charge);
            let crest = organization_crest(definition).into_string();
            assert!(crest.contains(&format!("/static/icons/game/{charge}.svg")));
            assert!(!crest.contains("/static/icons/game/shield.svg"));
        }
    }

    #[test]
    fn service_memberships_name_the_profession() {
        let definition = organization("weaponsmith_guild").expect("weapons guild");
        let rank = definition.ranks.first().expect("weapons guild rank");
        assert_eq!(profession_name(definition, rank), "Apprentice Weaponsmith");
    }
}
