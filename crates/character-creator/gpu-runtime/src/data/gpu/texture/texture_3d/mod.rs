use std::sync::Arc;

use crate::data::gpu::texture::TextureFormat;
use anyhow::Result;

use crate::{data::vector::Vec3, globals::WgpuContext};

#[derive(Clone, Debug)]
pub struct Texture3D {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: (u32, u32, u32),
    pub format: TextureFormat,
}

impl PartialEq for Texture3D {
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
    }
}

impl Default for Texture3D {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: (0, 0, 0),
            format: TextureFormat::default(),
        }
    }
}

impl Texture3D {
    pub fn new(context: &WgpuContext, size: Vec3, format: TextureFormat) -> Result<Texture3D> {
        let wgpu_format: wgpu::TextureFormat = format.into();

        if size.x.is_nan() || size.y.is_nan() || size.z.is_nan() {
            return Err(anyhow::anyhow!("Texture size contains NaN: {:?}", size));
        }

        let width = size.x.max(1.0) as u32;
        let height = size.y.max(1.0) as u32;
        let depth = size.z.max(1.0) as u32;

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Texture3D"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: depth,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };

        let texture = context.device.create_texture(&texture_desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_value = Texture3D {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size: (width, height, depth),
            format,
        };

        Ok(texture_value)
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
            label: Some("Texture3D Staging Buffer"),
            size: staging_buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture3D Read Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
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

        let (tx, rx) = futures_channel::oneshot::channel();
        {
            let slice = staging_buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        let _ = context.device.poll(wgpu::PollType::Wait);

        rx.await
            .map_err(|_| anyhow::anyhow!("Mapping channel closed"))?
            .map_err(|_| anyhow::anyhow!("GPU Mapping error"))?;

        let slice = staging_buffer.slice(..);
        let data = slice.get_mapped_range();

        let mut result =
            Vec::with_capacity((width * height * depth) as usize * pixel_size as usize);
        if padded_bytes_per_row == bytes_per_row {
            result.extend_from_slice(&data);
        } else {
            for slice_idx in 0..depth {
                for row in 0..height {
                    let start = (slice_idx * height * padded_bytes_per_row + row * padded_bytes_per_row) as usize;
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
                texture: &texture,
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
}

unsafe impl Send for Texture3D {}
unsafe impl Sync for Texture3D {}
