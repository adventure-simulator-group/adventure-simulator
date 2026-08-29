//! Versioned post-hydrology reconstruction of dominant 1544 surface cover.

use adventuresim_world_schema::{
    AgriculturalLimitation, BuiltSettlementCover, CURRENT_INFERENCE_RULES_VERSION, CanopyDensity,
    CompiledWorld, CroplandCover, DerivedHistoricalVegetation, DerivedHistoricalVegetationCover,
    DerivedHistoricalVegetationMethod, DirectHistoricalVegetation, DirectHistoricalVegetationCover,
    DirectHistoricalVegetationMethod, DominantLeafType, DroughtProfile,
    FallbackHistoricalVegetation, FallbackHistoricalVegetationCover,
    FallbackHistoricalVegetationMethod, ForestCover, HistoricalVegetation, HistoricalWetland,
    HistoricalWoodland, MarineWaterAccess, PastureCover, PotentialVegetation,
    PotentialVegetationClass, SoilAcidity, SoilEvidence, SoilFertility, SoilSubstrate,
    SurfaceGeology, SurfaceLithology, TopsoilOrganicCarbon, TreeSpeciesProfile,
    WORLD_SCHEMA_VERSION, WorldMetadata, wetland_context_is_convergent,
};

use crate::{
    Result,
    draft::{
        FinalizedSoilSettlementDraft, FinalizedSoilWorldDraft, LandUseEvidence, push_source_note,
    },
};

const CLOSE_MARGIN: i32 = 250;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NaturalCover {
    Woodland,
    Heath,
    Grass,
    Sparse,
    Wetland,
    TransitionalWater,
}

pub(crate) fn finalize(mut draft: FinalizedSoilWorldDraft) -> Result<CompiledWorld> {
    crate::manifest::canonicalize(&mut draft.sources)?;
    let manifest_digest = crate::manifest::digest(draft.year, draft.spatial_grid, &draft.sources)?;
    let mut direct = 0;
    let mut derived = 0;
    let mut fallback = 0;
    let mut tie_breaks = 0;
    let settlements = draft
        .settlements
        .into_iter()
        .map(|mut finalized| {
            let (historical_vegetation, tied) = synthesize(&finalized);
            match historical_vegetation {
                HistoricalVegetation::Direct(_) => direct += 1,
                HistoricalVegetation::Derived(_) => derived += 1,
                HistoricalVegetation::Fallback(_) => fallback += 1,
            }
            tie_breaks += usize::from(tied);
            let method = match historical_vegetation {
                HistoricalVegetation::Direct(_) => "direct HYDE 3.5 dominant land use",
                HistoricalVegetation::Derived(DerivedHistoricalVegetation { method: DerivedHistoricalVegetationMethod::MultiSourceRulesV4TieBreak, .. }) => "derived multi-source rules v4 (coordinate/schema tie-break)",
                HistoricalVegetation::Derived(_) => "derived multi-source rules v4",
                HistoricalVegetation::Fallback(_) => "fallback potential envelope v4",
            };
            let land_evidence = match finalized.hydrologic.drought.religious.geologic.predicted.trees.vegetated.forest.land.evidence {
                LandUseEvidence::Hyde35Sampled { normalized: false } => "sampled HYDE 3.5 land-use fractions",
                LandUseEvidence::Hyde35Sampled { normalized: true } => "sampled and normalized HYDE 3.5 land-use fractions",
                LandUseEvidence::DeterministicFallback => "deterministic land-use fallback",
            };
            push_source_note(
                &mut finalized,
                &format!(
                    "**Historical vegetation synthesis v4:** Potential vegetation remains the modern-climate envelope; {land_evidence}, Copernicus canopy, EU-Trees4F candidates, finalized SoilGrids/EGDI soil, GLO-30 elevation, latitude, EU-Hydro context, and OWDA 1544/current-window moisture produce the separate 1544 cover by {method}."
                ),
            );

            let wet = finalized.hydrologic;
            let drought = wet.drought;
            let religious = drought.religious;
            let geologic = religious.geologic;
            let predicted = geologic.predicted;
            let trees = predicted.trees;
            let vegetated = trees.vegetated;
            let forest = vegetated.forest;
            let land = forest.land;
            let elevated = land.elevated;
            let settlement = elevated.settlement;
            adventuresim_world_schema::SettlementImport {
                id: settlement.id,
                source_node_id: settlement.source_node_id,
                name: settlement.name,
                longitude: settlement.longitude,
                latitude: settlement.latitude,
                population_level: settlement.population_level,
                population_estimate: settlement.population_estimate,
                elevation: elevated.elevation,
                land_use: land.land_use,
                forest_cover: forest.forest_cover,
                potential_vegetation: vegetated.potential_vegetation,
                historical_vegetation,
                tree_species: trees.tree_species,
                soil: finalized.soil,
                geology: geologic.geology,
                religious_status: religious.religious_status,
                languages: adventuresim_world_schema::infer_settlement_language_profile(
                    settlement.longitude,
                    settlement.latitude,
                ).expect("bounded imported settlement has a language profile"),
                drought: drought.drought,
                hydrology: wet.hydrology,
                industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                    adventuresim_world_schema::IndustryEvidence::Fallback(
                        adventuresim_world_schema::FallbackIndustry::CommonAggregate,
                    ),
                ]).expect("stage placeholder is valid"),
                economy: adventuresim_world_schema::SettlementEconomyProfile::stage_placeholder(),
                scene_key: settlement.scene_key,
                sources: settlement.sources,
            }
        })
        .collect();
    draft.report.historical_vegetation_direct_samples = direct;
    draft.report.historical_vegetation_derived_samples = derived;
    draft.report.historical_vegetation_fallback_samples = fallback;
    draft.report.historical_vegetation_tie_breaks = tie_breaks;
    Ok(CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            inference_rules_version: CURRENT_INFERENCE_RULES_VERSION,
            spatial_grid: draft.spatial_grid,
            world_year: draft.year,
            manifest_digest,
            sources: draft.sources,
            road_types: draft.road_types,
        },
        nodes: draft.nodes,
        edges: draft.edges,
        settlements,
        settlement_aliases: draft.settlement_aliases,
        settlement_descriptions: draft.settlement_descriptions,
        terrain_features: Vec::new(),
        report: draft.report,
    })
}

fn synthesize(s: &FinalizedSoilSettlementDraft) -> (HistoricalVegetation, bool) {
    let wet = &s.hydrologic;
    let drought = &wet.drought;
    let religious = &drought.religious;
    let geologic = &religious.geologic;
    let trees = &geologic.predicted.trees;
    let vegetated = &trees.vegetated;
    let forest = &vegetated.forest;
    let land = &forest.land;
    let base = &land.elevated.settlement;
    let profile = land.land_use;

    if let Some(cover) = direct_hyde35_cover(profile, land.evidence) {
        return (
            HistoricalVegetation::Direct(DirectHistoricalVegetation {
                cover,
                method: DirectHistoricalVegetationMethod::Hyde35DominantLandUse,
            }),
            false,
        );
    }

    let fallback = matches!(
        vegetated.potential_vegetation,
        PotentialVegetation::Inferred(_)
    ) && matches!(forest.forest_cover, ForestCover::Open)
        && s.soil.evidence == SoilEvidence::DeterministicInference
        && !wet.hydrology.has_freshwater()
        && !wet.hydrology.has_saltwater();
    if fallback {
        return (
            HistoricalVegetation::Fallback(FallbackHistoricalVegetation {
                cover: fallback_cover_for(
                    natural_from_potential(vegetated.potential_vegetation.class()),
                    s,
                ),
                method: FallbackHistoricalVegetationMethod::PotentialEnvelopeV4,
            }),
            false,
        );
    }

    let posterior = |class| posterior_score(&vegetated.potential_vegetation, class);
    let managed = i32::from(profile.cropland().basis_points())
        + i32::from(profile.grazing().basis_points())
        + i32::from(profile.built_up().basis_points());
    let canopy = match forest.forest_cover {
        ForestCover::Open => 0,
        ForestCover::Wooded(v) => i32::from(v.density.percent()) * 100,
    };
    let candidates = match &trees.tree_species {
        TreeSpeciesProfile::Modeled(v) => v.candidates().len(),
        TreeSpeciesProfile::Inferred(v) => v.species().len(),
    } as i32;
    let tidal = matches!(wet.hydrology.marine, Some(MarineWaterAccess::Tidal(_)));
    let mean_pdsi = match drought.drought {
        DroughtProfile::Reconstructed(h) | DroughtProfile::Inferred(h) => {
            h.twenty_year_mean().milli_units()
        }
    };
    let dry = mean_pdsi <= -1_000;
    let moist = mean_pdsi >= 1_000;
    let rocky = matches!(s.soil.properties.substrate, SoilSubstrate::RockOutcrop(_))
        || matches!(
            s.soil.properties.agricultural_limitation,
            AgriculturalLimitation::ShallowRock | AgriculturalLimitation::Stony
        );
    let acidic_poor = matches!(
        s.soil.acidity,
        SoilAcidity::StronglyAcid | SoilAcidity::Acid
    ) && matches!(
        s.soil.fertility,
        SoilFertility::VeryLow | SoilFertility::Low
    );
    let high = land.elevated.elevation.get() >= 900;
    let lithic = match &geologic.geology {
        SurfaceGeology::Mapped(v) => matches!(
            v.setting.lithology,
            adventuresim_world_schema::GeologicLithologyEvidence::Mapped(
                SurfaceLithology::Igneous(_) | SurfaceLithology::Metamorphic(_)
            ) | adventuresim_world_schema::GeologicLithologyEvidence::Inferred(
                SurfaceLithology::Igneous(_) | SurfaceLithology::Metamorphic(_)
            )
        ),
        SurfaceGeology::Inferred(v) => matches!(
            v.lithology,
            SurfaceLithology::Igneous(_) | SurfaceLithology::Metamorphic(_)
        ),
    };
    let organic = match s.soil.properties.substrate {
        SoilSubstrate::Mineral(v) => v.organic_carbon == TopsoilOrganicCarbon::High,
        SoilSubstrate::OtherNonTextured(v) => v.organic_carbon == TopsoilOrganicCarbon::High,
        SoilSubstrate::Organic(_) => true,
        SoilSubstrate::RockOutcrop(_) => false,
    };
    let latitude_adjust = if base.latitude.abs() >= 55.0 { 200 } else { 0 };

    let scores = [
        (
            NaturalCover::Woodland,
            posterior(PotentialVegetationClass::WoodlandAndForest)
                + canopy / 2
                + candidates * 80
                + i32::from(profile.natural().basis_points()) / 8
                - managed / 10,
        ),
        (
            NaturalCover::Heath,
            posterior(PotentialVegetationClass::HeathlandAndShrub)
                + if acidic_poor { 1_800 } else { 0 }
                + if dry { 300 } else { 0 },
        ),
        (
            NaturalCover::Grass,
            posterior(PotentialVegetationClass::Grassland)
                + i32::from(profile.grazing().basis_points()) / 3
                + if moist { 150 } else { 0 },
        ),
        (
            NaturalCover::Sparse,
            posterior(PotentialVegetationClass::SparselyVegetatedAreas)
                + if rocky { 1_500 } else { 0 }
                + if lithic { 500 } else { 0 }
                + if dry { 500 } else { 0 }
                + if high { 1_000 } else { 0 },
        ),
        // Wetland requires convergence: Jung plus soil/hydrology, never SOC alone.
        (
            NaturalCover::Wetland,
            if wetland_context_is_convergent(&vegetated.potential_vegetation, s.soil, wet.hydrology)
            {
                posterior(PotentialVegetationClass::Wetlands)
                    + 2_000
                    + if organic { 200 } else { 0 }
                    + if moist { 300 } else { 0 }
            } else {
                -10_000
            },
        ),
        // Transitional water is impossible without actual tidal/estuarine context.
        (
            NaturalCover::TransitionalWater,
            if tidal {
                posterior(PotentialVegetationClass::MarineInletsAndTransitionalWaters)
                    + 3_000
                    + latitude_adjust
            } else {
                -10_000
            },
        ),
    ];
    let (chosen, close) = choose_from_scores(scores, base.latitude, base.longitude);
    (
        HistoricalVegetation::Derived(DerivedHistoricalVegetation {
            cover: cover_for(chosen, s),
            method: if close {
                DerivedHistoricalVegetationMethod::MultiSourceRulesV4TieBreak
            } else {
                DerivedHistoricalVegetationMethod::MultiSourceRulesV4
            },
        }),
        close,
    )
}

fn direct_hyde35_cover(
    profile: adventuresim_world_schema::LandUseProfile,
    evidence: LandUseEvidence,
) -> Option<DirectHistoricalVegetationCover> {
    if !matches!(evidence, LandUseEvidence::Hyde35Sampled { .. }) {
        return None;
    }
    let crop = profile.cropland().basis_points();
    let grazing = profile.grazing().basis_points();
    let built = profile.built_up().basis_points();
    if built >= crop && built >= grazing {
        (built >= 1_000).then_some(DirectHistoricalVegetationCover::BuiltSettlement(
            BuiltSettlementCover {
                built_fraction: profile.built_up(),
            },
        ))
    } else if crop >= grazing {
        (crop >= 3_500).then_some(DirectHistoricalVegetationCover::Cropland(CroplandCover {
            cultivated_fraction: profile.cropland(),
        }))
    } else {
        (grazing >= 3_500).then_some(DirectHistoricalVegetationCover::Pasture(PastureCover {
            grazing_fraction: profile.grazing(),
        }))
    }
}

fn choose_from_scores(
    mut scores: [(NaturalCover, i32); 6],
    latitude: f64,
    longitude: f64,
) -> (NaturalCover, bool) {
    scores.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| natural_rank(a.0).cmp(&natural_rank(b.0)))
    });
    let close = scores[0].1 - scores[1].1 <= CLOSE_MARGIN;
    let chosen = if close {
        let close_count = scores
            .iter()
            .take_while(|x| scores[0].1 - x.1 <= CLOSE_MARGIN)
            .count();
        scores[(stable_hash(latitude, longitude) as usize) % close_count].0
    } else {
        scores[0].0
    };
    (chosen, close)
}

fn posterior_score(value: &PotentialVegetation, class: PotentialVegetationClass) -> i32 {
    match value {
        PotentialVegetation::Posterior(v) => i32::from(
            match class {
                PotentialVegetationClass::WoodlandAndForest => v.woodland_and_forest,
                PotentialVegetationClass::HeathlandAndShrub => v.heathland_and_shrub,
                PotentialVegetationClass::Grassland => v.grassland,
                PotentialVegetationClass::SparselyVegetatedAreas => v.sparsely_vegetated_areas,
                PotentialVegetationClass::Wetlands => v.wetlands,
                PotentialVegetationClass::MarineInletsAndTransitionalWaters => {
                    v.marine_inlets_and_transitional_waters
                }
            }
            .get(),
        ),
        PotentialVegetation::Categorical(v) => {
            if *v == class {
                7_500
            } else {
                0
            }
        }
        PotentialVegetation::Inferred(v) => {
            if *v == class {
                5_000
            } else {
                0
            }
        }
    }
}

fn natural_from_potential(value: PotentialVegetationClass) -> NaturalCover {
    match value {
        PotentialVegetationClass::WoodlandAndForest => NaturalCover::Woodland,
        PotentialVegetationClass::HeathlandAndShrub => NaturalCover::Heath,
        PotentialVegetationClass::Grassland => NaturalCover::Grass,
        PotentialVegetationClass::SparselyVegetatedAreas => NaturalCover::Sparse,
        PotentialVegetationClass::Wetlands => NaturalCover::Wetland,
        PotentialVegetationClass::MarineInletsAndTransitionalWaters => {
            NaturalCover::TransitionalWater
        }
    }
}

fn cover_for(
    value: NaturalCover,
    s: &FinalizedSoilSettlementDraft,
) -> DerivedHistoricalVegetationCover {
    match value {
        NaturalCover::Woodland => match s
            .hydrologic
            .drought
            .religious
            .geologic
            .predicted
            .trees
            .vegetated
            .forest
            .forest_cover
        {
            ForestCover::Wooded(v) => {
                DerivedHistoricalVegetationCover::Woodland(HistoricalWoodland {
                    canopy: v.density,
                    dominant: v.dominant,
                })
            }
            ForestCover::Open => DerivedHistoricalVegetationCover::Woodland(HistoricalWoodland {
                canopy: CanopyDensity::new(20).unwrap(),
                dominant: DominantLeafType::Mixed,
            }),
        },
        NaturalCover::Heath => DerivedHistoricalVegetationCover::HeathAndShrub,
        NaturalCover::Grass => DerivedHistoricalVegetationCover::Grassland,
        NaturalCover::Sparse => DerivedHistoricalVegetationCover::Sparse,
        NaturalCover::Wetland => DerivedHistoricalVegetationCover::Wetland(HistoricalWetland {
            water_regime: s.soil.properties.water_regime,
        }),
        NaturalCover::TransitionalWater => DerivedHistoricalVegetationCover::TransitionalWater,
    }
}

fn fallback_cover_for(
    value: NaturalCover,
    s: &FinalizedSoilSettlementDraft,
) -> FallbackHistoricalVegetationCover {
    match safe_fallback_natural(value) {
        NaturalCover::Woodland => match s
            .hydrologic
            .drought
            .religious
            .geologic
            .predicted
            .trees
            .vegetated
            .forest
            .forest_cover
        {
            ForestCover::Wooded(v) => {
                FallbackHistoricalVegetationCover::Woodland(HistoricalWoodland {
                    canopy: v.density,
                    dominant: v.dominant,
                })
            }
            ForestCover::Open => FallbackHistoricalVegetationCover::Woodland(HistoricalWoodland {
                canopy: CanopyDensity::new(20).unwrap(),
                dominant: DominantLeafType::Mixed,
            }),
        },
        NaturalCover::Heath => FallbackHistoricalVegetationCover::HeathAndShrub,
        NaturalCover::Sparse => FallbackHistoricalVegetationCover::Sparse,
        NaturalCover::Grass => FallbackHistoricalVegetationCover::Grassland,
        NaturalCover::Wetland | NaturalCover::TransitionalWater => {
            unreachable!("water fallback was normalized")
        }
    }
}

const fn safe_fallback_natural(value: NaturalCover) -> NaturalCover {
    match value {
        NaturalCover::Wetland | NaturalCover::TransitionalWater => NaturalCover::Grass,
        other => other,
    }
}

const fn natural_rank(value: NaturalCover) -> u8 {
    match value {
        NaturalCover::Woodland => 0,
        NaturalCover::Heath => 1,
        NaturalCover::Grass => 2,
        NaturalCover::Sparse => 3,
        NaturalCover::Wetland => 4,
        NaturalCover::TransitionalWater => 5,
    }
}

fn stable_hash(latitude: f64, longitude: f64) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in latitude
        .to_bits()
        .to_le_bytes()
        .into_iter()
        .chain(longitude.to_bits().to_le_bytes())
        .chain(WORLD_SCHEMA_VERSION.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{
        CLOSE_MARGIN, NaturalCover, choose_from_scores, direct_hyde35_cover, finalize,
        safe_fallback_natural, stable_hash,
    };
    use crate::draft::{FinalizedSoilWorldDraft, LandUseEvidence};
    use adventuresim_world_schema::{
        DirectHistoricalVegetationCover, FerryRoute, FerryWaterway, LandUseFraction,
        LandUseProfile, SpatialGridSpec, TravelEdgeImport, TravelEdgeKind, TravelRoute,
        WorldBuildReport, WorldNodeImport,
    };

    fn scores(winner: NaturalCover, advantage: i32) -> [(NaturalCover, i32); 6] {
        let mut values = [
            (NaturalCover::Woodland, 1_000),
            (NaturalCover::Heath, 1_000),
            (NaturalCover::Grass, 1_000),
            (NaturalCover::Sparse, 1_000),
            (NaturalCover::Wetland, 1_000),
            (NaturalCover::TransitionalWater, 1_000),
        ];
        values.iter_mut().find(|v| v.0 == winner).unwrap().1 += advantage;
        values
    }

    #[test]
    fn table_driven_cross_source_outcomes_cover_natural_classes() {
        for expected in [
            NaturalCover::Woodland,
            NaturalCover::Heath,
            NaturalCover::Grass,
            NaturalCover::Sparse,
            NaturalCover::Wetland,
            NaturalCover::TransitionalWater,
        ] {
            assert_eq!(
                choose_from_scores(scores(expected, 2_000), 54.0, 10.0),
                (expected, false)
            );
        }
    }

    #[test]
    fn moisture_and_drought_adjustments_can_change_close_outcomes() {
        let mut baseline = scores(NaturalCover::Grass, CLOSE_MARGIN + 1);
        assert_eq!(
            choose_from_scores(baseline, 54.0, 10.0).0,
            NaturalCover::Grass
        );
        baseline
            .iter_mut()
            .find(|v| v.0 == NaturalCover::Sparse)
            .unwrap()
            .1 += 1_000;
        assert_eq!(
            choose_from_scores(baseline, 54.0, 10.0).0,
            NaturalCover::Sparse
        );
    }

    #[test]
    fn coordinate_hash_is_used_only_inside_close_margin() {
        let far = scores(NaturalCover::Woodland, CLOSE_MARGIN + 1);
        assert_eq!(
            choose_from_scores(far, 10.0, 10.0),
            (NaturalCover::Woodland, false)
        );
        let close = scores(NaturalCover::Woodland, CLOSE_MARGIN);
        assert!(choose_from_scores(close, 10.0, 10.0).1);
        assert_eq!(stable_hash(10.0, 20.0), stable_hash(10.0, 20.0));
        assert_ne!(stable_hash(10.0, 20.0), stable_hash(10.0, 20.001));
    }

    fn land_use(crop: u16, grazing: u16, built: u16) -> LandUseProfile {
        LandUseProfile::new(
            LandUseFraction::new(crop).unwrap(),
            LandUseFraction::new(grazing).unwrap(),
            LandUseFraction::new(built).unwrap(),
            LandUseFraction::new(10_000 - crop - grazing - built).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn only_sampled_hyde35_and_the_selected_threshold_can_be_direct() {
        let larger_crop_below_threshold = land_use(2_000, 0, 1_000);
        assert!(
            direct_hyde35_cover(
                larger_crop_below_threshold,
                LandUseEvidence::Hyde35Sampled { normalized: false }
            )
            .is_none()
        );
        let built_selected = land_use(500, 0, 1_000);
        assert!(matches!(
            direct_hyde35_cover(
                built_selected,
                LandUseEvidence::Hyde35Sampled { normalized: false }
            ),
            Some(DirectHistoricalVegetationCover::BuiltSettlement(_))
        ));
        assert!(
            direct_hyde35_cover(built_selected, LandUseEvidence::DeterministicFallback).is_none()
        );
        assert!(matches!(
            direct_hyde35_cover(
                land_use(4_000, 0, 1_000),
                LandUseEvidence::Hyde35Sampled { normalized: false }
            ),
            Some(DirectHistoricalVegetationCover::Cropland(_))
        ));
        assert!(
            direct_hyde35_cover(
                land_use(3_400, 0, 0),
                LandUseEvidence::Hyde35Sampled { normalized: false }
            )
            .is_none()
        );
        assert!(matches!(
            direct_hyde35_cover(
                land_use(3_500, 0, 0),
                LandUseEvidence::Hyde35Sampled { normalized: true }
            ),
            Some(DirectHistoricalVegetationCover::Cropland(_))
        ));
        assert!(matches!(
            direct_hyde35_cover(
                land_use(2_000, 3_500, 0),
                LandUseEvidence::Hyde35Sampled { normalized: false }
            ),
            Some(DirectHistoricalVegetationCover::Pasture(_))
        ));
    }

    #[test]
    fn fallback_water_classes_become_non_water_cover() {
        assert_eq!(
            safe_fallback_natural(NaturalCover::TransitionalWater),
            NaturalCover::Grass
        );
        assert_eq!(
            safe_fallback_natural(NaturalCover::Wetland),
            NaturalCover::Grass
        );
    }

    fn topology_draft() -> FinalizedSoilWorldDraft {
        FinalizedSoilWorldDraft {
            year: 1544,
            spatial_grid: SpatialGridSpec::default(),
            sources: vec![crate::manifest::hydrology()],
            road_types: vec![TravelEdgeKind::Ferry],
            nodes: vec![
                WorldNodeImport {
                    id: 1,
                    parent_node_id: None,
                    latitude: 54.0,
                    longitude: 10.0,
                    is_settlement: false,
                    is_town: false,
                    is_ferry: true,
                    is_harbour: false,
                    sources: "- test".into(),
                },
                WorldNodeImport {
                    id: 2,
                    parent_node_id: None,
                    latitude: 54.1,
                    longitude: 10.1,
                    is_settlement: false,
                    is_town: false,
                    is_ferry: true,
                    is_harbour: false,
                    sources: "- test".into(),
                },
            ],
            edges: vec![TravelEdgeImport {
                id: 7,
                from_node_id: 1,
                to_node_id: 2,
                route: TravelRoute::Ferry(FerryRoute {
                    waterway: FerryWaterway::TidalWater,
                }),
                provenance: adventuresim_world_schema::TravelEdgeProvenance::DocumentedViabundus,
                geometry: Vec::new(),
                toll: None,
                length_m: 10,
                slope_multiplier: 1.0,
                terrain: adventuresim_world_schema::RouteTerrain::stage_placeholder(),
                certainty: 1,
                section: "test".into(),
                sources: "- test".into(),
            }],
            settlement_aliases: vec![],
            settlement_descriptions: vec![],
            settlements: vec![],
            report: WorldBuildReport {
                nodes: 2,
                edges: 1,
                route_crossings: 1,
                ..Default::default()
            },
        }
    }

    #[test]
    fn final_stage_preserves_topology_counts_and_is_order_stable() {
        let first = finalize(topology_draft()).unwrap();
        let second = finalize(topology_draft()).unwrap();
        assert_eq!(first.nodes.len(), 2);
        assert_eq!(first.edges.len(), 1);
        assert!(matches!(first.edges[0].route, TravelRoute::Ferry(_)));
        assert_eq!(first.report.nodes, 2);
        assert_eq!(first.report.edges, 1);
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&second).unwrap()
        );
    }
}
