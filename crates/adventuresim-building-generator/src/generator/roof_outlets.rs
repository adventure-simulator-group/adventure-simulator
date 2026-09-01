/// Consolidate face gutters into a small set of physical outlet stations.
/// Project gates: principal gable/hip roofs use at most two stations and
/// round/pavilion roofs at most four. Attached children explicitly free-drip
/// to the parent weather face instead of growing a detached pipe to grade.
fn consolidate_roof_outlet_stations(
    archetype: BuildingArchetype,
    assemblies: &mut [RoofAssembly],
    stairs: &[Stair],
    walls: &[crate::WallAssembly],
    _openings: &[crate::OpeningAssembly],
    geometry: &mut ResolvedGeometry,
) {
    geometry.roof_drainage_outlets.clear();
    let assembly_read = assemblies.to_vec();
    for assembly in &assembly_read {
        let cross_facade_wall = assembly.parent.and_then(|parent_id| {
            assembly_read
                .iter()
                .find(|parent| parent.id == parent_id)
                .and_then(|parent| {
                    parent.children.iter().find_map(|child| {
                        (child.child == assembly.id && child.kind == RoofChildKind::CrossGable)
                            .then_some(child.facade_wall)
                            .flatten()
                    })
                })
        });
        let is_cross_gable = assembly.parent.is_some_and(|parent_id| {
            assembly_read.iter().any(|parent| {
                parent.id == parent_id
                    && parent.children.iter().any(|child| {
                        child.child == assembly.id && child.kind == RoofChildKind::CrossGable
                    })
            })
        });
        let is_timber_child = assembly.parent.is_some()
            && matches!(
                archetype,
                BuildingArchetype::TownHouse
                    | BuildingArchetype::HallHouse
                    | BuildingArchetype::FachwerkCottage
                    | BuildingArchetype::FachwerkMerchantHouse
                    | BuildingArchetype::RenaissanceTownHall
            );
        let mut network_indices = geometry
            .roof_drainage_networks
            .iter()
            .enumerate()
            .filter_map(|(index, network)| (network.owner == assembly.owner).then_some(index))
            .collect::<Vec<_>>();
        if network_indices.is_empty() {
            continue;
        }
        let roof_centre = assembly
            .faces
            .iter()
            .flat_map(|face| &face.polygon)
            .map(|point| Vec2::new(point.x, point.z))
            .sum::<Vec2>()
            / assembly
                .faces
                .iter()
                .map(|face| face.polygon.len())
                .sum::<usize>() as f32;
        network_indices.sort_by(|left, right| {
            let left = geometry.roof_drainage_networks[*left].channel_low;
            let right = geometry.roof_drainage_networks[*right].channel_low;
            (left.z - roof_centre.y)
                .atan2(left.x - roof_centre.x)
                .total_cmp(&(right.z - roof_centre.y).atan2(right.x - roof_centre.x))
        });
        let maximum_stations = match assembly.kind {
            RoofKind::Conical | RoofKind::Pavilion => 4,
            _ => 4,
        };
        let station_count = maximum_stations.min(network_indices.len()).max(1);
        let chunk_size = network_indices.len().div_ceil(station_count);
        for (station_slot, chunk) in network_indices.chunks(chunk_size).enumerate() {
            let mut desired = chunk
                .iter()
                .map(|index| geometry.roof_drainage_networks[*index].channel_low)
                .sum::<Vec3>()
                / chunk.len() as f32;
            if matches!(assembly.kind, RoofKind::Conical | RoofKind::Pavilion) {
                let mean_radius = chunk
                    .iter()
                    .map(|index| {
                        let point = geometry.roof_drainage_networks[*index].channel_low;
                        Vec2::new(point.x, point.z).distance(roof_centre)
                    })
                    .sum::<f32>()
                    / chunk.len() as f32;
                let radial = (Vec2::new(desired.x, desired.z) - roof_centre).normalize_or_zero();
                desired.x = roof_centre.x + radial.x * (mean_radius + 0.14);
                desired.z = roof_centre.y + radial.y * (mean_radius + 0.14);
            } else {
                // Keep ordinary outlets on a real eave endpoint; averaging
                // opposing or stepped eaves manufactures a collector through
                // the roof field.
                let network = &geometry.roof_drainage_networks[chunk[chunk.len() / 2]];
                let endpoint = network.channel_low;
                // Use the receiving eave's exact outward normal. A radial
                // corner vector leaves the drip over the side trimmer on a
                // small dormer, so the later timber pass legitimately blocks
                // it even though the roof-only solver looked clear.
                let outward = geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == network.catchment)
                    .map_or_else(
                        || (Vec2::new(endpoint.x, endpoint.z) - roof_centre).normalize_or_zero(),
                        |catchment| catchment.outward,
                    );
                let outlet_offset = if assembly.parent.is_some() {
                    0.10
                } else {
                    0.14
                };
                desired.x = endpoint.x + outward.x * outlet_offset;
                desired.z = endpoint.z + outward.y * outlet_offset;
            }
            let old_outlets = chunk
                .iter()
                .map(|index| geometry.roof_drainage_networks[*index].outlet_void)
                .collect::<HashSet<_>>();
            let old_spouts = chunk
                .iter()
                .filter_map(|index| geometry.roof_drainage_networks[*index].downspout)
                .collect::<HashSet<_>>();
            let shared_outlet = geometry.roof_drainage_networks[chunk[0]].outlet_void;
            let station_id = ResolvedItemId(
                (0x7_u64 << 60) | (assembly.id.0 << 16) | 0x7000 | station_slot as u64,
            );
            let recipient_surface = ResolvedItemId(
                (0x9_u64 << 60) | (assembly.id.0 << 16) | 0x7000 | station_slot as u64,
            );
            let resolved_solids = &geometry.solids;

            let free_drip = assembly.parent.and_then(|parent_id| {
                let parent = assembly_read.iter().find(|roof| roof.id == parent_id)?;
                // A child eave free-drips vertically onto the parent weather
                // face directly below; it does not run an unframed diagonal
                // collector through the dormer enclosure.
                let desired_plan = Vec2::new(desired.x, desired.z);
                let face_contains_recipient = |face: &RoofFace, target_plan: Vec2| {
                    let outline = face
                        .polygon
                        .iter()
                        .map(|point| Vec2::new(point.x, point.z))
                        .collect::<Vec<_>>();
                    plan_point_in_polygon(target_plan, &outline)
                        && !face.cutouts.iter().any(|cutout| {
                            let cutout = cutout
                                .iter()
                                .map(|point| Vec2::new(point.x, point.z))
                                .collect::<Vec<_>>();
                            plan_point_in_polygon(target_plan, &cutout)
                        })
                };
                let ordinary_offsets = [
                    Vec2::ZERO,
                    Vec2::X * 0.25,
                    -Vec2::X * 0.25,
                    Vec2::Y * 0.25,
                    -Vec2::Y * 0.25,
                    Vec2::X * 0.50,
                    -Vec2::X * 0.50,
                    Vec2::Y * 0.50,
                    -Vec2::Y * 0.50,
                    Vec2::X * 0.75,
                    -Vec2::X * 0.75,
                    Vec2::Y * 0.75,
                    -Vec2::Y * 0.75,
                ];
                let tower_offsets = [
                    Vec2::ZERO,
                    Vec2::X * 0.50,
                    -Vec2::X * 0.50,
                    Vec2::Y * 0.50,
                    -Vec2::Y * 0.50,
                    Vec2::X * 0.75,
                    -Vec2::X * 0.75,
                    Vec2::Y * 0.75,
                    -Vec2::Y * 0.75,
                ];
                let offsets: &[Vec2] = if assembly.kind == RoofKind::Pavilion {
                    &tower_offsets
                } else {
                    &ordinary_offsets
                };
                let source_low = chunk
                    .iter()
                    .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                    .fold(f32::INFINITY, f32::min);
                let selected = offsets
                    .iter()
                    .copied()
                    .flat_map(|offset| {
                        let target = desired_plan + offset;
                        parent
                            .faces
                            .iter()
                            .filter(move |face| face_contains_recipient(face, target))
                            .filter(move |face| {
                                let recipient_y = roof_plane_height(face.plane, target);
                                resolved_solids.iter().all(|solid| {
                                    if solid.role == SolidRole::RoofFace
                                        || (solid.owner == assembly.owner
                                            && matches!(
                                                solid.role,
                                                SolidRole::RoofGutter
                                                    | SolidRole::RoofEdgeTreatment
                                            ))
                                    {
                                        return true;
                                    }
                                    let cosine = solid.yaw_radians.cos().abs();
                                    let sine = solid.yaw_radians.sin().abs();
                                    let half = Vec3::new(
                                        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                                        solid.size.y * 0.5,
                                        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                                    );
                                    let bounds = (solid.centre - half, solid.centre + half);
                                    if solid.role == SolidRole::RoofFlashing
                                        && solid.owner == parent.owner
                                        && bounds.1.y <= recipient_y + 0.86
                                    {
                                        return true;
                                    }
                                    let plan_hit = match solid.shape {
                                        crate::ResolvedSolidShape::RoundTowerShell {
                                            outer_radius_metres,
                                            ..
                                        } => {
                                            target
                                                .distance(Vec2::new(solid.centre.x, solid.centre.z))
                                                <= outer_radius_metres + 0.08
                                        }
                                        _ => {
                                            target.x >= bounds.0.x - 0.08
                                                && target.x <= bounds.1.x + 0.08
                                                && target.y >= bounds.0.z - 0.08
                                                && target.y <= bounds.1.z + 0.08
                                        }
                                    };
                                    let vertical_hit = source_low - 0.15 > bounds.0.y
                                        && recipient_y + 0.14 < bounds.1.y;
                                    !(plan_hit && vertical_hit)
                                })
                            })
                            .map(move |face| (target, face))
                    })
                    .min_by(|left, right| {
                        left.0
                            .distance(desired_plan)
                            .total_cmp(&right.0.distance(desired_plan))
                    });
                if let Some((target_plan, face)) = selected {
                    let recipient_y = roof_plane_height(face.plane, target_plan);
                    if recipient_y + 0.08 < source_low {
                        return Some((parent, face, target_plan, recipient_y));
                    }
                }

                // The obstacle-aware first pass may reject all candidates at
                // a non-convex dormer notch. Select a geometrically exact
                // downhill parent-face point as a fallback; the independent
                // drainage audit then performs the full fall-cone collision
                // sweep and rejects it if anything actually blocks the drop.
                [0.25_f32, 0.50, 0.75, 1.00, 1.25, 1.50]
                    .into_iter()
                    .flat_map(|distance| {
                        [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                            .into_iter()
                            .map(move |axis| desired_plan + axis * distance)
                    })
                    .find_map(|target_plan| {
                        parent.faces.iter().find_map(|face| {
                            if !face_contains_recipient(face, target_plan) {
                                return None;
                            }
                            let recipient_y = roof_plane_height(face.plane, target_plan);
                            (recipient_y + 0.08 < source_low).then_some((
                                parent,
                                face,
                                target_plan,
                                recipient_y,
                            ))
                        })
                    })
            });

            let opening_voids = &geometry.voids;
            let drainage_networks = &geometry.roof_drainage_networks;
            let child_host_candidate = is_timber_child
                .then(|| {
                    let desired_plan = Vec2::new(desired.x, desired.z);
                    let seed = cross_facade_wall
                        .and_then(|facade_wall_id| {
                            walls.iter().find(|wall| wall.id == facade_wall_id)
                        })
                        .or_else(|| {
                            walls
                                .iter()
                                .filter(|wall| {
                                    wall.base_elevation_metres <= 0.30
                                        && wall.frame.outside_room.is_none()
                                        && wall.radial_frame.is_none()
                                })
                                .min_by(|left, right| {
                                    left.frame
                                        .origin
                                        .distance(roof_centre)
                                        .total_cmp(&right.frame.origin.distance(roof_centre))
                                })
                        })?;
                    let away = (desired_plan - roof_centre)
                        .dot(seed.frame.tangent)
                        .signum();
                    let clear_target = desired_plan + seed.frame.tangent * away * 1.20;
                    walls
                        .iter()
                        .filter(|wall| {
                            wall.base_elevation_metres <= 0.30
                                && wall.frame.outward.dot(seed.frame.outward) >= 0.99
                                && (wall.frame.origin - seed.frame.origin)
                                    .dot(seed.frame.outward)
                                    .abs()
                                    <= 0.05
                        })
                        .flat_map(|wall| {
                            let preferred =
                                (clear_target - wall.frame.origin).dot(wall.frame.tangent);
                            [-0.60_f32, -0.30, 0.0, 0.30, 0.60]
                                .into_iter()
                                .map(move |adjustment| {
                                    let along = (preferred + adjustment).clamp(
                                        -wall.length_metres * 0.5 + 0.12,
                                        wall.length_metres * 0.5 - 0.12,
                                    );
                                    let face = wall.frame.origin
                                        + wall.frame.tangent * along
                                        + wall.frame.outward * wall.thickness_metres * 0.5;
                                    (wall, face, face.distance(clear_target))
                                })
                        })
                        .filter(|(_, face, _)| {
                            opening_voids
                                .iter()
                                .filter(|void| {
                                    matches!(
                                        void.role,
                                        VoidRole::WallOpening | VoidRole::AccessPortal
                                    )
                                })
                                .all(|void| {
                                    face.x < void.bounds.min.x - 0.10
                                        || face.x > void.bounds.max.x + 0.10
                                        || face.y < void.bounds.min.z - 0.10
                                        || face.y > void.bounds.max.z + 0.10
                                })
                        })
                        .min_by(|left, right| left.2.total_cmp(&right.2))
                })
                .flatten();
            let host_candidate = if free_drip.is_none() {
                child_host_candidate.or_else(|| {
                    walls
                        .iter()
                        .filter(|wall| {
                            let tower_face =
                                matches!(wall.source, crate::WallSourceId::SquareTowerFace { .. });
                            wall.replaced_by_owner.is_none()
                                && (wall.frame.outside_room.is_none()
                                    || (is_cross_gable
                                        && matches!(
                                            wall.source,
                                            crate::WallSourceId::RoofChildFront { .. }
                                        )))
                                && wall.radial_frame.is_none()
                                && (!matches!(
                                    wall.source,
                                    crate::WallSourceId::RoofChildFront { .. }
                                ) || is_cross_gable)
                                && (tower_face
                                    || wall.base_elevation_metres <= 0.30
                                    || (is_cross_gable
                                        && matches!(
                                            wall.source,
                                            crate::WallSourceId::RoofChildFront { .. }
                                        )))
                        })
                        .flat_map(|wall| {
                            let desired_plan = Vec2::new(desired.x, desired.z);
                            let preferred =
                                (desired_plan - wall.frame.origin).dot(wall.frame.tangent);
                            [-1.20_f32, -0.90, -0.60, -0.30, 0.0, 0.30, 0.60, 0.90, 1.20]
                                .into_iter()
                                .filter_map(move |adjustment| {
                                    let face = if let Some(radial) = wall.radial_frame {
                                        let radius = wall.length_metres / std::f32::consts::TAU;
                                        let desired_axis = (desired_plan - radial.centre)
                                            .normalize_or(radial.reference_outward);
                                        let angle = adjustment / radius.max(0.1);
                                        let cosine = angle.cos();
                                        let sine = angle.sin();
                                        let axis = Vec2::new(
                                            desired_axis.x * cosine - desired_axis.y * sine,
                                            desired_axis.x * sine + desired_axis.y * cosine,
                                        );
                                        radial.centre
                                            + axis * (radius + wall.thickness_metres * 0.5)
                                    } else {
                                        let along = (preferred + adjustment).clamp(
                                            -wall.length_metres * 0.5 + 0.12,
                                            wall.length_metres * 0.5 - 0.12,
                                        );
                                        wall.frame.origin
                                            + wall.frame.tangent * along
                                            + wall.frame.outward * wall.thickness_metres * 0.5
                                    };
                                    // A downspout spans the complete stacked facade, not
                                    // merely this ground-storey wall record. Keep its plan
                                    // station clear of openings on every collinear storey.
                                    let opening_clear = opening_voids
                                        .iter()
                                        .filter(|void| {
                                            matches!(
                                                void.role,
                                                VoidRole::WallOpening | VoidRole::AccessPortal
                                            )
                                        })
                                        .all(|void| {
                                            face.x < void.bounds.min.x - 0.18
                                                || face.x > void.bounds.max.x + 0.18
                                                || face.y < void.bounds.min.z - 0.18
                                                || face.y > void.bounds.max.z + 0.18
                                        });
                                    let collector_start = Vec2::new(
                                        drainage_networks[chunk[0]].channel_low.x,
                                        drainage_networks[chunk[0]].channel_low.z,
                                    );
                                    let collector_clear =
                                        (1..10).all(|sample| {
                                            let point =
                                                collector_start.lerp(face, sample as f32 / 10.0);
                                            resolved_solids.iter().all(|solid| {
                                                if ((!is_timber_child
                                                    && solid.role != SolidRole::WallHost)
                                                    || (is_timber_child
                                                        && !matches!(
                                                            solid.role,
                                                            SolidRole::WallHost
                                                                | SolidRole::OpeningJamb
                                                                | SolidRole::OpeningSill
                                                                | SolidRole::OpeningHead
                                                                | SolidRole::OpeningSpandrel
                                                                | SolidRole::OpeningClosure
                                                        )))
                                                    || wall.host_solids.contains(&solid.id)
                                                    || (is_timber_child
                                                        && solid.centre.y + solid.size.y * 0.5
                                                            < drainage_networks[chunk[0]]
                                                                .channel_low
                                                                .y
                                                                - 0.15)
                                                {
                                                    return true;
                                                }
                                                match solid.shape {
                                            crate::ResolvedSolidShape::RoundTowerShell {
                                                outer_radius_metres,
                                                ..
                                            } => {
                                                point.distance(Vec2::new(
                                                    solid.centre.x,
                                                    solid.centre.z,
                                                )) > outer_radius_metres + 0.06
                                            }
                                            _ => {
                                                let half = solid.size * 0.5;
                                                let margin = if solid.role == SolidRole::WallHost {
                                                    0.06
                                                } else {
                                                    0.30
                                                };
                                                point.x < solid.centre.x - half.x - margin
                                                    || point.x > solid.centre.x + half.x + margin
                                                    || point.y < solid.centre.z - half.z - margin
                                                    || point.y > solid.centre.z + half.z + margin
                                            }
                                        }
                                            })
                                        });
                                    (opening_clear && collector_clear).then_some((
                                        wall,
                                        face,
                                        face.distance(desired_plan),
                                    ))
                                })
                        })
                        .min_by(|left, right| left.2.total_cmp(&right.2))
                })
            } else {
                None
            };

            let (disposition, host_wall, facade_contact, recipient, outlet, discharge, spout_id) =
                if let Some((parent, face, target_plan, recipient_y)) = free_drip {
                    let outlet = Vec3::new(
                        target_plan.x,
                        chunk
                            .iter()
                            .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                            .fold(f32::INFINITY, f32::min)
                            - 0.07,
                        target_plan.y,
                    );
                    let discharge = Vec3::new(target_plan.x, recipient_y + 0.06, target_plan.y);
                    (
                        RoofDrainageDisposition::FreeDripToParentRoof,
                        None,
                        None,
                        RoofDrainageRecipient::ParentRoofFace {
                            roof: parent.id,
                            face: face.id,
                        },
                        outlet,
                        discharge,
                        None,
                    )
                } else if let Some((host, face, _distance)) = host_candidate.filter(|candidate| {
                    candidate.2
                        <= if is_cross_gable || is_timber_child {
                            3.20
                        } else {
                            1.20
                        }
                }) {
                    let host_outward = host.radial_frame.map_or(host.frame.outward, |radial| {
                        (face - radial.centre).normalize_or(radial.reference_outward)
                    });
                    let projected_facade_clearance = match archetype {
                        BuildingArchetype::TownHouse => 0.22,
                        BuildingArchetype::FachwerkMerchantHouse => 0.28,
                        BuildingArchetype::RenaissanceTownHall => 0.24,
                        _ => 0.0,
                    };
                    let pipe_plan =
                        face + host_outward * (0.055 + projected_facade_clearance + 0.10);
                    let outlet_y = chunk
                        .iter()
                        .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                        .fold(f32::INFINITY, f32::min)
                        - 0.08;
                    let outlet = Vec3::new(pipe_plan.x, outlet_y, pipe_plan.y);
                    let discharge = Vec3::new(pipe_plan.x, 0.24, pipe_plan.y);
                    (
                        RoofDrainageDisposition::BoundDownspout,
                        Some(host.id),
                        Some(Vec3::new(face.x, outlet_y * 0.5, face.y)),
                        RoofDrainageRecipient::GroundSplashApron,
                        outlet,
                        discharge,
                        old_spouts.iter().min().copied(),
                    )
                } else {
                    // Deep overhangs without a facade beneath the eave are an
                    // explicit free-drip condition. Do not manufacture a
                    // detached pipe across open air to the nearest wall.
                    // Move the fall cone a further 200 mm beyond the fascia;
                    // combined with the ordinary outlet offset this freezes a
                    // 340 mm clearance from the eave without displacing child
                    // outlets that must land on a parent weather face.
                    let desired_plan = Vec2::new(desired.x, desired.z);
                    let source_network = &geometry.roof_drainage_networks[chunk[0]];
                    let mut source_channel_ids = vec![source_network.channel_floor];
                    source_channel_ids.extend(source_network.channel_lips);
                    source_channel_ids.extend(source_network.collector_solids.iter().copied());
                    let downhill = assembly
                        .faces
                        .iter()
                        .find(|face| face.id == source_network.face)
                        .map(|face| {
                            Vec2::new(
                                face.plane.normal.x / face.plane.normal.y,
                                face.plane.normal.z / face.plane.normal.y,
                            )
                            .normalize_or_zero()
                        })
                        .unwrap_or_else(|| (desired_plan - roof_centre).normalize_or_zero());
                    let outlet_y = chunk
                        .iter()
                        .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                        .fold(f32::INFINITY, f32::min)
                        - 0.07;
                    let channel_low_plan =
                        Vec2::new(source_network.channel_low.x, source_network.channel_low.z);
                    let toward_channel_high =
                        (Vec2::new(source_network.channel_high.x, source_network.channel_high.z)
                            - channel_low_plan)
                            .normalize_or_zero();
                    let defensive_roof = matches!(
                        archetype,
                        BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
                    );
                    let along_offsets: &[f32] = if defensive_roof {
                        &[
                            0.0, 0.60, -0.60, 1.20, -1.20, 1.80, -1.80, 2.40, -2.40, 3.00, -3.00,
                            3.60, -3.60, 4.20, -4.20,
                        ]
                    } else if assembly_read.len() == 1 {
                        &[0.0, 0.30, 0.60, 0.90, 1.20, -0.30, -0.60, -0.90, -1.20]
                    } else {
                        // Try both gutter directions before accepting a free
                        // fall.  Attached pavilion eaves commonly overlap the
                        // choir floor in one direction but can discharge at
                        // the exposed corner in the other.  The ordered
                        // search preserves existing stations whenever their
                        // shorter positive collector remains valid.
                        &[
                            0.0, 0.30, 0.60, -0.30, -0.60, 0.90, -0.90, 1.20, -1.20, 1.50, -1.50,
                            1.80, -1.80,
                        ]
                    };
                    let outward_offsets: &[f32] = if defensive_roof {
                        // A corner catchment first follows its owned eave away
                        // from the return wall.  Only then should it step
                        // outward.  Omitting the zero-offset option forced a
                        // diagonal shortcut through the courtyard corner.
                        &[0.0, 0.20, 0.35, 0.50, 0.65, 0.80, 1.00, 1.20]
                    } else if assembly_read.len() == 1 {
                        &[0.20, 0.35, 0.50, 0.65, 0.80]
                    } else {
                        &[0.20, 0.40, 0.60, 0.80, 1.00, 1.20]
                    };
                    let candidate_origin = if defensive_roof {
                        // A defensive eave may already project clear of its
                        // supporting return. Prefer a direct drip from the
                        // physical low end before inventing a diagonal
                        // collector across the corner wall.
                        channel_low_plan - toward_channel_high * 0.10
                    } else {
                        desired_plan
                    };
                    let mut fall_candidates = along_offsets.iter().copied().flat_map(|along| {
                        outward_offsets.iter().copied().map(move |outward| {
                            candidate_origin + toward_channel_high * along + downhill * outward
                        })
                    });
                    let fall_plan = fall_candidates
                        .find(|candidate| {
                            let clears_solids = geometry.solids.iter().all(|solid| {
                                if solid.owner == assembly.owner
                                    && (solid.role == SolidRole::RoofEdgeTreatment
                                        || (solid.role == SolidRole::RoofGutter
                                            && source_channel_ids.contains(&solid.id)))
                                {
                                    return true;
                                }
                                if solid.role == SolidRole::RoofFace {
                                    return true;
                                }
                                let cosine = solid.yaw_radians.cos().abs();
                                let sine = solid.yaw_radians.sin().abs();
                                let half = Vec3::new(
                                    (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                                    solid.size.y * 0.5,
                                    (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                                );
                                let min = solid.centre - half;
                                let max = solid.centre + half;
                                let plan_hit = match solid.shape {
                                    crate::ResolvedSolidShape::RoundTowerShell {
                                        outer_radius_metres,
                                        ..
                                    } => {
                                        candidate
                                            .distance(Vec2::new(solid.centre.x, solid.centre.z))
                                            <= outer_radius_metres + 0.08
                                    }
                                    _ => {
                                        candidate.x >= min.x - 0.08
                                            && candidate.x <= max.x + 0.08
                                            && candidate.y >= min.z - 0.08
                                            && candidate.y <= max.z + 0.08
                                    }
                                };
                                let height_hit = outlet_y - 0.08 > min.y && 0.16 < max.y;
                                let outlet_cut_hits_other_gutter = solid.role
                                    == SolidRole::RoofGutter
                                    && candidate.x >= min.x - 0.05
                                    && candidate.x <= max.x + 0.05
                                    && candidate.y >= min.z - 0.05
                                    && candidate.y <= max.z + 0.05
                                    && outlet_y + 0.05 >= min.y
                                    && outlet_y - 0.05 <= max.y;
                                !(plan_hit && height_hit) && !outlet_cut_hits_other_gutter
                            });
                            let collector_start_y = source_network.channel_low.y - 0.025;
                            let source_channel_delta = Vec2::new(
                                source_network.channel_high.x - source_network.channel_low.x,
                                source_network.channel_high.z - source_network.channel_low.z,
                            );
                            let source_channel_t = ((*candidate - channel_low_plan)
                                .dot(source_channel_delta)
                                / source_channel_delta.length_squared().max(0.000_001))
                            .clamp(0.0, 1.0);
                            let on_source_channel = defensive_roof
                                && candidate.distance(
                                    channel_low_plan + source_channel_delta * source_channel_t,
                                ) <= 0.08;
                            let direct_drip = candidate.distance(channel_low_plan) <= 0.11;
                            let collector_clears_solids = direct_drip
                                || on_source_channel
                                || (1..10).all(|sample| {
                                    let fraction = sample as f32 / 10.0;
                                    let point = channel_low_plan.lerp(*candidate, fraction);
                                    let height = collector_start_y
                                        + (outlet_y - collector_start_y) * fraction;
                                    geometry.solids.iter().all(|solid| {
                                        if solid.owner == assembly.owner
                                            && matches!(
                                                solid.role,
                                                SolidRole::RoofGutter
                                                    | SolidRole::RoofEdgeTreatment
                                            )
                                        {
                                            return true;
                                        }
                                        if !matches!(
                                            solid.role,
                                            SolidRole::WallHost
                                                | SolidRole::OpeningJamb
                                                | SolidRole::OpeningSill
                                                | SolidRole::OpeningHead
                                                | SolidRole::OpeningSpandrel
                                                | SolidRole::RoofFlashing
                                        ) {
                                            return true;
                                        }
                                        let cosine = solid.yaw_radians.cos().abs();
                                        let sine = solid.yaw_radians.sin().abs();
                                        let half = Vec3::new(
                                            (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                                            solid.size.y * 0.5,
                                            (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                                        );
                                        point.x < solid.centre.x - half.x - 0.06
                                            || point.x > solid.centre.x + half.x + 0.06
                                            || point.y < solid.centre.z - half.z - 0.06
                                            || point.y > solid.centre.z + half.z + 0.06
                                            || height < solid.centre.y - half.y - 0.04
                                            || height > solid.centre.y + half.y + 0.04
                                    })
                                });
                            let clears_stairs = stairs.iter().all(|stair| match *stair {
                                Stair::Straight {
                                    start,
                                    direction,
                                    width_metres,
                                    tread_count,
                                    ..
                                } => {
                                    let axis = match direction {
                                        Direction::North => Vec2::Y,
                                        Direction::South => -Vec2::Y,
                                        Direction::East => Vec2::X,
                                        Direction::West => -Vec2::X,
                                    };
                                    let end = start + axis * tread_count as f32 * 0.28;
                                    let delta = end - start;
                                    let t = ((*candidate - start).dot(delta)
                                        / delta.length_squared().max(0.000_001))
                                    .clamp(0.0, 1.0);
                                    candidate.distance(start + delta * t)
                                        > width_metres * 0.5 + 0.30
                                }
                                Stair::Spiral {
                                    centre,
                                    outer_radius_metres,
                                    ..
                                } => candidate.distance(centre) > outer_radius_metres + 0.30,
                            });
                            let clears_portals = opening_voids
                                .iter()
                                .filter(|void| {
                                    matches!(
                                        void.role,
                                        VoidRole::WallOpening | VoidRole::AccessPortal
                                    ) && void.bounds.min.y < 1.08
                                })
                                .all(|void| {
                                    candidate.x < void.bounds.min.x - 0.30
                                        || candidate.x > void.bounds.max.x + 0.30
                                        || candidate.y < void.bounds.min.z - 0.30
                                        || candidate.y > void.bounds.max.z + 0.30
                                });
                            clears_solids
                                && collector_clears_solids
                                && clears_stairs
                                && clears_portals
                        })
                        .unwrap_or(
                            candidate_origin
                                + toward_channel_high * along_offsets[along_offsets.len() - 1]
                                + downhill * outward_offsets[outward_offsets.len() - 1],
                        );
                    let outlet = Vec3::new(fall_plan.x, outlet_y, fall_plan.y);
                    let discharge = Vec3::new(fall_plan.x, 0.08, fall_plan.y);
                    (
                        RoofDrainageDisposition::FreeDripToGround,
                        None,
                        None,
                        RoofDrainageRecipient::GroundSplashApron,
                        outlet,
                        discharge,
                        None,
                    )
                };

            geometry
                .solids
                .retain(|solid| !old_spouts.contains(&solid.id));
            geometry.support_interfaces.retain(|interface| {
                !old_spouts.iter().any(|id| {
                    interface.id == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
                })
            });
            geometry
                .voids
                .retain(|void| !old_outlets.contains(&void.id));
            geometry.voids.push(ResolvedVoid {
                id: shared_outlet,
                owner: assembly.owner,
                bounds: ResolvedBounds {
                    min: outlet - Vec3::splat(0.045),
                    max: outlet + Vec3::splat(0.045),
                },
                role: VoidRole::Drain,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assembly.owner,
            });

            if let (Some(spout), Some(host_id)) = (spout_id, host_wall) {
                let host = walls
                    .iter()
                    .find(|wall| wall.id == host_id)
                    .expect("selected roof drain host");
                let height = (outlet.y - discharge.y - 0.14).max(0.09);
                let centre = Vec3::new(outlet.x, discharge.y + height * 0.5 + 0.07, outlet.z);
                geometry.solids.push(ResolvedSolid {
                    id: spout,
                    owner: assembly.owner,
                    centre,
                    size: Vec3::new(0.09, height, 0.09),
                    yaw_radians: 0.0,
                    crossfall_radians: 0.0,
                    longfall_radians: 0.0,
                    role: SolidRole::RoofGutter,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: vec![host.support_node],
                });
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId((0x9_u64 << 60) | (spout.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                    owner: assembly.owner,
                    node: host.support_node,
                    bounds: ResolvedBounds {
                        min: centre - Vec3::splat(0.04),
                        max: centre + Vec3::splat(0.04),
                    },
                });
            }

            let mut member_networks = Vec::new();
            for (member_slot, index) in chunk.iter().copied().enumerate() {
                let network_id = geometry.roof_drainage_networks[index].id;
                member_networks.push(network_id);
                let start = geometry.roof_drainage_networks[index].channel_low - Vec3::Y * 0.025;
                let raw_delta = outlet - start;
                let plan_direction = Vec2::new(raw_delta.x, raw_delta.z).normalize_or_zero();
                // The collector terminates at the rim of the outlet cut rather
                // than occupying its free volume. The 0.10 m setback is the
                // project gutter-mouth radius plus a small construction joint.
                let collector_end =
                    outlet - Vec3::new(plan_direction.x * 0.10, 0.0, plan_direction.y * 0.10);
                let delta = collector_end - start;
                let plan_length = Vec2::new(delta.x, delta.z).length();
                let channel_low_plan = Vec2::new(
                    geometry.roof_drainage_networks[index].channel_low.x,
                    geometry.roof_drainage_networks[index].channel_low.z,
                );
                let channel_delta = Vec2::new(
                    geometry.roof_drainage_networks[index].channel_high.x
                        - geometry.roof_drainage_networks[index].channel_low.x,
                    geometry.roof_drainage_networks[index].channel_high.z
                        - geometry.roof_drainage_networks[index].channel_low.z,
                );
                let outlet_plan = Vec2::new(outlet.x, outlet.z);
                let channel_t = ((outlet_plan - channel_low_plan).dot(channel_delta)
                    / channel_delta.length_squared().max(0.000_001))
                .clamp(0.0, 1.0);
                let outlet_is_on_channel =
                    matches!(
                        archetype,
                        BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
                    ) && outlet_plan.distance(channel_low_plan + channel_delta * channel_t) <= 0.08;
                let mut collectors = Vec::new();
                if plan_length > 0.10 && !outlet_is_on_channel {
                    let collector = ResolvedItemId(
                        (0x8_u64 << 60)
                            | (assembly.id.0 << 16)
                            | 0x7800
                            | ((station_slot as u64 & 0x7) << 5)
                            | (member_slot as u64 & 0x1F),
                    );
                    let face = assembly
                        .faces
                        .iter()
                        .find(|face| face.id == geometry.roof_drainage_networks[index].face)
                        .expect("drainage face");
                    let compact_child_collector = assembly.parent.is_some()
                        && matches!(assembly.kind, RoofKind::Gable | RoofKind::Shed);
                    geometry.solids.push(ResolvedSolid {
                        id: collector,
                        owner: assembly.owner,
                        centre: (start + collector_end) * 0.5,
                        size: Vec3::new(
                            plan_length,
                            if compact_child_collector {
                                0.018
                            } else {
                                0.035
                            },
                            if compact_child_collector { 0.070 } else { 0.12 },
                        ),
                        yaw_radians: delta.z.atan2(delta.x),
                        crossfall_radians: 0.0,
                        longfall_radians: delta.y.atan2(plan_length),
                        role: SolidRole::RoofGutter,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: face.support_nodes.clone(),
                    });
                    geometry.support_interfaces.push(SupportInterface {
                        id: ResolvedItemId((0x9_u64 << 60) | (collector.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                        owner: assembly.owner,
                        node: face.support_nodes[0],
                        bounds: ResolvedBounds {
                            min: start - Vec3::splat(0.035),
                            max: start + Vec3::splat(0.035),
                        },
                    });
                    collectors.push(collector);
                }
                let network = &mut geometry.roof_drainage_networks[index];
                if let Some(edge) = assemblies
                    .iter_mut()
                    .find(|roof| roof.id == assembly.id)
                    .and_then(|roof| {
                        roof.edges
                            .iter_mut()
                            .find(|edge| edge.id == network.receiving_edge)
                    })
                {
                    edge.drainage_terminal = Some(shared_outlet);
                }
                network.collector_solids = collectors.clone();
                network.outlet_station = station_id;
                network.outlet_void = shared_outlet;
                network.downspout = spout_id;
                network.discharge = discharge;
                if let Some(catchment) = geometry
                    .drainage_catchments
                    .iter_mut()
                    .find(|catchment| catchment.id == network.catchment)
                {
                    catchment.toe_channel_solids.extend(collectors);
                    if let Some(route) = geometry
                        .drainage_routes
                        .iter_mut()
                        .find(|route| route.id == catchment.outlet_route)
                    {
                        route.outlet_void = shared_outlet;
                        route.outlet = outlet;
                    }
                }
            }
            let recipient_bounds = ResolvedBounds {
                min: discharge - Vec3::new(0.30, 0.03, 0.30),
                max: discharge + Vec3::new(0.30, 0.03, 0.30),
            };
            geometry.surfaces.push(ResolvedSurface {
                id: recipient_surface,
                owner: assembly.owner,
                bounds: recipient_bounds,
                role: SurfaceRole::DrainageRecipient,
                shape: crate::ResolvedSurfaceShape::Planar,
            });
            geometry
                .roof_drainage_outlets
                .push(RoofDrainageOutletStation {
                    id: station_id,
                    owner: assembly.owner,
                    disposition,
                    member_networks,
                    host_wall,
                    facade_contact,
                    outlet_void: shared_outlet,
                    downspout: spout_id,
                    recipient,
                    recipient_surface,
                    discharge,
                });
        }
    }
}
