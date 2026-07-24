use adventuresim_core::morale::fervor_event_occurs;
use adventuresim_core::prelude::*;
use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, CanopyDensity, CationExchangeCapacity,
    CrossingWatercourse, DominantLeafType, DroughtHistory, DroughtProfile, EdgeEndpoint,
    ElevationMeters, FerryWaterway, FlowingWaterAccess, ForestCover, GeologicEra, GeologicUnitId,
    HabitatSuitability, HistoricalVegetation, IndustryInferenceContext, InferredGeologicSetting,
    InferredIndustryProfile, InferredTreeSpeciesProfile, LandRoute, LandUseFraction,
    LandUseProfile, LanguageCode, MarineWaterAccess, MineralSoil, MineralSoilTexture,
    ModeledTreeSpecies, ModeledTreeSpeciesProfile, OfficialReligion, PalmerDroughtSeverityIndex,
    PotentialVegetation, PotentialVegetationClass, ProductionScale, RouteTerrain,
    SETTLEMENT_ALIAS_NAME_MAX_BYTES, SETTLEMENT_ALIAS_PREFIX_MAX_BYTES,
    SETTLEMENT_DESCRIPTION_MAX_BYTES, SettlementDescriptionKind, SettlementEconomyProfile,
    SettlementHydrology, SettlementImport, SettlementReligiousStatus, SoilAcidity, SoilBasisPoints,
    SoilDepth, SoilEvidence, SoilFertility, SoilProfile, SoilProperties, SoilSubstrate,
    SoilWaterRegime, StoneContentPercent, SurfaceGeology, SurfaceLithology, TopsoilOrganicCarbon,
    TravelEdgeLoad, TravelEdgeProvenance, TravelRoute, TreeSpeciesId, TreeSpeciesProfile,
    UnconsolidatedDeposit, WORLD_SCHEMA_VERSION, Woodland, WorldNodeImport,
    historical_vegetation_matches_context, industry_profile_is_canonical,
    valid_bounded_source_text, valid_sources_markdown,
};
use sha2::{Digest, Sha256};
use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use crate::{
    capability::character_capability,
    character::{
        character, character_attributes, character_equip, character_limbs, character_skills,
        character_stats,
    },
    condition::character_condition,
    investigation::{
        CaseSiteAuthority, CaseSiteId, EvidencePresentationKind, PartyCaseSiteTracking,
        case_site_authority, disclose_exact_case_site, exact_case_site_for_observer,
        investigation_area_authority, investigation_belief, investigation_case_authority,
        investigation_event_authority, investigation_evidence_authority,
        investigation_evidence_knowledge, investigation_lead, investigation_received_testimony,
        investigation_testimony_bundle, mark_case_site_visited, party_case_site_tracking,
    },
    item::{InventoryItem, inventory_item, item},
    local_problem::{local_problem_receipt, local_problem_rumor_delivery},
    repair::item_condition,
    settlement_population::{settlement_npc, settlement_npc_presence},
    tactical::{
        tactical_server_authority, tactical_server_claim, tactical_server_request_authority,
    },
    time::{
        advance_travel_time, character_apprenticeship, character_time, character_training_schedule,
        preview_travel_time, settle_travel_boundary,
    },
};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

const WALKING_SPEED_KM_PER_HOUR: u64 = 5;
const QUEST_TRAVEL_SPEED_DIVISOR: u64 = 4;
const METERS_PER_KILOMETER: u64 = 1_000;
const MINUTES_PER_HOUR: u64 = 60;
const MIN_QUESTS_PER_SETTLEMENT: usize = 3;
const MAX_QUESTS_PER_SETTLEMENT: usize = 5;
const COMPILED_DEV_BOOTSTRAP_TOKEN: Option<&str> = option_env!("ADVENTURESIM_DEV_BOOTSTRAP_TOKEN");
fn parse_threat(enemy_type: &str) -> Result<adventuresim_core::bestiary::ThreatId, String> {
    enemy_type
        .parse()
        .map_err(|_| format!("Unknown threat ID: {enemy_type}"))
}

fn quest_encounter_archetype(
    enemy_type: &str,
) -> Option<adventuresim_core::encounter::EncounterArchetype> {
    use adventuresim_core::{bestiary::ThreatId, encounter::EncounterArchetype};
    match parse_threat(enemy_type).ok()? {
        ThreatId::Goblin | ThreatId::Kobold => Some(EncounterArchetype::Goblins),
        ThreatId::Skeleton | ThreatId::Ghoul | ThreatId::Revenant | ThreatId::Nachzehrer => {
            Some(EncounterArchetype::Undead)
        }
        ThreatId::Bandit
        | ThreatId::Deserter
        | ThreatId::Poacher
        | ThreatId::Smuggler
        | ThreatId::Cultist
        | ThreatId::GraveRobber => Some(EncounterArchetype::Bandits),
        _ => None,
    }
}

fn autoresolve_enemy(id: u64, enemy_type: &str, difficulty: i32) -> Result<Combatant, String> {
    use adventuresim_core::bestiary::{AttackStyle, Protection};
    let rating = (1.2 + difficulty.max(1) as f32 * 0.35).min(4.0);
    let profile = parse_threat(enemy_type)?.profile().combat;
    let mut combatant = Combatant::new(id);
    combatant.attributes = CombatAttributes {
        endurance: rating,
        immunity: rating,
        gut: rating,
        precision: rating + profile.precision_bonus,
        intelligence: rating * 0.7,
        instinct: rating,
        eyesight: rating,
        hearing: rating,
        left_arm_strength: rating,
        right_arm_strength: rating,
        left_leg_strength: rating,
        right_leg_strength: rating,
        left_arm_agility: rating,
        right_arm_agility: rating,
        left_leg_agility: rating,
        right_leg_agility: rating,
    };
    let training = rating * 1_500.0 * profile.training_multiplier;
    combatant.skills = CombatSkills {
        sword_hours: training,
        bow_hours: if profile.ranged { training * 2.0 } else { 0.0 },
        dodge_hours: training,
        block_hours: if matches!(
            profile.protection,
            Protection::Shielded | Protection::Armored
        ) {
            training
        } else {
            training * 0.4
        },
        will_hours: training * (0.5 + f32::from(profile.morale) / 50.0),
        balance_hours: training,
        ..CombatSkills::default()
    };
    combatant.body.weight_kg = profile.weight_kg;
    let (blunt, slash, pierce) = match profile.attack {
        AttackStyle::Blunt => (true, false, false),
        AttackStyle::Blade => (false, true, false),
        AttackStyle::Knife
        | AttackStyle::Spear
        | AttackStyle::Bow
        | AttackStyle::Bite
        | AttackStyle::Claw => (false, false, true),
    };
    let weapon = CombatWeapon {
        skills: if profile.ranged {
            adventuresim_core::equipment::WeaponSkillDistribution {
                bow: 1.0,
                ..Default::default()
            }
        } else {
            adventuresim_core::equipment::WeaponSkillDistribution {
                sword: 1.0,
                ..Default::default()
            }
        },
        melee: !profile.ranged,
        ranged: profile.ranged,
        blunt,
        slash,
        pierce,
        accuracy: 0.8 + profile.precision_bonus,
        weight: if profile.rig == adventuresim_core::bestiary::RigTopology::Quadruped {
            1.0
        } else {
            1.5
        },
        penetration: if matches!(profile.attack, AttackStyle::Spear | AttackStyle::Claw) {
            1.5
        } else {
            0.8
        },
        melee_reach: if profile.ranged { 0.0 } else { 0.8 },
        ranged_range: if profile.ranged { 20.0 } else { 0.0 },
        attack_interval_seconds: if profile.ranged { 1.0 } else { 0.75 },
        precise: profile.precision_bonus > 0.0,
        balance: 0.3,
        ranged_force_joules: if profile.ranged { 40.0 } else { 0.0 },
    };
    combatant.equipment.weapon = Some(weapon);
    if profile.ranged {
        combatant.equipment.ranged_weapon = Some(weapon);
        combatant.equipment.ranged_projectile_kind =
            Some(adventuresim_core::autoresolve::CombatProjectileKind::Arrowhead);
        combatant.equipment.melee_weapon = Some(CombatWeapon {
            melee: true,
            slash: true,
            pierce: true,
            accuracy: 1.0,
            weight: 0.5,
            penetration: 0.5,
            melee_reach: 0.5,
            attack_interval_seconds: 0.6,
            balance: 0.5,
            ..CombatWeapon::default()
        });
        combatant.equipment.ammunition = 12;
        combatant.initial_ammunition = 12;
    } else {
        combatant.equipment.melee_weapon = Some(weapon);
    }
    let innate = profile.innate_protection;
    if innate.resistance_joules > 0.0 || innate.padding_joules > 0.0 {
        combatant.equipment.armor.fill(CombatArmor::innate(
            innate.resistance_joules,
            innate.padding_joules,
        ));
    }
    if matches!(profile.protection, Protection::Armored) {
        combatant.equipment.shield_block_bonus = 1.0;
        combatant.equipment.armor.fill(CombatArmor {
            resistance: 25.0,
            padding: 15.0,
            flexibility: 0.8,
            range_of_motion: 0.9,
            coverage: 0.5,
        });
    }
    Ok(combatant)
}

fn autoresolve_drop(enemy_type: &str) -> Result<Option<&'static str>, String> {
    Ok(parse_threat(enemy_type)?.profile().combat.loot_item_id)
}

fn consume_autoresolve_ammunition(ctx: &ReducerContext, character_id: u64, mut quantity: u32) {
    let stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, "arrow"))
        .collect();
    for mut stack in stacks {
        if quantity == 0 {
            break;
        }
        let consumed = quantity.min(stack.quantity);
        quantity -= consumed;
        stack.quantity -= consumed;
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
    }
}

fn record_autoresolve_report(
    ctx: &ReducerContext,
    battle_id: &str,
    party_id: &str,
    outcome: &BattleOutcome,
) {
    ctx.db
        .autoresolve_report()
        .battle_id()
        .delete(battle_id.to_string());
    let summary = format!(
        "{} rounds; {} stealth successes from {} attempts; {} opening shots; {} ranged attacks; {} melee attacks; {} hits; {:.3} health damage; {} ammunition used",
        outcome.rounds,
        outcome.summary.stealth_successes,
        outcome.summary.stealth_attempts,
        outcome.summary.opening_ranged_attacks,
        outcome.summary.ranged_attacks,
        outcome.summary.melee_attacks,
        outcome.summary.hits,
        outcome.summary.total_health_damage,
        outcome.summary.ammunition_used,
    );
    let log = outcome
        .log
        .iter()
        .map(|entry| {
            format!(
                "#{} {} round {}: {} used {} against {}'s {:?}: {}",
                entry.sequence + 1,
                entry.phase,
                entry.round,
                entry.attacker_id,
                entry.attack_kind,
                entry.defender_id,
                entry.body_part,
                entry.outcome,
            )
        })
        .collect();
    ctx.db.autoresolve_report().insert(AutoresolveReport {
        battle_id: battle_id.to_string(),
        party_id: party_id.to_string(),
        seed: outcome.seed,
        victor: match outcome.victor {
            BattleVictor::Allies => "allies",
            BattleVictor::Enemies => "enemies",
            BattleVictor::Stalemate => "stalemate",
        }
        .to_string(),
        rounds: outcome.rounds as u32,
        summary,
        log,
    });
}

#[cfg(test)]
mod healing_tests {
    use super::{
        CaseSiteAuthority, HostileResolutionKind, IncidentStatus, LocalChatMessage,
        MissionApproachCapability, MissionAttemptStatus, MissionAuthority, MissionOutcomeCandidate,
        RecruitmentOfferStatus, activity_incident_source_id, autoresolve_drop,
        case_refs_have_exact_dialogue_provenance, generated_dialogue_action_matches,
        generated_dialogue_producer_recipient, generated_scene_key,
        generated_witness_visible_description, hostile_group_authority_row,
        hostile_resolution_for_objective, incident_group_matches,
        mission_candidate_from_capability, npc_conversation_authority_matches,
        player_participant_ids, project_local_chat_message, quest_encounter_archetype,
        refreshed_recruitment_offer_status, sample_mission_candidate,
    };
    use adventuresim_core::encounter::EncounterArchetype;
    use std::collections::HashSet;

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
            scene_key: "crypt".into(),
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
    fn persistent_npc_chat_authority_accepts_generated_ids_without_trusting_their_prefix() {
        assert!(npc_conversation_authority_matches(
            "riverdale",
            "riverdale",
            "npc:riverdale:inn:0",
            "npc:riverdale:inn:0",
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
            "npc:riverdale:inn:0",
            "npc:riverdale:inn:0",
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
            "npc:riverdale:inn:0",
            "npc:riverdale:inn:1",
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
            "npc:riverdale:inn:0",
            "npc:riverdale:inn:0",
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
            "npc:riverdale:inn:0",
            "npc:riverdale:inn:0",
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
            npc_id: String::new(),
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
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
    fn unresolved_encounters_guard_party_and_preview_mutations() {
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
        let source = include_str!("strategic.rs");
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
    }

    #[test]
    fn forged_recruitment_mutations_must_cross_character_authority() {
        let source = include_str!("strategic.rs");
        for function in [
            "create_recruitment_role",
            "update_recruitment_role",
            "delete_recruitment_role",
            "save_recruitment_role",
            "rename_saved_recruitment_role",
            "delete_saved_recruitment_role",
            "request_to_join_party",
            "request_general_party_join",
            "accept_party_join_request",
            "reject_party_join_request",
            "update_party_check_targets",
        ] {
            let body = source
                .split(&format!("pub fn {function}"))
                .nth(1)
                .and_then(|tail| tail.split("#[reducer]").next())
                .unwrap_or_else(|| panic!("{function} reducer body"));
            assert!(
                body.contains("require_strategic_character_authority"),
                "{function} trusts a caller-provided character ID"
            );
        }
    }

    #[test]
    fn incident_sources_are_retry_stable_and_group_resolution_is_exact() {
        let first = activity_incident_source_id("raiding", "party", "town", 7, 1440);
        let retry = activity_incident_source_id("raiding", "party", "town", 7, 1440);
        let next = activity_incident_source_id("raiding", "party", "town", 7, 1441);
        assert_eq!(first, retry);
        assert_ne!(first, next);
        assert!(incident_group_matches(
            IncidentStatus::Pending,
            "group:a",
            "group:a"
        ));
        assert!(!incident_group_matches(
            IncidentStatus::Pending,
            "group:a",
            "group:b"
        ));
        assert!(!incident_group_matches(
            IncidentStatus::Resolved,
            "group:a",
            "group:a"
        ));
    }

    #[test]
    fn autoresolve_uses_explicit_mission_and_exactly_once_source_authority() {
        let source = include_str!("strategic.rs");
        let body = source
            .split("pub fn autoresolve_mission")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("autoresolve reducer");
        assert!(body.contains("ensure_bound_mission_authority("));
        assert!(body.contains("complete_bound_mission_success("));
        assert!(body.contains("autoresolve_report()"));
        assert!(!body.contains("record_battle_result("));
    }

    #[test]
    fn mission_binding_entropy_and_terminal_replays_fail_closed() {
        let source = include_str!("strategic.rs");
        let binding = source
            .split("pub(crate) fn ensure_bound_mission_authority")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("mission binding");
        assert!(binding.contains("existing.status == MissionAttemptStatus::Bound"));
        assert!(binding.contains("already terminal and cannot be reused"));
        assert!(binding.contains("outcome_entropy: ctx.random()"));
        assert!(binding.contains("mission_approach_capability_id("));

        let cancel = source
            .split("pub fn cancel_mission_request")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("cancel reducer");
        let inspect = cancel
            .find("mission_authority()")
            .expect("mission inspected first");
        let request = cancel
            .find("tactical_server_request_authority()")
            .expect("request inspected");
        assert!(inspect < request);
        assert!(cancel.contains("MissionAttemptStatus::Cancelled => return Ok(())"));
        assert!(cancel.contains("mission.status = MissionAttemptStatus::Cancelled"));
    }

    #[test]
    fn mission_gateway_reducers_require_character_authority() {
        let source = include_str!("strategic.rs");
        for function in [
            "store_battle_loot",
            "autoresolve_mission",
            "cancel_mission_request",
        ] {
            let body = source
                .split(&format!("pub fn {function}"))
                .nth(1)
                .and_then(|tail| tail.split("#[reducer]").next())
                .expect("reducer body");
            assert!(
                body.contains("require_strategic_character_authority(ctx, character_id)?"),
                "{function} lacks gateway authority"
            );
        }
    }

    #[test]
    fn loot_transfer_rejects_duplicates_and_has_no_unchecked_subtraction() {
        let source = include_str!("strategic.rs");
        let body = source
            .split("pub fn store_battle_loot")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("loot reducer");
        assert!(body.contains("Duplicate battle loot IDs"));
        assert!(body.contains("checked_sub"));
        assert!(!body.contains(".unwrap()"));
    }

    #[test]
    fn tracking_is_presentation_only_and_travel_revalidates_exact_knowledge() {
        let source = include_str!("strategic.rs");
        let tracking = source
            .split("pub fn track_case_site")
            .nth(1)
            .and_then(|tail| tail.split("pub fn abandon_contract").next())
            .expect("tracking reducer");
        assert!(tracking.contains("exact_case_site_for_observer"));
        assert!(tracking.contains("party_case_site_tracking"));
        assert!(!tracking.contains("accept_contract("));
        assert!(!tracking.contains("active_quest_id"));
        assert!(!tracking.contains("gold_reward"));

        let travel = source
            .split("fn travel_to_case_site_impl")
            .nth(1)
            .and_then(|tail| tail.split("pub fn travel_to_settlement").next())
            .expect("case-site travel implementation");
        assert_eq!(travel.matches("exact_case_site_for_observer").count(), 2);
        assert!(travel.contains("\"case_site\""));
        assert!(!travel.contains("ctx.db.quest().id().find(&case_site_id)"));
    }

    #[test]
    fn case_contract_and_tactical_authority_are_separated() {
        let source = include_str!("strategic.rs");
        let accept = source
            .split("pub fn accept_contract")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("accept contract reducer");
        assert!(accept.contains("contract_authority()"));
        assert!(!accept.contains("case_authority().insert"));
        assert!(!accept.contains("case_authority().delete"));
        assert!(!accept.contains("gold_reward.max"));

        let battle = source
            .split("pub(crate) fn commit_hostile_battle_resolution")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("battle commit");
        assert!(battle.contains("ingest_hostile_group_defeat_fact"));
        assert!(!battle.contains("report_contract("));
        assert!(!battle.contains("credit_party_currency("));
    }

    #[test]
    fn hostile_resolution_contract_distinguishes_results_subjects_and_loot() {
        use super::HostileResolutionKind as H;
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::Defeated),
                None,
                H::Defeated,
                None,
                true,
            )
            .is_ok()
        );
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::DrivenOff),
                None,
                H::DrivenOff,
                None,
                false,
            )
            .is_ok()
        );
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::Captured),
                Some("subject"),
                H::Captured,
                Some("subject"),
                false,
            )
            .is_ok()
        );
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::Captured),
                Some("subject"),
                H::Defeated,
                None,
                false,
            )
            .is_err()
        );
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::Captured),
                Some("subject"),
                H::Captured,
                Some("other"),
                false,
            )
            .is_err()
        );
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::DrivenOff),
                None,
                H::DrivenOff,
                None,
                true,
            )
            .is_err()
        );
        assert!(
            super::validate_hostile_resolution_contract(
                Some(H::Captured),
                Some("subject"),
                H::CaptureTargetKilled,
                Some("subject"),
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn reporting_is_ready_only_and_paid_once() {
        let source = include_str!("strategic.rs");
        let report = source
            .split("pub fn report_contract")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("report contract reducer");
        assert!(report.contains("ContractStatus::ReadyToReport"));
        assert!(report.contains("paid_at_minute.is_some()"));
        assert!(report.contains("ContractStatus::Paid"));
        assert!(report.contains("paid_at_minute = Some"));
    }

    #[test]
    fn private_objective_authority_has_only_a_gateway_projection() {
        let source = include_str!("strategic.rs");
        for schema in [
            "case_authority",
            "case_outcome",
            "case_outcome_fact",
            "case_custody",
            "contract_authority",
            "mission_authority",
            "mission_approach_capability",
            "mission_outcome_candidate",
        ] {
            assert!(
                source.contains(&format!("#[table(accessor = {schema})]")),
                "{schema} must remain private"
            );
            assert!(
                !source.contains(&format!("#[table(accessor = {schema}, public)]")),
                "{schema} leaked as public"
            );
        }
        assert!(source.contains("#[view(accessor = backend_contracts, public)]"));
        assert!(source.contains("strategic_view_is_gateway(ctx)"));
    }

    #[test]
    fn case_blocker_authority_paths_remain_reachable_and_private() {
        let source = include_str!("strategic.rs");
        assert!(source.contains("#[table(accessor = backend_case_battle_authority)]"));
        assert!(source.contains("#[view(accessor = backend_case_battles, public)]"));
        assert!(!source.contains("#[table(accessor = backend_case_battle, public)]"));
        assert!(!source.contains("pub fn interact_with_contract_issuer("));
        let simulated_interaction = source
            .split("pub fn simulate_contract_issuer_interaction")
            .nth(1)
            .and_then(|tail| tail.split("fn consume_contract_interaction").next())
            .unwrap();
        assert!(
            simulated_interaction.contains("sender_owns_simulation_character(ctx, character_id)")
        );
        let dialogue_effect = source
            .split("fn apply_dialogue_effect")
            .nth(1)
            .and_then(|tail| tail.split("fn dialogue_service_id").next())
            .unwrap();
        assert!(dialogue_effect.contains("record_dialogue_contract_issuer_interaction"));
        let receipt = source
            .split("pub struct ContractIssuerInteractionReceipt")
            .nth(1)
            .and_then(|tail| tail.split("pub enum IncidentKind").next())
            .unwrap();
        assert!(receipt.contains("pub dialogue_session_id: String"));
        assert!(receipt.contains("pub dialogue_action_id: String"));
        assert!(receipt.contains("pub dialogue_revision: u64"));
        assert!(receipt.contains("pub location_id: String"));

        let accept = source
            .split("pub fn accept_contract")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .unwrap();
        let report = source
            .split("pub fn report_contract")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .unwrap();
        assert!(accept.contains("ContractInteractionStage::Accept"));
        assert!(report.contains("ContractInteractionStage::Report"));
        assert!(report.contains("xp_reward"));
    }

    #[test]
    fn dialogue_objectives_are_knowledge_bound_and_replay_safe() {
        let source = include_str!("strategic.rs");
        let issuer = source
            .split("fn issue_dialogue_investigation_bindings")
            .nth(1)
            .and_then(|tail| tail.split("fn case_has_exact_dialogue_provenance").next())
            .expect("eligibility-time dialogue binding issuer");
        let producer = source
            .split("fn apply_dialogue_investigation_action")
            .nth(1)
            .and_then(|tail| tail.split("fn same_location").next())
            .expect("private dialogue objective producer");
        assert!(issuer.contains("local_problem_rumor_delivery()"));
        assert!(issuer.contains("investigation_received_testimony()"));
        assert!(issuer.contains("exact_case_refs"));
        assert!(issuer.contains("dialogue_investigation_binding()"));
        assert!(producer.contains("has no pre-issued binding"));
        assert!(!producer.contains("case_authority().iter()"));
        assert!(!producer.contains(".insert(DialogueInvestigationBinding"));
        assert!(producer.contains("\"dialogue-objective:{}:{action_id}:{}\""));
        assert!(producer.contains("ingest_case_outcome_fact"));
        assert!(!source.contains("pub fn apply_dialogue_investigation_action"));
    }

    #[test]
    fn unrelated_known_case_is_not_dialogue_provenance() {
        let refs = HashSet::from(["session-case".to_string()]);
        let matching = HashSet::from([
            "canonical-session-case".to_string(),
            "session-case".to_string(),
        ]);
        let unrelated = HashSet::from([
            "known-but-unrelated".to_string(),
            "private-known-but-unrelated".to_string(),
        ]);
        assert!(case_refs_have_exact_dialogue_provenance(&matching, &refs));
        assert!(!case_refs_have_exact_dialogue_provenance(&unrelated, &refs));
    }

    #[test]
    fn physical_proof_and_participant_projection_fail_closed() {
        let source = include_str!("strategic.rs");
        let proof = source
            .split("fn evidence_can_be_presented")
            .nth(1)
            .and_then(|tail| tail.split("fn dialogue_objective_recipient").next())
            .expect("evidence presentation authority");
        assert!(proof.contains("EvidencePresentationKind::Physical"));
        assert!(proof.contains(".is_some_and(|custody|"));
        assert!(!proof.contains(".is_none_or("));
        assert!(proof.contains("EvidencePresentationKind::Informational"));
        assert!(proof.contains("investigation_evidence_knowledge()"));

        let viewers = player_participant_ids([Some(11), None, Some(22)].into_iter());
        assert!(viewers.contains(&11));
        assert!(
            viewers.contains(&22),
            "joined player must receive view rows"
        );
        assert!(!viewers.contains(&33), "outsider must receive no view rows");
    }

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
                requested_family: Some(family),
                witness_candidates: adventuresim_core::quest_generation::test_witnesses(),
            },
        )
        .unwrap()
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
            let present = HashSet::from([producer.recipient_npc_id.clone()]);
            assert_eq!(
                generated_dialogue_producer_recipient(
                    &generated,
                    producer.objective_id.as_str(),
                    &action,
                    &present,
                ),
                Some(producer.recipient_npc_id.clone())
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

        let source = include_str!("strategic.rs");
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
            let group = hostile_group_authority_row(
                hostile_group_id,
                &site,
                threat.as_str().into(),
                *count,
                2,
            )
            .expect("canonical hostile-group row materializes");
            assert_eq!(group.id, *hostile_group_id);
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
                assert_eq!(candidates.len(), 2);
                assert!(candidates.iter().any(|candidate| {
                    candidate.resolution == HostileResolutionKind::Defeated
                        && candidate.weight == 50
                }));
                assert!(candidates.iter().any(|candidate| {
                    candidate.resolution == HostileResolutionKind::DrivenOff
                        && candidate.weight == 30
                }));
            }
        }

        let materializer = include_str!("strategic.rs")
            .split("fn materialize_hostile_group")
            .nth(1)
            .and_then(|tail| tail.split("fn hostile_group_authority_row").next())
            .expect("hostile-group materializer");
        assert!(!materializer.contains("site.id.value"));
    }

    #[test]
    fn generated_truth_and_replay_authority_have_no_public_subscription_surface() {
        let strategic = include_str!("strategic.rs");
        let authority = strategic
            .split("pub struct QuestGenerationAuthority")
            .nth(1)
            .and_then(|tail| tail.split("pub struct Contract").next())
            .unwrap();
        for field in [
            "seed",
            "context_snapshot_json",
            "manifest_json",
            "factor_trace_json",
        ] {
            assert!(authority.contains(field));
        }
        assert!(strategic.contains("#[table(accessor = quest_generation_authority)]"));
        assert!(!strategic.contains("#[table(accessor = quest_generation_authority, public)]"));

        let generated_client = include_str!("../../adventuresim-stdb-client/src/mod.rs");
        assert!(!generated_client.contains("quest_generation_authority_table"));
        let web_types = include_str!("../../strategic-web/src/spacetimedb/types.rs");
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
        let source = include_str!("strategic.rs");
        let generation = source
            .split("fn generate_quest_for_settlement")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("generated quest materialization");
        assert!(!generation.contains("contract_authority().insert"));
        assert!(!generation.contains("Contract {"));
        let activity = source
            .split("fn ensure_settlement_activity_inner")
            .nth(1)
            .and_then(|tail| tail.split("fn ensure_npc_recruiting_parties").next())
            .expect("settlement activity");
        assert!(activity.contains("active_generated_cases"));
        assert!(activity.contains("quest_generation_authority()"));
        assert!(activity.contains("CaseResolutionStatus::Open"));
        let resolution = source
            .split("pub(crate) fn ingest_case_outcome_fact")
            .nth(1)
            .and_then(|tail| tail.split("fn select_case_finale").next())
            .expect("case resolution");
        assert!(resolution.contains("ensure_settlement_activity_inner"));
    }

    #[test]
    fn generated_noncombat_resolution_closes_problem_without_contract_authority() {
        let source = include_str!("strategic.rs");
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
}

/// Returns the living members who participate in strategic party activity.
/// Membership rows for dead characters remain durable, but corpses never
/// advance time, travel, consume provisions, affect readiness, or enter combat.
pub(crate) fn living_party_member_ids(ctx: &ReducerContext, party_id: &str) -> Vec<u64> {
    let mut character_ids: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .filter_map(|membership| {
            ctx.db
                .character()
                .id()
                .find(membership.character_id)
                .filter(|character| character.alive)
                .map(|character| character.id)
        })
        .collect();
    character_ids.sort_unstable();
    character_ids
}

pub(crate) fn require_party_ready(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let character_ids = living_party_member_ids(ctx, party_id);
    if character_ids.is_empty() {
        return Err("Party has no living members".into());
    }
    crate::condition::require_characters_ready(ctx, &character_ids)
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractStatus {
    Offered,
    Accepted,
    ReadyToReport,
    Paid,
    Withdrawn,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseResolutionStatus {
    Open,
    Resolved,
    Failed,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinaleStatus {
    Available,
    Selected,
    Executed,
    Ineligible,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinaleKind {
    RecordResolution,
    ResolveLocalProblem,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyObjectKind {
    Asset,
    Subject,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyHolderKind {
    Site,
    Party,
    Character,
    Npc,
    Destroyed,
    Released,
}

/// Stable gameplay/UI classification derived from the best population data on
/// hand. The world artifact remains source-oriented; this public projection is
/// assigned when a settlement row is materialized.
#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementCategory {
    Unknown,
    Hamlet,
    Village,
    Town,
    City,
    Capital,
}

pub(crate) const fn settlement_category(
    population_estimate: u32,
    population_level: i32,
) -> SettlementCategory {
    if population_estimate > 0 {
        match population_estimate {
            0..=1_999 => SettlementCategory::Hamlet,
            2_000..=3_999 => SettlementCategory::Village,
            4_000..=7_999 => SettlementCategory::Town,
            8_000..=12_999 => SettlementCategory::City,
            _ => SettlementCategory::Capital,
        }
    } else {
        match population_level {
            1 => SettlementCategory::Hamlet,
            2 => SettlementCategory::Village,
            3 => SettlementCategory::Town,
            4 => SettlementCategory::City,
            5 => SettlementCategory::Capital,
            _ => SettlementCategory::Unknown,
        }
    }
}

#[cfg(test)]
mod settlement_category_tests {
    use super::{SettlementCategory, settlement_category};

    #[test]
    fn population_estimate_boundaries_use_regional_bands() {
        let cases = [
            (1, SettlementCategory::Hamlet),
            (1_999, SettlementCategory::Hamlet),
            (2_000, SettlementCategory::Village),
            (3_999, SettlementCategory::Village),
            (4_000, SettlementCategory::Town),
            (7_999, SettlementCategory::Town),
            (8_000, SettlementCategory::City),
            (12_999, SettlementCategory::City),
            (13_000, SettlementCategory::Capital),
        ];
        for (population, expected) in cases {
            assert_eq!(settlement_category(population, -1), expected);
        }
    }

    #[test]
    fn missing_estimates_fall_back_to_levels_and_reject_invalid_levels() {
        for (level, expected) in [
            (1, SettlementCategory::Hamlet),
            (2, SettlementCategory::Village),
            (3, SettlementCategory::Town),
            (4, SettlementCategory::City),
            (5, SettlementCategory::Capital),
        ] {
            assert_eq!(settlement_category(0, level), expected);
        }
        assert_eq!(settlement_category(0, 0), SettlementCategory::Unknown);
        assert_eq!(settlement_category(0, 6), SettlementCategory::Unknown);
    }
}

#[derive(Clone, Debug)]
#[table(accessor = settlement, public)]
pub struct Settlement {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32,
    /// Approximate population in inhabitants; zero means the world data has no estimate.
    pub population_estimate: u32,
    pub category: SettlementCategory,
    pub elevation: ElevationMeters,
    pub land_use: LandUseProfile,
    pub forest_cover: ForestCover,
    pub potential_vegetation: PotentialVegetation,
    pub historical_vegetation: HistoricalVegetation,
    pub tree_species: TreeSpeciesProfile,
    pub soil: SoilProfile,
    pub geology: SurfaceGeology,
    pub religious_status: SettlementReligiousStatus,
    pub languages: adventuresim_world_schema::SettlementLanguageProfile,
    pub drought: DroughtProfile,
    pub hydrology: SettlementHydrology,
    pub industries: InferredIndustryProfile,
    pub economy: SettlementEconomyProfile,
    pub scene_key: String,
    /// The single faith represented by this settlement's church and priest.
    pub religion_id: String,
    /// Stable local denomination assigned from the settlement ID.
    pub currency_id: String,
    /// Viabundus node that supplies this settlement, if it was imported from
    /// the historical world dataset. Demo settlements deliberately leave this
    /// empty.
    pub source_node_id: Option<u64>,
    /// Unstructured Markdown explaining source evidence and deterministic
    /// inferences. Reserved for a future debug view.
    pub sources: String,
}

pub(crate) fn require_settlement_service(
    ctx: &ReducerContext,
    settlement_id: &str,
    service: adventuresim_world_schema::SettlementService,
) -> Result<(), String> {
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_owned())
        .ok_or("Settlement not found")?;
    if settlement.economy.has_service(service) {
        Ok(())
    } else {
        Err("This settlement does not offer that service".into())
    }
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_alias, public)]
pub struct SettlementAlias {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub language: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_description, public)]
pub struct SettlementDescription {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub kind: SettlementDescriptionKind,
    pub language: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct SettlementAliasBatchRow {
    pub id: String,
    pub settlement_id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub language: Option<String>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct SettlementDescriptionBatchRow {
    pub id: String,
    pub settlement_id: String,
    pub kind: SettlementDescriptionKind,
    pub language: Option<String>,
    pub body: String,
}

/// A navigational point in the imported Viabundus network. This contains the
/// topology required for strategic routing, not tactical state or map artwork.
#[derive(Clone, Debug)]
#[table(accessor = world_node, public)]
pub struct WorldNode {
    #[primary_key]
    pub id: u64,
    pub parent_node_id: Option<u64>,
    pub latitude: f64,
    pub longitude: f64,
    pub is_settlement: bool,
    pub is_town: bool,
    pub is_ferry: bool,
    pub is_harbour: bool,
    /// Unstructured Markdown source notes for future debugging.
    pub sources: String,
}

/// An active 1544 land-network segment. Geometry remains an offline map asset;
/// the strategic database needs only endpoint topology and travel metadata.
#[derive(Clone, Debug)]
#[table(accessor = travel_edge, public)]
pub struct TravelEdge {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub from_node_id: u64,
    #[index(btree)]
    pub to_node_id: u64,
    pub route: TravelRoute,
    pub provenance: TravelEdgeProvenance,
    pub toll_at: Option<EdgeEndpoint>,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub terrain: adventuresim_world_schema::RouteTerrain,
    pub certainty: u8,
    pub section: String,
    /// Unstructured Markdown source and inference notes for future debugging.
    pub sources: String,
}

/// The identity that started the one-time local world-data import. All later
/// batches must come from the same identity.
#[derive(Clone, Debug)]
#[table(accessor = world_data_import, public)]
pub struct WorldDataImport {
    #[primary_key]
    pub id: u8,
    pub owner: Identity,
    pub schema_version: u32,
    pub artifact_id: String,
    /// Canonical source/rules/grid manifest digest for audit and cache boundaries.
    pub manifest_digest: String,
    /// Unstructured Markdown describing the source distributions in this
    /// compiled artifact. Per-record inference details live on imported rows.
    pub sources: String,
    pub completed: bool,
}

/// Start a world import. This must be called before sending any import batch.
/// The first caller becomes the owner of this import session; in production the
/// deployment operator must claim it before the database is opened to players.
#[reducer]
pub fn begin_world_data_import(
    ctx: &ReducerContext,
    schema_version: u32,
    artifact_id: String,
    manifest_digest: String,
    sources: String,
) -> Result<(), String> {
    if schema_version != WORLD_SCHEMA_VERSION {
        return Err(format!(
            "World schema version {schema_version} is unsupported; expected {WORLD_SCHEMA_VERSION}"
        ));
    }
    if artifact_id.trim().is_empty() {
        return Err("World artifact ID must not be empty".into());
    }
    if manifest_digest.len() != 64
        || !manifest_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("World manifest digest must be 64 lowercase hexadecimal characters".into());
    }
    if !valid_sources_markdown(&sources) {
        return Err("World source notes are empty, too large, or contain a NUL byte".into());
    }
    match ctx.db.world_data_import().id().find(0) {
        Some(import) if import.owner != ctx.sender() => {
            Err("World data import is owned by another identity".into())
        }
        Some(import)
            if import.schema_version == schema_version
                && import.artifact_id == artifact_id
                && import.manifest_digest == manifest_digest
                && import.sources == sources =>
        {
            if import.completed {
                Err("This world artifact has already been imported".into())
            } else {
                Ok(())
            }
        }
        Some(import) => Err(format!(
            "A different world artifact is already active (schema version {}, artifact {})",
            import.schema_version, import.artifact_id
        )),
        None => {
            ctx.db.world_data_import().insert(WorldDataImport {
                id: 0,
                owner: ctx.sender(),
                schema_version,
                artifact_id,
                manifest_digest,
                sources,
                completed: false,
            });
            Ok(())
        }
    }
}

fn require_active_world_import(ctx: &ReducerContext) -> Result<WorldDataImport, String> {
    let Some(import) = ctx.db.world_data_import().id().find(0) else {
        return Err("Call begin_world_data_import before loading world data".into());
    };
    if import.owner != ctx.sender() {
        return Err("Only the world data import owner may load batches".into());
    }
    if import.completed {
        return Err("The world data import has already completed".into());
    }
    Ok(import)
}

/// Mark a resumable world import complete. Once completed, the session rejects
/// further batches and a different artifact requires an explicit database reset.
#[reducer]
pub fn finish_world_data_import(ctx: &ReducerContext, artifact_id: String) -> Result<(), String> {
    let mut import = require_active_world_import(ctx)?;
    if import.artifact_id != artifact_id {
        return Err("Cannot finish a different world artifact".into());
    }
    validate_final_settlement_industries(ctx)?;
    validate_final_settlement_economies(ctx)?;
    import.completed = true;
    ctx.db.world_data_import().id().update(import);
    Ok(())
}

#[reducer]
pub fn import_world_nodes(ctx: &ReducerContext, nodes: Vec<WorldNodeImport>) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if nodes.is_empty() {
        return Err("World-node batch is empty".into());
    }
    for node in nodes {
        if !valid_sources_markdown(&node.sources) {
            return Err(format!("World node {} has invalid source notes", node.id));
        }
        let row = WorldNode {
            id: node.id,
            parent_node_id: node.parent_node_id,
            latitude: node.latitude,
            longitude: node.longitude,
            is_settlement: node.is_settlement,
            is_town: node.is_town,
            is_ferry: node.is_ferry,
            is_harbour: node.is_harbour,
            sources: node.sources,
        };
        if ctx.db.world_node().id().find(row.id).is_some() {
            ctx.db.world_node().id().update(row);
        } else {
            ctx.db.world_node().insert(row);
        }
    }
    Ok(())
}

#[reducer]
pub fn import_travel_edges(ctx: &ReducerContext, edges: Vec<TravelEdgeLoad>) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if edges.is_empty() {
        return Err("Travel-edge batch is empty".into());
    }
    for edge in edges {
        if edge.provenance == TravelEdgeProvenance::InferredWalkingLink && edge.id >> 63 != 1 {
            return Err(format!(
                "Inferred travel edge {} lacks its stable high-bit identity",
                edge.id
            ));
        }
        validate_travel_edge_endpoints(edge.id, edge.from_node_id, edge.to_node_id)?;
        if ctx.db.world_node().id().find(edge.from_node_id).is_none()
            || ctx.db.world_node().id().find(edge.to_node_id).is_none()
        {
            return Err(format!(
                "Travel edge {} references an unknown world node",
                edge.id
            ));
        }
        validate_travel_route(edge.id, &edge.route)?;
        edge.terrain
            .validate_context(&edge.route, edge.length_m)
            .map_err(|reason| format!("Travel edge {} has invalid terrain: {reason}", edge.id))?;
        if !valid_sources_markdown(&edge.sources) {
            return Err(format!("Travel edge {} has invalid source notes", edge.id));
        }
        let row = TravelEdge {
            id: edge.id,
            from_node_id: edge.from_node_id,
            to_node_id: edge.to_node_id,
            route: edge.route,
            provenance: edge.provenance,
            toll_at: edge.toll,
            length_m: edge.length_m,
            slope_multiplier: edge.slope_multiplier,
            terrain: edge.terrain,
            certainty: edge.certainty,
            section: edge.section,
            sources: edge.sources,
        };
        if ctx.db.travel_edge().id().find(row.id).is_some() {
            ctx.db.travel_edge().id().update(row);
        } else {
            ctx.db.travel_edge().insert(row);
        }
    }
    Ok(())
}

#[reducer]
pub fn import_settlements(
    ctx: &ReducerContext,
    settlements: Vec<SettlementImport>,
) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if settlements.is_empty() {
        return Err("Settlement batch is empty".into());
    }
    for settlement in settlements {
        let elevation = ElevationMeters::new(settlement.elevation.get()).ok_or_else(|| {
            format!(
                "Settlement {} has elevation outside the supported range",
                settlement.id
            )
        })?;
        let land_use = LandUseProfile::new(
            settlement.land_use.cropland(),
            settlement.land_use.grazing(),
            settlement.land_use.built_up(),
            settlement.land_use.natural(),
        )
        .ok_or_else(|| {
            format!(
                "Settlement {} has invalid land-use fractions",
                settlement.id
            )
        })?;
        let forest_cover = match settlement.forest_cover {
            ForestCover::Open => ForestCover::Open,
            ForestCover::Wooded(woodland) => ForestCover::Wooded(Woodland {
                density: CanopyDensity::new(woodland.density.percent()).ok_or_else(|| {
                    format!("Settlement {} has invalid canopy density", settlement.id)
                })?,
                dominant: woodland.dominant,
            }),
        };
        let potential_vegetation = settlement.potential_vegetation;
        let historical_vegetation = settlement.historical_vegetation;
        let tree_species = match settlement.tree_species {
            TreeSpeciesProfile::Modeled(profile) => {
                let candidates = profile
                    .candidates()
                    .iter()
                    .map(|candidate| {
                        Ok(ModeledTreeSpecies {
                            species: TreeSpeciesId::new(candidate.species.as_str().to_owned())
                                .ok_or_else(|| {
                                    format!(
                                        "Settlement {} has an invalid tree species",
                                        settlement.id
                                    )
                                })?,
                            suitability: HabitatSuitability::new(candidate.suitability.score())
                                .ok_or_else(|| {
                                    format!(
                                        "Settlement {} has invalid tree suitability",
                                        settlement.id
                                    )
                                })?,
                            native_range: candidate.native_range,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                TreeSpeciesProfile::Modeled(ModeledTreeSpeciesProfile::new(candidates).ok_or_else(
                    || {
                        format!(
                            "Settlement {} has an invalid modeled tree profile",
                            settlement.id
                        )
                    },
                )?)
            }
            TreeSpeciesProfile::Inferred(profile) => {
                let species = profile
                    .species()
                    .iter()
                    .map(|species| {
                        TreeSpeciesId::new(species.as_str().to_owned()).ok_or_else(|| {
                            format!("Settlement {} has an invalid tree species", settlement.id)
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                TreeSpeciesProfile::Inferred(InferredTreeSpeciesProfile::new(species).ok_or_else(
                    || {
                        format!(
                            "Settlement {} has an invalid inferred tree profile",
                            settlement.id
                        )
                    },
                )?)
            }
        };
        let soil = reconstruct_soil_profile(&settlement.id, settlement.soil)?;
        let geology = reconstruct_geology_profile(&settlement.id, settlement.geology)?;
        let drought = reconstruct_drought_profile(&settlement.id, settlement.drought)?;
        validate_settlement_hydrology(&settlement.id, settlement.hydrology)?;
        settlement.industries.validate().map_err(|reason| {
            format!(
                "Settlement {} has invalid industries: {reason}",
                settlement.id
            )
        })?;
        if !adventuresim_world_schema::coordinates_in_bounds(
            settlement.longitude,
            settlement.latitude,
            adventuresim_world_schema::PLAYABLE_BOUNDS,
        ) || !settlement.languages.is_valid()
            || adventuresim_world_schema::infer_settlement_language_profile(
                settlement.longitude,
                settlement.latitude,
            )
            .ok()
                != Some(settlement.languages)
        {
            return Err(format!(
                "Settlement {} has an invalid language profile",
                settlement.id
            ));
        }
        settlement.economy.validate().map_err(|reason| {
            format!("Settlement {} has invalid economy: {reason}", settlement.id)
        })?;
        // Route batches are resumable and may arrive before or after settlement
        // batches. Exact industry/profile equality is therefore checked against
        // the final edge table by `finish_world_data_import`.
        if !historical_vegetation_matches_context(
            historical_vegetation,
            land_use,
            &potential_vegetation,
            soil,
            settlement.hydrology,
        ) {
            return Err(format!(
                "Settlement {} has historical vegetation inconsistent with its evidence",
                settlement.id
            ));
        }
        if !valid_sources_markdown(&settlement.sources) {
            return Err(format!(
                "Settlement {} has invalid source notes",
                settlement.id
            ));
        }
        if !adventuresim_world_schema::valid_settlement_name(&settlement.name) {
            return Err(format!("Settlement {} has an invalid name", settlement.id));
        }
        if ctx
            .db
            .world_node()
            .id()
            .find(settlement.source_node_id)
            .is_none()
        {
            return Err(format!(
                "Settlement {} references an unknown world node",
                settlement.id
            ));
        }
        let currency_id = crate::item::settlement_currency_id(&settlement.id).to_string();
        let row = Settlement {
            id: settlement.id,
            name: settlement.name,
            coord_x: settlement.longitude,
            coord_y: settlement.latitude,
            population_level: settlement.population_level,
            population_estimate: settlement.population_estimate,
            category: settlement_category(
                settlement.population_estimate,
                settlement.population_level,
            ),
            elevation,
            land_use,
            forest_cover,
            potential_vegetation,
            historical_vegetation,
            tree_species,
            soil,
            geology,
            scene_key: settlement.scene_key,
            religion_id: settlement.religious_status.church().religion_id().into(),
            currency_id,
            religious_status: settlement.religious_status,
            languages: settlement.languages,
            drought,
            hydrology: settlement.hydrology,
            industries: settlement.industries,
            economy: settlement.economy,
            source_node_id: Some(settlement.source_node_id),
            sources: settlement.sources,
        };
        let settlement_id = row.id.clone();
        if ctx.db.settlement().id().find(&row.id).is_some() {
            ctx.db.settlement().id().update(row);
        } else {
            ctx.db.settlement().insert(row);
        }
        ensure_settlement_activity_inner(ctx, &settlement_id)?;
        crate::repair::ensure_settlement_smith(ctx, &settlement_id);
        crate::disease::ensure_settlement_herbalist(ctx, &settlement_id);
    }
    Ok(())
}

fn validate_travel_edge_endpoints(
    edge_id: u64,
    from_node_id: u64,
    to_node_id: u64,
) -> Result<(), String> {
    if from_node_id == to_node_id {
        Err(format!(
            "Travel edge {edge_id} connects a world node to itself"
        ))
    } else {
        Ok(())
    }
}

fn industry_scale_from_incident_routes(
    route_count: usize,
    best_class: Option<adventuresim_world_schema::RouteTerrainClass>,
    max_slope_permille: u16,
) -> ProductionScale {
    if route_count >= 2
        && best_class
            .is_some_and(|class| class <= adventuresim_world_schema::RouteTerrainClass::Rolling)
        && max_slope_permille <= 250
    {
        ProductionScale::Regional
    } else if route_count == 0 {
        ProductionScale::Marginal
    } else {
        ProductionScale::Local
    }
}

fn max_industry_scale_for_node(ctx: &ReducerContext, node_id: u64) -> ProductionScale {
    let mut route_count = 0usize;
    let mut best_class: Option<adventuresim_world_schema::RouteTerrainClass> = None;
    let mut max_slope = 0u16;
    for edge in ctx
        .db
        .travel_edge()
        .iter()
        .filter(|edge| edge.from_node_id == node_id || edge.to_node_id == node_id)
    {
        route_count += 1;
        best_class =
            Some(best_class.map_or(edge.terrain.class, |best| best.min(edge.terrain.class)));
        max_slope = max_slope.max(edge.terrain.max_slope.get());
    }
    industry_scale_from_incident_routes(route_count, best_class, max_slope)
}

fn validate_final_settlement_industries(ctx: &ReducerContext) -> Result<(), String> {
    for settlement in ctx.db.settlement().iter() {
        let Some(source_node_id) = settlement.source_node_id else {
            continue;
        };
        let max_scale = max_industry_scale_for_node(ctx, source_node_id);
        if !industry_profile_is_canonical(
            &settlement.industries,
            IndustryInferenceContext {
                elevation: settlement.elevation,
                drought: settlement.drought,
                land_use: settlement.land_use,
                historical_vegetation: settlement.historical_vegetation,
                soil: settlement.soil,
                geology: &settlement.geology,
                hydrology: settlement.hydrology,
                population_estimate: settlement.population_estimate,
                max_scale,
            },
        ) {
            return Err(format!(
                "Settlement {} industries do not match the final travel-edge graph",
                settlement.id
            ));
        }
    }
    Ok(())
}

fn validate_final_settlement_economies(ctx: &ReducerContext) -> Result<(), String> {
    for settlement in ctx.db.settlement().iter() {
        let Some(node_id) = settlement.source_node_id else {
            continue;
        };
        let routes = ctx
            .db
            .travel_edge()
            .iter()
            .filter(|e| e.from_node_id == node_id || e.to_node_id == node_id)
            .count();
        let documented_town = ctx
            .db
            .world_node()
            .id()
            .find(node_id)
            .is_some_and(|n| n.is_town);
        let expected = adventuresim_world_schema::infer_settlement_economy(
            settlement.population_level,
            settlement.population_estimate,
            u16::try_from(routes).unwrap_or(u16::MAX),
            documented_town,
            &settlement.industries,
        )?;
        if settlement.economy != expected {
            return Err(format!(
                "Settlement {} economy does not match canonical facts and final travel graph",
                settlement.id
            ));
        }
    }
    Ok(())
}

fn validate_travel_route(edge_id: u64, route: &TravelRoute) -> Result<(), String> {
    match route {
        TravelRoute::Land(route) => {
            if route
                .water_crossings
                .windows(2)
                .any(|pair| pair[0].position.get() > pair[1].position.get())
            {
                return Err(format!(
                    "Travel edge {edge_id} has unsorted water crossings"
                ));
            }
            for crossing in &route.water_crossings {
                if adventuresim_world_schema::EdgeProgressPermille::new(crossing.position.get())
                    .is_err()
                    || !valid_crossing_watercourse(crossing.watercourse)
                {
                    return Err(format!(
                        "Travel edge {edge_id} has an invalid water crossing"
                    ));
                }
            }
        }
        TravelRoute::Ferry(route) => {
            if let FerryWaterway::River(river) = route.waterway
                && !valid_river_watercourse(river)
            {
                return Err(format!(
                    "Travel edge {edge_id} has an invalid ferry waterway"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod route_terrain_boundary_tests {
    use super::{industry_scale_from_incident_routes, validate_travel_edge_endpoints};
    use adventuresim_world_schema::{LandRoute, RouteSlopePermille, RouteTerrain, TravelRoute};

    #[test]
    fn strategic_boundary_rejects_raw_out_of_range_terrain_without_panicking() {
        let route = TravelRoute::Land(LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        let mut terrain = RouteTerrain::stage_placeholder();
        // Simulates a raw Spacetime-decoded newtype that bypassed serde and its
        // constructor. Every u16 bit pattern remains valid to read.
        terrain.max_slope = unsafe { std::mem::transmute::<u16, RouteSlopePermille>(10_001) };
        assert!(
            std::panic::catch_unwind(|| terrain.validate_context(&route, 1_000))
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn final_route_scale_catches_late_edges_and_edge_downgrades() {
        use adventuresim_world_schema::{ProductionScale, RouteTerrainClass};

        assert_eq!(
            industry_scale_from_incident_routes(0, None, 0),
            ProductionScale::Marginal,
            "a settlement imported before its edges is initially isolated"
        );
        assert_eq!(
            industry_scale_from_incident_routes(2, Some(RouteTerrainClass::Flat), 250),
            ProductionScale::Regional,
            "late finalized edges can establish connected access"
        );
        assert_eq!(
            industry_scale_from_incident_routes(2, Some(RouteTerrainClass::Flat), 251),
            ProductionScale::Local,
            "updating an incident edge to a steeper route downgrades the final cap"
        );
    }

    #[test]
    fn self_loops_are_rejected_and_cannot_manufacture_connected_access() {
        use adventuresim_world_schema::ProductionScale;

        assert!(validate_travel_edge_endpoints(1, 7, 7).is_err());
        assert!(validate_travel_edge_endpoints(2, 7, 7).is_err());
        assert_eq!(
            industry_scale_from_incident_routes(0, None, 0),
            ProductionScale::Marginal
        );
    }
}

#[reducer]
pub fn import_settlement_aliases(
    ctx: &ReducerContext,
    aliases: Vec<SettlementAliasBatchRow>,
) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if aliases.is_empty() {
        return Err("Settlement-alias batch is empty".into());
    }
    for alias in aliases {
        if alias.id.trim().is_empty() {
            return Err("Settlement alias ID must not be empty".into());
        }
        if ctx
            .db
            .settlement()
            .id()
            .find(&alias.settlement_id)
            .is_none()
        {
            return Err(format!(
                "Settlement alias {} references an unknown settlement",
                alias.id
            ));
        }
        if !valid_bounded_source_text(&alias.name, SETTLEMENT_ALIAS_NAME_MAX_BYTES) {
            return Err(format!(
                "Settlement alias {} name must be trimmed, NUL-free, and at most {} bytes",
                alias.id, SETTLEMENT_ALIAS_NAME_MAX_BYTES
            ));
        }
        if let Some(prefix) = &alias.prefix
            && !valid_bounded_source_text(prefix, SETTLEMENT_ALIAS_PREFIX_MAX_BYTES)
        {
            return Err(format!(
                "Settlement alias {} prefix must be trimmed, NUL-free, and at most {} bytes",
                alias.id, SETTLEMENT_ALIAS_PREFIX_MAX_BYTES
            ));
        }
        let language = alias
            .language
            .map(|value| {
                value
                    .parse::<LanguageCode>()
                    .map(String::from)
                    .map_err(|error| format!("Settlement alias {}: {error}", alias.id))
            })
            .transpose()?;
        let row = SettlementAlias {
            id: alias.id,
            settlement_id: alias.settlement_id,
            name: alias.name,
            prefix: alias.prefix,
            language,
        };
        if ctx.db.settlement_alias().id().find(&row.id).is_some() {
            ctx.db.settlement_alias().id().update(row);
        } else {
            ctx.db.settlement_alias().insert(row);
        }
    }
    Ok(())
}

fn valid_crossing_watercourse(watercourse: CrossingWatercourse) -> bool {
    match watercourse {
        CrossingWatercourse::River(river) => valid_river_watercourse(river),
        CrossingWatercourse::Canal(_) | CrossingWatercourse::Ditch => true,
    }
}

fn valid_river_watercourse(river: adventuresim_world_schema::RiverWatercourse) -> bool {
    adventuresim_world_schema::StrahlerOrder::new(river.order.get()).is_ok()
}

fn validate_settlement_hydrology(
    settlement_id: &str,
    hydrology: SettlementHydrology,
) -> Result<(), String> {
    let valid_distance = |distance: adventuresim_world_schema::WaterDistanceMeters| {
        adventuresim_world_schema::WaterDistanceMeters::new(distance.get()).is_ok()
    };
    let valid_river = |river: adventuresim_world_schema::RiverAccess| {
        valid_distance(river.distance)
            && adventuresim_world_schema::StrahlerOrder::new(river.order.get()).is_ok()
    };
    let flowing_is_valid = match hydrology.flowing {
        Some(FlowingWaterAccess::River(river)) => valid_river(river),
        Some(FlowingWaterAccess::RiverAndCanal(access)) => {
            valid_river(access.river) && valid_distance(access.canal_distance)
        }
        None => true,
    };
    let inland_is_valid = hydrology
        .inland
        .is_none_or(|access| valid_distance(access.distance));
    let marine_is_valid = hydrology.marine.is_none_or(|access| match access {
        MarineWaterAccess::Tidal(distance) | MarineWaterAccess::OpenCoast(distance) => {
            valid_distance(distance)
        }
    });
    if flowing_is_valid && inland_is_valid && marine_is_valid {
        Ok(())
    } else {
        Err(format!("Settlement {settlement_id} has invalid hydrology"))
    }
}

fn reconstruct_drought_profile(
    settlement_id: &str,
    profile: DroughtProfile,
) -> Result<DroughtProfile, String> {
    let reconstruct = |history: DroughtHistory| {
        let current = PalmerDroughtSeverityIndex::new(history.current_summer().milli_units())
            .ok_or_else(|| format!("Settlement {settlement_id} has invalid current PDSI"))?;
        let mean = PalmerDroughtSeverityIndex::new(history.twenty_year_mean().milli_units())
            .ok_or_else(|| format!("Settlement {settlement_id} has invalid mean PDSI"))?;
        DroughtHistory::new(
            current,
            mean,
            history.drought_summers(),
            history.wet_summers(),
        )
        .ok_or_else(|| format!("Settlement {settlement_id} has invalid drought history counts"))
    };
    match profile {
        DroughtProfile::Reconstructed(history) => {
            reconstruct(history).map(DroughtProfile::Reconstructed)
        }
        DroughtProfile::Inferred(history) => reconstruct(history).map(DroughtProfile::Inferred),
    }
}

fn reconstruct_soil_profile(
    settlement_id: &str,
    profile: SoilProfile,
) -> Result<SoilProfile, String> {
    let reconstruct_properties = |mut properties: SoilProperties| {
        let stones = |value: StoneContentPercent| {
            StoneContentPercent::new(value.percent())
                .ok_or_else(|| format!("Settlement {settlement_id} has invalid soil stone content"))
        };
        properties.substrate = match properties.substrate {
            SoilSubstrate::Mineral(mut soil) => {
                soil.stones = stones(soil.stones)?;
                SoilSubstrate::Mineral(soil)
            }
            SoilSubstrate::Organic(mut soil) => {
                soil.stones = stones(soil.stones)?;
                SoilSubstrate::Organic(soil)
            }
            SoilSubstrate::RockOutcrop(mut soil) => {
                soil.stones = stones(soil.stones)?;
                SoilSubstrate::RockOutcrop(soil)
            }
            SoilSubstrate::OtherNonTextured(mut soil) => {
                soil.stones = stones(soil.stones)?;
                SoilSubstrate::OtherNonTextured(soil)
            }
        };
        Ok::<_, String>(properties)
    };
    Ok(SoilProfile {
        properties: reconstruct_properties(profile.properties)?,
        ..profile
    })
}

fn reconstruct_geology_profile(
    settlement_id: &str,
    profile: SurfaceGeology,
) -> Result<SurfaceGeology, String> {
    match profile {
        SurfaceGeology::Mapped(mut mapped) => {
            mapped.unit =
                GeologicUnitId::new(mapped.unit.as_str().to_owned()).ok_or_else(|| {
                    format!("Settlement {settlement_id} has an invalid geologic unit identifier")
                })?;
            Ok(SurfaceGeology::Mapped(mapped))
        }
        SurfaceGeology::Inferred(setting) => Ok(SurfaceGeology::Inferred(setting)),
    }
}

#[reducer]
pub fn import_settlement_descriptions(
    ctx: &ReducerContext,
    descriptions: Vec<SettlementDescriptionBatchRow>,
) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if descriptions.is_empty() {
        return Err("Settlement-description batch is empty".into());
    }
    for description in descriptions {
        if description.id.trim().is_empty() {
            return Err("Settlement description ID must not be empty".into());
        }
        if ctx
            .db
            .settlement()
            .id()
            .find(&description.settlement_id)
            .is_none()
        {
            return Err(format!(
                "Settlement description {} references an unknown settlement",
                description.id
            ));
        }
        if !valid_bounded_source_text(&description.body, SETTLEMENT_DESCRIPTION_MAX_BYTES) {
            return Err(format!(
                "Settlement description {} body must be trimmed, NUL-free, and at most {} bytes",
                description.id, SETTLEMENT_DESCRIPTION_MAX_BYTES
            ));
        }
        let language = description
            .language
            .map(|value| {
                value
                    .parse::<LanguageCode>()
                    .map(String::from)
                    .map_err(|error| format!("Settlement description {}: {error}", description.id))
            })
            .transpose()?;
        let row = SettlementDescription {
            id: description.id,
            settlement_id: description.settlement_id,
            kind: description.kind,
            language,
            body: description.body,
        };
        if ctx.db.settlement_description().id().find(&row.id).is_some() {
            ctx.db.settlement_description().id().update(row);
        } else {
            ctx.db.settlement_description().insert(row);
        }
    }
    Ok(())
}

/// Private world-state and objective authority. Presentation and rewards live
/// on a separate contract, and investigation truth remains in
/// `investigation_case_authority`.
#[derive(Clone, Debug)]
#[table(accessor = case_authority)]
pub struct CaseAuthority {
    #[primary_key]
    pub id: String,
    #[unique]
    pub investigation_case_id: String,
    pub local_problem_id: Option<String>,
    pub objective_expression_json: String,
    pub resolution_status: CaseResolutionStatus,
    pub resolved_by_party_id: Option<String>,
}

/// Immutable private replay authority for one generated case. The manifest and
/// factor trace include canonical truth and must never be exposed by a public
/// table or view.
#[derive(Clone, Debug)]
#[table(accessor = quest_generation_authority)]
pub struct QuestGenerationAuthority {
    #[primary_key]
    pub case_id: String,
    pub seed: u64,
    pub catalog_revision: String,
    pub context_snapshot_json: String,
    pub manifest_json: String,
    pub factor_trace_json: String,
}

/// A separately accepted agreement concerning a case. This row is private:
/// the web gateway builds observer-safe disclosures rather than subscribing
/// clients to undiscovered postings or acceptance state.
#[derive(Clone, Debug)]
#[table(accessor = contract_authority)]
pub struct Contract {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub case_id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    #[index(btree)]
    pub settlement_id: String,
    #[index(btree)]
    pub service_id: String,
    pub issuer_npc_id: String,
    pub status: ContractStatus,
    pub accepted_by: Option<String>,
    pub opposition_wording: String,
    pub opposition_count_wording: String,
    pub accepted_at_minute: Option<u64>,
    pub paid_at_minute: Option<u64>,
}

/// Trusted-gateway projection. This is not a direct player subscription; web
/// handlers still select only locally surfaced or party-accepted contracts.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendContract {
    pub id: String,
    pub case_id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    pub settlement_id: String,
    pub service_id: String,
    pub issuer_npc_id: String,
    pub status: ContractStatus,
    pub accepted_by: Option<String>,
    pub opposition_wording: String,
    pub opposition_count_wording: String,
    pub accepted_at_minute: Option<u64>,
    pub paid_at_minute: Option<u64>,
}

#[view(accessor = backend_contracts, public)]
pub fn backend_contracts(ctx: &ViewContext) -> Vec<BackendContract> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .contract_authority()
        .gateway_bucket()
        .filter(0u8)
        .map(|row| BackendContract {
            id: row.id,
            case_id: row.case_id,
            title: row.title,
            description: row.description,
            difficulty: row.difficulty,
            gold_reward: row.gold_reward,
            xp_reward: row.xp_reward,
            settlement_id: row.settlement_id,
            service_id: row.service_id,
            issuer_npc_id: row.issuer_npc_id,
            status: row.status,
            accepted_by: row.accepted_by,
            opposition_wording: row.opposition_wording,
            opposition_count_wording: row.opposition_count_wording,
            accepted_at_minute: row.accepted_at_minute,
            paid_at_minute: row.paid_at_minute,
        })
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = case_outcome)]
pub struct CaseOutcome {
    #[primary_key]
    pub case_id: String,
    pub party_id: String,
    pub status: CaseResolutionStatus,
    pub winning_path_index: Option<u16>,
    pub resolved_at_minute: u64,
    pub selected_finale_id: String,
    pub finale_executed: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = case_outcome_fact)]
pub struct CaseOutcomeFact {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub party_id: String,
    #[unique]
    pub source_id: String,
    pub fact_json: String,
    pub happened_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = case_custody)]
pub struct CaseCustody {
    #[primary_key]
    pub object_id: String,
    #[index(btree)]
    pub case_id: String,
    pub object_kind: CustodyObjectKind,
    pub holder_kind: CustodyHolderKind,
    pub holder_id: String,
    pub version: u32,
    #[unique]
    pub source_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ObjectiveContinuityKind {
    SurviveAtSite,
    ProtectSubject,
}

/// Private continuous-history guard. A deadline is satisfiable only when this
/// row has remained unbroken from `started_at_minute` through the deadline.
#[derive(Clone, Debug)]
#[table(accessor = objective_continuity_guard)]
pub struct ObjectiveContinuityGuard {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    pub case_id: String,
    pub objective_id: String,
    pub kind: ObjectiveContinuityKind,
    pub site_id: String,
    pub subject_id: String,
    pub custody_version: Option<u32>,
    pub started_at_minute: u64,
    pub through_minute: u64,
    pub broken_at_minute: Option<u64>,
    pub completed: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = case_finale_authority)]
pub struct CaseFinaleAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub kind: FinaleKind,
    pub resolution_status: CaseResolutionStatus,
    pub eligible_path_index: Option<u16>,
    pub priority: u16,
    pub status: FinaleStatus,
}

#[derive(Clone, Debug)]
#[table(accessor = case_finale_execution)]
pub struct CaseFinaleExecution {
    #[primary_key]
    pub finale_id: String,
    #[unique]
    pub source_id: String,
    pub case_id: String,
    pub party_id: String,
    pub executed_at_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ContractInteractionStage {
    Accept,
    Report,
}

#[derive(Clone, Debug)]
#[table(accessor = contract_issuer_interaction_receipt)]
pub struct ContractIssuerInteractionReceipt {
    #[primary_key]
    pub id: String,
    pub contract_id: String,
    pub party_id: String,
    pub stage: ContractInteractionStage,
    pub issuer_npc_id: String,
    pub interacting_character_id: u64,
    pub interacted_at_minute: u64,
    pub dialogue_session_id: String,
    pub dialogue_action_id: String,
    pub dialogue_revision: u64,
    pub location_id: String,
    pub consumed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct IncidentId {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct IncidentSourceId {
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum IncidentKind {
    Religious,
    RaidingRetaliation,
    ThieveryDiscovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum IncidentStatus {
    Pending,
    Resolved,
    Avoided,
}

/// Private strategic authority for an interruption. The source is the durable
/// dedupe key; its site and hostile group bind directly to mission authority.
#[derive(Clone, Debug)]
#[table(accessor = strategic_incident)]
pub struct StrategicIncident {
    #[primary_key]
    pub id_key: String,
    pub id: IncidentId,
    #[unique]
    pub source_id: IncidentSourceId,
    #[index(btree)]
    pub party_id: String,
    pub settlement_id: String,
    pub instigator_id: u64,
    pub kind: IncidentKind,
    pub status: IncidentStatus,
    #[unique]
    pub case_site_id: CaseSiteId,
    #[unique]
    pub hostile_group_id: String,
    pub created_at_minute: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct RecruitmentOfferId {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct RecruitmentSourceId {
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum RecruitmentOfferStatus {
    Open,
    Closed,
    Expired,
}

/// Public, social-only projection for a persistent NPC company's recruiting
/// lifecycle. It intentionally contains no investigation or quest identity.
#[derive(Clone, Debug)]
#[table(accessor = recruitment_offer, public)]
pub struct RecruitmentOffer {
    #[primary_key]
    pub id_key: String,
    pub id: RecruitmentOfferId,
    #[unique]
    pub source_id: RecruitmentSourceId,
    #[unique]
    pub recruiting_party_id: String,
    #[index(btree)]
    pub settlement_id: String,
    #[unique]
    pub settlement_npc_id: String,
    pub location_id: String,
    pub leader_id: u64,
    pub status: RecruitmentOfferStatus,
    pub created_at_minute: u64,
    pub expires_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = party_authority)]
pub struct Party {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_case_site_id: Option<CaseSiteId>,
    pub active_contract_id: Option<String>,
    pub is_solo: bool,
    /// The fatigue level at which the first tiring party member makes camp.
    #[default(50u8)]
    pub camp_fatigue_percent: u8,
    /// Leader-selected daily walking budget. The itinerary centers it on noon.
    #[default(480u16)]
    pub walking_minutes_per_day: u16,
    /// False travels in the daylight window centered on noon; true travels in
    /// the night window centered on midnight.
    #[default(false)]
    pub travel_at_night: bool,
    /// Automatic camps clear every living member's carried fatigue. A fixed
    /// duration preserves the leader's deliberate shorter or longer override.
    #[default(CampDurationMode::Auto)]
    pub camp_duration_mode: CampDurationMode,
    #[default(0u16)]
    pub fixed_camp_minutes: u16,
    /// A non-empty destination means the party is currently camped en route.
    #[default(None::<JourneyEndpoint>)]
    pub camp_destination: Option<JourneyEndpoint>,
    #[default(0u64)]
    pub camp_remaining_minutes: u64,
    /// Water currently held in shared party-inventory waterskins.
    #[default(0.0)]
    pub pooled_water_ml: f32,
    #[default(0.0)]
    pub medicine_target: f32,
    #[default(0.0)]
    pub command_target: f32,
    #[default(0.0)]
    pub religion_target: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, SpacetimeType)]
pub enum CampDurationMode {
    #[default]
    Auto,
    Fixed,
}

/// Party movement and case-site occupancy are visible only through the trusted
/// gateway; direct subscribers cannot enumerate secret destinations.
#[view(accessor = party, public)]
pub fn party(ctx: &ViewContext) -> Vec<Party> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, SpacetimeType)]
pub struct JourneyCampInterval {
    pub movement_minute: u64,
    pub elapsed_start_minute: u64,
    pub elapsed_minutes: u64,
    pub average_fatigue_start: f32,
    pub average_fatigue_end: f32,
    pub maximum_fatigue_end: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct JourneySettlementEndpoint {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct JourneyCaseSiteEndpoint {
    pub id: CaseSiteId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub enum JourneyEndpoint {
    Settlement(JourneySettlementEndpoint),
    CaseSite(JourneyCaseSiteEndpoint),
    Camp(String),
}

impl JourneyEndpoint {
    fn settlement_id(&self) -> Option<&str> {
        match self {
            Self::Settlement(endpoint) => Some(&endpoint.id),
            _ => None,
        }
    }

    fn case_site_id(&self) -> Option<&str> {
        match self {
            Self::CaseSite(endpoint) => Some(&endpoint.id.value),
            _ => None,
        }
    }
}

/// The durable strategic record behind the travel tracker. Party location
/// answers where the party is right now; this record retains the journey's
/// original endpoints, completed camp stops, and authoritative forecast.
#[derive(Clone, Debug)]
#[table(accessor = party_journey_authority)]
pub struct PartyJourney {
    #[primary_key]
    pub party_id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub origin: JourneyEndpoint,
    pub destination: JourneyEndpoint,
    pub total_minutes: u64,
    pub completed_minutes: u64,
    /// Cumulative journey minutes for camps the party has actually reached.
    pub camp_stop_minutes: Vec<u64>,
    /// Cumulative future camp estimates, recalculated after each camp rest.
    pub forecast_camp_stop_minutes: Vec<u64>,
    /// A journey keeps the leader's chosen threshold from departure.
    pub fatigue_percent: u8,
    /// Zero identifies a pre elapsed-itinerary row requiring conservative
    /// reconstruction from the party's current absolute time.
    #[default(0u8)]
    pub plan_version: u8,
    /// Additive v2 itinerary coordinates. Legacy minute fields above remain
    /// route-movement coordinates for compatibility.
    #[default(0u64)]
    pub departure_minute: u64,
    #[default(0u64)]
    pub total_elapsed_minutes: u64,
    #[default(0u64)]
    pub completed_elapsed_minutes: u64,
    #[default(480u16)]
    pub walking_minutes_per_day: u16,
    #[default(false)]
    pub travel_at_night: bool,
    #[default(CampDurationMode::Auto)]
    pub camp_duration_mode: CampDurationMode,
    #[default(0u16)]
    pub fixed_camp_minutes: u16,
}

/// Private encounter authority. Public journey and encounter projections never
/// reveal future-roll entropy to clients.
#[derive(Clone, Debug)]
#[table(accessor = party_journey_encounter_authority)]
pub struct PartyJourneyEncounterAuthority {
    #[primary_key]
    pub party_id: String,
    pub seed: u64,
    pub next_roll: u64,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct StrategicEncounterLoss {
    pub owner_kind: String,
    pub owner_id: u64,
    pub inventory_id: u64,
    pub item_id: String,
    pub quantity: u32,
    pub value_each: u32,
}

/// Durable strategic interruption only. Tactical exchanges, positions, HP,
/// and enemies remain transient and are committed only through final outcomes.
#[derive(Clone, Debug)]
#[table(accessor = strategic_encounter, public)]
pub struct StrategicEncounter {
    #[primary_key]
    pub party_id: String,
    pub encounter_id: String,
    pub archetype: String,
    pub enemy_count: u16,
    pub roll_index: u64,
    pub journey_movement_minute: u64,
    pub journey_elapsed_minute: u64,
    pub absolute_minute: u64,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub terrain: String,
    pub party_aware: bool,
    pub enemy_aware: bool,
    pub available_choices: Vec<String>,
    pub status: String,
    pub selected_choice: Option<String>,
    pub selection_explanation: String,
    pub party_speed_m_per_minute: u32,
    pub enemy_speed_m_per_minute: u32,
    pub run_ineligibility: Option<String>,
    pub penalty_minutes: u64,
    pub loss_preview: Vec<StrategicEncounterLoss>,
    pub outcome: Option<String>,
}

/// Typed elapsed-time camp coordinates for the journey tracker. Keeping these
/// in an additive table avoids changing the movement-coordinate legacy rows.
#[derive(Clone, Debug)]
#[table(accessor = party_journey_itinerary, public)]
pub struct PartyJourneyItinerary {
    #[primary_key]
    pub party_id: String,
    pub actual_camp_intervals: Vec<JourneyCampInterval>,
    pub forecast_camp_intervals: Vec<JourneyCampInterval>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum JourneyTerrainKind {
    Road,
    Open,
    SparseWoods,
    DeepWoods,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyRoutePoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub struct JourneyTerrainWeights {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub urban: u16,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyTerrainSpan {
    pub kind: JourneyTerrainKind,
    pub terrain: JourneyTerrainWeights,
    pub training_multiplier_permille: u16,
    pub check_millirank: u16,
    pub start_minute: u64,
    pub duration_minutes: u64,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyRouteLeg {
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyRoutePlan {
    pub package_digest: String,
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
    pub return_route: Option<JourneyRouteLeg>,
}

#[derive(Clone, Debug)]
#[table(accessor = party_journey_route_authority)]
pub struct PartyJourneyRoute {
    #[primary_key]
    pub party_id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub package_digest: String,
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
    pub return_route: Option<JourneyRouteLeg>,
}

fn strategic_view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

/// Gateway-only projection of journey endpoints and progress. Case-site names
/// and identifiers never reside in a globally subscribable table.
#[view(accessor = party_journey, public)]
pub fn party_journey(ctx: &ViewContext) -> Vec<PartyJourney> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_journey_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

/// Gateway-only projection of exact route geometry.
#[view(accessor = party_journey_route, public)]
pub fn party_journey_route(ctx: &ViewContext) -> Vec<PartyJourneyRoute> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_journey_route_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

/// The authenticated strategic-web identity trusted to submit server-planned
/// travel. The singleton also pins the immutable terrain package digest.
#[derive(Clone, Debug)]
#[table(accessor = strategic_gateway_authority, public)]
pub struct StrategicGatewayAuthority {
    #[primary_key]
    pub id: u8,
    pub identity: Identity,
    pub terrain_package_digest: Option<String>,
    pub terrain_schema: u32,
}

fn valid_route_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// First authenticated registration claims the singleton. Subsequent package
/// rotations must be made by the same SpacetimeDB identity.
#[reducer]
pub fn register_strategic_gateway(
    ctx: &ReducerContext,
    terrain_package_digest: Option<String>,
    terrain_schema: u32,
) -> Result<(), String> {
    if ctx.sender() == Identity::ZERO {
        return Err("Strategic gateway registration requires authentication".into());
    }
    if terrain_package_digest
        .as_deref()
        .is_some_and(|digest| !valid_route_digest(digest))
        || (terrain_package_digest.is_some() && terrain_schema != 1)
        || (terrain_package_digest.is_none() && terrain_schema != 0)
    {
        return Err("Strategic gateway terrain package metadata is invalid".into());
    }
    let authority = StrategicGatewayAuthority {
        id: 0,
        identity: ctx.sender(),
        terrain_package_digest,
        terrain_schema,
    };
    if let Some(existing) = ctx.db.strategic_gateway_authority().id().find(0) {
        if existing.identity != ctx.sender() {
            return Err("A different authenticated identity owns the strategic gateway".into());
        }
        ctx.db.strategic_gateway_authority().id().update(authority);
    } else {
        ctx.db.strategic_gateway_authority().insert(authority);
    }
    Ok(())
}

pub(crate) fn require_strategic_gateway(
    ctx: &ReducerContext,
) -> Result<StrategicGatewayAuthority, String> {
    let authority = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .ok_or("Strategic gateway is not registered")?;
    if authority.identity != ctx.sender() {
        return Err("This reducer may only be called by the strategic gateway".into());
    }
    Ok(authority)
}

pub(crate) fn require_strategic_character_authority(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    if require_strategic_gateway(ctx).is_ok()
        || crate::simulation::sender_owns_simulation_character(ctx, character_id)
    {
        Ok(())
    } else {
        Err("Character-mutating strategic reducers may only be called by the strategic gateway or the owner of the target disposable simulation character".into())
    }
}

#[derive(Clone, Debug)]
#[table(accessor = party_member, public)]
pub struct PartyMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub role: Option<String>,
    pub recruitment_role_id: Option<u64>,
}

#[derive(Clone, Debug)]
#[table(accessor = party_inventory_item, public)]
pub struct PartyInventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub item_id: String,
    pub quantity: u32,
}

/// Condition follows a durable item while it is held in the shared party pool.
/// Durable party rows are always individual (`quantity == 1`) and never merge.
#[derive(Clone, Debug)]
#[table(accessor = party_item_condition, public)]
pub struct PartyItemCondition {
    #[primary_key]
    pub party_inventory_item_id: u64,
    pub tier_1: f32,
    pub tier_2: f32,
    pub tier_3: f32,
    pub tier_4: f32,
    pub tier_5: f32,
}

/// Desired retained quantity used by bulk inventory actions. Party targets are
/// owned by the leader character so they survive party disbanding/recreation.
#[derive(Clone, Debug)]
#[table(
    accessor = inventory_quantity_target, public,
    index(accessor = owner_and_scope, btree(columns = [owner_character_id, party_scope])),
)]
pub struct InventoryQuantityTarget {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub party_scope: bool,
    #[index(btree)]
    pub item_id: String,
    pub quantity: u32,
}

#[reducer]
pub fn set_inventory_quantity_target(
    ctx: &ReducerContext,
    character_id: u64,
    party_scope: bool,
    item_id: String,
    quantity: u32,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if ctx.db.item().id().find(&item_id).is_none() {
        return Err("Item not found".into());
    }
    let owner_character_id = if party_scope {
        let party_id = character.party_id.ok_or("Character has no party")?;
        let party = ctx
            .db
            .party_authority()
            .id()
            .find(&party_id)
            .ok_or("Party not found")?;
        if party.leader_id != character_id {
            return Err("Only the party leader can change party quantity targets".into());
        }
        party.leader_id
    } else {
        character_id
    };
    let id = format!(
        "{}:{owner_character_id}:{item_id}",
        if party_scope { "party" } else { "player" }
    );
    let row = InventoryQuantityTarget {
        id: id.clone(),
        owner_character_id,
        party_scope,
        item_id,
        quantity,
    };
    if ctx.db.inventory_quantity_target().id().find(&id).is_some() {
        ctx.db.inventory_quantity_target().id().update(row);
    } else {
        ctx.db.inventory_quantity_target().insert(row);
    }
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor = party_stake, public)]
pub struct PartyStake {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub value: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = party_inventory_state, public)]
pub struct PartyInventoryState {
    #[primary_key]
    pub party_id: String,
    pub reserve_value: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_result, public)]
pub struct BattleResult {
    #[primary_key]
    pub battle_id: String,
    #[index(btree)]
    pub party_id: String,
}

/// Reproducible strategic-combat diagnostics retained whether the party wins
/// or loses. Clients can show `summary` immediately and expand `log` on demand.
#[derive(Clone, Debug)]
#[table(accessor = autoresolve_report, public)]
pub struct AutoresolveReport {
    #[primary_key]
    pub battle_id: String,
    #[index(btree)]
    pub party_id: String,
    pub seed: u64,
    pub victor: String,
    pub rounds: u32,
    pub summary: String,
    pub log: Vec<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_loot_item, public)]
pub struct BattleLootItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub loot_battle_id: String,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_participant, public)]
pub struct BattleParticipant {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub participant_battle_id: String,
    pub character_id: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = backend_case_battle_authority)]
pub struct BackendCaseBattle {
    #[index(btree)]
    pub gateway_bucket: u8,
    pub case_id: String,
    pub party_id: String,
    #[primary_key]
    pub battle_id: String,
    pub mission_id: String,
}

#[view(accessor = backend_case_battles, public)]
pub fn backend_case_battles(ctx: &ViewContext) -> Vec<BackendCaseBattle> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .backend_case_battle_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

/// Persistent strategic identity for a specific combat opportunity. A mission
/// may be unbound (random encounter) or bound to both a known case site and a
/// specific hostile group. Enemy similarity never creates a binding.
#[derive(Clone, Debug)]
#[table(accessor = mission_authority)]
pub struct MissionAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    pub case_site_id: Option<CaseSiteId>,
    pub hostile_group_id: Option<String>,
    pub observer_character_id: u64,
    pub case_id: String,
    pub outcome_entropy: u64,
    pub status: MissionAttemptStatus,
    pub committed_resolution: Option<HostileResolutionKind>,
    pub committed_capture_subject_id: Option<String>,
    pub scene_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum MissionAttemptStatus {
    Bound,
    Committed,
    Failed,
    Cancelled,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, SpacetimeType, serde::Serialize, serde::Deserialize,
)]
pub enum HostileResolutionKind {
    Defeated,
    DrivenOff,
    Captured,
    CaptureTargetKilled,
}

/// Private observer-authorized approach authority. These rows are exact,
/// objective-scoped capabilities, never a broad public "can capture" flag.
#[derive(Clone, Debug)]
#[table(accessor = mission_approach_capability)]
pub struct MissionApproachCapability {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub observer_character_id: u64,
    #[index(btree)]
    pub hostile_group_id: String,
    pub case_id: String,
    pub case_site_id: CaseSiteId,
    pub path_index: u16,
    pub objective_id: String,
    pub resolution: HostileResolutionKind,
    pub weight: u32,
    pub capture_subject_id: Option<String>,
    pub capture_custody_version: Option<u32>,
    pub active: bool,
}

/// Immutable private snapshot of the exact candidates a mission may sample.
/// The tactical child cannot read, select, or amend this manifest.
#[derive(Clone, Debug)]
#[table(accessor = mission_outcome_candidate)]
pub struct MissionOutcomeCandidate {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub mission_id: String,
    pub capability_id: String,
    pub case_id: String,
    pub case_site_id: CaseSiteId,
    pub hostile_group_id: String,
    pub path_index: u16,
    pub objective_id: String,
    pub resolution: HostileResolutionKind,
    pub weight: u32,
    pub capture_subject_id: Option<String>,
    pub capture_custody_version: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HostileGroupDisposition {
    Active,
    Defeated,
    DrivenOff,
    Captured,
}

fn validate_hostile_resolution_contract(
    expected: Option<HostileResolutionKind>,
    expected_capture_subject: Option<&str>,
    resolution: HostileResolutionKind,
    capture_subject: Option<&str>,
    has_loot: bool,
) -> Result<(), &'static str> {
    if resolution == HostileResolutionKind::CaptureTargetKilled {
        return Err("A killed capture target is not a successful tactical resolution");
    }
    if resolution != HostileResolutionKind::Defeated && has_loot {
        return Err("Only defeated hostiles can produce battle loot");
    }
    if let Some(expected) = expected {
        if expected != resolution {
            return Err("Battle result does not match the mission-selected objective");
        }
        if resolution == HostileResolutionKind::Captured
            && expected_capture_subject != capture_subject
        {
            return Err("Captured subject does not match mission authority");
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor = hostile_group_authority)]
pub struct HostileGroupAuthority {
    #[primary_key]
    pub id: String,
    #[unique]
    pub case_site_id: CaseSiteId,
    pub enemy_type: String,
    pub enemy_count: u32,
    pub difficulty: i32,
    pub drop_item_id: Option<String>,
    pub drop_quantity: u32,
    pub disposition: HostileGroupDisposition,
}

fn materialize_hostile_group(
    ctx: &ReducerContext,
    hostile_group_id: &str,
    site: &CaseSiteAuthority,
    enemy_type: String,
    enemy_count: u32,
    difficulty: i32,
) -> Result<HostileGroupAuthority, String> {
    let group =
        hostile_group_authority_row(hostile_group_id, site, enemy_type, enemy_count, difficulty)?;
    if let Some(existing) = ctx.db.hostile_group_authority().id().find(&group.id) {
        return if existing.case_site_id == group.case_site_id
            && existing.enemy_type == group.enemy_type
            && existing.enemy_count == group.enemy_count
            && existing.difficulty == group.difficulty
            && existing.drop_item_id == group.drop_item_id
            && existing.drop_quantity == group.drop_quantity
        {
            Ok(existing)
        } else {
            Err("Hostile-group ID is already bound to different authority".into())
        };
    }
    ctx.db.hostile_group_authority().insert(group.clone());
    Ok(group)
}

fn hostile_group_authority_row(
    hostile_group_id: &str,
    site: &CaseSiteAuthority,
    enemy_type: String,
    enemy_count: u32,
    difficulty: i32,
) -> Result<HostileGroupAuthority, String> {
    if hostile_group_id.is_empty() {
        return Err("Hostile-group authority requires a canonical ID".into());
    }
    Ok(HostileGroupAuthority {
        id: hostile_group_id.to_string(),
        case_site_id: site.id.clone(),
        drop_item_id: autoresolve_drop(&enemy_type)?.map(str::to_string),
        drop_quantity: enemy_count,
        enemy_type,
        enemy_count,
        difficulty,
        disposition: HostileGroupDisposition::Active,
    })
}

/// Idempotency and attribution receipt for a persistent victorious outcome.
/// Its primary key is supplied by the trusted battle producer.
#[derive(Clone, Debug)]
#[table(accessor = outcome_source_authority)]
pub struct OutcomeSourceAuthority {
    #[primary_key]
    pub id: String,
    #[unique]
    pub battle_id: String,
    pub mission_id: Option<String>,
    pub hostile_group_id: Option<String>,
    pub resolution: HostileResolutionKind,
    pub party_id: String,
}

#[derive(SpacetimeType, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecruitmentRequirements {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: u8,
    pub endurance: u8,
    pub medicine: u8,
    pub surgery: u8,
    pub command: u8,
    pub religion: u8,
}

impl From<RecruitmentRequirements> for adventuresim_core::capability::RoleRequirements {
    fn from(value: RecruitmentRequirements) -> Self {
        Self {
            melee: value.melee,
            ranged: value.ranged,
            weapon_precision: adventuresim_core::capability::legacy_weapon_precision(
                value.precise,
                value.blunt,
                value.slash,
                value.pierce,
            ),
            heavy: value.heavy,
            quarter_armor: value.quarter_armor,
            half_armor: value.half_armor,
            three_quarter_armor: value.three_quarter_armor,
            full_armor: value.full_armor,
            athletics: value.athletics,
            endurance: value.endurance,
            medicine: value.medicine,
            surgery: value.surgery,
            command: value.command,
            religion: value.religion,
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = party_recruitment_role, public)]
pub struct PartyRecruitmentRole {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub quantity: u32,
    #[default(0.0)]
    pub weapon_precision: f32,
}

#[derive(Clone, Debug)]
#[table(accessor = saved_recruitment_role, public)]
pub struct SavedRecruitmentRole {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner_character_id: u64,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    #[default(0.0)]
    pub weapon_precision: f32,
}

#[derive(Clone, Debug)]
#[table(accessor = party_join_request, public)]
pub struct PartyJoinRequest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub recruitment_role_id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub meets_requirements: bool,
}

/// A party member's proposed use of authority normally reserved for the leader.
/// `payload` is JSON so approval can replay the original typed reducer call.
#[derive(Clone, Debug)]
#[table(accessor = party_action_request_authority)]
pub struct PartyActionRequest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub requester_id: u64,
    pub action_kind: String,
    pub summary: String,
    pub payload: String,
}

/// Gateway-only projection: proposed case-site travel contains observer-secret
/// identifiers and must never be visible to ordinary subscribers.
#[view(accessor = party_action_request, public)]
pub fn party_action_request(ctx: &ViewContext) -> Vec<PartyActionRequest> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_action_request_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = resolved_party_action)]
struct ResolvedPartyAction {
    #[primary_key]
    id: u64,
    party_id: String,
    approved_by: u64,
}

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ApprovedPartyAction {
    TravelToSettlement {
        settlement_id: String,
    },
    TravelToCaseSite {
        case_site_id: String,
    },
    RemovePartyMember {
        character_id: u64,
    },
    CreateRecruitmentRole {
        name: String,
        quantity: u32,
        requirements: RecruitmentRequirements,
        weapon_precision: f32,
        save_role: bool,
    },
    UpdateRecruitmentRole {
        role_id: u64,
        name: String,
        quantity: u32,
        requirements: RecruitmentRequirements,
        weapon_precision: f32,
    },
    DeleteRecruitmentRole {
        role_id: u64,
    },
    AcceptJoinRequest {
        request_id: u64,
    },
    RejectJoinRequest {
        request_id: u64,
    },
    AcceptContract {
        contract_id: String,
    },
    AbandonContract {
        contract_id: String,
    },
    ReportContract {
        contract_id: String,
    },
    AutoresolveMission {
        mission_id: String,
    },
    UpdatePartyCheckTargets {
        medicine: f32,
        command: f32,
        religion: f32,
    },
    SetInventoryQuantityTarget {
        item_id: String,
        quantity: u32,
    },
    DisbandParty {
        party_id: String,
    },
    RequestTacticalServer {
        mission_id: String,
        scene_key: String,
    },
    CancelMission {
        mission_id: String,
    },
    PerformInvestigation {
        action_id: String,
        method: String,
        expected_version: u32,
    },
}

impl ApprovedPartyAction {
    fn kind(&self) -> &'static str {
        match self {
            Self::TravelToSettlement { .. } | Self::TravelToCaseSite { .. } => "travel",
            Self::RemovePartyMember { .. } => "kick",
            Self::CreateRecruitmentRole { .. } => "add_role",
            Self::UpdateRecruitmentRole { .. } => "edit_role",
            Self::DeleteRecruitmentRole { .. } => "delete_role",
            Self::AcceptJoinRequest { .. } => "accept_join",
            Self::RejectJoinRequest { .. } => "reject_join",
            Self::AcceptContract { .. } => "accept_contract",
            Self::AbandonContract { .. } => "abandon_contract",
            Self::ReportContract { .. } => "report_contract",
            Self::AutoresolveMission { .. } => "autoresolve",
            Self::UpdatePartyCheckTargets { .. } => "party_checks",
            Self::SetInventoryQuantityTarget { .. } => "party_inventory",
            Self::DisbandParty { .. } => "disband_party",
            Self::RequestTacticalServer { .. } => "initiate_combat",
            Self::CancelMission { .. } => "cancel_mission",
            Self::PerformInvestigation { .. } => "investigate",
        }
    }

    fn execute(
        self,
        ctx: &ReducerContext,
        leader_id: u64,
        requester_id: u64,
    ) -> Result<(), String> {
        match self {
            Self::TravelToSettlement { settlement_id } => {
                travel_to_settlement(ctx, leader_id, settlement_id)
            }
            Self::TravelToCaseSite { case_site_id } => {
                travel_to_case_site(ctx, leader_id, CaseSiteId::from(case_site_id))
            }
            Self::RemovePartyMember { character_id } => {
                remove_party_member(ctx, leader_id, character_id)
            }
            Self::CreateRecruitmentRole {
                name,
                quantity,
                requirements,
                weapon_precision,
                save_role,
            } => create_recruitment_role(
                ctx,
                leader_id,
                name,
                quantity,
                requirements,
                weapon_precision,
                save_role,
            ),
            Self::UpdateRecruitmentRole {
                role_id,
                name,
                quantity,
                requirements,
                weapon_precision,
            } => update_recruitment_role(
                ctx,
                leader_id,
                role_id,
                name,
                quantity,
                requirements,
                weapon_precision,
            ),
            Self::DeleteRecruitmentRole { role_id } => {
                delete_recruitment_role(ctx, leader_id, role_id)
            }
            Self::AcceptJoinRequest { request_id } => {
                accept_party_join_request(ctx, leader_id, request_id)
            }
            Self::RejectJoinRequest { request_id } => {
                reject_party_join_request(ctx, leader_id, request_id)
            }
            Self::AcceptContract { contract_id } => accept_contract(ctx, leader_id, contract_id),
            Self::AbandonContract { contract_id } => abandon_contract(ctx, leader_id, contract_id),
            Self::ReportContract { contract_id } => report_contract(ctx, leader_id, contract_id),
            Self::AutoresolveMission { mission_id } => {
                autoresolve_mission(ctx, leader_id, mission_id)
            }
            Self::UpdatePartyCheckTargets {
                medicine,
                command,
                religion,
            } => update_party_check_targets(ctx, leader_id, medicine, command, religion),
            Self::SetInventoryQuantityTarget { item_id, quantity } => {
                set_inventory_quantity_target(ctx, leader_id, true, item_id, quantity)
            }
            Self::DisbandParty { party_id } => disband_party(ctx, leader_id, party_id),
            Self::RequestTacticalServer {
                mission_id,
                scene_key,
            } => crate::tactical::request_tactical_server(ctx, leader_id, mission_id, scene_key),
            Self::CancelMission { mission_id } => {
                cancel_mission_request(ctx, leader_id, mission_id)
            }
            Self::PerformInvestigation {
                action_id,
                method,
                expected_version,
            } => crate::investigation::perform_investigation_action_authorized(
                ctx,
                requester_id,
                action_id,
                method,
                expected_version,
                true,
            ),
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = party_leader_vote, public)]
pub struct PartyLeaderVote {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub voter_id: u64,
    pub candidate_id: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = local_chat_message)]
pub struct LocalChatMessage {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub audience_party_id: String,
    pub other_party_id: String,
    pub npc_id: String,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendLocalChatMessage {
    pub id: u64,
    pub owner_character_id: u64,
    pub conversation_kind: String,
    pub subject_party_id: String,
    pub subject_npc_id: String,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

fn local_chat_party_viewer_ids(ctx: &ViewContext, party_id: &str) -> Vec<u64> {
    ctx.db
        .party_member()
        .party_id()
        .filter(party_id)
        .map(|member| member.character_id)
        .collect()
}

fn project_local_chat_message(
    row: LocalChatMessage,
    audience_viewers: &[u64],
    other_viewers: &[u64],
) -> Vec<BackendLocalChatMessage> {
    let mut projections = Vec::new();
    let mut push = |owner_character_id, conversation_kind, subject_party_id, subject_npc_id| {
        projections.push(BackendLocalChatMessage {
            id: row.id,
            owner_character_id,
            conversation_kind,
            subject_party_id,
            subject_npc_id,
            sender_id: row.sender_id,
            sender_name: row.sender_name.clone(),
            body: row.body.clone(),
            created_micros: row.created_micros,
        });
    };
    if row.npc_id.is_empty() {
        for &owner_character_id in audience_viewers {
            push(
                owner_character_id,
                "player".into(),
                row.other_party_id.clone(),
                String::new(),
            );
        }
        if row.other_party_id != row.audience_party_id {
            for &owner_character_id in other_viewers {
                push(
                    owner_character_id,
                    "player".into(),
                    row.audience_party_id.clone(),
                    String::new(),
                );
            }
        }
    } else {
        for &owner_character_id in audience_viewers {
            push(
                owner_character_id,
                "npc".into(),
                String::new(),
                row.npc_id.clone(),
            );
        }
    }
    projections
}

#[view(accessor = backend_local_chat_messages, public)]
pub fn backend_local_chat_messages(ctx: &ViewContext) -> Vec<BackendLocalChatMessage> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .local_chat_message()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            let audience_viewers = local_chat_party_viewer_ids(ctx, &row.audience_party_id);
            let other_viewers = if row.npc_id.is_empty() {
                local_chat_party_viewer_ids(ctx, &row.other_party_id)
            } else {
                Vec::new()
            };
            project_local_chat_message(row, &audience_viewers, &other_viewers)
        })
        .collect()
}

/// Scripted dialogue is authoritative and intentionally separate from free-form local chat.
#[derive(Clone, Debug)]
#[table(accessor = dialogue_session)]
pub struct DialogueSession {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub conversation_id: String,
    pub catalog_revision: String,
    pub settlement_id: String,
    pub location_id: String,
    pub owner_character_id: u64,
    pub owner_party_id: String,
    pub state: String,
    pub revision: u64,
    pub created_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_participant)]
pub struct DialogueParticipant {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub role: String,
    pub character_id: Option<u64>,
    #[index(btree)]
    pub actor_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_event)]
pub struct DialogueEvent {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub sequence: u32,
    pub response_id: String,
    pub speaker_role: String,
    pub fragments_json: String,
    pub source_refs_json: String,
    pub created_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_prompt)]
pub struct DialoguePrompt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub prompt_id: String,
    pub mode: String,
    pub respondent_role: String,
    pub resolution_policy: String,
    pub choices_json: String,
    pub min_choices: u32,
    pub max_choices: u32,
    pub state: String,
    pub resolved_choice_ids_json: String,
    pub source_refs_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_action)]
pub struct DialogueAction {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub session_id: String,
    pub action_id: String,
    pub action_kind: String,
    pub resulting_revision: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_answer)]
pub struct DialogueAnswer {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub prompt_row_id: String,
    pub character_id: u64,
    pub choice_ids_json: String,
    pub created_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = character_topic_knowledge)]
pub struct CharacterTopicKnowledge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub conversation_id: String,
    pub topic_id: String,
    pub learned_micros: i64,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_topic_option)]
pub struct DialogueTopicOption {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub session_id: String,
    pub topic_id: String,
    pub label: String,
    pub source_ref_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = dialogue_investigation_binding)]
pub struct DialogueInvestigationBinding {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub session_id: String,
    pub character_id: u64,
    pub party_id: String,
    pub intended_recipient_id: String,
    pub action_family: String,
    pub source_scope: String,
    pub case_id: String,
    pub objective_id: String,
    pub expected_custody_version: Option<u32>,
    pub issued_revision: u64,
    pub consumed_by: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueSession {
    pub id: String,
    pub conversation_id: String,
    pub catalog_revision: String,
    pub settlement_id: String,
    pub location_id: String,
    pub state: String,
    pub revision: u64,
    pub created_micros: i64,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueParticipant {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub character_id: Option<u64>,
    pub actor_id: String,
    pub display_name: String,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueEvent {
    pub id: String,
    pub session_id: String,
    pub sequence: u32,
    pub response_id: String,
    pub speaker_role: String,
    pub fragments_json: String,
    pub source_refs_json: String,
    pub created_micros: i64,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialoguePrompt {
    pub id: String,
    pub session_id: String,
    pub prompt_id: String,
    pub mode: String,
    pub respondent_role: String,
    pub resolution_policy: String,
    pub choices_json: String,
    pub min_choices: u32,
    pub max_choices: u32,
    pub state: String,
    pub resolved_choice_ids_json: String,
    pub source_refs_json: String,
    pub owner_character_id: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDialogueTopicOption {
    pub id: String,
    pub session_id: String,
    pub topic_id: String,
    pub label: String,
    pub source_ref_json: String,
    pub owner_character_id: u64,
}

fn player_participant_ids(participants: impl Iterator<Item = Option<u64>>) -> Vec<u64> {
    participants.flatten().collect()
}

fn dialogue_viewer_ids(ctx: &ViewContext, session_id: &str) -> Vec<u64> {
    player_participant_ids(
        ctx.db
            .dialogue_participant()
            .session_id()
            .filter(session_id)
            .map(|participant| participant.character_id),
    )
}

#[view(accessor = backend_dialogue_sessions, public)]
pub fn backend_dialogue_sessions(ctx: &ViewContext) -> Vec<BackendDialogueSession> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_session()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueSession {
                    id: row.id.clone(),
                    conversation_id: row.conversation_id.clone(),
                    catalog_revision: row.catalog_revision.clone(),
                    settlement_id: row.settlement_id.clone(),
                    location_id: row.location_id.clone(),
                    state: row.state.clone(),
                    revision: row.revision,
                    created_micros: row.created_micros,
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_participants, public)]
pub fn backend_dialogue_participants(ctx: &ViewContext) -> Vec<BackendDialogueParticipant> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_participant()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueParticipant {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    role: row.role.clone(),
                    character_id: row.character_id,
                    actor_id: row.actor_id.clone(),
                    display_name: row.display_name.clone(),
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_events, public)]
pub fn backend_dialogue_events(ctx: &ViewContext) -> Vec<BackendDialogueEvent> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_event()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueEvent {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    sequence: row.sequence,
                    response_id: row.response_id.clone(),
                    speaker_role: row.speaker_role.clone(),
                    fragments_json: row.fragments_json.clone(),
                    source_refs_json: row.source_refs_json.clone(),
                    created_micros: row.created_micros,
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_prompts, public)]
pub fn backend_dialogue_prompts(ctx: &ViewContext) -> Vec<BackendDialoguePrompt> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_prompt()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialoguePrompt {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    prompt_id: row.prompt_id.clone(),
                    mode: row.mode.clone(),
                    respondent_role: row.respondent_role.clone(),
                    resolution_policy: row.resolution_policy.clone(),
                    choices_json: row.choices_json.clone(),
                    min_choices: row.min_choices,
                    max_choices: row.max_choices,
                    state: row.state.clone(),
                    resolved_choice_ids_json: row.resolved_choice_ids_json.clone(),
                    source_refs_json: row.source_refs_json.clone(),
                    owner_character_id,
                })
        })
        .collect()
}

#[view(accessor = backend_dialogue_topic_options, public)]
pub fn backend_dialogue_topic_options(ctx: &ViewContext) -> Vec<BackendDialogueTopicOption> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .dialogue_topic_option()
        .gateway_bucket()
        .filter(0u8)
        .flat_map(|row| {
            dialogue_viewer_ids(ctx, &row.session_id)
                .into_iter()
                .map(move |owner_character_id| BackendDialogueTopicOption {
                    id: row.id.clone(),
                    session_id: row.session_id.clone(),
                    topic_id: row.topic_id.clone(),
                    label: row.label.clone(),
                    source_ref_json: row.source_ref_json.clone(),
                    owner_character_id,
                })
        })
        .collect()
}

fn require_dialogue_revision(revision: &str) -> Result<(), String> {
    if revision == adventuresim_dialogue::CATALOG_DIGEST {
        Ok(())
    } else {
        Err("Dialogue catalog revision is stale".into())
    }
}

/// Revalidates the complete physical authority boundary for every dialogue mutation.
fn require_live_dialogue_presence(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
) -> Result<crate::settlement_population::SettlementNpc, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.party_id.as_deref() != Some(session.owner_party_id.as_str()) {
        return Err("Dialogue joins are limited to the owning party".into());
    }
    if character.current_settlement_id.as_deref() != Some(session.settlement_id.as_str()) {
        return Err("Dialogue participant has left the settlement".into());
    }
    let npc_participants: Vec<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|participant| participant.character_id.is_none())
        .collect();
    if npc_participants.is_empty() {
        return Err("Dialogue has no persistent NPC participant".into());
    }
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(720, |time| time.minutes);
    let mut primary = None;
    for participant in npc_participants {
        let npc = ctx
            .db
            .settlement_npc()
            .id()
            .find(&participant.actor_id)
            .ok_or("Dialogue NPC is no longer authoritative")?;
        let presence = ctx
            .db
            .settlement_npc_presence()
            .npc_id()
            .find(&npc.id)
            .ok_or("Dialogue NPC has no authoritative presence")?;
        if npc.home_settlement_id != session.settlement_id
            || presence.settlement_id != session.settlement_id
            || presence.location_id != session.location_id
            || !crate::settlement_population::npc_is_present(&presence, minute)
        {
            return Err("Dialogue NPC is not present at the session location and time".into());
        }
        primary.get_or_insert(npc);
    }
    Ok(primary.expect("nonempty NPC participants"))
}

#[reducer]
pub fn start_dialogue(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: String,
    conversation_id: String,
    npc_actor_id: String,
    location_id: String,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let owner_party_id = character
        .party_id
        .clone()
        .ok_or("Dialogue requires a party")?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Dialogue requires a settlement")?;
    let npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&npc_actor_id)
        .ok_or("Dialogue actor is not a persistent settlement NPC")?;
    if npc.home_settlement_id != settlement_id {
        return Err("Dialogue actor is not at this settlement".into());
    }
    let presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&npc_actor_id)
        .ok_or("Dialogue actor has no authoritative presence")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(720, |time| time.minutes);
    if presence.settlement_id != settlement_id
        || presence.location_id != location_id
        || !crate::settlement_population::npc_is_present(&presence, minute)
    {
        return Err("Dialogue actor is not present at this time".into());
    }
    if conversation_id != npc.conversation_id {
        return Err("Dialogue conversation is not valid for this NPC".into());
    }
    let conversation = adventuresim_dialogue::find_conversation(&conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    adventuresim_dialogue::validate(adventuresim_dialogue::catalog())
        .map_err(|_| "Dialogue catalog is invalid")?;
    let player_role = conversation
        .roles
        .iter()
        .find(|(_, role)| role.kind == adventuresim_dialogue::ParticipantKind::Player)
        .map(|(name, _)| name.clone())
        .ok_or("Conversation has no player role")?;
    if !conversation
        .roles
        .values()
        .any(|role| role.kind == adventuresim_dialogue::ParticipantKind::Npc)
    {
        return Err("Conversation has no NPC role".into());
    }
    if !session_id.starts_with(&format!("dialogue:{character_id}:"))
        || session_id.len() > 160
        || session_id.chars().any(char::is_control)
    {
        return Err("Invalid dialogue session ID".into());
    }
    if let Some(existing) = ctx.db.dialogue_session().id().find(&session_id) {
        return if existing.conversation_id == conversation_id
            && existing.settlement_id == settlement_id
            && existing.location_id == location_id
            && existing.catalog_revision == catalog_revision
        {
            require_live_dialogue_presence(ctx, &existing, character_id).map(|_| ())
        } else {
            Err("Dialogue session ID conflicts with another request".into())
        };
    }
    let id = session_id;
    ctx.db.dialogue_session().insert(DialogueSession {
        id: id.clone(),
        gateway_bucket: 0,
        conversation_id,
        catalog_revision,
        settlement_id: settlement_id.clone(),
        location_id: location_id.clone(),
        owner_character_id: character_id,
        owner_party_id,
        state: "active".into(),
        revision: 0,
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    ctx.db.dialogue_participant().insert(DialogueParticipant {
        id: format!("{id}:character:{character_id}"),
        gateway_bucket: 0,
        session_id: id.clone(),
        role: player_role,
        character_id: Some(character_id),
        actor_id: format!("character:{character_id}"),
        display_name: character.name.clone(),
    });
    let available_npcs: Vec<_> = ctx
        .db
        .settlement_npc_presence()
        .settlement_id()
        .filter(&settlement_id)
        .filter(|candidate| {
            candidate.location_id == location_id
                && crate::settlement_population::npc_is_present(candidate, minute)
        })
        .filter_map(|presence| ctx.db.settlement_npc().id().find(&presence.npc_id))
        .collect();
    let mut used_npcs = HashSet::new();
    for (index, (role_name, role)) in conversation
        .roles
        .iter()
        .filter(|(_, role)| role.kind == adventuresim_dialogue::ParticipantKind::Npc)
        .enumerate()
    {
        if role.min > 1 {
            return Err("Dialogue currently supports one persistent actor per NPC role".into());
        }
        let bound = if index == 0 {
            npc.clone()
        } else {
            available_npcs
                .iter()
                .find(|candidate| {
                    candidate.id != npc_actor_id && !used_npcs.contains(&candidate.id)
                })
                .cloned()
                .ok_or("Required NPC role has no persistent actor at this location")?
        };
        used_npcs.insert(bound.id.clone());
        ctx.db.dialogue_participant().insert(DialogueParticipant {
            id: format!("{id}:npc:{role_name}"),
            gateway_bucket: 0,
            session_id: id.clone(),
            role: role_name.clone(),
            character_id: None,
            display_name: bound.name,
            actor_id: bound.id,
        });
    }
    let session = ctx
        .db
        .dialogue_session()
        .id()
        .find(&id)
        .ok_or("Dialogue session not found")?;
    validate_dialogue_cardinality(ctx, &session, conversation)?;
    require_live_dialogue_presence(ctx, &session, character_id)?;
    if !conversation.on_start.is_empty() {
        let facts = dialogue_fact_context(ctx, &session, character_id)?;
        let response = adventuresim_dialogue::select_start_response(conversation, &facts)
            .map_err(|_| "No unambiguous eligible conversation greeting")?;
        for (turn_index, turn) in response.turns.iter().enumerate() {
            let source_refs: Vec<_> = turn
                .fragments
                .iter()
                .enumerate()
                .map(|(fragment_index, authored)| {
                    let field = match authored {
                        adventuresim_dialogue::Fragment::Text { .. } => "value",
                        adventuresim_dialogue::Fragment::Topic { .. } => "label",
                        adventuresim_dialogue::Fragment::Runtime { .. } => "slot",
                    };
                    adventuresim_dialogue::source_for_start_fragment(
                        &session.conversation_id,
                        &response.id,
                        turn_index,
                        fragment_index,
                        field,
                    )
                })
                .collect();
            ctx.db.dialogue_event().insert(DialogueEvent {
                id: format!("{}:event:{turn_index}", session.id),
                gateway_bucket: 0,
                session_id: session.id.clone(),
                sequence: turn_index as u32,
                response_id: response.id.clone(),
                speaker_role: turn.speaker.clone(),
                fragments_json: serde_json::to_string(&resolve_dialogue_fragments(
                    ctx,
                    &session,
                    character_id,
                    &turn.speaker,
                    &turn.fragments,
                )?)
                .map_err(|_| "Could not encode dialogue greeting")?,
                source_refs_json: serde_json::to_string(&source_refs)
                    .map_err(|_| "Could not encode dialogue greeting sources")?,
                created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
            });
        }
        for effect in &response.effects {
            let source_scope = format!("start:{}", response.id);
            apply_dialogue_effect(
                ctx,
                character_id,
                &session,
                &source_scope,
                "start",
                session.revision,
                effect,
            )?;
        }
    }
    crate::local_problem::surface_problem(
        ctx,
        character_id,
        &session.id,
        &npc_actor_id,
        &session.location_id,
    )?;
    // Entering a tavern (or the overview fallback) is itself the reliable,
    // markerless discovery action. Persist the observer-safe journal lead
    // immediately; it does not accept a contract or disclose a hidden cause.
    if let Some(delivery) = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|row| row.character_id == character_id)
    {
        let receipt = ctx
            .db
            .local_problem_receipt()
            .id()
            .find(&delivery.receipt_id)
            .ok_or("Rumor delivery receipt disappeared")?;
        crate::investigation::receive_local_problem_rumor(
            ctx,
            character_id,
            receipt.id.clone(),
            format!("receive-rumor:{character_id}:{}", receipt.id),
        )?;
    }
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}

#[reducer]
pub fn join_dialogue_session(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: String,
    role: String,
    action_id: String,
    expected_revision: u64,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    crate::character::require_living_character(ctx, character_id)?;
    validate_dialogue_action_id(&action_id)?;
    let action_row_id = format!("{session_id}:{action_id}");
    let mut session = ctx
        .db
        .dialogue_session()
        .id()
        .find(&session_id)
        .ok_or("Dialogue session not found")?;
    if session.catalog_revision != catalog_revision || session.state != "active" {
        return Err("Dialogue session is stale or closed".into());
    }
    require_live_dialogue_presence(ctx, &session, character_id)?;
    if ctx.db.dialogue_action().id().find(&action_row_id).is_some() {
        return Ok(());
    }
    if session.revision != expected_revision {
        return Err("Dialogue join used a stale session revision".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(session.settlement_id.as_str()) {
        return Err("Dialogue participants must share a location".into());
    }
    let conversation = adventuresim_dialogue::find_conversation(&session.conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    let specification = conversation
        .roles
        .get(&role)
        .filter(|role| role.kind == adventuresim_dialogue::ParticipantKind::Player)
        .ok_or("Unknown player dialogue role")?;
    let count = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session_id)
        .filter(|participant| participant.role == role)
        .count();
    let id = format!("{session_id}:character:{character_id}");
    if ctx.db.dialogue_participant().id().find(&id).is_some() {
        return Ok(());
    }
    if count >= usize::from(specification.max) {
        return Err("Dialogue role is full".into());
    }
    ctx.db.dialogue_participant().insert(DialogueParticipant {
        id,
        gateway_bucket: 0,
        session_id: session_id.clone(),
        role,
        character_id: Some(character_id),
        actor_id: format!("character:{character_id}"),
        display_name: character.name,
    });
    session.revision += 1;
    ctx.db.dialogue_session().id().update(session.clone());
    ctx.db.dialogue_action().insert(DialogueAction {
        id: action_row_id,
        session_id,
        action_id,
        action_kind: "join".into(),
        resulting_revision: session.revision,
    });
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}

fn require_session_member(
    ctx: &ReducerContext,
    session_id: &str,
    character_id: u64,
) -> Result<DialogueSession, String> {
    let session = ctx
        .db
        .dialogue_session()
        .id()
        .find(session_id.to_owned())
        .ok_or("Dialogue session not found")?;
    if session.state != "active" {
        return Err("Dialogue session is closed".into());
    }
    let member = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(session_id)
        .any(|p| p.character_id == Some(character_id));
    if !member {
        return Err("Character is not a dialogue participant".into());
    }
    require_live_dialogue_presence(ctx, &session, character_id)?;
    Ok(session)
}

fn validate_dialogue_action_id(action_id: &str) -> Result<(), String> {
    if action_id.is_empty()
        || action_id.len() > 100
        || action_id.chars().any(|c| c.is_control() || c == ':')
    {
        Err("Invalid dialogue action ID".into())
    } else {
        Ok(())
    }
}

fn validate_dialogue_cardinality(
    ctx: &ReducerContext,
    session: &DialogueSession,
    conversation: &adventuresim_dialogue::Conversation,
) -> Result<(), String> {
    let participants: Vec<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .collect();
    for (role_name, role) in &conversation.roles {
        let count = participants
            .iter()
            .filter(|participant| participant.role == *role_name)
            .count();
        if count < usize::from(role.min) || count > usize::from(role.max) {
            return Err(format!(
                "Dialogue role {role_name} does not meet its cardinality"
            ));
        }
    }
    Ok(())
}

fn dialogue_fact_context(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
) -> Result<adventuresim_dialogue::FactContext, String> {
    use adventuresim_dialogue::{FactKey, FactValue};
    let mut result = adventuresim_dialogue::FactContext::default();
    result.facts.insert(
        FactKey::Location,
        FactValue::Text(session.settlement_id.clone()),
    );
    let participants: Vec<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .collect();
    for participant in &participants {
        result.facts.insert(
            FactKey::ParticipantPresent {
                role: participant.role.clone(),
            },
            FactValue::Bool(true),
        );
        result.facts.insert(
            FactKey::ParticipantCount {
                role: participant.role.clone(),
            },
            FactValue::Integer(
                participants
                    .iter()
                    .filter(|other| other.role == participant.role)
                    .count() as i64,
            ),
        );
        if participant.character_id.is_none() {
            if let Some(npc) = ctx.db.settlement_npc().id().find(&participant.actor_id) {
                if !npc.service_id.is_empty() {
                    result.facts.insert(
                        FactKey::Service {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(npc.service_id.clone()),
                    );
                }
                result.facts.insert(
                    FactKey::ParticipantProfession {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(npc.profession.clone()),
                );
                result.facts.insert(
                    FactKey::ParticipantAgeBand {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(format!("{:?}", npc.age_band).to_lowercase()),
                );
                result.facts.insert(
                    FactKey::ParticipantSex {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(format!("{:?}", npc.sex).to_lowercase()),
                );
                result.facts.insert(
                    FactKey::ParticipantLocalRole {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(npc.local_role.clone()),
                );
                result.facts.insert(
                    FactKey::LocalCircumstance,
                    FactValue::Text(
                        if session.location_id == "residences" {
                            "household errand"
                        } else {
                            npc.local_role.as_str()
                        }
                        .into(),
                    ),
                );
                if let Some(presence) = ctx.db.settlement_npc_presence().npc_id().find(&npc.id) {
                    result
                        .facts
                        .insert(FactKey::LocationRole, FactValue::Text(presence.location_id));
                }
            }
        }
        if let Some(id) = participant.character_id {
            if let Some(character) = ctx.db.character().id().find(id) {
                let age = match character.age_years {
                    0..=12 => "child",
                    13..=17 => "adolescent",
                    60.. => "elder",
                    _ => "adult",
                };
                result.facts.insert(
                    FactKey::ParticipantAgeBand {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(age.into()),
                );
            }
            if let Some(apprenticeship) = ctx
                .db
                .character_apprenticeship()
                .character_id()
                .filter(id)
                .next()
            {
                result.facts.insert(
                    FactKey::ParticipantProfession {
                        role: participant.role.clone(),
                    },
                    FactValue::Text(apprenticeship.service_id),
                );
            }
            if let Some(character) = ctx.db.character().id().find(id) {
                if let Some(party_id) = character.party_id.as_ref() {
                    let leader = ctx
                        .db
                        .party_authority()
                        .id()
                        .find(party_id)
                        .is_some_and(|party| party.leader_id == id);
                    result.facts.insert(
                        FactKey::PartyLeader {
                            role: participant.role.clone(),
                        },
                        FactValue::Bool(leader),
                    );
                    result.facts.insert(
                        FactKey::ParticipantStatus {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(
                            if leader {
                                "party_leader"
                            } else {
                                "party_member"
                            }
                            .into(),
                        ),
                    );
                }
            }
            if let Some(equipment) = ctx.db.character_equip().character_id().find(id) {
                let equipped = [
                    equipment.left_hand_item_id,
                    equipment.right_hand_item_id,
                    equipment.left_arm_armor_id,
                    equipment.right_arm_armor_id,
                    equipment.left_leg_armor_id,
                    equipment.right_leg_armor_id,
                    equipment.head_armor_id,
                    equipment.chest_armor_id,
                    equipment.stomach_armor_id,
                ];
                let clothing = equipped
                    .into_iter()
                    .flatten()
                    .filter_map(|inventory_id| ctx.db.inventory_item().id().find(inventory_id))
                    .filter_map(|inventory| ctx.db.item().id().find(&inventory.item_id))
                    .find(|item| item.kind == crate::item::ItemKind::Clothing);
                if let Some(item) = clothing {
                    result.facts.insert(
                        FactKey::ParticipantClothingCategory {
                            role: participant.role.clone(),
                        },
                        FactValue::Text(item.id),
                    );
                    result.facts.insert(
                        FactKey::ParticipantHasVisibleClothing {
                            role: participant.role.clone(),
                        },
                        FactValue::Bool(true),
                    );
                }
            }
        }
    }
    if let Some(npc) = participants.iter().find(|p| p.character_id.is_none()) {
        let delivery_receipt = ctx
            .db
            .local_problem_rumor_delivery()
            .session_id()
            .filter(&session.id)
            .find(|delivery| delivery.character_id == character_id)
            .and_then(|delivery| {
                ctx.db
                    .local_problem_receipt()
                    .id()
                    .find(&delivery.receipt_id)
            });
        result.facts.insert(
            FactKey::ParticipantRumorCase {
                role: npc.role.clone(),
            },
            FactValue::Bool(delivery_receipt.is_some()),
        );
        result.facts.insert(
            FactKey::ParticipantReferralContact {
                role: npc.role.clone(),
            },
            FactValue::Bool(delivery_receipt.is_some_and(|receipt| {
                adventuresim_core::quest_generation::exact_referral_contact(
                    &receipt.contact_npc_id,
                    &npc.actor_id,
                )
            })),
        );
    }
    if let (Some(player), Some(npc)) = (
        participants
            .iter()
            .find(|p| p.character_id == Some(character_id)),
        participants.iter().find(|p| p.character_id.is_none()),
    ) {
        let prior_sessions: HashSet<_> = ctx
            .db
            .dialogue_participant()
            .actor_id()
            .filter(&npc.actor_id)
            .filter(|other| other.session_id != session.id)
            .map(|other| other.session_id)
            .collect();
        let prior = prior_sessions.iter().any(|prior_session| {
            ctx.db
                .dialogue_participant()
                .session_id()
                .filter(prior_session)
                .any(|other| other.character_id == Some(character_id))
        });
        result.facts.insert(
            FactKey::ParticipantPriorInteraction {
                left: player.role.clone(),
                right: npc.role.clone(),
            },
            FactValue::Bool(prior),
        );
        result.facts.insert(
            FactKey::PriorQuestioning {
                role: npc.role.clone(),
            },
            FactValue::Bool(prior),
        );
        if let (Some(skills), Some(settlement)) = (
            ctx.db.character_skills().character_id().find(character_id),
            ctx.db.settlement().id().find(&session.settlement_id),
        ) {
            let coefficient = skills
                .oral_languages
                .effective(settlement.languages.dominant_german())
                / adventuresim_world_schema::ORAL_FLUENCY_HOURS;
            result.facts.insert(
                FactKey::ParticipantLanguageCompatibility {
                    left: player.role.clone(),
                    right: npc.role.clone(),
                },
                FactValue::Text(
                    if coefficient >= 0.75 {
                        "fluent"
                    } else if coefficient >= 0.35 {
                        "limited"
                    } else {
                        "poor"
                    }
                    .into(),
                ),
            );
            result.facts.insert(
                FactKey::LanguageCheck {
                    left: player.role.clone(),
                    right: npc.role.clone(),
                },
                FactValue::Bool(coefficient >= 0.35),
            );
        }
    }
    let beliefs: Vec<_> = ctx
        .db
        .investigation_belief()
        .owner_character_id()
        .filter(character_id)
        .collect();
    result
        .facts
        .insert(FactKey::KnownClaim, FactValue::Bool(!beliefs.is_empty()));
    result.facts.insert(
        FactKey::Confidence,
        FactValue::Integer(
            beliefs
                .iter()
                .map(|belief| i64::from(belief.confidence_bps))
                .max()
                .unwrap_or(0),
        ),
    );
    result.facts.insert(
        FactKey::KnownLead,
        FactValue::Bool(
            ctx.db
                .investigation_lead()
                .owner_character_id()
                .filter(character_id)
                .next()
                .is_some(),
        ),
    );
    result
        .facts
        .insert(FactKey::SocialCheck, FactValue::Bool(false));
    if let Some(time) = ctx.db.character_time().character_id().find(character_id) {
        let period = match time.minutes % 1440 {
            300..720 => "morning",
            720..1020 => "afternoon",
            1020..1260 => "evening",
            _ => "night",
        };
        result
            .facts
            .insert(FactKey::TimePeriod, FactValue::Text(period.into()));
    }
    let service = dialogue_service_id(ctx, session)?;
    if !service.is_empty() {
        if let Some(contract) = ctx
            .db
            .contract_authority()
            .service_id()
            .filter(&service)
            .find(|contract| contract.settlement_id == session.settlement_id)
        {
            result.facts.insert(
                FactKey::ContractState {
                    contract: "selected-service-contract".into(),
                },
                FactValue::Text(format!("{:?}", contract.status).to_lowercase()),
            );
        }
    }
    Ok(result)
}

fn dialogue_runtime_bindings(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    speaker_role: &str,
) -> Result<adventuresim_dialogue::RuntimeBindings, String> {
    use adventuresim_dialogue::RuntimeSlot as S;
    let participant = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find(|participant| participant.character_id.is_none() && participant.role == speaker_role)
        .ok_or("Dialogue has no NPC speaker")?;
    let npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&participant.actor_id)
        .ok_or("Dialogue NPC is no longer authoritative")?;
    let mut bindings = adventuresim_dialogue::RuntimeBindings::default();
    bindings.bind(S::SpeakerName, npc.name.clone());
    bindings.bind(
        S::SpeakerDescription,
        format!(
            "{}, {}, {}, with {} and {}",
            npc.height, npc.build, npc.complexion, npc.hair, npc.visible_features
        ),
    );
    bindings.bind(S::Settlement, session.settlement_id.clone());
    bindings.bind(S::Location, session.location_id.clone());
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(720, |time| time.minutes);
    bindings.bind(
        S::TimeWindow,
        match minute % 1_440 {
            300..720 => "in the morning",
            720..1_020 => "in the afternoon",
            1_020..1_260 => "in the evening",
            _ => "at night",
        },
    );
    if let Some(lead) = ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(character_id)
        .next()
    {
        bindings.bind(S::Landmark, lead.directions.clone());
        bindings.bind(S::DescribedLocation, lead.directions);
    }
    let beliefs: Vec<_> = ctx
        .db
        .investigation_belief()
        .owner_character_id()
        .filter(character_id)
        .collect();
    if let Some(belief) = beliefs.last() {
        bindings.bind(S::Testimony, belief.statement.clone());
        bindings.bind(S::Claim, belief.statement.clone());
    }
    if let Some(circumstance) = beliefs
        .iter()
        .find(|belief| belief.proposition_id.contains("circumstance"))
    {
        bindings.bind(S::WitnessCircumstance, circumstance.statement.clone());
    }
    if let Some(evidence) = ctx
        .db
        .investigation_evidence_knowledge()
        .owner_character_id()
        .filter(character_id)
        .next()
    {
        bindings.bind(S::Evidence, format!("evidence {}", evidence.evidence_id));
        bindings.bind(S::Proof, format!("proof {}", evidence.evidence_id));
    }
    if let Some(character) = ctx.db.character().id().find(character_id)
        && let Some(contract) = character
            .party_id
            .and_then(|party_id| ctx.db.party_authority().id().find(&party_id))
            .and_then(|party| party.active_contract_id)
            .and_then(|contract_id| ctx.db.contract_authority().id().find(&contract_id))
    {
        bindings.bind(
            S::ContractTerms,
            format!(
                "{} gold and {} experience",
                contract.gold_reward, contract.xp_reward
            ),
        );
    }
    if let Some(delivery) = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|row| row.character_id == character_id)
    {
        let receipt = ctx
            .db
            .local_problem_receipt()
            .id()
            .find(&delivery.receipt_id)
            .ok_or("Rumor delivery has no observer receipt")?;
        let contact = ctx
            .db
            .settlement_npc()
            .id()
            .find(&receipt.contact_npc_id)
            .ok_or("Rumor referral contact is unavailable")?;
        bindings.bind(S::Symptom, receipt.safe_summary.clone());
        bindings.bind(S::Claim, receipt.safe_summary);
        bindings.bind(S::Uncertainty, "an unconfirmed local account");
        bindings.bind(S::ReferralName, contact.name);
        bindings.bind(
            S::ReferralDescription,
            format!(
                "a {} {}, {} build, with {} and {}",
                format!("{:?}", contact.age_band).to_lowercase(),
                format!("{:?}", contact.sex).to_lowercase(),
                contact.build,
                contact.hair,
                contact.visible_features
            ),
        );
        bindings.bind(S::ReferralRole, contact.profession);
        bindings.bind(S::ReferralLocation, receipt.expected_location_id.clone());
        bindings.bind(S::DescribedLocation, receipt.expected_location_id);
        if adventuresim_core::quest_generation::exact_referral_contact(&contact.id, &npc.id) {
            let authority = ctx
                .db
                .quest_generation_authority()
                .case_id()
                .find(&receipt.opaque_case_ref)
                .ok_or("Rumor is not backed by a generated case")?;
            let generated: adventuresim_core::quest_generation::GeneratedCase =
                serde_json::from_str(&authority.manifest_json)
                    .map_err(|_| "Generated testimony manifest is invalid")?;
            let witness = generated
                .witnesses
                .iter()
                .find(|witness| witness.npc_id == npc.id)
                .ok_or("Referral contact is not the generated witness")?;
            let testimony = witness
                .testimony
                .iter()
                .map(|draft| draft.spoken_text.trim())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if testimony.is_empty() || testimony.len() > 4_096 {
                return Err("Generated witness testimony is unavailable or too large".into());
            }
            bindings.bind(S::Testimony, testimony);
        }
    }
    Ok(bindings)
}

fn receive_referred_testimony(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    live_npc: &crate::settlement_population::SettlementNpc,
    action_id: &str,
) -> Result<(), String> {
    let delivery = ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .find(|delivery| delivery.character_id == character_id)
        .ok_or("Dialogue has no exact local-problem referral")?;
    let receipt = ctx
        .db
        .local_problem_receipt()
        .id()
        .find(&delivery.receipt_id)
        .ok_or("Rumor delivery receipt disappeared")?;
    if !adventuresim_core::quest_generation::exact_referral_contact(
        &receipt.contact_npc_id,
        &live_npc.id,
    ) || receipt.settlement_id != session.settlement_id
        || receipt.expected_location_id != session.location_id
    {
        return Err("Addressed NPC is not the exact referred witness here".into());
    }
    let authority = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&receipt.opaque_case_ref)
        .ok_or("Rumor is not backed by a generated case")?;
    let generated: adventuresim_core::quest_generation::GeneratedCase =
        serde_json::from_str(&authority.manifest_json)
            .map_err(|_| "Generated testimony manifest is invalid")?;
    let witness = generated
        .witnesses
        .iter()
        .find(|witness| witness.npc_id == live_npc.id)
        .ok_or("Referral contact is not the generated witness")?;
    crate::investigation::persist_generated_testimony(
        ctx,
        character_id,
        &generated,
        witness,
        action_id,
    )
}

fn resolve_dialogue_fragments(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
    speaker_role: &str,
    fragments: &[adventuresim_dialogue::Fragment],
) -> Result<Vec<adventuresim_dialogue::Fragment>, String> {
    if fragments
        .iter()
        .any(|fragment| matches!(fragment, adventuresim_dialogue::Fragment::Runtime { .. }))
    {
        dialogue_runtime_bindings(ctx, session, character_id, speaker_role)?
            .resolve(fragments)
            .map_err(|_| "Dialogue runtime binding is incomplete or unsafe".into())
    } else {
        Ok(fragments.to_vec())
    }
}

fn dialogue_binding_id(
    session_id: &str,
    character_id: u64,
    source_scope: &str,
    action: &adventuresim_dialogue::InvestigationAction,
    revision: u64,
) -> String {
    format!("{session_id}:{character_id}:{source_scope}:{action:?}:{revision}")
}

fn observer_case_refs(ctx: &ReducerContext, case: &CaseAuthority) -> HashSet<String> {
    let mut refs = HashSet::from([case.id.clone(), case.investigation_case_id.clone()]);
    if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(&case.id)
        && let Ok(manifest) = serde_json::from_str::<
            adventuresim_core::quest_generation::GeneratedCase,
        >(&authority.manifest_json)
    {
        refs.insert(manifest.public_case_id);
    }
    refs
}

fn generated_dialogue_recipient(
    ctx: &ReducerContext,
    case: &CaseAuthority,
    objective_id: &str,
    action: &adventuresim_dialogue::InvestigationAction,
    npc_ids: &HashSet<String>,
    fallback: String,
) -> Result<Option<String>, String> {
    let Some(authority) = ctx.db.quest_generation_authority().case_id().find(&case.id) else {
        return Ok(Some(fallback));
    };
    let manifest: adventuresim_core::quest_generation::GeneratedCase =
        serde_json::from_str(&authority.manifest_json)
            .map_err(|_| "Generated quest manifest is invalid")?;
    Ok(generated_dialogue_producer_recipient(
        &manifest,
        objective_id,
        action,
        npc_ids,
    ))
}

fn generated_dialogue_producer_recipient(
    manifest: &adventuresim_core::quest_generation::GeneratedCase,
    objective_id: &str,
    action: &adventuresim_dialogue::InvestigationAction,
    npc_ids: &HashSet<String>,
) -> Option<String> {
    manifest
        .dialogue_producers
        .iter()
        .find(|producer| {
            producer.objective_id.as_str() == objective_id
                && generated_dialogue_action_matches(producer.action, action)
        })
        .filter(|producer| npc_ids.contains(&producer.recipient_npc_id))
        .map(|producer| producer.recipient_npc_id.clone())
}

fn generated_dialogue_action_matches(
    generated: adventuresim_core::quest_generation::GeneratedDialogueAction,
    action: &adventuresim_dialogue::InvestigationAction,
) -> bool {
    matches!(
        (generated, action),
        (
            adventuresim_core::quest_generation::GeneratedDialogueAction::Expose,
            adventuresim_dialogue::InvestigationAction::Expose
        ) | (
            adventuresim_core::quest_generation::GeneratedDialogueAction::ReturnAsset,
            adventuresim_dialogue::InvestigationAction::ReturnAsset
        )
    )
}

fn evidence_can_be_presented(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    case: &CaseAuthority,
    evidence_id: &str,
) -> bool {
    let observer_case_refs = observer_case_refs(ctx, case);
    let Some(evidence) = ctx
        .db
        .investigation_evidence_authority()
        .id()
        .find(&evidence_id.to_string())
    else {
        return false;
    };
    if !observer_case_refs.contains(&evidence.case_id) {
        return false;
    }
    match evidence.presentation_kind {
        EvidencePresentationKind::Physical => ctx
            .db
            .case_custody()
            .object_id()
            .find(&evidence_id.to_string())
            .is_some_and(|custody| {
                custody.case_id == case.id
                    && ((custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id)
                        || (custody.holder_kind == CustodyHolderKind::Character
                            && custody.holder_id == character_id.to_string()))
            }),
        EvidencePresentationKind::Informational => ctx
            .db
            .investigation_evidence_knowledge()
            .owner_character_id()
            .filter(character_id)
            .any(|knowledge| {
                observer_case_refs.contains(&knowledge.case_id)
                    && knowledge.evidence_id == evidence_id
            }),
    }
}

fn dialogue_objective_recipient(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    case: &CaseAuthority,
    requirement: &adventuresim_core::case::ObjectiveRequirement,
    action: &adventuresim_dialogue::InvestigationAction,
    npc_ids: &HashSet<String>,
    fallback_recipient_id: &str,
    active_contract: Option<&Contract>,
) -> Option<String> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    use adventuresim_dialogue::InvestigationAction as A;
    let observer_case_refs = observer_case_refs(ctx, case);

    match (requirement, action) {
        (R::Locate { subject_ref }, A::Locate) => ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(character_id)
            .any(|lead| {
                observer_case_refs.contains(&lead.case_id)
                    && matches!(
                        lead.destination_stage.as_str(),
                        "exact_believed" | "visited"
                    )
                    && lead.exact_location_id == subject_ref.as_str()
                    && ctx
                        .db
                        .case_site_authority()
                        .id_key()
                        .find(&lead.exact_location_id)
                        .is_some_and(|site| site.case_id == case.id)
            })
            .then(|| fallback_recipient_id.into()),
        (R::Identify { subject_ref }, A::Identify) | (R::Expose { subject_ref }, A::Expose) => ctx
            .db
            .investigation_belief()
            .owner_character_id()
            .filter(character_id)
            .any(|belief| {
                observer_case_refs.contains(&belief.case_id)
                    && belief.proposition_id == subject_ref.as_str()
            })
            .then(|| fallback_recipient_id.into()),
        (R::Negotiate { subject_ref }, A::Negotiate) => (npc_ids.contains(subject_ref.as_str())
            && ctx
                .db
                .investigation_belief()
                .owner_character_id()
                .filter(character_id)
                .any(|belief| {
                    observer_case_refs.contains(&belief.case_id)
                        && belief.proposition_id == subject_ref.as_str()
                }))
        .then(|| subject_ref.as_str().into()),
        (
            R::PresentProof {
                evidence_id,
                recipient_id,
            },
            A::PresentProof,
        ) => (npc_ids.contains(recipient_id.as_str())
            && evidence_can_be_presented(ctx, character_id, party_id, case, evidence_id.as_str()))
        .then(|| recipient_id.as_str().into()),
        (
            R::PresentTestimony {
                witness_id,
                recipient_id,
            },
            A::PresentTestimony,
        ) => (npc_ids.contains(witness_id.as_str())
            && npc_ids.contains(recipient_id.as_str())
            && ctx
                .db
                .investigation_received_testimony()
                .owner_character_id()
                .filter(character_id)
                .any(|received| {
                    observer_case_refs.contains(&received.public_case_id)
                        && received.witness_ref == witness_id.as_str()
                }))
        .then(|| recipient_id.as_str().into()),
        (
            R::Return {
                asset_id,
                custodian_id,
            },
            A::ReturnAsset,
        ) => (npc_ids.contains(custodian_id.as_str())
            && ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .is_some_and(|custody| {
                    custody.case_id == case.id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                }))
        .then(|| custodian_id.as_str().into()),
        (R::Release { subject_id }, A::ReleaseSubject) => (npc_ids.contains(subject_id.as_str())
            && ctx
                .db
                .case_custody()
                .object_id()
                .find(&subject_id.as_str().to_string())
                .is_some_and(|custody| {
                    custody.case_id == case.id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                }))
        .then(|| subject_id.as_str().into()),
        (
            R::Exchange {
                asset_id,
                recipient_id,
            },
            A::ExchangeAsset,
        ) => (npc_ids.contains(recipient_id.as_str())
            && ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .is_some_and(|custody| {
                    custody.case_id == case.id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                }))
        .then(|| recipient_id.as_str().into()),
        (R::ReportToIssuer { issuer_id }, A::ReportToIssuer) => (npc_ids
            .contains(issuer_id.as_str())
            && active_contract.is_some_and(|contract| {
                contract.case_id == case.id && contract.issuer_npc_id == issuer_id.as_str()
            }))
        .then(|| issuer_id.as_str().into()),
        _ => None,
    }
}

fn issue_dialogue_investigation_bindings(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    source_scope: &str,
    issued_revision: u64,
    effects: &[adventuresim_dialogue::Effect],
) -> Result<bool, String> {
    let actions: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            adventuresim_dialogue::Effect::InvestigationAction { action } => Some(action),
            _ => None,
        })
        .collect();
    if actions.is_empty() {
        return Ok(true);
    }
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Ok(false);
    }
    let npc_ids: HashSet<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|participant| participant.character_id.is_none())
        .map(|participant| participant.actor_id)
        .collect();
    let fallback_recipient_id = npc_ids
        .iter()
        .next()
        .ok_or("Dialogue has no NPC participant")?;
    let active_contract = party
        .active_contract_id
        .as_ref()
        .and_then(|id| ctx.db.contract_authority().id().find(id));

    // These are exact, session-relevant observer-safe provenance sources. A
    // character merely knowing some other case never makes this dialogue
    // eligible to advance it.
    let mut exact_case_refs = HashSet::new();
    for delivery in ctx
        .db
        .local_problem_rumor_delivery()
        .session_id()
        .filter(&session.id)
        .filter(|delivery| delivery.character_id == character_id)
    {
        if let Some(receipt) = ctx
            .db
            .local_problem_receipt()
            .id()
            .find(&delivery.receipt_id)
        {
            exact_case_refs.insert(receipt.opaque_case_ref);
        }
    }
    for received in ctx
        .db
        .investigation_received_testimony()
        .owner_character_id()
        .filter(character_id)
        .filter(|received| npc_ids.contains(&received.witness_ref))
    {
        exact_case_refs.insert(received.public_case_id);
    }
    for action in &actions {
        match action {
            adventuresim_dialogue::InvestigationAction::PresentProof
            | adventuresim_dialogue::InvestigationAction::ReturnAsset
            | adventuresim_dialogue::InvestigationAction::ReleaseSubject
            | adventuresim_dialogue::InvestigationAction::ExchangeAsset => {
                exact_case_refs.extend(
                    ctx.db
                        .investigation_evidence_knowledge()
                        .owner_character_id()
                        .filter(character_id)
                        .map(|knowledge| knowledge.case_id),
                );
                exact_case_refs.extend(
                    ctx.db
                        .case_custody()
                        .iter()
                        .filter(|custody| {
                            (custody.holder_kind == CustodyHolderKind::Party
                                && custody.holder_id == party_id)
                                || (custody.holder_kind == CustodyHolderKind::Character
                                    && custody.holder_id == character_id.to_string())
                        })
                        .map(|custody| custody.case_id),
                );
            }
            adventuresim_dialogue::InvestigationAction::Negotiate => {
                exact_case_refs.extend(
                    ctx.db
                        .investigation_belief()
                        .owner_character_id()
                        .filter(character_id)
                        .filter(|belief| npc_ids.contains(&belief.proposition_id))
                        .map(|belief| belief.case_id),
                );
            }
            adventuresim_dialogue::InvestigationAction::Identify
            | adventuresim_dialogue::InvestigationAction::Expose => {
                exact_case_refs.extend(
                    ctx.db
                        .investigation_belief()
                        .owner_character_id()
                        .filter(character_id)
                        .map(|belief| belief.case_id),
                );
            }
            adventuresim_dialogue::InvestigationAction::ReportToIssuer => {
                if let Some(contract) = &active_contract
                    && npc_ids.contains(&contract.issuer_npc_id)
                {
                    exact_case_refs.insert(contract.case_id.clone());
                }
            }
            _ => {}
        }
    }

    let mut pending = Vec::new();
    for action in actions {
        let mut matches = Vec::new();
        for case in ctx
            .db
            .case_authority()
            .iter()
            .filter(|case| case_has_exact_dialogue_provenance(ctx, case, &exact_case_refs))
        {
            if case.resolution_status != CaseResolutionStatus::Open {
                continue;
            }
            let expression: adventuresim_core::case::ObjectiveExpression =
                serde_json::from_str(&case.objective_expression_json)
                    .map_err(|_| "Case objective authority is invalid")?;
            for objective in expression
                .alternatives
                .iter()
                .flat_map(|path| &path.objectives)
            {
                if let Some(recipient_id) = dialogue_objective_recipient(
                    ctx,
                    character_id,
                    &party_id,
                    &case,
                    &objective.requirement,
                    action,
                    &npc_ids,
                    fallback_recipient_id,
                    active_contract.as_ref(),
                ) && let Some(recipient_id) = generated_dialogue_recipient(
                    ctx,
                    &case,
                    objective.id.as_str(),
                    action,
                    &npc_ids,
                    recipient_id,
                )? {
                    let expected_custody_version = match &objective.requirement {
                        adventuresim_core::case::ObjectiveRequirement::Return {
                            asset_id, ..
                        }
                        | adventuresim_core::case::ObjectiveRequirement::Exchange {
                            asset_id,
                            ..
                        } => ctx
                            .db
                            .case_custody()
                            .object_id()
                            .find(&asset_id.as_str().to_string())
                            .map(|row| row.version),
                        adventuresim_core::case::ObjectiveRequirement::Release { subject_id } => {
                            ctx.db
                                .case_custody()
                                .object_id()
                                .find(&subject_id.as_str().to_string())
                                .map(|row| row.version)
                        }
                        _ => None,
                    };
                    matches.push((
                        case.id.clone(),
                        objective.id.as_str().to_string(),
                        recipient_id,
                        expected_custody_version,
                    ));
                }
            }
        }
        if matches.is_empty() {
            return Ok(false);
        }
        if matches.len() != 1 {
            return Err("Dialogue objective authority is ambiguous for this response".into());
        }
        let (case_id, objective_id, intended_recipient_id, expected_custody_version) =
            matches.pop().expect("exactly one binding candidate");
        pending.push((
            action,
            case_id,
            objective_id,
            intended_recipient_id,
            expected_custody_version,
        ));
    }

    for (action, case_id, objective_id, intended_recipient_id, expected_custody_version) in pending
    {
        let id = dialogue_binding_id(
            &session.id,
            character_id,
            source_scope,
            action,
            issued_revision,
        );
        if let Some(existing) = ctx.db.dialogue_investigation_binding().id().find(&id) {
            if existing.party_id != party_id
                || existing.intended_recipient_id != intended_recipient_id
                || existing.case_id != case_id
                || existing.objective_id != objective_id
                || existing.expected_custody_version != expected_custody_version
                || !existing.consumed_by.is_empty()
            {
                return Err("Dialogue investigation binding conflicts with prior authority".into());
            }
            continue;
        }
        ctx.db
            .dialogue_investigation_binding()
            .insert(DialogueInvestigationBinding {
                id,
                session_id: session.id.clone(),
                character_id,
                party_id: party_id.clone(),
                intended_recipient_id,
                action_family: format!("{action:?}"),
                source_scope: source_scope.into(),
                case_id,
                objective_id,
                expected_custody_version,
                issued_revision,
                consumed_by: String::new(),
            });
    }
    Ok(true)
}

fn case_has_exact_dialogue_provenance(
    ctx: &ReducerContext,
    case: &CaseAuthority,
    exact_case_refs: &HashSet<String>,
) -> bool {
    case_refs_have_exact_dialogue_provenance(&observer_case_refs(ctx, case), exact_case_refs)
}

fn case_refs_have_exact_dialogue_provenance(
    observer_case_refs: &HashSet<String>,
    exact_case_refs: &HashSet<String>,
) -> bool {
    observer_case_refs
        .iter()
        .any(|case_ref| exact_case_refs.contains(case_ref))
}

fn refresh_dialogue_topic_options(
    ctx: &ReducerContext,
    session: &DialogueSession,
    character_id: u64,
) -> Result<(), String> {
    let conversation = adventuresim_dialogue::find_conversation(&session.conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    let facts = dialogue_fact_context(ctx, session, character_id)?;
    let existing: Vec<_> = ctx
        .db
        .dialogue_topic_option()
        .session_id()
        .filter(&session.id)
        .map(|row| row.id)
        .collect();
    for id in existing {
        ctx.db.dialogue_topic_option().id().delete(&id);
    }
    for topic in &conversation.topics {
        let known = topic.initially_known
            || ctx
                .db
                .character_topic_knowledge()
                .character_id()
                .filter(character_id)
                .any(|row| {
                    row.conversation_id == session.conversation_id && row.topic_id == topic.id
                });
        if known && facts.matches(&topic.conditions) {
            let response = adventuresim_dialogue::select_response(topic, &facts)
                .map_err(|_| "No unambiguous eligible dialogue response")?;
            let response_scope = format!("topic:{}:{}", topic.id, response.id);
            let response_is_bound = issue_dialogue_investigation_bindings(
                ctx,
                character_id,
                session,
                &response_scope,
                session.revision,
                &response.effects,
            )?;
            let choices_are_bound = if let Some(prompt) = &response.prompt {
                let mut bound = true;
                for choice in &prompt.choices {
                    let choice_scope =
                        format!("prompt:{}:{}:{}", prompt.id, response.id, choice.id);
                    bound &= issue_dialogue_investigation_bindings(
                        ctx,
                        character_id,
                        session,
                        &choice_scope,
                        session.revision.saturating_add(1),
                        &choice.effects,
                    )?;
                }
                bound
            } else {
                true
            };
            if !response_is_bound || !choices_are_bound {
                continue;
            }
            ctx.db.dialogue_topic_option().insert(DialogueTopicOption {
                id: format!("{}:{}", session.id, topic.id),
                gateway_bucket: 0,
                session_id: session.id.clone(),
                topic_id: topic.id.clone(),
                label: topic.label.clone(),
                source_ref_json: serde_json::to_string(&adventuresim_dialogue::source_for_topic(
                    &session.conversation_id,
                    &topic.id,
                ))
                .map_err(|_| "Could not encode topic source")?,
            });
        }
    }
    Ok(())
}

#[reducer]
pub fn choose_dialogue_topic(
    ctx: &ReducerContext,
    character_id: u64,
    session_id: String,
    topic_id: String,
    action_id: String,
    expected_revision: u64,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    validate_dialogue_action_id(&action_id)?;
    let action_row_id = format!("{session_id}:{action_id}");
    let mut session = require_session_member(ctx, &session_id, character_id)?;
    if ctx.db.dialogue_action().id().find(&action_row_id).is_some() {
        return Ok(());
    }
    if session.catalog_revision != catalog_revision {
        return Err("Dialogue session revision is stale".into());
    }
    if session.revision != expected_revision {
        return Err("Dialogue action used a stale session revision".into());
    }
    let conversation = adventuresim_dialogue::find_conversation(&session.conversation_id)
        .ok_or("Unknown dialogue conversation")?;
    let topic = conversation
        .topics
        .iter()
        .find(|topic| topic.id == topic_id)
        .ok_or("Unknown dialogue topic")?;
    let known = topic.initially_known
        || ctx
            .db
            .character_topic_knowledge()
            .character_id()
            .filter(character_id)
            .any(|row| row.conversation_id == session.conversation_id && row.topic_id == topic.id);
    if !known {
        return Err("Dialogue topic is not known by this character".into());
    }
    validate_dialogue_cardinality(ctx, &session, conversation)?;
    let facts = dialogue_fact_context(ctx, &session, character_id)?;
    if !facts.matches(&topic.conditions) {
        return Err("Dialogue topic is not eligible in this context".into());
    }
    let response = adventuresim_dialogue::select_response(topic, &facts)
        .map_err(|_| "No unambiguous eligible dialogue response")?;
    let sequence = ctx
        .db
        .dialogue_event()
        .session_id()
        .filter(&session_id)
        .count() as u32;
    for (offset, turn) in response.turns.iter().enumerate() {
        let source_refs: Vec<_> = turn
            .fragments
            .iter()
            .enumerate()
            .map(|(fragment, authored)| {
                let field = match authored {
                    adventuresim_dialogue::Fragment::Text { .. } => "value",
                    adventuresim_dialogue::Fragment::Topic { .. } => "label",
                    adventuresim_dialogue::Fragment::Runtime { .. } => "slot",
                };
                adventuresim_dialogue::source_for_fragment(
                    &session.conversation_id,
                    &topic.id,
                    &response.id,
                    offset,
                    fragment,
                    field,
                )
            })
            .collect();
        ctx.db.dialogue_event().insert(DialogueEvent {
            id: format!("{session_id}:event:{}", sequence + offset as u32),
            gateway_bucket: 0,
            session_id: session_id.clone(),
            sequence: sequence + offset as u32,
            response_id: response.id.clone(),
            speaker_role: turn.speaker.clone(),
            fragments_json: serde_json::to_string(&resolve_dialogue_fragments(
                ctx,
                &session,
                character_id,
                &turn.speaker,
                &turn.fragments,
            )?)
            .map_err(|_| "Could not encode dialogue turn")?,
            source_refs_json: serde_json::to_string(&source_refs)
                .map_err(|_| "Could not encode dialogue sources")?,
            created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
    for effect in &response.effects {
        let source_scope = format!("topic:{}:{}", topic.id, response.id);
        apply_dialogue_effect(
            ctx,
            character_id,
            &session,
            &source_scope,
            &action_id,
            session.revision.saturating_add(1),
            effect,
        )?;
    }
    if let Some(prompt) = &response.prompt {
        let id = format!("{session_id}:prompt:{}:{action_id}", prompt.id);
        if ctx.db.dialogue_prompt().id().find(&id).is_none() {
            ctx.db.dialogue_prompt().insert(DialoguePrompt {
                id,
                gateway_bucket: 0,
                session_id: session_id.clone(),
                prompt_id: prompt.id.clone(),
                mode: format!("{:?}", prompt.mode),
                respondent_role: prompt.respondent.clone(),
                resolution_policy: format!("{:?}", prompt.resolution),
                choices_json: serde_json::to_string(&prompt.choices)
                    .map_err(|_| "Could not encode dialogue choices")?,
                min_choices: prompt.min_choices as u32,
                max_choices: prompt.max_choices as u32,
                state: "open".into(),
                resolved_choice_ids_json: "[]".into(),
                source_refs_json: serde_json::to_string(
                    &prompt
                        .choices
                        .iter()
                        .map(|choice| {
                            adventuresim_dialogue::source_for_choice(
                                &session.conversation_id,
                                &topic.id,
                                &response.id,
                                &choice.id,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| "Could not encode prompt sources")?,
            });
        }
    }
    session.revision += 1;
    ctx.db.dialogue_session().id().update(session.clone());
    ctx.db.dialogue_action().insert(DialogueAction {
        id: action_row_id,
        session_id,
        action_id,
        action_kind: "topic".into(),
        resulting_revision: session.revision,
    });
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}

#[reducer]
pub fn answer_dialogue_prompt(
    ctx: &ReducerContext,
    character_id: u64,
    prompt_row_id: String,
    choice_ids_json: String,
    action_id: String,
    expected_revision: u64,
    catalog_revision: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_dialogue_revision(&catalog_revision)?;
    validate_dialogue_action_id(&action_id)?;
    let prompt = ctx
        .db
        .dialogue_prompt()
        .id()
        .find(&prompt_row_id)
        .ok_or("Dialogue prompt not found")?;
    let action_row_id = format!("{}:{action_id}", prompt.session_id);
    if prompt.state != "open" {
        return Err("Dialogue prompt is closed".into());
    }
    let mut session = require_session_member(ctx, &prompt.session_id, character_id)?;
    if ctx.db.dialogue_action().id().find(&action_row_id).is_some() {
        return Ok(());
    }
    if session.catalog_revision != catalog_revision {
        return Err("Dialogue session revision is stale".into());
    }
    if session.revision != expected_revision {
        return Err("Dialogue answer used a stale session revision".into());
    }
    let participant = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&prompt.session_id)
        .find(|participant| participant.character_id == Some(character_id))
        .ok_or("Character is not a dialogue participant")?;
    if participant.role != prompt.respondent_role {
        return Err("Character is not an eligible respondent for this prompt".into());
    }
    let chosen: Vec<String> =
        serde_json::from_str(&choice_ids_json).map_err(|_| "Invalid dialogue choices")?;
    let allowed: Vec<adventuresim_dialogue::Choice> =
        serde_json::from_str(&prompt.choices_json).map_err(|_| "Invalid authoritative choices")?;
    let unique: std::collections::BTreeSet<_> = chosen.iter().collect();
    if chosen.len() != unique.len()
        || chosen.len() < prompt.min_choices as usize
        || chosen.len() > prompt.max_choices as usize
        || chosen
            .iter()
            .any(|id| !allowed.iter().any(|choice| &choice.id == id))
        || (!prompt.mode.contains("Multi") && chosen.len() != 1)
    {
        return Err("Invalid dialogue answer".into());
    }
    let id = format!("{}:{character_id}", prompt.id);
    if ctx.db.dialogue_answer().id().find(&id).is_some() {
        return Err("Dialogue prompt was already answered by this character".into());
    }
    ctx.db.dialogue_answer().insert(DialogueAnswer {
        id,
        prompt_row_id,
        character_id,
        choice_ids_json: serde_json::to_string(&chosen).unwrap(),
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    let answer_count = ctx
        .db
        .dialogue_answer()
        .prompt_row_id()
        .filter(&prompt.id)
        .count();
    let respondent_count = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&prompt.session_id)
        .filter(|participant| participant.role == prompt.respondent_role)
        .count();
    let answers: Vec<_> = ctx
        .db
        .dialogue_answer()
        .prompt_row_id()
        .filter(&prompt.id)
        .collect();
    let ballots: Vec<Vec<String>> = answers
        .iter()
        .filter_map(|answer| serde_json::from_str(&answer.choice_ids_json).ok())
        .collect();
    let mut vote_counts = std::collections::BTreeMap::<String, usize>::new();
    for ballot in &ballots {
        for choice in ballot {
            *vote_counts.entry(choice.clone()).or_default() += 1;
        }
    }
    let winning = if prompt.resolution_policy.contains("FirstResponse") {
        ballots.first().cloned()
    } else if prompt.resolution_policy.contains("Majority") {
        vote_counts
            .iter()
            .filter(|(_, count)| **count > respondent_count / 2)
            .map(|(choice, _)| vec![choice.clone()])
            .next()
    } else if prompt.resolution_policy.contains("Unanimous")
        && answer_count >= respondent_count
        && ballots.windows(2).all(|pair| pair[0] == pair[1])
    {
        ballots.first().cloned()
    } else if prompt.resolution_policy.contains("AllRespondents")
        && answer_count >= respondent_count
    {
        vote_counts
            .iter()
            .max_by_key(|(choice, count)| (**count, std::cmp::Reverse((*choice).clone())))
            .map(|(choice, _)| vec![choice.clone()])
    } else {
        None
    };
    if let Some(winning) = winning {
        let topic = adventuresim_dialogue::find_conversation(&session.conversation_id)
            .and_then(|conversation| {
                conversation.topics.iter().find(|topic| {
                    topic.responses.iter().any(|response| {
                        response
                            .prompt
                            .as_ref()
                            .is_some_and(|authored| authored.id == prompt.prompt_id)
                    })
                })
            })
            .ok_or("Dialogue prompt topic is no longer authored")?;
        let response = topic
            .responses
            .iter()
            .find(|response| {
                response
                    .prompt
                    .as_ref()
                    .is_some_and(|authored| authored.id == prompt.prompt_id)
            })
            .ok_or("Dialogue prompt response is no longer authored")?;
        for choice in allowed.iter().filter(|choice| winning.contains(&choice.id)) {
            for effect in &choice.effects {
                let source_scope =
                    format!("prompt:{}:{}:{}", prompt.prompt_id, response.id, choice.id);
                apply_dialogue_effect(
                    ctx,
                    character_id,
                    &session,
                    &source_scope,
                    &action_id,
                    session.revision.saturating_add(1),
                    effect,
                )?;
            }
            let mut sequence = ctx
                .db
                .dialogue_event()
                .session_id()
                .filter(&session.id)
                .count() as u32;
            for (turn_index, turn) in choice.result_turns.iter().enumerate() {
                let source_refs: Vec<_> = turn
                    .fragments
                    .iter()
                    .enumerate()
                    .map(|(fragment_index, fragment)| {
                        let field = match fragment {
                            adventuresim_dialogue::Fragment::Text { .. } => "value",
                            adventuresim_dialogue::Fragment::Topic { .. } => "label",
                            adventuresim_dialogue::Fragment::Runtime { .. } => "slot",
                        };
                        adventuresim_dialogue::source_for_choice_fragment(
                            &session.conversation_id,
                            &topic.id,
                            &response.id,
                            &choice.id,
                            turn_index,
                            fragment_index,
                            field,
                        )
                    })
                    .collect();
                ctx.db.dialogue_event().insert(DialogueEvent {
                    id: format!("{}:event:{sequence}", session.id),
                    gateway_bucket: 0,
                    session_id: session.id.clone(),
                    sequence,
                    response_id: format!("{}:{}", response.id, choice.id),
                    speaker_role: turn.speaker.clone(),
                    fragments_json: serde_json::to_string(&resolve_dialogue_fragments(
                        ctx,
                        &session,
                        character_id,
                        &turn.speaker,
                        &turn.fragments,
                    )?)
                    .map_err(|_| "Could not encode dialogue result")?,
                    source_refs_json: serde_json::to_string(&source_refs)
                        .map_err(|_| "Could not encode dialogue result sources")?,
                    created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
                });
                sequence += 1;
            }
        }
        let mut prompt = prompt;
        prompt.state = "resolved".into();
        prompt.resolved_choice_ids_json = serde_json::to_string(&winning).unwrap();
        ctx.db.dialogue_prompt().id().update(prompt);
    }
    session.revision += 1;
    ctx.db.dialogue_session().id().update(session.clone());
    ctx.db.dialogue_action().insert(DialogueAction {
        id: action_row_id,
        session_id: session.id.clone(),
        action_id,
        action_kind: "answer".into(),
        resulting_revision: session.revision,
    });
    refresh_dialogue_topic_options(ctx, &session, character_id)?;
    Ok(())
}

fn apply_dialogue_effect(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    source_scope: &str,
    action_id: &str,
    resulting_revision: u64,
    effect: &adventuresim_dialogue::Effect,
) -> Result<(), String> {
    let live_npc = require_live_dialogue_presence(ctx, session, character_id)?;
    match effect {
        adventuresim_dialogue::Effect::LearnTopic { topic } => {
            let id = format!("{character_id}:{}:{topic}", session.conversation_id);
            if ctx.db.character_topic_knowledge().id().find(&id).is_none() {
                ctx.db
                    .character_topic_knowledge()
                    .insert(CharacterTopicKnowledge {
                        id,
                        character_id,
                        conversation_id: session.conversation_id.clone(),
                        topic_id: topic.clone(),
                        learned_micros: ctx.timestamp.to_micros_since_unix_epoch(),
                    });
            }
            Ok(())
        }
        adventuresim_dialogue::Effect::AcceptContract { contract }
            if contract != "selected-service-contract" =>
        {
            Err("Dialogue contracts must use the session-bound selection".into())
        }
        adventuresim_dialogue::Effect::ReportContract { contract }
            if contract != "selected-service-contract" =>
        {
            Err("Dialogue reports must use the session-bound active contract".into())
        }
        adventuresim_dialogue::Effect::BeginApprenticeship { profession } => {
            let service = if profession == "selected-service" {
                dialogue_service_id(ctx, session)?
            } else {
                profession.clone()
            };
            crate::time::begin_apprenticeship(ctx, character_id, &service)
        }
        adventuresim_dialogue::Effect::ExamineDisease => {
            crate::disease::examine_by_herbalist(ctx, character_id, session.settlement_id.clone())
        }
        adventuresim_dialogue::Effect::ReceiveReferredTestimony => {
            receive_referred_testimony(ctx, character_id, session, &live_npc, action_id)
        }
        adventuresim_dialogue::Effect::InvestigationAction { action } => {
            apply_dialogue_investigation_action(
                ctx,
                character_id,
                session,
                action,
                source_scope,
                action_id,
            )
        }
        adventuresim_dialogue::Effect::AcceptContract { .. } => {
            let service = dialogue_service_id(ctx, session)?;
            let contract_id = ctx
                .db
                .contract_authority()
                .service_id()
                .filter(&service)
                .find(|contract| {
                    contract.settlement_id == session.settlement_id
                        && contract.issuer_npc_id == live_npc.id
                        && contract.status == ContractStatus::Offered
                })
                .map(|contract| contract.id)
                .ok_or("This issuer has no available contract")?;
            record_dialogue_contract_issuer_interaction(
                ctx,
                character_id,
                contract_id.clone(),
                ContractInteractionStage::Accept,
                session,
                action_id,
                resulting_revision,
            )?;
            accept_contract(ctx, character_id, contract_id)
        }
        adventuresim_dialogue::Effect::ReportContract { .. } => {
            let character = ctx
                .db
                .character()
                .id()
                .find(character_id)
                .ok_or("Character not found")?;
            let party_id = character.party_id.ok_or("Character has no party")?;
            let contract_id = ctx
                .db
                .party_authority()
                .id()
                .find(&party_id)
                .and_then(|party| party.active_contract_id)
                .ok_or("Party has no active contract")?;
            let service = dialogue_service_id(ctx, session)?;
            let local_issuer = ctx
                .db
                .contract_authority()
                .id()
                .find(&contract_id)
                .is_some_and(|contract| {
                    contract.service_id == service
                        && contract.settlement_id == session.settlement_id
                        && contract.issuer_npc_id == live_npc.id
                });
            if !local_issuer {
                return Err("This NPC did not issue the active contract".into());
            }
            record_dialogue_contract_issuer_interaction(
                ctx,
                character_id,
                contract_id.clone(),
                ContractInteractionStage::Report,
                session,
                action_id,
                resulting_revision,
            )?;
            report_contract(ctx, character_id, contract_id)
        }
        adventuresim_dialogue::Effect::SetFlag { flag, value } if flag == "profess-local-faith" => {
            let religion_id = if *value {
                ctx.db
                    .settlement()
                    .id()
                    .find(&session.settlement_id)
                    .ok_or("Dialogue settlement not found")?
                    .religion_id
            } else {
                String::new()
            };
            crate::condition::set_character_religion(ctx, character_id, religion_id)
        }
        adventuresim_dialogue::Effect::SetFlag { .. } => Err("Unknown dialogue flag".into()),
    }
}

fn dialogue_service_id(ctx: &ReducerContext, session: &DialogueSession) -> Result<String, String> {
    ctx.db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .find_map(|participant| {
            participant
                .character_id
                .is_none()
                .then(|| {
                    ctx.db
                        .settlement_npc()
                        .id()
                        .find(&participant.actor_id)
                        .map(|npc| npc.service_id)
                })
                .flatten()
        })
        .ok_or("Dialogue has no service actor".into())
}

fn apply_dialogue_investigation_action(
    ctx: &ReducerContext,
    character_id: u64,
    session: &DialogueSession,
    action: &adventuresim_dialogue::InvestigationAction,
    source_scope: &str,
    action_id: &str,
) -> Result<(), String> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    use adventuresim_core::case::OutcomeFactKind as F;

    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can perform case objectives".into());
    }
    let binding_id = dialogue_binding_id(
        &session.id,
        character_id,
        source_scope,
        action,
        session.revision,
    );
    let binding = ctx
        .db
        .dialogue_investigation_binding()
        .id()
        .find(&binding_id)
        .ok_or("Dialogue investigation action has no pre-issued binding")?;
    if binding.session_id != session.id
        || binding.character_id != character_id
        || binding.party_id != party_id
        || binding.action_family != format!("{action:?}")
        || binding.source_scope != source_scope
        || binding.issued_revision != session.revision
        || !binding.consumed_by.is_empty()
    {
        return Err("Dialogue investigation binding is stale, replayed, or conflicting".into());
    }
    let npc_ids: HashSet<_> = ctx
        .db
        .dialogue_participant()
        .session_id()
        .filter(&session.id)
        .filter(|row| row.character_id.is_none())
        .map(|row| row.actor_id)
        .collect();
    if !npc_ids.contains(&binding.intended_recipient_id) {
        return Err("Dialogue investigation recipient is no longer in this session".into());
    }
    let active_contract = party
        .active_contract_id
        .as_ref()
        .and_then(|id| ctx.db.contract_authority().id().find(id));
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&binding.case_id)
        .ok_or("Dialogue investigation case no longer exists")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Dialogue investigation case is no longer open".into());
    }
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    let objective = expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .find(|objective| objective.id.as_str() == binding.objective_id)
        .ok_or("Pre-issued dialogue objective no longer exists")?;
    let recipient = dialogue_objective_recipient(
        ctx,
        character_id,
        &party_id,
        &case,
        &objective.requirement,
        action,
        &npc_ids,
        &binding.intended_recipient_id,
        active_contract.as_ref(),
    )
    .ok_or("Pre-issued dialogue objective is no longer authorized")?;
    let recipient = generated_dialogue_recipient(
        ctx,
        &case,
        objective.id.as_str(),
        action,
        &npc_ids,
        recipient,
    )?
    .ok_or("Generated dialogue producer is no longer authorized")?;
    if recipient != binding.intended_recipient_id {
        return Err("Pre-issued dialogue recipient no longer matches".into());
    }
    let source_id = format!(
        "dialogue-objective:{}:{action_id}:{}",
        session.id,
        objective.id.as_str()
    );
    match &objective.requirement {
        R::Return {
            asset_id,
            custodian_id,
        } => {
            let current = ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .ok_or("Returned asset has no custody authority")?;
            if Some(current.version) != binding.expected_custody_version
                || current.case_id != case.id
                || current.holder_kind != CustodyHolderKind::Party
                || current.holder_id != party_id
            {
                return Err("Returned asset custody is stale or belongs elsewhere".into());
            }
            record_asset_returned_or_exchanged(
                ctx,
                &source_id,
                &case.id,
                &party_id,
                asset_id.as_str(),
                custodian_id,
                current.version.saturating_add(1),
                false,
            )?;
            let mut binding = binding;
            binding.consumed_by = action_id.into();
            ctx.db.dialogue_investigation_binding().id().update(binding);
            return Ok(());
        }
        R::Release { subject_id } => {
            let current = ctx
                .db
                .case_custody()
                .object_id()
                .find(&subject_id.as_str().to_string())
                .ok_or("Released subject has no custody authority")?;
            if Some(current.version) != binding.expected_custody_version
                || current.case_id != case.id
                || current.holder_kind != CustodyHolderKind::Party
                || current.holder_id != party_id
            {
                return Err("Released subject custody is stale or belongs elsewhere".into());
            }
            record_subject_rescued_or_released(
                ctx,
                &source_id,
                &case.id,
                &party_id,
                subject_id.as_str(),
                current.version.saturating_add(1),
                true,
            )?;
            let mut binding = binding;
            binding.consumed_by = action_id.into();
            ctx.db.dialogue_investigation_binding().id().update(binding);
            return Ok(());
        }
        R::Exchange {
            asset_id,
            recipient_id,
        } => {
            let current = ctx
                .db
                .case_custody()
                .object_id()
                .find(&asset_id.as_str().to_string())
                .ok_or("Exchanged asset has no custody authority")?;
            if Some(current.version) != binding.expected_custody_version
                || current.case_id != case.id
                || current.holder_kind != CustodyHolderKind::Party
                || current.holder_id != party_id
            {
                return Err("Exchanged asset custody is stale or belongs elsewhere".into());
            }
            record_asset_returned_or_exchanged(
                ctx,
                &source_id,
                &case.id,
                &party_id,
                asset_id.as_str(),
                recipient_id,
                current.version.saturating_add(1),
                true,
            )?;
            let mut binding = binding;
            binding.consumed_by = action_id.into();
            ctx.db.dialogue_investigation_binding().id().update(binding);
            return Ok(());
        }
        _ => {}
    }
    let fact = match &objective.requirement {
        R::Locate { subject_ref } => F::Located {
            subject_ref: subject_ref.clone(),
        },
        R::Identify { subject_ref } => F::Identified {
            subject_ref: subject_ref.clone(),
        },
        R::Expose { subject_ref } => F::Exposed {
            subject_ref: subject_ref.clone(),
        },
        R::PresentProof {
            evidence_id,
            recipient_id,
        } => F::ProofPresented {
            evidence_id: evidence_id.clone(),
            recipient_id: recipient_id.clone(),
        },
        R::PresentTestimony {
            witness_id,
            recipient_id,
        } => F::TestimonyPresented {
            witness_id: witness_id.clone(),
            recipient_id: recipient_id.clone(),
        },
        R::Negotiate { subject_ref } => F::Negotiated {
            subject_ref: subject_ref.clone(),
        },
        R::ReportToIssuer { issuer_id } => F::Reported {
            issuer_id: issuer_id.clone(),
        },
        _ => return Err("Dialogue action selected the wrong objective family".into()),
    };
    ingest_case_outcome_fact(ctx, &source_id, &case.id, &party_id, fact)?;
    let mut binding = binding;
    binding.consumed_by = action_id.into();
    ctx.db.dialogue_investigation_binding().id().update(binding);
    Ok(())
}

fn same_location(ctx: &ReducerContext, left: &crate::Character, right: &crate::Character) -> bool {
    let left_site = crate::investigation::character_case_site_id(ctx, left.id);
    let right_site = crate::investigation::character_case_site_id(ctx, right.id);
    left.current_settlement_id == right.current_settlement_id
        && left_site == right_site
        && (left.current_settlement_id.is_some() || left_site.is_some())
}

fn player_conversation_parties(
    ctx: &ReducerContext,
    sender: &crate::Character,
    subject_id: u64,
) -> Result<(String, String), String> {
    let subject = ctx
        .db
        .character()
        .id()
        .find(subject_id)
        .ok_or("Conversation subject not found")?;
    if !same_location(ctx, sender, &subject) {
        return Err("Local conversations require a shared location".into());
    }
    let sender_party = sender.party_id.as_deref().ok_or("Sender has no party")?;
    let subject_party = subject.party_id.as_deref().ok_or("Subject has no party")?;
    Ok((sender_party.to_string(), subject_party.to_string()))
}

fn npc_conversation_authority_matches(
    settlement_id: &str,
    npc_home_settlement_id: &str,
    npc_id: &str,
    presence_npc_id: &str,
    presence_settlement_id: &str,
    presence_location_id: &str,
    requested_location_id: &str,
    presence_start_minute: u16,
    presence_end_minute: u16,
    minute: u64,
) -> bool {
    let minute = (minute % 1_440) as u16;
    npc_home_settlement_id == settlement_id
        && presence_npc_id == npc_id
        && presence_settlement_id == settlement_id
        && presence_location_id == requested_location_id
        && !requested_location_id.is_empty()
        && presence_start_minute <= minute
        && minute < presence_end_minute
}

fn npc_conversation_party(
    ctx: &ReducerContext,
    sender: &crate::Character,
    subject_id: &str,
    location_id: &str,
) -> Result<String, String> {
    let party_id = sender.party_id.as_deref().ok_or("Sender has no party")?;
    let settlement_id = sender
        .current_settlement_id
        .as_deref()
        .ok_or("NPC conversations require a settlement")?;
    if subject_id.is_empty()
        || subject_id.chars().count() > 160
        || subject_id.chars().any(char::is_control)
    {
        return Err("NPC is not at the sender's settlement".into());
    }
    let npc = ctx
        .db
        .settlement_npc()
        .id()
        .find(&subject_id.to_string())
        .ok_or("NPC is not at the sender's settlement")?;
    let presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&subject_id.to_string())
        .ok_or("NPC is not at the sender's settlement")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(sender.id)
        .map_or(720, |time| time.minutes);
    if !npc_conversation_authority_matches(
        settlement_id,
        &npc.home_settlement_id,
        &npc.id,
        &presence.npc_id,
        &presence.settlement_id,
        &presence.location_id,
        location_id,
        presence.start_minute,
        presence.end_minute,
        minute,
    ) {
        return Err("NPC is not at the sender's settlement".into());
    }
    Ok(party_id.to_string())
}

#[reducer]
pub fn send_local_chat_message(
    ctx: &ReducerContext,
    sender_id: u64,
    subject_kind: String,
    subject_id: String,
    location_id: String,
    body: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, sender_id)?;
    let sender = ctx
        .db
        .character()
        .id()
        .find(sender_id)
        .ok_or("Sender not found")?;
    let body = body.trim();
    if body.is_empty() || body.chars().count() > 500 {
        return Err("Messages must contain 1 to 500 characters".into());
    }
    let (audience_party_id, other_party_id, npc_id) = match subject_kind.as_str() {
        "player" => {
            if !location_id.is_empty() {
                return Err("Player conversations do not accept an NPC location".into());
            }
            let (audience_party_id, other_party_id) = player_conversation_parties(
                ctx,
                &sender,
                subject_id.parse().map_err(|_| "Invalid player subject")?,
            )?;
            (audience_party_id, other_party_id, String::new())
        }
        "npc" => (
            npc_conversation_party(ctx, &sender, &subject_id, &location_id)?,
            String::new(),
            subject_id,
        ),
        _ => return Err("Unknown Local conversation subject".into()),
    };
    ctx.db.local_chat_message().insert(LocalChatMessage {
        id: 0,
        gateway_bucket: 0,
        audience_party_id,
        other_party_id,
        npc_id,
        sender_id,
        sender_name: sender.name,
        body: body.to_string(),
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    Ok(())
}

#[reducer]
pub fn request_party_action(
    ctx: &ReducerContext,
    requester_id: u64,
    action_kind: String,
    summary: String,
    payload: String,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, requester_id)?;
    let requester = ctx
        .db
        .character()
        .id()
        .find(requester_id)
        .ok_or("Character not found")?;
    let party_id = requester.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id == requester_id {
        return Err("The party leader does not need to request permission".into());
    }
    let allowed = [
        "travel",
        "kick",
        "add_role",
        "edit_role",
        "delete_role",
        "accept_join",
        "reject_join",
        "accept_contract",
        "abandon_contract",
        "report_contract",
        "autoresolve",
        "party_checks",
        "party_inventory",
        "disband_party",
        "initiate_combat",
        "cancel_mission",
        "investigate",
    ];
    if !allowed.contains(&action_kind.as_str()) {
        return Err("Unknown party action request".into());
    }
    // Travel destinations supersede one another. Inventory target/staging edits
    // are intentionally coalesced to one notification per requesting member.
    if action_kind == "travel" || action_kind == "party_inventory" {
        let old: Vec<_> = ctx
            .db
            .party_action_request_authority()
            .requester_id()
            .filter(requester_id)
            .filter(|request| request.party_id == party_id && request.action_kind == action_kind)
            .map(|request| request.id)
            .collect();
        for id in old {
            ctx.db.party_action_request_authority().id().delete(id);
        }
    }
    ctx.db
        .party_action_request_authority()
        .insert(PartyActionRequest {
            id: 0,
            gateway_bucket: 0,
            party_id,
            requester_id,
            action_kind,
            summary: summary.trim().to_string(),
            payload,
        });
    Ok(())
}

#[reducer]
pub fn dismiss_party_action_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, leader_id)?;
    let request = ctx
        .db
        .party_action_request_authority()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can resolve requests".into());
    }
    ctx.db
        .party_action_request_authority()
        .id()
        .delete(request_id);
    Ok(())
}

/// Atomically execute and resolve a member's approved action. SpacetimeDB
/// reducers are transactional, so a failed action leaves the request intact;
/// a committed request id is recorded to make retries idempotent.
#[reducer]
pub fn approve_party_action_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if let Some(resolved) = ctx.db.resolved_party_action().id().find(request_id) {
        if resolved.approved_by != leader_id {
            return Err("Only the party leader can approve requests".into());
        }
        return Ok(());
    }
    let request = ctx
        .db
        .party_action_request_authority()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can approve requests".into());
    }
    let action: ApprovedPartyAction = serde_json::from_str(&request.payload)
        .map_err(|error| format!("Invalid party action payload: {error}"))?;
    if action.kind() != request.action_kind {
        return Err("Party action kind does not match its typed payload".into());
    }
    action.execute(ctx, leader_id, request.requester_id)?;
    ctx.db.resolved_party_action().insert(ResolvedPartyAction {
        id: request.id,
        party_id: request.party_id,
        approved_by: leader_id,
    });
    ctx.db
        .party_action_request_authority()
        .id()
        .delete(request_id);
    Ok(())
}

#[reducer]
pub fn approve_party_action_request_planned(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
    route: JourneyRoutePlan,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if let Some(resolved) = ctx.db.resolved_party_action().id().find(request_id) {
        if resolved.approved_by != leader_id {
            return Err("Only the party leader can approve requests".into());
        }
        return Ok(());
    }
    let request = ctx
        .db
        .party_action_request_authority()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can resolve requests".into());
    }
    let action: ApprovedPartyAction = serde_json::from_str(&request.payload)
        .map_err(|error| format!("Invalid party action payload: {error}"))?;
    if action.kind() != request.action_kind {
        return Err("Party action kind does not match its typed payload".into());
    }
    match action {
        ApprovedPartyAction::TravelToSettlement { settlement_id } => {
            travel_to_settlement_impl(ctx, leader_id, settlement_id, Some(route))?
        }
        ApprovedPartyAction::TravelToCaseSite { case_site_id } => {
            travel_to_case_site_impl(ctx, leader_id, case_site_id, Some(route))?
        }
        _ => return Err("A planned approval is only valid for travel".into()),
    }
    ctx.db.resolved_party_action().insert(ResolvedPartyAction {
        id: request.id,
        party_id: request.party_id,
        approved_by: leader_id,
    });
    ctx.db
        .party_action_request_authority()
        .id()
        .delete(request_id);
    Ok(())
}

#[reducer]
pub fn vote_for_party_leader(
    ctx: &ReducerContext,
    voter_id: u64,
    candidate_id: u64,
) -> Result<(), String> {
    let voter = ctx
        .db
        .character()
        .id()
        .find(voter_id)
        .ok_or("Voter not found")?;
    if !voter.alive {
        return Err("Dead characters cannot vote".into());
    }
    let party_id = voter.party_id.ok_or("Voter has no party")?;
    require_no_unresolved_encounter(ctx, &party_id)?;
    ctx.db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    let candidate = ctx
        .db
        .character()
        .id()
        .find(candidate_id)
        .ok_or("Candidate not found")?;
    if !candidate.alive || candidate.party_id.as_deref() != Some(&party_id) {
        return Err("Candidate must be a living member of this party".into());
    }
    let id = format!("{party_id}:{voter_id}");
    let vote = PartyLeaderVote {
        id: id.clone(),
        party_id: party_id.clone(),
        voter_id,
        candidate_id,
    };
    if ctx.db.party_leader_vote().id().find(&id).is_some() {
        ctx.db.party_leader_vote().id().update(vote);
    } else {
        ctx.db.party_leader_vote().insert(vote);
    }
    normalize_and_elect_party_leader(ctx, &party_id)?;
    Ok(())
}

fn put_leader_vote(ctx: &ReducerContext, party_id: &str, voter_id: u64, candidate_id: u64) {
    let id = format!("{party_id}:{voter_id}");
    let row = PartyLeaderVote {
        id: id.clone(),
        party_id: party_id.to_string(),
        voter_id,
        candidate_id,
    };
    if ctx.db.party_leader_vote().id().find(&id).is_some() {
        ctx.db.party_leader_vote().id().update(row);
    } else {
        ctx.db.party_leader_vote().insert(row);
    }
}

/// Lazily backfills standing votes and discards stale legacy succession rows.
/// This is intentionally safe to call after every membership or life-state
/// transition, preserving non-destructive compatibility with existing parties.
pub(crate) fn normalize_and_elect_party_leader(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let living = living_party_member_ids(ctx, party_id);
    let living_set: std::collections::HashSet<_> = living.iter().copied().collect();
    for vote in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        if !living_set.contains(&vote.voter_id) || !living_set.contains(&vote.candidate_id) {
            ctx.db.party_leader_vote().id().delete(&vote.id);
        }
    }
    if living_set.contains(&party.leader_id) {
        for voter_id in &living {
            let id = format!("{party_id}:{voter_id}");
            if ctx.db.party_leader_vote().id().find(&id).is_none() {
                // New and legacy members begin by supporting the incumbent.
                put_leader_vote(ctx, party_id, *voter_id, party.leader_id);
            }
        }
    } else if let [sole_survivor] = living.as_slice() {
        // Ensure a sole survivor can complete succession without deadlocking.
        put_leader_vote(ctx, party_id, *sole_survivor, *sole_survivor);
    }
    let leader_alive = living_set.contains(&party.leader_id);
    let ballots: Vec<_> = ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .map(|vote| (vote.voter_id, vote.candidate_id))
        .collect();
    if let Some(next) = adventuresim_core::leadership::elect_leader(
        party.leader_id,
        leader_alive,
        &living,
        &ballots,
    ) {
        party.leader_id = next;
        party.is_solo = living.len() == 1;
        ctx.db.party_authority().id().update(party);
    }
    Ok(())
}

#[reducer]
pub fn update_character(ctx: &ReducerContext, id: u64, name: String) -> Result<(), String> {
    crate::character::require_living_character(ctx, id)?;
    let Some(mut character) = ctx.db.character().id().find(id) else {
        return Err("Character not found".into());
    };

    character.name = name;
    ctx.db.character().id().update(character);
    Ok(())
}

pub(crate) fn create_solo_party_for_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<String, String> {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let party_id = format!("solo-{character_id}");
    if ctx.db.party_authority().id().find(&party_id).is_none() {
        ctx.db.party_authority().insert(Party {
            id: party_id.clone(),
            gateway_bucket: 0,
            name: format!("{}'s party", character.name),
            leader_id: character_id,
            current_settlement_id: character.current_settlement_id.clone(),
            current_case_site_id: crate::investigation::character_case_site_id(ctx, character_id)
                .map(CaseSiteId::from),
            active_contract_id: None,
            is_solo: true,
            camp_fatigue_percent: 50,
            walking_minutes_per_day: DEFAULT_WALKING_MINUTES_PER_DAY,
            travel_at_night: false,
            camp_duration_mode: CampDurationMode::Auto,
            fixed_camp_minutes: 0,
            camp_destination: None,
            camp_remaining_minutes: 0,
            pooled_water_ml: 0.0,
            medicine_target: 0.0,
            command_target: 0.0,
            religion_target: 0.0,
        });
        ctx.db.party_member().insert(PartyMember {
            id: 0,
            party_id: party_id.clone(),
            character_id,
            role: Some("Leader".into()),
            recruitment_role_id: None,
        });
        put_leader_vote(ctx, &party_id, character_id, character_id);
    }
    character.party_id = Some(party_id.clone());
    ctx.db.character().id().update(character);
    crate::social::reset_familiarity_after_join(ctx, character_id);
    normalize_and_elect_party_leader(ctx, &party_id)?;
    Ok(party_id)
}

/// Remove the isolated party created for a temporary tactical character.
/// Refuse to delete a party that has acquired any other member.
pub(crate) fn delete_temporary_character_party(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
) -> Result<(), String> {
    let party_key = party_id.to_string();
    let members: Vec<_> = ctx.db.party_member().party_id().filter(party_id).collect();
    if members
        .iter()
        .any(|member| member.character_id != character_id)
    {
        return Err("Temporary character party contains another member".into());
    }
    for member in members {
        ctx.db.party_member().id().delete(member.id);
    }
    for row in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_leader_vote().id().delete(&row.id);
    }
    for row in ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_stake().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        if let Some(condition) = ctx
            .db
            .party_item_condition()
            .party_inventory_item_id()
            .find(row.id)
        {
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(condition.party_inventory_item_id);
        }
        ctx.db.party_inventory_item().id().delete(row.id);
    }
    if ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db.party_inventory_state().party_id().delete(&party_key);
    }
    if ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db
            .party_journey_authority()
            .party_id()
            .delete(&party_key);
    }
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party_key);
    }
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_key)
        .is_some()
    {
        ctx.db
            .party_journey_itinerary()
            .party_id()
            .delete(&party_key);
    }
    for row in ctx
        .db
        .party_action_request_authority()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_action_request_authority().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_join_request()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_join_request().id().delete(row.id);
    }
    for row in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_recruitment_role().id().delete(row.id);
    }
    ctx.db.party_authority().id().delete(&party_key);
    Ok(())
}

/// Move a deterministic development fixture into another fixture's party
/// without going through the player-facing recruitment workflow.
pub(crate) fn attach_seeded_party_member(
    ctx: &ReducerContext,
    leader_id: u64,
    member_id: u64,
    role: &str,
) -> Result<(), String> {
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Seed party leader not found")?;
    let party_id = leader
        .party_id
        .clone()
        .ok_or("Seed party leader has no party")?;
    let mut member = ctx
        .db
        .character()
        .id()
        .find(member_id)
        .ok_or("Seed party member not found")?;

    if member.party_id.as_deref() == Some(&party_id) {
        if let Some(mut membership) = ctx
            .db
            .party_member()
            .character_id()
            .filter(member_id)
            .find(|membership| membership.party_id == party_id)
        {
            membership.role = Some(role.into());
            ctx.db.party_member().id().update(membership);
        }
        return Ok(());
    }

    if let Some(source_party_id) = member.party_id.clone() {
        let source_members: Vec<_> = ctx
            .db
            .party_member()
            .party_id()
            .filter(&source_party_id)
            .collect();
        if source_members
            .iter()
            .any(|membership| membership.character_id != member_id)
        {
            return Err("Seed party member belongs to a non-solo party".into());
        }
        for membership in source_members {
            ctx.db.party_member().id().delete(membership.id);
        }
        for vote in ctx
            .db
            .party_leader_vote()
            .party_id()
            .filter(&source_party_id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_leader_vote().id().delete(&vote.id);
        }
        ctx.db.party_authority().id().delete(&source_party_id);
    }

    member.party_id = Some(party_id.clone());
    member.current_settlement_id = leader.current_settlement_id.clone();
    ctx.db.character().id().update(member);
    crate::investigation::set_character_case_site(
        ctx,
        member_id,
        crate::investigation::character_case_site_id(ctx, leader_id),
    );
    crate::social::reset_familiarity_after_join(ctx, member_id);
    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: party_id.clone(),
        character_id: member_id,
        role: Some(role.into()),
        recruitment_role_id: None,
    });
    put_leader_vote(ctx, &party_id, member_id, leader_id);
    if let Some(mut party) = ctx.db.party_authority().id().find(&party_id) {
        party.is_solo = false;
        ctx.db.party_authority().id().update(party);
    }
    normalize_and_elect_party_leader(ctx, &party_id)?;
    Ok(())
}

#[reducer]
pub fn backfill_solo_parties(ctx: &ReducerContext) -> Result<(), String> {
    let ids: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|c| c.party_id.is_none())
        .map(|c| c.id)
        .collect();
    for id in ids {
        create_solo_party_for_character(ctx, id)?;
    }
    Ok(())
}

#[reducer]
pub fn create_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    name: String,
    quantity: u32,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
    save_role: bool,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if quantity == 0 || quantity > 8 {
        return Err("Role quantity must be between 1 and 8".into());
    }
    if !(0.0..=adventuresim_core::capability::WEAPON_PRECISION_RAPIER).contains(&weapon_precision)
        || (weapon_precision * 2.0).fract() != 0.0
    {
        return Err("Weapon precision must use a 0.5 step between 0 and 2".into());
    }
    if [
        requirements.athletics,
        requirements.endurance,
        requirements.medicine,
        requirements.surgery,
        requirements.command,
        requirements.religion,
    ]
    .iter()
    .any(|v| *v > 5)
    {
        return Err("Role ratings must be between 0 and 5".into());
    }
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can create roles".into());
    }
    let role_name = if name.trim().is_empty() {
        "Any adventurer".to_string()
    } else {
        name.trim().to_string()
    };
    ctx.db
        .party_recruitment_role()
        .insert(PartyRecruitmentRole {
            id: 0,
            party_id,
            name: role_name.clone(),
            requirements,
            quantity,
            weapon_precision,
        });
    if save_role {
        if name.trim().is_empty() {
            return Err("Name a role before saving it".into());
        }
        ctx.db
            .saved_recruitment_role()
            .insert(SavedRecruitmentRole {
                id: 0,
                owner_character_id: leader_id,
                name: role_name,
                requirements,
                weapon_precision,
            });
    }
    Ok(())
}

#[reducer]
pub fn update_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    role_id: u64,
    name: String,
    quantity: u32,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if quantity > 8 {
        return Err("Role quantity must be between 0 and 8".into());
    }
    validate_recruitment_requirements(requirements, weapon_precision)?;
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can edit roles".into());
    }
    let mut role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Recruitment role not found")?;
    if role.party_id != party_id {
        return Err("Cannot edit another party's role".into());
    }
    let filled = filled_role_slots(ctx, role_id);
    if quantity < filled {
        return Err(format!("This role already has {filled} filled slots"));
    }
    let role_name = if name.trim().is_empty() {
        "Any adventurer".to_string()
    } else {
        name.trim().to_string()
    };
    role.name = role_name.clone();
    role.quantity = quantity;
    role.requirements = requirements;
    role.weapon_precision = weapon_precision;
    ctx.db.party_recruitment_role().id().update(role);
    for mut member in ctx
        .db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .collect::<Vec<_>>()
    {
        member.role = Some(role_name.clone());
        ctx.db.party_member().id().update(member);
    }
    Ok(())
}

#[reducer]
pub fn delete_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    role_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can delete roles".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Recruitment role not found")?;
    if role.party_id != party_id {
        return Err("Cannot delete another party's role".into());
    }
    for request in ctx
        .db
        .party_join_request()
        .recruitment_role_id()
        .filter(role_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_join_request().id().delete(request.id);
    }
    for mut member in ctx
        .db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .collect::<Vec<_>>()
    {
        member.role = None;
        member.recruitment_role_id = None;
        ctx.db.party_member().id().update(member);
    }
    ctx.db.party_recruitment_role().id().delete(role_id);
    Ok(())
}

#[reducer]
pub fn save_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    name: String,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, owner_id)?;
    crate::character::require_living_character(ctx, owner_id)?;
    if ctx.db.character().id().find(owner_id).is_none() {
        return Err("Character not found".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("Saved roles must have a name".into());
    }
    validate_recruitment_requirements(requirements, weapon_precision)?;
    ctx.db
        .saved_recruitment_role()
        .insert(SavedRecruitmentRole {
            id: 0,
            owner_character_id: owner_id,
            name: name.to_string(),
            requirements,
            weapon_precision,
        });
    Ok(())
}

#[reducer]
pub fn rename_saved_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    role_id: u64,
    name: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, owner_id)?;
    crate::character::require_living_character(ctx, owner_id)?;
    let mut role = ctx
        .db
        .saved_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Saved role not found")?;
    if role.owner_character_id != owner_id {
        return Err("Cannot rename another character's saved role".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("Saved roles must have a name".into());
    }
    role.name = name.to_string();
    ctx.db.saved_recruitment_role().id().update(role);
    Ok(())
}

fn validate_recruitment_requirements(
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
) -> Result<(), String> {
    if !(0.0..=adventuresim_core::capability::WEAPON_PRECISION_RAPIER).contains(&weapon_precision)
        || (weapon_precision * 2.0).fract() != 0.0
    {
        return Err("Weapon precision must use a 0.5 step between 0 and 2".into());
    }
    if [requirements.athletics, requirements.endurance]
        .iter()
        .any(|value| *value > 5)
    {
        return Err("Role ratings must be between 0 and 5".into());
    }
    Ok(())
}

#[reducer]
pub fn update_party_check_targets(
    ctx: &ReducerContext,
    leader_id: u64,
    medicine: f32,
    command: f32,
    religion: f32,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    if [medicine, command, religion]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=5.0).contains(&value) || value.fract() != 0.0)
    {
        return Err("Party check targets must be whole numbers between 0 and 5".into());
    }
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can configure party checks".into());
    }
    party.medicine_target = medicine;
    party.command_target = command;
    party.religion_target = religion;
    ctx.db.party_authority().id().update(party);
    Ok(())
}

#[reducer]
pub fn delete_saved_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    role_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, owner_id)?;
    crate::character::require_living_character(ctx, owner_id)?;
    let role = ctx
        .db
        .saved_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Saved role not found")?;
    if role.owner_character_id != owner_id {
        return Err("Cannot delete another character's saved role".into());
    }
    ctx.db.saved_recruitment_role().id().delete(role_id);
    Ok(())
}

fn filled_role_slots(ctx: &ReducerContext, role_id: u64) -> u32 {
    ctx.db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .count() as u32
}

fn role_requirements(
    role: &PartyRecruitmentRole,
) -> adventuresim_core::capability::RoleRequirements {
    let mut requirements = adventuresim_core::capability::RoleRequirements::from(role.requirements);
    requirements.weapon_precision = requirements.weapon_precision.max(role.weapon_precision);
    requirements.medicine = 0;
    requirements.surgery = 0;
    requirements.command = 0;
    requirements.religion = 0;
    requirements
}

fn require_open_recruitment_offer(
    ctx: &ReducerContext,
    party: &Party,
) -> Result<Option<RecruitmentOffer>, String> {
    let Some(leader) = ctx.db.character().id().find(party.leader_id) else {
        return Err("Recruiting party leader not found".into());
    };
    let offer = ctx
        .db
        .recruitment_offer()
        .recruiting_party_id()
        .find(&party.id);
    let Some(mut offer) = offer else {
        return if leader.temporary {
            Err("NPC company has no recruitment authority".into())
        } else {
            Ok(None)
        };
    };
    let now = crate::time::refresh_clock(ctx)?;
    if offer.status != RecruitmentOfferStatus::Open {
        return Err("This recruitment offer is no longer open".into());
    }
    let npc = ctx.db.settlement_npc().id().find(&offer.settlement_npc_id);
    let presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&offer.settlement_npc_id);
    let bindings_are_live = offer.leader_id == party.leader_id
        && leader.party_id.as_deref() == Some(party.id.as_str())
        && party.current_settlement_id.as_deref() == Some(offer.settlement_id.as_str())
        && leader.current_settlement_id.as_deref() == Some(offer.settlement_id.as_str())
        && npc.is_some_and(|npc| npc.home_settlement_id == offer.settlement_id)
        && presence.is_some_and(|presence| {
            presence.settlement_id == offer.settlement_id
                && presence.location_id == offer.location_id
                && crate::settlement_population::npc_is_present(&presence, now)
        });
    let refreshed = refreshed_recruitment_offer_status(
        offer.status,
        now,
        offer.expires_at_minute,
        bindings_are_live,
    );
    if refreshed != RecruitmentOfferStatus::Open {
        offer.status = refreshed;
        ctx.db.recruitment_offer().id_key().update(offer);
        return Err(if refreshed == RecruitmentOfferStatus::Expired {
            "This recruitment offer has expired".into()
        } else {
            "Recruiting company's advertised identity or presence is stale".into()
        });
    }
    Ok(Some(offer))
}

fn refreshed_recruitment_offer_status(
    current: RecruitmentOfferStatus,
    now: u64,
    expires_at: u64,
    bindings_are_live: bool,
) -> RecruitmentOfferStatus {
    if current != RecruitmentOfferStatus::Open {
        current
    } else if now >= expires_at {
        RecruitmentOfferStatus::Expired
    } else if !bindings_are_live {
        RecruitmentOfferStatus::Closed
    } else {
        RecruitmentOfferStatus::Open
    }
}

#[reducer]
pub fn request_to_join_party(
    ctx: &ReducerContext,
    character_id: u64,
    recruitment_role_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let current_party_id = character.party_id.clone().ok_or("Character has no party")?;
    let current_party = ctx
        .db
        .party_authority()
        .id()
        .find(&current_party_id)
        .ok_or("Current party not found")?;
    if current_party.leader_id != character_id {
        return Err("Only a party leader may request a party merge".into());
    }
    if current_party.active_contract_id.is_some() {
        return Err("Abandon the current quest before joining another party".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    let party_id = role.party_id.clone();
    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if current_party_id == party_id {
        return Err("Cannot join your own party".into());
    }
    require_open_recruitment_offer(ctx, &party)?;
    if !crate::simulation::same_simulation_scope(ctx, character_id, party.leader_id) {
        return Err("Simulation and ordinary parties cannot merge".into());
    }
    if current_party.current_settlement_id != party.current_settlement_id
        || current_party.current_case_site_id != party.current_case_site_id
    {
        return Err("Parties must be in the same location to merge".into());
    }
    if role.quantity > 0 && filled_role_slots(ctx, role.id) >= role.quantity {
        return Err("Recruitment role is full".into());
    }
    if ctx
        .db
        .party_join_request()
        .character_id()
        .filter(character_id)
        .any(|request| request.recruitment_role_id == recruitment_role_id)
    {
        return Ok(());
    }
    let capabilities = crate::capability::refresh_character_capability(ctx, character_id)?;
    ctx.db.party_join_request().insert(PartyJoinRequest {
        id: 0,
        party_id,
        recruitment_role_id,
        character_id,
        meets_requirements: capabilities.meets(role_requirements(&role)),
    });
    Ok(())
}

#[reducer]
pub fn request_general_party_join(
    ctx: &ReducerContext,
    character_id: u64,
    target_party_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    let role = ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&target_party_id)
        .find(|role| role.quantity == 0 && role.name == "Unassigned")
        .unwrap_or_else(|| {
            ctx.db
                .party_recruitment_role()
                .insert(PartyRecruitmentRole {
                    id: 0,
                    party_id: target_party_id.clone(),
                    name: "Unassigned".into(),
                    requirements: RecruitmentRequirements::default(),
                    quantity: 0,
                    weapon_precision: 0.0,
                })
        });
    request_to_join_party(ctx, character_id, role.id)
}

#[reducer]
pub fn accept_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party_authority().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    let recruitment_offer = require_open_recruitment_offer(ctx, &party)?;
    require_no_unresolved_encounter(ctx, &request.party_id)?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can accept join requests".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(request.recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    if role.quantity > 0 && filled_role_slots(ctx, role.id) >= role.quantity {
        return Err("Recruitment role is full".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(request.character_id)
        .ok_or("Applicant not found")?;
    let source_party_id = character.party_id.clone().ok_or("Applicant has no party")?;
    require_no_unresolved_encounter(ctx, &source_party_id)?;
    let source_party = ctx
        .db
        .party_authority()
        .id()
        .find(&source_party_id)
        .ok_or("Applicant party not found")?;
    if source_party.leader_id != request.character_id {
        return Err("Applicant is no longer their party leader".into());
    }
    if !crate::simulation::same_simulation_scope(ctx, request.character_id, leader_id) {
        return Err("Simulation and ordinary parties cannot merge".into());
    }
    if source_party.active_contract_id.is_some() {
        return Err("Applicant's party must abandon its current quest first".into());
    }
    if source_party.current_settlement_id != party.current_settlement_id
        || source_party.current_case_site_id != party.current_case_site_id
    {
        return Err("Parties must be in the same location to merge".into());
    }

    // Preserve the source party's jointly-owned assets and each member's absolute
    // stake. Combining the ledgers does not dilute either party; only future loot
    // is shared among the newly combined membership.
    for mut entry in ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        if item_is_durable(ctx, &entry.item_id) {
            entry.party_id = request.party_id.clone();
            ctx.db.party_inventory_item().id().update(entry);
        } else {
            add_to_party_inventory(ctx, &request.party_id, &entry.item_id, entry.quantity);
            ctx.db.party_inventory_item().id().delete(entry.id);
        }
    }
    for stake in ctx
        .db
        .party_stake()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        credit_party_stake(ctx, &request.party_id, stake.character_id, stake.value)?;
        ctx.db.party_stake().id().delete(stake.id);
    }
    if let Some(state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&source_party_id)
    {
        credit_party_reserve(ctx, &request.party_id, state.reserve_value)?;
        ctx.db
            .party_inventory_state()
            .party_id()
            .delete(&source_party_id);
    }

    let source_members: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&source_party_id)
        .collect();
    let source_member_ids: Vec<_> = source_members
        .iter()
        .map(|member| member.character_id)
        .collect();
    if source_member_ids.iter().any(|member_id| {
        ctx.db
            .character()
            .id()
            .find(*member_id)
            .is_some_and(|character| !character.alive)
    }) {
        return Err("A party containing dead members cannot merge".into());
    }
    for member in source_members {
        ctx.db.party_member().id().delete(member.id);
        ctx.db.party_member().insert(PartyMember {
            id: 0,
            party_id: request.party_id.clone(),
            character_id: member.character_id,
            role: if member.character_id == request.character_id {
                Some(role.name.clone())
            } else {
                member.role
            },
            recruitment_role_id: (member.character_id == request.character_id).then_some(role.id),
        });
        if let Some(mut source_character) = ctx.db.character().id().find(member.character_id) {
            source_character.party_id = Some(request.party_id.clone());
            source_character.current_settlement_id = party.current_settlement_id.clone();
            ctx.db.character().id().update(source_character);
            crate::investigation::set_character_case_site(
                ctx,
                member.character_id,
                party.current_case_site_id.clone().map(|id| id.value),
            );
            crate::social::reset_familiarity_after_join(ctx, member.character_id);
        }
    }

    // Incoming applications and recruitment roles belonged to the source party,
    // so they cannot survive after its leader relinquishes command.
    for source_role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        for pending in ctx
            .db
            .party_join_request()
            .recruitment_role_id()
            .filter(source_role.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
        ctx.db.party_recruitment_role().id().delete(source_role.id);
    }
    for member_id in &source_member_ids {
        for pending in ctx
            .db
            .party_join_request()
            .character_id()
            .filter(*member_id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
    }
    ctx.db.party_authority().id().delete(&source_party_id);
    for old_vote in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_leader_vote().id().delete(&old_vote.id);
    }
    for member_id in &source_member_ids {
        put_leader_vote(ctx, &request.party_id, *member_id, party.leader_id);
    }
    if party.is_solo {
        let mut party = party;
        party.is_solo = false;
        ctx.db.party_authority().id().update(party);
    }
    let requests: Vec<_> = ctx
        .db
        .party_join_request()
        .character_id()
        .filter(request.character_id)
        .collect();
    for pending in requests {
        ctx.db.party_join_request().id().delete(pending.id);
    }
    if role.quantity > 0 && filled_role_slots(ctx, role.id) >= role.quantity {
        for pending in ctx
            .db
            .party_join_request()
            .recruitment_role_id()
            .filter(role.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
    }
    if let Some(mut offer) = recruitment_offer {
        let has_open_role = ctx
            .db
            .party_recruitment_role()
            .party_id()
            .filter(&request.party_id)
            .any(|candidate| {
                candidate.quantity == 0 || filled_role_slots(ctx, candidate.id) < candidate.quantity
            });
        if !has_open_role {
            offer.status = RecruitmentOfferStatus::Closed;
            ctx.db.recruitment_offer().id_key().update(offer);
        }
    }
    normalize_and_elect_party_leader(ctx, &request.party_id)?;
    Ok(())
}

#[reducer]
pub fn reject_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, leader_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party_authority().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can reject join requests".into());
    }
    ctx.db.party_join_request().id().delete(request_id);
    Ok(())
}

/// Transfer a stack of items between two members of the same party.
#[reducer]
pub fn transfer_party_item(
    ctx: &ReducerContext,
    from_character_id: u64,
    to_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, from_character_id)?;
    crate::character::require_living_character(ctx, from_character_id)?;
    crate::character::require_living_character(ctx, to_character_id)?;
    if quantity == 0 || from_character_id == to_character_id {
        return Err("Transfer quantity must be positive and between different characters".into());
    }
    let Some(from) = ctx.db.character().id().find(from_character_id) else {
        return Err("Source character not found".into());
    };
    let Some(to) = ctx.db.character().id().find(to_character_id) else {
        return Err("Recipient character not found".into());
    };
    if from.party_id.is_none() || from.party_id != to.party_id {
        return Err("Characters must belong to the same party".into());
    }
    let Some(source_item) = ctx.db.inventory_item().id().find(inventory_item_id) else {
        return Err("Inventory item not found".into());
    };
    if source_item.character_id != from_character_id || source_item.quantity < quantity {
        return Err("Source character does not have that quantity".into());
    }
    if ctx
        .db
        .character_equip()
        .character_id()
        .find(from_character_id)
        .is_some_and(|equip| equip.is_equiped(inventory_item_id).is_some())
    {
        return Err("Unequip an item before transferring it".into());
    }

    let durable = item_is_durable(ctx, &source_item.item_id);
    if durable {
        if quantity != 1 || source_item.quantity != 1 {
            return Err("Equipment instances must be transferred individually".into());
        }
        let mut transferred = source_item;
        transferred.character_id = to_character_id;
        ctx.db.inventory_item().id().update(transferred);
        return Ok(());
    }

    let food = ctx
        .db
        .item()
        .id()
        .find(&source_item.item_id)
        .is_some_and(|row| row.kind == crate::ItemKind::Food)
        || adventuresim_core::food::definition(&source_item.item_id).is_some();
    if food {
        if source_item.quantity == quantity {
            let mut moved = source_item;
            moved.character_id = to_character_id;
            ctx.db.inventory_item().id().update(moved);
        } else {
            let original_quantity = source_item.quantity;
            let item_id = source_item.item_id.clone();
            let mut remaining = source_item;
            remaining.quantity -= quantity;
            ctx.db.inventory_item().id().update(remaining);
            let destination = ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id: to_character_id,
                item_id,
                quantity,
            });
            crate::food::split_lot(
                ctx,
                inventory_item_id,
                destination.id,
                quantity,
                original_quantity,
            )?;
        }
        crate::capability::refresh_character_capability(ctx, from_character_id)?;
        crate::capability::refresh_character_capability(ctx, to_character_id)?;
        return Ok(());
    }

    let destination_item = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((to_character_id, &source_item.item_id))
        .next();
    let merged_quantity = destination_item
        .as_ref()
        .and_then(|destination| destination.quantity.checked_add(quantity));

    if source_item.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory_item_id);
    } else {
        let mut updated = source_item.clone();
        updated.quantity -= quantity;
        ctx.db.inventory_item().id().update(updated);
    }
    if let (Some(mut destination_item), Some(merged_quantity)) = (destination_item, merged_quantity)
    {
        destination_item.quantity = merged_quantity;
        ctx.db.inventory_item().id().update(destination_item);
    } else {
        ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id: to_character_id,
            item_id: source_item.item_id,
            quantity,
        });
    }
    Ok(())
}

/// Permanently removes staged quantities from a character's unequipped inventory.
fn objective_item_value(ctx: &ReducerContext, item_id: &str) -> Result<u64, String> {
    ctx.db
        .item()
        .id()
        .find(&item_id.to_string())
        .and_then(|item| item.base_value)
        .map(u64::from)
        .ok_or_else(|| format!("Item {item_id} has no objective value"))
}

fn food_lot_value(value: f32) -> Result<u64, String> {
    if !value.is_finite() || value < 0.0 {
        return Err("Food lot has invalid value".into());
    }
    Ok(value.floor() as u64)
}

fn personal_inventory_value(
    ctx: &ReducerContext,
    inventory: &InventoryItem,
    quantity: u32,
) -> Result<u64, String> {
    if let Some(lot) = crate::food::personal_lot(ctx, inventory.id) {
        if quantity != inventory.quantity {
            return Err("Food batches must move as complete lots".into());
        }
        food_lot_value(lot.total_value)
    } else {
        objective_item_value(ctx, &inventory.item_id)?
            .checked_mul(u64::from(quantity))
            .ok_or_else(|| "Party asset liquidation line value overflow".into())
    }
}

fn party_inventory_value(
    ctx: &ReducerContext,
    inventory: &PartyInventoryItem,
    quantity: u32,
) -> Result<u64, String> {
    if let Some(lot) = crate::food::party_lot(ctx, inventory.id) {
        if quantity != inventory.quantity {
            return Err("Food batches must move as complete lots".into());
        }
        food_lot_value(lot.total_value)
    } else {
        objective_item_value(ctx, &inventory.item_id)?
            .checked_mul(u64::from(quantity))
            .ok_or_else(|| "Inventory value overflow".into())
    }
}

fn item_is_durable(ctx: &ReducerContext, item_id: &str) -> bool {
    ctx.db
        .item()
        .id()
        .find(item_id.to_owned())
        .is_some_and(|definition| {
            matches!(
                definition.kind,
                crate::ItemKind::Weapon | crate::ItemKind::Armor | crate::ItemKind::Shield
            )
        })
}

pub(crate) fn add_to_party_inventory(
    ctx: &ReducerContext,
    party_id: &str,
    item_id: &str,
    quantity: u32,
) {
    if quantity == 0 {
        return;
    }
    if ctx
        .db
        .item()
        .id()
        .find(item_id.to_string())
        .is_some_and(|row| row.kind == crate::ItemKind::Food)
        || adventuresim_core::food::definition(item_id).is_some()
    {
        let minute = ctx
            .db
            .party_authority()
            .id()
            .find(&party_id.to_string())
            .and_then(|party| ctx.db.character_time().character_id().find(party.leader_id))
            .map_or(0, |time| time.minutes);
        for _ in 0..quantity {
            let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.into(),
                item_id: item_id.into(),
                quantity: 1,
            });
            crate::food::create_party_food_lot(ctx, row.id, item_id, 1, minute);
        }
        return;
    }
    if item_is_durable(ctx, item_id) {
        for _ in 0..quantity {
            let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.to_string(),
                item_id: item_id.to_string(),
                quantity: 1,
            });
            ctx.db.party_item_condition().insert(PartyItemCondition {
                party_inventory_item_id: row.id,
                tier_1: 0.0,
                tier_2: 0.0,
                tier_3: 0.0,
                tier_4: 0.0,
                tier_5: 0.0,
            });
        }
        return;
    }
    if let Some(mut stack) = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .find(|stack| stack.item_id == item_id)
    {
        if let Some(merged) = stack.quantity.checked_add(quantity) {
            stack.quantity = merged;
            ctx.db.party_inventory_item().id().update(stack);
        } else {
            ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: party_id.to_string(),
                item_id: item_id.to_string(),
                quantity,
            });
        }
    } else {
        ctx.db.party_inventory_item().insert(PartyInventoryItem {
            id: 0,
            party_id: party_id.to_string(),
            item_id: item_id.to_string(),
            quantity,
        });
    }
}

fn credit_party_stake(
    ctx: &ReducerContext,
    party_id: &str,
    character_id: u64,
    value: u64,
) -> Result<(), String> {
    if value == 0 {
        return Ok(());
    }
    if let Some(mut stake) = ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .find(|stake| stake.character_id == character_id)
    {
        stake.value = stake
            .value
            .checked_add(value)
            .ok_or("Party stake overflow")?;
        ctx.db.party_stake().id().update(stake);
    } else {
        ctx.db.party_stake().insert(PartyStake {
            id: 0,
            party_id: party_id.to_string(),
            character_id,
            value,
        });
    }
    Ok(())
}

fn credit_party_reserve(ctx: &ReducerContext, party_id: &str, value: u64) -> Result<(), String> {
    if value == 0 {
        return Ok(());
    }
    if let Some(mut state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
    {
        state.reserve_value = state
            .reserve_value
            .checked_add(value)
            .ok_or("Party reserve overflow")?;
        ctx.db.party_inventory_state().party_id().update(state);
    } else {
        ctx.db.party_inventory_state().insert(PartyInventoryState {
            party_id: party_id.to_string(),
            reserve_value: value,
        });
    }
    Ok(())
}

fn mission_candidate_is_current(
    ctx: &ReducerContext,
    mission: &MissionAuthority,
    candidate: &MissionOutcomeCandidate,
) -> Result<bool, String> {
    if mission.status != MissionAttemptStatus::Bound
        || candidate.mission_id != mission.id
        || candidate.case_id != mission.case_id
        || mission.case_site_id.as_ref() != Some(&candidate.case_site_id)
        || mission.hostile_group_id.as_deref() != Some(&candidate.hostile_group_id)
    {
        return Ok(false);
    }
    let Some(capability) = ctx
        .db
        .mission_approach_capability()
        .id()
        .find(&candidate.capability_id)
    else {
        return Ok(false);
    };
    if !capability.active
        || capability.observer_character_id != mission.observer_character_id
        || capability.case_id != candidate.case_id
        || capability.case_site_id != candidate.case_site_id
        || capability.hostile_group_id != candidate.hostile_group_id
        || capability.path_index != candidate.path_index
        || capability.objective_id != candidate.objective_id
        || capability.resolution != candidate.resolution
        || capability.weight != candidate.weight
        || capability.capture_subject_id != candidate.capture_subject_id
        || capability.capture_custody_version != candidate.capture_custody_version
    {
        return Ok(false);
    }
    let Some(case) = ctx.db.case_authority().id().find(&candidate.case_id) else {
        return Ok(false);
    };
    if case.resolution_status != CaseResolutionStatus::Open {
        return Ok(false);
    }
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| {
            serde_json::from_str::<adventuresim_core::case::OutcomeFact>(&row.fact_json)
                .map_err(|_| "Stored outcome fact is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let core_case_id =
        adventuresim_core::case::CaseId::new(case.id.clone()).map_err(|_| "Case ID is invalid")?;
    let evaluation = expression.evaluate(&core_case_id, &mission.party_id, &facts);
    let Some(path) = expression
        .alternatives
        .get(usize::from(candidate.path_index))
    else {
        return Ok(false);
    };
    let Some(objective_index) = path
        .objectives
        .iter()
        .position(|objective| objective.id.as_str() == candidate.objective_id)
    else {
        return Ok(false);
    };
    if evaluation
        .alternatives
        .get(usize::from(candidate.path_index))
        .and_then(|path| path.get(objective_index))
        .is_none_or(|progress| progress.state != adventuresim_core::case::EvaluationState::Pending)
    {
        return Ok(false);
    }
    if candidate.resolution != HostileResolutionKind::Captured {
        return Ok(true);
    }
    let (Some(subject_id), Some(version)) = (
        candidate.capture_subject_id.as_ref(),
        candidate.capture_custody_version,
    ) else {
        return Ok(false);
    };
    Ok(ctx
        .db
        .case_custody()
        .object_id()
        .find(subject_id)
        .is_some_and(|custody| {
            custody.case_id == candidate.case_id
                && custody.object_kind == CustodyObjectKind::Subject
                && custody.holder_kind == CustodyHolderKind::Site
                && custody.holder_id == candidate.case_site_id.value
                && custody.version == version
        }))
}

fn mission_outcome_draw(mission: &MissionAuthority, candidates: &[MissionOutcomeCandidate]) -> u64 {
    let mut candidates = candidates.to_vec();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let mut hasher = Sha256::new();
    hasher.update(b"adventuresim:strategic-mission-outcome:v1\0");
    hasher.update(mission.party_id.as_bytes());
    hasher.update(mission.outcome_entropy.to_le_bytes());
    for candidate in &candidates {
        hasher.update([0]);
        hasher.update(candidate.id.as_bytes());
        hasher.update([0]);
        hasher.update(candidate.capability_id.as_bytes());
        hasher.update(candidate.weight.to_le_bytes());
        hasher.update([candidate.resolution as u8]);
    }
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes)
}

#[allow(clippy::too_many_arguments)]
fn mission_approach_capability_id(
    observer_character_id: u64,
    case_id: &str,
    site_id: &str,
    hostile_group_id: &str,
    path_index: u16,
    objective_id: &str,
    resolution: HostileResolutionKind,
    capture_subject_id: Option<&str>,
    capture_custody_version: Option<u32>,
) -> String {
    format!(
        "mission-approach:{observer_character_id}:{case_id}:{site_id}:{hostile_group_id}:{path_index}:{objective_id}:{}:{}:{}",
        resolution as u8,
        capture_subject_id.unwrap_or("-"),
        capture_custody_version.map_or_else(|| "-".into(), |version| version.to_string()),
    )
}

fn sample_mission_candidate(
    mission: &MissionAuthority,
    mut candidates: Vec<MissionOutcomeCandidate>,
) -> Option<MissionOutcomeCandidate> {
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let total_weight = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(u64::from(candidate.weight))
    });
    if total_weight == 0 {
        return None;
    }
    let mut draw = mission_outcome_draw(mission, &candidates) % total_weight;
    for candidate in candidates {
        let weight = u64::from(candidate.weight);
        if draw < weight {
            return Some(candidate);
        }
        draw -= weight;
    }
    None
}

/// Commit the strategic meaning of an authenticated successful combat
/// session. The child reports only success; the exact result is sampled here.
pub(crate) fn complete_bound_mission_success(
    ctx: &ReducerContext,
    mission_id: &str,
) -> Result<bool, String> {
    let mut mission = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id.to_string())
        .ok_or("Mission authority not found")?;
    if mission.status == MissionAttemptStatus::Committed {
        return Ok(false);
    }
    if mission.status != MissionAttemptStatus::Bound {
        return Err("Mission attempt is no longer eligible for completion".into());
    }
    let mut candidates = Vec::new();
    for candidate in ctx
        .db
        .mission_outcome_candidate()
        .mission_id()
        .filter(&mission.id)
    {
        if mission_candidate_is_current(ctx, &mission, &candidate)? {
            candidates.push(candidate);
        }
    }
    let Some(selected) = sample_mission_candidate(&mission, candidates) else {
        mission.status = MissionAttemptStatus::Failed;
        ctx.db.mission_authority().id().update(mission);
        return Ok(false);
    };
    let group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&selected.hostile_group_id)
        .ok_or("Bound mission hostile group no longer exists")?;
    if group.disposition != HostileGroupDisposition::Active {
        return Err("Bound hostile group is already resolved".into());
    }
    let dropped_items = if selected.resolution == HostileResolutionKind::Defeated {
        group
            .drop_item_id
            .clone()
            .map(|item| vec![(item, group.drop_quantity)])
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let battle_id = format!("battle:{mission_id}");
    let outcome_source_id = format!("outcome:{mission_id}");
    let committed = commit_hostile_battle_resolution(
        ctx,
        &outcome_source_id,
        &battle_id,
        &mission.party_id,
        Some(mission_id),
        Some(&selected.hostile_group_id),
        selected.resolution,
        selected.capture_subject_id.as_deref(),
        dropped_items,
        selected.resolution == HostileResolutionKind::Defeated,
    )?;
    mission.status = MissionAttemptStatus::Committed;
    mission.committed_resolution = Some(selected.resolution);
    mission.committed_capture_subject_id = selected.capture_subject_id;
    ctx.db.mission_authority().id().update(mission);
    for mut capability in ctx
        .db
        .mission_approach_capability()
        .hostile_group_id()
        .filter(&selected.hostile_group_id)
        .filter(|capability| capability.case_site_id == selected.case_site_id)
        .collect::<Vec<_>>()
    {
        capability.active = false;
        ctx.db.mission_approach_capability().id().update(capability);
    }
    if committed {
        finish_incident_for_hostile_group(ctx, &selected.hostile_group_id)?;
    }
    Ok(committed)
}

pub(crate) fn fail_bound_mission_attempt(
    ctx: &ReducerContext,
    mission_id: &str,
) -> Result<(), String> {
    let Some(mut mission) = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id.to_string())
    else {
        return Ok(());
    };
    match mission.status {
        MissionAttemptStatus::Bound => {
            mission.status = MissionAttemptStatus::Failed;
            ctx.db.mission_authority().id().update(mission);
            Ok(())
        }
        MissionAttemptStatus::Failed => Ok(()),
        MissionAttemptStatus::Committed | MissionAttemptStatus::Cancelled => {
            Err("Conflicting terminal mission retry".into())
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_hostile_battle_resolution(
    ctx: &ReducerContext,
    outcome_source_id: &str,
    battle_id: &str,
    party_id: &str,
    mission_id: Option<&str>,
    hostile_group_id: Option<&str>,
    resolution: HostileResolutionKind,
    capture_subject_id: Option<&str>,
    dropped_items: Vec<(String, u32)>,
    include_random_gold: bool,
) -> Result<bool, String> {
    validate_hostile_resolution_contract(
        None,
        None,
        resolution,
        capture_subject_id,
        !dropped_items.is_empty() || include_random_gold,
    )
    .map_err(str::to_string)?;
    adventuresim_core::mission::OutcomeSourceId::new(outcome_source_id).map_err(str::to_string)?;
    adventuresim_core::mission::BattleId::new(battle_id).map_err(str::to_string)?;
    if let Some(id) = mission_id {
        adventuresim_core::mission::MissionId::new(id).map_err(str::to_string)?;
    }
    if let Some(id) = hostile_group_id {
        adventuresim_core::mission::HostileGroupId::new(id).map_err(str::to_string)?;
    }
    if let Some(existing) = ctx
        .db
        .outcome_source_authority()
        .id()
        .find(&outcome_source_id.to_string())
    {
        return if existing.battle_id == battle_id
            && existing.party_id == party_id
            && existing.mission_id.as_deref() == mission_id
            && existing.hostile_group_id.as_deref() == hostile_group_id
            && existing.resolution == resolution
        {
            Ok(false)
        } else {
            Err("Conflicting retry for strategic battle outcome source".into())
        };
    }
    let group = hostile_group_id
        .map(|id| {
            ctx.db
                .hostile_group_authority()
                .id()
                .find(&id.to_string())
                .ok_or_else(|| "Hostile group not found".to_string())
        })
        .transpose()?;
    if let Some(mission_id) = mission_id {
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&mission_id.to_string())
            .ok_or("Mission authority not found")?;
        if mission.party_id != party_id || mission.hostile_group_id.as_deref() != hostile_group_id {
            return Err("Battle attribution does not match mission authority".into());
        }
        if mission.status != MissionAttemptStatus::Bound {
            return Err("Mission attempt is not bound for strategic completion".into());
        }
        let mut candidates = Vec::new();
        for candidate in ctx
            .db
            .mission_outcome_candidate()
            .mission_id()
            .filter(&mission.id)
        {
            if mission_candidate_is_current(ctx, &mission, &candidate)? {
                candidates.push(candidate);
            }
        }
        let selected = sample_mission_candidate(&mission, candidates)
            .ok_or("Mission has no current strategic outcome candidate")?;
        if selected.resolution != resolution
            || selected.capture_subject_id.as_deref() != capture_subject_id
        {
            return Err("Strategic result is not an exact current mission candidate".into());
        }
    }
    ctx.db
        .outcome_source_authority()
        .insert(OutcomeSourceAuthority {
            id: outcome_source_id.to_string(),
            battle_id: battle_id.to_string(),
            mission_id: mission_id.map(str::to_string),
            hostile_group_id: hostile_group_id.map(str::to_string),
            resolution,
            party_id: party_id.to_string(),
        });
    ctx.db.battle_result().insert(BattleResult {
        battle_id: battle_id.to_string(),
        party_id: party_id.to_string(),
    });
    if let Some(mission_id) = mission_id {
        let mission = ctx
            .db
            .mission_authority()
            .id()
            .find(&mission_id.to_string())
            .ok_or("Mission authority not found")?;
        if let Some(site_id) = mission.case_site_id {
            let site = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&site_id.value)
                .ok_or("Case site authority not found")?;
            ctx.db
                .backend_case_battle_authority()
                .insert(BackendCaseBattle {
                    gateway_bucket: 0,
                    case_id: site.case_id,
                    party_id: party_id.to_string(),
                    battle_id: battle_id.to_string(),
                    mission_id: mission_id.to_string(),
                });
        }
    }
    let difficulty = group.as_ref().map_or(1, |group| group.difficulty);
    for member_id in living_party_member_ids(ctx, party_id) {
        ctx.db.battle_participant().insert(BattleParticipant {
            id: 0,
            participant_battle_id: battle_id.to_string(),
            character_id: member_id,
        });
        crate::condition::record_morale_event(
            ctx,
            member_id,
            "victory",
            5.0 + difficulty.max(0) as f32,
            Some(outcome_source_id.to_string()),
        )?;
    }
    let mut combined: HashMap<String, u32> = HashMap::new();
    for (item_id, quantity) in dropped_items {
        if quantity > 0 && ctx.db.item().id().find(&item_id).is_some() {
            *combined.entry(item_id).or_default() = combined
                .get(&item_id)
                .copied()
                .unwrap_or_default()
                .saturating_add(quantity);
        }
    }
    if include_random_gold && ctx.random::<u64>().is_multiple_of(2) {
        let maximum_gold = difficulty.max(1) as u32 * 10;
        let gold = 1 + (ctx.random::<u64>() % u64::from(maximum_gold)) as u32;
        if gold > 0
            && let Some(group) = &group
            && let Some(site) = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&group.case_site_id.value)
        {
            *combined
                .entry(crate::item::currency_id_for_settlement(
                    ctx,
                    &site.origin_settlement_id,
                )?)
                .or_default() += gold;
        }
    }
    for (item_id, quantity) in combined {
        ctx.db.battle_loot_item().insert(BattleLootItem {
            id: 0,
            loot_battle_id: battle_id.to_string(),
            item_id,
            quantity,
        });
    }
    if let Some(mut group) = group {
        group.disposition = match resolution {
            HostileResolutionKind::Defeated => HostileGroupDisposition::Defeated,
            HostileResolutionKind::DrivenOff => HostileGroupDisposition::DrivenOff,
            HostileResolutionKind::Captured => HostileGroupDisposition::Captured,
            HostileResolutionKind::CaptureTargetKilled => unreachable!(),
        };
        ctx.db.hostile_group_authority().id().update(group.clone());
        match resolution {
            HostileResolutionKind::Defeated => ingest_hostile_group_defeat_fact(
                ctx,
                outcome_source_id,
                party_id,
                &group,
                group.enemy_count,
            )?,
            HostileResolutionKind::DrivenOff => {
                let site = ctx
                    .db
                    .case_site_authority()
                    .id_key()
                    .find(&group.case_site_id.value)
                    .ok_or("Hostile group case site not found")?;
                ingest_case_outcome_fact(
                    ctx,
                    &format!("{outcome_source_id}:drive-off"),
                    &site.case_id,
                    party_id,
                    adventuresim_core::case::OutcomeFactKind::HostilesDrivenOff {
                        hostile_group_id: group.id.clone(),
                    },
                )?;
            }
            HostileResolutionKind::Captured => {
                let subject_id =
                    capture_subject_id.ok_or("Capture result has no mission-bound subject")?;
                let current = ctx
                    .db
                    .case_custody()
                    .object_id()
                    .find(&subject_id.to_string())
                    .ok_or("Captured subject has no custody authority")?;
                if current.case_id
                    != ctx
                        .db
                        .case_site_authority()
                        .id_key()
                        .find(&group.case_site_id.value)
                        .ok_or("Hostile group case site not found")?
                        .case_id
                    || current.holder_kind != CustodyHolderKind::Site
                    || current.holder_id != group.case_site_id.value
                {
                    return Err("Capture subject is not bound to this mission site and case".into());
                }
                transition_case_custody(
                    ctx,
                    &format!("{outcome_source_id}:capture"),
                    &current.case_id,
                    party_id,
                    CustodyObjectKind::Subject,
                    subject_id,
                    CustodyHolderKind::Party,
                    party_id,
                    current.version.saturating_add(1),
                    Some(adventuresim_core::case::OutcomeFactKind::SubjectCaptured {
                        subject_id: adventuresim_core::case::SubjectId::new(subject_id)
                            .map_err(|_| "Capture subject ID is invalid")?,
                    }),
                )?;
            }
            HostileResolutionKind::CaptureTargetKilled => unreachable!(),
        }
    }
    Ok(true)
}

#[reducer]
pub fn store_battle_loot(
    ctx: &ReducerContext,
    character_id: u64,
    battle_id: String,
    loot_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    adventuresim_core::mission::BattleId::new(battle_id.clone()).map_err(str::to_string)?;
    crate::character::require_living_character(ctx, character_id)?;
    if loot_item_ids.len() != quantities.len() {
        return Err("Loot entries must be aligned".into());
    }
    if loot_item_ids.iter().copied().collect::<HashSet<_>>().len() != loot_item_ids.len() {
        return Err("Duplicate battle loot IDs are not allowed".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let result = ctx
        .db
        .battle_result()
        .battle_id()
        .find(&battle_id)
        .ok_or("Battle result not found")?;
    if result.party_id != party_id {
        return Err("Battle loot belongs to another party".into());
    }
    let available: Vec<_> = ctx
        .db
        .battle_loot_item()
        .loot_battle_id()
        .filter(&battle_id)
        .collect();
    let loot: Vec<_> = if loot_item_ids.is_empty() {
        available
    } else {
        loot_item_ids
            .iter()
            .zip(&quantities)
            .map(|(id, quantity)| {
                let mut entry = available
                    .iter()
                    .find(|entry| entry.id == *id)
                    .cloned()
                    .ok_or("Loot item not found")?;
                if *quantity == 0 || *quantity > entry.quantity {
                    return Err("Invalid loot quantity".into());
                }
                entry.quantity = *quantity;
                Ok(entry)
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    let mut total_value = 0_u64;
    for entry in &loot {
        let entry_value = objective_item_value(ctx, &entry.item_id)?
            .checked_mul(u64::from(entry.quantity))
            .ok_or("Battle loot value overflow")?;
        total_value = total_value
            .checked_add(entry_value)
            .ok_or("Battle loot value overflow")?;
    }
    let recorded_participants: Vec<_> = ctx
        .db
        .battle_participant()
        .participant_battle_id()
        .filter(&battle_id)
        .map(|participant| participant.character_id)
        .collect();
    let living_recorded: Vec<_> = recorded_participants
        .iter()
        .copied()
        .filter(|participant_id| {
            ctx.db
                .character()
                .id()
                .find(*participant_id)
                .is_some_and(|character| character.alive)
        })
        .collect();
    let participants = adventuresim_core::battle_rewards::living_participant_ids(
        &recorded_participants,
        &living_recorded,
    );
    if participants.is_empty() {
        return Err("Battle has no eligible participants".into());
    }
    for entry in loot {
        add_to_party_inventory(ctx, &party_id, &entry.item_id, entry.quantity);
        let original = ctx
            .db
            .battle_loot_item()
            .id()
            .find(entry.id)
            .ok_or("Battle loot changed during transfer")?;
        if original.quantity == entry.quantity {
            ctx.db.battle_loot_item().id().delete(entry.id);
        } else {
            let mut original = original;
            original.quantity = original
                .quantity
                .checked_sub(entry.quantity)
                .ok_or("Battle loot quantity underflow")?;
            ctx.db.battle_loot_item().id().update(original);
        }
    }
    let participant_count = participants.len() as u64;
    let share = total_value / participant_count;
    for participant_id in participants {
        credit_party_stake(ctx, &party_id, participant_id, share)?;
    }
    credit_party_reserve(ctx, &party_id, total_value % participant_count)?;
    Ok(())
}

#[reducer]
pub fn deposit_party_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let mut inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Inventory item not found")?;
    if quantity == 0 || inventory.character_id != character_id || inventory.quantity < quantity {
        return Err("Invalid party inventory deposit".into());
    }
    if ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .is_some_and(|equip| equip.is_equiped(inventory_item_id).is_some())
    {
        return Err("Unequip an item before depositing it".into());
    }
    let value = personal_inventory_value(ctx, &inventory, quantity)?;
    let durable = item_is_durable(ctx, &inventory.item_id);
    if durable && (quantity != 1 || inventory.quantity != 1) {
        return Err("Equipment instances must be deposited individually".into());
    }
    let preserved_condition = if durable {
        ctx.db
            .item_condition()
            .inventory_item_id()
            .find(inventory.id)
    } else {
        None
    };
    let food = crate::food::personal_lot(ctx, inventory.id).is_some();
    if food {
        let party_row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
            id: 0,
            party_id: party_id.clone(),
            item_id: inventory.item_id.clone(),
            quantity,
        });
        crate::food::move_or_split_to_party(
            ctx,
            inventory.id,
            party_row.id,
            quantity,
            inventory.quantity,
        )?;
    } else {
        add_to_party_inventory(ctx, &party_id, &inventory.item_id, quantity);
    }
    if let Some(condition) = preserved_condition {
        let party_row = ctx
            .db
            .party_inventory_item()
            .party_id()
            .filter(&party_id)
            .filter(|row| row.item_id == inventory.item_id)
            .max_by_key(|row| row.id)
            .expect("durable party row was just inserted");
        ctx.db
            .party_item_condition()
            .party_inventory_item_id()
            .update(PartyItemCondition {
                party_inventory_item_id: party_row.id,
                tier_1: condition.tier_1,
                tier_2: condition.tier_2,
                tier_3: condition.tier_3,
                tier_4: condition.tier_4,
                tier_5: condition.tier_5,
            });
        ctx.db
            .item_condition()
            .inventory_item_id()
            .delete(inventory.id);
    }
    credit_party_stake(ctx, &party_id, character_id, value)?;
    if inventory.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory.id);
    } else {
        inventory.quantity -= quantity;
        ctx.db.inventory_item().id().update(inventory);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

pub(crate) fn consume_personal_gold(
    ctx: &ReducerContext,
    character_id: u64,
    amount: u64,
) -> Result<(), String> {
    crate::item::consume_personal_currency(ctx, character_id, amount)
}

pub(crate) fn party_currency_total(ctx: &ReducerContext, party_id: &str) -> u64 {
    ctx.db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .map(|stack| u64::from(stack.quantity))
        .sum()
}

pub(crate) fn consume_party_currency(
    ctx: &ReducerContext,
    party_id: &str,
    amount: u64,
) -> Result<(), String> {
    let mut stacks: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    if stacks
        .iter()
        .map(|stack| u64::from(stack.quantity))
        .sum::<u64>()
        < amount
    {
        return Err("Not enough party coin to cover this payment".into());
    }
    stacks.sort_by(|a, b| (&a.item_id, a.id).cmp(&(&b.item_id, b.id)));
    let mut remaining = amount;
    for mut stack in stacks {
        let taken = remaining.min(u64::from(stack.quantity)) as u32;
        stack.quantity -= taken;
        remaining -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.party_inventory_item().id().delete(stack.id);
        } else {
            ctx.db.party_inventory_item().id().update(stack);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

pub(crate) fn credit_party_currency(
    ctx: &ReducerContext,
    party_id: &str,
    settlement_id: &str,
    amount: u32,
) -> Result<(), String> {
    let currency_id = crate::item::currency_id_for_settlement(ctx, settlement_id)?;
    add_to_party_inventory(ctx, party_id, &currency_id, amount);
    Ok(())
}

fn transfer_personal_currency_to_party(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    amount: u64,
) -> Result<(), String> {
    let mut stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    if stacks.iter().map(|s| u64::from(s.quantity)).sum::<u64>() < amount {
        return Err("Not enough personal coin".into());
    }
    stacks.sort_by(|a, b| (&a.item_id, a.id).cmp(&(&b.item_id, b.id)));
    let mut remaining = amount;
    for mut stack in stacks {
        let taken = remaining.min(u64::from(stack.quantity)) as u32;
        add_to_party_inventory(ctx, party_id, &stack.item_id, taken);
        stack.quantity -= taken;
        remaining -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

fn transfer_party_currency_to_personal(
    ctx: &ReducerContext,
    party_id: &str,
    character_id: u64,
    amount: u64,
) -> Result<(), String> {
    let mut stacks: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    if stacks.iter().map(|s| u64::from(s.quantity)).sum::<u64>() < amount {
        return Err("The party has insufficient coin".into());
    }
    stacks.sort_by(|a, b| (&a.item_id, a.id).cmp(&(&b.item_id, b.id)));
    let mut remaining = amount;
    for mut stack in stacks {
        let taken = remaining.min(u64::from(stack.quantity)) as u32;
        crate::add_inventory_item(ctx, character_id, &stack.item_id, taken);
        stack.quantity -= taken;
        remaining -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.party_inventory_item().id().delete(stack.id);
        } else {
            ctx.db.party_inventory_item().id().update(stack);
        }
        if remaining == 0 {
            break;
        }
    }
    Ok(())
}

#[reducer]
pub fn withdraw_party_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    party_inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let mut inventory = ctx
        .db
        .party_inventory_item()
        .id()
        .find(party_inventory_item_id)
        .ok_or("Party inventory item not found")?;
    if quantity == 0 || inventory.party_id != party_id || inventory.quantity < quantity {
        return Err("Invalid party inventory withdrawal".into());
    }
    let cost = party_inventory_value(ctx, &inventory, quantity)?;
    let mut stake = ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .find(|stake| stake.character_id == character_id);
    let stake_value = stake.as_ref().map_or(0, |stake| stake.value);
    if cost > stake_value {
        let top_up = cost - stake_value;
        transfer_personal_currency_to_party(ctx, character_id, &party_id, top_up)?;
    }
    if let Some(ref mut stake) = stake {
        stake.value = stake.value.saturating_sub(cost);
        ctx.db.party_stake().id().update(stake.clone());
    }
    let durable = item_is_durable(ctx, &inventory.item_id);
    if durable && (quantity != 1 || inventory.quantity != 1) {
        return Err("Equipment instances must be withdrawn individually".into());
    }
    let preserved_condition = ctx
        .db
        .party_item_condition()
        .party_inventory_item_id()
        .find(inventory.id);
    let food = crate::food::party_lot(ctx, inventory.id).is_some();
    let new_inventory_id = if food {
        let row = ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id,
            item_id: inventory.item_id.clone(),
            quantity,
        });
        crate::food::move_or_split_to_personal(
            ctx,
            inventory.id,
            row.id,
            quantity,
            inventory.quantity,
        )?;
        Some(row.id)
    } else {
        crate::add_inventory_item(ctx, character_id, &inventory.item_id, quantity)
    };
    if let (Some(condition), Some(new_id)) = (preserved_condition, new_inventory_id) {
        ctx.db
            .item_condition()
            .inventory_item_id()
            .update(crate::repair::ItemCondition {
                inventory_item_id: new_id,
                tier_1: condition.tier_1,
                tier_2: condition.tier_2,
                tier_3: condition.tier_3,
                tier_4: condition.tier_4,
                tier_5: condition.tier_5,
            });
        ctx.db
            .party_item_condition()
            .party_inventory_item_id()
            .delete(inventory.id);
    }
    if inventory.quantity == quantity {
        ctx.db.party_inventory_item().id().delete(inventory.id);
    } else {
        inventory.quantity -= quantity;
        ctx.db.party_inventory_item().id().update(inventory);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn liquidate_party_inventory(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    party_inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if party_inventory_item_ids.is_empty() || party_inventory_item_ids.len() != quantities.len() {
        return Err("Liquidation entries must be non-empty and aligned".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Character must be at this settlement to liquidate party assets".into());
    }
    if require_settlement_service(
        ctx,
        &settlement_id,
        adventuresim_world_schema::SettlementService::Market,
    )
    .is_err()
    {
        require_settlement_service(
            ctx,
            &settlement_id,
            adventuresim_world_schema::SettlementService::GeneralStore,
        )?;
    }
    let party_id = character.party_id.ok_or("Character has no party")?;
    let mut staged = Vec::new();
    let mut proceeds = 0_u64;
    let mut seen = HashSet::new();
    for (&id, &quantity) in party_inventory_item_ids.iter().zip(&quantities) {
        if !seen.insert(id) {
            return Err("Party liquidation item IDs must be unique".into());
        }
        let entry = ctx
            .db
            .party_inventory_item()
            .id()
            .find(id)
            .ok_or("Party inventory item not found")?;
        if quantity == 0
            || entry.party_id != party_id
            || entry.quantity < quantity
            || crate::item::is_currency(ctx, &entry.item_id)
        {
            return Err("Invalid party asset liquidation".into());
        }
        let line_value = party_inventory_value(ctx, &entry, quantity)?;
        proceeds = proceeds
            .checked_add(line_value)
            .ok_or("Party asset liquidation total overflow")?;
        staged.push((entry, quantity));
    }
    let proceeds =
        u32::try_from(proceeds).map_err(|_| "Party asset liquidation exceeds currency limits")?;
    for (mut entry, quantity) in staged {
        let is_food = crate::food::party_lot(ctx, entry.id).is_some();
        if is_food {
            crate::food::remove_party_lot_quantity(ctx, entry.id, quantity, entry.quantity)?;
        }
        if entry.quantity == quantity {
            ctx.db.party_inventory_item().id().delete(entry.id);
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(entry.id);
        } else {
            entry.quantity -= quantity;
            ctx.db.party_inventory_item().id().update(entry);
        }
    }
    credit_party_currency(ctx, &party_id, &settlement_id, proceeds)?;
    Ok(())
}

#[reducer]
pub fn discard_inventory_items(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    if inventory_item_ids.is_empty() || inventory_item_ids.len() != quantities.len() {
        return Err("Discarded item IDs and quantities must be non-empty and aligned".into());
    }
    if ctx.db.character().id().find(character_id).is_none() {
        return Err("Character not found".into());
    }
    let equip = ctx.db.character_equip().character_id().find(character_id);
    let mut seen = HashSet::new();
    let mut staged = Vec::with_capacity(inventory_item_ids.len());
    for (&inventory_item_id, &quantity) in inventory_item_ids.iter().zip(&quantities) {
        if quantity == 0 || !seen.insert(inventory_item_id) {
            return Err("Discard quantities must be positive and item IDs unique".into());
        }
        let item = ctx
            .db
            .inventory_item()
            .id()
            .find(inventory_item_id)
            .ok_or("Inventory item not found")?;
        if item.character_id != character_id || item.quantity < quantity {
            return Err("Character does not have the staged quantity".into());
        }
        if equip
            .as_ref()
            .is_some_and(|equip| equip.is_equiped(inventory_item_id).is_some())
        {
            return Err("Unequip an item before discarding it".into());
        }
        staged.push((item, quantity));
    }

    for (mut item, quantity) in staged {
        if item.quantity == quantity {
            ctx.db.inventory_item().id().delete(item.id);
            ctx.db.item_condition().inventory_item_id().delete(item.id);
            crate::food::delete_personal_food_lot(ctx, item.id);
        } else {
            crate::food::remove_lot_quantity(ctx, item.id, quantity, item.quantity)?;
            item.quantity -= quantity;
            ctx.db.inventory_item().id().update(item);
        }
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn finalize_party_offer(
    ctx: &ReducerContext,
    from_character_ids: Vec<u64>,
    to_character_ids: Vec<u64>,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    for character_id in from_character_ids.iter().chain(&to_character_ids) {
        require_character_no_unresolved_encounter(ctx, *character_id)?;
        crate::character::require_living_character(ctx, *character_id)?;
    }
    if from_character_ids.len() != to_character_ids.len()
        || from_character_ids.len() != inventory_item_ids.len()
        || from_character_ids.len() != quantities.len()
        || from_character_ids.is_empty()
    {
        return Err("Offer entries must be non-empty and aligned".into());
    }
    for index in 0..from_character_ids.len() {
        let from_id = from_character_ids[index];
        let to_id = to_character_ids[index];
        let quantity = quantities[index];
        let Some(from) = ctx.db.character().id().find(from_id) else {
            return Err("Source character not found".into());
        };
        let Some(to) = ctx.db.character().id().find(to_id) else {
            return Err("Recipient character not found".into());
        };
        let Some(item) = ctx.db.inventory_item().id().find(inventory_item_ids[index]) else {
            return Err("Inventory item not found".into());
        };
        if quantity == 0
            || from_id == to_id
            || from.party_id.is_none()
            || from.party_id != to.party_id
            || item.character_id != from_id
            || item.quantity < quantity
        {
            return Err("Invalid party trade offer".into());
        }
        if ctx
            .db
            .character_equip()
            .character_id()
            .find(from_id)
            .is_some_and(|equip| equip.is_equiped(item.id).is_some())
        {
            return Err("Unequip an item before offering it".into());
        }
    }
    for index in 0..from_character_ids.len() {
        transfer_party_item(
            ctx,
            from_character_ids[index],
            to_character_ids[index],
            inventory_item_ids[index],
            quantities[index],
        )?;
    }
    Ok(())
}

#[reducer]
pub fn finalize_merchant_trade(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    buy_item_ids: Vec<String>,
    buy_quantities: Vec<u32>,
    sell_inventory_ids: Vec<u64>,
    sell_quantities: Vec<u32>,
    party_scope: bool,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if require_settlement_service(
        ctx,
        &settlement_id,
        adventuresim_world_schema::SettlementService::Market,
    )
    .is_err()
    {
        require_settlement_service(
            ctx,
            &settlement_id,
            adventuresim_world_schema::SettlementService::GeneralStore,
        )?;
    }
    if buy_item_ids.len() != buy_quantities.len()
        || sell_inventory_ids.len() != sell_quantities.len()
    {
        return Err("Trade entries must be aligned".into());
    }
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    if character.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Character must be at this settlement to trade".into());
    }
    let party_id = character.party_id.clone();
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Settlement not found")?;
    let speaker = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?
        .oral_languages;
    let mut merchant = adventuresim_world_schema::OralLanguageHours::default();
    *merchant.direct_mut(settlement.languages.dominant_german()) =
        adventuresim_world_schema::ORAL_FLUENCY_HOURS;
    let (_, shared_language) =
        adventuresim_world_schema::best_common_oral_language(speaker, merchant);
    let settlement_economy = settlement.economy.clone();
    let problem_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let problem_effects =
        crate::local_problem::settlement_effects(ctx, &settlement_id, problem_minute);
    // Sales are inventory-instance operations. Preserve each submitted stack
    // and quantity rather than netting by item ID, which can assign the whole
    // net sale to every matching stack.
    let mut seen_sale_ids = HashSet::new();
    if !sell_inventory_ids
        .iter()
        .all(|inventory_id| seen_sale_ids.insert(*inventory_id))
    {
        return Err("Merchant sale inventory IDs must be unique".into());
    }
    let mut cost = 0_u64;
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        let Some(item) = ctx.db.item().id().find(item_id) else {
            return Err("Merchant item not found".into());
        };
        if matches!(
            item.kind,
            crate::ItemKind::Currency | crate::ItemKind::Medication
        ) || *quantity == 0
        {
            return Err("Invalid merchant purchase".into());
        }
        use adventuresim_core::settlement_economy::{CatalogKind as C, Storefront as S};
        let catalog_kind = crate::item::economy_catalog_kind(item.kind);
        let storefront = match catalog_kind {
            C::Weapon | C::Shield => S::Weapons,
            C::Armor => S::Armor,
            C::Clothing => S::Clothing,
            C::Food => S::Inn,
            _ => S::General,
        };
        if !adventuresim_core::settlement_economy::storefront_stocks(
            &settlement_economy,
            storefront,
            item_id,
            catalog_kind,
        ) {
            return Err("This settlement does not stock that merchant item".into());
        }
        let quoted = adventuresim_core::strategic_economy::language_adjusted_buy_price(
            adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1)),
            shared_language,
        );
        let quoted =
            adventuresim_core::local_problem::adjust_price(quoted, problem_effects.buy_bps);
        let line =
            adventuresim_core::strategic_economy::checked_merchant_line_total(quoted, *quantity)
                .ok_or("Merchant purchase total overflow")?;
        cost = adventuresim_core::strategic_economy::checked_add_merchant_total(cost, line)
            .ok_or("Merchant purchase total overflow")?;
    }
    let mut proceeds = 0_u64;
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let (item_id, available, food_value) = if party_scope {
            let inventory = ctx
                .db
                .party_inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Party inventory item not found")?;
            if Some(&inventory.party_id) != party_id.as_ref() {
                return Err("Invalid party inventory sale".into());
            }
            let food_value = crate::food::party_lot(ctx, inventory.id).map(|lot| lot.total_value);
            (inventory.item_id, inventory.quantity, food_value)
        } else {
            let inventory = ctx
                .db
                .inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Inventory item not found")?;
            if inventory.character_id != character_id {
                return Err("Invalid merchant sale".into());
            }
            let food_value =
                crate::food::personal_lot(ctx, inventory.id).map(|lot| lot.total_value);
            (inventory.item_id, inventory.quantity, food_value)
        };
        let Some(item) = ctx.db.item().id().find(&item_id) else {
            return Err("Item definition not found".into());
        };
        if available < *quantity
            || *quantity == 0
            || matches!(
                item.kind,
                crate::ItemKind::Currency | crate::ItemKind::Medication
            )
        {
            return Err("Invalid merchant sale".into());
        }
        if !party_scope
            && ctx
                .db
                .character_equip()
                .character_id()
                .find(character_id)
                .is_some_and(|equip| equip.is_equiped(*inventory_id).is_some())
        {
            return Err("Unequip an item before selling it".into());
        }
        let line = if let Some(value) = food_value {
            if *quantity != available || !value.is_finite() || value < 0.0 {
                return Err("Food batches must be sold as complete valid lots".into());
            }
            let base = adventuresim_core::strategic_economy::merchant_sell_food_lot_value(value)
                .ok_or("Food lot has invalid value")?;
            let quoted = adventuresim_core::strategic_economy::language_adjusted_sell_price(
                u32::try_from(base).map_err(|_| "Food lot quote overflow")?,
                shared_language,
            );
            u64::from(adventuresim_core::local_problem::adjust_price(
                quoted,
                -problem_effects.sell_penalty_bps,
            ))
        } else {
            let quoted = adventuresim_core::strategic_economy::language_adjusted_sell_price(
                adventuresim_core::strategic_economy::merchant_sell_price(
                    item.base_value.unwrap_or(1),
                ),
                shared_language,
            );
            let quoted = adventuresim_core::local_problem::adjust_price(
                quoted,
                -problem_effects.sell_penalty_bps,
            );
            adventuresim_core::strategic_economy::checked_merchant_line_total(quoted, *quantity)
                .ok_or("Merchant sale total overflow")?
        };
        proceeds = adventuresim_core::strategic_economy::checked_add_merchant_total(proceeds, line)
            .ok_or("Merchant sale total overflow")?;
    }
    let coins = if party_scope {
        party_currency_total(ctx, party_id.as_ref().ok_or("Character has no party")?)
            .checked_add(crate::item::personal_currency_total(ctx, character_id))
            .ok_or("Merchant balance overflow")?
    } else {
        crate::item::personal_currency_total(ctx, character_id)
    };
    if coins
        .checked_add(proceeds)
        .ok_or("Merchant balance overflow")?
        < cost
    {
        return Err("Not enough coin".into());
    }
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        if party_scope {
            let mut inventory = ctx
                .db
                .party_inventory_item()
                .id()
                .find(*inventory_id)
                .unwrap();
            if inventory.quantity == *quantity {
                crate::food::delete_party_food_lot(ctx, *inventory_id);
                ctx.db.party_inventory_item().id().delete(*inventory_id);
                ctx.db
                    .party_item_condition()
                    .party_inventory_item_id()
                    .delete(*inventory_id);
            } else {
                if crate::food::party_lot(ctx, inventory.id).is_some() {
                    crate::food::remove_party_lot_quantity(
                        ctx,
                        *inventory_id,
                        *quantity,
                        inventory.quantity,
                    )?;
                }
                inventory.quantity -= quantity;
                ctx.db.party_inventory_item().id().update(inventory);
            }
        } else {
            let mut inventory = ctx.db.inventory_item().id().find(*inventory_id).unwrap();
            if inventory.quantity == *quantity {
                crate::food::delete_personal_food_lot(ctx, *inventory_id);
                ctx.db.inventory_item().id().delete(*inventory_id);
                ctx.db
                    .item_condition()
                    .inventory_item_id()
                    .delete(*inventory_id);
            } else {
                if crate::food::personal_lot(ctx, inventory.id).is_some() {
                    crate::food::remove_lot_quantity(
                        ctx,
                        *inventory_id,
                        *quantity,
                        inventory.quantity,
                    )?;
                }
                inventory.quantity -= quantity;
                ctx.db.inventory_item().id().update(inventory);
            }
        }
    }
    let equip = ctx.db.character_equip().character_id().find(character_id);
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        if party_scope {
            add_to_party_inventory(ctx, party_id.as_ref().unwrap(), item_id, *quantity);
            continue;
        }
        // Never add purchases to an equipped stack. An equipped item must stay
        // independently sellable from an otherwise identical spare item.
        let durable = ctx.db.item().id().find(item_id).is_some_and(|definition| {
            matches!(
                definition.kind,
                crate::ItemKind::Weapon | crate::ItemKind::Armor | crate::ItemKind::Shield
            )
        });
        let food = ctx
            .db
            .item()
            .id()
            .find(item_id)
            .is_some_and(|definition| definition.kind == crate::ItemKind::Food)
            || adventuresim_core::food::definition(item_id).is_some();
        if !durable
            && !food
            && let Some(mut stack) = ctx
                .db
                .inventory_item()
                .character_and_item_id()
                .filter((character_id, item_id))
                .find(|stack| {
                    !equip
                        .as_ref()
                        .is_some_and(|equip| equip.is_equiped(stack.id).is_some())
                })
        {
            if let Some(merged) = stack.quantity.checked_add(*quantity) {
                stack.quantity = merged;
                ctx.db.inventory_item().id().update(stack);
            } else {
                crate::add_inventory_item(ctx, character_id, item_id, *quantity);
            }
        } else {
            crate::add_inventory_item(ctx, character_id, item_id, *quantity);
        }
    }
    let (owes, receives) = if cost >= proceeds {
        (cost - proceeds, 0)
    } else {
        (0, proceeds - cost)
    };
    if party_scope && receives > 0 {
        let party_id = party_id.as_ref().unwrap();
        credit_party_currency(
            ctx,
            party_id,
            &settlement_id,
            u32::try_from(receives).map_err(|_| "Merchant proceeds exceed inventory capacity")?,
        )?;
    } else if party_scope && owes > 0 {
        let party_id = party_id.as_ref().unwrap();
        let party_coins = party_currency_total(ctx, party_id);
        let personal_coins = crate::item::personal_currency_total(ctx, character_id);
        let (pooled, personal) =
            adventuresim_core::strategic_economy::split_party_purchase_payment(
                party_coins,
                personal_coins,
                owes,
            )
            .ok_or("Not enough coin")?;
        consume_party_currency(ctx, party_id, pooled)?;
        consume_personal_gold(ctx, character_id, personal)?;
        if personal > 0 {
            credit_party_stake(ctx, party_id, character_id, personal)?;
        }
    } else if owes > 0 {
        consume_personal_gold(ctx, character_id, owes)?;
    } else if receives > 0 {
        crate::item::credit_personal_currency(
            ctx,
            character_id,
            &settlement_id,
            u32::try_from(receives).map_err(|_| "Merchant proceeds exceed inventory capacity")?,
        )?;
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn leave_party(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    remove_party_member(ctx, character_id, character_id)
}

/// Removes a non-leader member. Leaders may remove their members and non-leaders
/// may remove themselves; a leader must disband rather than remove themselves.
#[reducer]
pub fn remove_party_member(
    ctx: &ReducerContext,
    actor_character_id: u64,
    member_character_id: u64,
) -> Result<(), String> {
    require_character_no_unresolved_encounter(ctx, actor_character_id)?;
    crate::character::require_living_character(ctx, actor_character_id)?;
    let Some(actor) = ctx.db.character().id().find(actor_character_id) else {
        return Err("Acting character not found".into());
    };
    let Some(mut character) = ctx.db.character().id().find(member_character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Character is not in a party".into());
    };

    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if actor.party_id.as_deref() != Some(&party_id) {
        return Err("Characters are not in the same party".into());
    }
    if party.leader_id == member_character_id {
        return Err("Party leader cannot leave. Use disband_party instead.".into());
    }
    if actor_character_id != member_character_id && party.leader_id != actor_character_id {
        return Err("Only the party leader may remove another member".into());
    }
    if actor_character_id == party.leader_id && character.temporary {
        settle_temporary_member_stake(ctx, &party_id, member_character_id)?;
    }
    if ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .any(|stake| stake.character_id == member_character_id && stake.value > 0)
    {
        return Err("Withdraw this character's party stake before leaving".into());
    }

    if let Some(membership) = ctx
        .db
        .party_member()
        .character_id()
        .filter(member_character_id)
        .find(|m| m.party_id == party_id)
    {
        ctx.db.party_member().id().delete(membership.id);
    }

    character.party_id = None;
    ctx.db.character().id().update(character);
    for vote in ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        if vote.voter_id == member_character_id || vote.candidate_id == member_character_id {
            ctx.db.party_leader_vote().id().delete(&vote.id);
        }
    }
    normalize_and_elect_party_leader(ctx, &party_id)?;
    create_solo_party_for_character(ctx, member_character_id)?;
    Ok(())
}

/// Generated companions retain the value they contributed to the shared pool
/// when the leader dismisses them. Use the normal gold-withdrawal path before
/// removing them, rather than silently deleting their stake.
fn settle_temporary_member_stake(
    ctx: &ReducerContext,
    party_id: &str,
    member_character_id: u64,
) -> Result<(), String> {
    let stake_value = ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .find(|stake| stake.character_id == member_character_id)
        .map_or(0, |stake| stake.value);
    if stake_value == 0 {
        return Ok(());
    }
    transfer_party_currency_to_personal(ctx, party_id, member_character_id, stake_value)
}

#[reducer]
pub fn disband_party(ctx: &ReducerContext, leader_id: u64, party_id: String) -> Result<(), String> {
    require_no_unresolved_encounter(ctx, &party_id)?;
    crate::character::require_living_character(ctx, leader_id)?;
    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can disband the party".into());
    }
    if party.current_case_site_id.is_some() {
        return Err("Travel to a settlement before disbanding the party".into());
    }
    if ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .any(|stake| stake.value > 0)
    {
        return Err("Settle every member's party stake before disbanding".into());
    }
    let pooled_items: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(&party_id)
        .collect();
    let reserve = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id)
        .map_or(0, |state| state.reserve_value);
    if pooled_items
        .iter()
        .any(|entry| !crate::item::is_currency(ctx, &entry.item_id))
        || pooled_items
            .iter()
            .map(|entry| u64::from(entry.quantity))
            .sum::<u64>()
            != reserve
    {
        return Err("Liquidate and distribute the party inventory before disbanding".into());
    }
    if reserve > 0 {
        transfer_party_currency_to_personal(ctx, &party_id, party.leader_id, reserve)?;
    }
    for entry in pooled_items {
        ctx.db.party_inventory_item().id().delete(entry.id);
    }
    if ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db.party_inventory_state().party_id().delete(&party_id);
    }
    for stake in ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_stake().id().delete(stake.id);
    }

    let members: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();
    let member_ids: Vec<_> = members.iter().map(|member| member.character_id).collect();
    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(member.character_id) {
            character.party_id = None;
            ctx.db.character().id().update(character);
        }
        ctx.db.party_member().id().delete(member.id);
    }

    let requests: Vec<_> = ctx
        .db
        .party_join_request()
        .party_id()
        .filter(&party_id)
        .collect();
    for request in requests {
        ctx.db.party_join_request().id().delete(request.id);
    }
    for role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_recruitment_role().id().delete(role.id);
    }

    if let Some(contract_id) = party.active_contract_id
        && let Some(mut contract) = ctx.db.contract_authority().id().find(&contract_id)
    {
        contract.status = ContractStatus::Withdrawn;
        ctx.db.contract_authority().id().update(contract);
    }

    ctx.db.party_authority().id().delete(&party_id);
    for character_id in member_ids {
        create_solo_party_for_character(ctx, character_id)?;
    }
    Ok(())
}

fn contract_interaction_receipt_id(
    contract_id: &str,
    party_id: &str,
    stage: ContractInteractionStage,
) -> String {
    format!("interaction:{contract_id}:{party_id}:{stage:?}").to_lowercase()
}

fn record_contract_issuer_interaction(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
    stage: ContractInteractionStage,
    dialogue_session_id: String,
    dialogue_action_id: String,
    dialogue_revision: u64,
    location_id: String,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can interact for a contract".into());
    }
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Contract not found")?;
    let expected = match stage {
        ContractInteractionStage::Accept => ContractStatus::Offered,
        ContractInteractionStage::Report => ContractStatus::ReadyToReport,
    };
    if contract.status != expected
        || (stage == ContractInteractionStage::Report
            && contract.accepted_by.as_deref() != Some(&party_id))
    {
        return Err("Contract is not at the requested interaction stage".into());
    }
    if character.current_settlement_id.as_deref() != Some(&contract.settlement_id) {
        return Err("Contract issuer interaction requires their settlement".into());
    }
    let issuer = ctx
        .db
        .settlement_npc()
        .id()
        .find(&contract.issuer_npc_id)
        .ok_or("Contract issuer is not persistent")?;
    let presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&issuer.id)
        .ok_or("Contract issuer has no presence")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(720, |time| time.minutes);
    if issuer.home_settlement_id != contract.settlement_id
        || issuer.service_id != contract.service_id
        || presence.settlement_id != contract.settlement_id
        || presence.location_id != location_id
        || !crate::settlement_population::npc_is_present(&presence, minute)
    {
        return Err("Contract issuer is not available for interaction".into());
    }
    let id = contract_interaction_receipt_id(&contract_id, &party_id, stage);
    let row = ContractIssuerInteractionReceipt {
        id: id.clone(),
        contract_id,
        party_id,
        stage,
        issuer_npc_id: issuer.id,
        interacting_character_id: character_id,
        interacted_at_minute: crate::time::refresh_clock(ctx)?,
        dialogue_session_id,
        dialogue_action_id,
        dialogue_revision,
        location_id,
        consumed: false,
    };
    if ctx
        .db
        .contract_issuer_interaction_receipt()
        .id()
        .find(&id)
        .is_some()
    {
        ctx.db
            .contract_issuer_interaction_receipt()
            .id()
            .update(row);
    } else {
        ctx.db.contract_issuer_interaction_receipt().insert(row);
    }
    Ok(())
}

fn record_dialogue_contract_issuer_interaction(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
    stage: ContractInteractionStage,
    session: &DialogueSession,
    action_id: &str,
    resulting_revision: u64,
) -> Result<(), String> {
    if action_id == "start" {
        return Err("Contract interaction requires an explicit dialogue action".into());
    }
    let issuer = require_live_dialogue_presence(ctx, session, character_id)?;
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Contract not found")?;
    if issuer.id != contract.issuer_npc_id
        || issuer.service_id != contract.service_id
        || session.settlement_id != contract.settlement_id
    {
        return Err("This dialogue is not with the contract issuer".into());
    }
    record_contract_issuer_interaction(
        ctx,
        character_id,
        contract_id,
        stage,
        session.id.clone(),
        action_id.to_string(),
        resulting_revision,
        session.location_id.clone(),
    )
}

/// Disposable simulations cannot use the gateway-owned dialogue reducers.
/// This is the sole alternate producer and is restricted to the identity that
/// owns the simulation character; ordinary players and the web gateway cannot
/// mint an interaction receipt through it.
#[reducer]
pub fn simulate_contract_issuer_interaction(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
    stage: ContractInteractionStage,
) -> Result<(), String> {
    if !crate::simulation::sender_owns_simulation_character(ctx, character_id) {
        return Err("Only an owned disposable simulation may simulate NPC interaction".into());
    }
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Contract not found")?;
    let presence = ctx
        .db
        .settlement_npc_presence()
        .npc_id()
        .find(&contract.issuer_npc_id)
        .ok_or("Contract issuer has no presence")?;
    record_contract_issuer_interaction(
        ctx,
        character_id,
        contract_id,
        stage,
        format!("simulation:{character_id}"),
        format!("simulation:{stage:?}").to_lowercase(),
        0,
        presence.location_id,
    )
}

fn consume_contract_interaction(
    ctx: &ReducerContext,
    contract_id: &str,
    party_id: &str,
    stage: ContractInteractionStage,
) -> Result<(), String> {
    let id = contract_interaction_receipt_id(contract_id, party_id, stage);
    let mut receipt = ctx
        .db
        .contract_issuer_interaction_receipt()
        .id()
        .find(&id)
        .ok_or("Interact with the contract issuer first")?;
    if receipt.consumed || receipt.stage != stage {
        return Err("Contract issuer interaction receipt is unavailable".into());
    }
    receipt.consumed = true;
    ctx.db
        .contract_issuer_interaction_receipt()
        .id()
        .update(receipt);
    Ok(())
}

#[reducer]
pub fn accept_contract(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to accept quests".into());
    };

    let Some(mut party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can accept quests".into());
    }

    if party.active_contract_id.is_some() {
        return Err("Party already has an active quest".into());
    }

    let Some(mut quest) = ctx.db.contract_authority().id().find(&contract_id) else {
        return Err("Quest not found".into());
    };

    if quest.status != ContractStatus::Offered {
        return Err("Quest is not available".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&quest.case_id)
        .ok_or("Contract case not found")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("This case is no longer open".into());
    }

    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Must be at the quest's settlement to accept it".into());
    }
    consume_contract_interaction(
        ctx,
        &contract_id,
        &party_id,
        ContractInteractionStage::Accept,
    )?;

    quest.status = ContractStatus::Accepted;
    quest.accepted_by = Some(party_id.clone());
    quest.accepted_at_minute = Some(crate::time::refresh_clock(ctx)?);
    let case_id = quest.case_id.clone();
    let contract_id = quest.id.clone();
    ctx.db.contract_authority().id().update(quest);

    let site = ctx
        .db
        .case_site_authority()
        .case_id()
        .filter(&case_id)
        .next()
        .ok_or("Quest destination is not configured")?;
    disclose_exact_case_site(ctx, character_id, &case_id, &site, "the contract issuer")?;

    party.active_contract_id = Some(contract_id);
    ctx.db.party_authority().id().update(party);
    Ok(())
}

/// Selects an already-known exact site for presentation. This reducer has no
/// quest, contract, objective, reward, movement, or knowledge side effects.
#[reducer]
pub fn track_case_site(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: CaseSiteId,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character
        .party_id
        .ok_or("Must be in a party to track a case site")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can change the tracked site".into());
    }
    exact_case_site_for_observer(ctx, character_id, case_site_id.as_str())
        .ok_or("That exact site has not been disclosed to this observer")?;
    let row = PartyCaseSiteTracking {
        party_id: party_id.clone(),
        observer_character_id: character_id,
        case_site_id,
        tracked_at: crate::time::refresh_clock(ctx)?,
    };
    if ctx
        .db
        .party_case_site_tracking()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db.party_case_site_tracking().party_id().update(row);
    } else {
        ctx.db.party_case_site_tracking().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn abandon_contract(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Not in a party".into());
    };

    let Some(mut party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can abandon quests".into());
    }
    if crate::investigation::character_case_site_id(ctx, character_id).is_some() {
        return Err("Travel to a settlement before abandoning the quest".into());
    }

    let Some(mut quest) = ctx.db.contract_authority().id().find(&contract_id) else {
        return Err("Quest not found".into());
    };

    if quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("This quest is not accepted by your party".into());
    }
    if quest.status == ContractStatus::ReadyToReport {
        return Err("A completed quest must be returned to its questgiver".into());
    }

    quest.status = ContractStatus::Withdrawn;
    ctx.db.contract_authority().id().update(quest);

    party.active_contract_id = None;
    ctx.db.party_authority().id().update(party);
    Ok(())
}

fn travel_neighbors(ctx: &ReducerContext, node: u64) -> Vec<(u64, u32)> {
    let mut neighbors: Vec<_> = ctx
        .db
        .travel_edge()
        .from_node_id()
        .filter(&node)
        .map(|edge| (edge.to_node_id, edge.length_m))
        .collect();
    neighbors.extend(
        ctx.db
            .travel_edge()
            .to_node_id()
            .filter(&node)
            .map(|edge| (edge.from_node_id, edge.length_m)),
    );
    neighbors
}

/// Returns the next settlements reached from a source. Paths end at the first
/// settlement encountered, so journeys cannot skip intermediate settlements.
fn connected_settlement_distances(ctx: &ReducerContext, source_node_id: u64) -> HashMap<u64, u64> {
    let settlement_nodes: HashSet<u64> = ctx
        .db
        .settlement()
        .iter()
        .filter_map(|settlement| settlement.source_node_id)
        .collect();
    let mut distances = HashMap::from([(source_node_id, 0_u64)]);
    let mut pending = BinaryHeap::from([std::cmp::Reverse((0_u64, source_node_id))]);
    let mut destinations = HashMap::new();

    while let Some(std::cmp::Reverse((distance, node))) = pending.pop() {
        if distances.get(&node).is_some_and(|known| *known != distance) {
            continue;
        }
        if node != source_node_id && settlement_nodes.contains(&node) {
            destinations.insert(node, distance);
            continue;
        }
        for (neighbor, length_m) in travel_neighbors(ctx, node) {
            let next_distance = distance.saturating_add(u64::from(length_m));
            if distances
                .get(&neighbor)
                .is_none_or(|known| next_distance < *known)
            {
                distances.insert(neighbor, next_distance);
                pending.push(std::cmp::Reverse((next_distance, neighbor)));
            }
        }
    }
    destinations
}

fn journey_minutes(distance_m: u64) -> u64 {
    distance_m
        .saturating_mul(MINUTES_PER_HOUR)
        .div_ceil(WALKING_SPEED_KM_PER_HOUR * METERS_PER_KILOMETER)
        .max(1)
}

fn quest_journey_minutes(distance_m: u64) -> u64 {
    journey_minutes(distance_m).saturating_mul(QUEST_TRAVEL_SPEED_DIVISOR)
}

fn validate_journey_route(
    ctx: &ReducerContext,
    route: &JourneyRoutePlan,
    origin: (f64, f64),
    destination: (f64, f64),
) -> Result<(), String> {
    let authority = require_strategic_gateway(ctx)?;
    if authority.terrain_schema != 1
        || authority.terrain_package_digest.as_deref() != Some(route.package_digest.as_str())
    {
        return Err("Terrain route does not match the gateway terrain package".into());
    }
    validate_journey_route_payload(route, origin, destination)
}

fn validate_journey_route_payload(
    route: &JourneyRoutePlan,
    origin: (f64, f64),
    destination: (f64, f64),
) -> Result<(), String> {
    const MAX_POINTS: usize = 512;
    const MAX_SPANS: usize = 256;
    if !valid_route_digest(&route.package_digest) {
        return Err("Terrain route has an invalid package digest".into());
    }
    if !(2..=MAX_POINTS).contains(&route.points.len())
        || route.spans.is_empty()
        || route.spans.len() > MAX_SPANS
        || route.distance_m == 0
        || route.distance_m > 2_000_000
        || route.minutes == 0
        || route.minutes > 2_000_000
    {
        return Err("Terrain route exceeds its collection or aggregate bounds".into());
    }
    let coordinate = |point: &JourneyRoutePoint| {
        (
            f64::from(point.longitude_e7) / 10_000_000.0,
            f64::from(point.latitude_e7) / 10_000_000.0,
        )
    };
    if route.points.iter().any(|point| {
        !(-900_000_000..=900_000_000).contains(&point.latitude_e7)
            || !(-1_800_000_000..=1_800_000_000).contains(&point.longitude_e7)
    }) {
        return Err("Terrain route contains an invalid coordinate".into());
    }
    let first = coordinate(route.points.first().expect("bounded nonempty route"));
    let last = coordinate(route.points.last().expect("bounded nonempty route"));
    if straight_line_distance_m(first.0, first.1, origin.0, origin.1, true) > 500
        || straight_line_distance_m(last.0, last.1, destination.0, destination.1, true) > 500
    {
        return Err("Terrain route endpoints do not match the current journey".into());
    }
    let mut physical = 0_u64;
    for pair in route.points.windows(2) {
        let from = coordinate(&pair[0]);
        let to = coordinate(&pair[1]);
        let segment = straight_line_distance_m(from.0, from.1, to.0, to.1, true);
        if segment == 0 || segment > 100_000 {
            return Err("Terrain route points are not a bounded continuous path".into());
        }
        physical = physical
            .checked_add(segment)
            .ok_or("Terrain route distance overflow")?;
    }
    let tolerance = route.distance_m / 20 + 250;
    if physical.abs_diff(route.distance_m) > tolerance {
        return Err("Terrain route distance does not match its geometry".into());
    }
    let minimum_minutes = route
        .distance_m
        .saturating_mul(MINUTES_PER_HOUR)
        .div_ceil(7_500)
        .max(1);
    if route.minutes < minimum_minutes {
        return Err("Terrain route duration is faster than the maximum travel speed".into());
    }
    let mut cursor = 0_u64;
    for span in &route.spans {
        let weight_sum = u32::from(span.terrain.plains)
            + u32::from(span.terrain.forest)
            + u32::from(span.terrain.hills)
            + u32::from(span.terrain.urban);
        if weight_sum != 1_000
            || span.terrain.urban != 0
            || span.training_multiplier_permille > 1_000
            || span.check_millirank > 5_000
        {
            return Err("Terrain route span has invalid bounded skill metadata".into());
        }
        if span.start_minute != cursor || span.duration_minutes == 0 {
            return Err("Terrain route spans are discontinuous".into());
        }
        cursor = cursor
            .checked_add(span.duration_minutes)
            .ok_or("Terrain route minutes overflow")?;
    }
    if cursor != route.minutes {
        return Err("Terrain route spans do not match aggregate minutes".into());
    }
    Ok(())
}

fn validate_return_journey_route(
    ctx: &ReducerContext,
    route: &JourneyRoutePlan,
    origin: (f64, f64),
    destination: (f64, f64),
) -> Result<(), String> {
    let leg = route
        .return_route
        .as_ref()
        .ok_or("Quest travel requires an independently planned return route")?;
    validate_journey_route(
        ctx,
        &JourneyRoutePlan {
            package_digest: route.package_digest.clone(),
            distance_m: leg.distance_m,
            minutes: leg.minutes,
            points: leg.points.clone(),
            spans: leg.spans.clone(),
            return_route: None,
        },
        origin,
        destination,
    )
}

fn straight_line_distance_m(
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    geographic: bool,
) -> u64 {
    if geographic {
        let earth_radius_m = 6_371_000.0_f64;
        let lat1 = from_y.to_radians();
        let lat2 = to_y.to_radians();
        let delta_lat = (to_y - from_y).to_radians();
        let delta_lon = (to_x - from_x).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        if !a.is_finite() {
            return u64::MAX;
        }
        let a = a.clamp(0.0, 1.0);
        let distance_m = earth_radius_m * 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        if distance_m.is_finite() {
            distance_m.round() as u64
        } else {
            u64::MAX
        }
    } else {
        (((from_x - to_x).powi(2) + (from_y - to_y).powi(2)).sqrt() * METERS_PER_KILOMETER as f64)
            .round() as u64
    }
}

/// Canonical travel distance between signed E7 coordinates, in meters.
/// Geographic mode uses great-circle distance. Abstract mode uses the same
/// Euclidean coordinate-units-as-kilometers convention as strategic travel.
/// Invalid geographic latitude/longitude values fail closed.
pub(crate) fn coordinate_distance_e7_m(
    from_longitude_e7: i32,
    from_latitude_e7: i32,
    to_longitude_e7: i32,
    to_latitude_e7: i32,
    coordinates_are_geographic: bool,
) -> Option<u64> {
    if coordinates_are_geographic
        && (!(-900_000_000..=900_000_000).contains(&from_latitude_e7)
            || !(-1_800_000_000..=1_800_000_000).contains(&from_longitude_e7)
            || !(-900_000_000..=900_000_000).contains(&to_latitude_e7)
            || !(-1_800_000_000..=1_800_000_000).contains(&to_longitude_e7))
    {
        return None;
    }
    Some(straight_line_distance_m(
        f64::from(from_longitude_e7) / 10_000_000.0,
        f64::from(from_latitude_e7) / 10_000_000.0,
        f64::from(to_longitude_e7) / 10_000_000.0,
        f64::from(to_latitude_e7) / 10_000_000.0,
        coordinates_are_geographic,
    ))
}

struct IncidentSpec<'a> {
    kind: IncidentKind,
    title: &'a str,
    description: String,
    enemy_type: &'a str,
    difficulty: i32,
}

fn create_strategic_incident(
    ctx: &ReducerContext,
    party_id: &str,
    settlement: &Settlement,
    instigator_id: u64,
    source_id: IncidentSourceId,
    spec: IncidentSpec<'_>,
) -> Result<Option<IncidentId>, String> {
    parse_threat(spec.enemy_type)?;
    let Some(mut party) = ctx.db.party_authority().id().find(&party_id.to_string()) else {
        return Ok(None);
    };
    if party.current_settlement_id.as_deref() != Some(&settlement.id) {
        return Ok(None);
    }
    if ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .any(|incident| incident.status == IncidentStatus::Pending)
    {
        return Ok(None);
    }
    if let Some(existing) = ctx
        .db
        .strategic_incident()
        .iter()
        .find(|incident| incident.source_id == source_id)
    {
        return Ok(Some(existing.id));
    }
    let incident_key = format!("incident:{}", source_id.value);
    let incident_id = IncidentId {
        value: incident_key.clone(),
    };
    let case_site_id = format!("case-site:{incident_key}");
    let enemy_count = living_party_member_ids(ctx, party_id).len().max(2) as i32;
    let site = CaseSiteAuthority {
        id_key: case_site_id.clone(),
        id: CaseSiteId::from(case_site_id.clone()),
        case_id: incident_id.value.clone(),
        origin_settlement_id: settlement.id.clone(),
        name: spec.title.into(),
        description: spec.description,
        scene_key: settlement.scene_key.clone(),
        longitude_e7: (settlement.coord_x * 10_000_000.0).round() as i32,
        latitude_e7: (settlement.coord_y * 10_000_000.0).round() as i32,
        coordinates_are_geographic: settlement.source_node_id.is_some(),
        distance_m: 0,
    };
    ctx.db.case_site_authority().insert(site.clone());
    let hostile_group_id = format!("hostile-group:{}", incident_id.value);
    let hostile_group = materialize_hostile_group(
        ctx,
        &hostile_group_id,
        &site,
        spec.enemy_type.into(),
        enemy_count as u32,
        spec.difficulty,
    )?;
    ctx.db.strategic_incident().insert(StrategicIncident {
        id_key: incident_id.value.clone(),
        id: incident_id.clone(),
        source_id,
        party_id: party_id.into(),
        settlement_id: settlement.id.clone(),
        instigator_id,
        kind: spec.kind,
        status: IncidentStatus::Pending,
        case_site_id: site.id.clone(),
        hostile_group_id: hostile_group.id,
        created_at_minute: crate::time::refresh_clock(ctx)?,
    });

    for member_id in living_party_member_ids(ctx, party_id) {
        if let Some(mut member) = ctx.db.character().id().find(member_id) {
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(
                ctx,
                member.id,
                Some(case_site_id.clone()),
            );
            ctx.db.character().id().update(member);
        }
    }
    party.current_settlement_id = None;
    party.current_case_site_id = Some(CaseSiteId::from(case_site_id));
    ctx.db.party_authority().id().update(party);
    Ok(Some(incident_id))
}

fn maybe_trigger_religious_incident(
    ctx: &ReducerContext,
    party_id: &str,
    settlement: &Settlement,
) -> Result<Option<IncidentId>, String> {
    if ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .any(|incident| {
            incident.kind == IncidentKind::Religious && incident.settlement_id == settlement.id
        })
    {
        return Ok(None);
    }
    let mut instigator = None;
    for member_id in living_party_member_ids(ctx, party_id) {
        crate::condition::initialize_character_condition(ctx, member_id)?;
        let religion = ctx
            .db
            .character_condition()
            .character_id()
            .find(member_id)
            .and_then(|condition| condition.religion_id);
        if religion
            .as_deref()
            .is_none_or(|faith| faith == settlement.religion_id)
        {
            continue;
        }
        let condition = crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
        if instigator
            .as_ref()
            .is_none_or(|(_, fervor)| condition.fervor > *fervor)
        {
            instigator = Some((member_id, condition.fervor));
        }
    }
    let Some((instigator_id, instigator_fervor)) = instigator else {
        return Ok(None);
    };
    let roll = (ctx.random::<u64>() >> 40) as f32 / ((1_u32 << 24) as f32);
    if !fervor_event_occurs(instigator_fervor, roll) {
        return Ok(None);
    }
    let source_id = IncidentSourceId {
        value: format!("religious:{party_id}:{}", settlement.id),
    };
    create_strategic_incident(
        ctx,
        party_id,
        settlement,
        instigator_id,
        source_id,
        IncidentSpec {
            kind: IncidentKind::Religious,
            title: "A Quarrel at the Gate",
            description: format!(
                "At the gate of {}, a loud insult against the local faith has drawn an angry crowd. Combat is imminent, but the party can still withdraw and travel away.",
                settlement.name
            ),
            enemy_type: "angry_mob",
            difficulty: 1,
        },
    )
}

pub(crate) fn maybe_trigger_activity_incident(
    ctx: &ReducerContext,
    character_id: u64,
    risks: crate::time::ActivityRisks,
) -> Result<Option<IncidentId>, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(party_id) = character.party_id.as_deref() else {
        return Ok(None);
    };
    let Some(settlement_id) = character.current_settlement_id.as_ref() else {
        return Ok(None);
    };
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id)
        .ok_or("Character's settlement not found")?;
    let roll = |ctx: &ReducerContext| (ctx.random::<u64>() >> 40) as f32 / ((1_u32 << 24) as f32);
    let outcome = if fervor_event_occurs(risks.raiding_retaliation, roll(ctx)) {
        Some((
            "raiding",
            "Retaliation at Dawn",
            "The people raided from the surrounding countryside have tracked the party back to town. An armed band closes in; fight them or flee by road.",
            "armed_retainer",
            2,
        ))
    } else if fervor_event_occurs(risks.thievery_discovery, roll(ctx)) {
        Some((
            "thievery",
            "Caught Red-Handed",
            "A theft has been discovered and the watch has cornered the party near the market. Fight through them or abandon the settlement.",
            "town_watch",
            1,
        ))
    } else {
        None
    };
    let Some((kind, title, description, enemy_type, difficulty)) = outcome else {
        return Ok(None);
    };
    let (incident_kind, kind_key) = if kind == "raiding" {
        (IncidentKind::RaidingRetaliation, "raiding")
    } else {
        (IncidentKind::ThieveryDiscovery, "thievery")
    };
    let occurrence_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let source_id = activity_incident_source_id(
        kind_key,
        party_id,
        &settlement.id,
        character_id,
        occurrence_minute,
    );
    create_strategic_incident(
        ctx,
        party_id,
        &settlement,
        character_id,
        source_id,
        IncidentSpec {
            kind: incident_kind,
            title,
            description: description.into(),
            enemy_type,
            difficulty,
        },
    )
}

fn finish_strategic_incident(
    ctx: &ReducerContext,
    incident_id: &IncidentId,
    status: IncidentStatus,
) -> Result<(), String> {
    let Some(mut incident) = ctx
        .db
        .strategic_incident()
        .id_key()
        .find(&incident_id.value)
    else {
        return Ok(());
    };
    if incident.status != IncidentStatus::Pending {
        return Ok(());
    }
    incident.status = status;
    ctx.db.strategic_incident().id_key().update(incident);
    Ok(())
}

pub(crate) fn finish_incident_for_hostile_group(
    ctx: &ReducerContext,
    hostile_group_id: &str,
) -> Result<bool, String> {
    let incident = ctx.db.strategic_incident().iter().find(|incident| {
        incident_group_matches(
            incident.status,
            &incident.hostile_group_id,
            hostile_group_id,
        )
    });
    let Some(incident) = incident else {
        return Ok(false);
    };
    finish_strategic_incident(ctx, &incident.id, IncidentStatus::Resolved)?;
    Ok(true)
}

fn incident_group_matches(
    status: IncidentStatus,
    incident_hostile_group_id: &str,
    completed_hostile_group_id: &str,
) -> bool {
    status == IncidentStatus::Pending && incident_hostile_group_id == completed_hostile_group_id
}

fn activity_incident_source_id(
    kind: &str,
    party_id: &str,
    settlement_id: &str,
    character_id: u64,
    occurrence_minute: u64,
) -> IncidentSourceId {
    IncidentSourceId {
        value: format!(
            "activity:{kind}:{party_id}:{settlement_id}:{character_id}:{occurrence_minute}"
        ),
    }
}

/// Return the next leg's length. The least-rested member sets the party's
/// pace: once that member reaches the configured raw fatigue percentage, the
/// party makes camp. A one-minute minimum lets an already-tired party begin a
/// journey and immediately establish camp rather than becoming stranded.
fn party_travel_leg_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    _fatigue_percent: u8,
) -> Result<u64, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    if party.walking_minutes_per_day == 0 {
        return Err("The party is configured not to travel".into());
    }
    if daylight_walking_window(party.walking_minutes_per_day).is_none() {
        return Err("Party walking hours are invalid".into());
    }
    Ok(u64::from(party.walking_minutes_per_day))
}

fn party_next_walking_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    remaining_movement: u64,
) -> Result<u64, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let now = living_party_member_ids(ctx, party_id)
        .into_iter()
        .filter_map(|id| ctx.db.character_time().character_id().find(id))
        .map(|time| time.minutes)
        .max()
        .unwrap_or(0);
    let itinerary = forecast_itinerary(
        now,
        remaining_movement,
        party.walking_minutes_per_day,
        party.travel_at_night,
        party_camp_policy(&party),
        &party_itinerary_members(ctx, party_id)?,
    )
    .ok_or("Unable to forecast the next travel leg")?;
    Ok(itinerary.segments.first().map_or(0, |segment| {
        if matches!(segment.kind, ItinerarySegmentKind::Walking) {
            segment.movement_minutes
        } else {
            0
        }
    }))
}

fn full_rest_party_travel_leg_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    fatigue_percent: u8,
) -> Result<u64, String> {
    party_travel_leg_minutes(ctx, party_id, fatigue_percent)
}

fn party_camp_policy(party: &Party) -> CampDurationPolicy {
    match party.camp_duration_mode {
        CampDurationMode::Auto => CampDurationPolicy::Auto,
        CampDurationMode::Fixed => CampDurationPolicy::FixedMinutes(party.fixed_camp_minutes),
    }
}

fn party_itinerary_members(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<Vec<ItineraryMember>, String> {
    let mut members = Vec::new();
    for member_id in living_party_member_ids(ctx, party_id) {
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(member_id)
            .ok_or("Party member attributes not found")?;
        let limbs = ctx
            .db
            .character_limbs()
            .character_id()
            .find(member_id)
            .ok_or("Party member limbs not found")?;
        let stats = ctx
            .db
            .character_stats()
            .character_id()
            .find(member_id)
            .ok_or("Party member stats not found")?;
        let schedule = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(member_id)
            .ok_or("Party member schedule not found")?;
        let allowed = crate::time::allowed_camp_schedule(&schedule.downtime);
        members.push(ItineraryMember {
            fatigue_capacity: attributes
                .attr_by_parts(SimpleAttribute::Endurance, &limbs)
                .max(0.01)
                * 1_000.0,
            calories_used: stats.calories_used.max(0.0),
            camp_schedule: crate::time::core_schedule(&allowed),
        });
    }
    Ok(members)
}

fn itinerary_camps(forecast: &ItineraryForecast) -> Vec<JourneyCampInterval> {
    let mut camps: Vec<JourneyCampInterval> = Vec::new();
    for segment in forecast
        .segments
        .iter()
        .filter(|segment| segment.kind == ItinerarySegmentKind::Camp)
    {
        if let Some(last) = camps.last_mut()
            && last.movement_minute == segment.movement_start
            && last
                .elapsed_start_minute
                .saturating_add(last.elapsed_minutes)
                == segment.elapsed_start
        {
            last.elapsed_minutes = last.elapsed_minutes.saturating_add(segment.elapsed_minutes);
            last.average_fatigue_end = segment.average_fatigue_end;
            last.maximum_fatigue_end = last.maximum_fatigue_end.max(segment.maximum_fatigue_end);
            continue;
        }
        if camps.len() >= MAX_ITINERARY_SEGMENTS {
            break;
        }
        camps.push(JourneyCampInterval {
            movement_minute: segment.movement_start,
            elapsed_start_minute: segment.elapsed_start,
            elapsed_minutes: segment.elapsed_minutes,
            average_fatigue_start: segment.average_fatigue_start,
            average_fatigue_end: segment.average_fatigue_end,
            maximum_fatigue_end: segment.maximum_fatigue_end,
        });
    }
    camps
}

fn forecast_camp_stop_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    total_minutes: u64,
    completed_minutes: u64,
    fatigue_percent: u8,
) -> Result<Vec<u64>, String> {
    let mut stops = Vec::new();
    let mut elapsed = completed_minutes.min(total_minutes);
    let mut use_current_fatigue = true;
    while elapsed < total_minutes {
        let leg_minutes = if use_current_fatigue {
            party_travel_leg_minutes(ctx, party_id, fatigue_percent)?
        } else {
            full_rest_party_travel_leg_minutes(ctx, party_id, fatigue_percent)?
        };
        elapsed = elapsed.saturating_add(leg_minutes).min(total_minutes);
        if elapsed < total_minutes {
            if stops.len() >= MAX_ITINERARY_SEGMENTS {
                return Err("Journey requires too many legacy camp checkpoints".into());
            }
            stops.push(elapsed);
        }
        use_current_fatigue = false;
    }
    Ok(stops)
}

fn start_party_journey(
    ctx: &ReducerContext,
    party: &Party,
    origin: JourneyEndpoint,
    destination: JourneyEndpoint,
    total_minutes: u64,
    departure_minute: u64,
    route: Option<&JourneyRoutePlan>,
) -> Result<(), String> {
    require_no_unresolved_encounter(ctx, &party.id)?;
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db.strategic_encounter().party_id().delete(&party.id);
    }
    if ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_authority()
            .party_id()
            .delete(&party.id);
    }
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_itinerary()
            .party_id()
            .delete(&party.id);
    }
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party.id);
    }
    let fatigue_percent = party.camp_fatigue_percent;
    let forecast_camp_stop_minutes =
        forecast_camp_stop_minutes(ctx, &party.id, total_minutes, 0, fatigue_percent)?;
    let planned_movement = if matches!(destination, JourneyEndpoint::CaseSite(_)) {
        total_minutes.saturating_add(
            route
                .and_then(|route| route.return_route.as_ref())
                .map_or(total_minutes, |return_route| return_route.minutes),
        )
    } else {
        total_minutes
    };
    let itinerary = forecast_itinerary(
        departure_minute,
        planned_movement,
        party.walking_minutes_per_day,
        party.travel_at_night,
        party_camp_policy(party),
        &party_itinerary_members(ctx, &party.id)?,
    )
    .ok_or("Unable to forecast the party itinerary")?;
    if itinerary.truncated {
        return Err("Journey requires too many itinerary checkpoints".into());
    }
    // Actual departure ends any uninterrupted site/protection interval before
    // movement time is committed. Re-entering can create a new guard, but
    // never retroactively repairs this one.
    break_party_objective_continuity(ctx, &party.id)?;
    ctx.db.party_journey_authority().insert(PartyJourney {
        party_id: party.id.clone(),
        gateway_bucket: 0,
        origin,
        destination,
        total_minutes,
        completed_minutes: 0,
        camp_stop_minutes: Vec::new(),
        forecast_camp_stop_minutes,
        fatigue_percent,
        plan_version: 1,
        departure_minute,
        total_elapsed_minutes: itinerary.total_elapsed_minutes,
        completed_elapsed_minutes: 0,
        walking_minutes_per_day: party.walking_minutes_per_day,
        travel_at_night: party.travel_at_night,
        camp_duration_mode: party.camp_duration_mode,
        fixed_camp_minutes: party.fixed_camp_minutes,
    });
    ctx.db
        .party_journey_encounter_authority()
        .insert(PartyJourneyEncounterAuthority {
            party_id: party.id.clone(),
            seed: ctx.random(),
            next_roll: 1,
        });
    ctx.db
        .party_journey_itinerary()
        .insert(PartyJourneyItinerary {
            party_id: party.id.clone(),
            actual_camp_intervals: Vec::new(),
            forecast_camp_intervals: itinerary_camps(&itinerary),
        });
    if let Some(route) = route {
        ctx.db
            .party_journey_route_authority()
            .insert(PartyJourneyRoute {
                party_id: party.id.clone(),
                gateway_bucket: 0,
                package_digest: route.package_digest.clone(),
                distance_m: route.distance_m,
                minutes: route.minutes,
                points: route.points.clone(),
                spans: route.spans.clone(),
                return_route: route.return_route.clone(),
            });
    }
    Ok(())
}

fn record_party_journey_camp(
    ctx: &ReducerContext,
    party_id: &str,
    leg_minutes: u64,
) -> Result<(), String> {
    let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    journey.completed_minutes = journey
        .completed_minutes
        .saturating_add(leg_minutes)
        .min(journey.total_minutes);
    journey.completed_elapsed_minutes = journey
        .completed_elapsed_minutes
        .saturating_add(leg_minutes);
    if journey.camp_stop_minutes.last() != Some(&journey.completed_minutes) {
        journey.camp_stop_minutes.push(journey.completed_minutes);
    }
    ctx.db.party_journey_authority().party_id().update(journey);
    Ok(())
}

fn record_party_journey_interruption(ctx: &ReducerContext, party_id: &str, movement_minutes: u64) {
    if let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    {
        journey.completed_minutes = journey
            .completed_minutes
            .saturating_add(movement_minutes)
            .min(journey.total_minutes);
        journey.completed_elapsed_minutes = journey
            .completed_elapsed_minutes
            .saturating_add(movement_minutes);
        ctx.db.party_journey_authority().party_id().update(journey);
    }
}

/// Award conserved terrain exposure for the exact movement interval about to
/// be advanced. Camp time never reaches this function. The persisted route is
/// the departure snapshot, so chunked/offline continuation cannot change the
/// check, duration, or skill mixture mid-journey.
fn train_party_terrain_movement(
    ctx: &ReducerContext,
    party_id: &str,
    movement_minutes: u64,
) -> Result<(), String> {
    if movement_minutes == 0 {
        return Ok(());
    }
    let Some(journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    let Some(route) = ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    let start = journey.completed_minutes;
    let end = start.saturating_add(movement_minutes).min(route.minutes);
    let exposure = terrain_training_exposure(&route.spans, start, end);
    for member_id in living_party_member_ids(ctx, party_id) {
        if let Some(mut skills) = ctx.db.character_skills().character_id().find(member_id) {
            skills.terrain_plains_hours = (skills.terrain_plains_hours + exposure[0])
                .clamp(0.0, Skill::TerrainPlains.max_hours());
            skills.terrain_forest_hours = (skills.terrain_forest_hours + exposure[1])
                .clamp(0.0, Skill::TerrainForest.max_hours());
            skills.terrain_hills_hours = (skills.terrain_hills_hours + exposure[2])
                .clamp(0.0, Skill::TerrainHills.max_hours());
            skills.terrain_urban_hours = (skills.terrain_urban_hours + exposure[3])
                .clamp(0.0, Skill::TerrainUrban.max_hours());
            ctx.db.character_skills().character_id().update(skills);
        }
    }
    Ok(())
}

/// Give each traveler at most one interval of conversational exposure. Choices
/// are made from one sorted pre-gain snapshot, so party iteration cannot affect
/// the result and additional companions cannot multiply elapsed time.
fn train_party_oral_communication(ctx: &ReducerContext, party_id: &str, movement_minutes: u64) {
    if movement_minutes == 0 {
        return;
    }
    let mut snapshot: Vec<_> = living_party_member_ids(ctx, party_id)
        .into_iter()
        .filter_map(|id| {
            ctx.db
                .character_skills()
                .character_id()
                .find(id)
                .map(|skills| (id, skills.oral_languages))
        })
        .collect();
    snapshot.sort_by_key(|(id, _)| *id);
    let interval_hours = movement_minutes as f32 / 60.0;
    let gains = adventuresim_world_schema::party_oral_training_gains(&snapshot, interval_hours);
    for (id, language, hours) in gains {
        if let Some(mut skills) = ctx.db.character_skills().character_id().find(id) {
            skills.oral_languages.add_direct(language, hours);
            ctx.db.character_skills().character_id().update(skills);
        }
    }
}

fn terrain_training_exposure(spans: &[JourneyTerrainSpan], start: u64, end: u64) -> [f32; 4] {
    let mut exposure = [0.0_f32; 4];
    for span in spans {
        let overlap = end
            .min(span.start_minute.saturating_add(span.duration_minutes))
            .saturating_sub(start.max(span.start_minute));
        if overlap == 0 {
            continue;
        }
        let hours = overlap as f32 / 60.0 * f32::from(span.training_multiplier_permille) / 1_000.0;
        exposure[0] += hours * f32::from(span.terrain.plains) / 1_000.0;
        exposure[1] += hours * f32::from(span.terrain.forest) / 1_000.0;
        exposure[2] += hours * f32::from(span.terrain.hills) / 1_000.0;
        exposure[3] += hours * f32::from(span.terrain.urban) / 1_000.0;
    }
    exposure
}

fn advance_party_movement(
    ctx: &ReducerContext,
    party_id: &str,
    traveler_ids: &[u64],
    requested_minutes: u64,
) -> Result<(u64, bool), String> {
    let mut safe_prefixes = Vec::with_capacity(traveler_ids.len());
    for member_id in traveler_ids {
        safe_prefixes.push(preview_travel_time(ctx, *member_id, requested_minutes)?);
    }
    let actual_minutes = common_movement_prefix(requested_minutes, safe_prefixes.iter().copied());
    if actual_minutes == 0 {
        let mut all_survived = true;
        for (member_id, safe_prefix) in traveler_ids.iter().zip(safe_prefixes) {
            if zero_boundary_requires_settlement(actual_minutes, safe_prefix) {
                all_survived &= settle_travel_boundary(ctx, *member_id)?;
            }
        }
        return Ok((0, all_survived));
    }
    let mut all_survived = true;
    for member_id in traveler_ids.iter().copied() {
        all_survived &= advance_travel_time(ctx, member_id, actual_minutes)?;
    }
    // Training is committed only after every participant's authoritative
    // clock has committed the same safe movement prefix.
    train_party_terrain_movement(ctx, party_id, actual_minutes)?;
    train_party_oral_communication(ctx, party_id, actual_minutes);
    Ok((actual_minutes, all_survived))
}

fn zero_boundary_requires_settlement(actual_minutes: u64, safe_prefix: u64) -> bool {
    actual_minutes == 0 && safe_prefix == 0
}

fn set_party_journey_state(
    party: &mut Party,
    current_settlement_id: Option<String>,
    current_case_site_id: Option<CaseSiteId>,
    camp_destination_id: Option<String>,
    camp_destination_kind: Option<String>,
    camp_remaining_minutes: u64,
) {
    // Deliberately touch only journey fields. In particular, leadership may
    // have changed while movement committed a terminal event.
    party.current_settlement_id = current_settlement_id;
    party.current_case_site_id = current_case_site_id;
    party.camp_destination = match (camp_destination_id, camp_destination_kind.as_deref()) {
        (Some(id), Some("settlement")) => {
            Some(JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id,
                name: String::new(),
            }))
        }
        (Some(id), Some("case_site")) => Some(JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
            id: CaseSiteId { value: id },
            name: String::new(),
        })),
        (None, None) => None,
        _ => unreachable!("validated camp destination"),
    };
    party.camp_remaining_minutes = camp_remaining_minutes;
}

fn party_can_continue_travel(party: &Party, character_id: u64) -> bool {
    party.leader_id == character_id
}

fn common_movement_prefix(
    requested_minutes: u64,
    safe_prefixes: impl IntoIterator<Item = u64>,
) -> u64 {
    safe_prefixes.into_iter().fold(requested_minutes, u64::min)
}

pub(crate) fn refresh_party_journey_forecast(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    if journey.plan_version == 0 {
        let current = living_party_member_ids(ctx, party_id)
            .into_iter()
            .filter_map(|id| ctx.db.character_time().character_id().find(id))
            .map(|time| time.minutes)
            .max()
            .unwrap_or(0);
        (journey.departure_minute, journey.completed_elapsed_minutes) =
            reconstruct_legacy_journey_coordinates(current, journey.completed_minutes);
        journey.plan_version = 1;
    }
    journey.forecast_camp_stop_minutes = forecast_camp_stop_minutes(
        ctx,
        party_id,
        journey.total_minutes,
        journey.completed_minutes,
        journey.fatigue_percent,
    )?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let start = journey
        .departure_minute
        .saturating_add(journey.completed_elapsed_minutes);
    let planned_movement = if journey.destination.case_site_id().is_some() {
        journey.total_minutes.saturating_mul(2)
    } else {
        journey.total_minutes
    };
    let remaining = planned_movement.saturating_sub(journey.completed_minutes);
    let itinerary = forecast_itinerary(
        start,
        remaining,
        party.walking_minutes_per_day,
        party.travel_at_night,
        party_camp_policy(&party),
        &party_itinerary_members(ctx, party_id)?,
    )
    .ok_or("Unable to forecast the remaining itinerary")?;
    if itinerary.truncated {
        return Err("Journey requires too many itinerary checkpoints".into());
    }
    journey.walking_minutes_per_day = party.walking_minutes_per_day;
    journey.travel_at_night = party.travel_at_night;
    journey.camp_duration_mode = party.camp_duration_mode;
    journey.fixed_camp_minutes = party.fixed_camp_minutes;
    journey.total_elapsed_minutes = journey
        .completed_elapsed_minutes
        .saturating_add(itinerary.total_elapsed_minutes);
    let forecast_camp_intervals = itinerary_camps(&itinerary)
        .into_iter()
        .map(|mut interval| {
            interval.movement_minute = interval
                .movement_minute
                .saturating_add(journey.completed_minutes);
            interval.elapsed_start_minute = interval
                .elapsed_start_minute
                .saturating_add(journey.completed_elapsed_minutes);
            interval
        })
        .collect();
    let mut typed = ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .unwrap_or(PartyJourneyItinerary {
            party_id: party_id.to_string(),
            actual_camp_intervals: Vec::new(),
            forecast_camp_intervals: Vec::new(),
        });
    typed.forecast_camp_intervals = forecast_camp_intervals;
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .is_some()
    {
        ctx.db.party_journey_itinerary().party_id().update(typed);
    } else {
        ctx.db.party_journey_itinerary().insert(typed);
    }
    ctx.db.party_journey_authority().party_id().update(journey);
    Ok(())
}

pub(crate) fn record_party_camp_rest(
    ctx: &ReducerContext,
    party_id: &str,
    elapsed: u64,
    average_start: f32,
    average_end: f32,
    maximum_end: f32,
) -> Result<(), String> {
    let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    let start = journey.completed_elapsed_minutes;
    journey.completed_elapsed_minutes = journey.completed_elapsed_minutes.saturating_add(elapsed);
    let mut typed = ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .unwrap_or(PartyJourneyItinerary {
            party_id: party_id.to_string(),
            actual_camp_intervals: Vec::new(),
            forecast_camp_intervals: Vec::new(),
        });
    let typed_exists = ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .is_some();
    if let Some(last) = typed.actual_camp_intervals.last_mut()
        && last.movement_minute == journey.completed_minutes
        && last
            .elapsed_start_minute
            .saturating_add(last.elapsed_minutes)
            == start
    {
        last.elapsed_minutes = last.elapsed_minutes.saturating_add(elapsed);
        last.average_fatigue_end = average_end;
        last.maximum_fatigue_end = maximum_end;
    } else if typed.actual_camp_intervals.len() < MAX_ITINERARY_SEGMENTS {
        typed.actual_camp_intervals.push(JourneyCampInterval {
            movement_minute: journey.completed_minutes,
            elapsed_start_minute: start,
            elapsed_minutes: elapsed,
            average_fatigue_start: average_start,
            average_fatigue_end: average_end,
            maximum_fatigue_end: maximum_end,
        });
    } else {
        return Err("Journey has too many camp checkpoints".into());
    }
    if typed_exists {
        ctx.db.party_journey_itinerary().party_id().update(typed);
    } else {
        ctx.db.party_journey_itinerary().insert(typed);
    }
    ctx.db.party_journey_authority().party_id().update(journey);
    Ok(())
}

fn finish_party_journey(ctx: &ReducerContext, party_id: &str) {
    let party_id = party_id.to_string();
    if ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_authority()
            .party_id()
            .delete(&party_id);
    }
    if ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_encounter_authority()
            .party_id()
            .delete(&party_id);
    }
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party_id);
    }
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_itinerary()
            .party_id()
            .delete(&party_id);
    }
}

fn camp_redirect_minutes(journey: &PartyJourney, settlement_id: &str) -> Option<u64> {
    if journey.origin.settlement_id() == Some(settlement_id) {
        return Some(journey.completed_minutes);
    }
    if journey.destination.settlement_id() == Some(settlement_id) {
        return Some(
            journey
                .total_minutes
                .saturating_sub(journey.completed_minutes),
        );
    }
    None
}

fn route_position_at_minute(route: &PartyJourneyRoute, minute: u64) -> Option<(f64, f64)> {
    let coordinate = |point: &JourneyRoutePoint| {
        (
            f64::from(point.longitude_e7) / 10_000_000.0,
            f64::from(point.latitude_e7) / 10_000_000.0,
        )
    };
    let lengths = route
        .points
        .windows(2)
        .map(|pair| {
            let from = coordinate(&pair[0]);
            let to = coordinate(&pair[1]);
            straight_line_distance_m(from.0, from.1, to.0, to.1, true)
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<u64>();
    if total == 0 || route.minutes == 0 {
        return route.points.first().map(coordinate);
    }
    let target = total.saturating_mul(minute.min(route.minutes)) / route.minutes;
    let mut traversed = 0_u64;
    for (index, length) in lengths.into_iter().enumerate() {
        if traversed.saturating_add(length) >= target {
            let from = coordinate(&route.points[index]);
            let to = coordinate(&route.points[index + 1]);
            let fraction = if length == 0 {
                0.0
            } else {
                (target.saturating_sub(traversed)) as f64 / length as f64
            };
            return Some((
                from.0 + (to.0 - from.0) * fraction,
                from.1 + (to.1 - from.1) * fraction,
            ));
        }
        traversed = traversed.saturating_add(length);
    }
    route.points.last().map(coordinate)
}

fn unresolved_encounter(ctx: &ReducerContext, party_id: &str) -> Option<StrategicEncounter> {
    ctx.db
        .strategic_encounter()
        .party_id()
        .find(&party_id.to_string())
        .filter(|encounter| encounter.status == "awaiting_choice")
}

pub(crate) fn require_no_unresolved_encounter(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    if unresolved_encounter(ctx, party_id).is_some() {
        Err("Resolve the strategic encounter before changing or continuing travel".into())
    } else {
        Ok(())
    }
}

pub(crate) fn require_character_no_unresolved_encounter(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    if let Some(party_id) = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id)
    {
        require_no_unresolved_encounter(ctx, &party_id)?;
    }
    Ok(())
}

fn encounter_terrain_at(route: Option<&PartyJourneyRoute>, minute: u64) -> JourneyTerrainKind {
    route
        .and_then(|route| {
            route.spans.iter().find(|span| {
                minute >= span.start_minute
                    && minute < span.start_minute.saturating_add(span.duration_minutes)
            })
        })
        .map_or(JourneyTerrainKind::Open, |span| span.kind)
}

fn core_encounter_terrain(
    kind: JourneyTerrainKind,
) -> adventuresim_core::encounter::EncounterTerrain {
    use adventuresim_core::encounter::EncounterTerrain;
    match kind {
        JourneyTerrainKind::Road => EncounterTerrain::Road,
        JourneyTerrainKind::Open => EncounterTerrain::Open,
        JourneyTerrainKind::SparseWoods => EncounterTerrain::SparseWoods,
        JourneyTerrainKind::DeepWoods => EncounterTerrain::DeepWoods,
    }
}

fn journey_fallback_position(
    ctx: &ReducerContext,
    journey: &PartyJourney,
    minute: u64,
) -> (f64, f64) {
    let endpoint = |endpoint: &JourneyEndpoint| -> Option<(f64, f64)> {
        match endpoint {
            JourneyEndpoint::Settlement(endpoint) => ctx
                .db
                .settlement()
                .id()
                .find(&endpoint.id)
                .map(|v| (v.coord_x, v.coord_y)),
            JourneyEndpoint::CaseSite(endpoint) => ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&endpoint.id.value)
                .map(|v| {
                    (
                        f64::from(v.longitude_e7) / 10_000_000.0,
                        f64::from(v.latitude_e7) / 10_000_000.0,
                    )
                }),
            JourneyEndpoint::Camp(_) => None,
        }
    };
    let start = endpoint(&journey.origin).unwrap_or((0.0, 0.0));
    let end = endpoint(&journey.destination).unwrap_or(start);
    let progress = minute.min(journey.total_minutes) as f64 / journey.total_minutes.max(1) as f64;
    (
        start.0 + (end.0 - start.0) * progress,
        start.1 + (end.1 - start.1) * progress,
    )
}

fn party_encumbrance_remaining_basis_points(
    ctx: &ReducerContext,
    party_id: &str,
    member_ids: &[u64],
) -> u32 {
    let personal_burden: f32 = member_ids
        .iter()
        .flat_map(|member_id| ctx.db.inventory_item().character_id().filter(*member_id))
        .map(|row| {
            ctx.db
                .item()
                .id()
                .find(&row.item_id)
                .map_or(0.0, |item| item.weight * row.quantity as f32)
        })
        .sum();
    let party_burden: f32 = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .map(|row| {
            ctx.db
                .item()
                .id()
                .find(&row.item_id)
                .map_or(0.0, |item| item.weight * row.quantity as f32)
        })
        .sum();
    let capacity: f32 = member_ids
        .iter()
        .map(|member_id| {
            let Some(attributes) = ctx
                .db
                .character_attributes()
                .character_id()
                .find(*member_id)
            else {
                return 0.0;
            };
            let Some(limbs) = ctx.db.character_limbs().character_id().find(*member_id) else {
                return 0.0;
            };
            let adjusted_leg_strength = (attributes.left_leg_strength * limbs.left_leg_health
                + attributes.right_leg_strength * limbs.right_leg_health)
                * 0.5;
            adventuresim_core::equipment::encumbrance_capacity_kg(adjusted_leg_strength)
        })
        .sum();
    let body_burden = member_ids.len() as f32 * 70.0;
    let remaining = adventuresim_core::equipment::encumbrance_remaining_multiplier(
        body_burden + personal_burden + party_burden,
        capacity,
    );
    (remaining.clamp(0.0, 1.0) * 10_000.0).round() as u32
}

fn current_party_fatigue_percent(ctx: &ReducerContext, member_ids: &[u64]) -> u8 {
    member_ids
        .iter()
        .filter_map(|member_id| {
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(*member_id)?;
            let limbs = ctx.db.character_limbs().character_id().find(*member_id)?;
            let stats = ctx.db.character_stats().character_id().find(*member_id)?;
            let capacity = attributes
                .attr_by_parts(SimpleAttribute::Endurance, &limbs)
                .max(0.01)
                * 1_000.0;
            Some(((stats.calories_used.max(0.0) / capacity) * 100.0).round() as u16)
        })
        .max()
        .unwrap_or(0)
        .min(100) as u8
}

fn whole_party_sneak_score(ctx: &ReducerContext, member_ids: &[u64]) -> u16 {
    member_ids
        .iter()
        .filter_map(|member_id| {
            let skills = ctx.db.character_skills().character_id().find(*member_id)?;
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(*member_id)?;
            let training =
                adventuresim_core::prelude::Skill::Stealth.training_rank(skills.stealth_hours);
            let agility = (attributes.left_arm_agility + attributes.right_arm_agility) * 0.5;
            Some((training.min(agility).max(0.0) * 100.0).round() as u16)
        })
        .min()
        .unwrap_or(0)
}

/// Truncates a walking leg at its first canonical random-encounter boundary.
/// The caller advances ordinary time/needs/fatigue by the returned duration.
fn maybe_interrupt_travel(
    ctx: &ReducerContext,
    party_id: &str,
    requested_minutes: u64,
) -> Result<(u64, Option<StrategicEncounter>, u64), String> {
    require_no_unresolved_encounter(ctx, party_id)?;
    let Some(journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok((requested_minutes, None, 1));
    };
    let absolute_start = journey
        .departure_minute
        .saturating_add(journey.completed_elapsed_minutes);
    if let (Some(origin_id), Some(destination_id)) = (
        journey.origin.settlement_id(),
        journey.destination.settlement_id(),
    ) {
        crate::local_problem::ensure_route_problem(ctx, origin_id, destination_id, absolute_start)?;
    }
    let authority = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Journey encounter authority is missing")?;
    let route = ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_id.to_string());
    let active_contract_archetype = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .and_then(|party| party.active_contract_id)
        .and_then(|contract_id| ctx.db.contract_authority().id().find(&contract_id))
        .filter(|contract| {
            ctx.db
                .case_site_authority()
                .id_key()
                .find(&journey.destination.case_site_id().unwrap().to_string())
                .is_some_and(|site| site.case_id == contract.case_id)
        })
        .and_then(|contract| {
            ctx.db
                .case_site_authority()
                .case_id()
                .filter(&contract.case_id)
                .find_map(|site| {
                    ctx.db
                        .hostile_group_authority()
                        .iter()
                        .find(|group| group.case_site_id == site.id)
                })
        })
        .and_then(|group| quest_encounter_archetype(&group.enemy_type));
    let member_ids = living_party_member_ids(ctx, party_id);
    let capable = member_ids
        .iter()
        .filter(|id| {
            ctx.db
                .character_capability()
                .character_id()
                .find(**id)
                .is_some_and(|capability| capability.melee || capability.ranged)
        })
        .count()
        .max(1) as u16;
    let completed = journey.completed_minutes;
    let selection = adventuresim_core::encounter::first_encounter_with_problem(
        authority.seed,
        completed,
        requested_minutes,
        |minute| {
            let terrain = core_encounter_terrain(encounter_terrain_at(route.as_ref(), minute));
            let absolute_minute = absolute_start.saturating_add(minute.saturating_sub(completed));
            let night = absolute_minute % 1_440 < 360 || absolute_minute % 1_440 >= 1_200;
            adventuresim_core::encounter::EncounterContext {
                terrain,
                night,
                accepted_active_quest: active_contract_archetype.map(|archetype| {
                    adventuresim_core::encounter::AcceptedQuestInfluence {
                        archetype,
                        distance_minutes: journey.total_minutes.saturating_sub(minute),
                    }
                }),
                combat_capable_members: capable,
                party_awareness: 250,
                enemy_awareness: 250
                    + if night {
                        adventuresim_core::encounter::NIGHT_ENEMY_AWARENESS_BONUS
                    } else {
                        0
                    },
                party_speed_m_per_minute:
                    adventuresim_core::encounter::PARTY_WALKING_SPEED_M_PER_MINUTE,
            }
        },
        |minute| {
            let absolute_minute = absolute_start.saturating_add(minute.saturating_sub(completed));
            match (
                journey.origin.settlement_id(),
                journey.destination.settlement_id(),
            ) {
                (Some(origin_id), Some(destination_id)) => {
                    crate::local_problem::route_encounter_influence(
                        ctx,
                        origin_id,
                        destination_id,
                        absolute_minute,
                    )
                }
                _ => None,
            }
        },
    );
    let crossed_end = completed.saturating_add(requested_minutes);
    let next_roll = crossed_end / adventuresim_core::encounter::ENCOUNTER_ROLL_INTERVAL_MINUTES + 1;
    let Some(selection) = selection else {
        return Ok((requested_minutes, None, next_roll));
    };

    use adventuresim_core::encounter::{Awareness, EncounterArchetype};
    let (party_aware, enemy_aware) = match selection.awareness {
        Awareness::PartyOnly => (true, false),
        Awareness::EnemyOnly => (false, true),
        Awareness::Both => (true, true),
        Awareness::Neither => return Ok((requested_minutes, None, next_roll)),
    };
    let archetype = match selection.archetype {
        EncounterArchetype::Bandits => "bandit",
        EncounterArchetype::Goblins => "goblin",
        EncounterArchetype::Undead => "skeleton",
    };
    let encounter_terrain = core_encounter_terrain(encounter_terrain_at(
        route.as_ref(),
        selection.boundary_minute,
    ));
    let party_speed = adventuresim_core::encounter::sustainable_speed_m_per_minute(
        journey.fatigue_percent,
        party_encumbrance_remaining_basis_points(ctx, party_id, &member_ids),
        member_ids.len().min(u16::MAX as usize) as u16,
        encounter_terrain,
    );
    let enemy_speed = selection.archetype.enemy_speed_m_per_minute();
    let run_eligible =
        adventuresim_core::encounter::run_is_eligible(party_speed, selection.archetype);
    let choices = adventuresim_core::encounter::available_choices(
        selection.awareness,
        selection.archetype,
        party_speed,
    )
    .into_iter()
    .map(|choice| match choice {
        adventuresim_core::encounter::EncounterChoice::Sneak => "sneak",
        adventuresim_core::encounter::EncounterChoice::Detour => "detour",
        adventuresim_core::encounter::EncounterChoice::Attack => "attack",
        adventuresim_core::encounter::EncounterChoice::Run => "run",
        adventuresim_core::encounter::EncounterChoice::Surrender => "surrender",
    })
    .map(str::to_string)
    .collect();
    let position = route
        .as_ref()
        .and_then(|route| route_position_at_minute(route, selection.boundary_minute))
        .unwrap_or_else(|| journey_fallback_position(ctx, &journey, selection.boundary_minute));
    let terrain = encounter_terrain_at(route.as_ref(), selection.boundary_minute);
    let mut encounter = StrategicEncounter {
        party_id: party_id.into(),
        encounter_id: format!("{}:{}", party_id, selection.roll_index),
        archetype: archetype.into(),
        enemy_count: selection.count,
        roll_index: selection.roll_index,
        journey_movement_minute: selection.boundary_minute,
        journey_elapsed_minute: journey
            .completed_elapsed_minutes
            .saturating_add(selection.boundary_minute.saturating_sub(completed)),
        absolute_minute: absolute_start
            .saturating_add(selection.boundary_minute.saturating_sub(completed)),
        longitude_e7: (position.0 * 10_000_000.0).round() as i32,
        latitude_e7: (position.1 * 10_000_000.0).round() as i32,
        terrain: format!("{terrain:?}").to_ascii_lowercase(),
        party_aware,
        enemy_aware,
        available_choices: choices,
        status: "awaiting_choice".into(),
        selected_choice: None,
        selection_explanation: format!(
            "Canonical journey roll {} in {:?}; party awareness {} vs enemy awareness {}",
            selection.roll_index, terrain, selection.party_roll, selection.enemy_roll
        ),
        party_speed_m_per_minute: party_speed,
        enemy_speed_m_per_minute: enemy_speed,
        run_ineligibility: (!run_eligible).then(|| {
            format!(
                "Party speed {party_speed} m/min does not exceed enemy speed {enemy_speed} m/min"
            )
        }),
        penalty_minutes: 0,
        loss_preview: Vec::new(),
        outcome: None,
    };
    if encounter
        .available_choices
        .iter()
        .any(|choice| choice == "surrender")
    {
        encounter.loss_preview = encounter_loss_preview(ctx, party_id);
    }
    Ok((
        selection.boundary_minute.saturating_sub(completed),
        Some(encounter),
        selection.roll_index.saturating_add(1),
    ))
}

fn advance_party_movement_until_encounter(
    ctx: &ReducerContext,
    party_id: &str,
    traveler_ids: &[u64],
    proposed_leg_minutes: u64,
) -> Result<(u64, Option<StrategicEncounter>, u64), String> {
    let (requested_leg_minutes, mut encounter, mut next_roll) =
        maybe_interrupt_travel(ctx, party_id, proposed_leg_minutes)?;
    let (actual_minutes, _) =
        advance_party_movement(ctx, party_id, traveler_ids, requested_leg_minutes)?;
    if actual_minutes < requested_leg_minutes {
        let (rescanned_minutes, rescanned_encounter, rescanned_next_roll) =
            maybe_interrupt_travel(ctx, party_id, actual_minutes)?;
        debug_assert_eq!(rescanned_minutes, actual_minutes);
        encounter = rescanned_encounter;
        next_roll = rescanned_next_roll;
    }
    Ok((actual_minutes, encounter, next_roll))
}

/// Commits the scan cursor and, when one was found, materializes the encounter
/// only after every traveler has reached the same canonical boundary.
fn commit_encounter_scan(
    ctx: &ReducerContext,
    party_id: &str,
    next_roll: u64,
    encounter: Option<StrategicEncounter>,
) -> Result<(), String> {
    let mut authority = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Journey encounter authority is missing")?;
    authority.next_roll = next_roll;
    let seed = authority.seed;
    ctx.db
        .party_journey_encounter_authority()
        .party_id()
        .update(authority);

    let Some(mut encounter) = encounter else {
        return Ok(());
    };
    let member_ids = living_party_member_ids(ctx, party_id);
    if member_ids.is_empty() {
        return Err("A party with no living members cannot enter an encounter".into());
    }
    let capable = member_ids
        .iter()
        .filter(|id| {
            ctx.db
                .character_capability()
                .character_id()
                .find(**id)
                .is_some_and(|capability| capability.melee || capability.ranged)
        })
        .count()
        .max(1) as u16;
    let archetype = match encounter.archetype.as_str() {
        "bandit" => adventuresim_core::encounter::EncounterArchetype::Bandits,
        "goblin" => adventuresim_core::encounter::EncounterArchetype::Goblins,
        "skeleton" => adventuresim_core::encounter::EncounterArchetype::Undead,
        _ => return Err("Encounter has an unknown archetype".into()),
    };
    let awareness = match (encounter.party_aware, encounter.enemy_aware) {
        (true, false) => adventuresim_core::encounter::Awareness::PartyOnly,
        (false, true) => adventuresim_core::encounter::Awareness::EnemyOnly,
        (true, true) => adventuresim_core::encounter::Awareness::Both,
        (false, false) => return Err("Encounter has no aware participants".into()),
    };
    let terrain = core_encounter_terrain(match encounter.terrain.as_str() {
        "road" => JourneyTerrainKind::Road,
        "open" => JourneyTerrainKind::Open,
        "sparsewoods" | "sparse_woods" => JourneyTerrainKind::SparseWoods,
        "deepwoods" | "deep_woods" => JourneyTerrainKind::DeepWoods,
        _ => JourneyTerrainKind::Open,
    });
    encounter.enemy_count = adventuresim_core::encounter::scale_enemy_count(
        adventuresim_core::encounter::enemy_count(seed, encounter.roll_index, capable),
        archetype,
    );
    encounter.party_speed_m_per_minute =
        adventuresim_core::encounter::sustainable_speed_m_per_minute(
            current_party_fatigue_percent(ctx, &member_ids),
            party_encumbrance_remaining_basis_points(ctx, party_id, &member_ids),
            member_ids.len().min(u16::MAX as usize) as u16,
            terrain,
        );
    let run_eligible = adventuresim_core::encounter::run_is_eligible(
        encounter.party_speed_m_per_minute,
        archetype,
    );
    encounter.available_choices = adventuresim_core::encounter::available_choices(
        awareness,
        archetype,
        encounter.party_speed_m_per_minute,
    )
    .into_iter()
    .map(|choice| match choice {
        adventuresim_core::encounter::EncounterChoice::Sneak => "sneak",
        adventuresim_core::encounter::EncounterChoice::Detour => "detour",
        adventuresim_core::encounter::EncounterChoice::Attack => "attack",
        adventuresim_core::encounter::EncounterChoice::Run => "run",
        adventuresim_core::encounter::EncounterChoice::Surrender => "surrender",
    })
    .map(str::to_string)
    .collect();
    encounter.run_ineligibility = (!run_eligible).then(|| {
        format!(
            "Party speed {} m/min does not exceed enemy speed {} m/min",
            encounter.party_speed_m_per_minute, encounter.enemy_speed_m_per_minute
        )
    });
    encounter.loss_preview = if encounter
        .available_choices
        .iter()
        .any(|choice| choice == "surrender")
    {
        encounter_loss_preview(ctx, party_id)
    } else {
        Vec::new()
    };
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party_id.to_string())
        .is_some()
    {
        ctx.db.strategic_encounter().party_id().update(encounter);
    } else {
        ctx.db.strategic_encounter().insert(encounter);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParsedEncounterChoice {
    Sneak,
    Detour,
    Attack,
    Run,
    Surrender,
}

impl ParsedEncounterChoice {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "sneak" => Ok(Self::Sneak),
            "detour" => Ok(Self::Detour),
            "attack" => Ok(Self::Attack),
            "run" => Ok(Self::Run),
            "surrender" => Ok(Self::Surrender),
            _ => Err("Unknown encounter choice".into()),
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Sneak => "sneak",
            Self::Detour => "detour",
            Self::Attack => "attack",
            Self::Run => "run",
            Self::Surrender => "surrender",
        }
    }
}

fn encounter_loss_preview(ctx: &ReducerContext, party_id: &str) -> Vec<StrategicEncounterLoss> {
    let minimum = adventuresim_core::encounter::SURRENDER_MINIMUM_ITEM_VALUE;
    let mut losses = Vec::new();
    for row in ctx.db.party_inventory_item().party_id().filter(party_id) {
        let currency = crate::item::is_currency(ctx, &row.item_id);
        let value = ctx
            .db
            .item()
            .id()
            .find(&row.item_id)
            .and_then(|item| item.base_value)
            .unwrap_or(0);
        if currency || value >= minimum {
            losses.push(StrategicEncounterLoss {
                owner_kind: "party".into(),
                owner_id: 0,
                inventory_id: row.id,
                item_id: row.item_id,
                quantity: row.quantity,
                value_each: value,
            });
        }
    }
    let mut member_ids: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .map(|membership| membership.character_id)
        .collect();
    member_ids.sort_unstable();
    member_ids.dedup();
    for member_id in member_ids {
        for row in ctx.db.inventory_item().character_id().filter(member_id) {
            let currency = crate::item::is_currency(ctx, &row.item_id);
            let value = ctx
                .db
                .item()
                .id()
                .find(&row.item_id)
                .and_then(|item| item.base_value)
                .unwrap_or(0);
            if currency || value >= minimum {
                losses.push(StrategicEncounterLoss {
                    owner_kind: "member".into(),
                    owner_id: member_id,
                    inventory_id: row.id,
                    item_id: row.item_id,
                    quantity: row.quantity,
                    value_each: value,
                });
            }
        }
    }
    losses.sort_by(|a, b| {
        (&a.owner_kind, a.owner_id, a.inventory_id).cmp(&(
            &b.owner_kind,
            b.owner_id,
            b.inventory_id,
        ))
    });
    losses
}

fn commit_encounter_surrender(
    ctx: &ReducerContext,
    party_id: &str,
    current: &[StrategicEncounterLoss],
) -> Result<(), String> {
    for loss in current {
        if loss.owner_kind == "party" {
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(loss.inventory_id);
            ctx.db.party_inventory_item().id().delete(loss.inventory_id);
        } else {
            if let Some(mut equip) = ctx.db.character_equip().character_id().find(loss.owner_id)
                && crate::repair::is_equipped(&equip, loss.inventory_id)
            {
                crate::repair::unequip(&mut equip, loss.inventory_id);
                ctx.db.character_equip().character_id().update(equip);
            }
            ctx.db
                .item_condition()
                .inventory_item_id()
                .delete(loss.inventory_id);
            ctx.db.inventory_item().id().delete(loss.inventory_id);
        }
    }
    reconcile_party_pool_ledger(ctx, party_id)?;
    for member_id in living_party_member_ids(ctx, party_id) {
        crate::capability::refresh_character_capability(ctx, member_id)?;
    }
    Ok(())
}

fn reconcile_party_pool_ledger(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let remaining_value = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .try_fold(0_u64, |total, row| {
            Ok::<_, String>(total.saturating_add(
                objective_item_value(ctx, &row.item_id)?.saturating_mul(u64::from(row.quantity)),
            ))
        })?;
    let mut stakes: Vec<_> = ctx.db.party_stake().party_id().filter(party_id).collect();
    stakes.sort_by_key(|stake| stake.id);
    let prior_reserve = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
        .map_or(0, |state| state.reserve_value);
    let total_claims = stakes.iter().fold(prior_reserve, |total, stake| {
        total.saturating_add(stake.value)
    });
    let mut allocated = 0_u64;
    for mut stake in stakes {
        stake.value = if total_claims == 0 {
            0
        } else {
            ((u128::from(stake.value) * u128::from(remaining_value)) / u128::from(total_claims))
                as u64
        };
        allocated = allocated.saturating_add(stake.value);
        ctx.db.party_stake().id().update(stake);
    }
    let reserve_value = remaining_value.saturating_sub(allocated);
    if let Some(mut state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
    {
        state.reserve_value = reserve_value;
        ctx.db.party_inventory_state().party_id().update(state);
    } else {
        ctx.db.party_inventory_state().insert(PartyInventoryState {
            party_id: party_id.to_string(),
            reserve_value,
        });
    }
    Ok(())
}

fn encounter_core_terrain(value: &str) -> adventuresim_core::encounter::EncounterTerrain {
    use adventuresim_core::encounter::EncounterTerrain;
    match value {
        "road" => EncounterTerrain::Road,
        "sparsewoods" => EncounterTerrain::SparseWoods,
        "deepwoods" => EncounterTerrain::DeepWoods,
        _ => EncounterTerrain::Open,
    }
}

fn advance_encounter_penalty(
    ctx: &ReducerContext,
    encounter: &mut StrategicEncounter,
    choice: ParsedEncounterChoice,
) -> Result<(), String> {
    use adventuresim_core::encounter::EncounterChoice;
    let core_choice = match choice {
        ParsedEncounterChoice::Detour => EncounterChoice::Detour,
        ParsedEncounterChoice::Run => EncounterChoice::Run,
        _ => return Ok(()),
    };
    let minutes = adventuresim_core::encounter::penalty_minutes(
        encounter_core_terrain(&encounter.terrain),
        core_choice,
    );
    for member_id in living_party_member_ids(ctx, &encounter.party_id) {
        if !advance_travel_time(ctx, member_id, minutes)? {
            return Err(
                "Every living party member must be able to complete the encounter delay".into(),
            );
        }
    }
    if let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&encounter.party_id)
    {
        journey.completed_elapsed_minutes =
            journey.completed_elapsed_minutes.saturating_add(minutes);
        ctx.db.party_journey_authority().party_id().update(journey);
    }
    encounter.penalty_minutes = minutes;
    Ok(())
}

/// Commit every persistent consequence shared by quest and random autoresolve.
/// The battle itself remains transient; only its bounded summary and strategic
/// condition effects cross into SpacetimeDB.
fn commit_autoresolve_outcome(
    ctx: &ReducerContext,
    source_id: &str,
    party_id: &str,
    member_ids: &[u64],
    defeat_morale_penalty: f32,
    outcome: &adventuresim_core::autoresolve::BattleOutcome,
) -> Result<(), String> {
    record_autoresolve_report(ctx, source_id, party_id, outcome);
    for member_id in member_ids {
        crate::filth::deposit_now(
            ctx,
            *member_id,
            crate::filth::FilthSubstance::Dirt,
            None,
            adventuresim_core::filth::COMBAT_DIRT,
        )?;
    }
    for exchange in &outcome.log {
        if exchange.cut_damage > 0.0 && member_ids.contains(&exchange.attacker_id) {
            crate::filth::deposit_now(
                ctx,
                exchange.attacker_id,
                crate::filth::FilthSubstance::Blood,
                member_ids
                    .contains(&exchange.defender_id)
                    .then_some(exchange.defender_id),
                (exchange.cut_damage * 35.0).ceil().clamp(1.0, 15.0) as u16,
            )?;
        }
        if let Some(id) = exchange.weapon_inventory_item_id {
            crate::repair::apply_impact(ctx, id, exchange.contact_stress);
        }
        if let Some(id) = exchange.defender_contact_item_id {
            crate::repair::apply_impact(ctx, id, exchange.contact_stress);
        }
        if exchange.armor_contact
            && exchange.contact_stress > 0.0
            && let Some(equip) = ctx
                .db
                .character_equip()
                .character_id()
                .find(exchange.defender_id)
        {
            let armor_id = match exchange.body_part {
                BodyPart::LeftArm => equip.left_arm_armor_id,
                BodyPart::RightArm => equip.right_arm_armor_id,
                BodyPart::LeftLeg => equip.left_leg_armor_id,
                BodyPart::RightLeg => equip.right_leg_armor_id,
                BodyPart::Chest => equip.chest_armor_id,
                BodyPart::Stomach => equip.stomach_armor_id,
                BodyPart::Head => equip.head_armor_id,
            };
            if let Some(id) = armor_id {
                crate::repair::apply_impact(ctx, id, exchange.contact_stress);
            }
        }
    }
    for member in &outcome.allies {
        consume_autoresolve_ammunition(ctx, member.id, member.ammunition_used);
        for exchange in outcome
            .log
            .iter()
            .filter(|exchange| exchange.defender_id == member.id && exchange.health_damage > 0.0)
        {
            let limb = match exchange.body_part {
                BodyPart::LeftArm => crate::surgery::LimbRegion::LeftArm,
                BodyPart::RightArm => crate::surgery::LimbRegion::RightArm,
                BodyPart::LeftLeg => crate::surgery::LimbRegion::LeftLeg,
                BodyPart::RightLeg => crate::surgery::LimbRegion::RightLeg,
                BodyPart::Chest => crate::surgery::LimbRegion::Chest,
                BodyPart::Stomach => crate::surgery::LimbRegion::Stomach,
                BodyPart::Head => crate::surgery::LimbRegion::Head,
            };
            let projectile = exchange.projectile_kind.map(|kind| match kind {
                adventuresim_core::autoresolve::CombatProjectileKind::Arrowhead => {
                    crate::surgery::ProjectileKind::Arrowhead
                }
                adventuresim_core::autoresolve::CombatProjectileKind::Ball => {
                    crate::surgery::ProjectileKind::Ball
                }
            });
            crate::surgery::commit_hit_injury(
                ctx,
                member.id,
                limb,
                exchange.cut_damage,
                exchange.blunt_damage,
                projectile,
            )?;
        }
        crate::condition::apply_blood_loss(ctx, member.id, member.blood_loss_fraction)?;
        crate::capability::refresh_character_capability(ctx, member.id)?;
    }
    if outcome.victor != BattleVictor::Allies {
        for member_id in member_ids {
            crate::condition::record_morale_event(
                ctx,
                *member_id,
                "defeat",
                -defeat_morale_penalty,
                Some(source_id.to_string()),
            )?;
        }
    }
    Ok(())
}

fn resolve_random_encounter_battle(
    ctx: &ReducerContext,
    encounter: &StrategicEncounter,
    seed: u64,
    opening: BattleOpening,
) -> Result<String, String> {
    let member_ids = living_party_member_ids(ctx, &encounter.party_id);
    let allies = member_ids
        .iter()
        .map(|id| {
            let condition = crate::condition::refresh_character_strategic_condition(ctx, *id)?;
            crate::capability::load_combatant(
                ctx,
                *id,
                condition.incapacitation,
                condition.pain,
                condition.blood_loss,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let difficulty = i32::from(encounter.enemy_count.max(1));
    let enemies = (0..u64::from(encounter.enemy_count))
        .map(|index| {
            autoresolve_enemy(
                u64::MAX.saturating_sub(index),
                &encounter.archetype,
                difficulty,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let outcome = resolve_battle(allies, enemies, seed ^ encounter.roll_index, opening);
    commit_autoresolve_outcome(
        ctx,
        &encounter.encounter_id,
        &encounter.party_id,
        &member_ids,
        5.0 + f32::from(encounter.enemy_count),
        &outcome,
    )?;
    if outcome.victor == BattleVictor::Allies {
        if let Some(item_id) = autoresolve_drop(&encounter.archetype)? {
            add_to_party_inventory(
                ctx,
                &encounter.party_id,
                item_id,
                u32::from(encounter.enemy_count),
            );
        }
        for member_id in &member_ids {
            crate::condition::record_morale_event(
                ctx,
                *member_id,
                "victory",
                5.0 + f32::from(encounter.enemy_count),
                Some(encounter.encounter_id.clone()),
            )?;
        }
    }
    Ok(match outcome.victor {
        BattleVictor::Allies => "victory",
        BattleVictor::Enemies => "defeat",
        BattleVictor::Stalemate => "stalemate",
    }
    .into())
}

#[reducer]
pub fn resolve_strategic_encounter(
    ctx: &ReducerContext,
    character_id: u64,
    choice: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can resolve an encounter".into());
    }
    let parsed = ParsedEncounterChoice::parse(&choice)?;
    let mut encounter = unresolved_encounter(ctx, &party_id).ok_or("No unresolved encounter")?;
    let seed = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id)
        .ok_or("Journey encounter authority is missing")?
        .seed;
    if !encounter
        .available_choices
        .iter()
        .any(|available| available == parsed.label())
    {
        return Err("That choice is not available for this encounter".into());
    }
    encounter.selected_choice = Some(parsed.label().into());
    match parsed {
        ParsedEncounterChoice::Sneak => {
            let enemy_stealth =
                u16::from(parse_threat(&encounter.archetype)?.profile().combat.stealth);
            if adventuresim_core::encounter::sneak_succeeds(
                seed,
                encounter.roll_index,
                whole_party_sneak_score(ctx, &living_party_member_ids(ctx, &party_id)),
                200_u16.saturating_add(enemy_stealth),
            ) {
                encounter.outcome = Some("avoided".into());
            } else {
                encounter.outcome = Some(resolve_random_encounter_battle(
                    ctx,
                    &encounter,
                    seed,
                    BattleOpening::Normal,
                )?);
            }
        }
        ParsedEncounterChoice::Detour | ParsedEncounterChoice::Run => {
            if parsed == ParsedEncounterChoice::Run
                && encounter.party_speed_m_per_minute <= encounter.enemy_speed_m_per_minute
            {
                return Err("The party is not fast enough to run".into());
            }
            advance_encounter_penalty(ctx, &mut encounter, parsed)?;
            encounter.outcome = Some("avoided".into());
        }
        ParsedEncounterChoice::Attack => {
            let opening = match (encounter.party_aware, encounter.enemy_aware) {
                (true, false) => BattleOpening::AlliesSurprise,
                (false, true) => BattleOpening::EnemiesSurprise,
                _ => BattleOpening::Normal,
            };
            encounter.outcome = Some(resolve_random_encounter_battle(
                ctx, &encounter, seed, opening,
            )?);
        }
        ParsedEncounterChoice::Surrender => {
            let current = encounter_loss_preview(ctx, &party_id);
            if current != encounter.loss_preview {
                encounter.selected_choice = None;
                encounter.loss_preview = current;
                ctx.db.strategic_encounter().party_id().update(encounter);
                return Ok(());
            }
            commit_encounter_surrender(ctx, &party_id, &current)?;
            encounter.outcome = Some("surrendered".into());
        }
    }
    encounter.status = "resolved".into();
    ctx.db.strategic_encounter().party_id().update(encounter);
    Ok(())
}

fn redirect_camped_party_to_settlement(
    ctx: &ReducerContext,
    party: &mut Party,
    destination: &Settlement,
    route: Option<JourneyRoutePlan>,
) -> Result<(), String> {
    let mut journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party.id)
        .ok_or("Camp journey not found")?;
    let travel_minutes = if let Some(route) = route.as_ref() {
        let current_route = ctx
            .db
            .party_journey_route_authority()
            .party_id()
            .find(&party.id)
            .ok_or("Camp has no persisted terrain route")?;
        let origin = route_position_at_minute(&current_route, journey.completed_minutes)
            .ok_or("Camp route position is unavailable")?;
        validate_journey_route(
            ctx,
            route,
            origin,
            (destination.coord_x, destination.coord_y),
        )?;
        route.minutes
    } else {
        camp_redirect_minutes(&journey, &destination.id)
            .ok_or("That settlement is not an endpoint of this camp journey")?
    };
    if travel_minutes == 0 {
        return Err("The party is already at that journey endpoint".into());
    }

    journey.origin = JourneyEndpoint::Camp(party.id.clone());
    journey.destination = JourneyEndpoint::Settlement(JourneySettlementEndpoint {
        id: destination.id.clone(),
        name: destination.name.clone(),
    });
    journey.total_minutes = travel_minutes;
    journey.completed_minutes = 0;
    journey.departure_minute = living_party_member_ids(ctx, &party.id)
        .into_iter()
        .filter_map(|member_id| ctx.db.character_time().character_id().find(member_id))
        .map(|time| time.minutes)
        .max()
        .unwrap_or(journey.departure_minute);
    journey.completed_elapsed_minutes = 0;
    journey.camp_stop_minutes.clear();
    if let Some(mut typed) = ctx.db.party_journey_itinerary().party_id().find(&party.id) {
        typed.actual_camp_intervals.clear();
        typed.forecast_camp_intervals.clear();
        ctx.db.party_journey_itinerary().party_id().update(typed);
    }
    journey.forecast_camp_stop_minutes =
        forecast_camp_stop_minutes(ctx, &party.id, travel_minutes, 0, journey.fatigue_percent)?;
    ctx.db.party_journey_authority().party_id().update(journey);
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party.id);
    }
    if let Some(route) = route {
        ctx.db
            .party_journey_route_authority()
            .insert(PartyJourneyRoute {
                party_id: party.id.clone(),
                gateway_bucket: 0,
                package_digest: route.package_digest,
                distance_m: route.distance_m,
                minutes: route.minutes,
                points: route.points,
                spans: route.spans,
                return_route: route.return_route,
            });
    }

    party.current_settlement_id = None;
    party.current_case_site_id = None;
    party.camp_destination = Some(JourneyEndpoint::Settlement(JourneySettlementEndpoint {
        id: destination.id.clone(),
        name: destination.name.clone(),
    }));
    party.camp_remaining_minutes = travel_minutes;
    ctx.db.party_authority().id().update(party.clone());
    refresh_party_journey_forecast(ctx, &party.id)?;
    Ok(())
}

fn revalidate_party_after_departure_sync(
    ctx: &ReducerContext,
    party_id: &str,
    leader_id: u64,
    expected_settlement_id: Option<&str>,
    expected_quest_location_id: Option<&str>,
    expected_active_contract_id: Option<&str>,
) -> Result<Party, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party changed during departure synchronization")?;
    let party_matches = party.leader_id == leader_id
        && party.camp_destination.is_none()
        && party.current_settlement_id.as_deref() == expected_settlement_id
        && party.current_case_site_id.as_deref() == expected_quest_location_id
        && !expected_active_contract_id
            .is_some_and(|id| party.active_contract_id.as_deref() != Some(id));
    let pending_incident_sites: Vec<_> = ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .filter(|incident| incident.status == IncidentStatus::Pending)
        .map(|incident| incident.case_site_id.value)
        .collect();
    if !departure_snapshot_allows_travel(
        party_matches,
        true,
        pending_incident_allows_departure(
            expected_quest_location_id,
            pending_incident_sites.iter().map(String::as_str),
        ),
    ) {
        return Err("Travel was interrupted while the party synchronized its clocks".into());
    }
    let members = living_party_member_ids(ctx, party_id);
    let members_match = !members.is_empty()
        && !members.iter().any(|id| {
            ctx.db.character().id().find(*id).is_none_or(|member| {
                member.current_settlement_id.as_deref() != expected_settlement_id
                    || crate::investigation::character_case_site_id(ctx, member.id).as_deref()
                        != expected_quest_location_id
            })
        });
    if !departure_snapshot_allows_travel(true, members_match, true) {
        return Err("A party member changed location during departure synchronization".into());
    }
    require_party_ready(ctx, party_id)?;
    Ok(party)
}

fn departure_snapshot_allows_travel(
    party_matches: bool,
    members_match: bool,
    incident_snapshot_allows_departure: bool,
) -> bool {
    party_matches && members_match && incident_snapshot_allows_departure
}

fn pending_incident_allows_departure<'a>(
    expected_case_site_id: Option<&str>,
    pending_case_site_ids: impl Iterator<Item = &'a str>,
) -> bool {
    let mut pending = pending_case_site_ids;
    match (pending.next(), pending.next()) {
        (None, _) => true,
        (Some(site), None) => expected_case_site_id == Some(site),
        (Some(_), Some(_)) => false,
    }
}

fn reconstruct_legacy_journey_coordinates(
    current_minute: u64,
    completed_movement: u64,
) -> (u64, u64) {
    (
        current_minute.saturating_sub(completed_movement),
        completed_movement,
    )
}

#[cfg(test)]
mod departure_invariant_tests {
    use super::{
        CampDurationMode, JourneyEndpoint, JourneyRoutePlan, JourneyRoutePoint,
        JourneySettlementEndpoint, JourneyTerrainKind, JourneyTerrainSpan, JourneyTerrainWeights,
        Party, PartyJourneyRoute, common_movement_prefix, departure_snapshot_allows_travel,
        party_can_continue_travel, pending_incident_allows_departure,
        reconstruct_legacy_journey_coordinates, route_position_at_minute, set_party_journey_state,
        straight_line_distance_m, terrain_training_exposure, validate_journey_route_payload,
        zero_boundary_requires_settlement,
    };

    #[test]
    fn departure_requires_unchanged_party_members_and_incident_snapshot() {
        assert!(!departure_snapshot_allows_travel(true, true, false));
        assert!(!departure_snapshot_allows_travel(false, true, true));
        assert!(!departure_snapshot_allows_travel(true, false, true));
        assert!(departure_snapshot_allows_travel(true, true, true));
    }

    #[test]
    fn only_the_exact_departing_incident_site_may_be_avoided() {
        assert!(pending_incident_allows_departure(None, std::iter::empty()));
        assert!(pending_incident_allows_departure(
            Some("site:a"),
            ["site:a"].into_iter()
        ));
        assert!(!pending_incident_allows_departure(
            Some("site:a"),
            ["site:b"].into_iter()
        ));
        assert!(!pending_incident_allows_departure(
            None,
            ["site:a"].into_iter()
        ));
        assert!(!pending_incident_allows_departure(
            Some("site:a"),
            ["site:a", "site:b"].into_iter()
        ));
    }

    #[test]
    fn legacy_journey_never_falls_back_to_day_one() {
        assert_eq!(
            reconstruct_legacy_journey_coordinates(20_000, 600),
            (19_400, 600)
        );
        assert_eq!(reconstruct_legacy_journey_coordinates(300, 600), (0, 600));
    }

    fn route_fixture() -> JourneyRoutePlan {
        let origin = (10.0, 53.0);
        let destination = (10.01, 53.0);
        JourneyRoutePlan {
            package_digest: "a".repeat(64),
            distance_m: straight_line_distance_m(
                origin.0,
                origin.1,
                destination.0,
                destination.1,
                true,
            ),
            minutes: 12,
            points: vec![
                JourneyRoutePoint {
                    latitude_e7: 530_000_000,
                    longitude_e7: 100_000_000,
                },
                JourneyRoutePoint {
                    latitude_e7: 530_000_000,
                    longitude_e7: 100_100_000,
                },
            ],
            spans: vec![
                JourneyTerrainSpan {
                    kind: JourneyTerrainKind::Road,
                    terrain: JourneyTerrainWeights {
                        plains: 1_000,
                        forest: 0,
                        hills: 0,
                        urban: 0,
                    },
                    training_multiplier_permille: 250,
                    check_millirank: 0,
                    start_minute: 0,
                    duration_minutes: 5,
                },
                JourneyTerrainSpan {
                    kind: JourneyTerrainKind::Open,
                    terrain: JourneyTerrainWeights {
                        plains: 1_000,
                        forest: 0,
                        hills: 0,
                        urban: 0,
                    },
                    training_multiplier_permille: 1_000,
                    check_millirank: 0,
                    start_minute: 5,
                    duration_minutes: 7,
                },
            ],
            return_route: None,
        }
    }

    #[test]
    fn planned_route_validation_binds_endpoints_geometry_and_exact_minutes() {
        let route = route_fixture();
        assert!(validate_journey_route_payload(&route, (10.0, 53.0), (10.01, 53.0)).is_ok());

        let mut bad = route.clone();
        bad.points[0].longitude_e7 += 1_000_000;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.distance_m *= 2;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.spans[1].start_minute = 6;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route.clone();
        bad.spans[0].terrain.plains = 999;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        for index in 0..2 {
            let mut bad = route.clone();
            bad.spans[index].terrain.plains -= 1;
            bad.spans[index].terrain.urban = 1;
            assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());
        }

        let mut bad = route.clone();
        bad.minutes = 1;
        bad.spans = vec![JourneyTerrainSpan {
            kind: JourneyTerrainKind::Road,
            terrain: JourneyTerrainWeights {
                plains: 1_000,
                forest: 0,
                hills: 0,
                urban: 0,
            },
            training_multiplier_permille: 250,
            check_millirank: 0,
            start_minute: 0,
            duration_minutes: 1,
        }];
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());

        let mut bad = route;
        bad.points[0].latitude_e7 = i32::MAX;
        assert!(validate_journey_route_payload(&bad, (10.0, 53.0), (10.01, 53.0)).is_err());
    }

    #[test]
    fn max_rank_seventy_five_hundred_metres_per_hour_route_is_accepted() {
        let mut route = route_fixture();
        route.minutes = route.distance_m.saturating_mul(60).div_ceil(7_500);
        route.spans[0].duration_minutes = 2;
        route.spans[1].start_minute = 2;
        route.spans[1].duration_minutes = route.minutes - 2;
        for span in &mut route.spans {
            span.check_millirank = 5_000;
        }
        assert_eq!(route.minutes, 6);
        assert!(validate_journey_route_payload(&route, (10.0, 53.0), (10.01, 53.0)).is_ok());
    }

    #[test]
    fn terrain_training_uses_exact_overlap_and_conserves_mixed_exposure() {
        let spans = route_fixture().spans;
        let exposure = terrain_training_exposure(&spans, 3, 9);
        // Two road minutes at 25%, then four open minutes at full exposure.
        assert!((exposure[0] - 4.5 / 60.0).abs() < 0.0001);
        assert_eq!(exposure[1..], [0.0, 0.0, 0.0]);
        let none = terrain_training_exposure(&spans, 12, 30);
        assert_eq!(none, [0.0; 4]);
    }

    #[test]
    fn terminal_prefix_and_retry_train_each_committed_minute_once() {
        let spans = route_fixture().spans;
        let first = common_movement_prefix(12, [12, 4]);
        assert_eq!(
            first, 4,
            "the earliest death boundary limits the whole party"
        );
        let retry = common_movement_prefix(12 - first, [8]);
        let chunked = terrain_training_exposure(&spans, 0, first);
        let resumed = terrain_training_exposure(&spans, first, first + retry);
        let whole = terrain_training_exposure(&spans, 0, 12);
        for index in 0..4 {
            assert!((chunked[index] + resumed[index] - whole[index]).abs() < 0.0001);
        }
    }

    #[test]
    fn zero_minute_terminal_is_settled_before_survivors_retry() {
        let first_prefixes = [12, 0];
        let first = common_movement_prefix(12, first_prefixes);
        assert_eq!(first, 0);
        assert!(!zero_boundary_requires_settlement(first, first_prefixes[0]));
        assert!(zero_boundary_requires_settlement(first, first_prefixes[1]));

        // Once the terminal member has been authoritatively removed from the
        // living traveler list, the survivor's retry advances normally and no
        // zero-boundary settlement is repeated.
        let retry_prefixes = [12];
        let retry = common_movement_prefix(12, retry_prefixes);
        assert_eq!(retry, 12);
        assert!(!zero_boundary_requires_settlement(retry, retry_prefixes[0]));
    }

    #[test]
    fn journey_state_update_preserves_elected_successor_authority() {
        let mut fresh_party = Party {
            id: "party".into(),
            gateway_bucket: 0,
            name: "Travelers".into(),
            leader_id: 2,
            current_settlement_id: None,
            current_case_site_id: None,
            active_contract_id: None,
            is_solo: true,
            camp_fatigue_percent: 50,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: CampDurationMode::Auto,
            fixed_camp_minutes: 0,
            camp_destination: Some(JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id: "destination".into(),
                name: "Destination".into(),
            })),
            camp_remaining_minutes: 30,
            pooled_water_ml: 0.0,
            medicine_target: 0.0,
            command_target: 0.0,
            religion_target: 0.0,
        };
        set_party_journey_state(
            &mut fresh_party,
            Some("destination".into()),
            None,
            None,
            None,
            0,
        );
        assert_eq!(fresh_party.leader_id, 2);
        assert_eq!(
            fresh_party.current_settlement_id.as_deref(),
            Some("destination")
        );
        assert!(fresh_party.camp_destination.is_none());
        assert_eq!(
            fresh_party.leader_id, 2,
            "the successor can continue leading"
        );
        assert!(party_can_continue_travel(&fresh_party, 2));
        assert!(!party_can_continue_travel(&fresh_party, 1));
    }

    #[test]
    fn camp_origin_is_interpolated_from_persisted_route_progress() {
        let route = route_fixture();
        let persisted = PartyJourneyRoute {
            party_id: "party".into(),
            gateway_bucket: 0,
            package_digest: route.package_digest,
            distance_m: route.distance_m,
            minutes: route.minutes,
            points: route.points,
            spans: route.spans,
            return_route: route.return_route,
        };
        let midpoint = route_position_at_minute(&persisted, persisted.minutes / 2).unwrap();
        assert!((midpoint.0 - 10.005).abs() < 0.000_1);
        assert!((midpoint.1 - 53.0).abs() < 0.000_1);
    }
}

#[reducer]
pub fn travel_to_case_site(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: CaseSiteId,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    travel_to_case_site_impl(ctx, character_id, case_site_id.value, None)
}

#[reducer]
pub fn travel_to_case_site_planned(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: CaseSiteId,
    route: JourneyRoutePlan,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    travel_to_case_site_impl(ctx, character_id, case_site_id.value, Some(route))
}

fn travel_to_case_site_impl(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: String,
    route: Option<JourneyRoutePlan>,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to travel to a case site".into());
    };
    let Some(mut party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can travel".into());
    }
    require_no_unresolved_encounter(ctx, &party_id)?;
    if party.camp_destination.is_some() {
        return Err("Break camp and continue the current journey first".into());
    }
    let (site, _lead) = exact_case_site_for_observer(ctx, character_id, &case_site_id)
        .ok_or("That exact site has not been disclosed to this observer")?;
    if character.current_settlement_id.as_ref() != Some(&site.origin_settlement_id) {
        return Err("Travel to this site must begin at its known origin settlement".into());
    }
    require_party_ready(ctx, &party_id)?;
    let traveler_ids = living_party_member_ids(ctx, &party_id);
    let departure_minute = crate::time::synchronize_party_departure_time(ctx, &traveler_ids)?;
    party = revalidate_party_after_departure_sync(
        ctx,
        &party_id,
        character_id,
        Some(&site.origin_settlement_id),
        None,
        None,
    )?;
    let (site, lead) = exact_case_site_for_observer(ctx, character_id, &case_site_id)
        .ok_or("Exact destination knowledge changed during departure synchronization")?;
    let traveler_ids = living_party_member_ids(ctx, &party_id);

    let origin = ctx
        .db
        .settlement()
        .id()
        .find(&site.origin_settlement_id)
        .ok_or("Case-site origin settlement not found")?;
    let destination = (
        f64::from(lead.longitude_e7) / 10_000_000.0,
        f64::from(lead.latitude_e7) / 10_000_000.0,
    );
    if let Some(route) = route.as_ref() {
        validate_journey_route(ctx, route, (origin.coord_x, origin.coord_y), destination)?;
        validate_return_journey_route(ctx, route, destination, (origin.coord_x, origin.coord_y))?;
    }
    let distance_m = straight_line_distance_m(
        origin.coord_x,
        origin.coord_y,
        destination.0,
        destination.1,
        site.coordinates_are_geographic && origin.source_node_id.is_some(),
    );
    let travel_minutes = route
        .as_ref()
        .map_or_else(|| quest_journey_minutes(distance_m), |route| route.minutes);
    start_party_journey(
        ctx,
        &party,
        JourneyEndpoint::Settlement(JourneySettlementEndpoint {
            id: origin.id.clone(),
            name: origin.name.clone(),
        }),
        JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
            id: CaseSiteId {
                value: site.id.value.clone(),
            },
            name: site.name.clone(),
        }),
        travel_minutes,
        departure_minute,
        route.as_ref(),
    )?;
    crate::condition::prepare_party_waterskins(ctx, &party_id, true)?;
    for member_id in traveler_ids.iter().copied() {
        crate::condition::prepare_character_waterskins(ctx, member_id, true)?;
    }
    // Filling shared waterskins updates the persisted party row. Keep the
    // local copy in sync so the camp/location update below cannot restore the
    // pre-departure pooled-water value.
    party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party changed while its waterskins were filled")?;
    let proposed_leg_minutes =
        travel_minutes.min(party_next_walking_minutes(ctx, &party.id, travel_minutes)?);
    let (leg_minutes, encounter, next_roll) = advance_party_movement_until_encounter(
        ctx,
        &party_id,
        &traveler_ids,
        proposed_leg_minutes,
    )?;
    party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party changed during travel")?;
    let interrupted = encounter.is_some();
    if interrupted || leg_minutes < travel_minutes {
        for member_id in living_party_member_ids(ctx, &party_id) {
            let mut member = ctx
                .db
                .character()
                .id()
                .find(member_id)
                .ok_or("Party member not found")?;
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(ctx, member.id, None);
            ctx.db.character().id().update(member);
        }
        set_party_journey_state(
            &mut party,
            None,
            None,
            Some(case_site_id),
            Some("case_site".into()),
            travel_minutes.saturating_sub(leg_minutes),
        );
        ctx.db.party_authority().id().update(party);
        if interrupted {
            record_party_journey_interruption(ctx, &party_id, leg_minutes);
            commit_encounter_scan(ctx, &party_id, next_roll, encounter)?;
        } else {
            if leg_minutes > 0 {
                record_party_journey_camp(ctx, &party_id, leg_minutes)?;
            }
            commit_encounter_scan(ctx, &party_id, next_roll, None)?;
        }
        return Ok(());
    }
    for member_id in traveler_ids {
        if let Some(mut member) = ctx.db.character().id().find(member_id) {
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(
                ctx,
                member.id,
                Some(case_site_id.clone()),
            );
            ctx.db.character().id().update(member);
            mark_case_site_visited(ctx, member_id, &site)?;
        }
    }
    set_party_journey_state(
        &mut party,
        None,
        Some(CaseSiteId::from(case_site_id)),
        None,
        None,
        0,
    );
    ctx.db.party_authority().id().update(party);
    commit_case_site_arrival_objectives(ctx, &party_id, &site)?;
    finish_party_journey(ctx, &party_id);
    Ok(())
}

#[reducer]
pub fn travel_to_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    travel_to_settlement_impl(ctx, character_id, settlement_id, None)
}

#[reducer]
pub fn travel_to_settlement_planned(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    route: JourneyRoutePlan,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    travel_to_settlement_impl(ctx, character_id, settlement_id, Some(route))
}

fn travel_to_settlement_impl(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    route: Option<JourneyRoutePlan>,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let Some(destination) = ctx.db.settlement().id().find(&settlement_id) else {
        return Err("Settlement not found".into());
    };

    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let mut party = character
        .party_id
        .as_ref()
        .map(|party_id| {
            ctx.db
                .party_authority()
                .id()
                .find(party_id)
                .ok_or_else(|| "Party not found".to_string())
        })
        .transpose()?;
    if let Some(party) = party.as_ref() {
        if party.leader_id != character_id {
            return Err("Only the party leader can travel".into());
        }
        require_no_unresolved_encounter(ctx, &party.id)?;
    }

    // Choosing a different camp destination only changes the planned route.
    // The party can rest before it attempts the newly selected leg.
    if let Some(party) = party.as_mut()
        && party.camp_destination.is_some()
    {
        return redirect_camped_party_to_settlement(ctx, party, &destination, route);
    }

    if let Some(party) = party.as_ref() {
        // A defeated party can withdraw from an off-road quest location to
        // recover at a settlement, but may not begin ordinary travel while a
        // member is incapacitated.
        if party.current_case_site_id.is_none() {
            require_party_ready(ctx, &party.id)?;
        }
    } else {
        crate::condition::require_character_ready(ctx, character_id)?;
    }

    let (travel_minutes, origin_kind, origin_id, origin_name) =
        if let Some(origin_id) = &character.current_settlement_id {
            let Some(origin) = ctx.db.settlement().id().find(origin_id) else {
                return Err("Character's current settlement does not exist".into());
            };
            // Demo settlements remain usable before a Viabundus world is loaded.
            // Imported journeys must lead to the next settlement on the road graph.
            let minutes = if let (Some(origin_node), Some(destination_node)) =
                (origin.source_node_id, destination.source_node_id)
            {
                let Some(distance_m) = connected_settlement_distances(ctx, origin_node)
                    .get(&destination_node)
                    .copied()
                else {
                    return Err("That settlement is not directly connected by land or ferry".into());
                };
                journey_minutes(distance_m)
            } else {
                let distance_km = ((origin.coord_x - destination.coord_x).powi(2)
                    + (origin.coord_y - destination.coord_y).powi(2))
                .sqrt()
                .ceil() as u64;
                journey_minutes(distance_km.saturating_mul(METERS_PER_KILOMETER))
            };
            if let Some(route) = route.as_ref() {
                validate_journey_route(
                    ctx,
                    route,
                    (origin.coord_x, origin.coord_y),
                    (destination.coord_x, destination.coord_y),
                )?;
            }
            let minutes = route.as_ref().map_or(minutes, |route| route.minutes);
            (minutes, "settlement", origin.id, origin.name)
        } else if let Some(case_site_id) =
            crate::investigation::character_case_site_id(ctx, character_id)
        {
            let Some(site) = ctx.db.case_site_authority().id_key().find(case_site_id) else {
                return Err("Character's current case site does not exist".into());
            };
            let site_x = f64::from(site.longitude_e7) / 10_000_000.0;
            let site_y = f64::from(site.latitude_e7) / 10_000_000.0;
            let distance_m = straight_line_distance_m(
                site_x,
                site_y,
                destination.coord_x,
                destination.coord_y,
                site.coordinates_are_geographic && destination.source_node_id.is_some(),
            );
            if let Some(route) = route.as_ref() {
                validate_journey_route(
                    ctx,
                    route,
                    (site_x, site_y),
                    (destination.coord_x, destination.coord_y),
                )?;
            }
            (
                route
                    .as_ref()
                    .map_or_else(|| quest_journey_minutes(distance_m), |route| route.minutes),
                "case_site",
                site.id.value,
                site.name,
            )
        } else {
            return Err("Character is not at a known location".into());
        };

    let departing_case_site = crate::investigation::character_case_site_id(ctx, character_id);
    let traveler_ids: Vec<u64> = if let Some(party) = party.as_ref() {
        living_party_member_ids(ctx, &party.id)
    } else {
        vec![character_id]
    };
    let departure_minute = crate::time::synchronize_party_departure_time(ctx, &traveler_ids)?;
    if let Some(current_party) = party.as_ref() {
        party = Some(revalidate_party_after_departure_sync(
            ctx,
            &current_party.id,
            character_id,
            (origin_kind == "settlement").then_some(origin_id.as_str()),
            (origin_kind == "case_site").then_some(origin_id.as_str()),
            None,
        )?);
    }
    let traveler_ids: Vec<u64> = if let Some(party) = party.as_ref() {
        living_party_member_ids(ctx, &party.id)
    } else {
        vec![character_id]
    };
    if let Some(party) = party.as_ref() {
        start_party_journey(
            ctx,
            party,
            match origin_kind {
                "settlement" => JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                    id: origin_id.clone(),
                    name: origin_name.clone(),
                }),
                "case_site" => JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
                    id: CaseSiteId {
                        value: origin_id.clone(),
                    },
                    name: origin_name.clone(),
                }),
                _ => return Err("Journey origin kind is invalid".into()),
            },
            JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id: destination.id.clone(),
                name: destination.name.clone(),
            }),
            travel_minutes,
            departure_minute,
            route.as_ref(),
        )?;
    }
    let departing_settlement = character.current_settlement_id.is_some();
    if let Some(current_party) = party.as_ref() {
        crate::condition::prepare_party_waterskins(ctx, &current_party.id, departing_settlement)?;
        // prepare_party_waterskins persists the new volume. Reload before any
        // later camp/location write so that write preserves the filled water.
        party = Some(
            ctx.db
                .party_authority()
                .id()
                .find(&current_party.id)
                .ok_or("Party changed while its waterskins were prepared")?,
        );
    }
    for traveler_id in traveler_ids.iter().copied() {
        crate::condition::prepare_character_waterskins(ctx, traveler_id, departing_settlement)?;
    }
    let mut party_movement_committed = false;
    if let Some(current_party) = party.as_ref() {
        let party_id = current_party.id.clone();
        let proposed_leg_minutes =
            travel_minutes.min(party_next_walking_minutes(ctx, &party_id, travel_minutes)?);
        let (leg_minutes, encounter, next_roll) = advance_party_movement_until_encounter(
            ctx,
            &party_id,
            &traveler_ids,
            proposed_leg_minutes,
        )?;
        party = Some(
            ctx.db
                .party_authority()
                .id()
                .find(&party_id)
                .ok_or("Party changed during travel")?,
        );
        party_movement_committed = true;
        let interrupted = encounter.is_some();
        if interrupted || leg_minutes < travel_minutes {
            for traveler_id in living_party_member_ids(ctx, &party_id) {
                let mut traveler = ctx
                    .db
                    .character()
                    .id()
                    .find(traveler_id)
                    .ok_or("Party member not found")?;
                traveler.current_settlement_id = None;
                crate::investigation::set_character_case_site(ctx, traveler.id, None);
                ctx.db.character().id().update(traveler);
            }
            let party = party.as_mut().expect("party was just reloaded");
            set_party_journey_state(
                party,
                None,
                None,
                Some(settlement_id),
                Some("settlement".into()),
                travel_minutes.saturating_sub(leg_minutes),
            );
            ctx.db.party_authority().id().update(party.clone());
            if interrupted {
                record_party_journey_interruption(ctx, &party.id, leg_minutes);
                commit_encounter_scan(ctx, &party.id, next_roll, encounter)?;
            } else {
                if leg_minutes > 0 {
                    record_party_journey_camp(ctx, &party.id, leg_minutes)?;
                }
                commit_encounter_scan(ctx, &party.id, next_roll, None)?;
            }
            return Ok(());
        }
    }
    for traveler_id in traveler_ids {
        if !party_movement_committed && !advance_travel_time(ctx, traveler_id, travel_minutes)? {
            return Ok(());
        }
        let mut traveler = ctx
            .db
            .character()
            .id()
            .find(traveler_id)
            .ok_or("Party member not found")?;
        traveler.current_settlement_id = Some(settlement_id.clone());
        crate::investigation::set_character_case_site(ctx, traveler.id, None);
        ctx.db.character().id().update(traveler);
        crate::condition::replenish_needs_at_settlement(ctx, traveler_id)?;
        crate::condition::refresh_character_strategic_condition(ctx, traveler_id)?;
        crate::capability::refresh_character_capability(ctx, traveler_id)?;
        crate::time::rest_temporary_party_member_until_healed_at_settlement(ctx, traveler_id)?;
    }

    if let Some(ref mut party) = party {
        set_party_journey_state(party, Some(settlement_id.clone()), None, None, None, 0);
        ctx.db.party_authority().id().update(party.clone());
        finish_party_journey(ctx, &party.id);
        let departing_incident = departing_case_site.as_ref().and_then(|site_id| {
            ctx.db
                .strategic_incident()
                .iter()
                .find(|incident| {
                    incident.case_site_id.value == *site_id
                        && incident.status == IncidentStatus::Pending
                })
                .map(|incident| incident.id)
        });
        if let Some(incident_id) = departing_incident.as_ref() {
            finish_strategic_incident(ctx, incident_id, IncidentStatus::Avoided)?;
        }
        if departing_incident.is_none() {
            maybe_trigger_religious_incident(ctx, &party.id, &destination)?;
        }
    }

    Ok(())
}

#[reducer]
pub fn set_party_camp_fatigue_percent(
    ctx: &ReducerContext,
    character_id: u64,
    fatigue_percent: u8,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if !(10..=100).contains(&fatigue_percent) {
        return Err("Camp fatigue must be between 10% and 100%".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can configure travel".into());
    }
    party.camp_fatigue_percent = fatigue_percent;
    ctx.db.party_authority().id().update(party);
    Ok(())
}

#[reducer]
pub fn set_party_travel_itinerary(
    ctx: &ReducerContext,
    character_id: u64,
    walking_minutes_per_day: u16,
    travel_at_night: bool,
    automatic_camp_duration: bool,
    fixed_camp_minutes: u16,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if walking_minutes_per_day > 24 * 60
        || (walking_minutes_per_day > 0
            && daylight_walking_window(walking_minutes_per_day).is_none())
    {
        return Err("Daily walking time must be between 0 and 24 hours".into());
    }
    // Retain the reducer's wire shape for existing clients while the daily
    // walking window becomes the sole authoritative configuration.
    let _legacy_camp_override = (automatic_camp_duration, fixed_camp_minutes);
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can configure travel".into());
    }
    party.walking_minutes_per_day = walking_minutes_per_day;
    party.travel_at_night = travel_at_night;
    // The daily cycle has one degree of freedom: all time outside the
    // walking window is camp/downtime.
    party.camp_duration_mode = CampDurationMode::Fixed;
    party.fixed_camp_minutes = (24 * 60_u16).saturating_sub(walking_minutes_per_day);
    let camped = party.camp_destination.is_some();
    ctx.db.party_authority().id().update(party);
    if camped {
        refresh_party_journey_forecast(ctx, &party_id)?;
    }
    Ok(())
}

/// Advance a single planned leg from a camp. A journey remains a strategic
/// state, rather than a tactical simulation: the UI animates this instantaneous
/// transition between pins.
#[reducer]
pub fn continue_camp_travel(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if !party_can_continue_travel(&party, character_id) {
        return Err("Only the party leader can continue travel".into());
    }
    require_no_unresolved_encounter(ctx, &party_id)?;
    let destination = party
        .camp_destination
        .clone()
        .ok_or("The party is not camped")?;
    // This also upgrades pre elapsed-itinerary rows before any celestial or
    // progress coordinates are used.
    refresh_party_journey_forecast(ctx, &party_id)?;
    let proposed_leg_minutes = party.camp_remaining_minutes.min(party_next_walking_minutes(
        ctx,
        &party.id,
        party.camp_remaining_minutes,
    )?);
    if proposed_leg_minutes == 0 {
        return Err("Rest until the party reaches its next daylight walking window".into());
    }
    let traveler_ids = living_party_member_ids(ctx, &party_id);
    let (leg_minutes, encounter, next_roll) = advance_party_movement_until_encounter(
        ctx,
        &party_id,
        &traveler_ids,
        proposed_leg_minutes,
    )?;
    party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party changed during travel")?;
    let interrupted = encounter.is_some();
    party.camp_remaining_minutes = party.camp_remaining_minutes.saturating_sub(leg_minutes);
    if interrupted || party.camp_remaining_minutes > 0 {
        ctx.db.party_authority().id().update(party);
        if interrupted {
            record_party_journey_interruption(ctx, &party_id, leg_minutes);
            commit_encounter_scan(ctx, &party_id, next_roll, encounter)?;
        } else {
            if leg_minutes > 0 {
                record_party_journey_camp(ctx, &party_id, leg_minutes)?;
            }
            commit_encounter_scan(ctx, &party_id, next_roll, None)?;
        }
        return Ok(());
    }
    match destination {
        JourneyEndpoint::Settlement(endpoint) => {
            let destination_id = endpoint.id;
            let _destination = ctx
                .db
                .settlement()
                .id()
                .find(&destination_id)
                .ok_or("Camp destination settlement not found")?;
            for member_id in traveler_ids.iter().copied() {
                let mut member = ctx
                    .db
                    .character()
                    .id()
                    .find(member_id)
                    .ok_or("Party member not found")?;
                member.current_settlement_id = Some(destination_id.clone());
                crate::investigation::set_character_case_site(ctx, member.id, None);
                ctx.db.character().id().update(member);
                crate::condition::replenish_needs_at_settlement(ctx, member_id)?;
                crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
                crate::time::rest_temporary_party_member_until_healed_at_settlement(
                    ctx, member_id,
                )?;
            }
            party.current_settlement_id = Some(destination_id);
            party.current_case_site_id = None;
        }
        JourneyEndpoint::CaseSite(endpoint) => {
            let destination_id = endpoint.id.value;
            let site = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&destination_id)
                .ok_or("Camp destination case site not found")?;
            for member_id in traveler_ids.iter().copied() {
                let mut member = ctx
                    .db
                    .character()
                    .id()
                    .find(member_id)
                    .ok_or("Party member not found")?;
                member.current_settlement_id = None;
                crate::investigation::set_character_case_site(
                    ctx,
                    member.id,
                    Some(destination_id.clone()),
                );
                ctx.db.character().id().update(member);
                mark_case_site_visited(ctx, member_id, &site)?;
                crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
            }
            party.current_settlement_id = None;
            party.current_case_site_id = Some(CaseSiteId::from(destination_id));
        }
        JourneyEndpoint::Camp(_) => return Err("A camp cannot be a journey destination".into()),
    }
    let current_settlement_id = party.current_settlement_id.clone();
    let current_case_site_id = party.current_case_site_id.clone();
    set_party_journey_state(
        &mut party,
        current_settlement_id,
        current_case_site_id,
        None,
        None,
        0,
    );
    ctx.db.party_authority().id().update(party);
    if let Some(arrived_site) = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .and_then(|party| party.current_case_site_id)
        .and_then(|site_id| ctx.db.case_site_authority().id_key().find(&site_id.value))
    {
        commit_case_site_arrival_objectives(ctx, &party_id, &arrived_site)?;
    }
    finish_party_journey(ctx, &party_id);
    Ok(())
}

/// Converts a trusted mission outcome into a typed strategic fact. This is the
/// only battle-to-case seam: tactical code cannot resolve a case or pay a
/// contract directly.
fn ingest_hostile_group_defeat_fact(
    ctx: &ReducerContext,
    outcome_source_id: &str,
    party_id: &str,
    group: &HostileGroupAuthority,
    count: u32,
) -> Result<(), String> {
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&group.case_site_id.value)
        .ok_or("Hostile group has no case site")?;
    let Some(_) = ctx.db.case_authority().id().find(&site.case_id) else {
        // Incidents and random encounters intentionally have no case.
        return Ok(());
    };
    ingest_case_outcome_fact(
        ctx,
        outcome_source_id,
        &site.case_id,
        party_id,
        adventuresim_core::case::OutcomeFactKind::HostilesDefeated {
            hostile_group_id: group.id.clone(),
            count,
        },
    )
}

fn custody_object(
    kind: CustodyObjectKind,
    object_id: &str,
) -> Result<adventuresim_core::case::CustodyObject, String> {
    match kind {
        CustodyObjectKind::Asset => adventuresim_core::case::AssetId::new(object_id)
            .map(adventuresim_core::case::CustodyObject::Asset)
            .map_err(|_| "Custody asset ID is invalid".into()),
        CustodyObjectKind::Subject => adventuresim_core::case::SubjectId::new(object_id)
            .map(adventuresim_core::case::CustodyObject::Subject)
            .map_err(|_| "Custody subject ID is invalid".into()),
    }
}

fn custody_holder(
    kind: CustodyHolderKind,
    holder_id: &str,
) -> Result<adventuresim_core::case::CustodyHolder, String> {
    if holder_id.len() > 160 {
        return Err("Custody holder ID is too long".into());
    }
    match kind {
        CustodyHolderKind::Site if !holder_id.is_empty() => Ok(
            adventuresim_core::case::CustodyHolder::Site(holder_id.into()),
        ),
        CustodyHolderKind::Party if !holder_id.is_empty() => Ok(
            adventuresim_core::case::CustodyHolder::Party(holder_id.into()),
        ),
        CustodyHolderKind::Character => holder_id
            .parse()
            .map(adventuresim_core::case::CustodyHolder::Character)
            .map_err(|_| "Custody character ID is invalid".into()),
        CustodyHolderKind::Npc if !holder_id.is_empty() => Ok(
            adventuresim_core::case::CustodyHolder::Npc(holder_id.into()),
        ),
        CustodyHolderKind::Destroyed if holder_id.is_empty() => {
            Ok(adventuresim_core::case::CustodyHolder::Destroyed)
        }
        CustodyHolderKind::Released if holder_id.is_empty() => {
            Ok(adventuresim_core::case::CustodyHolder::Released)
        }
        _ => Err("Custody holder ID does not match its typed holder".into()),
    }
}

/// Sole typed custody transition. Domain adapters provide the corresponding
/// outcome fact only after the core custody state machine accepts the move.
fn transition_case_custody(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
    holder_kind: CustodyHolderKind,
    holder_id: &str,
    version: u32,
    fact: Option<adventuresim_core::case::OutcomeFactKind>,
) -> Result<bool, String> {
    let object = custody_object(object_kind, object_id)?;
    let next = adventuresim_core::case::CustodyRecord {
        case_id: adventuresim_core::case::CaseId::new(case_id)
            .map_err(|_| "Custody case ID is invalid")?,
        object: object.clone(),
        holder: custody_holder(holder_kind, holder_id)?,
        version,
        source_id: source_id.to_string(),
    };
    if let Some(existing) = ctx
        .db
        .case_custody()
        .source_id()
        .find(&source_id.to_string())
    {
        return if existing.case_id == case_id
            && existing.object_id == object_id
            && existing.object_kind == object_kind
            && existing.holder_kind == holder_kind
            && existing.holder_id == holder_id
            && existing.version == version
        {
            Ok(false)
        } else {
            Err("Conflicting retry for custody source".into())
        };
    }
    let mut records = BTreeMap::new();
    if let Some(current) = ctx
        .db
        .case_custody()
        .object_id()
        .find(&object_id.to_string())
    {
        records.insert(
            custody_object(current.object_kind, &current.object_id)?,
            adventuresim_core::case::CustodyRecord {
                case_id: adventuresim_core::case::CaseId::new(current.case_id)
                    .map_err(|_| "Stored custody case ID is invalid")?,
                object: object.clone(),
                holder: custody_holder(current.holder_kind, &current.holder_id)?,
                version: current.version,
                source_id: current.source_id,
            },
        );
    }
    if !adventuresim_core::case::apply_custody(&mut records, next).map_err(str::to_string)? {
        return Ok(false);
    }
    let row = CaseCustody {
        object_id: object_id.to_string(),
        case_id: case_id.to_string(),
        object_kind,
        holder_kind,
        holder_id: holder_id.to_string(),
        version,
        source_id: source_id.to_string(),
    };
    if ctx
        .db
        .case_custody()
        .object_id()
        .find(&row.object_id)
        .is_some()
    {
        ctx.db.case_custody().object_id().update(row);
    } else {
        ctx.db.case_custody().insert(row);
    }
    if let Some(fact) = fact {
        ingest_case_outcome_fact(
            ctx,
            &format!("custody:{source_id}"),
            case_id,
            party_id,
            fact,
        )?;
    }
    if !party_id.is_empty() {
        ensure_objective_continuity_guards(ctx, party_id, case_id)?;
        reconcile_party_objective_continuity(ctx, party_id)?;
    }
    if matches!(
        holder_kind,
        CustodyHolderKind::Destroyed | CustodyHolderKind::Released
    ) {
        emit_terminal_custody_impossibility(
            ctx,
            source_id,
            case_id,
            party_id,
            object_kind,
            object_id,
            holder_kind,
        )?;
    }
    Ok(true)
}

fn seed_case_custody(
    ctx: &ReducerContext,
    case_id: &str,
    site_id: &str,
    expression: &adventuresim_core::case::ObjectiveExpression,
) -> Result<(), String> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    for requirement in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .map(|objective| &objective.requirement)
    {
        let (kind, object_id) = match requirement {
            R::Retrieve { asset_id }
            | R::Return { asset_id, .. }
            | R::Exchange { asset_id, .. } => (CustodyObjectKind::Asset, asset_id.as_str()),
            R::Capture { subject_id }
            | R::Rescue { subject_id }
            | R::EscortTo { subject_id, .. }
            | R::Protect { subject_id, .. }
            | R::Release { subject_id } => (CustodyObjectKind::Subject, subject_id.as_str()),
            _ => continue,
        };
        if ctx
            .db
            .case_custody()
            .object_id()
            .find(&object_id.to_string())
            .is_none()
        {
            transition_case_custody(
                ctx,
                &format!("spawn:{case_id}:{object_id}"),
                case_id,
                "",
                kind,
                object_id,
                CustodyHolderKind::Site,
                site_id,
                0,
                None,
            )?;
        }
    }
    Ok(())
}

fn party_strategic_minute(ctx: &ReducerContext, party_id: &str) -> Result<u64, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    Ok(ctx
        .db
        .character_time()
        .character_id()
        .find(party.leader_id)
        .map_or(0, |time| time.minutes))
}

fn open_case_expression(
    ctx: &ReducerContext,
    case_id: &str,
) -> Result<Option<adventuresim_core::case::ObjectiveExpression>, String> {
    let Some(case) = ctx.db.case_authority().id().find(&case_id.to_string()) else {
        return Ok(None);
    };
    if case.resolution_status != CaseResolutionStatus::Open {
        return Ok(None);
    }
    serde_json::from_str(&case.objective_expression_json)
        .map(Some)
        .map_err(|_| "Case objective authority is invalid".into())
}

fn ensure_objective_continuity_guards(
    ctx: &ReducerContext,
    party_id: &str,
    case_id: &str,
) -> Result<(), String> {
    let Some(expression) = open_case_expression(ctx, case_id)? else {
        return Ok(());
    };
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let now = party_strategic_minute(ctx, party_id)?;
    use adventuresim_core::case::ObjectiveRequirement as R;
    for objective in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let (kind, site_id, subject_id, custody_version, through, valid_now) =
            match &objective.requirement {
                R::SurviveWindow {
                    site_id,
                    through_minute,
                } => (
                    ObjectiveContinuityKind::SurviveAtSite,
                    site_id.clone(),
                    String::new(),
                    None,
                    *through_minute,
                    party.current_case_site_id.as_deref() == Some(site_id),
                ),
                R::Protect {
                    subject_id,
                    through_minute,
                } => {
                    let custody = ctx
                        .db
                        .case_custody()
                        .object_id()
                        .find(&subject_id.as_str().to_string());
                    let valid = custody.as_ref().is_some_and(|row| {
                        row.case_id == case_id
                            && row.holder_kind == CustodyHolderKind::Party
                            && row.holder_id == party_id
                    });
                    (
                        ObjectiveContinuityKind::ProtectSubject,
                        String::new(),
                        subject_id.as_str().to_string(),
                        custody.map(|row| row.version),
                        *through_minute,
                        valid,
                    )
                }
                _ => continue,
            };
        if !valid_now || now > through {
            continue;
        }
        let has_active = ctx
            .db
            .objective_continuity_guard()
            .party_id()
            .filter(party_id)
            .any(|guard| {
                guard.case_id == case_id
                    && guard.objective_id == objective.id.as_str()
                    && guard.broken_at_minute.is_none()
                    && !guard.completed
            });
        if has_active {
            continue;
        }
        let id = format!(
            "continuity:{case_id}:{}:{party_id}:{now}",
            objective.id.as_str()
        );
        ctx.db
            .objective_continuity_guard()
            .insert(ObjectiveContinuityGuard {
                id,
                party_id: party_id.to_string(),
                case_id: case_id.to_string(),
                objective_id: objective.id.as_str().to_string(),
                kind,
                site_id,
                subject_id,
                custody_version,
                started_at_minute: now,
                through_minute: through,
                broken_at_minute: None,
                completed: false,
            });
    }
    Ok(())
}

pub(crate) fn reconcile_party_objective_continuity(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let Some(party) = ctx.db.party_authority().id().find(&party_id.to_string()) else {
        return Ok(());
    };
    let now = party_strategic_minute(ctx, party_id)?;
    let mut touched_cases = HashSet::new();
    let guards: Vec<_> = ctx
        .db
        .objective_continuity_guard()
        .party_id()
        .filter(party_id)
        .filter(|guard| guard.broken_at_minute.is_none() && !guard.completed)
        .collect();
    for mut guard in guards {
        touched_cases.insert(guard.case_id.clone());
        let valid = match guard.kind {
            ObjectiveContinuityKind::SurviveAtSite => {
                party.current_case_site_id.as_deref() == Some(&guard.site_id)
            }
            ObjectiveContinuityKind::ProtectSubject => ctx
                .db
                .case_custody()
                .object_id()
                .find(&guard.subject_id)
                .is_some_and(|custody| {
                    custody.case_id == guard.case_id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                        && Some(custody.version) == guard.custody_version
                }),
        };
        if !valid {
            guard.broken_at_minute = Some(now);
            ctx.db.objective_continuity_guard().id().update(guard);
            continue;
        }
        if now < guard.through_minute {
            continue;
        }
        let Some(expression) = open_case_expression(ctx, &guard.case_id)? else {
            continue;
        };
        let objective = expression
            .alternatives
            .iter()
            .flat_map(|path| &path.objectives)
            .find(|objective| objective.id.as_str() == guard.objective_id)
            .ok_or("Continuity objective no longer exists")?;
        use adventuresim_core::case::{ObjectiveRequirement as R, OutcomeFactKind as F};
        let fact = match (&guard.kind, &objective.requirement) {
            (
                ObjectiveContinuityKind::SurviveAtSite,
                R::SurviveWindow {
                    site_id,
                    through_minute,
                },
            ) if site_id == &guard.site_id && *through_minute == guard.through_minute => {
                F::WindowSurvived {
                    site_id: site_id.clone(),
                    through_minute: *through_minute,
                }
            }
            (
                ObjectiveContinuityKind::ProtectSubject,
                R::Protect {
                    subject_id,
                    through_minute,
                },
            ) if subject_id.as_str() == guard.subject_id
                && *through_minute == guard.through_minute =>
            {
                F::SubjectProtected {
                    subject_id: subject_id.clone(),
                    through_minute: *through_minute,
                }
            }
            _ => return Err("Continuity guard no longer matches objective authority".into()),
        };
        ingest_case_outcome_fact(
            ctx,
            &format!("timed-objective:{}", guard.id),
            &guard.case_id,
            party_id,
            fact,
        )?;
        guard.completed = true;
        ctx.db.objective_continuity_guard().id().update(guard);
    }
    for case_id in touched_cases {
        ensure_objective_continuity_guards(ctx, party_id, &case_id)?;
    }
    Ok(())
}

fn break_party_objective_continuity(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let now = party_strategic_minute(ctx, party_id)?;
    let guards: Vec<_> = ctx
        .db
        .objective_continuity_guard()
        .party_id()
        .filter(party_id)
        .filter(|guard| guard.broken_at_minute.is_none() && !guard.completed)
        .collect();
    for mut guard in guards {
        guard.broken_at_minute = Some(now);
        ctx.db.objective_continuity_guard().id().update(guard);
    }
    Ok(())
}

fn commit_case_site_arrival_objectives(
    ctx: &ReducerContext,
    party_id: &str,
    site: &CaseSiteAuthority,
) -> Result<(), String> {
    let Some(expression) = open_case_expression(ctx, &site.case_id)? else {
        return Ok(());
    };
    use adventuresim_core::case::{ObjectiveRequirement as R, OutcomeFactKind as F};
    for objective in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let R::EscortTo {
            subject_id,
            site_id,
        } = &objective.requirement
        else {
            continue;
        };
        if site_id != &site.id.value {
            continue;
        }
        let current = ctx
            .db
            .case_custody()
            .object_id()
            .find(&subject_id.as_str().to_string())
            .ok_or("Escorted subject has no custody authority")?;
        if current.case_id != site.case_id
            || current.holder_kind != CustodyHolderKind::Party
            || current.holder_id != party_id
        {
            continue;
        }
        transition_case_custody(
            ctx,
            &format!(
                "arrival:{}:{party_id}:{}:{}",
                site.case_id,
                site.id.value,
                objective.id.as_str()
            ),
            &site.case_id,
            party_id,
            CustodyObjectKind::Subject,
            subject_id.as_str(),
            CustodyHolderKind::Site,
            &site.id.value,
            current.version.saturating_add(1),
            Some(F::SubjectEscorted {
                subject_id: subject_id.clone(),
                site_id: site_id.clone(),
            }),
        )?;
    }
    ensure_objective_continuity_guards(ctx, party_id, &site.case_id)?;
    reconcile_party_objective_continuity(ctx, party_id)
}

fn emit_terminal_custody_impossibility(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
    terminal_holder: CustodyHolderKind,
) -> Result<(), String> {
    let Some(expression) = open_case_expression(ctx, case_id)? else {
        return Ok(());
    };
    use adventuresim_core::case::{ObjectiveRequirement as R, OutcomeFactKind as F};
    for objective in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let affected = match (&objective.requirement, object_kind, terminal_holder) {
            (R::Retrieve { asset_id }, CustodyObjectKind::Asset, CustodyHolderKind::Destroyed)
            | (
                R::Return { asset_id, .. },
                CustodyObjectKind::Asset,
                CustodyHolderKind::Destroyed,
            )
            | (
                R::Exchange { asset_id, .. },
                CustodyObjectKind::Asset,
                CustodyHolderKind::Destroyed,
            ) => asset_id.as_str() == object_id,
            (R::Capture { subject_id }, CustodyObjectKind::Subject, _)
            | (R::Rescue { subject_id }, CustodyObjectKind::Subject, _)
            | (R::EscortTo { subject_id, .. }, CustodyObjectKind::Subject, _)
            | (R::Protect { subject_id, .. }, CustodyObjectKind::Subject, _) => {
                subject_id.as_str() == object_id
            }
            (
                R::Release { subject_id },
                CustodyObjectKind::Subject,
                CustodyHolderKind::Destroyed,
            ) => subject_id.as_str() == object_id,
            _ => false,
        };
        if !affected {
            continue;
        }
        if ctx
            .db
            .case_authority()
            .id()
            .find(&case_id.to_string())
            .is_none_or(|case| case.resolution_status != CaseResolutionStatus::Open)
        {
            break;
        }
        ingest_case_outcome_fact(
            ctx,
            &format!("{source_id}:impossible:{}", objective.id.as_str()),
            case_id,
            party_id,
            F::ObjectiveImpossible {
                objective_id: objective.id.clone(),
            },
        )?;
    }
    Ok(())
}

/// Trusted destruction/death/expiry adapter. Callers must bind `source_id` to
/// the concrete domain event; there is intentionally no public reducer.
pub(crate) fn record_case_object_destroyed(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
    version: u32,
) -> Result<bool, String> {
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        object_kind,
        object_id,
        CustodyHolderKind::Destroyed,
        "",
        version,
        None,
    )
}

pub(crate) fn record_asset_retrieved(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    asset_id: &str,
    version: u32,
) -> Result<bool, String> {
    let asset = adventuresim_core::case::AssetId::new(asset_id)
        .map_err(|_| "Custody asset ID is invalid")?;
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        CustodyObjectKind::Asset,
        asset_id,
        CustodyHolderKind::Party,
        party_id,
        version,
        Some(adventuresim_core::case::OutcomeFactKind::AssetRetrieved { asset_id: asset }),
    )
}

pub(crate) fn record_asset_returned_or_exchanged(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    asset_id: &str,
    recipient_id: &str,
    version: u32,
    exchange: bool,
) -> Result<bool, String> {
    let asset = adventuresim_core::case::AssetId::new(asset_id)
        .map_err(|_| "Custody asset ID is invalid")?;
    let fact = if exchange {
        adventuresim_core::case::OutcomeFactKind::AssetExchanged {
            asset_id: asset,
            recipient_id: recipient_id.into(),
        }
    } else {
        adventuresim_core::case::OutcomeFactKind::AssetReturned {
            asset_id: asset,
            custodian_id: recipient_id.into(),
        }
    };
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        CustodyObjectKind::Asset,
        asset_id,
        CustodyHolderKind::Npc,
        recipient_id,
        version,
        Some(fact),
    )
}

pub(crate) fn record_subject_rescued_or_released(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    subject_id: &str,
    version: u32,
    release: bool,
) -> Result<bool, String> {
    let subject = adventuresim_core::case::SubjectId::new(subject_id)
        .map_err(|_| "Custody subject ID is invalid")?;
    let fact = if release {
        adventuresim_core::case::OutcomeFactKind::SubjectReleased {
            subject_id: subject,
        }
    } else {
        adventuresim_core::case::OutcomeFactKind::SubjectRescued {
            subject_id: subject,
        }
    };
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        CustodyObjectKind::Subject,
        subject_id,
        if release {
            CustodyHolderKind::Released
        } else {
            CustodyHolderKind::Party
        },
        if release { "" } else { party_id },
        version,
        Some(fact),
    )
}

/// Sole trusted fact ingestion and evaluation seam. Callers must already have
/// authenticated their strategic authority and bind `source_id` to a durable
/// domain receipt; no public reducer exposes this function.
pub(crate) fn ingest_case_outcome_fact(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    kind: adventuresim_core::case::OutcomeFactKind,
) -> Result<(), String> {
    let mut case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id.to_string())
        .ok_or("Case not found")?;
    let fact_id = format!(
        "fact:{}",
        source_id.strip_prefix("outcome:").unwrap_or(source_id)
    );
    let fact = adventuresim_core::case::OutcomeFact {
        id: adventuresim_core::case::OutcomeFactId::new(fact_id.clone())
            .map_err(|_| "Outcome fact ID is invalid")?,
        case_id: adventuresim_core::case::CaseId::new(case.id.clone())
            .map_err(|_| "Case ID is invalid")?,
        party_id: party_id.to_string(),
        source_id: source_id.to_string(),
        happened_at: crate::time::refresh_clock(ctx)?,
        kind,
    };
    let encoded = serde_json::to_string(&fact).map_err(|_| "Could not encode outcome fact")?;
    if let Some(existing) = ctx
        .db
        .case_outcome_fact()
        .source_id()
        .find(&source_id.to_string())
    {
        return if existing.case_id == case.id
            && existing.party_id == party_id
            && existing.fact_json == encoded
        {
            Ok(())
        } else {
            Err("Conflicting retry for case outcome source".into())
        };
    }
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
    }
    ctx.db.case_outcome_fact().insert(CaseOutcomeFact {
        id: fact_id,
        case_id: case.id.clone(),
        party_id: party_id.to_string(),
        source_id: source_id.to_string(),
        fact_json: encoded,
        happened_at_minute: fact.happened_at,
    });

    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| {
            serde_json::from_str::<adventuresim_core::case::OutcomeFact>(&row.fact_json)
                .map_err(|_| "Stored outcome fact is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let core_case_id =
        adventuresim_core::case::CaseId::new(case.id.clone()).map_err(|_| "Case ID is invalid")?;
    let evaluation = expression.evaluate(&core_case_id, party_id, &facts);
    if evaluation.state == adventuresim_core::case::EvaluationState::Pending {
        return Ok(());
    }

    let now = crate::time::refresh_clock(ctx)?;
    let winning_path_index = (evaluation.state
        == adventuresim_core::case::EvaluationState::Satisfied)
        .then(|| {
            evaluation.alternatives.iter().position(|path| {
                path.iter().all(|progress| {
                    progress.state == adventuresim_core::case::EvaluationState::Satisfied
                })
            })
        })
        .flatten()
        .and_then(|index| u16::try_from(index).ok());
    case.resolution_status =
        if evaluation.state == adventuresim_core::case::EvaluationState::Satisfied {
            CaseResolutionStatus::Resolved
        } else {
            CaseResolutionStatus::Failed
        };
    case.resolved_by_party_id = Some(party_id.to_string());
    ctx.db.case_authority().id().update(case.clone());

    for mut contract in ctx
        .db
        .contract_authority()
        .case_id()
        .filter(&case.id)
        .collect::<Vec<_>>()
    {
        if case.resolution_status == CaseResolutionStatus::Resolved
            && contract.status == ContractStatus::Accepted
            && contract.accepted_by.as_deref() == Some(party_id)
        {
            contract.status = ContractStatus::ReadyToReport;
        } else if matches!(
            contract.status,
            ContractStatus::Offered | ContractStatus::Accepted
        ) {
            contract.status = ContractStatus::Withdrawn;
        }
        ctx.db.contract_authority().id().update(contract);
    }

    let selected_finale_id =
        select_case_finale(ctx, &case.id, case.resolution_status, winning_path_index)?
            .unwrap_or_default();
    ctx.db.case_outcome().insert(CaseOutcome {
        case_id: case.id.clone(),
        party_id: party_id.to_string(),
        status: case.resolution_status,
        winning_path_index,
        resolved_at_minute: now,
        selected_finale_id: selected_finale_id.clone(),
        finale_executed: false,
    });
    if !selected_finale_id.is_empty() {
        execute_case_finale(
            ctx,
            &selected_finale_id,
            &format!("finale:{source_id}"),
            party_id,
        )?;
    }
    if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(&case.id)
        && let Ok(context) = serde_json::from_str::<
            adventuresim_core::quest_generation::GenerationContext,
        >(&authority.context_snapshot_json)
    {
        ensure_settlement_activity_inner(ctx, &context.settlement_id)?;
    }
    Ok(())
}

fn select_case_finale(
    ctx: &ReducerContext,
    case_id: &str,
    resolution_status: CaseResolutionStatus,
    winning_path_index: Option<u16>,
) -> Result<Option<String>, String> {
    let mut finales = ctx
        .db
        .case_finale_authority()
        .case_id()
        .filter(&case_id.to_string())
        .collect::<Vec<_>>();
    finales.sort_by_key(|finale| (finale.priority, finale.id.clone()));
    let selected = finales
        .iter()
        .find(|finale| {
            finale.status == FinaleStatus::Available
                && finale.resolution_status == resolution_status
                && finale
                    .eligible_path_index
                    .is_none_or(|path| Some(path) == winning_path_index)
        })
        .map(|finale| finale.id.clone());
    for mut finale in finales {
        finale.status = if selected.as_deref() == Some(&finale.id) {
            FinaleStatus::Selected
        } else {
            FinaleStatus::Ineligible
        };
        ctx.db.case_finale_authority().id().update(finale);
    }
    Ok(selected)
}

fn execute_case_finale(
    ctx: &ReducerContext,
    finale_id: &str,
    source_id: &str,
    party_id: &str,
) -> Result<(), String> {
    if let Some(existing) = ctx
        .db
        .case_finale_execution()
        .finale_id()
        .find(&finale_id.to_string())
    {
        return if existing.source_id == source_id && existing.party_id == party_id {
            Ok(())
        } else {
            Err("Finale already executed by different authority".into())
        };
    }
    let mut finale = ctx
        .db
        .case_finale_authority()
        .id()
        .find(&finale_id.to_string())
        .ok_or("Finale not found")?;
    if finale.status != FinaleStatus::Selected {
        return Err("Finale is not selected".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&finale.case_id)
        .ok_or("Finale case not found")?;
    let now = crate::time::refresh_clock(ctx)?;
    if finale.kind == FinaleKind::ResolveLocalProblem
        && case.resolution_status == CaseResolutionStatus::Resolved
        && let Some(problem_id) = case.local_problem_id
    {
        crate::local_problem::apply_outcome(
            ctx,
            &problem_id,
            &crate::local_problem::LocalProblemOutcomeInput {
                source_outcome_id: source_id.to_string(),
                at_minute: now,
                mitigation_bps: 10_000,
                resolve: true,
            },
        )?;
    }
    finale.status = FinaleStatus::Executed;
    ctx.db.case_finale_authority().id().update(finale.clone());
    ctx.db.case_finale_execution().insert(CaseFinaleExecution {
        finale_id: finale.id.clone(),
        source_id: source_id.to_string(),
        case_id: finale.case_id,
        party_id: party_id.to_string(),
        executed_at_minute: now,
    });
    if let Some(mut outcome) = ctx.db.case_outcome().case_id().find(&case.id) {
        outcome.finale_executed = true;
        ctx.db.case_outcome().case_id().update(outcome);
    }
    Ok(())
}

fn hostile_resolution_for_objective(
    requirement: &adventuresim_core::case::ObjectiveRequirement,
    hostile_group_id: &str,
) -> Option<(HostileResolutionKind, u32)> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    match requirement {
        R::Defeat {
            hostile_group_id: id,
            ..
        } if id == hostile_group_id => Some((HostileResolutionKind::Defeated, 50)),
        R::DriveOff {
            hostile_group_id: id,
        } if id == hostile_group_id => Some((HostileResolutionKind::DrivenOff, 30)),
        _ => None,
    }
}

fn mission_candidate_from_capability(
    mission_id: &str,
    index: usize,
    capability: MissionApproachCapability,
) -> MissionOutcomeCandidate {
    MissionOutcomeCandidate {
        id: format!("{mission_id}:candidate:{index:03}"),
        mission_id: mission_id.to_string(),
        capability_id: capability.id,
        case_id: capability.case_id,
        case_site_id: capability.case_site_id,
        hostile_group_id: capability.hostile_group_id,
        path_index: capability.path_index,
        objective_id: capability.objective_id,
        resolution: capability.resolution,
        weight: capability.weight,
        capture_subject_id: capability.capture_subject_id,
        capture_custody_version: capability.capture_custody_version,
    }
}

pub(crate) fn ensure_bound_mission_authority(
    ctx: &ReducerContext,
    mission_id: &str,
    party_id: &str,
    observer_character_id: u64,
    case_site: &CaseSiteAuthority,
    scene_key: &str,
) -> Result<MissionAuthority, String> {
    if let Some(existing) = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id.to_string())
    {
        return if existing.party_id == party_id
            && existing.observer_character_id == observer_character_id
            && existing.case_site_id.as_ref() == Some(&case_site.id)
            && existing.scene_key == scene_key
            && existing.status == MissionAttemptStatus::Bound
        {
            Ok(existing)
        } else if existing.status != MissionAttemptStatus::Bound {
            Err("Mission ID is already terminal and cannot be reused".into())
        } else {
            Err("Mission ID is already bound to different authority".into())
        };
    }
    if exact_case_site_for_observer(ctx, observer_character_id, &case_site.id.value).is_none() {
        return Err("Mission observer does not know or have a visited exact case site".into());
    }
    let group = ctx
        .db
        .hostile_group_authority()
        .iter()
        .find(|group| group.case_site_id == case_site.id)
        .ok_or("Case site has no materialized hostile group")?;
    if group.disposition != HostileGroupDisposition::Active {
        return Err("Hostile group is already resolved".into());
    }
    let hostile_group_id = group.id;
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_site.case_id)
        .ok_or("Case authority no longer exists")?;
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
    }
    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| {
            serde_json::from_str::<adventuresim_core::case::OutcomeFact>(&row.fact_json)
                .map_err(|_| "Stored outcome fact is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let core_case_id =
        adventuresim_core::case::CaseId::new(case.id.clone()).map_err(|_| "Case ID is invalid")?;
    let evaluation = expression.evaluate(&core_case_id, party_id, &facts);
    use adventuresim_core::case::ObjectiveRequirement as R;
    let mut capabilities = Vec::new();
    for (path_index, path) in expression.alternatives.iter().enumerate() {
        let Some(progress) = evaluation.alternatives.get(path_index) else {
            return Err("Case objective evaluation shape is invalid".into());
        };
        for (objective_index, objective) in path.objectives.iter().enumerate() {
            if progress.get(objective_index).is_none_or(|progress| {
                progress.state != adventuresim_core::case::EvaluationState::Pending
            }) {
                continue;
            }
            let (resolution, weight, capture_subject_id, capture_custody_version) =
                if let Some((resolution, weight)) =
                    hostile_resolution_for_objective(&objective.requirement, &hostile_group_id)
                {
                    (resolution, weight, None, None)
                } else {
                    match &objective.requirement {
                        R::Capture { subject_id } => {
                            let subject = subject_id.as_str().to_string();
                            let Some(custody) = ctx.db.case_custody().object_id().find(&subject)
                            else {
                                continue;
                            };
                            if custody.case_id != case.id
                                || custody.object_kind != CustodyObjectKind::Subject
                                || custody.holder_kind != CustodyHolderKind::Site
                                || custody.holder_id != case_site.id.value
                            {
                                continue;
                            }
                            (
                                HostileResolutionKind::Captured,
                                20u32,
                                Some(subject),
                                Some(custody.version),
                            )
                        }
                        _ => continue,
                    }
                };
            let path_index =
                u16::try_from(path_index).map_err(|_| "Case has too many objective paths")?;
            let id = mission_approach_capability_id(
                observer_character_id,
                &case.id,
                &case_site.id.value,
                &hostile_group_id,
                path_index,
                objective.id.as_str(),
                resolution,
                capture_subject_id.as_deref(),
                capture_custody_version,
            );
            let capability = MissionApproachCapability {
                id: id.clone(),
                observer_character_id,
                hostile_group_id: hostile_group_id.clone(),
                case_id: case.id.clone(),
                case_site_id: case_site.id.clone(),
                path_index,
                objective_id: objective.id.as_str().to_string(),
                resolution,
                weight,
                capture_subject_id,
                capture_custody_version,
                active: true,
            };
            if let Some(existing) = ctx.db.mission_approach_capability().id().find(&id) {
                if existing.observer_character_id != capability.observer_character_id
                    || existing.hostile_group_id != capability.hostile_group_id
                    || existing.case_id != capability.case_id
                    || existing.case_site_id != capability.case_site_id
                    || existing.path_index != capability.path_index
                    || existing.objective_id != capability.objective_id
                    || existing.resolution != capability.resolution
                    || existing.weight != capability.weight
                    || existing.capture_subject_id != capability.capture_subject_id
                    || existing.capture_custody_version != capability.capture_custody_version
                {
                    return Err(
                        "Mission approach capability conflicts with existing authority".into(),
                    );
                }
                if !existing.active {
                    continue;
                }
                capabilities.push(existing);
            } else {
                ctx.db
                    .mission_approach_capability()
                    .insert(capability.clone());
                capabilities.push(capability);
            }
        }
    }
    capabilities.sort_by(|left, right| left.id.cmp(&right.id));
    if capabilities.is_empty() {
        return Err("Case site has no unresolved observer-authorized combat approach".into());
    }
    let authority = MissionAuthority {
        id: mission_id.to_string(),
        party_id: party_id.to_string(),
        case_site_id: Some(case_site.id.clone()),
        hostile_group_id: Some(hostile_group_id.clone()),
        observer_character_id,
        case_id: case.id.clone(),
        outcome_entropy: ctx.random(),
        status: MissionAttemptStatus::Bound,
        committed_resolution: None,
        committed_capture_subject_id: None,
        scene_key: scene_key.to_string(),
    };
    ctx.db.mission_authority().insert(authority.clone());
    for (index, capability) in capabilities.into_iter().enumerate() {
        ctx.db
            .mission_outcome_candidate()
            .insert(mission_candidate_from_capability(
                mission_id, index, capability,
            ));
    }
    Ok(authority)
}

#[reducer]
pub fn report_contract(
    ctx: &ReducerContext,
    character_id: u64,
    contract_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.active_contract_id.as_deref() != Some(&contract_id) {
        return Err("This is not the party's active quest".into());
    }
    let mut quest = ctx
        .db
        .contract_authority()
        .id()
        .find(&contract_id)
        .ok_or("Quest not found")?;
    if quest.status != ContractStatus::ReadyToReport
        || quest.accepted_by.as_ref() != Some(&party_id)
    {
        return Err("The quest has not been completed by this party".into());
    }
    if quest.paid_at_minute.is_some() {
        return Err("This contract has already been paid".into());
    }
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Return to the questgiver's settlement to claim the reward".into());
    }
    consume_contract_interaction(
        ctx,
        &contract_id,
        &party_id,
        ContractInteractionStage::Report,
    )?;
    let reported_at_minute = crate::time::refresh_clock(ctx)?;

    let reward = quest.gold_reward.max(0) as u64;
    if reward > 0 {
        credit_party_currency(ctx, &party_id, &quest.settlement_id, reward as u32)?;
        let recipients = living_party_member_ids(ctx, &party_id);
        let recipient_count = recipients.len().max(1) as u64;
        let share = reward / recipient_count;
        for recipient in recipients {
            credit_party_stake(ctx, &party_id, recipient, share)?;
        }
        credit_party_reserve(ctx, &party_id, reward % recipient_count)?;
    }
    let total_xp = quest.xp_reward.max(0) as u32;
    let members = living_party_member_ids(ctx, &party_id);
    let xp_per_member = total_xp / members.len().max(1) as u32;
    for member_id in members {
        if let Some(mut member) = ctx.db.character().id().find(member_id) {
            member.xp = member.xp.saturating_add(xp_per_member);
            member.level = 1 + member.xp / 100;
            ctx.db.character().id().update(member);
        }
    }
    quest.status = ContractStatus::Paid;
    quest.paid_at_minute = Some(reported_at_minute);
    ctx.db.contract_authority().id().update(quest.clone());

    party.active_contract_id = None;
    ctx.db.party_authority().id().update(party);
    let obsolete_requests: Vec<u64> = ctx
        .db
        .party_action_request_authority()
        .party_id()
        .filter(&party_id)
        .filter(|request| request.action_kind == "report_contract")
        .map(|request| request.id)
        .collect();
    for request_id in obsolete_requests {
        ctx.db
            .party_action_request_authority()
            .id()
            .delete(request_id);
    }
    ensure_settlement_activity_inner(ctx, &quest.settlement_id)?;
    Ok(())
}

#[reducer]
pub fn autoresolve_mission(
    ctx: &ReducerContext,
    character_id: u64,
    mission_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id else {
        return Err("Must be in a party".into());
    };
    let Some(party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can autoresolve".into());
    }
    let case_site = party
        .current_case_site_id
        .as_ref()
        .and_then(|id| ctx.db.case_site_authority().id_key().find(&id.value))
        .ok_or("Party is not at a case site")?;
    require_party_ready(ctx, &party_id)?;

    let mission = ensure_bound_mission_authority(
        ctx,
        &mission_id,
        &party_id,
        character_id,
        &case_site,
        &case_site.scene_key,
    )?;
    if mission.status != MissionAttemptStatus::Bound {
        return Err("Autoresolve requires a newly bound mission attempt".into());
    }
    let hostile_group_id = mission
        .hostile_group_id
        .as_deref()
        .ok_or("Quest mission must bind a hostile group")?;
    let battle_id = format!("battle:{mission_id}");
    if ctx
        .db
        .autoresolve_report()
        .battle_id()
        .find(&battle_id)
        .is_some()
    {
        return Ok(());
    }
    let hostile_group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&hostile_group_id.to_string())
        .ok_or("Hostile group not found")?;
    if hostile_group.disposition != HostileGroupDisposition::Active {
        return Err("Hostile group is already resolved".into());
    }
    if ctx
        .db
        .tactical_server_request_authority()
        .iter()
        .any(|request| {
            ctx.db
                .mission_authority()
                .id()
                .find(&request.mission_id)
                .is_some_and(|authority| {
                    authority.hostile_group_id.as_deref() == Some(hostile_group_id)
                })
        })
        || ctx.db.tactical_server_authority().iter().any(|server| {
            ctx.db
                .mission_authority()
                .id()
                .find(&server.mission_id)
                .is_some_and(|authority| {
                    authority.hostile_group_id.as_deref() == Some(hostile_group_id)
                })
        })
    {
        return Err("Hostile group already has a tactical resolution in progress".into());
    }

    let member_ids = living_party_member_ids(ctx, &party_id);
    let allies = member_ids
        .iter()
        .map(|member_id| {
            let condition =
                crate::condition::refresh_character_strategic_condition(ctx, *member_id)?;
            crate::capability::load_combatant(
                ctx,
                *member_id,
                condition.incapacitation,
                condition.pain,
                condition.blood_loss,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let enemies = (0..u64::from(hostile_group.enemy_count))
        .map(|index| {
            autoresolve_enemy(
                u64::MAX.saturating_sub(index),
                &hostile_group.enemy_type,
                hostile_group.difficulty,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let seed = ctx.random();
    let outcome = resolve_battle(allies, enemies, seed, BattleOpening::Normal);
    commit_autoresolve_outcome(
        ctx,
        &battle_id,
        &party_id,
        &member_ids,
        5.0 + hostile_group.difficulty.max(0) as f32,
        &outcome,
    )?;

    if outcome.victor != BattleVictor::Allies {
        fail_bound_mission_attempt(ctx, &mission_id)?;
        return Ok(());
    }

    complete_bound_mission_success(ctx, &mission_id)?;
    Ok(())
}

#[reducer]
pub fn cancel_mission_request(
    ctx: &ReducerContext,
    character_id: u64,
    mission_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    adventuresim_core::mission::MissionId::new(mission_id.clone()).map_err(str::to_string)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can cancel a mission request".into());
    }
    let mut mission = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id)
        .ok_or("Mission authority not found")?;
    if mission.party_id != party_id {
        return Err("Mission request belongs to another party".into());
    }
    match mission.status {
        MissionAttemptStatus::Cancelled => return Ok(()),
        MissionAttemptStatus::Bound => {}
        MissionAttemptStatus::Committed | MissionAttemptStatus::Failed => {
            return Err("Mission request is already terminal".into());
        }
    }
    let request = ctx
        .db
        .tactical_server_request_authority()
        .mission_id()
        .find(&mission_id)
        .ok_or("Mission request not found")?;
    if request.party_id != party_id {
        return Err("Mission request belongs to another party".into());
    }
    ctx.db
        .tactical_server_request_authority()
        .mission_id()
        .delete(&mission_id);
    ctx.db
        .tactical_server_claim()
        .mission_id()
        .delete(&mission_id);
    mission.status = MissionAttemptStatus::Cancelled;
    ctx.db.mission_authority().id().update(mission);
    Ok(())
}

/// Seed the local demonstration world only when this module binary was built
/// with a matching high-entropy development capability. Normal builds contain
/// no usable token, so ordinary database and tactical identities cannot seed.
#[reducer]
pub fn bootstrap_development_world(
    ctx: &ReducerContext,
    bootstrap_token: String,
    include_visual_demos: bool,
) -> Result<(), String> {
    if !adventuresim_core::simulation_security::simulation_bootstrap_authorized(
        COMPILED_DEV_BOOTSTRAP_TOKEN,
        &bootstrap_token,
    ) {
        return Err("Development bootstrap is disabled or unauthorized".into());
    }
    seed_world(ctx)?;
    crate::disease::seed_sick_character(ctx)?;
    if include_visual_demos {
        crate::character::seed_damaged_character(ctx)?;
        crate::character::seed_religion_scholar_character(ctx)?;
        crate::social::seed_social_demo(ctx)?;
    }
    Ok(())
}

pub(crate) fn seed_world(ctx: &ReducerContext) -> Result<(), String> {
    const RIVERDALE_NODE: u64 = u64::MAX - 2;
    const IRONFORGE_NODE: u64 = u64::MAX - 1;
    const DEMO_EDGE: u64 = u64::MAX;
    const DEMO_SOURCES: &str = "- **Adventure Simulator renderer demo:** Hand-authored geographic fixture for exercising map and terrain-routing UI.";

    for (id, latitude, longitude) in [
        (RIVERDALE_NODE, 53.50, 10.00),
        (IRONFORGE_NODE, 53.62, 10.20),
    ] {
        if ctx.db.world_node().id().find(id).is_none() {
            ctx.db.world_node().insert(WorldNode {
                id,
                parent_node_id: None,
                latitude,
                longitude,
                is_settlement: true,
                is_town: true,
                is_ferry: false,
                is_harbour: false,
                sources: DEMO_SOURCES.into(),
            });
        }
    }
    if ctx.db.travel_edge().id().find(DEMO_EDGE).is_none() {
        ctx.db.travel_edge().insert(TravelEdge {
            id: DEMO_EDGE,
            from_node_id: RIVERDALE_NODE,
            to_node_id: IRONFORGE_NODE,
            route: TravelRoute::Land(LandRoute {
                bridge: None,
                water_crossings: vec![],
            }),
            provenance: TravelEdgeProvenance::DocumentedViabundus,
            toll_at: None,
            length_m: 19_000,
            slope_multiplier: 1.0,
            terrain: RouteTerrain::stage_placeholder(),
            certainty: 1,
            section: "renderer-demo".into(),
            sources: DEMO_SOURCES.into(),
        });
    }

    let settlements = [
        (
            "riverdale",
            "Riverdale",
            10.00,
            53.50,
            Some(RIVERDALE_NODE),
            3,
            "hills",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::RomanCatholic,
            },
        ),
        (
            "ironforge",
            "Ironforge",
            10.20,
            53.62,
            Some(IRONFORGE_NODE),
            4,
            "desert",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::Reformed,
            },
        ),
        (
            "willowmere",
            "Willowmere",
            -50.0,
            75.0,
            None,
            2,
            "hills",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::EasternOrthodox,
            },
        ),
    ];

    for (id, name, x, y, source_node_id, pop, scene, religious_status) in settlements {
        if ctx.db.settlement().id().find(&id.to_string()).is_none() {
            let languages = match id {
                "oakenshire" => adventuresim_world_schema::SettlementLanguageProfile {
                    east_central_bp: 1_500,
                    west_central_bp: 7_500,
                    low_bp: 1_000,
                    yiddish_incidence_bp: 75,
                },
                "ravenmoor" => adventuresim_world_schema::SettlementLanguageProfile {
                    east_central_bp: 7_500,
                    west_central_bp: 1_500,
                    low_bp: 1_000,
                    yiddish_incidence_bp: 75,
                },
                _ => adventuresim_world_schema::SettlementLanguageProfile {
                    east_central_bp: 1_000,
                    west_central_bp: 1_000,
                    low_bp: 8_000,
                    yiddish_incidence_bp: 75,
                },
            };
            ctx.db.settlement().insert(Settlement {
                id: id.into(),
                name: name.into(),
                coord_x: x,
                coord_y: y,
                population_level: pop,
                population_estimate: 0,
                category: settlement_category(0, pop),
                elevation: ElevationMeters::new(100).unwrap(),
                land_use: LandUseProfile::new(
                    LandUseFraction::new(2_500).unwrap(),
                    LandUseFraction::new(2_000).unwrap(),
                    LandUseFraction::new(100).unwrap(),
                    LandUseFraction::new(5_400).unwrap(),
                )
                .unwrap(),
                forest_cover: ForestCover::Wooded(Woodland {
                    density: CanopyDensity::new(35).unwrap(),
                    dominant: DominantLeafType::Mixed,
                }),
                potential_vegetation: PotentialVegetation::Inferred(
                    PotentialVegetationClass::WoodlandAndForest,
                ),
                historical_vegetation: HistoricalVegetation::Fallback(adventuresim_world_schema::FallbackHistoricalVegetation {
                    cover: adventuresim_world_schema::FallbackHistoricalVegetationCover::Woodland(adventuresim_world_schema::HistoricalWoodland {
                        canopy: CanopyDensity::new(35).unwrap(),
                        dominant: DominantLeafType::Mixed,
                    }),
                    method: adventuresim_world_schema::FallbackHistoricalVegetationMethod::PotentialEnvelopeV4,
                }),
                tree_species: TreeSpeciesProfile::Inferred(
                    InferredTreeSpeciesProfile::new(vec![
                        TreeSpeciesId::new("Quercus_robur").unwrap(),
                    ])
                    .unwrap(),
                ),
                soil: SoilProfile {
                    wrb_group: adventuresim_world_schema::WrbReferenceGroup::Cambisol,
                    parent_material: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                    properties: SoilProperties {
                    substrate: SoilSubstrate::Mineral(MineralSoil {
                        texture: MineralSoilTexture::Medium,
                        depth: SoilDepth::Deep,
                        available_water: AvailableWaterCapacity::Medium,
                        organic_carbon: TopsoilOrganicCarbon::Medium,
                        stones: StoneContentPercent::new(10).unwrap(),
                    }),
                    water_regime: SoilWaterRegime::SeasonallyWet,
                    agricultural_limitation: AgriculturalLimitation::None,
                    },
                    acidity: SoilAcidity::Acid,
                    cation_exchange_capacity: CationExchangeCapacity::Medium,
                    fertility: SoilFertility::Medium,
                    confidence: SoilBasisPoints::new(2_500).unwrap(),
                    evidence: SoilEvidence::DeterministicInference,
                },
                geology: SurfaceGeology::Inferred(InferredGeologicSetting {
                    lithology: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                    age: GeologicEra::Quaternary,
                }),
                religious_status,
                languages,
                drought: DroughtProfile::Inferred(
                    DroughtHistory::new(
                        PalmerDroughtSeverityIndex::new(0).unwrap(),
                        PalmerDroughtSeverityIndex::new(0).unwrap(),
                        0,
                        0,
                    )
                    .unwrap(),
                ),
                hydrology: SettlementHydrology::default(),
                industries: InferredIndustryProfile::new(vec![
                    adventuresim_world_schema::IndustryEvidence::Fallback(
                        adventuresim_world_schema::FallbackIndustry::WoodlandFuelwood,
                    ),
                ]).unwrap(),
                economy: SettlementEconomyProfile::stage_placeholder(),
                scene_key: scene.into(),
                religion_id: religious_status.church().religion_id().into(),
                currency_id: crate::item::settlement_currency_id(id).into(),
                source_node_id,
                sources: "- **Adventure Simulator demo data:** Hand-authored settlement and deterministic placeholder environment; no external world-data source was imported.".into(),
            });
        }
    }

    let settlement_ids: Vec<_> = ctx
        .db
        .settlement()
        .iter()
        .map(|settlement| settlement.id)
        .collect();
    for settlement_id in settlement_ids {
        ensure_settlement_activity_inner(ctx, &settlement_id)?;
        crate::repair::ensure_settlement_smith(ctx, &settlement_id);
        crate::disease::ensure_settlement_herbalist(ctx, &settlement_id);
    }

    Ok(())
}

#[reducer]
pub fn ensure_settlement_activity(
    ctx: &ReducerContext,
    settlement_id: String,
) -> Result<(), String> {
    ensure_settlement_activity_inner(ctx, &settlement_id)
}

fn settlement_activity_target(settlement_id: &str) -> usize {
    MIN_QUESTS_PER_SETTLEMENT
        + settlement_id.bytes().map(usize::from).sum::<usize>()
            % (MAX_QUESTS_PER_SETTLEMENT - MIN_QUESTS_PER_SETTLEMENT + 1)
}

fn ensure_settlement_activity_inner(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    crate::settlement_population::ensure_settlement_population(ctx, settlement_id)?;
    for mut contract in ctx
        .db
        .contract_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|contract| {
            matches!(
                contract.status,
                ContractStatus::Offered | ContractStatus::Accepted
            ) && ctx
                .db
                .case_authority()
                .id()
                .find(&contract.case_id)
                .is_some_and(|case| case.resolution_status != CaseResolutionStatus::Open)
        })
        .collect::<Vec<_>>()
    {
        contract.status = ContractStatus::Withdrawn;
        ctx.db.contract_authority().id().update(contract);
    }
    let tracked_quests: HashSet<String> = ctx
        .db
        .party_authority()
        .iter()
        .filter_map(|party| party.active_contract_id)
        .collect();
    let active_contracts = ctx
        .db
        .contract_authority()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|quest| {
            matches!(
                quest.status,
                ContractStatus::Offered | ContractStatus::Accepted
            ) || (quest.status == ContractStatus::ReadyToReport
                && tracked_quests.contains(&quest.id))
        })
        .count();
    let active_generated_cases = ctx
        .db
        .quest_generation_authority()
        .iter()
        .filter(|authority| {
            serde_json::from_str::<adventuresim_core::quest_generation::GenerationContext>(
                &authority.context_snapshot_json,
            )
            .is_ok_and(|context| context.settlement_id == settlement_id)
                && ctx
                    .db
                    .case_authority()
                    .id()
                    .find(&authority.case_id)
                    .is_some_and(|case| case.resolution_status == CaseResolutionStatus::Open)
        })
        .count();
    let active = active_contracts.saturating_add(active_generated_cases);
    for _ in active..settlement_activity_target(settlement_id) {
        generate_quest_for_settlement(ctx, settlement_id)?;
    }
    ensure_npc_recruiting_parties(ctx, settlement_id)?;
    Ok(())
}

fn ensure_npc_recruiting_parties(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let target = 1 + settlement_id.bytes().map(usize::from).sum::<usize>() % 2;
    let now = crate::time::refresh_clock(ctx)?;
    for mut offer in ctx.db.recruitment_offer().iter().collect::<Vec<_>>() {
        if offer.status == RecruitmentOfferStatus::Open && now >= offer.expires_at_minute {
            offer.status = RecruitmentOfferStatus::Expired;
            ctx.db.recruitment_offer().id_key().update(offer);
        }
    }
    let existing = ctx
        .db
        .recruitment_offer()
        .iter()
        .filter(|offer| offer.settlement_id == settlement_id)
        .filter(|offer| offer.status == RecruitmentOfferStatus::Open)
        .count();
    for _ in existing..target {
        let used_npcs: HashSet<_> = ctx
            .db
            .recruitment_offer()
            .settlement_id()
            .filter(&settlement_id.to_string())
            .map(|offer| offer.settlement_npc_id)
            .collect();
        let Some((npc, presence)) = ctx
            .db
            .settlement_npc()
            .home_settlement_id()
            .filter(&settlement_id.to_string())
            .filter(|npc| !used_npcs.contains(&npc.id))
            .filter_map(|npc| {
                ctx.db
                    .settlement_npc_presence()
                    .npc_id()
                    .find(&npc.id)
                    .filter(|presence| crate::settlement_population::npc_is_present(presence, now))
                    .map(|presence| (npc, presence))
            })
            .min_by_key(|(npc, _)| (!npc.service_id.is_empty(), npc.id.clone()))
        else {
            break;
        };
        let leader_name = npc.name.clone();
        let mut leader_id =
            adventuresim_core::settlement_population::stable_hash(&npc.id) | (1_u64 << 63);
        while ctx.db.character().id().find(leader_id).is_some() {
            leader_id = leader_id.wrapping_add(1) | (1_u64 << 63);
        }
        crate::character::insert_new_npc_character(ctx, leader_name.clone(), leader_id, true)?;
        let mut leader = ctx.db.character().id().find(leader_id).unwrap();
        leader.current_settlement_id = Some(settlement_id.to_string());
        ctx.db.character().id().update(leader.clone());
        crate::character::set_character_languages_for_settlement(
            ctx,
            leader_id,
            settlement_id,
            true,
        )?;
        let party_id = leader.party_id.clone().ok_or("NPC leader has no party")?;
        let mut party = ctx.db.party_authority().id().find(&party_id).unwrap();
        party.name = format!("{}'s company", leader_name);
        party.current_settlement_id = Some(settlement_id.to_string());
        party.medicine_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.command_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.religion_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        ctx.db.party_authority().id().update(party);

        let mut requirements = RecruitmentRequirements::default();
        if ctx.random::<u64>() % 2 == 0 {
            requirements.melee = true;
        } else {
            requirements.ranged = true;
        }
        requirements.athletics = (ctx.random::<u64>() % 4) as u8;
        requirements.endurance = (ctx.random::<u64>() % 4) as u8;
        let armor = ctx.random::<u64>() % 3;
        requirements.quarter_armor = armor == 1;
        requirements.half_armor = armor == 2;
        ctx.db
            .party_recruitment_role()
            .insert(PartyRecruitmentRole {
                id: 0,
                party_id: party_id.clone(),
                name: if requirements.ranged {
                    "Ranged support".into()
                } else {
                    "Vanguard".into()
                },
                requirements,
                quantity: 3,
                weapon_precision: (ctx.random::<u64>() % 4) as f32 * 0.5,
            });
        let source_key = format!("settlement-recruiter:{}", npc.id);
        let offer_key = format!("recruitment-offer:{source_key}");
        ctx.db.recruitment_offer().insert(RecruitmentOffer {
            id_key: offer_key.clone(),
            id: RecruitmentOfferId { value: offer_key },
            source_id: RecruitmentSourceId { value: source_key },
            recruiting_party_id: party_id,
            settlement_id: settlement_id.to_string(),
            settlement_npc_id: npc.id,
            location_id: presence.location_id,
            leader_id,
            status: RecruitmentOfferStatus::Open,
            created_at_minute: now,
            expires_at_minute: now.saturating_add(7 * 1_440),
        });
    }
    Ok(())
}

fn generated_witness_visible_description(
    height: &str,
    build: &str,
    hair: &str,
    clothing: &str,
) -> String {
    format!("{height}, {build}, with {hair}, wearing {clothing}")
}

fn generated_witness_candidates(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Vec<adventuresim_core::quest_generation::WitnessCandidate> {
    use adventuresim_core::quest_generation::{Circumstance, WitnessCandidate, WitnessDemographic};
    let mut candidates = ctx
        .db
        .settlement_npc()
        .home_settlement_id()
        .filter(&settlement_id.to_string())
        .filter_map(|npc| {
            let presence = ctx.db.settlement_npc_presence().npc_id().find(&npc.id)?;
            let demographic = generated_npc_demographic(&npc);
            let mut circumstances = BTreeSet::from([
                Circumstance::NightWindow,
                Circumstance::RoadJourney,
                Circumstance::LivestockWatch,
            ]);
            if presence.location_id == "church" {
                circumstances.insert(Circumstance::GraveDuty);
            }
            if presence.location_id == "adult_venue"
                || !matches!(demographic, WitnessDemographic::Child)
            {
                circumstances.insert(Circumstance::AdultVenue);
            }
            if !matches!(demographic, WitnessDemographic::Child) {
                circumstances.insert(Circumstance::SecretRiversideMeeting);
            }
            let age_band = format!("{:?}", npc.age_band).to_ascii_lowercase();
            let sex = format!("{:?}", npc.sex).to_ascii_lowercase();
            let presence_version = generated_npc_presence_version(&npc, &presence);
            Some(WitnessCandidate {
                npc_id: npc.id,
                demographic,
                age_band,
                sex,
                profession: npc.profession,
                visible_description: generated_witness_visible_description(
                    &npc.height,
                    &npc.build,
                    &npc.hair,
                    &npc.clothing,
                ),
                expected_location: presence.location_id,
                presence_version,
                allowed_circumstances: circumstances,
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.npc_id.cmp(&right.npc_id));
    candidates
}

pub(crate) fn generated_npc_demographic(
    npc: &crate::settlement_population::SettlementNpc,
) -> adventuresim_core::quest_generation::WitnessDemographic {
    use adventuresim_core::quest_generation::WitnessDemographic;
    match npc.age_band {
        crate::settlement_population::NpcAgeBand::Child
        | crate::settlement_population::NpcAgeBand::Adolescent => WitnessDemographic::Child,
        crate::settlement_population::NpcAgeBand::Adult
        | crate::settlement_population::NpcAgeBand::Elder => {
            if npc.profession.contains("merchant") {
                WitnessDemographic::Merchant
            } else if npc.profession.contains("cleric") {
                WitnessDemographic::Cleric
            } else if npc.profession.contains("guard") || npc.local_role.contains("retainer") {
                WitnessDemographic::Guard
            } else if npc.local_role.contains("lord") {
                WitnessDemographic::Noble
            } else {
                WitnessDemographic::Laborer
            }
        }
    }
}

pub(crate) fn generated_npc_presence_version(
    npc: &crate::settlement_population::SettlementNpc,
    presence: &crate::settlement_population::SettlementNpcPresence,
) -> u64 {
    adventuresim_core::settlement_population::stable_hash(&format!(
        "victim-presence-v1:{}:{:?}:{:?}:{}:{}:{}:{}:{}:{}",
        npc.id,
        npc.age_band,
        npc.sex,
        npc.profession,
        presence.settlement_id,
        presence.location_id,
        presence.start_minute,
        presence.end_minute,
        presence.is_default
    ))
}

fn generated_scene_key(kind: adventuresim_core::quest_generation::SiteKind) -> &'static str {
    use adventuresim_core::quest_generation::SiteKind;
    match kind {
        SiteKind::Cave | SiteKind::Crypt => "cave",
        SiteKind::ForestCamp | SiteKind::AbandonedFarm => "woods",
        SiteKind::OccupiedHouse | SiteKind::Graveyard => "ruins",
        SiteKind::Riverside | SiteKind::Roadside => "camp",
    }
}

fn generate_quest_for_settlement(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    use adventuresim_core::quest_generation as qg;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id.to_string())
        .ok_or("Settlement not found")?;
    let ordinal = ctx
        .db
        .quest_generation_authority()
        .iter()
        .filter(|row| {
            serde_json::from_str::<qg::GenerationContext>(&row.context_snapshot_json)
                .is_ok_and(|context| context.settlement_id == settlement_id)
        })
        .count() as u16;
    let seed = ctx.random::<u64>();
    let observer_entropy_hi = ctx.random::<u64>();
    let observer_entropy_lo = ctx.random::<u64>();
    let context = qg::GenerationContext {
        seed,
        observer_entropy_hi,
        observer_entropy_lo,
        settlement_id: settlement_id.into(),
        settlement_name: settlement.name.clone(),
        scope: adventuresim_core::local_problem::Scope::Settlement {
            settlement_id: settlement_id.into(),
        },
        ordinal,
        now_minute: crate::time::refresh_clock(ctx)?,
        requested_family: (ordinal < 2).then_some(if ordinal == 0 {
            qg::TemplateFamily::RecurringDepredation
        } else {
            qg::TemplateFamily::DisappearanceOrLoss
        }),
        witness_candidates: generated_witness_candidates(ctx, settlement_id),
    };
    let generated = qg::generate(&context)
        .map_err(|error| format!("Quest generator exhausted its bounded search: {error:?}"))?;
    qg::validate(&generated)
        .map_err(|errors| format!("Generated quest manifest is invalid: {}", errors.join("; ")))?;

    ctx.db.case_authority().insert(CaseAuthority {
        id: generated.canonical_case_id.clone(),
        investigation_case_id: generated.canonical_case_id.clone(),
        local_problem_id: Some(generated.problem_id.clone()),
        objective_expression_json: serde_json::to_string(&generated.objectives)
            .map_err(|_| "Could not encode generated objectives")?,
        resolution_status: CaseResolutionStatus::Open,
        resolved_by_party_id: None,
    });
    ctx.db.investigation_case_authority().insert(
        crate::investigation::InvestigationCaseAuthority {
            id: generated.canonical_case_id.clone(),
            problem_id: generated.problem_id.clone(),
            hidden_target_json: serde_json::to_string(&generated.cause)
                .map_err(|_| "Could not encode canonical generated cause")?,
            generation_explanation_json: serde_json::to_string(&generated.factor_trace)
                .map_err(|_| "Could not encode generated factor trace")?,
        },
    );
    for event in &generated.canonical_events {
        ctx.db.investigation_event_authority().insert(
            crate::investigation::InvestigationEventAuthority {
                id: event.id.clone(),
                case_id: generated.canonical_case_id.clone(),
                canonical_propositions_json: serde_json::to_string(&[serde_json::json!({
                    "id": event.proposition_id,
                    "subject": event.subject,
                    "predicate": event.predicate,
                    "object": event.object,
                })])
                .map_err(|_| "Could not encode generated event")?,
                occurred_at: event.occurred_at,
            },
        );
    }

    let geographic = settlement.source_node_id.is_some();
    let mut site_rows = BTreeMap::new();
    for (index, site) in generated.sites.iter().enumerate() {
        let distance_m = 4_000 + (seed.rotate_left(index as u32) % 17_000);
        let angle_seed = seed.rotate_left((index as u32).saturating_mul(11));
        let angle = (angle_seed as f64 / u64::MAX as f64) * std::f64::consts::TAU;
        let distance_km = distance_m as f64 / 1_000.0;
        let (offset_x, offset_y) = if geographic {
            let latitude_scale = 111.0;
            let longitude_scale =
                latitude_scale * settlement.coord_y.to_radians().cos().abs().max(0.1);
            (
                angle.cos() * distance_km / longitude_scale,
                angle.sin() * distance_km / latitude_scale,
            )
        } else {
            (angle.cos() * distance_km, angle.sin() * distance_km)
        };
        let row = CaseSiteAuthority {
            id_key: site.id.0.clone(),
            id: CaseSiteId::from(site.id.0.clone()),
            case_id: generated.canonical_case_id.clone(),
            origin_settlement_id: settlement_id.into(),
            name: site.safe_label.clone(),
            description: format!("You arrive at {}.", site.safe_label),
            scene_key: generated_scene_key(site.kind).into(),
            longitude_e7: ((settlement.coord_x + offset_x) * 10_000_000.0).round() as i32,
            latitude_e7: ((settlement.coord_y + offset_y) * 10_000_000.0).round() as i32,
            coordinates_are_geographic: geographic,
            distance_m,
        };
        ctx.db.case_site_authority().insert(row.clone());
        site_rows.insert(site.id.clone(), row);
    }
    for area in &generated.areas {
        ctx.db.investigation_area_authority().insert(
            crate::investigation::InvestigationAreaAuthority {
                id: area.id.clone(),
                case_id: generated.canonical_case_id.clone(),
                origin_settlement_id: settlement_id.into(),
                safe_label: area.safe_label.clone(),
                center_longitude_e7: (settlement.coord_x * 10_000_000.0).round() as i32,
                center_latitude_e7: (settlement.coord_y * 10_000_000.0).round() as i32,
                radius_m: 5_000,
                coordinates_are_geographic: geographic,
                terrain: serde_json::to_string(&area.terrain)
                    .map_err(|_| "Could not encode generated area terrain")?
                    .trim_matches('"')
                    .into(),
            },
        );
    }
    for evidence in &generated.evidence {
        ctx.db.investigation_evidence_authority().insert(
            crate::investigation::InvestigationEvidenceAuthority {
                id: evidence.id.0.clone(),
                case_id: generated.canonical_case_id.clone(),
                proposition_id: evidence.proposition_id.clone(),
                presentation_kind: crate::investigation::EvidencePresentationKind::Physical,
                authority_json: serde_json::to_string(evidence)
                    .map_err(|_| "Could not encode generated evidence")?,
                hidden_coordinates_json: serde_json::to_string(&evidence.site_id)
                    .map_err(|_| "Could not encode generated evidence site")?,
            },
        );
    }
    for witness in &generated.witnesses {
        ctx.db.investigation_testimony_bundle().insert(
            crate::investigation::InvestigationTestimonyBundle {
                id: witness.id.0.clone(),
                case_id: generated.canonical_case_id.clone(),
                witness_ref: witness.npc_id.clone(),
                reliability_json: serde_json::to_string(
                    &witness
                        .testimony
                        .iter()
                        .map(|draft| draft.reliability)
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| "Could not encode testimony reliability")?,
                stages_json: serde_json::to_string(&witness.testimony)
                    .map_err(|_| "Could not encode testimony drafts")?,
            },
        );
    }
    for (group_id, site_id, threat, count) in &generated.hostile_groups {
        let site = site_rows
            .get(site_id)
            .ok_or("Generated hostile group references a missing site")?;
        materialize_hostile_group(ctx, group_id, site, threat.as_str().into(), *count, 2)?;
    }
    seed_case_custody(
        ctx,
        &generated.canonical_case_id,
        &generated
            .sites
            .iter()
            .find(|site| site.role == qg::SiteRole::Finale)
            .ok_or("Generated case has no finale site")?
            .id
            .0,
        &generated.objectives,
    )?;
    for (path_index, _) in generated.objectives.alternatives.iter().enumerate() {
        ctx.db.case_finale_authority().insert(CaseFinaleAuthority {
            id: format!("finale:{}:{path_index}", generated.canonical_case_id),
            case_id: generated.canonical_case_id.clone(),
            kind: FinaleKind::RecordResolution,
            resolution_status: CaseResolutionStatus::Resolved,
            eligible_path_index: Some(path_index as u16),
            priority: 100u16.saturating_sub(path_index as u16),
            status: FinaleStatus::Available,
        });
    }
    ctx.db.case_finale_authority().insert(CaseFinaleAuthority {
        id: format!("finale:{}:problem", generated.canonical_case_id),
        case_id: generated.canonical_case_id.clone(),
        kind: FinaleKind::ResolveLocalProblem,
        resolution_status: CaseResolutionStatus::Resolved,
        eligible_path_index: None,
        priority: 1,
        status: FinaleStatus::Available,
    });
    crate::local_problem::materialize_generated_problem(ctx, &generated, settlement_id)?;
    ctx.db
        .quest_generation_authority()
        .insert(QuestGenerationAuthority {
            case_id: generated.canonical_case_id.clone(),
            seed,
            catalog_revision: generated.catalog_revision.clone(),
            context_snapshot_json: serde_json::to_string(&context)
                .map_err(|_| "Could not encode quest generation context")?,
            manifest_json: serde_json::to_string(&generated)
                .map_err(|_| "Could not encode quest generation manifest")?,
            factor_trace_json: serde_json::to_string(&generated.factor_trace)
                .map_err(|_| "Could not encode quest generation trace")?,
        });
    Ok(())
}
