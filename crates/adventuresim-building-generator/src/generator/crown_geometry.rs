fn crown_merlon_ranges(length: f32, profile: CrownProfile) -> Vec<(f32, f32)> {
    let minimum_end = 0.25;
    let nominal = profile.merlon_width_metres + profile.crenel_width_metres;
    let crenel_count = (((length - minimum_end * 2.0) / nominal).floor() as usize).max(1);
    let actual_merlon =
        (length - profile.crenel_width_metres * crenel_count as f32) / (crenel_count + 1) as f32;
    let mut cursor = 0.0;
    let mut ranges = Vec::with_capacity(crenel_count + 1);
    for index in 0..=crenel_count {
        ranges.push((cursor, cursor + actual_merlon));
        cursor += actual_merlon;
        if index < crenel_count {
            cursor += profile.crenel_width_metres;
        }
    }
    ranges
}

fn resolve_crown_geometry(
    crowns: &[CrownAssembly],
    walks: &[WallWalk],
    stairs: &[Stair],
    tower_portals: &[TowerPortal],
) -> ResolvedGeometry {
    let mut geometry = ResolvedGeometry {
        schema_version: 2,
        ..ResolvedGeometry::default()
    };
    for crown in crowns {
        let support_node = StructuralNodeId(u64::from(crown.owner.0) * 10 + 1);
        let p = crown.profile;
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let original_start = start;
                let original_end = end;
                let original_tangent = (end - start).normalize_or_zero();
                let splice_trim = |position: Vec2| {
                    crown
                        .junctions
                        .iter()
                        .find(|junction| {
                            junction.kind == CrownJunctionKind::TowerSplice
                                && (junction.position - position).length() < 0.02
                        })
                        .and_then(|junction| {
                            crowns.iter().find_map(|other| {
                                (other.owner == junction.other_owner)
                                    .then_some(other.path)
                                    .and_then(|path| match path {
                                        CrownPath::Round { radius_metres, .. } => {
                                            Some(radius_metres + p.thickness_metres * 0.5 - 0.08)
                                        }
                                        CrownPath::Straight { .. } => None,
                                    })
                            })
                        })
                        .unwrap_or(0.0)
                };
                let start = start + original_tangent * splice_trim(start);
                let end = end - original_tangent * splice_trim(end);
                let delta = end - start;
                let length = delta.length();
                let tangent = delta.normalize_or_zero();
                let normal = direction_vector(outward);
                let horizontal = tangent.x.abs() >= tangent.y.abs();
                let mut exclusions = crown
                    .junctions
                    .iter()
                    .filter_map(|junction| {
                        let other = crowns
                            .iter()
                            .find(|other| other.owner == junction.other_owner)?;
                        let CrownPath::Round { radius_metres, .. } = other.path else {
                            return None;
                        };
                        let distance = (junction.position - start).dot(tangent);
                        (distance > 0.02 && distance < length - 0.02).then_some((
                            (distance - radius_metres - p.thickness_metres * 0.5 + 0.08).max(0.0),
                            (distance + radius_metres + p.thickness_metres * 0.5 - 0.08)
                                .min(length),
                        ))
                    })
                    .collect::<Vec<_>>();
                exclusions.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut active_ranges = Vec::new();
                let mut active_start = 0.0;
                for (cut_start, cut_end) in exclusions {
                    if cut_start > active_start + 0.02 {
                        active_ranges.push((active_start, cut_start));
                    }
                    active_start = active_start.max(cut_end);
                }
                if length > active_start + 0.02 {
                    active_ranges.push((active_start, length));
                }
                let solid = |role, along_centre: f32, along_size: f32, z: f32, height: f32| {
                    let plan = start + tangent * along_centre + normal * p.thickness_metres * 0.5;
                    let transverse =
                        p.thickness_metres + if role == SolidRole::Coping { 0.04 } else { 0.0 };
                    ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(plan.x, z + height * 0.5, plan.y),
                        size: if horizontal {
                            Vec3::new(along_size, height, transverse)
                        } else {
                            Vec3::new(transverse, height, along_size)
                        },
                        yaw_radians: 0.0,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    }
                };
                let drain_height = 0.18;
                for &(range_start, range_end) in &active_ranges {
                    let mut chunk_start = range_start;
                    while chunk_start < range_end - 0.01 {
                        let chunk_end = (chunk_start + 2.4).min(range_end);
                        geometry.solids.push(solid(
                            SolidRole::Breastwork,
                            (chunk_start + chunk_end) * 0.5,
                            chunk_end - chunk_start,
                            crown.base_height_metres + drain_height,
                            p.breastwork_height_metres - drain_height,
                        ));
                        geometry.solids.push(solid(
                            SolidRole::Coping,
                            (chunk_start + chunk_end) * 0.5,
                            chunk_end - chunk_start,
                            crown.base_height_metres + p.breastwork_height_metres,
                            p.coping_height_metres,
                        ));
                        let guard_start_trim = crown.junctions.iter().any(|junction| {
                            junction.kind == CrownJunctionKind::Corner
                                && (junction.position - original_start).length() < 0.02
                        });
                        let guard_end_trim = crown.junctions.iter().any(|junction| {
                            junction.kind == CrownJunctionKind::Corner
                                && (junction.position - original_end).length() < 0.02
                        });
                        let guard_start = chunk_start.max(if guard_start_trim {
                            p.walk_clear_width_metres
                        } else {
                            0.0
                        });
                        let guard_end = chunk_end.min(if guard_end_trim {
                            length - p.walk_clear_width_metres
                        } else {
                            length
                        });
                        if guard_end > guard_start + 0.02 {
                            let inner_guard_plan = start
                                + tangent * ((guard_start + guard_end) * 0.5)
                                - normal * (p.walk_clear_width_metres + 0.08);
                            geometry.solids.push(ResolvedSolid {
                                id: ResolvedItemId::default(),
                                owner: crown.owner,
                                centre: Vec3::new(
                                    inner_guard_plan.x,
                                    crown.base_height_metres + p.inner_guard_height_metres * 0.5,
                                    inner_guard_plan.y,
                                ),
                                size: if horizontal {
                                    Vec3::new(
                                        guard_end - guard_start,
                                        p.inner_guard_height_metres,
                                        0.12,
                                    )
                                } else {
                                    Vec3::new(
                                        0.12,
                                        p.inner_guard_height_metres,
                                        guard_end - guard_start,
                                    )
                                },
                                yaw_radians: 0.0,
                                crossfall_radians: 0.0,
                                longfall_radians: 0.0,
                                role: SolidRole::EdgeGuard,
                                shape: crate::ResolvedSolidShape::Cuboid,
                                supported_by: vec![support_node],
                            });
                        }
                        chunk_start = chunk_end;
                    }
                }
                let mut cuts = crown
                    .drain_positions
                    .iter()
                    .map(|drain| (*drain - start).dot(tangent))
                    .filter(|distance| {
                        active_ranges.iter().any(|(range_start, range_end)| {
                            *distance > *range_start + 0.08 && *distance < *range_end - 0.08
                        })
                    })
                    .collect::<Vec<_>>();
                cuts.sort_by(f32::total_cmp);
                for &(range_start, range_end) in &active_ranges {
                    let mut cursor = range_start;
                    for cut in cuts
                        .iter()
                        .copied()
                        .filter(|cut| *cut > range_start && *cut < range_end)
                        .chain(std::iter::once(range_end + 0.08))
                    {
                        let end = (cut - 0.08).min(range_end);
                        if end - cursor > 0.02 {
                            geometry.solids.push(solid(
                                SolidRole::Breastwork,
                                (cursor + end) * 0.5,
                                end - cursor,
                                crown.base_height_metres,
                                drain_height,
                            ));
                        }
                        cursor = (cut + 0.08).min(range_end);
                    }
                }
                for (from, to) in crown_merlon_ranges(length, p)
                    .into_iter()
                    .filter(|(from, to)| {
                        let start_owned = crown
                            .junctions
                            .iter()
                            .any(|junction| (junction.position - original_start).length() < 0.02);
                        let end_owned = crown
                            .junctions
                            .iter()
                            .any(|junction| (junction.position - original_end).length() < 0.02);
                        !(start_owned && *from < 0.02) && !(end_owned && length - *to < 0.02)
                    })
                    .flat_map(|(from, to)| {
                        active_ranges
                            .iter()
                            .filter_map(move |(range_start, range_end)| {
                                let clipped_from = from.max(*range_start);
                                let clipped_to = to.min(*range_end);
                                (clipped_to - clipped_from >= 0.25)
                                    .then_some((clipped_from, clipped_to))
                            })
                    })
                {
                    geometry.solids.push(solid(
                        SolidRole::Merlon,
                        (from + to) * 0.5,
                        to - from,
                        crown.base_height_metres
                            + p.breastwork_height_metres
                            + p.coping_height_metres,
                        p.merlon_height_metres - p.coping_height_metres,
                    ));
                    geometry.solids.push(solid(
                        SolidRole::Coping,
                        (from + to) * 0.5,
                        to - from,
                        crown.base_height_metres
                            + p.breastwork_height_metres
                            + p.merlon_height_metres,
                        p.coping_height_metres,
                    ));
                }
                let walk = walks.iter().find(|walk| matches!(walk, WallWalk::Linear { start: a, end: b, .. } if (*a-original_start).length()<0.02 && (*b-original_end).length()<0.02));
                if let Some(walk) = walk {
                    let bounds = linear_walk_bounds_for_geometry(*walk);
                    geometry.surfaces.push(ResolvedSurface {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        bounds,
                        role: SurfaceRole::Stance,
                        shape: crate::ResolvedSurfaceShape::Planar,
                    });
                }
                for drain in crown.drain_positions.iter().filter(|drain| {
                    let distance = (**drain - start).dot(tangent);
                    active_ranges.iter().any(|(range_start, range_end)| {
                        distance > *range_start + 0.08 && distance < *range_end - 0.08
                    })
                }) {
                    let inner = *drain + normal * 0.01;
                    let outer = *drain + normal * (p.thickness_metres + 0.01);
                    let lateral = tangent.abs() * 0.06;
                    geometry.voids.push(ResolvedVoid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                inner.x.min(outer.x) - lateral.x,
                                crown.base_height_metres,
                                inner.y.min(outer.y) - lateral.y,
                            ),
                            max: Vec3::new(
                                inner.x.max(outer.x) + lateral.x,
                                crown.base_height_metres + 0.18,
                                inner.y.max(outer.y) + lateral.y,
                            ),
                        },
                        role: VoidRole::Drain,
                        shape: crate::ResolvedVoidShape::Box,
                        subtracts_from: crown.owner,
                    });
                }
                geometry.surfaces.push(ResolvedSurface {
                    id: ResolvedItemId::default(),
                    owner: crown.owner,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            start.x.min(end.x),
                            crown.base_height_metres + p.firing_height_metres,
                            start.y.min(end.y),
                        ),
                        max: Vec3::new(
                            start.x.max(end.x),
                            crown.base_height_metres + p.firing_height_metres + 0.01,
                            start.y.max(end.y),
                        ),
                    },
                    role: SurfaceRole::FiringLine,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
            }
            CrownPath::Round {
                tower_index,
                centre,
                radius_metres,
            } => {
                let segments = 24;
                let segment_angle = std::f32::consts::TAU / segments as f32;
                let mut portal_angles = tower_portals
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
                // Gate towers can splice into the middle of a straight crown
                // without owning a defensive-circuit portal. Resolve both
                // wallward directions from the reciprocal crown junction so
                // their circular breastwork and merlons are cut as well.
                for junction in crown
                    .junctions
                    .iter()
                    .filter(|junction| junction.kind == CrownJunctionKind::TowerSplice)
                {
                    if let Some(CrownPath::Straight { start, end, .. }) = crowns
                        .iter()
                        .find(|other| other.owner == junction.other_owner)
                        .map(|other| other.path)
                    {
                        for point in [start, end] {
                            let direction = point - centre;
                            if direction.length() > radius_metres + 0.1 {
                                portal_angles.push(direction.y.atan2(direction.x));
                            }
                        }
                    }
                }
                let angular_distance = |a: f32, b: f32| {
                    ((a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI)
                        .abs()
                };
                let outer_sector_open = |angle: f32| {
                    portal_angles.iter().any(|portal_angle| {
                        angular_distance(angle, *portal_angle)
                            <= segment_angle + 0.45 / radius_metres
                    })
                };
                // A merlon whose centre is outside the clear portal can still
                // project across its edge. Expand the centre exclusion by the
                // half-merlon chord so the masonry cannot overlap the entering
                // straight crown or narrow the declared route.
                let outer_merlon_sector_open = |angle: f32| {
                    portal_angles.iter().any(|portal_angle| {
                        angular_distance(angle, *portal_angle)
                            <= segment_angle + (0.45 + p.merlon_width_metres * 0.5) / radius_metres
                    })
                };
                let stair_arrival = stairs.iter().find_map(|stair| match *stair {
                    Stair::Spiral {
                        centre: stair_centre,
                        turns,
                        clockwise,
                        tread_count,
                        ..
                    } if (stair_centre - centre).length() < 0.02 => {
                        let progress = f32::from(tread_count.saturating_sub(1))
                            / f32::from(tread_count.max(1));
                        let handedness = if clockwise { -1.0 } else { 1.0 };
                        Some(handedness * progress * turns * std::f32::consts::TAU)
                    }
                    _ => None,
                });
                for index in 0..segments {
                    let angle = index as f32 * std::f32::consts::TAU / segments as f32;
                    if outer_sector_open(angle) {
                        continue;
                    }
                    let radial = Vec2::new(angle.cos(), angle.sin());
                    let tangent_length = std::f32::consts::TAU * radius_metres / segments as f32;
                    let plan = centre + radial * (radius_metres + p.thickness_metres * 0.5);
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + 0.18
                                + (p.breastwork_height_metres - 0.18) * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(
                            tangent_length + 0.03,
                            p.breastwork_height_metres - 0.18,
                            p.thickness_metres,
                        ),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Breastwork,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                    // Every third segment is a genuine open scupper through the
                    // lower breastwork band, aligned with the eight declared drains.
                    if index % 3 != 0 {
                        geometry.solids.push(ResolvedSolid {
                            id: ResolvedItemId::default(),
                            owner: crown.owner,
                            centre: Vec3::new(plan.x, crown.base_height_metres + 0.09, plan.y),
                            size: Vec3::new(tangent_length + 0.03, 0.18, p.thickness_metres),
                            yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                            crossfall_radians: 0.0,
                            longfall_radians: 0.0,
                            role: SolidRole::Breastwork,
                            shape: crate::ResolvedSolidShape::Cuboid,
                            supported_by: vec![support_node],
                        });
                    }
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + p.breastwork_height_metres
                                + p.coping_height_metres * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(
                            tangent_length + 0.03,
                            p.coping_height_metres,
                            p.thickness_metres + 0.04,
                        ),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Coping,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                }
                let circumference = std::f32::consts::TAU * radius_metres;
                let merlon_count = (circumference / (p.merlon_width_metres + p.crenel_width_metres))
                    .floor()
                    .max(4.0) as usize;
                let pitch = circumference / merlon_count as f32;
                let merlon_width = pitch - p.crenel_width_metres;
                for index in 0..merlon_count {
                    let angle = index as f32 * std::f32::consts::TAU / merlon_count as f32;
                    if outer_merlon_sector_open(angle) {
                        continue;
                    }
                    let radial = Vec2::new(angle.cos(), angle.sin());
                    let plan = centre + radial * (radius_metres + p.thickness_metres * 0.5);
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + p.breastwork_height_metres
                                + p.merlon_height_metres * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(merlon_width, p.merlon_height_metres, p.thickness_metres),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Merlon,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + p.breastwork_height_metres
                                + p.merlon_height_metres
                                + p.coping_height_metres * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(
                            merlon_width,
                            p.coping_height_metres,
                            p.thickness_metres + 0.04,
                        ),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Coping,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                }
                if let Some(WallWalk::Round {
                    stairwell_radius_metres,
                    ..
                }) = walks.iter().find(|walk| matches!(walk, WallWalk::Round { centre: walk_centre, .. } if (*walk_centre-centre).length()<0.02))
                {
                    for index in 0..24 {
                        let angle = index as f32 * std::f32::consts::TAU / 24.0;
                        let radial = Vec2::new(angle.cos(), angle.sin());
                        let radius = *stairwell_radius_metres + 0.08;
                        if stair_arrival.is_some_and(|arrival| {
                            angular_distance(angle, arrival)
                                <= segment_angle + 0.45 / radius
                        }) {
                            continue;
                        }
                        let plan = centre + radial * radius;
                        geometry.solids.push(ResolvedSolid {
                            id: ResolvedItemId::default(),
                            owner: crown.owner,
                            centre: Vec3::new(
                                plan.x,
                                crown.base_height_metres + p.inner_guard_height_metres * 0.5,
                                plan.y,
                            ),
                            size: Vec3::new(
                                std::f32::consts::TAU * radius / 24.0 + 0.02,
                                p.inner_guard_height_metres,
                                0.12,
                            ),
                            yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                            crossfall_radians: 0.0,
                            longfall_radians: 0.0,
                            role: SolidRole::EdgeGuard,
                            shape: crate::ResolvedSolidShape::Cuboid,
                            supported_by: vec![support_node],
                        });
                    }
                }
                geometry.surfaces.push(ResolvedSurface {
                    id: ResolvedItemId::default(),
                    owner: crown.owner,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            centre.x - radius_metres,
                            crown.base_height_metres - 0.08,
                            centre.y - radius_metres,
                        ),
                        max: Vec3::new(
                            centre.x + radius_metres,
                            crown.base_height_metres,
                            centre.y + radius_metres,
                        ),
                    },
                    role: SurfaceRole::Stance,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
                geometry.surfaces.push(ResolvedSurface {
                    id: ResolvedItemId::default(),
                    owner: crown.owner,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            centre.x - radius_metres,
                            crown.base_height_metres + p.firing_height_metres,
                            centre.y - radius_metres,
                        ),
                        max: Vec3::new(
                            centre.x + radius_metres,
                            crown.base_height_metres + p.firing_height_metres + 0.01,
                            centre.y + radius_metres,
                        ),
                    },
                    role: SurfaceRole::FiringLine,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
                for drain in crown.drain_positions.iter().filter(|drain| {
                    let radial = **drain - centre;
                    !outer_sector_open(radial.y.atan2(radial.x))
                }) {
                    let outward = (*drain - centre).normalize_or_zero();
                    let tangent = Vec2::new(-outward.y, outward.x);
                    let inner = *drain + outward * 0.01;
                    let outer = *drain + outward * (p.thickness_metres + 0.01);
                    let lateral = tangent.abs() * 0.06;
                    geometry.voids.push(ResolvedVoid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                inner.x.min(outer.x) - lateral.x,
                                crown.base_height_metres,
                                inner.y.min(outer.y) - lateral.y,
                            ),
                            max: Vec3::new(
                                inner.x.max(outer.x) + lateral.x,
                                crown.base_height_metres + 0.18,
                                inner.y.max(outer.y) + lateral.y,
                            ),
                        },
                        role: VoidRole::Drain,
                        shape: crate::ResolvedVoidShape::Box,
                        subtracts_from: crown.owner,
                    });
                }
            }
        }
        for junction in crown.junctions.iter().filter(|junction| {
            junction.kind == CrownJunctionKind::Corner && crown.owner.0 < junction.other_owner.0
        }) {
            geometry.solids.push(ResolvedSolid {
                id: ResolvedItemId::default(),
                owner: crown.owner,
                centre: Vec3::new(
                    junction.position.x,
                    crown.base_height_metres
                        + p.breastwork_height_metres
                        + p.coping_height_metres
                        + (p.merlon_height_metres - p.coping_height_metres) * 0.5,
                    junction.position.y,
                ),
                size: Vec3::new(
                    p.merlon_width_metres,
                    p.merlon_height_metres - p.coping_height_metres,
                    p.merlon_width_metres,
                ),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::Merlon,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![support_node],
            });
            geometry.solids.push(ResolvedSolid {
                id: ResolvedItemId::default(),
                owner: crown.owner,
                centre: Vec3::new(
                    junction.position.x,
                    crown.base_height_metres
                        + p.breastwork_height_metres
                        + p.merlon_height_metres
                        + p.coping_height_metres * 0.5,
                    junction.position.y,
                ),
                size: Vec3::new(
                    p.merlon_width_metres + 0.04,
                    p.coping_height_metres,
                    p.merlon_width_metres + 0.04,
                ),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::Coping,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![support_node],
            });
        }
        geometry.structural_nodes.push(StructuralNode {
            id: support_node,
            owner: crown.owner,
            kind: if matches!(crown.path, CrownPath::Round { .. }) {
                StructuralNodeKind::TowerShellBearing
            } else {
                StructuralNodeKind::WallBearing
            },
            position: match crown.path {
                CrownPath::Straight { start, end, .. } => Vec3::new(
                    (start.x + end.x) * 0.5,
                    crown.base_height_metres,
                    (start.y + end.y) * 0.5,
                ),
                CrownPath::Round { centre, .. } => {
                    Vec3::new(centre.x, crown.base_height_metres, centre.y)
                }
            },
            supported_by: Vec::new(),
            grounded: true,
        });
    }
    for (index, solid) in geometry.solids.iter_mut().enumerate() {
        solid.id = ResolvedItemId((1_u64 << 60) | (u64::from(solid.owner.0) << 32) | index as u64);
        if solid.role == SolidRole::Coping {
            solid.crossfall_radians = 0.045;
        }
        let bounds = resolved_axis_bounds(solid.centre, solid.size);
        geometry.support_interfaces.push(SupportInterface {
            id: ResolvedItemId((4_u64 << 60) | index as u64),
            owner: solid.owner,
            node: solid.supported_by[0],
            bounds: ResolvedBounds {
                min: Vec3::new(bounds.min.x, bounds.min.y - 0.015, bounds.min.z),
                max: Vec3::new(bounds.max.x, bounds.min.y + 0.015, bounds.max.z),
            },
        });
    }
    for (index, surface) in geometry.surfaces.iter_mut().enumerate() {
        surface.id =
            ResolvedItemId((2_u64 << 60) | (u64::from(surface.owner.0) << 32) | index as u64);
    }
    for (index, void) in geometry.voids.iter_mut().enumerate() {
        void.id = ResolvedItemId((3_u64 << 60) | (u64::from(void.owner.0) << 32) | index as u64);
        let crown = crowns
            .iter()
            .find(|crown| crown.owner == void.owner)
            .expect("resolved void crown owner");
        let centre = (void.bounds.min + void.bounds.max) * 0.5;
        let outward = match crown.path {
            CrownPath::Straight { outward, .. } => direction_vector(outward),
            CrownPath::Round { centre: tower, .. } => {
                (Vec2::new(centre.x, centre.z) - tower).normalize_or_zero()
            }
        };
        geometry.drainage_routes.push(DrainageRoute {
            id: ResolvedItemId((5_u64 << 60) | index as u64),
            owner: void.owner,
            outlet_void: void.id,
            inlet: Vec3::new(
                centre.x - outward.x * (crown.profile.thickness_metres * 0.5 + 0.01),
                crown.base_height_metres - 0.02,
                centre.z - outward.y * (crown.profile.thickness_metres * 0.5 + 0.01),
            ),
            outlet: Vec3::new(
                centre.x + outward.x * 0.35,
                crown.base_height_metres - 0.08,
                centre.z + outward.y * 0.35,
            ),
        });
    }
    // The wall-walk catchment is resolved as physical geometry rather than as
    // a nominal drainage arrow. Local +X follows the walk and local +Z is the
    // transverse axis; the signed crossfall therefore has one unambiguous
    // downhill direction in every cardinal orientation. The 60 mm fall is a
    // project readability/drainage gate, not a universal historical dimension.
    for crown in crowns {
        let routes = geometry
            .drainage_routes
            .iter()
            .filter(|route| route.owner == crown.owner)
            .copied()
            .collect::<Vec<_>>();
        if routes.is_empty() {
            continue;
        }
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let tangent = (end - start).normalize_or_zero();
                let outward = direction_vector(outward);
                let length = (end - start).length();
                // At a right-angle corner one walk owns the shared square and
                // the other butts into it. Owner ordering is deterministic and
                // avoids two independently rendered slabs occupying the same
                // volume while preserving continuous walking area.
                let delegated_corner_trim = |position: Vec2| {
                    crown
                        .junctions
                        .iter()
                        .find(|junction| {
                            junction.kind == CrownJunctionKind::Corner
                                && (junction.position - position).length() < 0.02
                                && crown.owner > junction.other_owner
                        })
                        .map_or(0.0, |_| {
                            crown.profile.walk_clear_width_metres + crown.profile.thickness_metres
                        })
                };
                let start_trim = delegated_corner_trim(start);
                let end_trim = delegated_corner_trim(end);
                let mut exclusions = crown
                    .junctions
                    .iter()
                    .filter_map(|junction| {
                        let other = crowns
                            .iter()
                            .find(|other| other.owner == junction.other_owner)?;
                        let CrownPath::Round { radius_metres, .. } = other.path else {
                            return None;
                        };
                        let distance = (junction.position - start).dot(tangent);
                        (distance >= -0.02 && distance <= length + 0.02).then_some((
                            (distance - radius_metres - crown.profile.thickness_metres * 0.5
                                + 0.08)
                                .max(0.0),
                            (distance + radius_metres + crown.profile.thickness_metres * 0.5
                                - 0.08)
                                .min(length),
                        ))
                    })
                    .collect::<Vec<_>>();
                exclusions.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut active_ranges = Vec::new();
                let mut cursor = start_trim;
                for (cut_start, cut_end) in exclusions {
                    if cut_start > cursor + 0.02 {
                        active_ranges.push((cursor, cut_start));
                    }
                    cursor = cursor.max(cut_end);
                }
                let owned_end = length - end_trim;
                if owned_end > cursor + 0.02 {
                    active_ranges.push((cursor, owned_end));
                }
                for (range_start, range_end) in active_ranges {
                    let mut basin_routes = routes
                        .iter()
                        .map(|route| {
                            let along =
                                (Vec2::new(route.inlet.x, route.inlet.z) - start).dot(tangent);
                            (along, *route)
                        })
                        .filter(|(along, _)| {
                            *along >= range_start - 0.02 && *along <= range_end + 0.02
                        })
                        .collect::<Vec<_>>();
                    basin_routes.sort_by(|a, b| a.0.total_cmp(&b.0));
                    for (index, (along, route)) in basin_routes.iter().enumerate() {
                        let left = if index == 0 {
                            range_start
                        } else {
                            (basin_routes[index - 1].0 + *along) * 0.5
                        };
                        let right = if index + 1 == basin_routes.len() {
                            range_end
                        } else {
                            (*along + basin_routes[index + 1].0) * 0.5
                        };
                        for (half_start, half_end) in [(left, *along), (*along, right)] {
                            if half_end <= half_start + 0.03 {
                                continue;
                            }
                            let centre = start + tangent * ((half_start + half_end) * 0.5)
                                - outward
                                    * (crown.profile.walk_clear_width_metres * 0.5
                                        + crown.profile.thickness_metres * 0.5);
                            push_drainage_catchment(
                                &mut geometry,
                                crown,
                                *route,
                                centre,
                                tangent,
                                outward,
                                half_end - half_start,
                            );
                        }
                    }
                }
            }
            CrownPath::Round {
                centre,
                radius_metres,
                ..
            } => {
                let segment_count = 24;
                // Faceted deck chords sit slightly inside the mathematical
                // circle so their outer corners do not obstruct the scupper
                // mouths in the round breastwork.
                let deck_radius = radius_metres
                    - crown.profile.thickness_metres * 0.5
                    - crown.profile.walk_clear_width_metres * 0.5
                    - 0.03;
                let outer_walk_radius = deck_radius + crown.profile.walk_clear_width_metres * 0.5;
                for index in 0..segment_count {
                    let angle = index as f32 * std::f32::consts::TAU / segment_count as f32;
                    let outward = Vec2::new(angle.cos(), angle.sin());
                    let tangent = Vec2::new(-outward.y, outward.x);
                    let segment_centre = centre + outward * deck_radius;
                    let route = routes
                        .iter()
                        .min_by(|a, b| {
                            let a_direction = Vec2::new(a.outlet.x, a.outlet.z) - centre;
                            let b_direction = Vec2::new(b.outlet.x, b.outlet.z) - centre;
                            let a_dot = a_direction.normalize_or_zero().dot(outward);
                            let b_dot = b_direction.normalize_or_zero().dot(outward);
                            b_dot.total_cmp(&a_dot)
                        })
                        .expect("round crown has a drainage route");
                    let full_length = 2.0
                        * outer_walk_radius
                        * (std::f32::consts::PI / segment_count as f32).tan()
                        + 0.02;
                    for side in [-1.0_f32, 1.0] {
                        push_drainage_catchment(
                            &mut geometry,
                            crown,
                            *route,
                            segment_centre + tangent * side * full_length * 0.25,
                            tangent,
                            outward,
                            full_length * 0.5,
                        );
                    }
                }
            }
        }
    }
    for crown in crowns {
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let normal = direction_vector(outward);
                let mut accepted = Vec::new();
                for step in 1..120 {
                    let sample = step as f32 / 120.0;
                    let line = start.lerp(end, sample);
                    let firing_point = Vec3::new(
                        line.x + normal.x * crown.profile.thickness_metres * 0.5,
                        crown.base_height_metres + crown.profile.firing_height_metres,
                        line.y + normal.y * crown.profile.thickness_metres * 0.5,
                    );
                    let blocked = geometry.solids.iter().any(|solid| {
                        solid.owner == crown.owner && solid.role == SolidRole::Merlon && {
                            resolved_solid_contains_point(solid, firing_point, 0.005)
                        }
                    });
                    if !blocked
                        && accepted
                            .last()
                            .is_none_or(|previous: &Vec2| previous.distance(line) >= 1.0)
                    {
                        accepted.push(line);
                    }
                    if accepted.len() == 3 {
                        break;
                    }
                }
                for line in accepted {
                    let stance = line - normal * 0.55;
                    geometry.defender_samples.push(DefenderSample {
                        owner: crown.owner,
                        stance: Vec3::new(stance.x, crown.base_height_metres, stance.y),
                        eye: Vec3::new(stance.x, crown.base_height_metres + 1.62, stance.y),
                        target: Vec3::new(
                            line.x + normal.x * 12.0,
                            crown.base_height_metres + 1.2,
                            line.y + normal.y * 12.0,
                        ),
                    });
                }
            }
            CrownPath::Round {
                centre,
                radius_metres,
                ..
            } => {
                let mut accepted = 0;
                for sample in 0..48 {
                    let angle = sample as f32 * std::f32::consts::TAU / 48.0;
                    let radial = Vec2::new(angle.cos(), angle.sin());
                    let firing_point = Vec3::new(
                        centre.x
                            + radial.x * (radius_metres + crown.profile.thickness_metres * 0.5),
                        crown.base_height_metres + crown.profile.firing_height_metres,
                        centre.y
                            + radial.y * (radius_metres + crown.profile.thickness_metres * 0.5),
                    );
                    let blocked = geometry.solids.iter().any(|solid| {
                        solid.owner == crown.owner && solid.role == SolidRole::Merlon && {
                            resolved_solid_contains_point(solid, firing_point, 0.005)
                        }
                    });
                    if blocked {
                        continue;
                    }
                    let stance = centre + radial * (radius_metres - 0.55);
                    geometry.defender_samples.push(DefenderSample {
                        owner: crown.owner,
                        stance: Vec3::new(stance.x, crown.base_height_metres, stance.y),
                        eye: Vec3::new(stance.x, crown.base_height_metres + 1.62, stance.y),
                        target: Vec3::new(
                            centre.x + radial.x * (radius_metres + 12.0),
                            crown.base_height_metres + 1.2,
                            centre.y + radial.y * (radius_metres + 12.0),
                        ),
                    });
                    accepted += 1;
                    if accepted == 8 {
                        break;
                    }
                }
            }
        }
    }
    let mut seen_bonds = std::collections::BTreeSet::new();
    for crown in crowns {
        for junction in &crown.junctions {
            let pair = if crown.owner < junction.other_owner {
                [crown.owner, junction.other_owner]
            } else {
                [junction.other_owner, crown.owner]
            };
            let (mut positions, tangent) = match crown.path {
                CrownPath::Straight { start, end, .. }
                    if junction.kind == CrownJunctionKind::TowerSplice =>
                {
                    let tangent = (end - start).normalize_or_zero();
                    let Some((tower_centre, tower_radius)) = crowns
                        .iter()
                        .find_map(|other| {
                            (other.owner == junction.other_owner).then_some(other.path)
                        })
                        .and_then(|path| match path {
                            CrownPath::Round {
                                centre,
                                radius_metres,
                                ..
                            } => Some((centre, radius_metres)),
                            CrownPath::Straight { .. } => None,
                        })
                    else {
                        continue;
                    };
                    let offset = tower_radius + crown.profile.thickness_metres * 0.5 - 0.08;
                    let positions = if (junction.position - start).length() < 0.02 {
                        vec![tower_centre + tangent * offset]
                    } else if (junction.position - end).length() < 0.02 {
                        vec![tower_centre - tangent * offset]
                    } else {
                        vec![
                            tower_centre - tangent * offset,
                            tower_centre + tangent * offset,
                        ]
                    };
                    (positions, Some(tangent))
                }
                CrownPath::Round { .. } if junction.kind == CrownJunctionKind::TowerSplice => {
                    continue;
                }
                _ => (vec![junction.position], None),
            };
            if let CrownPath::Straight { outward, .. } = crown.path {
                let inward =
                    -direction_vector(outward) * (crown.profile.walk_clear_width_metres + 0.08);
                if junction.kind == CrownJunctionKind::TowerSplice {
                    let guard_positions = positions
                        .iter()
                        .map(|position| *position + inward)
                        .collect::<Vec<_>>();
                    positions.extend(guard_positions);
                } else if let Some(other_outward) = crowns
                    .iter()
                    .find_map(|other| (other.owner == junction.other_owner).then_some(other.path))
                    .and_then(|path| match path {
                        CrownPath::Straight { outward, .. } => Some(outward),
                        CrownPath::Round { .. } => None,
                    })
                {
                    positions.push(
                        junction.position + inward
                            - direction_vector(other_outward)
                                * (crown.profile.walk_clear_width_metres + 0.08),
                    );
                }
            }
            for position in positions {
                if !seen_bonds.insert((
                    pair[0].0,
                    pair[1].0,
                    position.x.to_bits(),
                    position.y.to_bits(),
                )) {
                    continue;
                }
                let half = tangent.map_or(Vec2::splat(0.40), |tangent| {
                    if tangent.x.abs() >= tangent.y.abs() {
                        Vec2::new(0.12, crown.profile.thickness_metres * 0.8)
                    } else {
                        Vec2::new(crown.profile.thickness_metres * 0.8, 0.12)
                    }
                });
                let mut bond_height = crown.profile.breastwork_height_metres
                    + crown.profile.merlon_height_metres
                    + crown.profile.coping_height_metres;
                if let Some(tangent) = tangent
                    && let Some(round_owner) = pair.iter().copied().find(|owner| {
                        crowns.iter().any(|other| {
                            other.owner == *owner && matches!(other.path, CrownPath::Round { .. })
                        })
                    })
                    && let Some(node) = geometry
                        .structural_nodes
                        .iter()
                        .find(|node| node.owner == round_owner)
                        .map(|node| node.id)
                {
                    let offset_from_wall = match crown.path {
                        CrownPath::Straight { start, .. } => {
                            (position - start).perp_dot(tangent).abs()
                        }
                        CrownPath::Round { .. } => 0.0,
                    };
                    let (role, transverse, height) = if offset_from_wall > 0.5 {
                        (
                            SolidRole::EdgeGuard,
                            0.12,
                            crown.profile.inner_guard_height_metres,
                        )
                    } else {
                        (
                            SolidRole::Breastwork,
                            crown.profile.thickness_metres,
                            crown.profile.breastwork_height_metres,
                        )
                    };
                    bond_height = height;
                    let size = if tangent.x.abs() >= tangent.y.abs() {
                        Vec3::new(0.16, height, transverse)
                    } else {
                        Vec3::new(transverse, height, 0.16)
                    };
                    let solid_index = geometry.solids.len();
                    let solid = ResolvedSolid {
                        id: ResolvedItemId(
                            (1_u64 << 60) | (u64::from(round_owner.0) << 32) | solid_index as u64,
                        ),
                        owner: round_owner,
                        centre: Vec3::new(
                            position.x,
                            crown.base_height_metres + height * 0.5,
                            position.y,
                        ),
                        size,
                        yaw_radians: 0.0,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![node],
                    };
                    let bounds = resolved_axis_bounds(solid.centre, solid.size);
                    geometry.support_interfaces.push(SupportInterface {
                        id: ResolvedItemId((4_u64 << 60) | solid_index as u64),
                        owner: round_owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(bounds.min.x, bounds.min.y - 0.015, bounds.min.z),
                            max: Vec3::new(bounds.max.x, bounds.min.y + 0.015, bounds.max.z),
                        },
                    });
                    geometry.solids.push(solid);
                }
                geometry.junction_bonds.push(JunctionBond {
                    id: ResolvedItemId((6_u64 << 60) | geometry.junction_bonds.len() as u64),
                    owners: pair,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            position.x - half.x,
                            crown.base_height_metres - 0.1,
                            position.y - half.y,
                        ),
                        max: Vec3::new(
                            position.x + half.x,
                            crown.base_height_metres + bond_height,
                            position.y + half.y,
                        ),
                    },
                    minimum_interface_area_square_metres: 0.08,
                    maximum_penetration_metres: 0.18,
                });
            }
        }
    }
    geometry
}
