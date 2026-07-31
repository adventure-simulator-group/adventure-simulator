use adventuresim_core::errantry::{
    ChallengePresenter, OrderedSigilProjection, PresenterKind, Sigil, presented_clue_text,
};
use maud::{Markup, html};

use super::journal_layout;

pub fn ordered_sigil_page(
    challenge_id: &str,
    case_id: &str,
    revision: u32,
    projection: &OrderedSigilProjection,
    presenter: &ChallengePresenter,
    solved: bool,
    last_attempt_correct: Option<bool>,
    character_name: &str,
) -> Markup {
    let supernatural = presenter.kind == PresenterKind::FeySpoken;
    let content = html! {
        aside class="left-sidebar challenge-rail" aria-hidden="true" {}
        main class="center-content challenge-page" {
            article class="panel challenge-panel" aria-labelledby="challenge-title"
                data-challenge-id=(challenge_id)
                data-presenter-kind=(if supernatural { "supernatural-spoken" } else { "contraption" }) {
                header {
                    p class="eyebrow" { "A trial of discernment" }
                    h1 id="challenge-title" { (&presenter.title) }
                    p class="challenge-presenter" { (&presenter.name) }
                }
                section aria-labelledby="challenge-address" {
                    h2 id="challenge-address" class="visually-hidden" { "The challenge" }
                    @for line in &presenter.introduction {
                        p class=(if supernatural { "supernatural-spoken-line" } else { "inscription-line" }) {
                            (line)
                        }
                    }
                    @for line in &presenter.instruction {
                        p class=(if supernatural { "supernatural-spoken-line" } else { "inscription-line" }) {
                            (line)
                        }
                    }
                }
                section class="challenge-clues" aria-labelledby="challenge-clues-title" {
                    h2 id="challenge-clues-title" { "The clues" }
                    @if supernatural {
                        blockquote class="supernatural-clue-speech" {
                            ol aria-label="The Lady's spoken clues" {
                                @for clue in &projection.clues {
                                    li class="supernatural-spoken-line" {
                                        (presented_clue_text(presenter.kind, clue))
                                    }
                                }
                            }
                            cite { "The Lady Beneath the Thorn" }
                        }
                    } @else {
                        ol class="mechanism-clue-inscriptions" aria-label="Engraved clues" {
                            @for clue in &projection.clues {
                                li class="inscription-line" {
                                    (presented_clue_text(presenter.kind, clue))
                                }
                            }
                        }
                    }
                }
                @if solved {
                    div class="notice success" role="status" data-challenge-correct {
                        @for line in &presenter.correct_feedback {
                            p class=(if supernatural { "supernatural-spoken-line" } else { "mechanism-feedback" }) {
                                (line)
                            }
                        }
                        a class="btn btn-primary" href="/quests" { "Return to the journal" }
                    }
                } @else {
                    @if last_attempt_correct == Some(false) {
                        div class="notice warning" role="alert" data-challenge-wrong {
                            @for line in &presenter.wrong_feedback {
                                p class=(if supernatural { "supernatural-spoken-line" } else { "mechanism-feedback" }) {
                                    (line)
                                }
                            }
                        }
                    }
                    form method="post"
                        action=(format!("/quests/{case_id}/challenges/{challenge_id}"))
                        class="challenge-ordering-form" {
                        input type="hidden" name="expected_revision" value=(revision);
                        fieldset {
                            legend { "Arrange the five sigils from first to fifth" }
                            div class="challenge-ordering" {
                                @for position in 0..projection.sigils.len() {
                                    label {
                                        span { (ordinal(position)) }
                                        select name=(format!("sigil_{position}")) required {
                                            option value="" { "Choose a sigil" }
                                            @for sigil in projection.sigils {
                                                option value=(sigil.label()) { (sigil.label()) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        button type="submit" class="btn btn-primary" { "Submit the ordering" }
                    }
                }
            }
        }
        aside class="right-sidebar challenge-rail" aria-hidden="true" {}
    };
    journal_layout(content, Some(character_name))
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
    let parse = |value| match value {
        "Crown" => Ok(Sigil::Crown),
        "Hart" => Ok(Sigil::Hart),
        "Moon" => Ok(Sigil::Moon),
        "Rose" => Ok(Sigil::Rose),
        "Sword" => Ok(Sigil::Sword),
        _ => Err("Choose each of the five named sigils"),
    };
    Ok([
        parse(values[0])?,
        parse(values[1])?,
        parse(values[2])?,
        parse(values[3])?,
        parse(values[4])?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::errantry::{
        ORDERED_SIGIL_RULES_VERSION, OrderedSigilClue, OrderedSigilPuzzle, presenter,
    };

    #[test]
    fn page_is_no_js_required_and_accessibly_labelled() {
        let puzzle = OrderedSigilPuzzle::generate(4);
        let markup = ordered_sigil_page(
            "challenge:test",
            "case:test",
            0,
            &puzzle.projection(),
            &presenter(PresenterKind::RuinContraption),
            false,
            Some(false),
            "Ada",
        )
        .into_string();
        assert!(markup.contains("method=\"post\""));
        assert!(markup.contains("<fieldset>"));
        assert!(markup.contains("<legend>"));
        assert!(markup.contains("role=\"alert\""));
        assert!(markup.contains("mechanism-clue-inscriptions"));
        assert!(!markup.contains("solution"));
        assert!(!markup.contains("data-private-seed"));
        let projection_json = serde_json::to_string(&puzzle.projection()).unwrap();
        assert!(!projection_json.contains("\"seed\""));
        assert!(!projection_json.contains("\"solution\""));
    }

    #[test]
    fn fey_clues_are_marked_as_the_supernatural_presenters_speech() {
        let mut clues = Vec::new();
        for sigil in Sigil::ALL {
            for position in 0..5 {
                clues.push(OrderedSigilClue::Exact { sigil, position });
                clues.push(OrderedSigilClue::NotAt { sigil, position });
            }
            for other in Sigil::ALL {
                if sigil != other {
                    clues.push(OrderedSigilClue::Before {
                        first: sigil,
                        second: other,
                    });
                    clues.push(OrderedSigilClue::Adjacent {
                        first: sigil,
                        second: other,
                    });
                }
            }
        }
        assert_eq!(clues.len(), 90);
        let projection = OrderedSigilProjection {
            rules_version: ORDERED_SIGIL_RULES_VERSION,
            sigils: Sigil::ALL,
            clues,
        };
        let markup = ordered_sigil_page(
            "challenge:test",
            "case:test",
            0,
            &projection,
            &presenter(PresenterKind::FeySpoken),
            false,
            None,
            "Ada",
        )
        .into_string();
        assert!(markup.contains("supernatural-clue-speech"));
        assert!(markup.contains("aria-label=\"The Lady's spoken clues\""));
        assert!(markup.contains("<cite>The Lady Beneath the Thorn</cite>"));
        for clue in &projection.clues {
            assert!(markup.contains(presented_clue_text(PresenterKind::FeySpoken, clue)));
            assert!(!markup.contains(&clue.text()));
        }
    }
}
