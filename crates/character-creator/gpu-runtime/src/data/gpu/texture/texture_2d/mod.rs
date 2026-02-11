use std::sync::Arc;

use gpu_runtime_base::Result;

use crate::{
    data::{TextureFormat, Vec2},
    globals::WgpuContext,
};

#[derive(Clone, Debug)]
pub struct Texture2D {
    pub texture: Option<Arc<wgpu::Texture>>,
    pub view: Option<Arc<wgpu::TextureView>>,
    pub size: (u32, u32),
}

impl Default for Texture2D {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            size: (0, 0),
        }
    }
}

unsafe impl Send for Texture2D {}
unsafe impl Sync for Texture2D {}

impl Texture2D {
    pub fn new(
        context: &WgpuContext,
        size: Option<Vec2>,
        format: Option<TextureFormat>,
    ) -> Result<Texture2D> {
        let size: Vec2 = size.unwrap_or(Vec2::new(256.0, 256.0));
        let format = format.unwrap_or_default();

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
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };

        let texture = context.device.create_texture(&texture_desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_value = Texture2D {
            texture: Some(Arc::new(texture)),
            view: Some(Arc::new(view)),
            size: (width, height),
        };

        Ok(texture_value)
    }
}
