/// Resolve the single artillery-adapted MVP as one authority.  The exact
/// dimensions below are project animation/gameplay gates, not universal
/// measurements for sixteenth-century fortifications.
fn resolve_artillery_castle(
    program: &BuildingProgram,
    towers: &[RoundTower],
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) -> Option<crate::ArtilleryCastleAssembly> {
    if program.archetype != BuildingArchetype::ArtilleryRondelCastle {
        return None;
    }
    let trace = [
        crate::GridPoint::new(-240, -180),
        crate::GridPoint::new(480, -180),
        crate::GridPoint::new(480, 420),
        crate::GridPoint::new(-240, 420),
    ];
    let crown = 6.0_f32;
    let total_depth = crate::GridLength::new(90).expect("4.5m artillery curtain depth");
    let mut curtains = Vec::new();
    let mut support_ids = Vec::new();
    let mut artillery_drainage_routes = Vec::new();
    let curtain_specs = [
        (crate::Direction::South, Vec2::new(6.0, -9.0), 34.8_f32),
        (crate::Direction::East, Vec2::new(24.0, 6.0), 28.8),
        (crate::Direction::North, Vec2::new(6.0, 21.0), 34.8),
        (crate::Direction::West, Vec2::new(-12.0, 6.0), 28.8),
    ];
    for (index, (direction, inner_mid, length)) in curtain_specs.into_iter().enumerate() {
        let owner = GeometryOwnerId(80_000 + index as u32);
        let outward = direction_vector(direction);
        let tangent = Vec2::new(-outward.y, outward.x);
        let revetment_node = StructuralNodeId(40_000_000 + index as u64 * 4);
        let retaining_node = StructuralNodeId(revetment_node.0 + 1);
        let terreplein_node = StructuralNodeId(revetment_node.0 + 2);
        geometry.structural_nodes.extend([
            StructuralNode {
                id: revetment_node,
                owner,
                kind: StructuralNodeKind::ArtilleryRevetmentBearing,
                position: Vec3::new(
                    inner_mid.x + outward.x * 4.05,
                    0.0,
                    inner_mid.y + outward.y * 4.05,
                ),
                supported_by: Vec::new(),
                grounded: true,
            },
            StructuralNode {
                id: retaining_node,
                owner,
                kind: StructuralNodeKind::ArtilleryRetainingBearing,
                position: Vec3::new(
                    inner_mid.x + outward.x * 0.25,
                    0.0,
                    inner_mid.y + outward.y * 0.25,
                ),
                supported_by: Vec::new(),
                grounded: true,
            },
            StructuralNode {
                id: terreplein_node,
                owner,
                kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
                position: Vec3::new(
                    inner_mid.x + outward.x * 2.25,
                    5.55,
                    inner_mid.y + outward.y * 2.25,
                ),
                supported_by: vec![revetment_node, retaining_node],
                grounded: false,
            },
        ]);
        let rev_plan = inner_mid + outward * 4.05;
        let earth_plan = inner_mid + outward * 2.25;
        let retain_plan = inner_mid + outward * 0.25;
        let split_layer = |geometry: &mut ResolvedGeometry,
                           plan: Vec2,
                           depth: f32,
                           height: f32,
                           role: SolidRole,
                           supports: Vec<StructuralNodeId>| {
            if direction == crate::Direction::South {
                [-3.5_f32, 15.5]
                    .into_iter()
                    .map(|x| {
                        projected_solid(
                            geometry,
                            owner,
                            Vec3::new(x, height * 0.5, plan.y),
                            Vec3::new(15.8, height, depth),
                            0.0,
                            role,
                            supports.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![projected_solid(
                    geometry,
                    owner,
                    Vec3::new(plan.x, height * 0.5, plan.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(length, height, depth)
                    } else {
                        Vec3::new(depth, height, length)
                    },
                    0.0,
                    role,
                    supports,
                )]
            }
        };
        let revetments = split_layer(
            geometry,
            rev_plan,
            0.9,
            crown,
            SolidRole::ArtilleryRevetment,
            vec![revetment_node],
        );
        let earths = split_layer(
            geometry,
            earth_plan,
            3.1,
            5.5,
            SolidRole::ArtilleryEarthCore,
            vec![revetment_node, retaining_node],
        );
        let retainings = split_layer(
            geometry,
            retain_plan,
            0.5,
            crown,
            SolidRole::ArtilleryRetainingWall,
            vec![retaining_node],
        );
        let deck_plan = inner_mid + outward * 1.95;
        let terreplein = projected_solid(
            geometry,
            owner,
            Vec3::new(deck_plan.x, 5.74, deck_plan.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(length, 0.22, 3.10)
            } else {
                Vec3::new(3.10, 0.22, length)
            },
            0.0,
            SolidRole::ArtilleryTerreplein,
            vec![terreplein_node],
        );
        let yaw = -tangent.y.atan2(tangent.x);
        let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
        if let Some(deck) = geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == terreplein)
        {
            deck.yaw_radians = yaw;
            deck.size = Vec3::new(length, 0.22, 3.10);
            deck.crossfall_radians = 0.025 * outward.dot(local_positive_z).signum();
        }
        let parapet_plan = inner_mid + outward * 4.02;
        let parapet = projected_solid(
            geometry,
            owner,
            Vec3::new(parapet_plan.x, 6.65, parapet_plan.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(length, 1.3, 0.95)
            } else {
                Vec3::new(0.95, 1.3, length)
            },
            0.0,
            SolidRole::ArtilleryParapet,
            vec![revetment_node],
        );
        if let Some(solid) = geometry.solids.iter_mut().find(|solid| solid.id == parapet) {
            if tangent.x.abs() > 0.5 {
                solid.size.x -= 1.8;
            } else {
                solid.size.z -= 1.8;
            }
        }
        let route_surface = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    deck_plan.x - tangent.x.abs() * length * 0.5 - outward.x.abs() * 1.25,
                    5.85,
                    deck_plan.y - tangent.y.abs() * length * 0.5 - outward.y.abs() * 1.25,
                ),
                max: Vec3::new(
                    deck_plan.x + tangent.x.abs() * length * 0.5 + outward.x.abs() * 1.25,
                    5.88,
                    deck_plan.y + tangent.y.abs() * length * 0.5 + outward.y.abs() * 1.25,
                ),
            },
            SurfaceRole::ArtilleryRoute,
        );
        let catchment = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    deck_plan.x - tangent.x.abs() * length * 0.5 - outward.x.abs() * 1.65,
                    5.84,
                    deck_plan.y - tangent.y.abs() * length * 0.5 - outward.y.abs() * 1.65,
                ),
                max: Vec3::new(
                    deck_plan.x + tangent.x.abs() * length * 0.5 + outward.x.abs() * 1.65,
                    5.87,
                    deck_plan.y + tangent.y.abs() * length * 0.5 + outward.y.abs() * 1.65,
                ),
            },
            SurfaceRole::ArtilleryDrainage,
        );
        let channel_plan = inner_mid + outward * 3.55;
        let channel = projected_solid(
            geometry,
            owner,
            Vec3::new(channel_plan.x, 5.595, channel_plan.y),
            Vec3::new(length, 0.05, 0.10),
            yaw,
            SolidRole::DrainageFloor,
            vec![terreplein_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .expect("artillery curtain gutter")
            .longfall_radians = 0.010;
        let inlet_plan = channel_plan - tangent * (length * 0.5 - 0.08);
        let route = projected_edge_drain(
            geometry,
            owner,
            Vec3::new(inlet_plan.x, 5.57, inlet_plan.y),
            outward,
        );
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment,
            owner,
            walk_solid: terreplein,
            toe_channel_solids: vec![channel],
            drainage_surface: catchment,
            outlet_route: route,
            centre: Vec3::new(deck_plan.x, 5.85, deck_plan.y),
            tangent,
            outward,
            length_metres: length,
            width_metres: 3.10,
            inner_elevation_metres: 5.88,
            outer_elevation_metres: 5.82,
            outlet_along_metres: -length * 0.5 + 0.08,
        });
        artillery_drainage_routes.push(route);
        let (inner_start, inner_end) = if tangent.x.abs() > 0.5 {
            (
                crate::GridPoint::new(-240, (inner_mid.y / crate::GRID_UNIT_METRES) as i32),
                crate::GridPoint::new(480, (inner_mid.y / crate::GRID_UNIT_METRES) as i32),
            )
        } else {
            (
                crate::GridPoint::new((inner_mid.x / crate::GRID_UNIT_METRES) as i32, -180),
                crate::GridPoint::new((inner_mid.x / crate::GRID_UNIT_METRES) as i32, 420),
            )
        };
        curtains.push(crate::ArtilleryCurtainAssembly {
            id: crate::ArtilleryCurtainId(index as u64),
            owner,
            outward: direction,
            inner_start,
            inner_end,
            total_depth,
            height_metres: crown,
            revetment_solids: revetments,
            earth_solids: earths,
            retaining_solids: retainings,
            terreplein_solid: terreplein,
            parapet_solid: parapet,
            route_surface,
            drainage_catchment: catchment,
            drainage_route: route,
            suppressed_source_walls: Vec::new(),
        });
        support_ids.extend(
            geometry
                .support_interfaces
                .iter()
                .rev()
                .take(5)
                .map(|interface| interface.id),
        );
    }

    let mut rondels = Vec::new();
    let mut stations = Vec::new();
    for (index, tower) in towers.iter().take(4).enumerate() {
        let owner = GeometryOwnerId(60_000 + index as u32);
        let centre = tower.centre_metres();
        let bearing = StructuralNodeId(41_000_000 + index as u64 * 3);
        let deck_node = StructuralNodeId(bearing.0 + 1);
        geometry.structural_nodes.extend([
            StructuralNode {
                id: bearing,
                owner,
                kind: StructuralNodeKind::ArtilleryRondelBearing,
                position: Vec3::new(centre.x, 0.0, centre.y),
                supported_by: Vec::new(),
                grounded: true,
            },
            StructuralNode {
                id: deck_node,
                owner,
                kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
                position: Vec3::new(centre.x, 5.55, centre.y),
                supported_by: vec![bearing],
                grounded: false,
            },
        ]);
        let casemate_void = projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(centre.x - 2.5, 0.20, centre.y - 2.5),
                max: Vec3::new(centre.x + 2.5, 2.75, centre.y + 2.5),
            },
            VoidRole::ArtilleryCasemate,
        );
        let floor = projected_solid(
            geometry,
            owner,
            Vec3::new(centre.x, 0.10, centre.y),
            Vec3::new(5.0, 0.20, 5.0),
            0.0,
            SolidRole::ArtilleryCasemateFloor,
            vec![bearing],
        );
        let roof = projected_solid(
            geometry,
            owner,
            Vec3::new(centre.x, 2.90, centre.y),
            Vec3::new(5.2, 0.30, 5.2),
            0.0,
            SolidRole::ArtilleryCasemateRoof,
            vec![bearing],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == roof)
            .unwrap()
            .shape = crate::ResolvedSolidShape::AnnularPrism {
            inner_radius_metres: 1.10,
            outer_radius_metres: 2.60,
            inner_top_offset_metres: 0.0,
            outer_top_offset_metres: 0.0,
            drainage_outlet_count: 0,
            circumferential_fall_metres: 0.0,
        };
        // The low battery is a genuine earth-backed rondel. The residual
        // sectors are resolved after the station working volumes exist, so
        // the actual port, stance, mount, recoil, vent, and access authority
        // determines every omission rather than a nominal angular slot.
        let inward = Vec2::new(
            if index % 2 == 0 { 1.0 } else { -1.0 },
            if index < 2 { 1.0 } else { -1.0 },
        );
        let mut earths = Vec::new();
        let terreplein = projected_solid(
            geometry,
            owner,
            Vec3::new(centre.x, 5.72, centre.y),
            Vec3::new(9.60, 0.24, 9.60),
            0.0,
            SolidRole::ArtilleryTerreplein,
            vec![deck_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == terreplein)
            .expect("rondel annular deck")
            .shape = crate::ResolvedSolidShape::AnnularPrism {
            inner_radius_metres: 1.10,
            outer_radius_metres: 4.80,
            inner_top_offset_metres: 0.035,
            outer_top_offset_metres: -0.035,
            drainage_outlet_count: 4,
            circumferential_fall_metres: 0.025,
        };
        let route = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(centre.x - 3.5, 5.84, centre.y - 3.5),
                max: Vec3::new(centre.x + 3.5, 5.87, centre.y + 3.5),
            },
            SurfaceRole::ArtilleryRoute,
        );
        let mut rondel_drainage = Vec::new();
        for (drain_index, outward) in [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y]
            .into_iter()
            .enumerate()
        {
            let tangent = Vec2::new(-outward.y, outward.x);
            let channel_plan = centre + outward * 4.83;
            let channels = [-1.0_f32, 1.0]
                .into_iter()
                .map(|side| {
                    let plan = channel_plan + tangent * side * 0.46;
                    let channel = projected_solid(
                        geometry,
                        owner,
                        Vec3::new(plan.x, 5.645, plan.y),
                        Vec3::new(0.92, 0.05, 0.10),
                        -tangent.y.atan2(tangent.x),
                        SolidRole::DrainageFloor,
                        vec![deck_node],
                    );
                    geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == channel)
                        .expect("rondel V gutter half")
                        .longfall_radians = side * 0.015;
                    channel
                })
                .collect::<Vec<_>>();
            let drain_surface = projected_surface(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(centre.x - 4.8, 5.64, centre.y - 4.8),
                    max: Vec3::new(centre.x + 4.8, 5.88, centre.y + 4.8),
                },
                SurfaceRole::ArtilleryDrainage,
            );
            let route_id = projected_edge_drain(
                geometry,
                owner,
                Vec3::new(channel_plan.x, 5.61, channel_plan.y),
                outward,
            );
            geometry.drainage_catchments.push(DrainageCatchment {
                id: drain_surface,
                owner,
                walk_solid: terreplein,
                toe_channel_solids: channels,
                drainage_surface: drain_surface,
                outlet_route: route_id,
                centre: Vec3::new(centre.x, 5.84, centre.y),
                tangent,
                outward,
                length_metres: std::f32::consts::PI * 4.8 * 0.5,
                width_metres: 3.7,
                inner_elevation_metres: 5.875,
                outer_elevation_metres: 5.805,
                outlet_along_metres: drain_index as f32,
            });
            rondel_drainage.push(route_id);
            artillery_drainage_routes.push(route_id);
        }
        let adjoining = match index {
            0 => [crate::ArtilleryCurtainId(0), crate::ArtilleryCurtainId(3)],
            1 => [crate::ArtilleryCurtainId(0), crate::ArtilleryCurtainId(1)],
            2 => [crate::ArtilleryCurtainId(2), crate::ArtilleryCurtainId(3)],
            _ => [crate::ArtilleryCurtainId(2), crate::ArtilleryCurtainId(1)],
        };
        let mut bonds = [ResolvedItemId::default(); 2];
        for bond_index in 0..2 {
            let interface = tower
                .chord_interfaces()
                .nth(bond_index)
                .expect("two artillery returns");
            let toward = direction_vector(interface.toward_gate);
            let bond_centre =
                centre + toward * (tower.radius_metres() - interface.bearing_depth.metres() * 0.5);
            let id = ResolvedItemId((7_u64 << 60) | (u64::from(owner.0) << 24) | bond_index as u64);
            geometry.junction_bonds.push(JunctionBond {
                id,
                owners: [owner, curtains[adjoining[bond_index].0 as usize].owner],
                bounds: ResolvedBounds {
                    min: Vec3::new(bond_centre.x - 1.25, 0.0, bond_centre.y - 1.25),
                    max: Vec3::new(bond_centre.x + 1.25, crown + 0.25, bond_centre.y + 1.25),
                },
                minimum_interface_area_square_metres: 0.25,
                maximum_penetration_metres: 1.25,
            });
            bonds[bond_index] = id;
        }
        let mut station_ids = Vec::new();
        let outward_y = if index < 2 { -1.0 } else { 1.0 };
        // The covered batteries fire tangentially along the two adjoining
        // curtain feet. The open upper position covers the outward ditch.
        let facings = [
            Vec2::new(inward.x, 0.0),
            Vec2::new(0.0, inward.y),
            Vec2::new(0.0, outward_y),
        ];
        for (station_index, facing) in facings.into_iter().enumerate() {
            let level = if station_index < 2 {
                crate::ArtilleryStationLevel::LowerCasemate
            } else {
                crate::ArtilleryStationLevel::UpperTerreplein
            };
            let opening_id =
                crate::OpeningAssemblyId(90_000 + index as u64 * 3 + station_index as u64);
            let wall_index = walls.iter().position(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { tower_index } if tower_index == index)).expect("artillery rondel radial host");
            let mut station_wall = walls[wall_index].clone();
            station_wall.id =
                crate::WallAssemblyId(90_000 + index as u64 * 3 + station_index as u64);
            station_wall.source = crate::WallSourceId::ArtilleryRondel {
                rondel_index: index,
                station_index,
            };
            station_wall.owner = GeometryOwnerId(83_000 + (index * 3 + station_index) as u32);
            station_wall.length_metres = 1.50;
            station_wall.radial_frame = Some(crate::RadialWallFrame {
                centre: tower.centre_metres(),
                reference_outward: facing,
            });
            station_wall.opening_ids.clear();
            station_wall.host_solids.clear();
            let station = resolve_artillery_gun_opening(
                index,
                station_index,
                facing,
                level,
                opening_id,
                &mut station_wall,
                openings,
                geometry,
            );
            let resolved_opening = openings.last().expect("artillery opening");
            station_wall.host_solids = resolved_opening
                .jamb_solids
                .iter()
                .copied()
                .chain([resolved_opening.head_solid, resolved_opening.spandrel_solid])
                .collect();
            walls.push(station_wall);
            let aperture_origin = openings.last().unwrap().frame.origin;
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId(
                    (7_u64 << 60) | (u64::from(owner.0) << 20) | (0x100 + station_index as u64),
                ),
                owners: [
                    owner,
                    GeometryOwnerId(83_000 + (index * 3 + station_index) as u32),
                ],
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        aperture_origin.x - 0.9,
                        floor_for_artillery_level(level),
                        aperture_origin.y - 0.9,
                    ),
                    max: Vec3::new(
                        aperture_origin.x + 0.9,
                        floor_for_artillery_level(level) + 2.5,
                        aperture_origin.y + 0.9,
                    ),
                },
                minimum_interface_area_square_metres: 0.05,
                maximum_penetration_metres: 1.25,
            });
            if station_index < 2 {
                geometry.junction_bonds.push(JunctionBond {
                    id: ResolvedItemId(
                        (7_u64 << 60) | (u64::from(owner.0) << 20) | (0x180 + station_index as u64),
                    ),
                    owners: [
                        curtains[adjoining[station_index].0 as usize].owner,
                        GeometryOwnerId(83_000 + (index * 3 + station_index) as u32),
                    ],
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            aperture_origin.x - 1.25,
                            floor_for_artillery_level(level),
                            aperture_origin.y - 1.25,
                        ),
                        max: Vec3::new(
                            aperture_origin.x + 1.25,
                            floor_for_artillery_level(level) + 2.55,
                            aperture_origin.y + 1.25,
                        ),
                    },
                    minimum_interface_area_square_metres: 0.08,
                    maximum_penetration_metres: 1.25,
                });
            }
            station_ids.push(station.id);
            stations.push(station);
        }
        let mut earth_clearances = vec![
            geometry
                .voids
                .iter()
                .find(|void| void.id == casemate_void)
                .expect("rondel casemate void")
                .bounds,
        ];
        for station in stations.iter().filter(|station| {
            station.rondel == crate::ArtilleryRondelId(index as u64)
                && station.level == crate::ArtilleryStationLevel::LowerCasemate
        }) {
            earth_clearances.push(station.recoil_envelope);
            if let Some(stance) = geometry
                .surfaces
                .iter()
                .find(|surface| surface.id == station.stance_surface)
            {
                earth_clearances.push(ResolvedBounds {
                    min: stance.bounds.min - Vec3::new(0.02, 0.0, 0.02),
                    max: stance.bounds.max + Vec3::new(0.02, 1.90, 0.02),
                });
            }
            if let Some(mount) = geometry
                .solids
                .iter()
                .find(|solid| solid.id == station.mount_solid)
            {
                earth_clearances.push(ResolvedBounds {
                    min: mount.centre - mount.size * 0.5,
                    max: mount.centre + mount.size * 0.5,
                });
            }
            if let Some(opening) = openings
                .iter()
                .find(|opening| opening.id == station.opening)
                && let Some(void) = geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
            {
                earth_clearances.push(void.bounds);
            }
            if let Some(vent) = station.smoke_vent
                && let Some(void) = geometry.voids.iter().find(|void| void.id == vent)
            {
                earth_clearances.push(void.bounds);
            }
        }
        let sector_bounds = |start: f32, end: f32| {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for angle in [start, (start + end) * 0.5, end] {
                for radius in [3.60_f32, 4.775] {
                    let point = Vec3::new(
                        centre.x + radius * angle.cos(),
                        0.0,
                        centre.y + radius * angle.sin(),
                    );
                    min = min.min(point);
                    max = max.max(point + Vec3::Y * 5.50);
                }
            }
            ResolvedBounds { min, max }
        };
        for sector in 0..32 {
            let start = sector as f32 * std::f32::consts::TAU / 32.0;
            let end = (sector + 1) as f32 * std::f32::consts::TAU / 32.0;
            let bounds = sector_bounds(start, end);
            let reserved = earth_clearances.iter().any(|clearance| {
                bounds.max.x.min(clearance.max.x) - bounds.min.x.max(clearance.min.x) > 0.005
                    && bounds.max.y.min(clearance.max.y) - bounds.min.y.max(clearance.min.y) > 0.005
                    && bounds.max.z.min(clearance.max.z) - bounds.min.z.max(clearance.min.z) > 0.005
            });
            if reserved {
                continue;
            }
            let id = projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, 2.75, centre.y),
                Vec3::new(9.55, 5.50, 9.55),
                0.0,
                SolidRole::ArtilleryEarthCore,
                vec![bearing],
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .expect("rondel residual earth sector")
                .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres: 3.60,
                outer_radius_metres: 4.775,
                start_angle_radians: start,
                end_angle_radians: end,
                inner_top_offset_metres: 0.0,
                outer_top_offset_metres: 0.0,
            };
            earths.push(id);
        }
        let shell = walls.iter().find(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { tower_index } if tower_index == index)).and_then(|wall| wall.host_solids.first()).copied().expect("rondel shell");
        let mut stair_solids = Vec::new();
        let stair_arrival_angle = inward.y.atan2(inward.x).rem_euclid(std::f32::consts::TAU);
        for tread in 0..32_u16 {
            let progress = f32::from(tread) / 31.0;
            let angle = stair_arrival_angle + progress * std::f32::consts::TAU * 2.0;
            let radial = Vec2::new(angle.cos(), angle.sin());
            let tread_plan = centre + radial * 0.65;
            let tread_solid = projected_solid(
                geometry,
                owner,
                Vec3::new(tread_plan.x, 0.22 + progress * 5.58, tread_plan.y),
                Vec3::new(0.90, 0.12, 0.38),
                -radial.y.atan2(radial.x),
                SolidRole::ArtilleryStairTread,
                vec![bearing],
            );
            stair_solids.push(tread_solid);
        }
        let mut parapet_solids = Vec::new();
        let access_angles = tower
            .chord_interfaces()
            .map(|interface| {
                let toward = direction_vector(interface.toward_gate);
                toward.y.atan2(toward.x).rem_euclid(std::f32::consts::TAU)
            })
            .collect::<Vec<_>>();
        let firing_angle = Vec2::new(0.0, outward_y)
            .y
            .atan2(0.0_f32)
            .rem_euclid(std::f32::consts::TAU);
        for sector in 0..32 {
            let start = sector as f32 * std::f32::consts::TAU / 32.0;
            let end = (sector + 1) as f32 * std::f32::consts::TAU / 32.0;
            let middle = (start + end) * 0.5;
            let angular_distance = |angle: f32| {
                (middle - angle)
                    .rem_euclid(std::f32::consts::TAU)
                    .min((angle - middle).rem_euclid(std::f32::consts::TAU))
            };
            if access_angles
                .iter()
                .any(|angle| angular_distance(*angle) < 0.50)
                || angular_distance(firing_angle) < 0.14
            {
                continue;
            }
            let id = projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, 6.475, centre.y),
                Vec3::new(11.7, 1.25, 11.7),
                0.0,
                SolidRole::ArtilleryParapet,
                vec![deck_node],
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .unwrap()
                .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres: 5.00,
                outer_radius_metres: 5.85,
                start_angle_radians: start,
                end_angle_radians: end,
                inner_top_offset_metres: 0.04,
                outer_top_offset_metres: -0.04,
            };
            parapet_solids.push(id);
        }
        let mut stair_guard_solids = Vec::new();
        let guard_sector_angle = std::f32::consts::TAU / 32.0;
        let arrival_half_angle = (0.45_f32 / 1.30).asin();
        for sector in 0..32 {
            let start = sector as f32 * guard_sector_angle;
            let end = (sector + 1) as f32 * guard_sector_angle;
            let middle = (start + end) * 0.5;
            let arrival_distance = (middle - stair_arrival_angle)
                .rem_euclid(std::f32::consts::TAU)
                .min((stair_arrival_angle - middle).rem_euclid(std::f32::consts::TAU));
            // The 1.10m stair well gets a continuous 0.95m guard. The only
            // omitted sectors are exactly those intersecting the positive-
            // width 0.90 m occupant sweep at the authoritative tread arrival.
            // Half a sector is included because this discrete authority omits
            // complete annular sectors, never partial visual-only wedges.
            if arrival_distance < arrival_half_angle + guard_sector_angle * 0.5 {
                continue;
            }
            let id = projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, 6.335, centre.y),
                Vec3::new(2.86, 0.95, 2.86),
                0.0,
                SolidRole::ArtilleryStairGuard,
                vec![deck_node],
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .expect("rondel stair-well guard")
                .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres: 1.30,
                outer_radius_metres: 1.43,
                start_angle_radians: start,
                end_angle_radians: end,
                inner_top_offset_metres: 0.0,
                outer_top_offset_metres: -0.02,
            };
            stair_guard_solids.push(id);
        }
        rondels.push(crate::ArtilleryRondelAssembly {
            id: crate::ArtilleryRondelId(index as u64),
            owner,
            anchor: tower.anchor(),
            diameter: tower.diameter(),
            shell: crate::GridLength::new(
                (tower.wall_thickness_metres / crate::GRID_UNIT_METRES).round() as i32,
            )
            .expect("shell"),
            adjoining_curtains: adjoining,
            curtain_bonds: bonds,
            shell_solid: shell,
            earth_solids: earths,
            casemate_void,
            casemate_floor: floor,
            casemate_roof: roof,
            terreplein_solid: terreplein,
            parapet_solids,
            stair_guard_solids,
            route_surfaces: vec![route],
            stair_solids,
            drainage_routes: rondel_drainage,
            station_ids,
            support_nodes: vec![bearing, deck_node],
        });
    }

    // Stable tactical targets turn the firing proof into a coverage matrix.
    // Auxiliary targets preserve each station's near/middle/far calibration;
    // required targets sample every curtain foot, ditch corner, and approach.
    let mut defense_targets = Vec::new();
    let mut next_target = 0_u64;
    for station in &mut stations {
        for ray in &mut station.rays {
            let id = crate::ArtilleryTargetId(next_target);
            next_target += 1;
            ray.target_id = id;
            defense_targets.push(crate::ArtilleryDefenseTarget {
                id,
                kind: ray.target_kind,
                centre: ray.target,
                half_extent_metres: Vec2::splat(0.35),
                required_independent_stations: 0,
            });
        }
    }
    let mut required = Vec::new();
    for (kind, points, independent) in [
        (
            crate::ArtilleryTargetKind::CurtainFoot,
            vec![
                Vec3::new(-8.0, 0.2, -13.5),
                Vec3::new(6.0, 0.2, -13.5),
                Vec3::new(20.0, 0.2, -13.5),
                Vec3::new(28.5, 0.2, -4.0),
                Vec3::new(28.5, 0.2, 6.0),
                Vec3::new(28.5, 0.2, 16.0),
                Vec3::new(-8.0, 0.2, 25.5),
                Vec3::new(6.0, 0.2, 25.5),
                Vec3::new(20.0, 0.2, 25.5),
                Vec3::new(-16.5, 0.2, -4.0),
                Vec3::new(-16.5, 0.2, 6.0),
                Vec3::new(-16.5, 0.2, 16.0),
            ],
            2_u8,
        ),
        (
            crate::ArtilleryTargetKind::DitchCorner,
            vec![
                Vec3::new(-20.0, -1.0, -17.0),
                Vec3::new(32.0, -1.0, -17.0),
                Vec3::new(-20.0, -1.0, 29.0),
                Vec3::new(32.0, -1.0, 29.0),
            ],
            1,
        ),
        (
            crate::ArtilleryTargetKind::GateThreshold,
            vec![Vec3::new(6.0, 0.2, -13.5)],
            2,
        ),
        (
            crate::ArtilleryTargetKind::Bridge,
            vec![Vec3::new(6.0, 0.2, -17.0)],
            2,
        ),
        (
            crate::ArtilleryTargetKind::Approach,
            vec![Vec3::new(6.0, 0.2, -25.0)],
            2,
        ),
    ] {
        for point in points {
            let id = crate::ArtilleryTargetId(next_target);
            next_target += 1;
            defense_targets.push(crate::ArtilleryDefenseTarget {
                id,
                kind,
                centre: point,
                half_extent_metres: Vec2::splat(0.45),
                required_independent_stations: independent,
            });
            required.push(id);
        }
    }
    for target_id in required {
        let target = defense_targets
            .iter()
            .find(|target| target.id == target_id)
            .unwrap()
            .clone();
        let mut candidates = stations
            .iter()
            .enumerate()
            .filter_map(|(index, station)| {
                let origin = station.rays.first()?.origin;
                let delta = Vec2::new(target.centre.x - origin.x, target.centre.z - origin.z);
                let distance = delta.length();
                (distance > 2.0
                    && station.facing.dot(delta / distance) >= 38.0_f32.to_radians().cos() - 0.01)
                    .then_some((distance, index))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (distance, index) in candidates
            .into_iter()
            .take(target.required_independent_stations as usize)
        {
            let origin = stations[index].rays[0].origin;
            let range = if distance < 8.0 {
                crate::ProjectedDefenseRange::Near
            } else if distance < 18.0 {
                crate::ProjectedDefenseRange::Middle
            } else {
                crate::ProjectedDefenseRange::Far
            };
            stations[index].rays.push(crate::ArtilleryFireRay {
                target_id,
                origin,
                target: target.centre,
                target_kind: target.kind,
                range,
            });
        }
    }

    let ditch_owner = GeometryOwnerId(82_000);
    let ditch_node = StructuralNodeId(42_000_000);
    geometry.structural_nodes.push(StructuralNode {
        id: ditch_node,
        owner: ditch_owner,
        kind: StructuralNodeKind::ArtilleryRevetmentBearing,
        position: Vec3::new(6.0, -2.2, 6.0),
        supported_by: Vec::new(),
        grounded: true,
    });
    let ditch_void = projected_void(
        geometry,
        ditch_owner,
        ResolvedBounds {
            min: Vec3::new(-22.5, -2.19, -19.5),
            max: Vec3::new(34.5, 0.0, 31.5),
        },
        VoidRole::DryDitch,
    );
    geometry
        .voids
        .iter_mut()
        .find(|void| void.id == ditch_void)
        .unwrap()
        .shape = crate::ResolvedVoidShape::RectangularRing {
        inner_min: Vec2::new(-16.7, -13.5),
        inner_max: Vec2::new(28.7, 25.5),
    };
    let mut floors = Vec::new();
    for (centre, tangent, length) in [
        (Vec3::new(6.0, -2.3, -17.0), Vec2::X, 57.0_f32),
        (Vec3::new(6.0, -2.3, 29.0), Vec2::X, 57.0),
        (Vec3::new(-20.0, -2.3, 6.0), Vec2::Y, 41.0),
        (Vec3::new(32.0, -2.3, 6.0), Vec2::Y, 41.0),
    ] {
        let floor = projected_solid(
            geometry,
            ditch_owner,
            centre,
            Vec3::new(length, 0.20, 5.0),
            -tangent.y.atan2(tangent.x),
            SolidRole::DitchFloor,
            vec![ditch_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor)
            .expect("sloped ditch floor")
            .longfall_radians = 0.004;
        floors.push(floor);
    }
    let mut scarp_solids = Vec::new();
    let mut counterscarp_solids = Vec::new();
    for (centre, size, yaw) in [
        // The south scarp is split around the grounded bridge abutment.
        (
            Vec3::new(-6.10, -1.15, -13.67),
            Vec3::new(20.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(18.10, -1.15, -13.67),
            Vec3::new(20.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(6.0, -1.15, 25.67),
            Vec3::new(45.0, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(-16.67, -1.15, 6.0),
            Vec3::new(0.30, 2.30, 34.0),
            0.0,
        ),
        (
            Vec3::new(28.67, -1.15, 6.0),
            Vec3::new(0.30, 2.30, 34.0),
            0.0,
        ),
    ] {
        scarp_solids.push(projected_solid(
            geometry,
            ditch_owner,
            centre,
            size,
            yaw,
            SolidRole::DitchScarp,
            vec![ditch_node],
        ));
    }
    for (centre, size, yaw) in [
        (
            Vec3::new(-9.10, -1.15, -19.35),
            Vec3::new(26.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(21.10, -1.15, -19.35),
            Vec3::new(26.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(6.0, -1.15, 31.35),
            Vec3::new(57.0, 2.30, 0.35),
            0.0,
        ),
        (
            Vec3::new(-22.35, -1.15, 6.0),
            Vec3::new(0.35, 2.30, 51.0),
            0.0,
        ),
        (
            Vec3::new(34.35, -1.15, 6.0),
            Vec3::new(0.35, 2.30, 51.0),
            0.0,
        ),
    ] {
        counterscarp_solids.push(projected_solid(
            geometry,
            ditch_owner,
            centre,
            size,
            yaw,
            SolidRole::DitchCounterscarp,
            vec![ditch_node],
        ));
    }
    let ditch_outlet = projected_surface(
        geometry,
        ditch_owner,
        ResolvedBounds {
            min: Vec3::new(31.5, -2.31, 28.5),
            max: Vec3::new(32.5, -2.27, 29.5),
        },
        SurfaceRole::DitchSplash,
    );
    let ditch_drain =
        projected_edge_drain(geometry, ditch_owner, Vec3::new(32.0, -2.42, 29.0), Vec2::X);
    let bridge_owner = GeometryOwnerId(82_100);
    let bridge_node = StructuralNodeId(42_100_000);
    geometry.structural_nodes.push(StructuralNode {
        id: bridge_node,
        owner: bridge_owner,
        kind: StructuralNodeKind::ArtilleryBridgeAbutment,
        position: Vec3::new(6.0, 0.0, -17.0),
        supported_by: Vec::new(),
        grounded: true,
    });
    let inner_abutment = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, -0.1, -14.2),
        Vec3::new(3.2, 1.0, 1.2),
        0.0,
        SolidRole::ArtilleryBridgeAbutment,
        vec![bridge_node],
    );
    let outer_abutment = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, -0.1, -19.8),
        Vec3::new(3.2, 1.0, 1.2),
        0.0,
        SolidRole::ArtilleryBridgeAbutment,
        vec![bridge_node],
    );
    let fixed = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, 0.18, -15.9),
        Vec3::new(2.4, 0.28, 2.4),
        0.0,
        SolidRole::ArtilleryBridgeDeck,
        vec![bridge_node],
    );
    let bridge_state = if program.seed % 1_000 == 702 {
        crate::BridgeState::Denied
    } else {
        crate::BridgeState::Deployed
    };
    let removable = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, 0.18, -18.10),
        Vec3::new(2.4, 0.28, 2.10),
        0.0,
        SolidRole::ArtilleryBridgeDeck,
        vec![bridge_node],
    );
    let denied_gap = (bridge_state == crate::BridgeState::Denied).then(|| {
        projected_void(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(4.8, 0.0, -19.15),
                max: Vec3::new(7.2, 2.0, -17.4),
            },
            VoidRole::BridgeDeniedGap,
        )
    });
    if bridge_state == crate::BridgeState::Denied {
        geometry.solids.retain(|solid| solid.id != removable);
    }
    let bridge_route = (bridge_state == crate::BridgeState::Deployed).then(|| {
        projected_surface(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(4.9, 0.32, -19.0),
                max: Vec3::new(7.1, 0.35, -13.6),
            },
            SurfaceRole::ArtilleryRoute,
        )
    });
    let controls = [
        projected_surface(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(4.7, 0.0, -13.5),
                max: Vec3::new(5.7, 0.03, -12.5),
            },
            SurfaceRole::ArtilleryStance,
        ),
        projected_surface(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(6.3, 0.0, -13.5),
                max: Vec3::new(7.3, 0.03, -12.5),
            },
            SurfaceRole::ArtilleryStance,
        ),
    ];
    let gate_owner = curtains[0].owner;
    let gate_void = projected_void(
        geometry,
        gate_owner,
        ResolvedBounds {
            min: Vec3::new(4.4, 0.0, -13.55),
            max: Vec3::new(7.6, 3.6, -8.95),
        },
        VoidRole::Passage,
    );
    let gate_node = StructuralNodeId(42_200_000);
    geometry.structural_nodes.push(StructuralNode {
        id: gate_node,
        owner: gate_owner,
        kind: StructuralNodeKind::OpeningJamb,
        position: Vec3::new(6.0, 0.0, -9.0),
        supported_by: Vec::new(),
        grounded: true,
    });
    let chamber_node = StructuralNodeId(42_200_001);
    geometry.structural_nodes.push(StructuralNode {
        id: chamber_node,
        owner: gate_owner,
        kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
        position: Vec3::new(6.0, 5.58, -10.65),
        supported_by: vec![gate_node],
        grounded: false,
    });
    let gate_leaf = projected_solid(
        geometry,
        gate_owner,
        Vec3::new(6.0, 1.7, -9.2),
        Vec3::new(3.0, 3.4, 0.12),
        0.0,
        SolidRole::OpeningClosure,
        vec![gate_node],
    );
    let portcullis = projected_solid(
        geometry,
        gate_owner,
        Vec3::new(6.0, 1.8, -10.1),
        Vec3::new(3.0, 3.6, 0.10),
        0.0,
        SolidRole::OpeningClosure,
        vec![gate_node],
    );
    let gate_chamber_solids = vec![
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(6.0, 5.70, -10.65),
            Vec3::new(3.6, 0.24, 3.0),
            0.0,
            SolidRole::ArtilleryCasemateFloor,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(6.0, 8.02, -10.65),
            Vec3::new(3.6, 0.24, 3.0),
            0.0,
            SolidRole::ArtilleryCasemateRoof,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(4.35, 6.86, -10.65),
            Vec3::new(0.30, 2.10, 3.0),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(7.65, 6.86, -10.65),
            Vec3::new(0.30, 2.10, 3.0),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(4.95, 6.86, -12.0),
            Vec3::new(0.90, 2.10, 0.30),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(7.05, 6.86, -12.0),
            Vec3::new(0.90, 2.10, 0.30),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(5.05, 6.32, -10.35),
            Vec3::new(1.60, 0.22, 0.22),
            0.0,
            SolidRole::ArtilleryGateMechanism,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(4.22, 4.75, -10.35),
            Vec3::new(0.08, 2.95, 0.08),
            0.0,
            SolidRole::ArtilleryGateMechanism,
            vec![chamber_node],
        ),
    ];
    let operator = projected_surface(
        geometry,
        gate_owner,
        ResolvedBounds {
            min: Vec3::new(4.65, 5.83, -11.7),
            max: Vec3::new(7.35, 5.86, -9.4),
        },
        SurfaceRole::ArtilleryStance,
    );
    let mut route_nodes = Vec::new();
    let mut route_edges = Vec::new();
    let mut next_route = 0_u64;
    let mut add_route_node = |surface, position, nodes: &mut Vec<crate::ArtilleryRouteNode>| {
        let id = crate::ArtilleryRouteNodeId(next_route);
        next_route += 1;
        nodes.push(crate::ArtilleryRouteNode {
            id,
            surface,
            position,
        });
        id
    };
    let outer_approach = bridge_route
        .map(|surface| add_route_node(surface, Vec3::new(6.0, 0.34, -18.6), &mut route_nodes));
    let gate_outer = add_route_node(controls[0], Vec3::new(5.2, 0.34, -13.0), &mut route_nodes);
    let gate_inner = add_route_node(controls[1], Vec3::new(6.8, 0.02, -7.8), &mut route_nodes);
    if let Some(outer) = outer_approach {
        route_edges.push(crate::ArtilleryRouteEdge {
            from: outer,
            to: gate_outer,
            width_metres: 1.8,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: vec![fixed, removable],
            sweep_path: Vec::new(),
        });
    }
    route_edges.push(crate::ArtilleryRouteEdge {
        from: gate_outer,
        to: gate_inner,
        width_metres: 0.9,
        headroom_metres: 2.1,
        portal_void: Some(gate_void),
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    let ramp_owner = GeometryOwnerId(82_300);
    let ramp_node = StructuralNodeId(42_300_000);
    let retaining = retaining_support_node(&curtains, geometry);
    geometry.structural_nodes.push(StructuralNode {
        id: ramp_node,
        owner: ramp_owner,
        kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
        position: Vec3::new(20.5, 0.0, -5.0),
        supported_by: vec![retaining],
        grounded: false,
    });
    let ramp = projected_solid(
        geometry,
        ramp_owner,
        Vec3::new(20.5, 2.9, 6.0),
        Vec3::new(22.0, 0.28, 2.2),
        -std::f32::consts::FRAC_PI_2,
        SolidRole::ArtilleryRamp,
        vec![ramp_node],
    );
    if let Some(solid) = geometry.solids.iter_mut().find(|solid| solid.id == ramp) {
        solid.longfall_radians = (5.8_f32 / 22.0).atan();
    }
    let court_surface = projected_surface(
        geometry,
        ramp_owner,
        ResolvedBounds {
            min: Vec3::new(4.0, 0.0, -7.5),
            max: Vec3::new(8.0, 0.03, -3.5),
        },
        SurfaceRole::ArtilleryRoute,
    );
    let ramp_bottom = projected_surface(
        geometry,
        ramp_owner,
        ResolvedBounds {
            min: Vec3::new(19.4, 0.0, -5.0),
            max: Vec3::new(21.6, 0.03, -2.8),
        },
        SurfaceRole::ArtilleryRoute,
    );
    let ramp_top = projected_surface(
        geometry,
        ramp_owner,
        ResolvedBounds {
            min: Vec3::new(19.4, 5.8, 14.8),
            max: Vec3::new(21.6, 5.83, 17.0),
        },
        SurfaceRole::ArtilleryRoute,
    );
    // Cut the inner retaining wall at the protected ramp landing.  The ramp
    // reaches the terreplein through this real 2 m portal rather than a
    // semantic edge through intact masonry.
    let ramp_portal = projected_void(
        geometry,
        curtains[1].owner,
        ResolvedBounds {
            min: Vec3::new(23.65, 6.00, 14.85),
            max: Vec3::new(27.50, 8.10, 16.95),
        },
        VoidRole::AccessPortal,
    );
    for layer in 0..2 {
        let old_id = if layer == 0 {
            curtains[1].retaining_solids[0]
        } else {
            curtains[1].earth_solids[0]
        };
        if let Some(old) = geometry
            .solids
            .iter()
            .find(|solid| solid.id == old_id)
            .cloned()
        {
            let original_min = old.centre.z - old.size.z * 0.5;
            let original_max = old.centre.z + old.size.z * 0.5;
            let south_length = 14.85 - original_min;
            let north_length = original_max - 16.95;
            if let Some(south) = geometry.solids.iter_mut().find(|solid| solid.id == old_id) {
                south.centre.z = original_min + south_length * 0.5;
                south.size.z = south_length;
            }
            let north = projected_solid(
                geometry,
                old.owner,
                Vec3::new(old.centre.x, old.centre.y, 16.95 + north_length * 0.5),
                Vec3::new(old.size.x, old.size.y, north_length),
                old.yaw_radians,
                old.role,
                old.supported_by,
            );
            if layer == 0 {
                curtains[1].retaining_solids = vec![old_id, north];
            } else {
                curtains[1].earth_solids = vec![old_id, north];
            }
        }
    }
    let court_id = add_route_node(court_surface, Vec3::new(6.0, 0.02, -5.5), &mut route_nodes);
    route_edges.push(crate::ArtilleryRouteEdge {
        from: gate_inner,
        to: court_id,
        width_metres: 1.8,
        headroom_metres: 2.2,
        portal_void: None,
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    let ramp_bottom_id = add_route_node(ramp_bottom, Vec3::new(20.5, 0.02, -3.9), &mut route_nodes);
    let ramp_top_id = add_route_node(ramp_top, Vec3::new(20.5, 5.82, 15.9), &mut route_nodes);
    route_edges.extend([
        crate::ArtilleryRouteEdge {
            from: court_id,
            to: ramp_bottom_id,
            width_metres: 2.0,
            headroom_metres: 2.2,
            portal_void: None,
            traversal_surface: None,
            connector_solids: Vec::new(),
            sweep_path: Vec::new(),
        },
        crate::ArtilleryRouteEdge {
            from: ramp_bottom_id,
            to: ramp_top_id,
            width_metres: 2.0,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: vec![ramp],
            sweep_path: Vec::new(),
        },
    ]);
    let mut curtain_nodes = Vec::new();
    for curtain in &curtains {
        let surface = geometry
            .surfaces
            .iter()
            .find(|surface| surface.id == curtain.route_surface)
            .unwrap();
        let mut position = (surface.bounds.min + surface.bounds.max) * 0.5;
        if curtain.id == crate::ArtilleryCurtainId(0) {
            position.x = 10.5;
        }
        curtain_nodes.push(add_route_node(
            curtain.route_surface,
            position,
            &mut route_nodes,
        ));
    }
    route_edges.push(crate::ArtilleryRouteEdge {
        from: ramp_top_id,
        to: curtain_nodes[1],
        width_metres: 2.0,
        headroom_metres: 2.1,
        portal_void: Some(ramp_portal),
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    for index in 0..4 {
        route_edges.push(crate::ArtilleryRouteEdge {
            from: curtain_nodes[index],
            to: curtain_nodes[(index + 1) % 4],
            width_metres: 2.0,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: Vec::new(),
            sweep_path: Vec::new(),
        });
    }
    for (rondel_index, rondel) in rondels.iter().enumerate() {
        let surface = rondel.route_surfaces[0];
        let centre = rondel.anchor.metres();
        let curtain_index = rondel
            .adjoining_curtains
            .into_iter()
            .map(|id| id.0 as usize)
            .min_by(|left, right| {
                let lp = route_nodes
                    .iter()
                    .find(|node| node.id == curtain_nodes[*left])
                    .unwrap()
                    .position;
                let rp = route_nodes
                    .iter()
                    .find(|node| node.id == curtain_nodes[*right])
                    .unwrap()
                    .position;
                Vec2::new(lp.x, lp.z)
                    .distance(centre)
                    .total_cmp(&Vec2::new(rp.x, rp.z).distance(centre))
            })
            .unwrap();
        let curtain_position = route_nodes
            .iter()
            .find(|node| node.id == curtain_nodes[curtain_index])
            .unwrap()
            .position;
        let toward = towers[rondel_index]
            .chord_interfaces()
            .map(|interface| direction_vector(interface.toward_gate))
            .min_by(|left, right| {
                (centre + *left * 5.0 - Vec2::new(curtain_position.x, curtain_position.z))
                    .length()
                    .total_cmp(
                        &(centre + *right * 5.0
                            - Vec2::new(curtain_position.x, curtain_position.z))
                        .length(),
                    )
            })
            .unwrap();
        let position = Vec3::new(centre.x + toward.x * 3.5, 5.86, centre.y + toward.y * 3.5);
        let upper = add_route_node(surface, position, &mut route_nodes);
        let portal = Vec3::new(centre.x + toward.x * 5.1, 5.86, centre.y + toward.y * 5.1);
        let pre = if curtain_index == 0 || curtain_index == 2 {
            Vec3::new(
                portal.x - (portal.x - curtain_position.x).signum() * 0.8,
                5.86,
                curtain_position.z,
            )
        } else {
            Vec3::new(
                curtain_position.x,
                5.86,
                portal.z - (portal.z - curtain_position.z).signum() * 0.8,
            )
        };
        route_edges.push(crate::ArtilleryRouteEdge {
            from: curtain_nodes[curtain_index],
            to: upper,
            width_metres: 2.0,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: Vec::new(),
            sweep_path: vec![curtain_position, pre, portal, position],
        });
        let stair_arrival = Vec2::new(
            if rondel_index % 2 == 0 { 1.0 } else { -1.0 },
            if rondel_index < 2 { 1.0 } else { -1.0 },
        )
        .normalize();
        for station in stations.iter().filter(|station| {
            station.rondel == crate::ArtilleryRondelId(rondel_index as u64)
                && station.level == crate::ArtilleryStationLevel::LowerCasemate
        }) {
            let lower_position = geometry
                .surfaces
                .iter()
                .find(|item| item.id == station.stance_surface)
                .map(|item| (item.bounds.min + item.bounds.max) * 0.5)
                .unwrap();
            let lower = add_route_node(station.stance_surface, lower_position, &mut route_nodes);
            let mut stair_path = vec![
                position,
                Vec3::new(
                    centre.x + stair_arrival.x * 1.65,
                    5.86,
                    centre.y + stair_arrival.y * 1.65,
                ),
            ];
            stair_path.extend(rondel.stair_solids.iter().rev().filter_map(|id| {
                geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
                    .map(|solid| solid.centre)
            }));
            stair_path.push(lower_position);
            route_edges.push(crate::ArtilleryRouteEdge {
                from: upper,
                to: lower,
                width_metres: 0.9,
                headroom_metres: 2.1,
                portal_void: Some(rondel.casemate_void),
                traversal_surface: None,
                connector_solids: rondel.stair_solids.clone(),
                sweep_path: stair_path,
            });
        }
    }
    let operator_id = add_route_node(operator, Vec3::new(6.0, 5.84, -10.5), &mut route_nodes);
    let curtain_operator_start = route_nodes
        .iter()
        .find(|node| node.id == curtain_nodes[0])
        .unwrap()
        .position;
    route_edges.push(crate::ArtilleryRouteEdge {
        from: curtain_nodes[0],
        to: operator_id,
        width_metres: 0.9,
        headroom_metres: 2.0,
        portal_void: None,
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: vec![
            curtain_operator_start,
            Vec3::new(8.35, 5.84, -8.35),
            Vec3::new(6.8, 5.84, -8.35),
            Vec3::new(6.0, 5.84, -10.5),
        ],
    });
    let second_control = add_route_node(controls[1], Vec3::new(6.8, 0.02, -13.0), &mut route_nodes);
    route_edges.push(crate::ArtilleryRouteEdge {
        from: gate_outer,
        to: second_control,
        width_metres: 0.9,
        headroom_metres: 2.0,
        portal_void: None,
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    let route_owner = GeometryOwnerId(82_400);
    for edge in &mut route_edges {
        let from = route_nodes
            .iter()
            .find(|node| node.id == edge.from)
            .unwrap()
            .position;
        let to = route_nodes
            .iter()
            .find(|node| node.id == edge.to)
            .unwrap()
            .position;
        let curtain_pair = curtain_nodes
            .iter()
            .position(|id| *id == edge.from)
            .zip(curtain_nodes.iter().position(|id| *id == edge.to));
        let coarse_path = if !edge.sweep_path.is_empty() {
            edge.sweep_path.clone()
        } else if let Some((left, right)) = curtain_pair {
            let rondel_index = rondels
                .iter()
                .position(|rondel| {
                    rondel
                        .adjoining_curtains
                        .contains(&crate::ArtilleryCurtainId(left as u64))
                        && rondel
                            .adjoining_curtains
                            .contains(&crate::ArtilleryCurtainId(right as u64))
                })
                .unwrap();
            let centre = towers[rondel_index].centre_metres();
            let mut directions = towers[rondel_index]
                .chord_interfaces()
                .map(|interface| direction_vector(interface.toward_gate))
                .collect::<Vec<_>>();
            let from_plan = Vec2::new(from.x, from.z);
            directions.sort_by(|a, b| {
                (centre + *a * 5.0 - from_plan)
                    .length()
                    .total_cmp(&(centre + *b * 5.0 - from_plan).length())
            });
            let start_angle = directions[0].y.atan2(directions[0].x);
            let end_angle = directions[1].y.atan2(directions[1].x);
            let mut sweep = (end_angle - start_angle).rem_euclid(std::f32::consts::TAU);
            if sweep > std::f32::consts::PI {
                sweep -= std::f32::consts::TAU;
            }
            let portal0 = Vec3::new(
                centre.x + directions[0].x * 5.1,
                from.y,
                centre.y + directions[0].y * 5.1,
            );
            let pre0 = if left == 0 || left == 2 {
                Vec3::new(
                    portal0.x - (portal0.x - from.x).signum() * 0.8,
                    from.y,
                    from.z,
                )
            } else {
                Vec3::new(
                    from.x,
                    from.y,
                    portal0.z - (portal0.z - from.z).signum() * 0.8,
                )
            };
            let mut path = vec![from, pre0, portal0];
            path.extend((1..=6).map(|step| {
                let angle = start_angle + sweep * step as f32 / 6.0;
                Vec3::new(
                    centre.x + angle.cos() * 4.35,
                    (from.y + to.y) * 0.5,
                    centre.y + angle.sin() * 4.35,
                )
            }));
            let portal1 = Vec3::new(
                centre.x + directions[1].x * 5.1,
                to.y,
                centre.y + directions[1].y * 5.1,
            );
            path.push(portal1);
            let post1 = if right == 0 || right == 2 {
                Vec3::new(portal1.x - (portal1.x - to.x).signum() * 0.8, to.y, to.z)
            } else {
                Vec3::new(to.x, to.y, portal1.z - (portal1.z - to.z).signum() * 0.8)
            };
            path.push(post1);
            if left == 3 && right == 0 {
                path.extend([
                    Vec3::new(3.2, to.y, -10.95),
                    Vec3::new(3.2, to.y, -7.35),
                    Vec3::new(9.2, to.y, -7.35),
                    Vec3::new(9.2, to.y, -10.95),
                ]);
            }
            path.push(to);
            path
        } else if edge.connector_solids.len() >= 30 {
            let mut points = edge
                .connector_solids
                .iter()
                .filter_map(|id| {
                    geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == *id)
                        .map(|solid| solid.centre)
                })
                .collect::<Vec<_>>();
            points.sort_by(|a, b| a.y.total_cmp(&b.y));
            if from.y > to.y {
                points.reverse();
            }
            let mut path = vec![from];
            path.extend(points);
            path.push(to);
            path
        } else if (from.y - to.y).abs() < 0.25
            && (from.x - to.x).abs() > 3.0
            && (from.z - to.z).abs() > 3.0
        {
            vec![from, Vec3::new(to.x, (from.y + to.y) * 0.5, from.z), to]
        } else {
            vec![from, to]
        };
        edge.sweep_path = coarse_path
            .windows(2)
            .enumerate()
            .flat_map(|(segment, pair)| {
                let steps = (pair[0].distance(pair[1]) / 0.30).ceil() as usize;
                (0..steps).filter_map(move |step| {
                    ((segment == 0) || step > 0)
                        .then_some(pair[0].lerp(pair[1], step as f32 / steps as f32))
                })
            })
            .chain([to])
            .collect();
        let half = Vec3::new(edge.width_metres * 0.5, 0.03, edge.width_metres * 0.5);
        let id = projected_surface(
            geometry,
            route_owner,
            ResolvedBounds {
                min: from.min(to) - half,
                max: from.max(to) + half,
            },
            SurfaceRole::ArtilleryRoute,
        );
        geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == id)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::RouteCorridor {
            start: from,
            end: to,
            width_metres: edge.width_metres,
        };
        edge.traversal_surface = Some(id);
    }
    Some(crate::ArtilleryCastleAssembly {
        id: crate::ArtilleryCastleAssemblyId(1),
        phase: crate::CastleConstructionPhase::ArtilleryRetrofit1544,
        trace,
        clear_court_size_metres: Vec2::new(36.0, 30.0),
        crown_elevation_metres: crown,
        curtains,
        rondels,
        stations,
        defense_targets,
        ditch: crate::ArtilleryDitchAssembly {
            width_metres: 5.0,
            depth_metres: 2.3,
            void_id: ditch_void,
            scarp_solids,
            counterscarp_solids,
            floor_solids: floors,
            drainage_routes: vec![ditch_drain],
            outlet_surface: ditch_outlet,
        },
        bridge: crate::ArtilleryBridgeAssembly {
            state: bridge_state,
            clear_width_metres: 2.2,
            inner_abutment,
            outer_abutment,
            fixed_solids: vec![fixed],
            removable_solids: vec![removable],
            denied_gap_void: denied_gap,
            route_surface: bridge_route,
            control_surfaces: controls,
        },
        gate_passage_void: gate_void,
        gate_closure_solids: vec![gate_leaf, portcullis],
        gate_chamber_solids,
        gate_operator_surface: operator,
        service_ramp_solids: vec![ramp],
        route_nodes,
        route_edges,
        retained_keep_setback_metres: 4.5,
        support_interfaces: support_ids,
        drainage_routes: artillery_drainage_routes,
    })
}
