use bevy::prelude::*;

mod material;
pub use material::*;

pub struct SphereTracingPlugin;

impl Plugin for SphereTracingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SphereTracingMaterial>::default())
            .add_systems(Startup, setup_scene);
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SphereTracingMaterial>>,
) {
    // Spawn a cube that will contain the sphere tracing volume
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.5, 2.5, 2.5))), // Ensure the cube is large enough to contain the sphere
        MeshMaterial3d(materials.add(SphereTracingMaterial {
            color: LinearRgba::GREEN,
            sphere_params: Vec4::new(0.0, 3.5, 0.0, 1.0),
        })),
        Transform::from_xyz(0.0, 1.5, 0.0),
    ));
}
