//! Opt-in real SpacetimeDB integration assertion.
//!
//! Publish a fresh module first, then set `ADVENTURESIM_SIM_DATABASE` to its
//! unique `adventuresim-sim-*` name. The test process must inherit the same
//! `ADVENTURESIM_SIM_BOOTSTRAP_TOKEN` used for that module build; use the safe
//! recipe for routine runs.

use adventuresim_strategic_sim::{
    CoreLoopConfig, CoreLoopEventKind, EquipmentStyle, run_core_loop,
};

#[test]
#[ignore = "requires an explicitly published disposable loopback SpacetimeDB database"]
fn authoritative_core_loop_is_isolated_and_branch_tolerant() {
    let database = std::env::var("ADVENTURESIM_SIM_DATABASE")
        .expect("set ADVENTURESIM_SIM_DATABASE to a fresh adventuresim-sim-* database");
    let config = CoreLoopConfig {
        host: std::env::var("ADVENTURESIM_SIM_HOST")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".into()),
        database,
        seed: 42,
        population: 8,
        cycles: 12,
        duration_days: 14,
        party_size: 2,
        run_nonce: std::env::var("ADVENTURESIM_SIM_NONCE")
            .expect("set ADVENTURESIM_SIM_NONCE to the launcher's nonce"),
        use_imported_world: false,
        expected_world_manifest_digest: None,
        failure_output: None,
    };
    let report = run_core_loop(config.clone()).expect("authoritative core loop should complete");

    assert_eq!(report.metrics.parties_formed, 8);
    assert_eq!(report.metrics.joins_accepted, 4);
    assert!(report.metrics.quests_attempted > 0);
    assert!(
        report.metrics.quests_attempted > 1 || report.metrics.activity_days > 0,
        "the run should exercise repeated autonomous decisions"
    );
    assert!(
        report.trace.iter().any(|event| {
            event.kind == CoreLoopEventKind::QuestDecision
                && event.detail.contains("offered_contracts=")
                && event.detail.contains("fallback=")
        }),
        "each autonomous choice should expose its observer-safe quest decision"
    );
    if report.metrics.activity_days > 0 {
        assert!(
            report.trace.iter().any(|event| {
                event.kind == CoreLoopEventKind::Activity
                    && event.detail.contains("outcome=completed")
                    && event.detail.contains("purse_delta=")
                    && event.detail.contains("condition_before=")
                    && event.detail.contains("elapsed_delta=")
            }),
            "activity diagnostics should retain public pre/post consequences"
        );
    }
    assert!(
        report.trace.iter().any(|event| {
            event.kind == CoreLoopEventKind::Activity
                && event.detail.contains("outcome=completed")
                && event.detail.contains("effective=Labor")
                && detail_number(&event.detail, "purse_delta=").is_some_and(|delta| delta > 0.0)
        }),
        "the authoritative activity policy should complete productive legal labor"
    );
    let inn_activities = report
        .trace
        .iter()
        .filter(|event| {
            event.kind == CoreLoopEventKind::Activity
                && event.detail.contains("outcome=completed")
                && event.detail.contains("venue=inn")
        })
        .collect::<Vec<_>>();
    assert!(
        !inn_activities.is_empty(),
        "the deterministic live fixture should exercise full-board Inn activity"
    );
    assert!(
        inn_activities.iter().all(|event| {
            let before = detail_number(&event.detail, "hunger_before=");
            let after = detail_number(&event.detail, "hunger_after=");
            matches!((before, after), (Some(before), Some(after)) if after <= before + 0.001)
        }),
        "full-board Inn activity must not worsen authoritative hunger"
    );
    assert!(
        report.profiles.iter().any(|profile| matches!(
            profile.equipment.style,
            EquipmentStyle::Light | EquipmentStyle::Heavy
        )),
        "the population should include an armor-preferring policy"
    );

    if report.metrics.quests_completed > 0 {
        assert_ordered_subsequence(
            &report,
            &[
                CoreLoopEventKind::AutoresolveVictory,
                CoreLoopEventKind::StoreLoot,
                CoreLoopEventKind::TurnIn,
                CoreLoopEventKind::Liquidate,
            ],
        );
    } else {
        assert!(report.metrics.defeats > 0);
        assert!(
            report
                .trace
                .iter()
                .any(|event| event.kind == CoreLoopEventKind::AbandonQuest)
        );
    }

    assert!(
        report
            .final_agents
            .iter()
            .all(|agent| agent.elapsed_minutes > 0)
    );
    assert!(
        report
            .final_agents
            .iter()
            .all(|agent| agent.personal_gold_coin < 1_000_000),
        "gold-stack deduction must not underflow"
    );
    assert!(
        report.final_agents.iter().all(|agent| {
            agent.hunger.is_finite()
                && agent.thirst.is_finite()
                && agent.food_days.is_finite()
                && agent.water_days.is_finite()
                && agent.visible_food_kcal.is_finite()
                && agent.visible_water_ml.is_finite()
        }),
        "successful reports must retain public need and supply diagnostics"
    );
    assert_eq!(report.metrics.reducer_failures, 0);
    assert_eq!(report.metrics.stuck_detections, 0);
    assert_eq!(report.metrics.duplicate_semantic_events, 0);
    assert!(
        report.metrics.repair_submissions > 0,
        "fixture wear should exercise NPC repair policy"
    );
    assert_eq!(
        report.metrics.repair_submissions,
        report.metrics.repair_retrievals
    );
    assert!(report.metrics.repair_wait_minutes > 0);
    assert!(
        report
            .final_agents
            .iter()
            .all(|agent| agent.outstanding_repair_orders == 0),
        "NPCs must not strand completed work"
    );
    assert_ordered_subsequence(
        &report,
        &[
            CoreLoopEventKind::SubmitRepair,
            CoreLoopEventKind::WaitForRepair,
            CoreLoopEventKind::RetrieveRepair,
            CoreLoopEventKind::Equip,
        ],
    );
    assert!(
        report
            .final_agents
            .iter()
            .filter(|agent| agent.alive)
            .all(|agent| !agent.equipment_item_ids.is_empty()),
        "living NPCs must retain equipped items after smith retrieval"
    );
    assert!(report.metrics.preparations_purchased > 0);
    assert!(report.metrics.interventions_administered > 0);
    assert!(report.metrics.treatment_rest_minutes > 0);
    assert_ordered_subsequence(
        &report,
        &[
            CoreLoopEventKind::AdministerPreparation,
            CoreLoopEventKind::BuyMedication,
            CoreLoopEventKind::AdministerPreparation,
            CoreLoopEventKind::Recover,
            CoreLoopEventKind::IllnessRecovered,
        ],
    );

    let reuse_error = run_core_loop(config).expect_err("a claimed database cannot be reused");
    assert!(
        reuse_error.contains("reused or populated") || reuse_error.contains("already claimed"),
        "unexpected reuse error: {reuse_error}"
    );
}

fn detail_number(detail: &str, key: &str) -> Option<f64> {
    detail
        .split(';')
        .find_map(|field| field.strip_prefix(key))
        .and_then(|value| value.parse().ok())
}

fn assert_ordered_subsequence(
    report: &adventuresim_strategic_sim::CoreLoopReport,
    expected: &[CoreLoopEventKind],
) {
    let mut cursor = 0;
    for event in &report.trace {
        if cursor < expected.len() && event.kind == expected[cursor] {
            cursor += 1;
        }
    }
    assert_eq!(cursor, expected.len(), "victory trace was incomplete");
}
