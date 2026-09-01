fn audit_timber_frame(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    let expected = match plan.archetype {
        BuildingArchetype::TownHouse => Some(crate::TimberFrameProgramKind::NarrowUrbanTownHouse),
        BuildingArchetype::HallHouse => {
            Some(crate::TimberFrameProgramKind::NorthernTwoPostHallHouse)
        }
        BuildingArchetype::FachwerkCottage => {
            Some(crate::TimberFrameProgramKind::DirectRoofCottage)
        }
        BuildingArchetype::FachwerkMerchantHouse => {
            Some(crate::TimberFrameProgramKind::JettiedMerchantHouse)
        }
        BuildingArchetype::RenaissanceTownHall => {
            Some(crate::TimberFrameProgramKind::CivicMasonryTimberHall)
        }
        _ => None,
    };
    let Some(expected) = expected else {
        if plan.timber_frame.is_some() {
            issues.push(issue(
                "invalid_timber_program",
                "non-civilian fixture declares a semantic timber-frame program".to_owned(),
            ));
        }
        return;
    };
    let Some(frame) = &plan.timber_frame else {
        issues.push(issue(
            "missing_authoritative_timber_frame",
            "civilian fixture has no semantic timber-frame assembly".to_owned(),
        ));
        return;
    };
    if frame.program != expected || frame.id.0 == 0 {
        issues.push(issue(
            "invalid_timber_program",
            "timber-frame program does not match the curated archetype".to_owned(),
        ));
    }

    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let interfaces = plan
        .resolved_geometry
        .support_interfaces
        .iter()
        .map(|interface| (interface.id, interface))
        .collect::<std::collections::HashMap<_, _>>();
    let members = frame
        .members
        .iter()
        .map(|member| (member.id, member))
        .collect::<std::collections::HashMap<_, _>>();
    let joints = frame
        .joints
        .iter()
        .map(|joint| (joint.id, joint))
        .collect::<std::collections::HashMap<_, _>>();
    let mut ids = std::collections::HashSet::new();
    if members.len() != frame.members.len()
        || joints.len() != frame.joints.len()
        || frame
            .facades
            .iter()
            .any(|facade| !ids.insert((0_u8, facade.id.0)))
        || frame
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .chain(frame.internal_lines.iter())
            .any(|line| !ids.insert((1_u8, line.id.0)))
        || frame
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .chain(frame.internal_lines.iter())
            .flat_map(|line| &line.storeys)
            .any(|storey| !ids.insert((2_u8, storey.id.0)))
        || frame.bays.iter().any(|bay| !ids.insert((3_u8, bay.id.0)))
    {
        issues.push(issue(
            "duplicate_timber_frame_id",
            "timber frame contains duplicate assembly-local IDs".to_owned(),
        ));
    }

    let frame_owner = frame
        .members
        .first()
        .map_or(crate::GeometryOwnerId(0), |member| member.owner);
    for node in nodes.values().filter(|node| node.owner == frame_owner) {
        for parent in &node.supported_by {
            let endpoint_member = frame.members.iter().any(|member| {
                member.structural
                    && ((member.start_node == node.id && member.end_node == *parent)
                        || (member.end_node == node.id && member.start_node == *parent))
            });
            let measured_body_contact = interfaces.values().any(|interface| {
                interface.owner == frame_owner
                    && (interface.node == node.id || interface.node == *parent)
                    && frame.members.iter().any(|member| {
                        member.structural
                            && ((interface.node == node.id
                                && [member.start_node, member.end_node].contains(parent))
                                || (interface.node == *parent
                                    && [member.start_node, member.end_node].contains(&node.id)))
                            && solids.get(&member.solid).is_some_and(|solid| {
                                resolved_solid_overlaps_bounds(
                                    solid,
                                    (interface.bounds.min, interface.bounds.max),
                                    0.001,
                                )
                            })
                    })
            });
            let measured_external_contact = nodes.get(parent).is_some_and(|support| {
                support.owner != node.owner
                    && interfaces.values().any(|interface| {
                        interface.node == node.id
                            && (solids.values().any(|solid| {
                                (solid.owner == support.owner
                                    || solid.supported_by.contains(parent))
                                    && resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                            }) || plan.roof_assemblies.iter().any(|roof| {
                                roof.support_nodes.contains(parent)
                                    && roof.faces.iter().any(|face| {
                                        let centre =
                                            (interface.bounds.min + interface.bounds.max) * 0.5;
                                        let point = Vec2::new(centre.x, centre.z);
                                        roof_face_contains_plan_point_inclusive(face, point)
                                            && roof_face_height(face, point).is_some_and(|height| {
                                                let underside = height
                                                    - face.plane.normal.normalize_or_zero().y
                                                        * face.thickness_metres;
                                                underside >= interface.bounds.min.y - 0.002
                                                    && underside <= interface.bounds.max.y + 0.002
                                            })
                                    })
                            }))
                    })
            });
            if !endpoint_member && !measured_body_contact && !measured_external_contact {
                issues.push(issue(
                    "false_timber_support_edge",
                    format!(
                        "timber node {} claims support {} without a member/contact interface",
                        node.id.0, parent.0
                    ),
                ));
            }
        }
    }

    for member in &frame.members {
        let endpoints = [nodes.get(&member.start_node), nodes.get(&member.end_node)];
        let member_joints = [
            joints.get(&member.start_joint),
            joints.get(&member.end_joint),
        ];
        let solid = solids.get(&member.solid);
        let valid_role = solid.is_some_and(|solid| {
            matches!(
                (member.role, solid.role),
                (crate::TimberMemberRole::Sill, SolidRole::FrameSill)
                    | (
                        crate::TimberMemberRole::PrimaryPost
                            | crate::TimberMemberRole::CornerPost
                            | crate::TimberMemberRole::IntermediatePost,
                        SolidRole::FramePost
                    )
                    | (crate::TimberMemberRole::WallPlate, SolidRole::FramePlate)
                    | (crate::TimberMemberRole::Rail, SolidRole::FrameRail)
                    | (crate::TimberMemberRole::FloorJoist, SolidRole::FrameJoist)
                    | (
                        crate::TimberMemberRole::Girder | crate::TimberMemberRole::Purlin,
                        SolidRole::FrameGirder
                    )
                    | (crate::TimberMemberRole::TransverseTie, SolidRole::FrameTie)
                    | (
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace,
                        SolidRole::FrameBrace
                    )
                    | (
                        crate::TimberMemberRole::JettyBeam,
                        SolidRole::FrameJettyBeam
                    )
                    | (crate::TimberMemberRole::Knagge, SolidRole::FrameKnagge)
                    | (
                        crate::TimberMemberRole::GableTie
                            | crate::TimberMemberRole::GablePost
                            | crate::TimberMemberRole::Rafter
                            | crate::TimberMemberRole::Collar,
                        SolidRole::FrameGableMember
                    )
                    | (
                        crate::TimberMemberRole::DormerTrimmer,
                        SolidRole::FrameDormerTrimmer
                    )
                    | (crate::TimberMemberRole::Ornament, SolidRole::FrameOrnament)
            )
        });
        let endpoint_contact = endpoints[0]
            .is_some_and(|node| node.position.distance(member.start) <= 0.003)
            && endpoints[1].is_some_and(|node| node.position.distance(member.end) <= 0.003)
            && member.start_node != member.end_node
            && member.start.distance(member.end) > 0.05;
        let joint_contact = member_joints[0].is_some_and(|joint| {
            joint.node == member.start_node
                && joint.member_ids.contains(&member.id)
                && joint.contact_area_square_metres >= 0.008
        }) && member_joints[1].is_some_and(|joint| {
            joint.node == member.end_node
                && joint.member_ids.contains(&member.id)
                && joint.contact_area_square_metres >= 0.008
        });
        let bearing_contact = member.support_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                interface.owner == member.owner
                    && [member.start_node, member.end_node].contains(&interface.node)
                    && solid.is_some_and(|solid| {
                        resolved_solid_overlaps_bounds(
                            solid,
                            (interface.bounds.min, interface.bounds.max),
                            0.001,
                        )
                    })
            })
        });
        let solid_correspondence = solid.is_some_and(|solid| {
            solid.centre.distance((member.start + member.end) * 0.5) <= 0.003
                && (solid.size.y - member.start.distance(member.end)).abs() <= 0.003
                && (solid.size.x - member.section_metres.x).abs() <= 0.003
                && (solid.size.z - member.section_metres.y).abs() <= 0.003
        });
        if !valid_role
            || !endpoint_contact
            || !joint_contact
            || !bearing_contact
            || !solid_correspondence
            || member.section_metres.min_element() < 0.08
            || member.structural == (member.role == crate::TimberMemberRole::Ornament)
        {
            issues.push(issue(
                "invalid_timber_member_joint",
                format!(
                    "timber member {} {:?} has false truth role={valid_role} endpoint={endpoint_contact} joint={joint_contact} bearing={bearing_contact} solid={solid_correspondence}",
                    member.id.0, member.role
                ),
            ));
        }
    }
    for (index, left) in frame.members.iter().enumerate() {
        if left.role == crate::TimberMemberRole::Ornament {
            continue;
        }
        for right in frame.members.iter().skip(index + 1) {
            if right.role == crate::TimberMemberRole::Ornament {
                continue;
            }
            let nested_posts = matches!(
                left.role,
                crate::TimberMemberRole::PrimaryPost
                    | crate::TimberMemberRole::CornerPost
                    | crate::TimberMemberRole::IntermediatePost
            ) && matches!(
                right.role,
                crate::TimberMemberRole::PrimaryPost
                    | crate::TimberMemberRole::CornerPost
                    | crate::TimberMemberRole::IntermediatePost
            ) && Vec2::new(left.start.x, left.start.z)
                .distance(Vec2::new(right.start.x, right.start.z))
                < 0.02
                && solids
                    .get(&left.solid)
                    .zip(solids.get(&right.solid))
                    .is_some_and(|(left_solid, right_solid)| {
                        resolved_solids_overlap_positive_volume(left_solid, right_solid, 0.005)
                    });
            if nested_posts {
                issues.push(issue(
                    "overlapping_timber_members",
                    format!(
                        "timber posts {} {:?}->{:?} and {} {:?}->{:?} claim the same positive-volume load path",
                        left.id.0, left.start, left.end, right.id.0, right.start, right.end
                    ),
                ));
            }
        }
    }
    for joint in &frame.joints {
        let unique = joint
            .member_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        if joint.id.0 == 0
            || joint.contact_area_square_metres < 0.008
            || unique.len() != joint.member_ids.len()
            || unique.is_empty()
            || unique.iter().any(|id| {
                members.get(id).is_none_or(|member| {
                    ![member.start_joint, member.end_joint].contains(&joint.id)
                        || ![member.start_node, member.end_node].contains(&joint.node)
                })
            })
        {
            issues.push(issue(
                "orphan_timber_joint",
                format!(
                    "timber joint {} at node {} {:?} with members {:?} is orphaned, duplicated, or drifts from its members",
                    joint.id.0,
                    joint.node.0,
                    nodes.get(&joint.node).map(|node| node.position),
                    joint.member_ids
                ),
            ));
            continue;
        }
        let participant_roles = joint
            .member_ids
            .iter()
            .filter_map(|id| members.get(id).map(|member| member.role))
            .collect::<Vec<_>>();
        let has = |role| participant_roles.contains(&role);
        let grounded = nodes.get(&joint.node).is_some_and(|node| node.grounded);
        let expected_kind = if grounded {
            crate::TimberJointKind::FoundationBearing
        } else if (has(crate::TimberMemberRole::JettyBeam)
            && (has(crate::TimberMemberRole::Knagge)
                || has(crate::TimberMemberRole::Girder)
                || has(crate::TimberMemberRole::Sill)))
            || (has(crate::TimberMemberRole::Knagge)
                && (has(crate::TimberMemberRole::PrimaryPost)
                    || has(crate::TimberMemberRole::CornerPost)))
        {
            crate::TimberJointKind::JettyBearing
        } else if (has(crate::TimberMemberRole::Rafter)
            && (has(crate::TimberMemberRole::WallPlate)
                || has(crate::TimberMemberRole::Collar)
                || has(crate::TimberMemberRole::GablePost)))
            || (has(crate::TimberMemberRole::DormerTrimmer)
                && (has(crate::TimberMemberRole::Rafter) || has(crate::TimberMemberRole::Purlin)))
            || (has(crate::TimberMemberRole::Purlin)
                && (has(crate::TimberMemberRole::PrimaryPost)
                    || has(crate::TimberMemberRole::GablePost)))
        {
            crate::TimberJointKind::RoofSeat
        } else if (has(crate::TimberMemberRole::FloorJoist) && has(crate::TimberMemberRole::Girder))
            || (has(crate::TimberMemberRole::TransverseTie)
                && (has(crate::TimberMemberRole::PrimaryPost)
                    || has(crate::TimberMemberRole::Purlin)))
        {
            crate::TimberJointKind::HousedBeam
        } else if participant_roles.iter().any(|role| {
            matches!(
                role,
                crate::TimberMemberRole::HeadBrace
                    | crate::TimberMemberRole::FootBrace
                    | crate::TimberMemberRole::StoreyBrace
            )
        }) && participant_roles.len() >= 2
        {
            crate::TimberJointKind::Lap
        } else if participant_roles
            .iter()
            .filter(|role| {
                matches!(
                    role,
                    crate::TimberMemberRole::Sill | crate::TimberMemberRole::WallPlate
                )
            })
            .count()
            >= 2
        {
            crate::TimberJointKind::Scarf
        } else {
            crate::TimberJointKind::MortiseTenon
        };
        let contact_set = joint
            .contact_interfaces
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let contacts_valid = contact_set.len() == joint.member_ids.len()
            && joint.member_ids.iter().all(|member_id| {
                members.get(member_id).is_some_and(|member| {
                    let interface_id = if member.start_node == joint.node {
                        member.support_interfaces[0]
                    } else {
                        member.support_interfaces[1]
                    };
                    contact_set.contains(&interface_id)
                        && interfaces.get(&interface_id).is_some_and(|interface| {
                            interface.node == joint.node
                                && solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                        })
                })
            });
        let expected_participants = joint
            .member_ids
            .iter()
            .filter_map(|member_id| {
                let member = members.get(member_id)?;
                let axis = if member.start_node == joint.node {
                    member.end - member.start
                } else if member.end_node == joint.node {
                    member.start - member.end
                } else {
                    return None;
                }
                .normalize_or_zero();
                Some((*member_id, member.role, axis))
            })
            .collect::<Vec<_>>();
        let participants_valid = joint.participants.len() == expected_participants.len()
            && expected_participants.iter().all(|(member_id, _, axis)| {
                joint.participants.iter().any(|participant| {
                    participant.member == *member_id
                        && participant.axis_from_joint.dot(*axis) >= 0.999
                        && participant.reaction_direction.dot(-*axis) >= 0.999
                        && (participant.axis_from_joint + participant.reaction_direction).length()
                            <= 0.002
                })
            });
        let role_axis = |role| {
            expected_participants
                .iter()
                .find_map(|(_, participant_role, axis)| {
                    (*participant_role == role).then_some(*axis)
                })
        };
        let downward = |axis: Vec3| if axis.y <= 0.0 { axis } else { -axis };
        let gravity_biased = |axis: Vec3, lateral_weight: f32| {
            let lateral = Vec3::new(axis.x, 0.0, axis.z).normalize_or_zero();
            (lateral * lateral_weight - Vec3::Y).normalize_or_zero()
        };
        let expected_load = match joint.kind {
            crate::TimberJointKind::JettyBearing => {
                role_axis(crate::TimberMemberRole::JettyBeam).map(|axis| gravity_biased(axis, 0.65))
            }
            crate::TimberJointKind::Lap => [
                crate::TimberMemberRole::HeadBrace,
                crate::TimberMemberRole::FootBrace,
                crate::TimberMemberRole::StoreyBrace,
            ]
            .into_iter()
            .find_map(role_axis)
            .map(downward),
            crate::TimberJointKind::RoofSeat => role_axis(crate::TimberMemberRole::Rafter)
                .or_else(|| role_axis(crate::TimberMemberRole::Purlin))
                .map(downward),
            crate::TimberJointKind::HousedBeam => role_axis(crate::TimberMemberRole::FloorJoist)
                .or_else(|| role_axis(crate::TimberMemberRole::TransverseTie))
                .map(|axis| gravity_biased(axis, 0.25)),
            crate::TimberJointKind::Scarf => role_axis(crate::TimberMemberRole::Sill)
                .or_else(|| role_axis(crate::TimberMemberRole::WallPlate))
                .map(|axis| gravity_biased(axis, 0.20)),
            _ => expected_participants
                .iter()
                .max_by(|left, right| left.2.y.abs().total_cmp(&right.2.y.abs()))
                .map(|(_, _, axis)| {
                    if axis.y.abs() >= 0.35 {
                        downward(*axis)
                    } else {
                        gravity_biased(*axis, 0.15)
                    }
                }),
        }
        .unwrap_or(-Vec3::Y)
        .normalize_or_zero();
        let load_valid = participants_valid
            && joint.load_direction.length() >= 0.99
            && joint.load_direction.dot(expected_load) >= 0.999;
        let material_valid = joint.member_ids.iter().all(|id| {
            members.get(id).is_some_and(|member| {
                member.material == frame.material
                    && member.phase != crate::TimberFramePhase::NonStructuralFinish
            })
        });
        if joint.kind != expected_kind || !contacts_valid || !load_valid || !material_valid {
            issues.push(issue(
                "invalid_timber_joint_contact",
                format!(
                    "timber joint {} {:?} does not match participants {:?}, contacts, load direction, or member material",
                    joint.id.0, joint.kind, participant_roles
                ),
            ));
        }
    }

    let allowed_joint = |kind| {
        matches!(
            kind,
            crate::TimberJointKind::FoundationBearing
                | crate::TimberJointKind::MortiseTenon
                | crate::TimberJointKind::HousedBeam
                | crate::TimberJointKind::Scarf
                | crate::TimberJointKind::Lap
                | crate::TimberJointKind::RoofSeat
                | crate::TimberJointKind::JettyBearing
        )
    };
    let has_joint = |kind| frame.joints.iter().any(|joint| joint.kind == kind);
    if frame.joints.iter().any(|joint| !allowed_joint(joint.kind))
        || !has_joint(crate::TimberJointKind::FoundationBearing)
        || (plan.storeys.len() > 1 && !has_joint(crate::TimberJointKind::HousedBeam))
        || !has_joint(crate::TimberJointKind::Lap)
        || !has_joint(crate::TimberJointKind::RoofSeat)
        || (plan.upper_storey_projection_metres > 0.0
            && !has_joint(crate::TimberJointKind::JettyBearing))
    {
        issues.push(issue(
            "invalid_timber_joint_vocabulary",
            "timber frame does not use the compact physical Stage 6 joint vocabulary".to_owned(),
        ));
    }

    let unique_floor_levels = frame
        .floors
        .iter()
        .map(|floor| floor.level)
        .collect::<std::collections::HashSet<_>>();
    let floor_program_valid = frame.floors.len() == plan.storeys.len()
        && unique_floor_levels.len() == frame.floors.len()
        && frame.floors.iter().all(|floor| {
            let floor_solids = floor
                .floor_solids
                .iter()
                .filter_map(|id| solids.get(id).copied())
                .collect::<Vec<_>>();
            let surface = plan
                .resolved_geometry
                .surfaces
                .iter()
                .find(|surface| surface.id == floor.route_surface);
            let joists = floor
                .joist_members
                .iter()
                .filter_map(|id| members.get(id).copied())
                .collect::<Vec<_>>();
            let girders = floor
                .girder_members
                .iter()
                .filter_map(|id| members.get(id).copied())
                .collect::<Vec<_>>();
            let mut floor_contacts_valid = true;
            let mut housed_contacts_valid = true;
            let mut every_piece_supported = true;
            let floor_bearings_valid = if floor.level == 0 {
                floor.bearing_interfaces.iter().any(|id| {
                    interfaces.get(id).is_some_and(|interface| {
                        floor_solids.iter().any(|solid| {
                            bounds_overlap_3d(
                                resolved_solid_bounds(solid),
                                (interface.bounds.min, interface.bounds.max),
                                0.001,
                            )
                        })
                    })
                })
            } else {
                floor_contacts_valid = !floor.floor_joist_interfaces.is_empty()
                    && floor.floor_joist_interfaces.iter().all(|id| {
                        interfaces.get(id).is_some_and(|interface| {
                            floor_solids.iter().any(|solid| {
                                bounds_overlap_3d(
                                    resolved_solid_bounds(solid),
                                    (interface.bounds.min, interface.bounds.max),
                                    0.001,
                                )
                            }) && joists.iter().any(|member| {
                                solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            })
                        })
                    });
                housed_contacts_valid = floor.joist_girder_interfaces.iter().all(|id| {
                        interfaces.get(id).is_some_and(|interface| {
                            joists.iter().any(|member| {
                                solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            }) && girders.iter().any(|member| {
                                solids.get(&member.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            })
                        })
                    });
                every_piece_supported = floor_solids.iter().all(|solid| {
                        floor.floor_joist_interfaces.iter().any(|id| {
                            interfaces.get(id).is_some_and(|interface| {
                                solid.supported_by.contains(&interface.node)
                                    && bounds_overlap_3d(
                                        resolved_solid_bounds(solid),
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                            })
                        })
                    });
                floor_contacts_valid && housed_contacts_valid && every_piece_supported
            };
            let stair_valid = if floor.level == 0 {
                floor.stair_connection.is_none()
            } else {
                floor.stair_connection.is_some_and(|point| {
                    surface.is_some_and(|surface| {
                        point.x >= surface.bounds.min.x
                            && point.x <= surface.bounds.max.x
                            && point.y >= surface.bounds.min.z
                            && point.y <= surface.bounds.max.z
                    }) && plan.stairs.iter().any(|stair| match *stair {
                        crate::Stair::Straight { start, .. } => start.distance(point) <= 0.02,
                        crate::Stair::Spiral { centre, .. } => centre.distance(point) <= 0.02,
                    })
                })
            };
            let valid = floor_solids.len() == floor.floor_solids.len()
                && floor_solids.iter().all(|solid| {
                    solid.role == SolidRole::FrameFloor
                        && solid.size.y >= 0.12
                        && !solid.supported_by.is_empty()
                })
                && surface.is_some_and(|surface| {
                surface.role == crate::SurfaceRole::TimberCirculation
                    && surface.bounds.max.x - surface.bounds.min.x >= 0.90
                    && surface.bounds.max.z - surface.bounds.min.z >= 0.90
            }) && joists.len() == floor.joist_members.len()
                && (floor.level == 0 || joists.len() >= 3)
                && joists
                    .iter()
                    .all(|member| member.role == crate::TimberMemberRole::FloorJoist)
                && girders.len() == floor.girder_members.len()
                && (floor.level == 0 || girders.len() >= 2)
                && girders
                    .iter()
                    .all(|member| member.role == crate::TimberMemberRole::Girder)
                && floor_bearings_valid
                && stair_valid;
            if !valid {
                issues.push(issue(
                    "unsupported_timber_floor_route",
                    format!(
                        "timber floor {} invalid: solids {}/{}, joists {}/{}, girders {}/{}, bearings {} (floor {}, housed {}, pieces {}), stair {}",
                        floor.level,
                        floor_solids.len(),
                        floor.floor_solids.len(),
                        joists.len(),
                        floor.joist_members.len(),
                        girders.len(),
                        floor.girder_members.len(),
                        floor_bearings_valid,
                        floor_contacts_valid,
                        housed_contacts_valid,
                        every_piece_supported,
                        stair_valid,
                    ),
                ));
            }
            valid
        });
    if !floor_program_valid {
        issues.push(issue(
            "unsupported_timber_floor_route",
            "occupied timber levels lack joists, girders, bearings, or stair-connected routes"
                .to_owned(),
        ));
    }

    // Occupied civilian levels are one physical route, not a collection of
    // floor labels.  Validate adjacency through the exact entry void, each
    // tread/landing surface, and every floor subtraction before allowing the
    // route to satisfy the program contract.
    let circulation = &frame.circulation;
    let route_nodes = circulation
        .nodes
        .iter()
        .map(|node| (node.surface, node))
        .collect::<std::collections::HashMap<_, _>>();
    let route_surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .filter(|surface| surface.role == crate::SurfaceRole::TimberCirculation)
        .map(|surface| (surface.id, surface))
        .collect::<std::collections::HashMap<_, _>>();
    let entry = circulation.entry_opening.and_then(|id| {
        plan.opening_assemblies
            .iter()
            .find(|opening| opening.id == id)
    });
    let node_geometry_valid = route_nodes.len() == circulation.nodes.len()
        && circulation.nodes.iter().all(|node| {
            route_surfaces.get(&node.surface).is_some_and(|surface| {
                node.position.x >= surface.bounds.min.x - 0.01
                    && node.position.x <= surface.bounds.max.x + 0.01
                    && node.position.y >= surface.bounds.min.y - 0.02
                    && node.position.y <= surface.bounds.max.y + 0.02
                    && node.position.z >= surface.bounds.min.z - 0.01
                    && node.position.z <= surface.bounds.max.z + 0.01
            })
        });
    let entry_geometry_valid = entry.is_some_and(|opening| {
        let Some(void) = plan
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == opening.void_id)
        else {
            return false;
        };
        let approach = circulation
            .nodes
            .iter()
            .find(|node| node.kind == crate::TimberRouteNodeKind::ExteriorApproach);
        let threshold = circulation
            .nodes
            .iter()
            .find(|node| node.kind == crate::TimberRouteNodeKind::DoorThreshold);
        let min_width = opening
            .sectional_void
            .iter()
            .map(|slice| slice.width_metres)
            .fold(f32::INFINITY, f32::min);
        let min_height = opening
            .sectional_void
            .iter()
            .map(|slice| slice.height_metres)
            .fold(f32::INFINITY, f32::min);
        approach
            .zip(threshold)
            .is_some_and(|(approach, threshold)| {
                let lateral = (Vec2::new(threshold.position.x, threshold.position.z)
                    - opening.frame.origin)
                    .dot(opening.frame.tangent)
                    .abs();
                lateral <= min_width * 0.5 - 0.45
                    && threshold.position.x >= void.bounds.min.x - 0.01
                    && threshold.position.x <= void.bounds.max.x + 0.01
                    && threshold.position.z >= void.bounds.min.z - 0.01
                    && threshold.position.z <= void.bounds.max.z + 0.01
                    && circulation.edges.iter().any(|edge| {
                        edge.from == approach.surface
                            && edge.to == threshold.surface
                            && edge.clear_width_metres <= min_width + 0.001
                            && edge.clear_headroom_metres <= min_height + 0.001
                    })
            })
    });
    let edges_valid = circulation.edges.iter().all(|edge| {
        edge.from != edge.to
            && route_nodes.contains_key(&edge.from)
            && route_nodes.contains_key(&edge.to)
            && edge.clear_width_metres >= 0.90
            && edge.clear_headroom_metres >= 1.90
    });
    let start = circulation
        .nodes
        .iter()
        .find(|node| node.kind == crate::TimberRouteNodeKind::ExteriorApproach)
        .map(|node| node.surface);
    let reachable = start.map_or_else(std::collections::HashSet::new, |start| {
        let mut seen = std::collections::HashSet::from([start]);
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            for edge in &circulation.edges {
                if edge.from == current && seen.insert(edge.to) {
                    queue.push_back(edge.to);
                }
            }
        }
        seen
    });
    let graph_valid = reachable.len() == circulation.nodes.len()
        && circulation
            .nodes
            .iter()
            .all(|node| reachable.contains(&node.surface))
        && (0..plan.storeys.len() as u16).all(|level| {
            circulation.nodes.iter().any(|node| {
                node.level == level
                    && matches!(
                        node.kind,
                        crate::TimberRouteNodeKind::GroundFloor
                            | crate::TimberRouteNodeKind::UpperFloor
                    )
            })
        });
    let tread_nodes = circulation
        .nodes
        .iter()
        .filter(|node| node.kind == crate::TimberRouteNodeKind::StairTread)
        .collect::<Vec<_>>();
    let tread_geometry_valid = tread_nodes.len() == circulation.stair_solids.len()
        && tread_nodes.iter().all(|node| {
            circulation.stair_solids.iter().any(|id| {
                solids.get(id).is_some_and(|solid| {
                    solid.role == SolidRole::Landing
                        && !solid.supported_by.is_empty()
                        && (solid.centre.y + solid.size.y * 0.5 - node.position.y).abs() <= 0.012
                        && Vec2::new(solid.centre.x, solid.centre.z)
                            .distance(Vec2::new(node.position.x, node.position.z))
                            <= 0.03
                        && solid.supported_by.iter().all(|node_id| {
                            interfaces.values().any(|interface| {
                                interface.node == *node_id
                                    && resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                            })
                        })
                })
            })
        });
    let cuts_valid = circulation.floor_cut_voids.len() + 1 == frame.floors.len()
        && circulation.floor_cut_voids.iter().all(|id| {
            plan.resolved_geometry
                .voids
                .iter()
                .find(|void| void.id == *id)
                .is_some_and(|void| {
                    void.role == crate::VoidRole::AccessPortal
                        && frame
                            .floors
                            .iter()
                            .flat_map(|floor| &floor.floor_solids)
                            .all(|solid_id| {
                                solids.get(solid_id).is_none_or(|solid| {
                                    !bounds_overlap_3d(
                                        resolved_solid_bounds(solid),
                                        (void.bounds.min, void.bounds.max),
                                        0.001,
                                    )
                                })
                            })
                })
        });
    let swept_blocker = circulation.edges.iter().find_map(|edge| {
        let Some((from, to)) = route_nodes.get(&edge.from).zip(route_nodes.get(&edge.to)) else {
            return Some((
                edge.from,
                edge.to,
                0,
                Vec3::ZERO,
                Vec3::ZERO,
                crate::ResolvedItemId(0),
                Vec3::ZERO,
                Vec3::ZERO,
            ));
        };
        let plan_delta = Vec2::new(
            to.position.x - from.position.x,
            to.position.z - from.position.z,
        );
        let along = plan_delta.normalize_or_zero();
        let along = if along.length_squared() < 0.9 {
            Vec2::X
        } else {
            along
        };
        let across = Vec2::new(-along.y, along.x);
        (0..=12).find_map(|sample| {
            let foot = from.position.lerp(to.position, sample as f32 / 12.0);
            solids.values().find_map(|solid| {
                (matches!(
                    solid.role,
                    SolidRole::WallHost
                        | SolidRole::OpeningJamb
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                ) && oriented_occupant_overlaps_solid(foot, along, across, solid, 0.01))
                .then_some((
                    edge.from,
                    edge.to,
                    sample,
                    from.position,
                    to.position,
                    solid.id,
                    solid.centre,
                    solid.size,
                ))
            })
        })
    });
    let swept_clear = swept_blocker.is_none();
    if !node_geometry_valid
        || !entry_geometry_valid
        || !edges_valid
        || !graph_valid
        || !tread_geometry_valid
        || !cuts_valid
        || !swept_clear
    {
        issues.push(issue(
            "invalid_timber_circulation",
            format!(
                "timber circulation lacks physical entry/levels/stairs (nodes={node_geometry_valid}, entry={entry_geometry_valid}, edges={edges_valid}, graph={graph_valid}, treads={tread_geometry_valid}, cuts={cuts_valid}, sweep={swept_clear} blocker={swept_blocker:?})"
            ),
        ));
    }

    for bay in &frame.bays {
        let bay_members = bay
            .member_ids
            .iter()
            .filter_map(|id| members.get(id).copied())
            .collect::<Vec<_>>();
        let residual_valid = bay
            .wall
            .and_then(|wall_id| plan.wall_assemblies.iter().find(|wall| wall.id == wall_id))
            .is_some_and(|wall| timber_infill_residual_valid(plan, frame, wall, bay, &solids));
        if bay.member_ids.len() != bay_members.len()
            || bay.infill_solids.is_empty()
            || bay.infill_solids.iter().any(|id| !solids.contains_key(id))
            || !residual_valid
        {
            issues.push(issue(
                "invalid_timber_bay",
                format!(
                    "timber bay {} lacks exact member/Gefach residual authority (residual={residual_valid})",
                    bay.id.0,
                ),
            ));
        }
        if let Some(opening_id) = bay.opening {
            let Some(opening) = plan
                .opening_assemblies
                .iter()
                .find(|opening| opening.id == opening_id)
            else {
                issues.push(issue(
                    "invalid_timber_opening_bay",
                    format!("timber bay {} references a missing opening", bay.id.0),
                ));
                continue;
            };
            let posts = bay_members
                .iter()
                .filter(|member| member.role == crate::TimberMemberRole::IntermediatePost)
                .count();
            let rails = bay_members
                .iter()
                .filter(|member| member.role == crate::TimberMemberRole::Rail)
                .count();
            let forbidden = bay.wall.is_some_and(|wall_id| {
                plan.wall_assemblies
                    .iter()
                    .find(|wall| wall.id == wall_id)
                    .is_some_and(|wall| {
                        bay_members.iter().any(|member| {
                            if !member.structural {
                                return false;
                            }
                            let mut sample_points = (0..=16)
                                .map(|sample| member.start.lerp(member.end, sample as f32 / 16.0))
                                .collect::<Vec<_>>();
                            if let Some(solid) = solids.get(&member.solid) {
                                sample_points.push(solid.centre);
                            }
                            sample_points.into_iter().any(|point| {
                                let plan_point = Vec2::new(point.x, point.z);
                                let depth = 0.5
                                    - (plan_point - opening.frame.origin)
                                        .dot(opening.frame.outward)
                                        / wall.thickness_metres;
                                if !(-0.001..=1.001).contains(&depth) {
                                    return false;
                                }
                                let lateral = (plan_point - opening.frame.origin)
                                    .dot(opening.frame.tangent)
                                    .abs();
                                let (width, height) = opening
                                    .sectional_void
                                    .windows(2)
                                    .find_map(|pair| {
                                        (depth >= pair[0].depth_fraction - 0.001
                                            && depth <= pair[1].depth_fraction + 0.001)
                                            .then(|| {
                                                let t = ((depth - pair[0].depth_fraction)
                                                    / (pair[1].depth_fraction
                                                        - pair[0].depth_fraction)
                                                        .max(0.0001))
                                                .clamp(0.0, 1.0);
                                                (
                                                    pair[0].width_metres
                                                        + (pair[1].width_metres
                                                            - pair[0].width_metres)
                                                            * t,
                                                    pair[0].height_metres
                                                        + (pair[1].height_metres
                                                            - pair[0].height_metres)
                                                            * t,
                                                )
                                            })
                                    })
                                    .unwrap_or_else(|| {
                                        let slice = opening
                                            .sectional_void
                                            .first()
                                            .expect("audited opening has slices");
                                        (slice.width_metres, slice.height_metres)
                                    });
                                let member_half = member.section_metres.min_element() * 0.48;
                                lateral + member_half < width * 0.5 - 0.01
                                    && point.y - member_half > opening.sill_elevation_metres + 0.01
                                    && point.y + member_half
                                        < opening.sill_elevation_metres + height - 0.01
                            })
                        })
                    })
            });
            let exact_residual = bay.wall.is_some_and(|wall_id| {
                plan.wall_assemblies
                    .iter()
                    .find(|wall| wall.id == wall_id)
                    .is_some_and(|wall| {
                        let declared = bay
                            .infill_solids
                            .iter()
                            .copied()
                            .collect::<std::collections::HashSet<_>>();
                        let authoritative = wall
                            .host_solids
                            .iter()
                            .copied()
                            .filter(|id| {
                                solids
                                    .get(id)
                                    .is_some_and(|solid| solid.role == SolidRole::WallHost)
                            })
                            .collect::<std::collections::HashSet<_>>();
                        let panels = bay
                            .infill_solids
                            .iter()
                            .filter_map(|id| solids.get(id).copied())
                            .filter(|solid| solid.role == SolidRole::WallHost)
                            .collect::<Vec<_>>();
                        let half_length = wall.length_metres * 0.5;
                        let mut expected = MultiPolygon(vec![timber_audit_polygon([
                            Vec2::new(-half_length, 0.0),
                            Vec2::new(half_length, 0.0),
                            Vec2::new(half_length, wall.height_metres),
                            Vec2::new(-half_length, wall.height_metres),
                        ])]);
                        for opening in plan
                            .opening_assemblies
                            .iter()
                            .filter(|opening| opening.host_wall == wall.id)
                        {
                            let half_opening = (opening.profile.interior_width_metres() * 0.5)
                                .min(half_length - 0.02);
                            let centre =
                                (opening.frame.origin - wall.frame.origin).dot(wall.frame.tangent);
                            let sill = (opening.sill_elevation_metres - wall.base_elevation_metres)
                                .clamp(0.0, wall.height_metres);
                            let head = (sill + opening.profile.clear_height_metres())
                                .clamp(sill, wall.height_metres);
                            expected = expected.difference(&timber_audit_polygon([
                                Vec2::new(centre - half_opening, sill),
                                Vec2::new(centre + half_opening, sill),
                                Vec2::new(centre + half_opening, head),
                                Vec2::new(centre - half_opening, head),
                            ]));
                        }
                        let wall_member_ids = frame
                            .bays
                            .iter()
                            .filter(|candidate| candidate.wall == Some(wall.id))
                            .flat_map(|candidate| candidate.member_ids.iter().copied())
                            .collect::<std::collections::HashSet<_>>();
                        for member in frame
                            .members
                            .iter()
                            .filter(|member| wall_member_ids.contains(&member.id))
                        {
                            expected =
                                expected.difference(&timber_member_audit_polygon(member, wall));
                        }

                        let panel_polygons = panels
                            .iter()
                            .filter_map(|panel| timber_panel_audit_polygon(panel, wall))
                            .collect::<Vec<_>>();
                        let panel_union = panel_polygons.iter().cloned().fold(
                            MultiPolygon(Vec::new()),
                            |union, panel| {
                                if union.0.is_empty() {
                                    MultiPolygon(vec![panel])
                                } else {
                                    union.union(&panel)
                                }
                            },
                        );
                        let expected_area = expected.unsigned_area();
                        let panel_area_sum = panel_polygons
                            .iter()
                            .map(Polygon::unsigned_area)
                            .sum::<f32>();
                        let union_area = panel_union.unsigned_area();
                        let missing_area = expected.difference(&panel_union).unsigned_area();
                        let excess_area = panel_union.difference(&expected).unsigned_area();
                        let duplicate_area = (panel_area_sum - union_area).max(0.0);
                        declared == authoritative
                            && panels.len() == panel_polygons.len()
                            && !panels.is_empty()
                            && expected_area > 0.02
                            && missing_area <= 0.0005
                            && excess_area <= 0.0005
                            && duplicate_area <= 0.0005
                    })
            });
            if posts < 2 || rails < 2 || forbidden || !exact_residual {
                issues.push(issue(
                    "invalid_timber_opening_bay",
                    format!(
                        "timber bay {} wall {:?}/{:?}/{:?} does not frame its opening-first clear space (posts {posts}, rails {rails}, collision {forbidden}, residual {exact_residual})",
                        bay.id.0,
                        bay.wall,
                        bay.wall.and_then(|id| plan.wall_assemblies.iter().find(|wall| wall.id == id).map(|wall| wall.source)),
                        bay.wall.and_then(|id| plan.wall_assemblies.iter().find(|wall| wall.id == id).map(|wall| wall.material)),
                    ),
                ));
            }
        }
    }

    let lines = frame
        .facades
        .iter()
        .flat_map(|facade| &facade.lines)
        .chain(frame.internal_lines.iter())
        .collect::<Vec<_>>();
    for line in &lines {
        if line.tangent.length_squared() < 0.99
            || line.outward.length_squared() < 0.99
            || line.tangent.dot(line.outward).abs() > 0.001
            || line.length_metres < 1.4
            || line.storeys.is_empty()
        {
            issues.push(issue(
                "invalid_timber_frame_line",
                format!(
                    "timber frame line {} has invalid local-frame/storey authority",
                    line.id.0
                ),
            ));
        }
        for storey in &line.storeys {
            let listed = storey.member_ids.iter().all(|id| members.contains_key(id));
            let braces = storey
                .member_ids
                .iter()
                .filter(|id| {
                    members.get(id).is_some_and(|member| {
                        matches!(
                            member.role,
                            crate::TimberMemberRole::HeadBrace
                                | crate::TimberMemberRole::FootBrace
                                | crate::TimberMemberRole::StoreyBrace
                        )
                    })
                })
                .count();
            // A brace is structural only when it closes a real, non-collinear
            // three-member frame. Merely ending near another timber (the old
            // test) provided no racking path at all.
            let storey_members = storey
                .member_ids
                .iter()
                .filter_map(|id| members.get(id).copied())
                .collect::<Vec<_>>();
            let closes_triangle = |brace: &crate::TimberFrameMember| {
                let endpoints = [brace.start_node, brace.end_node];
                storey_members.iter().any(|first| {
                    if first.id == brace.id {
                        return false;
                    }
                    let third = if first.start_node == endpoints[0] {
                        Some(first.end_node)
                    } else if first.end_node == endpoints[0] {
                        Some(first.start_node)
                    } else {
                        None
                    };
                    third.is_some_and(|third| {
                        third != endpoints[1]
                            && storey_members.iter().any(|second| {
                                second.id != brace.id
                                    && second.id != first.id
                                    && ((second.start_node == third
                                        && second.end_node == endpoints[1])
                                        || (second.end_node == third
                                            && second.start_node == endpoints[1]))
                            })
                            && nodes
                                .get(&endpoints[0])
                                .zip(nodes.get(&endpoints[1]))
                                .zip(nodes.get(&third))
                                .is_some_and(|((a, b), c)| {
                                    (b.position - a.position)
                                        .cross(c.position - a.position)
                                        .length()
                                        > 0.08
                                })
                    })
                })
            };
            let valid_braces = storey_members
                .iter()
                .filter(|member| {
                    matches!(
                        member.role,
                        crate::TimberMemberRole::HeadBrace
                            | crate::TimberMemberRole::FootBrace
                            | crate::TimberMemberRole::StoreyBrace
                    ) && closes_triangle(member)
                })
                .collect::<Vec<_>>();
            let brace_cycles_valid = if line.internal || line.length_metres < 4.5 {
                !valid_braces.is_empty()
            } else {
                let regions = valid_braces
                    .iter()
                    .map(|brace| {
                        let midpoint = (brace.start + brace.end) * 0.5;
                        let along = (Vec2::new(midpoint.x, midpoint.z) - line.origin)
                            .dot(line.tangent)
                            / line.length_metres
                            + 0.5;
                        if (0.30..=0.70).contains(&along) {
                            1_u8
                        } else {
                            0_u8
                        }
                    })
                    .collect::<std::collections::HashSet<_>>();
                regions.contains(&0) && regions.contains(&1)
            };
            if !listed
                || storey.top_elevation_metres <= storey.base_elevation_metres + 1.9
                || (!line.internal
                    && storey.kind != crate::TimberStoreyKind::MasonryPlinth
                    && braces == 0)
                || !brace_cycles_valid
            {
                issues.push(issue(
                    "unbraced_timber_storey",
                    format!(
                        "timber storey {} is unregistered, unsupported, or unbraced",
                        storey.id.0
                    ),
                ));
            }
            if let Some(jetty) = &storey.jetty {
                let valid_ids = jetty.jetty_beams.iter().all(|id| {
                    members
                        .get(id)
                        .is_some_and(|member| member.role == crate::TimberMemberRole::JettyBeam)
                }) && jetty.knaggen.iter().all(|id| {
                    members
                        .get(id)
                        .is_some_and(|member| member.role == crate::TimberMemberRole::Knagge)
                });
                let floor = solids.get(&jetty.floor_solid);
                let polygon_area = jetty
                    .support_polygon
                    .iter()
                    .zip(jetty.support_polygon.iter().cycle().skip(1))
                    .take(jetty.support_polygon.len())
                    .map(|(left, right)| left.perp_dot(*right))
                    .sum::<f32>()
                    .abs()
                    * 0.5;
                let floor_bearings_valid = !jetty.floor_bearing_interfaces.is_empty()
                    && jetty.floor_bearing_interfaces.iter().all(|id| {
                        interfaces.get(id).is_some_and(|interface| {
                            floor.is_some_and(|floor| {
                                resolved_solid_overlaps_bounds(
                                    floor,
                                    (interface.bounds.min, interface.bounds.max),
                                    0.001,
                                )
                            }) && jetty.jetty_beams.iter().any(|beam| {
                                members
                                    .get(beam)
                                    .and_then(|member| solids.get(&member.solid))
                                    .is_some_and(|solid| {
                                        resolved_solid_overlaps_bounds(
                                            solid,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.001,
                                        )
                                    })
                            })
                        })
                    });
                let beam_geometry_valid = jetty.jetty_beams.iter().all(|id| {
                    members.get(id).is_some_and(|beam| {
                        let delta = beam.end - beam.start;
                        let plan_delta = Vec2::new(delta.x, delta.z);
                        let inner_interface = interfaces.get(&beam.support_interfaces[0]);
                        let outer_interface = interfaces.get(&beam.support_interfaces[1]);
                        delta.y.abs() <= 0.02
                            && plan_delta.normalize_or_zero().dot(line.outward) >= 0.98
                            && plan_delta.length()
                                >= jetty.projection_metres + jetty.backspan_metres - 0.03
                            && inner_interface.is_some_and(|interface| {
                                frame.members.iter().any(|girder| {
                                    girder.role == crate::TimberMemberRole::Girder
                                        && solids.get(&girder.solid).is_some_and(|solid| {
                                            resolved_solid_overlaps_bounds(
                                                solid,
                                                (interface.bounds.min, interface.bounds.max),
                                                0.001,
                                            )
                                        })
                                })
                            })
                            && outer_interface.is_some_and(|interface| {
                                frame.members.iter().any(|sill| {
                                    sill.role == crate::TimberMemberRole::Sill
                                        && solids.get(&sill.solid).is_some_and(|solid| {
                                            resolved_solid_overlaps_bounds(
                                                solid,
                                                (interface.bounds.min, interface.bounds.max),
                                                0.001,
                                            )
                                        })
                                })
                            })
                    })
                });
                let knaggen_geometry_valid = jetty.knaggen.iter().all(|id| {
                    members.get(id).is_some_and(|knagge| {
                        let lower = interfaces.get(&knagge.support_interfaces[0]);
                        let upper = interfaces.get(&knagge.support_interfaces[1]);
                        lower.is_some_and(|interface| {
                            frame.members.iter().any(|post| {
                                matches!(
                                    post.role,
                                    crate::TimberMemberRole::PrimaryPost
                                        | crate::TimberMemberRole::CornerPost
                                ) && solids.get(&post.solid).is_some_and(|solid| {
                                    resolved_solid_overlaps_bounds(
                                        solid,
                                        (interface.bounds.min, interface.bounds.max),
                                        0.001,
                                    )
                                })
                            }) || plan.wall_assemblies.iter().any(|wall| {
                                wall.storey_level == 0
                                    && wall.material == crate::WallMaterialClass::CivilianMasonry
                                    && wall.host_solids.iter().any(|id| {
                                        solids.get(id).is_some_and(|solid| {
                                            resolved_solid_overlaps_bounds(
                                                solid,
                                                (interface.bounds.min, interface.bounds.max),
                                                0.001,
                                            )
                                        })
                                    })
                            })
                        }) && upper.is_some_and(|interface| {
                            jetty.jetty_beams.iter().any(|beam| {
                                members
                                    .get(beam)
                                    .and_then(|beam| solids.get(&beam.solid))
                                    .is_some_and(|solid| {
                                        resolved_solid_overlaps_bounds(
                                            solid,
                                            (interface.bounds.min, interface.bounds.max),
                                            0.001,
                                        )
                                    })
                            })
                        })
                    })
                });
                if jetty.projection_metres <= 0.0
                    || jetty.projection_metres > 0.65
                    || jetty.backspan_metres < jetty.projection_metres
                    || jetty.jetty_beams.len() < 2
                    || jetty.knaggen.len() < 2
                    || jetty.corner_supports.len() < 2
                    || jetty.support_polygon.len() != 4
                    || polygon_area < jetty.projection_metres + jetty.backspan_metres
                    || !valid_ids
                    || !beam_geometry_valid
                    || !knaggen_geometry_valid
                    || floor.is_none_or(|floor| {
                        floor.role != SolidRole::FrameFloor
                            || floor.size.y < 0.12
                            || floor.size.x.min(floor.size.z) > jetty.projection_metres + 0.05
                            || floor.size.x * floor.size.z > polygon_area + 0.05
                            || floor.size.x * floor.size.z
                                < line.length_metres * jetty.projection_metres * 0.90
                            || !jetty.jetty_beams.iter().all(|beam| {
                                members.get(beam).is_some_and(|member| {
                                    floor.supported_by.contains(&member.start_node)
                                        && floor.supported_by.contains(&member.end_node)
                                })
                            })
                    })
                    || !floor_bearings_valid
                {
                    issues.push(issue(
                        "unsupported_timber_jetty",
                        format!(
                            "timber storey {} has an unsupported jetty/backspan (beam_geometry {beam_geometry_valid}, knaggen_geometry {knaggen_geometry_valid})",
                            storey.id.0,
                        ),
                    ));
                }
            }
        }
    }

    let role_count = |role| {
        frame
            .members
            .iter()
            .filter(|member| member.role == role)
            .count()
    };
    let jetty_count = lines
        .iter()
        .flat_map(|line| &line.storeys)
        .filter(|storey| storey.jetty.is_some())
        .count();
    let ground_route = frame
        .floors
        .iter()
        .find(|floor| floor.level == 0)
        .and_then(|floor| {
            plan.resolved_geometry
                .surfaces
                .iter()
                .find(|surface| surface.id == floor.route_surface)
        });
    let ground_route_has_door = ground_route.is_some_and(|surface| {
        plan.opening_assemblies.iter().any(|opening| {
            opening.use_kind == crate::OpeningUse::Door
                && plan
                    .resolved_geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
                    .is_some_and(|void| {
                        let expanded = 0.20;
                        void.bounds.max.x >= surface.bounds.min.x - expanded
                            && void.bounds.min.x <= surface.bounds.max.x + expanded
                            && void.bounds.max.z >= surface.bounds.min.z - expanded
                            && void.bounds.min.z <= surface.bounds.max.z + expanded
                    })
        })
    });
    let material_valid = match frame.program {
        crate::TimberFrameProgramKind::NorthernTwoPostHallHouse
        | crate::TimberFrameProgramKind::DirectRoofCottage => {
            frame.material == crate::StructuralTimberMaterial::Oak
        }
        _ => frame.material == crate::StructuralTimberMaterial::Fir,
    };
    let program_valid = match frame.program {
        crate::TimberFrameProgramKind::NarrowUrbanTownHouse => {
            jetty_count >= 1 && ground_route_has_door
        }
        crate::TimberFrameProgramKind::NorthernTwoPostHallHouse => {
            let longitudinal = frame
                .internal_lines
                .iter()
                .filter(|line| {
                    line.storeys
                        .iter()
                        .flat_map(|storey| &storey.member_ids)
                        .any(|id| {
                            members.get(id).is_some_and(|member| {
                                member.role == crate::TimberMemberRole::Purlin
                            })
                        })
                })
                .collect::<Vec<_>>();
            let transverse = frame
                .internal_lines
                .iter()
                .filter(|line| {
                    line.storeys
                        .iter()
                        .flat_map(|storey| &storey.member_ids)
                        .any(|id| {
                            members.get(id).is_some_and(|member| {
                                member.role == crate::TimberMemberRole::TransverseTie
                            })
                        })
                })
                .collect::<Vec<_>>();
            longitudinal.len() == 2
                && transverse.len() >= 2
                && frame.internal_lines.iter().all(|line| line.internal)
                && role_count(crate::TimberMemberRole::TransverseTie) >= 2
                && role_count(crate::TimberMemberRole::Purlin) >= 2
                && longitudinal.iter().all(|line| {
                    line.storeys
                        .iter()
                        .flat_map(|storey| &storey.member_ids)
                        .any(|id| {
                            members.get(id).is_some_and(|member| {
                                member.role == crate::TimberMemberRole::PrimaryPost
                            })
                        })
                })
                && frame
                    .members
                    .iter()
                    .filter(|member| member.role == crate::TimberMemberRole::TransverseTie)
                    .all(|tie| {
                        [tie.support_interfaces[0], tie.support_interfaces[1]]
                            .iter()
                            .all(|id| interfaces.contains_key(id))
                    })
                && ground_route_has_door
                && jetty_count == 0
        }
        crate::TimberFrameProgramKind::DirectRoofCottage => {
            jetty_count == 0
                && frame
                    .facades
                    .iter()
                    .flat_map(|facade| &facade.lines)
                    .all(|line| line.storeys.len() == 1)
                && ground_route_has_door
        }
        crate::TimberFrameProgramKind::JettiedMerchantHouse => {
            jetty_count >= 1 && ground_route_has_door
        }
        crate::TimberFrameProgramKind::CivicMasonryTimberHall => {
            lines
                .iter()
                .flat_map(|line| &line.storeys)
                .any(|storey| storey.kind == crate::TimberStoreyKind::CivicTimberHall)
                && plan.wall_assemblies.iter().any(|wall| {
                    wall.storey_level == 0
                        && wall.material == crate::WallMaterialClass::CivilianMasonry
                })
                && frame.masonry_bearing_interfaces.len() >= 4
                && frame.members.iter().any(|member| {
                    member.role == crate::TimberMemberRole::Girder
                        && Vec2::new(member.end.x - member.start.x, member.end.z - member.start.z)
                            .length()
                            >= plan.dimensions_metres().min_element() * 0.75
                })
                && ground_route_has_door
                && frame.masonry_bearing_interfaces.iter().all(|id| {
                    interfaces.get(id).is_some_and(|interface| {
                        nodes.get(&interface.node).is_some_and(|node| {
                            node.supported_by.iter().any(|support| {
                                nodes
                                    .get(support)
                                    .is_some_and(|support| support.owner != node.owner)
                            })
                        })
                    })
                })
        }
    };
    if !program_valid || !material_valid {
        issues.push(issue(
            "invalid_timber_program",
            "timber-frame assembly does not satisfy its archetype-specific structural program"
                .to_owned(),
        ));
    }
    let roof_bearings_valid = !frame.roof_bearing_interfaces.is_empty()
        && frame.roof_bearing_interfaces.iter().all(|id| {
            interfaces.get(id).is_some_and(|interface| {
                nodes.get(&interface.node).is_some_and(|node| {
                    node.kind == crate::StructuralNodeKind::TimberFrameRoofBearing
                }) && frame.members.iter().any(|member| {
                    solids.get(&member.solid).is_some_and(|solid| {
                        resolved_solid_overlaps_bounds(
                            solid,
                            (interface.bounds.min, interface.bounds.max),
                            0.001,
                        )
                    })
                }) && plan.roof_assemblies.iter().any(|roof| {
                    roof.support_nodes.iter().any(|roof_node_id| {
                        nodes.get(roof_node_id).is_some_and(|roof_node| {
                            roof_node.supported_by.contains(&interface.node)
                                && roof_node.position.x >= interface.bounds.min.x - 0.001
                                && roof_node.position.x <= interface.bounds.max.x + 0.001
                                && roof_node.position.y >= interface.bounds.min.y - 0.001
                                && roof_node.position.y <= interface.bounds.max.y + 0.001
                                && roof_node.position.z >= interface.bounds.min.z - 0.001
                                && roof_node.position.z <= interface.bounds.max.z + 0.001
                        })
                    })
                })
            })
        });
    if !roof_bearings_valid
        || (!plan.roof_dormers.is_empty()
            && (frame.dormer_trimmer_members.len() < plan.roof_dormers.len() * 4
                || frame.dormer_trimmer_members.iter().any(|id| {
                    members
                        .get(id)
                        .is_none_or(|member| member.role != crate::TimberMemberRole::DormerTrimmer)
                })))
    {
        issues.push(issue(
            "severed_timber_roof_bearing",
            "timber frame lacks exact roof seats or dormer trimmers".to_owned(),
        ));
    }
    let roof_envelope_intrusions = timber_roof_envelope_intrusions(plan);
    if !roof_envelope_intrusions.is_empty() {
        issues.push(issue(
            "timber_intrudes_through_roof",
            format!(
                "roof-construction members leave the authoritative roof envelope: {:?}",
                roof_envelope_intrusions
            ),
        ));
    }
    let exposed_child_supports = exposed_roof_child_support_posts(plan);
    if !exposed_child_supports.is_empty() {
        issues.push(issue(
            "exposed_roof_child_support",
            format!(
                "roof children contain freestanding generic support posts outside their declared curb/front/cheek authority: {:?}",
                exposed_child_supports
            ),
        ));
    }
    let oversized_child_flashings = oversized_child_roof_flashings(plan);
    if !oversized_child_flashings.is_empty() {
        issues.push(issue(
            "invalid_child_roof_flashing_profile",
            format!(
                "child-roof flashing rises above the seated civilian seam profile: {:?}",
                oversized_child_flashings
            ),
        ));
    }
    let invalid_child_drainage = invalid_attached_child_drainage(plan);
    if !invalid_child_drainage.is_empty() {
        issues.push(issue(
            "invalid_child_roof_drainage",
            format!(
                "attached civilian roof drains bypass the containing parent weather face: {:?}",
                invalid_child_drainage
            ),
        ));
    }
    let invalid_child_curb = invalid_dormer_trimmer_envelope(plan);
    if !invalid_child_curb.is_empty() {
        issues.push(issue(
            "invalid_dormer_trimmer_envelope",
            format!(
                "dormer trimmers project outside the exact front-to-rear child enclosure: {:?}",
                invalid_child_curb
            ),
        ));
    }
    let oversized_child_gutters = oversized_attached_child_gutters(plan);
    if !oversized_child_gutters.is_empty() {
        issues.push(issue(
            "invalid_child_roof_drainage_profile",
            format!(
                "attached child roof uses a full-building gutter profile: {:?}",
                oversized_child_gutters
            ),
        ));
    }
    let unseated_dormers = unseated_gabled_dormer_roofs(plan);
    if !unseated_dormers.is_empty() {
        issues.push(issue(
            "unseated_dormer_roof",
            format!(
                "gabled dormer retains a free rear verge or misses the parent weather plane: {:?}",
                unseated_dormers
            ),
        ));
    }
    let coplanar_openings = coplanar_timber_opening_faces(plan);
    if !coplanar_openings.is_empty() {
        issues.push(issue(
            "coplanar_timber_opening_face",
            format!(
                "timber-wall opening solids reach the exposed frame plane: {:?}",
                coplanar_openings
            ),
        ));
    }
    let undeclared_intersections = undeclared_timber_intersections(plan);
    if !undeclared_intersections.is_empty() {
        let mut role_counts = std::collections::BTreeMap::<String, usize>::new();
        for (left, right) in &undeclared_intersections {
            let roles = [left, right].map(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
                    .map_or("missing".to_owned(), |solid| format!("{:?}", solid.role))
            });
            *role_counts
                .entry(format!("{} x {}", roles[0], roles[1]))
                .or_default() += 1;
        }
        let mut seen_roles = std::collections::HashSet::new();
        let sample = undeclared_intersections
            .iter()
            .filter(|(left, right)| {
                let key = [left, right]
                    .into_iter()
                    .filter_map(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == *id)
                    })
                    .map(|solid| format!("{:?}", solid.role))
                    .collect::<Vec<_>>()
                    .join(" x ");
                seen_roles.insert(key)
            })
            .take(20)
            .map(|(left, right)| {
                let describe = |id: ResolvedItemId| {
                    plan.resolved_geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == id)
                        .map(|solid| (id, solid.role, solid.owner, solid.centre, solid.size))
                };
                (describe(*left), describe(*right))
            })
            .collect::<Vec<_>>();
        issues.push(issue(
            "undeclared_timber_intersection",
            format!(
                "{} timber pairs overlap without an exact typed joint or bearing interface; roles: {:?}; first pairs (id, role, owner): {:?}",
                undeclared_intersections.len(), role_counts, sample
            ),
        ));
    }
    let dormer_fronts_valid = frame.bays.iter().all(|bay| {
        let Some(wall_id) = bay.wall else {
            return true;
        };
        let Some(wall) = plan.wall_assemblies.iter().find(|wall| wall.id == wall_id) else {
            return false;
        };
        let crate::WallSourceId::RoofChildFront { roof: roof_id } = wall.source else {
            return true;
        };
        let roles = bay
            .member_ids
            .iter()
            .filter_map(|id| members.get(id).map(|member| member.role))
            .collect::<Vec<_>>();
        let is_shed = plan
            .roof_assemblies
            .iter()
            .find(|roof| roof.id == roof_id)
            .is_some_and(|roof| roof.kind == crate::RoofKind::Shed);
        roles
            .iter()
            .filter(|role| **role == crate::TimberMemberRole::IntermediatePost)
            .count()
            >= 2
            && !roles.contains(&crate::TimberMemberRole::PrimaryPost)
            && (is_shed
                || (roles.contains(&crate::TimberMemberRole::GablePost)
                    && roles
                        .iter()
                        .filter(|role| **role == crate::TimberMemberRole::Rafter)
                        .count()
                        >= 2))
    });
    if !dormer_fronts_valid {
        issues.push(issue(
            "invalid_timber_dormer_front",
            "dormer child front lacks jamb-supported gable framing or retains free corner posts"
                .to_owned(),
        ));
    }
    if plan
        .resolved_geometry
        .solids
        .iter()
        .any(|solid| solid.role == SolidRole::FrameMember)
    {
        issues.push(issue(
            "legacy_timber_frame_overlay",
            "accepted civilian fixture still contains legacy viewer/generic framing".to_owned(),
        ));
    }
}
