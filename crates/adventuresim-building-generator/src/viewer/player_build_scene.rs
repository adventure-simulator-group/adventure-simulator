fn player_build_wall_style(material: PlayerBuildMaterial) -> Option<WallStyle> {
    match material {
        PlayerBuildMaterial::Stone => Some(WallStyle::Stone),
        PlayerBuildMaterial::Brick => Some(WallStyle::Brick),
        PlayerBuildMaterial::Plaster => Some(WallStyle::Plaster),
        PlayerBuildMaterial::TimberFrame | PlayerBuildMaterial::Timber => {
            Some(WallStyle::TimberFrame)
        }
        PlayerBuildMaterial::Tile | PlayerBuildMaterial::Thatch | PlayerBuildMaterial::Earth => {
            None
        }
    }
}

/// Player builds use the same semantic wall renderer as the generated plan.
/// A TimberFrame style consequently creates plaster infill and its grouped
/// fachwerk members together, rather than a brown placeholder cuboid.
fn setup_player_build_scene(world: &mut World, document: &PlayerBuildDocument) {
    let palette = create_palette(world);
    let (width, depth) = document.assembly.footprint.dimensions();
    let origin = Vec2::new(
        -f32::from(width) * CELL_SIZE_METRES * 0.5,
        -f32::from(depth) * CELL_SIZE_METRES * 0.5,
    );
    for storey in &document.assembly.storeys {
        let base_y = f32::from(storey.level) * document.assembly.storey_height_metres;
        world.insert_resource(PlayerBuildSpawnContext {
            storey: usize::from(storey.level),
            role: EditorVisibilityRole::Floor,
        });
        let stair_cuts = player_stair_floor_cuts(
            &document.assembly.stairs,
            base_y,
            document.assembly.storey_height_metres,
        );
        for room in &storey.rooms {
            for cell in &room.cells {
                spawn_player_floor_tile(world, &palette, *cell, base_y, origin, &stair_cuts);
            }
        }
        world.insert_resource(PlayerBuildSpawnContext {
            storey: usize::from(storey.level),
            role: EditorVisibilityRole::Wall,
        });
        for (wall_index, wall) in storey.walls.iter().copied().enumerate() {
            let selector = WallSelector {
                storey_level: storey.level,
                cell: wall.cell,
                direction: wall.direction,
            };
            let opening = storey
                .openings
                .iter()
                .find(|opening| opening.wall == wall_index);
            for (face_index, face) in freeform_wall_faces(storey, wall).into_iter().enumerate() {
                spawn_wall(
                    world,
                    &palette,
                    face,
                    if face_index == 0 { opening } else { None },
                    origin,
                    base_y,
                    document.assembly.storey_height_metres,
                    document.assembly.wall_style_for(selector),
                    document.assembly.interior_wall_finish,
                    document.assembly.timber_frame_style,
                    document.assembly.upper_storey_projection_metres * f32::from(storey.level),
                );
            }
        }
        world.remove_resource::<PlayerBuildSpawnContext>();
    }
    world.insert_resource(PlayerBuildSpawnContext {
        storey: 0,
        role: EditorVisibilityRole::Structure,
    });
    for stair in document.assembly.stairs.iter().copied() {
        spawn_stair(world, &palette, stair, origin);
    }
    world.remove_resource::<PlayerBuildSpawnContext>();
    world.insert_resource(PlayerBuildSpawnContext {
        storey: document
            .assembly
            .storeys
            .iter()
            .map(|storey| usize::from(storey.level))
            .max()
            .unwrap_or(0)
            + 1,
        role: EditorVisibilityRole::Roof,
    });
    for (roof_index, roof) in document.assembly.roofs.iter().copied().enumerate() {
        spawn_roof(
            world,
            &palette,
            roof,
            origin,
            roof_index,
            document.assembly.wall_style,
        );
    }
    for dormer in document.assembly.roof_dormers.iter().copied() {
        spawn_roof_dormer(
            world,
            &palette,
            dormer,
            origin,
            document.assembly.wall_style,
        );
    }
    world.remove_resource::<PlayerBuildSpawnContext>();
}

/// The floor aperture at the head of a straight flight. This matches the
/// resolver's stair-floor cut instead of leaving a whole grid tile across the
/// last tread after a generated building is detached.
fn player_stair_floor_cuts(
    stairs: &[Stair],
    floor_y: f32,
    storey_height: f32,
) -> Vec<(Vec2, Vec2)> {
    stairs
        .iter()
        .filter_map(|stair| match *stair {
            Stair::Straight {
                start,
                direction,
                base_height_metres,
                rise_metres,
                width_metres,
                run_metres,
                ..
            } if (base_height_metres + rise_metres - floor_y).abs() < storey_height * 0.08 => {
                let axis = direction_vector_2d(direction);
                let run = run_metres;
                let end = start + axis * run;
                // A floor opening must begin where the ascending occupant's
                // 1.90 m clearance prism first reaches this floor, not only
                // at the final tread. Include a small envelope beyond both
                // ends of that prism for the approach and landing.
                let lateral = Vec2::new(-axis.y, axis.x) * (width_metres.max(0.90) * 0.5);
                let clearance_start =
                    ((rise_metres - 1.90) / rise_metres.max(0.001) * run - 0.30).clamp(0.0, run);
                let inner = start + axis * clearance_start;
                let outer = end + axis * 0.30;
                Some((
                    (inner - lateral)
                        .min(inner + lateral)
                        .min(outer - lateral)
                        .min(outer + lateral),
                    (inner - lateral)
                        .max(inner + lateral)
                        .max(outer - lateral)
                        .max(outer + lateral),
                ))
            }
            _ => None,
        })
        .collect()
}

fn spawn_player_floor_tile(
    world: &mut World,
    palette: &RenderPalette,
    cell: Cell,
    base_y: f32,
    origin: Vec2,
    cuts: &[(Vec2, Vec2)],
) {
    let half = (CELL_SIZE_METRES - 0.04) * 0.5;
    let centre = cell.centre();
    let tile_min = centre - Vec2::splat(half);
    let tile_max = centre + Vec2::splat(half);
    let mut rectangles = vec![(tile_min, tile_max)];
    for (cut_min, cut_max) in cuts {
        let mut remaining = Vec::new();
        for (min, max) in rectangles {
            let overlap_min = min.max(*cut_min);
            let overlap_max = max.min(*cut_max);
            if overlap_min.x >= overlap_max.x || overlap_min.y >= overlap_max.y {
                remaining.push((min, max));
                continue;
            }
            // Subtract an axis-aligned stair opening while keeping the four
            // surrounding floor pieces separately selectable/renderable.
            remaining.extend([
                (min, Vec2::new(max.x, overlap_min.y)),
                (Vec2::new(min.x, overlap_max.y), max),
                (
                    Vec2::new(min.x, overlap_min.y),
                    Vec2::new(overlap_min.x, overlap_max.y),
                ),
                (
                    Vec2::new(overlap_max.x, overlap_min.y),
                    Vec2::new(max.x, overlap_max.y),
                ),
            ]);
        }
        rectangles = remaining;
    }
    for (min, max) in rectangles {
        let size = max - min;
        if size.min_element() <= 0.01 {
            continue;
        }
        let centre = (min + max) * 0.5 + origin;
        let entity = spawn_box(
            world,
            &palette.floor,
            Vec3::new(size.x, 0.12, size.y),
            Vec3::new(centre.x, base_y + 0.06, centre.y),
            Quat::IDENTITY,
            "player build floor tile",
        );
        world.entity_mut(entity).insert(PlayerBuildFloorPrism {
            min: Vec3::new(min.x + origin.x, base_y, min.y + origin.y),
            max: Vec3::new(max.x + origin.x, base_y + 0.12, max.y + origin.y),
        });
    }
}

fn freeform_wall_faces(
    storey: &adventuresim_building_generator::StoreyPlan,
    wall: WallSegment,
) -> Vec<WallSegment> {
    let has_floor = |cell| storey.rooms.iter().any(|room| room.cells.contains(&cell));
    let inside = has_floor(wall.cell);
    let outside_cell = wall.cell.neighbour(wall.direction);
    let outside = has_floor(outside_cell);
    let reverse = WallSegment {
        cell: outside_cell,
        direction: wall.direction.opposite(),
        inside_room: wall.outside_room.unwrap_or(0),
        outside_room: None,
    };
    match (inside, outside) {
        (true, true) => vec![WallSegment {
            outside_room: Some(0),
            ..wall
        }],
        (true, false) => vec![WallSegment {
            outside_room: None,
            ..wall
        }],
        (false, true) => vec![reverse],
        (false, false) => vec![
            WallSegment {
                outside_room: None,
                ..wall
            },
            reverse,
        ],
    }
}

fn update_editor_visibility(
    runtime: Res<EditorRuntime>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut targets: Query<(
        Ref<EditorVisibilityTarget>,
        &EditorBaseMaterial,
        &mut EditorAppearanceIsTranslucent,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
        Option<&EditorFachwerkForFinishedWall>,
    )>,
) {
    let runtime_changed = runtime.is_changed();
    if !runtime_changed && targets.iter().all(|(target, ..)| !target.is_added()) {
        return;
    }
    for (target, base_material, mut appearance, mut material, mut visibility, hide_fachwerk) in
        &mut targets
    {
        if !runtime_changed && !target.is_added() {
            continue;
        }
        let above_active_storey = target.storey > runtime.active_storey;
        let hidden_wall = target.role == EditorVisibilityRole::Wall
            && runtime.wall_visibility == WallVisibility::Down;
        *visibility = if above_active_storey || hidden_wall || hide_fachwerk.is_some() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        let translucent = match target.role {
            EditorVisibilityRole::Wall if runtime.wall_visibility == WallVisibility::Cutaway => {
                true
            }
            _ => false,
        };
        if appearance.0 != translucent {
            material.0 = if translucent {
                let mut ghost = materials
                    .get(&base_material.0)
                    .cloned()
                    .unwrap_or_else(StandardMaterial::default);
                let colour = ghost.base_color.to_srgba();
                ghost.base_color = Color::srgba(colour.red, colour.green, colour.blue, 0.24);
                ghost.alpha_mode = AlphaMode::Blend;
                materials.add(ghost)
            } else {
                base_material.0.clone()
            };
            appearance.0 = translucent;
        }
    }
}

fn frame_editor_selection(
    keys: Res<ButtonInput<KeyCode>>,
    runtime: Res<EditorRuntime>,
    targets: Query<(
        &EditorSelectable,
        &GlobalTransform,
        Option<&ResolvedRenderItem>,
        Option<&RoofRenderItem>,
    )>,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(selected) = runtime.selected else {
        return;
    };
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut found = false;
    for (selectable, transform, resolved, roof) in &targets {
        if selectable.0 != selected {
            continue;
        }
        let half_size = resolved
            .map(|item| item.local_half_size)
            .or_else(|| roof.map(|item| item.local_half_size))
            .unwrap_or(Vec3::splat(0.25));
        let centre = transform.translation();
        minimum = minimum.min(centre - half_size);
        maximum = maximum.max(centre + half_size);
        found = true;
    }
    if !found {
        return;
    }
    let focus = (minimum + maximum) * 0.5;
    let radius = (maximum - minimum).length().max(1.0) * 1.6;
    for mut camera in &mut cameras {
        camera.target_focus = focus;
        camera.target_radius = radius;
        camera.force_update = true;
    }
}

fn rebuild_editor_scene(world: &mut World) {
    let pending = world
        .get_resource::<EditorRuntime>()
        .is_some_and(|runtime| runtime.pending_rebuild);
    if pending {
        let old_entities = {
            let mut query = world.query_filtered::<Entity, With<EditorBuildingEntity>>();
            query.iter(world).collect::<Vec<_>>()
        };
        for entity in old_entities {
            let _ = world.despawn(entity);
        }
        let plan = world.resource::<EditorRuntime>().plan.clone();
        if world.resource::<EditorRuntime>().show_generated_building {
            setup(
                world,
                &plan,
                ViewerView::Exterior,
                ProjectedProofKind::Machicolation,
                None,
                SceneSetup::EditorBuilding,
            );
            configure_editor_scene(world, &plan, false);
        }
        world.resource_mut::<EditorRuntime>().pending_rebuild = false;
    }

    let player_rebuild = world
        .get_resource::<EditorRuntime>()
        .is_some_and(|runtime| runtime.pending_player_rebuild);
    if player_rebuild {
        let old_entities = {
            let mut query = world.query_filtered::<Entity, With<PlayerBuildEntity>>();
            query.iter(world).collect::<Vec<_>>()
        };
        for entity in old_entities {
            let _ = world.despawn(entity);
        }
        if let Some(document) = world.resource::<EditorRuntime>().player_build.clone() {
            setup_player_build_scene(world, &document);
        }
        world.resource_mut::<EditorRuntime>().pending_player_rebuild = false;
    }
    if pending || player_rebuild {
        world
            .run_system_once(update_editor_visibility)
            .expect("editor visibility system must run after rebuilding its scene");
    }
}
