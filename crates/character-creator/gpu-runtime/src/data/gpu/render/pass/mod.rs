use crate::data::RenderPipeline;
use crate::data::matrix::{Mat2, Mat3, Mat4};
use crate::data::transform::Transform;
use crate::data::vector::{Vec2, Vec3, Vec4};
use crate::data::{RenderAttachments, Sampler, Texture2D, Texture3D};
use crate::globals::WgpuContext;
use anyhow::anyhow;
use gpu_runtime_base::{Result, Value};
use indexmap::IndexMap;
use wgpu::util::DeviceExt;

pub struct RenderPass;


impl RenderPass {
    pub fn new(
        context: &WgpuContext,

        pipeline_def: RenderPipeline,
        attachments: Option<RenderAttachments>,
        parameters: Option<IndexMap<String, Value>>,
    ) -> Result<RenderAttachments> {
        let attachments = attachments.unwrap_or_default();
        let parameters = parameters.unwrap_or_default();

        if attachments.colors.is_empty() && attachments.depth_stencil.is_none() {
            return Ok(attachments);
        }

        // --- GET PIPELINE FROM INPUT ---
        let pipeline_val = pipeline_def
            .pipeline
            .as_ref()
            .ok_or_else(|| anyhow!("RenderPass: RenderPipeline missing actual WGPU pipeline."))?;

        // Check for validation errors in the pipeline
        if let Ok(guard) = pipeline_def.validation_error.lock() {
            if let Some(err) = guard.as_ref() {
                return Err(anyhow!(
                    "RenderPass: Cannot use invalid RenderPipeline: {}",
                    err
                ));
            }
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

        if pipeline_def.baked_color_formats != current_color_formats
            || pipeline_def.baked_depth_format != current_depth_format
        {
            return Err(anyhow!(
                "RenderPass: Pipeline format mismatch. Pipeline was baked for {:?}/{:?}, but RenderPass has {:?}/{:?}.",
                pipeline_def.baked_color_formats,
                pipeline_def.baked_depth_format,
                current_color_formats,
                current_depth_format
            ));
        }

        let pipeline = pipeline_val;

        let bind_group_layout = pipeline_def
            .bind_group_layout
            .as_ref()
            .ok_or_else(|| anyhow!("RenderPass: RenderPipeline missing BindGroupLayout"))?;
        let reflection = pipeline_def
            .reflection
            .as_ref()
            .ok_or_else(|| anyhow!("RenderPass: RenderPipeline missing ReflectionData"))?;

        // Ensure state exists - handled by macro

        // 1. Uniforms
        let buffer = if reflection.uniform_buffer_size > 0 {
            let mut buffer_data = vec![0u8; reflection.uniform_buffer_size as usize];

            for member in &reflection.uniform_members {
                if let Some(val) = parameters.get(&member.name) {
                    if let Some(n) = val.as_number() {
                        let bytes = (*n as f32).to_le_bytes();
                        if (member.offset as usize + 4) <= buffer_data.len() {
                            buffer_data[member.offset as usize..member.offset as usize + 4]
                                .copy_from_slice(&bytes);
                        }
                    } else if let Some((arc, _)) = val.as_any() {
                        if let Some(v) = arc.downcast_ref::<Vec2>() {
                            let bytes_x = v.x.to_le_bytes();
                            let bytes_y = v.y.to_le_bytes();
                            if (member.offset as usize + 8) <= buffer_data.len() {
                                let start = member.offset as usize;
                                buffer_data[start..start + 4].copy_from_slice(&bytes_x);
                                buffer_data[start + 4..start + 8].copy_from_slice(&bytes_y);
                            }
                        } else if let Some(v) = arc.downcast_ref::<Vec3>() {
                            let bytes_x = v.x.to_le_bytes();
                            let bytes_y = v.y.to_le_bytes();
                            let bytes_z = v.z.to_le_bytes();
                            if (member.offset as usize + 12) <= buffer_data.len() {
                                let start = member.offset as usize;
                                buffer_data[start..start + 4].copy_from_slice(&bytes_x);
                                buffer_data[start + 4..start + 8].copy_from_slice(&bytes_y);
                                buffer_data[start + 8..start + 12].copy_from_slice(&bytes_z);
                            }
                        } else if let Some(v) = arc.downcast_ref::<Vec4>() {
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
                        } else if let Some(v) = arc.downcast_ref::<Mat2>() {
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
                        } else if let Some(v) = arc.downcast_ref::<Mat3>() {
                            if (member.offset as usize + member.size as usize) <= buffer_data.len()
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
                        } else if let Some(v) = arc.downcast_ref::<Mat4>() {
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
                        } else if let Some(v) = arc.downcast_ref::<Transform>() {
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
                    }
                }
            }

            let buffer = context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("RenderPass Uniforms"),
                    contents: &buffer_data,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
            Some(buffer)
        } else {
            None
        };

        // 2. Bind Group
        let mut bind_group_entries = Vec::new();

        if let Some(buffer) = &buffer {
            if let Some(uniform_binding_idx) = reflection.uniform_binding {
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: uniform_binding_idx,
                    resource: buffer.as_entire_binding(),
                });
            }
        }

        for binding_info in &reflection.texture_bindings {
            let name = &binding_info.name;
            let binding = binding_info.binding;
            let val = parameters.get(name).ok_or_else(|| {
                anyhow!(
                    "RenderPass: Parameter '{}' (texture binding {}) not found",
                    name,
                    binding
                )
            })?;

            if let Some((arc, _)) = val.as_any() {
                let (view, actual_format, dim) = if let Some(tex) = arc.downcast_ref::<Texture2D>()
                {
                    (
                        tex.view.as_ref(),
                        tex.texture.as_ref().map(|t| t.format()),
                        wgpu::TextureViewDimension::D2,
                    )
                } else if let Some(tex) = arc.downcast_ref::<Texture3D>() {
                    (
                        tex.view.as_ref(),
                        tex.texture.as_ref().map(|t| t.format()),
                        wgpu::TextureViewDimension::D3,
                    )
                } else {
                    return Err(anyhow!(
                        "RenderPass: Parameter '{}' is not a Texture2D or Texture3D",
                        name
                    ));
                };

                // Synchronous Validation
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
            } else {
                return Err(anyhow!("RenderPass: Parameter '{}' is not a texture", name));
            }
        }

        for (name, binding) in &reflection.sampler_bindings {
            let val = parameters.get(name).ok_or_else(|| {
                anyhow!(
                    "RenderPass: Parameter '{}' (sampler binding {}) not found",
                    name,
                    binding
                )
            })?;

            if let Some((arc, _)) = val.as_any() {
                if let Some(s) = arc.downcast_ref::<Sampler>() {
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
            } else {
                return Err(anyhow!("RenderPass: Parameter '{}' is not a sampler", name));
            }
        }

        let bind_group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RenderPass Bind Group"),
                layout: &bind_group_layout,
                entries: &bind_group_entries,
            });

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
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..4, 0..1);
        }
        context.queue.submit(std::iter::once(encoder.finish()));

        Ok(attachments)
    }
}
