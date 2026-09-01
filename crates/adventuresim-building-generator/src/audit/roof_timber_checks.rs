fn timber_roof_envelope_intrusions(plan: &BuildingPlan) -> Vec<crate::TimberMemberId> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    let roof_faces = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| &roof.faces)
        .collect::<Vec<_>>();
    let maximum_covering_thickness = roof_faces
        .iter()
        .map(|face| face.thickness_metres)
        .fold(0.0_f32, f32::max);

    frame
        .members
        .iter()
        .filter(|member| {
            member.phase == crate::TimberFramePhase::RoofConstruction
                && matches!(
                    member.role,
                    crate::TimberMemberRole::GableTie
                        | crate::TimberMemberRole::GablePost
                        | crate::TimberMemberRole::Rafter
                        | crate::TimberMemberRole::Collar
                        | crate::TimberMemberRole::Purlin
                )
        })
        .filter_map(|member| {
            let clearance =
                member.section_metres.max_element() * 0.75 + maximum_covering_thickness + 0.02;
            let intrudes = (0..=32).any(|sample| {
                let t = sample as f32 / 32.0;
                let point = member.start.lerp(member.end, t);
                let plan_point = Vec2::new(point.x, point.z);
                let roof_height = roof_faces
                    .iter()
                    .filter(|face| roof_face_contains_plan_point_inclusive(face, plan_point))
                    .filter_map(|face| roof_face_height(face, plan_point))
                    .fold(None, |highest: Option<f32>, height| {
                        Some(highest.map_or(height, |current| current.max(height)))
                    });
                roof_height.is_none_or(|height| point.y > height + clearance)
            });
            intrudes.then_some(member.id)
        })
        .collect()
}

fn exposed_roof_child_support_posts(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_roofs = plan
        .roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_some())
        .map(|roof| (roof.owner, roof.parent))
        .collect::<std::collections::HashMap<_, _>>();

    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| {
            child_roofs.get(&solid.owner).is_some_and(|parent_id| {
                let allowed_top = parent_id
                    .and_then(|id| plan.roof_assemblies.iter().find(|roof| roof.id == id))
                    .and_then(|parent| {
                        let point = Vec2::new(solid.centre.x, solid.centre.z);
                        parent
                            .faces
                            .iter()
                            .filter(|face| roof_face_contains_plan_point_inclusive(face, point))
                            .filter_map(|face| roof_face_height(face, point))
                            .max_by(f32::total_cmp)
                    })
                    .unwrap_or(f32::NEG_INFINITY);
                solid.centre.y + solid.size.y * 0.5 > allowed_top + 0.025
            }) && solid.role == SolidRole::RoofFraming
                && matches!(solid.shape, crate::ResolvedSolidShape::Cuboid)
                && solid.size.y > 0.35
                && solid.size.y > solid.size.x * 2.0
                && solid.size.y > solid.size.z * 2.0
                && solid.longfall_radians.abs() < 0.01
                && solid.crossfall_radians.abs() < 0.01
        })
        .map(|solid| solid.id)
        .collect()
}

fn oversized_child_roof_flashings(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_flashings = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| &roof.children)
        .filter(|child| child.kind != crate::RoofChildKind::Tower)
        .flat_map(|child| &child.flashing_ids)
        .copied()
        .collect::<std::collections::HashSet<_>>();

    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| {
            child_flashings.contains(&solid.id)
                && solid.role == SolidRole::RoofFlashing
                && (solid.size.y > 0.03 || solid.size.z > 0.12)
        })
        .map(|solid| solid.id)
        .collect()
}

fn invalid_attached_child_drainage(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_owners = plan
        .roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_some())
        .filter(|roof| {
            plan.roof_assemblies.iter().any(|parent| {
                parent.children.iter().any(|child| {
                    child.child == roof.id && child.kind != crate::RoofChildKind::Tower
                })
            })
        })
        .map(|roof| roof.owner)
        .collect::<std::collections::HashSet<_>>();

    plan.resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| child_owners.contains(&network.owner))
        .filter(|network| {
            plan.resolved_geometry
                .roof_drainage_outlets
                .iter()
                .find(|station| station.id == network.outlet_station)
                .is_none_or(|station| {
                    station.disposition != crate::RoofDrainageDisposition::FreeDripToParentRoof
                })
        })
        .map(|network| network.id)
        .collect()
}

fn invalid_dormer_trimmer_envelope(plan: &BuildingPlan) -> Vec<crate::TimberMemberId> {
    let Some(frame) = &plan.timber_frame else {
        return Vec::new();
    };
    frame
        .members
        .iter()
        .filter(|member| member.role == crate::TimberMemberRole::DormerTrimmer)
        .filter(|member| {
            !plan.roof_dormers.iter().enumerate().any(|(index, dormer)| {
                let outward = match dormer.facing {
                    crate::Direction::North => Vec2::Y,
                    crate::Direction::South => -Vec2::Y,
                    crate::Direction::East => Vec2::X,
                    crate::Direction::West => -Vec2::X,
                };
                let tangent = Vec2::new(outward.y, -outward.x);
                let half_width = dormer.width_metres
                    * if dormer.kind == crate::DormerKind::TransverseGable {
                        2.20
                    } else {
                        1.0
                    }
                    * 0.5;
                let roof_id = crate::RoofAssemblyId(1_000 + index as u64);
                let cut = plan
                    .roof_assemblies
                    .iter()
                    .flat_map(|roof| &roof.children)
                    .find(|child| child.child == roof_id)
                    .and_then(|child| {
                        plan.resolved_geometry
                            .voids
                            .iter()
                            .find(|void| void.id == child.parent_cut)
                    });
                let (rear, front) = cut.map_or((-dormer.depth_metres * 0.84, 0.0), |cut| {
                    [
                        Vec2::new(cut.bounds.min.x, cut.bounds.min.z),
                        Vec2::new(cut.bounds.min.x, cut.bounds.max.z),
                        Vec2::new(cut.bounds.max.x, cut.bounds.min.z),
                        Vec2::new(cut.bounds.max.x, cut.bounds.max.z),
                    ]
                    .map(|point| (point - dormer.centre).dot(outward))
                    .into_iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), depth| {
                        (min.min(depth), max.max(depth))
                    })
                });
                [member.start, member.end].iter().all(|point| {
                    let relative = Vec2::new(point.x, point.z) - dormer.centre;
                    let depth = relative.dot(outward);
                    depth >= rear - 0.025
                        && depth <= front + 0.025
                        && relative.dot(tangent).abs() <= half_width + 0.025
                })
            })
        })
        .map(|member| member.id)
        .collect()
}

fn oversized_attached_child_gutters(plan: &BuildingPlan) -> Vec<ResolvedItemId> {
    let child_owners = plan
        .roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_some())
        .filter(|roof| matches!(roof.kind, crate::RoofKind::Gable | crate::RoofKind::Shed))
        .map(|roof| roof.owner)
        .collect::<std::collections::HashSet<_>>();
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let mut invalid = plan
        .resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| child_owners.contains(&network.owner))
        .flat_map(|network| {
            std::iter::once(network.channel_floor)
                .chain(network.channel_lips)
                .chain(network.collector_solids.iter().copied())
                .filter(|id| {
                    solids.get(id).is_none_or(|solid| {
                        if *id == network.channel_floor {
                            solid.size.y > 0.025 || solid.size.z > 0.10
                        } else if network.collector_solids.contains(id) {
                            solid.size.y > 0.025 || solid.size.z > 0.08
                        } else {
                            solid.size.y > 0.05 || solid.size.z > 0.025
                        }
                    })
                })
        })
        .collect::<Vec<_>>();
    for network in plan
        .resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| child_owners.contains(&network.owner))
    {
        let edge = plan
            .roof_assemblies
            .iter()
            .find(|roof| roof.owner == network.owner)
            .and_then(|roof| {
                roof.edges
                    .iter()
                    .find(|edge| edge.id == network.receiving_edge)
            });
        let outlet = plan
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == network.outlet_void);
        if edge.zip(outlet).is_none_or(|(edge, outlet)| {
            let point = Vec2::new(
                (outlet.bounds.min.x + outlet.bounds.max.x) * 0.5,
                (outlet.bounds.min.z + outlet.bounds.max.z) * 0.5,
            );
            let start = Vec2::new(edge.start.x, edge.start.z);
            let delta = Vec2::new(edge.end.x - edge.start.x, edge.end.z - edge.start.z);
            let along = ((point - start).dot(delta) / delta.length_squared().max(0.000_001))
                .clamp(0.0, 1.0);
            point.distance(start + delta * along) > 0.30
        }) {
            invalid.push(network.outlet_void);
        }
    }
    invalid.sort_unstable();
    invalid.dedup();
    invalid
}

fn unseated_gabled_dormer_roofs(plan: &BuildingPlan) -> Vec<crate::RoofAssemblyId> {
    plan.roof_assemblies
        .iter()
        .flat_map(|parent| parent.children.iter().map(move |child| (parent, child)))
        .filter(|(_, child)| child.kind == crate::RoofChildKind::GabledDormer)
        .filter_map(|(parent, link)| {
            let child = plan
                .roof_assemblies
                .iter()
                .find(|roof| roof.id == link.child)?;
            let base = child
                .faces
                .iter()
                .flat_map(|face| &face.polygon)
                .map(|point| point.y)
                .fold(f32::INFINITY, f32::min);
            let rear_points = child
                .edges
                .iter()
                .filter(|edge| edge.kind == crate::RoofEdgeKind::OpeningCut)
                .flat_map(|edge| [edge.start, edge.end])
                .filter(|point| (point.y - base).abs() <= 0.025)
                .collect::<Vec<_>>();
            let rear_is_seated = rear_points.len() >= 2
                && rear_points.iter().all(|point| {
                    let plan_point = Vec2::new(point.x, point.z);
                    parent
                        .faces
                        .iter()
                        .filter(|face| roof_face_contains_plan_point_inclusive(face, plan_point))
                        .filter_map(|face| roof_face_height(face, plan_point))
                        .max_by(f32::total_cmp)
                        // The derived seam is searched on a 1 cm project
                        // station grid; allow one sloped station of vertical
                        // rise while still rejecting a visible rear gap.
                        .is_some_and(|height| height >= point.y - 0.03 && height <= point.y + 0.06)
                });
            (!rear_is_seated).then_some(child.id)
        })
        .collect()
}
