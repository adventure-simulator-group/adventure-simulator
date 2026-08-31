fn resolve_storey_wall_assemblies(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    projected_defenses: &[ProjectedDefenseAssembly],
    geometry: &mut ResolvedGeometry,
) -> (Vec<crate::WallAssembly>, Vec<crate::OpeningAssembly>) {
    let mut walls_out = Vec::new();
    let mut openings_out = Vec::new();
    let mut global_index = 0_u64;
    for storey in storeys {
        let base = f32::from(storey.level) * program.storey_height_metres;
        for (wall_index, wall) in storey.walls.iter().copied().enumerate() {
            let id = crate::WallAssemblyId(global_index + 1);
            let owner = GeometryOwnerId(30_000 + global_index as u32);
            let source = crate::WallSourceId::StoreyWall {
                storey_level: storey.level,
                wall_index,
            };
            let outward = direction_vector(wall.direction);
            let tangent = if wall.is_horizontal() {
                Vec2::X
            } else {
                Vec2::Y
            };
            let projection = if wall.exterior() {
                program.upper_storey_projection_metres * f32::from(storey.level.min(1))
            } else {
                0.0
            };
            let origin = wall.centre() + outward * projection;
            let (material, structural_role, thickness) =
                wall_material_and_thickness(program.archetype, wall.exterior(), storey.level);
            let wall_node = StructuralNodeId(2_000_000 + global_index * 8);
            geometry.structural_nodes.push(StructuralNode {
                id: wall_node,
                owner,
                kind: StructuralNodeKind::WallBearing,
                position: Vec3::new(origin.x, base, origin.y),
                supported_by: Vec::new(),
                grounded: true,
            });
            let replacement = projected_defenses.iter().find(|defense| {
                defense.host_source_walls.iter().any(|candidate| {
                    candidate.storey_level == storey.level && candidate.wall_index == wall_index
                })
            });
            let source_opening = storey
                .openings
                .iter()
                .copied()
                .find(|opening| opening.wall == wall_index);
            let mut host_solids = Vec::new();
            let mut opening_ids = Vec::new();
            if replacement.is_none() {
                if let Some(opening) = source_opening {
                    let opening_id = crate::OpeningAssemblyId(global_index + 1);
                    opening_ids.push(opening_id);
                    let (use_kind, mut profile, head_kind) =
                        opening_profile_for(program.archetype, opening);
                    if use_kind == crate::OpeningUse::Window {
                        let maximum_bay_width = if program.archetype == BuildingArchetype::Cathedral
                        {
                            // Buttressed cathedral bays carry their opening at
                            // the bay divisions; wall thickness is depth, not a
                            // subtraction from the clear facade span.
                            CELL_SIZE_METRES - 0.30
                        } else {
                            (CELL_SIZE_METRES - thickness).max(0.35)
                        };
                        profile = match profile {
                            crate::OpeningProfile::Rectangular {
                                width_metres,
                                height_metres,
                            } => crate::OpeningProfile::Rectangular {
                                width_metres: width_metres.min(maximum_bay_width),
                                height_metres,
                            },
                            crate::OpeningProfile::Segmental {
                                width_metres,
                                spring_height_metres,
                                rise_metres,
                                intrados_depth_metres,
                            } => crate::OpeningProfile::Segmental {
                                width_metres: width_metres.min(maximum_bay_width),
                                spring_height_metres,
                                rise_metres,
                                intrados_depth_metres,
                            },
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                ..
                            } => crate::OpeningProfile::PointedTwoCentred {
                                width_metres: width_metres.min(maximum_bay_width),
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres: two_centred_arc_radius(
                                    width_metres.min(maximum_bay_width),
                                    apex_height_metres - spring_height_metres,
                                ),
                            },
                            other => other,
                        };
                    }
                    let mut mouth_width = profile.interior_width_metres().min(1.30);
                    let mut exterior_width = profile.exterior_width_metres().min(mouth_width);
                    let endpoint_bearing_depth = |sign: f32| {
                        let endpoint = wall.centre() + tangent * sign * CELL_SIZE_METRES * 0.5;
                        storey
                            .walls
                            .iter()
                            .enumerate()
                            .filter_map(|(other_index, other)| {
                                if other_index == wall_index
                                    || wall.is_horizontal() == other.is_horizontal()
                                {
                                    return None;
                                }
                                let other_tangent = if other.is_horizontal() {
                                    Vec2::X
                                } else {
                                    Vec2::Y
                                };
                                [
                                    other.centre() - other_tangent * CELL_SIZE_METRES * 0.5,
                                    other.centre() + other_tangent * CELL_SIZE_METRES * 0.5,
                                ]
                                .into_iter()
                                .any(|candidate| candidate.distance(endpoint) <= 0.02)
                                .then(|| {
                                    wall_material_and_thickness(
                                        program.archetype,
                                        other.exterior(),
                                        storey.level,
                                    )
                                    .2
                                })
                            })
                            .fold(0.0_f32, f32::max)
                    };
                    let negative_bearing = endpoint_bearing_depth(-1.0);
                    let positive_bearing = endpoint_bearing_depth(1.0);
                    let negative_bond = negative_bearing > 0.0;
                    let positive_bond = positive_bearing > 0.0;
                    if program.archetype == BuildingArchetype::Cathedral
                        && use_kind == crate::OpeningUse::Window
                        && (negative_bond || positive_bond)
                    {
                        let corner_clear = if negative_bond && positive_bond {
                            0.58
                        } else {
                            0.84
                        };
                        mouth_width = mouth_width.min(corner_clear);
                        exterior_width = exterior_width.min(mouth_width);
                    }
                    let required_negative = thickness.max(negative_bearing) * 0.5 + 0.03;
                    let required_positive = thickness.max(positive_bearing) * 0.5 + 0.03;
                    if negative_bond && positive_bond {
                        let available =
                            (CELL_SIZE_METRES - required_negative - required_positive).max(0.68);
                        if mouth_width > available {
                            mouth_width = available;
                            exterior_width = exterior_width.min(mouth_width);
                            profile = match profile {
                                crate::OpeningProfile::Rectangular { height_metres, .. } => {
                                    crate::OpeningProfile::Rectangular {
                                        width_metres: mouth_width,
                                        height_metres,
                                    }
                                }
                                crate::OpeningProfile::Segmental {
                                    spring_height_metres,
                                    rise_metres,
                                    intrados_depth_metres,
                                    ..
                                } => crate::OpeningProfile::Segmental {
                                    width_metres: mouth_width,
                                    spring_height_metres,
                                    rise_metres,
                                    intrados_depth_metres,
                                },
                                crate::OpeningProfile::PointedTwoCentred {
                                    spring_height_metres,
                                    apex_height_metres,
                                    ..
                                } => crate::OpeningProfile::PointedTwoCentred {
                                    width_metres: mouth_width,
                                    spring_height_metres,
                                    apex_height_metres,
                                    arc_radius_metres: two_centred_arc_radius(
                                        mouth_width,
                                        apex_height_metres - spring_height_metres,
                                    ),
                                },
                                crate::OpeningProfile::ArrowLoop {
                                    exterior_height_metres,
                                    interior_height_metres,
                                    ..
                                } => crate::OpeningProfile::ArrowLoop {
                                    exterior_width_metres: exterior_width.min(mouth_width - 0.04),
                                    interior_width_metres: mouth_width,
                                    exterior_height_metres,
                                    interior_height_metres,
                                },
                                crate::OpeningProfile::GunLoop {
                                    exterior_height_metres,
                                    interior_height_metres,
                                    mount,
                                    traverse_degrees,
                                    recoil_metres,
                                    crew_clearance_metres,
                                    ..
                                } => crate::OpeningProfile::GunLoop {
                                    exterior_width_metres: exterior_width.min(mouth_width - 0.04),
                                    interior_width_metres: mouth_width,
                                    exterior_height_metres,
                                    interior_height_metres,
                                    mount,
                                    traverse_degrees,
                                    recoil_metres,
                                    crew_clearance_metres,
                                },
                            };
                        }
                    }
                    let nominal_pier = (CELL_SIZE_METRES - mouth_width) * 0.5;
                    let opening_offset = match (negative_bond, positive_bond) {
                        (true, false) => (required_negative - nominal_pier)
                            .max(0.0)
                            .min((nominal_pier - 0.05).max(0.0)),
                        (false, true) => -(required_positive - nominal_pier)
                            .max(0.0)
                            .min((nominal_pier - 0.05).max(0.0)),
                        (true, true) => ((required_negative - required_positive) * 0.5)
                            .clamp(-nominal_pier + 0.05, nominal_pier - 0.05),
                        (false, false) => 0.0,
                    };
                    let origin = origin + tangent * opening_offset;
                    profile = match profile {
                        crate::OpeningProfile::Rectangular { height_metres, .. } => {
                            crate::OpeningProfile::Rectangular {
                                width_metres: mouth_width,
                                height_metres,
                            }
                        }
                        crate::OpeningProfile::Segmental {
                            spring_height_metres,
                            rise_metres,
                            intrados_depth_metres,
                            ..
                        } => crate::OpeningProfile::Segmental {
                            width_metres: mouth_width,
                            spring_height_metres,
                            rise_metres,
                            intrados_depth_metres,
                        },
                        crate::OpeningProfile::PointedTwoCentred {
                            spring_height_metres,
                            apex_height_metres,
                            ..
                        } => crate::OpeningProfile::PointedTwoCentred {
                            width_metres: mouth_width,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres: two_centred_arc_radius(
                                mouth_width,
                                apex_height_metres - spring_height_metres,
                            ),
                        },
                        crate::OpeningProfile::ArrowLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        } => crate::OpeningProfile::ArrowLoop {
                            exterior_width_metres: exterior_width,
                            interior_width_metres: mouth_width,
                            exterior_height_metres,
                            interior_height_metres,
                        },
                        crate::OpeningProfile::GunLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            mount,
                            traverse_degrees,
                            recoil_metres,
                            crew_clearance_metres,
                            ..
                        } => crate::OpeningProfile::GunLoop {
                            exterior_width_metres: exterior_width,
                            interior_width_metres: mouth_width,
                            exterior_height_metres,
                            interior_height_metres,
                            mount,
                            traverse_degrees,
                            recoil_metres,
                            crew_clearance_metres,
                        },
                    };
                    let clear_height = profile
                        .clear_height_metres()
                        .min(program.storey_height_metres - opening.sill_metres - 0.10);
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
                                origin.x + tangent.x * side * mouth_width * 0.5,
                                base,
                                origin.y + tangent.y * side * mouth_width * 0.5,
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
                        position: Vec3::new(
                            origin.x,
                            base + opening.sill_metres + clear_height,
                            origin.y,
                        ),
                        supported_by: jamb_nodes.to_vec(),
                        grounded: false,
                    });
                    let spandrel_node = StructuralNodeId(wall_node.0 + 4);
                    geometry.structural_nodes.push(StructuralNode {
                        id: spandrel_node,
                        owner,
                        kind: StructuralNodeKind::OpeningSpandrel,
                        position: Vec3::new(
                            origin.x,
                            base + program.storey_height_metres,
                            origin.y,
                        ),
                        supported_by: vec![head_node],
                        grounded: false,
                    });
                    let tracery_node =
                        (matches!(profile, crate::OpeningProfile::PointedTwoCentred { .. })
                            && mouth_width >= 0.90)
                            .then(|| {
                                let node = StructuralNodeId(wall_node.0 + 5);
                                geometry.structural_nodes.push(StructuralNode {
                                    id: node,
                                    owner,
                                    kind: StructuralNodeKind::MullionBearing,
                                    position: Vec3::new(
                                        origin.x,
                                        base + opening.sill_metres,
                                        origin.y,
                                    ),
                                    supported_by: vec![wall_node],
                                    grounded: false,
                                });
                                node
                            });
                    // Splayed military apertures are resolved as the actual masonry
                    // wedges between the narrow exterior throat and broad interior
                    // mouth.  The exterior pier footprint is authoritative; its
                    // inner face retreats toward the cell edge through the wall
                    // depth.  A broad rectangular void plus cuboid jambs would leave
                    // the semantic throat disconnected from the rendered opening.
                    let side_widths = [
                        CELL_SIZE_METRES * 0.5 + opening_offset - exterior_width * 0.5,
                        CELL_SIZE_METRES * 0.5 - opening_offset - exterior_width * 0.5,
                    ];
                    let mut jamb_solids = [ResolvedItemId::default(); 2];
                    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
                        let side_width = side_widths[index];
                        let plan = origin + tangent * side * (exterior_width + side_width) * 0.5;
                        let size = if wall.is_horizontal() {
                            Vec3::new(side_width, program.storey_height_metres, thickness)
                        } else {
                            Vec3::new(thickness, program.storey_height_metres, side_width)
                        };
                        let shape = if mouth_width > exterior_width + 0.01 {
                            crate::ResolvedSolidShape::SplayedReveal {
                                exterior_width_metres: exterior_width,
                                interior_width_metres: mouth_width,
                                side: if side < 0.0 { -1 } else { 1 },
                                exterior_depth_sign: if wall.is_horizontal() {
                                    if outward.y >= 0.0 { 1 } else { -1 }
                                } else if outward.x <= 0.0 {
                                    1
                                } else {
                                    -1
                                },
                            }
                        } else {
                            crate::ResolvedSolidShape::Cuboid
                        };
                        let solid = wall_solid(
                            geometry,
                            owner,
                            index as u64,
                            Vec3::new(plan.x, base + program.storey_height_metres * 0.5, plan.y),
                            size,
                            SolidRole::OpeningJamb,
                            shape,
                            jamb_nodes[index],
                        );
                        jamb_solids[index] = solid;
                        host_solids.push(solid);
                    }
                    let sill_solid = if opening.sill_metres > 0.01 {
                        let size = if wall.is_horizontal() {
                            Vec3::new(mouth_width, opening.sill_metres, thickness)
                        } else {
                            Vec3::new(thickness, opening.sill_metres, mouth_width)
                        };
                        let solid = wall_solid(
                            geometry,
                            owner,
                            2,
                            Vec3::new(origin.x, base + opening.sill_metres * 0.5, origin.y),
                            size,
                            SolidRole::OpeningSill,
                            crate::ResolvedSolidShape::Cuboid,
                            wall_node,
                        );
                        host_solids.push(solid);
                        Some(solid)
                    } else {
                        None
                    };
                    let header_base = opening.sill_metres
                        + match profile {
                            crate::OpeningProfile::Segmental {
                                spring_height_metres,
                                ..
                            }
                            | crate::OpeningProfile::PointedTwoCentred {
                                spring_height_metres,
                                ..
                            } => spring_height_metres,
                            _ => clear_height,
                        };
                    let (head_bottom, head_top, head_shape) = match profile {
                        crate::OpeningProfile::Segmental {
                            width_metres,
                            spring_height_metres,
                            rise_metres,
                            intrados_depth_metres,
                        } => (
                            opening.sill_metres + spring_height_metres,
                            opening.sill_metres + spring_height_metres + rise_metres + 0.20,
                            crate::ResolvedSolidShape::SegmentalArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                rise_metres,
                                ring_depth_metres: intrados_depth_metres,
                            },
                        ),
                        crate::OpeningProfile::PointedTwoCentred {
                            width_metres,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres,
                        } => (
                            opening.sill_metres + spring_height_metres,
                            opening.sill_metres + apex_height_metres + 0.20,
                            crate::ResolvedSolidShape::PointedArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres,
                                ring_depth_metres: 0.20,
                            },
                        ),
                        crate::OpeningProfile::ArrowLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        }
                        | crate::OpeningProfile::GunLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        } => (
                            opening.sill_metres
                                + exterior_height_metres.min(interior_height_metres),
                            opening.sill_metres
                                + exterior_height_metres.max(interior_height_metres)
                                + 0.20,
                            crate::ResolvedSolidShape::SplayedHead {
                                exterior_clear_height_metres: exterior_height_metres,
                                interior_clear_height_metres: interior_height_metres,
                                exterior_depth_sign: if wall.is_horizontal() {
                                    if outward.y >= 0.0 { 1 } else { -1 }
                                } else if outward.x <= 0.0 {
                                    1
                                } else {
                                    -1
                                },
                            },
                        ),
                        crate::OpeningProfile::Rectangular { .. } => (
                            opening.sill_metres + clear_height,
                            opening.sill_metres + clear_height + 0.20,
                            crate::ResolvedSolidShape::Cuboid,
                        ),
                    };
                    let head_top = head_top.min(program.storey_height_metres - 0.05);
                    let head_height = (head_top - head_bottom).max(0.10);
                    let bearing_width = 0.10_f32.min((CELL_SIZE_METRES - mouth_width) * 0.25);
                    let head_total_width = mouth_width + bearing_width * 2.0;
                    let head_size = if wall.is_horizontal() {
                        Vec3::new(head_total_width, head_height, thickness)
                    } else {
                        Vec3::new(thickness, head_height, head_total_width)
                    };
                    let head_solid = wall_solid(
                        geometry,
                        owner,
                        3,
                        Vec3::new(origin.x, base + head_bottom + head_height * 0.5, origin.y),
                        head_size,
                        SolidRole::OpeningHead,
                        head_shape,
                        head_node,
                    );
                    host_solids.push(head_solid);
                    let spandrel_bottom = (head_top - 0.025).max(head_bottom);
                    let spandrel_height =
                        (program.storey_height_metres - spandrel_bottom).max(0.05);
                    let spandrel_size = if wall.is_horizontal() {
                        Vec3::new(head_total_width, spandrel_height, thickness)
                    } else {
                        Vec3::new(thickness, spandrel_height, head_total_width)
                    };
                    let spandrel_solid = wall_solid(
                        geometry,
                        owner,
                        4,
                        Vec3::new(
                            origin.x,
                            base + spandrel_bottom + spandrel_height * 0.5,
                            origin.y,
                        ),
                        spandrel_size,
                        SolidRole::OpeningSpandrel,
                        crate::ResolvedSolidShape::Cuboid,
                        spandrel_node,
                    );
                    host_solids.push(spandrel_solid);
                    // These interfaces are measured from the resolved head and
                    // pier geometry rather than inferred from node IDs. The
                    // narrow contact bands represent the two springings/end
                    // bearings; the third interface is the measured contact
                    // between this head and a distinct upper-spandrel solid.
                    let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
                        let slot = if side < 0.0 { 50_u64 } else { 51_u64 };
                        let centre_plan =
                            origin + tangent * side * (mouth_width * 0.5 + bearing_width * 0.5);
                        let extent = tangent.abs() * (bearing_width * 0.5)
                            + outward.abs() * (thickness * 0.5);
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
                    let above_half_tangent = tangent.abs() * (mouth_width * 0.5);
                    let above_half_depth = outward.abs() * (thickness * 0.5);
                    geometry.support_interfaces.push(SupportInterface {
                        id: wall_above_interface,
                        owner,
                        node: spandrel_node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                origin.x - above_half_tangent.x - above_half_depth.x,
                                base + head_top - 0.025,
                                origin.y - above_half_tangent.y - above_half_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + above_half_tangent.x + above_half_depth.x,
                                base + head_top + 0.025,
                                origin.y + above_half_tangent.y + above_half_depth.y,
                            ),
                        },
                    });
                    let half_tangent = tangent.abs() * (mouth_width * 0.5);
                    let half_depth = outward.abs() * (thickness * 0.55);
                    let (exterior_height, interior_height) = match profile {
                        crate::OpeningProfile::ArrowLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        }
                        | crate::OpeningProfile::GunLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        } => (
                            exterior_height_metres.min(clear_height),
                            interior_height_metres.min(clear_height),
                        ),
                        _ => (clear_height, clear_height),
                    };
                    let exterior_depth_sign = if wall.is_horizontal() {
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
                                base + opening.sill_metres,
                                origin.y - half_tangent.y - half_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + half_tangent.x + half_depth.x,
                                base + opening.sill_metres + clear_height,
                                origin.y + half_tangent.y + half_depth.y,
                            ),
                        },
                        opening_id,
                        exterior_width,
                        mouth_width,
                        exterior_height,
                        interior_height,
                        exterior_depth_sign,
                    );
                    let mut reveal_surfaces = Vec::new();
                    for (index, side) in [-1_i8, 1].into_iter().enumerate() {
                        let along = f32::from(side) * (exterior_width + mouth_width) * 0.25;
                        let plan = origin + tangent * along;
                        let half_depth = outward.abs() * (thickness * 0.5);
                        let half_reveal = tangent.abs() * 0.015;
                        reveal_surfaces.push(wall_shaped_surface(
                            geometry,
                            owner,
                            10 + index as u64,
                            ResolvedBounds {
                                min: Vec3::new(
                                    plan.x - half_depth.x - half_reveal.x,
                                    base + opening.sill_metres,
                                    plan.y - half_depth.y - half_reveal.y,
                                ),
                                max: Vec3::new(
                                    plan.x + half_depth.x + half_reveal.x,
                                    base + opening.sill_metres + clear_height,
                                    plan.y + half_depth.y + half_reveal.y,
                                ),
                            },
                            if side < 0 {
                                SurfaceRole::LeftJambReveal
                            } else {
                                SurfaceRole::RightJambReveal
                            },
                            crate::ResolvedSurfaceShape::SplayedJamb {
                                side,
                                exterior_width_metres: exterior_width,
                                interior_width_metres: mouth_width,
                                exterior_depth_sign,
                            },
                        ));
                    }
                    let half_mouth = tangent.abs() * (mouth_width * 0.5);
                    let half_wall_depth = outward.abs() * (thickness * 0.5);
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        12,
                        ResolvedBounds {
                            min: Vec3::new(
                                origin.x - half_mouth.x - half_wall_depth.x,
                                base + opening.sill_metres,
                                origin.y - half_mouth.y - half_wall_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + half_mouth.x + half_wall_depth.x,
                                base + opening.sill_metres + 0.015,
                                origin.y + half_mouth.y + half_wall_depth.y,
                            ),
                        },
                        SurfaceRole::WeatherSill,
                        crate::ResolvedSurfaceShape::WeatherSill {
                            interior_elevation_metres: base + opening.sill_metres,
                            exterior_elevation_metres: base + opening.sill_metres - 0.035,
                            drip_depth_metres: 0.025,
                        },
                    ));
                    let intrados_shape = match profile {
                        crate::OpeningProfile::Segmental {
                            width_metres,
                            spring_height_metres,
                            rise_metres,
                            ..
                        } => crate::ResolvedSurfaceShape::SegmentalIntrados {
                            clear_span_metres: width_metres,
                            spring_height_metres,
                            rise_metres,
                        },
                        crate::OpeningProfile::PointedTwoCentred {
                            width_metres,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres,
                        } => crate::ResolvedSurfaceShape::PointedIntrados {
                            clear_span_metres: width_metres,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres,
                        },
                        _ => crate::ResolvedSurfaceShape::Planar,
                    };
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        13,
                        ResolvedBounds {
                            min: Vec3::new(
                                origin.x - half_mouth.x - half_wall_depth.x,
                                base + header_base - 0.015,
                                origin.y - half_mouth.y - half_wall_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + half_mouth.x + half_wall_depth.x,
                                base + header_base,
                                origin.y + half_mouth.y + half_wall_depth.y,
                            ),
                        },
                        SurfaceRole::Intrados,
                        intrados_shape,
                    ));
                    for (slot, depth_sign, role, width, height) in [
                        (
                            14_u64,
                            1.0_f32,
                            SurfaceRole::ExteriorThroat,
                            exterior_width,
                            exterior_height,
                        ),
                        (
                            15_u64,
                            -1.0_f32,
                            SurfaceRole::InteriorMouth,
                            mouth_width,
                            interior_height,
                        ),
                    ] {
                        let face = origin + outward * (thickness * 0.5 * depth_sign);
                        let half_width = tangent.abs() * (width * 0.5);
                        let half_face_depth = outward.abs() * 0.006;
                        reveal_surfaces.push(wall_shaped_surface(
                            geometry,
                            owner,
                            slot,
                            ResolvedBounds {
                                min: Vec3::new(
                                    face.x - half_width.x - half_face_depth.x,
                                    base + opening.sill_metres,
                                    face.y - half_width.y - half_face_depth.y,
                                ),
                                max: Vec3::new(
                                    face.x + half_width.x + half_face_depth.x,
                                    base + opening.sill_metres + height,
                                    face.y + half_width.y + half_face_depth.y,
                                ),
                            },
                            role,
                            crate::ResolvedSurfaceShape::Planar,
                        ));
                    }
                    if matches!(profile, crate::OpeningProfile::PointedTwoCentred { .. })
                        && mouth_width >= 0.90
                    {
                        let tracery_node = tracery_node.expect("wide pointed opening tracery node");
                        let mullion_height = match profile {
                            crate::OpeningProfile::PointedTwoCentred {
                                spring_height_metres,
                                ..
                            } => spring_height_metres,
                            _ => clear_height * 0.75,
                        };
                        let bearing_embed = 0.025;
                        let mullion = wall_solid(
                            geometry,
                            owner,
                            12,
                            Vec3::new(
                                origin.x,
                                base + opening.sill_metres - bearing_embed
                                    + (mullion_height + bearing_embed) * 0.5,
                                origin.y,
                            ),
                            if wall.is_horizontal() {
                                Vec3::new(0.08, mullion_height + bearing_embed, thickness * 0.35)
                            } else {
                                Vec3::new(thickness * 0.35, mullion_height + bearing_embed, 0.08)
                            },
                            SolidRole::Mullion,
                            crate::ResolvedSolidShape::Cuboid,
                            tracery_node,
                        );
                        host_solids.push(mullion);
                        let transom = wall_solid(
                            geometry,
                            owner,
                            13,
                            Vec3::new(
                                origin.x,
                                base + opening.sill_metres + mullion_height * 0.72,
                                origin.y,
                            ),
                            if wall.is_horizontal() {
                                Vec3::new(mouth_width * 0.82, 0.09, thickness * 0.30)
                            } else {
                                Vec3::new(thickness * 0.30, 0.09, mouth_width * 0.82)
                            },
                            SolidRole::Mullion,
                            crate::ResolvedSolidShape::Cuboid,
                            tracery_node,
                        );
                        host_solids.push(transom);
                        let extent = tangent.abs() * 0.04 + outward.abs() * (thickness * 0.175);
                        geometry.support_interfaces.push(SupportInterface {
                            id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 53),
                            owner,
                            node: tracery_node,
                            bounds: ResolvedBounds {
                                min: Vec3::new(
                                    origin.x - extent.x,
                                    base + opening.sill_metres - bearing_embed,
                                    origin.y - extent.y,
                                ),
                                max: Vec3::new(
                                    origin.x + extent.x,
                                    base + opening.sill_metres + 0.01,
                                    origin.y + extent.y,
                                ),
                            },
                        });
                    }
                    let closure = opening_closure(program, storey.level, opening_id, use_kind);
                    let mut closure_solids = Vec::new();
                    for (index, layer) in closure_solid_layers(&closure) {
                        let plan = origin
                            - outward
                                * (thickness * (0.12 + index as f32 * 0.08)
                                    + if material == crate::WallMaterialClass::TimberInfill {
                                        0.07
                                    } else {
                                        0.0
                                    });
                        if layer == crate::ClosureKind::LeadedGlazing
                            && matches!(profile, crate::OpeningProfile::PointedTwoCentred { .. })
                            && mouth_width >= 0.90
                        {
                            let panel_width = (mouth_width - 0.10) * 0.5;
                            let panel_offset = panel_width * 0.5 + 0.025;
                            let (spring, apex) = match profile {
                                crate::OpeningProfile::PointedTwoCentred {
                                    spring_height_metres,
                                    apex_height_metres,
                                    ..
                                } => (spring_height_metres, apex_height_metres),
                                _ => unreachable!(),
                            };
                            for (panel_index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
                                let panel_plan = plan + tangent * side * panel_offset;
                                closure_solids.push(wall_solid(
                                    geometry,
                                    owner,
                                    20 + index as u64 * 2 + panel_index as u64,
                                    Vec3::new(
                                        panel_plan.x,
                                        base + opening.sill_metres + clear_height * 0.5,
                                        panel_plan.y,
                                    ),
                                    if wall.is_horizontal() {
                                        Vec3::new(panel_width * 0.94, clear_height, 0.025)
                                    } else {
                                        Vec3::new(0.025, clear_height, panel_width * 0.94)
                                    },
                                    SolidRole::LeadedGlazing,
                                    crate::ResolvedSolidShape::PointedArchRing {
                                        clear_span_metres: panel_width,
                                        spring_height_metres: spring,
                                        apex_height_metres: apex,
                                        arc_radius_metres: two_centred_arc_radius(
                                            panel_width,
                                            apex - spring,
                                        ),
                                        ring_depth_metres: 0.025,
                                    },
                                    tracery_node.expect("wide pointed opening tracery node"),
                                ));
                            }
                            continue;
                        }
                        let role = if layer == crate::ClosureKind::LeadedGlazing {
                            SolidRole::LeadedGlazing
                        } else {
                            SolidRole::OpeningClosure
                        };
                        let closure_shape = match profile {
                            crate::OpeningProfile::Segmental {
                                width_metres,
                                spring_height_metres,
                                rise_metres,
                                intrados_depth_metres,
                            } => crate::ResolvedSolidShape::SegmentalArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                rise_metres,
                                ring_depth_metres: intrados_depth_metres,
                            },
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres,
                            } => crate::ResolvedSolidShape::PointedArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres,
                                ring_depth_metres: 0.025,
                            },
                            _ => crate::ResolvedSolidShape::Cuboid,
                        };
                        closure_solids.push(wall_solid(
                            geometry,
                            owner,
                            20 + index as u64,
                            Vec3::new(
                                plan.x,
                                base + opening.sill_metres + clear_height * 0.5,
                                plan.y,
                            ),
                            if wall.is_horizontal() {
                                Vec3::new(
                                    (exterior_width * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                    (clear_height * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                    0.025,
                                )
                            } else {
                                Vec3::new(
                                    0.025,
                                    (clear_height * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                    (exterior_width * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                )
                            },
                            role,
                            closure_shape,
                            head_node,
                        ));
                    }
                    let military = matches!(
                        use_kind,
                        crate::OpeningUse::ArrowLoop | crate::OpeningUse::GunLoop
                    );
                    let stance_surface = military.then(|| {
                        projected_surface(
                            geometry,
                            owner,
                            ResolvedBounds {
                                min: Vec3::new(
                                    origin.x - tangent.x.abs() * 0.40 - outward.x.abs() * 0.85,
                                    base,
                                    origin.y - tangent.y.abs() * 0.40 - outward.y.abs() * 0.85,
                                ),
                                max: Vec3::new(
                                    origin.x + tangent.x.abs() * 0.40,
                                    base + 0.02,
                                    origin.y + tangent.y.abs() * 0.40,
                                ),
                            },
                            SurfaceRole::Stance,
                        )
                    });
                    let mount_solid = (use_kind == crate::OpeningUse::GunLoop).then(|| {
                        let plan = origin - outward * thickness * 0.35;
                        wall_solid(
                            geometry,
                            owner,
                            30,
                            Vec3::new(plan.x, base + opening.sill_metres + 0.20, plan.y),
                            Vec3::splat(0.18),
                            SolidRole::WeaponMount,
                            crate::ResolvedSolidShape::Cuboid,
                            wall_node,
                        )
                    });
                    let mut ray_indices = Vec::new();
                    if military {
                        let stance = Vec3::new(
                            origin.x - outward.x * (thickness * 0.5 + 0.55),
                            base,
                            origin.y - outward.y * (thickness * 0.5 + 0.55),
                        );
                        let eye_height = if use_kind == crate::OpeningUse::GunLoop {
                            opening.sill_metres + 0.32
                        } else {
                            opening.sill_metres + clear_height * 0.56
                        };
                        let origin3 = Vec3::new(
                            origin.x - outward.x * (thickness * 0.5 + 0.01),
                            base + eye_height,
                            origin.y - outward.y * (thickness * 0.5 + 0.01),
                        );
                        for (range, distance) in [
                            (ProjectedDefenseRange::Near, 2.0_f32),
                            (ProjectedDefenseRange::Middle, 7.0_f32),
                            (ProjectedDefenseRange::Far, 16.0_f32),
                        ] {
                            ray_indices.push(geometry.projected_defense_rays.len());
                            geometry.projected_defense_rays.push(ProjectedDefenseRay {
                                owner,
                                throat: void_id,
                                stance,
                                origin: origin3,
                                target: origin3
                                    + Vec3::new(
                                        outward.x * distance,
                                        -0.08 * distance.min(5.0),
                                        outward.y * distance,
                                    ),
                                range,
                            });
                        }
                    }
                    openings_out.push(crate::OpeningAssembly {
                        id: opening_id,
                        owner,
                        host_wall: id,
                        host_source: source,
                        frame: crate::WallLocalFrame {
                            origin,
                            tangent,
                            outward,
                            inside_room: Some(wall.inside_room),
                            outside_room: wall.outside_room,
                        },
                        use_kind,
                        profile,
                        sill_elevation_metres: base + opening.sill_metres,
                        closure,
                        head_kind,
                        void_id,
                        jamb_solids,
                        sill_solid,
                        head_solid,
                        spandrel_solid,
                        reveal_surfaces,
                        closure_solids,
                        jamb_nodes,
                        head_node,
                        spandrel_node,
                        tracery_node,
                        stance_surface,
                        mount_solid,
                        ray_indices,
                        sectional_void: (0..=8)
                            .map(|index| {
                                let depth_fraction = index as f32 / 8.0;
                                crate::OpeningVoidSlice {
                                    depth_fraction,
                                    width_metres: exterior_width
                                        + (mouth_width - exterior_width) * depth_fraction,
                                    height_metres: exterior_height
                                        + (interior_height - exterior_height) * depth_fraction,
                                }
                            })
                            .collect(),
                        head_bearing_interfaces,
                        wall_above_interface,
                    });
                } else {
                    // Resolve an ordinary wall bay as two closed tangent
                    // prisms. Section proofs can therefore omit one exact
                    // authority ID and expose a genuine capped cut plane;
                    // the full render remains the exact source envelope.
                    for (slot, side) in [(0_u64, -1.0_f32), (1, 1.0)] {
                        let half_centre = origin + tangent * side * CELL_SIZE_METRES * 0.25;
                        let size = if wall.is_horizontal() {
                            Vec3::new(
                                CELL_SIZE_METRES * 0.5,
                                program.storey_height_metres,
                                thickness,
                            )
                        } else {
                            Vec3::new(
                                thickness,
                                program.storey_height_metres,
                                CELL_SIZE_METRES * 0.5,
                            )
                        };
                        host_solids.push(wall_solid(
                            geometry,
                            owner,
                            slot,
                            Vec3::new(
                                half_centre.x,
                                base + program.storey_height_metres * 0.5,
                                half_centre.y,
                            ),
                            size,
                            SolidRole::WallHost,
                            crate::ResolvedSolidShape::Cuboid,
                            wall_node,
                        ));
                    }
                    if material == crate::WallMaterialClass::CathedralMasonry && wall.exterior() {
                        let buttress_depth = 0.78;
                        for (slot, side) in [(80_u64, -1.0_f32), (81, 1.0)] {
                            let buttress_plan = origin
                                + tangent * side * 0.12
                                + outward * (thickness * 0.5 + buttress_depth * 0.5);
                            host_solids.push(wall_solid(
                                geometry,
                                owner,
                                slot,
                                Vec3::new(
                                    buttress_plan.x,
                                    base + program.storey_height_metres * 0.44,
                                    buttress_plan.y,
                                ),
                                if wall.is_horizontal() {
                                    Vec3::new(
                                        0.24,
                                        program.storey_height_metres * 0.88,
                                        buttress_depth,
                                    )
                                } else {
                                    Vec3::new(
                                        buttress_depth,
                                        program.storey_height_metres * 0.88,
                                        0.24,
                                    )
                                },
                                SolidRole::WallButtress,
                                crate::ResolvedSolidShape::Cuboid,
                                wall_node,
                            ));
                        }
                    }
                }
            }
            walls_out.push(crate::WallAssembly {
                id,
                owner,
                source,
                material,
                storey_level: storey.level,
                frame: crate::WallLocalFrame {
                    origin,
                    tangent,
                    outward,
                    inside_room: Some(wall.inside_room),
                    outside_room: wall.outside_room,
                },
                radial_frame: None,
                length_metres: CELL_SIZE_METRES,
                height_metres: program.storey_height_metres,
                base_elevation_metres: base,
                thickness_metres: thickness,
                structural_role,
                support_node: wall_node,
                host_solids: replacement
                    .map(|defense| defense.host_wall_solids.clone())
                    .unwrap_or(host_solids),
                opening_ids,
                replaced_by_owner: replacement.map(|defense| defense.host_owner),
            });
            global_index += 1;
        }
    }
    (walls_out, openings_out)
}
