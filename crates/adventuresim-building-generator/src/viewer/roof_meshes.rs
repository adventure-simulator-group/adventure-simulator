fn boundary_notched_polygon(outer: &[Vec3], cutout: &[Vec3]) -> Option<Vec<Vec3>> {
    let on_segment = |point: Vec3, a: Vec3, b: Vec3| {
        let delta = b - a;
        let t = ((point - a).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
        point.distance_squared(a + delta * t) <= 0.000_004
    };
    for edge_index in 0..outer.len() {
        let a = outer[edge_index];
        let b = outer[(edge_index + 1) % outer.len()];
        let delta = b - a;
        let mut touches = cutout
            .iter()
            .enumerate()
            .filter(|(_, point)| on_segment(**point, a, b))
            .map(|(index, point)| {
                (
                    index,
                    ((point - a).dot(delta) / delta.length_squared()).clamp(0.0, 1.0),
                )
            })
            .collect::<Vec<_>>();
        if touches.len() != 2 {
            continue;
        }
        touches.sort_by(|left, right| left.1.total_cmp(&right.1));
        let (first, second) = (touches[0].0, touches[1].0);
        let forward_steps = (second + cutout.len() - first) % cutout.len();
        let step: isize = if forward_steps > 1 { 1 } else { -1 };
        let mut path = Vec::new();
        let mut current = first;
        loop {
            path.push(cutout[current]);
            if current == second {
                break;
            }
            current = (current as isize + step).rem_euclid(cutout.len() as isize) as usize;
        }
        let mut polygon = Vec::with_capacity(outer.len() + path.len());
        for (index, point) in outer.iter().copied().enumerate() {
            polygon.push(point);
            if index == edge_index {
                polygon.extend(path.iter().copied().filter(|candidate| {
                    candidate.distance_squared(a) > 0.000_004
                        && candidate.distance_squared(b) > 0.000_004
                }));
            }
        }
        return Some(polygon);
    }
    let removed = outer.iter().position(|outer_point| {
        cutout
            .iter()
            .any(|cut| cut.distance_squared(*outer_point) <= 0.000_004)
    })?;
    let previous = outer[(removed + outer.len() - 1) % outer.len()];
    let removed_point = outer[removed];
    let next = outer[(removed + 1) % outer.len()];
    let previous_touch = cutout.iter().copied().find(|point| {
        point.distance_squared(removed_point) > 0.000_004
            && on_segment(*point, previous, removed_point)
    })?;
    let next_touch = cutout.iter().copied().find(|point| {
        point.distance_squared(removed_point) > 0.000_004 && on_segment(*point, removed_point, next)
    })?;
    let interior = cutout.iter().copied().find(|point| {
        point.distance_squared(removed_point) > 0.000_004
            && point.distance_squared(previous_touch) > 0.000_004
            && point.distance_squared(next_touch) > 0.000_004
    })?;
    let mut polygon = Vec::with_capacity(outer.len() + 2);
    for (index, point) in outer.iter().copied().enumerate() {
        if index == removed {
            polygon.extend([previous_touch, interior, next_touch]);
        } else {
            polygon.push(point);
        }
    }
    Some(polygon)
}

fn roof_face_prism_mesh(face: &RoofFace) -> Mesh {
    let offset = -face.plane.normal.normalize_or_zero() * face.thickness_metres;
    let mut outer = face.polygon.clone();
    let mut remaining_cutouts = Vec::new();
    for cutout in &face.cutouts {
        if let Some(notched) = boundary_notched_polygon(&outer, cutout) {
            outer = notched;
        } else {
            remaining_cutouts.push(cutout.clone());
        }
    }
    loop {
        let removable = (0..outer.len()).find(|index| {
            let previous = outer[(*index + outer.len() - 1) % outer.len()];
            let current = outer[*index];
            let next = outer[(*index + 1) % outer.len()];
            (current - previous).cross(next - current).length_squared() <= 0.000_004
        });
        if outer.len() <= 3 || removable.is_none() {
            break;
        }
        outer.remove(removable.unwrap());
    }
    let mut vertices = outer.clone();
    let mut hole_indices = Vec::new();
    for cutout in &remaining_cutouts {
        hole_indices.push(vertices.len() as u32);
        vertices.extend(cutout.iter().copied());
    }
    let mut triangles = Vec::new();
    earcut::Earcut::<f32>::new().earcut(
        vertices.iter().map(|point| [point.x, point.z]),
        &hole_indices,
        &mut triangles,
    );
    let mut faces = Vec::new();
    let mut top_edges = Vec::new();
    let (triangles, remainder) = triangles.as_chunks::<3>();
    debug_assert!(remainder.is_empty());
    for triangle in triangles {
        let mut top = triangle
            .iter()
            .map(|index| vertices[*index as usize])
            .collect::<Vec<_>>();
        if (top[1] - top[0])
            .cross(top[2] - top[0])
            .dot(face.plane.normal)
            < 0.0
        {
            top.reverse();
        }
        for index in 0..3 {
            top_edges.push((top[index], top[(index + 1) % 3]));
        }
        faces.push(top.clone());
        faces.push(top.iter().rev().map(|point| *point + offset).collect());
    }
    // Earcut is allowed to elide collinear boundary vertices. Deriving the
    // prism walls from the source loops then creates T-junctions where one top
    // triangle spans two side quads. Instead, extrude the actual one-use
    // boundary edges of the triangulated top; this keeps notched eaves and
    // interior cuts watertight even when their source loops contain collinear
    // construction points.
    for (index, (a, b)) in top_edges.iter().copied().enumerate() {
        let uses = top_edges
            .iter()
            .enumerate()
            .filter(|(candidate_index, (start, end))| {
                *candidate_index != index
                    && (((*start - a).length_squared() <= 0.000_004
                        && (*end - b).length_squared() <= 0.000_004)
                        || ((*start - b).length_squared() <= 0.000_004
                            && (*end - a).length_squared() <= 0.000_004))
            })
            .count();
        if uses == 0 {
            faces.push(vec![a, a + offset, b + offset, b]);
        }
    }
    flat_face_mesh(&faces)
}

fn spawn_resolved_roof(
    world: &mut World,
    palette: &RenderPalette,
    roof: &RoofAssembly,
    geometry: &adventuresim_building_generator::ResolvedGeometry,
    origin: Vec2,
    removed_items: &std::collections::HashSet<u64>,
    lighting_calibration: bool,
    cutaway_material: bool,
) {
    for face in roof
        .faces
        .iter()
        .filter(|face| !removed_items.contains(&face.id.0))
    {
        let mesh = roof_face_prism_mesh(face);
        let handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = if cutaway_material {
            &palette.cutaway
        } else {
            match face.material {
                RoofMaterial::ClayTile | RoofMaterial::TimberShingle => &palette.roof,
                RoofMaterial::Slate | RoofMaterial::Lead => &palette.roof_secondary,
                RoofMaterial::TimberInfill => &palette.plaster,
                RoofMaterial::MasonryInfill => &palette.stone,
            }
        };
        let bounds = face.polygon.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), point| (min.min(*point), max.max(*point)),
        );
        let mut entity = world.spawn((
            Name::new(format!("resolved roof {} face {}", roof.id.0, face.id.0)),
            ClosedSolid,
            GeometryOwner(roof.owner.0),
            RoofRenderItem {
                id: face.id.0,
                fingerprint: stable_u64(&serde_json::to_vec(face).expect("serialize roof face")),
                local_center: (bounds.0 + bounds.1) * 0.5,
                local_half_size: (bounds.1 - bounds.0) * 0.5,
            },
            Mesh3d(handle),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(origin.x, 0.0, origin.y),
        ));
        if lighting_calibration {
            entity.insert(LightingCalibration {
                local_center: (bounds.0 + bounds.1) * 0.5,
                local_half_size: (bounds.1 - bounds.0) * 0.5,
            });
        }
    }
    for enclosure in &roof.enclosure_faces {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(roof_enclosure_prism_mesh(enclosure));
        let material = if cutaway_material {
            &palette.cutaway
        } else {
            match enclosure.material {
                RoofMaterial::TimberInfill => &palette.plaster,
                RoofMaterial::MasonryInfill => &palette.stone,
                RoofMaterial::ClayTile | RoofMaterial::TimberShingle => &palette.roof,
                RoofMaterial::Slate | RoofMaterial::Lead => &palette.roof_secondary,
            }
        };
        let bounds = enclosure.polygon.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), point| (min.min(*point), max.max(*point)),
        );
        world.spawn((
            Name::new(format!(
                "resolved roof {} enclosure {}",
                roof.id.0, enclosure.id.0
            )),
            ClosedSolid,
            GeometryOwner(roof.owner.0),
            RoofRenderItem {
                id: enclosure.id.0,
                fingerprint: stable_u64(
                    &serde_json::to_vec(enclosure).expect("serialize roof enclosure"),
                ),
                local_center: (bounds.0 + bounds.1) * 0.5,
                local_half_size: (bounds.1 - bounds.0) * 0.5,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(origin.x, 0.0, origin.y),
        ));
    }
    // Cuboidal framing, flashing, gutters, and edge treatments are spawned by
    // the shared resolved-solid renderer. Keeping their rendering there makes
    // the exact solid multiset authoritative and prevents duplicate roof
    // volume at this polygonal-face pass.
    let _ = geometry;
}

fn roof_enclosure_prism_mesh(enclosure: &RoofEnclosureFace) -> Mesh {
    let normal = (enclosure.polygon[1] - enclosure.polygon[0])
        .cross(enclosure.polygon[2] - enclosure.polygon[0])
        .normalize_or_zero();
    let offset = -normal * 0.16;
    let mut polygons = vec![
        enclosure.polygon.clone(),
        enclosure
            .polygon
            .iter()
            .rev()
            .map(|point| *point + offset)
            .collect::<Vec<_>>(),
    ];
    for index in 0..enclosure.polygon.len() {
        let next = (index + 1) % enclosure.polygon.len();
        polygons.push(vec![
            enclosure.polygon[index],
            enclosure.polygon[index] + offset,
            enclosure.polygon[next] + offset,
            enclosure.polygon[next],
        ]);
    }
    outward_flat_face_mesh(polygons)
}
