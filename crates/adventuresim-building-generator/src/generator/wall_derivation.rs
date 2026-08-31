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
    straight_stair_core: Option<&StraightStairCore>,
) -> Result<Vec<Opening>, GenerationError> {
    let mut openings = Vec::new();
    let exterior_extent = walls
        .iter()
        .filter(|wall| wall.exterior())
        .fold(None, |extent, wall| {
            let cell = wall.cell;
            Some(extent.map_or(
                (cell.x, cell.x, cell.z, cell.z),
                |(min_x, max_x, min_z, max_z): (i16, i16, i16, i16)| {
                    (
                        min_x.min(cell.x),
                        max_x.max(cell.x),
                        min_z.min(cell.z),
                        max_z.max(cell.z),
                    )
                },
            ))
        });
    let mut occupied_walls = HashSet::new();
    let mut required_room_connections = Vec::new();

    if let Some(core) = straight_stair_core.filter(|core| core.serves(level as u16)) {
        let stair_hall = requirements
            .iter()
            .position(|room| room.kind == core.landing_room)
            .expect("straight stair core landing room was validated before opening derivation")
            as u16;
        let flight_index = level as u16 - core.lowest_storey;
        let axis = direction_vector(core.direction);
        let landing = if flight_index.is_multiple_of(2) {
            core.origin
        } else {
            core.origin + axis * STRAIGHT_STAIR_RUN_METRES
        };
        let reserved_cells = core.reserved_cells.iter().copied().collect::<HashSet<_>>();
        let Some((wall_index, wall)) = walls
            .iter()
            .enumerate()
            .filter(|(_, wall)| {
                reserved_cells.contains(&wall.cell)
                    && wall.outside_room.is_some()
                    && (wall.inside_room == stair_hall || wall.outside_room == Some(stair_hall))
            })
            .min_by(|(_, left), (_, right)| {
                left.centre()
                    .distance_squared(landing)
                    .total_cmp(&right.centre().distance_squared(landing))
            })
        else {
            return Err(GenerationError::UnsatisfiedVerticalCirculation {
                connection: 0,
                reason: format!(
                    "storey {level} has no room-graph doorway adjacent to its stair landing"
                ),
            });
        };
        let other_room = wall
            .outside_room
            .filter(|room| *room != stair_hall)
            .unwrap_or(wall.inside_room);
        openings.push(Opening {
            wall: wall_index,
            kind: OpeningKind::Door,
            width_metres: 0.95,
            sill_metres: 0.0,
            height_metres: 2.1,
        });
        occupied_walls.insert(wall_index);
        required_room_connections.push((stair_hall, other_room));
    }

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
                | BuildingArchetype::WalledKeep
                | BuildingArchetype::ArtilleryRondelCastle
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
            let circulation_required =
                left.kind == RoomKind::StairHall || right.kind == RoomKind::StairHall;
            (circulation_required, preferred, pair, candidates)
        })
        .collect::<Vec<_>>();
    edges.sort_by_key(|(circulation_required, preferred, pair, _)| {
        (!*circulation_required, !*preferred, *pair)
    });
    let mut sets = DisjointSets::new(requirements.len());
    for (left, right) in required_room_connections {
        sets.union(usize::from(left), usize::from(right));
    }
    for (_, _, (left, right), candidates) in edges {
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
        // The two-post HallHouse MVP keeps its roof-carrying transverse
        // frames uninterrupted. The large hall doors remain opening-first;
        // optional ordinary lights are deferred rather than allowing a
        // seed-dependent window to cut a roof brace.
        if archetype == BuildingArchetype::HallHouse {
            continue;
        }
        // A one-cell opening at a perimeter corner consumes the return pier:
        // its jamb/reveal then occupies the perpendicular facade's frame
        // plane. Keep corner cells solid; nearby bays still provide light.
        let corner_cell = exterior_extent.is_some_and(|(min_x, max_x, min_z, max_z)| {
            (wall.cell.x == min_x || wall.cell.x == max_x)
                && (wall.cell.z == min_z || wall.cell.z == max_z)
        });
        if corner_cell {
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
            BuildingArchetype::CastleGatehouse
                | BuildingArchetype::CourtyardCastle
                | BuildingArchetype::WalledKeep
                | BuildingArchetype::ArtilleryRondelCastle
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

fn wall_material_and_thickness(
    archetype: BuildingArchetype,
    exterior: bool,
    level: u16,
) -> (crate::WallMaterialClass, crate::WallStructuralRole, f32) {
    if !exterior {
        return if matches!(
            archetype,
            BuildingArchetype::TownHouse
                | BuildingArchetype::HallHouse
                | BuildingArchetype::FachwerkCottage
                | BuildingArchetype::FachwerkMerchantHouse
                | BuildingArchetype::RenaissanceTownHall
        ) {
            (
                crate::WallMaterialClass::InternalTimber,
                crate::WallStructuralRole::LoadBearing,
                0.16,
            )
        } else {
            (
                crate::WallMaterialClass::InternalMasonry,
                crate::WallStructuralRole::LoadBearing,
                0.30,
            )
        };
    }
    match archetype {
        BuildingArchetype::TownHouse
        | BuildingArchetype::HallHouse
        | BuildingArchetype::FachwerkCottage
        | BuildingArchetype::FachwerkMerchantHouse => (
            crate::WallMaterialClass::TimberInfill,
            crate::WallStructuralRole::Infill,
            if level == 0 { 0.24 } else { 0.22 },
        ),
        BuildingArchetype::RenaissanceTownHall if level == 0 => (
            crate::WallMaterialClass::CivilianMasonry,
            crate::WallStructuralRole::LoadBearing,
            0.50,
        ),
        BuildingArchetype::RenaissanceTownHall => (
            crate::WallMaterialClass::TimberInfill,
            crate::WallStructuralRole::Infill,
            0.22,
        ),
        BuildingArchetype::Cathedral => (
            crate::WallMaterialClass::CathedralMasonry,
            crate::WallStructuralRole::Buttressed,
            0.90,
        ),
        BuildingArchetype::CastleGatehouse
        | BuildingArchetype::CourtyardCastle
        | BuildingArchetype::WalledKeep
        | BuildingArchetype::ArtilleryRondelCastle => (
            crate::WallMaterialClass::FortifiedMasonry,
            crate::WallStructuralRole::LoadBearing,
            1.20,
        ),
    }
}

fn two_centred_arc_radius(width_metres: f32, rise_metres: f32) -> f32 {
    let half_span = width_metres * 0.5;
    half_span + (rise_metres * rise_metres - half_span * half_span) / (2.0 * half_span.max(0.01))
}

fn opening_profile_for(
    archetype: BuildingArchetype,
    opening: Opening,
) -> (
    crate::OpeningUse,
    crate::OpeningProfile,
    crate::OpeningHeadKind,
) {
    match opening.kind {
        OpeningKind::Door => (
            crate::OpeningUse::Door,
            crate::OpeningProfile::Rectangular {
                width_metres: if matches!(
                    archetype,
                    BuildingArchetype::CastleGatehouse
                        | BuildingArchetype::CourtyardCastle
                        | BuildingArchetype::WalledKeep
                        | BuildingArchetype::ArtilleryRondelCastle
                ) {
                    // Project gate: a 0.78 m service-door pinch is permitted in
                    // thick inherited masonry where a full 0.90 m route would
                    // erase the bonded corner pier of a single-cell bay.
                    0.78
                } else {
                    opening.width_metres
                },
                height_metres: opening.height_metres,
            },
            if matches!(
                archetype,
                BuildingArchetype::TownHouse
                    | BuildingArchetype::HallHouse
                    | BuildingArchetype::FachwerkCottage
                    | BuildingArchetype::FachwerkMerchantHouse
            ) {
                crate::OpeningHeadKind::TimberLintel
            } else {
                crate::OpeningHeadKind::StoneLintel
            },
        ),
        OpeningKind::Gate => (
            crate::OpeningUse::Gate,
            crate::OpeningProfile::Segmental {
                width_metres: opening.width_metres,
                spring_height_metres: (opening.height_metres - 0.28).max(1.8),
                rise_metres: 0.28,
                intrados_depth_metres: 0.24,
            },
            crate::OpeningHeadKind::SegmentalArch,
        ),
        OpeningKind::Window if archetype == BuildingArchetype::Cathedral => (
            crate::OpeningUse::Window,
            crate::OpeningProfile::PointedTwoCentred {
                width_metres: 1.12,
                spring_height_metres: 3.0,
                apex_height_metres: 4.55,
                arc_radius_metres: two_centred_arc_radius(1.12, 4.55 - 3.0),
            },
            crate::OpeningHeadKind::PointedVoussoir,
        ),
        OpeningKind::Window if archetype == BuildingArchetype::RenaissanceTownHall => (
            crate::OpeningUse::Window,
            crate::OpeningProfile::Segmental {
                width_metres: 0.95,
                spring_height_metres: 1.0,
                rise_metres: 0.28,
                intrados_depth_metres: 0.18,
            },
            crate::OpeningHeadKind::SegmentalArch,
        ),
        OpeningKind::Window => (
            crate::OpeningUse::Window,
            crate::OpeningProfile::Rectangular {
                width_metres: opening.width_metres,
                height_metres: opening.height_metres,
            },
            crate::OpeningHeadKind::TimberLintel,
        ),
        OpeningKind::ArrowSlit
            if matches!(
                archetype,
                BuildingArchetype::WalledKeep | BuildingArchetype::ArtilleryRondelCastle
            ) =>
        {
            (
                crate::OpeningUse::GunLoop,
                crate::OpeningProfile::GunLoop {
                    exterior_width_metres: 0.20,
                    interior_width_metres: 0.92,
                    exterior_height_metres: 0.48,
                    interior_height_metres: 1.10,
                    mount: crate::WeaponMountClass::LightArquebus,
                    traverse_degrees: 28.0,
                    recoil_metres: 0.85,
                    crew_clearance_metres: 1.25,
                },
                crate::OpeningHeadKind::StoneLintel,
            )
        }
        OpeningKind::ArrowSlit => (
            crate::OpeningUse::ArrowLoop,
            crate::OpeningProfile::ArrowLoop {
                exterior_width_metres: 0.14,
                interior_width_metres: 0.68,
                exterior_height_metres: opening.height_metres,
                interior_height_metres: 1.18,
            },
            crate::OpeningHeadKind::StoneLintel,
        ),
    }
}

fn wall_solid(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    centre: Vec3,
    size: Vec3,
    role: SolidRole,
    shape: crate::ResolvedSolidShape,
    support: StructuralNodeId,
) -> ResolvedItemId {
    let id = ResolvedItemId((1_u64 << 60) | (u64::from(owner.0) << 32) | slot);
    geometry.solids.push(ResolvedSolid {
        id,
        owner,
        centre,
        size,
        yaw_radians: 0.0,
        crossfall_radians: 0.0,
        longfall_radians: 0.0,
        role,
        shape,
        supported_by: vec![support],
    });
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot),
        owner,
        node: support,
        bounds: ResolvedBounds {
            min: Vec3::new(
                centre.x - size.x * 0.5,
                centre.y - size.y * 0.5 - 0.015,
                centre.z - size.z * 0.5,
            ),
            max: Vec3::new(
                centre.x + size.x * 0.5,
                centre.y - size.y * 0.5 + 0.015,
                centre.z + size.z * 0.5,
            ),
        },
    });
    id
}

fn wall_void(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    bounds: ResolvedBounds,
    opening: crate::OpeningAssemblyId,
    exterior_width_metres: f32,
    interior_width_metres: f32,
    exterior_height_metres: f32,
    interior_height_metres: f32,
    exterior_depth_sign: i8,
) -> ResolvedItemId {
    let id = ResolvedItemId((3_u64 << 60) | (u64::from(owner.0) << 32) | slot);
    geometry.voids.push(ResolvedVoid {
        id,
        owner,
        bounds,
        role: VoidRole::WallOpening,
        shape: crate::ResolvedVoidShape::SectionalOpening {
            opening,
            exterior_width_metres,
            interior_width_metres,
            exterior_height_metres,
            interior_height_metres,
            exterior_depth_sign,
        },
        subtracts_from: owner,
    });
    id
}

fn wall_shaped_surface(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    bounds: ResolvedBounds,
    role: SurfaceRole,
    shape: crate::ResolvedSurfaceShape,
) -> ResolvedItemId {
    let id = wall_surface(geometry, owner, slot, bounds, role);
    geometry
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == id)
        .expect("new wall surface")
        .shape = shape;
    id
}

fn wall_surface(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    bounds: ResolvedBounds,
    role: SurfaceRole,
) -> ResolvedItemId {
    let id = ResolvedItemId((9_u64 << 60) | (u64::from(owner.0) << 32) | slot);
    geometry.surfaces.push(ResolvedSurface {
        id,
        owner,
        bounds,
        role,
        shape: crate::ResolvedSurfaceShape::Planar,
    });
    id
}
