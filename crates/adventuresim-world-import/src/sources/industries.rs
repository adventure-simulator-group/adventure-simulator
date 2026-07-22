//! Deterministic rules-v6 strategic production inference.

use std::collections::HashMap;

#[cfg(test)]
use adventuresim_world_schema::{
    AgriculturalCommodity as Ag, AgricultureIndustry, ConstructionCommodity as Construction,
    ConstructionIndustry, DerivedHistoricalVegetationCover as DerivedCover,
    FallbackHistoricalVegetationCover as FallbackCover, FallbackIndustry, FishCommodity as Fish,
    FishingIndustry, ForestCommodity as Forest, ForestryIndustry, HistoricalVegetation,
    IgneousRock, MarineWaterAccess, MetamorphicRock, MiningIndustry, PotteryCommodity as Pottery,
    PotteryIndustry, QuarryCommodity as Quarry, SaltSource, SaltmakingIndustry, SedimentaryRock,
    SurfaceGeology, SurfaceLithology,
};
use adventuresim_world_schema::{
    CompiledWorld, DerivedIndustry as Industry, IndustryEvidence, InferredIndustryProfile,
    ProductionScale as Scale, RouteTerrainClass,
};

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RouteAccessibility {
    Isolated,
    Difficult,
    Connected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SettlementRouteContext {
    pub route_count: u16,
    pub accessibility: RouteAccessibility,
    pub best_terrain: RouteTerrainClass,
    pub worst_terrain: RouteTerrainClass,
    pub max_slope_permille: u16,
    pub max_roughness_m: u16,
    pub max_relief_m: u16,
}

impl SettlementRouteContext {
    fn isolated() -> Self {
        Self {
            route_count: 0,
            accessibility: RouteAccessibility::Isolated,
            best_terrain: RouteTerrainClass::Mountainous,
            worst_terrain: RouteTerrainClass::Mountainous,
            max_slope_permille: 0,
            max_roughness_m: 0,
            max_relief_m: 0,
        }
    }
}

pub(crate) fn enrich(mut world: CompiledWorld) -> Result<CompiledWorld> {
    let contexts = route_contexts(&world);
    let mut counters = IndustryCounters::default();
    for settlement in &mut world.settlements {
        let context = contexts
            .get(&settlement.source_node_id)
            .copied()
            .unwrap_or_else(SettlementRouteContext::isolated);
        let profile = infer(settlement, context)?;
        counters.observe(&profile);
        append_note(
            settlement,
            &format!(
                "**Industry inference v6:** {} canonical production output(s) from HYDE 3.5 historical land use, finalized SoilGrids/EGDI evidence, OWDA moisture, EU-Hydro access, historical woodland, population, and {} incident finalized route(s); accessibility can only downgrade scale, never create a resource.",
                profile.outputs().len(),
                context.route_count
            ),
        )?;
        settlement.industries = profile;
    }
    counters.write(&mut world.report);
    Ok(world)
}

pub(crate) fn validate_semantics(world: &CompiledWorld) -> Result<()> {
    let contexts = route_contexts(world);
    for s in &world.settlements {
        let context = contexts
            .get(&s.source_node_id)
            .copied()
            .unwrap_or_else(SettlementRouteContext::isolated);
        let max_scale = match context.accessibility {
            RouteAccessibility::Connected => Scale::Regional,
            RouteAccessibility::Difficult => Scale::Local,
            RouteAccessibility::Isolated => Scale::Marginal,
        };
        if !adventuresim_world_schema::industry_profile_is_canonical(
            &s.industries,
            inference_context(s, max_scale),
        ) {
            return Err(Error::Validation(format!(
                "settlement {} has an industry output unsupported by canonical evidence",
                s.id
            )));
        }
    }
    Ok(())
}

fn route_contexts(world: &CompiledWorld) -> HashMap<u64, SettlementRouteContext> {
    let mut grouped: HashMap<u64, Vec<&adventuresim_world_schema::TravelEdgeImport>> =
        HashMap::new();
    for edge in &world.edges {
        grouped.entry(edge.from_node_id).or_default().push(edge);
        grouped.entry(edge.to_node_id).or_default().push(edge);
    }
    grouped
        .into_iter()
        .map(|(node, edges)| {
            let route_count = u16::try_from(edges.len()).unwrap_or(u16::MAX);
            let best = edges
                .iter()
                .map(|e| e.terrain.class)
                .min()
                .unwrap_or(RouteTerrainClass::Mountainous);
            let worst = edges
                .iter()
                .map(|e| e.terrain.class)
                .max()
                .unwrap_or(RouteTerrainClass::Mountainous);
            let max_slope = edges
                .iter()
                .map(|e| e.terrain.max_slope.get())
                .max()
                .unwrap_or(0);
            let roughness = edges
                .iter()
                .map(|e| e.terrain.roughness.get())
                .max()
                .unwrap_or(0);
            let relief = edges
                .iter()
                .map(|e| e.terrain.relief.get())
                .max()
                .unwrap_or(0);
            let accessibility =
                if route_count >= 2 && best <= RouteTerrainClass::Rolling && max_slope <= 250 {
                    RouteAccessibility::Connected
                } else if route_count > 0 {
                    RouteAccessibility::Difficult
                } else {
                    RouteAccessibility::Isolated
                };
            (
                node,
                SettlementRouteContext {
                    route_count,
                    accessibility,
                    best_terrain: best,
                    worst_terrain: worst,
                    max_slope_permille: max_slope,
                    max_roughness_m: roughness,
                    max_relief_m: relief,
                },
            )
        })
        .collect()
}

fn infer(
    s: &adventuresim_world_schema::SettlementImport,
    route: SettlementRouteContext,
) -> Result<InferredIndustryProfile> {
    let max_scale = match route.accessibility {
        RouteAccessibility::Connected => Scale::Regional,
        RouteAccessibility::Difficult => Scale::Local,
        RouteAccessibility::Isolated => Scale::Marginal,
    };
    adventuresim_world_schema::infer_industries(inference_context(s, max_scale))
        .map_err(Error::Validation)
}

fn inference_context<'a>(
    s: &'a adventuresim_world_schema::SettlementImport,
    max_scale: Scale,
) -> adventuresim_world_schema::IndustryInferenceContext<'a> {
    adventuresim_world_schema::IndustryInferenceContext {
        elevation: s.elevation,
        drought: s.drought,
        land_use: s.land_use,
        historical_vegetation: s.historical_vegetation,
        soil: s.soil,
        geology: &s.geology,
        hydrology: s.hydrology,
        population_estimate: s.population_estimate,
        max_scale,
    }
}

fn append_note(s: &mut adventuresim_world_schema::SettlementImport, note: &str) -> Result<()> {
    let addition = format!("\n- {note}");
    if s.sources
        .chars()
        .count()
        .checked_add(addition.chars().count())
        .is_none_or(|v| v > adventuresim_world_schema::MAX_SOURCES_MARKDOWN_CHARS)
    {
        return Err(Error::Validation(format!(
            "settlement {} has no room for required industry provenance",
            s.id
        )));
    }
    s.sources.push_str(&addition);
    Ok(())
}

#[derive(Default)]
struct IndustryCounters {
    settlements: usize,
    derived: usize,
    fallback_settlements: usize,
    fallback: usize,
    categories: [usize; 10],
}
impl IndustryCounters {
    fn observe(&mut self, p: &InferredIndustryProfile) {
        self.settlements += 1;
        let mut had_fallback = false;
        for v in p.outputs() {
            match v {
                IndustryEvidence::Fallback(_) => {
                    self.fallback += 1;
                    had_fallback = true;
                }
                IndustryEvidence::Derived(d) => {
                    self.derived += 1;
                    self.categories[match d {
                        Industry::Agriculture(_) => 0,
                        Industry::Fishing(_) => 1,
                        Industry::Quarrying(_) => 2,
                        Industry::Mining(_) => 3,
                        Industry::Pottery(_) => 4,
                        Industry::PeatCutting(_) => 5,
                        Industry::Forestry(_) => 6,
                        Industry::CharcoalBurning(_) => 7,
                        Industry::Saltmaking(_) => 8,
                        Industry::Construction(_) => 9,
                    }] += 1;
                }
            }
        }
        if had_fallback {
            self.fallback_settlements += 1;
        }
    }
    fn write(self, r: &mut adventuresim_world_schema::WorldBuildReport) {
        r.industry_settlements = self.settlements;
        r.industry_derived_outputs = self.derived;
        r.industry_fallback_settlements = self.fallback_settlements;
        r.industry_fallback_outputs = self.fallback;
        r.industry_agriculture_outputs = self.categories[0];
        r.industry_fishing_outputs = self.categories[1];
        r.industry_quarrying_outputs = self.categories[2];
        r.industry_mining_outputs = self.categories[3];
        r.industry_pottery_outputs = self.categories[4];
        r.industry_peat_outputs = self.categories[5];
        r.industry_forestry_outputs = self.categories[6];
        r.industry_charcoal_outputs = self.categories[7];
        r.industry_saltmaking_outputs = self.categories[8];
        r.industry_construction_outputs = self.categories[9];
    }
}

#[cfg(test)]
fn scaled(score: u16, route: SettlementRouteContext) -> Scale {
    let cap = match route.accessibility {
        RouteAccessibility::Connected => Scale::Regional,
        RouteAccessibility::Difficult => Scale::Local,
        RouteAccessibility::Isolated => Scale::Marginal,
    };
    (if score >= 7_000 {
        Scale::Regional
    } else if score >= 4_000 {
        Scale::Local
    } else {
        Scale::Marginal
    })
    .min(cap)
}
#[cfg(test)]
fn lithology(g: SurfaceGeology) -> SurfaceLithology {
    match g {
        SurfaceGeology::Mapped(v) => match v.setting.lithology {
            adventuresim_world_schema::GeologicLithologyEvidence::Mapped(l)
            | adventuresim_world_schema::GeologicLithologyEvidence::Inferred(l) => l,
        },
        SurfaceGeology::Inferred(v) => v.lithology,
    }
}
#[cfg(test)]
fn quarry(l: SurfaceLithology) -> Option<Quarry> {
    match l {
        SurfaceLithology::Sedimentary(SedimentaryRock::Limestone | SedimentaryRock::Dolostone) => {
            Some(Quarry::Limestone)
        }
        SurfaceLithology::Sedimentary(SedimentaryRock::Chalk) => Some(Quarry::Chalk),
        SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone) => Some(Quarry::Sandstone),
        SurfaceLithology::Metamorphic(MetamorphicRock::Slate) => Some(Quarry::Slate),
        SurfaceLithology::Igneous(IgneousRock::Granite | IgneousRock::Granitoid) => {
            Some(Quarry::Granite)
        }
        SurfaceLithology::Igneous(IgneousRock::Basalt) => Some(Quarry::Basalt),
        SurfaceLithology::Metamorphic(MetamorphicRock::Marble) => Some(Quarry::Marble),
        SurfaceLithology::Metamorphic(MetamorphicRock::Quartzite) => Some(Quarry::Quartzite),
        SurfaceLithology::Igneous(_)
        | SurfaceLithology::Metamorphic(_)
        | SurfaceLithology::Mixed(_) => Some(Quarry::OtherHardStone),
        _ => None,
    }
}
#[cfg(test)]
fn fallback(
    s: &adventuresim_world_schema::SettlementImport,
    _: SurfaceLithology,
) -> FallbackIndustry {
    if s.hydrology.has_freshwater() {
        FallbackIndustry::FreshwaterFishing
    } else if s.land_use.grazing().basis_points() > 0 {
        FallbackIndustry::GrazingDairy
    } else if s.land_use.cropland().basis_points() > 0 {
        FallbackIndustry::CroplandGrain
    } else if matches!(
        s.historical_vegetation,
        HistoricalVegetation::Derived(adventuresim_world_schema::DerivedHistoricalVegetation {
            cover: DerivedCover::Woodland(_),
            ..
        }) | HistoricalVegetation::Fallback(
            adventuresim_world_schema::FallbackHistoricalVegetation {
                cover: FallbackCover::Woodland(_),
                ..
            }
        )
    ) {
        FallbackIndustry::WoodlandFuelwood
    } else {
        FallbackIndustry::CommonAggregate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_world_schema::*;

    fn context(accessibility: RouteAccessibility) -> SettlementRouteContext {
        SettlementRouteContext {
            route_count: usize::from(accessibility != RouteAccessibility::Isolated) as u16,
            accessibility,
            best_terrain: RouteTerrainClass::Flat,
            worst_terrain: RouteTerrainClass::Flat,
            max_slope_permille: 0,
            max_roughness_m: 0,
            max_relief_m: 0,
        }
    }

    fn settlement() -> SettlementImport {
        SettlementImport {
            id: "test".into(),
            source_node_id: 1,
            name: "Test".into(),
            longitude: 10.0,
            latitude: 54.0,
            population_level: 3,
            population_estimate: 2_000,
            elevation: ElevationMeters::new(20).unwrap(),
            land_use: LandUseProfile::new(
                LandUseFraction::new(3_000).unwrap(),
                LandUseFraction::new(2_000).unwrap(),
                LandUseFraction::new(500).unwrap(),
                LandUseFraction::new(4_500).unwrap(),
            )
            .unwrap(),
            forest_cover: ForestCover::Wooded(Woodland {
                density: CanopyDensity::new(50).unwrap(),
                dominant: DominantLeafType::Mixed,
            }),
            potential_vegetation: PotentialVegetation::Inferred(
                PotentialVegetationClass::WoodlandAndForest,
            ),
            historical_vegetation: HistoricalVegetation::Derived(DerivedHistoricalVegetation {
                cover: DerivedHistoricalVegetationCover::Woodland(HistoricalWoodland {
                    canopy: CanopyDensity::new(50).unwrap(),
                    dominant: DominantLeafType::Mixed,
                }),
                method: DerivedHistoricalVegetationMethod::MultiSourceRulesV4,
            }),
            tree_species: TreeSpeciesProfile::Inferred(
                InferredTreeSpeciesProfile::new(vec![TreeSpeciesId::new("Quercus_robur").unwrap()])
                    .unwrap(),
            ),
            soil: SoilProfile {
                wrb_group: WrbReferenceGroup::Cambisol,
                parent_material: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                properties: SoilProperties {
                    substrate: SoilSubstrate::Mineral(MineralSoil {
                        texture: MineralSoilTexture::Medium,
                        depth: SoilDepth::Deep,
                        available_water: AvailableWaterCapacity::High,
                        organic_carbon: TopsoilOrganicCarbon::Medium,
                        stones: StoneContentPercent::new(5).unwrap(),
                    }),
                    water_regime: SoilWaterRegime::SeasonallyWet,
                    agricultural_limitation: AgriculturalLimitation::None,
                },
                acidity: SoilAcidity::Neutral,
                cation_exchange_capacity: CationExchangeCapacity::High,
                fertility: SoilFertility::High,
                confidence: SoilBasisPoints::new(8_000).unwrap(),
                evidence: SoilEvidence::SoilGridsPrediction,
            },
            geology: SurfaceGeology::Inferred(InferredGeologicSetting {
                lithology: SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
                age: GeologicEra::Quaternary,
            }),
            religious_status: SettlementReligiousStatus::Established {
                religion: OfficialReligion::Lutheran,
            },
            languages: adventuresim_world_schema::infer_settlement_language_profile(10.0, 51.0)
                .unwrap(),
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
            industries: InferredIndustryProfile::new(vec![IndustryEvidence::Fallback(
                FallbackIndustry::CommonAggregate,
            )])
            .unwrap(),
            scene_key: "village".into(),
            sources: "- test".into(),
        }
    }

    #[test]
    fn every_supported_quarry_lithology_maps_exactly() {
        let cases = [
            (
                SurfaceLithology::Sedimentary(SedimentaryRock::Limestone),
                Quarry::Limestone,
            ),
            (
                SurfaceLithology::Sedimentary(SedimentaryRock::Dolostone),
                Quarry::Limestone,
            ),
            (
                SurfaceLithology::Sedimentary(SedimentaryRock::Chalk),
                Quarry::Chalk,
            ),
            (
                SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
                Quarry::Sandstone,
            ),
            (
                SurfaceLithology::Metamorphic(MetamorphicRock::Slate),
                Quarry::Slate,
            ),
            (
                SurfaceLithology::Igneous(IgneousRock::Granite),
                Quarry::Granite,
            ),
            (
                SurfaceLithology::Igneous(IgneousRock::Basalt),
                Quarry::Basalt,
            ),
            (
                SurfaceLithology::Metamorphic(MetamorphicRock::Marble),
                Quarry::Marble,
            ),
            (
                SurfaceLithology::Metamorphic(MetamorphicRock::Quartzite),
                Quarry::Quartzite,
            ),
            (
                SurfaceLithology::Igneous(IgneousRock::Gabbro),
                Quarry::OtherHardStone,
            ),
        ];
        for (lithology, expected) in cases {
            assert_eq!(quarry(lithology), Some(expected));
        }
        assert_eq!(
            quarry(SurfaceLithology::Sedimentary(SedimentaryRock::Coal)),
            None
        );
    }

    #[test]
    fn route_accessibility_only_downgrades_scale() {
        assert_eq!(
            scaled(9_000, context(RouteAccessibility::Connected)),
            Scale::Regional
        );
        assert_eq!(
            scaled(9_000, context(RouteAccessibility::Difficult)),
            Scale::Local
        );
        assert_eq!(
            scaled(9_000, context(RouteAccessibility::Isolated)),
            Scale::Marginal
        );
        let mut s = settlement();
        s.geology = SurfaceGeology::Inferred(InferredGeologicSetting {
            lithology: SurfaceLithology::Sedimentary(SedimentaryRock::Coal),
            age: GeologicEra::Carboniferous,
        });
        let isolated = infer(&s, context(RouteAccessibility::Isolated)).unwrap();
        assert!(isolated.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Mining(MiningIndustry {
                commodity: MinedCommodity::Coal,
                scale: Scale::Marginal
            }))
        )));
    }

    #[test]
    fn crystalline_rock_never_invents_metal_mining() {
        let mut s = settlement();
        s.geology = SurfaceGeology::Inferred(InferredGeologicSetting {
            lithology: SurfaceLithology::Igneous(IgneousRock::Granite),
            age: GeologicEra::Precambrian,
        });
        let p = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(
            !p.outputs()
                .iter()
                .any(|v| matches!(v, IndustryEvidence::Derived(Industry::Mining(_))))
        );
    }

    #[test]
    fn fishing_variants_follow_only_hydrology() {
        let mut s = settlement();
        s.hydrology.marine = Some(MarineWaterAccess::Tidal(
            WaterDistanceMeters::new(100).unwrap(),
        ));
        let tidal = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(tidal.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Fishing(FishingIndustry {
                commodity: Fish::Estuarine,
                ..
            }))
        )));
        s.hydrology.marine = Some(MarineWaterAccess::OpenCoast(
            WaterDistanceMeters::new(100).unwrap(),
        ));
        let marine = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(marine.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Fishing(FishingIndustry {
                commodity: Fish::Marine,
                ..
            }))
        )));
    }

    #[test]
    fn agriculture_rejects_drought_and_flood_boundaries() {
        let mut s = settlement();
        s.drought = DroughtProfile::Inferred(
            DroughtHistory::new(
                PalmerDroughtSeverityIndex::new(-2_000).unwrap(),
                PalmerDroughtSeverityIndex::new(-2_000).unwrap(),
                20,
                0,
            )
            .unwrap(),
        );
        let dry = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(!dry.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Agriculture(AgricultureIndustry {
                commodity: Ag::Grain | Ag::Flax,
                ..
            }))
        )));
        s.drought = settlement().drought;
        s.soil.properties.agricultural_limitation = AgriculturalLimitation::Flooded;
        let flooded = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(!flooded.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Agriculture(AgricultureIndustry {
                commodity: Ag::Grain | Ag::Flax,
                ..
            }))
        )));
    }

    #[test]
    fn peat_and_coastal_salt_require_convergent_inputs() {
        let mut s = settlement();
        s.soil.wrb_group = WrbReferenceGroup::Histosol;
        s.soil.properties.water_regime = SoilWaterRegime::PermanentlyWet;
        let no_water = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(
            !no_water
                .outputs()
                .iter()
                .any(|v| matches!(v, IndustryEvidence::Derived(Industry::PeatCutting(_))))
        );
        s.hydrology.inland = Some(InlandWaterAccess {
            distance: WaterDistanceMeters::new(100).unwrap(),
            size: InlandWaterSize::Lake,
        });
        s.hydrology.marine = Some(MarineWaterAccess::OpenCoast(
            WaterDistanceMeters::new(100).unwrap(),
        ));
        let converged = infer(&s, context(RouteAccessibility::Connected)).unwrap();
        assert!(
            converged
                .outputs()
                .iter()
                .any(|v| matches!(v, IndustryEvidence::Derived(Industry::PeatCutting(_))))
        );
        assert!(converged.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Saltmaking(SaltmakingIndustry {
                source: SaltSource::CoastalBrine,
                ..
            }))
        )));
    }

    #[test]
    fn fallback_precedence_is_stable() {
        let mut s = settlement();
        assert_eq!(
            fallback(&s, lithology(s.geology.clone())),
            FallbackIndustry::GrazingDairy
        );
        s.hydrology.inland = Some(InlandWaterAccess {
            distance: WaterDistanceMeters::new(100).unwrap(),
            size: InlandWaterSize::Pond,
        });
        assert_eq!(
            fallback(&s, lithology(s.geology.clone())),
            FallbackIndustry::FreshwaterFishing
        );
    }

    fn exact(s: &SettlementImport, profile: &InferredIndustryProfile) -> bool {
        adventuresim_world_schema::industry_profile_is_canonical(
            profile,
            inference_context(s, ProductionScale::Regional),
        )
    }

    fn replace_output(
        profile: &InferredIndustryProfile,
        predicate: impl Fn(&IndustryEvidence) -> bool,
        replacement: IndustryEvidence,
    ) -> InferredIndustryProfile {
        let mut outputs = profile.outputs().to_vec();
        let value = outputs.iter_mut().find(|v| predicate(v)).unwrap();
        *value = replacement;
        InferredIndustryProfile::new(outputs).unwrap()
    }

    #[test]
    fn exact_boundary_rejects_rules_v6_profiles_the_canonical_builder_did_not_emit() {
        let connected = context(RouteAccessibility::Connected);

        let mut dry = settlement();
        dry.drought = DroughtProfile::Inferred(
            DroughtHistory::new(
                PalmerDroughtSeverityIndex::new(-2_000).unwrap(),
                PalmerDroughtSeverityIndex::new(-2_000).unwrap(),
                20,
                0,
            )
            .unwrap(),
        );
        let mut outputs = infer(&dry, connected).unwrap().outputs().to_vec();
        outputs.push(IndustryEvidence::Derived(Industry::Agriculture(
            AgricultureIndustry {
                commodity: Ag::Flax,
                scale: Scale::Marginal,
            },
        )));
        assert!(!exact(
            &dry,
            &InferredIndustryProfile::new(outputs).unwrap()
        ));

        let mut forest = settlement();
        forest.historical_vegetation = HistoricalVegetation::Derived(DerivedHistoricalVegetation {
            cover: DerivedHistoricalVegetationCover::Woodland(HistoricalWoodland {
                canopy: CanopyDensity::new(50).unwrap(),
                dominant: DominantLeafType::Coniferous,
            }),
            method: DerivedHistoricalVegetationMethod::MultiSourceRulesV4,
        });
        let canonical = infer(&forest, connected).unwrap();
        let wrong_leaf = replace_output(
            &canonical,
            |v| {
                matches!(
                    v,
                    IndustryEvidence::Derived(Industry::Forestry(ForestryIndustry {
                        commodity: Forest::Softwood,
                        ..
                    }))
                )
            },
            IndustryEvidence::Derived(Industry::Forestry(ForestryIndustry {
                commodity: Forest::Hardwood,
                scale: Scale::Local,
            })),
        );
        assert!(!exact(&forest, &wrong_leaf));

        let mut clay = settlement();
        clay.population_estimate = 0;
        clay.land_use = LandUseProfile::new(
            LandUseFraction::new(0).unwrap(),
            LandUseFraction::new(0).unwrap(),
            LandUseFraction::new(499).unwrap(),
            LandUseFraction::new(9_501).unwrap(),
        )
        .unwrap();
        clay.historical_vegetation = HistoricalVegetation::Derived(DerivedHistoricalVegetation {
            cover: DerivedHistoricalVegetationCover::Grassland,
            method: DerivedHistoricalVegetationMethod::MultiSourceRulesV4,
        });
        let canonical = infer(&clay, connected).unwrap();
        assert!(canonical.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Pottery(PotteryIndustry {
                commodity: Pottery::Clay,
                ..
            }))
        )));
        let wrong_pottery = replace_output(
            &canonical,
            |v| matches!(v, IndustryEvidence::Derived(Industry::Pottery(_))),
            IndustryEvidence::Derived(Industry::Pottery(PotteryIndustry {
                commodity: Pottery::Earthenware,
                scale: Scale::Marginal,
            })),
        );
        assert!(!exact(&clay, &wrong_pottery));
        let mut with_brick = canonical.outputs().to_vec();
        with_brick.push(IndustryEvidence::Derived(Industry::Construction(
            ConstructionIndustry {
                commodity: Construction::Brick,
                scale: Scale::Local,
            },
        )));
        assert!(!exact(
            &clay,
            &InferredIndustryProfile::new(with_brick).unwrap()
        ));
        clay.land_use = LandUseProfile::new(
            LandUseFraction::new(0).unwrap(),
            LandUseFraction::new(0).unwrap(),
            LandUseFraction::new(999).unwrap(),
            LandUseFraction::new(9_001).unwrap(),
        )
        .unwrap();
        let mut with_tile = infer(&clay, connected).unwrap().outputs().to_vec();
        with_tile.push(IndustryEvidence::Derived(Industry::Construction(
            ConstructionIndustry {
                commodity: Construction::RoofTile,
                scale: Scale::Local,
            },
        )));
        assert!(!exact(
            &clay,
            &InferredIndustryProfile::new(with_tile).unwrap()
        ));

        let mut salty = settlement();
        salty.geology = SurfaceGeology::Inferred(InferredGeologicSetting {
            lithology: SurfaceLithology::Sedimentary(SedimentaryRock::Evaporite),
            age: GeologicEra::Permian,
        });
        salty.soil.properties.agricultural_limitation = AgriculturalLimitation::Saline;
        salty.hydrology.marine = Some(MarineWaterAccess::OpenCoast(
            WaterDistanceMeters::new(100).unwrap(),
        ));
        let canonical = infer(&salty, connected).unwrap();
        let salts = canonical
            .outputs()
            .iter()
            .filter_map(|v| match v {
                IndustryEvidence::Derived(Industry::Saltmaking(v)) => Some(v.source),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(salts, vec![SaltSource::Evaporite]);
        let wrong_salt = replace_output(
            &canonical,
            |v| {
                matches!(
                    v,
                    IndustryEvidence::Derived(Industry::Saltmaking(SaltmakingIndustry {
                        source: SaltSource::Evaporite,
                        ..
                    }))
                )
            },
            IndustryEvidence::Derived(Industry::Saltmaking(SaltmakingIndustry {
                source: SaltSource::SalineSoil,
                scale: Scale::Marginal,
            })),
        );
        assert!(!exact(&salty, &wrong_salt));
        salty.geology = SurfaceGeology::Inferred(InferredGeologicSetting {
            lithology: SurfaceLithology::Sedimentary(SedimentaryRock::Siltstone),
            age: GeologicEra::Quaternary,
        });
        let saline = infer(&salty, connected).unwrap();
        assert!(saline.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Saltmaking(SaltmakingIndustry {
                source: SaltSource::SalineSoil,
                ..
            }))
        )));
        assert!(!saline.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Saltmaking(SaltmakingIndustry {
                source: SaltSource::CoastalBrine,
                ..
            }))
        )));
    }

    #[test]
    fn exact_boundary_rejects_distance_and_scale_inflation() {
        let connected = context(RouteAccessibility::Connected);
        let mut s = settlement();
        s.population_estimate = 0;
        s.land_use = LandUseProfile::new(
            LandUseFraction::new(1_500).unwrap(),
            LandUseFraction::new(0).unwrap(),
            LandUseFraction::new(0).unwrap(),
            LandUseFraction::new(8_500).unwrap(),
        )
        .unwrap();
        s.historical_vegetation = HistoricalVegetation::Derived(DerivedHistoricalVegetation {
            cover: DerivedHistoricalVegetationCover::Grassland,
            method: DerivedHistoricalVegetationMethod::MultiSourceRulesV4,
        });
        s.geology = SurfaceGeology::Inferred(InferredGeologicSetting {
            lithology: SurfaceLithology::Sedimentary(SedimentaryRock::Siltstone),
            age: GeologicEra::Quaternary,
        });
        let canonical = infer(&s, connected).unwrap();
        let inflated = replace_output(
            &canonical,
            |v| {
                matches!(
                    v,
                    IndustryEvidence::Derived(Industry::Agriculture(AgricultureIndustry {
                        commodity: Ag::Grain,
                        ..
                    }))
                )
            },
            IndustryEvidence::Derived(Industry::Agriculture(AgricultureIndustry {
                commodity: Ag::Grain,
                scale: Scale::Regional,
            })),
        );
        assert!(!exact(&s, &inflated));

        s.hydrology.flowing = Some(FlowingWaterAccess::River(RiverAccess {
            distance: WaterDistanceMeters::new(6_000).unwrap(),
            order: StrahlerOrder::new(6).unwrap(),
            persistence: FlowPersistence::Perennial,
        }));
        s.hydrology.inland = Some(InlandWaterAccess {
            distance: WaterDistanceMeters::new(100).unwrap(),
            size: InlandWaterSize::Lake,
        });
        let nearby_inland = infer(&s, connected).unwrap();
        assert!(nearby_inland.outputs().iter().any(|v| matches!(
            v,
            IndustryEvidence::Derived(Industry::Fishing(FishingIndustry {
                commodity: Fish::Freshwater,
                ..
            }))
        )));
        s.hydrology.inland = None;
        let distant_only = infer(&s, connected).unwrap();
        assert!(
            !distant_only
                .outputs()
                .iter()
                .any(|v| matches!(v, IndustryEvidence::Derived(Industry::Fishing(_))))
        );
    }
}
