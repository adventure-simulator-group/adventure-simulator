use anyhow::{Context, Result};
use gpu_runtime::{
    data::{ComputePass, ComputePipeline, ComputeShader, Texture3D, TextureFormat, Vec3},
    globals::WgpuContext,
};
use gpu_runtime_base::{std::Object, Value};

pub fn generate(context: &WgpuContext) -> Result<Texture3D> {
    let v0 = Vec3::new(512.0, 512.0, 512.0);
    let v1 = String::from(include_str!("shapes.wgsl"));
    let v2 = String::from(include_str!("object.wgsl"));
    let v3 = String::from(include_str!("entrypoint.wgsl"));
    let v4 = Texture3D::new(context, v0, TextureFormat::R32Float)?;
    let v5 = format!("{}\n{}\n{}", v1, v2, v3);
    let v6 = Object::insert(Default::default(), "output_tex".into(), Value::new_any(v4));
    let v7 = ComputeShader::new(context, v5)?;
    let v8 = ComputePipeline::new(context, v7)?;
    let v9 = ComputePass::new(context, v8, v6, 64, 64, 128)?;
    let v10 = Object::get(v9, "output_tex".into())
        .expect("Texture")
        .as_any()
        .context("Not any")?
        .0
        .downcast_ref::<Texture3D>()
        .unwrap()
        .clone();
    Ok(v10)
}
