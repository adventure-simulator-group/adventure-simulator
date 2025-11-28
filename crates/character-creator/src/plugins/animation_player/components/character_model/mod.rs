use bevy::{prelude::*, render::render_resource::AsBindGroup, shader::ShaderRef};

const SHADER_ASSET_PATH: &str = "shaders/custom_material.wgsl";

// This struct defines the data that will be passed to your shader
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CustomMaterial {
    #[uniform(0)]
    color: LinearRgba,
    // #[texture(1)]
    // #[sampler(2)]
    // color_texture: Option<Handle<Image>>,
    alpha_mode: AlphaMode,
}

/// The Material trait is very configurable, but comes with sensible defaults for all methods.
/// You only need to implement functions for features that need non-default behavior. See the Material api docs for details!
impl Material for CustomMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

#[derive(Component)]
pub struct CharacterModel;

impl CharacterModel {
    pub fn spawn(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<CustomMaterial>>,
        // asset_server: Res<AssetServer>,
    ) {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::default())),
            MeshMaterial3d(materials.add(CustomMaterial {
                color: LinearRgba::BLUE,
                alpha_mode: AlphaMode::Blend,
            })),
            Transform::from_xyz(0.0, 1.0, -2.0),
            CharacterModel,
        ));
    }

    pub fn update(
        mut characters: Query<&mut Transform, With<CharacterModel>>,
        time: Res<Time>,
    ) {
        for mut transform in &mut characters {
            transform.rotation = Quat::from_rotation_y(time.elapsed_secs() as f32);
        }
    }
}