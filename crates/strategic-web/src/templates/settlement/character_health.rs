use super::*;

pub(super) fn surgery_limb_name(limb: LimbRegion) -> &'static str {
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

pub(super) fn surgery_limb_slug(limb: LimbRegion) -> &'static str {
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

pub(super) fn surgery_duration(procedure: &str, skill: f32, dc: f32) -> u64 {
    adventuresim_core::surgery::procedure_duration_minutes(procedure, skill, dc)
}

pub(super) fn surgery_procedure_skill(
    procedure: &str,
    checks: [f32; 3],
    self_treatment: bool,
) -> f32 {
    adventuresim_core::surgery::procedure_skill(
        procedure,
        checks[0],
        checks[1],
        checks[2],
        self_treatment,
    )
}

#[derive(Clone, Copy)]
pub(super) enum SurgeryItemRequirement {
    BandageConsumed,
    SurgeryKitReusable,
    SplintEquipped,
}

pub(super) fn surgery_supply(label: &str, icon: &str, quantity: u32) -> Markup {
    let description = format!("{label}: {quantity} available");
    html! {
        div class="surgery-supply" data-strategic-tooltip=(&description)
            aria-label=(&description) tabindex="0" {
            (decorative_game_icon(icon))
            span class="surgery-item-overlay surgery-item-quantity" aria-hidden="true" { "x" (quantity) }
        }
    }
}

pub(super) fn surgery_item_requirement(requirement: SurgeryItemRequirement) -> Markup {
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

pub(super) fn surgery_difficulty_meter(
    procedure_label: &str,
    dc: f32,
    effective_skill: f32,
) -> Markup {
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

pub(super) fn surgery_procedure_row(
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
    procedure_checks: [f32; 3],
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
    let procedure_skill =
        |procedure| surgery_procedure_skill(procedure, procedure_checks, self_treatment);
    let anatomy_skill = procedure_skill("bandage");
    let extraction_skill = procedure_skill("extract");
    let stitching_skill = procedure_skill("stitch");
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
                        (surgery_procedure_row(&action, match projectile.kind { ProjectileKind::Arrowhead => "Remove arrowhead", ProjectileKind::Ball => "Remove ball" }, match projectile.kind { ProjectileKind::Arrowhead => "plain-arrow", ProjectileKind::Ball => "bullet-visual" }, "extract", if requires_kit { &[SurgeryItemRequirement::SurgeryKitReusable] } else { &[] }, surgery_duration("extract", extraction_skill, projectile.extraction_dc), projectile.extraction_dc,
                            extraction_skill, None, if extraction_skill < projectile.extraction_dc { Some("Insufficient Anatomy + Knife skill") } else if requires_kit && !has_kit { Some("No surgery kit") } else { None }, Some(projectile.id), soaps > 0, true, selected_alcohol))
                    }
                    (surgery_procedure_row(&action, "Bandage", "bandage-roll", "bandage", &[SurgeryItemRequirement::BandageConsumed], surgery_duration("bandage", anatomy_skill, 0.0), 0.0,
                        anatomy_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if bandaged { Some("Already bandaged") } else if bandages == 0 { Some("No bandages") } else { None }, None, soaps > 0, true, selected_alcohol))
                    (surgery_procedure_row(&action, "Stitch", "scalpel", "stitch", &[SurgeryItemRequirement::SurgeryKitReusable], surgery_duration("stitch", stitching_skill, 2.0), 2.0,
                        stitching_skill, if cut <= 0.0 { Some("No injury is present") } else { None }, if cut <= 0.0 { Some("No injury is present") } else if stitched { Some("Already stitched") } else if stitching_skill < 2.0 { Some("Insufficient Anatomy + Tailoring skill") } else if !has_kit { Some("No surgery kit") } else { None }, None, soaps > 0, true, selected_alcohol))
                    @if splinted {
                        (surgery_procedure_row(&action, "Remove splint", "arm-bandage", "remove-splint", &[], surgery_duration("remove-splint", anatomy_skill, 0.0), 0.0, anatomy_skill, None, None, None, false, false, None))
                    } @else {
                        (surgery_procedure_row(&action, "Splint", "arm-bandage", "splint", &[SurgeryItemRequirement::SplintEquipped], surgery_duration("splint", anatomy_skill, 1.0), 1.0,
                            anatomy_skill, if fracture <= 0.0 { Some("No injury is present") } else { None }, if fracture <= 0.0 { Some("No injury is present") } else if anatomy_skill < 1.0 { Some("Insufficient Anatomy skill") } else if splints == 0 { Some("No splints") } else { None }, None, false, false, None))
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
    _morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
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
    let meter_style = format!("--morale-fear: {fear_fill}%; --morale-bonus: {bonus_fill}%");
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
            div class=(if condition.fear > 0.0 { "morale-meter is-fearful" } else { "morale-meter" }) style=(meter_style) role="meter" aria-valuemin="-5" aria-valuemax="5" aria-valuenow=(format!("{:.1}", condition.morale)) aria-label=(format!(
                "Morale {:.1}; fear {}; inspiration {:.1}%",
                condition.morale,
                percent(condition.fear),
                condition.morale_bonus * 100.0,
            )) {
                div class="morale-meter-heading" {
                    strong class="metric-label" { (decorative_game_icon("sun")) span { "Morale" } }
                    span class="morale-meter-value" { (format!("{:+.1}", condition.morale)) }
                    a class=(if social_open { "character-menu-button is-open" } else { "character-menu-button" })
                        href=(social_href) title="Open social menu" aria-label="Open social menu"
                        aria-haspopup="dialog" aria-expanded=(social_open) {
                        span class="stat-icon" style="--stat-icon: url('/static/icons/game/social.svg')" aria-hidden="true" {}
                        @if social_open { span class="sr-only" { " (open)" } }
                    }
                }
                div class="morale-meter-track" aria-hidden="true" {
                    span class="morale-meter-fear" {}
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

pub(super) fn need_balance_meter(
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

pub(super) fn regional_health_values(limbs: Option<&CharacterLimbs>) -> [f32; 7] {
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

pub(super) fn limb_attribute_column(
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

pub(super) fn attribute_group(
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

pub(super) fn attribute_group_with_labels(
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

pub(super) fn regional_health_bar(
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

pub(super) fn attribute_row(
    name: &str,
    icon: &str,
    value: f32,
    health: f32,
    show_label: bool,
) -> Markup {
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
