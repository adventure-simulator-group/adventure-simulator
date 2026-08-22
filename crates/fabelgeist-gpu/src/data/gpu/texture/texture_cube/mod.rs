use std::sync::Arc;

use anyhow::Result;

use crate::{
    data::gpu::texture::{Image, TextureFormat},
    globals::WgpuContext,
};

#[derive(Clone, Debug)]
pub struct TextureCube {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: u32,
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
}

impl PartialEq for TextureCube {
    fn eq(&self, other: &Self) -> bool {
        (match (&self.texture, &other.texture) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }) && (match (&self.view, &other.view) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }) && self.size == other.size
            && self.format == other.format
            && self.usage == other.usage
    }
}

impl Default for TextureCube {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: 0,
            format: TextureFormat::default(),
            usage: wgpu::TextureUsages::empty(),
        }
    }
}

impl TextureCube {
    pub fn new(context: &WgpuContext, size: f32, format: TextureFormat) -> Result<TextureCube> {
        let _wgpu_format: wgpu::TextureFormat = format.into();

        if size.is_nan() {
            return Err(anyhow::anyhow!("Texture size contains NaN: {:?}", size));
        }

        let size = size as u32;

        if size < 1 {
            return Err(anyhow::anyhow!(
                "Texture size must be at least 1. Got {} (from {:?})",
                size,
                size
            ));
        }

        let limits = context.device.limits();
        let max_dim = limits.max_texture_dimension_2d;
        if size > max_dim {
            return Err(anyhow::anyhow!(
                "Texture size {} exceeds maximum supported dimension {} (from {:?})",
                size,
                max_dim,
                size
            ));
        }

        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC;

        if format.supports_render_attachment() {
            usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }

        if format.supports_storage() && !format.is_srgb() {
            usage |= wgpu::TextureUsages::STORAGE_BINDING;
        }

        let base_format = format.linear_counterpart();
        let wgpu_base_format: wgpu::TextureFormat = base_format.into();

        let mut view_formats = vec![wgpu_base_format];
        let counterpart = base_format.srgb_counterpart().into();
        if counterpart != wgpu_base_format {
            view_formats.push(counterpart);
        }

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("TextureCube"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_base_format,
            usage,
            view_formats: &view_formats,
        };

        #[cfg(not(target_arch = "wasm32"))]
        let error_scope = context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);

        let texture = context.device.create_texture(&texture_desc);

        let mut initial_view_format = format;
        if usage.contains(wgpu::TextureUsages::STORAGE_BINDING) && format.is_srgb() {
            initial_view_format = format.linear_counterpart();
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("TextureCube View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            format: Some(initial_view_format.into()),
            ..Default::default()
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = context.device.poll(wgpu::PollType::wait_indefinitely());
            if let Some(err) = pollster::block_on(error_scope.pop()) {
                return Err(anyhow::anyhow!("WGPU TextureCube Creation Error: {}", err));
            }
        }

        let texture_value = TextureCube {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size,
            format,
            usage,
        };

        Ok(texture_value)
    }

    pub fn create_from_images(
        context: &WgpuContext,
        images: [Image; 6],
        format: TextureFormat,
    ) -> Result<TextureCube> {
        let size = images[0].width;
        for (i, img) in images.iter().enumerate() {
            if img.width != size || img.height != size {
                return Err(anyhow::anyhow!(
                    "Cube map images must be square and identical in size. Face 0: {}x{}, Face {}: {}x{}",
                    size,
                    size,
                    i,
                    img.width,
                    img.height
                ));
            }
        }

        let tex = TextureCube::new(context, size as f32, format)?;
        let pixel_size = format.pixel_size();

        for (i, image) in images.into_iter().enumerate() {
            let raw_data = &image.data;
            let converted_data: Vec<u8> = match format {
                TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm => raw_data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|rgba| {
                        let mut out = [0u8; 4];
                        for (c, channel) in rgba.iter().take(3).enumerate() {
                            let f = *channel as f32 / 255.0;
                            let linear = if f <= 0.04045 {
                                f / 12.92
                            } else {
                                ((f + 0.055) / 1.055).powf(2.4)
                            };
                            out[c] = (linear.clamp(0.0, 1.0) * 255.0) as u8;
                        }
                        out[3] = rgba[3];
                        if matches!(format, TextureFormat::Bgra8Unorm) {
                            out.swap(0, 2);
                        }
                        out
                    })
                    .collect(),
                TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8UnormSrgb => {
                    if matches!(format, TextureFormat::Bgra8UnormSrgb) {
                        raw_data
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .flat_map(|rgba| [rgba[2], rgba[1], rgba[0], rgba[3]])
                            .collect()
                    } else {
                        raw_data.to_vec()
                    }
                }
                TextureFormat::Rgba32Float => {
                    let mut floats = Vec::with_capacity((size * size * 4) as usize);
                    for rgba in raw_data.as_chunks::<4>().0 {
                        for channel in rgba.iter().take(3) {
                            let f = *channel as f32 / 255.0;
                            let linear = if f <= 0.04045 {
                                f / 12.92
                            } else {
                                ((f + 0.055) / 1.055).powf(2.4)
                            };
                            floats.push(linear);
                        }
                        floats.push(rgba[3] as f32 / 255.0);
                    }
                    bytemuck::cast_slice(&floats).to_vec()
                }
                _ => {
                    if pixel_size == 4 {
                        raw_data.to_vec()
                    } else {
                        return Err(anyhow::anyhow!(
                            "Unsupported image conversion to format: {:?}",
                            format
                        ));
                    }
                }
            };

            context.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex.texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &converted_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * pixel_size),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }

        Ok(tex)
    }

    pub fn size(&self) -> f32 {
        self.size as f32
    }

    pub fn view_with_format(
        &self,
        _context: &WgpuContext,
        format: TextureFormat,
    ) -> Result<Arc<wgpu::TextureView>> {
        if format == self.format {
            return Ok(self.view.as_ref().unwrap().clone());
        }
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Texture is not initialized"))?;

        let mut requested_format = format;
        if self.usage.contains(wgpu::TextureUsages::STORAGE_BINDING) && format.is_srgb() {
            requested_format = format.linear_counterpart();
        }

        #[cfg(not(target_arch = "wasm32"))]
        let error_scope = _context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("TextureCube View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            format: Some(requested_format.into()),
            ..Default::default()
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = _context.device.poll(wgpu::PollType::wait_indefinitely());
            if let Some(err) = pollster::block_on(error_scope.pop()) {
                return Err(anyhow::anyhow!(
                    "WGPU TextureCube view_with_format Error (requested {:?}): {}",
                    format,
                    err
                ));
            }
        }

        Ok(Arc::new(view))
    }

    pub fn view_2d_array(&self) -> Option<wgpu::TextureView> {
        self.texture.as_ref().map(|tex| {
            let mut requested_format = self.format;
            if self.usage.contains(wgpu::TextureUsages::STORAGE_BINDING) && self.format.is_srgb() {
                requested_format = self.format.linear_counterpart();
            }
            tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("TextureCube 2D Array View for Storage"),
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                array_layer_count: Some(6),
                format: Some(requested_format.into()),
                ..Default::default()
            })
        })
    }

    pub fn render_shader(
        context: &WgpuContext,
        target: TextureCube,
        shader_src: String,
        time: f32,
    ) -> Result<TextureCube> {
        target.render_shader_raw(context, &shader_src, time)?;
        Ok(target)
    }

    pub fn render_shader_raw(
        &self,
        context: &WgpuContext,
        shader_src: &str,
        time: f32,
    ) -> Result<()> {
        let storage_view = self
            .view_2d_array()
            .ok_or_else(|| anyhow::anyhow!("Failed to create 2D array view"))?;

        let wgsl_code = format!(
            r#"
            {}

            @group(0) @binding(0) var out_cube: texture_storage_2d_array<{}, write>;

            struct Uniforms {{
                time: f32,
                _pad0: f32,
                _pad1: f32,
                _pad2: f32,
            }};
            @group(0) @binding(1) var<uniform> uniforms: Uniforms;

            fn get_cube_direction(uv: vec2<f32>, face: u32) -> vec3<f32> {{
                let u = uv.x * 2.0 - 1.0;
                let v = uv.y * 2.0 - 1.0;
                var dir = vec3<f32>(0.0);
                if (face == 0u) {{ dir = vec3<f32>(1.0, -v, -u); }}
                else if (face == 1u) {{ dir = vec3<f32>(-1.0, -v, u); }}
                else if (face == 2u) {{ dir = vec3<f32>(u, 1.0, v); }}
                else if (face == 3u) {{ dir = vec3<f32>(u, -1.0, -v); }}
                else if (face == 4u) {{ dir = vec3<f32>(u, -v, 1.0); }}
                else {{ dir = vec3<f32>(-u, -v, -1.0); }}
                return normalize(dir);
            }}

            @compute @workgroup_size(8, 8, 1)
            fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
                let size = textureDimensions(out_cube).xy;
                if (id.x >= size.x || id.y >= size.y || id.z >= 6u) {{
                    return;
                }}
                let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
                let dir = get_cube_direction(uv, id.z);
                let color = cube(dir);
                textureStore(out_cube, id.xy, id.z, color);
            }}
            "#,
            shader_src,
            self.format.to_wgsl_storage_format()
        );

        let shader_module = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("TextureCube Shader Renderer"),
                source: wgpu::ShaderSource::Wgsl(wgsl_code.into()),
            });

        use wgpu::util::DeviceExt;
        let uniforms_data = [time, 0.0, 0.0, 0.0];
        let uniform_buffer = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("TextureCube Renderer Uniforms"),
                contents: bytemuck::cast_slice(&uniforms_data),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("TextureCube Renderer Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: self.format.linear_counterpart().into(),
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("TextureCube Renderer Bind Group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&storage_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                ],
            });

        let pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("TextureCube Renderer Pipeline Layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        let pipeline = context
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("TextureCube Renderer Compute Pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("TextureCube Renderer Command Encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("TextureCube Renderer Compute Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroup_count = self.size.div_ceil(8);
            pass.dispatch_workgroups(workgroup_count, workgroup_count, 6);
        }

        context.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

unsafe impl Send for TextureCube {}
unsafe impl Sync for TextureCube {}
