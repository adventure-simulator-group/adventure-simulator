fn audit_resolved_geometry(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if plan.resolved_geometry.schema_version != 2 {
        issues.push(issue(
            "stale_resolver_schema",
            "resolved geometry does not use resolver schema 2".to_owned(),
        ));
    }
    let owners = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| node.owner)
        .collect::<std::collections::HashSet<_>>();
    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let mut item_ids = std::collections::HashSet::new();
    for (index, solid) in plan.resolved_geometry.solids.iter().enumerate() {
        if solid.size.min_element() <= 0.0
            || !owners.contains(&solid.owner)
            || !item_ids.insert(solid.id)
            || solid.supported_by.is_empty()
            || solid
                .supported_by
                .iter()
                .any(|node| !nodes.contains_key(node))
        {
            issues.push(issue(
                "invalid_resolved_geometry",
                format!("resolved solid {index} {:?} owner={} centre={:?} size={:?} support={:?} has invalid extent, owner, or support provenance",solid.role,solid.owner.0,solid.centre,solid.size,solid.supported_by),
            ));
        }
        let has_bearing = plan
            .resolved_geometry
            .support_interfaces
            .iter()
            .any(|bearing| {
                let timber_member = matches!(
                    solid.role,
                    SolidRole::FrameSill
                        | SolidRole::FramePost
                        | SolidRole::FramePlate
                        | SolidRole::FrameRail
                        | SolidRole::FrameJoist
                        | SolidRole::FrameGirder
                        | SolidRole::FrameTie
                        | SolidRole::FrameBrace
                        | SolidRole::FrameJettyBeam
                        | SolidRole::FrameKnagge
                        | SolidRole::FrameGableMember
                        | SolidRole::FrameDormerTrimmer
                        | SolidRole::FrameOrnament
                );
                bearing.owner == solid.owner
                    && solid.supported_by.contains(&bearing.node)
                    && if timber_member {
                        resolved_solid_overlaps_bounds(
                            solid,
                            (bearing.bounds.min, bearing.bounds.max),
                            0.001,
                        )
                    } else {
                        bounds_overlap_3d(
                            resolved_solid_bounds(solid),
                            (bearing.bounds.min, bearing.bounds.max),
                            0.001,
                        )
                    }
            });
        if !has_bearing {
            issues.push(issue(
                "missing_positive_bearing",
                format!(
                    "resolved solid {} has no positive bearing interface",
                    solid.id.0
                ),
            ));
        }
    }
    for surface in &plan.resolved_geometry.surfaces {
        if !item_ids.insert(surface.id) {
            issues.push(issue(
                "duplicate_resolved_item_id",
                format!(
                    "duplicate surface item {} owner {} role {:?}",
                    surface.id.0, surface.owner.0, surface.role
                ),
            ));
        }
    }
    for void in &plan.resolved_geometry.voids {
        if !item_ids.insert(void.id) {
            issues.push(issue(
                "duplicate_resolved_item_id",
                format!("duplicate item {}", void.id.0),
            ));
        }
    }
    fn reaches_ground(
        id: crate::StructuralNodeId,
        nodes: &std::collections::HashMap<crate::StructuralNodeId, &crate::StructuralNode>,
        visiting: &mut std::collections::HashSet<crate::StructuralNodeId>,
    ) -> bool {
        let Some(node) = nodes.get(&id) else {
            return false;
        };
        if node.grounded {
            return true;
        }
        if !visiting.insert(id) {
            return false;
        }
        let reaches = node
            .supported_by
            .iter()
            .all(|parent| reaches_ground(*parent, nodes, visiting));
        visiting.remove(&id);
        reaches && !node.supported_by.is_empty()
    }
    for node in nodes.values() {
        if !reaches_ground(node.id, &nodes, &mut std::collections::HashSet::new()) {
            issues.push(issue(
                "unsupported_resolved_structure",
                format!(
                    "structural node {} {:?} at {:?} supports {:?} does not reach ground through an acyclic graph",
                    node.id.0, node.kind, node.position, node.supported_by
                ),
            ));
        }
    }
    for left in 0..plan.resolved_geometry.solids.len() {
        for right in (left + 1)..plan.resolved_geometry.solids.len() {
            let a = &plan.resolved_geometry.solids[left];
            let b = &plan.resolved_geometry.solids[right];
            let separated_by_chord =
                (matches!(a.shape, crate::ResolvedSolidShape::RoundTowerShell { .. })
                    && tower_chord_void_separates(plan, a, b))
                    || (matches!(b.shape, crate::ResolvedSolidShape::RoundTowerShell { .. })
                        && tower_chord_void_separates(plan, b, a));
            let enclosed_by_round_shell =
                round_shell_clears_inner_solid(a, b) || round_shell_clears_inner_solid(b, a);
            if a.owner != b.owner
                && !separated_by_chord
                && !enclosed_by_round_shell
                && resolved_shape_overlap(a, b, 0.025)
            {
                let (a_min, a_max) = resolved_solid_bounds(a);
                let (b_min, b_max) = resolved_solid_bounds(b);
                let overlap_min = a_min.max(b_min);
                let overlap_max = a_max.min(b_max);
                let overlap_size = overlap_max - overlap_min;
                let penetration = overlap_size.x.min(overlap_size.z);
                let valid_drain_contact =
                    plan.resolved_geometry
                        .roof_drainage_outlets
                        .iter()
                        .any(|station| {
                            let pair = [(a, b), (b, a)];
                            pair.into_iter().any(|(spout, host_solid)| {
                                station.downspout == Some(spout.id)
                                    && spout.role == SolidRole::RoofGutter
                                    && station.host_wall.is_some_and(|host_id| {
                                        plan.wall_assemblies.iter().any(|wall| {
                                            wall.id == host_id
                                                && wall.host_solids.contains(&host_solid.id)
                                        })
                                    })
                            })
                        });
                let is_timber_frame_role = |role: SolidRole| {
                    matches!(
                        role,
                        SolidRole::FrameSill
                            | SolidRole::FramePost
                            | SolidRole::FramePlate
                            | SolidRole::FrameRail
                            | SolidRole::FrameJoist
                            | SolidRole::FrameGirder
                            | SolidRole::FrameTie
                            | SolidRole::FrameBrace
                            | SolidRole::FrameJettyBeam
                            | SolidRole::FrameKnagge
                            | SolidRole::FrameFloor
                            | SolidRole::FrameGableMember
                            | SolidRole::FrameDormerTrimmer
                            | SolidRole::FrameOrnament
                    )
                };
                let valid_frame_opening_contact = plan.timber_frame.as_ref().is_some_and(|frame| {
                    [(a, b), (b, a)].into_iter().any(|(frame_solid, closure)| {
                        closure.role == SolidRole::OpeningClosure
                            && frame.members.iter().any(|member| {
                                member.solid == frame_solid.id
                                    && matches!(
                                        member.role,
                                        crate::TimberMemberRole::PrimaryPost
                                            | crate::TimberMemberRole::CornerPost
                                            | crate::TimberMemberRole::IntermediatePost
                                            | crate::TimberMemberRole::Rail
                                    )
                                    && (frame.bays.iter().any(|bay| {
                                        bay.member_ids.contains(&member.id)
                                            && bay.opening.is_some_and(|opening_id| {
                                                plan.opening_assemblies.iter().any(|opening| {
                                                    opening.id == opening_id
                                                        && opening
                                                            .closure_solids
                                                            .contains(&closure.id)
                                                })
                                            })
                                    }) || plan.opening_assemblies.iter().any(|opening| {
                                        if !opening.closure_solids.contains(&closure.id) {
                                            return false;
                                        }
                                        let Some(void) = plan
                                            .resolved_geometry
                                            .voids
                                            .iter()
                                            .find(|void| void.id == opening.void_id)
                                        else {
                                            return false;
                                        };
                                        let size = void.bounds.max - void.bounds.min;
                                        let half = (size.x * opening.frame.tangent.x.abs()
                                            + size.z * opening.frame.tangent.y.abs())
                                            * 0.5;
                                        let point = Vec2::new(member.start.x, member.start.z);
                                        ((point - opening.frame.origin)
                                            .dot(opening.frame.tangent)
                                            .abs()
                                            - half)
                                            .abs()
                                            <= member.section_metres.x * 0.6 + 0.02
                                    }))
                            })
                    })
                });
                let valid_frame_contact = (is_timber_frame_role(a.role)
                    || is_timber_frame_role(b.role))
                    && (!oriented_cuboids_overlap(a, b, 0.012)
                        || matches!(
                            (a.role, b.role),
                            (
                                SolidRole::FrameSill
                                    | SolidRole::FramePost
                                    | SolidRole::FramePlate
                                    | SolidRole::FrameRail
                                    | SolidRole::FrameJoist
                                    | SolidRole::FrameGirder
                                    | SolidRole::FrameTie
                                    | SolidRole::FrameBrace
                                    | SolidRole::FrameJettyBeam
                                    | SolidRole::FrameKnagge
                                    | SolidRole::FrameFloor
                                    | SolidRole::FrameGableMember
                                    | SolidRole::FrameDormerTrimmer,
                                SolidRole::WallHost | SolidRole::FrameInfill
                                    | SolidRole::OpeningJamb
                                    | SolidRole::OpeningSill
                                    | SolidRole::OpeningHead
                                    | SolidRole::OpeningSpandrel
                            ) | (
                                SolidRole::WallHost | SolidRole::FrameInfill
                                    | SolidRole::OpeningJamb
                                    | SolidRole::OpeningSill
                                    | SolidRole::OpeningHead
                                    | SolidRole::OpeningSpandrel,
                                SolidRole::FrameSill
                                    | SolidRole::FramePost
                                    | SolidRole::FramePlate
                                    | SolidRole::FrameRail
                                    | SolidRole::FrameJoist
                                    | SolidRole::FrameGirder
                                    | SolidRole::FrameTie
                                    | SolidRole::FrameBrace
                                    | SolidRole::FrameJettyBeam
                                    | SolidRole::FrameKnagge
                                    | SolidRole::FrameFloor
                                    | SolidRole::FrameGableMember
                                    | SolidRole::FrameDormerTrimmer
                            ) | (
                                SolidRole::FrameDormerTrimmer,
                                SolidRole::RoofFlashing | SolidRole::RoofFraming
                            ) | (
                                SolidRole::RoofFlashing | SolidRole::RoofFraming,
                                SolidRole::FrameDormerTrimmer
                            ) | (SolidRole::FramePost, SolidRole::RoofFlashing)
                                | (SolidRole::RoofFlashing, SolidRole::FramePost)
                        ));
                let valid_frame_gutter_contact = matches!(
                    (a.role, b.role),
                    (SolidRole::FrameFloor, SolidRole::RoofGutter)
                        | (SolidRole::RoofGutter, SolidRole::FrameFloor)
                ) && penetration <= 0.10
                    || plan.timber_frame.as_ref().is_some_and(|frame| {
                        [(a, b), (b, a)].into_iter().any(|(timber, gutter)| {
                            timber.role == SolidRole::FrameGableMember
                                && gutter.role == SolidRole::RoofGutter
                                && penetration
                                    <= frame
                                        .members
                                        .iter()
                                        .find(|member| member.solid == timber.id)
                                        .map_or(0.0, |member| {
                                            member.section_metres.max_element() + 0.05
                                        })
                                && frame.members.iter().any(|member| {
                                    member.solid == timber.id
                                        && (member
                                            .support_interfaces
                                            .iter()
                                            .chain(&frame.roof_bearing_interfaces)
                                            .any(|id| {
                                                plan.resolved_geometry
                                                    .support_interfaces
                                                    .iter()
                                                    .find(|interface| interface.id == *id)
                                                    .is_some_and(|interface| {
                                                        resolved_solid_overlaps_bounds(
                                                            timber,
                                                            (
                                                                interface.bounds.min,
                                                                interface.bounds.max,
                                                            ),
                                                            0.001,
                                                        ) && resolved_solid_overlaps_bounds(
                                                            gutter,
                                                            (
                                                                interface.bounds.min,
                                                                interface.bounds.max,
                                                            ),
                                                            0.001,
                                                        )
                                                    })
                                            })
                                            || {
                                                let (min, max) = resolved_solid_bounds(gutter);
                                                let endpoint_radius =
                                                    member.section_metres.max_element() * 0.6
                                                        + 0.02;
                                                [member.start, member.end].into_iter().any(
                                                    |point| {
                                                        point
                                                            .cmpge(
                                                                min - Vec3::splat(endpoint_radius),
                                                            )
                                                            .all()
                                                            && point
                                                                .cmple(
                                                                    max + Vec3::splat(
                                                                        endpoint_radius,
                                                                    ),
                                                                )
                                                                .all()
                                                    },
                                                )
                                            })
                                })
                        })
                    });
                let valid_frame_roof_contact = plan.timber_frame.as_ref().is_some_and(|frame| {
                    [(a, b), (b, a)].into_iter().any(|(timber, roof)| {
                        is_timber_frame_role(timber.role)
                            && matches!(roof.role, SolidRole::RoofFraming | SolidRole::RoofPlate)
                            && frame.members.iter().any(|member| {
                                member.solid == timber.id
                                    && member.support_interfaces.iter().any(|id| {
                                        plan.resolved_geometry
                                            .support_interfaces
                                            .iter()
                                            .find(|interface| interface.id == *id)
                                            .is_some_and(|interface| {
                                                resolved_solid_overlaps_bounds(
                                                    roof,
                                                    (interface.bounds.min, interface.bounds.max),
                                                    0.015,
                                                )
                                            })
                                    })
                            })
                    })
                });
                let valid_child_gable_verge_contact =
                    plan.timber_frame.as_ref().is_some_and(|frame| {
                        [(a, b), (b, a)].into_iter().any(|(timber, verge)| {
                            verge.role == SolidRole::RoofEdgeTreatment
                                && frame.members.iter().any(|member| {
                                    member.solid == timber.id
                                        && matches!(
                                            member.role,
                                            crate::TimberMemberRole::WallPlate
                                                | crate::TimberMemberRole::GablePost
                                                | crate::TimberMemberRole::Rafter
                                        )
                                        && penetration
                                            <= member.section_metres.max_element() + 0.05
                                        && frame.bays.iter().any(|bay| {
                                            bay.member_ids.contains(&member.id)
                                                && bay.wall.is_some_and(|wall_id| {
                                                    plan.wall_assemblies
                                                        .iter()
                                                        .find(|wall| wall.id == wall_id)
                                                        .is_some_and(|wall| {
                                                            matches!(
                                                                wall.source,
                                                                crate::WallSourceId::RoofChildFront { roof }
                                                                    if plan.roof_assemblies.iter().any(|child| {
                                                                        child.id == roof
                                                                            && child.owner == verge.owner
                                                                    })
                                                            )
                                                        })
                                                })
                                        })
                                })
                        })
                    });
                let valid_bond = valid_drain_contact
                    || valid_frame_opening_contact
                    || valid_frame_contact
                    || valid_frame_gutter_contact
                    || valid_frame_roof_contact
                    || valid_child_gable_verge_contact
                    || plan.resolved_geometry.junction_bonds.iter().any(|bond| {
                        bond.owners.contains(&a.owner)
                            && bond.owners.contains(&b.owner)
                            && overlap_min
                                .cmpge(bond.bounds.min - Vec3::splat(0.025))
                                .all()
                            && overlap_max
                                .cmple(bond.bounds.max + Vec3::splat(0.025))
                                .all()
                            && penetration <= bond.maximum_penetration_metres + 0.025
                            && bond.minimum_interface_area_square_metres > 0.0
                            && (bond.maximum_penetration_metres <= 0.18
                                || matches!(
                                    (a.role, b.role),
                                    (
                                        SolidRole::RoofFlashing,
                                        SolidRole::WallHost
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                    ) | (
                                        SolidRole::WallHost
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                            | SolidRole::ArtilleryRevetment
                                            | SolidRole::ArtilleryEarthCore
                                            | SolidRole::ArtilleryRetainingWall,
                                        SolidRole::RoofFlashing
                                    )
                                )
                                || matches!(
                                    (a.role, b.role),
                                    (
                                        SolidRole::WallHost
                                            | SolidRole::DefenseHostWall
                                            | SolidRole::CircuitWalk
                                            | SolidRole::LoadBearing
                                            | SolidRole::Breastwork
                                            | SolidRole::WalkSurface
                                            | SolidRole::DrainageChannel
                                            | SolidRole::Landing
                                            | SolidRole::DefenseHostButtress
                                            | SolidRole::ProjectionSupport
                                            | SolidRole::GalleryFloor
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningSill
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                            | SolidRole::ArtilleryRevetment
                                            | SolidRole::ArtilleryEarthCore
                                            | SolidRole::ArtilleryRetainingWall,
                                        SolidRole::WallHost
                                            | SolidRole::DefenseHostWall
                                            | SolidRole::CircuitWalk
                                            | SolidRole::LoadBearing
                                            | SolidRole::Breastwork
                                            | SolidRole::WalkSurface
                                            | SolidRole::DrainageChannel
                                            | SolidRole::Landing
                                            | SolidRole::DefenseHostButtress
                                            | SolidRole::ProjectionSupport
                                            | SolidRole::GalleryFloor
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningSill
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                            | SolidRole::ArtilleryRevetment
                                            | SolidRole::ArtilleryEarthCore
                                            | SolidRole::ArtilleryRetainingWall
                                    )
                                ))
                    });
                if !valid_bond {
                    issues.push(issue(
                        "undeclared_solid_overlap",
                        format!(
                            "resolved {} owner {} {:?} at {:?} size {:?} and {} owner {} {:?} at {:?} size {:?} overlap beyond a local bonded junction timber={:?}",
                            a.id.0, a.owner.0, a.role, a.centre, a.size,
                            b.id.0, b.owner.0, b.role, b.centre, b.size,
                            plan.timber_frame.as_ref().and_then(|frame| frame.members.iter().find(|member| member.solid == a.id || member.solid == b.id)).map(|member| (member.role, member.start, member.end, member.section_metres))
                        ),
                    ));
                }
            }
        }
    }
    for bond in &plan.resolved_geometry.junction_bonds {
        let valid_interface = valid_tower_chord_bond(plan, bond)
            || plan.resolved_geometry.solids.iter().any(|a| {
                a.owner == bond.owners[0]
                    && plan.resolved_geometry.solids.iter().any(|b| {
                        b.owner == bond.owners[1]
                            && bonded_interface_metrics(a, b).is_some_and(
                                |(contact_min, contact_max, area, penetration)| {
                                    contact_min
                                        .cmpge(bond.bounds.min - Vec3::splat(0.025))
                                        .all()
                                        && contact_max
                                            .cmple(bond.bounds.max + Vec3::splat(0.025))
                                            .all()
                                        && area + 0.005 >= bond.minimum_interface_area_square_metres
                                        && penetration <= bond.maximum_penetration_metres + 0.025
                                },
                            )
                    })
            });
        if !valid_interface {
            issues.push(issue(
                "invalid_crown_junction_bond",
                format!(
                    "crown bond {} does not contain a positive-area, gap-limited interface",
                    bond.id.0
                ),
            ));
        }
    }
    for (index, void) in plan.resolved_geometry.voids.iter().enumerate() {
        let void_bounds = (void.bounds.min, void.bounds.max);
        let blocking_solid = plan.resolved_geometry.solids.iter().find(|solid| {
            let exact_opening_piece = (void.role == VoidRole::WallOpening)
                && plan.opening_assemblies.iter().any(|opening| {
                    opening.void_id == void.id
                        && (opening.jamb_solids.contains(&solid.id)
                            || opening.sill_solid == Some(solid.id)
                            || opening.head_solid == solid.id
                            || opening.spandrel_solid == solid.id)
                });
            let round_shell_clears_central_room = void.role == VoidRole::ArtilleryCasemate
                && matches!(solid.shape, crate::ResolvedSolidShape::RoundTowerShell { inner_radius_metres, .. } if {
                    let corners = [
                        Vec2::new(void.bounds.min.x, void.bounds.min.z),
                        Vec2::new(void.bounds.min.x, void.bounds.max.z),
                        Vec2::new(void.bounds.max.x, void.bounds.min.z),
                        Vec2::new(void.bounds.max.x, void.bounds.max.z),
                    ];
                    corners.into_iter().all(|corner| corner.distance(Vec2::new(solid.centre.x, solid.centre.z)) < inner_radius_metres - 0.01)
                });
            let artillery_scupper_subtraction = void.role == VoidRole::Drain
                && plan.artillery_castle.as_ref().is_some_and(|castle| {
                    castle
                        .drainage_routes
                        .iter()
                        .chain(&castle.ditch.drainage_routes)
                        .any(|route_id| {
                            plan.resolved_geometry.drainage_routes.iter().any(|route| {
                                route.id == *route_id && route.outlet_void == void.id
                            })
                        })
                })
                && matches!(
                    solid.role,
                    SolidRole::ArtilleryRevetment
                        | SolidRole::ArtilleryEarthCore
                        | SolidRole::WallHost
                        | SolidRole::DrainageFloor
                );
            let roof_gutter_outlet_subtraction = void.role == VoidRole::Drain
                && solid.role == SolidRole::RoofGutter
                && plan
                    .resolved_geometry
                    .roof_drainage_outlets
                    .iter()
                    .find(|station| station.outlet_void == void.id)
                    .is_some_and(|station| {
                        station.member_networks.iter().any(|network_id| {
                            plan.resolved_geometry
                                .roof_drainage_networks
                                .iter()
                                .find(|network| network.id == *network_id)
                                .is_some_and(|network| {
                                    network.channel_floor == solid.id
                                        || network.channel_lips.contains(&solid.id)
                                        || network.collector_solids.contains(&solid.id)
                                })
                        })
                    });
            let casemate_furnishing = void.role == VoidRole::ArtilleryCasemate
                && matches!(solid.role, SolidRole::ArtilleryStairTread | SolidRole::WeaponMount);
            solid.owner == void.subtracts_from
                && !round_shell_clears_central_room
                && !casemate_furnishing
                && !artillery_scupper_subtraction
                && !roof_gutter_outlet_subtraction
                && !(void.role == VoidRole::DryDitch
                    && matches!(
                        solid.role,
                        SolidRole::DitchScarp
                            | SolidRole::DitchCounterscarp
                            | SolidRole::DitchFloor
                    ))
                && !(void.role == VoidRole::Passage && solid.role == SolidRole::OpeningClosure)
                && !exact_opening_piece
                && (void.role != VoidRole::WallOpening
                    || matches!(solid.role, SolidRole::WallHost | SolidRole::OpeningSill)
                    || solid.role == SolidRole::OpeningSpandrel
                    || (solid.role == SolidRole::OpeningJamb
                        && !matches!(solid.shape, crate::ResolvedSolidShape::SplayedReveal { .. }))
                    || (solid.role == SolidRole::OpeningHead
                        && matches!(solid.shape, crate::ResolvedSolidShape::Cuboid)))
                && (void.role != VoidRole::RoofOpening
                    || !matches!(
                        solid.role,
                        SolidRole::RoofFlashing | SolidRole::RoofFraming | SolidRole::WallHost
                    ))
                && resolved_shape_overlaps_bounds(solid, void_bounds, 0.001)
        });
        if void.owner != void.subtracts_from
            || !owners.contains(&void.subtracts_from)
            || blocking_solid.is_some()
        {
            issues.push(issue(
                "unresolved_void_subtraction",
                format!(
                    "resolved void {index} {:?} {:?}..{:?} is not an open subtraction from its owner; blocker={:?}",
                    void.role,
                    void.bounds.min,
                    void.bounds.max,
                    blocking_solid.map(|solid| (solid.id, solid.role, solid.centre, solid.size))
                ),
            ));
        }
    }
    for route in &plan.resolved_geometry.drainage_routes {
        let outward_drop = route.inlet.y - route.outlet.y;
        if outward_drop < 0.04
            || !plan
                .resolved_geometry
                .voids
                .iter()
                .any(|void| void.owner == route.owner && void.id == route.outlet_void)
        {
            issues.push(issue(
                "broken_crown_drainage",
                format!(
                    "drainage route {} does not reach a lower owned outlet",
                    route.id.0
                ),
            ));
        }
    }
}
