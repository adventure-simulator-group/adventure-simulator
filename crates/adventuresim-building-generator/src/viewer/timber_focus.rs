fn timber_proof_slug(plan: &BuildingPlan, view: ViewerView) -> Option<String> {
    let suffix = timber_proof_suffix(view)?;
    Some(
        if matches!(
            view,
            ViewerView::TimberWholeExterior
                | ViewerView::TimberFrameFacade
                | ViewerView::TimberRegistrationCut
                | ViewerView::TimberSupportLoad
                | ViewerView::TimberProgramDetail
        ) {
            format!("timber-{}-{suffix}", plan.archetype.slug())
        } else {
            suffix.to_owned()
        },
    )
}

fn timber_target_component_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<String> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let claim = match view {
        ViewerView::TimberWholeExterior => "whole",
        ViewerView::TimberFrameFacade => "south-facade/frame",
        ViewerView::TimberRegistrationCut => "occupied-level/circulation-registration",
        ViewerView::TimberSupportLoad => "south-facade/support-load",
        ViewerView::TimberProgramDetail => match frame.program {
            adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse => {
                "two-post-hall/inner-rows"
            }
            adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage => {
                "direct-roof/gable"
            }
            adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall => {
                "civic-hall/broad-span"
            }
            adventuresim_building_generator::TimberFrameProgramKind::NarrowUrbanTownHouse => {
                "urban-frame/jetty"
            }
            adventuresim_building_generator::TimberFrameProgramKind::JettiedMerchantHouse => {
                "merchant-frame/jetty"
            }
        },
        ViewerView::TimberOpeningBayExterior
        | ViewerView::TimberOpeningBayInterior
        | ViewerView::TimberOpeningBaySection => "opening-bay/reframed-load",
        ViewerView::TimberJointClose => "joint/post-plate",
        ViewerView::TimberJettyExterior
        | ViewerView::TimberJettyUnderside
        | ViewerView::TimberJettyLoad => "jetty/cantilever-bearing",
        ViewerView::TimberGableRoofBearing => "gable/roof-seat",
        ViewerView::TimberDormerTrimmer => "roof-child/trimmer",
        ViewerView::TimberTownHallJunction => "civic-hall/masonry-timber-bearing",
        _ => return Vec::new(),
    };
    vec![format!("timber:{}/{}", frame.id.0, claim)]
}

fn timber_focus_interface_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let focused = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let focused_nodes = frame
        .members
        .iter()
        .filter(|member| focused.contains(&member.solid.0))
        .flat_map(|member| [member.start_node, member.end_node])
        .collect::<std::collections::HashSet<_>>();
    plan.resolved_geometry
        .support_interfaces
        .iter()
        .filter(|interface| interface.owner == frame.members[0].owner)
        .filter(|interface| focused_nodes.contains(&interface.node))
        .map(|interface| interface.id.0)
        .collect()
}

fn timber_section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::TimberRegistrationCut
            | ViewerView::TimberSupportLoad
            | ViewerView::TimberOpeningBaySection
            | ViewerView::TimberGableRoofBearing
            | ViewerView::TimberDormerTrimmer
            | ViewerView::TimberTownHallJunction
    )
}

fn timber_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    use adventuresim_building_generator::TimberMemberRole as Role;
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let mut member_ids = match view {
        ViewerView::TimberRegistrationCut
            if frame.program
                == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse =>
        {
            frame
                .internal_lines
                .iter()
                .flat_map(|line| &line.storeys)
                .flat_map(|storey| &storey.member_ids)
                .copied()
                .collect()
        }
        ViewerView::TimberRegistrationCut
            if frame.program
                == adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage =>
        {
            frame
                .facades
                .iter()
                .find(|facade| facade.outward == Direction::South)
                .into_iter()
                .flat_map(|facade| &facade.lines)
                .flat_map(|line| &line.storeys)
                .flat_map(|storey| &storey.member_ids)
                .copied()
                .collect()
        }
        ViewerView::TimberRegistrationCut => std::collections::HashSet::new(),
        ViewerView::TimberFrameFacade | ViewerView::TimberSupportLoad => frame
            .facades
            .iter()
            .find(|facade| facade.outward == Direction::South)
            .into_iter()
            .flat_map(|facade| &facade.lines)
            .flat_map(|line| &line.storeys)
            .flat_map(|storey| &storey.member_ids)
            .copied()
            .collect::<std::collections::HashSet<_>>(),
        ViewerView::TimberOpeningBayExterior
        | ViewerView::TimberOpeningBayInterior
        | ViewerView::TimberOpeningBaySection => frame
            .bays
            .iter()
            .find(|bay| bay.opening.is_some())
            .into_iter()
            .flat_map(|bay| &bay.member_ids)
            .copied()
            .collect(),
        ViewerView::TimberJointClose => frame
            .joints
            .iter()
            .filter(|joint| {
                let has_role = |role| {
                    joint.member_ids.iter().any(|id| {
                        frame
                            .members
                            .iter()
                            .find(|member| member.id == *id)
                            .is_some_and(|member| member.role == role)
                    })
                };
                has_role(Role::PrimaryPost) && has_role(Role::WallPlate)
            })
            .max_by_key(|joint| joint.member_ids.len())
            .into_iter()
            .flat_map(|joint| &joint.member_ids)
            .copied()
            .collect(),
        ViewerView::TimberJettyExterior
        | ViewerView::TimberJettyUnderside
        | ViewerView::TimberJettyLoad => frame
            .facades
            .iter()
            .flat_map(|facade| &facade.lines)
            .flat_map(|line| &line.storeys)
            .find(|storey| storey.jetty.is_some())
            .into_iter()
            .filter_map(|storey| storey.jetty.as_ref())
            .flat_map(|jetty| {
                jetty
                    .jetty_beams
                    .iter()
                    .chain(&jetty.knaggen)
                    .chain(&jetty.corner_supports)
            })
            .copied()
            .collect(),
        ViewerView::TimberGableRoofBearing => frame
            .members
            .iter()
            .filter(|member| {
                matches!(
                    member.role,
                    Role::GableTie | Role::GablePost | Role::Rafter | Role::Collar | Role::Purlin
                )
            })
            .map(|member| member.id)
            .collect(),
        ViewerView::TimberDormerTrimmer => frame.dormer_trimmer_members.iter().copied().collect(),
        ViewerView::TimberTownHallJunction => {
            // Prove the masonry-to-timber transition where the civic hall's
            // broad internal girder actually meets the storey frame, rather
            // than at an arbitrary facade corner. This keeps the cut on the
            // continuous masonry bearing run and makes both structural
            // systems visible in the same exact-ID detail.
            let hall_girder_centre = frame
                .members
                .iter()
                .filter(|member| member.role == Role::Girder)
                .max_by(|left, right| {
                    left.start
                        .distance(left.end)
                        .total_cmp(&right.start.distance(right.end))
                })
                .map(|member| (member.start + member.end) * 0.5);
            frame
                .members
                .iter()
                .filter(|member| {
                    member.role == Role::Sill
                        && (member.start.y - plan.storey_height_metres).abs() <= 0.02
                })
                .min_by(|left, right| {
                    let distance = |member: &adventuresim_building_generator::TimberFrameMember| {
                        hall_girder_centre.map_or(0.0, |centre| {
                            ((member.start + member.end) * 0.5).distance(centre)
                        })
                    };
                    distance(left).total_cmp(&distance(right))
                })
                .map(|member| member.id)
                .into_iter()
                .collect()
        }
        ViewerView::TimberProgramDetail
            if frame.program
                == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse =>
        {
            frame
                .internal_lines
                .iter()
                .flat_map(|line| &line.storeys)
                .flat_map(|storey| &storey.member_ids)
                .copied()
                .collect()
        }
        _ => frame.members.iter().map(|member| member.id).collect(),
    };
    if matches!(
        view,
        ViewerView::TimberJettyExterior
            | ViewerView::TimberJettyUnderside
            | ViewerView::TimberJettyLoad
    ) {
        let connected = frame
            .joints
            .iter()
            .filter(|joint| joint.member_ids.iter().any(|id| member_ids.contains(id)))
            .flat_map(|joint| joint.member_ids.iter().copied())
            .collect::<Vec<_>>();
        member_ids.extend(connected);
    }
    if view == ViewerView::TimberTownHallJunction {
        let connected = frame
            .joints
            .iter()
            .filter(|joint| joint.member_ids.iter().any(|id| member_ids.contains(id)))
            .flat_map(|joint| joint.member_ids.iter().copied())
            .collect::<Vec<_>>();
        member_ids.extend(connected);
        let target_centre = frame
            .members
            .iter()
            .find(|member| member_ids.contains(&member.id) && member.role == Role::Sill)
            .map(|member| (member.start + member.end) * 0.5);
        if let Some(centre) = target_centre {
            member_ids.extend(
                frame
                    .members
                    .iter()
                    .filter(|member| {
                        member.role == Role::Sill
                            && ((member.start + member.end) * 0.5).distance(centre) <= 5.5
                    })
                    .map(|member| member.id),
            );
        }
        if let Some(girder) = target_centre.and_then(|centre| {
            frame
                .members
                .iter()
                .filter(|member| member.role == Role::Girder)
                .min_by(|left, right| {
                    ((left.start + left.end) * 0.5)
                        .distance(centre)
                        .total_cmp(&((right.start + right.end) * 0.5).distance(centre))
                })
        }) {
            member_ids.insert(girder.id);
        }
    }
    if view == ViewerView::TimberProgramDetail
        && frame.program
            == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
    {
        member_ids.extend(
            frame
                .members
                .iter()
                .filter(|member| {
                    matches!(
                        member.role,
                        Role::TransverseTie | Role::GableTie | Role::GablePost | Role::Rafter
                    ) || (member.role == Role::Purlin && member.start.distance(member.end) >= 3.0)
                })
                .map(|member| member.id),
        );
    }
    if view == ViewerView::TimberGableRoofBearing {
        let ridge_x = plan
            .roofs
            .first()
            .is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
        let coordinate = |point: Vec3| if ridge_x { point.x } else { point.z };
        let end_plane = frame
            .members
            .iter()
            .filter(|member| member_ids.contains(&member.id))
            .flat_map(|member| [coordinate(member.start), coordinate(member.end)])
            .fold(f32::INFINITY, f32::min);
        member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_some_and(|member| {
                    (coordinate(member.start) - end_plane).abs() <= 0.45
                        && (coordinate(member.end) - end_plane).abs() <= 0.45
                })
        });
    }
    if view == ViewerView::TimberDormerTrimmer
        && let Some(dormer) = plan.roof_dormers.first()
    {
        member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_some_and(|member| {
                    let centre = (member.start + member.end) * 0.5;
                    Vec2::new(centre.x, centre.z).distance(dormer.centre)
                        <= dormer.width_metres.max(dormer.depth_metres) * 0.75
                })
        });
        // Keep the trimmer proof structural rather than showing two floating
        // bars: include members which share the exact trimmer end joints.
        let connected = frame
            .joints
            .iter()
            .filter(|joint| joint.member_ids.iter().any(|id| member_ids.contains(id)))
            .flat_map(|joint| joint.member_ids.iter().copied())
            .collect::<Vec<_>>();
        member_ids.extend(connected);

        // Include the authoritative timber curb/front framing belonging to
        // the same Stage 4 child roof. A trimmer-only proof can otherwise
        // look like detached bars even when the parent cut is correctly
        // framed; these exact bay members show what those trimmers carry.
        let child_roof = plan
            .roof_assemblies
            .iter()
            .filter(|roof| roof.parent.is_some())
            .min_by(|left, right| {
                let centre = |roof: &adventuresim_building_generator::RoofAssembly| {
                    let count = roof.outer_loop.vertices.len().max(1) as f32;
                    roof.outer_loop
                        .vertices
                        .iter()
                        .map(|point| point.metres())
                        .sum::<Vec2>()
                        / count
                };
                centre(left)
                    .distance(dormer.centre)
                    .total_cmp(&centre(right).distance(dormer.centre))
            })
            .map(|roof| roof.id);
        if let Some(child_roof) = child_roof {
            member_ids.extend(
                frame
                    .bays
                    .iter()
                    .filter(|bay| {
                        bay.wall
                            .and_then(|wall_id| {
                                plan.wall_assemblies.iter().find(|wall| wall.id == wall_id)
                            })
                            .is_some_and(|wall| {
                                matches!(
                                    wall.source,
                                    adventuresim_building_generator::WallSourceId::RoofChildFront {
                                        roof
                                    } if roof == child_roof
                                )
                            })
                    })
                    .flat_map(|bay| bay.member_ids.iter().copied()),
            );
        }
    }
    if matches!(
        view,
        ViewerView::TimberFrameFacade | ViewerView::TimberSupportLoad
    ) && let Some(line) = frame
        .facades
        .iter()
        .find(|facade| facade.outward == Direction::South)
        .and_then(|facade| facade.lines.first())
    {
        member_ids.retain(|id| {
            frame
                .members
                .iter()
                .find(|member| member.id == *id)
                .is_some_and(|member| {
                    let centre = (member.start + member.end) * 0.5;
                    (Vec2::new(centre.x, centre.z) - line.origin)
                        .dot(line.tangent)
                        .abs()
                        <= 5.5
                })
        });
    }
    let mut resolved = frame
        .members
        .iter()
        .filter(|member| member_ids.contains(&member.id))
        .map(|member| member.solid.0)
        .collect::<Vec<_>>();
    if matches!(
        view,
        ViewerView::TimberRegistrationCut | ViewerView::TimberSupportLoad
    ) {
        let stair_centre = frame
            .circulation
            .stair_solids
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
            })
            .map(|solid| solid.centre)
            .sum::<Vec3>()
            / frame.circulation.stair_solids.len().max(1) as f32;
        let near_stair = |solid: &adventuresim_building_generator::ResolvedSolid,
                          clearance: f32| {
            let delta = (solid.centre - stair_centre).abs() - solid.size * 0.5;
            Vec2::new(delta.x.max(0.0), delta.z.max(0.0)).length() <= clearance
        };
        if view == ViewerView::TimberSupportLoad {
            resolved.extend(
                frame
                    .floors
                    .iter()
                    .flat_map(|floor| {
                        let pieces = floor
                            .floor_solids
                            .iter()
                            .filter_map(|id| {
                                plan.resolved_geometry
                                    .solids
                                    .iter()
                                    .find(|solid| solid.id == *id)
                            })
                            .filter(|solid| {
                                view == ViewerView::TimberSupportLoad || near_stair(solid, 0.75)
                            });
                        pieces.collect::<Vec<_>>()
                    })
                    .map(|solid| solid.id.0),
            );
        }
        resolved.extend(
            frame
                .floors
                .iter()
                .flat_map(|floor| floor.joist_members.iter().chain(&floor.girder_members))
                .filter_map(|id| frame.members.iter().find(|member| member.id == *id))
                .filter(|member| {
                    view == ViewerView::TimberSupportLoad
                        || plan
                            .resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == member.solid)
                            .is_some_and(|solid| near_stair(solid, 2.25))
                })
                .map(|member| member.solid.0),
        );
        if view == ViewerView::TimberRegistrationCut {
            // The route is only meaningful against the authoritative occupied
            // floor it reaches. Include those exact floor solids in the cut,
            // rather than proving circulation as floating tread surfaces.
            resolved.extend(frame.floors.iter().map(|floor| floor.floor_solid.0));
            resolved.extend(
                frame
                    .circulation
                    .nodes
                    .iter()
                    .filter(|node| {
                        if matches!(
                            frame.program,
                            adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
                                | adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
                        ) {
                            // These one-storey programs have no stair flight.
                            // Their registration proof instead follows the
                            // complete exterior-door-to-occupied-floor route.
                            true
                        } else {
                            node.kind
                                == adventuresim_building_generator::TimberRouteNodeKind::StairTread
                                || (node.kind
                                    == adventuresim_building_generator::TimberRouteNodeKind::Landing
                                    && Vec2::new(node.position.x, node.position.z)
                                        .distance(Vec2::new(stair_centre.x, stair_centre.z))
                                        <= 2.0)
                        }
                    })
                    .map(|node| node.surface.0),
            );
            resolved.extend(frame.circulation.stair_solids.iter().map(|id| id.0));
            resolved.extend(frame.circulation.landing_solids.iter().map(|id| id.0));
        }
    }
    if view == ViewerView::TimberProgramDetail {
        resolved.extend(frame.floors.iter().map(|floor| floor.floor_solid.0));
        if frame.program
            == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
        {
            resolved.extend(frame.floors.iter().map(|floor| floor.route_surface.0));
        }
    }
    if view == ViewerView::TimberGableRoofBearing {
        let bearing_interfaces = frame
            .roof_bearing_interfaces
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .support_interfaces
                    .iter()
                    .find(|interface| interface.id == *id)
            })
            .collect::<Vec<_>>();
        resolved.extend(
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    matches!(solid.role, SolidRole::RoofPlate | SolidRole::RoofFraming)
                        && bearing_interfaces.iter().any(|interface| {
                            let half = solid.size * 0.5;
                            let min = solid.centre - half;
                            let max = solid.centre + half;
                            min.x <= interface.bounds.max.x + 0.02
                                && max.x >= interface.bounds.min.x - 0.02
                                && min.y <= interface.bounds.max.y + 0.02
                                && max.y >= interface.bounds.min.y - 0.02
                                && min.z <= interface.bounds.max.z + 0.02
                                && max.z >= interface.bounds.min.z - 0.02
                        })
                })
                .map(|solid| solid.id.0),
        );
    }
    if view == ViewerView::TimberDormerTrimmer {
        let trimmer_centres = frame
            .members
            .iter()
            .filter(|member| member_ids.contains(&member.id))
            .map(|member| (member.start + member.end) * 0.5)
            .collect::<Vec<_>>();
        // Only include the parent rafters physically adjacent to this dormer
        // curb. Pulling every roof-bearing solid into the proof produced
        // detached posts from the opposite roof slope and obscured the exact
        // trimmer-to-rafter contact.
        resolved.extend(
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| solid.role == SolidRole::RoofFraming)
                .filter(|solid| {
                    trimmer_centres
                        .iter()
                        .any(|centre| centre.distance(solid.centre) <= 2.5)
                })
                .map(|solid| solid.id.0),
        );
    }
    if matches!(
        view,
        ViewerView::TimberJettyExterior
            | ViewerView::TimberJettyUnderside
            | ViewerView::TimberJettyLoad
    ) {
        resolved.extend(
            frame
                .facades
                .iter()
                .flat_map(|facade| &facade.lines)
                .flat_map(|line| &line.storeys)
                .filter_map(|storey| storey.jetty.as_ref())
                .filter(|jetty| jetty.jetty_beams.iter().any(|id| member_ids.contains(id)))
                .map(|jetty| jetty.floor_solid.0),
        );
    }
    if matches!(
        view,
        ViewerView::TimberOpeningBayExterior
            | ViewerView::TimberOpeningBayInterior
            | ViewerView::TimberOpeningBaySection
    ) && let Some(bay) = frame.bays.iter().find(|bay| bay.opening.is_some())
    {
        // The exact triangular Gefach partition is part of the proof: unlike
        // the old backing sheet, these cells terminate on posts/rails/braces
        // and leave the opening void clear. Their shallower wall depth keeps
        // the exterior frame readable; interior/section views render them as
        // cut material so both contact boundaries and timbers remain visible.
        resolved.extend(bay.infill_solids.iter().map(|id| id.0));
        if let Some(opening) = bay.opening.and_then(|id| {
            plan.opening_assemblies
                .iter()
                .find(|opening| opening.id == id)
        }) {
            resolved.extend(opening.closure_solids.iter().map(|id| id.0));
        }
    }
    if view == ViewerView::TimberTownHallJunction {
        let sill_centre = frame
            .members
            .iter()
            .find(|member| member_ids.contains(&member.id))
            .map(|member| (member.start + member.end) * 0.5);
        if let Some(wall) = sill_centre.and_then(|centre| {
            plan.wall_assemblies
                .iter()
                .filter(|wall| wall.storey_level == 0)
                .min_by(|left, right| {
                    left.frame
                        .origin
                        .distance(Vec2::new(centre.x, centre.z))
                        .total_cmp(&right.frame.origin.distance(Vec2::new(centre.x, centre.z)))
                })
        }) {
            let centre = sill_centre.expect("sill centre was present when wall was selected");
            let centre_2d = Vec2::new(centre.x, centre.z);
            resolved.extend(
                plan.wall_assemblies
                    .iter()
                    .filter(|candidate| {
                        candidate.storey_level == 0
                            && candidate.frame.outward.dot(wall.frame.outward) >= 0.99
                            && ((candidate.frame.origin - centre_2d).dot(wall.frame.outward)).abs()
                                <= 0.45
                            && ((candidate.frame.origin - centre_2d).dot(wall.frame.tangent)).abs()
                                <= 1.0
                    })
                    .flat_map(|candidate| candidate.host_solids.iter().map(|id| id.0)),
            );
        }
    }
    resolved.sort_unstable();
    resolved.dedup();
    resolved
}

fn timber_isolated_view(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::TimberFrameFacade
            | ViewerView::TimberRegistrationCut
            | ViewerView::TimberSupportLoad
            | ViewerView::TimberProgramDetail
            | ViewerView::TimberOpeningBayExterior
            | ViewerView::TimberOpeningBayInterior
            | ViewerView::TimberOpeningBaySection
            | ViewerView::TimberJointClose
            | ViewerView::TimberJettyExterior
            | ViewerView::TimberJettyUnderside
            | ViewerView::TimberJettyLoad
            | ViewerView::TimberGableRoofBearing
            | ViewerView::TimberDormerTrimmer
            | ViewerView::TimberTownHallJunction
    )
}

fn timber_camera(plan: &BuildingPlan, view: ViewerView, origin: Vec2) -> Option<(Vec3, Vec3)> {
    timber_proof_suffix(view)?;
    let ids = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let removed = timber_section_removed_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let focused = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| ids.contains(&solid.id.0) && !removed.contains(&solid.id.0))
        .collect::<Vec<_>>();
    let camera_focused = &focused;
    let focus = if camera_focused.is_empty() {
        let dimensions = plan.dimensions_metres();
        Vec3::new(
            dimensions.x * 0.5,
            plan.storey_height_metres,
            dimensions.y * 0.5,
        )
    } else {
        camera_focused
            .iter()
            .map(|solid| solid.centre)
            .sum::<Vec3>()
            / camera_focused.len() as f32
    } + Vec3::new(origin.x, 0.0, origin.y);
    let span = camera_focused
        .iter()
        .map(|solid| solid.size.length())
        .fold(4.0_f32, f32::max)
        .clamp(4.0, 20.0);
    let focus_extent = if camera_focused.is_empty() {
        span
    } else {
        let min = camera_focused
            .iter()
            .map(|solid| solid.centre - solid.size * 0.5)
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let max = camera_focused
            .iter()
            .map(|solid| solid.centre + solid.size * 0.5)
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        (max - min).max_element().max(4.0)
    };
    let opening_frame = plan
        .timber_frame
        .as_ref()
        .and_then(|frame| frame.bays.iter().find_map(|bay| bay.opening))
        .and_then(|id| {
            plan.opening_assemblies
                .iter()
                .find(|opening| opening.id == id)
        })
        .map(|opening| opening.frame);
    let offset = match view {
        ViewerView::TimberWholeExterior => Vec3::new(-span * 2.25, span * 1.05, -span * 2.25),
        ViewerView::TimberFrameFacade
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall
            }) => Vec3::new(
            focus_extent * 0.10,
            focus_extent * 0.25,
            -focus_extent * 1.25,
        ),
        ViewerView::TimberFrameFacade => Vec3::new(
            focus_extent * 0.12,
            focus_extent * 0.32,
            -focus_extent * 1.65,
        ),
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall
            }) =>
        {
            Vec3::new(focus_extent * 1.35, focus_extent * 0.72, -focus_extent * 1.35)
        }
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) =>
        {
            // Include the exterior threshold and the full central-hall route,
            // not only the internal post-and-tie frame used to derive the
            // solid focus extent.
            Vec3::new(focus_extent * 1.55, focus_extent * 0.72, -focus_extent * 1.55)
        }
        ViewerView::TimberRegistrationCut if plan.storeys.len() <= 2 => {
            Vec3::new(focus_extent * 1.05, focus_extent * 0.62, -focus_extent * 1.05)
        }
        ViewerView::TimberRegistrationCut => {
            Vec3::new(focus_extent * 1.30, focus_extent * 0.72, -focus_extent * 1.30)
        }
        ViewerView::TimberSupportLoad
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) => Vec3::new(span * 1.10, span * 0.34, -span * 1.10),
        ViewerView::TimberSupportLoad
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
            }) => Vec3::new(span * 1.20, span * 0.38, -span * 1.20),
        ViewerView::TimberSupportLoad => Vec3::new(span * 1.65, span * 0.72, -span * 1.70),
        ViewerView::TimberProgramDetail
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) => Vec3::new(-span * 1.30, span * 0.55, -span * 1.40),
        ViewerView::TimberProgramDetail => Vec3::new(-span * 1.80, span * 0.75, -span * 1.90),
        ViewerView::TimberOpeningBayExterior if let Some(frame) = opening_frame => {
            Vec3::new(frame.outward.x, 0.32, frame.outward.y) * focus_extent * 1.45
                + Vec3::new(frame.tangent.x, 0.0, frame.tangent.y) * focus_extent * 0.16
        }
        ViewerView::TimberOpeningBayInterior if let Some(frame) = opening_frame => {
            Vec3::new(-frame.outward.x, 0.28, -frame.outward.y) * focus_extent * 1.35
                - Vec3::new(frame.tangent.x, 0.0, frame.tangent.y) * focus_extent * 0.14
        }
        ViewerView::TimberOpeningBaySection if let Some(frame) = opening_frame => {
            Vec3::new(frame.tangent.x, 0.34, frame.tangent.y) * focus_extent * 0.9
                - Vec3::new(frame.outward.x, 0.0, frame.outward.y) * focus_extent
        }
        ViewerView::TimberOpeningBayExterior => Vec3::new(-4.5, 2.2, -6.5),
        ViewerView::TimberOpeningBayInterior => Vec3::new(4.5, 2.0, 5.5),
        ViewerView::TimberOpeningBaySection => Vec3::new(5.5, 2.5, -4.5),
        ViewerView::TimberJointClose => Vec3::new(-3.5, 2.0, -3.5),
        ViewerView::TimberJettyExterior => Vec3::new(
            -focus_extent * 1.5,
            focus_extent * 0.7,
            -focus_extent * 1.5,
        ),
        ViewerView::TimberJettyUnderside => Vec3::new(
            -focus_extent * 0.82,
            -focus_extent * 0.14,
            -focus_extent * 0.82,
        ),
        ViewerView::TimberJettyLoad => Vec3::new(
            focus_extent * 1.5,
            focus_extent * 0.7,
            -focus_extent * 1.5,
        ),
        ViewerView::TimberGableRoofBearing => {
            let ridge_x = plan
                .roofs
                .first()
                .is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
            let ridge = if ridge_x { Vec3::X } else { Vec3::Z };
            let side = if ridge_x { Vec3::Z } else { Vec3::X };
            -ridge * focus_extent * 1.35
                + side * focus_extent * 0.32
                + Vec3::Y * focus_extent * 0.42
        }
        ViewerView::TimberDormerTrimmer => Vec3::new(
            -focus_extent * 1.5,
            focus_extent * 0.65,
            -focus_extent * 1.5,
        ),
        ViewerView::TimberTownHallJunction => Vec3::new(
            -focus_extent * 0.78,
            focus_extent * 0.46,
            -focus_extent * 0.74,
        ),
        _ => return None,
    };
    Some((focus + offset, focus))
}

fn timber_required_roles(plan: &BuildingPlan, view: ViewerView) -> Vec<String> {
    let roles: &[&str] = match view {
        ViewerView::TimberWholeExterior => &["FramePost", "FramePlate", "FrameBrace"],
        ViewerView::TimberFrameFacade => &["FramePost", "FrameRail", "FrameBrace"],
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
            }) =>
        {
            &["FrameFloor", "FramePost", "FrameTie", "TimberCirculation"]
        }
        ViewerView::TimberRegistrationCut
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.program
                    == adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
            }) =>
        {
            &["FrameFloor", "FramePost", "FrameBrace", "TimberCirculation"]
        }
        ViewerView::TimberRegistrationCut => &[
            "FrameFloor",
            "FrameJoist",
            "FrameGirder",
            "TimberCirculation",
        ],
        ViewerView::TimberOpeningBayExterior
        | ViewerView::TimberOpeningBayInterior
        | ViewerView::TimberOpeningBaySection => &["FramePost", "FrameRail", "WallHost"],
        ViewerView::TimberJointClose => &["FramePost", "FramePlate"],
        ViewerView::TimberJettyExterior
        | ViewerView::TimberJettyUnderside
        | ViewerView::TimberJettyLoad => &["FrameJettyBeam", "FrameKnagge"],
        ViewerView::TimberGableRoofBearing => &["FrameGableMember"],
        ViewerView::TimberDormerTrimmer => &["FrameDormerTrimmer", "RoofFraming"],
        ViewerView::TimberTownHallJunction => &["FrameSill", "FrameGirder", "WallHost"],
        ViewerView::TimberSupportLoad
            if plan.timber_frame.as_ref().is_some_and(|frame| {
                matches!(
                    frame.program,
                    adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse
                        | adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage
                )
            }) =>
        {
            &["FramePost", "FrameBrace", "FramePlate", "FrameFloor"]
        }
        ViewerView::TimberSupportLoad => &[
            "FramePost",
            "FrameBrace",
            "FramePlate",
            "FrameJoist",
            "FrameGirder",
        ],
        ViewerView::TimberProgramDetail => match plan
            .timber_frame
            .as_ref()
            .map(|frame| frame.program)
        {
            Some(
                adventuresim_building_generator::TimberFrameProgramKind::NorthernTwoPostHallHouse,
            ) => &[
                "FramePost",
                "FrameTie",
                "FrameGableMember",
                "FrameFloor",
                "TimberCirculation",
            ],
            Some(adventuresim_building_generator::TimberFrameProgramKind::DirectRoofCottage) => {
                &["FramePost", "FrameBrace", "FrameGableMember"]
            }
            Some(
                adventuresim_building_generator::TimberFrameProgramKind::CivicMasonryTimberHall,
            ) => &["FrameSill", "FrameGirder", "FramePost"],
            Some(
                adventuresim_building_generator::TimberFrameProgramKind::NarrowUrbanTownHouse
                | adventuresim_building_generator::TimberFrameProgramKind::JettiedMerchantHouse,
            ) => &["FrameJettyBeam", "FrameKnagge", "FrameFloor"],
            None => &[],
        },
        _ => &[],
    };
    roles.iter().map(|role| (*role).to_owned()).collect()
}

fn timber_cut_plane(plan: &BuildingPlan, view: ViewerView) -> Option<[f32; 4]> {
    timber_section_proof(view).then(|| {
        let dimensions = plan.dimensions_metres();
        if view == ViewerView::TimberOpeningBaySection
            && let Some((opening, bounds)) = plan
                .timber_frame
                .as_ref()
                .and_then(|frame| frame.bays.iter().find_map(|bay| bay.opening))
                .and_then(|id| {
                    let opening = plan
                        .opening_assemblies
                        .iter()
                        .find(|opening| opening.id == id)?;
                    let bounds = plan
                        .resolved_geometry
                        .voids
                        .iter()
                        .find(|void| void.id == opening.void_id)?
                        .bounds;
                    Some((opening, bounds))
                })
        {
            let centre = (bounds.min + bounds.max) * 0.5;
            let normal = opening.frame.tangent;
            [
                normal.x,
                0.0,
                normal.y,
                -normal.dot(Vec2::new(centre.x, centre.z)),
            ]
        } else if view == ViewerView::TimberTownHallJunction {
            // Retain the centre and one end bearing of the authoritative
            // broad-span girder. A cut through x=7 removed the girder as one
            // resolved solid even though its masonry/sill counterparts
            // remained, producing a misleading one-sided junction proof.
            [1.0, 0.0, 0.0, -12.0]
        } else if view == ViewerView::TimberGableRoofBearing {
            let roof = plan.roofs.first();
            let ridge_x = roof.is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
            let end_plane = plan
                .timber_frame
                .as_ref()
                .into_iter()
                .flat_map(|frame| &frame.members)
                .filter(|member| {
                    member.role == adventuresim_building_generator::TimberMemberRole::GableTie
                })
                .flat_map(|member| [member.start, member.end])
                .map(|point| if ridge_x { point.x } else { point.z })
                .fold(f32::INFINITY, f32::min);
            let cut = if end_plane.is_finite() {
                end_plane + 0.45
            } else if ridge_x {
                dimensions.x * 0.5
            } else {
                dimensions.y * 0.5
            };
            if ridge_x {
                [1.0, 0.0, 0.0, -cut]
            } else {
                [0.0, 0.0, 1.0, -cut]
            }
        } else if view == ViewerView::TimberDormerTrimmer {
            [
                1.0,
                0.0,
                0.0,
                -plan
                    .roof_dormers
                    .first()
                    .map_or(dimensions.x * 0.5, |dormer| dormer.centre.x),
            ]
        } else {
            [0.0, 0.0, 1.0, -dimensions.y * 0.5]
        }
    })
}

fn timber_section_removed_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(plane) = timber_cut_plane(plan, view) else {
        return Vec::new();
    };
    let focused = timber_focus_item_ids(plan, view)
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| focused.contains(&solid.id.0))
        .filter(|solid| {
            plane[0] * solid.centre.x
                + plane[1] * solid.centre.y
                + plane[2] * solid.centre.z
                + plane[3]
                > 0.05
        })
        .map(|solid| solid.id.0)
        .collect()
}
