fn resolve_one_roof(
    id: RoofAssemblyId,
    owner: GeometryOwnerId,
    roof: RoofPiece,
    source_piece_index: Option<usize>,
    source_tower_index: Option<usize>,
    parent: Option<RoofAssemblyId>,
    phase: RoofPhase,
    shed_high_side: Option<Direction>,
    support_post_parent: Option<&RoofAssembly>,
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) -> RoofAssembly {
    // Project gates: 0.13 m positive build-up and 15–75 degree pitch are
    // animation/rendering constraints, not universal historic dimensions.
    let thickness = if roof.kind == RoofKind::Flat {
        0.18
    } else {
        0.13
    };
    let mut apse_walls = walls
        .iter()
        .filter(|wall| matches!(wall.source, crate::WallSourceId::ChurchApse { .. }))
        .collect::<Vec<_>>();
    apse_walls.sort_by_key(|wall| match wall.source {
        crate::WallSourceId::ChurchApse { facet } => facet,
        _ => unreachable!(),
    });
    let is_church_apse = source_piece_index == Some(4) && apse_walls.len() == 5;
    let apse_outline: Option<Vec<Vec2>> = is_church_apse.then(|| {
        let first = apse_walls[0];
        let mut points = vec![first.frame.origin - first.frame.tangent * first.length_metres * 0.5];
        points.extend(
            apse_walls
                .iter()
                .map(|wall| wall.frame.origin + wall.frame.tangent * wall.length_metres * 0.5),
        );
        let diameter_mid = (points[0] + points[points.len() - 1]) * 0.5;
        points
            .into_iter()
            .map(|point| {
                // The chord wall is 0.90 m thick; a 0.75 m radial eave keeps
                // the physical gutter outside the masonry even at the acute
                // five-sided shoulders.  This is a frozen coarse-detail gate,
                // not a universal historic apse overhang.
                point + (point - diameter_mid).normalize_or_zero() * roof.eave_metres.max(0.75)
            })
            .collect()
    });
    let polygons = if let Some(outline) = &apse_outline {
        let diameter_mid = (outline[0] + outline[outline.len() - 1]) * 0.5;
        let radius = outline
            .iter()
            .map(|point| point.distance(diameter_mid))
            .fold(0.0_f32, f32::max);
        let apex = Vec3::new(
            diameter_mid.x,
            roof.base_height_metres + radius * roof.pitch_degrees.to_radians().tan(),
            diameter_mid.y,
        );
        outline
            .windows(2)
            .map(|pair| {
                vec![
                    Vec3::new(pair[0].x, roof.base_height_metres, pair[0].y),
                    Vec3::new(pair[1].x, roof.base_height_metres, pair[1].y),
                    apex,
                ]
            })
            .collect::<Vec<_>>()
    } else {
        roof_face_polygons(roof, shed_high_side)
    };
    let node_base = StructuralNodeId((0xA_u64 << 60) | (id.0 << 8));
    let mut host_nodes = walls
        .iter()
        .filter(|wall| wall.replaced_by_owner.is_none())
        .filter(|wall| {
            let top = wall.base_elevation_metres + wall.height_metres;
            (top - roof.base_height_metres).abs() <= 0.35
                || (wall.base_elevation_metres <= roof.base_height_metres
                    && top >= roof.base_height_metres)
        })
        .map(|wall| wall.support_node)
        .collect::<Vec<_>>();
    host_nodes.sort_unstable();
    host_nodes.dedup();
    if host_nodes.is_empty() {
        host_nodes.extend(
            walls
                .iter()
                .filter(|wall| wall.replaced_by_owner.is_none())
                .filter(|wall| {
                    wall.base_elevation_metres + wall.height_metres
                        <= roof.base_height_metres + 0.05
                })
                .max_by(|left, right| {
                    (left.base_elevation_metres + left.height_metres)
                        .total_cmp(&(right.base_elevation_metres + right.height_metres))
                })
                .map(|wall| wall.support_node),
        );
    }
    let host_top = walls
        .iter()
        .filter(|wall| host_nodes.contains(&wall.support_node))
        .map(|wall| wall.base_elevation_metres + wall.height_metres)
        .fold(f32::NEG_INFINITY, f32::max)
        .min(roof.base_height_metres);
    let line_x = Vec3::new(roof.size.x * 0.5, 0.04, 0.12);
    let line_z = Vec3::new(0.12, 0.04, roof.size.y * 0.5);
    let mut plate_specs = match roof.ridge_axis {
        RidgeAxis::Z => vec![
            (
                Vec3::new(
                    roof.centre.x - roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
            (
                Vec3::new(
                    roof.centre.x + roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
        ],
        RidgeAxis::X => vec![
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y - roof.size.y * 0.5,
                ),
                line_x,
            ),
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y + roof.size.y * 0.5,
                ),
                line_x,
            ),
        ],
    };
    if is_church_apse {
        plate_specs = apse_walls
            .iter()
            .map(|wall| {
                (
                    Vec3::new(
                        wall.frame.origin.x,
                        roof.base_height_metres,
                        wall.frame.origin.y,
                    ),
                    Vec3::splat(0.12),
                )
            })
            .collect();
    } else if matches!(
        roof.kind,
        RoofKind::Hip | RoofKind::HalfHip | RoofKind::Pavilion
    ) {
        plate_specs = vec![
            (
                Vec3::new(
                    roof.centre.x - roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
            (
                Vec3::new(
                    roof.centre.x + roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y - roof.size.y * 0.5,
                ),
                line_x,
            ),
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y + roof.size.y * 0.5,
                ),
                line_x,
            ),
        ];
    } else if roof.kind == RoofKind::Conical {
        plate_specs = (0..8)
            .map(|index| {
                // Keep ring bearings between the 24 sector drain vertices.
                let angle =
                    std::f32::consts::TAU * index as f32 / 8.0 + std::f32::consts::TAU / 48.0;
                (
                    Vec3::new(
                        roof.centre.x + angle.cos() * roof.size.x * 0.5,
                        roof.base_height_metres,
                        roof.centre.y + angle.sin() * roof.size.y * 0.5,
                    ),
                    Vec3::new(0.14, 0.04, 0.14),
                )
            })
            .collect();
    }
    let support_nodes = (0..plate_specs.len())
        .map(|index| StructuralNodeId(node_base.0 + index as u64))
        .collect::<Vec<_>>();
    for (index, (position, half)) in plate_specs.into_iter().enumerate() {
        let node = support_nodes[index];
        geometry.structural_nodes.push(StructuralNode {
            id: node,
            owner,
            kind: if source_tower_index.is_some() {
                StructuralNodeKind::RoofTowerRing
            } else {
                StructuralNodeKind::RoofWallPlate
            },
            position,
            supported_by: host_nodes.clone(),
            grounded: false,
        });
        geometry.support_interfaces.push(SupportInterface {
            id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | index as u64),
            owner,
            node,
            bounds: ResolvedBounds {
                min: position - half,
                max: position + half,
            },
        });
        let plate_plan = Vec2::new(position.x, position.z);
        let nearest_wall = walls
            .iter()
            .filter(|wall| wall.replaced_by_owner.is_none())
            .filter(|wall| {
                wall.base_elevation_metres <= position.y + 0.02
                    && wall.base_elevation_metres + wall.height_metres >= position.y - 0.02
            })
            .map(|wall| {
                let half_length = wall.length_metres * 0.5;
                let along = (plate_plan - wall.frame.origin)
                    .dot(wall.frame.tangent)
                    .clamp(-half_length, half_length);
                let contact = wall.frame.origin + wall.frame.tangent * along;
                (wall, contact, contact.distance(plate_plan))
            })
            .min_by(|left, right| left.2.total_cmp(&right.2));
        if let Some((wall, contact, distance)) = nearest_wall
            && distance > 0.65
        {
            let direction = (plate_plan - contact).normalize_or_zero();
            let beam_id = ResolvedItemId((0x8_u64 << 60) | (id.0 << 8) | 0x20 | index as u64);
            geometry.solids.push(ResolvedSolid {
                id: beam_id,
                owner,
                centre: Vec3::new(
                    (plate_plan.x + contact.x) * 0.5 + direction.x * 0.01,
                    position.y,
                    (plate_plan.y + contact.y) * 0.5 + direction.y * 0.01,
                ),
                size: Vec3::new((distance - 0.02).max(0.02), 0.18, 0.18),
                yaw_radians: direction.y.atan2(direction.x),
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![wall.support_node],
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | 0x20 | index as u64),
                owner,
                node: wall.support_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(contact.x - 0.09, position.y - 0.09, contact.y - 0.09),
                    max: Vec3::new(contact.x + 0.09, position.y + 0.09, contact.y + 0.09),
                },
            });
        }
        // A dormer may need a concealed support from the host wall to its curb,
        // but never a generic post continuing from that wall to the child
        // eave. The latter produced the two freestanding poles visible in the
        // parent's OpeningCut. Standalone roofs continue to use their eave as
        // the fallback top; child roofs receive the actual curb elevation.
        let support_top = support_post_parent
            .and_then(|parent| roof_surface_height_at(parent, plate_plan))
            .unwrap_or(roof.base_height_metres);
        if support_post_parent.is_none() && host_top.is_finite() && support_top - host_top > 0.35 {
            let height = support_top - host_top;
            let post_id = ResolvedItemId((0x8_u64 << 60) | (id.0 << 8) | index as u64);
            let host_node = host_nodes[0];
            geometry.solids.push(ResolvedSolid {
                id: post_id,
                owner,
                centre: Vec3::new(position.x, host_top + height * 0.5, position.z),
                size: Vec3::new(0.22, height, 0.22),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![host_node],
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | 0x40 | index as u64),
                owner,
                node: host_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(position.x - 0.11, host_top - 0.01, position.z - 0.11),
                    max: Vec3::new(position.x + 0.11, host_top + 0.01, position.z + 0.11),
                },
            });
        }
    }
    if is_church_apse {
        // The polygonal apse uses a continuous timber wall plate between the
        // 11.35 m masonry chord tops and the 11.50 m roof planes.  Besides a
        // credible bearing chain this keeps the eave gutter outside masonry
        // rather than intersecting the opening spandrels at acute corners.
        for (index, wall) in apse_walls.iter().enumerate() {
            let plate_id = ResolvedItemId((0x8_u64 << 60) | (id.0 << 8) | 0x80 | index as u64);
            geometry.solids.push(ResolvedSolid {
                id: plate_id,
                owner,
                centre: Vec3::new(wall.frame.origin.x, 11.425, wall.frame.origin.y),
                size: Vec3::new(wall.length_metres, 0.15, 0.80),
                yaw_radians: -wall.frame.tangent.y.atan2(wall.frame.tangent.x),
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![wall.support_node],
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | 0x80 | index as u64),
                owner,
                node: wall.support_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        wall.frame.origin.x - 0.08,
                        11.335,
                        wall.frame.origin.y - 0.08,
                    ),
                    max: Vec3::new(
                        wall.frame.origin.x + 0.08,
                        11.365,
                        wall.frame.origin.y + 0.08,
                    ),
                },
            });
        }
    }
    let mut faces = Vec::new();
    for (index, polygon) in polygons.iter().enumerate() {
        let face_id = ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | index as u64);
        let catchment_id = ResolvedItemId((0xC_u64 << 60) | (id.0 << 16) | index as u64);
        let route_id = ResolvedItemId((0xD_u64 << 60) | (id.0 << 16) | index as u64);
        let outlet_id = ResolvedItemId((0xE_u64 << 60) | (id.0 << 16) | index as u64);
        let bounds = roof_polygon_bounds(polygon);
        let low = polygon
            .iter()
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .copied()
            .unwrap();
        let centre = polygon.iter().copied().sum::<Vec3>() / polygon.len() as f32;
        geometry.surfaces.push(ResolvedSurface {
            id: catchment_id,
            owner,
            bounds,
            role: SurfaceRole::RoofDrainage,
            shape: crate::ResolvedSurfaceShape::Planar,
        });
        geometry.voids.push(ResolvedVoid {
            id: outlet_id,
            owner,
            bounds: ResolvedBounds {
                min: low - Vec3::splat(0.04),
                max: low + Vec3::splat(0.04),
            },
            role: VoidRole::Drain,
            shape: crate::ResolvedVoidShape::Box,
            subtracts_from: owner,
        });
        geometry.drainage_routes.push(DrainageRoute {
            id: route_id,
            owner,
            outlet_void: outlet_id,
            inlet: centre,
            outlet: low,
        });
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment_id,
            owner,
            walk_solid: face_id,
            toe_channel_solids: Vec::new(),
            drainage_surface: catchment_id,
            outlet_route: route_id,
            centre,
            tangent: Vec2::X,
            outward: Vec2::new(low.x - centre.x, low.z - centre.z).normalize_or_zero(),
            length_metres: (bounds.max.x - bounds.min.x).max(bounds.max.z - bounds.min.z),
            width_metres: (bounds.max.x - bounds.min.x).min(bounds.max.z - bounds.min.z),
            inner_elevation_metres: polygon
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max),
            outer_elevation_metres: low.y,
            outlet_along_metres: 0.0,
        });
        faces.push(RoofFace {
            id: face_id,
            polygon: polygon.to_vec(),
            cutouts: Vec::new(),
            plane: roof_plane(polygon),
            pitch_degrees: roof.pitch_degrees,
            thickness_metres: thickness,
            material: RoofMaterial::ClayTile,
            support_nodes: support_nodes.clone(),
            drainage_catchment: catchment_id,
        });
    }
    let mut edges: Vec<RoofEdge> = Vec::new();
    for face in &faces {
        for index in 0..face.polygon.len() {
            let a = face.polygon[index];
            let b = face.polygon[(index + 1) % face.polygon.len()];
            if let Some(edge) = edges.iter_mut().find(|edge| {
                (same_roof_vertex(edge.start, a) && same_roof_vertex(edge.end, b))
                    || (same_roof_vertex(edge.start, b) && same_roof_vertex(edge.end, a))
            }) {
                edge.adjacent_faces.push(face.id);
            } else {
                let edge_id = ResolvedItemId((0xB_u64 << 60) | (id.0 << 16) | edges.len() as u64);
                edges.push(RoofEdge {
                    id: edge_id,
                    start: a,
                    end: b,
                    kind: RoofEdgeKind::Eave,
                    adjacent_faces: vec![face.id],
                    flashing: None,
                    drainage_terminal: None,
                });
            }
        }
    }
    for (edge_index, edge) in edges.iter_mut().enumerate() {
        if edge.adjacent_faces.len() == 2 {
            edge.kind = if (edge.start.y - edge.end.y).abs() <= 0.01 {
                RoofEdgeKind::Ridge
            } else {
                RoofEdgeKind::Hip
            };
        } else if (edge.start.y - edge.end.y).abs() <= 0.01
            && (((edge.start.y - roof.base_height_metres).abs() <= 0.01
                && (edge.end.y - roof.base_height_metres).abs() <= 0.01)
                || roof.kind == RoofKind::HalfHip)
        {
            edge.kind = RoofEdgeKind::Eave;
            edge.drainage_terminal = faces
                .iter()
                .find(|face| {
                    face.polygon
                        .iter()
                        .any(|p| same_roof_vertex(*p, edge.start))
                        && face.polygon.iter().any(|p| same_roof_vertex(*p, edge.end))
                })
                .and_then(|face| {
                    geometry
                        .drainage_catchments
                        .iter()
                        .find(|catchment| catchment.id == face.drainage_catchment)
                })
                .and_then(|catchment| {
                    geometry
                        .drainage_routes
                        .iter()
                        .find(|route| route.id == catchment.outlet_route)
                })
                .map(|route| route.outlet_void);
        } else if roof.kind == RoofKind::Shed && (edge.start.y - edge.end.y).abs() <= 0.01 {
            edge.kind = RoofEdgeKind::WallAbutment;
            let flashing_id =
                ResolvedItemId((0x8_u64 << 60) | (id.0 << 16) | 0x5800 | edge_index as u64);
            edge.flashing = Some(flashing_id);
            let delta = edge.end - edge.start;
            geometry.solids.push(ResolvedSolid {
                id: flashing_id,
                owner,
                centre: (edge.start + edge.end) * 0.5 + Vec3::Y * 0.035,
                size: Vec3::new(Vec2::new(delta.x, delta.z).length(), 0.07, 0.18),
                yaw_radians: delta.z.atan2(delta.x),
                crossfall_radians: 0.12,
                longfall_radians: 0.0,
                role: SolidRole::RoofFlashing,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: support_nodes.clone(),
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 16) | 0x5800 | edge_index as u64),
                owner,
                node: support_nodes[0],
                bounds: ResolvedBounds {
                    min: (edge.start + edge.end) * 0.5 - Vec3::new(0.08, 0.025, 0.08),
                    max: (edge.start + edge.end) * 0.5 + Vec3::new(0.08, 0.025, 0.08),
                },
            });
        } else {
            edge.kind = RoofEdgeKind::GableVerge;
        }
        if matches!(
            edge.kind,
            RoofEdgeKind::Eave | RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::GableVerge
        ) {
            let weather_id =
                ResolvedItemId((0x8_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64);
            let delta = edge.end - edge.start;
            let plan_length = Vec2::new(delta.x, delta.z).length().max(0.05);
            let centre = (edge.start + edge.end) * 0.5;
            // Roof faces may drain to either end of a shared perimeter eave.
            // Leave a physical outlet gap at both corners rather than letting
            // the gutter cuboid seal an adjacent face's terminal.
            let treated_plan_length = if edge.kind == RoofEdgeKind::Eave {
                (plan_length - 0.36_f32.min(plan_length * 0.5)).max(0.05)
            } else {
                plan_length
            };
            let edge_pitch = delta.y.atan2(plan_length);
            let treated_length = if edge.kind == RoofEdgeKind::Eave {
                treated_plan_length
            } else {
                // Edge treatments are authoritative solids on the actual 3D
                // edge, not horizontal plan-projection bars.
                treated_plan_length / edge_pitch.cos().abs().max(0.01)
            };
            geometry.solids.push(ResolvedSolid {
                id: weather_id,
                owner,
                centre: centre
                    + if edge.kind == RoofEdgeKind::Eave {
                        Vec3::NEG_Y * 0.06
                    } else {
                        Vec3::Y * 0.035
                    },
                size: Vec3::new(
                    treated_length,
                    if edge.kind == RoofEdgeKind::Eave {
                        0.12
                    } else {
                        0.07
                    },
                    if edge.kind == RoofEdgeKind::Eave {
                        0.16
                    } else {
                        0.14
                    },
                ),
                yaw_radians: delta.z.atan2(delta.x),
                // Edge treatment's long axis is the typed source contour.
                // Applying coping crossfall as an X rotation skewed that axis
                // off the roof plane and produced detached diagonal rods.
                crossfall_radians: 0.0,
                longfall_radians: if edge.kind == RoofEdgeKind::Eave {
                    0.012
                } else {
                    edge_pitch
                },
                role: if edge.kind == RoofEdgeKind::Eave {
                    SolidRole::RoofGutter
                } else {
                    SolidRole::RoofEdgeTreatment
                },
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: support_nodes.clone(),
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64),
                owner,
                node: support_nodes[0],
                bounds: ResolvedBounds {
                    min: centre - Vec3::new(0.08, 0.025, 0.08),
                    max: centre + Vec3::new(0.08, 0.025, 0.08),
                },
            });
        }
    }
    let hx = roof.size.x * 0.5;
    let hz = roof.size.y * 0.5;
    let roof_grid_point = |position: Vec2| {
        GridPoint::new(
            (position.x / GRID_UNIT_METRES).round() as i32,
            (position.y / GRID_UNIT_METRES).round() as i32,
        )
    };
    let infill_material = if walls
        .iter()
        .any(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
    {
        RoofMaterial::TimberInfill
    } else {
        RoofMaterial::MasonryInfill
    };
    let mut enclosure_faces = Vec::new();
    if roof.kind == RoofKind::Gable {
        let apex_y = faces
            .iter()
            .flat_map(|face| &face.polygon)
            .map(|point| point.y)
            .fold(roof.base_height_metres, f32::max);
        let (first, second) = match roof.ridge_axis {
            RidgeAxis::Z => {
                let triangle = |z: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(
                            roof.centre.x - roof.size.x * 0.5,
                            roof.base_height_metres,
                            z,
                        ),
                        Vec3::new(roof.centre.x, apex_y, z),
                        Vec3::new(
                            roof.centre.x + roof.size.x * 0.5,
                            roof.base_height_metres,
                            z,
                        ),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    triangle(roof.centre.y - roof.size.y * 0.5, false),
                    triangle(roof.centre.y + roof.size.y * 0.5, true),
                )
            }
            RidgeAxis::X => {
                let triangle = |x: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(
                            x,
                            roof.base_height_metres,
                            roof.centre.y - roof.size.y * 0.5,
                        ),
                        Vec3::new(x, apex_y, roof.centre.y),
                        Vec3::new(
                            x,
                            roof.base_height_metres,
                            roof.centre.y + roof.size.y * 0.5,
                        ),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    triangle(roof.centre.x - roof.size.x * 0.5, true),
                    triangle(roof.centre.x + roof.size.x * 0.5, false),
                )
            }
        };
        for (index, polygon) in [first, second].into_iter().enumerate() {
            enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4000 | index as u64),
                polygon,
                material: infill_material,
                support_nodes: support_nodes.clone(),
            });
        }
    }
    if roof.kind == RoofKind::HalfHip {
        let face_hx = roof.size.x * 0.5 + roof.eave_metres;
        let face_hz = roof.size.y * 0.5 + roof.eave_metres;
        let shoulder_fraction = 0.55;
        let polygons = match roof.ridge_axis {
            RidgeAxis::Z => {
                let shoulder_x = face_hx * (1.0 - shoulder_fraction);
                let shoulder_y = roof.base_height_metres
                    + face_hx * roof.pitch_degrees.to_radians().tan() * shoulder_fraction;
                let gable = |z: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(roof.centre.x - face_hx, roof.base_height_metres, z),
                        Vec3::new(roof.centre.x + face_hx, roof.base_height_metres, z),
                        Vec3::new(roof.centre.x + shoulder_x, shoulder_y, z),
                        Vec3::new(roof.centre.x - shoulder_x, shoulder_y, z),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                vec![
                    gable(roof.centre.y - face_hz, false),
                    gable(roof.centre.y + face_hz, true),
                ]
            }
            RidgeAxis::X => {
                let shoulder_z = face_hz * (1.0 - shoulder_fraction);
                let shoulder_y = roof.base_height_metres
                    + face_hz * roof.pitch_degrees.to_radians().tan() * shoulder_fraction;
                let gable = |x: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(x, roof.base_height_metres, roof.centre.y - face_hz),
                        Vec3::new(x, roof.base_height_metres, roof.centre.y + face_hz),
                        Vec3::new(x, shoulder_y, roof.centre.y + shoulder_z),
                        Vec3::new(x, shoulder_y, roof.centre.y - shoulder_z),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                vec![
                    gable(roof.centre.x - face_hx, true),
                    gable(roof.centre.x + face_hx, false),
                ]
            }
        };
        for (index, polygon) in polygons.into_iter().enumerate() {
            enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4200 | index as u64),
                polygon,
                material: infill_material,
                support_nodes: support_nodes.clone(),
            });
        }
    }
    // A raised primary roof needs an actual clerestory/attic wall under each
    // eave; posts alone are a support skeleton, not a weather-tight building
    // envelope.  This is especially important for the cathedral nave above
    // its independent aisle roofs.
    if parent.is_none()
        && host_top.is_finite()
        && roof.base_height_metres - host_top > 0.35
        && matches!(roof.kind, RoofKind::Gable | RoofKind::Shed)
    {
        let (first, second) = match roof.ridge_axis {
            RidgeAxis::Z => {
                let wall = |x: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(x, host_top, roof.centre.y - hz),
                        Vec3::new(x, host_top, roof.centre.y + hz),
                        Vec3::new(x, roof.base_height_metres, roof.centre.y + hz),
                        Vec3::new(x, roof.base_height_metres, roof.centre.y - hz),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    wall(roof.centre.x - hx, true),
                    wall(roof.centre.x + hx, false),
                )
            }
            RidgeAxis::X => {
                let wall = |z: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(roof.centre.x - hx, host_top, z),
                        Vec3::new(roof.centre.x + hx, host_top, z),
                        Vec3::new(roof.centre.x + hx, roof.base_height_metres, z),
                        Vec3::new(roof.centre.x - hx, roof.base_height_metres, z),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    wall(roof.centre.y - hz, false),
                    wall(roof.centre.y + hz, true),
                )
            }
        };
        for (slot, polygon) in [first, second].into_iter().enumerate() {
            enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4300 | slot as u64),
                polygon,
                material: infill_material,
                support_nodes: support_nodes.clone(),
            });
        }
    }
    RoofAssembly {
        id,
        owner,
        kind: roof.kind,
        outer_loop: RoofFootprintLoop {
            vertices: apse_outline.map_or_else(
                || {
                    vec![
                        roof_grid_point(roof.centre + Vec2::new(-hx, -hz)),
                        roof_grid_point(roof.centre + Vec2::new(hx, -hz)),
                        roof_grid_point(roof.centre + Vec2::new(hx, hz)),
                        roof_grid_point(roof.centre + Vec2::new(-hx, hz)),
                    ]
                },
                |outline| outline.into_iter().map(roof_grid_point).collect(),
            ),
        },
        holes: Vec::new(),
        faces,
        enclosure_faces,
        edges,
        children: Vec::new(),
        abutments: Vec::new(),
        parent,
        material: RoofMaterial::ClayTile,
        phase,
        pivot_policy: RoofPivotPolicy::KeepEave,
        shed_high_side,
        support_nodes,
        source_piece_index,
        source_tower_index,
    }
}
