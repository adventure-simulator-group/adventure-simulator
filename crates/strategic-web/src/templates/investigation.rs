#[cfg(test)]
use crate::spacetimedb::DestinationKnowledgeStage;
use crate::spacetimedb::{
    BackendBestiaryDeduction, BackendInvestigationCaseSummary, BackendInvestigationJournalEntry,
    BackendInvestigationLead, BestiaryDeductionExt,
};
use maud::{Markup, html};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Clone, Debug)]
struct JournalRecord {
    recorded_at: u64,
    summary: String,
    source: String,
}

fn bestiary_journal_results(results: &[BackendBestiaryDeduction]) -> Markup {
    html! {
        @if !results.is_empty() {
            section class="bestiary-check-results journal-bestiary-results" {
                strong class="bestiary-check-heading" { "Possible monster kinds:" }
                span class="bestiary-result-list" {
                    @for result in results {
                        span class=(format!("bestiary-result-chip support-{}", result.support_band)) {
                            (&result.monster_kind) " — " (&result.support_band)
                        }
                    }
                }
                ul class="bestiary-provenance" {
                    @for source in results.iter()
                        .flat_map(BackendBestiaryDeduction::provenance)
                        .collect::<BTreeSet<_>>() {
                        li { (source) }
                    }
                }
            }
        }
    }
}

pub fn journal_page(
    entries: &[BackendInvestigationJournalEntry],
    leads: &[BackendInvestigationLead],
    cases: &[BackendInvestigationCaseSummary],
    deductions: &[BackendBestiaryDeduction],
    character_name: &str,
    feedback: Option<&str>,
) -> Markup {
    let mut ordered_cases = cases.to_vec();
    ordered_cases.sort_by_key(|case| {
        (
            case.status != "open",
            Reverse(case.latest_update_at),
            case.case_id.clone(),
        )
    });
    let mut records: BTreeMap<String, Vec<JournalRecord>> = BTreeMap::new();
    for entry in entries {
        records
            .entry(entry.case_id.clone())
            .or_default()
            .push(JournalRecord {
                recorded_at: entry.recorded_at,
                summary: entry.summary.clone(),
                source: entry.source_label.clone(),
            });
    }
    for lead in leads
        .iter()
        .filter(|lead| lead.source_label != "witness referral")
    {
        records
            .entry(lead.case_id.clone())
            .or_default()
            .push(JournalRecord {
                recorded_at: lead.recorded_at,
                summary: lead.summary.clone(),
                source: lead.source_label.clone(),
            });
    }
    for rows in records.values_mut() {
        rows.sort_by_key(|record| {
            (
                record.recorded_at,
                record.summary.clone(),
                record.source.clone(),
            )
        });
        rows.dedup_by(|left, right| {
            left.recorded_at == right.recorded_at
                && left.summary == right.summary
                && left.source == right.source
        });
    }

    let content = html! {
        aside class="left-sidebar journal-case-index" data-journal-case-index {
            header class="journal-rail-header" {
                h2 { "Journal" }
                p class="text-muted" { "Problems known to " (character_name) }
            }
            @if ordered_cases.is_empty() {
                p class="text-muted" { "No problems have reached you yet." }
            } @else {
                nav class="journal-case-tabs" role="tablist" aria-label="Known quests" {
                    @for (index, case) in ordered_cases.iter().enumerate() {
                        button type="button"
                            id=(format!("journal-case-tab-{index}"))
                            class=(if index == 0 { "journal-case-tab active" } else { "journal-case-tab" })
                            role="tab" aria-selected=(index == 0)
                            tabindex=(if index == 0 { "0" } else { "-1" })
                            aria-controls=(format!("journal-case-panel-{index}"))
                            data-journal-case-select=(case.case_id.as_str()) {
                            span class="journal-case-title" { (&case.subject) }
                            span class=(format!("journal-case-status journal-case-status-{}", case.status)) {
                                (status_label(&case.status))
                            }
                        }
                    }
                }
            }
        }
        main class="center-content investigation-journal journal-case-log"
            data-investigation-journal data-journal-case-log {
            @if let Some(feedback) = feedback {
                section class="strategic-notice journal-feedback" role="alert" { p { (feedback) } }
            }
            @if ordered_cases.is_empty() {
                p class="text-muted" { "The journal is empty." }
            }
            @for (index, case) in ordered_cases.iter().enumerate() {
                section id=(format!("journal-case-panel-{index}"))
                    class="journal-case-panel" role="tabpanel"
                    aria-labelledby=(format!("journal-case-tab-{index}"))
                    data-journal-case-panel=(case.case_id.as_str())
                    hidden[index != 0] {
                    header class="journal-log-header" {
                        h2 { (&case.subject) }
                        span class=(format!("journal-case-status journal-case-status-{}", case.status)) {
                            (status_label(&case.status))
                        }
                    }
                    @if let Some(case_records) = records.get(&case.case_id) {
                        @for record in case_records {
                            article class="journal-record" {
                                p { (&record.summary) }
                                p class="journal-source" { "Source: " (&record.source) }
                            }
                        }
                        (bestiary_journal_results(
                            &deductions.iter()
                                .filter(|deduction| deduction.case_id == case.case_id)
                                .cloned()
                                .collect::<Vec<_>>()
                        ))
                    } @else {
                        p class="text-muted" { "No recorded reports." }
                    }
                }
            }
        }
        aside class="right-sidebar journal-context" data-journal-context {
            (super::sidebar_section("Journal", html! {
                @let open_count = ordered_cases.iter().filter(|case| case.status == "open").count();
                dl class="location-stat-list journal-summary" {
                    div { dt { "Known" } dd { (ordered_cases.len()) } }
                    div { dt { "Open" } dd { (open_count) } }
                    div { dt { "Closed" } dd { (ordered_cases.len().saturating_sub(open_count)) } }
                }
            }))
        }
    };
    super::journal_layout(content, Some(character_name))
}

fn status_label(status: &str) -> &'static str {
    match status {
        "completed" => "Completed",
        "failed" => "Failed",
        _ => "Open",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(
        case_id: &str,
        title: &str,
        status: &str,
        latest_update_at: u64,
    ) -> BackendInvestigationCaseSummary {
        BackendInvestigationCaseSummary {
            owner_character_id: 1,
            case_id: case_id.into(),
            subject: title.into(),
            status: status.into(),
            latest_update_at,
        }
    }

    fn lead(
        case_id: &str,
        lead_id: &str,
        summary: &str,
        source_label: &str,
        recorded_at: u64,
    ) -> BackendInvestigationLead {
        BackendInvestigationLead {
            owner_character_id: 1,
            case_id: case_id.into(),
            lead_id: lead_id.into(),
            summary: summary.into(),
            source_label: source_label.into(),
            confidence_bps: 5500,
            destination_stage: DestinationKnowledgeStage::Textual,
            directions: "beyond the mill".into(),
            exact_location_id: String::new(),
            latitude_e_7: 0,
            longitude_e_7: 0,
            witness_name: "Marta".into(),
            witness_description: "tall, red-haired cooper".into(),
            witness_occupation_or_relationship: "cooper".into(),
            expected_location: "workshops".into(),
            current_learned_location: String::new(),
            contradiction_group: "shape".into(),
            corrected_by: String::new(),
            recorded_at,
        }
    }

    fn entry(
        case_id: &str,
        record_id: &str,
        summary: &str,
        source_label: &str,
        recorded_at: u64,
    ) -> BackendInvestigationJournalEntry {
        BackendInvestigationJournalEntry {
            owner_character_id: 1,
            case_id: case_id.into(),
            record_id: record_id.into(),
            kind: "notice".into(),
            summary: summary.into(),
            source_label: source_label.into(),
            confidence_bps: 0,
            contradiction_group: String::new(),
            corrected_by: String::new(),
            supersedes: String::new(),
            recorded_at,
        }
    }

    #[test]
    fn journal_orders_open_cases_by_recency_before_closed_cases() {
        let cases = [
            case("old-open", "Old open problem", "open", 10),
            case("completed", "Recently completed", "completed", 100),
            case("new-open", "New open problem", "open", 20),
            case("failed", "Recently failed", "failed", 200),
        ];

        let markup = journal_page(&[], &[], &cases, &[], "Ada", None).into_string();
        let new_open = markup.find("New open problem").unwrap();
        let old_open = markup.find("Old open problem").unwrap();
        let failed = markup.find("Recently failed").unwrap();
        let completed = markup.find("Recently completed").unwrap();

        assert!(new_open < old_open);
        assert!(old_open < failed);
        assert!(failed < completed);
        assert!(
            markup
                .contains("class=\"journal-case-tab active\" role=\"tab\" aria-selected=\"true\"")
        );
        assert!(markup.contains("id=\"journal-case-tab-0\""));
        assert!(markup.contains("tabindex=\"0\" aria-controls=\"journal-case-panel-0\""));
        assert!(markup.contains(
            "id=\"journal-case-panel-0\" class=\"journal-case-panel\" role=\"tabpanel\" aria-labelledby=\"journal-case-tab-0\""
        ));
        assert!(markup.contains("data-journal-case-log"));
        assert!(markup.contains("data-journal-context"));
        assert!(markup.contains("data-journal-case-panel=\"new-open\""));
    }

    #[test]
    fn journal_groups_dry_records_by_case() {
        let cases = [
            case("missing-cart", "The missing cart", "open", 4),
            case("graveyard", "Trouble at the graveyard", "open", 3),
        ];
        let leads = [
            lead(
                "missing-cart",
                "cart-tracks",
                "A cart left the road by the alder trees.",
                "tracks beside the north road",
                2,
            ),
            lead(
                "graveyard",
                "night-noise",
                "Scraping was heard after midnight.",
                "the gravedigger",
                1,
            ),
        ];

        let markup = journal_page(&[], &leads, &cases, &[], "Ada", None).into_string();
        assert!(markup.contains("data-journal-case-index"));
        assert!(markup.contains("data-journal-case-log"));
        assert!(markup.contains("data-journal-case-panel=\"missing-cart\""));
        assert!(markup.contains("data-journal-case-panel=\"graveyard\""));
        assert!(markup.contains("A cart left the road by the alder trees."));
        assert!(markup.contains("Source: tracks beside the north road"));
        assert!(markup.contains("Scraping was heard after midnight."));
        assert!(markup.contains("Source: the gravedigger"));
    }

    #[test]
    fn journal_records_only_the_report_and_its_source() {
        let report = lead("case", "lead", "Screams after dark", "the innkeeper", 1);
        let cases = [case("case", "Night screams", "open", 1)];
        let markup = journal_page(&[], &[report], &cases, &[], "Ada", None).into_string();

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
    fn journal_deduplicates_only_identical_notice_and_lead_presentations() {
        let summary = "Several expected caravans have not arrived.";
        let notice = entry("case", "notice", summary, "local rumor", 7);
        let duplicate_lead = lead("case", "lead", summary, "local rumor", 7);
        let later_repeat = lead("case", "later", summary, "local rumor", 8);
        let distinct_source = lead("case", "witness", summary, "the carter", 7);
        let cases = [case("case", "Missing caravans", "open", 8)];

        let markup = journal_page(
            &[notice],
            &[duplicate_lead, later_repeat, distinct_source],
            &cases,
            &[],
            "Ada",
            None,
        )
        .into_string();

        assert_eq!(markup.matches(summary).count(), 3);
        assert_eq!(markup.matches("Source: local rumor").count(), 2);
        assert_eq!(markup.matches("Source: the carter").count(), 1);
    }

    #[test]
    fn journal_renders_qualitative_monster_candidates_and_provenance() {
        let results = vec![BackendBestiaryDeduction {
            owner_character_id: 1,
            case_id: "case".into(),
            monster_kind: "Werewolf".into(),
            support_band: "plausible".into(),
            provenance_json:
                r#"["received report from the miller","learned diagnostic clue: pawprints"]"#.into(),
            updated_at: 1,
        }];

        let markup = bestiary_journal_results(&results).into_string();

        assert!(markup.contains("Possible monster kinds:"));
        assert!(markup.contains("Werewolf"));
        assert!(markup.contains("plausible"));
        assert!(markup.contains("received report from the miller"));
        assert!(markup.contains("learned diagnostic clue: pawprints"));
        assert!(!markup.contains('%'));
        assert!(!markup.contains("support_bps"));
        assert!(!markup.contains("difficulty"));
        assert!(!markup.contains("threshold"));
        assert!(!markup.contains("canonical"));
    }

    #[test]
    fn journal_omits_referrals_and_actionable_metadata() {
        let referral = lead(
            "case",
            "referral",
            "Ask Marta at the mill.",
            "witness referral",
            1,
        );
        let ordinary = lead(
            "case",
            "heard",
            "Screams were heard after dark.",
            "the innkeeper",
            2,
        );
        let cases = [case("case", "Night screams", "open", 2)];
        let markup =
            journal_page(&[], &[referral, ordinary], &cases, &[], "Ada", None).into_string();

        assert!(markup.contains("Screams were heard after dark."));
        assert!(!markup.contains("Ask Marta"));
        assert!(!markup.contains("55%"));
        assert!(!markup.contains("beyond the mill"));
        assert!(!markup.contains("tall, red-haired cooper"));
        assert!(!markup.contains("workshops"));
    }
}
