fn audit_church_assembly(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    let Some(church) = &plan.church else {
        if plan.archetype == BuildingArchetype::Cathedral {
            issues.push(issue(
                "missing_church_program",
                "cathedral has no authoritative church assembly".to_owned(),
            ));
        }
        return;
    };
    let program = church.program;
    if plan.archetype != BuildingArchetype::Cathedral
        || program.liturgical_east != Direction::East
        || program.nave_bays != 4
        || program.transept_bays != 1
        || program.choir_bays != 2
        || program.apse_sides != 5
        || program.aisles != 3
    {
        issues.push(issue(
            "invalid_church_program",
            "church is not the frozen east-oriented 4-bay cruciform basilica type".to_owned(),
        ));
    }
    if plan
        .storeys
        .iter()
        .any(|storey| !storey.walls.is_empty() || !storey.openings.is_empty())
    {
        issues.push(issue(
            "legacy_church_authority",
            "church still contains generic cell walls or overlay openings".to_owned(),
        ));
    }
    let strictly_increasing =
        |values: &[f32]| values.windows(2).all(|pair| pair[1] > pair[0] + 0.10);
    if church.nave_axes_metres.len() != usize::from(program.nave_bays)
        || church.choir_axes_metres.len() != usize::from(program.choir_bays)
        || !strictly_increasing(&church.nave_axes_metres)
        || !strictly_increasing(&church.choir_axes_metres)
        || church
            .nave_axes_metres
            .last()
            .is_none_or(|axis| *axis >= church.crossing_axis_metres)
        || church
            .choir_axes_metres
            .first()
            .is_none_or(|axis| *axis <= church.crossing_axis_metres)
    {
        issues.push(issue(
            "invalid_church_bay_axes",
            "nave/crossing/choir axes are missing, unordered, or blocked".to_owned(),
        ));
    }
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<BTreeMap<_, _>>();
    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let openings = plan
        .opening_assemblies
        .iter()
        .map(|opening| (opening.id, opening))
        .collect::<BTreeMap<_, _>>();
    let walls = plan
        .wall_assemblies
        .iter()
        .map(|wall| (wall.id, wall))
        .collect::<BTreeMap<_, _>>();
    let surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<BTreeMap<_, _>>();
    let voids = plan
        .resolved_geometry
        .voids
        .iter()
        .map(|void| (void.id, void))
        .collect::<BTreeMap<_, _>>();
    let interfaces = plan
        .resolved_geometry
        .support_interfaces
        .iter()
        .map(|interface| (interface.id, interface))
        .collect::<BTreeMap<_, _>>();
    let support_solid = |node_id: StructuralNodeId| {
        plan.resolved_geometry
            .solids
            .iter()
            .find(|solid| solid.supported_by.contains(&node_id))
    };
    let true_arch = |solid: &crate::ResolvedSolid| {
        matches!(
            solid.shape,
            crate::ResolvedSolidShape::SegmentalArchRing {
                clear_span_metres,
                rise_metres,
                ..
            } if clear_span_metres >= 0.90 && rise_metres >= 0.35
        ) || matches!(
            solid.shape,
            crate::ResolvedSolidShape::PointedArchRing {
                clear_span_metres,
                spring_height_metres,
                apex_height_metres,
                ..
            } if clear_span_metres >= 0.90 && apex_height_metres - spring_height_metres >= 0.35
        )
    };
    if church.bay_assemblies.len() != usize::from(program.nave_bays)
        || church.bay_assemblies.iter().any(|bay| {
            bay.pier_nodes
                .iter()
                .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
                || bay.pier_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::ChurchPier)
                })
                || bay.arcade_solids.iter().any(|id| {
                    solids.get(id).is_none_or(|arcade| {
                        arcade.role != SolidRole::ChurchArcade
                            || !true_arch(arcade)
                            || arcade.supported_by.len() != 1
                            || nodes.get(&arcade.supported_by[0]).is_none_or(|spring| {
                                spring.grounded
                                    || spring.supported_by.len() != 2
                                    || !spring.supported_by.iter().all(|bearing| {
                                        nodes.get(bearing).is_some_and(|node| node.grounded)
                                    })
                            })
                    })
                })
                || bay
                    .arcade_bearing_nodes
                    .iter()
                    .any(|pair| pair[0] == pair[1])
                || bay
                    .arcade_bearing_interfaces
                    .iter()
                    .enumerate()
                    .any(|(side, pair)| {
                        pair.iter().enumerate().any(|(end, id)| {
                            let Some(interface) = interfaces.get(id) else {
                                return true;
                            };
                            let Some(arcade) = solids.get(&bay.arcade_solids[side]) else {
                                return true;
                            };
                            let bearing_node = bay.arcade_bearing_nodes[side][end];
                            let Some(pier) = support_solid(bearing_node) else {
                                return true;
                            };
                            interface.node != arcade.supported_by[0]
                                || !bounds_overlap_3d(
                                    (interface.bounds.min, interface.bounds.max),
                                    resolved_solid_bounds(arcade),
                                    0.02,
                                )
                                || !bounds_overlap_3d(
                                    (interface.bounds.min, interface.bounds.max),
                                    resolved_solid_bounds(pier),
                                    0.02,
                                )
                        })
                    })
                || bay
                    .buttress_nodes
                    .iter()
                    .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
                || bay.buttress_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::WallButtress)
                })
                || bay.vault_solids.is_empty()
                || bay.vault_solids.iter().any(|id| {
                    solids.get(id).is_none_or(|solid| {
                        solid.role != SolidRole::ChurchVaultShell
                            || solid.supported_by.len() != 1
                            || !bay.vault_spring_nodes.contains(&solid.supported_by[0])
                    })
                })
                || bay.vault_thrust_solids.len() != 4
                || bay.vault_thrust_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::ChurchVaultThrust)
                })
                || bay.vault_load_surfaces.len() != 2
                || bay.vault_load_surfaces.iter().any(|id| {
                    surfaces
                        .get(id)
                        .is_none_or(|surface| surface.role != SurfaceRole::ChurchVaultLoad)
                })
                || bay.vault_spring_nodes.len() != 2
                || bay.vault_spring_nodes.iter().any(|id| {
                    nodes.get(id).is_none_or(|spring| {
                        spring.grounded
                            || spring.supported_by.len() != 4
                            || spring
                                .supported_by
                                .iter()
                                .filter(|support| {
                                    nodes.get(support).is_some_and(|node| {
                                        node.kind == crate::StructuralNodeKind::ChurchPier
                                    })
                                })
                                .count()
                                != 2
                            || spring
                                .supported_by
                                .iter()
                                .filter(|support| {
                                    nodes.get(support).is_some_and(|node| {
                                        node.kind == crate::StructuralNodeKind::ChurchButtress
                                    })
                                })
                                .count()
                                != 2
                            || !spring
                                .supported_by
                                .iter()
                                .all(|support| nodes.get(support).is_some_and(|node| node.grounded))
                    })
                })
                || bay.vault_bearing_interfaces.len() != 8
                || bay.vault_bearing_interfaces.iter().any(|id| {
                    interfaces.get(id).is_none_or(|interface| {
                        !bay.vault_spring_nodes.contains(&interface.node)
                            || !bay.vault_thrust_solids.iter().any(|solid_id| {
                                solids.get(solid_id).is_some_and(|solid| {
                                    bounds_overlap_3d(
                                        (interface.bounds.min, interface.bounds.max),
                                        resolved_solid_bounds(solid),
                                        0.03,
                                    )
                                })
                            })
                    })
                })
        })
    {
        issues.push(issue(
            "invalid_church_bay_structure",
            "a nave bay lacks paired piers, arcades, buttresses, or vault load structure"
                .to_owned(),
        ));
    }
    let church_windows = plan
        .opening_assemblies
        .iter()
        .filter(|opening| {
            opening.use_kind == crate::OpeningUse::Window
                && matches!(
                    opening.host_source,
                    crate::WallSourceId::ChurchExterior { .. }
                        | crate::WallSourceId::ChurchArcade { .. }
                        | crate::WallSourceId::ChurchApse { .. }
                )
        })
        .collect::<Vec<_>>();
    let expected_windows = usize::from(program.nave_bays) * 4
        + usize::from(program.choir_bays) * 2
        + 2
        + usize::from(program.apse_sides.saturating_sub(1));
    let clerestory_is_bay_bound = church.bay_assemblies.iter().all(|bay| {
        [Direction::South, Direction::North]
            .into_iter()
            .zip(bay.clerestory_openings)
            .all(|(side, id)| {
                openings.get(&id).is_some_and(|opening| {
                    opening.host_source
                        == crate::WallSourceId::ChurchArcade {
                            side,
                            bay: bay.axis_index,
                        }
                        && opening.closure.layers == [crate::ClosureKind::LeadedGlazing]
                        && matches!(
                            opening.profile,
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                apex_height_metres,
                                ..
                            } if width_metres >= 1.20 && apex_height_metres >= 2.20
                        )
                })
            })
    });
    let transept_has_principal_lights =
        [Direction::South, Direction::North]
            .into_iter()
            .all(|side| {
                church_windows.iter().any(|opening| {
                    opening.host_source
                        == crate::WallSourceId::ChurchExterior {
                            range: crate::ChurchRange::Transept,
                            side,
                            bay: 0,
                        }
                        && matches!(
                            opening.profile,
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                apex_height_metres,
                                ..
                            } if width_metres >= 2.20 && apex_height_metres >= 7.40
                        )
                })
            });
    if church_windows.len() != expected_windows
        || !clerestory_is_bay_bound
        || !transept_has_principal_lights
        || church_windows.iter().any(|opening| {
            opening.tracery_node.is_none()
                || opening.closure_solids.len() != 2
                || opening.closure.layers != [crate::ClosureKind::LeadedGlazing]
        })
    {
        issues.push(issue(
            "invalid_church_window_hierarchy",
            "church lights are not pointed, bay-aligned, stone-divided, and hierarchically scaled"
                .to_owned(),
        ));
    }
    let crossing_arches_valid =
        church
            .crossing
            .arch_solids
            .iter()
            .enumerate()
            .all(|(arch_index, id)| {
                let Some(arch) = solids.get(id) else {
                    return false;
                };
                if arch.role != SolidRole::ChurchCrossingArch
                    || !true_arch(arch)
                    || arch.supported_by.len() != 1
                {
                    return false;
                }
                let Some(spring) = nodes.get(&arch.supported_by[0]) else {
                    return false;
                };
                let bearings = church.crossing.arch_bearing_nodes[arch_index];
                spring.supported_by.len() == 2
                    && bearings[0] != bearings[1]
                    && spring
                        .supported_by
                        .iter()
                        .all(|node| bearings.contains(node))
                    && church.crossing.arch_bearing_interfaces[arch_index]
                        .iter()
                        .enumerate()
                        .all(|(end, id)| {
                            interfaces.get(id).is_some_and(|interface| {
                                interface.node == spring.id
                                    && bounds_overlap_3d(
                                        (interface.bounds.min, interface.bounds.max),
                                        resolved_solid_bounds(arch),
                                        0.02,
                                    )
                                    && support_solid(bearings[end]).is_some_and(|bearing| {
                                        bounds_overlap_3d(
                                            (interface.bounds.min, interface.bounds.max),
                                            resolved_solid_bounds(bearing),
                                            0.02,
                                        )
                                    })
                            })
                        })
            });
    let crossing_load_valid = church
        .crossing
        .buttress_nodes
        .iter()
        .all(|id| nodes.get(id).is_some_and(|node| node.grounded))
        && church.crossing.buttress_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::WallButtress)
        })
        && church.crossing.vault_thrust_solids.len() == 4
        && church.crossing.vault_thrust_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::ChurchVaultThrust)
        })
        && church.crossing.vault_load_surfaces.len() == 1
        && church.crossing.vault_load_surfaces.iter().all(|id| {
            surfaces
                .get(id)
                .is_some_and(|surface| surface.role == SurfaceRole::ChurchVaultLoad)
        })
        && church.crossing.vault_spring_nodes.len() == 1
        && church.crossing.vault_spring_nodes.iter().all(|id| {
            nodes.get(id).is_some_and(|spring| {
                !spring.grounded
                    && spring.supported_by.len() == 8
                    && spring
                        .supported_by
                        .iter()
                        .all(|support| nodes.get(support).is_some_and(|node| node.grounded))
            })
        })
        && church.crossing.vault_bearing_interfaces.len() == 8
        && church.crossing.vault_bearing_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                church.crossing.vault_spring_nodes.contains(&interface.node)
                    && church.crossing.vault_thrust_solids.iter().any(|solid_id| {
                        solids.get(solid_id).is_some_and(|solid| {
                            bounds_overlap_3d(
                                (interface.bounds.min, interface.bounds.max),
                                resolved_solid_bounds(solid),
                                0.03,
                            )
                        })
                    })
            })
        });
    if church
        .crossing
        .pier_nodes
        .iter()
        .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
        || !crossing_arches_valid
        || church.crossing.vault_solids.is_empty()
        || church.crossing.vault_solids.iter().any(|id| {
            solids.get(id).is_none_or(|vault| {
                vault.role != SolidRole::ChurchVaultShell
                    || vault.supported_by.len() != 1
                    || !church
                        .crossing
                        .vault_spring_nodes
                        .contains(&vault.supported_by[0])
            })
        })
        || !crossing_load_valid
    {
        issues.push(issue(
            "invalid_church_crossing",
            "crossing lacks four grounded piers, four arches, or a closed vault".to_owned(),
        ));
    }
    let choir_arches_valid = church.choir.arch_solids.len() == usize::from(program.choir_bays) * 2
        && church.choir.arch_bearing_nodes.len() == church.choir.arch_solids.len()
        && church.choir.arch_bearing_interfaces.len() == church.choir.arch_solids.len()
        && church
            .choir
            .arch_solids
            .iter()
            .enumerate()
            .all(|(index, id)| {
                let Some(arch) = solids.get(id) else {
                    return false;
                };
                let Some(spring) = arch.supported_by.first().and_then(|id| nodes.get(id)) else {
                    return false;
                };
                let bearings = church.choir.arch_bearing_nodes[index];
                arch.role == SolidRole::ChurchArcade
                    && true_arch(arch)
                    && arch.supported_by.len() == 1
                    && spring.supported_by.len() == 2
                    && spring.supported_by.iter().all(|id| bearings.contains(id))
                    && church.choir.arch_bearing_interfaces[index]
                        .iter()
                        .enumerate()
                        .all(|(end, id)| {
                            interfaces.get(id).is_some_and(|interface| {
                                interface.node == spring.id
                                    && bounds_overlap_3d(
                                        (interface.bounds.min, interface.bounds.max),
                                        resolved_solid_bounds(arch),
                                        0.02,
                                    )
                                    && support_solid(bearings[end]).is_some_and(|bearing| {
                                        bounds_overlap_3d(
                                            (interface.bounds.min, interface.bounds.max),
                                            resolved_solid_bounds(bearing),
                                            0.02,
                                        )
                                    })
                            })
                        })
            });
    let choir_load_valid = church.choir.vault_thrust_solids.len()
        == usize::from(program.choir_bays) * 4
        && church.choir.vault_load_surfaces.len() == usize::from(program.choir_bays)
        && church.choir.vault_spring_nodes.len() == usize::from(program.choir_bays) * 2
        && church.choir.vault_bearing_interfaces.len() == usize::from(program.choir_bays) * 8
        && church.choir.vault_thrust_solids.iter().all(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::ChurchVaultThrust)
        })
        && church.choir.vault_load_surfaces.iter().all(|id| {
            surfaces
                .get(id)
                .is_some_and(|surface| surface.role == SurfaceRole::ChurchVaultLoad)
        })
        && church.choir.vault_spring_nodes.iter().all(|id| {
            nodes.get(id).is_some_and(|spring| {
                !spring.grounded
                    && spring.supported_by.len() == 4
                    && spring
                        .supported_by
                        .iter()
                        .filter(|id| {
                            nodes.get(id).is_some_and(|node| {
                                node.kind == crate::StructuralNodeKind::ChurchPier
                                    || node.kind == crate::StructuralNodeKind::ChurchCrossingPier
                            })
                        })
                        .count()
                        == 2
                    && spring
                        .supported_by
                        .iter()
                        .filter(|id| {
                            nodes.get(id).is_some_and(|node| {
                                node.kind == crate::StructuralNodeKind::ChurchButtress
                            })
                        })
                        .count()
                        == 2
            })
        })
        && church.choir.vault_bearing_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                church.choir.vault_spring_nodes.contains(&interface.node)
                    && church.choir.vault_thrust_solids.iter().any(|solid_id| {
                        solids.get(solid_id).is_some_and(|solid| {
                            bounds_overlap_3d(
                                (interface.bounds.min, interface.bounds.max),
                                resolved_solid_bounds(solid),
                                0.03,
                            )
                        })
                    })
            })
        });
    if church.choir.apse_facets.len() != usize::from(program.apse_sides)
        || church.choir.radial_buttress_nodes.len() != usize::from(program.apse_sides)
        || church
            .choir
            .radial_buttress_nodes
            .iter()
            .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
        || church.choir.pier_nodes.len() != usize::from(program.choir_bays) * 2
        || church
            .choir
            .pier_nodes
            .iter()
            .any(|id| nodes.get(id).is_none_or(|node| !node.grounded))
        || church.choir.buttress_nodes.len() != usize::from(program.choir_bays) * 2
        || church.choir.vault_solids.len() != usize::from(program.choir_bays) * 2
        || church.choir.floor_solids.is_empty()
        || !choir_arches_valid
        || !choir_load_valid
    {
        issues.push(issue(
            "invalid_church_choir_apse",
            "choir/apse lacks continuous five-sided wall, radial support, or floor authority"
                .to_owned(),
        ));
    }
    let portal_valid = |id: crate::OpeningAssemblyId| {
        openings.get(&id).is_some_and(|opening| {
            opening.use_kind == crate::OpeningUse::Door
                && opening.profile.interior_width_metres() >= 0.90
                && opening.profile.clear_height_metres() >= 1.90
        })
    };
    let stair_valid = plan.stairs.get(church.tower.stair_index).is_some_and(|stair| {
        matches!(stair, Stair::Spiral { base_height_metres, rise_metres, .. }
            if *base_height_metres <= 0.01
                && (*base_height_metres + *rise_metres - church.datum.bell_floor_metres).abs() <= 0.05)
    });
    if !portal_valid(church.tower.west_portal)
        || !portal_valid(church.tower.nave_passage)
        || !stair_valid
        || church.tower.landing_solids.len() < 3
        || church.tower.guard_solids.len() < 5
        || church.tower.bell_floor_solids.len() != 4
        || church.tower.bell_floor_solids.iter().any(|id| {
            solids
                .get(id)
                .is_none_or(|solid| solid.role != SolidRole::ChurchBellFloor)
        })
        || church.tower.roof_ladder_solids.len() < 15
        || church.tower.roof_ladder_solids.iter().any(|id| {
            solids
                .get(id)
                .is_none_or(|solid| solid.role != SolidRole::ChurchServiceLadder)
        })
        || church.tower.bell_frame_solids.len() < 2
        || solids
            .get(&church.tower.bell_solid)
            .is_none_or(|solid| solid.role != SolidRole::ChurchBell)
        || church.tower.bell_openings.len() != 8
        || church.tower.bell_openings.iter().any(|id| {
            openings.get(id).is_none_or(|opening| {
                opening.use_kind != crate::OpeningUse::BellOpening
                    || opening.closure.layers != vec![crate::ClosureKind::TimberLouvre]
                    || opening
                        .closure
                        .layers
                        .contains(&crate::ClosureKind::LeadedGlazing)
            })
        })
    {
        issues.push(issue(
            "invalid_church_west_tower",
            "west tower lacks portal/passage, guarded service stair, bell floor/frame/bell, or paired unglazed louvres".to_owned(),
        ));
    }
    let stairwell_bounds = (
        Vec3::new(
            church.tower.centre.x - 1.21,
            church.datum.bell_floor_metres - 0.20,
            church.tower.centre.y - 1.21,
        ),
        Vec3::new(
            church.tower.centre.x + 1.21,
            church.datum.bell_floor_metres + 0.20,
            church.tower.centre.y + 1.21,
        ),
    );
    let floor_area = church
        .tower
        .bell_floor_solids
        .iter()
        .filter_map(|id| solids.get(id))
        .map(|solid| solid.size.x * solid.size.z)
        .sum::<f32>();
    let floor_blocks_stairwell = church.tower.bell_floor_solids.iter().any(|id| {
        solids.get(id).is_some_and(|solid| {
            bounds_overlap_3d(resolved_solid_bounds(solid), stairwell_bounds, 0.005)
        })
    });
    let ladder_bounds = church
        .tower
        .roof_ladder_solids
        .iter()
        .filter_map(|id| solids.get(id))
        .fold(None, |bounds, solid| {
            let (min, max) = resolved_solid_bounds(solid);
            Some(
                bounds.map_or((min, max), |(old_min, old_max): (Vec3, Vec3)| {
                    (old_min.min(min), old_max.max(max))
                }),
            )
        });
    let tower_wall_supports = church
        .tower
        .wall_ids
        .iter()
        .filter_map(|id| walls.get(id))
        .map(|wall| wall.support_node)
        .collect::<BTreeSet<_>>();
    let bell_floor_bearing_valid = church.tower.bell_floor_solids.iter().all(|id| {
        solids.get(id).is_some_and(|solid| {
            solid.supported_by.iter().all(|support| {
                nodes.get(support).is_some_and(|stage| {
                    !stage.grounded
                        && stage.supported_by.len() >= 2
                        && stage
                            .supported_by
                            .iter()
                            .all(|wall| tower_wall_supports.contains(wall))
                })
            })
        })
    });
    if floor_blocks_stairwell
        || !(11.5..=12.6).contains(&floor_area)
        || !bell_floor_bearing_valid
        || ladder_bounds.is_none_or(|(min, max)| {
            min.y > church.datum.bell_floor_metres + 0.25
                || max.y < 21.25
                || min.x < church.tower.centre.x - church.tower.footprint_size_metres.x * 0.5
                || max.x > church.tower.centre.x + church.tower.footprint_size_metres.x * 0.5
                || min.z < church.tower.centre.y - church.tower.footprint_size_metres.y * 0.5
                || max.z > church.tower.centre.y + church.tower.footprint_size_metres.y * 0.5
        })
    {
        issues.push(issue(
            "invalid_church_tower_service_geometry",
            "bell floor must be a tower-wall-bearing guarded ring around a clear stairwell with a contained floor-to-roof service ladder".to_owned(),
        ));
    }
    let item_bounds = |id: ResolvedItemId| {
        solids
            .get(&id)
            .map(|solid| resolved_solid_bounds(solid))
            .or_else(|| {
                surfaces
                    .get(&id)
                    .map(|surface| (surface.bounds.min, surface.bounds.max))
            })
    };
    let bounds_gap = |a: (Vec3, Vec3), b: (Vec3, Vec3)| {
        let axis_gap = |a_min: f32, a_max: f32, b_min: f32, b_max: f32| {
            (b_min - a_max).max(a_min - b_max).max(0.0)
        };
        Vec3::new(
            axis_gap(a.0.x, a.1.x, b.0.x, b.1.x),
            axis_gap(a.0.y, a.1.y, b.0.y, b.1.y),
            axis_gap(a.0.z, a.1.z, b.0.z, b.1.z),
        )
    };
    let route_surface_point = |id: ResolvedItemId| {
        item_bounds(id)
            .map(|(min, max)| Vec3::new((min.x + max.x) * 0.5, max.y, (min.z + max.z) * 0.5))
    };
    let route_crosses_opening = |edge: &crate::ChurchRouteEdge| {
        let Some(opening_id) = edge.through_opening else {
            return true;
        };
        let Some(((opening, wall), void)) = openings
            .get(&opening_id)
            .zip(
                openings
                    .get(&opening_id)
                    .and_then(|opening| walls.get(&opening.host_wall)),
            )
            .zip(
                openings
                    .get(&opening_id)
                    .and_then(|opening| voids.get(&opening.void_id)),
            )
        else {
            return false;
        };
        if edge.from == edge.to
            || edge.clear_width_metres > opening.profile.interior_width_metres() + 0.001
            || edge.clear_headroom_metres > opening.profile.clear_height_metres() + 0.001
            || opening.sectional_void.len() < 5
        {
            return false;
        }
        let Some((from, to)) = route_surface_point(edge.from).zip(route_surface_point(edge.to))
        else {
            return false;
        };
        let travel = Vec2::new(to.x - from.x, to.z - from.z);
        let along_outward = travel.dot(opening.frame.outward);
        if along_outward.abs() < wall.thickness_metres * 0.5 {
            return false;
        }
        opening.sectional_void.iter().all(|slice| {
            let plane = opening.frame.origin
                + opening.frame.outward * wall.thickness_metres * (0.5 - slice.depth_fraction);
            let t = (plane - Vec2::new(from.x, from.z)).dot(opening.frame.outward) / along_outward;
            if !(-0.001..=1.001).contains(&t) {
                return false;
            }
            let foot = from.lerp(to, t.clamp(0.0, 1.0));
            let plan_point = Vec2::new(foot.x, foot.z);
            let lateral = (plan_point - opening.frame.origin)
                .dot(opening.frame.tangent)
                .abs();
            let inside_void_envelope = foot.x >= void.bounds.min.x - 0.005
                && foot.x <= void.bounds.max.x + 0.005
                && foot.z >= void.bounds.min.z - 0.005
                && foot.z <= void.bounds.max.z + 0.005
                && foot.y >= void.bounds.min.y - 0.005
                && foot.y + edge.clear_headroom_metres <= void.bounds.max.y + 0.005;
            lateral + edge.clear_width_metres * 0.5 <= slice.width_metres * 0.5 + 0.005
                && foot.y >= opening.sill_elevation_metres - 0.005
                && foot.y + edge.clear_headroom_metres
                    <= opening.sill_elevation_metres + slice.height_metres + 0.005
                && inside_void_envelope
        })
    };
    let route_contract_invalid = church.circulation.iter().any(|route| {
        if route.width_metres < 0.90
            || route.headroom_metres < 1.90
            || route.waypoints.len() < 2
            || route
                .surface_ids
                .iter()
                .any(|id| !surfaces.contains_key(id))
            || route
                .traversable_solid_ids
                .iter()
                .any(|id| !solids.contains_key(id))
            || route.edges.is_empty()
        {
            return true;
        }
        let allowed = route
            .surface_ids
            .iter()
            .chain(&route.traversable_solid_ids)
            .copied()
            .collect::<BTreeSet<_>>();
        if route.edges.iter().any(|edge| {
            edge.clear_width_metres < 0.90
                || edge.clear_headroom_metres < 1.90
                || !allowed.contains(&edge.from)
                || !allowed.contains(&edge.to)
                || edge.through_opening.is_some_and(|opening| {
                    !route.opening_ids.contains(&opening) || !openings.contains_key(&opening)
                })
                || !route_crosses_opening(edge)
                || (edge.through_opening.is_none()
                    && item_bounds(edge.from)
                        .zip(item_bounds(edge.to))
                        .is_none_or(|(from, to)| bounds_gap(from, to).length() > 0.62))
        }) {
            return true;
        }
        let mut adjacency = BTreeMap::<ResolvedItemId, Vec<ResolvedItemId>>::new();
        for edge in &route.edges {
            adjacency.entry(edge.from).or_default().push(edge.to);
            adjacency.entry(edge.to).or_default().push(edge.from);
        }
        let Some(start) = route.surface_ids.first().copied() else {
            return true;
        };
        let mut reached = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            for next in adjacency.get(&current).into_iter().flatten() {
                if reached.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        !allowed.is_subset(&reached)
    });
    let public_route_invalid = church
        .circulation
        .iter()
        .find(|route| route.kind == crate::ChurchRouteKind::PublicProcessional)
        .is_none_or(|route| {
            let expected_surfaces = [
                church.tower.exterior_approach_surface,
                church.tower.vestibule_surface,
                church.tower.nave_entry_surface,
            ];
            expected_surfaces
                .iter()
                .any(|id| !route.surface_ids.contains(id))
                || route.width_metres > 1.80 + 0.001
                || !route.edges.iter().any(|edge| {
                    edge.from == church.tower.exterior_approach_surface
                        && edge.to == church.tower.vestibule_surface
                        && edge.through_opening == Some(church.tower.west_portal)
                })
                || !route.edges.iter().any(|edge| {
                    edge.from == church.tower.vestibule_surface
                        && edge.to == church.tower.nave_entry_surface
                        && edge.through_opening == Some(church.tower.nave_passage)
                })
        });
    let bell_route_invalid = church
        .circulation
        .iter()
        .find(|route| route.kind == crate::ChurchRouteKind::BellService)
        .is_none_or(|route| {
            let ladder_rungs = church.tower.roof_ladder_solids.iter().skip(2);
            let route_degree = |id: ResolvedItemId| {
                route
                    .edges
                    .iter()
                    .filter(|edge| edge.from == id || edge.to == id)
                    .count()
            };
            let tower_wall_solids = church
                .tower
                .wall_ids
                .iter()
                .filter_map(|id| walls.get(id))
                .flat_map(|wall| wall.host_solids.iter())
                .filter_map(|id| solids.get(id))
                .collect::<Vec<_>>();
            let bell_obstacles = church
                .tower
                .bell_frame_solids
                .iter()
                .chain(std::iter::once(&church.tower.bell_solid))
                .filter_map(|id| solids.get(id))
                .collect::<Vec<_>>();
            let stair_bearing_invalid =
                nodes
                    .get(&church.tower.stair_bearing_node)
                    .is_none_or(|bearing| {
                        bearing.grounded
                            || bearing.supported_by.len() < 2
                            || bearing
                                .supported_by
                                .iter()
                                .any(|support| !tower_wall_supports.contains(support))
                    })
                    || solids
                        .get(&church.tower.stair_newel_solid)
                        .is_none_or(|newel| {
                            newel.role != SolidRole::ChurchStairNewel
                                || newel.supported_by != vec![church.tower.stair_bearing_node]
                        })
                    || church.tower.stair_tread_interfaces.len()
                        != church.tower.stair_tread_solids.len()
                    || church
                        .tower
                        .stair_tread_solids
                        .iter()
                        .zip(&church.tower.stair_tread_interfaces)
                        .any(|(tread_id, interface_id)| {
                            solids
                                .get(tread_id)
                                .zip(interfaces.get(interface_id))
                                .zip(solids.get(&church.tower.stair_newel_solid))
                                .is_none_or(|((tread, interface), newel)| {
                                    interface.node != church.tower.stair_bearing_node
                                        || tread.supported_by
                                            != vec![church.tower.stair_bearing_node]
                                        || !resolved_solid_overlaps_bounds(
                                            tread,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.005,
                                        )
                                        || !resolved_solid_overlaps_bounds(
                                            newel,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.005,
                                        )
                                })
                        });
            let route_point = |id: ResolvedItemId| {
                solids
                    .get(&id)
                    .map(|solid| {
                        Vec3::new(
                            solid.centre.x,
                            resolved_solid_bounds(solid).1.y + 0.015,
                            solid.centre.z,
                        )
                    })
                    .or_else(|| route_surface_point(id).map(|point| point + Vec3::Y * 0.015))
            };
            let route_items = route
                .surface_ids
                .iter()
                .chain(&route.traversable_solid_ids)
                .copied()
                .collect::<BTreeSet<_>>();
            let sweep_obstacles = tower_wall_solids
                .iter()
                .copied()
                .chain(
                    church
                        .tower
                        .guard_solids
                        .iter()
                        .filter_map(|id| solids.get(id)),
                )
                .chain(bell_obstacles.iter().copied())
                .chain(solids.get(&church.tower.stair_newel_solid))
                .collect::<Vec<_>>();
            let swept_route_invalid = route.edges.iter().any(|edge| {
                route_point(edge.from)
                    .zip(route_point(edge.to))
                    .is_none_or(|(from, to)| {
                        // Seven samples per adjacency are sufficient for the
                        // project's coarse 0.34 m treads while also sampling
                        // both ends and the turn chord.  The 0.90 x 1.90 m
                        // prism is the animation/collision gate, not a claim
                        // about universal medieval stair dimensions.
                        let travel = Vec2::new(to.x - from.x, to.z - from.z);
                        let along = if travel.length_squared() > 0.0001 {
                            travel.normalize()
                        } else {
                            Vec2::X
                        };
                        let across = Vec2::new(-along.y, along.x);
                        (0..=6).any(|sample| {
                            let t = sample as f32 / 6.0;
                            let foot = from.lerp(to, t);
                            sweep_obstacles.iter().any(|obstacle| {
                                !route_items.contains(&obstacle.id)
                                    && oriented_occupant_overlaps_solid(
                                        foot, along, across, obstacle, 0.015,
                                    )
                            })
                        })
                    })
            });
            stair_bearing_invalid
                || swept_route_invalid
                || church.tower.stair_tread_solids.len() != 72
                || !route
                    .traversable_solid_ids
                    .contains(&church.tower.stair_tread_solids[0])
                || church.tower.stair_tread_solids.iter().any(|id| {
                    solids.get(id).is_none_or(|solid| {
                        solid.role != SolidRole::ChurchStairTread
                            || solid.size.x < 0.90
                            || solid.centre.x
                                < church.tower.centre.x - church.tower.footprint_size_metres.x * 0.5
                                    + 0.90
                            || solid.centre.x
                                > church.tower.centre.x + church.tower.footprint_size_metres.x * 0.5
                                    - 0.90
                            || solid.centre.z
                                < church.tower.centre.y - church.tower.footprint_size_metres.y * 0.5
                                    + 0.90
                            || solid.centre.z
                                > church.tower.centre.y + church.tower.footprint_size_metres.y * 0.5
                                    - 0.90
                            || tower_wall_solids.iter().any(|wall| {
                                bounds_overlap_3d(
                                    resolved_solid_bounds(solid),
                                    resolved_solid_bounds(wall),
                                    0.01,
                                )
                            })
                    })
                })
                || church
                    .tower
                    .stair_tread_solids
                    .iter()
                    .zip(church.tower.stair_tread_solids.iter().skip(18))
                    .any(|(lower, upper)| {
                        solids
                            .get(lower)
                            .zip(solids.get(upper))
                            .is_none_or(|(a, b)| b.centre.y - a.centre.y < 1.90)
                    })
                || church
                    .tower
                    .landing_solids
                    .iter()
                    .chain(&church.tower.bell_floor_solids)
                    .chain(ladder_rungs)
                    .any(|id| !route.traversable_solid_ids.contains(id))
                || church
                    .tower
                    .bell_floor_corner_surfaces
                    .iter()
                    .any(|id| !route.surface_ids.contains(id))
                || church
                    .tower
                    .landing_solids
                    .iter()
                    .any(|id| route_degree(*id) < 2)
                || church.tower.roof_ladder_solids.iter().skip(2).any(|id| {
                    solids.get(id).is_none_or(|rung| {
                        bell_obstacles.iter().any(|obstacle| {
                            bounds_overlap_3d(
                                resolved_solid_bounds(rung),
                                resolved_solid_bounds(obstacle),
                                0.02,
                            )
                        })
                    })
                })
                || !route
                    .surface_ids
                    .contains(&church.tower.roof_service_surface)
                || !route.surface_ids.contains(&church.tower.vestibule_surface)
                || route.opening_ids.contains(&church.tower.nave_passage)
                || !route.edges.iter().any(|edge| {
                    edge.through_opening.is_none()
                        && edge.from == church.tower.vestibule_surface
                        && edge.to == church.tower.stair_tread_solids[0]
                })
        });
    if church.circulation.len() < 2
        || route_contract_invalid
        || public_route_invalid
        || bell_route_invalid
    {
        issues.push(issue(
            "invalid_church_circulation",
            format!(
                "public or bell-service circulation lacks an adjacent, swept 0.90 x 1.90 m route across its physical surfaces (contract={route_contract_invalid}, public={public_route_invalid}, bell={bell_route_invalid})"
            ),
        ));
    }
    if church.roof_assemblies.len() < 6
        || church
            .roof_assemblies
            .iter()
            .any(|id| !plan.roof_assemblies.iter().any(|roof| roof.id == *id))
    {
        issues.push(issue(
            "invalid_church_roof_program",
            "church nave/aisle/transept/apse/tower roofs are not bound to Stage4 assemblies"
                .to_owned(),
        ));
    }
}
