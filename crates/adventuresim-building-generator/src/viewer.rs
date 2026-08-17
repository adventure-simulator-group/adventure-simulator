use std::{fs, path::PathBuf};

use adventuresim_building_generator::{
    Bartizan, BattlementKind, BattlementRun, BuildingArchetype, BuildingPlan, BuildingProgram,
    CELL_SIZE_METRES, Direction, DormerKind, GableProfile, Opening, OpeningKind, RidgeAxis,
    RoofDormer, RoofKind, RoofPiece, RoundTower, Stair, TimberFrameStyle, WALL_THICKNESS_METRES,
    WallSegment, WallStyle, WallWalk, generate,
};
use bevy::{
    app::AppExit,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{PresentMode, WindowResolution},
};
use serde::Serialize;

use crate::ViewerView;

const VIEW_WIDTH: u32 = 1440;
const VIEW_HEIGHT: u32 = 900;

#[derive(Resource)]
struct CaptureState {
    output: Option<PathBuf>,
    settle_frames: u32,
    settled: u32,
    primed: bool,
    in_flight: bool,
    manifest: CaptureManifest,
}

#[derive(Clone, Serialize)]
struct CaptureManifest {
    schema_version: u16,
    fixture: &'static str,
    view: &'static str,
    seed: u64,
    resolution: [u32; 2],
    room_count: usize,
    wall_count: usize,
    opening_count: usize,
    roof_piece_count: usize,
    roof_dormer_count: usize,
    tower_count: usize,
    stair_count: usize,
    battlement_run_count: usize,
    wall_walk_count: usize,
    bartizan_count: usize,
    observed_mesh_count: usize,
    visible_mesh_count: usize,
    active_camera_count: usize,
    subject_pixel_bps: u16,
    validation_passed: bool,
}

#[derive(Resource)]
struct RenderPalette {
    plaster: Handle<StandardMaterial>,
    brick: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
    roof_secondary: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    door: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    void: Handle<StandardMaterial>,
    stair: Handle<StandardMaterial>,
    room_floors: Vec<Handle<StandardMaterial>>,
}

pub(crate) fn run(
    archetype: BuildingArchetype,
    view: ViewerView,
    seed: u64,
    output: Option<PathBuf>,
    settle_frames: u32,
) {
    let program = BuildingProgram::fixture(archetype, seed);
    let plan = generate(&program).expect("curated building fixture must generate");
    if let Some(path) = &output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create building capture directory");
        }
        fs::write(
            path.with_extension("plan.json"),
            serde_json::to_vec_pretty(&plan).expect("serialize building plan"),
        )
        .expect("write generated building plan");
    }

    let manifest = CaptureManifest {
        schema_version: 1,
        fixture: archetype.slug(),
        view: match view {
            ViewerView::Exterior => "exterior",
            ViewerView::Defenses => "defenses",
            ViewerView::Cutaway => "cutaway",
        },
        seed,
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        room_count: plan.storeys.iter().map(|storey| storey.rooms.len()).sum(),
        wall_count: plan.storeys.iter().map(|storey| storey.walls.len()).sum(),
        opening_count: plan
            .storeys
            .iter()
            .map(|storey| storey.openings.len())
            .sum(),
        roof_piece_count: plan.roofs.len(),
        roof_dormer_count: plan.roof_dormers.len(),
        tower_count: plan.towers.len(),
        stair_count: plan.stairs.len(),
        battlement_run_count: plan.battlements.len(),
        wall_walk_count: plan.wall_walks.len(),
        bartizan_count: plan.bartizans.len(),
        observed_mesh_count: 0,
        visible_mesh_count: 0,
        active_camera_count: 0,
        subject_pixel_bps: 0,
        validation_passed: false,
    };

    let title = format!("Fabelgeist building prototype: {archetype:?} {view:?}");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title,
            resolution:
                WindowResolution::new(VIEW_WIDTH, VIEW_HEIGHT).with_scale_factor_override(1.0),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            decorations: output.is_none(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.72, 0.80, 0.86)))
    .insert_resource(CaptureState {
        output,
        settle_frames,
        settled: 0,
        primed: false,
        in_flight: false,
        manifest,
    })
    .add_systems(Startup, move |world: &mut World| setup(world, &plan, view))
    .add_systems(Last, capture_when_ready);
    let exit = app.run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}

fn setup(world: &mut World, plan: &BuildingPlan, view: ViewerView) {
    let palette = create_palette(world);
    let dimensions = plan.dimensions_metres();
    let origin = Vec2::new(-dimensions.x * 0.5, -dimensions.y * 0.5);
    let storey_height = plan.storey_height_metres;

    spawn_ground(world, dimensions);
    for storey in &plan.storeys {
        if view == ViewerView::Cutaway && storey.level > 0 {
            continue;
        }
        let base_y = f32::from(storey.level) * storey_height;
        for room in &storey.rooms {
            let floor_material = if view == ViewerView::Cutaway {
                &palette.room_floors[usize::from(room.id) % palette.room_floors.len()]
            } else {
                &palette.floor
            };
            for cell in &room.cells {
                spawn_box(
                    world,
                    floor_material,
                    Vec3::new(CELL_SIZE_METRES - 0.04, 0.12, CELL_SIZE_METRES - 0.04),
                    Vec3::new(
                        cell.centre().x + origin.x,
                        base_y + 0.06,
                        cell.centre().y + origin.y,
                    ),
                    Quat::IDENTITY,
                    "room floor",
                );
            }
        }
        for (wall_index, wall) in storey.walls.iter().copied().enumerate() {
            if view == ViewerView::Cutaway
                && wall.exterior()
                && matches!(wall.direction, Direction::South | Direction::East)
            {
                continue;
            }
            let opening = storey
                .openings
                .iter()
                .find(|opening| opening.wall == wall_index);
            spawn_wall(
                world,
                &palette,
                wall,
                opening,
                origin,
                base_y,
                storey_height,
                plan.wall_style,
                plan.timber_frame_style,
                plan.upper_storey_projection_metres * f32::from(storey.level),
            );
        }
        let projection = plan.upper_storey_projection_metres * f32::from(storey.level);
        if plan.wall_style == WallStyle::TimberFrame && projection > 0.01 {
            let min_x = origin.x - projection;
            let max_x = origin.x + dimensions.x + projection;
            let min_z = origin.y - projection;
            let max_z = origin.y + dimensions.y + projection;
            for z in [min_z, max_z] {
                spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(max_x - min_x, 0.14, 0.16),
                    Vec3::new((min_x + max_x) * 0.5, base_y + 0.04, z),
                    Quat::IDENTITY,
                    "projecting storey sill",
                );
            }
            for x in [min_x, max_x] {
                spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(0.16, 0.14, max_z - min_z),
                    Vec3::new(x, base_y + 0.04, (min_z + max_z) * 0.5),
                    Quat::IDENTITY,
                    "projecting storey sill",
                );
                for z in [min_z, max_z] {
                    spawn_box(
                        world,
                        &palette.timber,
                        Vec3::new(0.18, storey_height, 0.18),
                        Vec3::new(x, base_y + storey_height * 0.5, z),
                        Quat::IDENTITY,
                        "projecting storey corner post",
                    );
                }
            }
        }
    }

    if view != ViewerView::Cutaway {
        for (roof_index, roof) in plan.roofs.iter().copied().enumerate() {
            spawn_roof(world, &palette, roof, origin, roof_index, plan.wall_style);
        }
        for dormer in plan.roof_dormers.iter().copied() {
            spawn_roof_dormer(world, &palette, dormer, origin, plan.wall_style);
        }
    }
    for tower in plan.towers.iter().copied() {
        spawn_tower(world, &palette, tower, origin, view);
    }
    for stair in plan.stairs.iter().copied() {
        spawn_stair(world, &palette, stair, origin);
    }
    if view != ViewerView::Cutaway {
        for wall_walk in plan.wall_walks.iter().copied() {
            spawn_wall_walk(world, &palette, wall_walk, origin);
        }
        for run in plan.battlements.iter().copied() {
            spawn_battlement_run(world, &palette, run, origin);
        }
        for bartizan in plan.bartizans.iter().copied() {
            spawn_bartizan(world, &palette, bartizan, origin);
        }
    }

    let roof_height = plan
        .roofs
        .iter()
        .map(|roof| {
            let span = match roof.ridge_axis {
                RidgeAxis::Z => roof.size.x * 0.5 + roof.eave_metres,
                RidgeAxis::X => roof.size.y * 0.5 + roof.eave_metres,
            };
            roof.base_height_metres + span * roof.pitch_degrees.to_radians().tan()
        })
        .chain(plan.roof_dormers.iter().map(|dormer| {
            dormer.base_height_metres + dormer.height_metres + dormer.width_metres * 0.65
        }))
        .fold(0.0, f32::max);
    let max_height = plan
        .towers
        .iter()
        .map(|tower| tower.wall_height_metres + tower.radius_metres * 1.8)
        .fold(
            (plan.storeys.len() as f32 * storey_height + 7.0).max(roof_height),
            f32::max,
        );
    let radius = dimensions.length().max(max_height) * 1.05;
    let target = Vec3::new(0.0, max_height * 0.35, 0.0);
    let camera_position = match view {
        ViewerView::Exterior => Vec3::new(radius, max_height * 0.95, -radius * 1.3),
        ViewerView::Defenses => Vec3::new(-radius * 1.05, max_height * 1.35, radius * 1.15),
        ViewerView::Cutaway => Vec3::new(radius * 0.75, max_height * 1.8, -radius * 1.1),
    };
    world.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(target, Vec3::Y),
    ));
    world.spawn((
        DirectionalLight {
            illuminance: 16_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -55_f32.to_radians(),
            -35_f32.to_radians(),
            0.0,
        )),
    ));
    world.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.75, 0.80, 0.9),
        brightness: 240.0,
        affects_lightmapped_meshes: true,
    });
    world.insert_resource(palette);
}

fn create_palette(world: &mut World) -> RenderPalette {
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let material = |materials: &mut Assets<StandardMaterial>, color: Color, roughness| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: roughness,
            unlit: true,
            ..default()
        })
    };
    RenderPalette {
        plaster: material(&mut materials, Color::srgb(0.80, 0.74, 0.60), 0.9),
        brick: material(&mut materials, Color::srgb(0.48, 0.20, 0.13), 0.92),
        stone: material(&mut materials, Color::srgb(0.43, 0.44, 0.40), 0.95),
        timber: material(&mut materials, Color::srgb(0.16, 0.09, 0.045), 0.88),
        roof: material(&mut materials, Color::srgb(0.28, 0.08, 0.045), 0.95),
        roof_secondary: material(&mut materials, Color::srgb(0.17, 0.20, 0.22), 0.92),
        floor: material(&mut materials, Color::srgb(0.32, 0.25, 0.16), 0.98),
        door: material(&mut materials, Color::srgb(0.20, 0.105, 0.045), 0.86),
        glass: material(&mut materials, Color::srgb(0.18, 0.42, 0.56), 0.35),
        void: material(&mut materials, Color::srgb(0.025, 0.022, 0.018), 1.0),
        stair: material(&mut materials, Color::srgb(0.35, 0.23, 0.11), 0.9),
        room_floors: [
            Color::srgb(0.47, 0.24, 0.18),
            Color::srgb(0.25, 0.39, 0.51),
            Color::srgb(0.42, 0.46, 0.25),
            Color::srgb(0.52, 0.40, 0.20),
            Color::srgb(0.37, 0.29, 0.48),
            Color::srgb(0.24, 0.46, 0.42),
            Color::srgb(0.53, 0.31, 0.40),
        ]
        .into_iter()
        .map(|color| material(&mut materials, color, 0.98))
        .collect(),
    }
}

fn spawn_ground(world: &mut World, dimensions: Vec2) {
    let mesh = world.resource_mut::<Assets<Mesh>>().add(
        Plane3d::default()
            .mesh()
            .size(dimensions.x * 2.4, dimensions.y * 2.4),
    );
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.38, 0.22),
            perceptual_roughness: 1.0,
            unlit: true,
            ..default()
        });
    world.spawn((
        Name::new("ground"),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, -0.02, 0.0),
    ));
}

fn spawn_wall(
    world: &mut World,
    palette: &RenderPalette,
    wall: WallSegment,
    opening: Option<&Opening>,
    origin: Vec2,
    base_y: f32,
    storey_height: f32,
    style: WallStyle,
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
    let material = match style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    if let Some(opening) = opening {
        let side_width = (CELL_SIZE_METRES - opening.width_metres) * 0.5;
        for sign in [-1.0, 1.0] {
            let offset = sign * (opening.width_metres + side_width) * 0.5;
            if horizontal {
                centre.x += offset;
            } else {
                centre.y += offset;
            }
            spawn_wall_box(
                world,
                material,
                horizontal,
                side_width,
                storey_height,
                centre,
                base_y,
                "wall pier",
            );
            if horizontal {
                centre.x -= offset;
            } else {
                centre.y -= offset;
            }
        }
        if opening.sill_metres > 0.0 {
            spawn_wall_box_at_height(
                world,
                material,
                horizontal,
                opening.width_metres,
                opening.sill_metres,
                centre,
                base_y + opening.sill_metres * 0.5,
                "wall below opening",
            );
        }
        let header_base = opening.sill_metres + opening.height_metres;
        if header_base < storey_height {
            let header_height = storey_height - header_base;
            spawn_wall_box_at_height(
                world,
                material,
                horizontal,
                opening.width_metres,
                header_height,
                centre,
                base_y + header_base + header_height * 0.5,
                "wall header",
            );
        }
        spawn_opening_depth(
            world, palette, wall, *opening, horizontal, centre, outward, base_y,
        );
    } else {
        spawn_wall_box(
            world,
            material,
            horizontal,
            CELL_SIZE_METRES,
            storey_height,
            centre,
            base_y,
            "wall",
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
    spawn_box(
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

fn spawn_wall_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    horizontal: bool,
    length: f32,
    height: f32,
    centre: Vec2,
    base_y: f32,
    name: &'static str,
) {
    spawn_wall_box_at_height(
        world,
        material,
        horizontal,
        length,
        height,
        centre,
        base_y + height * 0.5,
        name,
    );
}

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
    let size = if horizontal {
        Vec3::new(length.max(0.02), height.max(0.02), WALL_THICKNESS_METRES)
    } else {
        Vec3::new(WALL_THICKNESS_METRES, height.max(0.02), length.max(0.02))
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

fn spawn_roof(
    world: &mut World,
    palette: &RenderPalette,
    mut roof: RoofPiece,
    origin: Vec2,
    roof_index: usize,
    wall_style: WallStyle,
) {
    roof.centre += origin;
    match roof.kind {
        RoofKind::Gable => spawn_gable_roof(world, palette, roof, wall_style),
        RoofKind::Hip | RoofKind::HalfHip | RoofKind::Pavilion => {
            let mesh = roof_surface_mesh(roof);
            let handle = world.resource_mut::<Assets<Mesh>>().add(mesh);
            world.spawn((
                Name::new(format!("roof piece {roof_index}")),
                Mesh3d(handle),
                MeshMaterial3d(palette.roof_secondary.clone()),
            ));
        }
        RoofKind::Shed => spawn_shed_roof(world, &palette.roof, roof),
        RoofKind::Flat => spawn_box(
            world,
            &palette.roof_secondary,
            Vec3::new(roof.size.x, 0.18, roof.size.y),
            Vec3::new(roof.centre.x, roof.base_height_metres + 0.09, roof.centre.y),
            Quat::IDENTITY,
            "flat roof",
        ),
        RoofKind::Conical => spawn_conical_roof(world, &palette.roof_secondary, roof),
    }
}

fn spawn_gable_roof(
    world: &mut World,
    palette: &RenderPalette,
    roof: RoofPiece,
    wall_style: WallStyle,
) {
    let pitch = roof.pitch_degrees.to_radians();
    let (span, run) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x * 0.5 + roof.eave_metres,
            roof.size.y + roof.eave_metres * 2.0,
        ),
        RidgeAxis::X => (
            roof.size.y * 0.5 + roof.eave_metres,
            roof.size.x + roof.eave_metres * 2.0,
        ),
    };
    let slope = span / pitch.cos();
    let rise = span * pitch.tan();
    for sign in [-1.0_f32, 1.0] {
        let (size, translation, rotation) = match roof.ridge_axis {
            RidgeAxis::Z => (
                Vec3::new(slope, 0.13, run),
                Vec3::new(
                    roof.centre.x + sign * span * 0.5,
                    roof.base_height_metres + rise * 0.5,
                    roof.centre.y,
                ),
                Quat::from_rotation_z(-sign * pitch),
            ),
            RidgeAxis::X => (
                Vec3::new(run, 0.13, slope),
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres + rise * 0.5,
                    roof.centre.y + sign * span * 0.5,
                ),
                Quat::from_rotation_x(sign * pitch),
            ),
        };
        spawn_box(
            world,
            &palette.roof,
            size,
            translation,
            rotation,
            "gable roof slope",
        );
    }
    let facade_material = match wall_style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    let half_x = roof.size.x * 0.5;
    let half_z = roof.size.y * 0.5;
    let triangles = match roof.ridge_axis {
        RidgeAxis::Z => {
            let south = roof.centre.y - half_z;
            let north = roof.centre.y + half_z;
            vec![
                vec![
                    Vec3::new(roof.centre.x - half_x, roof.base_height_metres, south),
                    Vec3::new(roof.centre.x, roof.base_height_metres + rise, south),
                    Vec3::new(roof.centre.x + half_x, roof.base_height_metres, south),
                ],
                vec![
                    Vec3::new(roof.centre.x + half_x, roof.base_height_metres, north),
                    Vec3::new(roof.centre.x, roof.base_height_metres + rise, north),
                    Vec3::new(roof.centre.x - half_x, roof.base_height_metres, north),
                ],
            ]
        }
        RidgeAxis::X => {
            let west = roof.centre.x - half_x;
            let east = roof.centre.x + half_x;
            vec![
                vec![
                    Vec3::new(west, roof.base_height_metres, roof.centre.y + half_z),
                    Vec3::new(west, roof.base_height_metres + rise, roof.centre.y),
                    Vec3::new(west, roof.base_height_metres, roof.centre.y - half_z),
                ],
                vec![
                    Vec3::new(east, roof.base_height_metres, roof.centre.y - half_z),
                    Vec3::new(east, roof.base_height_metres + rise, roof.centre.y),
                    Vec3::new(east, roof.base_height_metres, roof.centre.y + half_z),
                ],
            ]
        }
    };
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(flat_face_mesh(&triangles));
    world.spawn((
        Name::new("gable infill"),
        Mesh3d(mesh),
        MeshMaterial3d(facade_material.clone()),
    ));
    spawn_gable_detail(world, palette, roof, rise, wall_style);
}

fn spawn_gable_detail(
    world: &mut World,
    palette: &RenderPalette,
    roof: RoofPiece,
    rise: f32,
    wall_style: WallStyle,
) {
    let (half_span, face_a, face_b, tangent) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x * 0.5,
            Vec2::new(roof.centre.x, roof.centre.y - roof.size.y * 0.5 - 0.02),
            Vec2::new(roof.centre.x, roof.centre.y + roof.size.y * 0.5 + 0.02),
            Vec2::X,
        ),
        RidgeAxis::X => (
            roof.size.y * 0.5,
            Vec2::new(roof.centre.x - roof.size.x * 0.5 - 0.02, roof.centre.y),
            Vec2::new(roof.centre.x + roof.size.x * 0.5 + 0.02, roof.centre.y),
            Vec2::Y,
        ),
    };
    if wall_style == WallStyle::TimberFrame {
        for face in [face_a, face_b] {
            let apex = Vec3::new(face.x, roof.base_height_metres + rise, face.y);
            let base_left = face - tangent * half_span;
            let base_right = face + tangent * half_span;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(base_left.x, roof.base_height_metres, base_left.y),
                Vec3::new(base_right.x, roof.base_height_metres, base_right.y),
                0.13,
                "gable tie beam",
            );
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(face.x, roof.base_height_metres, face.y),
                apex,
                0.11,
                "gable king post",
            );
            let collar_y = roof.base_height_metres + rise * 0.56;
            let collar_half = half_span * 0.44;
            let collar_left = face - tangent * collar_half;
            let collar_right = face + tangent * collar_half;
            spawn_timber_beam(
                world,
                &palette.timber,
                Vec3::new(collar_left.x, collar_y, collar_left.y),
                Vec3::new(collar_right.x, collar_y, collar_right.y),
                0.105,
                "gable collar beam",
            );
            for fraction in [-0.66_f32, -0.33, 0.33, 0.66] {
                let stud = face + tangent * half_span * fraction;
                let top_y = roof.base_height_metres + rise * (1.0 - fraction.abs());
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(stud.x, roof.base_height_metres, stud.y),
                    Vec3::new(stud.x, top_y, stud.y),
                    0.085,
                    "gable vertical stud",
                );
            }
            for sign in [-1.0, 1.0] {
                let foot = face + tangent * half_span * 0.1 * sign;
                let head = face + tangent * half_span * 0.62 * sign;
                spawn_timber_beam(
                    world,
                    &palette.timber,
                    Vec3::new(foot.x, roof.base_height_metres + 0.06, foot.y),
                    Vec3::new(head.x, roof.base_height_metres + rise * 0.38, head.y),
                    0.09,
                    "gable outward brace",
                );
            }
        }
    }
    match roof.gable_profile {
        GableProfile::Plain => {}
        GableProfile::Stepped => {
            let material = if wall_style == WallStyle::TimberFrame {
                &palette.timber
            } else {
                &palette.stone
            };
            for face in [face_a, face_b] {
                for sign in [-1.0, 1.0] {
                    for step in 0..4 {
                        let lower = step as f32 / 4.0;
                        let upper = (step + 1) as f32 / 4.0;
                        let outer = face + tangent * half_span * (1.0 - lower) * sign;
                        let inner = face + tangent * half_span * (1.0 - upper) * sign;
                        spawn_timber_beam(
                            world,
                            material,
                            Vec3::new(outer.x, roof.base_height_metres + rise * lower, outer.y),
                            Vec3::new(outer.x, roof.base_height_metres + rise * upper, outer.y),
                            0.16,
                            "stepped gable vertical",
                        );
                        spawn_timber_beam(
                            world,
                            material,
                            Vec3::new(outer.x, roof.base_height_metres + rise * upper, outer.y),
                            Vec3::new(inner.x, roof.base_height_metres + rise * upper, inner.y),
                            0.16,
                            "stepped gable tread",
                        );
                    }
                }
            }
        }
        GableProfile::Curved => {
            for face in [face_a, face_b] {
                for sign in [-1.0, 1.0] {
                    let outer = face + tangent * half_span * 0.82 * sign;
                    let shoulder = face + tangent * half_span * 0.42 * sign;
                    spawn_timber_beam(
                        world,
                        &palette.stone,
                        Vec3::new(outer.x, roof.base_height_metres + rise * 0.12, outer.y),
                        Vec3::new(
                            shoulder.x,
                            roof.base_height_metres + rise * 0.58,
                            shoulder.y,
                        ),
                        0.14,
                        "curved gable lower sweep",
                    );
                    spawn_timber_beam(
                        world,
                        &palette.stone,
                        Vec3::new(
                            shoulder.x,
                            roof.base_height_metres + rise * 0.58,
                            shoulder.y,
                        ),
                        Vec3::new(face.x, roof.base_height_metres + rise, face.y),
                        0.14,
                        "curved gable upper sweep",
                    );
                }
            }
        }
    }
}

fn spawn_roof_dormer(
    world: &mut World,
    palette: &RenderPalette,
    mut dormer: RoofDormer,
    origin: Vec2,
    wall_style: WallStyle,
) {
    dormer.centre += origin;
    let (horizontal, inward, roof_size, ridge_axis) = match dormer.facing {
        Direction::North => (
            true,
            -Vec2::Y,
            Vec2::new(dormer.width_metres, dormer.depth_metres),
            RidgeAxis::Z,
        ),
        Direction::South => (
            true,
            Vec2::Y,
            Vec2::new(dormer.width_metres, dormer.depth_metres),
            RidgeAxis::Z,
        ),
        Direction::East => (
            false,
            -Vec2::X,
            Vec2::new(dormer.depth_metres, dormer.width_metres),
            RidgeAxis::X,
        ),
        Direction::West => (
            false,
            Vec2::X,
            Vec2::new(dormer.depth_metres, dormer.width_metres),
            RidgeAxis::X,
        ),
    };
    let scale = if dormer.kind == DormerKind::TransverseGable {
        1.55
    } else {
        1.0
    };
    dormer.width_metres *= scale;
    let facade_material = match wall_style {
        WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
        WallStyle::Brick => &palette.brick,
        WallStyle::Stone => &palette.stone,
    };
    let facade_centre = dormer.centre + inward * 0.18;
    spawn_wall_box_at_height(
        world,
        facade_material,
        horizontal,
        dormer.width_metres,
        dormer.height_metres,
        facade_centre,
        dormer.base_height_metres + dormer.height_metres * 0.5,
        "roof dormer facade",
    );
    let window_width = dormer.width_metres * 0.42;
    let window_height = dormer.height_metres * 0.48;
    let window_y = dormer.base_height_metres + dormer.height_metres * 0.48;
    let pane = facade_centre + inward * (WALL_THICKNESS_METRES * 0.44);
    spawn_box(
        world,
        &palette.glass,
        if horizontal {
            Vec3::new(window_width, window_height, 0.025)
        } else {
            Vec3::new(0.025, window_height, window_width)
        },
        Vec3::new(pane.x, window_y, pane.y),
        Quat::IDENTITY,
        "recessed roof dormer glazing",
    );
    let tangent = if horizontal { Vec2::X } else { Vec2::Y };
    let frame = facade_centre - inward * (WALL_THICKNESS_METRES * 0.56);
    for sign in [-1.0, 1.0] {
        let jamb = frame + tangent * window_width * 0.5 * sign;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(jamb.x, window_y - window_height * 0.5, jamb.y),
            Vec3::new(jamb.x, window_y + window_height * 0.5, jamb.y),
            0.065,
            "dormer window jamb",
        );
    }
    for sign in [-1.0, 1.0] {
        let y = window_y + window_height * 0.5 * sign;
        spawn_timber_beam(
            world,
            &palette.timber,
            Vec3::new(
                frame.x - tangent.x * window_width * 0.5,
                y,
                frame.y - tangent.y * window_width * 0.5,
            ),
            Vec3::new(
                frame.x + tangent.x * window_width * 0.5,
                y,
                frame.y + tangent.y * window_width * 0.5,
            ),
            0.065,
            "dormer window sill or lintel",
        );
    }
    let roof = RoofPiece {
        kind: match dormer.kind {
            DormerKind::Hipped => RoofKind::Hip,
            DormerKind::Shed => RoofKind::Shed,
            DormerKind::Gabled | DormerKind::TransverseGable => RoofKind::Gable,
        },
        centre: dormer.centre + inward * dormer.depth_metres * 0.42,
        size: roof_size * Vec2::new(scale, 1.0),
        base_height_metres: dormer.base_height_metres + dormer.height_metres,
        pitch_degrees: 48.0,
        ridge_axis,
        eave_metres: 0.16,
        gable_profile: dormer.gable_profile,
    };
    match roof.kind {
        RoofKind::Gable => spawn_gable_roof(world, palette, roof, wall_style),
        RoofKind::Hip => {
            let mesh = world
                .resource_mut::<Assets<Mesh>>()
                .add(roof_surface_mesh(roof));
            world.spawn((
                Name::new("hipped roof dormer"),
                Mesh3d(mesh),
                MeshMaterial3d(palette.roof_secondary.clone()),
            ));
        }
        RoofKind::Shed => spawn_shed_roof(world, &palette.roof, roof),
        RoofKind::HalfHip | RoofKind::Flat | RoofKind::Pavilion | RoofKind::Conical => {}
    }
}

fn spawn_shed_roof(world: &mut World, material: &Handle<StandardMaterial>, roof: RoofPiece) {
    let pitch = roof.pitch_degrees.to_radians();
    let (span, run) = match roof.ridge_axis {
        RidgeAxis::Z => (
            roof.size.x + roof.eave_metres * 2.0,
            roof.size.y + roof.eave_metres * 2.0,
        ),
        RidgeAxis::X => (
            roof.size.y + roof.eave_metres * 2.0,
            roof.size.x + roof.eave_metres * 2.0,
        ),
    };
    let slope = span / pitch.cos();
    let (size, rotation) = match roof.ridge_axis {
        RidgeAxis::Z => (Vec3::new(slope, 0.13, run), Quat::from_rotation_z(-pitch)),
        RidgeAxis::X => (Vec3::new(run, 0.13, slope), Quat::from_rotation_x(pitch)),
    };
    spawn_box(
        world,
        material,
        size,
        Vec3::new(
            roof.centre.x,
            roof.base_height_metres + span * pitch.tan() * 0.5,
            roof.centre.y,
        ),
        rotation,
        "shed roof",
    );
}

fn roof_surface_mesh(roof: RoofPiece) -> Mesh {
    let half_x = roof.size.x * 0.5 + roof.eave_metres;
    let half_z = roof.size.y * 0.5 + roof.eave_metres;
    let (ridge_half, rise) = match roof.ridge_axis {
        RidgeAxis::Z => {
            let inset = if roof.kind == RoofKind::HalfHip {
                half_x * 0.42
            } else if roof.kind == RoofKind::Pavilion {
                half_z
            } else {
                half_x.min(half_z * 0.85)
            };
            (
                (half_z - inset).max(0.0),
                half_x * roof.pitch_degrees.to_radians().tan(),
            )
        }
        RidgeAxis::X => {
            let inset = if roof.kind == RoofKind::HalfHip {
                half_z * 0.42
            } else if roof.kind == RoofKind::Pavilion {
                half_x
            } else {
                half_z.min(half_x * 0.85)
            };
            (
                (half_x - inset).max(0.0),
                half_z * roof.pitch_degrees.to_radians().tan(),
            )
        }
    };
    let y = roof.base_height_metres;
    let corners = [
        Vec3::new(roof.centre.x - half_x, y, roof.centre.y - half_z),
        Vec3::new(roof.centre.x + half_x, y, roof.centre.y - half_z),
        Vec3::new(roof.centre.x + half_x, y, roof.centre.y + half_z),
        Vec3::new(roof.centre.x - half_x, y, roof.centre.y + half_z),
    ];
    let (ridge_a, ridge_b) = match roof.ridge_axis {
        RidgeAxis::Z => (
            Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half),
            Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half),
        ),
        RidgeAxis::X => (
            Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y),
            Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y),
        ),
    };
    let faces = match roof.ridge_axis {
        RidgeAxis::Z => vec![
            vec![corners[0], corners[3], ridge_b, ridge_a],
            vec![corners[2], corners[1], ridge_a, ridge_b],
            vec![corners[1], corners[0], ridge_a],
            vec![corners[3], corners[2], ridge_b],
        ],
        RidgeAxis::X => vec![
            vec![corners[1], corners[0], ridge_a, ridge_b],
            vec![corners[3], corners[2], ridge_b, ridge_a],
            vec![corners[0], corners[3], ridge_a],
            vec![corners[2], corners[1], ridge_b],
        ],
    };
    flat_face_mesh(&faces)
}

fn flat_face_mesh(faces: &[Vec<Vec3>]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for face in faces {
        if face.len() < 3 {
            continue;
        }
        let normal = (face[1] - face[0])
            .cross(face[2] - face[0])
            .normalize_or_zero();
        let base = positions.len() as u32;
        positions.extend(face.iter().map(|point| point.to_array()));
        normals.extend((0..face.len()).map(|_| normal.to_array()));
        for index in 1..face.len() - 1 {
            indices.extend_from_slice(&[base, base + index as u32, base + index as u32 + 1]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_tower(
    world: &mut World,
    palette: &RenderPalette,
    tower: RoundTower,
    origin: Vec2,
    view: ViewerView,
) {
    let centre = tower.centre + origin;
    if view != ViewerView::Cutaway {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(tower_shell_mesh(tower));
        world.spawn((
            Name::new("round tower shell with open firing loops"),
            Mesh3d(mesh),
            MeshMaterial3d(palette.stone.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres * 0.5, centre.y),
        ));
        let inner = world.resource_mut::<Assets<Mesh>>().add(Cylinder::new(
            (tower.radius_metres - tower.wall_thickness_metres).max(0.2),
            tower.wall_height_metres - 0.04,
        ));
        world.spawn((
            Name::new("dark tower embrasure interior"),
            Mesh3d(inner),
            MeshMaterial3d(palette.void.clone()),
            Transform::from_xyz(centre.x, tower.wall_height_metres * 0.5, centre.y),
        ));
        if let Some(mut roof) = tower.roof {
            roof.centre += origin;
            spawn_conical_roof(world, &palette.roof_secondary, roof);
        }
    } else {
        for level in 0..=0 {
            let mesh = world
                .resource_mut::<Assets<Mesh>>()
                .add(Cylinder::new(tower.radius_metres - 0.18, 0.12));
            world.spawn((
                Name::new("cutaway tower floor"),
                Mesh3d(mesh),
                MeshMaterial3d(palette.floor.clone()),
                Transform::from_xyz(centre.x, level as f32 * 3.4 + 0.06, centre.y),
            ));
        }
    }
    if view != ViewerView::Cutaway
        && let Some(kind) = tower.battlement
    {
        spawn_round_battlement(world, palette, tower, origin, kind);
    }
}

fn tower_shell_mesh(tower: RoundTower) -> Mesh {
    const SEGMENTS: usize = 64;
    let mut faces = Vec::new();
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
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let radial_a = Vec2::new(angle_a.cos(), angle_a.sin()) * tower.radius_metres;
        let radial_b = Vec2::new(angle_b.cos(), angle_b.sin()) * tower.radius_metres;
        let firing_loop = segment.is_multiple_of(SEGMENTS / 8);
        let mut solid_ranges = Vec::new();
        if firing_loop {
            let mut cursor = 0.0;
            for (low, high) in &slit_ranges {
                if cursor < *low {
                    solid_ranges.push((cursor, *low));
                }
                cursor = *high;
            }
            if cursor < tower.wall_height_metres {
                solid_ranges.push((cursor, tower.wall_height_metres));
            }
        } else {
            solid_ranges.push((0.0, tower.wall_height_metres));
        }
        for (low, high) in solid_ranges {
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
) {
    let centre = tower.centre + origin;
    let radius = tower.radius_metres
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
        let inner = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cylinder::new((radius - 0.24).max(0.2), 1.11));
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
            let corbel_position = centre + radial * (tower.radius_metres + 0.18);
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

fn spawn_stair(world: &mut World, palette: &RenderPalette, stair: Stair, origin: Vec2) {
    match stair {
        Stair::Straight {
            start,
            direction,
            base_height_metres,
            rise_metres,
            width_metres,
            tread_count,
        } => {
            let forward = match direction {
                Direction::North => Vec2::Y,
                Direction::East => Vec2::X,
                Direction::South => -Vec2::Y,
                Direction::West => -Vec2::X,
            };
            for tread in 0..tread_count {
                let progress = tread as f32 / tread_count.max(1) as f32;
                let position = start + origin + forward * progress * 3.8;
                spawn_box(
                    world,
                    &palette.stair,
                    Vec3::new(width_metres, 0.14, 0.28),
                    Vec3::new(
                        position.x,
                        base_height_metres + progress * rise_metres,
                        position.y,
                    ),
                    Quat::from_rotation_y(match direction {
                        Direction::North | Direction::South => 0.0,
                        Direction::East | Direction::West => std::f32::consts::FRAC_PI_2,
                    }),
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
                Mesh3d(mesh),
                MeshMaterial3d(palette.floor.clone()),
                Transform::from_xyz(centre.x, elevation_metres - 0.08, centre.y),
            ));
        }
    }
}

fn annulus_mesh(inner_radius: f32, outer_radius: f32, height: f32) -> Mesh {
    const SEGMENTS: usize = 64;
    let half_height = height * 0.5;
    let mut faces = Vec::with_capacity(SEGMENTS * 4);
    for segment in 0..SEGMENTS {
        let angle_a = segment as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let angle_b = (segment + 1) as f32 * std::f32::consts::TAU / SEGMENTS as f32;
        let direction_a = Vec2::new(angle_a.cos(), angle_a.sin());
        let direction_b = Vec2::new(angle_b.cos(), angle_b.sin());
        let outer_a = direction_a * outer_radius;
        let outer_b = direction_b * outer_radius;
        let inner_a = direction_a * inner_radius;
        let inner_b = direction_b * inner_radius;
        faces.push(vec![
            Vec3::new(inner_a.x, half_height, inner_a.y),
            Vec3::new(outer_a.x, half_height, outer_a.y),
            Vec3::new(outer_b.x, half_height, outer_b.y),
            Vec3::new(inner_b.x, half_height, inner_b.y),
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
            Vec3::new(outer_b.x, half_height, outer_b.y),
            Vec3::new(outer_a.x, half_height, outer_a.y),
        ]);
        faces.push(vec![
            Vec3::new(inner_b.x, -half_height, inner_b.y),
            Vec3::new(inner_a.x, -half_height, inner_a.y),
            Vec3::new(inner_a.x, half_height, inner_a.y),
            Vec3::new(inner_b.x, half_height, inner_b.y),
        ]);
    }
    flat_face_mesh(&faces)
}

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

fn spawn_bartizan(world: &mut World, palette: &RenderPalette, bartizan: Bartizan, origin: Vec2) {
    let centre = bartizan.centre + origin;
    let shell = world.resource_mut::<Assets<Mesh>>().add(Cylinder::new(
        bartizan.radius_metres,
        bartizan.height_metres,
    ));
    world.spawn((
        Name::new("corbelled bartizan"),
        Mesh3d(shell),
        MeshMaterial3d(palette.stone.clone()),
        Transform::from_xyz(
            centre.x,
            bartizan.base_height_metres + bartizan.height_metres * 0.5,
            centre.y,
        ),
    ));
    for index in 0..6 {
        let angle = index as f32 * std::f32::consts::TAU / 6.0;
        let radial = Vec2::new(angle.cos(), angle.sin());
        let support = centre + radial * bartizan.radius_metres * 0.58;
        spawn_box(
            world,
            &palette.stone,
            Vec3::new(0.22, 0.75, 0.22),
            Vec3::new(support.x, bartizan.base_height_metres - 0.26, support.y),
            Quat::from_rotation_y(-angle),
            "bartizan corbel",
        );
    }
    if bartizan.roofed {
        let roof = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cone::new(bartizan.radius_metres + 0.18, 1.55));
        world.spawn((
            Name::new("bartizan roof"),
            Mesh3d(roof),
            MeshMaterial3d(palette.roof.clone()),
            Transform::from_xyz(
                centre.x,
                bartizan.base_height_metres + bartizan.height_metres + 0.76,
                centre.y,
            ),
        ));
    }
}

fn spawn_box(
    world: &mut World,
    material: &Handle<StandardMaterial>,
    size: Vec3,
    translation: Vec3,
    rotation: Quat,
    name: &'static str,
) {
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(size.x, size.y, size.z));
    world.spawn((
        Name::new(name),
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        Transform {
            translation,
            rotation,
            ..default()
        },
    ));
}

fn capture_when_ready(
    mut commands: Commands,
    mut state: ResMut<CaptureState>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    cameras: Query<&Camera, With<Camera3d>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(output) = state.output.clone() else {
        return;
    };
    if state.in_flight {
        return;
    }
    if state.settled < state.settle_frames {
        state.settled += 1;
        return;
    }
    state.manifest.observed_mesh_count = meshes.iter().count();
    state.manifest.visible_mesh_count = meshes.iter().filter(|visible| visible.get()).count();
    state.manifest.active_camera_count = cameras.iter().filter(|camera| camera.is_active).count();
    state.in_flight = true;
    if !state.primed {
        commands.spawn(Screenshot::primary_window()).observe(
            |_: On<ScreenshotCaptured>, mut state: ResMut<CaptureState>| {
                state.primed = true;
                state.settled = 0;
                state.in_flight = false;
            },
        );
        return;
    }

    let manifest_path = output.with_extension("capture.json");
    let mut manifest = state.manifest.clone();
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            manifest.subject_pixel_bps = subject_pixel_bps(captured.image.data.as_deref());
            manifest.validation_passed = manifest.subject_pixel_bps >= 100;
            save_to_disk(&output)(captured);
            fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&manifest).expect("serialize capture manifest"),
            )
            .expect("write capture manifest");
            if manifest.validation_passed {
                let _ = fs::remove_file(output.with_extension("failure.txt"));
                exit.write(AppExit::Success);
            } else {
                fs::write(
                    output.with_extension("failure.txt"),
                    "capture contains less than one percent non-background content\n",
                )
                .expect("write capture failure");
                exit.write(AppExit::Error(1.try_into().expect("one is non-zero")));
            }
        },
    );
    let _ = &mut exit;
}

fn subject_pixel_bps(data: Option<&[u8]>) -> u16 {
    let Some(data) = data else {
        return 0;
    };
    let (pixels, _) = data.as_chunks::<4>();
    let Some((reference, remaining)) = pixels.split_first() else {
        return 0;
    };
    let mut total = 1_usize;
    let mut different = 0_usize;
    for pixel in remaining {
        total += 1;
        if pixel[..3]
            .iter()
            .zip(&reference[..3])
            .any(|(channel, background)| channel.abs_diff(*background) > 8)
        {
            different += 1;
        }
    }
    (different.saturating_mul(10_000) / total).min(10_000) as u16
}
