fn spawn_architectural_section_markers(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    view: ViewerView,
    origin: Vec2,
) {
    let annotation = if let Some(opening) = focused_opening(plan, view) {
        let wall = plan
            .wall_assemblies
            .iter()
            .find(|wall| wall.id == opening.host_wall)
            .expect("focused opening wall");
        format!(
            "wall={}  opening={}  profile={}  thickness={:.2}m  throat={:.2}m  mouth={:.2}m",
            wall.id.0,
            opening.id.0,
            opening_profile_slug(opening.profile),
            wall.thickness_metres,
            opening.profile.exterior_width_metres(),
            opening.profile.interior_width_metres(),
        )
    } else if let Some(wall) = focused_wall(plan, view) {
        format!(
            "wall={}  opening=none  profile=solid_section  thickness={:.2}m",
            wall.id.0, wall.thickness_metres
        )
    } else {
        format!(
            "wall=round_tower  opening=radial  profile=shell_section  thickness={:.2}m",
            plan.towers
                .first()
                .map_or(0.0, |tower| tower.wall_thickness_metres)
        )
    };
    world.spawn((
        Name::new("architectural section authority annotation"),
        Text::new(annotation),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.06, 0.06, 0.05)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(4.0),
            bottom: Val::Percent(4.0),
            ..default()
        },
        NonCollidingVisualization,
    ));
    let (centre, outward, tangent, thickness, base) =
        if let Some(opening) = focused_opening(plan, view) {
            let Some(wall) = plan
                .wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall)
            else {
                return;
            };
            (
                Vec2::new(
                    opening.frame.origin.x + origin.x,
                    opening.frame.origin.y + origin.y,
                ),
                opening.frame.outward,
                opening.frame.tangent,
                wall.thickness_metres,
                opening.sill_elevation_metres,
            )
        } else if let Some(wall) = focused_wall(plan, view) {
            (
                wall.frame.origin + origin,
                wall.frame.outward,
                wall.frame.tangent,
                wall.thickness_metres,
                wall.base_elevation_metres,
            )
        } else if view == ViewerView::WallRoundTowerRadialSection {
            let Some(tower) = plan.towers.first().copied() else {
                return;
            };
            (
                tower.centre_metres() + origin,
                -Vec2::Y,
                Vec2::X,
                tower.wall_thickness_metres,
                0.0,
            )
        } else {
            return;
        };
    let label_view = Vec3::new(
        tangent.x + outward.x * 0.55,
        0.22,
        tangent.y + outward.y * 0.55,
    )
    .normalize();
    let label_rotation = Quat::from_rotation_arc(Vec3::Z, label_view);
    for (label, sign) in [("OUTSIDE", 1.0_f32), ("INSIDE", -1.0_f32)] {
        let position = centre + outward * sign * (thickness * 0.5 + 0.75);
        world.spawn((
            Name::new(format!("architectural section {label} label")),
            Text2d::new(label),
            TextFont {
                font_size: FontSize::Px(44.0),
                ..default()
            },
            TextColor(Color::srgb(0.12, 0.12, 0.10)),
            Transform {
                translation: Vec3::new(position.x, base + 2.45, position.y),
                // Text2d's front faces local -Z; face the deterministic
                // oblique section camera rather than relying on a cardinal yaw.
                rotation: label_rotation,
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    let outside_left_percent = if tangent.perp_dot(outward) < 0.0 {
        70.0
    } else {
        18.0
    };
    let inside_left_percent = 88.0 - outside_left_percent;
    for (label, left_percent) in [
        ("OUTSIDE", outside_left_percent),
        ("INSIDE", inside_left_percent),
    ] {
        world.spawn((
            Name::new(format!("architectural section screen {label} label")),
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(34.0),
                ..default()
            },
            TextColor(Color::srgb(0.08, 0.08, 0.07)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left_percent),
                top: Val::Percent(8.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    let figure = centre - outward * (thickness * 0.5 + 0.75);
    spawn_box(
        world,
        &palette.timber,
        Vec3::new(0.32, 1.35, 0.22),
        Vec3::new(figure.x, base + 0.675, figure.y),
        Quat::IDENTITY,
        "architectural section 1.75m scale torso",
    );
    spawn_box(
        world,
        &palette.timber,
        Vec3::splat(0.40),
        Vec3::new(figure.x, base + 1.55, figure.y),
        Quat::IDENTITY,
        "architectural section 1.75m scale head",
    );
    let marker_size = outward.abs() * thickness + tangent.abs() * 0.055;
    spawn_box(
        world,
        &palette.roof_secondary,
        Vec3::new(marker_size.x.max(0.055), 0.055, marker_size.y.max(0.055)),
        Vec3::new(centre.x, base + 0.18, centre.y),
        Quat::IDENTITY,
        "architectural section wall thickness dimension",
    );
}

fn spawn_resolved_architectural_surfaces(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    origin: Vec2,
    visible_owners: &std::collections::HashSet<u32>,
    view: ViewerView,
) {
    let removed_reveal = focused_opening(plan, view)
        .filter(|_| section_proof(view))
        .and_then(|opening| opening.reveal_surfaces.get(1).copied());
    let timber_focus = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    for surface in &plan.resolved_geometry.surfaces {
        if removed_reveal == Some(surface.id)
            || (!visible_owners.contains(&surface.owner.0)
                && !(surface.role
                    == adventuresim_building_generator::SurfaceRole::TimberCirculation
                    && timber_focus.contains(&surface.id.0)))
            || !matches!(
                surface.role,
                adventuresim_building_generator::SurfaceRole::LeftJambReveal
                    | adventuresim_building_generator::SurfaceRole::RightJambReveal
                    | adventuresim_building_generator::SurfaceRole::WeatherSill
                    | adventuresim_building_generator::SurfaceRole::Intrados
                    | adventuresim_building_generator::SurfaceRole::ExteriorThroat
                    | adventuresim_building_generator::SurfaceRole::InteriorMouth
                    | adventuresim_building_generator::SurfaceRole::Stance
                    | adventuresim_building_generator::SurfaceRole::TimberCirculation
            )
        {
            continue;
        }
        let centre = (surface.bounds.min + surface.bounds.max) * 0.5;
        let size = (surface.bounds.max - surface.bounds.min).max(Vec3::splat(0.008));
        let opening = plan
            .opening_assemblies
            .iter()
            .find(|opening| opening.owner == surface.owner);
        let wall = opening.and_then(|opening| {
            plan.wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall)
        });
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(match (opening, wall) {
                (Some(opening), Some(wall)) => {
                    opening_surface_mesh(surface, opening, wall, centre, size)
                }
                _ => resolved_surface_plane_mesh(size),
            });
        let boundary = match surface.role {
            adventuresim_building_generator::SurfaceRole::ExteriorThroat => {
                Some(OpeningBoundaryKind::ExteriorThroat)
            }
            adventuresim_building_generator::SurfaceRole::InteriorMouth => {
                Some(OpeningBoundaryKind::InteriorMouth)
            }
            _ => None,
        };
        let mut entity = world.spawn((
            Name::new(format!(
                "resolved surface owner {} {:?}",
                surface.owner.0, surface.role
            )),
            NonCollidingVisualization,
            GeometryOwner(surface.owner.0),
            ResolvedRenderItem {
                id: surface.id.0,
                fingerprint: stable_u64(
                    &serde_json::to_vec(surface).expect("serialize rendered architectural surface"),
                ),
                local_half_size: size * 0.5,
            },
            Mesh3d(mesh),
            MeshMaterial3d(
                if matches!(
                    surface.role,
                    adventuresim_building_generator::SurfaceRole::Stance
                        | adventuresim_building_generator::SurfaceRole::TimberCirculation
                ) {
                    if view == ViewerView::TimberRegistrationCut
                        && surface.role
                            == adventuresim_building_generator::SurfaceRole::TimberCirculation
                    {
                        palette.cutaway.clone()
                    } else {
                        palette.floor.clone()
                    }
                } else {
                    palette.roof_secondary.clone()
                },
            ),
            Transform::from_translation(centre + Vec3::new(origin.x, 0.0, origin.y)),
        ));
        if let Some(kind) = boundary {
            entity.insert(OpeningBoundary(kind));
        }
    }
}

fn opening_surface_mesh(
    surface: &adventuresim_building_generator::ResolvedSurface,
    opening: &adventuresim_building_generator::OpeningAssembly,
    wall: &adventuresim_building_generator::WallAssembly,
    centre: Vec3,
    size: Vec3,
) -> Mesh {
    use adventuresim_building_generator::SurfaceRole;
    let tangent = opening.frame.tangent;
    let outward = opening.frame.outward;
    let local = |plan: Vec2, y: f32| Vec3::new(plan.x, y, plan.y) - centre;
    let two_sided = |face: Vec<Vec3>| {
        let reverse = face.iter().copied().rev().collect::<Vec<_>>();
        flat_face_mesh(&[face, reverse])
    };
    match surface.role {
        SurfaceRole::LeftJambReveal | SurfaceRole::RightJambReveal => {
            let (side, exterior_width, interior_width) = match surface.shape {
                adventuresim_building_generator::ResolvedSurfaceShape::SplayedJamb {
                    side,
                    exterior_width_metres,
                    interior_width_metres,
                    ..
                } => (
                    f32::from(side),
                    exterior_width_metres,
                    interior_width_metres,
                ),
                _ => return resolved_surface_plane_mesh(size),
            };
            let exterior = opening.frame.origin
                + tangent * (side * exterior_width * 0.5)
                + outward * (wall.thickness_metres * 0.5);
            let interior = opening.frame.origin + tangent * (side * interior_width * 0.5)
                - outward * (wall.thickness_metres * 0.5);
            let bottom = opening.sill_elevation_metres;
            let top = bottom + opening.profile.clear_height_metres();
            two_sided(vec![
                local(exterior, bottom),
                local(interior, bottom),
                local(interior, top),
                local(exterior, top),
            ])
        }
        SurfaceRole::WeatherSill => {
            let half_width = opening.profile.interior_width_metres() * 0.5;
            let (inside_y, outside_y, drip_depth) = match surface.shape {
                adventuresim_building_generator::ResolvedSurfaceShape::WeatherSill {
                    interior_elevation_metres,
                    exterior_elevation_metres,
                    drip_depth_metres,
                } => (
                    interior_elevation_metres,
                    exterior_elevation_metres,
                    drip_depth_metres,
                ),
                _ => return resolved_surface_plane_mesh(size),
            };
            two_sided(vec![
                local(
                    opening.frame.origin - tangent * half_width
                        + outward * wall.thickness_metres * 0.5,
                    outside_y,
                ),
                local(
                    opening.frame.origin
                        + tangent * half_width
                        + outward * wall.thickness_metres * 0.5,
                    outside_y,
                ),
                local(
                    opening.frame.origin + tangent * half_width
                        - outward * wall.thickness_metres * 0.5,
                    inside_y,
                ),
                local(
                    opening.frame.origin
                        - tangent * half_width
                        - outward * wall.thickness_metres * 0.5,
                    inside_y,
                ),
                local(
                    opening.frame.origin - tangent * half_width
                        + outward * (wall.thickness_metres * 0.5 + drip_depth),
                    outside_y - drip_depth,
                ),
            ])
        }
        SurfaceRole::Intrados => {
            let segments = 16;
            let width = opening.profile.interior_width_metres();
            let half_width = width * 0.5;
            let sill = opening.sill_elevation_metres;
            let height_at = |along: f32| match opening.profile {
                adventuresim_building_generator::OpeningProfile::Segmental {
                    spring_height_metres,
                    rise_metres,
                    ..
                } => {
                    let radius = width * width / (8.0 * rise_metres.max(0.01)) + rise_metres * 0.5;
                    sill + spring_height_metres
                        + (radius * radius - along * along).max(0.0).sqrt()
                        + rise_metres
                        - radius
                }
                adventuresim_building_generator::OpeningProfile::PointedTwoCentred {
                    spring_height_metres,
                    arc_radius_metres,
                    ..
                } => {
                    let offset = (arc_radius_metres - half_width).max(0.0);
                    sill + spring_height_metres
                        + (arc_radius_metres * arc_radius_metres - (along.abs() + offset).powi(2))
                            .max(0.0)
                            .sqrt()
                }
                _ => sill + opening.profile.clear_height_metres(),
            };
            let mut faces = Vec::new();
            for index in 0..segments {
                let a = -half_width + width * index as f32 / segments as f32;
                let b = -half_width + width * (index + 1) as f32 / segments as f32;
                let outside_a =
                    opening.frame.origin + tangent * a + outward * wall.thickness_metres * 0.5;
                let outside_b =
                    opening.frame.origin + tangent * b + outward * wall.thickness_metres * 0.5;
                let inside_a =
                    opening.frame.origin + tangent * a - outward * wall.thickness_metres * 0.5;
                let inside_b =
                    opening.frame.origin + tangent * b - outward * wall.thickness_metres * 0.5;
                faces.push(vec![
                    local(outside_a, height_at(a)),
                    local(outside_b, height_at(b)),
                    local(inside_b, height_at(b)),
                    local(inside_a, height_at(a)),
                ]);
            }
            flat_face_mesh(&faces)
        }
        SurfaceRole::ExteriorThroat | SurfaceRole::InteriorMouth => {
            opening_boundary_outline_mesh(surface.role, opening, wall, centre)
        }
        _ => resolved_surface_plane_mesh(size),
    }
}

fn opening_boundary_outline_mesh(
    role: adventuresim_building_generator::SurfaceRole,
    opening: &adventuresim_building_generator::OpeningAssembly,
    wall: &adventuresim_building_generator::WallAssembly,
    centre: Vec3,
) -> Mesh {
    use adventuresim_building_generator::{OpeningProfile, SurfaceRole};

    let exterior = role == SurfaceRole::ExteriorThroat;
    let width = if exterior {
        opening.profile.exterior_width_metres()
    } else {
        opening.profile.interior_width_metres()
    };
    let height = match opening.profile {
        OpeningProfile::ArrowLoop {
            exterior_height_metres,
            interior_height_metres,
            ..
        }
        | OpeningProfile::GunLoop {
            exterior_height_metres,
            interior_height_metres,
            ..
        } => {
            if exterior {
                exterior_height_metres
            } else {
                interior_height_metres
            }
        }
        _ => opening.profile.clear_height_metres(),
    };
    let depth = if exterior {
        wall.thickness_metres * 0.5
    } else {
        -wall.thickness_metres * 0.5
    };
    let tangent = opening.frame.tangent;
    let outward = opening.frame.outward;
    let sill = opening.sill_elevation_metres;
    let border = 0.018_f32.min(width * 0.12).min(height * 0.06);
    let half_width = width * 0.5;
    let point = |along: f32, elevation: f32| {
        let plan = opening.frame.origin + tangent * along + outward * depth;
        Vec3::new(plan.x, elevation, plan.y) - centre
    };
    let top_at = |along: f32| match opening.profile {
        OpeningProfile::Segmental {
            spring_height_metres,
            rise_metres,
            ..
        } => {
            let radius = width * width / (8.0 * rise_metres.max(0.01)) + rise_metres * 0.5;
            sill + spring_height_metres
                + (radius * radius - along * along).max(0.0).sqrt()
                + rise_metres
                - radius
        }
        OpeningProfile::PointedTwoCentred {
            spring_height_metres,
            arc_radius_metres,
            ..
        } => {
            let offset = (arc_radius_metres - half_width).max(0.0);
            sill + spring_height_metres
                + (arc_radius_metres * arc_radius_metres - (along.abs() + offset).powi(2))
                    .max(0.0)
                    .sqrt()
        }
        _ => sill + height,
    };

    let mut faces = vec![
        vec![
            point(-half_width, sill),
            point(half_width, sill),
            point(half_width, sill + border),
            point(-half_width, sill + border),
        ],
        vec![
            point(-half_width, sill),
            point(-half_width + border, sill),
            point(-half_width + border, top_at(-half_width)),
            point(-half_width, top_at(-half_width)),
        ],
        vec![
            point(half_width - border, sill),
            point(half_width, sill),
            point(half_width, top_at(half_width)),
            point(half_width - border, top_at(half_width)),
        ],
    ];
    let segments = if matches!(
        opening.profile,
        OpeningProfile::Segmental { .. } | OpeningProfile::PointedTwoCentred { .. }
    ) {
        16
    } else {
        1
    };
    for index in 0..segments {
        let a = -half_width + width * index as f32 / segments as f32;
        let b = -half_width + width * (index + 1) as f32 / segments as f32;
        faces.push(vec![
            point(a, top_at(a)),
            point(b, top_at(b)),
            point(b, top_at(b) + border),
            point(a, top_at(a) + border),
        ]);
    }
    let reverse = faces
        .iter()
        .map(|face| face.iter().copied().rev().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    faces.extend(reverse);
    flat_face_mesh(&faces)
}

fn resolved_surface_plane_mesh(size: Vec3) -> Mesh {
    let half = size * 0.5;
    let face = if size.x <= size.y && size.x <= size.z {
        vec![
            Vec3::new(0.0, -half.y, -half.z),
            Vec3::new(0.0, half.y, -half.z),
            Vec3::new(0.0, half.y, half.z),
            Vec3::new(0.0, -half.y, half.z),
        ]
    } else if size.y <= size.z {
        vec![
            Vec3::new(-half.x, 0.0, -half.z),
            Vec3::new(-half.x, 0.0, half.z),
            Vec3::new(half.x, 0.0, half.z),
            Vec3::new(half.x, 0.0, -half.z),
        ]
    } else {
        vec![
            Vec3::new(-half.x, -half.y, 0.0),
            Vec3::new(half.x, -half.y, 0.0),
            Vec3::new(half.x, half.y, 0.0),
            Vec3::new(-half.x, half.y, 0.0),
        ]
    };
    let reverse = face.iter().copied().rev().collect::<Vec<_>>();
    flat_face_mesh(&[face, reverse])
}

fn timber_panel_prism_mesh(
    vertices: [Vec3; 3],
    outward: Vec2,
    depth_metres: f32,
    centre: Vec3,
) -> Mesh {
    let offset = Vec3::new(outward.x, 0.0, outward.y) * depth_metres * 0.5;
    let outward_3d = Vec3::new(outward.x, 0.0, outward.y);
    let mut oriented = vertices;
    if (oriented[1] - oriented[0])
        .cross(oriented[2] - oriented[0])
        .dot(outward_3d)
        < 0.0
    {
        oriented.swap(1, 2);
    }
    let front = oriented.map(|vertex| vertex + offset - centre);
    let back = oriented.map(|vertex| vertex - offset - centre);
    let mut faces = vec![front.to_vec(), vec![back[0], back[2], back[1]]];
    for index in 0..3 {
        let next = (index + 1) % 3;
        faces.push(vec![front[index], back[index], back[next], front[next]]);
    }
    flat_face_mesh(&faces)
}
