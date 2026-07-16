use adventuresim_core::{capability::aggregate_bounded_party_check, morale::fervor_event_occurs};
use adventuresim_world_schema::{
    AgriculturalLimitation, AvailableWaterCapacity, CanopyDensity, DominantLeafType,
    DroughtHistory, DroughtProfile, EdgeEndpoint, ElevationMeters, ForestCover, GeologicEra,
    GeologicUnitId, HabitatSuitability, InferredGeologicSetting, InferredTreeSpeciesProfile,
    LandUseFraction, LandUseProfile, MappedPotentialVegetation, MappedSoilProfile, MineralSoil,
    MineralSoilTexture, ModeledTreeSpecies, ModeledTreeSpeciesProfile, OfficialReligion,
    PalmerDroughtSeverityIndex, ParentMaterialCode, PotentialVegetation,
    PotentialVegetationFormation, SettlementImport, SettlementReligiousStatus, SoilDepth,
    SoilMappingUnit, SoilProfile, SoilProperties, SoilSubstrate, SoilWaterRegime,
    StoneContentPercent, SurfaceGeology, SurfaceLithology, TopsoilOrganicCarbon, TravelEdgeImport,
    TravelRoute, TreeSpeciesId, TreeSpeciesProfile, UnconsolidatedDeposit, WORLD_SCHEMA_VERSION,
    Woodland, WorldNodeImport,
};
use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    character::{
        character, character_attributes, character_equip, character_limbs, character_skills,
    },
    condition::character_condition,
    item::{InventoryItem, inventory_item, item},
    tactical::tactical_server_request,
    time::advance_character_time,
};
use std::collections::{BinaryHeap, HashMap, HashSet};

const MERCHANT_MARGIN: f32 = 1.25;
const SALES_TAX: f32 = 0.10;
const WALKING_SPEED_KM_PER_HOUR: u64 = 5;
const QUEST_TRAVEL_SPEED_DIVISOR: u64 = 4;
const METERS_PER_KILOMETER: u64 = 1_000;
const MINUTES_PER_HOUR: u64 = 60;
const MIN_QUESTS_PER_SETTLEMENT: usize = 3;
const MAX_QUESTS_PER_SETTLEMENT: usize = 5;
const AUTORE_SOLVE_BLOOD_LOSS_PER_HEALTH_DAMAGE: f32 = 0.5;
const SURGERY_CHECK_PER_HEALTH_DAMAGE: f32 = 20.0;

/// Untreated autoresolve wounds can deteriorate by as much as their original
/// damage. Every five percentage points of damage require one point of the
/// party Surgery check to prevent deterioration completely.
fn post_battle_wound_deterioration(damage: f32, surgery_check: f32) -> f32 {
    let damage = damage.max(0.0);
    let required_check = (damage * SURGERY_CHECK_PER_HEALTH_DAMAGE).clamp(0.0, 5.0);
    if required_check == 0.0 {
        return 0.0;
    }
    let untreated_fraction =
        ((required_check - surgery_check.clamp(0.0, 5.0)) / required_check).clamp(0.0, 1.0);
    damage * untreated_fraction
}

#[cfg(test)]
mod healing_tests {
    use super::post_battle_wound_deterioration;

    #[test]
    fn surgery_prevents_deterioration_when_it_meets_wound_severity() {
        assert!((post_battle_wound_deterioration(0.20, 4.0)).abs() < f32::EPSILON);
        assert!((post_battle_wound_deterioration(0.05, 1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn surgery_shortfall_proportionally_worsens_the_wound() {
        assert!((post_battle_wound_deterioration(0.20, 2.0) - 0.10).abs() < f32::EPSILON);
        assert!((post_battle_wound_deterioration(0.20, 0.0) - 0.20).abs() < f32::EPSILON);
    }
}

fn require_party_ready(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let character_ids: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .map(|membership| membership.character_id)
        .collect();
    crate::condition::require_characters_ready(ctx, &character_ids)
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestStatus {
    Available,
    Accepted,
    Completed,
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
    pub elevation: ElevationMeters,
    pub land_use: LandUseProfile,
    pub forest_cover: ForestCover,
    pub potential_vegetation: PotentialVegetation,
    pub tree_species: TreeSpeciesProfile,
    pub soil: SoilProfile,
    pub geology: SurfaceGeology,
    pub religious_status: SettlementReligiousStatus,
    pub drought: DroughtProfile,
    pub scene_key: String,
    /// The single faith represented by this settlement's church and priest.
    pub religion_id: String,
    /// Viabundus node that supplies this settlement, if it was imported from
    /// the historical world dataset. Demo settlements deliberately leave this
    /// empty.
    pub source_node_id: Option<u64>,
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
    pub kind: String,
    pub bridge_at: Option<EdgeEndpoint>,
    pub toll_at: Option<EdgeEndpoint>,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub certainty: u8,
    pub section: String,
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
) -> Result<(), String> {
    if schema_version != WORLD_SCHEMA_VERSION {
        return Err(format!(
            "World schema version {schema_version} is unsupported; expected {WORLD_SCHEMA_VERSION}"
        ));
    }
    if artifact_id.trim().is_empty() {
        return Err("World artifact ID must not be empty".into());
    }
    match ctx.db.world_data_import().id().find(0) {
        Some(import) if import.owner != ctx.sender() => {
            Err("World data import is owned by another identity".into())
        }
        Some(import)
            if import.schema_version == schema_version && import.artifact_id == artifact_id =>
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
        let row = WorldNode {
            id: node.id,
            parent_node_id: node.parent_node_id,
            latitude: node.latitude,
            longitude: node.longitude,
            is_settlement: node.is_settlement,
            is_town: node.is_town,
            is_ferry: node.is_ferry,
            is_harbour: node.is_harbour,
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
pub fn import_travel_edges(
    ctx: &ReducerContext,
    edges: Vec<TravelEdgeImport>,
) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if edges.is_empty() {
        return Err("Travel-edge batch is empty".into());
    }
    for edge in edges {
        if ctx.db.world_node().id().find(edge.from_node_id).is_none()
            || ctx.db.world_node().id().find(edge.to_node_id).is_none()
        {
            return Err(format!(
                "Travel edge {} references an unknown world node",
                edge.id
            ));
        }
        let (kind, bridge_at) = match edge.route {
            TravelRoute::Land { bridge } => ("land", bridge),
            TravelRoute::Ferry => ("ferry", None),
        };
        let row = TravelEdge {
            id: edge.id,
            from_node_id: edge.from_node_id,
            to_node_id: edge.to_node_id,
            kind: kind.into(),
            bridge_at,
            toll_at: edge.toll,
            length_m: edge.length_m,
            slope_multiplier: edge.slope_multiplier,
            certainty: edge.certainty,
            section: edge.section,
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
        let potential_vegetation = match settlement.potential_vegetation {
            PotentialVegetation::Mapped(mapped) => {
                let unit = adventuresim_world_schema::EuroVegMapUnitCode::new(
                    mapped.unit().as_str().to_owned(),
                )
                .ok_or_else(|| {
                    format!(
                        "Settlement {} has invalid EuroVegMap unit code",
                        settlement.id
                    )
                })?;
                PotentialVegetation::Mapped(
                    MappedPotentialVegetation::new(unit, mapped.formation()).ok_or_else(|| {
                        format!(
                            "Settlement {} has contradictory EuroVegMap vegetation",
                            settlement.id
                        )
                    })?,
                )
            }
            PotentialVegetation::Inferred(formation) => PotentialVegetation::Inferred(formation),
        };
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
        let row = Settlement {
            id: settlement.id,
            name: settlement.name,
            coord_x: settlement.longitude,
            coord_y: settlement.latitude,
            population_level: settlement.population_level,
            population_estimate: settlement.population_estimate,
            elevation,
            land_use,
            forest_cover,
            potential_vegetation,
            tree_species,
            soil,
            geology,
            scene_key: settlement.scene_key,
            religion_id: settlement.religious_status.church().faith_id().into(),
            religious_status: settlement.religious_status,
            drought,
            source_node_id: Some(settlement.source_node_id),
        };
        let settlement_id = row.id.clone();
        if ctx.db.settlement().id().find(&row.id).is_some() {
            ctx.db.settlement().id().update(row);
        } else {
            ctx.db.settlement().insert(row);
        }
        ensure_settlement_activity_inner(ctx, &settlement_id)?;
    }
    Ok(())
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
    match profile {
        SoilProfile::Inferred(properties) => {
            Ok(SoilProfile::Inferred(reconstruct_properties(properties)?))
        }
        SoilProfile::Mapped(mapped) => {
            let mapping_unit = SoilMappingUnit::new(
                mapped.mapping_unit.smu(),
                mapped.mapping_unit.dominant_stu(),
                mapped.mapping_unit.dominance_percent(),
            )
            .ok_or_else(|| {
                format!("Settlement {settlement_id} has an invalid soil mapping unit")
            })?;
            let parent_material =
                ParentMaterialCode::new(mapped.parent_material.as_str().to_owned()).ok_or_else(
                    || format!("Settlement {settlement_id} has an invalid parent-material code"),
                )?;
            Ok(SoilProfile::Mapped(MappedSoilProfile {
                mapping_unit,
                wrb_group: mapped.wrb_group,
                parent_material,
                properties: reconstruct_properties(mapped.properties)?,
            }))
        }
    }
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

#[derive(Clone, Debug)]
#[table(accessor = quest, public)]
pub struct Quest {
    #[primary_key]
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    #[index(btree)]
    pub settlement_id: String,
    pub status: QuestStatus,
    pub accepted_by: Option<String>,
    pub enemy_type: String,
    pub enemy_count: i32,
    pub location_description: String,
    pub location_scene_key: String,
    pub location_coord_x: f64,
    pub location_coord_y: f64,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = quest_issuer, public)]
pub struct QuestIssuer {
    #[primary_key]
    pub quest_id: String,
    #[index(btree)]
    pub settlement_id: String,
    #[index(btree)]
    pub service_id: String,
}

/// A quest-backed strategic interruption which offers tactical combat,
/// autoresolve, or retreat through the normal encounter flow.
#[derive(Clone, Debug)]
#[table(accessor = strategic_incident, public)]
pub struct StrategicIncident {
    #[primary_key]
    pub quest_id: String,
    #[index(btree)]
    pub party_id: String,
    pub settlement_id: String,
    pub instigator_id: u64,
    pub previous_active_quest_id: Option<String>,
    pub kind: String,
    pub status: String,
}

#[derive(Clone, Debug)]
#[table(accessor = party, public)]
pub struct Party {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_quest_location_id: Option<String>,
    pub active_quest_id: Option<String>,
    pub is_solo: bool,
    #[default(0.0)]
    pub medicine_target: f32,
    #[default(0.0)]
    pub surgery_target: f32,
    #[default(0.0)]
    pub charisma_target: f32,
    #[default(0.0)]
    pub faith_target: f32,
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
            .party()
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
    pub quest_id: String,
    #[index(btree)]
    pub party_id: String,
    pub mission_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_loot_item, public)]
pub struct BattleLootItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub quest_id: String,
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
    pub quest_id: String,
    pub character_id: u64,
}

#[derive(SpacetimeType, Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    pub charisma: u8,
    pub faith: u8,
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
            charisma: value.charisma,
            faith: value.faith,
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
#[table(accessor = party_action_request, public)]
pub struct PartyActionRequest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub requester_id: u64,
    pub action_kind: String,
    pub summary: String,
    pub payload: String,
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
#[table(accessor = local_chat_message, public)]
pub struct LocalChatMessage {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub conversation_key: String,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

fn same_location(left: &crate::Character, right: &crate::Character) -> bool {
    left.current_settlement_id == right.current_settlement_id
        && left.current_quest_location_id == right.current_quest_location_id
        && (left.current_settlement_id.is_some() || left.current_quest_location_id.is_some())
}

fn player_conversation_key(
    ctx: &ReducerContext,
    sender: &crate::Character,
    subject_id: u64,
) -> Result<String, String> {
    let subject = ctx
        .db
        .character()
        .id()
        .find(subject_id)
        .ok_or("Conversation subject not found")?;
    if !same_location(sender, &subject) {
        return Err("Local conversations require a shared location".into());
    }
    let sender_party = sender.party_id.as_deref().ok_or("Sender has no party")?;
    let subject_party = subject.party_id.as_deref().ok_or("Subject has no party")?;
    let (first, second) = if sender_party <= subject_party {
        (sender_party, subject_party)
    } else {
        (subject_party, sender_party)
    };
    Ok(format!("players:{first}:{second}"))
}

fn npc_conversation_key(sender: &crate::Character, subject_id: &str) -> Result<String, String> {
    let party_id = sender.party_id.as_deref().ok_or("Sender has no party")?;
    let settlement_id = sender
        .current_settlement_id
        .as_deref()
        .ok_or("NPC conversations require a settlement")?;
    if !subject_id.starts_with(&format!("{settlement_id}:")) {
        return Err("NPC is not at the sender's settlement".into());
    }
    Ok(format!("npc:{party_id}:{subject_id}"))
}

#[reducer]
pub fn send_local_chat_message(
    ctx: &ReducerContext,
    sender_id: u64,
    subject_kind: String,
    subject_id: String,
    body: String,
) -> Result<(), String> {
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
    let conversation_key = match subject_kind.as_str() {
        "player" => player_conversation_key(
            ctx,
            &sender,
            subject_id.parse().map_err(|_| "Invalid player subject")?,
        )?,
        "npc" => npc_conversation_key(&sender, &subject_id)?,
        _ => return Err("Unknown Local conversation subject".into()),
    };
    ctx.db.local_chat_message().insert(LocalChatMessage {
        id: 0,
        conversation_key,
        sender_id,
        sender_name: sender.name,
        body: body.to_string(),
        created_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    Ok(())
}

#[reducer]
pub fn record_local_npc_message(
    ctx: &ReducerContext,
    actor_id: u64,
    subject_id: String,
    npc_name: String,
    body: String,
) -> Result<(), String> {
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Character not found")?;
    let body = body.trim();
    if body.is_empty() || body.chars().count() > 1000 {
        return Err("NPC messages must contain 1 to 1000 characters".into());
    }
    let conversation_key = npc_conversation_key(&actor, &subject_id)?;
    ctx.db.local_chat_message().insert(LocalChatMessage {
        id: 0,
        conversation_key,
        sender_id: 0,
        sender_name: npc_name.trim().to_string(),
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
    let requester = ctx
        .db
        .character()
        .id()
        .find(requester_id)
        .ok_or("Character not found")?;
    let party_id = requester.party_id.ok_or("Character has no party")?;
    let party = ctx
        .db
        .party()
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
        "accept_join",
        "reject_join",
        "accept_quest",
        "abandon_quest",
        "turn_in_quest",
        "autoresolve",
        "party_checks",
        "party_inventory",
        "disband_party",
        "initiate_combat",
        "cancel_mission",
    ];
    if !allowed.contains(&action_kind.as_str()) {
        return Err("Unknown party action request".into());
    }
    // Travel destinations supersede one another. Inventory target/staging edits
    // are intentionally coalesced to one notification per requesting member.
    if action_kind == "travel" || action_kind == "party_inventory" {
        let old: Vec<_> = ctx
            .db
            .party_action_request()
            .requester_id()
            .filter(requester_id)
            .filter(|request| request.party_id == party_id && request.action_kind == action_kind)
            .map(|request| request.id)
            .collect();
        for id in old {
            ctx.db.party_action_request().id().delete(id);
        }
    }
    ctx.db.party_action_request().insert(PartyActionRequest {
        id: 0,
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
    let request = ctx
        .db
        .party_action_request()
        .id()
        .find(request_id)
        .ok_or("Request not found")?;
    let party = ctx
        .db
        .party()
        .id()
        .find(&request.party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can resolve requests".into());
    }
    ctx.db.party_action_request().id().delete(request_id);
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
    let mut party = ctx
        .db
        .party()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    let leader = ctx
        .db
        .character()
        .id()
        .find(party.leader_id)
        .ok_or("Leader not found")?;
    if leader.alive {
        return Err("Succession voting begins only after the leader dies".into());
    }
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
    let living = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .filter(|member| {
            ctx.db
                .character()
                .id()
                .find(member.character_id)
                .is_some_and(|c| c.alive)
        })
        .count();
    let votes = ctx
        .db
        .party_leader_vote()
        .party_id()
        .filter(&party_id)
        .filter(|vote| vote.candidate_id == candidate_id)
        .count();
    if votes * 2 > living {
        party.leader_id = candidate_id;
        party.is_solo = living == 1;
        ctx.db.party().id().update(party);
        let ids: Vec<_> = ctx
            .db
            .party_leader_vote()
            .party_id()
            .filter(&party_id)
            .map(|v| v.id)
            .collect();
        for id in ids {
            ctx.db.party_leader_vote().id().delete(&id);
        }
    }
    Ok(())
}

#[reducer]
pub fn update_character(ctx: &ReducerContext, id: u64, name: String) -> Result<(), String> {
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
    if ctx.db.party().id().find(&party_id).is_none() {
        ctx.db.party().insert(Party {
            id: party_id.clone(),
            name: format!("{}'s party", character.name),
            leader_id: character_id,
            current_settlement_id: character.current_settlement_id.clone(),
            current_quest_location_id: character.current_quest_location_id.clone(),
            active_quest_id: None,
            is_solo: true,
            medicine_target: 0.0,
            surgery_target: 0.0,
            charisma_target: 0.0,
            faith_target: 0.0,
        });
        ctx.db.party_member().insert(PartyMember {
            id: 0,
            party_id: party_id.clone(),
            character_id,
            role: Some("Leader".into()),
            recruitment_role_id: None,
        });
    }
    character.party_id = Some(party_id.clone());
    ctx.db.character().id().update(character);
    Ok(party_id)
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
        requirements.charisma,
        requirements.faith,
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
        .party()
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
        .party()
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
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party()
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
    surgery: f32,
    charisma: f32,
    faith: f32,
) -> Result<(), String> {
    if [medicine, surgery, charisma, faith]
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
        .party()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can configure party checks".into());
    }
    party.medicine_target = medicine;
    party.surgery_target = surgery;
    party.charisma_target = charisma;
    party.faith_target = faith;
    ctx.db.party().id().update(party);
    Ok(())
}

#[reducer]
pub fn delete_saved_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    role_id: u64,
) -> Result<(), String> {
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
    requirements.charisma = 0;
    requirements.faith = 0;
    requirements
}

#[reducer]
pub fn request_to_join_party(
    ctx: &ReducerContext,
    character_id: u64,
    recruitment_role_id: u64,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let current_party_id = character.party_id.clone().ok_or("Character has no party")?;
    let current_party = ctx
        .db
        .party()
        .id()
        .find(&current_party_id)
        .ok_or("Current party not found")?;
    if current_party.leader_id != character_id {
        return Err("Only a party leader may request a party merge".into());
    }
    if current_party.active_quest_id.is_some() {
        return Err("Abandon the current quest before joining another party".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    let party_id = role.party_id.clone();
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if current_party_id == party_id {
        return Err("Cannot join your own party".into());
    }
    if current_party.current_settlement_id != party.current_settlement_id
        || current_party.current_quest_location_id != party.current_quest_location_id
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
        return Err("A join request is already pending".into());
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
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
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
    let source_party = ctx
        .db
        .party()
        .id()
        .find(&source_party_id)
        .ok_or("Applicant party not found")?;
    if source_party.leader_id != request.character_id {
        return Err("Applicant is no longer their party leader".into());
    }
    if source_party.active_quest_id.is_some() {
        return Err("Applicant's party must abandon its current quest first".into());
    }
    if source_party.current_settlement_id != party.current_settlement_id
        || source_party.current_quest_location_id != party.current_quest_location_id
    {
        return Err("Parties must be in the same location to merge".into());
    }

    // Preserve the source party's jointly-owned assets and each member's absolute
    // stake. Combining the ledgers does not dilute either party; only future loot
    // is shared among the newly combined membership.
    for entry in ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        add_to_party_inventory(ctx, &request.party_id, &entry.item_id, entry.quantity);
        ctx.db.party_inventory_item().id().delete(entry.id);
    }
    for stake in ctx
        .db
        .party_stake()
        .party_id()
        .filter(&source_party_id)
        .collect::<Vec<_>>()
    {
        credit_party_stake(ctx, &request.party_id, stake.character_id, stake.value);
        ctx.db.party_stake().id().delete(stake.id);
    }
    if let Some(state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&source_party_id)
    {
        credit_party_reserve(ctx, &request.party_id, state.reserve_value);
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
            source_character.current_quest_location_id = party.current_quest_location_id.clone();
            ctx.db.character().id().update(source_character);
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
    for member_id in source_member_ids {
        for pending in ctx
            .db
            .party_join_request()
            .character_id()
            .filter(member_id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
    }
    ctx.db.party().id().delete(&source_party_id);
    if party.is_solo {
        let mut party = party;
        party.is_solo = false;
        ctx.db.party().id().update(party);
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
    Ok(())
}

#[reducer]
pub fn reject_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can reject join requests".into());
    }
    ctx.db.party_join_request().id().delete(request_id);
    Ok(())
}

#[reducer]
pub fn seed_bot_join_requests(
    ctx: &ReducerContext,
    recruitment_role_id: u64,
) -> Result<(), String> {
    use petname::Generator;

    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    let party = ctx
        .db
        .party()
        .id()
        .find(&role.party_id)
        .ok_or("Party not found")?;
    for index in 0..role.quantity.saturating_mul(2).min(16) {
        let name = petname::Petnames::default()
            .generate(&mut ctx.rng(), 2, " ")
            .ok_or("Could not generate bot applicant name")?;
        let mut id = ctx.random::<u64>() | (1_u64 << 63);
        while ctx.db.character().id().find(id).is_some() {
            id = ctx.random::<u64>() | (1_u64 << 63);
        }
        crate::character::insert_new_character(ctx, name, id, true)?;
        let mut bot = ctx.db.character().id().find(id).unwrap();
        bot.current_settlement_id = party.current_settlement_id.clone();
        bot.current_quest_location_id = party.current_quest_location_id.clone();
        ctx.db.character().id().update(bot);
        let mut attrs = ctx
            .db
            .character_attributes()
            .character_id()
            .find(id)
            .unwrap();
        attrs.endurance = 10.0;
        attrs.left_arm_strength = 10.0;
        attrs.right_arm_strength = 10.0;
        attrs.left_arm_agility = 10.0;
        attrs.right_arm_agility = 10.0;
        attrs.left_leg_agility = 10.0;
        attrs.right_leg_agility = 10.0;
        attrs.intelligence = 10.0;
        attrs.instinct = 10.0;
        let mut skills = ctx.db.character_skills().character_id().find(id).unwrap();
        skills.medicine_hours = 1_000_000.0;
        skills.surgeon_hours = 1_000_000.0;
        skills.charisma_hours = 1_000_000.0;
        skills.faith_hours = 1_000_000.0;
        crate::character::add_and_equip_item(
            ctx,
            id,
            "bot_multirole_weapon",
            crate::ItemSlot::RightHolding,
        )?;
        if role.requirements.full_armor {
            for (item, slot) in [
                ("bot_plate_arm", crate::ItemSlot::LeftArm),
                ("bot_plate_arm", crate::ItemSlot::RightArm),
                ("bot_plate_leg", crate::ItemSlot::LeftLeg),
                ("bot_plate_leg", crate::ItemSlot::RightLeg),
                ("bot_plate_chest", crate::ItemSlot::Chest),
                ("bot_plate_stomach", crate::ItemSlot::Stomach),
                ("bot_plate_helmet", crate::ItemSlot::Head),
            ] {
                crate::character::add_and_equip_item(ctx, id, item, slot)?;
            }
        }

        if index % 2 == 1 {
            let requirements = role.requirements;
            if requirements.endurance > 0 {
                attrs.endurance = 0.0;
            } else if requirements.athletics > 0 || requirements.heavy {
                attrs.left_arm_strength = 0.0;
                attrs.right_arm_strength = 0.0;
                if requirements.athletics > 0 {
                    attrs.left_arm_agility = 0.0;
                    attrs.right_arm_agility = 0.0;
                    attrs.left_leg_agility = 0.0;
                    attrs.right_leg_agility = 0.0;
                    attrs.endurance = 0.0;
                }
            } else if requirements.medicine > 0 {
                skills.medicine_hours = 0.0;
            } else if requirements.surgery > 0 {
                skills.surgeon_hours = 0.0;
            } else if requirements.charisma > 0 {
                skills.charisma_hours = 0.0;
                attrs.intelligence = 0.0;
                attrs.instinct = 0.0;
            } else if requirements.faith > 0 {
                skills.faith_hours = 0.0;
            } else if requirements.quarter_armor
                || requirements.half_armor
                || requirements.three_quarter_armor
                || requirements.full_armor
            {
                let mut equip = ctx.db.character_equip().character_id().find(id).unwrap();
                equip.left_arm_armor_id = None;
                equip.right_arm_armor_id = None;
                equip.left_leg_armor_id = None;
                equip.right_leg_armor_id = None;
                equip.chest_armor_id = None;
                equip.stomach_armor_id = None;
                equip.head_armor_id = None;
                ctx.db.character_equip().character_id().update(equip);
            } else if requirements.melee {
                crate::character::add_and_equip_item(
                    ctx,
                    id,
                    "short_bow",
                    crate::ItemSlot::RightHolding,
                )?;
            } else if requirements.ranged {
                crate::character::add_and_equip_item(
                    ctx,
                    id,
                    "club",
                    crate::ItemSlot::RightHolding,
                )?;
            } else if role.weapon_precision > 0.0 {
                let mut equip = ctx.db.character_equip().character_id().find(id).unwrap();
                equip.left_hand_item_id = None;
                equip.right_hand_item_id = None;
                ctx.db.character_equip().character_id().update(equip);
            }
        }
        ctx.db.character_attributes().character_id().update(attrs);
        ctx.db.character_skills().character_id().update(skills);
        request_to_join_party(ctx, id, recruitment_role_id)?;
    }
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

    if source_item.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory_item_id);
    } else {
        let mut updated = source_item.clone();
        updated.quantity -= quantity;
        ctx.db.inventory_item().id().update(updated);
    }
    if let Some(mut destination_item) = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((to_character_id, &source_item.item_id))
        .next()
    {
        destination_item.quantity = destination_item.quantity.saturating_add(quantity);
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

fn add_to_party_inventory(ctx: &ReducerContext, party_id: &str, item_id: &str, quantity: u32) {
    if quantity == 0 {
        return;
    }
    if let Some(mut stack) = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .find(|stack| stack.item_id == item_id)
    {
        stack.quantity = stack.quantity.saturating_add(quantity);
        ctx.db.party_inventory_item().id().update(stack);
    } else {
        ctx.db.party_inventory_item().insert(PartyInventoryItem {
            id: 0,
            party_id: party_id.to_string(),
            item_id: item_id.to_string(),
            quantity,
        });
    }
}

fn credit_party_stake(ctx: &ReducerContext, party_id: &str, character_id: u64, value: u64) {
    if value == 0 {
        return;
    }
    if let Some(mut stake) = ctx
        .db
        .party_stake()
        .party_id()
        .filter(party_id)
        .find(|stake| stake.character_id == character_id)
    {
        stake.value = stake.value.saturating_add(value);
        ctx.db.party_stake().id().update(stake);
    } else {
        ctx.db.party_stake().insert(PartyStake {
            id: 0,
            party_id: party_id.to_string(),
            character_id,
            value,
        });
    }
}

fn credit_party_reserve(ctx: &ReducerContext, party_id: &str, value: u64) {
    if value == 0 {
        return;
    }
    if let Some(mut state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
    {
        state.reserve_value = state.reserve_value.saturating_add(value);
        ctx.db.party_inventory_state().party_id().update(state);
    } else {
        ctx.db.party_inventory_state().insert(PartyInventoryState {
            party_id: party_id.to_string(),
            reserve_value: value,
        });
    }
}

pub(crate) fn record_battle_result(
    ctx: &ReducerContext,
    party_id: &str,
    quest_id: &str,
    mission_id: &str,
    dropped_items: Vec<(String, u32)>,
    include_random_quest_gold: bool,
) -> Result<(), String> {
    if ctx
        .db
        .battle_result()
        .quest_id()
        .find(&quest_id.to_string())
        .is_some()
    {
        return Ok(());
    }
    let quest = ctx
        .db
        .quest()
        .id()
        .find(&quest_id.to_string())
        .ok_or("Quest not found")?;
    ctx.db.battle_result().insert(BattleResult {
        quest_id: quest_id.to_string(),
        party_id: party_id.to_string(),
        mission_id: mission_id.to_string(),
    });
    for member in ctx.db.party_member().party_id().filter(party_id) {
        ctx.db.battle_participant().insert(BattleParticipant {
            id: 0,
            quest_id: quest_id.to_string(),
            character_id: member.character_id,
        });
        crate::condition::record_morale_event(
            ctx,
            member.character_id,
            "victory",
            5.0 + quest.difficulty.max(0) as f32,
            Some(quest_id.to_string()),
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
    if include_random_quest_gold && ctx.random::<u64>().is_multiple_of(2) {
        let maximum_gold = quest.difficulty.max(1) as u32 * 10;
        let gold = 1 + (ctx.random::<u64>() % u64::from(maximum_gold)) as u32;
        if gold > 0 {
            *combined.entry("gold_coin".into()).or_default() += gold;
        }
    }
    for (item_id, quantity) in combined {
        ctx.db.battle_loot_item().insert(BattleLootItem {
            id: 0,
            quest_id: quest_id.to_string(),
            item_id,
            quantity,
        });
    }
    Ok(())
}

#[reducer]
pub fn store_battle_loot(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
    loot_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    if loot_item_ids.len() != quantities.len() {
        return Err("Loot entries must be aligned".into());
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
        .quest_id()
        .find(&quest_id)
        .ok_or("Battle result not found")?;
    if result.party_id != party_id {
        return Err("Battle loot belongs to another party".into());
    }
    let available: Vec<_> = ctx
        .db
        .battle_loot_item()
        .quest_id()
        .filter(&quest_id)
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
        total_value = total_value.saturating_add(
            objective_item_value(ctx, &entry.item_id)?.saturating_mul(u64::from(entry.quantity)),
        );
    }
    let participants: Vec<_> = ctx
        .db
        .battle_participant()
        .quest_id()
        .filter(&quest_id)
        .collect();
    if participants.is_empty() {
        return Err("Battle has no eligible participants".into());
    }
    for entry in loot {
        add_to_party_inventory(ctx, &party_id, &entry.item_id, entry.quantity);
        let original = ctx.db.battle_loot_item().id().find(entry.id).unwrap();
        if original.quantity == entry.quantity {
            ctx.db.battle_loot_item().id().delete(entry.id);
        } else {
            let mut original = original;
            original.quantity -= entry.quantity;
            ctx.db.battle_loot_item().id().update(original);
        }
    }
    let participant_count = participants.len() as u64;
    let share = total_value / participant_count;
    for participant in participants {
        credit_party_stake(ctx, &party_id, participant.character_id, share);
    }
    credit_party_reserve(ctx, &party_id, total_value % participant_count);
    Ok(())
}

#[reducer]
pub fn deposit_party_inventory_item(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
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
    let value = objective_item_value(ctx, &inventory.item_id)?.saturating_mul(u64::from(quantity));
    add_to_party_inventory(ctx, &party_id, &inventory.item_id, quantity);
    credit_party_stake(ctx, &party_id, character_id, value);
    if inventory.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory.id);
    } else {
        inventory.quantity -= quantity;
        ctx.db.inventory_item().id().update(inventory);
    }
    Ok(())
}

fn consume_personal_gold(
    ctx: &ReducerContext,
    character_id: u64,
    mut amount: u64,
) -> Result<(), String> {
    let stacks: Vec<_> = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, &"gold_coin".to_string()))
        .collect();
    let available: u64 = stacks.iter().map(|stack| u64::from(stack.quantity)).sum();
    if available < amount {
        return Err("Not enough personal gold to cover this withdrawal".into());
    }
    for mut stack in stacks {
        let taken = amount.min(u64::from(stack.quantity)) as u32;
        stack.quantity -= taken;
        amount -= u64::from(taken);
        if stack.quantity == 0 {
            ctx.db.inventory_item().id().delete(stack.id);
        } else {
            ctx.db.inventory_item().id().update(stack);
        }
        if amount == 0 {
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
    let cost = objective_item_value(ctx, &inventory.item_id)?.saturating_mul(u64::from(quantity));
    let mut stake = ctx
        .db
        .party_stake()
        .party_id()
        .filter(&party_id)
        .find(|stake| stake.character_id == character_id);
    let stake_value = stake.as_ref().map_or(0, |stake| stake.value);
    if cost > stake_value {
        let top_up = cost - stake_value;
        consume_personal_gold(ctx, character_id, top_up)?;
        add_to_party_inventory(ctx, &party_id, "gold_coin", top_up as u32);
    }
    if let Some(ref mut stake) = stake {
        stake.value = stake.value.saturating_sub(cost);
        ctx.db.party_stake().id().update(stake.clone());
    }
    crate::add_inventory_item(ctx, character_id, &inventory.item_id, quantity);
    if inventory.quantity == quantity {
        ctx.db.party_inventory_item().id().delete(inventory.id);
    } else {
        inventory.quantity -= quantity;
        ctx.db.party_inventory_item().id().update(inventory);
    }
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
            || entry.item_id == "gold_coin"
        {
            return Err("Invalid party asset liquidation".into());
        }
        proceeds = proceeds.saturating_add(
            objective_item_value(ctx, &entry.item_id)?.saturating_mul(u64::from(quantity)),
        );
        staged.push((entry, quantity));
    }
    for (mut entry, quantity) in staged {
        if entry.quantity == quantity {
            ctx.db.party_inventory_item().id().delete(entry.id);
        } else {
            entry.quantity -= quantity;
            ctx.db.party_inventory_item().id().update(entry);
        }
    }
    add_to_party_inventory(ctx, &party_id, "gold_coin", proceeds as u32);
    Ok(())
}

#[reducer]
pub fn discard_inventory_items(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
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
        } else {
            item.quantity -= quantity;
            ctx.db.inventory_item().id().update(item);
        }
    }
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
    let mut net: HashMap<String, i64> = HashMap::new();
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        *net.entry(item_id.clone()).or_default() += *quantity as i64;
    }
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let item_id = if party_scope {
            ctx.db
                .party_inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Party inventory item not found")?
                .item_id
        } else {
            ctx.db
                .inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Inventory item not found")?
                .item_id
        };
        *net.entry(item_id).or_default() -= *quantity as i64;
    }
    let buy_item_ids: Vec<String> = net
        .iter()
        .filter(|(_, value)| **value > 0)
        .map(|(item, _)| item.clone())
        .collect();
    let buy_quantities: Vec<u32> = buy_item_ids.iter().map(|item| net[item] as u32).collect();
    let sell_inventory_ids: Vec<u64> = sell_inventory_ids
        .into_iter()
        .filter(|id| {
            let item_id = if party_scope {
                ctx.db
                    .party_inventory_item()
                    .id()
                    .find(*id)
                    .map(|v| v.item_id)
            } else {
                ctx.db.inventory_item().id().find(*id).map(|v| v.item_id)
            };
            item_id.is_some_and(|item_id| net.get(&item_id).copied().unwrap_or(0) < 0)
        })
        .collect();
    let sell_quantities: Vec<u32> = sell_inventory_ids
        .iter()
        .map(|id| {
            let item_id = if party_scope {
                ctx.db
                    .party_inventory_item()
                    .id()
                    .find(*id)
                    .unwrap()
                    .item_id
            } else {
                ctx.db.inventory_item().id().find(*id).unwrap().item_id
            };
            (-net[&item_id]) as u32
        })
        .collect();
    let mut cost = 0_u32;
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        let Some(item) = ctx.db.item().id().find(item_id) else {
            return Err("Merchant item not found".into());
        };
        if item.kind == crate::ItemKind::Currency || *quantity == 0 {
            return Err("Invalid merchant purchase".into());
        }
        cost = cost.saturating_add(
            (item.base_value.unwrap_or(1) as f32 * MERCHANT_MARGIN * (1.0 + SALES_TAX)).ceil()
                as u32
                * quantity,
        );
    }
    let mut proceeds = 0_u32;
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let (item_id, available) = if party_scope {
            let inventory = ctx
                .db
                .party_inventory_item()
                .id()
                .find(*inventory_id)
                .ok_or("Party inventory item not found")?;
            if Some(&inventory.party_id) != party_id.as_ref() {
                return Err("Invalid party inventory sale".into());
            }
            (inventory.item_id, inventory.quantity)
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
            (inventory.item_id, inventory.quantity)
        };
        let Some(item) = ctx.db.item().id().find(&item_id) else {
            return Err("Item definition not found".into());
        };
        if available < *quantity || *quantity == 0 || item.kind == crate::ItemKind::Currency {
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
        proceeds = proceeds.saturating_add(
            (item.base_value.unwrap_or(1) as f32 / MERCHANT_MARGIN)
                .floor()
                .max(1.0) as u32
                * quantity,
        );
    }
    let coins: u32 = if party_scope {
        let party_id = party_id.as_ref().ok_or("Character has no party")?;
        ctx.db
            .party_inventory_item()
            .party_id()
            .filter(party_id)
            .filter(|v| v.item_id == "gold_coin")
            .map(|v| v.quantity)
            .sum()
    } else {
        ctx.db
            .inventory_item()
            .character_and_item_id()
            .filter((character_id, &"gold_coin".to_string()))
            .map(|coin| coin.quantity)
            .sum()
    };
    if coins.saturating_add(proceeds) < cost {
        return Err("Not enough gold".into());
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
                ctx.db.party_inventory_item().id().delete(*inventory_id);
            } else {
                inventory.quantity -= quantity;
                ctx.db.party_inventory_item().id().update(inventory);
            }
        } else {
            let mut inventory = ctx.db.inventory_item().id().find(*inventory_id).unwrap();
            if inventory.quantity == *quantity {
                ctx.db.inventory_item().id().delete(*inventory_id);
            } else {
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
        if let Some(mut stack) = ctx
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
            stack.quantity = stack.quantity.saturating_add(*quantity);
            ctx.db.inventory_item().id().update(stack);
        } else {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id,
                item_id: item_id.clone(),
                quantity: *quantity,
            });
        }
    }
    let net = proceeds as i64 - cost as i64;
    if party_scope && net != 0 {
        let party_id = party_id.as_ref().unwrap();
        let mut coin = ctx
            .db
            .party_inventory_item()
            .party_id()
            .filter(party_id)
            .find(|v| v.item_id == "gold_coin");
        if let Some(ref mut coin) = coin {
            coin.quantity = (coin.quantity as i64 + net) as u32;
            ctx.db.party_inventory_item().id().update(coin.clone());
        } else if net > 0 {
            add_to_party_inventory(ctx, party_id, "gold_coin", net as u32);
        }
    } else if net != 0 {
        if let Some(mut coin) = ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((character_id, &"gold_coin".to_string()))
            .next()
        {
            coin.quantity = (coin.quantity as i64 + net) as u32;
            ctx.db.inventory_item().id().update(coin);
        } else if net > 0 {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id,
                item_id: "gold_coin".into(),
                quantity: net as u32,
            });
        }
    }
    Ok(())
}

/// Adds two deterministic companions to the specified character's party for
/// strategic UI development. Safe to call repeatedly.
#[reducer]
pub fn seed_party_companions(ctx: &ReducerContext, leader_id: u64) -> Result<(), String> {
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.clone().ok_or("Leader has no party")?;
    let role = ctx
        .db
        .party_recruitment_role()
        .insert(PartyRecruitmentRole {
            id: 0,
            party_id: party_id.clone(),
            name: "Companion".into(),
            requirements: RecruitmentRequirements::default(),
            quantity: 2,
            weapon_precision: 0.0,
        });

    for (id, name) in [(9_000_001_u64, "Mara"), (9_000_002_u64, "Orrin")] {
        if ctx.db.character().id().find(id).is_none() {
            crate::character::insert_new_character(ctx, name.into(), id, false)?;
        }
        let mut companion = ctx.db.character().id().find(id).unwrap();
        if companion.party_id.as_deref() == Some(&party_id) {
            continue;
        }
        companion.current_settlement_id = leader.current_settlement_id.clone();
        companion.current_quest_location_id = leader.current_quest_location_id.clone();
        ctx.db.character().id().update(companion);
        request_to_join_party(ctx, id, role.id)?;
        let request = ctx
            .db
            .party_join_request()
            .character_id()
            .filter(id)
            .find(|request| request.recruitment_role_id == role.id)
            .unwrap();
        accept_party_join_request(ctx, leader_id, request.id)?;
    }
    Ok(())
}

#[reducer]
pub fn leave_party(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
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
    let Some(actor) = ctx.db.character().id().find(actor_character_id) else {
        return Err("Acting character not found".into());
    };
    let Some(mut character) = ctx.db.character().id().find(member_character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Character is not in a party".into());
    };

    let Some(party) = ctx.db.party().id().find(&party_id) else {
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
    create_solo_party_for_character(ctx, member_character_id)?;
    Ok(())
}

#[reducer]
pub fn disband_party(ctx: &ReducerContext, leader_id: u64, party_id: String) -> Result<(), String> {
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can disband the party".into());
    }
    if party.current_quest_location_id.is_some() {
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
        .any(|entry| entry.item_id != "gold_coin")
        || pooled_items
            .iter()
            .map(|entry| u64::from(entry.quantity))
            .sum::<u64>()
            != reserve
    {
        return Err("Liquidate and distribute the party inventory before disbanding".into());
    }
    if reserve > 0 {
        crate::add_inventory_item(ctx, party.leader_id, "gold_coin", reserve as u32);
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

    if let Some(quest_id) = party.active_quest_id {
        ctx.db.quest().id().delete(&quest_id);
    }

    ctx.db.party().id().delete(&party_id);
    for character_id in member_ids {
        create_solo_party_for_character(ctx, character_id)?;
    }
    Ok(())
}

#[reducer]
pub fn accept_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to accept quests".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can accept quests".into());
    }

    if party.active_quest_id.is_some() {
        return Err("Party already has an active quest".into());
    }

    let Some(mut quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.status != QuestStatus::Available {
        return Err("Quest is not available".into());
    }

    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Must be at the quest's settlement to accept it".into());
    }

    quest.status = QuestStatus::Accepted;
    quest.accepted_by = Some(party_id.clone());
    ctx.db.quest().id().update(quest);

    party.active_quest_id = Some(quest_id);
    ctx.db.party().id().update(party);
    Ok(())
}

#[reducer]
pub fn abandon_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Not in a party".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can abandon quests".into());
    }
    if character.current_quest_location_id.is_some() {
        return Err("Travel to a settlement before abandoning the quest".into());
    }

    let Some(quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("This quest is not accepted by your party".into());
    }
    if quest.status == QuestStatus::Completed {
        return Err("A completed quest must be returned to its questgiver".into());
    }

    ctx.db.quest().id().delete(&quest.id);

    party.active_quest_id = None;
    ctx.db.party().id().update(party);
    Ok(())
}

fn travel_neighbors(ctx: &ReducerContext, node: u64) -> Vec<(u64, u32)> {
    let mut neighbors: Vec<_> = ctx
        .db
        .travel_edge()
        .from_node_id()
        .filter(&node)
        .filter_map(|edge| {
            matches!(edge.kind.as_str(), "land" | "ferry")
                .then_some((edge.to_node_id, edge.length_m))
        })
        .collect();
    neighbors.extend(
        ctx.db
            .travel_edge()
            .to_node_id()
            .filter(&node)
            .filter_map(|edge| {
                matches!(edge.kind.as_str(), "land" | "ferry")
                    .then_some((edge.from_node_id, edge.length_m))
            }),
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
        (earth_radius_m * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())).round() as u64
    } else {
        (((from_x - to_x).powi(2) + (from_y - to_y).powi(2)).sqrt() * METERS_PER_KILOMETER as f64)
            .round() as u64
    }
}

struct IncidentSpec<'a> {
    kind: &'a str,
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
    quest_id: String,
    spec: IncidentSpec<'_>,
) -> Result<Option<String>, String> {
    let Some(mut party) = ctx.db.party().id().find(&party_id.to_string()) else {
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
        .any(|incident| incident.status == "pending")
    {
        return Ok(None);
    }
    ctx.db.quest().insert(Quest {
        id: quest_id.clone(),
        title: spec.title.into(),
        description: spec.description.clone(),
        difficulty: spec.difficulty,
        gold_reward: 0,
        xp_reward: 0,
        settlement_id: settlement.id.clone(),
        status: QuestStatus::Accepted,
        accepted_by: Some(party_id.into()),
        enemy_type: spec.enemy_type.into(),
        enemy_count: ctx
            .db
            .party_member()
            .party_id()
            .filter(party_id)
            .count()
            .max(2) as i32,
        location_description: spec.description,
        location_scene_key: settlement.scene_key.clone(),
        location_coord_x: settlement.coord_x,
        location_coord_y: settlement.coord_y,
        coordinates_are_geographic: settlement.source_node_id.is_some(),
        distance_m: 0,
    });
    ctx.db.strategic_incident().insert(StrategicIncident {
        quest_id: quest_id.clone(),
        party_id: party_id.into(),
        settlement_id: settlement.id.clone(),
        instigator_id,
        previous_active_quest_id: party.active_quest_id.clone(),
        kind: spec.kind.into(),
        status: "pending".into(),
    });

    for membership in ctx.db.party_member().party_id().filter(party_id) {
        if let Some(mut member) = ctx.db.character().id().find(membership.character_id) {
            member.current_settlement_id = None;
            member.current_quest_location_id = Some(quest_id.clone());
            ctx.db.character().id().update(member);
        }
    }
    party.current_settlement_id = None;
    party.current_quest_location_id = Some(quest_id.clone());
    party.active_quest_id = Some(quest_id.clone());
    ctx.db.party().id().update(party);
    Ok(Some(quest_id))
}

fn maybe_trigger_religious_incident(
    ctx: &ReducerContext,
    party_id: &str,
    settlement: &Settlement,
) -> Result<Option<String>, String> {
    if ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .any(|incident| incident.kind == "religious" && incident.settlement_id == settlement.id)
    {
        return Ok(None);
    }
    let mut instigator = None;
    for membership in ctx.db.party_member().party_id().filter(party_id) {
        crate::condition::initialize_character_condition(ctx, membership.character_id)?;
        let religion = ctx
            .db
            .character_condition()
            .character_id()
            .find(membership.character_id)
            .and_then(|condition| condition.religion_id);
        if religion
            .as_deref()
            .is_none_or(|faith| faith == settlement.religion_id)
        {
            continue;
        }
        let condition =
            crate::condition::refresh_character_strategic_condition(ctx, membership.character_id)?;
        if instigator
            .as_ref()
            .is_none_or(|(_, fervor)| condition.fervor > *fervor)
        {
            instigator = Some((membership.character_id, condition.fervor));
        }
    }
    let Some((instigator_id, instigator_fervor)) = instigator else {
        return Ok(None);
    };
    let roll = (ctx.random::<u64>() >> 40) as f32 / ((1_u32 << 24) as f32);
    if !fervor_event_occurs(instigator_fervor, roll) {
        return Ok(None);
    }
    let quest_id = format!("religious-incident-{party_id}-{}", settlement.id);
    create_strategic_incident(
        ctx,
        party_id,
        settlement,
        instigator_id,
        quest_id,
        IncidentSpec {
            kind: "religious",
            title: "A Quarrel at the Gate",
            description: format!(
                "At the gate of {}, a loud insult against the local faith has drawn an angry crowd. Combat is imminent, but the party can still withdraw and travel away.",
                settlement.name
            ),
            enemy_type: "angry townsfolk",
            difficulty: 1,
        },
    )
}

pub(crate) fn maybe_trigger_activity_incident(
    ctx: &ReducerContext,
    character_id: u64,
    risks: crate::time::ActivityRisks,
) -> Result<Option<String>, String> {
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
            "armed retainers",
            2,
        ))
    } else if fervor_event_occurs(risks.thievery_discovery, roll(ctx)) {
        Some((
            "thievery",
            "Caught Red-Handed",
            "A theft has been discovered and the watch has cornered the party near the market. Fight through them or abandon the settlement.",
            "town watch",
            1,
        ))
    } else {
        None
    };
    let Some((kind, title, description, enemy_type, difficulty)) = outcome else {
        return Ok(None);
    };
    let quest_id = format!(
        "{kind}-incident-{party_id}-{}-{}",
        settlement.id,
        ctx.random::<u64>()
    );
    create_strategic_incident(
        ctx,
        party_id,
        &settlement,
        character_id,
        quest_id,
        IncidentSpec {
            kind,
            title,
            description: description.into(),
            enemy_type,
            difficulty,
        },
    )
}

fn finish_strategic_incident(
    ctx: &ReducerContext,
    quest_id: &str,
    status: &str,
) -> Result<(), String> {
    let Some(mut incident) = ctx
        .db
        .strategic_incident()
        .quest_id()
        .find(&quest_id.to_string())
    else {
        return Ok(());
    };
    if incident.status != "pending" {
        return Ok(());
    }
    incident.status = status.into();
    ctx.db
        .strategic_incident()
        .quest_id()
        .update(incident.clone());
    if let Some(mut party) = ctx.db.party().id().find(&incident.party_id)
        && party.active_quest_id.as_deref() == Some(quest_id)
    {
        party.active_quest_id = incident.previous_active_quest_id;
        ctx.db.party().id().update(party);
    }
    if status == "avoided"
        && let Some(mut quest) = ctx.db.quest().id().find(&quest_id.to_string())
    {
        quest.status = QuestStatus::Completed;
        ctx.db.quest().id().update(quest);
    }
    Ok(())
}

#[reducer]
pub fn travel_to_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
    provision: bool,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to travel to a quest".into());
    };
    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can travel".into());
    }
    if party.active_quest_id.as_deref() != Some(&quest_id) {
        return Err("This is not the party's active quest".into());
    }
    let Some(quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };
    if quest.status != QuestStatus::Accepted || quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("Quest is not accepted by this party".into());
    }
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Travel to the quest must begin at its posting settlement".into());
    }
    require_party_ready(ctx, &party_id)?;

    let travel_minutes = quest_journey_minutes(quest.distance_m);
    if provision {
        let planning_minutes = travel_minutes.saturating_mul(2);
        for membership in ctx.db.party_member().party_id().filter(&party_id) {
            crate::condition::provision_character_for_travel(
                ctx,
                membership.character_id,
                planning_minutes,
            )?;
        }
    }
    for membership in ctx.db.party_member().party_id().filter(&party_id) {
        if let Some(mut member) = ctx.db.character().id().find(membership.character_id) {
            advance_character_time(ctx, member.id, travel_minutes)?;
            member.current_settlement_id = None;
            member.current_quest_location_id = Some(quest_id.clone());
            ctx.db.character().id().update(member);
        }
    }
    party.current_settlement_id = None;
    party.current_quest_location_id = Some(quest_id);
    ctx.db.party().id().update(party);
    Ok(())
}

#[reducer]
pub fn travel_to_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    provision: bool,
) -> Result<(), String> {
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
                .party()
                .id()
                .find(party_id)
                .ok_or_else(|| "Party not found".to_string())
        })
        .transpose()?;
    if let Some(party) = party.as_ref() {
        if party.leader_id != character_id {
            return Err("Only the party leader can travel".into());
        }
        require_party_ready(ctx, &party.id)?;
    } else {
        crate::condition::require_character_ready(ctx, character_id)?;
    }

    let travel_minutes = if let Some(origin_id) = &character.current_settlement_id {
        let Some(origin) = ctx.db.settlement().id().find(origin_id) else {
            return Err("Character's current settlement does not exist".into());
        };
        // Demo settlements remain usable before a Viabundus world is loaded.
        // Imported journeys must lead to the next settlement on the road graph.
        if let (Some(origin_node), Some(destination_node)) =
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
        }
    } else if let Some(quest_id) = &character.current_quest_location_id {
        let Some(quest) = ctx.db.quest().id().find(quest_id) else {
            return Err("Character's current quest location does not exist".into());
        };
        let distance_m = straight_line_distance_m(
            quest.location_coord_x,
            quest.location_coord_y,
            destination.coord_x,
            destination.coord_y,
            quest.coordinates_are_geographic && destination.source_node_id.is_some(),
        );
        quest_journey_minutes(distance_m)
    } else {
        return Err("Character is not at a known location".into());
    };

    let departing_quest = character.current_quest_location_id.clone();
    let traveler_ids: Vec<u64> = if let Some(party) = party.as_ref() {
        ctx.db
            .party_member()
            .party_id()
            .filter(&party.id)
            .map(|membership| membership.character_id)
            .collect()
    } else {
        vec![character_id]
    };
    if provision && character.current_settlement_id.is_some() {
        for traveler_id in traveler_ids.iter().copied() {
            crate::condition::provision_character_for_travel(ctx, traveler_id, travel_minutes)?;
        }
    }
    for traveler_id in traveler_ids {
        advance_character_time(ctx, traveler_id, travel_minutes)?;
        let mut traveler = ctx
            .db
            .character()
            .id()
            .find(traveler_id)
            .ok_or("Party member not found")?;
        traveler.current_settlement_id = Some(settlement_id.clone());
        traveler.current_quest_location_id = None;
        ctx.db.character().id().update(traveler);
        crate::condition::replenish_needs_at_settlement(ctx, traveler_id)?;
        crate::condition::refresh_character_strategic_condition(ctx, traveler_id)?;
        crate::capability::refresh_character_capability(ctx, traveler_id)?;
    }

    if let Some(ref mut party) = party {
        party.current_settlement_id = Some(settlement_id.clone());
        party.current_quest_location_id = None;
        ctx.db.party().id().update(party.clone());
        let fled_incident = departing_quest.as_ref().is_some_and(|quest_id| {
            ctx.db
                .strategic_incident()
                .quest_id()
                .find(quest_id)
                .is_some()
        });
        if let Some(quest_id) = departing_quest.as_deref()
            && fled_incident
        {
            finish_strategic_incident(ctx, quest_id, "avoided")?;
        }
        if !fled_incident {
            maybe_trigger_religious_incident(ctx, &party.id, &destination)?;
        }
    }

    Ok(())
}

#[reducer]
pub fn complete_quest(ctx: &ReducerContext, quest_id: String) -> Result<(), String> {
    let Some(mut quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.status != QuestStatus::Accepted {
        return Err("Quest is not in accepted state".into());
    }

    let Some(party_id) = quest.accepted_by.clone() else {
        return Err("Quest has no party assigned".into());
    };

    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.active_quest_id.as_deref() != Some(&quest_id) {
        return Err("This is not the party's active quest".into());
    }

    let members: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();
    let xp_per_member = quest.xp_reward.max(0) as u32 / members.len().max(1) as u32;

    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(member.character_id) {
            character.xp += xp_per_member;
            character.level = 1 + character.xp / 100;
            ctx.db.character().id().update(character);
        }
    }

    quest.status = QuestStatus::Completed;
    ctx.db.quest().id().update(quest);
    finish_strategic_incident(ctx, &quest_id, "resolved")?;
    Ok(())
}

#[reducer]
pub fn turn_in_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let mut party = ctx
        .db
        .party()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.active_quest_id.as_deref() != Some(&quest_id) {
        return Err("This is not the party's active quest".into());
    }
    let quest = ctx
        .db
        .quest()
        .id()
        .find(&quest_id)
        .ok_or("Quest not found")?;
    if quest.status != QuestStatus::Completed || quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("The quest has not been completed by this party".into());
    }
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Return to the questgiver's settlement to claim the reward".into());
    }

    let reward = quest.gold_reward.max(0) as u64;
    if reward > 0 {
        add_to_party_inventory(ctx, &party_id, "gold_coin", reward as u32);
        let recipients: Vec<u64> = ctx
            .db
            .party_member()
            .party_id()
            .filter(&party_id)
            .map(|member| member.character_id)
            .collect();
        let recipient_count = recipients.len().max(1) as u64;
        let share = reward / recipient_count;
        for recipient in recipients {
            credit_party_stake(ctx, &party_id, recipient, share);
        }
        credit_party_reserve(ctx, &party_id, reward % recipient_count);
    }

    party.active_quest_id = None;
    ctx.db.party().id().update(party);
    let obsolete_requests: Vec<u64> = ctx
        .db
        .party_action_request()
        .party_id()
        .filter(&party_id)
        .filter(|request| request.action_kind == "turn_in_quest")
        .map(|request| request.id)
        .collect();
    for request_id in obsolete_requests {
        ctx.db.party_action_request().id().delete(request_id);
    }
    ensure_settlement_activity_inner(ctx, &quest.settlement_id)?;
    Ok(())
}

#[reducer]
pub fn autoresolve_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id else {
        return Err("Must be in a party".into());
    };
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can autoresolve".into());
    }
    if party.active_quest_id.as_deref() != Some(&quest_id)
        || party.current_quest_location_id.as_deref() != Some(&quest_id)
    {
        return Err("Party must be at its active quest location".into());
    }
    require_party_ready(ctx, &party_id)?;

    let quest = ctx
        .db
        .quest()
        .id()
        .find(&quest_id)
        .ok_or("Quest not found")?;
    if ctx.db.battle_result().quest_id().find(&quest_id).is_some() {
        return Ok(());
    }

    let member_ids: Vec<u64> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .map(|member| member.character_id)
        .collect();
    let surgery_checks = member_ids
        .iter()
        .map(|member_id| {
            crate::capability::evaluate_character(ctx, *member_id)
                .map(|capabilities| capabilities.surgery)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let surgery_check = aggregate_bounded_party_check(surgery_checks);

    for member_id in member_ids {
        if let Some(mut limbs) = ctx.db.character_limbs().character_id().find(member_id) {
            let condition =
                crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
            let base_damage = 0.05 + (ctx.random::<u64>() % 16) as f32 / 100.0;
            let damage = base_damage * (2.0 - condition.check_multiplier);
            let deterioration = post_battle_wound_deterioration(damage, surgery_check);
            let final_damage = damage + deterioration;
            match ctx.random::<u64>() % 7 {
                0 => limbs.left_arm_health = (limbs.left_arm_health - final_damage).max(0.0),
                1 => limbs.right_arm_health = (limbs.right_arm_health - final_damage).max(0.0),
                2 => limbs.left_leg_health = (limbs.left_leg_health - final_damage).max(0.0),
                3 => limbs.right_leg_health = (limbs.right_leg_health - final_damage).max(0.0),
                4 => limbs.head_health = (limbs.head_health - final_damage).max(0.0),
                5 => limbs.chest_health = (limbs.chest_health - final_damage).max(0.0),
                _ => limbs.stomach_health = (limbs.stomach_health - final_damage).max(0.0),
            }
            ctx.db.character_limbs().character_id().update(limbs);
            crate::condition::apply_blood_loss(
                ctx,
                member_id,
                final_damage * AUTORE_SOLVE_BLOOD_LOSS_PER_HEALTH_DAMAGE,
            )?;
            crate::capability::refresh_character_capability(ctx, member_id)?;
        }
    }
    // The current enemy generator equips its placeholder enemies with clubs.
    let dropped_item = "club";
    record_battle_result(
        ctx,
        &party_id,
        &quest_id,
        &format!("autoresolve-{quest_id}"),
        vec![(dropped_item.to_string(), quest.enemy_count.max(0) as u32)],
        true,
    )?;
    complete_quest(ctx, quest_id)
}

#[reducer]
pub fn cancel_mission_request(ctx: &ReducerContext, mission_id: String) -> Result<(), String> {
    ctx.db
        .tactical_server_request()
        .mission_id()
        .delete(&mission_id);
    Ok(())
}

#[reducer]
pub fn seed_world(ctx: &ReducerContext) -> Result<(), String> {
    let settlements = [
        (
            "riverdale",
            "Riverdale",
            0.0,
            0.0,
            3,
            "hills",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::RomanCatholic,
            },
        ),
        (
            "ironforge",
            "Ironforge",
            100.0,
            50.0,
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
            2,
            "hills",
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::EasternOrthodox,
            },
        ),
    ];

    for (id, name, x, y, pop, scene, religious_status) in settlements {
        if ctx.db.settlement().id().find(&id.to_string()).is_none() {
            ctx.db.settlement().insert(Settlement {
                id: id.into(),
                name: name.into(),
                coord_x: x,
                coord_y: y,
                population_level: pop,
                population_estimate: 0,
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
                    PotentialVegetationFormation::DeciduousAndMixedForest,
                ),
                tree_species: TreeSpeciesProfile::Inferred(
                    InferredTreeSpeciesProfile::new(vec![
                        TreeSpeciesId::new("Quercus_robur").unwrap(),
                    ])
                    .unwrap(),
                ),
                soil: SoilProfile::Inferred(SoilProperties {
                    substrate: SoilSubstrate::Mineral(MineralSoil {
                        texture: MineralSoilTexture::Medium,
                        depth: SoilDepth::Deep,
                        available_water: AvailableWaterCapacity::Medium,
                        organic_carbon: TopsoilOrganicCarbon::Medium,
                        stones: StoneContentPercent::new(10).unwrap(),
                    }),
                    water_regime: SoilWaterRegime::SeasonallyWet,
                    agricultural_limitation: AgriculturalLimitation::None,
                }),
                geology: SurfaceGeology::Inferred(InferredGeologicSetting {
                    lithology: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                    age: GeologicEra::Quaternary,
                }),
                religious_status,
                drought: DroughtProfile::Inferred(
                    DroughtHistory::new(
                        PalmerDroughtSeverityIndex::new(0).unwrap(),
                        PalmerDroughtSeverityIndex::new(0).unwrap(),
                        0,
                        0,
                    )
                    .unwrap(),
                ),
                scene_key: scene.into(),
                religion_id: religious_status.church().faith_id().into(),
                source_node_id: None,
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
    let tracked_quests: HashSet<String> = ctx
        .db
        .party()
        .iter()
        .filter_map(|party| party.active_quest_id)
        .collect();
    let active = ctx
        .db
        .quest()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|quest| {
            quest.status != QuestStatus::Completed || tracked_quests.contains(&quest.id)
        })
        .count();
    for _ in active..settlement_activity_target(settlement_id) {
        generate_quest_for_settlement(ctx, settlement_id)?;
    }
    ensure_quest_issuers(ctx, settlement_id);
    ensure_npc_quest_parties(ctx, settlement_id)?;
    Ok(())
}

fn ensure_quest_issuers(ctx: &ReducerContext, settlement_id: &str) {
    for quest in ctx
        .db
        .quest()
        .settlement_id()
        .filter(&settlement_id.to_string())
    {
        if ctx.db.quest_issuer().quest_id().find(&quest.id).is_none() {
            ctx.db.quest_issuer().insert(QuestIssuer {
                quest_id: quest.id,
                settlement_id: settlement_id.to_string(),
                service_id: quest_service_for_title(&quest.title).to_string(),
            });
        }
    }
}

fn quest_service_for_title(title: &str) -> &'static str {
    if title.starts_with("Break Up the Bandit Camp") {
        "merchants"
    } else if title.starts_with("Recover the Stolen Ore")
        || title.starts_with("Recover the Stolen Arms")
    {
        "weapons"
    } else if title.starts_with("Purge the Old Mine") {
        "armor"
    } else if title.starts_with("Hunt the Wolf Pack") {
        "clothing"
    } else if title.starts_with("Quiet the Restless Dead") {
        "religion"
    } else {
        "inn"
    }
}

fn ensure_npc_quest_parties(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let target = 1 + settlement_id.bytes().map(usize::from).sum::<usize>() % 2;
    for mut party in ctx.db.party().iter().collect::<Vec<_>>() {
        let Some(quest_id) = party.active_quest_id.as_ref() else {
            continue;
        };
        let Some(quest) = ctx.db.quest().id().find(quest_id) else {
            continue;
        };
        let Some(leader) = ctx.db.character().id().find(party.leader_id) else {
            continue;
        };
        if !leader.temporary || quest.settlement_id != settlement_id {
            continue;
        }
        party.current_settlement_id = Some(settlement_id.to_string());
        if party.name.ends_with("'s party") {
            party.name = format!("{}'s company", leader.name);
        }
        if party.medicine_target == 0.0
            && party.surgery_target == 0.0
            && party.charisma_target == 0.0
            && party.faith_target == 0.0
        {
            party.medicine_target = 4.0;
            party.surgery_target = 4.0;
            party.charisma_target = 5.0;
            party.faith_target = 4.0;
        }
        party.medicine_target = party.medicine_target.round().clamp(0.0, 5.0);
        party.surgery_target = party.surgery_target.round().clamp(0.0, 5.0);
        party.charisma_target = party.charisma_target.round().clamp(0.0, 5.0);
        party.faith_target = party.faith_target.round().clamp(0.0, 5.0);
        ctx.db.party().id().update(party);
    }
    let existing = ctx
        .db
        .party()
        .iter()
        .filter(|party| party.current_settlement_id.as_deref() == Some(settlement_id))
        .filter(|party| party.active_quest_id.is_some())
        .filter(|party| {
            ctx.db
                .character()
                .id()
                .find(party.leader_id)
                .is_some_and(|leader| leader.temporary)
        })
        .count();
    for _ in existing..target {
        let Some(mut quest) = ctx
            .db
            .quest()
            .settlement_id()
            .filter(&settlement_id.to_string())
            .find(|quest| quest.status == QuestStatus::Available)
        else {
            break;
        };
        use petname::Generator;
        let leader_name = petname::Petnames::default()
            .generate(&mut ctx.rng(), 2, " ")
            .unwrap_or_else(|| "quest captain".into());
        let mut leader_id = ctx.random::<u64>() | (1_u64 << 63);
        while ctx.db.character().id().find(leader_id).is_some() {
            leader_id = ctx.random::<u64>() | (1_u64 << 63);
        }
        crate::character::insert_new_character(ctx, leader_name.clone(), leader_id, true)?;
        let mut leader = ctx.db.character().id().find(leader_id).unwrap();
        leader.current_settlement_id = Some(settlement_id.to_string());
        ctx.db.character().id().update(leader.clone());
        let party_id = leader.party_id.clone().ok_or("NPC leader has no party")?;
        let mut party = ctx.db.party().id().find(&party_id).unwrap();
        party.name = format!("{}'s company", leader_name);
        party.current_settlement_id = Some(settlement_id.to_string());
        party.active_quest_id = Some(quest.id.clone());
        party.medicine_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.surgery_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.charisma_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        party.faith_target = 3.0 + (ctx.random::<u64>() % 3) as f32;
        ctx.db.party().id().update(party);

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
        quest.status = QuestStatus::Accepted;
        quest.accepted_by = Some(party_id);
        ctx.db.quest().id().update(quest);
    }
    Ok(())
}

fn generate_quest_for_settlement(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let Some(settlement) = ctx.db.settlement().id().find(&settlement_id.to_string()) else {
        return Err("Settlement not found".into());
    };
    let archetypes = [
        (
            "Clear the Goblin Cave",
            "Goblins have been attacking travelers on the road after dark.",
            "goblins",
            "cave",
            "You arrive at a cave.",
            2,
            "inn",
        ),
        (
            "Break Up the Bandit Camp",
            "Bandits have been raiding merchant caravans.",
            "bandits",
            "camp",
            "You arrive at a rough camp.",
            3,
            "merchants",
        ),
        (
            "Hunt the Wolf Pack",
            "Wolves have been attacking the flocks that supply wool and hides.",
            "wolves",
            "woods",
            "You arrive at a wooded hollow.",
            1,
            "clothing",
        ),
        (
            "Purge the Old Mine",
            "Giant spiders have cut off the armourer's supply of ore.",
            "spiders",
            "mine",
            "You arrive at an old mine.",
            3,
            "armor",
        ),
        (
            "Recover the Stolen Arms",
            "Thieves are hiding with a stolen shipment of weapons.",
            "thieves",
            "camp",
            "You arrive at a hidden camp.",
            2,
            "weapons",
        ),
        (
            "Quiet the Restless Dead",
            "A necromancer has raised skeletons in a nearby crypt.",
            "skeletons",
            "ruins",
            "You arrive at ruined chapel.",
            4,
            "religion",
        ),
    ];
    let tracked_quests: HashSet<String> = ctx
        .db
        .party()
        .iter()
        .filter_map(|party| party.active_quest_id)
        .collect();
    let occupied: HashSet<String> = ctx
        .db
        .quest()
        .settlement_id()
        .filter(&settlement.id)
        .filter(|quest| {
            quest.status != QuestStatus::Completed || tracked_quests.contains(&quest.id)
        })
        .map(|quest| quest.title)
        .collect();
    let start = ctx.random::<u64>() as usize % archetypes.len();
    let Some((title, description, enemy, scene, arrival, difficulty, service_id)) = (0..archetypes
        .len())
        .map(|offset| archetypes[(start + offset) % archetypes.len()])
        .find(|archetype| !occupied.contains(&format!("{} near {}", archetype.0, settlement.name)))
    else {
        return Err("No distinct quest archetype is available".into());
    };
    let distance_m = 4_000 + ctx.random::<u64>() % 17_000;
    let angle = (ctx.random::<u64>() as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let geographic = settlement.source_node_id.is_some();
    let (offset_x, offset_y) = if geographic {
        let distance_km = distance_m as f64 / 1_000.0;
        let latitude_scale = 111.0;
        let longitude_scale = latitude_scale * settlement.coord_y.to_radians().cos().abs().max(0.1);
        (
            angle.cos() * distance_km / longitude_scale,
            angle.sin() * distance_km / latitude_scale,
        )
    } else {
        let distance_km = distance_m as f64 / 1_000.0;
        (angle.cos() * distance_km, angle.sin() * distance_km)
    };
    let enemy_count = difficulty * 2 + (ctx.random::<u64>() % 4) as i32;
    let nonce = ctx.random::<u64>();
    let quest_id = format!("{}-{nonce:016x}", settlement.id);
    ctx.db.quest().insert(Quest {
        id: quest_id.clone(),
        title: format!("{title} near {}", settlement.name),
        description: description.into(),
        difficulty,
        gold_reward: difficulty * 35 + distance_m.div_ceil(1_000) as i32 * 2,
        xp_reward: difficulty * 20,
        settlement_id: settlement.id.clone(),
        status: QuestStatus::Available,
        accepted_by: None,
        enemy_type: enemy.into(),
        enemy_count,
        location_description: arrival.into(),
        location_scene_key: scene.into(),
        location_coord_x: settlement.coord_x + offset_x,
        location_coord_y: settlement.coord_y + offset_y,
        coordinates_are_geographic: geographic,
        distance_m,
    });
    ctx.db.quest_issuer().insert(QuestIssuer {
        quest_id,
        settlement_id: settlement.id,
        service_id: service_id.into(),
    });
    Ok(())
}
