fn resolved_solid_contains_point(solid: &ResolvedSolid, point: Vec3, tolerance: f32) -> bool {
    let relative = point - solid.centre;
    let (sine, cosine) = solid.yaw_radians.sin_cos();
    let local = Vec3::new(
        relative.x * cosine - relative.z * sine,
        relative.y,
        relative.x * sine + relative.z * cosine,
    );
    let half = solid.size * 0.5 + Vec3::splat(tolerance);
    local.abs().cmple(half).all()
}

fn linear_walk_bounds_for_geometry(walk: WallWalk) -> ResolvedBounds {
    let WallWalk::Linear {
        start,
        end,
        elevation_metres,
        width_metres,
        outward,
    } = walk
    else {
        unreachable!()
    };
    let inward = -direction_vector(outward) * width_metres;
    let min = start.min(end).min(start + inward).min(end + inward);
    let max = start.max(end).max(start + inward).max(end + inward);
    ResolvedBounds {
        min: Vec3::new(min.x, elevation_metres - 0.08, min.y),
        max: Vec3::new(max.x, elevation_metres, max.y),
    }
}

fn derive_wall_walks(
    program: &BuildingProgram,
    battlements: &[BattlementRun],
    towers: &[RoundTower],
) -> Vec<WallWalk> {
    let mut walks = battlements
        .iter()
        .filter(|run| run.kind != BattlementKind::Breteche)
        .map(|run| WallWalk::Linear {
            start: run.start,
            end: run.end,
            elevation_metres: run.base_height_metres,
            width_metres: 1.25,
            outward: run.outward,
        })
        .chain(
            towers
                .iter()
                .filter(|tower| tower.battlement.is_some())
                .map(|tower| WallWalk::Round {
                    centre: tower.centre_metres(),
                    elevation_metres: tower.wall_height_metres,
                    outer_radius_metres: tower.radius_metres() - 0.08,
                    stairwell_radius_metres: 0.62,
                }),
        )
        .collect::<Vec<_>>();
    if program.archetype == BuildingArchetype::WalledKeep {
        let (width, depth) = program.footprint.dimensions();
        let size = Vec2::new(
            f32::from(width) * CELL_SIZE_METRES,
            f32::from(depth) * CELL_SIZE_METRES,
        );
        walks.push(WallWalk::RectangularDeck {
            centre: size * 0.5,
            size,
            elevation_metres: program.storeys.len() as f32 * program.storey_height_metres,
            stairwell_centre: size * 0.5,
            stairwell_size: Vec2::splat(1.6),
        });
    }
    walks
}

fn derive_defensive_junctions(walks: &[WallWalk]) -> Vec<DefensiveJunction> {
    let mut junctions = Vec::new();
    for walk_a in 0..walks.len() {
        for walk_b in (walk_a + 1)..walks.len() {
            let Some(centre) = walk_junction_centre(walks[walk_a], walks[walk_b]) else {
                continue;
            };
            let elevation_delta =
                (walk_elevation(walks[walk_a]) - walk_elevation(walks[walk_b])).abs();
            let kind = if elevation_delta <= 0.2 {
                DefensiveJunctionKind::LevelLanding
            } else {
                DefensiveJunctionKind::Steps {
                    riser_count: (elevation_delta / 0.18).ceil() as u8,
                }
            };
            junctions.push(DefensiveJunction {
                walk_a,
                walk_b,
                centre,
                width_metres: 1.0,
                clear_height_metres: 2.1,
                kind,
            });
        }
    }
    junctions
}

fn derive_defensive_circuits(
    program: &BuildingProgram,
    walks: &[WallWalk],
) -> Vec<DefensiveCircuit> {
    if walks.is_empty() {
        return Vec::new();
    }
    if program.archetype != BuildingArchetype::WalledKeep {
        return vec![DefensiveCircuit {
            label: "main fighting circuit".to_owned(),
            walks: (0..walks.len()).collect(),
        }];
    }
    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let is_outer = |walk: WallWalk| match walk {
        WallWalk::Linear { start, end, .. } => {
            start.x < -0.01
                || start.y < -0.01
                || end.x > dimensions.x + 0.01
                || end.y > dimensions.y + 0.01
        }
        WallWalk::Round { centre, .. } => {
            centre.x < -0.01
                || centre.y < -0.01
                || centre.x > dimensions.x + 0.01
                || centre.y > dimensions.y + 0.01
        }
        WallWalk::RectangularDeck { .. } => false,
    };
    let (outer, inner): (Vec<_>, Vec<_>) = walks
        .iter()
        .copied()
        .enumerate()
        .partition(|(_, walk)| is_outer(*walk));
    vec![
        DefensiveCircuit {
            label: "outer curtain circuit".to_owned(),
            walks: outer.into_iter().map(|(index, _)| index).collect(),
        },
        DefensiveCircuit {
            label: "inner keep circuit".to_owned(),
            walks: inner.into_iter().map(|(index, _)| index).collect(),
        },
    ]
}

fn derive_tower_portals(
    program: &BuildingProgram,
    towers: &[RoundTower],
    walks: &[WallWalk],
    junctions: &[DefensiveJunction],
) -> Vec<TowerPortal> {
    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let protected_centre = dimensions * 0.5;
    let mut portals = towers
        .iter()
        .enumerate()
        .map(|(tower_index, tower)| TowerPortal {
            tower_index,
            facing: cardinal_direction(protected_centre - tower.centre_metres()),
            sill_elevation_metres: 0.0,
            width_metres: 1.05,
            clear_height_metres: 2.15,
            kind: TowerPortalKind::GroundStairEntrance,
        })
        .collect::<Vec<_>>();
    for junction in junctions {
        let pair = [junction.walk_a, junction.walk_b];
        let Some(&linear_index) = pair
            .iter()
            .find(|&&index| matches!(walks.get(index), Some(WallWalk::Linear { .. })))
        else {
            continue;
        };
        let Some((round_index, tower_centre, elevation)) = pair.iter().find_map(|&index| {
            let WallWalk::Round {
                centre,
                elevation_metres,
                ..
            } = *walks.get(index)?
            else {
                return None;
            };
            Some((index, centre, elevation_metres))
        }) else {
            continue;
        };
        let Some(tower_index) = towers
            .iter()
            .position(|tower| (tower.centre_metres() - tower_centre).length_squared() < 0.001)
        else {
            continue;
        };
        let WallWalk::Linear { start, end, .. } = walks[linear_index] else {
            unreachable!()
        };
        let along =
            if (start - tower_centre).length_squared() < (end - tower_centre).length_squared() {
                end - start
            } else {
                start - end
            };
        portals.push(TowerPortal {
            tower_index,
            facing: cardinal_direction(along),
            sill_elevation_metres: elevation - 0.2,
            width_metres: junction.width_metres,
            clear_height_metres: junction.clear_height_metres,
            kind: TowerPortalKind::WallWalkJunction {
                walk_index: linear_index,
            },
        });
        let _ = round_index;
    }
    portals
}

fn cardinal_direction(vector: Vec2) -> Direction {
    if vector.x.abs() >= vector.y.abs() {
        if vector.x >= 0.0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if vector.y >= 0.0 {
        Direction::North
    } else {
        Direction::South
    }
}
