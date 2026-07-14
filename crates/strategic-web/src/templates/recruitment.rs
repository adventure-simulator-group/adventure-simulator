use maud::{Markup, html};

use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterSkills, Party,
    PartyJoinRequest, PartyRecruitmentRole, RecruitmentRequirements, SavedRecruitmentRole,
};
use crate::templates::settlement::{character_stats_panel, character_visual_preview};

#[derive(Clone, Copy, Default)]
pub struct PartyCheckSummary {
    pub medicine: f32,
    pub surgery: f32,
    pub charisma: f32,
    pub faith: f32,
}

pub struct RecruitmentApplicant {
    pub request: PartyJoinRequest,
    pub character: Character,
    pub capability: Option<CharacterCapability>,
    pub attributes: Option<CharacterAttributes>,
    pub skills: Option<CharacterSkills>,
    pub limbs: Option<CharacterLimbs>,
    pub contribution: PartyCheckSummary,
}

pub struct RecruitmentRolePanel {
    pub role: PartyRecruitmentRole,
    pub filled: Vec<Character>,
    pub requests: Vec<RecruitmentApplicant>,
}

pub fn recruitment_panel(
    party: &Party,
    active_character_id: u64,
    roles: &[RecruitmentRolePanel],
    saved_roles: &[SavedRecruitmentRole],
    checks: PartyCheckSummary,
) -> Markup {
    let can_manage = party.leader_id == active_character_id;
    html! {
        div data-party-recruitment-panel
            data-leader-id=(party.leader_id)
            data-can-manage=(can_manage)
        {
            div class="party-aggregate-checks" data-party-aggregate-checks {
                (aggregate_check_control(party, "Medicine", "medicine", "medicine", checks.medicine, party.medicine_target, can_manage))
                (aggregate_check_control(party, "Surgery", "surgeon", "surgery", checks.surgery, party.surgery_target, can_manage))
                (aggregate_check_control(party, "Charisma", "charisma", "charisma", checks.charisma, party.charisma_target, can_manage))
                (aggregate_check_control(party, "Faith", "faith", "faith", checks.faith, party.faith_target, can_manage))
            }
            div data-party-slot-groups hidden {
                @for panel in roles {
                    @let remaining = (panel.role.quantity as usize).saturating_sub(panel.filled.len());
                    @if remaining > 0 {
                        div class="party-role-group" data-party-role-group data-role-id=(panel.role.id) {
                            div class="party-role-portraits" {
                                @for slot in 0..remaining {
                                    button type="button" class="party-portrait party-role-slot"
                                        data-select-party-role aria-pressed="false"
                                        aria-label=(format!("{} slot {}", panel.role.name, slot + 1))
                                        title=(format!("Inspect open {} slot", panel.role.name))
                                        style=(format!("z-index: {}", remaining - slot)) {
                                        span class="party-slot-plus" { "+" }
                                        @if slot == 0 { span class="party-portrait-name" { (&panel.role.name) } }
                                    }
                                }
                                @if can_manage {
                                    span class="party-role-notification-badge"
                                        data-party-role-notification-badge data-role-id=(panel.role.id)
                                        hidden[panel.requests.is_empty()] { (panel.requests.len()) }
                                }
                            }
                            div class="party-role-hover-card" {
                                strong { (&panel.role.name) }
                                span class="party-role-hover-tags" { (requirements_label(panel.role.requirements, panel.role.effective_weapon_precision())) }
                                @if can_manage {
                                    @if panel.requests.is_empty() {
                                        span class="small-copy text-muted" { "No join requests" }
                                    } @else {
                                        @for applicant in &panel.requests {
                                            span class="party-role-hover-request" { (&applicant.character.name) }
                                        }
                                    }
                                }
                            }
                            template data-role-left-template {
                                (role_requirements_detail(&panel.role, remaining))
                            }
                            @if can_manage {
                                template data-role-right-template {
                                    (role_requests_detail(party, panel))
                                }
                            }
                        }
                    }
                }
            }
            @if can_manage {
                dialog class="recruitment-dialog" data-recruitment-dialog {
                    form method="dialog" class="dialog-close-form" {
                        button class="dialog-close" aria-label="Close recruitment" { "×" }
                    }
                    header class="recruitment-dialog-header" {
                        h2 { "Recruit party roles" }
                        p { "Describe the adventurers you want to add to your party." }
                    }
                    @if !saved_roles.is_empty() {
                        section class="saved-role-section" {
                            h3 { "Saved roles" }
                            div class="saved-role-list" {
                                @for saved in saved_roles {
                                    div class="saved-role-item" {
                                        button type="button" class="btn btn-secondary btn-small"
                                            data-load-saved-role
                                            data-role-name=(&saved.name)
                                            data-role-requirements=(requirements_json(saved.requirements))
                                            data-role-weapon-precision=(saved.effective_weapon_precision())
                                        { (&saved.name) }
                                        form action=(format!("/party-recruitment/saved/{}/delete", saved.id)) method="post" {
                                            button type="submit" class="btn btn-danger btn-small" aria-label=(format!("Delete {}", saved.name)) { "Delete" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    form action="/party-recruitment/roles" method="post" class="role-builder" data-role-builder {
                        section class="role-details-card" aria-labelledby="role-details-heading" {
                            h3 id="role-details-heading" { "Role details" }
                            div class="role-builder-header" {
                                label class="role-name-field" {
                                    span { "Role name" }
                                    input type="text" name="name" placeholder="e.g. Armored melee";
                                }
                                label class="role-slots-field" {
                                    span { "Slots" }
                                    input type="number" name="quantity" min="1" max="8" value="1" required;
                                }
                                label class="role-save-toggle" {
                                    input type="checkbox" name="save_role" value="true";
                                    span {
                                        strong { "Save as a template" }
                                        small { "Reuse these recommendations later" }
                                    }
                                }
                            }
                        }
                        div class="role-requirements-heading" {
                            h3 { "Individual recommendations" }
                            p { "Applicants may still request to join if they fall short." }
                        }
                        div class="role-requirement-columns role-requirement-columns-individual" {
                            (combat_requirements())
                            div class="role-requirement-group" {
                                header class="role-requirement-heading" {
                                    h3 { "Mobility" }
                                    p { "Movement and sustained physical capability" }
                                }
                                (numeric_requirement("athletics", "Athletics"))
                                (numeric_requirement("endurance", "Endurance"))
                            }
                        }
                        footer class="role-builder-footer" {
                            span class="small-copy text-muted" { "You can edit recruitment by removing and recreating a role." }
                            button type="submit" class="btn btn-primary" { "Add role" }
                        }
                    }
                }
            }
        }
    }
}

fn role_requirements_detail(role: &PartyRecruitmentRole, remaining: usize) -> Markup {
    html! {
        section class="role-inspection-content" {
            h3 class="sidebar-header" { (&role.name) }
            p class="small-copy text-muted" { (remaining) " open " @if remaining == 1 { "slot" } @else { "slots" } }
            div class="role-detail-list" {
                @for (required, label) in [
                    (role.requirements.melee, "Melee"),
                    (role.requirements.ranged, "Ranged"),
                    (role.requirements.heavy, "Heavy"),
                    (role.requirements.quarter_armor, "1/4 armor"),
                    (role.requirements.half_armor, "1/2 armor"),
                    (role.requirements.three_quarter_armor, "3/4 armor"),
                    (role.requirements.full_armor, "Full armor"),
                ] {
                    @if required { div class="role-detail-row" { span { (label) } strong { "Required" } } }
                }
                @if role.effective_weapon_precision() > 0.0 {
                    div class="role-detail-row" {
                        span { "Weapon precision" }
                        strong { (format!("{:.1}+", role.effective_weapon_precision())) }
                    }
                }
                @for (minimum, label) in [
                    (role.requirements.athletics, "Athletics"),
                    (role.requirements.endurance, "Endurance"),
                ] {
                    @if minimum > 0 { div class="role-detail-row" { span { (label) } strong { (minimum) "+" } } }
                }
            }
        }
    }
}

fn role_requests_detail(party: &Party, panel: &RecruitmentRolePanel) -> Markup {
    html! {
        section class="role-inspection-content" {
            h3 class="sidebar-header" { "Join requests" }
            @if panel.requests.is_empty() {
                p class="small-copy text-muted" { "No pending requests for this role." }
            } @else {
                div class="role-request-detail-list" {
                    @for applicant in &panel.requests {
                        article class=(if applicant.request.meets_requirements { "role-request-detail" } else { "role-request-detail role-applicant-warning" }) {
                            button type="button" class="role-applicant-portrait" data-select-role-applicant
                                aria-label=(format!("Inspect {}", applicant.character.name)) {
                                span class="party-portrait-initial" {
                                    span class="party-portrait-face" { (applicant.character.name.chars().next().unwrap_or('?')) }
                                    span class="party-portrait-name" { (&applicant.character.name) }
                                }
                            }
                            p class="small-copy" {
                                @if applicant.request.meets_requirements { "Meets every individual recommendation" }
                                @else { "Below one or more recommendations" }
                            }
                            (candidate_contribution(applicant.contribution))
                            @if let Some(capability) = &applicant.capability {
                                (capability_comparison(panel.role.requirements, panel.role.effective_weapon_precision(), capability))
                            } @else {
                                p class="small-copy text-muted" { "Exact capability details unavailable." }
                            }
                            div class="flex gap-sm" {
                                form action=(format!("/parties/{}/requests/{}/accept", party.id, applicant.request.id)) method="post" {
                                    button class="btn btn-primary btn-small" { "Accept" }
                                }
                                form action=(format!("/parties/{}/requests/{}/reject", party.id, applicant.request.id)) method="post" {
                                    button class="btn btn-secondary btn-small" { "Reject" }
                                }
                            }
                            template data-applicant-left-template {
                                (character_stats_panel(&applicant.character, applicant.capability.as_ref(), applicant.attributes.as_ref(), applicant.skills.as_ref(), applicant.limbs.as_ref()))
                            }
                            template data-applicant-center-template {
                                (character_visual_preview(&applicant.character))
                            }
                        }
                    }
                }
            }
        }
    }
}

fn capability_comparison(
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
    c: &CharacterCapability,
) -> Markup {
    html! {
        p class="capability-detail-tags" { (capability_tag_label(c)) }
        div class="role-detail-list capability-exact-values" {
            @for (actual, label) in [
                (c.athletics, "Athletics"),
                (c.weapon_precision, "Weapon precision"),
                (c.endurance, "Endurance"),
                (c.medicine, "Medicine"),
                (c.surgery, "Surgery"),
                (c.charisma, "Charisma"),
                (c.faith, "Faith"),
            ] {
                div class="role-detail-row" { span { (label) } strong { (format!("{actual:.2}")) } }
            }
        }
        div class="role-detail-list" {
            @for (required, actual, label) in [
                (requirements.melee, c.melee, "Melee"),
                (requirements.ranged, c.ranged, "Ranged"),
                (requirements.heavy, c.heavy, "Heavy"),
                (requirements.quarter_armor, c.quarter_armor, "1/4 armor"),
                (requirements.half_armor, c.half_armor, "1/2 armor"),
                (requirements.three_quarter_armor, c.three_quarter_armor, "3/4 armor"),
                (requirements.full_armor, c.full_armor, "Full armor"),
            ] {
                @if required {
                    div class=(if actual { "role-detail-row" } else { "role-detail-row role-detail-miss" }) {
                        span { (label) } strong { @if actual { "Yes" } @else { "No" } }
                    }
                }
            }
            @if weapon_precision > 0.0 {
                div class=(if c.weapon_precision >= weapon_precision { "role-detail-row" } else { "role-detail-row role-detail-miss" }) {
                    span { "Weapon precision" }
                    strong { (format!("{:.2} / {:.1}", c.weapon_precision, weapon_precision)) }
                }
            }
            @for (minimum, actual, label) in [
                (requirements.athletics, c.athletics, "Athletics"),
                (requirements.endurance, c.endurance, "Endurance"),
            ] {
                @if minimum > 0 {
                    div class=(if actual.round() as u8 >= minimum { "role-detail-row" } else { "role-detail-row role-detail-miss" }) {
                        span { (label) }
                        strong { (format!("{actual:.2} / {minimum}")) }
                    }
                }
            }
        }
    }
}

fn capability_tag_label(c: &CharacterCapability) -> String {
    let mut labels = Vec::new();
    for (enabled, label) in [(c.melee, "Melee"), (c.ranged, "Ranged"), (c.heavy, "Heavy")] {
        if enabled {
            labels.push(label);
        }
    }
    if let Some(label) =
        adventuresim_core::capability::weapon_precision_tier_label(c.weapon_precision)
    {
        labels.push(label);
    }
    if c.full_armor {
        labels.push("Full armor");
    } else if c.three_quarter_armor {
        labels.push("3/4 armor");
    } else if c.half_armor {
        labels.push("1/2 armor");
    } else if c.quarter_armor {
        labels.push("1/4 armor");
    }
    if labels.is_empty() {
        "No equipment tags".into()
    } else {
        labels.join(" · ")
    }
}

fn combat_requirements() -> Markup {
    html! { div class="role-requirement-group" {
        header class="role-requirement-heading" {
            h3 { "Combat" }
            p { "Preferred fighting capabilities" }
        }
        div class="role-toggle-grid" {
            @for (name, label) in [("melee", "Melee"), ("ranged", "Ranged"), ("heavy", "Heavy")] {
                label class="role-toggle" { input type="checkbox" name=(name) value="true"; span { (label) } }
            }
        }
        (weapon_precision_requirement())
        (armor_requirement())
    } }
}

fn weapon_precision_requirement() -> Markup {
    html! { label class="role-slider" {
        span class="role-slider-heading" { span { "Precision" } output data-slider-output="weapon_precision" { "Off" } }
        div class="role-slider-control" {
            span class="role-slider-rail" aria-hidden="true" {}
            input type="range" name="weapon_precision" min="0" max="2" step="0.5" value="0"
                data-discrete-slider data-slider-labels="Off|0.5|1.0|1.5|2.0";
        }
        span class="role-slider-ticks role-slider-ticks-precision" aria-hidden="true" {
            span { "Off" } span title="Clubs and hammers" { "0.5" } span title="Axes" { "1.0" }
            span title="Swords and spears" { "1.5" } span title="Rapiers and bodkin ammunition" { "2.0" }
        }
    } }
}

fn numeric_requirement(name: &str, label: &str) -> Markup {
    html! { label class="role-slider" {
        span class="role-slider-heading" { span { (label) } output data-slider-output=(name) { "Off" } }
        div class="role-slider-control" {
            span class="role-slider-rail" aria-hidden="true" {}
            input type="range" name=(name) min="0" max="5" step="1" value="0"
                data-discrete-slider data-slider-labels="Off|1|2|3|4|5";
        }
        span class="role-slider-ticks" aria-hidden="true" {
            @for value in 0..=5 { span { (value) } }
        }
    } }
}

fn aggregate_check_control(
    party: &Party,
    label: &str,
    icon: &str,
    field: &str,
    current: f32,
    target: f32,
    can_manage: bool,
) -> Markup {
    let target = target.round().clamp(0.0, 5.0);
    let deficient = target > 0.0 && current + 0.001 < target;
    let current_width = (current.clamp(0.0, 5.0) / 5.0) * 100.0;
    let target_position = (target / 5.0) * 100.0;
    html! {
        div class=(if deficient { "party-aggregate-check deficient" } else { "party-aggregate-check" })
            data-party-check=(field) data-party-check-current=(current) {
            span class=(format!("stat-icon stat-icon-{icon}"))
                style=(format!("--stat-icon: url('/static/icons/stats/skills/{icon}.png')"))
                role="img" aria-label=(label) {}
            (party_check_target_form(
                party, field, label, current, target, current_width, target_position, can_manage,
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn party_check_target_form(
    party: &Party,
    field: &str,
    label: &str,
    current: f32,
    target: f32,
    current_width: f32,
    target_position: f32,
    can_manage: bool,
) -> Markup {
    html! {
        form action="/party-recruitment/check-targets" method="post" class="party-check-target-form"
            data-party-check-target-form data-check-name=(field) {
            input type="hidden" name="medicine" value=(party.medicine_target.round().clamp(0.0, 5.0));
            input type="hidden" name="surgery" value=(party.surgery_target.round().clamp(0.0, 5.0));
            input type="hidden" name="charisma" value=(party.charisma_target.round().clamp(0.0, 5.0));
            input type="hidden" name="faith" value=(party.faith_target.round().clamp(0.0, 5.0));
            div class=(if can_manage { "party-check-track party-check-track-editable" } else { "party-check-track" })
                data-party-check-track data-check-name=(field) data-check-label=(label)
                data-check-current=(current) data-check-target=(target)
                title=(format!("{label}: {current:.1}; target {target:.0}")) {
                span class="party-check-current" style=(format!("width:{current_width:.1}%")) {}
                span class="party-check-exact" { (format!("{label}: {current:.1} · target {target:.0}")) }
                @if can_manage {
                    button type="button" class="party-check-target-handle"
                        data-party-check-target-handle data-check-name=(field)
                        style=(format!("left:{target_position:.1}%"))
                        role="slider" aria-label=(format!("{label} party target"))
                        aria-valuemin="0" aria-valuemax="5" aria-valuenow=(target as u8)
                        title=(format!("{label} target: {target:.0}")) {}
                }
            }
        }
    }
}

fn candidate_contribution(contribution: PartyCheckSummary) -> Markup {
    html! {
        div class="candidate-party-contribution" {
            @for (value, label) in [
                (contribution.medicine, "Medicine"),
                (contribution.surgery, "Surgery"),
                (contribution.charisma, "Charisma"),
                (contribution.faith, "Faith"),
            ] {
                @if value > 0.005 { span { (label) " +" (format!("{value:.2}")) } }
            }
        }
    }
}

fn armor_requirement() -> Markup {
    html! { label class="role-slider" {
        span class="role-slider-heading" { span { "Armor" } output data-slider-output="armor_tier" { "Off" } }
        div class="role-slider-control" {
            span class="role-slider-rail" aria-hidden="true" {}
            input type="range" name="armor_tier" min="0" max="4" step="1" value="0"
                data-discrete-slider data-slider-labels="Off|1/4|1/2|3/4|Full";
        }
        span class="role-slider-ticks role-slider-ticks-armor" aria-hidden="true" {
            span { "Off" } span { "1/4" } span { "1/2" } span { "3/4" } span { "Full" }
        }
    } }
}

fn requirements_json(requirements: RecruitmentRequirements) -> String {
    serde_json::to_string(&requirements).unwrap_or_else(|_| "{}".into())
}

pub fn requirements_label(r: RecruitmentRequirements, weapon_precision: f32) -> String {
    let mut labels = Vec::new();
    for (required, label) in [
        (r.melee, "Melee"),
        (r.ranged, "Ranged"),
        (r.heavy, "Heavy"),
        (r.quarter_armor, "1/4 armor"),
        (r.half_armor, "1/2 armor"),
        (r.three_quarter_armor, "3/4 armor"),
        (r.full_armor, "Full armor"),
    ] {
        if required {
            labels.push(label.to_string());
        }
    }
    if weapon_precision > 0.0 {
        labels.push(format!("Precision {weapon_precision:.1}+"));
    }
    for (value, label) in [(r.athletics, "Athletics"), (r.endurance, "Endurance")] {
        if value > 0 {
            labels.push(format!("{label} {value}+"));
        }
    }
    if labels.is_empty() {
        "No minimum recommendations".into()
    } else {
        labels.join(" · ")
    }
}
