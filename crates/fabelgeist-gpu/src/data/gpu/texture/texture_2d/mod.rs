mod image;
pub use image::*;

use std::sync::Arc;

use anyhow::Result;

use crate::{data::gpu::texture::TextureFormat, data::vector::Vec2, globals::WgpuContext};

#[derive(Clone, Debug)]
pub struct Texture2d {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: (u32, u32),
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
}

impl PartialEq for Texture2d {
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

impl Default for Texture2d {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: (0, 0),
            format: TextureFormat::default(),
            usage: wgpu::TextureUsages::empty(),
        }
    }
}

impl Texture2d {
    pub fn new(
        context: &WgpuContext,
        size: Option<Vec2>,
        format: Option<TextureFormat>,
    ) -> Result<Texture2d> {
        Self::create(
            context,
            size.unwrap_or_else(|| Vec2::new(256.0, 256.0)),
            format.unwrap_or_default(),
        )
    }

    pub fn create(context: &WgpuContext, size: Vec2, format: TextureFormat) -> Result<Texture2d> {
        let _wgpu_format: wgpu::TextureFormat = format.into();

        let limits = context.device.limits();
        let max_dim = limits.max_texture_dimension_2d;

        if size.x.is_nan() || size.y.is_nan() {
            return Err(anyhow::anyhow!("Texture size contains NaN: {:?}", size));
        }

        let width = size.x as u32;
        let height = size.y as u32;

        if width < 1 || height < 1 {
            return Err(anyhow::anyhow!(
                "Texture size must be at least 1x1. Got {}x{} (from {:?})",
                width,
                height,
                size
            ));
        }

        if width > max_dim || height > max_dim {
            return Err(anyhow::anyhow!(
                "Texture size {}x{} exceeds maximum supported dimension {} (from {:?})",
                width,
                height,
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
            label: Some("Texture2d"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
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
        // WebGPU Limitation: If a texture has STORAGE_BINDING, it cannot have an sRGB view.
        if usage.contains(wgpu::TextureUsages::STORAGE_BINDING) && format.is_srgb() {
            // Fallback to linear counterpart to avoid validation error during creation.
            initial_view_format = format.linear_counterpart();
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(initial_view_format.into()),
            ..Default::default()
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = context.device.poll(wgpu::PollType::wait_indefinitely());
            if let Some(err) = pollster::block_on(error_scope.pop()) {
                return Err(anyhow::anyhow!("WGPU Texture2d Creation Error: {}", err));
            }
        }

        let texture_value = Texture2d {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size: (width, height),
            format,
            usage,
        };

        Ok(texture_value)
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.size.0 as f32, self.size.1 as f32)
    }

    pub fn from_image(
        context: &WgpuContext,
        image: Image,
        format: Option<TextureFormat>,
    ) -> Result<Texture2d> {
        Self::create_from_image(
            context,
            image,
            format.unwrap_or(TextureFormat::Rgba8UnormSrgb),
        )
    }

    pub fn create_from_image(
        context: &WgpuContext,
        image: Image,
        format: TextureFormat,
    ) -> Result<Texture2d> {
        if image.width == 0 || image.height == 0 {
            return Ok(Texture2d::default());
        }

        let width = image.width;
        let height = image.height;

        let tex = Texture2d::create(context, Vec2::new(width as f32, height as f32), format)?;

        let raw_data = &image.data;
        let pixel_size = format.pixel_size();

        // Convert the source RGBA8 data to the target format
        let converted_data: Vec<u8> = match format {
            TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm => {
                // Manual sRGB to Linear conversion for 8-bit linear formats
                raw_data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|rgba| {
                        let mut out = [0u8; 4];
                        for i in 0..3 {
                            let f = rgba[i] as f32 / 255.0;
                            let linear = if f <= 0.04045 {
                                f / 12.92
                            } else {
                                ((f + 0.055) / 1.055).powf(2.4)
                            };
                            out[i] = (linear.clamp(0.0, 1.0) * 255.0) as u8;
                        }
                        out[3] = rgba[3]; // Keep alpha as is

                        if matches!(format, TextureFormat::Bgra8Unorm) {
                            out.swap(0, 2);
                        }
                        out
                    })
                    .collect()
            }
            TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8UnormSrgb => {
                // Keep raw bits for sRGB formats (hardware will convert on sample)
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
                let mut floats = Vec::with_capacity((width * height * 4) as usize);
                for rgba in raw_data.as_chunks::<4>().0 {
                    for channel in rgba.iter().take(3) {
                        let f = *channel as f32 / 255.0;
                        // Convert to linear for float formats
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
                // Fallback for other formats: copy if size matches, or error
                if raw_data.len() == (width * height * pixel_size) as usize || pixel_size == 4 {
                    raw_data.to_vec()
                } else {
                    return Err(anyhow::anyhow!(
                        "Unsupported image conversion to format: {:?}, expected raw data len {}, got {}",
                        format,
                        width * height * pixel_size,
                        raw_data.len()
                    ));
                }
            }
        };

        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex.texture.as_ref().unwrap(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &converted_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * pixel_size),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        Ok(tex)
    }

    pub fn from_color(
        context: &WgpuContext,
        size: Option<Vec2>,
        color: Option<crate::data::vector::Vec4>,
        format: Option<TextureFormat>,
    ) -> Result<Texture2d> {
        Self::create_from_color(
            context,
            size.unwrap_or_else(|| Vec2::new(256.0, 256.0)),
            color.unwrap_or_else(|| crate::data::vector::Vec4::new(0.0, 0.0, 0.0, 1.0)),
            format.unwrap_or(TextureFormat::Rgba8UnormSrgb),
        )
    }

    pub fn create_from_color(
        context: &WgpuContext,
        size: Vec2,
        color: crate::data::vector::Vec4,
        format: TextureFormat,
    ) -> Result<Texture2d> {
        let tex = Texture2d::create(context, size, format)?;
        tex.clear_raw(context, color)?;
        Ok(tex)
    }

    pub async fn read<T: bytemuck::AnyBitPattern>(&self, context: &WgpuContext) -> Result<Vec<T>> {
        let (width, height) = self.size;
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Texture is not initialized"))?;
        let pixel_size = self.format.pixel_size();
        let bytes_per_row = width * pixel_size;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = (bytes_per_row + align - 1) & !(align - 1);
        let staging_buffer_size = (padded_bytes_per_row * height) as u64;

        let staging_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Texture2d Staging Buffer"),
            size: staging_buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture2d Read Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        context.queue.submit(Some(encoder.finish()));

        #[allow(unused_mut)]
        let (tx, mut rx) = futures_channel::oneshot::channel();
        {
            let slice = staging_buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        }

        // 2. Poll if on native
        #[cfg(not(target_arch = "wasm32"))]
        {
            loop {
                match rx.try_recv() {
                    Ok(Some(res)) => {
                        res.map_err(|e| anyhow::anyhow!("GPU Mapping error: {:?}", e))?;
                        break;
                    }
                    Ok(None) => {
                        let _ = context.device.poll(wgpu::PollType::Poll);
                        fabelgeist_timer::sleep(std::time::Duration::from_millis(1)).await;
                    }
                    Err(_) => return Err(anyhow::anyhow!("Mapping channel closed")),
                }
            }
        }

        // 3. Await result (only on wasm, since native loop already consumed rx)
        #[cfg(target_arch = "wasm32")]
        rx.await
            .map_err(|_| anyhow::anyhow!("Mapping channel closed"))?
            .map_err(|_| anyhow::anyhow!("GPU Mapping error"))?;

        let slice = staging_buffer.slice(..);
        let data = slice.get_mapped_range()?;

        let mut result = Vec::with_capacity((width * height) as usize * pixel_size as usize);
        if padded_bytes_per_row == bytes_per_row {
            result.extend_from_slice(&data);
        } else {
            for row in 0..height {
                let start = (row * padded_bytes_per_row) as usize;
                let end = start + bytes_per_row as usize;
                result.extend_from_slice(&data[start..end]);
            }
        }

        drop(data);
        staging_buffer.unmap();

        Ok(bytemuck::cast_slice::<u8, T>(&result).to_vec())
    }

    pub async fn read_to_rgba8(&self, context: &WgpuContext) -> Result<Vec<u8>> {
        // For now, we assume it's already in a readable format or we should convert it.
        // If it's Rgba8UnormSrgb, read() works directly.
        self.read::<u8>(context).await
    }

    pub async fn to_image(context: WgpuContext, self_tex: Texture2d) -> Result<Image> {
        self_tex.read_image(&context).await
    }

    pub async fn read_image(&self, context: &WgpuContext) -> Result<Image> {
        let (width, height) = self.size;
        if width == 0 || height == 0 {
            return Ok(Image::default());
        }

        let raw_data = self.read::<u8>(context).await?;

        // Helper function for linear to sRGB conversion
        let linear_to_srgb = |f: f32| -> u8 {
            let srgb = if f <= 0.0031308 {
                f * 12.92
            } else {
                1.055 * f.powf(1.0 / 2.4) - 0.055
            };
            (srgb.clamp(0.0, 1.0) * 255.0) as u8
        };

        let rgba_data = match self.format {
            TextureFormat::Rgba8UnormSrgb => raw_data,
            TextureFormat::Bgra8UnormSrgb => raw_data
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], bgra[3]])
                .collect(),
            TextureFormat::Rgba8Unorm => {
                raw_data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|rgba| {
                        [
                            linear_to_srgb(rgba[0] as f32 / 255.0),
                            linear_to_srgb(rgba[1] as f32 / 255.0),
                            linear_to_srgb(rgba[2] as f32 / 255.0),
                            rgba[3], // Keep alpha as is
                        ]
                    })
                    .collect()
            }
            TextureFormat::Bgra8Unorm => raw_data
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|bgra| {
                    [
                        linear_to_srgb(bgra[2] as f32 / 255.0),
                        linear_to_srgb(bgra[1] as f32 / 255.0),
                        linear_to_srgb(bgra[0] as f32 / 255.0),
                        bgra[3],
                    ]
                })
                .collect(),
            TextureFormat::Rgba32Float => {
                let floats = bytemuck::cast_slice::<u8, f32>(&raw_data);
                floats
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|rgba| {
                        [
                            linear_to_srgb(rgba[0]),
                            linear_to_srgb(rgba[1]),
                            linear_to_srgb(rgba[2]),
                            (rgba[3].clamp(0.0, 1.0) * 255.0) as u8,
                        ]
                    })
                    .collect()
            }
            TextureFormat::R32Float => {
                let floats = bytemuck::cast_slice::<u8, f32>(&raw_data);
                floats
                    .iter()
                    .flat_map(|&f| {
                        let gray = linear_to_srgb(f);
                        [gray, gray, gray, 255]
                    })
                    .collect()
            }
            TextureFormat::R8Unorm => raw_data
                .iter()
                .flat_map(|&r| {
                    let gray = linear_to_srgb(r as f32 / 255.0);
                    [gray, gray, gray, 255]
                })
                .collect(),
            _ => {
                let pixel_size = self.format.pixel_size();
                if pixel_size == 4 {
                    raw_data
                } else {
                    return Err(anyhow::anyhow!(
                        "Unsupported texture conversion to image for format: {:?}",
                        self.format
                    ));
                }
            }
        };

        Image::from_pixels(rgba_data, width, height)
    }

    pub fn write<T: bytemuck::NoUninit>(&self, context: &WgpuContext, data: &[T]) -> Result<()> {
        let (width, height) = self.size;
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Texture is not initialized"))?;
        let bytes = bytemuck::cast_slice(data);
        let pixel_size = self.format.pixel_size();

        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * pixel_size),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        Ok(())
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

        // WebGPU Limitation: If a texture has STORAGE_BINDING, it cannot have an sRGB view.
        if self.usage.contains(wgpu::TextureUsages::STORAGE_BINDING) && format.is_srgb() {
            // Fallback to linear counterpart to avoid validation error.
            requested_format = format.linear_counterpart();
            // Optional: log or return a warning if we had a way to do so without noise.
        }

        #[cfg(not(target_arch = "wasm32"))]
        let error_scope = _context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(requested_format.into()),
            ..Default::default()
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = _context.device.poll(wgpu::PollType::wait_indefinitely());
            if let Some(err) = pollster::block_on(error_scope.pop()) {
                return Err(anyhow::anyhow!(
                    "WGPU Texture2d view_with_format Error (requested {:?}): {}",
                    format,
                    err
                ));
            }
        }

        Ok(Arc::new(view))
    }

    pub fn add(
        context: &WgpuContext,
        self_tex: Texture2d,
        other: Texture2d,
        amount: Option<f64>,
    ) -> Result<Texture2d> {
        self_tex.add_raw(context, &other, amount.unwrap_or(1.0) as f32)
    }

    pub fn add_raw(
        &self,
        context: &WgpuContext,
        other: &Texture2d,
        amount: f32,
    ) -> Result<Texture2d> {
        use crate::data::gpu::compute::texture_ops::{TextureBinaryOp, TextureBinaryOpDefinition};
        use crate::data::gpu::resource::GpuResource;

        let def = TextureBinaryOpDefinition::new("a + b * amount");
        let output = Texture2d::create(context, self.size(), self.format)?;

        TextureBinaryOp::execute(
            context,
            &def,
            &GpuResource::Texture2d(self.clone()),
            &GpuResource::Texture2d(other.clone()),
            amount,
            &GpuResource::Texture2d(output.clone()),
        )?;

        Ok(output)
    }

    pub fn mix(
        context: &WgpuContext,
        self_tex: Texture2d,
        other: Texture2d,
        amount: Option<f64>,
    ) -> Result<Texture2d> {
        self_tex.mix_raw(context, &other, amount.unwrap_or(0.5) as f32)
    }

    pub fn mix_raw(
        &self,
        context: &WgpuContext,
        other: &Texture2d,
        amount: f32,
    ) -> Result<Texture2d> {
        use crate::data::gpu::compute::texture_ops::{TextureBinaryOp, TextureBinaryOpDefinition};
        use crate::data::gpu::resource::GpuResource;

        let def = TextureBinaryOpDefinition::new("mix(a, b, amount)");
        let output = Texture2d::create(context, self.size(), self.format)?;

        TextureBinaryOp::execute(
            context,
            &def,
            &GpuResource::Texture2d(self.clone()),
            &GpuResource::Texture2d(other.clone()),
            amount,
            &GpuResource::Texture2d(output.clone()),
        )?;

        Ok(output)
    }

    pub fn blit(&self, context: &WgpuContext, target: &Texture2d) -> Result<()> {
        let blitter = crate::globals::Blitter::new(&context.device, target.format.into());
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture2d Blit Encoder"),
            });
        blitter.blit(
            &context.device,
            &mut encoder,
            self.view.as_ref().unwrap(),
            target.view.as_ref().unwrap(),
        );
        context.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }

    pub fn clear(
        context: &WgpuContext,
        target: Texture2d,
        color: Option<crate::data::vector::Vec4>,
    ) -> Result<Texture2d> {
        target.clear_raw(
            context,
            color.unwrap_or_else(|| crate::data::vector::Vec4::new(0.0, 0.0, 0.0, 1.0)),
        )?;
        Ok(target)
    }

    pub fn clear_raw(&self, context: &WgpuContext, color: crate::data::vector::Vec4) -> Result<()> {
        let view = self
            .view
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Texture has no view"))?;
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture2d Clear Encoder"),
            });
        {
            if self.format.is_depth() {
                let depth_clear_value = if color.x == 0.0 && color.y == 0.0 && color.z == 0.0 {
                    1.0
                } else {
                    color.x
                };
                let _rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Texture2d Clear Depth Pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(depth_clear_value),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            } else {
                let mut clear_color = color;
                if self.format.is_uint() || self.format.is_sint() {
                    let max_val = match self.format {
                        TextureFormat::R8Uint
                        | TextureFormat::R8Sint
                        | TextureFormat::Rg8Uint
                        | TextureFormat::Rg8Sint
                        | TextureFormat::Rgba8Uint
                        | TextureFormat::Rgba8Sint => 255.0,
                        _ => 255.0,
                    };
                    clear_color = crate::data::vector::Vec4::new(
                        color.x * max_val,
                        color.y * max_val,
                        color.z * max_val,
                        color.w * max_val,
                    );
                }

                let _rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Texture2d Clear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: clear_color.x as f64,
                                g: clear_color.y as f64,
                                b: clear_color.z as f64,
                                a: clear_color.w as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
        }
        context.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

unsafe impl Send for Texture2d {}
unsafe impl Sync for Texture2d {}
