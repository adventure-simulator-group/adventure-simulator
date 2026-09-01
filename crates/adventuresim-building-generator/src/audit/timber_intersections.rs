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
