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
    assert_eq!(report.metrics.reducer_failures, 0);
    assert_eq!(report.metrics.stuck_detections, 0);
    assert_eq!(report.metrics.duplicate_semantic_events, 0);

    let reuse_error = run_core_loop(config).expect_err("a claimed database cannot be reused");
    assert!(
        reuse_error.contains("reused or populated") || reuse_error.contains("already claimed"),
        "unexpected reuse error: {reuse_error}"
    );
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
