use crate::data::{PassParameter, PassParameters, RenderAttachments, RenderPipeline};
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use wgpu::util::DeviceExt;

pub struct RenderPass;

impl RenderPass {
    pub fn new(
        context: &WgpuContext,
        pipeline_def: RenderPipeline,
        attachments: RenderAttachments,
        parameters: PassParameters,
        vertex_buffers: Vec<crate::data::gpu::buffer::Buffer>,
        index_buffer: Option<crate::data::gpu::buffer::Buffer>,
        vertex_start: u32,
        vertex_count: u32,
        instance_start: u32,
        instance_count: u32,
    ) -> Result<RenderAttachments> {
        if attachments.colors.is_empty() && attachments.depth_stencil.is_none() {
            return Ok(attachments);
        }

        // Verify Format Compatibility
        let current_color_formats: Vec<wgpu::TextureFormat> = attachments
            .colors
            .iter()
            .map(|c| {
                c.texture
                    .texture
                    .as_ref()
                    .map(|t| t.format())
                    .unwrap_or(wgpu::TextureFormat::Rgba8Unorm)
            })
            .collect();
        let current_depth_format = attachments
            .depth_stencil
            .as_ref()
            .and_then(|d| d.texture.texture.as_ref().map(|t| t.format()));

        let reflection = pipeline_def
            .reflection
            .as_ref()
            .ok_or_else(|| anyhow!("RenderPass: RenderPipeline missing ReflectionData"))?;

        // --- GET COMPATIBLE PIPELINE ---
        let pipeline_arc = pipeline_def.get_or_create_pipeline(
            &context.device,
            &current_color_formats,
            current_depth_format,
            &reflection.fragment_entry_point,
            &reflection.vertex_entry_point,
        )?;
        let pipeline = &pipeline_arc;

        // Check for validation errors in the pipeline
        if let Ok(guard) = pipeline_def.validation_error.lock() {
            if let Some(err) = guard.as_ref() {
                return Err(anyhow!(
                    "RenderPass: Cannot use invalid RenderPipeline: {}",
                    err
                ));
            }
        }

        let bind_group_layouts = &pipeline_def.bind_group_layouts;
        let reflection = pipeline_def
            .reflection
            .as_ref()
            .ok_or_else(|| anyhow!("RenderPass: RenderPipeline missing ReflectionData"))?;

        let mut bind_groups = Vec::new();

        for bg_reflection in &reflection.bind_groups {
            let layout = bind_group_layouts
                .get(bg_reflection.index as usize)
                .ok_or_else(|| {
                    anyhow!(
                        "RenderPass: Bind group layout index {} missing in pipeline",
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
                            "RenderPass Group {} Uniforms",
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
                        "RenderPass: Parameter '{}' (buffer binding {}) not found",
                        name,
                        binding
                    )
                })?;

                if let PassParameter::Buffer(gpu_buf) = val {
                    if let Some(wgpu_buf) = &gpu_buf.buffer {
                        bind_group_entries.push(wgpu::BindGroupEntry {
                            binding,
                            resource: wgpu_buf.as_entire_binding(),
                        });
                    } else {
                        return Err(anyhow!(
                            "RenderPass: Buffer '{}' has no WGPU resource",
                            name
                        ));
                    }
                } else {
                    return Err(anyhow!("RenderPass: Parameter '{}' is not a Buffer", name));
                }
            }

            // 3. Textures
            for binding_info in &bg_reflection.texture_bindings {
                let name = &binding_info.name;
                let binding = binding_info.binding;
                let val = parameters.get(name).ok_or_else(|| {
                    anyhow!(
                        "RenderPass: Parameter '{}' (texture binding {}) not found",
                        name,
                        binding
                    )
                })?;

                let (view, actual_format, dim) = match val {
                    PassParameter::Texture2D(tex) => (
                        tex.view.as_ref(),
                        tex.texture.as_ref().map(|t| t.format()),
                        wgpu::TextureViewDimension::D2,
                    ),
                    PassParameter::Texture3D(tex) => (
                        tex.view.as_ref(),
                        tex.texture.as_ref().map(|t| t.format()),
                        wgpu::TextureViewDimension::D3,
                    ),
                    _ => {
                        return Err(anyhow!(
                            "RenderPass: Parameter '{}' is not a Texture2D or Texture3D",
                            name
                        ));
                    }
                };

                if let Some(expected_format) = binding_info.format {
                    if let Some(actual_fmt) = actual_format {
                        if actual_fmt != expected_format {
                            return Err(anyhow!(
                                "RenderPass: Texture '{}' format mismatch. Expected {:?}, got {:?}",
                                name,
                                expected_format,
                                actual_fmt
                            ));
                        }
                    }
                }

                if dim != binding_info.dimension {
                    return Err(anyhow!(
                        "RenderPass: Texture '{}' dimension mismatch. Expected {:?}, got {:?}",
                        name,
                        binding_info.dimension,
                        dim
                    ));
                }

                if let Some(view) = view {
                    bind_group_entries.push(wgpu::BindGroupEntry {
                        binding: binding,
                        resource: wgpu::BindingResource::TextureView(view),
                    });
                } else {
                    return Err(anyhow!("RenderPass: Parameter '{}' is missing view", name));
                }
            }

            // 4. Samplers
            for (name, binding) in &bg_reflection.sampler_bindings {
                let val = parameters.get(name).ok_or_else(|| {
                    anyhow!(
                        "RenderPass: Parameter '{}' (sampler binding {}) not found",
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
                            "RenderPass: Sampler '{}' has no WGPU resource",
                            name
                        ));
                    }
                } else {
                    return Err(anyhow!("RenderPass: Parameter '{}' is not a Sampler", name));
                }
            }

            let bind_group = context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!(
                        "RenderPass Group {} Bind Group",
                        bg_reflection.index
                    )),
                    layout,
                    entries: &bind_group_entries,
                });
            bind_groups.push(bind_group);
        }

        // Frame: Render Pass
        let mut color_attachments = Vec::new();
        for att in &attachments.colors {
            color_attachments.push(Some(wgpu::RenderPassColorAttachment {
                view: &att.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: att.ops.load,
                    store: att.ops.store,
                },
            }));
        }

        let depth_stencil_attachment =
            attachments
                .depth_stencil
                .as_ref()
                .map(|d| wgpu::RenderPassDepthStencilAttachment {
                    view: &d.view,
                    depth_ops: d.depth_ops.as_ref().map(|o| wgpu::Operations {
                        load: o.load,
                        store: o.store,
                    }),
                    stencil_ops: d.stencil_ops.as_ref().map(|o| wgpu::Operations {
                        load: o.load,
                        store: o.store,
                    }),
                });

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RenderPass Encoder"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("RenderPass"),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render_pass.set_pipeline(pipeline);
            for (i, bg) in bind_groups.iter().enumerate() {
                render_pass.set_bind_group(i as u32, bg, &[]);
            }
            
            for (i, vbuf) in vertex_buffers.iter().enumerate() {
                if let Some(buf) = &vbuf.buffer {
                    render_pass.set_vertex_buffer(i as u32, buf.slice(..));
                }
            }

            if let Some(ibuf) = &index_buffer {
                if let Some(buf) = &ibuf.buffer {
                    render_pass.set_index_buffer(buf.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(
                        vertex_start as u32..(vertex_start + vertex_count) as u32,
                        0,
                        instance_start as u32..(instance_start + instance_count) as u32,
                    );
                } else {
                    render_pass.draw(
                        vertex_start as u32..(vertex_start + vertex_count) as u32,
                        instance_start as u32..(instance_start + instance_count) as u32,
                    );
                }
            } else {
                render_pass.draw(
                    vertex_start as u32..(vertex_start + vertex_count) as u32,
                    instance_start as u32..(instance_start + instance_count) as u32,
                );
            }
        }
        context.queue.submit(std::iter::once(encoder.finish()));

        Ok(attachments)
    }
}
