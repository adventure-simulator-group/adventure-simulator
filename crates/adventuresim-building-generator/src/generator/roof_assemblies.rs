fn resolve_roof_assemblies(
    program: &BuildingProgram,
    roofs: &[RoofPiece],
    dormers: &[RoofDormer],
    towers: &[RoundTower],
    square_towers: &[SquareTower],
    stairs: &[Stair],
    walls: &[crate::WallAssembly],
    openings: &[crate::OpeningAssembly],
    geometry: &mut ResolvedGeometry,
) -> Vec<RoofAssembly> {
    let mut assemblies = Vec::new();
    for (index, roof) in roofs.iter().copied().enumerate() {
        let id = RoofAssemblyId(index as u64 + 1);
        let shed_high_side = match (program.archetype, index, roof.kind) {
            (BuildingArchetype::Cathedral, 1, RoofKind::Shed) => Some(Direction::North),
            (BuildingArchetype::Cathedral, 2, RoofKind::Shed) => Some(Direction::South),
            (_, _, RoofKind::Shed) => Some(match roof.ridge_axis {
                RidgeAxis::Z => Direction::East,
                RidgeAxis::X => Direction::North,
            }),
            _ => None,
        };
        assemblies.push(resolve_one_roof(
            id,
            GeometryOwnerId(60_000 + index as u32),
            roof,
            Some(index),
            None,
            None,
            RoofPhase::Primary,
            shed_high_side,
            None,
            walls,
            geometry,
        ));
    }
    if let Some(parent) = assemblies
        .first()
        .map(|assembly| (assembly.id, assembly.owner))
    {
        for index in 1..roofs.len() {
            let child_recipe = roofs[index];
            let parent_recipe = roofs[0];
            let child_min = child_recipe.centre - child_recipe.size * 0.5;
            let child_max = child_recipe.centre + child_recipe.size * 0.5;
            let parent_min = parent_recipe.centre - parent_recipe.size * 0.5;
            let parent_max = parent_recipe.centre + parent_recipe.size * 0.5;
            let overlaps = child_min.x < parent_max.x
                && child_max.x > parent_min.x
                && child_min.y < parent_max.y
                && child_max.y > parent_min.y;
            if overlaps && child_recipe.base_height_metres > parent_recipe.base_height_metres + 0.5
            {
                let child_id = assemblies[index].id;
                assemblies[index].parent = Some(parent.0);
                assemblies[index].phase = RoofPhase::AttachedChild;
                let enclosure_material = if walls
                    .iter()
                    .any(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
                {
                    RoofMaterial::TimberInfill
                } else {
                    RoofMaterial::MasonryInfill
                };
                let enclosure_supports = assemblies[index].support_nodes.clone();
                let top = child_recipe.base_height_metres;
                let parent_snapshot = assemblies[0].clone();
                let parent_height = |point: Vec2| {
                    roof_surface_height_at(&parent_snapshot, point)
                        .unwrap_or(parent_recipe.base_height_metres)
                };
                for (slot, polygon) in [
                    vec![
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(child_max.x, top, child_min.y),
                        Vec3::new(child_min.x, top, child_min.y),
                    ],
                    vec![
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(child_min.x, top, child_max.y),
                        Vec3::new(child_max.x, top, child_max.y),
                    ],
                    vec![
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(child_min.x, top, child_min.y),
                        Vec3::new(child_min.x, top, child_max.y),
                    ],
                    vec![
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(child_max.x, top, child_max.y),
                        Vec3::new(child_max.x, top, child_min.y),
                    ],
                ]
                .into_iter()
                .enumerate()
                {
                    assemblies[index].enclosure_faces.push(RoofEnclosureFace {
                        id: ResolvedItemId(
                            (0xA_u64 << 60) | (child_id.0 << 16) | 0x4200 | slot as u64,
                        ),
                        polygon,
                        material: enclosure_material,
                        support_nodes: enclosure_supports.clone(),
                    });
                }
                let cut_id = ResolvedItemId((0xF_u64 << 60) | child_id.0);
                let bounds = ResolvedBounds {
                    min: Vec3::new(
                        child_min.x,
                        parent_recipe.base_height_metres - 0.2,
                        child_min.y,
                    ),
                    max: Vec3::new(
                        child_max.x,
                        child_recipe.base_height_metres + 5.0,
                        child_max.y,
                    ),
                };
                geometry.voids.push(ResolvedVoid {
                    id: cut_id,
                    owner: parent.1,
                    bounds,
                    role: VoidRole::RoofOpening,
                    shape: crate::ResolvedVoidShape::Box,
                    subtracts_from: parent.1,
                });
                let child_supports = assemblies[index].support_nodes.clone();
                let child_copy = assemblies[index].clone();
                let cut_edges =
                    cut_parent_roof_face(&mut assemblies[0], &child_copy, bounds, geometry);
                let valleys =
                    bind_child_valleys(&mut assemblies[0], &child_copy, &cut_edges, geometry);
                let flashing_ids = assemblies[0]
                    .edges
                    .iter()
                    .filter(|edge| cut_edges.contains(&edge.id))
                    .filter_map(|edge| edge.flashing)
                    .collect();
                assemblies[0].children.push(RoofChildAssembly {
                    child: child_id,
                    kind: RoofChildKind::CrossGable,
                    parent_cut: cut_id,
                    trimmer_nodes: child_supports,
                    valley_edges: valleys,
                    flashing_ids,
                    facade_wall: None,
                    split_eave_edges: Vec::new(),
                });
            }
        }
    }
    let parent = assemblies.first().map(|roof| roof.id);
    for (index, dormer) in dormers.iter().copied().enumerate() {
        let scale = if dormer.kind == DormerKind::TransverseGable {
            2.20
        } else {
            1.0
        };
        let inward = match dormer.facing {
            Direction::North => -Vec2::Y,
            Direction::South => Vec2::Y,
            Direction::East => -Vec2::X,
            Direction::West => Vec2::X,
        };
        let ridge_axis = if matches!(dormer.facing, Direction::North | Direction::South) {
            RidgeAxis::Z
        } else {
            RidgeAxis::X
        };
        let top = dormer.base_height_metres
            + dormer.height_metres
                * if dormer.kind == DormerKind::TransverseGable {
                    1.35
                } else {
                    1.0
                };
        let tangent = if matches!(dormer.facing, Direction::North | Direction::South) {
            Vec2::X
        } else {
            Vec2::Y
        };
        let half_width = dormer.width_metres * scale * 0.5;
        let roof_eave = if dormer.kind == DormerKind::TransverseGable {
            0.16
        } else {
            0.10
        };
        let fallback_depth = dormer.depth_metres * 0.84;
        let minimum_usable_depth = fallback_depth.min(0.75);
        let seam_depth_at_height = |required_height: f32| {
            assemblies.first().and_then(|parent| {
                (0..=800)
                    .map(|step| minimum_usable_depth + roof_eave + step as f32 * 0.01)
                    .find(|depth| {
                        [-1.0_f32, 1.0].into_iter().all(|side| {
                            let point = dormer.centre
                                + inward * *depth
                                + tangent * side * (half_width + roof_eave);
                            roof_surface_height_at(parent, point)
                                .is_some_and(|height| height >= required_height - 0.015)
                        })
                    })
                    .map(|rear_edge_depth| {
                        (rear_edge_depth - roof_eave).max(minimum_usable_depth)
                    })
            })
        };
        // The rear edge of a dormer is not a second free gable. Extend the
        // child inward until its eave plane meets the actual parent weather
        // plane at both cheeks. The small overhang then seats on that seam.
        let enclosure_depth = seam_depth_at_height(top)
            .unwrap_or(fallback_depth);
        // A cross-gable does not have a rectangular rear edge: its low eaves
        // meet the parent first, while its ridge continues inward to the
        // higher intersection point. Ordinary dormers retain a square head.
        let ridge_seam_depth = if dormer.kind == DormerKind::TransverseGable {
            let ridge_height = top + 48.0_f32.to_radians().tan() * half_width;
            seam_depth_at_height(ridge_height).unwrap_or(enclosure_depth)
        } else {
            enclosure_depth
        };
        let size = if ridge_axis == RidgeAxis::Z {
            Vec2::new(dormer.width_metres * scale, enclosure_depth)
        } else {
            Vec2::new(enclosure_depth, dormer.width_metres * scale)
        };
        let recipe = RoofPiece {
            kind: if dormer.kind == DormerKind::Shed {
                RoofKind::Shed
            } else {
                RoofKind::Gable
            },
            centre: dormer.centre + inward * enclosure_depth * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: 48.0,
            ridge_axis,
            eave_metres: roof_eave,
            gable_profile: dormer.gable_profile,
        };
        let id = RoofAssemblyId(1_000 + index as u64);
        let mut child = resolve_one_roof(
            id,
            GeometryOwnerId(61_000 + index as u32),
            recipe,
            None,
            None,
            parent,
            RoofPhase::AttachedChild,
            (recipe.kind == RoofKind::Shed).then_some(dormer.facing.opposite()),
            assemblies.first(),
            walls,
            geometry,
        );
        if ridge_seam_depth > enclosure_depth + 0.01 {
            let extension = inward * (ridge_seam_depth - enclosure_depth);
            let mut moved_ridge_points = Vec::new();
            for face in &mut child.faces {
                let ridge_height = face
                    .polygon
                    .iter()
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                let Some(rear_ridge_index) = face
                    .polygon
                    .iter()
                    .enumerate()
                    .filter(|(_, point)| (point.y - ridge_height).abs() <= 0.01)
                    .max_by(|(_, left), (_, right)| {
                        Vec2::new(left.x, left.z)
                            .dot(inward)
                            .total_cmp(&Vec2::new(right.x, right.z).dot(inward))
                    })
                    .map(|(index, _)| index)
                else {
                    continue;
                };
                let old = face.polygon[rear_ridge_index];
                let new = old + Vec3::new(extension.x, 0.0, extension.y);
                face.polygon[rear_ridge_index] = new;
                moved_ridge_points.push((old, new));
            }
            for edge in &mut child.edges {
                for (old, new) in &moved_ridge_points {
                    if edge.start.distance_squared(*old) <= 0.000_004 {
                        edge.start = *new;
                    }
                    if edge.end.distance_squared(*old) <= 0.000_004 {
                        edge.end = *new;
                    }
                }
            }
        }
        if recipe.kind == RoofKind::Gable {
            // `resolve_one_roof` normally closes both gable ends. A dormer
            // owns only the visible front gable; its rear terminates in the
            // parent weather plane. Remove the otherwise floating rear
            // triangle and its two verge caps. The parent's cut-edge flashing
            // owns the seated head joint.
            child.enclosure_faces.retain(|face| {
                let mean_depth = face
                    .polygon
                    .iter()
                    .map(|point| (Vec2::new(point.x, point.z) - dormer.centre).dot(-inward))
                    .sum::<f32>()
                    / face.polygon.len() as f32;
                mean_depth > -enclosure_depth + 0.02
            });
            let rear_edge_depth = -(enclosure_depth + roof_eave);
            for (edge_index, edge) in child.edges.iter_mut().enumerate() {
                let start_depth =
                    (Vec2::new(edge.start.x, edge.start.z) - dormer.centre).dot(-inward);
                let end_depth = (Vec2::new(edge.end.x, edge.end.z) - dormer.centre).dot(-inward);
                if edge.kind == RoofEdgeKind::GableVerge
                    && start_depth <= rear_edge_depth + 0.02
                    && end_depth <= rear_edge_depth + 0.02
                {
                    edge.kind = RoofEdgeKind::OpeningCut;
                    let weather_id =
                        ResolvedItemId((0x8_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64);
                    let interface_id =
                        ResolvedItemId((0x9_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64);
                    geometry.solids.retain(|solid| solid.id != weather_id);
                    geometry
                        .support_interfaces
                        .retain(|interface| interface.id != interface_id);
                }
            }
        }
        let front_left = dormer.centre - tangent * half_width;
        let front_right = dormer.centre + tangent * half_width;
        let rear_left = front_left + inward * enclosure_depth;
        let rear_right = front_right + inward * enclosure_depth;
        let parent_height = |point: Vec2| {
            assemblies
                .first()
                .and_then(|parent| roof_surface_height_at(parent, point))
                .unwrap_or(dormer.base_height_metres)
        };
        for (slot, polygon) in [
            vec![
                Vec3::new(front_left.x, parent_height(front_left), front_left.y),
                Vec3::new(front_right.x, parent_height(front_right), front_right.y),
                Vec3::new(front_right.x, top, front_right.y),
                Vec3::new(front_left.x, top, front_left.y),
            ],
            vec![
                Vec3::new(front_left.x, parent_height(front_left), front_left.y),
                Vec3::new(front_left.x, top, front_left.y),
                Vec3::new(rear_left.x, top, rear_left.y),
                Vec3::new(rear_left.x, parent_height(rear_left), rear_left.y),
            ],
            vec![
                Vec3::new(front_right.x, parent_height(front_right), front_right.y),
                Vec3::new(rear_right.x, parent_height(rear_right), rear_right.y),
                Vec3::new(rear_right.x, top, rear_right.y),
                Vec3::new(front_right.x, top, front_right.y),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            child.enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4100 | slot as u64),
                polygon,
                material: if walls
                    .iter()
                    .any(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
                {
                    RoofMaterial::TimberInfill
                } else {
                    RoofMaterial::MasonryInfill
                },
                support_nodes: child.support_nodes.clone(),
            });
        }
        if parent.is_some() {
            let cut_id = ResolvedItemId((0xF_u64 << 60) | id.0);
            let bounds = ResolvedBounds {
                min: Vec3::new(
                    recipe.centre.x - recipe.size.x * 0.5,
                    recipe.base_height_metres - 0.2,
                    recipe.centre.y - recipe.size.y * 0.5,
                ),
                max: Vec3::new(
                    recipe.centre.x + recipe.size.x * 0.5,
                    recipe.base_height_metres + 4.0,
                    recipe.centre.y + recipe.size.y * 0.5,
                ),
            };
            geometry.voids.push(ResolvedVoid {
                id: cut_id,
                owner: assemblies[0].owner,
                bounds,
                role: VoidRole::RoofOpening,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assemblies[0].owner,
            });
            let child_kind = match dormer.kind {
                DormerKind::Gabled | DormerKind::Hipped => RoofChildKind::GabledDormer,
                DormerKind::Shed => RoofChildKind::ShedDormer,
                DormerKind::TransverseGable => RoofChildKind::CrossGable,
            };
            let cut_edges = cut_parent_roof_face(&mut assemblies[0], &child, bounds, geometry);
            let valleys = bind_child_valleys(&mut assemblies[0], &child, &cut_edges, geometry);
            let flashing_ids = assemblies[0]
                .edges
                .iter()
                .filter(|edge| cut_edges.contains(&edge.id))
                .filter_map(|edge| edge.flashing)
                .collect();
            assemblies[0].children.push(RoofChildAssembly {
                child: id,
                kind: child_kind,
                parent_cut: cut_id,
                trimmer_nodes: child.support_nodes.clone(),
                valley_edges: valleys,
                flashing_ids,
                facade_wall: None,
                split_eave_edges: Vec::new(),
            });
            if dormer.kind == DormerKind::TransverseGable {
                split_cross_gable_parent_eave(
                    &mut assemblies[0],
                    id,
                    dormer.centre,
                    tangent,
                    dormer.width_metres * scale,
                );
            }
        }
        child.phase = RoofPhase::AttachedChild;
        assemblies.push(child);
    }
    for (index, tower) in towers.iter().copied().enumerate() {
        if let Some(roof) = tower.roof {
            let id = RoofAssemblyId(2_000 + index as u64);
            assemblies.push(resolve_one_roof(
                id,
                GeometryOwnerId(62_000 + index as u32),
                roof,
                None,
                Some(index),
                None,
                RoofPhase::Primary,
                None,
                None,
                walls,
                geometry,
            ));
        }
    }
    for (index, tower) in square_towers.iter().copied().enumerate() {
        let id = RoofAssemblyId(3_000 + index as u64);
        assemblies.push(resolve_one_roof(
            id,
            GeometryOwnerId(63_000 + index as u32),
            tower.roof,
            None,
            Some(index),
            None,
            RoofPhase::Primary,
            None,
            None,
            walls,
            geometry,
        ));
    }
    // A tower piercing the principal roof is a true abutment, not two
    // overlapping independent meshes. Cut the main weather faces to the
    // tower footprint, flash every resulting edge, and bind the tower roof as
    // a child carried by its own masonry ring.
    if !square_towers.is_empty() && !assemblies.is_empty() {
        for (tower_index, tower) in square_towers.iter().copied().enumerate() {
            let Some(child_index) = assemblies
                .iter()
                .position(|roof| roof.id == RoofAssemblyId(3_000 + tower_index as u64))
            else {
                continue;
            };
            let child = assemblies[child_index].clone();
            let parent_id = assemblies[0].id;
            let cut_id = ResolvedItemId((0xF_u64 << 60) | child.id.0);
            // `SquareTower::size` locates the four authoritative wall
            // centrelines.  The parent roof must stop at the exterior shell
            // faces, not halfway through the masonry.
            let shell_half_thickness = walls
                .iter()
                .filter_map(|wall| match wall.source {
                    crate::WallSourceId::SquareTowerFace {
                        tower_index: source_tower,
                        ..
                    } if source_tower == tower_index => Some(wall.thickness_metres * 0.5),
                    _ => None,
                })
                .fold(0.0_f32, f32::max);
            let half = tower.size * 0.5 + Vec2::splat(shell_half_thickness);
            let bounds = ResolvedBounds {
                min: Vec3::new(
                    tower.centre.x - half.x,
                    assemblies[0]
                        .faces
                        .iter()
                        .flat_map(|face| face.polygon.iter().map(|point| point.y))
                        .fold(f32::INFINITY, f32::min)
                        - 0.2,
                    tower.centre.y - half.y,
                ),
                max: Vec3::new(
                    tower.centre.x + half.x,
                    tower.wall_height_metres + 8.0,
                    tower.centre.y + half.y,
                ),
            };
            geometry.voids.push(ResolvedVoid {
                id: cut_id,
                owner: assemblies[0].owner,
                bounds,
                role: VoidRole::RoofOpening,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assemblies[0].owner,
            });
            let cut_height = bounds.max.y;
            let mut vertical_cut = child.clone();
            let mut cut_face = child.faces[0].clone();
            cut_face.polygon = vec![
                Vec3::new(bounds.min.x, cut_height, bounds.min.z),
                Vec3::new(bounds.max.x, cut_height, bounds.min.z),
                Vec3::new(bounds.max.x, cut_height, bounds.max.z),
                Vec3::new(bounds.min.x, cut_height, bounds.max.z),
            ];
            cut_face.cutouts.clear();
            cut_face.plane = RoofPlaneEquation {
                normal: Vec3::Y,
                constant: -cut_height,
            };
            vertical_cut.faces = vec![cut_face];
            trim_roof_edge_treatments_for_cut(assemblies[0].owner, bounds, geometry);
            trim_roof_boundary_edges_for_cut(&mut assemblies[0], bounds);
            let cut_edges =
                cut_parent_roof_face(&mut assemblies[0], &vertical_cut, bounds, geometry);
            for edge in assemblies[0]
                .edges
                .iter_mut()
                .filter(|edge| cut_edges.contains(&edge.id))
            {
                edge.kind = RoofEdgeKind::TowerAbutment;
            }
            let flashing_ids = assemblies[0]
                .edges
                .iter()
                .filter(|edge| cut_edges.contains(&edge.id))
                .filter_map(|edge| edge.flashing)
                .collect::<Vec<_>>();
            assemblies[0].children.push(RoofChildAssembly {
                child: child.id,
                kind: RoofChildKind::Tower,
                parent_cut: cut_id,
                trimmer_nodes: child.support_nodes.clone(),
                valley_edges: cut_edges,
                flashing_ids,
                facade_wall: None,
                split_eave_edges: Vec::new(),
            });
            assemblies[child_index].parent = Some(parent_id);
            assemblies[child_index].phase = RoofPhase::AttachedChild;
        }
    }
    bind_coincident_primary_roof_edges(&mut assemblies, geometry);
    finalize_roof_drainage(program.archetype, &mut assemblies, geometry);
    supplement_split_eave_drainage(&assemblies, geometry);
    consolidate_roof_outlet_stations(
        program.archetype,
        &mut assemblies,
        stairs,
        walls,
        openings,
        geometry,
    );
    resolve_roof_abutment_contours(&mut assemblies, walls, geometry);
    // Tower/child clipping can shorten a verge after its treatment was first
    // resolved.  Refit the authoritative treatment to the final typed edge;
    // retaining the pre-cut bar would create a detached rod across the cut.
    let mut orphan_treatments = HashSet::new();
    for assembly in &mut assemblies {
        for treatment in geometry.solids.iter_mut().filter(|solid| {
            solid.owner == assembly.owner && solid.role == SolidRole::RoofEdgeTreatment
        }) {
            let pitch_cosine = treatment.longfall_radians.cos();
            let axis = Vec3::new(
                treatment.yaw_radians.cos() * pitch_cosine,
                treatment.longfall_radians.sin(),
                treatment.yaw_radians.sin() * pitch_cosine,
            );
            let endpoints = [
                treatment.centre - axis * treatment.size.x * 0.5,
                treatment.centre + axis * treatment.size.x * 0.5,
            ];
            let aligned = assembly.edges.iter().any(|edge| {
                if !matches!(
                    edge.kind,
                    RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::GableVerge
                ) {
                    return false;
                }
                let delta = edge.end - edge.start;
                let length_squared = delta.length_squared().max(0.000_001);
                treatment.size.x <= delta.length() + 0.03
                    && endpoints.iter().all(|point| {
                        let raw_t = (*point - edge.start).dot(delta) / length_squared;
                        let t = raw_t.clamp(0.0, 1.0);
                        point.distance(edge.start + delta * t) <= 0.075
                            && (-0.02..=1.02).contains(&raw_t)
                    })
            });
            if !aligned {
                orphan_treatments.insert(treatment.id);
            }
        }
        for edge in &mut assembly.edges {
            if edge
                .flashing
                .is_some_and(|id| orphan_treatments.contains(&id))
            {
                edge.flashing = None;
            }
        }
    }
    geometry
        .solids
        .retain(|solid| !orphan_treatments.contains(&solid.id));
    geometry.support_interfaces.retain(|interface| {
        !orphan_treatments.iter().any(|id| {
            interface.id == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
        })
    });
    for treatment in geometry
        .solids
        .iter()
        .filter(|solid| solid.role == SolidRole::RoofEdgeTreatment)
    {
        let interface_id =
            ResolvedItemId((0x9_u64 << 60) | (treatment.id.0 & 0x0FFF_FFFF_FFFF_FFFF));
        if let Some(interface) = geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
        {
            interface.bounds = ResolvedBounds {
                min: treatment.centre - Vec3::new(0.08, 0.025, 0.08),
                max: treatment.centre + Vec3::new(0.08, 0.025, 0.08),
            };
        }
    }
    let roof_owners = assemblies
        .iter()
        .map(|roof| roof.owner)
        .collect::<HashSet<_>>();
    let mut roof_bonds = Vec::new();
    for left in 0..geometry.solids.len() {
        for right in left + 1..geometry.solids.len() {
            let a = &geometry.solids[left];
            let b = &geometry.solids[right];
            if a.owner == b.owner
                || (!roof_owners.contains(&a.owner) && !roof_owners.contains(&b.owner))
            {
                continue;
            }
            let yaw_bounds = |solid: &ResolvedSolid| {
                let cosine = solid.yaw_radians.cos().abs();
                let sine = solid.yaw_radians.sin().abs();
                let half = Vec3::new(
                    (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                    solid.size.y * 0.5,
                    (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                );
                ResolvedBounds {
                    min: solid.centre - half,
                    max: solid.centre + half,
                }
            };
            let a_bounds = yaw_bounds(a);
            let b_bounds = yaw_bounds(b);
            let min = a_bounds.min.max(b_bounds.min);
            let max = a_bounds.max.min(b_bounds.max);
            let overlap = max - min;
            if overlap.min_element() > 0.001 {
                roof_bonds.push(JunctionBond {
                    id: ResolvedItemId((0x6_u64 << 60) | roof_bonds.len() as u64),
                    owners: [a.owner, b.owner],
                    bounds: ResolvedBounds {
                        min: min - Vec3::splat(0.01),
                        max: max + Vec3::splat(0.01),
                    },
                    minimum_interface_area_square_metres: 0.005,
                    maximum_penetration_metres: overlap.x.min(overlap.z).min(0.18),
                });
            }
        }
    }
    geometry.junction_bonds.extend(roof_bonds);
    assemblies
}
