use super::ops::AttachmentOps;
use crate::data::{LoadOp, StoreOp, Texture2D};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct DepthStencilAttachment {
    pub texture: Arc<Texture2D>,
    pub view: Arc<wgpu::TextureView>,
    pub depth_ops: Option<AttachmentOps<f32>>,
    pub stencil_ops: Option<AttachmentOps<u32>>,
}


impl DepthStencilAttachment {
    pub fn new(
        texture: Texture2D,
        depth_load_op: Option<LoadOp>,
        clear_depth: Option<f64>,
        depth_store_op: Option<StoreOp>,
        // TODO: Stencil support
    ) -> DepthStencilAttachment {
        let depth_load_op = depth_load_op.unwrap_or_default();
        let depth_store_op = depth_store_op.unwrap_or_default();
        let clear_depth = clear_depth.unwrap_or(1.0);

        let wgpu_depth_load_op = match depth_load_op {
            LoadOp::Load => wgpu::LoadOp::Load,
            LoadOp::Clear => wgpu::LoadOp::Clear(clear_depth as f32),
        };

        let wgpu_depth_store_op = match depth_store_op {
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

        DepthStencilAttachment {
            texture: Arc::new(texture),
            view,
            depth_ops: Some(AttachmentOps {
                load: wgpu_depth_load_op,
                store: wgpu_depth_store_op,
            }),
            stencil_ops: None,
        }
    }
}

unsafe impl Send for DepthStencilAttachment {}
unsafe impl Sync for DepthStencilAttachment {}
