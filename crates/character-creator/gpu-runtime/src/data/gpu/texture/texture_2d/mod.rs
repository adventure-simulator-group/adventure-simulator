mod image;
pub use image::*;

use std::sync::Arc;

use anyhow::Result;

use crate::{data::gpu::texture::TextureFormat, data::vector::Vec2, globals::WgpuContext};

#[derive(Clone, Debug)]
pub struct Texture2D {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: (u32, u32),
    pub format: TextureFormat,
}

impl PartialEq for Texture2D {
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

impl Default for Texture2D {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: (0, 0),
            format: TextureFormat::default(),
        }
    }
}

impl Texture2D {
    pub fn new(context: &WgpuContext, size: Vec2, format: TextureFormat) -> Result<Texture2D> {
        let wgpu_format: wgpu::TextureFormat = format.into();

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

        let is_depth = matches!(
            wgpu_format,
            wgpu::TextureFormat::Depth32Float
                | wgpu::TextureFormat::Depth24Plus
                | wgpu::TextureFormat::Depth24PlusStencil8
                | wgpu::TextureFormat::Depth32FloatStencil8
        );

        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT;

        if !is_depth {
            usage |= wgpu::TextureUsages::STORAGE_BINDING;
        }

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Texture2D"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage,
            view_formats: &[],
        };

        let texture = context.device.create_texture(&texture_desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_value = Texture2D {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size: (width, height),
            format,
        };

        Ok(texture_value)
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.size.0 as f32, self.size.1 as f32)
    }

    pub fn from_image(context: &WgpuContext, image: Image) -> Result<Texture2D> {
        if image.width == 0 || image.height == 0 {
            return Ok(Texture2D::default());
        }

        let width = image.width;
        let height = image.height;

        let texture_desc = wgpu::TextureDescriptor {
            label: Some("Image Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let texture = context.device.create_texture(&texture_desc);
        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(Texture2D {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size: (width, height),
            format: TextureFormat::Rgba8Unorm,
        })
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
            label: Some("Texture2D Staging Buffer"),
            size: staging_buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture2D Read Encoder"),
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
                depth_or_array_layers: 1,
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
                depth_or_array_layers: 1,
            },
        );

        Ok(())
    }
}

unsafe impl Send for Texture2D {}
unsafe impl Sync for Texture2D {}
