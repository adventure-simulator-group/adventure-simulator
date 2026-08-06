fn generated_case(
    seed: u64,
    family: adventuresim_core::quest_generation::TemplateFamily,
) -> adventuresim_core::quest_generation::GeneratedCase {
    adventuresim_core::quest_generation::generate(
        &adventuresim_core::quest_generation::GenerationContext {
            seed,
            observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
            observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
            settlement_id: "test-settlement".into(),
            settlement_name: "Test Settlement".into(),
            scope: adventuresim_core::local_problem::Scope::Settlement {
                settlement_id: "test-settlement".into(),
            },
            ordinal: 0,
            now_minute: 10_000,
            incident_weather: adventuresim_core::weather::Precipitation::Clear,
            requested_family: Some(family),
            witness_candidates: adventuresim_core::quest_generation::test_witnesses(),
        },
    )
    .unwrap()
}

#[test]
fn simulation_quest_fixture_exposes_ordinary_provisioning_to_both_paths() {
    use adventuresim_core::settlement_economy::{CatalogKind, Storefront, storefront_stocks};
    use adventuresim_world_schema::{SettlementEconomyProfile, SettlementService, StockCategory};

    let economy =
        simulation_quest_provisioning_economy(SettlementEconomyProfile::stage_placeholder())
            .unwrap();
    assert!(economy.services.contains(&SettlementService::GeneralStore));
    assert!(
        economy
            .stock
            .iter()
            .any(|stock| stock.category == StockCategory::GeneralGoods)
    );
    for (item_id, kind) in [
        (
            adventuresim_core::provisioning::STANDARD_TRAVEL_RATION_ID,
            CatalogKind::Food,
        ),
        (
            adventuresim_core::provisioning::STANDARD_WATERSKIN_ID,
            CatalogKind::Simple,
        ),
    ] {
        assert!(storefront_stocks(
            &economy,
            Storefront::General,
            item_id,
            kind
        ));
    }

    let environment = STRATEGIC_SOURCE
        .split("fn ensure_simulation_quest_provisioning_environment")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn seed_simulation_quest_fixture_inner")
                .next()
        })
        .unwrap();
    assert!(
        environment
            .contains("default_merchant_provider(ctx, &settlement_id, \"merchants\", \"market\")")
    );
    assert!(environment.contains("npc_is_present(ctx, &provider, minute)"));

    let fixture = STRATEGIC_SOURCE
        .split("pub(crate) fn seed_simulation_quest_fixture_inner")
        .nth(1)
        .and_then(|tail| tail.split("fn materialize_generated_quest").next())
        .unwrap();
    assert!(
        fixture.contains("ensure_simulation_quest_provisioning_environment(ctx, direct_leader_id)")
    );
    assert!(
        fixture
            .contains("ensure_simulation_quest_provisioning_environment(ctx, generated_leader_id)")
    );
    assert!(fixture.contains("SIMULATION_QUEST_ENEMY_TYPE.into()"));
    assert!(fixture.contains("SIMULATION_QUEST_ENEMY_DIFFICULTY"));
    assert!(!fixture.contains("\"bandit\".into(), 1, 1"));
}

#[test]
fn acceptance_fixture_selects_before_materialization_without_rewriting_sites() {
    let source = STRATEGIC_SOURCE;
    let fixture = source
        .split("pub(crate) fn seed_simulation_quest_fixture_inner")
        .nth(1)
        .and_then(|tail| tail.split("fn materialize_generated_quest").next())
        .expect("simulation acceptance fixture");
    assert!(fixture.contains("materialize_simulation_acceptance_outbreak"));
    assert!(!source.contains("bound_simulation_acceptance_generated_case_sites"));
    assert!(!source.contains("site.distance_m = distance_m"));

    let ordinary_generation = source
        .split("fn materialize_generated_quest")
        .nth(1)
        .expect("ordinary generated quest materialization");
    assert!(ordinary_generation.contains("ordinary_generated_site_distance_m(seed, index)"));
    assert_eq!(ordinary_generated_site_distance_m(0, 0), 4_000);
    assert!((0..64).all(|index| {
        (4_000..21_000).contains(&ordinary_generated_site_distance_m(u64::MAX, index))
    }));
    let selector = source
        .split("fn materialize_simulation_acceptance_outbreak")
        .nth(1)
        .and_then(|tail| tail.split("fn seed_outbreak_demo").next())
        .expect("acceptance outbreak selector");
    let selection = selector
        .find("generated.sites.iter().enumerate().any")
        .unwrap();
    let materialization = selector.find("materialize_generated_quest").unwrap();
    assert!(selection < materialization);
}

#[test]
fn generated_return_and_expose_bind_only_the_authored_case_and_recipient() {
    use adventuresim_core::quest_generation::{
        CanonicalCause, GeneratedDialogueAction, TemplateFamily,
    };
    use adventuresim_dialogue::InvestigationAction;

    let incidental = (0..1024)
        .map(|seed| generated_case(seed, TemplateFamily::DisappearanceOrLoss))
        .find(|generated| generated.cause == CanonicalCause::IncidentalLoss)
        .unwrap();
    let fabricated = (0..1024)
        .map(|seed| generated_case(seed, TemplateFamily::DisappearanceOrLoss))
        .find(|generated| generated.cause == CanonicalCause::FabricatedClaim)
        .unwrap();
    let cases = vec![incidental, fabricated];
    assert!(cases.iter().any(|case| {
        matches!(case.cause, CanonicalCause::IncidentalLoss)
            && case
                .dialogue_producers
                .iter()
                .any(|producer| producer.action == GeneratedDialogueAction::ReturnAsset)
    }));
    assert!(cases.iter().any(|case| {
        matches!(case.cause, CanonicalCause::FabricatedClaim)
            && case
                .dialogue_producers
                .iter()
                .any(|producer| producer.action == GeneratedDialogueAction::Expose)
    }));
    for generated in cases {
        let producer = &generated.dialogue_producers[0];
        let action = match producer.action {
            GeneratedDialogueAction::ReturnAsset => InvestigationAction::ReturnAsset,
            GeneratedDialogueAction::Expose => InvestigationAction::Expose,
        };
        let present = HashSet::from([producer.recipient_resident_character_id.to_string()]);
        assert_eq!(
            generated_dialogue_producer_recipient(
                &generated,
                producer.objective_id.as_str(),
                &action,
                &present,
            ),
            Some(producer.recipient_resident_character_id.to_string())
        );
        assert!(
            generated_dialogue_producer_recipient(
                &generated,
                producer.objective_id.as_str(),
                &action,
                &HashSet::from(["wrong-npc".into()]),
            )
            .is_none()
        );
        assert!(
            generated_dialogue_producer_recipient(
                &generated,
                "objective:unrelated",
                &action,
                &present,
            )
            .is_none()
        );
        let wrong_action = match action {
            InvestigationAction::ReturnAsset => InvestigationAction::Expose,
            InvestigationAction::Expose => InvestigationAction::ReturnAsset,
            _ => unreachable!(),
        };
        assert!(!generated_dialogue_action_matches(
            producer.action,
            &wrong_action
        ));
    }

    let source = STRATEGIC_SOURCE;
    let consumer = source
        .split("fn apply_dialogue_investigation_action")
        .nth(1)
        .and_then(|tail| tail.split("fn same_location").next())
        .unwrap();
    assert!(consumer.contains("generated_dialogue_recipient("));
    assert!(consumer.contains("binding.consumed_by = action_id.into()"));
    assert!(consumer.contains("recipient != binding.intended_recipient_id"));
}

#[test]
fn dialogue_case_provenance_fails_closed_for_generated_authority_damage() {
    use adventuresim_core::quest_generation::TemplateFamily;
    let generated = generated_case(11, TemplateFamily::DisappearanceOrLoss);
    let generated_case = CaseAuthority {
        id: generated.canonical_case_id.clone(),
        investigation_case_id: generated.canonical_case_id.clone(),
        provenance_kind: "generated".into(),
        generated_case_id: generated.canonical_case_id.clone(),
        local_problem_id: Some(generated.problem_id.clone()),
        objective_expression_json: serde_json::to_string(&generated.objectives).unwrap(),
        resolution_status: CaseResolutionStatus::Open,
        resolved_by_party_id: None,
    };
    let context = adventuresim_core::quest_generation::GenerationContext {
        seed: generated.generation_seed,
        observer_entropy_hi: generated.generation_seed ^ 0x6f62_7365_7276_6572,
        observer_entropy_lo: generated.generation_seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
        settlement_id: "test-settlement".into(),
        settlement_name: "Test Settlement".into(),
        scope: adventuresim_core::local_problem::Scope::Settlement {
            settlement_id: "test-settlement".into(),
        },
        ordinal: 0,
        now_minute: 10_000,
        incident_weather: adventuresim_core::weather::Precipitation::Clear,
        requested_family: Some(TemplateFamily::DisappearanceOrLoss),
        witness_candidates: adventuresim_core::quest_generation::test_witnesses(),
    };
    let context_snapshot_json = serde_json::to_string(&context).unwrap();
    let authority = QuestGenerationAuthority {
        case_id: generated.canonical_case_id.clone(),
        public_case_id: generated.public_case_id.clone(),
        settlement_id: context.settlement_id.clone(),
        settlement_name: context.settlement_name.clone(),
        seed: generated.generation_seed,
        catalog_revision: generated.catalog_revision.clone(),
        context_commitment: quest_generation_context_commitment(&context_snapshot_json),
        context_snapshot_json,
        manifest_json: serde_json::to_string(&generated).unwrap(),
        factor_trace_json: serde_json::to_string(&generated.factor_trace).unwrap(),
    };
    assert!(
        validated_generated_dialogue_manifest(&generated_case, Some(&authority))
            .unwrap()
            .is_some()
    );
    assert!(validate_quest_generation_authority(&authority).is_ok());
    assert!(validated_generated_dialogue_manifest(&generated_case, None).is_err());
    let mut malformed = authority.clone();
    malformed.manifest_json = "not-json".into();
    assert!(validated_generated_dialogue_manifest(&generated_case, Some(&malformed)).is_err());
    let mut mismatched = authority.clone();
    mismatched.public_case_id = "wrong-public".into();
    assert!(validated_generated_dialogue_manifest(&generated_case, Some(&mismatched)).is_err());
    let mut wrong_objective = generated_case.clone();
    wrong_objective.objective_expression_json = "[]".into();
    assert!(validated_generated_dialogue_manifest(&wrong_objective, Some(&authority)).is_err());
    let mut mutations = Vec::new();
    let mut wrong_seed = authority.clone();
    wrong_seed.seed ^= 1;
    mutations.push(wrong_seed);
    let mut wrong_catalog = authority.clone();
    wrong_catalog.catalog_revision = "old-catalog".into();
    mutations.push(wrong_catalog);
    let mut wrong_trace = authority.clone();
    wrong_trace.factor_trace_json = "[]".into();
    mutations.push(wrong_trace);
    let mut wrong_commitment = authority.clone();
    wrong_commitment.context_commitment = "wrong".into();
    mutations.push(wrong_commitment);
    let mutate_context =
        |authority: &QuestGenerationAuthority,
         mutate: fn(&mut adventuresim_core::quest_generation::GenerationContext),
         refresh_commitment: bool| {
            let mut changed = authority.clone();
            let mut context: adventuresim_core::quest_generation::GenerationContext =
                serde_json::from_str(&changed.context_snapshot_json).unwrap();
            mutate(&mut context);
            changed.context_snapshot_json = serde_json::to_string(&context).unwrap();
            if refresh_commitment {
                changed.context_commitment =
                    quest_generation_context_commitment(&changed.context_snapshot_json);
            }
            changed
        };
    mutations.push(mutate_context(
        &authority,
        |context| context.observer_entropy_hi ^= 1,
        false,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| context.observer_entropy_lo ^= 1,
        false,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| context.seed ^= 1,
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| context.settlement_id = "other-settlement".into(),
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| context.settlement_name = "Other Settlement".into(),
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| {
            context.scope = adventuresim_core::local_problem::Scope::Settlement {
                settlement_id: "other-scope".into(),
            }
        },
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| context.ordinal = context.ordinal.saturating_add(1),
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| context.now_minute = context.now_minute.saturating_add(1),
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| {
            context.requested_family = Some(TemplateFamily::RecurringDepredation);
        },
        true,
    ));
    mutations.push(mutate_context(
        &authority,
        |context| {
            context.witness_candidates.pop();
        },
        true,
    ));
    let mut wrong_manifest = authority.clone();
    let mut changed_manifest = generated.clone();
    changed_manifest.problem_id.push_str("-changed");
    wrong_manifest.manifest_json = serde_json::to_string(&changed_manifest).unwrap();
    mutations.push(wrong_manifest);
    for mutation in mutations {
        assert!(validate_quest_generation_authority(&mutation).is_err());
    }
    let manual = CaseAuthority {
        id: "manual-case".into(),
        investigation_case_id: "manual-case".into(),
        provenance_kind: "manual".into(),
        generated_case_id: String::new(),
        local_problem_id: None,
        objective_expression_json: "{}".into(),
        resolution_status: CaseResolutionStatus::Open,
        resolved_by_party_id: None,
    };
    assert_eq!(
        validated_generated_dialogue_manifest(&manual, None).unwrap(),
        None
    );

    let source = STRATEGIC_SOURCE;
    let eligibility = source
        .split("fn issue_dialogue_investigation_bindings")
        .nth(1)
        .and_then(|tail| tail.split("fn case_has_exact_dialogue_provenance").next())
        .unwrap();
    let execution = source
        .split("fn apply_dialogue_investigation_action")
        .nth(1)
        .and_then(|tail| tail.split("fn same_location").next())
        .unwrap();
    assert!(eligibility.contains("generated_dialogue_recipient("));
    assert!(execution.contains("generated_dialogue_recipient("));
    let validator_source = source
        .split("pub(crate) fn validate_quest_generation_authority")
        .nth(1)
        .and_then(|tail| tail.split("/// A separately accepted agreement").next())
        .unwrap();
    for required in [
        "context_commitment",
        "context.seed",
        "generation_seed",
        "CATALOG_REVISION",
        "factor_trace",
        "qg::validate",
        "qg::generate",
        "regenerated != manifest",
    ] {
        assert!(validator_source.contains(required));
    }
    for consumer in [
        "fn generated_dialogue_recipient",
        "fn ensure_settlement_activity_inner",
        "fn generate_quest_for_settlement",
    ] {
        let body = source.split(consumer).nth(1).unwrap();
        assert!(body.contains("validate_quest_generation_authority"));
    }
}

#[test]
fn generated_hostile_materialization_preserves_manifest_identity_across_links() {
    use adventuresim_core::{
        case::ObjectiveRequirement,
        quest_generation::{CanonicalCause, TemplateFamily},
    };

    let recurring = generated_case(7, TemplateFamily::RecurringDepredation);
    let disappearance = (0..1024)
        .map(|seed| generated_case(seed, TemplateFamily::DisappearanceOrLoss))
        .find(|generated| matches!(generated.cause, CanonicalCause::Hostile(_)))
        .expect("disappearance family has a hostile seed");

    for generated in [recurring, disappearance] {
        let [(hostile_group_id, site_id, threat, count)] = generated.hostile_groups.as_slice()
        else {
            panic!("hostile generated case has one canonical hostile-group authority");
        };
        assert_ne!(
            hostile_group_id,
            &format!("hostile-group:{}", site_id.0),
            "observer-facing authority IDs must not embed one another"
        );
        let generated_site = generated
            .sites
            .iter()
            .find(|site| site.id == *site_id)
            .expect("hostile-group site exists");
        let site = CaseSiteAuthority {
            id_key: generated_site.id.0.clone(),
            id: crate::investigation::CaseSiteId::from(generated_site.id.0.clone()),
            case_id: generated.canonical_case_id.clone(),
            origin_settlement_id: "test-settlement".into(),
            name: generated_site.safe_label.clone(),
            description: "materialization regression".into(),
            scene_key: generated_scene_key(generated_site.kind).into(),
            longitude_e7: 0,
            latitude_e7: 0,
            coordinates_are_geographic: false,
            distance_m: 1,
        };
        let group =
            hostile_group_authority_row(hostile_group_id, &site, threat.as_str().into(), *count, 2)
                .expect("canonical hostile-group row materializes");
        assert_eq!(group.id, *hostile_group_id);
        assert_eq!(group.case_site_id_key, site.id.value);
        assert_eq!(group.case_site_id, site.id);

        let linked_finales: Vec<_> = generated
            .finales
            .iter()
            .filter_map(|finale| finale.hostile_group_id.as_deref())
            .collect();
        assert!(!linked_finales.is_empty());
        assert!(
            linked_finales
                .iter()
                .all(|linked| *linked == hostile_group_id)
        );

        let mut candidates = Vec::new();
        for (path_index, path) in generated.objectives.alternatives.iter().enumerate() {
            for objective in &path.objectives {
                match &objective.requirement {
                    ObjectiveRequirement::Defeat {
                        hostile_group_id: linked,
                        ..
                    }
                    | ObjectiveRequirement::DriveOff {
                        hostile_group_id: linked,
                    }
                    | ObjectiveRequirement::Surrender {
                        hostile_group_id: linked,
                    } => assert_eq!(linked, hostile_group_id),
                    _ => {}
                }
                let Some((resolution, weight)) =
                    hostile_resolution_for_objective(&objective.requirement, hostile_group_id)
                else {
                    continue;
                };
                let capability = MissionApproachCapability {
                    id: format!("capability:{}", objective.id.as_str()),
                    observer_character_id: 7,
                    hostile_group_id: group.id.clone(),
                    case_id: generated.canonical_case_id.clone(),
                    case_site_id: site.id.clone(),
                    path_index: path_index as u16,
                    objective_id: objective.id.as_str().into(),
                    resolution,
                    weight,
                    capture_subject_id: None,
                    capture_custody_version: None,
                    active: true,
                };
                candidates.push(mission_candidate_from_capability(
                    "mission:generated",
                    candidates.len(),
                    capability,
                ));
            }
        }
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.hostile_group_id == *hostile_group_id)
        );
        if generated.family == TemplateFamily::RecurringDepredation {
            assert_eq!(candidates.len(), 3);
            assert!(candidates.iter().any(|candidate| {
                candidate.resolution == HostileResolutionKind::Defeated && candidate.weight == 50
            }));
            assert!(candidates.iter().any(|candidate| {
                candidate.resolution == HostileResolutionKind::DrivenOff && candidate.weight == 30
            }));
            assert!(candidates.iter().any(|candidate| {
                candidate.resolution == HostileResolutionKind::Surrendered
                    && candidate.weight == 20
            }));
        }
    }

    let materializer = STRATEGIC_SOURCE
        .split("fn materialize_hostile_group")
        .nth(1)
        .and_then(|tail| tail.split("fn hostile_group_authority_row").next())
        .expect("hostile-group materializer");
    assert!(!materializer.contains("site.id.value"));
}

#[test]
fn generated_combat_eligibility_fails_closed_across_site_group_and_finale_authority() {
    use adventuresim_core::quest_generation::TemplateFamily;

    let generated = generated_case(7, TemplateFamily::RecurringDepredation);
    let (hostile_group_id, hostile_site_id, _, _) = generated
        .hostile_groups
        .first()
        .expect("recurring case has hostile authority");
    let generated_site = generated
        .sites
        .iter()
        .find(|site| site.id == *hostile_site_id)
        .expect("hostile site exists");
    let site = CaseSiteAuthority {
        id_key: generated_site.id.0.clone(),
        id: CaseSiteId::from(generated_site.id.0.clone()),
        case_id: generated.canonical_case_id.clone(),
        origin_settlement_id: "test-settlement".into(),
        name: generated_site.safe_label.clone(),
        description: "known generated site".into(),
        scene_key: "forest".into(),
        longitude_e7: 0,
        latitude_e7: 0,
        coordinates_are_geographic: false,
        distance_m: 1,
    };
    let case = CaseAuthority {
        id: generated.canonical_case_id.clone(),
        investigation_case_id: generated.canonical_case_id.clone(),
        provenance_kind: "generated".into(),
        generated_case_id: generated.canonical_case_id.clone(),
        local_problem_id: Some(generated.problem_id.clone()),
        objective_expression_json: serde_json::to_string(&generated.objectives).unwrap(),
        resolution_status: CaseResolutionStatus::Open,
        resolved_by_party_id: None,
    };
    let group = HostileGroupAuthority {
        id: hostile_group_id.clone(),
        case_site_id_key: site.id.value.clone(),
        case_site_id: site.id.clone(),
        enemy_type: "test-hostile".into(),
        base_enemy_count: 1,
        base_difficulty: 1,
        baseline_enemy_power: adventuresim_core::threat_escalation::BASELINE_ORC_POWER,
        enemy_count: 1,
        difficulty: 1,
        escalation_incident_ordinal: 1,
        escalation_progress_bps: 0,
        combat_scale_bps: adventuresim_core::threat_escalation::COMBAT_SCALE_BPS,
        normalized_combat_power: adventuresim_core::threat_escalation::BASELINE_ORC_POWER,
        drop_item_id: None,
        drop_quantity: 0,
        disposition: HostileGroupDisposition::Active,
    };
    let finales: Vec<_> = generated
        .objectives
        .alternatives
        .iter()
        .enumerate()
        .map(|(path_index, _)| CaseFinaleAuthority {
            id: format!("finale:{}:{path_index}", generated.canonical_case_id),
            case_id: generated.canonical_case_id.clone(),
            kind: FinaleKind::RecordResolution,
            resolution_status: CaseResolutionStatus::Resolved,
            eligible_path_index: Some(path_index as u16),
            priority: 100,
            status: FinaleStatus::Available,
        })
        .collect();
    let facts = Vec::new();
    assert_eq!(
        generated_case_site_combat_eligible(
            &generated,
            &case,
            &site,
            std::slice::from_ref(&group),
            &finales,
            &facts,
            "party",
        )
        .map(|eligible| eligible.id.as_str()),
        Some(hostile_group_id.as_str()),
    );
    assert_eq!(
        generated_case_site_hostile_resolution_eligible(
            &generated,
            &case,
            &site,
            std::slice::from_ref(&group),
            &finales,
            &facts,
            "party",
            Some(HostileResolutionKind::DrivenOff),
        )
        .map(|eligible| eligible.id.as_str()),
        Some(hostile_group_id.as_str()),
        "generated pre-combat resolution must not depend on a bound mission capability",
    );
    assert!(
        generated_case_site_combat_eligible(
            &generated,
            &case,
            &site,
            std::slice::from_ref(&group),
            &[],
            &facts,
            "party",
        )
        .is_none()
    );
    let mut consumed_finales = finales.clone();
    for finale in &mut consumed_finales {
        finale.status = FinaleStatus::Executed;
    }
    assert!(
        generated_case_site_combat_eligible(
            &generated,
            &case,
            &site,
            std::slice::from_ref(&group),
            &consumed_finales,
            &facts,
            "party",
        )
        .is_none()
    );
    let mut wrong_group = group.clone();
    wrong_group.case_site_id = CaseSiteId::from("site:wrong".to_string());
    assert!(
        generated_case_site_combat_eligible(
            &generated,
            &case,
            &site,
            std::slice::from_ref(&wrong_group),
            &finales,
            &facts,
            "party",
        )
        .is_none()
    );
    let mut extra_group = group.clone();
    extra_group.id = "hostile-group:unexpected".into();
    assert!(
        generated_case_site_combat_eligible(
            &generated,
            &case,
            &site,
            &[group.clone(), extra_group],
            &finales,
            &facts,
            "party",
        )
        .is_none()
    );
    let evidence_site = generated
        .sites
        .iter()
        .find(|candidate| candidate.id != *hostile_site_id)
        .expect("recurring case has a non-hostile site");
    let mut noncombat_site = site.clone();
    noncombat_site.id_key = evidence_site.id.0.clone();
    noncombat_site.id = CaseSiteId::from(evidence_site.id.0.clone());
    noncombat_site.name = evidence_site.safe_label.clone();
    assert!(
        generated_case_site_combat_eligible(
            &generated,
            &case,
            &noncombat_site,
            std::slice::from_ref(&group),
            &finales,
            &facts,
            "party",
        )
        .is_none()
    );
    let mut stale_case = case.clone();
    stale_case.objective_expression_json = "[]".into();
    assert!(
        generated_case_site_combat_eligible(
            &generated,
            &stale_case,
            &site,
            std::slice::from_ref(&group),
            &finales,
            &facts,
            "party",
        )
        .is_none()
    );
}

#[test]
fn hostile_group_authority_enforces_one_group_per_case_site() {
    let source = STRATEGIC_SOURCE;
    let authority = source
        .split("#[table(accessor = hostile_group_authority)]")
        .nth(1)
        .and_then(|tail| tail.split("fn materialize_hostile_group").next())
        .expect("hostile-group authority declaration");
    assert!(authority.contains("#[unique]"));
    assert!(authority.contains("pub case_site_id_key: String"));
    assert!(authority.contains("pub case_site_id: CaseSiteId"));
}

#[test]
fn generated_truth_and_replay_authority_have_no_public_subscription_surface() {
    let strategic = STRATEGIC_SOURCE;
    let authority = strategic
        .split("pub struct QuestGenerationAuthority")
        .nth(1)
        .and_then(|tail| tail.split("pub struct Contract").next())
        .unwrap();
    for field in [
        "seed",
        "settlement_id",
        "settlement_name",
        "context_snapshot_json",
        "context_commitment",
        "manifest_json",
        "factor_trace_json",
    ] {
        assert!(authority.contains(field));
    }
    assert!(strategic.contains("#[table(accessor = quest_generation_authority)]"));
    assert!(!strategic.contains("#[table(accessor = quest_generation_authority, public)]"));

    let generated_client = include_str!("../../../../adventuresim-stdb-client/src/mod.rs");
    assert!(!generated_client.contains("quest_generation_authority_table"));
    let web_types = include_str!("../../../../strategic-web/src/spacetimedb/types.rs");
    let contract = web_types
        .split("pub struct ContractPresentation")
        .nth(1)
        .and_then(|tail| tail.split("pub enum ContractPresentationStatus").next())
        .unwrap();
    assert!(contract.contains("opposition_wording"));
    for forbidden in [
        "seed",
        "manifest",
        "factor_trace",
        "enemy_type",
        "enemy_count",
    ] {
        assert!(!contract.contains(forbidden), "{forbidden} leaked");
    }
}

#[test]
fn generated_activity_is_contract_free_and_counted_by_open_case_authority() {
    let source = STRATEGIC_SOURCE.replace('\r', "");
    let generation = source
        .rsplit("fn generate_quest_for_settlement")
        .next()
        .and_then(|tail| tail.split("#[reducer]").next())
        .expect("generated quest materialization");
    assert!(!generation.contains("contract_authority().insert"));
    assert!(!generation.contains("Contract {"));
    let activity = source
        .rsplit("fn ensure_settlement_activity_inner")
        .next()
        .and_then(|tail| tail.split("fn ensure_npc_recruiting_parties").next())
        .expect("settlement activity");
    assert!(activity.contains("active_generated_cases"));
    assert!(activity.contains("quest_generation_authority()"));
    assert!(activity.contains(".settlement_id()"));
    assert!(activity.contains(".filter(&settlement_id.to_string())"));
    assert!(!activity.contains("quest_generation_authority()\n            .iter()"));
    assert!(activity.contains("validated.context.settlement_id != settlement_id"));
    assert!(activity.contains("CaseResolutionStatus::Open"));
    let resolution = source
        .split("pub(crate) fn ingest_case_outcome_fact")
        .nth(1)
        .and_then(|tail| tail.split("fn select_case_finale").next())
        .expect("case resolution");
    assert!(resolution.contains("ensure_settlement_activity_inner"));
}

#[test]
fn settlement_activity_failures_identify_the_settlement_and_stage() {
    assert_eq!(
        settlement_activity_stage_error("viabundus-0", "NPC recruiting parties", "Party not found"),
        "Settlement activity for viabundus-0 failed during NPC recruiting parties: Party not found"
    );

    let source = STRATEGIC_SOURCE;
    let activity = source
        .rsplit("fn ensure_settlement_activity_inner")
        .next()
        .and_then(|tail| tail.split("fn ensure_npc_recruiting_parties").next())
        .expect("settlement activity implementation");
    for (callee, stage) in [
        ("ensure_settlement_population(ctx", "settlement population"),
        ("refresh_clock(ctx)", "official clock refresh"),
        (
            "validate_quest_generation_authority(&authority)",
            "generated activity validation",
        ),
        ("generate_quest_for_settlement(ctx", "quest generation"),
        ("ensure_generated_incidents(ctx", "generated incidents"),
        (
            "ensure_npc_recruiting_parties(ctx",
            "NPC recruiting parties",
        ),
    ] {
        let after_call = activity
            .split(callee)
            .nth(1)
            .unwrap_or_else(|| panic!("settlement activity omits {callee}"));
        let error_path = after_call
            .split("?;")
            .next()
            .expect("fallible settlement activity call");
        assert!(
            error_path.contains(&format!("\"{stage}\"")),
            "{callee} is not paired with its {stage} context"
        );
    }
    assert!(!activity.contains("ensure_npc_case_interventions"));
    assert!(activity.contains("ensure_npc_recruiting_parties"));
}

#[test]
fn world_import_persists_settlement_facts_without_activating_gameplay() {
    let source = STRATEGIC_SOURCE.replace('\r', "");
    let authority = source
        .split("pub struct QuestGenerationAuthority")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) struct ValidatedQuestGenerationAuthority")
                .next()
        })
        .expect("quest generation authority schema");
    assert!(authority.contains("#[index(btree)]\n    pub settlement_id: String"));

    let import = source
        .rsplit("pub fn import_settlements")
        .next()
        .and_then(|tail| tail.split("fn validate_travel_edge_endpoints").next())
        .expect("settlement import reducer");
    for forbidden in [
        "ensure_settlement_activity_inner",
        "ensure_settlement_smith",
        "ensure_settlement_herbalist",
        "ensure_settlement_population",
    ] {
        assert!(
            !import.contains(forbidden),
            "world import must not {forbidden}"
        );
    }

    let activity = source
        .split("fn ensure_settlement_activity_inner")
        .nth(1)
        .and_then(|tail| tail.split("fn ensure_npc_recruiting_parties").next())
        .expect("settlement activity");
    assert!(activity.contains("ensure_settlement_smith"));

    for consumer in [
        "fn generate_quest_for_settlement",
        "pub fn spawn_developer_quest",
    ] {
        let body = source
            .rsplit(consumer)
            .next()
            .expect("quest ordinal consumer");
        assert!(body.contains("validated.context.settlement_id == settlement_id"));
    }
}

#[test]
fn outcome_ingestion_preflights_provenance_before_any_mutation() {
    let source = STRATEGIC_SOURCE;
    let helper = source
        .split("fn validated_case_outcome_provenance")
        .nth(1)
        .and_then(|tail| tail.split("pub(crate) fn ingest_case_outcome_fact").next())
        .unwrap();
    for required in [
        "\"manual\" if case.generated_case_id.is_empty()",
        "authorities.is_empty()",
        "\"generated\" if case.generated_case_id == case.id",
        "authorities.len() != 1",
        "validate_quest_generation_authority",
        "objectives != validated.manifest.objectives",
    ] {
        assert!(helper.contains(required), "{required}");
    }
    let ingestion = source
        .split("pub(crate) fn ingest_case_outcome_fact")
        .nth(1)
        .and_then(|tail| tail.split("fn select_case_finale").next())
        .unwrap();
    let preflight = ingestion.find("validated_case_outcome_provenance").unwrap();
    let objective = ingestion.find("objective_expression_json").unwrap();
    let first_insert = ingestion.find(".insert(").unwrap();
    let first_update = ingestion.find(".update(").unwrap();
    assert!(preflight < objective);
    assert!(objective < first_insert);
    assert!(preflight < first_update);
    assert!(ingestion.contains("if let Some(validated) = generated_provenance"));
}

#[test]
fn generated_noncombat_resolution_closes_problem_without_contract_authority() {
    let source = STRATEGIC_SOURCE;
    let finale = source
        .split("fn execute_case_finale")
        .nth(1)
        .and_then(|tail| tail.split("fn hostile_resolution_for_objective").next())
        .unwrap();
    assert!(finale.contains("case.local_problem_id"));
    assert!(finale.contains("crate::local_problem::apply_outcome("));
    assert!(!finale.contains("contract_authority()"));
    assert!(!finale.contains("active_contract_id"));
}
