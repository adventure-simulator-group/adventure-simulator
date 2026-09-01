fn spawn_curtain_wall(
    world: &mut World,
    palette: &RenderPalette,
    wall: CurtainWallRun,
    origin: Vec2,
    closures: &[GateClosure],
) {
    let start = wall.start + origin;
    let end = wall.end + origin;
    let delta = end - start;
    let length = delta.length();
    if length <= 0.1 {
        return;
    }
    let tangent = delta / length;
    let horizontal = delta.x.abs() >= delta.y.abs();
    let wall_box = |world: &mut World, centre: Vec2, run_length: f32, height: f32, y: f32| {
        spawn_box(
            world,
            &palette.stone,
            if horizontal {
                Vec3::new(run_length, height, wall.thickness_metres)
            } else {
                Vec3::new(wall.thickness_metres, height, run_length)
            },
            Vec3::new(centre.x, y, centre.y),
            Quat::IDENTITY,
            "load-bearing curtain wall",
        );
    };
    if let Some(gate_width) = wall.gate_width_metres {
        let side_length = ((length - gate_width) * 0.5).max(0.1);
        let midpoint = (start + end) * 0.5;
        for sign in [-1.0, 1.0] {
            let centre = midpoint + tangent * (gate_width + side_length) * 0.5 * sign;
            wall_box(
                world,
                centre,
                side_length,
                wall.height_metres,
                wall.height_metres * 0.5,
            );
        }
        let lintel_height = wall.height_metres - wall.gate_height_metres;
        wall_box(
            world,
            midpoint,
            gate_width,
            lintel_height,
            wall.gate_height_metres + lintel_height * 0.5,
        );
        if closures.is_empty() {
            spawn_box(
                world,
                &palette.void,
                if horizontal {
                    Vec3::new(gate_width * 0.9, wall.gate_height_metres * 0.94, 0.08)
                } else {
                    Vec3::new(0.08, wall.gate_height_metres * 0.94, gate_width * 0.9)
                },
                Vec3::new(midpoint.x, wall.gate_height_metres * 0.47, midpoint.y),
                Quat::IDENTITY,
                "open curtain-wall gate passage",
            );
        }
        let inward = match wall.outward {
            Direction::North => -Vec2::Y,
            Direction::East => -Vec2::X,
            Direction::South => Vec2::Y,
            Direction::West => Vec2::X,
        };
        for closure in closures {
            let closure_centre = midpoint + inward * closure.inward_offset_metres;
            match closure.kind {
                GateClosureKind::HeavyLeaves => {
                    for sign in [-1.0, 1.0] {
                        let leaf_centre = closure_centre + tangent * gate_width * 0.25 * sign;
                        spawn_box(
                            world,
                            &palette.door,
                            if horizontal {
                                Vec3::new(gate_width * 0.48, wall.gate_height_metres * 0.9, 0.16)
                            } else {
                                Vec3::new(0.16, wall.gate_height_metres * 0.9, gate_width * 0.48)
                            },
                            Vec3::new(leaf_centre.x, wall.gate_height_metres * 0.45, leaf_centre.y),
                            Quat::from_rotation_y(sign * 0.22),
                            "closed heavy gate leaf",
                        );
                    }
                }
                GateClosureKind::Portcullis => {
                    for bar in 0..9 {
                        let across = (bar as f32 / 8.0 - 0.5) * gate_width * 0.9;
                        let position = closure_centre + tangent * across;
                        spawn_box(
                            world,
                            &palette.timber,
                            Vec3::splat(0.11).with_y(wall.gate_height_metres * 0.88),
                            Vec3::new(position.x, wall.gate_height_metres * 0.44, position.y),
                            Quat::IDENTITY,
                            "portcullis vertical bar",
                        );
                    }
                }
            }
        }
    } else {
        wall_box(
            world,
            (start + end) * 0.5,
            length,
            wall.height_metres,
            wall.height_metres * 0.5,
        );
    }
}

fn spawn_gatehouse_curtain(
    world: &mut World,
    palette: &RenderPalette,
    wall: CurtainWallRun,
    defense: &GateDefense,
    towers: &[RoundTower],
    origin: Vec2,
) {
    let GatehouseLoadPath::BondedTowerBearing {
        left_tower_index,
        right_tower_index,
        arch_centre,
        arch_spring_elevation_metres,
        arch_ring_depth,
        arch_rise,
        curtain_return_bond,
        ..
    } = defense.guard_chamber.load_path;
    let (Some(left), Some(right)) = (
        towers.get(left_tower_index).copied(),
        towers.get(right_tower_index).copied(),
    ) else {
        return;
    };
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let return_bond = curtain_return_bond.metres();
    let left_outer = left.centre_metres() - tangent * (left.radius_metres() - return_bond);
    let right_outer = right.centre_metres() + tangent * (right.radius_metres() - return_bond);
    for (start, end) in [(wall.start, left_outer), (right_outer, wall.end)] {
        if (end - start).length() > 0.05 {
            spawn_curtain_wall(
                world,
                palette,
                CurtainWallRun {
                    start,
                    end,
                    gate_width_metres: None,
                    ..wall
                },
                origin,
                &[],
            );
        }
    }

    let chamber = &defense.guard_chamber;
    let horizontal = tangent.x.abs() >= tangent.y.abs();
    let arch_depth = chamber.size.dot(direction_vector_2d(wall.outward).abs());
    let ring = arch_ring_depth.metres();
    let rise = arch_rise.metres();
    let half_span = wall.gate_width_metres.unwrap_or(3.2) * 0.5;
    let segments = 15;
    let block_width = half_span * 2.0 / segments as f32;
    for segment in 0..segments {
        let along = -half_span + (segment as f32 + 0.5) * block_width;
        let normalized = along / half_span;
        let elevation =
            arch_spring_elevation_metres + ring * 0.5 + rise * (1.0 - normalized * normalized);
        let slope = -2.0 * rise * normalized / half_span;
        let angle = slope.atan();
        let position = arch_centre + origin + tangent * along;
        spawn_box(
            world,
            &palette.stone,
            if horizontal {
                Vec3::new(block_width * 1.12, ring, arch_depth)
            } else {
                Vec3::new(arch_depth, ring, block_width * 1.12)
            },
            Vec3::new(position.x, elevation, position.y),
            if horizontal {
                Quat::from_rotation_z(angle)
            } else {
                Quat::from_rotation_x(-angle * tangent.y.signum())
            },
            "bonded segmental gate arch voussoir",
        );
    }
    let chamber_along = chamber.size.dot(tangent.abs());
    let shoulder_width = ((chamber_along - half_span * 2.0) * 0.5).max(0.0);
    let spandrel_height = ring + rise;
    for sign in [-1.0, 1.0] {
        spawn_wall_local_box(
            world,
            &palette.stone,
            chamber.centre + origin,
            tangent,
            direction_vector_2d(wall.outward),
            sign * (half_span + shoulder_width * 0.5),
            0.0,
            shoulder_width,
            arch_depth,
            spandrel_height,
            arch_spring_elevation_metres + spandrel_height * 0.5,
            "tower-bonded gate arch spandrel bearing",
        );
    }
    spawn_gate_closures(world, palette, wall, defense, origin);
}

fn spawn_gate_closures(
    world: &mut World,
    palette: &RenderPalette,
    wall: CurtainWallRun,
    defense: &GateDefense,
    origin: Vec2,
) {
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let horizontal = tangent.x.abs() >= tangent.y.abs();
    let inward = -direction_vector_2d(wall.outward);
    let gate_width = wall
        .gate_width_metres
        .unwrap_or_else(|| defense.guard_chamber.size.max_element());
    for closure in &defense.closures {
        let centre = defense.threshold + origin + inward * closure.inward_offset_metres;
        match closure.kind {
            GateClosureKind::HeavyLeaves => {
                for plank in 0..16 {
                    let across = (plank as f32 / 15.0 - 0.5) * gate_width * 0.97;
                    let height = closure.coverage.height_at(across);
                    let leaf = centre + tangent * across;
                    spawn_box(
                        world,
                        &palette.door,
                        if horizontal {
                            Vec3::new(gate_width / 15.0 * 1.04, height, 0.16)
                        } else {
                            Vec3::new(0.16, height, gate_width / 15.0 * 1.04)
                        },
                        Vec3::new(leaf.x, height * 0.5, leaf.y),
                        Quat::IDENTITY,
                        "closed heavy gate leaf",
                    );
                }
            }
            GateClosureKind::Portcullis => {
                for bar in 0..9 {
                    let across = (bar as f32 / 8.0 - 0.5) * gate_width * 0.9;
                    let position = centre + tangent * across;
                    let height = closure.coverage.height_at(across);
                    spawn_box(
                        world,
                        &palette.timber,
                        Vec3::splat(0.11).with_y(height),
                        Vec3::new(position.x, height * 0.5, position.y),
                        Quat::IDENTITY,
                        "portcullis vertical bar",
                    );
                }
            }
        }
    }
}

fn spawn_gate_guard_chamber(
    world: &mut World,
    palette: &RenderPalette,
    defense: &GateDefense,
    wall: CurtainWallRun,
    origin: Vec2,
    view: ViewerView,
) {
    let chamber = &defense.guard_chamber;
    let centre = chamber.centre + origin;
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let outward = direction_vector_2d(wall.outward);
    let along_size = chamber.size.dot(tangent.abs());
    let depth_size = chamber.size.dot(outward.abs());
    let half_along = along_size * 0.5;
    let half_depth = depth_size * 0.5;
    let floor_y = chamber.floor_elevation_metres;
    let wall_mid_y = floor_y + chamber.clear_height_metres * 0.5;
    let downward = chamber
        .openings
        .iter()
        .find(|opening| opening.kind == GuardOpeningKind::DownwardDefense);
    let hole_world = downward.map_or(centre, |opening| opening.position + origin);
    let hole_relative = hole_world - centre;
    let hole_along = hole_relative.dot(tangent);
    let hole_depth = hole_relative.dot(outward);
    let hole_size = downward.map_or(0.45, |opening| opening.width_metres.max(0.35));
    let left_width = hole_along - hole_size * 0.5 + half_along;
    let right_width = half_along - (hole_along + hole_size * 0.5);
    for (width, along) in [
        (left_width, -half_along + left_width * 0.5),
        (right_width, half_along - right_width * 0.5),
    ] {
        if width > 0.05 {
            spawn_wall_local_box(
                world,
                &palette.floor,
                centre,
                tangent,
                outward,
                along,
                0.0,
                width,
                depth_size,
                0.18,
                floor_y,
                "gate guard chamber floor",
            );
        }
    }
    let inward_depth = hole_depth - hole_size * 0.5 + half_depth;
    let outward_depth = half_depth - (hole_depth + hole_size * 0.5);
    for (depth, depth_offset) in [
        (inward_depth, -half_depth + inward_depth * 0.5),
        (outward_depth, half_depth - outward_depth * 0.5),
    ] {
        if depth > 0.05 {
            spawn_wall_local_box(
                world,
                &palette.floor,
                centre,
                tangent,
                outward,
                hole_along,
                depth_offset,
                hole_size,
                depth,
                0.18,
                floor_y,
                "gate guard chamber floor around downward opening",
            );
        }
    }
    // A recessed, explicitly non-colliding backdrop makes the downward
    // opening readable in section captures without filling the audited void.
    let hole_backdrop = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(
        hole_size * 0.9,
        0.04,
        hole_size * 0.9,
    ));
    world.spawn((
        Name::new("non-colliding downward opening depth"),
        NonCollidingVisualization,
        Mesh3d(hole_backdrop),
        MeshMaterial3d(palette.void.clone()),
        Transform::from_xyz(hole_world.x, floor_y - 0.12, hole_world.y),
    ));
    for support in &chamber.supports {
        let support_centre = support.centre + origin;
        spawn_box(
            world,
            &palette.stone,
            Vec3::new(
                support.size.x,
                support.top_elevation_metres - support.base_elevation_metres,
                support.size.y,
            ),
            Vec3::new(
                support_centre.x,
                (support.top_elevation_metres + support.base_elevation_metres) * 0.5,
                support_centre.y,
            ),
            Quat::IDENTITY,
            "gate guard chamber support pier",
        );
    }

    let observation = chamber
        .openings
        .iter()
        .find(|opening| opening.kind == GuardOpeningKind::OutwardObservation);
    let observation_width = observation.map_or(0.35, |opening| opening.width_metres);
    let observation_sill =
        observation.map_or(floor_y + 0.85, |opening| opening.sill_elevation_metres);
    let observation_height = observation.map_or(0.8, |opening| opening.clear_height_metres);
    let observation_along = observation
        .map(|opening| (opening.position - chamber.centre).dot(tangent))
        .unwrap_or(0.0);
    let left_wall_width = observation_along - observation_width * 0.5 + half_along;
    let right_wall_width = half_along - observation_along - observation_width * 0.5;
    for (width, along) in [
        (left_wall_width, -half_along + left_wall_width * 0.5),
        (right_wall_width, half_along - right_wall_width * 0.5),
    ] {
        if width <= 0.05 {
            continue;
        }
        spawn_wall_local_box(
            world,
            &palette.stone,
            centre,
            tangent,
            outward,
            along,
            half_depth,
            width,
            0.28,
            chamber.clear_height_metres,
            wall_mid_y,
            "gate guard chamber outward wall pier",
        );
    }
    let lower_height = (observation_sill - floor_y).max(0.2);
    spawn_wall_local_box(
        world,
        &palette.stone,
        centre,
        tangent,
        outward,
        observation_along,
        half_depth,
        observation_width,
        0.28,
        lower_height,
        floor_y + lower_height * 0.5,
        "gate guard chamber observation sill",
    );
    let upper_base = observation_sill + observation_height;
    let upper_height = (floor_y + chamber.clear_height_metres - upper_base).max(0.2);
    spawn_wall_local_box(
        world,
        &palette.stone,
        centre,
        tangent,
        outward,
        observation_along,
        half_depth,
        observation_width,
        0.28,
        upper_height,
        upper_base + upper_height * 0.5,
        "gate guard chamber observation lintel",
    );
    spawn_wall_local_box(
        world,
        &palette.void,
        centre,
        tangent,
        outward,
        observation_along,
        half_depth + 0.02,
        observation_width * 0.9,
        0.05,
        observation_height * 0.9,
        observation_sill + observation_height * 0.5,
        "gate guard chamber outward firing opening",
    );

    for sign in [-1.0, 1.0] {
        if view == ViewerView::GateDetailInterior && sign > 0.0 {
            continue;
        }
        spawn_wall_local_box(
            world,
            &palette.stone,
            centre,
            tangent,
            outward,
            sign * (half_along - 0.14),
            0.0,
            0.28,
            depth_size,
            chamber.clear_height_metres,
            wall_mid_y,
            "gate guard chamber side wall",
        );
    }
    {
        let door = chamber.access.door;
        let top_opening = chamber.access.top_walk_opening;
        let door_along = (door.position - chamber.centre).dot(tangent);
        let top_along = (top_opening.position - chamber.centre).dot(tangent);
        let top_left = top_along - top_opening.width_metres * 0.5;
        let top_right = top_along + top_opening.width_metres * 0.5;
        let door_left = door_along - door.width_metres * 0.5;
        let door_right = door_along + door.width_metres * 0.5;
        for (wall_section, (start, end)) in [
            (-half_along, top_left),
            (top_right, door_left),
            (door_right, half_along),
        ]
        .into_iter()
        .enumerate()
        {
            if view == ViewerView::GateDetailInterior && wall_section == 1 {
                continue;
            }
            let width = end - start;
            let along = (start + end) * 0.5;
            if width > 0.05 {
                spawn_wall_local_box(
                    world,
                    &palette.stone,
                    centre,
                    tangent,
                    outward,
                    along,
                    -half_depth,
                    width,
                    0.28,
                    chamber.clear_height_metres,
                    wall_mid_y,
                    "gate guard chamber access wall",
                );
            }
        }
        let below_top = top_opening.threshold_elevation_metres - floor_y;
        if below_top > 0.02 {
            spawn_wall_local_box(
                world,
                &palette.stone,
                centre,
                tangent,
                outward,
                top_along,
                -half_depth,
                top_opening.width_metres,
                0.28,
                below_top,
                floor_y + below_top * 0.5,
                "masonry below wall-walk access opening",
            );
        }
        let above_door = floor_y + chamber.clear_height_metres
            - (door.threshold_elevation_metres + door.clear_height_metres);
        if above_door > 0.02 {
            spawn_wall_local_box(
                world,
                &palette.stone,
                centre,
                tangent,
                outward,
                door_along,
                -half_depth,
                door.width_metres,
                0.28,
                above_door,
                door.threshold_elevation_metres + door.clear_height_metres + above_door * 0.5,
                "gate guard chamber access lintel",
            );
        }
        if view == ViewerView::GateDetailInterior {
            let hinge = door.position + origin + tangent * (door.width_metres * 0.5);
            let leaf_centre = hinge + outward * (door.width_metres * 0.5);
            spawn_wall_local_box(
                world,
                &palette.door,
                leaf_centre,
                tangent,
                outward,
                0.0,
                0.0,
                0.08,
                door.width_metres,
                door.clear_height_metres * 0.96,
                door.threshold_elevation_metres + door.clear_height_metres * 0.48,
                "open floor-level guard chamber door",
            );
        } else {
            spawn_wall_local_box(
                world,
                &palette.door,
                centre,
                tangent,
                outward,
                door_along,
                -half_depth - 0.02,
                door.width_metres * 0.92,
                0.08,
                door.clear_height_metres * 0.96,
                door.threshold_elevation_metres + door.clear_height_metres * 0.48,
                "floor-level guard chamber door",
            );
        }

        let cut = chamber.access.roof_clearance_opening;
        let cut_along = (cut.centre - chamber.centre).dot(tangent);
        let cut_depth = (cut.centre - chamber.centre).dot(outward);
        let cut_along_size = cut.size.dot(tangent.abs());
        let cut_depth_size = cut.size.dot(outward.abs());
        let left_roof = cut_along - cut_along_size * 0.5 + half_along;
        let right_roof = half_along - cut_along - cut_along_size * 0.5;
        for (roof_index, (width, along)) in [
            (left_roof, -half_along + left_roof * 0.5),
            (right_roof, half_along - right_roof * 0.5),
        ]
        .into_iter()
        .enumerate()
        {
            if view == ViewerView::GateDetailInterior && roof_index == 1 {
                continue;
            }
            if width > 0.02 {
                spawn_wall_local_box(
                    world,
                    &palette.roof_secondary,
                    centre,
                    tangent,
                    outward,
                    along,
                    0.0,
                    width,
                    depth_size,
                    0.22,
                    floor_y + chamber.clear_height_metres + 0.11,
                    "gate guard chamber roof slab",
                );
            }
        }
        let inner_depth = cut_depth - cut_depth_size * 0.5 + half_depth;
        let outer_depth = half_depth - cut_depth - cut_depth_size * 0.5;
        for (depth, offset) in [
            (inner_depth, -half_depth + inner_depth * 0.5),
            (outer_depth, half_depth - outer_depth * 0.5),
        ] {
            if depth > 0.02 {
                spawn_wall_local_box(
                    world,
                    &palette.roof_secondary,
                    centre,
                    tangent,
                    outward,
                    cut_along,
                    offset,
                    cut_along_size,
                    depth,
                    0.22,
                    floor_y + chamber.clear_height_metres + 0.11,
                    "gate guard chamber roof around access cut",
                );
            }
        }
    }

    let access = &chamber.access;
    for (landing, name) in [
        (access.top_landing, "gate access top landing"),
        (access.bottom_landing, "gate access bottom landing"),
    ] {
        spawn_box(
            world,
            &palette.stair,
            Vec3::new(landing.size.x, 0.16, landing.size.y),
            Vec3::new(
                landing.centre.x + origin.x,
                landing.elevation_metres,
                landing.centre.y + origin.y,
            ),
            Quat::IDENTITY,
            name,
        );
    }
    for guard in &access.landing_guards {
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                guard.start.x + origin.x,
                guard.elevation_metres + guard.height_metres,
                guard.start.y + origin.y,
            ),
            Vec3::new(
                guard.end.x + origin.x,
                guard.elevation_metres + guard.height_metres,
                guard.end.y + origin.y,
            ),
            0.1,
            "gate access landing perimeter guard",
        );
        for point in [guard.start, guard.end] {
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(
                    point.x + origin.x,
                    guard.elevation_metres,
                    point.y + origin.y,
                ),
                Vec3::new(
                    point.x + origin.x,
                    guard.elevation_metres + guard.height_metres,
                    point.y + origin.y,
                ),
                0.1,
                "gate access landing guard post",
            );
        }
    }
    for tread in 0..=access.flight.riser_count {
        let progress = f32::from(tread) / f32::from(access.flight.riser_count);
        let position = access.flight.top.lerp(access.flight.bottom, progress) + origin;
        let elevation = access.flight.top_elevation_metres
            + (access.flight.bottom_elevation_metres - access.flight.top_elevation_metres)
                * progress;
        spawn_wall_local_box(
            world,
            &palette.stair,
            position,
            tangent,
            outward,
            0.0,
            0.0,
            access.flight.going_metres + access.flight.nosing_metres,
            access.envelope.width_metres,
            0.12,
            elevation,
            "gate guard chamber access stair",
        );
        for sign in [-1.0, 1.0] {
            spawn_wall_local_box(
                world,
                &palette.timber,
                position,
                tangent,
                outward,
                0.0,
                sign * (access.envelope.width_metres * 0.38),
                access.flight.going_metres + access.flight.nosing_metres,
                0.16,
                0.22,
                elevation - 0.12,
                "gate access stepped stringer",
            );
        }
        for sign in [-1.0, 1.0] {
            spawn_wall_local_box(
                world,
                &palette.timber,
                position,
                tangent,
                outward,
                0.0,
                sign * (access.envelope.width_metres * 0.5 + 0.06),
                access.flight.going_metres + access.flight.nosing_metres,
                0.1,
                0.1,
                elevation + access.flight_guard_height_metres,
                "gate access continuous edge guard",
            );
            if tread % 2 == 0 {
                spawn_wall_local_box(
                    world,
                    &palette.timber,
                    position,
                    tangent,
                    outward,
                    0.0,
                    sign * (access.envelope.width_metres * 0.5 + 0.06),
                    0.1,
                    0.1,
                    access.flight_guard_height_metres,
                    elevation + access.flight_guard_height_metres * 0.5,
                    "gate access guard post",
                );
            }
        }
    }
    for support in &access.support_posts {
        let height = support.top_elevation_metres - support.base_elevation_metres;
        spawn_box(
            world,
            &palette.timber,
            Vec3::new(support.size.x, height, support.size.y),
            Vec3::new(
                support.centre.x + origin.x,
                support.base_elevation_metres + height * 0.5,
                support.centre.y + origin.y,
            ),
            Quat::IDENTITY,
            "gate access support post",
        );
    }
    spawn_box(
        world,
        &palette.timber,
        Vec3::new(
            access.wall_ledger.size.x,
            access.wall_ledger.height_metres,
            access.wall_ledger.size.y,
        ),
        Vec3::new(
            access.wall_ledger.centre.x + origin.x,
            access.wall_ledger.elevation_metres,
            access.wall_ledger.centre.y + origin.y,
        ),
        Quat::IDENTITY,
        "gate access masonry wall ledger",
    );
    for brace in &access.lateral_braces {
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                brace.start.x + origin.x,
                brace.start_elevation_metres,
                brace.start.y + origin.y,
            ),
            Vec3::new(
                brace.end.x + origin.x,
                brace.end_elevation_metres,
                brace.end.y + origin.y,
            ),
            brace.thickness_metres,
            "gate access diagonal lateral brace",
        );
    }
    for operating in &chamber.operating_positions {
        let position = operating.position + origin;
        spawn_box(
            world,
            &palette.timber,
            Vec3::new(1.3, 0.18, 0.18),
            Vec3::new(position.x, operating.elevation_metres + 0.95, position.y),
            Quat::IDENTITY,
            "portcullis operating windlass",
        );
        spawn_box(
            world,
            &palette.timber,
            Vec3::new(0.18, 1.1, 0.18),
            Vec3::new(position.x, operating.elevation_metres + 0.55, position.y),
            Quat::IDENTITY,
            "portcullis operating post",
        );
    }
}
