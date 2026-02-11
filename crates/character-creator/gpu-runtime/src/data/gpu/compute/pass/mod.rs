use crate::data::vector::{Vec2, Vec3, Vec4};
use crate::data::{ComputePipeline, Sampler, Texture2D, Texture3D};
use crate::globals::WgpuContext;
use anyhow::anyhow;
use gpu_runtime_base::{Result, Value};
use indexmap::IndexMap;
use wgpu::util::DeviceExt;

pub struct ComputePass;


impl ComputePass {
    pub fn new(
        context: &WgpuContext,
        pipeline_def: ComputePipeline,
        parameters: IndexMap<String, Value>,
        workgroups_x: u32,
        workgroups_y: u32,
        workgroups_z: u32,
    ) -> Result<IndexMap<String, Value>> {
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

        let bind_group_layout = pipeline_def
            .bind_group_layout
            .as_ref()
            .ok_or_else(|| anyhow!("ComputePass: ComputePipeline missing BindGroupLayout"))?;
        let reflection = pipeline_def
            .reflection
            .as_ref()
            .ok_or_else(|| anyhow!("ComputePass: ComputePipeline missing ReflectionData"))?;

        // Ensure state exists - handled by macro now using #[state(default)]

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
                        }
                    }
                }
            }

            let buffer = context
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("ComputePass Uniforms"),
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
                    "ComputePass: Parameter '{}' (texture binding {}) not found",
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
                        "ComputePass: Parameter '{}' is not a Texture2D or Texture3D",
                        name
                    ));
                };

                // Synchronous Validation
                if let Some(expected_format) = binding_info.format {
                    if let Some(actual_fmt) = actual_format {
                        if actual_fmt != expected_format {
                            return Err(anyhow!(
                                "ComputePass: Texture '{}' format mismatch. Expected {:?}, got {:?}",
                                name,
                                expected_format,
                                actual_fmt
                            ));
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
                    bind_group_entries.push(wgpu::BindGroupEntry {
                        binding: binding,
                        resource: wgpu::BindingResource::TextureView(view),
                    });
                } else {
                    return Err(anyhow!("ComputePass: Texture '{}' has no view", name));
                }
            } else {
                return Err(anyhow!(
                    "ComputePass: Parameter '{}' is not a texture",
                    name
                ));
            }
        }

        for (name, binding) in &reflection.sampler_bindings {
            let val = parameters.get(name).ok_or_else(|| {
                anyhow!(
                    "ComputePass: Parameter '{}' (sampler binding {}) not found",
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
            } else {
                return Err(anyhow!(
                    "ComputePass: Parameter '{}' is not a sampler",
                    name
                ));
            }
        }

        let bind_group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ComputePass Bind Group"),
                layout: &bind_group_layout,
                entries: &bind_group_entries,
            });

        // Frame: Compute Pass
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ComputePass Encoder"),
            });
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ComputePass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(workgroups_x, workgroups_y, workgroups_z);
        }
        context.queue.submit(std::iter::once(encoder.finish()));

        Ok(parameters)
    }
}
