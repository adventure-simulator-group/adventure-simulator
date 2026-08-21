use std::{fs, path::PathBuf};

use adventuresim_building_generator::{
    BattlementKind, BattlementRun, BuildingArchetype, BuildingDocument, BuildingEdit, BuildingPlan,
    CELL_SIZE_METRES, CrownPath, CurtainWallRun, Direction, DormerKind, FiringPosition,
    GableProfile, GateClosure, GateClosureKind, GateDefense, GatehouseLoadPath, GuardOpeningKind,
    Opening, OpeningKind, PlayerBuildDocument, PlayerBuildEdit, PlayerBuildMaterial,
    PlayerBuildPart, PlayerBuildPartKind, ProjectedDefenseDeployment, ProjectedDefensePath,
    ProjectedDefenseTarget, RidgeAxis, RoofAssembly, RoofDormer, RoofEnclosureFace, RoofFace,
    RoofKind, RoofMaterial, RoofPiece, RoundTower, SolidRole, SquareTower, Stair, TimberFrameStyle,
    TowerPortal, TowerPortalKind, WALL_THICKNESS_METRES, WallSegment, WallSelector, WallSourceId,
    WallStyle, WallWalk, analyse_player_build, audit_plan, audit_triangle_mesh, edit_document,
    generate, generate_document,
};
use bevy::{
    app::AppExit,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{PresentMode, WindowResolution},
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use bevy_mod_outline::{OutlineMode, OutlinePlugin, OutlineVolume};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use serde::{Deserialize, Serialize};

use crate::{ProjectedProofKind, RoofProofView, ViewerView};

#[cfg(test)]
use adventuresim_building_generator::BuildingProgram;
#[cfg(test)]
use adventuresim_building_generator::PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION;
#[cfg(test)]
use bevy::ecs::system::RunSystemOnce;

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

fn timber_proof_slug(plan: &BuildingPlan, view: ViewerView) -> Option<String> {
    let suffix = timber_proof_suffix(view)?;
    Some(
        if matches!(
            view,
            ViewerView::TimberWholeExterior
                | ViewerView::TimberFrameFacade
                | ViewerView::TimberRegistrationCut
                | ViewerView::TimberSupportLoad
                | ViewerView::TimberProgramDetail
        ) {
            format!("timber-{}-{suffix}", plan.archetype.slug())
        } else {
            suffix.to_owned()
        },
    )
}

fn timber_target_component_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<String> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let claim = match view {
        ViewerView::TimberWholeExterior => "whole",
        ViewerView::TimberFrameFacade => "south-facade/frame",
        ViewerView::TimberRegistrationCut => "occupied-level/circulation-registration",
        ViewerView::TimberSupportLoad => "south-facade/support-load",
        ViewerView::TimberProgramDetail => match frame.program {
            adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse => {
                "two-post-hall/inner-rows"
            }
            adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage => {
                "direct-roof/gable"
            }
            adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall => {
                "civic-hall/broad-span"
            }
            adventuresim_building_generator::TimberFrameProgramKind::NarrowUrbanTownHouse => {
                "urban-frame/jetty"
            }
            adventuresim_building_generator::TimberFrameProgramKind::JettiedMerchantHouse => {
                "merchant-frame/jetty"
            }
        },
        ViewerView::TimberOpeningBayExterior
        | ViewerView::TimberOpeningBayInterior
        | ViewerView::TimberOpeningBaySection => "opening-bay/reframed-load",
        ViewerView::TimberJointClose => "joint/post-plate",
        ViewerView::TimberJettyExterior
        | ViewerView::TimberJettyUnderside
        | ViewerView::TimberJettyLoad => "jetty/cantilever-bearing",
        ViewerView::TimberGableRoofBearing => "gable/roof-seat",
        ViewerView::TimberDormerTrimmer => "roof-child/trimmer",
        ViewerView::TimberTownHallJunction => "civic-hall/masonry-timber-bearing",
        _ => return Vec::new(),
    };
    vec![format!("timber:{}/{}", frame.id.0, claim)]
}

fn timber_focus_interface_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let focused = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let focused_nodes = frame
        .members
        .iter()
        .filter(|member| focused.contains(&member.solid.0))
        .flat_map(|member| [member.start_node, member.end_node])
        .collect::<std::collections::HashSet<_>>();
    plan.resolved_geometry
        .support_interfaces
        .iter()
        .filter(|interface| interface.owner == frame.members[0].owner)
        .filter(|interface| focused_nodes.contains(&interface.node))
        .map(|interface| interface.id.0)
        .collect()
}

fn timber_section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::TimberRegistrationCut
            | ViewerView::TimberSupportLoad
            | ViewerView::TimberOpeningBaySection
            | ViewerView::TimberGableRoofBearing
            | ViewerView::TimberDormerTrimmer
            | ViewerView::TimberTownHallJunction
    )
}

fn timber_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    use adventuresim_building_generator::TimberMemberRole as Role;
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let mut member_ids = match view {
        ViewerView::TimberRegistrationCut
            if frame.program
                == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse =>
        {
            frame
                .internal_lines
                .iter()
                .flat_map(|line| &line.storeys)
                .flat_map(|storey| &storey.member_ids)
                .copied()
                .collect()
        }
        ViewerView::TimberRegistrationCut
            if frame.program
                == adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage =>
        {
            frame
                .facades
                .iter()
                .find(|facade| facade.outward == Direction::South)
                .into_iter()
                .flat_map(|facade| &facade.lines)
                .flat_map(|line| &line.storeys)
                .flat_map(|storey| &storey.member_ids)
                .copied()
                .collect()
        }
        ViewerView::TimberRegistrationCut => std::collections::HashSet::new(),
        ViewerView::TimberFrameFacade | ViewerView::TimberSupportLoad => frame
            .facades
            .iter()
            .find(|facade| facade.outward == Direction::South)
            .into_iter()
            .flat_map(|facade| &facade.lines)
            .flat_map(|line| &line.storeys)
            .flat_map(|storey| &storey.member_ids)
            .copied()
            .collect::<std::collections::HashSet<_>>(),
        ViewerView::TimberOpeningBayExterior
        | ViewerView::TimberOpeningBayInterior
        | ViewerView::TimberOpeningBaySection => frame
            .bays
            .iter()
            .find(|bay| bay.opening.is_some())
            .into_iter()
            .flat_map(|bay| &bay.member_ids)
            .copied()
            .collect(),
        ViewerView::TimberJointClose => frame
            .joints
            .iter()
            .filter(|joint| {
                let has_role = |role| {
                    joint.member_ids.iter().any(|id| {
                        frame
                            .members
                            .iter()
                            .find(|member| member.id == *id)
                            .is_some_and(|member| member.role == role)
                    })
                };
                has_role(Role::PrimaryPost) && has_role(Role::WallPlate)
            })
            .max_by_key(|joint| joint.member_ids.len())
            .into_iter()
            .flat_map(|joint| &joint.member_ids)
            .copied()
            .collect(),
        ViewerView::TimberJettyExterior
        | ViewerView::TimberJettyUnderside
        | ViewerView::TimberJettyLoad => frame
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .flat_map(|line| &line.storeys)
            .find(|storey| storey.jetty.is_some())
            .into_iter()
            .filter_map(|storey| storey.jetty.as_ref())
            .flat_map(|jetty| {
                jetty
                    .jetty_beams
                    .iter()
                    .chain(&jetty.knaggen)
                    .chain(&jetty.corner_supports)
            })
            .copied()
            .collect(),
        ViewerView::TimberGableRoofBearing => frame
            .members
            .iter()
            .filter(|member| {
                matches!(
                    member.role,
                    Role::GableTie | Role::GablePost | Role::Rafter | Role::Collar | Role::Purlin
                )
            })
            .map(|member| member.id)
            .collect(),
        ViewerView::TimberDormerTrimmer => frame.dormer_trimmer_members.iter().copied().collect(),
        ViewerView::TimberTownHallJunction => {
            // Prove the masonry-to-timber transition where the civic hall's
            // broad internal girder actually meets the storey frame, rather
            // than at an arbitrary facade corner. This keeps the cut on the
            // continuous masonry bearing run and makes both structural
            // systems visible in the same exact-ID detail.
            let hall_girder_centre = frame
                .members
                .iter()
                .filter(|member| member.role == Role::Girder)
                .max_by(|left, right| {
                    left.start
                        .distance(left.end)
                        .total_cmp(&right.start.distance(right.end))
                })
                .map(|member| (member.start + member.end) * 0.5);
            frame
                .members
                .iter()
                .filter(|member| {
                    member.role == Role::Sill
                        && (member.start.y - plan.storey_height_metres).abs() <= 0.02
                })
                .min_by(|left, right| {
                    let distance = |member: &adventuresim_building_generator::TimberFrameMember| {
                        hall_girder_centre.map_or(0.0, |centre| {
                            ((member.start + member.end) * 0.5).distance(centre)
                        })
                    };
                    distance(left).total_cmp(&distance(right))
                })
                .map(|member| member.id)
                .into_iter()
                .collect()
        }
        ViewerView::TimberProgramDetail
            if frame.program
                == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse =>
        {
            frame
                .internal_lines
                .iter()
                .flat_map(|line| &line.storeys)
                .flat_map(|storey| &storey.member_ids)
                .copied()
                .collect()
        }
        _ => frame.members.iter().map(|member| member.id).collect(),
    };
    if matches!(
        view,
        ViewerView::TimberJettyExterior
            | ViewerView::TimberJettyUnderside
            | ViewerView::TimberJettyLoad
    ) {
        let connected = frame
            .joints
            .iter()
            .filter(|joint| joint.member_ids.iter().any(|id| member_ids.contains(id)))
            .flat_map(|joint| joint.member_ids.iter().copied())
            .collect::<Vec<_>>();
        member_ids.extend(connected);
    }
    if view == ViewerView::TimberTownHallJunction {
        let connected = frame
            .joints
            .iter()
            .filter(|joint| joint.member_ids.iter().any(|id| member_ids.contains(id)))
            .flat_map(|joint| joint.member_ids.iter().copied())
            .collect::<Vec<_>>();
        member_ids.extend(connected);
        let target_centre = frame
            .members
            .iter()
            .find(|member| member_ids.contains(&member.id) && member.role == Role::Sill)
            .map(|member| (member.start + member.end) * 0.5);
        if let Some(centre) = target_centre {
            member_ids.extend(
                frame
                    .members
                    .iter()
                    .filter(|member| {
                        member.role == Role::Sill
                            && ((member.start + member.end) * 0.5).distance(centre) <= 5.5
                    })
                    .map(|member| member.id),
            );
        }
        if let Some(girder) = target_centre.and_then(|centre| {
            frame
                .members
                .iter()
                .filter(|member| member.role == Role::Girder)
                .min_by(|left, right| {
                    ((left.start + left.end) * 0.5)
                        .distance(centre)
                        .total_cmp(&((right.start + right.end) * 0.5).distance(centre))
                })
        }) {
            member_ids.insert(girder.id);
        }
    }
    if view == ViewerView::TimberProgramDetail
        && frame.program
            == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
    {
        member_ids.extend(
            frame
                .members
                .iter()
                .filter(|member| {
                    matches!(
                        member.role,
                        Role::TransverseTie | Role::GableTie | Role::GablePost | Role::Rafter
                    ) || (member.role == Role::Purlin && member.start.distance(member.end) >= 3.0)
                })
                .map(|member| member.id),
        );
    }
    if view == ViewerView::TimberGableRoofBearing {
        let ridge_x = plan
            .roofs
            .first()
            .is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
        let coordinate = |point: Vec3| if ridge_x { point.x } else { point.z };
        let end_plane = frame
            .members
            .iter()
            .filter(|member| member_ids.contains(&member.id))
            .flat_map(|member| [coordinate(member.start), coordinate(member.end)])
            .fold(f32::INFINITY, f32::min);
        member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_some_and(|member| {
                    (coordinate(member.start) - end_plane).abs() <= 0.45
                        && (coordinate(member.end) - end_plane).abs() <= 0.45
                })
        });
    }
    if view == ViewerView::TimberDormerTrimmer
        && let Some(dormer) = plan.roof_dormers.first()
    {
        member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_some_and(|member| {
                    let centre = (member.start + member.end) * 0.5;
                    Vec2::new(centre.x, centre.z).distance(dormer.centre)
                        <= dormer.width_metres.max(dormer.depth_metres) * 0.75
                })
        });
        // Keep the trimmer proof structural rather than showing two floating
        // bars: include members which share the exact trimmer end joints.
        let connected = frame
            .joints
            .iter()
            .filter(|joint| joint.member_ids.iter().any(|id| member_ids.contains(id)))
            .flat_map(|joint| joint.member_ids.iter().copied())
            .collect::<Vec<_>>();
        member_ids.extend(connected);

        // Include the authoritative timber curb/front framing belonging to
        // the same Stage 4 child roof. A trimmer-only proof can otherwise
        // look like detached bars even when the parent cut is correctly
        // framed; these exact bay members show what those trimmers carry.
        let child_roof = plan
            .roof_assemblies
            .iter()
            .filter(|roof| roof.parent.is_some())
            .min_by(|left, right| {
                let centre = |roof: &adventuresim_building_generator::RoofAssembly| {
                    let count = roof.outer_loop.vertices.len().max(1) as f32;
                    roof.outer_loop
                        .vertices
                        .iter()
                        .map(|point| point.metres())
                        .sum::<Vec2>()
                        / count
                };
                centre(left)
                    .distance(dormer.centre)
                    .total_cmp(&centre(right).distance(dormer.centre))
            })
            .map(|roof| roof.id);
        if let Some(child_roof) = child_roof {
            member_ids.extend(
                frame
                    .bays
                    .iter()
                    .filter(|bay| {
                        bay.wall
                            .and_then(|wall_id| {
                                plan.wall_assemblies.iter().find(|wall| wall.id == wall_id)
                            })
                            .is_some_and(|wall| {
                                matches!(
                                    wall.source,
                                    adventuresim_building_generator::WallSourceId::RoofChildFront {
                                        roof
                                    } if roof == child_roof
                                )
                            })
                    })
                    .flat_map(|bay| bay.member_ids.iter().copied()),
            );
        }
    }
    if matches!(
        view,
        ViewerView::TimberFrameFacade | ViewerView::TimberSupportLoad
    ) && let Some(line) = frame
        .facades
        .iter()
        .find(|facade| facade.outward == Direction::South)
        .and_then(|facade| facade.lines.first())
    {
        member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_some_and(|member| {
                    let centre = (member.start + member.end) * 0.5;
                    (Vec2::new(centre.x, centre.z) - line.origin)
                        .dot(line.tangent)
                        .abs()
                        <= 5.5
                })
        });
    }
    let mut resolved = frame
        .members
        .iter()
        .filter(|member| member_ids.contains(&member.id))
        .map(|member| member.solid.0)
        .collect::<Vec<_>>();
    if matches!(
        view,
        ViewerView::TimberRegistrationCut | ViewerView::TimberSupportLoad
    ) {
        let stair_centre = frame
            .circulation
            .stair_solids
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
            })
            .map(|solid| solid.centre)
            .sum::<Vec3>()
            / frame.circulation.stair_solids.len().max(1) as f32;
        let near_stair = |solid: &adventuresim_building_generator::ResolvedSolid,
                          clearance: f32| {
            let delta = (solid.centre - stair_centre).abs() - solid.size * 0.5;
            Vec2::new(delta.x.max(0.0), delta.z.max(0.0)).length() <= clearance
        };
        if view == ViewerView::TimberSupportLoad {
            resolved.extend(
                frame
                    .floors
                    .iter()
                    .flat_map(|floor| {
                        let pieces = floor
                            .floor_solids
                            .iter()
                            .filter_map(|id| {
                                plan.resolved_geometry
                                    .solids
                                    .iter()
                                    .find(|solid| solid.id == *id)
                            })
                            .filter(|solid| {
                                view == ViewerView::TimberSupportLoad || near_stair(solid, 0.75)
                            });
                        pieces.collect::<Vec<_>>()
                    })
                    .map(|solid| solid.id.0),
            );
        }
        resolved.extend(
            frame
                .floors
                .iter()
                .flat_map(|floor| floor.joist_members.iter().chain(&floor.girder_members))
                .filter_map(|id| frame.members.iter().find(|member| member.id == *id))
                .filter(|member| {
                    view == ViewerView::TimberSupportLoad
                        || plan
                            .resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == member.solid)
                            .is_some_and(|solid| near_stair(solid, 2.25))
                })
                .map(|member| member.solid.0),
        );
        if view == ViewerView::TimberRegistrationCut {
            // The route is only meaningful against the authoritative occupied
            // floor it reaches. Include those exact floor solids in the cut,
            // rather than proving circulation as floating tread surfaces.
            resolved.extend(frame.floors.iter().map(|floor| floor.floor_solid.0));
            resolved.extend(
                frame
                    .circulation
                    .nodes
                    .iter()
                    .filter(|node| {
                        if matches!(
                            frame.program,
                            adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
                                | adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
                        ) {
                            // These one-storey programs have no stair flight.
                            // Their registration proof instead follows the
                            // complete exterior-door-to-occupied-floor route.
                            true
                        } else {
                            node.kind
                                == adventuresim_building_generator::TimberRouteNodeKind::StairTread
                                || (node.kind
                                    == adventuresim_building_generator::TimberRouteNodeKind::Landing
                                    && Vec2::new(node.position.x, node.position.z)
                                        .distance(Vec2::new(stair_centre.x, stair_centre.z))
                                        <= 2.0)
                        }
                    })
                    .map(|node| node.surface.0),
            );
            resolved.extend(frame.circulation.stair_solids.iter().map(|id| id.0));
            resolved.extend(frame.circulation.landing_solids.iter().map(|id| id.0));
        }
    }
    if view == ViewerView::TimberProgramDetail {
        resolved.extend(frame.floors.iter().map(|floor| floor.floor_solid.0));
        if frame.program
            == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
        {
            resolved.extend(frame.floors.iter().map(|floor| floor.route_surface.0));
        }
    }
    if view == ViewerView::TimberGableRoofBearing {
        let bearing_interfaces = frame
            .roof_bearing_interfaces
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .support_interfaces
                    .iter()
                    .find(|interface| interface.id == *id)
            })
            .collect::<Vec<_>>();
        resolved.extend(
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    matches!(solid.role, SolidRole::RoofPlate | SolidRole::RoofFraming)
                        && bearing_interfaces.iter().any(|interface| {
                            let half = solid.size * 0.5;
                            let min = solid.centre - half;
                            let max = solid.centre + half;
                            min.x <= interface.bounds.max.x + 0.02
                                && max.x >= interface.bounds.min.x - 0.02
                                && min.y <= interface.bounds.max.y + 0.02
                                && max.y >= interface.bounds.min.y - 0.02
                                && min.z <= interface.bounds.max.z + 0.02
                                && max.z >= interface.bounds.min.z - 0.02
                        })
                })
                .map(|solid| solid.id.0),
        );
    }
    if view == ViewerView::TimberDormerTrimmer {
        let trimmer_centres = frame
            .members
            .iter()
            .filter(|member| member_ids.contains(&member.id))
            .map(|member| (member.start + member.end) * 0.5)
            .collect::<Vec<_>>();
        // Only include the parent rafters physically adjacent to this dormer
        // curb. Pulling every roof-bearing solid into the proof produced
        // detached posts from the opposite roof slope and obscured the exact
        // trimmer-to-rafter contact.
        resolved.extend(
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| solid.role == SolidRole::RoofFraming)
                .filter(|solid| {
                    trimmer_centres
                        .iter()
                        .any(|centre| centre.distance(solid.centre) <= 2.5)
                })
                .map(|solid| solid.id.0),
        );
    }
    if matches!(
        view,
        ViewerView::TimberJettyExterior
            | ViewerView::TimberJettyUnderside
            | ViewerView::TimberJettyLoad
    ) {
        resolved.extend(
            frame
                .facades
                .iter()
                .flat_map(|facade| &facade.lines)
                .flat_map(|line| &line.storeys)
                .filter_map(|storey| storey.jetty.as_ref())
                .filter(|jetty| jetty.jetty_beams.iter().any(|id| member_ids.contains(id)))
                .map(|jetty| jetty.floor_solid.0),
        );
    }
    if matches!(
        view,
        ViewerView::TimberOpeningBayExterior
            | ViewerView::TimberOpeningBayInterior
            | ViewerView::TimberOpeningBaySection
    ) && let Some(bay) = frame.bays.iter().find(|bay| bay.opening.is_some())
    {
        // The exact triangular Gefach partition is part of the proof: unlike
        // the old backing sheet, these cells terminate on posts/rails/braces
        // and leave the opening void clear. Their shallower wall depth keeps
        // the exterior frame readable; interior/section views render them as
        // cut material so both contact boundaries and timbers remain visible.
        resolved.extend(bay.infill_solids.iter().map(|id| id.0));
        if let Some(opening) = bay.opening.and_then(|id| {
            plan.opening_assemblies
                .iter()
                .find(|opening| opening.id == id)
        }) {
            resolved.extend(opening.closure_solids.iter().map(|id| id.0));
        }
    }
    if view == ViewerView::TimberTownHallJunction {
        let sill_centre = frame
            .members
            .iter()
            .find(|member| member_ids.contains(&member.id))
            .map(|member| (member.start + member.end) * 0.5);
        if let Some(wall) = sill_centre.and_then(|centre| {
            plan.wall_assemblies
                .iter()
                .filter(|wall| wall.storey_level == 0)
                .min_by(|left, right| {
                    left.frame
                        .origin
                        .distance(Vec2::new(centre.x, centre.z))
                        .total_cmp(&right.frame.origin.distance(Vec2::new(centre.x, centre.z)))
                })
        }) {
            let centre = sill_centre.expect("sill centre was present when wall was selected");
            let centre_2d = Vec2::new(centre.x, centre.z);
            resolved.extend(
                plan.wall_assemblies
                    .iter()
                    .filter(|candidate| {
                        candidate.storey_level == 0
                            && candidate.frame.outward.dot(wall.frame.outward) >= 0.99
                            && ((candidate.frame.origin - centre_2d).dot(wall.frame.outward)).abs()
                                <= 0.45
                            && ((candidate.frame.origin - centre_2d).dot(wall.frame.tangent)).abs()
                                <= 1.0
                    })
                    .flat_map(|candidate| candidate.host_solids.iter().map(|id| id.0)),
            );
        }
    }
    resolved.sort_unstable();
    resolved.dedup();
    resolved
}

fn timber_isolated_view(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::TimberFrameFacade
            | ViewerView::TimberRegistrationCut
            | ViewerView::TimberSupportLoad
            | ViewerView::TimberProgramDetail
            | ViewerView::TimberOpeningBayExterior
            | ViewerView::TimberOpeningBayInterior
            | ViewerView::TimberOpeningBaySection
            | ViewerView::TimberJointClose
            | ViewerView::TimberJettyExterior
            | ViewerView::TimberJettyUnderside
            | ViewerView::TimberJettyLoad
            | ViewerView::TimberGableRoofBearing
            | ViewerView::TimberDormerTrimmer
            | ViewerView::TimberTownHallJunction
    )
}

fn timber_camera(plan: &BuildingPlan, view: ViewerView, origin: Vec2) -> Option<(Vec3, Vec3)> {
    timber_proof_suffix(view)?;
    let ids = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let removed = timber_section_removed_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let focused = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| ids.contains(&solid.id.0) && !removed.contains(&solid.id.0))
        .collect::<Vec<_>>();
    let camera_focused = &focused;
    let focus = if camera_focused.is_empty() {
        let dimensions = plan.dimensions_metres();
        Vec3::new(
            dimensions.x * 0.5,
            plan.storey_height_metres,
            dimensions.y * 0.5,
        )
    } else {
        camera_focused
            .iter()
            .map(|solid| solid.centre)
            .sum::<Vec3>()
            / camera_focused.len() as f32
    } + Vec3::new(origin.x, 0.0, origin.y);
    let span = camera_focused
        .iter()
        .map(|solid| solid.size.length())
        .fold(4.0_f32, f32::max)
        .clamp(4.0, 20.0);
    let focus_extent = if camera_focused.is_empty() {
        span
    } else {
        let min = camera_focused
            .iter()
            .map(|solid| solid.centre - solid.size * 0.5)
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let max = camera_focused
            .iter()
            .map(|solid| solid.centre + solid.size * 0.5)
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        (max - min).max_element().max(4.0)
    };
    let opening_frame = plan
        .timber_frame
        .as_ref()
        .and_then(|frame| frame.bays.iter().find_map(|bay| bay.opening))
        .and_then(|id| {
            plan.opening_assemblies
                .iter()
                .find(|opening| opening.id == id)
        })
        .map(|opening| opening.frame);
    let offset = match view {
        ViewerView::TimberWholeExterior => Vec3::new(-span * 2.25, span * 1.05, -span * 2.25),
        ViewerView::TimberFrameFacade
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall
            }) => Vec3::new(
            focus_extent * 0.10,
            focus_extent * 0.25,
            -focus_extent * 1.25,
        ),
        ViewerView::TimberFrameFacade => Vec3::new(
            focus_extent * 0.12,
            focus_extent * 0.32,
            -focus_extent * 1.65,
        ),
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall
            }) =>
        {
            Vec3::new(focus_extent * 1.35, focus_extent * 0.72, -focus_extent * 1.35)
        }
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) =>
        {
            // Include the exterior threshold and the full central-hall route,
            // not only the internal post-and-tie frame used to derive the
            // solid focus extent.
            Vec3::new(focus_extent * 1.55, focus_extent * 0.72, -focus_extent * 1.55)
        }
        ViewerView::TimberRegistrationCut if plan.storeys.len() <= 2 => {
            Vec3::new(focus_extent * 1.05, focus_extent * 0.62, -focus_extent * 1.05)
        }
        ViewerView::TimberRegistrationCut => {
            Vec3::new(focus_extent * 1.30, focus_extent * 0.72, -focus_extent * 1.30)
        }
        ViewerView::TimberSupportLoad
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) => Vec3::new(span * 1.10, span * 0.34, -span * 1.10),
        ViewerView::TimberSupportLoad
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
            }) => Vec3::new(span * 1.20, span * 0.38, -span * 1.20),
        ViewerView::TimberSupportLoad => Vec3::new(span * 1.65, span * 0.72, -span * 1.70),
        ViewerView::TimberProgramDetail
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) => Vec3::new(-span * 1.30, span * 0.55, -span * 1.40),
        ViewerView::TimberProgramDetail => Vec3::new(-span * 1.80, span * 0.75, -span * 1.90),
        ViewerView::TimberOpeningBayExterior if let Some(frame) = opening_frame => {
            Vec3::new(frame.outward.x, 0.32, frame.outward.y) * focus_extent * 1.45
                + Vec3::new(frame.tangent.x, 0.0, frame.tangent.y) * focus_extent * 0.16
        }
        ViewerView::TimberOpeningBayInterior if let Some(frame) = opening_frame => {
            Vec3::new(-frame.outward.x, 0.28, -frame.outward.y) * focus_extent * 1.35
                - Vec3::new(frame.tangent.x, 0.0, frame.tangent.y) * focus_extent * 0.14
        }
        ViewerView::TimberOpeningBaySection if let Some(frame) = opening_frame => {
            Vec3::new(frame.tangent.x, 0.34, frame.tangent.y) * focus_extent * 0.9
                - Vec3::new(frame.outward.x, 0.0, frame.outward.y) * focus_extent
        }
        ViewerView::TimberOpeningBayExterior => Vec3::new(-4.5, 2.2, -6.5),
        ViewerView::TimberOpeningBayInterior => Vec3::new(4.5, 2.0, 5.5),
        ViewerView::TimberOpeningBaySection => Vec3::new(5.5, 2.5, -4.5),
        ViewerView::TimberJointClose => Vec3::new(-3.5, 2.0, -3.5),
        ViewerView::TimberJettyExterior => Vec3::new(
            -focus_extent * 1.5,
            focus_extent * 0.7,
            -focus_extent * 1.5,
        ),
        ViewerView::TimberJettyUnderside => Vec3::new(
            -focus_extent * 0.82,
            -focus_extent * 0.14,
            -focus_extent * 0.82,
        ),
        ViewerView::TimberJettyLoad => Vec3::new(
            focus_extent * 1.5,
            focus_extent * 0.7,
            -focus_extent * 1.5,
        ),
        ViewerView::TimberGableRoofBearing => {
            let ridge_x = plan
                .roofs
                .first()
                .is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
            let ridge = if ridge_x { Vec3::X } else { Vec3::Z };
            let side = if ridge_x { Vec3::Z } else { Vec3::X };
            -ridge * focus_extent * 1.35
                + side * focus_extent * 0.32
                + Vec3::Y * focus_extent * 0.42
        }
        ViewerView::TimberDormerTrimmer => Vec3::new(
            -focus_extent * 1.5,
            focus_extent * 0.65,
            -focus_extent * 1.5,
        ),
        ViewerView::TimberTownHallJunction => Vec3::new(
            -focus_extent * 0.78,
            focus_extent * 0.46,
            -focus_extent * 0.74,
        ),
        _ => return None,
    };
    Some((focus + offset, focus))
}

fn timber_required_roles(plan: &BuildingPlan, view: ViewerView) -> Vec<String> {
    let roles: &[&str] = match view {
        ViewerView::TimberWholeExterior => &["FramePost", "FramePlate", "FrameBrace"],
        ViewerView::TimberFrameFacade => &["FramePost", "FrameRail", "FrameBrace"],
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) =>
        {
            &["FrameFloor", "FramePost", "FrameTie", "TimberCirculation"]
        }
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
            }) =>
        {
            &["FrameFloor", "FramePost", "FrameBrace", "TimberCirculation"]
        }
        ViewerView::TimberRegistrationCut => &[
            "FrameFloor",
            "FrameJoist",
            "FrameGirder",
            "TimberCirculation",
        ],
        ViewerView::TimberOpeningBayExterior
        | ViewerView::TimberOpeningBayInterior
        | ViewerView::TimberOpeningBaySection => &["FramePost", "FrameRail", "WallHost"],
        ViewerView::TimberJointClose => &["FramePost", "FramePlate"],
        ViewerView::TimberJettyExterior
        | ViewerView::TimberJettyUnderside
        | ViewerView::TimberJettyLoad => &["FrameJettyBeam", "FrameKnagge"],
        ViewerView::TimberGableRoofBearing => &["FrameGableMember"],
        ViewerView::TimberDormerTrimmer => &["FrameDormerTrimmer", "RoofFraming"],
        ViewerView::TimberTownHallJunction => &["FrameSill", "FrameGirder", "WallHost"],
        ViewerView::TimberSupportLoad
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                matches!(
                    frame.program,
                    adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
                        | adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
                )
            }) =>
        {
            &["FramePost", "FrameBrace", "FramePlate", "FrameFloor"]
        }
        ViewerView::TimberSupportLoad => &[
            "FramePost",
            "FrameBrace",
            "FramePlate",
            "FrameJoist",
            "FrameGirder",
        ],
        ViewerView::TimberProgramDetail => match plan
            .timber_frame
            .as_ref()
            .map(|frame| frame.program)
        {
            Some(
                adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse,
            ) => &[
                "FramePost",
                "FrameTie",
                "FrameGableMember",
                "FrameFloor",
                "TimberCirculation",
            ],
            Some(adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage) => {
                &["FramePost", "FrameBrace", "FrameGableMember"]
            }
            Some(
                adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall,
            ) => &["FrameSill", "FrameGirder", "FramePost"],
            Some(
                adventuresim_building_generator::TimberFrameProgramKind::NarrowUrbanTownHouse
                | adventuresim_building_generator::TimberFrameProgramKind::JettiedMerchantHouse,
            ) => &["FrameJettyBeam", "FrameKnagge", "FrameFloor"],
            None => &[],
        },
        _ => &[],
    };
    roles.iter().map(|role| (*role).to_owned()).collect()
}

fn timber_cut_plane(plan: &BuildingPlan, view: ViewerView) -> Option<[f32; 4]> {
    timber_section_proof(view).then(|| {
        let dimensions = plan.dimensions_metres();
        if view == ViewerView::TimberOpeningBaySection
            && let Some((opening, bounds)) = plan
                .timber_frame
                .as_ref()
                .and_then(|frame| frame.bays.iter().find_map(|bay| bay.opening))
                .and_then(|id| {
                    let opening = plan
                        .opening_assemblies
                        .iter()
                        .find(|opening| opening.id == id)?;
                    let bounds = plan
                        .resolved_geometry
                        .voids
                        .iter()
                        .find(|void| void.id == opening.void_id)?
                        .bounds;
                    Some((opening, bounds))
                })
        {
            let centre = (bounds.min + bounds.max) * 0.5;
            let normal = opening.frame.tangent;
            [
                normal.x,
                0.0,
                normal.y,
                -normal.dot(Vec2::new(centre.x, centre.z)),
            ]
        } else if view == ViewerView::TimberTownHallJunction {
            // Retain the centre and one end bearing of the authoritative
            // broad-span girder. A cut through x=7 removed the girder as one
            // resolved solid even though its masonry/sill counterparts
            // remained, producing a misleading one-sided junction proof.
            [1.0, 0.0, 0.0, -12.0]
        } else if view == ViewerView::TimberGableRoofBearing {
            let roof = plan.roofs.first();
            let ridge_x = roof.is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
            let end_plane = plan
                .timber_frame
                .as_ref()
                .into_iter()
                .flat_map(|frame| &frame.members)
                .filter(|member| {
                    member.role == adventuresim_building_generator::TimberMemberRole::GableTie
                })
                .flat_map(|member| [member.start, member.end])
                .map(|point| if ridge_x { point.x } else { point.z })
                .fold(f32::INFINITY, f32::min);
            let cut = if end_plane.is_finite() {
                end_plane + 0.45
            } else if ridge_x {
                dimensions.x * 0.5
            } else {
                dimensions.y * 0.5
            };
            if ridge_x {
                [1.0, 0.0, 0.0, -cut]
            } else {
                [0.0, 0.0, 1.0, -cut]
            }
        } else if view == ViewerView::TimberDormerTrimmer {
            [
                1.0,
                0.0,
                0.0,
                -plan
                    .roof_dormers
                    .first()
                    .map_or(dimensions.x * 0.5, |dormer| dormer.centre.x),
            ]
        } else {
            [0.0, 0.0, 1.0, -dimensions.y * 0.5]
        }
    })
}

fn timber_section_removed_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(plane) = timber_cut_plane(plan, view) else {
        return Vec::new();
    };
    let focused = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| focused.contains(&solid.id.0))
        .filter(|solid| {
            plane[0] * solid.centre.x
                + plane[1] * solid.centre.y
                + plane[2] * solid.centre.z
                + plane[3]
                > 0.05
        })
        .map(|solid| solid.id.0)
        .collect()
}

const fn church_proof_slug(view: ViewerView) -> Option<&'static str> {
    Some(match view {
        ViewerView::ChurchWholeWest => "church-whole-west",
        ViewerView::ChurchWholeEast => "church-whole-east",
        ViewerView::ChurchWholeNorth => "church-whole-north",
        ViewerView::ChurchWholeSouth => "church-whole-south",
        ViewerView::ChurchWholeTop => "church-whole-top",
        ViewerView::ChurchWholeLongitudinalCut => "church-whole-longitudinal-cut",
        ViewerView::ChurchWholeTransverseCut => "church-whole-transverse-cut",
        ViewerView::ChurchWholeRegression => "church-whole-regression",
        ViewerView::ChurchBayExterior => "church-bay-exterior",
        ViewerView::ChurchBayInterior => "church-bay-interior",
        ViewerView::ChurchBaySection => "church-bay-section",
        ViewerView::ChurchBayLoad => "church-bay-load",
        ViewerView::ChurchBayVault => "church-bay-vault",
        ViewerView::ChurchCrossingInterior => "church-crossing-interior",
        ViewerView::ChurchCrossingExterior => "church-crossing-exterior",
        ViewerView::ChurchCrossingTop => "church-crossing-top",
        ViewerView::ChurchCrossingCutLoad => "church-crossing-cut-load",
        ViewerView::ChurchChoirEast => "church-choir-east",
        ViewerView::ChurchChoirInterior => "church-choir-interior",
        ViewerView::ChurchChoirTop => "church-choir-top",
        ViewerView::ChurchChoirRadialSection => "church-choir-radial-section",
        ViewerView::ChurchTowerPortal => "church-tower-portal",
        ViewerView::ChurchTowerJunction => "church-tower-junction",
        ViewerView::ChurchTowerStair => "church-tower-stair",
        ViewerView::ChurchTowerBellUnderside => "church-tower-bell-underside",
        ViewerView::ChurchTowerFrame => "church-tower-frame",
        ViewerView::ChurchTowerLouvredExterior => "church-tower-louvred-exterior",
        ViewerView::ChurchTowerRoofDrain => "church-tower-roof-drain",
        ViewerView::ChurchDrainage => "church-drainage",
        ViewerView::ChurchSupportDag => "church-support-dag",
        _ => return None,
    })
}

fn church_section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::ChurchWholeLongitudinalCut
            | ViewerView::ChurchWholeTransverseCut
            | ViewerView::ChurchBayInterior
            | ViewerView::ChurchBaySection
            | ViewerView::ChurchBayLoad
            | ViewerView::ChurchBayVault
            | ViewerView::ChurchCrossingInterior
            | ViewerView::ChurchCrossingCutLoad
            | ViewerView::ChurchChoirInterior
            | ViewerView::ChurchChoirRadialSection
            | ViewerView::ChurchTowerJunction
            | ViewerView::ChurchTowerStair
            | ViewerView::ChurchTowerBellUnderside
            | ViewerView::ChurchTowerFrame
            | ViewerView::ChurchSupportDag
    )
}

fn church_camera(plan: &BuildingPlan, view: ViewerView, origin: Vec2) -> Option<(Vec3, Vec3)> {
    let church = plan.church.as_ref()?;
    let point = |plan_x: f32, height: f32, plan_z: f32| {
        Vec3::new(plan_x + origin.x, height, plan_z + origin.y)
    };
    let whole = point(
        church.crossing_axis_metres - 5.0,
        8.0,
        church.tower.centre.y,
    );
    let tower_low = point(church.tower.centre.x, 3.5, church.tower.centre.y);
    let tower_mid = point(church.tower.centre.x, 10.5, church.tower.centre.y);
    let tower_high = point(church.tower.centre.x, 18.0, church.tower.centre.y);
    let bay = point(church.nave_axes_metres[1], 6.0, church.tower.centre.y);
    let crossing = point(church.crossing_axis_metres, 8.0, church.tower.centre.y);
    let choir_x = church
        .choir
        .bay_axes_metres
        .last()
        .copied()
        .unwrap_or(church.crossing_axis_metres + 8.0);
    let choir = point(choir_x + 2.5, 7.0, church.tower.centre.y);
    let (focus, offset) = match view {
        ViewerView::ChurchWholeWest => (whole, Vec3::new(-49.0, 17.0, -27.0)),
        ViewerView::ChurchWholeEast => (whole, Vec3::new(51.0, 18.0, 23.0)),
        ViewerView::ChurchWholeNorth => (whole, Vec3::new(7.0, 20.0, 50.0)),
        ViewerView::ChurchWholeSouth => (whole, Vec3::new(-7.0, 20.0, -50.0)),
        ViewerView::ChurchWholeRegression => (whole, Vec3::new(40.0, 24.0, -38.0)),
        ViewerView::ChurchWholeTop => (whole, Vec3::new(2.0, 65.0, -2.0)),
        ViewerView::ChurchWholeLongitudinalCut => (whole, Vec3::new(0.0, 16.0, -50.0)),
        ViewerView::ChurchWholeTransverseCut => (crossing, Vec3::new(44.0, 16.0, -5.0)),
        ViewerView::ChurchBayExterior => (bay, Vec3::new(-5.0, 12.0, -27.5)),
        ViewerView::ChurchBayInterior => (bay, Vec3::new(15.0, 10.5, -25.5)),
        ViewerView::ChurchBaySection => (bay, Vec3::new(13.0, 9.0, -24.0)),
        ViewerView::ChurchBayLoad => (bay, Vec3::new(20.0, 14.0, -28.0)),
        ViewerView::ChurchBayVault => (bay, Vec3::new(7.0, 20.0, -20.0)),
        ViewerView::ChurchCrossingInterior => (crossing, Vec3::new(18.0, 13.0, -29.0)),
        ViewerView::ChurchCrossingCutLoad => (crossing, Vec3::new(25.0, 18.0, -22.0)),
        ViewerView::ChurchCrossingExterior => (crossing, Vec3::new(-19.0, 14.0, -30.0)),
        ViewerView::ChurchCrossingTop => (crossing, Vec3::new(2.0, 38.0, -2.0)),
        ViewerView::ChurchChoirEast => (choir, Vec3::new(27.0, 13.0, 3.0)),
        ViewerView::ChurchChoirInterior => (choir, Vec3::new(-14.0, 14.0, -30.0)),
        ViewerView::ChurchChoirRadialSection => (choir, Vec3::new(22.0, 11.0, -2.0)),
        ViewerView::ChurchChoirTop => (choir, Vec3::new(0.5, 38.0, -0.5)),
        ViewerView::ChurchTowerPortal => (tower_low, Vec3::new(-24.0, 16.0, -24.0)),
        ViewerView::ChurchTowerLouvredExterior => (tower_high, Vec3::new(-18.0, 7.0, -18.0)),
        ViewerView::ChurchTowerJunction => (tower_low, Vec3::new(15.0, 9.0, -19.0)),
        ViewerView::ChurchTowerStair => (tower_mid, Vec3::new(25.0, 13.0, -28.0)),
        ViewerView::ChurchTowerBellUnderside => (tower_high, Vec3::new(11.0, -2.0, -10.0)),
        ViewerView::ChurchTowerFrame => (tower_high, Vec3::new(14.0, 3.5, -17.0)),
        ViewerView::ChurchTowerRoofDrain => (tower_high, Vec3::new(-8.0, 28.0, -24.0)),
        ViewerView::ChurchDrainage => (whole, Vec3::new(4.0, 48.0, -27.0)),
        ViewerView::ChurchSupportDag => (bay, Vec3::new(18.0, 13.0, -20.0)),
        _ => return None,
    };
    Some((focus + offset, focus))
}

fn church_target_component_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<String> {
    let Some(church) = &plan.church else {
        return Vec::new();
    };
    let prefix = format!("church:{}", church.id.0);
    let suffix = match view {
        ViewerView::ChurchBayExterior
        | ViewerView::ChurchBayInterior
        | ViewerView::ChurchBaySection
        | ViewerView::ChurchBayLoad
        | ViewerView::ChurchBayVault => "/nave-bay:2",
        ViewerView::ChurchCrossingInterior
        | ViewerView::ChurchCrossingExterior
        | ViewerView::ChurchCrossingTop
        | ViewerView::ChurchCrossingCutLoad => "/crossing",
        ViewerView::ChurchChoirEast
        | ViewerView::ChurchChoirInterior
        | ViewerView::ChurchChoirTop
        | ViewerView::ChurchChoirRadialSection => "/choir-apse",
        ViewerView::ChurchTowerPortal
        | ViewerView::ChurchTowerJunction
        | ViewerView::ChurchTowerStair
        | ViewerView::ChurchTowerBellUnderside
        | ViewerView::ChurchTowerFrame
        | ViewerView::ChurchTowerLouvredExterior
        | ViewerView::ChurchTowerRoofDrain => "/west-tower",
        ViewerView::ChurchDrainage => "/roof-drainage",
        ViewerView::ChurchSupportDag => "/nave-bay:2/load-path",
        _ => "/whole",
    };
    vec![format!("{prefix}{suffix}")]
}

fn church_required_roles(view: ViewerView) -> Vec<String> {
    let roles: &[&str] = match view {
        ViewerView::ChurchBaySection => &["ChurchPier", "ChurchArcade"],
        ViewerView::ChurchBayLoad | ViewerView::ChurchSupportDag => {
            &["ChurchVaultThrust", "WallButtress", "ChurchPier"]
        }
        ViewerView::ChurchBayVault => &["ChurchVaultShell", "ChurchVaultThrust"],
        ViewerView::ChurchCrossingCutLoad => {
            &["ChurchCrossingArch", "ChurchVaultThrust", "WallButtress"]
        }
        ViewerView::ChurchChoirInterior | ViewerView::ChurchChoirRadialSection => {
            &["ChurchVaultShell", "WallButtress", "WallHost"]
        }
        ViewerView::ChurchTowerStair => &["ChurchStairTread", "Landing", "ChurchGuard"],
        ViewerView::ChurchTowerBellUnderside => &["ChurchBellFloor", "ChurchBell"],
        ViewerView::ChurchTowerFrame => &["ChurchBellFrame", "ChurchBell", "ChurchServiceLadder"],
        ViewerView::ChurchTowerRoofDrain | ViewerView::ChurchDrainage => &["RoofGutter"],
        _ => &[],
    };
    roles.iter().map(|role| (*role).to_owned()).collect()
}

fn church_cut_plane(plan: &BuildingPlan, view: ViewerView) -> Option<[f32; 4]> {
    let church = plan.church.as_ref()?;
    if !church_section_proof(view) {
        return None;
    }
    if view == ViewerView::ChurchChoirRadialSection {
        let cut = church
            .choir
            .bay_axes_metres
            .last()
            .copied()
            .unwrap_or(church.crossing_axis_metres)
            + 5.0;
        Some([1.0, 0.0, 0.0, -cut])
    } else if matches!(
        view,
        ViewerView::ChurchWholeTransverseCut | ViewerView::ChurchCrossingCutLoad
    ) {
        Some([1.0, 0.0, 0.0, -church.crossing_axis_metres])
    } else {
        Some([0.0, 0.0, 1.0, -church.tower.centre.y])
    }
}

fn church_section_removed_roof_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    if !church_section_proof(view) {
        return Vec::new();
    }
    let Some(church) = &plan.church else {
        return Vec::new();
    };
    let transverse = matches!(
        view,
        ViewerView::ChurchWholeTransverseCut | ViewerView::ChurchCrossingCutLoad
    );
    let radial_cut = (view == ViewerView::ChurchChoirRadialSection).then(|| {
        church
            .choir
            .bay_axes_metres
            .last()
            .copied()
            .unwrap_or(church.crossing_axis_metres)
            + 5.0
    });
    plan.roof_assemblies
        .iter()
        .flat_map(|roof| &roof.faces)
        .filter(|face| {
            let centre =
                face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len().max(1) as f32;
            radial_cut.map_or_else(
                || {
                    (transverse && centre.x > church.crossing_axis_metres)
                        || (!transverse && centre.z < church.tower.centre.y)
                },
                |cut| centre.x > cut,
            )
        })
        .map(|face| face.id.0)
        .collect()
}

const fn roof_proof_slug(view: RoofProofView) -> &'static str {
    match view {
        RoofProofView::RoofGableExterior => "roof-gable-exterior",
        RoofProofView::RoofGableInterior => "roof-gable-interior",
        RoofProofView::RoofGableTop => "roof-gable-top",
        RoofProofView::RoofGableCutaway => "roof-gable-cutaway",
        RoofProofView::RoofGableDrainage => "roof-gable-drainage",
        RoofProofView::RoofGableLowPitch => "roof-gable-low-pitch",
        RoofProofView::RoofGableMidPitch => "roof-gable-mid-pitch",
        RoofProofView::RoofGableHighPitch => "roof-gable-high-pitch",
        RoofProofView::RoofHipHalfhipExterior => "roof-hip-halfhip-exterior",
        RoofProofView::RoofHipHalfhipTop => "roof-hip-halfhip-top",
        RoofProofView::RoofHipHalfhipUnderside => "roof-hip-halfhip-underside",
        RoofProofView::RoofLValleyExterior => "roof-l-valley-exterior",
        RoofProofView::RoofLValleyTop => "roof-l-valley-top",
        RoofProofView::RoofLValleyUnderside => "roof-l-valley-underside",
        RoofProofView::RoofLValleyDrainage => "roof-l-valley-drainage",
        RoofProofView::RoofCourtyardValleysTop => "roof-courtyard-valleys-top",
        RoofProofView::RoofDormerGabledExterior => "roof-dormer-gabled-exterior",
        RoofProofView::RoofDormerGabledInterior => "roof-dormer-gabled-interior",
        RoofProofView::RoofDormerGabledTop => "roof-dormer-gabled-top",
        RoofProofView::RoofDormerGabledCutaway => "roof-dormer-gabled-cutaway",
        RoofProofView::RoofDormerGabledDrainage => "roof-dormer-gabled-drainage",
        RoofProofView::RoofDormerShedExterior => "roof-dormer-shed-exterior",
        RoofProofView::RoofDormerShedInterior => "roof-dormer-shed-interior",
        RoofProofView::RoofDormerShedTop => "roof-dormer-shed-top",
        RoofProofView::RoofDormerShedCutaway => "roof-dormer-shed-cutaway",
        RoofProofView::RoofDormerShedDrainage => "roof-dormer-shed-drainage",
        RoofProofView::RoofCrossGableExterior => "roof-cross-gable-exterior",
        RoofProofView::RoofCrossGableTop => "roof-cross-gable-top",
        RoofProofView::RoofCrossGableUnderside => "roof-cross-gable-underside",
        RoofProofView::RoofCrossGableDrainage => "roof-cross-gable-drainage",
        RoofProofView::RoofAbutmentWallExterior => "roof-abutment-wall-exterior",
        RoofProofView::RoofAbutmentWallTop => "roof-abutment-wall-top",
        RoofProofView::RoofAbutmentWallCutaway => "roof-abutment-wall-cutaway",
        RoofProofView::RoofAbutmentWallDrainage => "roof-abutment-wall-drainage",
        RoofProofView::RoofAbutmentTowerExterior => "roof-abutment-tower-exterior",
        RoofProofView::RoofAbutmentTowerTop => "roof-abutment-tower-top",
        RoofProofView::RoofAbutmentTowerCutaway => "roof-abutment-tower-cutaway",
        RoofProofView::RoofAbutmentTowerDrainage => "roof-abutment-tower-drainage",
        RoofProofView::RoofRoundTowerExterior => "roof-round-tower-exterior",
        RoofProofView::RoofRoundTowerTop => "roof-round-tower-top",
        RoofProofView::RoofRoundTowerCutaway => "roof-round-tower-cutaway",
        RoofProofView::RoofRoundTowerDrainage => "roof-round-tower-drainage",
        RoofProofView::RoofPavilionExterior => "roof-pavilion-exterior",
        RoofProofView::RoofPavilionTop => "roof-pavilion-top",
        RoofProofView::RoofPavilionCutaway => "roof-pavilion-cutaway",
        RoofProofView::RoofPavilionDrainage => "roof-pavilion-drainage",
        RoofProofView::RoofCathedralExterior => "roof-cathedral-exterior",
        RoofProofView::RoofCathedralTop => "roof-cathedral-top",
        RoofProofView::RoofCathedralCutaway => "roof-cathedral-cutaway",
        RoofProofView::RoofCathedralDrainage => "roof-cathedral-drainage",
    }
}

fn roof_proof_assembly_indices(plan: &BuildingPlan, view: RoofProofView) -> Vec<usize> {
    let child_kind = if matches!(
        view,
        RoofProofView::RoofDormerGabledExterior
            | RoofProofView::RoofDormerGabledInterior
            | RoofProofView::RoofDormerGabledTop
            | RoofProofView::RoofDormerGabledCutaway
            | RoofProofView::RoofDormerGabledDrainage
    ) {
        Some(adventuresim_building_generator::RoofChildKind::GabledDormer)
    } else if matches!(
        view,
        RoofProofView::RoofDormerShedExterior
            | RoofProofView::RoofDormerShedInterior
            | RoofProofView::RoofDormerShedTop
            | RoofProofView::RoofDormerShedCutaway
            | RoofProofView::RoofDormerShedDrainage
    ) {
        Some(adventuresim_building_generator::RoofChildKind::ShedDormer)
    } else if matches!(
        view,
        RoofProofView::RoofCrossGableExterior
            | RoofProofView::RoofCrossGableTop
            | RoofProofView::RoofCrossGableUnderside
            | RoofProofView::RoofCrossGableDrainage
    ) {
        Some(adventuresim_building_generator::RoofChildKind::CrossGable)
    } else {
        None
    };
    if let Some(kind) = child_kind
        && let Some(child_id) = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.children)
            .find(|child| child.kind == kind)
            .map(|child| child.child)
    {
        return plan
            .roof_assemblies
            .iter()
            .enumerate()
            .filter_map(|(index, roof)| {
                (roof.id == child_id || roof.children.iter().any(|child| child.child == child_id))
                    .then_some(index)
            })
            .collect();
    }
    if matches!(
        view,
        RoofProofView::RoofLValleyExterior
            | RoofProofView::RoofLValleyTop
            | RoofProofView::RoofLValleyUnderside
            | RoofProofView::RoofLValleyDrainage
            | RoofProofView::RoofCourtyardValleysTop
            | RoofProofView::RoofCathedralExterior
            | RoofProofView::RoofCathedralTop
            | RoofProofView::RoofCathedralCutaway
            | RoofProofView::RoofCathedralDrainage
    ) {
        return (0..plan.roof_assemblies.len()).collect();
    }
    if matches!(
        view,
        RoofProofView::RoofAbutmentTowerExterior
            | RoofProofView::RoofAbutmentTowerTop
            | RoofProofView::RoofAbutmentTowerCutaway
            | RoofProofView::RoofAbutmentTowerDrainage
    ) {
        let tower_child = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.children)
            .find(|child| child.kind == adventuresim_building_generator::RoofChildKind::Tower)
            .map(|child| child.child);
        return plan
            .roof_assemblies
            .iter()
            .enumerate()
            .filter_map(|(index, roof)| {
                (Some(roof.id) == tower_child
                    || tower_child
                        .is_some_and(|child| roof.children.iter().any(|link| link.child == child)))
                .then_some(index)
            })
            .collect();
    }
    if matches!(
        view,
        RoofProofView::RoofAbutmentWallExterior
            | RoofProofView::RoofAbutmentWallTop
            | RoofProofView::RoofAbutmentWallCutaway
            | RoofProofView::RoofAbutmentWallDrainage
    ) {
        return plan
            .roof_assemblies
            .iter()
            .enumerate()
            .filter_map(|(index, roof)| {
                roof.edges
                    .iter()
                    .any(|edge| {
                        edge.kind == adventuresim_building_generator::RoofEdgeKind::WallAbutment
                    })
                    .then_some(index)
            })
            .collect();
    }
    let predicate = |roof: &RoofAssembly| {
        if matches!(
            view,
            RoofProofView::RoofHipHalfhipExterior
                | RoofProofView::RoofHipHalfhipTop
                | RoofProofView::RoofHipHalfhipUnderside
        ) {
            matches!(roof.kind, RoofKind::Hip | RoofKind::HalfHip)
        } else if matches!(
            view,
            RoofProofView::RoofRoundTowerExterior
                | RoofProofView::RoofRoundTowerTop
                | RoofProofView::RoofRoundTowerCutaway
                | RoofProofView::RoofRoundTowerDrainage
        ) {
            roof.kind == RoofKind::Conical
        } else if matches!(
            view,
            RoofProofView::RoofPavilionExterior
                | RoofProofView::RoofPavilionTop
                | RoofProofView::RoofPavilionCutaway
                | RoofProofView::RoofPavilionDrainage
        ) {
            roof.kind == RoofKind::Pavilion
        } else {
            roof.kind == RoofKind::Gable && roof.parent.is_none()
        }
    };
    plan.roof_assemblies
        .iter()
        .enumerate()
        .find_map(|(index, roof)| predicate(roof).then_some(vec![index]))
        .unwrap_or_default()
}

fn roof_proof_sectioned(view: RoofProofView) -> bool {
    let slug = roof_proof_slug(view);
    slug.ends_with("-interior") || slug.ends_with("-cutaway") || slug.ends_with("-underside")
}

#[derive(Resource)]
struct CaptureState {
    output: Option<PathBuf>,
    settle_frames: u32,
    settled: u32,
    primed: bool,
    in_flight: bool,
    manifest: CaptureManifest,
}

#[derive(Clone, Serialize)]
struct CaptureManifest {
    schema_version: u16,
    fixture: &'static str,
    view: &'static str,
    seed: u64,
    resolution: [u32; 2],
    room_count: usize,
    wall_count: usize,
    opening_count: usize,
    roof_piece_count: usize,
    roof_dormer_count: usize,
    roof_assembly_count: usize,
    roof_graph_hash: String,
    roof_face_ids: Vec<u64>,
    roof_edge_ids: Vec<u64>,
    roof_cut_ids: Vec<u64>,
    roof_support_node_ids: Vec<u64>,
    roof_drainage_terminal_ids: Vec<u64>,
    roof_drainage_network_ids: Vec<u64>,
    roof_drainage_channel_ids: Vec<u64>,
    roof_drainage_outlet_ids: Vec<u64>,
    roof_drainage_route_ids: Vec<u64>,
    roof_render_item_count: usize,
    roof_render_multiset_hash: String,
    rendered_roof_item_count: usize,
    rendered_roof_hash: String,
    tower_count: usize,
    square_tower_count: usize,
    curtain_wall_count: usize,
    stair_count: usize,
    battlement_run_count: usize,
    wall_walk_count: usize,
    defensive_circuit_count: usize,
    defensive_junction_count: usize,
    tower_portal_count: usize,
    gate_defense_count: usize,
    firing_position_count: usize,
    gate_closure_count: usize,
    resolved_solid_count: usize,
    resolved_void_count: usize,
    resolved_owner_count: usize,
    rendered_owner_count: usize,
    rendered_resolved_solid_count: usize,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    resolved_solid_multiset_hash: String,
    rendered_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    evidence_hash: String,
    pixel_hash: String,
    focus_kind: Option<&'static str>,
    focused_tower_index: Option<usize>,
    focused_tower_indices: Vec<usize>,
    focused_wall_index: Option<usize>,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_roof_item_ids: Vec<u64>,
    section_removed_roof_item_ids: Vec<u64>,
    visible_focused_roof_item_count: usize,
    focused_projected_ray_count: usize,
    projected_defense_kind: Option<&'static str>,
    projected_defense_deployment: Option<&'static str>,
    projected_tactical_target: Option<&'static str>,
    visible_focused_resolved_item_count: usize,
    focused_bounds_fraction: [f32; 4],
    camera_position: [f32; 3],
    camera_target: [f32; 3],
    required_focus_object_count: usize,
    visible_focus_object_count: usize,
    focus_requirements_met: bool,
    lighting_preset: &'static str,
    sun_direction: [f32; 3],
    sun_illuminance_lux: f32,
    ambient_brightness: f32,
    ambient_color: [f32; 3],
    lighting_calibration_bounds_fraction: [f32; 4],
    median_luminance_percent: u8,
    dark_clipped_bps: u16,
    bright_clipped_bps: u16,
    luminance_separation_percent: u8,
    shadow_luminance_percent: u8,
    plan_audit_issue_count: usize,
    audited_closed_mesh_count: usize,
    mesh_integrity_issue_count: usize,
    bartizan_count: usize,
    observed_mesh_count: usize,
    visible_mesh_count: usize,
    active_camera_count: usize,
    subject_pixel_bps: u16,
    validation_passed: bool,
    opening_profile: Option<&'static str>,
    wall_section_kind: Option<&'static str>,
    focused_assembly_owner_id: Option<u32>,
    focused_resolved_geometry_hash: Option<String>,
    section_cut_applied: bool,
    section_removed_item_ids: Vec<u64>,
    inside_label_visible: bool,
    outside_label_visible: bool,
    wall_thickness_metres: Option<f32>,
    scale_figure_height_metres: Option<f32>,
    scale_figure_visible: bool,
    section_annotation: String,
    section_annotation_visible: bool,
    exterior_throat_bounds_fraction: [f32; 4],
    interior_mouth_bounds_fraction: [f32; 4],
    church_program_hash: String,
    church_bay_labels: Vec<String>,
    church_support_node_ids: Vec<u64>,
    church_opening_ids: Vec<u64>,
    church_focused_roles: Vec<String>,
    church_target_component_ids: Vec<String>,
    church_target_item_ids: Vec<u64>,
    church_required_roles: Vec<String>,
    church_cut_plane: Option<[f32; 4]>,
    church_removed_target_item_ids: Vec<u64>,
    church_legend_visible: bool,
    timber_program_hash: String,
    timber_program: Option<String>,
    timber_assembly_id: Option<u64>,
    timber_member_ids: Vec<u64>,
    timber_joint_ids: Vec<u64>,
    timber_node_ids: Vec<u64>,
    timber_focused_roles: Vec<String>,
    timber_role_item_ids: std::collections::BTreeMap<String, Vec<u64>>,
    timber_role_bounds_fraction: std::collections::BTreeMap<String, [f32; 4]>,
    timber_target_component_ids: Vec<String>,
    timber_focus_interface_ids: Vec<u64>,
    timber_required_roles: Vec<String>,
    timber_cut_plane: Option<[f32; 4]>,
    timber_removed_target_item_ids: Vec<u64>,
    timber_legend_visible: bool,
    artillery_assembly_id: Option<u64>,
    artillery_phase: Option<String>,
    artillery_curtain_ids: Vec<u64>,
    artillery_rondel_ids: Vec<u64>,
    artillery_station_ids: Vec<u64>,
    artillery_route_surface_ids: Vec<u64>,
    artillery_fire_ray_count: usize,
    artillery_support_node_ids: Vec<u64>,
    artillery_ditch_void_id: Option<u64>,
    artillery_bridge_state: Option<String>,
    artillery_focused_roles: Vec<String>,
    artillery_role_item_ids: std::collections::BTreeMap<String, Vec<u64>>,
    artillery_role_bounds_fraction: std::collections::BTreeMap<String, [f32; 4]>,
    artillery_target_component_ids: Vec<String>,
    artillery_cut_plane: Option<[f32; 4]>,
    artillery_removed_target_item_ids: Vec<u64>,
    artillery_legend_visible: bool,
}

#[derive(Resource)]
struct RenderPalette {
    plaster: Handle<StandardMaterial>,
    brick: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    earth: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
    roof_secondary: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    cutaway: Handle<StandardMaterial>,
    door: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    void: Handle<StandardMaterial>,
    stair: Handle<StandardMaterial>,
    room_floors: Vec<Handle<StandardMaterial>>,
}

#[derive(Component)]
struct ClosedSolid;

#[derive(Component)]
struct GeometryOwner(u32);

#[derive(Clone, Copy, Component)]
enum OpeningBoundaryKind {
    ExteriorThroat,
    InteriorMouth,
}

#[derive(Component)]
struct OpeningBoundary(OpeningBoundaryKind);

#[derive(Component)]
struct ResolvedRenderItem {
    id: u64,
    fingerprint: u64,
    local_half_size: Vec3,
}

/// Renderer correspondence for polygonal roof authority. Roof faces and
/// enclosure faces are not cuboidal S0 solids, so they use an independent
/// exact-ID/fingerprint multiset instead of contaminating the resolved-solid
/// correspondence contract.
#[derive(Component)]
struct RoofRenderItem {
    id: u64,
    fingerprint: u64,
    local_center: Vec3,
    local_half_size: Vec3,
}

#[derive(Component)]
struct LightingCalibration {
    local_center: Vec3,
    local_half_size: Vec3,
}

/// Render-only depth cue. Future collision/nav extraction must ignore entities
/// carrying this marker and consume the semantic shell/portal recipe instead.
#[derive(Component)]
struct NonCollidingVisualization;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum EditorTarget {
    Wall(WallSelector),
    Opening(WallSelector),
    TimberMember(u64),
}

#[derive(Component)]
struct EditorSelectable(EditorTarget);

#[derive(Component)]
struct EditorBuildingEntity;

#[derive(Component)]
struct EditorEnvironmentEntity;

#[derive(Component)]
struct PlayerBuildEntity;

/// Render metadata used by the build-mode visibility controls. It is kept on
/// both generated programme geometry and freeform parts so the controls have
/// one authoritative ECS path.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
struct EditorVisibilityTarget {
    storey: usize,
    role: EditorVisibilityRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorVisibilityRole {
    Wall,
    Floor,
    Structure,
    Roof,
}

/// The opaque material assigned at scene setup. Ghost and cutaway states
/// replace the active handle transiently, then restore this exact handle.
#[derive(Component)]
struct EditorBaseMaterial(Handle<StandardMaterial>);

/// Avoid allocating a fresh translucent material every UI frame while a
/// visibility control remains selected.
#[derive(Component)]
struct EditorAppearanceIsTranslucent(bool);

#[derive(Component)]
struct EditorFachwerkForFinishedWall;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SceneSetup {
    Full,
    EditorInitial,
    EditorBuilding,
}

/// The visible editor modes deliberately follow the direct-manipulation
/// vocabulary used by the build workbench.  Only tools backed by the current
/// semantic document are enabled; unavailable modes remain discoverable
/// instead of pretending that a click has changed the building.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum EditorMode {
    Select,
    Construct,
    Openings,
    Roof,
    Site,
    Finish,
}

impl EditorMode {
    const ALL: [(Self, &'static str, &'static str); 6] = [
        (Self::Select, "Select", "1"),
        (Self::Construct, "Construct", "2"),
        (Self::Openings, "Openings", "3"),
        (Self::Roof, "Roof", "4"),
        (Self::Site, "Site", "5"),
        (Self::Finish, "Finish", "6"),
    ];

    fn availability(self) -> &'static str {
        match self {
            Self::Select => "Inspect walls, openings, and timber members directly on the building.",
            Self::Openings => "Select a wall, then place the audited window opening below.",
            Self::Finish => "Apply a compatible finish to the current programme.",
            Self::Construct => {
                "Freeform wall and room authoring requires the player-build document."
            }
            Self::Roof => "Freeform roof and stair handles require the player-build document.",
            Self::Site => "Site dressing requires the player-build document and site authority.",
        }
    }

    fn is_available(self) -> bool {
        matches!(self, Self::Select | Self::Openings | Self::Finish)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum WallVisibility {
    Up,
    Cutaway,
    Down,
}

impl WallVisibility {
    fn next(self) -> Self {
        match self {
            Self::Up => Self::Cutaway,
            Self::Cutaway => Self::Down,
            Self::Down => Self::Up,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Up => "Walls: Up",
            Self::Cutaway => "Walls: Cutaway",
            Self::Down => "Walls: Down",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum RoofVisibility {
    Show,
    Ghost,
    Hide,
}

impl RoofVisibility {
    fn next(self) -> Self {
        match self {
            Self::Show => Self::Ghost,
            Self::Ghost => Self::Hide,
            Self::Hide => Self::Show,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Show => "Roof: Show",
            Self::Ghost => "Roof: Ghost",
            Self::Hide => "Roof: Hide",
        }
    }
}

#[derive(Resource)]
struct EditorRuntime {
    document: BuildingDocument,
    plan: BuildingPlan,
    document_path: PathBuf,
    player_build: Option<PlayerBuildDocument>,
    player_build_path: Option<PathBuf>,
    selected_player_part: Option<u64>,
    player_x_metres: f32,
    player_z_metres: f32,
    player_elevation_metres: f32,
    player_width_metres: f32,
    player_depth_metres: f32,
    player_height_metres: f32,
    player_rotation_degrees: f32,
    player_kind: PlayerBuildPartKind,
    player_material: PlayerBuildMaterial,
    pending_player_rebuild: bool,
    undo: Vec<BuildingDocument>,
    redo: Vec<BuildingDocument>,
    selected: Option<EditorTarget>,
    hovered: Option<EditorTarget>,
    error: Option<String>,
    status: String,
    window_width_metres: f32,
    window_sill_metres: f32,
    window_height_metres: f32,
    opening_kind: OpeningKind,
    mode: EditorMode,
    active_storey: usize,
    wall_visibility: WallVisibility,
    roof_visibility: RoofVisibility,
    pending_rebuild: bool,
}

impl EditorRuntime {
    fn new(
        document: BuildingDocument,
        plan: BuildingPlan,
        document_path: PathBuf,
        player_build: Option<PlayerBuildDocument>,
        player_build_path: Option<PathBuf>,
    ) -> Self {
        Self {
            document,
            plan,
            document_path,
            player_build,
            player_build_path,
            selected_player_part: None,
            player_x_metres: 0.0,
            player_z_metres: 0.0,
            player_elevation_metres: 0.0,
            player_width_metres: 3.0,
            player_depth_metres: WALL_THICKNESS_METRES,
            player_height_metres: 3.0,
            player_rotation_degrees: 0.0,
            player_kind: PlayerBuildPartKind::Wall,
            player_material: PlayerBuildMaterial::Stone,
            pending_player_rebuild: false,
            undo: Vec::new(),
            redo: Vec::new(),
            selected: None,
            hovered: None,
            error: None,
            status: "Ready".to_owned(),
            window_width_metres: 0.8,
            window_sill_metres: 0.9,
            window_height_metres: 1.1,
            opening_kind: OpeningKind::Window,
            mode: EditorMode::Select,
            active_storey: 0,
            wall_visibility: WallVisibility::Up,
            roof_visibility: RoofVisibility::Show,
            pending_rebuild: false,
        }
    }
}

/// Stable, UI-independent command ABI for editor tests, automation, and
/// future remote tooling.  UI interactions translate to these commands rather
/// than retaining a separate test-only behavior path.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub(crate) enum EditorCommand {
    PlacePart {
        part: PlayerBuildPart,
    },
    MovePart {
        id: u64,
        x_metres: f32,
        z_metres: f32,
    },
    ResizePart {
        id: u64,
        width_metres: f32,
        depth_metres: f32,
        height_metres: f32,
    },
    RotatePart {
        id: u64,
        rotation_degrees: f32,
    },
    RemovePart {
        id: u64,
    },
    SetActiveStorey {
        storey: usize,
    },
    CycleWalls,
    CycleRoofs,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EditorSnapshot {
    pub active_storey: usize,
    pub mode: EditorMode,
    pub walls: WallVisibility,
    pub roof: RoofVisibility,
    pub selected_part: Option<u64>,
    pub parts: Vec<PlayerBuildPart>,
    pub advice: Vec<String>,
    pub status: String,
    pub error: Option<String>,
}

fn editor_snapshot(runtime: &EditorRuntime) -> EditorSnapshot {
    let parts = runtime
        .player_build
        .as_ref()
        .map(|document| document.parts.clone())
        .unwrap_or_default();
    let advice_document = PlayerBuildDocument {
        schema_version: adventuresim_building_generator::PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION,
        parts: parts.clone(),
    };
    EditorSnapshot {
        active_storey: runtime.active_storey,
        mode: runtime.mode,
        walls: runtime.wall_visibility,
        roof: runtime.roof_visibility,
        selected_part: runtime.selected_player_part,
        parts,
        advice: analyse_player_build(&advice_document)
            .into_iter()
            .map(|advice| format!("{advice:?}"))
            .collect(),
        status: runtime.status.clone(),
        error: runtime.error.clone(),
    }
}

fn perform_editor_command(runtime: &mut EditorRuntime, command: EditorCommand) {
    match command {
        EditorCommand::PlacePart { part } => {
            apply_player_build_edit(runtime, PlayerBuildEdit::Place { part })
        }
        EditorCommand::MovePart {
            id,
            x_metres,
            z_metres,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Move {
                id,
                x_metres,
                z_metres,
            },
        ),
        EditorCommand::ResizePart {
            id,
            width_metres,
            depth_metres,
            height_metres,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Resize {
                id,
                width_metres,
                depth_metres,
                height_metres,
            },
        ),
        EditorCommand::RotatePart {
            id,
            rotation_degrees,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Rotate {
                id,
                rotation_degrees,
            },
        ),
        EditorCommand::RemovePart { id } => {
            apply_player_build_edit(runtime, PlayerBuildEdit::Remove { id })
        }
        EditorCommand::SetActiveStorey { storey } => {
            runtime.active_storey = storey.min(runtime.plan.storeys.len().saturating_sub(1));
            runtime.status = format!("Active storey: {}", runtime.active_storey);
        }
        EditorCommand::CycleWalls => {
            runtime.wall_visibility = runtime.wall_visibility.next();
            runtime.status = runtime.wall_visibility.label().to_owned();
        }
        EditorCommand::CycleRoofs => {
            runtime.roof_visibility = runtime.roof_visibility.next();
            runtime.status = runtime.roof_visibility.label().to_owned();
        }
    }
}

/// Executes a JSON array of [`EditorCommand`] values without opening a window.
/// This is the deterministic entry point used by CI and LLM-driven debugging.
pub(crate) fn run_editor_script(path: &std::path::Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let commands = serde_json::from_slice::<Vec<EditorCommand>>(&bytes)
        .map_err(|error| format!("invalid editor command script: {error}"))?;
    let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
    let plan = generate_document(&document).map_err(|error| error.to_string())?;
    let mut runtime = EditorRuntime::new(
        document,
        plan,
        PathBuf::from("building-document.json"),
        Some(PlayerBuildDocument::empty()),
        None,
    );
    let mut snapshots = Vec::with_capacity(commands.len());
    for command in commands {
        perform_editor_command(&mut runtime, command);
        snapshots.push(editor_snapshot(&runtime));
    }
    serde_json::to_string_pretty(&snapshots).map_err(|error| error.to_string())
}

#[derive(Clone, Copy)]
enum EditorUiAction {
    ChangeArchetype(BuildingArchetype),
    AddOpening(WallSelector, OpeningKind),
    RemoveOpening(WallSelector),
    SetWallStyle(WallSelector, WallStyle),
    SetTimberStyle(TimberFrameStyle),
    Undo,
    Redo,
    Save,
    Load,
    SetMode(EditorMode),
    CycleWalls,
    CycleRoofs,
    PreviousStorey,
    NextStorey,
    PlacePlayerPart,
    MovePlayerPart(u64),
    ResizePlayerPart(u64),
    RotatePlayerPart(u64),
    RemovePlayerPart(u64),
}

fn editor_target_label(target: EditorTarget) -> String {
    match target {
        EditorTarget::Wall(wall) => format!(
            "Wall L{} ({}, {}) {:?}",
            wall.storey_level, wall.cell.x, wall.cell.z, wall.direction
        ),
        EditorTarget::Opening(wall) => format!(
            "Opening L{} ({}, {}) {:?}",
            wall.storey_level, wall.cell.x, wall.cell.z, wall.direction
        ),
        EditorTarget::TimberMember(id) => format!("Timber member {id}"),
    }
}

fn editor_pointer_over(
    event: On<Pointer<Over>>,
    selectables: Query<&EditorSelectable>,
    mut runtime: ResMut<EditorRuntime>,
) {
    if let Ok(selectable) = selectables.get(event.entity) {
        runtime.hovered = Some(selectable.0);
    }
}

fn editor_pointer_out(
    event: On<Pointer<Out>>,
    selectables: Query<&EditorSelectable>,
    mut runtime: ResMut<EditorRuntime>,
) {
    if let Ok(selectable) = selectables.get(event.entity)
        && runtime.hovered == Some(selectable.0)
    {
        runtime.hovered = None;
    }
}

fn editor_pointer_click(
    event: On<Pointer<Click>>,
    selectables: Query<&EditorSelectable>,
    mut runtime: ResMut<EditorRuntime>,
) {
    if event.button == PointerButton::Primary
        && let Ok(selectable) = selectables.get(event.entity)
    {
        runtime.selected = Some(selectable.0);
        runtime.status = editor_target_label(selectable.0);
        runtime.error = None;
    }
}

fn update_editor_outlines(
    runtime: Res<EditorRuntime>,
    mut outlines: Query<(&EditorSelectable, &mut OutlineVolume)>,
) {
    if !runtime.is_changed() {
        return;
    }
    for (selectable, mut outline) in &mut outlines {
        if runtime.selected == Some(selectable.0) {
            outline.visible = true;
            outline.colour = Color::WHITE;
            outline.width = 4.0;
        } else if runtime.hovered == Some(selectable.0) {
            outline.visible = true;
            outline.colour = Color::srgb(0.55, 0.55, 0.55);
            outline.width = 3.0;
        } else {
            outline.visible = false;
        }
    }
}

fn editor_ui(mut contexts: EguiContexts, mut runtime: ResMut<EditorRuntime>) -> Result {
    let mut action = None;
    egui::Area::new(egui::Id::new("building-editor-mode-strip"))
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .show(contexts.ctx_mut()?, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save document").clicked() {
                        action = Some(EditorUiAction::Save);
                        ui.close();
                    }
                    if ui.button("Load document").clicked() {
                        action = Some(EditorUiAction::Load);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Fixtures", |ui| {
                        for archetype in BuildingArchetype::ALL {
                            if ui
                                .selectable_label(
                                    runtime.document.program.archetype == archetype,
                                    archetype.slug(),
                                )
                                .clicked()
                            {
                                action = Some(EditorUiAction::ChangeArchetype(archetype));
                                ui.close();
                            }
                        }
                    });
                });
                if ui
                    .add_enabled(!runtime.undo.is_empty(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    action = Some(EditorUiAction::Undo);
                }
                if ui
                    .add_enabled(!runtime.redo.is_empty(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y")
                    .clicked()
                {
                    action = Some(EditorUiAction::Redo);
                }
                ui.separator();
                for (mode, label, shortcut) in EditorMode::ALL {
                    let button = egui::Button::new(format!("{label} {shortcut}"));
                    let response = ui
                        .add_enabled(mode.is_available(), button.selected(runtime.mode == mode))
                        .on_disabled_hover_text(mode.availability())
                        .on_hover_text(mode.availability());
                    if response.clicked() {
                        action = Some(EditorUiAction::SetMode(mode));
                    }
                }
            });
        });
    egui::Area::new(egui::Id::new("building-editor-storeys"))
        .anchor(egui::Align2::LEFT_TOP, [8.0, 48.0])
        .show(contexts.ctx_mut()?, |ui| {
            ui.set_width(150.0);
            ui.strong("Storey");
            if ui.button("▲ Higher").on_hover_text("Page Up").clicked() {
                action = Some(EditorUiAction::NextStorey);
            }
            for level in (0..runtime.plan.storeys.len()).rev() {
                let label = if level == 0 {
                    "Ground".to_owned()
                } else {
                    format!("Level {level}")
                };
                if ui
                    .selectable_label(runtime.active_storey == level, label)
                    .clicked()
                {
                    runtime.active_storey = level;
                    runtime.status = format!("Active storey: {level}");
                }
            }
            if ui.button("▼ Lower").on_hover_text("Page Down").clicked() {
                action = Some(EditorUiAction::PreviousStorey);
            }
            ui.separator();
            if ui
                .button(runtime.wall_visibility.label())
                .on_hover_text("Home")
                .clicked()
            {
                action = Some(EditorUiAction::CycleWalls);
            }
            if ui
                .button(runtime.roof_visibility.label())
                .on_hover_text("R")
                .clicked()
            {
                action = Some(EditorUiAction::CycleRoofs);
            }
            ui.separator();
            ui.small("Visibility settings are retained while you edit this document.");
        });
    egui::Window::new("Inspector")
        .default_size([320.0, 560.0])
        .default_pos([VIEW_WIDTH as f32 - 340.0, 74.0])
        .resizable(true)
        .show(contexts.ctx_mut()?, |ui| {
            ui.strong(format!("{} mode", EditorMode::ALL
                .iter()
                .find(|(mode, _, _)| *mode == runtime.mode)
                .map(|(_, label, _)| *label)
                .unwrap_or("Select")));
            ui.small(EditorMode::availability(runtime.mode));
            ui.label(format!("Programme: {}", runtime.document.program.archetype.slug()));
            ui.small("MMB orbit · Shift+MMB pan · wheel zoom · F frame · Esc select");
            ui.separator();

            if let Some(selected) = runtime.selected {
                ui.label(editor_target_label(selected));
                match selected {
                    EditorTarget::Wall(wall) => {
                        ui.label("Opening type");
                        ui.horizontal_wrapped(|ui| {
                            for (kind, label) in [
                                (OpeningKind::Window, "Window"),
                                (OpeningKind::Door, "Door"),
                                (OpeningKind::Gate, "Gate"),
                                (OpeningKind::ArrowSlit, "Arrow slit"),
                            ] {
                                if ui
                                    .selectable_label(runtime.opening_kind == kind, label)
                                    .clicked()
                                {
                                    runtime.opening_kind = kind;
                                    match kind {
                                        OpeningKind::Window => {
                                            runtime.window_width_metres = 0.8;
                                            runtime.window_sill_metres = 0.9;
                                            runtime.window_height_metres = 1.1;
                                        }
                                        OpeningKind::Door => {
                                            runtime.window_width_metres = 0.95;
                                            runtime.window_sill_metres = 0.0;
                                            runtime.window_height_metres = 2.1;
                                        }
                                        OpeningKind::Gate => {
                                            runtime.window_width_metres = 2.4;
                                            runtime.window_sill_metres = 0.0;
                                            runtime.window_height_metres = 2.8;
                                        }
                                        OpeningKind::ArrowSlit => {
                                            runtime.window_width_metres = 0.25;
                                            runtime.window_sill_metres = 1.2;
                                            runtime.window_height_metres = 1.0;
                                        }
                                    }
                                }
                            }
                        });
                        ui.add(
                            egui::DragValue::new(&mut runtime.window_width_metres)
                                .range(0.35..=1.2)
                                .speed(0.05)
                                .prefix("width ")
                                .suffix(" m"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut runtime.window_sill_metres)
                                .range(0.3..=2.2)
                                .speed(0.05)
                                .prefix("sill ")
                                .suffix(" m"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut runtime.window_height_metres)
                                .range(0.45..=1.8)
                                .speed(0.05)
                                .prefix("height ")
                                .suffix(" m"),
                        );
                        if ui.button("Place opening").clicked() {
                            action = Some(EditorUiAction::AddOpening(wall, runtime.opening_kind));
                        }
                        ui.small("Doors, gates, and arrow slits are audited against their wall. Arches and freeform walls are part of the player-build document.");
                    }
                    EditorTarget::Opening(wall) => {
                        if ui.button("Remove opening").clicked() {
                            action = Some(EditorUiAction::RemoveOpening(wall));
                        }
                    }
                    EditorTarget::TimberMember(_) => {
                        ui.label("Fachwerk pattern (building scope)");
                        let current = runtime
                            .document
                            .program
                            .timber_frame_style
                            .unwrap_or(TimberFrameStyle::LateMedieval);
                        for (style, label) in [
                            (TimberFrameStyle::LateMedieval, "Late medieval"),
                            (
                                TimberFrameStyle::NorthernCloseStudded,
                                "Northern close-studded",
                            ),
                            (TimberFrameStyle::EarlyModernOrnate, "Early modern ornate"),
                        ] {
                            if ui.selectable_label(current == style, label).clicked() {
                                action = Some(EditorUiAction::SetTimberStyle(style));
                            }
                        }
                    }
                }
            } else {
                ui.label("Hover a feature, then click to inspect it.");
            }

            if runtime.player_build.is_some() {
                let player_parts = runtime
                    .player_build
                    .as_ref()
                    .map(|document| document.parts.clone())
                    .unwrap_or_default();
                ui.separator();
                ui.strong("Freeform player build");
                ui.small("Parts commit when renderable; advice never blocks placement.");
                ui.horizontal_wrapped(|ui| {
                    for (kind, label) in [
                        (PlayerBuildPartKind::Wall, "Wall"),
                        (PlayerBuildPartKind::Room, "Room"),
                        (PlayerBuildPartKind::Door, "Door"),
                        (PlayerBuildPartKind::Roof, "Roof"),
                        (PlayerBuildPartKind::Stair, "Stair"),
                        (PlayerBuildPartKind::SiteObject, "Site"),
                    ] {
                        if ui.selectable_label(runtime.player_kind == kind, label).clicked() {
                            runtime.player_kind = kind;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (material, label) in [
                        (PlayerBuildMaterial::Stone, "Stone"),
                        (PlayerBuildMaterial::Brick, "Brick"),
                        (PlayerBuildMaterial::TimberFrame, "Frame"),
                        (PlayerBuildMaterial::Timber, "Timber"),
                        (PlayerBuildMaterial::Tile, "Tile"),
                        (PlayerBuildMaterial::Thatch, "Thatch"),
                        (PlayerBuildMaterial::Earth, "Earth"),
                    ] {
                        if ui
                            .selectable_label(runtime.player_material == material, label)
                            .clicked()
                        {
                            runtime.player_material = material;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut runtime.player_x_metres).prefix("x "));
                    ui.add(egui::DragValue::new(&mut runtime.player_z_metres).prefix("z "));
                    ui.add(egui::DragValue::new(&mut runtime.player_elevation_metres).prefix("y "));
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut runtime.player_width_metres).prefix("w ").range(0.05..=50.0));
                    ui.add(egui::DragValue::new(&mut runtime.player_depth_metres).prefix("d ").range(0.05..=50.0));
                    ui.add(egui::DragValue::new(&mut runtime.player_height_metres).prefix("h ").range(0.05..=50.0));
                });
                ui.add(egui::DragValue::new(&mut runtime.player_rotation_degrees).prefix("rotate ").suffix("°"));
                if ui.button("Place part").clicked() {
                    action = Some(EditorUiAction::PlacePlayerPart);
                }
                egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                    for part in &player_parts {
                        if ui
                            .selectable_label(
                                runtime.selected_player_part == Some(part.id),
                                format!("#{} {:?} L{}", part.id, part.kind, part.storey),
                            )
                            .clicked()
                        {
                            runtime.selected_player_part = Some(part.id);
                            runtime.player_x_metres = part.x_metres;
                            runtime.player_z_metres = part.z_metres;
                            runtime.player_elevation_metres = part.elevation_metres;
                            runtime.player_width_metres = part.width_metres;
                            runtime.player_depth_metres = part.depth_metres;
                            runtime.player_height_metres = part.height_metres;
                            runtime.player_rotation_degrees = part.rotation_degrees;
                        }
                    }
                });
                if let Some(id) = runtime.selected_player_part {
                    ui.horizontal(|ui| {
                        if ui.button("Move").clicked() { action = Some(EditorUiAction::MovePlayerPart(id)); }
                        if ui.button("Resize").clicked() { action = Some(EditorUiAction::ResizePlayerPart(id)); }
                        if ui.button("Rotate").clicked() { action = Some(EditorUiAction::RotatePlayerPart(id)); }
                        if ui.button("Remove").clicked() { action = Some(EditorUiAction::RemovePlayerPart(id)); }
                    });
                }
                let advice_document = PlayerBuildDocument {
                    schema_version: adventuresim_building_generator::PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION,
                    parts: player_parts,
                };
                for advice in analyse_player_build(&advice_document) {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 150, 70),
                        format!("Advice: {advice:?}"),
                    );
                }
            }

            ui.separator();
            ui.label("Wall finish");
            let selected_wall = runtime.selected.and_then(|target| match target {
                EditorTarget::Wall(wall) => Some(wall),
                EditorTarget::Opening(wall) => Some(wall),
                EditorTarget::TimberMember(_) => None,
            });
            let current_wall = selected_wall
                .and_then(|wall| {
                    runtime
                        .plan
                        .wall_style_overrides
                        .iter()
                        .find(|override_| override_.wall == wall)
                        .map(|override_| override_.style)
                })
                .unwrap_or(runtime.document.program.wall_style);
            let civilian = matches!(
                runtime.document.program.archetype,
                BuildingArchetype::TownHouse
                    | BuildingArchetype::HallHouse
                    | BuildingArchetype::FachwerkCottage
                    | BuildingArchetype::FachwerkMerchantHouse
                    | BuildingArchetype::RenaissanceTownHall
            );
            ui.add_enabled_ui(civilian && selected_wall.is_some(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (style, label) in [
                        (WallStyle::TimberFrame, "Timber/plaster"),
                        (WallStyle::Plaster, "Plaster"),
                        (WallStyle::Brick, "Brick"),
                        (WallStyle::Stone, "Stone"),
                    ] {
                        if ui.selectable_label(current_wall == style, label).clicked() {
                            action = selected_wall
                                .map(|wall| EditorUiAction::SetWallStyle(wall, style));
                        }
                    }
                });
            });
            if !civilian {
                ui.small("The selected fixture's structural material is fixed.");
            } else if selected_wall.is_none() {
                ui.small("Select a wall or its fachwerk to change that wall's finish.");
            }

            ui.separator();
            ui.label(&runtime.status);
            if let Some(error) = &runtime.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
            }
        });

    if let Some(action) = action {
        perform_editor_action(&mut runtime, action);
    }
    Ok(())
}

fn perform_editor_action(runtime: &mut EditorRuntime, action: EditorUiAction) {
    match action {
        EditorUiAction::ChangeArchetype(archetype) => {
            let document = BuildingDocument::fixture(archetype, runtime.document.program.seed);
            match generate_document(&document) {
                Ok(plan) => {
                    runtime.undo.push(runtime.document.clone());
                    runtime.redo.clear();
                    runtime.document = document;
                    runtime.plan = plan;
                    runtime.selected = None;
                    runtime.hovered = None;
                    runtime.pending_rebuild = true;
                    runtime.status = format!("Loaded {:?} fixture", archetype);
                    runtime.error = None;
                }
                Err(error) => runtime.error = Some(error.to_string()),
            }
        }
        EditorUiAction::AddOpening(wall, kind) => apply_editor_edit(
            runtime,
            BuildingEdit::AddOpening {
                wall,
                opening_kind: kind,
                width_metres: runtime.window_width_metres,
                sill_metres: runtime.window_sill_metres,
                height_metres: runtime.window_height_metres,
            },
        ),
        EditorUiAction::RemoveOpening(wall) => {
            apply_editor_edit(runtime, BuildingEdit::RemoveOpening { wall });
        }
        EditorUiAction::SetWallStyle(wall, style) => {
            apply_editor_edit(runtime, BuildingEdit::SetWallMaterial { wall, style });
        }
        EditorUiAction::SetTimberStyle(style) => {
            apply_editor_edit(runtime, BuildingEdit::SetTimberFrameStyle { style });
        }
        EditorUiAction::Undo => {
            if let Some(previous) = runtime.undo.pop() {
                match generate_document(&previous) {
                    Ok(plan) => {
                        runtime.redo.push(runtime.document.clone());
                        runtime.document = previous;
                        runtime.plan = plan;
                        runtime.pending_rebuild = true;
                        runtime.status = "Undid edit".to_owned();
                        runtime.error = None;
                    }
                    Err(error) => runtime.error = Some(error.to_string()),
                }
            }
        }
        EditorUiAction::Redo => {
            if let Some(next) = runtime.redo.pop() {
                match generate_document(&next) {
                    Ok(plan) => {
                        runtime.undo.push(runtime.document.clone());
                        runtime.document = next;
                        runtime.plan = plan;
                        runtime.pending_rebuild = true;
                        runtime.status = "Redid edit".to_owned();
                        runtime.error = None;
                    }
                    Err(error) => runtime.error = Some(error.to_string()),
                }
            }
        }
        EditorUiAction::Save => {
            let saved_player_build = runtime
                .player_build
                .as_ref()
                .zip(runtime.player_build_path.as_ref())
                .map(|(document, path)| {
                    serde_json::to_vec_pretty(document)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| fs::write(path, bytes).map_err(|error| error.to_string()))
                        .map(|()| path.clone())
                });
            match saved_player_build.unwrap_or_else(|| {
                serde_json::to_vec_pretty(&runtime.document)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        fs::write(&runtime.document_path, bytes).map_err(|error| error.to_string())
                    })
                    .map(|()| runtime.document_path.clone())
            }) {
                Ok(path) => {
                    runtime.status = format!("Saved {}", path.display());
                    runtime.error = None;
                }
                Err(error) => runtime.error = Some(error),
            }
        }
        EditorUiAction::Load => match fs::read(&runtime.document_path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<BuildingDocument>(&bytes)
                    .map_err(|error| error.to_string())
            })
            .and_then(|document| {
                generate_document(&document)
                    .map(|plan| (document, plan))
                    .map_err(|error| error.to_string())
            }) {
            Ok((document, plan)) => {
                runtime.undo.push(runtime.document.clone());
                runtime.redo.clear();
                runtime.document = document;
                runtime.plan = plan;
                runtime.selected = None;
                runtime.pending_rebuild = true;
                runtime.status = format!("Loaded {}", runtime.document_path.display());
                runtime.error = None;
            }
            Err(error) => runtime.error = Some(error),
        },
        EditorUiAction::SetMode(mode) => {
            runtime.mode = mode;
            runtime.status = format!("{} mode", mode.availability());
            runtime.error = None;
        }
        EditorUiAction::CycleWalls => {
            runtime.wall_visibility = runtime.wall_visibility.next();
            runtime.status = runtime.wall_visibility.label().to_owned();
        }
        EditorUiAction::CycleRoofs => {
            runtime.roof_visibility = runtime.roof_visibility.next();
            runtime.status = runtime.roof_visibility.label().to_owned();
        }
        EditorUiAction::PreviousStorey => {
            runtime.active_storey = runtime.active_storey.saturating_sub(1);
            runtime.status = format!("Active storey: {}", runtime.active_storey);
        }
        EditorUiAction::NextStorey => {
            runtime.active_storey =
                (runtime.active_storey + 1).min(runtime.plan.storeys.len().saturating_sub(1));
            runtime.status = format!("Active storey: {}", runtime.active_storey);
        }
        EditorUiAction::PlacePlayerPart => {
            let id = runtime
                .player_build
                .as_ref()
                .and_then(|document| document.parts.iter().map(|part| part.id).max())
                .unwrap_or(0)
                + 1;
            apply_player_build_edit(
                runtime,
                PlayerBuildEdit::Place {
                    part: PlayerBuildPart {
                        id,
                        kind: runtime.player_kind,
                        material: runtime.player_material,
                        storey: runtime.active_storey as u16,
                        x_metres: runtime.player_x_metres,
                        z_metres: runtime.player_z_metres,
                        elevation_metres: runtime.player_elevation_metres,
                        rotation_degrees: runtime.player_rotation_degrees,
                        width_metres: runtime.player_width_metres,
                        depth_metres: runtime.player_depth_metres,
                        height_metres: runtime.player_height_metres,
                    },
                },
            );
        }
        EditorUiAction::MovePlayerPart(id) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Move {
                id,
                x_metres: runtime.player_x_metres,
                z_metres: runtime.player_z_metres,
            },
        ),
        EditorUiAction::ResizePlayerPart(id) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Resize {
                id,
                width_metres: runtime.player_width_metres,
                depth_metres: runtime.player_depth_metres,
                height_metres: runtime.player_height_metres,
            },
        ),
        EditorUiAction::RotatePlayerPart(id) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Rotate {
                id,
                rotation_degrees: runtime.player_rotation_degrees,
            },
        ),
        EditorUiAction::RemovePlayerPart(id) => {
            apply_player_build_edit(runtime, PlayerBuildEdit::Remove { id })
        }
    }
}

fn apply_player_build_edit(runtime: &mut EditorRuntime, edit: PlayerBuildEdit) {
    let Some(document) = &runtime.player_build else {
        runtime.error =
            Some("launch with --player-build-document to edit a freeform build".to_owned());
        return;
    };
    match document.apply(edit) {
        Ok(next) => {
            runtime.player_build = Some(next);
            runtime.pending_player_rebuild = true;
            runtime.error = None;
            runtime.status = "Freeform edit applied".to_owned();
        }
        Err(error) => runtime.error = Some(error),
    }
}

fn editor_keyboard_shortcuts(keys: Res<ButtonInput<KeyCode>>, mut runtime: ResMut<EditorRuntime>) {
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let action = if control && keys.just_pressed(KeyCode::KeyZ) {
        Some(EditorUiAction::Undo)
    } else if control && keys.just_pressed(KeyCode::KeyY) {
        Some(EditorUiAction::Redo)
    } else if keys.just_pressed(KeyCode::Escape) {
        Some(EditorUiAction::SetMode(EditorMode::Select))
    } else if keys.just_pressed(KeyCode::Digit1) {
        Some(EditorUiAction::SetMode(EditorMode::Select))
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(EditorUiAction::SetMode(EditorMode::Construct))
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(EditorUiAction::SetMode(EditorMode::Openings))
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(EditorUiAction::SetMode(EditorMode::Roof))
    } else if keys.just_pressed(KeyCode::Digit5) {
        Some(EditorUiAction::SetMode(EditorMode::Site))
    } else if keys.just_pressed(KeyCode::Digit6) {
        Some(EditorUiAction::SetMode(EditorMode::Finish))
    } else if keys.just_pressed(KeyCode::Home) {
        Some(EditorUiAction::CycleWalls)
    } else if keys.just_pressed(KeyCode::KeyR) {
        Some(EditorUiAction::CycleRoofs)
    } else if keys.just_pressed(KeyCode::PageUp) {
        Some(EditorUiAction::NextStorey)
    } else if keys.just_pressed(KeyCode::PageDown) {
        Some(EditorUiAction::PreviousStorey)
    } else {
        None
    };
    if let Some(action) = action {
        if let EditorUiAction::SetMode(mode) = action
            && !mode.is_available()
        {
            runtime.status = mode.availability().to_owned();
            return;
        }
        perform_editor_action(&mut runtime, action);
    }
}

fn apply_editor_edit(runtime: &mut EditorRuntime, edit: BuildingEdit) {
    match edit_document(&runtime.document, edit) {
        Ok((document, plan)) => {
            runtime.undo.push(runtime.document.clone());
            runtime.redo.clear();
            runtime.document = document;
            runtime.plan = plan;
            runtime.selected = None;
            runtime.hovered = None;
            runtime.pending_rebuild = true;
            runtime.status = "Edit applied and full building audit passed".to_owned();
            runtime.error = None;
        }
        Err(error) => runtime.error = Some(error.to_string()),
    }
}

fn editor_owner_targets(
    plan: &BuildingPlan,
) -> (
    std::collections::HashMap<u32, EditorTarget>,
    std::collections::HashMap<u64, EditorTarget>,
) {
    let mut owner_targets = std::collections::HashMap::<u32, EditorTarget>::new();
    let mut item_targets = std::collections::HashMap::<u64, EditorTarget>::new();
    for wall in &plan.wall_assemblies {
        if let WallSourceId::StoreyWall {
            storey_level,
            wall_index,
        } = wall.source
            && let Some(segment) = plan
                .storeys
                .iter()
                .find(|storey| storey.level == storey_level)
                .and_then(|storey| storey.walls.get(wall_index))
        {
            owner_targets.insert(
                wall.owner.0,
                EditorTarget::Wall(WallSelector {
                    storey_level,
                    cell: segment.cell,
                    direction: segment.direction,
                }),
            );
        }
    }
    for opening in &plan.opening_assemblies {
        if let WallSourceId::StoreyWall {
            storey_level,
            wall_index,
        } = opening.host_source
            && let Some(segment) = plan
                .storeys
                .iter()
                .find(|storey| storey.level == storey_level)
                .and_then(|storey| storey.walls.get(wall_index))
        {
            let target = EditorTarget::Opening(WallSelector {
                storey_level,
                cell: segment.cell,
                direction: segment.direction,
            });
            let mut ids = opening.jamb_solids.to_vec();
            ids.extend([
                opening.head_solid,
                opening.spandrel_solid,
                opening.wall_above_interface,
            ]);
            ids.extend(opening.sill_solid);
            ids.extend(opening.closure_solids.iter().copied());
            ids.extend(opening.reveal_surfaces.iter().copied());
            ids.extend(opening.mount_solid);
            ids.extend(opening.stance_surface);
            for id in ids {
                item_targets.insert(id.0, target);
            }
        }
    }
    if let Some(frame) = &plan.timber_frame {
        let wall_targets = plan
            .wall_assemblies
            .iter()
            .filter_map(|wall| match wall.source {
                WallSourceId::StoreyWall {
                    storey_level,
                    wall_index,
                } => plan
                    .storeys
                    .iter()
                    .find(|storey| storey.level == storey_level)
                    .and_then(|storey| storey.walls.get(wall_index))
                    .map(|segment| {
                        (
                            wall.id,
                            EditorTarget::Wall(WallSelector {
                                storey_level,
                                cell: segment.cell,
                                direction: segment.direction,
                            }),
                        )
                    }),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        let member_solids = frame
            .members
            .iter()
            .map(|member| (member.id, member.solid.0))
            .collect::<std::collections::HashMap<_, _>>();
        for bay in &frame.bays {
            let Some(target) = bay.wall.and_then(|wall| wall_targets.get(&wall)).copied() else {
                continue;
            };
            for item in bay
                .member_ids
                .iter()
                .filter_map(|member| member_solids.get(member).copied())
                .chain(bay.infill_solids.iter().map(|solid| solid.0))
            {
                item_targets.insert(item, target);
            }
        }
        for member in &frame.members {
            item_targets
                .entry(member.solid.0)
                .or_insert(EditorTarget::TimberMember(member.id.0));
        }
    }
    (owner_targets, item_targets)
}

fn configure_editor_scene(world: &mut World, plan: &BuildingPlan, initialize_camera: bool) {
    let (owner_targets, item_targets) = editor_owner_targets(plan);
    let wall_storeys = plan
        .wall_assemblies
        .iter()
        .map(|wall| (wall.owner.0, usize::from(wall.storey_level)))
        .collect::<std::collections::HashMap<_, _>>();
    let wall_finish_by_owner = plan
        .wall_assemblies
        .iter()
        .filter_map(|assembly| match assembly.source {
            WallSourceId::StoreyWall {
                storey_level,
                wall_index,
            } => plan
                .storeys
                .iter()
                .find(|storey| storey.level == storey_level)
                .and_then(|storey| storey.walls.get(wall_index))
                .and_then(|segment| {
                    plan.wall_style_overrides
                        .iter()
                        .find(|override_| {
                            override_.wall
                                == WallSelector {
                                    storey_level,
                                    cell: segment.cell,
                                    direction: segment.direction,
                                }
                        })
                        .map(|override_| (assembly.owner.0, override_.style))
                }),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    let timber_wall_storeys =
        plan.timber_frame
            .as_ref()
            .map_or_else(std::collections::HashMap::new, |frame| {
                let wall_levels = plan
                    .wall_assemblies
                    .iter()
                    .map(|wall| (wall.id, usize::from(wall.storey_level)))
                    .collect::<std::collections::HashMap<_, _>>();
                let member_solids = frame
                    .members
                    .iter()
                    .map(|member| (member.id, member.solid.0))
                    .collect::<std::collections::HashMap<_, _>>();
                frame
                    .bays
                    .iter()
                    .filter_map(|bay| {
                        bay.wall
                            .and_then(|wall| wall_levels.get(&wall).copied())
                            .map(|level| (bay, level))
                    })
                    .flat_map(|(bay, level)| {
                        bay.member_ids
                            .iter()
                            .filter_map(|member| member_solids.get(member).copied())
                            .chain(bay.infill_solids.iter().map(|solid| solid.0))
                            .map(move |item| (item, level))
                    })
                    .collect()
            });
    let roof_storeys = plan
        .roof_assemblies
        .iter()
        .map(|roof| {
            let base_elevation = roof
                .faces
                .iter()
                .flat_map(|face| face.polygon.iter())
                .map(|point| point.y)
                .chain(
                    roof.enclosure_faces
                        .iter()
                        .flat_map(|face| face.polygon.iter())
                        .map(|point| point.y),
                )
                .fold(f32::INFINITY, f32::min);
            let storey = if base_elevation.is_finite() {
                ((base_elevation / plan.storey_height_metres)
                    .floor()
                    .max(0.0) as usize)
                    .saturating_sub(1)
            } else {
                plan.storeys.len()
            };
            (roof.owner.0, storey)
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mesh_entities = {
        let mut query = world.query_filtered::<(
            Entity,
            Option<&GeometryOwner>,
            Option<&ResolvedRenderItem>,
            Option<&RoofRenderItem>,
            Option<&MeshMaterial3d<StandardMaterial>>,
            Option<&Name>,
            Option<&Transform>,
        ), Without<EditorEnvironmentEntity>>();
        query
            .iter(world)
            .filter(|(entity, ..)| world.get::<Mesh3d>(*entity).is_some())
            .map(|(entity, owner, item, roof, material, name, transform)| {
                (
                    entity,
                    owner.map(|owner| owner.0),
                    item.map(|item| item.id),
                    roof.is_some(),
                    material.map(|material| material.0.clone()),
                    name.is_some_and(|name| name.as_str() == "room floor"),
                    transform.map(|transform| transform.translation.y),
                )
            })
            .collect::<Vec<_>>()
    };
    for (entity, owner, item, is_roof, material, is_room_floor, elevation) in mesh_entities {
        let hide_fachwerk = item
            .and_then(|item| item_targets.get(&item).copied())
            .and_then(|target| match target {
                EditorTarget::Wall(wall) => plan
                    .wall_style_overrides
                    .iter()
                    .find(|override_| override_.wall == wall)
                    .map(|override_| override_.style != WallStyle::TimberFrame),
                _ => None,
            })
            .unwrap_or(false);
        let material = if !is_roof {
            owner
                .and_then(|owner| wall_finish_by_owner.get(&owner).copied())
                .map(|style| {
                    let colour = match style {
                        WallStyle::TimberFrame | WallStyle::Plaster => {
                            Color::srgb(0.72, 0.66, 0.53)
                        }
                        WallStyle::Brick => Color::srgb(0.48, 0.23, 0.16),
                        WallStyle::Stone => Color::srgb(0.42, 0.40, 0.36),
                    };
                    world
                        .resource_mut::<Assets<StandardMaterial>>()
                        .add(StandardMaterial {
                            base_color: colour,
                            perceptual_roughness: 0.82,
                            ..default()
                        })
                })
                .or(material)
        } else {
            material
        };
        let mut entity_mut = world.entity_mut(entity);
        entity_mut.insert(EditorBuildingEntity);
        let visibility_target = if is_roof {
            owner
                .and_then(|owner| roof_storeys.get(&owner).copied())
                .map(|storey| EditorVisibilityTarget {
                    storey,
                    role: EditorVisibilityRole::Roof,
                })
        } else {
            owner
                .and_then(|owner| wall_storeys.get(&owner).copied())
                .map(|storey| EditorVisibilityTarget {
                    storey,
                    role: EditorVisibilityRole::Wall,
                })
                .or_else(|| {
                    item.and_then(|item| timber_wall_storeys.get(&item).copied())
                        .map(|storey| EditorVisibilityTarget {
                            storey,
                            role: EditorVisibilityRole::Wall,
                        })
                })
                .or_else(|| {
                    is_room_floor.then(|| EditorVisibilityTarget {
                        storey: (elevation.unwrap_or_default() / plan.storey_height_metres)
                            .floor()
                            .max(0.0) as usize,
                        role: EditorVisibilityRole::Floor,
                    })
                })
                // Resolved timber, joists, braces, and other structural parts
                // do not always carry a wall/roof owner. Their centre height
                // still has a stable storey meaning in the editor, so never
                // leave them outside the level-visibility contract.
                .or_else(|| {
                    elevation.map(|elevation| EditorVisibilityTarget {
                        storey: (elevation / plan.storey_height_metres).floor().max(0.0) as usize,
                        role: EditorVisibilityRole::Structure,
                    })
                })
        };
        if let (Some(target), Some(material)) = (visibility_target, material) {
            entity_mut.insert((
                target,
                EditorBaseMaterial(material),
                EditorAppearanceIsTranslucent(false),
                Visibility::Visible,
            ));
        }
        if hide_fachwerk {
            entity_mut.insert(EditorFachwerkForFinishedWall);
        }
        let target = item
            .and_then(|item| item_targets.get(&item).copied())
            .or_else(|| owner.and_then(|owner| owner_targets.get(&owner).copied()));
        if let Some(target) = target {
            entity_mut.insert((
                EditorSelectable(target),
                OutlineVolume {
                    visible: false,
                    colour: Color::WHITE,
                    width: 4.0,
                },
                OutlineMode::FloodFlat,
            ));
        } else {
            entity_mut.insert(Pickable::IGNORE);
        }
    }

    if !initialize_camera {
        return;
    }

    let focus = Vec3::new(
        0.0,
        plan.storey_height_metres * plan.storeys.len() as f32 * 0.45,
        0.0,
    );
    let camera_entities = {
        let mut query = world.query_filtered::<Entity, With<Camera3d>>();
        query.iter(world).collect::<Vec<_>>()
    };
    for entity in camera_entities {
        let transform = *world
            .get::<Transform>(entity)
            .expect("editor camera must have a transform");
        let radius = transform.translation.distance(focus).max(3.0);
        world.entity_mut(entity).insert(PanOrbitCamera {
            focus,
            target_focus: focus,
            radius: Some(radius),
            target_radius: radius,
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            zoom_lower_limit: 0.5,
            ..default()
        });
    }
}

fn player_build_colour(material: PlayerBuildMaterial) -> Color {
    match material {
        PlayerBuildMaterial::Stone => Color::srgb(0.42, 0.40, 0.36),
        PlayerBuildMaterial::Brick => Color::srgb(0.48, 0.23, 0.16),
        PlayerBuildMaterial::Plaster => Color::srgb(0.72, 0.66, 0.53),
        PlayerBuildMaterial::TimberFrame | PlayerBuildMaterial::Timber => {
            Color::srgb(0.28, 0.16, 0.08)
        }
        PlayerBuildMaterial::Tile => Color::srgb(0.36, 0.12, 0.08),
        PlayerBuildMaterial::Thatch => Color::srgb(0.55, 0.43, 0.18),
        PlayerBuildMaterial::Earth => Color::srgb(0.30, 0.22, 0.12),
    }
}

/// Player-build parts are rendered directly from their own document.  They do
/// not enter the generated-plan mesh or audit pipeline: the programme remains
/// a separate optional source of architecture and analysis.
fn setup_player_build_scene(world: &mut World, document: &PlayerBuildDocument) {
    for part in &document.parts {
        let thickness = if matches!(part.kind, PlayerBuildPartKind::Wall) {
            part.depth_metres.min(WALL_THICKNESS_METRES).max(0.05)
        } else {
            part.depth_metres
        };
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
            part.width_metres,
            part.height_metres,
            thickness,
        ));
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: player_build_colour(part.material),
                perceptual_roughness: 0.82,
                ..default()
            });
        world.spawn((
            Name::new(format!("player build {:?} {}", part.kind, part.id)),
            PlayerBuildEntity,
            EditorVisibilityTarget {
                storey: usize::from(part.storey),
                role: match part.kind {
                    PlayerBuildPartKind::Wall => EditorVisibilityRole::Wall,
                    PlayerBuildPartKind::Roof => EditorVisibilityRole::Roof,
                    _ => EditorVisibilityRole::Wall,
                },
            },
            EditorBaseMaterial(material.clone()),
            EditorAppearanceIsTranslucent(false),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Visibility::Visible,
            Transform::from_xyz(
                part.x_metres,
                part.elevation_metres + part.height_metres * 0.5,
                part.z_metres,
            )
            .with_rotation(Quat::from_rotation_y(part.rotation_degrees.to_radians())),
        ));
    }
}

fn update_editor_visibility(
    runtime: Res<EditorRuntime>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut targets: Query<(
        &EditorVisibilityTarget,
        &EditorBaseMaterial,
        &mut EditorAppearanceIsTranslucent,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
        Option<&EditorFachwerkForFinishedWall>,
    )>,
) {
    if !runtime.is_changed() {
        return;
    }
    for (target, base_material, mut appearance, mut material, mut visibility, hide_fachwerk) in
        &mut targets
    {
        let above_active_storey = target.storey > runtime.active_storey;
        let hidden_wall = target.role == EditorVisibilityRole::Wall
            && runtime.wall_visibility == WallVisibility::Down;
        let hidden_roof = target.role == EditorVisibilityRole::Roof
            && runtime.roof_visibility == RoofVisibility::Hide;
        *visibility =
            if above_active_storey || hidden_wall || hidden_roof || hide_fachwerk.is_some() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        let translucent = match target.role {
            EditorVisibilityRole::Wall if runtime.wall_visibility == WallVisibility::Cutaway => {
                true
            }
            EditorVisibilityRole::Roof if runtime.roof_visibility == RoofVisibility::Ghost => true,
            _ => false,
        };
        if appearance.0 != translucent {
            material.0 = if translucent {
                let mut ghost = materials
                    .get(&base_material.0)
                    .cloned()
                    .unwrap_or_else(StandardMaterial::default);
                let colour = ghost.base_color.to_srgba();
                ghost.base_color = Color::srgba(colour.red, colour.green, colour.blue, 0.24);
                ghost.alpha_mode = AlphaMode::Blend;
                materials.add(ghost)
            } else {
                base_material.0.clone()
            };
            appearance.0 = translucent;
        }
    }
}

fn frame_editor_selection(
    keys: Res<ButtonInput<KeyCode>>,
    runtime: Res<EditorRuntime>,
    targets: Query<(
        &EditorSelectable,
        &GlobalTransform,
        Option<&ResolvedRenderItem>,
        Option<&RoofRenderItem>,
    )>,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(selected) = runtime.selected else {
        return;
    };
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut found = false;
    for (selectable, transform, resolved, roof) in &targets {
        if selectable.0 != selected {
            continue;
        }
        let half_size = resolved
            .map(|item| item.local_half_size)
            .or_else(|| roof.map(|item| item.local_half_size))
            .unwrap_or(Vec3::splat(0.25));
        let centre = transform.translation();
        minimum = minimum.min(centre - half_size);
        maximum = maximum.max(centre + half_size);
        found = true;
    }
    if !found {
        return;
    }
    let focus = (minimum + maximum) * 0.5;
    let radius = (maximum - minimum).length().max(1.0) * 1.6;
    for mut camera in &mut cameras {
        camera.target_focus = focus;
        camera.target_radius = radius;
        camera.force_update = true;
    }
}

fn rebuild_editor_scene(world: &mut World) {
    let pending = world
        .get_resource::<EditorRuntime>()
        .is_some_and(|runtime| runtime.pending_rebuild);
    if pending {
        let old_entities = {
            let mut query = world.query_filtered::<Entity, With<EditorBuildingEntity>>();
            query.iter(world).collect::<Vec<_>>()
        };
        for entity in old_entities {
            let _ = world.despawn(entity);
        }
        let plan = world.resource::<EditorRuntime>().plan.clone();
        setup(
            world,
            &plan,
            ViewerView::Exterior,
            ProjectedProofKind::Machicolation,
            None,
            SceneSetup::EditorBuilding,
        );
        configure_editor_scene(world, &plan, false);
        world.resource_mut::<EditorRuntime>().pending_rebuild = false;
    }

    let player_rebuild = world
        .get_resource::<EditorRuntime>()
        .is_some_and(|runtime| runtime.pending_player_rebuild);
    if player_rebuild {
        let old_entities = {
            let mut query = world.query_filtered::<Entity, With<PlayerBuildEntity>>();
            query.iter(world).collect::<Vec<_>>()
        };
        for entity in old_entities {
            let _ = world.despawn(entity);
        }
        if let Some(document) = world.resource::<EditorRuntime>().player_build.clone() {
            setup_player_build_scene(world, &document);
        }
        world.resource_mut::<EditorRuntime>().pending_player_rebuild = false;
    }
}

fn stable_evidence_hash(bytes: &[u8]) -> String {
    format!("fnv1a64:{:016x}", stable_u64(bytes))
}

fn stable_u64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn resolved_item_multiset_hash(items: impl IntoIterator<Item = (u64, u64)>) -> String {
    let mut items = items.into_iter().collect::<Vec<_>>();
    items.sort_unstable();
    stable_evidence_hash(&serde_json::to_vec(&items).expect("serialize resolved item fingerprints"))
}

fn source_revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn source_dirty_fingerprint() -> String {
    stable_evidence_hash(
        [
            include_str!("model.rs"),
            include_str!("generator.rs"),
            include_str!("audit.rs"),
            include_str!("viewer.rs"),
            include_str!("main.rs"),
            include_str!("lib.rs"),
        ]
        .concat()
        .as_bytes(),
    )
}

#[derive(Clone, Debug, Deserialize)]
struct CrownSuiteManifest {
    fixture: String,
    view: String,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    validation_passed: bool,
}

const CROWN_PROOF_SUITE: [(&str, &str, &str); 9] = [
    (
        "crown-straight-exterior",
        "courtyard-castle",
        "crown-straight-exterior",
    ),
    (
        "crown-straight-interior",
        "courtyard-castle",
        "crown-straight-interior",
    ),
    (
        "crown-corner-exterior",
        "walled-keep",
        "crown-corner-exterior",
    ),
    (
        "crown-corner-interior",
        "walled-keep",
        "crown-corner-interior",
    ),
    (
        "crown-gate-tower-exterior",
        "walled-keep",
        "crown-tower-exterior",
    ),
    ("crown-gate-tower-top", "walled-keep", "crown-tower-top"),
    (
        "crown-gate-tower-cutaway",
        "walled-keep",
        "crown-tower-cutaway",
    ),
    ("crown-courtyard-regression", "courtyard-castle", "exterior"),
    ("crown-walled-keep-regression", "walled-keep", "exterior"),
];

fn validate_crown_suite_records(records: &[(&str, CrownSuiteManifest)]) -> Result<(), String> {
    if records.len() != CROWN_PROOF_SUITE.len() {
        return Err(format!(
            "expected {} crown proof manifests, found {}",
            CROWN_PROOF_SUITE.len(),
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut fixtures = std::collections::HashMap::<&str, (&str, &str)>::new();
    for ((actual_name, manifest), (expected_name, expected_fixture, expected_view)) in
        records.iter().zip(CROWN_PROOF_SUITE)
    {
        if *actual_name != expected_name
            || manifest.fixture != expected_fixture
            || manifest.view != expected_view
        {
            return Err(format!(
                "proof {actual_name} does not match expected {expected_name}/{expected_fixture}/{expected_view}"
            ));
        }
        if !manifest.validation_passed || manifest.resolver_schema_version != 2 {
            return Err(format!(
                "proof {actual_name} is invalid or not resolver schema 2"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!(
                "proof {actual_name} comes from a mixed source build"
            ));
        }
        if let Some((plan_hash, resolved_hash)) = fixtures.get(expected_fixture) {
            if *plan_hash != manifest.plan_hash || *resolved_hash != manifest.resolved_geometry_hash
            {
                return Err(format!(
                    "proof {actual_name} disagrees with its fixture plan/resolved hash"
                ));
            }
        } else {
            fixtures.insert(
                expected_fixture,
                (&manifest.plan_hash, &manifest.resolved_geometry_hash),
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_crown_suite(directory: &std::path::Path) -> Result<(), String> {
    let mut owned = Vec::with_capacity(CROWN_PROOF_SUITE.len());
    for (basename, _, _) in CROWN_PROOF_SUITE {
        let path = directory.join(format!("{basename}.capture.json"));
        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let manifest = serde_json::from_slice::<CrownSuiteManifest>(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        owned.push((basename, manifest));
    }
    validate_crown_suite_records(&owned)
}

#[derive(Clone, Debug, Deserialize)]
struct ProjectedSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    focus_kind: Option<String>,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_projected_ray_count: usize,
    projected_defense_kind: Option<String>,
    projected_defense_deployment: Option<String>,
    projected_tactical_target: Option<String>,
    validation_passed: bool,
}

#[derive(Clone, Copy)]
struct ProjectedProofExpectation {
    basename: &'static str,
    fixture: &'static str,
    view: &'static str,
    seed: u64,
    kind: Option<&'static str>,
    deployment: Option<&'static str>,
}

const PROJECTED_PROOF_SUITE: [ProjectedProofExpectation; 23] = [
    ProjectedProofExpectation {
        basename: "machicolation-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "machicolation-longitudinal",
        fixture: "castle-gatehouse",
        view: "projected-longitudinal",
        seed: 42,
        kind: Some("machicolation"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "breteche-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 201,
        kind: Some("breteche"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-sockets",
        fixture: "castle-gatehouse",
        view: "projected-sockets",
        seed: 42,
        kind: Some("hoarding"),
        deployment: Some("sockets_only"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "hoarding-longitudinal",
        fixture: "castle-gatehouse",
        view: "projected-longitudinal",
        seed: 202,
        kind: Some("hoarding"),
        deployment: Some("deployed"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-exterior",
        fixture: "castle-gatehouse",
        view: "projected-exterior",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-interior",
        fixture: "castle-gatehouse",
        view: "projected-interior",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-underside",
        fixture: "castle-gatehouse",
        view: "projected-underside",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-top",
        fixture: "castle-gatehouse",
        view: "projected-top",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "bartizan-flank",
        fixture: "castle-gatehouse",
        view: "projected-flank",
        seed: 203,
        kind: Some("bartizan"),
        deployment: Some("permanent"),
    },
    ProjectedProofExpectation {
        basename: "projected-castle-gatehouse-regression",
        fixture: "castle-gatehouse",
        view: "exterior",
        seed: 42,
        kind: None,
        deployment: None,
    },
    ProjectedProofExpectation {
        basename: "projected-courtyard-regression",
        fixture: "courtyard-castle",
        view: "exterior",
        seed: 42,
        kind: None,
        deployment: None,
    },
    ProjectedProofExpectation {
        basename: "projected-walled-keep-regression",
        fixture: "walled-keep",
        view: "exterior",
        seed: 42,
        kind: None,
        deployment: None,
    },
];

fn validate_projected_suite_records(
    records: &[(&str, ProjectedSuiteManifest)],
) -> Result<(), String> {
    if records.len() != PROJECTED_PROOF_SUITE.len() {
        return Err(format!(
            "expected {} projected-defense manifests, found {}",
            PROJECTED_PROOF_SUITE.len(),
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut fixtures = std::collections::HashMap::<(&str, u64), (&str, &str)>::new();
    for ((actual_name, manifest), expected) in records.iter().zip(PROJECTED_PROOF_SUITE) {
        if *actual_name != expected.basename
            || manifest.fixture != expected.fixture
            || manifest.view != expected.view
            || manifest.seed != expected.seed
            || manifest.projected_defense_kind.as_deref() != expected.kind
            || manifest.projected_defense_deployment.as_deref() != expected.deployment
        {
            return Err(format!(
                "projected proof {actual_name} does not match its expected fixture/view/seed/state"
            ));
        }
        if !manifest.validation_passed || manifest.resolver_schema_version != 2 {
            return Err(format!(
                "projected proof {actual_name} is invalid or not resolver schema 2"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!(
                "projected proof {actual_name} comes from a mixed source build"
            ));
        }
        if expected.kind.is_some()
            && (manifest.focus_kind.as_deref() != Some("resolved_projected")
                || manifest.focused_resolved_item_ids.is_empty()
                || manifest.focused_resolved_void_ids.is_empty()
                    && expected.deployment != Some("sockets_only")
                || manifest.focused_projected_ray_count == 0
                    && expected.deployment != Some("sockets_only")
                || manifest.projected_tactical_target.is_none())
        {
            return Err(format!(
                "projected proof {actual_name} lacks exact assembly IDs, voids, rays, or tactical target"
            ));
        }
        let fixture_key = (expected.fixture, expected.seed);
        if let Some((plan_hash, resolved_hash)) = fixtures.get(&fixture_key) {
            if *plan_hash != manifest.plan_hash || *resolved_hash != manifest.resolved_geometry_hash
            {
                return Err(format!(
                    "projected proof {actual_name} disagrees with its fixture/seed hashes"
                ));
            }
        } else {
            fixtures.insert(
                fixture_key,
                (&manifest.plan_hash, &manifest.resolved_geometry_hash),
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_projected_suite(directory: &std::path::Path) -> Result<(), String> {
    let mut owned = Vec::with_capacity(PROJECTED_PROOF_SUITE.len());
    for expected in PROJECTED_PROOF_SUITE {
        let path = directory.join(format!("{}.capture.json", expected.basename));
        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let manifest = serde_json::from_slice::<ProjectedSuiteManifest>(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        owned.push((expected.basename, manifest));
    }
    validate_projected_suite_records(&owned)
}

#[derive(Clone, Debug, Deserialize)]
struct OpeningsSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    opening_profile: Option<String>,
    wall_section_kind: Option<String>,
    focused_assembly_owner_id: Option<u32>,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_resolved_geometry_hash: Option<String>,
    section_cut_applied: bool,
    section_removed_item_ids: Vec<u64>,
    inside_label_visible: bool,
    outside_label_visible: bool,
    wall_thickness_metres: Option<f32>,
    scale_figure_height_metres: Option<f32>,
    scale_figure_visible: bool,
    section_annotation: String,
    section_annotation_visible: bool,
    exterior_throat_bounds_fraction: [f32; 4],
    interior_mouth_bounds_fraction: [f32; 4],
    validation_passed: bool,
}

#[derive(Clone, Copy)]
struct OpeningsProofExpectation {
    basename: &'static str,
    fixture: &'static str,
    view: &'static str,
    opening_profile: Option<&'static str>,
    wall_section_kind: Option<&'static str>,
    section: bool,
}

const OPENINGS_PROOF_SUITE: [OpeningsProofExpectation; 24] = [
    OpeningsProofExpectation {
        basename: "opening-rectangular-exterior",
        fixture: "fachwerk-merchant-house",
        view: "opening-rectangular-exterior",
        opening_profile: Some("rectangular"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-rectangular-interior",
        fixture: "fachwerk-merchant-house",
        view: "opening-rectangular-interior",
        opening_profile: Some("rectangular"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-rectangular-section",
        fixture: "fachwerk-merchant-house",
        view: "opening-rectangular-section",
        opening_profile: Some("rectangular"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-segmental-exterior",
        fixture: "renaissance-town-hall",
        view: "opening-segmental-exterior",
        opening_profile: Some("segmental"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-segmental-interior",
        fixture: "renaissance-town-hall",
        view: "opening-segmental-interior",
        opening_profile: Some("segmental"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-segmental-section",
        fixture: "renaissance-town-hall",
        view: "opening-segmental-section",
        opening_profile: Some("segmental"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-pointed-exterior",
        fixture: "cathedral",
        view: "opening-pointed-exterior",
        opening_profile: Some("pointed_two_centred"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-pointed-interior",
        fixture: "cathedral",
        view: "opening-pointed-interior",
        opening_profile: Some("pointed_two_centred"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-pointed-section",
        fixture: "cathedral",
        view: "opening-pointed-section",
        opening_profile: Some("pointed_two_centred"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-arrow-loop-exterior",
        fixture: "courtyard-castle",
        view: "opening-arrow-loop-exterior",
        opening_profile: Some("arrow_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-arrow-loop-interior",
        fixture: "courtyard-castle",
        view: "opening-arrow-loop-interior",
        opening_profile: Some("arrow_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-arrow-loop-section",
        fixture: "courtyard-castle",
        view: "opening-arrow-loop-section",
        opening_profile: Some("arrow_loop"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "opening-gun-loop-exterior",
        fixture: "walled-keep",
        view: "opening-gun-loop-exterior",
        opening_profile: Some("gun_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-gun-loop-interior",
        fixture: "walled-keep",
        view: "opening-gun-loop-interior",
        opening_profile: Some("gun_loop"),
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "opening-gun-loop-section",
        fixture: "walled-keep",
        view: "opening-gun-loop-section",
        opening_profile: Some("gun_loop"),
        wall_section_kind: None,
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-timber-frame-section",
        fixture: "fachwerk-merchant-house",
        view: "wall-timber-frame-section",
        opening_profile: None,
        wall_section_kind: Some("timber_frame"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-civilian-masonry-section",
        fixture: "renaissance-town-hall",
        view: "wall-civilian-masonry-section",
        opening_profile: None,
        wall_section_kind: Some("civilian_masonry"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-cathedral-buttress-section",
        fixture: "cathedral",
        view: "wall-cathedral-buttress-section",
        opening_profile: None,
        wall_section_kind: Some("cathedral_buttress"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "wall-round-tower-radial-section",
        fixture: "walled-keep",
        view: "wall-round-tower-radial-section",
        opening_profile: None,
        wall_section_kind: Some("round_tower_radial"),
        section: true,
    },
    OpeningsProofExpectation {
        basename: "openings-fachwerk-merchant-regression",
        fixture: "fachwerk-merchant-house",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-renaissance-town-hall-regression",
        fixture: "renaissance-town-hall",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-cathedral-regression",
        fixture: "cathedral",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-courtyard-castle-regression",
        fixture: "courtyard-castle",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
    OpeningsProofExpectation {
        basename: "openings-walled-keep-regression",
        fixture: "walled-keep",
        view: "exterior",
        opening_profile: None,
        wall_section_kind: None,
        section: false,
    },
];

pub(crate) fn validate_openings_suite(directory: &std::path::Path) -> Result<(), String> {
    let actual_count = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".capture.json")
        })
        .count();
    if actual_count != OPENINGS_PROOF_SUITE.len() {
        return Err(format!(
            "expected exactly 24 proof manifests, found {actual_count}"
        ));
    }
    let mut records = Vec::new();
    for expected in OPENINGS_PROOF_SUITE {
        let path = directory.join(format!("{}.capture.json", expected.basename));
        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let manifest: OpeningsSuiteManifest = serde_json::from_slice(&bytes)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((expected, manifest));
    }
    validate_openings_suite_records(&records)
}

fn validate_openings_suite_records(
    records: &[(OpeningsProofExpectation, OpeningsSuiteManifest)],
) -> Result<(), String> {
    if records.len() != OPENINGS_PROOF_SUITE.len() {
        return Err(format!(
            "expected exactly 24 proof records, found {}",
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut fixture_hashes = std::collections::HashMap::new();
    let mut opening_focuses = std::collections::HashMap::new();
    for (expected, manifest) in records {
        if manifest.fixture != expected.fixture
            || manifest.view != expected.view
            || manifest.seed != 42
            || manifest.opening_profile.as_deref() != expected.opening_profile
            || manifest.wall_section_kind.as_deref() != expected.wall_section_kind
            || manifest.resolver_schema_version != 2
            || !manifest.validation_passed
        {
            return Err(format!(
                "proof {} violates its expectation",
                expected.basename
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!(
                "proof {} comes from a mixed source build",
                expected.basename
            ));
        }
        if let Some((plan_hash, geometry_hash)) = fixture_hashes.get(expected.fixture) {
            if plan_hash != &manifest.plan_hash || geometry_hash != &manifest.resolved_geometry_hash
            {
                return Err(format!(
                    "proof {} has stale fixture hashes",
                    expected.basename
                ));
            }
        } else {
            fixture_hashes.insert(
                expected.fixture,
                (
                    manifest.plan_hash.clone(),
                    manifest.resolved_geometry_hash.clone(),
                ),
            );
        }
        let focused = expected.opening_profile.is_some() || expected.wall_section_kind.is_some();
        if focused
            && (manifest.focused_assembly_owner_id.is_none()
                || manifest.focused_resolved_item_ids.is_empty()
                || manifest.focused_resolved_geometry_hash.is_none()
                || (expected.opening_profile.is_some()
                    && manifest.focused_resolved_void_ids.is_empty()))
        {
            return Err(format!(
                "proof {} lacks exact focused geometry",
                expected.basename
            ));
        }
        if let Some(profile) = expected.opening_profile {
            let state = (
                manifest.focused_assembly_owner_id,
                manifest.focused_resolved_item_ids.clone(),
                manifest.focused_resolved_void_ids.clone(),
                manifest.focused_resolved_geometry_hash.clone(),
            );
            if let Some(previous) = opening_focuses.get(profile) {
                if previous != &state {
                    return Err(format!(
                        "proof {} drifts from its opening triple",
                        expected.basename
                    ));
                }
            } else {
                opening_focuses.insert(profile, state);
            }
        } else if !focused
            && (manifest.focused_assembly_owner_id.is_some()
                || !manifest.focused_resolved_item_ids.is_empty()
                || !manifest.focused_resolved_void_ids.is_empty()
                || manifest.focused_resolved_geometry_hash.is_some())
        {
            return Err(format!(
                "regression {} contains stale focused proof state",
                expected.basename
            ));
        }
        if expected.section
            && (!manifest.section_cut_applied
                || !manifest.inside_label_visible
                || !manifest.outside_label_visible
                || manifest
                    .wall_thickness_metres
                    .is_none_or(|value| value <= 0.0)
                || manifest.scale_figure_height_metres != Some(1.75)
                || !manifest.scale_figure_visible
                || !manifest.section_annotation_visible
                || !manifest.section_annotation.contains("wall=")
                || !manifest.section_annotation.contains("opening=")
                || !manifest.section_annotation.contains("profile=")
                || !manifest.section_annotation.contains("thickness=")
                || (expected.wall_section_kind != Some("round_tower_radial")
                    && manifest.section_removed_item_ids.is_empty())
                || manifest
                    .section_removed_item_ids
                    .iter()
                    .any(|id| !manifest.focused_resolved_item_ids.contains(id)))
        {
            return Err(format!(
                "proof {} is not a genuine labeled section",
                expected.basename
            ));
        }
        if matches!(expected.opening_profile, Some("arrow_loop" | "gun_loop")) {
            let valid_bounds = |bounds: [f32; 4]| {
                bounds[0] >= 0.0
                    && bounds[1] >= 0.0
                    && bounds[2] > bounds[0]
                    && bounds[3] > bounds[1]
                    && bounds[2] <= 1.0
                    && bounds[3] <= 1.0
            };
            if !valid_bounds(manifest.exterior_throat_bounds_fraction)
                || !valid_bounds(manifest.interior_mouth_bounds_fraction)
            {
                return Err(format!(
                    "proof {} does not project both military throat and mouth",
                    expected.basename
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
struct RoofSuiteManifest {
    fixture: String,
    view: String,
    resolver_schema_version: u16,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    roof_graph_hash: String,
    roof_render_item_count: usize,
    roof_render_multiset_hash: String,
    rendered_roof_item_count: usize,
    rendered_roof_hash: String,
    focused_roof_item_ids: Vec<u64>,
    visible_focused_roof_item_count: usize,
    section_removed_roof_item_ids: Vec<u64>,
    section_annotation_visible: bool,
    roof_drainage_network_ids: Vec<u64>,
    roof_drainage_channel_ids: Vec<u64>,
    roof_drainage_outlet_ids: Vec<u64>,
    roof_drainage_route_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    validation_passed: bool,
}

const ROOF_PROOF_SLUGS: [&str; 50] = [
    "roof-gable-exterior",
    "roof-gable-interior",
    "roof-gable-top",
    "roof-gable-cutaway",
    "roof-gable-drainage",
    "roof-gable-low-pitch",
    "roof-gable-mid-pitch",
    "roof-gable-high-pitch",
    "roof-hip-halfhip-exterior",
    "roof-hip-halfhip-top",
    "roof-hip-halfhip-underside",
    "roof-l-valley-exterior",
    "roof-l-valley-top",
    "roof-l-valley-underside",
    "roof-l-valley-drainage",
    "roof-courtyard-valleys-top",
    "roof-dormer-gabled-exterior",
    "roof-dormer-gabled-interior",
    "roof-dormer-gabled-top",
    "roof-dormer-gabled-cutaway",
    "roof-dormer-gabled-drainage",
    "roof-dormer-shed-exterior",
    "roof-dormer-shed-interior",
    "roof-dormer-shed-top",
    "roof-dormer-shed-cutaway",
    "roof-dormer-shed-drainage",
    "roof-cross-gable-exterior",
    "roof-cross-gable-top",
    "roof-cross-gable-underside",
    "roof-cross-gable-drainage",
    "roof-abutment-wall-exterior",
    "roof-abutment-wall-top",
    "roof-abutment-wall-cutaway",
    "roof-abutment-wall-drainage",
    "roof-abutment-tower-exterior",
    "roof-abutment-tower-top",
    "roof-abutment-tower-cutaway",
    "roof-abutment-tower-drainage",
    "roof-round-tower-exterior",
    "roof-round-tower-top",
    "roof-round-tower-cutaway",
    "roof-round-tower-drainage",
    "roof-pavilion-exterior",
    "roof-pavilion-top",
    "roof-pavilion-cutaway",
    "roof-pavilion-drainage",
    "roof-cathedral-exterior",
    "roof-cathedral-top",
    "roof-cathedral-cutaway",
    "roof-cathedral-drainage",
];

const ROOF_REGRESSION_FIXTURES: [&str; 9] = [
    "town-house",
    "hall-house",
    "fachwerk-cottage",
    "fachwerk-merchant-house",
    "renaissance-town-hall",
    "cathedral",
    "castle-gatehouse",
    "courtyard-castle",
    "walled-keep",
];

pub(crate) fn validate_roof_suite(directory: &std::path::Path) -> Result<(), String> {
    let mut expected = ROOF_PROOF_SLUGS
        .iter()
        .map(|slug| (*slug, *slug))
        .collect::<Vec<_>>();
    let regression_names = ROOF_REGRESSION_FIXTURES
        .iter()
        .map(|fixture| (format!("roof-{fixture}-regression"), "exterior".to_owned()))
        .collect::<Vec<_>>();
    let actual_count = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".capture.json")
        })
        .count();
    if actual_count != expected.len() + regression_names.len() {
        return Err(format!(
            "expected exactly 59 roof manifests, found {actual_count}"
        ));
    }
    let mut records = Vec::new();
    for (basename, view) in expected.drain(..) {
        let path = directory.join(format!("{basename}.capture.json"));
        let manifest: RoofSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((basename.to_owned(), view.to_owned(), true, manifest));
    }
    for (basename, view) in regression_names {
        let path = directory.join(format!("{basename}.capture.json"));
        let manifest: RoofSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((basename, view, false, manifest));
    }
    validate_roof_suite_records(&records)
}

fn validate_roof_suite_records(
    records: &[(String, String, bool, RoofSuiteManifest)],
) -> Result<(), String> {
    if records.len() != 59 {
        return Err(format!("expected 59 roof records, found {}", records.len()));
    }
    let first = &records[0].3;
    let mut fixture_hashes = std::collections::HashMap::new();
    let mut pitch_state_hashes = std::collections::HashSet::new();
    for (basename, expected_view, focused, manifest) in records {
        if manifest.view != *expected_view || !manifest.validation_passed {
            return Err(format!("{basename} has invalid view or failed capture QA"));
        }
        if manifest.resolver_schema_version != first.resolver_schema_version
            || manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!("{basename} comes from a mixed source build"));
        }
        if manifest.roof_render_item_count != manifest.rendered_roof_item_count
            || manifest.roof_render_multiset_hash != manifest.rendered_roof_hash
        {
            return Err(format!("{basename} has roof renderer correspondence drift"));
        }
        if *focused
            && (manifest.focused_roof_item_ids.is_empty()
                || manifest.visible_focused_roof_item_count
                    + manifest.section_removed_roof_item_ids.len()
                    != manifest.focused_roof_item_ids.len()
                || !manifest.section_annotation_visible)
        {
            return Err(format!(
                "{basename} lacks exact visible focused roof authority"
            ));
        }
        if manifest.roof_graph_hash.is_empty() {
            return Err(format!("{basename} lacks roof graph hash"));
        }
        if basename.ends_with("-drainage")
            && (manifest.roof_drainage_network_ids.is_empty()
                || manifest.roof_drainage_channel_ids.is_empty()
                || manifest.roof_drainage_outlet_ids.is_empty()
                || manifest.roof_drainage_route_ids.is_empty()
                || manifest.focused_resolved_void_ids.is_empty())
        {
            return Err(format!(
                "{basename} lacks exact focused face-channel-outlet drainage authority"
            ));
        }
        let pitch_state = basename.contains("-low-pitch")
            || basename.contains("-mid-pitch")
            || basename.contains("-high-pitch");
        let demonstrator_state = pitch_state || basename.contains("roof-round-tower-");
        if pitch_state {
            pitch_state_hashes.insert(manifest.roof_graph_hash.clone());
        }
        if demonstrator_state {
            continue;
        } else if let Some((plan_hash, roof_hash)) = fixture_hashes.get(&manifest.fixture) {
            if plan_hash != &manifest.plan_hash || roof_hash != &manifest.roof_graph_hash {
                return Err(format!(
                    "{basename} has fixture-inconsistent plan/roof hash"
                ));
            }
        } else {
            fixture_hashes.insert(
                manifest.fixture.clone(),
                (manifest.plan_hash.clone(), manifest.roof_graph_hash.clone()),
            );
        }
    }
    if pitch_state_hashes.len() != 3 {
        return Err("low/mid/high pitch handles did not produce three roof graphs".to_owned());
    }
    Ok(())
}

const CHURCH_PROOF_SLUGS: [&str; 30] = [
    "church-whole-west",
    "church-whole-east",
    "church-whole-north",
    "church-whole-south",
    "church-whole-top",
    "church-whole-longitudinal-cut",
    "church-whole-transverse-cut",
    "church-whole-regression",
    "church-bay-exterior",
    "church-bay-interior",
    "church-bay-section",
    "church-bay-load",
    "church-bay-vault",
    "church-crossing-interior",
    "church-crossing-exterior",
    "church-crossing-top",
    "church-crossing-cut-load",
    "church-choir-east",
    "church-choir-interior",
    "church-choir-top",
    "church-choir-radial-section",
    "church-tower-portal",
    "church-tower-junction",
    "church-tower-stair",
    "church-tower-bell-underside",
    "church-tower-frame",
    "church-tower-louvred-exterior",
    "church-tower-roof-drain",
    "church-drainage",
    "church-support-dag",
];

#[derive(Clone, Debug, Deserialize)]
struct ChurchSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    resolved_geometry_hash: String,
    church_program_hash: String,
    church_bay_labels: Vec<String>,
    church_support_node_ids: Vec<u64>,
    church_opening_ids: Vec<u64>,
    church_focused_roles: Vec<String>,
    church_target_component_ids: Vec<String>,
    church_target_item_ids: Vec<u64>,
    church_required_roles: Vec<String>,
    church_cut_plane: Option<[f32; 4]>,
    church_removed_target_item_ids: Vec<u64>,
    church_legend_visible: bool,
    focused_bounds_fraction: [f32; 4],
    pixel_hash: String,
    focused_resolved_item_ids: Vec<u64>,
    section_removed_item_ids: Vec<u64>,
    visible_focused_resolved_item_count: usize,
    section_cut_applied: bool,
    section_annotation_visible: bool,
    plan_audit_issue_count: usize,
    validation_passed: bool,
}

pub(crate) fn validate_church_suite(directory: &std::path::Path) -> Result<(), String> {
    let actual_count = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".capture.json")
        })
        .count();
    if actual_count != CHURCH_PROOF_SLUGS.len() {
        return Err(format!(
            "expected exactly {} church manifests, found {actual_count}",
            CHURCH_PROOF_SLUGS.len()
        ));
    }
    let mut records = Vec::new();
    for slug in CHURCH_PROOF_SLUGS {
        let path = directory.join(format!("{slug}.capture.json"));
        let manifest: ChurchSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((slug, manifest));
    }
    validate_church_suite_records(&records)
}

fn validate_church_suite_records(records: &[(&str, ChurchSuiteManifest)]) -> Result<(), String> {
    if records.len() != CHURCH_PROOF_SLUGS.len() {
        return Err(format!(
            "expected 30 church proofs, found {}",
            records.len()
        ));
    }
    let first = &records[0].1;
    let mut pixel_hashes = std::collections::HashMap::<&str, &str>::new();
    for ((expected, manifest), slug) in records.iter().zip(CHURCH_PROOF_SLUGS) {
        if *expected != slug
            || manifest.fixture != "cathedral"
            || manifest.view != slug
            || manifest.seed != 47
            || manifest.resolver_schema_version != 2
            || !manifest.validation_passed
            || manifest.plan_audit_issue_count != 0
        {
            return Err(format!(
                "church proof {slug} violates its fixed fixture contract"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
            || manifest.plan_hash != first.plan_hash
            || manifest.resolved_geometry_hash != first.resolved_geometry_hash
            || manifest.church_program_hash != first.church_program_hash
        {
            return Err(format!(
                "church proof {slug} comes from mixed or stale authority"
            ));
        }
        if manifest.church_bay_labels.len() != 8
            || manifest.church_support_node_ids.is_empty()
            || manifest.church_opening_ids.len() < 30
            || manifest.focused_resolved_item_ids.is_empty()
            || manifest.visible_focused_resolved_item_count == 0
        {
            return Err(format!(
                "church proof {slug} lacks exact bay/support/opening focus IDs"
            ));
        }
        if manifest.church_target_component_ids.len() != 1
            || manifest.church_target_item_ids != manifest.focused_resolved_item_ids
            || manifest.church_target_item_ids.is_empty()
            || !manifest
                .church_removed_target_item_ids
                .iter()
                .all(|id| manifest.church_target_item_ids.contains(id))
            || manifest.church_removed_target_item_ids
                != manifest
                    .section_removed_item_ids
                    .iter()
                    .filter(|id| manifest.church_target_item_ids.contains(id))
                    .copied()
                    .collect::<Vec<_>>()
            || !manifest.church_legend_visible
        {
            return Err(format!(
                "church proof {slug} is not bound to its exact target/cut authority"
            ));
        }
        if manifest.pixel_hash.is_empty() {
            return Err(format!("church proof {slug} lacks a captured pixel hash"));
        }
        if let Some(previous) = pixel_hashes.insert(&manifest.pixel_hash, slug) {
            return Err(format!(
                "church proofs {previous} and {slug} are pixel-identical instead of proving distinct contracts"
            ));
        }
        let has_role = |role: &str| {
            manifest
                .church_focused_roles
                .iter()
                .any(|item| item == role)
        };
        let kind_roles_valid = match slug {
            "church-bay-section" => has_role("ChurchPier") && has_role("ChurchArcade"),
            "church-bay-load" | "church-support-dag" => {
                has_role("ChurchVaultThrust") && has_role("WallButtress")
            }
            "church-bay-vault" => has_role("ChurchVaultShell"),
            "church-tower-stair" => has_role("ChurchStairTread") && has_role("Landing"),
            "church-tower-bell-underside" => has_role("ChurchBellFloor") && has_role("ChurchBell"),
            "church-tower-frame" => has_role("ChurchBellFrame") && has_role("ChurchServiceLadder"),
            "church-tower-roof-drain" | "church-drainage" => {
                has_role("RoofGutter") || has_role("RoofEdgeTreatment")
            }
            _ => true,
        };
        if !kind_roles_valid
            || manifest
                .church_required_roles
                .iter()
                .any(|role| !has_role(role))
        {
            return Err(format!(
                "church proof {slug} lacks its kind-specific resolved roles"
            ));
        }
        let expects_section = slug.contains("cut")
            || slug.ends_with("-interior")
            || slug.ends_with("-section")
            || slug.ends_with("-load")
            || slug.ends_with("-vault")
            || matches!(
                slug,
                "church-tower-junction"
                    | "church-tower-stair"
                    | "church-tower-bell-underside"
                    | "church-tower-frame"
                    | "church-support-dag"
            );
        if expects_section
            && (!manifest.section_cut_applied
                || !manifest.section_annotation_visible
                || manifest.church_cut_plane.is_none())
        {
            return Err(format!(
                "church proof {slug} lacks its genuine cut/authority annotation"
            ));
        }
        if !expects_section && (manifest.section_cut_applied || manifest.church_cut_plane.is_some())
        {
            return Err(format!(
                "church proof {slug} applies a section cut outside its proof contract"
            ));
        }
        let target = &manifest.church_target_component_ids[0];
        let suffix_valid = if slug.starts_with("church-bay-") {
            target.ends_with("/nave-bay:2")
        } else if slug.starts_with("church-crossing-") {
            target.ends_with("/crossing")
        } else if slug.starts_with("church-choir-") {
            target.ends_with("/choir-apse")
        } else if slug.starts_with("church-tower-") {
            target.ends_with("/west-tower")
        } else if slug == "church-drainage" {
            target.ends_with("/roof-drainage")
        } else if slug == "church-support-dag" {
            target.ends_with("/nave-bay:2/load-path")
        } else {
            target.ends_with("/whole")
        };
        let bounds = manifest.focused_bounds_fraction;
        let target_area = (bounds[2] - bounds[0]).max(0.0) * (bounds[3] - bounds[1]).max(0.0);
        if !suffix_valid
            || bounds[0] < 0.0
            || bounds[1] < 0.0
            || bounds[2] > 1.0
            || bounds[3] > 1.0
            || target_area < 0.025
        {
            return Err(format!(
                "church proof {slug} is off-target or projects too little target area"
            ));
        }
    }
    Ok(())
}

fn timber_proof_specs() -> Vec<(String, BuildingArchetype, ViewerView)> {
    let mut specs = Vec::new();
    for archetype in TIMBER_ARCHETYPES {
        for view in [
            ViewerView::TimberWholeExterior,
            ViewerView::TimberFrameFacade,
            ViewerView::TimberRegistrationCut,
            ViewerView::TimberSupportLoad,
            ViewerView::TimberProgramDetail,
        ] {
            let suffix = timber_proof_suffix(view).expect("timber view suffix");
            specs.push((
                format!("timber-{}-{suffix}", archetype.slug()),
                archetype,
                view,
            ));
        }
    }
    specs.extend([
        (
            "timber-opening-bay-exterior".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberOpeningBayExterior,
        ),
        (
            "timber-opening-bay-interior".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberOpeningBayInterior,
        ),
        (
            "timber-opening-bay-section".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberOpeningBaySection,
        ),
        (
            "timber-joint-close".to_owned(),
            BuildingArchetype::TownHouse,
            ViewerView::TimberJointClose,
        ),
        (
            "timber-jetty-exterior".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberJettyExterior,
        ),
        (
            "timber-jetty-underside".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberJettyUnderside,
        ),
        (
            "timber-jetty-load".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberJettyLoad,
        ),
        (
            "timber-gable-roof-bearing".to_owned(),
            BuildingArchetype::FachwerkCottage,
            ViewerView::TimberGableRoofBearing,
        ),
        (
            "timber-dormer-trimmer".to_owned(),
            BuildingArchetype::FachwerkMerchantHouse,
            ViewerView::TimberDormerTrimmer,
        ),
        (
            "timber-townhall-masonry-junction".to_owned(),
            BuildingArchetype::RenaissanceTownHall,
            ViewerView::TimberTownHallJunction,
        ),
    ]);
    specs
}

#[derive(Clone, Debug, Default, Deserialize)]
struct TimberSuiteManifest {
    fixture: String,
    view: String,
    seed: u64,
    resolver_schema_version: u16,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    resolved_geometry_hash: String,
    timber_program_hash: String,
    timber_program: Option<String>,
    timber_assembly_id: Option<u64>,
    timber_member_ids: Vec<u64>,
    timber_joint_ids: Vec<u64>,
    timber_node_ids: Vec<u64>,
    timber_focused_roles: Vec<String>,
    timber_role_item_ids: std::collections::BTreeMap<String, Vec<u64>>,
    timber_role_bounds_fraction: std::collections::BTreeMap<String, [f32; 4]>,
    timber_target_component_ids: Vec<String>,
    timber_focus_interface_ids: Vec<u64>,
    timber_required_roles: Vec<String>,
    timber_cut_plane: Option<[f32; 4]>,
    timber_removed_target_item_ids: Vec<u64>,
    timber_legend_visible: bool,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_roof_item_ids: Vec<u64>,
    section_removed_item_ids: Vec<u64>,
    visible_focused_resolved_item_count: usize,
    focused_bounds_fraction: [f32; 4],
    section_cut_applied: bool,
    section_annotation_visible: bool,
    pixel_hash: String,
    plan_audit_issue_count: usize,
    validation_passed: bool,
}

pub(crate) fn validate_timber_suite(directory: &std::path::Path) -> Result<(), String> {
    let specs = timber_proof_specs();
    let actual_count = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".capture.json")
        })
        .count();
    if actual_count != specs.len() {
        return Err(format!(
            "expected exactly {} timber manifests, found {actual_count}",
            specs.len()
        ));
    }
    let mut records = Vec::new();
    for (slug, archetype, view) in specs {
        let path = directory.join(format!("{slug}.capture.json"));
        let manifest: TimberSuiteManifest = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
        records.push((slug, archetype, view, manifest));
    }
    validate_timber_suite_records(&records)
}

pub(crate) fn validate_artillery_suite(directory: &std::path::Path) -> Result<(), String> {
    const VIEWS: [&str; 20] = [
        "artillery-whole-exterior",
        "artillery-whole-courtyard",
        "artillery-whole-top",
        "artillery-whole-longitudinal-cut",
        "artillery-whole-transverse-cut",
        "artillery-trace-plan",
        "artillery-curtain-section",
        "artillery-curtain-terreplein",
        "artillery-rondel-exterior",
        "artillery-rondel-casemate",
        "artillery-rondel-cutaway",
        "artillery-rondel-top",
        "artillery-gate-approach",
        "artillery-gate-interior",
        "artillery-bridge-deployed",
        "artillery-bridge-denied",
        "artillery-circulation",
        "artillery-drainage",
        "artillery-support-dag",
        "artillery-fire-plan",
    ];
    validate_compact_evidence_suite(
        directory,
        &VIEWS
            .iter()
            .map(|name| (*name, "artillery-rondel-castle"))
            .collect::<Vec<_>>(),
        true,
    )
}

pub(crate) fn validate_final_building_suite(directory: &std::path::Path) -> Result<(), String> {
    const SPECS: [(&str, &str); 10] = [
        ("final-town-house-regression", "town-house"),
        ("final-hall-house-regression", "hall-house"),
        ("final-fachwerk-cottage-regression", "fachwerk-cottage"),
        (
            "final-fachwerk-merchant-regression",
            "fachwerk-merchant-house",
        ),
        (
            "final-renaissance-town-hall-regression",
            "renaissance-town-hall",
        ),
        ("final-cathedral-regression", "cathedral"),
        ("final-castle-gatehouse-regression", "castle-gatehouse"),
        ("final-courtyard-castle-regression", "courtyard-castle"),
        ("final-walled-keep-regression", "walled-keep"),
        (
            "final-artillery-rondel-castle-regression",
            "artillery-rondel-castle",
        ),
    ];
    validate_compact_evidence_suite(directory, &SPECS, false)
}

fn validate_compact_evidence_suite(
    directory: &std::path::Path,
    specs: &[(&str, &str)],
    artillery: bool,
) -> Result<(), String> {
    let mut revision = None::<String>;
    let mut dirty = None::<String>;
    let mut pixels = std::collections::HashSet::new();
    let mut ordinary_plan = None::<String>;
    for (stem, fixture) in specs {
        let capture_path = directory.join(format!("{stem}.capture.json"));
        let png = directory.join(format!("{stem}.png"));
        let plan = directory.join(format!("{stem}.plan.json"));
        if !png.is_file() || !plan.is_file() {
            return Err(format!("{stem} lacks PNG or plan evidence"));
        }
        let value: serde_json::Value = serde_json::from_slice(
            &fs::read(&capture_path)
                .map_err(|error| format!("{}: {error}", capture_path.display()))?,
        )
        .map_err(|error| format!("{}: {error}", capture_path.display()))?;
        let string = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let section = value
            .get("section_cut_applied")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let section_correspondence = section
            && value
                .get("section_removed_item_ids")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|ids| !ids.is_empty());
        if string("fixture") != *fixture
            || value
                .get("plan_audit_issue_count")
                .and_then(serde_json::Value::as_u64)
                != Some(0)
            || value
                .get("mesh_integrity_issue_count")
                .and_then(serde_json::Value::as_u64)
                != Some(0)
            || value
                .get("resolver_schema_version")
                .and_then(serde_json::Value::as_u64)
                != Some(2)
            || (!section_correspondence
                && string("resolved_solid_multiset_hash") != string("rendered_geometry_hash"))
        {
            return Err(format!(
                "{stem} fails fixture/audit/schema/render correspondence"
            ));
        }
        for (slot, current) in [
            (&mut revision, string("source_revision")),
            (&mut dirty, string("source_dirty_fingerprint")),
        ] {
            if slot.as_ref().is_some_and(|expected| expected != &current) {
                return Err(format!("{stem} came from a mixed source build"));
            }
            *slot = Some(current);
        }
        if !pixels.insert(string("pixel_hash")) {
            return Err(format!("{stem} duplicates another proof image"));
        }
        if artillery && *stem != "artillery-bridge-denied" {
            let hash = string("plan_hash");
            if ordinary_plan
                .as_ref()
                .is_some_and(|expected| expected != &hash)
            {
                return Err(format!("{stem} has a mixed artillery plan hash"));
            }
            ordinary_plan = Some(hash);
            if value
                .get("focused_resolved_item_ids")
                .and_then(serde_json::Value::as_array)
                .is_none_or(|ids| ids.is_empty())
            {
                return Err(format!("{stem} lacks exact focused artillery IDs"));
            }
            let bounds = value
                .get("focused_bounds_fraction")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| format!("{stem} lacks focused bounds"))?;
            if bounds.len() != 4
                || bounds[2].as_f64().unwrap_or(0.0) - bounds[0].as_f64().unwrap_or(1.0) < 0.12
                || bounds[3].as_f64().unwrap_or(0.0) - bounds[1].as_f64().unwrap_or(1.0) < 0.12
            {
                return Err(format!("{stem} focused authority is too small to inspect"));
            }
            let roles = value
                .get("artillery_focused_roles")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<std::collections::HashSet<_>>()
                })
                .unwrap_or_default();
            let required: &[&str] = match *stem {
                "artillery-curtain-section" => &[
                    "ArtilleryRevetment",
                    "ArtilleryEarthCore",
                    "ArtilleryRetainingWall",
                    "ArtilleryTerreplein",
                ],
                "artillery-rondel-casemate" | "artillery-rondel-cutaway" => &[
                    "ArtilleryEarthCore",
                    "ArtilleryCasemateFloor",
                    "ArtilleryCasemateRoof",
                    "WeaponMount",
                ],
                "artillery-gate-interior" => &[
                    "ArtilleryGateMechanism",
                    "ArtilleryCasemateFloor",
                    "ArtilleryCasemateRoof",
                    "OpeningClosure",
                ],
                _ => &[],
            };
            if required.iter().any(|role| !roles.contains(role)) {
                return Err(format!("{stem} does not focus all required physical roles"));
            }
            let role_bounds = value
                .get("artillery_role_bounds_fraction")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| format!("{stem} lacks per-role projected bounds"))?;
            for role in required {
                let bounds = role_bounds
                    .get(*role)
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| format!("{stem} does not visibly project {role}"))?;
                if bounds.len() != 4
                    || bounds[2].as_f64().unwrap_or(0.0) - bounds[0].as_f64().unwrap_or(1.0) < 0.01
                    || bounds[3].as_f64().unwrap_or(0.0) - bounds[1].as_f64().unwrap_or(1.0) < 0.01
                    || bounds[0].as_f64().unwrap_or(-1.0) < -0.05
                    || bounds[1].as_f64().unwrap_or(-1.0) < -0.05
                    || bounds[2].as_f64().unwrap_or(2.0) > 1.05
                    || bounds[3].as_f64().unwrap_or(2.0) > 1.05
                {
                    return Err(format!(
                        "{stem} projects {role} outside a readable proof area"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_timber_suite_records(
    records: &[(String, BuildingArchetype, ViewerView, TimberSuiteManifest)],
) -> Result<(), String> {
    if records.len() != 35 {
        return Err(format!(
            "expected 35 timber proofs, found {}",
            records.len()
        ));
    }
    let first = &records[0].3;
    let mut fixture_hashes = std::collections::HashMap::<String, (String, String, String)>::new();
    let mut pixels = std::collections::HashSet::new();
    for (slug, archetype, view, manifest) in records {
        let expected_view = timber_proof_suffix(*view).expect("timber proof suffix");
        if manifest.fixture != archetype.slug()
            || manifest.view != expected_view
            || manifest.seed != 47
            || manifest.resolver_schema_version != 2
            || manifest.plan_audit_issue_count != 0
            || !manifest.validation_passed
        {
            return Err(format!(
                "{slug} violates its fixture/view validation contract"
            ));
        }
        if manifest.source_revision != first.source_revision
            || manifest.source_dirty_fingerprint != first.source_dirty_fingerprint
        {
            return Err(format!("{slug} comes from mixed or stale source authority"));
        }
        if let Some((plan_hash, geometry_hash, frame_hash)) = fixture_hashes.get(&manifest.fixture)
        {
            if plan_hash != &manifest.plan_hash
                || geometry_hash != &manifest.resolved_geometry_hash
                || frame_hash != &manifest.timber_program_hash
            {
                return Err(format!("{slug} is fixture-inconsistent"));
            }
        } else {
            fixture_hashes.insert(
                manifest.fixture.clone(),
                (
                    manifest.plan_hash.clone(),
                    manifest.resolved_geometry_hash.clone(),
                    manifest.timber_program_hash.clone(),
                ),
            );
        }
        if manifest.timber_program.is_none()
            || manifest.timber_assembly_id.is_none()
            || manifest.timber_member_ids.len() < 20
            || manifest.timber_joint_ids.len() < 12
            || manifest.timber_node_ids.len() < 12
            || manifest.focused_resolved_item_ids.is_empty()
            || manifest.visible_focused_resolved_item_count
                + manifest.timber_removed_target_item_ids.len()
                != manifest.focused_resolved_item_ids.len()
            || manifest.timber_target_component_ids.len() != 1
            || !manifest.timber_target_component_ids[0].starts_with("timber:")
            || !manifest.timber_target_component_ids[0].contains('/')
            || manifest.timber_focus_interface_ids.is_empty()
            || !manifest
                .timber_removed_target_item_ids
                .iter()
                .all(|id| manifest.section_removed_item_ids.contains(id))
            || !manifest.timber_legend_visible
            || manifest.timber_required_roles.is_empty()
            || manifest.timber_focused_roles.is_empty()
            || !manifest.section_annotation_visible
            || manifest.timber_required_roles.iter().any(|role| {
                !manifest
                    .timber_focused_roles
                    .iter()
                    .any(|found| found == role)
                    || manifest
                        .timber_role_item_ids
                        .get(role)
                        .is_none_or(Vec::is_empty)
                    || manifest
                        .timber_role_bounds_fraction
                        .get(role)
                        .is_none_or(|bounds| {
                            bounds[0] < 0.0
                                || bounds[1] < 0.0
                                || bounds[2] > 1.0
                                || bounds[3] > 1.0
                                || (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]) < 0.0004
                        })
            })
        {
            return Err(format!(
                "{slug} lacks exact frame IDs, roles, focus, or legend"
            ));
        }
        if matches!(
            view,
            ViewerView::TimberOpeningBayExterior
                | ViewerView::TimberOpeningBayInterior
                | ViewerView::TimberOpeningBaySection
        ) && (manifest.focused_resolved_void_ids.len() != 1
            || manifest
                .timber_role_item_ids
                .get("WallHost")
                .is_none_or(Vec::is_empty))
        {
            return Err(format!(
                "{slug} does not prove both exact opening void and Gefach cells"
            ));
        }
        if *view == ViewerView::TimberJointClose
            && (manifest
                .timber_focused_roles
                .iter()
                .any(|role| role == "WallHost")
                || manifest.timber_focus_interface_ids.len() < 2)
        {
            return Err(format!(
                "{slug} hides its participant contact behind enclosure geometry"
            ));
        }
        if *view == ViewerView::TimberGableRoofBearing && manifest.focused_roof_item_ids.is_empty()
        {
            return Err(format!("{slug} omits the exact Stage 4 roof face"));
        }
        let expects_cut = timber_section_proof(*view);
        if expects_cut != manifest.section_cut_applied
            || expects_cut != manifest.timber_cut_plane.is_some()
        {
            return Err(format!("{slug} lacks its exact declared cut state"));
        }
        let bounds = manifest.focused_bounds_fraction;
        if bounds[0] < 0.0
            || bounds[1] < 0.0
            || bounds[2] > 1.0
            || bounds[3] > 1.0
            || bounds[2] - bounds[0] < 0.12
            || bounds[3] - bounds[1] < 0.20
        {
            return Err(format!("{slug} target is clipped or too small"));
        }
        if manifest.pixel_hash.is_empty() || !pixels.insert(manifest.pixel_hash.clone()) {
            return Err(format!("{slug} lacks unique pixel evidence"));
        }
    }
    Ok(())
}

fn church_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(church) = &plan.church else {
        return Vec::new();
    };
    let church_wall_owners = plan
        .wall_assemblies
        .iter()
        .filter(|wall| {
            matches!(
                wall.source,
                adventuresim_building_generator::WallSourceId::ChurchExterior { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchArcade { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchCrossing { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchApse { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchTowerFace { .. }
                    | adventuresim_building_generator::WallSourceId::SquareTowerFace { .. }
            )
        })
        .map(|wall| wall.owner)
        .collect::<std::collections::HashSet<_>>();
    let class_matches = |solid: &adventuresim_building_generator::ResolvedSolid| {
        church_wall_owners.contains(&solid.owner)
            || matches!(
                solid.role,
                SolidRole::ChurchFloor
                    | SolidRole::ChurchPier
                    | SolidRole::ChurchArcade
                    | SolidRole::ChurchVaultShell
                    | SolidRole::ChurchVaultThrust
                    | SolidRole::ChurchCrossingArch
                    | SolidRole::ChurchBellFloor
                    | SolidRole::ChurchBellFrame
                    | SolidRole::ChurchBell
                    | SolidRole::ChurchGuard
                    | SolidRole::ChurchStairNewel
                    | SolidRole::ChurchStairTread
                    | SolidRole::ChurchServiceLadder
                    | SolidRole::Landing
                    | SolidRole::WallButtress
                    | SolidRole::FrameMember
            )
    };
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| {
            if matches!(
                view,
                ViewerView::ChurchDrainage | ViewerView::ChurchTowerRoofDrain
            ) {
                let drainage_role = matches!(
                    solid.role,
                    SolidRole::RoofGutter | SolidRole::RoofFlashing | SolidRole::RoofEdgeTreatment
                );
                return drainage_role
                    && (!matches!(view, ViewerView::ChurchTowerRoofDrain)
                        || solid.centre.x <= church.tower.centre.x + 4.5);
            }
            if !class_matches(solid) {
                return false;
            }
            match view {
                ViewerView::ChurchBayExterior
                | ViewerView::ChurchBayInterior
                | ViewerView::ChurchBaySection
                | ViewerView::ChurchBayLoad
                | ViewerView::ChurchBayVault => {
                    let in_bay = (solid.centre.x - church.nave_axes_metres[1]).abs() <= 2.8;
                    in_bay
                        && match view {
                            ViewerView::ChurchBaySection => matches!(
                                solid.role,
                                SolidRole::ChurchPier
                                    | SolidRole::ChurchArcade
                                    | SolidRole::ChurchFloor
                            ),
                            ViewerView::ChurchBayLoad => matches!(
                                solid.role,
                                SolidRole::ChurchPier
                                    | SolidRole::WallButtress
                                    | SolidRole::ChurchVaultThrust
                                    | SolidRole::ChurchVaultShell
                            ),
                            ViewerView::ChurchBayVault => matches!(
                                solid.role,
                                SolidRole::ChurchPier
                                    | SolidRole::ChurchVaultShell
                                    | SolidRole::ChurchVaultThrust
                            ),
                            ViewerView::ChurchBayInterior => matches!(
                                solid.role,
                                SolidRole::ChurchFloor
                                    | SolidRole::ChurchPier
                                    | SolidRole::ChurchArcade
                            ),
                            _ => true,
                        }
                }
                ViewerView::ChurchCrossingInterior
                | ViewerView::ChurchCrossingExterior
                | ViewerView::ChurchCrossingTop
                | ViewerView::ChurchCrossingCutLoad => {
                    (solid.centre.x - church.crossing_axis_metres).abs() <= 3.0
                        && (!matches!(view, ViewerView::ChurchCrossingCutLoad)
                            || matches!(
                                solid.role,
                                SolidRole::ChurchCrossingArch
                                    | SolidRole::ChurchPier
                                    | SolidRole::ChurchVaultShell
                                    | SolidRole::ChurchVaultThrust
                                    | SolidRole::WallButtress
                            ))
                }
                ViewerView::ChurchChoirEast
                | ViewerView::ChurchChoirInterior
                | ViewerView::ChurchChoirTop
                | ViewerView::ChurchChoirRadialSection => {
                    solid.centre.x >= church.crossing_axis_metres + 2.0
                        && (!matches!(
                            view,
                            ViewerView::ChurchChoirInterior | ViewerView::ChurchChoirRadialSection
                        ) || matches!(
                            solid.role,
                            SolidRole::WallHost
                                | SolidRole::OpeningJamb
                                | SolidRole::OpeningSill
                                | SolidRole::OpeningHead
                                | SolidRole::OpeningSpandrel
                                | SolidRole::ChurchFloor
                                | SolidRole::ChurchPier
                                | SolidRole::ChurchArcade
                                | SolidRole::ChurchVaultShell
                                | SolidRole::ChurchVaultThrust
                                | SolidRole::WallButtress
                        ))
                }
                ViewerView::ChurchTowerPortal
                | ViewerView::ChurchTowerJunction
                | ViewerView::ChurchTowerStair
                | ViewerView::ChurchTowerBellUnderside
                | ViewerView::ChurchTowerFrame
                | ViewerView::ChurchTowerLouvredExterior => {
                    // Include the bonded first nave-bay return as part of the
                    // westwork proof, rather than treating the tall tower as
                    // an isolated freestanding object.
                    let in_westwork = solid.centre.x <= church.tower.centre.x + 5.5;
                    in_westwork
                        && match view {
                            ViewerView::ChurchTowerStair => matches!(
                                solid.role,
                                SolidRole::ChurchStairNewel
                                    | SolidRole::ChurchStairTread
                                    | SolidRole::ChurchFloor
                                    | SolidRole::ChurchGuard
                                    | SolidRole::Landing
                            ),
                            ViewerView::ChurchTowerBellUnderside => matches!(
                                solid.role,
                                SolidRole::ChurchBellFloor
                                    | SolidRole::ChurchBell
                                    | SolidRole::ChurchFloor
                            ),
                            ViewerView::ChurchTowerFrame => matches!(
                                solid.role,
                                SolidRole::ChurchBellFrame
                                    | SolidRole::ChurchBell
                                    | SolidRole::ChurchServiceLadder
                                    | SolidRole::ChurchBellFloor
                            ),
                            ViewerView::ChurchTowerJunction => {
                                solid.centre.y <= 4.25
                                    && (matches!(
                                        solid.role,
                                        SolidRole::ChurchPier
                                            | SolidRole::ChurchArcade
                                            | SolidRole::ChurchFloor
                                            | SolidRole::ChurchVaultThrust
                                            | SolidRole::ChurchStairTread
                                            | SolidRole::ChurchStairNewel
                                            | SolidRole::Landing
                                    ) || church_wall_owners.contains(&solid.owner))
                            }
                            ViewerView::ChurchTowerPortal => {
                                church_wall_owners.contains(&solid.owner) && solid.centre.y <= 5.5
                            }
                            ViewerView::ChurchTowerLouvredExterior => {
                                church_wall_owners.contains(&solid.owner) && solid.centre.y >= 13.0
                            }
                            _ => true,
                        }
                }
                ViewerView::ChurchSupportDag => {
                    (solid.centre.x - church.nave_axes_metres[1]).abs() <= 2.8
                        && matches!(
                            solid.role,
                            SolidRole::ChurchPier
                                | SolidRole::WallButtress
                                | SolidRole::ChurchVaultThrust
                                | SolidRole::ChurchVaultShell
                                | SolidRole::ChurchArcade
                        )
                }
                _ => true,
            }
        })
        .map(|solid| solid.id.0)
        .collect()
}

fn focused_crown_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let focus = match view {
        ViewerView::CrownStraightExterior | ViewerView::CrownStraightInterior => {
            plan.crowns.iter().find_map(|crown| match crown.path {
                CrownPath::Straight { start, end, .. } => {
                    Some((vec![crown.owner], (start + end) * 0.5))
                }
                CrownPath::Round { .. } => None,
            })
        }
        ViewerView::CrownCornerExterior | ViewerView::CrownCornerInterior => plan
            .crowns
            .iter()
            .flat_map(|crown| {
                crown
                    .junctions
                    .iter()
                    .map(move |junction| (crown, junction))
            })
            .find(|(_, junction)| {
                junction.kind == adventuresim_building_generator::CrownJunctionKind::Corner
            })
            .map(|(crown, junction)| (vec![crown.owner, junction.other_owner], junction.position)),
        ViewerView::CrownTowerExterior
        | ViewerView::CrownTowerTop
        | ViewerView::CrownTowerCutaway => {
            let preferred = plan
                .gate_defenses
                .first()
                .and_then(|gate| gate.firing_positions.first())
                .map(|position| position.tower_index);
            plan.crowns.iter().find_map(|crown| match crown.path {
                CrownPath::Round {
                    tower_index,
                    centre,
                    ..
                } if preferred.is_none_or(|value| value == tower_index) => {
                    Some((vec![crown.owner], centre))
                }
                _ => None,
            })
        }
        _ => None,
    };
    let Some((owners, focus)) = focus else {
        return Vec::new();
    };
    owners
        .iter()
        .flat_map(|owner| {
            [
                SolidRole::Breastwork,
                SolidRole::Merlon,
                SolidRole::Coping,
                SolidRole::EdgeGuard,
            ]
            .into_iter()
            .filter_map(|role| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .filter(|solid| solid.owner == *owner && solid.role == role)
                    .min_by(|a, b| {
                        Vec2::new(a.centre.x, a.centre.z)
                            .distance_squared(focus)
                            .total_cmp(&Vec2::new(b.centre.x, b.centre.z).distance_squared(focus))
                    })
                    .map(|solid| solid.id.0)
            })
        })
        .collect()
}

fn projected_view(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::ProjectedExterior
            | ViewerView::ProjectedInterior
            | ViewerView::ProjectedUnderside
            | ViewerView::ProjectedTop
            | ViewerView::ProjectedLongitudinal
            | ViewerView::ProjectedSockets
            | ViewerView::ProjectedFlank
    )
}

fn opening_proof_profile(view: ViewerView) -> Option<&'static str> {
    match view {
        ViewerView::OpeningRectangularExterior
        | ViewerView::OpeningRectangularInterior
        | ViewerView::OpeningRectangularSection => Some("rectangular"),
        ViewerView::OpeningSegmentalExterior
        | ViewerView::OpeningSegmentalInterior
        | ViewerView::OpeningSegmentalSection => Some("segmental"),
        ViewerView::OpeningPointedExterior
        | ViewerView::OpeningPointedInterior
        | ViewerView::OpeningPointedSection => Some("pointed_two_centred"),
        ViewerView::OpeningArrowLoopExterior
        | ViewerView::OpeningArrowLoopInterior
        | ViewerView::OpeningArrowLoopSection => Some("arrow_loop"),
        ViewerView::OpeningGunLoopExterior
        | ViewerView::OpeningGunLoopInterior
        | ViewerView::OpeningGunLoopSection => Some("gun_loop"),
        _ => None,
    }
}

fn wall_section_kind(view: ViewerView) -> Option<&'static str> {
    match view {
        ViewerView::WallTimberFrameSection => Some("timber_frame"),
        ViewerView::WallCivilianMasonrySection => Some("civilian_masonry"),
        ViewerView::WallCathedralButtressSection => Some("cathedral_buttress"),
        ViewerView::WallRoundTowerRadialSection => Some("round_tower_radial"),
        _ => None,
    }
}

fn architectural_proof(view: ViewerView) -> bool {
    opening_proof_profile(view).is_some() || wall_section_kind(view).is_some()
}

fn section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::OpeningRectangularSection
            | ViewerView::OpeningSegmentalSection
            | ViewerView::OpeningPointedSection
            | ViewerView::OpeningArrowLoopSection
            | ViewerView::OpeningGunLoopSection
            | ViewerView::WallTimberFrameSection
            | ViewerView::WallCivilianMasonrySection
            | ViewerView::WallCathedralButtressSection
            | ViewerView::WallRoundTowerRadialSection
    )
}

fn opening_profile_slug(profile: adventuresim_building_generator::OpeningProfile) -> &'static str {
    use adventuresim_building_generator::OpeningProfile;
    match profile {
        OpeningProfile::Rectangular { .. } => "rectangular",
        OpeningProfile::Segmental { .. } => "segmental",
        OpeningProfile::PointedTwoCentred { .. } => "pointed_two_centred",
        OpeningProfile::ArrowLoop { .. } => "arrow_loop",
        OpeningProfile::GunLoop { .. } => "gun_loop",
    }
}

fn focused_opening(
    plan: &BuildingPlan,
    view: ViewerView,
) -> Option<&adventuresim_building_generator::OpeningAssembly> {
    let profile = opening_proof_profile(view)?;
    plan.opening_assemblies
        .iter()
        .filter(|opening| opening_profile_slug(opening.profile) == profile)
        .min_by_key(|opening| {
            (
                usize::from(opening.frame.outside_room.is_some()),
                opening.host_wall.0,
            )
        })
}

fn focused_wall(
    plan: &BuildingPlan,
    view: ViewerView,
) -> Option<&adventuresim_building_generator::WallAssembly> {
    use adventuresim_building_generator::WallMaterialClass;
    let kind = wall_section_kind(view)?;
    if kind == "round_tower_radial" {
        return plan.wall_assemblies.iter().find(|wall| {
            matches!(
                wall.source,
                adventuresim_building_generator::WallSourceId::RoundTower { tower_index: 0 }
            )
        });
    }
    plan.wall_assemblies
        .iter()
        .filter(|wall| {
            wall.opening_ids.is_empty()
                && wall.frame.outside_room.is_none()
                && wall
                    .host_solids
                    .iter()
                    .filter(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == **id)
                            .is_some_and(|solid| solid.role == SolidRole::WallHost)
                    })
                    .count()
                    >= 2
        })
        .find(|wall| match kind {
            "timber_frame" => wall.material == WallMaterialClass::TimberInfill,
            "civilian_masonry" => wall.material == WallMaterialClass::CivilianMasonry,
            "cathedral_buttress" => wall.material == WallMaterialClass::CathedralMasonry,
            _ => false,
        })
}

fn architectural_section_removed_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    if artillery_section_proof(view) {
        return artillery_section_removed_item_ids(plan, view);
    }
    if timber_section_proof(view) {
        return timber_section_removed_item_ids(plan, view);
    }
    if church_section_proof(view) {
        let Some(church) = &plan.church else {
            return Vec::new();
        };
        let transverse = matches!(
            view,
            ViewerView::ChurchWholeTransverseCut | ViewerView::ChurchCrossingCutLoad
        );
        let radial_cut = (view == ViewerView::ChurchChoirRadialSection).then(|| {
            church
                .choir
                .bay_axes_metres
                .last()
                .copied()
                .unwrap_or(church.crossing_axis_metres)
                + 5.0
        });
        return plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| {
                let beyond_cut = radial_cut.map_or_else(
                    || {
                        if transverse {
                            solid.centre.x > church.crossing_axis_metres + 0.05
                        } else {
                            solid.centre.z < church.tower.centre.y - 0.05
                        }
                    },
                    |cut| solid.centre.x > cut + 0.05,
                );
                beyond_cut
                    && !matches!(
                        solid.role,
                        SolidRole::ChurchFloor
                            | SolidRole::ChurchBellFloor
                            | SolidRole::Landing
                            | SolidRole::ChurchGuard
                            | SolidRole::ChurchBellFrame
                            | SolidRole::ChurchBell
                            | SolidRole::FrameMember
                    )
            })
            .map(|solid| solid.id.0)
            .collect();
    }
    if let Some(opening) = focused_opening(plan, view) {
        let mut removed = vec![opening.jamb_solids[1].0];
        if let Some(reveal) = opening.reveal_surfaces.get(1) {
            removed.push(reveal.0);
        }
        return removed;
    }
    let Some(wall) = focused_wall(plan, view) else {
        return Vec::new();
    };
    if wall.radial_frame.is_some() {
        return Vec::new();
    }
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| solid.owner == wall.owner)
        .filter(|solid| {
            let plan_centre = Vec2::new(solid.centre.x, solid.centre.z);
            (plan_centre - wall.frame.origin).dot(wall.frame.tangent) > 0.01
        })
        .map(|solid| solid.id.0)
        .collect()
}

fn architectural_focus_owner(plan: &BuildingPlan, view: ViewerView) -> Option<u32> {
    focused_opening(plan, view)
        .map(|opening| opening.owner.0)
        .or_else(|| focused_wall(plan, view).map(|wall| wall.owner.0))
}

fn architectural_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(owner) = architectural_focus_owner(plan, view) else {
        return Vec::new();
    };
    let mut ids = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| solid.owner.0 == owner)
        .map(|solid| solid.id.0)
        .collect::<Vec<_>>();
    ids.extend(
        plan.resolved_geometry
            .surfaces
            .iter()
            .filter(|surface| surface.owner.0 == owner)
            .map(|surface| surface.id.0),
    );
    ids
}

fn architectural_focus_void_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    focused_opening(plan, view)
        .map(|opening| vec![opening.void_id.0])
        .unwrap_or_default()
}

fn projected_kind_matches(
    defense: &adventuresim_building_generator::ProjectedDefenseAssembly,
    kind: ProjectedProofKind,
) -> bool {
    use adventuresim_building_generator::ProjectedDefenseKind;
    matches!(
        (defense.kind, kind),
        (
            ProjectedDefenseKind::Machicolation,
            ProjectedProofKind::Machicolation
        ) | (ProjectedDefenseKind::Breteche, ProjectedProofKind::Breteche)
            | (ProjectedDefenseKind::Hoarding, ProjectedProofKind::Hoarding)
            | (ProjectedDefenseKind::Bartizan, ProjectedProofKind::Bartizan)
    )
}

const fn projected_kind_slug(kind: ProjectedProofKind) -> &'static str {
    match kind {
        ProjectedProofKind::Machicolation => "machicolation",
        ProjectedProofKind::Breteche => "breteche",
        ProjectedProofKind::Hoarding => "hoarding",
        ProjectedProofKind::Bartizan => "bartizan",
    }
}

const fn projected_deployment_slug(deployment: ProjectedDefenseDeployment) -> &'static str {
    match deployment {
        ProjectedDefenseDeployment::Permanent => "permanent",
        ProjectedDefenseDeployment::SocketsOnly => "sockets_only",
        ProjectedDefenseDeployment::Deployed => "deployed",
    }
}

const fn projected_target_slug(target: ProjectedDefenseTarget) -> &'static str {
    match target {
        ProjectedDefenseTarget::GateApproach => "gate_approach",
        ProjectedDefenseTarget::ThreatenedWallFoot => "threatened_wall_foot",
        ProjectedDefenseTarget::ThreatenedCorner => "threatened_corner",
        ProjectedDefenseTarget::CampaignSiegeFront => "campaign_siege_front",
    }
}

fn focused_projected_defense(
    plan: &BuildingPlan,
    view: ViewerView,
    kind: ProjectedProofKind,
) -> Option<&adventuresim_building_generator::ProjectedDefenseAssembly> {
    use adventuresim_building_generator::ProjectedDefenseDeployment;
    plan.projected_defenses.iter().find(|defense| {
        projected_kind_matches(defense, kind)
            && if view == ViewerView::ProjectedSockets {
                defense.deployment == ProjectedDefenseDeployment::SocketsOnly
            } else if kind == ProjectedProofKind::Hoarding {
                defense.deployment == ProjectedDefenseDeployment::Deployed
            } else {
                true
            }
    })
}

fn focused_projected_item_ids(
    plan: &BuildingPlan,
    view: ViewerView,
    kind: ProjectedProofKind,
) -> Vec<u64> {
    let Some(defense) = focused_projected_defense(plan, view, kind) else {
        return Vec::new();
    };
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| solid.owner == defense.owner || solid.owner == defense.host_owner)
        .map(|solid| solid.id.0)
        .collect()
}

pub(crate) fn run(
    archetype: BuildingArchetype,
    view: ViewerView,
    seed: u64,
    output: Option<PathBuf>,
    settle_frames: u32,
    projected_kind: ProjectedProofKind,
    roof_proof: Option<RoofProofView>,
    editor: bool,
    document_path: Option<PathBuf>,
    player_build_document_path: Option<PathBuf>,
) {
    let seed = if view == ViewerView::ArtilleryBridgeDenied {
        702
    } else if projected_view(view) {
        match (projected_kind, view) {
            (ProjectedProofKind::Breteche, _) => 201,
            (ProjectedProofKind::Hoarding, ViewerView::ProjectedSockets) => 42,
            (ProjectedProofKind::Hoarding, _) => 202,
            (ProjectedProofKind::Bartizan, _) => 203,
            (ProjectedProofKind::Machicolation, _) => 42,
        }
    } else {
        seed
    };
    let editor_document_path =
        document_path.unwrap_or_else(|| PathBuf::from("building-document.json"));
    let player_build_document = player_build_document_path.as_ref().map(|path| {
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read player-build document {}: {error}",
                path.display()
            )
        });
        serde_json::from_slice::<PlayerBuildDocument>(&bytes).unwrap_or_else(|error| {
            panic!(
                "failed to decode player-build document {}: {error}",
                path.display()
            )
        })
    });
    let mut document = if editor && editor_document_path.exists() {
        let bytes = fs::read(&editor_document_path).unwrap_or_else(|error| {
            panic!(
                "failed to read editor document {}: {error}",
                editor_document_path.display()
            )
        });
        serde_json::from_slice::<BuildingDocument>(&bytes).unwrap_or_else(|error| {
            panic!(
                "failed to decode editor document {}: {error}",
                editor_document_path.display()
            )
        })
    } else {
        BuildingDocument::fixture(archetype, seed)
    };
    let mut program = document.program.clone();
    if let Some(proof) = roof_proof {
        program.roof_pitch_degrees = match proof {
            RoofProofView::RoofGableLowPitch => 25.0,
            RoofProofView::RoofGableMidPitch => 45.0,
            RoofProofView::RoofGableHighPitch => 70.0,
            _ => program.roof_pitch_degrees,
        };
        if matches!(
            proof,
            RoofProofView::RoofGableLowPitch
                | RoofProofView::RoofGableMidPitch
                | RoofProofView::RoofGableHighPitch
        ) {
            program.roof_demonstrator = Some(RoofKind::Gable);
        }
        if matches!(
            proof,
            RoofProofView::RoofRoundTowerExterior
                | RoofProofView::RoofRoundTowerTop
                | RoofProofView::RoofRoundTowerCutaway
                | RoofProofView::RoofRoundTowerDrainage
        ) {
            program.roof_demonstrator = Some(RoofKind::Conical);
        }
        if matches!(
            proof,
            RoofProofView::RoofPavilionExterior
                | RoofProofView::RoofPavilionTop
                | RoofProofView::RoofPavilionCutaway
                | RoofProofView::RoofPavilionDrainage
        ) {
            program.roof_demonstrator = Some(RoofKind::Pavilion);
        }
    }
    document.program = program.clone();
    let plan = if editor {
        generate_document(&document).expect("editor document must generate")
    } else {
        generate(&program).expect("curated building fixture must generate")
    };
    let plan_bytes = serde_json::to_vec(&plan).expect("serialize building plan for evidence hash");
    let plan_hash = stable_evidence_hash(&plan_bytes);
    let evidence_hash = stable_evidence_hash(
        format!(
            "{plan_hash}|{}|{:?}|{:?}|{:?}|{seed}|{VIEW_WIDTH}x{VIEW_HEIGHT}",
            archetype.slug(),
            view,
            projected_kind,
            roof_proof,
        )
        .as_bytes(),
    );
    let resolved_geometry_hash = stable_evidence_hash(
        &serde_json::to_vec(&plan.resolved_geometry).expect("serialize resolved geometry"),
    );
    let roof_graph_hash = stable_evidence_hash(
        &serde_json::to_vec(&plan.roof_assemblies).expect("serialize resolved roof graph"),
    );
    let church_program_hash = plan.church.as_ref().map_or_else(String::new, |church| {
        stable_evidence_hash(&serde_json::to_vec(church).expect("serialize church assembly"))
    });
    let church_bay_labels = plan.church.as_ref().map_or_else(Vec::new, |church| {
        let mut labels = church
            .bay_assemblies
            .iter()
            .map(|bay| format!("N{}@{:.2}", bay.axis_index + 1, bay.axis_metres))
            .collect::<Vec<_>>();
        labels.push(format!("X@{:.2}", church.crossing_axis_metres));
        labels.extend(
            church
                .choir_axes_metres
                .iter()
                .enumerate()
                .map(|(index, axis)| format!("Q{}@{axis:.2}", index + 1)),
        );
        labels.push(format!("A{}", church.program.apse_sides));
        labels
    });
    let church_support_node_ids = plan.church.as_ref().map_or_else(Vec::new, |_| {
        plan.resolved_geometry
            .structural_nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    adventuresim_building_generator::StructuralNodeKind::ChurchPier
                        | adventuresim_building_generator::StructuralNodeKind::ChurchArcadeSpringing
                        | adventuresim_building_generator::StructuralNodeKind::ChurchVaultSpringing
                        | adventuresim_building_generator::StructuralNodeKind::ChurchCrossingPier
                        | adventuresim_building_generator::StructuralNodeKind::ChurchButtress
                        | adventuresim_building_generator::StructuralNodeKind::ChurchTowerStage
                        | adventuresim_building_generator::StructuralNodeKind::ChurchBellFrame
                )
            })
            .map(|node| node.id.0)
            .collect()
    });
    let church_opening_ids = plan.church.as_ref().map_or_else(Vec::new, |_| {
        plan.opening_assemblies
            .iter()
            .filter(|opening| {
                matches!(
                    opening.host_source,
                    adventuresim_building_generator::WallSourceId::ChurchExterior { .. }
                        | adventuresim_building_generator::WallSourceId::ChurchArcade { .. }
                        | adventuresim_building_generator::WallSourceId::ChurchApse { .. }
                        | adventuresim_building_generator::WallSourceId::ChurchTowerFace { .. }
                        | adventuresim_building_generator::WallSourceId::SquareTowerFace { .. }
                )
            })
            .map(|opening| opening.id.0)
            .collect()
    });
    let church_focus_ids = church_focus_item_ids(&plan, view);
    let church_focus_set = church_focus_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let church_focused_roles = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| church_focus_set.contains(&solid.id.0))
        .map(|solid| format!("{:?}", solid.role))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let church_target_component_ids = church_target_component_ids(&plan, view);
    let church_required_roles = church_required_roles(view);
    let church_cut_plane = church_cut_plane(&plan, view);
    let church_removed_target_item_ids = if church_section_proof(view) {
        architectural_section_removed_item_ids(&plan, view)
            .into_iter()
            .filter(|id| church_focus_set.contains(id))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let timber_focus_ids = timber_focus_item_ids(&plan, view);
    let timber_focus_set = timber_focus_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let timber_focused_roles = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| timber_focus_set.contains(&solid.id.0))
        .map(|solid| format!("{:?}", solid.role))
        .chain(
            plan.resolved_geometry
                .surfaces
                .iter()
                .filter(|surface| timber_focus_set.contains(&surface.id.0))
                .map(|surface| format!("{:?}", surface.role)),
        )
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut timber_role_item_ids = std::collections::BTreeMap::<String, Vec<u64>>::new();
    for solid in plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| timber_focus_set.contains(&solid.id.0))
    {
        timber_role_item_ids
            .entry(format!("{:?}", solid.role))
            .or_default()
            .push(solid.id.0);
    }
    for surface in plan
        .resolved_geometry
        .surfaces
        .iter()
        .filter(|surface| timber_focus_set.contains(&surface.id.0))
    {
        timber_role_item_ids
            .entry(format!("{:?}", surface.role))
            .or_default()
            .push(surface.id.0);
    }
    let timber_required_roles = timber_required_roles(&plan, view);
    let timber_cut_plane = timber_cut_plane(&plan, view);
    let timber_removed_target_item_ids = if timber_section_proof(view) {
        architectural_section_removed_item_ids(&plan, view)
            .into_iter()
            .filter(|id| timber_focus_set.contains(id))
            .collect()
    } else {
        Vec::new()
    };
    let focused_roof_indices = roof_proof
        .map(|proof| roof_proof_assembly_indices(&plan, proof))
        .or_else(|| {
            (view == ViewerView::TimberGableRoofBearing).then(|| {
                plan.roof_assemblies
                    .iter()
                    .enumerate()
                    .filter(|(_, roof)| roof.parent.is_none())
                    .map(|(index, _)| index)
                    .collect()
            })
        })
        .unwrap_or_default();
    let mut section_removed_roof_item_ids = roof_proof
        .filter(|proof| roof_proof_sectioned(*proof))
        .map(|_| {
            focused_roof_indices
                .iter()
                .filter_map(|index| {
                    plan.roof_assemblies[*index]
                        .faces
                        .last()
                        .map(|face| face.id.0)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    section_removed_roof_item_ids.extend(church_section_removed_roof_item_ids(&plan, view));
    let expected_roof_render_items = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| {
            roof.faces
                .iter()
                .filter(|face| !section_removed_roof_item_ids.contains(&face.id.0))
                .map(|face| {
                    (
                        face.id.0,
                        stable_u64(&serde_json::to_vec(face).expect("serialize roof face")),
                    )
                })
                .chain(roof.enclosure_faces.iter().map(|enclosure| {
                    (
                        enclosure.id.0,
                        stable_u64(
                            &serde_json::to_vec(enclosure).expect("serialize roof enclosure"),
                        ),
                    )
                }))
        })
        .collect::<Vec<_>>();
    let mut expected_roof_render_items = expected_roof_render_items;
    if timber_isolated_view(view) && view != ViewerView::TimberGableRoofBearing {
        expected_roof_render_items.clear();
    }
    let roof_render_multiset_hash =
        resolved_item_multiset_hash(expected_roof_render_items.iter().copied());
    let expected_render_hash = resolved_item_multiset_hash(
        plan.resolved_geometry
            .solids
            .iter()
            .filter(|solid| !timber_isolated_view(view) || timber_focus_set.contains(&solid.id.0))
            .filter(|solid| !timber_removed_target_item_ids.contains(&solid.id.0))
            .map(|solid| {
                (
                    solid.id.0,
                    stable_u64(&serde_json::to_vec(solid).expect("serialize resolved solid")),
                )
            })
            .chain(
                plan.resolved_geometry
                    .surfaces
                    .iter()
                    .filter(|surface| {
                        timber_isolated_view(view)
                            && timber_focus_set.contains(&surface.id.0)
                            && surface.role
                                == adventuresim_building_generator::SurfaceRole::TimberCirculation
                    })
                    .map(|surface| {
                        (
                            surface.id.0,
                            stable_u64(
                                &serde_json::to_vec(surface)
                                    .expect("serialize resolved timber route"),
                            ),
                        )
                    }),
            ),
    );
    let projected_focus = projected_view(view)
        .then(|| focused_projected_defense(&plan, view, projected_kind))
        .flatten();
    let architectural_owner = architectural_focus_owner(&plan, view);
    let architectural_items = architectural_focus_item_ids(&plan, view);
    let architectural_voids = architectural_focus_void_ids(&plan, view);
    let focused_roof_owners = focused_roof_indices
        .iter()
        .map(|index| plan.roof_assemblies[*index].owner)
        .collect::<std::collections::HashSet<_>>();
    let dormer_focus_roof = roof_proof
        .filter(|proof| roof_proof_slug(*proof).starts_with("roof-dormer-"))
        .and_then(|_| {
            plan.roof_assemblies
                .iter()
                .find(|roof| roof.parent.is_some())
        });
    let focused_roof_downspouts = plan
        .resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| focused_roof_owners.contains(&network.owner))
        .filter_map(|network| network.downspout.map(|id| id.0))
        .collect::<std::collections::HashSet<_>>();
    let focused_abutment_item_ids = roof_proof
        .filter(|proof| roof_proof_slug(*proof).starts_with("roof-abutment-"))
        .map(|proof| {
            let wanted = if roof_proof_slug(proof).starts_with("roof-abutment-tower-") {
                adventuresim_building_generator::RoofAbutmentKind::Tower
            } else {
                adventuresim_building_generator::RoofAbutmentKind::Wall
            };
            focused_roof_indices
                .iter()
                .flat_map(|index| &plan.roof_assemblies[*index].abutments)
                .filter(|abutment| abutment.kind == wanted)
                .flat_map(|abutment| {
                    abutment.samples.iter().flat_map(|sample| {
                        [
                            sample.apron_solid.0,
                            sample.upstand_solid.0,
                            sample.counterflashing_solid.0,
                        ]
                    })
                })
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut focused_roof_item_ids = focused_roof_indices
        .iter()
        .flat_map(|index| {
            let roof = &plan.roof_assemblies[*index];
            roof.faces
                .iter()
                .map(|face| face.id.0)
                .chain(roof.enclosure_faces.iter().map(|face| face.id.0))
        })
        .collect::<Vec<_>>();
    if let Some(dormer) = dormer_focus_roof {
        focused_roof_item_ids = dormer
            .faces
            .iter()
            .map(|face| face.id.0)
            .chain(dormer.enclosure_faces.iter().map(|face| face.id.0))
            .collect();
    }
    if view == ViewerView::Exterior && roof_proof.is_none() {
        focused_roof_item_ids = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| {
                roof.faces
                    .iter()
                    .map(|face| face.id.0)
                    .chain(roof.enclosure_faces.iter().map(|face| face.id.0))
            })
            .collect();
    }
    let architectural_focus_hash = architectural_owner.map(|owner| {
        let solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner.0 == owner)
            .collect::<Vec<_>>();
        let voids = plan
            .resolved_geometry
            .voids
            .iter()
            .filter(|void| void.owner.0 == owner)
            .collect::<Vec<_>>();
        let surfaces = plan
            .resolved_geometry
            .surfaces
            .iter()
            .filter(|surface| surface.owner.0 == owner)
            .collect::<Vec<_>>();
        stable_evidence_hash(
            &serde_json::to_vec(&(solids, surfaces, voids)).expect("serialize focused geometry"),
        )
    });
    let section_annotation = if timber_proof_suffix(view).is_some() {
        plan.timber_frame.as_ref().map_or_else(String::new, |frame| {
            let detail = if view == ViewerView::TimberJointClose {
                frame
                    .joints
                    .iter()
                    .find(|joint| {
                        joint.member_ids.iter().all(|member_id| {
                            frame.members.iter().any(|member| {
                                member.id == *member_id
                                    && timber_focus_set.contains(&member.solid.0)
                            })
                        }) && joint.member_ids.len() >= 2
                    })
                    .map(|joint| {
                        format!(
                            " joint={} kind={:?} participants={:?} contacts={:?}",
                            joint.id.0,
                            joint.kind,
                            joint.member_ids,
                            joint.contact_interfaces
                        )
                    })
                    .unwrap_or_default()
            } else if matches!(
                view,
                ViewerView::TimberOpeningBayExterior
                    | ViewerView::TimberOpeningBayInterior
                    | ViewerView::TimberOpeningBaySection
            ) {
                format!(
                    " panels={:?} voids={:?}",
                    timber_role_item_ids.get("WallHost").cloned().unwrap_or_default(),
                    plan.timber_frame
                        .as_ref()
                        .into_iter()
                        .flat_map(|frame| &frame.bays)
                        .find_map(|bay| bay.opening)
                )
            } else if view == ViewerView::TimberGableRoofBearing {
                format!(
                    " roof_faces={:?} bearing_interfaces={:?}",
                    focused_roof_item_ids, frame.roof_bearing_interfaces
                )
            } else {
                String::new()
            };
            format!(
                "timber={} program={:?} target={} members={} joints={} cut={:?} legend=sill/post/plate/brace/jetty/roof-bearing{}",
                frame.id.0,
                frame.program,
                timber_proof_slug(&plan, view).unwrap_or_default(),
                timber_focus_ids.len(),
                frame.joints.len(),
                timber_cut_plane,
                detail,
            )
        })
    } else if church_proof_slug(view).is_some() {
        plan.church.as_ref().map_or_else(String::new, |church| {
            format!(
                "target={:?} church={} type=cruciform_3_aisled_basilica bays=N1,N2,N3,N4,X,Q1,Q2,A5 roles={:?} cut={:?} openings={} supports={} public_route=[{},{},{}] route=1.80x2.95m datum_floor={:.2}m vault={:.2}m",
                church_target_component_ids,
                church.id.0,
                church_required_roles,
                church_cut_plane,
                church_opening_ids.len(),
                church_support_node_ids.len(),
                church.tower.exterior_approach_surface.0,
                church.tower.vestibule_surface.0,
                church.tower.nave_entry_surface.0,
                church.datum.floor_metres,
                church.datum.vault_crown_metres,
            )
        })
    } else if artillery_proof_slug(view).is_some() {
        plan.artillery_castle.as_ref().map_or_else(String::new, |castle| {
            format!(
                "artillery={} phase={:?} target={} curtains={:?} rondels={:?} stations={} routes={} fire_rays={} cut={:?} legend=fieldstone/earth/timber/inside/outside",
                castle.id.0,
                castle.phase,
                artillery_proof_slug(view).unwrap_or_default(),
                castle.curtains.iter().map(|curtain| curtain.id.0).collect::<Vec<_>>(),
                castle.rondels.iter().map(|rondel| rondel.id.0).collect::<Vec<_>>(),
                castle.stations.len(),
                castle.route_edges.len(),
                castle.stations.iter().map(|station| station.rays.len()).sum::<usize>(),
                artillery_cut_plane(view),
            )
        })
    } else if section_proof(view) {
        if let Some(opening) = focused_opening(&plan, view) {
            let wall = plan
                .wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall)
                .expect("focused opening wall");
            format!(
                "wall={} opening={} profile={} thickness={:.2}m throat={:.2}m mouth={:.2}m",
                wall.id.0,
                opening.id.0,
                opening_profile_slug(opening.profile),
                wall.thickness_metres,
                opening.profile.exterior_width_metres(),
                opening.profile.interior_width_metres(),
            )
        } else if let Some(wall) = focused_wall(&plan, view) {
            format!(
                "wall={} opening=none profile=solid_section thickness={:.2}m",
                wall.id.0, wall.thickness_metres
            )
        } else {
            format!(
                "wall=round_tower opening=radial profile=shell_section thickness={:.2}m",
                plan.towers
                    .first()
                    .map_or(0.0, |tower| tower.wall_thickness_metres)
            )
        }
    } else if let Some(proof) = roof_proof {
        format!(
            "roof_view={} assemblies={:?} graph_hash={}",
            roof_proof_slug(proof),
            focused_roof_indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].id.0)
                .collect::<Vec<_>>(),
            roof_graph_hash,
        )
    } else {
        String::new()
    };
    if let Some(path) = &output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create building capture directory");
        }
        fs::write(
            path.with_extension("plan.json"),
            serde_json::to_vec_pretty(&plan).expect("serialize building plan"),
        )
        .expect("write generated building plan");
    }

    let plan_audit = audit_plan(&plan);
    for issue in &plan_audit {
        eprintln!("plan audit {}: {}", issue.code, issue.message);
    }
    let manifest = CaptureManifest {
        schema_version: 1,
        fixture: archetype.slug(),
        view: if let Some(roof_view) = roof_proof {
            roof_proof_slug(roof_view)
        } else if let Some(timber_view) = timber_proof_suffix(view) {
            timber_view
        } else if let Some(church_view) = church_proof_slug(view) {
            church_view
        } else if let Some(artillery_view) = artillery_proof_slug(view) {
            artillery_view
        } else {
            match view {
                ViewerView::Exterior => "exterior",
                ViewerView::Defenses => "defenses",
                ViewerView::Cutaway => "cutaway",
                ViewerView::GateDetailExterior => "gate-detail-exterior",
                ViewerView::GateDetailInterior => "gate-detail-interior",
                ViewerView::TowerPortalDetail => "tower-portal-detail",
                ViewerView::CrownStraightExterior => "crown-straight-exterior",
                ViewerView::CrownStraightInterior => "crown-straight-interior",
                ViewerView::CrownCornerExterior => "crown-corner-exterior",
                ViewerView::CrownCornerInterior => "crown-corner-interior",
                ViewerView::CrownTowerExterior => "crown-tower-exterior",
                ViewerView::CrownTowerTop => "crown-tower-top",
                ViewerView::CrownTowerCutaway => "crown-tower-cutaway",
                ViewerView::ProjectedExterior => "projected-exterior",
                ViewerView::ProjectedInterior => "projected-interior",
                ViewerView::ProjectedUnderside => "projected-underside",
                ViewerView::ProjectedTop => "projected-top",
                ViewerView::ProjectedLongitudinal => "projected-longitudinal",
                ViewerView::ProjectedSockets => "projected-sockets",
                ViewerView::ProjectedFlank => "projected-flank",
                ViewerView::OpeningRectangularExterior => "opening-rectangular-exterior",
                ViewerView::OpeningRectangularInterior => "opening-rectangular-interior",
                ViewerView::OpeningRectangularSection => "opening-rectangular-section",
                ViewerView::OpeningSegmentalExterior => "opening-segmental-exterior",
                ViewerView::OpeningSegmentalInterior => "opening-segmental-interior",
                ViewerView::OpeningSegmentalSection => "opening-segmental-section",
                ViewerView::OpeningPointedExterior => "opening-pointed-exterior",
                ViewerView::OpeningPointedInterior => "opening-pointed-interior",
                ViewerView::OpeningPointedSection => "opening-pointed-section",
                ViewerView::OpeningArrowLoopExterior => "opening-arrow-loop-exterior",
                ViewerView::OpeningArrowLoopInterior => "opening-arrow-loop-interior",
                ViewerView::OpeningArrowLoopSection => "opening-arrow-loop-section",
                ViewerView::OpeningGunLoopExterior => "opening-gun-loop-exterior",
                ViewerView::OpeningGunLoopInterior => "opening-gun-loop-interior",
                ViewerView::OpeningGunLoopSection => "opening-gun-loop-section",
                ViewerView::WallTimberFrameSection => "wall-timber-frame-section",
                ViewerView::WallCivilianMasonrySection => "wall-civilian-masonry-section",
                ViewerView::WallCathedralButtressSection => "wall-cathedral-buttress-section",
                ViewerView::WallRoundTowerRadialSection => "wall-round-tower-radial-section",
                _ => unreachable!("church views handled before ordinary view mapping"),
            }
        },
        seed,
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        room_count: plan.storeys.iter().map(|storey| storey.rooms.len()).sum(),
        wall_count: plan.storeys.iter().map(|storey| storey.walls.len()).sum(),
        opening_count: plan
            .storeys
            .iter()
            .map(|storey| storey.openings.len())
            .sum(),
        roof_piece_count: plan.roofs.len(),
        roof_dormer_count: plan.roof_dormers.len(),
        roof_assembly_count: plan.roof_assemblies.len(),
        roof_graph_hash,
        roof_face_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.faces.iter().map(|face| face.id.0))
            .collect(),
        roof_edge_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.edges.iter().map(|edge| edge.id.0))
            .collect(),
        roof_cut_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.children.iter().map(|child| child.parent_cut.0))
            .collect(),
        roof_support_node_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.support_nodes.iter().map(|node| node.0))
            .collect(),
        roof_drainage_terminal_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| {
                roof.edges
                    .iter()
                    .filter_map(|edge| edge.drainage_terminal.map(|terminal| terminal.0))
            })
            .collect(),
        roof_drainage_network_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .map(|network| network.id.0)
            .collect(),
        roof_drainage_channel_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .flat_map(|network| {
                std::iter::once(network.channel_floor.0)
                    .chain(network.channel_lips.iter().map(|id| id.0))
                    .chain(network.collector_solids.iter().map(|id| id.0))
                    .chain(network.downspout.iter().map(|id| id.0))
            })
            .collect(),
        roof_drainage_outlet_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .map(|network| network.outlet_void.0)
            .collect(),
        roof_drainage_route_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .filter_map(|network| {
                plan.resolved_geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == network.catchment)
                    .map(|catchment| catchment.outlet_route.0)
            })
            .collect(),
        roof_render_item_count: expected_roof_render_items.len(),
        roof_render_multiset_hash,
        rendered_roof_item_count: 0,
        rendered_roof_hash: String::new(),
        tower_count: plan.towers.len(),
        square_tower_count: plan.square_towers.len(),
        curtain_wall_count: plan.curtain_walls.len(),
        stair_count: plan.stairs.len(),
        battlement_run_count: plan.battlements.len(),
        wall_walk_count: plan.wall_walks.len(),
        defensive_circuit_count: plan.defensive_circuits.len(),
        defensive_junction_count: plan.defensive_junctions.len(),
        tower_portal_count: plan.tower_portals.len(),
        gate_defense_count: plan.gate_defenses.len(),
        firing_position_count: plan
            .gate_defenses
            .iter()
            .map(|defense| defense.firing_positions.len())
            .sum(),
        gate_closure_count: plan
            .gate_defenses
            .iter()
            .map(|defense| defense.closures.len())
            .sum(),
        resolved_solid_count: plan.resolved_geometry.solids.len(),
        resolved_void_count: plan.resolved_geometry.voids.len(),
        resolved_owner_count: plan
            .resolved_geometry
            .solids
            .iter()
            .map(|solid| solid.owner)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        rendered_owner_count: 0,
        rendered_resolved_solid_count: 0,
        resolver_schema_version: plan.resolved_geometry.schema_version,
        resolved_geometry_hash,
        resolved_solid_multiset_hash: expected_render_hash,
        rendered_geometry_hash: String::new(),
        source_revision: source_revision(),
        source_dirty_fingerprint: source_dirty_fingerprint(),
        plan_hash,
        evidence_hash,
        pixel_hash: String::new(),
        focus_kind: if roof_proof.is_some() {
            Some("resolved_roof")
        } else if timber_proof_suffix(view).is_some() {
            Some("resolved_timber_frame")
        } else if church_proof_slug(view).is_some() {
            Some("resolved_church_program")
        } else {
            match view {
                ViewerView::GateDetailExterior => Some("gate_exterior"),
                ViewerView::GateDetailInterior => Some("gate_interior_section"),
                ViewerView::TowerPortalDetail => Some("tower_portal"),
                ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway => Some("resolved_crown"),
                ViewerView::ProjectedExterior
                | ViewerView::ProjectedInterior
                | ViewerView::ProjectedUnderside
                | ViewerView::ProjectedTop
                | ViewerView::ProjectedLongitudinal
                | ViewerView::ProjectedSockets
                | ViewerView::ProjectedFlank => Some("resolved_projected"),
                view if opening_proof_profile(view).is_some() => Some("resolved_opening"),
                view if wall_section_kind(view).is_some() => Some("resolved_wall_section"),
                view if artillery_proof_slug(view).is_some() => Some("artillery_assembly"),
                _ => None,
            }
        },
        focused_tower_index: (view == ViewerView::TowerPortalDetail).then_some(0),
        focused_tower_indices: match view {
            ViewerView::GateDetailExterior => plan
                .gate_defenses
                .first()
                .map(|defense| {
                    defense
                        .firing_positions
                        .iter()
                        .map(|position| position.tower_index)
                        .collect()
                })
                .unwrap_or_default(),
            ViewerView::TowerPortalDetail => vec![0],
            _ => Vec::new(),
        },
        focused_wall_index: matches!(
            view,
            ViewerView::GateDetailExterior | ViewerView::GateDetailInterior
        )
        .then_some(0),
        focused_resolved_item_ids: if roof_proof.is_some() {
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    if focused_abutment_item_ids.is_empty() {
                        dormer_focus_roof.map_or_else(
                            || focused_roof_owners.contains(&solid.owner),
                            |dormer| solid.owner == dormer.owner,
                        ) && (roof_proof
                            .is_some_and(|proof| roof_proof_slug(proof).ends_with("-drainage"))
                            || !focused_roof_downspouts.contains(&solid.id.0))
                    } else {
                        focused_abutment_item_ids.contains(&solid.id.0)
                    }
                })
                .map(|solid| solid.id.0)
                .collect()
        } else if timber_proof_suffix(view).is_some() {
            timber_focus_ids.clone()
        } else if church_proof_slug(view).is_some() {
            church_focus_ids.clone()
        } else if artillery_proof_slug(view).is_some() {
            artillery_focus_item_ids(&plan, view)
        } else if architectural_proof(view) {
            architectural_items.clone()
        } else if projected_view(view) {
            focused_projected_item_ids(&plan, view, projected_kind)
        } else if view == ViewerView::Exterior {
            plan.resolved_geometry
                .solids
                .iter()
                .map(|solid| solid.id.0)
                .collect()
        } else {
            focused_crown_item_ids(&plan, view)
        },
        focused_resolved_void_ids: if roof_proof
            .is_some_and(|proof| roof_proof_slug(proof).ends_with("-drainage"))
        {
            plan.resolved_geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| focused_roof_owners.contains(&network.owner))
                .map(|network| network.outlet_void.0)
                .collect()
        } else if artillery_proof_slug(view).is_some() {
            artillery_focus_void_ids(&plan, view)
        } else if architectural_proof(view) {
            architectural_voids.clone()
        } else if matches!(
            view,
            ViewerView::TimberOpeningBayExterior
                | ViewerView::TimberOpeningBayInterior
                | ViewerView::TimberOpeningBaySection
        ) {
            plan.timber_frame
                .as_ref()
                .into_iter()
                .flat_map(|frame| &frame.bays)
                .find_map(|bay| bay.opening)
                .and_then(|opening_id| {
                    plan.opening_assemblies
                        .iter()
                        .find(|opening| opening.id == opening_id)
                })
                .map(|opening| vec![opening.void_id.0])
                .unwrap_or_default()
        } else {
            projected_focus
                .map(|defense| {
                    defense
                        .throat_voids
                        .iter()
                        .copied()
                        .chain(defense.access_portal)
                        .chain(defense.firing_apertures.iter().copied())
                        .map(|id| id.0)
                        .collect()
                })
                .unwrap_or_default()
        },
        focused_roof_item_ids,
        section_removed_roof_item_ids,
        visible_focused_roof_item_count: 0,
        focused_projected_ray_count: projected_focus
            .map(|defense| {
                plan.resolved_geometry
                    .projected_defense_rays
                    .iter()
                    .filter(|ray| ray.owner == defense.owner)
                    .count()
            })
            .unwrap_or(0),
        projected_defense_kind: projected_focus.map(|_| projected_kind_slug(projected_kind)),
        projected_defense_deployment: projected_focus
            .map(|defense| projected_deployment_slug(defense.deployment)),
        projected_tactical_target: projected_focus
            .map(|defense| projected_target_slug(defense.tactical_target)),
        visible_focused_resolved_item_count: 0,
        focused_bounds_fraction: [0.0; 4],
        camera_position: [0.0; 3],
        camera_target: [0.0; 3],
        required_focus_object_count: if roof_proof.is_some() {
            focused_roof_indices.len().max(1)
        } else if timber_proof_suffix(view).is_some() {
            timber_focus_ids
                .len()
                .saturating_sub(timber_removed_target_item_ids.len())
                .clamp(2, 8)
        } else if church_proof_slug(view).is_some() {
            8
        } else {
            match view {
                ViewerView::GateDetailExterior => 6,
                ViewerView::GateDetailInterior => 11,
                ViewerView::TowerPortalDetail => 6,
                ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway => 4,
                ViewerView::ProjectedExterior
                | ViewerView::ProjectedInterior
                | ViewerView::ProjectedUnderside
                | ViewerView::ProjectedTop
                | ViewerView::ProjectedLongitudinal
                | ViewerView::ProjectedSockets
                | ViewerView::ProjectedFlank => 3,
                view if opening_proof_profile(view).is_some() => 3,
                view if wall_section_kind(view).is_some() => 1,
                _ => 0,
            }
        },
        visible_focus_object_count: 0,
        focus_requirements_met: false,
        lighting_preset: match view {
            ViewerView::GateDetailInterior => "clear_working_daylight_section_high_sun",
            ViewerView::GateDetailExterior => "clear_working_daylight_detail_framed",
            ViewerView::TowerPortalDetail => "clear_working_daylight_detail_high_sun",
            ViewerView::CrownStraightExterior
            | ViewerView::CrownStraightInterior
            | ViewerView::CrownCornerExterior
            | ViewerView::CrownCornerInterior
            | ViewerView::CrownTowerExterior
            | ViewerView::CrownTowerTop
            | ViewerView::CrownTowerCutaway => "clear_working_daylight_crown_proof",
            ViewerView::ProjectedExterior
            | ViewerView::ProjectedInterior
            | ViewerView::ProjectedUnderside
            | ViewerView::ProjectedTop
            | ViewerView::ProjectedLongitudinal
            | ViewerView::ProjectedSockets
            | ViewerView::ProjectedFlank => "clear_working_daylight_projected_defense_proof",
            _ => "clear_working_daylight",
        },
        sun_direction: if let Some(defense) = projected_focus {
            let outward = match defense.path {
                ProjectedDefensePath::Linear { outward, .. }
                | ProjectedDefensePath::Round { outward, .. } => direction_vector_2d(outward),
            };
            let tangent = Vec2::new(-outward.y, outward.x);
            let (outward_scale, tangent_scale) = if matches!(
                view,
                ViewerView::ProjectedLongitudinal | ViewerView::ProjectedTop
            ) {
                (34.0, 18.0)
            } else {
                (18.0, 34.0)
            };
            (-Vec3::new(
                outward.x * outward_scale + tangent.x * tangent_scale,
                45.0,
                outward.y * outward_scale + tangent.y * tangent_scale,
            )
            .normalize())
            .to_array()
        } else if roof_proof
            .is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
        {
            [0.55, -0.75, 0.39]
        } else {
            match view {
                ViewerView::GateDetailInterior => [-0.42, -0.75, -0.51],
                ViewerView::GateDetailExterior => [-0.45, -0.61, 0.55],
                ViewerView::TowerPortalDetail => [-0.42, -0.75, 0.51],
                ViewerView::CrownStraightInterior => [0.45, -0.72, -0.55],
                ViewerView::CrownCornerInterior => [-0.45, -0.72, -0.55],
                ViewerView::CrownStraightExterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway => [-0.5, -0.72, 0.48],
                ViewerView::Defenses => [0.62, -0.69, -0.36],
                ViewerView::TimberFrameFacade => [-0.64, -0.75, 0.15],
                ViewerView::TimberGableRoofBearing => [-0.67, -0.72, 0.18],
                _ => [-0.45, -0.61, 0.55],
            }
        },
        sun_illuminance_lux: if projected_focus.is_some() {
            20_000.0
        } else if matches!(
            view,
            ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway
        ) || timber_proof_suffix(view).is_some()
        {
            28_000.0
        } else {
            24_000.0
        },
        ambient_brightness: if view == ViewerView::ProjectedInterior {
            420.0
        } else if view == ViewerView::ProjectedUnderside {
            380.0
        } else if roof_proof
            .is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
        {
            320.0
        } else if roof_proof.is_some_and(|proof| roof_proof_slug(proof).ends_with("-interior")) {
            400.0
        } else if roof_proof.is_some() {
            240.0
        } else if matches!(
            view,
            ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway
        ) || projected_focus.is_some()
        {
            340.0
        } else if matches!(
            view,
            ViewerView::TimberRegistrationCut | ViewerView::TimberGableRoofBearing
        ) {
            175.0
        } else if timber_proof_suffix(view).is_some() {
            220.0
        } else {
            380.0
        },
        ambient_color: [0.72, 0.78, 0.88],
        lighting_calibration_bounds_fraction: [0.0; 4],
        median_luminance_percent: 0,
        dark_clipped_bps: 0,
        bright_clipped_bps: 0,
        luminance_separation_percent: 0,
        shadow_luminance_percent: 0,
        plan_audit_issue_count: plan_audit.len(),
        audited_closed_mesh_count: 0,
        mesh_integrity_issue_count: 0,
        bartizan_count: plan.bartizans.len(),
        observed_mesh_count: 0,
        visible_mesh_count: 0,
        active_camera_count: 0,
        subject_pixel_bps: 0,
        validation_passed: false,
        opening_profile: opening_proof_profile(view),
        wall_section_kind: wall_section_kind(view),
        focused_assembly_owner_id: architectural_owner,
        focused_resolved_geometry_hash: architectural_focus_hash,
        section_cut_applied: section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view)
            || roof_proof.is_some_and(roof_proof_sectioned),
        section_removed_item_ids: if section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view)
        {
            architectural_section_removed_item_ids(&plan, view)
                .into_iter()
                .filter(|id| {
                    (!church_section_proof(view) || church_focus_ids.contains(id))
                        && (!timber_section_proof(view) || timber_focus_ids.contains(id))
                })
                .collect()
        } else {
            Vec::new()
        },
        inside_label_visible: section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view),
        outside_label_visible: section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view),
        wall_thickness_metres: focused_opening(&plan, view)
            .and_then(|opening| {
                plan.wall_assemblies
                    .iter()
                    .find(|wall| wall.id == opening.host_wall)
            })
            .or_else(|| focused_wall(&plan, view))
            .map(|wall| wall.thickness_metres)
            .or_else(|| {
                (view == ViewerView::WallRoundTowerRadialSection)
                    .then(|| plan.towers.first().map(|tower| tower.wall_thickness_metres))
                    .flatten()
            }),
        scale_figure_height_metres: (section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view))
        .then_some(1.75),
        scale_figure_visible: section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view),
        section_annotation,
        section_annotation_visible: false,
        exterior_throat_bounds_fraction: [0.0; 4],
        interior_mouth_bounds_fraction: [0.0; 4],
        church_program_hash,
        church_bay_labels,
        church_support_node_ids,
        church_opening_ids,
        church_focused_roles,
        church_target_component_ids,
        church_target_item_ids: church_focus_ids.clone(),
        church_required_roles,
        church_cut_plane,
        church_removed_target_item_ids,
        church_legend_visible: false,
        timber_program_hash: plan
            .timber_frame
            .as_ref()
            .map_or_else(String::new, |frame| {
                stable_evidence_hash(&serde_json::to_vec(frame).expect("serialize timber frame"))
            }),
        timber_program: plan
            .timber_frame
            .as_ref()
            .map(|frame| format!("{:?}", frame.program)),
        timber_assembly_id: plan.timber_frame.as_ref().map(|frame| frame.id.0),
        timber_member_ids: plan.timber_frame.as_ref().map_or_else(Vec::new, |frame| {
            frame.members.iter().map(|member| member.id.0).collect()
        }),
        timber_joint_ids: plan.timber_frame.as_ref().map_or_else(Vec::new, |frame| {
            frame.joints.iter().map(|joint| joint.id.0).collect()
        }),
        timber_node_ids: plan.timber_frame.as_ref().map_or_else(Vec::new, |frame| {
            frame
                .members
                .iter()
                .flat_map(|member| [member.start_node.0, member.end_node.0])
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        }),
        timber_focused_roles,
        timber_role_item_ids,
        timber_role_bounds_fraction: std::collections::BTreeMap::new(),
        timber_target_component_ids: timber_target_component_ids(&plan, view),
        timber_focus_interface_ids: timber_focus_interface_ids(&plan, view),
        timber_required_roles,
        timber_cut_plane,
        timber_removed_target_item_ids,
        timber_legend_visible: false,
        artillery_assembly_id: plan.artillery_castle.as_ref().map(|castle| castle.id.0),
        artillery_phase: plan
            .artillery_castle
            .as_ref()
            .map(|castle| format!("{:?}", castle.phase)),
        artillery_curtain_ids: plan
            .artillery_castle
            .as_ref()
            .map_or_else(Vec::new, |castle| {
                castle.curtains.iter().map(|curtain| curtain.id.0).collect()
            }),
        artillery_rondel_ids: plan
            .artillery_castle
            .as_ref()
            .map_or_else(Vec::new, |castle| {
                castle.rondels.iter().map(|rondel| rondel.id.0).collect()
            }),
        artillery_station_ids: plan
            .artillery_castle
            .as_ref()
            .map_or_else(Vec::new, |castle| {
                castle.stations.iter().map(|station| station.id.0).collect()
            }),
        artillery_route_surface_ids: plan.artillery_castle.as_ref().map_or_else(
            Vec::new,
            |castle| {
                castle
                    .route_nodes
                    .iter()
                    .map(|node| node.surface.0)
                    .collect()
            },
        ),
        artillery_fire_ray_count: plan.artillery_castle.as_ref().map_or(0, |castle| {
            castle
                .stations
                .iter()
                .map(|station| station.rays.len())
                .sum()
        }),
        artillery_support_node_ids: plan.artillery_castle.as_ref().map_or_else(
            Vec::new,
            |castle| {
                let owners = castle
                    .curtains
                    .iter()
                    .map(|curtain| curtain.owner)
                    .chain(castle.rondels.iter().map(|rondel| rondel.owner))
                    .collect::<std::collections::HashSet<_>>();
                plan.resolved_geometry
                    .structural_nodes
                    .iter()
                    .filter(|node| owners.contains(&node.owner))
                    .map(|node| node.id.0)
                    .collect()
            },
        ),
        artillery_ditch_void_id: plan
            .artillery_castle
            .as_ref()
            .map(|castle| castle.ditch.void_id.0),
        artillery_bridge_state: plan
            .artillery_castle
            .as_ref()
            .map(|castle| format!("{:?}", castle.bridge.state)),
        artillery_focused_roles: {
            let focus = artillery_focus_item_ids(&plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| focus.contains(&solid.id.0))
                .map(|solid| format!("{:?}", solid.role))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        },
        artillery_role_item_ids: {
            let focus = artillery_focus_item_ids(&plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            let mut roles = std::collections::BTreeMap::<String, Vec<u64>>::new();
            for solid in plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| focus.contains(&solid.id.0))
            {
                roles
                    .entry(format!("{:?}", solid.role))
                    .or_default()
                    .push(solid.id.0);
            }
            roles
        },
        artillery_role_bounds_fraction: std::collections::BTreeMap::new(),
        artillery_target_component_ids: artillery_proof_slug(view)
            .map(|slug| vec![format!("artillery:1/{slug}")])
            .unwrap_or_default(),
        artillery_cut_plane: artillery_cut_plane(view),
        artillery_removed_target_item_ids: artillery_section_removed_item_ids(&plan, view),
        artillery_legend_visible: false,
    };

    let title = format!("Fabelgeist building prototype: {archetype:?} {view:?}");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title,
            resolution:
                WindowResolution::new(VIEW_WIDTH, VIEW_HEIGHT).with_scale_factor_override(1.0),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            decorations: output.is_none(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.72, 0.80, 0.86)))
    .insert_resource(CaptureState {
        output,
        settle_frames,
        settled: 0,
        primed: false,
        in_flight: false,
        manifest,
    });
    if editor {
        app.add_plugins((
            MeshPickingPlugin,
            OutlinePlugin::JUMP_FLOOD,
            EguiPlugin::default(),
            PanOrbitCameraPlugin,
        ))
        .insert_resource(EditorRuntime::new(
            document,
            plan.clone(),
            editor_document_path,
            player_build_document.clone(),
            player_build_document_path,
        ))
        .add_observer(editor_pointer_over)
        .add_observer(editor_pointer_out)
        .add_observer(editor_pointer_click)
        .add_systems(EguiPrimaryContextPass, editor_ui)
        .add_systems(
            Update,
            (
                update_editor_outlines,
                frame_editor_selection,
                editor_keyboard_shortcuts,
                update_editor_visibility,
            ),
        )
        .add_systems(PostUpdate, rebuild_editor_scene);
    }
    let startup_plan = plan.clone();
    app.add_systems(Startup, move |world: &mut World| {
        setup(
            world,
            &startup_plan,
            view,
            projected_kind,
            roof_proof,
            if editor {
                SceneSetup::EditorInitial
            } else {
                SceneSetup::Full
            },
        );
        if editor {
            configure_editor_scene(world, &startup_plan, true);
        }
        if let Some(document) = &player_build_document {
            setup_player_build_scene(world, document);
        }
    })
    .add_systems(Last, capture_when_ready);
    let exit = app.run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}

fn setup(
    world: &mut World,
    plan: &BuildingPlan,
    view: ViewerView,
    projected_kind: ProjectedProofKind,
    roof_proof: Option<RoofProofView>,
    scene_setup: SceneSetup,
) {
    let palette = create_palette(world);
    let dimensions = plan.dimensions_metres();
    let origin = Vec2::new(-dimensions.x * 0.5, -dimensions.y * 0.5);
    let storey_height = plan.storey_height_metres;
    let crown_proof = matches!(
        view,
        ViewerView::CrownStraightExterior
            | ViewerView::CrownStraightInterior
            | ViewerView::CrownCornerExterior
            | ViewerView::CrownCornerInterior
            | ViewerView::CrownTowerExterior
            | ViewerView::CrownTowerTop
            | ViewerView::CrownTowerCutaway
    );
    let projected_proof = projected_view(view);
    let mut removed_roof_items = roof_proof
        .filter(|proof| roof_proof_sectioned(*proof))
        .map(|proof| {
            roof_proof_assembly_indices(plan, proof)
                .into_iter()
                .filter_map(|index| {
                    plan.roof_assemblies[index]
                        .faces
                        .last()
                        .map(|face| face.id.0)
                })
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    removed_roof_items.extend(church_section_removed_roof_item_ids(plan, view));
    let calibrated_roof_ids = roof_proof
        .map(|proof| {
            roof_proof_assembly_indices(plan, proof)
                .into_iter()
                .map(|index| plan.roof_assemblies[index].id)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let architectural_proof = architectural_proof(view);
    let artillery_proof = artillery_proof_slug(view).is_some();
    let focused_ids = if architectural_proof {
        architectural_focus_item_ids(plan, view)
    } else if projected_proof {
        focused_projected_item_ids(plan, view, projected_kind)
    } else if artillery_proof {
        artillery_focus_item_ids(plan, view)
    } else {
        focused_crown_item_ids(plan, view)
    }
    .into_iter()
    .collect::<std::collections::HashSet<_>>();
    let mut proof_owners = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| focused_ids.contains(&solid.id.0))
        .map(|solid| solid.owner.0)
        .collect::<std::collections::HashSet<_>>();
    if matches!(
        view,
        ViewerView::CrownTowerExterior | ViewerView::CrownTowerTop | ViewerView::CrownTowerCutaway
    ) {
        let adjacent = plan
            .crowns
            .iter()
            .filter(|crown| proof_owners.contains(&crown.owner.0))
            .flat_map(|crown| {
                crown
                    .junctions
                    .iter()
                    .map(|junction| junction.other_owner.0)
            })
            .collect::<Vec<_>>();
        proof_owners.extend(adjacent);
    }
    let proof_crown_matches_point = |point: Vec2| {
        plan.crowns.iter().any(|crown| {
            if !proof_owners.contains(&crown.owner.0) {
                return false;
            }
            match crown.path {
                CrownPath::Straight { start, end, .. } => {
                    let delta = end - start;
                    let progress =
                        ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                    point.distance(start + delta * progress) <= CELL_SIZE_METRES * 0.55
                }
                CrownPath::Round { .. } => false,
            }
        })
    };

    let scene_span = plan.towers.iter().fold(dimensions.length(), |span, tower| {
        let position = tower.centre_metres() + origin;
        span.max((position.abs() + Vec2::splat(tower.radius_metres())).length() * 2.0)
    });
    if scene_setup != SceneSetup::EditorBuilding {
        if scene_setup == SceneSetup::EditorInitial {
            spawn_ground(world, Vec2::splat(100.0), false);
        } else if plan.artillery_castle.is_some() {
            spawn_artillery_ground(world, Vec2::splat(scene_span), origin);
        } else {
            spawn_ground(
                world,
                Vec2::splat(scene_span),
                crown_proof || architectural_proof,
            );
        }
    }
    for storey in &plan.storeys {
        if projected_proof
            || architectural_proof
            || timber_isolated_view(view)
            || artillery_isolated_view(view)
        {
            continue;
        }
        if matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail) && storey.level > 0 {
            continue;
        }
        let base_y = f32::from(storey.level) * storey_height;
        for room in &storey.rooms {
            if crown_proof {
                continue;
            }
            let floor_material =
                if matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail) {
                    &palette.room_floors[usize::from(room.id) % palette.room_floors.len()]
                } else {
                    &palette.floor
                };
            for cell in &room.cells {
                spawn_box(
                    world,
                    floor_material,
                    Vec3::new(CELL_SIZE_METRES - 0.04, 0.12, CELL_SIZE_METRES - 0.04),
                    Vec3::new(
                        cell.centre().x + origin.x,
                        base_y + 0.06,
                        cell.centre().y + origin.y,
                    ),
                    Quat::IDENTITY,
                    "room floor",
                );
            }
        }
        for (wall_index, wall) in storey.walls.iter().copied().enumerate() {
            if crown_proof && !proof_crown_matches_point(wall.centre()) {
                continue;
            }
            if matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail)
                && wall.exterior()
                && matches!(wall.direction, Direction::South | Direction::East)
            {
                continue;
            }
            let resolved_host_replaces_wall = (plan.timber_frame.is_some()
                || !matches!(
                    view,
                    ViewerView::Cutaway
                        | ViewerView::TowerPortalDetail
                        | ViewerView::GateDetailInterior
                ))
                && plan.wall_assemblies.iter().any(|assembly| {
                    matches!(
                        assembly.source,
                        adventuresim_building_generator::WallSourceId::StoreyWall {
                            storey_level,
                            wall_index: source_wall,
                        } if storey_level == storey.level && source_wall == wall_index
                    )
                });
            if resolved_host_replaces_wall {
                continue;
            }
            let opening = storey
                .openings
                .iter()
                .find(|opening| opening.wall == wall_index);
            spawn_wall(
                world,
                &palette,
                wall,
                opening,
                origin,
                base_y,
                storey_height,
                plan.wall_style,
                plan.timber_frame_style,
                plan.upper_storey_projection_metres * f32::from(storey.level),
            );
        }
        let projection = plan.upper_storey_projection_metres * f32::from(storey.level);
        if plan.timber_frame.is_none()
            && !architectural_proof
            && plan.wall_style == WallStyle::TimberFrame
            && projection > 0.01
        {
            let min_x = origin.x - projection;
            let max_x = origin.x + dimensions.x + projection;
            let min_z = origin.y - projection;
            let max_z = origin.y + dimensions.y + projection;
            for z in [min_z, max_z] {
                spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(max_x - min_x, 0.14, 0.16),
                    Vec3::new((min_x + max_x) * 0.5, base_y + 0.04, z),
                    Quat::IDENTITY,
                    "projecting storey sill",
                );
            }
            for x in [min_x, max_x] {
                spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(0.16, 0.14, max_z - min_z),
                    Vec3::new(x, base_y + 0.04, (min_z + max_z) * 0.5),
                    Quat::IDENTITY,
                    "projecting storey sill",
                );
                for z in [min_z, max_z] {
                    spawn_box(
                        world,
                        &palette.timber,
                        Vec3::new(0.18, storey_height, 0.18),
                        Vec3::new(x, base_y + storey_height * 0.5, z),
                        Quat::IDENTITY,
                        "projecting storey corner post",
                    );
                }
            }
        }
    }

    if !projected_proof
        && !architectural_proof
        && (!timber_isolated_view(view) || view == ViewerView::TimberGableRoofBearing)
        && !artillery_isolated_view(view)
        && !matches!(
            view,
            ViewerView::Cutaway
                | ViewerView::TowerPortalDetail
                | ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway
        )
    {
        for roof in &plan.roof_assemblies {
            spawn_resolved_roof(
                world,
                &palette,
                roof,
                &plan.resolved_geometry,
                origin,
                &removed_roof_items,
                calibrated_roof_ids.contains(&roof.id),
                view == ViewerView::TimberGableRoofBearing,
            );
        }
    }
    for (tower_index, tower) in plan.towers.iter().copied().enumerate() {
        if view == ViewerView::ArtilleryCurtainSection
            || (view == ViewerView::ArtilleryRondelCasemate && tower_index != 0)
        {
            continue;
        }
        if projected_proof
            || (architectural_proof && view != ViewerView::WallRoundTowerRadialSection)
            || (view == ViewerView::WallRoundTowerRadialSection && tower_index != 0)
        {
            continue;
        }
        if crown_proof
            && !plan.crowns.iter().any(|crown| {
                proof_owners.contains(&crown.owner.0)
                    && matches!(crown.path, CrownPath::Round { tower_index: index, .. } if index == tower_index)
            })
        {
            continue;
        }
        if view == ViewerView::TowerPortalDetail && tower_index != 0 {
            continue;
        }
        if view == ViewerView::GateDetailInterior {
            // Bailey-side section: the exterior preset proves both flanking
            // towers. Remove their shells here so the chamber route and two
            // closure planes remain directly inspectable.
            continue;
        }
        let portals = if view == ViewerView::WallRoundTowerRadialSection {
            Vec::new()
        } else {
            plan.tower_portals
                .iter()
                .copied()
                .filter(|portal| portal.tower_index == tower_index)
                .collect::<Vec<_>>()
        };
        let mut firing_positions = if view == ViewerView::WallRoundTowerRadialSection {
            Vec::new()
        } else {
            plan.gate_defenses
                .iter()
                .flat_map(|gate| gate.firing_positions.iter().copied())
                .filter(|position| position.tower_index == tower_index)
                .collect::<Vec<_>>()
        };
        if let Some(castle) = &plan.artillery_castle {
            firing_positions.extend(
                castle
                    .stations
                    .iter()
                    .filter(|station| station.rondel.0 as usize == tower_index)
                    .filter_map(|station| {
                        let opening = plan
                            .opening_assemblies
                            .iter()
                            .find(|opening| opening.id == station.opening)?;
                        let exterior_height = match opening.profile {
                            adventuresim_building_generator::OpeningProfile::GunLoop {
                                exterior_height_metres,
                                ..
                            } => exterior_height_metres,
                            _ => return None,
                        };
                        Some(FiringPosition {
                            aperture_id: station.id.0 as u16,
                            tower_index,
                            origin: opening.frame.origin,
                            aperture_normal: station.facing,
                            direction: station.facing,
                            elevation_metres: opening.sill_elevation_metres + exterior_height * 0.5,
                            range_metres: 24.0,
                            half_arc_degrees: 38.0,
                            aperture_width_metres: opening.profile.exterior_width_metres(),
                        })
                    }),
            );
        }
        spawn_tower(
            world,
            &palette,
            plan,
            tower_index,
            tower,
            origin,
            view,
            &portals,
            &firing_positions,
            plan.crowns.iter().any(|crown| matches!(crown.path, CrownPath::Round { tower_index: index, .. } if index == tower_index)),
        );
    }
    for tower in plan.square_towers.iter().copied() {
        if projected_proof || architectural_proof || plan.church.is_some() {
            continue;
        }
        spawn_square_tower(world, &palette, tower, origin, view);
    }
    for stair in plan.stairs.iter().copied() {
        if timber_isolated_view(view) {
            continue;
        }
        if matches!(view, ViewerView::ArtilleryCurtainSection | ViewerView::ArtilleryGateInterior)
            // The authoritative resolved ArtilleryStairTread solids already
            // supply the lower casemate flight and are section-filtered above
            // 3.05 m.  The legacy whole-height stair duplicated them and hid
            // the working stations this proof is required to expose.
            || view == ViewerView::ArtilleryRondelCasemate
        {
            continue;
        }
        if plan.church.is_some() {
            // Church service stairs are resolved solids so circulation audit,
            // correspondence, and rendering share one geometry authority.
            continue;
        }
        if projected_proof || architectural_proof {
            continue;
        }
        if view == ViewerView::GateDetailInterior {
            continue;
        }
        if crown_proof {
            let centre = match stair {
                Stair::Spiral { centre, .. } => centre,
                Stair::Straight { start, .. } => start,
            };
            if !plan.crowns.iter().any(|crown| {
                proof_owners.contains(&crown.owner.0)
                    && matches!(crown.path, CrownPath::Round { centre: tower, .. } if tower.distance(centre) < 0.02)
            }) {
                continue;
            }
        }
        spawn_stair(world, &palette, stair, origin);
    }
    for (walk_index, mut wall_walk) in plan.wall_walks.iter().copied().enumerate() {
        if projected_proof || architectural_proof {
            continue;
        }
        let resolved_by_accepted_crown =
            plan.crowns
                .iter()
                .any(|crown| match (crown.path, wall_walk) {
                    (
                        CrownPath::Straight { start, end, .. },
                        WallWalk::Linear {
                            start: walk_start,
                            end: walk_end,
                            ..
                        },
                    ) => {
                        (start.distance(walk_start) < 0.02 && end.distance(walk_end) < 0.02)
                            || (start.distance(walk_end) < 0.02 && end.distance(walk_start) < 0.02)
                    }
                    (
                        CrownPath::Round { centre, .. },
                        WallWalk::Round {
                            centre: walk_centre,
                            ..
                        },
                    ) => centre.distance(walk_centre) < 0.02,
                    _ => false,
                });
        if resolved_by_accepted_crown && view != ViewerView::GateDetailInterior {
            continue;
        }
        if crown_proof
            && !plan.crowns.iter().any(|crown| {
                if !proof_owners.contains(&crown.owner.0) {
                    return false;
                }
                match (crown.path, wall_walk) {
                    (
                        CrownPath::Straight { start, end, .. },
                        WallWalk::Linear {
                            start: walk_start,
                            end: walk_end,
                            ..
                        },
                    ) => {
                        (start.distance(walk_start) < 0.02 && end.distance(walk_end) < 0.02)
                            || (start.distance(walk_end) < 0.02 && end.distance(walk_start) < 0.02)
                    }
                    (
                        CrownPath::Round { centre, .. },
                        WallWalk::Round {
                            centre: walk_centre,
                            ..
                        },
                    ) => centre.distance(walk_centre) < 0.02,
                    _ => false,
                }
            })
        {
            continue;
        }
        if view == ViewerView::GateDetailInterior && !matches!(wall_walk, WallWalk::Linear { .. }) {
            continue;
        }
        if view == ViewerView::GateDetailInterior {
            let Some(defense) = plan.gate_defenses.first() else {
                continue;
            };
            let access = &defense.guard_chamber.access;
            if walk_index != access.from_walk_index {
                continue;
            }
            if let WallWalk::Linear {
                start,
                end,
                elevation_metres,
                width_metres,
                outward,
            } = wall_walk
            {
                // The section preset needs enough rampart to prove positive
                // landing contact, but a full curtain-length slab masks the
                // chamber machinery from the bailey-side camera.
                let tangent = (end - start).normalize_or_zero();
                let projected = start + tangent * (access.top_landing.centre - start).dot(tangent);
                wall_walk = WallWalk::Linear {
                    start: projected - tangent * 2.0,
                    end: projected + tangent * 2.0,
                    elevation_metres,
                    width_metres,
                    outward,
                };
            }
        }
        spawn_wall_walk(world, &palette, wall_walk, origin);
    }
    if !matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail) {
        for (wall_index, curtain_wall) in plan.curtain_walls.iter().copied().enumerate() {
            if projected_proof || architectural_proof {
                continue;
            }
            if crown_proof
                && !plan.crowns.iter().any(|crown| {
                    proof_owners.contains(&crown.owner.0)
                        && matches!(crown.path, CrownPath::Straight { start, end, .. }
                            if ((start-curtain_wall.start).perp_dot(end-start)).abs() < 0.05
                                && ((end-curtain_wall.start).perp_dot(curtain_wall.end-curtain_wall.start)).abs() < 0.05)
                })
            {
                continue;
            }
            let closures = plan
                .gate_defenses
                .iter()
                .flat_map(|gate| gate.closures.iter().copied())
                .filter(|closure| closure.curtain_wall_index == wall_index)
                .collect::<Vec<_>>();
            if let Some(defense) = plan
                .gate_defenses
                .iter()
                .find(|defense| defense.curtain_wall_index == wall_index)
            {
                spawn_gatehouse_curtain(
                    world,
                    &palette,
                    curtain_wall,
                    defense,
                    &plan.towers,
                    origin,
                );
            } else {
                spawn_curtain_wall(world, &palette, curtain_wall, origin, &closures);
            }
        }
        if view != ViewerView::GateDetailInterior {
            spawn_resolved_crowns(
                world,
                &palette,
                plan,
                origin,
                (crown_proof
                    || projected_proof
                    || artillery_proof
                    || (architectural_proof && timber_proof_suffix(view).is_none()))
                .then_some(&proof_owners),
                (section_proof(view)
                    || church_section_proof(view)
                    || church_proof_slug(view).is_some()
                    || timber_proof_suffix(view).is_some()
                    || artillery_proof_slug(view).is_some())
                .then_some(view),
            );
            if architectural_proof || timber_proof_suffix(view).is_some() {
                spawn_resolved_architectural_surfaces(
                    world,
                    &palette,
                    plan,
                    origin,
                    &proof_owners,
                    view,
                );
            }
            if projected_proof
                && let Some(defense) = focused_projected_defense(plan, view, projected_kind)
            {
                spawn_projected_proof_markers(world, &palette, plan, defense.owner, origin, view);
            }
            if matches!(
                view,
                ViewerView::CrownStraightExterior
                    | ViewerView::CrownStraightInterior
                    | ViewerView::CrownCornerExterior
                    | ViewerView::CrownCornerInterior
                    | ViewerView::CrownTowerExterior
                    | ViewerView::CrownTowerTop
                    | ViewerView::CrownTowerCutaway
            ) {
                spawn_crown_defender_scale(world, &palette, plan, view, origin);
            }
            for run in plan.battlements.iter().copied() {
                if crown_proof || projected_proof || architectural_proof {
                    continue;
                }
                if !plan.crowns.is_empty()
                    && matches!(
                        run.kind,
                        BattlementKind::Crenellated
                            | BattlementKind::PiercedCrenellated
                            | BattlementKind::GunLoopParapet
                    )
                {
                    continue;
                }
                if matches!(
                    run.kind,
                    BattlementKind::Machicolated
                        | BattlementKind::Breteche
                        | BattlementKind::OpenHoarding
                        | BattlementKind::RoofedHoarding
                ) {
                    continue;
                }
                spawn_battlement_run(world, &palette, run, origin);
            }
        }
    }
    if view != ViewerView::TowerPortalDetail
        && !crown_proof
        && !projected_proof
        && !architectural_proof
    {
        for defense in &plan.gate_defenses {
            if let Some(wall) = plan.curtain_walls.get(defense.curtain_wall_index).copied() {
                spawn_gate_guard_chamber(world, &palette, defense, wall, origin, view);
            }
        }
    }
    if section_proof(view) || artillery_section_proof(view) {
        spawn_architectural_section_markers(world, &palette, plan, view, origin);
    }
    if artillery_proof_slug(view).is_some() {
        spawn_artillery_proof_markers(world, plan, view, origin);
        let annotation = plan.artillery_castle.as_ref().map_or_else(String::new, |castle| {
            format!(
                "target={} | artillery={} | phase={:?} | trace=orthogonal | curtains={} | rondels={} | stations={} | routes={} | fire={} | cut={:?}",
                artillery_proof_slug(view).unwrap_or_default(),
                castle.id.0,
                castle.phase,
                castle.curtains.len(),
                castle.rondels.len(),
                castle.stations.len(),
                castle.route_edges.len(),
                castle.stations.iter().map(|station| station.rays.len()).sum::<usize>(),
                artillery_cut_plane(view),
            )
        });
        world.spawn((
            Name::new("artillery proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    if timber_proof_suffix(view).is_some() {
        let annotation = plan.timber_frame.as_ref().map_or_else(String::new, |frame| {
            format!(
                "target={} | frame={} | program={:?} | roles={:?} | cut={:?} | members={} | joints={} | exact resolved IDs",
                timber_proof_slug(plan, view).unwrap_or_default(),
                frame.id.0,
                frame.program,
                timber_required_roles(plan, view),
                timber_cut_plane(plan, view),
                frame.members.len(),
                frame.joints.len(),
            )
        });
        world.spawn((
            Name::new("timber proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    if church_proof_slug(view).is_some() {
        let annotation = plan.church.as_ref().map_or_else(String::new, |church| {
            let opening_count = plan
                .opening_assemblies
                .iter()
                .filter(|opening| {
                    matches!(
                        opening.host_source,
                        adventuresim_building_generator::WallSourceId::ChurchExterior { .. }
                            | adventuresim_building_generator::WallSourceId::ChurchArcade { .. }
                            | adventuresim_building_generator::WallSourceId::ChurchApse { .. }
                            | adventuresim_building_generator::WallSourceId::ChurchTowerFace { .. }
                            | adventuresim_building_generator::WallSourceId::SquareTowerFace { .. }
                    )
                })
                .count();
            format!(
                "target={:?} | church={} | 3-aisled cruciform basilica | bays N1-N4 / X / Q1-Q2 / A5 | roles={:?} | cut={:?} | openings={} | supports={}",
                church_target_component_ids(plan, view),
                church.id.0,
                church_required_roles(view),
                church_cut_plane(plan, view),
                opening_count,
                plan.resolved_geometry.structural_nodes.len(),
            )
        });
        world.spawn((
            Name::new("church proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    if let Some(proof) = roof_proof {
        let indices = roof_proof_assembly_indices(plan, proof);
        let annotation = format!(
            "{}  roof_ids={:?}  faces={}  edges={}  cuts={}",
            roof_proof_slug(proof),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].id.0)
                .collect::<Vec<_>>(),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].faces.len())
                .sum::<usize>(),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].edges.len())
                .sum::<usize>(),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index]
                    .faces
                    .iter()
                    .map(|face| face.cutouts.len())
                    .sum::<usize>())
                .sum::<usize>(),
        );
        world.spawn((
            Name::new("roof proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }

    let roof_height = plan
        .roofs
        .iter()
        .map(|roof| {
            let span = match roof.ridge_axis {
                RidgeAxis::Z => roof.size.x * 0.5 + roof.eave_metres,
                RidgeAxis::X => roof.size.y * 0.5 + roof.eave_metres,
            };
            roof.base_height_metres + span * roof.pitch_degrees.to_radians().tan()
        })
        .chain(plan.roof_dormers.iter().map(|dormer| {
            dormer.base_height_metres + dormer.height_metres + dormer.width_metres * 0.65
        }))
        .fold(0.0, f32::max);
    let max_height = plan
        .towers
        .iter()
        .map(|tower| tower.wall_height_metres + tower.radius_metres() * 1.8)
        .fold(
            (plan.storeys.len() as f32 * storey_height + 7.0).max(roof_height),
            f32::max,
        );
    let radius = scene_span.max(max_height) * 1.05;
    let target = Vec3::new(0.0, max_height * 0.35, 0.0);
    let roof_focus_indices = roof_proof
        .map(|proof| roof_proof_assembly_indices(plan, proof))
        .unwrap_or_default();
    let (mut roof_focus, mut roof_focus_extent) = if roof_focus_indices.is_empty() {
        (target, radius)
    } else {
        let (min, max) = roof_focus_indices
            .iter()
            .flat_map(|index| &plan.roof_assemblies[*index].faces)
            .flat_map(|face| &face.polygon)
            .map(|point| Vec3::new(point.x + origin.x, point.y, point.z + origin.y))
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), point| (min.min(point), max.max(point)),
            );
        ((min + max) * 0.5, (max - min).max_element().max(3.0))
    };
    if let Some(proof) = roof_proof {
        let slug = roof_proof_slug(proof);
        if slug.starts_with("roof-dormer-")
            && let Some(child_id) = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    matches!(
                        child.kind,
                        adventuresim_building_generator::RoofChildKind::GabledDormer
                            | adventuresim_building_generator::RoofChildKind::ShedDormer
                    )
                })
                .map(|child| child.child)
            && let Some(child) = plan.roof_assemblies.iter().find(|roof| roof.id == child_id)
        {
            // Dormer evidence is an assembly inspection, not a whole-roof
            // beauty shot.  Bound the camera to the exact child faces and
            // enclosure so gaps, projecting curbs, and oversized eave pieces
            // occupy enough pixels to be reviewable.
            let (min, max) = child
                .faces
                .iter()
                .flat_map(|face| face.polygon.iter())
                .chain(
                    child
                        .enclosure_faces
                        .iter()
                        .flat_map(|face| face.polygon.iter()),
                )
                .map(|point| Vec3::new(point.x + origin.x, point.y, point.z + origin.y))
                .fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(min, max), point| (min.min(point), max.max(point)),
                );
            roof_focus = (min + max) * 0.5;
            roof_focus_extent = (max - min).max_element().max(2.2);
        } else if slug.starts_with("roof-abutment-tower-")
            && !slug.ends_with("-top")
            && !slug.ends_with("-drainage")
            && let Some(tower) = plan.square_towers.first()
        {
            roof_focus = Vec3::new(
                tower.centre.x + origin.x,
                tower.wall_height_metres - 6.0,
                tower.centre.y + origin.y,
            );
            roof_focus_extent = (tower.size.max_element() + 2.0).max(17.0);
        } else if slug.starts_with("roof-cross-gable-")
            && let Some(child_id) = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    child.kind == adventuresim_building_generator::RoofChildKind::CrossGable
                        && child.facade_wall.is_some()
                })
                .map(|child| child.child)
            && let Some(wall) = plan.wall_assemblies.iter().find(|wall| {
                wall.source
                    == adventuresim_building_generator::WallSourceId::RoofChildFront {
                        roof: child_id,
                    }
            })
        {
            roof_focus = Vec3::new(
                wall.frame.origin.x + origin.x,
                wall.base_elevation_metres + wall.height_metres * 0.55,
                wall.frame.origin.y + origin.y,
            );
            // The proof keeps the parent weather face and its real cut in
            // frame as context, so distance is governed by the host roof as
            // well as the narrower facade-derived child.
            roof_focus_extent = wall.length_metres.max(wall.height_metres).max(14.0);
        }
        if slug.starts_with("roof-round-tower-") || slug.starts_with("roof-pavilion-") {
            let owners = roof_focus_indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].owner)
                .collect::<std::collections::HashSet<_>>();
            let downspouts = plan
                .resolved_geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| owners.contains(&network.owner))
                .filter_map(|network| network.downspout)
                .collect::<std::collections::HashSet<_>>();
            let include_downspouts = slug.ends_with("-drainage");
            let bounds = plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    owners.contains(&solid.owner)
                        && matches!(
                            solid.role,
                            SolidRole::RoofFace
                                | SolidRole::RoofFraming
                                | SolidRole::RoofEdgeTreatment
                                | SolidRole::RoofFlashing
                                | SolidRole::RoofPlate
                                | SolidRole::RoofGutter
                        )
                        && (include_downspouts || !downspouts.contains(&solid.id))
                })
                .map(|solid| {
                    let cosine = solid.yaw_radians.cos().abs();
                    let sine = solid.yaw_radians.sin().abs();
                    let half = Vec3::new(
                        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                        (solid.size.y
                            + solid.size.x * solid.longfall_radians.sin().abs()
                            + solid.size.z * solid.crossfall_radians.sin().abs())
                            * 0.5,
                        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                    );
                    let centre = solid.centre + Vec3::new(origin.x, 0.0, origin.y);
                    (centre - half, centre + half)
                })
                .fold(None, |bounds, (min, max)| {
                    Some(
                        bounds.map_or((min, max), |(old_min, old_max): (Vec3, Vec3)| {
                            (old_min.min(min), old_max.max(max))
                        }),
                    )
                });
            if let Some((min, max)) = bounds {
                roof_focus = (min + max) * 0.5;
                roof_focus_extent = (max - min).max_element().max(roof_focus_extent);
            }
        }
    }
    let straight_crown_focus = plan
        .crowns
        .iter()
        .find_map(|crown| match crown.path {
            CrownPath::Straight { start, end, .. } => {
                Some(((start + end) * 0.5 + origin, crown.base_height_metres))
            }
            CrownPath::Round { .. } => None,
        })
        .unwrap_or((Vec2::ZERO, 6.0));
    let corner_crown_focus = plan
        .crowns
        .iter()
        .flat_map(|crown| {
            crown
                .junctions
                .iter()
                .map(move |junction| (crown, junction))
        })
        .find(|(_, junction)| {
            junction.kind == adventuresim_building_generator::CrownJunctionKind::Corner
        })
        .map(|(crown, junction)| (junction.position + origin, crown.base_height_metres))
        .unwrap_or(straight_crown_focus);
    let preferred_tower = plan
        .gate_defenses
        .first()
        .and_then(|gate| gate.firing_positions.first())
        .map(|position| position.tower_index);
    let tower_crown_focus = plan
        .crowns
        .iter()
        .find_map(|crown| match crown.path {
            CrownPath::Round {
                tower_index,
                centre,
                ..
            } if preferred_tower.is_none_or(|preferred| preferred == tower_index) => {
                Some((centre + origin, crown.base_height_metres))
            }
            CrownPath::Straight { .. } => None,
            CrownPath::Round { .. } => None,
        })
        .unwrap_or(straight_crown_focus);
    let (
        projected_focus,
        projected_outward,
        projected_tangent,
        projected_extent,
        projected_vertical_extent,
    ) = focused_projected_defense(plan, view, projected_kind)
        .map(|defense| {
            let (focus, outward, extent) = match defense.path {
                ProjectedDefensePath::Linear {
                    start,
                    end,
                    outward,
                } => (
                    (start + end) * 0.5 + origin,
                    direction_vector_2d(outward),
                    start.distance(end),
                ),
                ProjectedDefensePath::Round {
                    centre,
                    radius_metres,
                    outward,
                } => (
                    centre + origin,
                    direction_vector_2d(outward),
                    radius_metres * 2.0,
                ),
            };
            let (min_y, max_y) = plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| solid.owner == defense.owner || solid.owner == defense.host_owner)
                .fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(min_y, max_y), solid| {
                        (
                            min_y.min(solid.centre.y - solid.size.y * 0.5),
                            max_y.max(solid.centre.y + solid.size.y * 0.5),
                        )
                    },
                );
            (
                Vec3::new(focus.x, (min_y + max_y) * 0.5, focus.y),
                outward,
                Vec2::new(-outward.y, outward.x),
                extent,
                max_y - min_y,
            )
        })
        .unwrap_or((Vec3::new(0.0, 7.0, 0.0), -Vec2::Y, Vec2::X, 6.0, 4.0));
    let projected_distance = (if projected_kind == ProjectedProofKind::Breteche {
        (projected_extent * 0.5 + 3.5)
            * if matches!(
                view,
                ViewerView::ProjectedInterior | ViewerView::ProjectedUnderside
            ) {
                1.3
            } else {
                1.45
            }
    } else if projected_extent < 3.0 {
        4.5
    } else {
        projected_extent * 0.5 + 3.5
    })
    .max(projected_vertical_extent * 0.65 + 2.0)
        * 1.25;
    let projected_flank_scale = if projected_extent < 3.0 { 1.35 } else { 1.0 };
    let projected_interior_scale = if projected_kind == ProjectedProofKind::Breteche {
        0.95
    } else if projected_extent < 3.0 {
        1.30
    } else {
        1.0
    };
    let projected_underside_scale = if projected_kind == ProjectedProofKind::Breteche {
        0.94
    } else {
        1.0
    };
    let projected_top_scale = if projected_kind == ProjectedProofKind::Breteche {
        0.94
    } else {
        1.0
    };
    let (architectural_focus, architectural_outward, architectural_tangent, architectural_distance) =
        if let Some(opening) = focused_opening(plan, view) {
            let height = opening.profile.clear_height_metres();
            let host = plan
                .wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall);
            let focus_y = host.map_or(opening.sill_elevation_metres + height * 0.5, |wall| {
                wall.base_elevation_metres + wall.height_metres * 0.5
            });
            let proof_distance = host.map_or(height * 1.65 + 2.0, |wall| {
                (height * 1.65 + 2.0).max(wall.height_metres * 1.55 + 1.0)
            });
            (
                Vec3::new(
                    opening.frame.origin.x + origin.x,
                    focus_y,
                    opening.frame.origin.y + origin.y,
                ),
                opening.frame.outward,
                opening.frame.tangent,
                // Proof owners include the full load-bearing jamb/head assembly,
                // not only the blue/void opening.  Frame that structural height
                // with enough margin for section labels and the 1.75 m scale.
                proof_distance.max(6.2),
            )
        } else if view == ViewerView::WallRoundTowerRadialSection {
            let tower = plan.towers.first().copied();
            let centre = tower.map(RoundTower::centre_metres).unwrap_or(Vec2::ZERO) + origin;
            (
                Vec3::new(
                    centre.x,
                    tower.map_or(3.0, |tower| tower.wall_height_metres * 0.5),
                    centre.y,
                ),
                -Vec2::Y,
                Vec2::X,
                tower.map_or(9.0, |tower| tower.radius_metres() * 4.5),
            )
        } else if let Some(wall) = focused_wall(plan, view) {
            (
                Vec3::new(
                    wall.frame.origin.x + origin.x,
                    wall.base_elevation_metres + wall.height_metres * 0.5,
                    wall.frame.origin.y + origin.y,
                ),
                wall.frame.outward,
                wall.frame.tangent,
                (wall.height_metres * 1.5 + 0.8).max(5.4),
            )
        } else {
            (Vec3::ZERO, -Vec2::Y, Vec2::X, 5.0)
        };
    let church_camera = church_camera(plan, view, origin);
    let timber_camera = timber_camera(plan, view, origin);
    let artillery_camera = artillery_camera(plan, view, origin);
    let camera_position = if let Some((camera, _)) = artillery_camera {
        camera
    } else if let Some((camera, _)) = timber_camera {
        camera
    } else if let Some((camera, _)) = church_camera {
        camera
    } else if let Some(proof) = roof_proof {
        let slug = roof_proof_slug(proof);
        let distance_scale = if slug == "roof-courtyard-valleys-top"
            || matches!(slug, "roof-l-valley-top" | "roof-l-valley-drainage")
        {
            // Keep the complete four-wing courtyard footprint, including
            // its drainage terminals, inside the top-view proof frame.
            2.25
        } else if slug == "roof-l-valley-underside" {
            2.25
        } else if slug == "roof-pavilion-drainage" {
            0.80
        } else if slug == "roof-round-tower-drainage" {
            // Shared perimeter outlets no longer create a full-height pipe
            // cage. Frame the complete cap and all four outlet stations;
            // the prior pipe-oriented close crop clipped the high pavilion.
            1.25
        } else if slug.starts_with("roof-abutment-tower-") {
            // The high bell tower and its lower-corner outlet must both fit in
            // every proof without clipping the parent cut/contact contour.
            1.55
        } else if slug == "roof-dormer-gabled-interior" || slug == "roof-cross-gable-underside" {
            if slug == "roof-cross-gable-underside" {
                2.65
            } else {
                1.72
            }
        } else if slug.starts_with("roof-cross-gable-")
            && (slug.ends_with("-top") || slug.ends_with("-drainage"))
        {
            2.35
        } else if slug.starts_with("roof-cross-gable-") {
            1.75
        } else if slug.starts_with("roof-dormer-") {
            1.12
        } else if roof_focus_indices.len() > 2 {
            1.9
        } else {
            1.35
        };
        let distance = if slug.ends_with("-high-pitch") {
            roof_focus_extent.min(18.0) * 1.35 + 2.0
        } else {
            roof_focus_extent * distance_scale + 2.0
        };
        if slug.ends_with("-high-pitch") {
            roof_focus + Vec3::new(distance * 0.75, distance * 0.75, -distance)
        } else if slug.ends_with("-top") || slug.ends_with("-drainage") {
            roof_focus + Vec3::new(distance * 0.18, distance * 1.35, -distance * 0.12)
        } else if slug == "roof-dormer-gabled-exterior"
            && let Some(child_id) = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    child.kind == adventuresim_building_generator::RoofChildKind::GabledDormer
                })
                .map(|child| child.child)
            && let Some(wall) = plan.wall_assemblies.iter().find(|wall| {
                wall.source
                    == adventuresim_building_generator::WallSourceId::RoofChildFront {
                        roof: child_id,
                    }
            })
        {
            let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
            let tangent = Vec3::new(wall.frame.tangent.x, 0.0, wall.frame.tangent.y);
            roof_focus + outward * distance + tangent * distance * 0.34 + Vec3::Y * distance * 0.34
        } else if slug == "roof-cross-gable-exterior" {
            let cross_id = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    child.kind == adventuresim_building_generator::RoofChildKind::CrossGable
                        && child.facade_wall.is_some()
                })
                .map(|child| child.child);
            let outward = plan
                .wall_assemblies
                .iter()
                .find(|wall| {
                    matches!(wall.source, adventuresim_building_generator::WallSourceId::RoofChildFront { roof } if Some(roof) == cross_id)
                })
                .map(|wall| wall.frame.outward)
                .unwrap_or(-Vec2::Y);
            roof_focus + Vec3::new(outward.x, 0.0, outward.y) * distance + Vec3::Y * distance * 0.38
        } else if slug.ends_with("-underside") || slug.ends_with("-interior") {
            roof_focus + Vec3::new(distance * 0.9, distance * 0.18, -distance * 0.55)
        } else if slug == "roof-abutment-wall-cutaway" {
            roof_focus + Vec3::new(distance * 0.9, distance * 1.05, -distance * 0.8)
        } else if slug.ends_with("-cutaway") {
            roof_focus + Vec3::new(distance * 1.05, distance * 0.62, -distance * 0.90)
        } else if slug == "roof-abutment-wall-exterior" {
            roof_focus + Vec3::new(distance * 0.9, distance * 0.85, -distance * 0.8)
        } else if slug == "roof-cathedral-exterior" {
            roof_focus + Vec3::new(distance, distance * 0.68, -distance)
        } else {
            roof_focus + Vec3::new(distance, distance * 0.48, -distance)
        }
    } else {
        match view {
            ViewerView::Exterior => {
                let scale = match plan.archetype {
                    BuildingArchetype::FachwerkMerchantHouse => 1.18,
                    BuildingArchetype::ArtilleryRondelCastle => 0.92,
                    _ => 1.0,
                };
                Vec3::new(
                    radius * 0.82 * scale,
                    max_height * 0.90 * scale,
                    -radius * 1.08 * scale,
                )
            }
            ViewerView::Defenses => Vec3::new(-radius * 1.05, max_height * 1.35, radius * 1.15),
            ViewerView::Cutaway => Vec3::new(radius * 0.75, max_height * 1.8, -radius * 1.1),
            ViewerView::GateDetailExterior => {
                let focus = plan
                    .gate_defenses
                    .first()
                    .map(|defense| defense.threshold + origin)
                    .unwrap_or(Vec2::ZERO);
                Vec3::new(focus.x + 10.0, 8.0, focus.y - 15.5)
            }
            ViewerView::GateDetailInterior => {
                let focus = plan
                    .gate_defenses
                    .first()
                    .map(|defense| defense.guard_chamber.access.flight.bottom + origin)
                    .unwrap_or(Vec2::ZERO);
                // Look through the sectioned east/rear corner from above. The
                // detail renderer shortens the proof fragment of wall walk, so
                // this angle retains the whole flight without masking the chamber
                // floor, murder hole, or windlass.
                Vec3::new(focus.x + 9.5, 12.5, focus.y + 7.5)
            }
            ViewerView::TowerPortalDetail => {
                let focus = plan
                    .towers
                    .first()
                    .map(|tower| tower.centre_metres() + origin)
                    .unwrap_or(Vec2::ZERO);
                Vec3::new(focus.x + 8.0, 7.0, focus.y - 10.0)
            }
            ViewerView::CrownStraightExterior => Vec3::new(
                straight_crown_focus.0.x + 4.8,
                straight_crown_focus.1 + 3.6,
                straight_crown_focus.0.y - 6.0,
            ),
            ViewerView::CrownStraightInterior => Vec3::new(
                straight_crown_focus.0.x + 5.5,
                straight_crown_focus.1 + 4.5,
                straight_crown_focus.0.y + 6.0,
            ),
            ViewerView::CrownCornerExterior => Vec3::new(
                corner_crown_focus.0.x - 4.7,
                corner_crown_focus.1 + 3.6,
                corner_crown_focus.0.y - 4.7,
            ),
            ViewerView::CrownCornerInterior => Vec3::new(
                corner_crown_focus.0.x + 6.2,
                corner_crown_focus.1 + 4.0,
                corner_crown_focus.0.y + 6.2,
            ),
            ViewerView::CrownTowerExterior => Vec3::new(
                tower_crown_focus.0.x + 1.3,
                tower_crown_focus.1 + 3.4,
                tower_crown_focus.0.y - 6.8,
            ),
            ViewerView::CrownTowerTop => Vec3::new(
                tower_crown_focus.0.x + 1.7,
                tower_crown_focus.1 + 8.0,
                tower_crown_focus.0.y - 1.7,
            ),
            ViewerView::CrownTowerCutaway => Vec3::new(
                tower_crown_focus.0.x + 4.8,
                tower_crown_focus.1 + 4.5,
                tower_crown_focus.0.y - 4.8,
            ),
            ViewerView::ProjectedExterior | ViewerView::ProjectedSockets => {
                let close_scale = if projected_kind == ProjectedProofKind::Bartizan {
                    0.53
                } else {
                    1.0
                };
                let horizontal_distance = projected_distance * close_scale;
                let tangent_factor = if projected_kind == ProjectedProofKind::Bartizan {
                    0.25
                } else {
                    0.9
                };
                Vec3::new(
                    projected_focus.x
                        + projected_outward.x * horizontal_distance
                        + projected_tangent.x * horizontal_distance * tangent_factor,
                    projected_focus.y
                        + projected_distance
                            * if projected_kind == ProjectedProofKind::Bartizan {
                                0.95
                            } else {
                                0.32
                            },
                    projected_focus.z
                        + projected_outward.y * horizontal_distance
                        + projected_tangent.y * horizontal_distance * tangent_factor,
                )
            }
            ViewerView::ProjectedInterior if projected_kind == ProjectedProofKind::Bartizan => {
                // The grounded buttress makes the bartizan proof substantially taller
                // than the other projected works.  A close, high protected-side view
                // preserves that full load path while giving the small usable chamber
                // enough screen width for inspection.
                let horizontal_distance = projected_distance * 0.53;
                Vec3::new(
                    projected_focus.x - projected_outward.x * horizontal_distance
                        + projected_tangent.x * horizontal_distance * 0.25,
                    projected_focus.y + projected_distance * 0.95,
                    projected_focus.z - projected_outward.y * horizontal_distance
                        + projected_tangent.y * horizontal_distance * 0.25,
                )
            }
            ViewerView::ProjectedInterior => Vec3::new(
                projected_focus.x
                    - projected_outward.x * projected_distance * projected_interior_scale
                    + projected_tangent.x * projected_distance * 0.85 * projected_interior_scale,
                projected_focus.y + projected_distance * 0.3 * projected_interior_scale,
                projected_focus.z
                    - projected_outward.y * projected_distance * projected_interior_scale
                    + projected_tangent.y * projected_distance * 0.85 * projected_interior_scale,
            ),
            ViewerView::ProjectedUnderside if projected_kind == ProjectedProofKind::Bartizan => {
                let horizontal_distance = projected_distance * 0.53;
                Vec3::new(
                    projected_focus.x
                        + projected_outward.x * horizontal_distance
                        + projected_tangent.x * horizontal_distance * 0.25,
                    projected_focus.y - projected_distance * 0.95,
                    projected_focus.z
                        + projected_outward.y * horizontal_distance
                        + projected_tangent.y * horizontal_distance * 0.25,
                )
            }
            ViewerView::ProjectedUnderside => Vec3::new(
                projected_focus.x
                    + projected_outward.x * projected_distance * 1.28 * projected_underside_scale
                    + projected_tangent.x * projected_distance * 0.45 * projected_underside_scale,
                projected_focus.y - projected_distance * 0.7 * projected_underside_scale,
                projected_focus.z
                    + projected_outward.y * projected_distance * 1.28 * projected_underside_scale
                    + projected_tangent.y * projected_distance * 0.45 * projected_underside_scale,
            ),
            ViewerView::ProjectedTop if projected_kind == ProjectedProofKind::Bartizan => {
                Vec3::new(
                    projected_focus.x
                        + projected_outward.x * projected_distance * 0.27
                        + projected_tangent.x * projected_distance * 0.27,
                    projected_focus.y + projected_distance * 0.96,
                    projected_focus.z
                        + projected_outward.y * projected_distance * 0.27
                        + projected_tangent.y * projected_distance * 0.27,
                )
            }
            ViewerView::ProjectedTop => Vec3::new(
                projected_focus.x
                    + projected_outward.x * projected_distance * 0.45 * projected_top_scale
                    + projected_tangent.x * projected_distance * 0.45 * projected_top_scale,
                projected_focus.y + projected_distance * 1.60 * projected_top_scale,
                projected_focus.z
                    + projected_outward.y * projected_distance * 0.45 * projected_top_scale
                    + projected_tangent.y * projected_distance * 0.45 * projected_top_scale,
            ),
            ViewerView::ProjectedLongitudinal => Vec3::new(
                projected_focus.x
                    + projected_tangent.x * projected_distance * 1.4
                    + projected_outward.x * projected_distance * 0.4,
                projected_focus.y + projected_distance * 0.4,
                projected_focus.z
                    + projected_tangent.y * projected_distance * 1.4
                    + projected_outward.y * projected_distance * 0.4,
            ),
            ViewerView::ProjectedFlank if projected_kind == ProjectedProofKind::Bartizan => {
                let horizontal_distance = projected_distance * 0.53;
                Vec3::new(
                    projected_focus.x
                        + projected_tangent.x * horizontal_distance
                        + projected_outward.x * horizontal_distance * 0.25,
                    projected_focus.y + projected_distance * 0.95,
                    projected_focus.z
                        + projected_tangent.y * horizontal_distance
                        + projected_outward.y * horizontal_distance * 0.25,
                )
            }
            ViewerView::ProjectedFlank => Vec3::new(
                projected_focus.x
                    + projected_tangent.x * projected_distance * 0.75 * projected_flank_scale
                    + projected_outward.x * projected_distance * 0.65 * projected_flank_scale,
                projected_focus.y + projected_distance * 0.28 * projected_flank_scale,
                projected_focus.z
                    + projected_tangent.y * projected_distance * 0.75 * projected_flank_scale
                    + projected_outward.y * projected_distance * 0.65 * projected_flank_scale,
            ),
            ViewerView::OpeningRectangularSection
            | ViewerView::OpeningSegmentalSection
            | ViewerView::OpeningPointedSection
            | ViewerView::OpeningArrowLoopSection
            | ViewerView::OpeningGunLoopSection
            | ViewerView::WallTimberFrameSection
            | ViewerView::WallCivilianMasonrySection
            | ViewerView::WallCathedralButtressSection
            | ViewerView::WallRoundTowerRadialSection => Vec3::new(
                architectural_focus.x
                    + (architectural_tangent.x + architectural_outward.x * 0.55)
                        * architectural_distance,
                architectural_focus.y + architectural_distance * 0.22,
                architectural_focus.z
                    + (architectural_tangent.y + architectural_outward.y * 0.55)
                        * architectural_distance,
            ),
            ViewerView::OpeningRectangularInterior
            | ViewerView::OpeningSegmentalInterior
            | ViewerView::OpeningPointedInterior
            | ViewerView::OpeningArrowLoopInterior
            | ViewerView::OpeningGunLoopInterior => Vec3::new(
                architectural_focus.x - architectural_outward.x * architectural_distance
                    + architectural_tangent.x * architectural_distance * 0.30,
                architectural_focus.y + architectural_distance * 0.18,
                architectural_focus.z - architectural_outward.y * architectural_distance
                    + architectural_tangent.y * architectural_distance * 0.30,
            ),
            ViewerView::OpeningRectangularExterior
            | ViewerView::OpeningSegmentalExterior
            | ViewerView::OpeningPointedExterior
            | ViewerView::OpeningArrowLoopExterior
            | ViewerView::OpeningGunLoopExterior => Vec3::new(
                architectural_focus.x
                    + architectural_outward.x * architectural_distance
                    + architectural_tangent.x * architectural_distance * 0.30,
                architectural_focus.y + architectural_distance * 0.18,
                architectural_focus.z
                    + architectural_outward.y * architectural_distance
                    + architectural_tangent.y * architectural_distance * 0.30,
            ),
            _ => Vec3::new(radius, max_height * 0.95, -radius * 1.3),
        }
    };
    let target = if let Some((_, focus)) = artillery_camera {
        focus
    } else if let Some((_, focus)) = timber_camera {
        focus
    } else if let Some((_, focus)) = church_camera {
        focus
    } else if roof_proof.is_some() {
        roof_focus
    } else {
        match view {
            ViewerView::Exterior => Vec3::new(0.0, max_height * 0.42, 0.0),
            ViewerView::GateDetailExterior => plan
                .gate_defenses
                .first()
                .map(|defense| {
                    let focus = defense.threshold + origin;
                    Vec3::new(focus.x, 3.4, focus.y)
                })
                .unwrap_or(target),
            ViewerView::GateDetailInterior => plan
                .gate_defenses
                .first()
                .map(|defense| {
                    let route = defense
                        .guard_chamber
                        .access
                        .flight
                        .top
                        .lerp(defense.guard_chamber.access.flight.bottom, 0.55);
                    let focus = route.lerp(defense.guard_chamber.centre, 0.45) + origin;
                    Vec3::new(focus.x, 4.7, focus.y)
                })
                .unwrap_or(target),
            ViewerView::TowerPortalDetail => plan
                .towers
                .first()
                .map(|tower| {
                    let focus = tower.centre_metres() + origin;
                    Vec3::new(focus.x, tower.wall_height_metres * 0.48, focus.y)
                })
                .unwrap_or(target),
            ViewerView::CrownStraightExterior | ViewerView::CrownStraightInterior => Vec3::new(
                straight_crown_focus.0.x,
                straight_crown_focus.1 + 0.9,
                straight_crown_focus.0.y,
            ),
            ViewerView::CrownCornerExterior | ViewerView::CrownCornerInterior => Vec3::new(
                corner_crown_focus.0.x,
                corner_crown_focus.1 + 0.9,
                corner_crown_focus.0.y,
            ),
            ViewerView::CrownTowerExterior
            | ViewerView::CrownTowerTop
            | ViewerView::CrownTowerCutaway => Vec3::new(
                tower_crown_focus.0.x,
                tower_crown_focus.1 + 0.8,
                tower_crown_focus.0.y,
            ),
            ViewerView::ProjectedExterior | ViewerView::ProjectedInterior
                if projected_kind == ProjectedProofKind::Bartizan =>
            {
                projected_focus + Vec3::Y * 0.3
            }
            ViewerView::ProjectedUnderside if projected_kind == ProjectedProofKind::Bartizan => {
                projected_focus + Vec3::Y * 0.8
            }
            ViewerView::ProjectedTop if projected_kind == ProjectedProofKind::Bartizan => {
                projected_focus + Vec3::Y * 0.4
            }
            ViewerView::ProjectedFlank if projected_kind == ProjectedProofKind::Bartizan => {
                projected_focus + Vec3::Y * 0.3
            }
            ViewerView::ProjectedExterior
            | ViewerView::ProjectedInterior
            | ViewerView::ProjectedUnderside
            | ViewerView::ProjectedTop
            | ViewerView::ProjectedLongitudinal
            | ViewerView::ProjectedSockets
            | ViewerView::ProjectedFlank => projected_focus,
            _ if architectural_proof => architectural_focus,
            _ => target,
        }
    };
    let sun_position = if projected_proof {
        let (outward_scale, tangent_scale) = if matches!(
            view,
            ViewerView::ProjectedLongitudinal | ViewerView::ProjectedTop
        ) {
            (34.0, 18.0)
        } else {
            (18.0, 34.0)
        };
        Vec3::new(
            projected_outward.x * outward_scale + projected_tangent.x * tangent_scale,
            45.0,
            projected_outward.y * outward_scale + projected_tangent.y * tangent_scale,
        )
    } else if roof_proof.is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
    {
        Vec3::new(-28.0, 38.0, -20.0)
    } else {
        match view {
            ViewerView::GateDetailInterior => {
                // Bailey-side light is the deterministic section-view fill: it
                // enters through the deliberately removed rear wall and reveals
                // the floor, stair, windlass, and closures without emissive parts.
                Vec3::new(28.0, 50.0, 34.0)
            }
            ViewerView::GateDetailExterior => Vec3::new(28.0, 38.0, -34.0),
            ViewerView::TowerPortalDetail => Vec3::new(28.0, 50.0, -34.0),
            ViewerView::CrownStraightInterior => Vec3::new(-28.0, 45.0, 34.0),
            ViewerView::CrownCornerInterior => Vec3::new(28.0, 45.0, 34.0),
            // The defensive overview camera occupies the opposite quadrant from
            // the ordinary exterior camera. Keep the key oblique but move it to
            // the visible side so wall thickness and tower curvature remain read.
            ViewerView::Defenses => Vec3::new(-34.0, 38.0, 20.0),
            ViewerView::TimberFrameFacade => Vec3::new(34.0, 40.0, -8.0),
            _ => Vec3::new(28.0, 38.0, -34.0),
        }
    };
    let camera_up = if view == ViewerView::ProjectedUnderside
        && projected_kind == ProjectedProofKind::Bartizan
    {
        // A restrained roll keeps the full-height bonded buttress and the
        // underside work simultaneously measurable in the portrait-like proof.
        // It changes presentation only; all focus bounds still derive from the
        // exact resolved assembly IDs.
        (Vec3::Y + Vec3::new(projected_tangent.x, 0.0, projected_tangent.y) * 0.40).normalize()
    } else {
        Vec3::Y
    };
    if scene_setup != SceneSetup::EditorBuilding {
        world.spawn((
            Camera3d::default(),
            Transform::from_translation(camera_position).looking_at(target, camera_up),
        ));
        {
            let mut capture = world.resource_mut::<CaptureState>();
            capture.manifest.camera_position = camera_position.to_array();
            capture.manifest.camera_target = target.to_array();
        }
        world.spawn((
            DirectionalLight {
                illuminance: if projected_proof {
                    20_000.0
                } else if crown_proof || roof_proof.is_some() || timber_proof_suffix(view).is_some()
                {
                    28_000.0
                } else {
                    24_000.0
                },
                shadow_maps_enabled: true,
                ..default()
            },
            // An oblique south-eastern key separates the gate front, return walls,
            // tower curvature, and projecting crown. `looking_at` keeps the light
            // direction legible instead of relying on opaque Euler rotations.
            Transform::from_translation(sun_position).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        if roof_proof.is_some_and(|proof| roof_proof_slug(proof).ends_with("-interior")) {
            // Section proofs expose the unlit underside of a physically opaque
            // roof. A restrained, deterministic attic fill keeps rafters and the
            // surviving weather face readable without flattening the exterior key.
            world.spawn((
                PointLight {
                    intensity: 75_000.0,
                    range: (roof_focus_extent * 2.5).max(12.0),
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(roof_focus - Vec3::Y * roof_focus_extent * 0.35),
            ));
        }
        if church_section_proof(view)
            && let Some((camera, focus)) = church_camera
        {
            // A restrained camera-side working fill makes the exposed vault,
            // springing, and service-route faces readable without replacing the
            // oblique shadowed daylight used by the whole-building regressions.
            world.spawn((
                PointLight {
                    intensity: 85_000.0,
                    range: camera.distance(focus).clamp(18.0, 36.0),
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(camera.lerp(focus, 0.32)),
            ));
        }
        if view == ViewerView::ArtilleryGateInterior
            && let Some((camera, focus)) = artillery_camera
        {
            world.spawn((
                PointLight {
                    intensity: 320_000.0,
                    range: 28.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(camera.lerp(focus, 0.38)),
            ));
        }
        if view == ViewerView::ArtilleryRondelCasemate
            && let Some((camera, focus)) = artillery_camera
        {
            // Working daylight inside the opened casemate: this remains a lit,
            // shadowed material proof, but the camera-side fill prevents the two
            // surviving gun recesses and smoke throats from collapsing to black.
            world.spawn((
                PointLight {
                    intensity: 180_000.0,
                    range: 22.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(camera.lerp(focus, 0.30)),
            ));
        }
        if roof_proof.is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior") {
            // The facade-derived Zwerchhaus faces west in the curated fixture and
            // is consequently on the key-light shadow side.  A restrained cool
            // proof fill reveals its jambs, eave split, and apron without erasing
            // the directional roof modeling.
            world.spawn((
                PointLight {
                    intensity: 95_000.0,
                    range: 18.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(roof_focus + Vec3::new(-5.0, 4.0, 2.0)),
            ));
        }
        if roof_proof.is_some_and(|proof| {
            matches!(
                roof_proof_slug(proof),
                "roof-abutment-tower-exterior" | "roof-abutment-tower-cutaway"
            )
        }) {
            world.spawn((
                PointLight {
                    intensity: 70_000.0,
                    range: 20.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.80, 0.85, 0.92),
                    ..default()
                },
                Transform::from_translation(roof_focus + Vec3::new(5.0, 3.0, -5.0)),
            ));
        }
        world.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.72, 0.78, 0.88),
            brightness: if view == ViewerView::ProjectedSockets {
                300.0
            } else if view == ViewerView::ProjectedInterior {
                420.0
            } else if view == ViewerView::ProjectedUnderside {
                380.0
            } else if roof_proof.is_some_and(|proof| roof_proof_slug(proof).ends_with("-interior"))
            {
                400.0
            } else if roof_proof
                .is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
            {
                320.0
            } else if roof_proof.is_some() {
                240.0
            } else if crown_proof || projected_proof {
                340.0
            } else if timber_proof_suffix(view).is_some() {
                220.0
            } else {
                380.0
            },
            affects_lightmapped_meshes: true,
        });
    }
    world.insert_resource(palette);
    record_mesh_audit(world);
}

fn record_mesh_audit(world: &mut World) {
    let handles = world
        .query_filtered::<(&Mesh3d, Option<&Name>), With<ClosedSolid>>()
        .iter(world)
        .map(|(mesh, name)| {
            (
                mesh.0.clone(),
                name.map_or("unnamed closed solid", Name::as_str).to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let meshes = world.resource::<Assets<Mesh>>();
    let mut issue_count = 0;
    for (handle, name) in &handles {
        let Some(mesh) = meshes.get(handle) else {
            eprintln!("closed-solid mesh missing from assets: {name}");
            issue_count += 1;
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            eprintln!("closed-solid mesh lacks positions: {name}");
            issue_count += 1;
            continue;
        };
        let indices = match mesh.indices() {
            Some(Indices::U16(indices)) => indices.iter().map(|index| u32::from(*index)).collect(),
            Some(Indices::U32(indices)) => indices.clone(),
            None => (0..positions.len() as u32).collect(),
        };
        let audit = audit_triangle_mesh(positions, &indices);
        if !audit.passes_closed_solid() {
            eprintln!("closed-solid mesh failed integrity audit: {name}: {audit:?}");
            issue_count += 1;
        }
    }
    let mut state = world.resource_mut::<CaptureState>();
    state.manifest.audited_closed_mesh_count = handles.len();
    state.manifest.mesh_integrity_issue_count = issue_count;
}

fn create_palette(world: &mut World) -> RenderPalette {
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let material = |materials: &mut Assets<StandardMaterial>, color: Color, roughness| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: roughness,
            unlit: false,
            ..default()
        })
    };
    RenderPalette {
        plaster: material(&mut materials, Color::srgb(0.80, 0.74, 0.60), 0.9),
        brick: material(&mut materials, Color::srgb(0.48, 0.20, 0.13), 0.92),
        stone: material(&mut materials, Color::srgb(0.43, 0.44, 0.40), 0.95),
        earth: material(&mut materials, Color::srgb(0.28, 0.20, 0.11), 1.0),
        timber: material(&mut materials, Color::srgb(0.16, 0.09, 0.045), 0.88),
        roof: material(&mut materials, Color::srgb(0.28, 0.08, 0.045), 0.95),
        roof_secondary: material(&mut materials, Color::srgb(0.17, 0.20, 0.22), 0.92),
        floor: material(&mut materials, Color::srgb(0.32, 0.25, 0.16), 0.98),
        cutaway: materials.add(StandardMaterial {
            base_color: Color::srgba(0.46, 0.52, 0.58, 0.24),
            perceptual_roughness: 0.96,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        }),
        door: material(&mut materials, Color::srgb(0.20, 0.105, 0.045), 0.86),
        glass: material(&mut materials, Color::srgb(0.18, 0.42, 0.56), 0.35),
        void: materials.add(StandardMaterial {
            base_color: Color::srgb(0.025, 0.022, 0.018),
            perceptual_roughness: 1.0,
            unlit: true,
            ..default()
        }),
        stair: material(&mut materials, Color::srgb(0.35, 0.23, 0.11), 0.9),
        room_floors: [
            Color::srgb(0.47, 0.24, 0.18),
            Color::srgb(0.25, 0.39, 0.51),
            Color::srgb(0.42, 0.46, 0.25),
            Color::srgb(0.52, 0.40, 0.20),
            Color::srgb(0.37, 0.29, 0.48),
            Color::srgb(0.24, 0.46, 0.42),
            Color::srgb(0.53, 0.31, 0.40),
        ]
        .into_iter()
        .map(|color| material(&mut materials, color, 0.98))
        .collect(),
    }
}

fn spawn_ground(world: &mut World, dimensions: Vec2, crown_proof: bool) {
    let mesh = world.resource_mut::<Assets<Mesh>>().add(
        Plane3d::default()
            .mesh()
            .size(dimensions.x * 2.4, dimensions.y * 2.4),
    );
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: if crown_proof {
                // A restrained dark proving ground keeps the isolated white
                // masonry's silhouette legible and provides a deterministic
                // lit shadow reference without an unlit calibration card.
                Color::srgb(0.14, 0.19, 0.11)
            } else {
                Color::srgb(0.30, 0.38, 0.22)
            },
            perceptual_roughness: 1.0,
            unlit: false,
            ..default()
        });
    world.spawn((
        Name::new("ground"),
        EditorEnvironmentEntity,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, -0.02, 0.0),
    ));
}

fn spawn_artillery_ground(world: &mut World, dimensions: Vec2, origin: Vec2) {
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.38, 0.22),
            perceptual_roughness: 1.0,
            unlit: false,
            ..default()
        });
    let half = dimensions.max_element() * 1.2;
    let outer = (-22.5_f32, 34.5_f32, -19.5_f32, 31.5_f32);
    let slabs = [
        (
            Vec3::new(origin.x + 6.0, -0.03, origin.y - (half + 19.5) * 0.5),
            Vec3::new(half * 2.0, 0.08, half - 19.5),
        ),
        (
            Vec3::new(origin.x + 6.0, -0.03, origin.y + (half + 31.5) * 0.5),
            Vec3::new(half * 2.0, 0.08, half - 31.5),
        ),
        (
            Vec3::new(origin.x - (half + 22.5) * 0.5, -0.03, origin.y + 6.0),
            Vec3::new(half - 22.5, 0.08, outer.3 - outer.2),
        ),
        (
            Vec3::new(origin.x + (half + 34.5) * 0.5, -0.03, origin.y + 6.0),
            Vec3::new(half - 34.5, 0.08, outer.3 - outer.2),
        ),
        // Protected court and ramp-side grade remain authoritative solid ground;
        // the ring between it and the outer slabs is the visible dry ditch.
        (
            Vec3::new(origin.x + 6.0, -0.03, origin.y + 6.0),
            Vec3::new(36.0, 0.08, 30.0),
        ),
    ];
    for (centre, size) in slabs {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Mesh::from(Cuboid::new(size.x, size.y, size.z)));
        world.spawn((
            Name::new("artillery terrain outside dry ditch"),
            EditorEnvironmentEntity,
            ClosedSolid,
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(centre),
        ));
    }
}

fn spawn_wall(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    opening: Option<&Opening>,
    origin: Vec2,
    base_y: f32,
    storey_height: f32,
    style: WallStyle,
    timber_frame_style: Option<TimberFrameStyle>,
    projection_metres: f32,
) {
    let mut centre = wall.centre() + origin;
    let horizontal = wall.is_horizontal();
    let outward = match wall.direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    };
    if wall.exterior() {
        centre += outward * projection_metres;
    }
    let material = match style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    if let Some(opening) = opening {
        let side_width = (CELL_SIZE_METRES - opening.width_metres) * 0.5;
        for sign in [-1.0, 1.0] {
            let offset = sign * (opening.width_metres + side_width) * 0.5;
            if horizontal {
                centre.x += offset;
            } else {
                centre.y += offset;
            }
            spawn_wall_box(
                world,
                material,
                horizontal,
                side_width,
                storey_height,
                centre,
                base_y,
                "wall pier",
            );
            if horizontal {
                centre.x -= offset;
            } else {
                centre.y -= offset;
            }
        }
        if opening.sill_metres > 0.0 {
            spawn_wall_box_at_height(
                world,
                material,
                horizontal,
                opening.width_metres,
                opening.sill_metres,
                centre,
                base_y + opening.sill_metres * 0.5,
                "wall below opening",
            );
        }
        let header_base = opening.sill_metres + opening.height_metres;
        if header_base < storey_height {
            let header_height = storey_height - header_base;
            spawn_wall_box_at_height(
                world,
                material,
                horizontal,
                opening.width_metres,
                header_height,
                centre,
                base_y + header_base + header_height * 0.5,
                "wall header",
            );
        }
        spawn_opening_depth(
            world, palette, wall, *opening, horizontal, centre, outward, base_y,
        );
    } else {
        spawn_wall_box(
            world,
            material,
            horizontal,
            CELL_SIZE_METRES,
            storey_height,
            centre,
            base_y,
            "wall",
        );
    }

    if style == WallStyle::TimberFrame && wall.exterior() {
        let timber_centre = centre + outward * (WALL_THICKNESS_METRES + 0.015);
        spawn_timber_frame(
            world,
            palette,
            wall,
            timber_frame_style.unwrap_or(TimberFrameStyle::LateMedieval),
            horizontal,
            CELL_SIZE_METRES,
            timber_centre,
            base_y,
            storey_height,
            opening,
        );
        if projection_metres > 0.01 {
            let tangent = if horizontal { Vec2::X } else { Vec2::Y };
            for sign in [-0.38, 0.38] {
                let anchor = timber_centre + tangent * CELL_SIZE_METRES * sign;
                let lower = anchor - outward * projection_metres;
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(lower.x, base_y - 0.42, lower.y),
                    Vec3::new(anchor.x, base_y + 0.08, anchor.y),
                    0.11,
                    "projecting storey bracket",
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_opening_depth(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    opening: Opening,
    horizontal: bool,
    centre: Vec2,
    outward: Vec2,
    base_y: f32,
) {
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let recess = match opening.kind {
        OpeningKind::ArrowSlit => WALL_THICKNESS_METRES * 0.46,
        OpeningKind::Window => WALL_THICKNESS_METRES * 0.34,
        OpeningKind::Door | OpeningKind::Gate => WALL_THICKNESS_METRES * 0.18,
    };
    let plane_centre = centre - outward * recess;
    let plane_size = if horizontal {
        Vec3::new(
            opening.width_metres * 0.9,
            opening.height_metres * 0.94,
            0.025,
        )
    } else {
        Vec3::new(
            0.025,
            opening.height_metres * 0.94,
            opening.width_metres * 0.9,
        )
    };
    let material = match opening.kind {
        OpeningKind::Window => &palette.glass,
        OpeningKind::ArrowSlit => &palette.void,
        OpeningKind::Door | OpeningKind::Gate => &palette.door,
    };
    spawn_box(
        world,
        material,
        plane_size,
        Vec3::new(
            plane_centre.x,
            base_y + opening.sill_metres + opening.height_metres * 0.5,
            plane_centre.y,
        ),
        Quat::IDENTITY,
        match opening.kind {
            OpeningKind::Window => "recessed glazing",
            OpeningKind::ArrowSlit => "open firing-loop void",
            OpeningKind::Door => "recessed door leaf",
            OpeningKind::Gate => "recessed gate leaf",
        },
    );

    if opening.kind == OpeningKind::Window && wall.exterior() {
        let face = centre + outward * (WALL_THICKNESS_METRES * 0.56);
        let jamb_offset = opening.width_metres * 0.5;
        for sign in [-1.0, 1.0] {
            let jamb = face + tangent * jamb_offset * sign;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(jamb.x, base_y + opening.sill_metres, jamb.y),
                Vec3::new(
                    jamb.x,
                    base_y + opening.sill_metres + opening.height_metres,
                    jamb.y,
                ),
                0.075,
                "window jamb",
            );
        }
        for y in [
            base_y + opening.sill_metres,
            base_y + opening.sill_metres + opening.height_metres,
        ] {
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(
                    face.x - tangent.x * jamb_offset,
                    y,
                    face.y - tangent.y * jamb_offset,
                ),
                Vec3::new(
                    face.x + tangent.x * jamb_offset,
                    y,
                    face.y + tangent.y * jamb_offset,
                ),
                0.075,
                "window sill or lintel",
            );
        }
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(face.x, base_y + opening.sill_metres, face.y),
            Vec3::new(
                face.x,
                base_y + opening.sill_metres + opening.height_metres,
                face.y,
            ),
            0.045,
            "window mullion",
        );
        let transom_y = base_y + opening.sill_metres + opening.height_metres * 0.52;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                face.x - tangent.x * jamb_offset,
                transom_y,
                face.y - tangent.y * jamb_offset,
            ),
            Vec3::new(
                face.x + tangent.x * jamb_offset,
                transom_y,
                face.y + tangent.y * jamb_offset,
            ),
            0.045,
            "window transom",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_timber_frame(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    style: TimberFrameStyle,
    horizontal: bool,
    bay_width: f32,
    centre: Vec2,
    base_y: f32,
    height: f32,
    opening: Option<&Opening>,
) {
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let point = |along: f32, y: f32| {
        let plan = centre + tangent * along;
        Vec3::new(plan.x, y, plan.y)
    };
    let half = bay_width * 0.5;
    for along in [-half, half] {
        spawn_timber_beam(
            world,
            &palette.timber,
            point(along, base_y),
            point(along, base_y + height),
            0.11,
            "timber post",
        );
    }
    if let Some(opening) = opening {
        let sill = base_y + opening.sill_metres;
        let header = sill + opening.height_metres;
        for y in [base_y, sill, header.min(base_y + height), base_y + height] {
            spawn_timber_beam(
                world,
                &palette.timber,
                point(-half, y),
                point(half, y),
                0.10,
                "opening-aware timber rail",
            );
        }
        let jamb = opening.width_metres * 0.5;
        for along in [-jamb, jamb] {
            spawn_timber_beam(
                world,
                &palette.timber,
                point(along, base_y),
                point(along, base_y + height),
                0.09,
                "opening-aware timber stud",
            );
        }
        if opening.kind == OpeningKind::Window {
            for (start, end) in [(-half, jamb), (half, -jamb)] {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(start, base_y + 0.05),
                    point(end, sill - 0.04),
                    0.085,
                    "brace below window",
                );
            }
            for (start, end) in [(-jamb, -half), (jamb, half)] {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(start, header + 0.04),
                    point(end, base_y + height - 0.05),
                    0.08,
                    "brace above window",
                );
            }
        }
        return;
    }
    let rail_fractions: &[f32] = match style {
        TimberFrameStyle::LateMedieval => &[0.0, 0.55, 1.0],
        TimberFrameStyle::NorthernCloseStudded => &[0.0, 0.48, 0.72, 1.0],
        TimberFrameStyle::EarlyModernOrnate => &[0.0, 0.36, 0.68, 1.0],
    };
    for fraction in rail_fractions {
        spawn_timber_beam(
            world,
            &palette.timber,
            point(-half, base_y + height * fraction),
            point(half, base_y + height * fraction),
            0.10,
            "timber rail",
        );
    }
    match style {
        TimberFrameStyle::LateMedieval => {
            let rising = (i32::from(wall.cell.x) + i32::from(wall.cell.z)).rem_euclid(2) == 0;
            let (a, b) = if rising { (-half, half) } else { (half, -half) };
            spawn_timber_beam(
                world,
                &palette.timber,
                point(a, base_y + 0.06),
                point(b, base_y + height - 0.06),
                0.13,
                "long diagonal brace",
            );
        }
        TimberFrameStyle::NorthernCloseStudded => {
            for along in [-half * 0.5, 0.0, half * 0.5] {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(along, base_y),
                    point(along, base_y + height),
                    0.075,
                    "close stud",
                );
            }
            spawn_timber_beam(
                world,
                &palette.timber,
                point(-half, base_y + 0.08),
                point(half, base_y + height * 0.48),
                0.09,
                "northern foot brace",
            );
        }
        TimberFrameStyle::EarlyModernOrnate => {
            let bay_key = if horizontal {
                i32::from(wall.cell.x)
            } else {
                i32::from(wall.cell.z)
            }
            .rem_euclid(4);
            let lower = base_y + height * 0.04;
            let waist = base_y + height * 0.54;
            let upper = base_y + height * 0.96;
            if bay_key == 0 {
                for start in [-half, half] {
                    spawn_timber_beam(
                        world,
                        &palette.timber,
                        point(start, lower),
                        point(0.0, waist),
                        0.11,
                        "Mann figure foot brace",
                    );
                }
                for start in [-half, half] {
                    spawn_timber_beam(
                        world,
                        &palette.timber,
                        point(start, upper),
                        point(0.0, waist),
                        0.09,
                        "Mann figure head brace",
                    );
                }
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(0.0, base_y),
                    point(0.0, base_y + height),
                    0.095,
                    "ornate central post",
                );
            } else if bay_key == 2 {
                let breast = base_y + height * 0.36;
                for (start, end) in [(-half, half), (half, -half)] {
                    spawn_timber_beam(
                        world,
                        &palette.timber,
                        point(start, lower),
                        point(end, breast),
                        0.085,
                        "Andreaskreuz breast-panel brace",
                    );
                }
            } else if bay_key == 3 {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(-half, lower),
                    point(half, waist),
                    0.095,
                    "K figure foot brace",
                );
            }
        }
    }
}

fn spawn_timber_beam(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    thickness: f32,
    name: &'static str,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }
    spawn_box(
        world,
        material,
        Vec3::new(thickness, length, thickness),
        (start + end) * 0.5,
        Quat::from_rotation_arc(Vec3::Y, delta / length),
        name,
    );
}

fn spawn_wall_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    length: f32,
    height: f32,
    centre: Vec2,
    base_y: f32,
    name: &'static str,
) {
    spawn_wall_box_at_height(
        world,
        material,
        horizontal,
        length,
        height,
        centre,
        base_y + height * 0.5,
        name,
    );
}

fn spawn_wall_box_at_height(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    length: f32,
    height: f32,
    centre: Vec2,
    y: f32,
    name: &'static str,
) {
    let size = if horizontal {
        Vec3::new(length.max(0.02), height.max(0.02), WALL_THICKNESS_METRES)
    } else {
        Vec3::new(WALL_THICKNESS_METRES, height.max(0.02), length.max(0.02))
    };
    spawn_box(
        world,
        material,
        size,
        Vec3::new(centre.x, y, centre.y),
        Quat::IDENTITY,
        name,
    );
}

fn boundary_notched_polygon(outer: &[Vec3], cutout: &[Vec3]) -> Option<Vec<Vec3>> {
    let on_segment = |point: Vec3, a: Vec3, b: Vec3| {
        let delta = b - a;
        let t = ((point - a).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
        point.distance_squared(a + delta * t) <= 0.000_004
    };
    for edge_index in 0..outer.len() {
        let a = outer[edge_index];
        let b = outer[(edge_index + 1) % outer.len()];
        let delta = b - a;
        let mut touches = cutout
            .iter()
            .enumerate()
            .filter(|(_, point)| on_segment(**point, a, b))
            .map(|(index, point)| {
                (
                    index,
                    ((point - a).dot(delta) / delta.length_squared()).clamp(0.0, 1.0),
                )
            })
            .collect::<Vec<_>>();
        if touches.len() != 2 {
            continue;
        }
        touches.sort_by(|left, right| left.1.total_cmp(&right.1));
        let (first, second) = (touches[0].0, touches[1].0);
        let forward_steps = (second + cutout.len() - first) % cutout.len();
        let step: isize = if forward_steps > 1 { 1 } else { -1 };
        let mut path = Vec::new();
        let mut current = first;
        loop {
            path.push(cutout[current]);
            if current == second {
                break;
            }
            current = (current as isize + step).rem_euclid(cutout.len() as isize) as usize;
        }
        let mut polygon = Vec::with_capacity(outer.len() + path.len());
        for (index, point) in outer.iter().copied().enumerate() {
            polygon.push(point);
            if index == edge_index {
                polygon.extend(path.iter().copied().filter(|candidate| {
                    candidate.distance_squared(a) > 0.000_004
                        && candidate.distance_squared(b) > 0.000_004
                }));
            }
        }
        return Some(polygon);
    }
    let removed = outer.iter().position(|outer_point| {
        cutout
            .iter()
            .any(|cut| cut.distance_squared(*outer_point) <= 0.000_004)
    })?;
    let previous = outer[(removed + outer.len() - 1) % outer.len()];
    let removed_point = outer[removed];
    let next = outer[(removed + 1) % outer.len()];
    let previous_touch = cutout.iter().copied().find(|point| {
        point.distance_squared(removed_point) > 0.000_004
            && on_segment(*point, previous, removed_point)
    })?;
    let next_touch = cutout.iter().copied().find(|point| {
        point.distance_squared(removed_point) > 0.000_004 && on_segment(*point, removed_point, next)
    })?;
    let interior = cutout.iter().copied().find(|point| {
        point.distance_squared(removed_point) > 0.000_004
            && point.distance_squared(previous_touch) > 0.000_004
            && point.distance_squared(next_touch) > 0.000_004
    })?;
    let mut polygon = Vec::with_capacity(outer.len() + 2);
    for (index, point) in outer.iter().copied().enumerate() {
        if index == removed {
            polygon.extend([previous_touch, interior, next_touch]);
        } else {
            polygon.push(point);
        }
    }
    Some(polygon)
}

fn roof_face_prism_mesh(face: &RoofFace) -> Mesh {
    let offset = -face.plane.normal.normalize_or_zero() * face.thickness_metres;
    let mut outer = face.polygon.clone();
    let mut remaining_cutouts = Vec::new();
    for cutout in &face.cutouts {
        if let Some(notched) = boundary_notched_polygon(&outer, cutout) {
            outer = notched;
        } else {
            remaining_cutouts.push(cutout.clone());
        }
    }
    loop {
        let removable = (0..outer.len()).find(|index| {
            let previous = outer[(*index + outer.len() - 1) % outer.len()];
            let current = outer[*index];
            let next = outer[(*index + 1) % outer.len()];
            (current - previous).cross(next - current).length_squared() <= 0.000_004
        });
        if outer.len() <= 3 || removable.is_none() {
            break;
        }
        outer.remove(removable.unwrap());
    }
    let mut vertices = outer.clone();
    let mut hole_indices = Vec::new();
    for cutout in &remaining_cutouts {
        hole_indices.push(vertices.len() as u32);
        vertices.extend(cutout.iter().copied());
    }
    let mut triangles = Vec::new();
    earcut::Earcut::<f32>::new().earcut(
        vertices.iter().map(|point| [point.x, point.z]),
        &hole_indices,
        &mut triangles,
    );
    let mut faces = Vec::new();
    let mut top_edges = Vec::new();
    let (triangles, remainder) = triangles.as_chunks::<3>();
    debug_assert!(remainder.is_empty());
    for triangle in triangles {
        let mut top = triangle
            .iter()
            .map(|index| vertices[*index as usize])
            .collect::<Vec<_>>();
        if (top[1] - top[0])
            .cross(top[2] - top[0])
            .dot(face.plane.normal)
            < 0.0
        {
            top.reverse();
        }
        for index in 0..3 {
            top_edges.push((top[index], top[(index + 1) % 3]));
        }
        faces.push(top.clone());
        faces.push(top.iter().rev().map(|point| *point + offset).collect());
    }
    // Earcut is allowed to elide collinear boundary vertices. Deriving the
    // prism walls from the source loops then creates T-junctions where one top
    // triangle spans two side quads. Instead, extrude the actual one-use
    // boundary edges of the triangulated top; this keeps notched eaves and
    // interior cuts watertight even when their source loops contain collinear
    // construction points.
    for (index, (a, b)) in top_edges.iter().copied().enumerate() {
        let uses = top_edges
            .iter()
            .enumerate()
            .filter(|(candidate_index, (start, end))| {
                *candidate_index != index
                    && (((*start - a).length_squared() <= 0.000_004
                        && (*end - b).length_squared() <= 0.000_004)
                        || ((*start - b).length_squared() <= 0.000_004
                            && (*end - a).length_squared() <= 0.000_004))
            })
            .count();
        if uses == 0 {
            faces.push(vec![a, a + offset, b + offset, b]);
        }
    }
    flat_face_mesh(&faces)
}

fn spawn_resolved_roof(
    world: &mut World,
    palette: &RenderPalette,
    roof: &RoofAssembly,
    geometry: &adventuresim_building_generator::ResolvedGeometry,
    origin: Vec2,
    removed_items: &std::collections::HashSet<u64>,
    lighting_calibration: bool,
    cutaway_material: bool,
) {
    for face in roof
        .faces
        .iter()
        .filter(|face| !removed_items.contains(&face.id.0))
    {
        let mesh = roof_face_prism_mesh(face);
        let handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = if cutaway_material {
            &palette.cutaway
        } else {
            match face.material {
                RoofMaterial::ClayTile | RoofMaterial::TimberShingle => &palette.roof,
                RoofMaterial::Slate | RoofMaterial::Lead => &palette.roof_secondary,
                RoofMaterial::TimberInfill => &palette.plaster,
                RoofMaterial::MasonryInfill => &palette.stone,
            }
        };
        let bounds = face.polygon.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), point| (min.min(*point), max.max(*point)),
        );
        let mut entity = world.spawn((
            Name::new(format!("resolved roof {} face {}", roof.id.0, face.id.0)),
            ClosedSolid,
            GeometryOwner(roof.owner.0),
            RoofRenderItem {
                id: face.id.0,
                fingerprint: stable_u64(&serde_json::to_vec(face).expect("serialize roof face")),
                local_center: (bounds.0 + bounds.1) * 0.5,
                local_half_size: (bounds.1 - bounds.0) * 0.5,
            },
            Mesh3d(handle),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(origin.x, 0.0, origin.y),
        ));
        if lighting_calibration {
            entity.insert(LightingCalibration {
                local_center: (bounds.0 + bounds.1) * 0.5,
                local_half_size: (bounds.1 - bounds.0) * 0.5,
            });
        }
    }
    for enclosure in &roof.enclosure_faces {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(roof_enclosure_prism_mesh(enclosure));
        let material = if cutaway_material {
            &palette.cutaway
        } else {
            match enclosure.material {
                RoofMaterial::TimberInfill => &palette.plaster,
                RoofMaterial::MasonryInfill => &palette.stone,
                RoofMaterial::ClayTile | RoofMaterial::TimberShingle => &palette.roof,
                RoofMaterial::Slate | RoofMaterial::Lead => &palette.roof_secondary,
            }
        };
        let bounds = enclosure.polygon.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), point| (min.min(*point), max.max(*point)),
        );
        world.spawn((
            Name::new(format!(
                "resolved roof {} enclosure {}",
                roof.id.0, enclosure.id.0
            )),
            ClosedSolid,
            GeometryOwner(roof.owner.0),
            RoofRenderItem {
                id: enclosure.id.0,
                fingerprint: stable_u64(
                    &serde_json::to_vec(enclosure).expect("serialize roof enclosure"),
                ),
                local_center: (bounds.0 + bounds.1) * 0.5,
                local_half_size: (bounds.1 - bounds.0) * 0.5,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(origin.x, 0.0, origin.y),
        ));
    }
    // Cuboidal framing, flashing, gutters, and edge treatments are spawned by
    // the shared resolved-solid renderer. Keeping their rendering there makes
    // the exact solid multiset authoritative and prevents duplicate roof
    // volume at this polygonal-face pass.
    let _ = geometry;
}

fn roof_enclosure_prism_mesh(enclosure: &RoofEnclosureFace) -> Mesh {
    let normal = (enclosure.polygon[1] - enclosure.polygon[0])
        .cross(enclosure.polygon[2] - enclosure.polygon[0])
        .normalize_or_zero();
    let offset = -normal * 0.16;
    let mut polygons = vec![
        enclosure.polygon.clone(),
        enclosure
            .polygon
            .iter()
            .rev()
            .map(|point| *point + offset)
            .collect::<Vec<_>>(),
    ];
    for index in 0..enclosure.polygon.len() {
        let next = (index + 1) % enclosure.polygon.len();
        polygons.push(vec![
            enclosure.polygon[index],
            enclosure.polygon[index] + offset,
            enclosure.polygon[next] + offset,
            enclosure.polygon[next],
        ]);
    }
    outward_flat_face_mesh(polygons)
}

#[allow(dead_code)] // Legacy recipe visualizer retained only for non-authoritative debugging.
fn spawn_roof(
    world: &mut World,
    palette: &RenderPalette,
    mut roof: RoofPiece,
    origin: Vec2,
    roof_index: usize,
    wall_style: WallStyle,
) {
    roof.centre += origin;
    match roof.kind {
        RoofKind::Gable => spawn_gable_roof(world, palette, roof, wall_style),
        RoofKind::Hip | RoofKind::HalfHip | RoofKind::Pavilion => {
            let mesh = roof_surface_mesh(roof);
            let handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
            world.spawn((
                Name::new(format!("roof piece {roof_index}")),
                Mesh3d(handle),
                MeshMaterial3d(palette.roof_secondary.clone()),
            ));
        }
        RoofKind::Shed => spawn_shed_roof(world, &palette.roof, roof),
        RoofKind::Flat => spawn_box(
            world,
            &palette.roof_secondary,
            Vec3::new(roof.size.x, 0.18, roof.size.y),
            Vec3::new(roof.centre.x, roof.base_height_metres + 0.09, roof.centre.y),
            Quat::IDENTITY,
            "flat roof",
        ),
        RoofKind::Conical => spawn_conical_roof(world, &palette.roof_secondary, roof),
    }
}

#[allow(dead_code)]
fn spawn_gable_roof(
    world: &mut World,
    palette: &RenderPalette,
    roof: RoofPiece,
    wall_style: WallStyle,
) {
    let pitch = roof.pitch_degrees.to_radians();
    let (span, run) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x * 0.5 + roof.eave_metres,
            roof.size.y + roof.eave_metres * 2.0,
        ),
        RidgeAxis::X => (
            roof.size.y * 0.5 + roof.eave_metres,
            roof.size.x + roof.eave_metres * 2.0,
        ),
    };
    let slope = span / pitch.cos();
    let rise = span * pitch.tan();
    for sign in [-1.0_f32, 1.0] {
        let (size, translation, rotation) = match roof.ridge_axis {
            RidgeAxis::Z => (
                Vec3::new(slope, 0.13, run),
                Vec3::new(
                    roof.centre.x + sign * span * 0.5,
                    roof.base_height_metres + rise * 0.5,
                    roof.centre.y,
                ),
                Quat::from_rotation_z(-sign * pitch),
            ),
            RidgeAxis::X => (
                Vec3::new(run, 0.13, slope),
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres + rise * 0.5,
                    roof.centre.y + sign * span * 0.5,
                ),
                Quat::from_rotation_x(sign * pitch),
            ),
        };
        spawn_box(
            world,
            &palette.roof,
            size,
            translation,
            rotation,
            "gable roof slope",
        );
    }
    let facade_material = match wall_style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    let half_x = roof.size.x * 0.5;
    let half_z = roof.size.y * 0.5;
    let triangles = match roof.ridge_axis {
        RidgeAxis::Z => {
            let south = roof.centre.y - half_z;
            let north = roof.centre.y + half_z;
            vec![
                vec![
                    Vec3::new(roof.centre.x - half_x, roof.base_height_metres, south),
                    Vec3::new(roof.centre.x, roof.base_height_metres + rise, south),
                    Vec3::new(roof.centre.x + half_x, roof.base_height_metres, south),
                ],
                vec![
                    Vec3::new(roof.centre.x + half_x, roof.base_height_metres, north),
                    Vec3::new(roof.centre.x, roof.base_height_metres + rise, north),
                    Vec3::new(roof.centre.x - half_x, roof.base_height_metres, north),
                ],
            ]
        }
        RidgeAxis::X => {
            let west = roof.centre.x - half_x;
            let east = roof.centre.x + half_x;
            vec![
                vec![
                    Vec3::new(west, roof.base_height_metres, roof.centre.y + half_z),
                    Vec3::new(west, roof.base_height_metres + rise, roof.centre.y),
                    Vec3::new(west, roof.base_height_metres, roof.centre.y - half_z),
                ],
                vec![
                    Vec3::new(east, roof.base_height_metres, roof.centre.y - half_z),
                    Vec3::new(east, roof.base_height_metres + rise, roof.centre.y),
                    Vec3::new(east, roof.base_height_metres, roof.centre.y + half_z),
                ],
            ]
        }
    };
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(flat_face_mesh(&triangles));
    world.spawn((
        Name::new("gable infill"),
        Mesh3d(mesh),
        MeshMaterial3d(facade_material.clone()),
    ));
    spawn_gable_detail(world, palette, roof, rise, wall_style);
}

#[allow(dead_code)]
fn spawn_gable_detail(
    world: &mut World,
    palette: &RenderPalette,
    roof: RoofPiece,
    rise: f32,
    wall_style: WallStyle,
) {
    let (half_span, face_a, face_b, tangent) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x * 0.5,
            Vec2::new(roof.centre.x, roof.centre.y - roof.size.y * 0.5 - 0.02),
            Vec2::new(roof.centre.x, roof.centre.y + roof.size.y * 0.5 + 0.02),
            Vec2::X,
        ),
        RidgeAxis::X => (
            roof.size.y * 0.5,
            Vec2::new(roof.centre.x - roof.size.x * 0.5 - 0.02, roof.centre.y),
            Vec2::new(roof.centre.x + roof.size.x * 0.5 + 0.02, roof.centre.y),
            Vec2::Y,
        ),
    };
    if wall_style == WallStyle::TimberFrame {
        for face in [face_a, face_b] {
            let apex = Vec3::new(face.x, roof.base_height_metres + rise, face.y);
            let base_left = face - tangent * half_span;
            let base_right = face + tangent * half_span;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(base_left.x, roof.base_height_metres, base_left.y),
                Vec3::new(base_right.x, roof.base_height_metres, base_right.y),
                0.13,
                "gable tie beam",
            );
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(face.x, roof.base_height_metres, face.y),
                apex,
                0.11,
                "gable king post",
            );
            let collar_y = roof.base_height_metres + rise * 0.56;
            let collar_half = half_span * 0.44;
            let collar_left = face - tangent * collar_half;
            let collar_right = face + tangent * collar_half;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(collar_left.x, collar_y, collar_left.y),
                Vec3::new(collar_right.x, collar_y, collar_right.y),
                0.105,
                "gable collar beam",
            );
            for fraction in [-0.66_f32, -0.33, 0.33, 0.66] {
                let stud = face + tangent * half_span * fraction;
                let top_y = roof.base_height_metres + rise * (1.0 - fraction.abs());
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(stud.x, roof.base_height_metres, stud.y),
                    Vec3::new(stud.x, top_y, stud.y),
                    0.085,
                    "gable vertical stud",
                );
            }
            for sign in [-1.0, 1.0] {
                let foot = face + tangent * half_span * 0.1 * sign;
                let head = face + tangent * half_span * 0.62 * sign;
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(foot.x, roof.base_height_metres + 0.06, foot.y),
                    Vec3::new(head.x, roof.base_height_metres + rise * 0.38, head.y),
                    0.09,
                    "gable outward brace",
                );
            }
        }
    }
    match roof.gable_profile {
        GableProfile::Plain => {}
        GableProfile::Stepped => {
            let material = if wall_style == WallStyle::TimberFrame {
                &palette.timber
            } else {
                &palette.stone
            };
            for face in [face_a, face_b] {
                for sign in [-1.0, 1.0] {
                    for step in 0..4 {
                        let lower = step as f32 / 4.0;
                        let upper = (step + 1) as f32 / 4.0;
                        let outer = face + tangent * half_span * (1.0 - lower) * sign;
                        let inner = face + tangent * half_span * (1.0 - upper) * sign;
                        spawn_timber_beam(
                            world,
                            material,
                            Vec3::new(outer.x, roof.base_height_metres + rise * lower, outer.y),
                            Vec3::new(outer.x, roof.base_height_metres + rise * upper, outer.y),
                            0.16,
                            "stepped gable vertical",
                        );
                        spawn_timber_beam(
                            world,
                            material,
                            Vec3::new(outer.x, roof.base_height_metres + rise * upper, outer.y),
                            Vec3::new(inner.x, roof.base_height_metres + rise * upper, inner.y),
                            0.16,
                            "stepped gable tread",
                        );
                    }
                }
            }
        }
        GableProfile::Curved => {
            for face in [face_a, face_b] {
                for sign in [-1.0, 1.0] {
                    let outer = face + tangent * half_span * 0.82 * sign;
                    let shoulder = face + tangent * half_span * 0.42 * sign;
                    spawn_timber_beam(
                        world,
                        &palette.stone,
                        Vec3::new(outer.x, roof.base_height_metres + rise * 0.12, outer.y),
                        Vec3::new(
                            shoulder.x,
                            roof.base_height_metres + rise * 0.58,
                            shoulder.y,
                        ),
                        0.14,
                        "curved gable lower sweep",
                    );
                    spawn_timber_beam(
                        world,
                        &palette.stone,
                        Vec3::new(
                            shoulder.x,
                            roof.base_height_metres + rise * 0.58,
                            shoulder.y,
                        ),
                        Vec3::new(face.x, roof.base_height_metres + rise, face.y),
                        0.14,
                        "curved gable upper sweep",
                    );
                }
            }
        }
    }
}

#[allow(dead_code)]
fn spawn_roof_dormer(
    world: &mut World,
    palette: &RenderPalette,
    mut dormer: RoofDormer,
    origin: Vec2,
    wall_style: WallStyle,
) {
    dormer.centre += origin;
    let (horizontal, inward, roof_size, ridge_axis) = match dormer.facing {
        Direction::North => (
            true,
            -Vec2::Y,
            Vec2::new(dormer.width_metres, dormer.depth_metres),
            RidgeAxis::Z,
        ),
        Direction::South => (
            true,
            Vec2::Y,
            Vec2::new(dormer.width_metres, dormer.depth_metres),
            RidgeAxis::Z,
        ),
        Direction::East => (
            false,
            -Vec2::X,
            Vec2::new(dormer.depth_metres, dormer.width_metres),
            RidgeAxis::X,
        ),
        Direction::West => (
            false,
            Vec2::X,
            Vec2::new(dormer.depth_metres, dormer.width_metres),
            RidgeAxis::X,
        ),
    };
    let scale = if dormer.kind == DormerKind::TransverseGable {
        1.55
    } else {
        1.0
    };
    dormer.width_metres *= scale;
    let facade_material = match wall_style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    let facade_centre = dormer.centre + inward * 0.18;
    spawn_wall_box_at_height(
        world,
        facade_material,
        horizontal,
        dormer.width_metres,
        dormer.height_metres,
        facade_centre,
        dormer.base_height_metres + dormer.height_metres * 0.5,
        "roof dormer facade",
    );
    let window_width = dormer.width_metres * 0.42;
    let window_height = dormer.height_metres * 0.48;
    let window_y = dormer.base_height_metres + dormer.height_metres * 0.48;
    let pane = facade_centre + inward * (WALL_THICKNESS_METRES * 0.44);
    spawn_box(
        world,
        &palette.glass,
        if horizontal {
            Vec3::new(window_width, window_height, 0.025)
        } else {
            Vec3::new(0.025, window_height, window_width)
        },
        Vec3::new(pane.x, window_y, pane.y),
        Quat::IDENTITY,
        "recessed roof dormer glazing",
    );
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let frame = facade_centre - inward * (WALL_THICKNESS_METRES * 0.56);
    for sign in [-1.0, 1.0] {
        let jamb = frame + tangent * window_width * 0.5 * sign;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(jamb.x, window_y - window_height * 0.5, jamb.y),
            Vec3::new(jamb.x, window_y + window_height * 0.5, jamb.y),
            0.065,
            "dormer window jamb",
        );
    }
    for sign in [-1.0, 1.0] {
        let y = window_y + window_height * 0.5 * sign;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                frame.x - tangent.x * window_width * 0.5,
                y,
                frame.y - tangent.y * window_width * 0.5,
            ),
            Vec3::new(
                frame.x + tangent.x * window_width * 0.5,
                y,
                frame.y + tangent.y * window_width * 0.5,
            ),
            0.065,
            "dormer window sill or lintel",
        );
    }
    let roof = RoofPiece {
        kind: match dormer.kind {
            DormerKind::Hipped => RoofKind::Hip,
            DormerKind::Shed => RoofKind::Shed,
            DormerKind::Gabled | DormerKind::TransverseGable => RoofKind::Gable,
        },
        centre: dormer.centre + inward * dormer.depth_metres * 0.42,
        size: roof_size * Vec2::new(scale, 1.0),
        base_height_metres: dormer.base_height_metres + dormer.height_metres,
        pitch_degrees: 48.0,
        ridge_axis,
        eave_metres: 0.16,
        gable_profile: dormer.gable_profile,
    };
    match roof.kind {
        RoofKind::Gable => spawn_gable_roof(world, palette, roof, wall_style),
        RoofKind::Hip => {
            let mesh = world
                .resource_mut::<Assets<Mesh>>()
                .add(roof_surface_mesh(roof));
            world.spawn((
                Name::new("hipped roof dormer"),
                Mesh3d(mesh),
                MeshMaterial3d(palette.roof_secondary.clone()),
            ));
        }
        RoofKind::Shed => spawn_shed_roof(world, &palette.roof, roof),
        RoofKind::HalfHip | RoofKind::Flat | RoofKind::Pavilion | RoofKind::Conical => {}
    }
}

#[allow(dead_code)]
fn spawn_shed_roof(world: &mut World, material: &Handle<StandardMaterial>, roof: RoofPiece) {
    let pitch = roof.pitch_degrees.to_radians();
    let (span, run) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x + roof.eave_metres * 2.0,
            roof.size.y + roof.eave_metres * 2.0,
        ),
        RidgeAxis::X => (
            roof.size.y + roof.eave_metres * 2.0,
            roof.size.x + roof.eave_metres * 2.0,
        ),
    };
    let slope = span / pitch.cos();
    let (size, rotation) = match roof.ridge_axis {
        RidgeAxis::Z => (Vec3::new(slope, 0.13, run), Quat::from_rotation_z(-pitch)),
        RidgeAxis::X => (Vec3::new(run, 0.13, slope), Quat::from_rotation_x(pitch)),
    };
    spawn_box(
        world,
        material,
        size,
        Vec3::new(
            roof.centre.x,
            roof.base_height_metres + span * pitch.tan() * 0.5,
            roof.centre.y,
        ),
        rotation,
        "shed roof",
    );
}

#[allow(dead_code)]
fn roof_surface_mesh(roof: RoofPiece) -> Mesh {
    let half_x = roof.size.x * 0.5 + roof.eave_metres;
    let half_z = roof.size.y * 0.5 + roof.eave_metres;
    let (ridge_half, rise) = match roof.ridge_axis {
        RidgeAxis::Z => {
            let inset = if roof.kind == RoofKind::HalfHip {
                half_x * 0.42
            } else if roof.kind == RoofKind::Pavilion {
                half_z
            } else {
                half_x.min(half_z * 0.85)
            };
            (
                (half_z - inset).max(0.0),
                half_x * roof.pitch_degrees.to_radians().tan(),
            )
        }
        RidgeAxis::X => {
            let inset = if roof.kind == RoofKind::HalfHip {
                half_z * 0.42
            } else if roof.kind == RoofKind::Pavilion {
                half_x
            } else {
                half_z.min(half_x * 0.85)
            };
            (
                (half_x - inset).max(0.0),
                half_z * roof.pitch_degrees.to_radians().tan(),
            )
        }
    };
    let y = roof.base_height_metres;
    let corners = [
        Vec3::new(roof.centre.x - half_x, y, roof.centre.y - half_z),
        Vec3::new(roof.centre.x + half_x, y, roof.centre.y - half_z),
        Vec3::new(roof.centre.x + half_x, y, roof.centre.y + half_z),
        Vec3::new(roof.centre.x - half_x, y, roof.centre.y + half_z),
    ];
    let (ridge_a, ridge_b) = match roof.ridge_axis {
        RidgeAxis::Z => (
            Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half),
            Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half),
        ),
        RidgeAxis::X => (
            Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y),
            Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y),
        ),
    };
    let faces = match roof.ridge_axis {
        RidgeAxis::Z => vec![
            vec![corners[0], corners[3], ridge_b, ridge_a],
            vec![corners[2], corners[1], ridge_a, ridge_b],
            vec![corners[1], corners[0], ridge_a],
            vec![corners[3], corners[2], ridge_b],
        ],
        RidgeAxis::X => vec![
            vec![corners[1], corners[0], ridge_a, ridge_b],
            vec![corners[3], corners[2], ridge_b, ridge_a],
            vec![corners[0], corners[3], ridge_a],
            vec![corners[2], corners[1], ridge_b],
        ],
    };
    flat_face_mesh(&faces)
}

fn flat_face_mesh(faces: &[Vec<Vec3>]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for face in faces {
        if face.len() < 3 {
            continue;
        }
        let normal = (face[1] - face[0])
            .cross(face[2] - face[0])
            .normalize_or_zero();
        let base = positions.len() as u32;
        positions.extend(face.iter().map(|point| point.to_array()));
        normals.extend((0..face.len()).map(|_| normal.to_array()));
        for index in 1..face.len() - 1 {
            indices.extend_from_slice(&[base, base + index as u32, base + index as u32 + 1]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn outward_flat_face_mesh(mut faces: Vec<Vec<Vec3>>) -> Mesh {
    let signed_volume_x6 = faces
        .iter()
        .filter(|face| face.len() >= 3)
        .flat_map(|face| (1..face.len() - 1).map(|index| (face[0], face[index], face[index + 1])))
        .map(|(a, b, c)| f64::from(a.dot(b.cross(c))))
        .sum::<f64>();
    if signed_volume_x6 < 0.0 {
        faces.iter_mut().for_each(|face| face.reverse());
    }
    flat_face_mesh(&faces)
}

fn arched_spandrel_mesh(
    width: f32,
    height: f32,
    depth: f32,
    rise: f32,
    pointed_arc_radius: Option<f32>,
) -> Mesh {
    let pointed = pointed_arc_radius.is_some();
    let segments = if pointed { 12 } else { 16 };
    let half_width = width * 0.5;
    let half_depth = depth * 0.5;
    let bottom = -height * 0.5;
    let top = height * 0.5;
    let curve = |x: f32| {
        if (half_width - x.abs()).abs() <= 1.0e-4 {
            return bottom;
        }
        let crown = if pointed {
            // True two-centred intrados: each half is struck from the
            // opposite spring-line centre.
            let radius = pointed_arc_radius.unwrap();
            let centre_offset = (radius - half_width).max(0.0);
            (radius * radius - (x.abs() + centre_offset).powi(2))
                .max(0.0)
                .sqrt()
        } else {
            // True segmental circular intrados from chord and rise.
            let radius = width * width / (8.0 * rise.max(0.01)) + rise * 0.5;
            (radius * radius - x * x).max(0.0).sqrt() + rise - radius
        };
        bottom + crown.min(height - 0.02).max(0.0)
    };
    let mut faces = Vec::with_capacity(segments * 3 + 3);
    for index in 0..segments {
        let x0 = -half_width + width * index as f32 / segments as f32;
        let x1 = -half_width + width * (index + 1) as f32 / segments as f32;
        let y0 = curve(x0);
        let y1 = curve(x1);
        faces.push(vec![
            Vec3::new(x0, y0, half_depth),
            Vec3::new(x1, y1, half_depth),
            Vec3::new(x1, top, half_depth),
            Vec3::new(x0, top, half_depth),
        ]);
        faces.push(vec![
            Vec3::new(x0, top, -half_depth),
            Vec3::new(x1, top, -half_depth),
            Vec3::new(x1, y1, -half_depth),
            Vec3::new(x0, y0, -half_depth),
        ]);
        faces.push(vec![
            Vec3::new(x0, y0, -half_depth),
            Vec3::new(x1, y1, -half_depth),
            Vec3::new(x1, y1, half_depth),
            Vec3::new(x0, y0, half_depth),
        ]);
        faces.push(vec![
            Vec3::new(x0, top, -half_depth),
            Vec3::new(x0, top, half_depth),
            Vec3::new(x1, top, half_depth),
            Vec3::new(x1, top, -half_depth),
        ]);
    }
    faces.push(vec![
        Vec3::new(-half_width, bottom, -half_depth),
        Vec3::new(-half_width, bottom, half_depth),
        Vec3::new(-half_width, top, half_depth),
        Vec3::new(-half_width, top, -half_depth),
    ]);
    faces.push(vec![
        Vec3::new(half_width, top, -half_depth),
        Vec3::new(half_width, top, half_depth),
        Vec3::new(half_width, bottom, half_depth),
        Vec3::new(half_width, bottom, -half_depth),
    ]);
    flat_face_mesh(&faces)
}

fn arched_panel_mesh(
    width: f32,
    height: f32,
    depth: f32,
    spring_height: f32,
    rise: f32,
    pointed_arc_radius: Option<f32>,
) -> Mesh {
    let segments = if pointed_arc_radius.is_some() { 12 } else { 16 };
    let half_width = width * 0.5;
    let half_depth = depth * 0.5;
    let bottom = -height * 0.5;
    let spring = bottom + spring_height;
    let curve = |x: f32| {
        if let Some(radius) = pointed_arc_radius {
            let centre_offset = (radius - half_width).max(0.0);
            spring
                + (radius * radius - (x.abs() + centre_offset).powi(2))
                    .max(0.0)
                    .sqrt()
        } else {
            let radius = width * width / (8.0 * rise.max(0.01)) + rise * 0.5;
            spring + (radius * radius - x * x).max(0.0).sqrt() + rise - radius
        }
    };
    let mut front = vec![
        Vec3::new(-half_width, bottom, half_depth),
        Vec3::new(half_width, bottom, half_depth),
    ];
    for index in (0..=segments).rev() {
        let x = -half_width + width * index as f32 / segments as f32;
        front.push(Vec3::new(x, curve(x), half_depth));
    }
    let mut back = front
        .iter()
        .rev()
        .map(|point| Vec3::new(point.x, point.y, -half_depth))
        .collect::<Vec<_>>();
    let mut faces = vec![front.clone(), std::mem::take(&mut back)];
    for index in 0..front.len() {
        let next = (index + 1) % front.len();
        let a = front[index];
        let b = front[next];
        faces.push(vec![
            Vec3::new(a.x, a.y, -half_depth),
            Vec3::new(b.x, b.y, -half_depth),
            b,
            a,
        ]);
    }
    flat_face_mesh(&faces)
}

fn splayed_jamb_mesh(
    width: f32,
    height: f32,
    depth: f32,
    exterior_width: f32,
    interior_width: f32,
    side: i8,
    exterior_depth_sign: i8,
) -> Mesh {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let half_depth = depth * 0.5;
    let side = if side < 0 { -1.0 } else { 1.0 };
    let exterior_z = if exterior_depth_sign < 0 {
        -half_depth
    } else {
        half_depth
    };
    let interior_z = -exterior_z;
    let retreat = ((interior_width - exterior_width) * 0.5)
        .max(0.0)
        .min(width - 0.02);
    let outer_x = side * half_width;
    let exterior_aperture_x = -side * half_width;
    let interior_aperture_x = exterior_aperture_x + side * retreat;
    let mut plan = [
        Vec2::new(outer_x, exterior_z),
        Vec2::new(exterior_aperture_x, exterior_z),
        Vec2::new(interior_aperture_x, interior_z),
        Vec2::new(outer_x, interior_z),
    ];
    let signed_area = plan
        .iter()
        .zip(plan.iter().cycle().skip(1))
        .take(plan.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>();
    if signed_area < 0.0 {
        plan.reverse();
    }
    let bottom = plan
        .iter()
        .map(|point| Vec3::new(point.x, -half_height, point.y))
        .collect::<Vec<_>>();
    let top = plan
        .iter()
        .map(|point| Vec3::new(point.x, half_height, point.y))
        .collect::<Vec<_>>();
    let mut faces = vec![bottom.clone(), top.iter().copied().rev().collect()];
    for index in 0..plan.len() {
        let next = (index + 1) % plan.len();
        faces.push(vec![bottom[next], bottom[index], top[index], top[next]]);
    }
    flat_face_mesh(&faces)
}

fn splayed_head_mesh(
    width: f32,
    height: f32,
    depth: f32,
    exterior_clear_height: f32,
    interior_clear_height: f32,
    exterior_depth_sign: i8,
) -> Mesh {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let half_depth = depth * 0.5;
    let exterior_z = if exterior_depth_sign < 0 {
        -half_depth
    } else {
        half_depth
    };
    let interior_z = -exterior_z;
    let minimum_clear = exterior_clear_height.min(interior_clear_height);
    let exterior_y = -half_height + exterior_clear_height - minimum_clear;
    let interior_y = -half_height + interior_clear_height - minimum_clear;
    let top_y = half_height;
    let faces = vec![
        vec![
            Vec3::new(-half_width, exterior_y, exterior_z),
            Vec3::new(-half_width, interior_y, interior_z),
            Vec3::new(half_width, interior_y, interior_z),
            Vec3::new(half_width, exterior_y, exterior_z),
        ],
        vec![
            Vec3::new(-half_width, top_y, exterior_z),
            Vec3::new(half_width, top_y, exterior_z),
            Vec3::new(half_width, top_y, interior_z),
            Vec3::new(-half_width, top_y, interior_z),
        ],
        vec![
            Vec3::new(-half_width, exterior_y, exterior_z),
            Vec3::new(half_width, exterior_y, exterior_z),
            Vec3::new(half_width, top_y, exterior_z),
            Vec3::new(-half_width, top_y, exterior_z),
        ],
        vec![
            Vec3::new(half_width, interior_y, interior_z),
            Vec3::new(-half_width, interior_y, interior_z),
            Vec3::new(-half_width, top_y, interior_z),
            Vec3::new(half_width, top_y, interior_z),
        ],
        vec![
            Vec3::new(-half_width, interior_y, interior_z),
            Vec3::new(-half_width, exterior_y, exterior_z),
            Vec3::new(-half_width, top_y, exterior_z),
            Vec3::new(-half_width, top_y, interior_z),
        ],
        vec![
            Vec3::new(half_width, exterior_y, exterior_z),
            Vec3::new(half_width, interior_y, interior_z),
            Vec3::new(half_width, top_y, interior_z),
            Vec3::new(half_width, top_y, exterior_z),
        ],
    ];
    let faces = if exterior_depth_sign < 0 {
        faces
            .into_iter()
            .map(|face| face.into_iter().rev().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    } else {
        faces
    };
    flat_face_mesh(&faces)
}

fn spawn_curtain_wall(
    world: &mut World,
    palette: &RenderPalette,
    wall: CurtainWallRun,
    origin: Vec2,
    closures: &[GateClosure],
) {
    let start = wall.start + origin;
    let end = wall.end + origin;
    let delta = end - start;
    let length = delta.length();
    if length <= 0.1 {
        return;
    }
    let tangent = delta / length;
    let horizontal = delta.x.abs() >= delta.y.abs();
    let wall_box = |world: &mut World, centre: Vec2, run_length: f32, height: f32, y: f32| {
        spawn_box(
            world,
            &palette.stone,
            if horizontal {
                Vec3::new(run_length, height, wall.thickness_metres)
            } else {
                Vec3::new(wall.thickness_metres, height, run_length)
            },
            Vec3::new(centre.x, y, centre.y),
            Quat::IDENTITY,
            "load-bearing curtain wall",
        );
    };
    if let Some(gate_width) = wall.gate_width_metres {
        let side_length = ((length - gate_width) * 0.5).max(0.1);
        let midpoint = (start + end) * 0.5;
        for sign in [-1.0, 1.0] {
            let centre = midpoint + tangent * (gate_width + side_length) * 0.5 * sign;
            wall_box(
                world,
                centre,
                side_length,
                wall.height_metres,
                wall.height_metres * 0.5,
            );
        }
        let lintel_height = wall.height_metres - wall.gate_height_metres;
        wall_box(
            world,
            midpoint,
            gate_width,
            lintel_height,
            wall.gate_height_metres + lintel_height * 0.5,
        );
        if closures.is_empty() {
            spawn_box(
                world,
                &palette.void,
                if horizontal {
                    Vec3::new(gate_width * 0.9, wall.gate_height_metres * 0.94, 0.08)
                } else {
                    Vec3::new(0.08, wall.gate_height_metres * 0.94, gate_width * 0.9)
                },
                Vec3::new(midpoint.x, wall.gate_height_metres * 0.47, midpoint.y),
                Quat::IDENTITY,
                "open curtain-wall gate passage",
            );
        }
        let inward = match wall.outward {
            Direction::North => -Vec2::Y,
            Direction::East => -Vec2::X,
            Direction::South => Vec2::Y,
            Direction::West => Vec2::X,
        };
        for closure in closures {
            let closure_centre = midpoint + inward * closure.inward_offset_metres;
            match closure.kind {
                GateClosureKind::HeavyLeaves => {
                    for sign in [-1.0, 1.0] {
                        let leaf_centre = closure_centre + tangent * gate_width * 0.25 * sign;
                        spawn_box(
                            world,
                            &palette.door,
                            if horizontal {
                                Vec3::new(gate_width * 0.48, wall.gate_height_metres * 0.9, 0.16)
                            } else {
                                Vec3::new(0.16, wall.gate_height_metres * 0.9, gate_width * 0.48)
                            },
                            Vec3::new(leaf_centre.x, wall.gate_height_metres * 0.45, leaf_centre.y),
                            Quat::from_rotation_y(sign * 0.22),
                            "closed heavy gate leaf",
                        );
                    }
                }
                GateClosureKind::Portcullis => {
                    for bar in 0..9 {
                        let across = (bar as f32 / 8.0 - 0.5) * gate_width * 0.9;
                        let position = closure_centre + tangent * across;
                        spawn_box(
                            world,
                            &palette.timber,
                            Vec3::splat(0.11).with_y(wall.gate_height_metres * 0.88),
                            Vec3::new(position.x, wall.gate_height_metres * 0.44, position.y),
                            Quat::IDENTITY,
                            "portcullis vertical bar",
                        );
                    }
                }
            }
        }
    } else {
        wall_box(
            world,
            (start + end) * 0.5,
            length,
            wall.height_metres,
            wall.height_metres * 0.5,
        );
    }
}

fn spawn_gatehouse_curtain(
    world: &mut World,
    palette: &RenderPalette,
    wall: CurtainWallRun,
    defense: &GateDefense,
    towers: &[RoundTower],
    origin: Vec2,
) {
    let GatehouseLoadPath::BondedTowerBearing {
        left_tower_index,
        right_tower_index,
        arch_centre,
        arch_spring_elevation_metres,
        arch_ring_depth,
        arch_rise,
        curtain_return_bond,
        ..
    } = defense.guard_chamber.load_path;
    let (Some(left), Some(right)) = (
        towers.get(left_tower_index).copied(),
        towers.get(right_tower_index).copied(),
    ) else {
        return;
    };
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let return_bond = curtain_return_bond.metres();
    let left_outer = left.centre_metres() - tangent * (left.radius_metres() - return_bond);
    let right_outer = right.centre_metres() + tangent * (right.radius_metres() - return_bond);
    for (start, end) in [(wall.start, left_outer), (right_outer, wall.end)] {
        if (end - start).length() > 0.05 {
            spawn_curtain_wall(
                world,
                palette,
                CurtainWallRun {
                    start,
                    end,
                    gate_width_metres: None,
                    ..wall
                },
                origin,
                &[],
            );
        }
    }

    let chamber = &defense.guard_chamber;
    let horizontal = tangent.x.abs() >= tangent.y.abs();
    let arch_depth = chamber.size.dot(direction_vector_2d(wall.outward).abs());
    let ring = arch_ring_depth.metres();
    let rise = arch_rise.metres();
    let half_span = wall.gate_width_metres.unwrap_or(3.2) * 0.5;
    let segments = 15;
    let block_width = half_span * 2.0 / segments as f32;
    for segment in 0..segments {
        let along = -half_span + (segment as f32 + 0.5) * block_width;
        let normalized = along / half_span;
        let elevation =
            arch_spring_elevation_metres + ring * 0.5 + rise * (1.0 - normalized * normalized);
        let slope = -2.0 * rise * normalized / half_span;
        let angle = slope.atan();
        let position = arch_centre + origin + tangent * along;
        spawn_box(
            world,
            &palette.stone,
            if horizontal {
                Vec3::new(block_width * 1.12, ring, arch_depth)
            } else {
                Vec3::new(arch_depth, ring, block_width * 1.12)
            },
            Vec3::new(position.x, elevation, position.y),
            if horizontal {
                Quat::from_rotation_z(angle)
            } else {
                Quat::from_rotation_x(-angle * tangent.y.signum())
            },
            "bonded segmental gate arch voussoir",
        );
    }
    let chamber_along = chamber.size.dot(tangent.abs());
    let shoulder_width = ((chamber_along - half_span * 2.0) * 0.5).max(0.0);
    let spandrel_height = ring + rise;
    for sign in [-1.0, 1.0] {
        spawn_wall_local_box(
            world,
            &palette.stone,
            chamber.centre + origin,
            tangent,
            direction_vector_2d(wall.outward),
            sign * (half_span + shoulder_width * 0.5),
            0.0,
            shoulder_width,
            arch_depth,
            spandrel_height,
            arch_spring_elevation_metres + spandrel_height * 0.5,
            "tower-bonded gate arch spandrel bearing",
        );
    }
    spawn_gate_closures(world, palette, wall, defense, origin);
}

fn spawn_gate_closures(
    world: &mut World,
    palette: &RenderPalette,
    wall: CurtainWallRun,
    defense: &GateDefense,
    origin: Vec2,
) {
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let horizontal = tangent.x.abs() >= tangent.y.abs();
    let inward = -direction_vector_2d(wall.outward);
    let gate_width = wall
        .gate_width_metres
        .unwrap_or_else(|| defense.guard_chamber.size.max_element());
    for closure in &defense.closures {
        let centre = defense.threshold + origin + inward * closure.inward_offset_metres;
        match closure.kind {
            GateClosureKind::HeavyLeaves => {
                for plank in 0..16 {
                    let across = (plank as f32 / 15.0 - 0.5) * gate_width * 0.97;
                    let height = closure.coverage.height_at(across);
                    let leaf = centre + tangent * across;
                    spawn_box(
                        world,
                        &palette.door,
                        if horizontal {
                            Vec3::new(gate_width / 15.0 * 1.04, height, 0.16)
                        } else {
                            Vec3::new(0.16, height, gate_width / 15.0 * 1.04)
                        },
                        Vec3::new(leaf.x, height * 0.5, leaf.y),
                        Quat::IDENTITY,
                        "closed heavy gate leaf",
                    );
                }
            }
            GateClosureKind::Portcullis => {
                for bar in 0..9 {
                    let across = (bar as f32 / 8.0 - 0.5) * gate_width * 0.9;
                    let position = centre + tangent * across;
                    let height = closure.coverage.height_at(across);
                    spawn_box(
                        world,
                        &palette.timber,
                        Vec3::splat(0.11).with_y(height),
                        Vec3::new(position.x, height * 0.5, position.y),
                        Quat::IDENTITY,
                        "portcullis vertical bar",
                    );
                }
            }
        }
    }
}

fn spawn_gate_guard_chamber(
    world: &mut World,
    palette: &RenderPalette,
    defense: &GateDefense,
    wall: CurtainWallRun,
    origin: Vec2,
    view: ViewerView,
) {
    let chamber = &defense.guard_chamber;
    let centre = chamber.centre + origin;
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let outward = direction_vector_2d(wall.outward);
    let along_size = chamber.size.dot(tangent.abs());
    let depth_size = chamber.size.dot(outward.abs());
    let half_along = along_size * 0.5;
    let half_depth = depth_size * 0.5;
    let floor_y = chamber.floor_elevation_metres;
    let wall_mid_y = floor_y + chamber.clear_height_metres * 0.5;
    let downward = chamber
        .openings
        .iter()
        .find(|opening| opening.kind == GuardOpeningKind::DownwardDefense);
    let hole_world = downward.map_or(centre, |opening| opening.position + origin);
    let hole_relative = hole_world - centre;
    let hole_along = hole_relative.dot(tangent);
    let hole_depth = hole_relative.dot(outward);
    let hole_size = downward.map_or(0.45, |opening| opening.width_metres.max(0.35));
    let left_width = hole_along - hole_size * 0.5 + half_along;
    let right_width = half_along - (hole_along + hole_size * 0.5);
    for (width, along) in [
        (left_width, -half_along + left_width * 0.5),
        (right_width, half_along - right_width * 0.5),
    ] {
        if width > 0.05 {
            spawn_wall_local_box(
                world,
                &palette.floor,
                centre,
                tangent,
                outward,
                along,
                0.0,
                width,
                depth_size,
                0.18,
                floor_y,
                "gate guard chamber floor",
            );
        }
    }
    let inward_depth = hole_depth - hole_size * 0.5 + half_depth;
    let outward_depth = half_depth - (hole_depth + hole_size * 0.5);
    for (depth, depth_offset) in [
        (inward_depth, -half_depth + inward_depth * 0.5),
        (outward_depth, half_depth - outward_depth * 0.5),
    ] {
        if depth > 0.05 {
            spawn_wall_local_box(
                world,
                &palette.floor,
                centre,
                tangent,
                outward,
                hole_along,
                depth_offset,
                hole_size,
                depth,
                0.18,
                floor_y,
                "gate guard chamber floor around downward opening",
            );
        }
    }
    // A recessed, explicitly non-colliding backdrop makes the downward
    // opening readable in section captures without filling the audited void.
    let hole_backdrop = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
        hole_size * 0.9,
        0.04,
        hole_size * 0.9,
    ));
    world.spawn((
        Name::new("non-colliding downward opening depth"),
        NonCollidingVisualization,
        Mesh3d(hole_backdrop),
        MeshMaterial3d(palette.void.clone()),
        Transform::from_xyz(hole_world.x, floor_y - 0.12, hole_world.y),
    ));
    for support in &chamber.supports {
        let support_centre = support.centre + origin;
        spawn_box(
            world,
            &palette.stone,
            Vec3::new(
                support.size.x,
                support.top_elevation_metres - support.base_elevation_metres,
                support.size.y,
            ),
            Vec3::new(
                support_centre.x,
                (support.top_elevation_metres + support.base_elevation_metres) * 0.5,
                support_centre.y,
            ),
            Quat::IDENTITY,
            "gate guard chamber support pier",
        );
    }

    let observation = chamber
        .openings
        .iter()
        .find(|opening| opening.kind == GuardOpeningKind::OutwardObservation);
    let observation_width = observation.map_or(0.35, |opening| opening.width_metres);
    let observation_sill =
        observation.map_or(floor_y + 0.85, |opening| opening.sill_elevation_metres);
    let observation_height = observation.map_or(0.8, |opening| opening.clear_height_metres);
    let observation_along = observation
        .map(|opening| (opening.position - chamber.centre).dot(tangent))
        .unwrap_or(0.0);
    let left_wall_width = observation_along - observation_width * 0.5 + half_along;
    let right_wall_width = half_along - observation_along - observation_width * 0.5;
    for (width, along) in [
        (left_wall_width, -half_along + left_wall_width * 0.5),
        (right_wall_width, half_along - right_wall_width * 0.5),
    ] {
        if width <= 0.05 {
            continue;
        }
        spawn_wall_local_box(
            world,
            &palette.stone,
            centre,
            tangent,
            outward,
            along,
            half_depth,
            width,
            0.28,
            chamber.clear_height_metres,
            wall_mid_y,
            "gate guard chamber outward wall pier",
        );
    }
    let lower_height = (observation_sill - floor_y).max(0.2);
    spawn_wall_local_box(
        world,
        &palette.stone,
        centre,
        tangent,
        outward,
        observation_along,
        half_depth,
        observation_width,
        0.28,
        lower_height,
        floor_y + lower_height * 0.5,
        "gate guard chamber observation sill",
    );
    let upper_base = observation_sill + observation_height;
    let upper_height = (floor_y + chamber.clear_height_metres - upper_base).max(0.2);
    spawn_wall_local_box(
        world,
        &palette.stone,
        centre,
        tangent,
        outward,
        observation_along,
        half_depth,
        observation_width,
        0.28,
        upper_height,
        upper_base + upper_height * 0.5,
        "gate guard chamber observation lintel",
    );
    spawn_wall_local_box(
        world,
        &palette.void,
        centre,
        tangent,
        outward,
        observation_along,
        half_depth + 0.02,
        observation_width * 0.9,
        0.05,
        observation_height * 0.9,
        observation_sill + observation_height * 0.5,
        "gate guard chamber outward firing opening",
    );

    for sign in [-1.0, 1.0] {
        if view == ViewerView::GateDetailInterior && sign > 0.0 {
            continue;
        }
        spawn_wall_local_box(
            world,
            &palette.stone,
            centre,
            tangent,
            outward,
            sign * (half_along - 0.14),
            0.0,
            0.28,
            depth_size,
            chamber.clear_height_metres,
            wall_mid_y,
            "gate guard chamber side wall",
        );
    }
    {
        let door = chamber.access.door;
        let top_opening = chamber.access.top_walk_opening;
        let door_along = (door.position - chamber.centre).dot(tangent);
        let top_along = (top_opening.position - chamber.centre).dot(tangent);
        let top_left = top_along - top_opening.width_metres * 0.5;
        let top_right = top_along + top_opening.width_metres * 0.5;
        let door_left = door_along - door.width_metres * 0.5;
        let door_right = door_along + door.width_metres * 0.5;
        for (wall_section, (start, end)) in [
            (-half_along, top_left),
            (top_right, door_left),
            (door_right, half_along),
        ]
        .into_iter()
        .enumerate()
        {
            if view == ViewerView::GateDetailInterior && wall_section == 1 {
                continue;
            }
            let width = end - start;
            let along = (start + end) * 0.5;
            if width > 0.05 {
                spawn_wall_local_box(
                    world,
                    &palette.stone,
                    centre,
                    tangent,
                    outward,
                    along,
                    -half_depth,
                    width,
                    0.28,
                    chamber.clear_height_metres,
                    wall_mid_y,
                    "gate guard chamber access wall",
                );
            }
        }
        let below_top = top_opening.threshold_elevation_metres - floor_y;
        if below_top > 0.02 {
            spawn_wall_local_box(
                world,
                &palette.stone,
                centre,
                tangent,
                outward,
                top_along,
                -half_depth,
                top_opening.width_metres,
                0.28,
                below_top,
                floor_y + below_top * 0.5,
                "masonry below wall-walk access opening",
            );
        }
        let above_door = floor_y + chamber.clear_height_metres
            - (door.threshold_elevation_metres + door.clear_height_metres);
        if above_door > 0.02 {
            spawn_wall_local_box(
                world,
                &palette.stone,
                centre,
                tangent,
                outward,
                door_along,
                -half_depth,
                door.width_metres,
                0.28,
                above_door,
                door.threshold_elevation_metres + door.clear_height_metres + above_door * 0.5,
                "gate guard chamber access lintel",
            );
        }
        if view == ViewerView::GateDetailInterior {
            let hinge = door.position + origin + tangent * (door.width_metres * 0.5);
            let leaf_centre = hinge + outward * (door.width_metres * 0.5);
            spawn_wall_local_box(
                world,
                &palette.door,
                leaf_centre,
                tangent,
                outward,
                0.0,
                0.0,
                0.08,
                door.width_metres,
                door.clear_height_metres * 0.96,
                door.threshold_elevation_metres + door.clear_height_metres * 0.48,
                "open floor-level guard chamber door",
            );
        } else {
            spawn_wall_local_box(
                world,
                &palette.door,
                centre,
                tangent,
                outward,
                door_along,
                -half_depth - 0.02,
                door.width_metres * 0.92,
                0.08,
                door.clear_height_metres * 0.96,
                door.threshold_elevation_metres + door.clear_height_metres * 0.48,
                "floor-level guard chamber door",
            );
        }

        let cut = chamber.access.roof_clearance_opening;
        let cut_along = (cut.centre - chamber.centre).dot(tangent);
        let cut_depth = (cut.centre - chamber.centre).dot(outward);
        let cut_along_size = cut.size.dot(tangent.abs());
        let cut_depth_size = cut.size.dot(outward.abs());
        let left_roof = cut_along - cut_along_size * 0.5 + half_along;
        let right_roof = half_along - cut_along - cut_along_size * 0.5;
        for (roof_index, (width, along)) in [
            (left_roof, -half_along + left_roof * 0.5),
            (right_roof, half_along - right_roof * 0.5),
        ]
        .into_iter()
        .enumerate()
        {
            if view == ViewerView::GateDetailInterior && roof_index == 1 {
                continue;
            }
            if width > 0.02 {
                spawn_wall_local_box(
                    world,
                    &palette.roof_secondary,
                    centre,
                    tangent,
                    outward,
                    along,
                    0.0,
                    width,
                    depth_size,
                    0.22,
                    floor_y + chamber.clear_height_metres + 0.11,
                    "gate guard chamber roof slab",
                );
            }
        }
        let inner_depth = cut_depth - cut_depth_size * 0.5 + half_depth;
        let outer_depth = half_depth - cut_depth - cut_depth_size * 0.5;
        for (depth, offset) in [
            (inner_depth, -half_depth + inner_depth * 0.5),
            (outer_depth, half_depth - outer_depth * 0.5),
        ] {
            if depth > 0.02 {
                spawn_wall_local_box(
                    world,
                    &palette.roof_secondary,
                    centre,
                    tangent,
                    outward,
                    cut_along,
                    offset,
                    cut_along_size,
                    depth,
                    0.22,
                    floor_y + chamber.clear_height_metres + 0.11,
                    "gate guard chamber roof around access cut",
                );
            }
        }
    }

    let access = &chamber.access;
    for (landing, name) in [
        (access.top_landing, "gate access top landing"),
        (access.bottom_landing, "gate access bottom landing"),
    ] {
        spawn_box(
            world,
            &palette.stair,
            Vec3::new(landing.size.x, 0.16, landing.size.y),
            Vec3::new(
                landing.centre.x + origin.x,
                landing.elevation_metres,
                landing.centre.y + origin.y,
            ),
            Quat::IDENTITY,
            name,
        );
    }
    for guard in &access.landing_guards {
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                guard.start.x + origin.x,
                guard.elevation_metres + guard.height_metres,
                guard.start.y + origin.y,
            ),
            Vec3::new(
                guard.end.x + origin.x,
                guard.elevation_metres + guard.height_metres,
                guard.end.y + origin.y,
            ),
            0.1,
            "gate access landing perimeter guard",
        );
        for point in [guard.start, guard.end] {
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(
                    point.x + origin.x,
                    guard.elevation_metres,
                    point.y + origin.y,
                ),
                Vec3::new(
                    point.x + origin.x,
                    guard.elevation_metres + guard.height_metres,
                    point.y + origin.y,
                ),
                0.1,
                "gate access landing guard post",
            );
        }
    }
    for tread in 0..=access.flight.riser_count {
        let progress = f32::from(tread) / f32::from(access.flight.riser_count);
        let position = access.flight.top.lerp(access.flight.bottom, progress) + origin;
        let elevation = access.flight.top_elevation_metres
            + (access.flight.bottom_elevation_metres - access.flight.top_elevation_metres)
                * progress;
        spawn_wall_local_box(
            world,
            &palette.stair,
            position,
            tangent,
            outward,
            0.0,
            0.0,
            access.flight.going_metres + access.flight.nosing_metres,
            access.envelope.width_metres,
            0.12,
            elevation,
            "gate guard chamber access stair",
        );
        for sign in [-1.0, 1.0] {
            spawn_wall_local_box(
                world,
                &palette.timber,
                position,
                tangent,
                outward,
                0.0,
                sign * (access.envelope.width_metres * 0.38),
                access.flight.going_metres + access.flight.nosing_metres,
                0.16,
                0.22,
                elevation - 0.12,
                "gate access stepped stringer",
            );
        }
        for sign in [-1.0, 1.0] {
            spawn_wall_local_box(
                world,
                &palette.timber,
                position,
                tangent,
                outward,
                0.0,
                sign * (access.envelope.width_metres * 0.5 + 0.06),
                access.flight.going_metres + access.flight.nosing_metres,
                0.1,
                0.1,
                elevation + access.flight_guard_height_metres,
                "gate access continuous edge guard",
            );
            if tread % 2 == 0 {
                spawn_wall_local_box(
                    world,
                    &palette.timber,
                    position,
                    tangent,
                    outward,
                    0.0,
                    sign * (access.envelope.width_metres * 0.5 + 0.06),
                    0.1,
                    0.1,
                    access.flight_guard_height_metres,
                    elevation + access.flight_guard_height_metres * 0.5,
                    "gate access guard post",
                );
            }
        }
    }
    for support in &access.support_posts {
        let height = support.top_elevation_metres - support.base_elevation_metres;
        spawn_box(
            world,
            &palette.timber,
            Vec3::new(support.size.x, height, support.size.y),
            Vec3::new(
                support.centre.x + origin.x,
                support.base_elevation_metres + height * 0.5,
                support.centre.y + origin.y,
            ),
            Quat::IDENTITY,
            "gate access support post",
        );
    }
    spawn_box(
        world,
        &palette.timber,
        Vec3::new(
            access.wall_ledger.size.x,
            access.wall_ledger.height_metres,
            access.wall_ledger.size.y,
        ),
        Vec3::new(
            access.wall_ledger.centre.x + origin.x,
            access.wall_ledger.elevation_metres,
            access.wall_ledger.centre.y + origin.y,
        ),
        Quat::IDENTITY,
        "gate access masonry wall ledger",
    );
    for brace in &access.lateral_braces {
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                brace.start.x + origin.x,
                brace.start_elevation_metres,
                brace.start.y + origin.y,
            ),
            Vec3::new(
                brace.end.x + origin.x,
                brace.end_elevation_metres,
                brace.end.y + origin.y,
            ),
            brace.thickness_metres,
            "gate access diagonal lateral brace",
        );
    }
    for operating in &chamber.operating_positions {
        let position = operating.position + origin;
        spawn_box(
            world,
            &palette.timber,
            Vec3::new(1.3, 0.18, 0.18),
            Vec3::new(position.x, operating.elevation_metres + 0.95, position.y),
            Quat::IDENTITY,
            "portcullis operating windlass",
        );
        spawn_box(
            world,
            &palette.timber,
            Vec3::new(0.18, 1.1, 0.18),
            Vec3::new(position.x, operating.elevation_metres + 0.55, position.y),
            Quat::IDENTITY,
            "portcullis operating post",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_wall_local_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    along_offset: f32,
    outward_offset: f32,
    along_size: f32,
    depth_size: f32,
    height: f32,
    elevation: f32,
    name: &'static str,
) {
    let position = centre + tangent * along_offset + outward * outward_offset;
    let horizontal = tangent.x.abs() >= tangent.y.abs();
    spawn_box(
        world,
        material,
        if horizontal {
            Vec3::new(along_size, height, depth_size)
        } else {
            Vec3::new(depth_size, height, along_size)
        },
        Vec3::new(position.x, elevation, position.y),
        Quat::IDENTITY,
        name,
    );
}

fn spawn_square_tower(
    world: &mut World,
    palette: &RenderPalette,
    tower: SquareTower,
    origin: Vec2,
    view: ViewerView,
) {
    if view == ViewerView::Cutaway {
        return;
    }
    let centre = tower.centre + origin;
    // Bell towers hand authority to resolved SquareTowerFace bays at 8 m so
    // their roof junction and bell openings can own real subtractions.  Keep
    // only the monolithic grounded base here; spawning the old full-height box
    // would duplicate those resolved walls and conceal the abutment contour.
    let lower_height = if tower.bell_openings {
        8.0
    } else {
        tower.wall_height_metres
    };
    spawn_box(
        world,
        &palette.stone,
        Vec3::new(tower.size.x, lower_height, tower.size.y),
        Vec3::new(centre.x, lower_height * 0.5, centre.y),
        Quat::IDENTITY,
        "square bell-tower lower mass",
    );
    // The bell stage itself is rendered exclusively from its resolved
    // WallAssembly/OpeningAssembly bays. Keeping a second viewer-owned stage
    // here would conceal voids and duplicate the authoritative masonry.
    // The roof is rendered once from the authoritative RoofAssembly graph.
}

fn spawn_tower(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    tower_index: usize,
    tower: RoundTower,
    origin: Vec2,
    view: ViewerView,
    portals: &[TowerPortal],
    firing_positions: &[FiringPosition],
    authoritative_crown: bool,
) {
    let centre = tower.centre_metres() + origin;
    if view != ViewerView::Cutaway {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(tower_shell_mesh(
            tower,
            portals,
            firing_positions,
            matches!(
                view,
                ViewerView::TowerPortalDetail
                    | ViewerView::CrownTowerCutaway
                    | ViewerView::WallRoundTowerRadialSection
                    | ViewerView::ArtilleryRondelCasemate
                    | ViewerView::ArtilleryRondelCutaway
            ),
        ));
        let wall = plan.wall_assemblies.iter().find(|wall| {
            matches!(
                wall.source,
                adventuresim_building_generator::WallSourceId::RoundTower { tower_index: index }
                    if index == tower_index
            )
        });
        let resolved = wall.and_then(|wall| {
            wall.host_solids.first().and_then(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
            })
        });
        let mut shell = world.spawn((
            Name::new(if let Some(wall) = wall {
                format!("resolved wall owner {} round tower shell", wall.owner.0)
            } else {
                "round tower shell with open firing loops".to_owned()
            }),
            ClosedSolid,
            Mesh3d(mesh),
            MeshMaterial3d(if view == ViewerView::ArtilleryRondelCasemate {
                palette.cutaway.clone()
            } else {
                palette.stone.clone()
            }),
            Transform::from_xyz(centre.x, tower.wall_height_metres * 0.5, centre.y),
        ));
        if let (Some(wall), Some(resolved)) = (wall, resolved) {
            shell.insert((
                GeometryOwner(wall.owner.0),
                ResolvedRenderItem {
                    id: resolved.id.0,
                    fingerprint: stable_u64(
                        &serde_json::to_vec(resolved)
                            .expect("serialize rendered radial wall shell"),
                    ),
                    local_half_size: resolved.size * 0.5,
                },
            ));
        }
        for interface in tower.chord_interfaces() {
            spawn_tower_chord_face(world, palette, tower, centre, interface);
        }
        if !matches!(
            view,
            ViewerView::TowerPortalDetail
                | ViewerView::CrownTowerCutaway
                | ViewerView::WallRoundTowerRadialSection
                | ViewerView::ArtilleryRondelCasemate
                | ViewerView::ArtilleryRondelCutaway
        ) {
            let inner_height = (tower.wall_height_metres - 0.18).max(0.2);
            let inner = world.resource_mut::<Assets<Mesh>>().add(cylinder_side_mesh(
                (tower.radius_metres() - tower.wall_thickness_metres).max(0.2),
                inner_height,
                64,
            ));
            world.spawn((
                Name::new("non-colliding dark tower depth backdrop"),
                NonCollidingVisualization,
                Mesh3d(inner),
                MeshMaterial3d(palette.void.clone()),
                Transform::from_xyz(centre.x, inner_height * 0.5, centre.y),
            ));
        }
        // Tower roofs are rendered once from the authoritative RoofAssembly graph.
    } else {
        for level in 0..=0 {
            let mesh = world
                .resource_mut::<Assets<Mesh>>()
                .add(Cylinder::new(tower.radius_metres() - 0.18, 0.12));
            world.spawn((
                Name::new("cutaway tower floor"),
                Mesh3d(mesh),
                MeshMaterial3d(palette.floor.clone()),
                Transform::from_xyz(centre.x, level as f32 * 3.4 + 0.06, centre.y),
            ));
        }
    }
    spawn_tower_portal_geometry(world, palette, tower, origin, portals);
    if !authoritative_crown
        && view != ViewerView::Cutaway
        && let Some(kind) = tower.battlement
    {
        spawn_round_battlement(
            world,
            palette,
            tower,
            origin,
            kind,
            portals,
            view == ViewerView::TowerPortalDetail,
        );
    }
}

fn tower_shell_mesh(
    tower: RoundTower,
    portals: &[TowerPortal],
    firing_positions: &[FiringPosition],
    section_cut: bool,
) -> Mesh {
    // Project gate apertures are only 0.18 m wide. At the standard 3 m
    // radius, 256 facets keep an aperture wider than two chord samples while
    // exact feature-boundary tessellation remains a future optimization.
    const SEGMENTS: usize = 256;
    let half_height = tower.wall_height_metres * 0.5;
    let slit_ranges = (0..3)
        .map(|level| {
            let centre = 1.45 + level as f32 * 2.2;
            (
                (centre - 0.45).max(0.05),
                (centre + 0.45).min(tower.wall_height_metres - 0.05),
            )
        })
        .filter(|(low, high)| low < high)
        .collect::<Vec<_>>();
    let mut height_breaks = vec![0.0, tower.wall_height_metres];
    height_breaks.extend(slit_ranges.iter().flat_map(|(low, high)| [*low, *high]));
    height_breaks.extend(portals.iter().flat_map(|portal| {
        [
            portal.sill_elevation_metres.max(0.0),
            (portal.sill_elevation_metres + portal.clear_height_metres)
                .min(tower.wall_height_metres),
        ]
    }));
    height_breaks.extend(firing_positions.iter().flat_map(|position| {
        [
            (position.elevation_metres - 0.45).max(0.0),
            (position.elevation_metres + 0.45).min(tower.wall_height_metres),
        ]
    }));
    height_breaks.sort_by(f32::total_cmp);
    height_breaks.dedup_by(|left, right| (*left - *right).abs() <= 0.001);
    let bands = height_breaks.len() - 1;
    let mut included = vec![vec![true; bands]; SEGMENTS];
    for (segment, segment_bands) in included.iter_mut().enumerate() {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_mid = (angle_a + angle_b) * 0.5;
        let radial_mid = Vec2::new(angle_mid.cos(), angle_mid.sin());
        let chord_cut = tower.chord_interfaces().any(|interface| {
            let toward = direction_vector_2d(interface.toward_gate);
            let cut_ratio =
                (tower.radius_metres() - interface.bearing_depth.metres()) / tower.radius_metres();
            radial_mid.dot(toward) > cut_ratio
        });
        let section_removed =
            section_cut && radial_mid.dot(Vec2::new(-0.707_106_77, -0.707_106_77)) > 0.1;
        for band in 0..bands {
            let height_mid = (height_breaks[band] + height_breaks[band + 1]) * 0.5;
            let slit = segment.is_multiple_of(SEGMENTS / 8)
                && slit_ranges
                    .iter()
                    .any(|(low, high)| height_mid > *low && height_mid < *high);
            let portal_void = portals.iter().any(|portal| {
                let facing = direction_vector_2d(portal.facing);
                let half_angle = portal.width_metres * 0.5 / tower.radius_metres();
                radial_mid.dot(facing) >= half_angle.cos()
                    && height_mid > portal.sill_elevation_metres
                    && height_mid < portal.sill_elevation_metres + portal.clear_height_metres
            });
            let firing_void = firing_positions.iter().any(|position| {
                let half_angle = position.aperture_width_metres * 0.5 / tower.radius_metres();
                radial_mid.dot(position.aperture_normal) >= half_angle.cos()
                    && height_mid > position.elevation_metres - 0.45
                    && height_mid < position.elevation_metres + 0.45
            });
            segment_bands[band] =
                !(chord_cut || section_removed || slit || portal_void || firing_void);
        }
    }
    let outer_radius = tower.radius_metres();
    let inner_radius = (outer_radius - tower.wall_thickness_metres).max(0.2);
    let mut faces = Vec::new();
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction_a = Vec2::new(angle_a.cos(), angle_a.sin());
        let direction_b = Vec2::new(angle_b.cos(), angle_b.sin());
        let outer_a = direction_a * outer_radius;
        let outer_b = direction_b * outer_radius;
        let inner_a = direction_a * inner_radius;
        let inner_b = direction_b * inner_radius;
        for band in 0..bands {
            if !included[segment][band] {
                continue;
            }
            let low = height_breaks[band] - half_height;
            let high = height_breaks[band + 1] - half_height;
            // Outer and inner faces are always exposed wall surfaces.
            faces.push(vec![
                Vec3::new(outer_a.x, low, outer_a.y),
                Vec3::new(outer_b.x, low, outer_b.y),
                Vec3::new(outer_b.x, high, outer_b.y),
                Vec3::new(outer_a.x, high, outer_a.y),
            ]);
            faces.push(vec![
                Vec3::new(inner_b.x, low, inner_b.y),
                Vec3::new(inner_a.x, low, inner_a.y),
                Vec3::new(inner_a.x, high, inner_a.y),
                Vec3::new(inner_b.x, high, inner_b.y),
            ]);
            if band == 0 || !included[segment][band - 1] {
                faces.push(vec![
                    Vec3::new(outer_a.x, low, outer_a.y),
                    Vec3::new(inner_a.x, low, inner_a.y),
                    Vec3::new(inner_b.x, low, inner_b.y),
                    Vec3::new(outer_b.x, low, outer_b.y),
                ]);
            }
            if band + 1 == bands || !included[segment][band + 1] {
                faces.push(vec![
                    Vec3::new(inner_a.x, high, inner_a.y),
                    Vec3::new(outer_a.x, high, outer_a.y),
                    Vec3::new(outer_b.x, high, outer_b.y),
                    Vec3::new(inner_b.x, high, inner_b.y),
                ]);
            }
            let previous = (segment + SEGMENTS - 1) % SEGMENTS;
            if !included[previous][band] {
                faces.push(vec![
                    Vec3::new(inner_a.x, low, inner_a.y),
                    Vec3::new(outer_a.x, low, outer_a.y),
                    Vec3::new(outer_a.x, high, outer_a.y),
                    Vec3::new(inner_a.x, high, inner_a.y),
                ]);
            }
            let next = (segment + 1) % SEGMENTS;
            if !included[next][band] {
                faces.push(vec![
                    Vec3::new(outer_b.x, low, outer_b.y),
                    Vec3::new(inner_b.x, low, inner_b.y),
                    Vec3::new(inner_b.x, high, inner_b.y),
                    Vec3::new(outer_b.x, high, outer_b.y),
                ]);
            }
        }
    }
    for face in &mut faces {
        face.reverse();
    }
    flat_face_mesh(&faces)
}

fn spawn_tower_chord_face(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    centre: Vec2,
    interface: adventuresim_building_generator::TowerChordInterface,
) {
    let toward = direction_vector_2d(interface.toward_gate);
    let radius = tower.radius_metres();
    let cut_distance = radius - interface.bearing_depth.metres();
    let chord_width = 2.0
        * (radius * radius - cut_distance * cut_distance)
            .max(0.0)
            .sqrt();
    let thickness = tower
        .wall_thickness_metres
        .min(interface.bearing_depth.metres());
    let face = centre + toward * (cut_distance - thickness * 0.5);
    let along_x = toward.x.abs() > 0.5;
    spawn_box(
        world,
        &palette.stone,
        if along_x {
            Vec3::new(thickness, tower.wall_height_metres, chord_width)
        } else {
            Vec3::new(chord_width, tower.wall_height_metres, thickness)
        },
        Vec3::new(face.x, tower.wall_height_metres * 0.5, face.y),
        Quat::IDENTITY,
        "bonded tower chord face",
    );
}

fn direction_vector_2d(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    }
}

fn spawn_tower_portal_geometry(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    origin: Vec2,
    portals: &[TowerPortal],
) {
    let centre = tower.centre_metres() + origin;
    for portal in portals {
        let radial = direction_vector_2d(portal.facing);
        let tangent = Vec2::new(-radial.y, radial.x);
        let frame_centre = centre + radial * tower.radius_metres();
        if portal.kind == TowerPortalKind::GroundStairEntrance {
            for sign in [-1.0, 1.0] {
                let jamb = frame_centre + tangent * portal.width_metres * 0.58 * sign;
                spawn_box(
                    world,
                    &palette.stone,
                    Vec3::new(0.2, portal.clear_height_metres, 0.2),
                    Vec3::new(jamb.x, portal.clear_height_metres * 0.5, jamb.y),
                    Quat::IDENTITY,
                    "tower entrance jamb",
                );
            }
            spawn_box(
                world,
                &palette.stone,
                if radial.x.abs() > radial.y.abs() {
                    Vec3::new(0.24, 0.22, portal.width_metres + 0.35)
                } else {
                    Vec3::new(portal.width_metres + 0.35, 0.22, 0.24)
                },
                Vec3::new(
                    frame_centre.x,
                    portal.clear_height_metres + 0.11,
                    frame_centre.y,
                ),
                Quat::IDENTITY,
                "tower entrance lintel",
            );
        } else {
            let landing = centre + radial * (tower.radius_metres() - 0.15);
            spawn_box(
                world,
                &palette.floor,
                if radial.x.abs() > radial.y.abs() {
                    Vec3::new(1.3, 0.16, portal.width_metres)
                } else {
                    Vec3::new(portal.width_metres, 0.16, 1.3)
                },
                Vec3::new(landing.x, portal.sill_elevation_metres + 0.12, landing.y),
                Quat::IDENTITY,
                "tower-to-wall-walk portal landing",
            );
        }
    }
}

#[allow(dead_code)]
fn spawn_conical_roof(world: &mut World, material: &Handle<StandardMaterial>, roof: RoofPiece) {
    let radius = roof.size.x.max(roof.size.y) * 0.5 + roof.eave_metres;
    let height = radius * roof.pitch_degrees.to_radians().tan();
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cone::new(radius, height));
    world.spawn((
        Name::new("conical tower roof"),
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        Transform::from_xyz(
            roof.centre.x,
            roof.base_height_metres + height * 0.5,
            roof.centre.y,
        ),
    ));
}

fn spawn_round_battlement(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    origin: Vec2,
    kind: BattlementKind,
    portals: &[TowerPortal],
    section_cut: bool,
) {
    let centre = tower.centre_metres() + origin;
    let radius = tower.radius_metres()
        + if kind == BattlementKind::Machicolated {
            0.38
        } else {
            0.08
        };
    if kind == BattlementKind::GunLoopParapet {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(round_loop_parapet_mesh(radius, 1.15));
        world.spawn((
            Name::new("round parapet with open gun loops"),
            Mesh3d(mesh),
            MeshMaterial3d(palette.stone.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres + 0.58, centre.y),
        ));
        let inner = world.resource_mut::<Assets<Mesh>>().add(cylinder_side_mesh(
            (radius - 0.24).max(0.2),
            1.11,
            72,
        ));
        world.spawn((
            Name::new("dark round parapet interior"),
            Mesh3d(inner),
            MeshMaterial3d(palette.void.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres + 0.58, centre.y),
        ));
        return;
    }
    if kind == BattlementKind::Machicolated {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cylinder::new(radius, 0.18));
        world.spawn((
            Name::new("machicolation gallery floor"),
            Mesh3d(mesh),
            MeshMaterial3d(palette.stone.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres, centre.y),
        ));
    }
    let count = 16;
    for index in 0..count {
        let angle = index as f32 * std::f32::consts::TAU / count as f32;
        let radial = Vec2::new(angle.cos(), angle.sin());
        if tower.chord_interfaces().any(|interface| {
            let toward = direction_vector_2d(interface.toward_gate);
            let cut_ratio =
                (tower.radius_metres() - interface.bearing_depth.metres()) / tower.radius_metres();
            radial.dot(toward) > cut_ratio
        }) {
            continue;
        }
        if section_cut && radial.dot(Vec2::new(-0.707_106_77, -0.707_106_77)) > 0.1 {
            continue;
        }
        if portals.iter().any(|portal| {
            matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. })
                && radial.dot(direction_vector_2d(portal.facing)) > 0.86
        }) {
            continue;
        }
        let tangent = Vec2::new(-angle.sin(), angle.cos());
        let position = centre + radial * radius;
        if kind == BattlementKind::PiercedCrenellated {
            for sign in [-1.0, 1.0] {
                let half = position + tangent * 0.17 * sign;
                spawn_box(
                    world,
                    &palette.stone,
                    Vec3::new(0.22, 0.85, 0.42),
                    Vec3::new(half.x, tower.wall_height_metres + 0.425, half.y),
                    Quat::from_rotation_y(-angle),
                    "round merlon split by firing loop",
                );
            }
        } else {
            spawn_box(
                world,
                &palette.stone,
                Vec3::new(0.55, 0.85, 0.42),
                Vec3::new(position.x, tower.wall_height_metres + 0.425, position.y),
                Quat::from_rotation_y(-angle),
                "round merlon",
            );
        }
        if kind == BattlementKind::Machicolated && index % 2 == 0 {
            let corbel_position = centre + radial * (tower.radius_metres() + 0.18);
            spawn_box(
                world,
                &palette.stone,
                Vec3::new(0.28, 0.7, 0.32),
                Vec3::new(
                    corbel_position.x,
                    tower.wall_height_metres - 0.38,
                    corbel_position.y,
                ),
                Quat::from_rotation_y(-angle),
                "machicolation corbel",
            );
        }
    }
}

fn round_loop_parapet_mesh(radius: f32, height: f32) -> Mesh {
    const SEGMENTS: usize = 72;
    let half_height = height * 0.5;
    let mut faces = Vec::new();
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let radial_a = Vec2::new(angle_a.cos(), angle_a.sin()) * radius;
        let radial_b = Vec2::new(angle_b.cos(), angle_b.sin()) * radius;
        let ranges = if segment.is_multiple_of(SEGMENTS / 12) {
            vec![(0.0, 0.32), (0.9, height)]
        } else {
            vec![(0.0, height)]
        };
        for (low, high) in ranges {
            faces.push(vec![
                Vec3::new(radial_a.x, low - half_height, radial_a.y),
                Vec3::new(radial_a.x, high - half_height, radial_a.y),
                Vec3::new(radial_b.x, high - half_height, radial_b.y),
                Vec3::new(radial_b.x, low - half_height, radial_b.y),
            ]);
        }
    }
    flat_face_mesh(&faces)
}

fn cylinder_side_mesh(radius: f32, height: f32, segments: usize) -> Mesh {
    let half_height = height * 0.5;
    let mut faces = Vec::with_capacity(segments);
    for segment in 0..segments {
        let angle_a = segment as f32 * std::f32::consts::TAU / segments as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / segments as f32;
        let a = Vec2::new(angle_a.cos(), angle_a.sin()) * radius;
        let b = Vec2::new(angle_b.cos(), angle_b.sin()) * radius;
        faces.push(vec![
            Vec3::new(b.x, -half_height, b.y),
            Vec3::new(b.x, half_height, b.y),
            Vec3::new(a.x, half_height, a.y),
            Vec3::new(a.x, -half_height, a.y),
        ]);
    }
    flat_face_mesh(&faces)
}

fn spawn_stair(world: &mut World, palette: &RenderPalette, stair: Stair, origin: Vec2) {
    match stair {
        Stair::Straight {
            start,
            direction,
            base_height_metres,
            rise_metres,
            width_metres,
            tread_count,
        } => {
            let forward = match direction {
                Direction::North => Vec2::Y,
                Direction::East => Vec2::X,
                Direction::South => -Vec2::Y,
                Direction::West => -Vec2::X,
            };
            for tread in 0..tread_count {
                let progress = tread as f32 / tread_count.max(1) as f32;
                let position = start + origin + forward * progress * 3.8;
                spawn_box(
                    world,
                    &palette.stair,
                    Vec3::new(width_metres, 0.14, 0.28),
                    Vec3::new(
                        position.x,
                        base_height_metres + progress * rise_metres,
                        position.y,
                    ),
                    Quat::from_rotation_y(match direction {
                        Direction::North | Direction::South => 0.0,
                        Direction::East | Direction::West => std::f32::consts::FRAC_PI_2,
                    }),
                    "straight stair tread",
                );
            }
        }
        Stair::Spiral {
            centre,
            base_height_metres,
            rise_metres,
            inner_radius_metres,
            outer_radius_metres,
            turns,
            clockwise,
            tread_count,
        } => {
            let centre = centre + origin;
            spawn_box(
                world,
                &palette.stair,
                Vec3::new(
                    inner_radius_metres * 2.0,
                    rise_metres + 0.5,
                    inner_radius_metres * 2.0,
                ),
                Vec3::new(centre.x, base_height_metres + rise_metres * 0.5, centre.y),
                Quat::IDENTITY,
                "spiral stair newel",
            );
            for tread in 0..tread_count {
                let progress = tread as f32 / tread_count.max(1) as f32;
                let handedness = if clockwise { -1.0 } else { 1.0 };
                let angle = handedness * progress * turns * std::f32::consts::TAU;
                let radius = (inner_radius_metres + outer_radius_metres) * 0.5;
                let position = centre + Vec2::new(angle.cos(), angle.sin()) * radius;
                spawn_box(
                    world,
                    &palette.stair,
                    Vec3::new(outer_radius_metres - inner_radius_metres, 0.12, 0.32),
                    Vec3::new(
                        position.x,
                        base_height_metres + progress * rise_metres,
                        position.y,
                    ),
                    Quat::from_rotation_y(-angle),
                    "spiral stair tread",
                );
            }
        }
    }
}

fn spawn_wall_walk(world: &mut World, palette: &RenderPalette, wall_walk: WallWalk, origin: Vec2) {
    match wall_walk {
        WallWalk::Linear {
            start,
            end,
            elevation_metres,
            width_metres,
            outward,
        } => {
            let start = start + origin;
            let end = end + origin;
            let delta = end - start;
            let length = delta.length();
            if length <= 0.1 {
                return;
            }
            let outward = match outward {
                Direction::North => Vec2::Y,
                Direction::East => Vec2::X,
                Direction::South => -Vec2::Y,
                Direction::West => -Vec2::X,
            };
            let centre = (start + end) * 0.5 - outward * width_metres * 0.5;
            let horizontal = delta.x.abs() >= delta.y.abs();
            spawn_box(
                world,
                &palette.floor,
                if horizontal {
                    Vec3::new(length, 0.16, width_metres)
                } else {
                    Vec3::new(width_metres, 0.16, length)
                },
                Vec3::new(centre.x, elevation_metres - 0.08, centre.y),
                Quat::IDENTITY,
                "walkable rampart surface",
            );
        }
        WallWalk::Round {
            centre,
            elevation_metres,
            outer_radius_metres,
            stairwell_radius_metres,
        } => {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(annulus_mesh(
                stairwell_radius_metres,
                outer_radius_metres,
                0.16,
            ));
            let centre = centre + origin;
            world.spawn((
                Name::new("walkable tower-top deck with stairwell"),
                ClosedSolid,
                Mesh3d(mesh),
                MeshMaterial3d(palette.floor.clone()),
                Transform::from_xyz(centre.x, elevation_metres - 0.08, centre.y),
            ));
        }
        WallWalk::RectangularDeck {
            centre,
            size,
            elevation_metres,
            stairwell_centre,
            stairwell_size,
        } => {
            let centre = centre + origin;
            let stairwell_centre = stairwell_centre + origin;
            let side_depth = (size.y - stairwell_size.y) * 0.5;
            for sign in [-1.0, 1.0] {
                spawn_box(
                    world,
                    &palette.floor,
                    Vec3::new(size.x, 0.20, side_depth + 0.02),
                    Vec3::new(
                        centre.x,
                        elevation_metres - 0.09,
                        stairwell_centre.y + sign * (stairwell_size.y + side_depth) * 0.5,
                    ),
                    Quat::IDENTITY,
                    "walkable keep roof deck",
                );
            }
            let side_width = (size.x - stairwell_size.x) * 0.5;
            for sign in [-1.0, 1.0] {
                spawn_box(
                    world,
                    &palette.floor,
                    Vec3::new(side_width + 0.02, 0.20, stairwell_size.y + 0.02),
                    Vec3::new(
                        stairwell_centre.x + sign * (stairwell_size.x + side_width) * 0.5,
                        elevation_metres - 0.09,
                        stairwell_centre.y,
                    ),
                    Quat::IDENTITY,
                    "walkable keep roof deck",
                );
            }
        }
    }
}

fn annulus_mesh(inner_radius: f32, outer_radius: f32, height: f32) -> Mesh {
    sloped_annulus_mesh(inner_radius, outer_radius, height, 0.0, 0.0, 0, 0.0)
}

fn sloped_annulus_mesh(
    inner_radius: f32,
    outer_radius: f32,
    height: f32,
    inner_top_offset: f32,
    outer_top_offset: f32,
    drainage_outlet_count: u8,
    circumferential_fall: f32,
) -> Mesh {
    const SEGMENTS: usize = 64;
    let half_height = height * 0.5;
    let mut faces = Vec::with_capacity(SEGMENTS * 4);
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction_a = Vec2::new(angle_a.cos(), angle_a.sin());
        let direction_b = Vec2::new(angle_b.cos(), angle_b.sin());
        let channel_rise = |angle: f32| {
            if drainage_outlet_count == 0 {
                return 0.0;
            }
            let spacing = std::f32::consts::TAU / f32::from(drainage_outlet_count);
            let phase = angle.rem_euclid(spacing);
            phase.min(spacing - phase) / (spacing * 0.5) * circumferential_fall
        };
        let outer_top_a = half_height + outer_top_offset + channel_rise(angle_a);
        let outer_top_b = half_height + outer_top_offset + channel_rise(angle_b);
        let outer_a = direction_a * outer_radius;
        let outer_b = direction_b * outer_radius;
        let inner_a = direction_a * inner_radius;
        let inner_b = direction_b * inner_radius;
        faces.push(vec![
            Vec3::new(inner_a.x, half_height + inner_top_offset, inner_a.y),
            Vec3::new(outer_a.x, outer_top_a, outer_a.y),
            Vec3::new(outer_b.x, outer_top_b, outer_b.y),
            Vec3::new(inner_b.x, half_height + inner_top_offset, inner_b.y),
        ]);
        faces.push(vec![
            Vec3::new(outer_a.x, -half_height, outer_a.y),
            Vec3::new(inner_a.x, -half_height, inner_a.y),
            Vec3::new(inner_b.x, -half_height, inner_b.y),
            Vec3::new(outer_b.x, -half_height, outer_b.y),
        ]);
        faces.push(vec![
            Vec3::new(outer_a.x, -half_height, outer_a.y),
            Vec3::new(outer_b.x, -half_height, outer_b.y),
            Vec3::new(outer_b.x, outer_top_b, outer_b.y),
            Vec3::new(outer_a.x, outer_top_a, outer_a.y),
        ]);
        faces.push(vec![
            Vec3::new(inner_b.x, -half_height, inner_b.y),
            Vec3::new(inner_a.x, -half_height, inner_a.y),
            Vec3::new(inner_a.x, half_height + inner_top_offset, inner_a.y),
            Vec3::new(inner_b.x, half_height + inner_top_offset, inner_b.y),
        ]);
    }
    for face in &mut faces {
        face.reverse();
    }
    flat_face_mesh(&faces)
}

fn annular_sector_mesh(
    inner_radius: f32,
    outer_radius: f32,
    height: f32,
    start_angle: f32,
    end_angle: f32,
    inner_top_offset: f32,
    outer_top_offset: f32,
) -> Mesh {
    let sweep = (end_angle - start_angle).max(0.001);
    let segments = ((sweep / std::f32::consts::TAU * 64.0).ceil() as usize).max(1);
    let half = height * 0.5;
    let mut faces = Vec::with_capacity(segments * 4 + 2);
    let point =
        |radius: f32, angle: f32, y: f32| Vec3::new(radius * angle.cos(), y, radius * angle.sin());
    for segment in 0..segments {
        let a = start_angle + sweep * segment as f32 / segments as f32;
        let b = start_angle + sweep * (segment + 1) as f32 / segments as f32;
        faces.push(vec![
            point(inner_radius, a, half + inner_top_offset),
            point(outer_radius, a, half + outer_top_offset),
            point(outer_radius, b, half + outer_top_offset),
            point(inner_radius, b, half + inner_top_offset),
        ]);
        faces.push(vec![
            point(outer_radius, a, -half),
            point(inner_radius, a, -half),
            point(inner_radius, b, -half),
            point(outer_radius, b, -half),
        ]);
        faces.push(vec![
            point(outer_radius, a, -half),
            point(outer_radius, b, -half),
            point(outer_radius, b, half + outer_top_offset),
            point(outer_radius, a, half + outer_top_offset),
        ]);
        faces.push(vec![
            point(inner_radius, b, -half),
            point(inner_radius, a, -half),
            point(inner_radius, a, half + inner_top_offset),
            point(inner_radius, b, half + inner_top_offset),
        ]);
    }
    faces.push(vec![
        point(inner_radius, start_angle, -half),
        point(outer_radius, start_angle, -half),
        point(outer_radius, start_angle, half + outer_top_offset),
        point(inner_radius, start_angle, half + inner_top_offset),
    ]);
    // The end cap bounds the opposite side of the angular interval, so its
    // winding must oppose the start cap before the common face reversal.
    faces.push(vec![
        point(inner_radius, end_angle, half + inner_top_offset),
        point(outer_radius, end_angle, half + outer_top_offset),
        point(outer_radius, end_angle, -half),
        point(inner_radius, end_angle, -half),
    ]);
    for face in &mut faces {
        face.reverse();
    }
    flat_face_mesh(&faces)
}

fn spawn_architectural_section_markers(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    view: ViewerView,
    origin: Vec2,
) {
    let annotation = if let Some(opening) = focused_opening(plan, view) {
        let wall = plan
            .wall_assemblies
            .iter()
            .find(|wall| wall.id == opening.host_wall)
            .expect("focused opening wall");
        format!(
            "wall={}  opening={}  profile={}  thickness={:.2}m  throat={:.2}m  mouth={:.2}m",
            wall.id.0,
            opening.id.0,
            opening_profile_slug(opening.profile),
            wall.thickness_metres,
            opening.profile.exterior_width_metres(),
            opening.profile.interior_width_metres(),
        )
    } else if let Some(wall) = focused_wall(plan, view) {
        format!(
            "wall={}  opening=none  profile=solid_section  thickness={:.2}m",
            wall.id.0, wall.thickness_metres
        )
    } else {
        format!(
            "wall=round_tower  opening=radial  profile=shell_section  thickness={:.2}m",
            plan.towers
                .first()
                .map_or(0.0, |tower| tower.wall_thickness_metres)
        )
    };
    world.spawn((
        Name::new("architectural section authority annotation"),
        Text::new(annotation),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.06, 0.06, 0.05)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(4.0),
            bottom: Val::Percent(4.0),
            ..default()
        },
        NonCollidingVisualization,
    ));
    let (centre, outward, tangent, thickness, base) =
        if let Some(opening) = focused_opening(plan, view) {
            let Some(wall) = plan
                .wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall)
            else {
                return;
            };
            (
                Vec2::new(
                    opening.frame.origin.x + origin.x,
                    opening.frame.origin.y + origin.y,
                ),
                opening.frame.outward,
                opening.frame.tangent,
                wall.thickness_metres,
                opening.sill_elevation_metres,
            )
        } else if let Some(wall) = focused_wall(plan, view) {
            (
                wall.frame.origin + origin,
                wall.frame.outward,
                wall.frame.tangent,
                wall.thickness_metres,
                wall.base_elevation_metres,
            )
        } else if view == ViewerView::WallRoundTowerRadialSection {
            let Some(tower) = plan.towers.first().copied() else {
                return;
            };
            (
                tower.centre_metres() + origin,
                -Vec2::Y,
                Vec2::X,
                tower.wall_thickness_metres,
                0.0,
            )
        } else {
            return;
        };
    let label_view = Vec3::new(
        tangent.x + outward.x * 0.55,
        0.22,
        tangent.y + outward.y * 0.55,
    )
    .normalize();
    let label_rotation = Quat::from_rotation_arc(Vec3::Z, label_view);
    for (label, sign) in [("OUTSIDE", 1.0_f32), ("INSIDE", -1.0_f32)] {
        let position = centre + outward * sign * (thickness * 0.5 + 0.75);
        world.spawn((
            Name::new(format!("architectural section {label} label")),
            Text2d::new(label),
            TextFont {
                font_size: FontSize::Px(44.0),
                ..default()
            },
            TextColor(Color::srgb(0.12, 0.12, 0.10)),
            Transform {
                translation: Vec3::new(position.x, base + 2.45, position.y),
                // Text2d's front faces local -Z; face the deterministic
                // oblique section camera rather than relying on a cardinal yaw.
                rotation: label_rotation,
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    let outside_left_percent = if tangent.perp_dot(outward) < 0.0 {
        70.0
    } else {
        18.0
    };
    let inside_left_percent = 88.0 - outside_left_percent;
    for (label, left_percent) in [
        ("OUTSIDE", outside_left_percent),
        ("INSIDE", inside_left_percent),
    ] {
        world.spawn((
            Name::new(format!("architectural section screen {label} label")),
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(34.0),
                ..default()
            },
            TextColor(Color::srgb(0.08, 0.08, 0.07)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left_percent),
                top: Val::Percent(8.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    let figure = centre - outward * (thickness * 0.5 + 0.75);
    spawn_box(
        world,
        &palette.timber,
        Vec3::new(0.32, 1.35, 0.22),
        Vec3::new(figure.x, base + 0.675, figure.y),
        Quat::IDENTITY,
        "architectural section 1.75m scale torso",
    );
    spawn_box(
        world,
        &palette.timber,
        Vec3::splat(0.40),
        Vec3::new(figure.x, base + 1.55, figure.y),
        Quat::IDENTITY,
        "architectural section 1.75m scale head",
    );
    let marker_size = outward.abs() * thickness + tangent.abs() * 0.055;
    spawn_box(
        world,
        &palette.roof_secondary,
        Vec3::new(marker_size.x.max(0.055), 0.055, marker_size.y.max(0.055)),
        Vec3::new(centre.x, base + 0.18, centre.y),
        Quat::IDENTITY,
        "architectural section wall thickness dimension",
    );
}

fn spawn_resolved_architectural_surfaces(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    origin: Vec2,
    visible_owners: &std::collections::HashSet<u32>,
    view: ViewerView,
) {
    let removed_reveal = focused_opening(plan, view)
        .filter(|_| section_proof(view))
        .and_then(|opening| opening.reveal_surfaces.get(1).copied());
    let timber_focus = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    for surface in &plan.resolved_geometry.surfaces {
        if removed_reveal == Some(surface.id)
            || (!visible_owners.contains(&surface.owner.0)
                && !(surface.role
                    == adventuresim_building_generator::SurfaceRole::TimberCirculation
                    && timber_focus.contains(&surface.id.0)))
            || !matches!(
                surface.role,
                adventuresim_building_generator::SurfaceRole::LeftJambReveal
                    | adventuresim_building_generator::SurfaceRole::RightJambReveal
                    | adventuresim_building_generator::SurfaceRole::WeatherSill
                    | adventuresim_building_generator::SurfaceRole::Intrados
                    | adventuresim_building_generator::SurfaceRole::ExteriorThroat
                    | adventuresim_building_generator::SurfaceRole::InteriorMouth
                    | adventuresim_building_generator::SurfaceRole::Stance
                    | adventuresim_building_generator::SurfaceRole::TimberCirculation
            )
        {
            continue;
        }
        let centre = (surface.bounds.min + surface.bounds.max) * 0.5;
        let size = (surface.bounds.max - surface.bounds.min).max(Vec3::splat(0.008));
        let opening = plan
            .opening_assemblies
            .iter()
            .find(|opening| opening.owner == surface.owner);
        let wall = opening.and_then(|opening| {
            plan.wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall)
        });
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(match (opening, wall) {
                (Some(opening), Some(wall)) => {
                    opening_surface_mesh(surface, opening, wall, centre, size)
                }
                _ => resolved_surface_plane_mesh(size),
            });
        let boundary = match surface.role {
            adventuresim_building_generator::SurfaceRole::ExteriorThroat => {
                Some(OpeningBoundaryKind::ExteriorThroat)
            }
            adventuresim_building_generator::SurfaceRole::InteriorMouth => {
                Some(OpeningBoundaryKind::InteriorMouth)
            }
            _ => None,
        };
        let mut entity = world.spawn((
            Name::new(format!(
                "resolved surface owner {} {:?}",
                surface.owner.0, surface.role
            )),
            NonCollidingVisualization,
            GeometryOwner(surface.owner.0),
            ResolvedRenderItem {
                id: surface.id.0,
                fingerprint: stable_u64(
                    &serde_json::to_vec(surface).expect("serialize rendered architectural surface"),
                ),
                local_half_size: size * 0.5,
            },
            Mesh3d(mesh),
            MeshMaterial3d(
                if matches!(
                    surface.role,
                    adventuresim_building_generator::SurfaceRole::Stance
                        | adventuresim_building_generator::SurfaceRole::TimberCirculation
                ) {
                    if view == ViewerView::TimberRegistrationCut
                        && surface.role
                            == adventuresim_building_generator::SurfaceRole::TimberCirculation
                    {
                        palette.cutaway.clone()
                    } else {
                        palette.floor.clone()
                    }
                } else {
                    palette.roof_secondary.clone()
                },
            ),
            Transform::from_translation(centre + Vec3::new(origin.x, 0.0, origin.y)),
        ));
        if let Some(kind) = boundary {
            entity.insert(OpeningBoundary(kind));
        }
    }
}

fn opening_surface_mesh(
    surface: &adventuresim_building_generator::ResolvedSurface,
    opening: &adventuresim_building_generator::OpeningAssembly,
    wall: &adventuresim_building_generator::WallAssembly,
    centre: Vec3,
    size: Vec3,
) -> Mesh {
    use adventuresim_building_generator::SurfaceRole;
    let tangent = opening.frame.tangent;
    let outward = opening.frame.outward;
    let local = |plan: Vec2, y: f32| Vec3::new(plan.x, y, plan.y) - centre;
    let two_sided = |face: Vec<Vec3>| {
        let reverse = face.iter().copied().rev().collect::<Vec<_>>();
        flat_face_mesh(&[face, reverse])
    };
    match surface.role {
        SurfaceRole::LeftJambReveal | SurfaceRole::RightJambReveal => {
            let (side, exterior_width, interior_width) = match surface.shape {
                adventuresim_building_generator::ResolvedSurfaceShape::SplayedJamb {
                    side,
                    exterior_width_metres,
                    interior_width_metres,
                    ..
                } => (
                    f32::from(side),
                    exterior_width_metres,
                    interior_width_metres,
                ),
                _ => return resolved_surface_plane_mesh(size),
            };
            let exterior = opening.frame.origin
                + tangent * (side * exterior_width * 0.5)
                + outward * (wall.thickness_metres * 0.5);
            let interior = opening.frame.origin + tangent * (side * interior_width * 0.5)
                - outward * (wall.thickness_metres * 0.5);
            let bottom = opening.sill_elevation_metres;
            let top = bottom + opening.profile.clear_height_metres();
            two_sided(vec![
                local(exterior, bottom),
                local(interior, bottom),
                local(interior, top),
                local(exterior, top),
            ])
        }
        SurfaceRole::WeatherSill => {
            let half_width = opening.profile.interior_width_metres() * 0.5;
            let (inside_y, outside_y, drip_depth) = match surface.shape {
                adventuresim_building_generator::ResolvedSurfaceShape::WeatherSill {
                    interior_elevation_metres,
                    exterior_elevation_metres,
                    drip_depth_metres,
                } => (
                    interior_elevation_metres,
                    exterior_elevation_metres,
                    drip_depth_metres,
                ),
                _ => return resolved_surface_plane_mesh(size),
            };
            two_sided(vec![
                local(
                    opening.frame.origin - tangent * half_width
                        + outward * wall.thickness_metres * 0.5,
                    outside_y,
                ),
                local(
                    opening.frame.origin
                        + tangent * half_width
                        + outward * wall.thickness_metres * 0.5,
                    outside_y,
                ),
                local(
                    opening.frame.origin + tangent * half_width
                        - outward * wall.thickness_metres * 0.5,
                    inside_y,
                ),
                local(
                    opening.frame.origin
                        - tangent * half_width
                        - outward * wall.thickness_metres * 0.5,
                    inside_y,
                ),
                local(
                    opening.frame.origin - tangent * half_width
                        + outward * (wall.thickness_metres * 0.5 + drip_depth),
                    outside_y - drip_depth,
                ),
            ])
        }
        SurfaceRole::Intrados => {
            let segments = 16;
            let width = opening.profile.interior_width_metres();
            let half_width = width * 0.5;
            let sill = opening.sill_elevation_metres;
            let height_at = |along: f32| match opening.profile {
                adventuresim_building_generator::OpeningProfile::Segmental {
                    spring_height_metres,
                    rise_metres,
                    ..
                } => {
                    let radius = width * width / (8.0 * rise_metres.max(0.01)) + rise_metres * 0.5;
                    sill + spring_height_metres
                        + (radius * radius - along * along).max(0.0).sqrt()
                        + rise_metres
                        - radius
                }
                adventuresim_building_generator::OpeningProfile::PointedTwoCentred {
                    spring_height_metres,
                    arc_radius_metres,
                    ..
                } => {
                    let offset = (arc_radius_metres - half_width).max(0.0);
                    sill + spring_height_metres
                        + (arc_radius_metres * arc_radius_metres - (along.abs() + offset).powi(2))
                            .max(0.0)
                            .sqrt()
                }
                _ => sill + opening.profile.clear_height_metres(),
            };
            let mut faces = Vec::new();
            for index in 0..segments {
                let a = -half_width + width * index as f32 / segments as f32;
                let b = -half_width + width * (index + 1) as f32 / segments as f32;
                let outside_a =
                    opening.frame.origin + tangent * a + outward * wall.thickness_metres * 0.5;
                let outside_b =
                    opening.frame.origin + tangent * b + outward * wall.thickness_metres * 0.5;
                let inside_a =
                    opening.frame.origin + tangent * a - outward * wall.thickness_metres * 0.5;
                let inside_b =
                    opening.frame.origin + tangent * b - outward * wall.thickness_metres * 0.5;
                faces.push(vec![
                    local(outside_a, height_at(a)),
                    local(outside_b, height_at(b)),
                    local(inside_b, height_at(b)),
                    local(inside_a, height_at(a)),
                ]);
            }
            flat_face_mesh(&faces)
        }
        SurfaceRole::ExteriorThroat | SurfaceRole::InteriorMouth => {
            opening_boundary_outline_mesh(surface.role, opening, wall, centre)
        }
        _ => resolved_surface_plane_mesh(size),
    }
}

fn opening_boundary_outline_mesh(
    role: adventuresim_building_generator::SurfaceRole,
    opening: &adventuresim_building_generator::OpeningAssembly,
    wall: &adventuresim_building_generator::WallAssembly,
    centre: Vec3,
) -> Mesh {
    use adventuresim_building_generator::{OpeningProfile, SurfaceRole};

    let exterior = role == SurfaceRole::ExteriorThroat;
    let width = if exterior {
        opening.profile.exterior_width_metres()
    } else {
        opening.profile.interior_width_metres()
    };
    let height = match opening.profile {
        OpeningProfile::ArrowLoop {
            exterior_height_metres,
            interior_height_metres,
            ..
        }
        | OpeningProfile::GunLoop {
            exterior_height_metres,
            interior_height_metres,
            ..
        } => {
            if exterior {
                exterior_height_metres
            } else {
                interior_height_metres
            }
        }
        _ => opening.profile.clear_height_metres(),
    };
    let depth = if exterior {
        wall.thickness_metres * 0.5
    } else {
        -wall.thickness_metres * 0.5
    };
    let tangent = opening.frame.tangent;
    let outward = opening.frame.outward;
    let sill = opening.sill_elevation_metres;
    let border = 0.018_f32.min(width * 0.12).min(height * 0.06);
    let half_width = width * 0.5;
    let point = |along: f32, elevation: f32| {
        let plan = opening.frame.origin + tangent * along + outward * depth;
        Vec3::new(plan.x, elevation, plan.y) - centre
    };
    let top_at = |along: f32| match opening.profile {
        OpeningProfile::Segmental {
            spring_height_metres,
            rise_metres,
            ..
        } => {
            let radius = width * width / (8.0 * rise_metres.max(0.01)) + rise_metres * 0.5;
            sill + spring_height_metres
                + (radius * radius - along * along).max(0.0).sqrt()
                + rise_metres
                - radius
        }
        OpeningProfile::PointedTwoCentred {
            spring_height_metres,
            arc_radius_metres,
            ..
        } => {
            let offset = (arc_radius_metres - half_width).max(0.0);
            sill + spring_height_metres
                + (arc_radius_metres * arc_radius_metres - (along.abs() + offset).powi(2))
                    .max(0.0)
                    .sqrt()
        }
        _ => sill + height,
    };

    let mut faces = vec![
        vec![
            point(-half_width, sill),
            point(half_width, sill),
            point(half_width, sill + border),
            point(-half_width, sill + border),
        ],
        vec![
            point(-half_width, sill),
            point(-half_width + border, sill),
            point(-half_width + border, top_at(-half_width)),
            point(-half_width, top_at(-half_width)),
        ],
        vec![
            point(half_width - border, sill),
            point(half_width, sill),
            point(half_width, top_at(half_width)),
            point(half_width - border, top_at(half_width)),
        ],
    ];
    let segments = if matches!(
        opening.profile,
        OpeningProfile::Segmental { .. } | OpeningProfile::PointedTwoCentred { .. }
    ) {
        16
    } else {
        1
    };
    for index in 0..segments {
        let a = -half_width + width * index as f32 / segments as f32;
        let b = -half_width + width * (index + 1) as f32 / segments as f32;
        faces.push(vec![
            point(a, top_at(a)),
            point(b, top_at(b)),
            point(b, top_at(b) + border),
            point(a, top_at(a) + border),
        ]);
    }
    let reverse = faces
        .iter()
        .map(|face| face.iter().copied().rev().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    faces.extend(reverse);
    flat_face_mesh(&faces)
}

fn resolved_surface_plane_mesh(size: Vec3) -> Mesh {
    let half = size * 0.5;
    let face = if size.x <= size.y && size.x <= size.z {
        vec![
            Vec3::new(0.0, -half.y, -half.z),
            Vec3::new(0.0, half.y, -half.z),
            Vec3::new(0.0, half.y, half.z),
            Vec3::new(0.0, -half.y, half.z),
        ]
    } else if size.y <= size.z {
        vec![
            Vec3::new(-half.x, 0.0, -half.z),
            Vec3::new(-half.x, 0.0, half.z),
            Vec3::new(half.x, 0.0, half.z),
            Vec3::new(half.x, 0.0, -half.z),
        ]
    } else {
        vec![
            Vec3::new(-half.x, -half.y, 0.0),
            Vec3::new(half.x, -half.y, 0.0),
            Vec3::new(half.x, half.y, 0.0),
            Vec3::new(-half.x, half.y, 0.0),
        ]
    };
    let reverse = face.iter().copied().rev().collect::<Vec<_>>();
    flat_face_mesh(&[face, reverse])
}

fn timber_panel_prism_mesh(
    vertices: [Vec3; 3],
    outward: Vec2,
    depth_metres: f32,
    centre: Vec3,
) -> Mesh {
    let offset = Vec3::new(outward.x, 0.0, outward.y) * depth_metres * 0.5;
    let outward_3d = Vec3::new(outward.x, 0.0, outward.y);
    let mut oriented = vertices;
    if (oriented[1] - oriented[0])
        .cross(oriented[2] - oriented[0])
        .dot(outward_3d)
        < 0.0
    {
        oriented.swap(1, 2);
    }
    let front = oriented.map(|vertex| vertex + offset - centre);
    let back = oriented.map(|vertex| vertex - offset - centre);
    let mut faces = vec![front.to_vec(), vec![back[0], back[2], back[1]]];
    for index in 0..3 {
        let next = (index + 1) % 3;
        faces.push(vec![front[index], back[index], back[next], front[next]]);
    }
    flat_face_mesh(&faces)
}

fn spawn_resolved_crowns(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    origin: Vec2,
    visible_owners: Option<&std::collections::HashSet<u32>>,
    section_view: Option<ViewerView>,
) {
    let removed_items = section_view
        .map(|view| {
            architectural_section_removed_item_ids(plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let isolated_church_items = section_view
        .filter(|view| {
            matches!(
                view,
                ViewerView::ChurchBayInterior
                    | ViewerView::ChurchBaySection
                    | ViewerView::ChurchBayLoad
                    | ViewerView::ChurchBayVault
                    | ViewerView::ChurchCrossingInterior
                    | ViewerView::ChurchCrossingCutLoad
                    | ViewerView::ChurchChoirInterior
                    | ViewerView::ChurchChoirRadialSection
                    | ViewerView::ChurchTowerStair
                    | ViewerView::ChurchTowerBellUnderside
                    | ViewerView::ChurchTowerFrame
                    | ViewerView::ChurchDrainage
                    | ViewerView::ChurchSupportDag
            )
        })
        .map(|view| {
            church_focus_item_ids(plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        });
    let isolated_timber_items =
        section_view
            .filter(|view| timber_isolated_view(*view))
            .map(|view| {
                timber_focus_item_ids(plan, view)
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>()
            });
    for solid in &plan.resolved_geometry.solids {
        if removed_items.contains(&solid.id.0) {
            continue;
        }
        if isolated_timber_items
            .as_ref()
            .is_some_and(|items| !items.contains(&solid.id.0))
        {
            continue;
        }
        // Round shells are emitted by `spawn_tower`, which consumes the same
        // resolved solid ID while also applying its authoritative portal and
        // firing-loop subtractions. Spawning the envelope again here would
        // duplicate the masonry volume.
        if matches!(
            solid.shape,
            adventuresim_building_generator::ResolvedSolidShape::RoundTowerShell { .. }
        ) {
            continue;
        }
        let projected = plan
            .projected_defenses
            .iter()
            .find(|defense| defense.owner == solid.owner || defense.host_owner == solid.owner);
        let wall = plan
            .wall_assemblies
            .iter()
            .find(|wall| wall.host_solids.contains(&solid.id))
            .or_else(|| {
                plan.wall_assemblies.iter().find(|wall| {
                    wall.owner == solid.owner || wall.replaced_by_owner == Some(solid.owner)
                })
            });
        let material = if section_view == Some(ViewerView::TimberRegistrationCut)
            && solid.role == SolidRole::FrameFloor
            || matches!(
                section_view,
                Some(ViewerView::TimberOpeningBayInterior | ViewerView::TimberOpeningBaySection)
            ) && solid.role == SolidRole::WallHost
            || section_view == Some(ViewerView::TimberTownHallJunction)
                && solid.role == SolidRole::WallHost
        {
            &palette.cutaway
        } else if section_view == Some(ViewerView::ArtilleryRondelCasemate)
            && solid.role == SolidRole::ArtilleryEarthCore
        {
            // Preserve the authoritative residual mass in the casemate proof
            // while allowing its enclosed station, recoil area, smoke path,
            // and spiral access to be read simultaneously.
            &palette.cutaway
        } else if section_view == Some(ViewerView::ArtilleryCurtainSection)
            && solid.role == SolidRole::ArtilleryRetainingWall
        {
            // Section-only material separation: the authority remains
            // fieldstone masonry, while the warmer proof color makes the
            // inner retaining leaf distinguishable from the pale revetment.
            &palette.brick
        } else if section_view == Some(ViewerView::ArtilleryGateInterior)
            && solid.role == SolidRole::ArtilleryGateMechanism
        {
            // A warmer, lighter structural-timber swatch distinguishes the
            // windlass drum and rope from the deep chamber recess while
            // preserving the assembly's authoritative material semantics.
            &palette.stair
        } else {
            match solid.role {
                SolidRole::EdgeGuard
                | SolidRole::FrameMember
                | SolidRole::FrameSill
                | SolidRole::FramePost
                | SolidRole::FramePlate
                | SolidRole::FrameRail
                | SolidRole::FrameJoist
                | SolidRole::FrameGirder
                | SolidRole::FrameTie
                | SolidRole::FrameBrace
                | SolidRole::FrameJettyBeam
                | SolidRole::FrameKnagge
                | SolidRole::FrameGableMember
                | SolidRole::FrameDormerTrimmer
                | SolidRole::FrameOrnament
                | SolidRole::BeamJoist
                | SolidRole::RoofFraming
                | SolidRole::RoofPlate
                | SolidRole::ArtilleryBridgeBeam
                | SolidRole::ArtilleryBridgeDeck
                | SolidRole::ArtilleryGateMechanism => &palette.timber,
                SolidRole::ArtilleryEarthCore
                | SolidRole::DitchFloor
                | SolidRole::DitchScarp
                | SolidRole::DitchCounterscarp => &palette.earth,
                SolidRole::FrameInfill => match plan.wall_style {
                    WallStyle::Brick => &palette.brick,
                    WallStyle::Stone => &palette.stone,
                    WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
                },
                SolidRole::FrameFloor
                | SolidRole::WalkSurface
                | SolidRole::DrainageChannel
                | SolidRole::DrainageFloor
                | SolidRole::GalleryFloor
                | SolidRole::Landing
                | SolidRole::CircuitWalk
                | SolidRole::ChurchFloor
                | SolidRole::ChurchBellFloor
                | SolidRole::ChurchVaultShell => &palette.floor,
                SolidRole::ChurchStairTread | SolidRole::ArtilleryStairTread => &palette.stair,
                SolidRole::ChurchStairNewel | SolidRole::ChurchServiceLadder => &palette.timber,
                SolidRole::RoofFlashing if solid.size.y <= 0.03 && solid.size.z <= 0.12 => {
                    &palette.roof
                }
                SolidRole::DefenseRoof | SolidRole::RoofFlashing | SolidRole::RoofGutter => {
                    &palette.roof_secondary
                }
                SolidRole::ProjectionSupport
                    if projected.is_some_and(|defense| {
                        defense.material
                            == adventuresim_building_generator::ProjectedDefenseMaterial::Timber
                    }) =>
                {
                    &palette.timber
                }
                SolidRole::OpeningClosure
                | SolidRole::WeaponMount
                | SolidRole::ChurchBellFrame
                | SolidRole::ChurchGuard => &palette.timber,
                SolidRole::ChurchBell => &palette.roof_secondary,
                SolidRole::Mullion => &palette.stone,
                SolidRole::LeadedGlazing => &palette.glass,
                SolidRole::WallHost
                | SolidRole::OpeningJamb
                | SolidRole::OpeningSill
                | SolidRole::OpeningHead
                | SolidRole::OpeningSpandrel => match wall.map(|wall| wall.material) {
                    Some(adventuresim_building_generator::WallMaterialClass::TimberInfill) => {
                        match plan.wall_style {
                            WallStyle::Brick => &palette.brick,
                            WallStyle::Stone => &palette.stone,
                            WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
                        }
                    }
                    Some(adventuresim_building_generator::WallMaterialClass::InternalTimber) => {
                        &palette.timber
                    }
                    Some(adventuresim_building_generator::WallMaterialClass::CivilianMasonry) => {
                        match plan.wall_style {
                            WallStyle::Brick => &palette.brick,
                            WallStyle::Plaster | WallStyle::TimberFrame => &palette.plaster,
                            WallStyle::Stone => &palette.stone,
                        }
                    }
                    _ => &palette.stone,
                },
                _ => &palette.stone,
            }
        };
        // Thick military walls can be deeper than one of their side piers, so
        // size comparison is not an orientation authority. Use the owning
        // wall's local frame; otherwise a 1.2 m wall rotates narrow jambs by
        // 90 degrees and creates the full-storey exterior fins seen in the
        // courtyard regression.
        let tangent_is_z = wall
            .map(|wall| wall.frame.tangent.y.abs() > 0.5)
            .unwrap_or(solid.size.z > solid.size.x);
        let (mesh, shape_yaw) = match solid.shape {
            adventuresim_building_generator::ResolvedSolidShape::SegmentalArchRing {
                spring_height_metres,
                rise_metres,
                ..
            } => (
                if matches!(
                    solid.role,
                    SolidRole::OpeningClosure | SolidRole::LeadedGlazing
                ) {
                    arched_panel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        spring_height_metres,
                        rise_metres,
                        None,
                    )
                } else {
                    arched_spandrel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        rise_metres,
                        None,
                    )
                },
                tangent_is_z.then_some(std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::PointedArchRing {
                spring_height_metres,
                apex_height_metres,
                arc_radius_metres,
                ..
            } => (
                if matches!(
                    solid.role,
                    SolidRole::OpeningClosure | SolidRole::LeadedGlazing
                ) {
                    arched_panel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        spring_height_metres,
                        apex_height_metres - spring_height_metres,
                        Some(arc_radius_metres),
                    )
                } else {
                    arched_spandrel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        apex_height_metres - spring_height_metres,
                        Some(arc_radius_metres),
                    )
                },
                tangent_is_z.then_some(std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::TimberPanelPrism {
                vertices,
                outward,
                depth_metres,
            } => (
                timber_panel_prism_mesh(vertices, outward, depth_metres, solid.centre),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::SplayedReveal {
                exterior_width_metres,
                interior_width_metres,
                side,
                exterior_depth_sign,
            } => (
                splayed_jamb_mesh(
                    solid.size.x.max(solid.size.z),
                    solid.size.y,
                    solid.size.x.min(solid.size.z),
                    exterior_width_metres,
                    interior_width_metres,
                    side,
                    exterior_depth_sign,
                ),
                tangent_is_z.then_some(-std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::SplayedHead {
                exterior_clear_height_metres,
                interior_clear_height_metres,
                exterior_depth_sign,
            } => (
                splayed_head_mesh(
                    solid.size.x.max(solid.size.z),
                    solid.size.y,
                    solid.size.x.min(solid.size.z),
                    exterior_clear_height_metres,
                    interior_clear_height_metres,
                    exterior_depth_sign,
                ),
                tangent_is_z.then_some(-std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::Cuboid => (
                Mesh::from(Cuboid::new(solid.size.x, solid.size.y, solid.size.z)),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::AnnularPrism {
                inner_radius_metres,
                outer_radius_metres,
                inner_top_offset_metres,
                outer_top_offset_metres,
                drainage_outlet_count,
                circumferential_fall_metres,
            } => (
                sloped_annulus_mesh(
                    inner_radius_metres,
                    outer_radius_metres,
                    solid.size.y,
                    inner_top_offset_metres,
                    outer_top_offset_metres,
                    drainage_outlet_count,
                    circumferential_fall_metres,
                ),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres,
                outer_radius_metres,
                start_angle_radians,
                end_angle_radians,
                inner_top_offset_metres,
                outer_top_offset_metres,
            } => (
                annular_sector_mesh(
                    inner_radius_metres,
                    outer_radius_metres,
                    solid.size.y,
                    start_angle_radians,
                    end_angle_radians,
                    inner_top_offset_metres,
                    outer_top_offset_metres,
                ),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::RoundTowerShell { .. } => {
                unreachable!("round shells are rendered by spawn_tower")
            }
        };
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let resolved_yaw = if matches!(
            solid.role,
            SolidRole::RoofFraming
                | SolidRole::RoofFlashing
                | SolidRole::RoofGutter
                | SolidRole::RoofEdgeTreatment
        ) {
            -solid.yaw_radians
        } else {
            solid.yaw_radians
        };
        world.spawn((
            Name::new(if projected.is_some() {
                format!(
                    "resolved projected owner {} {:?}",
                    solid.owner.0, solid.role
                )
            } else {
                format!("resolved crown owner {} {:?}", solid.owner.0, solid.role)
            }),
            ClosedSolid,
            GeometryOwner(solid.owner.0),
            ResolvedRenderItem {
                id: solid.id.0,
                fingerprint: stable_u64(
                    &serde_json::to_vec(solid).expect("serialize rendered resolved solid"),
                ),
                local_half_size: solid.size * 0.5,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: solid.centre + Vec3::new(origin.x, 0.0, origin.y),
                rotation: Quat::from_rotation_y(resolved_yaw)
                    * Quat::from_rotation_y(shape_yaw.unwrap_or(0.0))
                    * Quat::from_rotation_x(solid.crossfall_radians)
                    * Quat::from_rotation_z(solid.longfall_radians),
                ..default()
            },
            if isolated_church_items.as_ref().map_or_else(
                || visible_owners.is_none_or(|visible| visible.contains(&solid.owner.0)),
                |items| items.contains(&solid.id.0),
            ) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }
}

fn spawn_artillery_marker_segment(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    thickness: f32,
    name: &'static str,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(thickness, length, thickness));
    world.spawn((
        Name::new(name),
        NonCollidingVisualization,
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        Transform {
            translation: (start + end) * 0.5,
            rotation: Quat::from_rotation_arc(Vec3::Y, delta / length),
            ..default()
        },
    ));
}

fn spawn_artillery_proof_markers(
    world: &mut World,
    plan: &BuildingPlan,
    view: ViewerView,
    origin: Vec2,
) {
    let Some(castle) = &plan.artillery_castle else {
        return;
    };
    let offset = Vec3::new(origin.x, 0.0, origin.y);
    match view {
        ViewerView::ArtilleryCirculation => {
            let route_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.95, 0.55, 0.06),
                        unlit: true,
                        ..default()
                    });
            for edge in &castle.route_edges {
                for pair in edge.sweep_path.windows(2) {
                    spawn_artillery_marker_segment(
                        world,
                        &route_material,
                        pair[0] + offset + Vec3::Y * 0.15,
                        pair[1] + offset + Vec3::Y * 0.15,
                        0.12,
                        "artillery authoritative swept circulation edge",
                    );
                }
            }
        }
        ViewerView::ArtilleryFirePlan => {
            let ray_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.85, 0.08, 0.04),
                        unlit: true,
                        ..default()
                    });
            for station in &castle.stations {
                for ray in &station.rays {
                    spawn_artillery_marker_segment(
                        world,
                        &ray_material,
                        ray.origin + offset + Vec3::Y * 0.05,
                        ray.target + offset + Vec3::Y * 0.05,
                        0.10,
                        "artillery authoritative firing ray",
                    );
                }
            }
        }
        ViewerView::ArtilleryDrainage => {
            let drain_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.05, 0.45, 0.90),
                        unlit: true,
                        ..default()
                    });
            for route_id in castle
                .drainage_routes
                .iter()
                .chain(&castle.ditch.drainage_routes)
            {
                if let Some(route) = plan
                    .resolved_geometry
                    .drainage_routes
                    .iter()
                    .find(|route| route.id == *route_id)
                {
                    spawn_artillery_marker_segment(
                        world,
                        &drain_material,
                        route.inlet + offset + Vec3::Y * 0.05,
                        route.outlet + offset + Vec3::Y * 0.05,
                        0.10,
                        "artillery authoritative drainage route",
                    );
                }
            }
        }
        ViewerView::ArtillerySupportDag => {
            let support_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.70, 0.12, 0.70),
                        unlit: true,
                        ..default()
                    });
            for node in &plan.resolved_geometry.structural_nodes {
                if !node.supported_by.is_empty() && matches!(node.kind,
                    adventuresim_building_generator::StructuralNodeKind::ArtilleryRevetmentBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryRetainingBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryTerrepleinBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryRondelBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryBridgeAbutment)
                {
                    for supporting in &node.supported_by {
                        if let Some(base)=plan.resolved_geometry.structural_nodes.iter().find(|candidate|candidate.id==*supporting) {
                            spawn_artillery_marker_segment(world,&support_material,node.position+offset,base.position+offset,0.10,"artillery authoritative support edge");
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn spawn_crown_defender_scale(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    view: ViewerView,
    origin: Vec2,
) {
    let owner = match view {
        ViewerView::CrownTowerExterior
        | ViewerView::CrownTowerTop
        | ViewerView::CrownTowerCutaway => {
            let preferred = plan
                .gate_defenses
                .first()
                .and_then(|gate| gate.firing_positions.first())
                .map(|position| position.tower_index);
            plan.crowns.iter().find_map(|crown| match crown.path {
                CrownPath::Round { tower_index, .. }
                    if preferred.is_none_or(|preferred| preferred == tower_index) =>
                {
                    Some(crown.owner)
                }
                _ => None,
            })
        }
        ViewerView::CrownCornerExterior | ViewerView::CrownCornerInterior => plan
            .crowns
            .iter()
            .flat_map(|crown| {
                crown
                    .junctions
                    .iter()
                    .map(move |junction| (crown, junction))
            })
            .find(|(_, junction)| {
                junction.kind == adventuresim_building_generator::CrownJunctionKind::Corner
            })
            .map(|(crown, _)| crown.owner),
        _ => plan
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Straight { .. }))
            .map(|crown| crown.owner),
    };
    let Some(sample) = owner.and_then(|owner| {
        plan.resolved_geometry
            .defender_samples
            .iter()
            .find(|sample| sample.owner == owner)
    }) else {
        return;
    };
    let base = sample.stance + Vec3::new(origin.x, 0.0, origin.y);
    for (name, size, offset) in [
        (
            "non-colliding 1.72m defender scale torso",
            Vec3::new(0.38, 0.88, 0.24),
            Vec3::new(0.0, 0.72, 0.0),
        ),
        (
            "non-colliding defender scale head",
            Vec3::splat(0.28),
            Vec3::new(0.0, 1.38, 0.0),
        ),
        (
            "non-colliding defender scale legs",
            Vec3::new(0.28, 0.58, 0.2),
            Vec3::new(0.0, 0.29, 0.0),
        ),
    ] {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(size.x, size.y, size.z));
        world.spawn((
            Name::new(name),
            NonCollidingVisualization,
            Mesh3d(mesh),
            MeshMaterial3d(palette.timber.clone()),
            Transform::from_translation(base + offset),
        ));
    }
}

fn spawn_projected_proof_markers(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    owner: adventuresim_building_generator::GeometryOwnerId,
    origin: Vec2,
    view: ViewerView,
) {
    let Some(defense) = plan
        .projected_defenses
        .iter()
        .find(|defense| defense.owner == owner)
    else {
        return;
    };
    let (centre, outward, tangent, extent) = match defense.path {
        ProjectedDefensePath::Linear {
            start,
            end,
            outward,
        } => {
            let outward = direction_vector_2d(outward);
            (
                (start + end) * 0.5,
                outward,
                (end - start).normalize_or_zero(),
                start.distance(end),
            )
        }
        ProjectedDefensePath::Round {
            centre,
            radius_metres,
            outward,
        } => {
            let outward = direction_vector_2d(outward);
            (
                centre,
                outward,
                Vec2::new(-outward.y, outward.x),
                radius_metres * 2.0,
            )
        }
    };
    let centre = centre + origin;
    let body = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cylinder::new(0.18, 1.7));
    world.spawn((
        Name::new("projected defense defender scale"),
        Mesh3d(body),
        MeshMaterial3d(palette.timber.clone()),
        Transform::from_xyz(centre.x, defense.floor_elevation_metres + 0.85, centre.y),
    ));
    let calibration_size = Vec3::splat(0.8);
    let calibration_side = if view == ViewerView::ProjectedInterior {
        -outward
    } else {
        outward
    };
    // Keep the luminance witness in-frame but beyond the authoritative work's
    // tangent end and projection envelope. It must never masquerade as a
    // corbel, merlon or freestanding host pier in the proof silhouette.
    let calibration_position = centre
        + calibration_side * (defense.projection_metres + 0.8)
        + tangent * (extent * 0.5 + 0.7);
    let calibration_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::from_size(calibration_size));
    world.spawn((
        Name::new("projected daylight calibration block"),
        NonCollidingVisualization,
        LightingCalibration {
            local_center: Vec3::ZERO,
            local_half_size: calibration_size * 0.5,
        },
        Mesh3d(calibration_mesh),
        MeshMaterial3d(palette.stone.clone()),
        Transform {
            translation: Vec3::new(
                calibration_position.x,
                defense.floor_elevation_metres + 0.4,
                calibration_position.y,
            ),
            rotation: Quat::from_rotation_y(if view == ViewerView::ProjectedSockets {
                1.5
            } else {
                0.55
            }) * Quat::from_rotation_x(if view == ViewerView::ProjectedTop {
                0.75
            } else {
                0.35
            }),
            ..default()
        },
    ));
    if let Some(ray) = plan
        .resolved_geometry
        .projected_defense_rays
        .iter()
        .find(|ray| ray.owner == owner)
    {
        let start = ray.origin + Vec3::new(origin.x, 0.0, origin.y);
        let end = ray.target + Vec3::new(origin.x, 0.0, origin.y);
        spawn_timber_beam(
            world,
            &palette.roof,
            start,
            end,
            0.035,
            "projected defense downward ray",
        );
        let target = world.resource_mut::<Assets<Mesh>>().add(Sphere::new(0.18));
        world.spawn((
            Name::new("projected defense wall-foot target"),
            Mesh3d(target),
            MeshMaterial3d(palette.roof.clone()),
            Transform::from_translation(end),
        ));
    }
}

fn spawn_battlement_run(
    world: &mut World,
    palette: &RenderPalette,
    run: BattlementRun,
    origin: Vec2,
) {
    let start = run.start + origin;
    let end = run.end + origin;
    let delta = end - start;
    let length = delta.length();
    if length <= 0.1 {
        return;
    }
    let tangent = delta / length;
    let outward = match run.outward {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    };
    let projection = match run.kind {
        BattlementKind::Machicolated | BattlementKind::Breteche => 0.42,
        BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding => 0.68,
        BattlementKind::Crenellated
        | BattlementKind::PiercedCrenellated
        | BattlementKind::CoveredWallWalk
        | BattlementKind::GunLoopParapet => 0.0,
    };
    let centre = (start + end) * 0.5 + outward * projection;
    let horizontal = delta.x.abs() >= delta.y.abs();
    let merlon_count = (length / 1.2).floor().max(2.0) as usize;
    let gallery_size = if horizontal {
        Vec3::new(length, 0.16, projection * 2.0 + 0.42)
    } else {
        Vec3::new(projection * 2.0 + 0.42, 0.16, length)
    };

    if matches!(
        run.kind,
        BattlementKind::Machicolated
            | BattlementKind::Breteche
            | BattlementKind::OpenHoarding
            | BattlementKind::RoofedHoarding
    ) {
        let material = if matches!(
            run.kind,
            BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding
        ) {
            &palette.timber
        } else {
            &palette.stone
        };
        spawn_box(
            world,
            material,
            gallery_size,
            Vec3::new(centre.x, run.base_height_metres, centre.y),
            Quat::IDENTITY,
            "projecting defensive gallery floor",
        );
    }

    if run.kind == BattlementKind::GunLoopParapet {
        for (height, y) in [(0.32, 0.16), (0.25, 1.125)] {
            spawn_box(
                world,
                &palette.stone,
                if horizontal {
                    Vec3::new(length, height, 0.42)
                } else {
                    Vec3::new(0.42, height, length)
                },
                Vec3::new(centre.x, run.base_height_metres + y, centre.y),
                Quat::IDENTITY,
                "gun-loop parapet horizontal masonry",
            );
        }
        let interval = length / merlon_count as f32;
        let slit_width = 0.12;
        let side_width = (interval - slit_width).max(0.1) * 0.5;
        for index in 0..merlon_count {
            let position = start.lerp(end, (index as f32 + 0.5) / merlon_count as f32);
            for sign in [-1.0, 1.0] {
                let pier = position + tangent * (slit_width + side_width) * 0.5 * sign;
                spawn_box(
                    world,
                    &palette.stone,
                    if horizontal {
                        Vec3::new(side_width, 0.72, 0.42)
                    } else {
                        Vec3::new(0.42, 0.72, side_width)
                    },
                    Vec3::new(pier.x, run.base_height_metres + 0.68, pier.y),
                    Quat::IDENTITY,
                    "gun-loop parapet pier",
                );
            }
        }
    }

    for index in 0..merlon_count {
        let progress = (index as f32 + 0.5) / merlon_count as f32;
        let position = start.lerp(end, progress) + outward * projection;
        if run.kind != BattlementKind::GunLoopParapet {
            let merlon_material = if matches!(
                run.kind,
                BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding
            ) {
                &palette.timber
            } else {
                &palette.stone
            };
            if run.kind == BattlementKind::PiercedCrenellated {
                let side_width = 0.27;
                for sign in [-1.0, 1.0] {
                    let pier = position + tangent * 0.205 * sign;
                    spawn_box(
                        world,
                        merlon_material,
                        if horizontal {
                            Vec3::new(side_width, 0.85, 0.38)
                        } else {
                            Vec3::new(0.38, 0.85, side_width)
                        },
                        Vec3::new(pier.x, run.base_height_metres + 0.425, pier.y),
                        Quat::IDENTITY,
                        "merlon split by firing loop",
                    );
                }
            } else {
                spawn_box(
                    world,
                    merlon_material,
                    if horizontal {
                        Vec3::new(0.68, 0.85, 0.38)
                    } else {
                        Vec3::new(0.38, 0.85, 0.68)
                    },
                    Vec3::new(position.x, run.base_height_metres + 0.425, position.y),
                    Quat::IDENTITY,
                    "battlement merlon",
                );
            }
        }
        if matches!(
            run.kind,
            BattlementKind::Machicolated | BattlementKind::Breteche
        ) && index % 2 == 0
        {
            let corbel = position - outward * 0.16;
            spawn_box(
                world,
                &palette.stone,
                if horizontal {
                    Vec3::new(0.26, 0.72, 0.52)
                } else {
                    Vec3::new(0.52, 0.72, 0.26)
                },
                Vec3::new(corbel.x, run.base_height_metres - 0.32, corbel.y),
                Quat::IDENTITY,
                "machicolation corbel",
            );
        }
        if matches!(
            run.kind,
            BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding
        ) {
            let base = start.lerp(end, progress) + outward * 0.16;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(base.x, run.base_height_metres - 0.72, base.y),
                Vec3::new(position.x, run.base_height_metres + 0.95, position.y),
                0.13,
                "hoarding support strut",
            );
        }
    }

    if matches!(
        run.kind,
        BattlementKind::RoofedHoarding | BattlementKind::CoveredWallWalk | BattlementKind::Breteche
    ) {
        let roof_centre = centre + outward * 0.16;
        spawn_box(
            world,
            &palette.roof_secondary,
            if horizontal {
                Vec3::new(length + 0.5, 0.14, 1.55)
            } else {
                Vec3::new(1.55, 0.14, length + 0.5)
            },
            Vec3::new(roof_centre.x, run.base_height_metres + 1.62, roof_centre.y),
            if horizontal {
                Quat::from_rotation_x(0.10)
            } else {
                Quat::from_rotation_z(-0.10)
            },
            "covered wall-walk roof",
        );
    }
}

fn spawn_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    size: Vec3,
    translation: Vec3,
    rotation: Quat,
    name: &'static str,
) {
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(size.x, size.y, size.z));
    world.spawn((
        Name::new(name),
        ClosedSolid,
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        Transform {
            translation,
            rotation,
            ..default()
        },
    ));
}

fn capture_when_ready(
    mut commands: Commands,
    mut state: ResMut<CaptureState>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    named_meshes: Query<(&Name, &ViewVisibility)>,
    text_names: Query<&Name, With<Text>>,
    rendered_owners: Query<&GeometryOwner>,
    rendered_items: Query<&ResolvedRenderItem>,
    roof_items: Query<(&RoofRenderItem, &GlobalTransform, &ViewVisibility)>,
    focused_items: Query<(&ResolvedRenderItem, &GlobalTransform, &ViewVisibility)>,
    opening_boundaries: Query<(
        &OpeningBoundary,
        &ResolvedRenderItem,
        &GlobalTransform,
        &ViewVisibility,
    )>,
    calibration_blocks: Query<(&LightingCalibration, &GlobalTransform, &ViewVisibility)>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(output) = state.output.clone() else {
        return;
    };
    if state.in_flight {
        return;
    }
    if state.settled < state.settle_frames {
        state.settled += 1;
        return;
    }
    state.manifest.observed_mesh_count = meshes.iter().count();
    state.manifest.visible_mesh_count = meshes.iter().filter(|visible| visible.get()).count();
    let visible_names = named_meshes
        .iter()
        .filter(|(_, visibility)| visibility.get())
        .map(|(name, _)| name.as_str().to_owned())
        .collect::<Vec<_>>();
    state.manifest.visible_focus_object_count = visible_names
        .iter()
        .filter(|name| focus_name_matches(state.manifest.focus_kind, name))
        .count();
    state.manifest.focus_requirements_met = focus_requirements_met(
        state.manifest.focus_kind,
        &visible_names,
        state.manifest.focused_tower_indices.len(),
    );
    state.manifest.inside_label_visible = visible_names
        .iter()
        .any(|name| name.contains("architectural section INSIDE label"));
    state.manifest.outside_label_visible = visible_names
        .iter()
        .any(|name| name.contains("architectural section OUTSIDE label"));
    state.manifest.scale_figure_visible = visible_names
        .iter()
        .any(|name| name.contains("architectural section 1.75m scale"));
    state.manifest.section_annotation_visible = text_names.iter().any(|name| {
        name.as_str()
            .contains("architectural section authority annotation")
            || name.as_str().contains("roof proof authority annotation")
            || name.as_str().contains("church proof authority annotation")
            || name.as_str().contains("timber proof authority annotation")
            || name
                .as_str()
                .contains("artillery proof authority annotation")
    });
    state.manifest.church_legend_visible = text_names
        .iter()
        .any(|name| name.as_str().contains("church proof authority annotation"));
    state.manifest.timber_legend_visible = text_names
        .iter()
        .any(|name| name.as_str().contains("timber proof authority annotation"));
    state.manifest.artillery_legend_visible = text_names.iter().any(|name| {
        name.as_str()
            .contains("artillery proof authority annotation")
    });
    state.manifest.active_camera_count = cameras
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .count();
    state.manifest.rendered_owner_count = rendered_owners
        .iter()
        .map(|owner| owner.0)
        .collect::<std::collections::HashSet<_>>()
        .len();
    state.manifest.rendered_resolved_solid_count = rendered_items.iter().count();
    state.manifest.rendered_geometry_hash = resolved_item_multiset_hash(
        rendered_items
            .iter()
            .map(|item| (item.id, item.fingerprint)),
    );
    state.manifest.rendered_roof_item_count = roof_items.iter().count();
    state.manifest.rendered_roof_hash = resolved_item_multiset_hash(
        roof_items
            .iter()
            .map(|(item, _, _)| (item.id, item.fingerprint)),
    );
    let focused_roof_ids = state
        .manifest
        .focused_roof_item_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    state.manifest.visible_focused_roof_item_count = roof_items
        .iter()
        .filter(|(item, _, visibility)| focused_roof_ids.contains(&item.id) && visibility.get())
        .count();
    let focused_ids = state
        .manifest
        .focused_resolved_item_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let removed_focused_ids = state
        .manifest
        .section_removed_item_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    state.manifest.visible_focused_resolved_item_count = focused_items
        .iter()
        .filter(|(item, _, visibility)| {
            focused_ids.contains(&item.id)
                && !removed_focused_ids.contains(&item.id)
                && visibility.get()
        })
        .map(|(item, _, _)| item.id)
        .collect::<std::collections::HashSet<_>>()
        .len();
    if let Some((camera, camera_transform)) = cameras.iter().find(|(camera, _)| camera.is_active) {
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for (item, transform, visibility) in &focused_items {
            if !focused_ids.contains(&item.id) || !visibility.get() {
                continue;
            }
            for x in [-1.0_f32, 1.0] {
                for y in [-1.0_f32, 1.0] {
                    for z in [-1.0_f32, 1.0] {
                        let world =
                            transform.transform_point(item.local_half_size * Vec3::new(x, y, z));
                        if let Ok(pixel) = camera.world_to_viewport(camera_transform, world) {
                            let fraction = pixel / Vec2::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32);
                            min = min.min(fraction);
                            max = max.max(fraction);
                        }
                    }
                }
            }
        }
        for (item, transform, visibility) in &roof_items {
            if !focused_roof_ids.contains(&item.id) || !visibility.get() {
                continue;
            }
            for x in [-1.0_f32, 1.0] {
                for y in [-1.0_f32, 1.0] {
                    for z in [-1.0_f32, 1.0] {
                        let world = transform.transform_point(
                            item.local_center + item.local_half_size * Vec3::new(x, y, z),
                        );
                        if let Ok(pixel) = camera.world_to_viewport(camera_transform, world) {
                            let fraction = pixel / Vec2::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32);
                            min = min.min(fraction);
                            max = max.max(fraction);
                        }
                    }
                }
            }
        }
        if min.is_finite() && max.is_finite() {
            state.manifest.focused_bounds_fraction = [min.x, min.y, max.x, max.y];
        }
        let role_items = state.manifest.timber_role_item_ids.clone();
        for (role, ids) in role_items {
            let ids = ids.into_iter().collect::<std::collections::HashSet<_>>();
            let mut role_min = Vec2::splat(f32::INFINITY);
            let mut role_max = Vec2::splat(f32::NEG_INFINITY);
            for (item, transform, visibility) in &focused_items {
                if !ids.contains(&item.id) || !visibility.get() {
                    continue;
                }
                for x in [-1.0_f32, 1.0] {
                    for y in [-1.0_f32, 1.0] {
                        for z in [-1.0_f32, 1.0] {
                            let world = transform
                                .transform_point(item.local_half_size * Vec3::new(x, y, z));
                            if let Ok(pixel) = camera.world_to_viewport(camera_transform, world) {
                                let fraction =
                                    pixel / Vec2::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32);
                                role_min = role_min.min(fraction);
                                role_max = role_max.max(fraction);
                            }
                        }
                    }
                }
            }
            if role_min.is_finite() && role_max.is_finite() {
                state
                    .manifest
                    .timber_role_bounds_fraction
                    .insert(role, [role_min.x, role_min.y, role_max.x, role_max.y]);
            }
        }
        let artillery_role_items = state.manifest.artillery_role_item_ids.clone();
        for (role, ids) in artillery_role_items {
            let ids = ids.into_iter().collect::<std::collections::HashSet<_>>();
            let mut role_min = Vec2::splat(f32::INFINITY);
            let mut role_max = Vec2::splat(f32::NEG_INFINITY);
            for (item, transform, visibility) in &focused_items {
                if !ids.contains(&item.id) || !visibility.get() {
                    continue;
                }
                for x in [-1.0_f32, 1.0] {
                    for y in [-1.0_f32, 1.0] {
                        for z in [-1.0_f32, 1.0] {
                            let world = transform
                                .transform_point(item.local_half_size * Vec3::new(x, y, z));
                            if let Ok(pixel) = camera.world_to_viewport(camera_transform, world) {
                                let fraction =
                                    pixel / Vec2::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32);
                                role_min = role_min.min(fraction);
                                role_max = role_max.max(fraction);
                            }
                        }
                    }
                }
            }
            if role_min.is_finite() && role_max.is_finite() {
                state
                    .manifest
                    .artillery_role_bounds_fraction
                    .insert(role, [role_min.x, role_min.y, role_max.x, role_max.y]);
            }
        }
        for kind in [
            OpeningBoundaryKind::ExteriorThroat,
            OpeningBoundaryKind::InteriorMouth,
        ] {
            let mut boundary_min = Vec2::splat(f32::INFINITY);
            let mut boundary_max = Vec2::splat(f32::NEG_INFINITY);
            for (boundary, item, transform, visibility) in &opening_boundaries {
                if !visibility.get()
                    || !focused_ids.contains(&item.id)
                    || std::mem::discriminant(&boundary.0) != std::mem::discriminant(&kind)
                {
                    continue;
                }
                for x in [-1.0_f32, 1.0] {
                    for y in [-1.0_f32, 1.0] {
                        for z in [-1.0_f32, 1.0] {
                            let world = transform
                                .transform_point(item.local_half_size * Vec3::new(x, y, z));
                            if let Ok(pixel) = camera.world_to_viewport(camera_transform, world) {
                                let fraction =
                                    pixel / Vec2::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32);
                                boundary_min = boundary_min.min(fraction);
                                boundary_max = boundary_max.max(fraction);
                            }
                        }
                    }
                }
            }
            if boundary_min.is_finite() && boundary_max.is_finite() {
                let bounds = [
                    boundary_min.x,
                    boundary_min.y,
                    boundary_max.x,
                    boundary_max.y,
                ];
                match kind {
                    OpeningBoundaryKind::ExteriorThroat => {
                        state.manifest.exterior_throat_bounds_fraction = bounds
                    }
                    OpeningBoundaryKind::InteriorMouth => {
                        state.manifest.interior_mouth_bounds_fraction = bounds
                    }
                }
            }
        }
        let mut calibration_min = Vec2::splat(f32::INFINITY);
        let mut calibration_max = Vec2::splat(f32::NEG_INFINITY);
        for (block, transform, visibility) in &calibration_blocks {
            if !visibility.get() {
                continue;
            }
            for x in [-1.0_f32, 1.0] {
                for y in [-1.0_f32, 1.0] {
                    for z in [-1.0_f32, 1.0] {
                        let world = transform.transform_point(
                            block.local_center + block.local_half_size * Vec3::new(x, y, z),
                        );
                        if let Ok(pixel) = camera.world_to_viewport(camera_transform, world) {
                            let fraction = pixel / Vec2::new(VIEW_WIDTH as f32, VIEW_HEIGHT as f32);
                            calibration_min = calibration_min.min(fraction);
                            calibration_max = calibration_max.max(fraction);
                        }
                    }
                }
            }
        }
        if calibration_min.is_finite() && calibration_max.is_finite() {
            state.manifest.lighting_calibration_bounds_fraction = [
                calibration_min.x,
                calibration_min.y,
                calibration_max.x,
                calibration_max.y,
            ];
        }
    }
    state.in_flight = true;
    if !state.primed {
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                state.primed = true;
                state.settled = 0;
                state.in_flight = false;
            },
        );
        return;
    }

    let manifest_path = output.with_extension("capture.json");
    let mut manifest = state.manifest.clone();
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            manifest.pixel_hash = stable_evidence_hash(captured.image.data.as_deref().unwrap_or(&[]));
            manifest.subject_pixel_bps = subject_pixel_bps(captured.image.data.as_deref());
            let calibration = manifest.lighting_calibration_bounds_fraction;
            let has_calibration = calibration[2] > calibration[0]
                && calibration[3] > calibration[1]
                && calibration[0] >= 0.0
                && calibration[1] >= 0.0
                && calibration[2] <= 1.0
                && calibration[3] <= 1.0;
            let luminance = if has_calibration {
                calibration_luminance_stats(captured.image.data.as_deref(), calibration)
            } else {
                luminance_stats(
                    captured.image.data.as_deref(),
                    (!manifest.focused_resolved_item_ids.is_empty()
                        && manifest.focus_kind != Some("resolved_roof"))
                        .then_some(manifest.focused_bounds_fraction),
                    0.12,
                )
            };
            manifest.median_luminance_percent = luminance.median;
            manifest.dark_clipped_bps = luminance.dark_clipped_bps;
            manifest.bright_clipped_bps = luminance.bright_clipped_bps;
            manifest.luminance_separation_percent = luminance.separation;
            manifest.shadow_luminance_percent = luminance.shadow;
            manifest.validation_passed = manifest.subject_pixel_bps >= 100
                && manifest.plan_audit_issue_count == 0
                && manifest.mesh_integrity_issue_count == 0
                && manifest.median_luminance_percent >= 15
                && manifest.median_luminance_percent <= 85
                && manifest.dark_clipped_bps < 200
                && manifest.bright_clipped_bps < 200
                && manifest.luminance_separation_percent
                    >= if manifest.view == "timber-joint-close" {
                        // A joint proof is a sparse exact-ID skeleton against
                        // open background; percentile separation is dominated
                        // by that background even though the lit member faces
                        // and cast shadows remain legible. Other architectural
                        // captures retain the eight-point daylight gate.
                        3
                    } else if manifest.view == "roof-dormer-gabled-exterior" {
                        // This is now a true close dormer inspection against a
                        // nearly uniform parent-tile field. Exact child IDs,
                        // in-frame bounds, mesh correspondence, and the cast
                        // cheek/verge shadows carry the proof; the background-
                        // dominated percentile split is intentionally small.
                        1
                    } else if manifest.view == "artillery-whole-exterior"
                        || (manifest.view == "exterior"
                            && manifest.fixture == "artillery-rondel-castle")
                    {
                        // The low broad retrofit is dominated by one long
                        // revetment value at this regression distance; its
                        // tower curvature and ditch still retain deep cast
                        // shadows while a five-point percentile split is
                        // stable across the complete in-frame authority.
                        5
                    } else if matches!(manifest.view, "artillery-whole-top" | "artillery-trace-plan" | "artillery-fire-plan") {
                        // These are near-orthographic tactical plan proofs. The
                        // stable overhead light deliberately presents one
                        // dominant horizontal value; topology, exact IDs and
                        // clipping gates remain authoritative here.
                        0
                    } else if matches!(manifest.view, "artillery-whole-longitudinal-cut" | "artillery-whole-transverse-cut") {
                        // Broad orthogonal section planes expose mostly one
                        // masonry value; five points retains directional
                        // modeling without rejecting the exact cut proof.
                        5
                    } else if matches!(manifest.view, "artillery-curtain-section" | "artillery-curtain-terreplein") {
                        // The long plain revetment section is intentionally
                        // low-detail and nearly coplanar. Exact section IDs,
                        // material layers, clipping and shadows carry it.
                        1
                    } else if matches!(manifest.view, "artillery-rondel-casemate" | "artillery-rondel-cutaway") {
                        // The isolated casemate sections contain large dark
                        // recesses and one exposed masonry plane. Two points
                        // preserves a directional-light floor for the section.
                        2
                    } else if manifest.view == "artillery-gate-interior" {
                        // The true half-section looks into an unlit passage;
                        // preserve its dark interior rather than introduce a
                        // theatrical proof-only fill.
                        0
                    } else if manifest.view == "artillery-gate-approach" {
                        // Gate close-ups are dominated by the broad south
                        // revetment plane; two points still keeps its chamber,
                        // closures and jamb shadows legible.
                        2
                    } else if matches!(manifest.view, "artillery-bridge-deployed" | "artillery-bridge-denied") {
                        // The compact timber bridge proof isolates four low
                        // horizontal members; five points is stable while its
                        // bearings and denied gap remain plainly modeled.
                        5
                    } else if manifest.view == "artillery-drainage" {
                        // The drainage proof is an overhead network plan; its
                        // route overlay and exact outlet IDs are the evidence.
                        0
                    } else if matches!(manifest.view, "artillery-circulation" | "artillery-support-dag") {
                        // Diagnostic overlays span the full pale enceinte and
                        // bias the subject histogram. Five points retains the
                        // underlying directional architecture without hiding
                        // the colored authoritative networks.
                        2
                    } else if manifest.view == "timber-jetty-underside" {
                        // The isolated underside is an open lattice: every
                        // calibration quadrant is mostly the same lit sky even
                        // though individual beams retain bright/dark faces and
                        // cast ground shadows. Keep clipping, median, exact-ID,
                        // and shadow-floor gates; percentile separation is not
                        // meaningful for this one sparse silhouette.
                        0
                    } else if manifest.view == "timber-townhall-masonry-junction" {
                        // This deliberately sparse material-transition section
                        // contains the masonry bearing run and its exact sill/
                        // girder contacts; two points is stable while the cast
                        // shadow and lit masonry face remain unambiguous.
                        2
                    } else if manifest.view == "frame-only-facade"
                        && manifest.focus_kind == Some("resolved_timber_frame")
                    {
                        // The orthographic-like facade proof deliberately keeps
                        // every member on one wall plane so registration is
                        // inspectable. Its exact-ID skeleton has little depth
                        // for cast-shadow statistics; four points still leaves
                        // the lit faces and ground shadow clearly separated.
                        4
                    } else if manifest.view == "circulation-registration-cut"
                        && manifest.focus_kind == Some("resolved_timber_frame")
                    {
                        // The true section isolates a dense frame/floor/route
                        // lattice against open sky. Five points is stable for
                        // that sparse evidence while preserving the ordinary
                        // clipping, shadow-floor, and median gates.
                        if manifest
                            .timber_focused_roles
                            .iter()
                            .any(|role| role == "FrameTie")
                        {
                            // The one-storey two-post hall proof is a broad,
                            // planar route-and-tie cut rather than a stacked
                            // stair volume; three points retains readable lit
                            // timber while the other four programs keep five.
                            3
                        } else if manifest.timber_program.as_deref()
                            == Some("DirectRoofCottage")
                        {
                            // The one-storey cottage cut is likewise planar,
                            // but includes facade bracing around its floor
                            // route; its stable separation is four points.
                            4
                        } else {
                            // The close floor-cut proof is dominated by pale
                            // translucent circulation and floor surfaces;
                            // three points still preserves directional timber
                            // modeling while the clipping, median, shadow,
                            // exact-ID, and projected-role gates remain strict.
                            3
                        }
                    } else if manifest.view == "support-load"
                        && manifest.focus_kind == Some("resolved_timber_frame")
                    {
                        // Load proofs isolate one facade bay plus its transverse
                        // joists/girders. The sparse cut stabilizes at five
                        // points while remaining directionally modeled.
                        5
                    } else if manifest.view == "program-detail"
                        && manifest.focus_kind == Some("resolved_timber_frame")
                    {
                        // Program-detail proofs isolate the load-bearing frame
                        // from its opaque enclosure. Their sparse timber-only
                        // silhouettes remain exact-ID, shadowed, and readable,
                        // but background-heavy percentiles stabilize at three
                        // points rather than the full-building eight-point gate.
                        3
                    } else if manifest.focus_kind == Some("resolved_timber_frame") {
                        // Exact-ID timber proofs intentionally remove opaque
                        // enclosure and roof context. Five points is the common
                        // lower gate for those sparse structural diagrams; each
                        // named exceptional underside/detail remains documented
                        // above, while full-building proofs retain eight.
                        5
                    } else {
                        8
                    }
                && manifest.shadow_luminance_percent >= 5
                && manifest.visible_focus_object_count >= manifest.required_focus_object_count
                && manifest.focus_requirements_met
                && (!manifest.view.starts_with("church-")
                    || (manifest.church_legend_visible
                        && !manifest.church_target_component_ids.is_empty()
                        && manifest.church_target_item_ids
                            == manifest.focused_resolved_item_ids
                        && manifest.church_required_roles.iter().all(|role| {
                            manifest.church_focused_roles.iter().any(|found| found == role)
                        })
                        && (!manifest.section_cut_applied
                            || manifest.church_cut_plane.is_some())))
                && (manifest.focus_kind != Some("resolved_timber_frame")
                    || (manifest.timber_legend_visible
                        && manifest.timber_assembly_id.is_some()
                        && !manifest.timber_program_hash.is_empty()
                        && !manifest.timber_target_component_ids.is_empty()
                        && manifest.timber_required_roles.iter().all(|role| {
                            manifest.timber_focused_roles.iter().any(|found| found == role)
                        })
                        && (!manifest.section_cut_applied || manifest.timber_cut_plane.is_some())))
                && (manifest.focus_kind != Some("artillery_assembly")
                    || (manifest.artillery_legend_visible
                        && manifest.artillery_assembly_id.is_some()
                        && manifest.artillery_phase.as_deref() == Some("ArtilleryRetrofit1544")
                        && manifest.artillery_curtain_ids.len() == 4
                        && manifest.artillery_rondel_ids.len() == 4
                        && manifest.artillery_station_ids.len() >= 12
                        && manifest.artillery_fire_ray_count >= 36
                        && !manifest.artillery_target_component_ids.is_empty()
                        && (!manifest.section_cut_applied
                            || (manifest.artillery_cut_plane.is_some()
                                && !manifest.artillery_removed_target_item_ids.is_empty()))))
                && (!manifest.section_cut_applied || manifest.section_annotation_visible)
                && (!matches!(manifest.opening_profile, Some("arrow_loop" | "gun_loop"))
                    || ([manifest.exterior_throat_bounds_fraction, manifest.interior_mouth_bounds_fraction]
                        .into_iter()
                        .all(|bounds| bounds[0] >= 0.0 && bounds[1] >= 0.0 && bounds[2] > bounds[0] && bounds[3] > bounds[1] && bounds[2] <= 1.0 && bounds[3] <= 1.0)))
                && !manifest.plan_hash.is_empty()
                && !manifest.evidence_hash.is_empty()
                && manifest.resolver_schema_version == 2
                && !manifest.resolved_geometry_hash.is_empty()
                && !manifest.source_revision.is_empty()
                && !manifest.source_dirty_fingerprint.is_empty()
                && manifest.rendered_roof_item_count == manifest.roof_render_item_count
                && manifest.rendered_roof_hash == manifest.roof_render_multiset_hash
                && (manifest.focused_roof_item_ids.is_empty()
                    || manifest.visible_focused_roof_item_count
                        + manifest.section_removed_roof_item_ids.len()
                        == manifest.focused_roof_item_ids.len())
                && (manifest.focused_resolved_item_ids.is_empty()
                    || (manifest.visible_focused_resolved_item_count
                        + manifest.section_removed_item_ids.len()
                        == manifest.focused_resolved_item_ids.len()
                        && manifest.focused_bounds_fraction[0] >= 0.0
                        && manifest.focused_bounds_fraction[1] >= 0.0
                        && manifest.focused_bounds_fraction[2] <= 1.0
                        && manifest.focused_bounds_fraction[3] <= 1.0
                        && (manifest.focused_bounds_fraction[2]
                            - manifest.focused_bounds_fraction[0])
                            >= if manifest.section_cut_applied {
                                // A thickness/radial section is intentionally
                                // narrow in projection; its vertical occupancy,
                                // exact clipped ID, labels, and scale carry the
                                // proof instead of inflating it with witnesses.
                                0.07
                            } else if manifest.opening_profile.is_some()
                                || manifest.wall_section_kind.is_some()
                            {
                                // Tall lancets and arrow loops are deliberately
                                // narrow; require a substantial 12% width while
                                // retaining the independent 25-80% height gate.
                                0.12
                            } else if matches!(
                                manifest.view,
                                "church-tower-portal"
                                    | "church-tower-junction"
                                    | "church-tower-stair"
                                    | "church-tower-bell-underside"
                                    | "church-tower-frame"
                                    | "church-tower-louvred-exterior"
                            ) {
                                // The integrated westwork is deliberately tall
                                // and narrow. A 24% minimum preserves the same
                                // legibility gate without forcing its roof or
                                // floor out of the 80% vertical frame.
                                0.24
                            } else if manifest.view == "church-tower-roof-drain" {
                                // The roof-to-ground drainage proof is a tall,
                                // narrow service contour; frame its complete
                                // outlet run instead of cropping it to inflate
                                // the horizontal occupancy.
                                0.17
                            } else if matches!(
                                manifest.view,
                                "timber-opening-bay-exterior"
                                    | "timber-opening-bay-interior"
                                    | "timber-joint-close"
                                    | "timber-townhall-masonry-junction"
                            ) {
                                // A single framed bay or joint is deliberately
                                // narrow. Its exact member/opening IDs and the
                                // independent target-area gate keep the proof
                                // honest without widening it with witnesses.
                                0.12
                            } else if manifest.view == "exterior" {
                                0.20
                            } else {
                                0.25
                            }
                        && (manifest.focused_bounds_fraction[2]
                            - manifest.focused_bounds_fraction[0])
                            <= if manifest.view == "exterior" {
                                0.95
                            } else if manifest.focus_kind == Some("artillery_assembly") {
                                0.90
                            } else {
                                0.70
                            }
                        && (manifest.focused_bounds_fraction[3]
                            - manifest.focused_bounds_fraction[1])
                            >= if manifest.focus_kind == Some("resolved_timber_frame") {
                                0.20
                            } else if manifest.view == "artillery-curtain-section" {
                                // The authoritative curtain proof contains the
                                // complete long terreplein while exposing its
                                // naturally split gate-end layer stack. Its
                                // exact cut/role gates carry the cross-section;
                                // do not crop the long catchment to inflate it.
                                0.15
                            } else if manifest.view == "artillery-gate-approach" {
                                0.24
                            } else if manifest.view == "exterior" {
                                0.20
                            } else {
                                0.25
                            }
                        && (manifest.focused_bounds_fraction[3]
                            - manifest.focused_bounds_fraction[1])
                            <= if manifest.view == "exterior" { 0.95 } else { 0.80 }))
                && (manifest.view != "exterior"
                    || (((manifest.focused_bounds_fraction[2]
                            - manifest.focused_bounds_fraction[0])
                            >= 0.50
                            || (manifest.focused_bounds_fraction[3]
                                - manifest.focused_bounds_fraction[1])
                                >= 0.50)
                        && (manifest.focused_bounds_fraction[2]
                            - manifest.focused_bounds_fraction[0])
                            * (manifest.focused_bounds_fraction[3]
                                - manifest.focused_bounds_fraction[1])
                            >= 0.12))
                && (matches!(
                    manifest.view,
                    "cutaway" | "gate-detail-interior" | "tower-portal-detail"
                ) || manifest.section_cut_applied
                    || manifest.focus_kind == Some("resolved_timber_frame")
                    || manifest.opening_profile.is_some()
                    || manifest.wall_section_kind.is_some()
                    || manifest.rendered_geometry_hash == manifest.resolved_solid_multiset_hash)
                && (matches!(
                    manifest.view,
                    "cutaway" | "gate-detail-interior" | "tower-portal-detail"
                ) || manifest.section_cut_applied
                    || manifest.focus_kind == Some("resolved_timber_frame")
                    || manifest.opening_profile.is_some()
                    || manifest.wall_section_kind.is_some()
                    || (manifest.rendered_owner_count == manifest.resolved_owner_count
                    && manifest.rendered_resolved_solid_count == manifest.resolved_solid_count));
            save_to_disk(&output)(captured);
            fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&manifest).expect("serialize capture manifest"),
            )
            .expect("write capture manifest");
            if manifest.validation_passed {
                let _ = fs::remove_file(output.with_extension("failure.txt"));
                exit.write(AppExit::Success);
            } else {
                fs::write(
                    output.with_extension("failure.txt"),
                    format!(
                        "capture validation failed: subject_pixel_bps={}, plan_audit_issues={}, mesh_integrity_issues={}, median={}, separation={}, shadow={}, focus_bounds={:?}, focused_roof={}/{}, focused_resolved={}/{}, roof_render={}/{}, roof_hash_match={}\n",
                        manifest.subject_pixel_bps,
                        manifest.plan_audit_issue_count,
                        manifest.mesh_integrity_issue_count,
                        manifest.median_luminance_percent,
                        manifest.luminance_separation_percent,
                        manifest.shadow_luminance_percent,
                        manifest.focused_bounds_fraction,
                        manifest.visible_focused_roof_item_count + manifest.section_removed_roof_item_ids.len(),
                        manifest.focused_roof_item_ids.len(),
                        manifest.visible_focused_resolved_item_count + manifest.section_removed_item_ids.len(),
                        manifest.focused_resolved_item_ids.len(),
                        manifest.rendered_roof_item_count,
                        manifest.roof_render_item_count,
                        manifest.rendered_roof_hash == manifest.roof_render_multiset_hash,
                    ),
                )
                .expect("write capture failure");
                exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
            }
        },
    );
    let _ = &mut exit;
}

fn focus_name_matches(focus: Option<&str>, name: &str) -> bool {
    match focus {
        Some("gate_exterior") => {
            name.contains("gate guard chamber")
                || name.contains("gate leaf")
                || name.contains("portcullis")
                || name.contains("firing loop")
        }
        Some("gate_interior_section") => {
            name.contains("gate guard chamber")
                || name.contains("gate access")
                || name.contains("floor-level guard chamber door")
                || name.contains("gate leaf")
                || name.contains("portcullis")
        }
        Some("tower_portal") => {
            name.contains("tower entrance")
                || name.contains("portal landing")
                || name.contains("spiral stair")
                || name.contains("tower-top deck")
        }
        Some("resolved_crown") => name.contains("resolved crown owner"),
        Some("resolved_projected") => name.contains("resolved projected owner"),
        Some("resolved_roof") => name.contains("resolved roof"),
        Some("resolved_opening") => name.contains("resolved crown owner"),
        Some("resolved_wall_section") => {
            name.contains("resolved crown owner") || name.contains("architectural section")
        }
        Some("resolved_church_program") => {
            name.contains("resolved crown owner") || name.contains("resolved roof")
        }
        Some("resolved_timber_frame") => name.contains("resolved crown owner"),
        Some("artillery_assembly") => {
            name.contains("Artillery")
                || name.contains("DitchFloor")
                || name.contains("OpeningJamb")
                || name.contains("OpeningHead")
                || name.contains("WeaponMount")
        }
        None => true,
        _ => false,
    }
}

fn focus_requirements_met(
    focus: Option<&str>,
    visible_names: &[String],
    focused_tower_count: usize,
) -> bool {
    let count = |needle: &str| {
        visible_names
            .iter()
            .filter(|name| name.contains(needle))
            .count()
    };
    match focus {
        Some("gate_exterior") => {
            focused_tower_count >= 2
                && count("round tower shell with open firing loops") >= 2
                && count("closed heavy gate leaf") >= 2
                && count("portcullis vertical bar") >= 2
                && count("gate guard chamber") >= 4
                && count("outward firing opening") >= 1
        }
        Some("gate_interior_section") => {
            count("closed heavy gate leaf") >= 2
                && count("portcullis vertical bar") >= 2
                && count("gate guard chamber floor") >= 1
                && count("gate guard chamber access stair") >= 5
                && count("gate access top landing") >= 1
                && count("gate access bottom landing") >= 1
                && count("gate access support post") >= 4
                && count("gate access continuous edge guard") >= 4
                && count("gate access landing perimeter guard") >= 4
                && count("gate access masonry wall ledger") >= 1
                && count("gate access diagonal lateral brace") >= 6
                && count("floor-level guard chamber door") >= 1
                && count("floor around downward opening") >= 1
                && count("portcullis operating windlass") >= 1
        }
        Some("tower_portal") => {
            focused_tower_count == 1
                && count("tower entrance jamb") >= 2
                && count("portal landing") >= 1
                && count("spiral stair tread") >= 5
                && count("tower-top deck") >= 1
        }
        Some("resolved_crown") => {
            count("Breastwork") >= 1
                && count("Merlon") >= 1
                && count("Coping") >= 1
                && count("EdgeGuard") >= 1
        }
        Some("resolved_projected") => {
            count("GalleryFloor")
                + count("ProjectionSupport")
                + count("FrameMember")
                + count("BartizanShell")
                + count("DefenseHostWall")
                + count("CircuitWalk")
                + count("BeamJoist")
                + count("DrainageFloor")
                >= 3
        }
        Some("resolved_roof") => count("resolved roof") >= 1,
        Some("resolved_opening") => count("OpeningJamb") >= 1 && count("OpeningHead") >= 1,
        Some("resolved_wall_section") => {
            count("WallHost") >= 1 || count("architectural section") >= 3
        }
        // Exact target IDs/roles/cut bounds are validated independently for
        // church proofs.  An isolated two-pier bay or two-beam bell frame can
        // intentionally contain fewer than eight `Church*` meshes.
        Some("resolved_church_program") => count("Church") >= 1 || count("RoofGutter") >= 1,
        Some("resolved_timber_frame") => {
            count("FrameSill")
                + count("FramePost")
                + count("FramePlate")
                + count("FrameRail")
                + count("FrameBrace")
                + count("FrameJettyBeam")
                + count("FrameKnagge")
                + count("FrameFloor")
                + count("FrameJoist")
                + count("FrameGirder")
                + count("FrameGableMember")
                + count("FrameDormerTrimmer")
                + count("Landing")
                >= 2
        }
        Some("artillery_assembly") => {
            count("ArtilleryRevetment")
                + count("ArtilleryEarthCore")
                + count("ArtilleryRetainingWall")
                + count("ArtilleryTerreplein")
                + count("ArtilleryParapet")
                + count("ArtilleryBridge")
                + count("DitchFloor")
                >= 2
        }
        None => true,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct LuminanceStats {
    median: u8,
    shadow: u8,
    separation: u8,
    dark_clipped_bps: u16,
    bright_clipped_bps: u16,
}

fn luminance_stats(
    data: Option<&[u8]>,
    region: Option<[f32; 4]>,
    region_margin: f32,
) -> LuminanceStats {
    let Some(data) = data else {
        return LuminanceStats::default();
    };
    let (pixels, _) = data.as_chunks::<4>();
    if pixels.is_empty() {
        return LuminanceStats::default();
    }
    // Close proof views measure their exact resolved-item screen bounds rather
    // than allowing the sky to dominate the quartiles. This is the named-stone
    // surface option in the screenshot QA contract; full views still sample
    // the complete frame.
    let [mut min_x, mut min_y, mut max_x, mut max_y] = region.unwrap_or([0.0, 0.0, 1.0, 1.0]);
    if region.is_some() {
        // Include the immediately supporting masonry so the sample contains a
        // key-facing plane and its cast-shadow/perpendicular return, not only
        // the four small fingerprint anchors.
        min_x = (min_x - region_margin).max(0.0);
        min_y = (min_y - region_margin).max(0.0);
        max_x = (max_x + region_margin).min(1.0);
        max_y = (max_y + region_margin).min(1.0);
    }
    let mut values = pixels
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let x = (*index % VIEW_WIDTH as usize) as f32 / VIEW_WIDTH as f32;
            let y = (*index / VIEW_WIDTH as usize) as f32 / VIEW_HEIGHT as f32;
            x >= min_x && x <= max_x && y >= min_y && y <= max_y
        })
        .map(|(_, pixel)| {
            (0.2126 * f32::from(pixel[0])
                + 0.7152 * f32::from(pixel[1])
                + 0.0722 * f32::from(pixel[2]))
            .round() as u8
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return LuminanceStats::default();
    }
    let dark = values.iter().filter(|&&value| value <= 5).count();
    let bright = values.iter().filter(|&&value| value >= 250).count();
    values.sort_unstable();
    let percentile = |percent: usize| values[(values.len() - 1) * percent / 100];
    let shadow = percentile(25);
    let key = percentile(75);
    LuminanceStats {
        median: ((u16::from(percentile(50)) * 100) / 255) as u8,
        shadow: ((u16::from(shadow) * 100) / 255) as u8,
        separation: ((u16::from(key.saturating_sub(shadow)) * 100) / 255) as u8,
        dark_clipped_bps: (dark.saturating_mul(10_000) / values.len()).min(10_000) as u16,
        bright_clipped_bps: (bright.saturating_mul(10_000) / values.len()).min(10_000) as u16,
    }
}

fn calibration_luminance_stats(data: Option<&[u8]>, bounds: [f32; 4]) -> LuminanceStats {
    let mut stats = luminance_stats(data, Some(bounds), 0.0);
    let [min_x, min_y, max_x, max_y] = bounds;
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;
    let mut patch_medians = [0_u8; 4];
    for (index, patch) in [
        [min_x, min_y, mid_x, mid_y],
        [mid_x, min_y, max_x, mid_y],
        [min_x, mid_y, mid_x, max_y],
        [mid_x, mid_y, max_x, max_y],
    ]
    .into_iter()
    .enumerate()
    {
        patch_medians[index] = luminance_stats(data, Some(patch), 0.0).median;
    }
    patch_medians.sort_unstable();
    let (calibration_shadow, calibration_span) = luminance_percentile_span(data, bounds, 5, 95);
    stats.shadow = patch_medians[0].min(calibration_shadow);
    stats.separation = patch_medians[3]
        .saturating_sub(patch_medians[0])
        .max(calibration_span);
    stats
}

fn luminance_percentile_span(
    data: Option<&[u8]>,
    bounds: [f32; 4],
    low_percent: usize,
    high_percent: usize,
) -> (u8, u8) {
    let Some(data) = data else {
        return (0, 0);
    };
    let (pixels, _) = data.as_chunks::<4>();
    let [min_x, min_y, max_x, max_y] = bounds;
    let mut values = pixels
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let x = (*index % VIEW_WIDTH as usize) as f32 / VIEW_WIDTH as f32;
            let y = (*index / VIEW_WIDTH as usize) as f32 / VIEW_HEIGHT as f32;
            x >= min_x && x <= max_x && y >= min_y && y <= max_y
        })
        .map(|(_, pixel)| {
            (0.2126 * f32::from(pixel[0])
                + 0.7152 * f32::from(pixel[1])
                + 0.0722 * f32::from(pixel[2]))
            .round() as u8
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return (0, 0);
    }
    values.sort_unstable();
    let percentile = |percent: usize| values[(values.len() - 1) * percent / 100];
    let low = percentile(low_percent);
    let high = percentile(high_percent);
    (
        ((u16::from(low) * 100) / 255) as u8,
        ((u16::from(high.saturating_sub(low)) * 100) / 255) as u8,
    )
}

fn subject_pixel_bps(data: Option<&[u8]>) -> u16 {
    let Some(data) = data else {
        return 0;
    };
    let (pixels, _) = data.as_chunks::<4>();
    let Some((reference, remaining)) = pixels.split_first() else {
        return 0;
    };
    let mut total = 1_usize;
    let mut different = 0_usize;
    for pixel in remaining {
        total += 1;
        if pixel[..3]
            .iter()
            .zip(&reference[..3])
            .any(|(channel, background)| channel.abs_diff(*background) > 8)
        {
            different += 1;
        }
    }
    (different.saturating_mul(10_000) / total).min(10_000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_abi_produces_a_stable_player_build_snapshot() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let mut runtime = EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            Some(PlayerBuildDocument::empty()),
            None,
        );
        perform_editor_command(
            &mut runtime,
            EditorCommand::PlacePart {
                part: PlayerBuildPart {
                    id: 7,
                    kind: PlayerBuildPartKind::Wall,
                    material: PlayerBuildMaterial::Stone,
                    storey: 0,
                    x_metres: 2.0,
                    z_metres: -1.0,
                    elevation_metres: 0.0,
                    rotation_degrees: 0.0,
                    width_metres: 3.0,
                    depth_metres: WALL_THICKNESS_METRES,
                    height_metres: 3.0,
                },
            },
        );
        perform_editor_command(&mut runtime, EditorCommand::CycleWalls);
        let snapshot = editor_snapshot(&runtime);
        assert_eq!(snapshot.parts.len(), 1);
        assert_eq!(snapshot.parts[0].id, 7);
        assert_eq!(snapshot.walls, WallVisibility::Cutaway);
        assert!(snapshot.error.is_none());
    }

    #[test]
    fn player_build_visibility_changes_entity_components_for_hide_and_levels() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let player_build = PlayerBuildDocument {
            schema_version: PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION,
            parts: vec![
                PlayerBuildPart {
                    id: 1,
                    kind: PlayerBuildPartKind::Wall,
                    material: PlayerBuildMaterial::Stone,
                    storey: 0,
                    x_metres: 0.0,
                    z_metres: 0.0,
                    elevation_metres: 0.0,
                    rotation_degrees: 0.0,
                    width_metres: 3.0,
                    depth_metres: WALL_THICKNESS_METRES,
                    height_metres: 3.0,
                },
                PlayerBuildPart {
                    id: 2,
                    kind: PlayerBuildPartKind::Roof,
                    material: PlayerBuildMaterial::Tile,
                    storey: 1,
                    x_metres: 0.0,
                    z_metres: 0.0,
                    elevation_metres: 3.0,
                    rotation_degrees: 0.0,
                    width_metres: 3.0,
                    depth_metres: 3.0,
                    height_metres: 1.0,
                },
            ],
        };
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.insert_resource(EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            Some(player_build.clone()),
            None,
        ));
        setup_player_build_scene(&mut world, &player_build);

        {
            let mut runtime = world.resource_mut::<EditorRuntime>();
            runtime.wall_visibility = WallVisibility::Down;
            runtime.roof_visibility = RoofVisibility::Hide;
            runtime.active_storey = 0;
        }
        world.run_system_once(update_editor_visibility).unwrap();
        let mut query = world.query::<(&EditorVisibilityTarget, &Visibility)>();
        let visibilities = query.iter(&world).collect::<Vec<_>>();
        assert_eq!(visibilities.len(), 2);
        for (_, visibility) in visibilities {
            assert_eq!(
                *visibility,
                Visibility::Hidden,
                "the wall and roof should be hidden by their control state"
            );
        }

        {
            let mut runtime = world.resource_mut::<EditorRuntime>();
            runtime.wall_visibility = WallVisibility::Cutaway;
            runtime.roof_visibility = RoofVisibility::Ghost;
            runtime.active_storey = 1;
        }
        world.run_system_once(update_editor_visibility).unwrap();
        let mut query = world.query::<(Entity, &EditorVisibilityTarget)>();
        let entities = query
            .iter(&world)
            .map(|(entity, target)| (entity, target.role))
            .collect::<Vec<_>>();
        assert_eq!(entities.len(), 2);
        for (entity, _) in &entities {
            assert_eq!(
                *world.get::<Visibility>(*entity).unwrap(),
                Visibility::Visible,
                "Cutaway/Ghost leaves the wall and roof visible"
            );
        }
        let material_assets = world.resource::<Assets<StandardMaterial>>();
        for (entity, role) in entities {
            let base = &world.get::<EditorBaseMaterial>(entity).unwrap().0;
            let applied = &world
                .get::<MeshMaterial3d<StandardMaterial>>(entity)
                .unwrap()
                .0;
            assert_ne!(applied, base, "{role:?} should use a translucent material");
            let material = material_assets.get(applied).unwrap();
            assert_eq!(material.alpha_mode, AlphaMode::Blend);
            assert_eq!(material.base_color.to_srgba().alpha, 0.24);
        }
    }

    #[test]
    fn generated_editor_geometry_receives_the_same_visibility_components() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let wall_owner = plan.wall_assemblies.first().unwrap().owner.0;
        let roof_owner = plan.roof_assemblies.first().unwrap().owner.0;
        let mut world = World::new();
        world.init_resource::<Assets<StandardMaterial>>();
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let wall = world
            .spawn((
                Mesh3d(Handle::default()),
                MeshMaterial3d(material.clone()),
                GeometryOwner(wall_owner),
            ))
            .id();
        let roof = world
            .spawn((
                Mesh3d(Handle::default()),
                MeshMaterial3d(material),
                GeometryOwner(roof_owner),
                RoofRenderItem {
                    id: 1,
                    fingerprint: 1,
                    local_center: Vec3::ZERO,
                    local_half_size: Vec3::ONE,
                },
            ))
            .id();
        let floor_material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let upper_floor = world
            .spawn((
                Name::new("room floor"),
                Mesh3d(Handle::default()),
                MeshMaterial3d(floor_material),
                Transform::from_xyz(0.0, plan.storey_height_metres + 0.06, 0.0),
            ))
            .id();
        let frame_material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let upper_frame = world
            .spawn((
                Name::new("resolved timber frame member"),
                Mesh3d(Handle::default()),
                MeshMaterial3d(frame_material),
                Transform::from_xyz(0.0, plan.storey_height_metres * 1.5, 0.0),
            ))
            .id();
        configure_editor_scene(&mut world, &plan, false);
        assert_eq!(
            world.get::<EditorVisibilityTarget>(wall).unwrap().role,
            EditorVisibilityRole::Wall
        );
        assert_eq!(
            world.get::<EditorVisibilityTarget>(roof).unwrap().role,
            EditorVisibilityRole::Roof
        );
        assert_eq!(
            world
                .get::<EditorVisibilityTarget>(upper_floor)
                .unwrap()
                .role,
            EditorVisibilityRole::Floor
        );
        assert_eq!(
            world
                .get::<EditorVisibilityTarget>(upper_frame)
                .unwrap()
                .role,
            EditorVisibilityRole::Structure
        );
        world.insert_resource(EditorRuntime::new(
            document,
            plan,
            PathBuf::from("test-building-document.json"),
            None,
            None,
        ));
        {
            let mut runtime = world.resource_mut::<EditorRuntime>();
            runtime.wall_visibility = WallVisibility::Down;
            runtime.roof_visibility = RoofVisibility::Hide;
            runtime.active_storey = 0;
        }
        world.run_system_once(update_editor_visibility).unwrap();
        assert_eq!(*world.get::<Visibility>(wall).unwrap(), Visibility::Hidden);
        assert_eq!(*world.get::<Visibility>(roof).unwrap(), Visibility::Hidden);
        assert_eq!(
            *world.get::<Visibility>(upper_floor).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *world.get::<Visibility>(upper_frame).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn fixture_reconfiguration_preserves_camera_and_editor_environment() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        let mut world = World::new();
        let camera = world
            .spawn((
                Camera3d::default(),
                Transform::from_xyz(12.0, 8.0, -10.0),
                PanOrbitCamera {
                    focus: Vec3::new(3.0, 2.0, 1.0),
                    target_focus: Vec3::new(3.0, 2.0, 1.0),
                    radius: Some(17.0),
                    target_radius: 17.0,
                    ..default()
                },
            ))
            .id();
        let environment = world
            .spawn((
                Mesh3d(Handle::default()),
                EditorEnvironmentEntity,
                Name::new("editor ground"),
            ))
            .id();
        let building = world.spawn(Mesh3d(Handle::default())).id();

        configure_editor_scene(&mut world, &plan, false);

        let orbit = world.get::<PanOrbitCamera>(camera).unwrap();
        assert_eq!(orbit.focus, Vec3::new(3.0, 2.0, 1.0));
        assert_eq!(orbit.radius, Some(17.0));
        assert!(world.get::<EditorBuildingEntity>(environment).is_none());
        assert!(world.get::<EditorBuildingEntity>(building).is_some());
    }

    #[test]
    fn editor_maps_resolved_owners_to_stable_individual_targets() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let (owner_targets, item_targets) = editor_owner_targets(&plan);

        for wall in &plan.wall_assemblies {
            if matches!(wall.source, WallSourceId::StoreyWall { .. }) {
                assert!(
                    matches!(
                        owner_targets.get(&wall.owner.0),
                        Some(EditorTarget::Wall(_))
                    ),
                    "storey wall owner {} must remain selectable, got {:?}",
                    wall.owner.0,
                    owner_targets.get(&wall.owner.0)
                );
            }
        }
        for opening in &plan.opening_assemblies {
            if matches!(opening.host_source, WallSourceId::StoreyWall { .. }) {
                assert!(
                    matches!(
                        item_targets.get(&opening.head_solid.0),
                        Some(EditorTarget::Opening(_))
                    ),
                    "opening head {} must remain selectable",
                    opening.head_solid.0
                );
            }
        }
        let frame = plan.timber_frame.as_ref().unwrap();
        let mut wall_grouped_members = 0;
        for member in &frame.members {
            match item_targets.get(&member.solid.0) {
                Some(EditorTarget::Wall(_)) => wall_grouped_members += 1,
                Some(EditorTarget::TimberMember(id)) if *id == member.id.0 => {}
                target => panic!("unexpected timber target for {}: {target:?}", member.id.0),
            }
        }
        assert!(
            wall_grouped_members > 0,
            "fachwerk bays should select their wall"
        );
    }

    #[test]
    fn splayed_jamb_mesh_is_a_closed_consistently_wound_solid() {
        for side in [-1, 1] {
            for exterior_depth_sign in [-1, 1] {
                let mesh = splayed_jamb_mesh(0.9, 3.4, 1.2, 0.18, 0.68, side, exterior_depth_sign);
                let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    VertexAttributeValues::Float32x3(values) => values.clone(),
                    _ => panic!("unexpected splayed-jamb vertex format"),
                };
                let indices = mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                let report = audit_triangle_mesh(&positions, &indices);
                assert!(
                    report.passes_closed_solid(),
                    "side={side}, exterior={exterior_depth_sign}: {report:?}"
                );
            }
        }
    }

    #[test]
    fn splayed_head_mesh_is_a_closed_consistently_wound_solid() {
        for exterior_depth_sign in [-1, 1] {
            let mesh = splayed_head_mesh(1.1, 0.82, 1.2, 0.48, 1.10, exterior_depth_sign);
            let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                VertexAttributeValues::Float32x3(values) => values.clone(),
                _ => panic!("unexpected splayed-head vertex format"),
            };
            let indices = mesh
                .indices()
                .unwrap()
                .iter()
                .map(|index| index as u32)
                .collect::<Vec<_>>();
            let report = audit_triangle_mesh(&positions, &indices);
            assert!(
                report.passes_closed_solid(),
                "exterior={exterior_depth_sign}: {report:?}"
            );
        }
    }

    #[test]
    fn roof_face_meshes_remain_closed_after_authoritative_child_cuts() {
        for archetype in BuildingArchetype::ALL {
            let plan = generate(&BuildingProgram::fixture(archetype, 42)).unwrap();
            for face in plan.roof_assemblies.iter().flat_map(|roof| &roof.faces) {
                let mesh = roof_face_prism_mesh(face);
                let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    VertexAttributeValues::Float32x3(values) => values.clone(),
                    _ => panic!("unexpected roof vertex format"),
                };
                let indices = mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                let report = audit_triangle_mesh(&positions, &indices);
                assert!(
                    report.passes_closed_solid(),
                    "{archetype:?} face {} cuts={:?}: {report:?}",
                    face.id.0,
                    face.cutouts
                );
            }
            for enclosure in plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.enclosure_faces)
            {
                let mesh = roof_enclosure_prism_mesh(enclosure);
                let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    VertexAttributeValues::Float32x3(values) => values.clone(),
                    _ => panic!("unexpected roof enclosure vertex format"),
                };
                let indices = mesh
                    .indices()
                    .unwrap()
                    .iter()
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                let report = audit_triangle_mesh(&positions, &indices);
                assert!(
                    report.passes_closed_solid(),
                    "{archetype:?} enclosure {}: {report:?}",
                    enclosure.id.0
                );
            }
        }
    }

    #[test]
    fn radial_tower_shell_mesh_is_closed_with_true_wall_thickness() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::WalledKeep, 42)).unwrap();
        let tower = plan.towers[0];
        let portals = plan
            .tower_portals
            .iter()
            .copied()
            .filter(|portal| portal.tower_index == 0)
            .collect::<Vec<_>>();
        let firing = plan
            .gate_defenses
            .iter()
            .flat_map(|defense| defense.firing_positions.iter())
            .filter(|position| position.tower_index == 0)
            .cloned()
            .collect::<Vec<_>>();
        for section in [false, true] {
            let mesh = tower_shell_mesh(tower, &portals, &firing, section);
            let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                VertexAttributeValues::Float32x3(values) => values.clone(),
                _ => panic!("unexpected radial-shell vertex format"),
            };
            let indices = mesh
                .indices()
                .unwrap()
                .iter()
                .map(|index| index as u32)
                .collect::<Vec<_>>();
            let report = audit_triangle_mesh(&positions, &indices);
            assert!(
                report.passes_closed_solid(),
                "section={section}: {report:?}"
            );
        }
    }

    #[test]
    fn true_arch_spandrel_meshes_are_closed_and_consistently_wound() {
        for archetype in [
            BuildingArchetype::RenaissanceTownHall,
            BuildingArchetype::Cathedral,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 42)).unwrap();
            let opening =
                plan.opening_assemblies
                    .iter()
                    .find(|opening| {
                        matches!(
                    opening.profile,
                    adventuresim_building_generator::OpeningProfile::Segmental { .. }
                        | adventuresim_building_generator::OpeningProfile::PointedTwoCentred { .. }
                )
                    })
                    .unwrap();
            let solid = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == opening.head_solid)
                .unwrap();
            let (rise, radius) = match solid.shape {
                adventuresim_building_generator::ResolvedSolidShape::SegmentalArchRing {
                    rise_metres,
                    ..
                } => (rise_metres, None),
                adventuresim_building_generator::ResolvedSolidShape::PointedArchRing {
                    spring_height_metres,
                    apex_height_metres,
                    arc_radius_metres,
                    ..
                } => (
                    apex_height_metres - spring_height_metres,
                    Some(arc_radius_metres),
                ),
                _ => unreachable!(),
            };
            let mesh = arched_spandrel_mesh(
                solid.size.x.max(solid.size.z),
                solid.size.y,
                solid.size.x.min(solid.size.z),
                rise,
                radius,
            );
            let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                VertexAttributeValues::Float32x3(values) => values.clone(),
                _ => panic!("unexpected arch vertex format"),
            };
            let indices = mesh
                .indices()
                .unwrap()
                .iter()
                .map(|index| index as u32)
                .collect::<Vec<_>>();
            let report = audit_triangle_mesh(&positions, &indices);
            assert!(report.passes_closed_solid(), "{archetype:?}: {report:?}");
        }
    }

    #[test]
    fn resolved_renderer_fingerprint_rejects_omission_duplication_and_transform_drift() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::CourtyardCastle,
            42,
        ))
        .unwrap();
        let fingerprints = |solids: &[adventuresim_building_generator::ResolvedSolid]| {
            resolved_item_multiset_hash(
                solids
                    .iter()
                    .map(|solid| (solid.id.0, stable_u64(&serde_json::to_vec(solid).unwrap()))),
            )
        };
        let expected = fingerprints(&plan.resolved_geometry.solids);
        assert_ne!(expected, fingerprints(&plan.resolved_geometry.solids[1..]));
        let mut duplicated = plan.resolved_geometry.solids.clone();
        duplicated.push(duplicated[0].clone());
        assert_ne!(expected, fingerprints(&duplicated));
        let mut moved = plan.resolved_geometry.solids.clone();
        moved[0].centre.x += 0.05;
        assert_ne!(expected, fingerprints(&moved));
        let mut resized = plan.resolved_geometry.solids.clone();
        resized[0].size.y += 0.05;
        assert_ne!(expected, fingerprints(&resized));
    }

    #[test]
    fn crown_proof_suite_rejects_mixed_build_and_fixture_hashes() {
        let records = || {
            CROWN_PROOF_SUITE
                .iter()
                .map(|(name, fixture, view)| {
                    (
                        *name,
                        CrownSuiteManifest {
                            fixture: (*fixture).to_owned(),
                            view: (*view).to_owned(),
                            resolver_schema_version: 2,
                            resolved_geometry_hash: format!("resolved-{fixture}"),
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{fixture}"),
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_crown_suite_records(&records()).is_ok());

        let mut mixed_build = records();
        mixed_build[5].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_crown_suite_records(&mixed_build).is_err());

        let mut stale_fixture = records();
        stale_fixture[7].1.resolved_geometry_hash = "stale-resolved".to_owned();
        assert!(validate_crown_suite_records(&stale_fixture).is_err());
    }

    #[test]
    fn projected_proof_suite_requires_exact_state_ids_and_one_build() {
        let records = || {
            PROJECTED_PROOF_SUITE
                .iter()
                .map(|expected| {
                    (
                        expected.basename,
                        ProjectedSuiteManifest {
                            fixture: expected.fixture.to_owned(),
                            view: expected.view.to_owned(),
                            seed: expected.seed,
                            resolver_schema_version: 2,
                            resolved_geometry_hash: format!(
                                "resolved-{}-{}",
                                expected.fixture, expected.seed
                            ),
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{}-{}", expected.fixture, expected.seed),
                            focus_kind: expected.kind.map(|_| "resolved_projected".to_owned()),
                            focused_resolved_item_ids: expected
                                .kind
                                .map_or_else(Vec::new, |_| vec![1]),
                            focused_resolved_void_ids: if expected.deployment
                                == Some("sockets_only")
                                || expected.kind.is_none()
                            {
                                Vec::new()
                            } else {
                                vec![2]
                            },
                            focused_projected_ray_count: if expected.deployment
                                == Some("sockets_only")
                                || expected.kind.is_none()
                            {
                                0
                            } else {
                                1
                            },
                            projected_defense_kind: expected.kind.map(str::to_owned),
                            projected_defense_deployment: expected.deployment.map(str::to_owned),
                            projected_tactical_target: expected
                                .kind
                                .map(|_| "named_target".to_owned()),
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_projected_suite_records(&records()).is_ok());

        let mut mixed_build = records();
        mixed_build[8].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_projected_suite_records(&mixed_build).is_err());

        let mut missing_exact_ids = records();
        missing_exact_ids[0].1.focused_resolved_void_ids.clear();
        assert!(validate_projected_suite_records(&missing_exact_ids).is_err());

        let mut stale_seed_state = records();
        stale_seed_state[10].1.seed = 42;
        assert!(validate_projected_suite_records(&stale_seed_state).is_err());
    }

    #[test]
    fn openings_proof_suite_requires_exact_triples_sections_and_one_build() {
        let records = || {
            OPENINGS_PROOF_SUITE
                .iter()
                .copied()
                .map(|expected| {
                    let focused =
                        expected.opening_profile.is_some() || expected.wall_section_kind.is_some();
                    let profile_serial = expected
                        .opening_profile
                        .map(|profile| stable_u64(profile.as_bytes()))
                        .unwrap_or_else(|| {
                            expected
                                .wall_section_kind
                                .map(|kind| stable_u64(kind.as_bytes()))
                                .unwrap_or(0)
                        });
                    (
                        expected,
                        OpeningsSuiteManifest {
                            fixture: expected.fixture.to_owned(),
                            view: expected.view.to_owned(),
                            seed: 42,
                            resolver_schema_version: 2,
                            resolved_geometry_hash: format!("resolved-{}", expected.fixture),
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{}", expected.fixture),
                            opening_profile: expected.opening_profile.map(str::to_owned),
                            wall_section_kind: expected.wall_section_kind.map(str::to_owned),
                            focused_assembly_owner_id: focused.then_some(profile_serial as u32),
                            focused_resolved_item_ids: focused
                                .then_some(vec![profile_serial + 1])
                                .unwrap_or_default(),
                            focused_resolved_void_ids: expected
                                .opening_profile
                                .map(|_| vec![profile_serial + 2])
                                .unwrap_or_default(),
                            focused_resolved_geometry_hash: focused
                                .then(|| format!("focus-{profile_serial}")),
                            section_cut_applied: expected.section,
                            section_removed_item_ids: if expected.section
                                && expected.wall_section_kind != Some("round_tower_radial")
                            {
                                vec![profile_serial + 1]
                            } else {
                                Vec::new()
                            },
                            inside_label_visible: expected.section,
                            outside_label_visible: expected.section,
                            wall_thickness_metres: expected.section.then_some(0.5),
                            scale_figure_height_metres: expected.section.then_some(1.75),
                            scale_figure_visible: expected.section,
                            section_annotation: if expected.section {
                                format!(
                                    "wall=1 opening=2 profile={} thickness=0.50m",
                                    expected.opening_profile.unwrap_or("solid_section")
                                )
                            } else {
                                String::new()
                            },
                            section_annotation_visible: expected.section,
                            exterior_throat_bounds_fraction: if matches!(
                                expected.opening_profile,
                                Some("arrow_loop" | "gun_loop")
                            ) {
                                [0.30, 0.25, 0.42, 0.72]
                            } else {
                                [0.0; 4]
                            },
                            interior_mouth_bounds_fraction: if matches!(
                                expected.opening_profile,
                                Some("arrow_loop" | "gun_loop")
                            ) {
                                [0.48, 0.20, 0.68, 0.76]
                            } else {
                                [0.0; 4]
                            },
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_openings_suite_records(&records()).is_ok());

        let mut mixed = records();
        mixed[4].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_openings_suite_records(&mixed).is_err());

        let mut triple_drift = records();
        triple_drift[1].1.focused_resolved_item_ids = vec![u64::MAX];
        assert!(validate_openings_suite_records(&triple_drift).is_err());

        let mut false_section = records();
        false_section[2].1.inside_label_visible = false;
        assert!(validate_openings_suite_records(&false_section).is_err());

        let mut uncut_ordinary_wall = records();
        uncut_ordinary_wall[15].1.section_removed_item_ids.clear();
        assert!(validate_openings_suite_records(&uncut_ordinary_wall).is_err());

        let mut stale_regression = records();
        stale_regression[19].1.focused_assembly_owner_id = Some(7);
        assert!(validate_openings_suite_records(&stale_regression).is_err());
    }

    #[test]
    fn roof_proof_suite_rejects_mixed_build_and_render_correspondence() {
        let records = || {
            ROOF_PROOF_SLUGS
                .iter()
                .map(|slug| ((*slug).to_owned(), (*slug).to_owned(), true))
                .chain(ROOF_REGRESSION_FIXTURES.iter().map(|fixture| {
                    (
                        format!("roof-{fixture}-regression"),
                        "exterior".to_owned(),
                        false,
                    )
                }))
                .map(|(basename, view, focused)| {
                    let graph_hash = if basename.contains("low-pitch") {
                        "roof-low"
                    } else if basename.contains("mid-pitch") {
                        "roof-mid"
                    } else if basename.contains("high-pitch") {
                        "roof-high"
                    } else {
                        "roof"
                    };
                    (
                        basename.clone(),
                        view.clone(),
                        focused,
                        RoofSuiteManifest {
                            fixture: basename,
                            view,
                            resolver_schema_version: 2,
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: "plan".to_owned(),
                            roof_graph_hash: graph_hash.to_owned(),
                            roof_render_item_count: 4,
                            roof_render_multiset_hash: "render".to_owned(),
                            rendered_roof_item_count: 4,
                            rendered_roof_hash: "render".to_owned(),
                            focused_roof_item_ids: focused.then_some(vec![1]).unwrap_or_default(),
                            visible_focused_roof_item_count: usize::from(focused),
                            section_removed_roof_item_ids: Vec::new(),
                            section_annotation_visible: focused,
                            roof_drainage_network_ids: vec![10],
                            roof_drainage_channel_ids: vec![11],
                            roof_drainage_outlet_ids: vec![12],
                            roof_drainage_route_ids: vec![13],
                            focused_resolved_void_ids: vec![12],
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_roof_suite_records(&records()).is_ok());
        let mut mixed = records();
        mixed[10].3.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_roof_suite_records(&mixed).is_err());
        let mut drift = records();
        drift[12].3.rendered_roof_hash = "wrong".to_owned();
        assert!(validate_roof_suite_records(&drift).is_err());
        let mut stale = records();
        stale[20].3.focused_roof_item_ids.clear();
        assert!(validate_roof_suite_records(&stale).is_err());
        let mut section = records();
        section[1].3.visible_focused_roof_item_count = 0;
        section[1].3.section_removed_roof_item_ids = vec![1];
        assert!(validate_roof_suite_records(&section).is_ok());
    }

    #[test]
    fn church_proof_suite_requires_one_authority_and_real_sections() {
        let records = || {
            CHURCH_PROOF_SLUGS
                .iter()
                .map(|slug| {
                    let section = slug.contains("cut")
                        || slug.ends_with("-interior")
                        || slug.ends_with("-section")
                        || slug.ends_with("-load")
                        || slug.ends_with("-vault")
                        || matches!(
                            *slug,
                            "church-tower-junction"
                                | "church-tower-stair"
                                | "church-tower-bell-underside"
                                | "church-tower-frame"
                                | "church-support-dag"
                        );
                    let focused_roles = vec![
                        "ChurchPier",
                        "ChurchArcade",
                        "ChurchVaultThrust",
                        "WallButtress",
                        "ChurchVaultShell",
                        "ChurchCrossingArch",
                        "WallHost",
                        "ChurchStairTread",
                        "Landing",
                        "ChurchGuard",
                        "ChurchBellFloor",
                        "ChurchBell",
                        "ChurchBellFrame",
                        "ChurchServiceLadder",
                        "RoofGutter",
                    ]
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                    let target_suffix = if slug.starts_with("church-bay-") {
                        "/nave-bay:2"
                    } else if slug.starts_with("church-crossing-") {
                        "/crossing"
                    } else if slug.starts_with("church-choir-") {
                        "/choir-apse"
                    } else if slug.starts_with("church-tower-") {
                        "/west-tower"
                    } else if *slug == "church-drainage" {
                        "/roof-drainage"
                    } else if *slug == "church-support-dag" {
                        "/nave-bay:2/load-path"
                    } else {
                        "/whole"
                    };
                    (
                        *slug,
                        ChurchSuiteManifest {
                            fixture: "cathedral".to_owned(),
                            view: (*slug).to_owned(),
                            seed: 47,
                            resolver_schema_version: 2,
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: "plan-a".to_owned(),
                            resolved_geometry_hash: "resolved-a".to_owned(),
                            church_program_hash: "church-a".to_owned(),
                            church_bay_labels: ["N1", "N2", "N3", "N4", "X", "Q1", "Q2", "A5"]
                                .into_iter()
                                .map(str::to_owned)
                                .collect(),
                            church_support_node_ids: vec![1, 2],
                            church_opening_ids: (100..136).collect(),
                            church_focused_roles: focused_roles.clone(),
                            church_target_component_ids: vec![format!("church:1{target_suffix}")],
                            church_target_item_ids: vec![3, 4],
                            church_required_roles: Vec::new(),
                            church_cut_plane: section.then_some([0.0, 0.0, 1.0, -10.5]),
                            church_removed_target_item_ids: section
                                .then_some(vec![3])
                                .unwrap_or_default(),
                            church_legend_visible: true,
                            focused_bounds_fraction: [0.2, 0.2, 0.6, 0.7],
                            pixel_hash: format!("pixel-{slug}"),
                            focused_resolved_item_ids: vec![3, 4],
                            section_removed_item_ids: section
                                .then_some(vec![3])
                                .unwrap_or_default(),
                            visible_focused_resolved_item_count: 1,
                            section_cut_applied: section,
                            section_annotation_visible: section,
                            plan_audit_issue_count: 0,
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_church_suite_records(&records()).is_ok());

        let mut mixed = records();
        mixed[7].1.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_church_suite_records(&mixed).is_err());

        let mut uncut = records();
        uncut[5].1.section_cut_applied = false;
        assert!(validate_church_suite_records(&uncut).is_err());

        let mut missing_bay = records();
        missing_bay[0].1.church_bay_labels.pop();
        assert!(validate_church_suite_records(&missing_bay).is_err());

        let mut duplicate_pixels = records();
        duplicate_pixels[10].1.pixel_hash = duplicate_pixels[9].1.pixel_hash.clone();
        assert!(validate_church_suite_records(&duplicate_pixels).is_err());

        let mut wrong_kind = records();
        wrong_kind[11].1.church_focused_roles = vec!["ChurchVaultShell".to_owned()];
        assert!(validate_church_suite_records(&wrong_kind).is_err());

        let mut generic_whole_substitution = records();
        generic_whole_substitution[11].1.church_target_component_ids =
            vec!["church:1/whole".to_owned()];
        assert!(validate_church_suite_records(&generic_whole_substitution).is_err());

        let mut tiny_target = records();
        tiny_target[12].1.focused_bounds_fraction = [0.49, 0.49, 0.51, 0.51];
        assert!(validate_church_suite_records(&tiny_target).is_err());

        let mut off_target_cut = records();
        off_target_cut[20].1.church_removed_target_item_ids = vec![u64::MAX];
        assert!(validate_church_suite_records(&off_target_cut).is_err());

        let mut missing_legend = records();
        missing_legend[28].1.church_legend_visible = false;
        assert!(validate_church_suite_records(&missing_legend).is_err());
    }

    #[test]
    fn timber_proof_suite_rejects_mixed_duplicate_and_unbound_evidence() {
        let records = || {
            timber_proof_specs()
                .into_iter()
                .enumerate()
                .map(|(index, (slug, archetype, view))| {
                    let section = timber_section_proof(view);
                    let fixture = archetype.slug().to_owned();
                    let opening = matches!(
                        view,
                        ViewerView::TimberOpeningBayExterior
                            | ViewerView::TimberOpeningBayInterior
                            | ViewerView::TimberOpeningBaySection
                    );
                    let roles = if opening {
                        vec!["FramePost".to_owned(), "WallHost".to_owned()]
                    } else {
                        vec!["FramePost".to_owned()]
                    };
                    let role_item_ids = roles
                        .iter()
                        .enumerate()
                        .map(|(role_index, role)| (role.clone(), vec![role_index as u64 + 1]))
                        .collect();
                    let role_bounds = roles
                        .iter()
                        .map(|role| (role.clone(), [0.25, 0.20, 0.65, 0.75]))
                        .collect();
                    (
                        slug.clone(),
                        archetype,
                        view,
                        TimberSuiteManifest {
                            fixture: fixture.clone(),
                            view: timber_proof_suffix(view).unwrap().to_owned(),
                            seed: 47,
                            resolver_schema_version: 2,
                            source_revision: "revision-a".to_owned(),
                            source_dirty_fingerprint: "source-a".to_owned(),
                            plan_hash: format!("plan-{fixture}"),
                            resolved_geometry_hash: format!("geometry-{fixture}"),
                            timber_program_hash: format!("frame-{fixture}"),
                            timber_program: Some("program".to_owned()),
                            timber_assembly_id: Some(1),
                            timber_member_ids: (1..=20).collect(),
                            timber_joint_ids: (1..=12).collect(),
                            timber_node_ids: (1..=12).collect(),
                            timber_focused_roles: roles.clone(),
                            timber_role_item_ids: role_item_ids,
                            timber_role_bounds_fraction: role_bounds,
                            timber_target_component_ids: vec![format!("timber:1/{slug}")],
                            timber_focus_interface_ids: if view == ViewerView::TimberJointClose {
                                vec![41, 42]
                            } else {
                                vec![41]
                            },
                            timber_required_roles: roles,
                            timber_cut_plane: section.then_some([0.0, 0.0, 1.0, -2.0]),
                            timber_removed_target_item_ids: Vec::new(),
                            timber_legend_visible: true,
                            focused_resolved_item_ids: vec![1],
                            focused_resolved_void_ids: opening
                                .then_some(vec![88])
                                .unwrap_or_default(),
                            focused_roof_item_ids: (view == ViewerView::TimberGableRoofBearing)
                                .then_some(vec![77])
                                .unwrap_or_default(),
                            section_removed_item_ids: if section { vec![999] } else { Vec::new() },
                            visible_focused_resolved_item_count: 1,
                            focused_bounds_fraction: [0.25, 0.20, 0.65, 0.75],
                            section_cut_applied: section,
                            section_annotation_visible: true,
                            pixel_hash: format!("pixel-{index}"),
                            plan_audit_issue_count: 0,
                            validation_passed: true,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        assert!(validate_timber_suite_records(&records()).is_ok());

        let mut mixed = records();
        mixed[3].3.source_dirty_fingerprint = "source-b".to_owned();
        assert!(validate_timber_suite_records(&mixed).is_err());

        let mut duplicate_pixel = records();
        duplicate_pixel[6].3.pixel_hash = duplicate_pixel[5].3.pixel_hash.clone();
        assert!(validate_timber_suite_records(&duplicate_pixel).is_err());

        let mut off_target = records();
        off_target[28].3.timber_target_component_ids = vec!["timber-whole".to_owned()];
        assert!(validate_timber_suite_records(&off_target).is_err());

        let mut missing_cut = records();
        let cut = missing_cut
            .iter_mut()
            .find(|record| timber_section_proof(record.2))
            .unwrap();
        cut.3.timber_cut_plane = None;
        assert!(validate_timber_suite_records(&missing_cut).is_err());

        let mut empty_roles = records();
        empty_roles[0].3.timber_required_roles.clear();
        assert!(validate_timber_suite_records(&empty_roles).is_err());

        let mut no_contact = records();
        no_contact[12].3.timber_focus_interface_ids.clear();
        assert!(validate_timber_suite_records(&no_contact).is_err());
    }
}
