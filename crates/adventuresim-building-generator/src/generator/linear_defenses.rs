struct LinearDefenseHost {
    owner: GeometryOwnerId,
    bearing: StructuralNodeId,
    walls: Vec<ResolvedItemId>,
    buttresses: Vec<ResolvedItemId>,
    sources: Vec<ProjectedDefenseHostWallSource>,
    top_elevation_metres: f32,
    topology: ProjectedDefenseHostTopology,
    walk: ResolvedItemId,
    portal: Option<ResolvedItemId>,
    sockets: Vec<ResolvedItemId>,
}

/// Resolves the masonry that a projected defense actually cuts and bears on.
/// Dimensions here are project gates for coarse traversal/rendering, not a
/// claim that every historical curtain used this exact section.
fn resolve_linear_defense_host(
    geometry: &mut ResolvedGeometry,
    storeys: &[StoreyPlan],
    source_index: usize,
    run: BattlementRun,
    socket_count: Option<usize>,
    needs_portal: bool,
) -> LinearDefenseHost {
    let owner = GeometryOwnerId(10_000 + source_index as u32);
    let bearing = StructuralNodeId(900_000 + source_index as u64 * 100);
    let tangent = (run.end - run.start).normalize_or_zero();
    let outward = direction_vector(run.outward);
    let midpoint = (run.start + run.end) * 0.5;
    let length = run.start.distance(run.end);
    let yaw = -tangent.y.atan2(tangent.x);
    geometry.structural_nodes.push(StructuralNode {
        id: bearing,
        owner,
        kind: StructuralNodeKind::WallBearing,
        position: Vec3::new(midpoint.x, 0.0, midpoint.y),
        supported_by: Vec::new(),
        grounded: true,
    });
    let top_storey = storeys.last().expect("projected defense host storey");
    let wall_top = f32::from(top_storey.level + 1)
        * (run.base_height_metres / f32::from(top_storey.level + 1));
    let wall_bottom = wall_top - run.base_height_metres / f32::from(top_storey.level + 1);
    let wall_depth = 0.18;
    let source_walls = top_storey
        .walls
        .iter()
        .enumerate()
        .filter(|(_, wall)| {
            if !wall.exterior() || wall.direction != run.outward {
                return false;
            }
            let offset = wall.centre() - midpoint;
            offset.dot(outward).abs() <= 0.12
                && offset.dot(tangent).abs() <= length * 0.5 + CELL_SIZE_METRES * 0.51
        })
        .map(|(wall_index, wall)| (wall_index, *wall))
        .collect::<Vec<_>>();
    assert!(
        !source_walls.is_empty(),
        "projected defense must bind real source wall cells"
    );

    let mut cuts = Vec::<(f32, f32, f32, f32, VoidRole)>::new();
    let mut portal = None;
    if needs_portal {
        let width = 0.9;
        let bottom = wall_top - 0.14;
        let top = wall_top + 2.0;
        cuts.push((
            -width * 0.5,
            width * 0.5,
            bottom,
            top,
            VoidRole::AccessPortal,
        ));
    }
    if let Some(count) = socket_count {
        let bay = length / count as f32;
        for index in 0..count {
            let centre = -length * 0.5 + (index as f32 + 0.5) * bay;
            cuts.push((
                centre - 0.09,
                centre + 0.09,
                wall_top - 0.52,
                wall_top - 0.28,
                VoidRole::BeamSocket,
            ));
        }
    }
    let source_average = source_walls
        .iter()
        .map(|(_, wall)| wall.centre())
        .fold(Vec2::ZERO, |sum, centre| sum + centre)
        / source_walls.len() as f32;
    let host_line = midpoint + outward * (source_average - midpoint).dot(outward);
    for (from, to, bottom, top, role) in &cuts {
        let centre = host_line + tangent * ((*from + *to) * 0.5);
        let id = projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    centre.x
                        - tangent.x.abs() * (*to - *from) * 0.5
                        - outward.x.abs() * wall_depth * 0.5,
                    *bottom,
                    centre.y
                        - tangent.y.abs() * (*to - *from) * 0.5
                        - outward.y.abs() * wall_depth * 0.5,
                ),
                max: Vec3::new(
                    centre.x
                        + tangent.x.abs() * (*to - *from) * 0.5
                        + outward.x.abs() * wall_depth * 0.5,
                    *top,
                    centre.y
                        + tangent.y.abs() * (*to - *from) * 0.5
                        + outward.y.abs() * wall_depth * 0.5,
                ),
            },
            *role,
        );
        match role {
            VoidRole::AccessPortal => portal = Some(id),
            VoidRole::BeamSocket => {}
            _ => unreachable!(),
        }
    }
    let sockets = geometry
        .voids
        .iter()
        .filter(|void| void.owner == owner && void.role == VoidRole::BeamSocket)
        .map(|void| void.id)
        .collect::<Vec<_>>();
    let mut walls = Vec::new();
    for (_, wall) in &source_walls {
        let along_centre = (wall.centre() - midpoint).dot(tangent);
        let segment_min = (along_centre - CELL_SIZE_METRES * 0.5).max(-length * 0.5);
        let segment_max = (along_centre + CELL_SIZE_METRES * 0.5).min(length * 0.5);
        let mut along_cuts = vec![segment_min, segment_max];
        let mut height_cuts = vec![wall_bottom, wall_top];
        for (from, to, bottom, top, _) in &cuts {
            if *to > segment_min && *from < segment_max {
                along_cuts.extend([from.max(segment_min), to.min(segment_max)]);
                height_cuts.extend([bottom.max(wall_bottom), top.min(wall_top)]);
            }
        }
        along_cuts.sort_by(f32::total_cmp);
        along_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        height_cuts.sort_by(f32::total_cmp);
        height_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        for along in along_cuts.windows(2) {
            for height in height_cuts.windows(2) {
                let ac = (along[0] + along[1]) * 0.5;
                let hc = (height[0] + height[1]) * 0.5;
                if cuts.iter().any(|(from, to, bottom, top, _)| {
                    ac > *from && ac < *to && hc > *bottom && hc < *top
                }) {
                    continue;
                }
                let centre = midpoint + tangent * ac;
                walls.push(projected_solid(
                    geometry,
                    owner,
                    Vec3::new(centre.x, hc, centre.y),
                    Vec3::new(along[1] - along[0], height[1] - height[0], wall_depth),
                    yaw,
                    SolidRole::DefenseHostWall,
                    vec![bearing],
                ));
            }
        }
    }
    let walk_centre = midpoint - outward * 0.84;
    let walk = projected_solid(
        geometry,
        owner,
        Vec3::new(walk_centre.x, run.base_height_metres - 0.07, walk_centre.y),
        Vec3::new(length, 0.14, 1.0),
        yaw,
        SolidRole::CircuitWalk,
        vec![bearing],
    );
    LinearDefenseHost {
        owner,
        bearing,
        walls,
        buttresses: Vec::new(),
        sources: source_walls
            .into_iter()
            .map(|(wall_index, _)| ProjectedDefenseHostWallSource {
                storey_level: top_storey.level,
                wall_index,
            })
            .collect(),
        top_elevation_metres: wall_top,
        topology: ProjectedDefenseHostTopology::LinearFace,
        walk,
        portal,
        sockets,
    }
}
