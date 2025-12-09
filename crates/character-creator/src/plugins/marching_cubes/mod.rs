use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

mod tables;

use tables::{EDGE_TABLE, TRI_TABLE};

pub type Distance = f32;
pub type Voxel = bool;

#[derive(Clone)]
pub struct Field<T> {
    data: Vec<T>,
    width: usize,
    height: usize,
    depth: usize,
}

impl<T> Field<T> {
    pub fn get(&self, x: usize, y: usize, z: usize) -> &T {
        &self.data[x + y * self.width + z * self.width * self.height]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: T) {
        self.data[x + y * self.width + z * self.width * self.height] = value;
    }

    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.width, self.height, self.depth)
    }
}

impl DistanceField {
    pub fn new(width: usize, height: usize, depth: usize) -> Self {
        Self {
            data: vec![Distance::INFINITY; width * height * depth],
            width,
            height,
            depth,
        }
    }

    pub fn add_sphere(&mut self, center: Vec3, radius: f32, voxel_size: f32) {
        let (width, height, depth) = self.dimensions();
        let origin = MarchingCubes::grid_origin(width, height, depth, voxel_size);

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let world_position =
                        MarchingCubes::sample_to_world(origin, x, y, z, voxel_size);
                    let distance = world_position.distance(center) - radius;
                    let current = self.get(x, y, z);
                    self.set(x, y, z, current.min(distance));
                }
            }
        }
    }
}

impl VoxelField {
    pub fn new(width: usize, height: usize, depth: usize) -> Self {
        Self {
            data: vec![false; width * height * depth],
            width,
            height,
            depth,
        }
    }
}

pub type DistanceField = Field<Distance>;

pub type VoxelField = Field<Voxel>;

impl From<DistanceField> for VoxelField {
    fn from(field: DistanceField) -> Self {
        Self {
            data: field.data.iter().map(|&d| d <= 0.0).collect(),
            width: field.width,
            height: field.height,
            depth: field.depth,
        }
    }
}

const GRID_SIZE: usize = 36;
const ISO_LEVEL: f32 = 0.0; // This should always be 0.0, it's the level that defines where the surface lies.

#[derive(Resource)]
struct MarchingCubesConfig {
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
struct MarchingCubesMeshHandle(Handle<Mesh>);

pub struct MarchingCubesPlugin;

impl Plugin for MarchingCubesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .init_resource::<MarchingCubesConfig>()
            .add_systems(Startup, setup)
            .add_systems(EguiPrimaryContextPass, marching_cubes_ui)
            .add_systems(Update, update_marching_cubes_mesh);
    }
}

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
                let slider = egui::Slider::new(&mut current, config.min_radius..=config.max_radius)
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

struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }

    fn add_triangle(&mut self, v0: Vec3, v1: Vec3, v2: Vec3) {
        let normal = (v2 - v0).cross(v1 - v0);
        if normal.length_squared() <= f32::EPSILON {
            return;
        }
        let normal = normal.normalize();
        let normal_array = normal.to_array();

        let base_index = self.positions.len() as u32;

        self.positions.push(v0.to_array());
        self.positions.push(v2.to_array());
        self.positions.push(v1.to_array());

        self.normals.push(normal_array);
        self.normals.push(normal_array);
        self.normals.push(normal_array);

        const UV_SCALE: f32 = 0.25;
        self.uvs
            .push([v0.x * UV_SCALE + 0.5, v0.z * UV_SCALE + 0.5]);
        self.uvs
            .push([v2.x * UV_SCALE + 0.5, v2.z * UV_SCALE + 0.5]);
        self.uvs
            .push([v1.x * UV_SCALE + 0.5, v1.z * UV_SCALE + 0.5]);

        self.indices
            .extend([base_index, base_index + 1, base_index + 2]);
    }

    fn build(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

pub struct MarchingCubes;

impl MarchingCubes {
    pub fn generate_mesh(distance_field: &DistanceField, iso_level: f32, voxel_size: f32) -> Mesh {
        let (width, height, depth) = distance_field.dimensions();

        if width < 2 || height < 2 || depth < 2 {
            return Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
        }

        let origin = Self::grid_origin(width, height, depth, voxel_size);
        let mut builder = MeshBuilder::new();

        for z in 0..(depth - 1) {
            for y in 0..(height - 1) {
                for x in 0..(width - 1) {
                    let (cube_index, samples, corners) =
                        Self::get_cell_data(distance_field, origin, x, y, z, voxel_size, iso_level);

                    let edge_mask = EDGE_TABLE[cube_index];
                    if edge_mask == 0 {
                        continue;
                    }

                    let edge_vertices =
                        Self::calculate_intersections(edge_mask, iso_level, samples, corners);

                    Self::process_triangles(cube_index, &edge_vertices, &mut builder);
                }
            }
        }

        builder.build()
    }

    fn get_cell_data(
        distance_field: &DistanceField,
        origin: Vec3,
        x: usize,
        y: usize,
        z: usize,
        voxel_size: f32,
        iso_level: f32,
    ) -> (usize, [f32; 8], [Vec3; 8]) {
        let mut cube_index = 0usize;
        let mut samples = [0.0f32; 8];
        let mut corners = [Vec3::ZERO; 8];

        for (i, &(dx, dy, dz)) in VERTEX_OFFSETS.iter().enumerate() {
            let sx = x + dx;
            let sy = y + dy;
            let sz = z + dz;

            let d = *distance_field.get(sx, sy, sz);
            samples[i] = d;
            corners[i] = Self::sample_to_world(origin, sx, sy, sz, voxel_size);

            if d < iso_level {
                cube_index |= 1 << i;
            }
        }

        (cube_index, samples, corners)
    }

    fn calculate_intersections(
        edge_mask: i32,
        iso_level: f32,
        samples: [f32; 8],
        corners: [Vec3; 8],
    ) -> [Vec3; 12] {
        let mut edge_vertices = [Vec3::ZERO; 12];
        for edge in 0..12 {
            if (edge_mask & (1 << edge)) != 0 {
                let (v1, v2) = EDGE_CONNECTIONS[edge];
                edge_vertices[edge] = Self::vertex_interpolate(
                    iso_level,
                    corners[v1],
                    corners[v2],
                    samples[v1],
                    samples[v2],
                );
            }
        }
        edge_vertices
    }

    fn process_triangles(cube_index: usize, edge_vertices: &[Vec3; 12], builder: &mut MeshBuilder) {
        let mut tri_index = 0usize;
        while tri_index + 2 < TRI_TABLE[cube_index].len() && TRI_TABLE[cube_index][tri_index] != -1
        {
            let idx0 = TRI_TABLE[cube_index][tri_index] as usize;
            let idx1 = TRI_TABLE[cube_index][tri_index + 1] as usize;
            let idx2 = TRI_TABLE[cube_index][tri_index + 2] as usize;

            builder.add_triangle(
                edge_vertices[idx0],
                edge_vertices[idx1],
                edge_vertices[idx2],
            );

            tri_index += 3;
        }
    }

    pub fn grid_origin(width: usize, height: usize, depth: usize, voxel_size: f32) -> Vec3 {
        Vec3::new(
            -(width as f32 - 1.0) * 0.5 * voxel_size,
            -(height as f32 - 1.0) * 0.5 * voxel_size,
            -(depth as f32 - 1.0) * 0.5 * voxel_size,
        )
    }

    pub fn sample_to_world(origin: Vec3, x: usize, y: usize, z: usize, voxel_size: f32) -> Vec3 {
        origin
            + Vec3::new(
                x as f32 * voxel_size,
                y as f32 * voxel_size,
                z as f32 * voxel_size,
            )
    }

    fn vertex_interpolate(iso_level: f32, p1: Vec3, p2: Vec3, v1: f32, v2: f32) -> Vec3 {
        const EPSILON: f32 = 1.0e-6;

        if (iso_level - v1).abs() < EPSILON {
            return p1;
        }
        if (iso_level - v2).abs() < EPSILON {
            return p2;
        }
        if (v1 - v2).abs() < EPSILON {
            return p1;
        }

        let t = (iso_level - v1) / (v2 - v1);
        p1 + (p2 - p1) * t
    }
}

const VERTEX_OFFSETS: [(usize, usize, usize); 8] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (0, 1, 0),
    (0, 0, 1),
    (1, 0, 1),
    (1, 1, 1),
    (0, 1, 1),
];

const EDGE_CONNECTIONS: [(usize, usize); 12] = [
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
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marching_cubes_mesh_generation() {
        let size = 10;
        let voxel_size = 1.0;
        let radius = 2.5;

        let mut distance_field = DistanceField::new(size, size, size);
        distance_field.add_sphere(Vec3::ZERO, radius, voxel_size);

        let voxel_field: VoxelField = distance_field.clone().into();
        let mesh = MarchingCubes::generate_mesh(&distance_field, &voxel_field, 0.0, voxel_size);

        assert!(matches!(
            mesh.primitive_topology(),
            PrimitiveTopology::TriangleList
        ));

        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        // A sphere should generate some vertices
        assert!(positions.len() > 0);

        // Check if index count is divisible by 3 (triangles)
        if let Some(Indices::U32(indices)) = mesh.indices() {
            assert!(indices.len() > 0);
            assert_eq!(indices.len() % 3, 0);
        } else {
            panic!("Mesh should have U32 indices");
        }
    }
}
