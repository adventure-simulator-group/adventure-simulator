use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

use crate::plugins::{DistanceField, MarchingCubes};

const GRID_SIZE: usize = 36;
const ISO_LEVEL: f32 = 0.0; // This should always be 0.0, it's the level that defines where the surface lies.

#[derive(Resource)]
pub struct MarchingCubesConfig {
    radius: f32,
    min_radius: f32,
    max_radius: f32,
    voxel_size: f32,
    min_voxel_size: f32,
    max_voxel_size: f32,
    dirty: bool,
}

impl Default for MarchingCubesConfig {
    fn default() -> Self {
        let voxel_size = 0.12;
        let min_radius = GRID_SIZE as f32 * voxel_size * 0.1;
        let max_radius = GRID_SIZE as f32 * voxel_size * 0.6;
        let radius = GRID_SIZE as f32 * voxel_size * 0.35;

        Self {
            radius,
            min_radius,
            max_radius,
            voxel_size,
            min_voxel_size: 0.05,
            max_voxel_size: 0.5,
            dirty: true,
        }
    }
}

#[derive(Resource, Clone)]
pub struct MarchingCubesMeshHandle(Handle<Mesh>);

pub struct MarchingCubesPlugin;

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<MarchingCubesConfig>()
            .add_systems(Startup, Self::setup)
            .add_systems(EguiPrimaryContextPass, Self::marching_cubes_ui)
            .add_systems(Update, Self::update_marching_cubes_mesh);
    }
}

impl MarchingCubesPlugin {
    fn setup(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut config: ResMut<MarchingCubesConfig>,
    ) {
        let mut distance_field = DistanceField::new(GRID_SIZE, GRID_SIZE, GRID_SIZE);
        distance_field.add_sphere(
            Vec3::splat(-config.radius * 0.3),
            config.radius,
            config.voxel_size,
        );
        distance_field.add_sphere(
            Vec3::splat(config.radius * 0.3),
            config.radius,
            config.voxel_size,
        );

        let mesh = MarchingCubes::generate_mesh(&distance_field, ISO_LEVEL, config.voxel_size);

        let mesh_handle = meshes.add(mesh);
        commands.insert_resource(MarchingCubesMeshHandle(mesh_handle.clone()));
        config.dirty = false;

        let material_handle = materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9),
            metallic: 0.05,
            perceptual_roughness: 0.4,
            ..default()
        });

        commands.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::default(),
        ));

        commands.spawn((
            PointLight {
                intensity: 4000.0,
                range: 40.0,
                shadows_enabled: true,
                ..default()
            },
            Transform::from_xyz(8.0, 12.0, 8.0),
        ));
    }

    fn marching_cubes_ui(mut contexts: EguiContexts, mut config: ResMut<MarchingCubesConfig>) {
        if let Ok(ctx) = contexts.ctx_mut() {
            egui::Window::new("Marching Cubes")
                .default_width(220.0)
                .show(ctx, |ui| {
                    ui.label("Sphere Radius");
                    let mut current = config.radius;
                    let slider =
                        egui::Slider::new(&mut current, config.min_radius..=config.max_radius)
                            .text("meters");
                    if ui.add(slider).changed() {
                        config.radius = current;
                        config.dirty = true;
                    }
                    ui.label(format!("{:.2}", config.radius));

                    ui.separator();

                    ui.label("Voxel Size");
                    let mut current_voxel = config.voxel_size;
                    let slider_voxel = egui::Slider::new(
                        &mut current_voxel,
                        config.min_voxel_size..=config.max_voxel_size,
                    )
                    .text("meters");
                    if ui.add(slider_voxel).changed() {
                        config.voxel_size = current_voxel;
                        config.dirty = true;
                    }
                    ui.label(format!("{:.3}", config.voxel_size));
                });
        }
    }

    fn update_marching_cubes_mesh(
        mut config: ResMut<MarchingCubesConfig>,
        mesh_handle: Option<Res<MarchingCubesMeshHandle>>,
        mut meshes: ResMut<Assets<Mesh>>,
    ) {
        let Some(mesh_handle) = mesh_handle else {
            return;
        };

        if !config.dirty {
            return;
        }

        let mut distance_field = DistanceField::new(GRID_SIZE, GRID_SIZE, GRID_SIZE);
        distance_field.add_sphere(
            Vec3::splat(-config.radius * 0.3),
            config.radius,
            config.voxel_size,
        );
        distance_field.add_sphere(
            Vec3::splat(config.radius * 0.3),
            config.radius,
            config.voxel_size,
        );

        let mesh = MarchingCubes::generate_mesh(&distance_field, ISO_LEVEL, config.voxel_size);

        if let Some(existing) = meshes.get_mut(&mesh_handle.0) {
            *existing = mesh;
        }
        config.dirty = false;
    }
}
