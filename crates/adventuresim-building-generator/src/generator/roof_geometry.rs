fn roof_plane(polygon: &[Vec3]) -> RoofPlaneEquation {
    let mut normal = (polygon[1] - polygon[0])
        .cross(polygon[2] - polygon[0])
        .normalize_or_zero();
    if normal.y < 0.0 {
        normal = -normal;
    }
    RoofPlaneEquation {
        normal,
        constant: -normal.dot(polygon[0]),
    }
}

fn roof_polygon_bounds(polygon: &[Vec3]) -> ResolvedBounds {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for point in polygon {
        min = min.min(*point);
        max = max.max(*point);
    }
    ResolvedBounds { min, max }
}

fn roof_face_polygons(roof: RoofPiece, shed_high_side: Option<Direction>) -> Vec<Vec<Vec3>> {
    let hx = roof.size.x * 0.5 + roof.eave_metres;
    let hz = roof.size.y * 0.5 + roof.eave_metres;
    let y = roof.base_height_metres;
    let corners = [
        Vec3::new(roof.centre.x - hx, y, roof.centre.y - hz),
        Vec3::new(roof.centre.x + hx, y, roof.centre.y - hz),
        Vec3::new(roof.centre.x + hx, y, roof.centre.y + hz),
        Vec3::new(roof.centre.x - hx, y, roof.centre.y + hz),
    ];
    let pitch = roof.pitch_degrees.to_radians();
    match roof.kind {
        RoofKind::Gable => match roof.ridge_axis {
            RidgeAxis::Z => {
                let rise = hx * pitch.tan();
                let a = Vec3::new(roof.centre.x, y + rise, roof.centre.y - hz);
                let b = Vec3::new(roof.centre.x, y + rise, roof.centre.y + hz);
                vec![
                    vec![corners[0], corners[3], b, a],
                    vec![corners[2], corners[1], a, b],
                ]
            }
            RidgeAxis::X => {
                let rise = hz * pitch.tan();
                let a = Vec3::new(roof.centre.x - hx, y + rise, roof.centre.y);
                let b = Vec3::new(roof.centre.x + hx, y + rise, roof.centre.y);
                vec![
                    vec![corners[1], corners[0], a, b],
                    vec![corners[3], corners[2], b, a],
                ]
            }
        },
        RoofKind::Shed => {
            let rise = match roof.ridge_axis {
                RidgeAxis::Z => hx * 2.0,
                RidgeAxis::X => hz * 2.0,
            } * pitch.tan();
            match roof.ridge_axis {
                RidgeAxis::Z if shed_high_side == Some(Direction::West) => vec![vec![
                    corners[0] + Vec3::Y * rise,
                    corners[3] + Vec3::Y * rise,
                    corners[2],
                    corners[1],
                ]],
                RidgeAxis::Z => vec![vec![
                    corners[0],
                    corners[3],
                    corners[2] + Vec3::Y * rise,
                    corners[1] + Vec3::Y * rise,
                ]],
                RidgeAxis::X if shed_high_side == Some(Direction::South) => vec![vec![
                    corners[0] + Vec3::Y * rise,
                    corners[1] + Vec3::Y * rise,
                    corners[2],
                    corners[3],
                ]],
                RidgeAxis::X => vec![vec![
                    corners[0],
                    corners[1],
                    corners[2] + Vec3::Y * rise,
                    corners[3] + Vec3::Y * rise,
                ]],
            }
        }
        RoofKind::Flat => vec![corners.to_vec()],
        RoofKind::HalfHip => {
            // Project half-hip profile: the upper 45% of each gable is folded
            // back as a short hip while the lower gable remains vertical.
            // This is topology, not a label or a shortened full-hip ridge.
            let shoulder_fraction = 0.55;
            match roof.ridge_axis {
                RidgeAxis::Z => {
                    let rise = hx * pitch.tan();
                    let shoulder_x = hx * (1.0 - shoulder_fraction);
                    let shoulder_y = y + rise * shoulder_fraction;
                    let ridge_half = (hz - hx * 0.45).max(0.0);
                    let south_ridge =
                        Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half);
                    let north_ridge =
                        Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half);
                    let south_w =
                        Vec3::new(roof.centre.x - shoulder_x, shoulder_y, roof.centre.y - hz);
                    let south_e =
                        Vec3::new(roof.centre.x + shoulder_x, shoulder_y, roof.centre.y - hz);
                    let north_w =
                        Vec3::new(roof.centre.x - shoulder_x, shoulder_y, roof.centre.y + hz);
                    let north_e =
                        Vec3::new(roof.centre.x + shoulder_x, shoulder_y, roof.centre.y + hz);
                    vec![
                        vec![
                            corners[0],
                            corners[3],
                            north_w,
                            north_ridge,
                            south_ridge,
                            south_w,
                        ],
                        vec![
                            corners[2],
                            corners[1],
                            south_e,
                            south_ridge,
                            north_ridge,
                            north_e,
                        ],
                        vec![south_w, south_ridge, south_e],
                        vec![north_e, north_ridge, north_w],
                    ]
                }
                RidgeAxis::X => {
                    let rise = hz * pitch.tan();
                    let shoulder_z = hz * (1.0 - shoulder_fraction);
                    let shoulder_y = y + rise * shoulder_fraction;
                    let ridge_half = (hx - hz * 0.45).max(0.0);
                    let west_ridge = Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y);
                    let east_ridge = Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y);
                    let west_s =
                        Vec3::new(roof.centre.x - hx, shoulder_y, roof.centre.y - shoulder_z);
                    let west_n =
                        Vec3::new(roof.centre.x - hx, shoulder_y, roof.centre.y + shoulder_z);
                    let east_s =
                        Vec3::new(roof.centre.x + hx, shoulder_y, roof.centre.y - shoulder_z);
                    let east_n =
                        Vec3::new(roof.centre.x + hx, shoulder_y, roof.centre.y + shoulder_z);
                    vec![
                        vec![
                            corners[1], corners[0], west_s, west_ridge, east_ridge, east_s,
                        ],
                        vec![
                            corners[3], corners[2], east_n, east_ridge, west_ridge, west_n,
                        ],
                        vec![west_n, west_ridge, west_s],
                        vec![east_s, east_ridge, east_n],
                    ]
                }
            }
        }
        RoofKind::Hip | RoofKind::Pavilion => {
            let (ridge_half, rise) = match roof.ridge_axis {
                RidgeAxis::Z => {
                    let inset = if roof.kind == RoofKind::Pavilion {
                        hz
                    } else {
                        hx.min(hz * 0.85)
                    };
                    ((hz - inset).max(0.0), hx * pitch.tan())
                }
                RidgeAxis::X => {
                    let inset = if roof.kind == RoofKind::Pavilion {
                        hx
                    } else {
                        hz.min(hx * 0.85)
                    };
                    ((hx - inset).max(0.0), hz * pitch.tan())
                }
            };
            if roof.kind == RoofKind::Pavilion {
                let apex = Vec3::new(roof.centre.x, y + rise, roof.centre.y);
                vec![
                    vec![corners[0], corners[3], apex],
                    vec![corners[2], corners[1], apex],
                    vec![corners[1], corners[0], apex],
                    vec![corners[3], corners[2], apex],
                ]
            } else {
                match roof.ridge_axis {
                    RidgeAxis::Z => {
                        let a = Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half);
                        let b = Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half);
                        vec![
                            vec![corners[0], corners[3], b, a],
                            vec![corners[2], corners[1], a, b],
                            vec![corners[1], corners[0], a],
                            vec![corners[3], corners[2], b],
                        ]
                    }
                    RidgeAxis::X => {
                        let a = Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y);
                        let b = Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y);
                        vec![
                            vec![corners[1], corners[0], a, b],
                            vec![corners[3], corners[2], b, a],
                            vec![corners[0], corners[3], a],
                            vec![corners[2], corners[1], b],
                        ]
                    }
                }
            }
        }
        RoofKind::Conical => {
            let radius = hx.max(hz);
            let apex = Vec3::new(roof.centre.x, y + radius * pitch.tan(), roof.centre.y);
            (0..24)
                .map(|index| {
                    let a = std::f32::consts::TAU * index as f32 / 24.0;
                    let b = std::f32::consts::TAU * (index + 1) as f32 / 24.0;
                    vec![
                        Vec3::new(
                            roof.centre.x + a.cos() * radius,
                            y,
                            roof.centre.y + a.sin() * radius,
                        ),
                        Vec3::new(
                            roof.centre.x + b.cos() * radius,
                            y,
                            roof.centre.y + b.sin() * radius,
                        ),
                        apex,
                    ]
                })
                .collect()
        }
    }
}

fn same_roof_vertex(left: Vec3, right: Vec3) -> bool {
    (left - right).length_squared() <= 0.000_004
}

fn clip_plan_polygon_to_rect(mut polygon: Vec<Vec2>, min: Vec2, max: Vec2) -> Vec<Vec2> {
    for (axis, value, keep_greater) in [
        (0_usize, min.x, true),
        (0, max.x, false),
        (1, min.y, true),
        (1, max.y, false),
    ] {
        if polygon.is_empty() {
            break;
        }
        let input = std::mem::take(&mut polygon);
        let coordinate = |point: Vec2| if axis == 0 { point.x } else { point.y };
        let inside = |point: Vec2| {
            if keep_greater {
                coordinate(point) >= value - 0.0001
            } else {
                coordinate(point) <= value + 0.0001
            }
        };
        for index in 0..input.len() {
            let current = input[index];
            let previous = input[(index + input.len() - 1) % input.len()];
            let current_inside = inside(current);
            let previous_inside = inside(previous);
            if current_inside != previous_inside {
                let denominator = coordinate(current) - coordinate(previous);
                let fraction = if denominator.abs() <= 0.000_001 {
                    0.0
                } else {
                    (value - coordinate(previous)) / denominator
                };
                polygon.push(previous.lerp(current, fraction));
            }
            if current_inside {
                polygon.push(current);
            }
        }
    }
    polygon
}

fn signed_plan_area(polygon: &[Vec2]) -> f32 {
    polygon
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = polygon[(index + 1) % polygon.len()];
            point.x * next.y - next.x * point.y
        })
        .sum::<f32>()
        * 0.5
}

fn convex_plan_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.sort_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then_with(|| left.y.total_cmp(&right.y))
    });
    points.dedup_by(|left, right| left.distance_squared(*right) <= 0.000_004);
    if points.len() < 3 {
        return points;
    }
    let cross = |origin: Vec2, a: Vec2, b: Vec2| (a - origin).perp_dot(b - origin);
    let mut lower = Vec::new();
    for point in points.iter().copied() {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.000_002
        {
            lower.pop();
        }
        lower.push(point);
    }
    let mut upper = Vec::new();
    for point in points.iter().rev().copied() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.000_002
        {
            upper.pop();
        }
        upper.push(point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn clip_plan_polygon_to_convex(mut polygon: Vec<Vec2>, clip: &[Vec2]) -> Vec<Vec2> {
    if clip.len() < 3 {
        return Vec::new();
    }
    let orientation = signed_plan_area(clip).signum();
    for edge_index in 0..clip.len() {
        if polygon.is_empty() {
            break;
        }
        let edge_start = clip[edge_index];
        let edge_end = clip[(edge_index + 1) % clip.len()];
        let edge = edge_end - edge_start;
        let side = |point: Vec2| orientation * edge.perp_dot(point - edge_start);
        let input = std::mem::take(&mut polygon);
        for index in 0..input.len() {
            let current = input[index];
            let previous = input[(index + input.len() - 1) % input.len()];
            let current_side = side(current);
            let previous_side = side(previous);
            let current_inside = current_side >= -0.0001;
            let previous_inside = previous_side >= -0.0001;
            if current_inside != previous_inside {
                let denominator = previous_side - current_side;
                let fraction = if denominator.abs() <= 0.000_001 {
                    0.0
                } else {
                    previous_side / denominator
                };
                polygon.push(previous.lerp(current, fraction));
            }
            if current_inside {
                polygon.push(current);
            }
        }
    }
    polygon
}

fn roof_plane_height(plane: RoofPlaneEquation, point: Vec2) -> f32 {
    -(plane.normal.x * point.x + plane.normal.z * point.y + plane.constant) / plane.normal.y
}

fn plan_point_in_convex_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let orientation = signed_plan_area(polygon).signum();
    polygon.iter().enumerate().all(|(index, start)| {
        let end = polygon[(index + 1) % polygon.len()];
        orientation * (end - *start).perp_dot(point - *start) >= -0.002
    })
}

fn plan_point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for index in 0..polygon.len() {
        let start = polygon[index];
        let end = polygon[(index + 1) % polygon.len()];
        let crosses = (start.y > point.y) != (end.y > point.y)
            && point.x
                < (end.x - start.x) * (point.y - start.y) / (end.y - start.y).abs().max(0.000_001)
                    * (end.y - start.y).signum()
                    + start.x;
        if crosses {
            inside = !inside;
        }
    }
    inside
}

fn roof_surface_height_at(assembly: &RoofAssembly, point: Vec2) -> Option<f32> {
    assembly.faces.iter().find_map(|face| {
        let projected = face
            .polygon
            .iter()
            .map(|vertex| Vec2::new(vertex.x, vertex.z))
            .collect::<Vec<_>>();
        plan_point_in_convex_polygon(point, &projected)
            .then(|| roof_plane_height(face.plane, point))
    })
}

fn roof_underside_height_at(assembly: &RoofAssembly, point: Vec2) -> Option<f32> {
    assembly.faces.iter().find_map(|face| {
        let projected = face
            .polygon
            .iter()
            .map(|vertex| Vec2::new(vertex.x, vertex.z))
            .collect::<Vec<_>>();
        plan_point_in_convex_polygon(point, &projected).then(|| {
            roof_plane_height(face.plane, point)
                - face.plane.normal.normalize_or_zero().y * face.thickness_metres
        })
    })
}

fn ray_segment_intersection(origin: Vec2, direction: Vec2, a: Vec2, b: Vec2) -> Option<Vec2> {
    let edge = b - a;
    let denominator = direction.perp_dot(edge);
    if denominator.abs() <= 0.000_001 {
        return None;
    }
    let offset = a - origin;
    let ray_t = offset.perp_dot(edge) / denominator;
    let edge_t = offset.perp_dot(direction) / denominator;
    (ray_t >= -0.002 && (-0.002..=1.002).contains(&edge_t))
        .then(|| origin + direction * ray_t.max(0.0))
}
