fn spawn_artillery_marker_segment(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    thickness: f32,
    name: &'static str,
) {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.01 {
        return;
    }
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(thickness, length, thickness));
    world.spawn((
        Name::new(name),
        NonCollidingVisualization,
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        Transform {
            translation: (start + end) * 0.5,
            rotation: Quat::from_rotation_arc(Vec3::Y, delta / length),
            ..default()
        },
    ));
}

fn spawn_artillery_proof_markers(
    world: &mut World,
    plan: &BuildingPlan,
    view: ViewerView,
    origin: Vec2,
) {
    let Some(castle) = &plan.artillery_castle else {
        return;
    };
    let offset = Vec3::new(origin.x, 0.0, origin.y);
    match view {
        ViewerView::ArtilleryCirculation => {
            let route_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.95, 0.55, 0.06),
                        unlit: true,
                        ..default()
                    });
            for edge in &castle.route_edges {
                for pair in edge.sweep_path.windows(2) {
                    spawn_artillery_marker_segment(
                        world,
                        &route_material,
                        pair[0] + offset + Vec3::Y * 0.15,
                        pair[1] + offset + Vec3::Y * 0.15,
                        0.12,
                        "artillery authoritative swept circulation edge",
                    );
                }
            }
        }
        ViewerView::ArtilleryFirePlan => {
            let ray_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.85, 0.08, 0.04),
                        unlit: true,
                        ..default()
                    });
            for station in &castle.stations {
                for ray in &station.rays {
                    spawn_artillery_marker_segment(
                        world,
                        &ray_material,
                        ray.origin + offset + Vec3::Y * 0.05,
                        ray.target + offset + Vec3::Y * 0.05,
                        0.10,
                        "artillery authoritative firing ray",
                    );
                }
            }
        }
        ViewerView::ArtilleryDrainage => {
            let drain_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.05, 0.45, 0.90),
                        unlit: true,
                        ..default()
                    });
            for route_id in castle
                .drainage_routes
                .iter()
                .chain(&castle.ditch.drainage_routes)
            {
                if let Some(route) = plan
                    .resolved_geometry
                    .drainage_routes
                    .iter()
                    .find(|route| route.id == *route_id)
                {
                    spawn_artillery_marker_segment(
                        world,
                        &drain_material,
                        route.inlet + offset + Vec3::Y * 0.05,
                        route.outlet + offset + Vec3::Y * 0.05,
                        0.10,
                        "artillery authoritative drainage route",
                    );
                }
            }
        }
        ViewerView::ArtillerySupportDag => {
            let support_material =
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial {
                        base_color: Color::srgb(0.70, 0.12, 0.70),
                        unlit: true,
                        ..default()
                    });
            for node in &plan.resolved_geometry.structural_nodes {
                if !node.supported_by.is_empty() && matches!(node.kind,
                    adventuresim_building_generator::StructuralNodeKind::ArtilleryRevetmentBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryRetainingBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryTerrepleinBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryRondelBearing
                    | adventuresim_building_generator::StructuralNodeKind::ArtilleryBridgeAbutment)
                {
                    for supporting in &node.supported_by {
                        if let Some(base)=plan.resolved_geometry.structural_nodes.iter().find(|candidate|candidate.id==*supporting) {
                            spawn_artillery_marker_segment(world,&support_material,node.position+offset,base.position+offset,0.10,"artillery authoritative support edge");
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn spawn_crown_defender_scale(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    view: ViewerView,
    origin: Vec2,
) {
    let owner = match view {
        ViewerView::CrownTowerExterior
        | ViewerView::CrownTowerTop
        | ViewerView::CrownTowerCutaway => {
            let preferred = plan
                .gate_defenses
                .first()
                .and_then(|gate| gate.firing_positions.first())
                .map(|position| position.tower_index);
            plan.crowns.iter().find_map(|crown| match crown.path {
                CrownPath::Round { tower_index, .. }
                    if preferred.is_none_or(|preferred| preferred == tower_index) =>
                {
                    Some(crown.owner)
                }
                _ => None,
            })
        }
        ViewerView::CrownCornerExterior | ViewerView::CrownCornerInterior => plan
            .crowns
            .iter()
            .flat_map(|crown| {
                crown
                    .junctions
                    .iter()
                    .map(move |junction| (crown, junction))
            })
            .find(|(_, junction)| {
                junction.kind == adventuresim_building_generator::CrownJunctionKind::Corner
            })
            .map(|(crown, _)| crown.owner),
        _ => plan
            .crowns
            .iter()
            .find(|crown| matches!(crown.path, CrownPath::Straight { .. }))
            .map(|crown| crown.owner),
    };
    let Some(sample) = owner.and_then(|owner| {
        plan.resolved_geometry
            .defender_samples
            .iter()
            .find(|sample| sample.owner == owner)
    }) else {
        return;
    };
    let base = sample.stance + Vec3::new(origin.x, 0.0, origin.y);
    for (name, size, offset) in [
        (
            "non-colliding 1.72m defender scale torso",
            Vec3::new(0.38, 0.88, 0.24),
            Vec3::new(0.0, 0.72, 0.0),
        ),
        (
            "non-colliding defender scale head",
            Vec3::splat(0.28),
            Vec3::new(0.0, 1.38, 0.0),
        ),
        (
            "non-colliding defender scale legs",
            Vec3::new(0.28, 0.58, 0.2),
            Vec3::new(0.0, 0.29, 0.0),
        ),
    ] {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(size.x, size.y, size.z));
        world.spawn((
            Name::new(name),
            NonCollidingVisualization,
            Mesh3d(mesh),
            MeshMaterial3d(palette.timber.clone()),
            Transform::from_translation(base + offset),
        ));
    }
}

fn spawn_projected_proof_markers(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    owner: adventuresim_building_generator::GeometryOwnerId,
    origin: Vec2,
    view: ViewerView,
) {
    let Some(defense) = plan
        .projected_defenses
        .iter()
        .find(|defense| defense.owner == owner)
    else {
        return;
    };
    let (centre, outward, tangent, extent) = match defense.path {
        ProjectedDefensePath::Linear {
            start,
            end,
            outward,
        } => {
            let outward = direction_vector_2d(outward);
            (
                (start + end) * 0.5,
                outward,
                (end - start).normalize_or_zero(),
                start.distance(end),
            )
        }
        ProjectedDefensePath::Round {
            centre,
            radius_metres,
            outward,
        } => {
            let outward = direction_vector_2d(outward);
            (
                centre,
                outward,
                Vec2::new(-outward.y, outward.x),
                radius_metres * 2.0,
            )
        }
    };
    let centre = centre + origin;
    let body = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cylinder::new(0.18, 1.7));
    world.spawn((
        Name::new("projected defense defender scale"),
        Mesh3d(body),
        MeshMaterial3d(palette.timber.clone()),
        Transform::from_xyz(centre.x, defense.floor_elevation_metres + 0.85, centre.y),
    ));
    let calibration_size = Vec3::splat(0.8);
    let calibration_side = if view == ViewerView::ProjectedInterior {
        -outward
    } else {
        outward
    };
    // Keep the luminance witness in-frame but beyond the authoritative work's
    // tangent end and projection envelope. It must never masquerade as a
    // corbel, merlon or freestanding host pier in the proof silhouette.
    let calibration_position = centre
        + calibration_side * (defense.projection_metres + 0.8)
        + tangent * (extent * 0.5 + 0.7);
    let calibration_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::from_size(calibration_size));
    world.spawn((
        Name::new("projected daylight calibration block"),
        NonCollidingVisualization,
        LightingCalibration {
            local_center: Vec3::ZERO,
            local_half_size: calibration_size * 0.5,
        },
        Mesh3d(calibration_mesh),
        MeshMaterial3d(palette.stone.clone()),
        Transform {
            translation: Vec3::new(
                calibration_position.x,
                defense.floor_elevation_metres + 0.4,
                calibration_position.y,
            ),
            rotation: Quat::from_rotation_y(if view == ViewerView::ProjectedSockets {
                1.5
            } else {
                0.55
            }) * Quat::from_rotation_x(if view == ViewerView::ProjectedTop {
                0.75
            } else {
                0.35
            }),
            ..default()
        },
    ));
    if let Some(ray) = plan
        .resolved_geometry
        .projected_defense_rays
        .iter()
        .find(|ray| ray.owner == owner)
    {
        let start = ray.origin + Vec3::new(origin.x, 0.0, origin.y);
        let end = ray.target + Vec3::new(origin.x, 0.0, origin.y);
        spawn_timber_beam(
            world,
            &palette.roof,
            start,
            end,
            0.035,
            "projected defense downward ray",
        );
        let target = world.resource_mut::<Assets<Mesh>>().add(Sphere::new(0.18));
        world.spawn((
            Name::new("projected defense wall-foot target"),
            Mesh3d(target),
            MeshMaterial3d(palette.roof.clone()),
            Transform::from_translation(end),
        ));
    }
}
