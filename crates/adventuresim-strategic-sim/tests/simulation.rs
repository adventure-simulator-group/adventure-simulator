use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use adventuresim_strategic_sim::*;
use std::process::Command;

fn config(seed: u64, population: u32, days: u32) -> SimulationConfig {
    SimulationConfig {
        seed,
        population,
        days,
        max_decisions: u64::from(population) * u64::from(days),
        max_trace_events: 100,
        snapshot_interval_days: 30,
        max_snapshots: 100,
        ..SimulationConfig::default()
    }
}

#[test]
fn fixed_seed_is_reproducible_and_other_seed_differs() {
    let a = run(config(41, 4, 80)).unwrap();
    let b = run(config(41, 4, 80)).unwrap();
    let c = run(config(42, 4, 80)).unwrap();
    assert_eq!(a.canonical_digest, b.canonical_digest);
    assert_eq!(a, b);
    assert_ne!(a.manifest.profiles, c.manifest.profiles);
    assert_ne!(a.canonical_digest, c.canonical_digest);
}

#[test]
fn recorded_manifest_replays_to_same_digest() {
    let original = run(config(7, 3, 365)).unwrap();
    let replayed = replay(original.manifest.clone()).unwrap();
    assert_eq!(original.canonical_digest, replayed.canonical_digest);
    assert_eq!(digest(&replayed).unwrap(), replayed.canonical_digest);
    let decoded: SimulationReport =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(digest(&decoded).unwrap(), original.canonical_digest);
}

#[test]
fn canonical_digest_quantizes_subprecision_float_noise() {
    let report = run(config(70, 2, 30)).unwrap();
    let mut noisy = report.clone();
    noisy.metrics[0].notoriety += 0.000_001;
    assert_eq!(digest(&report).unwrap(), digest(&noisy).unwrap());
}

#[test]
fn serde_and_validation_reject_unknown_unbounded_and_nonfinite_config() {
    let json = r#"{"version":1,"seed":1,"population":1,"days":1,"max_decisions":1,"max_trace_events":1,"snapshot_interval_days":1,"max_snapshots":1,"population_scale":2,"typo":1}"#;
    assert!(serde_json::from_str::<SimulationConfig>(json).is_err());
    let mut bad = config(1, 1, 1);
    bad.population = MAX_POPULATION + 1;
    assert!(bad.validate().is_err());
    bad = config(1, 1, 1);
    bad.snapshot_interval_days = 0;
    assert!(bad.validate().is_err());
    bad = config(1, 1, 1);
    bad.population_scale = f32::NAN;
    assert!(bad.validate().is_err());
}

#[test]
fn canonical_trace_orders_day_then_agent_and_caps_storage() {
    let report = run(config(3, 3, 2)).unwrap();
    let ids: Vec<_> = report.trace.iter().map(|e| (e.day, e.agent_id)).collect();
    assert_eq!(ids, vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
    let mut capped = config(3, 3, 2);
    capped.max_trace_events = 2;
    capped.max_snapshots = 1;
    let report = run(capped).unwrap();
    assert_eq!(report.trace.len(), 2);
    assert!(report.trace_truncated);
    assert_eq!(report.snapshots.len(), 1);
    assert!(report.snapshots_truncated);
}

#[test]
fn matched_pair_changes_only_declared_activity_fields() {
    let (labor, thief) = matched_activity_pair(
        9,
        0,
        ActivityPreference::Labor,
        ActivityPreference::Thievery,
    );
    let mut normalized = thief.clone();
    normalized.preferred_activity = labor.preferred_activity;
    normalized.schedule = labor.schedule;
    assert_eq!(labor, normalized);
    let mut thief = thief;
    thief.agent_id = 1;
    let report = run_profiles(config(9, 2, 365), vec![labor, thief]).unwrap();
    assert_eq!(report.metrics[0].notoriety, 0.0);
    assert!(report.metrics[1].notoriety > 0.0);
}

#[test]
fn multi_year_smoke_tracks_time_allocation_and_finite_metrics() {
    let report = run(config(99, 16, 365 * 5)).unwrap();
    for (profile, metrics) in report.manifest.profiles.iter().zip(&report.metrics) {
        assert!(metrics.skill_hours.is_finite());
        assert!(metrics.notoriety.is_finite());
        assert!(metrics.cumulative_risk_exposure.is_finite());
        assert_eq!(
            metrics.activity_minutes
                + metrics.leisure_minutes
                + (profile.schedule.allocated_minutes()
                    - [
                        profile.schedule.labor,
                        profile.schedule.prayer,
                        profile.schedule.thievery,
                        profile.schedule.raiding
                    ]
                    .into_iter()
                    .map(u64::from)
                    .sum::<u64>())
                    * 365
                    * 5,
            MINUTES_PER_DAY * 365 * 5
        );
    }
}

#[test]
fn invalid_schedule_and_decision_cap_are_rejected_before_run() {
    let mut profile = generate_profile(1, 0);
    profile.schedule.labor = 1441;
    assert!(run_profiles(config(1, 1, 1), vec![profile]).is_err());
    let mut capped = config(1, 2, 2);
    capped.max_decisions = 3;
    assert!(run(capped).is_err());
}

#[test]
fn generated_profiles_exclude_raiding_and_custom_raiding_is_rejected() {
    for id in 0..100 {
        assert_eq!(generate_profile(17, id).schedule.raiding, 0);
    }
    let mut profile = generate_profile(1, 0);
    profile.schedule.raiding = 60;
    let error = run_profiles(config(1, 1, 1), vec![profile])
        .unwrap_err()
        .to_string();
    assert!(error.contains("raiding execution is unsupported"));
}

#[test]
fn dedicated_precision_changes_thievery_outcome() {
    let mut low = generate_profile(31, 0);
    low.schedule = adventuresim_core::strategic_schedule::DailySchedule {
        thievery: 720,
        ..Default::default()
    };
    low.preferred_activity = ActivityPreference::Thievery;
    low.initial_skills.stealth = 0.0;
    low.attributes.precision = 0.5;
    let mut high = low.clone();
    high.agent_id = 1;
    high.attributes.precision = 5.0;
    let report = run_profiles(config(31, 2, 30), vec![low, high]).unwrap();
    assert!(report.metrics[1].wealth > report.metrics[0].wealth);
    assert!(report.metrics[1].notoriety < report.metrics[0].notoriety);
}

#[test]
fn extreme_skill_hours_and_oversized_report_vectors_are_rejected() {
    let mut profile = generate_profile(1, 0);
    profile.initial_skills.sword = MAX_INITIAL_SKILL_HOURS + 1.0;
    assert!(run_profiles(config(1, 1, 1), vec![profile]).is_err());

    let mut bounded = config(2, 1, 1);
    bounded.max_trace_events = 1;
    let mut report = run(bounded).unwrap();
    report.trace.push(report.trace[0].clone());
    assert!(validate_report(&report).is_err());
    assert!(digest(&report).is_err());
}

#[test]
fn individual_religion_fields_must_be_finite_and_bounded() {
    for invalid in [-1.0, f32::NAN, f32::INFINITY, MAX_INITIAL_SKILL_HOURS + 1.0] {
        let mut profile = generate_profile(1, 0);
        profile.initial_skills.religion.judaism = invalid;
        assert!(
            run_profiles(config(1, 1, 1), vec![profile]).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn faithless_prayer_adds_no_religion_study() {
    let mut manual = generate_profile(4, 0);
    manual.schedule = adventuresim_core::strategic_schedule::DailySchedule {
        prayer: 60,
        ..Default::default()
    };
    let before = manual.initial_skills.religion;
    let report = run_profiles(config(4, 1, 1), vec![manual]).unwrap();
    let after = report.metrics[0].skill_hours.religion;
    assert_eq!(after, before);
}

#[test]
fn bounded_skill_state_remains_finite_for_maximum_duration() {
    let mut profile = generate_profile(88, 0);
    profile.initial_skills = adventuresim_core::strategic_schedule::SkillHours {
        polearm: MAX_INITIAL_SKILL_HOURS,
        axe: MAX_INITIAL_SKILL_HOURS,
        bludgeon: MAX_INITIAL_SKILL_HOURS,
        sword: MAX_INITIAL_SKILL_HOURS,
        knife: MAX_INITIAL_SKILL_HOURS,
        dodge: MAX_INITIAL_SKILL_HOURS,
        block: MAX_INITIAL_SKILL_HOURS,
        bow: MAX_INITIAL_SKILL_HOURS,
        crossbow: MAX_INITIAL_SKILL_HOURS,
        firearm: MAX_INITIAL_SKILL_HOURS,
        throw: MAX_INITIAL_SKILL_HOURS,
        will: MAX_INITIAL_SKILL_HOURS,
        insight: MAX_INITIAL_SKILL_HOURS,
        self_awareness: MAX_INITIAL_SKILL_HOURS,
        humor: MAX_INITIAL_SKILL_HOURS,
        command: MAX_INITIAL_SKILL_HOURS,
        deception: MAX_INITIAL_SKILL_HOURS,
        seduction: MAX_INITIAL_SKILL_HOURS,
        medicine: MAX_INITIAL_SKILL_HOURS,
        religion: adventuresim_world_schema::ReligionHours {
            roman_catholic: MAX_INITIAL_SKILL_HOURS,
            ..Default::default()
        },
        stealth: MAX_INITIAL_SKILL_HOURS,
        balance: MAX_INITIAL_SKILL_HOURS,
        anatomy: MAX_INITIAL_SKILL_HOURS,
        tailoring: MAX_INITIAL_SKILL_HOURS,
        smithing: MAX_INITIAL_SKILL_HOURS,
        cooking: MAX_INITIAL_SKILL_HOURS,
    };
    let report = run_profiles(config(88, 1, MAX_DAYS), vec![profile]).unwrap();
    assert!(report.metrics[0].skill_hours.is_finite());
    assert!(report.metrics[0].total_skill_hours_gained.is_finite());
}

#[test]
fn input_size_and_report_frontier_are_canonical() {
    assert!(validate_input_len(MAX_INPUT_BYTES).is_ok());
    assert!(validate_input_len(MAX_INPUT_BYTES + 1).is_err());
    let a = run(config(55, 8, 100)).unwrap();
    let b = run(config(55, 8, 100)).unwrap();
    assert_eq!(a.pareto_frontier, b.pareto_frontier);
    assert!(!a.pareto_frontier.agent_ids.is_empty());
    assert_eq!(a.canonical_digest, b.canonical_digest);
    assert!(human_summary(&a).contains("Pareto frontier"));
}

#[test]
fn cli_emits_machine_readable_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_adventuresim-strategic-sim"))
        .args(["run", "--seed", "5", "--population", "2", "--days", "3"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: SimulationReport = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.metrics.len(), 2);
    assert!(String::from_utf8_lossy(&output.stderr).contains("digest"));
}
