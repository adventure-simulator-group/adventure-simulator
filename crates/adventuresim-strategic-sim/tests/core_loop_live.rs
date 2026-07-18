//! Opt-in real SpacetimeDB integration assertion.
//!
//! Publish a fresh module first, then set `ADVENTURESIM_SIM_DATABASE` to its
//! unique `adventuresim-sim-*` name and run this ignored test explicitly.

use adventuresim_strategic_sim::{CoreLoopConfig, CoreLoopEventKind, run_core_loop};

#[test]
#[ignore = "requires an explicitly published disposable loopback SpacetimeDB database"]
fn authoritative_victory_path_reaches_an_equipment_upgrade() {
    let database = std::env::var("ADVENTURESIM_SIM_DATABASE")
        .expect("set ADVENTURESIM_SIM_DATABASE to a fresh adventuresim-sim-* database");
    let report = run_core_loop(CoreLoopConfig {
        host: std::env::var("ADVENTURESIM_SIM_HOST")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".into()),
        database,
        seed: 42,
        population: 4,
        cycles: 1,
    })
    .expect("authoritative core loop should complete");
    let kinds = report
        .trace
        .iter()
        .map(|event| &event.kind)
        .collect::<Vec<_>>();
    let expected = [
        CoreLoopEventKind::AcceptQuest,
        CoreLoopEventKind::AutoresolveVictory,
        CoreLoopEventKind::StoreLoot,
        CoreLoopEventKind::TurnIn,
        CoreLoopEventKind::Liquidate,
        CoreLoopEventKind::Purchase,
        CoreLoopEventKind::Equip,
    ];
    let mut cursor = 0;
    for kind in kinds {
        if cursor < expected.len() && *kind == expected[cursor] {
            cursor += 1;
        }
    }
    assert_eq!(cursor, expected.len(), "victory trace was incomplete");
    assert_eq!(report.metrics.quests_completed, 1);
    assert_eq!(report.metrics.equipment_upgrades, 1);
    assert_eq!(report.metrics.stuck_detections, 0);
    assert_eq!(report.metrics.duplicate_semantic_events, 0);
}
