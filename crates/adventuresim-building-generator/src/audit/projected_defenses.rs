/// Checks the resolved geometry cache against the grid-native gatehouse source.
///
/// The tolerances below are project construction gates, not historical claims.
fn audit_projected_defenses(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for defense in &plan.projected_defenses {
        let solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == defense.owner)
            .collect::<Vec<_>>();
        let voids = plan
            .resolved_geometry
            .voids
            .iter()
            .filter(|void| void.owner == defense.owner)
            .collect::<Vec<_>>();
        let expected_material_phase = match defense.kind {
            ProjectedDefenseKind::Hoarding => {
                defense.material == ProjectedDefenseMaterial::Timber
                    && defense.phase == ProjectedDefensePhase::TemporaryCampaignWork
                    && matches!(
                        defense.deployment,
                        ProjectedDefenseDeployment::SocketsOnly
                            | ProjectedDefenseDeployment::Deployed
                    )
            }
            _ => {
                defense.material == ProjectedDefenseMaterial::Masonry
                    && defense.phase == ProjectedDefensePhase::PermanentMainWork
                    && defense.deployment == ProjectedDefenseDeployment::Permanent
            }
        };
        if !expected_material_phase {
            issues.push(issue(
                "projected_defense_phase_material_mismatch",
                format!(
                    "projected defense owner {} has an incoherent material, phase, or deployment",
                    defense.owner.0
                ),
            ));
        }
        let target_matches_installation = match defense.kind {
            ProjectedDefenseKind::Machicolation => {
                defense.tactical_target == ProjectedDefenseTarget::GateApproach
            }
            ProjectedDefenseKind::Breteche => {
                defense.tactical_target == ProjectedDefenseTarget::ThreatenedWallFoot
            }
            ProjectedDefenseKind::Hoarding => {
                defense.tactical_target == ProjectedDefenseTarget::CampaignSiegeFront
            }
            ProjectedDefenseKind::Bartizan => {
                defense.tactical_target == ProjectedDefenseTarget::ThreatenedCorner
            }
        };
        if !target_matches_installation {
            issues.push(issue(
                "projected_defense_tactical_target_mismatch",
                format!(
                    "projected defense owner {} lacks a coherent named tactical target",
                    defense.owner.0
                ),
            ));
        }
        let outward = match defense.path {
            ProjectedDefensePath::Linear { outward, .. }
            | ProjectedDefensePath::Round { outward, .. } => direction_vector(outward),
        };
        let host_solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == defense.host_owner)
            .collect::<Vec<_>>();
        let host_is_authoritative = defense.host_owner != defense.owner
            && !defense.host_wall_solids.is_empty()
            && defense.host_wall_solids.iter().all(|id| {
                host_solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::DefenseHostWall)
            })
            && host_solids.iter().any(|solid| {
                solid.id == defense.host_walk_solid && solid.role == SolidRole::CircuitWalk
            })
            && host_solids
                .iter()
                .filter(|solid| solid.role == SolidRole::DefenseHostWall)
                .all(|solid| defense.host_wall_solids.contains(&solid.id));
        let host_portal_is_cut = defense.host_portal_void.is_none_or(|id| {
            plan.resolved_geometry.voids.iter().any(|void| {
                void.id == id
                    && void.owner == defense.host_owner
                    && void.subtracts_from == defense.host_owner
                    && void.role == VoidRole::AccessPortal
            })
        });
        let host_bond_is_physical = defense.host_bond.is_none_or(|id| {
            plan.resolved_geometry.junction_bonds.iter().any(|bond| {
                bond.id == id
                    && bond.owners.contains(&defense.owner)
                    && bond.owners.contains(&defense.host_owner)
            })
        });
        let source_walls_are_exact = !defense.host_source_walls.is_empty()
            && defense.host_source_walls.iter().all(|source| {
                let Some(storey) = plan
                    .storeys
                    .iter()
                    .find(|storey| storey.level == source.storey_level)
                else {
                    return false;
                };
                let Some(wall) = storey.walls.get(source.wall_index).copied() else {
                    return false;
                };
                let source_top = f32::from(storey.level + 1) * plan.storey_height_metres;
                let source_bottom = source_top - plan.storey_height_metres;
                if !wall.exterior() || (defense.host_top_elevation_metres - source_top).abs() > 0.01
                {
                    return false;
                }
                let centre = wall.centre();
                let along = if wall.is_horizontal() {
                    Vec2::X
                } else {
                    Vec2::Y
                };
                let source_contains_plan = |point: Vec2| {
                    (point - centre).dot(along).abs() <= crate::CELL_SIZE_METRES * 0.5 + 0.01
                        && (point - centre).dot(direction_vector(wall.direction)).abs() <= 0.1
                };
                let solids_inside = defense.host_wall_solids.iter().all(|id| {
                    host_solids
                        .iter()
                        .find(|solid| solid.id == *id)
                        .is_some_and(|solid| {
                            source_contains_plan(Vec2::new(solid.centre.x, solid.centre.z))
                                || defense.host_source_walls.iter().any(|other| {
                                    plan.storeys
                                        .iter()
                                        .find(|candidate| candidate.level == other.storey_level)
                                        .and_then(|candidate| candidate.walls.get(other.wall_index))
                                        .is_some_and(|other_wall| {
                                            let other_centre = other_wall.centre();
                                            let other_along = if other_wall.is_horizontal() {
                                                Vec2::X
                                            } else {
                                                Vec2::Y
                                            };
                                            (Vec2::new(solid.centre.x, solid.centre.z)
                                                - other_centre)
                                                .dot(other_along)
                                                .abs()
                                                <= crate::CELL_SIZE_METRES * 0.5 + 0.01
                                                && (Vec2::new(solid.centre.x, solid.centre.z)
                                                    - other_centre)
                                                    .dot(direction_vector(other_wall.direction))
                                                    .abs()
                                                    <= 0.1
                                        })
                                }) && solid.centre.y - solid.size.y * 0.5 >= source_bottom - 0.01
                                    && solid.centre.y + solid.size.y * 0.5 <= source_top + 0.01
                        })
                });
                let sampled_cover = [-0.4_f32, 0.0, 0.4].into_iter().all(|along_sample| {
                    [0.15_f32, 0.5, 0.85].into_iter().all(|height_sample| {
                        let plan_point = centre + along * crate::CELL_SIZE_METRES * along_sample;
                        let outside_replacement_run = match defense.path {
                            ProjectedDefensePath::Linear { start, end, .. } => {
                                let run = end - start;
                                let run_length = run.length();
                                let run_tangent = run.normalize_or_zero();
                                let offset = plan_point - start;
                                let projected = offset.dot(run_tangent);
                                projected < -0.01 || projected > run_length + 0.01
                            }
                            ProjectedDefensePath::Round {
                                centre,
                                radius_metres,
                                ..
                            } => (plan_point - centre).dot(along).abs() > radius_metres + 0.01,
                        };
                        if outside_replacement_run {
                            return true;
                        }
                        let point = Vec3::new(
                            plan_point.x,
                            source_bottom + plan.storey_height_metres * height_sample,
                            plan_point.y,
                        );
                        defense.host_wall_solids.iter().any(|id| {
                            host_solids
                                .iter()
                                .find(|solid| solid.id == *id)
                                .is_some_and(|solid| {
                                    resolved_solid_contains_point(solid, point, 0.012)
                                })
                        }) || plan.resolved_geometry.voids.iter().any(|void| {
                            void.owner == defense.host_owner
                                && point.x >= void.bounds.min.x - 0.01
                                && point.x <= void.bounds.max.x + 0.01
                                && point.y >= void.bounds.min.y - 0.01
                                && point.y <= void.bounds.max.y + 0.01
                                && point.z >= void.bounds.min.z - 0.01
                                && point.z <= void.bounds.max.z + 0.01
                        })
                    })
                });
                solids_inside && sampled_cover
            });
        let host_solids_do_not_duplicate =
            defense
                .host_wall_solids
                .iter()
                .enumerate()
                .all(|(index, left)| {
                    defense
                        .host_wall_solids
                        .iter()
                        .skip(index + 1)
                        .all(|right| {
                            host_solids
                                .iter()
                                .find(|solid| solid.id == *left)
                                .zip(host_solids.iter().find(|solid| solid.id == *right))
                                .is_some_and(|(left, right)| {
                                    !resolved_solids_overlap_positive_volume(left, right, 0.002)
                                })
                        })
                });
        let host_roof_clear = defense.host_wall_solids.iter().all(|host_id| {
            host_solids
                .iter()
                .find(|solid| solid.id == *host_id)
                .is_some_and(|host| {
                    plan.resolved_geometry
                        .solids
                        .iter()
                        .filter(|solid| {
                            solid.owner == defense.owner && solid.role == SolidRole::DefenseRoof
                        })
                        .all(|roof| !resolved_solids_overlap_positive_volume(host, roof, 0.002))
                })
        });
        let topology_is_supported = match defense.host_topology {
            crate::ProjectedDefenseHostTopology::LinearFace => {
                defense.kind != ProjectedDefenseKind::Bartizan
                    && defense.host_buttress_solids.is_empty()
            }
            crate::ProjectedDefenseHostTopology::CornerFaces => {
                defense.kind == ProjectedDefenseKind::Bartizan
                    && defense.host_source_walls.len() >= 2
            }
            crate::ProjectedDefenseHostTopology::Buttress => {
                defense.kind == ProjectedDefenseKind::Bartizan
                    && !defense.host_buttress_solids.is_empty()
                    && defense.host_buttress_solids.iter().all(|id| {
                        host_solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|solid| {
                                solid.role == SolidRole::DefenseHostButtress
                                    && solid.centre.y - solid.size.y * 0.5 <= 0.01
                                    && defense.host_wall_solids.iter().any(|wall_id| {
                                        host_solids
                                            .iter()
                                            .find(|wall| wall.id == *wall_id)
                                            .is_some_and(|wall| {
                                                resolved_solids_overlap_positive_volume(
                                                    solid, wall, -0.015,
                                                )
                                            })
                                    })
                            })
                    })
            }
        };
        if !host_is_authoritative
            || !host_portal_is_cut
            || !host_bond_is_physical
            || !source_walls_are_exact
            || !host_solids_do_not_duplicate
            || !host_roof_clear
            || !topology_is_supported
        {
            issues.push(issue(
                "unresolved_projected_defense_host",
                format!(
                    "projected defense owner {} is not bonded to a cut authoritative wall/walk host (authority={host_is_authoritative}, portal={host_portal_is_cut}, bond={host_bond_is_physical}, envelope={source_walls_are_exact}, disjoint={host_solids_do_not_duplicate}, roof_clear={host_roof_clear}, topology={topology_is_supported})",
                    defense.owner.0,
                ),
            ));
        }
        if defense.kind == ProjectedDefenseKind::Hoarding {
            let sockets_are_host_voids = !defense.beam_socket_voids.is_empty()
                && defense.beam_socket_voids.iter().all(|id| {
                    plan.resolved_geometry.voids.iter().any(|void| {
                        void.id == *id
                            && void.owner == defense.host_owner
                            && void.role == VoidRole::BeamSocket
                    })
                });
            let deployment_matches_sockets = match defense.deployment {
                ProjectedDefenseDeployment::SocketsOnly => defense.socket_joists.is_empty(),
                ProjectedDefenseDeployment::Deployed => {
                    defense.socket_joists.len() == defense.beam_socket_voids.len()
                        && defense.socket_joists.iter().all(|(socket_id, joist_id)| {
                            let socket = plan
                                .resolved_geometry
                                .voids
                                .iter()
                                .find(|void| void.id == *socket_id);
                            let joist = solids.iter().find(|solid| solid.id == *joist_id);
                            socket.zip(joist).is_some_and(|(socket, joist)| {
                                joist.role == SolidRole::BeamJoist
                                    && resolved_solid_overlaps_bounds(
                                        joist,
                                        (socket.bounds.min, socket.bounds.max),
                                        0.01,
                                    )
                            })
                        })
                }
                ProjectedDefenseDeployment::Permanent => false,
            };
            if !sockets_are_host_voids || !deployment_matches_sockets {
                issues.push(issue(
                    "invalid_hoarding_beam_sockets",
                    format!(
                        "hoarding owner {} does not use host-cut sockets occupied by state-linked joists",
                        defense.owner.0
                    ),
                ));
            }
        }
        let placement_faces_outward = match defense.path {
            ProjectedDefensePath::Linear { start, end, .. } => {
                let midpoint = (start + end) * 0.5;
                let floor_centroid = defense
                    .floor_solids
                    .iter()
                    .filter_map(|id| solids.iter().find(|solid| solid.id == *id))
                    .map(|solid| Vec2::new(solid.centre.x, solid.centre.z))
                    .reduce(|left, right| left + right)
                    .map(|sum| sum / defense.floor_solids.len().max(1) as f32);
                defense.deployment == ProjectedDefenseDeployment::SocketsOnly
                    || floor_centroid
                        .is_some_and(|centroid| (centroid - midpoint).dot(outward) > 0.08)
            }
            ProjectedDefensePath::Round { centre, .. } => {
                let plan_centre = plan.dimensions_metres() * 0.5;
                (centre - plan_centre).dot(outward) > 0.1
            }
        };
        if !placement_faces_outward {
            issues.push(issue(
                "inward_projected_defense",
                format!(
                    "projected defense owner {} is oriented away from its physical projection",
                    defense.owner.0
                ),
            ));
        }
        if defense.deployment == ProjectedDefenseDeployment::SocketsOnly {
            if !defense.floor_solids.is_empty()
                || !defense.throat_voids.is_empty()
                || defense.access_portal.is_some()
                || defense.access_landing.is_some()
                || defense.beam_socket_voids.is_empty()
                || !defense.socket_joists.is_empty()
                || defense.beam_socket_voids.iter().any(|id| {
                    !plan.resolved_geometry.voids.iter().any(|void| {
                        void.id == *id
                            && void.owner == defense.host_owner
                            && void.role == VoidRole::BeamSocket
                    })
                })
            {
                issues.push(issue(
                    "invalid_hoarding_deployment_state",
                    format!(
                        "socket-only hoarding owner {} contains deployed gallery work",
                        defense.owner.0
                    ),
                ));
            }
            continue;
        }
        if defense.clear_width_metres < 0.9
            || defense.clear_height_metres < 1.9
            || defense.breastwork_height_metres < 0.9
            || (defense.material == ProjectedDefenseMaterial::Timber
                && defense.projection_metres > 1.2)
        {
            issues.push(issue(
                "insufficient_projected_defense_clearance",
                format!(
                    "projected defense owner {} violates walk, headroom, cover, or cantilever gates",
                    defense.owner.0
                ),
            ));
        }
        let has_floor = !defense.floor_solids.is_empty()
            && defense.floor_solids.iter().all(|id| {
                solids.iter().any(|solid| {
                    solid.id == *id
                        && matches!(solid.role, SolidRole::GalleryFloor | SolidRole::Landing)
                })
            });
        let has_portal = defense.access_portal.is_some_and(|id| {
            plan.resolved_geometry.voids.iter().any(|void| {
                let size = void.bounds.max - void.bounds.min;
                void.id == id
                    && void.owner == defense.host_owner
                    && void.role == VoidRole::AccessPortal
                    && size.x.max(size.z) >= 0.75
                    && size.y >= 1.9
            })
        });
        let has_landing = defense.access_landing.is_some_and(|id| {
            solids
                .iter()
                .any(|solid| solid.id == id && solid.role == SolidRole::Landing)
        });
        let landing_overlaps_floor = defense.access_landing.is_some_and(|landing_id| {
            solids
                .iter()
                .find(|solid| solid.id == landing_id)
                .is_some_and(|landing| {
                    defense.floor_solids.iter().any(|floor_id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *floor_id)
                            .is_some_and(|floor| {
                                bounds_overlap_3d(
                                    resolved_solid_bounds(landing),
                                    resolved_solid_bounds(floor),
                                    0.01,
                                )
                            })
                    })
                })
        });
        let landing_overlaps_host_walk = defense.access_landing.is_some_and(|landing_id| {
            let landing = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == landing_id);
            let host_walk = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == defense.host_walk_solid);
            landing.zip(host_walk).is_some_and(|(landing, walk)| {
                resolved_solids_overlap_positive_volume(landing, walk, 0.004)
            })
        });
        if !has_floor
            || !has_portal
            || !has_landing
            || !landing_overlaps_floor
            || !landing_overlaps_host_walk
        {
            issues.push(issue(
                "inaccessible_projected_defense",
                format!(
                    "projected defense owner {} lacks a physical floor, portal, or landing",
                    defense.owner.0
                ),
            ));
        }
        let throats_valid = !defense.throat_voids.is_empty()
            && defense.throat_voids.iter().all(|id| {
                voids
                    .iter()
                    .any(|void| void.id == *id && void.role == VoidRole::DefenseThroat)
                    && plan
                        .resolved_geometry
                        .projected_defense_rays
                        .iter()
                        .any(|ray| ray.owner == defense.owner && ray.throat == *id)
            });
        if !throats_valid {
            issues.push(issue(
                "sealed_projected_defense_throat",
                format!(
                    "projected defense owner {} lacks open downward-defense throats and rays",
                    defense.owner.0
                ),
            ));
        }
        let working_points = plan
            .resolved_geometry
            .projected_defense_working_points
            .iter()
            .filter(|point| point.owner == defense.owner)
            .collect::<Vec<_>>();
        let working_points_valid = !working_points.is_empty()
            && working_points.iter().all(|point| {
                let support = solids.iter().find(|solid| solid.id == point.support_solid);
                let support_valid = support.is_some_and(|solid| {
                    matches!(solid.role, SolidRole::GalleryFloor | SolidRole::Landing)
                        && resolved_solid_contains_point(solid, point.stance - Vec3::Y * 0.02, 0.08)
                        && point.stance.y + 0.03 >= defense.floor_elevation_metres
                });
                let ranges = plan
                    .resolved_geometry
                    .projected_defense_rays
                    .iter()
                    .filter(|ray| ray.owner == defense.owner && ray.throat == point.aperture)
                    .map(|ray| ray.range)
                    .collect::<std::collections::HashSet<_>>();
                let aperture = plan
                    .resolved_geometry
                    .voids
                    .iter()
                    .find(|void| void.id == point.aperture);
                support_valid
                    && aperture.is_some()
                    && ranges
                        == std::collections::HashSet::from([
                            crate::ProjectedDefenseRange::Near,
                            crate::ProjectedDefenseRange::Middle,
                            crate::ProjectedDefenseRange::Far,
                        ])
            });
        if !working_points_valid {
            issues.push(issue(
                "inoperable_projected_defense_station",
                format!(
                    "projected defense owner {} lacks supported near/mid/far working positions",
                    defense.owner.0
                ),
            ));
        }
        for ray in plan
            .resolved_geometry
            .projected_defense_rays
            .iter()
            .filter(|ray| ray.owner == defense.owner)
        {
            let delta = ray.target - ray.origin;
            let outward_progress = Vec2::new(delta.x, delta.z).dot(outward);
            let is_downward_throat = plan
                .resolved_geometry
                .voids
                .iter()
                .any(|void| void.id == ray.throat && void.role == VoidRole::DefenseThroat);
            let aims_down_and_out = if is_downward_throat {
                delta.y < -1.0 && outward_progress > 0.5
            } else {
                delta.y < 0.0 && outward_progress > 0.5
            };
            let blocked = (1..20).any(|sample| {
                let point = ray.origin.lerp(ray.target, sample as f32 / 20.0);
                plan.resolved_geometry.solids.iter().any(|solid| {
                    !(matches!(
                        solid.shape,
                        crate::ResolvedSolidShape::RoundTowerShell { .. }
                    ) && segment_is_inside_tower_chord_void(plan, solid, ray.origin, ray.target))
                        && resolved_solid_contains_point(solid, point, -0.015)
                })
            });
            let below_floor_origin = ray.origin.y < defense.floor_elevation_metres - 0.001;
            let crosses_friendly_route = (1..20).any(|sample| {
                let point = ray.origin.lerp(ray.target, sample as f32 / 20.0);
                plan.resolved_geometry.solids.iter().any(|solid| {
                    matches!(solid.role, SolidRole::CircuitWalk | SolidRole::Landing)
                        && resolved_solid_contains_point(solid, point, -0.015)
                })
            });
            if !aims_down_and_out || blocked || below_floor_origin || crosses_friendly_route {
                issues.push(issue(
                    "blocked_projected_defense_ray",
                    format!(
                        "projected defense owner {} has a blocked, inward, or misaligned throat ray {:?}->{:?} blocked={blocked}",
                        defense.owner.0, ray.origin, ray.target
                    ),
                ));
                break;
            }
        }
        let support_nodes = defense
            .support_nodes
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .structural_nodes
                    .iter()
                    .find(|node| node.id == *id)
            })
            .collect::<Vec<_>>();
        let support_valid = support_nodes.len() == defense.support_nodes.len()
            && !support_nodes.is_empty()
            && support_nodes.iter().all(|node| {
                matches!(
                    node.kind,
                    crate::StructuralNodeKind::ProjectionCorbel
                        | crate::StructuralNodeKind::GalleryFrame
                ) && !node.supported_by.is_empty()
                    && plan
                        .resolved_geometry
                        .support_interfaces
                        .iter()
                        .any(|bearing| {
                            let size = bearing.bounds.max - bearing.bounds.min;
                            bearing.owner == defense.owner
                                && bearing.node == node.id
                                && size.x * size.z >= 0.08
                        })
            });
        let support_tangent = match defense.path {
            ProjectedDefensePath::Linear { start, end, .. } => (end - start).normalize_or_zero(),
            ProjectedDefensePath::Round { outward, .. } => {
                let radial = direction_vector(outward);
                Vec2::new(-radial.y, radial.x)
            }
        };
        let floor_supported_at_spacing = defense.floor_solids.iter().all(|floor_id| {
            solids
                .iter()
                .find(|solid| solid.id == *floor_id)
                .is_some_and(|floor| {
                    [-0.5_f32, 0.0, 0.5].into_iter().all(|sample| {
                        let local_x = Vec2::new(floor.yaw_radians.cos(), -floor.yaw_radians.sin());
                        let point = Vec2::new(floor.centre.x, floor.centre.z)
                            + local_x * floor.size.x * sample;
                        support_nodes.iter().any(|node| {
                            let support = Vec2::new(node.position.x, node.position.z);
                            (point - support).dot(support_tangent).abs() <= 0.75
                        })
                    })
                })
        });
        if !support_valid || !floor_supported_at_spacing {
            issues.push(issue(
                "unsupported_projected_defense",
                format!(
                    "projected defense owner {} lacks a grounded corbel or frame support graph",
                    defense.owner.0
                ),
            ));
        }
        let drain_valid = defense.drain_route.is_some_and(|id| {
            plan.resolved_geometry
                .drainage_routes
                .iter()
                .find(|route| route.id == id && route.owner == defense.owner)
                .is_some_and(|route| {
                    route.outlet.y < route.inlet.y - 0.04
                        && !defense.throat_voids.contains(&route.outlet_void)
                })
        });
        let catchments = defense
            .drainage_catchments
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == *id && catchment.owner == defense.owner)
            })
            .collect::<Vec<_>>();
        let physical_catchments = catchments.len() == defense.drainage_catchments.len()
            && !catchments.is_empty()
            && catchments.iter().all(|catchment| {
                let channels = catchment
                    .toe_channel_solids
                    .iter()
                    .filter_map(|id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *id && solid.role == SolidRole::DrainageFloor)
                            .copied()
                    })
                    .collect::<Vec<_>>();
                let route = plan
                    .resolved_geometry
                    .drainage_routes
                    .iter()
                    .find(|route| route.id == catchment.outlet_route);
                let channel_chain_valid = channels.len() == catchment.toe_channel_solids.len()
                    && !channels.is_empty()
                    && route.is_some_and(|route| {
                        channels.last().is_some_and(|channel| {
                            let local_x =
                                Vec2::new(channel.yaw_radians.cos(), -channel.yaw_radians.sin());
                            let downhill = local_x * -channel.longfall_radians.signum();
                            let endpoint = Vec2::new(channel.centre.x, channel.centre.z)
                                + downhill * channel.size.x * 0.5;
                            endpoint.distance(Vec2::new(route.inlet.x, route.inlet.z)) <= 0.015
                                && channel.longfall_radians.abs() >= 0.005
                                && channel.centre.y + channel.size.y * 0.5
                                    <= defense.floor_elevation_metres - 0.015
                        })
                    });
                let floors_reach_channel = defense.floor_solids.iter().all(|floor_id| {
                    let Some(floor) = solids.iter().find(|solid| solid.id == *floor_id) else {
                        return false;
                    };
                    let local_x = Vec2::new(floor.yaw_radians.cos(), -floor.yaw_radians.sin());
                    let local_z = Vec2::new(floor.yaw_radians.sin(), floor.yaw_radians.cos());
                    let gradient =
                        local_x * -floor.longfall_radians.signum() * floor.longfall_radians.abs()
                            + local_z
                                * floor.crossfall_radians.signum()
                                * floor.crossfall_radians.abs();
                    if gradient.length() < 0.005 {
                        return false;
                    }
                    let downhill = gradient.normalize();
                    [-0.4_f32, 0.0, 0.4].into_iter().all(|x| {
                        [-0.4_f32, 0.0, 0.4].into_iter().all(|z| {
                            let start = Vec2::new(floor.centre.x, floor.centre.z)
                                + local_x * floor.size.x * x
                                + local_z * floor.size.z * z;
                            (0..=100).any(|step| {
                                let point = start + downhill * step as f32 * 0.04;
                                channels.iter().any(|channel| {
                                    resolved_solid_contains_point(
                                        channel,
                                        Vec3::new(point.x, channel.centre.y, point.y),
                                        0.025,
                                    )
                                }) || route.is_some_and(|route| {
                                    plan.resolved_geometry.voids.iter().any(|void| {
                                        void.id == route.outlet_void
                                            && point.x >= void.bounds.min.x - 0.025
                                            && point.x <= void.bounds.max.x + 0.025
                                            && point.y >= void.bounds.min.z - 0.025
                                            && point.y <= void.bounds.max.z + 0.025
                                    })
                                })
                            })
                        })
                    })
                });
                let floors_and_channels_disjoint = defense.floor_solids.iter().all(|floor_id| {
                    solids
                        .iter()
                        .find(|solid| solid.id == *floor_id)
                        .is_some_and(|floor| {
                            channels.iter().all(|channel| {
                                !resolved_solids_overlap_positive_volume(floor, channel, 0.004)
                            })
                        })
                });
                channel_chain_valid && floors_reach_channel && floors_and_channels_disjoint
            });
        let weather_catchments = defense
            .weather_catchments
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == *id && catchment.owner == defense.owner)
            })
            .collect::<Vec<_>>();
        let weather_solids_exist = !defense.weathering_solids.is_empty()
            && defense.weathering_solids.iter().all(|id| {
                solids.iter().any(|solid| {
                    solid.id == *id
                        && matches!(
                            solid.role,
                            SolidRole::DefenseRoof
                                | SolidRole::Coping
                                | SolidRole::DrainageFloor
                                | SolidRole::RoofFlashing
                        )
                })
            });
        let weather_drains_physically = weather_catchments.len()
            == defense.weather_catchments.len()
            && !weather_catchments.is_empty()
            && weather_catchments.iter().all(|catchment| {
                let Some(source) = solids.iter().find(|solid| solid.id == catchment.walk_solid)
                else {
                    return false;
                };
                let Some(route) = plan.resolved_geometry.drainage_routes.iter().find(|route| {
                    route.id == catchment.outlet_route && route.owner == defense.owner
                }) else {
                    return false;
                };
                let local_z = Vec2::new(source.yaw_radians.sin(), source.yaw_radians.cos());
                let physical_downhill = local_z * source.crossfall_radians.signum();
                let weather_outward = match defense.path {
                    ProjectedDefensePath::Round { centre, .. }
                        if source.role == SolidRole::Coping =>
                    {
                        (Vec2::new(source.centre.x, source.centre.z) - centre).normalize_or_zero()
                    }
                    _ => outward,
                };
                let gradient_outward = source.crossfall_radians.abs() >= 0.04
                    && physical_downhill.dot(weather_outward) >= 0.8
                    && catchment.outward.dot(weather_outward) >= 0.8
                    && catchment.inner_elevation_metres > catchment.outer_elevation_metres + 0.01;
                let route_is_open_drip =
                    route.outlet.y < route.inlet.y - 0.04
                        && !defense.throat_voids.contains(&route.outlet_void)
                        && plan.resolved_geometry.voids.iter().any(|void| {
                            void.id == route.outlet_void && void.role == VoidRole::Drain
                        });
                let toe_reaches_inlet = if catchment.toe_channel_solids.is_empty() {
                    let source_centre = Vec2::new(source.centre.x, source.centre.z);
                    let expected = source_centre
                        + catchment.outward * catchment.width_metres * 0.5
                        + catchment.tangent * catchment.outlet_along_metres;
                    expected.distance(Vec2::new(route.inlet.x, route.inlet.z)) <= 0.22
                } else {
                    catchment.toe_channel_solids.iter().all(|id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|channel| {
                                channel.role == SolidRole::DrainageFloor
                                    && channel.longfall_radians.abs() >= 0.005
                                    && channel.centre.y + channel.size.y * 0.5
                                        <= catchment.outer_elevation_metres + 0.005
                            })
                    }) && catchment.toe_channel_solids.last().is_some_and(|id| {
                        solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|channel| {
                                let local_x = Vec2::new(
                                    channel.yaw_radians.cos(),
                                    -channel.yaw_radians.sin(),
                                );
                                let downhill = local_x * -channel.longfall_radians.signum();
                                let endpoint = Vec2::new(channel.centre.x, channel.centre.z)
                                    + downhill * channel.size.x * 0.5;
                                endpoint.distance(Vec2::new(route.inlet.x, route.inlet.z)) <= 0.02
                            })
                    })
                };
                gradient_outward && route_is_open_drip && toe_reaches_inlet
            });
        let roof_or_coping_contract = if defense.roofed {
            solids.iter().any(|solid| {
                solid.role == SolidRole::DefenseRoof
                    && defense.weathering_solids.contains(&solid.id)
            }) && solids.iter().any(|solid| {
                solid.role == SolidRole::RoofFlashing
                    && defense.weathering_solids.contains(&solid.id)
            })
        } else if defense.material == ProjectedDefenseMaterial::Masonry {
            solids.iter().any(|solid| {
                solid.role == SolidRole::Coping && defense.weathering_solids.contains(&solid.id)
            })
        } else {
            true
        };
        let roof_bearing_contract = if defense.kind == ProjectedDefenseKind::Breteche {
            let roof = solids
                .iter()
                .copied()
                .find(|solid| solid.role == SolidRole::DefenseRoof);
            let bearing_node = defense.roof_bearing_node.and_then(|id| {
                plan.resolved_geometry
                    .structural_nodes
                    .iter()
                    .find(|node| node.id == id && node.owner == defense.owner)
            });
            let support_solids = defense
                .roof_support_solids
                .iter()
                .filter_map(|id| solids.iter().copied().find(|solid| solid.id == *id))
                .collect::<Vec<_>>();
            let plates = support_solids
                .iter()
                .copied()
                .filter(|solid| solid.role == SolidRole::RoofPlate)
                .collect::<Vec<_>>();
            roof.is_some_and(|roof| {
                bearing_node.is_some_and(|node| {
                    roof.supported_by == [node.id]
                        && node.supported_by.len() == 2
                        && node.supported_by.iter().all(|parent| {
                            plan.resolved_geometry
                                .structural_nodes
                                .iter()
                                .any(|candidate| {
                                    candidate.id == *parent
                                        && candidate.owner == defense.owner
                                        && !candidate.supported_by.is_empty()
                                })
                        })
                }) && support_solids.len() == defense.roof_support_solids.len()
                    && support_solids.len() >= 5
                    && plates.len() == 2
                    && plates.iter().all(|plate| {
                        let expected_underside = roof.centre.y
                            - (Vec2::new(plate.centre.x, plate.centre.z)
                                - Vec2::new(roof.centre.x, roof.centre.z))
                            .dot(outward)
                                * roof.crossfall_radians.abs().tan()
                            - roof.size.y * 0.5;
                        let plate_top = plate.centre.y + plate.size.y * 0.5;
                        let roof_contact = (plate_top - expected_underside).abs() <= 0.025
                            && resolved_plan_overlap_area(roof, plate) >= 0.08;
                        let local_x = Vec2::new(plate.yaw_radians.cos(), -plate.yaw_radians.sin());
                        let bearing_samples = [-1.0_f32, 1.0].into_iter().all(|side| {
                            let point = Vec2::new(plate.centre.x, plate.centre.z)
                                + local_x * side * (plate.size.x * 0.5 - 0.47);
                            support_solids.iter().any(|support| {
                                support.id != plate.id
                                    && support.role != SolidRole::RoofPlate
                                    && (support.centre.y + support.size.y * 0.5
                                        - (plate.centre.y - plate.size.y * 0.5))
                                        .abs()
                                        <= 0.025
                                    && resolved_plan_overlap_area(support, plate) >= 0.014
                                    && resolved_solid_contains_point(
                                        support,
                                        Vec3::new(point.x, support.centre.y, point.y),
                                        0.12,
                                    )
                            })
                        });
                        roof_contact && bearing_samples
                    })
            })
        } else {
            defense.roof_support_solids.is_empty() && defense.roof_bearing_node.is_none()
        };
        if !drain_valid
            || !physical_catchments
            || !weather_solids_exist
            || !weather_drains_physically
            || !roof_or_coping_contract
        {
            issues.push(issue(
                "projected_defense_roof_drain_failure",
                format!(
                    "projected defense owner {} lacks independent roof/floor drainage route={drain_valid} catchment={physical_catchments} weather_solids={weather_solids_exist} weather_flow={weather_drains_physically} roof_or_coping={roof_or_coping_contract}",
                    defense.owner.0,
                ),
            ));
        }
        if !roof_bearing_contract {
            issues.push(issue(
                "unsupported_projected_defense_roof",
                format!(
                    "projected defense owner {} roof lacks two physically touching wall-plate load regions and a grounded bearing DAG",
                    defense.owner.0,
                ),
            ));
        }
        if defense.kind == ProjectedDefenseKind::Bartizan {
            let (centre, radius) = match defense.path {
                ProjectedDefensePath::Round {
                    centre,
                    radius_metres,
                    ..
                } => (centre, radius_metres),
                ProjectedDefensePath::Linear { .. } => unreachable!(),
            };
            let interior = Vec3::new(
                centre.x,
                defense.floor_elevation_metres + defense.clear_height_metres * 0.5,
                centre.y,
            );
            let floor_covers_usable_volume =
                [0.2_f32, 0.45, 0.65].into_iter().all(|radius_factor| {
                    (0..16).all(|segment| {
                        let angle = segment as f32 * std::f32::consts::TAU / 16.0;
                        let point =
                            centre + Vec2::new(angle.cos(), angle.sin()) * radius * radius_factor;
                        let in_throat = defense.throat_voids.iter().any(|id| {
                            plan.resolved_geometry.voids.iter().any(|void| {
                                void.id == *id
                                    && point.x >= void.bounds.min.x
                                    && point.x <= void.bounds.max.x
                                    && point.y >= void.bounds.min.z
                                    && point.y <= void.bounds.max.z
                            })
                        });
                        in_throat
                            || defense.floor_solids.iter().any(|id| {
                                solids
                                    .iter()
                                    .find(|solid| solid.id == *id)
                                    .is_some_and(|solid| {
                                        resolved_solid_contains_point(
                                            solid,
                                            Vec3::new(
                                                point.x,
                                                defense.floor_elevation_metres - 0.03,
                                                point.y,
                                            ),
                                            0.035,
                                        )
                                    })
                            })
                    })
                });
            let loops_are_narrow_split_openings = defense.firing_apertures.iter().all(|id| {
                plan.resolved_geometry
                    .voids
                    .iter()
                    .find(|void| void.id == *id)
                    .is_some_and(|void| {
                        let size = void.bounds.max - void.bounds.min;
                        size.x.max(size.z) <= 0.2
                            && size.y <= 0.55
                            && solids
                                .iter()
                                .filter(|solid| solid.role == SolidRole::BartizanShell)
                                .filter(|solid| {
                                    Vec2::new(solid.centre.x, solid.centre.z).distance(centre)
                                        <= radius + 0.15
                                })
                                .count()
                                >= 12
                    })
            });
            if solids
                .iter()
                .any(|solid| resolved_solid_contains_point(solid, interior, 0.0))
                || !solids
                    .iter()
                    .any(|solid| solid.role == SolidRole::BartizanShell)
                || defense.firing_apertures.is_empty()
                || !floor_covers_usable_volume
                || !loops_are_narrow_split_openings
            {
                issues.push(issue(
                    "closed_bartizan",
                    format!(
                        "bartizan owner {} is not a hollow usable firing volume",
                        defense.owner.0
                    ),
                ));
            }
        }
        if defense.material == ProjectedDefenseMaterial::Timber
            && solids
                .iter()
                .filter(|solid| solid.role == SolidRole::FrameMember)
                .any(|member| {
                    member.supported_by.iter().any(|id| {
                        !plan
                            .resolved_geometry
                            .structural_nodes
                            .iter()
                            .any(|node| node.id == *id)
                    })
                })
        {
            issues.push(issue(
                "dangling_hoarding_frame",
                format!(
                    "hoarding owner {} has a dangling frame member",
                    defense.owner.0
                ),
            ));
        }
    }
}
