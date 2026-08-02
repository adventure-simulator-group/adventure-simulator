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
    browser_session::browser_character_grant,
    capability::character_capability,
    character::{
        character, character_attributes, character_equipped_item, character_limbs,
        character_skills, character_skills__view, character_stats, equipment_occupancy,
        starting_character_claim,
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
        settlement_resident_presence, settlement_resident_profile,
        settlement_resident_seed_explanation,
    },
    surgery::limb_injury__view,
    social::character_familiarity,
    tactical::{
        tactical_server_authority, tactical_server_authority__view, tactical_server_claim,
        tactical_server_request_authority,
    },
    time::{
        advance_travel_time, character_time, character_training_schedule, settle_travel_boundary,
    },
    world_actor::{
        character_context_membership, character_context_membership__view,
        party_context_contact_authority, party_context_contact_authority__view,
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

pub(crate) fn autoresolve_enemy(
    id: u64,
    enemy_type: &str,
    difficulty: i32,
    combat_scale_bps: u32,
) -> Result<Combatant, String> {
    autoresolve_enemy_with_countermeasure(id, enemy_type, difficulty, combat_scale_bps, 10_000)
}

pub(crate) fn autoresolve_enemy_with_countermeasure(
    id: u64,
    enemy_type: &str,
    difficulty: i32,
    combat_scale_bps: u32,
    countermeasure_multiplier_bps: u32,
) -> Result<Combatant, String> {
    adventuresim_core::autoresolve::authored_threat_combatant(
        id,
        enemy_type,
        difficulty,
        combat_scale_bps,
        countermeasure_multiplier_bps,
    )
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
