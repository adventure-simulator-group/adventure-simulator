fn resolve_projected_defenses(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    battlements: &[BattlementRun],
    bartizans: &[Bartizan],
    geometry: &mut ResolvedGeometry,
) -> Vec<ProjectedDefenseAssembly> {
    let mut assemblies = Vec::new();
    for (source_index, run) in battlements.iter().copied().enumerate() {
        let (kind, material, phase, deployment, tactical_target, roofed) = match run.kind {
            BattlementKind::Machicolated => (
                ProjectedDefenseKind::Machicolation,
                ProjectedDefenseMaterial::Masonry,
                ProjectedDefensePhase::PermanentMainWork,
                ProjectedDefenseDeployment::Permanent,
                ProjectedDefenseTarget::GateApproach,
                false,
            ),
            BattlementKind::Breteche => (
                ProjectedDefenseKind::Breteche,
                ProjectedDefenseMaterial::Masonry,
                ProjectedDefensePhase::PermanentMainWork,
                ProjectedDefenseDeployment::Permanent,
                ProjectedDefenseTarget::ThreatenedWallFoot,
                true,
            ),
            BattlementKind::OpenHoarding => (
                ProjectedDefenseKind::Hoarding,
                ProjectedDefenseMaterial::Timber,
                ProjectedDefensePhase::TemporaryCampaignWork,
                ProjectedDefenseDeployment::SocketsOnly,
                ProjectedDefenseTarget::CampaignSiegeFront,
                false,
            ),
            BattlementKind::RoofedHoarding => (
                ProjectedDefenseKind::Hoarding,
                ProjectedDefenseMaterial::Timber,
                ProjectedDefensePhase::TemporaryCampaignWork,
                ProjectedDefenseDeployment::Deployed,
                ProjectedDefenseTarget::CampaignSiegeFront,
                true,
            ),
            _ => continue,
        };
        let owner = GeometryOwnerId(1_000 + source_index as u32);
        let tangent = (run.end - run.start).normalize_or_zero();
        let outward = direction_vector(run.outward);
        let length = run.start.distance(run.end);
        let yaw = -tangent.y.atan2(tangent.x);
        let socket_count = (material == ProjectedDefenseMaterial::Timber)
            .then(|| (length / 1.1).ceil().max(2.0) as usize);
        let host = resolve_linear_defense_host(
            geometry,
            storeys,
            source_index,
            run,
            socket_count,
            deployment != ProjectedDefenseDeployment::SocketsOnly,
        );
        let wall_node = host.bearing;
        let bond_id = ResolvedItemId((6_u64 << 60) | source_index as u64);
        let midpoint = (run.start + run.end) * 0.5;
        geometry.junction_bonds.push(JunctionBond {
            id: bond_id,
            owners: [host.owner, owner],
            bounds: ResolvedBounds {
                min: Vec3::new(
                    midpoint.x - tangent.x.abs() * (length * 0.5 + 0.12) - outward.x.abs() * 0.65,
                    run.base_height_metres - 0.65,
                    midpoint.y - tangent.y.abs() * (length * 0.5 + 0.12) - outward.y.abs() * 0.65,
                ),
                max: Vec3::new(
                    midpoint.x + tangent.x.abs() * (length * 0.5 + 0.12) + outward.x.abs() * 0.65,
                    run.base_height_metres + 2.6,
                    midpoint.y + tangent.y.abs() * (length * 0.5 + 0.12) + outward.y.abs() * 0.65,
                ),
            },
            minimum_interface_area_square_metres: 0.08,
            maximum_penetration_metres: 0.18,
        });
        if deployment == ProjectedDefenseDeployment::SocketsOnly {
            assemblies.push(ProjectedDefenseAssembly {
                owner,
                host_owner: host.owner,
                host_wall_solids: host.walls,
                host_buttress_solids: host.buttresses,
                host_source_walls: host.sources,
                host_top_elevation_metres: host.top_elevation_metres,
                host_topology: host.topology,
                host_walk_solid: host.walk,
                host_portal_void: None,
                host_bond: None,
                beam_socket_voids: host.sockets,
                socket_joists: Vec::new(),
                kind,
                material,
                phase,
                deployment,
                tactical_target,
                path: ProjectedDefensePath::Linear {
                    start: run.start,
                    end: run.end,
                    outward: run.outward,
                },
                floor_elevation_metres: run.base_height_metres,
                clear_width_metres: 0.0,
                clear_height_metres: 0.0,
                projection_metres: 0.0,
                breastwork_height_metres: 0.0,
                roofed,
                floor_solids: Vec::new(),
                throat_voids: Vec::new(),
                access_portal: None,
                access_landing: None,
                firing_apertures: Vec::new(),
                support_nodes: Vec::new(),
                drain_route: None,
                drainage_catchments: Vec::new(),
                weather_catchments: Vec::new(),
                weathering_solids: Vec::new(),
                roof_support_solids: Vec::new(),
                roof_bearing_node: None,
            });
            geometry.junction_bonds.pop();
            continue;
        }
        let projection = if material == ProjectedDefenseMaterial::Timber {
            1.15
        } else {
            1.35
        };
        let inner_walk = 0.9;
        let throat_depth = projection - inner_walk - 0.14;
        let floor_node = StructuralNodeId(wall_node.0 + 90);
        let bay_count = (length / 1.05).floor().max(2.0) as usize;
        let mut support_nodes = Vec::new();
        for index in 0..=bay_count {
            let progress = index as f32 / bay_count as f32;
            let mut anchor = run.start.lerp(run.end, progress);
            if index == 0 {
                anchor += tangent * 0.10;
            } else if index == bay_count {
                anchor -= tangent * 0.10;
            }
            let node = StructuralNodeId(wall_node.0 + 1 + index as u64);
            support_nodes.push(node);
            geometry.structural_nodes.push(StructuralNode {
                id: node,
                owner,
                kind: if material == ProjectedDefenseMaterial::Timber {
                    StructuralNodeKind::GalleryFrame
                } else {
                    StructuralNodeKind::ProjectionCorbel
                },
                position: Vec3::new(
                    anchor.x + outward.x * projection * 0.5,
                    run.base_height_metres - 0.42,
                    anchor.y + outward.y * projection * 0.5,
                ),
                supported_by: vec![wall_node],
                grounded: false,
            });
            projected_solid(
                geometry,
                owner,
                Vec3::new(
                    anchor.x + outward.x * projection * 0.42,
                    run.base_height_metres - 0.38,
                    anchor.y + outward.y * projection * 0.42,
                ),
                Vec3::new(0.16, 0.34, projection * 0.84),
                yaw,
                if material == ProjectedDefenseMaterial::Timber {
                    SolidRole::FrameMember
                } else {
                    SolidRole::ProjectionSupport
                },
                vec![wall_node],
            );
        }
        geometry.structural_nodes.push(StructuralNode {
            id: floor_node,
            owner,
            kind: StructuralNodeKind::GalleryFrame,
            position: Vec3::new(midpoint.x, run.base_height_metres, midpoint.y),
            supported_by: support_nodes.clone(),
            grounded: false,
        });
        for node in &support_nodes {
            let position = geometry
                .structural_nodes
                .iter()
                .find(|candidate| candidate.id == *node)
                .expect("projected support node")
                .position;
            let tangent_extent = tangent.abs() * 0.08;
            let outward_extent = outward.abs() * (projection * 0.5);
            let extent = tangent_extent + outward_extent;
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((4_u64 << 60) | geometry.support_interfaces.len() as u64),
                owner,
                node: *node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        position.x - extent.x,
                        run.base_height_metres - 0.09,
                        position.z - extent.y,
                    ),
                    max: Vec3::new(
                        position.x + extent.x,
                        run.base_height_metres - 0.06,
                        position.z + extent.y,
                    ),
                },
            });
        }
        let mut socket_joists = Vec::new();
        if material == ProjectedDefenseMaterial::Timber {
            for socket in &host.sockets {
                let bounds = geometry
                    .voids
                    .iter()
                    .find(|void| void.id == *socket)
                    .expect("host beam socket")
                    .bounds;
                let socket_centre = (bounds.min + bounds.max) * 0.5;
                let centre = Vec2::new(socket_centre.x, socket_centre.z) + outward * (0.52 - 0.17);
                let joist = projected_solid(
                    geometry,
                    owner,
                    Vec3::new(centre.x, socket_centre.y, centre.y),
                    Vec3::new(0.16, 0.18, 1.04),
                    yaw,
                    SolidRole::BeamJoist,
                    vec![wall_node],
                );
                socket_joists.push((*socket, joist));
            }
        }
        let mut floor_solids = vec![
            projected_solid(
                geometry,
                owner,
                Vec3::new(
                    midpoint.x + outward.x * (0.12 + (inner_walk - 0.14) * 0.5),
                    run.base_height_metres - 0.07,
                    midpoint.y + outward.y * (0.12 + (inner_walk - 0.14) * 0.5),
                ),
                // Keep the pitched floor skin positively clear of the first
                // downward-defense throat. Rotating a slab whose nominal edge
                // merely touches the throat would otherwise push its lower
                // corner a few millimetres into the opening.
                Vec3::new(length, 0.14, inner_walk - 0.14),
                yaw,
                SolidRole::GalleryFloor,
                vec![floor_node],
            ),
            projected_solid(
                geometry,
                owner,
                Vec3::new(
                    midpoint.x + outward.x * (projection - 0.07),
                    run.base_height_metres - 0.07,
                    midpoint.y + outward.y * (projection - 0.07),
                ),
                Vec3::new(length, 0.14, 0.14),
                yaw,
                SolidRole::GalleryFloor,
                vec![floor_node],
            ),
        ];
        let outer_breastwork_bearing = floor_solids.pop().expect("outer gallery bearing");
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == outer_breastwork_bearing)
            .expect("outer gallery bearing solid")
            .role = SolidRole::ProjectionSupport;
        let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
        let floor_crossfall = 0.025 * (-outward).dot(local_positive_z).signum();
        let floor = geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_solids[0])
            .expect("new projected gallery floor");
        floor.crossfall_radians = floor_crossfall;
        floor.longfall_radians = 0.003;
        let channel_length = length - 0.11;
        let channel_centre = midpoint - tangent * 0.055 + outward * 0.06;
        let drainage_floor = projected_solid(
            geometry,
            owner,
            Vec3::new(
                channel_centre.x,
                run.base_height_metres - 0.055,
                channel_centre.y,
            ),
            Vec3::new(channel_length, 0.06, 0.12),
            yaw,
            SolidRole::DrainageFloor,
            vec![floor_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == drainage_floor)
            .expect("new projected channel floor")
            .longfall_radians = -0.018;
        let bay_length = length / bay_count as f32;
        let mut throat_voids = Vec::new();
        for index in 0..bay_count {
            let along = -length * 0.5 + (index as f32 + 0.5) * bay_length;
            let throat_centre =
                midpoint + tangent * along + outward * (inner_walk + throat_depth * 0.5);
            let throat = projected_void(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(
                        throat_centre.x
                            - tangent.x.abs() * (bay_length - 0.18) * 0.5
                            - outward.x.abs() * throat_depth * 0.5,
                        run.base_height_metres - 0.17,
                        throat_centre.y
                            - tangent.y.abs() * (bay_length - 0.18) * 0.5
                            - outward.y.abs() * throat_depth * 0.5,
                    ),
                    max: Vec3::new(
                        throat_centre.x
                            + tangent.x.abs() * (bay_length - 0.18) * 0.5
                            + outward.x.abs() * throat_depth * 0.5,
                        run.base_height_metres + 0.03,
                        throat_centre.y
                            + tangent.y.abs() * (bay_length - 0.18) * 0.5
                            + outward.y.abs() * throat_depth * 0.5,
                    ),
                },
                VoidRole::DefenseThroat,
            );
            throat_voids.push(throat);
            let stance = Vec3::new(
                throat_centre.x - outward.x * 0.42,
                run.base_height_metres,
                throat_centre.y - outward.y * 0.42,
            );
            let origin = Vec3::new(
                throat_centre.x + outward.x * throat_depth * 0.48,
                run.base_height_metres + 0.025,
                throat_centre.y + outward.y * throat_depth * 0.48,
            );
            geometry
                .projected_defense_working_points
                .push(ProjectedDefenseWorkingPoint {
                    owner,
                    aperture: throat,
                    stance,
                    eye: stance + Vec3::Y * 1.55,
                    support_solid: floor_solids[0],
                });
            for (range, distance) in [
                (ProjectedDefenseRange::Near, 0.75_f32),
                (ProjectedDefenseRange::Middle, 1.6_f32),
                (ProjectedDefenseRange::Far, 3.0_f32),
            ] {
                geometry.projected_defense_rays.push(ProjectedDefenseRay {
                    owner,
                    throat,
                    stance,
                    origin,
                    target: Vec3::new(
                        throat_centre.x + outward.x * distance,
                        0.25,
                        throat_centre.y + outward.y * distance,
                    ),
                    range,
                });
            }
        }
        let outer_wall_centre = midpoint + outward * (projection + 0.09);
        let wall_role = if material == ProjectedDefenseMaterial::Timber {
            SolidRole::FrameMember
        } else {
            SolidRole::DefenseWall
        };
        let aperture_half_width = 0.12;
        let aperture_along = bay_length.min(length * 0.28);
        let middle_intervals = [
            (-length * 0.5, -aperture_along - aperture_half_width),
            (
                -aperture_along + aperture_half_width,
                aperture_along - aperture_half_width,
            ),
            (aperture_along + aperture_half_width, length * 0.5),
        ];
        let wall_segments = [(0.0, length, 0.55, 0.275), (0.0, length, 0.14, 1.09)]
            .into_iter()
            .chain(
                middle_intervals
                    .into_iter()
                    .map(|(start, end)| ((start + end) * 0.5, end - start, 0.47, 0.785)),
            );
        let mut enclosure_wall_solids = Vec::new();
        for (along, segment_length, height, vertical_centre) in wall_segments {
            if segment_length <= 0.05 {
                continue;
            }
            let centre = outer_wall_centre + tangent * along;
            enclosure_wall_solids.push(projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, run.base_height_metres + vertical_centre, centre.y),
                Vec3::new(segment_length, height, 0.18),
                yaw,
                wall_role,
                vec![floor_node],
            ));
        }
        if material == ProjectedDefenseMaterial::Timber {
            for index in 0..=bay_count {
                let anchor = run.start.lerp(run.end, index as f32 / bay_count as f32)
                    + outward * (projection + 0.09);
                projected_solid(
                    geometry,
                    owner,
                    Vec3::new(anchor.x, run.base_height_metres + 1.1, anchor.y),
                    Vec3::new(0.14, 2.2, 0.14),
                    yaw,
                    SolidRole::FrameMember,
                    vec![floor_node],
                );
            }
        }
        let access_portal = host.portal.expect("operational defense host portal");
        let landing_centre = midpoint - outward * 0.17;
        let access_landing = projected_solid(
            geometry,
            owner,
            Vec3::new(
                landing_centre.x,
                run.base_height_metres - 0.07,
                landing_centre.y,
            ),
            Vec3::new(0.86, 0.14, 0.66),
            yaw,
            SolidRole::Landing,
            vec![floor_node],
        );
        let mut firing_apertures = Vec::new();
        for side in [-1.0_f32, 1.0] {
            let aperture = outer_wall_centre + tangent * aperture_along * side;
            let aperture_id = projected_void(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(
                        aperture.x - 0.09,
                        run.base_height_metres + 0.55,
                        aperture.y - 0.09,
                    ),
                    max: Vec3::new(
                        aperture.x + 0.09,
                        run.base_height_metres + 1.02,
                        aperture.y + 0.09,
                    ),
                },
                VoidRole::FiringAperture,
            );
            firing_apertures.push(aperture_id);
            let stance = Vec3::new(
                aperture.x - outward.x * 0.52,
                run.base_height_metres,
                aperture.y - outward.y * 0.52,
            );
            let eye = Vec3::new(aperture.x, run.base_height_metres + 0.79, aperture.y);
            geometry
                .projected_defense_working_points
                .push(ProjectedDefenseWorkingPoint {
                    owner,
                    aperture: aperture_id,
                    stance,
                    eye,
                    support_solid: floor_solids[0],
                });
            for (range, distance) in [
                (ProjectedDefenseRange::Near, 2.0_f32),
                (ProjectedDefenseRange::Middle, 6.0_f32),
                (ProjectedDefenseRange::Far, 12.0_f32),
            ] {
                geometry.projected_defense_rays.push(ProjectedDefenseRay {
                    owner,
                    throat: aperture_id,
                    stance,
                    origin: eye,
                    target: eye + Vec3::new(outward.x * distance, -0.55, outward.y * distance),
                    range,
                });
            }
        }
        let mut weather_catchments = Vec::new();
        let mut weathering_solids = Vec::new();
        let mut roof_support_solids = Vec::new();
        let mut roof_bearing_node = None;
        if roofed {
            let roof_depth = projection + 0.45;
            let roof_support = if kind == ProjectedDefenseKind::Breteche {
                let inner_bearing = StructuralNodeId(floor_node.0 + 1);
                let outer_bearing = StructuralNodeId(floor_node.0 + 2);
                let roof_bearing = StructuralNodeId(floor_node.0 + 3);
                for (id, position) in [
                    (inner_bearing, midpoint),
                    (outer_bearing, outer_wall_centre),
                ] {
                    geometry.structural_nodes.push(StructuralNode {
                        id,
                        owner,
                        kind: StructuralNodeKind::GalleryFrame,
                        position: Vec3::new(position.x, run.base_height_metres, position.y),
                        supported_by: vec![floor_node],
                        grounded: false,
                    });
                }
                geometry.structural_nodes.push(StructuralNode {
                    id: roof_bearing,
                    owner,
                    kind: StructuralNodeKind::GalleryFrame,
                    position: Vec3::new(
                        midpoint.x + outward.x * projection * 0.55,
                        run.base_height_metres + 2.18,
                        midpoint.y + outward.y * projection * 0.55,
                    ),
                    supported_by: vec![inner_bearing, outer_bearing],
                    grounded: false,
                });
                roof_bearing_node = Some(roof_bearing);
                roof_bearing
            } else {
                floor_node
            };
            let roof_id = projected_solid(
                geometry,
                owner,
                Vec3::new(
                    midpoint.x + outward.x * projection * 0.55,
                    run.base_height_metres + 2.25,
                    midpoint.y + outward.y * projection * 0.55,
                ),
                Vec3::new(length + 0.35, 0.14, roof_depth),
                yaw,
                SolidRole::DefenseRoof,
                vec![roof_support],
            );
            let (catchment, solids) = resolve_linear_roof_weathering(
                geometry,
                owner,
                roof_id,
                midpoint + outward * projection * 0.55,
                tangent,
                outward,
                length + 0.35,
                roof_depth,
                yaw,
                roof_support,
            );
            weather_catchments.push(catchment);
            weathering_solids.extend(solids);
            if kind == ProjectedDefenseKind::Breteche {
                let roof = geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == roof_id)
                    .expect("resolved bretèche roof")
                    .clone();
                let roof_midpoint = Vec2::new(roof.centre.x, roof.centre.z);
                let underside_at = |point: Vec2| {
                    let offset = (point - roof_midpoint).dot(outward);
                    roof.centre.y - offset * roof.crossfall_radians.abs().tan() - roof.size.y * 0.5
                };
                let inner_plate_plan = midpoint + outward * 0.02;
                let outer_plate_plan = outer_wall_centre;
                let inner_underside = underside_at(inner_plate_plan);
                let outer_underside = underside_at(outer_plate_plan);
                let plate_height = 0.16;
                let inner_bearing = StructuralNodeId(floor_node.0 + 1);
                let outer_bearing = StructuralNodeId(floor_node.0 + 2);

                // Extend the already-resolved upper outer-wall band to the low
                // wall plate. This retains the two firing-loop cuts below it
                // while removing the formerly open metre-high sky band.
                let upper_wall = enclosure_wall_solids
                    .get(1)
                    .copied()
                    .expect("bretèche upper enclosure wall");
                let upper_wall_bottom = run.base_height_metres + 1.02;
                let upper_wall_top = outer_underside - plate_height;
                let wall = geometry
                    .solids
                    .iter_mut()
                    .find(|solid| solid.id == upper_wall)
                    .expect("bretèche upper enclosure wall solid");
                wall.centre.y = (upper_wall_bottom + upper_wall_top) * 0.5;
                wall.size.y = upper_wall_top - upper_wall_bottom;

                for side in [-1.0_f32, 1.0] {
                    let post_plan = inner_plate_plan + tangent * side * (length * 0.5 - 0.38);
                    let post_height = inner_underside - plate_height - run.base_height_metres;
                    roof_support_solids.push(projected_solid(
                        geometry,
                        owner,
                        Vec3::new(
                            post_plan.x,
                            run.base_height_metres + post_height * 0.5,
                            post_plan.y,
                        ),
                        Vec3::new(0.18, post_height, 0.18),
                        yaw,
                        SolidRole::FrameMember,
                        vec![floor_node],
                    ));
                }
                for (plan, underside, bearing) in [
                    (inner_plate_plan, inner_underside, inner_bearing),
                    (outer_plate_plan, outer_underside, outer_bearing),
                ] {
                    let plate = projected_solid(
                        geometry,
                        owner,
                        Vec3::new(plan.x, underside - plate_height * 0.5, plan.y),
                        Vec3::new(length + 0.18, plate_height, 0.18),
                        yaw,
                        SolidRole::RoofPlate,
                        vec![bearing],
                    );
                    geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == plate)
                        .expect("bretèche roof plate")
                        .crossfall_radians = roof.crossfall_radians;
                    roof_support_solids.push(plate);
                }
                roof_support_solids.push(upper_wall);
            }
        } else if material == ProjectedDefenseMaterial::Masonry {
            let (catchment, solids) = resolve_linear_coping_weathering(
                geometry,
                owner,
                outer_wall_centre,
                tangent,
                outward,
                length,
                run.base_height_metres + 1.22,
                yaw,
                floor_node,
            );
            weather_catchments.push(catchment);
            weathering_solids.extend(solids);
        }
        let drainage_surface = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    midpoint.x - tangent.x.abs() * length * 0.5,
                    run.base_height_metres,
                    midpoint.y - tangent.y.abs() * length * 0.5,
                ),
                max: Vec3::new(
                    midpoint.x + tangent.x.abs() * length * 0.5 + outward.x.abs() * projection,
                    run.base_height_metres + 0.02,
                    midpoint.y + tangent.y.abs() * length * 0.5 + outward.y.abs() * projection,
                ),
            },
            SurfaceRole::Drainage,
        );
        let drain_inlet = Vec3::new(
            midpoint.x + tangent.x * (length * 0.5 - 0.11) + outward.x * 0.06,
            run.base_height_metres - 0.03,
            midpoint.y + tangent.y * (length * 0.5 - 0.11) + outward.y * 0.06,
        );
        let drain_route = projected_edge_drain(geometry, owner, drain_inlet, tangent);
        let catchment_id =
            ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment_id,
            owner,
            walk_solid: floor_solids[0],
            toe_channel_solids: vec![drainage_floor],
            drainage_surface,
            outlet_route: drain_route,
            centre: Vec3::new(midpoint.x, run.base_height_metres, midpoint.y),
            tangent,
            outward: -outward,
            length_metres: length,
            width_metres: inner_walk - 0.12,
            inner_elevation_metres: run.base_height_metres,
            outer_elevation_metres: run.base_height_metres - 0.025,
            outlet_along_metres: length * 0.5 - 0.11,
        });
        assemblies.push(ProjectedDefenseAssembly {
            owner,
            host_owner: host.owner,
            host_wall_solids: host.walls,
            host_buttress_solids: host.buttresses,
            host_source_walls: host.sources,
            host_top_elevation_metres: host.top_elevation_metres,
            host_topology: host.topology,
            host_walk_solid: host.walk,
            host_portal_void: Some(access_portal),
            host_bond: Some(bond_id),
            beam_socket_voids: host.sockets,
            socket_joists,
            kind,
            material,
            phase,
            deployment,
            tactical_target,
            path: ProjectedDefensePath::Linear {
                start: run.start,
                end: run.end,
                outward: run.outward,
            },
            floor_elevation_metres: run.base_height_metres,
            clear_width_metres: inner_walk,
            clear_height_metres: 2.05,
            projection_metres: projection,
            breastwork_height_metres: 1.16,
            roofed,
            floor_solids,
            throat_voids,
            access_portal: Some(access_portal),
            access_landing: Some(access_landing),
            firing_apertures,
            support_nodes,
            drain_route: Some(drain_route),
            drainage_catchments: vec![catchment_id],
            weather_catchments,
            weathering_solids,
            roof_support_solids,
            roof_bearing_node,
        });
    }
    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let plan_centre = dimensions * 0.5;
    for (index, bartizan) in bartizans.iter().copied().enumerate() {
        let owner = GeometryOwnerId(2_000 + index as u32);
        let delta = bartizan.centre - plan_centre;
        let outward_direction = if delta.x.abs() >= delta.y.abs() {
            if delta.x >= 0.0 {
                Direction::East
            } else {
                Direction::West
            }
        } else if delta.y >= 0.0 {
            Direction::North
        } else {
            Direction::South
        };
        let outward = direction_vector(outward_direction);
        let tangent = Vec2::new(-outward.y, outward.x);
        let yaw = -tangent.y.atan2(tangent.x);
        let host_midpoint = match outward_direction {
            Direction::East => Vec2::new(dimensions.x, bartizan.centre.y),
            Direction::West => Vec2::new(0.0, bartizan.centre.y),
            Direction::North => Vec2::new(bartizan.centre.x, dimensions.y),
            Direction::South => Vec2::new(bartizan.centre.x, 0.0),
        };
        let mut host = resolve_linear_defense_host(
            geometry,
            storeys,
            100 + index,
            BattlementRun {
                start: host_midpoint - tangent * bartizan.radius_metres,
                end: host_midpoint + tangent * bartizan.radius_metres,
                base_height_metres: bartizan.base_height_metres,
                kind: BattlementKind::Breteche,
                outward: outward_direction,
            },
            None,
            true,
        );
        let buttress_depth = bartizan.radius_metres * 0.92;
        let buttress_centre = host_midpoint + outward * buttress_depth * 0.5;
        let buttress_top = bartizan.base_height_metres - 0.14;
        let buttress = projected_solid(
            geometry,
            host.owner,
            Vec3::new(buttress_centre.x, buttress_top * 0.5, buttress_centre.y),
            Vec3::new(0.18, buttress_top, buttress_depth),
            yaw,
            SolidRole::DefenseHostButtress,
            vec![host.bearing],
        );
        host.buttresses.push(buttress);
        host.topology = ProjectedDefenseHostTopology::Buttress;
        let wall_node = host.bearing;
        let floor_node = StructuralNodeId(wall_node.0 + 10);
        let host_bond = ResolvedItemId((6_u64 << 60) | (10_000 + index) as u64);
        geometry.junction_bonds.push(JunctionBond {
            id: host_bond,
            owners: [host.owner, owner],
            bounds: ResolvedBounds {
                min: Vec3::new(
                    host_midpoint.x
                        - tangent.x.abs() * bartizan.radius_metres
                        - outward.x.abs() * 0.75,
                    bartizan.base_height_metres - 0.6,
                    host_midpoint.y
                        - tangent.y.abs() * bartizan.radius_metres
                        - outward.y.abs() * 0.75,
                ),
                max: Vec3::new(
                    host_midpoint.x
                        + tangent.x.abs() * bartizan.radius_metres
                        + outward.x.abs() * 0.75,
                    bartizan.base_height_metres + bartizan.height_metres + 0.3,
                    host_midpoint.y
                        + tangent.y.abs() * bartizan.radius_metres
                        + outward.y.abs() * 0.75,
                ),
            },
            minimum_interface_area_square_metres: 0.08,
            maximum_penetration_metres: 0.18,
        });
        let mut corbel_nodes = Vec::new();
        for (index, offset) in [-0.5_f32, 0.0, 0.5].into_iter().enumerate() {
            let corbel_node = StructuralNodeId(wall_node.0 + 1 + index as u64);
            corbel_nodes.push(corbel_node);
            let centre =
                bartizan.centre - outward * bartizan.radius_metres * 0.35 + tangent * offset;
            geometry.structural_nodes.push(StructuralNode {
                id: corbel_node,
                owner,
                kind: StructuralNodeKind::ProjectionCorbel,
                position: Vec3::new(centre.x, bartizan.base_height_metres - 0.35, centre.y),
                supported_by: vec![wall_node],
                grounded: false,
            });
            projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, bartizan.base_height_metres - 0.35, centre.y),
                Vec3::new(0.22, 0.48, bartizan.radius_metres * 1.2),
                yaw,
                SolidRole::ProjectionSupport,
                vec![wall_node],
            );
        }
        geometry.structural_nodes.push(StructuralNode {
            id: floor_node,
            owner,
            kind: StructuralNodeKind::GalleryFrame,
            position: Vec3::new(
                bartizan.centre.x,
                bartizan.base_height_metres,
                bartizan.centre.y,
            ),
            supported_by: corbel_nodes.clone(),
            grounded: false,
        });
        for node in &corbel_nodes {
            let position = geometry
                .structural_nodes
                .iter()
                .find(|candidate| candidate.id == *node)
                .expect("bartizan support node")
                .position;
            let tangent_extent = tangent.abs() * 0.11;
            let outward_extent = outward.abs() * (bartizan.radius_metres * 0.55);
            let extent = tangent_extent + outward_extent;
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((4_u64 << 60) | geometry.support_interfaces.len() as u64),
                owner,
                node: *node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        position.x - extent.x,
                        bartizan.base_height_metres - 0.09,
                        position.z - extent.y,
                    ),
                    max: Vec3::new(
                        position.x + extent.x,
                        bartizan.base_height_metres - 0.06,
                        position.z + extent.y,
                    ),
                },
            });
        }
        let segments = 16;
        let half_span = bartizan.radius_metres * 0.82;
        // The resolved throat is an axis-aligned subtraction while the
        // bartizan floor bays are wall-local cuboids. Keep the authoritative
        // opening at 0.36 m, but trim the surrounding local bays by its
        // projected diagonal extent plus a construction joint.
        let throat_void_half = 0.18;
        let throat_clear_half = throat_void_half * (outward.x.abs() + outward.y.abs()) + 0.03;
        let throat_inner = bartizan.radius_metres * 0.55 - throat_clear_half;
        let inner_edge = -bartizan.radius_metres * 0.82;
        let outer_edge = bartizan.radius_metres * 0.82;
        let mut floor_solids = vec![projected_solid(
            geometry,
            owner,
            Vec3::new(
                bartizan.centre.x + outward.x * (inner_edge + throat_inner) * 0.5,
                bartizan.base_height_metres - 0.07,
                bartizan.centre.y + outward.y * (inner_edge + throat_inner) * 0.5,
            ),
            Vec3::new(half_span * 2.0, 0.14, throat_inner - inner_edge),
            yaw,
            SolidRole::GalleryFloor,
            vec![floor_node],
        )];
        let side_width = half_span - throat_clear_half;
        for side in [-1.0_f32, 1.0] {
            let centre = bartizan.centre
                + outward * ((throat_inner + outer_edge) * 0.5)
                + tangent * side * ((throat_clear_half + half_span) * 0.5);
            floor_solids.push(projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, bartizan.base_height_metres - 0.07, centre.y),
                Vec3::new(side_width, 0.14, outer_edge - throat_inner),
                yaw,
                SolidRole::GalleryFloor,
                vec![floor_node],
            ));
        }
        let throat_centre = bartizan.centre + outward * bartizan.radius_metres * 0.55;
        let throat = projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    throat_centre.x - throat_void_half,
                    bartizan.base_height_metres - 0.18,
                    throat_centre.y - throat_void_half,
                ),
                max: Vec3::new(
                    throat_centre.x + throat_void_half,
                    bartizan.base_height_metres + 0.03,
                    throat_centre.y + throat_void_half,
                ),
            },
            VoidRole::DefenseThroat,
        );
        let bartizan_stance = Vec3::new(
            throat_centre.x - outward.x * 0.42,
            bartizan.base_height_metres,
            throat_centre.y - outward.y * 0.42,
        );
        let bartizan_origin = Vec3::new(
            throat_centre.x,
            bartizan.base_height_metres + 0.025,
            throat_centre.y,
        );
        geometry
            .projected_defense_working_points
            .push(ProjectedDefenseWorkingPoint {
                owner,
                aperture: throat,
                stance: bartizan_stance,
                eye: bartizan_stance + Vec3::Y * 1.55,
                support_solid: floor_solids[0],
            });
        for (range, distance) in [
            (ProjectedDefenseRange::Near, 0.75_f32),
            (ProjectedDefenseRange::Middle, 1.4_f32),
            (ProjectedDefenseRange::Far, 2.6_f32),
        ] {
            geometry.projected_defense_rays.push(ProjectedDefenseRay {
                owner,
                throat,
                stance: bartizan_stance,
                origin: bartizan_origin,
                target: Vec3::new(
                    throat_centre.x + outward.x * distance,
                    0.25,
                    throat_centre.y + outward.y * distance,
                ),
                range,
            });
        }
        let inward = -outward;
        let access_portal = host.portal.expect("bartizan host access portal");
        let landing = host_midpoint - outward * 0.08;
        let access_landing = projected_solid(
            geometry,
            owner,
            Vec3::new(landing.x, bartizan.base_height_metres - 0.07, landing.y),
            Vec3::new(0.86, 0.14, 0.66),
            yaw,
            SolidRole::Landing,
            vec![floor_node],
        );
        let mut firing_apertures = Vec::new();
        for side in [-2_i32, 0, 2] {
            let side = side as f32 * std::f32::consts::TAU / segments as f32;
            let direction = Vec2::new(
                (outward.y.atan2(outward.x) + side).cos(),
                (outward.y.atan2(outward.x) + side).sin(),
            );
            let aperture_half = Vec2::splat(0.065);
            let wall_centre = bartizan.centre + direction * bartizan.radius_metres;
            let aperture = projected_void(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(
                        wall_centre.x - aperture_half.x,
                        bartizan.base_height_metres + 0.75,
                        wall_centre.y - aperture_half.y,
                    ),
                    max: Vec3::new(
                        wall_centre.x + aperture_half.x,
                        bartizan.base_height_metres + 1.22,
                        wall_centre.y + aperture_half.y,
                    ),
                },
                VoidRole::FiringAperture,
            );
            firing_apertures.push(aperture);
            let stance_plan = if side.abs() < 0.01 {
                bartizan.centre + outward * 0.20
            } else {
                bartizan.centre + direction * (bartizan.radius_metres - 0.38)
            };
            let stance = Vec3::new(stance_plan.x, bartizan.base_height_metres, stance_plan.y);
            let eye = Vec3::new(
                wall_centre.x,
                bartizan.base_height_metres + 0.985,
                wall_centre.y,
            );
            let support_solid = floor_solids
                .iter()
                .copied()
                .find(|id| {
                    geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == *id)
                        .is_some_and(|solid| {
                            resolved_solid_contains_point(solid, stance - Vec3::Y * 0.02, 0.08)
                        })
                })
                .unwrap_or(floor_solids[0]);
            geometry
                .projected_defense_working_points
                .push(ProjectedDefenseWorkingPoint {
                    owner,
                    aperture,
                    stance,
                    eye,
                    support_solid,
                });
            for (range, distance) in [
                (ProjectedDefenseRange::Near, 2.0_f32),
                (ProjectedDefenseRange::Middle, 6.0_f32),
                (ProjectedDefenseRange::Far, 12.0_f32),
            ] {
                geometry.projected_defense_rays.push(ProjectedDefenseRay {
                    owner,
                    throat: aperture,
                    stance,
                    origin: eye,
                    target: eye + Vec3::new(direction.x * distance, -0.55, direction.y * distance),
                    range,
                });
            }
        }
        for segment in 0..segments {
            let angle = segment as f32 * std::f32::consts::TAU / segments as f32;
            let radial = Vec2::new(angle.cos(), angle.sin());
            // Three inward facets form the real doorway chord; unlike the old
            // half-cylinder deletion, every other facet remains structural.
            if radial.dot(inward) > 0.88 {
                continue;
            }
            let centre = bartizan.centre + radial * bartizan.radius_metres;
            let facet_length =
                2.0 * bartizan.radius_metres * (std::f32::consts::PI / segments as f32).tan()
                    + 0.03;
            let aperture = firing_apertures
                .iter()
                .filter_map(|id| {
                    geometry
                        .voids
                        .iter()
                        .find(|void| void.id == *id)
                        .and_then(|void| {
                            let aperture_centre = (void.bounds.min + void.bounds.max) * 0.5;
                            let distance =
                                Vec2::new(aperture_centre.x, aperture_centre.z).distance(centre);
                            (distance < 0.55).then_some((distance, *id, *void))
                        })
                })
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .map(|(_, id, void)| (id, void));
            let shell_yaw = -angle - std::f32::consts::FRAC_PI_2;
            if let Some((_id, aperture)) = aperture {
                let lower_height = aperture.bounds.min.y - bartizan.base_height_metres;
                let upper_height =
                    bartizan.base_height_metres + bartizan.height_metres - aperture.bounds.max.y;
                for (height, vertical_centre) in [
                    (
                        lower_height,
                        bartizan.base_height_metres + lower_height * 0.5,
                    ),
                    (upper_height, aperture.bounds.max.y + upper_height * 0.5),
                ] {
                    if height > 0.02 {
                        projected_solid(
                            geometry,
                            owner,
                            Vec3::new(centre.x, vertical_centre, centre.y),
                            Vec3::new(facet_length, height, 0.18),
                            shell_yaw,
                            SolidRole::BartizanShell,
                            vec![floor_node],
                        );
                    }
                }
                let facet_tangent = Vec2::new(-radial.y, radial.x);
                let aperture_centre = (aperture.bounds.min + aperture.bounds.max) * 0.5;
                let aperture_half_bounds = (aperture.bounds.max - aperture.bounds.min) * 0.5;
                let aperture_offset =
                    (Vec2::new(aperture_centre.x, aperture_centre.z) - centre).dot(facet_tangent);
                let splayed_half_width = facet_tangent.x.abs() * aperture_half_bounds.x
                    + facet_tangent.y.abs() * aperture_half_bounds.z
                    + 0.06;
                let opening_min = (aperture_offset - splayed_half_width).max(-facet_length * 0.5);
                let opening_max = (aperture_offset + splayed_half_width).min(facet_length * 0.5);
                for (side_width, side_offset) in [
                    (
                        opening_min + facet_length * 0.5,
                        (-facet_length * 0.5 + opening_min) * 0.5,
                    ),
                    (
                        facet_length * 0.5 - opening_max,
                        (opening_max + facet_length * 0.5) * 0.5,
                    ),
                ] {
                    if side_width <= 0.01 {
                        continue;
                    }
                    let side_centre = centre + facet_tangent * side_offset;
                    projected_solid(
                        geometry,
                        owner,
                        Vec3::new(
                            side_centre.x,
                            (aperture.bounds.min.y + aperture.bounds.max.y) * 0.5,
                            side_centre.y,
                        ),
                        Vec3::new(
                            side_width,
                            aperture.bounds.max.y - aperture.bounds.min.y,
                            0.18,
                        ),
                        shell_yaw,
                        SolidRole::BartizanShell,
                        vec![floor_node],
                    );
                }
            } else {
                projected_solid(
                    geometry,
                    owner,
                    Vec3::new(
                        centre.x,
                        bartizan.base_height_metres + bartizan.height_metres * 0.5,
                        centre.y,
                    ),
                    Vec3::new(facet_length, bartizan.height_metres, 0.18),
                    shell_yaw,
                    SolidRole::BartizanShell,
                    vec![floor_node],
                );
            }
        }
        for floor_id in &floor_solids {
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == *floor_id)
                .expect("bartizan floor")
                .longfall_radians = -0.022;
        }
        let channel_yaw = -outward.y.atan2(outward.x);
        let bartizan_channel_centre =
            bartizan.centre + tangent * (half_span + 0.06) - outward * 0.055;
        let bartizan_channel = projected_solid(
            geometry,
            owner,
            Vec3::new(
                bartizan_channel_centre.x,
                bartizan.base_height_metres - 0.055,
                bartizan_channel_centre.y,
            ),
            Vec3::new(half_span * 2.0 - 0.11, 0.06, 0.12),
            channel_yaw,
            SolidRole::DrainageFloor,
            vec![floor_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == bartizan_channel)
            .expect("bartizan drainage channel")
            .longfall_radians = -0.018;
        let mut weather_catchments = Vec::new();
        let mut weathering_solids = Vec::new();
        if bartizan.roofed {
            let roof_extent = bartizan.radius_metres * 2.25;
            let roof_id = projected_solid(
                geometry,
                owner,
                Vec3::new(
                    bartizan.centre.x,
                    bartizan.base_height_metres + bartizan.height_metres + 0.08,
                    bartizan.centre.y,
                ),
                Vec3::new(roof_extent, 0.16, roof_extent),
                yaw,
                SolidRole::DefenseRoof,
                vec![floor_node],
            );
            let (catchment, solids) = resolve_linear_roof_weathering(
                geometry,
                owner,
                roof_id,
                bartizan.centre,
                tangent,
                outward,
                roof_extent,
                roof_extent,
                yaw,
                floor_node,
            );
            weather_catchments.push(catchment);
            weathering_solids.extend(solids);
        } else {
            for segment in 0..segments {
                let angle = segment as f32 * std::f32::consts::TAU / segments as f32;
                let radial = Vec2::new(angle.cos(), angle.sin());
                if radial.dot(inward) > 0.88 {
                    continue;
                }
                let facet_tangent = Vec2::new(-radial.y, radial.x);
                let facet_length =
                    2.0 * bartizan.radius_metres * (std::f32::consts::PI / segments as f32).tan()
                        + 0.03;
                let facet_centre = bartizan.centre + radial * bartizan.radius_metres;
                let facet_yaw = -angle - std::f32::consts::FRAC_PI_2;
                let (catchment, solids) = resolve_linear_coping_weathering(
                    geometry,
                    owner,
                    facet_centre,
                    facet_tangent,
                    radial,
                    facet_length,
                    bartizan.base_height_metres + bartizan.height_metres + 0.08,
                    facet_yaw,
                    floor_node,
                );
                weather_catchments.push(catchment);
                weathering_solids.extend(solids);
            }
        }
        let drainage_surface = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    bartizan.centre.x - bartizan.radius_metres,
                    bartizan.base_height_metres,
                    bartizan.centre.y - bartizan.radius_metres,
                ),
                max: Vec3::new(
                    bartizan.centre.x + bartizan.radius_metres,
                    bartizan.base_height_metres + 0.02,
                    bartizan.centre.y + bartizan.radius_metres,
                ),
            },
            SurfaceRole::Drainage,
        );
        let drain_route = projected_edge_drain(
            geometry,
            owner,
            Vec3::new(
                bartizan.centre.x + tangent.x * (half_span + 0.06) + outward.x * (half_span - 0.11),
                bartizan.base_height_metres - 0.03,
                bartizan.centre.y + tangent.y * (half_span + 0.06) + outward.y * (half_span - 0.11),
            ),
            outward,
        );
        let catchment_id =
            ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment_id,
            owner,
            walk_solid: floor_solids[0],
            toe_channel_solids: vec![bartizan_channel],
            drainage_surface,
            outlet_route: drain_route,
            centre: Vec3::new(
                bartizan.centre.x,
                bartizan.base_height_metres,
                bartizan.centre.y,
            ),
            tangent: outward,
            outward: tangent,
            length_metres: half_span * 2.0,
            width_metres: half_span * 2.0,
            inner_elevation_metres: bartizan.base_height_metres,
            outer_elevation_metres: bartizan.base_height_metres - 0.035,
            outlet_along_metres: half_span - 0.11,
        });
        assemblies.push(ProjectedDefenseAssembly {
            owner,
            host_owner: host.owner,
            host_wall_solids: host.walls,
            host_buttress_solids: host.buttresses,
            host_source_walls: host.sources,
            host_top_elevation_metres: host.top_elevation_metres,
            host_topology: host.topology,
            host_walk_solid: host.walk,
            host_portal_void: Some(access_portal),
            host_bond: Some(host_bond),
            beam_socket_voids: Vec::new(),
            socket_joists: Vec::new(),
            kind: ProjectedDefenseKind::Bartizan,
            material: ProjectedDefenseMaterial::Masonry,
            phase: ProjectedDefensePhase::PermanentMainWork,
            deployment: ProjectedDefenseDeployment::Permanent,
            tactical_target: ProjectedDefenseTarget::ThreatenedCorner,
            path: ProjectedDefensePath::Round {
                centre: bartizan.centre,
                radius_metres: bartizan.radius_metres,
                outward: outward_direction,
            },
            floor_elevation_metres: bartizan.base_height_metres,
            clear_width_metres: bartizan.radius_metres * 1.2,
            clear_height_metres: bartizan.height_metres,
            projection_metres: bartizan.radius_metres,
            breastwork_height_metres: bartizan.height_metres,
            roofed: bartizan.roofed,
            floor_solids,
            throat_voids: vec![throat],
            access_portal: Some(access_portal),
            access_landing: Some(access_landing),
            firing_apertures,
            support_nodes: corbel_nodes,
            drain_route: Some(drain_route),
            drainage_catchments: vec![catchment_id],
            weather_catchments,
            weathering_solids,
            roof_support_solids: Vec::new(),
            roof_bearing_node: None,
        });
    }
    assemblies
}
