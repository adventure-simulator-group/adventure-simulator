fn editor_ui(mut contexts: EguiContexts, mut runtime: ResMut<EditorRuntime>) -> Result {
    let mut action = None;
    egui::Area::new(egui::Id::new("building-editor-mode-strip"))
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .show(contexts.ctx_mut()?, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.menu_button("File", |ui| {
                    if runtime.player_build.is_none() && ui.button("New freeform build").clicked() {
                        action = Some(EditorUiAction::NewPlayerBuild);
                        ui.close();
                    }
                    if runtime.player_build.is_none()
                        && ui.button("Detach generated building").clicked()
                    {
                        action = Some(EditorUiAction::DetachPlayerBuild);
                        ui.close();
                    }
                    if ui.button("Save document").clicked() {
                        action = Some(EditorUiAction::Save);
                        ui.close();
                    }
                    if ui.button("Load document").clicked() {
                        action = Some(EditorUiAction::Load);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Fixtures", |ui| {
                        for archetype in BuildingArchetype::ALL {
                            if ui
                                .selectable_label(
                                    runtime.document.program.archetype == archetype,
                                    archetype.slug(),
                                )
                                .clicked()
                            {
                                action = Some(EditorUiAction::ChangeArchetype(archetype));
                                ui.close();
                            }
                        }
                    });
                });
                if ui
                    .add_enabled(!runtime.undo.is_empty(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    action = Some(EditorUiAction::Undo);
                }
                if ui
                    .add_enabled(!runtime.redo.is_empty(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y")
                    .clicked()
                {
                    action = Some(EditorUiAction::Redo);
                }
                ui.separator();
                for (mode, label, shortcut) in EditorMode::ALL {
                    let button = egui::Button::new(format!("{label} {shortcut}"));
                    let response = ui
                        .add_enabled(mode.is_available(), button.selected(runtime.mode == mode))
                        .on_disabled_hover_text(mode.availability())
                        .on_hover_text(mode.availability());
                    if response.clicked() {
                        action = Some(EditorUiAction::SetMode(mode));
                    }
                }
            });
        });
    egui::Area::new(egui::Id::new("building-editor-storeys"))
        .anchor(egui::Align2::LEFT_TOP, [8.0, 48.0])
        .show(contexts.ctx_mut()?, |ui| {
            ui.set_width(150.0);
            ui.strong("Storey");
            if ui.button("▲ Higher").on_hover_text("Page Up").clicked() {
                action = Some(EditorUiAction::NextStorey);
            }
            for level in (0..=runtime.highest_visible_storey()).rev() {
                let label = if level == runtime.highest_visible_storey()
                    && (runtime
                        .player_build
                        .as_ref()
                        .map_or(!runtime.plan.roofs.is_empty(), |document| {
                            !document.assembly.roofs.is_empty()
                        })) {
                    "Roof".to_owned()
                } else if level == 0 {
                    "Ground".to_owned()
                } else {
                    format!("Level {level}")
                };
                if ui
                    .selectable_label(runtime.active_storey == level, label)
                    .clicked()
                {
                    runtime.active_storey = level;
                    runtime.status = format!("Active storey: {level}");
                }
            }
            if ui.button("▼ Lower").on_hover_text("Page Down").clicked() {
                action = Some(EditorUiAction::PreviousStorey);
            }
            ui.separator();
            if ui
                .button(runtime.wall_visibility.label())
                .on_hover_text("Home")
                .clicked()
            {
                action = Some(EditorUiAction::CycleWalls);
            }
            ui.separator();
            ui.small("Visibility settings are retained while you edit this document.");
        });
    egui::Window::new("Inspector")
        .default_size([320.0, 560.0])
        .default_pos([VIEW_WIDTH as f32 - 340.0, 74.0])
        .resizable(true)
        .show(contexts.ctx_mut()?, |ui| {
            ui.strong(format!("{} mode", EditorMode::ALL
                .iter()
                .find(|(mode, _, _)| *mode == runtime.mode)
                .map(|(_, label, _)| *label)
                .unwrap_or("Select")));
            ui.small(EditorMode::availability(runtime.mode));
            ui.label(format!("Programme: {}", runtime.document.program.archetype.slug()));
            ui.small("MMB orbit · Shift+MMB pan · wheel zoom · F frame · Esc select");
            ui.separator();

            if let Some(selected) = runtime.selected {
                ui.label(editor_target_label(selected));
                match selected {
                    EditorTarget::Wall(wall) => {
                        ui.label("Opening type");
                        ui.horizontal_wrapped(|ui| {
                            for (kind, label) in [
                                (OpeningKind::Window, "Window"),
                                (OpeningKind::Door, "Door"),
                                (OpeningKind::Gate, "Gate"),
                                (OpeningKind::ArrowSlit, "Arrow slit"),
                            ] {
                                if ui
                                    .selectable_label(runtime.opening_kind == kind, label)
                                    .clicked()
                                {
                                    runtime.opening_kind = kind;
                                    match kind {
                                        OpeningKind::Window => {
                                            runtime.window_width_metres = 0.8;
                                            runtime.window_sill_metres = 0.9;
                                            runtime.window_height_metres = 1.1;
                                        }
                                        OpeningKind::Door => {
                                            runtime.window_width_metres = 0.95;
                                            runtime.window_sill_metres = 0.0;
                                            runtime.window_height_metres = 2.1;
                                        }
                                        OpeningKind::Gate => {
                                            runtime.window_width_metres = 2.4;
                                            runtime.window_sill_metres = 0.0;
                                            runtime.window_height_metres = 2.8;
                                        }
                                        OpeningKind::ArrowSlit => {
                                            runtime.window_width_metres = 0.25;
                                            runtime.window_sill_metres = 1.2;
                                            runtime.window_height_metres = 1.0;
                                        }
                                    }
                                }
                            }
                        });
                        ui.add(
                            egui::DragValue::new(&mut runtime.window_width_metres)
                                .range(0.35..=1.2)
                                .speed(0.05)
                                .prefix("width ")
                                .suffix(" m"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut runtime.window_sill_metres)
                                .range(0.3..=2.2)
                                .speed(0.05)
                                .prefix("sill ")
                                .suffix(" m"),
                        );
                        ui.add(
                            egui::DragValue::new(&mut runtime.window_height_metres)
                                .range(0.45..=1.8)
                                .speed(0.05)
                                .prefix("height ")
                                .suffix(" m"),
                        );
                        if ui.button("Place opening").clicked() {
                            action = Some(EditorUiAction::AddOpening(wall, runtime.opening_kind));
                        }
                        ui.small("Doors, gates, and arrow slits are audited against their wall. Arches and freeform walls are part of the player-build document.");
                    }
                    EditorTarget::Opening(wall) => {
                        if ui.button("Remove opening").clicked() {
                            action = Some(EditorUiAction::RemoveOpening(wall));
                        }
                    }
                    EditorTarget::TimberMember(_) => {
                        ui.label("Fachwerk pattern (building scope)");
                        let current = runtime
                            .document
                            .program
                            .timber_frame_style
                            .unwrap_or(TimberFrameStyle::LateMedieval);
                        for (style, label) in [
                            (TimberFrameStyle::LateMedieval, "Late medieval"),
                            (
                                TimberFrameStyle::NorthernCloseStudded,
                                "Northern close-studded",
                            ),
                            (TimberFrameStyle::EarlyModernOrnate, "Early modern ornate"),
                        ] {
                            if ui.selectable_label(current == style, label).clicked() {
                                action = Some(EditorUiAction::SetTimberStyle(style));
                            }
                        }
                    }
                }
            } else {
                ui.label("Hover a feature, then click to inspect it.");
            }

            if runtime.player_build.is_some() {
                let player_parts = runtime
                    .player_build
                    .as_ref()
                    .map(|document| document.parts.clone())
                    .unwrap_or_default();
                ui.separator();
                ui.strong("Freeform player build");
                ui.small("Walls and floors are semantic building assembly, not render parts.");
                ui.horizontal_wrapped(|ui| {
                    for (tool, label) in [
                        (PlayerBuildTool::Wall, "Wall"),
                        (PlayerBuildTool::FloorTile, "Floor tile"),
                    ] {
                        if ui.selectable_label(runtime.player_tool == tool, label).clicked() {
                            runtime.player_tool = tool;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (material, label) in [
                        (PlayerBuildMaterial::Stone, "Stone"),
                        (PlayerBuildMaterial::Brick, "Brick"),
                        (PlayerBuildMaterial::Plaster, "Plaster"),
                        (PlayerBuildMaterial::TimberFrame, "Frame"),
                    ] {
                        if ui
                            .selectable_label(runtime.player_material == material, label)
                            .clicked()
                        {
                            runtime.player_material = material;
                        }
                    }
                });
                ui.small("Drag to place walls. Click the ground to place a floor tile.");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Interior:");
                    let current = runtime
                        .player_build
                        .as_ref()
                        .map(|document| document.assembly.interior_wall_finish)
                        .unwrap_or(InteriorWallFinish::Plastered);
                    for (finish, label) in [
                        (InteriorWallFinish::Plastered, "Plaster"),
                        (InteriorWallFinish::Boarded, "Boards"),
                        (InteriorWallFinish::ExposedFrame, "Exposed frame"),
                    ] {
                        if ui.selectable_label(current == finish, label).clicked() {
                            action = Some(EditorUiAction::SetPlayerInteriorWallFinish(finish));
                        }
                    }
                });
                if runtime.mode == EditorMode::Roof {
                    ui.separator();
                    ui.strong("Roof pieces");
                    let roofs = runtime
                        .player_build
                        .as_ref()
                        .map(|document| document.assembly.roofs.clone())
                        .unwrap_or_default();
                    for (index, roof) in roofs.iter().copied().enumerate() {
                        if ui
                            .selectable_label(
                                runtime.selected_player_roof == Some(index),
                                format!("Roof {}: {:?}", index + 1, roof.kind),
                            )
                            .clicked()
                        {
                            runtime.selected_player_roof = Some(index);
                            runtime.player_roof = roof;
                        }
                    }
                    if let Some(index) = runtime.selected_player_roof {
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut runtime.player_roof.centre.x).prefix("x "));
                            ui.add(egui::DragValue::new(&mut runtime.player_roof.centre.y).prefix("z "));
                            ui.add(egui::DragValue::new(&mut runtime.player_roof.base_height_metres).prefix("base "));
                        });
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut runtime.player_roof.size.x).prefix("width ").range(0.1..=100.0));
                            ui.add(egui::DragValue::new(&mut runtime.player_roof.size.y).prefix("depth ").range(0.1..=100.0));
                            ui.add(egui::DragValue::new(&mut runtime.player_roof.pitch_degrees).prefix("pitch ").range(0.0..=85.0).suffix("°"));
                        });
                        ui.horizontal_wrapped(|ui| {
                            for (kind, label) in [(RoofKind::Gable, "Gable"), (RoofKind::Hip, "Hip"), (RoofKind::Shed, "Shed"), (RoofKind::Flat, "Flat")] {
                                if ui.selectable_label(runtime.player_roof.kind == kind, label).clicked() {
                                    runtime.player_roof.kind = kind;
                                }
                            }
                        });
                        if ui.button("Apply roof").clicked() {
                            action = Some(EditorUiAction::UpdatePlayerRoof(index));
                        }
                    }
                }
                egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                    for part in &player_parts {
                        if ui
                            .selectable_label(
                                runtime.selected_player_part == Some(part.id),
                                format!("#{} {:?} L{}", part.id, part.kind, part.storey),
                            )
                            .clicked()
                        {
                            runtime.selected_player_part = Some(part.id);
                            runtime.player_x_metres = part.x_metres;
                            runtime.player_z_metres = part.z_metres;
                            runtime.player_elevation_metres = part.elevation_metres;
                            runtime.player_width_metres = part.width_metres;
                            runtime.player_depth_metres = part.depth_metres;
                            runtime.player_height_metres = part.height_metres;
                            runtime.player_rotation_degrees = part.rotation_degrees;
                        }
                    }
                });
                if let Some(id) = runtime.selected_player_part {
                    ui.horizontal(|ui| {
                        if ui.button("Move").clicked() { action = Some(EditorUiAction::MovePlayerPart(id)); }
                        if ui.button("Resize").clicked() { action = Some(EditorUiAction::ResizePlayerPart(id)); }
                        if ui.button("Rotate").clicked() { action = Some(EditorUiAction::RotatePlayerPart(id)); }
                        if ui.button("Remove").clicked() { action = Some(EditorUiAction::RemovePlayerPart(id)); }
                    });
                }
                let advice_document = runtime
                    .player_build
                    .clone()
                    .unwrap_or_else(PlayerBuildDocument::empty);
                for advice in analyse_player_build(&advice_document) {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 150, 70),
                        format!("Advice: {advice:?}"),
                    );
                }
            } else {
                ui.separator();
                ui.strong("Freeform build");
                ui.label("Create a freeform build to use Construct tools such as drag-to-draw walls.");
                if ui.button("New freeform build").clicked() {
                    action = Some(EditorUiAction::NewPlayerBuild);
                }
                if ui.button("Detach generated building").clicked() {
                    action = Some(EditorUiAction::DetachPlayerBuild);
                }
                ui.small("Detaching copies the editable storeys, walls, openings, roofs, and finishes. It cannot be converted back into a programme.");
            }

            ui.separator();
            ui.label("Wall finish");
            let selected_wall = runtime.selected.and_then(|target| match target {
                EditorTarget::Wall(wall) => Some(wall),
                EditorTarget::Opening(wall) => Some(wall),
                EditorTarget::TimberMember(_) => None,
            });
            let current_wall = selected_wall
                .and_then(|wall| {
                    runtime
                        .plan
                        .wall_style_overrides
                        .iter()
                        .find(|override_| override_.wall == wall)
                        .map(|override_| override_.style)
                })
                .unwrap_or(runtime.document.program.wall_style);
            let civilian = matches!(
                runtime.document.program.archetype,
                BuildingArchetype::TownHouse
                    | BuildingArchetype::HallHouse
                    | BuildingArchetype::FachwerkCottage
                    | BuildingArchetype::FachwerkMerchantHouse
                    | BuildingArchetype::RenaissanceTownHall
            );
            ui.add_enabled_ui(civilian && selected_wall.is_some(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (style, label) in [
                        (WallStyle::TimberFrame, "Timber/plaster"),
                        (WallStyle::Plaster, "Plaster"),
                        (WallStyle::Brick, "Brick"),
                        (WallStyle::Stone, "Stone"),
                    ] {
                        if ui.selectable_label(current_wall == style, label).clicked() {
                            action = selected_wall
                                .map(|wall| EditorUiAction::SetWallStyle(wall, style));
                        }
                    }
                });
            });
            if !civilian {
                ui.small("The selected fixture's structural material is fixed.");
            } else if selected_wall.is_none() {
                ui.small("Select a wall or its fachwerk to change that wall's finish.");
            }

            ui.separator();
            ui.label(&runtime.status);
            if let Some(error) = &runtime.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
            }
        });

    if let Some(action) = action {
        perform_editor_action(&mut runtime, action);
    }
    Ok(())
}

fn perform_editor_action(runtime: &mut EditorRuntime, action: EditorUiAction) {
    match action {
        EditorUiAction::ChangeArchetype(archetype) => {
            let document = BuildingDocument::fixture(archetype, runtime.document.program.seed);
            match generate_document(&document) {
                Ok(plan) => {
                    runtime.undo.push(runtime.document.clone());
                    runtime.redo.clear();
                    runtime.document = document;
                    runtime.plan = plan;
                    runtime.selected = None;
                    runtime.hovered = None;
                    runtime.pending_rebuild = true;
                    runtime.status = format!("Loaded {:?} fixture", archetype);
                    runtime.error = None;
                }
                Err(error) => runtime.error = Some(error.to_string()),
            }
        }
        EditorUiAction::AddOpening(wall, kind) => apply_editor_edit(
            runtime,
            BuildingEdit::AddOpening {
                wall,
                opening_kind: kind,
                width_metres: runtime.window_width_metres,
                sill_metres: runtime.window_sill_metres,
                height_metres: runtime.window_height_metres,
            },
        ),
        EditorUiAction::RemoveOpening(wall) => {
            apply_editor_edit(runtime, BuildingEdit::RemoveOpening { wall });
        }
        EditorUiAction::SetWallStyle(wall, style) => {
            apply_editor_edit(runtime, BuildingEdit::SetWallMaterial { wall, style });
        }
        EditorUiAction::SetTimberStyle(style) => {
            apply_editor_edit(runtime, BuildingEdit::SetTimberFrameStyle { style });
        }
        EditorUiAction::UpdatePlayerRoof(index) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::UpdateRoof {
                index,
                roof: runtime.player_roof,
            },
        ),
        EditorUiAction::SetPlayerInteriorWallFinish(finish) => {
            apply_player_build_edit(runtime, PlayerBuildEdit::SetInteriorWallFinish { finish })
        }
        EditorUiAction::NewPlayerBuild => {
            let stem = runtime
                .document_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("building-document");
            runtime.player_build = Some(PlayerBuildDocument::empty());
            runtime.player_build_path = Some(
                runtime
                    .document_path
                    .with_file_name(format!("{stem}-player-build.json")),
            );
            runtime.selected_player_part = None;
            runtime.mode = EditorMode::Construct;
            runtime.show_generated_building = false;
            runtime.pending_rebuild = true;
            runtime.pending_player_rebuild = true;
            runtime.status = "New freeform build: drag in Construct mode to draw walls".to_owned();
            runtime.error = None;
        }
        EditorUiAction::DetachPlayerBuild => {
            let stem = runtime
                .document_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("building-document");
            runtime.player_build = Some(PlayerBuildDocument::from_plan(&runtime.plan));
            runtime.player_build_path = Some(
                runtime
                    .document_path
                    .with_file_name(format!("{stem}-player-build.json")),
            );
            runtime.selected_player_part = None;
            runtime.mode = EditorMode::Construct;
            runtime.show_generated_building = false;
            runtime.pending_rebuild = true;
            runtime.pending_player_rebuild = true;
            runtime.status = "Detached generated building into freeform assembly".to_owned();
            runtime.error = None;
        }
        EditorUiAction::Undo => {
            if let Some(previous) = runtime.undo.pop() {
                match generate_document(&previous) {
                    Ok(plan) => {
                        runtime.redo.push(runtime.document.clone());
                        runtime.document = previous;
                        runtime.plan = plan;
                        runtime.pending_rebuild = true;
                        runtime.status = "Undid edit".to_owned();
                        runtime.error = None;
                    }
                    Err(error) => runtime.error = Some(error.to_string()),
                }
            }
        }
        EditorUiAction::Redo => {
            if let Some(next) = runtime.redo.pop() {
                match generate_document(&next) {
                    Ok(plan) => {
                        runtime.undo.push(runtime.document.clone());
                        runtime.document = next;
                        runtime.plan = plan;
                        runtime.pending_rebuild = true;
                        runtime.status = "Redid edit".to_owned();
                        runtime.error = None;
                    }
                    Err(error) => runtime.error = Some(error.to_string()),
                }
            }
        }
        EditorUiAction::Save => {
            let saved_player_build = runtime
                .player_build
                .as_ref()
                .zip(runtime.player_build_path.as_ref())
                .map(|(document, path)| {
                    serde_json::to_vec_pretty(document)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| fs::write(path, bytes).map_err(|error| error.to_string()))
                        .map(|()| path.clone())
                });
            match saved_player_build.unwrap_or_else(|| {
                serde_json::to_vec_pretty(&runtime.document)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| {
                        fs::write(&runtime.document_path, bytes).map_err(|error| error.to_string())
                    })
                    .map(|()| runtime.document_path.clone())
            }) {
                Ok(path) => {
                    runtime.status = format!("Saved {}", path.display());
                    runtime.error = None;
                }
                Err(error) => runtime.error = Some(error),
            }
        }
        EditorUiAction::Load => match fs::read(&runtime.document_path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<BuildingDocument>(&bytes)
                    .map_err(|error| error.to_string())
            })
            .and_then(|document| {
                generate_document(&document)
                    .map(|plan| (document, plan))
                    .map_err(|error| error.to_string())
            }) {
            Ok((document, plan)) => {
                runtime.undo.push(runtime.document.clone());
                runtime.redo.clear();
                runtime.document = document;
                runtime.plan = plan;
                runtime.selected = None;
                runtime.pending_rebuild = true;
                runtime.status = format!("Loaded {}", runtime.document_path.display());
                runtime.error = None;
            }
            Err(error) => runtime.error = Some(error),
        },
        EditorUiAction::SetMode(mode) => {
            runtime.mode = mode;
            runtime.status = format!("{} mode", mode.availability());
            runtime.error = None;
        }
        EditorUiAction::CycleWalls => {
            runtime.wall_visibility = runtime.wall_visibility.next();
            runtime.status = runtime.wall_visibility.label().to_owned();
        }
        EditorUiAction::PreviousStorey => {
            runtime.active_storey = runtime.active_storey.saturating_sub(1);
            runtime.status = format!("Active storey: {}", runtime.active_storey);
        }
        EditorUiAction::NextStorey => {
            runtime.active_storey =
                (runtime.active_storey + 1).min(runtime.highest_visible_storey());
            runtime.status = format!("Active storey: {}", runtime.active_storey);
        }
        EditorUiAction::MovePlayerPart(id) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Move {
                id,
                x_metres: runtime.player_x_metres,
                z_metres: runtime.player_z_metres,
            },
        ),
        EditorUiAction::ResizePlayerPart(id) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Resize {
                id,
                width_metres: runtime.player_width_metres,
                depth_metres: runtime.player_depth_metres,
                height_metres: runtime.player_height_metres,
            },
        ),
        EditorUiAction::RotatePlayerPart(id) => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Rotate {
                id,
                rotation_degrees: runtime.player_rotation_degrees,
            },
        ),
        EditorUiAction::RemovePlayerPart(id) => {
            apply_player_build_edit(runtime, PlayerBuildEdit::Remove { id })
        }
    }
}
