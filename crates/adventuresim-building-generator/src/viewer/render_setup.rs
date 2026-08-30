fn record_mesh_audit(world: &mut World) {
    let handles = world
        .query_filtered::<(&Mesh3d, Option<&Name>), With<ClosedSolid>>()
        .iter(world)
        .map(|(mesh, name)| {
            (
                mesh.0.clone(),
                name.map_or("unnamed closed solid", Name::as_str).to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let meshes = world.resource::<Assets<Mesh>>();
    let mut issue_count = 0;
    for (handle, name) in &handles {
        let Some(mesh) = meshes.get(handle) else {
            eprintln!("closed-solid mesh missing from assets: {name}");
            issue_count += 1;
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            eprintln!("closed-solid mesh lacks positions: {name}");
            issue_count += 1;
            continue;
        };
        let indices = match mesh.indices() {
            Some(Indices::U16(indices)) => indices.iter().map(|index| u32::from(*index)).collect(),
            Some(Indices::U32(indices)) => indices.clone(),
            None => (0..positions.len() as u32).collect(),
        };
        let audit = audit_triangle_mesh(positions, &indices);
        if !audit.passes_closed_solid() {
            eprintln!("closed-solid mesh failed integrity audit: {name}: {audit:?}");
            issue_count += 1;
        }
    }
    if let Some(mut state) = world.get_resource_mut::<CaptureState>() {
        state.manifest.audited_closed_mesh_count = handles.len();
        state.manifest.mesh_integrity_issue_count = issue_count;
    }
}

fn create_palette(world: &mut World) -> RenderPalette {
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let material = |materials: &mut Assets<StandardMaterial>, color: Color, roughness| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: roughness,
            unlit: false,
            ..default()
        })
    };
    RenderPalette {
        plaster: material(&mut materials, Color::srgb(0.80, 0.74, 0.60), 0.9),
        brick: material(&mut materials, Color::srgb(0.48, 0.20, 0.13), 0.92),
        stone: material(&mut materials, Color::srgb(0.43, 0.44, 0.40), 0.95),
        earth: material(&mut materials, Color::srgb(0.28, 0.20, 0.11), 1.0),
        timber: material(&mut materials, Color::srgb(0.16, 0.09, 0.045), 0.88),
        roof: material(&mut materials, Color::srgb(0.28, 0.08, 0.045), 0.95),
        roof_secondary: material(&mut materials, Color::srgb(0.17, 0.20, 0.22), 0.92),
        floor: material(&mut materials, Color::srgb(0.32, 0.25, 0.16), 0.98),
        cutaway: materials.add(StandardMaterial {
            base_color: Color::srgba(0.46, 0.52, 0.58, 0.24),
            perceptual_roughness: 0.96,
            alpha_mode: AlphaMode::Blend,
            unlit: false,
            ..default()
        }),
        door: material(&mut materials, Color::srgb(0.20, 0.105, 0.045), 0.86),
        glass: material(&mut materials, Color::srgb(0.18, 0.42, 0.56), 0.35),
        void: materials.add(StandardMaterial {
            base_color: Color::srgb(0.025, 0.022, 0.018),
            perceptual_roughness: 1.0,
            unlit: true,
            ..default()
        }),
        stair: material(&mut materials, Color::srgb(0.35, 0.23, 0.11), 0.9),
        room_floors: [
            Color::srgb(0.47, 0.24, 0.18),
            Color::srgb(0.25, 0.39, 0.51),
            Color::srgb(0.42, 0.46, 0.25),
            Color::srgb(0.52, 0.40, 0.20),
            Color::srgb(0.37, 0.29, 0.48),
            Color::srgb(0.24, 0.46, 0.42),
            Color::srgb(0.53, 0.31, 0.40),
        ]
        .into_iter()
        .map(|color| material(&mut materials, color, 0.98))
        .collect(),
    }
}

fn spawn_ground(world: &mut World, dimensions: Vec2, crown_proof: bool) {
    let mesh = world.resource_mut::<Assets<Mesh>>().add(
        Plane3d::default()
            .mesh()
            .size(dimensions.x * 2.4, dimensions.y * 2.4),
    );
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: if crown_proof {
                // A restrained dark proving ground keeps the isolated white
                // masonry's silhouette legible and provides a deterministic
                // lit shadow reference without an unlit calibration card.
                Color::srgb(0.14, 0.19, 0.11)
            } else {
                Color::srgb(0.30, 0.38, 0.22)
            },
            perceptual_roughness: 1.0,
            unlit: false,
            ..default()
        });
    world.spawn((
        Name::new("ground"),
        EditorEnvironmentEntity,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, -0.02, 0.0),
    ));
}

fn spawn_artillery_ground(world: &mut World, dimensions: Vec2, origin: Vec2) {
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.38, 0.22),
            perceptual_roughness: 1.0,
            unlit: false,
            ..default()
        });
    let half = dimensions.max_element() * 1.2;
    let outer = (-22.5_f32, 34.5_f32, -19.5_f32, 31.5_f32);
    let slabs = [
        (
            Vec3::new(origin.x + 6.0, -0.03, origin.y - (half + 19.5) * 0.5),
            Vec3::new(half * 2.0, 0.08, half - 19.5),
        ),
        (
            Vec3::new(origin.x + 6.0, -0.03, origin.y + (half + 31.5) * 0.5),
            Vec3::new(half * 2.0, 0.08, half - 31.5),
        ),
        (
            Vec3::new(origin.x - (half + 22.5) * 0.5, -0.03, origin.y + 6.0),
            Vec3::new(half - 22.5, 0.08, outer.3 - outer.2),
        ),
        (
            Vec3::new(origin.x + (half + 34.5) * 0.5, -0.03, origin.y + 6.0),
            Vec3::new(half - 34.5, 0.08, outer.3 - outer.2),
        ),
        // Protected court and ramp-side grade remain authoritative solid ground;
        // the ring between it and the outer slabs is the visible dry ditch.
        (
            Vec3::new(origin.x + 6.0, -0.03, origin.y + 6.0),
            Vec3::new(36.0, 0.08, 30.0),
        ),
    ];
    for (centre, size) in slabs {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Mesh::from(Cuboid::new(size.x, size.y, size.z)));
        world.spawn((
            Name::new("artillery terrain outside dry ditch"),
            EditorEnvironmentEntity,
            ClosedSolid,
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(centre),
        ));
    }
}
