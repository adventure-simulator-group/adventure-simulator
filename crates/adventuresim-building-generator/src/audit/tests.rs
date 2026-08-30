#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_audit_rejects_open_and_inconsistently_wound_geometry() {
        let tetrahedron = [
            [1.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, -1.0],
            [1.0, -1.0, -1.0],
        ];
        let closed = audit_triangle_mesh(&tetrahedron, &[0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3]);
        assert!(closed.passes_closed_solid(), "{closed:?}");

        let inverted = audit_triangle_mesh(&tetrahedron, &[0, 1, 2, 0, 3, 1, 0, 2, 3, 1, 3, 2]);
        assert!(inverted.inverted_winding);

        let open = audit_triangle_mesh(
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            &[0, 1, 2, 0, 2, 3],
        );
        assert_eq!(open.boundary_edges, 4);

        let bad_winding = audit_triangle_mesh(&tetrahedron, &[0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 3, 2]);
        assert!(bad_winding.inconsistent_winding_edges > 0);
    }

    #[test]
    fn every_curated_plan_passes_the_structural_audit() {
        for archetype in crate::BuildingArchetype::ALL {
            let plan = crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap();
            assert!(
                audit_plan(&plan).is_empty(),
                "{archetype:?}: {:?}",
                audit_plan(&plan)
            );
        }
    }

    #[test]
    fn timber_roof_members_stay_within_the_authoritative_roof_envelope() {
        let mut failures = Vec::new();
        for seed in [42, 47, 101] {
            for archetype in crate::BuildingArchetype::ALL {
                let plan =
                    crate::generate(&crate::BuildingProgram::fixture(archetype, seed)).unwrap();
                let intrusions = timber_roof_envelope_intrusions(&plan);
                if !intrusions.is_empty() {
                    let members = plan
                        .timber_frame
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .filter(|member| intrusions.contains(&member.id))
                        .map(|member| (member.id, member.role, member.start, member.end))
                        .collect::<Vec<_>>();
                    failures.push((seed, archetype, members));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "roof-construction members protrude through authoritative roof faces: {failures:?}"
        );
    }

    #[test]
    fn timber_openings_are_recessed_and_dormers_use_jamb_supported_gable_frames() {
        let mut coplanar = Vec::new();
        let mut invalid_dormers = Vec::new();
        let mut exposed_child_supports = Vec::new();
        let mut oversized_child_flashings = Vec::new();
        let mut invalid_child_curbs = Vec::new();
        let mut oversized_child_gutters = Vec::new();
        let mut undeclared_cross_authority = Vec::new();
        let mut full_audit_failures = Vec::new();
        for seed in [42, 47, 101] {
            for archetype in crate::BuildingArchetype::ALL {
                let plan =
                    crate::generate(&crate::BuildingProgram::fixture(archetype, seed)).unwrap();
                let issues = audit_plan(&plan);
                if !issues.is_empty() {
                    full_audit_failures.push((seed, archetype, issues));
                }
                let conflicts = coplanar_timber_opening_faces(&plan);
                if !conflicts.is_empty() {
                    coplanar.push((seed, archetype, conflicts));
                }
                let exposed = exposed_roof_child_support_posts(&plan);
                if !exposed.is_empty() {
                    exposed_child_supports.push((seed, archetype, exposed));
                }
                let oversized = oversized_child_roof_flashings(&plan);
                if !oversized.is_empty() {
                    oversized_child_flashings.push((seed, archetype, oversized));
                }
                let invalid_curb = invalid_dormer_trimmer_envelope(&plan);
                if !invalid_curb.is_empty() {
                    invalid_child_curbs.push((seed, archetype, invalid_curb));
                }
                let oversized_gutters = oversized_attached_child_gutters(&plan);
                if !oversized_gutters.is_empty() {
                    oversized_child_gutters.push((seed, archetype, oversized_gutters));
                }
                let undeclared = undeclared_timber_intersections(&plan);
                if !undeclared.is_empty() {
                    let described = undeclared
                        .into_iter()
                        .map(|(left, right)| {
                            [left, right].map(|id| {
                                plan.resolved_geometry
                                    .solids
                                    .iter()
                                    .find(|solid| solid.id == id)
                                    .map(|solid| (id, solid.role, solid.centre, solid.size))
                            })
                        })
                        .collect::<Vec<_>>();
                    undeclared_cross_authority.push((seed, archetype, described));
                }
                if let Some(frame) = &plan.timber_frame {
                    let members = frame
                        .members
                        .iter()
                        .map(|member| (member.id, member))
                        .collect::<std::collections::HashMap<_, _>>();
                    for bay in &frame.bays {
                        let Some(wall_id) = bay.wall else { continue };
                        let Some(wall) =
                            plan.wall_assemblies.iter().find(|wall| wall.id == wall_id)
                        else {
                            continue;
                        };
                        if !matches!(wall.source, crate::WallSourceId::RoofChildFront { .. }) {
                            continue;
                        }
                        let roles = bay
                            .member_ids
                            .iter()
                            .filter_map(|id| members.get(id).map(|member| member.role))
                            .collect::<Vec<_>>();
                        let roof_id = match wall.source {
                            crate::WallSourceId::RoofChildFront { roof } => roof,
                            _ => unreachable!(),
                        };
                        let is_shed = plan
                            .roof_assemblies
                            .iter()
                            .find(|roof| roof.id == roof_id)
                            .is_some_and(|roof| roof.kind == crate::RoofKind::Shed);
                        if roles.contains(&crate::TimberMemberRole::PrimaryPost)
                            || (!is_shed
                                && (!roles.contains(&crate::TimberMemberRole::GablePost)
                                    || roles
                                        .iter()
                                        .filter(|role| **role == crate::TimberMemberRole::Rafter)
                                        .count()
                                        < 2))
                        {
                            invalid_dormers.push((seed, archetype, bay.id));
                        }
                    }
                }
            }
        }
        assert!(
            coplanar.is_empty(),
            "coplanar timber/opening faces: {coplanar:?}"
        );
        assert!(
            invalid_dormers.is_empty(),
            "invalid dormer child-front frames: {invalid_dormers:?}"
        );
        assert!(
            exposed_child_supports.is_empty(),
            "roof children retain generic freestanding support posts: {exposed_child_supports:?}"
        );
        assert!(
            oversized_child_flashings.is_empty(),
            "child-roof flashing is rendered as a projecting bar: {oversized_child_flashings:?}"
        );
        assert!(
            invalid_child_curbs.is_empty(),
            "dormer trimmers project beyond their child enclosure: {invalid_child_curbs:?}"
        );
        assert!(
            oversized_child_gutters.is_empty(),
            "attached child roofs reuse full-building gutter profiles: {oversized_child_gutters:?}"
        );
        assert!(
            undeclared_cross_authority.is_empty(),
            "timber intersections fell outside the exact-pair whitelist: {undeclared_cross_authority:?}"
        );
        assert!(
            full_audit_failures.is_empty(),
            "the expanded three-seed/all-archetype audit exposed regressions: {full_audit_failures:?}"
        );

        let mut mutation = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let child = mutation
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap();
        let id = ResolvedItemId(0x7fff_ffff_ffff_ff01);
        mutation
            .resolved_geometry
            .solids
            .push(crate::ResolvedSolid {
                id,
                owner: child.owner,
                centre: Vec3::new(
                    child.faces[0].polygon[0].x,
                    8.5,
                    child.faces[0].polygon[0].z,
                ),
                size: Vec3::new(0.22, 2.9, 0.22),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: child.support_nodes.clone(),
            });
        assert!(
            audit_plan(&mutation)
                .iter()
                .any(|issue| issue.code == "exposed_roof_child_support"),
            "a reintroduced generic dormer-corner post must fail the production audit"
        );

        let mut thick_flashing = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let flashing_id = thick_flashing.roof_assemblies[0].children[0].flashing_ids[0];
        let flashing = thick_flashing
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == flashing_id)
            .unwrap();
        flashing.size.y = 0.07;
        flashing.size.z = 0.16;
        assert!(
            audit_plan(&thick_flashing)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_flashing_profile"),
            "the former oversized child-roof flashing bar must fail the production audit"
        );

        let mut child_downspout = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let child_owner = child_downspout
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap()
            .owner;
        let station_id = child_downspout
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find(|network| network.owner == child_owner)
            .unwrap()
            .outlet_station;
        child_downspout
            .resolved_geometry
            .roof_drainage_outlets
            .iter_mut()
            .find(|station| station.id == station_id)
            .unwrap()
            .disposition = crate::RoofDrainageDisposition::BoundDownspout;
        assert!(
            audit_plan(&child_downspout)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_drainage"),
            "an attached dormer must not regain a detached ground-height downspout"
        );

        let mut projecting_curb = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let trimmer_id = projecting_curb
            .timber_frame
            .as_ref()
            .unwrap()
            .dormer_trimmer_members[0];
        let trimmer = projecting_curb
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| member.id == trimmer_id)
            .unwrap();
        let outward = match projecting_curb.roof_dormers[0].facing {
            crate::Direction::North => Vec2::Y,
            crate::Direction::South => -Vec2::Y,
            crate::Direction::East => Vec2::X,
            crate::Direction::West => -Vec2::X,
        };
        trimmer.end.x += outward.x * projecting_curb.roof_dormers[0].depth_metres * 0.45;
        trimmer.end.z += outward.y * projecting_curb.roof_dormers[0].depth_metres * 0.45;
        assert!(
            audit_plan(&projecting_curb)
                .iter()
                .any(|issue| issue.code == "invalid_dormer_trimmer_envelope"),
            "the former outward-projecting dormer curb must fail the production audit"
        );

        let mut large_child_gutter = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let child_owner = large_child_gutter
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap()
            .owner;
        let gutter_id = large_child_gutter
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find(|network| network.owner == child_owner)
            .unwrap()
            .channel_floor;
        large_child_gutter
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == gutter_id)
            .unwrap()
            .size
            .z = 0.18;
        assert!(
            audit_plan(&large_child_gutter)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_drainage_profile"),
            "a full-size gutter reused on an attached dormer must fail the production audit"
        );

        let mut projecting_child_outlet = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let child_owner = projecting_child_outlet
            .roof_assemblies
            .iter()
            .find(|roof| roof.parent.is_some())
            .unwrap()
            .owner;
        let outlet_id = projecting_child_outlet
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find(|network| network.owner == child_owner)
            .unwrap()
            .outlet_void;
        let outlet = projecting_child_outlet
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == outlet_id)
            .unwrap();
        outlet.bounds.min += Vec3::new(0.40, 0.0, 0.40);
        outlet.bounds.max += Vec3::new(0.40, 0.0, 0.40);
        assert!(
            audit_plan(&projecting_child_outlet)
                .iter()
                .any(|issue| issue.code == "invalid_child_roof_drainage_profile"),
            "a child outlet projecting beyond its compact eave zone must fail the audit"
        );

        let mut free_rear_gable = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::RenaissanceTownHall,
            42,
        ))
        .unwrap();
        let gabled_child = free_rear_gable.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::GabledDormer)
            .unwrap()
            .child;
        for edge in &mut free_rear_gable
            .roof_assemblies
            .iter_mut()
            .find(|roof| roof.id == gabled_child)
            .unwrap()
            .edges
        {
            if edge.kind == crate::RoofEdgeKind::OpeningCut {
                edge.kind = crate::RoofEdgeKind::GableVerge;
            }
        }
        assert!(
            audit_plan(&free_rear_gable)
                .iter()
                .any(|issue| issue.code == "unseated_dormer_roof"),
            "a dormer restored to a free rear gable must fail the production audit"
        );

        let mut unlisted_pair = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::TownHouse,
            47,
        ))
        .unwrap();
        let rail_solid = unlisted_pair
            .timber_frame
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.role == crate::TimberMemberRole::Rail)
            .unwrap()
            .solid;
        let closure_centre = unlisted_pair
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.role == SolidRole::OpeningClosure)
            .unwrap()
            .centre;
        let rail = unlisted_pair
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == rail_solid)
            .unwrap();
        rail.centre = closure_centre;
        rail.size = Vec3::splat(0.30);
        assert!(
            !undeclared_timber_intersections(&unlisted_pair).is_empty(),
            "an arbitrary frame/closure collision must not enter the exact-pair whitelist"
        );
    }

    #[test]
    fn artillery_audit_rejects_trace_profile_station_route_and_bridge_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::ArtilleryRondelCastle,
                47,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut missing = fixture();
        missing.artillery_castle = None;
        assert!(has(&missing, "missing_artillery_castle"));
        let mut phase = fixture();
        phase.castle_phase = Some(crate::CastleConstructionPhase::InheritedMedieval);
        assert!(has(&phase, "invalid_artillery_trace"));
        let mut thin = fixture();
        thin.artillery_castle.as_mut().unwrap().curtains[0].total_depth =
            crate::GridLength::new(60).unwrap();
        assert!(has(&thin, "invalid_artillery_curtain"));
        let mut small = fixture();
        small.artillery_castle.as_mut().unwrap().rondels[0].diameter =
            crate::CellDiameter::new(6).unwrap();
        assert!(has(&small, "invalid_artillery_rondel"));
        let mut unbonded = fixture();
        let bond = unbonded.artillery_castle.as_ref().unwrap().rondels[0].curtain_bonds[0];
        unbonded
            .resolved_geometry
            .junction_bonds
            .retain(|item| item.id != bond);
        assert!(has(&unbonded, "invalid_artillery_rondel"));
        let mut no_earth = fixture();
        no_earth.artillery_castle.as_mut().unwrap().curtains[0]
            .earth_solids
            .clear();
        assert!(has(&no_earth, "invalid_artillery_curtain"));
        let mut token_earth = fixture();
        let ids = token_earth.artillery_castle.as_ref().unwrap().rondels[0]
            .earth_solids
            .clone();
        for id in ids.iter().skip(4) {
            token_earth
                .resolved_geometry
                .solids
                .retain(|solid| solid.id != *id);
        }
        for id in ids.iter().take(4) {
            token_earth
                .resolved_geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == *id)
                .unwrap()
                .shape = crate::ResolvedSolidShape::Cuboid;
        }
        assert!(has(&token_earth, "invalid_artillery_rondel"));
        let mut missing_earth_sector = fixture();
        let id = missing_earth_sector
            .artillery_castle
            .as_ref()
            .unwrap()
            .rondels[0]
            .earth_solids[5];
        missing_earth_sector
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != id);
        assert!(has(&missing_earth_sector, "invalid_artillery_rondel"));
        let mut recoil_intrusion = fixture();
        let mut sector = recoil_intrusion
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.role == SolidRole::ArtilleryEarthCore && solid.owner.0 == 60_000)
            .unwrap()
            .clone();
        sector.id = crate::ResolvedItemId(0x7fff_ffff_ffff_f001);
        sector.shape = crate::ResolvedSolidShape::AnnularSectorPrism {
            inner_radius_metres: 3.60,
            outer_radius_metres: 4.775,
            start_angle_radians: std::f32::consts::PI / 16.0,
            end_angle_radians: std::f32::consts::FRAC_PI_8,
            inner_top_offset_metres: 0.0,
            outer_top_offset_metres: 0.0,
        };
        recoil_intrusion.resolved_geometry.solids.push(sector);
        recoil_intrusion.artillery_castle.as_mut().unwrap().rondels[0]
            .earth_solids
            .push(crate::ResolvedItemId(0x7fff_ffff_ffff_f001));
        assert!(has(&recoil_intrusion, "invalid_artillery_rondel"));
        let mut no_rondel_cover = fixture();
        no_rondel_cover.artillery_castle.as_mut().unwrap().rondels[0]
            .parapet_solids
            .clear();
        assert!(has(&no_rondel_cover, "invalid_artillery_rondel"));
        let mut foot_level_cover = fixture();
        let id = foot_level_cover.artillery_castle.as_ref().unwrap().rondels[0].parapet_solids[0];
        foot_level_cover
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .size
            .y = 0.4;
        assert!(has(&foot_level_cover, "invalid_artillery_rondel"));
        let mut no_stair_guard = fixture();
        no_stair_guard.artillery_castle.as_mut().unwrap().rondels[0]
            .stair_guard_solids
            .clear();
        assert!(has(&no_stair_guard, "invalid_artillery_rondel"));
        let mut low_stair_guard = fixture();
        let id =
            low_stair_guard.artillery_castle.as_ref().unwrap().rondels[0].stair_guard_solids[0];
        low_stair_guard
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .size
            .y = 0.45;
        assert!(has(&low_stair_guard, "invalid_artillery_rondel"));
        let mut blocked_stair_arrival = fixture();
        let id = blocked_stair_arrival
            .artillery_castle
            .as_ref()
            .unwrap()
            .rondels[0]
            .stair_guard_solids[0];
        blocked_stair_arrival
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
            inner_radius_metres: 1.30,
            outer_radius_metres: 1.43,
            start_angle_radians: 0.70,
            end_angle_radians: 0.90,
            inner_top_offset_metres: 0.0,
            outer_top_offset_metres: -0.02,
        };
        assert!(has(&blocked_stair_arrival, "invalid_artillery_rondel"));
        let mut unarmed = fixture();
        unarmed.artillery_castle.as_mut().unwrap().stations[0]
            .rays
            .clear();
        assert!(has(&unarmed, "inoperable_artillery_station"));
        let mut uncovered = fixture();
        for station in &mut uncovered.artillery_castle.as_mut().unwrap().stations {
            for ray in &mut station.rays {
                if ray.target_kind == crate::ArtilleryTargetKind::Bridge {
                    ray.target_kind = crate::ArtilleryTargetKind::Approach;
                }
            }
        }
        assert!(has(&uncovered, "incomplete_artillery_coverage"));
        let mut dead_corner = fixture();
        let target = dead_corner
            .artillery_castle
            .as_ref()
            .unwrap()
            .defense_targets
            .iter()
            .find(|target| {
                target.kind == crate::ArtilleryTargetKind::DitchCorner
                    && target.required_independent_stations > 0
            })
            .unwrap()
            .id;
        for station in &mut dead_corner.artillery_castle.as_mut().unwrap().stations {
            station.rays.retain(|ray| ray.target_id != target);
        }
        assert!(has(&dead_corner, "incomplete_artillery_coverage"));
        let mut duplicate_target = fixture();
        let cloned = duplicate_target
            .artillery_castle
            .as_ref()
            .unwrap()
            .defense_targets[0]
            .clone();
        duplicate_target
            .artillery_castle
            .as_mut()
            .unwrap()
            .defense_targets
            .push(cloned);
        assert!(has(&duplicate_target, "incomplete_artillery_coverage"));
        let mut off_envelope = fixture();
        let required = off_envelope
            .artillery_castle
            .as_ref()
            .unwrap()
            .defense_targets
            .iter()
            .find(|target| target.required_independent_stations > 0)
            .unwrap()
            .id;
        let station = off_envelope
            .artillery_castle
            .as_mut()
            .unwrap()
            .stations
            .iter_mut()
            .find(|station| station.rays.iter().any(|ray| ray.target_id == required))
            .unwrap();
        station
            .rays
            .iter_mut()
            .find(|ray| ray.target_id == required)
            .unwrap()
            .target
            .x += 2.0;
        assert!(has(&off_envelope, "inoperable_artillery_station"));
        let mut narrow = fixture();
        narrow.artillery_castle.as_mut().unwrap().route_edges[0].width_metres = 0.6;
        assert!(has(&narrow, "disconnected_artillery_route"));
        let mut low = fixture();
        low.artillery_castle.as_mut().unwrap().route_edges[0].headroom_metres = 1.2;
        assert!(has(&low, "disconnected_artillery_route"));
        let mut blocked_sweep = fixture();
        let point = blocked_sweep.artillery_castle.as_ref().unwrap().route_edges[2].sweep_path[3];
        let id = blocked_sweep.artillery_castle.as_ref().unwrap().rondels[0].parapet_solids[0];
        let solid = blocked_sweep
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap();
        solid.centre = point + Vec3::Y;
        solid.size = Vec3::new(1.2, 2.0, 1.2);
        solid.shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&blocked_sweep, "disconnected_artillery_route"));
        let mut missing_route_surface = fixture();
        missing_route_surface
            .artillery_castle
            .as_mut()
            .unwrap()
            .route_edges[0]
            .traversal_surface = None;
        assert!(has(&missing_route_surface, "disconnected_artillery_route"));
        let mut shifted_route_surface = fixture();
        let route_surface = shifted_route_surface
            .artillery_castle
            .as_ref()
            .unwrap()
            .route_edges[0]
            .traversal_surface
            .unwrap();
        shifted_route_surface
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == route_surface)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::RouteCorridor {
            start: Vec3::ZERO,
            end: Vec3::X,
            width_metres: 1.8,
        };
        assert!(has(&shifted_route_surface, "disconnected_artillery_route"));
        let mut shifted_portal = fixture();
        let edge = shifted_portal
            .artillery_castle
            .as_ref()
            .unwrap()
            .route_edges
            .iter()
            .find(|edge| edge.portal_void.is_some())
            .unwrap()
            .clone();
        let portal = edge.portal_void.unwrap();
        let void = shifted_portal
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == portal)
            .unwrap();
        void.bounds.min += Vec3::X * 20.0;
        void.bounds.max += Vec3::X * 20.0;
        assert!(has(&shifted_portal, "disconnected_artillery_route"));
        let mut missing_tread = fixture();
        let tread = missing_tread.artillery_castle.as_ref().unwrap().rondels[0].stair_solids[0];
        missing_tread
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != tread);
        assert!(
            has(&missing_tread, "disconnected_artillery_route")
                || has(&missing_tread, "renderer_geometry_mismatch")
        );
        let mut severed_ramp = fixture();
        let ramp = severed_ramp
            .artillery_castle
            .as_ref()
            .unwrap()
            .service_ramp_solids[0];
        for edge in &mut severed_ramp.artillery_castle.as_mut().unwrap().route_edges {
            edge.connector_solids.retain(|id| *id != ramp);
        }
        assert!(has(&severed_ramp, "disconnected_artillery_route"));
        let mut broken_bridge = fixture();
        broken_bridge
            .artillery_castle
            .as_mut()
            .unwrap()
            .bridge
            .route_surface = None;
        assert!(has(&broken_bridge, "invalid_artillery_approach"));
        let mut false_ditch = fixture();
        let ditch = false_ditch.artillery_castle.as_ref().unwrap().ditch.void_id;
        false_ditch
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == ditch)
            .unwrap()
            .shape = crate::ResolvedVoidShape::Box;
        assert!(has(&false_ditch, "invalid_artillery_approach"));
        let mut no_mechanism = fixture();
        let mechanism = no_mechanism
            .artillery_castle
            .as_ref()
            .unwrap()
            .gate_chamber_solids
            .iter()
            .find(|id| {
                no_mechanism.resolved_geometry.solids.iter().any(|solid| {
                    solid.id == **id && solid.role == SolidRole::ArtilleryGateMechanism
                })
            })
            .copied()
            .unwrap();
        no_mechanism
            .artillery_castle
            .as_mut()
            .unwrap()
            .gate_chamber_solids
            .retain(|id| *id != mechanism);
        assert!(has(&no_mechanism, "invalid_artillery_approach"));
        let mut glazed = fixture();
        let opening_id = glazed.artillery_castle.as_ref().unwrap().stations[0].opening;
        glazed
            .opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == opening_id)
            .unwrap()
            .closure
            .layers = vec![crate::ClosureKind::LeadedGlazing];
        assert!(
            has(&glazed, "inoperable_artillery_station") || has(&glazed, "illegal_opening_closure")
        );

        let denied = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::ArtilleryRondelCastle,
            702,
        ))
        .unwrap();
        assert!(audit_plan(&denied).is_empty(), "{:?}", audit_plan(&denied));
        let mut denied_crossing = denied.clone();
        let removable = denied_crossing
            .artillery_castle
            .as_ref()
            .unwrap()
            .bridge
            .removable_solids[0];
        denied_crossing
            .artillery_castle
            .as_mut()
            .unwrap()
            .route_edges[0]
            .connector_solids
            .push(removable);
        assert!(
            has(&denied_crossing, "invalid_artillery_approach")
                || has(&denied_crossing, "disconnected_artillery_route")
        );
        let mut contaminated = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            47,
        ))
        .unwrap();
        contaminated.castle_phase = Some(crate::CastleConstructionPhase::ArtilleryRetrofit1544);
        assert!(has(&contaminated, "artillery_phase_contamination"));
    }

    #[test]
    fn church_audit_rejects_program_structure_tower_and_route_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::Cathedral,
                47,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut missing = fixture();
        missing.church = None;
        assert!(has(&missing, "missing_church_program"));

        let mut axes = fixture();
        axes.church.as_mut().unwrap().nave_axes_metres.swap(0, 1);
        assert!(has(&axes, "invalid_church_bay_axes"));

        let mut pier = fixture();
        let pier_id = pier.church.as_ref().unwrap().bay_assemblies[0].pier_solids[0];
        pier.resolved_geometry
            .solids
            .retain(|solid| solid.id != pier_id);
        assert!(has(&pier, "invalid_church_bay_structure"));

        let mut buttress = fixture();
        let node_id = buttress.church.as_ref().unwrap().bay_assemblies[0].buttress_nodes[0];
        buttress
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == node_id)
            .unwrap()
            .grounded = false;
        assert!(has(&buttress, "invalid_church_bay_structure"));

        let mut one_sided_arcade = fixture();
        let arcade_id =
            one_sided_arcade.church.as_ref().unwrap().bay_assemblies[0].arcade_solids[0];
        let spring_id = one_sided_arcade
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == arcade_id)
            .unwrap()
            .supported_by[0];
        one_sided_arcade
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == spring_id)
            .unwrap()
            .supported_by
            .pop();
        assert!(has(&one_sided_arcade, "invalid_church_bay_structure"));

        let mut shifted_springing = fixture();
        let interface_id = shifted_springing.church.as_ref().unwrap().bay_assemblies[0]
            .arcade_bearing_interfaces[0][1];
        shifted_springing
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
            .unwrap()
            .bounds
            .min
            .x += 1.0;
        assert!(has(&shifted_springing, "invalid_church_bay_structure"));

        let mut decorative_buttress = fixture();
        let vault_spring =
            decorative_buttress.church.as_ref().unwrap().bay_assemblies[0].vault_spring_nodes[0];
        let buttress_supports = decorative_buttress
            .resolved_geometry
            .structural_nodes
            .iter()
            .filter(|node| node.kind == crate::StructuralNodeKind::ChurchButtress)
            .map(|node| node.id)
            .collect::<BTreeSet<_>>();
        decorative_buttress
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == vault_spring)
            .unwrap()
            .supported_by
            .retain(|support| !buttress_supports.contains(support));
        assert!(has(&decorative_buttress, "invalid_church_bay_structure"));

        let mut floating_vault = fixture();
        let vault_id = floating_vault.church.as_ref().unwrap().bay_assemblies[0].vault_solids[0];
        floating_vault
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == vault_id)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(&floating_vault, "invalid_church_bay_structure"));

        let mut missing_load_surface = fixture();
        missing_load_surface.church.as_mut().unwrap().bay_assemblies[0]
            .vault_load_surfaces
            .pop();
        assert!(has(&missing_load_surface, "invalid_church_bay_structure"));

        let mut crossing = fixture();
        crossing
            .church
            .as_mut()
            .unwrap()
            .crossing
            .vault_solids
            .clear();
        assert!(has(&crossing, "invalid_church_crossing"));

        let mut flat_crossing_arch = fixture();
        let crossing_arch = flat_crossing_arch
            .church
            .as_ref()
            .unwrap()
            .crossing
            .arch_solids[0];
        flat_crossing_arch
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == crossing_arch)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&flat_crossing_arch, "invalid_church_crossing"));

        let mut shifted_crossing_spring = fixture();
        let crossing_interface = shifted_crossing_spring
            .church
            .as_ref()
            .unwrap()
            .crossing
            .arch_bearing_interfaces[0][1];
        shifted_crossing_spring
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == crossing_interface)
            .unwrap()
            .bounds
            .min
            .x += 1.0;
        assert!(has(&shifted_crossing_spring, "invalid_church_crossing"));

        let mut severed_crossing_thrust = fixture();
        let crossing_spring = severed_crossing_thrust
            .church
            .as_ref()
            .unwrap()
            .crossing
            .vault_spring_nodes[0];
        let crossing_buttress = severed_crossing_thrust
            .church
            .as_ref()
            .unwrap()
            .crossing
            .buttress_nodes[0];
        severed_crossing_thrust
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == crossing_spring)
            .unwrap()
            .supported_by
            .retain(|support| *support != crossing_buttress);
        assert!(has(&severed_crossing_thrust, "invalid_church_crossing"));

        let mut missing_crossing_load = fixture();
        missing_crossing_load
            .church
            .as_mut()
            .unwrap()
            .crossing
            .vault_load_surfaces
            .clear();
        assert!(has(&missing_crossing_load, "invalid_church_crossing"));

        let mut apse = fixture();
        apse.church.as_mut().unwrap().choir.apse_facets.pop();
        assert!(has(&apse, "invalid_church_choir_apse"));

        let mut flat_choir_arch = fixture();
        let choir_arch = flat_choir_arch.church.as_ref().unwrap().choir.arch_solids[0];
        flat_choir_arch
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == choir_arch)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&flat_choir_arch, "invalid_church_choir_apse"));

        let mut severed_choir_thrust = fixture();
        let choir_spring = severed_choir_thrust
            .church
            .as_ref()
            .unwrap()
            .choir
            .vault_spring_nodes[0];
        let choir_buttress = severed_choir_thrust
            .church
            .as_ref()
            .unwrap()
            .choir
            .buttress_nodes[0];
        severed_choir_thrust
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == choir_spring)
            .unwrap()
            .supported_by
            .retain(|support| *support != choir_buttress);
        assert!(has(&severed_choir_thrust, "invalid_church_choir_apse"));

        let mut shifted_choir_spring = fixture();
        let choir_interface = shifted_choir_spring
            .church
            .as_ref()
            .unwrap()
            .choir
            .arch_bearing_interfaces[0][0];
        let interface = shifted_choir_spring
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == choir_interface)
            .unwrap();
        interface.bounds.min.z += 1.0;
        interface.bounds.max.z += 1.0;
        assert!(has(&shifted_choir_spring, "invalid_church_choir_apse"));

        let mut portal = fixture();
        portal.church.as_mut().unwrap().tower.west_portal = crate::OpeningAssemblyId(0);
        assert!(has(&portal, "invalid_church_west_tower"));

        let mut stair = fixture();
        let stair_index = stair.church.as_ref().unwrap().tower.stair_index;
        if let Stair::Spiral { rise_metres, .. } = &mut stair.stairs[stair_index] {
            *rise_metres -= 2.0;
        }
        assert!(has(&stair, "invalid_church_west_tower"));

        let mut bell = fixture();
        let bell_opening = bell.church.as_ref().unwrap().tower.bell_openings[0];
        bell.opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == bell_opening)
            .unwrap()
            .closure
            .layers = vec![crate::ClosureKind::LeadedGlazing];
        assert!(has(&bell, "invalid_church_west_tower"));

        let mut blocked_stairwell = fixture();
        let floor_id = blocked_stairwell
            .church
            .as_ref()
            .unwrap()
            .tower
            .bell_floor_solids[0];
        let centre = blocked_stairwell.church.as_ref().unwrap().tower.centre;
        let floor = blocked_stairwell
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap();
        floor.centre.x = centre.x;
        floor.centre.z = centre.y;
        assert!(has(
            &blocked_stairwell,
            "invalid_church_tower_service_geometry"
        ));

        let mut short_roof_ladder = fixture();
        let ladder_ids = short_roof_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .roof_ladder_solids
            .clone();
        for solid in &mut short_roof_ladder.resolved_geometry.solids {
            if ladder_ids.contains(&solid.id) {
                solid.centre.y -= 1.0;
                solid.size.y = solid.size.y.min(1.0);
            }
        }
        assert!(has(
            &short_roof_ladder,
            "invalid_church_tower_service_geometry"
        ));

        let mut unsupported_bell_floor = fixture();
        let floor_id = unsupported_bell_floor
            .church
            .as_ref()
            .unwrap()
            .tower
            .bell_floor_solids[0];
        let stage_id = unsupported_bell_floor
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == floor_id)
            .unwrap()
            .supported_by[0];
        unsupported_bell_floor
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == stage_id)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(
            &unsupported_bell_floor,
            "invalid_church_tower_service_geometry"
        ));

        let mut window = fixture();
        window.church.as_mut().unwrap().bay_assemblies[0].clerestory_openings[0] =
            crate::OpeningAssemblyId(0);
        assert!(has(&window, "invalid_church_window_hierarchy"));

        let mut unglazed_light = fixture();
        let window_id =
            unglazed_light.church.as_ref().unwrap().bay_assemblies[0].clerestory_openings[0];
        unglazed_light
            .opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == window_id)
            .unwrap()
            .closure_solids
            .clear();
        assert!(has(&unglazed_light, "invalid_church_window_hierarchy"));

        let mut route = fixture();
        route.church.as_mut().unwrap().circulation[0].width_metres = 0.70;
        assert!(has(&route, "invalid_church_circulation"));

        let mut shifted_west_portal_void = fixture();
        let west_portal = shifted_west_portal_void
            .church
            .as_ref()
            .unwrap()
            .tower
            .west_portal;
        let west_void = shifted_west_portal_void
            .opening_assemblies
            .iter()
            .find(|opening| opening.id == west_portal)
            .unwrap()
            .void_id;
        let west_void = shifted_west_portal_void
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == west_void)
            .unwrap();
        west_void.bounds.min.z += 1.0;
        west_void.bounds.max.z += 1.0;
        assert!(has(&shifted_west_portal_void, "invalid_church_circulation"));

        let mut shifted_nave_passage_void = fixture();
        let nave_passage = shifted_nave_passage_void
            .church
            .as_ref()
            .unwrap()
            .tower
            .nave_passage;
        let nave_void = shifted_nave_passage_void
            .opening_assemblies
            .iter()
            .find(|opening| opening.id == nave_passage)
            .unwrap()
            .void_id;
        let nave_void = shifted_nave_passage_void
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == nave_void)
            .unwrap();
        nave_void.bounds.min.z -= 1.0;
        nave_void.bounds.max.z -= 1.0;
        assert!(has(
            &shifted_nave_passage_void,
            "invalid_church_circulation"
        ));

        let mut narrowed_west_portal = fixture();
        let west_portal = narrowed_west_portal
            .church
            .as_ref()
            .unwrap()
            .tower
            .west_portal;
        let portal = narrowed_west_portal
            .opening_assemblies
            .iter_mut()
            .find(|opening| opening.id == west_portal)
            .unwrap();
        portal.profile = crate::OpeningProfile::Rectangular {
            width_metres: 1.60,
            height_metres: 3.20,
        };
        portal
            .sectional_void
            .iter_mut()
            .for_each(|slice| slice.width_metres = 1.60);
        assert!(has(&narrowed_west_portal, "invalid_church_circulation"));

        let mut public_self_edge = fixture();
        let public = public_self_edge
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::PublicProcessional)
            .unwrap();
        public.edges[0].to = public.edges[0].from;
        assert!(has(&public_self_edge, "invalid_church_circulation"));

        let mut missing_vestibule = fixture();
        let vestibule = missing_vestibule
            .church
            .as_ref()
            .unwrap()
            .tower
            .vestibule_surface;
        missing_vestibule
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::PublicProcessional)
            .unwrap()
            .surface_ids
            .retain(|id| *id != vestibule);
        assert!(has(&missing_vestibule, "invalid_church_circulation"));

        let mut missing_shared_service_connection = fixture();
        let vestibule = missing_shared_service_connection
            .church
            .as_ref()
            .unwrap()
            .tower
            .vestibule_surface;
        let bell_route = missing_shared_service_connection
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap();
        bell_route.surface_ids.retain(|id| *id != vestibule);
        bell_route
            .edges
            .retain(|edge| edge.from != vestibule && edge.to != vestibule);
        assert!(has(
            &missing_shared_service_connection,
            "invalid_church_circulation"
        ));

        let mut broken_tread = fixture();
        let tread = broken_tread
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_tread_solids[20];
        broken_tread
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != tread);
        assert!(has(&broken_tread, "invalid_church_circulation"));

        let mut severed_stair_bearing = fixture();
        let bearing = severed_stair_bearing
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_bearing_node;
        severed_stair_bearing
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == bearing)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(&severed_stair_bearing, "invalid_church_circulation"));

        let mut shifted_tread_bearing = fixture();
        let bearing_interface = shifted_tread_bearing
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_tread_interfaces[12];
        let interface = shifted_tread_bearing
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == bearing_interface)
            .unwrap();
        interface.bounds.min.x += 0.8;
        interface.bounds.max.x += 0.8;
        assert!(has(&shifted_tread_bearing, "invalid_church_circulation"));

        let mut moved_newel = fixture();
        let newel = moved_newel.church.as_ref().unwrap().tower.stair_newel_solid;
        moved_newel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == newel)
            .unwrap()
            .centre
            .x += 0.5;
        assert!(has(&moved_newel, "invalid_church_circulation"));

        let mut guard_in_route = fixture();
        let guard = guard_in_route.church.as_ref().unwrap().tower.guard_solids[0];
        let route_tread = guard_in_route
            .church
            .as_ref()
            .unwrap()
            .tower
            .stair_tread_solids[20];
        let route_centre = guard_in_route
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == route_tread)
            .unwrap()
            .centre;
        guard_in_route
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == guard)
            .unwrap()
            .centre = route_centre + Vec3::Y * 0.55;
        assert!(has(&guard_in_route, "invalid_church_circulation"));

        let mut frame_in_ladder = fixture();
        let frame = frame_in_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .bell_frame_solids[0];
        let rung = frame_in_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .roof_ladder_solids[6];
        let rung_centre = frame_in_ladder
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == rung)
            .unwrap()
            .centre;
        frame_in_ladder
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == frame)
            .unwrap()
            .centre = rung_centre;
        assert!(has(&frame_in_ladder, "invalid_church_circulation"));

        let mut broken_edge = fixture();
        let bell_route = broken_edge
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap();
        bell_route.edges.remove(12);
        assert!(has(&broken_edge, "invalid_church_circulation"));

        let mut missing_landing = fixture();
        let landing = missing_landing
            .church
            .as_ref()
            .unwrap()
            .tower
            .landing_solids[1];
        missing_landing
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap()
            .traversable_solid_ids
            .retain(|id| *id != landing);
        assert!(has(&missing_landing, "invalid_church_circulation"));

        let mut low_route = fixture();
        low_route
            .church
            .as_mut()
            .unwrap()
            .circulation
            .iter_mut()
            .find(|route| route.kind == crate::ChurchRouteKind::BellService)
            .unwrap()
            .edges[0]
            .clear_headroom_metres = 1.20;
        assert!(has(&low_route, "invalid_church_circulation"));

        let mut blocked_ladder = fixture();
        let bell_id = blocked_ladder.church.as_ref().unwrap().tower.bell_solid;
        let rung_id = blocked_ladder
            .church
            .as_ref()
            .unwrap()
            .tower
            .roof_ladder_solids[4];
        let rung_centre = blocked_ladder
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == rung_id)
            .unwrap()
            .centre;
        blocked_ladder
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == bell_id)
            .unwrap()
            .centre = rung_centre;
        assert!(has(&blocked_ladder, "invalid_church_circulation"));

        let mut roof = fixture();
        roof.church.as_mut().unwrap().roof_assemblies.clear();
        assert!(has(&roof, "invalid_church_roof_program"));
    }

    #[test]
    fn roof_audit_rejects_graph_support_child_and_weathering_drift() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::FachwerkMerchantHouse,
                47,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut pitch = fixture();
        pitch.roof_assemblies[0].faces[0].pitch_degrees = 10.0;
        assert!(has(&pitch, "invalid_roof_pitch"));

        let mut plane = fixture();
        plane.roof_assemblies[0].faces[0].plane.constant += 1.0;
        assert!(has(&plane, "invalid_roof_face_contract"));

        let mut edge = fixture();
        edge.roof_assemblies[0].edges[0].adjacent_faces.clear();
        assert!(has(&edge, "roof_edge_adjacency"));

        let mut drain = fixture();
        let route = drain.resolved_geometry.drainage_catchments[0].outlet_route;
        drain
            .resolved_geometry
            .drainage_routes
            .retain(|candidate| candidate.id != route);
        assert!(has(&drain, "invalid_roof_face_contract"));

        let mut support = fixture();
        support.roof_assemblies[0].faces[0].support_nodes.clear();
        assert!(has(&support, "invalid_roof_face_contract"));

        let mut intact_parent = fixture();
        intact_parent.roof_assemblies[0]
            .faces
            .iter_mut()
            .for_each(|face| face.cutouts.clear());
        assert!(has(&intact_parent, "unresolved_roof_child"));

        let mut no_flashing = fixture();
        no_flashing.roof_assemblies[0].children[0]
            .flashing_ids
            .clear();
        assert!(has(&no_flashing, "unresolved_roof_child"));

        let mut reversed_shed = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::Cathedral,
            47,
        ))
        .unwrap();
        let shed = reversed_shed
            .roof_assemblies
            .iter_mut()
            .find(|roof| roof.kind == crate::RoofKind::Shed)
            .unwrap();
        shed.shed_high_side = Some(shed.shed_high_side.unwrap().opposite());
        assert!(has(&reversed_shed, "invalid_shed_slope_authority"));

        let mut orphan_eave = fixture();
        orphan_eave.roof_assemblies[0]
            .edges
            .iter_mut()
            .find(|edge| edge.kind == RoofEdgeKind::Eave)
            .unwrap()
            .drainage_terminal = None;
        assert!(has(&orphan_eave, "orphan_roof_drainage"));

        let mut tower_overlap = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::Cathedral,
            47,
        ))
        .unwrap();
        let tower_child = tower_overlap.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::Tower)
            .unwrap()
            .clone();
        tower_overlap.roof_assemblies[0]
            .faces
            .iter_mut()
            .for_each(|face| face.cutouts.clear());
        assert!(has(&tower_overlap, "unresolved_roof_child"));
        tower_overlap.roof_assemblies[0]
            .children
            .retain(|child| child.child != tower_child.child);
        assert!(has(&tower_overlap, "orphan_roof_child"));

        let mut flat_valley = fixture();
        let valley_flashing = flat_valley.roof_assemblies[0]
            .edges
            .iter()
            .find(|edge| edge.kind == RoofEdgeKind::Valley)
            .and_then(|edge| edge.flashing)
            .unwrap();
        flat_valley
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == valley_flashing)
            .unwrap()
            .longfall_radians = 0.0;
        assert!(has(&flat_valley, "invalid_roof_valley_drainage"));

        let mut orphan_valley = fixture();
        orphan_valley.roof_assemblies[0]
            .edges
            .iter_mut()
            .find(|edge| edge.kind == RoofEdgeKind::Valley)
            .unwrap()
            .drainage_terminal = None;
        assert!(has(&orphan_valley, "invalid_roof_valley_drainage"));

        let mut relabelled_full_hip = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::HallHouse,
            47,
        ))
        .unwrap();
        let half_hip = relabelled_full_hip
            .roof_assemblies
            .iter_mut()
            .find(|roof| roof.kind == crate::RoofKind::HalfHip)
            .unwrap();
        half_hip.enclosure_faces.clear();
        let base_y = half_hip
            .faces
            .iter()
            .flat_map(|face| &face.polygon)
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        for cap in half_hip
            .faces
            .iter_mut()
            .filter(|face| face.polygon.len() == 3)
        {
            cap.polygon[0].y = base_y;
            cap.polygon[2].y = base_y;
        }
        assert!(has(&relabelled_full_hip, "invalid_half_hip_graph"));

        let mut missing_channel = fixture();
        let channel = missing_channel.resolved_geometry.roof_drainage_networks[0].channel_floor;
        missing_channel
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != channel);
        assert!(has(&missing_channel, "invalid_roof_drainage_network"));

        let mut flat_channel = fixture();
        let channel = flat_channel.resolved_geometry.roof_drainage_networks[0].channel_floor;
        flat_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .longfall_radians = 0.0;
        assert!(has(&flat_channel, "invalid_roof_drainage_network"));

        let mut reversed_channel = fixture();
        reversed_channel.resolved_geometry.roof_drainage_networks[0]
            .channel_low
            .y = reversed_channel.resolved_geometry.roof_drainage_networks[0]
            .channel_high
            .y
            + 0.05;
        assert!(has(&reversed_channel, "invalid_roof_drainage_network"));

        let mut trapped_basin = fixture();
        trapped_basin.resolved_geometry.roof_drainage_networks[0]
            .samples
            .remove(3);
        assert!(has(&trapped_basin, "invalid_roof_drainage_network"));

        let mut shifted_channel = fixture();
        let channel = shifted_channel.resolved_geometry.roof_drainage_networks[0].channel_floor;
        shifted_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .centre += Vec3::X;
        assert!(has(&shifted_channel, "invalid_roof_drainage_network"));

        let mut disconnected_outlet = fixture();
        let outlet = disconnected_outlet.resolved_geometry.roof_drainage_networks[0].outlet_void;
        let drain = disconnected_outlet
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == outlet)
            .unwrap();
        drain.bounds.min += Vec3::X;
        drain.bounds.max += Vec3::X;
        assert!(has(&disconnected_outlet, "invalid_roof_drainage_network"));

        let spout_fixture = || {
            crate::BuildingArchetype::ALL
                .into_iter()
                .map(|archetype| {
                    crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap()
                })
                .find(|plan| {
                    plan.resolved_geometry
                        .roof_drainage_networks
                        .iter()
                        .any(|network| network.downspout.is_some())
                })
                .expect("curated roof suite contains a host-bound downspout")
        };
        let mut shifted_spout = spout_fixture();
        let spout = shifted_spout
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .expect("principal roof has a downspout");
        shifted_spout
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == spout)
            .unwrap()
            .centre += Vec3::Z;
        assert!(has(&shifted_spout, "invalid_roof_drainage_network"));

        let mut spout_through_opening = spout_fixture();
        let spout = spout_through_opening
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .expect("principal roof has a downspout");
        let opening = spout_through_opening
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.role == VoidRole::WallOpening)
            .unwrap();
        let opening_plan = (opening.bounds.min + opening.bounds.max) * 0.5;
        let pipe = spout_through_opening
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == spout)
            .unwrap();
        pipe.centre.x = opening_plan.x;
        pipe.centre.z = opening_plan.z;
        assert!(has(&spout_through_opening, "invalid_roof_drainage_network"));

        let mut spout_through_walk = spout_fixture();
        let spout = spout_through_walk
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .unwrap();
        let mut walk = spout_through_walk
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == spout)
            .unwrap()
            .clone();
        walk.id = crate::ResolvedItemId(0xAFFF_FFFF_FFFF_0001);
        walk.role = SolidRole::CircuitWalk;
        spout_through_walk.resolved_geometry.solids.push(walk);
        assert!(has(&spout_through_walk, "invalid_roof_drainage_network"));

        let mut spout_through_stair = spout_fixture();
        let spout = spout_through_stair
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .find_map(|network| network.downspout)
            .unwrap();
        let pipe = spout_through_stair
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == spout)
            .unwrap();
        spout_through_stair.stairs.push(crate::Stair::Straight {
            start: Vec2::new(pipe.centre.x, pipe.centre.z),
            direction: crate::Direction::North,
            base_height_metres: 0.0,
            rise_metres: pipe.centre.y + pipe.size.y,
            width_metres: 1.0,
            tread_count: 12,
            run_metres: 3.8,
        });
        assert!(has(&spout_through_stair, "invalid_roof_drainage_network"));

        let mut round_program =
            crate::BuildingProgram::fixture(crate::BuildingArchetype::TownHouse, 47);
        round_program.roof_demonstrator = Some(crate::RoofKind::Conical);
        let mut per_facet_round_spouts = crate::generate(&round_program).unwrap();
        let round_owner = per_facet_round_spouts
            .roof_assemblies
            .iter()
            .find(|roof| {
                matches!(
                    roof.kind,
                    crate::RoofKind::Conical | crate::RoofKind::Pavilion
                )
            })
            .unwrap()
            .owner;
        let mut duplicate = per_facet_round_spouts
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.owner == round_owner)
            .unwrap()
            .clone();
        duplicate.id = crate::ResolvedItemId(0x7FFF_FFFF_FFFF_0001);
        per_facet_round_spouts
            .resolved_geometry
            .roof_drainage_outlets
            .push(duplicate);
        assert!(has(&per_facet_round_spouts, "invalid_roof_outlet_topology"));

        let free_ground_fixture = || {
            crate::BuildingArchetype::ALL
                .into_iter()
                .map(|archetype| {
                    crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap()
                })
                .find(|plan| {
                    plan.resolved_geometry
                        .roof_drainage_outlets
                        .iter()
                        .any(|station| {
                            station.disposition == crate::RoofDrainageDisposition::FreeDripToGround
                        })
                })
                .expect("curated roof suite contains an explicit ground free-drip")
        };
        let mut blocked_free_fall = free_ground_fixture();
        let station = blocked_free_fall
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.disposition == crate::RoofDrainageDisposition::FreeDripToGround)
            .unwrap()
            .clone();
        let outlet = blocked_free_fall
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == station.outlet_void)
            .map(|void| (void.bounds.min + void.bounds.max) * 0.5)
            .unwrap();
        blocked_free_fall
            .resolved_geometry
            .solids
            .push(crate::ResolvedSolid {
                id: crate::ResolvedItemId(0xAFFF_FFFF_FFFF_0100),
                owner: crate::GeometryOwnerId(0xFFFF_0100),
                centre: (outlet + station.discharge) * 0.5,
                size: Vec3::new(1.0, 0.18, 1.0),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::CircuitWalk,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: Vec::new(),
            });
        assert!(has(&blocked_free_fall, "invalid_roof_drainage_network"));

        let mut splash_on_portal = free_ground_fixture();
        let station = splash_on_portal
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.disposition == crate::RoofDrainageDisposition::FreeDripToGround)
            .unwrap()
            .clone();
        splash_on_portal
            .resolved_geometry
            .voids
            .push(crate::ResolvedVoid {
                id: crate::ResolvedItemId(0xEFFF_FFFF_FFFF_0100),
                owner: crate::GeometryOwnerId(0xFFFF_0101),
                bounds: crate::ResolvedBounds {
                    min: station.discharge - Vec3::new(0.4, 0.08, 0.4),
                    max: station.discharge + Vec3::new(0.4, 1.9, 0.4),
                },
                role: VoidRole::AccessPortal,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: crate::GeometryOwnerId(0xFFFF_0101),
            });
        assert!(has(&splash_on_portal, "invalid_roof_drainage_network"));

        let mut splash_on_stair = free_ground_fixture();
        let discharge = splash_on_stair
            .resolved_geometry
            .roof_drainage_outlets
            .iter()
            .find(|station| station.disposition == crate::RoofDrainageDisposition::FreeDripToGround)
            .unwrap()
            .discharge;
        splash_on_stair.stairs.push(crate::Stair::Straight {
            start: Vec2::new(discharge.x, discharge.z),
            direction: crate::Direction::North,
            base_height_metres: 0.0,
            rise_metres: 1.2,
            width_metres: 1.0,
            tread_count: 8,
            run_metres: 3.8,
        });
        assert!(has(&splash_on_stair, "invalid_roof_drainage_network"));

        let mut off_parent_face = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::Cathedral,
            47,
        ))
        .unwrap();
        let station = off_parent_face
            .resolved_geometry
            .roof_drainage_outlets
            .iter_mut()
            .find(|station| {
                station.disposition == crate::RoofDrainageDisposition::FreeDripToParentRoof
            })
            .expect("cathedral tower drip lands on an exact parent face");
        station.discharge.x += 20.0;
        assert!(has(&off_parent_face, "invalid_roof_drainage_network"));

        let edge_treatment_fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::Cathedral,
                47,
            ))
            .unwrap()
        };
        let treatment_id = |plan: &crate::BuildingPlan| {
            let owners = plan
                .roof_assemblies
                .iter()
                .map(|roof| roof.owner)
                .collect::<std::collections::HashSet<_>>();
            plan.resolved_geometry
                .solids
                .iter()
                .find(|solid| {
                    owners.contains(&solid.owner) && solid.role == SolidRole::RoofEdgeTreatment
                })
                .map(|solid| solid.id)
                .expect("roof graph owns typed edge treatment")
        };
        let mut offset_treatment = edge_treatment_fixture();
        let treatment = treatment_id(&offset_treatment);
        offset_treatment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == treatment)
            .unwrap()
            .centre += Vec3::Z;
        assert!(has(&offset_treatment, "invalid_roof_edge_treatment"));

        let mut rotated_treatment = edge_treatment_fixture();
        let treatment = treatment_id(&rotated_treatment);
        rotated_treatment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == treatment)
            .unwrap()
            .yaw_radians += 0.4;
        assert!(has(&rotated_treatment, "invalid_roof_edge_treatment"));

        let mut overlong_treatment = edge_treatment_fixture();
        let treatment = treatment_id(&overlong_treatment);
        overlong_treatment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == treatment)
            .unwrap()
            .size
            .x += 2.0;
        assert!(has(&overlong_treatment, "invalid_roof_edge_treatment"));

        let cross_fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::FachwerkMerchantHouse,
                47,
            ))
            .unwrap()
        };
        let mut blank_dormer = cross_fixture();
        let cross = blank_dormer.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::CrossGable && child.child.0 >= 1_000)
            .unwrap()
            .clone();
        let front = blank_dormer
            .wall_assemblies
            .iter()
            .find(|wall| wall.source == crate::WallSourceId::RoofChildFront { roof: cross.child })
            .unwrap()
            .clone();
        blank_dormer
            .opening_assemblies
            .retain(|opening| opening.host_wall != front.id);
        assert!(has(&blank_dormer, "invalid_roof_child_front"));

        let mut floating_cross = cross_fixture();
        let cross = floating_cross.roof_assemblies[0]
            .children
            .iter_mut()
            .find(|child| child.kind == crate::RoofChildKind::CrossGable && child.child.0 >= 1_000)
            .unwrap();
        cross.facade_wall = None;
        assert!(has(&floating_cross, "invalid_roof_child_front"));

        let mut unsplit_cross = cross_fixture();
        let middle = unsplit_cross.roof_assemblies[0]
            .children
            .iter()
            .find(|child| child.kind == crate::RoofChildKind::CrossGable && child.child.0 >= 1_000)
            .unwrap()
            .split_eave_edges[1];
        unsplit_cross.roof_assemblies[0]
            .edges
            .iter_mut()
            .find(|edge| edge.id == middle)
            .unwrap()
            .kind = RoofEdgeKind::Eave;
        assert!(has(&unsplit_cross, "invalid_roof_child_front"));

        let abutment_fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::Cathedral,
                47,
            ))
            .unwrap()
        };
        let abutment_plan = abutment_fixture();
        let wall_abutment_count = abutment_plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.abutments)
            .filter(|abutment| abutment.kind == crate::RoofAbutmentKind::Wall)
            .count();
        assert_eq!(wall_abutment_count, 2);

        let mut missing_clerestory_host = abutment_fixture();
        let clerestory = missing_clerestory_host
            .wall_assemblies
            .iter()
            .find(|wall| matches!(wall.source, crate::WallSourceId::ChurchArcade { .. }))
            .unwrap()
            .id;
        missing_clerestory_host
            .wall_assemblies
            .retain(|wall| wall.id != clerestory);
        assert!(has(
            &missing_clerestory_host,
            "invalid_roof_abutment_contour"
        ));

        let mut shifted_clerestory_face = abutment_fixture();
        let clerestory = shifted_clerestory_face
            .wall_assemblies
            .iter_mut()
            .find(|wall| matches!(wall.source, crate::WallSourceId::ChurchArcade { .. }))
            .unwrap();
        clerestory.frame.origin += clerestory.frame.outward * 0.40;
        assert!(has(
            &shifted_clerestory_face,
            "invalid_roof_abutment_contour"
        ));

        let mut contour_gap = abutment_fixture();
        contour_gap.roof_assemblies[0].abutments[0]
            .samples
            .remove(1);
        assert!(has(&contour_gap, "invalid_roof_abutment_contour"));

        let mut raised_flashing = abutment_fixture();
        let upstand = raised_flashing.roof_assemblies[0].abutments[0].samples[0].upstand_solid;
        raised_flashing
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == upstand)
            .unwrap()
            .centre
            .y += 0.5;
        assert!(has(&raised_flashing, "invalid_roof_abutment_contour"));

        let mut no_lower_outlet = abutment_fixture();
        let outlet = no_lower_outlet.roof_assemblies[0].abutments[0].lower_outlet;
        no_lower_outlet
            .resolved_geometry
            .voids
            .retain(|void| void.id != outlet);
        assert!(has(&no_lower_outlet, "invalid_roof_abutment_contour"));

        let mut wrong_tower_host = abutment_fixture();
        let ordinary_wall = wrong_tower_host
            .wall_assemblies
            .iter()
            .find(|wall| !matches!(wall.source, crate::WallSourceId::SquareTowerFace { .. }))
            .unwrap()
            .id;
        wrong_tower_host.roof_assemblies[0]
            .abutments
            .iter_mut()
            .find(|abutment| abutment.kind == crate::RoofAbutmentKind::Tower)
            .unwrap()
            .samples[0]
            .host_wall = ordinary_wall;
        assert!(has(&wrong_tower_host, "invalid_roof_abutment_contour"));
    }

    #[test]
    fn wall_opening_audit_rejects_authority_profile_support_and_operability_drift() {
        let fixture =
            |archetype| crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap();
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut thickness = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        thickness.wall_assemblies[0].thickness_metres = 0.08;
        assert!(has(&thickness, "wall_profile_thickness"));

        let mut duplicate = fixture(crate::BuildingArchetype::TownHouse);
        duplicate.wall_assemblies[1].source = duplicate.wall_assemblies[0].source;
        assert!(has(&duplicate, "invalid_wall_authority"));

        let mut host = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        host.wall_assemblies[0].host_solids.clear();
        assert!(has(&host, "invalid_wall_host_union"));

        let mut shallow = fixture(crate::BuildingArchetype::Cathedral);
        let opening = shallow.opening_assemblies[0].clone();
        let void = shallow
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == opening.void_id)
            .unwrap();
        if opening.frame.outward.x.abs() > 0.5 {
            let middle = (void.bounds.min.x + void.bounds.max.x) * 0.5;
            void.bounds.min.x = middle - 0.01;
            void.bounds.max.x = middle + 0.01;
        } else {
            let middle = (void.bounds.min.z + void.bounds.max.z) * 0.5;
            void.bounds.min.z = middle - 0.01;
            void.bounds.max.z = middle + 0.01;
        }
        assert!(has(&shallow, "shallow_wall_opening"));

        let mut substituted = fixture(crate::BuildingArchetype::Cathedral);
        substituted.opening_assemblies[0].profile = crate::OpeningProfile::Rectangular {
            width_metres: 1.0,
            height_metres: 2.0,
        };
        assert!(has(&substituted, "opening_head_profile_mismatch"));

        let mut reveal = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        reveal.opening_assemblies[0].reveal_surfaces.pop();
        assert!(has(&reveal, "missing_opening_reveal_piece"));

        let mut timber_frame = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let timber_wall = timber_frame
            .wall_assemblies
            .iter()
            .find(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
            .unwrap()
            .id;
        timber_frame
            .timber_frame
            .as_mut()
            .unwrap()
            .bays
            .retain(|bay| bay.wall != Some(timber_wall));
        assert!(has(&timber_frame, "missing_authoritative_timber_frame"));

        let mut bearing = fixture(crate::BuildingArchetype::Cathedral);
        let head = bearing.opening_assemblies[0].head_node;
        bearing
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == head)
            .unwrap()
            .supported_by
            .pop();
        assert!(has(&bearing, "false_opening_head_load_path"));

        let mut military = fixture(crate::BuildingArchetype::WalledKeep);
        let military_index = military
            .opening_assemblies
            .iter()
            .position(|opening| opening.use_kind == crate::OpeningUse::GunLoop)
            .unwrap();
        military.opening_assemblies[military_index].closure.layers =
            vec![crate::ClosureKind::LeadedGlazing];
        military.opening_assemblies[military_index]
            .ray_indices
            .pop();
        assert!(has(&military, "illegal_opening_closure"));
        assert!(has(&military, "inoperable_military_opening"));

        let mut false_splay = fixture(crate::BuildingArchetype::WalledKeep);
        let opening = false_splay
            .opening_assemblies
            .iter()
            .find(|opening| opening.use_kind == crate::OpeningUse::GunLoop)
            .unwrap()
            .clone();
        false_splay
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == opening.jamb_solids[0])
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&false_splay, "false_splayed_wall_opening"));

        let mut flat_military_head = fixture(crate::BuildingArchetype::WalledKeep);
        let opening = flat_military_head
            .opening_assemblies
            .iter()
            .find(|opening| opening.use_kind == crate::OpeningUse::GunLoop)
            .unwrap()
            .clone();
        flat_military_head
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == opening.head_solid)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&flat_military_head, "false_splayed_wall_opening"));

        for mutation in ["missing_spandrel", "moved_spandrel"] {
            let mut plan = fixture(crate::BuildingArchetype::Cathedral);
            let opening = plan.opening_assemblies[0].clone();
            if mutation == "missing_spandrel" {
                plan.resolved_geometry
                    .solids
                    .retain(|solid| solid.id != opening.spandrel_solid);
            } else {
                plan.resolved_geometry
                    .solids
                    .iter_mut()
                    .find(|solid| solid.id == opening.spandrel_solid)
                    .unwrap()
                    .centre
                    .y += 0.25;
            }
            assert!(has(&plan, "false_opening_head_load_path"), "{mutation}");
        }

        let mut hanging_tracery = fixture(crate::BuildingArchetype::Cathedral);
        let opening = hanging_tracery
            .opening_assemblies
            .iter()
            .find(|opening| opening.tracery_node.is_some())
            .unwrap()
            .clone();
        let mullion = hanging_tracery
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == opening.owner && solid.role == SolidRole::Mullion)
            .unwrap();
        mullion.centre.y += 0.20;
        assert!(has(&hanging_tracery, "unsupported_cathedral_tracery"));

        let military_opening = |plan: &crate::BuildingPlan| {
            plan.opening_assemblies
                .iter()
                .position(|opening| {
                    matches!(
                        opening.use_kind,
                        crate::OpeningUse::ArrowLoop | crate::OpeningUse::GunLoop
                    )
                })
                .unwrap()
        };
        let mut filled_middle = fixture(crate::BuildingArchetype::WalledKeep);
        let index = military_opening(&filled_middle);
        filled_middle.opening_assemblies[index].sectional_void[4].width_metres = 0.01;
        assert!(has(&filled_middle, "false_splayed_wall_opening"));

        let mut exterior_mouth = fixture(crate::BuildingArchetype::WalledKeep);
        let index = military_opening(&exterior_mouth);
        let mouth = exterior_mouth.opening_assemblies[index]
            .sectional_void
            .last()
            .unwrap()
            .width_metres;
        exterior_mouth.opening_assemblies[index].sectional_void[0].width_metres = mouth;
        assert!(has(&exterior_mouth, "false_splayed_wall_opening"));

        let mut reversed = fixture(crate::BuildingArchetype::WalledKeep);
        let index = military_opening(&reversed);
        reversed.opening_assemblies[index].sectional_void.reverse();
        assert!(has(&reversed, "false_splayed_wall_opening"));

        let mut flat_sill = fixture(crate::BuildingArchetype::Cathedral);
        let sill_id =
            flat_sill.opening_assemblies[0]
                .reveal_surfaces
                .iter()
                .copied()
                .find(|id| {
                    flat_sill.resolved_geometry.surfaces.iter().any(|surface| {
                        surface.id == *id && surface.role == SurfaceRole::WeatherSill
                    })
                })
                .unwrap();
        flat_sill
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == sill_id)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::Planar;
        assert!(has(&flat_sill, "invalid_opening_weather_or_intrados"));

        let mut flat_intrados = fixture(crate::BuildingArchetype::Cathedral);
        let intrados_id = flat_intrados.opening_assemblies[0]
            .reveal_surfaces
            .iter()
            .copied()
            .find(|id| {
                flat_intrados
                    .resolved_geometry
                    .surfaces
                    .iter()
                    .any(|surface| surface.id == *id && surface.role == SurfaceRole::Intrados)
            })
            .unwrap();
        flat_intrados
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == intrados_id)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::Planar;
        assert!(has(&flat_intrados, "invalid_opening_weather_or_intrados"));

        for mutation in ["raised_head", "short_head", "shifted_jamb", "thin_pier"] {
            let mut plan = fixture(crate::BuildingArchetype::Cathedral);
            let opening = plan.opening_assemblies[0].clone();
            match mutation {
                "raised_head" => {
                    plan.resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.head_solid)
                        .unwrap()
                        .centre
                        .y += 0.25
                }
                "short_head" => {
                    let head = plan
                        .resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.head_solid)
                        .unwrap();
                    if opening.frame.tangent.x.abs() > 0.5 {
                        head.size.x -= 0.35;
                    } else {
                        head.size.z -= 0.35;
                    }
                }
                "shifted_jamb" => {
                    plan.resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.jamb_solids[0])
                        .unwrap()
                        .centre += Vec3::new(
                        opening.frame.tangent.x * 0.25,
                        0.0,
                        opening.frame.tangent.y * 0.25,
                    )
                }
                "thin_pier" => {
                    let jamb = plan
                        .resolved_geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == opening.jamb_solids[0])
                        .unwrap();
                    if opening.frame.tangent.x.abs() > 0.5 {
                        jamb.size.x = 0.02;
                    } else {
                        jamb.size.z = 0.02;
                    }
                }
                _ => unreachable!(),
            }
            assert!(has(&plan, "false_opening_head_load_path"), "{mutation}");
        }
        let mut exterior_fin = fixture(crate::BuildingArchetype::CourtyardCastle);
        let exterior_opening = exterior_fin
            .opening_assemblies
            .iter()
            .find(|opening| opening.frame.outside_room.is_none())
            .unwrap()
            .clone();
        exterior_fin
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == exterior_opening.jamb_solids[0])
            .unwrap()
            .centre += Vec3::new(
            exterior_opening.frame.outward.x * 0.10,
            0.0,
            exterior_opening.frame.outward.y * 0.10,
        );
        assert!(has(&exterior_fin, "discontinuous_exterior_wall_face"));

        let mut radial = fixture(crate::BuildingArchetype::WalledKeep);
        let radial_wall = radial
            .wall_assemblies
            .iter()
            .find(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { .. }))
            .unwrap()
            .clone();
        radial
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == radial_wall.host_solids[0])
            .unwrap()
            .shape = crate::ResolvedSolidShape::RoundTowerShell {
            outer_radius_metres: 3.0,
            inner_radius_metres: 2.7,
            chord_interfaces: [None, None],
        };
        assert!(has(&radial, "invalid_round_wall_authority"));

        let mut legacy = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        legacy.opening_assemblies.pop();
        assert!(has(&legacy, "legacy_opening_not_migrated"));
    }

    #[test]
    fn projected_defense_audit_rejects_operability_support_and_phase_regressions() {
        let fixture = |seed| {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::CastleGatehouse,
                seed,
            ))
            .unwrap()
        };
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };
        let plan = fixture(149);
        let breteche_plan = fixture(201);
        let deployed_plan = fixture(202);
        let bartizan_plan = fixture(203);
        assert!(audit_plan(&plan).is_empty(), "{:?}", audit_plan(&plan));
        assert!(
            audit_plan(&breteche_plan).is_empty(),
            "{:?}",
            audit_plan(&breteche_plan)
        );
        assert!(
            audit_plan(&deployed_plan).is_empty(),
            "{:?}",
            audit_plan(&deployed_plan)
        );
        assert!(
            audit_plan(&bartizan_plan).is_empty(),
            "{:?}",
            audit_plan(&bartizan_plan)
        );
        assert!(
            plan.projected_defenses
                .iter()
                .any(|defense| { defense.kind == ProjectedDefenseKind::Machicolation })
        );
        assert!(
            breteche_plan
                .projected_defenses
                .iter()
                .any(|defense| { defense.kind == ProjectedDefenseKind::Breteche })
        );
        assert!(
            bartizan_plan
                .projected_defenses
                .iter()
                .any(|defense| { defense.kind == ProjectedDefenseKind::Bartizan })
        );
        assert!(plan.projected_defenses.iter().any(|defense| {
            defense.kind == ProjectedDefenseKind::Hoarding
                && defense.deployment == ProjectedDefenseDeployment::SocketsOnly
        }));
        assert!(deployed_plan.projected_defenses.iter().any(|defense| {
            defense.kind == ProjectedDefenseKind::Hoarding
                && defense.deployment == ProjectedDefenseDeployment::Deployed
        }));
        let orientations = [&plan, &breteche_plan, &deployed_plan]
            .into_iter()
            .flat_map(|plan| plan.projected_defenses.iter())
            .filter_map(|defense| match defense.path {
                ProjectedDefensePath::Linear { outward, .. } => Some(outward),
                ProjectedDefensePath::Round { .. } => None,
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(orientations.len(), 4);

        let operational_index = plan
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Machicolation)
            .unwrap();
        let bartizan_index = bartizan_plan
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Bartizan)
            .unwrap();
        let deployed_hoarding_index = deployed_plan
            .projected_defenses
            .iter()
            .position(|defense| {
                defense.kind == ProjectedDefenseKind::Hoarding
                    && defense.deployment == ProjectedDefenseDeployment::Deployed
            })
            .unwrap();

        let mut witness_only_host = fixture(149);
        witness_only_host.projected_defenses[operational_index].host_owner =
            witness_only_host.projected_defenses[operational_index].owner;
        assert!(has(&witness_only_host, "unresolved_projected_defense_host"));

        let mut intact_host_portal = fixture(149);
        let host_portal = intact_host_portal.projected_defenses[operational_index]
            .host_portal_void
            .unwrap();
        intact_host_portal
            .resolved_geometry
            .voids
            .retain(|void| void.id != host_portal);
        assert!(has(
            &intact_host_portal,
            "unresolved_projected_defense_host"
        ));

        let mut shifted_host_walk = fixture(149);
        let host_walk = shifted_host_walk.projected_defenses[operational_index].host_walk_solid;
        shifted_host_walk
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == host_walk)
            .unwrap()
            .centre += Vec3::new(2.0, 0.0, 2.0);
        assert!(has(&shifted_host_walk, "inaccessible_projected_defense"));

        let mut untrimmed_host_run = fixture(149);
        if let ProjectedDefensePath::Linear { start, end, .. } =
            &mut untrimmed_host_run.projected_defenses[operational_index].path
        {
            let tangent = (*end - *start).normalize_or_zero();
            *start -= tangent;
            *end += tangent;
        }
        assert!(has(
            &untrimmed_host_run,
            "unresolved_projected_defense_host"
        ));

        let mut blocked_tower_chord = fixture(149);
        let tower_index = blocked_tower_chord
            .towers
            .iter()
            .position(|tower| tower.chord_interface.is_some())
            .unwrap();
        blocked_tower_chord.towers[tower_index]
            .chord_interface
            .as_mut()
            .unwrap()
            .bearing_depth = crate::GridLength::new(1).unwrap();
        assert!(has(&blocked_tower_chord, "blocked_projected_defense_ray"));

        let mut missing_return_chord = fixture(202);
        missing_return_chord.towers[0].secondary_chord_interface = None;
        assert!(has(&missing_return_chord, "invalid_round_wall_authority"));

        let sockets_index = plan
            .projected_defenses
            .iter()
            .position(|defense| defense.deployment == ProjectedDefenseDeployment::SocketsOnly)
            .unwrap();
        let mut filled_socket = fixture(149);
        let socket = filled_socket.projected_defenses[sockets_index].beam_socket_voids[0];
        filled_socket
            .resolved_geometry
            .voids
            .retain(|void| void.id != socket);
        assert!(has(&filled_socket, "invalid_hoarding_beam_sockets"));

        let mut shifted_joist = fixture(202);
        let (_, joist) = shifted_joist.projected_defenses[deployed_hoarding_index].socket_joists[0];
        shifted_joist
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == joist)
            .unwrap()
            .centre += Vec3::new(2.0, 0.0, 2.0);
        assert!(has(&shifted_joist, "invalid_hoarding_beam_sockets"));

        let mut sealed = fixture(149);
        let throat = sealed.projected_defenses[operational_index].throat_voids[0];
        sealed
            .resolved_geometry
            .voids
            .retain(|void| void.id != throat);
        assert!(has(&sealed, "sealed_projected_defense_throat"));

        let mut no_ray = fixture(149);
        let owner = no_ray.projected_defenses[operational_index].owner;
        no_ray
            .resolved_geometry
            .projected_defense_rays
            .retain(|ray| ray.owner != owner);
        assert!(has(&no_ray, "sealed_projected_defense_throat"));

        let mut below_floor_ray = fixture(149);
        let owner = below_floor_ray.projected_defenses[operational_index].owner;
        below_floor_ray
            .resolved_geometry
            .projected_defense_rays
            .iter_mut()
            .find(|ray| ray.owner == owner)
            .unwrap()
            .origin
            .y = below_floor_ray.projected_defenses[operational_index].floor_elevation_metres - 0.2;
        assert!(has(&below_floor_ray, "blocked_projected_defense_ray"));

        let mut missing_far_range = fixture(149);
        let owner = missing_far_range.projected_defenses[operational_index].owner;
        let aperture = missing_far_range
            .resolved_geometry
            .projected_defense_working_points
            .iter()
            .find(|point| point.owner == owner)
            .unwrap()
            .aperture;
        missing_far_range
            .resolved_geometry
            .projected_defense_rays
            .retain(|ray| {
                !(ray.owner == owner
                    && ray.throat == aperture
                    && ray.range == crate::ProjectedDefenseRange::Far)
            });
        assert!(has(
            &missing_far_range,
            "inoperable_projected_defense_station"
        ));

        let mut inward = fixture(149);
        let owner = inward.projected_defenses[operational_index].owner;
        let ray = inward
            .resolved_geometry
            .projected_defense_rays
            .iter_mut()
            .find(|ray| ray.owner == owner)
            .unwrap();
        ray.target.z = ray.origin.z + 2.0;
        assert!(has(&inward, "blocked_projected_defense_ray"));

        let mut reversed_assembly = fixture(149);
        if let ProjectedDefensePath::Linear { outward, .. } =
            &mut reversed_assembly.projected_defenses[operational_index].path
        {
            *outward = outward.opposite();
        }
        assert!(has(&reversed_assembly, "inward_projected_defense"));

        let mut blocked = fixture(149);
        let owner = blocked.projected_defenses[operational_index].owner;
        let ray = *blocked
            .resolved_geometry
            .projected_defense_rays
            .iter()
            .find(|ray| ray.owner == owner)
            .unwrap();
        let blocker = blocked
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner != owner)
            .unwrap();
        blocker.centre = ray.origin.lerp(ray.target, 0.45);
        blocker.size = Vec3::splat(0.45);
        assert!(has(&blocked, "blocked_projected_defense_ray"));

        let mut no_support = fixture(149);
        let support = no_support.projected_defenses[operational_index].support_nodes[0];
        no_support
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == support)
            .unwrap()
            .supported_by
            .clear();
        assert!(has(&no_support, "unsupported_projected_defense"));

        let mut inadequate_bearing = fixture(149);
        let support = inadequate_bearing.projected_defenses[operational_index].support_nodes[0];
        let bearing = inadequate_bearing
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|bearing| bearing.node == support)
            .unwrap();
        bearing.bounds.max.x = bearing.bounds.min.x + 0.02;
        bearing.bounds.max.z = bearing.bounds.min.z + 0.02;
        assert!(has(&inadequate_bearing, "unsupported_projected_defense"));

        let mut no_portal = fixture(149);
        no_portal.projected_defenses[operational_index].access_portal = None;
        assert!(has(&no_portal, "inaccessible_projected_defense"));
        let mut no_landing = fixture(149);
        no_landing.projected_defenses[operational_index].access_landing = None;
        assert!(has(&no_landing, "inaccessible_projected_defense"));
        let mut narrow = fixture(149);
        narrow.projected_defenses[operational_index].clear_width_metres = 0.7;
        assert!(has(&narrow, "insufficient_projected_defense_clearance"));
        let mut low = fixture(149);
        low.projected_defenses[operational_index].clear_height_metres = 1.7;
        assert!(has(&low, "insufficient_projected_defense_clearance"));

        let mut gallery_overlap = fixture(149);
        let throat = gallery_overlap.projected_defenses[operational_index].throat_voids[0];
        let bounds = gallery_overlap
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == throat)
            .unwrap()
            .bounds;
        let floor_id = gallery_overlap.projected_defenses[operational_index].floor_solids[0];
        let floor = gallery_overlap
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap();
        floor.centre = (bounds.min + bounds.max) * 0.5;
        floor.size = (bounds.max - bounds.min) + Vec3::splat(0.1);
        assert!(has(&gallery_overlap, "unresolved_void_subtraction"));

        let mut closed_bartizan = fixture(203);
        let centre = match closed_bartizan.projected_defenses[bartizan_index].path {
            ProjectedDefensePath::Round { centre, .. } => centre,
            ProjectedDefensePath::Linear { .. } => unreachable!(),
        };
        let floor_id = closed_bartizan.projected_defenses[bartizan_index].floor_solids[0];
        let floor = closed_bartizan
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap();
        floor.centre = Vec3::new(
            centre.x,
            closed_bartizan.projected_defenses[bartizan_index].floor_elevation_metres + 1.0,
            centre.y,
        );
        floor.size = Vec3::splat(0.5);
        assert!(has(&closed_bartizan, "closed_bartizan"));

        let mut dangling_frame = fixture(202);
        let owner = dangling_frame.projected_defenses[deployed_hoarding_index].owner;
        dangling_frame
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::FrameMember)
            .unwrap()
            .supported_by = vec![crate::StructuralNodeId(u64::MAX)];
        assert!(has(&dangling_frame, "dangling_hoarding_frame"));

        let mut no_drain = fixture(149);
        no_drain.projected_defenses[operational_index].drain_route = None;
        assert!(has(&no_drain, "projected_defense_roof_drain_failure"));
        let mut flat_gallery = fixture(149);
        let floor = flat_gallery.projected_defenses[operational_index].floor_solids[0];
        let floor = flat_gallery
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor)
            .unwrap();
        floor.crossfall_radians = 0.0;
        floor.longfall_radians = 0.0;
        assert!(has(&flat_gallery, "projected_defense_roof_drain_failure"));
        let mut raised_channel = fixture(149);
        let catchment = raised_channel.projected_defenses[operational_index].drainage_catchments[0];
        let channel = raised_channel
            .resolved_geometry
            .drainage_catchments
            .iter()
            .find(|candidate| candidate.id == catchment)
            .unwrap()
            .toe_channel_solids[0];
        raised_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .centre
            .y += 0.2;
        assert!(has(&raised_channel, "projected_defense_roof_drain_failure"));
        let mut reversed_channel = fixture(149);
        let catchment =
            reversed_channel.projected_defenses[operational_index].drainage_catchments[0];
        let channel = reversed_channel
            .resolved_geometry
            .drainage_catchments
            .iter()
            .find(|candidate| candidate.id == catchment)
            .unwrap()
            .toe_channel_solids[0];
        reversed_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .unwrap()
            .longfall_radians *= -1.0;
        assert!(has(
            &reversed_channel,
            "projected_defense_roof_drain_failure"
        ));
        let mut throat_drain = fixture(149);
        let drain = throat_drain.projected_defenses[operational_index]
            .drain_route
            .unwrap();
        let throat = throat_drain.projected_defenses[operational_index].throat_voids[0];
        throat_drain
            .resolved_geometry
            .drainage_routes
            .iter_mut()
            .find(|route| route.id == drain)
            .unwrap()
            .outlet_void = throat;
        assert!(has(&throat_drain, "projected_defense_roof_drain_failure"));
        let mut no_roof = fixture(202);
        let owner = no_roof.projected_defenses[deployed_hoarding_index].owner;
        no_roof
            .resolved_geometry
            .solids
            .retain(|solid| !(solid.owner == owner && solid.role == SolidRole::DefenseRoof));
        assert!(has(&no_roof, "projected_defense_roof_drain_failure"));
        let mut flat_roof = fixture(202);
        let owner = flat_roof.projected_defenses[deployed_hoarding_index].owner;
        let roof = flat_roof
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::DefenseRoof)
            .unwrap();
        roof.crossfall_radians = 0.0;
        roof.longfall_radians = 0.0;
        assert!(has(&flat_roof, "projected_defense_roof_drain_failure"));
        let mut phase_mismatch = fixture(202);
        phase_mismatch.projected_defenses[deployed_hoarding_index].material =
            ProjectedDefenseMaterial::Masonry;
        assert!(has(
            &phase_mismatch,
            "projected_defense_phase_material_mismatch"
        ));
        let mut no_aperture = fixture(203);
        no_aperture.projected_defenses[bartizan_index]
            .firing_apertures
            .clear();
        assert!(has(&no_aperture, "closed_bartizan"));
        let mut wrong_target = fixture(201);
        let index = wrong_target
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Breteche)
            .unwrap();
        wrong_target.projected_defenses[index].tactical_target =
            ProjectedDefenseTarget::CampaignSiegeFront;
        assert!(has(
            &wrong_target,
            "projected_defense_tactical_target_mismatch"
        ));

        let mut duplicate_host_screen = fixture(149);
        let defense = &mut duplicate_host_screen.projected_defenses[operational_index];
        let mut duplicate = duplicate_host_screen
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == defense.host_wall_solids[0])
            .unwrap()
            .clone();
        duplicate.id = crate::ResolvedItemId(u64::MAX - 20);
        defense.host_wall_solids.push(duplicate.id);
        duplicate_host_screen
            .resolved_geometry
            .solids
            .push(duplicate);
        assert!(has(
            &duplicate_host_screen,
            "unresolved_projected_defense_host"
        ));

        let mut overheight_host = fixture(149);
        let host = overheight_host.projected_defenses[operational_index].host_wall_solids[0];
        let host = overheight_host
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == host)
            .unwrap();
        host.centre.y += 0.5;
        host.size.y += 1.0;
        assert!(has(&overheight_host, "unresolved_projected_defense_host"));

        let mut roof_intrusion = fixture(202);
        let host_id =
            roof_intrusion.projected_defenses[deployed_hoarding_index].host_wall_solids[0];
        let host = roof_intrusion
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == host_id)
            .unwrap()
            .clone();
        let owner = roof_intrusion.projected_defenses[deployed_hoarding_index].owner;
        let roof = roof_intrusion
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::DefenseRoof)
            .unwrap();
        roof.centre = host.centre;
        roof.size = host.size;
        assert!(has(&roof_intrusion, "unresolved_projected_defense_host"));

        let mut one_face_bartizan = fixture(203);
        one_face_bartizan.projected_defenses[bartizan_index].host_topology =
            crate::ProjectedDefenseHostTopology::LinearFace;
        one_face_bartizan.projected_defenses[bartizan_index]
            .host_buttress_solids
            .clear();
        assert!(has(&one_face_bartizan, "unresolved_projected_defense_host"));

        let mut disconnected_roof = fixture(202);
        let catchment =
            disconnected_roof.projected_defenses[deployed_hoarding_index].weather_catchments[0];
        disconnected_roof
            .resolved_geometry
            .drainage_catchments
            .retain(|candidate| candidate.id != catchment);
        assert!(has(
            &disconnected_roof,
            "projected_defense_roof_drain_failure"
        ));

        let mut trapped_host_edge = fixture(202);
        let owner = trapped_host_edge.projected_defenses[deployed_hoarding_index].owner;
        trapped_host_edge
            .resolved_geometry
            .solids
            .retain(|solid| !(solid.owner == owner && solid.role == SolidRole::RoofFlashing));
        assert!(has(
            &trapped_host_edge,
            "projected_defense_roof_drain_failure"
        ));

        let mut missing_coping = fixture(149);
        let owner = missing_coping.projected_defenses[operational_index].owner;
        missing_coping
            .resolved_geometry
            .solids
            .retain(|solid| !(solid.owner == owner && solid.role == SolidRole::Coping));
        assert!(has(&missing_coping, "projected_defense_roof_drain_failure"));

        let mut inward_drip = fixture(149);
        let owner = inward_drip.projected_defenses[operational_index].owner;
        inward_drip
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::Coping)
            .unwrap()
            .crossfall_radians *= -1.0;
        assert!(has(&inward_drip, "projected_defense_roof_drain_failure"));

        let breteche_index = fixture(201)
            .projected_defenses
            .iter()
            .position(|defense| defense.kind == ProjectedDefenseKind::Breteche)
            .unwrap();
        let mut raised_breteche_roof = fixture(201);
        let owner = raised_breteche_roof.projected_defenses[breteche_index].owner;
        raised_breteche_roof
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::DefenseRoof)
            .unwrap()
            .centre
            .y += 0.35;
        assert!(has(
            &raised_breteche_roof,
            "unsupported_projected_defense_roof"
        ));

        let mut removed_breteche_post = fixture(201);
        let post = removed_breteche_post.projected_defenses[breteche_index]
            .roof_support_solids
            .iter()
            .copied()
            .find(|id| {
                removed_breteche_post
                    .resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::FrameMember)
            })
            .unwrap();
        removed_breteche_post
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != post);
        assert!(has(
            &removed_breteche_post,
            "unsupported_projected_defense_roof"
        ));

        let mut shortened_breteche_post = fixture(201);
        let post = shortened_breteche_post.projected_defenses[breteche_index]
            .roof_support_solids
            .iter()
            .copied()
            .find(|id| {
                shortened_breteche_post
                    .resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::FrameMember)
            })
            .unwrap();
        shortened_breteche_post
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == post)
            .unwrap()
            .size
            .y -= 0.4;
        assert!(has(
            &shortened_breteche_post,
            "unsupported_projected_defense_roof"
        ));

        let mut shifted_breteche_post = fixture(201);
        let post = shifted_breteche_post.projected_defenses[breteche_index]
            .roof_support_solids
            .iter()
            .copied()
            .find(|id| {
                shifted_breteche_post
                    .resolved_geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::FrameMember)
            })
            .unwrap();
        shifted_breteche_post
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == post)
            .unwrap()
            .centre
            .x += 0.55;
        assert!(has(
            &shifted_breteche_post,
            "unsupported_projected_defense_roof"
        ));
    }

    #[test]
    fn audit_rejects_a_disconnected_or_vertically_broken_fighting_circuit() {
        let mut disconnected = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            51,
        ))
        .unwrap();
        disconnected.defensive_junctions.clear();
        assert!(
            audit_plan(&disconnected)
                .iter()
                .any(|issue| issue.code == "disconnected_defensive_circuit")
        );

        // Each isolated deck still has its own ground stair. A multi-source
        // reachability pass would incorrectly accept this as one circuit.
        let mut separately_accessible = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            56,
        ))
        .unwrap();
        separately_accessible.wall_walks = separately_accessible
            .wall_walks
            .iter()
            .copied()
            .filter(|walk| matches!(walk, WallWalk::Round { .. }))
            .take(2)
            .collect();
        separately_accessible.defensive_junctions.clear();
        separately_accessible.defensive_circuits = vec![DefensiveCircuit {
            label: "deliberately broken test circuit".to_owned(),
            walks: vec![0, 1],
        }];
        separately_accessible.battlements.clear();
        separately_accessible.towers.truncate(2);
        separately_accessible.stairs.retain(|stair| {
            separately_accessible.towers.iter().any(|tower| {
                matches!(
                    stair,
                    Stair::Spiral { centre, .. } if close_vec(*centre, tower.centre_metres())
                )
            })
        });
        assert!(
            audit_plan(&separately_accessible)
                .iter()
                .any(|issue| issue.code == "disconnected_defensive_circuit")
        );

        let mut broken = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            52,
        ))
        .unwrap();
        let junction = broken.defensive_junctions.first_mut().unwrap();
        junction.kind = DefensiveJunctionKind::LevelLanding;
        if let WallWalk::Linear {
            elevation_metres, ..
        } = &mut broken.wall_walks[junction.walk_a]
        {
            *elevation_metres += 0.5;
        } else if let WallWalk::Linear {
            elevation_metres, ..
        } = &mut broken.wall_walks[junction.walk_b]
        {
            *elevation_metres += 0.5;
        }
        assert!(
            audit_plan(&broken)
                .iter()
                .any(|issue| issue.code == "wall_walk_vertical_discontinuity")
        );

        let mut cramped = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            54,
        ))
        .unwrap();
        cramped.defensive_junctions[0].width_metres = 0.7;
        cramped.defensive_junctions[0].clear_height_metres = 1.7;
        assert!(
            audit_plan(&cramped)
                .iter()
                .any(|issue| issue.code == "insufficient_walk_clearance")
        );
    }

    #[test]
    fn audit_rejects_a_roof_intruding_into_the_wall_walk() {
        let mut plan = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            53,
        ))
        .unwrap();
        plan.roofs[0].centre.y = 0.4;
        assert!(
            audit_plan(&plan)
                .iter()
                .any(|issue| issue.code == "wall_walk_roof_obstruction")
        );
    }

    #[test]
    fn audit_requires_physical_portals_behind_tower_graph_edges() {
        let mut plan = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            57,
        ))
        .unwrap();
        let portal_index = plan
            .tower_portals
            .iter()
            .position(|portal| matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. }))
            .unwrap();
        plan.tower_portals.remove(portal_index);
        assert!(
            audit_plan(&plan)
                .iter()
                .any(|issue| issue.code == "missing_tower_portal")
        );

        let mut no_entrance = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            58,
        ))
        .unwrap();
        no_entrance.tower_portals.retain(|portal| {
            !(portal.tower_index == 0 && portal.kind == TowerPortalKind::GroundStairEntrance)
        });
        assert!(
            audit_plan(&no_entrance)
                .iter()
                .any(|issue| issue.code == "missing_tower_portal")
        );
    }

    #[test]
    fn audit_enforces_the_declared_walled_keep_defensive_profile() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::WalledKeep,
                55,
            ))
            .unwrap()
        };
        let mut thin = fixture();
        thin.curtain_walls[0].thickness_metres = 0.8;
        thin.towers[0].wall_thickness_metres = 0.7;
        assert!(
            audit_plan(&thin)
                .iter()
                .any(|issue| issue.code == "wall_too_thin_for_profile")
        );

        let mut undefended = fixture();
        let gate_centre =
            (undefended.curtain_walls[0].start + undefended.curtain_walls[0].end) * 0.5;
        undefended
            .towers
            .retain(|tower| (tower.centre_metres() - gate_centre).length() > 8.0);
        assert!(
            audit_plan(&undefended)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut blind = fixture();
        for position in &mut blind.gate_defenses[0].firing_positions {
            position.direction = -position.direction;
        }
        assert!(
            audit_plan(&blind)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut origin_inside = fixture();
        let tower =
            origin_inside.towers[origin_inside.gate_defenses[0].firing_positions[0].tower_index];
        origin_inside.gate_defenses[0].firing_positions[0].origin = tower.centre_metres();
        assert!(
            audit_plan(&origin_inside)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut duplicate_aperture = fixture();
        let duplicate = duplicate_aperture.gate_defenses[0].firing_positions[0];
        duplicate_aperture.gate_defenses[0].firing_positions = vec![duplicate, duplicate];
        assert!(
            audit_plan(&duplicate_aperture)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut missing_aperture = fixture();
        missing_aperture.gate_defenses[0].firing_positions[0].aperture_width_metres = 0.0;
        assert!(
            audit_plan(&missing_aperture)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut rotated_aperture = fixture();
        rotated_aperture.gate_defenses[0].firing_positions[0].aperture_normal =
            -rotated_aperture.gate_defenses[0].firing_positions[0].aperture_normal;
        assert!(
            audit_plan(&rotated_aperture)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut wall_occluded = fixture();
        let firing = wall_occluded.gate_defenses[0].firing_positions[0];
        let target = wall_occluded.gate_defenses[0].approach;
        assert!(ray_clear_of_solids(
            &wall_occluded,
            &firing,
            target,
            wall_occluded.gate_defenses[0].curtain_wall_index,
        ));
        let midpoint = firing.origin.lerp(target, 0.45);
        let perpendicular = Vec2::new(
            -(target - firing.origin).normalize().y,
            (target - firing.origin).normalize().x,
        );
        let mut blocker = wall_occluded.curtain_walls[1];
        blocker.start = midpoint - perpendicular;
        blocker.end = midpoint + perpendicular;
        blocker.height_metres = 3.0;
        blocker.thickness_metres = 0.6;
        blocker.gate_width_metres = None;
        wall_occluded.curtain_walls.push(blocker);
        assert!(!ray_clear_of_solids(
            &wall_occluded,
            &firing,
            target,
            wall_occluded.gate_defenses[0].curtain_wall_index,
        ));
        assert!(
            audit_plan(&wall_occluded)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut single_closure = fixture();
        single_closure.gate_defenses[0]
            .closures
            .retain(|closure| closure.kind != GateClosureKind::Portcullis);
        assert!(
            audit_plan(&single_closure)
                .iter()
                .any(|issue| issue.code == "undefended_gate")
        );

        let mut unsupported_chamber = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = &mut unsupported_chamber.gate_defenses[0].guard_chamber.load_path;
        *left_tower_index = usize::MAX;
        assert!(
            audit_plan(&unsupported_chamber)
                .iter()
                .any(|issue| issue.code == "unsupported_guard_chamber")
        );

        let mut inaccessible_chamber = fixture();
        inaccessible_chamber.gate_defenses[0]
            .guard_chamber
            .access
            .envelope
            .width_metres = 0.6;
        assert!(
            audit_plan(&inaccessible_chamber)
                .iter()
                .any(|issue| issue.code == "inaccessible_guard_chamber")
        );

        let mut inoperable_chamber = fixture();
        inoperable_chamber.gate_defenses[0]
            .guard_chamber
            .openings
            .clear();
        assert!(
            audit_plan(&inoperable_chamber)
                .iter()
                .any(|issue| issue.code == "inoperable_guard_chamber")
        );

        let mut misaligned_windlass = fixture();
        misaligned_windlass.gate_defenses[0]
            .guard_chamber
            .operating_positions[0]
            .position = misaligned_windlass.gate_defenses[0].guard_chamber.centre;
        assert!(
            audit_plan(&misaligned_windlass)
                .iter()
                .any(|issue| issue.code == "inoperable_guard_chamber")
        );

        let mut unflanked = fixture();
        unflanked.curtain_walls[1].end.y += 40.0;
        assert!(
            audit_plan(&unflanked)
                .iter()
                .any(|issue| issue.code == "unflanked_curtain")
        );

        let mut thin_courtyard = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::CourtyardCastle,
            59,
        ))
        .unwrap();
        thin_courtyard.towers[0].wall_thickness_metres = 0.35;
        assert!(
            audit_plan(&thin_courtyard)
                .iter()
                .any(|issue| issue.code == "wall_too_thin_for_profile")
        );
    }

    #[test]
    fn gatehouse_geometry_audit_rejects_passage_room_load_splice_and_aperture_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::WalledKeep,
                67,
            ))
            .unwrap()
        };
        let has =
            |plan: &BuildingPlan, code| audit_plan(plan).iter().any(|issue| issue.code == code);

        let mut blocked_passage = fixture();
        let threshold = blocked_passage.gate_defenses[0].threshold;
        let floor_elevation = blocked_passage.gate_defenses[0]
            .guard_chamber
            .floor_elevation_metres;
        blocked_passage.gate_defenses[0]
            .guard_chamber
            .supports
            .push(crate::GuardChamberSupport {
                centre: threshold,
                size: Vec2::splat(0.5),
                base_elevation_metres: 0.0,
                top_elevation_metres: floor_elevation,
            });
        assert!(has(&blocked_passage, "gate_passage_clear"));

        let mut room_collision = fixture();
        room_collision.gate_defenses[0]
            .guard_chamber
            .floor_elevation_metres = 3.0;
        assert!(has(&room_collision, "room_void_disjoint_from_solids"));

        let mut lateral_room_collision = fixture();
        lateral_room_collision.gate_defenses[0]
            .guard_chamber
            .centre
            .x += 0.4;
        assert!(has(
            &lateral_room_collision,
            "room_void_disjoint_from_solids"
        ));

        let mut deep_room_collision = fixture();
        deep_room_collision.gate_defenses[0].guard_chamber.size.y += 0.4;
        assert!(has(&deep_room_collision, "room_void_disjoint_from_solids"));

        let mut missing_load = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = &mut missing_load.gate_defenses[0].guard_chamber.load_path;
        *left_tower_index = usize::MAX;
        assert!(has(&missing_load, "declared_load_path"));

        let mut lowered_arch = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            arch_spring_elevation_metres,
            ..
        } = &mut lowered_arch.gate_defenses[0].guard_chamber.load_path;
        *arch_spring_elevation_metres -= 0.3;
        assert!(has(&lowered_arch, "gate_passage_clear"));

        let mut thick_arch = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            arch_ring_depth, ..
        } = &mut thick_arch.gate_defenses[0].guard_chamber.load_path;
        *arch_ring_depth = crate::GridLength::new(20).unwrap();
        assert!(has(&thick_arch, "gate_passage_clear"));

        let mut undertrimmed_return = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            curtain_return_bond,
            ..
        } = &mut undertrimmed_return.gate_defenses[0].guard_chamber.load_path;
        *curtain_return_bond = crate::GridLength::new(1).unwrap();
        assert!(has(&undertrimmed_return, "round_rect_splice"));

        let mut short_rectangular_closures = fixture();
        for closure in &mut short_rectangular_closures.gate_defenses[0].closures {
            closure.coverage.spring_height_metres = 3.24;
            closure.coverage.arch_rise_metres = 0.0;
        }
        assert!(has(&short_rectangular_closures, "unsealed_gate_passage"));

        let mut support_in_tower = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = support_in_tower.gate_defenses[0].guard_chamber.load_path;
        let centre = support_in_tower.towers[left_tower_index].centre_metres();
        let floor = support_in_tower.gate_defenses[0]
            .guard_chamber
            .floor_elevation_metres;
        support_in_tower.gate_defenses[0]
            .guard_chamber
            .supports
            .push(crate::GuardChamberSupport {
                centre,
                size: Vec2::splat(0.6),
                base_elevation_metres: 0.0,
                top_elevation_metres: floor,
            });
        assert!(has(&support_in_tower, "undeclared_solid_overlap"));

        let mut bad_splice = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = bad_splice.gate_defenses[0].guard_chamber.load_path;
        bad_splice.towers[left_tower_index].chord_interface = None;
        assert!(has(&bad_splice, "round_rect_splice"));

        let mut unresolved_aperture = fixture();
        unresolved_aperture.gate_defenses[0].firing_positions[0].aperture_width_metres = 0.08;
        assert!(has(&unresolved_aperture, "aperture_clearance"));

        let mut overlap = fixture();
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index, ..
        } = overlap.gate_defenses[0].guard_chamber.load_path;
        let tower = overlap.towers[left_tower_index];
        let shifted_anchor = crate::GridPoint::new(
            tower.anchor().x,
            tower.anchor().z + crate::GRID_UNITS_PER_CELL,
        );
        overlap.towers[left_tower_index] = crate::RoundTower::new(
            shifted_anchor,
            tower.diameter(),
            tower.wall_height_metres,
            tower.wall_thickness_metres,
            tower.roof,
            tower.battlement,
        )
        .expect("a one-cell shift preserves even-diameter anchor parity")
        .with_chord_interface(tower.chord_interface.unwrap());
        assert!(has(&overlap, "undeclared_solid_overlap"));

        let mut drift = fixture();
        drift.curtain_walls[0].gate_width_metres = Some(4.0);
        assert!(has(&drift, "gatehouse_spec_drift"));

        let mut diagonal = fixture();
        diagonal.curtain_walls[0].end.y += 1.0;
        assert!(has(&diagonal, "invalid_gatehouse_orientation"));

        let mut mismatched_outward = fixture();
        mismatched_outward.curtain_walls[0].outward = Direction::East;
        assert!(has(&mismatched_outward, "invalid_gatehouse_orientation"));
    }

    #[test]
    fn guard_access_audit_sweeps_landings_stair_door_roof_and_operating_clearances() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::WalledKeep,
                71,
            ))
            .unwrap()
        };
        let has =
            |plan: &BuildingPlan, code| audit_plan(plan).iter().any(|issue| issue.code == code);
        let mut low_ceiling = fixture();
        low_ceiling.gate_defenses[0]
            .guard_chamber
            .clear_height_metres = 1.5;
        assert!(has(&low_ceiling, "inaccessible_guard_chamber"));
        let mut missing_roof_cut = fixture();
        missing_roof_cut.gate_defenses[0]
            .guard_chamber
            .access
            .roof_clearance_opening
            .size = Vec2::splat(0.2);
        assert!(has(&missing_roof_cut, "inaccessible_guard_chamber"));
        let mut short_door = fixture();
        short_door.gate_defenses[0]
            .guard_chamber
            .access
            .door
            .clear_height_metres = 1.2;
        assert!(has(&short_door, "inaccessible_guard_chamber"));
        let mut small_top = fixture();
        small_top.gate_defenses[0]
            .guard_chamber
            .access
            .top_landing
            .size = Vec2::splat(0.5);
        assert!(has(&small_top, "inaccessible_guard_chamber"));
        let mut absent_bottom = fixture();
        absent_bottom.gate_defenses[0]
            .guard_chamber
            .access
            .bottom_landing
            .size = Vec2::ZERO;
        assert!(has(&absent_bottom, "inaccessible_guard_chamber"));
        let mut short_going = fixture();
        short_going.gate_defenses[0]
            .guard_chamber
            .access
            .flight
            .going_metres = 0.15;
        assert!(has(&short_going, "inaccessible_guard_chamber"));
        let mut hole_collision = fixture();
        let flight_mid = {
            let flight = hole_collision.gate_defenses[0].guard_chamber.access.flight;
            flight.top.lerp(flight.bottom, 0.5)
        };
        hole_collision.gate_defenses[0].guard_chamber.openings[1].position = flight_mid;
        assert!(has(&hole_collision, "inaccessible_guard_chamber"));
        let mut windlass_collision = fixture();
        windlass_collision.gate_defenses[0]
            .guard_chamber
            .operating_positions[0]
            .position = flight_mid;
        assert!(has(&windlass_collision, "inaccessible_guard_chamber"));
        let mut wall_collision = fixture();
        let wall_y = wall_collision.gate_defenses[0].guard_chamber.centre.y;
        wall_collision.gate_defenses[0]
            .guard_chamber
            .access
            .flight
            .top
            .y = wall_y;
        wall_collision.gate_defenses[0]
            .guard_chamber
            .access
            .flight
            .bottom
            .y = wall_y;
        assert!(has(&wall_collision, "inaccessible_guard_chamber"));
        let mut route_drift = fixture();
        route_drift.gate_defenses[0]
            .guard_chamber
            .access
            .door
            .position
            .x += 0.25;
        assert!(has(&route_drift, "gatehouse_spec_drift"));
        let mut unsupported = fixture();
        unsupported.gate_defenses[0]
            .guard_chamber
            .access
            .support_posts
            .clear();
        assert!(has(&unsupported, "unsupported_guard_access"));
        let mut missing_end_guard = fixture();
        missing_end_guard.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards
            .remove(1);
        assert!(has(&missing_end_guard, "unsupported_guard_access"));
        let mut blocked_connection = fixture();
        let top = blocked_connection.gate_defenses[0]
            .guard_chamber
            .access
            .top_landing;
        blocked_connection.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards
            .push(crate::AccessGuardSegment {
                start: top.centre + Vec2::new(0.5, -0.7),
                end: top.centre + Vec2::new(0.5, 0.7),
                elevation_metres: top.elevation_metres,
                height_metres: 1.0,
            });
        assert!(has(&blocked_connection, "unsupported_guard_access"));
        let mut guard_gap = fixture();
        guard_gap.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards[0]
            .end
            .x -= 0.2;
        assert!(has(&guard_gap, "unsupported_guard_access"));
        let mut low_guard = fixture();
        low_guard.gate_defenses[0]
            .guard_chamber
            .access
            .landing_guards[0]
            .height_metres = 0.4;
        assert!(has(&low_guard, "unsupported_guard_access"));
        let mut unbraced = fixture();
        unbraced.gate_defenses[0]
            .guard_chamber
            .access
            .lateral_braces
            .clear();
        assert!(has(&unbraced, "unsupported_guard_access"));
        let mut dangling_start = fixture();
        dangling_start.gate_defenses[0]
            .guard_chamber
            .access
            .lateral_braces[0]
            .start_elevation_metres = 20.0;
        assert!(has(&dangling_start, "unsupported_guard_access"));
        let mut dangling_end = fixture();
        dangling_end.gate_defenses[0]
            .guard_chamber
            .access
            .lateral_braces[0]
            .end_elevation_metres = 20.0;
        assert!(has(&dangling_end, "unsupported_guard_access"));
        let mut floating_ledger = fixture();
        floating_ledger.gate_defenses[0]
            .guard_chamber
            .access
            .wall_ledger
            .centre
            .y += 1.0;
        assert!(has(&floating_ledger, "unsupported_guard_access"));
        let mut blocked_swing = fixture();
        blocked_swing.gate_defenses[0]
            .guard_chamber
            .access
            .door
            .swing_inward = false;
        assert!(has(&blocked_swing, "inaccessible_guard_chamber"));
        let mut closure_collision = fixture();
        closure_collision.gate_defenses[0].closures[1].inward_offset_metres = 1.9;
        assert!(has(&closure_collision, "inaccessible_guard_chamber"));
        let mut aperture_collision = fixture();
        aperture_collision.gate_defenses[0].firing_positions[0].origin = flight_mid;
        assert!(has(&aperture_collision, "inaccessible_guard_chamber"));
        let mut sightline_collision = fixture();
        sightline_collision.gate_defenses[0].approach = flight_mid;
        assert!(has(&sightline_collision, "inaccessible_guard_chamber"));
    }

    #[test]
    fn crown_audit_rejects_profile_geometry_junction_and_correspondence_regressions() {
        let fixture = || {
            crate::generate(&crate::BuildingProgram::fixture(
                crate::BuildingArchetype::CourtyardCastle,
                91,
            ))
            .unwrap()
        };
        let has =
            |plan: &BuildingPlan, code| audit_plan(plan).iter().any(|issue| issue.code == code);

        let mut foot_level = fixture();
        foot_level.crowns[0].profile.breastwork_height_metres = 0.1;
        assert!(has(&foot_level, "unsafe_crown_profile"));
        let mut low_cover = fixture();
        low_cover.crowns[0].profile.merlon_height_metres = 0.1;
        assert!(has(&low_cover, "unsafe_crown_profile"));
        let mut inward = fixture();
        if let CrownPath::Straight { outward, .. } = &mut inward.crowns[0].path {
            *outward = outward.opposite();
        }
        assert!(has(&inward, "crown_faces_inward"));
        let mut blocked = fixture();
        if let WallWalk::Linear { width_metres, .. } = &mut blocked.wall_walks[0] {
            *width_metres = 0.5;
        }
        assert!(has(&blocked, "blocked_crown_walk"));
        let owner = fixture().crowns[0].owner;
        let mut missing_coping = fixture();
        missing_coping
            .resolved_geometry
            .solids
            .retain(|solid| solid.owner != owner || solid.role != SolidRole::Coping);
        assert!(has(&missing_coping, "incomplete_crown_geometry"));
        let mut missing_drain = fixture();
        missing_drain
            .resolved_geometry
            .voids
            .retain(|void| void.owner != owner || void.role != VoidRole::Drain);
        assert!(has(&missing_drain, "incomplete_crown_geometry"));
        let mut exposed_edge = fixture();
        exposed_edge
            .resolved_geometry
            .solids
            .retain(|solid| solid.owner != owner || solid.role != SolidRole::EdgeGuard);
        assert!(has(&exposed_edge, "incomplete_crown_geometry"));
        let mut bad_splice = fixture();
        let round = bad_splice
            .crowns
            .iter_mut()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap();
        if let CrownPath::Round { radius_metres, .. } = &mut round.path {
            *radius_metres += 0.5;
        }
        assert!(has(&bad_splice, "bad_tower_crown_splice"));
        let mut bad_round_crenel = fixture();
        let round_owner = bad_round_crenel
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap()
            .owner;
        bad_round_crenel
            .resolved_geometry
            .solids
            .iter_mut()
            .filter(|solid| solid.owner == round_owner && solid.role == SolidRole::Merlon)
            .for_each(|solid| solid.size.x = 0.1);
        assert!(has(&bad_round_crenel, "invalid_round_crenel_interval"));
        let mut overgrown_splice = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            91,
        ))
        .unwrap();
        let (straight_owner, splice_position) = overgrown_splice
            .crowns
            .iter()
            .find_map(|crown| match crown.path {
                CrownPath::Straight { start, end, .. } => crown
                    .junctions
                    .iter()
                    .find(|junction| {
                        junction.kind == CrownJunctionKind::TowerSplice
                            && (junction.position - start).length() > 0.1
                            && (junction.position - end).length() > 0.1
                    })
                    .map(|junction| (crown.owner, junction.position)),
                CrownPath::Round { .. } => None,
            })
            .unwrap();
        let breastwork = overgrown_splice
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == straight_owner && solid.role == SolidRole::Breastwork)
            .unwrap();
        breastwork.centre.x = splice_position.x;
        breastwork.centre.z = splice_position.y;
        breastwork.size.x = 1.0;
        breastwork.size.z = 1.0;
        assert!(has(&overgrown_splice, "unresolved_tower_crown_splice"));
        let mut duplicate_corner = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            91,
        ))
        .unwrap();
        let corner = duplicate_corner
            .crowns
            .iter()
            .flat_map(|crown| crown.junctions.iter())
            .find(|junction| junction.kind == CrownJunctionKind::Corner)
            .copied()
            .unwrap();
        let duplicate = duplicate_corner
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| {
                solid.role == SolidRole::Merlon
                    && Vec2::new(solid.centre.x, solid.centre.z).distance(corner.position) < 0.08
            })
            .cloned()
            .unwrap();
        duplicate_corner.resolved_geometry.solids.push(duplicate);
        assert!(has(&duplicate_corner, "duplicate_junction_merlon"));
        let mut overlap = fixture();
        let mut duplicate = overlap.resolved_geometry.solids[0].clone();
        duplicate.owner = crate::GeometryOwnerId(99_999);
        duplicate.id = crate::ResolvedItemId(999_990);
        duplicate.supported_by = vec![crate::StructuralNodeId(999_990)];
        overlap
            .resolved_geometry
            .structural_nodes
            .push(crate::StructuralNode {
                id: crate::StructuralNodeId(999_990),
                owner: duplicate.owner,
                kind: crate::StructuralNodeKind::WallBearing,
                position: duplicate.centre,
                supported_by: Vec::new(),
                grounded: true,
            });
        overlap.resolved_geometry.solids.push(duplicate);
        assert!(has(&overlap, "undeclared_solid_overlap"));
        let mut open = fixture();
        open.resolved_geometry.solids[0].size.y = 0.0;
        assert!(has(&open, "invalid_resolved_geometry"));

        let mut stale_schema = fixture();
        stale_schema.resolved_geometry.schema_version = 1;
        assert!(has(&stale_schema, "stale_resolver_schema"));
        let mut duplicate_item = fixture();
        duplicate_item.resolved_geometry.solids[1].id =
            duplicate_item.resolved_geometry.solids[0].id;
        assert!(has(&duplicate_item, "invalid_resolved_geometry"));
        let mut unsupported = fixture();
        unsupported.resolved_geometry.structural_nodes[0].grounded = false;
        assert!(has(&unsupported, "unsupported_resolved_structure"));
        let mut no_bearing = fixture();
        let solid_id = no_bearing.resolved_geometry.solids[0].id;
        let owner = no_bearing.resolved_geometry.solids[0].owner;
        no_bearing
            .resolved_geometry
            .support_interfaces
            .retain(|bearing| bearing.owner != owner);
        assert!(has(&no_bearing, "missing_positive_bearing"), "{solid_id:?}");
        let mut interval_gap = fixture();
        let straight_owner = interval_gap
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Straight { .. }))
            .unwrap()
            .owner;
        let removed = interval_gap
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| {
                solid.owner == straight_owner
                    && solid.role == SolidRole::Breastwork
                    && solid.size.y > 0.2
            })
            .max_by(|a, b| a.size.max_element().total_cmp(&b.size.max_element()))
            .unwrap()
            .id;
        interval_gap
            .resolved_geometry
            .solids
            .retain(|solid| solid.id != removed);
        assert!(has(&interval_gap, "crown_interval_gap"));
        let mut no_defender_samples = fixture();
        no_defender_samples
            .resolved_geometry
            .defender_samples
            .retain(|sample| sample.owner != straight_owner);
        assert!(has(&no_defender_samples, "unusable_crown_firing_position"));
        let mut flat_coping = fixture();
        flat_coping
            .resolved_geometry
            .solids
            .iter_mut()
            .filter(|solid| solid.role == SolidRole::Coping)
            .for_each(|solid| solid.crossfall_radians = 0.0);
        assert!(has(&flat_coping, "bad_crown_coping"));
        let mut uphill_drain = fixture();
        uphill_drain.resolved_geometry.drainage_routes[0].outlet.y =
            uphill_drain.resolved_geometry.drainage_routes[0].inlet.y;
        assert!(has(&uphill_drain, "broken_crown_drainage"));
        let mut flat_catchment = fixture();
        let walk_id = flat_catchment.resolved_geometry.drainage_catchments[0].walk_solid;
        flat_catchment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == walk_id)
            .unwrap()
            .crossfall_radians = 0.0;
        assert!(has(&flat_catchment, "broken_crown_drainage"));
        let mut reversed_catchment = fixture();
        let walk_id = reversed_catchment.resolved_geometry.drainage_catchments[0].walk_solid;
        let walk = reversed_catchment
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == walk_id)
            .unwrap();
        walk.crossfall_radians = -walk.crossfall_radians;
        assert!(has(&reversed_catchment, "broken_crown_drainage"));
        let mut stalled_toe_channel = fixture();
        let channel_id =
            stalled_toe_channel.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        stalled_toe_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel_id)
            .unwrap()
            .longfall_radians = 0.0;
        assert!(has(&stalled_toe_channel, "broken_crown_drainage"));
        let mut wrong_inlet = fixture();
        let route_id = wrong_inlet.resolved_geometry.drainage_catchments[0].outlet_route;
        wrong_inlet
            .resolved_geometry
            .drainage_routes
            .iter_mut()
            .find(|route| route.id == route_id)
            .unwrap()
            .inlet
            .x += 0.4;
        assert!(has(&wrong_inlet, "broken_crown_drainage"));
        let mut local_basin = fixture();
        let channel_id = local_basin.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        let channel = local_basin
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel_id)
            .unwrap();
        channel.longfall_radians = channel.longfall_radians.abs();
        assert!(has(&local_basin, "broken_crown_drainage"));
        let mut raised_channel = fixture();
        let channel_id =
            raised_channel.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        raised_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel_id)
            .unwrap()
            .centre
            .y += 0.05;
        assert!(has(&raised_channel, "broken_crown_drainage"));
        let mut uncut_walk = fixture();
        let catchment = uncut_walk.resolved_geometry.drainage_catchments[0].clone();
        let walk = uncut_walk
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == catchment.walk_solid)
            .unwrap();
        walk.size.z = catchment.width_metres;
        walk.centre.x = catchment.centre.x;
        walk.centre.z = catchment.centre.z;
        assert!(has(&uncut_walk, "broken_crown_drainage"));
        let mut blocked_channel = fixture();
        let channel_id =
            blocked_channel.resolved_geometry.drainage_catchments[0].toe_channel_solids[0];
        let channel_centre = blocked_channel
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == channel_id)
            .unwrap()
            .centre;
        let owner = blocked_channel.resolved_geometry.drainage_catchments[0].owner;
        let blocker = blocked_channel
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.owner == owner && solid.role == SolidRole::Breastwork)
            .unwrap();
        blocker.centre = channel_centre;
        blocker.size = Vec3::splat(0.2);
        assert!(has(&blocked_channel, "broken_crown_drainage"));
        let mut missing_catchment = fixture();
        missing_catchment
            .resolved_geometry
            .drainage_catchments
            .remove(0);
        assert!(has(&missing_catchment, "broken_crown_drainage"));
        let mut isolated_basin = fixture();
        isolated_basin.resolved_geometry.drainage_catchments[0].outlet_route =
            crate::ResolvedItemId(u64::MAX);
        assert!(has(&isolated_basin, "broken_crown_drainage"));
        let mut missing_drainage_surface = fixture();
        let surface_id = missing_drainage_surface
            .resolved_geometry
            .drainage_catchments[0]
            .drainage_surface;
        missing_drainage_surface
            .resolved_geometry
            .surfaces
            .retain(|surface| surface.id != surface_id);
        assert!(has(&missing_drainage_surface, "broken_crown_drainage"));
        let mut blocked_scupper = fixture();
        let route_id = blocked_scupper.resolved_geometry.drainage_catchments[0].outlet_route;
        let route = *blocked_scupper
            .resolved_geometry
            .drainage_routes
            .iter()
            .find(|route| route.id == route_id)
            .unwrap();
        let outlet = *blocked_scupper
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == route.outlet_void)
            .unwrap();
        let walk_id = blocked_scupper.resolved_geometry.drainage_catchments[0].walk_solid;
        let blocker = blocked_scupper
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == walk_id)
            .unwrap();
        blocker.centre = (outlet.bounds.min + outlet.bounds.max) * 0.5;
        assert!(has(&blocked_scupper, "broken_crown_drainage"));

        let cardinal = crate::generate(&crate::BuildingProgram::fixture(
            crate::BuildingArchetype::WalledKeep,
            91,
        ))
        .unwrap();
        let mut seen = std::collections::HashSet::new();
        for crown in cardinal
            .crowns
            .iter()
            .filter(|crown| matches!(crown.path, CrownPath::Straight { .. }))
        {
            let CrownPath::Straight { outward, .. } = crown.path else {
                unreachable!()
            };
            if !seen.insert(outward) {
                continue;
            }
            let mut reversed = cardinal.clone();
            let walk_id = reversed
                .resolved_geometry
                .drainage_catchments
                .iter()
                .find(|catchment| catchment.owner == crown.owner)
                .unwrap()
                .walk_solid;
            let walk = reversed
                .resolved_geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == walk_id)
                .unwrap();
            walk.crossfall_radians = -walk.crossfall_radians;
            assert!(has(&reversed, "broken_crown_drainage"));
        }
        assert_eq!(seen.len(), 4);
        let mut reversed_round = fixture();
        let round_owner = reversed_round
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap()
            .owner;
        reversed_round
            .resolved_geometry
            .drainage_catchments
            .iter_mut()
            .find(|catchment| catchment.owner == round_owner)
            .unwrap()
            .outward *= -1.0;
        assert!(has(&reversed_round, "broken_crown_drainage"));
        let mut round_outer_edge_gap = fixture();
        let round_owner = round_outer_edge_gap
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap()
            .owner;
        round_outer_edge_gap
            .resolved_geometry
            .solids
            .iter_mut()
            .filter(|solid| solid.owner == round_owner && solid.role == SolidRole::WalkSurface)
            .for_each(|solid| solid.size.x *= 0.65);
        assert!(has(&round_outer_edge_gap, "broken_crown_drainage"));
        let mut missing_bond = fixture();
        missing_bond.resolved_geometry.junction_bonds.clear();
        assert!(has(&missing_bond, "missing_crown_junction_bond"));
        let mut shifted_bond = fixture();
        let bonded_overlap_index = shifted_bond
            .resolved_geometry
            .junction_bonds
            .iter()
            .position(|bond| {
                shifted_bond.resolved_geometry.solids.iter().any(|a| {
                    a.owner == bond.owners[0]
                        && shifted_bond.resolved_geometry.solids.iter().any(|b| {
                            b.owner == bond.owners[1]
                                && bounds_overlap_3d(
                                    resolved_solid_bounds(a),
                                    resolved_solid_bounds(b),
                                    0.025,
                                )
                        })
                })
            })
            .unwrap();
        shifted_bond.resolved_geometry.junction_bonds[bonded_overlap_index]
            .bounds
            .min
            .x += 1.0;
        shifted_bond.resolved_geometry.junction_bonds[bonded_overlap_index]
            .bounds
            .max
            .x += 1.0;
        assert!(has(&shifted_bond, "undeclared_solid_overlap"));
        let mut overpenetrated_bond = fixture();
        let bond = overpenetrated_bond.resolved_geometry.junction_bonds[bonded_overlap_index];
        let (target_centre, intruder_id) = overpenetrated_bond
            .resolved_geometry
            .solids
            .iter()
            .find_map(|a| {
                (a.owner == bond.owners[0]).then(|| {
                    overpenetrated_bond
                        .resolved_geometry
                        .solids
                        .iter()
                        .find(|b| {
                            b.owner == bond.owners[1]
                                && bounds_overlap_3d(
                                    resolved_solid_bounds(a),
                                    resolved_solid_bounds(b),
                                    0.025,
                                )
                        })
                        .map(|b| (a.centre, b.id))
                })?
            })
            .unwrap();
        let intruder = overpenetrated_bond
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == intruder_id)
            .unwrap();
        intruder.centre = target_centre;
        intruder.size = Vec3::splat(0.5);
        assert!(has(&overpenetrated_bond, "undeclared_solid_overlap"));

        let mut blocked_round_portal = fixture();
        let (round_owner, tower_index, centre, radius, base, thickness) = blocked_round_portal
            .crowns
            .iter()
            .find_map(|crown| match crown.path {
                CrownPath::Round {
                    tower_index,
                    centre,
                    radius_metres,
                } => Some((
                    crown.owner,
                    tower_index,
                    centre,
                    radius_metres,
                    crown.base_height_metres,
                    crown.profile.thickness_metres,
                )),
                CrownPath::Straight { .. } => None,
            })
            .unwrap();
        let facing = blocked_round_portal
            .tower_portals
            .iter()
            .find(|portal| {
                portal.tower_index == tower_index
                    && matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. })
            })
            .unwrap()
            .facing;
        let facing_vector = direction_vector(facing);
        let facing_angle = facing_vector.y.atan2(facing_vector.x);
        let blocker_template = blocked_round_portal
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.owner == round_owner && solid.role == SolidRole::Breastwork)
            .unwrap()
            .clone();
        for (serial, offset) in (-3..=3).enumerate() {
            let angle = facing_angle + offset as f32 * std::f32::consts::TAU / 24.0;
            let radial = Vec2::new(angle.cos(), angle.sin());
            let mut blocker = blocker_template.clone();
            blocker.id = crate::ResolvedItemId(999_991 + serial as u64);
            blocker.centre = Vec3::new(
                centre.x + radial.x * (radius + thickness * 0.5),
                base + 0.45,
                centre.y + radial.y * (radius + thickness * 0.5),
            );
            blocker.size = Vec3::new(1.2, 0.9, 1.2);
            blocked_round_portal.resolved_geometry.solids.push(blocker);
        }
        assert!(has(&blocked_round_portal, "blocked_round_crown_portal"));

        let mut blocked_spiral = fixture();
        let round = blocked_spiral
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Round { .. }))
            .unwrap();
        let (tower_index, centre) = match round.path {
            CrownPath::Round {
                tower_index,
                centre,
                ..
            } => (tower_index, centre),
            CrownPath::Straight { .. } => unreachable!(),
        };
        let stair = blocked_spiral
            .stairs
            .iter()
            .find(|stair| matches!(stair, Stair::Spiral { centre: stair_centre, .. } if (*stair_centre-centre).length()<0.02))
            .copied()
            .unwrap();
        let Stair::Spiral {
            turns,
            clockwise,
            tread_count,
            ..
        } = stair
        else {
            unreachable!()
        };
        let progress = f32::from(tread_count.saturating_sub(1)) / f32::from(tread_count.max(1));
        let angle = if clockwise { -1.0 } else { 1.0 } * progress * turns * std::f32::consts::TAU;
        let stairwell_radius = blocked_spiral
            .wall_walks
            .iter()
            .find_map(|walk| match *walk {
                WallWalk::Round {
                    centre: walk_centre,
                    stairwell_radius_metres,
                    ..
                } if (walk_centre - centre).length() < 0.02 => Some(stairwell_radius_metres),
                _ => None,
            })
            .unwrap();
        let radial = Vec2::new(angle.cos(), angle.sin());
        let mut guard = blocked_spiral
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.owner == round.owner && solid.role == SolidRole::EdgeGuard)
            .unwrap()
            .clone();
        guard.id = crate::ResolvedItemId(999_992);
        guard.centre.x = centre.x + radial.x * (stairwell_radius + 0.08);
        guard.centre.z = centre.y + radial.y * (stairwell_radius + 0.08);
        guard.size.x = 1.0;
        guard.size.z = 1.0;
        blocked_spiral.resolved_geometry.solids.push(guard);
        assert_eq!(tower_index, 0);
        assert!(has(&blocked_spiral, "blocked_spiral_arrival"));
    }

    #[test]
    fn timber_frame_audit_rejects_program_joint_opening_jetty_and_roof_drift() {
        let fixture =
            |archetype| crate::generate(&crate::BuildingProgram::fixture(archetype, 47)).unwrap();
        let has = |plan: &crate::BuildingPlan, code: &str| {
            audit_plan(plan).iter().any(|issue| issue.code == code)
        };

        let mut relabelled = fixture(crate::BuildingArchetype::HallHouse);
        relabelled.timber_frame.as_mut().unwrap().program =
            crate::TimberFrameProgramKind::JettiedMerchantHouse;
        assert!(has(&relabelled, "invalid_timber_program"));

        let mut dangling_endpoint = fixture(crate::BuildingArchetype::TownHouse);
        dangling_endpoint.timber_frame.as_mut().unwrap().members[0]
            .start
            .x += 0.22;
        assert!(has(&dangling_endpoint, "invalid_timber_member_joint"));

        let mut missing_member_solid = fixture(crate::BuildingArchetype::TownHouse);
        let solid = missing_member_solid.timber_frame.as_ref().unwrap().members[0].solid;
        missing_member_solid
            .resolved_geometry
            .solids
            .retain(|candidate| candidate.id != solid);
        assert!(has(&missing_member_solid, "invalid_timber_member_joint"));

        let mut orphan_joint = fixture(crate::BuildingArchetype::TownHouse);
        orphan_joint.timber_frame.as_mut().unwrap().joints[0]
            .member_ids
            .clear();
        assert!(has(&orphan_joint, "orphan_timber_joint"));

        let mut unframed_opening = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let frame = unframed_opening.timber_frame.as_mut().unwrap();
        let bay = frame
            .bays
            .iter_mut()
            .find(|bay| bay.opening.is_some())
            .unwrap();
        bay.member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| member.role != crate::TimberMemberRole::IntermediatePost)
        });
        assert!(has(&unframed_opening, "invalid_timber_opening_bay"));

        let mut brace_through_window = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let (brace_solid, opening_void) = {
            let frame = brace_through_window.timber_frame.as_ref().unwrap();
            let bay = frame.bays.iter().find(|bay| bay.opening.is_some()).unwrap();
            let member = bay
                .member_ids
                .iter()
                .filter_map(|id| frame.members.iter().find(|member| member.id == *id))
                .find(|member| {
                    matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    )
                })
                .unwrap();
            let opening = brace_through_window
                .opening_assemblies
                .iter()
                .find(|opening| Some(opening.id) == bay.opening)
                .unwrap();
            (member.solid, opening.void_id)
        };
        let void = brace_through_window
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == opening_void)
            .unwrap()
            .bounds;
        let blocker = brace_through_window
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == brace_solid)
            .unwrap();
        blocker.centre = (void.min + void.max) * 0.5;
        blocker.size = Vec3::splat(0.30);
        assert!(has(&brace_through_window, "invalid_timber_opening_bay"));

        let mut one_post_row = fixture(crate::BuildingArchetype::HallHouse);
        let frame = one_post_row.timber_frame.as_mut().unwrap();
        let row_index = frame
            .internal_lines
            .iter()
            .position(|line| {
                line.storeys
                    .iter()
                    .flat_map(|storey| &storey.member_ids)
                    .any(|id| {
                        frame.members.iter().any(|member| {
                            member.id == *id && member.role == crate::TimberMemberRole::Purlin
                        })
                    })
            })
            .unwrap();
        frame.internal_lines.remove(row_index);
        assert!(has(&one_post_row, "invalid_timber_program"));

        let mut unsupported_jetty = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let jetty = unsupported_jetty
            .timber_frame
            .as_mut()
            .unwrap()
            .facades
            .iter_mut()
            .flat_map(|facade| &mut facade.lines)
            .flat_map(|line| &mut line.storeys)
            .find_map(|storey| storey.jetty.as_mut())
            .unwrap();
        jetty.knaggen.clear();
        assert!(has(&unsupported_jetty, "unsupported_timber_jetty"));

        let mut floor_outside_support = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let floor_id = floor_outside_support
            .timber_frame
            .as_ref()
            .unwrap()
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .flat_map(|line| &line.storeys)
            .find_map(|storey| storey.jetty.as_ref())
            .unwrap()
            .floor_solid;
        floor_outside_support
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_id)
            .unwrap()
            .size
            .z += 0.45;
        assert!(has(&floor_outside_support, "unsupported_timber_jetty"));

        let mut no_roof_seat = fixture(crate::BuildingArchetype::FachwerkCottage);
        no_roof_seat
            .timber_frame
            .as_mut()
            .unwrap()
            .roof_bearing_interfaces
            .clear();
        assert!(has(&no_roof_seat, "severed_timber_roof_bearing"));

        let mut missing_trimmer = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        missing_trimmer
            .timber_frame
            .as_mut()
            .unwrap()
            .dormer_trimmer_members
            .pop();
        assert!(has(&missing_trimmer, "severed_timber_roof_bearing"));

        let mut protruding_half_hip_truss = fixture(crate::BuildingArchetype::HallHouse);
        protruding_half_hip_truss
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| {
                member.phase == crate::TimberFramePhase::RoofConstruction
                    && member.role == crate::TimberMemberRole::GablePost
            })
            .unwrap()
            .end
            .y += 2.0;
        assert!(has(
            &protruding_half_hip_truss,
            "timber_intrudes_through_roof"
        ));

        let mut coplanar_window = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        let opening_sill = coplanar_window
            .opening_assemblies
            .iter()
            .find(|opening| {
                coplanar_window
                    .wall_assemblies
                    .iter()
                    .find(|wall| wall.id == opening.host_wall)
                    .is_some_and(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
            })
            .unwrap()
            .sill_solid
            .unwrap();
        coplanar_window
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == opening_sill)
            .unwrap()
            .size
            .z += 0.03;
        assert!(has(&coplanar_window, "coplanar_timber_opening_face"));

        let mut free_dormer_posts = fixture(crate::BuildingArchetype::RenaissanceTownHall);
        let dormer_gable_post = {
            let frame = free_dormer_posts.timber_frame.as_ref().unwrap();
            frame
                .bays
                .iter()
                .find(|bay| {
                    bay.wall.is_some_and(|wall_id| {
                        free_dormer_posts
                            .wall_assemblies
                            .iter()
                            .find(|wall| wall.id == wall_id)
                            .is_some_and(|wall| {
                                matches!(wall.source, crate::WallSourceId::RoofChildFront { .. })
                            })
                    })
                })
                .and_then(|bay| {
                    bay.member_ids.iter().find(|id| {
                        frame.members.iter().any(|member| {
                            member.id == **id && member.role == crate::TimberMemberRole::GablePost
                        })
                    })
                })
                .copied()
                .unwrap()
        };
        free_dormer_posts
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| member.id == dormer_gable_post)
            .unwrap()
            .role = crate::TimberMemberRole::PrimaryPost;
        assert!(has(&free_dormer_posts, "invalid_timber_dormer_front"));

        let mut legacy_overlay = fixture(crate::BuildingArchetype::TownHouse);
        let mut legacy = legacy_overlay.resolved_geometry.solids[0].clone();
        legacy.id = crate::ResolvedItemId(u64::MAX - 8);
        legacy.role = SolidRole::FrameMember;
        legacy_overlay.resolved_geometry.solids.push(legacy);
        assert!(has(&legacy_overlay, "legacy_timber_frame_overlay"));

        let mut missing_floor_frame = fixture(crate::BuildingArchetype::TownHouse);
        missing_floor_frame.timber_frame.as_mut().unwrap().floors[1]
            .joist_members
            .clear();
        assert!(has(&missing_floor_frame, "unsupported_timber_floor_route"));

        let mut severed_stair_route = fixture(crate::BuildingArchetype::TownHouse);
        severed_stair_route.timber_frame.as_mut().unwrap().floors[1].stair_connection = None;
        assert!(has(&severed_stair_route, "unsupported_timber_floor_route"));

        let mut illegal_joint = fixture(crate::BuildingArchetype::TownHouse);
        illegal_joint.timber_frame.as_mut().unwrap().joints[4].kind =
            crate::TimberJointKind::Bridle;
        assert!(has(&illegal_joint, "invalid_timber_joint_vocabulary"));

        let mut false_nearby = fixture(crate::BuildingArchetype::TownHouse);
        let owner = false_nearby.timber_frame.as_ref().unwrap().members[0].owner;
        let frame_nodes = false_nearby
            .resolved_geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == owner)
            .map(|node| (node.id, node.position))
            .collect::<Vec<_>>();
        let (child, child_position) = frame_nodes[0];
        let parent = frame_nodes
            .iter()
            .max_by(|left, right| {
                left.1
                    .distance(child_position)
                    .total_cmp(&right.1.distance(child_position))
            })
            .unwrap()
            .0;
        false_nearby
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == child)
            .unwrap()
            .supported_by
            .push(parent);
        assert!(has(&false_nearby, "false_timber_support_edge"));

        let mut cyclic_support = fixture(crate::BuildingArchetype::TownHouse);
        let timber_owner = cyclic_support.timber_frame.as_ref().unwrap().members[0].owner;
        let node_id = cyclic_support
            .resolved_geometry
            .structural_nodes
            .iter()
            .find(|node| node.owner == timber_owner && !node.grounded)
            .unwrap()
            .id;
        cyclic_support
            .resolved_geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == node_id)
            .unwrap()
            .supported_by
            .push(node_id);
        assert!(has(&cyclic_support, "unsupported_resolved_structure"));

        let mut shifted_roof_seat = fixture(crate::BuildingArchetype::FachwerkCottage);
        let interface_id = shifted_roof_seat
            .timber_frame
            .as_ref()
            .unwrap()
            .roof_bearing_interfaces[0];
        let interface = shifted_roof_seat
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
            .unwrap();
        interface.bounds.min.x += 1.0;
        interface.bounds.max.x += 1.0;
        assert!(has(&shifted_roof_seat, "severed_timber_roof_bearing"));

        let mut missing_infill_partition = fixture(crate::BuildingArchetype::TownHouse);
        missing_infill_partition
            .timber_frame
            .as_mut()
            .unwrap()
            .bays
            .iter_mut()
            .find(|bay| bay.opening.is_some())
            .unwrap()
            .infill_solids
            .pop();
        assert!(has(&missing_infill_partition, "invalid_timber_opening_bay"));

        let panel_id = |plan: &crate::BuildingPlan| {
            plan.timber_frame
                .as_ref()
                .unwrap()
                .bays
                .iter()
                .find(|bay| bay.opening.is_some())
                .and_then(|bay| bay.infill_solids.first().copied())
                .unwrap()
        };
        let refresh_panel_bounds = |solid: &mut ResolvedSolid| {
            let crate::ResolvedSolidShape::TimberPanelPrism {
                vertices,
                outward,
                depth_metres,
            } = solid.shape
            else {
                return;
            };
            let offset = Vec3::new(outward.x, 0.0, outward.y) * depth_metres * 0.5;
            let min = vertices
                .iter()
                .flat_map(|vertex| [*vertex - offset, *vertex + offset])
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = vertices
                .iter()
                .flat_map(|vertex| [*vertex - offset, *vertex + offset])
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            solid.centre = (min + max) * 0.5;
            solid.size = max - min;
        };

        let mut backing_sheet = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&backing_sheet);
        backing_sheet
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .shape = crate::ResolvedSolidShape::Cuboid;
        assert!(has(&backing_sheet, "invalid_timber_opening_bay"));

        let mut shifted_panel_gap = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&shifted_panel_gap);
        let panel = shifted_panel_gap
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap();
        if let crate::ResolvedSolidShape::TimberPanelPrism { vertices, .. } = &mut panel.shape {
            vertices[0].x += 0.08;
        }
        refresh_panel_bounds(panel);
        assert!(has(&shifted_panel_gap, "invalid_timber_opening_bay"));

        let mut panel_crosses_frame = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&panel_crosses_frame);
        let wall_id = panel_crosses_frame
            .timber_frame
            .as_ref()
            .unwrap()
            .bays
            .iter()
            .find(|bay| bay.infill_solids.contains(&id))
            .unwrap()
            .wall
            .unwrap();
        let wall = panel_crosses_frame
            .wall_assemblies
            .iter()
            .find(|wall| wall.id == wall_id)
            .unwrap()
            .clone();
        let panel = panel_crosses_frame
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap();
        if let crate::ResolvedSolidShape::TimberPanelPrism { vertices, .. } = &mut panel.shape {
            let plane = wall.frame.origin;
            *vertices = [
                Vec3::new(
                    (plane - wall.frame.tangent * wall.length_metres * 0.45).x,
                    wall.base_elevation_metres + 0.15,
                    (plane - wall.frame.tangent * wall.length_metres * 0.45).y,
                ),
                Vec3::new(
                    (plane + wall.frame.tangent * wall.length_metres * 0.45).x,
                    wall.base_elevation_metres + 0.15,
                    (plane + wall.frame.tangent * wall.length_metres * 0.45).y,
                ),
                Vec3::new(
                    plane.x,
                    wall.base_elevation_metres + wall.height_metres * 0.85,
                    plane.y,
                ),
            ];
        }
        refresh_panel_bounds(panel);
        assert!(has(&panel_crosses_frame, "invalid_timber_opening_bay"));

        let mut overlapping_panel = fixture(crate::BuildingArchetype::TownHouse);
        let id = panel_id(&overlapping_panel);
        let mut duplicate = overlapping_panel
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == id)
            .unwrap()
            .clone();
        duplicate.id = crate::ResolvedItemId(u64::MAX - 17);
        let wall_id = overlapping_panel
            .timber_frame
            .as_ref()
            .unwrap()
            .bays
            .iter()
            .find(|bay| bay.infill_solids.contains(&id))
            .unwrap()
            .wall
            .unwrap();
        overlapping_panel
            .wall_assemblies
            .iter_mut()
            .find(|wall| wall.id == wall_id)
            .unwrap()
            .host_solids
            .push(duplicate.id);
        for bay in overlapping_panel
            .timber_frame
            .as_mut()
            .unwrap()
            .bays
            .iter_mut()
            .filter(|bay| bay.wall == Some(wall_id))
        {
            bay.infill_solids.push(duplicate.id);
        }
        overlapping_panel.resolved_geometry.solids.push(duplicate);
        assert!(has(&overlapping_panel, "invalid_timber_opening_bay"));

        let mut untriangulated = fixture(crate::BuildingArchetype::TownHouse);
        let frame = untriangulated.timber_frame.as_mut().unwrap();
        let storey = &mut frame.facades[0].lines[0].storeys[0];
        storey.member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| {
                    !matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    )
                })
        });
        assert!(has(&untriangulated, "unbraced_timber_storey"));

        let mut missing_middle_racking = fixture(crate::BuildingArchetype::TownHouse);
        let frame = missing_middle_racking.timber_frame.as_mut().unwrap();
        let line = &mut frame.facades[0].lines[0];
        let storey = &mut line.storeys[0];
        storey.member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| {
                    let midpoint = (member.start + member.end) * 0.5;
                    let along = (Vec2::new(midpoint.x, midpoint.z) - line.origin).dot(line.tangent)
                        / line.length_metres
                        + 0.5;
                    !matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    ) || !(0.30..=0.70).contains(&along)
                })
        });
        assert!(has(&missing_middle_racking, "unbraced_timber_storey"));

        let mut broken_transverse = fixture(crate::BuildingArchetype::HallHouse);
        let frame = broken_transverse.timber_frame.as_mut().unwrap();
        let line = frame
            .internal_lines
            .iter_mut()
            .find(|line| {
                line.storeys
                    .iter()
                    .flat_map(|storey| &storey.member_ids)
                    .any(|id| {
                        frame.members.iter().any(|member| {
                            member.id == *id
                                && member.role == crate::TimberMemberRole::TransverseTie
                        })
                    })
            })
            .unwrap();
        line.storeys[0].member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_none_or(|member| member.role != crate::TimberMemberRole::StoreyBrace)
        });
        assert!(has(&broken_transverse, "unbraced_timber_storey"));

        let mut swapped_joint = fixture(crate::BuildingArchetype::TownHouse);
        let joint = swapped_joint
            .timber_frame
            .as_mut()
            .unwrap()
            .joints
            .iter_mut()
            .find(|joint| joint.kind == crate::TimberJointKind::Lap)
            .unwrap();
        joint.kind = crate::TimberJointKind::MortiseTenon;
        assert!(has(&swapped_joint, "invalid_timber_joint_contact"));

        let mut wrong_world_axis = fixture(crate::BuildingArchetype::TownHouse);
        let joint = wrong_world_axis
            .timber_frame
            .as_mut()
            .unwrap()
            .joints
            .iter_mut()
            .find(|joint| joint.load_direction.dot(Vec3::Z).abs() < 0.8)
            .unwrap();
        joint.load_direction = Vec3::Z;
        assert!(has(&wrong_world_axis, "invalid_timber_joint_contact"));

        let mut broken_action_reaction = fixture(crate::BuildingArchetype::TownHouse);
        let participant = broken_action_reaction
            .timber_frame
            .as_mut()
            .unwrap()
            .joints
            .iter_mut()
            .find_map(|joint| joint.participants.first_mut())
            .unwrap();
        participant.reaction_direction = participant.axis_from_joint;
        assert!(has(&broken_action_reaction, "invalid_timber_joint_contact"));

        let cardinal_frame = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let frame = cardinal_frame.timber_frame.as_ref().unwrap();
        let cardinal_reactions = frame
            .joints
            .iter()
            .filter(|joint| joint.kind == crate::TimberJointKind::JettyBearing)
            .map(|joint| {
                let horizontal =
                    Vec2::new(joint.load_direction.x, joint.load_direction.z).normalize_or_zero();
                (horizontal.x.round() as i8, horizontal.y.round() as i8)
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            cardinal_reactions,
            [(-1, 0), (1, 0), (0, -1), (0, 1)].into_iter().collect(),
            "facade-local joint reactions must rotate and mirror with all four facades"
        );

        let mut wrong_material = fixture(crate::BuildingArchetype::TownHouse);
        wrong_material.timber_frame.as_mut().unwrap().members[0].material =
            crate::StructuralTimberMaterial::Oak;
        assert!(has(&wrong_material, "invalid_timber_joint_contact"));

        let mut narrow_route = fixture(crate::BuildingArchetype::TownHouse);
        narrow_route
            .timber_frame
            .as_mut()
            .unwrap()
            .circulation
            .edges[0]
            .clear_width_metres = 0.70;
        assert!(has(&narrow_route, "invalid_timber_circulation"));

        let mut broken_route = fixture(crate::BuildingArchetype::TownHouse);
        broken_route
            .timber_frame
            .as_mut()
            .unwrap()
            .circulation
            .edges
            .pop();
        assert!(has(&broken_route, "invalid_timber_circulation"));

        let mut missing_cut = fixture(crate::BuildingArchetype::TownHouse);
        missing_cut
            .timber_frame
            .as_mut()
            .unwrap()
            .circulation
            .floor_cut_voids
            .clear();
        assert!(has(&missing_cut, "invalid_timber_circulation"));

        let mut shifted_entry = fixture(crate::BuildingArchetype::TownHouse);
        let opening_id = shifted_entry
            .timber_frame
            .as_ref()
            .unwrap()
            .circulation
            .entry_opening
            .unwrap();
        let void_id = shifted_entry
            .opening_assemblies
            .iter()
            .find(|opening| opening.id == opening_id)
            .unwrap()
            .void_id;
        let void = shifted_entry
            .resolved_geometry
            .voids
            .iter_mut()
            .find(|void| void.id == void_id)
            .unwrap();
        void.bounds.min.x += 0.8;
        void.bounds.max.x += 0.8;
        assert!(has(&shifted_entry, "invalid_timber_circulation"));

        let mut nested_posts = fixture(crate::BuildingArchetype::FachwerkMerchantHouse);
        let post_ids = nested_posts
            .timber_frame
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|member| {
                matches!(
                    member.role,
                    crate::TimberMemberRole::PrimaryPost
                        | crate::TimberMemberRole::CornerPost
                        | crate::TimberMemberRole::IntermediatePost
                )
            })
            .take(2)
            .map(|member| (member.id, member.solid, member.start, member.end))
            .collect::<Vec<_>>();
        let (target_start, target_end) = (post_ids[0].2, post_ids[0].3);
        let member = nested_posts
            .timber_frame
            .as_mut()
            .unwrap()
            .members
            .iter_mut()
            .find(|member| member.id == post_ids[1].0)
            .unwrap();
        member.start.x = target_start.x;
        member.start.z = target_start.z;
        member.end.x = target_end.x;
        member.end.z = target_end.z;
        let target_centre = nested_posts
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.id == post_ids[0].1)
            .unwrap()
            .centre;
        let solid = nested_posts
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == post_ids[1].1)
            .unwrap();
        solid.centre.x = target_centre.x;
        solid.centre.z = target_centre.z;
        assert!(has(&nested_posts, "overlapping_timber_members"));

        let mut severed_floor_chain = fixture(crate::BuildingArchetype::TownHouse);
        let interface_id =
            severed_floor_chain.timber_frame.as_ref().unwrap().floors[1].joist_girder_interfaces[0];
        let interface = severed_floor_chain
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
            .unwrap();
        interface.bounds.min.x += 0.8;
        interface.bounds.max.x += 0.8;
        assert!(has(&severed_floor_chain, "unsupported_timber_floor_route"));

        let mut buried_frame = fixture(crate::BuildingArchetype::TownHouse);
        let panel_id = buried_frame
            .timber_frame
            .as_ref()
            .unwrap()
            .bays
            .iter()
            .find(|bay| bay.opening.is_some())
            .unwrap()
            .infill_solids[0];
        let panel = buried_frame
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == panel_id)
            .unwrap();
        panel.size.z += 0.30;
        assert!(has(&buried_frame, "invalid_timber_opening_bay"));

        let mut shifted_joint_contact = fixture(crate::BuildingArchetype::TownHouse);
        let contact_id = shifted_joint_contact
            .timber_frame
            .as_ref()
            .unwrap()
            .joints
            .iter()
            .find(|joint| joint.member_ids.len() >= 2)
            .unwrap()
            .contact_interfaces[0];
        let contact = shifted_joint_contact
            .resolved_geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == contact_id)
            .unwrap();
        contact.bounds.min.z += 0.5;
        contact.bounds.max.z += 0.5;
        assert!(has(&shifted_joint_contact, "invalid_timber_joint_contact"));

        let mut blocked_route = fixture(crate::BuildingArchetype::TownHouse);
        let frame = blocked_route.timber_frame.as_ref().unwrap();
        let edge = frame
            .circulation
            .edges
            .iter()
            .find(|edge| {
                frame
                    .circulation
                    .nodes
                    .iter()
                    .find(|node| node.surface == edge.from)
                    .is_some_and(|node| node.kind == crate::TimberRouteNodeKind::Landing)
            })
            .unwrap();
        let from = frame
            .circulation
            .nodes
            .iter()
            .find(|node| node.surface == edge.from)
            .unwrap()
            .position;
        let to = frame
            .circulation
            .nodes
            .iter()
            .find(|node| node.surface == edge.to)
            .unwrap()
            .position;
        let mut blocker = blocked_route
            .resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.role == SolidRole::WallHost)
            .unwrap()
            .clone();
        blocker.id = crate::ResolvedItemId(u64::MAX - 31);
        blocker.centre = (from + to) * 0.5 + Vec3::Y;
        blocker.size = Vec3::new(0.22, 2.0, 1.2);
        blocked_route.resolved_geometry.solids.push(blocker);
        assert!(has(&blocked_route, "invalid_timber_circulation"));
    }
}
