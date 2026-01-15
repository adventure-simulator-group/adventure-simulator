use bevy::{prelude::*, render::render_resource::AsBindGroup, shader::ShaderRef};

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SphereTracingMaterial {
    #[uniform(0)]
    pub color: LinearRgba,
    #[uniform(0)]
    pub sphere_params: Vec4, // xyz = center, w = radius
}

impl Material for SphereTracingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/sphere_tracing.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}
