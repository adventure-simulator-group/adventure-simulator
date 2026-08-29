fn capability(
    character_id: u64,
    melee: bool,
    ranged: bool,
    precise: bool,
    heavy: bool,
) -> CharacterCapability {
    CharacterCapability {
        character_id,
        melee,
        ranged,
        precise,
        heavy,
        quarter_armor: false,
        half_armor: false,
        three_quarter_armor: false,
        full_armor: false,
        blunt: false,
        slash: melee,
        pierce: ranged,
        athletics: 1.0,
        endurance: 1.0,
        physiology: 0.0,
        knife: 0.0,
        tailoring: 0.0,
        surgery: 0.0,
        command: 0.0,
        religion: 0.0,
        weapon_precision: if precise { 1.0 } else { 0.0 },
        autoresolve_combat_power: 7_000,
    }
}

fn projected_action(action_id: &str, method: &str) -> BackendInvestigationAction {
    BackendInvestigationAction {
        owner_character_id: 7,
        case_id: "case".into(),
        action_id: action_id.into(),
        method: method.into(),
        expected_version: 1,
        summary: "public summary".into(),
        known_prerequisites: "none".into(),
        duration_min_minutes: 15,
        duration_max_minutes: 30,
        uncertainty_bps: 1_000,
        skill_contributions: "public contribution summary".into(),
        weather_available: true,
        contact_character_id: None,
        required_case_site_id: None,
        availability: InvestigationActionAvailability::Available,
        unavailable_reason: String::new(),
    }
}

#[test]
fn encounter_policy_is_permutation_invariant_and_never_attacks_during_evacuation() {
    for choices in [
        vec!["attack".into(), "sneak".into(), "detour".into()],
        vec!["detour".into(), "attack".into(), "sneak".into()],
    ] {
        let selected = select_expedition_encounter_choice(&choices, false).unwrap();
        assert_eq!(selected.choice, "detour");
        assert_eq!(selected.reason, "guaranteed_party_aware_detour");
    }
    assert_eq!(
        select_expedition_encounter_choice(&["attack".into(), "run".into()], false)
            .unwrap()
            .choice,
        "run"
    );
    assert_eq!(
        select_expedition_encounter_choice(&["attack".into(), "surrender".into()], false)
            .unwrap()
            .choice,
        "surrender"
    );
    assert_eq!(
        select_expedition_encounter_choice(&["attack".into()], false)
            .unwrap()
            .choice,
        "attack"
    );
    assert!(select_expedition_encounter_choice(&["attack".into()], true).is_none());
}

#[test]
fn public_contract_matchup_uses_readiness_count_difficulty_and_fails_closed() {
    let strong = |id, ready| {
        let mut capability = capability(id, true, false, true, true);
        capability.endurance = 2.0;
        capability.athletics = 2.0;
        PublicPartyCombatant { capability, ready }
    };
    // One superficially strong novice is not enough: the authoritative enemy
    // also owns weapon, dodge, block, balance, and protection mechanics.
    assert!(!public_contract_assessment(1, 1, 10_000, &[strong(1, true)]).eligible);
    assert!(!public_contract_assessment(1, 2, 20_000, &[strong(1, true)]).eligible);
    assert!(
        !public_contract_assessment(1, 2, 20_000, &[strong(1, true), strong(2, true)]).eligible
    );
    assert!(
        !public_contract_assessment(6, 2, 20_000, &[strong(1, true), strong(2, true)]).eligible
    );
    assert!(!public_contract_assessment(1, 1, 10_000, &[strong(1, false)]).eligible);
    assert!(
        !public_contract_assessment(1, 0, 20_000, &[strong(1, true), strong(2, true)])
            .eligible
    );
    assert_eq!(
        public_contract_assessment(1, 1, 0, &[strong(1, true)]).reason,
        "missing_authoritative_opposition_power"
    );

    let accepted = public_contract_assessment(
        1,
        3,
        18_000,
        &[
            strong(1, true),
            strong(2, true),
            strong(3, true),
            strong(4, true),
        ],
    );
    let deteriorated = public_contract_assessment(
        1,
        3,
        18_000,
        &[
            strong(1, true),
            strong(2, true),
            strong(3, false),
            strong(4, false),
        ],
    );
    assert!(accepted.eligible);
    assert!(!deteriorated.eligible);
    assert_eq!(deteriorated.reason, "public_matchup_below_safety_margin");

    let mut overflow = strong(9, true);
    overflow.capability.autoresolve_combat_power = u64::MAX;
    assert_eq!(
        public_contract_assessment(1, 1, 1, &[overflow.clone()]).reason,
        "public_combat_margin_overflow"
    );
    assert_eq!(
        public_contract_assessment(1, 1, 1, &[overflow.clone(), overflow]).reason,
        "public_party_power_overflow"
    );
}

#[test]
fn generated_action_score_prefers_progress_then_fit_then_public_costs() {
    let mut profile = generate_profile(42, 0);
    profile.initial_skills.insight = 8_000.0;
    profile.initial_skills.stealth = 1_000.0;
    let inspect = projected_action("z", "inspect_site");
    let ambush = projected_action("a", "lay_ambush");
    assert!(generated_action_score(&profile, &inspect) > generated_action_score(&profile, &ambush));

    let mut travel = inspect.clone();
    travel.availability =
        InvestigationActionAvailability::Unavailable(InvestigationActionUnavailableFields {
            reason: InvestigationActionUnavailableReason::TravelRequired,
            can_travel_to_required_site: true,
            wait_minutes: 0,
        });
    assert!(generated_action_score(&profile, &inspect) > generated_action_score(&profile, &travel));

    let mut reworded = travel.clone();
    reworded.unavailable_reason = "completely different presentation copy".into();
    assert_eq!(
        generated_action_score(&profile, &travel),
        generated_action_score(&profile, &reworded)
    );

    let mut uncertain = inspect.clone();
    uncertain.uncertainty_bps = 9_000;
    assert!(
        generated_action_score(&profile, &inspect) > generated_action_score(&profile, &uncertain)
    );

    let tied_a = projected_action("a", "unknown");
    let tied_b = projected_action("b", "unknown");
    assert_eq!(
        generated_action_score(&profile, &tied_a),
        generated_action_score(&profile, &tied_b)
    );
    let mut tied = vec![tied_b, tied_a];
    sort_generated_actions(&profile, &mut tied);
    assert_eq!(
        tied.iter()
            .map(|action| action.action_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}

#[test]
fn generated_skill_fit_exactly_mirrors_public_action_skill_mapping() {
    let mut profile = generate_profile(42, 0);
    profile.initial_skills.insight = 8_000.0;
    profile.initial_skills.stealth = 2_000.0;
    for method in [
        "inspect_site",
        "search_area",
        "locate_contact",
        "watch",
        "patrol",
        "approach_lead",
    ] {
        assert_eq!(
            generated_method_skill_fit(&profile, method),
            8_000,
            "{method}"
        );
    }
    assert_eq!(generated_method_skill_fit(&profile, "follow_tracks"), 0);
    assert_eq!(generated_method_skill_fit(&profile, "reacquire_tracks"), 0);
    assert_eq!(generated_method_skill_fit(&profile, "lay_ambush"), 5_000);
    assert!(
        generated_action_score(&profile, &projected_action("inspect", "inspect_site"))
            > generated_action_score(&profile, &projected_action("tracks", "follow_tracks"))
    );
}

#[test]
fn party_grouping_balances_roles_and_promotes_quest_capable_leaders() {
    let mut profiles = (0..6).map(|id| generate_profile(9, id)).collect::<Vec<_>>();
    let roles = [
        BuildRole::FrontLine,
        BuildRole::FrontLine,
        BuildRole::Ranged,
        BuildRole::Ranged,
        BuildRole::Healer,
        BuildRole::Healer,
    ];
    for (index, profile) in profiles.iter_mut().enumerate() {
        profile.agent_id = index as u32;
        profile.build.role = roles[index];
        profile.build.activity_only = index == 0 || index == 2;
    }
    let groups = balanced_party_groups(&profiles, 3);
    assert_eq!(groups.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 3]);
    assert!(
        groups
            .iter()
            .all(|group| !profiles[group[0]].build.activity_only)
    );
    assert!(groups.iter().all(|group| {
        let mut ranks = group
            .iter()
            .map(|&index| role_rank(profiles[index].build.role))
            .collect::<Vec<_>>();
        ranks.sort_unstable();
        ranks.dedup();
        ranks.len() == group.len()
    }));

    for profile in &mut profiles {
        profile.build.activity_only = true;
    }
    let all_content = balanced_party_groups(&profiles, 3);
    assert!(
        all_content
            .iter()
            .all(|group| profiles[group[0]].build.activity_only)
    );
}

#[test]
fn generated_defeat_policy_suppresses_work_until_public_capability_changes() {
    let original = public_combat_fingerprint(vec![capability(1, true, false, false, false)]);
    let unchanged = original.clone();
    let improved = public_combat_fingerprint(vec![capability(1, true, false, true, true)]);
    let planned_work = |decision| match decision {
        GeneratedDefeatDecision::SuppressUnchanged => (0, 0, 0),
        GeneratedDefeatDecision::Proceed => (1, 1, 1),
    };
    assert_eq!(
        planned_work(generated_defeat_decision(true, Some(&original), &unchanged)),
        (0, 0, 0),
    );
    assert_eq!(
        planned_work(generated_defeat_decision(true, Some(&original), &improved)),
        (1, 1, 1),
    );
    assert_eq!(
        generated_defeat_decision(false, Some(&original), &unchanged),
        GeneratedDefeatDecision::Proceed,
    );
}

#[test]
fn departure_preflights_the_selected_travel_action_not_a_longer_alternative() {
    let profile = generate_profile(42, 0);
    let mut selected = projected_action("selected-62", "search");
    selected.availability =
        InvestigationActionAvailability::Unavailable(InvestigationActionUnavailableFields {
            reason: InvestigationActionUnavailableReason::TravelRequired,
            can_travel_to_required_site: true,
            wait_minutes: 0,
        });
    selected.required_case_site_id = Some(CaseSiteId {
        value: "site".into(),
    });
    selected.duration_max_minutes = 62;
    let mut alternative = selected.clone();
    alternative.action_id = "alternative-67".into();
    alternative.duration_max_minutes = 67;
    let mut actions = vec![alternative, selected];
    let next = select_generated_travel_action(&profile, &mut actions, |action| {
        action.duration_max_minutes == 62
    })
    .expect("safe fallback travel action");
    assert_eq!(next.action_id, "selected-62");
    assert_eq!(next.duration_max_minutes, 62);
    assert!(projected_action_ready(0.1, 3_000.0, 6_000.0));
    assert!(!projected_action_ready(0.1, 5_400.0, 6_000.0));
    assert!(
        select_generated_travel_action(&profile, &mut actions, |_| false).is_none(),
        "all unsafe candidates must fail closed"
    );
}

#[test]
fn first_generated_combat_uses_the_same_checked_public_margin() {
    let members = [PublicPartyCombatant {
        capability: capability(1, true, false, false, false),
        ready: true,
    }];
    let unsafe_assessment = public_contract_assessment(1, 1, u64::MAX, &members);
    assert!(!unsafe_assessment.eligible);
    assert!(matches!(
        unsafe_assessment.reason,
        "public_combat_margin_overflow" | "public_matchup_below_safety_margin"
    ));
    let missing = public_contract_assessment(1, 1, 0, &members);
    assert!(!missing.eligible);
    assert_eq!(missing.reason, "missing_authoritative_opposition_power");

    let source = LIVE_CORE_SOURCE;
    let generated = source
        .split("pub(super) fn advance_generated_case")
        .nth(1)
        .expect("generated case loop");
    let first = generated.find("unsafe_first_generated_combat").unwrap();
    let revalidated = generated
        .find("generated_combat_revalidation_failed")
        .unwrap();
    let reducer = generated
        .find("ReducerOperation::AutoresolveGeneratedMission")
        .unwrap();
    assert!(first < revalidated && revalidated < reducer);
}

#[test]
fn public_quest_fixture_selection_is_bounded_and_validates_provenance() {
    let fixture = SimulationQuestFixture {
        id: 0,
        run_id: 17,
        direct_contract_id: "contract".into(),
        generated_case_id: "case".into(),
        direct_leader_id: 202,
        generated_leader_id: 101,
        direct_party_id: "party-b".into(),
        generated_party_id: "party-a".into(),
    };
    let expected = [(101, "party-a".into()), (202, "party-b".into())];

    assert_eq!(
        select_public_quest_fixture([fixture.clone()], 17, &expected),
        Ok(fixture.clone())
    );
    assert!(select_public_quest_fixture([], 17, &expected).is_err());
    assert!(
        select_public_quest_fixture([fixture.clone(), fixture.clone()], 17, &expected).is_err()
    );

    let mut wrong_id = fixture.clone();
    wrong_id.id = 1;
    assert!(select_public_quest_fixture([wrong_id], 17, &expected).is_err());

    let mut wrong_run = fixture.clone();
    wrong_run.run_id = 18;
    assert!(select_public_quest_fixture([wrong_run], 17, &expected).is_err());

    let mut wrong_leader = fixture.clone();
    wrong_leader.direct_leader_id = 303;
    assert!(select_public_quest_fixture([wrong_leader], 17, &expected).is_err());

    let mut wrong_party = fixture;
    wrong_party.direct_party_id = "party-c".into();
    assert!(select_public_quest_fixture([wrong_party], 17, &expected).is_err());

    assert_eq!(
        select_public_quest_fixture_if_present([], 17, &expected),
        Ok(None)
    );
    let bootstrap = include_str!("../bootstrap.rs");
    assert!(bootstrap.contains("std::time::Instant::now() + ACTION_TIMEOUT"));
    assert!(bootstrap.contains("std::thread::sleep(Duration::from_millis(5))"));
    assert!(bootstrap.contains("wait_for_new_public_direct_contract"));
    assert!(bootstrap.contains("generated_case_id: None"));
    assert!(bootstrap.contains("select_public_quest_fixture_if_present"));
    assert!(!bootstrap.contains("wait_for_public_quest_fixture("));
}

#[test]
fn quest_fixture_designates_the_strongest_publicly_safe_party_stably() {
    let assessment = |power, eligible| PublicContractAssessment {
        eligible,
        reason: if eligible { "safe" } else { "unsafe" },
        enemy_count: Some(1),
        ready_combatants: 1,
        party_power_milli: power,
        enemy_power_milli: 100,
    };
    let candidate = |leader_id, party_id: &str, assessment| FixturePartyCandidate {
        identity: FixturePartyIdentity {
            leader_id,
            party_id: party_id.into(),
        },
        assessment,
    };
    let selected = select_strongest_fixture_party(vec![
        candidate(10, "party-z", assessment(200, true)),
        candidate(20, "party-a", assessment(300, true)),
    ])
    .unwrap();
    assert_eq!(
        selected.direct,
        FixturePartyIdentity {
            leader_id: 20,
            party_id: "party-a".into(),
        }
    );
    assert_eq!(
        selected.generated,
        FixturePartyIdentity {
            leader_id: 10,
            party_id: "party-z".into(),
        }
    );

    let tied = select_strongest_fixture_party(vec![
        candidate(20, "party-z", assessment(300, true)),
        candidate(10, "party-a", assessment(300, true)),
    ])
    .unwrap();
    assert_eq!(
        tied.direct,
        FixturePartyIdentity {
            leader_id: 10,
            party_id: "party-a".into(),
        }
    );
    assert!(
        select_strongest_fixture_party(vec![
            candidate(10, "party-a", assessment(0, false)),
            candidate(20, "party-b", assessment(0, false)),
        ])
        .is_err()
    );
}

#[test]
fn quest_fixture_lane_plan_is_exact_and_order_independent() {
    let fixture = FixtureLanePlan {
        direct_contract_id: "contract:reserved".into(),
        generated_case_id: Some("case:reserved".into()),
        direct_leader_id: 20,
        generated_leader_id: 10,
        direct_party_id: "party:z".into(),
        generated_party_id: "party:a".into(),
    };
    // Formation/scheduler order and agent propensity are deliberately absent:
    // the authoritative fixture provenance alone assigns each lane.
    assert_eq!(
        fixture_quest_lane(Some(&fixture), 10, "party:a"),
        Some(FixtureQuestLane::Generated)
    );
    assert_eq!(
        fixture_quest_lane(Some(&fixture), 20, "party:z"),
        Some(FixtureQuestLane::Direct)
    );
    assert_eq!(fixture_quest_lane(Some(&fixture), 10, "party:z"), None);
    assert_eq!(fixture_quest_lane(None, 20, "party:z"), None);

    let bootstrap = include_str!("../bootstrap.rs");
    assert!(bootstrap.contains("contract.id == fixture.direct_contract_id"));
    assert!(bootstrap.contains("Some(FixtureQuestLane::Generated) => None"));
    assert!(bootstrap.contains("None => runner.choose_quest(&party, &profile)"));
}
#[test]
fn simulation_duration_is_relative_to_the_post_bootstrap_world_clock() {
    let absolute_start = 8_000_000;
    assert_eq!(
        simulation_elapsed_minutes(absolute_start, absolute_start),
        0
    );
    assert_eq!(
        simulation_elapsed_minutes(absolute_start, absolute_start + 1_440),
        1_440
    );
    assert_eq!(
        simulation_elapsed_minutes(absolute_start, absolute_start - 1),
        0
    );

    let bootstrap = LIVE_CORE_SOURCE
        .split("let simulation_start_minutes")
        .nth(1)
        .expect("absolute-clock baseline capture");
    assert!(bootstrap.contains("simulation_elapsed_minutes("));
    assert!(bootstrap.contains("recovery_started_at"));
    assert!(bootstrap.contains("missing simulation-start final character clock"));
}
