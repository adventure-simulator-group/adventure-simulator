use adventuresim_core::errantry::{
    FEY_PRESENTER_NAME, FeyPresenterCatalogId, FeySpeechPart, OrderedSigilProjection, Sigil,
    fey_clue_text, fey_speech,
};
use maud::{Markup, html};

use super::journal_layout;

pub fn ordered_sigil_page(
    challenge_id: &str,
    case_id: &str,
    revision: u32,
    projection: &OrderedSigilProjection,
    solved: bool,
    last_attempt_correct: Option<bool>,
    boon_item_id: Option<&str>,
    boon_reduction_bps: Option<u32>,
    character_name: &str,
) -> Markup {
    let catalog = FeyPresenterCatalogId::LadyBeneathThornV1;
    let content = html! {
        aside class="left-sidebar challenge-rail" aria-hidden="true" {}
        main class="center-content challenge-page" {
            section class="settlement-chat challenge-chat" aria-label="Fey conversation"
                data-challenge-id=(challenge_id) data-presenter-catalog="lady-beneath-thorn-v1" {
                div class="settlement-chat-layout" {
                    div class="settlement-chat-conversation" {
                        div class="settlement-chat-messages" aria-live="polite" {
                            div class="chat-system-message" data-chat-channel="info" {
                                "A trial of discernment interrupts the road."
                            }
                            @for line in fey_speech(catalog, FeySpeechPart::Introduction) {
                                p class="supernatural-spoken-line" {
                                    strong { (FEY_PRESENTER_NAME) ": " }
                                    (line)
                                }
                            }
                            @for line in fey_speech(catalog, FeySpeechPart::Instruction) {
                                p class="supernatural-spoken-line" {
                                    strong { (FEY_PRESENTER_NAME) ": " }
                                    (line)
                                }
                            }
                            ol aria-label="The Lady's spoken clues" {
                                @for clue in &projection.clues {
                                    li class="supernatural-spoken-line" {
                                        (fey_clue_text(catalog, clue))
                                    }
                                }
                            }
                            @if solved {
                                @for line in fey_speech(catalog, FeySpeechPart::Correct) {
                                    p class="supernatural-spoken-line notice success" role="status" {
                                        strong { (FEY_PRESENTER_NAME) ": " }
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
                                    @for line in fey_speech(catalog, FeySpeechPart::Wrong) {
                                        p class="supernatural-spoken-line notice warning" role="alert" {
                                            strong { (FEY_PRESENTER_NAME) ": " }
                                            (line)
                                        }
                                    }
                                }
                                form method="post"
                                    action=(format!("/quests/{case_id}/challenges/{challenge_id}"))
                                    class="challenge-ordering-form settlement-chat-composer" {
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
    use adventuresim_core::errantry::OrderedSigilPuzzle;

    #[test]
    fn whole_trial_is_a_no_js_shared_chat_visual() {
        let puzzle = OrderedSigilPuzzle::generate(4);
        let markup = ordered_sigil_page(
            "challenge:test",
            "case:test",
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
    }
}
