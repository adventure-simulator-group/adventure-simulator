fn resolve_roof_abutment_contours(
    assemblies: &mut [RoofAssembly],
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) {
    for assembly in assemblies {
        for (kind_slot, (edge_kind, abutment_kind)) in [
            (RoofEdgeKind::WallAbutment, RoofAbutmentKind::Wall),
            (RoofEdgeKind::TowerAbutment, RoofAbutmentKind::Tower),
        ]
        .into_iter()
        .enumerate()
        {
            let edge_indices = assembly
                .edges
                .iter()
                .enumerate()
                .filter_map(|(index, edge)| (edge.kind == edge_kind).then_some(index))
                .collect::<Vec<_>>();
            if edge_indices.is_empty() {
                continue;
            }
            let abutment_id =
                ResolvedItemId((0x7_u64 << 60) | (assembly.id.0 << 16) | 0xD000 | kind_slot as u64);
            let mut samples = Vec::new();
            let mut edge_ids = Vec::new();
            for edge_index in edge_indices {
                let edge = &mut assembly.edges[edge_index];
                let old_flashing = edge.flashing;
                let first_edge_sample = samples.len();
                let first_edge_bond = geometry.junction_bonds.len();
                let delta = edge.end - edge.start;
                let length = delta.length().max(0.01);
                let station_count = (length / 0.22).ceil().max(1.0) as usize;
                let horizontal = Vec2::new(delta.x, delta.z).normalize_or_zero();
                for station in 0..=station_count {
                    let t = station as f32 / station_count as f32;
                    let point = edge.start.lerp(edge.end, t);
                    let plan_point = Vec2::new(point.x, point.z);
                    let host = walls
                        .iter()
                        .filter(|wall| {
                            if abutment_kind == RoofAbutmentKind::Tower {
                                matches!(wall.source, crate::WallSourceId::SquareTowerFace { .. })
                            } else {
                                !matches!(wall.source, crate::WallSourceId::RoofChildFront { .. })
                            }
                        })
                        .filter_map(|wall| {
                            let offset = plan_point - wall.frame.origin;
                            let signed_normal = offset.dot(wall.frame.outward);
                            // A weatherable roof abutment lies on the exterior
                            // masonry face, never the wall centreline. Clipped
                            // fragments on the interior side are opening-cut
                            // boundaries, not valid contact contours.
                            let normal_distance =
                                (signed_normal - wall.thickness_metres * 0.5).abs();
                            let along = offset.dot(wall.frame.tangent).abs();
                            let corner_return = if abutment_kind == RoofAbutmentKind::Tower {
                                wall.thickness_metres * 0.5
                            } else {
                                0.0
                            };
                            (normal_distance <= wall.thickness_metres * 0.5 + 0.18
                                && along <= wall.length_metres * 0.5 + corner_return + 0.18
                                && point.y >= wall.base_elevation_metres - 0.08
                                && point.y
                                    <= wall.base_elevation_metres + wall.height_metres + 0.18)
                                .then_some((wall, normal_distance))
                        })
                        .min_by(|(left_wall, left), (right_wall, right)| {
                            let priority = |wall: &crate::WallAssembly| {
                                if abutment_kind == RoofAbutmentKind::Wall
                                    && matches!(
                                        wall.source,
                                        crate::WallSourceId::ChurchArcade { .. }
                                    )
                                {
                                    0_u8
                                } else {
                                    1_u8
                                }
                            };
                            priority(left_wall)
                                .cmp(&priority(right_wall))
                                .then_with(|| left.total_cmp(right))
                        });
                    let Some((host, _)) = host else { continue };
                    if samples.len() == first_edge_sample
                        && let Some(old) = old_flashing
                    {
                        geometry.solids.retain(|solid| solid.id != old);
                    }
                    // Reserve independent bit ranges for edge and station.
                    // Long clerestory contacts exceed 64 samples, so the old
                    // `edge << 8 | station * 4` encoding aliased IDs once the
                    // station carried into the edge bits.
                    let serial = ((edge_index as u64 & 0x7) << 10) | ((station as u64 & 0xFF) << 2);
                    let base = (0x8_u64 << 60) | (assembly.id.0 << 16) | 0xA000 | serial;
                    let apron = if station == 0 {
                        old_flashing.unwrap_or(ResolvedItemId(base))
                    } else {
                        ResolvedItemId(base)
                    };
                    let upstand = ResolvedItemId(base | 1);
                    let counter = ResolvedItemId(base | 2);
                    let outward = Vec3::new(host.frame.outward.x, 0.0, host.frame.outward.y);
                    let tangent_yaw = horizontal.y.atan2(horizontal.x);
                    let span = (length / station_count as f32 + 0.035).max(0.12);
                    for (id, centre, size, crossfall) in [
                        (
                            apron,
                            // The apron lies on the roof side of the masonry
                            // contour and laps the host by roughly 70 mm.  It
                            // must not be centred inside the tower shell.
                            point + outward * 0.10 + Vec3::Y * 0.022,
                            Vec3::new(span, 0.045, 0.34),
                            -0.10,
                        ),
                        (
                            upstand,
                            point + outward * 0.012 + Vec3::Y * 0.18,
                            Vec3::new(span, 0.36, 0.055),
                            0.0,
                        ),
                        (
                            counter,
                            point + outward * 0.018 + Vec3::Y * 0.315,
                            Vec3::new(span, 0.12, 0.075),
                            0.0,
                        ),
                    ] {
                        geometry.solids.push(ResolvedSolid {
                            id,
                            owner: assembly.owner,
                            centre,
                            size,
                            yaw_radians: tangent_yaw,
                            crossfall_radians: crossfall,
                            longfall_radians: 0.0,
                            role: SolidRole::RoofFlashing,
                            shape: crate::ResolvedSolidShape::Cuboid,
                            supported_by: vec![host.support_node],
                        });
                        geometry.support_interfaces.push(SupportInterface {
                            id: ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                            owner: assembly.owner,
                            node: host.support_node,
                            bounds: ResolvedBounds {
                                min: centre - size * 0.18,
                                max: centre + size * 0.18,
                            },
                        });
                    }
                    samples.push(RoofAbutmentSample {
                        point,
                        host_wall: host.id,
                        apron_solid: apron,
                        upstand_solid: upstand,
                        counterflashing_solid: counter,
                    });
                    // At a tower corner one weathering strip can bear on both
                    // adjoining wall-face assemblies.  Declare every measured
                    // positive interface instead of assigning the whole strip
                    // to whichever face happened to win the nearest-host query.
                    let weather_ids = [apron, upstand, counter];
                    let rotated_half_extents = |solid: &ResolvedSolid| {
                        let cosine = solid.yaw_radians.cos().abs();
                        let sine = solid.yaw_radians.sin().abs();
                        Vec3::new(
                            (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                            solid.size.y * 0.5,
                            (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                        )
                    };
                    let bonded_hosts = walls
                        .iter()
                        .filter(|candidate| {
                            // Jambs, heads, and spandrels remain pieces of the
                            // authoritative wall owner even though they are not
                            // included in `host_solids`.  Bind weathering to
                            // every resolved piece it physically contacts.
                            geometry
                                .solids
                                .iter()
                                .filter(|solid| solid.owner == candidate.owner)
                                .any(|host_solid| {
                                    let host_half = rotated_half_extents(host_solid);
                                    weather_ids.iter().any(|weather_id| {
                                        let weather = geometry
                                            .solids
                                            .iter()
                                            .find(|solid| solid.id == *weather_id)
                                            .expect("new roof weathering solid must resolve");
                                        let weather_half = rotated_half_extents(weather);
                                        let overlap_min = (host_solid.centre - host_half)
                                            .max(weather.centre - weather_half);
                                        let overlap_max = (host_solid.centre + host_half)
                                            .min(weather.centre + weather_half);
                                        (overlap_max - overlap_min).min_element() > 0.025
                                    })
                                })
                        })
                        .map(|candidate| candidate.owner)
                        .collect::<BTreeSet<_>>();
                    for bonded_owner in bonded_hosts {
                        geometry.junction_bonds.push(JunctionBond {
                            id: ResolvedItemId(
                                (0x6_u64 << 60)
                                    | (assembly.id.0 << 32)
                                    | ((edge_index as u64) << 24)
                                    | ((station as u64) << 12)
                                    | (u64::from(bonded_owner.0) & 0xFFF),
                            ),
                            owners: [assembly.owner, bonded_owner],
                            bounds: ResolvedBounds {
                                min: point - Vec3::new(0.40, 0.25, 0.40),
                                max: point + Vec3::new(0.40, 0.40, 0.40),
                            },
                            minimum_interface_area_square_metres: 0.0005,
                            maximum_penetration_metres: 0.50,
                        });
                    }
                }
                if samples.len() - first_edge_sample == station_count + 1 {
                    edge_ids.push(edge.id);
                    edge.flashing = samples.last().map(|sample| sample.apron_solid);
                } else {
                    // A parent-face subdivision edge inside the tower footprint
                    // is part of the opening cut, not part of the masonry contact
                    // contour.  It owns neither an upstand nor counterflashing.
                    if let Some(old) = old_flashing {
                        geometry.solids.retain(|solid| solid.id != old);
                    }
                    let rejected = samples
                        .drain(first_edge_sample..)
                        .flat_map(|sample| {
                            [
                                sample.apron_solid,
                                sample.upstand_solid,
                                sample.counterflashing_solid,
                            ]
                        })
                        .collect::<HashSet<_>>();
                    geometry
                        .solids
                        .retain(|solid| !rejected.contains(&solid.id));
                    geometry.support_interfaces.retain(|interface| {
                        !rejected.iter().any(|id| {
                            interface.id
                                == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
                        })
                    });
                    geometry.junction_bonds.truncate(first_edge_bond);
                    edge.kind = RoofEdgeKind::OpeningCut;
                    edge.flashing = None;
                }
            }
            if samples.is_empty() {
                continue;
            }
            let lower_sample = samples
                .iter()
                .min_by(|left, right| left.point.y.total_cmp(&right.point.y))
                .expect("non-empty abutment samples");
            let lower = lower_sample.point;
            let lower_outward = walls
                .iter()
                .find(|wall| wall.id == lower_sample.host_wall)
                .map(|wall| Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y))
                .unwrap_or(Vec3::Z);
            let outlet_point = lower + lower_outward * 0.30 - Vec3::Y * 0.08;
            let outlet =
                ResolvedItemId((0xE_u64 << 60) | (assembly.id.0 << 16) | 0xD000 | kind_slot as u64);
            let route =
                ResolvedItemId((0xD_u64 << 60) | (assembly.id.0 << 16) | 0xD000 | kind_slot as u64);
            geometry.voids.push(ResolvedVoid {
                id: outlet,
                owner: assembly.owner,
                bounds: ResolvedBounds {
                    min: outlet_point - Vec3::splat(0.055),
                    max: outlet_point + Vec3::splat(0.055),
                },
                role: VoidRole::Drain,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assembly.owner,
            });
            geometry.drainage_routes.push(DrainageRoute {
                id: route,
                owner: assembly.owner,
                outlet_void: outlet,
                inlet: samples
                    .iter()
                    .max_by(|left, right| left.point.y.total_cmp(&right.point.y))
                    .expect("non-empty abutment samples")
                    .point
                    + Vec3::Y * 0.03,
                outlet: outlet_point,
            });
            assembly.abutments.push(RoofAbutmentAssembly {
                id: abutment_id,
                kind: abutment_kind,
                edge_ids: edge_ids.clone(),
                samples,
                lower_outlet: outlet,
                drainage_route: route,
            });
            for child in &mut assembly.children {
                if child.kind == RoofChildKind::Tower {
                    child.valley_edges.retain(|id| edge_ids.contains(id));
                    child.flashing_ids = child
                        .valley_edges
                        .iter()
                        .filter_map(|id| {
                            assembly
                                .edges
                                .iter()
                                .find(|edge| edge.id == *id)
                                .and_then(|edge| edge.flashing)
                        })
                        .collect();
                }
            }
        }
    }
}
