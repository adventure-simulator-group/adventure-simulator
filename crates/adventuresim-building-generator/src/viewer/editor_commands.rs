fn editor_snapshot(runtime: &EditorRuntime) -> EditorSnapshot {
    let parts = runtime
        .player_build
        .as_ref()
        .map(|document| document.parts.clone())
        .unwrap_or_default();
    let advice_document = runtime
        .player_build
        .clone()
        .unwrap_or_else(PlayerBuildDocument::empty);
    EditorSnapshot {
        active_storey: runtime.active_storey,
        mode: runtime.mode,
        walls: runtime.wall_visibility,
        roof: runtime.roof_visibility,
        selected_part: runtime.selected_player_part,
        parts,
        advice: analyse_player_build(&advice_document)
            .into_iter()
            .map(|advice| format!("{advice:?}"))
            .collect(),
        status: runtime.status.clone(),
        error: runtime.error.clone(),
    }
}

fn perform_editor_command(runtime: &mut EditorRuntime, command: EditorCommand) {
    match command {
        EditorCommand::PlaceFloorTile {
            x_metres,
            z_metres,
            storey,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::PlaceFloorTile {
                cell: floor_cell_from_point(Vec2::new(x_metres, z_metres)),
                storey,
            },
        ),
        EditorCommand::DrawWall {
            start_x_metres,
            start_z_metres,
            end_x_metres,
            end_z_metres,
            material,
            storey,
        } => place_dragged_wall(
            runtime,
            Vec2::new(start_x_metres, start_z_metres),
            Vec2::new(end_x_metres, end_z_metres),
            material,
            storey,
        ),
        EditorCommand::PlacePart { part } => {
            apply_player_build_edit(runtime, PlayerBuildEdit::Place { part })
        }
        EditorCommand::MovePart {
            id,
            x_metres,
            z_metres,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Move {
                id,
                x_metres,
                z_metres,
            },
        ),
        EditorCommand::ResizePart {
            id,
            width_metres,
            depth_metres,
            height_metres,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Resize {
                id,
                width_metres,
                depth_metres,
                height_metres,
            },
        ),
        EditorCommand::RotatePart {
            id,
            rotation_degrees,
        } => apply_player_build_edit(
            runtime,
            PlayerBuildEdit::Rotate {
                id,
                rotation_degrees,
            },
        ),
        EditorCommand::RemovePart { id } => {
            apply_player_build_edit(runtime, PlayerBuildEdit::Remove { id })
        }
        EditorCommand::SetActiveStorey { storey } => {
            runtime.active_storey = storey.min(runtime.highest_visible_storey());
            runtime.status = format!("Active storey: {}", runtime.active_storey);
        }
        EditorCommand::CycleWalls => {
            runtime.wall_visibility = runtime.wall_visibility.next();
            runtime.status = runtime.wall_visibility.label().to_owned();
        }
        EditorCommand::CycleRoofs => {
            runtime.active_storey = runtime.highest_visible_storey();
            runtime.status = "Active storey: Roof".to_owned();
        }
    }
}

/// Executes a JSON array of [`EditorCommand`] values without opening a window.
/// This is the deterministic entry point used by CI and LLM-driven debugging.
pub(crate) fn run_editor_script(path: &std::path::Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let commands = serde_json::from_slice::<Vec<EditorCommand>>(&bytes)
        .map_err(|error| format!("invalid editor command script: {error}"))?;
    let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
    let plan = generate_document(&document).map_err(|error| error.to_string())?;
    let mut runtime = EditorRuntime::new(
        document,
        plan,
        PathBuf::from("building-document.json"),
        Some(PlayerBuildDocument::empty()),
        None,
    );
    let mut snapshots = Vec::with_capacity(commands.len());
    for command in commands {
        perform_editor_command(&mut runtime, command);
        snapshots.push(editor_snapshot(&runtime));
    }
    serde_json::to_string_pretty(&snapshots).map_err(|error| error.to_string())
}
