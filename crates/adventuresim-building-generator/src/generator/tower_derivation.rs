fn derive_towers(
    program: &BuildingProgram,
    gatehouses: &[GatehouseAssemblySpec],
    curtain_walls: &[CurtainWallRun],
) -> Vec<RoundTower> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let wall_height = program.storeys.len() as f32 * program.storey_height_metres;
    let tower = |centre: Vec2, battlement, roofed: bool| {
        RoundTower::new(
            grid_point(centre),
            CellDiameter::new(4).expect("four-cell tower diameter is valid"),
            wall_height,
            1.2,
            roofed.then_some(RoofPiece {
                kind: RoofKind::Conical,
                centre,
                size: Vec2::splat(4.7),
                base_height_metres: wall_height + 1.75,
                pitch_degrees: 58.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.15,
                gable_profile: GableProfile::Plain,
            }),
            battlement,
        )
        .expect("curated tower anchor matches its integral-cell footprint")
    };
    if program.roof_demonstrator == Some(RoofKind::Conical) {
        // A deterministic isolated kernel proof: a grounded round stair tower
        // alongside the civilian fixture, without changing any curated
        // archetype when the demonstrator is unset.
        return vec![tower(Vec2::new(width + 3.0, depth * 0.5), None, true)];
    }
    match program.archetype {
        BuildingArchetype::CastleGatehouse => vec![
            tower(
                Vec2::new(0.0, 0.0),
                Some(BattlementKind::Machicolated),
                false,
            )
            .with_chord_interface(TowerChordInterface {
                toward_gate: Direction::East,
                bearing_depth: GridLength::new(24).expect("1.2 m tower chord"),
            })
            .with_secondary_chord_interface(TowerChordInterface {
                toward_gate: Direction::North,
                bearing_depth: GridLength::new(24).expect("1.2 m tower return chord"),
            }),
            tower(
                Vec2::new(width, 0.0),
                Some(BattlementKind::Machicolated),
                false,
            )
            .with_chord_interface(TowerChordInterface {
                toward_gate: Direction::West,
                bearing_depth: GridLength::new(24).expect("1.2 m tower chord"),
            })
            .with_secondary_chord_interface(TowerChordInterface {
                toward_gate: Direction::North,
                bearing_depth: GridLength::new(24).expect("1.2 m tower return chord"),
            }),
        ],
        BuildingArchetype::CourtyardCastle => vec![
            tower(
                Vec2::new(0.0, 0.0),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(width, 0.0),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(0.0, depth),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(width, depth),
                Some(BattlementKind::Crenellated),
                false,
            ),
        ],
        BuildingArchetype::WalledKeep => {
            let margin = 9.0;
            let min = Vec2::splat(-margin);
            let max = Vec2::new(width + margin, depth + margin);
            let mut towers = [min, Vec2::new(max.x, min.y), Vec2::new(min.x, max.y), max]
                .into_iter()
                .map(|centre| {
                    RoundTower::new(
                        grid_point(centre),
                        CellDiameter::new(4).expect("four-cell tower diameter is valid"),
                        6.0,
                        1.2,
                        None,
                        Some(BattlementKind::Crenellated),
                    )
                    .expect("curtain corner tower uses a room-grid vertex")
                })
                .collect::<Vec<_>>();
            for gatehouse in gatehouses {
                if let Some(wall) = curtain_walls.get(gatehouse.curtain_wall_index)
                    && let Some(resolved) = resolve_gatehouse_towers(*gatehouse, *wall, 6.0)
                {
                    towers.extend(resolved);
                }
            }
            towers
        }
        BuildingArchetype::ArtilleryRondelCastle => {
            let diameter = CellDiameter::new(8).expect("eight-cell artillery rondel diameter");
            let bearing = GridLength::new(18).expect("0.9 m curtain return bearing");
            let make = |centre: Vec2, first: Direction, second: Direction| {
                RoundTower::new(grid_point(centre), diameter, 7.30, 1.2, None, None)
                    .expect("artillery rondel anchor matches even-cell parity")
                    .with_chord_interface(TowerChordInterface {
                        toward_gate: first,
                        bearing_depth: bearing,
                    })
                    .with_secondary_chord_interface(TowerChordInterface {
                        toward_gate: second,
                        bearing_depth: bearing,
                    })
            };
            vec![
                make(Vec2::new(-16.5, -13.5), Direction::East, Direction::North),
                make(Vec2::new(28.5, -13.5), Direction::West, Direction::North),
                make(Vec2::new(-16.5, 25.5), Direction::East, Direction::South),
                make(Vec2::new(28.5, 25.5), Direction::West, Direction::South),
            ]
        }
        _ => Vec::new(),
    }
}

fn derive_gatehouse_assemblies(program: &BuildingProgram) -> Vec<GatehouseAssemblySpec> {
    if program.archetype != BuildingArchetype::WalledKeep {
        return Vec::new();
    }
    vec![GatehouseAssemblySpec {
        curtain_wall_index: 0,
        gate_width: GridLength::new(64).expect("3.2 m project gate width"),
        tower_diameter: CellDiameter::new(4).expect("four-cell gate tower diameter"),
        tower_shell: GridLength::new(24).expect("1.2 m project shell"),
        jamb_reveal: GridLength::new(13).expect("0.65 m parity-aligned jamb reveal"),
        chord_bearing: GridLength::new(6).expect("0.3 m bonded bearing"),
        chamber_depth: GridLength::new(52).expect("2.6 m chamber depth"),
        arch_ring_depth: GridLength::new(5).expect("0.25 m masonry arch ring"),
        arch_rise: GridLength::new(8).expect("0.4 m segmental arch rise"),
        curtain_return_bond: GridLength::new(2).expect("0.1 m bonded curtain return"),
    }]
}

fn resolve_gatehouse_towers(
    spec: GatehouseAssemblySpec,
    wall: CurtainWallRun,
    wall_height: f32,
) -> Option<[RoundTower; 2]> {
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let outward = direction_vector(wall.outward);
    let cardinal = tangent.x.abs() >= 0.999 || tangent.y.abs() >= 0.999;
    if !cardinal || tangent.dot(outward).abs() > 0.001 {
        return None;
    }
    let threshold = (wall.start + wall.end) * 0.5;
    let radius = spec.tower_diameter.metres() * 0.5;
    let offset = spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + radius;
    let left_centre = threshold - tangent * offset;
    let right_centre = threshold + tangent * offset;
    let along = cardinal_direction(tangent);
    let against = along.opposite();
    let make = |centre: Vec2, toward_gate| {
        RoundTower::new(
            grid_point(centre),
            spec.tower_diameter,
            wall_height,
            spec.tower_shell.metres(),
            None,
            Some(BattlementKind::Crenellated),
        )
        .expect("gatehouse spec must resolve parity-aligned tower anchors")
        .with_chord_interface(TowerChordInterface {
            toward_gate,
            bearing_depth: spec.chord_bearing,
        })
    };
    Some([make(left_centre, along), make(right_centre, against)])
}

fn derive_square_towers(program: &BuildingProgram) -> Vec<SquareTower> {
    if program.archetype != BuildingArchetype::Cathedral {
        return Vec::new();
    }
    let size = Vec2::splat(5.4);
    // The bell stage begins above the nave weather contour.  21.5 metres
    // keeps its paired sound openings clear of the main-roof upstand while
    // retaining a substantial masonry tower between nave ridge and bell floor.
    let wall_height_metres = 21.5;
    vec![SquareTower {
        centre: Vec2::new(2.7, 10.5),
        size,
        wall_height_metres,
        roof: RoofPiece {
            kind: RoofKind::Pavilion,
            centre: Vec2::new(2.7, 10.5),
            size,
            base_height_metres: wall_height_metres,
            pitch_degrees: 68.0,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.3,
            gable_profile: GableProfile::Plain,
        },
        bell_openings: true,
    }]
}

fn derive_stairs(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    towers: &[RoundTower],
    straight_core: Option<&StraightStairCore>,
) -> Vec<Stair> {
    if storeys.len() < 2 {
        return Vec::new();
    }
    // A roof-kernel demonstrator may add an isolated round tower to an
    // otherwise civilian fixture. It is evidence geometry, not the occupied
    // building's circulation authority, so it must not replace the real
    // StairHall route.
    let towers_own_circulation = matches!(
        program.archetype,
        BuildingArchetype::CastleGatehouse
            | BuildingArchetype::CourtyardCastle
            | BuildingArchetype::WalledKeep
            | BuildingArchetype::ArtilleryRondelCastle
    );
    if towers_own_circulation && !towers.is_empty() {
        let mut stairs = towers
            .iter()
            .map(|tower| {
                let base_height_metres = 0.15;
                Stair::Spiral {
                    centre: tower.centre_metres(),
                    base_height_metres,
                    rise_metres: tower.wall_height_metres - base_height_metres,
                    inner_radius_metres: 0.28,
                    outer_radius_metres: (tower.radius_metres()
                        - tower.wall_thickness_metres
                        - 0.15)
                        .max(0.75),
                    turns: tower.wall_height_metres / program.storey_height_metres * 0.9,
                    clockwise: stable_noise(layout_seed(program), 11, Cell::new(0, 0))
                        .is_multiple_of(2),
                    tread_count: (tower.wall_height_metres / 0.19).ceil() as u16,
                }
            })
            .collect::<Vec<_>>();
        if matches!(
            program.archetype,
            BuildingArchetype::WalledKeep | BuildingArchetype::ArtilleryRondelCastle
        ) {
            let (width, depth) = program.footprint.dimensions();
            let base_height_metres = 0.15;
            let rise_metres =
                storeys.len() as f32 * program.storey_height_metres - base_height_metres;
            stairs.push(Stair::Spiral {
                centre: Vec2::new(
                    f32::from(width) * CELL_SIZE_METRES * 0.5,
                    f32::from(depth) * CELL_SIZE_METRES * 0.5,
                ),
                base_height_metres,
                rise_metres,
                inner_radius_metres: 0.25,
                outer_radius_metres: 1.25,
                turns: 2.8,
                clockwise: true,
                tread_count: (rise_metres / 0.19).ceil() as u16,
            });
        }
        return stairs;
    }

    if let Some(core) = straight_core {
        let axis = direction_vector(core.direction);
        let opposite = cardinal_direction(-axis);
        let far_origin = core.origin + axis * STRAIGHT_STAIR_RUN_METRES;
        return (core.lowest_storey..core.highest_storey)
            .enumerate()
            .map(|(flight_index, level)| {
                let ascending_forward = flight_index.is_multiple_of(2);
                Stair::Straight {
                    start: if ascending_forward {
                        core.origin
                    } else {
                        far_origin
                    },
                    direction: if ascending_forward {
                        core.direction
                    } else {
                        opposite
                    },
                    base_height_metres: f32::from(level) * program.storey_height_metres,
                    rise_metres: program.storey_height_metres,
                    width_metres: 1.0,
                    tread_count: 18,
                    run_metres: STRAIGHT_STAIR_RUN_METRES,
                }
            })
            .collect();
    }

    let stair_room = storeys[0]
        .rooms
        .iter()
        .find(|room| room.kind == RoomKind::StairHall)
        .or_else(|| storeys[0].rooms.first());
    stair_room
        .and_then(|room| room.cells.get(room.cells.len() / 2))
        .map(|cell| {
            vec![Stair::Straight {
                start: cell.centre(),
                direction: Direction::North,
                base_height_metres: 0.12,
                rise_metres: program.storey_height_metres,
                width_metres: 1.0,
                tread_count: 17,
                run_metres: STRAIGHT_STAIR_RUN_METRES,
            }]
        })
        .unwrap_or_default()
}
