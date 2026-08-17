use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use bevy::math::Vec2;
use thiserror::Error;

use crate::{
    Bartizan, BattlementKind, BattlementRun, BuildingArchetype, BuildingPlan, BuildingProgram,
    CELL_SIZE_METRES, Cell, Direction, DormerKind, Footprint, GableProfile, Opening, OpeningKind,
    RidgeAxis, RoofDormer, RoofKind, RoofPiece, Room, RoomKind, RoomRequirement, RoundTower, Stair,
    StoreyPlan, WallWalk,
};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GenerationError {
    #[error("building footprint is empty or invalid")]
    InvalidFootprint,
    #[error("storey {level} has no requested rooms")]
    EmptyStorey { level: usize },
    #[error("storey {level} requests {rooms} rooms for only {cells} usable cells")]
    TooManyRooms {
        level: usize,
        rooms: usize,
        cells: usize,
    },
    #[error("storey {level} produced a disconnected room {room}")]
    DisconnectedRoom { level: usize, room: u16 },
    #[error("storey {level} does not have enough shared boundaries to connect its rooms")]
    DisconnectedStorey { level: usize },
}

pub fn generate(program: &BuildingProgram) -> Result<BuildingPlan, GenerationError> {
    let footprint_cells = footprint_cells(program.footprint)?;
    let (width, depth) = program.footprint.dimensions();
    let mut storeys = Vec::with_capacity(program.storeys.len());

    for (level, storey_program) in program.storeys.iter().enumerate() {
        if storey_program.rooms.is_empty() {
            return Err(GenerationError::EmptyStorey { level });
        }
        if storey_program.rooms.len() > footprint_cells.len() {
            return Err(GenerationError::TooManyRooms {
                level,
                rooms: storey_program.rooms.len(),
                cells: footprint_cells.len(),
            });
        }

        let assignments = allocate_rooms(
            &footprint_cells,
            width,
            depth,
            &storey_program.rooms,
            program.seed.wrapping_add(level as u64 * 0x9e37_79b9),
            program.archetype,
        );
        let rooms = collect_rooms(&assignments, &storey_program.rooms);
        for room in &rooms {
            if !cells_are_connected(&room.cells) {
                return Err(GenerationError::DisconnectedRoom {
                    level,
                    room: room.id,
                });
            }
        }
        let walls = derive_walls(&footprint_cells, &assignments);
        let openings = derive_openings(
            &walls,
            &storey_program.rooms,
            program.archetype,
            program.seed.wrapping_add(level as u64),
            level,
        )?;
        storeys.push(StoreyPlan {
            level: level as u16,
            rooms,
            walls,
            openings,
        });
    }

    let roofs = derive_roofs(program);
    let roof_dormers = derive_roof_dormers(program);
    let towers = derive_towers(program);
    let stairs = derive_stairs(program, &storeys, &towers);
    let battlements = derive_battlements(program);
    let wall_walks = derive_wall_walks(&battlements, &towers);
    let bartizans = derive_bartizans(program);

    Ok(BuildingPlan {
        archetype: program.archetype,
        seed: program.seed,
        footprint: program.footprint,
        storey_height_metres: program.storey_height_metres,
        wall_style: program.wall_style,
        timber_frame_style: program.timber_frame_style,
        upper_storey_projection_metres: program.upper_storey_projection_metres,
        storeys,
        roofs,
        roof_dormers,
        towers,
        stairs,
        battlements,
        wall_walks,
        bartizans,
    })
}

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

fn allocate_rooms(
    footprint: &[Cell],
    width: u16,
    depth: u16,
    requirements: &[RoomRequirement],
    seed: u64,
    archetype: BuildingArchetype,
) -> BTreeMap<Cell, usize> {
    let usable = footprint.iter().copied().collect::<BTreeSet<_>>();
    let mut assignments = BTreeMap::new();
    let mut room_seeds = vec![None; requirements.len()];

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
        RoomKind::GreatHall | RoomKind::CommonRoom | RoomKind::Gallery | RoomKind::Chapel => {
            north_centre + centre_distance
        }
        RoomKind::Storage => west_centre + north_centre,
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

fn derive_walls(
    footprint: &[Cell],
    assignments: &BTreeMap<Cell, usize>,
) -> Vec<crate::WallSegment> {
    let occupied = footprint.iter().copied().collect::<HashSet<_>>();
    let mut walls = Vec::new();
    for cell in footprint.iter().copied() {
        let inside_room = assignments[&cell] as u16;
        for direction in Direction::ALL {
            let neighbour = cell.neighbour(direction);
            if !occupied.contains(&neighbour) {
                walls.push(crate::WallSegment {
                    cell,
                    direction,
                    inside_room,
                    outside_room: None,
                });
            } else if matches!(direction, Direction::North | Direction::East) {
                let other_room = assignments[&neighbour] as u16;
                if inside_room != other_room {
                    walls.push(crate::WallSegment {
                        cell,
                        direction,
                        inside_room,
                        outside_room: Some(other_room),
                    });
                }
            }
        }
    }
    walls
}

fn derive_openings(
    walls: &[crate::WallSegment],
    requirements: &[RoomRequirement],
    archetype: BuildingArchetype,
    seed: u64,
    level: usize,
) -> Result<Vec<Opening>, GenerationError> {
    let mut openings = Vec::new();
    let mut occupied_walls = HashSet::new();

    if level == 0 {
        let entrance_room = requirements
            .iter()
            .position(|room| matches!(room.kind, RoomKind::EntranceHall | RoomKind::Passage))
            .unwrap_or(0) as u16;
        let mut entrance_candidates = walls
            .iter()
            .enumerate()
            .filter(|(_, wall)| {
                wall.exterior()
                    && wall.inside_room == entrance_room
                    && wall.direction == Direction::South
            })
            .collect::<Vec<_>>();
        entrance_candidates.sort_by_key(|(_, wall)| wall.cell.x);
        let gate = matches!(
            archetype,
            BuildingArchetype::HallHouse
                | BuildingArchetype::CastleGatehouse
                | BuildingArchetype::CourtyardCastle
        );
        let selected_entrances = if gate {
            let middle = entrance_candidates.len() / 2;
            let start = middle.saturating_sub(1);
            &entrance_candidates[start..entrance_candidates.len().min(start + 2)]
        } else {
            let middle = entrance_candidates.len() / 2;
            &entrance_candidates[middle..entrance_candidates.len().min(middle + 1)]
        };
        for (wall_index, _) in selected_entrances {
            openings.push(Opening {
                wall: *wall_index,
                kind: if gate {
                    OpeningKind::Gate
                } else {
                    OpeningKind::Door
                },
                width_metres: if gate { 1.35 } else { 1.0 },
                sill_metres: 0.0,
                height_metres: if gate { 2.8 } else { 2.15 },
            });
            occupied_walls.insert(*wall_index);
        }
        if requirements[usize::from(entrance_room)].kind == RoomKind::Passage {
            let mut exit_candidates = walls
                .iter()
                .enumerate()
                .filter(|(_, wall)| {
                    wall.exterior()
                        && wall.inside_room == entrance_room
                        && wall.direction == Direction::North
                })
                .collect::<Vec<_>>();
            exit_candidates.sort_by_key(|(_, wall)| wall.cell.x);
            let middle = exit_candidates.len() / 2;
            let start = middle.saturating_sub(1);
            for (wall_index, _) in &exit_candidates[start..exit_candidates.len().min(start + 2)] {
                openings.push(Opening {
                    wall: *wall_index,
                    kind: OpeningKind::Gate,
                    width_metres: 1.35,
                    sill_metres: 0.0,
                    height_metres: 2.8,
                });
                occupied_walls.insert(*wall_index);
            }
        }
    }

    let mut shared = BTreeMap::<(u16, u16), Vec<usize>>::new();
    for (wall_index, wall) in walls.iter().enumerate() {
        if let Some(other) = wall.outside_room {
            let pair = if wall.inside_room < other {
                (wall.inside_room, other)
            } else {
                (other, wall.inside_room)
            };
            shared.entry(pair).or_default().push(wall_index);
        }
    }
    let mut edges = shared
        .into_iter()
        .map(|(pair, candidates)| {
            let left = &requirements[usize::from(pair.0)];
            let right = &requirements[usize::from(pair.1)];
            let preferred = left.preferred_neighbours.contains(&right.kind)
                || right.preferred_neighbours.contains(&left.kind);
            (preferred, pair, candidates)
        })
        .collect::<Vec<_>>();
    edges.sort_by_key(|(preferred, pair, _)| (!*preferred, *pair));
    let mut sets = DisjointSets::new(requirements.len());
    for (_, (left, right), candidates) in edges {
        if sets.union(usize::from(left), usize::from(right)) {
            let wall_index = candidates[candidates.len() / 2];
            openings.push(Opening {
                wall: wall_index,
                kind: OpeningKind::Door,
                width_metres: 0.95,
                sill_metres: 0.0,
                height_metres: 2.1,
            });
            occupied_walls.insert(wall_index);
        }
    }
    if sets.component_count() != 1 {
        return Err(GenerationError::DisconnectedStorey { level });
    }

    for (wall_index, wall) in walls.iter().enumerate() {
        if !wall.exterior() || occupied_walls.contains(&wall_index) {
            continue;
        }
        let room_kind = requirements[usize::from(wall.inside_room)].kind;
        if matches!(
            room_kind,
            RoomKind::Storage | RoomKind::Pantry | RoomKind::Passage
        ) || stable_noise(seed, wall_index as u64, wall.cell).is_multiple_of(3)
        {
            continue;
        }
        let fortified = matches!(
            archetype,
            BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
        );
        openings.push(Opening {
            wall: wall_index,
            kind: if fortified {
                OpeningKind::ArrowSlit
            } else {
                OpeningKind::Window
            },
            width_metres: if fortified { 0.18 } else { 0.85 },
            sill_metres: if fortified { 1.2 } else { 0.9 },
            height_metres: if fortified { 0.9 } else { 1.15 },
        });
    }

    openings.sort_by_key(|opening| opening.wall);
    Ok(openings)
}

fn derive_roofs(program: &BuildingProgram) -> Vec<RoofPiece> {
    let (width, depth) = program.footprint.dimensions();
    let size = Vec2::new(
        f32::from(width) * CELL_SIZE_METRES,
        f32::from(depth) * CELL_SIZE_METRES,
    );
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match (program.archetype, program.footprint) {
        (BuildingArchetype::TownHouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.45,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::HallHouse, _) => vec![RoofPiece {
            kind: RoofKind::HalfHip,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.65,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::FachwerkMerchantHouse, _) => vec![
            RoofPiece {
                kind: RoofKind::Gable,
                centre: size * 0.5,
                size,
                base_height_metres: top,
                pitch_degrees: program.roof_pitch_degrees,
                ridge_axis: RidgeAxis::Z,
                eave_metres: 0.55,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::new(size.x * 0.5, size.y * 0.28),
                size: Vec2::new(size.x * 0.48, size.y * 0.38),
                base_height_metres: top + 1.05,
                pitch_degrees: 59.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.28,
                gable_profile: GableProfile::Plain,
            },
        ],
        (BuildingArchetype::RenaissanceTownHall, _) => vec![
            RoofPiece {
                kind: RoofKind::HalfHip,
                centre: size * 0.5,
                size,
                base_height_metres: top,
                pitch_degrees: program.roof_pitch_degrees,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.65,
                gable_profile: GableProfile::Stepped,
            },
            RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::new(size.x * 0.5, size.y * 0.24),
                size: Vec2::new(size.x * 0.34, size.y * 0.42),
                base_height_metres: top + 0.85,
                pitch_degrees: 58.0,
                ridge_axis: RidgeAxis::Z,
                eave_metres: 0.3,
                gable_profile: GableProfile::Stepped,
            },
        ],
        (BuildingArchetype::CastleGatehouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5 + Vec2::Y * 0.5,
            size: Vec2::new(size.x - 0.8, size.y - 1.0),
            base_height_metres: top - 0.45,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.35,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::CourtyardCastle, Footprint::Courtyard { wing, .. }) => {
            let wing_metres = f32::from(wing) * CELL_SIZE_METRES;
            vec![
                RoofPiece {
                    kind: RoofKind::Gable,
                    centre: Vec2::new(size.x * 0.5, wing_metres * 0.5 + 0.45),
                    size: Vec2::new(size.x - 0.9, wing_metres - 0.9),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::X,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Stepped,
                },
                RoofPiece {
                    kind: RoofKind::Gable,
                    centre: Vec2::new(size.x * 0.5, size.y - wing_metres * 0.5 - 0.45),
                    size: Vec2::new(size.x - 0.9, wing_metres - 0.9),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::X,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Curved,
                },
                RoofPiece {
                    kind: RoofKind::Hip,
                    centre: Vec2::new(wing_metres * 0.5 + 0.45, size.y * 0.5),
                    size: Vec2::new(wing_metres - 0.9, size.y - wing_metres * 2.0),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::Z,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Plain,
                },
                RoofPiece {
                    kind: RoofKind::Hip,
                    centre: Vec2::new(size.x - wing_metres * 0.5 - 0.45, size.y * 0.5),
                    size: Vec2::new(wing_metres - 0.9, size.y - wing_metres * 2.0),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::Z,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Plain,
                },
            ]
        }
        _ => Vec::new(),
    }
}

fn derive_roof_dormers(program: &BuildingProgram) -> Vec<RoofDormer> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    let front_roof_inset = match program.footprint {
        Footprint::Courtyard { wing, .. } => f32::from(wing) * CELL_SIZE_METRES * 0.72,
        Footprint::Rectangle { .. } => 0.0,
    };
    let dormer = |centre, facing, kind, profile| RoofDormer {
        centre,
        base_height_metres: top + 1.15,
        width_metres: 2.15,
        depth_metres: 1.85,
        height_metres: 1.75,
        facing,
        kind,
        gable_profile: profile,
    };
    match program.archetype {
        BuildingArchetype::TownHouse => vec![dormer(
            Vec2::new(width, depth * 0.58),
            Direction::East,
            DormerKind::Gabled,
            GableProfile::Plain,
        )],
        BuildingArchetype::HallHouse => vec![
            dormer(
                Vec2::new(width, depth * 0.36),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.64),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::FachwerkMerchantHouse => vec![
            dormer(
                Vec2::new(width, depth * 0.38),
                Direction::East,
                DormerKind::Gabled,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.68),
                Direction::East,
                DormerKind::Hipped,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width * 0.5, 0.0),
                Direction::South,
                DormerKind::TransverseGable,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::RenaissanceTownHall => vec![
            dormer(
                Vec2::new(width * 0.22, 0.0),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
            dormer(
                Vec2::new(width * 0.78, 0.0),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
        ],
        BuildingArchetype::CastleGatehouse => Vec::new(),
        BuildingArchetype::CourtyardCastle => vec![
            dormer(
                Vec2::new(width * 0.3, front_roof_inset),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Stepped,
            ),
            dormer(
                Vec2::new(width * 0.7, front_roof_inset),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
        ],
    }
}

fn derive_towers(program: &BuildingProgram) -> Vec<RoundTower> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let wall_height = program.storeys.len() as f32 * program.storey_height_metres + 0.8;
    let tower = |centre: Vec2, battlement, roofed: bool| RoundTower {
        centre,
        radius_metres: 2.45,
        wall_height_metres: wall_height + 0.7,
        wall_thickness_metres: 0.35,
        roof: roofed.then_some(RoofPiece {
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
    };
    match program.archetype {
        BuildingArchetype::CastleGatehouse => vec![
            tower(
                Vec2::new(-0.25, 0.25),
                Some(BattlementKind::Machicolated),
                true,
            ),
            tower(
                Vec2::new(width + 0.25, 0.25),
                Some(BattlementKind::Machicolated),
                true,
            ),
        ],
        BuildingArchetype::CourtyardCastle => vec![
            tower(
                Vec2::new(0.0, 0.0),
                Some(BattlementKind::Machicolated),
                true,
            ),
            tower(
                Vec2::new(width, 0.0),
                Some(BattlementKind::Machicolated),
                true,
            ),
            tower(
                Vec2::new(0.0, depth),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(width, depth),
                Some(BattlementKind::GunLoopParapet),
                false,
            ),
        ],
        _ => Vec::new(),
    }
}

fn derive_stairs(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    towers: &[RoundTower],
) -> Vec<Stair> {
    if storeys.len() < 2 {
        return Vec::new();
    }
    if !towers.is_empty() {
        return towers
            .iter()
            .map(|tower| {
                let base_height_metres = 0.15;
                Stair::Spiral {
                    centre: tower.centre,
                    base_height_metres,
                    rise_metres: tower.wall_height_metres - base_height_metres,
                    inner_radius_metres: 0.28,
                    outer_radius_metres: tower.radius_metres - 0.45,
                    turns: tower.wall_height_metres / program.storey_height_metres * 0.9,
                    clockwise: stable_noise(program.seed, 11, Cell::new(0, 0)).is_multiple_of(2),
                    tread_count: (tower.wall_height_metres / 0.19).ceil() as u16,
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
            }]
        })
        .unwrap_or_default()
}

fn derive_battlements(program: &BuildingProgram) -> Vec<BattlementRun> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match program.archetype {
        BuildingArchetype::CastleGatehouse => vec![
            BattlementRun {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(width, 0.0),
                base_height_metres: top,
                kind: BattlementKind::Machicolated,
                outward: Direction::South,
            },
            BattlementRun {
                start: Vec2::new(width * 0.36, -0.08),
                end: Vec2::new(width * 0.64, -0.08),
                base_height_metres: top + 1.05,
                kind: BattlementKind::Breteche,
                outward: Direction::South,
            },
            BattlementRun {
                start: Vec2::new(0.0, depth),
                end: Vec2::new(width, depth),
                base_height_metres: top,
                kind: BattlementKind::OpenHoarding,
                outward: Direction::North,
            },
        ],
        BuildingArchetype::CourtyardCastle => vec![
            BattlementRun {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(width, 0.0),
                base_height_metres: top,
                kind: BattlementKind::Machicolated,
                outward: Direction::South,
            },
            BattlementRun {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(0.0, depth),
                base_height_metres: top,
                kind: BattlementKind::RoofedHoarding,
                outward: Direction::West,
            },
            BattlementRun {
                start: Vec2::new(width, 0.0),
                end: Vec2::new(width, depth),
                base_height_metres: top,
                kind: BattlementKind::CoveredWallWalk,
                outward: Direction::East,
            },
            BattlementRun {
                start: Vec2::new(0.0, depth),
                end: Vec2::new(width, depth),
                base_height_metres: top,
                kind: BattlementKind::PiercedCrenellated,
                outward: Direction::North,
            },
            BattlementRun {
                start: Vec2::new(width * 0.4, -0.08),
                end: Vec2::new(width * 0.6, -0.08),
                base_height_metres: top + 0.9,
                kind: BattlementKind::Breteche,
                outward: Direction::South,
            },
        ],
        _ => Vec::new(),
    }
}

fn derive_wall_walks(battlements: &[BattlementRun], towers: &[RoundTower]) -> Vec<WallWalk> {
    battlements
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
                    centre: tower.centre,
                    elevation_metres: tower.wall_height_metres,
                    outer_radius_metres: tower.radius_metres - 0.08,
                    stairwell_radius_metres: 0.62,
                }),
        )
        .collect()
}

fn derive_bartizans(program: &BuildingProgram) -> Vec<Bartizan> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match program.archetype {
        BuildingArchetype::CastleGatehouse => vec![
            Bartizan {
                centre: Vec2::new(width * 0.26, -0.35),
                base_height_metres: top - 0.45,
                radius_metres: 0.85,
                height_metres: 2.0,
                roofed: true,
            },
            Bartizan {
                centre: Vec2::new(width * 0.74, -0.35),
                base_height_metres: top - 0.45,
                radius_metres: 0.85,
                height_metres: 2.0,
                roofed: true,
            },
        ],
        BuildingArchetype::CourtyardCastle => vec![
            Bartizan {
                centre: Vec2::new(width * 0.35, -0.4),
                base_height_metres: top - 0.5,
                radius_metres: 0.9,
                height_metres: 2.15,
                roofed: false,
            },
            Bartizan {
                centre: Vec2::new(width * 0.65, -0.4),
                base_height_metres: top - 0.5,
                radius_metres: 0.9,
                height_metres: 2.15,
                roofed: true,
            },
            Bartizan {
                centre: Vec2::new(width + 0.4, depth * 0.5),
                base_height_metres: top - 0.5,
                radius_metres: 0.9,
                height_metres: 2.15,
                roofed: true,
            },
        ],
        _ => Vec::new(),
    }
}

struct DisjointSets {
    parents: Vec<usize>,
    components: usize,
}

impl DisjointSets {
    fn new(len: usize) -> Self {
        Self {
            parents: (0..len).collect(),
            components: len,
        }
    }

    fn find(&mut self, mut value: usize) -> usize {
        while self.parents[value] != value {
            self.parents[value] = self.parents[self.parents[value]];
            value = self.parents[value];
        }
        value
    }

    fn union(&mut self, left: usize, right: usize) -> bool {
        let left = self.find(left);
        let right = self.find(right);
        if left == right {
            return false;
        }
        self.parents[right] = left;
        self.components -= 1;
        true
    }

    const fn component_count(&self) -> usize {
        self.components
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BuildingArchetype, BuildingProgram, DormerKind, OpeningKind, RoofKind, TimberFrameStyle,
    };

    #[test]
    fn every_fixture_is_deterministic_connected_and_room_complete() {
        for archetype in BuildingArchetype::ALL {
            let program = BuildingProgram::fixture(archetype, 42);
            let first = generate(&program).unwrap();
            let second = generate(&program).unwrap();
            assert_eq!(
                serde_json::to_vec(&first).unwrap(),
                serde_json::to_vec(&second).unwrap(),
                "{archetype:?} must be reproducible"
            );
            for storey in &first.storeys {
                assert!(storey.rooms.iter().all(|room| !room.cells.is_empty()));
                assert!(
                    storey
                        .rooms
                        .iter()
                        .all(|room| cells_are_connected(&room.cells))
                );
                assert_eq!(
                    storey
                        .rooms
                        .iter()
                        .flat_map(|room| room.cells.iter())
                        .collect::<HashSet<_>>()
                        .len(),
                    storey
                        .rooms
                        .iter()
                        .map(|room| room.cells.len())
                        .sum::<usize>()
                );
                assert!(
                    storey
                        .openings
                        .iter()
                        .all(|opening| opening.wall < storey.walls.len())
                );
                assert!(
                    storey
                        .openings
                        .iter()
                        .any(|opening| opening.kind == OpeningKind::Door)
                );
            }
        }
    }

    #[test]
    fn civilian_profiles_have_steep_independent_roof_pieces() {
        let town = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 7)).unwrap();
        assert_eq!(town.roofs.len(), 1);
        assert_eq!(town.roofs[0].kind, RoofKind::Gable);
        assert!(town.roofs[0].pitch_degrees >= 50.0);

        let hall = generate(&BuildingProgram::fixture(BuildingArchetype::HallHouse, 7)).unwrap();
        assert_eq!(hall.roofs[0].kind, RoofKind::HalfHip);
        assert!(hall.roofs[0].eave_metres >= 0.5);
    }

    #[test]
    fn ornate_fachwerk_fixtures_have_projecting_storeys_and_complex_roofscapes() {
        for archetype in [
            BuildingArchetype::FachwerkMerchantHouse,
            BuildingArchetype::RenaissanceTownHall,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 17)).unwrap();
            assert_eq!(
                plan.timber_frame_style,
                Some(TimberFrameStyle::EarlyModernOrnate)
            );
            assert!(plan.upper_storey_projection_metres >= 0.2);
            assert!(plan.roofs.len() >= 2);
            let expected_dormers = if archetype == BuildingArchetype::FachwerkMerchantHouse {
                3
            } else {
                2
            };
            assert!(plan.roof_dormers.len() >= expected_dormers);
            if archetype == BuildingArchetype::FachwerkMerchantHouse {
                assert!(
                    plan.roof_dormers
                        .iter()
                        .any(|dormer| dormer.kind == DormerKind::TransverseGable)
                );
            }
        }
        let civic = generate(&BuildingProgram::fixture(
            BuildingArchetype::RenaissanceTownHall,
            17,
        ))
        .unwrap();
        assert!(
            civic
                .roofs
                .iter()
                .any(|roof| roof.gable_profile != GableProfile::Plain)
        );
    }

    #[test]
    fn castle_profiles_include_round_towers_spiral_stairs_and_defensive_crowns() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 19)).unwrap();
            assert!(plan.towers.len() >= 2);
            assert!(
                plan.stairs
                    .iter()
                    .any(|stair| matches!(stair, Stair::Spiral { .. }))
            );
            assert!(
                plan.battlements
                    .iter()
                    .any(|run| run.kind == BattlementKind::Machicolated)
            );
        }
    }

    #[test]
    fn castle_battlements_have_continuous_wall_walks_and_tower_access() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 29)).unwrap();
            let expected_linear_walks = plan
                .battlements
                .iter()
                .filter(|run| run.kind != BattlementKind::Breteche)
                .count();
            assert_eq!(
                plan.wall_walks
                    .iter()
                    .filter(|walk| matches!(walk, WallWalk::Linear { .. }))
                    .count(),
                expected_linear_walks
            );
            assert_eq!(
                plan.wall_walks
                    .iter()
                    .filter(|walk| matches!(walk, WallWalk::Round { .. }))
                    .count(),
                plan.towers.len()
            );
            for tower in &plan.towers {
                assert!(plan.stairs.iter().any(|stair| {
                    matches!(
                        stair,
                        Stair::Spiral {
                            centre,
                            base_height_metres,
                            rise_metres,
                            ..
                        } if *centre == tower.centre
                            && (*base_height_metres + *rise_metres
                                - tower.wall_height_metres)
                                .abs()
                                < 0.001
                    )
                }));
            }
            let wall_top = plan.storeys.len() as f32 * plan.storey_height_metres;
            for run in plan
                .battlements
                .iter()
                .filter(|run| run.kind != BattlementKind::Breteche)
            {
                assert_eq!(run.base_height_metres, wall_top);
            }
        }
    }

    #[test]
    fn fortified_exteriors_use_narrow_firing_loops_instead_of_glazed_windows() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 31)).unwrap();
            let exterior_openings = plan.storeys.iter().flat_map(|storey| {
                storey
                    .openings
                    .iter()
                    .filter(|opening| storey.walls[opening.wall].exterior())
            });
            let mut firing_loops = 0;
            for opening in exterior_openings {
                assert_ne!(opening.kind, OpeningKind::Window);
                if opening.kind == OpeningKind::ArrowSlit {
                    firing_loops += 1;
                    assert!(opening.width_metres <= 0.2);
                    assert!(opening.height_metres >= 0.8);
                }
            }
            assert!(firing_loops > 0);
        }
    }

    #[test]
    fn castle_fixtures_exercise_the_complete_defensive_crown_vocabulary() {
        let plans = [
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CastleGatehouse,
                23,
            ))
            .unwrap(),
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CourtyardCastle,
                23,
            ))
            .unwrap(),
        ];
        let kinds = plans
            .iter()
            .flat_map(|plan| {
                plan.battlements
                    .iter()
                    .map(|run| run.kind)
                    .chain(plan.towers.iter().filter_map(|tower| tower.battlement))
            })
            .collect::<HashSet<_>>();
        for expected in [
            BattlementKind::Crenellated,
            BattlementKind::PiercedCrenellated,
            BattlementKind::Machicolated,
            BattlementKind::OpenHoarding,
            BattlementKind::RoofedHoarding,
            BattlementKind::CoveredWallWalk,
            BattlementKind::GunLoopParapet,
            BattlementKind::Breteche,
        ] {
            assert!(kinds.contains(&expected), "missing {expected:?}");
        }
        assert!(plans.iter().all(|plan| !plan.bartizans.is_empty()));
    }

    #[test]
    fn courtyard_footprint_leaves_a_real_open_court() {
        let program = BuildingProgram::fixture(BuildingArchetype::CourtyardCastle, 3);
        let plan = generate(&program).unwrap();
        let Footprint::Courtyard {
            width, depth, wing, ..
        } = plan.footprint
        else {
            panic!("expected courtyard")
        };
        let centre = Cell::new((width / 2) as i16, (depth / 2) as i16);
        assert!(centre.x >= wing as i16 && centre.x < (width - wing) as i16);
        assert!(centre.z >= wing as i16 && centre.z < (depth - wing) as i16);
        assert!(
            plan.storeys[0]
                .rooms
                .iter()
                .all(|room| !room.cells.contains(&centre))
        );
        let passage = plan.storeys[0]
            .rooms
            .iter()
            .find(|room| room.kind == RoomKind::Passage)
            .unwrap();
        assert_eq!(passage.cells.len(), usize::from(wing * 4));
        assert_eq!(
            plan.storeys[0]
                .openings
                .iter()
                .filter(|opening| opening.kind == OpeningKind::Gate)
                .count(),
            4
        );
    }
}
