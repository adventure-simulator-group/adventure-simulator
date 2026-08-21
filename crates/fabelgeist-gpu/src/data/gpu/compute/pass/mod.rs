use crate::data::{ComputePipeline, PassParameter, PassParameters};
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use wgpu::util::DeviceExt;

pub struct ComputePass;

impl ComputePass {
    pub fn new(
        context: &WgpuContext,
        pipeline_def: ComputePipeline,
        parameters: PassParameters,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) -> Result<()> {
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ComputePass Encoder"),
            });

        Self::record(
            context,
            &pipeline_def,
            &parameters,
            &mut encoder,
            workgroups_x,
            workgroups_y,
            workgroups_z,
        )?;

        context.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }

    pub fn record(
        context: &WgpuContext,
        pipeline_def: &ComputePipeline,
        parameters: &PassParameters,
        encoder: &mut wgpu::CommandEncoder,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) -> Result<()> {
        // --- GET PIPELINE FROM INPUT ---
        let pipeline_val = pipeline_def
            .pipeline
            .as_ref()
            .ok_or_else(|| anyhow!("ComputePass: ComputePipeline missing actual WGPU pipeline."))?;

        // Check for validation errors in the pipeline itself
        if let Ok(guard) = pipeline_def.validation_error.lock() {
            if let Some(err) = guard.as_ref() {
                return Err(anyhow!(
                    "ComputePass: Cannot use invalid ComputePipeline: {}",
                    err
                ));
            }
        }

        let pipeline = pipeline_val;

        let bind_group_layouts = &pipeline_def.bind_group_layouts;
        let reflection = pipeline_def
            .reflection
            .as_ref()
            .ok_or_else(|| anyhow!("ComputePass: ComputePipeline missing ReflectionData"))?;

        let mut bind_groups = Vec::new();

        for bg_reflection in &reflection.bind_groups {
            let layout = bind_group_layouts
                .get(bg_reflection.index as usize)
                .ok_or_else(|| {
                    anyhow!(
                        "ComputePass: Bind group layout index {} missing in pipeline",
                        bg_reflection.index
                    )
                })?;

            let mut bind_group_entries = Vec::new();

            // 1. Uniforms
            let mut uniform_buffer = None;
            if bg_reflection.uniform_buffer_size > 0 {
                let mut buffer_data = vec![0u8; bg_reflection.uniform_buffer_size as usize];

                for member in &bg_reflection.uniform_members {
                    if let Some(val) = parameters.get(&member.name) {
                        match val {
                            PassParameter::Number(n) => {
                                let bytes = (*n as f32).to_le_bytes();
                                if (member.offset as usize + 4) <= buffer_data.len() {
                                    buffer_data[member.offset as usize..member.offset as usize + 4]
                                        .copy_from_slice(&bytes);
                                }
                            }
                            PassParameter::Unsigned(u) => {
                                let bytes = u.to_le_bytes();
                                if (member.offset as usize + 4) <= buffer_data.len() {
                                    buffer_data[member.offset as usize..member.offset as usize + 4]
                                        .copy_from_slice(&bytes);
                                }
                            }
                            PassParameter::Vec2(v) => {
                                let bytes_x = v.x.to_le_bytes();
                                let bytes_y = v.y.to_le_bytes();
                                if (member.offset as usize + 8) <= buffer_data.len() {
                                    let start = member.offset as usize;
                                    buffer_data[start..start + 4].copy_from_slice(&bytes_x);
                                    buffer_data[start + 4..start + 8].copy_from_slice(&bytes_y);
                                }
                            }
                            PassParameter::Vec3(v) => {
                                let bytes_x = v.x.to_le_bytes();
                                let bytes_y = v.y.to_le_bytes();
                                let bytes_z = v.z.to_le_bytes();
                                if (member.offset as usize + 12) <= buffer_data.len() {
                                    let start = member.offset as usize;
                                    buffer_data[start..start + 4].copy_from_slice(&bytes_x);
                                    buffer_data[start + 4..start + 8].copy_from_slice(&bytes_y);
                                    buffer_data[start + 8..start + 12].copy_from_slice(&bytes_z);
                                }
                            }
                            PassParameter::Vec4(v) => {
                                let bytes_x = v.x.to_le_bytes();
                                let bytes_y = v.y.to_le_bytes();
                                let bytes_z = v.z.to_le_bytes();
                                let bytes_w = v.w.to_le_bytes();
                                if (member.offset as usize + 16) <= buffer_data.len() {
                                    let start = member.offset as usize;
                                    buffer_data[start..start + 4].copy_from_slice(&bytes_x);
                                    buffer_data[start + 4..start + 8].copy_from_slice(&bytes_y);
                                    buffer_data[start + 8..start + 12].copy_from_slice(&bytes_z);
                                    buffer_data[start + 12..start + 16].copy_from_slice(&bytes_w);
                                }
                            }
                            PassParameter::Mat2(v) => {
                                if (member.offset as usize + 16) <= buffer_data.len() {
                                    let start = member.offset as usize;
                                    for i in 0..2 {
                                        for j in 0..2 {
                                            let offset = start + (i * 2 + j) * 4;
                                            buffer_data[offset..offset + 4]
                                                .copy_from_slice(&v.columns[i][j].to_le_bytes());
                                        }
                                    }
                                }
                            }
                            PassParameter::Mat3(v) => {
                                if (member.offset as usize + member.size as usize)
                                    <= buffer_data.len()
                                {
                                    let start = member.offset as usize;
                                    let col_stride = member.size / 3;
                                    for i in 0..3 {
                                        for j in 0..3 {
                                            let offset =
                                                start + i as usize * col_stride as usize + j * 4;
                                            buffer_data[offset..offset + 4]
                                                .copy_from_slice(&v.columns[i][j].to_le_bytes());
                                        }
                                    }
                                }
                            }
                            PassParameter::Mat4(v) => {
                                if (member.offset as usize + 64) <= buffer_data.len() {
                                    let start = member.offset as usize;
                                    for i in 0..4 {
                                        for j in 0..4 {
                                            let offset = start + (i * 4 + j) * 4;
                                            buffer_data[offset..offset + 4]
                                                .copy_from_slice(&v.columns[i][j].to_le_bytes());
                                        }
                                    }
                                }
                            }
                            PassParameter::Transform(v) => {
                                let mat = v.to_mat4();
                                if (member.offset as usize + 64) <= buffer_data.len() {
                                    let start = member.offset as usize;
                                    for i in 0..4 {
                                        for j in 0..4 {
                                            let offset = start + (i * 4 + j) * 4;
                                            buffer_data[offset..offset + 4]
                                                .copy_from_slice(&mat.columns[i][j].to_le_bytes());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                let buffer = context
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!(
                            "ComputePass Group {} Uniforms",
                            bg_reflection.index
                        )),
                        contents: &buffer_data,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
                uniform_buffer = Some(buffer);
            }

            if let Some(buffer) = &uniform_buffer {
                if let Some(uniform_binding_idx) = bg_reflection.uniform_binding {
                    bind_group_entries.push(wgpu::BindGroupEntry {
                        binding: uniform_binding_idx,
                        resource: buffer.as_entire_binding(),
                    });
                }
            }

            // 2. Buffers (Storage / Uniform from Buffer)
            for buffer_binding in &bg_reflection.buffer_bindings {
                let name = &buffer_binding.name;
                let binding = buffer_binding.binding;
                let val = parameters.get(name).ok_or_else(|| {
                    anyhow!(
                        "ComputePass: Parameter '{}' (buffer binding {}) not found",
                        name,
                        binding
                    )
                })?;

                if let PassParameter::Buffer(gpu_buf) = val {
                    let binding_resource = if gpu_buf.size < gpu_buf.buffer.size() {
                        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &gpu_buf.buffer,
                            offset: 0,
                            size: Some(std::num::NonZeroU64::new(gpu_buf.size).unwrap()),
                        })
                    } else {
                        gpu_buf.buffer.as_entire_binding()
                    };
                    bind_group_entries.push(wgpu::BindGroupEntry {
                        binding,
                        resource: binding_resource,
                    });
                } else {
                    return Err(anyhow!("ComputePass: Parameter '{}' is not a Buffer", name));
                }
            }

            // 3. Textures
            let mut texture_views = Vec::new();
            for binding_info in &bg_reflection.texture_bindings {
                let name = &binding_info.name;
                let binding = binding_info.binding;
                let val = parameters.get(name).ok_or_else(|| {
                    anyhow!(
                        "ComputePass: Parameter '{}' (texture binding {}) not found",
                        name,
                        binding
                    )
                })?;

                let (view, actual_format, dim) =
                    match val {
                        PassParameter::Texture2d(tex) => {
                            let view = if let Some(expected_fmt) = binding_info.format {
                                let view_format = wgpu::TextureFormat::from(tex.format);
                                if view_format != expected_fmt {
                                    // Check for sRGB/Unorm alias
                                    if wgpu::TextureFormat::from(tex.format.srgb_counterpart())
                                        == expected_fmt
                                    {
                                        Some(tex.view_with_format(
                                            context,
                                            tex.format.srgb_counterpart(),
                                        )?)
                                    } else {
                                        tex.view.clone()
                                    }
                                } else {
                                    tex.view.clone()
                                }
                            } else {
                                tex.view.clone()
                            };

                            (
                                view,
                                tex.texture.as_ref().map(|t| t.format()),
                                wgpu::TextureViewDimension::D2,
                            )
                        }
                        PassParameter::Texture3d(tex) => {
                            let view = if let Some(expected_fmt) = binding_info.format {
                                let view_format = wgpu::TextureFormat::from(tex.format);
                                if view_format != expected_fmt {
                                    // Check for sRGB/Unorm alias
                                    if wgpu::TextureFormat::from(tex.format.srgb_counterpart())
                                        == expected_fmt
                                    {
                                        Some(tex.view_with_format(
                                            context,
                                            tex.format.srgb_counterpart(),
                                        )?)
                                    } else {
                                        tex.view.clone()
                                    }
                                } else {
                                    tex.view.clone()
                                }
                            } else {
                                tex.view.clone()
                            };

                            (
                                view,
                                tex.texture.as_ref().map(|t| t.format()),
                                wgpu::TextureViewDimension::D3,
                            )
                        }
                        _ => {
                            return Err(anyhow!(
                                "ComputePass: Parameter '{}' is not a Texture2d or Texture3d",
                                name
                            ));
                        }
                    };

                if let Some(expected_format) = binding_info.format {
                    if let Some(actual_fmt) = actual_format {
                        if actual_fmt != expected_format {
                            // Allow sRGB counterparts
                            let mut allowed = false;
                            if let PassParameter::Texture2d(tex) = val {
                                if wgpu::TextureFormat::from(tex.format.srgb_counterpart())
                                    == expected_format
                                {
                                    allowed = true;
                                }
                            } else if let PassParameter::Texture3d(tex) = val {
                                if wgpu::TextureFormat::from(tex.format.srgb_counterpart())
                                    == expected_format
                                {
                                    allowed = true;
                                }
                            }

                            if !allowed {
                                return Err(anyhow!(
                                    "ComputePass: Texture '{}' format mismatch. Expected {:?}, got {:?}",
                                    name,
                                    expected_format,
                                    actual_fmt
                                ));
                            }
                        }
                    }
                }

                if dim != binding_info.dimension {
                    return Err(anyhow!(
                        "ComputePass: Texture '{}' dimension mismatch. Expected {:?}, got {:?}",
                        name,
                        binding_info.dimension,
                        dim
                    ));
                }

                if let Some(view) = view {
                    texture_views.push((binding, view));
                } else {
                    return Err(anyhow!("ComputePass: Texture '{}' has no view", name));
                }
            }

            for (binding, view) in &texture_views {
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: wgpu::BindingResource::TextureView(view),
                });
            }

            // 4. Samplers
            for (name, binding) in &bg_reflection.sampler_bindings {
                let val = parameters.get(name).ok_or_else(|| {
                    anyhow!(
                        "ComputePass: Parameter '{}' (sampler binding {}) not found",
                        name,
                        binding
                    )
                })?;

                if let PassParameter::Sampler(s) = val {
                    if let Some(wgpu_sampler) = &s.sampler {
                        bind_group_entries.push(wgpu::BindGroupEntry {
                            binding: *binding,
                            resource: wgpu::BindingResource::Sampler(wgpu_sampler),
                        });
                    } else {
                        return Err(anyhow!(
                            "ComputePass: Sampler '{}' has no WGPU resource",
                            name
                        ));
                    }
                } else {
                    return Err(anyhow!(
                        "ComputePass: Parameter '{}' is not a Sampler",
                        name
                    ));
                }
            }

            let bind_group = context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!(
                        "ComputePass Group {} Bind Group",
                        bg_reflection.index
                    )),
                    layout,
                    entries: &bind_group_entries,
                });
            bind_groups.push(bind_group);
        }

        // Frame: Compute Pass
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ComputePass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(pipeline);
            for (i, bg) in bind_groups.iter().enumerate() {
                compute_pass.set_bind_group(i as u32, bg, &[]);
            }
            compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, workgroups_z);
        }

        Ok(())
    }
}
