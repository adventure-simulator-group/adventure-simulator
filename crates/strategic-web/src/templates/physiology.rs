use adventuresim_core::{
    disease::STARTER_DISEASES,
    physiology::{HUMOUR_WEIGHTS, Humour, Meter},
};
use maud::{Markup, html};

fn weight_percent(weight: f32) -> String {
    format!("{:.0}%", weight * 100.0)
}

fn weight_style(weight: f32) -> String {
    format!("--weight: {:.0}%", weight.clamp(0.0, 1.0) * 100.0)
}

fn phase_width(minutes: u64, total: u64) -> String {
    format!(
        "--phase-width: {:.2}%",
        minutes as f64 / total.max(1) as f64 * 100.0
    )
}

fn meter_symbol(meter: Meter) -> &'static str {
    match meter {
        Meter::Oxygenation => "O₂",
        Meter::Perfusion => "↻",
        Meter::Hydration => "◉",
        Meter::Temperature => "°",
        Meter::Inflammation => "✦",
        Meter::Coagulation => "◆",
        Meter::Nutrition => "◒",
        Meter::Neurologic => "ϟ",
        Meter::RenalClearance => "◇",
        Meter::TissueIntegrity => "▦",
    }
}

pub fn physiology_reference_page(character_name: &str) -> Markup {
    let content = html! {
        aside class="left-sidebar physiology-reference-index" {
            h2 { "Physiology" }
            nav aria-label="Physiology reference sections" {
                a href="#meters" { "Private meter catalogue" }
                a href="#humours" { "Public humour model" }
                a href="#diseases" { "Disease courses" }
                a href="#observation" { "Physician notebooks" }
                a href="#limits" { "Non-goals" }
            }
        }
        main class="center-content physiology-reference" {
            p class="physiology-reference-todo" role="note" {
                "TODO: rewrite all this information into NPC dialogue"
            }
            header {
                h1 { "Physiology" }
                p {
                    "A physician observes changes in a patient over time. The body simulation uses "
                    "private functional losses; period characters describe their lossy public projection "
                    "as the four humours."
                }
                figure class="physiology-model-flow" aria-labelledby="physiology-model-caption" {
                    div class="physiology-flow-private" {
                        span class="physiology-flow-lock" aria-hidden="true" { "×" }
                        div class="physiology-flow-meter-grid" aria-hidden="true" {
                            @for meter in Meter::ALL {
                                i title=(meter.public_name()) { (meter_symbol(meter)) }
                            }
                        }
                        strong { "Private state" }
                    }
                    span class="physiology-flow-arrow" aria-hidden="true" { "→" }
                    div class="physiology-flow-humours" aria-hidden="true" {
                        @for (short, tone, value) in [
                            ("S", "sanguine", 68),
                            ("P", "phlegmatic", 42),
                            ("C", "choleric", 27),
                            ("M", "melancholic", 51),
                        ] {
                            div class=(format!("physiology-flow-bar physiology-tone-{tone}")) {
                                span { (short) }
                                i style=(format!("--flow-value: {value}%")) {}
                            }
                        }
                        strong { "Lossy projection" }
                    }
                    span class="physiology-flow-arrow" aria-hidden="true" { "→" }
                    div class="physiology-flow-chart" aria-hidden="true" {
                        svg viewBox="0 0 100 44" preserveAspectRatio="none" {
                            path d="M 0,34 L 22,29 L 48,32 L 72,16 L 100,20" {}
                            path d="M 0,38 L 22,35 L 48,24 L 72,27 L 100,12" {}
                        }
                        strong { "Notebook" }
                    }
                    figcaption id="physiology-model-caption" {
                        "The chart records a blurred observation of change—not the hidden cause."
                    }
                }
            }
            section id="meters" aria-labelledby="meters-heading" {
                h2 id="meters-heading" { "Private meter catalogue" }
                p {
                    "Each loss begins near zero for a healthy person. Losses combine explicitly and are "
                    "clamped to the authored simulation bounds. Reaching 100% loss on any meter is terminal."
                }
                ul class="physiology-meter-list" aria-label="Private functional loss meters" {
                    @for meter in Meter::ALL {
                        li title=(meter.interpretation()) tabindex="0" {
                            span class="physiology-meter-symbol" aria-hidden="true" { (meter_symbol(meter)) }
                            div {
                                strong { (meter.public_name()) }
                                span class="physiology-private-badge" { "private" }
                                div class="physiology-concept-track" aria-hidden="true" {
                                    i {}
                                    b {}
                                }
                                small { (meter.interpretation()) }
                            }
                        }
                    }
                }
            }
            section id="humours" aria-labelledby="humours-heading" {
                h2 id="humours-heading" { "The four humours" }
                p {
                    "Humours are signed deviations from a healthy zero baseline, formed from weighted private "
                    "meters and visible findings in each body region. They are observations, not diagnoses. "
                    "Several different hidden states can therefore produce the same displayed humour reading."
                }
                div class="physiology-weight-table-wrap" tabindex="0"
                    aria-label="Public physiology meter to humour weight matrix" {
                    table class="physiology-weight-table" {
                        caption { "Exact public humour weights" }
                        thead {
                            tr {
                                th scope="col" { "Functional loss" }
                                @for humour in Humour::ALL {
                                    th scope="col" class=(format!("physiology-tone-{}", humour.public_name().to_ascii_lowercase())) {
                                        i aria-hidden="true" {}
                                        (humour.public_name())
                                    }
                                }
                            }
                        }
                        tbody {
                            @for meter in Meter::ALL {
                                tr {
                                    th scope="row" tabindex="0"
                                        aria-label=(format!("{}: {}", meter.public_name(), meter.interpretation()))
                                        data-strategic-tooltip=(meter.interpretation()) {
                                        (meter.public_name())
                                    }
                                    @for (index, weight) in HUMOUR_WEIGHTS[meter.index()].into_iter().enumerate() {
                                        @let tone = ["sanguine", "phlegmatic", "choleric", "melancholic"][index];
                                        td class=(format!("physiology-weight-cell physiology-tone-{tone}"))
                                            style=(weight_style(weight))
                                            aria-label=(format!("{} weight {}", Humour::ALL[index].public_name(), weight_percent(weight))) {
                                            span aria-hidden="true" { (weight_percent(weight)) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                p class="text-muted" {
                    "Simplification: humour readings discard the underlying causes, interactions, visible findings, and phenotype. "
                    "Colour is decorative; every value and label is available as text, keyboard focus and screen-reader content."
                }
            }
            section id="diseases" aria-labelledby="diseases-heading" {
                h2 id="diseases-heading" { "Public disease courses" }
                p {
                    "These are broad public ranges and typical sequences, not a means of identifying a patient's illness."
                }
                div class="physiology-disease-grid" {
                    @for disease in STARTER_DISEASES {
                        @let total = disease.incubation_minutes + disease.rise_minutes + disease.peak_minutes + disease.recovery_minutes;
                        article class="physiology-disease-card" {
                            header {
                                h3 { (disease.period_name) }
                                span { (total / 1_440) "d" }
                            }
                            div class="physiology-course-track" role="img"
                                aria-label=(format!(
                                    "{} course: incubation {} days, rise {} days, established {} days, recovery {} days",
                                    disease.period_name,
                                    disease.incubation_minutes / 1_440,
                                    disease.rise_minutes / 1_440,
                                    disease.peak_minutes / 1_440,
                                    disease.recovery_minutes / 1_440,
                                )) {
                                i class="phase-incubation" style=(phase_width(disease.incubation_minutes, total)) title="Incubation" {}
                                i class="phase-rise" style=(phase_width(disease.rise_minutes, total)) title="Increasing complaints" {}
                                i class="phase-peak" style=(phase_width(disease.peak_minutes, total)) title="Established illness" {}
                                i class="phase-recovery" style=(phase_width(disease.recovery_minutes, total)) title="Convalescence" {}
                            }
                            ol class="physiology-course-labels" aria-hidden="true" {
                                li style=(phase_width(disease.incubation_minutes, total)) { "quiet" }
                                li style=(phase_width(disease.rise_minutes, total)) { "rise" }
                                li style=(phase_width(disease.peak_minutes, total)) { "ill" }
                                li style=(phase_width(disease.recovery_minutes, total)) { "recover" }
                            }
                            p { (disease.contagion) }
                        }
                    }
                }
            }
            section id="observation" aria-labelledby="observation-heading" {
                h2 id="observation-heading" { "Physician notebooks" }
                div class="physiology-observation-diagram" {
                    div class="physiology-observation-presence" {
                        span aria-hidden="true" { "●" }
                        i {}
                        span aria-hidden="true" { "●" }
                        strong { "Together" }
                    }
                    div class="physiology-observation-samples"
                        aria-label="Three quantized observations followed by an observation gap" {
                        @for height in [34, 58, 46] {
                            i style=(format!("--sample-height: {height}%")) {}
                        }
                        b aria-label="Observation gap" {}
                        @for height in [70, 62] {
                            i style=(format!("--sample-height: {height}%")) {}
                        }
                    }
                    div class="physiology-observation-skill" {
                        span { "low skill" }
                        i aria-hidden="true" {}
                        i aria-hidden="true" {}
                        i aria-hidden="true" {}
                        span { "high skill" }
                    }
                }
                p class="physiology-visual-caption" {
                    "Shared presence creates one sample per day. Historical skill controls its resolution and "
                    "diagnostic calibration; gaps remain gaps, and later training never sharpens old entries."
                }
            }
            section id="limits" aria-labelledby="limits-heading" {
                h2 id="limits-heading" { "What Physiology does not do" }
                ul class="physiology-limit-grid" {
                    li { span aria-hidden="true" { "◉̸" } strong { "No disclosure" } small { "infections, phenotype, private meters" } }
                    li { span aria-hidden="true" { "?̸" } strong { "No diagnosis" } small { "or automatic recommendation" } }
                    li { span aria-hidden="true" { "⚗̸" } strong { "No crafting" } small { "Herbalism issue #214 owns preparations" } }
                    li { span aria-hidden="true" { "⌬̸" } strong { "No chemistry" } small { "that belongs to issue #215" } }
                    li { span aria-hidden="true" { "◷̸" } strong { "No tactical ticks" } small { "only strategic state persists" } }
                }
            }
        }
        aside class="right-sidebar physiology-reference-notes" {
            h2 { "Notebook convention" }
            p { "Observed: a dated, quantized claim from shared presence." }
            p { "Explained: an authoritative public model fragment, distinct from the observer's claim." }
        }
    };
    super::journal_layout(content, Some(character_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclosure_is_exact_accessible_and_names_non_goals() {
        let markup = physiology_reference_page("Ada").into_string();
        assert!(markup.contains("Exact public humour weights"));
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("aria-label="));
        assert!(markup.contains("issue #214"));
        assert!(markup.contains("physiology-model-flow"));
        assert!(markup.contains("physiology-weight-cell"));
        assert!(markup.contains("physiology-course-track"));
        assert!(markup.contains("No diagnosis"));
        assert!(markup.contains("TODO: rewrite all this information into NPC dialogue"));
        assert!(markup.contains("one sample per day"));
    }
}
