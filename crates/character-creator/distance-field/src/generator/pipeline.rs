use gpu_runtime::prelude::*;

pub fn generate(context: &WgpuContext, size: Vec3) -> Result<(Texture3D, Texture3D, Texture3D, Texture2D)> {
    let vertex_shader = VertexShader::new(context, include_str!("vertex_shader.wgsl").into())?;
    let fragment_shader =
        FragmentShader::new(context, include_str!("fragment_shader.wgsl").into())?;
    let resolution = Vec2::new(size.x, size.y); // For the 2D texture, use X and Y
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
    let texture2d = Texture2D::new(context, resolution, TextureFormat::Rgba8Unorm.into())?;
    let compute_shader = format!("{}\n{}\n{}", shapes, map, entrypoint);
    let mut parameters = PassParameters::new();
    parameters.insert("distance_", distance_field.clone());
    let color_attachment = ColorAttachment::new(
        texture2d.clone(),
        Some(LoadOp::Clear.into()),
        None,
        Some(StoreOp::Store.into()),
    );
    let color_attachment2 = ColorAttachment::new(
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
    
    // Dynamic dispatch based on texture size and workgroup sizes in shaders
    // entrypoint.wgsl uses @workgroup_size(8, 8, 4)
    let dispatch_x = (size.x as u32 + 7) / 8;
    let dispatch_y = (size.y as u32 + 7) / 8;
    let dispatch_z = (size.z as u32 + 3) / 4;

    let _compute_pass = ComputePass::new(
        context,
        compute_pipeline,
        parameters.clone(),
        dispatch_x,
        dispatch_y,
        dispatch_z,
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
