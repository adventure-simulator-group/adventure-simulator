#[derive(Clone, Copy)]
struct ChurchWindowProfile {
    sill_metres: f32,
    width_metres: f32,
    spring_height_metres: f32,
    apex_height_metres: f32,
}

/// Replace one authoritative cathedral wall panel with a load-bearing,
/// two-light pointed opening.  This deliberately reuses the accepted Stage 3
/// wall/opening truth vocabulary: the source host is removed, the remaining
/// masonry is resolved around a full-depth void, and the stone mullion bears
/// on the sill rather than hanging from the arch.
fn resolve_church_pointed_window(
    wall: &mut crate::WallAssembly,
    opening_id: crate::OpeningAssemblyId,
    serial: u64,
    profile: ChurchWindowProfile,
    geometry: &mut ResolvedGeometry,
) -> crate::OpeningAssembly {
    let owner = wall.owner;
    let origin = wall.frame.origin;
    let tangent = wall.frame.tangent;
    let outward = wall.frame.outward;
    let thickness = wall.thickness_metres;
    let base = wall.base_elevation_metres;
    let wall_top = base + wall.height_metres;
    let sill = profile.sill_metres;
    let width = profile.width_metres;
    let spring = profile.spring_height_metres;
    let apex = profile.apex_height_metres;
    let radius = two_centred_arc_radius(width, apex - spring);
    let slot = 0x20_000 + serial * 0x40;

    let removed = wall.host_solids.clone();
    geometry.solids.retain(|solid| !removed.contains(&solid.id));
    let removed_interfaces = removed
        .iter()
        .map(|id| ResolvedItemId((4_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF)))
        .collect::<HashSet<_>>();
    geometry
        .support_interfaces
        .retain(|interface| !removed_interfaces.contains(&interface.id));

    let jamb_nodes = [
        StructuralNodeId(8_000_000 + serial * 8),
        StructuralNodeId(8_000_001 + serial * 8),
    ];
    for (side, node_id) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
        geometry.structural_nodes.push(StructuralNode {
            id: node_id,
            owner,
            kind: StructuralNodeKind::OpeningJamb,
            position: Vec3::new(
                origin.x + tangent.x * side * width * 0.5,
                base,
                origin.y + tangent.y * side * width * 0.5,
            ),
            supported_by: vec![wall.support_node],
            grounded: false,
        });
    }
    let head_node = StructuralNodeId(8_000_002 + serial * 8);
    let spandrel_node = StructuralNodeId(8_000_003 + serial * 8);
    let tracery_node = StructuralNodeId(8_000_004 + serial * 8);
    geometry.structural_nodes.extend([
        StructuralNode {
            id: head_node,
            owner,
            kind: StructuralNodeKind::OpeningHead,
            position: Vec3::new(origin.x, sill + apex, origin.y),
            supported_by: jamb_nodes.to_vec(),
            grounded: false,
        },
        StructuralNode {
            id: spandrel_node,
            owner,
            kind: StructuralNodeKind::OpeningSpandrel,
            position: Vec3::new(origin.x, sill + apex + 0.20, origin.y),
            supported_by: vec![head_node],
            grounded: false,
        },
        StructuralNode {
            id: tracery_node,
            owner,
            kind: StructuralNodeKind::MullionBearing,
            position: Vec3::new(origin.x, sill, origin.y),
            supported_by: vec![wall.support_node],
            grounded: false,
        },
    ]);

    let side_width = (wall.length_metres - width) * 0.5;
    let mut jamb_solids = [ResolvedItemId::default(); 2];
    let mut host_solids = Vec::new();
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let point = origin + tangent * side * (width * 0.5 + side_width * 0.5);
        let id = wall_solid(
            geometry,
            owner,
            slot + index as u64,
            Vec3::new(point.x, base + wall.height_metres * 0.5, point.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(side_width, wall.height_metres, thickness)
            } else {
                Vec3::new(thickness, wall.height_metres, side_width)
            },
            SolidRole::OpeningJamb,
            crate::ResolvedSolidShape::Cuboid,
            jamb_nodes[index],
        );
        jamb_solids[index] = id;
        host_solids.push(id);
    }
    let sill_height = sill - base;
    let sill_solid = wall_solid(
        geometry,
        owner,
        slot + 2,
        Vec3::new(origin.x, base + sill_height * 0.5, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width, sill_height, thickness)
        } else {
            Vec3::new(thickness, sill_height, width)
        },
        SolidRole::OpeningSill,
        crate::ResolvedSolidShape::Cuboid,
        wall.support_node,
    );
    host_solids.push(sill_solid);
    let ring_depth = 0.24_f32;
    let head_solid = wall_solid(
        geometry,
        owner,
        slot + 3,
        Vec3::new(
            origin.x,
            sill + (spring + apex + ring_depth) * 0.5,
            origin.y,
        ),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width + 0.30, apex - spring + ring_depth, thickness)
        } else {
            Vec3::new(thickness, apex - spring + ring_depth, width + 0.30)
        },
        SolidRole::OpeningHead,
        crate::ResolvedSolidShape::PointedArchRing {
            clear_span_metres: width,
            spring_height_metres: spring,
            apex_height_metres: apex,
            arc_radius_metres: radius,
            ring_depth_metres: ring_depth,
        },
        head_node,
    );
    host_solids.push(head_solid);
    let spandrel_bottom = sill + apex + ring_depth - 0.025;
    let spandrel_height = (wall_top - spandrel_bottom).max(0.08);
    let spandrel_solid = wall_solid(
        geometry,
        owner,
        slot + 4,
        Vec3::new(origin.x, spandrel_bottom + spandrel_height * 0.5, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width + 0.30, spandrel_height, thickness)
        } else {
            Vec3::new(thickness, spandrel_height, width + 0.30)
        },
        SolidRole::OpeningSpandrel,
        crate::ResolvedSolidShape::Cuboid,
        spandrel_node,
    );
    host_solids.push(spandrel_solid);

    let mullion_height = spring;
    let mullion = wall_solid(
        geometry,
        owner,
        slot + 5,
        Vec3::new(
            origin.x,
            sill - 0.0125 + (mullion_height + 0.025) * 0.5,
            origin.y,
        ),
        if tangent.x.abs() > 0.5 {
            Vec3::new(0.10, mullion_height + 0.025, thickness * 0.36)
        } else {
            Vec3::new(thickness * 0.36, mullion_height + 0.025, 0.10)
        },
        SolidRole::Mullion,
        crate::ResolvedSolidShape::Cuboid,
        tracery_node,
    );
    let transom = wall_solid(
        geometry,
        owner,
        slot + 6,
        Vec3::new(origin.x, sill + spring * 0.70, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width * 0.82, 0.10, thickness * 0.32)
        } else {
            Vec3::new(thickness * 0.32, 0.10, width * 0.82)
        },
        SolidRole::Mullion,
        crate::ResolvedSolidShape::Cuboid,
        tracery_node,
    );
    host_solids.extend([mullion, transom]);

    let half_tangent = tangent.abs() * width * 0.5;
    let half_depth = outward.abs() * thickness * 0.55;
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
        slot,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                sill,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                sill + apex,
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
    for (surface_slot, side, role) in [
        (slot + 10, -1_i8, SurfaceRole::LeftJambReveal),
        (slot + 11, 1_i8, SurfaceRole::RightJambReveal),
    ] {
        let point = origin + tangent * f32::from(side) * width * 0.5;
        let extent = outward.abs() * thickness * 0.5 + tangent.abs() * 0.015;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            surface_slot,
            ResolvedBounds {
                min: Vec3::new(point.x - extent.x, sill, point.y - extent.y),
                max: Vec3::new(point.x + extent.x, sill + apex, point.y + extent.y),
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
        slot + 12,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                sill - 0.035,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                sill + 0.015,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::WeatherSill,
        crate::ResolvedSurfaceShape::WeatherSill {
            interior_elevation_metres: sill,
            exterior_elevation_metres: sill - 0.035,
            drip_depth_metres: 0.025,
        },
    ));
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        slot + 13,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                sill + spring - 0.015,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                sill + apex,
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
    for (surface_slot, sign, role) in [
        (slot + 14, 1.0_f32, SurfaceRole::ExteriorThroat),
        (slot + 15, -1.0, SurfaceRole::InteriorMouth),
    ] {
        let point = origin + outward * thickness * 0.5 * sign;
        let depth = outward.abs() * 0.006;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            surface_slot,
            ResolvedBounds {
                min: Vec3::new(
                    point.x - half_tangent.x - depth.x,
                    sill,
                    point.y - half_tangent.y - depth.y,
                ),
                max: Vec3::new(
                    point.x + half_tangent.x + depth.x,
                    sill + apex,
                    point.y + half_tangent.y + depth.y,
                ),
            },
            role,
            crate::ResolvedSurfaceShape::Planar,
        ));
    }

    let panel_width = (width - 0.12) * 0.5;
    let panel_offset = panel_width * 0.5 + 0.03;
    let glazing_plan = origin - outward * thickness * 0.20;
    let mut closure_solids = Vec::new();
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let point = glazing_plan + tangent * side * panel_offset;
        closure_solids.push(wall_solid(
            geometry,
            owner,
            slot + 20 + index as u64,
            Vec3::new(point.x, sill + apex * 0.5, point.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(panel_width * 0.94, apex, 0.025)
            } else {
                Vec3::new(0.025, apex, panel_width * 0.94)
            },
            SolidRole::LeadedGlazing,
            crate::ResolvedSolidShape::PointedArchRing {
                clear_span_metres: panel_width,
                spring_height_metres: spring,
                apex_height_metres: apex,
                arc_radius_metres: two_centred_arc_radius(panel_width, apex - spring),
                ring_depth_metres: 0.025,
            },
            tracery_node,
        ));
    }

    let bearing_width = 0.15_f32;
    let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
        let local = if side < 0.0 { slot + 50 } else { slot + 51 };
        let point = origin + tangent * side * (width * 0.5 + bearing_width * 0.5);
        let extent = tangent.abs() * bearing_width * 0.5 + outward.abs() * thickness * 0.5;
        let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | local);
        geometry.support_interfaces.push(SupportInterface {
            id,
            owner,
            node: head_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    point.x - extent.x,
                    sill + spring - 0.025,
                    point.y - extent.y,
                ),
                max: Vec3::new(
                    point.x + extent.x,
                    sill + spring + 0.025,
                    point.y + extent.y,
                ),
            },
        });
        id
    });
    let wall_above_interface =
        ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | (slot + 52));
    geometry.support_interfaces.push(SupportInterface {
        id: wall_above_interface,
        owner,
        node: spandrel_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                spandrel_bottom - 0.025,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                spandrel_bottom + 0.025,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
    });
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | (slot + 53)),
        owner,
        node: tracery_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - tangent.x.abs() * 0.05 - outward.x.abs() * thickness * 0.18,
                sill - 0.025,
                origin.y - tangent.y.abs() * 0.05 - outward.y.abs() * thickness * 0.18,
            ),
            max: Vec3::new(
                origin.x + tangent.x.abs() * 0.05 + outward.x.abs() * thickness * 0.18,
                sill + 0.01,
                origin.y + tangent.y.abs() * 0.05 + outward.y.abs() * thickness * 0.18,
            ),
        },
    });

    // `wall_solid` emits local X-length/Z-depth cuboids.  Cardinal walls need
    // no transform; apse chords rotate every resolved masonry, mullion, and
    // glazing member into the authoritative wall-local frame.
    let wall_yaw = -tangent.y.atan2(tangent.x);
    for id in host_solids.iter().chain(&closure_solids) {
        if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == *id) {
            item.yaw_radians = wall_yaw;
        }
    }
    wall.host_solids = host_solids;
    wall.opening_ids = vec![opening_id];
    crate::OpeningAssembly {
        id: opening_id,
        owner,
        host_wall: wall.id,
        host_source: wall.source,
        frame: wall.frame,
        use_kind: crate::OpeningUse::Window,
        profile: crate::OpeningProfile::PointedTwoCentred {
            width_metres: width,
            spring_height_metres: spring,
            apex_height_metres: apex,
            arc_radius_metres: radius,
        },
        sill_elevation_metres: sill,
        closure: crate::ClosurePolicy {
            layers: vec![crate::ClosureKind::LeadedGlazing],
            state: crate::ClosureState::Closed,
            thickness_metres: 0.025,
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
        tracery_node: Some(tracery_node),
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
    }
}
