#[derive(Clone, Copy)]
enum EditorUiAction {
    ChangeArchetype(BuildingArchetype),
    AddOpening(WallSelector, OpeningKind),
    RemoveOpening(WallSelector),
    SetWallStyle(WallSelector, WallStyle),
    SetTimberStyle(TimberFrameStyle),
    NewPlayerBuild,
    DetachPlayerBuild,
    UpdatePlayerRoof(usize),
    SetPlayerInteriorWallFinish(InteriorWallFinish),
    Undo,
    Redo,
    Save,
    Load,
    SetMode(EditorMode),
    CycleWalls,
    PreviousStorey,
    NextStorey,
    MovePlayerPart(u64),
    ResizePlayerPart(u64),
    RotatePlayerPart(u64),
    RemovePlayerPart(u64),
}

fn editor_target_label(target: EditorTarget) -> String {
    match target {
        EditorTarget::Wall(wall) => format!(
            "Wall L{} ({}, {}) {:?}",
            wall.storey_level, wall.cell.x, wall.cell.z, wall.direction
        ),
        EditorTarget::Opening(wall) => format!(
            "Opening L{} ({}, {}) {:?}",
            wall.storey_level, wall.cell.x, wall.cell.z, wall.direction
        ),
        EditorTarget::TimberMember(id) => format!("Timber member {id}"),
    }
}

fn editor_pointer_over(
    event: On<Pointer<Over>>,
    selectables: Query<&EditorSelectable>,
    mut runtime: ResMut<EditorRuntime>,
) {
    if let Ok(selectable) = selectables.get(event.entity) {
        runtime.hovered = Some(selectable.0);
    }
}

fn editor_pointer_out(
    event: On<Pointer<Out>>,
    selectables: Query<&EditorSelectable>,
    mut runtime: ResMut<EditorRuntime>,
) {
    if let Ok(selectable) = selectables.get(event.entity)
        && runtime.hovered == Some(selectable.0)
    {
        runtime.hovered = None;
    }
}

fn editor_pointer_click(
    event: On<Pointer<Click>>,
    selectables: Query<&EditorSelectable>,
    mut runtime: ResMut<EditorRuntime>,
) {
    if event.button == PointerButton::Primary
        && runtime.mode == EditorMode::Construct
        && runtime.player_build.is_some()
        && runtime.player_tool == PlayerBuildTool::FloorTile
        && let Some(position) = event.hit.position
    {
        let storey = runtime.active_storey as u16;
        let cell = floor_cell_from_point(Vec2::new(position.x, position.z));
        apply_player_build_edit(
            &mut runtime,
            PlayerBuildEdit::PlaceFloorTile { cell, storey },
        );
        return;
    }
    if event.button == PointerButton::Primary
        && let Ok(selectable) = selectables.get(event.entity)
    {
        runtime.selected = Some(selectable.0);
        runtime.status = editor_target_label(selectable.0);
        runtime.error = None;
    }
}

fn snap_wall_grid(point: Vec2) -> Vec2 {
    (point / CELL_SIZE_METRES).round() * CELL_SIZE_METRES
}

fn floor_cell_from_point(point: Vec2) -> Cell {
    Cell::new(
        (point.x / CELL_SIZE_METRES).floor() as i16,
        (point.y / CELL_SIZE_METRES).floor() as i16,
    )
}

fn place_dragged_wall(
    runtime: &mut EditorRuntime,
    start: Vec2,
    end: Vec2,
    material: PlayerBuildMaterial,
    storey: u16,
) {
    let (start, end, _, _) = wall_drag_spec(start, end);
    let Some(style) = player_build_wall_style(material) else {
        runtime.error = Some("that material is not a wall finish".to_owned());
        return;
    };
    apply_player_build_edit(
        runtime,
        PlayerBuildEdit::DrawWall {
            start: adventuresim_building_generator::GridPoint::new(
                (start.x / CELL_SIZE_METRES).round() as i32,
                (start.y / CELL_SIZE_METRES).round() as i32,
            ),
            end: adventuresim_building_generator::GridPoint::new(
                (end.x / CELL_SIZE_METRES).round() as i32,
                (end.y / CELL_SIZE_METRES).round() as i32,
            ),
            storey,
            style,
        },
    );
}

fn wall_drag_spec(start: Vec2, end: Vec2) -> (Vec2, Vec2, f32, f32) {
    let start = snap_wall_grid(start);
    let end = snap_wall_grid(end);
    let delta = end - start;
    let (end, rotation_degrees, length) = if delta.x.abs() >= delta.y.abs() {
        (
            Vec2::new(end.x, start.y),
            0.0,
            delta.x.abs().max(CELL_SIZE_METRES),
        )
    } else {
        (
            Vec2::new(start.x, end.y),
            90.0,
            delta.y.abs().max(CELL_SIZE_METRES),
        )
    };
    (start, end, rotation_degrees, length)
}

fn editor_wall_drag_start(event: On<Pointer<DragStart>>, mut runtime: ResMut<EditorRuntime>) {
    if event.button != PointerButton::Primary
        || runtime.mode != EditorMode::Construct
        || runtime.player_build.is_none()
        || runtime.player_tool != PlayerBuildTool::Wall
    {
        return;
    }
    let Some(position) = event.hit.position else {
        runtime.error = Some("Wall tool needs a world-space pick hit".to_owned());
        return;
    };
    runtime.wall_drag = Some(WallDrag {
        start: Vec2::new(position.x, position.z),
        camera: event.hit.camera,
    });
    runtime.wall_preview = Some(WallPreview {
        start: Vec2::new(position.x, position.z),
        end: Vec2::new(position.x, position.z),
    });
    runtime.status = "Drag to draw wall".to_owned();
}

fn editor_wall_drag_move(event: On<Pointer<Move>>, mut runtime: ResMut<EditorRuntime>) {
    let Some(drag) = runtime.wall_drag else {
        return;
    };
    if let Some(position) = event.hit.position {
        runtime.wall_preview = Some(WallPreview {
            start: drag.start,
            end: Vec2::new(position.x, position.z),
        });
    }
}

fn editor_wall_drag_end(
    event: On<Pointer<DragEnd>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut runtime: ResMut<EditorRuntime>,
) {
    let Some(drag) = runtime.wall_drag.take() else {
        return;
    };
    runtime.wall_preview = None;
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok((camera, transform)) = cameras.get(drag.camera) else {
        runtime.error = Some("Wall tool lost its picking camera".to_owned());
        return;
    };
    let Ok(ray) = camera.viewport_to_world(transform, event.pointer_location.position) else {
        runtime.error =
            Some("Wall tool could not project the pointer onto the build plane".to_owned());
        return;
    };
    let plane_y = runtime.active_storey as f32 * runtime.plan.storey_height_metres;
    let direction_y = ray.direction.y;
    if direction_y.abs() < 0.0001 {
        runtime.error = Some("Wall tool view is parallel to the build plane".to_owned());
        return;
    }
    let distance = (plane_y - ray.origin.y) / direction_y;
    if distance < 0.0 {
        runtime.error = Some("Wall tool build plane is behind the camera".to_owned());
        return;
    }
    let end = ray.get_point(distance);
    let material = runtime.player_material;
    let storey = runtime.active_storey as u16;
    place_dragged_wall(
        &mut runtime,
        drag.start,
        Vec2::new(end.x, end.z),
        material,
        storey,
    );
}

fn draw_wall_preview(mut gizmos: Gizmos, runtime: Res<EditorRuntime>) {
    let Some(preview) = runtime.wall_preview else {
        return;
    };
    let (start, end, rotation, length) = wall_drag_spec(preview.start, preview.end);
    let centre = Vec2::new((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
    let half_length = length * 0.5;
    let half_depth = WALL_THICKNESS_METRES * 0.5;
    let (half_x, half_z) = if rotation == 0.0 {
        (half_length, half_depth)
    } else {
        (half_depth, half_length)
    };
    let base = runtime.active_storey as f32 * runtime.plan.storey_height_metres;
    let top = base + runtime.player_height_metres;
    let corners = [
        Vec3::new(centre.x - half_x, base, centre.y - half_z),
        Vec3::new(centre.x + half_x, base, centre.y - half_z),
        Vec3::new(centre.x + half_x, base, centre.y + half_z),
        Vec3::new(centre.x - half_x, base, centre.y + half_z),
        Vec3::new(centre.x - half_x, top, centre.y - half_z),
        Vec3::new(centre.x + half_x, top, centre.y - half_z),
        Vec3::new(centre.x + half_x, top, centre.y + half_z),
        Vec3::new(centre.x - half_x, top, centre.y + half_z),
    ];
    for (from, to) in [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        gizmos.line(corners[from], corners[to], Color::WHITE);
    }
}

fn update_editor_outlines(
    runtime: Res<EditorRuntime>,
    mut outlines: Query<(&EditorSelectable, &mut OutlineVolume)>,
) {
    if !runtime.is_changed() {
        return;
    }
    for (selectable, mut outline) in &mut outlines {
        if runtime.selected == Some(selectable.0) {
            outline.visible = true;
            outline.colour = Color::WHITE;
            outline.width = 4.0;
        } else if runtime.hovered == Some(selectable.0) {
            outline.visible = true;
            outline.colour = Color::srgb(0.55, 0.55, 0.55);
            outline.width = 3.0;
        } else {
            outline.visible = false;
        }
    }
}
