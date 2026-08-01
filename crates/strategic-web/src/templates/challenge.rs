use adventuresim_core::errantry::{
    FeyPresenterCatalogId, FeySpeechPart, PuzzleProjection, PuzzleSubmission, Sigil, WitnessPath,
    fey_clue_text, fey_puzzle_speech,
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
    last_submission: Option<&PuzzleSubmission>,
    tactical_insight_text: Option<&str>,
    tactical_preparation_text: Option<&str>,
    character_name: &str,
) -> Markup {
    let kind = projection.kind();
    let content = html! {
        aside class="left-sidebar challenge-rail" aria-hidden="true" {}
        main class="center-content challenge-page" data-live-navigation-static {
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
                            @if let Some(submission) = last_submission {
                                p class="player-spoken-line" data-puzzle-submission {
                                    strong { (character_name) ": " }
                                    (submission_text(submission))
                                }
                            }
                            @if solved {
                                @for line in fey_puzzle_speech(catalog, kind, FeySpeechPart::Correct) {
                                    p class="supernatural-spoken-line notice success" role="status" {
                                        strong { (catalog.name()) ": " }
                                        (line)
                                    }
                                }
                                @if let (Some(finding), Some(preparation)) = (tactical_insight_text, tactical_preparation_text) {
                                    div class="chat-system-message notice success" data-chat-channel="info"
                                        data-tactical-insight-source=(challenge_id) {
                                        p { strong { "Learned weakness: " } (finding) }
                                        p { strong { "Preparation: " } (preparation) }
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
                "The five sigils are Crown, Hart, Moon, Rose, and Sword. "
                "Each gate independently chooses one and only one rule below and uses it for both of its examples. "
                "Never combine rules within a gate; different gates may use the same rule."
            }
            ul aria-label="Possible rune transformation rules" {
                @for (index, rule) in puzzle.candidate_rules.into_iter().enumerate() {
                    li { "Candidate " ((b'A' + index as u8) as char) ": " (rule.rule_text()) }
                }
            }
            ol aria-label="Rune transformation examples" {
                @for example in &puzzle.examples {
                    li { ((*example).text()) }
                }
            }
            p class="chat-system-message" {
                "Question: the " (puzzle.query.label()) " passes through "
                @for (index, gate) in puzzle.route.into_iter().enumerate() {
                    @if index > 0 { ", then " }
                    (gate.label())
                }
                ". Which sigil finally emerges?"
            }
        },
    }
}

fn submission_text(submission: &PuzzleSubmission) -> String {
    match submission {
        PuzzleSubmission::OrderedSigils { ordering } => format!(
            "I place the sigils thus: {}.",
            ordering
                .iter()
                .map(|sigil| sigil.label())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PuzzleSubmission::TruthfulWitnesses { safe_path } => {
            format!("I choose the {}.", safe_path.label())
        }
        PuzzleSubmission::RuneTransformation { result } => {
            format!("The answer is the {}.", result.label())
        }
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
                legend { "Choose the sigil produced after all three gates" }
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
                None,
                "Ada",
            )
            .into_string();
            assert!(markup.contains("settlement-chat"));
            assert!(markup.contains("settlement-chat-messages"));
            assert!(markup.contains("data-live-navigation-static"));
            assert!(markup.contains("method=\"post\""));
            assert!(markup.contains("<fieldset>"));
            assert!(markup.contains("role=\"alert\""));
            assert!(!markup.contains("solution"));
            assert!(!markup.contains("data-private-seed"));
            assert!(!markup.contains("operation"));
        }
    }

    #[test]
    fn submitted_answer_remains_in_the_solved_chat_until_the_player_leaves() {
        let puzzle = PuzzleAuthority::generate(
            adventuresim_core::errantry::PuzzleKind::RuneTransformation,
            4,
        );
        let submission = PuzzleSubmission::RuneTransformation {
            result: Sigil::Sword,
        };
        let markup = puzzle_page(
            "challenge:test",
            "case:test",
            FeyPresenterCatalogId::LadyBeneathThornV1,
            1,
            &puzzle.projection(),
            true,
            Some(true),
            Some(&submission),
            Some("The armed retainers carry no missile weapons and must close to melee range before striking."),
            Some("Bring bows and arrows; archers can strike while these enemies close."),
            "Ada",
        )
        .into_string();

        assert!(markup.contains("data-puzzle-submission"));
        assert!(markup.contains("Ada: "));
        assert!(markup.contains("The answer is the Sword."));
        assert!(markup.contains("Learned weakness:"));
        assert!(markup.contains("bows and arrows"));
        assert!(!markup.contains("combat scale"));
        assert!(markup.contains("Return to camp"));
        assert!(
            include_str!("../../static/live-state.js").contains("[data-live-navigation-static]")
        );
    }

    #[test]
    fn rune_prompt_requires_three_inferred_gate_laws_in_route_order() {
        let puzzle = PuzzleAuthority::generate(
            adventuresim_core::errantry::PuzzleKind::RuneTransformation,
            19,
        );
        let markup = puzzle_page(
            "challenge:test",
            "case:test",
            FeyPresenterCatalogId::LadyBeneathThornV1,
            0,
            &puzzle.projection(),
            false,
            None,
            None,
            None,
            None,
            "Ada",
        )
        .into_string();

        assert!(markup.contains("one and only one rule"));
        assert!(markup.contains("Never combine rules within a gate"));
        assert!(markup.contains("Candidate A"));
        assert!(markup.contains("Gate of Ash"));
        assert!(markup.contains("Gate of Briar"));
        assert!(markup.contains("Gate of Glass"));
        assert!(markup.contains("after all three gates"));
    }
}
