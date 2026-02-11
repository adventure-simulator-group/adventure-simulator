mod compute_shader;
pub use compute_shader::*;

use crate::data::{ReflectionData, UniformMember};
use crate::globals::WgpuContext;
use anyhow::anyhow;
use gpu_runtime_base::Result;
use naga::front::wgsl;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ComputePipeline {
    pub shader: ComputeShader,
    pub bind_group_layout: Option<Arc<wgpu::BindGroupLayout>>,
    pub reflection: Option<Arc<ReflectionData>>,
    pub pipeline_cache: Arc<Mutex<HashMap<u64, Arc<wgpu::ComputePipeline>>>>,
    pub pipeline: Option<Arc<wgpu::ComputePipeline>>,
    pub validation_error: Arc<Mutex<Option<String>>>,
}

impl Default for ComputePipeline {
    fn default() -> Self {
        Self {
            shader: ComputeShader::default(),
            bind_group_layout: None,
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
        let bind_group_layout = self
            .bind_group_layout
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ComputePipeline: BindGroupLayout missing"))?;

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ComputePipeline Layout"),
            bind_group_layouts: &[bind_group_layout],
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
        use naga::front::wgsl;

        let naga_res = wgsl::parse_str(&self.shader.code)
            .map_err(|e| anyhow::anyhow!("Compute Shader Parse Error: {}", e))?;

        if !naga_res
            .entry_points
            .iter()
            .any(|ep| ep.stage == naga::ShaderStage::Compute)
        {
            return Err(anyhow::anyhow!("Compute Shader missing entry point"));
        }

        Ok(())
    }
}

unsafe impl Send for ComputePipeline {}
unsafe impl Sync for ComputePipeline {}


impl ComputePipeline {
    pub fn new(context: &WgpuContext, shader: ComputeShader) -> Result<ComputePipeline> {
        let mut pipeline = {
            // --- REFLECTION ---
            let naga_res = wgsl::parse_str(&shader.code)
                .map_err(|e| anyhow!("Compute Shader Parse Error for Reflection: {}", e))?;

            let entry_point = naga_res
                .entry_points
                .iter()
                .find(|ep| ep.stage == naga::ShaderStage::Compute)
                .map(|ep| ep.name.clone())
                .ok_or_else(|| anyhow!("Compute Shader missing entry point"))?;

            let mut uniform_members = Vec::new();
            let mut uniform_buffer_size = 0;
            let mut uniform_binding = None;
            let mut texture_bindings = Vec::new();
            let mut sampler_bindings = Vec::new();
            let mut bind_group_entries_layout = Vec::new();

            // Reflection of bindings (Group 0)
            let group0_vars = naga_res
                .global_variables
                .iter()
                .filter(|(_, var)| var.binding.as_ref().map_or(false, |b| b.group == 0));

            for (_, var) in group0_vars {
                let binding = var
                    .binding
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow!(
                            "Variable {} has no binding",
                            var.name.as_deref().unwrap_or("unknown")
                        )
                    })?
                    .binding;
                let name = var.name.clone().unwrap_or_default();

                match var.space {
                    naga::AddressSpace::Uniform => {
                        let ty = &naga_res.types[var.ty];
                        if let naga::TypeInner::Struct { members, span } = &ty.inner {
                            uniform_buffer_size = *span;
                            uniform_binding = Some(binding);

                            for member in members {
                                let member_name = member.name.clone().unwrap_or_default();
                                let size = naga_res.types[member.ty].inner.size(naga_res.to_ctx());
                                uniform_members.push(UniformMember {
                                    name: member_name,
                                    offset: member.offset,
                                    size,
                                });
                            }

                            bind_group_entries_layout.push(wgpu::BindGroupLayoutEntry {
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
                    naga::AddressSpace::Handle => {
                        let ty = &naga_res.types[var.ty];
                        match &ty.inner {
                            naga::TypeInner::Image { dim, class, .. } => {
                                let view_dimension = match dim {
                                    naga::ImageDimension::D1 => wgpu::TextureViewDimension::D1,
                                    naga::ImageDimension::D2 => wgpu::TextureViewDimension::D2,
                                    naga::ImageDimension::D3 => wgpu::TextureViewDimension::D3,
                                    naga::ImageDimension::Cube => wgpu::TextureViewDimension::Cube,
                                };

                                let storage = match class {
                                    naga::ImageClass::Storage { format, access } => {
                                        Some((format, access))
                                    }
                                    _ => None,
                                };

                                let mut wgpu_format = None;

                                if let Some((format, access)) = storage {
                                    // Storage Texture
                                    let access = if access.contains(naga::StorageAccess::STORE) {
                                        wgpu::StorageTextureAccess::WriteOnly // Simplified
                                    } else {
                                        wgpu::StorageTextureAccess::ReadOnly
                                    };

                                    let fmt = match *format {
                                        naga::StorageFormat::R8Unorm => {
                                            wgpu::TextureFormat::R8Unorm
                                        }
                                        naga::StorageFormat::R8Snorm => {
                                            wgpu::TextureFormat::R8Snorm
                                        }
                                        naga::StorageFormat::R8Uint => wgpu::TextureFormat::R8Uint,
                                        naga::StorageFormat::R8Sint => wgpu::TextureFormat::R8Sint,
                                        naga::StorageFormat::R16Uint => {
                                            wgpu::TextureFormat::R16Uint
                                        }
                                        naga::StorageFormat::R16Sint => {
                                            wgpu::TextureFormat::R16Sint
                                        }
                                        naga::StorageFormat::R16Float => {
                                            wgpu::TextureFormat::R16Float
                                        }
                                        naga::StorageFormat::Rg8Unorm => {
                                            wgpu::TextureFormat::Rg8Unorm
                                        }
                                        naga::StorageFormat::Rg8Snorm => {
                                            wgpu::TextureFormat::Rg8Snorm
                                        }
                                        naga::StorageFormat::Rg8Uint => {
                                            wgpu::TextureFormat::Rg8Uint
                                        }
                                        naga::StorageFormat::Rg8Sint => {
                                            wgpu::TextureFormat::Rg8Sint
                                        }
                                        naga::StorageFormat::R32Uint => {
                                            wgpu::TextureFormat::R32Uint
                                        }
                                        naga::StorageFormat::R32Sint => {
                                            wgpu::TextureFormat::R32Sint
                                        }
                                        naga::StorageFormat::R32Float => {
                                            wgpu::TextureFormat::R32Float
                                        }
                                        naga::StorageFormat::Rg16Uint => {
                                            wgpu::TextureFormat::Rg16Uint
                                        }
                                        naga::StorageFormat::Rg16Sint => {
                                            wgpu::TextureFormat::Rg16Sint
                                        }
                                        naga::StorageFormat::Rg16Float => {
                                            wgpu::TextureFormat::Rg16Float
                                        }
                                        naga::StorageFormat::Rgba8Unorm => {
                                            wgpu::TextureFormat::Rgba8Unorm
                                        }
                                        naga::StorageFormat::Rgba8Snorm => {
                                            wgpu::TextureFormat::Rgba8Snorm
                                        }
                                        naga::StorageFormat::Rgba8Uint => {
                                            wgpu::TextureFormat::Rgba8Uint
                                        }
                                        naga::StorageFormat::Rgba8Sint => {
                                            wgpu::TextureFormat::Rgba8Sint
                                        }
                                        naga::StorageFormat::Bgra8Unorm => {
                                            wgpu::TextureFormat::Bgra8Unorm
                                        }
                                        naga::StorageFormat::Rgb10a2Unorm => {
                                            wgpu::TextureFormat::Rgb10a2Unorm
                                        }
                                        naga::StorageFormat::Rg32Uint => {
                                            wgpu::TextureFormat::Rg32Uint
                                        }
                                        naga::StorageFormat::Rg32Sint => {
                                            wgpu::TextureFormat::Rg32Sint
                                        }
                                        naga::StorageFormat::Rg32Float => {
                                            wgpu::TextureFormat::Rg32Float
                                        }
                                        naga::StorageFormat::Rgba16Uint => {
                                            wgpu::TextureFormat::Rgba16Uint
                                        }
                                        naga::StorageFormat::Rgba16Sint => {
                                            wgpu::TextureFormat::Rgba16Sint
                                        }
                                        naga::StorageFormat::Rgba16Float => {
                                            wgpu::TextureFormat::Rgba16Float
                                        }
                                        naga::StorageFormat::Rgba32Uint => {
                                            wgpu::TextureFormat::Rgba32Uint
                                        }
                                        naga::StorageFormat::Rgba32Sint => {
                                            wgpu::TextureFormat::Rgba32Sint
                                        }
                                        naga::StorageFormat::Rgba32Float => {
                                            wgpu::TextureFormat::Rgba32Float
                                        }
                                        _ => wgpu::TextureFormat::Rgba8Unorm,
                                    };
                                    wgpu_format = Some(fmt);

                                    bind_group_entries_layout.push(wgpu::BindGroupLayoutEntry {
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
                                    // Sampled Texture
                                    bind_group_entries_layout.push(wgpu::BindGroupLayoutEntry {
                                        binding: binding,
                                        visibility: wgpu::ShaderStages::COMPUTE,
                                        ty: wgpu::BindingType::Texture {
                                            multisampled: false,
                                            view_dimension,
                                            sample_type: wgpu::TextureSampleType::Float {
                                                filterable: true,
                                            },
                                        },
                                        count: None,
                                    });
                                }

                                texture_bindings.push(crate::data::shader::TextureBinding {
                                    name: name.clone(),
                                    binding,
                                    format: wgpu_format,
                                    dimension: view_dimension,
                                });
                            }
                            naga::TypeInner::Sampler { .. } => {
                                sampler_bindings.push((name.clone(), binding));
                                bind_group_entries_layout.push(wgpu::BindGroupLayoutEntry {
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
                    naga::AddressSpace::Storage { access: _ } => {
                        // Storage Buffers
                        // TODO: Support Storage Buffers
                    }
                    _ => {}
                }
            }

            bind_group_entries_layout.sort_by_key(|e| e.binding);

            let bind_group_layout =
                context
                    .device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("ComputePipeline Bind Layout"),
                        entries: &bind_group_entries_layout,
                    });

            let reflection = Arc::new(ReflectionData {
                uniform_members,
                uniform_buffer_size,
                uniform_binding,
                texture_bindings,
                sampler_bindings,
                fragment_entry_point: String::new(), // Not used for compute
                vertex_entry_point: entry_point.clone(), // Reusing this field for compute entry point
            });

            ComputePipeline {
                shader: shader.clone(),
                bind_group_layout: Some(Arc::new(bind_group_layout)),
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
