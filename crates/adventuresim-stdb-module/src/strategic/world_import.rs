pub use adventuresim_core::case::{CaseStatus, ContractStatus};

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinaleStatus {
    Available,
    Selected,
    Executed,
    Ineligible,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinaleExecutionKind {
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

fn discard_placeholder_settlement_data(ctx: &ReducerContext) -> Result<(), String> {
    for settlement_id in PLACEHOLDER_SETTLEMENT_IDS {
        for alias in ctx
            .db
            .settlement_alias()
            .settlement_id()
            .filter(settlement_id)
            .collect::<Vec<_>>()
        {
            ctx.db.settlement_alias().id().delete(&alias.id);
        }
        for description in ctx
            .db
            .settlement_description()
            .settlement_id()
            .filter(settlement_id)
            .collect::<Vec<_>>()
        {
            ctx.db.settlement_description().id().delete(&description.id);
        }
        for presence in ctx
            .db
            .settlement_resident_presence()
            .settlement_id()
            .filter(settlement_id)
            .collect::<Vec<_>>()
        {
            ctx.db
                .settlement_resident_presence()
                .character_id()
                .delete(presence.character_id);
        }
        for npc in ctx
            .db
            .settlement_resident_profile()
            .home_settlement_id()
            .filter(settlement_id)
            .collect::<Vec<_>>()
        {
            ctx.db
                .settlement_resident_seed_explanation()
                .character_id()
                .delete(npc.character_id);
            ctx.db
                .settlement_resident_profile()
                .character_id()
                .delete(npc.character_id);
            if let Some(character) = ctx.db.character().id().find(npc.character_id) {
                crate::character::delete_character_for_world_import(ctx, character)?;
            }
        }
        crate::social_roles::delete_unreferenced_settlement_social_organizations(
            ctx,
            settlement_id,
        );
        let settlement_id = settlement_id.to_string();
        ctx.db
            .settlement_smith()
            .settlement_id()
            .delete(&settlement_id);
        ctx.db.settlement().id().delete(&settlement_id);
    }

    ctx.db.travel_edge().id().delete(RENDERER_DEMO_EDGE);
    ctx.db
        .world_node()
        .id()
        .delete(RIVERDALE_RENDERER_DEMO_NODE);
    ctx.db
        .world_node()
        .id()
        .delete(IRONFORGE_RENDERER_DEMO_NODE);
    Ok(())
}

fn discard_character_data_for_world_import(ctx: &ReducerContext) -> Result<(), String> {
    for claim in ctx.db.starting_character_claim().iter().collect::<Vec<_>>() {
        ctx.db
            .starting_character_claim()
            .request_key()
            .delete(&claim.request_key);
    }
    for offer in ctx.db.recruitment_offer().iter().collect::<Vec<_>>() {
        ctx.db.recruitment_offer().id_key().delete(&offer.id_key);
    }
    for membership in ctx.db.party_member().iter().collect::<Vec<_>>() {
        ctx.db.party_member().id().delete(membership.id);
    }
    for party in ctx.db.party_authority().iter().collect::<Vec<_>>() {
        delete_temporary_character_party(ctx, party.leader_id, &party.id)?;
    }
    for character in ctx.db.character().iter().collect::<Vec<_>>() {
        crate::character::delete_character_for_world_import(ctx, character)?;
    }
    Ok(())
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
            discard_placeholder_settlement_data(ctx)?;
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
    discard_character_data_for_world_import(ctx)?;
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
        if ctx.db.settlement().id().find(&row.id).is_some() {
            ctx.db.settlement().id().update(row);
        } else {
            ctx.db.settlement().insert(row);
        }
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
    aliases: Vec<SettlementAlias>,
) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if aliases.is_empty() {
        return Err("Settlement-alias batch is empty".into());
    }
    for mut alias in aliases {
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
        alias.language = alias
            .language
            .take()
            .map(|value| {
                value
                    .parse::<LanguageCode>()
                    .map(String::from)
                    .map_err(|error| format!("Settlement alias {}: {error}", alias.id))
            })
            .transpose()?;
        if ctx.db.settlement_alias().id().find(&alias.id).is_some() {
            ctx.db.settlement_alias().id().update(alias);
        } else {
            ctx.db.settlement_alias().insert(alias);
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
    descriptions: Vec<SettlementDescription>,
) -> Result<(), String> {
    require_active_world_import(ctx)?;
    if descriptions.is_empty() {
        return Err("Settlement-description batch is empty".into());
    }
    for mut description in descriptions {
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
        description.language = description
            .language
            .take()
            .map(|value| {
                value
                    .parse::<LanguageCode>()
                    .map(String::from)
                    .map_err(|error| format!("Settlement description {}: {error}", description.id))
            })
            .transpose()?;
        if ctx
            .db
            .settlement_description()
            .id()
            .find(&description.id)
            .is_some()
        {
            ctx.db.settlement_description().id().update(description);
        } else {
            ctx.db.settlement_description().insert(description);
        }
    }
    Ok(())
}
