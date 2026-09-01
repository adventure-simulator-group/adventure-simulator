#[allow(clippy::too_many_arguments)]
fn spawn_wall(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    opening: Option<&Opening>,
    origin: Vec2,
    base_y: f32,
    storey_height: f32,
    style: WallStyle,
    interior_finish: InteriorWallFinish,
    timber_frame_style: Option<TimberFrameStyle>,
    projection_metres: f32,
) {
    let mut centre = wall.centre() + origin;
    let horizontal = wall.is_horizontal();
    let outward = match wall.direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    };
    if wall.exterior() {
        centre += outward * projection_metres;
    }
    if !wall.exterior() {
        let material = match interior_finish {
            InteriorWallFinish::Plastered | InteriorWallFinish::ExposedFrame => &palette.plaster,
            InteriorWallFinish::Boarded => &palette.timber,
        };
        spawn_wall_body(
            world,
            material,
            horizontal,
            centre,
            base_y,
            storey_height,
            0.09,
            opening,
        );
        if let Some(opening) = opening {
            spawn_opening_depth(
                world, palette, wall, *opening, horizontal, centre, outward, base_y,
            );
        }
        if interior_finish == InteriorWallFinish::ExposedFrame {
            spawn_timber_frame(
                world,
                palette,
                wall,
                timber_frame_style.unwrap_or(TimberFrameStyle::LateMedieval),
                horizontal,
                CELL_SIZE_METRES,
                centre,
                base_y,
                storey_height,
                opening,
            );
        }
        return;
    }
    let material = match style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    spawn_wall_body(
        world,
        material,
        horizontal,
        centre,
        base_y,
        storey_height,
        WALL_THICKNESS_METRES,
        opening,
    );
    if let Some(opening) = opening {
        spawn_opening_depth(
            world, palette, wall, *opening, horizontal, centre, outward, base_y,
        );
    }

    if style == WallStyle::TimberFrame && wall.exterior() {
        let timber_centre = centre + outward * (WALL_THICKNESS_METRES + 0.015);
        spawn_timber_frame(
            world,
            palette,
            wall,
            timber_frame_style.unwrap_or(TimberFrameStyle::LateMedieval),
            horizontal,
            CELL_SIZE_METRES,
            timber_centre,
            base_y,
            storey_height,
            opening,
        );
        if projection_metres > 0.01 {
            let tangent = if horizontal { Vec2::X } else { Vec2::Y };
            for sign in [-0.38, 0.38] {
                let anchor = timber_centre + tangent * CELL_SIZE_METRES * sign;
                let lower = anchor - outward * projection_metres;
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(lower.x, base_y - 0.42, lower.y),
                    Vec3::new(anchor.x, base_y + 0.08, anchor.y),
                    0.11,
                    "projecting storey bracket",
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_opening_depth(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    opening: Opening,
    horizontal: bool,
    centre: Vec2,
    outward: Vec2,
    base_y: f32,
) {
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let recess = match opening.kind {
        OpeningKind::ArrowSlit => WALL_THICKNESS_METRES * 0.46,
        OpeningKind::Window => WALL_THICKNESS_METRES * 0.34,
        OpeningKind::Door | OpeningKind::Gate => WALL_THICKNESS_METRES * 0.18,
    };
    let plane_centre = centre - outward * recess;
    let plane_size = if horizontal {
        Vec3::new(
            opening.width_metres * 0.9,
            opening.height_metres * 0.94,
            0.025,
        )
    } else {
        Vec3::new(
            0.025,
            opening.height_metres * 0.94,
            opening.width_metres * 0.9,
        )
    };
    let material = match opening.kind {
        OpeningKind::Window => &palette.glass,
        OpeningKind::ArrowSlit => &palette.void,
        OpeningKind::Door | OpeningKind::Gate => &palette.door,
    };
    let plane = spawn_box(
        world,
        material,
        plane_size,
        Vec3::new(
            plane_centre.x,
            base_y + opening.sill_metres + opening.height_metres * 0.5,
            plane_centre.y,
        ),
        Quat::IDENTITY,
        match opening.kind {
            OpeningKind::Window => "recessed glazing",
            OpeningKind::ArrowSlit => "open firing-loop void",
            OpeningKind::Door => "recessed door leaf",
            OpeningKind::Gate => "recessed gate leaf",
        },
    );
    if matches!(opening.kind, OpeningKind::Door | OpeningKind::Gate) {
        // The leaf is an operable visual state, not permanent wall material.
        // Circulation and editor inspection therefore see the clear doorway.
        world
            .entity_mut(plane)
            .remove::<PlayerBuildRenderPrism>()
            .insert(NonCollidingVisualization);
    }

    if opening.kind == OpeningKind::Window && wall.exterior() {
        let face = centre + outward * (WALL_THICKNESS_METRES * 0.56);
        let jamb_offset = opening.width_metres * 0.5;
        for sign in [-1.0, 1.0] {
            let jamb = face + tangent * jamb_offset * sign;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(jamb.x, base_y + opening.sill_metres, jamb.y),
                Vec3::new(
                    jamb.x,
                    base_y + opening.sill_metres + opening.height_metres,
                    jamb.y,
                ),
                0.075,
                "window jamb",
            );
        }
        for y in [
            base_y + opening.sill_metres,
            base_y + opening.sill_metres + opening.height_metres,
        ] {
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(
                    face.x - tangent.x * jamb_offset,
                    y,
                    face.y - tangent.y * jamb_offset,
                ),
                Vec3::new(
                    face.x + tangent.x * jamb_offset,
                    y,
                    face.y + tangent.y * jamb_offset,
                ),
                0.075,
                "window sill or lintel",
            );
        }
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(face.x, base_y + opening.sill_metres, face.y),
            Vec3::new(
                face.x,
                base_y + opening.sill_metres + opening.height_metres,
                face.y,
            ),
            0.045,
            "window mullion",
        );
        let transom_y = base_y + opening.sill_metres + opening.height_metres * 0.52;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                face.x - tangent.x * jamb_offset,
                transom_y,
                face.y - tangent.y * jamb_offset,
            ),
            Vec3::new(
                face.x + tangent.x * jamb_offset,
                transom_y,
                face.y + tangent.y * jamb_offset,
            ),
            0.045,
            "window transom",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_timber_frame(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    style: TimberFrameStyle,
    horizontal: bool,
    bay_width: f32,
    centre: Vec2,
    base_y: f32,
    height: f32,
    opening: Option<&Opening>,
) {
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let point = |along: f32, y: f32| {
        let plan = centre + tangent * along;
        Vec3::new(plan.x, y, plan.y)
    };
    let half = bay_width * 0.5;
    for along in [-half, half] {
        spawn_timber_beam(
            world,
            &palette.timber,
            point(along, base_y),
            point(along, base_y + height),
            0.11,
            "timber post",
        );
    }
    if let Some(opening) = opening {
        let sill = base_y + opening.sill_metres;
        let header = sill + opening.height_metres;
        for y in [base_y, sill, header.min(base_y + height), base_y + height] {
            spawn_timber_beam(
                world,
                &palette.timber,
                point(-half, y),
                point(half, y),
                0.10,
                "opening-aware timber rail",
            );
        }
        let jamb = opening.width_metres * 0.5;
        for along in [-jamb, jamb] {
            spawn_timber_beam(
                world,
                &palette.timber,
                point(along, base_y),
                point(along, base_y + height),
                0.09,
                "opening-aware timber stud",
            );
        }
        if opening.kind == OpeningKind::Window {
            for (start, end) in [(-half, jamb), (half, -jamb)] {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(start, base_y + 0.05),
                    point(end, sill - 0.04),
                    0.085,
                    "brace below window",
                );
            }
            for (start, end) in [(-jamb, -half), (jamb, half)] {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(start, header + 0.04),
                    point(end, base_y + height - 0.05),
                    0.08,
                    "brace above window",
                );
            }
        }
        return;
    }
    let rail_fractions: &[f32] = match style {
        TimberFrameStyle::LateMedieval => &[0.0, 0.55, 1.0],
        TimberFrameStyle::NorthernCloseStudded => &[0.0, 0.48, 0.72, 1.0],
        TimberFrameStyle::EarlyModernOrnate => &[0.0, 0.36, 0.68, 1.0],
    };
    for fraction in rail_fractions {
        spawn_timber_beam(
            world,
            &palette.timber,
            point(-half, base_y + height * fraction),
            point(half, base_y + height * fraction),
            0.10,
            "timber rail",
        );
    }
    match style {
        TimberFrameStyle::LateMedieval => {
            let rising = (i32::from(wall.cell.x) + i32::from(wall.cell.z)).rem_euclid(2) == 0;
            let (a, b) = if rising { (-half, half) } else { (half, -half) };
            spawn_timber_beam(
                world,
                &palette.timber,
                point(a, base_y + 0.06),
                point(b, base_y + height - 0.06),
                0.13,
                "long diagonal brace",
            );
        }
        TimberFrameStyle::NorthernCloseStudded => {
            for along in [-half * 0.5, 0.0, half * 0.5] {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(along, base_y),
                    point(along, base_y + height),
                    0.075,
                    "close stud",
                );
            }
            spawn_timber_beam(
                world,
                &palette.timber,
                point(-half, base_y + 0.08),
                point(half, base_y + height * 0.48),
                0.09,
                "northern foot brace",
            );
        }
        TimberFrameStyle::EarlyModernOrnate => {
            let bay_key = if horizontal {
                i32::from(wall.cell.x)
            } else {
                i32::from(wall.cell.z)
            }
            .rem_euclid(4);
            let lower = base_y + height * 0.04;
            let waist = base_y + height * 0.54;
            let upper = base_y + height * 0.96;
            if bay_key == 0 {
                for start in [-half, half] {
                    spawn_timber_beam(
                        world,
                        &palette.timber,
                        point(start, lower),
                        point(0.0, waist),
                        0.11,
                        "Mann figure foot brace",
                    );
                }
                for start in [-half, half] {
                    spawn_timber_beam(
                        world,
                        &palette.timber,
                        point(start, upper),
                        point(0.0, waist),
                        0.09,
                        "Mann figure head brace",
                    );
                }
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(0.0, base_y),
                    point(0.0, base_y + height),
                    0.095,
                    "ornate central post",
                );
            } else if bay_key == 2 {
                let breast = base_y + height * 0.36;
                for (start, end) in [(-half, half), (half, -half)] {
                    spawn_timber_beam(
                        world,
                        &palette.timber,
                        point(start, lower),
                        point(end, breast),
                        0.085,
                        "Andreaskreuz breast-panel brace",
                    );
                }
            } else if bay_key == 3 {
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    point(-half, lower),
                    point(half, waist),
                    0.095,
                    "K figure foot brace",
                );
            }
        }
    }
}

fn spawn_timber_beam(
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
    spawn_box(
        world,
        material,
        Vec3::new(thickness, length, thickness),
        (start + end) * 0.5,
        Quat::from_rotation_arc(Vec3::Y, delta / length),
        name,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_wall_body(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    centre: Vec2,
    base_y: f32,
    storey_height: f32,
    depth: f32,
    opening: Option<&Opening>,
) {
    let Some(opening) = opening else {
        spawn_wall_box_with_depth(
            world,
            material,
            horizontal,
            CELL_SIZE_METRES,
            storey_height,
            depth,
            centre,
            base_y,
            "wall",
        );
        return;
    };
    let side_width = (CELL_SIZE_METRES - opening.width_metres) * 0.5;
    for sign in [-1.0, 1.0] {
        let offset = sign * (opening.width_metres + side_width) * 0.5;
        let pier_centre = if horizontal {
            centre + Vec2::X * offset
        } else {
            centre + Vec2::Y * offset
        };
        spawn_wall_box_with_depth(
            world,
            material,
            horizontal,
            side_width,
            storey_height,
            depth,
            pier_centre,
            base_y,
            "wall pier",
        );
    }
    if opening.sill_metres > 0.0 {
        spawn_wall_box_at_height_with_depth(
            world,
            material,
            horizontal,
            opening.width_metres,
            opening.sill_metres,
            depth,
            centre,
            base_y + opening.sill_metres * 0.5,
            "wall below opening",
        );
    }
    let header_base = opening.sill_metres + opening.height_metres;
    if header_base < storey_height {
        let header_height = storey_height - header_base;
        spawn_wall_box_at_height_with_depth(
            world,
            material,
            horizontal,
            opening.width_metres,
            header_height,
            depth,
            centre,
            base_y + header_base + header_height * 0.5,
            "wall header",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_wall_box_with_depth(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    length: f32,
    height: f32,
    depth: f32,
    centre: Vec2,
    base_y: f32,
    name: &'static str,
) {
    spawn_wall_box_at_height_with_depth(
        world,
        material,
        horizontal,
        length,
        height,
        depth,
        centre,
        base_y + height * 0.5,
        name,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_wall_box_at_height(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    length: f32,
    height: f32,
    centre: Vec2,
    y: f32,
    name: &'static str,
) {
    spawn_wall_box_at_height_with_depth(
        world,
        material,
        horizontal,
        length,
        height,
        WALL_THICKNESS_METRES,
        centre,
        y,
        name,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_wall_box_at_height_with_depth(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    length: f32,
    height: f32,
    depth: f32,
    centre: Vec2,
    y: f32,
    name: &'static str,
) {
    let size = if horizontal {
        Vec3::new(length.max(0.02), height.max(0.02), depth)
    } else {
        Vec3::new(depth, height.max(0.02), length.max(0.02))
    };
    spawn_box(
        world,
        material,
        size,
        Vec3::new(centre.x, y, centre.y),
        Quat::IDENTITY,
        name,
    );
}
