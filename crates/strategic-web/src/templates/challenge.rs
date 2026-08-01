use adventuresim_core::errantry::{
    FeyPresenterCatalogId, FeySpeechPart, PuzzleProjection, Sigil, WitnessPath, fey_clue_text,
    fey_puzzle_speech,
};
use maud::{Markup, html};

use super::journal_layout;

pub fn puzzle_page(
    challenge_id: &str,
    case_id: &str,
    catalog: FeyPresenterCatalogId,
    revision: u32,
    projection: &PuzzleProjection,
    solved: bool,
    last_attempt_correct: Option<bool>,
    boon_item_id: Option<&str>,
    boon_reduction_bps: Option<u32>,
    character_name: &str,
) -> Markup {
    let kind = projection.kind();
    let content = html! {
        aside class="left-sidebar challenge-rail" aria-hidden="true" {}
        main class="center-content challenge-page" {
            section class="settlement-chat challenge-chat" aria-label="Fey conversation"
                data-challenge-id=(challenge_id) data-presenter-catalog=(catalog.slug())
                data-puzzle-kind=(kind.slug()) {
                div class="settlement-chat-layout" {
                    div class="settlement-chat-conversation" {
                        div class="settlement-chat-messages" aria-live="polite" {
                            div class="chat-system-message" data-chat-channel="info" {
                                "A trial of discernment interrupts the road."
                            }
                            @for line in fey_puzzle_speech(catalog, kind, FeySpeechPart::Introduction) {
                                p class="supernatural-spoken-line" {
                                    strong { (catalog.name()) ": " }
                                    (line)
                                }
                            }
                            @for line in fey_puzzle_speech(catalog, kind, FeySpeechPart::Instruction) {
                                p class="supernatural-spoken-line" {
                                    strong { (catalog.name()) ": " }
                                    (line)
                                }
                            }
                            (puzzle_observations(catalog, projection))
                            @if solved {
                                @for line in fey_puzzle_speech(catalog, kind, FeySpeechPart::Correct) {
                                    p class="supernatural-spoken-line notice success" role="status" {
                                        strong { (catalog.name()) ": " }
                                        (line)
                                    }
                                }
                                @if let (Some(item_id), Some(reduction)) = (boon_item_id, boon_reduction_bps) {
                                    div class="chat-system-message notice success" data-chat-channel="info"
                                        data-countermeasure-source=(challenge_id) {
                                        "Received " (item_id) ". The boon reduces the bound finale's enemy combat scale by "
                                        (reduction / 100) "% when that mission is first bound."
                                    }
                                }
                                a class="btn btn-primary" href="/camp" { "Return to camp" }
                            } @else {
                                @if last_attempt_correct == Some(false) {
                                    @for line in fey_puzzle_speech(catalog, kind, FeySpeechPart::Wrong) {
                                        p class="supernatural-spoken-line notice warning" role="alert" {
                                            strong { (catalog.name()) ": " }
                                            (line)
                                        }
                                    }
                                }
                                form method="post"
                                    action=(format!("/quests/{case_id}/challenges/{challenge_id}"))
                                    class="challenge-ordering-form settlement-chat-composer" {
                                    input type="hidden" name="expected_revision" value=(revision);
                                    (puzzle_answer_fields(projection))
                                    button type="submit" class="btn btn-primary" { "Answer the Lady" }
                                }
                            }
                        }
                    }
                }
            }
        }
        aside class="right-sidebar challenge-rail" aria-hidden="true" {}
    };
    journal_layout(content, Some(character_name))
}

fn puzzle_observations(catalog: FeyPresenterCatalogId, projection: &PuzzleProjection) -> Markup {
    match projection {
        PuzzleProjection::OrderedSigils(puzzle) => html! {
            ol aria-label="The Lady's spoken clues" {
                @for clue in &puzzle.clues {
                    li class="supernatural-spoken-line" { (fey_clue_text(catalog, clue)) }
                }
            }
        },
        PuzzleProjection::TruthfulWitnesses(puzzle) => html! {
            div class="chat-system-message" data-chat-channel="info" {
                "Exactly one witness lies. Each witness is otherwise consistent, and exactly one path is safe."
            }
            ol aria-label="Witness statements" {
                @for statement in puzzle.statements {
                    li { (statement.text()) }
                }
            }
        },
        PuzzleProjection::RuneTransformation(puzzle) => html! {
            div class="chat-system-message" data-chat-channel="info" {
                "The same hidden transformation governs every example."
            }
            ol aria-label="Rune transformation examples" {
                @for example in &puzzle.examples {
                    li { ((*example).text()) }
                }
            }
            p class="chat-system-message" {
                "Question: when the " (puzzle.query.label()) " enters, which sigil emerges?"
            }
        },
    }
}

fn puzzle_answer_fields(projection: &PuzzleProjection) -> Markup {
    match projection {
        PuzzleProjection::OrderedSigils(puzzle) => html! {
            fieldset {
                legend { "Arrange the five sigils from first to fifth" }
                div class="challenge-ordering" {
                    @for position in 0..puzzle.sigils.len() {
                        label {
                            span { (ordinal(position)) }
                            select name=(format!("sigil_{position}")) required {
                                option value="" { "Choose a sigil" }
                                @for sigil in puzzle.sigils {
                                    option value=(sigil.label()) { (sigil.label()) }
                                }
                            }
                        }
                    }
                }
            }
        },
        PuzzleProjection::TruthfulWitnesses(puzzle) => html! {
            fieldset {
                legend { "Choose the path proved safe" }
                label {
                    span { "Safe path" }
                    select name="safe_path" required {
                        option value="" { "Choose a path" }
                        @for path in puzzle.paths {
                            option value=(path.label()) { (path.label()) }
                        }
                    }
                }
            }
        },
        PuzzleProjection::RuneTransformation(puzzle) => html! {
            fieldset {
                legend { "Choose the sigil produced by the hidden rule" }
                label {
                    span { "Result" }
                    select name="rune_result" required {
                        option value="" { "Choose a sigil" }
                        @for sigil in puzzle.sigils {
                            option value=(sigil.label()) { (sigil.label()) }
                        }
                    }
                }
            }
        },
    }
}

fn ordinal(position: usize) -> &'static str {
    match position {
        0 => "First",
        1 => "Second",
        2 => "Third",
        3 => "Fourth",
        _ => "Fifth",
    }
}

pub fn parse_form_sigils(values: [&str; 5]) -> Result<[Sigil; 5], &'static str> {
    Ok([
        parse_sigil(values[0])?,
        parse_sigil(values[1])?,
        parse_sigil(values[2])?,
        parse_sigil(values[3])?,
        parse_sigil(values[4])?,
    ])
}

pub fn parse_sigil(value: &str) -> Result<Sigil, &'static str> {
    match value {
        "Crown" => Ok(Sigil::Crown),
        "Hart" => Ok(Sigil::Hart),
        "Moon" => Ok(Sigil::Moon),
        "Rose" => Ok(Sigil::Rose),
        "Sword" => Ok(Sigil::Sword),
        _ => Err("Choose one of the named sigils"),
    }
}

pub fn parse_witness_path(value: &str) -> Result<WitnessPath, &'static str> {
    match value {
        "Ash path" => Ok(WitnessPath::Ash),
        "Moon path" => Ok(WitnessPath::Moon),
        "Thorn path" => Ok(WitnessPath::Thorn),
        _ => Err("Choose one of the named paths"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::errantry::PuzzleAuthority;

    #[test]
    fn every_puzzle_is_a_no_js_shared_chat_visual_without_private_truth() {
        for kind in adventuresim_core::errantry::PuzzleKind::ALL {
            let puzzle = PuzzleAuthority::generate(kind, 4);
            let markup = puzzle_page(
                "challenge:test",
                "case:test",
                FeyPresenterCatalogId::LadyBeneathThornV1,
                0,
                &puzzle.projection(),
                false,
                Some(false),
                None,
                None,
                "Ada",
            )
            .into_string();
            assert!(markup.contains("settlement-chat"));
            assert!(markup.contains("settlement-chat-messages"));
            assert!(markup.contains("method=\"post\""));
            assert!(markup.contains("<fieldset>"));
            assert!(markup.contains("role=\"alert\""));
            assert!(!markup.contains("solution"));
            assert!(!markup.contains("data-private-seed"));
            assert!(!markup.contains("operation"));
        }
    }
}
