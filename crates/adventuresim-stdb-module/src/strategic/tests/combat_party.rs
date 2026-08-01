fn sampler_fixture() -> (MissionAuthority, Vec<MissionOutcomeCandidate>) {
    let site = crate::investigation::CaseSiteId::from("case-site:test".to_string());
    let mission = MissionAuthority {
        id: "mission:test".into(),
        party_id: "party:test".into(),
        case_site_id: Some(site.clone()),
        hostile_group_id: Some("hostile-group:test".into()),
        observer_character_id: 7,
        case_id: "case:test".into(),
        outcome_entropy: 0x1234_5678_9abc_def0,
        status: MissionAttemptStatus::Bound,
        committed_resolution: None,
        committed_capture_subject_id: None,
        committed_capture_custody_version: None,
        scene_key: "crypt".into(),
        hostile_version: 1,
        enemy_count: 1,
        enemy_difficulty: 1,
        enemy_combat_scale_bps: 10_000,
        normalized_combat_power: 10_000,
        drop_item_id: None,
        drop_quantity: 0,
    };
    let mission_id = mission.id.clone();
    let case_id = mission.case_id.clone();
    let candidate = |id: &str, resolution, weight| MissionOutcomeCandidate {
        id: id.into(),
        mission_id: mission_id.clone(),
        capability_id: format!("capability:{id}"),
        case_id: case_id.clone(),
        case_site_id: site.clone(),
        hostile_group_id: "hostile-group:test".into(),
        path_index: 0,
        objective_id: format!("objective:{id}"),
        resolution,
        weight,
        capture_subject_id: None,
        capture_custody_version: None,
    };
    (
        mission,
        vec![
            candidate("candidate:b", HostileResolutionKind::DrivenOff, 30),
            candidate("candidate:a", HostileResolutionKind::Defeated, 50),
            candidate("candidate:c", HostileResolutionKind::Captured, 20),
        ],
    )
}

#[test]
fn quest_encounter_influence_is_scoped_to_outbound_case_site_destinations() {
    let outbound = JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
        id: CaseSiteId::from("case-site:test".to_string()),
        name: "Test Site".into(),
    });
    assert_eq!(
        quest_influence_case_site_id(&outbound),
        Some("case-site:test")
    );
    assert_eq!(
        quest_encounter_archetype("bandit"),
        Some(EncounterArchetype::Bandits)
    );

    let returning = JourneyEndpoint::Settlement(JourneySettlementEndpoint {
        id: "settlement:test".into(),
        name: "Test Settlement".into(),
    });
    assert_eq!(quest_influence_case_site_id(&returning), None);
    assert_eq!(
        quest_influence_case_site_id(&JourneyEndpoint::Camp("camp:test".into())),
        None
    );

    let multi_site_groups = [
        (
            "group:first".into(),
            "case-site:other".into(),
            "skeleton".into(),
        ),
        (
            "group:destination".into(),
            "case-site:test".into(),
            "bandit".into(),
        ),
        (
            "group:last".into(),
            "case-site:test".into(),
            "skeleton".into(),
        ),
    ];
    assert_eq!(
        destination_hostile_archetype("case-site:test", multi_site_groups),
        Some(EncounterArchetype::Bandits)
    );

    let interrupt = STRATEGIC_SOURCE
        .split("fn maybe_interrupt_travel")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn advance_party_movement_until_encounter")
                .next()
        })
        .unwrap();
    assert!(interrupt.contains("quest_influence_case_site_id(&journey.destination)"));
    assert!(!interrupt.contains("case_site_id().unwrap()"));
}

#[test]
fn persistent_npc_chat_authority_accepts_generated_ids_without_trusting_their_prefix() {
    assert!(npc_conversation_authority_matches(
        "riverdale",
        "riverdale",
        41,
        41,
        "riverdale",
        "inn",
        "inn",
        0,
        1_440,
        720,
    ));
    assert!(!npc_conversation_authority_matches(
        "riverdale",
        "ironforge",
        41,
        41,
        "riverdale",
        "inn",
        "inn",
        0,
        1_440,
        720,
    ));
    assert!(!npc_conversation_authority_matches(
        "riverdale",
        "riverdale",
        41,
        42,
        "riverdale",
        "inn",
        "inn",
        0,
        1_440,
        720,
    ));
    assert!(!npc_conversation_authority_matches(
        "riverdale",
        "riverdale",
        41,
        41,
        "riverdale",
        "inn",
        "inn",
        0,
        600,
        720,
    ));
    assert!(!npc_conversation_authority_matches(
        "riverdale",
        "riverdale",
        41,
        41,
        "riverdale",
        "inn",
        "market",
        0,
        1_440,
        720,
    ));
}

#[test]
fn private_chat_projection_only_emits_rows_for_the_two_parties() {
    let row = LocalChatMessage {
        id: 9,
        gateway_bucket: 0,
        audience_party_id: "party:a".into(),
        other_party_id: "party:b".into(),
        resident_character_id: Some(41),
        sender_id: 10,
        sender_name: "Ada".into(),
        body: "Meet by the well.".into(),
        created_micros: 20,
    };
    let projected = project_local_chat_message(row, &[10, 11], &[20]);
    assert_eq!(
        projected
            .iter()
            .map(|message| message.owner_character_id)
            .collect::<Vec<_>>(),
        vec![10, 11, 20]
    );
    assert!(
        projected
            .iter()
            .all(|message| message.body == "Meet by the well.")
    );
    assert_eq!(projected[0].subject_party_id, "party:b");
    assert_eq!(projected[2].subject_party_id, "party:a");
    assert!(
        !projected
            .iter()
            .any(|message| message.owner_character_id == 30)
    );
}

#[test]
fn local_chat_writes_are_gateway_only_and_raw_rows_are_private() {
    let source = STRATEGIC_SOURCE;
    assert!(source.contains("#[table(accessor = local_chat_message)]"));
    assert!(!source.contains("#[table(accessor = local_chat_message, public)]"));
    let reducer = source
        .split("pub fn send_local_chat_message")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("local chat reducer");
    assert!(reducer.contains("require_strategic_gateway(ctx)?"));
    assert!(reducer.contains("location_id: String"));
    let view = source
        .split("pub fn backend_local_chat_messages")
        .nth(1)
        .and_then(|tail| tail.split("/// Scripted dialogue").next())
        .expect("backend local chat view");
    assert!(view.contains("strategic_view_is_gateway(ctx)"));
    assert!(!view.contains("conversation_key"));
}

#[test]
fn generated_witness_hair_is_not_duplicated_in_referral_prose() {
    let description = generated_witness_visible_description(
        "average height",
        "sturdy",
        "brown hair",
        "practical local woolens",
    );
    assert_eq!(
        description,
        "average height, sturdy, with brown hair, wearing practical local woolens"
    );
    assert!(!description.contains("hair hair"));
}

#[test]
fn strategic_outcome_sampling_is_canonical_and_retry_stable() {
    let (mission, candidates) = sampler_fixture();
    let first = sample_mission_candidate(&mission, candidates.clone()).unwrap();
    let retry = sample_mission_candidate(&mission, candidates.clone()).unwrap();
    let reversed =
        sample_mission_candidate(&mission, candidates.into_iter().rev().collect()).unwrap();
    assert_eq!(first.id, retry.id);
    assert_eq!(first.id, reversed.id);
}

#[test]
fn zero_weight_candidates_are_never_selected() {
    let (mission, mut candidates) = sampler_fixture();
    for candidate in &mut candidates {
        candidate.weight = 0;
    }
    assert!(sample_mission_candidate(&mission, candidates).is_none());
}

#[test]
fn private_entropy_changes_draw_basis_and_candidate_order_does_not() {
    let (mission, candidates) = sampler_fixture();
    let mut other = mission.clone();
    other.outcome_entropy ^= u64::MAX;
    assert_ne!(
        super::mission_outcome_draw(&mission, &candidates),
        super::mission_outcome_draw(&other, &candidates)
    );
    let reversed = candidates.iter().cloned().rev().collect::<Vec<_>>();
    assert_eq!(
        super::mission_outcome_draw(&mission, &candidates),
        super::mission_outcome_draw(&mission, &reversed)
    );
    let mut caller_renamed = mission.clone();
    caller_renamed.id = "mission:another-caller-choice".into();
    assert_eq!(
        sample_mission_candidate(&mission, candidates.clone())
            .unwrap()
            .id,
        sample_mission_candidate(&caller_renamed, candidates)
            .unwrap()
            .id,
        "caller-controlled mission IDs cannot grind the private draw"
    );
}

#[test]
fn approach_identity_versions_exact_capture_custody_and_authority() {
    let id = |site: &str, group: &str, resolution, version| {
        super::mission_approach_capability_id(
            7,
            "case:test",
            site,
            group,
            0,
            "objective:capture",
            resolution,
            Some("subject:test"),
            Some(version),
        )
    };
    let original = id(
        "case-site:a",
        "hostile-group:a",
        HostileResolutionKind::Captured,
        3,
    );
    assert_ne!(
        original,
        id(
            "case-site:a",
            "hostile-group:a",
            HostileResolutionKind::Captured,
            4
        )
    );
    assert_ne!(
        original,
        id(
            "case-site:b",
            "hostile-group:a",
            HostileResolutionKind::Captured,
            3
        )
    );
    assert_ne!(
        original,
        id(
            "case-site:a",
            "hostile-group:b",
            HostileResolutionKind::Captured,
            3
        )
    );
    assert_ne!(
        original,
        id(
            "case-site:a",
            "hostile-group:a",
            HostileResolutionKind::Defeated,
            3
        )
    );
}

#[test]
fn enemy_archetypes_keep_combat_and_loot_classification_together() {
    assert_eq!(autoresolve_drop("goblin"), Ok(Some("self_bow")));
    assert_eq!(autoresolve_drop("bandit"), Ok(Some("katzbalger")));
    assert!(autoresolve_drop("unknown menace").is_err());
}

#[test]
fn only_supported_active_quest_enemies_influence_random_encounters() {
    assert_eq!(
        quest_encounter_archetype("skeleton"),
        Some(EncounterArchetype::Undead)
    );
    assert_eq!(
        quest_encounter_archetype("goblin"),
        Some(EncounterArchetype::Goblins)
    );
    assert_eq!(
        quest_encounter_archetype("bandit"),
        Some(EncounterArchetype::Bandits)
    );
    assert_eq!(quest_encounter_archetype("giant_spider"), None);
}

#[test]
fn random_encounter_battle_cannot_complete_or_record_a_quest() {
    let source = STRATEGIC_SOURCE;
    let random_battle = source
        .split("fn resolve_random_encounter_battle")
        .nth(1)
        .and_then(|tail| tail.split("pub fn resolve_strategic_encounter").next())
        .expect("random encounter battle implementation");
    assert!(!random_battle.contains("complete_quest("));
    assert!(!random_battle.contains("record_battle_result("));
}

#[test]
fn encounter_resolution_requires_character_authority_and_uses_private_entropy() {
    let source = STRATEGIC_SOURCE;
    let reducer = source
        .split("pub fn resolve_strategic_encounter")
        .nth(1)
        .and_then(|tail| tail.split("pub fn complete_quest").next())
        .expect("encounter resolution reducer");
    assert!(reducer.contains("require_strategic_character_authority(ctx, character_id)?"));
    assert!(reducer.contains("party_journey_encounter_authority()"));

    let encounter = source
        .split("pub struct StrategicEncounter")
        .nth(1)
        .and_then(|tail| tail.split("pub struct PartyJourneyItinerary").next())
        .expect("public encounter schema");
    assert!(!encounter.contains("pub seed:"));
}

#[test]
fn surrender_preview_refresh_is_revisioned_receipted_and_retry_safe() {
    let source = STRATEGIC_SOURCE;
    let reducer = source
        .split("pub fn resolve_strategic_encounter")
        .nth(1)
        .and_then(|tail| tail.split("pub fn complete_quest").next())
        .expect("encounter resolution reducer");
    let receipt_replay = reducer
        .find("strategic_encounter_retry_matches")
        .expect("receipt replay gate");
    let unresolved = reducer
        .find("unresolved_encounter")
        .expect("unresolved encounter read");
    assert!(receipt_replay < unresolved);
    let refresh = reducer
        .split("if current != encounter.loss_preview")
        .nth(1)
        .and_then(|tail| tail.split("commit_encounter_surrender").next())
        .expect("surrender preview refresh branch");
    assert!(refresh.contains("encounter.revision.saturating_add(1)"));
    assert!(refresh.contains("StrategicEncounterResolutionReceipt"));
    assert!(refresh.contains("preview_refreshed"));
    assert!(refresh.contains("return Ok(())"));
    assert!(reducer.contains("encounter.revision != expected_revision"));
}

#[test]
fn random_and_authored_encounters_share_the_only_constructor_literal() {
    let encounters = include_str!("../encounters.rs");
    assert_eq!(encounters.matches("StrategicEncounter {").count(), 1);
    let random = encounters
        .split("fn maybe_interrupt_travel")
        .nth(1)
        .and_then(|tail| {
            tail.split("fn advance_party_movement_until_encounter")
                .next()
        })
        .expect("random encounter materialization");
    assert!(random.contains("build_strategic_encounter("));
    let authored = include_str!("../challenges.rs");
    assert!(authored.contains("build_strategic_encounter("));
}

#[test]
fn recovery_direction_is_delegated_only_to_a_ready_member_for_an_unready_leader() {
    let source = STRATEGIC_SOURCE;
    let direction = source
        .split("pub(crate) fn party_member_can_direct_field_rest")
        .nth(1)
        .and_then(|tail| tail.split("fn authoritative_evacuation_settlement").next())
        .expect("field recovery direction gate");
    assert!(direction.contains("party.leader_id == character_id"));
    assert!(direction.contains("party.current_settlement_id.is_none()"));
    assert!(direction.contains("ready_companion_may_direct_recovery"));

    let time = include_str!("../../time.rs");
    let camp = time
        .split("pub fn rest_at_camp")
        .nth(1)
        .and_then(|tail| tail.split("fn party_fatigue_summary").next())
        .expect("camp reducer");
    assert!(camp.contains("party_member_can_direct_field_rest"));
}

#[test]
fn unresolved_encounters_guard_party_and_preview_mutations() {
    let source = STRATEGIC_SOURCE;
    for (function, guard) in [
        ("vote_for_party_leader", "require_no_unresolved_encounter"),
        (
            "accept_party_join_request",
            "require_no_unresolved_encounter",
        ),
        (
            "finalize_party_offer",
            "require_character_no_unresolved_encounter",
        ),
        (
            "remove_party_member",
            "require_character_no_unresolved_encounter",
        ),
        (
            "abandon_contract",
            "require_character_no_unresolved_encounter",
        ),
    ] {
        let body = source
            .split(&format!("pub fn {function}"))
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .unwrap_or_else(|| panic!("{function} reducer body"));
        assert!(body.contains(guard), "{function} must call {guard}");
    }
}

#[test]
fn quest_autoresolve_routes_consequences_through_shared_commit() {
    let source = STRATEGIC_SOURCE;
    let body = source
        .split("pub fn autoresolve_mission")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("quest autoresolve reducer body");
    assert!(body.contains("commit_autoresolve_outcome("));
    assert!(!body.contains("record_autoresolve_report("));
    assert!(!body.contains("consume_autoresolve_ammunition("));
}

#[test]
fn contract_schema_has_no_destination_or_tracking_authority() {
    let source = STRATEGIC_SOURCE;
    let schema = source
        .split("pub struct Contract {")
        .nth(1)
        .and_then(|tail| tail.split("pub struct CaseOutcome").next())
        .expect("contract schema");
    for forbidden in [
        "location_description",
        "location_scene_key",
        "location_coord_x",
        "location_coord_y",
        "coordinates_are_geographic",
        "distance_m",
        "tracked",
    ] {
        assert!(
            !schema.contains(forbidden),
            "Contract still owns {forbidden}"
        );
    }
}

#[test]
fn battle_outcome_and_loot_schema_have_no_quest_keys() {
    let source = STRATEGIC_SOURCE;
    for (schema, next) in [
        ("BattleResult", "AutoresolveReport"),
        ("AutoresolveReport", "BattleLootItem"),
        ("BattleLootItem", "BattleParticipant"),
        ("BattleParticipant", "MissionAuthority"),
    ] {
        let body = source
            .split(&format!("pub struct {schema}"))
            .nth(1)
            .and_then(|tail| tail.split(&format!("pub struct {next}")).next())
            .expect("schema body");
        assert!(!body.contains("quest_id"), "{schema} retained a quest key");
    }
}

#[test]
fn npc_recruitment_authority_is_independent_from_quests() {
    let source = STRATEGIC_SOURCE;
    let body = source
        .split("fn ensure_npc_recruiting_parties")
        .nth(1)
        .and_then(|tail| tail.split("fn generate_quest_for_settlement").next())
        .expect("NPC recruiting population");
    assert!(body.contains("recruitment_offer()"));
    assert!(body.contains("RecruitmentOfferStatus::Open"));
    assert!(!body.contains(".quest()"));
    assert!(!body.contains("active_quest_id"));
    assert!(!body.contains("accepted_by"));

    let request = source
        .split("pub fn request_to_join_party")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("join request reducer");
    assert!(request.contains("require_open_recruitment_offer"));
    assert!(request.contains("return Ok(())"));
    let accept = source
        .split("pub fn accept_party_join_request")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("join acceptance reducer");
    assert!(accept.contains("require_open_recruitment_offer"));
}

#[test]
fn incidents_own_sources_sites_and_lifecycle_without_quest_side_effects() {
    let source = STRATEGIC_SOURCE;
    let schema = source
        .split("pub struct StrategicIncident")
        .nth(1)
        .and_then(|tail| tail.split("pub struct RecruitmentOfferId").next())
        .expect("incident schema");
    for required in [
        "IncidentSourceId",
        "IncidentKind",
        "IncidentStatus",
        "CaseSiteId",
        "hostile_group_id",
    ] {
        assert!(schema.contains(required), "incident lacks {required}");
    }
    for forbidden in ["quest_id", "previous_active_quest_id"] {
        assert!(!schema.contains(forbidden), "incident retained {forbidden}");
    }

    let create = source
        .split("fn create_strategic_incident")
        .nth(1)
        .and_then(|tail| tail.split("fn maybe_trigger_religious_incident").next())
        .expect("incident creation");
    assert!(create.contains("source_id == source_id"));
    assert!(create.contains("materialize_hostile_group"));
    assert!(!create.contains("ctx.db.quest()"));
    assert!(!create.contains("active_quest_id"));

    let finish = source
        .split("fn finish_strategic_incident")
        .nth(1)
        .and_then(|tail| tail.split("pub(crate) fn finish_incident").next())
        .expect("incident completion");
    assert!(!finish.contains("ctx.db.quest()"));
    assert!(!finish.contains("active_quest_id"));
}

#[test]
fn recruitment_offer_lifecycle_expires_and_closes_stale_bindings() {
    assert_eq!(
        refreshed_recruitment_offer_status(RecruitmentOfferStatus::Open, 10, 20, true),
        RecruitmentOfferStatus::Open
    );
    assert_eq!(
        refreshed_recruitment_offer_status(RecruitmentOfferStatus::Open, 20, 20, true),
        RecruitmentOfferStatus::Expired
    );
    assert_eq!(
        refreshed_recruitment_offer_status(RecruitmentOfferStatus::Open, 10, 20, false),
        RecruitmentOfferStatus::Closed
    );
    assert_eq!(
        refreshed_recruitment_offer_status(RecruitmentOfferStatus::Open, 20, 20, false),
        RecruitmentOfferStatus::Closed
    );
    let first = renewed_recruitment_offer_expiry(20);
    let second = renewed_recruitment_offer_expiry(first);
    assert!(first > 20);
    assert!(second > first);
}

#[test]
fn recruitment_offer_requires_every_authoritative_location_binding() {
    let offer = RecruitmentOffer {
        id_key: "offer".into(),
        id: RecruitmentOfferId {
            value: "offer".into(),
        },
        source_id: RecruitmentSourceId {
            value: "source".into(),
        },
        recruiting_party_id: "party".into(),
        settlement_id: "lubeck".into(),
        settlement_resident_id: 41,
        location_id: "inn".into(),
        leader_id: 7,
        status: RecruitmentOfferStatus::Open,
        created_at_minute: 0,
        expires_at_minute: 10,
    };
    let live = RecruitmentOfferBindingFields {
        party_leader_id: 7,
        party_settlement_id: Some("lubeck"),
        leader_alive: true,
        leader_party_id: Some("party"),
        leader_settlement_id: Some("lubeck"),
        npc_home_settlement_id: Some("lubeck"),
        presence_settlement_id: Some("lubeck"),
        presence_location_id: Some("inn"),
        presence_is_current: true,
    };
    assert!(recruitment_offer_binding_fields_are_live(&offer, live));
    for stale in [
        RecruitmentOfferBindingFields {
            party_settlement_id: Some("hamburg"),
            ..live
        },
        RecruitmentOfferBindingFields {
            leader_settlement_id: Some("hamburg"),
            ..live
        },
        RecruitmentOfferBindingFields {
            npc_home_settlement_id: Some("hamburg"),
            ..live
        },
        RecruitmentOfferBindingFields {
            presence_location_id: Some("market"),
            ..live
        },
        RecruitmentOfferBindingFields {
            leader_alive: false,
            ..live
        },
        RecruitmentOfferBindingFields {
            leader_party_id: Some("other-party"),
            ..live
        },
        RecruitmentOfferBindingFields {
            presence_is_current: false,
            ..live
        },
    ] {
        assert!(!recruitment_offer_binding_fields_are_live(&offer, stale));
    }
    let source = STRATEGIC_SOURCE;
    assert_eq!(
        source
            .matches("recruitment_offer_bindings_are_live(ctx, &offer, now)")
            .count(),
        2
    );
}

#[test]
fn all_dead_party_teardown_clears_only_strategic_ghost_state() {
    let source = STRATEGIC_SOURCE;
    let teardown = source
        .split("pub(crate) fn teardown_all_dead_strategic_party")
        .nth(1)
        .and_then(|tail| tail.split("/// Lazily backfills").next())
        .expect("all-dead teardown");
    for required in [
        "party.camp_destination = None",
        "party.camp_remaining_minutes = 0",
        "finish_party_journey(ctx, party_id)",
        "strategic_encounter()",
        "party_action_request_authority()",
        "party_leader_vote()",
        "party_join_request()",
        "party_recruitment_role()",
        "delete_recruitment_role_authority(ctx, role.id)",
    ] {
        assert!(teardown.contains(required), "missing {required}");
    }
    for preserved in [
        "mission_authority()",
        "tactical_server_authority()",
        "party_inventory_item()",
        "party_authority().id().delete",
    ] {
        assert!(!teardown.contains(preserved), "must preserve {preserved}");
    }

    let time = include_str!("../../time.rs");
    let camp = time
        .split("pub fn rest_at_camp")
        .nth(1)
        .and_then(|tail| tail.split("fn party_fatigue_summary").next())
        .expect("camp rest reducer");
    let empty = camp.find("if living_after.is_empty()").unwrap();
    assert!(empty < camp.find("record_party_camp_rest").unwrap());
    assert!(empty < camp.find("refresh_party_journey_forecast").unwrap());
}

#[test]
fn join_entry_points_reject_an_all_dead_target_before_creating_state() {
    let source = STRATEGIC_SOURCE;
    let guard = source
        .split("fn require_living_recruitment_target")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("living recruitment target guard");
    assert!(guard.contains("!leader.alive"));
    assert!(guard.contains("living_party_member_ids"));

    let specific = source
        .split("pub fn request_to_join_party")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("specific join request");
    assert!(
        specific.find("require_living_recruitment_target").unwrap()
            < specific.find("party_join_request().insert").unwrap()
    );

    let general = source
        .split("pub fn request_general_party_join")
        .nth(1)
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("general join request");
    assert!(
        general.find("require_living_recruitment_target").unwrap()
            < general.find(".insert(PartyRecruitmentRole").unwrap()
    );
}
#[test]
fn authoritative_combat_power_tracks_enemy_difficulty() {
    let novice = autoresolve_enemy(1, "cultist", 1, 10_000).unwrap();
    let veteran = autoresolve_enemy(2, "cultist", 6, 10_000).unwrap();
    assert!(
        adventuresim_core::autoresolve::autoresolve_combat_power(&veteran)
            > adventuresim_core::autoresolve::autoresolve_combat_power(&novice)
    );
}
