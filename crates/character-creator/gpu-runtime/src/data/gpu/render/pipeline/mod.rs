mod fragment_shader;
pub mod topology;
pub mod cull_mode;
pub mod front_face;
mod vertex_shader;

pub use fragment_shader::*;
pub use topology::*;
pub use cull_mode::*;
pub use front_face::*;
pub use vertex_shader::*;
pub mod vertex;

pub use vertex::*;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::data::TextureFormat;
use crate::data::gpu::shader::parse_naga;
use crate::data::shader::ReflectionData;
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};

#[derive(Clone, Debug)]
pub struct RenderPipeline {
    pub vertex: VertexShader,
    pub fragment: FragmentShader,
    pub bind_group_layouts: Vec<Arc<wgpu::BindGroupLayout>>,
    pub reflection: Option<Arc<ReflectionData>>,
    pub pipeline_cache: Arc<Mutex<HashMap<u64, Arc<wgpu::RenderPipeline>>>>,
    pub pipeline: Option<Arc<wgpu::RenderPipeline>>,
    pub validation_error: Arc<Mutex<Option<String>>>,
    pub baked_color_formats: Vec<wgpu::TextureFormat>,
    pub baked_depth_format: Option<wgpu::TextureFormat>,
    pub topology: PrimitiveTopology,
    pub cull_mode: CullMode,
    pub front_face: FrontFace,
    pub vertex_layouts: Vec<VertexBufferLayout>,
}

impl Default for RenderPipeline {
    fn default() -> Self {
        Self {
            vertex: VertexShader::default(),
            fragment: FragmentShader::default(),
            bind_group_layouts: Vec::new(),
            reflection: None,
            pipeline_cache: Arc::new(Mutex::new(HashMap::new())),
            pipeline: None,
            validation_error: Arc::new(Mutex::new(None)),
            baked_color_formats: Vec::new(),
            baked_depth_format: None,
            topology: PrimitiveTopology::TriangleStrip,
            cull_mode: CullMode::None,
            front_face: FrontFace::Ccw,
            vertex_layouts: Vec::new(),
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
    ) -> Result<Arc<wgpu::RenderPipeline>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut s = DefaultHasher::new();
        color_formats.hash(&mut s);
        depth_format.hash(&mut s);
        self.topology.hash(&mut s);
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

        let layouts: Vec<_> = self.bind_group_layouts.iter().map(|l| l.as_ref()).collect();

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RenderPipeline Layout"),
            bind_group_layouts: &layouts,
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

        let mut wgpu_vertex_attributes = Vec::new();
        for layout in &self.vertex_layouts {
            let mut attrs = Vec::new();
            for attr in &layout.attributes {
                attrs.push(wgpu::VertexAttribute {
                    format: attr.format.into(),
                    offset: attr.offset,
                    shader_location: attr.shader_location,
                });
            }
            wgpu_vertex_attributes.push(attrs);
        }

        let wgpu_vertex_layouts: Vec<wgpu::VertexBufferLayout> = self
            .vertex_layouts
            .iter()
            .enumerate()
            .map(|(i, layout)| wgpu::VertexBufferLayout {
                array_stride: layout.array_stride,
                step_mode: layout.step_mode.into(),
                attributes: &wgpu_vertex_attributes[i],
            })
            .collect();

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RenderPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: vs_module,
                entry_point: Some(vertex_entry_point),
                buffers: &wgpu_vertex_layouts,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: fs_module,
                entry_point: Some(fragment_entry_point),
                targets: &targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: self.topology.into(),
                cull_mode: self.cull_mode.into(),
                front_face: self.front_face.into(),
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

    pub fn validate_interface(&self) -> Result<()> {
        let naga_vs = parse_naga(&self.vertex.code, wgpu::naga::ShaderStage::Vertex)
            .map_err(|e| anyhow::anyhow!("Vertex Shader Parse Error: {}", e))?;
        let naga_fs = parse_naga(&self.fragment.code, wgpu::naga::ShaderStage::Fragment)
            .map_err(|e| anyhow::anyhow!("Fragment Shader Parse Error: {}", e))?;

        if !naga_vs
            .entry_points
            .iter()
            .any(|ep| ep.stage == wgpu::naga::ShaderStage::Vertex)
        {
            return Err(anyhow::anyhow!("Vertex Shader missing entry point"));
        }
        if !naga_fs
            .entry_points
            .iter()
            .any(|ep| ep.stage == wgpu::naga::ShaderStage::Fragment)
        {
            return Err(anyhow::anyhow!("Fragment Shader missing entry point"));
        }

        // Potential for more deep validation here (IO match, etc)

        Ok(())
    }
}

impl RenderPipeline {
    pub fn new(
        context: &WgpuContext,
        vertex: VertexShader,
        fragment: FragmentShader,
        topology: PrimitiveTopology,
        cull_mode: CullMode,
        front_face: FrontFace,
        vertex_layouts: Vec<VertexBufferLayout>,
    ) -> Result<RenderPipeline> {
        let mut pipeline = {
            // --- REFLECTION ---
            let naga_vs = parse_naga(&vertex.code, wgpu::naga::ShaderStage::Vertex)
                .map_err(|e| anyhow!("Vertex Shader Parse Error for Reflection: {}", e))?;
            let naga_fs = parse_naga(&fragment.code, wgpu::naga::ShaderStage::Fragment)
                .map_err(|e| anyhow!("Fragment Shader Parse Error for Reflection: {}", e))?;

            let vertex_entry_point = naga_vs
                .entry_points
                .iter()
                .find(|ep| ep.stage == wgpu::naga::ShaderStage::Vertex)
                .map(|ep| ep.name.clone())
                .ok_or_else(|| anyhow!("Vertex Shader missing entry point"))?;

            let fragment_entry_point = naga_fs
                .entry_points
                .iter()
                .find(|ep| ep.stage == wgpu::naga::ShaderStage::Fragment)
                .map(|ep| ep.name.clone())
                .ok_or_else(|| anyhow!("Fragment Shader missing entry point"))?;

            let mut bind_groups_map: std::collections::BTreeMap<
                u32,
                crate::data::gpu::shader::BindGroupReflection,
            > = std::collections::BTreeMap::new();
            let mut bind_group_layouts_data: std::collections::BTreeMap<
                u32,
                Vec<wgpu::BindGroupLayoutEntry>,
            > = std::collections::BTreeMap::new();

            // Reflection of bindings
            let shaders = [
                (&naga_vs, wgpu::ShaderStages::VERTEX),
                (&naga_fs, wgpu::ShaderStages::FRAGMENT),
            ];

            for (naga, stage_visibility) in shaders {
                for (_, var) in naga.global_variables.iter() {
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
                                let ty = &naga.types[var.ty];
                                if let wgpu::naga::TypeInner::Struct { members, span } = &ty.inner {
                                    group_reflection.uniform_buffer_size = *span;
                                    group_reflection.uniform_binding = Some(binding);

                                    for member in members {
                                        let member_name = member.name.clone().unwrap_or_default();
                                        if !group_reflection
                                            .uniform_members
                                            .iter()
                                            .any(|m| m.name == member_name)
                                        {
                                            let size =
                                                naga.types[member.ty].inner.size(naga.to_ctx());
                                            group_reflection.uniform_members.push(
                                                crate::data::shader::UniformMember {
                                                    name: member_name,
                                                    offset: member.offset,
                                                    size,
                                                },
                                            );
                                        }
                                    }

                                    if let Some(entry) = layout_entries
                                        .iter_mut()
                                        .find(|e| e.binding == binding)
                                    {
                                        entry.visibility |= stage_visibility;
                                    } else {
                                        layout_entries.push(wgpu::BindGroupLayoutEntry {
                                            binding,
                                            visibility: stage_visibility,
                                            ty: wgpu::BindingType::Buffer {
                                                ty: wgpu::BufferBindingType::Uniform,
                                                has_dynamic_offset: false,
                                                min_binding_size: None,
                                            },
                                            count: None,
                                        });
                                    }
                                }
                            }
                            wgpu::naga::AddressSpace::Storage { access } => {
                                let read_only = !access.contains(wgpu::naga::StorageAccess::STORE);
                                if !group_reflection
                                    .buffer_bindings
                                    .iter()
                                    .any(|b| b.binding == binding)
                                {
                                    group_reflection.buffer_bindings.push(
                                        crate::data::shader::BufferBinding {
                                            name: name.clone(),
                                            binding,
                                            ty: wgpu::BufferBindingType::Storage { read_only },
                                        },
                                    );
                                }

                                if let Some(entry) = layout_entries
                                    .iter_mut()
                                    .find(|e| e.binding == binding)
                                {
                                    entry.visibility |= stage_visibility;
                                } else {
                                    layout_entries.push(wgpu::BindGroupLayoutEntry {
                                        binding,
                                        visibility: stage_visibility,
                                        ty: wgpu::BindingType::Buffer {
                                            ty: wgpu::BufferBindingType::Storage { read_only },
                                            has_dynamic_offset: false,
                                            min_binding_size: None,
                                        },
                                        count: None,
                                    });
                                }
                            }
                            wgpu::naga::AddressSpace::Handle => {
                                let ty = &naga.types[var.ty];
                                match &ty.inner {
                                    wgpu::naga::TypeInner::Image { dim, class, .. } => {
                                        let view_dimension = match dim {
                                            wgpu::naga::ImageDimension::D1 => {
                                                wgpu::TextureViewDimension::D2
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

                                        let mut wgpu_format: Option<wgpu::TextureFormat> = None;
                                        if let Some((format, access)) = storage {
                                            let access = if access
                                                .contains(wgpu::naga::StorageAccess::STORE)
                                            {
                                                wgpu::StorageTextureAccess::WriteOnly
                                            } else {
                                                wgpu::StorageTextureAccess::ReadOnly
                                            };

                                            let fmt = TextureFormat::naga_to_wgpu_format(*format);
                                            wgpu_format = Some(fmt);

                                            if let Some(entry) = layout_entries
                                                .iter_mut()
                                                .find(|e| e.binding == binding)
                                            {
                                                entry.visibility |= stage_visibility;
                                            } else {
                                                layout_entries.push(wgpu::BindGroupLayoutEntry {
                                                    binding,
                                                    visibility: stage_visibility,
                                                    ty: wgpu::BindingType::StorageTexture {
                                                        access,
                                                        format: fmt,
                                                        view_dimension,
                                                    },
                                                    count: None,
                                                });
                                            }
                                        } else {
                                            let sample_type = match class {
                                                wgpu::naga::ImageClass::Sampled {
                                                    kind, ..
                                                } => match kind {
                                                    wgpu::naga::ScalarKind::Float => {
                                                        let filterable =
                                                            TextureFormat::is_filterable(
                                                                wgpu::TextureFormat::R32Float,
                                                                context.device.features(),
                                                            );
                                                        wgpu::TextureSampleType::Float {
                                                            filterable,
                                                        }
                                                    }
                                                    wgpu::naga::ScalarKind::Uint => {
                                                        wgpu::TextureSampleType::Uint
                                                    }
                                                    wgpu::naga::ScalarKind::Sint => {
                                                        wgpu::TextureSampleType::Sint
                                                    }
                                                    _ => {
                                                        let filterable =
                                                            TextureFormat::is_filterable(
                                                                wgpu::TextureFormat::R32Float,
                                                                context.device.features(),
                                                            );
                                                        wgpu::TextureSampleType::Float {
                                                            filterable,
                                                        }
                                                    }
                                                },
                                                wgpu::naga::ImageClass::Depth { .. } => {
                                                    wgpu::TextureSampleType::Depth
                                                }
                                                _ => wgpu::TextureSampleType::Float {
                                                    filterable: true,
                                                },
                                            };

                                            if let Some(entry) = layout_entries
                                                .iter_mut()
                                                .find(|e| e.binding == binding)
                                            {
                                                entry.visibility |= stage_visibility;
                                            } else {
                                                layout_entries.push(wgpu::BindGroupLayoutEntry {
                                                    binding,
                                                    visibility: stage_visibility,
                                                    ty: wgpu::BindingType::Texture {
                                                        multisampled: false,
                                                        view_dimension,
                                                        sample_type,
                                                    },
                                                    count: None,
                                                });
                                            }
                                        }

                                        if !group_reflection
                                            .texture_bindings
                                            .iter()
                                            .any(|t| t.binding == binding)
                                        {
                                            group_reflection.texture_bindings.push(
                                                crate::data::shader::TextureBinding {
                                                    name: name.clone(),
                                                    binding,
                                                    format: wgpu_format,
                                                    dimension: view_dimension,
                                                },
                                            );
                                        }
                                    }
                                    wgpu::naga::TypeInner::Sampler { .. } => {
                                        if !group_reflection
                                            .sampler_bindings
                                            .iter()
                                            .any(|(_, b)| *b == binding)
                                        {
                                            group_reflection
                                                .sampler_bindings
                                                .push((name.clone(), binding));
                                        }

                                        if let Some(entry) = layout_entries
                                            .iter_mut()
                                            .find(|e| e.binding == binding)
                                        {
                                            entry.visibility |= stage_visibility;
                                        } else {
                                            let sampler_ty = if context
                                                .device
                                                .features()
                                                .contains(wgpu::Features::FLOAT32_FILTERABLE)
                                            {
                                                wgpu::SamplerBindingType::Filtering
                                            } else {
                                                wgpu::SamplerBindingType::NonFiltering
                                            };
                                            layout_entries.push(wgpu::BindGroupLayoutEntry {
                                                binding,
                                                visibility: stage_visibility,
                                                ty: wgpu::BindingType::Sampler(sampler_ty),
                                                count: None,
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
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
                            label: Some(&format!("RenderPipeline Bind Layout Group {}", group)),
                            entries: &entries,
                        });
                bind_group_layouts.push(Arc::new(layout));

                if let Some(reflection) = bind_groups_map.remove(&group) {
                    bind_groups_reflection.push(reflection);
                }
            }

            let reflection = Arc::new(ReflectionData {
                bind_groups: bind_groups_reflection,
                fragment_entry_point,
                vertex_entry_point,
            });

            RenderPipeline {
                vertex: vertex.clone(),
                bind_group_layouts,
                reflection: Some(reflection),
                topology,
                cull_mode,
                front_face,
                vertex_layouts: vertex_layouts.clone(),
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
