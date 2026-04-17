use crate::data::{PassParameter, ReflectionData};
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use indexmap::IndexMap;
use wgpu::util::DeviceExt;

pub fn create_bind_groups(
    context: &WgpuContext,
    bind_group_layouts: &[wgpu::BindGroupLayout],
    reflection: &ReflectionData,
    parameters: &IndexMap<String, PassParameter>,
    pass_name: &str,
) -> Result<Vec<wgpu::BindGroup>> {
    let mut bind_groups = Vec::new();

    for bg_reflection in &reflection.bind_groups {
        let layout = bind_group_layouts
            .get(bg_reflection.index as usize)
            .ok_or_else(|| {
                anyhow!(
                    "{}: Bind group layout index {} missing in pipeline",
                    pass_name,
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
                        "{} Group {} Uniforms",
                        pass_name, bg_reflection.index
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
                    "{}: Parameter '{}' (buffer binding {}) not found",
                    pass_name,
                    name,
                    binding
                )
            })?;

            if let PassParameter::Buffer(gpu_buf) = val {
                let wgpu_buf = &gpu_buf.buffer;
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu_buf.as_entire_binding(),
                });
            } else {
                return Err(anyhow!(
                    "{}: Parameter '{}' is not a Buffer",
                    pass_name,
                    name
                ));
            }
        }

        // 3. Textures
        for binding_info in &bg_reflection.texture_bindings {
            let name = &binding_info.name;
            let binding = binding_info.binding;
            let val = parameters.get(name).ok_or_else(|| {
                anyhow!(
                    "{}: Parameter '{}' (texture binding {}) not found",
                    pass_name,
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
                        "{}: Parameter '{}' is not a Texture2D or Texture3D",
                        pass_name,
                        name
                    ));
                }
            };

            if let Some(expected_format) = binding_info.format {
                if let Some(actual_fmt) = actual_format {
                    if actual_fmt != expected_format {
                        return Err(anyhow!(
                            "{}: Texture '{}' format mismatch. Expected {:?}, got {:?}",
                            pass_name,
                            name,
                            expected_format,
                            actual_fmt
                        ));
                    }
                }
            }

            if dim != binding_info.dimension {
                return Err(anyhow!(
                    "{}: Texture '{}' dimension mismatch. Expected {:?}, got {:?}",
                    pass_name,
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
                return Err(anyhow!(
                    "{}: Parameter '{}' is missing view",
                    pass_name,
                    name
                ));
            }
        }

        // 4. Samplers
        for (name, binding) in &bg_reflection.sampler_bindings {
            let val = parameters.get(name).ok_or_else(|| {
                anyhow!(
                    "{}: Parameter '{}' (sampler binding {}) not found",
                    pass_name,
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
                        "{}: Sampler '{}' has no WGPU resource",
                        pass_name,
                        name
                    ));
                }
            } else {
                return Err(anyhow!(
                    "{}: Parameter '{}' is not a Sampler",
                    pass_name,
                    name
                ));
            }
        }

        let bind_group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!(
                    "{} Group {} Bind Group",
                    pass_name, bg_reflection.index
                )),
                layout,
                entries: &bind_group_entries,
            });
        bind_groups.push(bind_group);
    }

    Ok(bind_groups)
}
