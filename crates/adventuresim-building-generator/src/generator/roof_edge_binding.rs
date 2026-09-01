fn bind_coincident_primary_roof_edges(
    assemblies: &mut [RoofAssembly],
    geometry: &mut ResolvedGeometry,
) {
    let mut removals = Vec::new();
    for left_index in 0..assemblies.len() {
        if assemblies[left_index].parent.is_some()
            || assemblies[left_index].source_piece_index.is_none()
        {
            continue;
        }
        for right_index in left_index + 1..assemblies.len() {
            if assemblies[right_index].parent.is_some()
                || assemblies[right_index].source_piece_index.is_none()
            {
                continue;
            }
            for left_edge_index in 0..assemblies[left_index].edges.len() {
                let left_edge = assemblies[left_index].edges[left_edge_index].clone();
                if left_edge.adjacent_faces.len() != 1 {
                    continue;
                }
                let Some((right_edge_index, right_edge)) = assemblies[right_index]
                    .edges
                    .iter()
                    .enumerate()
                    .find(|(_, edge)| {
                        edge.adjacent_faces.len() == 1
                            && ((same_roof_vertex(left_edge.start, edge.end)
                                && same_roof_vertex(left_edge.end, edge.start))
                                || (same_roof_vertex(left_edge.start, edge.start)
                                    && same_roof_vertex(left_edge.end, edge.end)))
                    })
                    .map(|(index, edge)| (index, edge.clone()))
                else {
                    continue;
                };
                let kind = if matches!(
                    left_edge.kind,
                    RoofEdgeKind::WallAbutment | RoofEdgeKind::TowerAbutment
                ) || matches!(
                    right_edge.kind,
                    RoofEdgeKind::WallAbutment | RoofEdgeKind::TowerAbutment
                ) {
                    RoofEdgeKind::WallAbutment
                } else {
                    RoofEdgeKind::Valley
                };
                let flashing_id = ResolvedItemId(
                    (0x8_u64 << 60)
                        | (assemblies[left_index].id.0 << 16)
                        | 0x6000
                        | left_edge_index as u64,
                );
                let delta = left_edge.end - left_edge.start;
                let support = assemblies[left_index].support_nodes[0];
                geometry.solids.push(ResolvedSolid {
                    id: flashing_id,
                    owner: assemblies[left_index].owner,
                    centre: (left_edge.start + left_edge.end) * 0.5 + Vec3::Y * 0.02,
                    size: Vec3::new(Vec2::new(delta.x, delta.z).length(), 0.06, 0.20),
                    yaw_radians: delta.z.atan2(delta.x),
                    crossfall_radians: if kind == RoofEdgeKind::Valley {
                        -0.08
                    } else {
                        0.12
                    },
                    longfall_radians: if kind == RoofEdgeKind::Valley {
                        0.012
                    } else {
                        0.0
                    },
                    role: SolidRole::RoofFlashing,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: vec![support],
                });
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId(
                        (0x9_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x6000
                            | left_edge_index as u64,
                    ),
                    owner: assemblies[left_index].owner,
                    node: support,
                    bounds: ResolvedBounds {
                        min: (left_edge.start + left_edge.end) * 0.5 - Vec3::new(0.08, 0.025, 0.08),
                        max: (left_edge.start + left_edge.end) * 0.5 + Vec3::new(0.08, 0.025, 0.08),
                    },
                });
                let edge = &mut assemblies[left_index].edges[left_edge_index];
                edge.kind = kind;
                edge.flashing = Some(flashing_id);
                edge.adjacent_faces.push(right_edge.adjacent_faces[0]);
                if kind == RoofEdgeKind::Valley {
                    let outlet_id = ResolvedItemId(
                        (0xE_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x6000
                            | left_edge_index as u64,
                    );
                    let outlet = left_edge.end + delta.normalize_or_zero() * 0.08 - Vec3::Y * 0.08;
                    geometry.voids.push(ResolvedVoid {
                        id: outlet_id,
                        owner: assemblies[left_index].owner,
                        bounds: ResolvedBounds {
                            min: outlet - Vec3::splat(0.04),
                            max: outlet + Vec3::splat(0.04),
                        },
                        role: VoidRole::Drain,
                        shape: crate::ResolvedVoidShape::Box,
                        subtracts_from: assemblies[left_index].owner,
                    });
                    let route_id = ResolvedItemId(
                        (0xD_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x6000
                            | left_edge_index as u64,
                    );
                    geometry.drainage_routes.push(DrainageRoute {
                        id: route_id,
                        owner: assemblies[left_index].owner,
                        outlet_void: outlet_id,
                        inlet: left_edge.start + Vec3::Y * 0.02,
                        outlet,
                    });
                    edge.drainage_terminal = Some(outlet_id);
                }
                let weather_ids = [
                    ResolvedItemId(
                        (0x8_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x5000
                            | left_edge_index as u64,
                    ),
                    ResolvedItemId(
                        (0x8_u64 << 60)
                            | (assemblies[right_index].id.0 << 16)
                            | 0x5000
                            | right_edge_index as u64,
                    ),
                ];
                geometry
                    .solids
                    .retain(|solid| !weather_ids.contains(&solid.id));
                removals.push((right_index, right_edge_index));
            }
        }
    }
    removals.sort_unstable();
    removals.dedup();
    for (assembly_index, edge_index) in removals.into_iter().rev() {
        assemblies[assembly_index].edges.remove(edge_index);
    }
}
