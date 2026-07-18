use adventuresim_render_contracts::{MapPackage, Point, StartupMode};
use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    math::primitives::{Cuboid, Cylinder},
    picking::{
        mesh_picking::{MeshPickingCamera, MeshPickingPlugin, MeshPickingSettings},
        pointer::PointerButton,
        prelude::{Click, Pickable, Pointer},
    },
    prelude::*,
};

use crate::{RendererConfig, RendererMode, publish_marker_selection, renderer_suspended};

pub struct StrategicRendererPlugin;
#[derive(Component)]
struct StrategicEntity;
#[derive(Component)]
struct NavigableMarkerId(String);
#[derive(Component)]
struct Idle {
    phase: f32,
}
#[derive(Resource, Clone, Copy)]
struct MapTransform {
    min: Point,
    scale: f64,
}
#[derive(Resource, Default)]
struct DragIntent(f32);

impl Plugin for StrategicRendererPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .insert_resource(MeshPickingSettings {
                require_markers: true,
                ..default()
            })
            .init_resource::<DragIntent>()
            .add_systems(OnEnter(RendererMode::StrategicMap), setup)
            .add_systems(OnEnter(RendererMode::StrategicScene), setup)
            .add_systems(OnExit(RendererMode::StrategicMap), cleanup)
            .add_systems(OnExit(RendererMode::StrategicScene), cleanup)
            .add_systems(
                Update,
                (
                    set_strategic_render_active,
                    pan_zoom
                        .run_if(in_state(RendererMode::StrategicMap))
                        .run_if(strategic_running),
                    animate_idle
                        .run_if(in_state(RendererMode::StrategicScene))
                        .run_if(strategic_running),
                ),
            );
    }
}

fn setup(
    mut commands: Commands,
    config: Res<RendererConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-4., 10., 4.).looking_at(Vec3::ZERO, Vec3::Y),
        StrategicEntity,
    ));
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scale: 18.,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0., 18., 0.01).looking_at(Vec3::ZERO, Vec3::Z),
        MeshPickingCamera,
        StrategicEntity,
    ));
    match &config.0.startup {
        StartupMode::StrategicMap {
            package, overlay, ..
        } => spawn_map(&mut commands, &mut meshes, &mut materials, package, overlay),
        StartupMode::StrategicScene { scene } => {
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(18., 0.1, 12.))),
                MeshMaterial3d(materials.add(Color::srgb(0.16, 0.19, 0.15))),
                StrategicEntity,
            ));
            let cylinder = meshes.add(Cylinder::new(0.45, 1.8));
            for (index, actor) in scene.actors.iter().enumerate() {
                let col = index % 5;
                let row = index / 5;
                let [r, g, b] = actor.color_rgb;
                commands.spawn((
                    Mesh3d(cylinder.clone()),
                    MeshMaterial3d(materials.add(Color::srgb_u8(r, g, b))),
                    Transform::from_xyz(col as f32 * 1.6 - 3.2, 0.95, row as f32 * 1.8 - 1.5),
                    Idle {
                        phase: (index as f32 * 1.7) % 6.28,
                    },
                    StrategicEntity,
                    Name::new(actor.label.clone()),
                ));
            }
        }
        StartupMode::Tactical { .. } => {}
    }
}

fn spawn_map(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    package: &MapPackage,
    overlay: &adventuresim_render_contracts::MapOverlay,
) {
    let dx = (package.bounds.max.x - package.bounds.min.x).max(0.001);
    let dy = (package.bounds.max.y - package.bounds.min.y).max(0.001);
    let scale = 16. / dx.max(dy);
    let tx = MapTransform {
        min: package.bounds.min,
        scale,
    };
    commands.insert_resource(tx);
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(19., 0.08, 13.))),
        MeshMaterial3d(materials.add(Color::srgb(0.48, 0.42, 0.28))),
        StrategicEntity,
    ));
    let road_mesh = meshes.add(Cuboid::new(1., 0.035, 0.055));
    let road_material = materials.add(Color::srgb(0.18, 0.14, 0.09));
    for road in &package.roads {
        let a = map_point(road.from, tx);
        let b = map_point(road.to, tx);
        let d = b - a;
        commands.spawn((
            Mesh3d(road_mesh.clone()),
            MeshMaterial3d(road_material.clone()),
            Transform::from_translation((a + b) * 0.5 + Vec3::Y * 0.08)
                .with_rotation(Quat::from_rotation_y(-d.z.atan2(d.x)))
                .with_scale(Vec3::new(
                    d.length(),
                    1.,
                    if road.ferry { 0.45 } else { 1. },
                )),
            StrategicEntity,
        ));
    }
    let route_material = materials.add(Color::srgb(1.0, 0.62, 0.05));
    for pair in overlay.selected_route.windows(2) {
        let a = map_point(pair[0], tx);
        let b = map_point(pair[1], tx);
        let delta = b - a;
        commands.spawn((
            Mesh3d(road_mesh.clone()),
            MeshMaterial3d(route_material.clone()),
            Transform::from_translation((a + b) * 0.5 + Vec3::Y * 0.14)
                .with_rotation(Quat::from_rotation_y(-delta.z.atan2(delta.x)))
                .with_scale(Vec3::new(delta.length(), 1., 1.8)),
            StrategicEntity,
        ));
    }
    let town = meshes.add(Cylinder::new(0.13, 0.2));
    let town_mat = materials.add(Color::srgb(0.35, 0.07, 0.04));
    for s in &package.settlements {
        commands.spawn((
            Mesh3d(town.clone()),
            MeshMaterial3d(town_mat.clone()),
            Transform::from_translation(map_point(s.point, tx) + Vec3::Y * 0.16)
                .with_scale(Vec3::splat(1. + s.population_level.max(0) as f32 * 0.12)),
            StrategicEntity,
            Name::new(s.name.clone()),
        ));
    }
    for marker in &overlay.markers {
        let color = match marker.kind {
            adventuresim_render_contracts::MarkerKind::Party => Color::srgb(0.1, 0.3, 0.9),
            adventuresim_render_contracts::MarkerKind::ActiveQuest => Color::srgb(0.9, 0.1, 0.1),
            adventuresim_render_contracts::MarkerKind::SelectedDestination => {
                Color::srgb(1., 0.7, 0.05)
            }
            _ => Color::srgb(0.2, 0.7, 0.25),
        };
        let mut entity = commands.spawn((
            Mesh3d(town.clone()),
            MeshMaterial3d(materials.add(color)),
            Transform::from_translation(map_point(marker.point, tx) + Vec3::Y * 0.32)
                .with_scale(Vec3::splat(1.8)),
            StrategicEntity,
            Name::new(marker.label.clone()),
        ));
        if marker.href.is_some()
            && matches!(
                marker.kind,
                adventuresim_render_contracts::MarkerKind::Destination
                    | adventuresim_render_contracts::MarkerKind::SelectedDestination
                    | adventuresim_render_contracts::MarkerKind::ActiveQuest
            )
        {
            entity
                .insert((Pickable::default(), NavigableMarkerId(marker.id.clone())))
                .observe(on_marker_click);
        }
    }
}

fn on_marker_click(
    click: On<Pointer<Click>>,
    markers: Query<&NavigableMarkerId>,
    drag: Res<DragIntent>,
) {
    if click.button != PointerButton::Primary || drag.0 > 4.0 {
        return;
    }
    if let Ok(marker) = markers.get(click.entity) {
        publish_marker_selection(&marker.0);
    }
}
fn map_point(p: Point, t: MapTransform) -> Vec3 {
    Vec3::new(
        ((p.x - t.min.x) * t.scale - 8.) as f32,
        0.,
        ((p.y - t.min.y) * t.scale - 5.) as f32,
    )
}
fn pan_zoom(
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut camera: Single<(&mut Transform, &mut Projection), With<StrategicEntity>>,
    mut drag: ResMut<DragIntent>,
) {
    if buttons.pressed(MouseButton::Left) {
        let delta = motion.read().fold(Vec2::ZERO, |a, e| a + e.delta);
        drag.0 += delta.length();
        camera.0.translation.x -= delta.x * 0.015;
        camera.0.translation.z -= delta.y * 0.015;
    } else {
        drag.0 = 0.0;
    }
    let scroll: f32 = wheel.read().map(|e| e.y).sum();
    if let Projection::Orthographic(p) = &mut *camera.1 {
        p.scale = (p.scale * (1. - scroll * 0.1)).clamp(4., 40.);
    }
}

fn cleanup(mut commands: Commands, entities: Query<Entity, With<StrategicEntity>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<MapTransform>();
}
fn animate_idle(time: Res<Time>, mut actors: Query<(&Idle, &mut Transform)>) {
    for (idle, mut transform) in &mut actors {
        transform.translation.y = 0.95 + (time.elapsed_secs() * 1.8 + idle.phase).sin() * 0.035;
    }
}

fn strategic_running() -> bool {
    !renderer_suspended()
}

fn set_strategic_render_active(mut cameras: Query<&mut Camera, With<StrategicEntity>>) {
    let active = !renderer_suspended();
    for mut camera in &mut cameras {
        camera.is_active = active;
    }
}
