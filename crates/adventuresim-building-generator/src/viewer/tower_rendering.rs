#[allow(clippy::too_many_arguments)]
fn spawn_wall_local_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    along_offset: f32,
    outward_offset: f32,
    along_size: f32,
    depth_size: f32,
    height: f32,
    elevation: f32,
    name: &'static str,
) {
    let position = centre + tangent * along_offset + outward * outward_offset;
    let horizontal = tangent.x.abs() >= tangent.y.abs();
    spawn_box(
        world,
        material,
        if horizontal {
            Vec3::new(along_size, height, depth_size)
        } else {
            Vec3::new(depth_size, height, along_size)
        },
        Vec3::new(position.x, elevation, position.y),
        Quat::IDENTITY,
        name,
    );
}

fn spawn_square_tower(
    world: &mut World,
    palette: &RenderPalette,
    tower: SquareTower,
    origin: Vec2,
    view: ViewerView,
) {
    if view == ViewerView::Cutaway {
        return;
    }
    let centre = tower.centre + origin;
    // Bell towers hand authority to resolved SquareTowerFace bays at 8 m so
    // their roof junction and bell openings can own real subtractions.  Keep
    // only the monolithic grounded base here; spawning the old full-height box
    // would duplicate those resolved walls and conceal the abutment contour.
    let lower_height = if tower.bell_openings {
        8.0
    } else {
        tower.wall_height_metres
    };
    spawn_box(
        world,
        &palette.stone,
        Vec3::new(tower.size.x, lower_height, tower.size.y),
        Vec3::new(centre.x, lower_height * 0.5, centre.y),
        Quat::IDENTITY,
        "square bell-tower lower mass",
    );
    // The bell stage itself is rendered exclusively from its resolved
    // WallAssembly/OpeningAssembly bays. Keeping a second viewer-owned stage
    // here would conceal voids and duplicate the authoritative masonry.
    // The roof is rendered once from the authoritative RoofAssembly graph.
}

fn spawn_tower(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    tower_index: usize,
    tower: RoundTower,
    origin: Vec2,
    view: ViewerView,
    portals: &[TowerPortal],
    firing_positions: &[FiringPosition],
    authoritative_crown: bool,
) {
    let centre = tower.centre_metres() + origin;
    if view != ViewerView::Cutaway {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(tower_shell_mesh(
            tower,
            portals,
            firing_positions,
            matches!(
                view,
                ViewerView::TowerPortalDetail
                    | ViewerView::CrownTowerCutaway
                    | ViewerView::WallRoundTowerRadialSection
                    | ViewerView::ArtilleryRondelCasemate
                    | ViewerView::ArtilleryRondelCutaway
            ),
        ));
        let wall = plan.wall_assemblies.iter().find(|wall| {
            matches!(
                wall.source,
                adventuresim_building_generator::WallSourceId::RoundTower { tower_index: index }
                    if index == tower_index
            )
        });
        let resolved = wall.and_then(|wall| {
            wall.host_solids.first().and_then(|id| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
            })
        });
        let mut shell = world.spawn((
            Name::new(if let Some(wall) = wall {
                format!("resolved wall owner {} round tower shell", wall.owner.0)
            } else {
                "round tower shell with open firing loops".to_owned()
            }),
            ClosedSolid,
            Mesh3d(mesh),
            MeshMaterial3d(if view == ViewerView::ArtilleryRondelCasemate {
                palette.cutaway.clone()
            } else {
                palette.stone.clone()
            }),
            Transform::from_xyz(centre.x, tower.wall_height_metres * 0.5, centre.y),
        ));
        if let (Some(wall), Some(resolved)) = (wall, resolved) {
            shell.insert((
                GeometryOwner(wall.owner.0),
                ResolvedRenderItem {
                    id: resolved.id.0,
                    fingerprint: stable_u64(
                        &serde_json::to_vec(resolved)
                            .expect("serialize rendered radial wall shell"),
                    ),
                    local_half_size: resolved.size * 0.5,
                },
            ));
        }
        for interface in tower.chord_interfaces() {
            spawn_tower_chord_face(world, palette, tower, centre, interface);
        }
        if !matches!(
            view,
            ViewerView::TowerPortalDetail
                | ViewerView::CrownTowerCutaway
                | ViewerView::WallRoundTowerRadialSection
                | ViewerView::ArtilleryRondelCasemate
                | ViewerView::ArtilleryRondelCutaway
        ) {
            let inner_height = (tower.wall_height_metres - 0.18).max(0.2);
            let inner = world.resource_mut::<Assets<Mesh>>().add(cylinder_side_mesh(
                (tower.radius_metres() - tower.wall_thickness_metres).max(0.2),
                inner_height,
                64,
            ));
            world.spawn((
                Name::new("non-colliding dark tower depth backdrop"),
                NonCollidingVisualization,
                Mesh3d(inner),
                MeshMaterial3d(palette.void.clone()),
                Transform::from_xyz(centre.x, inner_height * 0.5, centre.y),
            ));
        }
        // Tower roofs are rendered once from the authoritative RoofAssembly graph.
    } else {
        for level in 0..=0 {
            let mesh = world
                .resource_mut::<Assets<Mesh>>()
                .add(Cylinder::new(tower.radius_metres() - 0.18, 0.12));
            world.spawn((
                Name::new("cutaway tower floor"),
                Mesh3d(mesh),
                MeshMaterial3d(palette.floor.clone()),
                Transform::from_xyz(centre.x, level as f32 * 3.4 + 0.06, centre.y),
            ));
        }
    }
    spawn_tower_portal_geometry(world, palette, tower, origin, portals);
    if !authoritative_crown
        && view != ViewerView::Cutaway
        && let Some(kind) = tower.battlement
    {
        spawn_round_battlement(
            world,
            palette,
            tower,
            origin,
            kind,
            portals,
            view == ViewerView::TowerPortalDetail,
        );
    }
}

fn tower_shell_mesh(
    tower: RoundTower,
    portals: &[TowerPortal],
    firing_positions: &[FiringPosition],
    section_cut: bool,
) -> Mesh {
    // Project gate apertures are only 0.18 m wide. At the standard 3 m
    // radius, 256 facets keep an aperture wider than two chord samples while
    // exact feature-boundary tessellation remains a future optimization.
    const SEGMENTS: usize = 256;
    let half_height = tower.wall_height_metres * 0.5;
    let slit_ranges = (0..3)
        .map(|level| {
            let centre = 1.45 + level as f32 * 2.2;
            (
                (centre - 0.45).max(0.05),
                (centre + 0.45).min(tower.wall_height_metres - 0.05),
            )
        })
        .filter(|(low, high)| low < high)
        .collect::<Vec<_>>();
    let mut height_breaks = vec![0.0, tower.wall_height_metres];
    height_breaks.extend(slit_ranges.iter().flat_map(|(low, high)| [*low, *high]));
    height_breaks.extend(portals.iter().flat_map(|portal| {
        [
            portal.sill_elevation_metres.max(0.0),
            (portal.sill_elevation_metres + portal.clear_height_metres)
                .min(tower.wall_height_metres),
        ]
    }));
    height_breaks.extend(firing_positions.iter().flat_map(|position| {
        [
            (position.elevation_metres - 0.45).max(0.0),
            (position.elevation_metres + 0.45).min(tower.wall_height_metres),
        ]
    }));
    height_breaks.sort_by(f32::total_cmp);
    height_breaks.dedup_by(|left, right| (*left - *right).abs() <= 0.001);
    let bands = height_breaks.len() - 1;
    let mut included = vec![vec![true; bands]; SEGMENTS];
    for (segment, segment_bands) in included.iter_mut().enumerate() {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_mid = (angle_a + angle_b) * 0.5;
        let radial_mid = Vec2::new(angle_mid.cos(), angle_mid.sin());
        let chord_cut = tower.chord_interfaces().any(|interface| {
            let toward = direction_vector_2d(interface.toward_gate);
            let cut_ratio =
                (tower.radius_metres() - interface.bearing_depth.metres()) / tower.radius_metres();
            radial_mid.dot(toward) > cut_ratio
        });
        let section_removed =
            section_cut && radial_mid.dot(Vec2::new(-0.707_106_77, -0.707_106_77)) > 0.1;
        for band in 0..bands {
            let height_mid = (height_breaks[band] + height_breaks[band + 1]) * 0.5;
            let slit = segment.is_multiple_of(SEGMENTS / 8)
                && slit_ranges
                    .iter()
                    .any(|(low, high)| height_mid > *low && height_mid < *high);
            let portal_void = portals.iter().any(|portal| {
                let facing = direction_vector_2d(portal.facing);
                let half_angle = portal.width_metres * 0.5 / tower.radius_metres();
                radial_mid.dot(facing) >= half_angle.cos()
                    && height_mid > portal.sill_elevation_metres
                    && height_mid < portal.sill_elevation_metres + portal.clear_height_metres
            });
            let firing_void = firing_positions.iter().any(|position| {
                let half_angle = position.aperture_width_metres * 0.5 / tower.radius_metres();
                radial_mid.dot(position.aperture_normal) >= half_angle.cos()
                    && height_mid > position.elevation_metres - 0.45
                    && height_mid < position.elevation_metres + 0.45
            });
            segment_bands[band] =
                !(chord_cut || section_removed || slit || portal_void || firing_void);
        }
    }
    let outer_radius = tower.radius_metres();
    let inner_radius = (outer_radius - tower.wall_thickness_metres).max(0.2);
    let mut faces = Vec::new();
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction_a = Vec2::new(angle_a.cos(), angle_a.sin());
        let direction_b = Vec2::new(angle_b.cos(), angle_b.sin());
        let outer_a = direction_a * outer_radius;
        let outer_b = direction_b * outer_radius;
        let inner_a = direction_a * inner_radius;
        let inner_b = direction_b * inner_radius;
        for band in 0..bands {
            if !included[segment][band] {
                continue;
            }
            let low = height_breaks[band] - half_height;
            let high = height_breaks[band + 1] - half_height;
            // Outer and inner faces are always exposed wall surfaces.
            faces.push(vec![
                Vec3::new(outer_a.x, low, outer_a.y),
                Vec3::new(outer_b.x, low, outer_b.y),
                Vec3::new(outer_b.x, high, outer_b.y),
                Vec3::new(outer_a.x, high, outer_a.y),
            ]);
            faces.push(vec![
                Vec3::new(inner_b.x, low, inner_b.y),
                Vec3::new(inner_a.x, low, inner_a.y),
                Vec3::new(inner_a.x, high, inner_a.y),
                Vec3::new(inner_b.x, high, inner_b.y),
            ]);
            if band == 0 || !included[segment][band - 1] {
                faces.push(vec![
                    Vec3::new(outer_a.x, low, outer_a.y),
                    Vec3::new(inner_a.x, low, inner_a.y),
                    Vec3::new(inner_b.x, low, inner_b.y),
                    Vec3::new(outer_b.x, low, outer_b.y),
                ]);
            }
            if band + 1 == bands || !included[segment][band + 1] {
                faces.push(vec![
                    Vec3::new(inner_a.x, high, inner_a.y),
                    Vec3::new(outer_a.x, high, outer_a.y),
                    Vec3::new(outer_b.x, high, outer_b.y),
                    Vec3::new(inner_b.x, high, inner_b.y),
                ]);
            }
            let previous = (segment + SEGMENTS - 1) % SEGMENTS;
            if !included[previous][band] {
                faces.push(vec![
                    Vec3::new(inner_a.x, low, inner_a.y),
                    Vec3::new(outer_a.x, low, outer_a.y),
                    Vec3::new(outer_a.x, high, outer_a.y),
                    Vec3::new(inner_a.x, high, inner_a.y),
                ]);
            }
            let next = (segment + 1) % SEGMENTS;
            if !included[next][band] {
                faces.push(vec![
                    Vec3::new(outer_b.x, low, outer_b.y),
                    Vec3::new(inner_b.x, low, inner_b.y),
                    Vec3::new(inner_b.x, high, inner_b.y),
                    Vec3::new(outer_b.x, high, outer_b.y),
                ]);
            }
        }
    }
    for face in &mut faces {
        face.reverse();
    }
    flat_face_mesh(&faces)
}

fn spawn_tower_chord_face(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    centre: Vec2,
    interface: adventuresim_building_generator::TowerChordInterface,
) {
    let toward = direction_vector_2d(interface.toward_gate);
    let radius = tower.radius_metres();
    let cut_distance = radius - interface.bearing_depth.metres();
    let chord_width = 2.0
        * (radius * radius - cut_distance * cut_distance)
            .max(0.0)
            .sqrt();
    let thickness = tower
        .wall_thickness_metres
        .min(interface.bearing_depth.metres());
    let face = centre + toward * (cut_distance - thickness * 0.5);
    let along_x = toward.x.abs() > 0.5;
    spawn_box(
        world,
        &palette.stone,
        if along_x {
            Vec3::new(thickness, tower.wall_height_metres, chord_width)
        } else {
            Vec3::new(chord_width, tower.wall_height_metres, thickness)
        },
        Vec3::new(face.x, tower.wall_height_metres * 0.5, face.y),
        Quat::IDENTITY,
        "bonded tower chord face",
    );
}

fn direction_vector_2d(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    }
}

fn spawn_tower_portal_geometry(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    origin: Vec2,
    portals: &[TowerPortal],
) {
    let centre = tower.centre_metres() + origin;
    for portal in portals {
        let radial = direction_vector_2d(portal.facing);
        let tangent = Vec2::new(-radial.y, radial.x);
        let frame_centre = centre + radial * tower.radius_metres();
        if portal.kind == TowerPortalKind::GroundStairEntrance {
            for sign in [-1.0, 1.0] {
                let jamb = frame_centre + tangent * portal.width_metres * 0.58 * sign;
                spawn_box(
                    world,
                    &palette.stone,
                    Vec3::new(0.2, portal.clear_height_metres, 0.2),
                    Vec3::new(jamb.x, portal.clear_height_metres * 0.5, jamb.y),
                    Quat::IDENTITY,
                    "tower entrance jamb",
                );
            }
            spawn_box(
                world,
                &palette.stone,
                if radial.x.abs() > radial.y.abs() {
                    Vec3::new(0.24, 0.22, portal.width_metres + 0.35)
                } else {
                    Vec3::new(portal.width_metres + 0.35, 0.22, 0.24)
                },
                Vec3::new(
                    frame_centre.x,
                    portal.clear_height_metres + 0.11,
                    frame_centre.y,
                ),
                Quat::IDENTITY,
                "tower entrance lintel",
            );
        } else {
            let landing = centre + radial * (tower.radius_metres() - 0.15);
            spawn_box(
                world,
                &palette.floor,
                if radial.x.abs() > radial.y.abs() {
                    Vec3::new(1.3, 0.16, portal.width_metres)
                } else {
                    Vec3::new(portal.width_metres, 0.16, 1.3)
                },
                Vec3::new(landing.x, portal.sill_elevation_metres + 0.12, landing.y),
                Quat::IDENTITY,
                "tower-to-wall-walk portal landing",
            );
        }
    }
}

#[allow(dead_code)]
fn spawn_conical_roof(world: &mut World, material: &Handle<StandardMaterial>, roof: RoofPiece) {
    let radius = roof.size.x.max(roof.size.y) * 0.5 + roof.eave_metres;
    let height = radius * roof.pitch_degrees.to_radians().tan();
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cone::new(radius, height));
    world.spawn((
        Name::new("conical tower roof"),
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        Transform::from_xyz(
            roof.centre.x,
            roof.base_height_metres + height * 0.5,
            roof.centre.y,
        ),
    ));
}

fn spawn_round_battlement(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    origin: Vec2,
    kind: BattlementKind,
    portals: &[TowerPortal],
    section_cut: bool,
) {
    let centre = tower.centre_metres() + origin;
    let radius = tower.radius_metres()
        + if kind == BattlementKind::Machicolated {
            0.38
        } else {
            0.08
        };
    if kind == BattlementKind::GunLoopParapet {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(round_loop_parapet_mesh(radius, 1.15));
        world.spawn((
            Name::new("round parapet with open gun loops"),
            Mesh3d(mesh),
            MeshMaterial3d(palette.stone.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres + 0.58, centre.y),
        ));
        let inner = world.resource_mut::<Assets<Mesh>>().add(cylinder_side_mesh(
            (radius - 0.24).max(0.2),
            1.11,
            72,
        ));
        world.spawn((
            Name::new("dark round parapet interior"),
            Mesh3d(inner),
            MeshMaterial3d(palette.void.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres + 0.58, centre.y),
        ));
        return;
    }
    if kind == BattlementKind::Machicolated {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cylinder::new(radius, 0.18));
        world.spawn((
            Name::new("machicolation gallery floor"),
            Mesh3d(mesh),
            MeshMaterial3d(palette.stone.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres, centre.y),
        ));
    }
    let count = 16;
    for index in 0..count {
        let angle = index as f32 * std::f32::consts::TAU / count as f32;
        let radial = Vec2::new(angle.cos(), angle.sin());
        if tower.chord_interfaces().any(|interface| {
            let toward = direction_vector_2d(interface.toward_gate);
            let cut_ratio =
                (tower.radius_metres() - interface.bearing_depth.metres()) / tower.radius_metres();
            radial.dot(toward) > cut_ratio
        }) {
            continue;
        }
        if section_cut && radial.dot(Vec2::new(-0.707_106_77, -0.707_106_77)) > 0.1 {
            continue;
        }
        if portals.iter().any(|portal| {
            matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. })
                && radial.dot(direction_vector_2d(portal.facing)) > 0.86
        }) {
            continue;
        }
        let tangent = Vec2::new(-angle.sin(), angle.cos());
        let position = centre + radial * radius;
        if kind == BattlementKind::PiercedCrenellated {
            for sign in [-1.0, 1.0] {
                let half = position + tangent * 0.17 * sign;
                spawn_box(
                    world,
                    &palette.stone,
                    Vec3::new(0.22, 0.85, 0.42),
                    Vec3::new(half.x, tower.wall_height_metres + 0.425, half.y),
                    Quat::from_rotation_y(-angle),
                    "round merlon split by firing loop",
                );
            }
        } else {
            spawn_box(
                world,
                &palette.stone,
                Vec3::new(0.55, 0.85, 0.42),
                Vec3::new(position.x, tower.wall_height_metres + 0.425, position.y),
                Quat::from_rotation_y(-angle),
                "round merlon",
            );
        }
        if kind == BattlementKind::Machicolated && index % 2 == 0 {
            let corbel_position = centre + radial * (tower.radius_metres() + 0.18);
            spawn_box(
                world,
                &palette.stone,
                Vec3::new(0.28, 0.7, 0.32),
                Vec3::new(
                    corbel_position.x,
                    tower.wall_height_metres - 0.38,
                    corbel_position.y,
                ),
                Quat::from_rotation_y(-angle),
                "machicolation corbel",
            );
        }
    }
}

fn round_loop_parapet_mesh(radius: f32, height: f32) -> Mesh {
    const SEGMENTS: usize = 72;
    let half_height = height * 0.5;
    let mut faces = Vec::new();
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let radial_a = Vec2::new(angle_a.cos(), angle_a.sin()) * radius;
        let radial_b = Vec2::new(angle_b.cos(), angle_b.sin()) * radius;
        let ranges = if segment.is_multiple_of(SEGMENTS / 12) {
            vec![(0.0, 0.32), (0.9, height)]
        } else {
            vec![(0.0, height)]
        };
        for (low, high) in ranges {
            faces.push(vec![
                Vec3::new(radial_a.x, low - half_height, radial_a.y),
                Vec3::new(radial_a.x, high - half_height, radial_a.y),
                Vec3::new(radial_b.x, high - half_height, radial_b.y),
                Vec3::new(radial_b.x, low - half_height, radial_b.y),
            ]);
        }
    }
    flat_face_mesh(&faces)
}

fn cylinder_side_mesh(radius: f32, height: f32, segments: usize) -> Mesh {
    let half_height = height * 0.5;
    let mut faces = Vec::with_capacity(segments);
    for segment in 0..segments {
        let angle_a = segment as f32 * std::f32::consts::TAU / segments as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / segments as f32;
        let a = Vec2::new(angle_a.cos(), angle_a.sin()) * radius;
        let b = Vec2::new(angle_b.cos(), angle_b.sin()) * radius;
        faces.push(vec![
            Vec3::new(b.x, -half_height, b.y),
            Vec3::new(b.x, half_height, b.y),
            Vec3::new(a.x, half_height, a.y),
            Vec3::new(a.x, -half_height, a.y),
        ]);
    }
    flat_face_mesh(&faces)
}
