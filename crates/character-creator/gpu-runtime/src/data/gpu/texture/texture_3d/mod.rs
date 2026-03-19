use std::sync::Arc;

use crate::data::gpu::texture::TextureFormat;
use anyhow::Result;

use crate::{data::vector::Vec3, globals::WgpuContext};

#[derive(Clone, Debug)]
pub struct Texture3D {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: (u32, u32, u32),
}

impl Default for Texture3D {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: (0, 0, 0),
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
        };

        Ok(texture_value)
    }
}

unsafe impl Send for Texture3D {}
unsafe impl Sync for Texture3D {}
