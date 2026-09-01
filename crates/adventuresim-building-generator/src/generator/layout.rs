fn footprint_cells(footprint: Footprint) -> Result<Vec<Cell>, GenerationError> {
    let (width, depth) = footprint.dimensions();
    if width < 3 || depth < 3 || width > i16::MAX as u16 || depth > i16::MAX as u16 {
        return Err(GenerationError::InvalidFootprint);
    }
    let mut cells = Vec::new();
    match footprint {
        Footprint::Rectangle { .. } => {
            for z in 0..depth {
                for x in 0..width {
                    cells.push(Cell::new(x as i16, z as i16));
                }
            }
        }
        Footprint::Courtyard {
            wing, gate_width, ..
        } => {
            if wing < 2
                || wing * 2 >= width
                || wing * 2 >= depth
                || gate_width == 0
                || gate_width > width - wing * 2
            {
                return Err(GenerationError::InvalidFootprint);
            }
            for z in 0..depth {
                for x in 0..width {
                    if x < wing || x >= width - wing || z < wing || z >= depth - wing {
                        cells.push(Cell::new(x as i16, z as i16));
                    }
                }
            }
        }
    }
    Ok(cells)
}

fn resolve_straight_stair_core(
    program: &BuildingProgram,
    footprint: &[Cell],
) -> Result<Option<StraightStairCore>, GenerationError> {
    if program.storeys.len() > 1 && program.vertical_connections.is_empty() {
        return Err(GenerationError::UnsatisfiedVerticalCirculation {
            connection: 0,
            reason: "a multi-storey programme declares no vertical connection".to_owned(),
        });
    }
    for (connection, requirement) in program.vertical_connections.iter().enumerate() {
        let (lowest_storey, highest_storey) = match *requirement {
            VerticalConnectionRequirement::StraightStair {
                lowest_storey,
                highest_storey,
                ..
            }
            | VerticalConnectionRequirement::TowerSpiral {
                lowest_storey,
                highest_storey,
            } => (lowest_storey, highest_storey),
        };
        if lowest_storey >= highest_storey || usize::from(highest_storey) >= program.storeys.len() {
            return Err(GenerationError::UnsatisfiedVerticalCirculation {
                connection,
                reason: format!(
                    "invalid served-storey range {lowest_storey}..={highest_storey} for {} storeys",
                    program.storeys.len()
                ),
            });
        }
    }
    for lower_storey in 0..program.storeys.len().saturating_sub(1) as u16 {
        let covered = program.vertical_connections.iter().any(|requirement| {
            let (lowest_storey, highest_storey) = match *requirement {
                VerticalConnectionRequirement::StraightStair {
                    lowest_storey,
                    highest_storey,
                    ..
                }
                | VerticalConnectionRequirement::TowerSpiral {
                    lowest_storey,
                    highest_storey,
                } => (lowest_storey, highest_storey),
            };
            lowest_storey <= lower_storey && highest_storey > lower_storey
        });
        if !covered {
            return Err(GenerationError::UnsatisfiedVerticalCirculation {
                connection: 0,
                reason: format!(
                    "no declared connector crosses storeys {lower_storey} and {}",
                    lower_storey + 1
                ),
            });
        }
    }
    let straight = program
        .vertical_connections
        .iter()
        .enumerate()
        .filter_map(|(index, requirement)| match *requirement {
            VerticalConnectionRequirement::StraightStair {
                lowest_storey,
                highest_storey,
                landing_room,
            } => Some((index, lowest_storey, highest_storey, landing_room)),
            VerticalConnectionRequirement::TowerSpiral { .. } => None,
        })
        .collect::<Vec<_>>();
    let Some(&(connection, lowest_storey, highest_storey, landing_room)) = straight.first() else {
        return Ok(None);
    };
    if straight.len() > 1 {
        return Err(GenerationError::UnsatisfiedVerticalCirculation {
            connection,
            reason: "the bounded civilian solver supports one shared straight stair core"
                .to_owned(),
        });
    }
    if lowest_storey != 0 || usize::from(highest_storey) + 1 != program.storeys.len() {
        return Err(GenerationError::UnsatisfiedVerticalCirculation {
            connection,
            reason: "the bounded civilian stair core must serve every occupied storey".to_owned(),
        });
    }
    for level in lowest_storey..=highest_storey {
        if !program.storeys[usize::from(level)]
            .rooms
            .iter()
            .any(|room| room.kind == landing_room)
        {
            return Err(GenerationError::UnsatisfiedVerticalCirculation {
                connection,
                reason: format!("storey {level} has no {landing_room:?} landing room"),
            });
        }
    }

    // A 4 x 2 cell core contains the 3.2 m flight, its 1.0 m clear width,
    // stringers, and a landing at both ends.  Reserving the same room cells on
    // every served storey prevents later wall derivation from boxing in an
    // otherwise physically valid stair.
    let usable = footprint.iter().copied().collect::<HashSet<_>>();
    let (width, depth) = program.footprint.dimensions();
    let building_centre = Vec2::new(f32::from(width), f32::from(depth)) * 0.5;
    let mut candidates = Vec::new();
    for z in 0..i16::try_from(depth).unwrap() {
        for x in 0..i16::try_from(width).unwrap() {
            let anchor = Cell::new(x, z);
            for direction in Direction::ALL {
                let (long, lateral) = match direction {
                    Direction::North | Direction::South => ((0_i16, 1_i16), (1_i16, 0_i16)),
                    Direction::East | Direction::West => ((1_i16, 0_i16), (0_i16, 1_i16)),
                };
                let cells = (0..4_i16)
                    .flat_map(|along| {
                        (0..2_i16).map(move |across| {
                            Cell::new(
                                anchor.x + long.0 * along + lateral.0 * across,
                                anchor.z + long.1 * along + lateral.1 * across,
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                if !cells.iter().all(|cell| usable.contains(cell)) {
                    continue;
                }
                let rectangle_centre =
                    cells.iter().map(|cell| cell.centre()).sum::<Vec2>() / cells.len() as f32;
                let origin = match direction {
                    Direction::North => Vec2::new(
                        (f32::from(anchor.x) + 1.0) * CELL_SIZE_METRES,
                        (f32::from(anchor.z) + 0.5) * CELL_SIZE_METRES,
                    ),
                    Direction::South => Vec2::new(
                        (f32::from(anchor.x) + 1.0) * CELL_SIZE_METRES,
                        (f32::from(anchor.z) + 3.5) * CELL_SIZE_METRES,
                    ),
                    Direction::East => Vec2::new(
                        (f32::from(anchor.x) + 0.5) * CELL_SIZE_METRES,
                        (f32::from(anchor.z) + 1.0) * CELL_SIZE_METRES,
                    ),
                    Direction::West => Vec2::new(
                        (f32::from(anchor.x) + 3.5) * CELL_SIZE_METRES,
                        (f32::from(anchor.z) + 1.0) * CELL_SIZE_METRES,
                    ),
                };
                let centre_distance =
                    (rectangle_centre / CELL_SIZE_METRES - building_centre).length_squared();
                let direction_salt = match direction {
                    Direction::North => 0,
                    Direction::East => 1,
                    Direction::South => 2,
                    Direction::West => 3,
                };
                candidates.push((
                    centre_distance,
                    stable_noise(layout_seed(program), 0x51a1 + direction_salt, anchor),
                    origin,
                    direction,
                    cells,
                ));
            }
        }
    }
    candidates.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    let Some((_, _, origin, direction, reserved_cells)) = candidates.into_iter().next() else {
        return Err(GenerationError::UnsatisfiedVerticalCirculation {
            connection,
            reason: "the footprint has no contiguous 4 x 2 cell stair-and-landing core".to_owned(),
        });
    };
    Ok(Some(StraightStairCore {
        lowest_storey,
        highest_storey,
        landing_room,
        origin,
        direction,
        reserved_cells,
    }))
}

fn allocate_rooms(
    footprint: &[Cell],
    width: u16,
    depth: u16,
    requirements: &[RoomRequirement],
    seed: u64,
    archetype: BuildingArchetype,
    reservations: &BTreeMap<Cell, usize>,
) -> BTreeMap<Cell, usize> {
    let usable = footprint.iter().copied().collect::<BTreeSet<_>>();
    let mut assignments = reservations.clone();
    let mut room_seeds = vec![None; requirements.len()];
    for (cell, room_index) in reservations {
        room_seeds[*room_index].get_or_insert(*cell);
    }

    if let Some(passage_index) = requirements
        .iter()
        .position(|room| room.kind == RoomKind::Passage)
    {
        let passage_width = match archetype {
            BuildingArchetype::CastleGatehouse => 2,
            BuildingArchetype::CourtyardCastle => 4,
            _ => 1,
        };
        let start_x = i16::try_from(width / 2).unwrap() - passage_width / 2;
        let passage_depth = match archetype {
            BuildingArchetype::CourtyardCastle => match requirements.len() {
                0 => 0,
                _ => 4,
            },
            _ => i16::try_from(depth).unwrap(),
        };
        for z in 0..passage_depth {
            for x in start_x..start_x + passage_width {
                let cell = Cell::new(x, z);
                if usable.contains(&cell) {
                    assignments.insert(cell, passage_index);
                    room_seeds[passage_index].get_or_insert(cell);
                }
            }
        }
    }

    let mut claimed_seeds = assignments.keys().copied().collect::<HashSet<_>>();
    for (room_index, requirement) in requirements.iter().enumerate() {
        if room_seeds[room_index].is_some() {
            continue;
        }
        let selected = footprint
            .iter()
            .copied()
            .filter(|cell| !claimed_seeds.contains(cell))
            .min_by_key(|cell| seed_score(*cell, requirement, width, depth, room_index, seed))
            .expect("room count is bounded by footprint cells");
        assignments.insert(selected, room_index);
        claimed_seeds.insert(selected);
        room_seeds[room_index] = Some(selected);
    }

    while assignments.len() < footprint.len() {
        let room_counts = room_counts(requirements.len(), &assignments);
        let mut best: Option<(u64, u64, u64, Cell, usize)> = None;
        for cell in footprint.iter().copied() {
            if assignments.contains_key(&cell) {
                continue;
            }
            let neighbouring_rooms = Direction::ALL
                .into_iter()
                .filter_map(|direction| assignments.get(&cell.neighbour(direction)).copied())
                .collect::<BTreeSet<_>>();
            for room_index in neighbouring_rooms {
                if requirements[room_index].kind == RoomKind::Passage {
                    continue;
                }
                let preferred = u64::from(requirements[room_index].preferred_cells.max(1));
                let fill_ratio = room_counts[room_index] as u64 * 10_000 / preferred;
                let seed_cell = room_seeds[room_index].expect("every room has a seed");
                let distance =
                    cell.x.abs_diff(seed_cell.x) as u64 + cell.z.abs_diff(seed_cell.z) as u64;
                let same_room_neighbours = Direction::ALL
                    .into_iter()
                    .filter(|direction| {
                        assignments.get(&cell.neighbour(*direction)) == Some(&room_index)
                    })
                    .count() as u64;
                let geometry_score = distance * 8 + (4 - same_room_neighbours) * 12;
                let candidate = (
                    fill_ratio,
                    geometry_score,
                    stable_noise(seed, room_index as u64, cell) % 97,
                    cell,
                    room_index,
                );
                if best.is_none_or(|current| candidate < current) {
                    best = Some(candidate);
                }
            }
        }
        let (_, _, _, cell, room_index) =
            best.expect("connected footprint always has an expansion edge");
        assignments.insert(cell, room_index);
    }

    assignments
}

fn seed_score(
    cell: Cell,
    requirement: &RoomRequirement,
    width: u16,
    depth: u16,
    room_index: usize,
    seed: u64,
) -> u64 {
    let x = i32::from(cell.x);
    let z = i32::from(cell.z);
    let max_x = i32::from(width) - 1;
    let max_z = i32::from(depth) - 1;
    let centre_x = max_x / 2;
    let centre_z = max_z / 2;
    let exterior_distance = x.min(max_x - x).min(z).min(max_z - z).max(0) as u64;
    let centre_distance =
        (x - centre_x).unsigned_abs() as u64 + (z - centre_z).unsigned_abs() as u64;
    let south_centre = z.unsigned_abs() as u64 * 8 + (x - centre_x).unsigned_abs() as u64;
    let north_centre = (max_z - z).unsigned_abs() as u64 * 8 + (x - centre_x).unsigned_abs() as u64;
    let west_centre = x.unsigned_abs() as u64 * 8 + (z - centre_z).unsigned_abs() as u64;
    let east_centre = (max_x - x).unsigned_abs() as u64 * 8 + (z - centre_z).unsigned_abs() as u64;
    let functional = match requirement.kind {
        RoomKind::EntranceHall | RoomKind::Shop | RoomKind::Passage => south_centre,
        RoomKind::StairHall => centre_distance,
        RoomKind::Kitchen | RoomKind::Pantry => north_centre,
        RoomKind::Workshop | RoomKind::Armoury => west_centre,
        RoomKind::Guardroom => east_centre,
        RoomKind::GreatHall
        | RoomKind::CommonRoom
        | RoomKind::Gallery
        | RoomKind::Chapel
        | RoomKind::Nave
        | RoomKind::Chancel => north_centre + centre_distance,
        RoomKind::Storage | RoomKind::Sacristy => west_centre + north_centre,
        RoomKind::Bedchamber | RoomKind::TowerChamber => east_centre + north_centre,
    };
    functional * 1_000
        + if requirement.needs_exterior {
            exterior_distance * 4_000
        } else {
            0
        }
        + stable_noise(seed, room_index as u64, cell) % 499
}

fn stable_noise(seed: u64, salt: u64, cell: Cell) -> u64 {
    let mut value = seed
        ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (cell.x as u16 as u64) << 16
        ^ cell.z as u16 as u64;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn room_counts(room_count: usize, assignments: &BTreeMap<Cell, usize>) -> Vec<usize> {
    let mut counts = vec![0; room_count];
    for room in assignments.values().copied() {
        counts[room] += 1;
    }
    counts
}

fn collect_rooms(
    assignments: &BTreeMap<Cell, usize>,
    requirements: &[RoomRequirement],
) -> Vec<Room> {
    requirements
        .iter()
        .enumerate()
        .map(|(room_index, requirement)| Room {
            id: room_index as u16,
            kind: requirement.kind,
            cells: assignments
                .iter()
                .filter_map(|(cell, assigned)| (*assigned == room_index).then_some(*cell))
                .collect(),
        })
        .collect()
}

fn cells_are_connected(cells: &[Cell]) -> bool {
    let Some(first) = cells.first().copied() else {
        return false;
    };
    let all = cells.iter().copied().collect::<HashSet<_>>();
    let mut reached = HashSet::from([first]);
    let mut pending = VecDeque::from([first]);
    while let Some(cell) = pending.pop_front() {
        for direction in Direction::ALL {
            let neighbour = cell.neighbour(direction);
            if all.contains(&neighbour) && reached.insert(neighbour) {
                pending.push_back(neighbour);
            }
        }
    }
    reached.len() == cells.len()
}
