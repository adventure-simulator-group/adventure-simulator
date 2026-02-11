use super::ops::AttachmentOps;
use crate::data::{LoadOp, StoreOp, Texture2D, vector::Vec4};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ColorAttachment {
    pub texture: Arc<Texture2D>,
    pub view: Arc<wgpu::TextureView>,
    pub ops: AttachmentOps<wgpu::Color>,
}


impl ColorAttachment {
    pub fn new(
        texture: Texture2D,
        load_op: Option<LoadOp>,
        clear_color: Option<Vec4>,
        store_op: Option<StoreOp>,
    ) -> ColorAttachment {
        let load_op = load_op.unwrap_or_default();
        let store_op = store_op.unwrap_or_default();
        let clear_color = clear_color.unwrap_or(Vec4::new(0.0, 0.0, 0.0, 1.0));

        let wgpu_load_op = match load_op {
            LoadOp::Load => wgpu::LoadOp::Load,
            LoadOp::Clear => wgpu::LoadOp::Clear(clear_color.into()),
        };

        let wgpu_store_op = match store_op {
            StoreOp::Store => wgpu::StoreOp::Store,
            StoreOp::Discard => wgpu::StoreOp::Discard,
        };

        let view = if let Some(view) = &texture.view {
            view.clone()
        } else {
            Arc::new(
                texture
                    .texture
                    .as_ref()
                    .expect("Texture should be initialized")
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            )
        };

        ColorAttachment {
            texture: Arc::new(texture),
            view,
            ops: AttachmentOps {
                load: wgpu_load_op,
                store: wgpu_store_op,
            },
        }
    }

    pub fn get_texture(&self) -> Texture2D {
        Texture2D {
            texture: self.texture.texture.clone(),
            view: self.texture.view.clone(),
            size: self.texture.size,
        }
    }
}

unsafe impl Send for ColorAttachment {}
unsafe impl Sync for ColorAttachment {}
