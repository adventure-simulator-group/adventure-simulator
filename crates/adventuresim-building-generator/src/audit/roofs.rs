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
                                        tread_count: _,
                                        run_metres,
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
                                        let end = start + axis * run_metres;
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
