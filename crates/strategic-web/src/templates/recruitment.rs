use maud::{Markup, html};

use crate::spacetimedb::{
    Character, Party, PartyJoinRequest, PartyRecruitmentRole, RecruitmentRequirements,
    SavedRecruitmentRole,
};

pub struct RecruitmentRolePanel {
    pub role: PartyRecruitmentRole,
    pub filled: Vec<Character>,
    pub requests: Vec<(PartyJoinRequest, Character)>,
}

pub fn recruitment_panel(
    party: &Party,
    active_character_id: u64,
    roles: &[RecruitmentRolePanel],
    saved_roles: &[SavedRecruitmentRole],
) -> Markup {
    let can_manage = party.leader_id == active_character_id;
    html! {
        div data-party-recruitment-panel
            data-leader-id=(party.leader_id)
            data-can-manage=(can_manage)
        {
            div data-party-slot-groups hidden {
                @for panel in roles {
                    div class="party-role-group" data-party-role-group data-role-id=(panel.role.id) {
                        div class="party-role-portraits" {
                            span class="party-role-connector" aria-hidden="true" {}
                            @for character in &panel.filled {
                                span data-filled-character-id=(character.id) {}
                            }
                            @for _ in panel.filled.len()..panel.role.quantity as usize {
                                span class="party-portrait party-role-slot" title=(format!("Open slot: {}", panel.role.name)) {
                                    span class="party-slot-plus" { "+" }
                                    span class="party-portrait-name" { (&panel.role.name) }
                                }
                            }
                            @if can_manage {
                                span class="party-role-notification-badge"
                                    data-party-role-notification-badge data-role-id=(panel.role.id)
                                    hidden[panel.requests.is_empty()] { (panel.requests.len()) }
                            }
                        }
                        @if can_manage {
                            div class="party-role-invitations" {
                                strong { (&panel.role.name) }
                                span class="small-copy text-muted" { (requirements_label(panel.role.requirements)) }
                                @if panel.requests.is_empty() {
                                    span class="small-copy text-muted" { "No invitations" }
                                } @else {
                                    @for (request, character) in &panel.requests {
                                        div class=(if request.meets_requirements { "role-applicant" } else { "role-applicant role-applicant-warning" }) {
                                            span { (&character.name) @if !request.meets_requirements { " ! Below recommendations" } }
                                            div class="flex gap-sm" {
                                                form action=(format!("/parties/{}/requests/{}/accept", party.id, request.id)) method="post" {
                                                    button class="btn btn-primary btn-small" { "Accept" }
                                                }
                                                form action=(format!("/parties/{}/requests/{}/reject", party.id, request.id)) method="post" {
                                                    button class="btn btn-secondary btn-small" { "Reject" }
                                                }
                                            }
                                        }
                                    }
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
                    h2 { "Recruit party roles" }
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
                        div class="role-builder-header" {
                            label { "Role name" input type="text" name="name" placeholder="e.g. Armored melee"; }
                            label { "Slots" input type="number" name="quantity" min="1" max="8" value="1" required; }
                            label class="role-save-toggle" { input type="checkbox" name="save_role" value="true"; " Save this role" }
                        }
                        div class="role-requirement-columns" {
                            (boolean_group("Combat", &[
                                ("melee", "Melee"), ("ranged", "Ranged"), ("precise", "Precise"),
                                ("heavy", "Heavy"),
                                ("blunt", "Blunt"), ("slash", "Slash"), ("pierce", "Pierce"),
                            ]))
                            div class="role-requirement-group" {
                                h3 { "Armor & mobility" }
                                (armor_requirement())
                                (numeric_requirement("athletics", "Athletics"))
                                (numeric_requirement("endurance", "Endurance"))
                            }
                            div class="role-requirement-group" {
                                h3 { "Other" }
                                (numeric_requirement("medicine", "Medicine"))
                                (numeric_requirement("surgery", "Surgery"))
                                (numeric_requirement("charisma", "Charisma"))
                                (numeric_requirement("faith", "Faith"))
                            }
                        }
                        button type="submit" class="btn btn-primary" { "Add role" }
                    }
                }
            }
        }
    }
}

fn boolean_group(title: &str, requirements: &[(&str, &str)]) -> Markup {
    html! { div class="role-requirement-group" { h3 { (title) }
        @for (name, label) in requirements {
            label class="role-toggle" { input type="checkbox" name=(name) value="true"; span { (label) } }
        }
    } }
}

fn numeric_requirement(name: &str, label: &str) -> Markup {
    html! { label class="role-slider" {
        span class="role-slider-heading" { span { (label) } output data-slider-output=(name) { "Off" } }
        input type="range" name=(name) min="0" max="5" step="1" value="0"
            data-discrete-slider data-slider-labels="Off|1|2|3|4|5";
        span class="role-slider-ticks" aria-hidden="true" {
            @for value in 0..=5 { span { (value) } }
        }
    } }
}

fn armor_requirement() -> Markup {
    html! { label class="role-slider" {
        span class="role-slider-heading" { span { "Armor" } output data-slider-output="armor_tier" { "Off" } }
        input type="range" name="armor_tier" min="0" max="4" step="1" value="0"
            data-discrete-slider data-slider-labels="Off|1/4|1/2|3/4|Full";
        span class="role-slider-ticks role-slider-ticks-armor" aria-hidden="true" {
            span { "Off" } span { "1/4" } span { "1/2" } span { "3/4" } span { "Full" }
        }
    } }
}

fn requirements_json(requirements: RecruitmentRequirements) -> String {
    serde_json::to_string(&requirements).unwrap_or_else(|_| "{}".into())
}

pub fn requirements_label(r: RecruitmentRequirements) -> String {
    let mut labels = Vec::new();
    for (required, label) in [
        (r.melee, "Melee"),
        (r.ranged, "Ranged"),
        (r.precise, "Precise"),
        (r.heavy, "Heavy"),
        (r.quarter_armor, "1/4 armor"),
        (r.half_armor, "1/2 armor"),
        (r.three_quarter_armor, "3/4 armor"),
        (r.full_armor, "Full armor"),
        (r.blunt, "Blunt"),
        (r.slash, "Slash"),
        (r.pierce, "Pierce"),
    ] {
        if required {
            labels.push(label.to_string());
        }
    }
    for (value, label) in [
        (r.athletics, "Athletics"),
        (r.endurance, "Endurance"),
        (r.medicine, "Medicine"),
        (r.surgery, "Surgery"),
        (r.charisma, "Charisma"),
        (r.faith, "Faith"),
    ] {
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
