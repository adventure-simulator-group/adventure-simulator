fn resolve_church_assembly(
    program: &BuildingProgram,
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    stairs: &mut Vec<Stair>,
    geometry: &mut ResolvedGeometry,
) -> crate::ChurchAssembly {
    let church_program = program
        .church_program
        .expect("cathedral fixture has a church program");
    let owner = GeometryOwnerId(70_000);
    let datum = crate::ChurchDatum {
        floor_metres: 0.0,
        aisle_eave_metres: 7.0,
        clerestory_sill_metres: 9.10,
        nave_eave_metres: 11.5,
        vault_crown_metres: 10.85,
        bell_floor_metres: 17.3,
    };
    let tower_size = Vec2::splat(5.4);
    let tower_centre = Vec2::new(2.7, 10.5);
    let nave_west = 5.4_f32;
    let bay = f32::from(church_program.bay_length_cells) * CELL_SIZE_METRES;
    let nave_axes_metres = (0..church_program.nave_bays)
        .map(|index| nave_west + (f32::from(index) + 1.0) * bay)
        .collect::<Vec<_>>();
    let crossing_axis_metres = nave_west + f32::from(church_program.nave_bays) * bay + bay * 0.5;
    let crossing_west = crossing_axis_metres - bay * 0.5;
    let crossing_east = crossing_axis_metres + bay * 0.5;
    let choir_axes_metres = (0..church_program.choir_bays)
        .map(|index| crossing_east + (f32::from(index) + 0.5) * bay)
        .collect::<Vec<_>>();
    let choir_east = crossing_east + f32::from(church_program.choir_bays) * bay;

    let next_node = std::cell::Cell::new(7_000_000_u64);
    let next_slot = std::cell::Cell::new(0x7000_u64);
    let node = |kind: StructuralNodeKind,
                position: Vec3,
                supported_by: Vec<StructuralNodeId>,
                grounded: bool,
                geometry: &mut ResolvedGeometry| {
        let id = StructuralNodeId(next_node.get());
        next_node.set(next_node.get() + 1);
        geometry.structural_nodes.push(StructuralNode {
            id,
            owner,
            kind,
            position,
            supported_by,
            grounded,
        });
        id
    };
    let solid = |centre: Vec3,
                 size: Vec3,
                 role: SolidRole,
                 supports: Vec<StructuralNodeId>,
                 geometry: &mut ResolvedGeometry| {
        let slot = next_slot.get();
        next_slot.set(slot + 1);
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
            shape: crate::ResolvedSolidShape::Cuboid,
            supported_by: supports.clone(),
        });
        for support in supports {
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | next_slot.get()),
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
            next_slot.set(next_slot.get() + 1);
        }
        id
    };

    // Three floor strips remain separate so aisle/nave route widths are
    // explicit and testable.  Their shared boundaries have zero overlap.
    let floor_node = node(
        StructuralNodeKind::WallBearing,
        Vec3::new(21.0, 0.0, 10.5),
        Vec::new(),
        true,
        geometry,
    );
    let mut floor_solids = Vec::new();
    for (z, width) in [(6.0_f32, 2.10_f32), (10.5, 5.10), (15.0, 2.10)] {
        floor_solids.push(solid(
            Vec3::new((nave_west + choir_east) * 0.5, 0.10, z),
            Vec3::new(choir_east - nave_west - 0.90, 0.20, width),
            SolidRole::ChurchFloor,
            vec![floor_node],
            geometry,
        ));
    }
    floor_solids.push(solid(
        Vec3::new(crossing_axis_metres, 0.10, 10.5),
        Vec3::new(bay - 0.90, 0.20, 17.10),
        SolidRole::ChurchFloor,
        vec![floor_node],
        geometry,
    ));

    // The church envelope replaces the generic cell-wall vocabulary.  Each
    // bay-length host is authoritative for its masonry, later opening cuts,
    // buttress station, and roof bearing.
    let mut church_wall_serial = 0_u64;
    let mut exterior_segments = Vec::new();
    for bay_index in 0..church_program.nave_bays {
        let x = nave_west + (f32::from(bay_index) + 0.5) * bay;
        exterior_segments.push((
            crate::ChurchRange::Nave,
            Direction::South,
            bay_index,
            Vec2::new(x, 4.5),
            Vec2::X,
            Vec2::NEG_Y,
            bay,
        ));
        exterior_segments.push((
            crate::ChurchRange::Nave,
            Direction::North,
            bay_index,
            Vec2::new(x, 16.5),
            Vec2::X,
            Vec2::Y,
            bay,
        ));
    }
    for bay_index in 0..church_program.choir_bays {
        let x = crossing_east + (f32::from(bay_index) + 0.5) * bay;
        exterior_segments.push((
            crate::ChurchRange::Choir,
            Direction::South,
            bay_index,
            Vec2::new(x, 7.5),
            Vec2::X,
            Vec2::NEG_Y,
            bay,
        ));
        exterior_segments.push((
            crate::ChurchRange::Choir,
            Direction::North,
            bay_index,
            Vec2::new(x, 13.5),
            Vec2::X,
            Vec2::Y,
            bay,
        ));
    }
    exterior_segments.extend([
        (
            crate::ChurchRange::Transept,
            Direction::South,
            0,
            Vec2::new(crossing_axis_metres, 1.5),
            Vec2::X,
            Vec2::NEG_Y,
            bay,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::North,
            0,
            Vec2::new(crossing_axis_metres, 19.5),
            Vec2::X,
            Vec2::Y,
            bay,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::West,
            0,
            Vec2::new(crossing_west, 4.5),
            Vec2::Y,
            Vec2::NEG_X,
            6.0,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::West,
            1,
            Vec2::new(crossing_west, 16.5),
            Vec2::Y,
            Vec2::NEG_X,
            6.0,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::East,
            0,
            Vec2::new(crossing_east, 4.5),
            Vec2::Y,
            Vec2::X,
            6.0,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::East,
            1,
            Vec2::new(crossing_east, 16.5),
            Vec2::Y,
            Vec2::X,
            6.0,
        ),
        (
            crate::ChurchRange::Nave,
            Direction::West,
            0,
            Vec2::new(nave_west, 6.0),
            Vec2::Y,
            Vec2::NEG_X,
            3.0,
        ),
        (
            crate::ChurchRange::Nave,
            Direction::West,
            1,
            Vec2::new(nave_west, 15.0),
            Vec2::Y,
            Vec2::NEG_X,
            3.0,
        ),
    ]);
    for (range, side, bay_index, origin, tangent, outward, length) in exterior_segments {
        let wall_height = if matches!(
            range,
            crate::ChurchRange::Transept | crate::ChurchRange::Choir
        ) {
            datum.nave_eave_metres
        } else {
            datum.aisle_eave_metres
        };
        let wall_owner = owner;
        let wall_node = StructuralNodeId(7_100_000 + church_wall_serial);
        geometry.structural_nodes.push(StructuralNode {
            id: wall_node,
            owner: wall_owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, 0.0, origin.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let host = wall_solid(
            geometry,
            wall_owner,
            0x500 + church_wall_serial,
            Vec3::new(origin.x, wall_height * 0.5, origin.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(length, wall_height, 0.90)
            } else {
                Vec3::new(0.90, wall_height, length)
            },
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            wall_node,
        );
        walls.push(crate::WallAssembly {
            id: crate::WallAssemblyId(7_200_000 + church_wall_serial),
            owner: wall_owner,
            source: crate::WallSourceId::ChurchExterior {
                range,
                side,
                bay: bay_index,
            },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: length,
            height_metres: wall_height,
            base_elevation_metres: 0.0,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::Buttressed,
            support_node: wall_node,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
        church_wall_serial += 1;
    }

    let (west_tower_wall, west_portal_opening) = resolve_church_tower_door_wall(
        Direction::West,
        crate::OpeningAssemblyId(7_400_000),
        crate::WallAssemblyId(7_400_000),
        GeometryOwnerId(71_000),
        tower_centre,
        geometry,
    );
    let west_portal = west_portal_opening.id;
    walls.push(west_tower_wall);
    openings.push(west_portal_opening);
    let (east_tower_wall, nave_passage_opening) = resolve_church_tower_door_wall(
        Direction::East,
        crate::OpeningAssemblyId(7_400_001),
        crate::WallAssemblyId(7_400_001),
        GeometryOwnerId(71_001),
        tower_centre,
        geometry,
    );
    let nave_passage = nave_passage_opening.id;
    walls.push(east_tower_wall);
    openings.push(nave_passage_opening);
    for (serial, face) in [Direction::South, Direction::North].into_iter().enumerate() {
        let wall_owner = GeometryOwnerId(71_002 + serial as u32);
        let outward = direction_vector(face);
        let origin = tower_centre + outward * 2.70;
        let support = StructuralNodeId(7_600_000 + serial as u64);
        geometry.structural_nodes.push(StructuralNode {
            id: support,
            owner: wall_owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, 0.0, origin.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let host = wall_solid(
            geometry,
            wall_owner,
            0,
            Vec3::new(origin.x, 8.65, origin.y),
            Vec3::new(5.40, 17.30, 0.90),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            support,
        );
        walls.push(crate::WallAssembly {
            id: crate::WallAssemblyId(7_400_002 + serial as u64),
            owner: wall_owner,
            source: crate::WallSourceId::ChurchTowerFace {
                face,
                stage: crate::ChurchTowerStage::Stair,
                bay: 0,
            },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin,
                tangent: Vec2::X,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: 5.40,
            height_metres: 17.30,
            base_elevation_metres: 0.0,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::LoadBearing,
            support_node: support,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
    }

    // Bay spans share their west/east supports.  The former implementation
    // manufactured one pier at the east axis and let the arcade springing
    // depend on that one node; this boundary pair gives the first bay the
    // same two-ended bearing contract as every subsequent bay.
    // Keep the west springing on the established clerestory/roof-abutment
    // datum.  Moving this support half a bay east made the first arcade look
    // regular in isolation but silently detached both aisle-roof wall
    // abutments from their authoritative masonry host.
    let clerestory_west = nave_axes_metres[0] - bay;
    // The arcade/vault bearing sits inside the westwork return rather than on
    // the tower's east wall centreline.  The clerestory weather enclosure
    // continues to the roof-abutment datum below, but the grounded pier and
    // thrust member clear the tower shell by a positive 0.15 m.
    let nave_bearing_west = clerestory_west + 0.60;
    let mut previous_pier_nodes = [StructuralNodeId(0); 2];
    let mut previous_buttress_nodes = [StructuralNodeId(0); 2];
    for (side_index, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let arcade_z = 10.5 + side_sign * 3.0;
        let pier_node = node(
            StructuralNodeKind::ChurchPier,
            Vec3::new(nave_bearing_west, 0.0, arcade_z),
            Vec::new(),
            true,
            geometry,
        );
        solid(
            Vec3::new(nave_bearing_west, 3.55, arcade_z),
            Vec3::new(0.72, 7.10, 0.72),
            SolidRole::ChurchPier,
            vec![pier_node],
            geometry,
        );
        previous_pier_nodes[side_index] = pier_node;
        let outer_z = 10.5 + side_sign * 7.0;
        let buttress_node = node(
            StructuralNodeKind::ChurchButtress,
            Vec3::new(nave_bearing_west, 0.0, outer_z),
            Vec::new(),
            true,
            geometry,
        );
        solid(
            Vec3::new(nave_bearing_west, 3.2, outer_z),
            Vec3::new(0.85, 6.4, 1.10),
            SolidRole::WallButtress,
            vec![buttress_node],
            geometry,
        );
        previous_buttress_nodes[side_index] = buttress_node;
    }

    let mut bay_assemblies = Vec::new();
    for (index, axis) in nave_axes_metres.iter().copied().enumerate() {
        let mut pier_nodes = [StructuralNodeId(0); 2];
        let mut pier_solids = [ResolvedItemId(0); 2];
        let mut arcade_solids = [ResolvedItemId(0); 2];
        let mut arcade_bearing_nodes = [[StructuralNodeId(0); 2]; 2];
        let mut arcade_bearing_interfaces = [[ResolvedItemId(0); 2]; 2];
        let mut buttress_nodes = [StructuralNodeId(0); 2];
        let mut buttress_solids = [ResolvedItemId(0); 2];
        let mut vault_solids = Vec::new();
        let mut vault_thrust_solids = Vec::new();
        let mut vault_load_surfaces = Vec::new();
        let mut vault_spring_nodes = Vec::new();
        let mut vault_bearing_interfaces = Vec::new();
        let previous_axis = if index == 0 {
            nave_bearing_west
        } else {
            nave_axes_metres[index - 1]
        };
        for (side_index, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let arcade_z = 10.5 + side_sign * 3.0;
            let pier_node = node(
                StructuralNodeKind::ChurchPier,
                Vec3::new(axis, 0.0, arcade_z),
                Vec::new(),
                true,
                geometry,
            );
            pier_nodes[side_index] = pier_node;
            pier_solids[side_index] = solid(
                Vec3::new(axis, 3.55, arcade_z),
                Vec3::new(0.72, 7.10, 0.72),
                SolidRole::ChurchPier,
                vec![pier_node],
                geometry,
            );
            let spring_node = node(
                StructuralNodeKind::ChurchArcadeSpringing,
                Vec3::new((previous_axis + axis) * 0.5, 4.85, arcade_z),
                vec![previous_pier_nodes[side_index], pier_node],
                false,
                geometry,
            );
            arcade_bearing_nodes[side_index] = [previous_pier_nodes[side_index], pier_node];
            arcade_solids[side_index] = solid(
                Vec3::new((previous_axis + axis) * 0.5, 6.0, arcade_z),
                Vec3::new(axis - previous_axis, 2.30, 0.55),
                SolidRole::ChurchArcade,
                vec![spring_node],
                geometry,
            );
            if let Some(arcade) = geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == arcade_solids[side_index])
            {
                let rise = 1.60;
                arcade.shape = crate::ResolvedSolidShape::PointedArchRing {
                    clear_span_metres: axis - previous_axis,
                    spring_height_metres: 4.85,
                    apex_height_metres: 4.85 + rise,
                    arc_radius_metres: two_centred_arc_radius(axis - previous_axis, rise),
                    ring_depth_metres: 0.55,
                };
            }
            for (end_index, end_x) in [previous_axis, axis].into_iter().enumerate() {
                let interface_id = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (800 + index as u64 * 20 + side_index as u64 * 4 + end_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface_id,
                    owner,
                    node: spring_node,
                    bounds: ResolvedBounds {
                        min: Vec3::new(end_x - 0.30, 4.78, arcade_z - 0.27),
                        max: Vec3::new(end_x + 0.30, 5.12, arcade_z + 0.27),
                    },
                });
                arcade_bearing_interfaces[side_index][end_index] = interface_id;
            }
            let clerestory_owner = owner;
            let clerestory_wall_id =
                crate::WallAssemblyId(7_300_000 + index as u64 * 2 + side_index as u64);
            let clerestory_span_west = if index == 0 {
                clerestory_west
            } else {
                previous_axis
            };
            let clerestory_origin = Vec2::new((clerestory_span_west + axis) * 0.5, arcade_z);
            let clerestory_host = wall_solid(
                geometry,
                clerestory_owner,
                0,
                Vec3::new(clerestory_origin.x, 9.30, clerestory_origin.y),
                Vec3::new(axis - clerestory_span_west, 4.40, 0.75),
                SolidRole::WallHost,
                crate::ResolvedSolidShape::Cuboid,
                pier_node,
            );
            walls.push(crate::WallAssembly {
                id: clerestory_wall_id,
                owner: clerestory_owner,
                source: crate::WallSourceId::ChurchArcade {
                    side: if side_sign < 0.0 {
                        Direction::South
                    } else {
                        Direction::North
                    },
                    bay: index as u8,
                },
                material: crate::WallMaterialClass::CathedralMasonry,
                storey_level: 1,
                frame: crate::WallLocalFrame {
                    origin: clerestory_origin,
                    tangent: Vec2::X,
                    outward: if side_sign < 0.0 {
                        Vec2::NEG_Y
                    } else {
                        Vec2::Y
                    },
                    inside_room: None,
                    outside_room: None,
                },
                radial_frame: None,
                length_metres: axis - clerestory_span_west,
                height_metres: 4.40,
                base_elevation_metres: 7.10,
                thickness_metres: 0.75,
                structural_role: crate::WallStructuralRole::LoadBearing,
                support_node: pier_node,
                host_solids: vec![clerestory_host],
                opening_ids: Vec::new(),
                replaced_by_owner: None,
            });
            let outer_z = 10.5 + side_sign * 7.0;
            let buttress_node = node(
                StructuralNodeKind::ChurchButtress,
                Vec3::new(axis, 0.0, outer_z),
                Vec::new(),
                true,
                geometry,
            );
            buttress_nodes[side_index] = buttress_node;
            buttress_solids[side_index] = solid(
                Vec3::new(axis, 3.2, outer_z),
                Vec3::new(0.85, 6.4, 1.10),
                SolidRole::WallButtress,
                vec![buttress_node],
                geometry,
            );
        }
        let west = previous_axis;
        for (side_index, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let spring = node(
                StructuralNodeKind::ChurchVaultSpringing,
                Vec3::new((west + axis) * 0.5, 7.1, 10.5 + side_sign * 3.0),
                vec![
                    previous_pier_nodes[side_index],
                    pier_nodes[side_index],
                    previous_buttress_nodes[side_index],
                    buttress_nodes[side_index],
                ],
                false,
                geometry,
            );
            vault_spring_nodes.push(spring);
            let vault = solid(
                Vec3::new((west + axis) * 0.5, 9.05, 10.5 + side_sign * 1.5),
                Vec3::new(axis - west - 0.10, 0.22, 3.20),
                SolidRole::ChurchVaultShell,
                vec![spring],
                geometry,
            );
            if let Some(resolved) = geometry.solids.iter_mut().find(|item| item.id == vault) {
                resolved.crossfall_radians = side_sign * 0.50;
            }
            vault_solids.push(vault);
            for bearing_x in [west, axis] {
                vault_thrust_solids.push(solid(
                    Vec3::new(bearing_x, 7.05, 10.5 + side_sign * 5.0),
                    Vec3::new(0.46, 0.34, 4.0),
                    SolidRole::ChurchVaultThrust,
                    vec![spring],
                    geometry,
                ));
            }
            for (bearing_index, (bearing_x, bearing_z)) in [
                (west, 10.5 + side_sign * 3.0),
                (axis, 10.5 + side_sign * 3.0),
                (west, 10.5 + side_sign * 7.0),
                (axis, 10.5 + side_sign * 7.0),
            ]
            .into_iter()
            .enumerate()
            {
                let interface_id = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (900 + index as u64 * 20 + side_index as u64 * 6 + bearing_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface_id,
                    owner,
                    node: spring,
                    bounds: ResolvedBounds {
                        min: Vec3::new(bearing_x - 0.22, 6.95, bearing_z - 0.22),
                        max: Vec3::new(bearing_x + 0.22, 7.22, bearing_z + 0.22),
                    },
                });
                vault_bearing_interfaces.push(interface_id);
            }
            let surface_id = wall_surface(
                geometry,
                owner,
                next_slot.get(),
                ResolvedBounds {
                    min: Vec3::new(west, 7.0, 7.5),
                    max: Vec3::new(axis, datum.vault_crown_metres, 13.5),
                },
                SurfaceRole::ChurchVaultLoad,
            );
            next_slot.set(next_slot.get() + 1);
            vault_load_surfaces.push(surface_id);
        }
        bay_assemblies.push(crate::ChurchBayAssembly {
            axis_index: index as u8,
            axis_metres: axis,
            range: crate::ChurchRange::Nave,
            pier_nodes,
            pier_solids,
            arcade_solids,
            arcade_bearing_nodes,
            arcade_bearing_interfaces,
            buttress_nodes,
            buttress_solids,
            clerestory_openings: [crate::OpeningAssemblyId::default(); 2],
            vault_solids,
            vault_thrust_solids,
            vault_load_surfaces,
            vault_spring_nodes,
            vault_bearing_interfaces,
        });
        previous_pier_nodes = pier_nodes;
        previous_buttress_nodes = buttress_nodes;
    }

    let crossing_positions = [
        Vec2::new(crossing_west, 7.5),
        Vec2::new(crossing_west, 13.5),
        Vec2::new(crossing_east, 7.5),
        Vec2::new(crossing_east, 13.5),
    ];
    let mut crossing_nodes = [StructuralNodeId(0); 4];
    let mut crossing_piers = [ResolvedItemId(0); 4];
    for (index, position) in crossing_positions.into_iter().enumerate() {
        let support = node(
            StructuralNodeKind::ChurchCrossingPier,
            Vec3::new(position.x, 0.0, position.y),
            Vec::new(),
            true,
            geometry,
        );
        crossing_nodes[index] = support;
        crossing_piers[index] = solid(
            Vec3::new(position.x, 5.1, position.y),
            Vec3::new(1.05, 10.2, 1.05),
            SolidRole::ChurchPier,
            vec![support],
            geometry,
        );
    }
    let mut crossing_arches = [ResolvedItemId(0); 4];
    let mut crossing_arch_bearing_nodes = [[StructuralNodeId(0); 2]; 4];
    let mut crossing_arch_bearing_interfaces = [[ResolvedItemId(0); 2]; 4];
    for (index, (centre, size, supports)) in [
        (
            Vec3::new(crossing_axis_metres, 9.1, 7.5),
            Vec3::new(bay, 1.0, 0.70),
            vec![crossing_nodes[0], crossing_nodes[2]],
        ),
        (
            Vec3::new(crossing_axis_metres, 9.1, 13.5),
            Vec3::new(bay, 1.0, 0.70),
            vec![crossing_nodes[1], crossing_nodes[3]],
        ),
        (
            Vec3::new(crossing_west, 9.1, 10.5),
            Vec3::new(0.70, 1.0, 6.0),
            vec![crossing_nodes[0], crossing_nodes[1]],
        ),
        (
            Vec3::new(crossing_east, 9.1, 10.5),
            Vec3::new(0.70, 1.0, 6.0),
            vec![crossing_nodes[2], crossing_nodes[3]],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let arch_spring = node(
            StructuralNodeKind::ChurchArcadeSpringing,
            Vec3::new(centre.x, 5.75, centre.z),
            supports.clone(),
            false,
            geometry,
        );
        let arch_height = 3.0;
        crossing_arches[index] = solid(
            Vec3::new(centre.x, 7.25, centre.z),
            Vec3::new(size.x, arch_height, size.z),
            SolidRole::ChurchCrossingArch,
            vec![arch_spring],
            geometry,
        );
        let span = size.x.max(size.z);
        if let Some(arch) = geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == crossing_arches[index])
        {
            let rise = 2.0;
            arch.shape = crate::ResolvedSolidShape::PointedArchRing {
                clear_span_metres: span,
                spring_height_metres: 5.75,
                apex_height_metres: 5.75 + rise,
                arc_radius_metres: two_centred_arc_radius(span, rise),
                ring_depth_metres: size.x.min(size.z),
            };
        }
        crossing_arch_bearing_nodes[index] = [supports[0], supports[1]];
        let along_x = size.x > size.z;
        for (end_index, sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let contact = if along_x {
                Vec3::new(centre.x + sign * span * 0.5, 6.0, centre.z)
            } else {
                Vec3::new(centre.x, 6.0, centre.z + sign * span * 0.5)
            };
            let interface = ResolvedItemId(
                (4_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | (1_200 + index as u64 * 2 + end_index as u64),
            );
            geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner,
                node: arch_spring,
                bounds: ResolvedBounds {
                    min: contact - Vec3::new(0.28, 0.25, 0.28),
                    max: contact + Vec3::new(0.28, 0.25, 0.28),
                },
            });
            crossing_arch_bearing_interfaces[index][end_index] = interface;
        }
    }
    let mut crossing_buttress_nodes = [StructuralNodeId(0); 4];
    let mut crossing_buttress_solids = [ResolvedItemId(0); 4];
    let mut crossing_thrust_solids = Vec::new();
    let mut crossing_vault_bearings = Vec::new();
    for (index, pier_position) in crossing_positions.into_iter().enumerate() {
        let outward_z = if pier_position.y < 10.5 { -1.0 } else { 1.0 };
        let buttress_position = Vec2::new(pier_position.x, pier_position.y + outward_z * 2.0);
        let buttress = node(
            StructuralNodeKind::ChurchButtress,
            Vec3::new(buttress_position.x, 0.0, buttress_position.y),
            Vec::new(),
            true,
            geometry,
        );
        crossing_buttress_nodes[index] = buttress;
        crossing_buttress_solids[index] = solid(
            Vec3::new(buttress_position.x, 4.2, buttress_position.y),
            Vec3::new(1.05, 8.4, 1.25),
            SolidRole::WallButtress,
            vec![buttress],
            geometry,
        );
    }
    let crossing_vault_node = node(
        StructuralNodeKind::ChurchVaultSpringing,
        Vec3::new(crossing_axis_metres, 9.0, 10.5),
        crossing_nodes
            .iter()
            .chain(&crossing_buttress_nodes)
            .copied()
            .collect(),
        false,
        geometry,
    );
    for (index, pier_position) in crossing_positions.into_iter().enumerate() {
        let outward_z = if pier_position.y < 10.5 { -1.0 } else { 1.0 };
        crossing_thrust_solids.push(solid(
            Vec3::new(pier_position.x, 7.1, pier_position.y + outward_z),
            Vec3::new(0.48, 0.36, 2.0),
            SolidRole::ChurchVaultThrust,
            vec![crossing_vault_node],
            geometry,
        ));
        for (end, position) in [
            pier_position,
            Vec2::new(pier_position.x, pier_position.y + outward_z * 2.0),
        ]
        .into_iter()
        .enumerate()
        {
            let interface = ResolvedItemId(
                (4_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | (1_240 + index as u64 * 2 + end as u64),
            );
            geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner,
                node: crossing_vault_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(position.x - 0.24, 6.92, position.y - 0.24),
                    max: Vec3::new(position.x + 0.24, 7.28, position.y + 0.24),
                },
            });
            crossing_vault_bearings.push(interface);
        }
    }
    let crossing_vaults = vec![solid(
        Vec3::new(crossing_axis_metres, 10.45, 10.5),
        Vec3::new(bay - 0.15, 0.24, 5.85),
        SolidRole::ChurchVaultShell,
        vec![crossing_vault_node],
        geometry,
    )];
    let crossing_load_surface = wall_surface(
        geometry,
        owner,
        next_slot.get(),
        ResolvedBounds {
            min: Vec3::new(crossing_west, 7.0, 7.5),
            max: Vec3::new(crossing_east, datum.vault_crown_metres, 13.5),
        },
        SurfaceRole::ChurchVaultLoad,
    );
    next_slot.set(next_slot.get() + 1);
    let crossing = crate::ChurchCrossingAssembly {
        bounds: ResolvedBounds {
            min: Vec3::new(crossing_west, 0.0, 7.5),
            max: Vec3::new(crossing_east, datum.vault_crown_metres, 13.5),
        },
        pier_nodes: crossing_nodes,
        pier_solids: crossing_piers,
        arch_solids: crossing_arches,
        arch_bearing_nodes: crossing_arch_bearing_nodes,
        arch_bearing_interfaces: crossing_arch_bearing_interfaces,
        vault_solids: crossing_vaults,
        buttress_nodes: crossing_buttress_nodes,
        buttress_solids: crossing_buttress_solids,
        vault_thrust_solids: crossing_thrust_solids,
        vault_load_surfaces: vec![crossing_load_surface],
        vault_spring_nodes: vec![crossing_vault_node],
        vault_bearing_interfaces: crossing_vault_bearings,
    };

    let mut choir_pier_nodes = Vec::new();
    let mut choir_pier_solids = Vec::new();
    let mut choir_buttress_nodes = Vec::new();
    let mut choir_buttress_solids = Vec::new();
    let mut choir_arch_solids = Vec::new();
    let mut choir_arch_bearing_nodes = Vec::new();
    let mut choir_arch_bearing_interfaces = Vec::new();
    let mut choir_vault_solids = Vec::new();
    let mut choir_vault_thrust_solids = Vec::new();
    let mut choir_vault_load_surfaces = Vec::new();
    let mut choir_vault_spring_nodes = Vec::new();
    let mut choir_vault_bearing_interfaces = Vec::new();
    let mut previous_choir_piers = [crossing_nodes[2], crossing_nodes[3]];
    let mut previous_choir_buttresses = [crossing_buttress_nodes[2], crossing_buttress_nodes[3]];
    for (bay_index, axis) in choir_axes_metres.iter().copied().enumerate() {
        let west = crossing_east + bay_index as f32 * bay;
        let east = west + bay;
        let mut current_piers = [StructuralNodeId(0); 2];
        let mut current_buttresses = [StructuralNodeId(0); 2];
        for (side_index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let z = 10.5 + side * 3.0;
            let pier_node = node(
                StructuralNodeKind::ChurchPier,
                Vec3::new(east, 0.0, z),
                Vec::new(),
                true,
                geometry,
            );
            current_piers[side_index] = pier_node;
            choir_pier_nodes.push(pier_node);
            choir_pier_solids.push(solid(
                Vec3::new(east, 5.1, z),
                Vec3::new(0.78, 10.2, 0.78),
                SolidRole::ChurchPier,
                vec![pier_node],
                geometry,
            ));
            let buttress_node = node(
                StructuralNodeKind::ChurchButtress,
                Vec3::new(east, 0.0, 10.5 + side * 4.0),
                Vec::new(),
                true,
                geometry,
            );
            current_buttresses[side_index] = buttress_node;
            choir_buttress_nodes.push(buttress_node);
            choir_buttress_solids.push(solid(
                Vec3::new(east, 4.0, 10.5 + side * 4.0),
                Vec3::new(0.85, 8.0, 1.10),
                SolidRole::WallButtress,
                vec![buttress_node],
                geometry,
            ));
        }
        for (side_index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let arcade_z = 10.5 + side * 3.0;
            let arch_spring = node(
                StructuralNodeKind::ChurchArcadeSpringing,
                Vec3::new((west + east) * 0.5, 6.6, arcade_z),
                vec![previous_choir_piers[side_index], current_piers[side_index]],
                false,
                geometry,
            );
            let arch = solid(
                Vec3::new((west + east) * 0.5, 6.2, arcade_z),
                Vec3::new(east - west, 2.60, 0.62),
                SolidRole::ChurchArcade,
                vec![arch_spring],
                geometry,
            );
            if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == arch) {
                let rise = 1.75;
                item.shape = crate::ResolvedSolidShape::PointedArchRing {
                    clear_span_metres: east - west,
                    spring_height_metres: 4.90,
                    apex_height_metres: 4.90 + rise,
                    arc_radius_metres: two_centred_arc_radius(east - west, rise),
                    ring_depth_metres: 0.62,
                };
            }
            choir_arch_solids.push(arch);
            choir_arch_bearing_nodes
                .push([previous_choir_piers[side_index], current_piers[side_index]]);
            let mut arch_interfaces = [ResolvedItemId(0); 2];
            for (end_index, end_x) in [west, east].into_iter().enumerate() {
                let interface = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (1_300
                            + bay_index as u64 * 32
                            + side_index as u64 * 4
                            + end_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface,
                    owner,
                    node: arch_spring,
                    bounds: ResolvedBounds {
                        min: Vec3::new(end_x - 0.28, 5.0, arcade_z - 0.28),
                        max: Vec3::new(end_x + 0.28, 5.4, arcade_z + 0.28),
                    },
                });
                arch_interfaces[end_index] = interface;
            }
            choir_arch_bearing_interfaces.push(arch_interfaces);
            let spring = node(
                StructuralNodeKind::ChurchVaultSpringing,
                Vec3::new(axis, 7.5, 10.5 + side * 3.0),
                vec![
                    previous_choir_piers[side_index],
                    current_piers[side_index],
                    previous_choir_buttresses[side_index],
                    current_buttresses[side_index],
                ],
                false,
                geometry,
            );
            choir_vault_spring_nodes.push(spring);
            let vault = solid(
                Vec3::new((west + east) * 0.5, 9.25, 10.5 + side * 1.5),
                Vec3::new(bay - 0.10, 0.22, 3.20),
                SolidRole::ChurchVaultShell,
                vec![spring],
                geometry,
            );
            if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == vault) {
                item.crossfall_radians = side * 0.50;
            }
            choir_vault_solids.push(vault);
            for bearing_x in [west, east] {
                choir_vault_thrust_solids.push(solid(
                    Vec3::new(bearing_x, 7.35, 10.5 + side * 3.5),
                    Vec3::new(0.48, 0.34, 1.0),
                    SolidRole::ChurchVaultThrust,
                    vec![spring],
                    geometry,
                ));
            }
            for (bearing_index, (bearing_x, bearing_z)) in [
                (west, 10.5 + side * 3.0),
                (east, 10.5 + side * 3.0),
                (west, 10.5 + side * 4.0),
                (east, 10.5 + side * 4.0),
            ]
            .into_iter()
            .enumerate()
            {
                let interface = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (1_360
                            + bay_index as u64 * 32
                            + side_index as u64 * 8
                            + bearing_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface,
                    owner,
                    node: spring,
                    bounds: ResolvedBounds {
                        min: Vec3::new(bearing_x - 0.23, 7.15, bearing_z - 0.23),
                        max: Vec3::new(bearing_x + 0.23, 7.52, bearing_z + 0.23),
                    },
                });
                choir_vault_bearing_interfaces.push(interface);
            }
        }
        let surface = wall_surface(
            geometry,
            owner,
            next_slot.get(),
            ResolvedBounds {
                min: Vec3::new(west, 7.0, 7.5),
                max: Vec3::new(east, datum.vault_crown_metres, 13.5),
            },
            SurfaceRole::ChurchVaultLoad,
        );
        next_slot.set(next_slot.get() + 1);
        choir_vault_load_surfaces.push(surface);
        previous_choir_piers = current_piers;
        previous_choir_buttresses = current_buttresses;
    }

    let mut apse_facets = Vec::new();
    let mut radial_nodes = Vec::new();
    let mut radial_solids = Vec::new();
    let apse_centre = Vec2::new(choir_east, 10.5);
    let radius = 4.45_f32;
    for facet in 0..5_u8 {
        let angle0 = -std::f32::consts::FRAC_PI_2 + f32::from(facet) * std::f32::consts::PI / 5.0;
        let angle1 =
            -std::f32::consts::FRAC_PI_2 + f32::from(facet + 1) * std::f32::consts::PI / 5.0;
        let start = apse_centre + Vec2::new(angle0.cos(), angle0.sin()) * radius;
        let end = apse_centre + Vec2::new(angle1.cos(), angle1.sin()) * radius;
        let origin = (start + end) * 0.5;
        let tangent = (end - start).normalize();
        let mut outward = Vec2::new(tangent.y, -tangent.x);
        if outward.dot(origin - apse_centre) < 0.0 {
            outward = -outward;
        }
        let facet_length = start.distance(end);
        let angle = tangent.y.atan2(tangent.x);
        let wall_owner = owner;
        let support = StructuralNodeId(next_node.get());
        next_node.set(next_node.get() + 1);
        geometry.structural_nodes.push(StructuralNode {
            id: support,
            owner: wall_owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, 0.0, origin.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let id = crate::WallAssemblyId(7_100_000 + u64::from(facet));
        let host = wall_solid(
            geometry,
            wall_owner,
            0x600 + u64::from(facet),
            Vec3::new(origin.x, 5.675, origin.y),
            Vec3::new(facet_length, 11.35, 0.90),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            support,
        );
        if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == host) {
            item.yaw_radians = -angle;
        }
        apse_facets.push(id);
        let buttress_node = node(
            StructuralNodeKind::ChurchButtress,
            Vec3::new(origin.x, 0.0, origin.y),
            Vec::new(),
            true,
            geometry,
        );
        radial_nodes.push(buttress_node);
        let buttress = solid(
            Vec3::new(
                origin.x + outward.x * 1.075,
                3.6,
                origin.y + outward.y * 1.075,
            ),
            Vec3::new(0.72, 7.2, 1.25),
            SolidRole::WallButtress,
            vec![buttress_node],
            geometry,
        );
        if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == buttress) {
            item.yaw_radians = outward.x.atan2(outward.y);
        }
        radial_solids.push(buttress);
        walls.push(crate::WallAssembly {
            id,
            owner: wall_owner,
            source: crate::WallSourceId::ChurchApse { facet },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: facet_length,
            height_metres: 11.35,
            base_elevation_metres: 0.0,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::Buttressed,
            support_node: support,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
    }

    // One centered light per structural bay keeps the opening hierarchy tied
    // to the buttress/pier rhythm.  Transept end windows are deliberately
    // larger; the apse uses narrower radial lights.  Rich tracery is outside
    // the MVP, but every opening is already a real two-light stone assembly.
    let mut window_targets = Vec::new();
    for bay_index in 0..church_program.nave_bays {
        for side in [Direction::South, Direction::North] {
            window_targets.push((
                crate::WallSourceId::ChurchExterior {
                    range: crate::ChurchRange::Nave,
                    side,
                    bay: bay_index,
                },
                ChurchWindowProfile {
                    sill_metres: 1.70,
                    width_metres: 1.45,
                    spring_height_metres: 2.45,
                    apex_height_metres: 4.35,
                },
            ));
            window_targets.push((
                crate::WallSourceId::ChurchArcade {
                    side,
                    bay: bay_index,
                },
                ChurchWindowProfile {
                    // Clear the 8.635 m aisle-roof abutment and its upstand.
                    sill_metres: 9.10,
                    width_metres: 1.35,
                    spring_height_metres: 1.05,
                    apex_height_metres: 2.30,
                },
            ));
        }
    }
    for bay_index in 0..church_program.choir_bays {
        for side in [Direction::South, Direction::North] {
            window_targets.push((
                crate::WallSourceId::ChurchExterior {
                    range: crate::ChurchRange::Choir,
                    side,
                    bay: bay_index,
                },
                ChurchWindowProfile {
                    sill_metres: 2.15,
                    width_metres: 1.60,
                    spring_height_metres: 3.55,
                    apex_height_metres: 6.15,
                },
            ));
        }
    }
    for side in [Direction::South, Direction::North] {
        window_targets.push((
            crate::WallSourceId::ChurchExterior {
                range: crate::ChurchRange::Transept,
                side,
                bay: 0,
            },
            ChurchWindowProfile {
                sill_metres: 1.75,
                width_metres: 2.35,
                spring_height_metres: 4.35,
                apex_height_metres: 7.65,
            },
        ));
    }
    for facet in [0_u8, 1, 3, 4] {
        window_targets.push((
            crate::WallSourceId::ChurchApse { facet },
            ChurchWindowProfile {
                sill_metres: 2.20,
                width_metres: 1.30,
                spring_height_metres: 3.55,
                apex_height_metres: 5.95,
            },
        ));
    }
    for (serial, (source, profile)) in window_targets.into_iter().enumerate() {
        let wall = walls
            .iter_mut()
            .find(|wall| wall.source == source)
            .expect("church window host");
        let opening_id = crate::OpeningAssemblyId(7_500_000 + serial as u64);
        openings.push(resolve_church_pointed_window(
            wall,
            opening_id,
            serial as u64,
            profile,
            geometry,
        ));
    }
    for bay in &mut bay_assemblies {
        for (side_index, side) in [Direction::South, Direction::North].into_iter().enumerate() {
            bay.clerestory_openings[side_index] = openings
                .iter()
                .find(|opening| {
                    opening.host_source
                        == crate::WallSourceId::ChurchArcade {
                            side,
                            bay: bay.axis_index,
                        }
                })
                .expect("resolved clerestory light")
                .id;
        }
    }
    let choir = crate::ChurchChoirAssembly {
        bay_axes_metres: choir_axes_metres.clone(),
        pier_nodes: choir_pier_nodes,
        pier_solids: choir_pier_solids,
        buttress_nodes: choir_buttress_nodes,
        buttress_solids: choir_buttress_solids,
        arch_solids: choir_arch_solids,
        arch_bearing_nodes: choir_arch_bearing_nodes,
        arch_bearing_interfaces: choir_arch_bearing_interfaces,
        apse_facets,
        radial_buttress_nodes: radial_nodes,
        radial_buttress_solids: radial_solids,
        floor_solids: floor_solids.clone(),
        vault_solids: choir_vault_solids,
        vault_thrust_solids: choir_vault_thrust_solids,
        vault_load_surfaces: choir_vault_load_surfaces,
        vault_spring_nodes: choir_vault_spring_nodes,
        vault_bearing_interfaces: choir_vault_bearing_interfaces,
    };

    let mut tower_wall_supports = walls
        .iter()
        .filter(|wall| {
            matches!(
                wall.source,
                crate::WallSourceId::SquareTowerFace { .. }
                    | crate::WallSourceId::ChurchTowerFace { .. }
            )
        })
        .map(|wall| wall.support_node)
        .collect::<Vec<_>>();
    tower_wall_supports.sort_unstable_by_key(|id| id.0);
    tower_wall_supports.dedup();
    let bell_floor_node = node(
        StructuralNodeKind::ChurchTowerStage,
        Vec3::new(tower_centre.x, datum.bell_floor_metres, tower_centre.y),
        tower_wall_supports.clone(),
        false,
        geometry,
    );
    // The bell floor is a bearing ring, not a slab silently intersected by the
    // spiral.  A frozen 2.80 m square stairwell clears the 1.35 m outer tread
    // radius while the four surrounding slabs retain positive tower bearing.
    let outer = 4.25_f32;
    let stairwell = 2.45_f32;
    let ring = (outer - stairwell) * 0.5;
    let offset = (outer + stairwell) * 0.25;
    let mut bell_floor_solids = Vec::new();
    for (offset_x, offset_z, size_x, size_z) in [
        (0.0, -offset, outer, ring),
        (0.0, offset, outer, ring),
        (-offset, 0.0, ring, stairwell),
        (offset, 0.0, ring, stairwell),
    ] {
        bell_floor_solids.push(solid(
            Vec3::new(
                tower_centre.x + offset_x,
                datum.bell_floor_metres,
                tower_centre.y + offset_z,
            ),
            Vec3::new(size_x, 0.28, size_z),
            SolidRole::ChurchBellFloor,
            vec![bell_floor_node],
            geometry,
        ));
    }
    let frame_node = node(
        StructuralNodeKind::ChurchBellFrame,
        Vec3::new(
            tower_centre.x,
            datum.bell_floor_metres + 0.14,
            tower_centre.y,
        ),
        tower_wall_supports.clone(),
        false,
        geometry,
    );
    // Two wall-bearing cross beams are the accepted coarse bell frame.  The
    // earlier four-post cage consumed the only 0.90 m service ring; detailed
    // timber bracing remains an explicit visual refinement rather than a
    // false circulation obstacle.
    let bell_frame_solids = vec![
        solid(
            Vec3::new(
                tower_centre.x,
                datum.bell_floor_metres + 3.55,
                tower_centre.y,
            ),
            Vec3::new(4.50, 0.28, 0.30),
            SolidRole::ChurchBellFrame,
            vec![frame_node],
            geometry,
        ),
        solid(
            Vec3::new(
                tower_centre.x,
                datum.bell_floor_metres + 3.55,
                tower_centre.y,
            ),
            Vec3::new(0.30, 0.28, 4.50),
            SolidRole::ChurchBellFrame,
            vec![frame_node],
            geometry,
        ),
    ];
    let bell_solid = solid(
        Vec3::new(
            tower_centre.x,
            datum.bell_floor_metres + 2.85,
            tower_centre.y,
        ),
        Vec3::new(1.10, 1.00, 0.85),
        SolidRole::ChurchBell,
        vec![frame_node],
        geometry,
    );
    let stair_index = stairs.len();
    stairs.push(Stair::Spiral {
        centre: tower_centre,
        base_height_metres: 0.0,
        rise_metres: datum.bell_floor_metres,
        inner_radius_metres: 0.20,
        outer_radius_metres: 1.10,
        turns: 4.0,
        clockwise: true,
        tread_count: 72,
    });
    let stair_bearing_node = node(
        StructuralNodeKind::ChurchTowerStage,
        Vec3::new(tower_centre.x, 0.0, tower_centre.y),
        tower_wall_supports.clone(),
        false,
        geometry,
    );
    let stair_newel_solid = solid(
        Vec3::new(
            tower_centre.x,
            datum.bell_floor_metres * 0.5,
            tower_centre.y,
        ),
        Vec3::new(0.20, datum.bell_floor_metres + 0.5, 0.20),
        SolidRole::ChurchStairNewel,
        vec![stair_bearing_node],
        geometry,
    );
    let mut stair_tread_solids = Vec::new();
    let mut stair_tread_interfaces = Vec::new();
    for tread in 0..72_u16 {
        let progress = f32::from(tread) / 72.0;
        let angle = -progress * 4.0 * std::f32::consts::TAU;
        // The authoritative service line runs through the tread centre.  A
        // 0.95 m-wide tread centred between a 0.40 m newel and 1.35 m outer
        // radius left no physical 0.90 m occupant envelope at the newel.  The
        // A compact 0.20..1.10 m radial flight retains the full 0.90 m
        // project corridor while leaving a 0.90 m bearing ring inside the
        // authoritative tower shell.
        let radius = (0.20 + 1.10) * 0.5;
        let position = tower_centre + Vec2::new(angle.cos(), angle.sin()) * radius;
        let tread_id = solid(
            Vec3::new(position.x, progress * datum.bell_floor_metres, position.y),
            Vec3::new(0.90, 0.12, 0.34),
            SolidRole::ChurchStairTread,
            vec![stair_bearing_node],
            geometry,
        );
        if let Some(tread_solid) = geometry.solids.iter_mut().find(|item| item.id == tread_id) {
            tread_solid.yaw_radians = -angle;
        }
        let inner = tower_centre + Vec2::new(angle.cos(), angle.sin()) * 0.10;
        let interface_id =
            ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | (1_600 + u64::from(tread)));
        geometry.support_interfaces.push(SupportInterface {
            id: interface_id,
            owner,
            node: stair_bearing_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    inner.x - 0.11,
                    progress * datum.bell_floor_metres - 0.07,
                    inner.y - 0.11,
                ),
                max: Vec3::new(
                    inner.x + 0.11,
                    progress * datum.bell_floor_metres + 0.07,
                    inner.y + 0.11,
                ),
            },
        });
        stair_tread_solids.push(tread_id);
        stair_tread_interfaces.push(interface_id);
    }
    let mut landing_solids = Vec::new();
    let mut guard_solids = Vec::new();
    for (level, height) in [5.8_f32, 11.6, datum.bell_floor_metres]
        .into_iter()
        .enumerate()
    {
        let landing_angle = -(height / datum.bell_floor_metres) * 4.0 * std::f32::consts::TAU;
        let radial = Vec2::new(landing_angle.cos(), landing_angle.sin());
        let landing_plan = tower_centre + radial * 1.30;
        landing_solids.push(solid(
            Vec3::new(landing_plan.x, height, landing_plan.y),
            Vec3::new(1.20, 0.18, 1.20),
            SolidRole::Landing,
            if (height - datum.bell_floor_metres).abs() <= 0.05 {
                vec![bell_floor_node]
            } else {
                tower_wall_supports.clone()
            },
            geometry,
        ));
        if (height - datum.bell_floor_metres).abs() > 0.05 {
            let guard_plan = tower_centre + radial * 2.05;
            let guard = solid(
                Vec3::new(guard_plan.x, height + 0.55, guard_plan.y),
                Vec3::new(0.10, 1.10, 1.20),
                SolidRole::ChurchGuard,
                tower_wall_supports.clone(),
                geometry,
            );
            if let Some(guard_solid) = geometry.solids.iter_mut().find(|solid| solid.id == guard) {
                guard_solid.yaw_radians = -landing_angle;
            }
            guard_solids.push(guard);
        }
        let _ = level;
    }
    // Three sides of the bell-floor stairwell are protected; the east side is
    // the positive-width arrival from the landing.
    for (dx, dz, sx, sz) in [
        // West guard stops at the 0.90 m ladder transfer opening.
        (-1.175_f32, -0.45_f32, 0.10_f32, 1.45_f32),
        (0.0, -1.175, 2.35, 0.10),
        (0.0, 1.175, 2.35, 0.10),
    ] {
        guard_solids.push(solid(
            Vec3::new(
                tower_centre.x + dx,
                datum.bell_floor_metres + 0.55,
                tower_centre.y + dz,
            ),
            Vec3::new(sx, 1.10, sz),
            SolidRole::ChurchGuard,
            vec![bell_floor_node],
            geometry,
        ));
    }
    // A compact fixed ladder supplies the roof stage without forcing the bell
    // floor stair through the bell envelope. It is deliberately coarse MVP
    // service architecture rather than ornamental joinery.
    let mut roof_ladder_solids = Vec::new();
    let ladder_x = tower_centre.x - 1.65;
    let ladder_z = tower_centre.y + 1.0;
    let ladder_bottom = datum.bell_floor_metres + 0.18;
    let ladder_top = 21.30;
    for dz in [-0.38_f32, 0.38] {
        roof_ladder_solids.push(solid(
            Vec3::new(ladder_x, (ladder_bottom + ladder_top) * 0.5, ladder_z + dz),
            Vec3::new(0.10, ladder_top - ladder_bottom, 0.10),
            SolidRole::ChurchServiceLadder,
            vec![bell_floor_node],
            geometry,
        ));
    }
    let rung_count = 13_u8;
    for rung in 0..rung_count {
        let t = f32::from(rung) / f32::from(rung_count - 1);
        roof_ladder_solids.push(solid(
            Vec3::new(
                ladder_x,
                ladder_bottom + (ladder_top - ladder_bottom) * t,
                ladder_z,
            ),
            Vec3::new(0.10, 0.08, 0.98),
            SolidRole::ChurchServiceLadder,
            vec![bell_floor_node],
            geometry,
        ));
    }
    let bell_openings = openings
        .iter()
        .filter(|opening| opening.use_kind == crate::OpeningUse::BellOpening)
        .map(|opening| opening.id)
        .collect::<Vec<_>>();
    let roof_service_surface = wall_surface(
        geometry,
        owner,
        next_slot.get(),
        ResolvedBounds {
            min: Vec3::new(tower_centre.x - 1.5, 21.3, tower_centre.y - 1.5),
            max: Vec3::new(tower_centre.x + 1.5, 21.32, tower_centre.y + 1.5),
        },
        SurfaceRole::ChurchServiceRoute,
    );

    // Ground-level circulation is resolved as four physical patches.  The
    // portal edges span from opposite sides of each 0.90 m wall and are later
    // sampled through every sectional-void slice by the audit.  The 1.80 m
    // route width intentionally matches the tower doors; it is a project
    // processional-width gate, not a universal church dimension.
    let exterior_approach_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 1,
        ResolvedBounds {
            min: Vec3::new(tower_centre.x - 4.95, 0.20, tower_centre.y - 0.90),
            max: Vec3::new(tower_centre.x - 3.15, 0.22, tower_centre.y + 0.90),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let vestibule_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 2,
        ResolvedBounds {
            // The authoritative shared node is the clear east side of the
            // vestibule, beside (not through) the spiral newel.  Public
            // procession crosses it on axis while BellService turns here.
            min: Vec3::new(tower_centre.x + 0.20, 0.20, tower_centre.y - 0.48),
            max: Vec3::new(tower_centre.x + 1.10, 0.22, tower_centre.y + 0.48),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let nave_entry_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 3,
        ResolvedBounds {
            min: Vec3::new(tower_centre.x + 3.15, 0.20, tower_centre.y - 0.90),
            max: Vec3::new(tower_centre.x + 4.50, 0.22, tower_centre.y + 0.90),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let public_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 4,
        ResolvedBounds {
            min: Vec3::new(tower_centre.x + 3.15, 0.20, tower_centre.y - 0.90),
            max: Vec3::new(choir_east + radius - 0.4, 0.22, tower_centre.y + 0.90),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let ring_offset = 1.675_f32;
    let bell_floor_corner_surfaces = [
        (ring_offset, -ring_offset),
        (-ring_offset, -ring_offset),
        (ring_offset, ring_offset),
        (-ring_offset, ring_offset),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (dx, dz))| {
        wall_surface(
            geometry,
            owner,
            next_slot.get() + 10 + index as u64,
            ResolvedBounds {
                min: Vec3::new(
                    tower_centre.x + dx - 0.45,
                    datum.bell_floor_metres + 0.14,
                    tower_centre.y + dz - 0.45,
                ),
                max: Vec3::new(
                    tower_centre.x + dx + 0.45,
                    datum.bell_floor_metres + 0.16,
                    tower_centre.y + dz + 0.45,
                ),
            },
            SurfaceRole::ChurchServiceRoute,
        )
    })
    .collect::<Vec<_>>();
    let route_edge = |from, to, through_opening| crate::ChurchRouteEdge {
        from,
        to,
        clear_width_metres: 0.95,
        clear_headroom_metres: 2.0,
        through_opening,
    };
    let mut bell_route_edges = vec![route_edge(vestibule_surface, stair_tread_solids[0], None)];
    for pair in stair_tread_solids.windows(2) {
        bell_route_edges.push(route_edge(pair[0], pair[1], None));
    }
    for (landing_index, height) in [5.8_f32, 11.6].into_iter().enumerate() {
        let tread_index = ((height / datum.bell_floor_metres) * 72.0)
            .round()
            .clamp(1.0, 70.0) as usize;
        bell_route_edges.push(route_edge(
            stair_tread_solids[tread_index],
            landing_solids[landing_index],
            None,
        ));
        bell_route_edges.push(route_edge(
            landing_solids[landing_index],
            stair_tread_solids[tread_index + 1],
            None,
        ));
    }
    bell_route_edges.push(route_edge(
        *stair_tread_solids.last().expect("church stair tread"),
        landing_solids[2],
        None,
    ));
    // Ring indices are south, north, west, east. Corner surfaces keep both
    // protected ways around the stairwell on the physical bearing ring rather
    // than allowing one graph edge to cut diagonally through its void.
    bell_route_edges.push(route_edge(landing_solids[2], bell_floor_solids[3], None));
    bell_route_edges.push(route_edge(
        bell_floor_solids[3],
        bell_floor_corner_surfaces[0],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[0],
        bell_floor_solids[0],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_solids[0],
        bell_floor_corner_surfaces[1],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[1],
        bell_floor_solids[2],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_solids[3],
        bell_floor_corner_surfaces[2],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[2],
        bell_floor_solids[1],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_solids[1],
        bell_floor_corner_surfaces[3],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[3],
        bell_floor_solids[2],
        None,
    ));
    let ladder_rungs = roof_ladder_solids
        .iter()
        .copied()
        .skip(2)
        .collect::<Vec<_>>();
    bell_route_edges.push(route_edge(bell_floor_solids[2], ladder_rungs[0], None));
    for pair in ladder_rungs.windows(2) {
        bell_route_edges.push(route_edge(pair[0], pair[1], None));
    }
    bell_route_edges.push(route_edge(
        *ladder_rungs.last().expect("church roof ladder rung"),
        roof_service_surface,
        None,
    ));
    let mut bell_route_solids = stair_tread_solids.clone();
    bell_route_solids.extend(landing_solids.iter().copied());
    bell_route_solids.extend(bell_floor_solids.iter().copied());
    bell_route_solids.extend(ladder_rungs.iter().copied());
    let wall_ids = walls
        .iter()
        .filter(|wall| {
            matches!(
                wall.source,
                crate::WallSourceId::SquareTowerFace { .. }
                    | crate::WallSourceId::ChurchTowerFace { .. }
            )
        })
        .map(|wall| wall.id)
        .collect();
    let tower = crate::ChurchTowerAssembly {
        centre: tower_centre,
        footprint_size_metres: tower_size,
        wall_ids,
        west_portal,
        nave_passage,
        exterior_approach_surface,
        vestibule_surface,
        nave_entry_surface,
        stair_index,
        stair_bearing_node,
        stair_newel_solid,
        stair_tread_solids: stair_tread_solids.clone(),
        stair_tread_interfaces,
        landing_solids,
        guard_solids,
        bell_floor_solids,
        bell_floor_corner_surfaces: bell_floor_corner_surfaces.clone(),
        bell_frame_solids,
        bell_solid,
        bell_openings,
        roof_ladder_solids,
        roof_service_surface,
    };
    crate::ChurchAssembly {
        id: crate::ChurchAssemblyId(1),
        program: church_program,
        datum,
        west_elevation_metres: 0.0,
        nave_axes_metres,
        crossing_axis_metres,
        choir_axes_metres,
        bay_assemblies,
        crossing,
        choir,
        tower,
        circulation: vec![
            crate::ChurchCirculationRoute {
                kind: crate::ChurchRouteKind::PublicProcessional,
                waypoints: vec![
                    Vec3::new(tower_centre.x - 4.05, 0.20, tower_centre.y),
                    Vec3::new(tower_centre.x, 0.20, tower_centre.y),
                    Vec3::new(tower_centre.x + 3.825, 0.20, tower_centre.y),
                    Vec3::new(choir_east + radius - 0.4, 0.20, 10.5),
                ],
                width_metres: 1.80,
                headroom_metres: 2.95,
                surface_ids: vec![
                    exterior_approach_surface,
                    vestibule_surface,
                    nave_entry_surface,
                    public_surface,
                ],
                traversable_solid_ids: Vec::new(),
                edges: vec![
                    crate::ChurchRouteEdge {
                        from: exterior_approach_surface,
                        to: vestibule_surface,
                        clear_width_metres: 1.80,
                        clear_headroom_metres: 2.95,
                        through_opening: Some(west_portal),
                    },
                    crate::ChurchRouteEdge {
                        from: vestibule_surface,
                        to: nave_entry_surface,
                        clear_width_metres: 1.80,
                        clear_headroom_metres: 2.95,
                        through_opening: Some(nave_passage),
                    },
                    crate::ChurchRouteEdge {
                        from: nave_entry_surface,
                        to: public_surface,
                        clear_width_metres: 1.80,
                        clear_headroom_metres: 2.95,
                        through_opening: None,
                    },
                ],
                opening_ids: vec![west_portal, nave_passage],
            },
            crate::ChurchCirculationRoute {
                kind: crate::ChurchRouteKind::BellService,
                waypoints: vec![
                    Vec3::new(tower_centre.x, 0.20, tower_centre.y),
                    Vec3::new(tower_centre.x, datum.bell_floor_metres, tower_centre.y),
                ],
                width_metres: 0.95,
                headroom_metres: 2.0,
                surface_ids: std::iter::once(vestibule_surface)
                    .chain(bell_floor_corner_surfaces.iter().copied())
                    .chain(std::iter::once(roof_service_surface))
                    .collect(),
                traversable_solid_ids: bell_route_solids,
                edges: bell_route_edges,
                opening_ids: Vec::new(),
            },
        ],
        floor_solids,
        roof_assemblies: Vec::new(),
    }
}
