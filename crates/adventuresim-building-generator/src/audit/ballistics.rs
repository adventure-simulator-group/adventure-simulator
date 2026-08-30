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
