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
        required_case_site_id: String::new(),
        available: true,
        can_travel_to_required_site: false,
        unavailable_reason_code: String::new(),
        unavailable_reason: String::new(),
        wait_minutes: 0,
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
    assert_eq!(public_opposition_count("one"), Some(1));
    assert_eq!(public_opposition_count("a pair"), Some(2));
    assert_eq!(public_opposition_count("perhaps two"), Some(3));
    assert_eq!(public_opposition_count("perhaps eleven"), Some(12));
    assert_eq!(public_opposition_count("perhaps several"), None);
    assert_eq!(public_opposition_count("a household guard"), None);

    // One superficially strong novice is not enough: the authoritative enemy
    // also owns weapon, dodge, block, balance, and protection mechanics.
    assert!(!public_contract_assessment(1, "one", 10_000, &[strong(1, true)]).eligible);
    assert!(!public_contract_assessment(1, "two", 20_000, &[strong(1, true)]).eligible);
    assert!(
        !public_contract_assessment(1, "two", 20_000, &[strong(1, true), strong(2, true)]).eligible
    );
    assert!(
        !public_contract_assessment(6, "two", 20_000, &[strong(1, true), strong(2, true)]).eligible
    );
    assert!(!public_contract_assessment(1, "one", 10_000, &[strong(1, false)]).eligible);
    assert!(
        !public_contract_assessment(1, "several", 20_000, &[strong(1, true), strong(2, true)])
            .eligible
    );
    assert_eq!(
        public_contract_assessment(1, "one", 0, &[strong(1, true)]).reason,
        "missing_authoritative_opposition_power"
    );

    let accepted = public_contract_assessment(
        1,
        "perhaps two",
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
        "perhaps two",
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
        public_contract_assessment(1, "one", 1, &[overflow.clone()]).reason,
        "public_combat_margin_overflow"
    );
    assert_eq!(
        public_contract_assessment(1, "one", 1, &[overflow.clone(), overflow]).reason,
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
    travel.available = false;
    travel.can_travel_to_required_site = true;
    assert!(generated_action_score(&profile, &inspect) > generated_action_score(&profile, &travel));

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
