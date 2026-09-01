fn spawn_stair(world: &mut World, palette: &RenderPalette, stair: Stair, origin: Vec2) {
    match stair {
        Stair::Straight {
            start,
            direction,
            base_height_metres,
            rise_metres,
            width_metres,
            tread_count,
            run_metres,
        } => {
            let forward = direction_vector_2d(direction);
            let run = run_metres;
            let count = tread_count.max(1);
            let going = run / f32::from(count);
            let yaw = -forward.y.atan2(forward.x);
            let tread_yaw = forward.x.atan2(forward.y);
            let slope = rise_metres.atan2(run);
            let stringer_rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(slope);
            let lateral = Vec2::new(-forward.y, forward.x);
            // Match the resolved timber flight: two sloping stringers and
            // treads one rise above the lower floor, with the upper floor
            // itself acting as the final landing.
            for side in [-1.0_f32, 1.0] {
                let position = start
                    + origin
                    + forward * (run * 0.5)
                    + lateral * side * (width_metres * 0.5 - 0.0675);
                let entity = spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(run.hypot(rise_metres), 0.135, 0.135),
                    Vec3::new(
                        position.x,
                        base_height_metres + rise_metres * 0.5 - 0.03,
                        position.y,
                    ),
                    stringer_rotation,
                    "straight stair stringer",
                );
                world.entity_mut(entity).insert(PlayerBuildStairStringer);
            }
            for tread in 1..count {
                let progress = f32::from(tread) / f32::from(count);
                let position = start + origin + forward * (progress * run);
                spawn_box(
                    world,
                    &palette.stair,
                    Vec3::new(width_metres, 0.05, going * 0.96),
                    Vec3::new(
                        position.x,
                        base_height_metres + progress * rise_metres - 0.025,
                        position.y,
                    ),
                    Quat::from_rotation_y(tread_yaw),
                    "straight stair tread",
                );
            }
        }
        Stair::Spiral {
            centre,
            base_height_metres,
            rise_metres,
            inner_radius_metres,
            outer_radius_metres,
            turns,
            clockwise,
            tread_count,
        } => {
            let centre = centre + origin;
            spawn_box(
                world,
                &palette.stair,
                Vec3::new(
                    inner_radius_metres * 2.0,
                    rise_metres + 0.5,
                    inner_radius_metres * 2.0,
                ),
                Vec3::new(centre.x, base_height_metres + rise_metres * 0.5, centre.y),
                Quat::IDENTITY,
                "spiral stair newel",
            );
            for tread in 0..tread_count {
                let progress = tread as f32 / tread_count.max(1) as f32;
                let handedness = if clockwise { -1.0 } else { 1.0 };
                let angle = handedness * progress * turns * std::f32::consts::TAU;
                let radius = (inner_radius_metres + outer_radius_metres) * 0.5;
                let position = centre + Vec2::new(angle.cos(), angle.sin()) * radius;
                spawn_box(
                    world,
                    &palette.stair,
                    Vec3::new(outer_radius_metres - inner_radius_metres, 0.12, 0.32),
                    Vec3::new(
                        position.x,
                        base_height_metres + progress * rise_metres,
                        position.y,
                    ),
                    Quat::from_rotation_y(-angle),
                    "spiral stair tread",
                );
            }
        }
    }
}

fn spawn_wall_walk(world: &mut World, palette: &RenderPalette, wall_walk: WallWalk, origin: Vec2) {
    match wall_walk {
        WallWalk::Linear {
            start,
            end,
            elevation_metres,
            width_metres,
            outward,
        } => {
            let start = start + origin;
            let end = end + origin;
            let delta = end - start;
            let length = delta.length();
            if length <= 0.1 {
                return;
            }
            let outward = match outward {
                Direction::North => Vec2::Y,
                Direction::East => Vec2::X,
                Direction::South => -Vec2::Y,
                Direction::West => -Vec2::X,
            };
            let centre = (start + end) * 0.5 - outward * width_metres * 0.5;
            let horizontal = delta.x.abs() >= delta.y.abs();
            spawn_box(
                world,
                &palette.floor,
                if horizontal {
                    Vec3::new(length, 0.16, width_metres)
                } else {
                    Vec3::new(width_metres, 0.16, length)
                },
                Vec3::new(centre.x, elevation_metres - 0.08, centre.y),
                Quat::IDENTITY,
                "walkable rampart surface",
            );
        }
        WallWalk::Round {
            centre,
            elevation_metres,
            outer_radius_metres,
            stairwell_radius_metres,
        } => {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(annulus_mesh(
                stairwell_radius_metres,
                outer_radius_metres,
                0.16,
            ));
            let centre = centre + origin;
            world.spawn((
                Name::new("walkable tower-top deck with stairwell"),
                ClosedSolid,
                Mesh3d(mesh),
                MeshMaterial3d(palette.floor.clone()),
                Transform::from_xyz(centre.x, elevation_metres - 0.08, centre.y),
            ));
        }
        WallWalk::RectangularDeck {
            centre,
            size,
            elevation_metres,
            stairwell_centre,
            stairwell_size,
        } => {
            let centre = centre + origin;
            let stairwell_centre = stairwell_centre + origin;
            let side_depth = (size.y - stairwell_size.y) * 0.5;
            for sign in [-1.0, 1.0] {
                spawn_box(
                    world,
                    &palette.floor,
                    Vec3::new(size.x, 0.20, side_depth + 0.02),
                    Vec3::new(
                        centre.x,
                        elevation_metres - 0.09,
                        stairwell_centre.y + sign * (stairwell_size.y + side_depth) * 0.5,
                    ),
                    Quat::IDENTITY,
                    "walkable keep roof deck",
                );
            }
            let side_width = (size.x - stairwell_size.x) * 0.5;
            for sign in [-1.0, 1.0] {
                spawn_box(
                    world,
                    &palette.floor,
                    Vec3::new(side_width + 0.02, 0.20, stairwell_size.y + 0.02),
                    Vec3::new(
                        stairwell_centre.x + sign * (stairwell_size.x + side_width) * 0.5,
                        elevation_metres - 0.09,
                        stairwell_centre.y,
                    ),
                    Quat::IDENTITY,
                    "walkable keep roof deck",
                );
            }
        }
    }
}

fn annulus_mesh(inner_radius: f32, outer_radius: f32, height: f32) -> Mesh {
    sloped_annulus_mesh(inner_radius, outer_radius, height, 0.0, 0.0, 0, 0.0)
}

fn sloped_annulus_mesh(
    inner_radius: f32,
    outer_radius: f32,
    height: f32,
    inner_top_offset: f32,
    outer_top_offset: f32,
    drainage_outlet_count: u8,
    circumferential_fall: f32,
) -> Mesh {
    const SEGMENTS: usize = 64;
    let half_height = height * 0.5;
    let mut faces = Vec::with_capacity(SEGMENTS * 4);
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction_a = Vec2::new(angle_a.cos(), angle_a.sin());
        let direction_b = Vec2::new(angle_b.cos(), angle_b.sin());
        let channel_rise = |angle: f32| {
            if drainage_outlet_count == 0 {
                return 0.0;
            }
            let spacing = std::f32::consts::TAU / f32::from(drainage_outlet_count);
            let phase = angle.rem_euclid(spacing);
            phase.min(spacing - phase) / (spacing * 0.5) * circumferential_fall
        };
        let outer_top_a = half_height + outer_top_offset + channel_rise(angle_a);
        let outer_top_b = half_height + outer_top_offset + channel_rise(angle_b);
        let outer_a = direction_a * outer_radius;
        let outer_b = direction_b * outer_radius;
        let inner_a = direction_a * inner_radius;
        let inner_b = direction_b * inner_radius;
        faces.push(vec![
            Vec3::new(inner_a.x, half_height + inner_top_offset, inner_a.y),
            Vec3::new(outer_a.x, outer_top_a, outer_a.y),
            Vec3::new(outer_b.x, outer_top_b, outer_b.y),
            Vec3::new(inner_b.x, half_height + inner_top_offset, inner_b.y),
        ]);
        faces.push(vec![
            Vec3::new(outer_a.x, -half_height, outer_a.y),
            Vec3::new(inner_a.x, -half_height, inner_a.y),
            Vec3::new(inner_b.x, -half_height, inner_b.y),
            Vec3::new(outer_b.x, -half_height, outer_b.y),
        ]);
        faces.push(vec![
            Vec3::new(outer_a.x, -half_height, outer_a.y),
            Vec3::new(outer_b.x, -half_height, outer_b.y),
            Vec3::new(outer_b.x, outer_top_b, outer_b.y),
            Vec3::new(outer_a.x, outer_top_a, outer_a.y),
        ]);
        faces.push(vec![
            Vec3::new(inner_b.x, -half_height, inner_b.y),
            Vec3::new(inner_a.x, -half_height, inner_a.y),
            Vec3::new(inner_a.x, half_height + inner_top_offset, inner_a.y),
            Vec3::new(inner_b.x, half_height + inner_top_offset, inner_b.y),
        ]);
    }
    for face in &mut faces {
        face.reverse();
    }
    flat_face_mesh(&faces)
}

fn annular_sector_mesh(
    inner_radius: f32,
    outer_radius: f32,
    height: f32,
    start_angle: f32,
    end_angle: f32,
    inner_top_offset: f32,
    outer_top_offset: f32,
) -> Mesh {
    let sweep = (end_angle - start_angle).max(0.001);
    let segments = ((sweep / std::f32::consts::TAU * 64.0).ceil() as usize).max(1);
    let half = height * 0.5;
    let mut faces = Vec::with_capacity(segments * 4 + 2);
    let point =
        |radius: f32, angle: f32, y: f32| Vec3::new(radius * angle.cos(), y, radius * angle.sin());
    for segment in 0..segments {
        let a = start_angle + sweep * segment as f32 / segments as f32;
        let b = start_angle + sweep * (segment + 1) as f32 / segments as f32;
        faces.push(vec![
            point(inner_radius, a, half + inner_top_offset),
            point(outer_radius, a, half + outer_top_offset),
            point(outer_radius, b, half + outer_top_offset),
            point(inner_radius, b, half + inner_top_offset),
        ]);
        faces.push(vec![
            point(outer_radius, a, -half),
            point(inner_radius, a, -half),
            point(inner_radius, b, -half),
            point(outer_radius, b, -half),
        ]);
        faces.push(vec![
            point(outer_radius, a, -half),
            point(outer_radius, b, -half),
            point(outer_radius, b, half + outer_top_offset),
            point(outer_radius, a, half + outer_top_offset),
        ]);
        faces.push(vec![
            point(inner_radius, b, -half),
            point(inner_radius, a, -half),
            point(inner_radius, a, half + inner_top_offset),
            point(inner_radius, b, half + inner_top_offset),
        ]);
    }
    faces.push(vec![
        point(inner_radius, start_angle, -half),
        point(outer_radius, start_angle, -half),
        point(outer_radius, start_angle, half + outer_top_offset),
        point(inner_radius, start_angle, half + inner_top_offset),
    ]);
    // The end cap bounds the opposite side of the angular interval, so its
    // winding must oppose the start cap before the common face reversal.
    faces.push(vec![
        point(inner_radius, end_angle, half + inner_top_offset),
        point(outer_radius, end_angle, half + outer_top_offset),
        point(outer_radius, end_angle, -half),
        point(inner_radius, end_angle, -half),
    ]);
    for face in &mut faces {
        face.reverse();
    }
    flat_face_mesh(&faces)
}
