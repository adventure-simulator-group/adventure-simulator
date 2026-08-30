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
