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
