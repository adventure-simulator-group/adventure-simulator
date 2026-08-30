fn spawn_battlement_run(
    world: &mut World,
    palette: &RenderPalette,
    run: BattlementRun,
    origin: Vec2,
) {
    let start = run.start + origin;
    let end = run.end + origin;
    let delta = end - start;
    let length = delta.length();
    if length <= 0.1 {
        return;
    }
    let tangent = delta / length;
    let outward = match run.outward {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    };
    let projection = match run.kind {
        BattlementKind::Machicolated | BattlementKind::Breteche => 0.42,
        BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding => 0.68,
        BattlementKind::Crenellated
        | BattlementKind::PiercedCrenellated
        | BattlementKind::CoveredWallWalk
        | BattlementKind::GunLoopParapet => 0.0,
    };
    let centre = (start + end) * 0.5 + outward * projection;
    let horizontal = delta.x.abs() >= delta.y.abs();
    let merlon_count = (length / 1.2).floor().max(2.0) as usize;
    let gallery_size = if horizontal {
        Vec3::new(length, 0.16, projection * 2.0 + 0.42)
    } else {
        Vec3::new(projection * 2.0 + 0.42, 0.16, length)
    };

    if matches!(
        run.kind,
        BattlementKind::Machicolated
            | BattlementKind::Breteche
            | BattlementKind::OpenHoarding
            | BattlementKind::RoofedHoarding
    ) {
        let material = if matches!(
            run.kind,
            BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding
        ) {
            &palette.timber
        } else {
            &palette.stone
        };
        spawn_box(
            world,
            material,
            gallery_size,
            Vec3::new(centre.x, run.base_height_metres, centre.y),
            Quat::IDENTITY,
            "projecting defensive gallery floor",
        );
    }

    if run.kind == BattlementKind::GunLoopParapet {
        for (height, y) in [(0.32, 0.16), (0.25, 1.125)] {
            spawn_box(
                world,
                &palette.stone,
                if horizontal {
                    Vec3::new(length, height, 0.42)
                } else {
                    Vec3::new(0.42, height, length)
                },
                Vec3::new(centre.x, run.base_height_metres + y, centre.y),
                Quat::IDENTITY,
                "gun-loop parapet horizontal masonry",
            );
        }
        let interval = length / merlon_count as f32;
        let slit_width = 0.12;
        let side_width = (interval - slit_width).max(0.1) * 0.5;
        for index in 0..merlon_count {
            let position = start.lerp(end, (index as f32 + 0.5) / merlon_count as f32);
            for sign in [-1.0, 1.0] {
                let pier = position + tangent * (slit_width + side_width) * 0.5 * sign;
                spawn_box(
                    world,
                    &palette.stone,
                    if horizontal {
                        Vec3::new(side_width, 0.72, 0.42)
                    } else {
                        Vec3::new(0.42, 0.72, side_width)
                    },
                    Vec3::new(pier.x, run.base_height_metres + 0.68, pier.y),
                    Quat::IDENTITY,
                    "gun-loop parapet pier",
                );
            }
        }
    }

    for index in 0..merlon_count {
        let progress = (index as f32 + 0.5) / merlon_count as f32;
        let position = start.lerp(end, progress) + outward * projection;
        if run.kind != BattlementKind::GunLoopParapet {
            let merlon_material = if matches!(
                run.kind,
                BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding
            ) {
                &palette.timber
            } else {
                &palette.stone
            };
            if run.kind == BattlementKind::PiercedCrenellated {
                let side_width = 0.27;
                for sign in [-1.0, 1.0] {
                    let pier = position + tangent * 0.205 * sign;
                    spawn_box(
                        world,
                        merlon_material,
                        if horizontal {
                            Vec3::new(side_width, 0.85, 0.38)
                        } else {
                            Vec3::new(0.38, 0.85, side_width)
                        },
                        Vec3::new(pier.x, run.base_height_metres + 0.425, pier.y),
                        Quat::IDENTITY,
                        "merlon split by firing loop",
                    );
                }
            } else {
                spawn_box(
                    world,
                    merlon_material,
                    if horizontal {
                        Vec3::new(0.68, 0.85, 0.38)
                    } else {
                        Vec3::new(0.38, 0.85, 0.68)
                    },
                    Vec3::new(position.x, run.base_height_metres + 0.425, position.y),
                    Quat::IDENTITY,
                    "battlement merlon",
                );
            }
        }
        if matches!(
            run.kind,
            BattlementKind::Machicolated | BattlementKind::Breteche
        ) && index % 2 == 0
        {
            let corbel = position - outward * 0.16;
            spawn_box(
                world,
                &palette.stone,
                if horizontal {
                    Vec3::new(0.26, 0.72, 0.52)
                } else {
                    Vec3::new(0.52, 0.72, 0.26)
                },
                Vec3::new(corbel.x, run.base_height_metres - 0.32, corbel.y),
                Quat::IDENTITY,
                "machicolation corbel",
            );
        }
        if matches!(
            run.kind,
            BattlementKind::OpenHoarding | BattlementKind::RoofedHoarding
        ) {
            let base = start.lerp(end, progress) + outward * 0.16;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(base.x, run.base_height_metres - 0.72, base.y),
                Vec3::new(position.x, run.base_height_metres + 0.95, position.y),
                0.13,
                "hoarding support strut",
            );
        }
    }

    if matches!(
        run.kind,
        BattlementKind::RoofedHoarding | BattlementKind::CoveredWallWalk | BattlementKind::Breteche
    ) {
        let roof_centre = centre + outward * 0.16;
        spawn_box(
            world,
            &palette.roof_secondary,
            if horizontal {
                Vec3::new(length + 0.5, 0.14, 1.55)
            } else {
                Vec3::new(1.55, 0.14, length + 0.5)
            },
            Vec3::new(roof_centre.x, run.base_height_metres + 1.62, roof_centre.y),
            if horizontal {
                Quat::from_rotation_x(0.10)
            } else {
                Quat::from_rotation_z(-0.10)
            },
            "covered wall-walk roof",
        );
    }
}

fn spawn_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    size: Vec3,
    translation: Vec3,
    rotation: Quat,
    name: &'static str,
) -> Entity {
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(size.x, size.y, size.z));
    let entity = world
        .spawn((
            Name::new(name),
            ClosedSolid,
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform {
                translation,
                rotation,
                ..default()
            },
        ))
        .id();
    tag_player_build_entity(world, entity, material);
    if world.get_resource::<PlayerBuildSpawnContext>().is_some() {
        let half = size * 0.5;
        let axes = [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z];
        let extent = Vec3::new(
            half.x * axes[0].x.abs() + half.y * axes[1].x.abs() + half.z * axes[2].x.abs(),
            half.x * axes[0].y.abs() + half.y * axes[1].y.abs() + half.z * axes[2].y.abs(),
            half.x * axes[0].z.abs() + half.y * axes[1].z.abs() + half.z * axes[2].z.abs(),
        );
        world.entity_mut(entity).insert(PlayerBuildRenderPrism {
            min: translation - extent,
            max: translation + extent,
        });
    }
    entity
}

fn tag_player_build_entity(world: &mut World, entity: Entity, material: &Handle<StandardMaterial>) {
    if let Some(context) = world.get_resource::<PlayerBuildSpawnContext>().copied() {
        world.entity_mut(entity).insert((
            PlayerBuildEntity,
            EditorVisibilityTarget {
                storey: context.storey,
                role: context.role,
            },
            EditorBaseMaterial(material.clone()),
            EditorAppearanceIsTranslucent(false),
            Visibility::Visible,
        ));
    }
}
