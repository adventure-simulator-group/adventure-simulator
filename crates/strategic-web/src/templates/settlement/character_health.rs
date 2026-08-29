use adventuresim_core::surgery::SurgeryProcedure;
use maud::{Markup, html};

use super::{
    character_skills::{SkillRankBarOptions, skill_rank_bar},
    context::LocationView,
    trade::filth_status_bar,
};
use crate::medical::{ChartGapPresentation, ChartReadingPresentation, MedicalPresentation};
use crate::spacetimedb::{
    BackendCorpse, BodyRegion, CharacterAttributes, CharacterLimbs, CharacterStrategicCondition,
    CharacterView, LimbInjury, ProjectileKind, RetainedProjectile,
};
use crate::templates::{
    decorative_game_icon, game_icon, item_display_name, sidebar_section, stat_icon_path,
};

fn surgery_limb_name(limb: BodyRegion) -> &'static str {
    match limb {
        BodyRegion::LeftArm => "Left arm",
        BodyRegion::RightArm => "Right arm",
        BodyRegion::LeftLeg => "Left leg",
        BodyRegion::RightLeg => "Right leg",
        BodyRegion::Chest => "Chest",
        BodyRegion::Abdomen => "Stomach",
        BodyRegion::Head => "Head",
    }
}

fn surgery_limb_slug(limb: BodyRegion) -> &'static str {
    limb.slug()
}

fn surgery_duration(procedure: SurgeryProcedure, skill: f32, dc: f32) -> u64 {
    adventuresim_core::surgery::procedure_duration_minutes(procedure, skill, dc)
}

fn surgery_procedure_skill(procedure: SurgeryProcedure, surgery: f32, self_treatment: bool) -> f32 {
    adventuresim_core::surgery::procedure_skill(procedure, surgery, self_treatment)
}

#[derive(Clone, Copy)]
enum SurgeryItemRequirement {
    BandageConsumed,
    SurgeryKitReusable,
    SplintEquipped,
}

fn surgery_supply(label: &str, icon: &str, quantity: u32) -> Markup {
    let description = format!("{label}: {quantity} available");
    html! {
        div class="surgery-supply" data-strategic-tooltip=(&description)
            aria-label=(&description) tabindex="0" {
            (decorative_game_icon(icon))
            span class="surgery-item-overlay surgery-item-quantity" aria-hidden="true" { "x" (quantity) }
        }
    }
}

fn surgery_item_requirement(requirement: SurgeryItemRequirement) -> Markup {
    let (label, accessible_label, icon) = match requirement {
        SurgeryItemRequirement::BandageConsumed => {
            ("Expend one bandage", "Expend one bandage", "bandage-roll")
        }
        SurgeryItemRequirement::SurgeryKitReusable => (
            "Requires surgery kit",
            "Requires surgery kit; reusable and not consumed",
            "medical-pack",
        ),
        SurgeryItemRequirement::SplintEquipped => {
            ("Equips 1 splint", "Equips 1 splint", "arm-bandage")
        }
    };
    html! {
        span class="surgery-item-requirement" data-strategic-tooltip=(label)
            aria-label=(accessible_label) tabindex="0" {
            (decorative_game_icon(icon))
            @match requirement {
                SurgeryItemRequirement::BandageConsumed => {
                    span class="surgery-item-overlay surgery-item-quantity" aria-hidden="true" { "x1" }
                }
                SurgeryItemRequirement::SurgeryKitReusable => {}
                SurgeryItemRequirement::SplintEquipped => {
                    span class="surgery-item-overlay surgery-item-equipped" aria-hidden="true" {
                        (decorative_game_icon("check-mark"))
                    }
                }
            }
        }
    }
}

fn surgery_difficulty_meter(procedure_label: &str, dc: f32, effective_skill: f32) -> Markup {
    let difficulty = dc.max(0.0);
    let over_cap = difficulty > 5.0;
    let meter_label = format!("{procedure_label} procedure difficulty");
    let accessible_label = format!(
        "{procedure_label}: requires {difficulty:.1} procedure skill; current effective skill {:.1}",
        effective_skill.max(0.0)
    );
    html! {
        div class=(if over_cap { "surgery-difficulty surgery-difficulty-over-cap" } else { "surgery-difficulty" })
            title=[over_cap.then_some("Difficulty exceeds the normal procedure skill scale")] {
            (stat_icon(&meter_label, "skills", "surgeon", true))
            (skill_rank_bar(
                difficulty,
                effective_skill.min(difficulty),
                &meter_label,
                SkillRankBarOptions {
                    show_value: false,
                    extra_class: Some("surgery-difficulty-meter"),
                    aria_label: Some(&accessible_label),
                },
            ))
            @if over_cap {
                span class="surgery-difficulty-over-cap-marker" aria-hidden="true" { "+" }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the surgery row mirrors independent procedure requirements and form selections"
)]
fn surgery_procedure_row(
    action: &str,
    label: &str,
    icon: &str,
    procedure: SurgeryProcedure,
    item_requirements: &[SurgeryItemRequirement],
    duration: u64,
    dc: f32,
    effective_skill: f32,
    unavailable: Option<&str>,
    disabled: Option<&str>,
    projectile_id: Option<u64>,
    soap_available: bool,
    soap_applicable: bool,
    selected_alcohol: Option<&str>,
) -> Markup {
    let row_class = if unavailable.is_some() {
        "surgery-procedure surgery-procedure-unavailable"
    } else {
        "surgery-procedure"
    };
    let unavailable_label = unavailable.map(|reason| format!("{label}: {reason}"));
    html! {
        form method="post" action=(action) class=(row_class)
            data-strategic-tooltip=[unavailable] aria-label=[unavailable_label.as_deref()]
            tabindex=[unavailable.map(|_| "0")] {
            input type="hidden" name="procedure" value=(procedure.as_str());
            input type="hidden" name="action_id" value=(crate::templates::fresh_request_token("treatment"));
            @if let Some(projectile_id) = projectile_id {
                input type="hidden" name="projectile_id" value=(projectile_id);
            }
            @if soap_applicable {
                label class="surgery-soap-option" title="Consumes 0.04 of a soap unit; lowers contamination risk independently of other supplies" {
                    input type="checkbox" name="use_soap" value="true" disabled[!soap_available];
                    " Use 0.04 soft soap"
                }
            }
            @if icon == "bullet-visual" {
                span class="procedure-projectile-visual projectile-ball" role="img" aria-label=(label) {}
            } @else {
                (game_icon(label, icon))
            }
            div class="surgery-procedure-copy" {
                strong { (label) }
            }
            dl class="surgery-procedure-facts" {
                div { dt { "Time" } dd { (duration) " min" } }
                div class="surgery-procedure-difficulty" {
                    dt class="sr-only" { "Difficulty" }
                    dd { (surgery_difficulty_meter(label, dc, effective_skill)) }
                }
            }
            @if !item_requirements.is_empty() {
                ul class="surgery-item-requirements" aria-label="Required items" {
                    @for requirement in item_requirements {
                        li { (surgery_item_requirement(*requirement)) }
                    }
                }
            }
            @if let Some(item_id) = selected_alcohol {
                div class="surgery-alcohol-consumption" aria-label=(format!("Consumes one {} for disinfection", item_display_name(item_id))) {
                    (game_icon(&format!("Consumes one {}", item_display_name(item_id)), "beer-stein"))
                    span { "Consumes 1 " (item_display_name(item_id)) }
                }
            }
            @if let Some(reason) = disabled {
                button type="submit" class="btn btn-block" disabled title=(reason) aria-label=(format!("{label}: {reason}")) { (label) }
            } @else {
                button type="submit" class="btn btn-primary" { (label) }
            }
        }
    }
}

/// Manual limb treatment is an SSR-open dialog over the ordinary character rails.
#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub fn surgery_dialog(
    location: &LocationView,
    active_character: &CharacterView,
    patient: &CharacterView,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    selected_limb: BodyRegion,
    bandages: u32,
    surgery_kits: u32,
    splints: u32,
    soaps: u32,
    alcohol_units: u32,
    selected_alcohol: Option<&str>,
    surgery_check: f32,
) -> Markup {
    let action = location.preserve_building(format!(
        "{}/party/{}/surgery/{}/procedure",
        location.base_path(),
        patient.id,
        surgery_limb_slug(selected_limb)
    ));
    let selected = injuries
        .iter()
        .find(|injury| crate::spacetimedb::core_body_region(injury.limb) == selected_limb);
    let cut = selected.map_or(0.0, |injury| injury.cut_damage.max(0.0));
    let bruise = selected.map_or(0.0, |injury| injury.bruise_damage.max(0.0));
    let fracture = selected.map_or(0.0, |injury| injury.fracture_damage.max(0.0));
    let bandaged = selected.is_some_and(|injury| injury.bandaged);
    let stitched = selected.is_some_and(|injury| injury.stitched);
    let splinted = selected.is_some_and(|injury| injury.splint_inventory_item_id.is_some());
    let has_kit = surgery_kits > 0;
    let self_treatment = active_character.id == patient.id;
    let procedure_skill =
        |procedure| surgery_procedure_skill(procedure, surgery_check, self_treatment);
    let surgery_skill = procedure_skill(SurgeryProcedure::Bandage);
    let extraction_skill = procedure_skill(SurgeryProcedure::Extract);
    let stitching_skill = procedure_skill(SurgeryProcedure::Stitch);
    let close_href = location.preserve_building(if self_treatment {
        format!("{}/party/{}", location.base_path(), patient.id)
    } else {
        format!("{}/party/{}/stats", location.base_path(), patient.id)
    });
    html! {
        div class="character-action-overlay" data-character-action-dialog {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close surgery dialog" {}
            section class="character-action-dialog surgery-dialog" role="dialog" aria-modal="true" aria-labelledby="surgery-dialog-title" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="surgery-dialog-title" { (patient.name) " — " (surgery_limb_name(selected_limb)) }
                    a class="character-action-dialog-close" href=(&close_href) aria-label="Close surgery dialog" { "×" }
                }
                div class="surgery-rail" {
                div class="surgery-supplies" aria-label="Surgery supplies" {
                    (surgery_supply("Bandages", "bandage-roll", bandages))
                    (surgery_supply("Surgery kits", "medical-pack", surgery_kits))
                    (surgery_supply("Splints", "arm-bandage", splints))
                    (surgery_supply("Soft soap", "water-drop", soaps))
                    (surgery_supply("Disinfecting alcohol", "beer-stein", alcohol_units))
                }
                div class="surgery-procedures" {
                    @for projectile in projectiles.iter().filter(|projectile| crate::spacetimedb::core_body_region(projectile.limb) == selected_limb) {
                        @let requires_kit = adventuresim_core::surgery::extraction_requires_surgery_kit(projectile.extraction_dc);
                        (surgery_procedure_row(&action, match projectile.kind { ProjectileKind::Arrowhead => "Remove arrowhead", ProjectileKind::Ball => "Remove ball" }, match projectile.kind { ProjectileKind::Arrowhead => "plain-arrow", ProjectileKind::Ball => "bullet-visual" }, SurgeryProcedure::Extract, if requires_kit { &[SurgeryItemRequirement::SurgeryKitReusable] } else { &[] }, surgery_duration(SurgeryProcedure::Extract, extraction_skill, projectile.extraction_dc), projectile.extraction_dc,
                            extraction_skill, None, if extraction_skill < projectile.extraction_dc { Some("Insufficient Surgery skill") } else if requires_kit && !has_kit { Some("No surgery kit") } else { None }, Some(projectile.id), soaps > 0, true, selected_alcohol))
                    }
                    (surgery_procedure_row(&action, "Bandage", "bandage-roll", SurgeryProcedure::Bandage, &[SurgeryItemRequirement::BandageConsumed], surgery_duration(SurgeryProcedure::Bandage, surgery_skill, 0.0), 0.0,
                        surgery_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if bandaged { Some("Already bandaged") } else if bandages == 0 { Some("No bandages") } else { None }, None, soaps > 0, true, selected_alcohol))
                    (surgery_procedure_row(&action, "Stitch", "scalpel", SurgeryProcedure::Stitch, &[SurgeryItemRequirement::SurgeryKitReusable], surgery_duration(SurgeryProcedure::Stitch, stitching_skill, 2.0), 2.0,
                        stitching_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if stitched { Some("Already stitched") } else if stitching_skill < 2.0 { Some("Insufficient Surgery skill") } else if !has_kit { Some("No surgery kit") } else { None }, None, soaps > 0, true, selected_alcohol))
                    @if splinted {
                        (surgery_procedure_row(&action, "Remove splint", "arm-bandage", SurgeryProcedure::RemoveSplint, &[], surgery_duration(SurgeryProcedure::RemoveSplint, surgery_skill, 0.0), 0.0, surgery_skill, None, None, None, false, false, None))
                    } @else {
                        (surgery_procedure_row(&action, "Splint", "arm-bandage", SurgeryProcedure::Splint, &[SurgeryItemRequirement::SplintEquipped], surgery_duration(SurgeryProcedure::Splint, surgery_skill, 1.0), 1.0,
                            surgery_skill, if fracture <= 0.0 { Some("No injury is present") } else { None }, if fracture <= 0.0 { Some("No injury is present") } else if surgery_skill < 1.0 { Some("Insufficient Surgery skill") } else if splints == 0 { Some("No splints") } else { None }, None, false, false, None))
                    }
                    @if cut <= 0.0 && bruise > 0.0 && fracture <= 0.0 {
                        p class="text-muted small-copy" { "Bruising must heal on its own." }
                    }
                    (surgery_procedure_row(
                        &action,
                        "Open the body",
                        "scalpel",
                        SurgeryProcedure::OpenBody,
                        &[SurgeryItemRequirement::SurgeryKitReusable],
                        surgery_duration(SurgeryProcedure::OpenBody, surgery_skill, 1.0),
                        1.0,
                        surgery_skill,
                        Some("Patient is alive"),
                        Some("This procedure is available only for dead subjects"),
                        None,
                        false,
                        false,
                        None,
                    ))
                }
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the corpse action form keeps independent authorization and action fields explicit"
)]
fn corpse_action_form(
    corpse: &BackendCorpse,
    location_base: &str,
    window: &str,
    action_kind: crate::medical::CorpseActionKind,
    discipline: &str,
    stage: &str,
    label: &str,
    disabled: Option<&str>,
) -> Markup {
    use crate::medical::CorpseActionKind;

    let unauthorized = match action_kind {
        CorpseActionKind::Burn => !corpse.penalty_free_burning,
        CorpseActionKind::Bury => false,
        CorpseActionKind::Exhume => !corpse.exhumation_permission,
        CorpseActionKind::Open | CorpseActionKind::Examine => corpse.permission == "none",
    };
    let destructive = action_kind == CorpseActionKind::Burn;
    let action_tag = action_kind.tag();
    let warning = if destructive && unauthorized {
        "Burning a victim cannot be authorized. It permanently destroys the body and evidence, severely harms family affinity, and brings severe settlement infamy."
    } else if destructive {
        "Burning this slain enemy carries no social penalty, but permanently destroys the body and all remaining evidence."
    } else {
        "No permission: proceeding is likely to seriously upset the family and bring substantial settlement infamy."
    };
    let confirmation = if destructive && unauthorized {
        "Burning this victim cannot be authorized. It will permanently destroy the body and evidence, severely harm family affinity, and bring severe settlement infamy. Proceed?"
    } else if destructive {
        "Burn this slain enemy? This will permanently destroy the body and all remaining evidence."
    } else {
        "You do not have permission. The family will be seriously upset and the settlement will regard this as infamous. Proceed?"
    };
    html! {
        form method="post"
            action=(format!("/corpses/{}/action", corpse.corpse_id))
            class=(if unauthorized { "surgery-procedure autopsy-action unauthorized-action" } else { "surgery-procedure autopsy-action" })
            data-strategic-tooltip=[(unauthorized || destructive).then_some(warning)]
            tabindex=[(unauthorized || destructive).then_some("0")]
            onsubmit=[(unauthorized || destructive).then_some(format!("return confirm('{confirmation}')"))] {
            input type="hidden" name="action_kind" value=(action_tag);
            input type="hidden" name="discipline" value=(discipline);
            input type="hidden" name="stage" value=(stage);
            input type="hidden" name="expected_revision" value=(corpse.revision);
            input type="hidden" name="confirm_unauthorized" value=(if unauthorized { "true" } else { "false" });
            input type="hidden" name="action_id" value=(format!("autopsy:{action_tag}:{discipline}:{stage}:{}", corpse.revision));
            input type="hidden" name="return_to" value=(format!("{location_base}?corpse={}&medical={window}", corpse.corpse_id));
            (game_icon(label, match action_kind {
                CorpseActionKind::Open => "scalpel",
                CorpseActionKind::Burn => "campfire",
                _ => "magnifying-glass",
            }))
            div class="surgery-procedure-copy" {
                strong { (label) }
                @if destructive && unauthorized {
                    small { " Cannot be authorized" }
                } @else if unauthorized {
                    small { " Permission missing" }
                }
            }
            button type="submit"
                class=(if unauthorized || destructive { "btn btn-danger" } else { "btn btn-primary" })
                disabled[disabled.is_some()]
                title=[disabled] {
                (label)
            }
        }
    }
}

/// Corpse examinations deliberately reuse the existing Physiology notebook and
/// Surgery procedure-window idioms; there is no third autopsy dialogue.
pub fn corpse_medical_dialog(corpse: &BackendCorpse, location_base: &str, window: &str) -> Markup {
    use crate::medical::CorpseActionKind;

    let close_href = location_base;
    let internal_disabled = (!corpse.opened).then_some("Open the body in Surgery first");
    let title = if window == "surgery" {
        "Surgery"
    } else {
        "Physiology"
    };
    html! {
        div class="character-action-overlay" data-character-action-dialog {
            a class="character-action-backdrop" href=(&close_href) aria-label=(format!("Close {title} window")) {}
            section class=(if window == "surgery" { "character-action-dialog surgery-dialog" } else { "character-action-dialog physiology-dialog" })
                data-physiology-dialog[window == "physiology"]
                role="dialog" aria-modal="true" aria-labelledby="corpse-medical-title" tabindex="-1" {
                header class="character-action-dialog-header" {
                    h2 id="corpse-medical-title" {
                        (if corpse.location == "interred" { "Buried body" } else { &corpse.display_name })
                        " — " (title)
                    }
                    a class="character-action-dialog-close" href=(&close_href) aria-label=(format!("Close {title} window")) { "×" }
                }
                @if corpse.location == "interred" {
                    p class="text-muted small-copy" {
                        "The body is buried. Exhume it to reveal its identity, condition, or recorded findings."
                    }
                } @else {
                    p class="text-muted small-copy" {
                        "Body: " (corpse.location.replace('_', " ")) "; decomposition: " (&corpse.decomposition) "."
                    }
                }
                div class="surgery-procedures" {
                    @if corpse.location == "interred" {
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Exhume, "surgery", "handling", "Exhume the body", None))
                    } @else {
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Bury, "surgery", "handling", "Bury the body", None))
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Burn, "surgery", "handling", "Burn the body", None))
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Examine, window, "external", "External examination", None))
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Examine, "bestiary", "external", "Interpret external creature signs", None))
                        @if window == "surgery" {
                            (corpse_action_form(
                                corpse,
                                location_base,
                                window,
                                CorpseActionKind::Open,
                                "surgery",
                                "opening",
                                "Open the body",
                                corpse.opened.then_some("The body is already open"),
                            ))
                        }
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Examine, window, "internal", "Internal examination", internal_disabled))
                        (corpse_action_form(corpse, location_base, window, CorpseActionKind::Examine, "bestiary", "internal", "Interpret internal creature signs", internal_disabled))
                    }
                }
                @if corpse.location != "interred" && !corpse.findings.is_empty() {
                    section class="physiology-chart-readings" aria-label="Recorded autopsy findings" {
                        h3 { "Observed findings" }
                        ul {
                            @for finding in &corpse.findings { li { (finding) } }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn strategic_condition_rail(
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    filth: &[crate::spacetimedb::CharacterFilth],
    social_href: &str,
    social_open: bool,
) -> Markup {
    let Some(condition) = condition else {
        return html! {};
    };
    let percent = |value: f32| format!("{:.0}%", value.max(0.0) * 100.0);
    let status = crate::spacetimedb::core_incapacitation_status(condition.status);
    let fear_fill = (condition.fear.clamp(0.0, 1.0) * 100.0).round();
    let bonus_fill = if condition.morale_bonus_cap > 0.0 {
        (condition.morale_bonus / condition.morale_bonus_cap * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
    .round();
    let resolved_morale =
        adventuresim_core::social::resolved_social_morale(morale_sources.iter().map(|source| {
            (
                crate::spacetimedb::core_morale_source_kind(source.kind),
                source.magnitude,
            )
        }));
    let resolved_fill = resolved_morale.clamp(0.0, 100.0).round();
    let meter_style = format!(
        "--morale-fear: {fear_fill}%; --morale-resolved: {resolved_fill}%; --morale-bonus: {bonus_fill}%"
    );
    let incapacitation_segments = [
        ("Pain", "broken-heart", "pain", condition.pain),
        (
            "Blood loss",
            "bleeding-wound",
            "blood",
            condition.blood_loss,
        ),
        ("Fear", "terror", "fear", condition.fear),
        ("Fatigue", "night-sleep", "fatigue", condition.fatigue),
        ("Hunger", "meal", "hunger", condition.hunger),
        ("Thirst", "water-drop", "thirst", condition.thirst),
        (
            "Temperature",
            "thermometer-cold",
            "thermal",
            condition.thermal,
        ),
    ];
    let incapacitation_sources = [
        ("Pain", "broken-heart", "pain", condition.pain),
        (
            "Blood loss",
            "bleeding-wound",
            "blood",
            condition.blood_loss,
        ),
        ("Fatigue", "night-sleep", "fatigue", condition.fatigue),
    ];
    html! {
        (sidebar_section("Status", html! {
            div class="fervor-meter" tabindex="0" style=(format!("--fervor: {:.0}%", condition.fervor.clamp(0.0, 1.0) * 100.0)) aria-label=(format!("Fervor {}", percent(condition.fervor))) {
                div class="fervor-meter-heading" {
                    strong class="metric-label" { (decorative_game_icon("holy-symbol")) span { "Fervor" } }
                    span { (percent(condition.fervor)) }
                }
                div class="fervor-meter-track" aria-hidden="true" { span {} }
                div class="fervor-meter-labels" {
                    span { "Calm" }
                    span { "Fervent" }
                    span { "Frenzy" }
                }
                p class="fervor-help" role="tooltip" {
                    "Personality Conviction, a strong same-profession cohort, and surplus morale raise Fervor. Party Command restrains it. Characters without a professed religion have no Fervor."
                }
            }
            div class="incapacitation-overview" tabindex="0" title=(format!("{} incapacitation", percent(condition.incapacitation))) {
                div class="incapacitation-heading" {
                    strong class="metric-label" { (decorative_game_icon("coma")) span { "Incapacitation" } }
                    span class="incapacitation-status" { (status) }
                }
                div class="incapacitation-total-track" role="meter"
                    aria-label=(format!("Incapacitation {}; {status}", percent(condition.incapacitation)))
                    aria-valuemin="0" aria-valuemax="100"
                    aria-valuenow=(condition.incapacitation.clamp(0.0, 1.0) * 100.0) {
                    @for (_, _, color, value) in incapacitation_segments {
                        span class=(format!("incapacitation-segment incapacitation-{color}"))
                            style=(format!("--incap-amount: {:.1}%", value.max(0.0) * 100.0)) {}
                    }
                }
            }
            div class="incapacitation-sources" aria-label="Sources of incapacitation" {
                div class=(if condition.fear > 0.0 { "morale-meter incapacitation-morale is-fearful" } else { "morale-meter incapacitation-morale" }) style=(meter_style) role="meter" aria-valuemin="-100" aria-valuemax="100" aria-valuenow=(format!("{:.1}", condition.morale)) title=(format!("{resolved_morale:.1} morale from successful social support currently offsets actionable concerns")) aria-label=(format!(
                    "Morale {:.1}; fear {}; {:.1} morale resolved by successful social support; inspiration {:.1}%",
                    condition.morale,
                    percent(condition.fear),
                    resolved_morale,
                    condition.morale_bonus * 100.0,
                )) {
                    div class="morale-meter-heading" {
                        strong class="metric-label" { (decorative_game_icon("sun")) span { "Morale" } }
                        span class="morale-meter-value" { (format!("{:+.1}", condition.morale)) }
                        a class=(if social_open { "character-menu-button is-open" } else { "character-menu-button" })
                            href=(social_href) title="Open Recent Tidings" aria-label="Open conversation to Recent Tidings"
                            aria-current=[social_open.then_some("page")] {
                            span class="stat-icon" style="--stat-icon: url('/static/icons/game/conversation.svg')" aria-hidden="true" {}
                            @if social_open { span class="sr-only" { " (open)" } }
                        }
                    }
                    div class="morale-meter-track" aria-hidden="true" {
                        span class="morale-meter-fear" { span class="morale-meter-resolved" {} }
                        span class="morale-meter-neutral" {}
                        span class="morale-meter-bonus" {}
                    }
                    div class="morale-meter-labels" {
                        span { "100% fear" }
                        span { "Neutral" }
                        span { (format!("{:.1}% inspiration", condition.morale_bonus * 100.0)) }
                    }
                }
                @for (label, icon, color, value) in incapacitation_sources {
                    div class=(format!("incapacitation-source incapacitation-{color}"))
                        title=(format!("{label}: {} incapacitation", percent(value))) {
                        strong class="metric-label" { (decorative_game_icon(icon)) span { (label) } }
                        div class="incapacitation-source-track" role="meter"
                            aria-label=(format!("{label}: {} incapacitation", percent(value)))
                            aria-valuemin="0" aria-valuemax="100"
                            aria-valuenow=(value.clamp(0.0, 1.0) * 100.0) {
                            span style=(format!("--incap-amount: {:.1}%", value.clamp(0.0, 1.0) * 100.0)) {}
                        }
                    }
                }
                (temperature_strain_meter(condition.thermal_strain))
            }
            div class="need-balance-meters" aria-label="Food and water reserves" {
                (need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", condition.food_days, condition.hunger))
                (need_balance_meter("Water", "water-drop", "Thirst", "Hydrated", "thirst", condition.water_days, condition.thirst))
            }
            (filth_status_bar(filth, condition.wetness_bps))
        }))
    }
}

fn temperature_strain_meter(strain: i32) -> Markup {
    let percent = (strain.clamp(
        -adventuresim_core::survival::MAX_THERMAL_STRAIN,
        adventuresim_core::survival::MAX_THERMAL_STRAIN,
    ) as f32
        / adventuresim_core::survival::MAX_THERMAL_STRAIN as f32
        * 100.0)
        .round() as i32;
    let label = if percent < 0 {
        format!("Cold strain {}%", percent.unsigned_abs())
    } else if percent > 0 {
        format!("Heat strain {percent}%")
    } else {
        "Temperature comfortable".into()
    };
    let cold_width = if percent < 0 {
        percent.unsigned_abs()
    } else {
        0
    };
    let hot_width = percent.max(0);
    html! {
        div class="temperature-strain incapacitation-source incapacitation-thermal" tabindex="0" title=(&label) {
            strong class="metric-label temperature-strain-label" {
                span class="temperature-condition-icon" aria-hidden="true" {
                    span class="temperature-condition-cold" {}
                    span class="temperature-condition-hot" {}
                }
                span { "Temperature" }
            }
            div class="temperature-strain-track" role="meter" aria-label=(&label)
                aria-valuemin="-100" aria-valuemax="100" aria-valuenow=(percent)
                style=(format!(
                    "--thermal-cold: {:.1}%; --thermal-hot: {:.1}%",
                    cold_width as f32 / 2.0,
                    hot_width as f32 / 2.0,
                )) {
                span class="temperature-strain-cold" aria-hidden="true" {}
                span class="temperature-strain-hot" aria-hidden="true" {}
                i aria-hidden="true" {}
            }
        }
    }
}

fn need_balance_meter(
    label: &str,
    icon: &str,
    deficit_label: &str,
    reserve_label: &str,
    color: &str,
    reserve_days: f32,
    incapacitation: f32,
) -> Markup {
    let reserve_days = reserve_days.max(0.0);
    let reserve_fill = (reserve_days * 100.0).clamp(0.0, 100.0);
    let deficit_fill = (incapacitation.max(0.0) * 100.0).clamp(0.0, 100.0);
    let signed_value = if deficit_fill > 0.0 {
        -deficit_fill
    } else {
        reserve_fill
    };
    let description = format!(
        "{label}: {reserve_days:.1} travel days reserve; {deficit_label} {deficit_fill:.0}% incapacitation"
    );
    html! {
        div class=(format!("need-balance incapacitation-{color}"))
            style=(format!("--need-reserve: {reserve_fill:.1}%; --need-deficit: {deficit_fill:.1}%"))
            title=(&description) {
            strong class="metric-label" { (decorative_game_icon(icon)) span { (label) } }
            div class="need-balance-track" role="meter" aria-label=(description)
                aria-valuemin="-100" aria-valuemax="100" aria-valuenow=(format!("{signed_value:.0}")) {
                span class="need-balance-half need-balance-deficit" { span {} }
                span class="need-balance-half need-balance-reserve" { span {} }
                i aria-hidden="true" {}
            }
            div class="need-balance-labels" aria-hidden="true" {
                span { (deficit_label) }
                span { "0" }
                span { (reserve_label) }
            }
        }
    }
}

fn physiology_series_paths(
    readings: &[ChartReadingPresentation],
    gaps: &[ChartGapPresentation],
    region_index: usize,
    humour_index: usize,
) -> Vec<String> {
    if readings.is_empty() {
        return Vec::new();
    }
    let first_minute = readings.first().map_or(0, |reading| reading.minute);
    let last_minute = readings
        .last()
        .map_or(first_minute, |reading| reading.minute);
    let mut paths = vec![String::new()];

    for (index, reading) in readings.iter().enumerate() {
        let x = physiology_deviation_x(reading.humour_deviations_bps[region_index][humour_index]);
        let y = physiology_time_y(reading.minute, first_minute, last_minute);
        let begins_after_gap = index > 0
            && gaps
                .iter()
                .any(|gap| gap.from < reading.minute && gap.to > readings[index - 1].minute);
        if begins_after_gap {
            paths.push(String::new());
        }
        let path = paths.last_mut().expect("at least one trend path");
        if path.is_empty() {
            path.push_str(&format!("M {x:.1},{y:.1} l 0.01,0 "));
        } else {
            path.push_str(&format!("L {x:.1},{y:.1} "));
        }
    }
    paths
}

fn physiology_relative_day_label(minute: u64, today: u64) -> String {
    let days_ago =
        today.saturating_sub(minute) / adventuresim_core::strategic_time::MINUTES_PER_DAY;
    match days_ago {
        0 => "Today".to_owned(),
        1 => "1 day ago".to_owned(),
        days => format!("{days} days ago"),
    }
}

fn physiology_time_y(minute: u64, first_minute: u64, last_minute: u64) -> f32 {
    let duration = last_minute.saturating_sub(first_minute);
    if duration == 0 {
        50.0
    } else {
        4.0 + minute.saturating_sub(first_minute).min(duration) as f32 * 92.0 / duration as f32
    }
}

fn physiology_deviation_x(value_bps: i16) -> f32 {
    // The notebook is an observational instrument, not a full failure-range
    // meter. Expand its common ±35% window so ordinary daily wobble and
    // treatment response remain legible; accessible labels retain exact values.
    22.0 + (value_bps as f32 / 3_500.0).clamp(-1.0, 1.0) * 18.0
}

fn physiology_reading_bar(label: &str, short: &str, tone: &str, value_bps: i16) -> Markup {
    let percent = value_bps / 100;
    let magnitude = percent.unsigned_abs().min(100);
    let start = if percent < 0 {
        50.0 - magnitude as f32 / 2.0
    } else {
        50.0
    };
    html! {
        div class=(format!("physiology-reading-bar physiology-tone-{tone}")) {
            span class="physiology-reading-bar-label" aria-hidden="true" { (short) }
            span class="physiology-reading-track" role="meter"
                aria-label=(format!("{label} deviation {percent:+}% from baseline"))
                aria-valuemin="-100" aria-valuemax="100" aria-valuenow=(percent) {
                i aria-hidden="true" {}
                span style=(format!(
                    "--reading-start: {start:.1}%; --reading-width: {:.1}%",
                    magnitude as f32 / 2.0
                )) {}
            }
            output aria-hidden="true" { (format!("{percent:+}%")) }
        }
    }
}

fn physiology_likelihood(
    candidate: &crate::medical::DiseaseLikelihoodPresentation,
    tooltip_id: &str,
) -> Markup {
    let percent = candidate.likelihood_bps / 100;
    let hue = candidate.likelihood_bps as f32 * 120.0
        / f32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE);
    html! {
        li class="physiology-disease-likelihood"
            style=(format!("--likelihood-hue: {hue:.1}deg"))
            tabindex="0"
            aria-describedby=(tooltip_id)
            aria-label=(format!("{}: estimated likelihood {percent}%", candidate.label)) {
            span { (candidate.label) }
            aside id=(tooltip_id) class="physiology-disease-effects" role="tooltip" {
                strong { "Typical humour pattern" }
                @if candidate.typical_effects.is_empty() {
                    span { "No characteristic pattern recorded." }
                } @else {
                    ul {
                        @for effect in &candidate.typical_effects {
                            li { (effect) }
                        }
                    }
                }
                small aria-hidden="true" { (format!("Estimated likelihood: {percent}%")) }
            }
        }
    }
}

fn physiology_reading_snapshot(
    reading: &ChartReadingPresentation,
    region_index: usize,
    region_label: &str,
    today: u64,
) -> Markup {
    let values = reading.humour_deviations_bps[region_index];
    html! {
        header {
            div {
                strong { (physiology_relative_day_label(reading.minute, today)) }
                span class="physiology-region-chip" { (region_label) }
            }
            div class="physiology-confidence"
                title=(format!("Observation confidence {}%", reading.confidence_bps / 100)) {
                span aria-hidden="true" { "confidence" }
                span class="physiology-confidence-track" role="meter"
                    aria-label=(format!("Observation confidence {}%", reading.confidence_bps / 100))
                    aria-valuemin="0" aria-valuemax="100"
                    aria-valuenow=(reading.confidence_bps / 100) {
                    i style=(format!("--confidence: {}%", reading.confidence_bps / 100)) {}
                }
            }
        }
        div class="physiology-reading-bars" {
            (physiology_reading_bar("Sanguine", "S", "sanguine", values[0]))
            (physiology_reading_bar("Phlegmatic", "P", "phlegmatic", values[1]))
            (physiology_reading_bar("Choleric", "C", "choleric", values[2]))
            (physiology_reading_bar("Melancholic", "M", "melancholic", values[3]))
        }
        @if !reading.known_interventions.is_empty() {
            ul class="physiology-chip-list physiology-interventions" aria-label="Known preparations" {
                @for intervention in &reading.known_interventions {
                    li { span aria-hidden="true" { "+" } (intervention.replace('_', " ")) }
                }
            }
        }
    }
}

fn physiology_reading_aria_label(
    reading: &ChartReadingPresentation,
    region_index: usize,
    region_label: &str,
    today: u64,
) -> String {
    let values = reading.humour_deviations_bps[region_index];
    format!(
        "{} {} observation: Sanguine {:+}%, Phlegmatic {:+}%, Choleric {:+}%, Melancholic {:+}% from baseline; confidence {}%. Hover, focus, or select for details.",
        physiology_relative_day_label(reading.minute, today),
        region_label,
        values[0] / 100,
        values[1] / 100,
        values[2] / 100,
        values[3] / 100,
        reading.confidence_bps / 100,
    )
}

pub(super) fn physiology_dialog(
    medical: &MedicalPresentation,
    dialog_id: &str,
    patient_name: &str,
) -> Markup {
    html! {
        dialog id=(dialog_id) class="physiology-dialog" data-physiology-dialog
            aria-labelledby="physiology-dialog-title" {
            div class="physiology-dialog-shell" {
                header class="physiology-dialog-header" {
                    div {
                        span class="physiology-dialog-kicker" { "Physician notebook" }
                        h2 id="physiology-dialog-title" { (patient_name) }
                    }
                    button type="button" class="physiology-dialog-close"
                        aria-label="Close physician notebook" data-physiology-dialog-close { "×" }
                }
                div class="physiology-dialog-body" {
                    @if medical.unavailable {
                        p class="physiology-empty-state" { "Authorized Physiology chart unavailable." }
                    } @else if medical.readings.is_empty() {
                        p class="physiology-empty-state" { "No authorized shared-presence readings." }
                    } @else {
                        @let first_minute = medical.readings.first().map_or(0, |reading| reading.minute);
                        @let last_minute = medical.readings.last().map_or(first_minute, |reading| reading.minute);
                        @let latest = medical.readings.last().expect("nonempty chart");
                        section class="physiology-trend-panel" aria-labelledby="physiology-trend-title" {
                            div class="physiology-section-heading" {
                                div {
                                    span class="physiology-eyebrow" { "Over time" }
                                    h3 id="physiology-trend-title" { "Humours by region" }
                                }
                                ul class="physiology-trend-legend" aria-label="Humour colours" {
                                    @for (name, short, tone) in [
                                        ("Sanguine", "S", "sanguine"),
                                        ("Phlegmatic", "P", "phlegmatic"),
                                        ("Choleric", "C", "choleric"),
                                        ("Melancholic", "M", "melancholic"),
                                    ] {
                                        li class=(format!("physiology-tone-{tone}")) {
                                            i aria-hidden="true" {}
                                            span class="sr-only" { (name) }
                                            span aria-hidden="true" { (short) }
                                        }
                                    }
                                }
                            }
                            div class="physiology-differential" {
                                div {
                                    strong { "Possible diseases" }
                                    span { "Colour confidence improves with skill and observation." }
                                }
                                ul aria-label="Possible diseases ordered by estimated likelihood" {
                                    @for (candidate_index, candidate) in latest.possible_diseases.iter().enumerate() {
                                        @let tooltip_id = format!(
                                            "{dialog_id}-disease-effects-{candidate_index}"
                                        );
                                        (physiology_likelihood(candidate, &tooltip_id))
                                    }
                                }
                            }
                            ul class="physiology-trend-annotation-key" aria-label="Timeline annotations" {
                                li { i class="physiology-baseline-key" aria-hidden="true" {} "Healthy baseline" }
                                li { i class="physiology-event-key physiology-event-start" aria-hidden="true" {} "Medication start" }
                                li { i class="physiology-event-key physiology-event-stop" aria-hidden="true" {} "Medication stop" }
                                li { i class="physiology-gap-key" aria-hidden="true" {} "Not in party" }
                            }
                            div class="physiology-region-charts" {
                                aside class="physiology-timeline-labels"
                                    aria-label="Observation timeline labels" {
                                    span class="physiology-timeline-label physiology-timeline-boundary"
                                        style="--time-y: 4%" {
                                        strong { "Start" }
                                        small {
                                            (physiology_relative_day_label(first_minute, last_minute))
                                        }
                                    }
                                    @for gap in &medical.gaps {
                                        @let gap_start = gap.from.clamp(first_minute, last_minute);
                                        @let gap_end = gap.to.clamp(first_minute, last_minute);
                                        @if gap_end > gap_start {
                                            @let gap_midpoint = gap_start + (gap_end - gap_start) / 2;
                                            @let gap_y = physiology_time_y(
                                                gap_midpoint,
                                                first_minute,
                                                last_minute,
                                            );
                                            span class="physiology-timeline-label physiology-gap-label"
                                                style=(format!("--time-y: {gap_y:.2}%")) {
                                                strong { "Not in party" }
                                                small {
                                                    (physiology_relative_day_label(gap.from, last_minute))
                                                    "–"
                                                    (physiology_relative_day_label(gap.to, last_minute))
                                                }
                                            }
                                        }
                                    }
                                    @for administration in &medical.administrations {
                                        @if administration.administered_at >= first_minute && administration.administered_at <= last_minute {
                                            @let event_y = physiology_time_y(
                                                administration.administered_at,
                                                first_minute,
                                                last_minute,
                                            );
                                            span class="physiology-timeline-label physiology-start-label"
                                                style=(format!("--time-y: {event_y:.2}%")) {
                                                strong {
                                                    (administration.preparation_id.replace('_', " "))
                                                    " starts"
                                                }
                                                small {
                                                    (physiology_relative_day_label(
                                                        administration.administered_at,
                                                        last_minute,
                                                    ))
                                                }
                                            }
                                        }
                                        @if let Some(stopped_at) = administration.stopped_at {
                                            @if stopped_at >= first_minute && stopped_at <= last_minute {
                                                @let event_y = physiology_time_y(
                                                    stopped_at,
                                                    first_minute,
                                                    last_minute,
                                                );
                                                span class="physiology-timeline-label physiology-stop-label"
                                                    style=(format!("--time-y: {event_y:.2}%")) {
                                                    strong {
                                                        (administration.preparation_id.replace('_', " "))
                                                        " stops"
                                                    }
                                                    small {
                                                        (physiology_relative_day_label(
                                                            stopped_at,
                                                            last_minute,
                                                        ))
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    span class="physiology-timeline-label physiology-timeline-boundary"
                                        style="--time-y: 96%" {
                                        strong { "Today" }
                                    }
                                }
                                @for gap in &medical.gaps {
                                    @let gap_start = gap.from.clamp(first_minute, last_minute);
                                    @let gap_end = gap.to.clamp(first_minute, last_minute);
                                    @if gap_end > gap_start {
                                        @let gap_y = physiology_time_y(gap_start, first_minute, last_minute);
                                        @let gap_height = physiology_time_y(
                                            gap_end,
                                            first_minute,
                                            last_minute,
                                        ) - gap_y;
                                        div class="physiology-shared-gap" role="img"
                                            style=(format!(
                                                "--gap-y: {gap_y:.2}%; --gap-height: {gap_height:.2}%"
                                            ))
                                            aria-label=(format!(
                                                "Not in party from {} to {}",
                                                physiology_relative_day_label(gap.from, last_minute),
                                                physiology_relative_day_label(gap.to, last_minute),
                                            )) {}
                                    }
                                }
                                @for administration in &medical.administrations {
                                    @if administration.administered_at >= first_minute && administration.administered_at <= last_minute {
                                        @let event_y = physiology_time_y(
                                            administration.administered_at,
                                            first_minute,
                                            last_minute,
                                        );
                                        div class="physiology-shared-event physiology-treatment-start"
                                            style=(format!("--time-y: {event_y:.2}%"))
                                            role="img"
                                            aria-label=(format!(
                                                "{} started {}",
                                                administration.preparation_id.replace('_', " "),
                                                physiology_relative_day_label(
                                                    administration.administered_at,
                                                    last_minute,
                                                ),
                                            )) {}
                                    }
                                    @if let Some(stopped_at) = administration.stopped_at {
                                        @if stopped_at >= first_minute && stopped_at <= last_minute {
                                            @let event_y = physiology_time_y(
                                                stopped_at,
                                                first_minute,
                                                last_minute,
                                            );
                                            div class="physiology-shared-event physiology-treatment-stop"
                                                style=(format!("--time-y: {event_y:.2}%"))
                                                role="img"
                                                aria-label=(format!(
                                                    "{} stopped {}",
                                                    administration.preparation_id.replace('_', " "),
                                                    physiology_relative_day_label(
                                                        stopped_at,
                                                        last_minute,
                                                    ),
                                                )) {}
                                        }
                                    }
                                }
                                @for (region_index, region_label) in [
                                    (6, "Head"), (4, "Chest"), (5, "Stomach"),
                                    (0, "Left arm"), (1, "Right arm"),
                                    (2, "Left leg"), (3, "Right leg"),
                                ] {
                                    @let hit_height = if medical.readings.len() > 1 {
                                        (92.0 / (medical.readings.len() - 1) as f32 * 0.78).clamp(0.28, 2.4)
                                    } else {
                                        2.4
                                    };
                                    figure class="physiology-region-chart" {
                                        h4 class="physiology-region-heading" { (region_label) }
                                        svg viewBox="0 0 44 100" preserveAspectRatio="none" role="img"
                                            aria-label=(format!(
                                                "{region_label} humour deviations from healthy baseline over time"
                                            )) {
                                            line class="physiology-trend-baseline"
                                                x1="22" y1="4" x2="22" y2="96" {}
                                            @for (humour_index, tone) in [
                                                "sanguine", "phlegmatic", "choleric", "melancholic",
                                            ].into_iter().enumerate() {
                                                @for path_data in physiology_series_paths(
                                                    &medical.readings,
                                                    &medical.gaps,
                                                    region_index,
                                                    humour_index,
                                                ) {
                                                    path class=(format!("physiology-trend-line physiology-tone-{tone}"))
                                                        d=(path_data) {}
                                                }
                                            }
                                            @for (reading_index, reading) in medical.readings.iter().enumerate() {
                                                @let reading_y = physiology_time_y(reading.minute, first_minute, last_minute);
                                                @let tooltip_id = format!("{dialog_id}-reading-{region_index}-{reading_index}");
                                                g class="physiology-trend-point" {
                                                    rect class="physiology-trend-point-hit"
                                                        x="4"
                                                        y=(format!("{:.2}", (reading_y - hit_height / 2.0).clamp(4.0, 96.0 - hit_height)))
                                                        width="36" height=(format!("{hit_height:.2}"))
                                                        tabindex="0" role="button"
                                                        aria-label=(physiology_reading_aria_label(
                                                            reading,
                                                            region_index,
                                                            region_label,
                                                            last_minute,
                                                        ))
                                                        aria-controls=(&tooltip_id) aria-expanded="false"
                                                        data-physiology-reading-point
                                                        data-physiology-tooltip-id=(&tooltip_id) {}
                                                    line class="physiology-trend-point-guide"
                                                        x1="4" y1=(format!("{reading_y:.2}"))
                                                        x2="40" y2=(format!("{reading_y:.2}")) {}
                                                    @for (humour_index, tone) in [
                                                        "sanguine", "phlegmatic", "choleric", "melancholic",
                                                    ].into_iter().enumerate() {
                                                        @let reading_x = physiology_deviation_x(
                                                            reading.humour_deviations_bps[region_index][humour_index],
                                                        );
                                                        line class=(format!(
                                                            "physiology-trend-point-mark physiology-tone-{tone}"
                                                        ))
                                                            x1=(format!("{reading_x:.2}"))
                                                            y1=(format!("{:.2}", reading_y - 0.45))
                                                            x2=(format!("{reading_x:.2}"))
                                                            y2=(format!("{:.2}", reading_y + 0.45)) {}
                                                    }
                                                }
                                            }
                                        }
                                        @for (reading_index, reading) in medical.readings.iter().enumerate() {
                                            article id=(format!(
                                                "{dialog_id}-reading-{region_index}-{reading_index}"
                                            ))
                                                class="physiology-reading-tooltip"
                                                data-physiology-reading-tooltip hidden {
                                                (physiology_reading_snapshot(
                                                    reading,
                                                    region_index,
                                                    region_label,
                                                    last_minute,
                                                ))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    footer class="physiology-dialog-footer" {
                        span {
                            "Visible findings are folded into the four humours; deviations are observer estimates."
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn physiology_controls(medical: &MedicalPresentation, action_base: &str) -> Markup {
    html! {
        @if !medical.active_administrations.is_empty() {
        (sidebar_section("Current medication", html! {
            @for administration in &medical.active_administrations {
                form method="post"
                    action=(format!("{action_base}/physiology/{}/stop", administration.id)) {
                    span { (&administration.display_name) }
                    button type="submit" class="btn btn-small" { "Stop" }
                }
            }
        }))
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the health rail renders independently optional character physiology projections"
)]
pub(super) fn party_attributes_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    limbs: Option<&CharacterLimbs>,
    medical: &MedicalPresentation,
    physiology_dialog_id: Option<&str>,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
) -> Markup {
    let Some(attributes) = attributes else {
        return html! {};
    };
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let chest_health = limbs.map_or(1.0, |limbs| limbs.chest_health);
    let stomach_health = limbs.map_or(1.0, |limbs| limbs.stomach_health);
    let left_arm_health = limbs.map_or(1.0, |limbs| limbs.left_arm_health);
    let right_arm_health = limbs.map_or(1.0, |limbs| limbs.right_arm_health);
    let left_leg_health = limbs.map_or(1.0, |limbs| limbs.left_leg_health);
    let right_leg_health = limbs.map_or(1.0, |limbs| limbs.right_leg_health);
    html! {
        div class="sidebar-section party-attributes-section" {
            header class="party-attributes-heading" {
                h3 class="sidebar-header" { (title) }
                @if let Some(dialog_id) = physiology_dialog_id {
                    button type="button"
                        class="character-menu-button physiology-dialog-button physiology-attributes-button"
                        title="Open physician notebook"
                        aria-label="Open physician notebook"
                        aria-haspopup="dialog"
                        aria-controls=(dialog_id)
                        aria-expanded="false"
                        data-physiology-dialog-open=(dialog_id) {
                        span class="physician-notebook-icon" aria-hidden="true" { "☤" }
                    }
                }
            }
            div class="party-attributes-list" aria-label="Character attributes" {
                (attribute_group("Head", "head", head_health, medical, 6, surgery, injuries, projectiles, &[
                    ("Intelligence", "intelligence", attributes.intelligence),
                    ("Instinct", "instinct", attributes.instinct),
                    ("Eyesight", "eyesight", attributes.eyesight),
                    ("Hearing", "hearing", attributes.hearing),
                ]))
                (attribute_group("Chest", "chest", chest_health, medical, 4, surgery, injuries, projectiles, &[
                    ("Endurance", "endurance", attributes.endurance),
                ]))
                (attribute_group("Stomach", "abdomen", stomach_health, medical, 5, surgery, injuries, projectiles, &[
                    ("Immunity", "immunity", attributes.immunity),
                    ("Gut", "gut", attributes.gut),
                ]))
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left arm", "left-arm", "limb-left", left_arm_health, medical, 0, surgery, injuries, projectiles, &[
                        ("Strength", "strength-arm", attributes.left_arm_strength),
                        ("Agility", "agility-arm", attributes.left_arm_agility),
                    ]))
                    (limb_attribute_column("Right arm", "right-arm", "limb-right", right_arm_health, medical, 1, surgery, injuries, projectiles, &[
                        ("Strength", "strength-arm", attributes.right_arm_strength),
                        ("Agility", "agility-arm", attributes.right_arm_agility),
                    ]))
                }
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left leg", "left-leg", "limb-left", left_leg_health, medical, 2, surgery, injuries, projectiles, &[
                        ("Strength", "strength-leg", attributes.left_leg_strength),
                        ("Agility", "agility-leg", attributes.left_leg_agility),
                    ]))
                    (limb_attribute_column("Right leg", "right-leg", "limb-right", right_leg_health, medical, 3, surgery, injuries, projectiles, &[
                        ("Strength", "strength-leg", attributes.right_leg_strength),
                        ("Agility", "agility-leg", attributes.right_leg_agility),
                    ]))
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the limb column mirrors independent health, treatment, and diagnostic presentation fields"
)]
fn limb_attribute_column(
    name: &str,
    slug: &str,
    side: &str,
    health: f32,
    medical: &MedicalPresentation,
    region: usize,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(
        name,
        slug,
        health,
        medical,
        region,
        surgery,
        injuries,
        projectiles,
        rows,
        false,
        Some(side),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the attribute group mirrors independent health and treatment presentation fields"
)]
fn attribute_group(
    name: &str,
    slug: &str,
    health: f32,
    medical: &MedicalPresentation,
    region: usize,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(
        name,
        slug,
        health,
        medical,
        region,
        surgery,
        injuries,
        projectiles,
        rows,
        true,
        None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the labelled attribute group is the final Maud boundary for independent presentation fields"
)]
fn attribute_group_with_labels(
    name: &str,
    slug: &str,
    health: f32,
    medical: &MedicalPresentation,
    region: usize,
    surgery: Option<(&str, Option<&str>)>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    rows: &[(&str, &str, f32)],
    show_labels: bool,
    side: Option<&str>,
) -> Markup {
    let health = health.clamp(0.0, 1.0);
    html! {
        div class=(match side {
            Some(side) => format!("attribute-group limb-attribute-column {side}"),
            None => "attribute-group".to_owned(),
        }) {
            div class="attribute-group-heading" {
                span { (name) }
                @if let Some((path_template, open_limb)) = surgery {
                    @let open = open_limb == Some(slug);
                    a class=(if open { "character-menu-button limb-surgery-button is-open" } else { "character-menu-button limb-surgery-button" })
                        href=(path_template.replace("__limb__", slug)) title=(format!("Treat {name}")) aria-label=(format!("Open surgery menu for {name}"))
                        aria-haspopup="dialog" aria-expanded=(open) {
                        span class="stat-icon" style="--stat-icon: url('/static/icons/game/scalpel.svg')" aria-hidden="true" {}
                        @if open { span class="sr-only" { " (open)" } }
                    }
                }
            }
            (regional_health_bar(name, health, medical, region, injuries, projectiles))
            @for (attribute_name, icon, value) in rows {
                (attribute_row(attribute_name, icon, *value, health, show_labels))
            }
        }
    }
}

fn regional_health_bar(
    name: &str,
    physical_health: f32,
    medical: &MedicalPresentation,
    region: usize,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
) -> Markup {
    let physical_health = physical_health.clamp(0.0, 1.0);
    let physical_damage = 1.0 - physical_health;
    let limb = [
        BodyRegion::LeftArm,
        BodyRegion::RightArm,
        BodyRegion::LeftLeg,
        BodyRegion::RightLeg,
        BodyRegion::Chest,
        BodyRegion::Abdomen,
        BodyRegion::Head,
    ][region];
    let injury = injuries
        .iter()
        .find(|injury| crate::spacetimedb::core_body_region(injury.limb) == limb);
    let cut = injury
        .map_or(0.0, |row| row.cut_damage)
        .min(physical_damage);
    let frostbite = injury
        .map_or(0.0, |row| row.frostbite_damage)
        .min((physical_damage - cut).max(0.0));
    let total_blunt = injury
        .map_or(physical_damage - cut - frostbite, |row| {
            row.bruise_damage.max(row.fracture_damage)
        })
        .min((physical_damage - cut - frostbite).max(0.0));
    let fracture = injury
        .map_or(0.0, |row| row.fracture_damage)
        .min(total_blunt);
    let blunt = (total_blunt - fracture).max(0.0);
    let bandaged = injury.is_some_and(|row| row.bandaged);
    let splinted = injury.is_some_and(|row| row.splint_inventory_item_id.is_some());
    let fracture_label = if splinted {
        "splinted fracture"
    } else {
        "fracture"
    };
    let humour = medical.regional_humours.map(|values| values[region]);
    let values = humour.unwrap_or_default();
    let humour_total = if humour.is_some() {
        values.sanguine.abs()
            + values.phlegmatic.abs()
            + values.choleric.abs()
            + values.melancholic.abs()
    } else {
        medical.concealed_other[region]
    };
    let other = physical_health * humour_total.clamp(0.0, 1.0);
    let okay = (physical_health - other).max(0.0);
    let scale = if humour.is_some() && humour_total > 1.0 {
        other / humour_total
    } else {
        physical_health
    };
    let segments = if humour.is_some() {
        vec![
            (
                adventuresim_core::physiology::Humour::Sanguine,
                "attribute-health-sanguine",
                values.sanguine.abs() * scale,
            ),
            (
                adventuresim_core::physiology::Humour::Phlegmatic,
                "attribute-health-phlegmatic",
                values.phlegmatic.abs() * scale,
            ),
            (
                adventuresim_core::physiology::Humour::Choleric,
                "attribute-health-choleric",
                values.choleric.abs() * scale,
            ),
            (
                adventuresim_core::physiology::Humour::Melancholic,
                "attribute-health-melancholic",
                values.melancholic.abs() * scale,
            ),
        ]
    } else {
        Vec::new()
    };
    let reading = if humour.is_some() {
        format!(
            "{name}: {:.0}% sound, {:.0}% cut, {:.0}% frostbite, {:.0}% blunt, {:.0}% {fracture_label}, {:.0}% sanguine, {:.0}% phlegmatic, {:.0}% choleric, {:.0}% melancholic impairment",
            okay * 100.0,
            cut * 100.0,
            frostbite * 100.0,
            blunt * 100.0,
            fracture * 100.0,
            values.sanguine.abs() * scale * 100.0,
            values.phlegmatic.abs() * scale * 100.0,
            values.choleric.abs() * scale * 100.0,
            values.melancholic.abs() * scale * 100.0,
        )
    } else {
        format!(
            "{name}: {:.0}% sound, {:.0}% cut, {:.0}% frostbite, {:.0}% blunt, {:.0}% {fracture_label}, {:.0}% other impairment",
            okay * 100.0,
            cut * 100.0,
            frostbite * 100.0,
            blunt * 100.0,
            fracture * 100.0,
            other * 100.0,
        )
    };
    html! {
        div class="attribute-health-bar" role="meter"
            aria-label=(reading)
            aria-valuemin="0" aria-valuemax="100" aria-valuenow=(okay * 100.0) {
            span class="attribute-health-current" title="Sound" style=(format!("width:{:.1}%", okay * 100.0)) {}
            span class=(if bandaged { "attribute-health-cut bandaged-cut" } else { "attribute-health-cut" }) title=(if bandaged { "Bandaged cut damage" } else { "Cut damage" }) style=(format!("width:{:.1}%", cut * 100.0)) {}
            span class="attribute-health-frostbite" title="Frostbite damage" style=(format!("width:{:.1}%", frostbite * 100.0)) {}
            span class="attribute-health-blunt" title="Blunt damage" style=(format!("width:{:.1}%", blunt * 100.0)) {}
            span class=(if splinted { "attribute-health-fracture splinted-fracture" } else { "attribute-health-fracture" })
                title=(if splinted { "Splinted fracture" } else { "Fracture" })
                style=(format!("width:{:.1}%", fracture * 100.0)) {}
            @if humour.is_none() && other > 0.0 {
                span class="attribute-health-other" title="Other impairment"
                    style=(format!("width:{:.1}%", other * 100.0)) {}
            }
            @for (humour, class, amount) in segments {
                @if amount > 0.0 {
                    @let disclosure = adventuresim_core::physiology::humour_disclosure(humour);
                    span class=(class)
                        title=(&disclosure)
                        data-strategic-tooltip=(&disclosure)
                        tabindex="0"
                        aria-label=(format!(
                            "{}: {:.0}% of this region. {}",
                            humour.public_name(),
                            amount * 100.0,
                            disclosure,
                        ))
                        style=(format!("width:{:.1}%", amount * 100.0)) {}
                }
            }
            @for (projectile_index, projectile) in projectiles.iter().filter(|projectile| crate::spacetimedb::core_body_region(projectile.limb) == limb).enumerate() {
                span class=(match projectile.kind { ProjectileKind::Arrowhead => "surgery-projectile-icon projectile-arrowhead", ProjectileKind::Ball => "surgery-projectile-icon projectile-ball" })
                    style=(format!("right:{:.2}rem", 0.2 + projectile_index as f32 * 0.75))
                    title=(match projectile.kind { ProjectileKind::Arrowhead => "Retained arrowhead", ProjectileKind::Ball => "Retained ball" }) aria-hidden="true" {}
            }
        }
    }
}

fn attribute_row(name: &str, icon: &str, value: f32, health: f32, show_label: bool) -> Markup {
    let effective_value = value * health.clamp(0.0, 1.0);
    let current_width = (effective_value.clamp(0.0, 5.0) / 5.0) * 100.0;
    let damage_width = ((value - effective_value).max(0.0) / 5.0) * 100.0;
    html! {
        div class=(if show_label { "party-attribute-row" } else { "party-attribute-row party-attribute-icon-only" }) {
            (stat_icon(name, "attributes", icon, show_label))
            @if show_label { span class="party-attribute-name" { (name) } }
            div class="attribute-rank-bar"
                data-strategic-tooltip=(format!("{name}: {effective_value:.1} out of 5"))
                tabindex="0"
                role="meter" aria-valuemin="0" aria-valuemax="5" aria-valuenow=(format!("{effective_value:.1}"))
                aria-label=(format!("{name}: {effective_value:.1} out of 5")) {
                span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
            }
        }
    }
}

pub(super) fn stat_icon(label: &str, category: &str, icon: &str, decorative: bool) -> Markup {
    let path = stat_icon_path(category, icon);
    html! {
        span
            class=(format!("stat-icon stat-icon-{icon}"))
            style=(format!("--stat-icon: url('{path}')"))
            role=[(!decorative).then_some("img")]
            aria-label=[(!decorative).then_some(label)]
            title=[(!decorative).then_some(label)]
            aria-hidden=[decorative.then_some("true")]
        {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;

    fn corpse_fixture(permission: &str, opened: bool) -> BackendCorpse {
        BackendCorpse {
            owner_character_id: 7,
            corpse_id: "corpse:battle:1".into(),
            display_name: "Fallen kobold".into(),
            creature_kind: "kobold".into(),
            source_id: "battle:1".into(),
            location: "scene".into(),
            decomposition: "fresh".into(),
            case_site_id: Some(adventuresim_stdb_client::CaseSiteId {
                value: "site:1".into(),
            }),
            settlement_id: "town".into(),
            opened,
            permission: permission.into(),
            exhumation_permission: permission != "none",
            penalty_free_burning: false,
            revision: u32::from(opened),
            findings: Vec::new(),
        }
    }

    #[test]
    fn corpse_uses_existing_medical_windows_and_warns_before_unauthorized_actions() {
        let physiology = corpse_medical_dialog(
            &corpse_fixture("none", false),
            "/locations/case-site/site:1/enemy",
            "physiology",
        )
        .into_string();
        assert!(physiology.contains("physiology-dialog"));
        assert!(physiology.contains("btn btn-danger"));
        assert!(physiology.contains("No permission"));
        assert!(physiology.contains("return confirm"));
        assert!(physiology.contains("Open the body in Surgery first"));

        let surgery = corpse_medical_dialog(
            &corpse_fixture("family", true),
            "/locations/case-site/site:1/enemy",
            "surgery",
        )
        .into_string();
        assert!(surgery.contains("surgery-dialog"));
        assert!(surgery.contains("Open the body"));
        assert!(surgery.contains("Burning a victim cannot be authorized"));
        assert!(surgery.contains("severely harms family affinity"));
        assert!(surgery.contains("Cannot be authorized"));
        assert!(surgery.find("Bury the body").unwrap() < surgery.find("Burn the body").unwrap());
    }

    #[test]
    fn interred_corpse_hides_details_and_requires_exhumation() {
        let mut corpse = corpse_fixture("family", false);
        corpse.location = "interred".into();
        corpse.exhumation_permission = false;
        corpse.display_name = "Secret victim identity".into();
        corpse.creature_kind = "secret creature".into();
        corpse.decomposition = "secret decomposition".into();
        corpse.findings = vec!["Secret prior finding".into()];

        let markup =
            corpse_medical_dialog(&corpse, "/locations/settlement/town", "surgery").into_string();

        assert!(markup.contains("Buried body"));
        assert!(markup.contains("Exhume it to reveal"));
        assert!(markup.contains("Exhume the body"));
        assert!(!markup.contains("Secret victim identity"));
        assert!(!markup.contains("secret creature"));
        assert!(!markup.contains("secret decomposition"));
        assert!(!markup.contains("Secret prior finding"));
        assert!(!markup.contains("Bury the body"));
        assert!(!markup.contains("Burn the body"));
        assert!(!markup.contains("External examination"));
        assert!(!markup.contains("Open the body"));
        assert!(!markup.contains("Internal examination"));
    }

    #[test]
    fn corpse_burning_for_a_party_slain_enemy_warns_only_about_irreversible_evidence_loss() {
        let mut corpse = corpse_fixture("none", false);
        corpse.penalty_free_burning = true;

        let markup =
            corpse_medical_dialog(&corpse, "/locations/case-site/site:1/enemy", "physiology")
                .into_string();

        assert!(markup.contains("Burn this slain enemy"));
        assert!(markup.contains("no social penalty"));
        assert!(!markup.contains("Burning a victim cannot be authorized"));
    }

    #[test]
    fn public_filth_serialization_and_template_expose_only_aggregate_origin() {
        let deposit = CharacterFilth {
            id: 1,
            character_id: 7,
            substance: FilthSubstance::Blood,
            origin: FilthOrigin::Foreign,
            amount: 2,
            deposited_at: 10,
        };
        let serialized =
            serde_json::to_value(spacetimedb_sats::serde::SerdeWrapper::from_ref(&deposit))
                .unwrap();
        assert!(serialized.get("source_character_id").is_none());
        assert_eq!(
            serialized.get("origin"),
            Some(&serde_json::json!({ "Foreign": [] }))
        );
        let markup = filth_status_bar(&[deposit], 0).into_string();
        assert!(markup.contains("2 foreign"));
        assert!(!markup.contains("source_character_id"));
        assert!(!markup.contains("filth-legend"));
        assert!(!markup.contains("/100 filth</span>"));
        assert!(markup.contains("data-strategic-tooltip=\"Filth accumulates"));
        assert!(markup.contains("data-strategic-tooltip=\"Blood\n2\""));
    }

    #[test]
    fn status_rail_layers_filth_inside_independently_accessible_wetness() {
        let condition = CharacterStrategicCondition {
            character_id: 7,
            morale: 0.0,
            morale_bonus: 0.0,
            morale_bonus_cap: 0.0,
            fervor: 0.0,
            pain: 0.0,
            blood_loss: 0.0,
            fear: 0.0,
            fatigue: 0.0,
            hunger: 0.0,
            thirst: 0.0,
            thermal: 0.0,
            wetness_bps: 6_500,
            thermal_strain: -4_000,
            food_days: 1.0,
            water_days: 1.0,
            water_capacity_ml: 2_000,
            incapacitation: 0.0,
            check_multiplier: 1.0,
            status: adventuresim_stdb_client::IncapacitationStatus::Ready,
        };
        let markup =
            strategic_condition_rail(Some(&condition), &[], &[], "/social", false).into_string();
        let sources = markup
            .find("class=\"incapacitation-sources\"")
            .expect("incapacitation source grid");
        let morale = markup
            .find("class=\"morale-meter incapacitation-morale\"")
            .expect("full-width morale meter");
        let temperature = markup
            .find("class=\"temperature-strain incapacitation-source incapacitation-thermal\"")
            .expect("signed temperature meter");
        assert!(sources < morale);
        assert!(morale < temperature);
        assert!(!markup.contains("class=\"incapacitation-source incapacitation-fear\""));
        assert!(markup.contains("href=\"/social\" title=\"Open Recent Tidings\""));
        assert!(markup.contains("/static/icons/game/conversation.svg"));
        assert!(!markup.contains("href=\"/social\" title=\"Open Recent Tidings\" aria-haspopup"));
        let water = markup.find("Water").expect("water meter");
        let wetness = markup
            .find("class=\"wetness-status\"")
            .expect("wetness meter");
        let filth = markup.find("class=\"filth-status\"").expect("filth meter");
        assert!(water < filth);
        assert!(wetness < filth);
        assert!(markup.contains("class=\"coating-status\" role=\"group\""));
        assert!(!markup.contains("wetness-status-label"));
        assert!(markup.contains("aria-label=\"Wetness 65 out of 100\""));
        assert!(markup.contains("aria-label=\"Filth 0 out of 100\""));
        assert!(markup.contains("Wetness is the blue outer bar behind filth"));
        assert!(markup.contains("aria-label=\"Cold strain 40%\""));
        assert!(markup.contains("class=\"temperature-condition-icon\""));
        assert!(!markup.contains(">Cold</span>"));
        assert!(!markup.contains(">Comfort</span>"));
        assert!(!markup.contains(">Hot</span>"));
    }

    #[test]
    fn morale_meter_renders_capped_patterned_social_resolution_with_accessible_meaning() {
        let condition = CharacterStrategicCondition {
            character_id: 7,
            morale: -3.0,
            morale_bonus: 0.0,
            morale_bonus_cap: 0.0,
            fervor: 0.0,
            pain: 0.0,
            blood_loss: 0.0,
            fear: 0.03,
            fatigue: 0.0,
            hunger: 0.0,
            thirst: 0.0,
            thermal: 0.0,
            wetness_bps: 0,
            thermal_strain: 0,
            food_days: 1.0,
            water_days: 1.0,
            water_capacity_ml: 2_000,
            incapacitation: 0.03,
            check_multiplier: 1.0,
            status: adventuresim_stdb_client::IncapacitationStatus::Ready,
        };
        let sources = [
            CharacterMoraleSource {
                id: "loss".into(),
                character_id: 7,
                kind: adventuresim_stdb_client::MoraleSourceKind::Defeat,
                label: "Recent defeat".into(),
                magnitude: -5.0,
            },
            CharacterMoraleSource {
                id: "support".into(),
                character_id: 7,
                kind: adventuresim_stdb_client::MoraleSourceKind::SocialInteraction,
                label: "social interaction".into(),
                magnitude: 8.0,
            },
        ];
        let markup = strategic_condition_rail(Some(&condition), &sources, &[], "/social", false)
            .into_string();
        assert!(markup.contains("--morale-resolved: 5%"));
        assert!(markup.contains("class=\"morale-meter-resolved\""));
        assert!(markup.contains("5.0 morale resolved by successful social support"));
    }

    #[test]
    fn need_meter_places_reserve_right_and_incapacitation_left() {
        let reserve =
            need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", 0.5, 0.0).into_string();
        assert!(reserve.contains("--need-reserve: 50.0%; --need-deficit: 0.0%"));
        assert!(reserve.contains("aria-valuenow=\"50\""));
        assert!(reserve.contains(">Full</span>"));

        let hydration = need_balance_meter(
            "Water",
            "water-drop",
            "Thirst",
            "Hydrated",
            "thirst",
            1.0,
            0.0,
        )
        .into_string();
        assert!(hydration.contains(">Hydrated</span>"));

        let deficit =
            need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", 0.0, 1.0 / 9.0)
                .into_string();
        assert!(deficit.contains("--need-reserve: 0.0%; --need-deficit: 11.1%"));
        assert!(deficit.contains("aria-valuenow=\"-11\""));
    }

    #[test]
    fn surgery_supplies_are_icon_counts_with_hover_labels() {
        let supply = surgery_supply("Bandages", "bandage-roll", 8).into_string();
        assert!(supply.contains("class=\"surgery-supply\""));
        assert!(supply.contains("data-strategic-tooltip=\"Bandages: 8 available\""));
        assert!(supply.contains("bandage-roll.svg"));
        assert!(supply.contains(">x8</span>"));
        assert!(!supply.contains(">Bandages</span>"));
    }

    #[test]
    fn surgery_item_icons_explain_consumed_reusable_and_equipped_supplies() {
        let bandage =
            surgery_item_requirement(SurgeryItemRequirement::BandageConsumed).into_string();
        assert!(bandage.contains("data-strategic-tooltip=\"Expend one bandage\""));
        assert!(bandage.contains(">x1</span>"));

        let kit =
            surgery_item_requirement(SurgeryItemRequirement::SurgeryKitReusable).into_string();
        assert!(kit.contains("data-strategic-tooltip=\"Requires surgery kit\""));
        assert!(kit.contains("aria-label=\"Requires surgery kit; reusable and not consumed\""));
        assert!(kit.contains("medical-pack.svg"));
        assert!(!kit.contains("surgery-item-overlay"));

        let splint = surgery_item_requirement(SurgeryItemRequirement::SplintEquipped).into_string();
        assert!(splint.contains("data-strategic-tooltip=\"Equips 1 splint\""));
        assert!(splint.contains("check-mark.svg"));
    }

    #[test]
    fn surgery_difficulty_uses_shared_skill_meter_for_met_and_unmet_ranks() {
        let meter = surgery_difficulty_meter("Remove ball", 4.0, 2.0).into_string();
        assert!(meter.contains("stat-icon-surgeon"));
        assert!(meter.contains("role=\"meter\""));
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert_eq!(
            meter
                .matches("class=\"rank-current\" style=\"width:100.0%\"")
                .count(),
            2
        );
        assert_eq!(meter.matches("left:0.0%;width:100.0%").count(), 2);
        assert!(!meter.contains("skill-rank-value"));
        assert!(!meter.contains(">4.0<"));
        assert!(meter.contains(
            "aria-label=\"Remove ball: requires 4.0 procedure skill; current effective skill 2.0\""
        ));
        let over_cap = surgery_difficulty_meter("Remove ball", 7.2, 5.0).into_string();
        assert!(over_cap.contains("surgery-difficulty-over-cap-marker"));
        assert!(over_cap.contains("requires 7.2 procedure skill; current effective skill 5.0"));
        assert!(!adventuresim_core::surgery::extraction_requires_surgery_kit(1.0));
        assert!(adventuresim_core::surgery::extraction_requires_surgery_kit(
            1.01
        ));
    }

    #[test]
    fn surgery_preview_uses_the_same_direct_surgery_check_as_reducers() {
        let extraction = surgery_procedure_skill(SurgeryProcedure::Extract, 5.0, false);
        let stitching = surgery_procedure_skill(SurgeryProcedure::Stitch, 5.0, false);
        assert_eq!(
            extraction,
            adventuresim_core::surgery::procedure_skill(SurgeryProcedure::Extract, 5.0, false)
        );
        assert_eq!(
            stitching,
            adventuresim_core::surgery::procedure_skill(SurgeryProcedure::Stitch, 5.0, false)
        );
        assert_eq!(extraction, stitching);
        assert_eq!(
            surgery_procedure_skill(SurgeryProcedure::Extract, 5.0, true),
            2.5
        );
    }

    #[test]
    fn unavailable_surgery_rows_are_greyed_and_buttons_keep_procedure_names() {
        let row = surgery_procedure_row(
            "/test",
            "Stitch",
            "scalpel",
            SurgeryProcedure::Stitch,
            &[SurgeryItemRequirement::SurgeryKitReusable],
            10,
            2.0,
            0.0,
            Some("No injury is present"),
            Some("No injury is present"),
            None,
            true,
            true,
            None,
        )
        .into_string();
        assert!(row.contains("surgery-procedure-unavailable"));
        assert!(row.contains("data-strategic-tooltip=\"No injury is present\""));
        assert!(row.contains("aria-label=\"Stitch: No injury is present\" tabindex=\"0\""));
        assert!(row.contains("disabled title=\"No injury is present\""));
        assert!(row.contains(">Stitch</button>"));
        assert!(!row.contains(">No injury is present</button>"));
    }

    #[test]
    fn bloody_procedure_names_concrete_automatic_alcohol_without_risk_numbers() {
        let row = surgery_procedure_row(
            "/test",
            "Bandage",
            "bandage-roll",
            SurgeryProcedure::Bandage,
            &[],
            10,
            0.0,
            1.0,
            None,
            None,
            None,
            false,
            true,
            Some("aqua_vitae"),
        )
        .into_string();
        assert!(row.contains("Consumes 1 Aqua vitae"));
        assert!(row.contains("beer-stein.svg"));
        assert!(!row.contains("infection probability"));
        assert!(!row.contains("use_alcohol"));
    }

    #[test]
    fn low_physiology_chart_html_contains_no_hidden_payload() {
        let presentation = crate::medical::MedicalPresentation {
            unavailable: false,
            ..Default::default()
        };
        let markup =
            physiology_dialog(&presentation, "physiology-chart-dialog", "Patient").into_string();
        assert!(markup.contains("No authorized shared-presence readings."));
        assert!(markup.contains("data-physiology-dialog"));
        assert!(markup.contains("aria-labelledby=\"physiology-dialog-title\""));
        assert!(!markup.contains("Examine"));
        assert!(!markup.contains("Visible injuries"));
        for forbidden in ["Vitals", "influenza", "infection_id", "disease", "humour-"] {
            assert!(!markup.contains(forbidden), "leaked {forbidden}: {markup}");
        }
    }

    #[test]
    fn physiology_controls_show_only_compact_current_medication() {
        let empty = physiology_controls(
            &crate::medical::MedicalPresentation::default(),
            "/locations/settlement/test/party/1",
        )
        .into_string();
        assert!(empty.is_empty());
        for removed in [
            "Administer preparation",
            "No prepared interventions",
            "dose_milliunits",
            "name=\"route\"",
            "name=\"region\"",
        ] {
            assert!(!empty.contains(removed));
        }

        let presentation = crate::medical::MedicalPresentation {
            active_administrations: vec![crate::medical::AdministrationPresentation {
                id: 12,
                preparation_id: "oral_rehydration_draught".into(),
                display_name: "Oral rehydration draught".into(),
                profile_version: 1,
                route: adventuresim_core::physiology::InterventionRoute::Oral,
                dose: adventuresim_core::physiology::DoseMilliunits::STANDARD,
                region: None,
                administered_at: 100,
                stopped_at: None,
            }],
            ..Default::default()
        };
        let current =
            physiology_controls(&presentation, "/locations/settlement/test/party/1").into_string();
        assert!(current.contains("Current medication"));
        assert!(current.contains("Oral rehydration draught"));
        assert!(current.contains(">Stop</button>"));
        assert!(!current.contains("oral_rehydration_draught"));
        assert!(!current.contains("milliunits"));
        assert!(!current.contains("name=\"route\""));
        assert!(!current.contains("name=\"region\""));
    }

    #[test]
    fn physician_notebook_uses_visual_readings_with_accessible_values() {
        let presentation = crate::medical::MedicalPresentation {
            readings: vec![
                crate::medical::ChartReadingPresentation {
                    minute: 1_440,
                    physiology_band: 2,
                    observation_minutes: 1_440,
                    humour_deviations_bps: [[-1_200, 2_300, 3_400, 4_500]; 7],
                    possible_diseases: vec![crate::medical::DiseaseLikelihoodPresentation {
                        disease_id: "influenza".into(),
                        label: "Catarrhal fever".into(),
                        likelihood_bps: 7_500,
                        typical_effects: vec!["▲ chest phlegm".into(), "▲ head phlegm".into()],
                    }],
                    known_interventions: vec!["oral_rehydration".into()],
                    confidence_bps: 7_000,
                },
                crate::medical::ChartReadingPresentation {
                    minute: 4_320,
                    physiology_band: 2,
                    observation_minutes: 4_320,
                    humour_deviations_bps: [[-1_000, 2_100, 3_200, 4_300]; 7],
                    possible_diseases: vec![crate::medical::DiseaseLikelihoodPresentation {
                        disease_id: "influenza".into(),
                        label: "Catarrhal fever".into(),
                        likelihood_bps: 8_000,
                        typical_effects: vec!["▲ chest phlegm".into(), "▲ head phlegm".into()],
                    }],
                    known_interventions: Vec::new(),
                    confidence_bps: 7_000,
                },
            ],
            gaps: vec![crate::medical::ChartGapPresentation {
                from: 2_160,
                to: 2_880,
            }],
            administrations: vec![crate::medical::AdministrationPresentation {
                id: 1,
                preparation_id: "oral_rehydration".into(),
                display_name: "Oral rehydration draught".into(),
                profile_version: 1,
                route: adventuresim_core::physiology::InterventionRoute::Oral,
                dose: adventuresim_core::physiology::DoseMilliunits::STANDARD,
                region: None,
                administered_at: 1_800,
                stopped_at: Some(3_600),
            }],
            ..Default::default()
        };
        let markup =
            physiology_dialog(&presentation, "physiology-chart-dialog", "Patient").into_string();
        assert!(markup.contains("<svg"));
        assert!(markup.contains("data-physiology-reading-point"));
        assert!(markup.contains("data-physiology-reading-tooltip"));
        assert_eq!(markup.matches("class=\"physiology-shared-gap\"").count(), 1);
        assert_eq!(
            markup
                .matches("class=\"physiology-shared-event physiology-treatment-start\"")
                .count(),
            1
        );
        assert_eq!(
            markup
                .matches("class=\"physiology-shared-event physiology-treatment-stop\"")
                .count(),
            1
        );
        assert!(markup.contains("class=\"physiology-reading-bar"));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("2 days ago"));
        assert!(markup.contains("Today Head observation"));
        assert!(!markup.contains("Day 1"));
        assert!(!markup.contains("Minute 1440"));
        assert!(!markup.contains("physiology-chart-readings"));
        assert!(markup.contains("aria-label=\"Sanguine deviation -12% from baseline\""));
        assert!(markup.contains("Catarrhal fever"));
        assert!(markup.contains("role=\"tooltip\""));
        assert!(markup.contains("▲ chest phlegm"));
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("aria-label=\"Catarrhal fever: estimated likelihood 80%\""));
        assert!(
            !markup.contains(
                "aria-label=\"Catarrhal fever: estimated likelihood 80%. Typical effects"
            )
        );
        assert!(markup.contains("Head humour deviations from healthy baseline over time"));
        assert_eq!(
            markup.matches("class=\"physiology-region-chart\"").count(),
            7
        );
        let region_positions = [
            "Head humour deviations",
            "Chest humour deviations",
            "Stomach humour deviations",
            "Left arm humour deviations",
            "Right arm humour deviations",
            "Left leg humour deviations",
            "Right leg humour deviations",
        ]
        .map(|needle| markup.find(needle).expect(needle));
        assert!(region_positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(markup.contains(">Start<"));
        assert!(markup.contains(">Today<"));
        assert!(markup.contains(">Not in party<"));
        assert!(!markup.contains("Day 1 → Day 3"));
        assert!(!markup.contains("feverish"));
        assert!(markup.contains("oral rehydration"));
        assert!(!markup.contains("infection_id"));
    }

    #[test]
    fn physiology_chart_formats_relative_days() {
        assert_eq!(physiology_relative_day_label(0, 4_320), "3 days ago");
        assert_eq!(physiology_relative_day_label(2_880, 4_320), "1 day ago");
        assert_eq!(physiology_relative_day_label(4_320, 4_320), "Today");
    }

    #[test]
    fn examined_region_meter_has_text_and_aria_not_color_alone() {
        let presentation = crate::medical::MedicalPresentation {
            regional_humours: Some(
                [crate::medical::HumourVitals {
                    phlegmatic: 0.4,
                    ..Default::default()
                }; 7],
            ),
            ..Default::default()
        };
        let markup = regional_health_bar("Chest", 1.0, &presentation, 4, &[], &[]).into_string();
        assert!(markup.contains("Phlegmatic"));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("Chest:"));
    }

    #[test]
    fn surgery_button_is_explicit_and_the_health_meter_is_not_clickable() {
        let markup = attribute_group(
            "Head",
            "head",
            0.75,
            &crate::medical::MedicalPresentation::default(),
            6,
            Some(("/place/party/1/surgery", None)),
            &[],
            &[],
            &[("Intelligence", "intelligence", 3.0)],
        )
        .into_string();

        let link_start = markup.find("limb-surgery-button").unwrap();
        let health_bar = markup.find("class=\"attribute-health-bar\"").unwrap();
        let link_end = markup[link_start..].find("</a>").unwrap() + link_start;
        let attribute_row = markup.find("class=\"party-attribute-row\"").unwrap();

        assert!(link_start < link_end);
        assert!(link_end < health_bar);
        assert!(link_end < attribute_row);
        assert!(markup.contains("aria-label=\"Open surgery menu for Head\""));
        assert!(markup.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
        assert!(markup.contains("data-strategic-tooltip=\"Intelligence: 2.2 out of 5\""));
        assert!(markup.contains("class=\"attribute-rank-bar\" data-strategic-tooltip="));
        assert!(!markup.contains("attribute-rank-value"));
    }

    #[test]
    fn treated_cuts_and_fractures_expose_banded_health_bar_states() {
        let injury = LimbInjury {
            id: "1:chest".into(),
            character_id: 1,
            limb: adventuresim_stdb_client::BodyRegion::Chest,
            cut_damage: 0.2,
            bruise_damage: 0.2,
            frostbite_damage: 0.0,
            fracture_damage: 0.2,
            bandaged: true,
            stitched: false,
            stitch_quality: 0.0,
            splint_owner_id: Some(2),
            splint_inventory_item_id: Some(3),
            infection_exposure: 0.0,
            infection_checks: 0,
            infection_origin_minute: None,
        };
        let markup = regional_health_bar(
            "Chest",
            0.6,
            &crate::medical::MedicalPresentation::default(),
            4,
            &[injury],
            &[],
        )
        .into_string();

        assert!(markup.contains("attribute-health-cut bandaged-cut"));
        assert!(markup.contains("title=\"Bandaged cut damage\""));
        assert!(markup.contains("attribute-health-fracture splinted-fracture"));
        assert!(markup.contains("title=\"Splinted fracture\""));
        assert!(markup.contains("20% splinted fracture"));
    }
}
