type TimberMemberKey = (u8, (i32, i32, i32), (i32, i32, i32));

struct TimberFrameBuilder<'a> {
    geometry: &'a mut ResolvedGeometry,
    owner: GeometryOwnerId,
    material: crate::StructuralTimberMaterial,
    next_member: u64,
    next_node: u64,
    next_joint: u64,
    next_interface: u64,
    node_by_point: BTreeMap<(i32, i32, i32), StructuralNodeId>,
    joint_by_node: BTreeMap<StructuralNodeId, usize>,
    member_by_key: BTreeMap<TimberMemberKey, crate::TimberMemberId>,
    members: Vec<crate::TimberFrameMember>,
    joints: Vec<crate::TimberFrameJoint>,
}

impl<'a> TimberFrameBuilder<'a> {
    fn new(
        geometry: &'a mut ResolvedGeometry,
        owner: GeometryOwnerId,
        material: crate::StructuralTimberMaterial,
    ) -> Self {
        Self {
            geometry,
            owner,
            material,
            next_member: 1,
            next_node: 30_000_000,
            next_joint: 1,
            next_interface: 1,
            node_by_point: BTreeMap::new(),
            joint_by_node: BTreeMap::new(),
            member_by_key: BTreeMap::new(),
            members: Vec::new(),
            joints: Vec::new(),
        }
    }

    fn point_key(point: Vec3) -> (i32, i32, i32) {
        (
            (point.x * 1_000.0).round() as i32,
            (point.y * 1_000.0).round() as i32,
            (point.z * 1_000.0).round() as i32,
        )
    }

    fn node(&mut self, point: Vec3) -> StructuralNodeId {
        let key = Self::point_key(point);
        if let Some(id) = self.node_by_point.get(&key) {
            return *id;
        }
        let id = StructuralNodeId(self.next_node);
        self.next_node += 1;
        let grounded = point.y <= 0.011;
        self.geometry.structural_nodes.push(StructuralNode {
            id,
            owner: self.owner,
            kind: if grounded {
                StructuralNodeKind::TimberFrameFoundation
            } else {
                StructuralNodeKind::TimberFrameJoint
            },
            position: point,
            // Support edges are added only when a real member or measured
            // bearing interface is created. Spatial proximity is not a load
            // path: a nearby post must never support this node implicitly.
            supported_by: Vec::new(),
            grounded,
        });
        let joint_id = crate::TimberJointId(self.next_joint);
        self.next_joint += 1;
        self.joint_by_node.insert(id, self.joints.len());
        self.joints.push(crate::TimberFrameJoint {
            id: joint_id,
            node: id,
            kind: if grounded {
                crate::TimberJointKind::FoundationBearing
            } else {
                crate::TimberJointKind::MortiseTenon
            },
            member_ids: Vec::new(),
            contact_interfaces: Vec::new(),
            participants: Vec::new(),
            load_direction: Vec3::Y,
            contact_area_square_metres: 0.0144,
        });
        self.node_by_point.insert(key, id);
        id
    }

    fn solid_role(role: crate::TimberMemberRole) -> SolidRole {
        match role {
            crate::TimberMemberRole::Sill => SolidRole::FrameSill,
            crate::TimberMemberRole::PrimaryPost
            | crate::TimberMemberRole::CornerPost
            | crate::TimberMemberRole::IntermediatePost => SolidRole::FramePost,
            crate::TimberMemberRole::WallPlate => SolidRole::FramePlate,
            crate::TimberMemberRole::Rail => SolidRole::FrameRail,
            crate::TimberMemberRole::FloorJoist => SolidRole::FrameJoist,
            crate::TimberMemberRole::TransverseTie => SolidRole::FrameTie,
            crate::TimberMemberRole::Girder | crate::TimberMemberRole::Purlin => {
                SolidRole::FrameGirder
            }
            crate::TimberMemberRole::HeadBrace
            | crate::TimberMemberRole::FootBrace
            | crate::TimberMemberRole::StoreyBrace => SolidRole::FrameBrace,
            crate::TimberMemberRole::JettyBeam => SolidRole::FrameJettyBeam,
            crate::TimberMemberRole::Knagge => SolidRole::FrameKnagge,
            crate::TimberMemberRole::GableTie
            | crate::TimberMemberRole::GablePost
            | crate::TimberMemberRole::Rafter
            | crate::TimberMemberRole::Collar => SolidRole::FrameGableMember,
            crate::TimberMemberRole::DormerTrimmer => SolidRole::FrameDormerTrimmer,
            crate::TimberMemberRole::Ornament => SolidRole::FrameOrnament,
        }
    }

    fn member(
        &mut self,
        role: crate::TimberMemberRole,
        start: Vec3,
        end: Vec3,
        section: Vec2,
        phase: crate::TimberFramePhase,
    ) -> crate::TimberMemberId {
        let mut a = Self::point_key(start);
        let mut b = Self::point_key(end);
        if b < a {
            std::mem::swap(&mut a, &mut b);
        }
        // All vertical post labels share one physical member key.  A corner
        // referenced by two facade programs is still one timber, not nested
        // CornerPost/PrimaryPost solids occupying the same volume.
        let role_key = match role {
            crate::TimberMemberRole::PrimaryPost
            | crate::TimberMemberRole::CornerPost
            | crate::TimberMemberRole::IntermediatePost => {
                crate::TimberMemberRole::PrimaryPost as u8
            }
            _ => role as u8,
        };
        if let Some(id) = self.member_by_key.get(&(role_key, a, b)) {
            return *id;
        }
        let start_node = self.node(start);
        let end_node = self.node(end);
        if (start.y - end.y).abs() > 0.05 {
            let (upper, lower) = if start.y > end.y {
                (start_node, end_node)
            } else {
                (end_node, start_node)
            };
            if let Some(node) = self
                .geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == upper)
            {
                node.supported_by.push(lower);
                node.supported_by.sort_unstable();
                node.supported_by.dedup();
            }
        }
        let start_joint_index = self.joint_by_node[&start_node];
        let end_joint_index = self.joint_by_node[&end_node];
        let start_joint = self.joints[start_joint_index].id;
        let end_joint = self.joints[end_joint_index].id;
        let id = crate::TimberMemberId(self.next_member);
        self.next_member += 1;
        self.joints[start_joint_index].member_ids.push(id);
        if end_joint_index != start_joint_index {
            self.joints[end_joint_index].member_ids.push(id);
        }
        let delta = end - start;
        let length = delta.length();
        debug_assert!(length > 0.05);
        let horizontal = Vec2::new(delta.x, delta.z).length();
        let yaw = if horizontal > 0.001 {
            (-delta.z).atan2(delta.x)
        } else {
            0.0
        };
        let longfall = -horizontal.atan2(delta.y);
        let solid_id =
            ResolvedItemId((1_u64 << 60) | (u64::from(self.owner.0) << 32) | self.next_member);
        self.geometry.solids.push(ResolvedSolid {
            id: solid_id,
            owner: self.owner,
            centre: (start + end) * 0.5,
            size: Vec3::new(section.x, length, section.y),
            yaw_radians: yaw,
            crossfall_radians: 0.0,
            longfall_radians: longfall,
            role: Self::solid_role(role),
            shape: crate::ResolvedSolidShape::Cuboid,
            supported_by: vec![start_node, end_node],
        });
        let make_interface = |this: &mut Self, node, point: Vec3| {
            let interface = ResolvedItemId(
                (4_u64 << 60) | (u64::from(this.owner.0) << 32) | 0x100_000 | this.next_interface,
            );
            this.next_interface += 1;
            let half = Vec3::new(section.x, section.x.min(section.y), section.y) * 0.5;
            this.geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner: this.owner,
                node,
                bounds: ResolvedBounds {
                    min: point - half,
                    max: point + half,
                },
            });
            interface
        };
        let support_interfaces = [
            make_interface(self, start_node, start),
            make_interface(self, end_node, end),
        ];
        self.joints[start_joint_index]
            .contact_interfaces
            .push(support_interfaces[0]);
        if end_joint_index != start_joint_index {
            self.joints[end_joint_index]
                .contact_interfaces
                .push(support_interfaces[1]);
        }
        self.members.push(crate::TimberFrameMember {
            id,
            owner: self.owner,
            role,
            phase,
            material: self.material,
            start_node,
            end_node,
            start_joint,
            end_joint,
            start,
            end,
            section_metres: section,
            solid: solid_id,
            support_interfaces,
            structural: role != crate::TimberMemberRole::Ornament,
        });
        self.member_by_key.insert((role_key, a, b), id);
        id
    }

    /// Resolve a node which lands on the body of a post/brace to that exact
    /// member. Facade rails and opening headers commonly meet a continuous
    /// post between its end joints; the measured intersection is a housed or
    /// lap bearing, while mere nearby geometry is deliberately ignored.
    fn resolve_intermediate_member_bearings(&mut self) {
        let candidates = self
            .members
            .iter()
            .filter(|member| member.structural)
            .cloned()
            .collect::<Vec<_>>();
        let node_ids = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner && !node.grounded)
            .map(|node| node.id)
            .collect::<Vec<_>>();
        for node_id in node_ids {
            let Some(point) = self
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.position)
            else {
                continue;
            };
            let bearing = candidates
                .iter()
                .filter_map(|member| {
                    let delta = member.end - member.start;
                    let length_squared = delta.length_squared();
                    let t = ((point - member.start).dot(delta) / length_squared).clamp(0.0, 1.0);
                    if t <= 0.001 || t >= 0.999 {
                        return None;
                    }
                    let closest = member.start + delta * t;
                    let distance = closest.distance(point);
                    (distance <= member.section_metres.min_element() * 0.55 + 0.004)
                        .then_some((member, distance))
                })
                .min_by(|left, right| left.1.total_cmp(&right.1));
            let Some((member, _)) = bearing else { continue };
            let lower = if member.start.y <= member.end.y {
                member.start_node
            } else {
                member.end_node
            };
            if let Some(node) = self
                .geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == node_id)
            {
                node.supported_by.push(lower);
            }
            let interface = ResolvedItemId(
                (4_u64 << 60) | (u64::from(self.owner.0) << 32) | 0x180_000 | self.next_interface,
            );
            self.next_interface += 1;
            let half = Vec3::new(
                member.section_metres.x,
                member.section_metres.min_element(),
                member.section_metres.y,
            ) * 0.45;
            self.geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner: self.owner,
                node: node_id,
                bounds: ResolvedBounds {
                    min: point - half,
                    max: point + half,
                },
            });
        }
    }

    /// Orient the physical timber-member/contact graph into an acyclic load
    /// tree rooted at foundations. Each resulting support edge is either a
    /// member endpoint pair or an exact point-on-member housed bearing created
    /// above; no distance-based inference participates.
    fn rebuild_physical_support_tree(&mut self) {
        let node_ids = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner)
            .map(|node| node.id)
            .collect::<BTreeSet<_>>();
        let mut adjacency = BTreeMap::<StructuralNodeId, BTreeSet<StructuralNodeId>>::new();
        for member in self.members.iter().filter(|member| member.structural) {
            adjacency
                .entry(member.start_node)
                .or_default()
                .insert(member.end_node);
            adjacency
                .entry(member.end_node)
                .or_default()
                .insert(member.start_node);
        }
        // Preserve exact intermediate point-on-member contacts and exact
        // contacts with externally grounded wall/roof authorities. The latter
        // are roots of this frame graph, not inferred nearby supports.
        let body_contacts = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner && !node.grounded)
            .flat_map(|node| {
                node.supported_by
                    .iter()
                    .map(move |parent| (node.id, *parent))
            })
            .collect::<Vec<_>>();
        let mut external_roots = Vec::new();
        for (node, parent) in body_contacts {
            if node_ids.contains(&parent) {
                adjacency.entry(node).or_default().insert(parent);
                adjacency.entry(parent).or_default().insert(node);
            } else {
                external_roots.push((node, parent));
            }
        }
        for node in self
            .geometry
            .structural_nodes
            .iter_mut()
            .filter(|node| node.owner == self.owner)
        {
            node.supported_by.clear();
        }
        for (node_id, parent) in &external_roots {
            if let Some(node) = self
                .geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == *node_id)
            {
                node.supported_by.push(*parent);
            }
        }
        let mut roots = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner && node.grounded)
            .map(|node| node.id)
            .collect::<Vec<_>>();
        roots.extend(external_roots.iter().map(|(node, _)| *node));
        roots.sort_unstable();
        roots.dedup();
        let mut visited = roots.iter().copied().collect::<BTreeSet<_>>();
        let mut queue = VecDeque::from(roots);
        while let Some(parent) = queue.pop_front() {
            for child in adjacency.get(&parent).into_iter().flatten().copied() {
                if !node_ids.contains(&child) || !visited.insert(child) {
                    continue;
                }
                if let Some(node) = self
                    .geometry
                    .structural_nodes
                    .iter_mut()
                    .find(|node| node.id == child)
                {
                    node.supported_by.push(parent);
                }
                queue.push_back(child);
            }
        }
    }

    /// Assign the compact Stage 6 joint vocabulary from the physical members
    /// which actually meet at each node. Decorative or proximity-only labels
    /// never participate in load transfer.
    fn classify_physical_joints(&mut self) {
        for joint in &mut self.joints {
            joint.contact_interfaces.sort_unstable();
            joint.contact_interfaces.dedup();
            let roles = joint
                .member_ids
                .iter()
                .filter_map(|id| self.members.iter().find(|member| member.id == *id))
                .map(|member| member.role)
                .collect::<Vec<_>>();
            let grounded = self
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == joint.node)
                .is_some_and(|node| node.grounded);
            let has = |role| roles.contains(&role);
            joint.kind = if grounded {
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
                    && (has(crate::TimberMemberRole::Rafter)
                        || has(crate::TimberMemberRole::Purlin)))
                || (has(crate::TimberMemberRole::Purlin)
                    && (has(crate::TimberMemberRole::PrimaryPost)
                        || has(crate::TimberMemberRole::GablePost)))
            {
                crate::TimberJointKind::RoofSeat
            } else if (has(crate::TimberMemberRole::FloorJoist)
                && has(crate::TimberMemberRole::Girder))
                || (has(crate::TimberMemberRole::TransverseTie)
                    && (has(crate::TimberMemberRole::PrimaryPost)
                        || has(crate::TimberMemberRole::Purlin)))
            {
                crate::TimberJointKind::HousedBeam
            } else if roles.iter().any(|role| {
                matches!(
                    role,
                    crate::TimberMemberRole::HeadBrace
                        | crate::TimberMemberRole::FootBrace
                        | crate::TimberMemberRole::StoreyBrace
                )
            }) && roles.len() >= 2
            {
                crate::TimberJointKind::Lap
            } else if roles
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
            joint.participants = joint
                .member_ids
                .iter()
                .filter_map(|member_id| {
                    let member = self.members.iter().find(|member| member.id == *member_id)?;
                    let axis = if member.start_node == joint.node {
                        member.end - member.start
                    } else if member.end_node == joint.node {
                        member.start - member.end
                    } else {
                        return None;
                    }
                    .normalize_or_zero();
                    Some(crate::TimberJointParticipant {
                        member: *member_id,
                        axis_from_joint: axis,
                        reaction_direction: -axis,
                    })
                })
                .collect();
            let role_axis = |role| {
                joint.participants.iter().find_map(|participant| {
                    self.members
                        .iter()
                        .find(|member| member.id == participant.member && member.role == role)
                        .map(|_| participant.axis_from_joint)
                })
            };
            let downward = |axis: Vec3| if axis.y <= 0.0 { axis } else { -axis };
            let gravity_biased = |axis: Vec3, lateral_weight: f32| {
                let lateral = Vec3::new(axis.x, 0.0, axis.z).normalize_or_zero();
                (lateral * lateral_weight - Vec3::Y).normalize_or_zero()
            };
            joint.load_direction = match joint.kind {
                crate::TimberJointKind::JettyBearing => {
                    role_axis(crate::TimberMemberRole::JettyBeam)
                        .map(|axis| gravity_biased(axis, 0.65))
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
                crate::TimberJointKind::HousedBeam => {
                    role_axis(crate::TimberMemberRole::FloorJoist)
                        .or_else(|| role_axis(crate::TimberMemberRole::TransverseTie))
                        .map(|axis| gravity_biased(axis, 0.25))
                }
                crate::TimberJointKind::Scarf => role_axis(crate::TimberMemberRole::Sill)
                    .or_else(|| role_axis(crate::TimberMemberRole::WallPlate))
                    .map(|axis| gravity_biased(axis, 0.20)),
                _ => joint
                    .participants
                    .iter()
                    .max_by(|left, right| {
                        left.axis_from_joint
                            .y
                            .abs()
                            .total_cmp(&right.axis_from_joint.y.abs())
                    })
                    .map(|participant| {
                        if participant.axis_from_joint.y.abs() >= 0.35 {
                            downward(participant.axis_from_joint)
                        } else {
                            gravity_biased(participant.axis_from_joint, 0.15)
                        }
                    }),
            }
            .unwrap_or(-Vec3::Y)
            .normalize_or_zero();
        }
    }
}

fn timber_program_kind(archetype: BuildingArchetype) -> Option<crate::TimberFrameProgramKind> {
    Some(match archetype {
        BuildingArchetype::TownHouse => crate::TimberFrameProgramKind::NarrowUrbanTownHouse,
        BuildingArchetype::HallHouse => crate::TimberFrameProgramKind::NorthernTwoPostHallHouse,
        BuildingArchetype::FachwerkCottage => crate::TimberFrameProgramKind::DirectRoofCottage,
        BuildingArchetype::FachwerkMerchantHouse => {
            crate::TimberFrameProgramKind::JettiedMerchantHouse
        }
        BuildingArchetype::RenaissanceTownHall => {
            crate::TimberFrameProgramKind::CivicMasonryTimberHall
        }
        _ => return None,
    })
}

fn timber_member_wall_polygon(
    member: &crate::TimberFrameMember,
    wall: &crate::WallAssembly,
) -> Polygon<f32> {
    let project = |point: Vec3| {
        Vec2::new(
            (Vec2::new(point.x, point.z) - wall.frame.origin).dot(wall.frame.tangent),
            point.y - wall.base_elevation_metres,
        )
    };
    let start = project(member.start);
    let end = project(member.end);
    timber_member_end_face_polygon(start, end, member.section_metres.max_element() * 0.5)
}

#[cfg(test)]
mod wall_infill_tests {
    use super::*;

    #[test]
    fn timber_subtraction_stops_at_the_rendered_member_end_faces() {
        let polygon = timber_member_end_face_polygon(Vec2::new(1.0, 2.0), Vec2::new(3.0, 2.0), 0.25);
        let coordinates = &polygon.exterior().0;

        assert_eq!(coordinates[0], Coord { x: 1.0, y: 1.75 });
        assert_eq!(coordinates[1], Coord { x: 3.0, y: 1.75 });
        assert_eq!(coordinates[2], Coord { x: 3.0, y: 2.25 });
        assert_eq!(coordinates[3], Coord { x: 1.0, y: 2.25 });
    }
}

fn triangulate_panel_polygon(polygon: &Polygon<f32>) -> Vec<[Vec2; 3]> {
    let mut vertices = polygon
        .exterior()
        .0
        .iter()
        .take(polygon.exterior().0.len().saturating_sub(1))
        .map(|coord| Vec2::new(coord.x, coord.y))
        .collect::<Vec<_>>();
    let mut holes = Vec::new();
    for interior in polygon.interiors() {
        holes.push(vertices.len() as u32);
        vertices.extend(
            interior
                .0
                .iter()
                .take(interior.0.len().saturating_sub(1))
                .map(|coord| Vec2::new(coord.x, coord.y)),
        );
    }
    let mut indices = Vec::new();
    earcut::Earcut::<f32>::new().earcut(
        vertices.iter().map(|point| [point.x, point.y]),
        &holes,
        &mut indices,
    );
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|triangle| {
            let points = [
                vertices[triangle[0] as usize],
                vertices[triangle[1] as usize],
                vertices[triangle[2] as usize],
            ];
            (((points[1] - points[0]).perp_dot(points[2] - points[0])).abs() > 0.000_01)
                .then_some(points)
        })
        .collect()
}

fn resolve_timber_frame_assembly(
    program: &BuildingProgram,
    edits: &[BuildingEdit],
    walls: &mut [crate::WallAssembly],
    openings: &[crate::OpeningAssembly],
    roofs: &[RoofPiece],
    dormers: &[RoofDormer],
    stairs: &mut [Stair],
    roof_assemblies: &mut [RoofAssembly],
    geometry: &mut ResolvedGeometry,
) -> Option<crate::TimberFrameAssembly> {
    let program_kind = timber_program_kind(program.archetype)?;
    let owner = GeometryOwnerId(82_000);
    let frame_material = if matches!(
        program_kind,
        crate::TimberFrameProgramKind::NorthernTwoPostHallHouse
            | crate::TimberFrameProgramKind::DirectRoofCottage
    ) {
        crate::StructuralTimberMaterial::Oak
    } else {
        crate::StructuralTimberMaterial::Fir
    };
    let mut builder = TimberFrameBuilder::new(geometry, owner, frame_material);
    let mut facades = Vec::new();
    let mut bays = Vec::new();
    let mut next_facade = 1_u64;
    let mut next_line = 1_u64;
    let mut next_storey = 1_u64;
    let mut next_bay = 1_u64;
    let section = Vec2::splat(if program.archetype == BuildingArchetype::FachwerkCottage {
        0.13
    } else {
        0.15
    });
    let directions = [
        Direction::South,
        Direction::East,
        Direction::North,
        Direction::West,
    ];
    for direction in directions {
        let outward = direction_vector(direction);
        let tangent = if outward.y.abs() > 0.5 {
            Vec2::X
        } else {
            Vec2::Y
        };
        let mut line_storeys = Vec::new();
        let mut line_origin = Vec2::ZERO;
        let mut line_length = 0.0_f32;
        for level in 0..program.storeys.len() as u16 {
            let mut facade_walls = walls
                .iter()
                .filter(|wall| {
                    wall.storey_level == level
                        && wall.frame.outside_room.is_none()
                        && wall.frame.outward.dot(outward) > 0.99
                        && wall.material == crate::WallMaterialClass::TimberInfill
                        && matches!(wall.source, crate::WallSourceId::StoreyWall { .. })
                })
                .collect::<Vec<_>>();
            facade_walls.sort_by(|left, right| {
                left.frame
                    .origin
                    .dot(tangent)
                    .total_cmp(&right.frame.origin.dot(tangent))
            });
            if facade_walls.is_empty() {
                continue;
            }
            line_origin = facade_walls
                .iter()
                .map(|wall| wall.frame.origin)
                .sum::<Vec2>()
                / facade_walls.len() as f32;
            line_length = facade_walls.len() as f32 * CELL_SIZE_METRES;
            let base = f32::from(level) * program.storey_height_metres;
            let top = base + program.storey_height_metres;
            let mut storey_member_ids = Vec::new();
            let mut bay_ids = Vec::new();
            for (wall_index, wall) in facade_walls.iter().enumerate() {
                let plane = wall.frame.origin
                    + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5);
                let left_plan = plane - tangent * wall.length_metres * 0.5;
                let right_plan = plane + tangent * wall.length_metres * 0.5;
                let left_bottom = Vec3::new(left_plan.x, base, left_plan.y);
                let right_bottom = Vec3::new(right_plan.x, base, right_plan.y);
                let left_top = Vec3::new(left_plan.x, top, left_plan.y);
                let right_top = Vec3::new(right_plan.x, top, right_plan.y);
                let mut member_ids = vec![
                    builder.member(
                        crate::TimberMemberRole::Sill,
                        left_bottom,
                        right_bottom,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::WallPlate,
                        left_top,
                        right_top,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                    builder.member(
                        if wall_index == 0 {
                            crate::TimberMemberRole::CornerPost
                        } else {
                            crate::TimberMemberRole::PrimaryPost
                        },
                        left_bottom,
                        left_top,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                    builder.member(
                        if wall_index + 1 == facade_walls.len() {
                            crate::TimberMemberRole::CornerPost
                        } else {
                            crate::TimberMemberRole::PrimaryPost
                        },
                        right_bottom,
                        right_top,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                ];
                let opening = wall
                    .opening_ids
                    .first()
                    .and_then(|id| openings.iter().find(|opening| opening.id == *id));
                if let Some(opening) = opening {
                    let void_bounds = builder
                        .geometry
                        .voids
                        .iter()
                        .find(|void| void.id == opening.void_id)
                        .map(|void| void.bounds);
                    let half = void_bounds.map_or_else(
                        || opening.profile.interior_width_metres() * 0.5,
                        |bounds| {
                            let size = bounds.max - bounds.min;
                            (size.x * tangent.x.abs() + size.z * tangent.y.abs()) * 0.5
                        },
                    );
                    let (sill, head) = void_bounds.map_or_else(
                        || {
                            (
                                opening.sill_elevation_metres,
                                opening.sill_elevation_metres
                                    + opening.profile.clear_height_metres(),
                            )
                        },
                        |bounds| (bounds.min.y, bounds.max.y),
                    );
                    let left_jamb_plan = plane - tangent * half;
                    let right_jamb_plan = plane + tangent * half;
                    let left_jamb_bottom = Vec3::new(left_jamb_plan.x, base, left_jamb_plan.y);
                    let right_jamb_bottom = Vec3::new(right_jamb_plan.x, base, right_jamb_plan.y);
                    let left_jamb_top = Vec3::new(left_jamb_plan.x, top, left_jamb_plan.y);
                    let right_jamb_top = Vec3::new(right_jamb_plan.x, top, right_jamb_plan.y);
                    member_ids.extend([
                        builder.member(
                            crate::TimberMemberRole::IntermediatePost,
                            left_jamb_bottom,
                            left_jamb_top,
                            section * 0.9,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::IntermediatePost,
                            right_jamb_bottom,
                            right_jamb_top,
                            section * 0.9,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::Rail,
                            Vec3::new(left_jamb_plan.x, sill, left_jamb_plan.y),
                            Vec3::new(right_jamb_plan.x, sill, right_jamb_plan.y),
                            section * 0.88,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::Rail,
                            Vec3::new(
                                left_jamb_plan.x,
                                head + section.x * 0.5 + 0.01,
                                left_jamb_plan.y,
                            ),
                            Vec3::new(
                                right_jamb_plan.x,
                                head + section.x * 0.5 + 0.01,
                                right_jamb_plan.y,
                            ),
                            section,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                    ]);
                    // Each side panel owns a closed triangular racking frame:
                    // the paired braces share one explicit jamb node and the
                    // corner post closes the third side.  The former foot/head
                    // braces stopped at unrelated sill/head nodes, so they
                    // looked plausible but could not transmit racking load.
                    let brace_joint_y = (sill + head) * 0.5;
                    let left_brace_joint =
                        Vec3::new(left_jamb_plan.x, brace_joint_y, left_jamb_plan.y);
                    let right_brace_joint =
                        Vec3::new(right_jamb_plan.x, brace_joint_y, right_jamb_plan.y);
                    member_ids.extend([
                        builder.member(
                            crate::TimberMemberRole::FootBrace,
                            left_bottom,
                            left_brace_joint,
                            section * 0.74,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::HeadBrace,
                            left_brace_joint,
                            left_top,
                            section * 0.70,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::FootBrace,
                            right_bottom,
                            right_brace_joint,
                            section * 0.74,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::HeadBrace,
                            right_brace_joint,
                            right_top,
                            section * 0.70,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                    ]);
                } else {
                    let centre_plan = (left_plan + right_plan) * 0.5;
                    let centre_bottom = Vec3::new(centre_plan.x, base, centre_plan.y);
                    let centre_top = Vec3::new(centre_plan.x, top, centre_plan.y);
                    member_ids.push(builder.member(
                        crate::TimberMemberRole::IntermediatePost,
                        centre_bottom,
                        centre_top,
                        section * 0.78,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                    let waist = base + program.storey_height_metres * 0.56;
                    member_ids.push(builder.member(
                        crate::TimberMemberRole::Rail,
                        Vec3::new(left_plan.x, waist, left_plan.y),
                        Vec3::new(right_plan.x, waist, right_plan.y),
                        section * 0.78,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                    let editor_style = edits.iter().rev().find_map(|edit| match edit {
                        BuildingEdit::SetTimberFrameStyle { style } => Some(*style),
                        _ => None,
                    });
                    let rising = editor_style.map_or_else(
                        || (wall_index + usize::from(level)).is_multiple_of(2),
                        |style| match style {
                            crate::TimberFrameStyle::LateMedieval => {
                                (wall_index + usize::from(level)).is_multiple_of(2)
                            }
                            crate::TimberFrameStyle::NorthernCloseStudded => {
                                wall_index.is_multiple_of(2)
                            }
                            crate::TimberFrameStyle::EarlyModernOrnate => {
                                (wall_index / 2 + usize::from(level)).is_multiple_of(2)
                            }
                        },
                    );
                    let (brace_start, brace_end) = if rising {
                        (left_bottom, right_top)
                    } else {
                        (right_bottom, left_top)
                    };
                    member_ids.push(builder.member(
                        crate::TimberMemberRole::StoreyBrace,
                        brace_start,
                        brace_end,
                        section * 0.76,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                }
                member_ids.sort_unstable();
                member_ids.dedup();
                let bay_id = crate::TimberFrameBayId(next_bay);
                next_bay += 1;
                bay_ids.push(bay_id);
                storey_member_ids.extend(member_ids.iter().copied());
                bays.push(crate::TimberFrameBay {
                    id: bay_id,
                    wall: Some(wall.id),
                    opening: opening.map(|opening| opening.id),
                    member_ids,
                    infill_solids: wall
                        .host_solids
                        .iter()
                        .copied()
                        .filter(|id| {
                            builder.geometry.solids.iter().any(|solid| {
                                solid.id == *id
                                    && matches!(
                                        solid.role,
                                        SolidRole::WallHost
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningSill
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                    )
                            })
                        })
                        .collect(),
                });
            }
            storey_member_ids.sort_unstable();
            storey_member_ids.dedup();
            let jetty = if level == 1 && program.upper_storey_projection_metres > 0.01 {
                let projection = program.upper_storey_projection_metres;
                let backspan = 0.95_f32;
                let mut jetty_beams = Vec::new();
                let mut knaggen = Vec::new();
                let mut corner_supports = Vec::new();
                for (index, wall) in facade_walls.iter().enumerate() {
                    let plane = wall.frame.origin
                        + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5);
                    for sign in [-1.0_f32, 1.0] {
                        let boundary = plane + tangent * sign * wall.length_metres * 0.5;
                        let outer = Vec3::new(boundary.x, base, boundary.y);
                        let inner_plan = boundary - outward * (projection + backspan);
                        let inner = Vec3::new(inner_plan.x, base, inner_plan.y);
                        let beam = builder.member(
                            crate::TimberMemberRole::JettyBeam,
                            inner,
                            outer,
                            section,
                            crate::TimberFramePhase::UpperStoreyAddition,
                        );
                        jetty_beams.push(beam);
                        let lower_plan = boundary - outward * projection;
                        let lower = Vec3::new(
                            lower_plan.x,
                            base - program.storey_height_metres * 0.28,
                            lower_plan.y,
                        );
                        let knagge = builder.member(
                            crate::TimberMemberRole::Knagge,
                            lower,
                            outer,
                            section * 0.9,
                            crate::TimberFramePhase::UpperStoreyAddition,
                        );
                        knaggen.push(knagge);
                        if index == 0 || index + 1 == facade_walls.len() {
                            corner_supports.push(knagge);
                        }
                    }
                }
                jetty_beams.sort_unstable();
                jetty_beams.dedup();
                let mut inner_bearings = jetty_beams
                    .iter()
                    .filter_map(|id| builder.members.iter().find(|member| member.id == *id))
                    .map(|member| member.start)
                    .collect::<Vec<_>>();
                inner_bearings.sort_by(|left, right| {
                    Vec2::new(left.x, left.z)
                        .dot(tangent)
                        .total_cmp(&Vec2::new(right.x, right.z).dot(tangent))
                });
                if let (Some(first), Some(last)) = (
                    inner_bearings.first().copied(),
                    inner_bearings.last().copied(),
                ) && first.distance(last) > 0.10
                {
                    let inner_girder = builder.member(
                        crate::TimberMemberRole::Girder,
                        first,
                        last,
                        section * 1.12,
                        crate::TimberFramePhase::UpperStoreyAddition,
                    );
                    storey_member_ids.push(inner_girder);
                }
                knaggen.sort_unstable();
                knaggen.dedup();
                corner_supports.sort_unstable();
                corner_supports.dedup();
                storey_member_ids.extend(jetty_beams.iter().copied());
                storey_member_ids.extend(knaggen.iter().copied());
                let outer_plane = facade_walls
                    .iter()
                    .map(|wall| {
                        wall.frame.origin
                            + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5)
                    })
                    .sum::<Vec2>()
                    / facade_walls.len() as f32;
                // Only the projecting strip is a separate jetty plate. The
                // backspan remains part of the main storey floor assembled
                // below, avoiding duplicate overlapping floor authority.
                let floor_depth = projection;
                let floor_centre_plan = outer_plane - outward * floor_depth * 0.5;
                let floor_solid = ResolvedItemId(
                    (1_u64 << 60) | (u64::from(owner.0) << 32) | 0x0f00_0000 | next_storey,
                );
                let floor_support_nodes = jetty_beams
                    .iter()
                    .filter_map(|id| builder.members.iter().find(|member| member.id == *id))
                    .flat_map(|member| [member.start_node, member.end_node])
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                builder.geometry.solids.push(ResolvedSolid {
                    id: floor_solid,
                    owner,
                    centre: Vec3::new(floor_centre_plan.x, base - 0.08, floor_centre_plan.y),
                    size: Vec3::new(line_length, 0.16, floor_depth),
                    yaw_radians: (-tangent.y).atan2(tangent.x),
                    crossfall_radians: 0.0,
                    longfall_radians: 0.0,
                    role: SolidRole::FrameFloor,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: floor_support_nodes,
                });
                let mut floor_bearing_interfaces = Vec::new();
                for member in jetty_beams
                    .iter()
                    .filter_map(|id| builder.members.iter().find(|member| member.id == *id))
                {
                    let inward = (member.start - member.end).normalize_or_zero();
                    let contact = member.end + inward * (projection * 0.5) - Vec3::Y * 0.04;
                    let interface = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x300_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: interface,
                        owner,
                        node: member.end_node,
                        bounds: ResolvedBounds {
                            min: contact - Vec3::new(0.07, 0.025, 0.07),
                            max: contact + Vec3::new(0.07, 0.025, 0.07),
                        },
                    });
                    floor_bearing_interfaces.push(interface);
                }
                let half_length = line_length * 0.5;
                let left_outer = outer_plane - tangent * half_length;
                let right_outer = outer_plane + tangent * half_length;
                let structural_depth = projection + backspan;
                let left_inner = left_outer - outward * structural_depth;
                let right_inner = right_outer - outward * structural_depth;
                Some(crate::TimberJettyAssembly {
                    projection_metres: projection,
                    backspan_metres: backspan,
                    jetty_beams,
                    knaggen,
                    corner_supports,
                    floor_solid,
                    floor_bearing_interfaces,
                    support_polygon: vec![left_inner, right_inner, right_outer, left_outer],
                })
            } else {
                None
            };
            line_storeys.push(crate::TimberStoreyFrame {
                id: crate::TimberStoreyFrameId(next_storey),
                level,
                kind: match (program_kind, level) {
                    (crate::TimberFrameProgramKind::DirectRoofCottage, _) => {
                        crate::TimberStoreyKind::GroundFrame
                    }
                    (crate::TimberFrameProgramKind::CivicMasonryTimberHall, 0) => {
                        crate::TimberStoreyKind::MasonryPlinth
                    }
                    (crate::TimberFrameProgramKind::CivicMasonryTimberHall, _) => {
                        crate::TimberStoreyKind::CivicTimberHall
                    }
                    (_, 0) => crate::TimberStoreyKind::GroundFrame,
                    _ => crate::TimberStoreyKind::UpperFrame,
                },
                base_elevation_metres: base,
                top_elevation_metres: top,
                bay_ids,
                member_ids: storey_member_ids,
                jetty,
            });
            next_storey += 1;
        }
        if !line_storeys.is_empty() {
            facades.push(crate::TimberFrameFacade {
                id: crate::TimberFacadeId(next_facade),
                outward: direction,
                lines: vec![crate::TimberFrameLine {
                    id: crate::TimberFrameLineId(next_line),
                    origin: line_origin,
                    tangent,
                    outward,
                    length_metres: line_length,
                    internal: false,
                    storeys: line_storeys,
                }],
            });
            next_facade += 1;
            next_line += 1;
        }
    }

    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let mut internal_lines = Vec::new();
    if program_kind == crate::TimberFrameProgramKind::NorthernTwoPostHallHouse {
        let ridge_x = roofs
            .first()
            .is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
        let tangent = if ridge_x { Vec2::X } else { Vec2::Y };
        let cross = Vec2::new(-tangent.y, tangent.x);
        let centre = dimensions * 0.5;
        // The two longitudinal post rows terminate inside the gable enclosure;
        // they bear the roof without piercing an opening or the weather skin.
        // 0.60 m end clearances are a coarse animation/buildability gate.
        let length = (if ridge_x { dimensions.x } else { dimensions.y } - 1.20).max(3.0);
        let row_offset = if ridge_x { dimensions.y } else { dimensions.x } * 0.20;
        for side in [-1.0_f32, 1.0] {
            let row_centre = centre + cross * row_offset * side;
            let count = (length / 3.0).ceil() as usize;
            let mut member_ids = Vec::new();
            for index in 0..=count {
                let along = -length * 0.5 + length * index as f32 / count as f32;
                let plan = row_centre + tangent * along;
                member_ids.push(builder.member(
                    crate::TimberMemberRole::PrimaryPost,
                    Vec3::new(plan.x, 0.0, plan.y),
                    Vec3::new(plan.x, program.storey_height_metres, plan.y),
                    section * 1.15,
                    crate::TimberFramePhase::PrimaryConstruction,
                ));
                if index < count {
                    let next_along = -length * 0.5 + length * (index + 1) as f32 / count as f32;
                    let next = row_centre + tangent * next_along;
                    let brace_start = Vec3::new(plan.x, 0.0, plan.y);
                    let brace_end = Vec3::new(next.x, program.storey_height_metres, next.y);
                    let crosses_opening = openings.iter().any(|opening| {
                        builder
                            .geometry
                            .voids
                            .iter()
                            .find(|void| void.id == opening.void_id)
                            .is_some_and(|void| {
                                (0..=32).any(|sample| {
                                    let point = brace_start.lerp(brace_end, sample as f32 / 32.0);
                                    point.x >= void.bounds.min.x - 0.08
                                        && point.x <= void.bounds.max.x + 0.08
                                        && point.y >= void.bounds.min.y - 0.08
                                        && point.y <= void.bounds.max.y + 0.08
                                        && point.z >= void.bounds.min.z - 0.08
                                        && point.z <= void.bounds.max.z + 0.08
                                })
                            })
                    });
                    if !crosses_opening {
                        member_ids.push(builder.member(
                            crate::TimberMemberRole::FootBrace,
                            brace_start,
                            brace_end,
                            section * 0.82,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ));
                    }
                }
            }
            for index in 0..count {
                let a_along = -length * 0.5 + length * index as f32 / count as f32;
                let b_along = -length * 0.5 + length * (index + 1) as f32 / count as f32;
                let a = row_centre + tangent * a_along;
                let b = row_centre + tangent * b_along;
                member_ids.push(builder.member(
                    crate::TimberMemberRole::Purlin,
                    Vec3::new(a.x, program.storey_height_metres, a.y),
                    Vec3::new(b.x, program.storey_height_metres, b.y),
                    section * 1.2,
                    crate::TimberFramePhase::RoofConstruction,
                ));
            }
            internal_lines.push(crate::TimberFrameLine {
                id: crate::TimberFrameLineId(next_line),
                origin: row_centre,
                tangent,
                outward: cross * side,
                length_metres: length,
                internal: true,
                storeys: vec![crate::TimberStoreyFrame {
                    id: crate::TimberStoreyFrameId(next_storey),
                    level: 0,
                    kind: crate::TimberStoreyKind::GroundFrame,
                    base_elevation_metres: 0.0,
                    top_elevation_metres: program.storey_height_metres,
                    bay_ids: Vec::new(),
                    member_ids,
                    jetty: None,
                }],
            });
            next_line += 1;
            next_storey += 1;
        }
        let tie_count = (length / 3.0).ceil() as usize;
        for index in 0..=tie_count {
            let along = -length * 0.5 + length * index as f32 / tie_count as f32;
            let plan = centre + tangent * along;
            let a = plan - cross * row_offset;
            let b = plan + cross * row_offset;
            let (brace_start, brace_end) = if index.is_multiple_of(2) {
                (
                    Vec3::new(a.x, 0.0, a.y),
                    Vec3::new(b.x, program.storey_height_metres, b.y),
                )
            } else {
                (
                    Vec3::new(b.x, 0.0, b.y),
                    Vec3::new(a.x, program.storey_height_metres, a.y),
                )
            };
            let crosses_opening = openings.iter().any(|opening| {
                builder
                    .geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
                    .is_some_and(|void| {
                        (0..=32).any(|sample| {
                            let point = brace_start.lerp(brace_end, sample as f32 / 32.0);
                            point.x >= void.bounds.min.x - 0.08
                                && point.x <= void.bounds.max.x + 0.08
                                && point.y >= void.bounds.min.y - 0.08
                                && point.y <= void.bounds.max.y + 0.08
                                && point.z >= void.bounds.min.z - 0.08
                                && point.z <= void.bounds.max.z + 0.08
                        })
                    })
            });
            if crosses_opening {
                continue;
            }
            let tie = builder.member(
                crate::TimberMemberRole::TransverseTie,
                Vec3::new(a.x, program.storey_height_metres, a.y),
                Vec3::new(b.x, program.storey_height_metres, b.y),
                section * 1.1,
                crate::TimberFramePhase::RoofConstruction,
            );
            let brace = builder.member(
                crate::TimberMemberRole::StoreyBrace,
                brace_start,
                brace_end,
                section * 0.82,
                crate::TimberFramePhase::PrimaryConstruction,
            );
            let mut transverse_members = vec![tie, brace];
            transverse_members.extend(builder.members.iter().filter_map(|member| {
                (member.role == crate::TimberMemberRole::PrimaryPost
                    && ((member.start.distance(Vec3::new(a.x, 0.0, a.y)) <= 0.003
                        && member
                            .end
                            .distance(Vec3::new(a.x, program.storey_height_metres, a.y))
                            <= 0.003)
                        || (member.start.distance(Vec3::new(b.x, 0.0, b.y)) <= 0.003
                            && member.end.distance(Vec3::new(
                                b.x,
                                program.storey_height_metres,
                                b.y,
                            )) <= 0.003)))
                    .then_some(member.id)
            }));
            transverse_members.sort_unstable();
            transverse_members.dedup();
            internal_lines.push(crate::TimberFrameLine {
                id: crate::TimberFrameLineId(next_line),
                origin: plan,
                tangent: cross,
                outward: tangent,
                length_metres: row_offset * 2.0,
                internal: true,
                storeys: vec![crate::TimberStoreyFrame {
                    id: crate::TimberStoreyFrameId(next_storey),
                    level: 0,
                    kind: crate::TimberStoreyKind::GroundFrame,
                    base_elevation_metres: 0.0,
                    top_elevation_metres: program.storey_height_metres,
                    bay_ids: Vec::new(),
                    member_ids: transverse_members,
                    jetty: None,
                }],
            });
            next_line += 1;
            next_storey += 1;
        }
    }

    let top = program.storeys.len() as f32 * program.storey_height_metres;
    // Facade corner posts are storey-height segments with shared end joints.
    // Do not overlay them with a second ground-to-roof post: that former
    // shortcut created nested positive-volume timbers and two competing load
    // authorities at every corner.
    if let Some(roof) = roofs.first() {
        let half_width = if roof.ridge_axis == RidgeAxis::X {
            roof.size.y * 0.5
        } else {
            roof.size.x * 0.5
        };
        let rise = half_width * roof.pitch_degrees.to_radians().tan();
        let ridge_tangent = if roof.ridge_axis == RidgeAxis::X {
            Vec2::X
        } else {
            Vec2::Y
        };
        let gable_tangent = Vec2::new(-ridge_tangent.y, ridge_tangent.x);
        let half_length = if roof.ridge_axis == RidgeAxis::X {
            roof.size.x * 0.5
        } else {
            roof.size.y * 0.5
        };
        let frame_count = ((half_length * 2.0) / 1.80).ceil().max(1.0) as usize;
        let mut roof_frames = Vec::new();
        for frame_index in 0..=frame_count {
            let along = -half_length + half_length * 2.0 * frame_index as f32 / frame_count as f32;
            let gable_centre = roof.centre + ridge_tangent * along;
            if frame_index != 0
                && frame_index != frame_count
                && dormers.iter().any(|dormer| {
                    (dormer.centre - gable_centre).dot(ridge_tangent).abs()
                        <= dormer.width_metres * 0.5 + 0.40
                })
            {
                // The child roof owns its cut and four-sided trimmer frame;
                // a regular parent truss may not continue through that cut.
                continue;
            }
            let left = gable_centre - gable_tangent * half_width;
            let right = gable_centre + gable_tangent * half_width;
            // A half-hip does not have the full ridge elevation at its end
            // frames.  The former full-height A-frame recipe was structurally
            // grounded but projected through the two upper hip faces.  Match
            // the Stage 4 half-hip construction: the retained lower gable
            // reaches 55% of the rise at the end, then the frame apex climbs
            // along the short hip to the main ridge.
            let station_rise = if roof.kind == RoofKind::HalfHip {
                let hip_run = (half_width * 0.45).max(0.001);
                let distance_from_end = (half_length - along.abs()).max(0.0);
                rise * (0.55 + 0.45 * (distance_from_end / hip_run).clamp(0.0, 1.0))
            } else {
                rise
            };
            let apex = Vec3::new(gable_centre.x, top + station_rise, gable_centre.y);
            let left_base = Vec3::new(left.x, top, left.y);
            let right_base = Vec3::new(right.x, top, right.y);
            builder.member(
                crate::TimberMemberRole::GableTie,
                left_base,
                right_base,
                section,
                crate::TimberFramePhase::RoofConstruction,
            );
            builder.member(
                crate::TimberMemberRole::GablePost,
                Vec3::new(gable_centre.x, top, gable_centre.y),
                apex,
                section,
                crate::TimberFramePhase::RoofConstruction,
            );
            let collar_y = top + station_rise * 0.58;
            let collar_half = half_width * (1.0 - 0.58);
            let collar_left = gable_centre - gable_tangent * collar_half;
            let collar_right = gable_centre + gable_tangent * collar_half;
            let collar_left = Vec3::new(collar_left.x, collar_y, collar_left.y);
            let collar_right = Vec3::new(collar_right.x, collar_y, collar_right.y);
            for (base, collar) in [(left_base, collar_left), (right_base, collar_right)] {
                builder.member(
                    crate::TimberMemberRole::Rafter,
                    base,
                    collar,
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                );
                builder.member(
                    crate::TimberMemberRole::Rafter,
                    collar,
                    apex,
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                );
            }
            builder.member(
                crate::TimberMemberRole::Collar,
                collar_left,
                collar_right,
                section * 0.82,
                crate::TimberFramePhase::RoofConstruction,
            );
            roof_frames.push((collar_left, apex, collar_right));
        }
        for pair in roof_frames.windows(2) {
            for (left, right) in [
                (pair[0].0, pair[1].0),
                (pair[0].1, pair[1].1),
                (pair[0].2, pair[1].2),
            ] {
                builder.member(
                    crate::TimberMemberRole::Purlin,
                    left,
                    right,
                    section * 1.05,
                    crate::TimberFramePhase::RoofConstruction,
                );
            }
        }
    }

    let mut dormer_trimmer_members = Vec::new();
    for (dormer_index, dormer) in dormers.iter().enumerate() {
        let outward = direction_vector(dormer.facing);
        let tangent = Vec2::new(outward.y, -outward.x);
        let mut local_trimmers = Vec::new();
        // A facade-derived transverse gable starts at the facade and cuts
        // inward.  Treating its source centre like an ordinary dormer centre
        // put half the curb outside the wall/eave and directly through the
        // accepted roof drainage fall line.
        // Every attached child starts at its visible front wall and extends
        // inward through the parent slope.  The old ordinary-dormer curb was
        // centred on that front wall (-0.45..+0.45), leaving half of its
        // trimmers visibly cantilevered out over the parent covering.  Share
        // the exact front/rear datum used by the child enclosure instead.
        let roof_id = RoofAssemblyId(1_000 + dormer_index as u64);
        let exact_cut = roof_assemblies
            .iter()
            .flat_map(|roof| &roof.children)
            .find(|child| child.child == roof_id)
            .and_then(|child| {
                builder
                    .geometry
                    .voids
                    .iter()
                    .find(|void| void.id == child.parent_cut)
            });
        let (rear_depth, front_depth) = exact_cut.map_or((-0.84_f32, 0.0_f32), |cut| {
            let projected = [
                Vec2::new(cut.bounds.min.x, cut.bounds.min.z),
                Vec2::new(cut.bounds.min.x, cut.bounds.max.z),
                Vec2::new(cut.bounds.max.x, cut.bounds.min.z),
                Vec2::new(cut.bounds.max.x, cut.bounds.max.z),
            ]
            .map(|point| (point - dormer.centre).dot(outward) / dormer.depth_metres);
            (
                projected.iter().copied().fold(f32::INFINITY, f32::min),
                projected.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            )
        });
        let trimmer_height = |point: Vec2, depth: f32| {
            roof_assemblies
                .iter()
                .find(|roof| roof.parent.is_none())
                .and_then(|roof| roof_underside_height_at(roof, point))
                .unwrap_or(
                    dormer.base_height_metres
                        - if dormer.kind == DormerKind::TransverseGable && depth == front_depth {
                            0.18
                        } else {
                            0.0
                        },
                )
        };
        for side in [-1.0_f32, 1.0] {
            let offset = tangent * side * dormer.width_metres * 0.5;
            let start = dormer.centre + offset + outward * dormer.depth_metres * rear_depth;
            let end = dormer.centre + offset + outward * dormer.depth_metres * front_depth;
            let trimmer = builder.member(
                crate::TimberMemberRole::DormerTrimmer,
                Vec3::new(start.x, trimmer_height(start, rear_depth), start.y),
                Vec3::new(end.x, trimmer_height(end, front_depth), end.y),
                section * 0.9,
                crate::TimberFramePhase::RoofConstruction,
            );
            dormer_trimmer_members.push(trimmer);
            local_trimmers.push(trimmer);
        }
        // The two longitudinal trimmers are tied into front and rear headers,
        // forming an authoritative four-sided curb around the Stage 4 parent
        // cut. This gives the child cheeks/front a closed load-transfer frame
        // instead of two independently floating roof bars.
        for depth in [rear_depth, front_depth] {
            let centre = dormer.centre + outward * dormer.depth_metres * depth;
            let start = centre - tangent * dormer.width_metres * 0.5;
            let end = centre + tangent * dormer.width_metres * 0.5;
            let trimmer = builder.member(
                crate::TimberMemberRole::DormerTrimmer,
                Vec3::new(start.x, trimmer_height(start, depth), start.y),
                Vec3::new(end.x, trimmer_height(end, depth), end.y),
                section * 0.9,
                crate::TimberFramePhase::RoofConstruction,
            );
            dormer_trimmer_members.push(trimmer);
            local_trimmers.push(trimmer);
        }
        let child_wall = walls.iter().find(|wall| {
            matches!(wall.source, crate::WallSourceId::RoofChildFront { roof } if roof == roof_id)
                && wall.material == crate::WallMaterialClass::TimberInfill
        });
        if let Some(wall) = child_wall {
            let opening = wall
                .opening_ids
                .first()
                .and_then(|id| openings.iter().find(|opening| opening.id == *id));
            let plane = wall.frame.origin
                + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5);
            let half = wall.length_metres * 0.5;
            let left = plane - tangent * half;
            let right = plane + tangent * half;
            let base = wall.base_elevation_metres;
            let top = base + wall.height_metres;
            let mut member_ids = local_trimmers;
            member_ids.extend([
                builder.member(
                    crate::TimberMemberRole::Sill,
                    Vec3::new(left.x, base, left.y),
                    Vec3::new(right.x, base, right.y),
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                ),
                builder.member(
                    crate::TimberMemberRole::WallPlate,
                    Vec3::new(left.x, top, left.y),
                    Vec3::new(right.x, top, right.y),
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                ),
            ]);
            // The opening jamb posts below carry the compact dormer front.
            // Do not add a second pair of full-height corner posts: aligned
            // with the facade below, those read as free columns piercing the
            // parent roof.  Continue the frame above the eave as an explicit
            // triangular gable instead.
            if let Some(ridge_y) = (dormer.kind != DormerKind::Shed)
                .then(|| {
                    roof_assemblies
                        .iter()
                        .find(|roof| roof.id == roof_id)
                        .into_iter()
                        .flat_map(|roof| roof.faces.iter())
                        .flat_map(|face| face.polygon.iter())
                        .map(|point| point.y)
                        .max_by(f32::total_cmp)
                })
                .flatten()
            {
                let apex = Vec3::new(plane.x, ridge_y, plane.y);
                let eave_centre = Vec3::new(plane.x, top, plane.y);
                member_ids.extend([
                    builder.member(
                        crate::TimberMemberRole::GablePost,
                        eave_centre,
                        apex,
                        section * 0.72,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rafter,
                        Vec3::new(left.x, top, left.y),
                        apex,
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rafter,
                        Vec3::new(right.x, top, right.y),
                        apex,
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                ]);
            }
            if let Some(opening) = opening
                && let Some(void_bounds) = builder
                    .geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
                    .map(|void| void.bounds)
            {
                let opening_half = opening.profile.interior_width_metres() * 0.5;
                let left_jamb = plane - tangent * opening_half;
                let right_jamb = plane + tangent * opening_half;
                member_ids.extend([
                    builder.member(
                        crate::TimberMemberRole::IntermediatePost,
                        Vec3::new(left_jamb.x, base, left_jamb.y),
                        Vec3::new(left_jamb.x, top, left_jamb.y),
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::IntermediatePost,
                        Vec3::new(right_jamb.x, base, right_jamb.y),
                        Vec3::new(right_jamb.x, top, right_jamb.y),
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rail,
                        Vec3::new(left_jamb.x, void_bounds.min.y, left_jamb.y),
                        Vec3::new(right_jamb.x, void_bounds.min.y, right_jamb.y),
                        section * 0.75,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rail,
                        Vec3::new(left_jamb.x, void_bounds.max.y, left_jamb.y),
                        Vec3::new(right_jamb.x, void_bounds.max.y, right_jamb.y),
                        section * 0.85,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                ]);
            }
            member_ids.sort_unstable();
            member_ids.dedup();
            let bay_id = crate::TimberFrameBayId(next_bay);
            next_bay += 1;
            bays.push(crate::TimberFrameBay {
                id: bay_id,
                wall: Some(wall.id),
                opening: opening.map(|opening| opening.id),
                member_ids,
                infill_solids: wall.host_solids.clone(),
            });
        }
    }

    // Replace monolithic Stage 3 WallHost leaves with bay-local infill
    // panels. Opening jamb/head/sill/spandrel solids retain their independent
    // bearing authority; these residual panels cover only the wall field
    // around the opening and sit behind the structural timber layer.
    let mut removed_panel_ids = std::collections::HashSet::new();
    for wall in walls
        .iter_mut()
        .filter(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
    {
        let old_panels = wall
            .host_solids
            .iter()
            .copied()
            .filter(|id| {
                builder
                    .geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::WallHost)
            })
            .collect::<Vec<_>>();
        removed_panel_ids.extend(old_panels.iter().copied());
        wall.host_solids.retain(|id| !old_panels.contains(id));

        let half_length = wall.length_metres * 0.5;
        let field = closed_polygon([
            Vec2::new(-half_length, 0.0),
            Vec2::new(half_length, 0.0),
            Vec2::new(half_length, wall.height_metres),
            Vec2::new(-half_length, wall.height_metres),
        ]);
        let mut residual = MultiPolygon(vec![field]);
        for opening in openings
            .iter()
            .filter(|opening| opening.host_wall == wall.id)
        {
            let half_opening =
                (opening.profile.interior_width_metres() * 0.5).min(half_length - 0.02);
            let centre = (opening.frame.origin - wall.frame.origin).dot(wall.frame.tangent);
            let sill = (opening.sill_elevation_metres - wall.base_elevation_metres)
                .clamp(0.0, wall.height_metres);
            let head =
                (sill + opening.profile.clear_height_metres()).clamp(sill, wall.height_metres);
            let opening_polygon = closed_polygon([
                Vec2::new(centre - half_opening, sill),
                Vec2::new(centre + half_opening, sill),
                Vec2::new(centre + half_opening, head),
                Vec2::new(centre - half_opening, head),
            ]);
            residual = residual.difference(&opening_polygon);
        }
        let wall_member_ids = bays
            .iter()
            .filter(|bay| bay.wall == Some(wall.id))
            .flat_map(|bay| bay.member_ids.iter().copied())
            .collect::<std::collections::HashSet<_>>();
        for member in builder
            .members
            .iter()
            .filter(|member| wall_member_ids.contains(&member.id))
        {
            residual = residual.difference(&timber_member_wall_polygon(member, wall));
        }

        let panel_depth = (wall.thickness_metres - section.y).max(0.04);
        // Stage 3 opening-bearing solids retain the structural wall depth, but
        // their exposed face is recessed from the Fachwerk plane. Their exact
        // overlap with the opening's jamb/header members is a typed composite
        // opening-frame relation audited below; unrelated timber receives no
        // such permission.
        let opening_recess = 0.012_f32.min(wall.thickness_metres - 0.04);
        let inward = -wall.frame.outward;
        for solid in builder.geometry.solids.iter_mut().filter(|solid| {
            wall.host_solids.contains(&solid.id)
                && matches!(
                    solid.role,
                    SolidRole::OpeningJamb
                        | SolidRole::OpeningSill
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                )
        }) {
            solid.centre += Vec3::new(inward.x, 0.0, inward.y) * opening_recess * 0.5;
            if wall.frame.outward.x.abs() > 0.5 {
                solid.size.x = (solid.size.x - opening_recess).max(0.04);
            } else {
                solid.size.z = (solid.size.z - opening_recess).max(0.04);
            }
        }
        let mut panel_ids = Vec::new();
        let triangles = residual
            .0
            .iter()
            .flat_map(triangulate_panel_polygon)
            .collect::<Vec<_>>();
        for (index, triangle) in triangles.into_iter().enumerate() {
            let id = ResolvedItemId(
                (1_u64 << 60) | (u64::from(wall.owner.0) << 32) | 0x0f00_0000 | index as u64,
            );
            let contact = ResolvedItemId(
                (4_u64 << 60) | (u64::from(wall.owner.0) << 32) | 0x0f00_0000 | index as u64,
            );
            let mid_plane = wall.frame.origin - wall.frame.outward * (section.y * 0.5);
            let vertices = triangle.map(|point| {
                let plan = mid_plane + wall.frame.tangent * point.x;
                Vec3::new(plan.x, wall.base_elevation_metres + point.y, plan.y)
            });
            let depth_offset =
                Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y) * panel_depth * 0.5;
            let min = vertices
                .iter()
                .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = vertices
                .iter()
                .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            let centre = (min + max) * 0.5;
            let size = max - min;
            builder.geometry.solids.push(ResolvedSolid {
                id,
                owner: wall.owner,
                centre,
                size,
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::WallHost,
                shape: crate::ResolvedSolidShape::TimberPanelPrism {
                    vertices,
                    outward: wall.frame.outward,
                    depth_metres: panel_depth,
                },
                supported_by: vec![wall.support_node],
            });
            builder.geometry.support_interfaces.push(SupportInterface {
                id: contact,
                owner: wall.owner,
                node: wall.support_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        centre.x - size.x * 0.5,
                        centre.y - size.y * 0.5 - 0.004,
                        centre.z - size.z * 0.5,
                    ),
                    max: Vec3::new(
                        centre.x + size.x * 0.5,
                        centre.y - size.y * 0.5 + 0.008,
                        centre.z + size.z * 0.5,
                    ),
                },
            });
            wall.host_solids.push(id);
            panel_ids.push(id);
        }
        for bay in bays.iter_mut().filter(|bay| bay.wall == Some(wall.id)) {
            bay.infill_solids = panel_ids.clone();
        }
    }
    builder
        .geometry
        .solids
        .retain(|solid| !removed_panel_ids.contains(&solid.id));

    let (preferred_stair_origin, preferred_stair_axis, stair_width, stair_run) = stairs
        .iter()
        .find_map(|stair| match *stair {
            Stair::Straight {
                start,
                direction,
                width_metres,
                run_metres,
                ..
            } => Some((
                start,
                direction_vector(direction),
                // The opening includes the two stringers outside the one
                // metre occupant prism. `stair_width` is the structural cut;
                // the route/void below remains the clear one-metre core.
                width_metres.max(1.0) + 0.36,
                run_metres.min(dimensions.max_element() - 1.0),
            )),
            Stair::Spiral { .. } => None,
        })
        .unwrap_or((
            Vec2::new(dimensions.x * 0.5, dimensions.y * 0.5 - 2.1),
            Vec2::Y,
            1.36,
            (dimensions.y - 1.0).clamp(2.8, 4.2),
        ));
    let collect_wall_bounds = |ground_route_only: bool| {
        walls
            .iter()
            .flat_map(|wall| &wall.host_solids)
            .filter_map(|id| {
                builder
                    .geometry
                    .solids
                    .iter()
                    .find(|solid| {
                        solid.id == *id
                            && (!ground_route_only
                                || (solid.centre.y - solid.size.y * 0.5 < 1.90
                                    && solid.centre.y + solid.size.y * 0.5 > 0.02))
                    })
                    .map(|solid| {
                        let cosine = solid.yaw_radians.cos().abs();
                        let sine = solid.yaw_radians.sin().abs();
                        let half = Vec3::new(
                            (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                            solid.size.y * 0.5,
                            (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                        );
                        let min = solid.centre - half;
                        let max = solid.centre + half;
                        (Vec2::new(min.x, min.z), Vec2::new(max.x, max.z))
                    })
            })
            .collect::<Vec<_>>()
    };
    let stair_wall_bounds = collect_wall_bounds(false);
    let ground_route_wall_bounds = collect_wall_bounds(true);
    let mut stair_candidates = vec![(preferred_stair_origin, preferred_stair_axis)];
    for z in 1..((dimensions.y / CELL_SIZE_METRES) as i32) {
        for x in 1..((dimensions.x / CELL_SIZE_METRES) as i32) {
            let origin = Vec2::new(
                (x as f32 + 0.5) * CELL_SIZE_METRES,
                (z as f32 + 0.5) * CELL_SIZE_METRES,
            );
            for axis in [Vec2::Y, Vec2::X, -Vec2::Y, -Vec2::X] {
                stair_candidates.push((origin, axis));
            }
        }
    }
    stair_candidates.sort_by(|left, right| {
        left.0
            .distance(preferred_stair_origin)
            .total_cmp(&right.0.distance(preferred_stair_origin))
    });
    let candidate_is_clear = |origin: Vec2, axis: Vec2| {
        let end = origin + axis * stair_run;
        let lateral = Vec2::new(-axis.y, axis.x);
        let side = lateral * stair_width * 0.5;
        let min = (origin - side)
            .min(origin + side)
            .min(end - side)
            .min(end + side);
        let max = (origin - side)
            .max(origin + side)
            .max(end - side)
            .max(end + side);
        min.cmpge(Vec2::splat(0.20)).all()
            && max.cmple(dimensions - Vec2::splat(0.20)).all()
            && stair_wall_bounds.iter().all(|(wall_min, wall_max)| {
                max.x <= wall_min.x + 0.01
                    || min.x >= wall_max.x - 0.01
                    || max.y <= wall_min.y + 0.01
                    || min.y >= wall_max.y - 0.01
            })
    };
    let programme_owns_stair = program.vertical_connections.iter().any(|connection| {
        matches!(
            connection,
            VerticalConnectionRequirement::StraightStair { .. }
        )
    });
    let (stair_origin, stair_axis) = if programme_owns_stair {
        debug_assert!(candidate_is_clear(
            preferred_stair_origin,
            preferred_stair_axis
        ));
        (preferred_stair_origin, preferred_stair_axis)
    } else {
        stair_candidates
            .into_iter()
            .find(|(origin, axis)| candidate_is_clear(*origin, *axis))
            .unwrap_or((preferred_stair_origin, preferred_stair_axis))
    };
    // The programme-to-grid solver owns the core. This assembly expands that
    // authority into alternating physical flights without choosing a new site.
    for (straight_flight_index, stair) in stairs
        .iter_mut()
        .filter(|stair| matches!(stair, Stair::Straight { .. }))
        .enumerate()
    {
        let ascending_forward = straight_flight_index.is_multiple_of(2);
        let Stair::Straight {
            start,
            direction,
            base_height_metres: _,
            rise_metres,
            width_metres,
            tread_count,
            run_metres,
        } = stair
        else {
            unreachable!();
        };
        *start = if ascending_forward {
            stair_origin
        } else {
            stair_origin + stair_axis * stair_run
        };
        *direction = cardinal_direction(if ascending_forward {
            stair_axis
        } else {
            -stair_axis
        });
        *rise_metres = program.storey_height_metres;
        *width_metres = 1.0;
        *tread_count = 18;
        *run_metres = stair_run;
    }
    let stair_lateral = Vec2::new(-stair_axis.y, stair_axis.x);
    let stair_end = stair_origin + stair_axis * stair_run;
    let side = stair_lateral * stair_width * 0.5;
    let stair_min = (stair_origin - side)
        .min(stair_origin + side)
        .min(stair_end - side)
        .min(stair_end + side);
    let stair_max = (stair_origin - side)
        .max(stair_origin + side)
        .max(stair_end - side)
        .max(stair_end + side);
    let stair_min = stair_min.max(Vec2::splat(0.20));
    let stair_max = stair_max.min(dimensions - Vec2::splat(0.20));
    let stair_floor_cut = |level: u16| {
        let (flight_origin, flight_axis) = if level % 2 == 1 {
            (stair_origin, stair_axis)
        } else {
            (stair_end, -stair_axis)
        };
        let flight_lateral = Vec2::new(-flight_axis.y, flight_axis.x);
        let flight_end = flight_origin + flight_axis * stair_run;
        let clear_side = flight_lateral * 0.50;
        // The floor must be removed from the point where a person ascending
        // the flight first needs 1.90 m of headroom through the landing.
        // The old 0.19 m slot at the final tread satisfied bookkeeping but
        // left the physical circulation envelope buried in the upper floor.
        let clearance_start = ((program.storey_height_metres - 1.90)
            / program.storey_height_metres.max(0.001)
            * stair_run
            - 0.30)
            .clamp(0.0, stair_run);
        let cut_inner = flight_origin + flight_axis * clearance_start;
        let cut_outer = flight_end + flight_axis * 0.30;
        let clear_min = (cut_inner - clear_side)
            .min(cut_inner + clear_side)
            .min(cut_outer - clear_side)
            .min(cut_outer + clear_side);
        let clear_max = (cut_inner - clear_side)
            .max(cut_inner + clear_side)
            .max(cut_outer - clear_side)
            .max(cut_outer + clear_side);
        (clear_min, clear_max)
    };

    let mut floors = Vec::new();
    for level in 0..program.storeys.len() as u16 {
        let base = f32::from(level) * program.storey_height_metres;
        let mut girder_members = Vec::new();
        let mut joist_members = Vec::new();
        let mut bearing_interfaces = Vec::new();
        let mut floor_joist_interfaces = Vec::new();
        let mut joist_girder_interfaces = Vec::new();
        let joist_count = (dimensions.x / 1.35).ceil().max(2.0) as usize;
        let mut x_stations = (0..=joist_count)
            .map(|index| 0.20 + (dimensions.x - 0.40) * index as f32 / joist_count as f32)
            .collect::<Vec<_>>();
        let cut_bounds = (level > 0).then(|| stair_floor_cut(level));
        if let Some((cut_min, cut_max)) = cut_bounds {
            x_stations.extend([cut_min.x, cut_max.x]);
            x_stations.sort_by(f32::total_cmp);
            x_stations.dedup_by(|left, right| (*left - *right).abs() < 0.08);
        }
        let mut upper_girder_z = dimensions.y * 0.67;
        if (upper_girder_z - stair_end.y).abs() < 0.40 {
            upper_girder_z = (stair_end.y + 0.40).min(dimensions.y - 0.40);
        }
        let support_z_stations = [
            0.20_f32,
            dimensions.y * 0.33,
            upper_girder_z,
            dimensions.y - 0.20,
        ];
        let mut girder_z_stations = vec![support_z_stations[1], support_z_stations[2]];
        if level > 0 {
            girder_z_stations.extend([stair_min.y, stair_max.y]);
            girder_z_stations.sort_by(f32::total_cmp);
            girder_z_stations.dedup_by(|left, right| (*left - *right).abs() < 0.08);
        }
        let mut joist_z_stations = support_z_stations.to_vec();
        if level > 0 {
            joist_z_stations.extend([stair_min.y, stair_max.y]);
            joist_z_stations.sort_by(f32::total_cmp);
            joist_z_stations.dedup_by(|left, right| (*left - *right).abs() < 0.08);
        }
        let joist_section = section * 0.90;
        let girder_section = Vec2::new(section.x * 1.35, section.y * 1.20);
        let bearing_y = base - 0.16 - joist_section.y * 0.5;
        let mut floor_supports = Vec::new();
        if level > 0 {
            // Split both orthogonal member families at every crossing. Their
            // shared structural node is therefore a physical housed bearing,
            // not an interface floating at a member midpoint.
            for z in &girder_z_stations {
                for pair in x_stations.windows(2) {
                    if cut_bounds.is_some_and(|(cut_min, cut_max)| {
                        *z > cut_min.y - girder_section.x * 0.5
                            && *z < cut_max.y + girder_section.x * 0.5
                            && (pair[0] + pair[1]) * 0.5 > cut_min.x
                            && (pair[0] + pair[1]) * 0.5 < cut_max.x
                    }) {
                        continue;
                    }
                    girder_members.push(builder.member(
                        crate::TimberMemberRole::Girder,
                        Vec3::new(pair[0], bearing_y, *z),
                        Vec3::new(pair[1], bearing_y, *z),
                        girder_section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                }
                for x in [x_stations[0], *x_stations.last().expect("floor x station")] {
                    let lower_y = if level == 1 {
                        0.0
                    } else {
                        f32::from(level - 1) * program.storey_height_metres
                            - 0.16
                            - joist_section.y * 0.5
                    };
                    builder.member(
                        crate::TimberMemberRole::PrimaryPost,
                        Vec3::new(x, lower_y, *z),
                        Vec3::new(x, bearing_y, *z),
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    );
                }
            }
            for x in &x_stations {
                for pair in joist_z_stations.windows(2) {
                    let midpoint = Vec2::new(*x, (pair[0] + pair[1]) * 0.5);
                    if midpoint.x > stair_min.x + 0.001
                        && midpoint.x < stair_max.x - 0.001
                        && midpoint.y > stair_min.y + 0.001
                        && midpoint.y < stair_max.y - 0.001
                    {
                        continue;
                    }
                    joist_members.push(builder.member(
                        crate::TimberMemberRole::FloorJoist,
                        Vec3::new(*x, bearing_y, pair[0]),
                        Vec3::new(*x, bearing_y, pair[1]),
                        joist_section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                }
                for z in &girder_z_stations {
                    let z = *z;
                    let at = Vec3::new(*x, bearing_y, z);
                    let point_on = |member: &crate::TimberFrameMember| {
                        let axis = member.end - member.start;
                        let t = (at - member.start).dot(axis) / axis.length_squared().max(0.0001);
                        (-0.001..=1.001).contains(&t)
                            && member
                                .start
                                .lerp(member.end, t.clamp(0.0, 1.0))
                                .distance(at)
                                <= 0.003
                    };
                    if !joist_members.iter().any(|id| {
                        builder
                            .members
                            .iter()
                            .find(|member| member.id == *id)
                            .is_some_and(point_on)
                    }) || !girder_members.iter().any(|id| {
                        builder
                            .members
                            .iter()
                            .find(|member| member.id == *id)
                            .is_some_and(point_on)
                    }) {
                        continue;
                    }
                    let node = builder.node(Vec3::new(*x, bearing_y, z));
                    floor_supports.push(node);
                    let housed = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x390_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: housed,
                        owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                *x - joist_section.x * 0.5,
                                bearing_y - joist_section.y * 0.5,
                                z - girder_section.x * 0.5,
                            ),
                            max: Vec3::new(
                                *x + joist_section.x * 0.5,
                                bearing_y + joist_section.y * 0.5,
                                z + girder_section.x * 0.5,
                            ),
                        },
                    });
                    joist_girder_interfaces.push(housed);
                    if *x >= stair_min.x - 0.001
                        && *x <= stair_max.x + 0.001
                        && z >= stair_min.y - 0.001
                        && z <= stair_max.y + 0.001
                    {
                        bearing_interfaces.push(housed);
                        continue;
                    }
                    let floor_contact = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x3a0_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    let contact_y = base - 0.16;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: floor_contact,
                        owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(*x - 0.055, contact_y - 0.004, z - 0.16),
                            max: Vec3::new(*x + 0.055, contact_y + 0.004, z + 0.16),
                        },
                    });
                    floor_joist_interfaces.push(floor_contact);
                    bearing_interfaces.extend([housed, floor_contact]);
                }
            }
        } else {
            // The slab's ground bearing is not a timber joint. Keep it out of
            // the member/joint registry so the structural graph does not
            // invent an empty mortise at the centre of the room.
            let ground_node = StructuralNodeId(builder.next_node);
            builder.next_node += 1;
            builder.geometry.structural_nodes.push(StructuralNode {
                id: ground_node,
                owner,
                kind: StructuralNodeKind::TimberFrameFoundation,
                position: Vec3::new(dimensions.x * 0.5, 0.0, dimensions.y * 0.5),
                supported_by: Vec::new(),
                grounded: true,
            });
            floor_supports.push(ground_node);
            let ground_bearing = ResolvedItemId(
                (4_u64 << 60) | (u64::from(owner.0) << 32) | 0x3b0_000 | builder.next_interface,
            );
            builder.next_interface += 1;
            builder.geometry.support_interfaces.push(SupportInterface {
                id: ground_bearing,
                owner,
                node: ground_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(dimensions.x * 0.5 - 0.25, -0.005, dimensions.y * 0.5 - 0.25),
                    max: Vec3::new(dimensions.x * 0.5 + 0.25, 0.005, dimensions.y * 0.5 + 0.25),
                },
            });
            bearing_interfaces.push(ground_bearing);
        }
        floor_supports.sort_unstable();
        floor_supports.dedup();
        let floor_solid = ResolvedItemId(
            (1_u64 << 60) | (u64::from(owner.0) << 32) | 0x0e00_0000 | u64::from(level + 1),
        );
        let floor_centre_y = base - 0.08;
        let floor_rects = if level == 0 {
            vec![(Vec2::splat(0.15), dimensions - Vec2::splat(0.15))]
        } else {
            let (cut_min, cut_max) = cut_bounds.expect("upper timber floor has stair cut");
            vec![
                (
                    Vec2::new(0.15, 0.15),
                    Vec2::new(cut_min.x, dimensions.y - 0.15),
                ),
                (
                    Vec2::new(cut_max.x, 0.15),
                    Vec2::new(dimensions.x - 0.15, dimensions.y - 0.15),
                ),
                (Vec2::new(cut_min.x, 0.15), Vec2::new(cut_max.x, cut_min.y)),
                (
                    Vec2::new(cut_min.x, cut_max.y),
                    Vec2::new(cut_max.x, dimensions.y - 0.15),
                ),
            ]
        };
        let mut floor_solids = Vec::new();
        for (index, (min, max)) in floor_rects.into_iter().enumerate() {
            if (max - min).min_element() <= 0.05 {
                continue;
            }
            let id = if index == 0 {
                floor_solid
            } else {
                ResolvedItemId(floor_solid.0 | ((index as u64) << 12))
            };
            floor_solids.push(id);
            let mut piece_supports = if level == 0 {
                floor_supports.clone()
            } else {
                floor_joist_interfaces
                    .iter()
                    .filter_map(|id| {
                        builder
                            .geometry
                            .support_interfaces
                            .iter()
                            .find(|interface| interface.id == *id)
                    })
                    .filter(|interface| {
                        let centre = (interface.bounds.min + interface.bounds.max) * 0.5;
                        centre.x >= min.x - 0.001
                            && centre.x <= max.x + 0.001
                            && centre.z >= min.y - 0.001
                            && centre.z <= max.y + 0.001
                    })
                    .map(|interface| interface.node)
                    .collect::<Vec<_>>()
            };
            if level > 0 && piece_supports.is_empty() {
                let endpoint = joist_members.iter().find_map(|id| {
                    let member = builder.members.iter().find(|member| member.id == *id)?;
                    [
                        (member.start_node, member.start),
                        (member.end_node, member.end),
                    ]
                    .into_iter()
                    .find(|(_, point)| {
                        point.x >= min.x - 0.001
                            && point.x <= max.x + 0.001
                            && point.z >= min.y - 0.001
                            && point.z <= max.y + 0.001
                    })
                });
                if let Some((node, point)) = endpoint {
                    let contact = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x3d0_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: contact,
                        owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(point.x - 0.06, base - 0.164, point.z - 0.06),
                            max: Vec3::new(point.x + 0.06, base - 0.156, point.z + 0.06),
                        },
                    });
                    floor_joist_interfaces.push(contact);
                    bearing_interfaces.push(contact);
                    piece_supports.push(node);
                }
            }
            builder.geometry.solids.push(ResolvedSolid {
                id,
                owner,
                centre: Vec3::new((min.x + max.x) * 0.5, floor_centre_y, (min.y + max.y) * 0.5),
                size: Vec3::new(max.x - min.x, 0.16, max.y - min.y),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::FrameFloor,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: piece_supports,
            });
        }
        let route_surface = ResolvedItemId(
            (2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0e00_0000 | u64::from(level + 1),
        );
        builder.geometry.surfaces.push(ResolvedSurface {
            id: route_surface,
            owner,
            bounds: ResolvedBounds {
                min: Vec3::new(0.15, base + 0.001, 0.15),
                max: Vec3::new(dimensions.x - 0.15, base + 0.011, dimensions.y - 0.15),
            },
            role: SurfaceRole::TimberCirculation,
            shape: crate::ResolvedSurfaceShape::Planar,
        });
        floors.push(crate::TimberFloorAssembly {
            level,
            floor_solid,
            floor_solids,
            route_surface,
            girder_members,
            joist_members,
            bearing_interfaces,
            floor_joist_interfaces,
            joist_girder_interfaces,
            stair_connection: (level > 0).then(|| {
                stairs
                    .first()
                    .map_or(
                        Vec2::new(dimensions.x * 0.5, dimensions.y * 0.5),
                        |stair| match *stair {
                            Stair::Straight { start, .. } => start,
                            Stair::Spiral { centre, .. } => centre,
                        },
                    )
            }),
        });
    }

    let entry_opening = openings.iter().find(|opening| {
        opening.use_kind == crate::OpeningUse::Door
            && opening
                .closure
                .layers
                .contains(&crate::ClosureKind::DoorLeaf)
            && opening.sill_elevation_metres <= 0.02
    });
    let mut circulation_nodes = Vec::new();
    let mut circulation_edges = Vec::new();
    let mut circulation_stair_solids = Vec::new();
    let mut circulation_landings = Vec::new();
    let mut floor_cut_voids = Vec::new();
    let mut ground_route_position = Vec2::new(dimensions.x * 0.5, dimensions.y * 0.5);
    let mut previous_surface = floors
        .first()
        .map(|floor| floor.route_surface)
        .expect("timber program has a ground floor");
    if let Some(opening) = entry_opening {
        let approach = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0d00_0001);
        let threshold = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0d00_0002);
        let vestibule = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0d00_0003);
        let approach_centre = opening.frame.origin + opening.frame.outward * 0.75;
        let threshold_centre = opening.frame.origin;
        let vestibule_centre = opening.frame.origin - opening.frame.outward * 0.75;
        let entry_route_clearance_metres = 0.10_f32;
        ground_route_position =
            vestibule_centre - opening.frame.outward * entry_route_clearance_metres;
        for (id, centre, depth) in [
            (approach, approach_centre, 0.90_f32),
            (threshold, threshold_centre, 0.35_f32),
            (vestibule, vestibule_centre, 0.90_f32),
        ] {
            builder.geometry.surfaces.push(ResolvedSurface {
                id,
                owner,
                bounds: ResolvedBounds {
                    min: Vec3::new(centre.x - 0.50, 0.001, centre.y - depth * 0.5),
                    max: Vec3::new(centre.x + 0.50, 0.011, centre.y + depth * 0.5),
                },
                role: SurfaceRole::TimberCirculation,
                shape: crate::ResolvedSurfaceShape::Planar,
            });
            circulation_nodes.push(crate::TimberRouteNode {
                surface: id,
                kind: if id == approach {
                    crate::TimberRouteNodeKind::ExteriorApproach
                } else if id == threshold {
                    crate::TimberRouteNodeKind::DoorThreshold
                } else {
                    crate::TimberRouteNodeKind::Landing
                },
                position: Vec3::new(centre.x, 0.01, centre.y),
                level: 0,
            });
        }
        circulation_edges.extend([
            crate::TimberRouteEdge {
                from: approach,
                to: threshold,
                clear_width_metres: 0.90,
                clear_headroom_metres: 2.05,
            },
            crate::TimberRouteEdge {
                from: threshold,
                to: vestibule,
                clear_width_metres: 0.90,
                clear_headroom_metres: 2.05,
            },
            crate::TimberRouteEdge {
                from: vestibule,
                to: previous_surface,
                clear_width_metres: 0.90,
                clear_headroom_metres: 2.05,
            },
        ]);
    }
    circulation_nodes.push(crate::TimberRouteNode {
        surface: previous_surface,
        kind: crate::TimberRouteNodeKind::GroundFloor,
        position: Vec3::new(ground_route_position.x, 0.01, ground_route_position.y),
        level: 0,
    });
    // Route from the entry vestibule to the stair on a quarter-metre lattice,
    // inflating actual internal wall panels by half the 0.90 m occupant width.
    // This avoids the former diagonal shortcut through room partitions while
    // remaining a deliberately compact civilian circulation vocabulary.
    let route_step = 0.05_f32;
    let route_margin = 0.45_f32;
    let nx = ((dimensions.x - route_margin * 2.0) / route_step).floor() as i32;
    let nz = ((dimensions.y - route_margin * 2.0) / route_step).floor() as i32;
    let to_cell = |point: Vec2| {
        (
            ((point.x - route_margin) / route_step)
                .round()
                .clamp(0.0, nx as f32) as i32,
            ((point.y - route_margin) / route_step)
                .round()
                .clamp(0.0, nz as f32) as i32,
        )
    };
    let to_point = |cell: (i32, i32)| {
        Vec2::new(
            route_margin + cell.0 as f32 * route_step,
            route_margin + cell.1 as f32 * route_step,
        )
    };
    let route_start = to_cell(ground_route_position);
    let route_goal = to_cell(stair_origin);
    let blocked = |cell: (i32, i32)| {
        let point = to_point(cell);
        ground_route_wall_bounds.iter().any(|(min, max)| {
            point.x > min.x - route_margin
                && point.x < max.x + route_margin
                && point.y > min.y - route_margin
                && point.y < max.y + route_margin
        })
    };
    let mut frontier = std::collections::VecDeque::from([route_start]);
    let mut came_from = std::collections::HashMap::from([(route_start, route_start)]);
    while let Some(current) = frontier.pop_front() {
        if current == route_goal {
            break;
        }
        for next in [
            (current.0 + 1, current.1),
            (current.0 - 1, current.1),
            (current.0, current.1 + 1),
            (current.0, current.1 - 1),
        ] {
            if next.0 < 0
                || next.1 < 0
                || next.0 > nx
                || next.1 > nz
                || came_from.contains_key(&next)
                || (next != route_goal && next != route_start && blocked(next))
            {
                continue;
            }
            came_from.insert(next, current);
            frontier.push_back(next);
        }
    }
    let mut route_cells = Vec::new();
    if came_from.contains_key(&route_goal) {
        let mut cursor = route_goal;
        route_cells.push(cursor);
        while cursor != route_start {
            cursor = came_from[&cursor];
            route_cells.push(cursor);
        }
        route_cells.reverse();
    }
    let mut route_points = vec![ground_route_position];
    if to_point(route_start).distance(ground_route_position) > 0.03 {
        route_points.push(to_point(route_start));
    }
    for index in 1..route_cells.len() {
        let direction = (
            route_cells[index].0 - route_cells[index - 1].0,
            route_cells[index].1 - route_cells[index - 1].1,
        );
        let next_direction = route_cells
            .get(index + 1)
            .map(|next| (next.0 - route_cells[index].0, next.1 - route_cells[index].1));
        if next_direction != Some(direction) {
            route_points.push(to_point(route_cells[index]));
        }
    }
    if route_points
        .last()
        .is_none_or(|point| point.distance(stair_origin) > 0.03)
    {
        route_points.push(stair_origin);
    }
    for (index, point) in route_points.into_iter().skip(1).enumerate() {
        let surface =
            ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0b00_0000 | index as u64);
        builder.geometry.surfaces.push(ResolvedSurface {
            id: surface,
            owner,
            bounds: ResolvedBounds {
                min: Vec3::new(point.x - 0.45, 0.001, point.y - 0.45),
                max: Vec3::new(point.x + 0.45, 0.011, point.y + 0.45),
            },
            role: SurfaceRole::TimberCirculation,
            shape: crate::ResolvedSurfaceShape::Planar,
        });
        circulation_nodes.push(crate::TimberRouteNode {
            surface,
            kind: crate::TimberRouteNodeKind::Landing,
            position: Vec3::new(point.x, 0.01, point.y),
            level: 0,
        });
        circulation_edges.push(crate::TimberRouteEdge {
            from: previous_surface,
            to: surface,
            clear_width_metres: 0.90,
            clear_headroom_metres: 2.05,
        });
        previous_surface = surface;
    }
    for level in 1..program.storeys.len() as u16 {
        let lower_y = f32::from(level - 1) * program.storey_height_metres;
        let upper_y = f32::from(level) * program.storey_height_metres;
        let tread_count = 18_u64;
        let going = stair_run / tread_count as f32;
        let rise = (upper_y - lower_y) / tread_count as f32;
        let (flight_origin, flight_axis) = if level % 2 == 1 {
            (stair_origin, stair_axis)
        } else {
            (stair_end, -stair_axis)
        };
        let flight_lateral = Vec2::new(-flight_axis.y, flight_axis.x);
        // Split each stringer at every tread bearing. This makes every tread
        // support an actual member endpoint/contact instead of a synthetic
        // node placed near the middle of a diagonal member.
        for side in [-1.0_f32, 1.0] {
            let lateral = flight_lateral * (side * (stair_width * 0.5 - section.x * 0.5));
            for tread in 0..tread_count {
                let start_plan = flight_origin + flight_axis * (going * tread as f32) + lateral;
                let end_plan = flight_origin + flight_axis * (going * (tread + 1) as f32) + lateral;
                builder.member(
                    crate::TimberMemberRole::Girder,
                    Vec3::new(
                        start_plan.x,
                        lower_y + rise * tread as f32 - 0.03,
                        start_plan.y,
                    ),
                    Vec3::new(
                        end_plan.x,
                        lower_y + rise * (tread + 1) as f32 - 0.03,
                        end_plan.y,
                    ),
                    section * 0.90,
                    crate::TimberFramePhase::PrimaryConstruction,
                );
            }
        }
        // The upper floor itself is the eighteenth landing; do not place a
        // duplicate tread inside its subtraction prism.
        for tread in 0..(tread_count - 1) {
            let y = lower_y + rise * (tread + 1) as f32;
            let plan = flight_origin + flight_axis * (going * (tread + 1) as f32);
            let solid_id = ResolvedItemId(
                (1_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | 0x0c00_0000
                    | (u64::from(level) << 8)
                    | tread,
            );
            let surface_id = ResolvedItemId(
                (2_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | 0x0c00_0000
                    | (u64::from(level) << 8)
                    | tread,
            );
            let support_nodes = [-1.0_f32, 1.0]
                .map(|side| {
                    builder.node(Vec3::new(
                        plan.x + flight_lateral.x * side * (stair_width * 0.5 - section.x * 0.5),
                        y - 0.03,
                        plan.y + flight_lateral.y * side * (stair_width * 0.5 - section.x * 0.5),
                    ))
                })
                .to_vec();
            for node in &support_nodes {
                let bearing = ResolvedItemId(
                    (4_u64 << 60) | (u64::from(owner.0) << 32) | 0x3c0_000 | builder.next_interface,
                );
                builder.next_interface += 1;
                let node_position = builder
                    .geometry
                    .structural_nodes
                    .iter()
                    .find(|candidate| candidate.id == *node)
                    .expect("stair bearing node exists")
                    .position;
                builder.geometry.support_interfaces.push(SupportInterface {
                    id: bearing,
                    owner,
                    node: *node,
                    bounds: ResolvedBounds {
                        min: node_position - Vec3::new(0.18, 0.025, 0.18),
                        max: node_position + Vec3::new(0.18, 0.035, 0.18),
                    },
                });
            }
            builder.geometry.solids.push(ResolvedSolid {
                id: solid_id,
                owner,
                centre: Vec3::new(plan.x, y - 0.025, plan.y),
                size: Vec3::new(1.0, 0.05, going * 0.96),
                yaw_radians: flight_axis.y.atan2(flight_axis.x) - std::f32::consts::FRAC_PI_2,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::Landing,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: support_nodes,
            });
            builder.geometry.surfaces.push(ResolvedSurface {
                id: surface_id,
                owner,
                bounds: ResolvedBounds {
                    min: Vec3::new(plan.x - 0.50, y, plan.y - going * 0.48),
                    max: Vec3::new(plan.x + 0.50, y + 0.01, plan.y + going * 0.48),
                },
                role: SurfaceRole::TimberCirculation,
                shape: crate::ResolvedSurfaceShape::Planar,
            });
            circulation_nodes.push(crate::TimberRouteNode {
                surface: surface_id,
                kind: crate::TimberRouteNodeKind::StairTread,
                position: Vec3::new(plan.x, y, plan.y),
                level,
            });
            circulation_edges.push(crate::TimberRouteEdge {
                from: previous_surface,
                to: surface_id,
                clear_width_metres: 1.0,
                clear_headroom_metres: 2.05,
            });
            previous_surface = surface_id;
            circulation_stair_solids.push(solid_id);
        }
        let floor = &floors[usize::from(level)];
        circulation_edges.push(crate::TimberRouteEdge {
            from: previous_surface,
            to: floor.route_surface,
            clear_width_metres: 1.0,
            clear_headroom_metres: 2.05,
        });
        circulation_nodes.push(crate::TimberRouteNode {
            surface: floor.route_surface,
            kind: crate::TimberRouteNodeKind::UpperFloor,
            position: Vec3::new(
                flight_origin.x + flight_axis.x * stair_run,
                upper_y + 0.01,
                flight_origin.y + flight_axis.y * stair_run,
            ),
            level,
        });
        previous_surface = floor.route_surface;
        if let Some(landing) = floor.floor_solids.iter().copied().min_by(|left, right| {
            let score = |id: &ResolvedItemId| {
                builder
                    .geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
                    .map_or(f32::INFINITY, |solid| {
                        let half = solid.size * 0.5;
                        let plan = Vec2::new(solid.centre.x, solid.centre.z);
                        let arrival = flight_origin + flight_axis * stair_run;
                        let delta = (arrival - plan).abs() - Vec2::new(half.x, half.z);
                        delta.max(Vec2::ZERO).length() * 100.0 + solid.size.x * solid.size.z
                    })
            };
            score(left).total_cmp(&score(right))
        }) {
            circulation_landings.push(landing);
        }
        let void_id = ResolvedItemId(
            (3_u64 << 60) | (u64::from(owner.0) << 32) | 0x0c00_0000 | u64::from(level),
        );
        let (clear_min, clear_max) = stair_floor_cut(level);
        builder.geometry.voids.push(ResolvedVoid {
            id: void_id,
            owner,
            bounds: ResolvedBounds {
                min: Vec3::new(clear_min.x, upper_y - 0.16, clear_min.y),
                max: Vec3::new(clear_max.x, upper_y - 0.001, clear_max.y),
            },
            role: crate::VoidRole::AccessPortal,
            shape: crate::ResolvedVoidShape::Box,
            subtracts_from: owner,
        });
        floor_cut_voids.push(void_id);
    }
    let circulation = crate::TimberCirculationAssembly {
        entry_opening: entry_opening.map(|opening| opening.id),
        nodes: circulation_nodes,
        edges: circulation_edges,
        stair_solids: circulation_stair_solids,
        landing_solids: circulation_landings,
        floor_cut_voids,
    };

    // Bind dormer curbs and child fronts to the authoritative Stage 4 roof
    // framing / Stage 3 child-wall hosts only where an endpoint interface has
    // positive physical contact. This intentionally replaces the former
    // ground-to-dormer posts, which pierced the parent roof and drainage.
    let endpoint_contacts = builder
        .members
        .iter()
        .filter(|member| {
            member.role == crate::TimberMemberRole::DormerTrimmer
                || bays.iter().any(|bay| {
                    bay.member_ids.contains(&member.id)
                        && bay.wall.is_some_and(|wall_id| {
                            walls.iter().any(|wall| {
                                wall.id == wall_id
                                    && matches!(
                                        wall.source,
                                        crate::WallSourceId::RoofChildFront { .. }
                                    )
                            })
                        })
                })
        })
        .flat_map(|member| {
            [
                (
                    member.start_node,
                    member.support_interfaces[0],
                    member.role == crate::TimberMemberRole::DormerTrimmer,
                ),
                (
                    member.end_node,
                    member.support_interfaces[1],
                    member.role == crate::TimberMemberRole::DormerTrimmer,
                ),
            ]
        })
        .collect::<Vec<_>>();
    for (node_id, interface_id, is_dormer_trimmer) in endpoint_contacts {
        let Some(interface) = builder
            .geometry
            .support_interfaces
            .iter()
            .find(|interface| interface.id == interface_id)
            .cloned()
        else {
            continue;
        };
        let overlaps = |solid: &ResolvedSolid| {
            let half = solid.size * 0.5;
            let min = solid.centre - half;
            let max = solid.centre + half;
            let overlap = interface.bounds.max.min(max) - interface.bounds.min.max(min);
            overlap.cmpgt(Vec3::splat(0.001)).all()
        };
        let mut external_supports = builder
            .geometry
            .solids
            .iter()
            .filter(|solid| {
                matches!(
                    solid.role,
                    SolidRole::RoofFace | SolidRole::RoofFraming | SolidRole::RoofPlate
                ) && overlaps(solid)
            })
            .flat_map(|solid| solid.supported_by.iter().copied())
            .collect::<Vec<_>>();
        if is_dormer_trimmer {
            let node_position = builder
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.position);
            if let Some(position) = node_position {
                let plan = Vec2::new(position.x, position.z);
                for parent_roof in roof_assemblies.iter().filter(|roof| roof.parent.is_none()) {
                    let on_parent_plane = parent_roof.faces.iter().any(|face| {
                        let outline = face
                            .polygon
                            .iter()
                            .map(|point| Vec2::new(point.x, point.z))
                            .collect::<Vec<_>>();
                        let inside_face = plan_point_in_polygon(plan, &outline)
                            && !face.cutouts.iter().any(|cutout| {
                                let cutout = cutout
                                    .iter()
                                    .map(|point| Vec2::new(point.x, point.z))
                                    .collect::<Vec<_>>();
                                plan_point_in_polygon(plan, &cutout)
                            });
                        let underside = roof_plane_height(face.plane, plan)
                            - face.plane.normal.normalize_or_zero().y * face.thickness_metres;
                        inside_face && (underside - position.y).abs() <= 0.03
                    });
                    if on_parent_plane {
                        external_supports.extend(parent_roof.support_nodes.iter().copied());
                    }
                }
            }
        }
        for wall in walls.iter().filter(|wall| {
            matches!(wall.source, crate::WallSourceId::RoofChildFront { .. })
                && wall.host_solids.iter().any(|host| {
                    builder
                        .geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == *host)
                        .is_some_and(&overlaps)
                })
        }) {
            external_supports.push(wall.support_node);
        }
        external_supports.sort_unstable();
        external_supports.dedup();
        if let Some(node) = builder
            .geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == node_id)
        {
            node.supported_by.extend(external_supports);
            node.supported_by.sort_unstable();
            node.supported_by.dedup();
        }
    }

    // Gable frame ends are inset from the exterior wall plates by the roof
    // build-up. Join that known contour offset with short, measured timber
    // seats. This is deliberately bounded to a local perpendicular projection
    // (<= 0.40 m), not the former arbitrary nearest-member search.
    let wall_plates = builder
        .members
        .iter()
        .filter(|member| member.role == crate::TimberMemberRole::WallPlate)
        .cloned()
        .collect::<Vec<_>>();
    let gable_endpoints = builder
        .members
        .iter()
        .filter(|member| member.role == crate::TimberMemberRole::GableTie)
        .flat_map(|member| [member.start, member.end])
        .collect::<Vec<_>>();
    let mut gable_seats = Vec::new();
    for endpoint in gable_endpoints {
        let endpoint_plan = Vec2::new(endpoint.x, endpoint.z);
        let candidate = wall_plates
            .iter()
            .filter_map(|plate| {
                if (plate.start.y - endpoint.y).abs() > 0.02 {
                    return None;
                }
                let start = Vec2::new(plate.start.x, plate.start.z);
                let end = Vec2::new(plate.end.x, plate.end.z);
                let axis = end - start;
                let t = (endpoint_plan - start).dot(axis) / axis.length_squared().max(0.0001);
                if !(-0.001..=1.001).contains(&t) {
                    return None;
                }
                let projected = start + axis * t.clamp(0.0, 1.0);
                let distance = projected.distance(endpoint_plan);
                (distance > 0.051 && distance <= 0.40).then_some((distance, projected))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0));
        if let Some((_, projected)) = candidate {
            gable_seats.push((Vec3::new(projected.x, endpoint.y, projected.y), endpoint));
        }
    }
    for (plate_point, gable_point) in gable_seats {
        builder.member(
            crate::TimberMemberRole::GableTie,
            plate_point,
            gable_point,
            section,
            crate::TimberFramePhase::RoofConstruction,
        );
    }

    builder.resolve_intermediate_member_bearings();
    builder.rebuild_physical_support_tree();

    let mut roof_bearing_interfaces = Vec::new();
    let mut main_roof_supports = roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_none())
        .flat_map(|roof| &roof.support_nodes)
        .filter_map(|id| {
            builder
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == *id)
                .map(|node| (*id, node.position))
        })
        .filter(|(_, position)| position.y <= top + 0.25)
        .collect::<Vec<_>>();
    main_roof_supports.sort_by(|left, right| {
        left.1
            .x
            .total_cmp(&right.1.x)
            .then(left.1.z.total_cmp(&right.1.z))
    });
    let roof_support_members = builder
        .members
        .iter()
        .filter(|member| {
            matches!(
                member.role,
                crate::TimberMemberRole::WallPlate
                    | crate::TimberMemberRole::Purlin
                    | crate::TimberMemberRole::GableTie
                    | crate::TimberMemberRole::Rafter
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let roof_seats = main_roof_supports
        .iter()
        .filter_map(|(_, position)| {
            let point = Vec2::new(position.x, position.z);
            roof_support_members
                .iter()
                .filter_map(|plate| {
                    if (plate.start.y - position.y).abs() > 0.02 {
                        return None;
                    }
                    let start = Vec2::new(plate.start.x, plate.start.z);
                    let end = Vec2::new(plate.end.x, plate.end.z);
                    let axis = end - start;
                    let t = (point - start).dot(axis) / axis.length_squared().max(0.0001);
                    if !(-0.001..=1.001).contains(&t) {
                        return None;
                    }
                    let projected = start + axis * t.clamp(0.0, 1.0);
                    let distance = projected.distance(point);
                    // The Stage 4 eave contour may overhang the plate. A
                    // short, facade-perpendicular rafter tail (project gate
                    // <= 0.90 m) supplies that bearing; longer offsets are a
                    // topology error rather than an arbitrary nearest join.
                    (distance > 0.051 && distance <= 0.90).then_some((distance, projected))
                })
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .map(|(_, projected)| (Vec3::new(projected.x, position.y, projected.y), *position))
        })
        .collect::<Vec<_>>();
    for (plate_point, roof_point) in roof_seats {
        builder.member(
            crate::TimberMemberRole::Rafter,
            plate_point,
            roof_point,
            section * 1.10,
            crate::TimberFramePhase::RoofConstruction,
        );
    }
    // Regular truss stations above own the longitudinal ridge/collar purlins.
    // Do not connect this contour list by sort order: consecutive Stage 4
    // support IDs are not necessarily neighbours in the roof topology.
    for (roof_node_id, position) in main_roof_supports {
        let bearing_node = builder.node(position);
        if let Some(node) = builder
            .geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == bearing_node)
        {
            node.kind = StructuralNodeKind::TimberFrameRoofBearing;
        }
        if let Some(roof_node) = builder
            .geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == roof_node_id)
        {
            roof_node.supported_by.push(bearing_node);
            roof_node.supported_by.sort_unstable();
            roof_node.supported_by.dedup();
        }
        let interface = ResolvedItemId(
            (4_u64 << 60) | (u64::from(owner.0) << 32) | 0x200_000 | builder.next_interface,
        );
        builder.next_interface += 1;
        builder.geometry.support_interfaces.push(SupportInterface {
            id: interface,
            owner,
            node: bearing_node,
            bounds: ResolvedBounds {
                min: position - Vec3::new(0.12, 0.08, 0.12),
                max: position + Vec3::new(0.12, 0.08, 0.12),
            },
        });
        roof_bearing_interfaces.push(interface);
    }
    // Roof contour seats may land on the interior of a continuous plate or
    // regular purlin. Resolve those measured point-on-member contacts after
    // the roof nodes exist so none remain synthetic orphan bearings.
    builder.resolve_intermediate_member_bearings();
    builder.rebuild_physical_support_tree();
    // Bind only genuinely intersecting parent/dormer framing solids to those
    // exact seats. A resolved roof item without physical contact is not
    // silently rescued by a nearby frame node.
    let bearing_samples = roof_bearing_interfaces
        .iter()
        .filter_map(|id| {
            builder
                .geometry
                .support_interfaces
                .iter()
                .find(|interface| interface.id == *id)
                .copied()
        })
        .collect::<Vec<_>>();
    let roof_plate_ids = builder
        .geometry
        .solids
        .iter()
        .filter(|solid| matches!(solid.role, SolidRole::RoofPlate | SolidRole::RoofFraming))
        .map(|solid| solid.id)
        .collect::<Vec<_>>();
    for roof_solid_id in roof_plate_ids {
        if let Some(solid) = builder
            .geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == roof_solid_id)
        {
            let half = solid.size * 0.5;
            for sample in &bearing_samples {
                let overlap = sample.bounds.max.min(solid.centre + half)
                    - sample.bounds.min.max(solid.centre - half);
                if overlap.cmpgt(Vec3::splat(0.001)).all() {
                    solid.supported_by.push(sample.node);
                }
            }
            solid.supported_by.sort_unstable();
            solid.supported_by.dedup();
        }
    }

    let mut masonry_bearing_interfaces = Vec::new();
    if program_kind == crate::TimberFrameProgramKind::CivicMasonryTimberHall {
        let sill_contacts = builder
            .members
            .iter()
            .filter(|member| {
                member.role == crate::TimberMemberRole::Sill
                    && (member.start.y - program.storey_height_metres).abs() <= 0.01
            })
            .flat_map(|member| {
                [
                    (member.start_node, member.support_interfaces[0]),
                    (member.end_node, member.support_interfaces[1]),
                ]
            })
            .chain(
                builder
                    .members
                    .iter()
                    .filter(|member| member.role == crate::TimberMemberRole::Knagge)
                    .map(|member| (member.start_node, member.support_interfaces[0])),
            )
            .collect::<Vec<_>>();
        for (node_id, interface_id) in sill_contacts {
            let Some(interface) = builder
                .geometry
                .support_interfaces
                .iter()
                .find(|interface| interface.id == interface_id)
                .copied()
            else {
                continue;
            };
            let masonry_support = walls
                .iter()
                .filter(|wall| {
                    wall.storey_level == 0
                        && wall.material == crate::WallMaterialClass::CivilianMasonry
                })
                .find(|wall| {
                    wall.host_solids.iter().any(|id| {
                        builder
                            .geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|solid| {
                                let half = solid.size * 0.5 + Vec3::splat(0.01);
                                let min = solid.centre - half;
                                let max = solid.centre + half;
                                interface.bounds.max.cmpge(min).all()
                                    && interface.bounds.min.cmple(max).all()
                            })
                    })
                })
                .map(|wall| wall.support_node);
            if let Some(support) = masonry_support
                && let Some(node) = builder
                    .geometry
                    .structural_nodes
                    .iter_mut()
                    .find(|node| node.id == node_id)
            {
                node.supported_by.push(support);
                node.supported_by.sort_unstable();
                node.supported_by.dedup();
                masonry_bearing_interfaces.push(interface_id);
            }
        }
    }

    // Roof-contour members and civic masonry contacts are added after the
    // first floor/frame pass; orient the final physical graph once more so no
    // late member is left with a nominal but ungrounded endpoint.
    builder.resolve_intermediate_member_bearings();
    builder.rebuild_physical_support_tree();
    builder.classify_physical_joints();

    Some(crate::TimberFrameAssembly {
        id: crate::TimberFrameAssemblyId(1),
        program: program_kind,
        phase: crate::TimberFramePhase::PrimaryConstruction,
        material: frame_material,
        facades,
        internal_lines,
        bays,
        members: builder.members,
        joints: builder.joints,
        floors,
        circulation,
        masonry_bearing_interfaces,
        roof_bearing_interfaces,
        dormer_trimmer_members,
    })
}
