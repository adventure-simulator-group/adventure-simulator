fn capability(character_id: u64, melee: bool, ranged: bool, precise: bool, heavy: bool) -> CharacterCapability {
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
        select_expedition_encounter_choice(&["attack".into()], false).unwrap().choice,
        "attack"
    );
    assert!(select_expedition_encounter_choice(&["attack".into()], true).is_none());
}

#[test]
fn public_contract_ceiling_has_conservative_inclusive_boundaries() {
    assert_eq!(public_contract_difficulty_ceiling(&[]), 0);
    assert_eq!(public_contract_difficulty_ceiling(&[capability(1, false, false, false, false)]), 0);
    assert_eq!(public_contract_difficulty_ceiling(&[capability(1, true, false, false, false)]), 1);
    assert_eq!(public_contract_difficulty_ceiling(&[
        capability(1, true, false, true, true),
        capability(2, false, true, false, false),
    ]), 4);
    assert!(!public_contract_is_eligible(0, 4));
    assert!(public_contract_is_eligible(4, 4));
    assert!(!public_contract_is_eligible(5, 4));
}

#[test]
fn generated_action_score_prefers_progress_then_fit_then_public_costs() {
    let mut profile = generate_profile(42, 0);
    profile.initial_skills.insight = 8_000.0;
    profile.initial_skills.stealth = 1_000.0;
    let interview = projected_action("z", "interview");
    let search = projected_action("a", "search_area");
    assert!(generated_action_score(&profile, &interview) > generated_action_score(&profile, &search));

    let mut travel = interview.clone();
    travel.available = false;
    travel.can_travel_to_required_site = true;
    assert!(generated_action_score(&profile, &interview) > generated_action_score(&profile, &travel));

    let mut uncertain = interview.clone();
    uncertain.uncertainty_bps = 9_000;
    assert!(generated_action_score(&profile, &interview) > generated_action_score(&profile, &uncertain));

    let tied_a = projected_action("a", "unknown");
    let tied_b = projected_action("b", "unknown");
    assert_eq!(generated_action_score(&profile, &tied_a), generated_action_score(&profile, &tied_b));
    let mut tied = vec![tied_b, tied_a];
    sort_generated_actions(&profile, &mut tied);
    assert_eq!(tied.iter().map(|action| action.action_id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
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
    assert!(groups.iter().all(|group| !profiles[group[0]].build.activity_only));
    assert!(groups.iter().all(|group| {
        let mut ranks = group.iter().map(|&index| role_rank(profiles[index].build.role)).collect::<Vec<_>>();
        ranks.sort_unstable();
        ranks.dedup();
        ranks.len() == group.len()
    }));

    for profile in &mut profiles { profile.build.activity_only = true; }
    let all_content = balanced_party_groups(&profiles, 3);
    assert!(all_content.iter().all(|group| profiles[group[0]].build.activity_only));
}

#[test]
fn defeat_policy_has_no_rng_retry_and_generated_retries_need_public_change() {
    let production = LIVE_CORE_SOURCE.split("#[cfg(test)]").next().unwrap();
    assert!(!production.contains("MAX_DEFEAT_RETRIES"));
    assert!(!production.contains("retry_travel_to_case_site"));
    assert!(production.contains("reason=unchanged_defeated_threat"));
    assert!(production.contains("generated_defeat_fingerprints"));
    assert!(production.contains("public_party_combat_fingerprint"));
    assert!(production.contains("no_safe_contract"));
}
