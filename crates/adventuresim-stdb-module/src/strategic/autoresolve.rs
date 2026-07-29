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
        character_stats, starting_character_claim,
    },
    condition::{character_condition, character_strategic_condition},
    disease::character_illness_status,
    inventory_amount::{inventory_item_amount, party_item_amount},
    investigation::{
        CaseSiteAuthority, CaseSiteId, EvidencePresentationKind, PartyCaseSiteTracking,
        case_site_authority, case_site_authority__view, case_site_provenance_reducer,
        disclose_exact_case_site, exact_case_site_for_observer, investigation_area_authority,
        investigation_belief, investigation_case_authority, investigation_event_authority,
        investigation_evidence_authority, investigation_evidence_knowledge, investigation_lead,
        investigation_received_testimony, investigation_testimony_bundle, mark_case_site_visited,
        party_case_site_tracking, referred_generated_witness,
    },
    item::{InventoryItem, inventory_item, item},
    local_problem::{
        local_problem_receipt, local_problem_rumor_delivery, public_threat_disclosure,
    },
    npc_adventurer::npc_adventuring_party_authority,
    organization::organization_presentation,
    repair::{item_condition, settlement_smith},
    settlement_population::{
        settlement_npc, settlement_npc_presence, settlement_npc_seed_explanation,
    },
    tactical::{
        tactical_server_authority, tactical_server_claim, tactical_server_request_authority,
    },
    time::{
        advance_travel_time, character_time, character_training_schedule, preview_travel_time,
        settle_travel_boundary,
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
const RIVERDALE_RENDERER_DEMO_NODE: u64 = u64::MAX - 2;
const IRONFORGE_RENDERER_DEMO_NODE: u64 = u64::MAX - 1;
const RENDERER_DEMO_EDGE: u64 = u64::MAX;
const PLACEHOLDER_SETTLEMENT_IDS: [&str; 3] = ["riverdale", "ironforge", "willowmere"];

fn parse_threat(enemy_type: &str) -> Result<adventuresim_core::bestiary::ThreatId, String> {
    enemy_type
        .parse()
        .map_err(|_| format!("Unknown threat ID: {enemy_type}"))
}

fn quest_encounter_archetype(
    enemy_type: &str,
) -> Option<adventuresim_core::encounter::EncounterArchetype> {
    use adventuresim_core::{bestiary::ThreatId, encounter::EncounterArchetype};
    let threat = parse_threat(enemy_type).ok()?;
    if [ThreatId::Goblin, ThreatId::Kobold].contains(&threat) {
        Some(EncounterArchetype::Goblins)
    } else if [
        ThreatId::Skeleton,
        ThreatId::Ghoul,
        ThreatId::Revenant,
        ThreatId::Nachzehrer,
    ]
    .contains(&threat)
    {
        Some(EncounterArchetype::Undead)
    } else if [
        ThreatId::Bandit,
        ThreatId::Deserter,
        ThreatId::Poacher,
        ThreatId::Smuggler,
        ThreatId::Cultist,
        ThreatId::GraveRobber,
    ]
    .contains(&threat)
    {
        Some(EncounterArchetype::Bandits)
    } else {
        None
    }
}

fn autoresolve_enemy(
    id: u64,
    enemy_type: &str,
    difficulty: i32,
    combat_scale_bps: u32,
) -> Result<Combatant, String> {
    use adventuresim_core::bestiary::{AttackStyle, Protection};
    let scale = adventuresim_core::threat_escalation::combat_physical_multiplier(combat_scale_bps);
    let rating = (1.2 + difficulty.max(1) as f32 * 0.35) * scale;
    let threat_profile = parse_threat(enemy_type)?.profile();
    let profile = threat_profile.combat;
    let mut combatant = Combatant::new(id);
    combatant.bestiary_categories = threat_profile.categories().collect();
    combatant.attributes = CombatAttributes {
        endurance: rating,
        immunity: rating,
        gut: rating,
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
