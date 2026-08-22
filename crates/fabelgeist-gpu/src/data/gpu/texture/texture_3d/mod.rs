use std::sync::Arc;

use crate::data::gpu::texture::TextureFormat;
use anyhow::Result;

use crate::{data::vector::Vec3, globals::WgpuContext};

#[derive(Clone, Debug)]
pub struct Texture3d {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: (u32, u32, u32),
    pub format: TextureFormat,
    pub usage: wgpu::TextureUsages,
}

impl PartialEq for Texture3d {
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

impl Default for Texture3d {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: (0, 0, 0),
            format: TextureFormat::default(),
            usage: wgpu::TextureUsages::empty(),
        }
    }
}

impl Texture3d {
    pub fn new(context: &WgpuContext, size: Vec3, format: TextureFormat) -> Result<Texture3d> {
        let _wgpu_format: wgpu::TextureFormat = format.into();

        if size.x.is_nan() || size.y.is_nan() || size.z.is_nan() {
            return Err(anyhow::anyhow!("Texture size contains NaN: {:?}", size));
        }

        let width = size.x.max(1.0) as u32;
        let height = size.y.max(1.0) as u32;
        let depth = size.z.max(1.0) as u32;

        let base_format = format.linear_counterpart();
        let wgpu_base_format: wgpu::TextureFormat = base_format.into();

        let mut view_formats = vec![wgpu_base_format];
        let counterpart = base_format.srgb_counterpart().into();
        if counterpart != wgpu_base_format {
            view_formats.push(counterpart);
        }

        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC;

        if format.supports_storage() && !format.is_srgb() {
            usage |= wgpu::TextureUsages::STORAGE_BINDING;
        }

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Texture3d"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: depth,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
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
                return Err(anyhow::anyhow!("WGPU Texture3d Creation Error: {}", err));
            }
        }

        let texture_value = Texture3d {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size: (width, height, depth),
            format,
            usage,
        };

        Ok(texture_value)
    }

    pub fn size(&self) -> Vec3 {
        Vec3::new(self.size.0 as f32, self.size.1 as f32, self.size.2 as f32)
    }

    pub async fn read<T: bytemuck::AnyBitPattern>(&self, context: &WgpuContext) -> Result<Vec<T>> {
        let (width, height, depth) = self.size;
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Texture is not initialized"))?;
        let pixel_size = self.format.pixel_size();
        let bytes_per_row = width * pixel_size;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = (bytes_per_row + align - 1) & !(align - 1);
        let staging_buffer_size = (padded_bytes_per_row * height * depth) as u64;

        let staging_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Texture3d Staging Buffer"),
            size: staging_buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture3d Read Encoder"),
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
                depth_or_array_layers: depth,
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

        let mut result =
            Vec::with_capacity((width * height * depth) as usize * pixel_size as usize);
        if padded_bytes_per_row == bytes_per_row {
            result.extend_from_slice(&data);
        } else {
            for slice_idx in 0..depth {
                for row in 0..height {
                    let start = (slice_idx * height * padded_bytes_per_row
                        + row * padded_bytes_per_row) as usize;
                    let end = start + bytes_per_row as usize;
                    result.extend_from_slice(&data[start..end]);
                }
            }
        }

        drop(data);
        staging_buffer.unmap();

        Ok(bytemuck::cast_slice::<u8, T>(&result).to_vec())
    }

    pub fn write<T: bytemuck::NoUninit>(&self, context: &WgpuContext, data: &[T]) -> Result<()> {
        let (width, height, depth) = self.size;
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
                depth_or_array_layers: depth,
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
                    "WGPU Texture3d view_with_format Error (requested {:?}): {}",
                    format,
                    err
                ));
            }
        }

        Ok(Arc::new(view))
    }
}

unsafe impl Send for Texture3d {}
unsafe impl Sync for Texture3d {}
