fn resolve_roof_child_front_openings(
    program: &BuildingProgram,
    dormers: &[RoofDormer],
    roofs: &mut [RoofAssembly],
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    for (index, dormer) in dormers.iter().copied().enumerate() {
        let roof_id = RoofAssemblyId(1_000 + index as u64);
        let parent_owner = roofs
            .iter()
            .find(|roof| roof.id == roof_id)
            .and_then(|roof| roof.parent)
            .and_then(|parent| roofs.iter().find(|roof| roof.id == parent))
            .map(|roof| roof.owner);
        let parent_support_nodes = roofs
            .iter()
            .find(|roof| roof.id == roof_id)
            .and_then(|roof| roof.parent)
            .and_then(|parent| roofs.iter().find(|roof| roof.id == parent))
            .map(|roof| roof.support_nodes.clone())
            .unwrap_or_default();
        let Some(child) = roofs.iter_mut().find(|roof| roof.id == roof_id) else {
            continue;
        };
        let front_enclosure_id = ResolvedItemId((0xA_u64 << 60) | (roof_id.0 << 16) | 0x4100);
        let Some(front) = child
            .enclosure_faces
            .iter()
            .find(|face| face.id == front_enclosure_id)
            .cloned()
        else {
            continue;
        };
        child
            .enclosure_faces
            .retain(|face| face.id != front_enclosure_id);

        let wall_id = crate::WallAssemblyId(1_000_000 + index as u64);
        let opening_id = crate::OpeningAssemblyId(1_000_000 + index as u64);
        let owner = GeometryOwnerId(70_000 + index as u32);
        let outward = direction_vector(dormer.facing);
        let tangent = if outward.y.abs() > 0.5 {
            Vec2::X
        } else {
            Vec2::Y
        };
        let origin = dormer.centre;
        let width = front
            .polygon
            .iter()
            .map(|point| Vec2::new(point.x, point.z).dot(tangent))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
                (min.min(value), max.max(value))
            });
        let width = width.1 - width.0;
        let facade_wall = (dormer.kind == DormerKind::TransverseGable)
            .then(|| {
                walls
                    .iter()
                    .filter(|wall| {
                        matches!(wall.source, crate::WallSourceId::StoreyWall { .. })
                            && wall.frame.outside_room.is_none()
                            && wall.frame.outward.dot(outward) > 0.99
                    })
                    .min_by(|left, right| {
                        left.frame
                            .origin
                            .distance(origin)
                            .total_cmp(&right.frame.origin.distance(origin))
                    })
                    .map(|wall| (wall.id, wall.support_node))
            })
            .flatten();
        let base = front
            .polygon
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        let top = front
            .polygon
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let height = (top - base).max(1.15);
        let thickness = 0.20;
        let wall_node = StructuralNodeId((u64::from(owner.0) << 16) | 1);
        geometry.structural_nodes.push(StructuralNode {
            id: wall_node,
            owner,
            kind: StructuralNodeKind::RoofWallPlate,
            position: Vec3::new(origin.x, base, origin.y),
            supported_by: facade_wall
                .map(|(_, node)| vec![node])
                .unwrap_or_else(|| parent_support_nodes.clone()),
            grounded: false,
        });
        // The child facade/cheeks carry the child roof; the parent roof carries
        // their curb/trimmers.  Do not reverse this edge (wall -> child roof),
        // which forms a semantic cycle and previously encouraged generic
        // ground-to-eave fallback posts.
        for roof_node_id in &child.support_nodes {
            if let Some(roof_node) = geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == *roof_node_id)
            {
                roof_node.supported_by.push(wall_node);
                roof_node.supported_by.sort_unstable();
                roof_node.supported_by.dedup();
            }
        }
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
                    origin.x + tangent.x * side * width * 0.31,
                    base,
                    origin.y + tangent.y * side * width * 0.31,
                ),
                supported_by: vec![wall_node],
                grounded: false,
            });
        }
        let head_node = StructuralNodeId(wall_node.0 + 3);
        let spandrel_node = StructuralNodeId(wall_node.0 + 4);
        geometry.structural_nodes.push(StructuralNode {
            id: head_node,
            owner,
            kind: StructuralNodeKind::OpeningHead,
            position: Vec3::new(origin.x, base + height * 0.78, origin.y),
            supported_by: jamb_nodes.to_vec(),
            grounded: false,
        });
        geometry.structural_nodes.push(StructuralNode {
            id: spandrel_node,
            owner,
            kind: StructuralNodeKind::OpeningSpandrel,
            position: Vec3::new(origin.x, top, origin.y),
            supported_by: vec![head_node],
            grounded: false,
        });

        let opening_width = (width - 0.42).clamp(0.48, 0.82);
        let sill_height = 0.22;
        let sill_elevation = base + sill_height;
        let clear_height = (height - sill_height - 0.22).clamp(0.68, 1.12);
        let head_height = 0.14;
        let head_bottom = sill_elevation + clear_height;
        let side_width = (width - opening_width) * 0.5;
        let local_size = |tangent_width: f32, vertical: f32, depth: f32| {
            if tangent.x.abs() > 0.5 {
                Vec3::new(tangent_width, vertical, depth)
            } else {
                Vec3::new(depth, vertical, tangent_width)
            }
        };
        let mut host_solids = Vec::new();
        let mut jamb_solids = [ResolvedItemId::default(); 2];
        for (slot, side, node, target) in [
            (0_u64, -1.0_f32, jamb_nodes[0], 0_usize),
            (1, 1.0, jamb_nodes[1], 1),
        ] {
            let plan = origin + tangent * side * (opening_width + side_width) * 0.5;
            let solid = wall_solid(
                geometry,
                owner,
                slot,
                Vec3::new(plan.x, base + height * 0.5, plan.y),
                local_size(side_width, height, thickness),
                SolidRole::OpeningJamb,
                crate::ResolvedSolidShape::Cuboid,
                node,
            );
            host_solids.push(solid);
            jamb_solids[target] = solid;
        }
        let sill_solid = wall_solid(
            geometry,
            owner,
            2,
            Vec3::new(origin.x, base + sill_height * 0.5, origin.y),
            local_size(opening_width, sill_height, thickness),
            SolidRole::OpeningSill,
            crate::ResolvedSolidShape::Cuboid,
            wall_node,
        );
        host_solids.push(sill_solid);
        let head_solid = wall_solid(
            geometry,
            owner,
            3,
            Vec3::new(origin.x, head_bottom + head_height * 0.5, origin.y),
            local_size(opening_width + 0.12, head_height, thickness),
            SolidRole::OpeningHead,
            crate::ResolvedSolidShape::Cuboid,
            head_node,
        );
        host_solids.push(head_solid);
        let spandrel_height = (top - (head_bottom + head_height) + 0.025).max(0.08);
        let spandrel_solid = wall_solid(
            geometry,
            owner,
            4,
            Vec3::new(origin.x, top - spandrel_height * 0.5, origin.y),
            local_size(opening_width, spandrel_height, thickness),
            SolidRole::OpeningSpandrel,
            crate::ResolvedSolidShape::Cuboid,
            spandrel_node,
        );
        host_solids.push(spandrel_solid);
        // Non-Fachwerk fixtures retain the compact Stage-3 child-front frame.
        // The five accepted civilian programs instead receive their opening-
        // first members from `TimberFrameAssembly`, so duplicating this legacy
        // four-piece overlay would create two competing structural authorities.
        for (slot, plan, centre_y, frame_size) in (timber_program_kind(program.archetype).is_none())
            .then_some([
                (
                    100_u64,
                    origin - tangent * (width * 0.5 - 0.055),
                    base + height * 0.5,
                    local_size(0.11, height, 0.08),
                ),
                (
                    101,
                    origin + tangent * (width * 0.5 - 0.055),
                    base + height * 0.5,
                    local_size(0.11, height, 0.08),
                ),
                (102, origin, base + 0.055, local_size(width, 0.11, 0.08)),
                (103, origin, top - 0.055, local_size(width, 0.11, 0.08)),
            ])
            .into_iter()
            .flatten()
        {
            host_solids.push(wall_solid(
                geometry,
                owner,
                slot,
                Vec3::new(plan.x, centre_y, plan.y) + Vec3::new(outward.x, 0.0, outward.y) * 0.12,
                frame_size,
                SolidRole::FrameMember,
                crate::ResolvedSolidShape::Cuboid,
                wall_node,
            ));
        }

        let exterior_depth_sign = if tangent.x.abs() > 0.5 {
            if outward.y >= 0.0 { 1 } else { -1 }
        } else if outward.x <= 0.0 {
            1
        } else {
            -1
        };
        let void_half = tangent.abs() * (opening_width * 0.5) + outward.abs() * (thickness * 0.5);
        let void_id = wall_void(
            geometry,
            owner,
            10,
            ResolvedBounds {
                min: Vec3::new(
                    origin.x - void_half.x,
                    sill_elevation,
                    origin.y - void_half.y,
                ),
                max: Vec3::new(origin.x + void_half.x, head_bottom, origin.y + void_half.y),
            },
            opening_id,
            opening_width,
            opening_width,
            clear_height,
            clear_height,
            exterior_depth_sign,
        );
        let reveal_depth = outward.abs() * (thickness * 0.5);
        let side_half = tangent.abs() * 0.008;
        let mut reveal_surfaces = Vec::new();
        for (slot, side, role) in [
            (10_u64, -1.0_f32, SurfaceRole::LeftJambReveal),
            (11, 1.0, SurfaceRole::RightJambReveal),
        ] {
            let plan = origin + tangent * side * opening_width * 0.5;
            reveal_surfaces.push(wall_surface(
                geometry,
                owner,
                slot,
                ResolvedBounds {
                    min: Vec3::new(
                        plan.x - reveal_depth.x - side_half.x,
                        sill_elevation,
                        plan.y - reveal_depth.y - side_half.y,
                    ),
                    max: Vec3::new(
                        plan.x + reveal_depth.x + side_half.x,
                        head_bottom,
                        plan.y + reveal_depth.y + side_half.y,
                    ),
                },
                role,
            ));
        }
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            12,
            ResolvedBounds {
                min: Vec3::new(
                    origin.x - void_half.x,
                    sill_elevation,
                    origin.y - void_half.y,
                ),
                max: Vec3::new(
                    origin.x + void_half.x,
                    sill_elevation + 0.015,
                    origin.y + void_half.y,
                ),
            },
            SurfaceRole::WeatherSill,
            crate::ResolvedSurfaceShape::WeatherSill {
                interior_elevation_metres: sill_elevation,
                exterior_elevation_metres: sill_elevation - 0.035,
                drip_depth_metres: 0.025,
            },
        ));
        reveal_surfaces.push(wall_surface(
            geometry,
            owner,
            13,
            ResolvedBounds {
                min: Vec3::new(
                    origin.x - void_half.x,
                    head_bottom - 0.015,
                    origin.y - void_half.y,
                ),
                max: Vec3::new(origin.x + void_half.x, head_bottom, origin.y + void_half.y),
            },
            SurfaceRole::Intrados,
        ));
        for (slot, sign, role) in [
            (14_u64, 1.0_f32, SurfaceRole::ExteriorThroat),
            (15, -1.0, SurfaceRole::InteriorMouth),
        ] {
            let plan = origin + outward * thickness * 0.5 * sign;
            let half = tangent.abs() * opening_width * 0.5 + outward.abs() * 0.006;
            reveal_surfaces.push(wall_surface(
                geometry,
                owner,
                slot,
                ResolvedBounds {
                    min: Vec3::new(plan.x - half.x, sill_elevation, plan.y - half.y),
                    max: Vec3::new(plan.x + half.x, head_bottom, plan.y + half.y),
                },
                role,
            ));
        }
        let closure = fixed_window_closure_policy();
        let mut closure_solids = Vec::new();
        for (layer_index, layer) in closure.layers.iter().copied().enumerate() {
            let role = if layer == crate::ClosureKind::LeadedGlazing {
                SolidRole::LeadedGlazing
            } else {
                SolidRole::OpeningClosure
            };
            let plan = origin - outward * (0.065 + layer_index as f32 * 0.035);
            closure_solids.push(wall_solid(
                geometry,
                owner,
                20 + layer_index as u64,
                Vec3::new(plan.x, sill_elevation + clear_height * 0.5, plan.y),
                local_size(
                    (opening_width * 0.92 - 0.10).max(0.04),
                    (clear_height * 0.92 - 0.10).max(0.04),
                    0.025,
                ),
                role,
                crate::ResolvedSolidShape::Cuboid,
                head_node,
            ));
        }
        let bearing_ids = [
            ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 60),
            ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 61),
        ];
        for (side, id) in [-1.0_f32, 1.0].into_iter().zip(bearing_ids) {
            let plan = origin + tangent * side * (opening_width * 0.5 + 0.03);
            geometry.support_interfaces.push(SupportInterface {
                id,
                owner,
                node: head_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(plan.x - 0.08, head_bottom, plan.y - 0.08),
                    max: Vec3::new(plan.x + 0.08, head_bottom + 0.08, plan.y + 0.08),
                },
            });
        }
        let wall_above_interface = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 62);
        geometry.support_interfaces.push(SupportInterface {
            id: wall_above_interface,
            owner,
            node: spandrel_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    origin.x - 0.08,
                    head_bottom + head_height - 0.025,
                    origin.y - 0.08,
                ),
                max: Vec3::new(
                    origin.x + 0.08,
                    head_bottom + head_height + 0.025,
                    origin.y + 0.08,
                ),
            },
        });
        openings.push(crate::OpeningAssembly {
            id: opening_id,
            owner,
            host_wall: wall_id,
            host_source: crate::WallSourceId::RoofChildFront { roof: roof_id },
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            use_kind: crate::OpeningUse::Window,
            profile: crate::OpeningProfile::Rectangular {
                width_metres: opening_width,
                height_metres: clear_height,
            },
            sill_elevation_metres: sill_elevation,
            closure,
            head_kind: crate::OpeningHeadKind::TimberLintel,
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
                .map(|slice| crate::OpeningVoidSlice {
                    depth_fraction: slice as f32 / 8.0,
                    width_metres: opening_width,
                    height_metres: clear_height,
                })
                .collect(),
            head_bearing_interfaces: bearing_ids,
            wall_above_interface,
        });
        walls.push(crate::WallAssembly {
            id: wall_id,
            owner,
            source: crate::WallSourceId::RoofChildFront { roof: roof_id },
            material: crate::WallMaterialClass::TimberInfill,
            storey_level: program.storeys.len() as u16,
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: width,
            height_metres: height,
            base_elevation_metres: base,
            thickness_metres: thickness,
            structural_role: crate::WallStructuralRole::LoadBearing,
            support_node: wall_node,
            host_solids,
            opening_ids: vec![opening_id],
            replaced_by_owner: None,
        });
        for (bond_slot, roof_owner) in parent_owner.into_iter().enumerate() {
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId(
                    (0x6_u64 << 60) | (u64::from(owner.0) << 16) | (1 + bond_slot as u64),
                ),
                owners: [roof_owner, owner],
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        origin.x - tangent.x.abs() * width * 0.55 - outward.x.abs() * 0.30,
                        base - 0.12,
                        origin.y - tangent.y.abs() * width * 0.55 - outward.y.abs() * 0.30,
                    ),
                    max: Vec3::new(
                        origin.x + tangent.x.abs() * width * 0.55 + outward.x.abs() * 0.30,
                        top + 0.18,
                        origin.y + tangent.y.abs() * width * 0.55 + outward.y.abs() * 0.30,
                    ),
                },
                minimum_interface_area_square_metres: 0.005,
                maximum_penetration_metres: 0.18,
            });
        }
        if dormer.kind == DormerKind::TransverseGable
            && let (Some(parent_id), Some((facade_id, _))) = (child.parent, facade_wall)
            && let Some(parent) = roofs.iter_mut().find(|roof| roof.id == parent_id)
            && let Some(link) = parent
                .children
                .iter_mut()
                .find(|link| link.child == roof_id)
        {
            link.facade_wall = Some(facade_id);
        }
    }
}
