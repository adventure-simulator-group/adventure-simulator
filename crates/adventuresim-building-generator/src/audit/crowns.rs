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
