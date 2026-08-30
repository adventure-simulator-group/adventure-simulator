fn apply_player_build_edit(runtime: &mut EditorRuntime, edit: PlayerBuildEdit) {
    let Some(document) = &runtime.player_build else {
        runtime.error =
            Some("launch with --player-build-document to edit a freeform build".to_owned());
        return;
    };
    match document.apply(edit) {
        Ok(next) => {
            runtime.player_build = Some(next);
            runtime.pending_player_rebuild = true;
            runtime.error = None;
            runtime.status = "Freeform edit applied".to_owned();
        }
        Err(error) => runtime.error = Some(error),
    }
}

fn editor_keyboard_shortcuts(keys: Res<ButtonInput<KeyCode>>, mut runtime: ResMut<EditorRuntime>) {
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let action = if control && keys.just_pressed(KeyCode::KeyZ) {
        Some(EditorUiAction::Undo)
    } else if control && keys.just_pressed(KeyCode::KeyY) {
        Some(EditorUiAction::Redo)
    } else if keys.just_pressed(KeyCode::Escape) {
        Some(EditorUiAction::SetMode(EditorMode::Select))
    } else if keys.just_pressed(KeyCode::Digit1) {
        Some(EditorUiAction::SetMode(EditorMode::Select))
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(EditorUiAction::SetMode(EditorMode::Construct))
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(EditorUiAction::SetMode(EditorMode::Openings))
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(EditorUiAction::SetMode(EditorMode::Roof))
    } else if keys.just_pressed(KeyCode::Digit5) {
        Some(EditorUiAction::SetMode(EditorMode::Site))
    } else if keys.just_pressed(KeyCode::Digit6) {
        Some(EditorUiAction::SetMode(EditorMode::Finish))
    } else if keys.just_pressed(KeyCode::Home) {
        Some(EditorUiAction::CycleWalls)
    } else if keys.just_pressed(KeyCode::PageUp) {
        Some(EditorUiAction::NextStorey)
    } else if keys.just_pressed(KeyCode::PageDown) {
        Some(EditorUiAction::PreviousStorey)
    } else {
        None
    };
    if let Some(action) = action {
        if let EditorUiAction::SetMode(mode) = action
            && !mode.is_available()
        {
            runtime.status = mode.availability().to_owned();
            return;
        }
        perform_editor_action(&mut runtime, action);
    }
}

fn apply_editor_edit(runtime: &mut EditorRuntime, edit: BuildingEdit) {
    match edit_document(&runtime.document, edit) {
        Ok((document, plan)) => {
            runtime.undo.push(runtime.document.clone());
            runtime.redo.clear();
            runtime.document = document;
            runtime.plan = plan;
            runtime.selected = None;
            runtime.hovered = None;
            runtime.pending_rebuild = true;
            runtime.status = "Edit applied and full building audit passed".to_owned();
            runtime.error = None;
        }
        Err(error) => runtime.error = Some(error.to_string()),
    }
}

fn editor_owner_targets(
    plan: &BuildingPlan,
) -> (
    std::collections::HashMap<u32, EditorTarget>,
    std::collections::HashMap<u64, EditorTarget>,
) {
    let mut owner_targets = std::collections::HashMap::<u32, EditorTarget>::new();
    let mut item_targets = std::collections::HashMap::<u64, EditorTarget>::new();
    for wall in &plan.wall_assemblies {
        if let WallSourceId::StoreyWall {
            storey_level,
            wall_index,
        } = wall.source
            && let Some(segment) = plan
                .storeys
                .iter()
                .find(|storey| storey.level == storey_level)
                .and_then(|storey| storey.walls.get(wall_index))
        {
            owner_targets.insert(
                wall.owner.0,
                EditorTarget::Wall(WallSelector {
                    storey_level,
                    cell: segment.cell,
                    direction: segment.direction,
                }),
            );
        }
    }
    for opening in &plan.opening_assemblies {
        if let WallSourceId::StoreyWall {
            storey_level,
            wall_index,
        } = opening.host_source
            && let Some(segment) = plan
                .storeys
                .iter()
                .find(|storey| storey.level == storey_level)
                .and_then(|storey| storey.walls.get(wall_index))
        {
            let target = EditorTarget::Opening(WallSelector {
                storey_level,
                cell: segment.cell,
                direction: segment.direction,
            });
            let mut ids = opening.jamb_solids.to_vec();
            ids.extend([
                opening.head_solid,
                opening.spandrel_solid,
                opening.wall_above_interface,
            ]);
            ids.extend(opening.sill_solid);
            ids.extend(opening.closure_solids.iter().copied());
            ids.extend(opening.reveal_surfaces.iter().copied());
            ids.extend(opening.mount_solid);
            ids.extend(opening.stance_surface);
            for id in ids {
                item_targets.insert(id.0, target);
            }
        }
    }
    if let Some(frame) = &plan.timber_frame {
        let wall_targets = plan
            .wall_assemblies
            .iter()
            .filter_map(|wall| match wall.source {
                WallSourceId::StoreyWall {
                    storey_level,
                    wall_index,
                } => plan
                    .storeys
                    .iter()
                    .find(|storey| storey.level == storey_level)
                    .and_then(|storey| storey.walls.get(wall_index))
                    .map(|segment| {
                        (
                            wall.id,
                            EditorTarget::Wall(WallSelector {
                                storey_level,
                                cell: segment.cell,
                                direction: segment.direction,
                            }),
                        )
                    }),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        let member_solids = frame
            .members
            .iter()
            .map(|member| (member.id, member.solid.0))
            .collect::<std::collections::HashMap<_, _>>();
        for bay in &frame.bays {
            let Some(target) = bay.wall.and_then(|wall| wall_targets.get(&wall)).copied() else {
                continue;
            };
            for item in bay
                .member_ids
                .iter()
                .filter_map(|member| member_solids.get(member).copied())
                .chain(bay.infill_solids.iter().map(|solid| solid.0))
            {
                item_targets.insert(item, target);
            }
        }
        for member in &frame.members {
            item_targets
                .entry(member.solid.0)
                .or_insert(EditorTarget::TimberMember(member.id.0));
        }
    }
    (owner_targets, item_targets)
}

fn configure_editor_scene(world: &mut World, plan: &BuildingPlan, initialize_camera: bool) {
    let (owner_targets, item_targets) = editor_owner_targets(plan);
    let wall_storeys = plan
        .wall_assemblies
        .iter()
        .map(|wall| (wall.owner.0, usize::from(wall.storey_level)))
        .collect::<std::collections::HashMap<_, _>>();
    let wall_finish_by_owner = plan
        .wall_assemblies
        .iter()
        .filter_map(|assembly| match assembly.source {
            WallSourceId::StoreyWall {
                storey_level,
                wall_index,
            } => plan
                .storeys
                .iter()
                .find(|storey| storey.level == storey_level)
                .and_then(|storey| storey.walls.get(wall_index))
                .and_then(|segment| {
                    plan.wall_style_overrides
                        .iter()
                        .find(|override_| {
                            override_.wall
                                == WallSelector {
                                    storey_level,
                                    cell: segment.cell,
                                    direction: segment.direction,
                                }
                        })
                        .map(|override_| (assembly.owner.0, override_.style))
                }),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    let timber_wall_storeys =
        plan.timber_frame
            .as_ref()
            .map_or_else(std::collections::HashMap::new, |frame| {
                let wall_levels = plan
                    .wall_assemblies
                    .iter()
                    .map(|wall| (wall.id, usize::from(wall.storey_level)))
                    .collect::<std::collections::HashMap<_, _>>();
                let member_solids = frame
                    .members
                    .iter()
                    .map(|member| (member.id, member.solid.0))
                    .collect::<std::collections::HashMap<_, _>>();
                frame
                    .bays
                    .iter()
                    .filter_map(|bay| {
                        bay.wall
                            .and_then(|wall| wall_levels.get(&wall).copied())
                            .map(|level| (bay, level))
                    })
                    .flat_map(|(bay, level)| {
                        bay.member_ids
                            .iter()
                            .filter_map(|member| member_solids.get(member).copied())
                            .chain(bay.infill_solids.iter().map(|solid| solid.0))
                            .map(move |item| (item, level))
                    })
                    .collect()
            });
    let timber_floor_storeys =
        plan.timber_frame
            .as_ref()
            .map_or_else(std::collections::HashMap::new, |frame| {
                frame
                    .floors
                    .iter()
                    .flat_map(|floor| {
                        std::iter::once(floor.floor_solid)
                            .chain(floor.floor_solids.iter().copied())
                            .map(move |solid| (solid.0, usize::from(floor.level)))
                    })
                    .collect::<std::collections::HashMap<_, _>>()
            });
    let roof_storey = plan.storeys.len();
    let roof_storeys = plan
        .roof_assemblies
        .iter()
        .map(|roof| (roof.owner.0, roof_storey))
        .collect::<std::collections::HashMap<_, _>>();

    let mesh_entities = {
        let mut query = world.query_filtered::<(
            Entity,
            Option<&GeometryOwner>,
            Option<&ResolvedRenderItem>,
            Option<&RoofRenderItem>,
            Option<&MeshMaterial3d<StandardMaterial>>,
            Option<&Name>,
            Option<&Transform>,
        ), Without<EditorEnvironmentEntity>>();
        query
            .iter(world)
            .filter(|(entity, ..)| world.get::<Mesh3d>(*entity).is_some())
            .map(|(entity, owner, item, roof, material, name, transform)| {
                (
                    entity,
                    owner.map(|owner| owner.0),
                    item.map(|item| item.id),
                    roof.is_some() || name.is_some_and(|name| name.as_str().contains("gable")),
                    material.map(|material| material.0.clone()),
                    name.is_some_and(|name| name.as_str() == "room floor"),
                    transform.map(|transform| transform.translation.y),
                )
            })
            .collect::<Vec<_>>()
    };
    for (entity, owner, item, is_roof, material, is_room_floor, elevation) in mesh_entities {
        let hide_fachwerk = item
            .and_then(|item| item_targets.get(&item).copied())
            .and_then(|target| match target {
                EditorTarget::Wall(wall) => plan
                    .wall_style_overrides
                    .iter()
                    .find(|override_| override_.wall == wall)
                    .map(|override_| override_.style != WallStyle::TimberFrame),
                _ => None,
            })
            .unwrap_or(false);
        let material = if !is_roof {
            owner
                .and_then(|owner| wall_finish_by_owner.get(&owner).copied())
                .map(|style| {
                    let colour = match style {
                        WallStyle::TimberFrame | WallStyle::Plaster => {
                            Color::srgb(0.72, 0.66, 0.53)
                        }
                        WallStyle::Brick => Color::srgb(0.48, 0.23, 0.16),
                        WallStyle::Stone => Color::srgb(0.42, 0.40, 0.36),
                    };
                    world
                        .resource_mut::<Assets<StandardMaterial>>()
                        .add(StandardMaterial {
                            base_color: colour,
                            perceptual_roughness: 0.82,
                            ..default()
                        })
                })
                .or(material)
        } else {
            material
        };
        let mut entity_mut = world.entity_mut(entity);
        entity_mut.insert(EditorBuildingEntity);
        let visibility_target = if is_roof {
            Some(EditorVisibilityTarget {
                storey: owner
                    .and_then(|owner| roof_storeys.get(&owner).copied())
                    .unwrap_or(roof_storey),
                role: EditorVisibilityRole::Roof,
            })
        } else {
            owner
                .and_then(|owner| wall_storeys.get(&owner).copied())
                .map(|storey| EditorVisibilityTarget {
                    storey,
                    role: EditorVisibilityRole::Wall,
                })
                .or_else(|| {
                    item.and_then(|item| timber_wall_storeys.get(&item).copied())
                        .map(|storey| EditorVisibilityTarget {
                            storey,
                            role: EditorVisibilityRole::Wall,
                        })
                })
                .or_else(|| {
                    item.and_then(|item| timber_floor_storeys.get(&item).copied())
                        .map(|storey| EditorVisibilityTarget {
                            storey,
                            role: EditorVisibilityRole::Floor,
                        })
                })
                .or_else(|| {
                    is_room_floor.then(|| EditorVisibilityTarget {
                        storey: (elevation.unwrap_or_default() / plan.storey_height_metres)
                            .floor()
                            .max(0.0) as usize,
                        role: EditorVisibilityRole::Floor,
                    })
                })
                // Resolved timber, joists, braces, and other structural parts
                // do not always carry a wall/roof owner. Their centre height
                // still has a stable storey meaning in the editor, so never
                // leave them outside the level-visibility contract.
                .or_else(|| {
                    elevation.map(|elevation| EditorVisibilityTarget {
                        storey: (elevation / plan.storey_height_metres).floor().max(0.0) as usize,
                        role: EditorVisibilityRole::Structure,
                    })
                })
        };
        if let (Some(target), Some(material)) = (visibility_target, material) {
            entity_mut.insert((
                target,
                EditorBaseMaterial(material),
                EditorAppearanceIsTranslucent(false),
                Visibility::Visible,
            ));
        }
        if hide_fachwerk {
            entity_mut.insert(EditorFachwerkForFinishedWall);
        }
        let target = item
            .and_then(|item| item_targets.get(&item).copied())
            .or_else(|| owner.and_then(|owner| owner_targets.get(&owner).copied()));
        if let Some(target) = target {
            entity_mut.insert((
                EditorSelectable(target),
                OutlineVolume {
                    visible: false,
                    colour: Color::WHITE,
                    width: 4.0,
                },
                OutlineMode::FloodFlat,
            ));
        } else {
            entity_mut.insert(Pickable::IGNORE);
        }
    }

    if !initialize_camera {
        return;
    }

    let focus = Vec3::new(
        0.0,
        plan.storey_height_metres * plan.storeys.len() as f32 * 0.45,
        0.0,
    );
    let camera_entities = {
        let mut query = world.query_filtered::<Entity, With<Camera3d>>();
        query.iter(world).collect::<Vec<_>>()
    };
    for entity in camera_entities {
        let transform = *world
            .get::<Transform>(entity)
            .expect("editor camera must have a transform");
        let radius = transform.translation.distance(focus).max(3.0);
        world.entity_mut(entity).insert(PanOrbitCamera {
            focus,
            target_focus: focus,
            radius: Some(radius),
            target_radius: radius,
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            zoom_lower_limit: 0.5,
            ..default()
        });
    }
}
