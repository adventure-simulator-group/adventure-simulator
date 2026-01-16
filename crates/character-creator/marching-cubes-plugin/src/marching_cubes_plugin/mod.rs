use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

use crate::MarchingCubes;
use distance_field_plugin::{
    SdfBox, SdfShape, SdfSphere, components::{DistanceFieldComponent, SdfShapeComponent}
};

const GRID_SIZE: usize = 36;
const ISO_LEVEL: f32 = 0.0;

#[derive(Resource)]
pub struct MarchingCubesUIState {
    radius: f32,
    min_radius: f32,
    max_radius: f32,
    min_voxel_size: f32,
    max_voxel_size: f32,
}

impl Default for MarchingCubesUIState {
    fn default() -> Self {
        let voxel_size = 0.12;
        let min_radius = GRID_SIZE as f32 * voxel_size * 0.1;
        let max_radius = GRID_SIZE as f32 * voxel_size * 0.6;
        let radius = GRID_SIZE as f32 * voxel_size * 0.35;

        Self {
            radius,
            min_radius,
            max_radius,
            min_voxel_size: 0.05,
            max_voxel_size: 0.5,
        }
    }
}

#[derive(Resource, Clone)]
pub struct MarchingCubesMeshHandle(Handle<Mesh>);

pub struct MarchingCubesPlugin;

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<MarchingCubesUIState>()
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
        ui_state: Res<MarchingCubesUIState>,
    ) {
        // Create Mesh Handle
        let mesh = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default()); 
        let mesh_handle = meshes.add(mesh);
        
        // Register Mesh Handle resource? 
        // No, with multiple fields, we can't have a single handle resource driving them all easily unless we want them all to share the same mesh (which they don't).
        // The `update_marching_cubes_mesh` system will query entities.
        // So we just attach `Mesh3d` to the volume entity.
        // We'll rename `MarchingCubesMeshHandle` or just remove it if it's unused.
        // But `marching_cubes_ui` might need it? No, UI updates components.
        
        let material_handle = materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.7, 0.9),
            metallic: 0.05,
            perceptual_roughness: 0.4,
            ..default()
        });

        // Spawn Volume
        let volume_entity = commands.spawn((
            DistanceFieldComponent::new(GRID_SIZE, GRID_SIZE, GRID_SIZE, 0.12),
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(material_handle.clone()),
            Transform::from_xyz(0.0, 2.0, 0.0), // Raised up
        )).id();
        
        // Spawn Shapes as Children
        commands.entity(volume_entity).with_children(|parent| {
            parent.spawn((
                SdfShapeComponent::from(SdfSphere { radius: ui_state.radius }),
                Transform::from_xyz(-ui_state.radius * 0.3, -ui_state.radius * 0.3, -ui_state.radius * 0.3),
            ));

            parent.spawn((
                SdfShapeComponent::from(SdfSphere { radius: ui_state.radius }),
                Transform::from_xyz(ui_state.radius * 0.3, ui_state.radius * 0.3, ui_state.radius * 0.3),
            ));
        });

        // Spawn a SECOND independent SDF volume to verify multi-SDF support
        let volume_entity_2 = commands.spawn((
            DistanceFieldComponent::new(GRID_SIZE / 2, GRID_SIZE / 2, GRID_SIZE / 2, 0.15),
            Mesh3d(mesh_handle.clone()), 
            Transform::from_xyz(5.0, 3.0, 0.0), // Raised up and offset
        )).id();
        
        // We need to create a new mesh asset for the second volume
        let mesh_2 = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
        let mesh_handle_2 = meshes.add(mesh_2);
        
        // Re-spawn volume 2 with correct mesh handle
        commands.entity(volume_entity_2).insert((
            Mesh3d(mesh_handle_2),
            MeshMaterial3d(material_handle.clone()),
        ));

        // Add shapes to second volume
        commands.entity(volume_entity_2).with_children(|parent| {
             parent.spawn((
                SdfShapeComponent::from(SdfBox { size: Vec3::splat(0.6) }),
                Transform::IDENTITY,
            ));
             parent.spawn((
                SdfShapeComponent::from(SdfBox { size: Vec3::new(0.2, 1.0, 0.2) }), // Make it smaller to fit
                Transform::from_xyz(0.5, 0.0, 0.5),
            ));
        });

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

    fn marching_cubes_ui(
        mut contexts: EguiContexts,
        mut ui_state: ResMut<MarchingCubesUIState>,
        mut shapes: Query<&mut SdfShapeComponent>,
        mut fields: Query<&mut DistanceFieldComponent>,
    ) {
        if let Ok(ctx) = contexts.ctx_mut() {
            egui::Window::new("Marching Cubes")
                .default_width(220.0)
                .show(ctx, |ui| {
                    ui.label("Sphere Radius");
                    let mut current = ui_state.radius;
                    let slider =
                        egui::Slider::new(&mut current, ui_state.min_radius..=ui_state.max_radius)
                            .text("meters");
                    if ui.add(slider).changed() {
                        ui_state.radius = current;
                        // Update all spheres
                        for mut shape in shapes.iter_mut() {
                            if let SdfShapeComponent(SdfShape::Sphere(sphere)) = &mut *shape {
                                sphere.radius = current; // Simplification: set all spheres to same radius
                            }
                        }
                    }
                    ui.label(format!("{:.2}", ui_state.radius));

                    ui.separator();

                    ui.label("Voxel Size");
                    // We just pick the first one to display? Or local state?
                    let mut current_voxel = fields.iter().next().map(|c| c.voxel_size).unwrap_or(0.12);
                    
                    let slider_voxel = egui::Slider::new(
                        &mut current_voxel,
                        ui_state.min_voxel_size..=ui_state.max_voxel_size,
                    )
                    .text("meters");
                    if ui.add(slider_voxel).changed() {
                        for mut field in fields.iter_mut() {
                            field.voxel_size = current_voxel;
                        }
                    }
                    ui.label(format!("{:.3}", current_voxel));
                });
        }
    }

    fn update_marching_cubes_mesh(
        mut meshes: ResMut<Assets<Mesh>>,
        // Iterate all entities that have a DistanceField and an SdfConfig and a Mesh
        query: Query<(&DistanceFieldComponent, &Mesh3d), Changed<DistanceFieldComponent>>,
    ) {
        for (distance_field, mesh3d) in query.iter() {
             // We only run if changed (Changed filter handles this efficienty)
             // Generate Mesh
             let mesh = MarchingCubes::generate_mesh(distance_field, ISO_LEVEL, distance_field.voxel_size);
             
             if let Some(existing) = meshes.get_mut(&mesh3d.0) {
                 *existing = mesh;
             }
        }
    }
}
