fn direction_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
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

fn walk_junction_centre(a: WallWalk, b: WallWalk) -> Option<Vec2> {
    match (a, b) {
        (
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
            WallWalk::Round {
                centre,
                outer_radius_metres,
                ..
            },
        )
        | (
            WallWalk::Round {
                centre,
                outer_radius_metres,
                ..
            },
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
        ) => {
            let delta = end - start;
            let t = if delta.length_squared() < 0.001 {
                0.0
            } else {
                ((centre - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0)
            };
            let nearest = start + delta * t;
            ((nearest - centre).length() <= outer_radius_metres + width_metres * 0.5)
                .then_some(nearest)
        }
        (
            WallWalk::Linear {
                start: a0,
                end: a1,
                width_metres: aw,
                ..
            },
            WallWalk::Linear {
                start: b0,
                end: b1,
                width_metres: bw,
                ..
            },
        ) => [a0, a1]
            .into_iter()
            .find(|point| distance_to_segment(*point, b0, b1) <= (aw + bw) * 0.5),
        (
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
            WallWalk::RectangularDeck { centre, size, .. },
        )
        | (
            WallWalk::RectangularDeck { centre, size, .. },
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
        ) => {
            let half = size * 0.5 + Vec2::splat(width_metres * 0.5);
            [start, end].into_iter().find(|point| {
                point.x >= centre.x - half.x
                    && point.x <= centre.x + half.x
                    && point.y >= centre.y - half.y
                    && point.y <= centre.y + half.y
            })
        }
        _ => None,
    }
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let delta = end - start;
    if delta.length_squared() < 0.001 {
        return (point - start).length();
    }
    let t = ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
    (point - (start + delta * t)).length()
}

fn derive_curtain_walls(program: &BuildingProgram) -> Vec<CurtainWallRun> {
    if program.archetype != BuildingArchetype::WalledKeep {
        return Vec::new();
    }
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let margin = 9.0;
    let min = Vec2::splat(-margin);
    let max = Vec2::new(width + margin, depth + margin);
    let wall = |start, end, outward, gate_width_metres| CurtainWallRun {
        start,
        end,
        height_metres: 6.0,
        // 1.2 m is an inferred prototype minimum for a deliberately practical
        // early-artillery profile, not a universal historical threshold.
        thickness_metres: 1.2,
        outward,
        gate_width_metres,
        gate_height_metres: 3.6,
    };
    vec![
        wall(min, Vec2::new(max.x, min.y), Direction::South, Some(3.2)),
        wall(Vec2::new(max.x, min.y), max, Direction::East, None),
        wall(Vec2::new(min.x, max.y), max, Direction::North, None),
        wall(min, Vec2::new(min.x, max.y), Direction::West, None),
    ]
}

fn derive_bartizans(program: &BuildingProgram) -> Vec<Bartizan> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match program.archetype {
        BuildingArchetype::CastleGatehouse if program.seed % 1_000 == 203 => vec![
            Bartizan {
                // Keep the unroofed bartizan on its own grounded buttress bay,
                // beyond the resolved south gate-tower radius. It remains a
                // localized threatened-face work instead of overlapping the
                // newly authoritative radial tower shell.
                centre: Vec2::new(width + 0.4, depth * 0.44),
                base_height_metres: top,
                radius_metres: 0.85,
                height_metres: 2.0,
                roofed: false,
            },
            Bartizan {
                centre: Vec2::new(width + 0.4, depth * 0.8),
                base_height_metres: top,
                radius_metres: 0.85,
                height_metres: 2.0,
                roofed: true,
            },
        ],
        BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle => Vec::new(),
        _ => Vec::new(),
    }
}

fn projected_solid(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    centre: Vec3,
    size: Vec3,
    yaw_radians: f32,
    role: SolidRole,
    supported_by: Vec<StructuralNodeId>,
) -> ResolvedItemId {
    let index = geometry.solids.len();
    let id = ResolvedItemId((1_u64 << 60) | (u64::from(owner.0) << 32) | index as u64);
    let solid = ResolvedSolid {
        id,
        owner,
        centre,
        size,
        yaw_radians,
        crossfall_radians: 0.0,
        longfall_radians: 0.0,
        role,
        shape: crate::ResolvedSolidShape::Cuboid,
        supported_by: supported_by.clone(),
    };
    let bottom = centre.y - size.y * 0.5;
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | index as u64),
        owner,
        node: supported_by[0],
        bounds: ResolvedBounds {
            min: Vec3::new(
                centre.x - size.x * 0.5,
                bottom - 0.015,
                centre.z - size.z * 0.5,
            ),
            max: Vec3::new(
                centre.x + size.x * 0.5,
                bottom + 0.015,
                centre.z + size.z * 0.5,
            ),
        },
    });
    geometry.solids.push(solid);
    id
}

fn projected_void(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    bounds: ResolvedBounds,
    role: VoidRole,
) -> ResolvedItemId {
    let index = geometry.voids.len();
    let id = ResolvedItemId((3_u64 << 60) | (u64::from(owner.0) << 32) | index as u64);
    geometry.voids.push(ResolvedVoid {
        id,
        owner,
        bounds,
        role,
        shape: crate::ResolvedVoidShape::Box,
        subtracts_from: owner,
    });
    id
}

fn projected_surface(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    bounds: ResolvedBounds,
    role: SurfaceRole,
) -> ResolvedItemId {
    let index = geometry.surfaces.len();
    let id = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | index as u64);
    geometry.surfaces.push(ResolvedSurface {
        id,
        owner,
        bounds,
        role,
        shape: crate::ResolvedSurfaceShape::Planar,
    });
    id
}

fn projected_edge_drain(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    inlet: Vec3,
    direction: Vec2,
) -> ResolvedItemId {
    let far = inlet + Vec3::new(direction.x * 0.12, -0.02, direction.y * 0.12);
    let lateral = Vec3::new(direction.y.abs() * 0.01, 0.0, direction.x.abs() * 0.01);
    let outlet_void = projected_void(
        geometry,
        owner,
        ResolvedBounds {
            min: inlet.min(far) - lateral - Vec3::Y * 0.045,
            max: inlet.max(far) + lateral + Vec3::Y * 0.045,
        },
        VoidRole::Drain,
    );
    let id = ResolvedItemId((5_u64 << 60) | geometry.drainage_routes.len() as u64);
    geometry.drainage_routes.push(DrainageRoute {
        id,
        owner,
        outlet_void,
        inlet,
        outlet: far + Vec3::new(direction.x * 0.25, -0.08, direction.y * 0.25),
    });
    id
}

/// Resolves a mono-pitch defense roof into a physical catchment, a lowered
/// eave channel and a named drip outlet. The high host edge receives separate
/// flashing so no valley can be trapped between roof and source masonry.
fn resolve_linear_roof_weathering(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    roof_id: ResolvedItemId,
    midpoint: Vec2,
    tangent: Vec2,
    outward: Vec2,
    length: f32,
    depth: f32,
    yaw: f32,
    support: StructuralNodeId,
) -> (ResolvedItemId, Vec<ResolvedItemId>) {
    let roof = geometry
        .solids
        .iter()
        .find(|solid| solid.id == roof_id)
        .expect("roof catchment solid")
        .clone();
    let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
    let crossfall = 0.12 * outward.dot(local_positive_z).signum();
    geometry
        .solids
        .iter_mut()
        .find(|solid| solid.id == roof_id)
        .expect("roof catchment solid")
        .crossfall_radians = crossfall;
    let inner_edge = midpoint - outward * depth * 0.5;
    let flashing = projected_solid(
        geometry,
        owner,
        Vec3::new(
            inner_edge.x - outward.x * 0.035,
            roof.centre.y + 0.13,
            inner_edge.y - outward.y * 0.035,
        ),
        Vec3::new(length + 0.18, 0.26, 0.08),
        yaw,
        SolidRole::RoofFlashing,
        vec![support],
    );
    let roof_half_drop = crossfall.abs().tan() * depth * 0.5;
    let toe_elevation = roof.centre.y - roof_half_drop;
    let channel_length = length + 0.24;
    let channel_centre_plan = midpoint + outward * (depth * 0.5 + 0.06) - tangent * 0.055;
    let channel = projected_solid(
        geometry,
        owner,
        Vec3::new(
            channel_centre_plan.x,
            toe_elevation - 0.045,
            channel_centre_plan.y,
        ),
        Vec3::new(channel_length - 0.11, 0.06, 0.12),
        yaw,
        SolidRole::DrainageFloor,
        vec![support],
    );
    geometry
        .solids
        .iter_mut()
        .find(|solid| solid.id == channel)
        .expect("roof eave channel")
        .longfall_radians = -0.018;
    let inlet_plan = channel_centre_plan + tangent * ((channel_length - 0.11) * 0.5 + 0.018);
    let route = projected_edge_drain(
        geometry,
        owner,
        Vec3::new(inlet_plan.x, toe_elevation - 0.015, inlet_plan.y),
        outward,
    );
    let surface = projected_surface(
        geometry,
        owner,
        ResolvedBounds {
            min: roof.centre - roof.size * 0.5,
            max: roof.centre + roof.size * 0.5,
        },
        SurfaceRole::Drainage,
    );
    let catchment = ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
    geometry.drainage_catchments.push(DrainageCatchment {
        id: catchment,
        owner,
        walk_solid: roof_id,
        toe_channel_solids: vec![channel],
        drainage_surface: surface,
        outlet_route: route,
        centre: roof.centre,
        tangent,
        outward,
        length_metres: length,
        width_metres: depth,
        inner_elevation_metres: roof.centre.y + roof_half_drop,
        outer_elevation_metres: toe_elevation,
        outlet_along_metres: (channel_length - 0.11) * 0.5,
    });
    (catchment, vec![roof_id, channel, flashing])
}

fn resolve_linear_coping_weathering(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    length: f32,
    elevation: f32,
    yaw: f32,
    support: StructuralNodeId,
) -> (ResolvedItemId, Vec<ResolvedItemId>) {
    let coping = projected_solid(
        geometry,
        owner,
        Vec3::new(
            centre.x + outward.x * 0.035,
            elevation,
            centre.y + outward.y * 0.035,
        ),
        Vec3::new(length + 0.12, 0.12, 0.32),
        yaw,
        SolidRole::Coping,
        vec![support],
    );
    let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
    let crossfall = 0.07 * outward.dot(local_positive_z).signum();
    geometry
        .solids
        .iter_mut()
        .find(|solid| solid.id == coping)
        .expect("projected coping")
        .crossfall_radians = crossfall;
    let toe = elevation - 0.06 - crossfall.abs().tan() * 0.16;
    let inlet_plan = centre + tangent * (length * 0.5 - 0.06) + outward * 0.2;
    let route = projected_edge_drain(
        geometry,
        owner,
        Vec3::new(inlet_plan.x, toe - 0.06, inlet_plan.y),
        outward,
    );
    let surface = projected_surface(
        geometry,
        owner,
        ResolvedBounds {
            min: Vec3::new(centre.x, toe, centre.y)
                - Vec3::new(
                    tangent.x.abs() * length * 0.5 + outward.x.abs() * 0.16,
                    0.0,
                    tangent.y.abs() * length * 0.5 + outward.y.abs() * 0.16,
                ),
            max: Vec3::new(centre.x, elevation + 0.06, centre.y)
                + Vec3::new(
                    tangent.x.abs() * length * 0.5 + outward.x.abs() * 0.16,
                    0.0,
                    tangent.y.abs() * length * 0.5 + outward.y.abs() * 0.16,
                ),
        },
        SurfaceRole::Drainage,
    );
    let catchment = ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
    geometry.drainage_catchments.push(DrainageCatchment {
        id: catchment,
        owner,
        walk_solid: coping,
        toe_channel_solids: Vec::new(),
        drainage_surface: surface,
        outlet_route: route,
        centre: Vec3::new(centre.x, elevation, centre.y),
        tangent,
        outward,
        length_metres: length,
        width_metres: 0.32,
        inner_elevation_metres: elevation + crossfall.abs().tan() * 0.16,
        outer_elevation_metres: toe,
        outlet_along_metres: length * 0.5 - 0.06,
    });
    (catchment, vec![coping])
}
