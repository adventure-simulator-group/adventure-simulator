mod compute_shader;
pub use compute_shader::*;

use crate::data::TextureFormat;
use crate::data::gpu::shader::{ReflectionData, parse_naga};
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ComputePipeline {
    pub shader: ComputeShader,
    pub bind_group_layouts: Vec<Arc<wgpu::BindGroupLayout>>,
    pub reflection: Option<Arc<ReflectionData>>,
    pub pipeline_cache: Arc<Mutex<HashMap<u64, Arc<wgpu::ComputePipeline>>>>,
    pub pipeline: Option<Arc<wgpu::ComputePipeline>>,
    pub validation_error: Arc<Mutex<Option<String>>>,
}

impl Default for ComputePipeline {
    fn default() -> Self {
        Self {
            shader: ComputeShader::default(),
            bind_group_layouts: Vec::new(),
            reflection: None,
            pipeline_cache: Arc::new(Mutex::new(HashMap::new())),
            pipeline: None,
            validation_error: Arc::new(Mutex::new(None)),
        }
    }
}

impl ComputePipeline {
    pub fn get_or_create_pipeline(
        &self,
        device: &wgpu::Device,
        entry_point: &str,
    ) -> anyhow::Result<Arc<wgpu::ComputePipeline>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Cache key based on entry point (shader code is part of shader struct but not strictly hashed here, assuming one pipeline per shadernode usually)
        // Ideally we should hash the shader code or module ID too if it changes.
        // For now lets hash the entry point.
        let mut s = DefaultHasher::new();
        entry_point.hash(&mut s);
        let cache_key = s.finish();

        let mut cache = self
            .pipeline_cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to lock pipeline cache"))?;
        if let Some(p) = cache.get(&cache_key) {
            return Ok(p.clone());
        }

        // Reset validation error
        if let Ok(mut guard) = self.validation_error.lock() {
            *guard = None;
        }

        // Create Pipeline
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        let module = self
            .shader
            .module
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ComputePipeline: Shader Module missing"))?;

        let layouts: Vec<_> = self.bind_group_layouts.iter().map(|l| l.as_ref()).collect();

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ComputePipeline Layout"),
            bind_group_layouts: &layouts,
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ComputePipeline"),
            layout: Some(&pipeline_layout),
            module,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // POP ERROR SCOPE
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen_futures::spawn_local;
            let device_clone = device.clone();
            let validation_error = self.validation_error.clone();
            spawn_local(async move {
                if let Some(e) = device_clone.pop_error_scope().await {
                    if let Ok(mut guard) = validation_error.lock() {
                        *guard = Some(e.to_string());
                    }
                }
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(e) = pollster::block_on(device.pop_error_scope()) {
            if let Ok(mut guard) = self.validation_error.lock() {
                *guard = Some(e.to_string());
            }
        }

        let p_arc = Arc::new(pipeline);
        cache.insert(cache_key, p_arc.clone());
        Ok(p_arc)
    }

    pub fn validate_interface(&self) -> anyhow::Result<()> {
        let naga_res = parse_naga(&self.shader.code, wgpu::naga::ShaderStage::Compute)
            .map_err(|e| anyhow::anyhow!("Compute Shader Parse Error: {}", e))?;

        if !naga_res
            .entry_points
            .iter()
            .any(|ep| ep.stage == wgpu::naga::ShaderStage::Compute)
        {
            return Err(anyhow::anyhow!("Compute Shader missing entry point"));
        }

        Ok(())
    }
}

impl ComputePipeline {
    pub fn new(context: &WgpuContext, shader: ComputeShader) -> Result<ComputePipeline> {
        let mut pipeline = {
            // --- REFLECTION ---
            let naga_res = parse_naga(&shader.code, wgpu::naga::ShaderStage::Compute)
                .map_err(|e| anyhow!("Compute Shader Parse Error for Reflection: {}", e))?;

            let entry_point = naga_res
                .entry_points
                .iter()
                .find(|ep| ep.stage == wgpu::naga::ShaderStage::Compute)
                .map(|ep| ep.name.clone())
                .ok_or_else(|| anyhow!("Compute Shader missing entry point"))?;

            let mut bind_groups_map: std::collections::BTreeMap<
                u32,
                crate::data::gpu::shader::BindGroupReflection,
            > = std::collections::BTreeMap::new();
            let mut bind_group_layouts_data: std::collections::BTreeMap<
                u32,
                Vec<wgpu::BindGroupLayoutEntry>,
            > = std::collections::BTreeMap::new();

            // Reflection of bindings
            for (_, var) in naga_res.global_variables.iter() {
                if let Some(binding_info) = &var.binding {
                    let group = binding_info.group;
                    let binding = binding_info.binding;
                    let name = var.name.clone().unwrap_or_default();

                    let group_reflection = bind_groups_map.entry(group).or_insert_with(|| {
                        crate::data::gpu::shader::BindGroupReflection {
                            index: group,
                            ..Default::default()
                        }
                    });
                    let layout_entries = bind_group_layouts_data.entry(group).or_default();

                    match var.space {
                        wgpu::naga::AddressSpace::Uniform => {
                            let ty = &naga_res.types[var.ty];
                            if let wgpu::naga::TypeInner::Struct { members, span } = &ty.inner {
                                group_reflection.uniform_buffer_size = *span;
                                group_reflection.uniform_binding = Some(binding);

                                for member in members {
                                    let member_name = member.name.clone().unwrap_or_default();
                                    let size =
                                        naga_res.types[member.ty].inner.size(naga_res.to_ctx());
                                    group_reflection.uniform_members.push(
                                        crate::data::shader::UniformMember {
                                            name: member_name,
                                            offset: member.offset,
                                            size,
                                        },
                                    );
                                }

                                layout_entries.push(wgpu::BindGroupLayoutEntry {
                                    binding: binding,
                                    visibility: wgpu::ShaderStages::COMPUTE,
                                    ty: wgpu::BindingType::Buffer {
                                        ty: wgpu::BufferBindingType::Uniform,
                                        has_dynamic_offset: false,
                                        min_binding_size: None,
                                    },
                                    count: None,
                                });
                            } else {
                                // Support non-struct uniforms (primitive scalars or vectors)
                                group_reflection.uniform_buffer_size = ty.inner.size(naga_res.to_ctx());
                                group_reflection.uniform_binding = Some(binding);
                                
                                layout_entries.push(wgpu::BindGroupLayoutEntry {
                                    binding: binding,
                                    visibility: wgpu::ShaderStages::COMPUTE,
                                    ty: wgpu::BindingType::Buffer {
                                        ty: wgpu::BufferBindingType::Uniform,
                                        has_dynamic_offset: false,
                                        min_binding_size: None,
                                    },
                                    count: None,
                                });
                            }
                        }
                        wgpu::naga::AddressSpace::Storage { access } => {
                            let read_only = !access.contains(wgpu::naga::StorageAccess::STORE);
                            group_reflection.buffer_bindings.push(
                                crate::data::shader::BufferBinding {
                                    name: name.clone(),
                                    binding,
                                    ty: wgpu::BufferBindingType::Storage { read_only },
                                },
                            );

                            layout_entries.push(wgpu::BindGroupLayoutEntry {
                                binding: binding,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                },
                                count: None,
                            });
                        }
                        wgpu::naga::AddressSpace::Handle => {
                            let ty = &naga_res.types[var.ty];
                            match &ty.inner {
                                wgpu::naga::TypeInner::Image { dim, class, .. } => {
                                    let view_dimension = match dim {
                                        wgpu::naga::ImageDimension::D1 => {
                                            wgpu::TextureViewDimension::D1
                                        }
                                        wgpu::naga::ImageDimension::D2 => {
                                            wgpu::TextureViewDimension::D2
                                        }
                                        wgpu::naga::ImageDimension::D3 => {
                                            wgpu::TextureViewDimension::D3
                                        }
                                        wgpu::naga::ImageDimension::Cube => {
                                            wgpu::TextureViewDimension::Cube
                                        }
                                    };

                                    let storage = match class {
                                        wgpu::naga::ImageClass::Storage { format, access } => {
                                            Some((format, access))
                                        }
                                        _ => None,
                                    };

                                    let mut wgpu_format = None;

                                    if let Some((format, access)) = storage {
                                        let access =
                                            if access.contains(wgpu::naga::StorageAccess::STORE) {
                                                wgpu::StorageTextureAccess::WriteOnly
                                            } else {
                                                wgpu::StorageTextureAccess::ReadOnly
                                            };

                                        let fmt = TextureFormat::naga_to_wgpu_format(*format);
                                        wgpu_format = Some(fmt);

                                        layout_entries.push(wgpu::BindGroupLayoutEntry {
                                            binding: binding,
                                            visibility: wgpu::ShaderStages::COMPUTE,
                                            ty: wgpu::BindingType::StorageTexture {
                                                access,
                                                format: fmt,
                                                view_dimension,
                                            },
                                            count: None,
                                        });
                                    } else {
                                        let sample_type = match class {
                                            wgpu::naga::ImageClass::Sampled { kind, .. } => {
                                                match kind {
                                                    wgpu::naga::ScalarKind::Float => {
                                                        wgpu::TextureSampleType::Float {
                                                            filterable: true,
                                                        }
                                                    }
                                                    wgpu::naga::ScalarKind::Uint => {
                                                        wgpu::TextureSampleType::Uint
                                                    }
                                                    wgpu::naga::ScalarKind::Sint => {
                                                        wgpu::TextureSampleType::Sint
                                                    }
                                                    _ => wgpu::TextureSampleType::Float {
                                                        filterable: true,
                                                    },
                                                }
                                            }
                                            wgpu::naga::ImageClass::Depth { .. } => {
                                                wgpu::TextureSampleType::Depth
                                            }
                                            _ => {
                                                wgpu::TextureSampleType::Float { filterable: true }
                                            }
                                        };

                                        layout_entries.push(wgpu::BindGroupLayoutEntry {
                                            binding: binding,
                                            visibility: wgpu::ShaderStages::COMPUTE,
                                            ty: wgpu::BindingType::Texture {
                                                multisampled: false,
                                                view_dimension,
                                                sample_type,
                                            },
                                            count: None,
                                        });
                                    }

                                    group_reflection.texture_bindings.push(
                                        crate::data::shader::TextureBinding {
                                            name: name.clone(),
                                            binding,
                                            format: wgpu_format,
                                            dimension: view_dimension,
                                        },
                                    );
                                }
                                wgpu::naga::TypeInner::Sampler { .. } => {
                                    group_reflection
                                        .sampler_bindings
                                        .push((name.clone(), binding));
                                    layout_entries.push(wgpu::BindGroupLayoutEntry {
                                        binding: binding,
                                        visibility: wgpu::ShaderStages::COMPUTE,
                                        ty: wgpu::BindingType::Sampler(
                                            wgpu::SamplerBindingType::Filtering,
                                        ),
                                        count: None,
                                    });
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }

            let mut bind_group_layouts = Vec::new();
            let mut bind_groups_reflection = Vec::new();

            for (group, mut entries) in bind_group_layouts_data {
                entries.sort_by_key(|e| e.binding);
                let layout =
                    context
                        .device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: Some(&format!("ComputePipeline Bind Layout Group {}", group)),
                            entries: &entries,
                        });
                bind_group_layouts.push(Arc::new(layout));

                if let Some(reflection) = bind_groups_map.remove(&group) {
                    bind_groups_reflection.push(reflection);
                }
            }

            let reflection = Arc::new(ReflectionData {
                bind_groups: bind_groups_reflection,
                fragment_entry_point: String::new(),
                vertex_entry_point: entry_point.clone(),
            });

            ComputePipeline {
                shader: shader.clone(),
                bind_group_layouts,
                reflection: Some(reflection),
                ..Default::default()
            }
        };

        // Ensure shader is up to date
        pipeline.shader = shader.clone();

        // Bake WGPU Resource
        // 1. Synchronous Naga Validation
        pipeline.validate_interface()?;

        // 2. WGPU Bake
        let reflection = pipeline.reflection.as_ref().unwrap();
        // We stored compute entry in vertex_entry_point for now
        let entry = &reflection.vertex_entry_point;

        if let Ok(p_wgpu) = pipeline.get_or_create_pipeline(&context.device, entry) {
            pipeline.pipeline = Some(p_wgpu);
        }

        if let Ok(guard) = pipeline.validation_error.lock() {
            if let Some(err) = guard.as_ref() {
                return Err(anyhow!("ComputePipeline Creation Error: {}", err));
            }
        }

        Ok(pipeline)
    }
}
