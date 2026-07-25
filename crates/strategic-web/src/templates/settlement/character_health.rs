use maud::{Markup, html};

use super::{
    character_skills::{SkillRankBarOptions, skill_rank_bar},
    context::LocationView,
    trade::filth_status_bar,
};
use crate::medical::MedicalPresentation;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterLimbs, CharacterStrategicCondition, LimbInjury,
    LimbRegion, ProjectileKind, RetainedProjectile,
};
use crate::templates::{
    decorative_game_icon, game_icon, item_display_name, sidebar_section, stat_icon_path,
};

fn surgery_limb_name(limb: LimbRegion) -> &'static str {
    match limb {
        LimbRegion::LeftArm => "Left arm",
        LimbRegion::RightArm => "Right arm",
        LimbRegion::LeftLeg => "Left leg",
        LimbRegion::RightLeg => "Right leg",
        LimbRegion::Chest => "Chest",
        LimbRegion::Stomach => "Stomach",
        LimbRegion::Head => "Head",
    }
}

fn surgery_limb_slug(limb: LimbRegion) -> &'static str {
    match limb {
        LimbRegion::LeftArm => "left-arm",
        LimbRegion::RightArm => "right-arm",
        LimbRegion::LeftLeg => "left-leg",
        LimbRegion::RightLeg => "right-leg",
        LimbRegion::Chest => "chest",
        LimbRegion::Stomach => "stomach",
        LimbRegion::Head => "head",
    }
}

fn surgery_duration(procedure: &str, skill: f32, dc: f32) -> u64 {
    adventuresim_core::surgery::procedure_duration_minutes(procedure, skill, dc)
}

fn surgery_procedure_skill(checks: [f32; 2], self_treatment: bool) -> f32 {
    adventuresim_core::surgery::procedure_skill(checks[0], checks[1], self_treatment)
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

fn surgery_procedure_row(
    action: &str,
    label: &str,
    icon: &str,
    procedure: &str,
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
            input type="hidden" name="procedure" value=(procedure);
            @if let Some(projectile_id) = projectile_id {
                input type="hidden" name="projectile_id" value=(projectile_id);
            }
            @if soap_applicable {
                label class="surgery-soap-option" title="Consumes one unit; lowers contamination risk independently of other supplies" {
                    input type="checkbox" name="use_soap" value="true" disabled[!soap_available];
                    " Use 1 soft soap"
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
#[allow(clippy::too_many_arguments)]
pub fn surgery_dialog(
    location: &LocationView,
    active_character: &Character,
    patient: &Character,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    selected_limb: LimbRegion,
    bandages: u32,
    surgery_kits: u32,
    splints: u32,
    soaps: u32,
    alcohol_units: u32,
    selected_alcohol: Option<&str>,
    procedure_checks: [f32; 2],
) -> Markup {
    let action = location.preserve_building(format!(
        "{}/party/{}/surgery/{}/procedure",
        location.base_path(),
        patient.id,
        surgery_limb_slug(selected_limb)
    ));
    let selected = injuries.iter().find(|injury| injury.limb == selected_limb);
    let cut = selected.map_or(0.0, |injury| injury.cut_damage.max(0.0));
    let bruise = selected.map_or(0.0, |injury| injury.bruise_damage.max(0.0));
    let fracture = selected.map_or(0.0, |injury| injury.fracture_damage.max(0.0));
    let bandaged = selected.is_some_and(|injury| injury.bandaged);
    let stitched = selected.is_some_and(|injury| injury.stitched);
    let splinted = selected.is_some_and(|injury| injury.splint_inventory_item_id.is_some());
    let has_kit = surgery_kits > 0;
    let self_treatment = active_character.id == patient.id;
    let procedure_skill = surgery_procedure_skill(procedure_checks, self_treatment);
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
                    @for projectile in projectiles.iter().filter(|projectile| projectile.limb == selected_limb) {
                        @let requires_kit = adventuresim_core::surgery::extraction_requires_surgery_kit(projectile.extraction_dc);
                        (surgery_procedure_row(&action, match projectile.kind { ProjectileKind::Arrowhead => "Remove arrowhead", ProjectileKind::Ball => "Remove ball" }, match projectile.kind { ProjectileKind::Arrowhead => "plain-arrow", ProjectileKind::Ball => "bullet-visual" }, "extract", if requires_kit { &[SurgeryItemRequirement::SurgeryKitReusable] } else { &[] }, surgery_duration("extract", procedure_skill, projectile.extraction_dc), projectile.extraction_dc,
                            procedure_skill, None, if procedure_skill < projectile.extraction_dc { Some("Insufficient Surgery or Human knowledge") } else if requires_kit && !has_kit { Some("No surgery kit") } else { None }, Some(projectile.id), soaps > 0, true, selected_alcohol))
                    }
                    (surgery_procedure_row(&action, "Bandage", "bandage-roll", "bandage", &[SurgeryItemRequirement::BandageConsumed], surgery_duration("bandage", procedure_skill, 0.0), 0.0,
                        procedure_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if bandaged { Some("Already bandaged") } else if bandages == 0 { Some("No bandages") } else { None }, None, soaps > 0, true, selected_alcohol))
                    (surgery_procedure_row(&action, "Stitch", "scalpel", "stitch", &[SurgeryItemRequirement::SurgeryKitReusable], surgery_duration("stitch", procedure_skill, 2.0), 2.0,
                        procedure_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if stitched { Some("Already stitched") } else if procedure_skill < 2.0 { Some("Insufficient Surgery or Human knowledge") } else if !has_kit { Some("No surgery kit") } else { None }, None, soaps > 0, true, selected_alcohol))
                    @if splinted {
                        (surgery_procedure_row(&action, "Remove splint", "arm-bandage", "remove-splint", &[], surgery_duration("remove-splint", procedure_skill, 0.0), 0.0, procedure_skill, None, None, None, false, false, None))
                    } @else {
                        (surgery_procedure_row(&action, "Splint", "arm-bandage", "splint", &[SurgeryItemRequirement::SplintEquipped], surgery_duration("splint", procedure_skill, 1.0), 1.0,
                            procedure_skill, if fracture <= 0.0 { Some("No injury is present") } else { None }, if fracture <= 0.0 { Some("No injury is present") } else if procedure_skill < 1.0 { Some("Insufficient Surgery or Human knowledge") } else if splints == 0 { Some("No splints") } else { None }, None, false, false, None))
                    }
                    @if cut <= 0.0 && bruise > 0.0 && fracture <= 0.0 {
                        p class="text-muted small-copy" { "Bruising must heal on its own." }
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
    let fear_fill = (condition.fear.clamp(0.0, 1.0) * 100.0).round();
    let bonus_fill = if condition.morale_bonus_cap > 0.0 {
        (condition.morale_bonus / condition.morale_bonus_cap * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
    .round();
    let resolved_morale = adventuresim_core::social::resolved_social_morale(
        morale_sources
            .iter()
            .map(|source| (source.kind.as_str(), source.magnitude)),
    );
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
    ];
    let incapacitation_sources = [
        ("Pain", "broken-heart", "pain", condition.pain),
        (
            "Blood loss",
            "bleeding-wound",
            "blood",
            condition.blood_loss,
        ),
        ("Fear", "terror", "fear", condition.fear),
        ("Fatigue", "night-sleep", "fatigue", condition.fatigue),
    ];
    html! {
        (sidebar_section("Status", html! {
            div class=(if condition.fear > 0.0 { "morale-meter is-fearful" } else { "morale-meter" }) style=(meter_style) role="meter" aria-valuemin="-100" aria-valuemax="100" aria-valuenow=(format!("{:.1}", condition.morale)) title=(format!("{resolved_morale:.1} morale from successful social support currently offsets actionable concerns")) aria-label=(format!(
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
                        href=(social_href) title="Open social menu" aria-label="Open social menu"
                        aria-haspopup="dialog" aria-expanded=(social_open) {
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
                    span class="incapacitation-status" { (&condition.status) }
                }
                div class="incapacitation-total-track" role="meter"
                    aria-label=(format!("Incapacitation {}; {}", percent(condition.incapacitation), condition.status))
                    aria-valuemin="0" aria-valuemax="100"
                    aria-valuenow=(condition.incapacitation.clamp(0.0, 1.0) * 100.0) {
                    @for (_, _, color, value) in incapacitation_segments {
                        span class=(format!("incapacitation-segment incapacitation-{color}"))
                            style=(format!("--incap-amount: {:.1}%", value.max(0.0) * 100.0)) {}
                    }
                }
            }
            div class="incapacitation-sources" aria-label="Sources of incapacitation" {
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
            }
            div class="need-balance-meters" aria-label="Food and water reserves" {
                (need_balance_meter("Food", "meal", "Hunger", "Full", "hunger", condition.food_days, condition.hunger))
                (need_balance_meter("Water", "water-drop", "Thirst", "Hydrated", "thirst", condition.water_days, condition.thirst))
            }
            (filth_status_bar(filth))
        }))
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

pub(super) fn medical_rail(
    medical: &MedicalPresentation,
    location_path: &str,
    doctor_id: u64,
    target_id: u64,
    _allow_treatment: bool,
) -> Markup {
    html! {
        (sidebar_section("Symptoms", html! {
            @if medical.unavailable {p class="text-muted small-copy" {"Medical examination unavailable."}} @else if medical.symptoms.is_empty(){p class="text-muted small-copy" { "No visible symptoms." }}@else{p class="medical-symptoms" {(medical.symptoms.join(" · "))}}
            @for medication in &medical.medications {
                p class="medical-treatment-status" {
                    "Taking medication for " (medication.disease_name) "."
                    @if doctor_id == target_id {
                        form method="post" action=(format!("{location_path}/party/{target_id}/medication/{}/unequip", medication.equipment_id)) {
                            button type="submit" class="medical-medication-remove" aria-label=(format!("Stop medication for {}", medication.disease_name)) title="Stop taking this medication; the course will be discarded" { "×" }
                        }
                    }
                }
            }
        }))
    }
}

pub(super) fn medical_examination_popup(
    medical: &MedicalPresentation,
    location: &LocationView,
    target_id: u64,
    limbs: Option<&CharacterLimbs>,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
) -> Markup {
    let Some(examination_id) = medical.examination_id else {
        return html! {};
    };
    let dismiss_url = location.preserve_building(format!(
        "{}/party/{target_id}/examination/{examination_id}/dismiss",
        location.base_path()
    ));
    html! {
        div class="medical-examination-overlay" role="dialog" aria-modal="true" aria-labelledby="medical-examination-title"
            data-medical-examination
            data-dismiss-url=(&dismiss_url) {
            section class="medical-examination-popup" {
                header class="medical-examination-heading" {
                    div {
                        h2 id="medical-examination-title" { "Examination findings" }
                        @if let Some(examined_at) = medical.examined_at {
                            p class="text-muted small-copy" { "Observed at personal minute " (examined_at) "." }
                        }
                    }
                    form method="post" action=(&dismiss_url) {
                        button type="submit" class="medical-examination-close" aria-label="Close examination findings" { "×" }
                    }
                }
                @if medical.regional_humours.is_some() {
                    div class="examination-region-bars" aria-label="Examined body regions" {
                        h3 { "Body regions" }
                        @let health = regional_health_values(limbs);
                        @for (index, name) in ["Left arm", "Right arm", "Left leg", "Right leg", "Chest", "Stomach", "Head"].into_iter().enumerate() {
                            @let reading = medical.regional_humours.map(|regions| regions[index]).unwrap_or_default();
                            @if health[index] < 1.0 || reading.sanguine + reading.phlegmatic + reading.choleric + reading.melancholic > 0.0 {
                                div class="examination-region-row" {
                                    strong { (name) }
                                    (regional_health_bar(name, health[index], medical, index, injuries, projectiles))
                                }
                            }
                        }
                    }
                }
                @if !medical.findings.is_empty() {
                    h3 { "Observed signs" }
                    p class="medical-symptoms" { (medical.findings.join(" · ")) }
                }
                @if !medical.possible_diagnoses.is_empty() {
                    div class="medical-diagnoses" {
                        h3 { "Possible ailments" }
                        p class="small-copy" { "The findings do not permit a confident distinction." }
                        ul { @for possibility in &medical.possible_diagnoses { li { (possibility) } } }
                    }
                }
                @if !medical.diagnoses.is_empty() {
                    div class="medical-diagnoses" {
                        h3 { "Diagnosed conditions" }
                        @for diagnosis in &medical.diagnoses {
                            article {
                                strong { (diagnosis.period_name) }
                                span class="condition-stage" { " — " (diagnosis.stage) }
                                p class="small-copy" { (diagnosis.contagion) }
                            }
                        }
                    }
                }
                @if medical.findings.is_empty() && medical.possible_diagnoses.is_empty() && medical.diagnoses.is_empty() {
                    p class="text-muted" { "The examination did not reveal an identifiable internal cause." }
                }
            }
        }
    }
}

fn regional_health_values(limbs: Option<&CharacterLimbs>) -> [f32; 7] {
    limbs.map_or([1.0; 7], |limbs| {
        [
            limbs.left_arm_health,
            limbs.right_arm_health,
            limbs.left_leg_health,
            limbs.right_leg_health,
            limbs.chest_health,
            limbs.stomach_health,
            limbs.head_health,
        ]
    })
}

pub(super) fn party_attributes_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    limbs: Option<&CharacterLimbs>,
    medical: &MedicalPresentation,
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
        (sidebar_section(title, html! {
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
                (attribute_group("Stomach", "stomach", stomach_health, medical, 5, surgery, injuries, projectiles, &[
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
        }))
    }
}

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
        LimbRegion::LeftArm,
        LimbRegion::RightArm,
        LimbRegion::LeftLeg,
        LimbRegion::RightLeg,
        LimbRegion::Chest,
        LimbRegion::Stomach,
        LimbRegion::Head,
    ][region];
    let injury = injuries.iter().find(|injury| injury.limb == limb);
    let cut = injury
        .map_or(0.0, |row| row.cut_damage)
        .min(physical_damage);
    let total_blunt = injury
        .map_or(physical_damage - cut, |row| {
            row.bruise_damage.max(row.fracture_damage)
        })
        .min((physical_damage - cut).max(0.0));
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
        values.sanguine + values.phlegmatic + values.choleric + values.melancholic
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
                "Sanguine",
                "attribute-health-sanguine",
                values.sanguine * scale,
            ),
            (
                "Phlegmatic",
                "attribute-health-phlegmatic",
                values.phlegmatic * scale,
            ),
            (
                "Choleric",
                "attribute-health-choleric",
                values.choleric * scale,
            ),
            (
                "Melancholic",
                "attribute-health-melancholic",
                values.melancholic * scale,
            ),
        ]
    } else {
        vec![("Other impairment", "attribute-health-other", other)]
    };
    let reading = if humour.is_some() {
        format!(
            "{name}: {:.0}% sound, {:.0}% cut, {:.0}% blunt, {:.0}% {fracture_label}, {:.0}% sanguine, {:.0}% phlegmatic, {:.0}% choleric, {:.0}% melancholic impairment",
            okay * 100.0,
            cut * 100.0,
            blunt * 100.0,
            fracture * 100.0,
            values.sanguine * scale * 100.0,
            values.phlegmatic * scale * 100.0,
            values.choleric * scale * 100.0,
            values.melancholic * scale * 100.0,
        )
    } else {
        format!(
            "{name}: {:.0}% sound, {:.0}% cut, {:.0}% blunt, {:.0}% {fracture_label}, {:.0}% other impairment",
            okay * 100.0,
            cut * 100.0,
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
            span class="attribute-health-blunt" title="Blunt damage" style=(format!("width:{:.1}%", blunt * 100.0)) {}
            span class=(if splinted { "attribute-health-fracture splinted-fracture" } else { "attribute-health-fracture" })
                title=(if splinted { "Splinted fracture" } else { "Fracture" })
                style=(format!("width:{:.1}%", fracture * 100.0)) {}
            @for (label, class, amount) in segments {
                @if amount > 0.0 {
                    span class=(class) title=(label) style=(format!("width:{:.1}%", amount * 100.0)) {}
                }
            }
            @for (projectile_index, projectile) in projectiles.iter().filter(|projectile| projectile.limb == limb).enumerate() {
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
            div class="attribute-rank-bar" title=(format!("{effective_value:.1}"))
                role="meter" aria-valuemin="0" aria-valuemax="5" aria-valuenow=(format!("{effective_value:.1}"))
                aria-label=(format!("{name}: {effective_value:.1} out of 5")) {
                span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
            }
            span class="attribute-rank-value" aria-hidden="true" { (format!("{effective_value:.1}")) }
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
    use crate::templates::settlement::{
        character_skills::{SkillAction, skill_action_icon},
        chrome::party_portrait_overlay,
        context::LocationKind,
    };

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
        let serialized = serde_json::to_value(&deposit).unwrap();
        assert!(serialized.get("source_character_id").is_none());
        assert_eq!(
            serialized.get("origin").and_then(|value| value.as_str()),
            Some("Foreign")
        );
        let markup = filth_status_bar(&[deposit]).into_string();
        assert!(markup.contains("2 foreign"));
        assert!(!markup.contains("source_character_id"));
        assert!(!markup.contains("filth-legend"));
        assert!(!markup.contains("/100 filth</span>"));
        assert!(markup.contains("data-strategic-tooltip=\"Filth accumulates"));
        assert!(markup.contains("data-strategic-tooltip=\"Blood\n2\""));
    }

    #[test]
    fn status_rail_places_filth_after_water() {
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
            food_days: 1.0,
            water_days: 1.0,
            water_capacity_ml: 2_000,
            incapacitation: 0.0,
            check_multiplier: 1.0,
            status: "ready".into(),
        };
        let markup =
            strategic_condition_rail(Some(&condition), &[], &[], "/social", false).into_string();
        assert!(markup.contains("class=\"morale-meter\""));
        assert!(markup.contains("href=\"/social\" title=\"Open social menu\""));
        assert!(markup.contains("/static/icons/game/conversation.svg"));
        assert!(markup.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
        let water = markup.find("Water").expect("water meter");
        let filth = markup.find("Filth").expect("filth meter");
        assert!(water < filth);
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
            food_days: 1.0,
            water_days: 1.0,
            water_capacity_ml: 2_000,
            incapacitation: 0.03,
            check_multiplier: 1.0,
            status: "ready".into(),
        };
        let sources = [
            CharacterMoraleSource {
                id: "loss".into(),
                character_id: 7,
                kind: "defeat".into(),
                label: "Recent defeat".into(),
                magnitude: -5.0,
            },
            CharacterMoraleSource {
                id: "support".into(),
                character_id: 7,
                kind: "social_interaction".into(),
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
    fn surgery_preview_uses_the_same_species_cap_as_reducers() {
        let checks = [5.0, 2.0];
        assert_eq!(surgery_procedure_skill(checks, false), 2.0);
        assert_eq!(
            surgery_procedure_skill(checks, false),
            adventuresim_core::surgery::procedure_skill(5.0, 2.0, false)
        );
        assert_eq!(surgery_procedure_skill([5.0, 5.0], true), 2.5);
    }

    #[test]
    fn unavailable_surgery_rows_are_greyed_and_buttons_keep_procedure_names() {
        let row = surgery_procedure_row(
            "/test",
            "Stitch",
            "scalpel",
            "stitch",
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
            "bandage",
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
    fn low_medicine_medical_html_contains_no_hidden_payload() {
        let presentation = crate::medical::MedicalPresentation {
            unavailable: false,
            symptoms: vec!["coughing"],
            diagnoses: Vec::new(),
            ..Default::default()
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(markup.contains("coughing"));
        assert!(!markup.contains("Examine"));
        assert!(!markup.contains("Visible injuries"));
        for forbidden in ["Vitals", "influenza", "infection_id", "disease", "humour-"] {
            assert!(!markup.contains(forbidden), "leaked {forbidden}: {markup}");
        }
    }

    #[test]
    fn active_medication_is_listed_beneath_symptoms() {
        let presentation = crate::medical::MedicalPresentation {
            symptoms: vec!["coughing"],
            medications: vec![crate::medical::MedicationPresentation {
                equipment_id: 11,
                disease_name: "Consumption",
            }],
            ..Default::default()
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, false).into_string();
        let symptoms_at = markup.find("coughing").unwrap();
        let medication_at = markup.find("Taking medication for Consumption.").unwrap();
        assert!(medication_at > symptoms_at);
    }

    #[test]
    fn medicine_action_moves_from_portrait_to_the_skill_icon() {
        let doctor = Character {
            id: 1,
            name: "Doctor".into(),
            xp: 0,
            level: 1,
            gold: 100,
            current_settlement_id: Some("willowmere".into()),
            current_case_site_id: None,
            party_id: Some("demo".into()),
            age_years: 30,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let portrait = party_portrait_overlay(
            &[doctor.clone()],
            Some(&doctor),
            "/locations/settlement/willowmere",
            Some(1),
            true,
        )
        .into_string();
        assert!(!portrait.contains("/examine"));
        assert!(!portrait.contains("party-medical-examine"));
        assert!(portrait.contains("party-alchemy-action"));

        let skill = skill_action_icon(
            "Medicine",
            "medicine",
            SkillAction::Post {
                href: "/place/party/1/examine",
                label: "Perform medical examination (15 minutes)",
                open: false,
            },
            false,
        )
        .into_string();
        assert!(skill.contains("/place/party/1/examine"));
        assert!(skill.contains("Perform medical examination (15 minutes)"));
        assert!(skill.contains("aria-haspopup=\"dialog\" aria-expanded=\"false\""));
    }

    #[test]
    fn pending_examination_is_a_one_shot_center_popup_not_sidebar_history() {
        let presentation = crate::medical::MedicalPresentation {
            findings: vec!["coughing".into(), "fatigued".into()],
            examination_id: Some(44),
            examined_at: Some(8_640),
            regional_humours: Some(
                [crate::medical::HumourVitals {
                    sanguine: 0.9,
                    phlegmatic: 0.6,
                    choleric: 0.8,
                    melancholic: 1.0,
                }; 7],
            ),
            possible_diagnoses: vec!["Catarrhal fever", "Consumption"],
            ..Default::default()
        };
        let sidebar = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(!sidebar.contains("Four humours"));
        assert!(!sidebar.contains("Possible ailments"));
        assert!(!sidebar.contains("Observed at personal minute"));

        let location = LocationView {
            kind: LocationKind::Quest,
            id: "location".into(),
            name: "Location".into(),
            religion_id: None,
            category: None,
            active_building: Some("inn".into()),
        };
        let popup =
            medical_examination_popup(&presentation, &location, 2, None, &[], &[]).into_string();
        assert!(popup.contains("medical-examination-overlay"));
        assert!(popup.contains("aria-modal=\"true\""));
        assert!(popup.contains("Possible ailments"));
        assert!(popup.contains("Body regions"));
        assert!(popup.contains("attribute-health-phlegmatic"));
        assert!(popup.contains("Catarrhal fever"));
        assert!(popup.contains("Close examination findings"));
        assert!(popup.contains('×'));
        assert!(!popup.contains("This result is discarded"));
        assert!(popup.contains("/examination/44/dismiss"));
        assert!(popup.contains("?building=inn"));
        assert!(popup.contains("data-medical-examination"));
        let lifecycle = include_str!("../../../static/medical-examination.js");
        assert!(lifecycle.contains("pagehide"));
        assert!(lifecycle.contains("navigator.sendBeacon"));
        assert!(lifecycle.contains("event.key !== \"Escape\""));
        assert!(lifecycle.contains("restoreFocus"));
        assert!(lifecycle.contains(".party-offer[role='dialog']"));
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
    }

    #[test]
    fn treated_cuts_and_fractures_expose_banded_health_bar_states() {
        let injury = LimbInjury {
            id: "1:chest".into(),
            character_id: 1,
            limb: LimbRegion::Chest,
            cut_damage: 0.2,
            bruise_damage: 0.2,
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
