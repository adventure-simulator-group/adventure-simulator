/// Resolve the nave clerestory as the physical host of the aisle shed high
/// edges.  The former roof-only model labelled these edges as wall abutments
/// and then discarded them because no masonry actually occupied the contact
/// contour.  These two wall assemblies continue the arcade lines above the
/// aisle roofs; the roof resolver can therefore measure flashing contact
/// against real masonry rather than a synthetic proof surface.
#[allow(dead_code)]
fn resolve_cathedral_clerestory_walls(
    roofs: &[RoofPiece],
    walls: &mut Vec<crate::WallAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    let lower_supports = |base: f32, walls: &[crate::WallAssembly]| {
        let mut supports = walls
            .iter()
            .filter(|wall| {
                wall.replaced_by_owner.is_none()
                    && (wall.base_elevation_metres + wall.height_metres - base).abs() <= 0.08
            })
            .map(|wall| wall.support_node)
            .collect::<Vec<_>>();
        supports.sort_unstable();
        supports.dedup();
        supports
    };

    for (slot, (roof_index, high_side, outward)) in [
        (1_usize, Direction::East, Vec2::NEG_X),
        (2_usize, Direction::West, Vec2::X),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(shed) = roofs.get(roof_index).copied() else {
            continue;
        };
        let Some(polygon) = roof_face_polygons(shed, Some(high_side)).into_iter().next() else {
            continue;
        };
        let high = polygon
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let high_vertices = polygon
            .iter()
            .filter(|point| (point.y - high).abs() <= 0.01)
            .copied()
            .collect::<Vec<_>>();
        if high_vertices.len() != 2 {
            continue;
        }
        let contact_x = (high_vertices[0].x + high_vertices[1].x) * 0.5;
        let min_z = high_vertices
            .iter()
            .map(|point| point.z)
            .fold(f32::INFINITY, f32::min);
        let max_z = high_vertices
            .iter()
            .map(|point| point.z)
            .fold(f32::NEG_INFINITY, f32::max);
        let base = shed.base_height_metres;
        // The 0.24 m upstand above the contact is a project weathering gate,
        // not a historical universal dimension.
        let top = high + 0.24;
        let height = top - base;
        let length = max_z - min_z;
        // The shed terminates at the exterior face of the clerestory, not at
        // its centreline.  Keeping the masonry on the nave side avoids both a
        // buried roof edge and an additive flashing screen.
        let origin = Vec2::new(contact_x - outward.x * 0.90 * 0.5, (min_z + max_z) * 0.5);
        let owner = GeometryOwnerId(53_000 + slot as u32);
        let wall_id = crate::WallAssemblyId(900_000 + slot as u64);
        let node = StructuralNodeId(2_900_000 + slot as u64);
        let supports = lower_supports(base, walls);
        geometry.structural_nodes.push(StructuralNode {
            id: node,
            owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, base, origin.y),
            supported_by: supports,
            grounded: false,
        });
        let host = wall_solid(
            geometry,
            owner,
            0xC100 + slot as u64,
            Vec3::new(origin.x, base + height * 0.5, origin.y),
            Vec3::new(length, height, 0.90),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            node,
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == host)
            .expect("new clerestory wall solid")
            .yaw_radians = std::f32::consts::FRAC_PI_2;
        walls.push(crate::WallAssembly {
            id: wall_id,
            owner,
            source: crate::WallSourceId::CathedralClerestory { side: high_side },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 1,
            frame: crate::WallLocalFrame {
                origin,
                tangent: Vec2::Y,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: length,
            height_metres: height,
            base_elevation_metres: base,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::LoadBearing,
            support_node: node,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
    }
}
