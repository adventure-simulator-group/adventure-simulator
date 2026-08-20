use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy::math::{Quat, Vec2, Vec3};
use geo::{Area, BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use serde::{Deserialize, Serialize};

use crate::{
    BattlementKind, BuildingArchetype, BuildingPlan, CROWN_DRAIN_CHANNEL_WIDTH_METRES,
    CrownJunctionKind, CrownPath, DefensiveCircuit, DefensiveJunction, DefensiveJunctionKind,
    Direction, GateClosureKind, ProjectedDefenseDeployment, ProjectedDefenseKind,
    ProjectedDefenseMaterial, ProjectedDefensePath, ProjectedDefensePhase, ProjectedDefenseTarget,
    ResolvedItemId, ResolvedSolid, RoofEdgeKind, RoofPiece, SolidRole, Stair, StructuralNodeId,
    SurfaceRole, TowerPortalKind, VoidRole, WALL_THICKNESS_METRES, WallWalk,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditIssue {
    pub code: &'static str,
    pub message: String,
}

pub fn audit_plan(plan: &BuildingPlan) -> Vec<AuditIssue> {
    let mut issues = Vec::new();
    let dimensions = plan.dimensions_metres();
    let centre = dimensions * 0.5;

    for (index, run) in plan.battlements.iter().enumerate() {
        let midpoint = (run.start + run.end) * 0.5;
        if (midpoint - centre).dot(direction_vector(run.outward)) <= 0.01 {
            issues.push(issue(
                "battlement_faces_inward",
                format!("battlement run {index} faces the protected interior"),
            ));
        }
        if run.kind != BattlementKind::Breteche
            && !run_supported(plan, run.start, run.end, run.base_height_metres)
        {
            issues.push(issue(
                "unsupported_battlement",
                format!("battlement run {index} has no wall beneath it"),
            ));
        }
        if run.kind != BattlementKind::Breteche
            && !plan.wall_walks.iter().any(|walk| match walk {
                WallWalk::Linear {
                    start,
                    end,
                    elevation_metres,
                    ..
                } => {
                    same_run(*start, *end, run.start, run.end)
                        && close(*elevation_metres, run.base_height_metres)
                }
                WallWalk::Round { .. } | WallWalk::RectangularDeck { .. } => false,
            })
        {
            issues.push(issue(
                "missing_wall_walk",
                format!("battlement run {index} has no fighting platform"),
            ));
        }
    }

    for (index, walk) in plan.wall_walks.iter().enumerate() {
        if let WallWalk::Linear { width_metres, .. } = walk
            && *width_metres < 0.9
        {
            issues.push(issue(
                "wall_walk_too_narrow",
                format!("wall walk {index} is only {width_metres:.2} m wide"),
            ));
        }
        if let WallWalk::RectangularDeck {
            stairwell_centre,
            stairwell_size,
            elevation_metres,
            ..
        } = walk
        {
            if stairwell_size.min_element() < 0.9 {
                issues.push(issue(
                    "stairwell_too_small",
                    format!("rectangular deck {index} has an undersized stairwell"),
                ));
            }
            if !plan.stairs.iter().any(|stair| {
                matches!(
                    stair,
                    Stair::Spiral { centre, base_height_metres, rise_metres, .. }
                        if close_vec(*centre, *stairwell_centre)
                            && close(*base_height_metres + *rise_metres, *elevation_metres)
                )
            }) {
                issues.push(issue(
                    "inaccessible_rectangular_deck",
                    format!("rectangular deck {index} has no stair reaching its stairwell"),
                ));
            }
        }
    }

    audit_defensive_circuit(plan, &mut issues);
    audit_resolved_geometry(plan, &mut issues);
    audit_wall_opening_assemblies(plan, &mut issues);
    audit_crowns(plan, &mut issues);
    audit_projected_defenses(plan, &mut issues);
    audit_roof_assemblies(plan, &mut issues);
    audit_church_assembly(plan, &mut issues);
    audit_timber_frame(plan, &mut issues);
    audit_artillery_castle(plan, &mut issues);

    if matches!(
        plan.archetype,
        BuildingArchetype::CourtyardCastle | BuildingArchetype::WalledKeep
    ) {
        audit_walk_roof_clearance(plan, &mut issues);
    }

    for (index, tower) in plan.towers.iter().enumerate() {
        if tower.battlement.is_some() && tower.roof.is_some() {
            issues.push(issue(
                "obstructed_tower_deck",
                format!("tower {index} has both a roof and an open fighting crown"),
            ));
        }
        let deck = plan.wall_walks.iter().any(|walk| {
            matches!(
                walk,
                WallWalk::Round { centre, elevation_metres, .. }
                    if close_vec(*centre, tower.centre_metres())
                        && close(*elevation_metres, tower.wall_height_metres)
            )
        });
        let access = plan.stairs.iter().any(|stair| {
            matches!(
                stair,
                Stair::Spiral { centre, base_height_metres, rise_metres, .. }
                    if close_vec(*centre, tower.centre_metres())
                        && close(*base_height_metres + *rise_metres, tower.wall_height_metres)
            )
        });
        if tower.battlement.is_some() && !deck {
            issues.push(issue(
                "missing_tower_deck",
                format!("battlemented tower {index} has no annular fighting deck"),
            ));
        }
        if tower.battlement.is_some() && !access {
            issues.push(issue(
                "inaccessible_tower_deck",
                format!("battlemented tower {index} has no stair reaching its deck"),
            ));
        }
    }

    for (index, wall) in plan.curtain_walls.iter().enumerate() {
        if !plan.battlements.iter().any(|run| {
            same_run(run.start, run.end, wall.start, wall.end)
                && close(run.base_height_metres, wall.height_metres)
                && run.outward == wall.outward
        }) {
            issues.push(issue(
                "unprotected_curtain_wall",
                format!("curtain wall {index} has no outward-facing parapet"),
            ));
        }
    }
    audit_fortified_profile(plan, &mut issues);
    audit_gatehouse_assemblies(plan, &mut issues);
    issues
}

fn audit_artillery_castle(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    let inherited = matches!(
        plan.archetype,
        BuildingArchetype::CastleGatehouse
            | BuildingArchetype::CourtyardCastle
            | BuildingArchetype::WalledKeep
    );
    if inherited {
        if plan.castle_phase != Some(crate::CastleConstructionPhase::InheritedMedieval)
            || plan.artillery_castle.is_some()
        {
            issues.push(issue(
                "artillery_phase_contamination",
                "an inherited medieval fixture was relabelled or retrofitted by the artillery assembly".to_owned(),
            ));
        }
        return;
    }
    if plan.archetype != BuildingArchetype::ArtilleryRondelCastle {
        return;
    }
    let Some(castle) = &plan.artillery_castle else {
        issues.push(issue(
            "missing_artillery_castle",
            "the artillery fixture has no authoritative assembly".to_owned(),
        ));
        return;
    };
    if castle.phase != crate::CastleConstructionPhase::ArtilleryRetrofit1544
        || plan.castle_phase != Some(crate::CastleConstructionPhase::ArtilleryRetrofit1544)
        || castle
            .clear_court_size_metres
            .distance(Vec2::new(36.0, 30.0))
            > 0.01
        || !(5.5..=6.5).contains(&castle.crown_elevation_metres)
    {
        issues.push(issue(
            "invalid_artillery_trace",
            "artillery trace, phase, court, or crown datum drifted".to_owned(),
        ));
    }
    let trace = castle.trace.map(crate::GridPoint::metres);
    let cardinal_rectangle = trace[0].y == trace[1].y
        && trace[1].x == trace[2].x
        && trace[2].y == trace[3].y
        && trace[3].x == trace[0].x;
    if !cardinal_rectangle || castle.curtains.len() != 4 || castle.rondels.len() != 4 {
        issues.push(issue(
            "invalid_artillery_trace",
            "artillery enceinte is not one four-corner cardinal rectangle".to_owned(),
        ));
    }
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<std::collections::HashMap<_, _>>();
    let voids = plan
        .resolved_geometry
        .voids
        .iter()
        .map(|void| (void.id, void))
        .collect::<std::collections::HashMap<_, _>>();
    let openings = plan
        .opening_assemblies
        .iter()
        .map(|opening| (opening.id, opening))
        .collect::<std::collections::HashMap<_, _>>();
    let valid_artillery_drain = |catchment_id: crate::ResolvedItemId,
                                 route_id: crate::ResolvedItemId,
                                 walk_id: crate::ResolvedItemId| {
        plan.resolved_geometry
            .drainage_catchments
            .iter()
            .find(|catchment| catchment.id == catchment_id)
            .is_some_and(|catchment| {
                catchment.walk_solid == walk_id
                    && catchment.outlet_route == route_id
                    && catchment.inner_elevation_metres > catchment.outer_elevation_metres + 0.025
                    && !catchment.toe_channel_solids.is_empty()
                    && catchment.toe_channel_solids.iter().all(|id| {
                        solids.get(id).is_some_and(|solid| {
                            solid.role == SolidRole::DrainageFloor
                                && solid.longfall_radians.abs() >= 0.004
                        })
                    })
                    && plan.resolved_geometry.drainage_routes.iter().any(|route| {
                        route.id == route_id
                            && route.owner == catchment.owner
                            && route.inlet.y > route.outlet.y + 0.04
                            && voids.get(&route.outlet_void).is_some_and(|void| {
                                void.role == VoidRole::Drain && void.owner == route.owner
                            })
                    })
            })
    };
    for curtain in &castle.curtains {
        let layers = curtain
            .revetment_solids
            .iter()
            .chain(&curtain.earth_solids)
            .chain(&curtain.retaining_solids)
            .all(|id| solids.contains_key(id));
        let roles = curtain.revetment_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|s| s.role == SolidRole::ArtilleryRevetment)
        }) && curtain.earth_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|s| s.role == SolidRole::ArtilleryEarthCore)
        }) && curtain.retaining_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|s| s.role == SolidRole::ArtilleryRetainingWall)
        });
        if curtain.total_depth.metres() < 4.0
            || curtain.height_metres < 5.5
            || curtain.earth_solids.is_empty()
            || !layers
            || !roles
            || solids
                .get(&curtain.parapet_solid)
                .is_none_or(|s| s.role != SolidRole::ArtilleryParapet || s.size.y < 1.25)
            || surfaces
                .get(&curtain.route_surface)
                .is_none_or(|s| s.role != SurfaceRole::ArtilleryRoute)
            || solids.get(&curtain.terreplein_solid).is_none_or(|solid| {
                solid.role != SolidRole::ArtilleryTerreplein || solid.crossfall_radians.abs() < 0.02
            })
            || !valid_artillery_drain(
                curtain.drainage_catchment,
                curtain.drainage_route,
                curtain.terreplein_solid,
            )
        {
            issues.push(issue(
                "invalid_artillery_curtain",
                format!(
                    "artillery curtain {} lacks its thick earth-backed section or route",
                    curtain.id.0
                ),
            ));
        }
    }
    for rondel in &castle.rondels {
        let station_count = castle
            .stations
            .iter()
            .filter(|station| station.rondel == rondel.id)
            .count();
        let earth_volume = rondel
            .earth_solids
            .iter()
            .filter_map(|id| solids.get(id))
            .map(|solid| match solid.shape {
                crate::ResolvedSolidShape::AnnularSectorPrism {
                    inner_radius_metres,
                    outer_radius_metres,
                    start_angle_radians,
                    end_angle_radians,
                    ..
                } => {
                    0.5 * (outer_radius_metres.powi(2) - inner_radius_metres.powi(2))
                        * (end_angle_radians - start_angle_radians)
                        * solid.size.y
                }
                _ => 0.0,
            })
            .sum::<f32>();
        let earth_samples = (0..32)
            .map(|sample| {
                let angle = (sample as f32 + 0.5) * std::f32::consts::TAU / 32.0;
                let point = Vec3::new(
                    rondel.anchor.metres().x + 4.15 * angle.cos(),
                    2.4,
                    rondel.anchor.metres().y + 4.15 * angle.sin(),
                );
                rondel.earth_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_some_and(|solid| resolved_solid_contains_point(solid, point, 0.002))
                })
            })
            .collect::<Vec<_>>();
        let max_earth_gap = (0..64)
            .fold((0usize, 0usize), |(longest, current), index| {
                let next = if earth_samples[index % 32] {
                    0
                } else {
                    current + 1
                };
                (longest.max(next), next)
            })
            .0;
        let parapet_samples = (0..64)
            .map(|sample| {
                let angle = (sample as f32 + 0.5) * std::f32::consts::TAU / 64.0;
                let point = Vec3::new(
                    rondel.anchor.metres().x + 5.40 * angle.cos(),
                    6.45,
                    rondel.anchor.metres().y + 5.40 * angle.sin(),
                );
                rondel.parapet_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_some_and(|solid| resolved_solid_contains_point(solid, point, 0.002))
                })
            })
            .collect::<Vec<_>>();
        let max_parapet_gap = (0..128)
            .fold((0usize, 0usize), |(longest, current), index| {
                let next = if parapet_samples[index % 64] {
                    0
                } else {
                    current + 1
                };
                (longest.max(next), next)
            })
            .0;
        let guard_samples = (0..64)
            .map(|sample| {
                let angle = (sample as f32 + 0.5) * std::f32::consts::TAU / 64.0;
                let point = Vec3::new(
                    rondel.anchor.metres().x + 1.365 * angle.cos(),
                    6.32,
                    rondel.anchor.metres().y + 1.365 * angle.sin(),
                );
                rondel.stair_guard_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_some_and(|solid| resolved_solid_contains_point(solid, point, 0.002))
                })
            })
            .collect::<Vec<_>>();
        let max_guard_gap = (0..128)
            .fold((0usize, 0usize), |(longest, current), index| {
                let next = if guard_samples[index % 64] {
                    0
                } else {
                    current + 1
                };
                (longest.max(next), next)
            })
            .0;
        let guard_opening_chord =
            2.0 * 1.365 * (max_guard_gap as f32 * std::f32::consts::TAU / 64.0 * 0.5).sin();
        let arrival = Vec2::new(
            if rondel.id.0 % 2 == 0 { 1.0 } else { -1.0 },
            if rondel.id.0 < 2 { 1.0 } else { -1.0 },
        )
        .normalize();
        let arrival_clear = !rondel.stair_guard_solids.iter().any(|id| {
            solids.get(id).is_some_and(|solid| {
                resolved_solid_contains_point(
                    solid,
                    Vec3::new(
                        rondel.anchor.metres().x + arrival.x * 1.365,
                        6.32,
                        rondel.anchor.metres().y + arrival.y * 1.365,
                    ),
                    0.002,
                )
            })
        });
        let mut earth_forbidden = Vec::new();
        if let Some(void) = voids.get(&rondel.casemate_void) {
            earth_forbidden.push((void.bounds.min, void.bounds.max));
        }
        for station in castle.stations.iter().filter(|station| {
            station.rondel == rondel.id
                && station.level == crate::ArtilleryStationLevel::LowerCasemate
        }) {
            earth_forbidden.push((station.recoil_envelope.min, station.recoil_envelope.max));
            if let Some(surface) = surfaces.get(&station.stance_surface) {
                earth_forbidden.push((
                    surface.bounds.min - Vec3::new(0.02, 0.0, 0.02),
                    surface.bounds.max + Vec3::new(0.02, 1.90, 0.02),
                ));
            }
            if let Some(mount) = solids.get(&station.mount_solid) {
                earth_forbidden.push(resolved_solid_bounds(mount));
            }
            if let Some(opening) = openings.get(&station.opening)
                && let Some(void) = voids.get(&opening.void_id)
            {
                earth_forbidden.push((void.bounds.min, void.bounds.max));
            }
            if let Some(vent) = station.smoke_vent.and_then(|id| voids.get(&id)) {
                earth_forbidden.push((vent.bounds.min, vent.bounds.max));
            }
        }
        let earth_intrudes = rondel
            .earth_solids
            .iter()
            .filter_map(|id| solids.get(id))
            .any(|earth| {
                earth_forbidden
                    .iter()
                    .any(|bounds| resolved_shape_overlaps_bounds(earth, *bounds, 0.006))
            });
        let route_intrudes = castle
            .route_edges
            .iter()
            .flat_map(|edge| edge.sweep_path.iter())
            .filter(|point| {
                Vec2::new(
                    point.x - rondel.anchor.metres().x,
                    point.z - rondel.anchor.metres().y,
                )
                .length()
                    < 6.2
            })
            .any(|point| {
                [0.25_f32, 1.0, 1.85].into_iter().any(|height| {
                    rondel
                        .earth_solids
                        .iter()
                        .filter_map(|id| solids.get(id))
                        .any(|earth| {
                            resolved_solid_contains_point(earth, *point + Vec3::Y * height, 0.006)
                        })
                })
            });
        if !(11.0..=13.0).contains(&rondel.diameter.metres())
            || rondel.shell.metres() < 1.0
            || rondel.curtain_bonds.len()!=2
            || rondel.curtain_bonds.iter().any(|id|!plan.resolved_geometry.junction_bonds.iter().any(|bond|bond.id==*id))
            || station_count < 3
            || rondel.earth_solids.is_empty()
            || rondel.earth_solids.iter().any(|id| {
                solids.get(id).is_none_or(|solid| solid.role != SolidRole::ArtilleryEarthCore
                    || !matches!(solid.shape,crate::ResolvedSolidShape::AnnularSectorPrism{inner_radius_metres,outer_radius_metres,..} if inner_radius_metres>=3.5&&outer_radius_metres-inner_radius_metres>=1.1))
            })
            || earth_volume < 100.0 || earth_samples.iter().filter(|covered|**covered).count()<18 || max_earth_gap>6 || earth_intrudes || route_intrudes
            || rondel.parapet_solids.len()<18
            || rondel.parapet_solids.iter().any(|id|solids.get(id).is_none_or(|solid|solid.role!=SolidRole::ArtilleryParapet||solid.size.y<1.25||!matches!(solid.shape,crate::ResolvedSolidShape::AnnularSectorPrism{inner_radius_metres,outer_radius_metres,..} if outer_radius_metres-inner_radius_metres>=0.80)))
            || parapet_samples.iter().filter(|covered|**covered).count()<36 || max_parapet_gap>12
            || rondel.stair_guard_solids.len()<20
            || rondel.stair_guard_solids.iter().any(|id|solids.get(id).is_none_or(|solid|solid.role!=SolidRole::ArtilleryStairGuard||solid.size.y<0.90||!matches!(solid.shape,crate::ResolvedSolidShape::AnnularSectorPrism{inner_radius_metres,outer_radius_metres,..} if inner_radius_metres>=1.25&&outer_radius_metres-inner_radius_metres>=0.10)))
            || guard_samples.iter().filter(|covered|**covered).count()<54
            || max_guard_gap>10
            || !(0.90..=1.35).contains(&guard_opening_chord)
            || !arrival_clear
            || voids.get(&rondel.casemate_void).is_none_or(|v|v.role!=VoidRole::ArtilleryCasemate)
            || solids.get(&rondel.casemate_roof).is_none_or(|s|s.centre.y-s.size.y*0.5 < 2.1)
            || solids.get(&rondel.terreplein_solid).is_none_or(|solid| {
                !matches!(
                    solid.shape,
                    crate::ResolvedSolidShape::AnnularPrism {
                        inner_top_offset_metres,
                        outer_top_offset_metres,
                        drainage_outlet_count: 4,
                        circumferential_fall_metres,
                        ..
                    } if inner_top_offset_metres > outer_top_offset_metres + 0.05
                        && circumferential_fall_metres >= 0.02
                )
            })
            || rondel.drainage_routes.len() != 4
            || rondel.drainage_routes.iter().any(|route_id| {
                !plan.resolved_geometry.drainage_routes.iter().any(|route| {
                    route.id == *route_id
                        && route.inlet.y > route.outlet.y + 0.04
                        && voids.contains_key(&route.outlet_void)
                })
            })
        {
            issues.push(issue("invalid_artillery_rondel", format!("rondel {} lacks bonded two-level artillery authority (earth_volume={earth_volume:.1}, earth_samples={}, earth_gap={max_earth_gap}, earth_intrudes={earth_intrudes}, route_intrudes={route_intrudes}, parapet_count={}, parapet_samples={}, parapet_gap={max_parapet_gap}, guard_samples={}, guard_gap={max_guard_gap}, arrival_clear={arrival_clear})",rondel.id.0,earth_samples.iter().filter(|covered|**covered).count(),rondel.parapet_solids.len(),parapet_samples.iter().filter(|covered|**covered).count(),guard_samples.iter().filter(|covered|**covered).count())));
        }
    }
    let artillery_targets = castle
        .defense_targets
        .iter()
        .map(|target| (target.id, target))
        .collect::<std::collections::HashMap<_, _>>();
    if artillery_targets.len() != castle.defense_targets.len() {
        issues.push(issue(
            "incomplete_artillery_coverage",
            "duplicate artillery tactical target id".to_owned(),
        ));
    }
    let mut target_kinds = std::collections::HashSet::new();
    for station in &castle.stations {
        let opening = openings.get(&station.opening);
        let stance = surfaces.get(&station.stance_surface);
        let recoil = station.recoil_envelope.max - station.recoil_envelope.min;
        for ray in &station.rays {
            target_kinds.insert(ray.target_kind);
        }
        let station_geometry_valid = opening.is_some_and(|opening| {
            let stance_centre =
                stance.map(|surface| (surface.bounds.min + surface.bounds.max) * 0.5);
            let mount = solids.get(&station.mount_solid);
            let ranges = station
                .rays
                .iter()
                .map(|ray| ray.range)
                .collect::<std::collections::HashSet<_>>();
            let recoil_contains = |point: Vec3| {
                point
                    .cmpge(station.recoil_envelope.min - Vec3::splat(0.01))
                    .all()
                    && point
                        .cmple(station.recoil_envelope.max + Vec3::splat(0.01))
                        .all()
            };
            let rays_valid = station.rays.iter().all(|ray| {
                let target_binding = artillery_targets.get(&ray.target_id).is_some_and(|target| {
                    target.kind == ray.target_kind
                        && (Vec2::new(
                            ray.target.x - target.centre.x,
                            ray.target.z - target.centre.z,
                        )
                        .abs()
                        .cmple(target.half_extent_metres + Vec2::splat(0.001)))
                        .all()
                });
                let plan_delta =
                    Vec2::new(ray.target.x - ray.origin.x, ray.target.z - ray.origin.z);
                let distance = plan_delta.length();
                let aim_valid = distance > 2.0
                    && station.facing.dot(plan_delta / distance)
                        >= 38.0_f32.to_radians().cos() - 0.01;
                let origin_plan = Vec2::new(ray.origin.x, ray.origin.z);
                let depth = (origin_plan - opening.frame.origin).dot(opening.frame.outward);
                let origin_valid = (-1.25..=-0.45).contains(&depth)
                    && (ray.origin.y - opening.sill_elevation_metres) >= 0.05
                    && (ray.origin.y - opening.sill_elevation_metres)
                        <= opening.profile.clear_height_metres() - 0.05;
                let segment = ray.target - ray.origin;
                let exit_t = (1.30 / segment.length()).clamp(0.04, 0.45);
                let blocked = (0..24).any(|sample| {
                    // Stop before the declared target envelope itself (gate
                    // closure, bridge deck, or ditch scarp); those are what
                    // the station is meant to cover, not intervening blockers.
                    let t = exit_t + (0.88 - exit_t) * sample as f32 / 23.0;
                    let point = ray.origin.lerp(ray.target, t);
                    plan.resolved_geometry.solids.iter().any(|solid| {
                        !matches!(
                            solid.role,
                            SolidRole::DitchFloor
                                | SolidRole::DitchScarp
                                | SolidRole::DitchCounterscarp
                                | SolidRole::DrainageFloor
                        ) && solid.owner != opening.owner
                            && resolved_solid_contains_point(solid, point, -0.02)
                    })
                });
                target_binding && aim_valid && origin_valid && !blocked
            });
            stance_centre.is_some_and(recoil_contains)
                && mount.is_some_and(|solid| recoil_contains(solid.centre))
                && ranges
                    == std::collections::HashSet::from([
                        crate::ProjectedDefenseRange::Near,
                        crate::ProjectedDefenseRange::Middle,
                        crate::ProjectedDefenseRange::Far,
                    ])
                && rays_valid
        });
        let smoke_valid = station.level != crate::ArtilleryStationLevel::LowerCasemate
            || station.smoke_vent.is_some_and(|id| {
                voids.get(&id).is_some_and(|void| {
                    void.role == VoidRole::ArtillerySmokeVent
                        && void.bounds.max.y > 3.0
                        && void.bounds.max.y - void.bounds.min.y >= 0.6
                })
            });
        if opening.is_none_or(|opening| {
            opening.use_kind != crate::OpeningUse::GunLoop
                || opening.closure.layers != [crate::ClosureKind::OpenMilitary]
        }) || stance.is_none_or(|surface| {
            let size = surface.bounds.max - surface.bounds.min;
            size.x.max(size.z) < 1.0 || size.x.min(size.z) < 0.9
        }) || recoil.x.max(recoil.z) < 4.0
            || recoil.x.min(recoil.z) < 2.5
            || recoil.y < 1.9
            || station.rays.len() < 3
            || !smoke_valid
            || !station_geometry_valid
        {
            issues.push(issue("inoperable_artillery_station",format!("artillery station {} lacks a full-depth open port, stance, recoil, smoke vent, or three ranges",station.id.0)));
        }
    }
    for required in [
        crate::ArtilleryTargetKind::CurtainFoot,
        crate::ArtilleryTargetKind::DitchCorner,
        crate::ArtilleryTargetKind::GateThreshold,
        crate::ArtilleryTargetKind::Bridge,
        crate::ArtilleryTargetKind::Approach,
    ] {
        if !target_kinds.contains(&required) {
            issues.push(issue(
                "incomplete_artillery_coverage",
                format!("no station covers {required:?}"),
            ));
        }
    }
    for required in [
        crate::ArtilleryTargetKind::GateThreshold,
        crate::ArtilleryTargetKind::Bridge,
        crate::ArtilleryTargetKind::Approach,
    ] {
        let independent = castle
            .stations
            .iter()
            .filter(|station| station.rays.iter().any(|ray| ray.target_kind == required))
            .map(|station| station.id)
            .collect::<std::collections::HashSet<_>>();
        if independent.len() < 2 {
            issues.push(issue(
                "incomplete_artillery_coverage",
                format!("{required:?} lacks two independent flanking stations"),
            ));
        }
    }
    for target in castle
        .defense_targets
        .iter()
        .filter(|target| target.required_independent_stations > 0)
    {
        let covering = castle
            .stations
            .iter()
            .filter(|station| station.rays.iter().any(|ray| ray.target_id == target.id))
            .map(|station| station.id)
            .collect::<std::collections::HashSet<_>>();
        if covering.len() < target.required_independent_stations as usize {
            issues.push(issue(
                "incomplete_artillery_coverage",
                format!(
                    "tactical target {} {:?} has {} of {} independent stations",
                    target.id.0,
                    target.kind,
                    covering.len(),
                    target.required_independent_stations
                ),
            ));
        }
    }
    let route_ids = castle
        .route_nodes
        .iter()
        .map(|node| node.id)
        .collect::<std::collections::HashSet<_>>();
    let route_nodes = castle
        .route_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let node_surfaces_valid = castle.route_nodes.iter().all(|node| {
        surfaces.get(&node.surface).is_some_and(|surface| {
            surface.role == SurfaceRole::ArtilleryRoute
                || surface.role == SurfaceRole::ArtilleryStance
        })
    });
    let route_geometry_valid = castle.route_edges.iter().all(|edge| {
        let Some((from, to)) = route_nodes.get(&edge.from).zip(route_nodes.get(&edge.to)) else { return false; };
        let Some(surface) = edge.traversal_surface.and_then(|id| surfaces.get(&id).copied()) else { return false; };
        let shape_valid = matches!(surface.shape, crate::ResolvedSurfaceShape::RouteCorridor { start, end, width_metres }
            if start.distance(from.position) <= 0.02 && end.distance(to.position) <= 0.02 && (width_metres-edge.width_metres).abs() <= 0.01);
        let connectors_valid = edge.connector_solids.iter().all(|id| solids.get(id).is_some_and(|solid| matches!(solid.role,
            SolidRole::ArtilleryRamp | SolidRole::ArtilleryStairTread | SolidRole::ArtilleryBridgeDeck)));
        let portal_valid = edge.portal_void.is_none_or(|id| voids.get(&id).is_some_and(|void| matches!(void.role, VoidRole::Passage | VoidRole::AccessPortal | VoidRole::ArtilleryCasemate)));
        let path_valid=edge.sweep_path.len()>=2&&edge.sweep_path.first().is_some_and(|point|point.distance(from.position)<=0.02)&&edge.sweep_path.last().is_some_and(|point|point.distance(to.position)<=0.02)
            && edge.sweep_path.windows(2).all(|pair|pair[0].distance(pair[1])<=3.0);
        let portal_crossed=edge.portal_void.is_none_or(|id|voids.get(&id).is_some_and(|void|edge.sweep_path.windows(2).any(|pair|{
            (0..=8).any(|sample|{let point=pair[0].lerp(pair[1],sample as f32/8.0)+Vec3::Y*0.25;point.cmpge(void.bounds.min-Vec3::splat(0.02)).all()&&point.cmple(void.bounds.max+Vec3::splat(0.02)).all()})
        })));
        let swept_clear=edge.sweep_path.windows(2).all(|pair|{
            let delta=Vec2::new(pair[1].x-pair[0].x,pair[1].z-pair[0].z);let along=if delta.length()>0.01{delta.normalize()}else{Vec2::X};let across=Vec2::new(-along.y,along.x);
            let steps=((pair[0].distance(pair[1])/0.35).ceil() as usize).max(1);
            (0..=steps).all(|step|{
                let foot=pair[0].lerp(pair[1],step as f32/steps as f32);
                let samples=[-0.45_f32,0.0,0.45].into_iter().flat_map(|side|[0.25_f32,1.0,1.85].into_iter().map(move|height|foot+Vec3::new(across.x*side,height,across.y*side)));
                samples.into_iter().all(|point|!plan.resolved_geometry.solids.iter().any(|solid|{
                    let supporting=edge.connector_solids.contains(&solid.id)||matches!(solid.role,SolidRole::ArtilleryTerreplein|SolidRole::ArtilleryCasemateFloor|SolidRole::ArtilleryRamp|SolidRole::ArtilleryStairTread|SolidRole::ArtilleryBridgeDeck|SolidRole::ArtilleryBridgeAbutment|SolidRole::OpeningClosure|SolidRole::DrainageFloor);
                    !supporting&&artillery_route_solid_contains(solid,point,-0.015)
                }))
            })
        });
        shape_valid && connectors_valid && portal_valid && path_valid && portal_crossed && swept_clear
    });
    let stair_geometry_valid = castle.rondels.iter().all(|rondel| {
        rondel.stair_solids.len() >= 30
            && rondel.stair_solids.iter().all(|id| {
                solids.get(id).is_some_and(|solid| {
                    solid.role == SolidRole::ArtilleryStairTread
                        && solid.size.x >= 0.9
                        && solid.size.y <= 0.20
                        && solid.size.z >= 0.35
                })
            })
            && castle.route_edges.iter().any(|edge| {
                rondel
                    .stair_solids
                    .iter()
                    .all(|id| edge.connector_solids.contains(id))
            })
    });
    let ramp_route_valid = castle.service_ramp_solids.iter().all(|id| {
        castle
            .route_edges
            .iter()
            .any(|edge| edge.connector_solids.contains(id))
    });
    if castle.route_nodes.len() < 12
        || !node_surfaces_valid
        || !route_geometry_valid
        || !stair_geometry_valid
        || !ramp_route_valid
        || castle.route_edges.iter().any(|edge| {
            !route_ids.contains(&edge.from)
                || !route_ids.contains(&edge.to)
                || edge.width_metres < 0.9
                || edge.headroom_metres < 1.9
        })
    {
        issues.push(issue(
            "disconnected_artillery_route",
            "artillery circulation lacks a swept gate/casemate/terreplein/ramp graph".to_owned(),
        ));
    } else {
        let mut reached = std::collections::HashSet::from([castle.route_nodes[0].id]);
        loop {
            let before = reached.len();
            for edge in &castle.route_edges {
                if reached.contains(&edge.from) {
                    reached.insert(edge.to);
                }
                if reached.contains(&edge.to) {
                    reached.insert(edge.from);
                }
            }
            if before == reached.len() {
                break;
            }
        }
        if reached.len() != route_ids.len() {
            issues.push(issue(
                "disconnected_artillery_route",
                "not every artillery working surface is reachable".to_owned(),
            ));
        }
    }
    let deployed_bridge_edge = castle.route_edges.iter().any(|edge| {
        edge.connector_solids
            .iter()
            .any(|id| castle.bridge.removable_solids.contains(id))
    });
    let gate_chamber_valid = castle.gate_chamber_solids.len() >= 6
        && castle
            .gate_chamber_solids
            .iter()
            .all(|id| solids.contains_key(id))
        && castle
            .gate_chamber_solids
            .iter()
            .filter(|id| {
                solids
                    .get(id)
                    .is_some_and(|solid| solid.role == SolidRole::ArtilleryGateMechanism)
            })
            .count()
            >= 2
        && surfaces
            .get(&castle.gate_operator_surface)
            .is_some_and(|surface| {
                let size = surface.bounds.max - surface.bounds.min;
                surface.role == SurfaceRole::ArtilleryStance && size.x * size.z >= 6.0
            })
        && castle.route_edges.iter().any(|edge| {
            route_nodes
                .get(&edge.to)
                .is_some_and(|node| node.surface == castle.gate_operator_surface)
                || route_nodes
                    .get(&edge.from)
                    .is_some_and(|node| node.surface == castle.gate_operator_surface)
        });
    let ditch_void_valid = voids.get(&castle.ditch.void_id).is_some_and(|void| {
        matches!(void.shape,
        crate::ResolvedVoidShape::RectangularRing { inner_min, inner_max }
            if inner_max.x-inner_min.x >= 40.0 && inner_max.y-inner_min.y >= 34.0
                && void.bounds.min.y <= -2.0 && void.bounds.max.y <= 0.01)
    });
    if castle.service_ramp_solids.is_empty()
        || !gate_chamber_valid
        || castle.bridge.clear_width_metres < 1.8
        || castle.ditch.width_metres < 5.0
        || castle.ditch.depth_metres < 2.0
        || !ditch_void_valid
        || castle.bridge.state == crate::BridgeState::Deployed
            && (castle.bridge.route_surface.is_none() || castle.bridge.denied_gap_void.is_some())
        || castle.bridge.state == crate::BridgeState::Denied
            && (castle.bridge.route_surface.is_some() || castle.bridge.denied_gap_void.is_none())
        || castle.bridge.state == crate::BridgeState::Deployed && !deployed_bridge_edge
        || castle.bridge.state == crate::BridgeState::Denied && deployed_bridge_edge
    {
        issues.push(issue(
            "invalid_artillery_approach",
            "ditch, service ramp, or deployed/denied bridge state is not physical".to_owned(),
        ));
    }
    if castle.retained_keep_setback_metres < 4.0 {
        issues.push(issue(
            "artillery_keep_clearance",
            "retained keep crowds artillery circulation/recoil".to_owned(),
        ));
    }
}

fn timber_audit_polygon(points: impl IntoIterator<Item = Vec2>) -> Polygon<f32> {
    let mut coordinates = points
        .into_iter()
        .map(|point| Coord {
            x: point.x,
            y: point.y,
        })
        .collect::<Vec<_>>();
    if coordinates.first() != coordinates.last() {
        coordinates.push(coordinates[0]);
    }
    Polygon::new(LineString::new(coordinates), Vec::new())
}

fn timber_member_audit_polygon(
    member: &crate::TimberFrameMember,
    wall: &crate::WallAssembly,
) -> Polygon<f32> {
    let project = |point: Vec3| {
        Vec2::new(
            (Vec2::new(point.x, point.z) - wall.frame.origin).dot(wall.frame.tangent),
            point.y - wall.base_elevation_metres,
        )
    };
    let start = project(member.start);
    let end = project(member.end);
    let axis = (end - start).normalize_or_zero();
    let normal = Vec2::new(-axis.y, axis.x);
    let half = member.section_metres.max_element() * 0.5;
    timber_audit_polygon([
        start - axis * half - normal * half,
        end + axis * half - normal * half,
        end + axis * half + normal * half,
        start - axis * half + normal * half,
    ])
}

fn timber_panel_audit_polygon(
    solid: &ResolvedSolid,
    wall: &crate::WallAssembly,
) -> Option<Polygon<f32>> {
    let crate::ResolvedSolidShape::TimberPanelPrism {
        vertices,
        outward,
        depth_metres,
    } = solid.shape
    else {
        return None;
    };
    if outward.dot(wall.frame.outward) < 0.999
        || depth_metres <= 0.02
        || depth_metres >= wall.thickness_metres - 0.02
    {
        return None;
    }
    let depth_offset = Vec3::new(outward.x, 0.0, outward.y) * depth_metres * 0.5;
    let min = vertices
        .iter()
        .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let max = vertices
        .iter()
        .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    if solid.centre.distance((min + max) * 0.5) > 0.002 || solid.size.distance(max - min) > 0.002 {
        return None;
    }
    Some(timber_audit_polygon(vertices.map(|vertex| {
        Vec2::new(
            (Vec2::new(vertex.x, vertex.z) - wall.frame.origin).dot(wall.frame.tangent),
            vertex.y - wall.base_elevation_metres,
        )
    })))
}

fn timber_infill_residual_valid(
    plan: &BuildingPlan,
    frame: &crate::TimberFrameAssembly,
    wall: &crate::WallAssembly,
    bay: &crate::TimberFrameBay,
    solids: &std::collections::HashMap<ResolvedItemId, &ResolvedSolid>,
) -> bool {
    let declared = bay
        .infill_solids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let authoritative = wall
        .host_solids
        .iter()
        .copied()
        .filter(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::WallHost)
        })
        .collect::<std::collections::HashSet<_>>();
    let panels = bay
        .infill_solids
        .iter()
        .filter_map(|id| solids.get(id).copied())
        .filter(|solid| solid.role == SolidRole::WallHost)
        .collect::<Vec<_>>();
    let half_length = wall.length_metres * 0.5;
    let mut expected = MultiPolygon(vec![timber_audit_polygon([
        Vec2::new(-half_length, 0.0),
        Vec2::new(half_length, 0.0),
        Vec2::new(half_length, wall.height_metres),
        Vec2::new(-half_length, wall.height_metres),
    ])]);
    for opening in plan
        .opening_assemblies
        .iter()
        .filter(|opening| opening.host_wall == wall.id)
    {
        let half_opening = (opening.profile.interior_width_metres() * 0.5).min(half_length - 0.02);
        let centre = (opening.frame.origin - wall.frame.origin).dot(wall.frame.tangent);
        let sill = (opening.sill_elevation_metres - wall.base_elevation_metres)
            .clamp(0.0, wall.height_metres);
        let head = (sill + opening.profile.clear_height_metres()).clamp(sill, wall.height_metres);
        expected = expected.difference(&timber_audit_polygon([
            Vec2::new(centre - half_opening, sill),
            Vec2::new(centre + half_opening, sill),
            Vec2::new(centre + half_opening, head),
            Vec2::new(centre - half_opening, head),
        ]));
    }
    let wall_member_ids = frame
        .bays
        .iter()
        .filter(|candidate| candidate.wall == Some(wall.id))
        .flat_map(|candidate| candidate.member_ids.iter().copied())
        .collect::<std::collections::HashSet<_>>();
    for member in frame
        .members
        .iter()
        .filter(|member| wall_member_ids.contains(&member.id))
    {
        expected = expected.difference(&timber_member_audit_polygon(member, wall));
    }
    let panel_polygons = panels
        .iter()
        .filter_map(|panel| timber_panel_audit_polygon(panel, wall))
        .collect::<Vec<_>>();
    let panel_union =
        panel_polygons
            .iter()
            .cloned()
            .fold(MultiPolygon(Vec::new()), |union, panel| {
                if union.0.is_empty() {
                    MultiPolygon(vec![panel])
                } else {
                    union.union(&panel)
                }
            });
    let expected_area = expected.unsigned_area();
    let panel_area_sum = panel_polygons
        .iter()
        .map(Polygon::unsigned_area)
        .sum::<f32>();
    let union_area = panel_union.unsigned_area();
    declared == authoritative
        && panels.len() == panel_polygons.len()
        && !panels.is_empty()
        && expected_area > 0.02
        && expected.difference(&panel_union).unsigned_area() <= 0.0005
        && panel_union.difference(&expected).unsigned_area() <= 0.0005
        && (panel_area_sum - union_area).max(0.0) <= 0.0005
}

fn audit_timber_frame(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    let expected = match plan.archetype {
        BuildingArchetype::TownHouse => Some(crate::TimberFrameProgramKind::NarrowUrbanTownHouse),
        BuildingArchetype::HallHouse => {
            Some(crate::TimberFrameProgramKind::NorthernTwoPostHallHouse)
        }
        BuildingArchetype::FachwerkCottage => {
            Some(crate::TimberFrameProgramKind::DirectRoofCottage)
        }
        BuildingArchetype::FachwerkMerchantHouse => {
            Some(crate::TimberFrameProgramKind::JettiedMerchantHouse)
        }
        BuildingArchetype::RenaissanceTownHall => {
            Some(crate::TimberFrameProgramKind::CivicMasonryTimberHall)
        }
        _ => None,
    };
    let Some(expected) = expected else {
        if plan.timber_frame.is_some() {
            issues.push(issue(
                "invalid_timber_program",
                "non-civilian fixture declares a semantic timber-frame program".to_owned(),
            ));
        }
        return;
    };
    let Some(frame) = &plan.timber_frame else {
        issues.push(issue(
            "missing_authoritative_timber_frame",
            "civilian fixture has no semantic timber-frame assembly".to_owned(),
        ));
        return;
    };
    if frame.program != expected || frame.id.0 == 0 {
        issues.push(issue(
            "invalid_timber_program",
            "timber-frame program does not match the curated archetype".to_owned(),
        ));
    }

    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let interfaces = plan
        .resolved_geometry
        .support_interfaces
        .iter()
        .map(|interface| (interface.id, interface))
        .collect::<std::collections::HashMap<_, _>>();
    let members = frame
        .members
        .iter()
        .map(|member| (member.id, member))
        .collect::<std::collections::HashMap<_, _>>();
    let joints = frame
        .joints
        .iter()
        .map(|joint| (joint.id, joint))
        .collect::<std::collections::HashMap<_, _>>();
    let mut ids = std::collections::HashSet::new();
    if members.len() != frame.members.len()
        || joints.len() != frame.joints.len()
        || frame
            .facades
            .iter()
            .any(|facade| !ids.insert((0_u8, facade.id.0)))
        || frame
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .chain(frame.internal_lines.iter())
            .any(|line| !ids.insert((1_u8, line.id.0)))
        || frame
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .chain(frame.internal_lines.iter())
            .flat_map(|line| &line.storeys)
            .any(|storey| !ids.insert((2_u8, storey.id.0)))
        || frame.bays.iter().any(|bay| !ids.insert((3_u8, bay.id.0)))
    {
        issues.push(issue(
            "duplicate_timber_frame_id",
            "timber frame contains duplicate assembly-local IDs".to_owned(),
        ));
    }

    let frame_owner = frame
        .members
        .first()
        .map_or(crate::GeometryOwnerId(0), |member| member.owner);
    for node in nodes.values().filter(|node| node.owner == frame_owner) {
        for parent in &node.supported_by {
            let endpoint_member = frame.members.iter().any(|member| {
                member.structural
                    && ((member.start_node == node.id && member.end_node == *parent)
                        || (member.end_node == node.id && member.start_node == *parent))
            });
            let measured_body_contact = interfaces.values().any(|interface| {
                interface.owner == frame_owner
                    && (interface.node == node.id || interface.node == *parent)
                    && frame.members.iter().any(|member| {
                        member.structural
                            && ((interface.node == node.id
                                && [member.start_node, member.end_node].contains(parent))
                                || (interface.node == *parent
                                    && [member.start_node, member.end_node].contains(&node.id)))
                            && solids.get(&member.solid).is_some_and(|solid| {
                                resolved_solid_overlaps_bounds(
                                    solid,
                                    (interface.bounds.min, interface.bounds.max),
                                    0.001,
                                )
                            })
                    })
            });
            let measured_external_contact = nodes.get(parent).is_some_and(|support| {
                support.owner != node.owner
                    && interfaces.values().any(|interface| {
                        interface.node == node.id
                            && (solids.values().any(|solid| {
                                (solid.owner == support.owner
                                    || solid.supported_by.contains(parent))
                                    && resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                            }) || plan.roof_assemblies.iter().any(|roof| {
                                roof.support_nodes.contains(parent)
                                    && roof.faces.iter().any(|face| {
                                        let centre =
                                            (interface.bounds.min + interface.bounds.max) * 0.5;
                                        let point = Vec2::new(centre.x, centre.z);
                                        roof_face_contains_plan_point_inclusive(face, point)
                                            && roof_face_height(face, point).is_some_and(|height| {
                                                let underside = height
                                                    - face.plane.normal.normalize_or_zero().y
                                                        * face.thickness_metres;
                                                underside >= interface.bounds.min.y - 0.002
                                                    && underside <= interface.bounds.max.y + 0.002
                                            })
                                    })
                            }))
                    })
            });
            if !endpoint_member && !measured_body_contact && !measured_external_contact {
                issues.push(issue(
                    "false_timber_support_edge",
                    format!(
                        "timber node {} claims support {} without a member/contact interface",
                        node.id.0, parent.0
                    ),
                ));
            }
        }
    }

    for member in &frame.members {
        let endpoints = [nodes.get(&member.start_node), nodes.get(&member.end_node)];
        let member_joints = [
            joints.get(&member.start_joint),
            joints.get(&member.end_joint),
        ];
        let solid = solids.get(&member.solid);
        let valid_role = solid.is_some_and(|solid| {
            matches!(
                (member.role, solid.role),
                (crate::TimberMemberRole::Sill, SolidRole::FrameSill)
                    | (
                        crate::TimberMemberRole::PrimaryPost
                            | crate::TimberMemberRole::CornerPost
                            | crate::TimberMemberRole::IntermediatePost,
                        SolidRole::FramePost
                    )
                    | (crate::TimberMemberRole::WallPlate, SolidRole::FramePlate)
                    | (crate::TimberMemberRole::Rail, SolidRole::FrameRail)
                    | (crate::TimberMemberRole::FloorJoist, SolidRole::FrameJoist)
                    | (
                        crate::TimberMemberRole::Girder | crate::TimberMemberRole::Purlin,
                        SolidRole::FrameGirder
                    )
                    | (crate::TimberMemberRole::TransverseTie, SolidRole::FrameTie)
                    | (
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace,
                        SolidRole::FrameBrace
                    )
                    | (
                        crate::TimberMemberRole::JettyBeam,
                        SolidRole::FrameJettyBeam
                    )
                    | (crate::TimberMemberRole::Knagge, SolidRole::FrameKnagge)
                    | (
                        crate::TimberMemberRole::GableTie
                            | crate::TimberMemberRole::GablePost
                            | crate::TimberMemberRole::Rafter
                            | crate::TimberMemberRole::Collar,
                        SolidRole::FrameGableMember
                    )
                    | (
                        crate::TimberMemberRole::DormerTrimmer,
                        SolidRole::FrameDormerTrimmer
                    )
                    | (crate::TimberMemberRole::Ornament, SolidRole::FrameOrnament)
            )
        });
        let endpoint_contact = endpoints[0]
            .is_some_and(|node| node.position.distance(member.start) <= 0.003)
            && endpoints[1].is_some_and(|node| node.position.distance(member.end) <= 0.003)
            && member.start_node != member.end_node
            && member.start.distance(member.end) > 0.05;
        let joint_contact = member_joints[0].is_some_and(|joint| {
            joint.node == member.start_node
                && joint.member_ids.contains(&member.id)
                && joint.contact_area_square_metres >= 0.008
        }) && member_joints[1].is_some_and(|joint| {
            joint.node == member.end_node
                && joint.member_ids.contains(&member.id)
                && joint.contact_area_square_metres >= 0.008
        });
        let bearing_contact = member.support_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                interface.owner == member.owner
                    && [member.start_node, member.end_node].contains(&interface.node)
                    && solid.is_some_and(|solid| {
                        resolved_solid_overlaps_bounds(
                            solid,
                            (interface.bounds.min, interface.bounds.max),
                            0.001,
                        )
                    })
            })
        });
        let solid_correspondence = solid.is_some_and(|solid| {
            solid.centre.distance((member.start + member.end) * 0.5) <= 0.003
                && (solid.size.y - member.start.distance(member.end)).abs() <= 0.003
                && (solid.size.x - member.section_metres.x).abs() <= 0.003
                && (solid.size.z - member.section_metres.y).abs() <= 0.003
        });
        if !valid_role
            || !endpoint_contact
            || !joint_contact
            || !bearing_contact
            || !solid_correspondence
            || member.section_metres.min_element() < 0.08
            || member.structural == (member.role == crate::TimberMemberRole::Ornament)
        {
            issues.push(issue(
                "invalid_timber_member_joint",
                format!(
                    "timber member {} {:?} has false truth role={valid_role} endpoint={endpoint_contact} joint={joint_contact} bearing={bearing_contact} solid={solid_correspondence}",
                    member.id.0, member.role
                ),
            ));
        }
    }
    for (index, left) in frame.members.iter().enumerate() {
        if left.role == crate::TimberMemberRole::Ornament {
            continue;
        }
        for right in frame.members.iter().skip(index + 1) {
            if right.role == crate::TimberMemberRole::Ornament {
                continue;
            }
            let nested_posts = matches!(
                left.role,
                crate::TimberMemberRole::PrimaryPost
                    | crate::TimberMemberRole::CornerPost
                    | crate::TimberMemberRole::IntermediatePost
            ) && matches!(
                right.role,
                crate::TimberMemberRole::PrimaryPost
                    | crate::TimberMemberRole::CornerPost
                    | crate::TimberMemberRole::IntermediatePost
            ) && Vec2::new(left.start.x, left.start.z)
                .distance(Vec2::new(right.start.x, right.start.z))
                < 0.02
                && solids
                    .get(&left.solid)
                    .zip(solids.get(&right.solid))
                    .is_some_and(|(left_solid, right_solid)| {
                        resolved_solids_overlap_positive_volume(left_solid, right_solid, 0.005)
                    });
            if nested_posts {
                issues.push(issue(
                    "overlapping_timber_members",
                    format!(
                        "timber posts {} {:?}->{:?} and {} {:?}->{:?} claim the same positive-volume load path",
                        left.id.0, left.start, left.end, right.id.0, right.start, right.end
                    ),
                ));
            }
        }
    }
    for joint in &frame.joints {
        let unique = joint
            .member_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        if joint.id.0 == 0
            || joint.contact_area_square_metres < 0.008
            || unique.len() != joint.member_ids.len()
            || unique.is_empty()
            || unique.iter().any(|id| {
                members.get(id).is_none_or(|member| {
                    ![member.start_joint, member.end_joint].contains(&joint.id)
                        || ![member.start_node, member.end_node].contains(&joint.node)
                })
            })
        {
            issues.push(issue(
                "orphan_timber_joint",
                format!(
                    "timber joint {} at node {} {:?} with members {:?} is orphaned, duplicated, or drifts from its members",
                    joint.id.0,
                    joint.node.0,
                    nodes.get(&joint.node).map(|node| node.position),
                    joint.member_ids
                ),
            ));
            continue;
        }
        let participant_roles = joint
            .member_ids
            .iter()
            .filter_map(|id| members.get(id).map(|member| member.role))
            .collect::<Vec<_>>();
        let has = |role| participant_roles.contains(&role);
        let grounded = nodes.get(&joint.node).is_some_and(|node| node.grounded);
        let expected_kind = if grounded {
            crate::TimberJointKind::FoundationBearing
        } else if (has(crate::TimberMemberRole::JettyBeam)
            && (has(crate::TimberMemberRole::Knagge)
                || has(crate::TimberMemberRole::Girder)
                || has(crate::TimberMemberRole::Sill)))
            || (has(crate::TimberMemberRole::Knagge)
                && (has(crate::TimberMemberRole::PrimaryPost)
                    || has(crate::TimberMemberRole::CornerPost)))
        {
            crate::TimberJointKind::JettyBearing
        } else if (has(crate::TimberMemberRole::Rafter)
            && (has(crate::TimberMemberRole::WallPlate)
                || has(crate::TimberMemberRole::Collar)
                || has(crate::TimberMemberRole::GablePost)))
            || (has(crate::TimberMemberRole::DormerTrimmer)
                && (has(crate::TimberMemberRole::Rafter) || has(crate::TimberMemberRole::Purlin)))
            || (has(crate::TimberMemberRole::Purlin)
                && (has(crate::TimberMemberRole::PrimaryPost)
                    || has(crate::TimberMemberRole::GablePost)))
        {
            crate::TimberJointKind::RoofSeat
        } else if (has(crate::TimberMemberRole::FloorJoist) && has(crate::TimberMemberRole::Girder))
            || (has(crate::TimberMemberRole::TransverseTie)
                && (has(crate::TimberMemberRole::PrimaryPost)
                    || has(crate::TimberMemberRole::Purlin)))
        {
            crate::TimberJointKind::HousedBeam
        } else if participant_roles.iter().any(|role| {
            matches!(
                role,
                crate::TimberMemberRole::HeadBrace
                    | crate::TimberMemberRole::FootBrace
                    | crate::TimberMemberRole::StoreyBrace
            )
        }) && participant_roles.len() >= 2
        {
            crate::TimberJointKind::Lap
        } else if participant_roles
            .iter()
            .filter(|role| {
                matches!(
                    role,
                    crate::TimberMemberRole::Sill | crate::TimberMemberRole::WallPlate
                )
            })
            .count()
            >= 2
        {
            crate::TimberJointKind::Scarf
        } else {
            crate::TimberJointKind::MortiseTenon
        };
        let contact_set = joint
            .contact_interfaces
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let contacts_valid = contact_set.len() == joint.member_ids.len()
            && joint.member_ids.iter().all(|member_id| {
                members.get(member_id).is_some_and(|member| {
                    let interface_id = if member.start_node == joint.node {
                        member.support_interfaces[0]
                    } else {
                        member.support_interfaces[1]
                    };
                    contact_set.contains(&interface_id)
                        && interfaces.get(&interface_id).is_some_and(|interface| {
                            interface.node == joint.node
                                && solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                        })
                })
            });
        let expected_participants = joint
            .member_ids
            .iter()
            .filter_map(|member_id| {
                let member = members.get(member_id)?;
                let axis = if member.start_node == joint.node {
                    member.end - member.start
                } else if member.end_node == joint.node {
                    member.start - member.end
                } else {
                    return None;
                }
                .normalize_or_zero();
                Some((*member_id, member.role, axis))
            })
            .collect::<Vec<_>>();
        let participants_valid = joint.participants.len() == expected_participants.len()
            && expected_participants.iter().all(|(member_id, _, axis)| {
                joint.participants.iter().any(|participant| {
                    participant.member == *member_id
                        && participant.axis_from_joint.dot(*axis) >= 0.999
                        && participant.reaction_direction.dot(-*axis) >= 0.999
                        && (participant.axis_from_joint + participant.reaction_direction).length()
                            <= 0.002
                })
            });
        let role_axis = |role| {
            expected_participants
                .iter()
                .find_map(|(_, participant_role, axis)| {
                    (*participant_role == role).then_some(*axis)
                })
        };
        let downward = |axis: Vec3| if axis.y <= 0.0 { axis } else { -axis };
        let gravity_biased = |axis: Vec3, lateral_weight: f32| {
            let lateral = Vec3::new(axis.x, 0.0, axis.z).normalize_or_zero();
            (lateral * lateral_weight - Vec3::Y).normalize_or_zero()
        };
        let expected_load = match joint.kind {
            crate::TimberJointKind::JettyBearing => {
                role_axis(crate::TimberMemberRole::JettyBeam).map(|axis| gravity_biased(axis, 0.65))
            }
            crate::TimberJointKind::Lap => [
                crate::TimberMemberRole::HeadBrace,
                crate::TimberMemberRole::FootBrace,
                crate::TimberMemberRole::StoreyBrace,
            ]
            .into_iter()
            .find_map(role_axis)
            .map(downward),
            crate::TimberJointKind::RoofSeat => role_axis(crate::TimberMemberRole::Rafter)
                .or_else(|| role_axis(crate::TimberMemberRole::Purlin))
                .map(downward),
            crate::TimberJointKind::HousedBeam => role_axis(crate::TimberMemberRole::FloorJoist)
                .or_else(|| role_axis(crate::TimberMemberRole::TransverseTie))
                .map(|axis| gravity_biased(axis, 0.25)),
            crate::TimberJointKind::Scarf => role_axis(crate::TimberMemberRole::Sill)
                .or_else(|| role_axis(crate::TimberMemberRole::WallPlate))
                .map(|axis| gravity_biased(axis, 0.20)),
            _ => expected_participants
                .iter()
                .max_by(|left, right| left.2.y.abs().total_cmp(&right.2.y.abs()))
                .map(|(_, _, axis)| {
                    if axis.y.abs() >= 0.35 {
                        downward(*axis)
                    } else {
                        gravity_biased(*axis, 0.15)
                    }
                }),
        }
        .unwrap_or(-Vec3::Y)
        .normalize_or_zero();
        let load_valid = participants_valid
            && joint.load_direction.length() >= 0.99
            && joint.load_direction.dot(expected_load) >= 0.999;
        let material_valid = joint.member_ids.iter().all(|id| {
            members.get(id).is_some_and(|member| {
                member.material == frame.material
                    && member.phase != crate::TimberFramePhase::NonStructuralFinish
            })
        });
        if joint.kind != expected_kind || !contacts_valid || !load_valid || !material_valid {
            issues.push(issue(
                "invalid_timber_joint_contact",
                format!(
                    "timber joint {} {:?} does not match participants {:?}, contacts, load direction, or member material",
                    joint.id.0, joint.kind, participant_roles
                ),
            ));
        }
    }

    let allowed_joint = |kind| {
        matches!(
            kind,
            crate::TimberJointKind::FoundationBearing
                | crate::TimberJointKind::MortiseTenon
                | crate::TimberJointKind::HousedBeam
                | crate::TimberJointKind::Scarf
                | crate::TimberJointKind::Lap
                | crate::TimberJointKind::RoofSeat
                | crate::TimberJointKind::JettyBearing
        )
    };
    let has_joint = |kind| frame.joints.iter().any(|joint| joint.kind == kind);
    if frame.joints.iter().any(|joint| !allowed_joint(joint.kind))
        || !has_joint(crate::TimberJointKind::FoundationBearing)
        || (plan.storeys.len() > 1 && !has_joint(crate::TimberJointKind::HousedBeam))
        || !has_joint(crate::TimberJointKind::Lap)
        || !has_joint(crate::TimberJointKind::RoofSeat)
        || (plan.upper_storey_projection_metres > 0.0
            && !has_joint(crate::TimberJointKind::JettyBearing))
    {
        issues.push(issue(
            "invalid_timber_joint_vocabulary",
            "timber frame does not use the compact physical Stage 6 joint vocabulary".to_owned(),
        ));
    }

    let unique_floor_levels = frame
        .floors
        .iter()
        .map(|floor| floor.level)
        .collect::<std::collections::HashSet<_>>();
    let floor_program_valid = frame.floors.len() == plan.storeys.len()
        && unique_floor_levels.len() == frame.floors.len()
        && frame.floors.iter().all(|floor| {
            let floor_solids = floor
                .floor_solids
                .iter()
                .filter_map(|id| solids.get(id).copied())
                .collect::<Vec<_>>();
            let surface = plan
                .resolved_geometry
                .surfaces
                .iter()
                .find(|surface| surface.id == floor.route_surface);
            let joists = floor
                .joist_members
                .iter()
                .filter_map(|id| members.get(id).copied())
                .collect::<Vec<_>>();
            let girders = floor
                .girder_members
                .iter()
                .filter_map(|id| members.get(id).copied())
                .collect::<Vec<_>>();
            let mut floor_contacts_valid = true;
            let mut housed_contacts_valid = true;
            let mut every_piece_supported = true;
            let floor_bearings_valid = if floor.level == 0 {
                floor.bearing_interfaces.iter().any(|id| {
                    interfaces.get(id).is_some_and(|interface| {
                        floor_solids.iter().any(|solid| {
                            bounds_overlap_3d(
                                resolved_solid_bounds(solid),
                                (interface.bounds.min, interface.bounds.max),
                                0.001,
                            )
                        })
                    })
                })
            } else {
                floor_contacts_valid = !floor.floor_joist_interfaces.is_empty()
                    && floor.floor_joist_interfaces.iter().all(|id| {
                        interfaces.get(id).is_some_and(|interface| {
                            floor_solids.iter().any(|solid| {
                                bounds_overlap_3d(
                                    resolved_solid_bounds(solid),
                                    (interface.bounds.min, interface.bounds.max),
                                    0.001,
                                )
                            }) && joists.iter().any(|member| {
                                solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            })
                        })
                    });
                housed_contacts_valid = floor.joist_girder_interfaces.iter().all(|id| {
                        interfaces.get(id).is_some_and(|interface| {
                            joists.iter().any(|member| {
                                solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            }) && girders.iter().any(|member| {
                                solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            })
                        })
                    });
                every_piece_supported = floor_solids.iter().all(|solid| {
                        floor.floor_joist_interfaces.iter().any(|id| {
                            interfaces.get(id).is_some_and(|interface| {
                                solid.supported_by.contains(&interface.node)
                                    && bounds_overlap_3d(
                                        resolved_solid_bounds(solid),
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                            })
                        })
                    });
                floor_contacts_valid && housed_contacts_valid && every_piece_supported
            };
            let stair_valid = if floor.level == 0 {
                floor.stair_connection.is_none()
            } else {
                floor.stair_connection.is_some_and(|point| {
                    surface.is_some_and(|surface| {
                        point.x >= surface.bounds.min.x
                            && point.x <= surface.bounds.max.x
                            && point.y >= surface.bounds.min.z
                            && point.y <= surface.bounds.max.z
                    }) && plan.stairs.iter().any(|stair| match *stair {
                        crate::Stair::Straight { start, .. } => start.distance(point) <= 0.02,
                        crate::Stair::Spiral { centre, .. } => centre.distance(point) <= 0.02,
                    })
                })
            };
            let valid = floor_solids.len() == floor.floor_solids.len()
                && floor_solids.iter().all(|solid| {
                    solid.role == SolidRole::FrameFloor
                        && solid.size.y >= 0.12
                        && !solid.supported_by.is_empty()
                })
                && surface.is_some_and(|surface| {
                surface.role == crate::SurfaceRole::TimberCirculation
                    && surface.bounds.max.x - surface.bounds.min.x >= 0.90
                    && surface.bounds.max.z - surface.bounds.min.z >= 0.90
            }) && joists.len() == floor.joist_members.len()
                && (floor.level == 0 || joists.len() >= 3)
                && joists
                    .iter()
                    .all(|member| member.role == crate::TimberMemberRole::FloorJoist)
                && girders.len() == floor.girder_members.len()
                && (floor.level == 0 || girders.len() >= 2)
                && girders
                    .iter()
                    .all(|member| member.role == crate::TimberMemberRole::Girder)
                && floor_bearings_valid
                && stair_valid;
            if !valid {
                issues.push(issue(
                    "unsupported_timber_floor_route",
                    format!(
                        "timber floor {} invalid: solids {}/{}, joists {}/{}, girders {}/{}, bearings {} (floor {}, housed {}, pieces {}), stair {}",
                        floor.level,
                        floor_solids.len(),
                        floor.floor_solids.len(),
                        joists.len(),
                        floor.joist_members.len(),
                        girders.len(),
                        floor.girder_members.len(),
                        floor_bearings_valid,
                        floor_contacts_valid,
                        housed_contacts_valid,
                        every_piece_supported,
                        stair_valid,
                    ),
                ));
            }
            valid
        });
    if !floor_program_valid {
        issues.push(issue(
            "unsupported_timber_floor_route",
            "occupied timber levels lack joists, girders, bearings, or stair-connected routes"
                .to_owned(),
        ));
    }

    // Occupied civilian levels are one physical route, not a collection of
    // floor labels.  Validate adjacency through the exact entry void, each
    // tread/landing surface, and every floor subtraction before allowing the
    // route to satisfy the program contract.
    let circulation = &frame.circulation;
    let route_nodes = circulation
        .nodes
        .iter()
        .map(|node| (node.surface, node))
        .collect::<std::collections::HashMap<_, _>>();
    let route_surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .filter(|surface| surface.role == crate::SurfaceRole::TimberCirculation)
        .map(|surface| (surface.id, surface))
        .collect::<std::collections::HashMap<_, _>>();
    let entry = circulation.entry_opening.and_then(|id| {
        plan.opening_assemblies
            .iter()
            .find(|opening| opening.id == id)
    });
    let node_geometry_valid = route_nodes.len() == circulation.nodes.len()
        && circulation.nodes.iter().all(|node| {
            route_surfaces.get(&node.surface).is_some_and(|surface| {
                node.position.x >= surface.bounds.min.x - 0.01
                    && node.position.x <= surface.bounds.max.x + 0.01
                    && node.position.y >= surface.bounds.min.y - 0.02
                    && node.position.y <= surface.bounds.max.y + 0.02
                    && node.position.z >= surface.bounds.min.z - 0.01
                    && node.position.z <= surface.bounds.max.z + 0.01
            })
        });
    let entry_geometry_valid = entry.is_some_and(|opening| {
        let Some(void) = plan
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == opening.void_id)
        else {
            return false;
        };
        let approach = circulation
            .nodes
            .iter()
            .find(|node| node.kind == crate::TimberRouteNodeKind::ExteriorApproach);
        let threshold = circulation
            .nodes
            .iter()
            .find(|node| node.kind == crate::TimberRouteNodeKind::DoorThreshold);
        let min_width = opening
            .sectional_void
            .iter()
            .map(|slice| slice.width_metres)
            .fold(f32::INFINITY, f32::min);
        let min_height = opening
            .sectional_void
            .iter()
            .map(|slice| slice.height_metres)
            .fold(f32::INFINITY, f32::min);
        approach
            .zip(threshold)
            .is_some_and(|(approach, threshold)| {
                let lateral = (Vec2::new(threshold.position.x, threshold.position.z)
                    - opening.frame.origin)
                    .dot(opening.frame.tangent)
                    .abs();
                lateral <= min_width * 0.5 - 0.45
                    && threshold.position.x >= void.bounds.min.x - 0.01
                    && threshold.position.x <= void.bounds.max.x + 0.01
                    && threshold.position.z >= void.bounds.min.z - 0.01
                    && threshold.position.z <= void.bounds.max.z + 0.01
                    && circulation.edges.iter().any(|edge| {
                        edge.from == approach.surface
                            && edge.to == threshold.surface
                            && edge.clear_width_metres <= min_width + 0.001
                            && edge.clear_headroom_metres <= min_height + 0.001
                    })
            })
    });
    let edges_valid = circulation.edges.iter().all(|edge| {
        edge.from != edge.to
            && route_nodes.contains_key(&edge.from)
            && route_nodes.contains_key(&edge.to)
            && edge.clear_width_metres >= 0.90
            && edge.clear_headroom_metres >= 1.90
    });
    let start = circulation
        .nodes
        .iter()
        .find(|node| node.kind == crate::TimberRouteNodeKind::ExteriorApproach)
        .map(|node| node.surface);
    let reachable = start.map_or_else(std::collections::HashSet::new, |start| {
        let mut seen = std::collections::HashSet::from([start]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            for edge in &circulation.edges {
                if edge.from == current && seen.insert(edge.to) {
                    queue.push_back(edge.to);
                }
            }
        }
        seen
    });
    let graph_valid = reachable.len() == circulation.nodes.len()
        && circulation
            .nodes
            .iter()
            .all(|node| reachable.contains(&node.surface))
        && (0..plan.storeys.len() as u16).all(|level| {
            circulation.nodes.iter().any(|node| {
                node.level == level
                    && matches!(
                        node.kind,
                        crate::TimberRouteNodeKind::GroundFloor
                            | crate::TimberRouteNodeKind::UpperFloor
                    )
            })
        });
    let tread_nodes = circulation
        .nodes
        .iter()
        .filter(|node| node.kind == crate::TimberRouteNodeKind::StairTread)
        .collect::<Vec<_>>();
    let tread_geometry_valid = tread_nodes.len() == circulation.stair_solids.len()
        && tread_nodes.iter().all(|node| {
            circulation.stair_solids.iter().any(|id| {
                solids.get(id).is_some_and(|solid| {
                    solid.role == SolidRole::Landing
                        && !solid.supported_by.is_empty()
                        && (solid.centre.y + solid.size.y * 0.5 - node.position.y).abs() <= 0.012
                        && Vec2::new(solid.centre.x, solid.centre.z)
                            .distance(Vec2::new(node.position.x, node.position.z))
                            <= 0.03
                        && solid.supported_by.iter().all(|node_id| {
                            interfaces.values().any(|interface| {
                                interface.node == *node_id
                                    && resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                            })
                        })
                })
            })
        });
    let cuts_valid = circulation.floor_cut_voids.len() + 1 == frame.floors.len()
        && circulation.floor_cut_voids.iter().all(|id| {
            plan.resolved_geometry
                .voids
                .iter()
                .find(|void| void.id == *id)
                .is_some_and(|void| {
                    void.role == crate::VoidRole::AccessPortal
                        && frame
                            .floors
                            .iter()
                            .flat_map(|floor| &floor.floor_solids)
                            .all(|solid_id| {
                                solids.get(solid_id).is_none_or(|solid| {
                                    !bounds_overlap_3d(
                                        resolved_solid_bounds(solid),
                                        (void.bounds.min, void.bounds.max),
                                        0.001,
                                    )
                                })
                            })
                })
        });
    let swept_blocker = circulation.edges.iter().find_map(|edge| {
        let Some((from, to)) = route_nodes.get(&edge.from).zip(route_nodes.get(&edge.to)) else {
            return Some((
                edge.from,
                edge.to,
                0,
                Vec3::ZERO,
                Vec3::ZERO,
                crate::ResolvedItemId(0),
                Vec3::ZERO,
                Vec3::ZERO,
            ));
        };
        let plan_delta = Vec2::new(
            to.position.x - from.position.x,
            to.position.z - from.position.z,
        );
        let along = plan_delta.normalize_or_zero();
        let along = if along.length_squared() < 0.9 {
            Vec2::X
        } else {
            along
        };
        let across = Vec2::new(-along.y, along.x);
        (0..=12).find_map(|sample| {
            let foot = from.position.lerp(to.position, sample as f32 / 12.0);
            solids.values().find_map(|solid| {
                (matches!(
                    solid.role,
                    SolidRole::WallHost
                        | SolidRole::OpeningJamb
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                ) && oriented_occupant_overlaps_solid(foot, along, across, solid, 0.01))
                .then_some((
                    edge.from,
                    edge.to,
                    sample,
                    from.position,
                    to.position,
                    solid.id,
                    solid.centre,
                    solid.size,
                ))
            })
        })
    });
    let swept_clear = swept_blocker.is_none();
    if !node_geometry_valid
        || !entry_geometry_valid
        || !edges_valid
        || !graph_valid
        || !tread_geometry_valid
        || !cuts_valid
        || !swept_clear
    {
        issues.push(issue(
            "invalid_timber_circulation",
            format!(
                "timber circulation lacks physical entry/levels/stairs (nodes={node_geometry_valid}, entry={entry_geometry_valid}, edges={edges_valid}, graph={graph_valid}, treads={tread_geometry_valid}, cuts={cuts_valid}, sweep={swept_clear} blocker={swept_blocker:?})"
            ),
        ));
    }

    for bay in &frame.bays {
        let bay_members = bay
            .member_ids
            .iter()
            .filter_map(|id| members.get(id).copied())
            .collect::<Vec<_>>();
        let residual_valid = bay
            .wall
            .and_then(|wall_id| plan.wall_assemblies.iter().find(|wall| wall.id == wall_id))
            .is_some_and(|wall| timber_infill_residual_valid(plan, frame, wall, bay, &solids));
        if bay.member_ids.len() != bay_members.len()
            || bay.infill_solids.is_empty()
            || bay.infill_solids.iter().any(|id| !solids.contains_key(id))
            || !residual_valid
        {
            issues.push(issue(
                "invalid_timber_bay",
                format!(
                    "timber bay {} lacks exact member/Gefach residual authority (residual={residual_valid})",
                    bay.id.0,
                ),
            ));
        }
        if let Some(opening_id) = bay.opening {
            let Some(opening) = plan
                .opening_assemblies
                .iter()
                .find(|opening| opening.id == opening_id)
            else {
                issues.push(issue(
                    "invalid_timber_opening_bay",
                    format!("timber bay {} references a missing opening", bay.id.0),
                ));
                continue;
            };
            let posts = bay_members
                .iter()
                .filter(|member| member.role == crate::TimberMemberRole::IntermediatePost)
                .count();
            let rails = bay_members
                .iter()
                .filter(|member| member.role == crate::TimberMemberRole::Rail)
                .count();
            let forbidden = bay.wall.is_some_and(|wall_id| {
                plan.wall_assemblies
                    .iter()
                    .find(|wall| wall.id == wall_id)
                    .is_some_and(|wall| {
                        bay_members.iter().any(|member| {
                            if !member.structural {
                                return false;
                            }
                            let mut sample_points = (0..=16)
                                .map(|sample| member.start.lerp(member.end, sample as f32 / 16.0))
                                .collect::<Vec<_>>();
                            if let Some(solid) = solids.get(&member.solid) {
                                sample_points.push(solid.centre);
                            }
                            sample_points.into_iter().any(|point| {
                                let plan_point = Vec2::new(point.x, point.z);
                                let depth = 0.5
                                    - (plan_point - opening.frame.origin)
                                        .dot(opening.frame.outward)
                                        / wall.thickness_metres;
                                if !(-0.001..=1.001).contains(&depth) {
                                    return false;
                                }
                                let lateral = (plan_point - opening.frame.origin)
                                    .dot(opening.frame.tangent)
                                    .abs();
                                let (width, height) = opening
                                    .sectional_void
                                    .windows(2)
                                    .find_map(|pair| {
                                        (depth >= pair[0].depth_fraction - 0.001
                                            && depth <= pair[1].depth_fraction + 0.001)
                                            .then(|| {
                                                let t = ((depth - pair[0].depth_fraction)
                                                    / (pair[1].depth_fraction
                                                        - pair[0].depth_fraction)
                                                        .max(0.0001))
                                                .clamp(0.0, 1.0);
                                                (
                                                    pair[0].width_metres
                                                        + (pair[1].width_metres
                                                            - pair[0].width_metres)
                                                            * t,
                                                    pair[0].height_metres
                                                        + (pair[1].height_metres
                                                            - pair[0].height_metres)
                                                            * t,
                                                )
                                            })
                                    })
                                    .unwrap_or_else(|| {
                                        let slice = opening
                                            .sectional_void
                                            .first()
                                            .expect("audited opening has slices");
                                        (slice.width_metres, slice.height_metres)
                                    });
                                let member_half = member.section_metres.min_element() * 0.48;
                                lateral + member_half < width * 0.5 - 0.01
                                    && point.y - member_half > opening.sill_elevation_metres + 0.01
                                    && point.y + member_half
                                        < opening.sill_elevation_metres + height - 0.01
                            })
                        })
                    })
            });
            let exact_residual = bay.wall.is_some_and(|wall_id| {
                plan.wall_assemblies
                    .iter()
                    .find(|wall| wall.id == wall_id)
                    .is_some_and(|wall| {
                        let declared = bay
                            .infill_solids
                            .iter()
                            .copied()
                            .collect::<std::collections::HashSet<_>>();
                        let authoritative = wall
                            .host_solids
                            .iter()
                            .copied()
                            .filter(|id| {
                                solids
                                    .get(id)
                                    .is_some_and(|solid| solid.role == SolidRole::WallHost)
                            })
                            .collect::<std::collections::HashSet<_>>();
                        let panels = bay
                            .infill_solids
                            .iter()
                            .filter_map(|id| solids.get(id).copied())
                            .filter(|solid| solid.role == SolidRole::WallHost)
                            .collect::<Vec<_>>();
                        let half_length = wall.length_metres * 0.5;
                        let mut expected = MultiPolygon(vec![timber_audit_polygon([
                            Vec2::new(-half_length, 0.0),
                            Vec2::new(half_length, 0.0),
                            Vec2::new(half_length, wall.height_metres),
                            Vec2::new(-half_length, wall.height_metres),
                        ])]);
                        for opening in plan
                            .opening_assemblies
                            .iter()
                            .filter(|opening| opening.host_wall == wall.id)
                        {
                            let half_opening = (opening.profile.interior_width_metres() * 0.5)
                                .min(half_length - 0.02);
                            let centre =
                                (opening.frame.origin - wall.frame.origin).dot(wall.frame.tangent);
                            let sill = (opening.sill_elevation_metres - wall.base_elevation_metres)
                                .clamp(0.0, wall.height_metres);
                            let head = (sill + opening.profile.clear_height_metres())
                                .clamp(sill, wall.height_metres);
                            expected = expected.difference(&timber_audit_polygon([
                                Vec2::new(centre - half_opening, sill),
                                Vec2::new(centre + half_opening, sill),
                                Vec2::new(centre + half_opening, head),
                                Vec2::new(centre - half_opening, head),
                            ]));
                        }
                        let wall_member_ids = frame
                            .bays
                            .iter()
                            .filter(|candidate| candidate.wall == Some(wall.id))
                            .flat_map(|candidate| candidate.member_ids.iter().copied())
                            .collect::<std::collections::HashSet<_>>();
                        for member in frame
                            .members
                            .iter()
                            .filter(|member| wall_member_ids.contains(&member.id))
                        {
                            expected =
                                expected.difference(&timber_member_audit_polygon(member, wall));
                        }

                        let panel_polygons = panels
                            .iter()
                            .filter_map(|panel| timber_panel_audit_polygon(panel, wall))
                            .collect::<Vec<_>>();
                        let panel_union = panel_polygons.iter().cloned().fold(
                            MultiPolygon(Vec::new()),
                            |union, panel| {
                                if union.0.is_empty() {
                                    MultiPolygon(vec![panel])
                                } else {
                                    union.union(&panel)
                                }
                            },
                        );
                        let expected_area = expected.unsigned_area();
                        let panel_area_sum = panel_polygons
                            .iter()
                            .map(Polygon::unsigned_area)
                            .sum::<f32>();
                        let union_area = panel_union.unsigned_area();
                        let missing_area = expected.difference(&panel_union).unsigned_area();
                        let excess_area = panel_union.difference(&expected).unsigned_area();
                        let duplicate_area = (panel_area_sum - union_area).max(0.0);
                        declared == authoritative
                            && panels.len() == panel_polygons.len()
                            && !panels.is_empty()
                            && expected_area > 0.02
                            && missing_area <= 0.0005
                            && excess_area <= 0.0005
                            && duplicate_area <= 0.0005
                    })
            });
            if posts < 2 || rails < 2 || forbidden || !exact_residual {
                issues.push(issue(
                    "invalid_timber_opening_bay",
                    format!(
                        "timber bay {} wall {:?}/{:?}/{:?} does not frame its opening-first clear space (posts {posts}, rails {rails}, collision {forbidden}, residual {exact_residual})",
                        bay.id.0,
                        bay.wall,
                        bay.wall.and_then(|id| plan.wall_assemblies.iter().find(|wall| wall.id == id).map(|wall| wall.source)),
                        bay.wall.and_then(|id| plan.wall_assemblies.iter().find(|wall| wall.id == id).map(|wall| wall.material)),
                    ),
                ));
            }
        }
    }

    let lines = frame
        .facades
        .iter()
        .flat_map(|facade| &facade.lines)
        .chain(frame.internal_lines.iter())
        .collect::<Vec<_>>();
    for line in &lines {
        if line.tangent.length_squared() < 0.99
            || line.outward.length_squared() < 0.99
            || line.tangent.dot(line.outward).abs() > 0.001
            || line.length_metres < 1.4
            || line.storeys.is_empty()
        {
            issues.push(issue(
                "invalid_timber_frame_line",
                format!(
                    "timber frame line {} has invalid local-frame/storey authority",
                    line.id.0
                ),
            ));
        }
        for storey in &line.storeys {
            let listed = storey.member_ids.iter().all(|id| members.contains_key(id));
            let braces = storey
                .member_ids
                .iter()
                .filter(|id| {
                    members.get(id).is_some_and(|member| {
                        matches!(
                            member.role,
                            crate::TimberMemberRole::HeadBrace
                                | crate::TimberMemberRole::FootBrace
                                | crate::TimberMemberRole::StoreyBrace
                        )
                    })
                })
                .count();
            // A brace is structural only when it closes a real, non-collinear
            // three-member frame. Merely ending near another timber (the old
            // test) provided no racking path at all.
            let storey_members = storey
                .member_ids
                .iter()
                .filter_map(|id| members.get(id).copied())
                .collect::<Vec<_>>();
            let closes_triangle = |brace: &crate::TimberFrameMember| {
                let endpoints = [brace.start_node, brace.end_node];
                storey_members.iter().any(|first| {
                    if first.id == brace.id {
                        return false;
                    }
                    let third = if first.start_node == endpoints[0] {
                        Some(first.end_node)
                    } else if first.end_node == endpoints[0] {
                        Some(first.start_node)
                    } else {
                        None
                    };
                    third.is_some_and(|third| {
                        third != endpoints[1]
                            && storey_members.iter().any(|second| {
                                second.id != brace.id
                                    && second.id != first.id
                                    && ((second.start_node == third
                                        && second.end_node == endpoints[1])
                                        || (second.end_node == third
                                            && second.start_node == endpoints[1]))
                            })
                            && nodes
                                .get(&endpoints[0])
                                .zip(nodes.get(&endpoints[1]))
                                .zip(nodes.get(&third))
                                .is_some_and(|((a, b), c)| {
                                    (b.position - a.position)
                                        .cross(c.position - a.position)
                                        .length()
                                        > 0.08
                                })
                    })
                })
            };
            let valid_braces = storey_members
                .iter()
                .filter(|member| {
                    matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    ) && closes_triangle(member)
                })
                .collect::<Vec<_>>();
            let brace_cycles_valid = if line.internal || line.length_metres < 4.5 {
                !valid_braces.is_empty()
            } else {
                let regions = valid_braces
                    .iter()
                    .map(|brace| {
                        let midpoint = (brace.start + brace.end) * 0.5;
                        let along = (Vec2::new(midpoint.x, midpoint.z) - line.origin)
                            .dot(line.tangent)
                            / line.length_metres
                            + 0.5;
                        if (0.30..=0.70).contains(&along) {
                            1_u8
                        } else {
                            0_u8
                        }
                    })
                    .collect::<std::collections::HashSet<_>>();
                regions.contains(&0) && regions.contains(&1)
            };
            if !listed
                || storey.top_elevation_metres <= storey.base_elevation_metres + 1.9
                || (!line.internal
                    && storey.kind != crate::TimberStoreyKind::MasonryPlinth
                    && braces == 0)
                || !brace_cycles_valid
            {
                issues.push(issue(
                    "unbraced_timber_storey",
                    format!(
                        "timber storey {} is unregistered, unsupported, or unbraced",
                        storey.id.0
                    ),
                ));
            }
            if let Some(jetty) = &storey.jetty {
                let valid_ids = jetty.jetty_beams.iter().all(|id| {
                    members
                        .get(id)
                        .is_some_and(|member| member.role == crate::TimberMemberRole::JettyBeam)
                }) && jetty.knaggen.iter().all(|id| {
                    members
                        .get(id)
                        .is_some_and(|member| member.role == crate::TimberMemberRole::Knagge)
                });
                let floor = solids.get(&jetty.floor_solid);
                let polygon_area = jetty
                    .support_polygon
                    .iter()
                    .zip(jetty.support_polygon.iter().cycle().skip(1))
                    .take(jetty.support_polygon.len())
                    .map(|(left, right)| left.perp_dot(*right))
                    .sum::<f32>()
                    .abs()
                    * 0.5;
                let floor_bearings_valid = !jetty.floor_bearing_interfaces.is_empty()
                    && jetty.floor_bearing_interfaces.iter().all(|id| {
                        interfaces.get(id).is_some_and(|interface| {
                            floor.is_some_and(|floor| {
                                resolved_solid_overlaps_bounds(
                                    floor,
                                    (interface.bounds.min, interface.bounds.max),
                                    0.001,
                                )
                            }) && jetty.jetty_beams.iter().any(|beam| {
                                members
                                    .get(beam)
                                    .and_then(|member| solids.get(&member.solid))
                                    .is_some_and(|solid| {
                                        resolved_solid_overlaps_bounds(
                                            solid,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.001,
                                        )
                                    })
                            })
                        })
                    });
                let beam_geometry_valid = jetty.jetty_beams.iter().all(|id| {
                    members.get(id).is_some_and(|beam| {
                        let delta = beam.end - beam.start;
                        let plan_delta = Vec2::new(delta.x, delta.z);
                        let inner_interface = interfaces.get(&beam.support_interfaces[0]);
                        let outer_interface = interfaces.get(&beam.support_interfaces[1]);
                        delta.y.abs() <= 0.02
                            && plan_delta.normalize_or_zero().dot(line.outward) >= 0.98
                            && plan_delta.length()
                                >= jetty.projection_metres + jetty.backspan_metres - 0.03
                            && inner_interface.is_some_and(|interface| {
                                frame.members.iter().any(|girder| {
                                    girder.role == crate::TimberMemberRole::Girder
                                        && solids.get(&girder.solid).is_some_and(|solid| {
                                            resolved_solid_overlaps_bounds(
                                                solid,
                                                (interface.bounds.min, interface.bounds.max),
                                                0.001,
                                            )
                                        })
                                })
                            })
                            && outer_interface.is_some_and(|interface| {
                                frame.members.iter().any(|sill| {
                                    sill.role == crate::TimberMemberRole::Sill
                                        && solids.get(&sill.solid).is_some_and(|solid| {
                                            resolved_solid_overlaps_bounds(
                                                solid,
                                                (interface.bounds.min, interface.bounds.max),
                                                0.001,
                                            )
                                        })
                                })
                            })
                    })
                });
                let knaggen_geometry_valid = jetty.knaggen.iter().all(|id| {
                    members.get(id).is_some_and(|knagge| {
                        let lower = interfaces.get(&knagge.support_interfaces[0]);
                        let upper = interfaces.get(&knagge.support_interfaces[1]);
                        lower.is_some_and(|interface| {
                            frame.members.iter().any(|post| {
                                matches!(
                                    post.role,
                                    crate::TimberMemberRole::PrimaryPost
                                        | crate::TimberMemberRole::CornerPost
                                ) && solids.get(&post.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            }) || plan.wall_assemblies.iter().any(|wall| {
                                wall.storey_level == 0
                                    && wall.material == crate::WallMaterialClass::CivilianMasonry
                                    && wall.host_solids.iter().any(|id| {
                                        solids.get(id).is_some_and(|solid| {
                                            resolved_solid_overlaps_bounds(
                                                solid,
                                                (interface.bounds.min, interface.bounds.max),
                                                0.001,
                                            )
                                        })
                                    })
                            })
                        }) && upper.is_some_and(|interface| {
                            jetty.jetty_beams.iter().any(|beam| {
                                members
                                    .get(beam)
                                    .and_then(|beam| solids.get(&beam.solid))
                                    .is_some_and(|solid| {
                                        resolved_solid_overlaps_bounds(
                                            solid,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.001,
                                        )
                                    })
                            })
                        })
                    })
                });
                if jetty.projection_metres <= 0.0
                    || jetty.projection_metres > 0.65
                    || jetty.backspan_metres < jetty.projection_metres
                    || jetty.jetty_beams.len() < 2
                    || jetty.knaggen.len() < 2
                    || jetty.corner_supports.len() < 2
                    || jetty.support_polygon.len() != 4
                    || polygon_area < jetty.projection_metres + jetty.backspan_metres
                    || !valid_ids
                    || !beam_geometry_valid
                    || !knaggen_geometry_valid
                    || floor.is_none_or(|floor| {
                        floor.role != SolidRole::FrameFloor
                            || floor.size.y < 0.12
                            || floor.size.x.min(floor.size.z) > jetty.projection_metres + 0.05
                            || floor.size.x * floor.size.z > polygon_area + 0.05
                            || floor.size.x * floor.size.z
                                < line.length_metres * jetty.projection_metres * 0.90
                            || !jetty.jetty_beams.iter().all(|beam| {
                                members.get(beam).is_some_and(|member| {
                                    floor.supported_by.contains(&member.start_node)
                                        && floor.supported_by.contains(&member.end_node)
                                })
                            })
                    })
                    || !floor_bearings_valid
                {
                    issues.push(issue(
                        "unsupported_timber_jetty",
                        format!(
                            "timber storey {} has an unsupported jetty/backspan (beam_geometry {beam_geometry_valid}, knaggen_geometry {knaggen_geometry_valid})",
                            storey.id.0,
                        ),
                    ));
                }
            }
        }
    }

    let role_count = |role| {
        frame
            .members
            .iter()
            .filter(|member| member.role == role)
            .count()
    };
    let jetty_count = lines
        .iter()
        .flat_map(|line| &line.storeys)
        .filter(|storey| storey.jetty.is_some())
        .count();
    let ground_route = frame
        .floors
        .iter()
        .find(|floor| floor.level == 0)
        .and_then(|floor| {
            plan.resolved_geometry
                .surfaces
                .iter()
                .find(|surface| surface.id == floor.route_surface)
        });
    let ground_route_has_door = ground_route.is_some_and(|surface| {
        plan.opening_assemblies.iter().any(|opening| {
            opening.use_kind == crate::OpeningUse::Door
                && plan
                    .resolved_geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
                    .is_some_and(|void| {
                        let expanded = 0.20;
                        void.bounds.max.x >= surface.bounds.min.x - expanded
                            && void.bounds.min.x <= surface.bounds.max.x + expanded
                            && void.bounds.max.z >= surface.bounds.min.z - expanded
                            && void.bounds.min.z <= surface.bounds.max.z + expanded
                    })
        })
    });
    let material_valid = match frame.program {
        crate::TimberFrameProgramKind::NorthernTwoPostHallHouse
        | crate::TimberFrameProgramKind::DirectRoofCottage => {
            frame.material == crate::StructuralTimberMaterial::Oak
        }
        _ => frame.material == crate::StructuralTimberMaterial::Fir,
    };
    let program_valid = match frame.program {
        crate::TimberFrameProgramKind::NarrowUrbanTownHouse => {
            jetty_count >= 1 && ground_route_has_door
        }
        crate::TimberFrameProgramKind::NorthernTwoPostHallHouse => {
            let longitudinal = frame
                .internal_lines
                .iter()
                .filter(|line| {
                    line.storeys
                        .iter()
                        .flat_map(|storey| &storey.member_ids)
                        .any(|id| {
                            members.get(id).is_some_and(|member| {
                                member.role == crate::TimberMemberRole::Purlin
                            })
                        })
                })
                .collect::<Vec<_>>();
            let transverse = frame
                .internal_lines
                .iter()
                .filter(|line| {
                    line.storeys
                        .iter()
                        .flat_map(|storey| &storey.member_ids)
                        .any(|id| {
                            members.get(id).is_some_and(|member| {
                                member.role == crate::TimberMemberRole::TransverseTie
                            })
                        })
                })
                .collect::<Vec<_>>();
            longitudinal.len() == 2
                && transverse.len() >= 2
                && frame.internal_lines.iter().all(|line| line.internal)
                && role_count(crate::TimberMemberRole::TransverseTie) >= 2
                && role_count(crate::TimberMemberRole::Purlin) >= 2
                && longitudinal.iter().all(|line| {
                    line.storeys
                        .iter()
                        .flat_map(|storey| &storey.member_ids)
                        .any(|id| {
                            members.get(id).is_some_and(|member| {
                                member.role == crate::TimberMemberRole::PrimaryPost
                            })
                        })
                })
                && frame
                    .members
                    .iter()
                    .filter(|member| member.role == crate::TimberMemberRole::TransverseTie)
                    .all(|tie| {
                        [tie.support_interfaces[0], tie.support_interfaces[1]]
                            .iter()
                            .all(|id| interfaces.contains_key(id))
                    })
                && ground_route_has_door
                && jetty_count == 0
        }
        crate::TimberFrameProgramKind::DirectRoofCottage => {
            jetty_count == 0
                && frame
                    .facades
                    .iter()
                    .flat_map(|facade| &facade.lines)
                    .all(|line| line.storeys.len() == 1)
                && ground_route_has_door
        }
        crate::TimberFrameProgramKind::JettiedMerchantHouse => {
            jetty_count >= 1 && ground_route_has_door
        }
        crate::TimberFrameProgramKind::CivicMasonryTimberHall => {
            lines
                .iter()
                .flat_map(|line| &line.storeys)
                .any(|storey| storey.kind == crate::TimberStoreyKind::CivicTimberHall)
                && plan.wall_assemblies.iter().any(|wall| {
                    wall.storey_level == 0
                        && wall.material == crate::WallMaterialClass::CivilianMasonry
                })
                && frame.masonry_bearing_interfaces.len() >= 4
                && frame.members.iter().any(|member| {
                    member.role == crate::TimberMemberRole::Girder
                        && Vec2::new(member.end.x - member.start.x, member.end.z - member.start.z)
                            .length()
                            >= plan.dimensions_metres().min_element() * 0.75
                })
                && ground_route_has_door
                && frame.masonry_bearing_interfaces.iter().all(|id| {
                    interfaces.get(id).is_some_and(|interface| {
                        nodes.get(&interface.node).is_some_and(|node| {
                            node.supported_by.iter().any(|support| {
                                nodes
                                    .get(support)
                                    .is_some_and(|support| support.owner != node.owner)
                            })
                        })
                    })
                })
        }
    };
    if !program_valid || !material_valid {
        issues.push(issue(
            "invalid_timber_program",
            "timber-frame assembly does not satisfy its archetype-specific structural program"
                .to_owned(),
        ));
    }
    let roof_bearings_valid = !frame.roof_bearing_interfaces.is_empty()
        && frame.roof_bearing_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                nodes.get(&interface.node).is_some_and(|node| {
                    node.kind == crate::StructuralNodeKind::TimberFrameRoofBearing
                }) && frame.members.iter().any(|member| {
                    solids.get(&member.solid).is_some_and(|solid| {
                        resolved_solid_overlaps_bounds(
                            solid,
                            (interface.bounds.min, interface.bounds.max),
                            0.001,
                        )
                    })
                }) && plan.roof_assemblies.iter().any(|roof| {
                    roof.support_nodes.iter().any(|roof_node_id| {
                        nodes.get(roof_node_id).is_some_and(|roof_node| {
                            roof_node.supported_by.contains(&interface.node)
                                && roof_node.position.x >= interface.bounds.min.x - 0.001
                                && roof_node.position.x <= interface.bounds.max.x + 0.001
                                && roof_node.position.y >= interface.bounds.min.y - 0.001
                                && roof_node.position.y <= interface.bounds.max.y + 0.001
                                && roof_node.position.z >= interface.bounds.min.z - 0.001
                                && roof_node.position.z <= interface.bounds.max.z + 0.001
                        })
                    })
                })
            })
        });
    if !roof_bearings_valid
        || (!plan.roof_dormers.is_empty()
            && (frame.dormer_trimmer_members.len() < plan.roof_dormers.len() * 4
                || frame.dormer_trimmer_members.iter().any(|id| {
                    members
                        .get(id)
                        .is_none_or(|member| member.role != crate::TimberMemberRole::DormerTrimmer)
                })))
    {
        issues.push(issue(
            "severed_timber_roof_bearing",
            "timber frame lacks exact roof seats or dormer trimmers".to_owned(),
        ));
    }
    let roof_envelope_intrusions = timber_roof_envelope_intrusions(plan);
    if !roof_envelope_intrusions.is_empty() {
        issues.push(issue(
            "timber_intrudes_through_roof",
            format!(
                "roof-construction members leave the authoritative roof envelope: {:?}",
                roof_envelope_intrusions
            ),
        ));
    }
    let exposed_child_supports = exposed_roof_child_support_posts(plan);
    if !exposed_child_supports.is_empty() {
        issues.push(issue(
            "exposed_roof_child_support",
            format!(
                "roof children contain freestanding generic support posts outside their declared curb/front/cheek authority: {:?}",
                exposed_child_supports
            ),
        ));
    }
    let oversized_child_flashings = oversized_child_roof_flashings(plan);
    if !oversized_child_flashings.is_empty() {
        issues.push(issue(
            "invalid_child_roof_flashing_profile",
            format!(
                "child-roof flashing rises above the seated civilian seam profile: {:?}",
                oversized_child_flashings
            ),
        ));
    }
    let invalid_child_drainage = invalid_attached_child_drainage(plan);
    if !invalid_child_drainage.is_empty() {
        issues.push(issue(
            "invalid_child_roof_drainage",
            format!(
                "attached civilian roof drains bypass the containing parent weather face: {:?}",
                invalid_child_drainage
            ),
        ));
    }
    let invalid_child_curb = invalid_dormer_trimmer_envelope(plan);
    if !invalid_child_curb.is_empty() {
        issues.push(issue(
            "invalid_dormer_trimmer_envelope",
            format!(
                "dormer trimmers project outside the exact front-to-rear child enclosure: {:?}",
                invalid_child_curb
            ),
        ));
    }
    let oversized_child_gutters = oversized_attached_child_gutters(plan);
    if !oversized_child_gutters.is_empty() {
        issues.push(issue(
            "invalid_child_roof_drainage_profile",
            format!(
                "attached child roof uses a full-building gutter profile: {:?}",
                oversized_child_gutters
            ),
        ));
    }
    let unseated_dormers = unseated_gabled_dormer_roofs(plan);
    if !unseated_dormers.is_empty() {
        issues.push(issue(
            "unseated_dormer_roof",
            format!(
                "gabled dormer retains a free rear verge or misses the parent weather plane: {:?}",
                unseated_dormers
            ),
        ));
    }
    let coplanar_openings = coplanar_timber_opening_faces(plan);
    if !coplanar_openings.is_empty() {
        issues.push(issue(
            "coplanar_timber_opening_face",
            format!(
                "timber-wall opening solids reach the exposed frame plane: {:?}",
                coplanar_openings
            ),
        ));
    }
    let undeclared_intersections = undeclared_timber_intersections(plan);
    if !undeclared_intersections.is_empty() {
        let mut role_counts = std::collections::BTreeMap::<String, usize>::new();
        for (left, right) in &undeclared_intersections {
            let roles = [left, right].map(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
                    .map_or("missing".to_owned(), |solid| format!("{:?}", solid.role))
            });
            *role_counts
                .entry(format!("{} x {}", roles[0], roles[1]))
                .or_default() += 1;
        }
        let mut seen_roles = std::collections::HashSet::new();
        let sample = undeclared_intersections
            .iter()
            .filter(|(left, right)| {
                let key = [left, right]
                    .into_iter()
                    .filter_map(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == *id)
                    })
                    .map(|solid| format!("{:?}", solid.role))
                    .collect::<Vec<_>>()
                    .join(" x ");
                seen_roles.insert(key)
            })
            .take(20)
            .map(|(left, right)| {
                let describe = |id: ResolvedItemId| {
                    plan.resolved_geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == id)
                        .map(|solid| (id, solid.role, solid.owner, solid.centre, solid.size))
                };
                (describe(*left), describe(*right))
            })
            .collect::<Vec<_>>();
        issues.push(issue(
            "undeclared_timber_intersection",
            format!(
                "{} timber pairs overlap without an exact typed joint or bearing interface; roles: {:?}; first pairs (id, role, owner): {:?}",
                undeclared_intersections.len(), role_counts, sample
            ),
        ));
    }
    let dormer_fronts_valid = frame.bays.iter().all(|bay| {
        let Some(wall_id) = bay.wall else {
            return true;
        };
        let Some(wall) = plan.wall_assemblies.iter().find(|wall| wall.id == wall_id) else {
            return false;
        };
        let crate::WallSourceId::RoofChildFront { roof: roof_id } = wall.source else {
            return true;
        };
        let roles = bay
            .member_ids
            .iter()
            .filter_map(|id| members.get(id).map(|member| member.role))
            .collect::<Vec<_>>();
        let is_shed = plan
            .roof_assemblies
            .iter()
            .find(|roof| roof.id == roof_id)
            .is_some_and(|roof| roof.kind == crate::RoofKind::Shed);
        roles
            .iter()
            .filter(|role| **role == crate::TimberMemberRole::IntermediatePost)
            .count()
            >= 2
            && !roles.contains(&crate::TimberMemberRole::PrimaryPost)
            && (is_shed
                || (roles.contains(&crate::TimberMemberRole::GablePost)
                    && roles
                        .iter()
                        .filter(|role| **role == crate::TimberMemberRole::Rafter)
                        .count()
                        >= 2))
    });
    if !dormer_fronts_valid {
        issues.push(issue(
            "invalid_timber_dormer_front",
            "dormer child front lacks jamb-supported gable framing or retains free corner posts"
                .to_owned(),
        ));
    }
    if plan
        .resolved_geometry
        .solids
        .iter()
        .any(|solid| solid.role == SolidRole::FrameMember)
    {
        issues.push(issue(
            "legacy_timber_frame_overlay",
            "accepted civilian fixture still contains legacy viewer/generic framing".to_owned(),
        ));
    }
}

fn audit_church_assembly(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    let Some(church) = &plan.church else {
        if plan.archetype == BuildingArchetype::Cathedral {
            issues.push(issue(
                "missing_church_program",
                "cathedral has no authoritative church assembly".to_owned(),
            ));
        }
        return;
    };
    let program = church.program;
    if plan.archetype != BuildingArchetype::Cathedral
        || program.liturgical_east != Direction::East
        || program.nave_bays != 4
        || program.transept_bays != 1
        || program.choir_bays != 2
        || program.apse_sides != 5
        || program.aisles != 3
    {
        issues.push(issue(
            "invalid_church_program",
            "church is not the frozen east-oriented 4-bay cruciform basilica type".to_owned(),
        ));
    }
    if plan
        .storeys
        .iter()
        .any(|storey| !storey.walls.is_empty() || !storey.openings.is_empty())
    {
        issues.push(issue(
            "legacy_church_authority",
            "church still contains generic cell walls or overlay openings".to_owned(),
        ));
    }
    let strictly_increasing =
        |values: &[f32]| values.windows(2).all(|pair| pair[1] > pair[0] + 0.10);
    if church.nave_axes_metres.len() != usize::from(program.nave_bays)
        || church.choir_axes_metres.len() != usize::from(program.choir_bays)
        || !strictly_increasing(&church.nave_axes_metres)
        || !strictly_increasing(&church.choir_axes_metres)
        || church
            .nave_axes_metres
            .last()
            .is_none_or(|axis| *axis >= church.crossing_axis_metres)
        || church
            .choir_axes_metres
            .first()
            .is_none_or(|axis| *axis <= church.crossing_axis_metres)
    {
        issues.push(issue(
            "invalid_church_bay_axes",
            "nave/crossing/choir axes are missing, unordered, or blocked".to_owned(),
        ));
    }
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<BTreeMap<_, _>>();
    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let openings = plan
        .opening_assemblies
        .iter()
        .map(|opening| (opening.id, opening))
        .collect::<BTreeMap<_, _>>();
    let walls = plan
        .wall_assemblies
        .iter()
        .map(|wall| (wall.id, wall))
        .collect::<BTreeMap<_, _>>();
    let surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<BTreeMap<_, _>>();
    let voids = plan
        .resolved_geometry
        .voids
        .iter()
        .map(|void| (void.id, void))
        .collect::<BTreeMap<_, _>>();
    let interfaces = plan
        .resolved_geometry
        .support_interfaces
        .iter()
        .map(|interface| (interface.id, interface))
        .collect::<BTreeMap<_, _>>();
    let support_solid = |node_id: StructuralNodeId| {
        plan.resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.supported_by.contains(&node_id))
    };
    let true_arch = |solid: &crate::ResolvedSolid| {
        matches!(
            solid.shape,
            crate::ResolvedSolidShape::SegmentalArchRing {
                clear_span_metres,
                rise_metres,
                ..
            } if clear_span_metres >= 0.90 && rise_metres >= 0.35
        ) || matches!(
            solid.shape,
            crate::ResolvedSolidShape::PointedArchRing {
                clear_span_metres,
                spring_height_metres,
                apex_height_metres,
                ..
            } if clear_span_metres >= 0.90 && apex_height_metres - spring_height_metres >= 0.35
        )
    };
    if church.bay_assemblies.len() != usize::from(program.nave_bays)
        || church.bay_assemblies.iter().any(|bay| {
            bay.pier_nodes
                .iter()
                .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
                || bay.pier_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::ChurchPier)
                })
                || bay.arcade_solids.iter().any(|id| {
                    solids.get(id).is_none_or(|arcade| {
                        arcade.role != SolidRole::ChurchArcade
                            || !true_arch(arcade)
                            || arcade.supported_by.len() != 1
                            || nodes.get(&arcade.supported_by[0]).is_none_or(|spring| {
                                spring.grounded
                                    || spring.supported_by.len() != 2
                                    || !spring.supported_by.iter().all(|bearing| {
                                        nodes.get(bearing).is_some_and(|node| node.grounded)
                                    })
                            })
                    })
                })
                || bay
                    .arcade_bearing_nodes
                    .iter()
                    .any(|pair| pair[0] == pair[1])
                || bay
                    .arcade_bearing_interfaces
                    .iter()
                    .enumerate()
                    .any(|(side, pair)| {
                        pair.iter().enumerate().any(|(end, id)| {
                            let Some(interface) = interfaces.get(id) else {
                                return true;
                            };
                            let Some(arcade) = solids.get(&bay.arcade_solids[side]) else {
                                return true;
                            };
                            let bearing_node = bay.arcade_bearing_nodes[side][end];
                            let Some(pier) = support_solid(bearing_node) else {
                                return true;
                            };
                            interface.node != arcade.supported_by[0]
                                || !bounds_overlap_3d(
                                    (interface.bounds.min, interface.bounds.max),
                                    resolved_solid_bounds(arcade),
                                    0.02,
                                )
                                || !bounds_overlap_3d(
                                    (interface.bounds.min, interface.bounds.max),
                                    resolved_solid_bounds(pier),
                                    0.02,
                                )
                        })
                    })
                || bay
                    .buttress_nodes
                    .iter()
                    .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
                || bay.buttress_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::WallButtress)
                })
                || bay.vault_solids.is_empty()
                || bay.vault_solids.iter().any(|id| {
                    solids.get(id).is_none_or(|solid| {
                        solid.role != SolidRole::ChurchVaultShell
                            || solid.supported_by.len() != 1
                            || !bay.vault_spring_nodes.contains(&solid.supported_by[0])
                    })
                })
                || bay.vault_thrust_solids.len() != 4
                || bay.vault_thrust_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::ChurchVaultThrust)
                })
                || bay.vault_load_surfaces.len() != 2
                || bay.vault_load_surfaces.iter().any(|id| {
                    surfaces
                        .get(id)
                        .is_none_or(|surface| surface.role != SurfaceRole::ChurchVaultLoad)
                })
                || bay.vault_spring_nodes.len() != 2
                || bay.vault_spring_nodes.iter().any(|id| {
                    nodes.get(id).is_none_or(|spring| {
                        spring.grounded
                            || spring.supported_by.len() != 4
                            || spring
                                .supported_by
                                .iter()
                                .filter(|support| {
                                    nodes.get(support).is_some_and(|node| {
                                        node.kind == crate::StructuralNodeKind::ChurchPier
                                    })
                                })
                                .count()
                                != 2
                            || spring
                                .supported_by
                                .iter()
                                .filter(|support| {
                                    nodes.get(support).is_some_and(|node| {
                                        node.kind == crate::StructuralNodeKind::ChurchButtress
                                    })
                                })
                                .count()
                                != 2
                            || !spring
                                .supported_by
                                .iter()
                                .all(|support| nodes.get(support).is_some_and(|node| node.grounded))
                    })
                })
                || bay.vault_bearing_interfaces.len() != 8
                || bay.vault_bearing_interfaces.iter().any(|id| {
                    interfaces.get(id).is_none_or(|interface| {
                        !bay.vault_spring_nodes.contains(&interface.node)
                            || !bay.vault_thrust_solids.iter().any(|solid_id| {
                                solids.get(solid_id).is_some_and(|solid| {
                                    bounds_overlap_3d(
                                        (interface.bounds.min, interface.bounds.max),
                                        resolved_solid_bounds(solid),
                                        0.03,
                                    )
                                })
                            })
                    })
                })
        })
    {
        issues.push(issue(
            "invalid_church_bay_structure",
            "a nave bay lacks paired piers, arcades, buttresses, or vault load structure"
                .to_owned(),
        ));
    }
    let church_windows = plan
        .opening_assemblies
        .iter()
        .filter(|opening| {
            opening.use_kind == crate::OpeningUse::Window
                && matches!(
                    opening.host_source,
                    crate::WallSourceId::ChurchExterior { .. }
                        | crate::WallSourceId::ChurchArcade { .. }
                        | crate::WallSourceId::ChurchApse { .. }
                )
        })
        .collect::<Vec<_>>();
    let expected_windows = usize::from(program.nave_bays) * 4
        + usize::from(program.choir_bays) * 2
        + 2
        + usize::from(program.apse_sides.saturating_sub(1));
    let clerestory_is_bay_bound = church.bay_assemblies.iter().all(|bay| {
        [Direction::South, Direction::North]
            .into_iter()
            .zip(bay.clerestory_openings)
            .all(|(side, id)| {
                openings.get(&id).is_some_and(|opening| {
                    opening.host_source
                        == crate::WallSourceId::ChurchArcade {
                            side,
                            bay: bay.axis_index,
                        }
                        && opening.closure.layers == [crate::ClosureKind::LeadedGlazing]
                        && matches!(
                            opening.profile,
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                apex_height_metres,
                                ..
                            } if width_metres >= 1.20 && apex_height_metres >= 2.20
                        )
                })
            })
    });
    let transept_has_principal_lights =
        [Direction::South, Direction::North]
            .into_iter()
            .all(|side| {
                church_windows.iter().any(|opening| {
                    opening.host_source
                        == crate::WallSourceId::ChurchExterior {
                            range: crate::ChurchRange::Transept,
                            side,
                            bay: 0,
                        }
                        && matches!(
                            opening.profile,
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                apex_height_metres,
                                ..
                            } if width_metres >= 2.20 && apex_height_metres >= 7.40
                        )
                })
            });
    if church_windows.len() != expected_windows
        || !clerestory_is_bay_bound
        || !transept_has_principal_lights
        || church_windows.iter().any(|opening| {
            opening.tracery_node.is_none()
                || opening.closure_solids.len() != 2
                || opening.closure.layers != [crate::ClosureKind::LeadedGlazing]
        })
    {
        issues.push(issue(
            "invalid_church_window_hierarchy",
            "church lights are not pointed, bay-aligned, stone-divided, and hierarchically scaled"
                .to_owned(),
        ));
    }
    let crossing_arches_valid =
        church
            .crossing
            .arch_solids
            .iter()
            .enumerate()
            .all(|(arch_index, id)| {
                let Some(arch) = solids.get(id) else {
                    return false;
                };
                if arch.role != SolidRole::ChurchCrossingArch
                    || !true_arch(arch)
                    || arch.supported_by.len() != 1
                {
                    return false;
                }
                let Some(spring) = nodes.get(&arch.supported_by[0]) else {
                    return false;
                };
                let bearings = church.crossing.arch_bearing_nodes[arch_index];
                spring.supported_by.len() == 2
                    && bearings[0] != bearings[1]
                    && spring
                        .supported_by
                        .iter()
                        .all(|node| bearings.contains(node))
                    && church.crossing.arch_bearing_interfaces[arch_index]
                        .iter()
                        .enumerate()
                        .all(|(end, id)| {
                            interfaces.get(id).is_some_and(|interface| {
                                interface.node == spring.id
                                    && bounds_overlap_3d(
                                        (interface.bounds.min, interface.bounds.max),
                                        resolved_solid_bounds(arch),
                                        0.02,
                                    )
                                    && support_solid(bearings[end]).is_some_and(|bearing| {
                                        bounds_overlap_3d(
                                            (interface.bounds.min, interface.bounds.max),
                                            resolved_solid_bounds(bearing),
                                            0.02,
                                        )
                                    })
                            })
                        })
            });
    let crossing_load_valid = church
        .crossing
        .buttress_nodes
        .iter()
        .all(|id| nodes.get(id).is_some_and(|node| node.grounded))
        && church.crossing.buttress_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::WallButtress)
        })
        && church.crossing.vault_thrust_solids.len() == 4
        && church.crossing.vault_thrust_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::ChurchVaultThrust)
        })
        && church.crossing.vault_load_surfaces.len() == 1
        && church.crossing.vault_load_surfaces.iter().all(|id| {
            surfaces
                .get(id)
                .is_some_and(|surface| surface.role == SurfaceRole::ChurchVaultLoad)
        })
        && church.crossing.vault_spring_nodes.len() == 1
        && church.crossing.vault_spring_nodes.iter().all(|id| {
            nodes.get(id).is_some_and(|spring| {
                !spring.grounded
                    && spring.supported_by.len() == 8
                    && spring
                        .supported_by
                        .iter()
                        .all(|support| nodes.get(support).is_some_and(|node| node.grounded))
            })
        })
        && church.crossing.vault_bearing_interfaces.len() == 8
        && church.crossing.vault_bearing_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                church.crossing.vault_spring_nodes.contains(&interface.node)
                    && church.crossing.vault_thrust_solids.iter().any(|solid_id| {
                        solids.get(solid_id).is_some_and(|solid| {
                            bounds_overlap_3d(
                                (interface.bounds.min, interface.bounds.max),
                                resolved_solid_bounds(solid),
                                0.03,
                            )
                        })
                    })
            })
        });
    if church
        .crossing
        .pier_nodes
        .iter()
        .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
        || !crossing_arches_valid
        || church.crossing.vault_solids.is_empty()
        || church.crossing.vault_solids.iter().any(|id| {
            solids.get(id).is_none_or(|vault| {
                vault.role != SolidRole::ChurchVaultShell
                    || vault.supported_by.len() != 1
                    || !church
                        .crossing
                        .vault_spring_nodes
                        .contains(&vault.supported_by[0])
            })
        })
        || !crossing_load_valid
    {
        issues.push(issue(
            "invalid_church_crossing",
            "crossing lacks four grounded piers, four arches, or a closed vault".to_owned(),
        ));
    }
    let choir_arches_valid = church.choir.arch_solids.len() == usize::from(program.choir_bays) * 2
        && church.choir.arch_bearing_nodes.len() == church.choir.arch_solids.len()
        && church.choir.arch_bearing_interfaces.len() == church.choir.arch_solids.len()
        && church
            .choir
            .arch_solids
            .iter()
            .enumerate()
            .all(|(index, id)| {
                let Some(arch) = solids.get(id) else {
                    return false;
                };
                let Some(spring) = arch.supported_by.first().and_then(|id| nodes.get(id)) else {
                    return false;
                };
                let bearings = church.choir.arch_bearing_nodes[index];
                arch.role == SolidRole::ChurchArcade
                    && true_arch(arch)
                    && arch.supported_by.len() == 1
                    && spring.supported_by.len() == 2
                    && spring.supported_by.iter().all(|id| bearings.contains(id))
                    && church.choir.arch_bearing_interfaces[index]
                        .iter()
                        .enumerate()
                        .all(|(end, id)| {
                            interfaces.get(id).is_some_and(|interface| {
                                interface.node == spring.id
                                    && bounds_overlap_3d(
                                        (interface.bounds.min, interface.bounds.max),
                                        resolved_solid_bounds(arch),
                                        0.02,
                                    )
                                    && support_solid(bearings[end]).is_some_and(|bearing| {
                                        bounds_overlap_3d(
                                            (interface.bounds.min, interface.bounds.max),
                                            resolved_solid_bounds(bearing),
                                            0.02,
                                        )
                                    })
                            })
                        })
            });
    let choir_load_valid = church.choir.vault_thrust_solids.len()
        == usize::from(program.choir_bays) * 4
        && church.choir.vault_load_surfaces.len() == usize::from(program.choir_bays)
        && church.choir.vault_spring_nodes.len() == usize::from(program.choir_bays) * 2
        && church.choir.vault_bearing_interfaces.len() == usize::from(program.choir_bays) * 8
        && church.choir.vault_thrust_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::ChurchVaultThrust)
        })
        && church.choir.vault_load_surfaces.iter().all(|id| {
            surfaces
                .get(id)
                .is_some_and(|surface| surface.role == SurfaceRole::ChurchVaultLoad)
        })
        && church.choir.vault_spring_nodes.iter().all(|id| {
            nodes.get(id).is_some_and(|spring| {
                !spring.grounded
                    && spring.supported_by.len() == 4
                    && spring
                        .supported_by
                        .iter()
                        .filter(|id| {
                            nodes.get(id).is_some_and(|node| {
                                node.kind == crate::StructuralNodeKind::ChurchPier
                                    || node.kind == crate::StructuralNodeKind::ChurchCrossingPier
                            })
                        })
                        .count()
                        == 2
                    && spring
                        .supported_by
                        .iter()
                        .filter(|id| {
                            nodes.get(id).is_some_and(|node| {
                                node.kind == crate::StructuralNodeKind::ChurchButtress
                            })
                        })
                        .count()
                        == 2
            })
        })
        && church.choir.vault_bearing_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                church.choir.vault_spring_nodes.contains(&interface.node)
                    && church.choir.vault_thrust_solids.iter().any(|solid_id| {
                        solids.get(solid_id).is_some_and(|solid| {
                            bounds_overlap_3d(
                                (interface.bounds.min, interface.bounds.max),
                                resolved_solid_bounds(solid),
                                0.03,
                            )
                        })
                    })
            })
        });
    if church.choir.apse_facets.len() != usize::from(program.apse_sides)
        || church.choir.radial_buttress_nodes.len() != usize::from(program.apse_sides)
        || church
            .choir
            .radial_buttress_nodes
            .iter()
            .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
        || church.choir.pier_nodes.len() != usize::from(program.choir_bays) * 2
        || church
            .choir
            .pier_nodes
            .iter()
            .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
        || church.choir.buttress_nodes.len() != usize::from(program.choir_bays) * 2
        || church.choir.vault_solids.len() != usize::from(program.choir_bays) * 2
        || church.choir.floor_solids.is_empty()
        || !choir_arches_valid
        || !choir_load_valid
    {
        issues.push(issue(
            "invalid_church_choir_apse",
            "choir/apse lacks continuous five-sided wall, radial support, or floor authority"
                .to_owned(),
        ));
    }
    let portal_valid = |id: crate::OpeningAssemblyId| {
        openings.get(&id).is_some_and(|opening| {
            opening.use_kind == crate::OpeningUse::Door
                && opening.profile.interior_width_metres() >= 0.90
                && opening.profile.clear_height_metres() >= 1.90
        })
    };
    let stair_valid = plan.stairs.get(church.tower.stair_index).is_some_and(|stair| {
        matches!(stair, Stair::Spiral { base_height_metres, rise_metres, .. }
            if *base_height_metres <= 0.01
                && (*base_height_metres + *rise_metres - church.datum.bell_floor_metres).abs() <= 0.05)
    });
    if !portal_valid(church.tower.west_portal)
        || !portal_valid(church.tower.nave_passage)
        || !stair_valid
        || church.tower.landing_solids.len() < 3
        || church.tower.guard_solids.len() < 5
        || church.tower.bell_floor_solids.len() != 4
        || church.tower.bell_floor_solids.iter().any(|id| {
            solids
                .get(id)
                .is_none_or(|solid| solid.role != SolidRole::ChurchBellFloor)
        })
        || church.tower.roof_ladder_solids.len() < 15
        || church.tower.roof_ladder_solids.iter().any(|id| {
            solids
                .get(id)
                .is_none_or(|solid| solid.role != SolidRole::ChurchServiceLadder)
        })
        || church.tower.bell_frame_solids.len() < 2
        || solids
            .get(&church.tower.bell_solid)
            .is_none_or(|solid| solid.role != SolidRole::ChurchBell)
        || church.tower.bell_openings.len() != 8
        || church.tower.bell_openings.iter().any(|id| {
            openings.get(id).is_none_or(|opening| {
                opening.use_kind != crate::OpeningUse::BellOpening
                    || opening.closure.layers != vec![crate::ClosureKind::TimberLouvre]
                    || opening
                        .closure
                        .layers
                        .contains(&crate::ClosureKind::LeadedGlazing)
            })
        })
    {
        issues.push(issue(
            "invalid_church_west_tower",
            "west tower lacks portal/passage, guarded service stair, bell floor/frame/bell, or paired unglazed louvres".to_owned(),
        ));
    }
    let stairwell_bounds = (
        Vec3::new(
            church.tower.centre.x - 1.21,
            church.datum.bell_floor_metres - 0.20,
            church.tower.centre.y - 1.21,
        ),
        Vec3::new(
            church.tower.centre.x + 1.21,
            church.datum.bell_floor_metres + 0.20,
            church.tower.centre.y + 1.21,
        ),
    );
    let floor_area = church
        .tower
        .bell_floor_solids
        .iter()
        .filter_map(|id| solids.get(id))
        .map(|solid| solid.size.x * solid.size.z)
        .sum::<f32>();
    let floor_blocks_stairwell = church.tower.bell_floor_solids.iter().any(|id| {
        solids.get(id).is_some_and(|solid| {
            bounds_overlap_3d(resolved_solid_bounds(solid), stairwell_bounds, 0.005)
        })
    });
    let ladder_bounds = church
        .tower
        .roof_ladder_solids
        .iter()
        .filter_map(|id| solids.get(id))
        .fold(None, |bounds, solid| {
            let (min, max) = resolved_solid_bounds(solid);
            Some(
                bounds.map_or((min, max), |(old_min, old_max): (Vec3, Vec3)| {
                    (old_min.min(min), old_max.max(max))
                }),
            )
        });
    let tower_wall_supports = church
        .tower
        .wall_ids
        .iter()
        .filter_map(|id| walls.get(id))
        .map(|wall| wall.support_node)
        .collect::<BTreeSet<_>>();
    let bell_floor_bearing_valid = church.tower.bell_floor_solids.iter().all(|id| {
        solids.get(id).is_some_and(|solid| {
            solid.supported_by.iter().all(|support| {
                nodes.get(support).is_some_and(|stage| {
                    !stage.grounded
                        && stage.supported_by.len() >= 2
                        && stage
                            .supported_by
                            .iter()
                            .all(|wall| tower_wall_supports.contains(wall))
                })
            })
        })
    });
    if floor_blocks_stairwell
        || !(11.5..=12.6).contains(&floor_area)
        || !bell_floor_bearing_valid
        || ladder_bounds.is_none_or(|(min, max)| {
            min.y > church.datum.bell_floor_metres + 0.25
                || max.y < 21.25
                || min.x < church.tower.centre.x - church.tower.footprint_size_metres.x * 0.5
                || max.x > church.tower.centre.x + church.tower.footprint_size_metres.x * 0.5
                || min.z < church.tower.centre.y - church.tower.footprint_size_metres.y * 0.5
                || max.z > church.tower.centre.y + church.tower.footprint_size_metres.y * 0.5
        })
    {
        issues.push(issue(
            "invalid_church_tower_service_geometry",
            "bell floor must be a tower-wall-bearing guarded ring around a clear stairwell with a contained floor-to-roof service ladder".to_owned(),
        ));
    }
    let item_bounds = |id: ResolvedItemId| {
        solids
            .get(&id)
            .map(|solid| resolved_solid_bounds(solid))
            .or_else(|| {
                surfaces
                    .get(&id)
                    .map(|surface| (surface.bounds.min, surface.bounds.max))
            })
    };
    let bounds_gap = |a: (Vec3, Vec3), b: (Vec3, Vec3)| {
        let axis_gap = |a_min: f32, a_max: f32, b_min: f32, b_max: f32| {
            (b_min - a_max).max(a_min - b_max).max(0.0)
        };
        Vec3::new(
            axis_gap(a.0.x, a.1.x, b.0.x, b.1.x),
            axis_gap(a.0.y, a.1.y, b.0.y, b.1.y),
            axis_gap(a.0.z, a.1.z, b.0.z, b.1.z),
        )
    };
    let route_surface_point = |id: ResolvedItemId| {
        item_bounds(id)
            .map(|(min, max)| Vec3::new((min.x + max.x) * 0.5, max.y, (min.z + max.z) * 0.5))
    };
    let route_crosses_opening = |edge: &crate::ChurchRouteEdge| {
        let Some(opening_id) = edge.through_opening else {
            return true;
        };
        let Some(((opening, wall), void)) = openings
            .get(&opening_id)
            .zip(
                openings
                    .get(&opening_id)
                    .and_then(|opening| walls.get(&opening.host_wall)),
            )
            .zip(
                openings
                    .get(&opening_id)
                    .and_then(|opening| voids.get(&opening.void_id)),
            )
        else {
            return false;
        };
        if edge.from == edge.to
            || edge.clear_width_metres > opening.profile.interior_width_metres() + 0.001
            || edge.clear_headroom_metres > opening.profile.clear_height_metres() + 0.001
            || opening.sectional_void.len() < 5
        {
            return false;
        }
        let Some((from, to)) = route_surface_point(edge.from).zip(route_surface_point(edge.to))
        else {
            return false;
        };
        let travel = Vec2::new(to.x - from.x, to.z - from.z);
        let along_outward = travel.dot(opening.frame.outward);
        if along_outward.abs() < wall.thickness_metres * 0.5 {
            return false;
        }
        opening.sectional_void.iter().all(|slice| {
            let plane = opening.frame.origin
                + opening.frame.outward * wall.thickness_metres * (0.5 - slice.depth_fraction);
            let t = (plane - Vec2::new(from.x, from.z)).dot(opening.frame.outward) / along_outward;
            if !(-0.001..=1.001).contains(&t) {
                return false;
            }
            let foot = from.lerp(to, t.clamp(0.0, 1.0));
            let plan_point = Vec2::new(foot.x, foot.z);
            let lateral = (plan_point - opening.frame.origin)
                .dot(opening.frame.tangent)
                .abs();
            let inside_void_envelope = foot.x >= void.bounds.min.x - 0.005
                && foot.x <= void.bounds.max.x + 0.005
                && foot.z >= void.bounds.min.z - 0.005
                && foot.z <= void.bounds.max.z + 0.005
                && foot.y >= void.bounds.min.y - 0.005
                && foot.y + edge.clear_headroom_metres <= void.bounds.max.y + 0.005;
            lateral + edge.clear_width_metres * 0.5 <= slice.width_metres * 0.5 + 0.005
                && foot.y >= opening.sill_elevation_metres - 0.005
                && foot.y + edge.clear_headroom_metres
                    <= opening.sill_elevation_metres + slice.height_metres + 0.005
                && inside_void_envelope
        })
    };
    let route_contract_invalid = church.circulation.iter().any(|route| {
        if route.width_metres < 0.90
            || route.headroom_metres < 1.90
            || route.waypoints.len() < 2
            || route
                .surface_ids
                .iter()
                .any(|id| !surfaces.contains_key(id))
            || route
                .traversable_solid_ids
                .iter()
                .any(|id| !solids.contains_key(id))
            || route.edges.is_empty()
        {
            return true;
        }
        let allowed = route
            .surface_ids
            .iter()
            .chain(&route.traversable_solid_ids)
            .copied()
            .collect::<BTreeSet<_>>();
        if route.edges.iter().any(|edge| {
            edge.clear_width_metres < 0.90
                || edge.clear_headroom_metres < 1.90
                || !allowed.contains(&edge.from)
                || !allowed.contains(&edge.to)
                || edge.through_opening.is_some_and(|opening| {
                    !route.opening_ids.contains(&opening) || !openings.contains_key(&opening)
                })
                || !route_crosses_opening(edge)
                || (edge.through_opening.is_none()
                    && item_bounds(edge.from)
                        .zip(item_bounds(edge.to))
                        .is_none_or(|(from, to)| bounds_gap(from, to).length() > 0.62))
        }) {
            return true;
        }
        let mut adjacency = BTreeMap::<ResolvedItemId, Vec<ResolvedItemId>>::new();
        for edge in &route.edges {
            adjacency.entry(edge.from).or_default().push(edge.to);
            adjacency.entry(edge.to).or_default().push(edge.from);
        }
        let Some(start) = route.surface_ids.first().copied() else {
            return true;
        };
        let mut reached = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            for next in adjacency.get(&current).into_iter().flatten() {
                if reached.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        !allowed.is_subset(&reached)
    });
    let public_route_invalid = church
        .circulation
        .iter()
        .find(|route| route.kind == crate::ChurchRouteKind::PublicProcessional)
        .is_none_or(|route| {
            let expected_surfaces = [
                church.tower.exterior_approach_surface,
                church.tower.vestibule_surface,
                church.tower.nave_entry_surface,
            ];
            expected_surfaces
                .iter()
                .any(|id| !route.surface_ids.contains(id))
                || route.width_metres > 1.80 + 0.001
                || !route.edges.iter().any(|edge| {
                    edge.from == church.tower.exterior_approach_surface
                        && edge.to == church.tower.vestibule_surface
                        && edge.through_opening == Some(church.tower.west_portal)
                })
                || !route.edges.iter().any(|edge| {
                    edge.from == church.tower.vestibule_surface
                        && edge.to == church.tower.nave_entry_surface
                        && edge.through_opening == Some(church.tower.nave_passage)
                })
        });
    let bell_route_invalid = church
        .circulation
        .iter()
        .find(|route| route.kind == crate::ChurchRouteKind::BellService)
        .is_none_or(|route| {
            let ladder_rungs = church.tower.roof_ladder_solids.iter().skip(2);
            let route_degree = |id: ResolvedItemId| {
                route
                    .edges
                    .iter()
                    .filter(|edge| edge.from == id || edge.to == id)
                    .count()
            };
            let tower_wall_solids = church
                .tower
                .wall_ids
                .iter()
                .filter_map(|id| walls.get(id))
                .flat_map(|wall| wall.host_solids.iter())
                .filter_map(|id| solids.get(id))
                .collect::<Vec<_>>();
            let bell_obstacles = church
                .tower
                .bell_frame_solids
                .iter()
                .chain(std::iter::once(&church.tower.bell_solid))
                .filter_map(|id| solids.get(id))
                .collect::<Vec<_>>();
            let stair_bearing_invalid =
                nodes
                    .get(&church.tower.stair_bearing_node)
                    .is_none_or(|bearing| {
                        bearing.grounded
                            || bearing.supported_by.len() < 2
                            || bearing
                                .supported_by
                                .iter()
                                .any(|support| !tower_wall_supports.contains(support))
                    })
                    || solids
                        .get(&church.tower.stair_newel_solid)
                        .is_none_or(|newel| {
                            newel.role != SolidRole::ChurchStairNewel
                                || newel.supported_by != vec![church.tower.stair_bearing_node]
                        })
                    || church.tower.stair_tread_interfaces.len()
                        != church.tower.stair_tread_solids.len()
                    || church
                        .tower
                        .stair_tread_solids
                        .iter()
                        .zip(&church.tower.stair_tread_interfaces)
                        .any(|(tread_id, interface_id)| {
                            solids
                                .get(tread_id)
                                .zip(interfaces.get(interface_id))
                                .zip(solids.get(&church.tower.stair_newel_solid))
                                .is_none_or(|((tread, interface), newel)| {
                                    interface.node != church.tower.stair_bearing_node
                                        || tread.supported_by
                                            != vec![church.tower.stair_bearing_node]
                                        || !resolved_solid_overlaps_bounds(
                                            tread,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.005,
                                        )
                                        || !resolved_solid_overlaps_bounds(
                                            newel,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.005,
                                        )
                                })
                        });
            let route_point = |id: ResolvedItemId| {
                solids
                    .get(&id)
                    .map(|solid| {
                        Vec3::new(
                            solid.centre.x,
                            resolved_solid_bounds(solid).1.y + 0.015,
                            solid.centre.z,
                        )
                    })
                    .or_else(|| route_surface_point(id).map(|point| point + Vec3::Y * 0.015))
            };
            let route_items = route
                .surface_ids
                .iter()
                .chain(&route.traversable_solid_ids)
                .copied()
                .collect::<BTreeSet<_>>();
            let sweep_obstacles = tower_wall_solids
                .iter()
                .copied()
                .chain(
                    church
                        .tower
                        .guard_solids
                        .iter()
                        .filter_map(|id| solids.get(id)),
                )
                .chain(bell_obstacles.iter().copied())
                .chain(solids.get(&church.tower.stair_newel_solid))
                .collect::<Vec<_>>();
            let swept_route_invalid = route.edges.iter().any(|edge| {
                route_point(edge.from)
                    .zip(route_point(edge.to))
                    .is_none_or(|(from, to)| {
                        // Seven samples per adjacency are sufficient for the
                        // project's coarse 0.34 m treads while also sampling
                        // both ends and the turn chord.  The 0.90 x 1.90 m
                        // prism is the animation/collision gate, not a claim
                        // about universal medieval stair dimensions.
                        let travel = Vec2::new(to.x - from.x, to.z - from.z);
                        let along = if travel.length_squared() > 0.0001 {
                            travel.normalize()
                        } else {
                            Vec2::X
                        };
                        let across = Vec2::new(-along.y, along.x);
                        (0..=6).any(|sample| {
                            let t = sample as f32 / 6.0;
                            let foot = from.lerp(to, t);
                            sweep_obstacles.iter().any(|obstacle| {
                                !route_items.contains(&obstacle.id)
                                    && oriented_occupant_overlaps_solid(
                                        foot, along, across, obstacle, 0.015,
                                    )
                            })
                        })
                    })
            });
            stair_bearing_invalid
                || swept_route_invalid
                || church.tower.stair_tread_solids.len() != 72
                || !route
                    .traversable_solid_ids
                    .contains(&church.tower.stair_tread_solids[0])
                || church.tower.stair_tread_solids.iter().any(|id| {
                    solids.get(id).is_none_or(|solid| {
                        solid.role != SolidRole::ChurchStairTread
                            || solid.size.x < 0.90
                            || solid.centre.x
                                < church.tower.centre.x - church.tower.footprint_size_metres.x * 0.5
                                    + 0.90
                            || solid.centre.x
                                > church.tower.centre.x + church.tower.footprint_size_metres.x * 0.5
                                    - 0.90
                            || solid.centre.z
                                < church.tower.centre.y - church.tower.footprint_size_metres.y * 0.5
                                    + 0.90
                            || solid.centre.z
                                > church.tower.centre.y + church.tower.footprint_size_metres.y * 0.5
                                    - 0.90
                            || tower_wall_solids.iter().any(|wall| {
                                bounds_overlap_3d(
                                    resolved_solid_bounds(solid),
                                    resolved_solid_bounds(wall),
                                    0.01,
                                )
                            })
                    })
                })
                || church
                    .tower
                    .stair_tread_solids
                    .iter()
                    .zip(church.tower.stair_tread_solids.iter().skip(18))
                    .any(|(lower, upper)| {
                        solids
                            .get(lower)
                            .zip(solids.get(upper))
                            .is_none_or(|(a, b)| b.centre.y - a.centre.y < 1.90)
                    })
                || church
                    .tower
                    .landing_solids
                    .iter()
                    .chain(&church.tower.bell_floor_solids)
                    .chain(ladder_rungs)
                    .any(|id| !route.traversable_solid_ids.contains(id))
                || church
                    .tower
                    .bell_floor_corner_surfaces
                    .iter()
                    .any(|id| !route.surface_ids.contains(id))
                || church
                    .tower
                    .landing_solids
                    .iter()
                    .any(|id| route_degree(*id) < 2)
                || church.tower.roof_ladder_solids.iter().skip(2).any(|id| {
                    solids.get(id).is_none_or(|rung| {
                        bell_obstacles.iter().any(|obstacle| {
                            bounds_overlap_3d(
                                resolved_solid_bounds(rung),
                                resolved_solid_bounds(obstacle),
                                0.02,
                            )
                        })
                    })
                })
                || !route
                    .surface_ids
                    .contains(&church.tower.roof_service_surface)
                || !route.surface_ids.contains(&church.tower.vestibule_surface)
                || route.opening_ids.contains(&church.tower.nave_passage)
                || !route.edges.iter().any(|edge| {
                    edge.through_opening.is_none()
                        && edge.from == church.tower.vestibule_surface
                        && edge.to == church.tower.stair_tread_solids[0]
                })
        });
    if church.circulation.len() < 2
        || route_contract_invalid
        || public_route_invalid
        || bell_route_invalid
    {
        issues.push(issue(
            "invalid_church_circulation",
            format!(
                "public or bell-service circulation lacks an adjacent, swept 0.90 x 1.90 m route across its physical surfaces (contract={route_contract_invalid}, public={public_route_invalid}, bell={bell_route_invalid})"
            ),
        ));
    }
    if church.roof_assemblies.len() < 6
        || church
            .roof_assemblies
            .iter()
            .any(|id| !plan.roof_assemblies.iter().any(|roof| roof.id == *id))
    {
        issues.push(issue(
            "invalid_church_roof_program",
            "church nave/aisle/transept/apse/tower roofs are not bound to Stage4 assemblies"
                .to_owned(),
        ));
    }
}

fn audit_roof_assemblies(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    let mut assembly_ids = BTreeSet::new();
    let mut item_ids = BTreeSet::new();
    let all_face_ids = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| roof.faces.iter().map(|face| face.id))
        .collect::<BTreeSet<_>>();
    for assembly in &plan.roof_assemblies {
        if !assembly_ids.insert(assembly.id) {
            issues.push(issue(
                "duplicate_roof_assembly",
                format!("roof assembly {} is duplicated", assembly.id.0),
            ));
        }
        if assembly.outer_loop.vertices.len() < 3 {
            issues.push(issue(
                "invalid_roof_footprint",
                format!("roof {} has an invalid outer loop", assembly.id.0),
            ));
        }
        if !(15.0..=75.0).contains(
            &assembly
                .faces
                .first()
                .map_or(0.0, |face| face.pitch_degrees),
        ) {
            issues.push(issue(
                "invalid_roof_pitch",
                format!(
                    "roof {} is outside the 15-75 degree project interval",
                    assembly.id.0
                ),
            ));
        }
        if assembly.faces.is_empty() || assembly.edges.is_empty() {
            issues.push(issue(
                "incomplete_roof_graph",
                format!("roof {} lacks faces or typed edges", assembly.id.0),
            ));
        }
        let outlet_stations = plan
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .filter(|station| station.owner == assembly.owner)
            .collect::<Vec<_>>();
        // Project presentation gate: at most four architecturally located
        // outlets per assembly. This prevents per-facet pipe cages while still
        // permitting one station at each corner of a hipped roof.
        let station_cap = 4;
        let network_ids = plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| network.owner == assembly.owner)
            .map(|network| network.id)
            .collect::<Vec<_>>();
        let assigned = outlet_stations
            .iter()
            .flat_map(|station| station.member_networks.iter().copied())
            .collect::<Vec<_>>();
        if outlet_stations.is_empty()
            || outlet_stations.len() > station_cap
            || network_ids.iter().any(|network| {
                assigned
                    .iter()
                    .filter(|candidate| *candidate == network)
                    .count()
                    != 1
            })
            || assigned
                .iter()
                .any(|network| !network_ids.contains(network))
        {
            issues.push(issue(
                "invalid_roof_outlet_topology",
                format!(
                    "roof {} has {} outlet stations for {} catchments (project cap {station_cap})",
                    assembly.id.0,
                    outlet_stations.len(),
                    network_ids.len()
                ),
            ));
        }
        for treatment in plan.resolved_geometry.solids.iter().filter(|solid| {
            solid.owner == assembly.owner && solid.role == SolidRole::RoofEdgeTreatment
        }) {
            let pitch_cosine = treatment.longfall_radians.cos();
            let axis = Vec3::new(
                treatment.yaw_radians.cos() * pitch_cosine,
                treatment.longfall_radians.sin(),
                treatment.yaw_radians.sin() * pitch_cosine,
            );
            let endpoints = [
                treatment.centre - axis * treatment.size.x * 0.5,
                treatment.centre + axis * treatment.size.x * 0.5,
            ];
            let aligned = treatment.crossfall_radians.abs() <= 0.001
                && assembly.edges.iter().any(|edge| {
                    if !matches!(
                        edge.kind,
                        RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::GableVerge
                    ) {
                        return false;
                    }
                    let edge_delta = edge.end - edge.start;
                    let edge_length_squared = edge_delta.length_squared().max(0.000_001);
                    treatment.size.x <= edge_delta.length() + 0.03
                        && endpoints.iter().all(|point| {
                            let t = ((*point - edge.start).dot(edge_delta) / edge_length_squared)
                                .clamp(0.0, 1.0);
                            let nearest = edge.start + edge_delta * t;
                            point.distance(nearest) <= 0.075
                                && (*point - edge.start).dot(edge_delta) / edge_length_squared
                                    >= -0.02
                                && (*point - edge.start).dot(edge_delta) / edge_length_squared
                                    <= 1.02
                        })
                });
            if !aligned {
                issues.push(issue(
                    "invalid_roof_edge_treatment",
                    format!(
                        "roof edge treatment {} is offset, rotated, or outside its typed source contour",
                        treatment.id.0
                    ),
                ));
            }
        }
        let shed_authority_valid = match (assembly.kind, assembly.shed_high_side) {
            (crate::RoofKind::Shed, Some(crate::Direction::East | crate::Direction::West)) => {
                let high = assembly
                    .faces
                    .iter()
                    .flat_map(|face| &face.polygon)
                    .filter(|point| {
                        let centre = assembly
                            .faces
                            .iter()
                            .flat_map(|face| &face.polygon)
                            .map(|point| point.x)
                            .sum::<f32>()
                            / assembly
                                .faces
                                .iter()
                                .map(|face| face.polygon.len())
                                .sum::<usize>() as f32;
                        if assembly.shed_high_side == Some(crate::Direction::East) {
                            point.x >= centre
                        } else {
                            point.x <= centre
                        }
                    })
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                let low = assembly
                    .faces
                    .iter()
                    .flat_map(|face| &face.polygon)
                    .map(|point| point.y)
                    .fold(f32::INFINITY, f32::min);
                high > low + 0.05
            }
            (crate::RoofKind::Shed, Some(crate::Direction::North | crate::Direction::South)) => {
                let high = assembly
                    .faces
                    .iter()
                    .flat_map(|face| &face.polygon)
                    .filter(|point| {
                        let centre = assembly
                            .faces
                            .iter()
                            .flat_map(|face| &face.polygon)
                            .map(|point| point.z)
                            .sum::<f32>()
                            / assembly
                                .faces
                                .iter()
                                .map(|face| face.polygon.len())
                                .sum::<usize>() as f32;
                        if assembly.shed_high_side == Some(crate::Direction::North) {
                            point.z >= centre
                        } else {
                            point.z <= centre
                        }
                    })
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                let low = assembly
                    .faces
                    .iter()
                    .flat_map(|face| &face.polygon)
                    .map(|point| point.y)
                    .fold(f32::INFINITY, f32::min);
                high > low + 0.05
            }
            (crate::RoofKind::Shed, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        if !shed_authority_valid {
            issues.push(issue(
                "invalid_shed_slope_authority",
                format!(
                    "roof {} has a missing or contradictory high side",
                    assembly.id.0
                ),
            ));
        }
        for face in &assembly.faces {
            if !item_ids.insert(face.id) || face.polygon.len() < 3 || face.thickness_metres <= 0.0 {
                issues.push(issue(
                    "invalid_roof_face",
                    format!(
                        "roof {} has duplicate, open, or zero-thickness face {}",
                        assembly.id.0, face.id.0
                    ),
                ));
            }
            let on_plane = face
                .polygon
                .iter()
                .all(|point| (face.plane.normal.dot(*point) + face.plane.constant).abs() <= 0.003);
            let support_exists = !face.support_nodes.is_empty()
                && face.support_nodes.iter().all(|id| {
                    plan.resolved_geometry
                        .structural_nodes
                        .iter()
                        .any(|node| node.id == *id)
                });
            let catchment = plan
                .resolved_geometry
                .drainage_catchments
                .iter()
                .find(|catchment| {
                    catchment.id == face.drainage_catchment && catchment.walk_solid == face.id
                });
            let drainage_valid = catchment.is_some_and(|catchment| {
                plan.resolved_geometry.drainage_routes.iter().any(|route| {
                    route.id == catchment.outlet_route
                        && route.inlet.y + 0.001 >= route.outlet.y
                        && plan.resolved_geometry.voids.iter().any(|void| {
                            void.id == route.outlet_void && void.role == VoidRole::Drain
                        })
                })
            });
            if !on_plane || !support_exists || !drainage_valid {
                issues.push(issue(
                    "invalid_roof_face_contract",
                    format!(
                        "roof face {} plane/support/drain contract failed",
                        face.id.0
                    ),
                ));
            }
            let networks = plan
                .resolved_geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| network.owner == assembly.owner && network.face == face.id)
                .collect::<Vec<_>>();
            let network_valid = !networks.is_empty()
                && networks.iter().all(|network| {
                    let edge = assembly
                        .edges
                        .iter()
                        .find(|edge| edge.id == network.receiving_edge);
                    let floor = plan
                        .resolved_geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == network.channel_floor);
                    let lips_exist = network.channel_lips.iter().all(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .any(|solid| solid.id == *id && solid.role == SolidRole::RoofGutter)
                    });
                    let station =
                        plan.resolved_geometry
                            .roof_drainage_outlets
                            .iter()
                            .find(|station| {
                                station.id == network.outlet_station
                                    && station.owner == assembly.owner
                                    && station.member_networks.contains(&network.id)
                                    && station.outlet_void == network.outlet_void
                                    && station.downspout == network.downspout
                            });
                    let outlet = plan.resolved_geometry.voids.iter().find(|void| {
                        void.id == network.outlet_void
                            && void.owner == assembly.owner
                            && void.role == VoidRole::Drain
                    });
                    let collector_valid = network.collector_solids.iter().all(|id| {
                        plan.resolved_geometry.solids.iter().any(|solid| {
                            solid.id == *id
                                && solid.role == SolidRole::RoofGutter
                                && solid.longfall_radians < -0.001
                        })
                    });
                    let collector_connects_outlet = outlet.is_some_and(|outlet| {
                        let outlet_centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
                        network.collector_solids.iter().any(|id| {
                            plan.resolved_geometry.solids.iter().any(|solid| {
                                if solid.id != *id {
                                    return false;
                                }
                                let tangent = Vec3::new(
                                    solid.yaw_radians.cos(),
                                    0.0,
                                    solid.yaw_radians.sin(),
                                );
                                let half_run = solid.size.x * 0.5;
                                let half_drop = solid.longfall_radians.sin() * half_run;
                                let start = solid.centre - tangent * half_run - Vec3::Y * half_drop;
                                let end = solid.centre + tangent * half_run + Vec3::Y * half_drop;
                                (start.distance(network.channel_low) <= 0.12
                                    && end.distance(outlet_centre) <= 0.12)
                                    || (end.distance(network.channel_low) <= 0.12
                                        && start.distance(outlet_centre) <= 0.12)
                            })
                        })
                    });
                    let station_valid = station.is_some_and(|station| {
                        let recipient_exists =
                            plan.resolved_geometry.surfaces.iter().any(|surface| {
                                surface.id == station.recipient_surface
                                    && surface.owner == assembly.owner
                                    && surface.role == crate::SurfaceRole::DrainageRecipient
                                    && station
                                        .discharge
                                        .cmpge(surface.bounds.min - Vec3::splat(0.01))
                                        .all()
                                    && station
                                        .discharge
                                        .cmple(surface.bounds.max + Vec3::splat(0.01))
                                        .all()
                            });
                        let outlet_matches = outlet.is_some_and(|outlet| {
                            let centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
                            centre.distance(
                                plan.resolved_geometry
                                    .drainage_routes
                                    .iter()
                                    .find(|route| route.outlet_void == outlet.id)
                                    .map_or(centre, |route| route.outlet),
                            ) <= 0.02
                        });
                        let fall_plan = Vec2::new(station.discharge.x, station.discharge.z);
                        let fall_top = outlet
                            .map(|outlet| (outlet.bounds.min + outlet.bounds.max).y * 0.5)
                            .unwrap_or(station.discharge.y);
                        let fall_bottom = station.discharge.y;
                        let fall_is_vertical = outlet.is_some_and(|outlet| {
                            let centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
                            Vec2::new(centre.x, centre.z).distance(fall_plan) <= 0.04
                                && centre.y > fall_bottom + 0.08
                        });
                        let recipient_roof_owner = match station.recipient {
                            crate::RoofDrainageRecipient::ParentRoofFace { roof, .. } => plan
                                .roof_assemblies
                                .iter()
                                .find(|assembly| assembly.id == roof)
                                .map(|assembly| assembly.owner),
                            crate::RoofDrainageRecipient::GroundSplashApron => None,
                        };
                        let fall_clears_solids =
                            plan.resolved_geometry.solids.iter().all(|solid| {
                                if solid.owner == assembly.owner
                                    && matches!(
                                        solid.role,
                                        SolidRole::RoofGutter | SolidRole::RoofEdgeTreatment
                                    )
                                {
                                    return true;
                                }
                                if solid.role == SolidRole::RoofFace {
                                    return true;
                                }
                                if matches!(
                                    solid.role,
                                    SolidRole::FrameSill
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
                                ) {
                                    let fall_bounds = (
                                        Vec3::new(
                                            fall_plan.x - 0.08,
                                            fall_bottom + 0.08,
                                            fall_plan.y - 0.08,
                                        ),
                                        Vec3::new(
                                            fall_plan.x + 0.08,
                                            fall_top - 0.08,
                                            fall_plan.y + 0.08,
                                        ),
                                    );
                                    return !resolved_solid_overlaps_bounds(
                                        solid,
                                        fall_bounds,
                                        0.001,
                                    );
                                }
                                let bounds = resolved_solid_bounds(solid);
                                if solid.role == SolidRole::RoofFlashing
                                    && recipient_roof_owner == Some(solid.owner)
                                    && bounds.1.y <= station.discharge.y + 0.80
                                {
                                    // A parent-roof drip may terminate on the
                                    // authoritative upstand/apron at the recipient
                                    // contour. The flashing is the weathered
                                    // landing, not an obstruction in the fall path.
                                    return true;
                                }
                                let plan_hit = match solid.shape {
                                    crate::ResolvedSolidShape::RoundTowerShell {
                                        outer_radius_metres,
                                        ..
                                    } => {
                                        fall_plan
                                            .distance(Vec2::new(solid.centre.x, solid.centre.z))
                                            <= outer_radius_metres + 0.08
                                    }
                                    _ => {
                                        fall_plan.x >= bounds.0.x - 0.08
                                            && fall_plan.x <= bounds.1.x + 0.08
                                            && fall_plan.y >= bounds.0.z - 0.08
                                            && fall_plan.y <= bounds.1.z + 0.08
                                    }
                                };
                                let vertical_hit =
                                    fall_top - 0.08 > bounds.0.y && fall_bottom + 0.08 < bounds.1.y;
                                !(plan_hit && vertical_hit)
                            });
                        let roof_intersections = plan
                            .roof_assemblies
                            .iter()
                            .flat_map(|roof| roof.faces.iter().map(move |face| (roof, face)))
                            .filter_map(|(roof, face)| {
                                roof_face_contains_plan_point(face, fall_plan)
                                    .then(|| roof_face_height(face, fall_plan))
                                    .flatten()
                                    .map(|height| (roof, face, height))
                            })
                            .filter(|(_, _, height)| {
                                *height > fall_bottom + 0.08 && *height < fall_top - 0.08
                            })
                            .collect::<Vec<_>>();
                        let splash = plan
                            .resolved_geometry
                            .surfaces
                            .iter()
                            .find(|surface| surface.id == station.recipient_surface);
                        let splash_clears_portals = splash.is_some_and(|surface| {
                            plan.resolved_geometry
                                .voids
                                .iter()
                                .filter(|void| {
                                    matches!(
                                        void.role,
                                        VoidRole::WallOpening | VoidRole::AccessPortal
                                    ) && void.bounds.min.y < surface.bounds.max.y + 1.0
                                })
                                .all(|void| {
                                    surface.bounds.max.x < void.bounds.min.x
                                        || surface.bounds.min.x > void.bounds.max.x
                                        || surface.bounds.max.z < void.bounds.min.z
                                        || surface.bounds.min.z > void.bounds.max.z
                                })
                        });
                        let splash_clears_stairs = plan.stairs.iter().all(|stair| match *stair {
                            crate::Stair::Straight {
                                start,
                                direction,
                                width_metres,
                                tread_count,
                                ..
                            } => {
                                let axis = match direction {
                                    crate::Direction::North => Vec2::Y,
                                    crate::Direction::South => -Vec2::Y,
                                    crate::Direction::East => Vec2::X,
                                    crate::Direction::West => -Vec2::X,
                                };
                                let end = start + axis * tread_count as f32 * 0.28;
                                let delta = end - start;
                                let t = ((fall_plan - start).dot(delta)
                                    / delta.length_squared().max(0.000_001))
                                .clamp(0.0, 1.0);
                                fall_plan.distance(start + delta * t) > width_metres * 0.5 + 0.30
                            }
                            crate::Stair::Spiral {
                                centre,
                                outer_radius_metres,
                                ..
                            } => fall_plan.distance(centre) > outer_radius_metres + 0.30,
                        });
                        let disposition_valid = match station.disposition {
                            crate::RoofDrainageDisposition::FreeDripToParentRoof => {
                                let recipient_face = match station.recipient {
                                    crate::RoofDrainageRecipient::ParentRoofFace { roof, face } => {
                                        plan.roof_assemblies
                                            .iter()
                                            .find(|candidate| candidate.id == roof)
                                            .and_then(|roof| {
                                                roof.faces
                                                    .iter()
                                                    .find(|candidate| candidate.id == face)
                                            })
                                    }
                                    _ => None,
                                };
                                station.host_wall.is_none()
                                    && station.downspout.is_none()
                                    && fall_is_vertical
                                    && fall_clears_solids
                                    && roof_intersections.is_empty()
                                    && recipient_face.is_some_and(|face| {
                                        roof_face_contains_plan_point(face, fall_plan)
                                            && roof_face_height(face, fall_plan).is_some_and(
                                                |height| {
                                                    (height + 0.06 - station.discharge.y).abs()
                                                        <= 0.03
                                                },
                                            )
                                    })
                            }
                            crate::RoofDrainageDisposition::FreeDripToGround => {
                                station.host_wall.is_none()
                                    && station.downspout.is_none()
                                    && matches!(
                                        station.recipient,
                                        crate::RoofDrainageRecipient::GroundSplashApron
                                    )
                                    && station.discharge.y <= 0.12
                                    && fall_is_vertical
                                    && fall_clears_solids
                                    && roof_intersections.is_empty()
                                    && splash_clears_portals
                                    && splash_clears_stairs
                            }
                            crate::RoofDrainageDisposition::BoundDownspout => {
                                let Some(host_id) = station.host_wall else {
                                    return false;
                                };
                                let Some(host) =
                                    plan.wall_assemblies.iter().find(|wall| wall.id == host_id)
                                else {
                                    return false;
                                };
                                let Some(spout_id) = station.downspout else {
                                    return false;
                                };
                                let Some(spout) = plan
                                    .resolved_geometry
                                    .solids
                                    .iter()
                                    .find(|solid| solid.id == spout_id)
                                else {
                                    return false;
                                };
                                let plan_point = Vec2::new(spout.centre.x, spout.centre.z);
                                let offset = plan_point - host.frame.origin;
                                let projected_facade_clearance = match plan.archetype {
                                    crate::BuildingArchetype::TownHouse => 0.22,
                                    crate::BuildingArchetype::FachwerkMerchantHouse => 0.28,
                                    crate::BuildingArchetype::RenaissanceTownHall => 0.24,
                                    _ => 0.0,
                                };
                                let (facade_offset, along, expected_contact) = if let Some(radial) =
                                    host.radial_frame
                                {
                                    let radius = host.length_metres / std::f32::consts::TAU;
                                    let axis = (plan_point - radial.centre)
                                        .normalize_or(radial.reference_outward);
                                    (
                                        ((plan_point - radial.centre).length()
                                            - radius
                                            - host.thickness_metres * 0.5
                                            - 0.055)
                                            .abs(),
                                        0.0,
                                        radial.centre
                                            + axis * (radius + host.thickness_metres * 0.5),
                                    )
                                } else {
                                    (
                                        (offset.dot(host.frame.outward)
                                            - host.thickness_metres * 0.5
                                            - 0.055
                                            - projected_facade_clearance
                                            - if projected_facade_clearance > 0.0 {
                                                0.10
                                            } else {
                                                0.0
                                            })
                                        .abs(),
                                        offset.dot(host.frame.tangent).abs(),
                                        host.frame.origin
                                            + host.frame.tangent * offset.dot(host.frame.tangent)
                                            + host.frame.outward * host.thickness_metres * 0.5,
                                    )
                                };
                                let spout_bounds = resolved_solid_bounds(spout);
                                let avoids_openings = plan
                                    .resolved_geometry
                                    .voids
                                    .iter()
                                    .filter(|void| {
                                        matches!(
                                            void.role,
                                            VoidRole::WallOpening | VoidRole::AccessPortal
                                        )
                                    })
                                    .all(|void| {
                                        !bounds_overlap_3d(
                                            spout_bounds,
                                            (void.bounds.min, void.bounds.max),
                                            -0.08,
                                        )
                                    });
                                let avoids_routes = plan
                                    .resolved_geometry
                                    .solids
                                    .iter()
                                    .filter(|solid| {
                                        matches!(
                                            solid.role,
                                            SolidRole::CircuitWalk
                                                | SolidRole::WalkSurface
                                                | SolidRole::Landing
                                        )
                                    })
                                    .all(|solid| {
                                        !bounds_overlap_3d(
                                            spout_bounds,
                                            resolved_solid_bounds(solid),
                                            -0.08,
                                        )
                                    });
                                let spout_plan = Vec2::new(spout.centre.x, spout.centre.z);
                                let spout_bottom = spout.centre.y - spout.size.y * 0.5;
                                let spout_top = spout.centre.y + spout.size.y * 0.5;
                                let avoids_stairs = plan.stairs.iter().all(|stair| match *stair {
                                    crate::Stair::Straight {
                                        start,
                                        direction,
                                        base_height_metres,
                                        rise_metres,
                                        width_metres,
                                        tread_count,
                                    } => {
                                        if spout_top < base_height_metres - 0.08
                                            || spout_bottom
                                                > base_height_metres + rise_metres + 0.08
                                        {
                                            return true;
                                        }
                                        let axis = match direction {
                                            crate::Direction::North => Vec2::Y,
                                            crate::Direction::South => -Vec2::Y,
                                            crate::Direction::East => Vec2::X,
                                            crate::Direction::West => -Vec2::X,
                                        };
                                        let end = start + axis * tread_count as f32 * 0.28;
                                        let delta = end - start;
                                        let t = ((spout_plan - start).dot(delta)
                                            / delta.length_squared().max(0.000_001))
                                        .clamp(0.0, 1.0);
                                        spout_plan.distance(start + delta * t)
                                            > width_metres * 0.5 + 0.08
                                    }
                                    crate::Stair::Spiral {
                                        centre,
                                        base_height_metres,
                                        rise_metres,
                                        outer_radius_metres,
                                        ..
                                    } => {
                                        spout_top < base_height_metres - 0.08
                                            || spout_bottom
                                                > base_height_metres + rise_metres + 0.08
                                            || spout_plan.distance(centre)
                                                > outer_radius_metres + 0.08
                                    }
                                });
                                spout.role == SolidRole::RoofGutter
                                    && facade_offset <= 0.12
                                    && (host.radial_frame.is_some()
                                        || along <= host.length_metres * 0.5 + 0.02)
                                    && station.facade_contact.is_some_and(|contact| {
                                        Vec2::new(contact.x, contact.z).distance(expected_contact)
                                            <= 0.02
                                    })
                                    && matches!(
                                        station.recipient,
                                        crate::RoofDrainageRecipient::GroundSplashApron
                                    )
                                    && avoids_openings
                                    && avoids_routes
                                    && avoids_stairs
                            }
                        };
                        recipient_exists && outlet_matches && disposition_valid
                    });
                    let channel_valid = floor.is_some_and(|floor| {
                        let Some(edge) = edge else { return false };
                        let edge_a = Vec2::new(edge.start.x, edge.start.z);
                        let edge_b = Vec2::new(edge.end.x, edge.end.z);
                        let edge_delta = edge_b - edge_a;
                        let floor_plan = Vec2::new(floor.centre.x, floor.centre.z);
                        let along = ((floor_plan - edge_a).dot(edge_delta)
                            / edge_delta.length_squared().max(0.000_001))
                        .clamp(0.0, 1.0);
                        let contact_distance = floor_plan.distance(edge_a + edge_delta * along);
                        let maximum_fascia_offset = if matches!(
                            plan.archetype,
                            BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
                        ) {
                            0.42
                        } else {
                            0.15
                        };
                        let minimum_longfall = if assembly.parent.is_some()
                            && matches!(
                                assembly.kind,
                                crate::RoofKind::Gable | crate::RoofKind::Shed
                            ) {
                            0.012
                        } else {
                            0.035
                        };
                        floor.role == SolidRole::RoofGutter
                            && floor.longfall_radians.abs() >= 0.004
                            && floor.size.x + 0.05 >= edge_delta.length()
                            && contact_distance <= maximum_fascia_offset
                            && network.channel_high.y > network.channel_low.y + minimum_longfall
                    }) && lips_exist
                        && collector_valid
                        && station_valid
                        && network.discharge.y + 0.02 < network.channel_low.y
                        && outlet.is_some_and(|outlet| {
                            let centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
                            let low_plan = Vec2::new(network.channel_low.x, network.channel_low.z);
                            let channel_delta = Vec2::new(
                                network.channel_high.x - network.channel_low.x,
                                network.channel_high.z - network.channel_low.z,
                            );
                            let outlet_plan = Vec2::new(centre.x, centre.z);
                            let along = ((outlet_plan - low_plan).dot(channel_delta)
                                / channel_delta.length_squared().max(0.000_001))
                            .clamp(0.0, 1.0);
                            let outlet_is_on_channel =
                                outlet_plan.distance(low_plan + channel_delta * along) <= 0.08;
                            centre.distance(network.channel_low) <= 0.50
                                || outlet_is_on_channel
                                || collector_connects_outlet
                        });
                    let projected = face
                        .polygon
                        .iter()
                        .map(|point| Vec2::new(point.x, point.z))
                        .collect::<Vec<_>>();
                    let polygon_contains = |polygon: &[Vec2], point: Vec2| {
                        let signed_area = polygon
                            .iter()
                            .enumerate()
                            .map(|(index, start)| {
                                let end = polygon[(index + 1) % polygon.len()];
                                start.x * end.y - end.x * start.y
                            })
                            .sum::<f32>();
                        let sign = signed_area.signum();
                        polygon.iter().enumerate().all(|(index, start)| {
                            let end = polygon[(index + 1) % polygon.len()];
                            sign * (end - *start).perp_dot(point - *start) >= -0.002
                        })
                    };
                    let plan_min = projected
                        .iter()
                        .copied()
                        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
                    let plan_max = projected
                        .iter()
                        .copied()
                        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                    let cutouts = face
                        .cutouts
                        .iter()
                        .map(|cutout| {
                            cutout
                                .iter()
                                .map(|point| Vec2::new(point.x, point.z))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let mut expected_samples = Vec::new();
                    for x_step in 0..5 {
                        for z_step in 0..5 {
                            let fraction =
                                Vec2::new((x_step as f32 + 0.5) / 5.0, (z_step as f32 + 0.5) / 5.0);
                            let point = plan_min + (plan_max - plan_min) * fraction;
                            if polygon_contains(&projected, point)
                                && !cutouts.iter().any(|cutout| polygon_contains(cutout, point))
                            {
                                expected_samples.push(point);
                            }
                        }
                    }
                    let samples_valid = edge.is_some_and(|edge| {
                        let a = Vec2::new(edge.start.x, edge.start.z);
                        let b = Vec2::new(edge.end.x, edge.end.z);
                        let edge_delta = b - a;
                        network.samples.iter().all(|sample| {
                            let point = sample.surface_point;
                            let on_face =
                                (face.plane.normal.dot(point) + face.plane.constant).abs() <= 0.004;
                            let inlet = Vec2::new(sample.channel_inlet.x, sample.channel_inlet.z);
                            let along = ((inlet - a).dot(edge_delta)
                                / edge_delta.length_squared().max(0.000_001))
                            .clamp(0.0, 1.0);
                            let edge_distance = inlet.distance(a + edge_delta * along);
                            let flow = inlet - Vec2::new(point.x, point.z);
                            let downhill = Vec2::new(
                                face.plane.normal.x / face.plane.normal.y,
                                face.plane.normal.z / face.plane.normal.y,
                            )
                            .normalize_or_zero();
                            on_face
                                && point.y > sample.channel_inlet.y + 0.005
                                && edge_distance <= 0.04
                                && flow.normalize_or_zero().dot(downhill) >= 0.98
                        })
                    });
                    channel_valid && samples_valid
                })
                && {
                    let projected = face
                        .polygon
                        .iter()
                        .map(|point| Vec2::new(point.x, point.z))
                        .collect::<Vec<_>>();
                    let polygon_contains = |polygon: &[Vec2], point: Vec2| {
                        let signed_area = polygon
                            .iter()
                            .enumerate()
                            .map(|(index, start)| {
                                let end = polygon[(index + 1) % polygon.len()];
                                start.x * end.y - end.x * start.y
                            })
                            .sum::<f32>();
                        let sign = signed_area.signum();
                        polygon.iter().enumerate().all(|(index, start)| {
                            let end = polygon[(index + 1) % polygon.len()];
                            sign * (end - *start).perp_dot(point - *start) >= -0.002
                        })
                    };
                    let plan_min = projected
                        .iter()
                        .copied()
                        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
                    let plan_max = projected
                        .iter()
                        .copied()
                        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                    let cutouts = face
                        .cutouts
                        .iter()
                        .map(|cutout| {
                            cutout
                                .iter()
                                .map(|point| Vec2::new(point.x, point.z))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    let expected_samples = (0..5)
                        .flat_map(|x_step| {
                            let polygon_contains = &polygon_contains;
                            let cutouts = &cutouts;
                            let projected = &projected;
                            (0..5).filter_map(move |z_step| {
                                let fraction = Vec2::new(
                                    (x_step as f32 + 0.5) / 5.0,
                                    (z_step as f32 + 0.5) / 5.0,
                                );
                                let point = plan_min + (plan_max - plan_min) * fraction;
                                (polygon_contains(projected, point)
                                    && !cutouts
                                        .iter()
                                        .any(|cutout| polygon_contains(cutout, point)))
                                .then_some(point)
                            })
                        })
                        .collect::<Vec<_>>();
                    let sample_count = networks
                        .iter()
                        .map(|network| network.samples.len())
                        .sum::<usize>();
                    let coverage = expected_samples.iter().all(|expected| {
                        networks.iter().any(|network| {
                            network.samples.iter().any(|sample| {
                                Vec2::new(sample.surface_point.x, sample.surface_point.z)
                                    .distance(*expected)
                                    <= 0.01
                            })
                        })
                    });
                    sample_count == expected_samples.len() && coverage
                };
            if !network_valid {
                issues.push(issue(
                    "invalid_roof_drainage_network",
                    format!(
                        "roof face {} lacks sampled downhill flow into a physical channel, outlet, and spout ({} networks, {} samples, stations {:?})",
                        face.id.0,
                        networks.len(),
                        networks.iter().map(|network| network.samples.len()).sum::<usize>(),
                        networks
                            .iter()
                            .filter_map(|network| plan.resolved_geometry.roof_drainage_outlets
                                .iter()
                                .find(|station| station.id == network.outlet_station)
                                .map(|station| (
                                    station.disposition,
                                    station.host_wall,
                                    station.facade_contact,
                                    station.discharge,
                                    station.downspout,
                                )))
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }
        for enclosure in &assembly.enclosure_faces {
            let valid = item_ids.insert(enclosure.id)
                && enclosure.polygon.len() >= 3
                && !enclosure.support_nodes.is_empty()
                && enclosure.support_nodes.iter().all(|id| {
                    plan.resolved_geometry
                        .structural_nodes
                        .iter()
                        .any(|node| node.id == *id)
                });
            if !valid {
                issues.push(issue(
                    "invalid_roof_enclosure",
                    format!(
                        "roof {} enclosure {} lacks closed supported authority",
                        assembly.id.0, enclosure.id.0
                    ),
                ));
            }
        }
        if assembly.kind == crate::RoofKind::Gable && assembly.enclosure_faces.len() < 2 {
            issues.push(issue(
                "missing_gable_enclosure",
                format!("roof {} has open gable ends", assembly.id.0),
            ));
        }
        if assembly.kind == crate::RoofKind::HalfHip {
            // Project half-hip gate: a retained lower gable must rise above the
            // eave to a horizontal shoulder, while exactly two short upper hip
            // caps begin at that shoulder.  Merely relabelling a four-face full
            // hip therefore cannot pass.
            let base_y = assembly
                .faces
                .iter()
                .flat_map(|face| &face.polygon)
                .map(|point| point.y)
                .fold(f32::INFINITY, f32::min);
            let apex_y = assembly
                .faces
                .iter()
                .flat_map(|face| &face.polygon)
                .map(|point| point.y)
                .fold(f32::NEG_INFINITY, f32::max);
            let retained_gables = assembly
                .enclosure_faces
                .iter()
                .filter(|face| {
                    face.polygon.len() == 4
                        && face
                            .polygon
                            .iter()
                            .map(|point| point.y)
                            .fold(f32::NEG_INFINITY, f32::max)
                            > base_y + 0.1
                        && face
                            .polygon
                            .iter()
                            .map(|point| point.y)
                            .fold(f32::NEG_INFINITY, f32::max)
                            < apex_y - 0.1
                })
                .count();
            let upper_caps = assembly
                .faces
                .iter()
                .filter(|face| {
                    face.polygon.len() == 3
                        && face
                            .polygon
                            .iter()
                            .map(|point| point.y)
                            .fold(f32::INFINITY, f32::min)
                            > base_y + 0.1
                })
                .count();
            let shoulder_eaves = assembly
                .edges
                .iter()
                .filter(|edge| {
                    edge.kind == RoofEdgeKind::Eave
                        && (edge.start.y - edge.end.y).abs() <= 0.01
                        && edge.start.y > base_y + 0.1
                })
                .count();
            if retained_gables != 2 || upper_caps != 2 || shoulder_eaves != 2 {
                issues.push(issue(
                    "invalid_half_hip_graph",
                    format!(
                        "roof {} is a relabelled full hip or lacks two retained gables and shoulder eaves",
                        assembly.id.0
                    ),
                ));
            }
        }
        if assembly.parent.is_none() {
            for support in &assembly.support_nodes {
                let Some(interface) =
                    plan.resolved_geometry
                        .support_interfaces
                        .iter()
                        .find(|interface| {
                            interface.owner == assembly.owner && interface.node == *support
                        })
                else {
                    issues.push(issue(
                        "unsupported_roof",
                        format!(
                            "roof {} plate {} has no measured bearing",
                            assembly.id.0, support.0
                        ),
                    ));
                    continue;
                };
                let touches_wall = plan
                    .wall_assemblies
                    .iter()
                    .filter(|wall| {
                        wall.replaced_by_owner.is_none() && wall.support_node != *support
                    })
                    .flat_map(|wall| wall.host_solids.iter())
                    .filter_map(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == *id)
                    })
                    .chain(plan.resolved_geometry.solids.iter().filter(|solid| {
                        plan.timber_frame.is_some()
                            && matches!(
                                solid.role,
                                SolidRole::FramePlate
                                    | SolidRole::FrameGirder
                                    | SolidRole::FrameGableMember
                            )
                    }))
                    .chain(plan.resolved_geometry.solids.iter().filter(|solid| {
                        solid.owner == assembly.owner && solid.role == SolidRole::RoofFraming
                    }))
                    .any(|solid| {
                        bounds_overlap_3d(
                            resolved_solid_bounds(solid),
                            (interface.bounds.min, interface.bounds.max),
                            0.003,
                        )
                    });
                if !touches_wall {
                    issues.push(issue(
                        "unsupported_roof",
                        format!(
                            "roof {} plate {} does not contact an authoritative wall",
                            assembly.id.0, support.0
                        ),
                    ));
                }
            }
        }
        for edge in &assembly.edges {
            if !item_ids.insert(edge.id) || (edge.start - edge.end).length() <= 0.02 {
                issues.push(issue(
                    "invalid_roof_edge",
                    format!("roof {} has a duplicate or degenerate edge", assembly.id.0),
                ));
            }
            let adjacency = edge.adjacent_faces.len();
            let expected = match edge.kind {
                RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::Valley => 2,
                RoofEdgeKind::Eave
                | RoofEdgeKind::GableVerge
                | RoofEdgeKind::WallAbutment
                | RoofEdgeKind::TowerAbutment
                | RoofEdgeKind::OpeningCut => 1,
            };
            let known = edge
                .adjacent_faces
                .iter()
                .all(|id| all_face_ids.contains(id));
            if adjacency != expected || !known {
                issues.push(issue(
                    "roof_edge_adjacency",
                    format!(
                        "roof edge {} has {adjacency} faces, expected {expected}",
                        edge.id.0
                    ),
                ));
            }
            if edge.kind == RoofEdgeKind::Eave
                && !edge.drainage_terminal.is_some_and(|terminal| {
                    plan.resolved_geometry
                        .voids
                        .iter()
                        .any(|void| void.id == terminal && void.role == VoidRole::Drain)
                })
            {
                issues.push(issue(
                    "orphan_roof_drainage",
                    format!("roof eave {} has no drainage terminal", edge.id.0),
                ));
            }
            if matches!(
                edge.kind,
                RoofEdgeKind::WallAbutment | RoofEdgeKind::TowerAbutment
            ) && !edge.flashing.is_some_and(|flashing| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == flashing && solid.role == SolidRole::RoofFlashing)
            }) {
                issues.push(issue(
                    "unflashed_roof_abutment",
                    format!("roof abutment {} has no physical flashing", edge.id.0),
                ));
            }
            if edge.kind == RoofEdgeKind::Valley {
                let flashing_is_physical = edge.flashing.is_some_and(|flashing| {
                    plan.resolved_geometry.solids.iter().any(|solid| {
                        solid.id == flashing
                            && solid.role == SolidRole::RoofFlashing
                            && solid.longfall_radians.abs() > 0.001
                    })
                });
                let terminal = edge.drainage_terminal.and_then(|terminal| {
                    plan.resolved_geometry
                        .voids
                        .iter()
                        .find(|void| void.id == terminal && void.role == VoidRole::Drain)
                });
                let route_is_physical = terminal.is_some_and(|terminal| {
                    let terminal_centre = (terminal.bounds.min + terminal.bounds.max) * 0.5;
                    plan.resolved_geometry.drainage_routes.iter().any(|route| {
                        route.outlet_void == terminal.id
                            && route.inlet.y > route.outlet.y + 0.01
                            && route.outlet.distance(terminal_centre) <= 0.12
                            && [edge.start, edge.end]
                                .iter()
                                .any(|point| point.distance(route.inlet) <= 0.12)
                            && [edge.start, edge.end]
                                .iter()
                                .any(|point| point.distance(route.outlet) <= 0.12)
                    })
                });
                if !flashing_is_physical || !route_is_physical {
                    issues.push(issue(
                        "invalid_roof_valley_drainage",
                        format!(
                            "roof valley {} lacks sloped flashing or an exact downhill terminal",
                            edge.id.0
                        ),
                    ));
                }
            }
        }
        for child in &assembly.children {
            let child_exists = plan
                .roof_assemblies
                .iter()
                .any(|roof| roof.id == child.child && roof.parent == Some(assembly.id));
            let cut_exists = plan.resolved_geometry.voids.iter().any(|void| {
                void.id == child.parent_cut
                    && void.role == VoidRole::RoofOpening
                    && void.subtracts_from == assembly.owner
            });
            let cut_edges = child.valley_edges.iter().all(|id| {
                assembly.edges.iter().any(|edge| {
                    edge.id == *id
                        && edge.kind
                            == if child.kind == crate::RoofChildKind::Tower {
                                RoofEdgeKind::TowerAbutment
                            } else {
                                RoofEdgeKind::Valley
                            }
                        && edge.flashing.is_some()
                })
            });
            let flashing = !child.flashing_ids.is_empty()
                && child.flashing_ids.iter().all(|id| {
                    plan.resolved_geometry.solids.iter().any(|solid| {
                        solid.id == *id
                            && solid.owner == assembly.owner
                            && solid.role == SolidRole::RoofFlashing
                    })
                });
            let physical_hole = plan
                .resolved_geometry
                .voids
                .iter()
                .find(|void| void.id == child.parent_cut)
                .is_some_and(|void| {
                    assembly
                        .faces
                        .iter()
                        .flat_map(|face| &face.cutouts)
                        .any(|cutout| {
                            cutout.iter().all(|point| {
                                point.x >= void.bounds.min.x - 0.01
                                    && point.x <= void.bounds.max.x + 0.01
                                    && point.z >= void.bounds.min.z - 0.01
                                    && point.z <= void.bounds.max.z + 0.01
                            })
                        })
                });
            if !child_exists
                || !cut_exists
                || child.trimmer_nodes.is_empty()
                || !cut_edges
                || !flashing
                || !physical_hole
            {
                issues.push(issue(
                    "unresolved_roof_child",
                    format!(
                        "roof {} child {} lacks exact cut/trimmer authority",
                        assembly.id.0, child.child.0
                    ),
                ));
            }
            if matches!(
                child.kind,
                crate::RoofChildKind::GabledDormer
                    | crate::RoofChildKind::ShedDormer
                    | crate::RoofChildKind::CrossGable
            ) && child.child.0 >= 1_000
            {
                let front = plan.wall_assemblies.iter().find(|wall| {
                    wall.source == crate::WallSourceId::RoofChildFront { roof: child.child }
                });
                let front_opening = front.and_then(|wall| {
                    wall.opening_ids.iter().find_map(|id| {
                        plan.opening_assemblies.iter().find(|opening| {
                            opening.id == *id
                                && opening.host_wall == wall.id
                                && plan.resolved_geometry.voids.iter().any(|void| {
                                    void.id == opening.void_id && void.subtracts_from == wall.owner
                                })
                        })
                    })
                });
                let cross_gable_valid = child.kind != crate::RoofChildKind::CrossGable
                    || child.facade_wall.is_some_and(|facade_id| {
                        let facade = plan
                            .wall_assemblies
                            .iter()
                            .find(|wall| wall.id == facade_id);
                        let front_node = front.and_then(|wall| {
                            plan.resolved_geometry
                                .structural_nodes
                                .iter()
                                .find(|node| node.id == wall.support_node)
                        });
                        let split = child
                            .split_eave_edges
                            .iter()
                            .filter_map(|id| assembly.edges.iter().find(|edge| edge.id == *id))
                            .collect::<Vec<_>>();
                        facade.is_some_and(|facade| {
                            front_node.is_some_and(|node| {
                                node.supported_by.contains(&facade.support_node)
                            })
                        }) && split.len() == 3
                            && split[0].kind == RoofEdgeKind::Eave
                            && split[1].kind == RoofEdgeKind::OpeningCut
                            && split[2].kind == RoofEdgeKind::Eave
                            && split[0].end.distance(split[1].start) <= 0.01
                            && split[1].end.distance(split[2].start) <= 0.01
                            && split
                                .iter()
                                .all(|edge| edge.start.distance(edge.end) > 0.10)
                    });
                if front.is_none() || front_opening.is_none() || !cross_gable_valid {
                    issues.push(issue(
                        "invalid_roof_child_front",
                        format!(
                            "roof child {} lacks a subtracted weathered opening or facade-grounded cross-gable topology",
                            child.child.0
                        ),
                    ));
                }
            }
        }
        for abutment in &assembly.abutments {
            let edge_kind = match abutment.kind {
                crate::RoofAbutmentKind::Wall => RoofEdgeKind::WallAbutment,
                crate::RoofAbutmentKind::Tower => RoofEdgeKind::TowerAbutment,
            };
            let edges = abutment
                .edge_ids
                .iter()
                .filter_map(|id| assembly.edges.iter().find(|edge| edge.id == *id))
                .collect::<Vec<_>>();
            let uncovered_edges = edges
                .iter()
                .filter(|edge| {
                    !(edge.kind == edge_kind && {
                        let length = edge.start.distance(edge.end);
                        let station_count = (length / 0.10).ceil().max(1.0) as usize;
                        (0..=station_count).all(|station| {
                            let point = edge
                                .start
                                .lerp(edge.end, station as f32 / station_count as f32);
                            abutment
                                .samples
                                .iter()
                                .any(|sample| sample.point.distance(point) <= 0.14)
                        })
                    })
                })
                .count();
            let contour_covered = edges.len() == abutment.edge_ids.len() && uncovered_edges == 0;
            let samples_valid = !abutment.samples.is_empty()
                && abutment.samples.iter().all(|sample| {
                    let Some(host) = plan
                        .wall_assemblies
                        .iter()
                        .find(|wall| wall.id == sample.host_wall)
                    else {
                        return false;
                    };
                    let offset = Vec2::new(sample.point.x, sample.point.z) - host.frame.origin;
                    let signed_normal = offset.dot(host.frame.outward);
                    let normal_distance = (signed_normal - host.thickness_metres * 0.5).abs();
                    let corner_return = if abutment.kind == crate::RoofAbutmentKind::Tower {
                        host.thickness_metres * 0.5
                    } else {
                        0.0
                    };
                    let touches_host = normal_distance <= 0.18
                        && offset.dot(host.frame.tangent).abs()
                            <= host.length_metres * 0.5 + corner_return + 0.18;
                    let pieces = [
                        sample.apron_solid,
                        sample.upstand_solid,
                        sample.counterflashing_solid,
                    ]
                    .map(|id| {
                        plan.resolved_geometry.solids.iter().find(|solid| {
                            solid.id == id
                                && solid.owner == assembly.owner
                                && solid.role == SolidRole::RoofFlashing
                        })
                    });
                    let pieces_exist = pieces.iter().all(Option::is_some);
                    let weathering_seated = pieces[0]
                        .is_some_and(|solid| solid.centre.distance(sample.point) <= 0.24)
                        && pieces[1].is_some_and(|solid| {
                            (solid.centre.y - sample.point.y - 0.18).abs() <= 0.03
                                && Vec2::new(solid.centre.x, solid.centre.z)
                                    .distance(Vec2::new(sample.point.x, sample.point.z))
                                    <= 0.08
                        })
                        && pieces[2].is_some_and(|solid| {
                            (solid.centre.y - sample.point.y - 0.315).abs() <= 0.03
                                && Vec2::new(solid.centre.x, solid.centre.z)
                                    .distance(Vec2::new(sample.point.x, sample.point.z))
                                    <= 0.08
                        });
                    let host_kind_valid = abutment.kind != crate::RoofAbutmentKind::Tower
                        || matches!(host.source, crate::WallSourceId::SquareTowerFace { .. });
                    let opening_clear = plan.opening_assemblies.iter().all(|opening| {
                        if opening.host_wall != host.id {
                            return true;
                        }
                        let Some(void) = plan
                            .resolved_geometry
                            .voids
                            .iter()
                            .find(|void| void.id == opening.void_id)
                        else {
                            return false;
                        };
                        pieces.iter().flatten().all(|solid| {
                            !bounds_overlap_3d(
                                resolved_solid_bounds(solid),
                                (void.bounds.min, void.bounds.max),
                                -0.01,
                            )
                        })
                    });
                    touches_host
                        && pieces_exist
                        && weathering_seated
                        && host_kind_valid
                        && opening_clear
                });
            let drainage_valid = plan
                .resolved_geometry
                .voids
                .iter()
                .find(|void| {
                    void.id == abutment.lower_outlet
                        && void.role == VoidRole::Drain
                        && void.owner == assembly.owner
                })
                .is_some_and(|outlet| {
                    let outlet_centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
                    plan.resolved_geometry.drainage_routes.iter().any(|route| {
                        route.id == abutment.drainage_route
                            && route.outlet_void == outlet.id
                            && route.outlet.distance(outlet_centre) <= 0.02
                            && route.inlet.y > route.outlet.y + 0.02
                    })
                });
            if !contour_covered || !samples_valid || !drainage_valid {
                issues.push(issue(
                    "invalid_roof_abutment_contour",
                    format!(
                        "roof abutment {} lacks continuous host contact, weathering, opening clearance, or lower-corner drainage (contour={contour_covered}, uncovered={uncovered_edges}/{}, samples={samples_valid}, drainage={drainage_valid})",
                        abutment.id.0,
                        edges.len(),
                    ),
                ));
            }
        }
    }
    for assembly in plan
        .roof_assemblies
        .iter()
        .filter(|assembly| assembly.parent.is_some())
    {
        let parent = assembly.parent.expect("filtered parent");
        let references = plan
            .roof_assemblies
            .iter()
            .filter(|candidate| candidate.id == parent)
            .flat_map(|candidate| &candidate.children)
            .filter(|child| child.child == assembly.id)
            .count();
        if references != 1 {
            issues.push(issue(
                "orphan_roof_child",
                format!(
                    "roof {} has {references} parent graph references, expected one",
                    assembly.id.0
                ),
            ));
        }
    }
    let expected = plan.roofs.len()
        + plan.roof_dormers.len()
        + plan
            .towers
            .iter()
            .filter(|tower| tower.roof.is_some())
            .count()
        + plan.square_towers.len();
    if expected != plan.roof_assemblies.len() {
        issues.push(issue(
            "legacy_roof_authority",
            format!(
                "expected {expected} resolved roof assemblies, found {}",
                plan.roof_assemblies.len()
            ),
        ));
    }
}

fn audit_wall_opening_assemblies(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    use crate::{
        ClosureKind, OpeningHeadKind, OpeningProfile, OpeningUse, ResolvedItemId,
        WallMaterialClass, WallSourceId,
    };
    let expected_walls = if let Some(church) = &plan.church {
        usize::from(church.program.nave_bays) * 2
            + usize::from(church.program.nave_bays) * 2
            + usize::from(church.program.choir_bays) * 2
            + 8
            + usize::from(church.program.apse_sides)
            + 8
            + 4
    } else {
        plan.storeys
            .iter()
            .map(|storey| storey.walls.len())
            .sum::<usize>()
            + if matches!(
                plan.archetype,
                BuildingArchetype::CastleGatehouse
                    | BuildingArchetype::CourtyardCastle
                    | BuildingArchetype::WalledKeep
                    | BuildingArchetype::ArtilleryRondelCastle
            ) {
                plan.towers.len()
            } else {
                0
            }
            + plan
                .square_towers
                .iter()
                .filter(|tower| tower.bell_openings)
                .count()
                * 8
            + if plan.archetype == BuildingArchetype::Cathedral {
                2
            } else {
                0
            }
            + plan.roof_dormers.len()
            + plan
                .artillery_castle
                .as_ref()
                .map_or(0, |castle| castle.stations.len())
    };
    if plan.wall_assemblies.len() != expected_walls {
        issues.push(issue(
            "legacy_wall_not_migrated",
            format!(
                "resolved {} of {expected_walls} storey walls",
                plan.wall_assemblies.len()
            ),
        ));
    }
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<std::collections::HashMap<_, _>>();
    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let mut wall_ids = std::collections::HashSet::new();
    let mut wall_sources = std::collections::HashSet::new();
    for wall in &plan.wall_assemblies {
        if !wall_ids.insert(wall.id)
            || !wall_sources.insert(wall.source)
            || (wall.frame.tangent.length() - 1.0).abs() > 0.001
            || (wall.frame.outward.length() - 1.0).abs() > 0.001
            || wall.frame.tangent.dot(wall.frame.outward).abs() > 0.001
            || (!matches!(wall.source, WallSourceId::ChurchApse { .. })
                && !matches!(
                    (wall.frame.tangent, wall.frame.outward),
                    (
                        Vec2 {
                            x: -1.0 | 0.0 | 1.0,
                            y: -1.0 | 0.0 | 1.0
                        },
                        Vec2 {
                            x: -1.0 | 0.0 | 1.0,
                            y: -1.0 | 0.0 | 1.0
                        }
                    )
                ))
        {
            issues.push(issue(
                "invalid_wall_authority",
                format!(
                    "wall {} has duplicate source/ID or a non-cardinal local frame",
                    wall.id.0
                ),
            ));
        }
        let valid_thickness = match wall.material {
            WallMaterialClass::TimberInfill => (0.18..=0.24).contains(&wall.thickness_metres),
            WallMaterialClass::CivilianMasonry => (0.40..=0.70).contains(&wall.thickness_metres),
            WallMaterialClass::CathedralMasonry => (0.75..=1.10).contains(&wall.thickness_metres),
            WallMaterialClass::FortifiedMasonry => wall.thickness_metres >= 1.20,
            WallMaterialClass::InternalTimber => (0.12..=0.18).contains(&wall.thickness_metres),
            WallMaterialClass::InternalMasonry => (0.20..=0.35).contains(&wall.thickness_metres),
        };
        if !valid_thickness {
            issues.push(issue(
                "wall_profile_thickness",
                format!(
                    "wall {} violates its material/profile thickness table",
                    wall.id.0
                ),
            ));
        }
        let requires_semantic_frame = matches!(
            plan.archetype,
            BuildingArchetype::TownHouse
                | BuildingArchetype::HallHouse
                | BuildingArchetype::FachwerkCottage
                | BuildingArchetype::FachwerkMerchantHouse
                | BuildingArchetype::RenaissanceTownHall
        );
        if requires_semantic_frame
            && wall.material == WallMaterialClass::TimberInfill
            && !plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.bays.iter().any(|bay| {
                    bay.wall == Some(wall.id)
                        && bay.member_ids.len() >= 4
                        && bay.member_ids.iter().all(|id| {
                            frame.members.iter().any(|member| {
                                member.id == *id
                                    && member.structural
                                    && member.role != crate::TimberMemberRole::Ornament
                            })
                        })
                })
            })
        {
            issues.push(issue(
                "missing_authoritative_timber_frame",
                format!(
                    "timber wall {} suppresses its source without an opening-first semantic load bay",
                    wall.id.0
                ),
            ));
        }
        match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => {
                let tower = plan.towers.get(tower_index);
                let radial = wall.radial_frame;
                let shell = wall.host_solids.first().and_then(|id| solids.get(id));
                let valid = tower.is_some_and(|tower| {
                    radial.is_some_and(|radial| {
                        radial.centre.distance(tower.centre_metres()) <= 0.001
                            && radial.reference_outward.length_squared() > 0.99
                    }) && shell.is_some_and(|shell| {
                        matches!(
                            shell.shape,
                            crate::ResolvedSolidShape::RoundTowerShell {
                                outer_radius_metres,
                                inner_radius_metres,
                                chord_interfaces,
                            } if (outer_radius_metres - tower.radius_metres()).abs() <= 0.001
                                && (outer_radius_metres - inner_radius_metres
                                    - wall.thickness_metres).abs() <= 0.001
                                && chord_interfaces
                                    == [tower.chord_interface, tower.secondary_chord_interface]
                        )
                    })
                });
                if !valid {
                    issues.push(issue(
                        "invalid_round_wall_authority",
                        format!("round wall {} drifts from its grid tower shell", wall.id.0),
                    ));
                }
            }
            WallSourceId::StoreyWall { .. }
            | WallSourceId::CurtainWall { .. }
            | WallSourceId::ArtilleryCurtain { .. }
            | WallSourceId::SquareTowerFace { .. }
            | WallSourceId::CathedralClerestory { .. }
            | WallSourceId::RoofChildFront { .. }
            | WallSourceId::ChurchExterior { .. }
            | WallSourceId::ChurchArcade { .. }
            | WallSourceId::ChurchCrossing { .. }
            | WallSourceId::ChurchApse { .. }
            | WallSourceId::ChurchTowerFace { .. } => {
                if wall.radial_frame.is_some() {
                    issues.push(issue(
                        "invalid_wall_authority",
                        format!("linear wall {} declares a radial frame", wall.id.0),
                    ));
                }
            }
            WallSourceId::ArtilleryRondel { .. } => {}
        }
        if wall.host_solids.is_empty()
            || wall.host_solids.iter().any(|id| {
                solids.get(id).is_none_or(|solid| {
                    wall.replaced_by_owner
                        .map_or(solid.owner != wall.owner, |owner| solid.owner != owner)
                })
            })
        {
            issues.push(issue(
                "invalid_wall_host_union",
                format!("wall {} does not own an exact resolved host set", wall.id.0),
            ));
        }
        if !nodes.contains_key(&wall.support_node) {
            issues.push(issue(
                "unsupported_wall_assembly",
                format!("wall {} has no structural support node", wall.id.0),
            ));
        }
        if matches!(wall.source, WallSourceId::StoreyWall { .. })
            && wall.frame.outside_room.is_none()
            && wall.replaced_by_owner.is_none()
        {
            let expected_face =
                wall.frame.origin.dot(wall.frame.outward) + wall.thickness_metres * 0.5;
            let discontinuous = wall
                .host_solids
                .iter()
                .filter_map(|id| solids.get(id))
                .filter(|solid| {
                    matches!(
                        solid.role,
                        SolidRole::WallHost
                            | SolidRole::OpeningJamb
                            | SolidRole::OpeningSill
                            | SolidRole::OpeningHead
                            | SolidRole::OpeningSpandrel
                    ) && !(wall.material == WallMaterialClass::TimberInfill
                        && solid.role == SolidRole::WallHost)
                })
                .any(|solid| {
                    let centre = Vec2::new(solid.centre.x, solid.centre.z);
                    let radial_extent = wall.frame.outward.x.abs() * solid.size.x * 0.5
                        + wall.frame.outward.y.abs() * solid.size.z * 0.5;
                    (centre.dot(wall.frame.outward) + radial_extent - expected_face).abs() > 0.015
                });
            if discontinuous {
                issues.push(issue(
                    "discontinuous_exterior_wall_face",
                    format!(
                        "wall {} projects a displaced leaf/fin beyond its collinear exterior plane",
                        wall.id.0
                    ),
                ));
            }
        }
    }
    let mut opening_ids = std::collections::HashSet::new();
    for opening in &plan.opening_assemblies {
        let Some(wall) = plan
            .wall_assemblies
            .iter()
            .find(|wall| wall.id == opening.host_wall)
        else {
            issues.push(issue(
                "opening_without_host",
                format!("opening {} has no wall", opening.id.0),
            ));
            continue;
        };
        if !opening_ids.insert(opening.id)
            || opening.host_source != wall.source
            || !wall.opening_ids.contains(&opening.id)
        {
            issues.push(issue(
                "invalid_opening_authority",
                format!(
                    "opening {} is duplicated or drifts from its host",
                    opening.id.0
                ),
            ));
        }
        let Some(void) = plan
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == opening.void_id)
        else {
            issues.push(issue(
                "shallow_wall_opening",
                format!("opening {} has no void", opening.id.0),
            ));
            continue;
        };
        let depth = opening.frame.outward.x.abs() * (void.bounds.max.x - void.bounds.min.x)
            + opening.frame.outward.y.abs() * (void.bounds.max.z - void.bounds.min.z);
        if void.role != VoidRole::WallOpening
            || void.owner != opening.owner
            || void.subtracts_from != opening.owner
            || depth + 0.01 < wall.thickness_metres
        {
            issues.push(issue(
                "shallow_wall_opening",
                format!(
                    "opening {} is not a connected full-depth subtraction",
                    opening.id.0
                ),
            ));
        }
        let (
            profile_exterior_width,
            profile_interior_width,
            profile_exterior_height,
            profile_interior_height,
        ) = match opening.profile {
            OpeningProfile::ArrowLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            }
            | OpeningProfile::GunLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                ..
            } => (
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            ),
            profile => (
                profile.exterior_width_metres(),
                profile.interior_width_metres(),
                profile.clear_height_metres(),
                profile.clear_height_metres(),
            ),
        };
        let expected_depth_sign = if opening.frame.tangent.x.abs() > 0.5 {
            if opening.frame.outward.y >= 0.0 {
                1
            } else {
                -1
            }
        } else if opening.frame.outward.x <= 0.0 {
            1
        } else {
            -1
        };
        let sectional_shape_matches = matches!(void.shape,
            crate::ResolvedVoidShape::SectionalOpening {
                opening: resolved_opening,
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                exterior_depth_sign,
            } if resolved_opening == opening.id
                && (exterior_width_metres-profile_exterior_width).abs() <= 0.001
                && (interior_width_metres-profile_interior_width).abs() <= 0.001
                && (exterior_height_metres-profile_exterior_height.min(opening.profile.clear_height_metres())).abs() <= 0.001
                && (interior_height_metres-profile_interior_height.min(opening.profile.clear_height_metres())).abs() <= 0.001
                && exterior_depth_sign == expected_depth_sign);
        let slices_valid = opening.sectional_void.len() >= 5
            && opening.sectional_void.first().is_some_and(|slice| {
                slice.depth_fraction.abs() <= 0.001
                    && (slice.width_metres - profile_exterior_width).abs() <= 0.001
            })
            && opening.sectional_void.last().is_some_and(|slice| {
                (slice.depth_fraction - 1.0).abs() <= 0.001
                    && (slice.width_metres - profile_interior_width).abs() <= 0.001
            })
            && opening.sectional_void.windows(2).all(|pair| {
                pair[1].depth_fraction > pair[0].depth_fraction
                    && pair[1].width_metres + 0.001 >= pair[0].width_metres
                    && pair[1].height_metres + 0.001 >= pair[0].height_metres
            })
            && opening.sectional_void.iter().all(|slice| {
                let expected_width = profile_exterior_width
                    + (profile_interior_width - profile_exterior_width) * slice.depth_fraction;
                let expected_height = profile_exterior_height
                    + (profile_interior_height - profile_exterior_height) * slice.depth_fraction;
                (slice.width_metres - expected_width).abs() <= 0.002
                    && (slice.height_metres - expected_height).abs() <= 0.002
            });
        if !sectional_shape_matches || !slices_valid {
            issues.push(issue(
                "false_splayed_wall_opening",
                format!(
                    "opening {} lacks an ordered connected throat-to-mouth free-space field",
                    opening.id.0
                ),
            ));
        }
        let profile_valid = match opening.profile {
            OpeningProfile::Rectangular {
                width_metres,
                height_metres,
            } => {
                width_metres >= 0.68
                    && height_metres
                        >= if matches!(opening.host_source, WallSourceId::RoofChildFront { .. }) {
                            0.68
                        } else {
                            1.0
                        }
            }
            OpeningProfile::Segmental {
                width_metres,
                spring_height_metres,
                rise_metres,
                intrados_depth_metres,
            } => {
                width_metres >= 0.75
                    && spring_height_metres
                        >= if opening.use_kind == OpeningUse::Gate {
                            1.8
                        } else {
                            0.8
                        }
                    && rise_metres > 0.12
                    && intrados_depth_metres >= 0.12
            }
            OpeningProfile::PointedTwoCentred {
                width_metres,
                spring_height_metres,
                apex_height_metres,
                arc_radius_metres,
            } => {
                let half_span = width_metres * 0.5;
                let rise = apex_height_metres - spring_height_metres;
                let constructed_radius =
                    half_span + (rise * rise - half_span * half_span) / (2.0 * half_span.max(0.01));
                width_metres >= 0.35
                    && apex_height_metres > spring_height_metres + 0.40
                    && arc_radius_metres > width_metres * 0.5
                    && (arc_radius_metres - constructed_radius).abs() <= 0.01
            }
            OpeningProfile::ArrowLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            } => {
                exterior_width_metres < interior_width_metres
                    && exterior_height_metres <= interior_height_metres
                    && exterior_width_metres <= 0.22
            }
            OpeningProfile::GunLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                traverse_degrees,
                recoil_metres,
                crew_clearance_metres,
                ..
            } => {
                exterior_width_metres < interior_width_metres
                    && exterior_height_metres < interior_height_metres
                    && traverse_degrees >= 20.0
                    && recoil_metres >= 0.65
                    && crew_clearance_metres >= 1.0
            }
        };
        let use_profile_match = matches!(
            (opening.use_kind, opening.profile),
            (OpeningUse::Door, OpeningProfile::Rectangular { .. })
                | (OpeningUse::Gate, OpeningProfile::Segmental { .. })
                | (
                    OpeningUse::Window,
                    OpeningProfile::Rectangular { .. }
                        | OpeningProfile::Segmental { .. }
                        | OpeningProfile::PointedTwoCentred { .. }
                )
                | (OpeningUse::ArrowLoop, OpeningProfile::ArrowLoop { .. })
                | (OpeningUse::GunLoop, OpeningProfile::GunLoop { .. })
                | (
                    OpeningUse::BellOpening,
                    OpeningProfile::PointedTwoCentred { .. }
                )
        );
        if !profile_valid || !use_profile_match {
            issues.push(issue(
                "invalid_opening_profile",
                format!(
                    "opening {} has an invalid or substituted section",
                    opening.id.0
                ),
            ));
        }
        let exact_piece = |id: ResolvedItemId, role: SolidRole| {
            solids
                .get(&id)
                .is_some_and(|solid| solid.owner == opening.owner && solid.role == role)
        };
        if !exact_piece(opening.jamb_solids[0], SolidRole::OpeningJamb)
            || !exact_piece(opening.jamb_solids[1], SolidRole::OpeningJamb)
            || !exact_piece(opening.head_solid, SolidRole::OpeningHead)
            || !exact_piece(opening.spandrel_solid, SolidRole::OpeningSpandrel)
            || opening.reveal_surfaces.len() < 6
            || opening.reveal_surfaces.iter().any(|id| {
                surfaces.get(id).is_none_or(|surface| {
                    surface.owner != opening.owner
                        || !matches!(
                            surface.role,
                            SurfaceRole::LeftJambReveal
                                | SurfaceRole::RightJambReveal
                                | SurfaceRole::WeatherSill
                                | SurfaceRole::Intrados
                                | SurfaceRole::ExteriorThroat
                                | SurfaceRole::InteriorMouth
                        )
                })
            })
        {
            issues.push(issue(
                "missing_opening_reveal_piece",
                format!("opening {} lacks exact jamb/head/reveal IDs", opening.id.0),
            ));
        }
        let opening_offset = (opening.frame.origin - wall.frame.origin).dot(opening.frame.tangent);
        let exterior_width_for_layout = opening.profile.exterior_width_metres();
        let jambs_on_declared_reveals =
            opening.jamb_solids.iter().enumerate().all(|(index, id)| {
                let side = if index == 0 { -1.0_f32 } else { 1.0 };
                let side_width = if side < 0.0 {
                    wall.length_metres * 0.5 + opening_offset - exterior_width_for_layout * 0.5
                } else {
                    wall.length_metres * 0.5 - opening_offset - exterior_width_for_layout * 0.5
                };
                solids.get(id).is_some_and(|solid| {
                    let expected = opening.frame.origin
                        + opening.frame.tangent
                            * side
                            * (exterior_width_for_layout + side_width)
                            * 0.5;
                    Vec2::new(solid.centre.x, solid.centre.z).distance(expected) <= 0.015
                })
            });
        if !jambs_on_declared_reveals {
            issues.push(issue(
                "false_opening_head_load_path",
                format!(
                    "opening {} jambs drift from their measured springing/reveal lines",
                    opening.id.0
                ),
            ));
        }
        let splayed_profile = match opening.profile {
            OpeningProfile::ArrowLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                ..
            }
            | OpeningProfile::GunLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                ..
            } => Some((
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            )),
            _ => None,
        };
        if let Some((exterior_width, interior_width, exterior_height, interior_height)) =
            splayed_profile
        {
            let exact_splayed_jamb = |id: ResolvedItemId, expected_side: i8| {
                solids.get(&id).is_some_and(|solid| {
                    matches!(
                        solid.shape,
                        crate::ResolvedSolidShape::SplayedReveal {
                            exterior_width_metres,
                            interior_width_metres,
                            side,
                            exterior_depth_sign,
                        } if (exterior_width_metres - exterior_width).abs() <= 0.001
                            && (interior_width_metres - interior_width).abs() <= 0.001
                            && side == expected_side
                            && exterior_depth_sign == expected_depth_sign
                    )
                })
            };
            let tangent_depth = opening.frame.tangent.x.abs()
                * (void.bounds.max.x - void.bounds.min.x)
                + opening.frame.tangent.y.abs() * (void.bounds.max.z - void.bounds.min.z);
            let exact_splayed_head = solids.get(&opening.head_solid).is_some_and(|solid| {
                matches!(
                    solid.shape,
                    crate::ResolvedSolidShape::SplayedHead {
                        exterior_clear_height_metres,
                        interior_clear_height_metres,
                        exterior_depth_sign,
                    } if (exterior_clear_height_metres - exterior_height).abs() <= 0.001
                        && (interior_clear_height_metres - interior_height).abs() <= 0.001
                        && exterior_depth_sign == expected_depth_sign
                )
            });
            let sampled_host = opening.sectional_void.iter().all(|slice| {
                let plan = opening.frame.origin
                    + opening.frame.outward
                        * (wall.thickness_metres * (0.5 - slice.depth_fraction));
                let clear_top = opening.sill_elevation_metres + slice.height_metres;
                let free_point = Vec3::new(plan.x, clear_top - 0.015, plan.y);
                let head_point = Vec3::new(plan.x, clear_top + 0.015, plan.y);
                let side_height = opening.sill_elevation_metres + slice.height_metres * 0.5;
                let side_offset = slice.width_metres * 0.5 + 0.015;
                let side_points = [-1.0_f32, 1.0].map(|side| {
                    let side_plan = plan + opening.frame.tangent * side * side_offset;
                    Vec3::new(side_plan.x, side_height, side_plan.y)
                });
                let host_solids = [
                    opening.jamb_solids[0],
                    opening.jamb_solids[1],
                    opening.head_solid,
                    opening.spandrel_solid,
                ];
                let contains = |point| {
                    host_solids.iter().any(|id| {
                        solids.get(id).is_some_and(|solid| {
                            opening_host_contains_point(opening, wall, solid, point)
                        })
                    })
                };
                !contains(free_point)
                    && contains(head_point)
                    && side_points.into_iter().all(contains)
            });
            if !exact_splayed_jamb(opening.jamb_solids[0], -1)
                || !exact_splayed_jamb(opening.jamb_solids[1], 1)
                || !exact_splayed_head
                || !sampled_host
                || (tangent_depth - interior_width).abs() > 0.02
            {
                issues.push(issue(
                    "false_splayed_wall_opening",
                    format!(
                        "opening {} does not resolve its sampled narrow throat, broad mouth, and rising head into physical host masonry",
                        opening.id.0
                    ),
                ));
            }
        } else if opening.jamb_solids.iter().any(|id| {
            solids.get(id).is_some_and(|solid| {
                matches!(solid.shape, crate::ResolvedSolidShape::SplayedReveal { .. })
            })
        }) {
            issues.push(issue(
                "false_splayed_wall_opening",
                format!(
                    "opening {} substitutes a splay for its declared profile",
                    opening.id.0
                ),
            ));
        }
        let resolved_surface_roles = opening
            .reveal_surfaces
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == *id && surface.owner == opening.owner)
                    .map(|surface| surface.role)
            })
            .collect::<Vec<_>>();
        if !resolved_surface_roles.contains(&crate::SurfaceRole::LeftJambReveal)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::RightJambReveal)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::WeatherSill)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::Intrados)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::ExteriorThroat)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::InteriorMouth)
        {
            issues.push(issue(
                "missing_opening_reveal_surface",
                format!(
                    "opening {} lacks exact reveal, weather-sill, or intrados surfaces",
                    opening.id.0
                ),
            ));
        }
        // Surface identity belongs to the opening's exact reveal multiset.
        // Several structural bays may legitimately share a church assembly
        // owner, so owner+role alone can select a neighbouring sill/intrados.
        let shaped_surface = |role| {
            opening.reveal_surfaces.iter().find_map(|id| {
                surfaces
                    .get(id)
                    .filter(|surface| surface.owner == opening.owner && surface.role == role)
                    .copied()
            })
        };
        let sill_and_intrados_valid = shaped_surface(crate::SurfaceRole::WeatherSill).is_some_and(|surface| matches!(surface.shape,
                crate::ResolvedSurfaceShape::WeatherSill { interior_elevation_metres, exterior_elevation_metres, drip_depth_metres }
                    if exterior_elevation_metres + 0.02 < interior_elevation_metres && drip_depth_metres >= 0.02))
            && shaped_surface(crate::SurfaceRole::Intrados).is_some_and(|surface| match (opening.profile, surface.shape) {
                (OpeningProfile::Segmental { width_metres, spring_height_metres, rise_metres, .. }, crate::ResolvedSurfaceShape::SegmentalIntrados { clear_span_metres, spring_height_metres: spring, rise_metres: rise }) => (clear_span_metres-width_metres).abs() <= 0.001 && (spring-spring_height_metres).abs() <= 0.001 && (rise-rise_metres).abs() <= 0.001,
                (OpeningProfile::PointedTwoCentred { width_metres, spring_height_metres, apex_height_metres, arc_radius_metres }, crate::ResolvedSurfaceShape::PointedIntrados { clear_span_metres, spring_height_metres: spring, apex_height_metres: apex, arc_radius_metres: radius }) => (clear_span_metres-width_metres).abs() <= 0.001 && (spring-spring_height_metres).abs() <= 0.001 && (apex-apex_height_metres).abs() <= 0.001 && (radius-arc_radius_metres).abs() <= 0.001,
                (OpeningProfile::Rectangular { .. } | OpeningProfile::ArrowLoop { .. } | OpeningProfile::GunLoop { .. }, crate::ResolvedSurfaceShape::Planar) => true,
                _ => false,
            });
        if !sill_and_intrados_valid {
            issues.push(issue(
                "invalid_opening_weather_or_intrados",
                format!(
                    "opening {} has a flat/uphill sill or substituted intrados",
                    opening.id.0
                ),
            ));
        }
        let head = nodes.get(&opening.head_node);
        let spandrel_node = nodes.get(&opening.spandrel_node);
        if head.is_none_or(|head| {
            head.kind != crate::StructuralNodeKind::OpeningHead
                || head.supported_by.len() != 2
                || !opening
                    .jamb_nodes
                    .iter()
                    .all(|jamb| head.supported_by.contains(jamb))
        }) || opening.jamb_nodes.iter().any(|jamb| {
            nodes.get(jamb).is_none_or(|node| {
                node.kind != crate::StructuralNodeKind::OpeningJamb
                    || !node.supported_by.contains(&wall.support_node)
            })
        }) || spandrel_node.is_none_or(|node| {
            node.kind != crate::StructuralNodeKind::OpeningSpandrel
                || node.supported_by != [opening.head_node]
        }) {
            issues.push(issue(
                "false_opening_head_load_path",
                format!(
                    "opening {} head does not bear through two grounded jambs",
                    opening.id.0
                ),
            ));
        }
        let head_solid = solids.get(&opening.head_solid);
        let spandrel_solid = solids.get(&opening.spandrel_solid);
        let bearing_interfaces = opening.head_bearing_interfaces.map(|id| {
            plan.resolved_geometry
                .support_interfaces
                .iter()
                .find(|interface| {
                    interface.id == id
                        && interface.owner == opening.owner
                        && interface.node == opening.head_node
                })
        });
        let wall_above = plan
            .resolved_geometry
            .support_interfaces
            .iter()
            .find(|interface| {
                interface.id == opening.wall_above_interface
                    && interface.owner == opening.owner
                    && interface.node == opening.spandrel_node
            });
        let contact_valid =
            head_solid.is_some_and(|head_solid| {
                bearing_interfaces.into_iter().zip(opening.jamb_solids).all(
                    |(interface, jamb_id)| {
                        let Some(interface) = interface else {
                            return false;
                        };
                        let Some(jamb) = solids.get(&jamb_id) else {
                            return false;
                        };
                        let (head_min, head_max) = resolved_solid_bounds(head_solid);
                        let (jamb_min, jamb_max) = resolved_solid_bounds(jamb);
                        let contact_min = head_min.max(jamb_min).max(interface.bounds.min);
                        let contact_max = head_max.min(jamb_max).min(interface.bounds.max);
                        let size = contact_max - contact_min;
                        size.min_element() > 0.001 && {
                            let mut extents = [size.x, size.y, size.z];
                            extents.sort_by(f32::total_cmp);
                            extents[1] * extents[2] >= 0.01
                        }
                    },
                ) && spandrel_solid.is_some_and(|spandrel| {
                    spandrel.supported_by == [opening.spandrel_node]
                        && wall_above.is_some_and(|interface| {
                            let (head_min, head_max) = resolved_solid_bounds(head_solid);
                            let (spandrel_min, spandrel_max) = resolved_solid_bounds(spandrel);
                            let contact_min = head_min.max(spandrel_min).max(interface.bounds.min);
                            let contact_max = head_max.min(spandrel_max).min(interface.bounds.max);
                            let size = contact_max - contact_min;
                            size.min_element() > 0.001 && {
                                let mut extents = [size.x, size.y, size.z];
                                extents.sort_by(f32::total_cmp);
                                extents[1] * extents[2] >= 0.02
                            }
                        })
                })
            });
        if !contact_valid {
            issues.push(issue("false_opening_head_load_path", format!("opening {} head lacks measured two-ended bearing or distinct upper-spandrel contact; head={:?} spandrel={:?} bearings={:?} wall_above={:?}", opening.id.0, head_solid.map(|solid| (solid.centre, solid.size)), spandrel_solid.map(|solid| (solid.centre, solid.size)), bearing_interfaces.map(|interface| interface.map(|interface| (interface.bounds.min, interface.bounds.max))), wall_above.map(|interface| (interface.bounds.min, interface.bounds.max)))));
        }
        let wide_cathedral_light = opening.use_kind == OpeningUse::Window
            && matches!(opening.profile, OpeningProfile::PointedTwoCentred { width_metres, .. } if width_metres >= 0.90);
        if wide_cathedral_light {
            let tracery_node = opening.tracery_node.and_then(|id| nodes.get(&id));
            let tracery_solids = plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    solid.owner == opening.owner
                        && solid.role == SolidRole::Mullion
                        && opening
                            .tracery_node
                            .is_some_and(|node| solid.supported_by == [node])
                })
                .collect::<Vec<_>>();
            let bearing = opening.tracery_node.and_then(|node| {
                plan.resolved_geometry
                    .support_interfaces
                    .iter()
                    .find(|interface| interface.owner == opening.owner && interface.node == node)
            });
            let sill = opening.sill_solid.and_then(|id| solids.get(&id));
            let mullion_bears = tracery_solids.first().is_some_and(|mullion| {
                sill.is_some_and(|sill| {
                    bearing.is_some_and(|interface| {
                        let (mullion_min, mullion_max) = resolved_solid_bounds(mullion);
                        let (sill_min, sill_max) = resolved_solid_bounds(sill);
                        let contact_min = mullion_min.max(sill_min).max(interface.bounds.min);
                        let contact_max = mullion_max.min(sill_max).min(interface.bounds.max);
                        let size = contact_max - contact_min;
                        size.min_element() > 0.001 && size.x.max(size.z) >= 0.06
                    })
                })
            });
            if tracery_node.is_none_or(|node| {
                node.kind != crate::StructuralNodeKind::MullionBearing
                    || node.supported_by != [wall.support_node]
            }) || tracery_solids.len() < 2
                || !mullion_bears
                || opening.closure_solids.len() < 2
                || opening.closure_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::LeadedGlazing)
                })
            {
                issues.push(issue(
                    "unsupported_cathedral_tracery",
                    format!(
                        "opening {} lacks stone mullion/transom bearing or subdivided glazing",
                        opening.id.0
                    ),
                ));
            }
        } else if opening.tracery_node.is_some() {
            issues.push(issue(
                "unsupported_cathedral_tracery",
                format!(
                    "opening {} declares tracery outside a principal cathedral light",
                    opening.id.0
                ),
            ));
        }
        let illegal_closure = match opening.use_kind {
            OpeningUse::ArrowLoop | OpeningUse::GunLoop => {
                opening.closure.layers != [ClosureKind::OpenMilitary]
                    || !opening.closure_solids.is_empty()
            }
            OpeningUse::Window if plan.archetype == BuildingArchetype::Cathedral => {
                opening.closure.layers != [ClosureKind::LeadedGlazing]
            }
            OpeningUse::Window => !opening.closure.layers.contains(&ClosureKind::TimberShutter),
            OpeningUse::Door | OpeningUse::Gate => {
                !opening.closure.layers.contains(&ClosureKind::DoorLeaf)
            }
            OpeningUse::BellOpening => opening.closure.layers != [ClosureKind::TimberLouvre],
        };
        if illegal_closure {
            issues.push(issue(
                "illegal_opening_closure",
                format!(
                    "opening {} has an illegal glazing/closure policy",
                    opening.id.0
                ),
            ));
        }
        if matches!(
            opening.use_kind,
            OpeningUse::ArrowLoop | OpeningUse::GunLoop
        ) && (opening.stance_surface.is_none()
            || opening.ray_indices.len() != 3
            || opening.ray_indices.iter().any(|index| {
                plan.resolved_geometry
                    .projected_defense_rays
                    .get(*index)
                    .is_none_or(|ray| ray.owner != opening.owner || ray.throat != opening.void_id)
            })
            || (opening.use_kind == OpeningUse::GunLoop && opening.mount_solid.is_none()))
        {
            issues.push(issue(
                "inoperable_military_opening",
                format!(
                    "opening {} lacks stance/mount/near-mid-far rays",
                    opening.id.0
                ),
            ));
        }
        let head_shape = solids.get(&opening.head_solid).map(|solid| solid.shape);
        let head_matches = match opening.profile {
            OpeningProfile::Segmental {
                width_metres,
                spring_height_metres,
                rise_metres,
                intrados_depth_metres,
            } => {
                opening.head_kind == OpeningHeadKind::SegmentalArch
                    && matches!(
                        head_shape,
                        Some(crate::ResolvedSolidShape::SegmentalArchRing {
                            clear_span_metres,
                            spring_height_metres: resolved_spring,
                            rise_metres: resolved_rise,
                            ring_depth_metres,
                        }) if (clear_span_metres - width_metres).abs() <= 0.001
                            && (resolved_spring - spring_height_metres).abs() <= 0.001
                            && (resolved_rise - rise_metres).abs() <= 0.001
                            && (ring_depth_metres - intrados_depth_metres).abs() <= 0.001
                    )
            }
            OpeningProfile::PointedTwoCentred {
                width_metres,
                spring_height_metres,
                apex_height_metres,
                arc_radius_metres,
            } => {
                opening.head_kind == OpeningHeadKind::PointedVoussoir
                    && matches!(
                        head_shape,
                        Some(crate::ResolvedSolidShape::PointedArchRing {
                            clear_span_metres,
                            spring_height_metres: resolved_spring,
                            apex_height_metres: resolved_apex,
                            arc_radius_metres: resolved_radius,
                            ..
                        }) if (clear_span_metres - width_metres).abs() <= 0.001
                            && (resolved_spring - spring_height_metres).abs() <= 0.001
                            && (resolved_apex - apex_height_metres).abs() <= 0.001
                            && (resolved_radius - arc_radius_metres).abs() <= 0.001
                    )
            }
            OpeningProfile::Rectangular { .. } => matches!(
                opening.head_kind,
                OpeningHeadKind::TimberLintel | OpeningHeadKind::StoneLintel
            ),
            OpeningProfile::ArrowLoop {
                exterior_height_metres,
                interior_height_metres,
                ..
            }
            | OpeningProfile::GunLoop {
                exterior_height_metres,
                interior_height_metres,
                ..
            } => matches!(
                (opening.head_kind, head_shape),
                (
                    OpeningHeadKind::StoneLintel,
                    Some(crate::ResolvedSolidShape::SplayedHead {
                        exterior_clear_height_metres,
                        interior_clear_height_metres,
                        ..
                    })
                ) if (exterior_clear_height_metres - exterior_height_metres).abs() <= 0.001
                    && (interior_clear_height_metres - interior_height_metres).abs() <= 0.001
            ),
        };
        if !head_matches {
            issues.push(issue(
                "opening_head_profile_mismatch",
                format!("opening {} head does not match its section", opening.id.0),
            ));
        }
    }
    let source_openings = plan
        .storeys
        .iter()
        .map(|storey| storey.openings.len())
        .sum::<usize>();
    let replaced_openings = plan
        .wall_assemblies
        .iter()
        .filter(|wall| wall.replaced_by_owner.is_some())
        .filter(|wall| match wall.source {
            WallSourceId::StoreyWall {
                storey_level,
                wall_index,
            } => plan
                .storeys
                .get(storey_level as usize)
                .is_some_and(|storey| {
                    storey
                        .openings
                        .iter()
                        .any(|opening| opening.wall == wall_index)
                }),
            _ => false,
        })
        .count();
    let bell_openings = plan
        .square_towers
        .iter()
        .filter(|tower| tower.bell_openings)
        .count()
        * 8;
    let roof_child_openings = plan.roof_dormers.len();
    let church_portals = usize::from(plan.church.is_some()) * 2;
    let church_windows = plan.church.as_ref().map_or(0, |church| {
        usize::from(church.program.nave_bays) * 4
            + usize::from(church.program.choir_bays) * 2
            + 2
            + usize::from(church.program.apse_sides.saturating_sub(1))
    });
    let artillery_openings = plan
        .artillery_castle
        .as_ref()
        .map_or(0, |castle| castle.stations.len());
    if plan.opening_assemblies.len() + replaced_openings
        != source_openings
            + bell_openings
            + roof_child_openings
            + church_portals
            + church_windows
            + artillery_openings
    {
        issues.push(issue(
            "legacy_opening_not_migrated",
            format!(
                "resolved {} of {} openings",
                plan.opening_assemblies.len(),
                source_openings
                    + bell_openings
                    + roof_child_openings
                    + church_portals
                    + church_windows
                    + artillery_openings
            ),
        ));
    }
}

fn resolved_solid_bounds(solid: &ResolvedSolid) -> (Vec3, Vec3) {
    let cosine = solid.yaw_radians.cos().abs();
    let sine = solid.yaw_radians.sin().abs();
    let half = Vec3::new(
        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
        solid.size.y * 0.5,
        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
    );
    (solid.centre - half, solid.centre + half)
}

fn tower_chord_void_separates(
    plan: &BuildingPlan,
    shell: &ResolvedSolid,
    other: &ResolvedSolid,
) -> bool {
    let Some((tower_index, tower)) = plan
        .wall_assemblies
        .iter()
        .find(|wall| wall.owner == shell.owner)
        .and_then(|wall| match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => {
                Some((tower_index, plan.towers.get(tower_index)?))
            }
            _ => None,
        })
    else {
        return false;
    };
    let _ = tower_index;
    let (min, max) = resolved_solid_bounds(other);
    let centre = Vec2::new((min.x + max.x) * 0.5, (min.z + max.z) * 0.5);
    let half = Vec2::new((max.x - min.x) * 0.5, (max.z - min.z) * 0.5);
    tower.chord_interfaces().any(|interface| {
        let toward = match interface.toward_gate {
            crate::Direction::North => Vec2::Y,
            crate::Direction::East => Vec2::X,
            crate::Direction::South => -Vec2::Y,
            crate::Direction::West => -Vec2::X,
        };
        let minimum_projection = (centre - tower.centre_metres()).dot(toward)
            - half.x * toward.x.abs()
            - half.y * toward.y.abs();
        minimum_projection >= tower.radius_metres() - interface.bearing_depth.metres() - 0.025
    })
}

fn point_is_inside_tower_chord_void(
    plan: &BuildingPlan,
    shell: &ResolvedSolid,
    point: Vec3,
) -> bool {
    let Some(tower) = plan
        .wall_assemblies
        .iter()
        .find(|wall| wall.owner == shell.owner)
        .and_then(|wall| match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => plan.towers.get(tower_index),
            _ => None,
        })
    else {
        return false;
    };
    tower.chord_interfaces().any(|interface| {
        let toward = match interface.toward_gate {
            crate::Direction::North => Vec2::Y,
            crate::Direction::East => Vec2::X,
            crate::Direction::South => -Vec2::Y,
            crate::Direction::West => -Vec2::X,
        };
        (Vec2::new(point.x, point.z) - tower.centre_metres()).dot(toward)
            >= tower.radius_metres() - interface.bearing_depth.metres() - 0.025
    })
}

fn segment_is_inside_tower_chord_void(
    plan: &BuildingPlan,
    shell: &ResolvedSolid,
    start: Vec3,
    end: Vec3,
) -> bool {
    point_is_inside_tower_chord_void(plan, shell, start)
        && point_is_inside_tower_chord_void(plan, shell, end)
}

fn valid_tower_chord_bond(plan: &BuildingPlan, bond: &crate::JunctionBond) -> bool {
    for (shell_owner, target_owner) in [
        (bond.owners[0], bond.owners[1]),
        (bond.owners[1], bond.owners[0]),
    ] {
        let Some(shell) = plan.resolved_geometry.solids.iter().find(|solid| {
            solid.owner == shell_owner
                && matches!(
                    solid.shape,
                    crate::ResolvedSolidShape::RoundTowerShell { .. }
                )
        }) else {
            continue;
        };
        if plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == target_owner)
            .any(|solid| tower_chord_void_separates(plan, shell, solid))
            && bond.minimum_interface_area_square_metres >= 0.08
            && bond.maximum_penetration_metres <= 0.08
        {
            return true;
        }
    }
    false
}

fn resolved_solid_contains_point(solid: &ResolvedSolid, point: Vec3, tolerance: f32) -> bool {
    let relative = point - solid.centre;
    let (sine, cosine) = solid.yaw_radians.sin_cos();
    let local = Vec3::new(
        relative.x * cosine - relative.z * sine,
        relative.y,
        relative.x * sine + relative.z * cosine,
    );
    let half = solid.size * 0.5 + Vec3::splat(tolerance);
    if !local.abs().cmple(half).all() {
        return false;
    }
    match solid.shape {
        crate::ResolvedSolidShape::AnnularPrism {
            inner_radius_metres,
            outer_radius_metres,
            ..
        } => {
            let radius = Vec2::new(local.x, local.z).length();
            radius >= inner_radius_metres - tolerance && radius <= outer_radius_metres + tolerance
        }
        crate::ResolvedSolidShape::AnnularSectorPrism {
            inner_radius_metres,
            outer_radius_metres,
            start_angle_radians,
            end_angle_radians,
            ..
        } => {
            let radius = Vec2::new(local.x, local.z).length();
            let angle = local.z.atan2(local.x).rem_euclid(std::f32::consts::TAU);
            let start = start_angle_radians.rem_euclid(std::f32::consts::TAU);
            let sweep = (end_angle_radians - start_angle_radians)
                .rem_euclid(std::f32::consts::TAU)
                .max(0.0001);
            radius >= inner_radius_metres - tolerance
                && radius <= outer_radius_metres + tolerance
                && (angle - start).rem_euclid(std::f32::consts::TAU) <= sweep + 0.0001
        }
        crate::ResolvedSolidShape::RoundTowerShell {
            inner_radius_metres,
            outer_radius_metres,
            ..
        } => {
            let radius = Vec2::new(local.x, local.z).length();
            radius >= inner_radius_metres - tolerance && radius <= outer_radius_metres + tolerance
        }
        _ => true,
    }
}

fn artillery_route_solid_contains(solid: &ResolvedSolid, point: Vec3, tolerance: f32) -> bool {
    if let crate::ResolvedSolidShape::RoundTowerShell {
        outer_radius_metres,
        chord_interfaces,
        ..
    } = solid.shape
    {
        let radial =
            Vec2::new(point.x - solid.centre.x, point.z - solid.centre.z).normalize_or_zero();
        if chord_interfaces.into_iter().flatten().any(|interface| {
            radial.dot(direction_vector(interface.toward_gate))
                > (outer_radius_metres - interface.bearing_depth.metres()) / outer_radius_metres
        }) {
            return false;
        }
    }
    resolved_solid_contains_point(solid, point, tolerance)
}

fn opening_host_contains_point(
    opening: &crate::OpeningAssembly,
    wall: &crate::WallAssembly,
    solid: &ResolvedSolid,
    point: Vec3,
) -> bool {
    if !resolved_solid_contains_point(solid, point, 0.001) {
        return false;
    }
    let plan = Vec2::new(point.x, point.z);
    let along = (plan - opening.frame.origin).dot(opening.frame.tangent);
    let depth = (plan - opening.frame.origin).dot(opening.frame.outward);
    let depth_fraction = (0.5 - depth / wall.thickness_metres).clamp(0.0, 1.0);
    match solid.shape {
        crate::ResolvedSolidShape::SplayedReveal {
            exterior_width_metres,
            interior_width_metres,
            side,
            ..
        } => {
            let clear_width = exterior_width_metres
                + (interior_width_metres - exterior_width_metres) * depth_fraction;
            if side < 0 {
                along <= -clear_width * 0.5 + 0.001
            } else {
                along >= clear_width * 0.5 - 0.001
            }
        }
        crate::ResolvedSolidShape::SplayedHead {
            exterior_clear_height_metres,
            interior_clear_height_metres,
            ..
        } => {
            let clear_height = exterior_clear_height_metres
                + (interior_clear_height_metres - exterior_clear_height_metres) * depth_fraction;
            point.y + 0.001 >= opening.sill_elevation_metres + clear_height
        }
        _ => true,
    }
}

fn bounds_overlap_3d(a: (Vec3, Vec3), b: (Vec3, Vec3), tolerance: f32) -> bool {
    a.1.x.min(b.1.x) - a.0.x.max(b.0.x) > tolerance
        && a.1.y.min(b.1.y) - a.0.y.max(b.0.y) > tolerance
        && a.1.z.min(b.1.z) - a.0.z.max(b.0.z) > tolerance
}

fn resolved_shape_overlap(a: &ResolvedSolid, b: &ResolvedSolid, tolerance: f32) -> bool {
    if !bounds_overlap_3d(
        resolved_solid_bounds(a),
        resolved_solid_bounds(b),
        tolerance,
    ) {
        return false;
    }
    if !matches!(
        a.shape,
        crate::ResolvedSolidShape::AnnularPrism { .. }
            | crate::ResolvedSolidShape::AnnularSectorPrism { .. }
    ) && !matches!(
        b.shape,
        crate::ResolvedSolidShape::AnnularPrism { .. }
            | crate::ResolvedSolidShape::AnnularSectorPrism { .. }
    ) {
        return true;
    }
    let (amin, amax) = resolved_solid_bounds(a);
    let (bmin, bmax) = resolved_solid_bounds(b);
    let min = amin.max(bmin);
    let max = amax.min(bmax);
    (0..=8).any(|x| {
        (0..=4).any(|y| {
            (0..=8).any(|z| {
                let point = Vec3::new(
                    min.x + (max.x - min.x) * x as f32 / 8.0,
                    min.y + (max.y - min.y) * y as f32 / 4.0,
                    min.z + (max.z - min.z) * z as f32 / 8.0,
                );
                resolved_solid_contains_point(a, point, -tolerance)
                    && resolved_solid_contains_point(b, point, -tolerance)
            })
        })
    })
}

fn resolved_shape_overlaps_bounds(
    solid: &ResolvedSolid,
    bounds: (Vec3, Vec3),
    tolerance: f32,
) -> bool {
    if !matches!(
        solid.shape,
        crate::ResolvedSolidShape::AnnularPrism { .. }
            | crate::ResolvedSolidShape::AnnularSectorPrism { .. }
    ) {
        return resolved_solid_overlaps_bounds(solid, bounds, tolerance);
    }
    (0..=8).any(|x| {
        (0..=4).any(|y| {
            (0..=8).any(|z| {
                let point = Vec3::new(
                    bounds.0.x + (bounds.1.x - bounds.0.x) * x as f32 / 8.0,
                    bounds.0.y + (bounds.1.y - bounds.0.y) * y as f32 / 4.0,
                    bounds.0.z + (bounds.1.z - bounds.0.z) * z as f32 / 8.0,
                );
                resolved_solid_contains_point(solid, point, -tolerance)
            })
        })
    })
}

fn oriented_occupant_overlaps_solid(
    foot: Vec3,
    along: Vec2,
    across: Vec2,
    solid: &ResolvedSolid,
    tolerance: f32,
) -> bool {
    let (solid_min, solid_max) = resolved_solid_bounds(solid);
    if solid_max.y.min(foot.y + 1.90) - solid_min.y.max(foot.y) <= tolerance {
        return false;
    }
    let cosine = solid.yaw_radians.cos();
    let sine = solid.yaw_radians.sin();
    let solid_x = Vec2::new(cosine, -sine);
    let solid_z = Vec2::new(sine, cosine);
    let delta = Vec2::new(solid.centre.x - foot.x, solid.centre.z - foot.z);
    [along, across, solid_x, solid_z].into_iter().all(|axis| {
        let occupant_radius = 0.10 * along.dot(axis).abs() + 0.45 * across.dot(axis).abs();
        let solid_radius = solid.size.x * 0.5 * solid_x.dot(axis).abs()
            + solid.size.z * 0.5 * solid_z.dot(axis).abs();
        occupant_radius + solid_radius - delta.dot(axis).abs() > tolerance
    })
}

fn resolved_solid_overlaps_bounds(
    solid: &ResolvedSolid,
    bounds: (Vec3, Vec3),
    tolerance: f32,
) -> bool {
    let bounds_centre = (bounds.0 + bounds.1) * 0.5;
    let bounds_half = (bounds.1 - bounds.0) * 0.5;
    let rotation = Quat::from_rotation_y(solid.yaw_radians)
        * Quat::from_rotation_x(solid.crossfall_radians)
        * Quat::from_rotation_z(solid.longfall_radians);
    let solid_axes = [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z];
    let world_axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let solid_half = solid.size * 0.5;
    let delta = bounds_centre - solid.centre;
    let mut axes = Vec::with_capacity(15);
    axes.extend(world_axes);
    axes.extend(solid_axes);
    for world in world_axes {
        for local in solid_axes {
            let cross = world.cross(local);
            if cross.length_squared() > 0.000_001 {
                axes.push(cross.normalize());
            }
        }
    }
    axes.into_iter().all(|axis| {
        let solid_radius = solid_half.x * solid_axes[0].dot(axis).abs()
            + solid_half.y * solid_axes[1].dot(axis).abs()
            + solid_half.z * solid_axes[2].dot(axis).abs();
        let bounds_radius = bounds_half.x * axis.x.abs()
            + bounds_half.y * axis.y.abs()
            + bounds_half.z * axis.z.abs();
        solid_radius + bounds_radius - delta.dot(axis).abs() > tolerance
    })
}

fn oriented_cuboids_overlap(a: &ResolvedSolid, b: &ResolvedSolid, tolerance: f32) -> bool {
    let rotation = |solid: &ResolvedSolid| {
        Quat::from_rotation_y(solid.yaw_radians)
            * Quat::from_rotation_x(solid.crossfall_radians)
            * Quat::from_rotation_z(solid.longfall_radians)
    };
    let a_rotation = rotation(a);
    let b_rotation = rotation(b);
    let a_axes = [
        a_rotation * Vec3::X,
        a_rotation * Vec3::Y,
        a_rotation * Vec3::Z,
    ];
    let b_axes = [
        b_rotation * Vec3::X,
        b_rotation * Vec3::Y,
        b_rotation * Vec3::Z,
    ];
    let delta = b.centre - a.centre;
    let a_half = a.size * 0.5;
    let b_half = b.size * 0.5;
    let radius = |half: Vec3, axes: [Vec3; 3], axis: Vec3| {
        half.x * axes[0].dot(axis).abs()
            + half.y * axes[1].dot(axis).abs()
            + half.z * axes[2].dot(axis).abs()
    };
    let mut axes = Vec::with_capacity(15);
    axes.extend(a_axes);
    axes.extend(b_axes);
    for left in a_axes {
        for right in b_axes {
            let cross = left.cross(right);
            if cross.length_squared() > 0.000_001 {
                axes.push(cross.normalize());
            }
        }
    }
    axes.into_iter().all(|axis| {
        radius(a_half, a_axes, axis) + radius(b_half, b_axes, axis) - delta.dot(axis).abs()
            > tolerance
    })
}

fn resolved_solids_overlap_positive_volume(
    left: &ResolvedSolid,
    right: &ResolvedSolid,
    tolerance: f32,
) -> bool {
    let left_vertical = (
        left.centre.y - left.size.y * 0.5,
        left.centre.y + left.size.y * 0.5,
    );
    let right_vertical = (
        right.centre.y - right.size.y * 0.5,
        right.centre.y + right.size.y * 0.5,
    );
    if left_vertical.1.min(right_vertical.1) - left_vertical.0.max(right_vertical.0) <= tolerance {
        return false;
    }
    let left_x = Vec2::new(left.yaw_radians.cos(), -left.yaw_radians.sin());
    let left_z = Vec2::new(left.yaw_radians.sin(), left.yaw_radians.cos());
    let right_x = Vec2::new(right.yaw_radians.cos(), -right.yaw_radians.sin());
    let right_z = Vec2::new(right.yaw_radians.sin(), right.yaw_radians.cos());
    let delta = Vec2::new(
        right.centre.x - left.centre.x,
        right.centre.z - left.centre.z,
    );
    [left_x, left_z, right_x, right_z].into_iter().all(|axis| {
        let left_radius =
            left.size.x * 0.5 * left_x.dot(axis).abs() + left.size.z * 0.5 * left_z.dot(axis).abs();
        let right_radius = right.size.x * 0.5 * right_x.dot(axis).abs()
            + right.size.z * 0.5 * right_z.dot(axis).abs();
        left_radius + right_radius - delta.dot(axis).abs() > tolerance
    })
}

fn resolved_plan_overlap_area(left: &ResolvedSolid, right: &ResolvedSolid) -> f32 {
    let local_x = Vec2::new(left.yaw_radians.cos(), -left.yaw_radians.sin());
    let local_z = Vec2::new(left.yaw_radians.sin(), left.yaw_radians.cos());
    let delta = Vec2::new(
        right.centre.x - left.centre.x,
        right.centre.z - left.centre.z,
    );
    let right_x = Vec2::new(right.yaw_radians.cos(), -right.yaw_radians.sin());
    let right_z = Vec2::new(right.yaw_radians.sin(), right.yaw_radians.cos());
    let overlap = |axis: Vec2, left_extent: f32| {
        let right_extent = right.size.x * 0.5 * right_x.dot(axis).abs()
            + right.size.z * 0.5 * right_z.dot(axis).abs();
        (left_extent + right_extent - delta.dot(axis).abs()).max(0.0)
    };
    overlap(local_x, left.size.x * 0.5) * overlap(local_z, left.size.z * 0.5)
}

fn bonded_interface_metrics(
    a: &ResolvedSolid,
    b: &ResolvedSolid,
) -> Option<(Vec3, Vec3, f32, f32)> {
    let (a_min, a_max) = resolved_solid_bounds(a);
    let (b_min, b_max) = resolved_solid_bounds(b);
    let signed = a_max.min(b_max) - a_min.max(b_min);
    let mut axes = [(signed.x, 0_usize), (signed.y, 1), (signed.z, 2)];
    axes.sort_by(|left, right| left.0.total_cmp(&right.0));
    if axes[0].0 < -0.025 || axes[1].0 <= 0.0 || axes[2].0 <= 0.0 {
        return None;
    }
    let contact_min = a_min.max(b_min);
    let mut contact_max = a_max.min(b_max);
    if axes[0].0 < 0.0 {
        let axis = axes[0].1;
        let midpoint = (contact_min[axis] + contact_max[axis]) * 0.5;
        contact_max[axis] = midpoint;
    }
    Some((
        contact_min.min(contact_max),
        contact_min.max(contact_max),
        axes[1].0 * axes[2].0,
        axes[0].0.max(0.0),
    ))
}

/// Conservative cuboid-in-cavity test for resolved round shells.  This keeps
/// the generic AABB overlap sweep from treating a gun mount or casemate fitting
/// wholly inside the hollow cylinder as masonry penetration.
fn round_shell_clears_inner_solid(shell: &ResolvedSolid, inner: &ResolvedSolid) -> bool {
    let crate::ResolvedSolidShape::RoundTowerShell {
        inner_radius_metres,
        ..
    } = shell.shape
    else {
        return false;
    };
    let (min, max) = resolved_solid_bounds(inner);
    [
        Vec2::new(min.x, min.z),
        Vec2::new(min.x, max.z),
        Vec2::new(max.x, min.z),
        Vec2::new(max.x, max.z),
    ]
    .into_iter()
    .all(|corner| {
        corner.distance(Vec2::new(shell.centre.x, shell.centre.z)) <= inner_radius_metres - 0.005
    })
}

fn audit_resolved_geometry(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if plan.resolved_geometry.schema_version != 2 {
        issues.push(issue(
            "stale_resolver_schema",
            "resolved geometry does not use resolver schema 2".to_owned(),
        ));
    }
    let owners = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| node.owner)
        .collect::<std::collections::HashSet<_>>();
    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let mut item_ids = std::collections::HashSet::new();
    for (index, solid) in plan.resolved_geometry.solids.iter().enumerate() {
        if solid.size.min_element() <= 0.0
            || !owners.contains(&solid.owner)
            || !item_ids.insert(solid.id)
            || solid.supported_by.is_empty()
            || solid
                .supported_by
                .iter()
                .any(|node| !nodes.contains_key(node))
        {
            issues.push(issue(
                "invalid_resolved_geometry",
                format!("resolved solid {index} {:?} owner={} centre={:?} size={:?} support={:?} has invalid extent, owner, or support provenance",solid.role,solid.owner.0,solid.centre,solid.size,solid.supported_by),
            ));
        }
        let has_bearing = plan
            .resolved_geometry
            .support_interfaces
            .iter()
            .any(|bearing| {
                let timber_member = matches!(
                    solid.role,
                    SolidRole::FrameSill
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
                );
                bearing.owner == solid.owner
                    && solid.supported_by.contains(&bearing.node)
                    && if timber_member {
                        resolved_solid_overlaps_bounds(
                            solid,
                            (bearing.bounds.min, bearing.bounds.max),
                            0.001,
                        )
                    } else {
                        bounds_overlap_3d(
                            resolved_solid_bounds(solid),
                            (bearing.bounds.min, bearing.bounds.max),
                            0.001,
                        )
                    }
            });
        if !has_bearing {
            issues.push(issue(
                "missing_positive_bearing",
                format!(
                    "resolved solid {} has no positive bearing interface",
                    solid.id.0
                ),
            ));
        }
    }
    for surface in &plan.resolved_geometry.surfaces {
        if !item_ids.insert(surface.id) {
            issues.push(issue(
                "duplicate_resolved_item_id",
                format!(
                    "duplicate surface item {} owner {} role {:?}",
                    surface.id.0, surface.owner.0, surface.role
                ),
            ));
        }
    }
    for void in &plan.resolved_geometry.voids {
        if !item_ids.insert(void.id) {
            issues.push(issue(
                "duplicate_resolved_item_id",
                format!("duplicate item {}", void.id.0),
            ));
        }
    }
    fn reaches_ground(
        id: crate::StructuralNodeId,
        nodes: &std::collections::HashMap<crate::StructuralNodeId, &crate::StructuralNode>,
        visiting: &mut std::collections::HashSet<crate::StructuralNodeId>,
    ) -> bool {
        let Some(node) = nodes.get(&id) else {
            return false;
        };
        if node.grounded {
            return true;
        }
        if !visiting.insert(id) {
            return false;
        }
        let reaches = node
            .supported_by
            .iter()
            .all(|parent| reaches_ground(*parent, nodes, visiting));
        visiting.remove(&id);
        reaches && !node.supported_by.is_empty()
    }
    for node in nodes.values() {
        if !reaches_ground(node.id, &nodes, &mut std::collections::HashSet::new()) {
            issues.push(issue(
                "unsupported_resolved_structure",
                format!(
                    "structural node {} {:?} at {:?} supports {:?} does not reach ground through an acyclic graph",
                    node.id.0, node.kind, node.position, node.supported_by
                ),
            ));
        }
    }
    for left in 0..plan.resolved_geometry.solids.len() {
        for right in (left + 1)..plan.resolved_geometry.solids.len() {
            let a = &plan.resolved_geometry.solids[left];
            let b = &plan.resolved_geometry.solids[right];
            let separated_by_chord =
                (matches!(a.shape, crate::ResolvedSolidShape::RoundTowerShell { .. })
                    && tower_chord_void_separates(plan, a, b))
                    || (matches!(b.shape, crate::ResolvedSolidShape::RoundTowerShell { .. })
                        && tower_chord_void_separates(plan, b, a));
            let enclosed_by_round_shell =
                round_shell_clears_inner_solid(a, b) || round_shell_clears_inner_solid(b, a);
            if a.owner != b.owner
                && !separated_by_chord
                && !enclosed_by_round_shell
                && resolved_shape_overlap(a, b, 0.025)
            {
                let (a_min, a_max) = resolved_solid_bounds(a);
                let (b_min, b_max) = resolved_solid_bounds(b);
                let overlap_min = a_min.max(b_min);
                let overlap_max = a_max.min(b_max);
                let overlap_size = overlap_max - overlap_min;
                let penetration = overlap_size.x.min(overlap_size.z);
                let valid_drain_contact =
                    plan.resolved_geometry
                        .roof_drainage_outlets
                        .iter()
                        .any(|station| {
                            let pair = [(a, b), (b, a)];
                            pair.into_iter().any(|(spout, host_solid)| {
                                station.downspout == Some(spout.id)
                                    && spout.role == SolidRole::RoofGutter
                                    && station.host_wall.is_some_and(|host_id| {
                                        plan.wall_assemblies.iter().any(|wall| {
                                            wall.id == host_id
                                                && wall.host_solids.contains(&host_solid.id)
                                        })
                                    })
                            })
                        });
                let is_timber_frame_role = |role: SolidRole| {
                    matches!(
                        role,
                        SolidRole::FrameSill
                            | SolidRole::FramePost
                            | SolidRole::FramePlate
                            | SolidRole::FrameRail
                            | SolidRole::FrameJoist
                            | SolidRole::FrameGirder
                            | SolidRole::FrameTie
                            | SolidRole::FrameBrace
                            | SolidRole::FrameJettyBeam
                            | SolidRole::FrameKnagge
                            | SolidRole::FrameFloor
                            | SolidRole::FrameGableMember
                            | SolidRole::FrameDormerTrimmer
                            | SolidRole::FrameOrnament
                    )
                };
                let valid_frame_opening_contact = plan.timber_frame.as_ref().is_some_and(|frame| {
                    [(a, b), (b, a)].into_iter().any(|(frame_solid, closure)| {
                        closure.role == SolidRole::OpeningClosure
                            && frame.members.iter().any(|member| {
                                member.solid == frame_solid.id
                                    && matches!(
                                        member.role,
                                        crate::TimberMemberRole::PrimaryPost
                                            | crate::TimberMemberRole::CornerPost
                                            | crate::TimberMemberRole::IntermediatePost
                                            | crate::TimberMemberRole::Rail
                                    )
                                    && (frame.bays.iter().any(|bay| {
                                        bay.member_ids.contains(&member.id)
                                            && bay.opening.is_some_and(|opening_id| {
                                                plan.opening_assemblies.iter().any(|opening| {
                                                    opening.id == opening_id
                                                        && opening
                                                            .closure_solids
                                                            .contains(&closure.id)
                                                })
                                            })
                                    }) || plan.opening_assemblies.iter().any(|opening| {
                                        if !opening.closure_solids.contains(&closure.id) {
                                            return false;
                                        }
                                        let Some(void) = plan
                                            .resolved_geometry
                                            .voids
                                            .iter()
                                            .find(|void| void.id == opening.void_id)
                                        else {
                                            return false;
                                        };
                                        let size = void.bounds.max - void.bounds.min;
                                        let half = (size.x * opening.frame.tangent.x.abs()
                                            + size.z * opening.frame.tangent.y.abs())
                                            * 0.5;
                                        let point = Vec2::new(member.start.x, member.start.z);
                                        ((point - opening.frame.origin)
                                            .dot(opening.frame.tangent)
                                            .abs()
                                            - half)
                                            .abs()
                                            <= member.section_metres.x * 0.6 + 0.02
                                    }))
                            })
                    })
                });
                let valid_frame_contact = (is_timber_frame_role(a.role)
                    || is_timber_frame_role(b.role))
                    && (!oriented_cuboids_overlap(a, b, 0.012)
                        || matches!(
                            (a.role, b.role),
                            (
                                SolidRole::FrameSill
                                    | SolidRole::FramePost
                                    | SolidRole::FramePlate
                                    | SolidRole::FrameRail
                                    | SolidRole::FrameJoist
                                    | SolidRole::FrameGirder
                                    | SolidRole::FrameTie
                                    | SolidRole::FrameBrace
                                    | SolidRole::FrameJettyBeam
                                    | SolidRole::FrameKnagge
                                    | SolidRole::FrameFloor
                                    | SolidRole::FrameGableMember
                                    | SolidRole::FrameDormerTrimmer,
                                SolidRole::WallHost
                                    | SolidRole::OpeningJamb
                                    | SolidRole::OpeningSill
                                    | SolidRole::OpeningHead
                                    | SolidRole::OpeningSpandrel
                            ) | (
                                SolidRole::WallHost
                                    | SolidRole::OpeningJamb
                                    | SolidRole::OpeningSill
                                    | SolidRole::OpeningHead
                                    | SolidRole::OpeningSpandrel,
                                SolidRole::FrameSill
                                    | SolidRole::FramePost
                                    | SolidRole::FramePlate
                                    | SolidRole::FrameRail
                                    | SolidRole::FrameJoist
                                    | SolidRole::FrameGirder
                                    | SolidRole::FrameTie
                                    | SolidRole::FrameBrace
                                    | SolidRole::FrameJettyBeam
                                    | SolidRole::FrameKnagge
                                    | SolidRole::FrameFloor
                                    | SolidRole::FrameGableMember
                                    | SolidRole::FrameDormerTrimmer
                            ) | (
                                SolidRole::FrameDormerTrimmer,
                                SolidRole::RoofFlashing | SolidRole::RoofFraming
                            ) | (
                                SolidRole::RoofFlashing | SolidRole::RoofFraming,
                                SolidRole::FrameDormerTrimmer
                            ) | (SolidRole::FramePost, SolidRole::RoofFlashing)
                                | (SolidRole::RoofFlashing, SolidRole::FramePost)
                        ));
                let valid_frame_gutter_contact = matches!(
                    (a.role, b.role),
                    (SolidRole::FrameFloor, SolidRole::RoofGutter)
                        | (SolidRole::RoofGutter, SolidRole::FrameFloor)
                ) && penetration <= 0.10
                    || plan.timber_frame.as_ref().is_some_and(|frame| {
                        [(a, b), (b, a)].into_iter().any(|(timber, gutter)| {
                            timber.role == SolidRole::FrameGableMember
                                && gutter.role == SolidRole::RoofGutter
                                && penetration
                                    <= frame
                                        .members
                                        .iter()
                                        .find(|member| member.solid == timber.id)
                                        .map_or(0.0, |member| {
                                            member.section_metres.max_element() + 0.05
                                        })
                                && frame.members.iter().any(|member| {
                                    member.solid == timber.id
                                        && (member
                                            .support_interfaces
                                            .iter()
                                            .chain(&frame.roof_bearing_interfaces)
                                            .any(|id| {
                                                plan.resolved_geometry
                                                    .support_interfaces
                                                    .iter()
                                                    .find(|interface| interface.id == *id)
                                                    .is_some_and(|interface| {
                                                        resolved_solid_overlaps_bounds(
                                                            timber,
                                                            (
                                                                interface.bounds.min,
                                                                interface.bounds.max,
                                                            ),
                                                            0.001,
                                                        ) && resolved_solid_overlaps_bounds(
                                                            gutter,
                                                            (
                                                                interface.bounds.min,
                                                                interface.bounds.max,
                                                            ),
                                                            0.001,
                                                        )
                                                    })
                                            })
                                            || {
                                                let (min, max) = resolved_solid_bounds(gutter);
                                                let endpoint_radius =
                                                    member.section_metres.max_element() * 0.6
                                                        + 0.02;
                                                [member.start, member.end].into_iter().any(
                                                    |point| {
                                                        point
                                                            .cmpge(
                                                                min - Vec3::splat(endpoint_radius),
                                                            )
                                                            .all()
                                                            && point
                                                                .cmple(
                                                                    max + Vec3::splat(
                                                                        endpoint_radius,
                                                                    ),
                                                                )
                                                                .all()
                                                    },
                                                )
                                            })
                                })
                        })
                    });
                let valid_frame_roof_contact = plan.timber_frame.as_ref().is_some_and(|frame| {
                    [(a, b), (b, a)].into_iter().any(|(timber, roof)| {
                        is_timber_frame_role(timber.role)
                            && matches!(roof.role, SolidRole::RoofFraming | SolidRole::RoofPlate)
                            && frame.members.iter().any(|member| {
                                member.solid == timber.id
                                    && member.support_interfaces.iter().any(|id| {
                                        plan.resolved_geometry
                                            .support_interfaces
                                            .iter()
                                            .find(|interface| interface.id == *id)
                                            .is_some_and(|interface| {
                                                resolved_solid_overlaps_bounds(
                                                    roof,
                                                    (interface.bounds.min, interface.bounds.max),
                                                    0.015,
                                                )
                                            })
                                    })
                            })
                    })
                });
                let valid_child_gable_verge_contact =
                    plan.timber_frame.as_ref().is_some_and(|frame| {
                        [(a, b), (b, a)].into_iter().any(|(timber, verge)| {
                            verge.role == SolidRole::RoofEdgeTreatment
                                && frame.members.iter().any(|member| {
                                    member.solid == timber.id
                                        && matches!(
                                            member.role,
                                            crate::TimberMemberRole::WallPlate
                                                | crate::TimberMemberRole::GablePost
                                                | crate::TimberMemberRole::Rafter
                                        )
                                        && penetration
                                            <= member.section_metres.max_element() + 0.05
                                        && frame.bays.iter().any(|bay| {
                                            bay.member_ids.contains(&member.id)
                                                && bay.wall.is_some_and(|wall_id| {
                                                    plan.wall_assemblies
                                                        .iter()
                                                        .find(|wall| wall.id == wall_id)
                                                        .is_some_and(|wall| {
                                                            matches!(
                                                                wall.source,
                                                                crate::WallSourceId::RoofChildFront { roof }
                                                                    if plan.roof_assemblies.iter().any(|child| {
                                                                        child.id == roof
                                                                            && child.owner == verge.owner
                                                                    })
                                                            )
                                                        })
                                                })
                                        })
                                })
                        })
                    });
                let valid_bond = valid_drain_contact
                    || valid_frame_opening_contact
                    || valid_frame_contact
                    || valid_frame_gutter_contact
                    || valid_frame_roof_contact
                    || valid_child_gable_verge_contact
                    || plan.resolved_geometry.junction_bonds.iter().any(|bond| {
                        bond.owners.contains(&a.owner)
                            && bond.owners.contains(&b.owner)
                            && overlap_min
                                .cmpge(bond.bounds.min - Vec3::splat(0.025))
                                .all()
                            && overlap_max
                                .cmple(bond.bounds.max + Vec3::splat(0.025))
                                .all()
                            && penetration <= bond.maximum_penetration_metres + 0.025
                            && bond.minimum_interface_area_square_metres > 0.0
                            && (bond.maximum_penetration_metres <= 0.18
                                || matches!(
                                    (a.role, b.role),
                                    (
                                        SolidRole::RoofFlashing,
                                        SolidRole::WallHost
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                    ) | (
                                        SolidRole::WallHost
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                            | SolidRole::ArtilleryRevetment
                                            | SolidRole::ArtilleryEarthCore
                                            | SolidRole::ArtilleryRetainingWall,
                                        SolidRole::RoofFlashing
                                    )
                                )
                                || matches!(
                                    (a.role, b.role),
                                    (
                                        SolidRole::WallHost
                                            | SolidRole::DefenseHostWall
                                            | SolidRole::CircuitWalk
                                            | SolidRole::LoadBearing
                                            | SolidRole::Breastwork
                                            | SolidRole::WalkSurface
                                            | SolidRole::DrainageChannel
                                            | SolidRole::Landing
                                            | SolidRole::DefenseHostButtress
                                            | SolidRole::ProjectionSupport
                                            | SolidRole::GalleryFloor
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningSill
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                            | SolidRole::ArtilleryRevetment
                                            | SolidRole::ArtilleryEarthCore
                                            | SolidRole::ArtilleryRetainingWall,
                                        SolidRole::WallHost
                                            | SolidRole::DefenseHostWall
                                            | SolidRole::CircuitWalk
                                            | SolidRole::LoadBearing
                                            | SolidRole::Breastwork
                                            | SolidRole::WalkSurface
                                            | SolidRole::DrainageChannel
                                            | SolidRole::Landing
                                            | SolidRole::DefenseHostButtress
                                            | SolidRole::ProjectionSupport
                                            | SolidRole::GalleryFloor
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningSill
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                            | SolidRole::ArtilleryRevetment
                                            | SolidRole::ArtilleryEarthCore
                                            | SolidRole::ArtilleryRetainingWall
                                    )
                                ))
                    });
                if !valid_bond {
                    issues.push(issue(
                        "undeclared_solid_overlap",
                        format!(
                            "resolved {} owner {} {:?} at {:?} size {:?} and {} owner {} {:?} at {:?} size {:?} overlap beyond a local bonded junction timber={:?}",
                            a.id.0, a.owner.0, a.role, a.centre, a.size,
                            b.id.0, b.owner.0, b.role, b.centre, b.size,
                            plan.timber_frame.as_ref().and_then(|frame| frame.members.iter().find(|member| member.solid == a.id || member.solid == b.id)).map(|member| (member.role, member.start, member.end, member.section_metres))
                        ),
                    ));
                }
            }
        }
    }
    for bond in &plan.resolved_geometry.junction_bonds {
        let valid_interface = valid_tower_chord_bond(plan, bond)
            || plan.resolved_geometry.solids.iter().any(|a| {
                a.owner == bond.owners[0]
                    && plan.resolved_geometry.solids.iter().any(|b| {
                        b.owner == bond.owners[1]
                            && bonded_interface_metrics(a, b).is_some_and(
                                |(contact_min, contact_max, area, penetration)| {
                                    contact_min
                                        .cmpge(bond.bounds.min - Vec3::splat(0.025))
                                        .all()
                                        && contact_max
                                            .cmple(bond.bounds.max + Vec3::splat(0.025))
                                            .all()
                                        && area + 0.005 >= bond.minimum_interface_area_square_metres
                                        && penetration <= bond.maximum_penetration_metres + 0.025
                                },
                            )
                    })
            });
        if !valid_interface {
            issues.push(issue(
                "invalid_crown_junction_bond",
                format!(
                    "crown bond {} does not contain a positive-area, gap-limited interface",
                    bond.id.0
                ),
            ));
        }
    }
    for (index, void) in plan.resolved_geometry.voids.iter().enumerate() {
        let void_bounds = (void.bounds.min, void.bounds.max);
        let blocking_solid = plan.resolved_geometry.solids.iter().find(|solid| {
            let exact_opening_piece = (void.role == VoidRole::WallOpening)
                && plan.opening_assemblies.iter().any(|opening| {
                    opening.void_id == void.id
                        && (opening.jamb_solids.contains(&solid.id)
                            || opening.sill_solid == Some(solid.id)
                            || opening.head_solid == solid.id
                            || opening.spandrel_solid == solid.id)
                });
            let round_shell_clears_central_room = void.role == VoidRole::ArtilleryCasemate
                && matches!(solid.shape, crate::ResolvedSolidShape::RoundTowerShell { inner_radius_metres, .. } if {
                    let corners = [
                        Vec2::new(void.bounds.min.x, void.bounds.min.z),
                        Vec2::new(void.bounds.min.x, void.bounds.max.z),
                        Vec2::new(void.bounds.max.x, void.bounds.min.z),
                        Vec2::new(void.bounds.max.x, void.bounds.max.z),
                    ];
                    corners.into_iter().all(|corner| corner.distance(Vec2::new(solid.centre.x, solid.centre.z)) < inner_radius_metres - 0.01)
                });
            let artillery_scupper_subtraction = void.role == VoidRole::Drain
                && plan.artillery_castle.as_ref().is_some_and(|castle| {
                    castle
                        .drainage_routes
                        .iter()
                        .chain(&castle.ditch.drainage_routes)
                        .any(|route_id| {
                            plan.resolved_geometry.drainage_routes.iter().any(|route| {
                                route.id == *route_id && route.outlet_void == void.id
                            })
                        })
                })
                && matches!(
                    solid.role,
                    SolidRole::ArtilleryRevetment
                        | SolidRole::ArtilleryEarthCore
                        | SolidRole::WallHost
                        | SolidRole::DrainageFloor
                );
            let roof_gutter_outlet_subtraction = void.role == VoidRole::Drain
                && solid.role == SolidRole::RoofGutter
                && plan
                    .resolved_geometry
                    .roof_drainage_outlets
                    .iter()
                    .find(|station| station.outlet_void == void.id)
                    .is_some_and(|station| {
                        station.member_networks.iter().any(|network_id| {
                            plan.resolved_geometry
                                .roof_drainage_networks
                                .iter()
                                .find(|network| network.id == *network_id)
                                .is_some_and(|network| {
                                    network.channel_floor == solid.id
                                        || network.channel_lips.contains(&solid.id)
                                        || network.collector_solids.contains(&solid.id)
                                })
                        })
                    });
            let casemate_furnishing = void.role == VoidRole::ArtilleryCasemate
                && matches!(solid.role, SolidRole::ArtilleryStairTread | SolidRole::WeaponMount);
            solid.owner == void.subtracts_from
                && !round_shell_clears_central_room
                && !casemate_furnishing
                && !artillery_scupper_subtraction
                && !roof_gutter_outlet_subtraction
                && !(void.role == VoidRole::DryDitch
                    && matches!(
                        solid.role,
                        SolidRole::DitchScarp
                            | SolidRole::DitchCounterscarp
                            | SolidRole::DitchFloor
                    ))
                && !(void.role == VoidRole::Passage && solid.role == SolidRole::OpeningClosure)
                && !exact_opening_piece
                && (void.role != VoidRole::WallOpening
                    || matches!(solid.role, SolidRole::WallHost | SolidRole::OpeningSill)
                    || solid.role == SolidRole::OpeningSpandrel
                    || (solid.role == SolidRole::OpeningJamb
                        && !matches!(solid.shape, crate::ResolvedSolidShape::SplayedReveal { .. }))
                    || (solid.role == SolidRole::OpeningHead
                        && matches!(solid.shape, crate::ResolvedSolidShape::Cuboid)))
                && (void.role != VoidRole::RoofOpening
                    || !matches!(
                        solid.role,
                        SolidRole::RoofFlashing | SolidRole::RoofFraming | SolidRole::WallHost
                    ))
                && resolved_shape_overlaps_bounds(solid, void_bounds, 0.001)
        });
        if void.owner != void.subtracts_from
            || !owners.contains(&void.subtracts_from)
            || blocking_solid.is_some()
        {
            issues.push(issue(
                "unresolved_void_subtraction",
                format!(
                    "resolved void {index} {:?} {:?}..{:?} is not an open subtraction from its owner; blocker={:?}",
                    void.role,
                    void.bounds.min,
                    void.bounds.max,
                    blocking_solid.map(|solid| (solid.id, solid.role, solid.centre, solid.size))
                ),
            ));
        }
    }
    for route in &plan.resolved_geometry.drainage_routes {
        let outward_drop = route.inlet.y - route.outlet.y;
        if outward_drop < 0.04
            || !plan
                .resolved_geometry
                .voids
                .iter()
                .any(|void| void.owner == route.owner && void.id == route.outlet_void)
        {
            issues.push(issue(
                "broken_crown_drainage",
                format!(
                    "drainage route {} does not reach a lower owned outlet",
                    route.id.0
                ),
            ));
        }
    }
}

fn audit_crowns(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if plan.crowns.is_empty() {
        return;
    }
    let plan_centre = plan.dimensions_metres() * 0.5;
    let crown_owners = plan
        .crowns
        .iter()
        .map(|crown| crown.owner)
        .collect::<std::collections::HashSet<_>>();
    if crown_owners.len() != plan.crowns.len() {
        issues.push(issue(
            "duplicate_geometry_owner",
            "crown assemblies do not have unique ownership IDs".to_owned(),
        ));
    }
    for crown in &plan.crowns {
        let p = crown.profile;
        let merlon_top =
            p.breastwork_height_metres + p.merlon_height_metres + p.coping_height_metres;
        if !(0.8..=1.0).contains(&p.breastwork_height_metres)
            || !(1.5..=1.8).contains(&merlon_top)
            || p.thickness_metres < 0.35
            || !(0.35..=0.6).contains(&p.crenel_width_metres)
            || p.walk_clear_width_metres < 0.9
            || p.inner_guard_height_metres < 0.9
            || p.firing_height_metres <= p.breastwork_height_metres
            || p.firing_height_metres >= merlon_top
        {
            issues.push(issue(
                "unsafe_crown_profile",
                format!(
                    "crown owner {} violates the declared cover/clearance envelope",
                    crown.owner.0
                ),
            ));
        }
        let solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == crown.owner)
            .collect::<Vec<_>>();
        for role in [
            SolidRole::Breastwork,
            SolidRole::Merlon,
            SolidRole::Coping,
            SolidRole::EdgeGuard,
        ] {
            if !solids.iter().any(|solid| solid.role == role) {
                issues.push(issue(
                    "incomplete_crown_geometry",
                    format!("crown owner {} lacks resolved {role:?}", crown.owner.0),
                ));
            }
        }
        if solids.iter().any(|solid| {
            let transverse = match crown.path {
                CrownPath::Straight { start, end, .. }
                    if (end - start).x.abs() >= (end - start).y.abs() =>
                {
                    solid.size.z
                }
                CrownPath::Straight { .. } => solid.size.x,
                CrownPath::Round { .. } => solid.size.z,
            };
            solid.role == SolidRole::Coping
                && (solid.crossfall_radians.abs() < 0.02 || transverse < p.thickness_metres + 0.02)
        }) {
            issues.push(issue(
                "bad_crown_coping",
                format!(
                    "crown owner {} lacks sloped overhanging drip coping",
                    crown.owner.0
                ),
            ));
        }
        let has_stance = plan
            .resolved_geometry
            .surfaces
            .iter()
            .any(|surface| surface.owner == crown.owner && surface.role == SurfaceRole::Stance);
        let has_firing =
            plan.resolved_geometry.surfaces.iter().any(|surface| {
                surface.owner == crown.owner && surface.role == SurfaceRole::FiringLine
            });
        let drains = plan
            .resolved_geometry
            .voids
            .iter()
            .filter(|void| void.owner == crown.owner && void.role == VoidRole::Drain)
            .count();
        if !has_stance || !has_firing || drains == 0 || crown.drain_positions.is_empty() {
            issues.push(issue(
                "incomplete_crown_geometry",
                format!(
                    "crown owner {} lacks stance, firing, coping, or drainage evidence",
                    crown.owner.0
                ),
            ));
        }
        let routes = plan
            .resolved_geometry
            .drainage_routes
            .iter()
            .filter(|route| route.owner == crown.owner)
            .collect::<Vec<_>>();
        let all_routes_outward = routes.iter().all(|route| {
            let delta = Vec2::new(
                route.outlet.x - route.inlet.x,
                route.outlet.z - route.inlet.z,
            );
            match crown.path {
                CrownPath::Straight { outward, .. } => delta.dot(direction_vector(outward)) >= 0.5,
                CrownPath::Round { centre, .. } => {
                    let radial =
                        (Vec2::new(route.outlet.x, route.outlet.z) - centre).normalize_or_zero();
                    delta.dot(radial) >= 0.5
                }
            }
        });
        if routes.len() != drains || !all_routes_outward {
            issues.push(issue(
                "broken_crown_drainage",
                format!(
                    "crown owner {} lacks a crossfall route to every scupper",
                    crown.owner.0
                ),
            ));
        }
        let catchments = plan
            .resolved_geometry
            .drainage_catchments
            .iter()
            .filter(|catchment| catchment.owner == crown.owner)
            .collect::<Vec<_>>();
        let catchment_contains = |catchment: &crate::DrainageCatchment, point: Vec2| {
            let relative = point - Vec2::new(catchment.centre.x, catchment.centre.z);
            relative.dot(catchment.tangent).abs() <= catchment.length_metres * 0.5 + 0.025
                && relative.dot(catchment.outward).abs() <= catchment.width_metres * 0.5 + 0.025
        };
        let catchments_valid = !catchments.is_empty()
            && catchments.iter().all(|catchment| {
                let solid = plan
                    .resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == catchment.walk_solid);
                let channels = catchment
                    .toe_channel_solids
                    .iter()
                    .filter_map(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == *id)
                    })
                    .collect::<Vec<_>>();
                let surface = plan
                    .resolved_geometry
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == catchment.drainage_surface);
                let route = routes
                    .iter()
                    .find(|route| route.id == catchment.outlet_route);
                let frames_are_canonical = (catchment.tangent.length() - 1.0).abs() < 0.01
                    && (catchment.outward.length() - 1.0).abs() < 0.01
                    && catchment.tangent.dot(catchment.outward).abs() < 0.01;
                let positive_drop =
                    catchment.inner_elevation_metres - catchment.outer_elevation_metres >= 0.04;
                let solid_slopes_outward = solid.is_some_and(|solid| {
                    let slab_width = catchment.width_metres - CROWN_DRAIN_CHANNEL_WIDTH_METRES;
                    let local_z = Vec2::new(solid.yaw_radians.sin(), solid.yaw_radians.cos());
                    let downhill = local_z * solid.crossfall_radians.signum();
                    let expected_slope = ((catchment.inner_elevation_metres
                        - catchment.outer_elevation_metres)
                        / slab_width)
                        .atan();
                    let expected_centre = Vec2::new(catchment.centre.x, catchment.centre.z)
                        - catchment.outward * (CROWN_DRAIN_CHANNEL_WIDTH_METRES * 0.5);
                    solid.role == SolidRole::WalkSurface
                        && solid.owner == crown.owner
                        && solid.crossfall_radians.abs() >= 0.01
                        && (solid.crossfall_radians.abs() - expected_slope).abs() < 0.002
                        && downhill.dot(catchment.outward) >= 0.98
                        && (solid.size.x - catchment.length_metres).abs() < 0.01
                        && (solid.size.z - slab_width).abs() < 0.01
                        && Vec2::new(solid.centre.x, solid.centre.z).distance(expected_centre)
                            < 0.01
                });
                let surface_is_drainage = surface.is_some_and(|surface| {
                    surface.owner == crown.owner && surface.role == SurfaceRole::Drainage
                });
                let channel_reaches_inlet = if channels.len() == catchment.toe_channel_solids.len()
                    && !channels.is_empty()
                {
                    let Some(route) = route else {
                        return false;
                    };
                    let inlet = Vec2::new(route.inlet.x, route.inlet.z);
                    let channel_segments = channels
                        .iter()
                        .map(|channel| {
                            let local_x =
                                Vec2::new(channel.yaw_radians.cos(), -channel.yaw_radians.sin());
                            let downhill = local_x * -channel.longfall_radians.signum();
                            let centre = Vec2::new(channel.centre.x, channel.centre.z);
                            (
                                centre - downhill * channel.size.x * 0.5,
                                centre + downhill * channel.size.x * 0.5,
                                channel.centre.y
                                    + channel.size.y * 0.5
                                    + channel.longfall_radians.tan().abs() * channel.size.x * 0.5,
                                channel.centre.y + channel.size.y * 0.5
                                    - channel.longfall_radians.tan().abs() * channel.size.x * 0.5,
                                *channel,
                            )
                        })
                        .collect::<Vec<_>>();
                    let distance_to_channels = |point: Vec2| {
                        channel_segments
                            .iter()
                            .map(|(start, end, _, _, _)| {
                                let delta = *end - *start;
                                let progress = if delta.length_squared() < 0.0001 {
                                    0.0
                                } else {
                                    ((point - *start).dot(delta) / delta.length_squared())
                                        .clamp(0.0, 1.0)
                                };
                                point.distance(*start + delta * progress)
                            })
                            .min_by(f32::total_cmp)
                            .unwrap_or(f32::INFINITY)
                    };
                    let toe_centre = Vec2::new(catchment.centre.x, catchment.centre.z)
                        + catchment.outward
                            * (catchment.width_metres * 0.5
                                - CROWN_DRAIN_CHANNEL_WIDTH_METRES * 0.5);
                    let all_toe_samples_reach_channel = (0..=4).all(|sample| {
                        let along = -catchment.length_metres * 0.5
                            + catchment.length_metres * sample as f32 / 4.0;
                        distance_to_channels(toe_centre + catchment.tangent * along) <= 0.13
                    });
                    let chain_is_continuous = channel_segments.windows(2).all(|pair| {
                        pair[0].1.distance(pair[1].0) <= 0.035
                            && (pair[0].3 - pair[1].2).abs() <= 0.006
                    });
                    let channel_unblocked =
                        channel_segments.iter().all(|(start, end, _, _, channel)| {
                            !(0..=4).any(|sample| {
                                let point = start.lerp(*end, sample as f32 / 4.0);
                                let point = Vec3::new(point.x, channel.centre.y, point.y);
                                let blocker = plan.resolved_geometry.solids.iter().find(|solid| {
                                    solid.owner == crown.owner
                                        && solid.id != catchment.walk_solid
                                        && !catchment.toe_channel_solids.contains(&solid.id)
                                        && !matches!(
                                            solid.role,
                                            SolidRole::WalkSurface | SolidRole::DrainageChannel
                                        )
                                        && resolved_solid_contains_point(solid, point, 0.0)
                                });
                                blocker.is_some()
                            })
                        });
                    let channel_is_recessed = solid.is_some_and(|walk| {
                        channel_segments.iter().all(|(_, _, high_top, _, channel)| {
                            *high_top <= catchment.outer_elevation_metres + 0.001
                                && !resolved_solids_overlap_positive_volume(walk, channel, 0.008)
                        })
                    });
                    let roles_and_fall = channel_segments.iter().all(|(_, _, _, _, channel)| {
                        channel.role == SolidRole::DrainageChannel
                            && channel.owner == crown.owner
                            && channel.longfall_radians < -0.0005
                    });
                    let endpoint_matches =
                        channel_segments
                            .last()
                            .is_some_and(|(_, end, _, low_top, _)| {
                                end.distance(inlet) <= 0.035
                                    && (*low_top - route.inlet.y).abs() <= 0.006
                            });
                    roles_and_fall
                        && chain_is_continuous
                        && endpoint_matches
                        && all_toe_samples_reach_channel
                        && channel_unblocked
                        && channel_is_recessed
                } else {
                    false
                };
                let route_reaches_open_scupper = route.is_some_and(|route| {
                    plan.resolved_geometry.voids.iter().any(|void| {
                        void.id == route.outlet_void
                            && void.owner == crown.owner
                            && void.role == VoidRole::Drain
                            && !plan.resolved_geometry.solids.iter().any(|solid| {
                                solid.owner == crown.owner
                                    && resolved_solid_overlaps_bounds(
                                        solid,
                                        (void.bounds.min, void.bounds.max),
                                        0.001,
                                    )
                            })
                    })
                });
                frames_are_canonical
                    && positive_drop
                    && catchment.width_metres >= 0.9
                    && catchment.length_metres > 0.05
                    && solid_slopes_outward
                    && channel_reaches_inlet
                    && surface_is_drainage
                    && route_reaches_open_scupper
            })
            && routes.iter().all(|route| {
                catchments
                    .iter()
                    .any(|catchment| catchment.outlet_route == route.id)
            });
        let catchment_coverage = match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let tangent = (end - start).normalize_or_zero();
                let outward = direction_vector(outward);
                let length = (end - start).length();
                (0..=(length / 0.1).ceil() as usize).all(|index| {
                    let along = (index as f32 * 0.1).min(length);
                    let in_tower_splice = crown.junctions.iter().any(|junction| {
                        let Some(radius) = plan.crowns.iter().find_map(|other| {
                            (other.owner == junction.other_owner)
                                .then_some(other.path)
                                .and_then(|path| match path {
                                    CrownPath::Round { radius_metres, .. } => Some(radius_metres),
                                    CrownPath::Straight { .. } => None,
                                })
                        }) else {
                            return false;
                        };
                        let splice = (junction.position - start).dot(tangent);
                        (along - splice).abs()
                            < radius + crown.profile.thickness_metres * 0.5 - 0.08
                    });
                    let delegated_corner = crown.junctions.iter().any(|junction| {
                        if junction.kind != CrownJunctionKind::Corner
                            || crown.owner <= junction.other_owner
                        {
                            return false;
                        }
                        ((junction.position - start).length() < 0.02
                            && along
                                <= crown.profile.walk_clear_width_metres
                                    + crown.profile.thickness_metres
                                    + 0.02)
                            || ((junction.position - end).length() < 0.02
                                && length - along
                                    <= crown.profile.walk_clear_width_metres
                                        + crown.profile.thickness_metres
                                        + 0.02)
                    });
                    in_tower_splice
                        || delegated_corner
                        || [
                            0.03,
                            crown.profile.walk_clear_width_metres * 0.5,
                            crown.profile.walk_clear_width_metres - 0.03,
                        ]
                        .into_iter()
                        .all(|inward| {
                            let point = start + tangent * along
                                - outward * (crown.profile.thickness_metres * 0.5 + inward);
                            catchments
                                .iter()
                                .any(|catchment| catchment_contains(catchment, point))
                        })
                })
            }
            CrownPath::Round {
                centre,
                radius_metres,
                ..
            } => {
                let deck_radius = radius_metres
                    - crown.profile.thickness_metres * 0.5
                    - crown.profile.walk_clear_width_metres * 0.5
                    - 0.03;
                let half_width = crown.profile.walk_clear_width_metres * 0.5;
                (0..144).all(|index| {
                    let angle = index as f32 * std::f32::consts::TAU / 144.0;
                    [
                        deck_radius - half_width + 0.03,
                        deck_radius,
                        deck_radius + half_width - 0.03,
                    ]
                    .into_iter()
                    .all(|radius| {
                        let point = centre + Vec2::new(angle.cos(), angle.sin()) * radius;
                        catchments
                            .iter()
                            .any(|catchment| catchment_contains(catchment, point))
                    })
                })
            }
        };
        if !catchments_valid || !catchment_coverage {
            issues.push(issue(
                "broken_crown_drainage",
                format!(
                    "crown owner {} lacks a continuous outward-sloped walk catchment to open scuppers (catchments_valid={catchments_valid}, coverage={catchment_coverage})",
                    crown.owner.0,
                ),
            ));
        }
        let defender_samples = plan
            .resolved_geometry
            .defender_samples
            .iter()
            .filter(|sample| sample.owner == crown.owner)
            .collect::<Vec<_>>();
        let required_samples = if matches!(crown.path, CrownPath::Round { .. }) {
            8
        } else {
            3
        };
        if defender_samples.len() < required_samples
            || defender_samples.iter().any(|sample| {
                let short_eye = sample.eye.y - sample.stance.y < 1.5;
                let uphill = sample.target.y > sample.eye.y;
                let off_stance = !plan.resolved_geometry.surfaces.iter().any(|surface| {
                    surface.owner == crown.owner
                        && surface.role == SurfaceRole::Stance
                        && sample
                            .stance
                            .cmpge(surface.bounds.min - Vec3::splat(0.02))
                            .all()
                        && sample
                            .stance
                            .cmple(surface.bounds.max + Vec3::splat(0.02))
                            .all()
                });
                let blocked = solids.iter().any(|solid| {
                    solid.role == SolidRole::Merlon && {
                        let line = Vec2::new(sample.target.x, sample.target.z)
                            - Vec2::new(sample.stance.x, sample.stance.z);
                        let firing_plane_offset = 0.55 + p.thickness_metres * 0.5;
                        let wall_point = Vec3::new(
                            sample.stance.x + line.normalize_or_zero().x * firing_plane_offset,
                            crown.base_height_metres + p.firing_height_metres,
                            sample.stance.z + line.normalize_or_zero().y * firing_plane_offset,
                        );
                        resolved_solid_contains_point(solid, wall_point, 0.005)
                    }
                });
                short_eye || uphill || off_stance || blocked
            })
        {
            issues.push(issue(
                "unusable_crown_firing_position",
                format!(
                    "crown owner {} lacks sampled stance/crenel firing usability",
                    crown.owner.0
                ),
            ));
        }
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let midpoint = (start + end) * 0.5;
                if (midpoint - plan_centre).dot(direction_vector(outward)) <= 0.01 {
                    issues.push(issue(
                        "crown_faces_inward",
                        format!("crown owner {} has an inward normal", crown.owner.0),
                    ));
                }
                let length = (end - start).length();
                let nominal = p.merlon_width_metres + p.crenel_width_metres;
                let crenels = (((length - 0.5) / nominal).floor() as usize).max(1);
                let end_merlon =
                    (length - p.crenel_width_metres * crenels as f32) / (crenels + 1) as f32;
                if end_merlon < 0.25 {
                    issues.push(issue(
                        "crown_end_fragment",
                        format!(
                            "crown owner {} leaves a sub-0.25m end fragment",
                            crown.owner.0
                        ),
                    ));
                }
                for endpoint in [start, end] {
                    let count = crown
                        .junctions
                        .iter()
                        .filter(|junction| (junction.position - endpoint).length() < 0.02)
                        .count();
                    if count != 1 {
                        issues.push(issue(
                            "unowned_crown_junction",
                            format!(
                                "crown owner {} endpoint has {count} junction owners",
                                crown.owner.0
                            ),
                        ));
                    }
                }
                let matching_walk = plan.wall_walks.iter().find(|walk| matches!(walk, WallWalk::Linear { start: a, end: b, width_metres, .. } if ((*a-start).length()<0.02 && (*b-end).length()<0.02) && *width_metres >= p.walk_clear_width_metres + 0.1));
                if matching_walk.is_none() {
                    issues.push(issue(
                        "blocked_crown_walk",
                        format!(
                            "crown owner {} has no clear matching wall walk",
                            crown.owner.0
                        ),
                    ));
                }
                let tangent = (end - start).normalize_or_zero();
                let normal = direction_vector(outward);
                let length = (end - start).length();
                for step in 1..(length / 0.2).floor() as usize {
                    let distance = step as f32 * 0.2;
                    let in_tower_splice = crown.junctions.iter().any(|junction| {
                        if junction.kind != CrownJunctionKind::TowerSplice {
                            return false;
                        }
                        let Some(radius) = plan
                            .crowns
                            .iter()
                            .find_map(|other| {
                                (other.owner == junction.other_owner).then_some(other.path)
                            })
                            .and_then(|path| match path {
                                CrownPath::Round { radius_metres, .. } => Some(radius_metres),
                                CrownPath::Straight { .. } => None,
                            })
                        else {
                            return false;
                        };
                        let centre = (junction.position - start).dot(tangent);
                        (distance - centre).abs() < radius + p.thickness_metres * 0.5 - 0.1
                    });
                    if in_tower_splice {
                        continue;
                    }
                    let line = start + tangent * distance;
                    let upper = Vec3::new(
                        line.x + normal.x * p.thickness_metres * 0.5,
                        crown.base_height_metres + p.breastwork_height_metres * 0.5,
                        line.y + normal.y * p.thickness_metres * 0.5,
                    );
                    let covered = solids.iter().any(|solid| {
                        solid.role == SolidRole::Breastwork && {
                            let (min, max) = resolved_solid_bounds(solid);
                            upper.cmpge(min - Vec3::splat(0.01)).all()
                                && upper.cmple(max + Vec3::splat(0.01)).all()
                        }
                    });
                    if !covered {
                        issues.push(issue(
                            "crown_interval_gap",
                            format!(
                                "straight crown owner {} has an uncovered middle interval",
                                crown.owner.0
                            ),
                        ));
                        break;
                    }
                }
                for junction in crown
                    .junctions
                    .iter()
                    .filter(|junction| junction.kind == CrownJunctionKind::TowerSplice)
                {
                    let Some(radius) = plan
                        .crowns
                        .iter()
                        .find_map(|other| {
                            (other.owner == junction.other_owner).then_some(other.path)
                        })
                        .and_then(|path| match path {
                            CrownPath::Round { radius_metres, .. } => Some(radius_metres),
                            CrownPath::Straight { .. } => None,
                        })
                    else {
                        continue;
                    };
                    let splice_centre = (junction.position - start).dot(tangent);
                    let half_clear = radius + p.thickness_metres * 0.5 - 0.08;
                    let splice_min = splice_centre - half_clear;
                    let splice_max = splice_centre + half_clear;
                    let penetrates = solids.iter().any(|solid| {
                        if matches!(
                            solid.role,
                            SolidRole::WalkSurface | SolidRole::DrainageChannel
                        ) {
                            return false;
                        }
                        let centre =
                            (Vec2::new(solid.centre.x, solid.centre.z) - start).dot(tangent);
                        let half = if tangent.x.abs() >= tangent.y.abs() {
                            solid.size.x * 0.5
                        } else {
                            solid.size.z * 0.5
                        };
                        centre + half > splice_min + 0.02 && centre - half < splice_max - 0.02
                    });
                    if penetrates {
                        issues.push(issue(
                            "unresolved_tower_crown_splice",
                            format!(
                                "straight crown owner {} penetrates tower owner {} instead of yielding the splice",
                                crown.owner.0, junction.other_owner.0
                            ),
                        ));
                    }
                }
            }
            CrownPath::Round {
                tower_index,
                centre,
                radius_metres,
            } => {
                if plan.towers.get(tower_index).is_none_or(|tower| {
                    (tower.centre_metres() - centre).length() > 0.02
                        || (tower.radius_metres() - radius_metres).abs() > 0.02
                }) {
                    issues.push(issue(
                        "bad_tower_crown_splice",
                        format!(
                            "round crown owner {} does not match its tower",
                            crown.owner.0
                        ),
                    ));
                }
                let mut merlons = solids
                    .iter()
                    .filter(|solid| solid.role == SolidRole::Merlon)
                    .map(|solid| {
                        let radial = Vec2::new(solid.centre.x, solid.centre.z) - centre;
                        (
                            radial.y.atan2(radial.x).rem_euclid(std::f32::consts::TAU),
                            solid.size.x,
                        )
                    })
                    .collect::<Vec<_>>();
                let mut route_angles = plan
                    .tower_portals
                    .iter()
                    .filter(|portal| {
                        portal.tower_index == tower_index
                            && matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. })
                    })
                    .map(|portal| {
                        let facing = direction_vector(portal.facing);
                        facing.y.atan2(facing.x)
                    })
                    .collect::<Vec<_>>();
                for junction in crown
                    .junctions
                    .iter()
                    .filter(|junction| junction.kind == CrownJunctionKind::TowerSplice)
                {
                    if let Some(CrownPath::Straight { start, end, .. }) = plan
                        .crowns
                        .iter()
                        .find(|other| other.owner == junction.other_owner)
                        .map(|other| other.path)
                    {
                        for point in [start, end] {
                            let direction = point - centre;
                            if direction.length() > radius_metres + 0.1 {
                                route_angles.push(direction.y.atan2(direction.x));
                            }
                        }
                    }
                }
                merlons.sort_by(|a, b| a.0.total_cmp(&b.0));
                for index in 0..merlons.len() {
                    let (angle, width) = merlons[index];
                    let (mut next_angle, next_width) = merlons[(index + 1) % merlons.len()];
                    if index + 1 == merlons.len() {
                        next_angle += std::f32::consts::TAU;
                    }
                    let gap = (next_angle - angle) * radius_metres - (width + next_width) * 0.5;
                    let midpoint = (angle + next_angle) * 0.5;
                    let at_portal = route_angles.iter().any(|portal_angle| {
                        ((midpoint - *portal_angle + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI)
                            .abs()
                            < 0.65
                    });
                    if !at_portal && !(0.35..=0.60).contains(&gap) {
                        issues.push(issue(
                            "invalid_round_crenel_interval",
                            format!("tower crown owner {} has a {gap:.2}m crenel", crown.owner.0),
                        ));
                        break;
                    }
                }
                let segment_angle = std::f32::consts::TAU / 24.0;
                for angle in route_angles {
                    let open_segments = (-3..=3)
                        .filter(|offset| {
                            let sample_angle = angle + *offset as f32 * segment_angle;
                            let radial = Vec2::new(sample_angle.cos(), sample_angle.sin());
                            let point = Vec3::new(
                                centre.x + radial.x * (radius_metres + p.thickness_metres * 0.5),
                                crown.base_height_metres + p.breastwork_height_metres * 0.5,
                                centre.y + radial.y * (radius_metres + p.thickness_metres * 0.5),
                            );
                            !solids.iter().any(|solid| {
                                solid.role == SolidRole::Breastwork && {
                                    resolved_solid_contains_point(solid, point, 0.005)
                                }
                            })
                        })
                        .count();
                    if open_segments as f32 * segment_angle * radius_metres < 0.9 {
                        issues.push(issue(
                            "blocked_round_crown_portal",
                            format!(
                                "tower crown owner {} does not yield a 0.90m portal sector",
                                crown.owner.0
                            ),
                        ));
                    }
                }
                let Some(WallWalk::Round { stairwell_radius_metres, .. }) = plan.wall_walks.iter().find(|walk| matches!(walk, WallWalk::Round { centre: walk_centre, .. } if (*walk_centre-centre).length()<0.02)) else { continue; };
                let Some(arrival) = plan.stairs.iter().find_map(|stair| match *stair {
                    Stair::Spiral {
                        centre: stair_centre,
                        turns,
                        clockwise,
                        tread_count,
                        ..
                    } if (stair_centre - centre).length() < 0.02 => {
                        let progress = f32::from(tread_count.saturating_sub(1))
                            / f32::from(tread_count.max(1));
                        Some(
                            if clockwise { -1.0 } else { 1.0 }
                                * progress
                                * turns
                                * std::f32::consts::TAU,
                        )
                    }
                    _ => None,
                }) else {
                    continue;
                };
                let gap_segments = (-5..=5)
                    .filter(|offset| {
                        let angle = arrival + *offset as f32 * segment_angle;
                        let radial = Vec2::new(angle.cos(), angle.sin());
                        let radius = *stairwell_radius_metres + 0.08;
                        let point = Vec3::new(
                            centre.x + radial.x * radius,
                            crown.base_height_metres + p.inner_guard_height_metres * 0.5,
                            centre.y + radial.y * radius,
                        );
                        !solids.iter().any(|solid| {
                            solid.role == SolidRole::EdgeGuard && {
                                let (min, max) = resolved_solid_bounds(solid);
                                point.cmpge(min).all() && point.cmple(max).all()
                            }
                        })
                    })
                    .count();
                if gap_segments as f32 * segment_angle * (*stairwell_radius_metres + 0.08) < 0.9 {
                    issues.push(issue(
                        "blocked_spiral_arrival",
                        format!(
                            "tower crown owner {} guards across its spiral landing",
                            crown.owner.0
                        ),
                    ));
                }
            }
        }
        for junction in &crown.junctions {
            if !plan.resolved_geometry.junction_bonds.iter().any(|bond| {
                bond.owners.contains(&crown.owner)
                    && bond.owners.contains(&junction.other_owner)
                    && bond.minimum_interface_area_square_metres >= 0.08
                    && bond.maximum_penetration_metres <= 0.18
            }) {
                issues.push(issue(
                    "missing_crown_junction_bond",
                    format!(
                        "crown owners {} and {} have no positive local bond",
                        crown.owner.0, junction.other_owner.0
                    ),
                ));
            }
            let reciprocal = plan.crowns.iter().any(|other| {
                other.owner == junction.other_owner
                    && other.junctions.iter().any(|back| {
                        back.other_owner == crown.owner
                            && (back.position - junction.position).length() < 0.02
                    })
            });
            if junction.owner != crown.owner
                || !crown_owners.contains(&junction.other_owner)
                || junction.clear_width_metres < 0.9
                || !reciprocal
            {
                issues.push(issue(
                    "bad_crown_junction",
                    format!(
                        "crown owner {} has an invalid corner/tower splice",
                        crown.owner.0
                    ),
                ));
            }
            if junction.kind == CrownJunctionKind::TowerSplice
                && !plan.crowns.iter().any(|other| {
                    other.owner == junction.other_owner
                        && matches!(other.path, CrownPath::Round { .. })
                })
                && !matches!(crown.path, CrownPath::Round { .. })
            {
                issues.push(issue(
                    "bad_tower_crown_splice",
                    format!(
                        "crown owner {} labels a non-tower splice as tower-owned",
                        crown.owner.0
                    ),
                ));
            }
            if junction.kind == CrownJunctionKind::Corner && crown.owner.0 < junction.other_owner.0
            {
                let corner_merlons = plan
                    .resolved_geometry
                    .solids
                    .iter()
                    .filter(|solid| {
                        solid.role == SolidRole::Merlon
                            && Vec2::new(solid.centre.x, solid.centre.z).distance(junction.position)
                                < 0.08
                    })
                    .count();
                if corner_merlons != 1 {
                    issues.push(issue(
                        "duplicate_junction_merlon",
                        format!(
                            "corner at {:?} has {corner_merlons} owned merlons",
                            junction.position
                        ),
                    ));
                }
            }
        }
    }
}

/// Checks the resolved geometry cache against the grid-native gatehouse source.
///
/// The tolerances below are project construction gates, not historical claims.
fn audit_projected_defenses(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for defense in &plan.projected_defenses {
        let solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == defense.owner)
            .collect::<Vec<_>>();
        let voids = plan
            .resolved_geometry
            .voids
            .iter()
            .filter(|void| void.owner == defense.owner)
            .collect::<Vec<_>>();
        let expected_material_phase = match defense.kind {
            ProjectedDefenseKind::Hoarding => {
                defense.material == ProjectedDefenseMaterial::Timber
                    && defense.phase == ProjectedDefensePhase::TemporaryCampaignWork
                    && matches!(
                        defense.deployment,
                        ProjectedDefenseDeployment::SocketsOnly
                            | ProjectedDefenseDeployment::Deployed
                    )
            }
            _ => {
                defense.material == ProjectedDefenseMaterial::Masonry
                    && defense.phase == ProjectedDefensePhase::PermanentMainWork
                    && defense.deployment == ProjectedDefenseDeployment::Permanent
            }
        };
        if !expected_material_phase {
            issues.push(issue(
                "projected_defense_phase_material_mismatch",
                format!(
                    "projected defense owner {} has an incoherent material, phase, or deployment",
                    defense.owner.0
                ),
            ));
        }
        let target_matches_installation = match defense.kind {
            ProjectedDefenseKind::Machicolation => {
                defense.tactical_target == ProjectedDefenseTarget::GateApproach
            }
            ProjectedDefenseKind::Breteche => {
                defense.tactical_target == ProjectedDefenseTarget::ThreatenedWallFoot
            }
            ProjectedDefenseKind::Hoarding => {
                defense.tactical_target == ProjectedDefenseTarget::CampaignSiegeFront
            }
            ProjectedDefenseKind::Bartizan => {
                defense.tactical_target == ProjectedDefenseTarget::ThreatenedCorner
            }
        };
        if !target_matches_installation {
            issues.push(issue(
                "projected_defense_tactical_target_mismatch",
                format!(
                    "projected defense owner {} lacks a coherent named tactical target",
                    defense.owner.0
                ),
            ));
        }
        let outward = match defense.path {
            ProjectedDefensePath::Linear { outward, .. }
            | ProjectedDefensePath::Round { outward, .. } => direction_vector(outward),
        };
        let host_solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == defense.host_owner)
            .collect::<Vec<_>>();
        let host_is_authoritative = defense.host_owner != defense.owner
            && !defense.host_wall_solids.is_empty()
            && defense.host_wall_solids.iter().all(|id| {
                host_solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::DefenseHostWall)
            })
            && host_solids.iter().any(|solid| {
                solid.id == defense.host_walk_solid && solid.role == SolidRole::CircuitWalk
            })
            && host_solids
                .iter()
                .filter(|solid| solid.role == SolidRole::DefenseHostWall)
                .all(|solid| defense.host_wall_solids.contains(&solid.id));
        let host_portal_is_cut = defense.host_portal_void.is_none_or(|id| {
            plan.resolved_geometry.voids.iter().any(|void| {
                void.id == id
                    && void.owner == defense.host_owner
                    && void.subtracts_from == defense.host_owner
                    && void.role == VoidRole::AccessPortal
            })
        });
        let host_bond_is_physical = defense.host_bond.is_none_or(|id| {
            plan.resolved_geometry.junction_bonds.iter().any(|bond| {
                bond.id == id
                    && bond.owners.contains(&defense.owner)
                    && bond.owners.contains(&defense.host_owner)
            })
        });
        let source_walls_are_exact = !defense.host_source_walls.is_empty()
            && defense.host_source_walls.iter().all(|source| {
                let Some(storey) = plan
                    .storeys
                    .iter()
                    .find(|storey| storey.level == source.storey_level)
                else {
                    return false;
                };
                let Some(wall) = storey.walls.get(source.wall_index).copied() else {
                    return false;
                };
                let source_top = f32::from(storey.level + 1) * plan.storey_height_metres;
                let source_bottom = source_top - plan.storey_height_metres;
                if !wall.exterior() || (defense.host_top_elevation_metres - source_top).abs() > 0.01
                {
                    return false;
                }
                let centre = wall.centre();
                let along = if wall.is_horizontal() {
                    Vec2::X
                } else {
                    Vec2::Y
                };
                let source_contains_plan = |point: Vec2| {
                    (point - centre).dot(along).abs() <= crate::CELL_SIZE_METRES * 0.5 + 0.01
                        && (point - centre).dot(direction_vector(wall.direction)).abs() <= 0.1
                };
                let solids_inside = defense.host_wall_solids.iter().all(|id| {
                    host_solids
                        .iter()
                        .find(|solid| solid.id == *id)
                        .is_some_and(|solid| {
                            source_contains_plan(Vec2::new(solid.centre.x, solid.centre.z))
                                || defense.host_source_walls.iter().any(|other| {
                                    plan.storeys
                                        .iter()
                                        .find(|candidate| candidate.level == other.storey_level)
                                        .and_then(|candidate| candidate.walls.get(other.wall_index))
                                        .is_some_and(|other_wall| {
                                            let other_centre = other_wall.centre();
                                            let other_along = if other_wall.is_horizontal() {
                                                Vec2::X
                                            } else {
                                                Vec2::Y
                                            };
                                            (Vec2::new(solid.centre.x, solid.centre.z)
                                                - other_centre)
                                                .dot(other_along)
                                                .abs()
                                                <= crate::CELL_SIZE_METRES * 0.5 + 0.01
                                                && (Vec2::new(solid.centre.x, solid.centre.z)
                                                    - other_centre)
                                                    .dot(direction_vector(other_wall.direction))
                                                    .abs()
                                                    <= 0.1
                                        })
                                }) && solid.centre.y - solid.size.y * 0.5 >= source_bottom - 0.01
                                    && solid.centre.y + solid.size.y * 0.5 <= source_top + 0.01
                        })
                });
                let sampled_cover = [-0.4_f32, 0.0, 0.4].into_iter().all(|along_sample| {
                    [0.15_f32, 0.5, 0.85].into_iter().all(|height_sample| {
                        let plan_point = centre + along * crate::CELL_SIZE_METRES * along_sample;
                        let outside_replacement_run = match defense.path {
                            ProjectedDefensePath::Linear { start, end, .. } => {
                                let run = end - start;
                                let run_length = run.length();
                                let run_tangent = run.normalize_or_zero();
                                let offset = plan_point - start;
                                let projected = offset.dot(run_tangent);
                                projected < -0.01 || projected > run_length + 0.01
                            }
                            ProjectedDefensePath::Round {
                                centre,
                                radius_metres,
                                ..
                            } => (plan_point - centre).dot(along).abs() > radius_metres + 0.01,
                        };
                        if outside_replacement_run {
                            return true;
                        }
                        let point = Vec3::new(
                            plan_point.x,
                            source_bottom + plan.storey_height_metres * height_sample,
                            plan_point.y,
                        );
                        defense.host_wall_solids.iter().any(|id| {
                            host_solids
                                .iter()
                                .find(|solid| solid.id == *id)
                                .is_some_and(|solid| {
                                    resolved_solid_contains_point(solid, point, 0.012)
                                })
                        }) || plan.resolved_geometry.voids.iter().any(|void| {
                            void.owner == defense.host_owner
                                && point.x >= void.bounds.min.x - 0.01
                                && point.x <= void.bounds.max.x + 0.01
                                && point.y >= void.bounds.min.y - 0.01
                                && point.y <= void.bounds.max.y + 0.01
                                && point.z >= void.bounds.min.z - 0.01
                                && point.z <= void.bounds.max.z + 0.01
                        })
                    })
                });
                solids_inside && sampled_cover
            });
        let host_solids_do_not_duplicate =
            defense
                .host_wall_solids
                .iter()
                .enumerate()
                .all(|(index, left)| {
                    defense
                        .host_wall_solids
                        .iter()
                        .skip(index + 1)
                        .all(|right| {
                            host_solids
                                .iter()
                                .find(|solid| solid.id == *left)
                                .zip(host_solids.iter().find(|solid| solid.id == *right))
                                .is_some_and(|(left, right)| {
                                    !resolved_solids_overlap_positive_volume(left, right, 0.002)
                                })
                        })
                });
        let host_roof_clear = defense.host_wall_solids.iter().all(|host_id| {
            host_solids
                .iter()
                .find(|solid| solid.id == *host_id)
                .is_some_and(|host| {
                    plan.resolved_geometry
                        .solids
                        .iter()
                        .filter(|solid| {
                            solid.owner == defense.owner && solid.role == SolidRole::DefenseRoof
                        })
                        .all(|roof| !resolved_solids_overlap_positive_volume(host, roof, 0.002))
                })
        });
        let topology_is_supported = match defense.host_topology {
            crate::ProjectedDefenseHostTopology::LinearFace => {
                defense.kind != ProjectedDefenseKind::Bartizan
                    && defense.host_buttress_solids.is_empty()
            }
            crate::ProjectedDefenseHostTopology::CornerFaces => {
                defense.kind == ProjectedDefenseKind::Bartizan
                    && defense.host_source_walls.len() >= 2
            }
            crate::ProjectedDefenseHostTopology::Buttress => {
                defense.kind == ProjectedDefenseKind::Bartizan
                    && !defense.host_buttress_solids.is_empty()
                    && defense.host_buttress_solids.iter().all(|id| {
                        host_solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|solid| {
                                solid.role == SolidRole::DefenseHostButtress
                                    && solid.centre.y - solid.size.y * 0.5 <= 0.01
                                    && defense.host_wall_solids.iter().any(|wall_id| {
                                        host_solids
                                            .iter()
                                            .find(|wall| wall.id == *wall_id)
                                            .is_some_and(|wall| {
                                                resolved_solids_overlap_positive_volume(
                                                    solid, wall, -0.015,
                                                )
                                            })
                                    })
                            })
                    })
            }
        };
        if !host_is_authoritative
            || !host_portal_is_cut
            || !host_bond_is_physical
            || !source_walls_are_exact
            || !host_solids_do_not_duplicate
            || !host_roof_clear
            || !topology_is_supported
        {
            issues.push(issue(
                "unresolved_projected_defense_host",
                format!(
                    "projected defense owner {} is not bonded to a cut authoritative wall/walk host (authority={host_is_authoritative}, portal={host_portal_is_cut}, bond={host_bond_is_physical}, envelope={source_walls_are_exact}, disjoint={host_solids_do_not_duplicate}, roof_clear={host_roof_clear}, topology={topology_is_supported})",
                    defense.owner.0,
                ),
            ));
        }
        if defense.kind == ProjectedDefenseKind::Hoarding {
            let sockets_are_host_voids = !defense.beam_socket_voids.is_empty()
                && defense.beam_socket_voids.iter().all(|id| {
                    plan.resolved_geometry.voids.iter().any(|void| {
                        void.id == *id
                            && void.owner == defense.host_owner
                            && void.role == VoidRole::BeamSocket
                    })
                });
            let deployment_matches_sockets = match defense.deployment {
                ProjectedDefenseDeployment::SocketsOnly => defense.socket_joists.is_empty(),
                ProjectedDefenseDeployment::Deployed => {
                    defense.socket_joists.len() == defense.beam_socket_voids.len()
                        && defense.socket_joists.iter().all(|(socket_id, joist_id)| {
                            let socket = plan
                                .resolved_geometry
                                .voids
                                .iter()
                                .find(|void| void.id == *socket_id);
                            let joist = solids.iter().find(|solid| solid.id == *joist_id);
                            socket.zip(joist).is_some_and(|(socket, joist)| {
                                joist.role == SolidRole::BeamJoist
                                    && resolved_solid_overlaps_bounds(
                                        joist,
                                        (socket.bounds.min, socket.bounds.max),
                                        0.01,
                                    )
                            })
                        })
                }
                ProjectedDefenseDeployment::Permanent => false,
            };
            if !sockets_are_host_voids || !deployment_matches_sockets {
                issues.push(issue(
                    "invalid_hoarding_beam_sockets",
                    format!(
                        "hoarding owner {} does not use host-cut sockets occupied by state-linked joists",
                        defense.owner.0
                    ),
                ));
            }
        }
        let placement_faces_outward = match defense.path {
            ProjectedDefensePath::Linear { start, end, .. } => {
                let midpoint = (start + end) * 0.5;
                let floor_centroid = defense
                    .floor_solids
                    .iter()
                    .filter_map(|id| solids.iter().find(|solid| solid.id == *id))
                    .map(|solid| Vec2::new(solid.centre.x, solid.centre.z))
                    .reduce(|left, right| left + right)
                    .map(|sum| sum / defense.floor_solids.len().max(1) as f32);
                defense.deployment == ProjectedDefenseDeployment::SocketsOnly
                    || floor_centroid
                        .is_some_and(|centroid| (centroid - midpoint).dot(outward) > 0.08)
            }
            ProjectedDefensePath::Round { centre, .. } => {
                let plan_centre = plan.dimensions_metres() * 0.5;
                (centre - plan_centre).dot(outward) > 0.1
            }
        };
        if !placement_faces_outward {
            issues.push(issue(
                "inward_projected_defense",
                format!(
                    "projected defense owner {} is oriented away from its physical projection",
                    defense.owner.0
                ),
            ));
        }
        if defense.deployment == ProjectedDefenseDeployment::SocketsOnly {
            if !defense.floor_solids.is_empty()
                || !defense.throat_voids.is_empty()
                || defense.access_portal.is_some()
                || defense.access_landing.is_some()
                || defense.beam_socket_voids.is_empty()
                || !defense.socket_joists.is_empty()
                || defense.beam_socket_voids.iter().any(|id| {
                    !plan.resolved_geometry.voids.iter().any(|void| {
                        void.id == *id
                            && void.owner == defense.host_owner
                            && void.role == VoidRole::BeamSocket
                    })
                })
            {
                issues.push(issue(
                    "invalid_hoarding_deployment_state",
                    format!(
                        "socket-only hoarding owner {} contains deployed gallery work",
                        defense.owner.0
                    ),
                ));
            }
            continue;
        }
        if defense.clear_width_metres < 0.9
            || defense.clear_height_metres < 1.9
            || defense.breastwork_height_metres < 0.9
            || (defense.material == ProjectedDefenseMaterial::Timber
                && defense.projection_metres > 1.2)
        {
            issues.push(issue(
                "insufficient_projected_defense_clearance",
                format!(
                    "projected defense owner {} violates walk, headroom, cover, or cantilever gates",
                    defense.owner.0
                ),
            ));
        }
        let has_floor = !defense.floor_solids.is_empty()
            && defense.floor_solids.iter().all(|id| {
                solids.iter().any(|solid| {
                    solid.id == *id
                        && matches!(solid.role, SolidRole::GalleryFloor | SolidRole::Landing)
                })
            });
        let has_portal = defense.access_portal.is_some_and(|id| {
            plan.resolved_geometry.voids.iter().any(|void| {
                let size = void.bounds.max - void.bounds.min;
                void.id == id
                    && void.owner == defense.host_owner
                    && void.role == VoidRole::AccessPortal
                    && size.x.max(size.z) >= 0.75
                    && size.y >= 1.9
            })
        });
        let has_landing = defense.access_landing.is_some_and(|id| {
            solids
                .iter()
                .any(|solid| solid.id == id && solid.role == SolidRole::Landing)
        });
        let landing_overlaps_floor = defense.access_landing.is_some_and(|landing_id| {
            solids
                .iter()
                .find(|solid| solid.id == landing_id)
                .is_some_and(|landing| {
                    defense.floor_solids.iter().any(|floor_id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *floor_id)
                            .is_some_and(|floor| {
                                bounds_overlap_3d(
                                    resolved_solid_bounds(landing),
                                    resolved_solid_bounds(floor),
                                    0.01,
                                )
                            })
                    })
                })
        });
        let landing_overlaps_host_walk = defense.access_landing.is_some_and(|landing_id| {
            let landing = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == landing_id);
            let host_walk = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == defense.host_walk_solid);
            landing.zip(host_walk).is_some_and(|(landing, walk)| {
                resolved_solids_overlap_positive_volume(landing, walk, 0.004)
            })
        });
        if !has_floor
            || !has_portal
            || !has_landing
            || !landing_overlaps_floor
            || !landing_overlaps_host_walk
        {
            issues.push(issue(
                "inaccessible_projected_defense",
                format!(
                    "projected defense owner {} lacks a physical floor, portal, or landing",
                    defense.owner.0
                ),
            ));
        }
        let throats_valid = !defense.throat_voids.is_empty()
            && defense.throat_voids.iter().all(|id| {
                voids
                    .iter()
                    .any(|void| void.id == *id && void.role == VoidRole::DefenseThroat)
                    && plan
                        .resolved_geometry
                        .projected_defense_rays
                        .iter()
                        .any(|ray| ray.owner == defense.owner && ray.throat == *id)
            });
        if !throats_valid {
            issues.push(issue(
                "sealed_projected_defense_throat",
                format!(
                    "projected defense owner {} lacks open downward-defense throats and rays",
                    defense.owner.0
                ),
            ));
        }
        let working_points = plan
            .resolved_geometry
            .projected_defense_working_points
            .iter()
            .filter(|point| point.owner == defense.owner)
            .collect::<Vec<_>>();
        let working_points_valid = !working_points.is_empty()
            && working_points.iter().all(|point| {
                let support = solids.iter().find(|solid| solid.id == point.support_solid);
                let support_valid = support.is_some_and(|solid| {
                    matches!(solid.role, SolidRole::GalleryFloor | SolidRole::Landing)
                        && resolved_solid_contains_point(solid, point.stance - Vec3::Y * 0.02, 0.08)
                        && point.stance.y + 0.03 >= defense.floor_elevation_metres
                });
                let ranges = plan
                    .resolved_geometry
                    .projected_defense_rays
                    .iter()
                    .filter(|ray| ray.owner == defense.owner && ray.throat == point.aperture)
                    .map(|ray| ray.range)
                    .collect::<std::collections::HashSet<_>>();
                let aperture = plan
                    .resolved_geometry
                    .voids
                    .iter()
                    .find(|void| void.id == point.aperture);
                support_valid
                    && aperture.is_some()
                    && ranges
                        == std::collections::HashSet::from([
                            crate::ProjectedDefenseRange::Near,
                            crate::ProjectedDefenseRange::Middle,
                            crate::ProjectedDefenseRange::Far,
                        ])
            });
        if !working_points_valid {
            issues.push(issue(
                "inoperable_projected_defense_station",
                format!(
                    "projected defense owner {} lacks supported near/mid/far working positions",
                    defense.owner.0
                ),
            ));
        }
        for ray in plan
            .resolved_geometry
            .projected_defense_rays
            .iter()
            .filter(|ray| ray.owner == defense.owner)
        {
            let delta = ray.target - ray.origin;
            let outward_progress = Vec2::new(delta.x, delta.z).dot(outward);
            let is_downward_throat = plan
                .resolved_geometry
                .voids
                .iter()
                .any(|void| void.id == ray.throat && void.role == VoidRole::DefenseThroat);
            let aims_down_and_out = if is_downward_throat {
                delta.y < -1.0 && outward_progress > 0.5
            } else {
                delta.y < 0.0 && outward_progress > 0.5
            };
            let blocked = (1..20).any(|sample| {
                let point = ray.origin.lerp(ray.target, sample as f32 / 20.0);
                plan.resolved_geometry.solids.iter().any(|solid| {
                    !(matches!(
                        solid.shape,
                        crate::ResolvedSolidShape::RoundTowerShell { .. }
                    ) && segment_is_inside_tower_chord_void(plan, solid, ray.origin, ray.target))
                        && resolved_solid_contains_point(solid, point, -0.015)
                })
            });
            let below_floor_origin = ray.origin.y < defense.floor_elevation_metres - 0.001;
            let crosses_friendly_route = (1..20).any(|sample| {
                let point = ray.origin.lerp(ray.target, sample as f32 / 20.0);
                plan.resolved_geometry.solids.iter().any(|solid| {
                    matches!(solid.role, SolidRole::CircuitWalk | SolidRole::Landing)
                        && resolved_solid_contains_point(solid, point, -0.015)
                })
            });
            if !aims_down_and_out || blocked || below_floor_origin || crosses_friendly_route {
                issues.push(issue(
                    "blocked_projected_defense_ray",
                    format!(
                        "projected defense owner {} has a blocked, inward, or misaligned throat ray {:?}->{:?} blocked={blocked}",
                        defense.owner.0, ray.origin, ray.target
                    ),
                ));
                break;
            }
        }
        let support_nodes = defense
            .support_nodes
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .structural_nodes
                    .iter()
                    .find(|node| node.id == *id)
            })
            .collect::<Vec<_>>();
        let support_valid = support_nodes.len() == defense.support_nodes.len()
            && !support_nodes.is_empty()
            && support_nodes.iter().all(|node| {
                matches!(
                    node.kind,
                    crate::StructuralNodeKind::ProjectionCorbel
                        | crate::StructuralNodeKind::GalleryFrame
                ) && !node.supported_by.is_empty()
                    && plan
                        .resolved_geometry
                        .support_interfaces
                        .iter()
                        .any(|bearing| {
                            let size = bearing.bounds.max - bearing.bounds.min;
                            bearing.owner == defense.owner
                                && bearing.node == node.id
                                && size.x * size.z >= 0.08
                        })
            });
        let support_tangent = match defense.path {
            ProjectedDefensePath::Linear { start, end, .. } => (end - start).normalize_or_zero(),
            ProjectedDefensePath::Round { outward, .. } => {
                let radial = direction_vector(outward);
                Vec2::new(-radial.y, radial.x)
            }
        };
        let floor_supported_at_spacing = defense.floor_solids.iter().all(|floor_id| {
            solids
                .iter()
                .find(|solid| solid.id == *floor_id)
                .is_some_and(|floor| {
                    [-0.5_f32, 0.0, 0.5].into_iter().all(|sample| {
                        let local_x = Vec2::new(floor.yaw_radians.cos(), -floor.yaw_radians.sin());
                        let point = Vec2::new(floor.centre.x, floor.centre.z)
                            + local_x * floor.size.x * sample;
                        support_nodes.iter().any(|node| {
                            let support = Vec2::new(node.position.x, node.position.z);
                            (point - support).dot(support_tangent).abs() <= 0.75
                        })
                    })
                })
        });
        if !support_valid || !floor_supported_at_spacing {
            issues.push(issue(
                "unsupported_projected_defense",
                format!(
                    "projected defense owner {} lacks a grounded corbel or frame support graph",
                    defense.owner.0
                ),
            ));
        }
        let drain_valid = defense.drain_route.is_some_and(|id| {
            plan.resolved_geometry
                .drainage_routes
                .iter()
                .find(|route| route.id == id && route.owner == defense.owner)
                .is_some_and(|route| {
                    route.outlet.y < route.inlet.y - 0.04
                        && !defense.throat_voids.contains(&route.outlet_void)
                })
        });
        let catchments = defense
            .drainage_catchments
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == *id && catchment.owner == defense.owner)
            })
            .collect::<Vec<_>>();
        let physical_catchments = catchments.len() == defense.drainage_catchments.len()
            && !catchments.is_empty()
            && catchments.iter().all(|catchment| {
                let channels = catchment
                    .toe_channel_solids
                    .iter()
                    .filter_map(|id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *id && solid.role == SolidRole::DrainageFloor)
                            .copied()
                    })
                    .collect::<Vec<_>>();
                let route = plan
                    .resolved_geometry
                    .drainage_routes
                    .iter()
                    .find(|route| route.id == catchment.outlet_route);
                let channel_chain_valid = channels.len() == catchment.toe_channel_solids.len()
                    && !channels.is_empty()
                    && route.is_some_and(|route| {
                        channels.last().is_some_and(|channel| {
                            let local_x =
                                Vec2::new(channel.yaw_radians.cos(), -channel.yaw_radians.sin());
                            let downhill = local_x * -channel.longfall_radians.signum();
                            let endpoint = Vec2::new(channel.centre.x, channel.centre.z)
                                + downhill * channel.size.x * 0.5;
                            endpoint.distance(Vec2::new(route.inlet.x, route.inlet.z)) <= 0.015
                                && channel.longfall_radians.abs() >= 0.005
                                && channel.centre.y + channel.size.y * 0.5
                                    <= defense.floor_elevation_metres - 0.015
                        })
                    });
                let floors_reach_channel = defense.floor_solids.iter().all(|floor_id| {
                    let Some(floor) = solids.iter().find(|solid| solid.id == *floor_id) else {
                        return false;
                    };
                    let local_x = Vec2::new(floor.yaw_radians.cos(), -floor.yaw_radians.sin());
                    let local_z = Vec2::new(floor.yaw_radians.sin(), floor.yaw_radians.cos());
                    let gradient =
                        local_x * -floor.longfall_radians.signum() * floor.longfall_radians.abs()
                            + local_z
                                * floor.crossfall_radians.signum()
                                * floor.crossfall_radians.abs();
                    if gradient.length() < 0.005 {
                        return false;
                    }
                    let downhill = gradient.normalize();
                    [-0.4_f32, 0.0, 0.4].into_iter().all(|x| {
                        [-0.4_f32, 0.0, 0.4].into_iter().all(|z| {
                            let start = Vec2::new(floor.centre.x, floor.centre.z)
                                + local_x * floor.size.x * x
                                + local_z * floor.size.z * z;
                            (0..=100).any(|step| {
                                let point = start + downhill * step as f32 * 0.04;
                                channels.iter().any(|channel| {
                                    resolved_solid_contains_point(
                                        channel,
                                        Vec3::new(point.x, channel.centre.y, point.y),
                                        0.025,
                                    )
                                }) || route.is_some_and(|route| {
                                    plan.resolved_geometry.voids.iter().any(|void| {
                                        void.id == route.outlet_void
                                            && point.x >= void.bounds.min.x - 0.025
                                            && point.x <= void.bounds.max.x + 0.025
                                            && point.y >= void.bounds.min.z - 0.025
                                            && point.y <= void.bounds.max.z + 0.025
                                    })
                                })
                            })
                        })
                    })
                });
                let floors_and_channels_disjoint = defense.floor_solids.iter().all(|floor_id| {
                    solids
                        .iter()
                        .find(|solid| solid.id == *floor_id)
                        .is_some_and(|floor| {
                            channels.iter().all(|channel| {
                                !resolved_solids_overlap_positive_volume(floor, channel, 0.004)
                            })
                        })
                });
                channel_chain_valid && floors_reach_channel && floors_and_channels_disjoint
            });
        let weather_catchments = defense
            .weather_catchments
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == *id && catchment.owner == defense.owner)
            })
            .collect::<Vec<_>>();
        let weather_solids_exist = !defense.weathering_solids.is_empty()
            && defense.weathering_solids.iter().all(|id| {
                solids.iter().any(|solid| {
                    solid.id == *id
                        && matches!(
                            solid.role,
                            SolidRole::DefenseRoof
                                | SolidRole::Coping
                                | SolidRole::DrainageFloor
                                | SolidRole::RoofFlashing
                        )
                })
            });
        let weather_drains_physically = weather_catchments.len()
            == defense.weather_catchments.len()
            && !weather_catchments.is_empty()
            && weather_catchments.iter().all(|catchment| {
                let Some(source) = solids.iter().find(|solid| solid.id == catchment.walk_solid)
                else {
                    return false;
                };
                let Some(route) = plan.resolved_geometry.drainage_routes.iter().find(|route| {
                    route.id == catchment.outlet_route && route.owner == defense.owner
                }) else {
                    return false;
                };
                let local_z = Vec2::new(source.yaw_radians.sin(), source.yaw_radians.cos());
                let physical_downhill = local_z * source.crossfall_radians.signum();
                let weather_outward = match defense.path {
                    ProjectedDefensePath::Round { centre, .. }
                        if source.role == SolidRole::Coping =>
                    {
                        (Vec2::new(source.centre.x, source.centre.z) - centre).normalize_or_zero()
                    }
                    _ => outward,
                };
                let gradient_outward = source.crossfall_radians.abs() >= 0.04
                    && physical_downhill.dot(weather_outward) >= 0.8
                    && catchment.outward.dot(weather_outward) >= 0.8
                    && catchment.inner_elevation_metres > catchment.outer_elevation_metres + 0.01;
                let route_is_open_drip =
                    route.outlet.y < route.inlet.y - 0.04
                        && !defense.throat_voids.contains(&route.outlet_void)
                        && plan.resolved_geometry.voids.iter().any(|void| {
                            void.id == route.outlet_void && void.role == VoidRole::Drain
                        });
                let toe_reaches_inlet = if catchment.toe_channel_solids.is_empty() {
                    let source_centre = Vec2::new(source.centre.x, source.centre.z);
                    let expected = source_centre
                        + catchment.outward * catchment.width_metres * 0.5
                        + catchment.tangent * catchment.outlet_along_metres;
                    expected.distance(Vec2::new(route.inlet.x, route.inlet.z)) <= 0.22
                } else {
                    catchment.toe_channel_solids.iter().all(|id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|channel| {
                                channel.role == SolidRole::DrainageFloor
                                    && channel.longfall_radians.abs() >= 0.005
                                    && channel.centre.y + channel.size.y * 0.5
                                        <= catchment.outer_elevation_metres + 0.005
                            })
                    }) && catchment.toe_channel_solids.last().is_some_and(|id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|channel| {
                                let local_x = Vec2::new(
                                    channel.yaw_radians.cos(),
                                    -channel.yaw_radians.sin(),
                                );
                                let downhill = local_x * -channel.longfall_radians.signum();
                                let endpoint = Vec2::new(channel.centre.x, channel.centre.z)
                                    + downhill * channel.size.x * 0.5;
                                endpoint.distance(Vec2::new(route.inlet.x, route.inlet.z)) <= 0.02
                            })
                    })
                };
                gradient_outward && route_is_open_drip && toe_reaches_inlet
            });
        let roof_or_coping_contract = if defense.roofed {
            solids.iter().any(|solid| {
                solid.role == SolidRole::DefenseRoof
                    && defense.weathering_solids.contains(&solid.id)
            }) && solids.iter().any(|solid| {
                solid.role == SolidRole::RoofFlashing
                    && defense.weathering_solids.contains(&solid.id)
            })
        } else if defense.material == ProjectedDefenseMaterial::Masonry {
            solids.iter().any(|solid| {
                solid.role == SolidRole::Coping && defense.weathering_solids.contains(&solid.id)
            })
        } else {
            true
        };
        let roof_bearing_contract = if defense.kind == ProjectedDefenseKind::Breteche {
            let roof = solids
                .iter()
                .copied()
                .find(|solid| solid.role == SolidRole::DefenseRoof);
            let bearing_node = defense.roof_bearing_node.and_then(|id| {
                plan.resolved_geometry
                    .structural_nodes
                    .iter()
                    .find(|node| node.id == id && node.owner == defense.owner)
            });
            let support_solids = defense
                .roof_support_solids
                .iter()
                .filter_map(|id| solids.iter().copied().find(|solid| solid.id == *id))
                .collect::<Vec<_>>();
            let plates = support_solids
                .iter()
                .copied()
                .filter(|solid| solid.role == SolidRole::RoofPlate)
                .collect::<Vec<_>>();
            roof.is_some_and(|roof| {
                bearing_node.is_some_and(|node| {
                    roof.supported_by == [node.id]
                        && node.supported_by.len() == 2
                        && node.supported_by.iter().all(|parent| {
                            plan.resolved_geometry
                                .structural_nodes
                                .iter()
                                .any(|candidate| {
                                    candidate.id == *parent
                                        && candidate.owner == defense.owner
                                        && !candidate.supported_by.is_empty()
                                })
                        })
                }) && support_solids.len() == defense.roof_support_solids.len()
                    && support_solids.len() >= 5
                    && plates.len() == 2
                    && plates.iter().all(|plate| {
                        let expected_underside = roof.centre.y
                            - (Vec2::new(plate.centre.x, plate.centre.z)
                                - Vec2::new(roof.centre.x, roof.centre.z))
                            .dot(outward)
                                * roof.crossfall_radians.abs().tan()
                            - roof.size.y * 0.5;
                        let plate_top = plate.centre.y + plate.size.y * 0.5;
                        let roof_contact = (plate_top - expected_underside).abs() <= 0.025
                            && resolved_plan_overlap_area(roof, plate) >= 0.08;
                        let local_x = Vec2::new(plate.yaw_radians.cos(), -plate.yaw_radians.sin());
                        let bearing_samples = [-1.0_f32, 1.0].into_iter().all(|side| {
                            let point = Vec2::new(plate.centre.x, plate.centre.z)
                                + local_x * side * (plate.size.x * 0.5 - 0.47);
                            support_solids.iter().any(|support| {
                                support.id != plate.id
                                    && support.role != SolidRole::RoofPlate
                                    && (support.centre.y + support.size.y * 0.5
                                        - (plate.centre.y - plate.size.y * 0.5))
                                        .abs()
                                        <= 0.025
                                    && resolved_plan_overlap_area(support, plate) >= 0.014
                                    && resolved_solid_contains_point(
                                        support,
                                        Vec3::new(point.x, support.centre.y, point.y),
                                        0.12,
                                    )
                            })
                        });
                        roof_contact && bearing_samples
                    })
            })
        } else {
            defense.roof_support_solids.is_empty() && defense.roof_bearing_node.is_none()
        };
        if !drain_valid
            || !physical_catchments
            || !weather_solids_exist
            || !weather_drains_physically
            || !roof_or_coping_contract
        {
            issues.push(issue(
                "projected_defense_roof_drain_failure",
                format!(
                    "projected defense owner {} lacks independent roof/floor drainage route={drain_valid} catchment={physical_catchments} weather_solids={weather_solids_exist} weather_flow={weather_drains_physically} roof_or_coping={roof_or_coping_contract}",
                    defense.owner.0,
                ),
            ));
        }
        if !roof_bearing_contract {
            issues.push(issue(
                "unsupported_projected_defense_roof",
                format!(
                    "projected defense owner {} roof lacks two physically touching wall-plate load regions and a grounded bearing DAG",
                    defense.owner.0,
                ),
            ));
        }
        if defense.kind == ProjectedDefenseKind::Bartizan {
            let (centre, radius) = match defense.path {
                ProjectedDefensePath::Round {
                    centre,
                    radius_metres,
                    ..
                } => (centre, radius_metres),
                ProjectedDefensePath::Linear { .. } => unreachable!(),
            };
            let interior = Vec3::new(
                centre.x,
                defense.floor_elevation_metres + defense.clear_height_metres * 0.5,
                centre.y,
            );
            let floor_covers_usable_volume =
                [0.2_f32, 0.45, 0.65].into_iter().all(|radius_factor| {
                    (0..16).all(|segment| {
                        let angle = segment as f32 * std::f32::consts::TAU / 16.0;
                        let point =
                            centre + Vec2::new(angle.cos(), angle.sin()) * radius * radius_factor;
                        let in_throat = defense.throat_voids.iter().any(|id| {
                            plan.resolved_geometry.voids.iter().any(|void| {
                                void.id == *id
                                    && point.x >= void.bounds.min.x
                                    && point.x <= void.bounds.max.x
                                    && point.y >= void.bounds.min.z
                                    && point.y <= void.bounds.max.z
                            })
                        });
                        in_throat
                            || defense.floor_solids.iter().any(|id| {
                                solids
                                    .iter()
                                    .find(|solid| solid.id == *id)
                                    .is_some_and(|solid| {
                                        resolved_solid_contains_point(
                                            solid,
                                            Vec3::new(
                                                point.x,
                                                defense.floor_elevation_metres - 0.03,
                                                point.y,
                                            ),
                                            0.035,
                                        )
                                    })
                            })
                    })
                });
            let loops_are_narrow_split_openings = defense.firing_apertures.iter().all(|id| {
                plan.resolved_geometry
                    .voids
                    .iter()
                    .find(|void| void.id == *id)
                    .is_some_and(|void| {
                        let size = void.bounds.max - void.bounds.min;
                        size.x.max(size.z) <= 0.2
                            && size.y <= 0.55
                            && solids
                                .iter()
                                .filter(|solid| solid.role == SolidRole::BartizanShell)
                                .filter(|solid| {
                                    Vec2::new(solid.centre.x, solid.centre.z).distance(centre)
                                        <= radius + 0.15
                                })
                                .count()
                                >= 12
                    })
            });
            if solids
                .iter()
                .any(|solid| resolved_solid_contains_point(solid, interior, 0.0))
                || !solids
                    .iter()
                    .any(|solid| solid.role == SolidRole::BartizanShell)
                || defense.firing_apertures.is_empty()
                || !floor_covers_usable_volume
                || !loops_are_narrow_split_openings
            {
                issues.push(issue(
                    "closed_bartizan",
                    format!(
                        "bartizan owner {} is not a hollow usable firing volume",
                        defense.owner.0
                    ),
                ));
            }
        }
        if defense.material == ProjectedDefenseMaterial::Timber
            && solids
                .iter()
                .filter(|solid| solid.role == SolidRole::FrameMember)
                .any(|member| {
                    member.supported_by.iter().any(|id| {
                        !plan
                            .resolved_geometry
                            .structural_nodes
                            .iter()
                            .any(|node| node.id == *id)
                    })
                })
        {
            issues.push(issue(
                "dangling_hoarding_frame",
                format!(
                    "hoarding owner {} has a dangling frame member",
                    defense.owner.0
                ),
            ));
        }
    }
}

fn audit_gatehouse_assemblies(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for (assembly_index, spec) in plan.gatehouse_assemblies.iter().copied().enumerate() {
        let Some(wall) = plan.curtain_walls.get(spec.curtain_wall_index).copied() else {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} references a missing curtain"),
            ));
            continue;
        };
        let Some(defense) = plan
            .gate_defenses
            .iter()
            .find(|defense| defense.curtain_wall_index == spec.curtain_wall_index)
        else {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} has no resolved defense"),
            ));
            continue;
        };
        let tangent = (wall.end - wall.start).normalize_or_zero();
        let outward = direction_vector(wall.outward);
        if !(tangent.x.abs() >= 0.999 || tangent.y.abs() >= 0.999)
            || tangent.dot(outward).abs() > 0.001
        {
            issues.push(issue(
                "invalid_gatehouse_orientation",
                format!(
                    "gatehouse {assembly_index} requires a cardinal wall and perpendicular outward"
                ),
            ));
            continue;
        }
        let threshold = (wall.start + wall.end) * 0.5;
        if wall
            .gate_width_metres
            .is_none_or(|width| (width - spec.gate_width.metres()).abs() > 0.01)
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} curtain opening differs from its source spec"),
            ));
        }
        if (defense.passage_profile.width_metres - spec.gate_width.metres()).abs() > 0.01
            || (defense.passage_profile.spring_height_metres - wall.gate_height_metres).abs() > 0.01
            || (defense.passage_profile.arch_rise_metres - spec.arch_rise.metres()).abs() > 0.01
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!(
                    "gatehouse {assembly_index} passage cross-section differs from its source spec"
                ),
            ));
        }
        let radius = spec.tower_diameter.metres() * 0.5;
        let tower_offset = spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + radius;
        let expected_centres = [
            threshold - tangent * tower_offset,
            threshold + tangent * tower_offset,
        ];
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index,
            right_tower_index,
            bearing_depth,
            arch_centre,
            arch_spring_elevation_metres,
            arch_ring_depth,
            arch_rise,
            curtain_return_bond,
        } = defense.guard_chamber.load_path;
        let tower_indices = [left_tower_index, right_tower_index];
        let mut resolved = [None, None];
        for (side, (&index, expected)) in tower_indices.iter().zip(expected_centres).enumerate() {
            resolved[side] = plan.towers.get(index).copied();
            let Some(tower) = resolved[side] else {
                issues.push(issue(
                    "declared_load_path",
                    format!("gatehouse {assembly_index} bearing references missing tower {index}"),
                ));
                continue;
            };
            if tower.diameter() != spec.tower_diameter
                || (tower.centre_metres() - expected).length() > 0.01
                || (tower.wall_thickness_metres - spec.tower_shell.metres()).abs() > 0.01
            {
                issues.push(issue("gatehouse_spec_drift", format!("gatehouse {assembly_index} tower {index} is not derived from its discrete anchor/diameter")));
            }
            let expected_direction = if side == 0 {
                cardinal_direction(tangent)
            } else {
                cardinal_direction(-tangent)
            };
            if !tower.chord_interface.is_some_and(|interface| {
                interface.toward_gate == expected_direction
                    && interface.bearing_depth == spec.chord_bearing
            }) {
                issues.push(issue(
                    "round_rect_splice",
                    format!(
                        "gatehouse {assembly_index} tower {index} lacks its derived chord interface"
                    ),
                ));
            }
        }
        if bearing_depth != spec.chord_bearing
            || arch_ring_depth != spec.arch_ring_depth
            || arch_rise != spec.arch_rise
            || curtain_return_bond != spec.curtain_return_bond
            || (arch_centre - threshold).length() > 0.01
            || (arch_spring_elevation_metres - wall.gate_height_metres).abs() > 0.01
        {
            issues.push(issue(
                "declared_load_path",
                format!("gatehouse {assembly_index} load path differs from its source spec"),
            ));
        }

        let chamber = &defense.guard_chamber;
        let chamber_along = chamber.size.dot(tangent.abs());
        let chamber_depth = chamber.size.dot(outward.abs());
        let expected_along = 2.0 * (tower_offset - (radius - spec.chord_bearing.metres()));
        if (chamber.centre - threshold).length() > 0.01
            || (chamber_along - expected_along).abs() > 0.01
            || (chamber_depth - spec.chamber_depth.metres()).abs() > 0.01
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} chamber is not the wall-local derived volume"),
            ));
        }
        let access = &chamber.access;
        let expected_depth = spec.chamber_depth.metres() * 0.5 + 0.6;
        let expected_top = threshold - tangent * 1.9 + (-outward) * expected_depth;
        let expected_bottom = threshold + tangent * 1.9 + (-outward) * expected_depth;
        let expected_door =
            threshold + tangent * 1.9 + (-outward) * (spec.chamber_depth.metres() * 0.5);
        if (access.top_landing.centre - expected_top).length() > 0.01
            || (access.bottom_landing.centre - expected_bottom).length() > 0.01
            || (access.door.position - expected_door).length() > 0.01
            || (access.flight.top - (expected_top + tangent * 0.5)).length() > 0.01
            || (access.flight.bottom - (expected_bottom - tangent * 0.5)).length() > 0.01
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} access route is not derived wall-locally"),
            ));
        }
        let supported_half_span =
            spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + bearing_depth.metres();
        if chamber_along * 0.5 > supported_half_span + 0.01 {
            issues.push(issue(
                "declared_load_path",
                format!("gatehouse {assembly_index} floor projection exceeds its arch-and-tower tributary support"),
            ));
        }
        let chord_half = (radius * radius - (radius - spec.chord_bearing.metres()).powi(2)).sqrt();
        if (chord_half - chamber_depth * 0.5).abs() > 0.02 {
            issues.push(issue(
                "round_rect_splice",
                format!("gatehouse {assembly_index} chamber edge does not match its tower chord"),
            ));
        }
        let arch_bottom = arch_spring_elevation_metres;
        let arch_top = arch_bottom + arch_ring_depth.metres() + arch_rise.metres();
        let floor_bottom = chamber.floor_elevation_metres - 0.09;
        if arch_bottom + 0.01 < wall.gate_height_metres || floor_bottom + 0.01 < arch_top {
            issues.push(issue(
                "gate_passage_clear",
                format!("gatehouse {assembly_index} arch/floor intrudes into the required passage"),
            ));
        }
        let required_return = radius
            - (radius * radius - (wall.thickness_metres * 0.5).powi(2))
                .max(0.0)
                .sqrt();
        if curtain_return_bond.metres() + 0.001 < required_return
            || curtain_return_bond.metres() > spec.tower_shell.metres()
        {
            issues.push(issue(
                "round_rect_splice",
                format!("gatehouse {assembly_index} curtain return lacks a positive full-face tower bond"),
            ));
        }
        if chamber.supports.iter().any(|support| {
            let local = support.centre - threshold;
            let along = local.dot(tangent).abs();
            let half_along = support.size.dot(tangent.abs()) * 0.5;
            along - half_along < spec.gate_width.metres() * 0.5
                && support.base_elevation_metres < wall.gate_height_metres
        }) {
            issues.push(issue(
                "gate_passage_clear",
                format!("gatehouse {assembly_index} has a solid in its required passage void"),
            ));
        }
        if chamber.floor_elevation_metres < wall.gate_height_metres
            || chamber
                .openings
                .iter()
                .any(|opening| opening.width_metres < 0.1 || opening.clear_height_metres < 0.1)
        {
            issues.push(issue("room_void_disjoint_from_solids", format!("gatehouse {assembly_index} usable chamber/opening volume intersects resolved solids")));
        }
        let room_rect = oriented_rect(
            chamber.centre,
            tangent,
            outward,
            (chamber_along * 0.5 - 0.28).max(0.0),
            (chamber_depth * 0.5 - 0.28).max(0.0),
        );
        let passage_rect = oriented_rect(
            threshold,
            tangent,
            outward,
            spec.gate_width.metres() * 0.5,
            chamber_depth * 0.5,
        );
        let room_prism = Prism {
            rect: room_rect,
            low: chamber.floor_elevation_metres + 0.09,
            high: chamber.floor_elevation_metres + chamber.clear_height_metres,
        };
        let passage_prism = Prism {
            rect: passage_rect,
            low: 0.0,
            high: wall.gate_height_metres,
        };
        for tower in resolved.into_iter().flatten() {
            if retained_tower_overlaps_rect(tower, room_prism.rect) {
                issues.push(issue(
                    "room_void_disjoint_from_solids",
                    format!(
                        "gatehouse {assembly_index} chamber clear prism intersects a tower solid"
                    ),
                ));
                issues.push(issue(
                    "undeclared_solid_overlap",
                    format!("gatehouse {assembly_index} chamber crosses its declared tower chord"),
                ));
            }
            if circle_overlaps_rect(
                tower.centre_metres(),
                tower.radius_metres(),
                passage_prism.rect,
            ) {
                issues.push(issue(
                    "gate_passage_clear",
                    format!("gatehouse {assembly_index} tower intrudes into the passage prism"),
                ));
            }
        }
        for support in &chamber.supports {
            let support_prism = Prism {
                rect: axis_rect(support.centre, support.size * 0.5),
                low: support.base_elevation_metres,
                high: support.top_elevation_metres,
            };
            if prisms_overlap(support_prism, passage_prism) {
                issues.push(issue(
                    "gate_passage_clear",
                    format!("gatehouse {assembly_index} support intersects the passage prism"),
                ));
            }
            if prisms_overlap(support_prism, room_prism) {
                issues.push(issue(
                    "room_void_disjoint_from_solids",
                    format!(
                        "gatehouse {assembly_index} support intersects the chamber clear prism"
                    ),
                ));
            }
            if resolved.into_iter().flatten().any(|tower| {
                circle_overlaps_rect(
                    tower.centre_metres(),
                    tower.radius_metres(),
                    support_prism.rect,
                )
            }) {
                issues.push(issue(
                    "undeclared_solid_overlap",
                    format!("gatehouse {assembly_index} support overlaps a tower outside a declared bearing"),
                ));
            }
        }
        for (index, support) in chamber.supports.iter().enumerate() {
            let a = Prism {
                rect: axis_rect(support.centre, support.size * 0.5),
                low: support.base_elevation_metres,
                high: support.top_elevation_metres,
            };
            for other in chamber.supports.iter().skip(index + 1) {
                let b = Prism {
                    rect: axis_rect(other.centre, other.size * 0.5),
                    low: other.base_elevation_metres,
                    high: other.top_elevation_metres,
                };
                if prisms_overlap(a, b) {
                    issues.push(issue(
                        "undeclared_solid_overlap",
                        format!("gatehouse {assembly_index} supports overlap with positive volume"),
                    ));
                }
            }
        }
        if let [Some(left), Some(right)] = resolved
            && (left.centre_metres() - right.centre_metres()).length()
                < left.radius_metres() + right.radius_metres() - 0.001
        {
            issues.push(issue(
                "undeclared_solid_overlap",
                format!("gatehouse {assembly_index} flanking towers overlap"),
            ));
        }
        if chamber_depth > chord_half * 2.0 + 0.02 {
            issues.push(issue(
                "room_void_disjoint_from_solids",
                format!(
                    "gatehouse {assembly_index} chamber side walls exceed the open tower chords"
                ),
            ));
        }
        // A 256-segment tower shell must omit at least two facets across every
        // firing slit, otherwise the semantic aperture would render closed.
        let shell_sample = std::f32::consts::TAU * radius / 256.0;
        if defense.firing_positions.iter().any(|position| {
            !tower_indices.contains(&position.tower_index)
                || position.aperture_width_metres + 0.001 < shell_sample * 2.0
        }) {
            issues.push(issue(
                "aperture_clearance",
                format!(
                    "gatehouse {assembly_index} firing aperture is not resolved by the tower shell"
                ),
            ));
        }
        // The independent curtain renderer is required to terminate at the
        // outer tower tangencies; a tower located away from the wall axis would
        // make that trim overlap or leave an undeclared gap.
        if resolved
            .into_iter()
            .flatten()
            .any(|tower| ((tower.centre_metres() - threshold).dot(outward)).abs() > 0.01)
        {
            issues.push(issue("undeclared_solid_overlap", format!("gatehouse {assembly_index} tower is not coplanar with the resolved curtain splice")));
        }
    }
}

fn cardinal_direction(vector: Vec2) -> Direction {
    if vector.x.abs() >= vector.y.abs() {
        if vector.x >= 0.0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if vector.y >= 0.0 {
        Direction::North
    } else {
        Direction::South
    }
}

#[derive(Clone, Copy)]
struct Rect2 {
    min: Vec2,
    max: Vec2,
}

#[derive(Clone, Copy)]
struct Prism {
    rect: Rect2,
    low: f32,
    high: f32,
}

fn axis_rect(centre: Vec2, half: Vec2) -> Rect2 {
    Rect2 {
        min: centre - half,
        max: centre + half,
    }
}

fn oriented_rect(
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    half_along: f32,
    half_depth: f32,
) -> Rect2 {
    let half = tangent.abs() * half_along + outward.abs() * half_depth;
    axis_rect(centre, half)
}

fn prisms_overlap(a: Prism, b: Prism) -> bool {
    a.low < b.high - 0.001 && a.high > b.low + 0.001 && rects_overlap(a.rect, b.rect)
}

fn rects_overlap(a: Rect2, b: Rect2) -> bool {
    a.min.x < b.max.x - 0.001
        && a.max.x > b.min.x + 0.001
        && a.min.y < b.max.y - 0.001
        && a.max.y > b.min.y + 0.001
}

fn circle_overlaps_rect(centre: Vec2, radius: f32, rect: Rect2) -> bool {
    let nearest = centre.clamp(rect.min, rect.max);
    (nearest - centre).length_squared() < (radius - 0.001).powi(2)
}

fn retained_tower_overlaps_rect(tower: crate::RoundTower, mut rect: Rect2) -> bool {
    let centre = tower.centre_metres();
    for interface in tower.chord_interfaces() {
        let cut = tower.radius_metres() - interface.bearing_depth.metres();
        match interface.toward_gate {
            Direction::East => rect.max.x = rect.max.x.min(centre.x + cut),
            Direction::West => rect.min.x = rect.min.x.max(centre.x - cut),
            Direction::North => rect.max.y = rect.max.y.min(centre.y + cut),
            Direction::South => rect.min.y = rect.min.y.max(centre.y - cut),
        }
    }
    rect.min.x < rect.max.x - 0.001
        && rect.min.y < rect.max.y - 0.001
        && circle_overlaps_rect(centre, tower.radius_metres(), rect)
}

fn audit_fortified_profile(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if !matches!(
        plan.archetype,
        BuildingArchetype::CourtyardCastle | BuildingArchetype::WalledKeep
    ) {
        return;
    }
    // These are declared game-profile gates for the modernized walled keep,
    // not universal measurements claimed for every sixteenth-century work.
    const MIN_MASONRY_THICKNESS: f32 = 1.2;
    const MAX_UNFLANKED_RUN: f32 = 36.0;

    for (index, wall) in plan.curtain_walls.iter().enumerate() {
        if wall.thickness_metres + 0.001 < MIN_MASONRY_THICKNESS {
            issues.push(issue(
                "wall_too_thin_for_profile",
                format!(
                    "curtain wall {index} is only {:.2} m thick",
                    wall.thickness_metres
                ),
            ));
        }
        let delta = wall.end - wall.start;
        let length = delta.length();
        let axis = delta / length.max(0.001);
        let mut positions = plan
            .towers
            .iter()
            .filter_map(|tower| {
                let projected = (tower.centre_metres() - wall.start).dot(axis);
                let nearest = wall.start + axis * projected.clamp(0.0, length);
                ((tower.centre_metres() - nearest).length() <= tower.radius_metres() + 0.25)
                    .then_some(projected.clamp(0.0, length))
            })
            .collect::<Vec<_>>();
        positions.extend([0.0, length]);
        positions.sort_by(f32::total_cmp);
        if positions
            .windows(2)
            .any(|pair| pair[1] - pair[0] > MAX_UNFLANKED_RUN)
        {
            issues.push(issue(
                "unflanked_curtain",
                format!("curtain wall {index} exceeds the declared flanking interval"),
            ));
        }
    }

    for (index, tower) in plan.towers.iter().enumerate() {
        if tower.wall_thickness_metres + 0.001 < MIN_MASONRY_THICKNESS {
            issues.push(issue(
                "wall_too_thin_for_profile",
                format!(
                    "tower {index} shell is only {:.2} m thick",
                    tower.wall_thickness_metres
                ),
            ));
        }
        let interior_radius = tower.radius_metres() - tower.wall_thickness_metres;
        if plan.stairs.iter().any(|stair| matches!(
            stair,
            Stair::Spiral { centre, outer_radius_metres, .. }
                if close_vec(*centre, tower.centre_metres()) && *outer_radius_metres > interior_radius - 0.1
        )) {
            issues.push(issue(
                "insufficient_walk_clearance",
                format!("tower {index} stair collides with its masonry shell"),
            ));
        }
    }
    if plan.archetype == BuildingArchetype::WalledKeep {
        audit_gate_defenses(plan, issues);
    }
}

fn audit_gate_defenses(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for (wall_index, wall) in plan.curtain_walls.iter().enumerate() {
        if wall.gate_width_metres.is_none() {
            continue;
        }
        let Some(defense) = plan
            .gate_defenses
            .iter()
            .find(|defense| defense.curtain_wall_index == wall_index)
        else {
            issues.push(issue(
                "undefended_gate",
                format!("curtain gate {wall_index} has no declared defense"),
            ));
            continue;
        };
        let mut independent = std::collections::BTreeSet::new();
        let sectors = defense
            .firing_positions
            .iter()
            .filter(|position| {
                position.aperture_width_metres > 0.0
                    && position.aperture_width_metres <= 0.2
                    && firing_origin_matches_aperture(plan, position)
                    && firing_sector_covers(position, defense.threshold)
                    && firing_sector_covers(position, defense.approach)
                    && ray_clear_of_solids(plan, position, defense.threshold, wall_index)
                    && ray_clear_of_solids(plan, position, defense.approach, wall_index)
                    && independent.insert((position.tower_index, position.aperture_id))
            })
            .count();
        if sectors < 2 {
            issues.push(issue(
                "undefended_gate",
                format!("curtain gate {wall_index} has only {sectors} valid firing sectors"),
            ));
        }
        let heavy = defense
            .closures
            .iter()
            .any(|closure| closure.kind == GateClosureKind::HeavyLeaves);
        let second = defense
            .closures
            .iter()
            .any(|closure| closure.kind == GateClosureKind::Portcullis);
        if !heavy || !second {
            issues.push(issue(
                "undefended_gate",
                format!("curtain gate {wall_index} lacks two closures"),
            ));
        }
        if defense
            .closures
            .iter()
            .any(|closure| !closure_covers_passage(*closure, defense.passage_profile))
        {
            issues.push(issue(
                "unsealed_gate_passage",
                format!("curtain gate {wall_index} has a closure that leaves part of its arch profile open"),
            ));
        }
        audit_guard_chamber(plan, defense, wall_index, issues);
    }
}

fn closure_covers_passage(closure: crate::GateClosure, passage: crate::GatePassageProfile) -> bool {
    if closure.coverage.width_metres + 0.01 < passage.width_metres {
        return false;
    }
    (0..=16).all(|sample| {
        let along = (sample as f32 / 16.0 - 0.5) * passage.width_metres;
        closure.coverage.height_at(along) + 0.01 >= passage.height_at(along)
    })
}

fn audit_guard_chamber(
    plan: &BuildingPlan,
    defense: &crate::GateDefense,
    wall_index: usize,
    issues: &mut Vec<AuditIssue>,
) {
    let chamber = &defense.guard_chamber;
    let Some(wall) = plan.curtain_walls.get(chamber.supporting_wall_index) else {
        issues.push(issue(
            "unsupported_guard_chamber",
            format!("gate chamber for wall {wall_index} references a missing support wall"),
        ));
        return;
    };
    let wall_tangent = (wall.end - wall.start).normalize_or_zero();
    let inward = -direction_vector(wall.outward);
    let gate_half_width = wall.gate_width_metres.unwrap_or(0.0) * 0.5;
    let chamber_half = chamber.size * 0.5;
    let supports_are_piers = !chamber.supports.is_empty()
        && chamber.supports.iter().all(|support| {
            let relative = support.centre - chamber.centre;
            let tangent_offset = relative.dot(wall_tangent);
            let inward_offset = relative.dot(inward);
            let tangent_half_extent = support.size.x * wall_tangent.x.abs() * 0.5
                + support.size.y * wall_tangent.y.abs() * 0.5;
            let inward_half_extent =
                support.size.x * inward.x.abs() * 0.5 + support.size.y * inward.y.abs() * 0.5;
            tangent_offset.abs() + tangent_half_extent <= chamber_half.x.max(chamber_half.y) + 0.05
                && inward_offset.abs() + inward_half_extent
                    <= chamber_half.x.max(chamber_half.y) + 0.05
                && (support.centre - defense.threshold).dot(wall_tangent).abs()
                    - tangent_half_extent
                    >= gate_half_width - 0.05
        });
    let bonded_load_path = match chamber.load_path {
        crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index,
            right_tower_index,
            bearing_depth,
            arch_ring_depth,
            arch_rise,
            curtain_return_bond,
            ..
        } => {
            let towers =
                [left_tower_index, right_tower_index].map(|index| plan.towers.get(index).copied());
            towers.iter().all(Option::is_some)
                && towers.into_iter().flatten().all(|tower| {
                    tower
                        .chord_interface
                        .is_some_and(|interface| interface.bearing_depth == bearing_depth)
                })
                && bearing_depth.units() > 0
                && arch_ring_depth.units() > 0
                && arch_rise.units() > 0
                && curtain_return_bond.units() > 0
        }
    };
    if chamber.supporting_wall_index != wall_index
        || chamber.size.x * chamber.size.y < 6.0
        || chamber.clear_height_metres < 2.0
        || chamber.floor_elevation_metres + 0.01 < wall.gate_height_metres
        || chamber.supports.iter().any(|support| {
            !close(support.top_elevation_metres, chamber.floor_elevation_metres)
                || support.base_elevation_metres > 0.2
        })
        || (!supports_are_piers && !bonded_load_path)
    {
        issues.push(issue(
            "unsupported_guard_chamber",
            format!("gate chamber for wall {wall_index} lacks supported usable volume"),
        ));
    }
    let access = &chamber.access;
    let access_walk = plan.wall_walks.get(access.from_walk_index).copied();
    let access_walk_is_in_reachable_circuit = plan.defensive_circuits.iter().any(|circuit| {
        circuit.walks.contains(&access.from_walk_index)
            && circuit.walks.iter().copied().any(|index| {
                plan.wall_walks
                    .get(index)
                    .copied()
                    .is_some_and(|walk| walk_has_stair_access(plan, walk))
            })
    });
    if !access_walk_is_in_reachable_circuit {
        issues.push(issue(
            "inaccessible_guard_chamber",
            format!("gate chamber for wall {wall_index} has no usable route from the wall walk"),
        ));
    }
    audit_guard_access(plan, defense, wall, access_walk, issues);
    let portcullis_position = chamber.operating_positions.iter().any(|position| {
        defense
            .closures
            .get(position.closure_index)
            .is_some_and(|closure| {
                closure.kind == GateClosureKind::Portcullis
                    && (position.position
                        - (defense.threshold + inward * closure.inward_offset_metres))
                        .length()
                        <= 0.4
            })
            && close(position.elevation_metres, chamber.floor_elevation_metres)
            && point_in_rect(position.position, chamber.centre, chamber.size)
    });
    let outward = chamber.openings.iter().any(|opening| {
        opening.kind == crate::GuardOpeningKind::OutwardObservation
            && opening.facing == wall.outward
            && opening.width_metres > 0.0
            && opening.clear_height_metres > 0.0
            && point_on_rect_boundary(opening.position, chamber.centre, chamber.size, 0.15)
    });
    let downward = chamber.openings.iter().any(|opening| {
        opening.kind == crate::GuardOpeningKind::DownwardDefense
            && (opening.target - defense.threshold).length() < 0.25
            && opening.width_metres > 0.0
            && opening.clear_height_metres > 0.0
            && point_in_rect(opening.position, chamber.centre, chamber.size)
    });
    if !portcullis_position || !outward || !downward {
        issues.push(issue(
            "inoperable_guard_chamber",
            format!("gate chamber for wall {wall_index} cannot observe and operate its defenses"),
        ));
    }
}

fn point_on_rect_boundary(point: Vec2, centre: Vec2, size: Vec2, tolerance: f32) -> bool {
    if !point_in_rect(point, centre, size + Vec2::splat(tolerance * 2.0)) {
        return false;
    }
    let local = (point - centre).abs();
    let half = size * 0.5;
    (local.x - half.x).abs() <= tolerance || (local.y - half.y).abs() <= tolerance
}

fn audit_guard_access(
    plan: &BuildingPlan,
    defense: &crate::GateDefense,
    wall: &crate::CurtainWallRun,
    access_walk: Option<WallWalk>,
    issues: &mut Vec<AuditIssue>,
) {
    let chamber = &defense.guard_chamber;
    let access = &chamber.access;
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let inward = -direction_vector(wall.outward);
    let outward = -inward;
    let along_size = |size: Vec2| size.dot(tangent.abs());
    let depth_size = |size: Vec2| size.dot(inward.abs());
    let chamber_half_depth = chamber.size.dot(inward.abs()) * 0.5;
    let top_rect = axis_rect(access.top_landing.centre, access.top_landing.size * 0.5);
    let bottom_rect = axis_rect(
        access.bottom_landing.centre,
        access.bottom_landing.size * 0.5,
    );
    let chamber_rect = axis_rect(chamber.centre, chamber.size * 0.5);
    let walk_rect = access_walk
        .map(linear_walk_bounds)
        .map(|(min, max)| Rect2 { min, max });
    let landing_gate = access.envelope.width_metres >= 0.9
        && access.envelope.height_metres >= 1.9
        && along_size(access.top_landing.size) + 0.001 >= access.envelope.width_metres
        && depth_size(access.top_landing.size) + 0.001 >= access.envelope.width_metres
        && along_size(access.bottom_landing.size) + 0.001 >= access.envelope.width_metres
        && depth_size(access.bottom_landing.size) + 0.001 >= access.envelope.width_metres
        && walk_rect.is_some_and(|walk| rects_overlap_positive(top_rect, walk, 0.02))
        && rects_overlap_positive(bottom_rect, chamber_rect, 0.02)
        && access_walk
            .is_some_and(|walk| close(walk_elevation(walk), access.top_landing.elevation_metres))
        && close(
            access.bottom_landing.elevation_metres,
            chamber.floor_elevation_metres,
        )
        && point_in_rect(
            access.flight.top,
            access.top_landing.centre,
            access.top_landing.size,
        )
        && point_in_rect(
            access.flight.bottom,
            access.bottom_landing.centre,
            access.bottom_landing.size,
        );
    if !landing_gate {
        issues.push(issue(
            "inaccessible_guard_chamber",
            "guard access lacks positive-overlap, full-depth top/bottom landings".to_owned(),
        ));
    }

    let run = (access.flight.bottom - access.flight.top).length();
    let rise = access.flight.top_elevation_metres - access.flight.bottom_elevation_metres;
    let riser = rise / f32::from(access.flight.riser_count.max(1));
    let expected_run = access.flight.going_metres * f32::from(access.flight.riser_count);
    let pitch = (riser / access.flight.going_metres.max(0.001))
        .atan()
        .to_degrees();
    if access.flight.riser_count == 0
        || (run - expected_run).abs() > 0.03
        || !(0.12..=0.19).contains(&riser)
        || !(0.25..=0.34).contains(&access.flight.going_metres)
        || pitch > 38.0
        || !(0.0..=0.05).contains(&access.flight.nosing_metres)
    {
        issues.push(issue(
            "inaccessible_guard_chamber",
            format!("guard access stair has {riser:.3} m risers, {:.3} m going and {pitch:.1} degree pitch", access.flight.going_metres),
        ));
    }

    let door = access.door;
    let swing_centre = door.position + outward * (door.width_metres * 0.5);
    let swing_rect = oriented_rect(
        swing_centre,
        tangent,
        outward,
        door.width_metres * 0.5,
        door.width_metres * 0.5,
    );
    if !close(
        door.threshold_elevation_metres,
        chamber.floor_elevation_metres,
    ) || door.width_metres + 0.001 < access.envelope.width_metres
        || door.clear_height_metres + 0.001 < access.envelope.height_metres
        || door.threshold_elevation_metres + door.clear_height_metres
            > chamber.floor_elevation_metres + chamber.clear_height_metres + 0.01
        || !door.swing_inward
        || door.facing != wall.outward.opposite()
        || !point_on_rect_boundary(door.position, chamber.centre, chamber.size, 0.02)
        || !point_in_rect(swing_centre, chamber.centre, chamber.size)
        || !rects_overlap_positive(bottom_rect, swing_rect, 0.02)
    {
        issues.push(issue(
            "inaccessible_guard_chamber",
            "guard chamber rear door lacks a floor-level threshold, full opening, or clear swing"
                .to_owned(),
        ));
    }

    let top_opening = access.top_walk_opening;
    let cut = access.roof_clearance_opening;
    let cut_rect = axis_rect(cut.centre, cut.size * 0.5);
    let wall_centre = (wall.start + wall.end) * 0.5;
    let route_along = (access.top_landing.centre - wall_centre).dot(tangent);
    let route_depth = (access.top_landing.centre - wall_centre).dot(inward)
        + depth_size(access.top_landing.size) * 0.5;
    let top_route_rect = oriented_rect(
        wall_centre + tangent * route_along + inward * (route_depth * 0.5),
        tangent,
        inward,
        access.envelope.width_metres * 0.5,
        route_depth * 0.5,
    );
    if top_opening.width_metres + 0.001 < access.envelope.width_metres
        || top_opening.clear_height_metres + 0.001 < access.envelope.height_metres
        || !close(
            top_opening.threshold_elevation_metres,
            access.top_landing.elevation_metres,
        )
        || !point_on_rect_boundary(top_opening.position, chamber.centre, chamber.size, 0.02)
        || !rect_contains(cut_rect, top_route_rect)
        || !close(
            cut.elevation_metres,
            chamber.floor_elevation_metres + chamber.clear_height_metres,
        )
    {
        issues.push(issue(
            "inaccessible_guard_chamber",
            "wall-walk exit lacks its threshold-based opening or swept roof-clearance cut"
                .to_owned(),
        ));
    }

    let flight_direction = (access.flight.bottom - access.flight.top).normalize_or_zero();
    let flight_rect = oriented_rect(
        (access.flight.top + access.flight.bottom) * 0.5,
        flight_direction,
        Vec2::new(-flight_direction.y, flight_direction.x),
        run * 0.5,
        access.envelope.width_metres * 0.5,
    );
    let protected = [
        access.top_landing.centre,
        access.bottom_landing.centre,
        access.flight.top,
        access.flight.bottom,
    ]
    .into_iter()
    .all(|point| (point - chamber.centre).dot(inward) >= chamber_half_depth - 0.01);
    if !protected || rects_overlap(flight_rect, chamber_rect) {
        issues.push(issue(
            "inaccessible_guard_chamber",
            "guard stair is not wholly on the protected exterior side".to_owned(),
        ));
    }

    let hole = chamber
        .openings
        .iter()
        .find(|opening| opening.kind == crate::GuardOpeningKind::DownwardDefense);
    let hole_collision = hole.is_some_and(|opening| {
        let rect = axis_rect(opening.position, Vec2::splat(opening.width_metres * 0.5));
        rects_overlap(rect, top_rect)
            || rects_overlap(rect, bottom_rect)
            || rects_overlap(rect, flight_rect)
            || rects_overlap(rect, swing_rect)
    });
    let windlass_collision = chamber.operating_positions.iter().any(|position| {
        let rect = oriented_rect(position.position, tangent, inward, 0.75, 0.55);
        rects_overlap(rect, top_rect)
            || rects_overlap(rect, bottom_rect)
            || rects_overlap(rect, flight_rect)
            || rects_overlap(rect, swing_rect)
    });
    let tower_collision = plan.towers.iter().any(|tower| {
        [top_rect, bottom_rect, flight_rect]
            .into_iter()
            .any(|rect| circle_overlaps_rect(tower.centre_metres(), tower.radius_metres(), rect))
    });
    let traversal_rects = [top_rect, bottom_rect, flight_rect];
    let closure_collision = defense.closures.iter().any(|closure| {
        let closure_rect = oriented_rect(
            defense.threshold + inward * closure.inward_offset_metres,
            tangent,
            inward,
            closure.coverage.width_metres * 0.5,
            0.12,
        );
        traversal_rects
            .into_iter()
            .any(|route| rects_overlap(route, closure_rect))
            || door_swing_intersects_rect(door, tangent, outward, closure_rect)
    });
    let aperture_or_sightline_collision = defense.firing_positions.iter().any(|position| {
        traversal_rects.into_iter().any(|route| {
            circle_overlaps_rect(position.origin, position.aperture_width_metres * 0.5, route)
                || [defense.threshold, defense.approach]
                    .into_iter()
                    .any(|target| segment_intersects_rect(position.origin, target, route))
        })
    });
    if hole_collision
        || windlass_collision
        || tower_collision
        || closure_collision
        || aperture_or_sightline_collision
    {
        issues.push(issue(
            "inaccessible_guard_chamber",
            format!(
                "guard access obstruction: murder_hole={hole_collision}, windlass={windlass_collision}, tower={tower_collision}, closure={closure_collision}, aperture_or_sightline={aperture_or_sightline_collision}"
            ),
        ));
    }

    let arch_top = match chamber.load_path {
        crate::GatehouseLoadPath::BondedTowerBearing {
            arch_spring_elevation_metres,
            arch_ring_depth,
            arch_rise,
            ..
        } => arch_spring_elevation_metres + arch_ring_depth.metres() + arch_rise.metres(),
    };
    let support_near = |point: Vec2, elevation: f32| {
        access.support_posts.iter().any(|support| {
            (support.centre - point).length() <= 0.65
                && support.base_elevation_metres <= 0.01
                && (support.top_elevation_metres - elevation).abs() <= 0.15
        })
    };
    let upper_third = access.flight.top.lerp(access.flight.bottom, 0.33);
    let lower_third = access.flight.top.lerp(access.flight.bottom, 0.67);
    let landing_along = along_size(access.top_landing.size) * 0.5;
    let landing_depth = depth_size(access.top_landing.size) * 0.5;
    let expected_guards = [
        (
            access.top_landing.centre - tangent * landing_along + inward * landing_depth,
            access.top_landing.centre + tangent * landing_along + inward * landing_depth,
            access.top_landing.elevation_metres,
        ),
        (
            access.top_landing.centre - tangent * landing_along - inward * landing_depth,
            access.top_landing.centre - tangent * landing_along + inward * landing_depth,
            access.top_landing.elevation_metres,
        ),
        (
            access.bottom_landing.centre - tangent * landing_along + inward * landing_depth,
            access.bottom_landing.centre + tangent * landing_along + inward * landing_depth,
            access.bottom_landing.elevation_metres,
        ),
        (
            access.bottom_landing.centre + tangent * landing_along - inward * landing_depth,
            access.bottom_landing.centre + tangent * landing_along + inward * landing_depth,
            access.bottom_landing.elevation_metres,
        ),
    ];
    let guards_match = access.landing_guards.len() == expected_guards.len()
        && expected_guards.into_iter().all(|(start, end, elevation)| {
            access.landing_guards.iter().any(|guard| {
                let endpoints_match = ((guard.start - start).length() <= 0.02
                    && (guard.end - end).length() <= 0.02)
                    || ((guard.start - end).length() <= 0.02
                        && (guard.end - start).length() <= 0.02);
                endpoints_match
                    && close(guard.elevation_metres, elevation)
                    && guard.height_metres >= 0.9
            })
        });
    if door.threshold_elevation_metres + 0.001 < arch_top
        || access.flight_guard_height_metres < 0.9
        || !guards_match
        || access.support_posts.len() < 4
        || access.support_posts.iter().any(|support| {
            support.base_elevation_metres > 0.01
                || support.top_elevation_metres > access.top_landing.elevation_metres + 0.01
        })
        || !support_near(
            access.top_landing.centre,
            access.top_landing.elevation_metres,
        )
        || !support_near(
            access.bottom_landing.centre,
            access.bottom_landing.elevation_metres,
        )
        || !support_near(
            upper_third,
            access.flight.top_elevation_metres
                + (access.flight.bottom_elevation_metres - access.flight.top_elevation_metres)
                    * 0.33,
        )
        || !support_near(
            lower_third,
            access.flight.top_elevation_metres
                + (access.flight.bottom_elevation_metres - access.flight.top_elevation_metres)
                    * 0.67,
        )
    {
        issues.push(issue(
            "unsupported_guard_access",
            "guard stair lacks bearing clearance, continuous edge guards, or a declared support path".to_owned(),
        ));
    }

    let ledger = access.wall_ledger;
    let ledger_rect = axis_rect(ledger.centre, ledger.size * 0.5);
    let rear_wall_point = chamber.centre + inward * chamber_half_depth;
    let rear_wall_probe = oriented_rect(
        rear_wall_point,
        tangent,
        inward,
        along_size(chamber.size) * 0.5,
        0.02,
    );
    let transverse_braces = access
        .lateral_braces
        .iter()
        .filter(|brace| (brace.end - brace.start).dot(inward).abs() >= 0.7)
        .count();
    let longitudinal_braces = access
        .lateral_braces
        .iter()
        .filter(|brace| (brace.end - brace.start).dot(tangent).abs() >= 2.0)
        .count();
    let endpoint_on_structure = |point: Vec2, elevation: f32| {
        let on_post = access.support_posts.iter().any(|support| {
            let tolerance = support.size.max_element() * 0.5 + 0.12;
            (support.centre - point).length() <= tolerance
                && elevation >= support.base_elevation_metres - 0.08
                && elevation <= support.top_elevation_metres + 0.08
        });
        let on_ledger = point_in_rect(point, ledger.centre, ledger.size + Vec2::splat(0.16))
            && (elevation - ledger.elevation_metres).abs() <= ledger.height_metres * 0.5 + 0.08;
        let on_landing = [access.top_landing, access.bottom_landing]
            .into_iter()
            .any(|landing| {
                point_in_rect(point, landing.centre, landing.size + Vec2::splat(0.16))
                    && (elevation - landing.elevation_metres).abs() <= 0.16
            });
        let endpoint = Vec3::new(point.x, elevation, point.y);
        let on_stringer = [-1.0, 1.0].into_iter().any(|sign| {
            let offset = inward * sign * access.envelope.width_metres * 0.38;
            let start = Vec3::new(
                access.flight.top.x + offset.x,
                access.flight.top_elevation_metres - 0.12,
                access.flight.top.y + offset.y,
            );
            let end = Vec3::new(
                access.flight.bottom.x + offset.x,
                access.flight.bottom_elevation_metres - 0.12,
                access.flight.bottom.y + offset.y,
            );
            point_segment_distance(endpoint, start, end) <= 0.2
        });
        on_post || on_ledger || on_landing || on_stringer
    };
    let braces_connect = access.lateral_braces.iter().all(|brace| {
        brace.thickness_metres >= 0.14
            && (brace.start_elevation_metres - brace.end_elevation_metres).abs() > 0.2
            && endpoint_on_structure(brace.start, brace.start_elevation_metres)
            && endpoint_on_structure(brace.end, brace.end_elevation_metres)
    });
    if ledger.height_metres < 0.25
        || !rects_overlap_positive(ledger_rect, rear_wall_probe, 0.01)
        || along_size(ledger.size) + 0.01 < 4.0
        || transverse_braces < 4
        || longitudinal_braces < 2
        || !braces_connect
    {
        issues.push(issue(
            "unsupported_guard_access",
            "guard access lacks a masonry ledger and transverse/longitudinal lateral bracing"
                .to_owned(),
        ));
    }
}

fn rects_overlap_positive(a: Rect2, b: Rect2, minimum: f32) -> bool {
    (a.max.x.min(b.max.x) - a.min.x.max(b.min.x)) > minimum
        && (a.max.y.min(b.max.y) - a.min.y.max(b.min.y)) > minimum
}

fn segment_intersects_rect(start: Vec2, end: Vec2, rect: Rect2) -> bool {
    let delta = end - start;
    let mut t_min: f32 = 0.0;
    let mut t_max: f32 = 1.0;
    for (origin, direction, minimum, maximum) in [
        (start.x, delta.x, rect.min.x, rect.max.x),
        (start.y, delta.y, rect.min.y, rect.max.y),
    ] {
        if direction.abs() <= 1.0e-6 {
            if origin < minimum || origin > maximum {
                return false;
            }
            continue;
        }
        let inverse = direction.recip();
        let near = (minimum - origin) * inverse;
        let far = (maximum - origin) * inverse;
        t_min = t_min.max(near.min(far));
        t_max = t_max.min(near.max(far));
        if t_min > t_max {
            return false;
        }
    }
    true
}

fn point_segment_distance(point: Vec3, start: Vec3, end: Vec3) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= 1.0e-6 {
        return point.distance(start);
    }
    let progress = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * progress)
}

fn door_swing_intersects_rect(
    door: crate::AccessDoor,
    tangent: Vec2,
    outward: Vec2,
    rect: Rect2,
) -> bool {
    // The protected service door hinges at the jamb away from the gate axis
    // and folds against the chamber side. Sampling the moving leaf avoids the
    // false positives of treating the entire square around the doorway as
    // occupied while still auditing the swept quarter-circle.
    let hinge = door.position + tangent * (door.width_metres * 0.5);
    (0..=16).any(|sample| {
        let angle = std::f32::consts::FRAC_PI_2 * sample as f32 / 16.0;
        let end = hinge - tangent * (door.width_metres * angle.cos())
            + outward * (door.width_metres * angle.sin());
        segment_intersects_rect(hinge, end, rect)
    })
}

fn rect_contains(outer: Rect2, inner: Rect2) -> bool {
    outer.min.x <= inner.min.x + 0.01
        && outer.min.y <= inner.min.y + 0.01
        && outer.max.x >= inner.max.x - 0.01
        && outer.max.y >= inner.max.y - 0.01
}

fn point_in_rect(point: Vec2, centre: Vec2, size: Vec2) -> bool {
    let half = size * 0.5;
    point.x >= centre.x - half.x
        && point.x <= centre.x + half.x
        && point.y >= centre.y - half.y
        && point.y <= centre.y + half.y
}

fn firing_sector_covers(position: &crate::FiringPosition, target: Vec2) -> bool {
    let to_target = target - position.origin;
    let distance = to_target.length();
    distance <= position.range_metres
        && distance > 0.01
        && position
            .direction
            .normalize_or_zero()
            .dot(to_target / distance)
            >= position.half_arc_degrees.to_radians().cos()
}

fn firing_origin_matches_aperture(plan: &BuildingPlan, position: &crate::FiringPosition) -> bool {
    let Some(tower) = plan.towers.get(position.tower_index) else {
        return false;
    };
    let radial = position.origin - tower.centre_metres();
    (radial.length() - tower.radius_metres()).abs() <= 0.05
        && radial
            .normalize_or_zero()
            .dot(position.aperture_normal.normalize_or_zero())
            >= 0.98
        && position
            .direction
            .normalize_or_zero()
            .dot(position.aperture_normal.normalize_or_zero())
            >= position.half_arc_degrees.to_radians().cos()
}

fn ray_clear_of_solids(
    plan: &BuildingPlan,
    position: &crate::FiringPosition,
    target: Vec2,
    gate_wall_index: usize,
) -> bool {
    let start = Vec3::new(
        position.origin.x,
        position.elevation_metres,
        position.origin.y,
    );
    let end = Vec3::new(target.x, 1.2, target.y);
    for (index, wall) in plan.curtain_walls.iter().enumerate() {
        if index != gate_wall_index
            && segment_hits_run_prism(
                start,
                end,
                wall.start,
                wall.end,
                wall.thickness_metres,
                0.0,
                wall.height_metres,
            )
        {
            return false;
        }
    }
    for (index, tower) in plan.towers.iter().enumerate() {
        if index != position.tower_index
            && segment_hits_vertical_cylinder(
                start,
                end,
                tower.centre_metres(),
                tower.radius_metres(),
                tower.wall_height_metres,
            )
        {
            return false;
        }
    }
    for storey in &plan.storeys {
        let low = f32::from(storey.level) * plan.storey_height_metres;
        let high = low + plan.storey_height_metres;
        for wall in &storey.walls {
            let centre = wall.centre();
            let (half_x, half_z) = if wall.is_horizontal() {
                (crate::CELL_SIZE_METRES * 0.5, WALL_THICKNESS_METRES * 0.5)
            } else {
                (WALL_THICKNESS_METRES * 0.5, crate::CELL_SIZE_METRES * 0.5)
            };
            if segment_hits_aabb(
                start,
                end,
                Vec3::new(centre.x - half_x, low, centre.y - half_z),
                Vec3::new(centre.x + half_x, high, centre.y + half_z),
            ) {
                return false;
            }
        }
    }
    for defense in &plan.gate_defenses {
        let chamber = &defense.guard_chamber;
        for support in &chamber.supports {
            let half = support.size * 0.5;
            if segment_hits_aabb(
                start,
                end,
                Vec3::new(
                    support.centre.x - half.x,
                    support.base_elevation_metres,
                    support.centre.y - half.y,
                ),
                Vec3::new(
                    support.centre.x + half.x,
                    support.top_elevation_metres,
                    support.centre.y + half.y,
                ),
            ) {
                return false;
            }
        }
        let half = chamber.size * 0.5;
        if segment_hits_aabb(
            start,
            end,
            Vec3::new(
                chamber.centre.x - half.x,
                chamber.floor_elevation_metres,
                chamber.centre.y - half.y,
            ),
            Vec3::new(
                chamber.centre.x + half.x,
                chamber.floor_elevation_metres + chamber.clear_height_metres + 0.2,
                chamber.centre.y + half.y,
            ),
        ) {
            return false;
        }
        let Some(gate_wall) = plan.curtain_walls.get(defense.curtain_wall_index) else {
            continue;
        };
        let tangent = (gate_wall.end - gate_wall.start).normalize_or_zero();
        let inward = -direction_vector(gate_wall.outward);
        let gate_width = gate_wall.gate_width_metres.unwrap_or(0.0);
        for closure in &defense.closures {
            let centre = defense.threshold + inward * closure.inward_offset_metres;
            let half_x = tangent.x.abs() * gate_width * 0.5 + inward.x.abs() * 0.05;
            let half_z = tangent.y.abs() * gate_width * 0.5 + inward.y.abs() * 0.05;
            if segment_hits_aabb(
                start,
                end,
                Vec3::new(centre.x - half_x, 0.0, centre.y - half_z),
                Vec3::new(
                    centre.x + half_x,
                    closure.coverage.crown_height(),
                    centre.y + half_z,
                ),
            ) {
                return false;
            }
        }
    }
    for roof in &plan.roofs {
        let half = roof.size * 0.5 + Vec2::splat(roof.eave_metres);
        let span = match roof.ridge_axis {
            crate::RidgeAxis::X => half.y,
            crate::RidgeAxis::Z => half.x,
        };
        let peak = roof.base_height_metres + span * roof.pitch_degrees.to_radians().tan();
        if segment_hits_aabb(
            start,
            end,
            Vec3::new(
                roof.centre.x - half.x,
                roof.base_height_metres,
                roof.centre.y - half.y,
            ),
            Vec3::new(roof.centre.x + half.x, peak, roof.centre.y + half.y),
        ) {
            return false;
        }
    }
    for walk in &plan.wall_walks {
        match *walk {
            WallWalk::Linear {
                start: run_start,
                end: run_end,
                elevation_metres,
                width_metres,
                outward,
            } => {
                let inward = -direction_vector(outward) * width_metres;
                let min = run_start
                    .min(run_end)
                    .min(run_start + inward)
                    .min(run_end + inward);
                let max = run_start
                    .max(run_end)
                    .max(run_start + inward)
                    .max(run_end + inward);
                if segment_hits_aabb(
                    start,
                    end,
                    Vec3::new(min.x, elevation_metres - 0.16, min.y),
                    Vec3::new(max.x, elevation_metres + 0.04, max.y),
                ) {
                    return false;
                }
            }
            WallWalk::Round {
                centre,
                elevation_metres,
                outer_radius_metres,
                ..
            } => {
                if segment_hits_aabb(
                    start,
                    end,
                    Vec3::new(
                        centre.x - outer_radius_metres,
                        elevation_metres - 0.16,
                        centre.y - outer_radius_metres,
                    ),
                    Vec3::new(
                        centre.x + outer_radius_metres,
                        elevation_metres + 0.04,
                        centre.y + outer_radius_metres,
                    ),
                ) {
                    return false;
                }
            }
            WallWalk::RectangularDeck {
                centre,
                size,
                elevation_metres,
                ..
            } => {
                let half = size * 0.5;
                if segment_hits_aabb(
                    start,
                    end,
                    Vec3::new(
                        centre.x - half.x,
                        elevation_metres - 0.16,
                        centre.y - half.y,
                    ),
                    Vec3::new(
                        centre.x + half.x,
                        elevation_metres + 0.04,
                        centre.y + half.y,
                    ),
                ) {
                    return false;
                }
            }
        }
    }
    true
}

fn segment_hits_run_prism(
    start: Vec3,
    end: Vec3,
    run_start: Vec2,
    run_end: Vec2,
    thickness: f32,
    low: f32,
    high: f32,
) -> bool {
    let half = Vec2::splat(thickness * 0.5);
    let min = run_start.min(run_end) - half;
    let max = run_start.max(run_end) + half;
    segment_hits_aabb(
        start,
        end,
        Vec3::new(min.x, low, min.y),
        Vec3::new(max.x, high, max.y),
    )
}

fn segment_hits_vertical_cylinder(
    start: Vec3,
    end: Vec3,
    centre: Vec2,
    radius: f32,
    height: f32,
) -> bool {
    let a = Vec2::new(start.x, start.z);
    let b = Vec2::new(end.x, end.z);
    let delta = b - a;
    let t = ((centre - a).dot(delta) / delta.length_squared().max(0.0001)).clamp(0.001, 0.999);
    let elevation = start.y + (end.y - start.y) * t;
    (a + delta * t - centre).length() < radius && elevation > 0.0 && elevation < height
}

fn segment_hits_aabb(start: Vec3, end: Vec3, min: Vec3, max: Vec3) -> bool {
    let delta = end - start;
    let mut t_min: f32 = 0.001;
    let mut t_max: f32 = 0.999;
    for axis in 0..3 {
        let origin = start[axis];
        let direction = delta[axis];
        if direction.abs() < 0.0001 {
            if origin < min[axis] || origin > max[axis] {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / direction;
        let mut near = (min[axis] - origin) * inverse;
        let mut far = (max[axis] - origin) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t_min = t_min.max(near);
        t_max = t_max.min(far);
        if t_min > t_max {
            return false;
        }
    }
    true
}

fn audit_defensive_circuit(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if plan.wall_walks.is_empty() {
        return;
    }
    let mut graph = vec![Vec::new(); plan.wall_walks.len()];
    for (index, junction) in plan.defensive_junctions.iter().enumerate() {
        let Some(a) = plan.wall_walks.get(junction.walk_a).copied() else {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("defensive junction {index} references a missing walk"),
            ));
            continue;
        };
        let Some(b) = plan.wall_walks.get(junction.walk_b).copied() else {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("defensive junction {index} references a missing walk"),
            ));
            continue;
        };
        let delta = (walk_elevation(a) - walk_elevation(b)).abs();
        match junction.kind {
            DefensiveJunctionKind::LevelLanding if delta > 0.2 => issues.push(issue(
                "wall_walk_vertical_discontinuity",
                format!("level junction {index} bridges a {delta:.2} m height difference"),
            )),
            DefensiveJunctionKind::Steps { riser_count }
                if riser_count == 0 || delta / f32::from(riser_count) > 0.20 =>
            {
                issues.push(issue(
                    "wall_walk_vertical_discontinuity",
                    format!("stepped junction {index} has unusable risers"),
                ));
            }
            _ => {}
        }
        if junction.width_metres < 0.9 || junction.clear_height_metres < 1.9 {
            issues.push(issue(
                "insufficient_walk_clearance",
                format!(
                    "defensive junction {index} has {:.2} m width and {:.2} m headroom",
                    junction.width_metres, junction.clear_height_metres
                ),
            ));
        }
        if !junction_has_physical_connection(plan, junction, a, b) {
            issues.push(issue(
                "missing_tower_portal",
                format!("defensive junction {index} is not backed by a physical portal/landing"),
            ));
            continue;
        }
        graph[junction.walk_a].push(junction.walk_b);
        graph[junction.walk_b].push(junction.walk_a);
    }

    let mut assignments = vec![0_u8; plan.wall_walks.len()];
    for circuit in &plan.defensive_circuits {
        audit_one_defensive_circuit(plan, circuit, &graph, &mut assignments, issues);
    }
    for (index, count) in assignments.into_iter().enumerate() {
        if count != 1 {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("wall walk {index} belongs to {count} declared circuits instead of one"),
            ));
        }
    }
    for (tower_index, tower) in plan.towers.iter().enumerate() {
        if !plan.tower_portals.iter().any(|portal| {
            portal.tower_index == tower_index
                && portal.kind == TowerPortalKind::GroundStairEntrance
                && portal.width_metres >= 0.9
                && portal.clear_height_metres >= 1.9
                && portal.sill_elevation_metres <= 0.2
        }) {
            issues.push(issue(
                "missing_tower_portal",
                format!("tower {tower_index} has no usable ground stair entrance"),
            ));
        }
        if tower.radius_metres() - tower.wall_thickness_metres < 0.9 {
            issues.push(issue(
                "insufficient_walk_clearance",
                format!("tower {tower_index} has less than 0.90 m interior radius"),
            ));
        }
    }
}

fn junction_has_physical_connection(
    plan: &BuildingPlan,
    junction: &DefensiveJunction,
    a: WallWalk,
    b: WallWalk,
) -> bool {
    let (linear_index, round) = match (a, b) {
        (WallWalk::Linear { .. }, round @ WallWalk::Round { .. }) => (junction.walk_a, round),
        (round @ WallWalk::Round { .. }, WallWalk::Linear { .. }) => (junction.walk_b, round),
        _ => return true,
    };
    let WallWalk::Round { centre, .. } = round else {
        unreachable!()
    };
    let Some(tower_index) = plan
        .towers
        .iter()
        .position(|tower| close_vec(tower.centre_metres(), centre))
    else {
        return false;
    };
    plan.tower_portals.iter().any(|portal| {
        portal.tower_index == tower_index
            && portal.kind
                == (TowerPortalKind::WallWalkJunction {
                    walk_index: linear_index,
                })
            && portal.width_metres >= junction.width_metres
            && portal.clear_height_metres >= junction.clear_height_metres
    })
}

fn audit_one_defensive_circuit(
    plan: &BuildingPlan,
    circuit: &DefensiveCircuit,
    graph: &[Vec<usize>],
    assignments: &mut [u8],
    issues: &mut Vec<AuditIssue>,
) {
    let mut member = vec![false; plan.wall_walks.len()];
    for &index in &circuit.walks {
        let Some(assignment) = assignments.get_mut(index) else {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("{} references missing wall walk {index}", circuit.label),
            ));
            continue;
        };
        *assignment = assignment.saturating_add(1);
        member[index] = true;
    }
    let Some(seed) = circuit.walks.iter().copied().find(|&index| {
        plan.wall_walks
            .get(index)
            .is_some_and(|walk| walk_has_stair_access(plan, *walk))
    }) else {
        issues.push(issue(
            "disconnected_defensive_circuit",
            format!("{} has no interior stair access", circuit.label),
        ));
        return;
    };
    let mut reachable = vec![false; plan.wall_walks.len()];
    let mut queue = VecDeque::from([seed]);
    reachable[seed] = true;
    while let Some(current) = queue.pop_front() {
        for &next in &graph[current] {
            if member[next] && !reachable[next] {
                reachable[next] = true;
                queue.push_back(next);
            }
        }
    }
    for &index in &circuit.walks {
        if index < reachable.len() && !reachable[index] {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("wall walk {index} is disconnected within {}", circuit.label),
            ));
        }
    }
}

fn walk_has_stair_access(plan: &BuildingPlan, walk: WallWalk) -> bool {
    match walk {
        WallWalk::Round {
            centre,
            elevation_metres,
            ..
        } => plan.stairs.iter().any(|stair| {
            matches!(
                stair,
                Stair::Spiral { centre: stair_centre, base_height_metres, rise_metres, .. }
                    if close_vec(*stair_centre, centre)
                        && close(*base_height_metres + *rise_metres, elevation_metres)
            )
        }),
        WallWalk::RectangularDeck {
            stairwell_centre,
            elevation_metres,
            ..
        } => plan.stairs.iter().any(|stair| {
            matches!(
                stair,
                Stair::Spiral { centre, base_height_metres, rise_metres, .. }
                    if close_vec(*centre, stairwell_centre)
                        && close(*base_height_metres + *rise_metres, elevation_metres)
            )
        }),
        WallWalk::Linear { .. } => false,
    }
}

fn audit_walk_roof_clearance(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for (walk_index, walk) in plan.wall_walks.iter().copied().enumerate() {
        let WallWalk::Linear { .. } = walk else {
            continue;
        };
        let walk_bounds = linear_walk_bounds(walk);
        for (roof_index, roof) in plan.roofs.iter().copied().enumerate() {
            if bounds_overlap(walk_bounds, roof_bounds(roof))
                && roof.base_height_metres < walk_elevation(walk) + 1.9
            {
                issues.push(issue(
                    "wall_walk_roof_obstruction",
                    format!("roof {roof_index} obstructs headroom over wall walk {walk_index}"),
                ));
            }
        }
    }
}

fn walk_elevation(walk: WallWalk) -> f32 {
    match walk {
        WallWalk::Linear {
            elevation_metres, ..
        }
        | WallWalk::Round {
            elevation_metres, ..
        }
        | WallWalk::RectangularDeck {
            elevation_metres, ..
        } => elevation_metres,
    }
}

fn linear_walk_bounds(walk: WallWalk) -> (Vec2, Vec2) {
    let WallWalk::Linear {
        start,
        end,
        width_metres,
        outward,
        ..
    } = walk
    else {
        unreachable!()
    };
    let inward = -direction_vector(outward) * width_metres;
    let opposite_start = start + inward;
    let opposite_end = end + inward;
    (
        start.min(end).min(opposite_start).min(opposite_end),
        start.max(end).max(opposite_start).max(opposite_end),
    )
}

fn roof_bounds(roof: RoofPiece) -> (Vec2, Vec2) {
    let half = roof.size * 0.5 + Vec2::splat(roof.eave_metres);
    (roof.centre - half, roof.centre + half)
}

fn bounds_overlap(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> bool {
    a.0.x < b.1.x && a.1.x > b.0.x && a.0.y < b.1.y && a.1.y > b.0.y
}

fn run_supported(plan: &BuildingPlan, start: Vec2, end: Vec2, elevation: f32) -> bool {
    if plan.curtain_walls.iter().any(|wall| {
        same_run(start, end, wall.start, wall.end) && close(elevation, wall.height_metres)
    }) {
        return true;
    }
    let dimensions = plan.dimensions_metres();
    let top = plan.storeys.len() as f32 * plan.storey_height_metres;
    close(elevation, top)
        && ((close(start.x, 0.0) && close(end.x, 0.0))
            || (close(start.x, dimensions.x) && close(end.x, dimensions.x))
            || (close(start.y, 0.0) && close(end.y, 0.0))
            || (close(start.y, dimensions.y) && close(end.y, dimensions.y)))
}

fn same_run(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> bool {
    (close_vec(a0, b0) && close_vec(a1, b1)) || (close_vec(a0, b1) && close_vec(a1, b0))
}

fn close_vec(a: Vec2, b: Vec2) -> bool {
    (a - b).length_squared() < 0.001
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn direction_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    }
}

fn issue(code: &'static str, message: String) -> AuditIssue {
    AuditIssue { code, message }
}

fn point_in_polygon_2d(polygon: &[Vec2], point: Vec2) -> bool {
    let mut inside = false;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}

fn roof_face_contains_plan_point(face: &crate::RoofFace, point: Vec2) -> bool {
    let outer = face
        .polygon
        .iter()
        .map(|vertex| Vec2::new(vertex.x, vertex.z))
        .collect::<Vec<_>>();
    point_in_polygon_2d(&outer, point)
        && !face.cutouts.iter().any(|cutout| {
            point_in_polygon_2d(
                &cutout
                    .iter()
                    .map(|vertex| Vec2::new(vertex.x, vertex.z))
                    .collect::<Vec<_>>(),
                point,
            )
        })
}

fn roof_face_height(face: &crate::RoofFace, point: Vec2) -> Option<f32> {
    (face.plane.normal.y.abs() > 0.000_1).then(|| {
        -(face.plane.normal.x * point.x + face.plane.normal.z * point.y + face.plane.constant)
            / face.plane.normal.y
    })
}

fn point_on_polygon_edge(polygon: &[Vec2], point: Vec2, tolerance: f32) -> bool {
    polygon.iter().enumerate().any(|(index, start)| {
        let end = polygon[(index + 1) % polygon.len()];
        let axis = end - *start;
        let length_squared = axis.length_squared();
        if length_squared <= f32::EPSILON {
            return point.distance(*start) <= tolerance;
        }
        let t = ((point - *start).dot(axis) / length_squared).clamp(0.0, 1.0);
        point.distance(*start + axis * t) <= tolerance
    })
}

fn roof_face_contains_plan_point_inclusive(face: &crate::RoofFace, point: Vec2) -> bool {
    let outer = face
        .polygon
        .iter()
        .map(|vertex| Vec2::new(vertex.x, vertex.z))
        .collect::<Vec<_>>();
    let inside_outer =
        point_in_polygon_2d(&outer, point) || point_on_polygon_edge(&outer, point, 0.01);
    inside_outer
        && !face.cutouts.iter().any(|cutout| {
            let polygon = cutout
                .iter()
                .map(|vertex| Vec2::new(vertex.x, vertex.z))
                .collect::<Vec<_>>();
            point_in_polygon_2d(&polygon, point) && !point_on_polygon_edge(&polygon, point, 0.01)
        })
}

fn timber_roof_envelope_intrusions(plan: &BuildingPlan) -> Vec<crate::TimberMemberId> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let roof_faces = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| &roof.faces)
        .collect::<Vec<_>>();
    let maximum_covering_thickness = roof_faces
        .iter()
        .map(|face| face.thickness_metres)
        .fold(0.0_f32, f32::max);

    frame
        .members
        .iter()
        .filter(|member| {
            member.phase == crate::TimberFramePhase::RoofConstruction
                && matches!(
                    member.role,
                    crate::TimberMemberRole::GableTie
                        | crate::TimberMemberRole::GablePost
                        | crate::TimberMemberRole::Rafter
                        | crate::TimberMemberRole::Collar
                        | crate::TimberMemberRole::Purlin
                )
        })
        .filter_map(|member| {
            let clearance =
                member.section_metres.max_element() * 0.75 + maximum_covering_thickness + 0.02;
            let intrudes = (0..=32).any(|sample| {
                let t = sample as f32 / 32.0;
                let point = member.start.lerp(member.end, t);
                let plan_point = Vec2::new(point.x, point.z);
                let roof_height = roof_faces
                    .iter()
                    .filter(|face| roof_face_contains_plan_point_inclusive(face, plan_point))
                    .filter_map(|face| roof_face_height(face, plan_point))
                    .fold(None, |highest: Option<f32>, height| {
                        Some(highest.map_or(height, |current| current.max(height)))
                    });
                roof_height.is_none_or(|height| point.y > height + clearance)
            });
            intrudes.then_some(member.id)
        })
        .collect()
}

fn exposed_roof_child_support_posts(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_roofs = plan
        .roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_some())
        .map(|roof| (roof.owner, roof.parent))
        .collect::<std::collections::HashMap<_, _>>();

    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| {
            child_roofs.get(&solid.owner).is_some_and(|parent_id| {
                let allowed_top = parent_id
                    .and_then(|id| plan.roof_assemblies.iter().find(|roof| roof.id == id))
                    .and_then(|parent| {
                        let point = Vec2::new(solid.centre.x, solid.centre.z);
                        parent
                            .faces
                            .iter()
                            .filter(|face| roof_face_contains_plan_point_inclusive(face, point))
                            .filter_map(|face| roof_face_height(face, point))
                            .max_by(f32::total_cmp)
                    })
                    .unwrap_or(f32::NEG_INFINITY);
                solid.centre.y + solid.size.y * 0.5 > allowed_top + 0.025
            }) && solid.role == SolidRole::RoofFraming
                && matches!(solid.shape, crate::ResolvedSolidShape::Cuboid)
                && solid.size.y > 0.35
                && solid.size.y > solid.size.x * 2.0
                && solid.size.y > solid.size.z * 2.0
                && solid.longfall_radians.abs() < 0.01
                && solid.crossfall_radians.abs() < 0.01
        })
        .map(|solid| solid.id)
        .collect()
}

fn oversized_child_roof_flashings(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_flashings = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| &roof.children)
        .filter(|child| child.kind != crate::RoofChildKind::Tower)
        .flat_map(|child| &child.flashing_ids)
        .copied()
        .collect::<std::collections::HashSet<_>>();

    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| {
            child_flashings.contains(&solid.id)
                && solid.role == SolidRole::RoofFlashing
                && (solid.size.y > 0.03 || solid.size.z > 0.12)
        })
        .map(|solid| solid.id)
        .collect()
}

fn invalid_attached_child_drainage(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_owners = plan
        .roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_some())
        .filter(|roof| {
            plan.roof_assemblies.iter().any(|parent| {
                parent.children.iter().any(|child| {
                    child.child == roof.id && child.kind != crate::RoofChildKind::Tower
                })
            })
        })
        .map(|roof| roof.owner)
        .collect::<std::collections::HashSet<_>>();

    plan.resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| child_owners.contains(&network.owner))
        .filter(|network| {
            plan.resolved_geometry
                .roof_drainage_outlets
                .iter()
                .find(|station| station.id == network.outlet_station)
                .is_none_or(|station| {
                    station.disposition != crate::RoofDrainageDisposition::FreeDripToParentRoof
                })
        })
        .map(|network| network.id)
        .collect()
}

fn invalid_dormer_trimmer_envelope(plan: &BuildingPlan) -> Vec<crate::TimberMemberId> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    frame
        .members
        .iter()
        .filter(|member| member.role == crate::TimberMemberRole::DormerTrimmer)
        .filter(|member| {
            !plan.roof_dormers.iter().enumerate().any(|(index, dormer)| {
                let outward = match dormer.facing {
                    crate::Direction::North => Vec2::Y,
                    crate::Direction::South => -Vec2::Y,
                    crate::Direction::East => Vec2::X,
                    crate::Direction::West => -Vec2::X,
                };
                let tangent = Vec2::new(outward.y, -outward.x);
                let half_width = dormer.width_metres
                    * if dormer.kind == crate::DormerKind::TransverseGable {
                        2.20
                    } else {
                        1.0
                    }
                    * 0.5;
                let roof_id = crate::RoofAssemblyId(1_000 + index as u64);
                let cut = plan
                    .roof_assemblies
                    .iter()
                    .flat_map(|roof| &roof.children)
                    .find(|child| child.child == roof_id)
                    .and_then(|child| {
                        plan.resolved_geometry
                            .voids
                            .iter()
                            .find(|void| void.id == child.parent_cut)
                    });
                let (rear, front) = cut.map_or((-dormer.depth_metres * 0.84, 0.0), |cut| {
                    [
                        Vec2::new(cut.bounds.min.x, cut.bounds.min.z),
                        Vec2::new(cut.bounds.min.x, cut.bounds.max.z),
                        Vec2::new(cut.bounds.max.x, cut.bounds.min.z),
                        Vec2::new(cut.bounds.max.x, cut.bounds.max.z),
                    ]
                    .map(|point| (point - dormer.centre).dot(outward))
                    .into_iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), depth| {
                        (min.min(depth), max.max(depth))
                    })
                });
                [member.start, member.end].iter().all(|point| {
                    let relative = Vec2::new(point.x, point.z) - dormer.centre;
                    let depth = relative.dot(outward);
                    depth >= rear - 0.025
                        && depth <= front + 0.025
                        && relative.dot(tangent).abs() <= half_width + 0.025
                })
            })
        })
        .map(|member| member.id)
        .collect()
}

fn oversized_attached_child_gutters(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_owners = plan
        .roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_some())
        .filter(|roof| matches!(roof.kind, crate::RoofKind::Gable | crate::RoofKind::Shed))
        .map(|roof| roof.owner)
        .collect::<std::collections::HashSet<_>>();
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let mut invalid = plan
        .resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| child_owners.contains(&network.owner))
        .flat_map(|network| {
            std::iter::once(network.channel_floor)
                .chain(network.channel_lips)
                .chain(network.collector_solids.iter().copied())
                .filter(|id| {
                    solids.get(id).is_none_or(|solid| {
                        if *id == network.channel_floor {
                            solid.size.y > 0.025 || solid.size.z > 0.10
                        } else if network.collector_solids.contains(id) {
                            solid.size.y > 0.025 || solid.size.z > 0.08
                        } else {
                            solid.size.y > 0.05 || solid.size.z > 0.025
                        }
                    })
                })
        })
        .collect::<Vec<_>>();
    for network in plan
        .resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| child_owners.contains(&network.owner))
    {
        let edge = plan
            .roof_assemblies
            .iter()
            .find(|roof| roof.owner == network.owner)
            .and_then(|roof| {
                roof.edges
                    .iter()
                    .find(|edge| edge.id == network.receiving_edge)
            });
        let outlet = plan
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == network.outlet_void);
        if edge.zip(outlet).is_none_or(|(edge, outlet)| {
            let point = Vec2::new(
                (outlet.bounds.min.x + outlet.bounds.max.x) * 0.5,
                (outlet.bounds.min.z + outlet.bounds.max.z) * 0.5,
            );
            let start = Vec2::new(edge.start.x, edge.start.z);
            let delta = Vec2::new(edge.end.x - edge.start.x, edge.end.z - edge.start.z);
            let along = ((point - start).dot(delta) / delta.length_squared().max(0.000_001))
                .clamp(0.0, 1.0);
            point.distance(start + delta * along) > 0.30
        }) {
            invalid.push(network.outlet_void);
        }
    }
    invalid.sort_unstable();
    invalid.dedup();
    invalid
}

fn unseated_gabled_dormer_roofs(plan: &BuildingPlan) -> Vec<crate::RoofAssemblyId> {
    plan.roof_assemblies
        .iter()
        .flat_map(|parent| parent.children.iter().map(move |child| (parent, child)))
        .filter(|(_, child)| child.kind == crate::RoofChildKind::GabledDormer)
        .filter_map(|(parent, link)| {
            let child = plan
                .roof_assemblies
                .iter()
                .find(|roof| roof.id == link.child)?;
            let base = child
                .faces
                .iter()
                .flat_map(|face| &face.polygon)
                .map(|point| point.y)
                .fold(f32::INFINITY, f32::min);
            let rear_points = child
                .edges
                .iter()
                .filter(|edge| edge.kind == crate::RoofEdgeKind::OpeningCut)
                .flat_map(|edge| [edge.start, edge.end])
                .filter(|point| (point.y - base).abs() <= 0.025)
                .collect::<Vec<_>>();
            let rear_is_seated = rear_points.len() >= 2
                && rear_points.iter().all(|point| {
                    let plan_point = Vec2::new(point.x, point.z);
                    parent
                        .faces
                        .iter()
                        .filter(|face| roof_face_contains_plan_point_inclusive(face, plan_point))
                        .filter_map(|face| roof_face_height(face, plan_point))
                        .max_by(f32::total_cmp)
                        // The derived seam is searched on a 1 cm project
                        // station grid; allow one sloped station of vertical
                        // rise while still rejecting a visible rear gap.
                        .is_some_and(|height| height >= point.y - 0.03 && height <= point.y + 0.06)
                });
            (!rear_is_seated).then_some(child.id)
        })
        .collect()
}

fn undeclared_timber_intersections(plan: &BuildingPlan) -> Vec<(ResolvedItemId, ResolvedItemId)> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let interfaces = plan
        .resolved_geometry
        .support_interfaces
        .iter()
        .map(|interface| (interface.id, interface))
        .collect::<std::collections::HashMap<_, _>>();
    let member_by_solid = frame
        .members
        .iter()
        .map(|member| (member.solid, member))
        .collect::<std::collections::HashMap<_, _>>();

    let overlap_inside_interface =
        |a: &crate::ResolvedSolid,
         b: &crate::ResolvedSolid,
         interface: &crate::SupportInterface| {
            let (a_min, a_max) = resolved_solid_bounds(a);
            let (b_min, b_max) = resolved_solid_bounds(b);
            let overlap_min = a_min.max(b_min);
            let overlap_max = a_max.min(b_max);
            overlap_min
                .cmpge(interface.bounds.min - Vec3::splat(0.012))
                .all()
                && overlap_max
                    .cmple(interface.bounds.max + Vec3::splat(0.012))
                    .all()
                && resolved_solid_overlaps_bounds(
                    a,
                    (interface.bounds.min, interface.bounds.max),
                    0.001,
                )
                && resolved_solid_overlaps_bounds(
                    b,
                    (interface.bounds.min, interface.bounds.max),
                    0.001,
                )
        };

    let mut failures = Vec::new();
    let mut checked = std::collections::HashSet::new();
    for member in &frame.members {
        let Some(a) = solids.get(&member.solid).copied() else {
            continue;
        };
        for b in &plan.resolved_geometry.solids {
            // Member-to-member construction is already governed by the exact
            // TimberFrameJoint participant/contact audit, including action and
            // reaction. This pass owns cross-authority intersections: timber
            // against walls, openings, roofs, drainage, and other assemblies.
            if a.id == b.id || member_by_solid.contains_key(&b.id) {
                continue;
            }
            let pair = if a.id < b.id {
                (a.id, b.id)
            } else {
                (b.id, a.id)
            };
            // Gefach prisms have a dedicated constructive polygon-difference
            // audit above; their AABBs intentionally span triangular empty
            // corners. Treating those AABBs as solid would manufacture
            // intersections with every diagonal brace.
            if matches!(a.shape, crate::ResolvedSolidShape::TimberPanelPrism { .. })
                || matches!(b.shape, crate::ResolvedSolidShape::TimberPanelPrism { .. })
            {
                continue;
            }
            let overlaps = if matches!(a.shape, crate::ResolvedSolidShape::Cuboid)
                && matches!(b.shape, crate::ResolvedSolidShape::Cuboid)
            {
                oriented_cuboids_overlap(a, b, 0.008)
            } else {
                resolved_shape_overlap(a, b, 0.008)
            };
            if !checked.insert(pair) || !overlaps {
                continue;
            }
            // Stage 3 masonry/reveal pieces and the exposed frame are a
            // deliberate composite only when both are bound to the exact same
            // wall assembly. A shared post may belong to the adjacent bay, so
            // bay.opening alone is too narrow; owner or role alone would be a
            // dangerously broad whitelist.
            let member_walls = frame
                .bays
                .iter()
                .filter(|bay| bay.member_ids.contains(&member.id))
                .filter_map(|bay| bay.wall)
                .collect::<std::collections::HashSet<_>>();
            let exact_opening_composite = plan.opening_assemblies.iter().any(|opening| {
                let same_or_adjacent_bay = member_walls.contains(&opening.host_wall)
                    || frame
                        .facades
                        .iter()
                        .flat_map(|facade| &facade.lines)
                        .any(|line| {
                            line.storeys.iter().any(|storey| {
                                let positions = storey
                                    .bay_ids
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(index, id)| {
                                        frame
                                            .bays
                                            .iter()
                                            .find(|bay| bay.id == *id)
                                            .map(|bay| (index, bay))
                                    })
                                    .collect::<Vec<_>>();
                                let opening_position = positions
                                    .iter()
                                    .find(|(_, bay)| bay.wall == Some(opening.host_wall))
                                    .map(|(index, _)| *index);
                                let member_position = positions
                                    .iter()
                                    .find(|(_, bay)| bay.member_ids.contains(&member.id))
                                    .map(|(index, _)| *index);
                                (storey.member_ids.contains(&member.id)
                                    || storey.jetty.as_ref().is_some_and(|jetty| {
                                        jetty.jetty_beams.contains(&member.id)
                                            || jetty.knaggen.contains(&member.id)
                                            || jetty.corner_supports.contains(&member.id)
                                    }))
                                    && opening_position.is_some()
                                    || opening_position
                                        .zip(member_position)
                                        .is_some_and(|(left, right)| left.abs_diff(right) <= 1)
                            })
                        });
                let local_recessed_composite = plan
                    .wall_assemblies
                    .iter()
                    .find(|wall| wall.id == opening.host_wall)
                    .is_some_and(|wall| {
                        let member_reaches_wall = [member.start, member.end, a.centre]
                            .into_iter()
                            .any(|point| {
                                let local = Vec2::new(point.x, point.z) - wall.frame.origin;
                                local.dot(wall.frame.tangent).abs()
                                    <= wall.length_metres * 0.5 + 0.25
                                    && local.dot(wall.frame.outward).abs() <= 0.55
                                    && point.y >= wall.base_elevation_metres - 0.20
                                    && point.y
                                        <= wall.base_elevation_metres + wall.height_metres + 0.20
                            });
                        wall.material == crate::WallMaterialClass::TimberInfill
                            && member_reaches_wall
                            && a.centre.y + a.size.y * 0.5 >= wall.base_elevation_metres - 0.20
                            && a.centre.y - a.size.y * 0.5
                                <= wall.base_elevation_metres + wall.height_metres + 0.20
                    });
                (same_or_adjacent_bay || local_recessed_composite)
                    && (opening.jamb_solids.contains(&b.id)
                        || opening.sill_solid == Some(b.id)
                        || opening.head_solid == b.id
                        || opening.spandrel_solid == b.id)
                    || plan
                        .wall_assemblies
                        .iter()
                        .find(|wall| wall.id == opening.host_wall)
                        .is_some_and(|wall| {
                            matches!(
                                member.role,
                                crate::TimberMemberRole::JettyBeam
                                    | crate::TimberMemberRole::GableTie
                                    | crate::TimberMemberRole::GablePost
                                    | crate::TimberMemberRole::Rafter
                                    | crate::TimberMemberRole::Collar
                                    | crate::TimberMemberRole::Purlin
                            ) && ((a.centre.y - wall.base_elevation_metres).abs() <= 0.20
                                || (a.centre.y - (wall.base_elevation_metres + wall.height_metres))
                                    .abs()
                                    <= 0.20)
                        })
                        && (opening.jamb_solids.contains(&b.id)
                            || opening.sill_solid == Some(b.id)
                            || opening.head_solid == b.id
                            || opening.spandrel_solid == b.id)
            });
            let exact_partition_join = plan.wall_assemblies.iter().any(|wall| {
                wall.frame.outside_room.is_some() && wall.host_solids.contains(&b.id) && {
                    let half = wall.length_metres * 0.5;
                    let endpoints = [
                        wall.frame.origin - wall.frame.tangent * half,
                        wall.frame.origin + wall.frame.tangent * half,
                    ];
                    let member_endpoints = [
                        Vec2::new(member.start.x, member.start.z),
                        Vec2::new(member.end.x, member.end.z),
                    ];
                    member_endpoints.iter().any(|point| {
                        endpoints
                            .iter()
                            .any(|endpoint| point.distance(*endpoint) <= 0.24)
                    }) || matches!(
                        member.role,
                        crate::TimberMemberRole::FloorJoist
                            | crate::TimberMemberRole::Girder
                            | crate::TimberMemberRole::JettyBeam
                            | crate::TimberMemberRole::GableTie
                            | crate::TimberMemberRole::GablePost
                            | crate::TimberMemberRole::Rafter
                            | crate::TimberMemberRole::Collar
                            | crate::TimberMemberRole::Purlin
                    ) && (a.centre.y - (wall.base_elevation_metres + wall.height_metres))
                        .abs()
                        <= 0.40
                        || matches!(
                            member.role,
                            crate::TimberMemberRole::FloorJoist
                                | crate::TimberMemberRole::Girder
                                | crate::TimberMemberRole::JettyBeam
                        ) && (a.centre.y - wall.base_elevation_metres).abs() <= 0.40
                }
            });
            let exact_hall_transverse_infill = frame.program
                == crate::TimberFrameProgramKind::NorthernTwoPostHallHouse
                && plan.wall_assemblies.iter().any(|wall| {
                    wall.frame.outside_room.is_some()
                        && wall.host_solids.contains(&b.id)
                        && frame.internal_lines.iter().any(|line| {
                            line.storeys
                                .iter()
                                .any(|storey| storey.member_ids.contains(&member.id))
                        })
                });
            let exact_civic_plinth_join = frame.program
                == crate::TimberFrameProgramKind::CivicMasonryTimberHall
                && plan.wall_assemblies.iter().any(|wall| {
                    let owns_other = wall.host_solids.contains(&b.id)
                        || plan.opening_assemblies.iter().any(|opening| {
                            opening.host_wall == wall.id
                                && (opening.jamb_solids.contains(&b.id)
                                    || opening.sill_solid == Some(b.id)
                                    || opening.head_solid == b.id
                                    || opening.spandrel_solid == b.id)
                        });
                    let wall_top = wall.base_elevation_metres + wall.height_metres;
                    owns_other
                        && wall.storey_level == 0
                        && wall.material == crate::WallMaterialClass::CivilianMasonry
                        && member.structural
                        && ([member.start.y, member.end.y, a.centre.y]
                            .into_iter()
                            .any(|height| (height - wall_top).abs() <= 0.40)
                            || frame.internal_lines.iter().any(|line| {
                                line.storeys
                                    .iter()
                                    .any(|storey| storey.member_ids.contains(&member.id))
                            }))
                });
            let exact_frame_floor_join = b.role == SolidRole::FrameFloor
                && (frame.floors.iter().any(|floor| {
                    (floor.floor_solid == b.id || floor.floor_solids.contains(&b.id))
                        && (floor.joist_members.contains(&member.id)
                            || floor.girder_members.contains(&member.id)
                            || {
                                let (floor_min, floor_max) = resolved_solid_bounds(b);
                                [member.start.y, member.end.y].into_iter().any(|height| {
                                    height >= floor_min.y - 0.08 && height <= floor_max.y + 0.08
                                })
                            })
                }) || frame
                    .facades
                    .iter()
                    .flat_map(|facade| &facade.lines)
                    .any(|line| {
                        line.storeys.iter().any(|storey| {
                            storey.jetty.as_ref().is_some_and(|jetty| {
                                if jetty.floor_solid != b.id {
                                    return false;
                                }
                                let (floor_min, floor_max) = resolved_solid_bounds(b);
                                jetty.jetty_beams.contains(&member.id)
                                    || jetty.knaggen.contains(&member.id)
                                    || jetty.corner_supports.contains(&member.id)
                                    || [member.start.y, member.end.y].into_iter().any(|height| {
                                        height >= floor_min.y - 0.08 && height <= floor_max.y + 0.08
                                    })
                            })
                        })
                    }));
            let exact_landing_girder_join = b.role == SolidRole::Landing
                && frame.circulation.stair_solids.contains(&b.id)
                && member.role == crate::TimberMemberRole::Girder
                && frame.floors.iter().any(|floor| {
                    floor.girder_members.contains(&member.id) && {
                        let (landing_min, landing_max) = resolved_solid_bounds(b);
                        [member.start.y, member.end.y, a.centre.y]
                            .into_iter()
                            .any(|height| {
                                height >= landing_min.y - 0.08 && height <= landing_max.y + 0.08
                            })
                    }
                });
            let exact_child_roof_join = (frame.dormer_trimmer_members.contains(&member.id)
                && matches!(
                    b.role,
                    SolidRole::RoofFlashing | SolidRole::RoofFraming | SolidRole::RoofGutter
                )
                && plan.roof_assemblies.iter().any(|roof| {
                    (roof.owner == b.owner && roof.parent.is_some())
                        || roof
                            .children
                            .iter()
                            .any(|child| child.flashing_ids.contains(&b.id))
                })
                && (b.role != SolidRole::RoofGutter || {
                    plan.resolved_geometry
                        .roof_drainage_networks
                        .iter()
                        .any(|network| {
                            network.owner == b.owner
                                && (network.channel_floor == b.id
                                    || network.channel_lips.contains(&b.id))
                                && plan.roof_assemblies.iter().any(|roof| {
                                    roof.owner == b.owner
                                        && roof.edges.iter().any(|edge| {
                                            if edge.id != network.receiving_edge {
                                                return false;
                                            }
                                            let a = Vec2::new(edge.start.x, edge.start.z);
                                            let delta = Vec2::new(
                                                edge.end.x - edge.start.x,
                                                edge.end.z - edge.start.z,
                                            );
                                            [member.start, member.end].into_iter().all(|point| {
                                                let point = Vec2::new(point.x, point.z);
                                                let along = ((point - a).dot(delta)
                                                    / delta.length_squared().max(0.000_001))
                                                .clamp(0.0, 1.0);
                                                point.distance(a + delta * along) <= 0.16
                                            })
                                        })
                                })
                        })
                }))
                || (member.role == crate::TimberMemberRole::Sill
                    && b.role == SolidRole::RoofFlashing
                    && frame.bays.iter().any(|bay| {
                        bay.member_ids.contains(&member.id)
                            && bay.wall.is_some_and(|wall_id| {
                                plan.wall_assemblies.iter().any(|wall| {
                                    wall.id == wall_id
                                        && matches!(
                                            wall.source,
                                            crate::WallSourceId::RoofChildFront { .. }
                                        )
                                })
                            })
                    }));
            let exact_child_front_roof_join = matches!(
                member.role,
                crate::TimberMemberRole::GableTie
                    | crate::TimberMemberRole::GablePost
                    | crate::TimberMemberRole::WallPlate
                    | crate::TimberMemberRole::Sill
                    | crate::TimberMemberRole::Rafter
                    | crate::TimberMemberRole::Collar
                    | crate::TimberMemberRole::Purlin
            ) && frame.bays.iter().any(|bay| {
                bay.member_ids.contains(&member.id)
                    && bay.wall.is_some_and(|wall_id| {
                        plan.wall_assemblies
                            .iter()
                            .find(|wall| wall.id == wall_id)
                            .is_some_and(|wall| {
                                matches!(
                                    wall.source,
                                    crate::WallSourceId::RoofChildFront { roof }
                                        if plan.roof_assemblies.iter().any(|assembly| {
                                            (assembly.id == roof && assembly.owner == b.owner)
                                                || assembly.children.iter().any(|child| {
                                                    child.child == roof
                                                        && child.flashing_ids.contains(&b.id)
                                                })
                                        })
                                )
                            })
                    })
            });
            let declared = exact_opening_composite
                || exact_partition_join
                || exact_hall_transverse_infill
                || exact_civic_plinth_join
                || exact_frame_floor_join
                || exact_landing_girder_join
                || exact_child_roof_join
                || exact_child_front_roof_join
                || member.support_interfaces.iter().any(|id| {
                    interfaces
                        .get(id)
                        .is_some_and(|interface| overlap_inside_interface(a, b, interface))
                })
                || frame
                    .floors
                    .iter()
                    .flat_map(|floor| {
                        floor
                            .bearing_interfaces
                            .iter()
                            .chain(&floor.floor_joist_interfaces)
                            .chain(&floor.joist_girder_interfaces)
                    })
                    .chain(&frame.masonry_bearing_interfaces)
                    .chain(&frame.roof_bearing_interfaces)
                    .filter_map(|id| interfaces.get(id).copied())
                    .any(|interface| overlap_inside_interface(a, b, interface))
                || frame
                    .facades
                    .iter()
                    .flat_map(|facade| &facade.lines)
                    .any(|line| {
                        line.storeys.iter().any(|storey| {
                            storey.jetty.as_ref().is_some_and(|jetty| {
                                jetty
                                    .floor_bearing_interfaces
                                    .iter()
                                    .filter_map(|id| interfaces.get(id).copied())
                                    .any(|interface| overlap_inside_interface(a, b, interface))
                            })
                        })
                    });
            if !declared {
                failures.push(pair);
            }
        }
    }
    failures.sort_unstable();
    failures.dedup();
    failures
}

fn coplanar_timber_opening_faces(plan: &BuildingPlan) -> Vec<crate::OpeningAssemblyId> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let framed_walls = frame
        .bays
        .iter()
        .filter_map(|bay| bay.wall)
        .collect::<std::collections::HashSet<_>>();
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let mut conflicts = Vec::new();
    for wall in plan.wall_assemblies.iter().filter(|wall| {
        wall.material == crate::WallMaterialClass::TimberInfill && framed_walls.contains(&wall.id)
    }) {
        let wall_exterior = wall.frame.origin.dot(wall.frame.outward) + wall.thickness_metres * 0.5;
        for opening in plan
            .opening_assemblies
            .iter()
            .filter(|opening| opening.host_wall == wall.id)
        {
            let reaches_frame_plane = opening
                .jamb_solids
                .iter()
                .copied()
                .chain(opening.sill_solid)
                .chain([opening.head_solid, opening.spandrel_solid])
                .filter_map(|id| solids.get(&id).copied())
                .any(|solid| {
                    let half_depth = if wall.frame.outward.x.abs() > 0.5 {
                        solid.size.x * 0.5
                    } else {
                        solid.size.z * 0.5
                    };
                    let centre = Vec2::new(solid.centre.x, solid.centre.z).dot(wall.frame.outward);
                    centre + half_depth > wall_exterior - 0.009
                });
            if reaches_frame_plane {
                conflicts.push(opening.id);
            }
        }
    }
    conflicts
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeshAuditReport {
    pub boundary_edges: usize,
    pub nonmanifold_edges: usize,
    pub inconsistent_winding_edges: usize,
    pub degenerate_triangles: usize,
    pub inverted_winding: bool,
}

impl MeshAuditReport {
    pub const fn passes_closed_solid(self) -> bool {
        self.boundary_edges == 0
            && self.nonmanifold_edges == 0
            && self.inconsistent_winding_edges == 0
            && self.degenerate_triangles == 0
            && !self.inverted_winding
    }
}

pub fn audit_triangle_mesh(positions: &[[f32; 3]], indices: &[u32]) -> MeshAuditReport {
    type Point = [i64; 3];
    let quantize = |position: [f32; 3]| -> Point {
        position.map(|component| (component * 10_000.0).round() as i64)
    };
    let points = positions.iter().copied().map(quantize).collect::<Vec<_>>();
    let mut edges: BTreeMap<(Point, Point), (usize, i32)> = BTreeMap::new();
    let mut report = MeshAuditReport::default();
    let mut signed_volume_x6 = 0.0_f64;
    let (triangles, remainder) = indices.as_chunks::<3>();
    report.degenerate_triangles += usize::from(!remainder.is_empty());
    for triangle in triangles {
        let (Some(&a), Some(&b), Some(&c)) = (
            points.get(triangle[0] as usize),
            points.get(triangle[1] as usize),
            points.get(triangle[2] as usize),
        ) else {
            report.degenerate_triangles += 1;
            continue;
        };
        if a == b || b == c || c == a {
            report.degenerate_triangles += 1;
            continue;
        }
        let af = a.map(|component| component as f64);
        let bf = b.map(|component| component as f64);
        let cf = c.map(|component| component as f64);
        signed_volume_x6 += af[0] * (bf[1] * cf[2] - bf[2] * cf[1])
            + af[1] * (bf[2] * cf[0] - bf[0] * cf[2])
            + af[2] * (bf[0] * cf[1] - bf[1] * cf[0]);
        for (from, to) in [(a, b), (b, c), (c, a)] {
            let (key, direction) = if from < to {
                ((from, to), 1)
            } else {
                ((to, from), -1)
            };
            let edge = edges.entry(key).or_default();
            edge.0 += 1;
            edge.1 += direction;
        }
    }
    for (count, winding) in edges.into_values() {
        match count {
            1 => report.boundary_edges += 1,
            2 if winding != 0 => report.inconsistent_winding_edges += 1,
            2 => {}
            _ => report.nonmanifold_edges += 1,
        }
    }
    report.inverted_winding = signed_volume_x6 < -0.5;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_audit_rejects_open_and_inconsistently_wound_geometry() {
        let tetrahedron = [
            [1.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, -1.0],
            [1.0, -1.0, -1.0],
        ];
        let closed = audit_triangle_mesh(&tetrahedron, &[0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3]);
        assert!(closed.passes_closed_solid(), "{closed:?}");

        let inverted = audit_triangle_mesh(&tetrahedron, &[0, 1, 2, 0, 3, 1, 0, 2, 3, 1, 3, 2]);
        assert!(inverted.inverted_winding);

        let open = audit_triangle_mesh(
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            &[0, 1, 2, 0, 2, 3],
        );
        assert_eq!(open.boundary_edges, 4);

        let bad_winding = audit_triangle_mesh(&tetrahedron, &[0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 3, 2]);
        assert!(bad_winding.inconsistent_winding_edges > 0);
    }

    #[test]
    fn every_curated_plan_passes_the_structural_audit() {
        for archetype in crate::BuildingArchetype::ALL {
            let plan = crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap();
            assert!(
                audit_plan(&plan).is_empty(),
                "{archetype:?}: {:?}",
                audit_plan(&plan)
            );
        }
    }

    #[test]
    fn timber_roof_members_stay_within_the_authoritative_roof_envelope() {
        let mut failures = Vec::new();
        for seed in [42, 47, 101] {
            for archetype in crate::BuildingArchetype::ALL {
                let plan =
                    crate::generate(&crate::BuildingProgram::fixture(archetype, seed)).unwrap();
                let intrusions = timber_roof_envelope_intrusions(&plan);
                if !intrusions.is_empty() {
                    let members = plan
                        .timber_frame
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .filter(|member| intrusions.contains(&member.id))
                        .map(|member| (member.id, member.role, member.start, member.end))
                        .collect::<Vec<_>>();
                    failures.push((seed, archetype, members));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "roof-construction members protrude through authoritative roof faces: {failures:?}"
        );
    }

    #[test]
    fn timber_openings_are_recessed_and_dormers_use_jamb_supported_gable_frames() {
        let mut coplanar = Vec::new();
        let mut invalid_dormers = Vec::new();
        let mut exposed_child_supports = Vec::new();
        let mut oversized_child_flashings = Vec::new();
        let mut invalid_child_curbs = Vec::new();
        let mut oversized_child_gutters = Vec::new();
        let mut undeclared_cross_authority = Vec::new();
        let mut full_audit_failures = Vec::new();
        for seed in [42, 47, 101] {
            for archetype in crate::BuildingArchetype::ALL {
                let plan =
                    crate::generate(&crate::BuildingProgram::fixture(archetype, seed)).unwrap();
                let issues = audit_plan(&plan);
                if !issues.is_empty() {
                    full_audit_failures.push((seed, archetype, issues));
                }
                let conflicts = coplanar_timber_opening_faces(&plan);
                if !conflicts.is_empty() {
                    coplanar.push((seed, archetype, conflicts));
                }
                let exposed = exposed_roof_child_support_posts(&plan);
                if !exposed.is_empty() {
                    exposed_child_supports.push((seed, archetype, exposed));
                }
                let oversized = oversized_child_roof_flashings(&plan);
                if !oversized.is_empty() {
                    oversized_child_flashings.push((seed, archetype, oversized));
                }
                let invalid_curb = invalid_dormer_trimmer_envelope(&plan);
                if !invalid_curb.is_empty() {
                    invalid_child_curbs.push((seed, archetype, invalid_curb));
                }
                let oversized_gutters = oversized_attached_child_gutters(&plan);
                if !oversized_gutters.is_empty() {
                    oversized_child_gutters.push((seed, archetype, oversized_gutters));
                }
                let undeclared = undeclared_timber_intersections(&plan);
                if !undeclared.is_empty() {
                    let described = undeclared
                        .into_iter()
                        .map(|(left, right)| {
                            [left, right].map(|id| {
                                plan.resolved_geometry
                                    .solids
                                    .iter()
                                    .find(|solid| solid.id == id)
                                    .map(|solid| (id, solid.role, solid.centre, solid.size))
                            })
                        })
                        .collect::<Vec<_>>();
                    undeclared_cross_authority.push((seed, archetype, described));
                }
                if let Some(frame) = &plan.timber_frame {
                    let members = frame
                        .members
                        .iter()
                        .map(|member| (member.id, member))
                        .collect::<std::collections::HashMap<_, _>>();
                    for bay in &frame.bays {
                        let Some(wall_id) = bay.wall else { continue };
                        let Some(wall) =
                            plan.wall_assemblies.iter().find(|wall| wall.id == wall_id)
                        else {
                            continue;
                        };
                        if !matches!(wall.source, crate::WallSourceId::RoofChildFront { .. }) {
                            continue;
                        }
                        let roles = bay
                            .member_ids
                            .iter()
                            .filter_map(|id| members.get(id).map(|member| member.role))
                            .collect::<Vec<_>>();
                        let roof_id = match wall.source {
                            crate::WallSourceId::RoofChildFront { roof } => roof,
                            _ => unreachable!(),
                        };
                        let is_shed = plan
                            .roof_assemblies
                            .iter()
                            .find(|roof| roof.id == roof_id)
                            .is_some_and(|roof| roof.kind == crate::RoofKind::Shed);
                        if roles.contains(&crate::TimberMemberRole::PrimaryPost)
                            || (!is_shed
                                && (!roles.contains(&crate::TimberMemberRole::GablePost)
                                    || roles
                                        .iter()
                                        .filter(|role| **role == crate::TimberMemberRole::Rafter)
                                        .count()
                                        < 2))
                        {
                            invalid_dormers.push((seed, archetype, bay.id));
                        }
                    }
                }
            }
        }
        assert!(
            coplanar.is_empty(),
            "coplanar timber/opening faces: {coplanar:?}"
        );
        assert!(
            invalid_dormers.is_empty(),
            "invalid dormer child-front frames: {invalid_dormers:?}"
        );
        assert!(
            exposed_child_supports.is_empty(),
            "roof children retain generic freestanding support posts: {exposed_child_supports:?}"
        );
        assert!(
            oversized_child_flashings.is_empty(),
            "child-roof flashing is rendered as a projecting bar: {oversized_child_flashings:?}"
        );
        assert!(
            invalid_child_curbs.is_empty(),
            "dormer trimmers project beyond their child enclosure: {invalid_child_curbs:?}"
        );
        assert!(
            oversized_child_gutters.is_empty(),
            "attached child roofs reuse full-building gutter profiles: {oversized_child_gutters:?}"
        );
        assert!(
            undeclared_cross_authority.is_empty(),
            "timber intersections fell outside the exact-pair whitelist: {undeclared_cross_authority:?}"
        );
        assert!(
            full_audit_failures.is_empty(),
            "the expanded three-seed/all-archetype audit exposed regressions: {full_audit_failures:?}"
        );

        let mut mutation = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let child = mutation
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap();
        let id = ResolvedItemId(0x7fff_ffff_ffff_ff01);
        mutation
            .resolved_geometry
            .solids
            .push(crate::ResolvedSolid {
                id,
                owner: child.owner,
                centre: Vec3::new(
                    child.faces[0].polygon[0].x,
                    8.5,
                    child.faces[0].polygon[0].z,
                ),
                size: Vec3::new(0.22, 2.9, 0.22),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: child.support_nodes.clone(),
            });
        assert!(
            audit_plan(&mutation)
                .iter()
                .any(|issue| issue.code == "exposed_roof_child_support"),
            "a reintroduced generic dormer-corner post must fail the production audit"
        );

        let mut thick_flashing = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let flashing_id = thick_flashing.roof_assemblies[0].children[0].flashing_ids[0];
        let flashing = thick_flashing
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == flashing_id)
            .unwrap();
        flashing.size.y = 0.07;
        flashing.size.z = 0.16;
        assert!(
            audit_plan(&thick_flashing)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_flashing_profile"),
            "the former oversized child-roof flashing bar must fail the production audit"
        );

        let mut child_downspout = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let child_owner = child_downspout
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap()
            .owner;
        let station_id = child_downspout
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find(|network| network.owner == child_owner)
            .unwrap()
            .outlet_station;
        child_downspout
            .resolved_geometry
            .roof_drainage_outlets
            .iter_mut()
            .find(|station| station.id == station_id)
            .unwrap()
            .disposition = crate::RoofDrainageDisposition::BoundDownspout;
        assert!(
            audit_plan(&child_downspout)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_drainage"),
            "an attached dormer must not regain a detached ground-height downspout"
        );

        let mut projecting_curb = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let trimmer_id = projecting_curb
            .timber_frame
            .as_ref()
            .unwrap()
            .dormer_trimmer_members[0];
        let trimmer = projecting_curb
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| member.id == trimmer_id)
            .unwrap();
        let outward = match projecting_curb.roof_dormers[0].facing {
            crate::Direction::North => Vec2::Y,
            crate::Direction::South => -Vec2::Y,
            crate::Direction::East => Vec2::X,
            crate::Direction::West => -Vec2::X,
        };
        trimmer.end.x += outward.x * projecting_curb.roof_dormers[0].depth_metres * 0.45;
        trimmer.end.z += outward.y * projecting_curb.roof_dormers[0].depth_metres * 0.45;
        assert!(
            audit_plan(&projecting_curb)
                .iter()
                .any(|issue| issue.code == "invalid_dormer_trimmer_envelope"),
            "the former outward-projecting dormer curb must fail the production audit"
        );

        let mut large_child_gutter = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let child_owner = large_child_gutter
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap()
            .owner;
        let gutter_id = large_child_gutter
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find(|network| network.owner == child_owner)
            .unwrap()
            .channel_floor;
        large_child_gutter
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == gutter_id)
            .unwrap()
            .size
            .z = 0.18;
        assert!(
            audit_plan(&large_child_gutter)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_drainage_profile"),
            "a full-size gutter reused on an attached dormer must fail the production audit"
        );

        let mut projecting_child_outlet = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let child_owner = projecting_child_outlet
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap()
            .owner;
        let outlet_id = projecting_child_outlet
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find(|network| network.owner == child_owner)
            .unwrap()
            .outlet_void;
        let outlet = projecting_child_outlet
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == outlet_id)
            .unwrap();
        outlet.bounds.min += Vec3::new(0.40, 0.0, 0.40);
        outlet.bounds.max += Vec3::new(0.40, 0.0, 0.40);
        assert!(
            audit_plan(&projecting_child_outlet)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_drainage_profile"),
            "a child outlet projecting beyond its compact eave zone must fail the audit"
        );

        let mut free_rear_gable = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let gabled_child = free_rear_gable.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::GabledDormer)
            .unwrap()
            .child;
        for edge in &mut free_rear_gable
            .roof_assemblies
            .iter_mut()
            .find(|roof| roof.id == gabled_child)
            .unwrap()
            .edges
        {
            if edge.kind == crate::RoofEdgeKind::OpeningCut {
                edge.kind = crate::RoofEdgeKind::GableVerge;
            }
        }
        assert!(
            audit_plan(&free_rear_gable)
                .iter()
                .any(|issue| issue.code == "unseated_dormer_roof"),
            "a dormer restored to a free rear gable must fail the production audit"
        );

        let mut unlisted_pair = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::TownHouse,
            47,
        ))
        .unwrap();
        let rail_solid = unlisted_pair
            .timber_frame
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.role == crate::TimberMemberRole::Rail)
            .unwrap()
            .solid;
        let closure_centre = unlisted_pair
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.role == SolidRole::OpeningClosure)
            .unwrap()
            .centre;
        let rail = unlisted_pair
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == rail_solid)
            .unwrap();
        rail.centre = closure_centre;
        rail.size = Vec3::splat(0.30);
        assert!(
            !undeclared_timber_intersections(&unlisted_pair).is_empty(),
            "an arbitrary frame/closure collision must not enter the exact-pair whitelist"
        );
    }

    #[test]
    fn artillery_audit_rejects_trace_profile_station_route_and_bridge_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::ArtilleryRondelCastle,
                47,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut missing = fixture();
        missing.artillery_castle = None;
        assert!(has(&missing, "missing_artillery_castle"));
        let mut phase = fixture();
        phase.castle_phase = Some(crate::CastleConstructionPhase::InheritedMedieval);
        assert!(has(&phase, "invalid_artillery_trace"));
        let mut thin = fixture();
        thin.artillery_castle.as_mut().unwrap().curtains[0].total_depth =
            crate::GridLength::new(60).unwrap();
        assert!(has(&thin, "invalid_artillery_curtain"));
        let mut small = fixture();
        small.artillery_castle.as_mut().unwrap().rondels[0].diameter =
            crate::CellDiameter::new(6).unwrap();
        assert!(has(&small, "invalid_artillery_rondel"));
        let mut unbonded = fixture();
        let bond = unbonded.artillery_castle.as_ref().unwrap().rondels[0].curtain_bonds[0];
        unbonded
            .resolved_geometry
            .junction_bonds
            .retain(|item| item.id != bond);
        assert!(has(&unbonded, "invalid_artillery_rondel"));
        let mut no_earth = fixture();
        no_earth.artillery_castle.as_mut().unwrap().curtains[0]
            .earth_solids
            .clear();
        assert!(has(&no_earth, "invalid_artillery_curtain"));
        let mut token_earth = fixture();
        let ids = token_earth.artillery_castle.as_ref().unwrap().rondels[0]
            .earth_solids
            .clone();
        for id in ids.iter().skip(4) {
            token_earth
                .resolved_geometry
                .solids
                .retain(|solid| solid.id != *id);
        }
        for id in ids.iter().take(4) {
            token_earth
                .resolved_geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == *id)
                .unwrap()
                .shape = crate::ResolvedSolidShape::Cuboid;
        }
        assert!(has(&token_earth, "invalid_artillery_rondel"));
        let mut missing_earth_sector = fixture();
        let id = missing_earth_sector
            .artillery_castle
            .as_ref()
            .unwrap()
            .rondels[0]
            .earth_solids[5];
        missing_earth_sector
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != id);
        assert!(has(&missing_earth_sector, "invalid_artillery_rondel"));
        let mut recoil_intrusion = fixture();
        let mut sector = recoil_intrusion
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.role == SolidRole::ArtilleryEarthCore && solid.owner.0 == 60_000)
            .unwrap()
            .clone();
        sector.id = crate::ResolvedItemId(0x7fff_ffff_ffff_f001);
        sector.shape = crate::ResolvedSolidShape::AnnularSectorPrism {
            inner_radius_metres: 3.60,
            outer_radius_metres: 4.775,
            start_angle_radians: std::f32::consts::PI / 16.0,
            end_angle_radians: std::f32::consts::FRAC_PI_8,
            inner_top_offset_metres: 0.0,
            outer_top_offset_metres: 0.0,
        };
        recoil_intrusion.resolved_geometry.solids.push(sector);
        recoil_intrusion.artillery_castle.as_mut().unwrap().rondels[0]
            .earth_solids
            .push(crate::ResolvedItemId(0x7fff_ffff_ffff_f001));
        assert!(has(&recoil_intrusion, "invalid_artillery_rondel"));
        let mut no_rondel_cover = fixture();
        no_rondel_cover.artillery_castle.as_mut().unwrap().rondels[0]
            .parapet_solids
            .clear();
        assert!(has(&no_rondel_cover, "invalid_artillery_rondel"));
        let mut foot_level_cover = fixture();
        let id = foot_level_cover.artillery_castle.as_ref().unwrap().rondels[0].parapet_solids[0];
        foot_level_cover
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .size
            .y = 0.4;
        assert!(has(&foot_level_cover, "invalid_artillery_rondel"));
        let mut no_stair_guard = fixture();
        no_stair_guard.artillery_castle.as_mut().unwrap().rondels[0]
            .stair_guard_solids
            .clear();
        assert!(has(&no_stair_guard, "invalid_artillery_rondel"));
        let mut low_stair_guard = fixture();
        let id =
            low_stair_guard.artillery_castle.as_ref().unwrap().rondels[0].stair_guard_solids[0];
        low_stair_guard
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .size
            .y = 0.45;
        assert!(has(&low_stair_guard, "invalid_artillery_rondel"));
        let mut blocked_stair_arrival = fixture();
        let id = blocked_stair_arrival
            .artillery_castle
            .as_ref()
            .unwrap()
            .rondels[0]
            .stair_guard_solids[0];
        blocked_stair_arrival
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
            inner_radius_metres: 1.30,
            outer_radius_metres: 1.43,
            start_angle_radians: 0.70,
            end_angle_radians: 0.90,
            inner_top_offset_metres: 0.0,
            outer_top_offset_metres: -0.02,
        };
        assert!(has(&blocked_stair_arrival, "invalid_artillery_rondel"));
        let mut unarmed = fixture();
        unarmed.artillery_castle.as_mut().unwrap().stations[0]
            .rays
            .clear();
        assert!(has(&unarmed, "inoperable_artillery_station"));
        let mut uncovered = fixture();
        for station in &mut uncovered.artillery_castle.as_mut().unwrap().stations {
            for ray in &mut station.rays {
                if ray.target_kind == crate::ArtilleryTargetKind::Bridge {
                    ray.target_kind = crate::ArtilleryTargetKind::Approach;
                }
            }
        }
        assert!(has(&uncovered, "incomplete_artillery_coverage"));
        let mut dead_corner = fixture();
        let target = dead_corner
            .artillery_castle
            .as_ref()
            .unwrap()
            .defense_targets
            .iter()
            .find(|target| {
                target.kind == crate::ArtilleryTargetKind::DitchCorner
                    && target.required_independent_stations > 0
            })
            .unwrap()
            .id;
        for station in &mut dead_corner.artillery_castle.as_mut().unwrap().stations {
            station.rays.retain(|ray| ray.target_id != target);
        }
        assert!(has(&dead_corner, "incomplete_artillery_coverage"));
        let mut duplicate_target = fixture();
        let cloned = duplicate_target
            .artillery_castle
            .as_ref()
            .unwrap()
            .defense_targets[0]
            .clone();
        duplicate_target
            .artillery_castle
            .as_mut()
            .unwrap()
            .defense_targets
            .push(cloned);
        assert!(has(&duplicate_target, "incomplete_artillery_coverage"));
        let mut off_envelope = fixture();
        let required = off_envelope
            .artillery_castle
            .as_ref()
            .unwrap()
            .defense_targets
            .iter()
            .find(|target| target.required_independent_stations > 0)
            .unwrap()
            .id;
        let station = off_envelope
            .artillery_castle
            .as_mut()
            .unwrap()
            .stations
            .iter_mut()
            .find(|station| station.rays.iter().any(|ray| ray.target_id == required))
            .unwrap();
        station
            .rays
            .iter_mut()
            .find(|ray| ray.target_id == required)
            .unwrap()
            .target
            .x += 2.0;
        assert!(has(&off_envelope, "inoperable_artillery_station"));
        let mut narrow = fixture();
        narrow.artillery_castle.as_mut().unwrap().route_edges[0].width_metres = 0.6;
        assert!(has(&narrow, "disconnected_artillery_route"));
        let mut low = fixture();
        low.artillery_castle.as_mut().unwrap().route_edges[0].headroom_metres = 1.2;
        assert!(has(&low, "disconnected_artillery_route"));
        let mut blocked_sweep = fixture();
        let point = blocked_sweep.artillery_castle.as_ref().unwrap().route_edges[2].sweep_path[3];
        let id = blocked_sweep.artillery_castle.as_ref().unwrap().rondels[0].parapet_solids[0];
        let solid = blocked_sweep
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap();
        solid.centre = point + Vec3::Y;
        solid.size = Vec3::new(1.2, 2.0, 1.2);
        solid.shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&blocked_sweep, "disconnected_artillery_route"));
        let mut missing_route_surface = fixture();
        missing_route_surface
            .artillery_castle
            .as_mut()
            .unwrap()
            .route_edges[0]
            .traversal_surface = None;
        assert!(has(&missing_route_surface, "disconnected_artillery_route"));
        let mut shifted_route_surface = fixture();
        let route_surface = shifted_route_surface
            .artillery_castle
            .as_ref()
            .unwrap()
            .route_edges[0]
            .traversal_surface
            .unwrap();
        shifted_route_surface
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == route_surface)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::RouteCorridor {
            start: Vec3::ZERO,
            end: Vec3::X,
            width_metres: 1.8,
        };
        assert!(has(&shifted_route_surface, "disconnected_artillery_route"));
        let mut shifted_portal = fixture();
        let edge = shifted_portal
            .artillery_castle
            .as_ref()
            .unwrap()
            .route_edges
            .iter()
            .find(|edge| edge.portal_void.is_some())
            .unwrap()
            .clone();
        let portal = edge.portal_void.unwrap();
        let void = shifted_portal
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == portal)
            .unwrap();
        void.bounds.min += Vec3::X * 20.0;
        void.bounds.max += Vec3::X * 20.0;
        assert!(has(&shifted_portal, "disconnected_artillery_route"));
        let mut missing_tread = fixture();
        let tread = missing_tread.artillery_castle.as_ref().unwrap().rondels[0].stair_solids[0];
        missing_tread
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != tread);
        assert!(
            has(&missing_tread, "disconnected_artillery_route")
                || has(&missing_tread, "renderer_geometry_mismatch")
        );
        let mut severed_ramp = fixture();
        let ramp = severed_ramp
            .artillery_castle
            .as_ref()
            .unwrap()
            .service_ramp_solids[0];
        for edge in &mut severed_ramp.artillery_castle.as_mut().unwrap().route_edges {
            edge.connector_solids.retain(|id| *id != ramp);
        }
        assert!(has(&severed_ramp, "disconnected_artillery_route"));
        let mut broken_bridge = fixture();
        broken_bridge
            .artillery_castle
            .as_mut()
            .unwrap()
            .bridge
            .route_surface = None;
        assert!(has(&broken_bridge, "invalid_artillery_approach"));
        let mut false_ditch = fixture();
        let ditch = false_ditch.artillery_castle.as_ref().unwrap().ditch.void_id;
        false_ditch
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == ditch)
            .unwrap()
            .shape = crate::ResolvedVoidShape::Box;
        assert!(has(&false_ditch, "invalid_artillery_approach"));
        let mut no_mechanism = fixture();
        let mechanism = no_mechanism
            .artillery_castle
            .as_ref()
            .unwrap()
            .gate_chamber_solids
            .iter()
            .find(|id| {
                no_mechanism.resolved_geometry.solids.iter().any(|solid| {
                    solid.id == **id && solid.role == SolidRole::ArtilleryGateMechanism
                })
            })
            .copied()
            .unwrap();
        no_mechanism
            .artillery_castle
            .as_mut()
            .unwrap()
            .gate_chamber_solids
            .retain(|id| *id != mechanism);
        assert!(has(&no_mechanism, "invalid_artillery_approach"));
        let mut glazed = fixture();
        let opening_id = glazed.artillery_castle.as_ref().unwrap().stations[0].opening;
        glazed
            .opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == opening_id)
            .unwrap()
            .closure
            .layers = vec![crate::ClosureKind::LeadedGlazing];
        assert!(
            has(&glazed, "inoperable_artillery_station") || has(&glazed, "illegal_opening_closure")
        );

        let denied = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::ArtilleryRondelCastle,
            702,
        ))
        .unwrap();
        assert!(audit_plan(&denied).is_empty(), "{:?}", audit_plan(&denied));
        let mut denied_crossing = denied.clone();
        let removable = denied_crossing
            .artillery_castle
            .as_ref()
            .unwrap()
            .bridge
            .removable_solids[0];
        denied_crossing
            .artillery_castle
            .as_mut()
            .unwrap()
            .route_edges[0]
            .connector_solids
            .push(removable);
        assert!(
            has(&denied_crossing, "invalid_artillery_approach")
                || has(&denied_crossing, "disconnected_artillery_route")
        );
        let mut contaminated = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            47,
        ))
        .unwrap();
        contaminated.castle_phase = Some(crate::CastleConstructionPhase::ArtilleryRetrofit1544);
        assert!(has(&contaminated, "artillery_phase_contamination"));
    }

    #[test]
    fn church_audit_rejects_program_structure_tower_and_route_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::Cathedral,
                47,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut missing = fixture();
        missing.church = None;
        assert!(has(&missing, "missing_church_program"));

        let mut axes = fixture();
        axes.church.as_mut().unwrap().nave_axes_metres.swap(0, 1);
        assert!(has(&axes, "invalid_church_bay_axes"));

        let mut pier = fixture();
        let pier_id = pier.church.as_ref().unwrap().bay_assemblies[0].pier_solids[0];
        pier.resolved_geometry
            .solids
            .retain(|solid| solid.id != pier_id);
        assert!(has(&pier, "invalid_church_bay_structure"));

        let mut buttress = fixture();
        let node_id = buttress.church.as_ref().unwrap().bay_assemblies[0].buttress_nodes[0];
        buttress
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == node_id)
            .unwrap()
            .grounded = false;
        assert!(has(&buttress, "invalid_church_bay_structure"));

        let mut one_sided_arcade = fixture();
        let arcade_id =
            one_sided_arcade.church.as_ref().unwrap().bay_assemblies[0].arcade_solids[0];
        let spring_id = one_sided_arcade
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == arcade_id)
            .unwrap()
            .supported_by[0];
        one_sided_arcade
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == spring_id)
            .unwrap()
            .supported_by
            .pop();
        assert!(has(&one_sided_arcade, "invalid_church_bay_structure"));

        let mut shifted_springing = fixture();
        let interface_id = shifted_springing.church.as_ref().unwrap().bay_assemblies[0]
            .arcade_bearing_interfaces[0][1];
        shifted_springing
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
            .unwrap()
            .bounds
            .min
            .x += 1.0;
        assert!(has(&shifted_springing, "invalid_church_bay_structure"));

        let mut decorative_buttress = fixture();
        let vault_spring =
            decorative_buttress.church.as_ref().unwrap().bay_assemblies[0].vault_spring_nodes[0];
        let buttress_supports = decorative_buttress
            .resolved_geometry
            .structural_nodes
            .iter()
            .filter(|node| node.kind == crate::StructuralNodeKind::ChurchButtress)
            .map(|node| node.id)
            .collect::<BTreeSet<_>>();
        decorative_buttress
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == vault_spring)
            .unwrap()
            .supported_by
            .retain(|support| !buttress_supports.contains(support));
        assert!(has(&decorative_buttress, "invalid_church_bay_structure"));

        let mut floating_vault = fixture();
        let vault_id = floating_vault.church.as_ref().unwrap().bay_assemblies[0].vault_solids[0];
        floating_vault
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == vault_id)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(&floating_vault, "invalid_church_bay_structure"));

        let mut missing_load_surface = fixture();
        missing_load_surface.church.as_mut().unwrap().bay_assemblies[0]
            .vault_load_surfaces
            .pop();
        assert!(has(&missing_load_surface, "invalid_church_bay_structure"));

        let mut crossing = fixture();
        crossing
            .church
            .as_mut()
            .unwrap()
            .crossing
            .vault_solids
            .clear();
        assert!(has(&crossing, "invalid_church_crossing"));

        let mut flat_crossing_arch = fixture();
        let crossing_arch = flat_crossing_arch
            .church
            .as_ref()
            .unwrap()
            .crossing
            .arch_solids[0];
        flat_crossing_arch
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == crossing_arch)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&flat_crossing_arch, "invalid_church_crossing"));

        let mut shifted_crossing_spring = fixture();
        let crossing_interface = shifted_crossing_spring
            .church
            .as_ref()
            .unwrap()
            .crossing
            .arch_bearing_interfaces[0][1];
        shifted_crossing_spring
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == crossing_interface)
            .unwrap()
            .bounds
            .min
            .x += 1.0;
        assert!(has(&shifted_crossing_spring, "invalid_church_crossing"));

        let mut severed_crossing_thrust = fixture();
        let crossing_spring = severed_crossing_thrust
            .church
            .as_ref()
            .unwrap()
            .crossing
            .vault_spring_nodes[0];
        let crossing_buttress = severed_crossing_thrust
            .church
            .as_ref()
            .unwrap()
            .crossing
            .buttress_nodes[0];
        severed_crossing_thrust
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == crossing_spring)
            .unwrap()
            .supported_by
            .retain(|support| *support != crossing_buttress);
        assert!(has(&severed_crossing_thrust, "invalid_church_crossing"));

        let mut missing_crossing_load = fixture();
        missing_crossing_load
            .church
            .as_mut()
            .unwrap()
            .crossing
            .vault_load_surfaces
            .clear();
        assert!(has(&missing_crossing_load, "invalid_church_crossing"));

        let mut apse = fixture();
        apse.church.as_mut().unwrap().choir.apse_facets.pop();
        assert!(has(&apse, "invalid_church_choir_apse"));

        let mut flat_choir_arch = fixture();
        let choir_arch = flat_choir_arch.church.as_ref().unwrap().choir.arch_solids[0];
        flat_choir_arch
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == choir_arch)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&flat_choir_arch, "invalid_church_choir_apse"));

        let mut severed_choir_thrust = fixture();
        let choir_spring = severed_choir_thrust
            .church
            .as_ref()
            .unwrap()
            .choir
            .vault_spring_nodes[0];
        let choir_buttress = severed_choir_thrust
            .church
            .as_ref()
            .unwrap()
            .choir
            .buttress_nodes[0];
        severed_choir_thrust
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == choir_spring)
            .unwrap()
            .supported_by
            .retain(|support| *support != choir_buttress);
        assert!(has(&severed_choir_thrust, "invalid_church_choir_apse"));

        let mut shifted_choir_spring = fixture();
        let choir_interface = shifted_choir_spring
            .church
            .as_ref()
            .unwrap()
            .choir
            .arch_bearing_interfaces[0][0];
        let interface = shifted_choir_spring
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == choir_interface)
            .unwrap();
        interface.bounds.min.z += 1.0;
        interface.bounds.max.z += 1.0;
        assert!(has(&shifted_choir_spring, "invalid_church_choir_apse"));

        let mut portal = fixture();
        portal.church.as_mut().unwrap().tower.west_portal = crate::OpeningAssemblyId(0);
        assert!(has(&portal, "invalid_church_west_tower"));

        let mut stair = fixture();
        let stair_index = stair.church.as_ref().unwrap().tower.stair_index;
        if let Stair::Spiral { rise_metres, .. } = &mut stair.stairs[stair_index] {
            *rise_metres -= 2.0;
        }
        assert!(has(&stair, "invalid_church_west_tower"));

        let mut bell = fixture();
        let bell_opening = bell.church.as_ref().unwrap().tower.bell_openings[0];
        bell.opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == bell_opening)
            .unwrap()
            .closure
            .layers = vec![crate::ClosureKind::LeadedGlazing];
        assert!(has(&bell, "invalid_church_west_tower"));

        let mut blocked_stairwell = fixture();
        let floor_id = blocked_stairwell
            .church
            .as_ref()
            .unwrap()
            .tower
            .bell_floor_solids[0];
        let centre = blocked_stairwell.church.as_ref().unwrap().tower.centre;
        let floor = blocked_stairwell
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap();
        floor.centre.x = centre.x;
        floor.centre.z = centre.y;
        assert!(has(
            &blocked_stairwell,
            "invalid_church_tower_service_geometry"
        ));

        let mut short_roof_ladder = fixture();
        let ladder_ids = short_roof_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .roof_ladder_solids
            .clone();
        for solid in &mut short_roof_ladder.resolved_geometry.solids {
            if ladder_ids.contains(&solid.id) {
                solid.centre.y -= 1.0;
                solid.size.y = solid.size.y.min(1.0);
            }
        }
        assert!(has(
            &short_roof_ladder,
            "invalid_church_tower_service_geometry"
        ));

        let mut unsupported_bell_floor = fixture();
        let floor_id = unsupported_bell_floor
            .church
            .as_ref()
            .unwrap()
            .tower
            .bell_floor_solids[0];
        let stage_id = unsupported_bell_floor
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == floor_id)
            .unwrap()
            .supported_by[0];
        unsupported_bell_floor
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == stage_id)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(
            &unsupported_bell_floor,
            "invalid_church_tower_service_geometry"
        ));

        let mut window = fixture();
        window.church.as_mut().unwrap().bay_assemblies[0].clerestory_openings[0] =
            crate::OpeningAssemblyId(0);
        assert!(has(&window, "invalid_church_window_hierarchy"));

        let mut unglazed_light = fixture();
        let window_id =
            unglazed_light.church.as_ref().unwrap().bay_assemblies[0].clerestory_openings[0];
        unglazed_light
            .opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == window_id)
            .unwrap()
            .closure_solids
            .clear();
        assert!(has(&unglazed_light, "invalid_church_window_hierarchy"));

        let mut route = fixture();
        route.church.as_mut().unwrap().circulation[0].width_metres = 0.70;
        assert!(has(&route, "invalid_church_circulation"));

        let mut shifted_west_portal_void = fixture();
        let west_portal = shifted_west_portal_void
            .church
            .as_ref()
            .unwrap()
            .tower
            .west_portal;
        let west_void = shifted_west_portal_void
            .opening_assemblies
            .iter()
            .find(|opening| opening.id == west_portal)
            .unwrap()
            .void_id;
        let west_void = shifted_west_portal_void
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == west_void)
            .unwrap();
        west_void.bounds.min.z += 1.0;
        west_void.bounds.max.z += 1.0;
        assert!(has(&shifted_west_portal_void, "invalid_church_circulation"));

        let mut shifted_nave_passage_void = fixture();
        let nave_passage = shifted_nave_passage_void
            .church
            .as_ref()
            .unwrap()
            .tower
            .nave_passage;
        let nave_void = shifted_nave_passage_void
            .opening_assemblies
            .iter()
            .find(|opening| opening.id == nave_passage)
            .unwrap()
            .void_id;
        let nave_void = shifted_nave_passage_void
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == nave_void)
            .unwrap();
        nave_void.bounds.min.z -= 1.0;
        nave_void.bounds.max.z -= 1.0;
        assert!(has(
            &shifted_nave_passage_void,
            "invalid_church_circulation"
        ));

        let mut narrowed_west_portal = fixture();
        let west_portal = narrowed_west_portal
            .church
            .as_ref()
            .unwrap()
            .tower
            .west_portal;
        let portal = narrowed_west_portal
            .opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == west_portal)
            .unwrap();
        portal.profile = crate::OpeningProfile::Rectangular {
            width_metres: 1.60,
            height_metres: 3.20,
        };
        portal
            .sectional_void
            .iter_mut()
            .for_each(|slice| slice.width_metres = 1.60);
        assert!(has(&narrowed_west_portal, "invalid_church_circulation"));

        let mut public_self_edge = fixture();
        let public = public_self_edge
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::PublicProcessional)
            .unwrap();
        public.edges[0].to = public.edges[0].from;
        assert!(has(&public_self_edge, "invalid_church_circulation"));

        let mut missing_vestibule = fixture();
        let vestibule = missing_vestibule
            .church
            .as_ref()
            .unwrap()
            .tower
            .vestibule_surface;
        missing_vestibule
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::PublicProcessional)
            .unwrap()
            .surface_ids
            .retain(|id| *id != vestibule);
        assert!(has(&missing_vestibule, "invalid_church_circulation"));

        let mut missing_shared_service_connection = fixture();
        let vestibule = missing_shared_service_connection
            .church
            .as_ref()
            .unwrap()
            .tower
            .vestibule_surface;
        let bell_route = missing_shared_service_connection
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap();
        bell_route.surface_ids.retain(|id| *id != vestibule);
        bell_route
            .edges
            .retain(|edge| edge.from != vestibule && edge.to != vestibule);
        assert!(has(
            &missing_shared_service_connection,
            "invalid_church_circulation"
        ));

        let mut broken_tread = fixture();
        let tread = broken_tread
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_tread_solids[20];
        broken_tread
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != tread);
        assert!(has(&broken_tread, "invalid_church_circulation"));

        let mut severed_stair_bearing = fixture();
        let bearing = severed_stair_bearing
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_bearing_node;
        severed_stair_bearing
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == bearing)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(&severed_stair_bearing, "invalid_church_circulation"));

        let mut shifted_tread_bearing = fixture();
        let bearing_interface = shifted_tread_bearing
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_tread_interfaces[12];
        let interface = shifted_tread_bearing
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == bearing_interface)
            .unwrap();
        interface.bounds.min.x += 0.8;
        interface.bounds.max.x += 0.8;
        assert!(has(&shifted_tread_bearing, "invalid_church_circulation"));

        let mut moved_newel = fixture();
        let newel = moved_newel.church.as_ref().unwrap().tower.stair_newel_solid;
        moved_newel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == newel)
            .unwrap()
            .centre
            .x += 0.5;
        assert!(has(&moved_newel, "invalid_church_circulation"));

        let mut guard_in_route = fixture();
        let guard = guard_in_route.church.as_ref().unwrap().tower.guard_solids[0];
        let route_tread = guard_in_route
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_tread_solids[20];
        let route_centre = guard_in_route
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == route_tread)
            .unwrap()
            .centre;
        guard_in_route
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == guard)
            .unwrap()
            .centre = route_centre + Vec3::Y * 0.55;
        assert!(has(&guard_in_route, "invalid_church_circulation"));

        let mut frame_in_ladder = fixture();
        let frame = frame_in_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .bell_frame_solids[0];
        let rung = frame_in_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .roof_ladder_solids[6];
        let rung_centre = frame_in_ladder
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == rung)
            .unwrap()
            .centre;
        frame_in_ladder
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == frame)
            .unwrap()
            .centre = rung_centre;
        assert!(has(&frame_in_ladder, "invalid_church_circulation"));

        let mut broken_edge = fixture();
        let bell_route = broken_edge
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap();
        bell_route.edges.remove(12);
        assert!(has(&broken_edge, "invalid_church_circulation"));

        let mut missing_landing = fixture();
        let landing = missing_landing
            .church
            .as_ref()
            .unwrap()
            .tower
            .landing_solids[1];
        missing_landing
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap()
            .traversable_solid_ids
            .retain(|id| *id != landing);
        assert!(has(&missing_landing, "invalid_church_circulation"));

        let mut low_route = fixture();
        low_route
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap()
            .edges[0]
            .clear_headroom_metres = 1.20;
        assert!(has(&low_route, "invalid_church_circulation"));

        let mut blocked_ladder = fixture();
        let bell_id = blocked_ladder.church.as_ref().unwrap().tower.bell_solid;
        let rung_id = blocked_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .roof_ladder_solids[4];
        let rung_centre = blocked_ladder
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == rung_id)
            .unwrap()
            .centre;
        blocked_ladder
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == bell_id)
            .unwrap()
            .centre = rung_centre;
        assert!(has(&blocked_ladder, "invalid_church_circulation"));

        let mut roof = fixture();
        roof.church.as_mut().unwrap().roof_assemblies.clear();
        assert!(has(&roof, "invalid_church_roof_program"));
    }

    #[test]
    fn roof_audit_rejects_graph_support_child_and_weathering_drift() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::FachwerkMerchantHouse,
                47,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut pitch = fixture();
        pitch.roof_assemblies[0].faces[0].pitch_degrees = 10.0;
        assert!(has(&pitch, "invalid_roof_pitch"));

        let mut plane = fixture();
        plane.roof_assemblies[0].faces[0].plane.constant += 1.0;
        assert!(has(&plane, "invalid_roof_face_contract"));

        let mut edge = fixture();
        edge.roof_assemblies[0].edges[0].adjacent_faces.clear();
        assert!(has(&edge, "roof_edge_adjacency"));

        let mut drain = fixture();
        let route = drain.resolved_geometry.drainage_catchments[0].outlet_route;
        drain
            .resolved_geometry
            .drainage_routes
            .retain(|candidate| candidate.id != route);
        assert!(has(&drain, "invalid_roof_face_contract"));

        let mut support = fixture();
        support.roof_assemblies[0].faces[0].support_nodes.clear();
        assert!(has(&support, "invalid_roof_face_contract"));

        let mut intact_parent = fixture();
        intact_parent.roof_assemblies[0]
            .faces
            .iter_mut()
            .for_each(|face| face.cutouts.clear());
        assert!(has(&intact_parent, "unresolved_roof_child"));

        let mut no_flashing = fixture();
        no_flashing.roof_assemblies[0].children[0]
            .flashing_ids
            .clear();
        assert!(has(&no_flashing, "unresolved_roof_child"));

        let mut reversed_shed = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::Cathedral,
            47,
        ))
        .unwrap();
        let shed = reversed_shed
            .roof_assemblies
            .iter_mut()
            .find(|roof| roof.kind == crate::RoofKind::Shed)
            .unwrap();
        shed.shed_high_side = Some(shed.shed_high_side.unwrap().opposite());
        assert!(has(&reversed_shed, "invalid_shed_slope_authority"));

        let mut orphan_eave = fixture();
        orphan_eave.roof_assemblies[0]
            .edges
            .iter_mut()
            .find(|edge| edge.kind == RoofEdgeKind::Eave)
            .unwrap()
            .drainage_terminal = None;
        assert!(has(&orphan_eave, "orphan_roof_drainage"));

        let mut tower_overlap = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::Cathedral,
            47,
        ))
        .unwrap();
        let tower_child = tower_overlap.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::Tower)
            .unwrap()
            .clone();
        tower_overlap.roof_assemblies[0]
            .faces
            .iter_mut()
            .for_each(|face| face.cutouts.clear());
        assert!(has(&tower_overlap, "unresolved_roof_child"));
        tower_overlap.roof_assemblies[0]
            .children
            .retain(|child| child.child != tower_child.child);
        assert!(has(&tower_overlap, "orphan_roof_child"));

        let mut flat_valley = fixture();
        let valley_flashing = flat_valley.roof_assemblies[0]
            .edges
            .iter()
            .find(|edge| edge.kind == RoofEdgeKind::Valley)
            .and_then(|edge| edge.flashing)
            .unwrap();
        flat_valley
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == valley_flashing)
            .unwrap()
            .longfall_radians = 0.0;
        assert!(has(&flat_valley, "invalid_roof_valley_drainage"));

        let mut orphan_valley = fixture();
        orphan_valley.roof_assemblies[0]
            .edges
            .iter_mut()
            .find(|edge| edge.kind == RoofEdgeKind::Valley)
            .unwrap()
            .drainage_terminal = None;
        assert!(has(&orphan_valley, "invalid_roof_valley_drainage"));

        let mut relabelled_full_hip = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::HallHouse,
            47,
        ))
        .unwrap();
        let half_hip = relabelled_full_hip
            .roof_assemblies
            .iter_mut()
            .find(|roof| roof.kind == crate::RoofKind::HalfHip)
            .unwrap();
        half_hip.enclosure_faces.clear();
        let base_y = half_hip
            .faces
            .iter()
            .flat_map(|face| &face.polygon)
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        for cap in half_hip
            .faces
            .iter_mut()
            .filter(|face| face.polygon.len() == 3)
        {
            cap.polygon[0].y = base_y;
            cap.polygon[2].y = base_y;
        }
        assert!(has(&relabelled_full_hip, "invalid_half_hip_graph"));

        let mut missing_channel = fixture();
        let channel = missing_channel.resolved_geometry.roof_drainage_networks[0].channel_floor;
        missing_channel
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != channel);
        assert!(has(&missing_channel, "invalid_roof_drainage_network"));

        let mut flat_channel = fixture();
        let channel = flat_channel.resolved_geometry.roof_drainage_networks[0].channel_floor;
        flat_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .longfall_radians = 0.0;
        assert!(has(&flat_channel, "invalid_roof_drainage_network"));

        let mut reversed_channel = fixture();
        reversed_channel.resolved_geometry.roof_drainage_networks[0]
            .channel_low
            .y = reversed_channel.resolved_geometry.roof_drainage_networks[0]
            .channel_high
            .y
            + 0.05;
        assert!(has(&reversed_channel, "invalid_roof_drainage_network"));

        let mut trapped_basin = fixture();
        trapped_basin.resolved_geometry.roof_drainage_networks[0]
            .samples
            .remove(3);
        assert!(has(&trapped_basin, "invalid_roof_drainage_network"));

        let mut shifted_channel = fixture();
        let channel = shifted_channel.resolved_geometry.roof_drainage_networks[0].channel_floor;
        shifted_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .centre += Vec3::X;
        assert!(has(&shifted_channel, "invalid_roof_drainage_network"));

        let mut disconnected_outlet = fixture();
        let outlet = disconnected_outlet.resolved_geometry.roof_drainage_networks[0].outlet_void;
        let drain = disconnected_outlet
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == outlet)
            .unwrap();
        drain.bounds.min += Vec3::X;
        drain.bounds.max += Vec3::X;
        assert!(has(&disconnected_outlet, "invalid_roof_drainage_network"));

        let spout_fixture = || {
            crate::BuildingArchetype::ALL
                .into_iter()
                .map(|archetype| {
                    crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap()
                })
                .find(|plan| {
                    plan.resolved_geometry
                        .roof_drainage_networks
                        .iter()
                        .any(|network| network.downspout.is_some())
                })
                .expect("curated roof suite contains a host-bound downspout")
        };
        let mut shifted_spout = spout_fixture();
        let spout = shifted_spout
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .expect("principal roof has a downspout");
        shifted_spout
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == spout)
            .unwrap()
            .centre += Vec3::Z;
        assert!(has(&shifted_spout, "invalid_roof_drainage_network"));

        let mut spout_through_opening = spout_fixture();
        let spout = spout_through_opening
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .expect("principal roof has a downspout");
        let opening = spout_through_opening
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.role == VoidRole::WallOpening)
            .unwrap();
        let opening_plan = (opening.bounds.min + opening.bounds.max) * 0.5;
        let pipe = spout_through_opening
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == spout)
            .unwrap();
        pipe.centre.x = opening_plan.x;
        pipe.centre.z = opening_plan.z;
        assert!(has(&spout_through_opening, "invalid_roof_drainage_network"));

        let mut spout_through_walk = spout_fixture();
        let spout = spout_through_walk
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .unwrap();
        let mut walk = spout_through_walk
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == spout)
            .unwrap()
            .clone();
        walk.id = crate::ResolvedItemId(0xAFFF_FFFF_FFFF_0001);
        walk.role = SolidRole::CircuitWalk;
        spout_through_walk.resolved_geometry.solids.push(walk);
        assert!(has(&spout_through_walk, "invalid_roof_drainage_network"));

        let mut spout_through_stair = spout_fixture();
        let spout = spout_through_stair
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .unwrap();
        let pipe = spout_through_stair
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == spout)
            .unwrap();
        spout_through_stair.stairs.push(crate::Stair::Straight {
            start: Vec2::new(pipe.centre.x, pipe.centre.z),
            direction: crate::Direction::North,
            base_height_metres: 0.0,
            rise_metres: pipe.centre.y + pipe.size.y,
            width_metres: 1.0,
            tread_count: 12,
        });
        assert!(has(&spout_through_stair, "invalid_roof_drainage_network"));

        let mut round_program =
            crate::BuildingProgram::fixture(crate::BuildingArchetype::TownHouse, 47);
        round_program.roof_demonstrator = Some(crate::RoofKind::Conical);
        let mut per_facet_round_spouts = crate::generate(&round_program).unwrap();
        let round_owner = per_facet_round_spouts
            .roof_assemblies
            .iter()
            .find(|roof| {
                matches!(
                    roof.kind,
                    crate::RoofKind::Conical | crate::RoofKind::Pavilion
                )
            })
            .unwrap()
            .owner;
        let mut duplicate = per_facet_round_spouts
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.owner == round_owner)
            .unwrap()
            .clone();
        duplicate.id = crate::ResolvedItemId(0x7FFF_FFFF_FFFF_0001);
        per_facet_round_spouts
            .resolved_geometry
            .roof_drainage_outlets
            .push(duplicate);
        assert!(has(&per_facet_round_spouts, "invalid_roof_outlet_topology"));

        let free_ground_fixture = || {
            crate::BuildingArchetype::ALL
                .into_iter()
                .map(|archetype| {
                    crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap()
                })
                .find(|plan| {
                    plan.resolved_geometry
                        .roof_drainage_outlets
                        .iter()
                        .any(|station| {
                            station.disposition == crate::RoofDrainageDisposition::FreeDripToGround
                        })
                })
                .expect("curated roof suite contains an explicit ground free-drip")
        };
        let mut blocked_free_fall = free_ground_fixture();
        let station = blocked_free_fall
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.disposition == crate::RoofDrainageDisposition::FreeDripToGround)
            .unwrap()
            .clone();
        let outlet = blocked_free_fall
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == station.outlet_void)
            .map(|void| (void.bounds.min + void.bounds.max) * 0.5)
            .unwrap();
        blocked_free_fall
            .resolved_geometry
            .solids
            .push(crate::ResolvedSolid {
                id: crate::ResolvedItemId(0xAFFF_FFFF_FFFF_0100),
                owner: crate::GeometryOwnerId(0xFFFF_0100),
                centre: (outlet + station.discharge) * 0.5,
                size: Vec3::new(1.0, 0.18, 1.0),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::CircuitWalk,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: Vec::new(),
            });
        assert!(has(&blocked_free_fall, "invalid_roof_drainage_network"));

        let mut splash_on_portal = free_ground_fixture();
        let station = splash_on_portal
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.disposition == crate::RoofDrainageDisposition::FreeDripToGround)
            .unwrap()
            .clone();
        splash_on_portal
            .resolved_geometry
            .voids
            .push(crate::ResolvedVoid {
                id: crate::ResolvedItemId(0xEFFF_FFFF_FFFF_0100),
                owner: crate::GeometryOwnerId(0xFFFF_0101),
                bounds: crate::ResolvedBounds {
                    min: station.discharge - Vec3::new(0.4, 0.08, 0.4),
                    max: station.discharge + Vec3::new(0.4, 1.9, 0.4),
                },
                role: VoidRole::AccessPortal,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: crate::GeometryOwnerId(0xFFFF_0101),
            });
        assert!(has(&splash_on_portal, "invalid_roof_drainage_network"));

        let mut splash_on_stair = free_ground_fixture();
        let discharge = splash_on_stair
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.disposition == crate::RoofDrainageDisposition::FreeDripToGround)
            .unwrap()
            .discharge;
        splash_on_stair.stairs.push(crate::Stair::Straight {
            start: Vec2::new(discharge.x, discharge.z),
            direction: crate::Direction::North,
            base_height_metres: 0.0,
            rise_metres: 1.2,
            width_metres: 1.0,
            tread_count: 8,
        });
        assert!(has(&splash_on_stair, "invalid_roof_drainage_network"));

        let mut off_parent_face = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::Cathedral,
            47,
        ))
        .unwrap();
        let station = off_parent_face
            .resolved_geometry
            .roof_drainage_outlets
            .iter_mut()
            .find(|station| {
                station.disposition == crate::RoofDrainageDisposition::FreeDripToParentRoof
            })
            .expect("cathedral tower drip lands on an exact parent face");
        station.discharge.x += 20.0;
        assert!(has(&off_parent_face, "invalid_roof_drainage_network"));

        let edge_treatment_fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::Cathedral,
                47,
            ))
            .unwrap()
        };
        let treatment_id = |plan: &crate::BuildingPlan| {
            let owners = plan
                .roof_assemblies
                .iter()
                .map(|roof| roof.owner)
                .collect::<std::collections::HashSet<_>>();
            plan.resolved_geometry
                .solids
                .iter()
                .find(|solid| {
                    owners.contains(&solid.owner) && solid.role == SolidRole::RoofEdgeTreatment
                })
                .map(|solid| solid.id)
                .expect("roof graph owns typed edge treatment")
        };
        let mut offset_treatment = edge_treatment_fixture();
        let treatment = treatment_id(&offset_treatment);
        offset_treatment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == treatment)
            .unwrap()
            .centre += Vec3::Z;
        assert!(has(&offset_treatment, "invalid_roof_edge_treatment"));

        let mut rotated_treatment = edge_treatment_fixture();
        let treatment = treatment_id(&rotated_treatment);
        rotated_treatment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == treatment)
            .unwrap()
            .yaw_radians += 0.4;
        assert!(has(&rotated_treatment, "invalid_roof_edge_treatment"));

        let mut overlong_treatment = edge_treatment_fixture();
        let treatment = treatment_id(&overlong_treatment);
        overlong_treatment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == treatment)
            .unwrap()
            .size
            .x += 2.0;
        assert!(has(&overlong_treatment, "invalid_roof_edge_treatment"));

        let cross_fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::FachwerkMerchantHouse,
                47,
            ))
            .unwrap()
        };
        let mut blank_dormer = cross_fixture();
        let cross = blank_dormer.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::CrossGable && child.child.0 >= 1_000)
            .unwrap()
            .clone();
        let front = blank_dormer
            .wall_assemblies
            .iter()
            .find(|wall| wall.source == crate::WallSourceId::RoofChildFront { roof: cross.child })
            .unwrap()
            .clone();
        blank_dormer
            .opening_assemblies
            .retain(|opening| opening.host_wall != front.id);
        assert!(has(&blank_dormer, "invalid_roof_child_front"));

        let mut floating_cross = cross_fixture();
        let cross = floating_cross.roof_assemblies[0]
            .children
            .iter_mut()
            .find(|child| child.kind == crate::RoofChildKind::CrossGable && child.child.0 >= 1_000)
            .unwrap();
        cross.facade_wall = None;
        assert!(has(&floating_cross, "invalid_roof_child_front"));

        let mut unsplit_cross = cross_fixture();
        let middle = unsplit_cross.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::CrossGable && child.child.0 >= 1_000)
            .unwrap()
            .split_eave_edges[1];
        unsplit_cross.roof_assemblies[0]
            .edges
            .iter_mut()
            .find(|edge| edge.id == middle)
            .unwrap()
            .kind = RoofEdgeKind::Eave;
        assert!(has(&unsplit_cross, "invalid_roof_child_front"));

        let abutment_fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::Cathedral,
                47,
            ))
            .unwrap()
        };
        let abutment_plan = abutment_fixture();
        let wall_abutment_count = abutment_plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.abutments)
            .filter(|abutment| abutment.kind == crate::RoofAbutmentKind::Wall)
            .count();
        assert_eq!(wall_abutment_count, 2);

        let mut missing_clerestory_host = abutment_fixture();
        let clerestory = missing_clerestory_host
            .wall_assemblies
            .iter()
            .find(|wall| matches!(wall.source, crate::WallSourceId::ChurchArcade { .. }))
            .unwrap()
            .id;
        missing_clerestory_host
            .wall_assemblies
            .retain(|wall| wall.id != clerestory);
        assert!(has(
            &missing_clerestory_host,
            "invalid_roof_abutment_contour"
        ));

        let mut shifted_clerestory_face = abutment_fixture();
        let clerestory = shifted_clerestory_face
            .wall_assemblies
            .iter_mut()
            .find(|wall| matches!(wall.source, crate::WallSourceId::ChurchArcade { .. }))
            .unwrap();
        clerestory.frame.origin += clerestory.frame.outward * 0.40;
        assert!(has(
            &shifted_clerestory_face,
            "invalid_roof_abutment_contour"
        ));

        let mut contour_gap = abutment_fixture();
        contour_gap.roof_assemblies[0].abutments[0]
            .samples
            .remove(1);
        assert!(has(&contour_gap, "invalid_roof_abutment_contour"));

        let mut raised_flashing = abutment_fixture();
        let upstand = raised_flashing.roof_assemblies[0].abutments[0].samples[0].upstand_solid;
        raised_flashing
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == upstand)
            .unwrap()
            .centre
            .y += 0.5;
        assert!(has(&raised_flashing, "invalid_roof_abutment_contour"));

        let mut no_lower_outlet = abutment_fixture();
        let outlet = no_lower_outlet.roof_assemblies[0].abutments[0].lower_outlet;
        no_lower_outlet
            .resolved_geometry
            .voids
            .retain(|void| void.id != outlet);
        assert!(has(&no_lower_outlet, "invalid_roof_abutment_contour"));

        let mut wrong_tower_host = abutment_fixture();
        let ordinary_wall = wrong_tower_host
            .wall_assemblies
            .iter()
            .find(|wall| !matches!(wall.source, crate::WallSourceId::SquareTowerFace { .. }))
            .unwrap()
            .id;
        wrong_tower_host.roof_assemblies[0]
            .abutments
            .iter_mut()
            .find(|abutment| abutment.kind == crate::RoofAbutmentKind::Tower)
            .unwrap()
            .samples[0]
            .host_wall = ordinary_wall;
        assert!(has(&wrong_tower_host, "invalid_roof_abutment_contour"));
    }

    #[test]
    fn wall_opening_audit_rejects_authority_profile_support_and_operability_drift() {
        let fixture =
            |archetype| crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap();
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut thickness = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        thickness.wall_assemblies[0].thickness_metres = 0.08;
        assert!(has(&thickness, "wall_profile_thickness"));

        let mut duplicate = fixture(crate::BuildingArchetype::TownHouse);
        duplicate.wall_assemblies[1].source = duplicate.wall_assemblies[0].source;
        assert!(has(&duplicate, "invalid_wall_authority"));

        let mut host = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        host.wall_assemblies[0].host_solids.clear();
        assert!(has(&host, "invalid_wall_host_union"));

        let mut shallow = fixture(crate::BuildingArchetype::Cathedral);
        let opening = shallow.opening_assemblies[0].clone();
        let void = shallow
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == opening.void_id)
            .unwrap();
        if opening.frame.outward.x.abs() > 0.5 {
            let middle = (void.bounds.min.x + void.bounds.max.x) * 0.5;
            void.bounds.min.x = middle - 0.01;
            void.bounds.max.x = middle + 0.01;
        } else {
            let middle = (void.bounds.min.z + void.bounds.max.z) * 0.5;
            void.bounds.min.z = middle - 0.01;
            void.bounds.max.z = middle + 0.01;
        }
        assert!(has(&shallow, "shallow_wall_opening"));

        let mut substituted = fixture(crate::BuildingArchetype::Cathedral);
        substituted.opening_assemblies[0].profile = crate::OpeningProfile::Rectangular {
            width_metres: 1.0,
            height_metres: 2.0,
        };
        assert!(has(&substituted, "opening_head_profile_mismatch"));

        let mut reveal = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        reveal.opening_assemblies[0].reveal_surfaces.pop();
        assert!(has(&reveal, "missing_opening_reveal_piece"));

        let mut timber_frame = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let timber_wall = timber_frame
            .wall_assemblies
            .iter()
            .find(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
            .unwrap()
            .id;
        timber_frame
            .timber_frame
            .as_mut()
            .unwrap()
            .bays
            .retain(|bay| bay.wall != Some(timber_wall));
        assert!(has(&timber_frame, "missing_authoritative_timber_frame"));

        let mut bearing = fixture(crate::BuildingArchetype::Cathedral);
        let head = bearing.opening_assemblies[0].head_node;
        bearing
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == head)
            .unwrap()
            .supported_by
            .pop();
        assert!(has(&bearing, "false_opening_head_load_path"));

        let mut military = fixture(crate::BuildingArchetype::WalledKeep);
        let military_index = military
            .opening_assemblies
            .iter()
            .position(|opening| opening.use_kind == crate::OpeningUse::GunLoop)
            .unwrap();
        military.opening_assemblies[military_index].closure.layers =
            vec![crate::ClosureKind::LeadedGlazing];
        military.opening_assemblies[military_index]
            .ray_indices
            .pop();
        assert!(has(&military, "illegal_opening_closure"));
        assert!(has(&military, "inoperable_military_opening"));

        let mut false_splay = fixture(crate::BuildingArchetype::WalledKeep);
        let opening = false_splay
            .opening_assemblies
            .iter()
            .find(|opening| opening.use_kind == crate::OpeningUse::GunLoop)
            .unwrap()
            .clone();
        false_splay
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == opening.jamb_solids[0])
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&false_splay, "false_splayed_wall_opening"));

        let mut flat_military_head = fixture(crate::BuildingArchetype::WalledKeep);
        let opening = flat_military_head
            .opening_assemblies
            .iter()
            .find(|opening| opening.use_kind == crate::OpeningUse::GunLoop)
            .unwrap()
            .clone();
        flat_military_head
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == opening.head_solid)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&flat_military_head, "false_splayed_wall_opening"));

        for mutation in ["missing_spandrel", "moved_spandrel"] {
            let mut plan = fixture(crate::BuildingArchetype::Cathedral);
            let opening = plan.opening_assemblies[0].clone();
            if mutation == "missing_spandrel" {
                plan.resolved_geometry
                    .solids
                    .retain(|solid| solid.id != opening.spandrel_solid);
            } else {
                plan.resolved_geometry
                    .solids
                    .iter_mut()
                    .find(|solid| solid.id == opening.spandrel_solid)
                    .unwrap()
                    .centre
                    .y += 0.25;
            }
            assert!(has(&plan, "false_opening_head_load_path"), "{mutation}");
        }

        let mut hanging_tracery = fixture(crate::BuildingArchetype::Cathedral);
        let opening = hanging_tracery
            .opening_assemblies
            .iter()
            .find(|opening| opening.tracery_node.is_some())
            .unwrap()
            .clone();
        let mullion = hanging_tracery
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == opening.owner && solid.role == SolidRole::Mullion)
            .unwrap();
        mullion.centre.y += 0.20;
        assert!(has(&hanging_tracery, "unsupported_cathedral_tracery"));

        let military_opening = |plan: &crate::BuildingPlan| {
            plan.opening_assemblies
                .iter()
                .position(|opening| {
                    matches!(
                        opening.use_kind,
                        crate::OpeningUse::ArrowLoop | crate::OpeningUse::GunLoop
                    )
                })
                .unwrap()
        };
        let mut filled_middle = fixture(crate::BuildingArchetype::WalledKeep);
        let index = military_opening(&filled_middle);
        filled_middle.opening_assemblies[index].sectional_void[4].width_metres = 0.01;
        assert!(has(&filled_middle, "false_splayed_wall_opening"));

        let mut exterior_mouth = fixture(crate::BuildingArchetype::WalledKeep);
        let index = military_opening(&exterior_mouth);
        let mouth = exterior_mouth.opening_assemblies[index]
            .sectional_void
            .last()
            .unwrap()
            .width_metres;
        exterior_mouth.opening_assemblies[index].sectional_void[0].width_metres = mouth;
        assert!(has(&exterior_mouth, "false_splayed_wall_opening"));

        let mut reversed = fixture(crate::BuildingArchetype::WalledKeep);
        let index = military_opening(&reversed);
        reversed.opening_assemblies[index].sectional_void.reverse();
        assert!(has(&reversed, "false_splayed_wall_opening"));

        let mut flat_sill = fixture(crate::BuildingArchetype::Cathedral);
        let sill_id =
            flat_sill.opening_assemblies[0]
                .reveal_surfaces
                .iter()
                .copied()
                .find(|id| {
                    flat_sill.resolved_geometry.surfaces.iter().any(|surface| {
                        surface.id == *id && surface.role == SurfaceRole::WeatherSill
                    })
                })
                .unwrap();
        flat_sill
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == sill_id)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::Planar;
        assert!(has(&flat_sill, "invalid_opening_weather_or_intrados"));

        let mut flat_intrados = fixture(crate::BuildingArchetype::Cathedral);
        let intrados_id = flat_intrados.opening_assemblies[0]
            .reveal_surfaces
            .iter()
            .copied()
            .find(|id| {
                flat_intrados
                    .resolved_geometry
                    .surfaces
                    .iter()
                    .any(|surface| surface.id == *id && surface.role == SurfaceRole::Intrados)
            })
            .unwrap();
        flat_intrados
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == intrados_id)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::Planar;
        assert!(has(&flat_intrados, "invalid_opening_weather_or_intrados"));

        for mutation in ["raised_head", "short_head", "shifted_jamb", "thin_pier"] {
            let mut plan = fixture(crate::BuildingArchetype::Cathedral);
            let opening = plan.opening_assemblies[0].clone();
            match mutation {
                "raised_head" => {
                    plan.resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.head_solid)
                        .unwrap()
                        .centre
                        .y += 0.25
                }
                "short_head" => {
                    let head = plan
                        .resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.head_solid)
                        .unwrap();
                    if opening.frame.tangent.x.abs() > 0.5 {
                        head.size.x -= 0.35;
                    } else {
                        head.size.z -= 0.35;
                    }
                }
                "shifted_jamb" => {
                    plan.resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.jamb_solids[0])
                        .unwrap()
                        .centre += Vec3::new(
                        opening.frame.tangent.x * 0.25,
                        0.0,
                        opening.frame.tangent.y * 0.25,
                    )
                }
                "thin_pier" => {
                    let jamb = plan
                        .resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.jamb_solids[0])
                        .unwrap();
                    if opening.frame.tangent.x.abs() > 0.5 {
                        jamb.size.x = 0.02;
                    } else {
                        jamb.size.z = 0.02;
                    }
                }
                _ => unreachable!(),
            }
            assert!(has(&plan, "false_opening_head_load_path"), "{mutation}");
        }
        let mut exterior_fin = fixture(crate::BuildingArchetype::CourtyardCastle);
        let exterior_opening = exterior_fin
            .opening_assemblies
            .iter()
            .find(|opening| opening.frame.outside_room.is_none())
            .unwrap()
            .clone();
        exterior_fin
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == exterior_opening.jamb_solids[0])
            .unwrap()
            .centre += Vec3::new(
            exterior_opening.frame.outward.x * 0.10,
            0.0,
            exterior_opening.frame.outward.y * 0.10,
        );
        assert!(has(&exterior_fin, "discontinuous_exterior_wall_face"));

        let mut radial = fixture(crate::BuildingArchetype::WalledKeep);
        let radial_wall = radial
            .wall_assemblies
            .iter()
            .find(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { .. }))
            .unwrap()
            .clone();
        radial
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == radial_wall.host_solids[0])
            .unwrap()
            .shape = crate::ResolvedSolidShape::RoundTowerShell {
            outer_radius_metres: 3.0,
            inner_radius_metres: 2.7,
            chord_interfaces: [None, None],
        };
        assert!(has(&radial, "invalid_round_wall_authority"));

        let mut legacy = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        legacy.opening_assemblies.pop();
        assert!(has(&legacy, "legacy_opening_not_migrated"));
    }

    #[test]
    fn projected_defense_audit_rejects_operability_support_and_phase_regressions() {
        let fixture = |seed| {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::CastleGatehouse,
                seed,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };
        let plan = fixture(149);
        let breteche_plan = fixture(201);
        let deployed_plan = fixture(202);
        let bartizan_plan = fixture(203);
        assert!(audit_plan(&plan).is_empty(), "{:?}", audit_plan(&plan));
        assert!(
            audit_plan(&breteche_plan).is_empty(),
            "{:?}",
            audit_plan(&breteche_plan)
        );
        assert!(
            audit_plan(&deployed_plan).is_empty(),
            "{:?}",
            audit_plan(&deployed_plan)
        );
        assert!(
            audit_plan(&bartizan_plan).is_empty(),
            "{:?}",
            audit_plan(&bartizan_plan)
        );
        assert!(
            plan.projected_defenses
                .iter()
                .any(|defense| { defense.kind == ProjectedDefenseKind::Machicolation })
        );
        assert!(
            breteche_plan
                .projected_defenses
                .iter()
                .any(|defense| { defense.kind == ProjectedDefenseKind::Breteche })
        );
        assert!(
            bartizan_plan
                .projected_defenses
                .iter()
                .any(|defense| { defense.kind == ProjectedDefenseKind::Bartizan })
        );
        assert!(plan.projected_defenses.iter().any(|defense| {
            defense.kind == ProjectedDefenseKind::Hoarding
                && defense.deployment == ProjectedDefenseDeployment::SocketsOnly
        }));
        assert!(deployed_plan.projected_defenses.iter().any(|defense| {
            defense.kind == ProjectedDefenseKind::Hoarding
                && defense.deployment == ProjectedDefenseDeployment::Deployed
        }));
        let orientations = [&plan, &breteche_plan, &deployed_plan]
            .into_iter()
            .flat_map(|plan| plan.projected_defenses.iter())
            .filter_map(|defense| match defense.path {
                ProjectedDefensePath::Linear { outward, .. } => Some(outward),
                ProjectedDefensePath::Round { .. } => None,
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(orientations.len(), 4);

        let operational_index = plan
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Machicolation)
            .unwrap();
        let bartizan_index = bartizan_plan
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Bartizan)
            .unwrap();
        let deployed_hoarding_index = deployed_plan
            .projected_defenses
            .iter()
            .position(|defense| {
                defense.kind == ProjectedDefenseKind::Hoarding
                    && defense.deployment == ProjectedDefenseDeployment::Deployed
            })
            .unwrap();

        let mut witness_only_host = fixture(149);
        witness_only_host.projected_defenses[operational_index].host_owner =
            witness_only_host.projected_defenses[operational_index].owner;
        assert!(has(&witness_only_host, "unresolved_projected_defense_host"));

        let mut intact_host_portal = fixture(149);
        let host_portal = intact_host_portal.projected_defenses[operational_index]
            .host_portal_void
            .unwrap();
        intact_host_portal
            .resolved_geometry
            .voids
            .retain(|void| void.id != host_portal);
        assert!(has(
            &intact_host_portal,
            "unresolved_projected_defense_host"
        ));

        let mut shifted_host_walk = fixture(149);
        let host_walk = shifted_host_walk.projected_defenses[operational_index].host_walk_solid;
        shifted_host_walk
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == host_walk)
            .unwrap()
            .centre += Vec3::new(2.0, 0.0, 2.0);
        assert!(has(&shifted_host_walk, "inaccessible_projected_defense"));

        let mut untrimmed_host_run = fixture(149);
        if let ProjectedDefensePath::Linear { start, end, .. } =
            &mut untrimmed_host_run.projected_defenses[operational_index].path
        {
            let tangent = (*end - *start).normalize_or_zero();
            *start -= tangent;
            *end += tangent;
        }
        assert!(has(
            &untrimmed_host_run,
            "unresolved_projected_defense_host"
        ));

        let mut blocked_tower_chord = fixture(149);
        let tower_index = blocked_tower_chord
            .towers
            .iter()
            .position(|tower| tower.chord_interface.is_some())
            .unwrap();
        blocked_tower_chord.towers[tower_index]
            .chord_interface
            .as_mut()
            .unwrap()
            .bearing_depth = crate::GridLength::new(1).unwrap();
        assert!(has(&blocked_tower_chord, "blocked_projected_defense_ray"));

        let mut missing_return_chord = fixture(202);
        missing_return_chord.towers[0].secondary_chord_interface = None;
        assert!(has(&missing_return_chord, "invalid_round_wall_authority"));

        let sockets_index = plan
            .projected_defenses
            .iter()
            .position(|defense| defense.deployment == ProjectedDefenseDeployment::SocketsOnly)
            .unwrap();
        let mut filled_socket = fixture(149);
        let socket = filled_socket.projected_defenses[sockets_index].beam_socket_voids[0];
        filled_socket
            .resolved_geometry
            .voids
            .retain(|void| void.id != socket);
        assert!(has(&filled_socket, "invalid_hoarding_beam_sockets"));

        let mut shifted_joist = fixture(202);
        let (_, joist) = shifted_joist.projected_defenses[deployed_hoarding_index].socket_joists[0];
        shifted_joist
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == joist)
            .unwrap()
            .centre += Vec3::new(2.0, 0.0, 2.0);
        assert!(has(&shifted_joist, "invalid_hoarding_beam_sockets"));

        let mut sealed = fixture(149);
        let throat = sealed.projected_defenses[operational_index].throat_voids[0];
        sealed
            .resolved_geometry
            .voids
            .retain(|void| void.id != throat);
        assert!(has(&sealed, "sealed_projected_defense_throat"));

        let mut no_ray = fixture(149);
        let owner = no_ray.projected_defenses[operational_index].owner;
        no_ray
            .resolved_geometry
            .projected_defense_rays
            .retain(|ray| ray.owner != owner);
        assert!(has(&no_ray, "sealed_projected_defense_throat"));

        let mut below_floor_ray = fixture(149);
        let owner = below_floor_ray.projected_defenses[operational_index].owner;
        below_floor_ray
            .resolved_geometry
            .projected_defense_rays
            .iter_mut()
            .find(|ray| ray.owner == owner)
            .unwrap()
            .origin
            .y = below_floor_ray.projected_defenses[operational_index].floor_elevation_metres - 0.2;
        assert!(has(&below_floor_ray, "blocked_projected_defense_ray"));

        let mut missing_far_range = fixture(149);
        let owner = missing_far_range.projected_defenses[operational_index].owner;
        let aperture = missing_far_range
            .resolved_geometry
            .projected_defense_working_points
            .iter()
            .find(|point| point.owner == owner)
            .unwrap()
            .aperture;
        missing_far_range
            .resolved_geometry
            .projected_defense_rays
            .retain(|ray| {
                !(ray.owner == owner
                    && ray.throat == aperture
                    && ray.range == crate::ProjectedDefenseRange::Far)
            });
        assert!(has(
            &missing_far_range,
            "inoperable_projected_defense_station"
        ));

        let mut inward = fixture(149);
        let owner = inward.projected_defenses[operational_index].owner;
        let ray = inward
            .resolved_geometry
            .projected_defense_rays
            .iter_mut()
            .find(|ray| ray.owner == owner)
            .unwrap();
        ray.target.z = ray.origin.z + 2.0;
        assert!(has(&inward, "blocked_projected_defense_ray"));

        let mut reversed_assembly = fixture(149);
        if let ProjectedDefensePath::Linear { outward, .. } =
            &mut reversed_assembly.projected_defenses[operational_index].path
        {
            *outward = outward.opposite();
        }
        assert!(has(&reversed_assembly, "inward_projected_defense"));

        let mut blocked = fixture(149);
        let owner = blocked.projected_defenses[operational_index].owner;
        let ray = *blocked
            .resolved_geometry
            .projected_defense_rays
            .iter()
            .find(|ray| ray.owner == owner)
            .unwrap();
        let blocker = blocked
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner != owner)
            .unwrap();
        blocker.centre = ray.origin.lerp(ray.target, 0.45);
        blocker.size = Vec3::splat(0.45);
        assert!(has(&blocked, "blocked_projected_defense_ray"));

        let mut no_support = fixture(149);
        let support = no_support.projected_defenses[operational_index].support_nodes[0];
        no_support
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == support)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(&no_support, "unsupported_projected_defense"));

        let mut inadequate_bearing = fixture(149);
        let support = inadequate_bearing.projected_defenses[operational_index].support_nodes[0];
        let bearing = inadequate_bearing
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|bearing| bearing.node == support)
            .unwrap();
        bearing.bounds.max.x = bearing.bounds.min.x + 0.02;
        bearing.bounds.max.z = bearing.bounds.min.z + 0.02;
        assert!(has(&inadequate_bearing, "unsupported_projected_defense"));

        let mut no_portal = fixture(149);
        no_portal.projected_defenses[operational_index].access_portal = None;
        assert!(has(&no_portal, "inaccessible_projected_defense"));
        let mut no_landing = fixture(149);
        no_landing.projected_defenses[operational_index].access_landing = None;
        assert!(has(&no_landing, "inaccessible_projected_defense"));
        let mut narrow = fixture(149);
        narrow.projected_defenses[operational_index].clear_width_metres = 0.7;
        assert!(has(&narrow, "insufficient_projected_defense_clearance"));
        let mut low = fixture(149);
        low.projected_defenses[operational_index].clear_height_metres = 1.7;
        assert!(has(&low, "insufficient_projected_defense_clearance"));

        let mut gallery_overlap = fixture(149);
        let throat = gallery_overlap.projected_defenses[operational_index].throat_voids[0];
        let bounds = gallery_overlap
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == throat)
            .unwrap()
            .bounds;
        let floor_id = gallery_overlap.projected_defenses[operational_index].floor_solids[0];
        let floor = gallery_overlap
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap();
        floor.centre = (bounds.min + bounds.max) * 0.5;
        floor.size = (bounds.max - bounds.min) + Vec3::splat(0.1);
        assert!(has(&gallery_overlap, "unresolved_void_subtraction"));

        let mut closed_bartizan = fixture(203);
        let centre = match closed_bartizan.projected_defenses[bartizan_index].path {
            ProjectedDefensePath::Round { centre, .. } => centre,
            ProjectedDefensePath::Linear { .. } => unreachable!(),
        };
        let floor_id = closed_bartizan.projected_defenses[bartizan_index].floor_solids[0];
        let floor = closed_bartizan
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap();
        floor.centre = Vec3::new(
            centre.x,
            closed_bartizan.projected_defenses[bartizan_index].floor_elevation_metres + 1.0,
            centre.y,
        );
        floor.size = Vec3::splat(0.5);
        assert!(has(&closed_bartizan, "closed_bartizan"));

        let mut dangling_frame = fixture(202);
        let owner = dangling_frame.projected_defenses[deployed_hoarding_index].owner;
        dangling_frame
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::FrameMember)
            .unwrap()
            .supported_by = vec![crate::StructuralNodeId(u64::MAX)];
        assert!(has(&dangling_frame, "dangling_hoarding_frame"));

        let mut no_drain = fixture(149);
        no_drain.projected_defenses[operational_index].drain_route = None;
        assert!(has(&no_drain, "projected_defense_roof_drain_failure"));
        let mut flat_gallery = fixture(149);
        let floor = flat_gallery.projected_defenses[operational_index].floor_solids[0];
        let floor = flat_gallery
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor)
            .unwrap();
        floor.crossfall_radians = 0.0;
        floor.longfall_radians = 0.0;
        assert!(has(&flat_gallery, "projected_defense_roof_drain_failure"));
        let mut raised_channel = fixture(149);
        let catchment = raised_channel.projected_defenses[operational_index].drainage_catchments[0];
        let channel = raised_channel
            .resolved_geometry
            .drainage_catchments
            .iter()
            .find(|candidate| candidate.id == catchment)
            .unwrap()
            .toe_channel_solids[0];
        raised_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .centre
            .y += 0.2;
        assert!(has(&raised_channel, "projected_defense_roof_drain_failure"));
        let mut reversed_channel = fixture(149);
        let catchment =
            reversed_channel.projected_defenses[operational_index].drainage_catchments[0];
        let channel = reversed_channel
            .resolved_geometry
            .drainage_catchments
            .iter()
            .find(|candidate| candidate.id == catchment)
            .unwrap()
            .toe_channel_solids[0];
        reversed_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .longfall_radians *= -1.0;
        assert!(has(
            &reversed_channel,
            "projected_defense_roof_drain_failure"
        ));
        let mut throat_drain = fixture(149);
        let drain = throat_drain.projected_defenses[operational_index]
            .drain_route
            .unwrap();
        let throat = throat_drain.projected_defenses[operational_index].throat_voids[0];
        throat_drain
            .resolved_geometry
            .drainage_routes
            .iter_mut()
            .find(|route| route.id == drain)
            .unwrap()
            .outlet_void = throat;
        assert!(has(&throat_drain, "projected_defense_roof_drain_failure"));
        let mut no_roof = fixture(202);
        let owner = no_roof.projected_defenses[deployed_hoarding_index].owner;
        no_roof
            .resolved_geometry
            .solids
            .retain(|solid| !(solid.owner == owner && solid.role == SolidRole::DefenseRoof));
        assert!(has(&no_roof, "projected_defense_roof_drain_failure"));
        let mut flat_roof = fixture(202);
        let owner = flat_roof.projected_defenses[deployed_hoarding_index].owner;
        let roof = flat_roof
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::DefenseRoof)
            .unwrap();
        roof.crossfall_radians = 0.0;
        roof.longfall_radians = 0.0;
        assert!(has(&flat_roof, "projected_defense_roof_drain_failure"));
        let mut phase_mismatch = fixture(202);
        phase_mismatch.projected_defenses[deployed_hoarding_index].material =
            ProjectedDefenseMaterial::Masonry;
        assert!(has(
            &phase_mismatch,
            "projected_defense_phase_material_mismatch"
        ));
        let mut no_aperture = fixture(203);
        no_aperture.projected_defenses[bartizan_index]
            .firing_apertures
            .clear();
        assert!(has(&no_aperture, "closed_bartizan"));
        let mut wrong_target = fixture(201);
        let index = wrong_target
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Breteche)
            .unwrap();
        wrong_target.projected_defenses[index].tactical_target =
            ProjectedDefenseTarget::CampaignSiegeFront;
        assert!(has(
            &wrong_target,
            "projected_defense_tactical_target_mismatch"
        ));

        let mut duplicate_host_screen = fixture(149);
        let defense = &mut duplicate_host_screen.projected_defenses[operational_index];
        let mut duplicate = duplicate_host_screen
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == defense.host_wall_solids[0])
            .unwrap()
            .clone();
        duplicate.id = crate::ResolvedItemId(u64::MAX - 20);
        defense.host_wall_solids.push(duplicate.id);
        duplicate_host_screen
            .resolved_geometry
            .solids
            .push(duplicate);
        assert!(has(
            &duplicate_host_screen,
            "unresolved_projected_defense_host"
        ));

        let mut overheight_host = fixture(149);
        let host = overheight_host.projected_defenses[operational_index].host_wall_solids[0];
        let host = overheight_host
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == host)
            .unwrap();
        host.centre.y += 0.5;
        host.size.y += 1.0;
        assert!(has(&overheight_host, "unresolved_projected_defense_host"));

        let mut roof_intrusion = fixture(202);
        let host_id =
            roof_intrusion.projected_defenses[deployed_hoarding_index].host_wall_solids[0];
        let host = roof_intrusion
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == host_id)
            .unwrap()
            .clone();
        let owner = roof_intrusion.projected_defenses[deployed_hoarding_index].owner;
        let roof = roof_intrusion
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::DefenseRoof)
            .unwrap();
        roof.centre = host.centre;
        roof.size = host.size;
        assert!(has(&roof_intrusion, "unresolved_projected_defense_host"));

        let mut one_face_bartizan = fixture(203);
        one_face_bartizan.projected_defenses[bartizan_index].host_topology =
            crate::ProjectedDefenseHostTopology::LinearFace;
        one_face_bartizan.projected_defenses[bartizan_index]
            .host_buttress_solids
            .clear();
        assert!(has(&one_face_bartizan, "unresolved_projected_defense_host"));

        let mut disconnected_roof = fixture(202);
        let catchment =
            disconnected_roof.projected_defenses[deployed_hoarding_index].weather_catchments[0];
        disconnected_roof
            .resolved_geometry
            .drainage_catchments
            .retain(|candidate| candidate.id != catchment);
        assert!(has(
            &disconnected_roof,
            "projected_defense_roof_drain_failure"
        ));

        let mut trapped_host_edge = fixture(202);
        let owner = trapped_host_edge.projected_defenses[deployed_hoarding_index].owner;
        trapped_host_edge
            .resolved_geometry
            .solids
            .retain(|solid| !(solid.owner == owner && solid.role == SolidRole::RoofFlashing));
        assert!(has(
            &trapped_host_edge,
            "projected_defense_roof_drain_failure"
        ));

        let mut missing_coping = fixture(149);
        let owner = missing_coping.projected_defenses[operational_index].owner;
        missing_coping
            .resolved_geometry
            .solids
            .retain(|solid| !(solid.owner == owner && solid.role == SolidRole::Coping));
        assert!(has(&missing_coping, "projected_defense_roof_drain_failure"));

        let mut inward_drip = fixture(149);
        let owner = inward_drip.projected_defenses[operational_index].owner;
        inward_drip
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::Coping)
            .unwrap()
            .crossfall_radians *= -1.0;
        assert!(has(&inward_drip, "projected_defense_roof_drain_failure"));

        let breteche_index = fixture(201)
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Breteche)
            .unwrap();
        let mut raised_breteche_roof = fixture(201);
        let owner = raised_breteche_roof.projected_defenses[breteche_index].owner;
        raised_breteche_roof
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::DefenseRoof)
            .unwrap()
            .centre
            .y += 0.35;
        assert!(has(
            &raised_breteche_roof,
            "unsupported_projected_defense_roof"
        ));

        let mut removed_breteche_post = fixture(201);
        let post = removed_breteche_post.projected_defenses[breteche_index]
            .roof_support_solids
            .iter()
            .copied()
            .find(|id| {
                removed_breteche_post
                    .resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::FrameMember)
            })
            .unwrap();
        removed_breteche_post
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != post);
        assert!(has(
            &removed_breteche_post,
            "unsupported_projected_defense_roof"
        ));

        let mut shortened_breteche_post = fixture(201);
        let post = shortened_breteche_post.projected_defenses[breteche_index]
            .roof_support_solids
            .iter()
            .copied()
            .find(|id| {
                shortened_breteche_post
                    .resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::FrameMember)
            })
            .unwrap();
        shortened_breteche_post
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == post)
            .unwrap()
            .size
            .y -= 0.4;
        assert!(has(
            &shortened_breteche_post,
            "unsupported_projected_defense_roof"
        ));

        let mut shifted_breteche_post = fixture(201);
        let post = shifted_breteche_post.projected_defenses[breteche_index]
            .roof_support_solids
            .iter()
            .copied()
            .find(|id| {
                shifted_breteche_post
                    .resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::FrameMember)
            })
            .unwrap();
        shifted_breteche_post
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == post)
            .unwrap()
            .centre
            .x += 0.55;
        assert!(has(
            &shifted_breteche_post,
            "unsupported_projected_defense_roof"
        ));
    }

    #[test]
    fn audit_rejects_a_disconnected_or_vertically_broken_fighting_circuit() {
        let mut disconnected = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            51,
        ))
        .unwrap();
        disconnected.defensive_junctions.clear();
        assert!(
            audit_plan(&disconnected)
                .iter()
                .any(|issue| issue.code == "disconnected_defensive_circuit")
        );

        // Each isolated deck still has its own ground stair. A multi-source
        // reachability pass would incorrectly accept this as one circuit.
        let mut separately_accessible = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            56,
        ))
        .unwrap();
        separately_accessible.wall_walks = separately_accessible
            .wall_walks
            .iter()
            .copied()
            .filter(|walk| matches!(walk, WallWalk::Round { .. }))
            .take(2)
            .collect();
        separately_accessible.defensive_junctions.clear();
        separately_accessible.defensive_circuits = vec![DefensiveCircuit {
            label: "deliberately broken test circuit".to_owned(),
            walks: vec![0, 1],
        }];
        separately_accessible.battlements.clear();
        separately_accessible.towers.truncate(2);
        separately_accessible.stairs.retain(|stair| {
            separately_accessible.towers.iter().any(|tower| {
                matches!(
                    stair,
                    Stair::Spiral { centre, .. } if close_vec(*centre, tower.centre_metres())
                )
            })
        });
        assert!(
            audit_plan(&separately_accessible)
                .iter()
                .any(|issue| issue.code == "disconnected_defensive_circuit")
        );

        let mut broken = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            52,
        ))
        .unwrap();
        let junction = broken.defensive_junctions.first_mut().unwrap();
        junction.kind = DefensiveJunctionKind::LevelLanding;
        if let WallWalk::Linear {
            elevation_metres, ..
        } = &mut broken.wall_walks[junction.walk_a]
        {
            *elevation_metres += 0.5;
        } else if let WallWalk::Linear {
            elevation_metres, ..
        } = &mut broken.wall_walks[junction.walk_b]
        {
            *elevation_metres += 0.5;
        }
        assert!(
            audit_plan(&broken)
                .iter()
                .any(|issue| issue.code == "wall_walk_vertical_discontinuity")
        );

        let mut cramped = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            54,
        ))
        .unwrap();
        cramped.defensive_junctions[0].width_metres = 0.7;
        cramped.defensive_junctions[0].clear_height_metres = 1.7;
        assert!(
            audit_plan(&cramped)
                .iter()
                .any(|issue| issue.code == "insufficient_walk_clearance")
        );
    }

    #[test]
    fn audit_rejects_a_roof_intruding_into_the_wall_walk() {
        let mut plan = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            53,
        ))
        .unwrap();
        plan.roofs[0].centre.y = 0.4;
        assert!(
            audit_plan(&plan)
                .iter()
                .any(|issue| issue.code == "wall_walk_roof_obstruction")
        );
    }

    #[test]
    fn audit_requires_physical_portals_behind_tower_graph_edges() {
        let mut plan = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            57,
        ))
        .unwrap();
        let portal_index = plan
            .tower_portals
            .iter()
            .position(|portal| matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. }))
            .unwrap();
        plan.tower_portals.remove(portal_index);
        assert!(
            audit_plan(&plan)
                .iter()
                .any(|issue| issue.code == "missing_tower_portal")
        );

        let mut no_entrance = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            58,
        ))
        .unwrap();
        no_entrance.tower_portals.retain(|portal| {
            !(portal.tower_index == 0 && portal.kind == TowerPortalKind::GroundStairEntrance)
        });
        assert!(
            audit_plan(&no_entrance)
                .iter()
                .any(|issue| issue.code == "missing_tower_portal")
        );
    }

    #[test]
    fn audit_enforces_the_declared_walled_keep_defensive_profile() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::WalledKeep,
                55,
            ))
            .unwrap()
        };
        let mut thin = fixture();
        thin.curtain_walls[0].thickness_metres = 0.8;
        thin.towers[0].wall_thickness_metres = 0.7;
        assert!(
            audit_plan(&thin)
                .iter()
                .any(|issue| issue.code == "wall_too_thin_for_profile")
        );

        let mut undefended = fixture();
        let gate_centre =
            (undefended.curtain_walls[0].start + undefended.curtain_walls[0].end) * 0.5;
        undefended
            .towers
            .retain(|tower| (tower.centre_metres() - gate_centre).length() > 8.0);
        assert!(
            audit_plan(&undefended)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut blind = fixture();
        for position in &mut blind.gate_defenses[0].firing_positions {
            position.direction = -position.direction;
        }
        assert!(
            audit_plan(&blind)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut origin_inside = fixture();
        let tower =
            origin_inside.towers[origin_inside.gate_defenses[0].firing_positions[0].tower_index];
        origin_inside.gate_defenses[0].firing_positions[0].origin = tower.centre_metres();
        assert!(
            audit_plan(&origin_inside)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut duplicate_aperture = fixture();
        let duplicate = duplicate_aperture.gate_defenses[0].firing_positions[0];
        duplicate_aperture.gate_defenses[0].firing_positions = vec![duplicate, duplicate];
        assert!(
            audit_plan(&duplicate_aperture)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut missing_aperture = fixture();
        missing_aperture.gate_defenses[0].firing_positions[0].aperture_width_metres = 0.0;
        assert!(
            audit_plan(&missing_aperture)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut rotated_aperture = fixture();
        rotated_aperture.gate_defenses[0].firing_positions[0].aperture_normal =
            -rotated_aperture.gate_defenses[0].firing_positions[0].aperture_normal;
        assert!(
            audit_plan(&rotated_aperture)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut wall_occluded = fixture();
        let firing = wall_occluded.gate_defenses[0].firing_positions[0];
        let target = wall_occluded.gate_defenses[0].approach;
        assert!(ray_clear_of_solids(
            &wall_occluded,
            &firing,
            target,
            wall_occluded.gate_defenses[0].curtain_wall_index,
        ));
        let midpoint = firing.origin.lerp(target, 0.45);
        let perpendicular = Vec2::new(
            -(target - firing.origin).normalize().y,
            (target - firing.origin).normalize().x,
        );
        let mut blocker = wall_occluded.curtain_walls[1];
        blocker.start = midpoint - perpendicular;
        blocker.end = midpoint + perpendicular;
        blocker.height_metres = 3.0;
        blocker.thickness_metres = 0.6;
        blocker.gate_width_metres = None;
        wall_occluded.curtain_walls.push(blocker);
        assert!(!ray_clear_of_solids(
            &wall_occluded,
            &firing,
            target,
            wall_occluded.gate_defenses[0].curtain_wall_index,
        ));
        assert!(
            audit_plan(&wall_occluded)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut single_closure = fixture();
        single_closure.gate_defenses[0]
            .closures
            .retain(|closure| closure.kind != GateClosureKind::Portcullis);
        assert!(
            audit_plan(&single_closure)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut unsupported_chamber = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = &mut unsupported_chamber.gate_defenses[0].guard_chamber.load_path;
        *left_tower_index = usize::MAX;
        assert!(
            audit_plan(&unsupported_chamber)
                .iter()
                .any(|issue| issue.code == "unsupported_guard_chamber")
        );

        let mut inaccessible_chamber = fixture();
        inaccessible_chamber.gate_defenses[0]
            .guard_chamber
            .access
            .envelope
            .width_metres = 0.6;
        assert!(
            audit_plan(&inaccessible_chamber)
                .iter()
                .any(|issue| issue.code == "inaccessible_guard_chamber")
        );

        let mut inoperable_chamber = fixture();
        inoperable_chamber.gate_defenses[0]
            .guard_chamber
            .openings
            .clear();
        assert!(
            audit_plan(&inoperable_chamber)
                .iter()
                .any(|issue| issue.code == "inoperable_guard_chamber")
        );

        let mut misaligned_windlass = fixture();
        misaligned_windlass.gate_defenses[0]
            .guard_chamber
            .operating_positions[0]
            .position = misaligned_windlass.gate_defenses[0].guard_chamber.centre;
        assert!(
            audit_plan(&misaligned_windlass)
                .iter()
                .any(|issue| issue.code == "inoperable_guard_chamber")
        );

        let mut unflanked = fixture();
        unflanked.curtain_walls[1].end.y += 40.0;
        assert!(
            audit_plan(&unflanked)
                .iter()
                .any(|issue| issue.code == "unflanked_curtain")
        );

        let mut thin_courtyard = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            59,
        ))
        .unwrap();
        thin_courtyard.towers[0].wall_thickness_metres = 0.35;
        assert!(
            audit_plan(&thin_courtyard)
                .iter()
                .any(|issue| issue.code == "wall_too_thin_for_profile")
        );
    }

    #[test]
    fn gatehouse_geometry_audit_rejects_passage_room_load_splice_and_aperture_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::WalledKeep,
                67,
            ))
            .unwrap()
        };
        let has =
            |plan: &BuildingPlan, code| audit_plan(plan).iter().any(|issue| issue.code == code);

        let mut blocked_passage = fixture();
        let threshold = blocked_passage.gate_defenses[0].threshold;
        let floor_elevation = blocked_passage.gate_defenses[0]
            .guard_chamber
            .floor_elevation_metres;
        blocked_passage.gate_defenses[0]
            .guard_chamber
            .supports
            .push(crate::GuardChamberSupport {
                centre: threshold,
                size: Vec2::splat(0.5),
                base_elevation_metres: 0.0,
                top_elevation_metres: floor_elevation,
            });
        assert!(has(&blocked_passage, "gate_passage_clear"));

        let mut room_collision = fixture();
        room_collision.gate_defenses[0]
            .guard_chamber
            .floor_elevation_metres = 3.0;
        assert!(has(&room_collision, "room_void_disjoint_from_solids"));

        let mut lateral_room_collision = fixture();
        lateral_room_collision.gate_defenses[0]
            .guard_chamber
            .centre
            .x += 0.4;
        assert!(has(
            &lateral_room_collision,
            "room_void_disjoint_from_solids"
        ));

        let mut deep_room_collision = fixture();
        deep_room_collision.gate_defenses[0].guard_chamber.size.y += 0.4;
        assert!(has(&deep_room_collision, "room_void_disjoint_from_solids"));

        let mut missing_load = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = &mut missing_load.gate_defenses[0].guard_chamber.load_path;
        *left_tower_index = usize::MAX;
        assert!(has(&missing_load, "declared_load_path"));

        let mut lowered_arch = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            arch_spring_elevation_metres,
            ..
        } = &mut lowered_arch.gate_defenses[0].guard_chamber.load_path;
        *arch_spring_elevation_metres -= 0.3;
        assert!(has(&lowered_arch, "gate_passage_clear"));

        let mut thick_arch = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            arch_ring_depth, ..
        } = &mut thick_arch.gate_defenses[0].guard_chamber.load_path;
        *arch_ring_depth = crate::GridLength::new(20).unwrap();
        assert!(has(&thick_arch, "gate_passage_clear"));

        let mut undertrimmed_return = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            curtain_return_bond,
            ..
        } = &mut undertrimmed_return.gate_defenses[0].guard_chamber.load_path;
        *curtain_return_bond = crate::GridLength::new(1).unwrap();
        assert!(has(&undertrimmed_return, "round_rect_splice"));

        let mut short_rectangular_closures = fixture();
        for closure in &mut short_rectangular_closures.gate_defenses[0].closures {
            closure.coverage.spring_height_metres = 3.24;
            closure.coverage.arch_rise_metres = 0.0;
        }
        assert!(has(&short_rectangular_closures, "unsealed_gate_passage"));

        let mut support_in_tower = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = support_in_tower.gate_defenses[0].guard_chamber.load_path;
        let centre = support_in_tower.towers[left_tower_index].centre_metres();
        let floor = support_in_tower.gate_defenses[0]
            .guard_chamber
            .floor_elevation_metres;
        support_in_tower.gate_defenses[0]
            .guard_chamber
            .supports
            .push(crate::GuardChamberSupport {
                centre,
                size: Vec2::splat(0.6),
                base_elevation_metres: 0.0,
                top_elevation_metres: floor,
            });
        assert!(has(&support_in_tower, "undeclared_solid_overlap"));

        let mut bad_splice = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = bad_splice.gate_defenses[0].guard_chamber.load_path;
        bad_splice.towers[left_tower_index].chord_interface = None;
        assert!(has(&bad_splice, "round_rect_splice"));

        let mut unresolved_aperture = fixture();
        unresolved_aperture.gate_defenses[0].firing_positions[0].aperture_width_metres = 0.08;
        assert!(has(&unresolved_aperture, "aperture_clearance"));

        let mut overlap = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = overlap.gate_defenses[0].guard_chamber.load_path;
        let tower = overlap.towers[left_tower_index];
        let shifted_anchor = crate::GridPoint::new(
            tower.anchor().x,
            tower.anchor().z + crate::GRID_UNITS_PER_CELL,
        );
        overlap.towers[left_tower_index] = crate::RoundTower::new(
            shifted_anchor,
            tower.diameter(),
            tower.wall_height_metres,
            tower.wall_thickness_metres,
            tower.roof,
            tower.battlement,
        )
        .expect("a one-cell shift preserves even-diameter anchor parity")
        .with_chord_interface(tower.chord_interface.unwrap());
        assert!(has(&overlap, "undeclared_solid_overlap"));

        let mut drift = fixture();
        drift.curtain_walls[0].gate_width_metres = Some(4.0);
        assert!(has(&drift, "gatehouse_spec_drift"));

        let mut diagonal = fixture();
        diagonal.curtain_walls[0].end.y += 1.0;
        assert!(has(&diagonal, "invalid_gatehouse_orientation"));

        let mut mismatched_outward = fixture();
        mismatched_outward.curtain_walls[0].outward = Direction::East;
        assert!(has(&mismatched_outward, "invalid_gatehouse_orientation"));
    }

    #[test]
    fn guard_access_audit_sweeps_landings_stair_door_roof_and_operating_clearances() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::WalledKeep,
                71,
            ))
            .unwrap()
        };
        let has =
            |plan: &BuildingPlan, code| audit_plan(plan).iter().any(|issue| issue.code == code);
        let mut low_ceiling = fixture();
        low_ceiling.gate_defenses[0]
            .guard_chamber
            .clear_height_metres = 1.5;
        assert!(has(&low_ceiling, "inaccessible_guard_chamber"));
        let mut missing_roof_cut = fixture();
        missing_roof_cut.gate_defenses[0]
            .guard_chamber
            .access
            .roof_clearance_opening
            .size = Vec2::splat(0.2);
        assert!(has(&missing_roof_cut, "inaccessible_guard_chamber"));
        let mut short_door = fixture();
        short_door.gate_defenses[0]
            .guard_chamber
            .access
            .door
            .clear_height_metres = 1.2;
        assert!(has(&short_door, "inaccessible_guard_chamber"));
        let mut small_top = fixture();
        small_top.gate_defenses[0]
            .guard_chamber
            .access
            .top_landing
            .size = Vec2::splat(0.5);
        assert!(has(&small_top, "inaccessible_guard_chamber"));
        let mut absent_bottom = fixture();
        absent_bottom.gate_defenses[0]
            .guard_chamber
            .access
            .bottom_landing
            .size = Vec2::ZERO;
        assert!(has(&absent_bottom, "inaccessible_guard_chamber"));
        let mut short_going = fixture();
        short_going.gate_defenses[0]
            .guard_chamber
            .access
            .flight
            .going_metres = 0.15;
        assert!(has(&short_going, "inaccessible_guard_chamber"));
        let mut hole_collision = fixture();
        let flight_mid = {
            let flight = hole_collision.gate_defenses[0].guard_chamber.access.flight;
            flight.top.lerp(flight.bottom, 0.5)
        };
        hole_collision.gate_defenses[0].guard_chamber.openings[1].position = flight_mid;
        assert!(has(&hole_collision, "inaccessible_guard_chamber"));
        let mut windlass_collision = fixture();
        windlass_collision.gate_defenses[0]
            .guard_chamber
            .operating_positions[0]
            .position = flight_mid;
        assert!(has(&windlass_collision, "inaccessible_guard_chamber"));
        let mut wall_collision = fixture();
        let wall_y = wall_collision.gate_defenses[0].guard_chamber.centre.y;
        wall_collision.gate_defenses[0]
            .guard_chamber
            .access
            .flight
            .top
            .y = wall_y;
        wall_collision.gate_defenses[0]
            .guard_chamber
            .access
            .flight
            .bottom
            .y = wall_y;
        assert!(has(&wall_collision, "inaccessible_guard_chamber"));
        let mut route_drift = fixture();
        route_drift.gate_defenses[0]
            .guard_chamber
            .access
            .door
            .position
            .x += 0.25;
        assert!(has(&route_drift, "gatehouse_spec_drift"));
        let mut unsupported = fixture();
        unsupported.gate_defenses[0]
            .guard_chamber
            .access
            .support_posts
            .clear();
        assert!(has(&unsupported, "unsupported_guard_access"));
        let mut missing_end_guard = fixture();
        missing_end_guard.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards
            .remove(1);
        assert!(has(&missing_end_guard, "unsupported_guard_access"));
        let mut blocked_connection = fixture();
        let top = blocked_connection.gate_defenses[0]
            .guard_chamber
            .access
            .top_landing;
        blocked_connection.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards
            .push(crate::AccessGuardSegment {
                start: top.centre + Vec2::new(0.5, -0.7),
                end: top.centre + Vec2::new(0.5, 0.7),
                elevation_metres: top.elevation_metres,
                height_metres: 1.0,
            });
        assert!(has(&blocked_connection, "unsupported_guard_access"));
        let mut guard_gap = fixture();
        guard_gap.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards[0]
            .end
            .x -= 0.2;
        assert!(has(&guard_gap, "unsupported_guard_access"));
        let mut low_guard = fixture();
        low_guard.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards[0]
            .height_metres = 0.4;
        assert!(has(&low_guard, "unsupported_guard_access"));
        let mut unbraced = fixture();
        unbraced.gate_defenses[0]
            .guard_chamber
            .access
            .lateral_braces
            .clear();
        assert!(has(&unbraced, "unsupported_guard_access"));
        let mut dangling_start = fixture();
        dangling_start.gate_defenses[0]
            .guard_chamber
            .access
            .lateral_braces[0]
            .start_elevation_metres = 20.0;
        assert!(has(&dangling_start, "unsupported_guard_access"));
        let mut dangling_end = fixture();
        dangling_end.gate_defenses[0]
            .guard_chamber
            .access
            .lateral_braces[0]
            .end_elevation_metres = 20.0;
        assert!(has(&dangling_end, "unsupported_guard_access"));
        let mut floating_ledger = fixture();
        floating_ledger.gate_defenses[0]
            .guard_chamber
            .access
            .wall_ledger
            .centre
            .y += 1.0;
        assert!(has(&floating_ledger, "unsupported_guard_access"));
        let mut blocked_swing = fixture();
        blocked_swing.gate_defenses[0]
            .guard_chamber
            .access
            .door
            .swing_inward = false;
        assert!(has(&blocked_swing, "inaccessible_guard_chamber"));
        let mut closure_collision = fixture();
        closure_collision.gate_defenses[0].closures[1].inward_offset_metres = 1.9;
        assert!(has(&closure_collision, "inaccessible_guard_chamber"));
        let mut aperture_collision = fixture();
        aperture_collision.gate_defenses[0].firing_positions[0].origin = flight_mid;
        assert!(has(&aperture_collision, "inaccessible_guard_chamber"));
        let mut sightline_collision = fixture();
        sightline_collision.gate_defenses[0].approach = flight_mid;
        assert!(has(&sightline_collision, "inaccessible_guard_chamber"));
    }

    #[test]
    fn crown_audit_rejects_profile_geometry_junction_and_correspondence_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::CourtyardCastle,
                91,
            ))
            .unwrap()
        };
        let has =
            |plan: &BuildingPlan, code| audit_plan(plan).iter().any(|issue| issue.code == code);

        let mut foot_level = fixture();
        foot_level.crowns[0].profile.breastwork_height_metres = 0.1;
        assert!(has(&foot_level, "unsafe_crown_profile"));
        let mut low_cover = fixture();
        low_cover.crowns[0].profile.merlon_height_metres = 0.1;
        assert!(has(&low_cover, "unsafe_crown_profile"));
        let mut inward = fixture();
        if let CrownPath::Straight { outward, .. } = &mut inward.crowns[0].path {
            *outward = outward.opposite();
        }
        assert!(has(&inward, "crown_faces_inward"));
        let mut blocked = fixture();
        if let WallWalk::Linear { width_metres, .. } = &mut blocked.wall_walks[0] {
            *width_metres = 0.5;
        }
        assert!(has(&blocked, "blocked_crown_walk"));
        let owner = fixture().crowns[0].owner;
        let mut missing_coping = fixture();
        missing_coping
            .resolved_geometry
            .solids
            .retain(|solid| solid.owner != owner || solid.role != SolidRole::Coping);
        assert!(has(&missing_coping, "incomplete_crown_geometry"));
        let mut missing_drain = fixture();
        missing_drain
            .resolved_geometry
            .voids
            .retain(|void| void.owner != owner || void.role != VoidRole::Drain);
        assert!(has(&missing_drain, "incomplete_crown_geometry"));
        let mut exposed_edge = fixture();
        exposed_edge
            .resolved_geometry
            .solids
            .retain(|solid| solid.owner != owner || solid.role != SolidRole::EdgeGuard);
        assert!(has(&exposed_edge, "incomplete_crown_geometry"));
        let mut bad_splice = fixture();
        let round = bad_splice
            .crowns
            .iter_mut()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap();
        if let CrownPath::Round { radius_metres, .. } = &mut round.path {
            *radius_metres += 0.5;
        }
        assert!(has(&bad_splice, "bad_tower_crown_splice"));
        let mut bad_round_crenel = fixture();
        let round_owner = bad_round_crenel
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap()
            .owner;
        bad_round_crenel
            .resolved_geometry
            .solids
            .iter_mut()
            .filter(|solid| solid.owner == round_owner && solid.role == SolidRole::Merlon)
            .for_each(|solid| solid.size.x = 0.1);
        assert!(has(&bad_round_crenel, "invalid_round_crenel_interval"));
        let mut overgrown_splice = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            91,
        ))
        .unwrap();
        let (straight_owner, splice_position) = overgrown_splice
            .crowns
            .iter()
            .find_map(|crown| match crown.path {
                CrownPath::Straight { start, end, .. } => crown
                    .junctions
                    .iter()
                    .find(|junction| {
                        junction.kind == CrownJunctionKind::TowerSplice
                            && (junction.position - start).length() > 0.1
                            && (junction.position - end).length() > 0.1
                    })
                    .map(|junction| (crown.owner, junction.position)),
                CrownPath::Round { .. } => None,
            })
            .unwrap();
        let breastwork = overgrown_splice
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == straight_owner && solid.role == SolidRole::Breastwork)
            .unwrap();
        breastwork.centre.x = splice_position.x;
        breastwork.centre.z = splice_position.y;
        breastwork.size.x = 1.0;
        breastwork.size.z = 1.0;
        assert!(has(&overgrown_splice, "unresolved_tower_crown_splice"));
        let mut duplicate_corner = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            91,
        ))
        .unwrap();
        let corner = duplicate_corner
            .crowns
            .iter()
            .flat_map(|crown| crown.junctions.iter())
            .find(|junction| junction.kind == CrownJunctionKind::Corner)
            .copied()
            .unwrap();
        let duplicate = duplicate_corner
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| {
                solid.role == SolidRole::Merlon
                    && Vec2::new(solid.centre.x, solid.centre.z).distance(corner.position) < 0.08
            })
            .cloned()
            .unwrap();
        duplicate_corner.resolved_geometry.solids.push(duplicate);
        assert!(has(&duplicate_corner, "duplicate_junction_merlon"));
        let mut overlap = fixture();
        let mut duplicate = overlap.resolved_geometry.solids[0].clone();
        duplicate.owner = crate::GeometryOwnerId(99_999);
        duplicate.id = crate::ResolvedItemId(999_990);
        duplicate.supported_by = vec![crate::StructuralNodeId(999_990)];
        overlap
            .resolved_geometry
            .structural_nodes
            .push(crate::StructuralNode {
                id: crate::StructuralNodeId(999_990),
                owner: duplicate.owner,
                kind: crate::StructuralNodeKind::WallBearing,
                position: duplicate.centre,
                supported_by: Vec::new(),
                grounded: true,
            });
        overlap.resolved_geometry.solids.push(duplicate);
        assert!(has(&overlap, "undeclared_solid_overlap"));
        let mut open = fixture();
        open.resolved_geometry.solids[0].size.y = 0.0;
        assert!(has(&open, "invalid_resolved_geometry"));

        let mut stale_schema = fixture();
        stale_schema.resolved_geometry.schema_version = 1;
        assert!(has(&stale_schema, "stale_resolver_schema"));
        let mut duplicate_item = fixture();
        duplicate_item.resolved_geometry.solids[1].id =
            duplicate_item.resolved_geometry.solids[0].id;
        assert!(has(&duplicate_item, "invalid_resolved_geometry"));
        let mut unsupported = fixture();
        unsupported.resolved_geometry.structural_nodes[0].grounded = false;
        assert!(has(&unsupported, "unsupported_resolved_structure"));
        let mut no_bearing = fixture();
        let solid_id = no_bearing.resolved_geometry.solids[0].id;
        let owner = no_bearing.resolved_geometry.solids[0].owner;
        no_bearing
            .resolved_geometry
            .support_interfaces
            .retain(|bearing| bearing.owner != owner);
        assert!(has(&no_bearing, "missing_positive_bearing"), "{solid_id:?}");
        let mut interval_gap = fixture();
        let straight_owner = interval_gap
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Straight { .. }))
            .unwrap()
            .owner;
        let removed = interval_gap
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| {
                solid.owner == straight_owner
                    && solid.role == SolidRole::Breastwork
                    && solid.size.y > 0.2
            })
            .max_by(|a, b| a.size.max_element().total_cmp(&b.size.max_element()))
            .unwrap()
            .id;
        interval_gap
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != removed);
        assert!(has(&interval_gap, "crown_interval_gap"));
        let mut no_defender_samples = fixture();
        no_defender_samples
            .resolved_geometry
            .defender_samples
            .retain(|sample| sample.owner != straight_owner);
        assert!(has(&no_defender_samples, "unusable_crown_firing_position"));
        let mut flat_coping = fixture();
        flat_coping
            .resolved_geometry
            .solids
            .iter_mut()
            .filter(|solid| solid.role == SolidRole::Coping)
            .for_each(|solid| solid.crossfall_radians = 0.0);
        assert!(has(&flat_coping, "bad_crown_coping"));
        let mut uphill_drain = fixture();
        uphill_drain.resolved_geometry.drainage_routes[0].outlet.y =
            uphill_drain.resolved_geometry.drainage_routes[0].inlet.y;
        assert!(has(&uphill_drain, "broken_crown_drainage"));
        let mut flat_catchment = fixture();
        let walk_id = flat_catchment.resolved_geometry.drainage_catchments[0].walk_solid;
        flat_catchment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == walk_id)
            .unwrap()
            .crossfall_radians = 0.0;
        assert!(has(&flat_catchment, "broken_crown_drainage"));
        let mut reversed_catchment = fixture();
        let walk_id = reversed_catchment.resolved_geometry.drainage_catchments[0].walk_solid;
        let walk = reversed_catchment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == walk_id)
            .unwrap();
        walk.crossfall_radians = -walk.crossfall_radians;
        assert!(has(&reversed_catchment, "broken_crown_drainage"));
        let mut stalled_toe_channel = fixture();
        let channel_id =
            stalled_toe_channel.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        stalled_toe_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel_id)
            .unwrap()
            .longfall_radians = 0.0;
        assert!(has(&stalled_toe_channel, "broken_crown_drainage"));
        let mut wrong_inlet = fixture();
        let route_id = wrong_inlet.resolved_geometry.drainage_catchments[0].outlet_route;
        wrong_inlet
            .resolved_geometry
            .drainage_routes
            .iter_mut()
            .find(|route| route.id == route_id)
            .unwrap()
            .inlet
            .x += 0.4;
        assert!(has(&wrong_inlet, "broken_crown_drainage"));
        let mut local_basin = fixture();
        let channel_id = local_basin.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        let channel = local_basin
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel_id)
            .unwrap();
        channel.longfall_radians = channel.longfall_radians.abs();
        assert!(has(&local_basin, "broken_crown_drainage"));
        let mut raised_channel = fixture();
        let channel_id =
            raised_channel.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        raised_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel_id)
            .unwrap()
            .centre
            .y += 0.05;
        assert!(has(&raised_channel, "broken_crown_drainage"));
        let mut uncut_walk = fixture();
        let catchment = uncut_walk.resolved_geometry.drainage_catchments[0].clone();
        let walk = uncut_walk
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == catchment.walk_solid)
            .unwrap();
        walk.size.z = catchment.width_metres;
        walk.centre.x = catchment.centre.x;
        walk.centre.z = catchment.centre.z;
        assert!(has(&uncut_walk, "broken_crown_drainage"));
        let mut blocked_channel = fixture();
        let channel_id =
            blocked_channel.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        let channel_centre = blocked_channel
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == channel_id)
            .unwrap()
            .centre;
        let owner = blocked_channel.resolved_geometry.drainage_catchments[0].owner;
        let blocker = blocked_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::Breastwork)
            .unwrap();
        blocker.centre = channel_centre;
        blocker.size = Vec3::splat(0.2);
        assert!(has(&blocked_channel, "broken_crown_drainage"));
        let mut missing_catchment = fixture();
        missing_catchment
            .resolved_geometry
            .drainage_catchments
            .remove(0);
        assert!(has(&missing_catchment, "broken_crown_drainage"));
        let mut isolated_basin = fixture();
        isolated_basin.resolved_geometry.drainage_catchments[0].outlet_route =
            crate::ResolvedItemId(u64::MAX);
        assert!(has(&isolated_basin, "broken_crown_drainage"));
        let mut missing_drainage_surface = fixture();
        let surface_id = missing_drainage_surface
            .resolved_geometry
            .drainage_catchments[0]
            .drainage_surface;
        missing_drainage_surface
            .resolved_geometry
            .surfaces
            .retain(|surface| surface.id != surface_id);
        assert!(has(&missing_drainage_surface, "broken_crown_drainage"));
        let mut blocked_scupper = fixture();
        let route_id = blocked_scupper.resolved_geometry.drainage_catchments[0].outlet_route;
        let route = *blocked_scupper
            .resolved_geometry
            .drainage_routes
            .iter()
            .find(|route| route.id == route_id)
            .unwrap();
        let outlet = *blocked_scupper
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == route.outlet_void)
            .unwrap();
        let walk_id = blocked_scupper.resolved_geometry.drainage_catchments[0].walk_solid;
        let blocker = blocked_scupper
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == walk_id)
            .unwrap();
        blocker.centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
        assert!(has(&blocked_scupper, "broken_crown_drainage"));

        let cardinal = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            91,
        ))
        .unwrap();
        let mut seen = std::collections::HashSet::new();
        for crown in cardinal
            .crowns
            .iter()
            .filter(|crown| matches!(crown.path, CrownPath::Straight { .. }))
        {
            let CrownPath::Straight { outward, .. } = crown.path else {
                unreachable!()
            };
            if !seen.insert(outward) {
                continue;
            }
            let mut reversed = cardinal.clone();
            let walk_id = reversed
                .resolved_geometry
                .drainage_catchments
                .iter()
                .find(|catchment| catchment.owner == crown.owner)
                .unwrap()
                .walk_solid;
            let walk = reversed
                .resolved_geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == walk_id)
                .unwrap();
            walk.crossfall_radians = -walk.crossfall_radians;
            assert!(has(&reversed, "broken_crown_drainage"));
        }
        assert_eq!(seen.len(), 4);
        let mut reversed_round = fixture();
        let round_owner = reversed_round
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap()
            .owner;
        reversed_round
            .resolved_geometry
            .drainage_catchments
            .iter_mut()
            .find(|catchment| catchment.owner == round_owner)
            .unwrap()
            .outward *= -1.0;
        assert!(has(&reversed_round, "broken_crown_drainage"));
        let mut round_outer_edge_gap = fixture();
        let round_owner = round_outer_edge_gap
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap()
            .owner;
        round_outer_edge_gap
            .resolved_geometry
            .solids
            .iter_mut()
            .filter(|solid| solid.owner == round_owner && solid.role == SolidRole::WalkSurface)
            .for_each(|solid| solid.size.x *= 0.65);
        assert!(has(&round_outer_edge_gap, "broken_crown_drainage"));
        let mut missing_bond = fixture();
        missing_bond.resolved_geometry.junction_bonds.clear();
        assert!(has(&missing_bond, "missing_crown_junction_bond"));
        let mut shifted_bond = fixture();
        let bonded_overlap_index = shifted_bond
            .resolved_geometry
            .junction_bonds
            .iter()
            .position(|bond| {
                shifted_bond.resolved_geometry.solids.iter().any(|a| {
                    a.owner == bond.owners[0]
                        && shifted_bond.resolved_geometry.solids.iter().any(|b| {
                            b.owner == bond.owners[1]
                                && bounds_overlap_3d(
                                    resolved_solid_bounds(a),
                                    resolved_solid_bounds(b),
                                    0.025,
                                )
                        })
                })
            })
            .unwrap();
        shifted_bond.resolved_geometry.junction_bonds[bonded_overlap_index]
            .bounds
            .min
            .x += 1.0;
        shifted_bond.resolved_geometry.junction_bonds[bonded_overlap_index]
            .bounds
            .max
            .x += 1.0;
        assert!(has(&shifted_bond, "undeclared_solid_overlap"));
        let mut overpenetrated_bond = fixture();
        let bond = overpenetrated_bond.resolved_geometry.junction_bonds[bonded_overlap_index];
        let (target_centre, intruder_id) = overpenetrated_bond
            .resolved_geometry
            .solids
            .iter()
            .find_map(|a| {
                (a.owner == bond.owners[0]).then(|| {
                    overpenetrated_bond
                        .resolved_geometry
                        .solids
                        .iter()
                        .find(|b| {
                            b.owner == bond.owners[1]
                                && bounds_overlap_3d(
                                    resolved_solid_bounds(a),
                                    resolved_solid_bounds(b),
                                    0.025,
                                )
                        })
                        .map(|b| (a.centre, b.id))
                })?
            })
            .unwrap();
        let intruder = overpenetrated_bond
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == intruder_id)
            .unwrap();
        intruder.centre = target_centre;
        intruder.size = Vec3::splat(0.5);
        assert!(has(&overpenetrated_bond, "undeclared_solid_overlap"));

        let mut blocked_round_portal = fixture();
        let (round_owner, tower_index, centre, radius, base, thickness) = blocked_round_portal
            .crowns
            .iter()
            .find_map(|crown| match crown.path {
                CrownPath::Round {
                    tower_index,
                    centre,
                    radius_metres,
                } => Some((
                    crown.owner,
                    tower_index,
                    centre,
                    radius_metres,
                    crown.base_height_metres,
                    crown.profile.thickness_metres,
                )),
                CrownPath::Straight { .. } => None,
            })
            .unwrap();
        let facing = blocked_round_portal
            .tower_portals
            .iter()
            .find(|portal| {
                portal.tower_index == tower_index
                    && matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. })
            })
            .unwrap()
            .facing;
        let facing_vector = direction_vector(facing);
        let facing_angle = facing_vector.y.atan2(facing_vector.x);
        let blocker_template = blocked_round_portal
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.owner == round_owner && solid.role == SolidRole::Breastwork)
            .unwrap()
            .clone();
        for (serial, offset) in (-3..=3).enumerate() {
            let angle = facing_angle + offset as f32 * std::f32::consts::TAU / 24.0;
            let radial = Vec2::new(angle.cos(), angle.sin());
            let mut blocker = blocker_template.clone();
            blocker.id = crate::ResolvedItemId(999_991 + serial as u64);
            blocker.centre = Vec3::new(
                centre.x + radial.x * (radius + thickness * 0.5),
                base + 0.45,
                centre.y + radial.y * (radius + thickness * 0.5),
            );
            blocker.size = Vec3::new(1.2, 0.9, 1.2);
            blocked_round_portal.resolved_geometry.solids.push(blocker);
        }
        assert!(has(&blocked_round_portal, "blocked_round_crown_portal"));

        let mut blocked_spiral = fixture();
        let round = blocked_spiral
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap();
        let (tower_index, centre) = match round.path {
            CrownPath::Round {
                tower_index,
                centre,
                ..
            } => (tower_index, centre),
            CrownPath::Straight { .. } => unreachable!(),
        };
        let stair = blocked_spiral
            .stairs
            .iter()
            .find(|stair| matches!(stair, Stair::Spiral { centre: stair_centre, .. } if (*stair_centre-centre).length()<0.02))
            .copied()
            .unwrap();
        let Stair::Spiral {
            turns,
            clockwise,
            tread_count,
            ..
        } = stair
        else {
            unreachable!()
        };
        let progress = f32::from(tread_count.saturating_sub(1)) / f32::from(tread_count.max(1));
        let angle = if clockwise { -1.0 } else { 1.0 } * progress * turns * std::f32::consts::TAU;
        let stairwell_radius = blocked_spiral
            .wall_walks
            .iter()
            .find_map(|walk| match *walk {
                WallWalk::Round {
                    centre: walk_centre,
                    stairwell_radius_metres,
                    ..
                } if (walk_centre - centre).length() < 0.02 => Some(stairwell_radius_metres),
                _ => None,
            })
            .unwrap();
        let radial = Vec2::new(angle.cos(), angle.sin());
        let mut guard = blocked_spiral
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.owner == round.owner && solid.role == SolidRole::EdgeGuard)
            .unwrap()
            .clone();
        guard.id = crate::ResolvedItemId(999_992);
        guard.centre.x = centre.x + radial.x * (stairwell_radius + 0.08);
        guard.centre.z = centre.y + radial.y * (stairwell_radius + 0.08);
        guard.size.x = 1.0;
        guard.size.z = 1.0;
        blocked_spiral.resolved_geometry.solids.push(guard);
        assert_eq!(tower_index, 0);
        assert!(has(&blocked_spiral, "blocked_spiral_arrival"));
    }

    #[test]
    fn timber_frame_audit_rejects_program_joint_opening_jetty_and_roof_drift() {
        let fixture =
            |archetype| crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap();
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut relabelled = fixture(crate::BuildingArchetype::HallHouse);
        relabelled.timber_frame.as_mut().unwrap().program =
            crate::TimberFrameProgramKind::JettiedMerchantHouse;
        assert!(has(&relabelled, "invalid_timber_program"));

        let mut dangling_endpoint = fixture(crate::BuildingArchetype::TownHouse);
        dangling_endpoint.timber_frame.as_mut().unwrap().members[0]
            .start
            .x += 0.22;
        assert!(has(&dangling_endpoint, "invalid_timber_member_joint"));

        let mut missing_member_solid = fixture(crate::BuildingArchetype::TownHouse);
        let solid = missing_member_solid.timber_frame.as_ref().unwrap().members[0].solid;
        missing_member_solid
            .resolved_geometry
            .solids
            .retain(|candidate| candidate.id != solid);
        assert!(has(&missing_member_solid, "invalid_timber_member_joint"));

        let mut orphan_joint = fixture(crate::BuildingArchetype::TownHouse);
        orphan_joint.timber_frame.as_mut().unwrap().joints[0]
            .member_ids
            .clear();
        assert!(has(&orphan_joint, "orphan_timber_joint"));

        let mut unframed_opening = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let frame = unframed_opening.timber_frame.as_mut().unwrap();
        let bay = frame
            .bays
            .iter_mut()
            .find(|bay| bay.opening.is_some())
            .unwrap();
        bay.member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| member.role != crate::TimberMemberRole::IntermediatePost)
        });
        assert!(has(&unframed_opening, "invalid_timber_opening_bay"));

        let mut brace_through_window = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let (brace_solid, opening_void) = {
            let frame = brace_through_window.timber_frame.as_ref().unwrap();
            let bay = frame.bays.iter().find(|bay| bay.opening.is_some()).unwrap();
            let member = bay
                .member_ids
                .iter()
                .filter_map(|id| frame.members.iter().find(|member| member.id == *id))
                .find(|member| {
                    matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    )
                })
                .unwrap();
            let opening = brace_through_window
                .opening_assemblies
                .iter()
                .find(|opening| Some(opening.id) == bay.opening)
                .unwrap();
            (member.solid, opening.void_id)
        };
        let void = brace_through_window
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == opening_void)
            .unwrap()
            .bounds;
        let blocker = brace_through_window
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == brace_solid)
            .unwrap();
        blocker.centre = (void.min + void.max) * 0.5;
        blocker.size = Vec3::splat(0.30);
        assert!(has(&brace_through_window, "invalid_timber_opening_bay"));

        let mut one_post_row = fixture(crate::BuildingArchetype::HallHouse);
        let frame = one_post_row.timber_frame.as_mut().unwrap();
        let row_index = frame
            .internal_lines
            .iter()
            .position(|line| {
                line.storeys
                    .iter()
                    .flat_map(|storey| &storey.member_ids)
                    .any(|id| {
                        frame.members.iter().any(|member| {
                            member.id == *id && member.role == crate::TimberMemberRole::Purlin
                        })
                    })
            })
            .unwrap();
        frame.internal_lines.remove(row_index);
        assert!(has(&one_post_row, "invalid_timber_program"));

        let mut unsupported_jetty = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let jetty = unsupported_jetty
            .timber_frame
            .as_mut()
            .unwrap()
            .facades
            .iter_mut()
            .flat_map(|facade| &mut facade.lines)
            .flat_map(|line| &mut line.storeys)
            .find_map(|storey| storey.jetty.as_mut())
            .unwrap();
        jetty.knaggen.clear();
        assert!(has(&unsupported_jetty, "unsupported_timber_jetty"));

        let mut floor_outside_support = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let floor_id = floor_outside_support
            .timber_frame
            .as_ref()
            .unwrap()
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .flat_map(|line| &line.storeys)
            .find_map(|storey| storey.jetty.as_ref())
            .unwrap()
            .floor_solid;
        floor_outside_support
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap()
            .size
            .z += 0.45;
        assert!(has(&floor_outside_support, "unsupported_timber_jetty"));

        let mut no_roof_seat = fixture(crate::BuildingArchetype::FachwerkCottage);
        no_roof_seat
            .timber_frame
            .as_mut()
            .unwrap()
            .roof_bearing_interfaces
            .clear();
        assert!(has(&no_roof_seat, "severed_timber_roof_bearing"));

        let mut missing_trimmer = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        missing_trimmer
            .timber_frame
            .as_mut()
            .unwrap()
            .dormer_trimmer_members
            .pop();
        assert!(has(&missing_trimmer, "severed_timber_roof_bearing"));

        let mut protruding_half_hip_truss = fixture(crate::BuildingArchetype::HallHouse);
        protruding_half_hip_truss
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| {
                member.phase == crate::TimberFramePhase::RoofConstruction
                    && member.role == crate::TimberMemberRole::GablePost
            })
            .unwrap()
            .end
            .y += 2.0;
        assert!(has(
            &protruding_half_hip_truss,
            "timber_intrudes_through_roof"
        ));

        let mut coplanar_window = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        let opening_sill = coplanar_window
            .opening_assemblies
            .iter()
            .find(|opening| {
                coplanar_window
                    .wall_assemblies
                    .iter()
                    .find(|wall| wall.id == opening.host_wall)
                    .is_some_and(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
            })
            .unwrap()
            .sill_solid
            .unwrap();
        coplanar_window
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == opening_sill)
            .unwrap()
            .size
            .z += 0.03;
        assert!(has(&coplanar_window, "coplanar_timber_opening_face"));

        let mut free_dormer_posts = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        let dormer_gable_post = {
            let frame = free_dormer_posts.timber_frame.as_ref().unwrap();
            frame
                .bays
                .iter()
                .find(|bay| {
                    bay.wall.is_some_and(|wall_id| {
                        free_dormer_posts
                            .wall_assemblies
                            .iter()
                            .find(|wall| wall.id == wall_id)
                            .is_some_and(|wall| {
                                matches!(wall.source, crate::WallSourceId::RoofChildFront { .. })
                            })
                    })
                })
                .and_then(|bay| {
                    bay.member_ids.iter().find(|id| {
                        frame.members.iter().any(|member| {
                            member.id == **id && member.role == crate::TimberMemberRole::GablePost
                        })
                    })
                })
                .copied()
                .unwrap()
        };
        free_dormer_posts
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| member.id == dormer_gable_post)
            .unwrap()
            .role = crate::TimberMemberRole::PrimaryPost;
        assert!(has(&free_dormer_posts, "invalid_timber_dormer_front"));

        let mut legacy_overlay = fixture(crate::BuildingArchetype::TownHouse);
        let mut legacy = legacy_overlay.resolved_geometry.solids[0].clone();
        legacy.id = crate::ResolvedItemId(u64::MAX - 8);
        legacy.role = SolidRole::FrameMember;
        legacy_overlay.resolved_geometry.solids.push(legacy);
        assert!(has(&legacy_overlay, "legacy_timber_frame_overlay"));

        let mut missing_floor_frame = fixture(crate::BuildingArchetype::TownHouse);
        missing_floor_frame.timber_frame.as_mut().unwrap().floors[1]
            .joist_members
            .clear();
        assert!(has(&missing_floor_frame, "unsupported_timber_floor_route"));

        let mut severed_stair_route = fixture(crate::BuildingArchetype::TownHouse);
        severed_stair_route.timber_frame.as_mut().unwrap().floors[1].stair_connection = None;
        assert!(has(&severed_stair_route, "unsupported_timber_floor_route"));

        let mut illegal_joint = fixture(crate::BuildingArchetype::TownHouse);
        illegal_joint.timber_frame.as_mut().unwrap().joints[4].kind =
            crate::TimberJointKind::Bridle;
        assert!(has(&illegal_joint, "invalid_timber_joint_vocabulary"));

        let mut false_nearby = fixture(crate::BuildingArchetype::TownHouse);
        let owner = false_nearby.timber_frame.as_ref().unwrap().members[0].owner;
        let frame_nodes = false_nearby
            .resolved_geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == owner)
            .map(|node| (node.id, node.position))
            .collect::<Vec<_>>();
        let (child, child_position) = frame_nodes[0];
        let parent = frame_nodes
            .iter()
            .max_by(|left, right| {
                left.1
                    .distance(child_position)
                    .total_cmp(&right.1.distance(child_position))
            })
            .unwrap()
            .0;
        false_nearby
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == child)
            .unwrap()
            .supported_by
            .push(parent);
        assert!(has(&false_nearby, "false_timber_support_edge"));

        let mut cyclic_support = fixture(crate::BuildingArchetype::TownHouse);
        let timber_owner = cyclic_support.timber_frame.as_ref().unwrap().members[0].owner;
        let node_id = cyclic_support
            .resolved_geometry
            .structural_nodes
            .iter()
            .find(|node| node.owner == timber_owner && !node.grounded)
            .unwrap()
            .id;
        cyclic_support
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == node_id)
            .unwrap()
            .supported_by
            .push(node_id);
        assert!(has(&cyclic_support, "unsupported_resolved_structure"));

        let mut shifted_roof_seat = fixture(crate::BuildingArchetype::FachwerkCottage);
        let interface_id = shifted_roof_seat
            .timber_frame
            .as_ref()
            .unwrap()
            .roof_bearing_interfaces[0];
        let interface = shifted_roof_seat
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
            .unwrap();
        interface.bounds.min.x += 1.0;
        interface.bounds.max.x += 1.0;
        assert!(has(&shifted_roof_seat, "severed_timber_roof_bearing"));

        let mut missing_infill_partition = fixture(crate::BuildingArchetype::TownHouse);
        missing_infill_partition
            .timber_frame
            .as_mut()
            .unwrap()
            .bays
            .iter_mut()
            .find(|bay| bay.opening.is_some())
            .unwrap()
            .infill_solids
            .pop();
        assert!(has(&missing_infill_partition, "invalid_timber_opening_bay"));

        let panel_id = |plan: &crate::BuildingPlan| {
            plan.timber_frame
                .as_ref()
                .unwrap()
                .bays
                .iter()
                .find(|bay| bay.opening.is_some())
                .and_then(|bay| bay.infill_solids.first().copied())
                .unwrap()
        };
        let refresh_panel_bounds = |solid: &mut ResolvedSolid| {
            let crate::ResolvedSolidShape::TimberPanelPrism {
                vertices,
                outward,
                depth_metres,
            } = solid.shape
            else {
                return;
            };
            let offset = Vec3::new(outward.x, 0.0, outward.y) * depth_metres * 0.5;
            let min = vertices
                .iter()
                .flat_map(|vertex| [*vertex - offset, *vertex + offset])
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = vertices
                .iter()
                .flat_map(|vertex| [*vertex - offset, *vertex + offset])
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            solid.centre = (min + max) * 0.5;
            solid.size = max - min;
        };

        let mut backing_sheet = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&backing_sheet);
        backing_sheet
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&backing_sheet, "invalid_timber_opening_bay"));

        let mut shifted_panel_gap = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&shifted_panel_gap);
        let panel = shifted_panel_gap
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap();
        if let crate::ResolvedSolidShape::TimberPanelPrism { vertices, .. } = &mut panel.shape {
            vertices[0].x += 0.08;
        }
        refresh_panel_bounds(panel);
        assert!(has(&shifted_panel_gap, "invalid_timber_opening_bay"));

        let mut panel_crosses_frame = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&panel_crosses_frame);
        let wall_id = panel_crosses_frame
            .timber_frame
            .as_ref()
            .unwrap()
            .bays
            .iter()
            .find(|bay| bay.infill_solids.contains(&id))
            .unwrap()
            .wall
            .unwrap();
        let wall = panel_crosses_frame
            .wall_assemblies
            .iter()
            .find(|wall| wall.id == wall_id)
            .unwrap()
            .clone();
        let panel = panel_crosses_frame
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap();
        if let crate::ResolvedSolidShape::TimberPanelPrism { vertices, .. } = &mut panel.shape {
            let plane = wall.frame.origin;
            *vertices = [
                Vec3::new(
                    (plane - wall.frame.tangent * wall.length_metres * 0.45).x,
                    wall.base_elevation_metres + 0.15,
                    (plane - wall.frame.tangent * wall.length_metres * 0.45).y,
                ),
                Vec3::new(
                    (plane + wall.frame.tangent * wall.length_metres * 0.45).x,
                    wall.base_elevation_metres + 0.15,
                    (plane + wall.frame.tangent * wall.length_metres * 0.45).y,
                ),
                Vec3::new(
                    plane.x,
                    wall.base_elevation_metres + wall.height_metres * 0.85,
                    plane.y,
                ),
            ];
        }
        refresh_panel_bounds(panel);
        assert!(has(&panel_crosses_frame, "invalid_timber_opening_bay"));

        let mut overlapping_panel = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&overlapping_panel);
        let mut duplicate = overlapping_panel
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == id)
            .unwrap()
            .clone();
        duplicate.id = crate::ResolvedItemId(u64::MAX - 17);
        let wall_id = overlapping_panel
            .timber_frame
            .as_ref()
            .unwrap()
            .bays
            .iter()
            .find(|bay| bay.infill_solids.contains(&id))
            .unwrap()
            .wall
            .unwrap();
        overlapping_panel
            .wall_assemblies
            .iter_mut()
            .find(|wall| wall.id == wall_id)
            .unwrap()
            .host_solids
            .push(duplicate.id);
        for bay in overlapping_panel
            .timber_frame
            .as_mut()
            .unwrap()
            .bays
            .iter_mut()
            .filter(|bay| bay.wall == Some(wall_id))
        {
            bay.infill_solids.push(duplicate.id);
        }
        overlapping_panel.resolved_geometry.solids.push(duplicate);
        assert!(has(&overlapping_panel, "invalid_timber_opening_bay"));

        let mut untriangulated = fixture(crate::BuildingArchetype::TownHouse);
        let frame = untriangulated.timber_frame.as_mut().unwrap();
        let storey = &mut frame.facades[0].lines[0].storeys[0];
        storey.member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| {
                    !matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    )
                })
        });
        assert!(has(&untriangulated, "unbraced_timber_storey"));

        let mut missing_middle_racking = fixture(crate::BuildingArchetype::TownHouse);
        let frame = missing_middle_racking.timber_frame.as_mut().unwrap();
        let line = &mut frame.facades[0].lines[0];
        let storey = &mut line.storeys[0];
        storey.member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| {
                    let midpoint = (member.start + member.end) * 0.5;
                    let along = (Vec2::new(midpoint.x, midpoint.z) - line.origin).dot(line.tangent)
                        / line.length_metres
                        + 0.5;
                    !matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    ) || !(0.30..=0.70).contains(&along)
                })
        });
        assert!(has(&missing_middle_racking, "unbraced_timber_storey"));

        let mut broken_transverse = fixture(crate::BuildingArchetype::HallHouse);
        let frame = broken_transverse.timber_frame.as_mut().unwrap();
        let line = frame
            .internal_lines
            .iter_mut()
            .find(|line| {
                line.storeys
                    .iter()
                    .flat_map(|storey| &storey.member_ids)
                    .any(|id| {
                        frame.members.iter().any(|member| {
                            member.id == *id
                                && member.role == crate::TimberMemberRole::TransverseTie
                        })
                    })
            })
            .unwrap();
        line.storeys[0].member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| member.role != crate::TimberMemberRole::StoreyBrace)
        });
        assert!(has(&broken_transverse, "unbraced_timber_storey"));

        let mut swapped_joint = fixture(crate::BuildingArchetype::TownHouse);
        let joint = swapped_joint
            .timber_frame
            .as_mut()
            .unwrap()
            .joints
            .iter_mut()
            .find(|joint| joint.kind == crate::TimberJointKind::Lap)
            .unwrap();
        joint.kind = crate::TimberJointKind::MortiseTenon;
        assert!(has(&swapped_joint, "invalid_timber_joint_contact"));

        let mut wrong_world_axis = fixture(crate::BuildingArchetype::TownHouse);
        let joint = wrong_world_axis
            .timber_frame
            .as_mut()
            .unwrap()
            .joints
            .iter_mut()
            .find(|joint| joint.load_direction.dot(Vec3::Z).abs() < 0.8)
            .unwrap();
        joint.load_direction = Vec3::Z;
        assert!(has(&wrong_world_axis, "invalid_timber_joint_contact"));

        let mut broken_action_reaction = fixture(crate::BuildingArchetype::TownHouse);
        let participant = broken_action_reaction
            .timber_frame
            .as_mut()
            .unwrap()
            .joints
            .iter_mut()
            .find_map(|joint| joint.participants.first_mut())
            .unwrap();
        participant.reaction_direction = participant.axis_from_joint;
        assert!(has(&broken_action_reaction, "invalid_timber_joint_contact"));

        let cardinal_frame = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let frame = cardinal_frame.timber_frame.as_ref().unwrap();
        let cardinal_reactions = frame
            .joints
            .iter()
            .filter(|joint| joint.kind == crate::TimberJointKind::JettyBearing)
            .map(|joint| {
                let horizontal =
                    Vec2::new(joint.load_direction.x, joint.load_direction.z).normalize_or_zero();
                (horizontal.x.round() as i8, horizontal.y.round() as i8)
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            cardinal_reactions,
            [(-1, 0), (1, 0), (0, -1), (0, 1)].into_iter().collect(),
            "facade-local joint reactions must rotate and mirror with all four facades"
        );

        let mut wrong_material = fixture(crate::BuildingArchetype::TownHouse);
        wrong_material.timber_frame.as_mut().unwrap().members[0].material =
            crate::StructuralTimberMaterial::Oak;
        assert!(has(&wrong_material, "invalid_timber_joint_contact"));

        let mut narrow_route = fixture(crate::BuildingArchetype::TownHouse);
        narrow_route
            .timber_frame
            .as_mut()
            .unwrap()
            .circulation
            .edges[0]
            .clear_width_metres = 0.70;
        assert!(has(&narrow_route, "invalid_timber_circulation"));

        let mut broken_route = fixture(crate::BuildingArchetype::TownHouse);
        broken_route
            .timber_frame
            .as_mut()
            .unwrap()
            .circulation
            .edges
            .pop();
        assert!(has(&broken_route, "invalid_timber_circulation"));

        let mut missing_cut = fixture(crate::BuildingArchetype::TownHouse);
        missing_cut
            .timber_frame
            .as_mut()
            .unwrap()
            .circulation
            .floor_cut_voids
            .clear();
        assert!(has(&missing_cut, "invalid_timber_circulation"));

        let mut shifted_entry = fixture(crate::BuildingArchetype::TownHouse);
        let opening_id = shifted_entry
            .timber_frame
            .as_ref()
            .unwrap()
            .circulation
            .entry_opening
            .unwrap();
        let void_id = shifted_entry
            .opening_assemblies
            .iter()
            .find(|opening| opening.id == opening_id)
            .unwrap()
            .void_id;
        let void = shifted_entry
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == void_id)
            .unwrap();
        void.bounds.min.x += 0.8;
        void.bounds.max.x += 0.8;
        assert!(has(&shifted_entry, "invalid_timber_circulation"));

        let mut nested_posts = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let post_ids = nested_posts
            .timber_frame
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|member| {
                matches!(
                    member.role,
                    crate::TimberMemberRole::PrimaryPost
                        | crate::TimberMemberRole::CornerPost
                        | crate::TimberMemberRole::IntermediatePost
                )
            })
            .take(2)
            .map(|member| (member.id, member.solid, member.start, member.end))
            .collect::<Vec<_>>();
        let (target_start, target_end) = (post_ids[0].2, post_ids[0].3);
        let member = nested_posts
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| member.id == post_ids[1].0)
            .unwrap();
        member.start.x = target_start.x;
        member.start.z = target_start.z;
        member.end.x = target_end.x;
        member.end.z = target_end.z;
        let target_centre = nested_posts
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == post_ids[0].1)
            .unwrap()
            .centre;
        let solid = nested_posts
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == post_ids[1].1)
            .unwrap();
        solid.centre.x = target_centre.x;
        solid.centre.z = target_centre.z;
        assert!(has(&nested_posts, "overlapping_timber_members"));

        let mut severed_floor_chain = fixture(crate::BuildingArchetype::TownHouse);
        let interface_id =
            severed_floor_chain.timber_frame.as_ref().unwrap().floors[1].joist_girder_interfaces[0];
        let interface = severed_floor_chain
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
            .unwrap();
        interface.bounds.min.x += 0.8;
        interface.bounds.max.x += 0.8;
        assert!(has(&severed_floor_chain, "unsupported_timber_floor_route"));

        let mut buried_frame = fixture(crate::BuildingArchetype::TownHouse);
        let panel_id = buried_frame
            .timber_frame
            .as_ref()
            .unwrap()
            .bays
            .iter()
            .find(|bay| bay.opening.is_some())
            .unwrap()
            .infill_solids[0];
        let panel = buried_frame
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == panel_id)
            .unwrap();
        panel.size.z += 0.30;
        assert!(has(&buried_frame, "invalid_timber_opening_bay"));

        let mut shifted_joint_contact = fixture(crate::BuildingArchetype::TownHouse);
        let contact_id = shifted_joint_contact
            .timber_frame
            .as_ref()
            .unwrap()
            .joints
            .iter()
            .find(|joint| joint.member_ids.len() >= 2)
            .unwrap()
            .contact_interfaces[0];
        let contact = shifted_joint_contact
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == contact_id)
            .unwrap();
        contact.bounds.min.z += 0.5;
        contact.bounds.max.z += 0.5;
        assert!(has(&shifted_joint_contact, "invalid_timber_joint_contact"));

        let mut blocked_route = fixture(crate::BuildingArchetype::TownHouse);
        let frame = blocked_route.timber_frame.as_ref().unwrap();
        let edge = frame
            .circulation
            .edges
            .iter()
            .find(|edge| {
                frame
                    .circulation
                    .nodes
                    .iter()
                    .find(|node| node.surface == edge.from)
                    .is_some_and(|node| node.kind == crate::TimberRouteNodeKind::Landing)
            })
            .unwrap();
        let from = frame
            .circulation
            .nodes
            .iter()
            .find(|node| node.surface == edge.from)
            .unwrap()
            .position;
        let to = frame
            .circulation
            .nodes
            .iter()
            .find(|node| node.surface == edge.to)
            .unwrap()
            .position;
        let mut blocker = blocked_route
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.role == SolidRole::WallHost)
            .unwrap()
            .clone();
        blocker.id = crate::ResolvedItemId(u64::MAX - 31);
        blocker.centre = (from + to) * 0.5 + Vec3::Y;
        blocker.size = Vec3::new(0.22, 2.0, 1.2);
        blocked_route.resolved_geometry.solids.push(blocker);
        assert!(has(&blocked_route, "invalid_timber_circulation"));
    }
}
