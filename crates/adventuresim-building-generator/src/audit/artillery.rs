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
