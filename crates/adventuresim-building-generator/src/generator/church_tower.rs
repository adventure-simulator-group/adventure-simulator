fn suppress_cathedral_legacy_storey_walls(
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    let removed_owners = walls
        .iter()
        .filter(|wall| matches!(wall.source, crate::WallSourceId::StoreyWall { .. }))
        .map(|wall| wall.owner)
        .collect::<HashSet<_>>();
    walls.retain(|wall| !removed_owners.contains(&wall.owner));
    openings.retain(|opening| !removed_owners.contains(&opening.owner));
    geometry
        .solids
        .retain(|solid| !removed_owners.contains(&solid.owner));
    geometry
        .surfaces
        .retain(|surface| !removed_owners.contains(&surface.owner));
    geometry
        .voids
        .retain(|void| !removed_owners.contains(&void.owner));
    geometry
        .structural_nodes
        .retain(|node| !removed_owners.contains(&node.owner));
    geometry
        .support_interfaces
        .retain(|interface| !removed_owners.contains(&interface.owner));
}

fn resolve_church_tower_door_wall(
    face: Direction,
    opening_id: crate::OpeningAssemblyId,
    wall_id: crate::WallAssemblyId,
    owner: GeometryOwnerId,
    centre: Vec2,
    geometry: &mut ResolvedGeometry,
) -> (crate::WallAssembly, crate::OpeningAssembly) {
    let outward = direction_vector(face);
    let tangent = if outward.y.abs() > 0.5 {
        Vec2::X
    } else {
        Vec2::Y
    };
    let origin = centre + outward * 2.70;
    let length = 4.50_f32;
    let thickness = 0.90_f32;
    let height = 17.30_f32;
    let width = 1.80_f32;
    let clear_height = 3.20_f32;
    let wall_node = StructuralNodeId(7_500_000 + u64::from(owner.0) * 8);
    let jamb_nodes = [
        StructuralNodeId(wall_node.0 + 1),
        StructuralNodeId(wall_node.0 + 2),
    ];
    let head_node = StructuralNodeId(wall_node.0 + 3);
    let spandrel_node = StructuralNodeId(wall_node.0 + 4);
    geometry.structural_nodes.push(StructuralNode {
        id: wall_node,
        owner,
        kind: StructuralNodeKind::WallBearing,
        position: Vec3::new(origin.x, 0.0, origin.y),
        supported_by: Vec::new(),
        grounded: true,
    });
    for (index, node_id) in jamb_nodes.into_iter().enumerate() {
        let side = if index == 0 { -1.0 } else { 1.0 };
        geometry.structural_nodes.push(StructuralNode {
            id: node_id,
            owner,
            kind: StructuralNodeKind::OpeningJamb,
            position: Vec3::new(
                origin.x + tangent.x * side * width * 0.5,
                0.0,
                origin.y + tangent.y * side * width * 0.5,
            ),
            supported_by: vec![wall_node],
            grounded: false,
        });
    }
    geometry.structural_nodes.push(StructuralNode {
        id: head_node,
        owner,
        kind: StructuralNodeKind::OpeningHead,
        position: Vec3::new(origin.x, clear_height, origin.y),
        supported_by: jamb_nodes.to_vec(),
        grounded: false,
    });
    geometry.structural_nodes.push(StructuralNode {
        id: spandrel_node,
        owner,
        kind: StructuralNodeKind::OpeningSpandrel,
        position: Vec3::new(origin.x, clear_height + 0.35, origin.y),
        supported_by: vec![head_node],
        grounded: false,
    });
    let side_width = (length - width) * 0.5;
    let mut jamb_solids = [ResolvedItemId(0); 2];
    let mut host_solids = Vec::new();
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let position = origin + tangent * side * (width * 0.5 + side_width * 0.5);
        jamb_solids[index] = wall_solid(
            geometry,
            owner,
            index as u64,
            Vec3::new(position.x, height * 0.5, position.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(side_width, height, thickness)
            } else {
                Vec3::new(thickness, height, side_width)
            },
            SolidRole::OpeningJamb,
            crate::ResolvedSolidShape::Cuboid,
            jamb_nodes[index],
        );
        host_solids.push(jamb_solids[index]);
    }
    let head_solid = wall_solid(
        geometry,
        owner,
        2,
        Vec3::new(origin.x, clear_height + 0.175, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width + 0.30, 0.35, thickness)
        } else {
            Vec3::new(thickness, 0.35, width + 0.30)
        },
        SolidRole::OpeningHead,
        crate::ResolvedSolidShape::Cuboid,
        head_node,
    );
    host_solids.push(head_solid);
    let spandrel_bottom = clear_height + 0.325;
    let spandrel_height = height - spandrel_bottom;
    let spandrel_solid = wall_solid(
        geometry,
        owner,
        3,
        Vec3::new(origin.x, spandrel_bottom + spandrel_height * 0.5, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width, spandrel_height, thickness)
        } else {
            Vec3::new(thickness, spandrel_height, width)
        },
        SolidRole::OpeningSpandrel,
        crate::ResolvedSolidShape::Cuboid,
        spandrel_node,
    );
    host_solids.push(spandrel_solid);
    let half_tangent = tangent.abs() * (width * 0.5);
    let half_depth = outward.abs() * (thickness * 0.55);
    let depth_sign = if tangent.x.abs() > 0.5 {
        if outward.y >= 0.0 { 1 } else { -1 }
    } else if outward.x <= 0.0 {
        1
    } else {
        -1
    };
    let void_id = wall_void(
        geometry,
        owner,
        0,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                0.0,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                clear_height,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        opening_id,
        width,
        width,
        clear_height,
        clear_height,
        depth_sign,
    );
    let mut reveal_surfaces = Vec::new();
    for (slot, side, role) in [
        (10_u64, -1.0_f32, SurfaceRole::LeftJambReveal),
        (11, 1.0, SurfaceRole::RightJambReveal),
    ] {
        let plan = origin + tangent * side * width * 0.5;
        let extent = outward.abs() * thickness * 0.5 + tangent.abs() * 0.015;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            slot,
            ResolvedBounds {
                min: Vec3::new(plan.x - extent.x, 0.0, plan.y - extent.y),
                max: Vec3::new(plan.x + extent.x, clear_height, plan.y + extent.y),
            },
            role,
            crate::ResolvedSurfaceShape::SplayedJamb {
                side: side as i8,
                exterior_width_metres: width,
                interior_width_metres: width,
                exterior_depth_sign: depth_sign,
            },
        ));
    }
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        12,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                clear_height - 0.02,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                clear_height + 0.02,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::Intrados,
        crate::ResolvedSurfaceShape::Planar,
    ));
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        15,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                -0.025,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                0.025,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::WeatherSill,
        crate::ResolvedSurfaceShape::WeatherSill {
            interior_elevation_metres: 0.02,
            exterior_elevation_metres: -0.02,
            drip_depth_metres: 0.025,
        },
    ));
    for (slot, sign, role) in [
        (13_u64, 1.0_f32, SurfaceRole::ExteriorThroat),
        (14, -1.0, SurfaceRole::InteriorMouth),
    ] {
        let plan = origin + outward * thickness * 0.5 * sign;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            slot,
            ResolvedBounds {
                min: Vec3::new(
                    plan.x - half_tangent.x - 0.006,
                    0.0,
                    plan.y - half_tangent.y - 0.006,
                ),
                max: Vec3::new(
                    plan.x + half_tangent.x + 0.006,
                    clear_height,
                    plan.y + half_tangent.y + 0.006,
                ),
            },
            role,
            crate::ResolvedSurfaceShape::Planar,
        ));
    }
    let leaf_plan = origin - outward * thickness * 0.20;
    let closure_solid = wall_solid(
        geometry,
        owner,
        20,
        Vec3::new(leaf_plan.x, clear_height * 0.5, leaf_plan.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width * 0.94, clear_height * 0.96, 0.06)
        } else {
            Vec3::new(0.06, clear_height * 0.96, width * 0.94)
        },
        SolidRole::OpeningClosure,
        crate::ResolvedSolidShape::Cuboid,
        jamb_nodes[0],
    );
    let bearing_width = 0.15_f32;
    let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
        let slot = if side < 0.0 { 50 } else { 51 };
        let plan = origin + tangent * side * (width * 0.5 + bearing_width * 0.5);
        let extent = tangent.abs() * bearing_width * 0.5 + outward.abs() * thickness * 0.5;
        let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot);
        geometry.support_interfaces.push(SupportInterface {
            id,
            owner,
            node: head_node,
            bounds: ResolvedBounds {
                min: Vec3::new(plan.x - extent.x, clear_height - 0.025, plan.y - extent.y),
                max: Vec3::new(plan.x + extent.x, clear_height + 0.025, plan.y + extent.y),
            },
        });
        id
    });
    let wall_above_interface = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 52);
    geometry.support_interfaces.push(SupportInterface {
        id: wall_above_interface,
        owner,
        node: spandrel_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                clear_height + 0.325,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                clear_height + 0.375,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
    });
    let source = crate::WallSourceId::ChurchTowerFace {
        face,
        stage: crate::ChurchTowerStage::Portal,
        bay: 0,
    };
    let wall = crate::WallAssembly {
        id: wall_id,
        owner,
        source,
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
        height_metres: height,
        base_elevation_metres: 0.0,
        thickness_metres: thickness,
        structural_role: crate::WallStructuralRole::LoadBearing,
        support_node: wall_node,
        host_solids,
        opening_ids: vec![opening_id],
        replaced_by_owner: None,
    };
    let opening = crate::OpeningAssembly {
        id: opening_id,
        owner,
        host_wall: wall_id,
        host_source: source,
        frame: wall.frame,
        use_kind: crate::OpeningUse::Door,
        profile: crate::OpeningProfile::Rectangular {
            width_metres: width,
            height_metres: clear_height,
        },
        sill_elevation_metres: 0.0,
        closure: crate::ClosurePolicy {
            layers: vec![crate::ClosureKind::DoorLeaf],
            state: crate::ClosureState::Operable,
            thickness_metres: 0.06,
            swing_clearance_metres: 1.0,
        },
        head_kind: crate::OpeningHeadKind::StoneLintel,
        void_id,
        jamb_solids,
        sill_solid: None,
        head_solid,
        spandrel_solid,
        reveal_surfaces,
        closure_solids: vec![closure_solid],
        jamb_nodes,
        head_node,
        spandrel_node,
        tracery_node: None,
        stance_surface: None,
        mount_solid: None,
        ray_indices: Vec::new(),
        sectional_void: (0..=8)
            .map(|index| crate::OpeningVoidSlice {
                depth_fraction: index as f32 / 8.0,
                width_metres: width,
                height_metres: clear_height,
            })
            .collect(),
        head_bearing_interfaces,
        wall_above_interface,
    };
    (wall, opening)
}

fn resolve_cathedral_bell_stage(
    towers: &[SquareTower],
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    for (tower_index, tower) in towers
        .iter()
        .enumerate()
        .filter(|(_, tower)| tower.bell_openings)
    {
        let stage_height = 4.2_f32;
        let base = tower.wall_height_metres - stage_height;
        let thickness = 0.90_f32;
        for (face_index, face) in [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ]
        .into_iter()
        .enumerate()
        {
            let outward = direction_vector(face);
            let tangent = if outward.y.abs() > 0.5 {
                Vec2::X
            } else {
                Vec2::Y
            };
            let face_span = if tangent.x.abs() > 0.5 {
                tower.size.x
            } else {
                tower.size.y
            };
            let depth_span = if outward.x.abs() > 0.5 {
                tower.size.x
            } else {
                tower.size.y
            };
            let bay_length = face_span * 0.5;
            for bay in 0..2_u8 {
                let serial = (tower_index * 8 + face_index * 2 + usize::from(bay)) as u64;
                let wall_id = crate::WallAssemblyId(50_000 + serial);
                let opening_id = crate::OpeningAssemblyId(50_000 + serial);
                let owner = GeometryOwnerId(45_000 + serial as u32);
                let wall_node = StructuralNodeId(3_000_000 + serial * 8);
                let bay_sign = if bay == 0 { -1.0 } else { 1.0 };
                let origin = tower.centre
                    + outward * (depth_span * 0.5)
                    + tangent * (bay_sign * bay_length * 0.5);
                geometry.structural_nodes.push(StructuralNode {
                    id: wall_node,
                    owner,
                    kind: StructuralNodeKind::WallBearing,
                    position: Vec3::new(origin.x, 0.0, origin.y),
                    supported_by: Vec::new(),
                    grounded: true,
                });
                let width = 1.15_f32;
                let sill = 0.45_f32;
                let spring = 2.10_f32;
                let apex = 3.35_f32;
                let radius = two_centred_arc_radius(width, apex - spring);
                let profile = crate::OpeningProfile::PointedTwoCentred {
                    width_metres: width,
                    spring_height_metres: spring,
                    apex_height_metres: apex,
                    arc_radius_metres: radius,
                };
                let jamb_nodes = [
                    StructuralNodeId(wall_node.0 + 1),
                    StructuralNodeId(wall_node.0 + 2),
                ];
                for (side, node) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
                    geometry.structural_nodes.push(StructuralNode {
                        id: node,
                        owner,
                        kind: StructuralNodeKind::OpeningJamb,
                        position: Vec3::new(
                            origin.x + tangent.x * side * width * 0.5,
                            base,
                            origin.y + tangent.y * side * width * 0.5,
                        ),
                        supported_by: vec![wall_node],
                        grounded: false,
                    });
                }
                let head_node = StructuralNodeId(wall_node.0 + 3);
                geometry.structural_nodes.push(StructuralNode {
                    id: head_node,
                    owner,
                    kind: StructuralNodeKind::OpeningHead,
                    position: Vec3::new(origin.x, base + sill + apex, origin.y),
                    supported_by: jamb_nodes.to_vec(),
                    grounded: false,
                });
                let spandrel_node = StructuralNodeId(wall_node.0 + 4);
                geometry.structural_nodes.push(StructuralNode {
                    id: spandrel_node,
                    owner,
                    kind: StructuralNodeKind::OpeningSpandrel,
                    position: Vec3::new(origin.x, base + stage_height, origin.y),
                    supported_by: vec![head_node],
                    grounded: false,
                });
                let side_width = (bay_length - width) * 0.5;
                let mut jamb_solids = [ResolvedItemId::default(); 2];
                let mut host_solids = Vec::new();
                // The tower/nave weather junction lies below the bell stage.
                // Resolve that upper shaft as part of the same bay authority;
                // the lower eight metres remain the existing monolithic tower
                // base and do not need opening subdivision.
                let shaft_base = 8.0_f32;
                let shaft_height = base - shaft_base;
                let shaft_solid = wall_solid(
                    geometry,
                    owner,
                    60,
                    Vec3::new(origin.x, shaft_base + shaft_height * 0.5, origin.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(bay_length, shaft_height, thickness)
                    } else {
                        Vec3::new(thickness, shaft_height, bay_length)
                    },
                    SolidRole::WallHost,
                    crate::ResolvedSolidShape::Cuboid,
                    wall_node,
                );
                host_solids.push(shaft_solid);
                for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
                    let plan = origin + tangent * side * (width + side_width) * 0.5;
                    let id = wall_solid(
                        geometry,
                        owner,
                        index as u64,
                        Vec3::new(plan.x, base + stage_height * 0.5, plan.y),
                        if tangent.x.abs() > 0.5 {
                            Vec3::new(side_width, stage_height, thickness)
                        } else {
                            Vec3::new(thickness, stage_height, side_width)
                        },
                        SolidRole::OpeningJamb,
                        crate::ResolvedSolidShape::Cuboid,
                        jamb_nodes[index],
                    );
                    jamb_solids[index] = id;
                    host_solids.push(id);
                }
                let sill_solid = wall_solid(
                    geometry,
                    owner,
                    2,
                    Vec3::new(origin.x, base + sill * 0.5, origin.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(width, sill, thickness)
                    } else {
                        Vec3::new(thickness, sill, width)
                    },
                    SolidRole::OpeningSill,
                    crate::ResolvedSolidShape::Cuboid,
                    wall_node,
                );
                host_solids.push(sill_solid);
                let bearing_width = 0.12_f32;
                let header_base = sill + spring;
                let head_top = sill + apex + 0.20;
                let header_height = head_top - header_base;
                let head_solid = wall_solid(
                    geometry,
                    owner,
                    3,
                    Vec3::new(origin.x, base + header_base + header_height * 0.5, origin.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(width + bearing_width * 2.0, header_height, thickness)
                    } else {
                        Vec3::new(thickness, header_height, width + bearing_width * 2.0)
                    },
                    SolidRole::OpeningHead,
                    crate::ResolvedSolidShape::PointedArchRing {
                        clear_span_metres: width,
                        spring_height_metres: spring,
                        apex_height_metres: apex,
                        arc_radius_metres: radius,
                        ring_depth_metres: 0.22,
                    },
                    head_node,
                );
                host_solids.push(head_solid);
                let spandrel_bottom = head_top - 0.025;
                let spandrel_height = stage_height - spandrel_bottom;
                let spandrel_solid = wall_solid(
                    geometry,
                    owner,
                    4,
                    Vec3::new(
                        origin.x,
                        base + spandrel_bottom + spandrel_height * 0.5,
                        origin.y,
                    ),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(width + bearing_width * 2.0, spandrel_height, thickness)
                    } else {
                        Vec3::new(thickness, spandrel_height, width + bearing_width * 2.0)
                    },
                    SolidRole::OpeningSpandrel,
                    crate::ResolvedSolidShape::Cuboid,
                    spandrel_node,
                );
                host_solids.push(spandrel_solid);
                let half_tangent = tangent.abs() * (width * 0.5);
                let half_depth = outward.abs() * (thickness * 0.55);
                let depth_sign = if tangent.x.abs() > 0.5 {
                    if outward.y >= 0.0 { 1 } else { -1 }
                } else if outward.x <= 0.0 {
                    1
                } else {
                    -1
                };
                let void_id = wall_void(
                    geometry,
                    owner,
                    0,
                    ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - half_depth.x,
                            base + sill,
                            origin.y - half_tangent.y - half_depth.y,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + half_depth.x,
                            base + sill + apex,
                            origin.y + half_tangent.y + half_depth.y,
                        ),
                    },
                    opening_id,
                    width,
                    width,
                    apex,
                    apex,
                    depth_sign,
                );
                let mut reveal_surfaces = Vec::new();
                for (slot, side, role) in [
                    (10_u64, -1_i8, SurfaceRole::LeftJambReveal),
                    (11, 1, SurfaceRole::RightJambReveal),
                ] {
                    let plan = origin + tangent * (f32::from(side) * width * 0.5);
                    let hd = outward.abs() * (thickness * 0.5);
                    let hr = tangent.abs() * 0.015;
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        slot,
                        ResolvedBounds {
                            min: Vec3::new(plan.x - hd.x - hr.x, base + sill, plan.y - hd.y - hr.y),
                            max: Vec3::new(
                                plan.x + hd.x + hr.x,
                                base + sill + apex,
                                plan.y + hd.y + hr.y,
                            ),
                        },
                        role,
                        crate::ResolvedSurfaceShape::SplayedJamb {
                            side,
                            exterior_width_metres: width,
                            interior_width_metres: width,
                            exterior_depth_sign: depth_sign,
                        },
                    ));
                }
                reveal_surfaces.push(wall_shaped_surface(
                    geometry,
                    owner,
                    12,
                    ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - half_depth.x,
                            base + sill - 0.035,
                            origin.y - half_tangent.y - half_depth.y,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + half_depth.x,
                            base + sill + 0.015,
                            origin.y + half_tangent.y + half_depth.y,
                        ),
                    },
                    SurfaceRole::WeatherSill,
                    crate::ResolvedSurfaceShape::WeatherSill {
                        interior_elevation_metres: base + sill,
                        exterior_elevation_metres: base + sill - 0.035,
                        drip_depth_metres: 0.025,
                    },
                ));
                reveal_surfaces.push(wall_shaped_surface(
                    geometry,
                    owner,
                    13,
                    ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - half_depth.x,
                            base + sill + spring - 0.015,
                            origin.y - half_tangent.y - half_depth.y,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + half_depth.x,
                            base + sill + apex,
                            origin.y + half_tangent.y + half_depth.y,
                        ),
                    },
                    SurfaceRole::Intrados,
                    crate::ResolvedSurfaceShape::PointedIntrados {
                        clear_span_metres: width,
                        spring_height_metres: spring,
                        apex_height_metres: apex,
                        arc_radius_metres: radius,
                    },
                ));
                for (slot, sign, role) in [
                    (14_u64, 1.0_f32, SurfaceRole::ExteriorThroat),
                    (15, -1.0, SurfaceRole::InteriorMouth),
                ] {
                    let face_plan = origin + outward * (thickness * 0.5 * sign);
                    let hf = outward.abs() * 0.006;
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        slot,
                        ResolvedBounds {
                            min: Vec3::new(
                                face_plan.x - half_tangent.x - hf.x,
                                base + sill,
                                face_plan.y - half_tangent.y - hf.y,
                            ),
                            max: Vec3::new(
                                face_plan.x + half_tangent.x + hf.x,
                                base + sill + apex,
                                face_plan.y + half_tangent.y + hf.y,
                            ),
                        },
                        role,
                        crate::ResolvedSurfaceShape::Planar,
                    ));
                }
                let mut closure_solids = Vec::new();
                for (index, height) in [0.75_f32, 1.25, 1.75, 2.25].into_iter().enumerate() {
                    closure_solids.push(wall_solid(
                        geometry,
                        owner,
                        20 + index as u64,
                        Vec3::new(
                            origin.x - outward.x * thickness * 0.20,
                            base + sill + height,
                            origin.y - outward.y * thickness * 0.20,
                        ),
                        if tangent.x.abs() > 0.5 {
                            Vec3::new(width * 0.82, 0.10, 0.09)
                        } else {
                            Vec3::new(0.09, 0.10, width * 0.82)
                        },
                        SolidRole::OpeningClosure,
                        crate::ResolvedSolidShape::Cuboid,
                        head_node,
                    ));
                }
                let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
                    let slot = if side < 0.0 { 50 } else { 51 };
                    let centre_plan = origin + tangent * side * (width * 0.5 + bearing_width * 0.5);
                    let extent =
                        tangent.abs() * (bearing_width * 0.5) + outward.abs() * (thickness * 0.5);
                    let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot);
                    geometry.support_interfaces.push(SupportInterface {
                        id,
                        owner,
                        node: head_node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                centre_plan.x - extent.x,
                                base + header_base - 0.025,
                                centre_plan.y - extent.y,
                            ),
                            max: Vec3::new(
                                centre_plan.x + extent.x,
                                base + header_base + 0.025,
                                centre_plan.y + extent.y,
                            ),
                        },
                    });
                    id
                });
                let wall_above_interface =
                    ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 52);
                geometry.support_interfaces.push(SupportInterface {
                    id: wall_above_interface,
                    owner,
                    node: spandrel_node,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - outward.x.abs() * thickness * 0.5,
                            base + head_top - 0.025,
                            origin.y - half_tangent.y - outward.y.abs() * thickness * 0.5,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + outward.x.abs() * thickness * 0.5,
                            base + head_top + 0.025,
                            origin.y + half_tangent.y + outward.y.abs() * thickness * 0.5,
                        ),
                    },
                });
                openings.push(crate::OpeningAssembly {
                    id: opening_id,
                    owner,
                    host_wall: wall_id,
                    host_source: crate::WallSourceId::SquareTowerFace {
                        tower_index,
                        face,
                        bay,
                    },
                    frame: crate::WallLocalFrame {
                        origin,
                        tangent,
                        outward,
                        inside_room: None,
                        outside_room: None,
                    },
                    use_kind: crate::OpeningUse::BellOpening,
                    profile,
                    sill_elevation_metres: base + sill,
                    closure: crate::ClosurePolicy {
                        layers: vec![crate::ClosureKind::TimberLouvre],
                        state: crate::ClosureState::Open,
                        thickness_metres: 0.08,
                        swing_clearance_metres: 0.0,
                    },
                    head_kind: crate::OpeningHeadKind::PointedVoussoir,
                    void_id,
                    jamb_solids,
                    sill_solid: Some(sill_solid),
                    head_solid,
                    spandrel_solid,
                    reveal_surfaces,
                    closure_solids,
                    jamb_nodes,
                    head_node,
                    spandrel_node,
                    tracery_node: None,
                    stance_surface: None,
                    mount_solid: None,
                    ray_indices: Vec::new(),
                    sectional_void: (0..=8)
                        .map(|index| crate::OpeningVoidSlice {
                            depth_fraction: index as f32 / 8.0,
                            width_metres: width,
                            height_metres: apex,
                        })
                        .collect(),
                    head_bearing_interfaces,
                    wall_above_interface,
                });
                walls.push(crate::WallAssembly {
                    id: wall_id,
                    owner,
                    source: crate::WallSourceId::SquareTowerFace {
                        tower_index,
                        face,
                        bay,
                    },
                    material: crate::WallMaterialClass::CathedralMasonry,
                    storey_level: 2,
                    frame: crate::WallLocalFrame {
                        origin,
                        tangent,
                        outward,
                        inside_room: None,
                        outside_room: None,
                    },
                    radial_frame: None,
                    length_metres: bay_length,
                    height_metres: tower.wall_height_metres - 8.0,
                    base_elevation_metres: 8.0,
                    thickness_metres: thickness,
                    structural_role: crate::WallStructuralRole::LoadBearing,
                    support_node: wall_node,
                    host_solids,
                    opening_ids: vec![opening_id],
                    replaced_by_owner: None,
                });
            }
        }
    }
}
