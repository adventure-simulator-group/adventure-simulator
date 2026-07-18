use maud::{Markup, html};

use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterSkills, Party,
    PartyJoinRequest, PartyRecruitmentRole, RecruitmentRequirements, SavedRecruitmentRole,
};
use crate::templates::settlement::{character_stats_panel, character_visual_preview};
use crate::templates::stat_game_icon_name;

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
    pub medical: crate::medical::MedicalPresentation,
}

pub struct RecruitmentRolePanel {
    pub role: PartyRecruitmentRole,
    pub filled: Vec<Character>,
    pub requests: Vec<RecruitmentApplicant>,
}

/// Server-rendered role inspection panels used by the service-quest fragment.
/// Keeping this markup here prevents the browser from maintaining a second
/// component renderer alongside Maud.
pub fn service_role_inspection(
    role_name: &str,
    requirements: &[String],
    party_name: &str,
    leader_name: &str,
    remaining: u32,
    match_level: &str,
    match_summary: &str,
    join_path: &str,
    can_accept: bool,
) -> (String, String) {
    let left = html! {
        section class="role-inspection-panel role-inspection-content" data-service-role-inspection {
            h3 class="sidebar-header" { (role_name) }
            div class="role-detail-list" {
                @if requirements.is_empty() { div class="role-detail-row" { "No minimum recommendations" } }
                @for requirement in requirements { div class="role-detail-row" { (requirement) } }
            }
        }
    };
    let right = html! {
        section class="role-inspection-panel role-inspection-content" data-service-role-inspection {
            h3 class="sidebar-header" { (party_name) }
            p { "Led by " (leader_name) }
            p class=(format!("small-copy service-role-match service-role-match-{match_level}")) { (match_summary) }
            p class="small-copy text-muted" { (remaining) " opening" @if remaining != 1 { "s" } }
            form method="post" action=(join_path) {
                button type="submit" class="btn btn-primary btn-block mt-1" disabled[!can_accept] {
                    "Send request to join"
                }
            }
        }
    };
    (left.into_string(), right.into_string())
}

pub fn recruitment_panel(
    party: &Party,
    _active_character_id: u64,
    roles: &[RecruitmentRolePanel],
    saved_roles: &[SavedRecruitmentRole],
    checks: PartyCheckSummary,
) -> Markup {
    let can_manage = true;
    html! {
        div data-party-recruitment-panel
            data-leader-id=(party.leader_id)
            data-can-manage=(can_manage)
        {
            (aggregate_check_bars(party, checks, None, can_manage, false))
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
                                    (role_requests_detail(party, panel, checks))
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
                        h2 data-role-builder-heading { "Recruit party roles" }
                        p { "Describe the adventurers you want to add to your party." }
                    }
                    section class="saved-role-section" aria-labelledby="current-role-heading" {
                        div class="saved-role-heading" {
                            h3 id="current-role-heading" { "Current roles" }
                            span class="small-copy text-muted" { "Edit recommendations and slot counts, or remove a role." }
                        }
                        div class="saved-role-list current-role-list" {
                            @if roles.is_empty() {
                                span class="small-copy text-muted" { "No recruitment roles yet." }
                            } @else {
                                @for panel in roles {
                                    div class="saved-role-item current-role-item" {
                                        button type="button" class="saved-role-load"
                                            data-edit-current-role
                                            data-role-id=(panel.role.id)
                                            data-role-name=(&panel.role.name)
                                            data-role-quantity=(panel.role.quantity)
                                            data-role-filled=(panel.filled.len())
                                            data-role-requirements=(requirements_json(panel.role.requirements))
                                            data-role-weapon-precision=(panel.role.effective_weapon_precision())
                                        {
                                            (&panel.role.name)
                                            span class="small-copy text-muted" {
                                                " " (panel.filled.len()) "/" (panel.role.quantity)
                                            }
                                        }
                                        button type="button" class="saved-role-action saved-role-rename"
                                            data-edit-current-role
                                            data-role-id=(panel.role.id)
                                            data-role-name=(&panel.role.name)
                                            data-role-quantity=(panel.role.quantity)
                                            data-role-filled=(panel.filled.len())
                                            data-role-requirements=(requirements_json(panel.role.requirements))
                                            data-role-weapon-precision=(panel.role.effective_weapon_precision())
                                            aria-label=(format!("Edit {}", panel.role.name)) { "Edit" }
                                        form action=(format!("/party-recruitment/roles/{}/delete", panel.role.id)) method="post" class="saved-role-delete-form" {
                                            button type="submit" class="saved-role-action saved-role-delete"
                                                aria-label=(format!("Delete {}", panel.role.name)) title=(format!("Delete {}", panel.role.name)) { "Ã—" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    section class="saved-role-section" aria-labelledby="saved-role-heading" {
                        div class="saved-role-heading" {
                            h3 id="saved-role-heading" { "Saved roles" }
                            span class="small-copy text-muted" { "Choose a template or save the current recommendations." }
                        }
                        div class="saved-role-list" {
                            @for saved in saved_roles {
                                div class="saved-role-item" {
                                    button type="button" class="saved-role-load"
                                        data-load-saved-role
                                        data-role-name=(&saved.name)
                                        data-role-requirements=(requirements_json(saved.requirements))
                                        data-role-weapon-precision=(saved.effective_weapon_precision())
                                    { (&saved.name) }
                                    button type="button" class="saved-role-action saved-role-rename"
                                        data-rename-saved-role data-role-id=(saved.id) data-role-name=(&saved.name)
                                        aria-label=(format!("Rename {}", saved.name)) { "Rename" }
                                    form action=(format!("/party-recruitment/saved/{}/delete", saved.id)) method="post" class="saved-role-delete-form" {
                                        button type="submit" class="saved-role-action saved-role-delete"
                                            aria-label=(format!("Delete {}", saved.name)) title=(format!("Delete {}", saved.name)) { "×" }
                                    }
                                }
                            }
                            button type="button" class="saved-role-save" data-save-current-role {
                                span aria-hidden="true" { "+" }
                                "Save role"
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
                            span class="small-copy text-muted" data-role-builder-help { "Create one visually grouped portrait per slot." }
                            button type="submit" class="btn btn-primary" data-role-builder-submit { "Add role" }
                        }
                    }
                }
                dialog class="role-name-dialog" data-save-role-dialog {
                    form method="dialog" class="dialog-close-form" {
                        button class="dialog-close" aria-label="Close save role dialog" { "×" }
                    }
                    form action="/party-recruitment/saved" method="post" class="role-name-form" data-save-role-form {
                        h3 { "Save this role" }
                        p class="small-copy text-muted" { "Name this set of recommendations so you can reuse it later." }
                        label {
                            span { "Role name" }
                            input type="text" name="name" required autofocus placeholder="e.g. Armored melee";
                        }
                        div data-saved-role-fields {}
                        footer {
                            button type="button" class="btn btn-secondary" data-cancel-role-name { "Cancel" }
                            button type="submit" class="btn btn-primary" { "Save" }
                        }
                    }
                }
                dialog class="role-name-dialog" data-rename-role-dialog {
                    form method="dialog" class="dialog-close-form" {
                        button class="dialog-close" aria-label="Close rename role dialog" { "×" }
                    }
                    form method="post" class="role-name-form" data-rename-role-form {
                        h3 { "Rename saved role" }
                        label {
                            span { "Role name" }
                            input type="text" name="name" required autofocus;
                        }
                        footer {
                            button type="button" class="btn btn-secondary" data-cancel-role-name { "Cancel" }
                            button type="submit" class="btn btn-primary" { "Rename" }
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

fn role_requests_detail(
    party: &Party,
    panel: &RecruitmentRolePanel,
    checks: PartyCheckSummary,
) -> Markup {
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
                            (aggregate_check_bars(party, checks, Some(applicant.contribution), false, true))
                            div class="flex gap-sm" {
                                form action=(format!("/parties/{}/requests/{}/accept", party.id, applicant.request.id)) method="post" {
                                    button class="btn btn-primary btn-small" { "Accept" }
                                }
                                form action=(format!("/parties/{}/requests/{}/reject", party.id, applicant.request.id)) method="post" {
                                    button class="btn btn-secondary btn-small" { "Reject" }
                                }
                            }
                            template data-applicant-left-template {
                                (character_stats_panel(&applicant.character, applicant.capability.as_ref(), applicant.attributes.as_ref(), applicant.skills.as_ref(), applicant.limbs.as_ref(), &applicant.medical))
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

/// Shared aggregate-check display for leader editing and applicant projections.
/// Projected displays omit checks to which the candidate contributes nothing.
pub fn aggregate_check_bars(
    party: &Party,
    checks: PartyCheckSummary,
    contribution: Option<PartyCheckSummary>,
    can_manage: bool,
    inline: bool,
) -> Markup {
    html! {
        div class=(if inline { "party-aggregate-checks party-aggregate-checks-inline" } else { "party-aggregate-checks" })
            data-party-aggregate-checks {
            @for (label, icon, field, current, target, added) in [
                ("Medicine", "medicine", "medicine", checks.medicine, party.medicine_target, contribution.map_or(0.0, |value| value.medicine)),
                ("Surgery", "surgeon", "surgery", checks.surgery, party.surgery_target, contribution.map_or(0.0, |value| value.surgery)),
                ("Charisma", "charisma", "charisma", checks.charisma, party.charisma_target, contribution.map_or(0.0, |value| value.charisma)),
                ("Faith", "faith", "faith", checks.faith, party.faith_target, contribution.map_or(0.0, |value| value.faith)),
            ] {
                @if contribution.is_none() || added.abs() > 0.005 {
                    (aggregate_check_control(party, label, icon, field, current, target, added, can_manage))
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn aggregate_check_control(
    party: &Party,
    label: &str,
    icon: &str,
    field: &str,
    current: f32,
    target: f32,
    contribution: f32,
    can_manage: bool,
) -> Markup {
    let target = target.round().clamp(0.0, 5.0);
    let deficient = target > 0.0 && current + 0.001 < target;
    let current_width = (current.clamp(0.0, 5.0) / 5.0) * 100.0;
    let projected = (current + contribution).clamp(0.0, 5.0);
    let contribution_width = ((projected - current.clamp(0.0, 5.0)).abs() / 5.0) * 100.0;
    let target_position = (target / 5.0) * 100.0;
    html! {
        div class=(if deficient { "party-aggregate-check deficient" } else { "party-aggregate-check" })
            data-party-check=(field) data-party-check-current=(current) {
            span class=(format!("stat-icon stat-icon-{icon}"))
                style=(format!("--stat-icon: url('/static/icons/game/{}.svg')", stat_game_icon_name(icon)))
                role="img" aria-label=(label) {}
            (party_check_target_form(
                party, field, label, current, contribution, target, current_width,
                contribution_width, target_position, can_manage,
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
    contribution: f32,
    target: f32,
    current_width: f32,
    contribution_width: f32,
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
                title=(if contribution.abs() > 0.005 {
                    format!("{label}: {current:.1} {contribution:+.1} = {:.1}; target {target:.0}", current + contribution)
                } else {
                    format!("{label}: {current:.1}; target {target:.0}")
                }) {
                span class="party-check-current" style=(format!("width:{current_width:.1}%")) {}
                @if contribution.abs() > 0.005 {
                    span class=(if contribution > 0.0 { "party-check-contribution" } else { "party-check-contribution party-check-contribution-negative" })
                        style=(format!("left:{:.1}%;width:{contribution_width:.1}%", if contribution > 0.0 { current_width } else { current_width - contribution_width })) {}
                }
                span class="party-check-exact" {
                    @if contribution.abs() > 0.005 {
                        (format!("{label}: {current:.1} {contribution:+.1} = {:.1} · target {target:.0}", current + contribution))
                    } @else {
                        (format!("{label}: {current:.1} · target {target:.0}"))
                    }
                }
                @if can_manage {
                    button type="button" class="party-check-target-handle"
                        data-party-check-target-handle data-check-name=(field)
                        style=(format!("left:{target_position:.1}%"))
                        role="slider" aria-label=(format!("{label} party target"))
                        aria-valuemin="0" aria-valuemax="5" aria-valuenow=(target as u8)
                        title=(format!("{label} target: {target:.0}")) {}
                } @else {
                    span class="party-check-target-handle party-check-target-static"
                        style=(format!("left:{target_position:.1}%")) aria-hidden="true" {}
                }
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
