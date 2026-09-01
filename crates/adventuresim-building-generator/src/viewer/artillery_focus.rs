const VIEW_WIDTH: u32 = 1440;
const VIEW_HEIGHT: u32 = 900;

const TIMBER_ARCHETYPES: [BuildingArchetype; 5] = [
    BuildingArchetype::TownHouse,
    BuildingArchetype::HallHouse,
    BuildingArchetype::FachwerkCottage,
    BuildingArchetype::FachwerkMerchantHouse,
    BuildingArchetype::RenaissanceTownHall,
];

const fn timber_proof_suffix(view: ViewerView) -> Option<&'static str> {
    Some(match view {
        ViewerView::TimberWholeExterior => "whole-exterior",
        ViewerView::TimberFrameFacade => "frame-only-facade",
        ViewerView::TimberRegistrationCut => "circulation-registration-cut",
        ViewerView::TimberSupportLoad => "support-load",
        ViewerView::TimberProgramDetail => "program-detail",
        ViewerView::TimberOpeningBayExterior => "timber-opening-bay-exterior",
        ViewerView::TimberOpeningBayInterior => "timber-opening-bay-interior",
        ViewerView::TimberOpeningBaySection => "timber-opening-bay-section",
        ViewerView::TimberJointClose => "timber-joint-close",
        ViewerView::TimberJettyExterior => "timber-jetty-exterior",
        ViewerView::TimberJettyUnderside => "timber-jetty-underside",
        ViewerView::TimberJettyLoad => "timber-jetty-load",
        ViewerView::TimberGableRoofBearing => "timber-gable-roof-bearing",
        ViewerView::TimberDormerTrimmer => "timber-dormer-trimmer",
        ViewerView::TimberTownHallJunction => "timber-townhall-masonry-junction",
        _ => return None,
    })
}

const fn artillery_proof_slug(view: ViewerView) -> Option<&'static str> {
    Some(match view {
        ViewerView::ArtilleryWholeExterior => "artillery-whole-exterior",
        ViewerView::ArtilleryWholeCourtyard => "artillery-whole-courtyard",
        ViewerView::ArtilleryWholeTop => "artillery-whole-top",
        ViewerView::ArtilleryWholeLongitudinalCut => "artillery-whole-longitudinal-cut",
        ViewerView::ArtilleryWholeTransverseCut => "artillery-whole-transverse-cut",
        ViewerView::ArtilleryTracePlan => "artillery-trace-plan",
        ViewerView::ArtilleryCurtainSection => "artillery-curtain-section",
        ViewerView::ArtilleryCurtainTerreplein => "artillery-curtain-terreplein",
        ViewerView::ArtilleryRondelExterior => "artillery-rondel-exterior",
        ViewerView::ArtilleryRondelCasemate => "artillery-rondel-casemate",
        ViewerView::ArtilleryRondelCutaway => "artillery-rondel-cutaway",
        ViewerView::ArtilleryRondelTop => "artillery-rondel-top",
        ViewerView::ArtilleryGateApproach => "artillery-gate-approach",
        ViewerView::ArtilleryGateInterior => "artillery-gate-interior",
        ViewerView::ArtilleryBridgeDeployed => "artillery-bridge-deployed",
        ViewerView::ArtilleryBridgeDenied => "artillery-bridge-denied",
        ViewerView::ArtilleryCirculation => "artillery-circulation",
        ViewerView::ArtilleryDrainage => "artillery-drainage",
        ViewerView::ArtillerySupportDag => "artillery-support-dag",
        ViewerView::ArtilleryFirePlan => "artillery-fire-plan",
        _ => return None,
    })
}

fn artillery_camera(plan: &BuildingPlan, view: ViewerView, origin: Vec2) -> Option<(Vec3, Vec3)> {
    plan.artillery_castle.as_ref()?;
    artillery_proof_slug(view)?;
    let whole = Vec3::new(6.0 + origin.x, 3.0, 6.0 + origin.y);
    let rondel = plan
        .towers
        .first()
        .map(|tower| tower.centre_metres() + origin)
        .unwrap_or(Vec2::ZERO);
    let rondel_focus = Vec3::new(rondel.x, 3.6, rondel.y);
    let gate = Vec3::new(6.0 + origin.x, 2.4, -11.5 + origin.y);
    let bridge = Vec3::new(6.0 + origin.x, 0.0, -17.0 + origin.y);
    Some(match view {
        ViewerView::ArtilleryWholeExterior => (whole + Vec3::new(48.0, 24.0, -57.0), whole),
        ViewerView::ArtilleryWholeCourtyard => {
            (whole + Vec3::new(-56.0, 30.0, 59.0), whole + Vec3::Y * 1.8)
        }
        ViewerView::ArtilleryWholeTop
        | ViewerView::ArtilleryTracePlan
        | ViewerView::ArtilleryFirePlan => (whole + Vec3::new(30.0, 120.0, -30.0), whole),
        ViewerView::ArtilleryWholeLongitudinalCut => (whole + Vec3::new(90.0, 25.0, -3.0), whole),
        ViewerView::ArtilleryWholeTransverseCut => (whole + Vec3::new(2.0, 25.0, -90.0), whole),
        // Look squarely onto the exposed end of the western south-curtain
        // half.  The gate gap lies at x=6, so the previous view along the
        // facade collapsed the revetment/earth/retaining stack into one pale
        // silhouette instead of proving its authoritative 4.5 m depth.
        ViewerView::ArtilleryCurtainSection => (
            Vec3::new(38.0 + origin.x, 10.0, -35.0 + origin.y),
            Vec3::new(4.35 + origin.x, 3.05, -11.25 + origin.y),
        ),
        ViewerView::ArtilleryCurtainTerreplein => (
            Vec3::new(-34.0 + origin.x, 18.0, -46.0 + origin.y),
            Vec3::new(6.0 + origin.x, 5.5, -11.0 + origin.y),
        ),
        ViewerView::ArtilleryRondelExterior => {
            (rondel_focus + Vec3::new(-14.0, 6.0, -15.0), rondel_focus)
        }
        // View the removed south-west quadrant at working height so the two
        // lower flanking recesses on the surviving north/east shell, their
        // mounts, recoil rooms, smoke paths and residual earth read together.
        ViewerView::ArtilleryRondelCasemate => (
            rondel_focus + Vec3::new(-16.0, 4.2, -20.0),
            rondel_focus - Vec3::Y * 2.1,
        ),
        ViewerView::ArtilleryRondelCutaway => {
            (rondel_focus + Vec3::new(22.0, 14.0, -22.0), rondel_focus)
        }
        ViewerView::ArtilleryRondelTop => (rondel_focus + Vec3::new(5.0, 42.0, -5.0), rondel_focus),
        ViewerView::ArtilleryGateApproach => (gate + Vec3::new(0.0, 20.0, -54.0), gate + Vec3::Y),
        // Aim into the open bailey side of the upper chamber rather than
        // beneath its floor.  A slight three-quarter offset separates the
        // windlass, rope, paired closures, access and side bearings.
        ViewerView::ArtilleryGateInterior => (
            gate + Vec3::new(7.5, 7.5, 12.0),
            Vec3::new(gate.x, 4.0, gate.z - 0.15),
        ),
        ViewerView::ArtilleryBridgeDeployed | ViewerView::ArtilleryBridgeDenied => {
            (bridge + Vec3::new(10.0, 6.0, -12.0), bridge)
        }
        ViewerView::ArtilleryCirculation | ViewerView::ArtillerySupportDag => {
            (whole + Vec3::new(58.0, 60.0, -68.0), whole)
        }
        ViewerView::ArtilleryDrainage => {
            (whole + Vec3::new(30.0, 120.0, -30.0), whole - Vec3::Y * 0.7)
        }
        _ => return None,
    })
}

const fn artillery_isolated_view(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::ArtilleryCurtainSection
            | ViewerView::ArtilleryRondelCasemate
            | ViewerView::ArtilleryGateInterior
    )
}

fn artillery_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(castle) = &plan.artillery_castle else {
        return Vec::new();
    };
    if matches!(
        view,
        ViewerView::ArtilleryWholeExterior
            | ViewerView::ArtilleryWholeCourtyard
            | ViewerView::ArtilleryWholeTop
            | ViewerView::ArtilleryWholeLongitudinalCut
            | ViewerView::ArtilleryWholeTransverseCut
            | ViewerView::ArtilleryTracePlan
            | ViewerView::ArtilleryCirculation
            | ViewerView::ArtilleryDrainage
            | ViewerView::ArtillerySupportDag
            | ViewerView::ArtilleryFirePlan
    ) {
        return plan
            .resolved_geometry
            .solids
            .iter()
            .map(|solid| solid.id.0)
            .collect();
    }
    if matches!(
        view,
        ViewerView::ArtilleryGateApproach | ViewerView::ArtilleryGateInterior
    ) {
        let mut ids = castle
            .gate_closure_solids
            .iter()
            .chain(&castle.gate_chamber_solids)
            .map(|id| id.0)
            .collect::<Vec<_>>();
        if view == ViewerView::ArtilleryGateApproach {
            ids.extend(
                castle
                    .bridge
                    .fixed_solids
                    .iter()
                    .chain(&castle.bridge.removable_solids)
                    .map(|id| id.0),
            );
            ids.extend(
                castle
                    .rondels
                    .iter()
                    .take(2)
                    .flat_map(|rondel| [rondel.shell_solid, rondel.terreplein_solid])
                    .map(|id| id.0),
            );
        }
        return ids;
    }
    if matches!(
        view,
        ViewerView::ArtilleryBridgeDeployed | ViewerView::ArtilleryBridgeDenied
    ) {
        return std::iter::once(castle.bridge.inner_abutment)
            .chain(std::iter::once(castle.bridge.outer_abutment))
            .chain(castle.bridge.fixed_solids.iter().copied())
            .chain(castle.bridge.removable_solids.iter().copied())
            .filter(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id)
            })
            .map(|id| id.0)
            .collect();
    }
    let owners = match view {
        ViewerView::ArtilleryRondelExterior
        | ViewerView::ArtilleryRondelCasemate
        | ViewerView::ArtilleryRondelCutaway
        | ViewerView::ArtilleryRondelTop => {
            let rondel = &castle.rondels[0];
            std::collections::HashSet::from_iter(
                std::iter::once(rondel.owner).chain(
                    castle
                        .stations
                        .iter()
                        .filter(|station| station.rondel == rondel.id)
                        .filter_map(|station| {
                            plan.opening_assemblies
                                .iter()
                                .find(|opening| opening.id == station.opening)
                                .map(|opening| opening.owner)
                        }),
                ),
            )
        }
        ViewerView::ArtilleryCurtainSection | ViewerView::ArtilleryCurtainTerreplein => {
            std::collections::HashSet::from([castle.curtains[0].owner])
        }
        _ => castle
            .curtains
            .iter()
            .map(|curtain| curtain.owner)
            .chain(castle.rondels.iter().map(|rondel| rondel.owner))
            .collect(),
    };
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| owners.contains(&solid.owner))
        .map(|solid| solid.id.0)
        .collect()
}

fn artillery_focus_void_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(castle) = &plan.artillery_castle else {
        return Vec::new();
    };
    match view {
        ViewerView::ArtilleryRondelCasemate | ViewerView::ArtilleryRondelCutaway => {
            std::iter::once(castle.rondels[0].casemate_void.0)
                .chain(
                    castle
                        .stations
                        .iter()
                        .filter(|station| station.rondel == castle.rondels[0].id)
                        .filter_map(|station| {
                            plan.opening_assemblies
                                .iter()
                                .find(|opening| opening.id == station.opening)
                                .map(|opening| opening.void_id.0)
                        }),
                )
                .chain(
                    castle
                        .stations
                        .iter()
                        .filter(|station| station.rondel == castle.rondels[0].id)
                        .filter_map(|station| station.smoke_vent.map(|id| id.0)),
                )
                .collect()
        }
        ViewerView::ArtilleryGateApproach | ViewerView::ArtilleryGateInterior => {
            vec![castle.gate_passage_void.0]
        }
        ViewerView::ArtilleryBridgeDenied => castle
            .bridge
            .denied_gap_void
            .into_iter()
            .map(|id| id.0)
            .collect(),
        ViewerView::ArtilleryDrainage
        | ViewerView::ArtilleryTracePlan
        | ViewerView::ArtilleryWholeTop => vec![castle.ditch.void_id.0],
        ViewerView::ArtilleryFirePlan => castle
            .stations
            .iter()
            .filter_map(|station| {
                plan.opening_assemblies
                    .iter()
                    .find(|opening| opening.id == station.opening)
                    .map(|opening| opening.void_id.0)
            })
            .collect(),
        _ => Vec::new(),
    }
}

const fn artillery_section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::ArtilleryWholeLongitudinalCut
            | ViewerView::ArtilleryWholeTransverseCut
            | ViewerView::ArtilleryCurtainSection
            | ViewerView::ArtilleryRondelCasemate
            | ViewerView::ArtilleryRondelCutaway
            | ViewerView::ArtilleryCirculation
            | ViewerView::ArtillerySupportDag
    )
}

fn artillery_cut_plane(view: ViewerView) -> Option<[f32; 4]> {
    Some(match view {
        ViewerView::ArtilleryWholeLongitudinalCut => [1.0, 0.0, 0.0, -6.0],
        ViewerView::ArtilleryWholeTransverseCut => [0.0, 0.0, 1.0, -6.0],
        ViewerView::ArtilleryCurtainSection => [1.0, 0.0, 0.0, -6.0],
        ViewerView::ArtilleryRondelCasemate | ViewerView::ArtilleryRondelCutaway => {
            // Plane through the first rondel centre, normal toward the
            // removed south-west quadrant used by the proof camera.
            [-0.707_106_77, 0.0, -0.707_106_77, -21.213_203]
        }
        ViewerView::ArtilleryCirculation | ViewerView::ArtillerySupportDag => [1.0, 0.0, 0.0, -6.0],
        _ => return None,
    })
}

fn artillery_section_removed_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    if !artillery_section_proof(view) {
        return Vec::new();
    }
    let focus = artillery_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let rondel_centre = plan.towers.first().map(|tower| tower.centre_metres());
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| focus.contains(&solid.id.0))
        .filter(|solid| match view {
            ViewerView::ArtilleryWholeLongitudinalCut
            | ViewerView::ArtilleryCirculation
            | ViewerView::ArtillerySupportDag => solid.centre.x > 6.0,
            ViewerView::ArtilleryWholeTransverseCut => solid.centre.z < 6.0,
            ViewerView::ArtilleryCurtainSection => solid.centre.x > 6.0,
            ViewerView::ArtilleryRondelCasemate | ViewerView::ArtilleryRondelCutaway => {
                rondel_centre.is_some_and(|centre| {
                    (Vec2::new(solid.centre.x, solid.centre.z) - centre)
                        .dot(Vec2::new(-0.707_106_77, -0.707_106_77))
                        > 0.1
                        || (view == ViewerView::ArtilleryRondelCasemate && solid.centre.y > 3.05)
                })
            }
            _ => false,
        })
        .map(|solid| solid.id.0)
        .collect()
}
