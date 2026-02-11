mod fragment_shader;
mod vertex_shader;

pub use fragment_shader::*;
pub use vertex_shader::*;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::data::{ReflectionData, TextureBinding, UniformMember};
use crate::globals::WgpuContext;
use anyhow::anyhow;
use gpu_runtime_base::Result;
use naga::front::wgsl;

#[derive(Clone, Debug)]
pub struct RenderPipeline {
    pub vertex: VertexShader,
    pub fragment: FragmentShader,
    pub bind_group_layout: Option<Arc<wgpu::BindGroupLayout>>,
    pub reflection: Option<Arc<ReflectionData>>,
    pub pipeline_cache: Arc<Mutex<HashMap<u64, Arc<wgpu::RenderPipeline>>>>,
    pub pipeline: Option<Arc<wgpu::RenderPipeline>>,
    pub validation_error: Arc<Mutex<Option<String>>>,
    pub baked_color_formats: Vec<wgpu::TextureFormat>,
    pub baked_depth_format: Option<wgpu::TextureFormat>,
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self {
            vertex: VertexShader::default(),
            fragment: FragmentShader::default(),
            bind_group_layout: None,
            reflection: None,
            pipeline_cache: Arc::new(Mutex::new(HashMap::new())),
            pipeline: None,
            validation_error: Arc::new(Mutex::new(None)),
            baked_color_formats: Vec::new(),
            baked_depth_format: None,
        }
    }
}

impl RenderPipeline {
    pub fn get_or_create_pipeline(
        &self,
        device: &wgpu::Device,
        color_formats: &[wgpu::TextureFormat],
        depth_format: Option<wgpu::TextureFormat>,
        fragment_entry_point: &str,
        vertex_entry_point: &str,
    ) -> anyhow::Result<Arc<wgpu::RenderPipeline>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut s = DefaultHasher::new();
        color_formats.hash(&mut s);
        depth_format.hash(&mut s);
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

        let vs_module = self
            .vertex
            .module
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("RenderPipeline: Vertex Shader Module missing"))?;
        let fs_module = self
            .fragment
            .module
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("RenderPipeline: Fragment Shader Module missing"))?;
        let bind_group_layout = self
            .bind_group_layout
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("RenderPipeline: BindGroupLayout missing"))?;

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RenderPipeline Layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        let mut targets = Vec::new();
        for format in color_formats {
            targets.push(Some(wgpu::ColorTargetState {
                format: *format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            }));
        }

        let depth_stencil_state = depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RenderPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: vs_module,
                entry_point: Some(vertex_entry_point),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: fs_module,
                entry_point: Some(fragment_entry_point),
                targets: &targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: depth_stencil_state,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
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

        let naga_vs = wgsl::parse_str(&self.vertex.code)
            .map_err(|e| anyhow::anyhow!("Vertex Shader Parse Error: {}", e))?;
        let naga_fs = wgsl::parse_str(&self.fragment.code)
            .map_err(|e| anyhow::anyhow!("Fragment Shader Parse Error: {}", e))?;

        if !naga_vs
            .entry_points
            .iter()
            .any(|ep| ep.stage == naga::ShaderStage::Vertex)
        {
            return Err(anyhow::anyhow!("Vertex Shader missing entry point"));
        }
        if !naga_fs
            .entry_points
            .iter()
            .any(|ep| ep.stage == naga::ShaderStage::Fragment)
        {
            return Err(anyhow::anyhow!("Fragment Shader missing entry point"));
        }

        // Potential for more deep validation here (IO match, etc)

        Ok(())
    }
}

unsafe impl Send for RenderPipeline {}
unsafe impl Sync for RenderPipeline {}


impl RenderPipeline {
    pub fn new(
        context: &WgpuContext,
        vertex: VertexShader,
        fragment: FragmentShader,
    ) -> Result<RenderPipeline> {
        let mut pipeline = {
            // --- REFLECTION ---
            let naga_vs = wgsl::parse_str(&vertex.code)
                .map_err(|e| anyhow!("Vertex Shader Parse Error for Reflection: {}", e))?;
            let naga_fs = wgsl::parse_str(&fragment.code)
                .map_err(|e| anyhow!("Fragment Shader Parse Error for Reflection: {}", e))?;

            let vertex_entry_point = naga_vs
                .entry_points
                .iter()
                .find(|ep| ep.stage == naga::ShaderStage::Vertex)
                .map(|ep| ep.name.clone())
                .ok_or_else(|| anyhow!("Vertex Shader missing entry point"))?;

            let fragment_entry_point = naga_fs
                .entry_points
                .iter()
                .find(|ep| ep.stage == naga::ShaderStage::Fragment)
                .map(|ep| ep.name.clone())
                .ok_or_else(|| anyhow!("Fragment Shader missing entry point"))?;

            let mut uniform_members = Vec::new();
            let mut uniform_buffer_size = 0;
            let mut uniform_binding = None;
            let mut texture_bindings: Vec<TextureBinding> = Vec::new();
            let mut sampler_bindings = Vec::new();
            let mut bind_group_entries_layout = Vec::new();

            // We use the fragment shader for reflection of bindings (Group 0)
            let group0_vars = naga_fs
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
                        let ty = &naga_fs.types[var.ty];
                        if let naga::TypeInner::Struct { members, span } = &ty.inner {
                            uniform_buffer_size = *span;
                            uniform_binding = Some(binding);

                            for member in members {
                                let member_name = member.name.clone().unwrap_or_default();
                                let size = naga_fs.types[member.ty].inner.size(naga_fs.to_ctx());
                                uniform_members.push(UniformMember {
                                    name: member_name,
                                    offset: member.offset,
                                    size,
                                });
                            }

                            bind_group_entries_layout.push(wgpu::BindGroupLayoutEntry {
                                binding: binding,
                                visibility: wgpu::ShaderStages::FRAGMENT,
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
                        let ty = &naga_fs.types[var.ty];
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
                                    // Storage Texture (Fragment Shader)
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
                                        visibility: wgpu::ShaderStages::FRAGMENT,
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
                                        visibility: wgpu::ShaderStages::FRAGMENT,
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
                                    visibility: wgpu::ShaderStages::FRAGMENT,
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

            bind_group_entries_layout.sort_by_key(|e| e.binding);

            let bind_group_layout =
                context
                    .device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("RenderPipeline Bind Layout"),
                        entries: &bind_group_entries_layout,
                    });

            let reflection = Arc::new(ReflectionData {
                uniform_members,
                uniform_buffer_size,
                uniform_binding,
                texture_bindings,
                sampler_bindings,
                fragment_entry_point,
                vertex_entry_point,
            });

            RenderPipeline {
                vertex: vertex.clone(),
                fragment: fragment.clone(),
                bind_group_layout: Some(Arc::new(bind_group_layout)),
                reflection: Some(reflection),
                ..Default::default()
            }
        };

        // Ensure vertex and fragment are up to date (in case modules were added late)
        pipeline.vertex = vertex.clone();
        pipeline.fragment = fragment.clone();

        // Bake WGPU Resource
        let color_formats: Vec<wgpu::TextureFormat> = vec![wgpu::TextureFormat::Rgba8Unorm];
        let depth_format = None;

        // 1. Synchronous Naga Validation
        pipeline.validate_interface()?;

        // 2. WGPU Bake
        let reflection = pipeline.reflection.as_ref().unwrap();
        if let Ok(p_wgpu) = pipeline.get_or_create_pipeline(
            &context.device,
            &color_formats,
            depth_format,
            &reflection.fragment_entry_point,
            &reflection.vertex_entry_point,
        ) {
            pipeline.pipeline = Some(p_wgpu);
            pipeline.baked_color_formats = color_formats;
            pipeline.baked_depth_format = depth_format;
        }

        // Check for validation errors (Link errors)
        if let Ok(guard) = pipeline.validation_error.lock() {
            if let Some(err) = guard.as_ref() {
                return Err(anyhow!("RenderPipeline Creation Error: {}", err));
            }
        }

        Ok(pipeline)
    }
}
