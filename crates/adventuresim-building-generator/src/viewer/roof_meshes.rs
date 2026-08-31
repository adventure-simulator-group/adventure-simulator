fn roof_face_prism_mesh(face: &RoofFace) -> Mesh {
    let faces = adventuresim_building_generator::tessellate_roof_face(face)
        .into_iter()
        .map(|triangle| triangle.positions.to_vec())
        .collect::<Vec<_>>();
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
    let faces = adventuresim_building_generator::tessellate_roof_enclosure(enclosure)
        .into_iter()
        .map(|triangle| triangle.positions.to_vec())
        .collect::<Vec<_>>();
    flat_face_mesh(&faces)
}
