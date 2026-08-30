#[allow(dead_code)] // Legacy recipe visualizer retained only for non-authoritative debugging.
fn spawn_roof(
    world: &mut World,
    palette: &RenderPalette,
    mut roof: RoofPiece,
    origin: Vec2,
    roof_index: usize,
    wall_style: WallStyle,
) {
    roof.centre += origin;
    match roof.kind {
        RoofKind::Gable => spawn_gable_roof(world, palette, roof, wall_style),
        RoofKind::Hip | RoofKind::HalfHip | RoofKind::Pavilion => {
            let mesh = roof_surface_mesh(roof);
            let handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
            let entity = world
                .spawn((
                    Name::new(format!("roof piece {roof_index}")),
                    Mesh3d(handle),
                    MeshMaterial3d(palette.roof_secondary.clone()),
                ))
                .id();
            tag_player_build_entity(world, entity, &palette.roof_secondary);
        }
        RoofKind::Shed => spawn_shed_roof(world, &palette.roof, roof),
        RoofKind::Flat => {
            spawn_box(
                world,
                &palette.roof_secondary,
                Vec3::new(roof.size.x, 0.18, roof.size.y),
                Vec3::new(roof.centre.x, roof.base_height_metres + 0.09, roof.centre.y),
                Quat::IDENTITY,
                "flat roof",
            );
        }
        RoofKind::Conical => spawn_conical_roof(world, &palette.roof_secondary, roof),
    }
}

#[allow(dead_code)]
fn spawn_gable_roof(
    world: &mut World,
    palette: &RenderPalette,
    roof: RoofPiece,
    wall_style: WallStyle,
) {
    let pitch = roof.pitch_degrees.to_radians();
    let (span, run) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x * 0.5 + roof.eave_metres,
            roof.size.y + roof.eave_metres * 2.0,
        ),
        RidgeAxis::X => (
            roof.size.y * 0.5 + roof.eave_metres,
            roof.size.x + roof.eave_metres * 2.0,
        ),
    };
    let slope = span / pitch.cos();
    let rise = span * pitch.tan();
    for sign in [-1.0_f32, 1.0] {
        let (size, translation, rotation) = match roof.ridge_axis {
            RidgeAxis::Z => (
                Vec3::new(slope, 0.13, run),
                Vec3::new(
                    roof.centre.x + sign * span * 0.5,
                    roof.base_height_metres + rise * 0.5,
                    roof.centre.y,
                ),
                Quat::from_rotation_z(-sign * pitch),
            ),
            RidgeAxis::X => (
                Vec3::new(run, 0.13, slope),
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres + rise * 0.5,
                    roof.centre.y + sign * span * 0.5,
                ),
                Quat::from_rotation_x(sign * pitch),
            ),
        };
        spawn_box(
            world,
            &palette.roof,
            size,
            translation,
            rotation,
            "gable roof slope",
        );
    }
    let facade_material = match wall_style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    let half_x = roof.size.x * 0.5;
    let half_z = roof.size.y * 0.5;
    let triangles = match roof.ridge_axis {
        RidgeAxis::Z => {
            let south = roof.centre.y - half_z;
            let north = roof.centre.y + half_z;
            vec![
                vec![
                    Vec3::new(roof.centre.x - half_x, roof.base_height_metres, south),
                    Vec3::new(roof.centre.x, roof.base_height_metres + rise, south),
                    Vec3::new(roof.centre.x + half_x, roof.base_height_metres, south),
                ],
                vec![
                    Vec3::new(roof.centre.x + half_x, roof.base_height_metres, north),
                    Vec3::new(roof.centre.x, roof.base_height_metres + rise, north),
                    Vec3::new(roof.centre.x - half_x, roof.base_height_metres, north),
                ],
            ]
        }
        RidgeAxis::X => {
            let west = roof.centre.x - half_x;
            let east = roof.centre.x + half_x;
            vec![
                vec![
                    Vec3::new(west, roof.base_height_metres, roof.centre.y + half_z),
                    Vec3::new(west, roof.base_height_metres + rise, roof.centre.y),
                    Vec3::new(west, roof.base_height_metres, roof.centre.y - half_z),
                ],
                vec![
                    Vec3::new(east, roof.base_height_metres, roof.centre.y - half_z),
                    Vec3::new(east, roof.base_height_metres + rise, roof.centre.y),
                    Vec3::new(east, roof.base_height_metres, roof.centre.y + half_z),
                ],
            ]
        }
    };
    let local_triangles = triangles
        .iter()
        .map(|triangle| {
            triangle
                .iter()
                .map(|point| {
                    point - Vec3::new(roof.centre.x, roof.base_height_metres, roof.centre.y)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(flat_face_mesh(&local_triangles));
    let entity = world
        .spawn((
            Name::new("gable infill"),
            Mesh3d(mesh),
            MeshMaterial3d(facade_material.clone()),
            Transform::from_xyz(roof.centre.x, roof.base_height_metres, roof.centre.y),
        ))
        .id();
    tag_player_build_entity(world, entity, facade_material);
    spawn_gable_detail(world, palette, roof, rise, wall_style);
}

#[allow(dead_code)]
fn spawn_gable_detail(
    world: &mut World,
    palette: &RenderPalette,
    roof: RoofPiece,
    rise: f32,
    wall_style: WallStyle,
) {
    let (half_span, face_a, face_b, tangent) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x * 0.5,
            Vec2::new(roof.centre.x, roof.centre.y - roof.size.y * 0.5 - 0.02),
            Vec2::new(roof.centre.x, roof.centre.y + roof.size.y * 0.5 + 0.02),
            Vec2::X,
        ),
        RidgeAxis::X => (
            roof.size.y * 0.5,
            Vec2::new(roof.centre.x - roof.size.x * 0.5 - 0.02, roof.centre.y),
            Vec2::new(roof.centre.x + roof.size.x * 0.5 + 0.02, roof.centre.y),
            Vec2::Y,
        ),
    };
    if wall_style == WallStyle::TimberFrame {
        for face in [face_a, face_b] {
            let apex = Vec3::new(face.x, roof.base_height_metres + rise, face.y);
            let base_left = face - tangent * half_span;
            let base_right = face + tangent * half_span;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(base_left.x, roof.base_height_metres, base_left.y),
                Vec3::new(base_right.x, roof.base_height_metres, base_right.y),
                0.13,
                "gable tie beam",
            );
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(face.x, roof.base_height_metres, face.y),
                apex,
                0.11,
                "gable king post",
            );
            let collar_y = roof.base_height_metres + rise * 0.56;
            let collar_half = half_span * 0.44;
            let collar_left = face - tangent * collar_half;
            let collar_right = face + tangent * collar_half;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(collar_left.x, collar_y, collar_left.y),
                Vec3::new(collar_right.x, collar_y, collar_right.y),
                0.105,
                "gable collar beam",
            );
            for fraction in [-0.66_f32, -0.33, 0.33, 0.66] {
                let stud = face + tangent * half_span * fraction;
                let top_y = roof.base_height_metres + rise * (1.0 - fraction.abs());
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(stud.x, roof.base_height_metres, stud.y),
                    Vec3::new(stud.x, top_y, stud.y),
                    0.085,
                    "gable vertical stud",
                );
            }
            for sign in [-1.0, 1.0] {
                let foot = face + tangent * half_span * 0.1 * sign;
                let head = face + tangent * half_span * 0.62 * sign;
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(foot.x, roof.base_height_metres + 0.06, foot.y),
                    Vec3::new(head.x, roof.base_height_metres + rise * 0.38, head.y),
                    0.09,
                    "gable outward brace",
                );
            }
        }
    }
    match roof.gable_profile {
        GableProfile::Plain => {}
        GableProfile::Stepped => {
            let material = if wall_style == WallStyle::TimberFrame {
                &palette.timber
            } else {
                &palette.stone
            };
            for face in [face_a, face_b] {
                for sign in [-1.0, 1.0] {
                    for step in 0..4 {
                        let lower = step as f32 / 4.0;
                        let upper = (step + 1) as f32 / 4.0;
                        let outer = face + tangent * half_span * (1.0 - lower) * sign;
                        let inner = face + tangent * half_span * (1.0 - upper) * sign;
                        spawn_timber_beam(
                            world,
                            material,
                            Vec3::new(outer.x, roof.base_height_metres + rise * lower, outer.y),
                            Vec3::new(outer.x, roof.base_height_metres + rise * upper, outer.y),
                            0.16,
                            "stepped gable vertical",
                        );
                        spawn_timber_beam(
                            world,
                            material,
                            Vec3::new(outer.x, roof.base_height_metres + rise * upper, outer.y),
                            Vec3::new(inner.x, roof.base_height_metres + rise * upper, inner.y),
                            0.16,
                            "stepped gable tread",
                        );
                    }
                }
            }
        }
        GableProfile::Curved => {
            for face in [face_a, face_b] {
                for sign in [-1.0, 1.0] {
                    let outer = face + tangent * half_span * 0.82 * sign;
                    let shoulder = face + tangent * half_span * 0.42 * sign;
                    spawn_timber_beam(
                        world,
                        &palette.stone,
                        Vec3::new(outer.x, roof.base_height_metres + rise * 0.12, outer.y),
                        Vec3::new(
                            shoulder.x,
                            roof.base_height_metres + rise * 0.58,
                            shoulder.y,
                        ),
                        0.14,
                        "curved gable lower sweep",
                    );
                    spawn_timber_beam(
                        world,
                        &palette.stone,
                        Vec3::new(
                            shoulder.x,
                            roof.base_height_metres + rise * 0.58,
                            shoulder.y,
                        ),
                        Vec3::new(face.x, roof.base_height_metres + rise, face.y),
                        0.14,
                        "curved gable upper sweep",
                    );
                }
            }
        }
    }
}

#[allow(dead_code)]
fn spawn_roof_dormer(
    world: &mut World,
    palette: &RenderPalette,
    mut dormer: RoofDormer,
    origin: Vec2,
    wall_style: WallStyle,
) {
    dormer.centre += origin;
    let (horizontal, inward, roof_size, ridge_axis) = match dormer.facing {
        Direction::North => (
            true,
            -Vec2::Y,
            Vec2::new(dormer.width_metres, dormer.depth_metres),
            RidgeAxis::Z,
        ),
        Direction::South => (
            true,
            Vec2::Y,
            Vec2::new(dormer.width_metres, dormer.depth_metres),
            RidgeAxis::Z,
        ),
        Direction::East => (
            false,
            -Vec2::X,
            Vec2::new(dormer.depth_metres, dormer.width_metres),
            RidgeAxis::X,
        ),
        Direction::West => (
            false,
            Vec2::X,
            Vec2::new(dormer.depth_metres, dormer.width_metres),
            RidgeAxis::X,
        ),
    };
    let scale = if dormer.kind == DormerKind::TransverseGable {
        1.55
    } else {
        1.0
    };
    dormer.width_metres *= scale;
    let facade_material = match wall_style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    let facade_centre = dormer.centre + inward * 0.18;
    spawn_wall_box_at_height(
        world,
        facade_material,
        horizontal,
        dormer.width_metres,
        dormer.height_metres,
        facade_centre,
        dormer.base_height_metres + dormer.height_metres * 0.5,
        "roof dormer facade",
    );
    let window_width = dormer.width_metres * 0.42;
    let window_height = dormer.height_metres * 0.48;
    let window_y = dormer.base_height_metres + dormer.height_metres * 0.48;
    let pane = facade_centre + inward * (WALL_THICKNESS_METRES * 0.44);
    spawn_box(
        world,
        &palette.glass,
        if horizontal {
            Vec3::new(window_width, window_height, 0.025)
        } else {
            Vec3::new(0.025, window_height, window_width)
        },
        Vec3::new(pane.x, window_y, pane.y),
        Quat::IDENTITY,
        "recessed roof dormer glazing",
    );
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let frame = facade_centre - inward * (WALL_THICKNESS_METRES * 0.56);
    for sign in [-1.0, 1.0] {
        let jamb = frame + tangent * window_width * 0.5 * sign;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(jamb.x, window_y - window_height * 0.5, jamb.y),
            Vec3::new(jamb.x, window_y + window_height * 0.5, jamb.y),
            0.065,
            "dormer window jamb",
        );
    }
    for sign in [-1.0, 1.0] {
        let y = window_y + window_height * 0.5 * sign;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                frame.x - tangent.x * window_width * 0.5,
                y,
                frame.y - tangent.y * window_width * 0.5,
            ),
            Vec3::new(
                frame.x + tangent.x * window_width * 0.5,
                y,
                frame.y + tangent.y * window_width * 0.5,
            ),
            0.065,
            "dormer window sill or lintel",
        );
    }
    let roof = RoofPiece {
        kind: match dormer.kind {
            DormerKind::Hipped => RoofKind::Hip,
            DormerKind::Shed => RoofKind::Shed,
            DormerKind::Gabled | DormerKind::TransverseGable => RoofKind::Gable,
        },
        centre: dormer.centre + inward * dormer.depth_metres * 0.42,
        size: roof_size * Vec2::new(scale, 1.0),
        base_height_metres: dormer.base_height_metres + dormer.height_metres,
        pitch_degrees: 48.0,
        ridge_axis,
        eave_metres: 0.16,
        gable_profile: dormer.gable_profile,
    };
    match roof.kind {
        RoofKind::Gable => spawn_gable_roof(world, palette, roof, wall_style),
        RoofKind::Hip => {
            let mesh = world
                .resource_mut::<Assets<Mesh>>()
                .add(roof_surface_mesh(roof));
            world.spawn((
                Name::new("hipped roof dormer"),
                Mesh3d(mesh),
                MeshMaterial3d(palette.roof_secondary.clone()),
            ));
        }
        RoofKind::Shed => spawn_shed_roof(world, &palette.roof, roof),
        RoofKind::HalfHip | RoofKind::Flat | RoofKind::Pavilion | RoofKind::Conical => {}
    }
}

#[allow(dead_code)]
fn spawn_shed_roof(world: &mut World, material: &Handle<StandardMaterial>, roof: RoofPiece) {
    let pitch = roof.pitch_degrees.to_radians();
    let (span, run) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x + roof.eave_metres * 2.0,
            roof.size.y + roof.eave_metres * 2.0,
        ),
        RidgeAxis::X => (
            roof.size.y + roof.eave_metres * 2.0,
            roof.size.x + roof.eave_metres * 2.0,
        ),
    };
    let slope = span / pitch.cos();
    let (size, rotation) = match roof.ridge_axis {
        RidgeAxis::Z => (Vec3::new(slope, 0.13, run), Quat::from_rotation_z(-pitch)),
        RidgeAxis::X => (Vec3::new(run, 0.13, slope), Quat::from_rotation_x(pitch)),
    };
    spawn_box(
        world,
        material,
        size,
        Vec3::new(
            roof.centre.x,
            roof.base_height_metres + span * pitch.tan() * 0.5,
            roof.centre.y,
        ),
        rotation,
        "shed roof",
    );
}

#[allow(dead_code)]
fn roof_surface_mesh(roof: RoofPiece) -> Mesh {
    let half_x = roof.size.x * 0.5 + roof.eave_metres;
    let half_z = roof.size.y * 0.5 + roof.eave_metres;
    let (ridge_half, rise) = match roof.ridge_axis {
        RidgeAxis::Z => {
            let inset = if roof.kind == RoofKind::HalfHip {
                half_x * 0.42
            } else if roof.kind == RoofKind::Pavilion {
                half_z
            } else {
                half_x.min(half_z * 0.85)
            };
            (
                (half_z - inset).max(0.0),
                half_x * roof.pitch_degrees.to_radians().tan(),
            )
        }
        RidgeAxis::X => {
            let inset = if roof.kind == RoofKind::HalfHip {
                half_z * 0.42
            } else if roof.kind == RoofKind::Pavilion {
                half_x
            } else {
                half_z.min(half_x * 0.85)
            };
            (
                (half_x - inset).max(0.0),
                half_z * roof.pitch_degrees.to_radians().tan(),
            )
        }
    };
    let y = roof.base_height_metres;
    let corners = [
        Vec3::new(roof.centre.x - half_x, y, roof.centre.y - half_z),
        Vec3::new(roof.centre.x + half_x, y, roof.centre.y - half_z),
        Vec3::new(roof.centre.x + half_x, y, roof.centre.y + half_z),
        Vec3::new(roof.centre.x - half_x, y, roof.centre.y + half_z),
    ];
    let (ridge_a, ridge_b) = match roof.ridge_axis {
        RidgeAxis::Z => (
            Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half),
            Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half),
        ),
        RidgeAxis::X => (
            Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y),
            Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y),
        ),
    };
    let faces = match roof.ridge_axis {
        RidgeAxis::Z => vec![
            vec![corners[0], corners[3], ridge_b, ridge_a],
            vec![corners[2], corners[1], ridge_a, ridge_b],
            vec![corners[1], corners[0], ridge_a],
            vec![corners[3], corners[2], ridge_b],
        ],
        RidgeAxis::X => vec![
            vec![corners[1], corners[0], ridge_a, ridge_b],
            vec![corners[3], corners[2], ridge_b, ridge_a],
            vec![corners[0], corners[3], ridge_a],
            vec![corners[2], corners[1], ridge_b],
        ],
    };
    flat_face_mesh(&faces)
}

fn flat_face_mesh(faces: &[Vec<Vec3>]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for face in faces {
        if face.len() < 3 {
            continue;
        }
        let normal = (face[1] - face[0])
            .cross(face[2] - face[0])
            .normalize_or_zero();
        let base = positions.len() as u32;
        positions.extend(face.iter().map(|point| point.to_array()));
        normals.extend((0..face.len()).map(|_| normal.to_array()));
        for index in 1..face.len() - 1 {
            indices.extend_from_slice(&[base, base + index as u32, base + index as u32 + 1]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn outward_flat_face_mesh(mut faces: Vec<Vec<Vec3>>) -> Mesh {
    let signed_volume_x6 = faces
        .iter()
        .filter(|face| face.len() >= 3)
        .flat_map(|face| (1..face.len() - 1).map(|index| (face[0], face[index], face[index + 1])))
        .map(|(a, b, c)| f64::from(a.dot(b.cross(c))))
        .sum::<f64>();
    if signed_volume_x6 < 0.0 {
        faces.iter_mut().for_each(|face| face.reverse());
    }
    flat_face_mesh(&faces)
}

fn arched_spandrel_mesh(
    width: f32,
    height: f32,
    depth: f32,
    rise: f32,
    pointed_arc_radius: Option<f32>,
) -> Mesh {
    let pointed = pointed_arc_radius.is_some();
    let segments = if pointed { 12 } else { 16 };
    let half_width = width * 0.5;
    let half_depth = depth * 0.5;
    let bottom = -height * 0.5;
    let top = height * 0.5;
    let curve = |x: f32| {
        if (half_width - x.abs()).abs() <= 1.0e-4 {
            return bottom;
        }
        let crown = if pointed {
            // True two-centred intrados: each half is struck from the
            // opposite spring-line centre.
            let radius = pointed_arc_radius.unwrap();
            let centre_offset = (radius - half_width).max(0.0);
            (radius * radius - (x.abs() + centre_offset).powi(2))
                .max(0.0)
                .sqrt()
        } else {
            // True segmental circular intrados from chord and rise.
            let radius = width * width / (8.0 * rise.max(0.01)) + rise * 0.5;
            (radius * radius - x * x).max(0.0).sqrt() + rise - radius
        };
        bottom + crown.min(height - 0.02).max(0.0)
    };
    let mut faces = Vec::with_capacity(segments * 3 + 3);
    for index in 0..segments {
        let x0 = -half_width + width * index as f32 / segments as f32;
        let x1 = -half_width + width * (index + 1) as f32 / segments as f32;
        let y0 = curve(x0);
        let y1 = curve(x1);
        faces.push(vec![
            Vec3::new(x0, y0, half_depth),
            Vec3::new(x1, y1, half_depth),
            Vec3::new(x1, top, half_depth),
            Vec3::new(x0, top, half_depth),
        ]);
        faces.push(vec![
            Vec3::new(x0, top, -half_depth),
            Vec3::new(x1, top, -half_depth),
            Vec3::new(x1, y1, -half_depth),
            Vec3::new(x0, y0, -half_depth),
        ]);
        faces.push(vec![
            Vec3::new(x0, y0, -half_depth),
            Vec3::new(x1, y1, -half_depth),
            Vec3::new(x1, y1, half_depth),
            Vec3::new(x0, y0, half_depth),
        ]);
        faces.push(vec![
            Vec3::new(x0, top, -half_depth),
            Vec3::new(x0, top, half_depth),
            Vec3::new(x1, top, half_depth),
            Vec3::new(x1, top, -half_depth),
        ]);
    }
    faces.push(vec![
        Vec3::new(-half_width, bottom, -half_depth),
        Vec3::new(-half_width, bottom, half_depth),
        Vec3::new(-half_width, top, half_depth),
        Vec3::new(-half_width, top, -half_depth),
    ]);
    faces.push(vec![
        Vec3::new(half_width, top, -half_depth),
        Vec3::new(half_width, top, half_depth),
        Vec3::new(half_width, bottom, half_depth),
        Vec3::new(half_width, bottom, -half_depth),
    ]);
    flat_face_mesh(&faces)
}

fn arched_panel_mesh(
    width: f32,
    height: f32,
    depth: f32,
    spring_height: f32,
    rise: f32,
    pointed_arc_radius: Option<f32>,
) -> Mesh {
    let segments = if pointed_arc_radius.is_some() { 12 } else { 16 };
    let half_width = width * 0.5;
    let half_depth = depth * 0.5;
    let bottom = -height * 0.5;
    let spring = bottom + spring_height;
    let curve = |x: f32| {
        if let Some(radius) = pointed_arc_radius {
            let centre_offset = (radius - half_width).max(0.0);
            spring
                + (radius * radius - (x.abs() + centre_offset).powi(2))
                    .max(0.0)
                    .sqrt()
        } else {
            let radius = width * width / (8.0 * rise.max(0.01)) + rise * 0.5;
            spring + (radius * radius - x * x).max(0.0).sqrt() + rise - radius
        }
    };
    let mut front = vec![
        Vec3::new(-half_width, bottom, half_depth),
        Vec3::new(half_width, bottom, half_depth),
    ];
    for index in (0..=segments).rev() {
        let x = -half_width + width * index as f32 / segments as f32;
        front.push(Vec3::new(x, curve(x), half_depth));
    }
    let mut back = front
        .iter()
        .rev()
        .map(|point| Vec3::new(point.x, point.y, -half_depth))
        .collect::<Vec<_>>();
    let mut faces = vec![front.clone(), std::mem::take(&mut back)];
    for index in 0..front.len() {
        let next = (index + 1) % front.len();
        let a = front[index];
        let b = front[next];
        faces.push(vec![
            Vec3::new(a.x, a.y, -half_depth),
            Vec3::new(b.x, b.y, -half_depth),
            b,
            a,
        ]);
    }
    flat_face_mesh(&faces)
}

fn splayed_jamb_mesh(
    width: f32,
    height: f32,
    depth: f32,
    exterior_width: f32,
    interior_width: f32,
    side: i8,
    exterior_depth_sign: i8,
) -> Mesh {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let half_depth = depth * 0.5;
    let side = if side < 0 { -1.0 } else { 1.0 };
    let exterior_z = if exterior_depth_sign < 0 {
        -half_depth
    } else {
        half_depth
    };
    let interior_z = -exterior_z;
    let retreat = ((interior_width - exterior_width) * 0.5)
        .max(0.0)
        .min(width - 0.02);
    let outer_x = side * half_width;
    let exterior_aperture_x = -side * half_width;
    let interior_aperture_x = exterior_aperture_x + side * retreat;
    let mut plan = [
        Vec2::new(outer_x, exterior_z),
        Vec2::new(exterior_aperture_x, exterior_z),
        Vec2::new(interior_aperture_x, interior_z),
        Vec2::new(outer_x, interior_z),
    ];
    let signed_area = plan
        .iter()
        .zip(plan.iter().cycle().skip(1))
        .take(plan.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>();
    if signed_area < 0.0 {
        plan.reverse();
    }
    let bottom = plan
        .iter()
        .map(|point| Vec3::new(point.x, -half_height, point.y))
        .collect::<Vec<_>>();
    let top = plan
        .iter()
        .map(|point| Vec3::new(point.x, half_height, point.y))
        .collect::<Vec<_>>();
    let mut faces = vec![bottom.clone(), top.iter().copied().rev().collect()];
    for index in 0..plan.len() {
        let next = (index + 1) % plan.len();
        faces.push(vec![bottom[next], bottom[index], top[index], top[next]]);
    }
    flat_face_mesh(&faces)
}

fn splayed_head_mesh(
    width: f32,
    height: f32,
    depth: f32,
    exterior_clear_height: f32,
    interior_clear_height: f32,
    exterior_depth_sign: i8,
) -> Mesh {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let half_depth = depth * 0.5;
    let exterior_z = if exterior_depth_sign < 0 {
        -half_depth
    } else {
        half_depth
    };
    let interior_z = -exterior_z;
    let minimum_clear = exterior_clear_height.min(interior_clear_height);
    let exterior_y = -half_height + exterior_clear_height - minimum_clear;
    let interior_y = -half_height + interior_clear_height - minimum_clear;
    let top_y = half_height;
    let faces = vec![
        vec![
            Vec3::new(-half_width, exterior_y, exterior_z),
            Vec3::new(-half_width, interior_y, interior_z),
            Vec3::new(half_width, interior_y, interior_z),
            Vec3::new(half_width, exterior_y, exterior_z),
        ],
        vec![
            Vec3::new(-half_width, top_y, exterior_z),
            Vec3::new(half_width, top_y, exterior_z),
            Vec3::new(half_width, top_y, interior_z),
            Vec3::new(-half_width, top_y, interior_z),
        ],
        vec![
            Vec3::new(-half_width, exterior_y, exterior_z),
            Vec3::new(half_width, exterior_y, exterior_z),
            Vec3::new(half_width, top_y, exterior_z),
            Vec3::new(-half_width, top_y, exterior_z),
        ],
        vec![
            Vec3::new(half_width, interior_y, interior_z),
            Vec3::new(-half_width, interior_y, interior_z),
            Vec3::new(-half_width, top_y, interior_z),
            Vec3::new(half_width, top_y, interior_z),
        ],
        vec![
            Vec3::new(-half_width, interior_y, interior_z),
            Vec3::new(-half_width, exterior_y, exterior_z),
            Vec3::new(-half_width, top_y, exterior_z),
            Vec3::new(-half_width, top_y, interior_z),
        ],
        vec![
            Vec3::new(half_width, exterior_y, exterior_z),
            Vec3::new(half_width, interior_y, interior_z),
            Vec3::new(half_width, top_y, interior_z),
            Vec3::new(half_width, top_y, exterior_z),
        ],
    ];
    let faces = if exterior_depth_sign < 0 {
        faces
            .into_iter()
            .map(|face| face.into_iter().rev().collect::<Vec<_>>())
            .collect::<Vec<_>>()
    } else {
        faces
    };
    flat_face_mesh(&faces)
}
