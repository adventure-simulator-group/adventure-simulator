fn supplement_split_eave_drainage(assemblies: &[RoofAssembly], geometry: &mut ResolvedGeometry) {
    for assembly in assemblies {
        for link in assembly.children.iter().filter(|link| {
            link.kind == RoofChildKind::CrossGable && link.split_eave_edges.len() == 3
        }) {
            // The two retained eaves and the recessed apron at the facade cut are
            // distinct physical recipients.  The apron is not an eave relabel: it
            // catches the narrow strip of parent weather face that terminates at
            // the Zwerchhaus opening instead of allowing it to discharge onto the
            // facade or through the opening cut.
            let receivers = [
                link.split_eave_edges[0],
                link.split_eave_edges[1],
                link.split_eave_edges[2],
            ];
            let existing_edges = geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| network.owner == assembly.owner)
                .map(|network| network.receiving_edge)
                .collect::<HashSet<_>>();
            for (slot, edge_id) in receivers.into_iter().enumerate() {
                if existing_edges.contains(&edge_id) {
                    continue;
                }
                let Some(edge) = assembly.edges.iter().find(|edge| edge.id == edge_id) else {
                    continue;
                };
                let Some(face) = assembly
                    .faces
                    .iter()
                    .find(|face| edge.adjacent_faces.contains(&face.id))
                else {
                    continue;
                };
                let a = Vec2::new(edge.start.x, edge.start.z);
                let b = Vec2::new(edge.end.x, edge.end.z);
                let delta = b - a;
                let length = delta.length().max(0.05);
                let tangent = delta / length;
                let centre = face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len() as f32;
                let downhill = Vec2::new(
                    face.plane.normal.x / face.plane.normal.y,
                    face.plane.normal.z / face.plane.normal.y,
                )
                .normalize_or_zero();
                let projected = face
                    .polygon
                    .iter()
                    .map(|point| Vec2::new(point.x, point.z))
                    .collect::<Vec<_>>();
                let min = projected
                    .iter()
                    .copied()
                    .fold(Vec2::splat(f32::INFINITY), Vec2::min);
                let max = projected
                    .iter()
                    .copied()
                    .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                let cutouts = face
                    .cutouts
                    .iter()
                    .map(|cut| cut.iter().map(|p| Vec2::new(p.x, p.z)).collect::<Vec<_>>())
                    .collect::<Vec<_>>();
                let mut samples = Vec::new();
                for x in 0..5 {
                    for z in 0..5 {
                        let fraction = Vec2::new((x as f32 + 0.5) / 5.0, (z as f32 + 0.5) / 5.0);
                        let origin = min + (max - min) * fraction;
                        if !plan_point_in_convex_polygon(origin, &projected)
                            || cutouts
                                .iter()
                                .any(|cut| plan_point_in_convex_polygon(origin, cut))
                        {
                            continue;
                        }
                        let Some(hit) = ray_segment_intersection(origin, downhill, a, b) else {
                            continue;
                        };
                        let surface_y = roof_plane_height(face.plane, origin);
                        let edge_y = roof_plane_height(face.plane, hit);
                        if surface_y > edge_y + 0.005 {
                            samples.push(RoofDrainageSample {
                                surface_point: Vec3::new(origin.x, surface_y, origin.y),
                                channel_inlet: Vec3::new(hit.x, edge_y - 0.025, hit.y),
                            });
                        }
                    }
                }
                if samples.is_empty() {
                    continue;
                }
                let serial = 0x6800 | ((link.child.0 & 0xFF) << 4) | (slot as u64 * 4);
                let floor = ResolvedItemId((0x8_u64 << 60) | (assembly.id.0 << 16) | serial);
                let lips = [ResolvedItemId(floor.0 | 1), ResolvedItemId(floor.0 | 2)];
                let spout = ResolvedItemId(floor.0 | 3);
                let catchment = ResolvedItemId((0xC_u64 << 60) | (assembly.id.0 << 16) | serial);
                let route = ResolvedItemId((0xD_u64 << 60) | (assembly.id.0 << 16) | serial);
                let outlet_void = ResolvedItemId((0xE_u64 << 60) | (assembly.id.0 << 16) | serial);
                let network = ResolvedItemId((0x7_u64 << 60) | (assembly.id.0 << 16) | serial);
                let forward = (b.x, b.y) >= (a.x, a.y);
                let (high_plan, low_plan, channel_tangent) = if forward {
                    (a, b, tangent)
                } else {
                    (b, a, -tangent)
                };
                let drop = (length * 0.012).max(0.045);
                let mean_y = (edge.start.y + edge.end.y) * 0.5;
                let high = Vec3::new(high_plan.x, mean_y - 0.035, high_plan.y);
                let low = Vec3::new(low_plan.x, mean_y - 0.035 - drop, low_plan.y);
                let outward = (Vec2::new((high.x + low.x) * 0.5, (high.z + low.z) * 0.5)
                    - Vec2::new(centre.x, centre.z))
                .normalize_or_zero();
                let channel_centre = (high + low) * 0.5;
                let yaw = channel_tangent.y.atan2(channel_tangent.x);
                let longfall = -drop.atan2(length);
                let lip_offset = Vec3::new(outward.x, 0.0, outward.y) * 0.075;
                for (id, item_centre, size) in [
                    (floor, channel_centre, Vec3::new(length, 0.035, 0.18)),
                    (
                        lips[0],
                        channel_centre - lip_offset + Vec3::Y * 0.045,
                        Vec3::new(length, 0.11, 0.035),
                    ),
                    (
                        lips[1],
                        channel_centre + lip_offset + Vec3::Y * 0.045,
                        Vec3::new(length, 0.11, 0.035),
                    ),
                ] {
                    geometry.solids.push(ResolvedSolid {
                        id,
                        owner: assembly.owner,
                        centre: item_centre,
                        size,
                        yaw_radians: yaw,
                        crossfall_radians: 0.0,
                        longfall_radians: longfall,
                        role: SolidRole::RoofGutter,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: face.support_nodes.clone(),
                    });
                    geometry.support_interfaces.push(SupportInterface {
                        id: ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                        owner: assembly.owner,
                        node: face.support_nodes[0],
                        bounds: ResolvedBounds {
                            min: item_centre - Vec3::splat(0.035),
                            max: item_centre + Vec3::splat(0.035),
                        },
                    });
                }
                let outlet_plan = low_plan + channel_tangent * 0.08 + outward * 0.38;
                let outlet = Vec3::new(outlet_plan.x, low.y - 0.025, outlet_plan.y);
                let discharge = Vec3::new(outlet.x, 0.24, outlet.z);
                geometry.voids.push(ResolvedVoid {
                    id: outlet_void,
                    owner: assembly.owner,
                    bounds: ResolvedBounds {
                        min: outlet - Vec3::splat(0.04),
                        max: outlet + Vec3::splat(0.04),
                    },
                    role: VoidRole::Drain,
                    shape: crate::ResolvedVoidShape::Box,
                    subtracts_from: assembly.owner,
                });
                geometry.drainage_routes.push(DrainageRoute {
                    id: route,
                    owner: assembly.owner,
                    outlet_void,
                    inlet: samples[0].surface_point,
                    outlet,
                });
                geometry.surfaces.push(ResolvedSurface {
                    id: catchment,
                    owner: assembly.owner,
                    bounds: roof_polygon_bounds(&face.polygon),
                    role: SurfaceRole::RoofDrainage,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
                geometry.drainage_catchments.push(DrainageCatchment {
                    id: catchment,
                    owner: assembly.owner,
                    walk_solid: face.id,
                    toe_channel_solids: vec![floor, lips[0], lips[1]],
                    drainage_surface: catchment,
                    outlet_route: route,
                    centre,
                    tangent: channel_tangent,
                    outward,
                    length_metres: length,
                    width_metres: 0.18,
                    inner_elevation_metres: samples[0].surface_point.y,
                    outer_elevation_metres: low.y,
                    outlet_along_metres: length * 0.5,
                });
                geometry.solids.push(ResolvedSolid {
                    id: spout,
                    owner: assembly.owner,
                    centre: (outlet + discharge) * 0.5 - Vec3::Y * 0.10,
                    size: Vec3::new(0.09, (outlet.y - discharge.y - 0.20).max(0.09), 0.09),
                    yaw_radians: 0.0,
                    crossfall_radians: 0.0,
                    longfall_radians: 0.0,
                    role: SolidRole::RoofGutter,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: face.support_nodes.clone(),
                });
                let spout_top = Vec3::new(outlet.x, outlet.y - 0.20, outlet.z);
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId((0x9_u64 << 60) | (spout.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                    owner: assembly.owner,
                    node: face.support_nodes[0],
                    bounds: ResolvedBounds {
                        min: spout_top - Vec3::splat(0.035),
                        max: spout_top + Vec3::splat(0.035),
                    },
                });
                geometry.roof_drainage_networks.push(RoofDrainageNetwork {
                    id: network,
                    owner: assembly.owner,
                    face: face.id,
                    catchment,
                    receiving_edge: edge.id,
                    samples,
                    channel_floor: floor,
                    channel_lips: lips,
                    collector_solids: Vec::new(),
                    outlet_station: network,
                    outlet_void,
                    downspout: Some(spout),
                    channel_high: high,
                    channel_low: low,
                    discharge,
                });
            }
        }
    }
}
