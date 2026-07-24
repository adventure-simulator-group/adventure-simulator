use crate::spacetimedb::{
    BackendInvestigationAction, BackendInvestigationActionOutcome,
    BackendInvestigationJournalEntry, BackendInvestigationLead,
};
use maud::{Markup, html};

pub fn journal_page(
    entries: &[BackendInvestigationJournalEntry],
    leads: &[BackendInvestigationLead],
    actions: &[BackendInvestigationAction],
    outcomes: &[BackendInvestigationActionOutcome],
    character_name: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            section class="sidebar-section" {
                h3 class="sidebar-header" { "How this journal works" }
                p class="small-copy" {
                    "It records what " (character_name) " has learned, including uncertain and conflicting accounts. It does not reveal objective truth."
                }
            }
        }
        main class="center-content investigation-journal" data-investigation-journal {
            header {
                h1 { "Investigation journal" }
                p class="text-muted" { "Claims, evidence, referrals, and places known to this character." }
            }
            @if entries.is_empty() && leads.is_empty() {
                section class="strategic-notice" { p { "No problems or leads have reached you yet." } }
            }
            @if !outcomes.is_empty() {
                section class="journal-actions-outcomes" {
                    h2 { "Recent investigation results" }
                    @for outcome in outcomes.iter().rev().take(5) {
                        article class="journal-card journal-action-outcome" {
                            p { (&outcome.wording) }
                        }
                    }
                }
            }
            @if !actions.is_empty() {
                section class="journal-actions" {
                    h2 { "Ways to investigate" }
                    @for action in actions {
                        article class="journal-card journal-action" data-action-id=(&action.action_id) {
                            h3 { (&action.summary) }
                            p { (&action.known_prerequisites) }
                            p class="journal-action-cost" {
                                "Estimated duration: " (action.duration_min_minutes) "–"
                                (action.duration_max_minutes) " minutes. Needs and fatigue are settled authoritatively when the action is performed."
                            }
                            p class="journal-action-skills" {
                                "Relevant contributions: " (&action.skill_contributions)
                                ". Current uncertainty: " (action.uncertainty_bps / 100) "%."
                            }
                            @if !action.weather_available {
                                p class="text-muted" { "Weather effects are unavailable and are not estimated." }
                            }
                            form method="post" action="/investigations/actions" {
                                input type="hidden" name="action_id" value=(&action.action_id);
                                input type="hidden" name="method" value=(&action.method);
                                input type="hidden" name="expected_version" value=(action.expected_version);
                                button type="submit" { "Attempt " (action.method.replace('_', " ")) }
                            }
                        }
                    }
                }
            }
            @for lead in leads {
                article class="journal-card journal-lead" data-case-id=(&lead.case_id) data-lead-id=(&lead.lead_id) {
                    h2 { (&lead.summary) }
                    p class="journal-source" { "Source: " (&lead.source_label) " / confidence " (lead.confidence_bps / 100) "%" }
                    @if !lead.contradiction_group.is_empty() {
                        p class="journal-contradiction" { "Conflicts with another account." }
                    }
                    @if !lead.corrected_by.is_empty() {
                        p class="journal-correction" { "Corrected by " (&lead.corrected_by) }
                    }
                    @if !lead.witness_name.is_empty() || !lead.witness_description.is_empty() {
                        section class="journal-referral" {
                            h3 { "Person to ask" }
                            @if !lead.witness_name.is_empty() { p { strong { (&lead.witness_name) } } }
                            @if !lead.witness_description.is_empty() { p { (&lead.witness_description) } }
                            @if !lead.witness_occupation_or_relationship.is_empty() {
                                p { "Known as: " (&lead.witness_occupation_or_relationship) }
                            }
                            @if !lead.expected_location.is_empty() {
                                p { "Expected at: " (&lead.expected_location) }
                            }
                            @if !lead.current_learned_location.is_empty() {
                                p { "Last learned location: " (&lead.current_learned_location) }
                            }
                        }
                    }
                    @match lead.destination_stage.as_str() {
                        "textual" | "landmark" | "approximate_area" | "route_segment" => {
                            p class="journal-directions" { "Directions: " (&lead.directions) }
                        },
                        "exact_believed" | "visited" => {
                            p class="journal-destination" data-exact-destination=(&lead.exact_location_id) {
                                "Believed exact destination: " (&lead.exact_location_id)
                            }
                        },
                        _ => {}
                    }
                }
            }
            @for entry in entries {
                article class="journal-card journal-revision" data-record-id=(&entry.record_id) {
                    p { (&entry.summary) }
                    p class="journal-source" { "Source: " (&entry.source_label) " / confidence " (entry.confidence_bps / 100) "%" }
                    @if !entry.supersedes.is_empty() {
                        p class="journal-correction" { "Revises " (&entry.supersedes) }
                    }
                }
            }
        }
        aside class="right-sidebar" {
            section class="sidebar-section" {
                h3 class="sidebar-header" { "Navigation" }
                p class="small-copy" {
                    "Only an exact believed location creates a destination pin. Directions and search areas remain descriptive."
                }
            }
        }
    };
    super::journal_layout(content, Some(character_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_shows_referral_uncertainty_and_only_exact_destination_data() {
        let lead = BackendInvestigationLead {
            owner_character_id: 1,
            case_id: "case".into(),
            lead_id: "lead".into(),
            summary: "Screams after dark".into(),
            source_label: "the innkeeper".into(),
            confidence_bps: 5500,
            destination_stage: "textual".into(),
            directions: "beyond the mill".into(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: "Marta".into(),
            witness_description: "tall, red-haired cooper".into(),
            witness_occupation_or_relationship: "cooper".into(),
            expected_location: "workshops".into(),
            current_learned_location: String::new(),
            contradiction_group: "shape".into(),
            corrected_by: String::new(),
            recorded_at: 1,
        };
        let markup = journal_page(&[], &[lead], &[], &[], "Ada").into_string();
        assert!(markup.contains("Source: the innkeeper"));
        assert!(markup.contains("confidence 55%"));
        assert!(markup.contains("Conflicts with another account"));
        assert!(markup.contains("tall, red-haired cooper"));
        assert!(markup.contains("Expected at: workshops"));
        assert!(markup.contains("Directions: beyond the mill"));
        assert!(!markup.contains("data-exact-destination"));
    }
}
