use crate::spacetimedb::{
    BackendInvestigationAction, BackendInvestigationActionOutcome,
    BackendInvestigationJournalEntry, BackendInvestigationLead,
};
use maud::{Markup, html};

pub fn journal_page(
    entries: &[BackendInvestigationJournalEntry],
    leads: &[BackendInvestigationLead],
    _actions: &[BackendInvestigationAction],
    _outcomes: &[BackendInvestigationActionOutcome],
    character_name: &str,
    feedback: Option<&str>,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" aria-hidden="true" {}
        main class="center-content investigation-journal" data-investigation-journal {
            header {
                h1 { "Journal" }
                p class="text-muted" { "What " (character_name) " has learned about local problems." }
            }
            @if let Some(feedback) = feedback {
                section class="strategic-notice journal-feedback" role="alert" {
                    p { (feedback) }
                }
            }
            @if entries.is_empty() && leads.is_empty() {
                section class="strategic-notice" { p { "No problems or leads have reached you yet." } }
            }
            @for lead in leads.iter().filter(|lead| lead.source_label != "witness referral") {
                article class="journal-card journal-lead" {
                    h2 { (&lead.summary) }
                    p class="journal-source" { "Source: " (&lead.source_label) }
                }
            }
            @for entry in entries {
                article class="journal-card journal-revision" {
                    p { (&entry.summary) }
                    p class="journal-source" { "Source: " (&entry.source_label) }
                }
            }
        }
        aside class="right-sidebar" aria-hidden="true" {}
    };
    super::journal_layout(content, Some(character_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_records_only_the_report_and_its_source() {
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
        let markup = journal_page(&[], &[lead], &[], &[], "Ada", None).into_string();
        assert!(markup.contains("Screams after dark"));
        assert!(markup.contains("Source: the innkeeper"));
        assert!(!markup.contains("confidence"));
        assert!(!markup.contains("55%"));
        assert!(!markup.contains("Conflicts with another account"));
        assert!(!markup.contains("tall, red-haired cooper"));
        assert!(!markup.contains("Expected at: workshops"));
        assert!(!markup.contains("Directions: beyond the mill"));
        assert!(!markup.contains("data-exact-destination"));
    }

    #[test]
    fn journal_omits_referrals_and_mechanical_action_outcomes() {
        let referral = BackendInvestigationLead {
            owner_character_id: 1,
            case_id: "case".into(),
            lead_id: "referral".into(),
            summary: "Ask Marta at the mill.".into(),
            source_label: "witness referral".into(),
            confidence_bps: 9_000,
            destination_stage: "textual".into(),
            directions: "the mill".into(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: "Marta".into(),
            witness_description: "the miller".into(),
            witness_occupation_or_relationship: "miller".into(),
            expected_location: "mill".into(),
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: 1,
        };
        let outcome = BackendInvestigationActionOutcome {
            owner_character_id: 1,
            outcome_id: "outcome".into(),
            action_id: "inspect".into(),
            wording: "Uncertainty fell to 20%; try another route.".into(),
            recorded_at: 2,
        };
        let markup = journal_page(&[], &[referral], &[], &[outcome], "Ada", None).into_string();
        assert!(!markup.contains("Ask Marta"));
        assert!(!markup.contains("20%"));
        assert!(!markup.contains("another route"));
    }

    #[test]
    fn journal_does_not_interpret_a_learned_site_for_the_player() {
        let lead = BackendInvestigationLead {
            owner_character_id: 1,
            case_id: "case".into(),
            lead_id: "lead".into(),
            summary: "The trail ends at an old croft.".into(),
            source_label: "your party's investigation".into(),
            confidence_bps: 8_000,
            destination_stage: "exact_believed".into(),
            directions: String::new(),
            exact_location_id: "site:private-hash".into(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: String::new(),
            witness_description: String::new(),
            witness_occupation_or_relationship: String::new(),
            expected_location: String::new(),
            current_learned_location: "The abandoned croft".into(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: 1,
        };
        let markup = journal_page(&[], &[lead], &[], &[], "Ada", None).into_string();
        assert!(markup.contains("The trail ends at an old croft."));
        assert!(markup.contains("your party"));
        assert!(markup.contains("investigation"));
        assert!(!markup.contains("Believed exact destination"));
        assert!(!markup.contains("The abandoned croft"));
        assert!(!markup.contains("data-exact-destination"));
        assert!(!markup.contains("site:private-hash"));
    }

    #[test]
    fn journal_feedback_does_not_expose_investigation_suggestions() {
        let action = BackendInvestigationAction {
            owner_character_id: 1,
            action_id: "inspect".into(),
            method: "inspect_site".into(),
            expected_version: 2,
            summary: "Inspect the abandoned croft.".into(),
            known_prerequisites: "Reach the croft.".into(),
            duration_min_minutes: 30,
            duration_max_minutes: 90,
            uncertainty_bps: 2_000,
            skill_contributions: "awareness".into(),
            weather_available: false,
            required_case_site_id: "site-public".into(),
            available: false,
            can_travel_to_required_site: true,
            unavailable_reason: "Travel to the known investigation site before inspecting it."
                .into(),
        };
        let markup = journal_page(
            &[],
            &[],
            &[action],
            &[],
            "Ada",
            Some("That investigation route is no longer available."),
        )
        .into_string();
        assert!(markup.contains("role=\"alert\""));
        assert!(markup.contains("That investigation route is no longer available."));
        assert!(!markup.contains("Inspect the abandoned croft."));
        assert!(!markup.contains("Travel to the known investigation site"));
        assert!(!markup.contains("action=\"/case-sites/site-public/travel\""));
        assert!(!markup.contains("action=\"/quests/actions\""));
        assert!(!markup.contains("20%"));
        assert!(!markup.contains("Relevant contributions"));
    }

    #[test]
    fn journal_hides_available_actions_too() {
        let action = BackendInvestigationAction {
            owner_character_id: 1,
            action_id: "inspect".into(),
            method: "inspect_site".into(),
            expected_version: 2,
            summary: "Inspect the abandoned croft.".into(),
            known_prerequisites: "Reach the croft.".into(),
            duration_min_minutes: 30,
            duration_max_minutes: 90,
            uncertainty_bps: 2_000,
            skill_contributions: "awareness".into(),
            weather_available: false,
            required_case_site_id: "site-public".into(),
            available: true,
            can_travel_to_required_site: false,
            unavailable_reason: String::new(),
        };
        let markup = journal_page(&[], &[], &[action], &[], "Ada", None).into_string();
        assert!(!markup.contains("Inspect the abandoned croft."));
        assert!(!markup.contains("action=\"/quests/actions\""));
        assert!(!markup.contains("journal-action-unavailable"));
    }

    #[test]
    fn journal_hides_unavailable_action_commentary() {
        let action = BackendInvestigationAction {
            owner_character_id: 1,
            action_id: "inspect".into(),
            method: "inspect_site".into(),
            expected_version: 2,
            summary: "Inspect the abandoned croft.".into(),
            known_prerequisites: "Reach the croft.".into(),
            duration_min_minutes: 30,
            duration_max_minutes: 90,
            uncertainty_bps: 2_000,
            skill_contributions: "awareness".into(),
            weather_available: false,
            required_case_site_id: "site-public".into(),
            available: false,
            can_travel_to_required_site: false,
            unavailable_reason:
                "An incapacitated party member must recover before the party can investigate."
                    .into(),
        };
        let markup = journal_page(&[], &[], &[action], &[], "Ada", None).into_string();
        assert!(!markup.contains("incapacitated party member"));
        assert!(!markup.contains("Travel to investigation site"));
        assert!(!markup.contains("action=\"/case-sites/site-public/travel\""));
        assert!(!markup.contains("action=\"/quests/actions\""));
    }
}
