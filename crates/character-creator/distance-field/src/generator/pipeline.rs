use gpu_runtime::prelude::*;

pub fn generate(context: &WgpuContext) -> Result<(Texture3D, Texture3D, Texture3D, Texture2D)> {
    let size = Vec3::new(512.0, 512.0, 512.0);
    let vertex_shader = VertexShader::new(context, include_str!("vertex_shader.wgsl").into())?;
    let fragment_shader =
        FragmentShader::new(context, include_str!("fragment_shader.wgsl").into())?;
    let resolution = Vec2::new(512.0, 512.0);
    let shapes = String::from(include_str!("shapes.wgsl"));
    let map = String::from(include_str!("map.wgsl"));
    let entrypoint = String::from(include_str!("entrypoint.wgsl"));
    let sampler = Sampler::new(
        context,
        Some(SamplerAddressMode::ClampToEdge.into()),
        Some(SamplerAddressMode::ClampToEdge.into()),
        Some(SamplerAddressMode::ClampToEdge.into()),
        Some(SamplerFilterMode::Linear.into()),
        Some(SamplerFilterMode::Linear.into()),
    )?;
    let distance_field = Texture3D::new(context, size, TextureFormat::R32Float.into())?;
    let bone_index = Texture3D::new(context, size, TextureFormat::Rgba8Uint.into())?;
    let bone_weight = Texture3D::new(context, size, TextureFormat::Rgba32Float.into())?;
    let render_pipeline = RenderPipeline::new(
        context,
        vertex_shader,
        fragment_shader,
        PrimitiveTopology::TriangleStrip.into(),
        CullMode::None.into(),
        FrontFace::Ccw.into(),
        vec![],
    )?;
    let texture2d = Texture2D::new(
        context,
        Some(resolution),
        Some(TextureFormat::Rgba8Unorm.into()),
    )?;
    let compute_shader = format!("{}\n{}\n{}", shapes, map, entrypoint);
    let mut parameters = PassParameters::new();
    parameters.insert("distance_", distance_field.clone());
    let color_attachment = ColorAttachment::new(
        texture2d.clone(),
        Some(LoadOp::Clear.into()),
        None,
        Some(StoreOp::Store.into()),
    );
    let compute_shader = ComputeShader::new(context, compute_shader)?;
    parameters.insert("bone_index", bone_index.clone());
    let render_attachments = RenderAttachments::new(vec![color_attachment], None);
    let compute_pipeline = ComputePipeline::new(context, compute_shader)?;
    parameters.insert("bone_weight", bone_weight.clone());
    let _compute_pass = ComputePass::new(
        context,
        compute_pipeline,
        parameters.clone(),
        64 as u32,
        64 as u32,
        128 as u32,
    )?;
    parameters.insert("my_sampler".to_string(), PassParameter::Sampler(sampler));
    parameters.insert("resolution".to_string(), PassParameter::Vec2(resolution));
    let _render_pass = RenderPass::new(
        context,
        render_pipeline,
        render_attachments,
        parameters,
        vec![],
        None,
        0,
        4,
        0,
        1,
    )?;
    Ok((distance_field, bone_index, bone_weight, texture2d))
}
