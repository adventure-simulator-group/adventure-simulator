fn resolve_round_tower_wall_assemblies(
    towers: &[RoundTower],
    crowns: &[CrownAssembly],
    walls: &mut Vec<crate::WallAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    for (tower_index, tower) in towers.iter().copied().enumerate() {
        let serial = walls.len() as u64 + 1;
        let id = crate::WallAssemblyId(serial);
        let owner = GeometryOwnerId(60_000 + tower_index as u32);
        let support_node = StructuralNodeId(3_000_000 + tower_index as u64);
        let centre = tower.centre_metres();
        geometry.structural_nodes.push(StructuralNode {
            id: support_node,
            owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(centre.x, 0.0, centre.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let host = wall_solid(
            geometry,
            owner,
            0,
            Vec3::new(centre.x, tower.wall_height_metres * 0.5, centre.y),
            Vec3::new(
                tower.radius_metres() * 2.0,
                tower.wall_height_metres,
                tower.radius_metres() * 2.0,
            ),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::RoundTowerShell {
                outer_radius_metres: tower.radius_metres(),
                inner_radius_metres: tower.radius_metres() - tower.wall_thickness_metres,
                chord_interfaces: [tower.chord_interface, tower.secondary_chord_interface],
            },
            support_node,
        );
        walls.push(crate::WallAssembly {
            id,
            owner,
            source: crate::WallSourceId::RoundTower { tower_index },
            material: crate::WallMaterialClass::FortifiedMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin: centre,
                tangent: Vec2::X,
                outward: -Vec2::Y,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: Some(crate::RadialWallFrame {
                centre,
                reference_outward: -Vec2::Y,
            }),
            length_metres: std::f32::consts::TAU * tower.radius_metres(),
            height_metres: tower.wall_height_metres,
            base_elevation_metres: 0.0,
            thickness_metres: tower.wall_thickness_metres,
            structural_role: crate::WallStructuralRole::TowerShell,
            support_node,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
        if let Some(crown) = crowns.iter().find(|crown| {
            matches!(
                crown.path,
                CrownPath::Round { tower_index: index, .. } if index == tower_index
            )
        }) {
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId((7_u64 << 60) | (u64::from(owner.0) << 24) | tower_index as u64),
                owners: [owner, crown.owner],
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        centre.x - tower.radius_metres() - 0.05,
                        tower.wall_height_metres - 0.08,
                        centre.y - tower.radius_metres() - 0.05,
                    ),
                    max: Vec3::new(
                        centre.x + tower.radius_metres() + 0.05,
                        tower.wall_height_metres + 0.18,
                        centre.y + tower.radius_metres() + 0.05,
                    ),
                },
                minimum_interface_area_square_metres: 0.01,
                // The resolved annular shell's conservative AABB overlaps the
                // full radial depth of its segmented deck. The physical
                // interface remains the tower-top annulus recorded above.
                maximum_penetration_metres: 1.10,
            });
        }
    }
}
