use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Buffer {
    pub buffer: Option<Arc<wgpu::Buffer>>,
    pub size: u64,
}

impl Buffer {
    pub fn new(
        context: &WgpuContext,
        size: f64,
        usage_uniform: Option<bool>,
        usage_storage: Option<bool>,
        usage_vertex: Option<bool>,
        usage_index: Option<bool>,
        usage_copy_src: Option<bool>,
        usage_copy_dst: Option<bool>,
    ) -> Result<Buffer> {
        let size = size as u64;
        if size == 0 {
            return Err(anyhow!("Buffer size must be greater than 0"));
        }

        let mut usage = wgpu::BufferUsages::empty();
        if usage_uniform.unwrap_or(false) {
            usage |= wgpu::BufferUsages::UNIFORM;
        }
        if usage_storage.unwrap_or(false) {
            usage |= wgpu::BufferUsages::STORAGE;
        }
        if usage_vertex.unwrap_or(false) {
            usage |= wgpu::BufferUsages::VERTEX;
        }
        if usage_index.unwrap_or(false) {
            usage |= wgpu::BufferUsages::INDEX;
        }
        if usage_copy_src.unwrap_or(false) {
            usage |= wgpu::BufferUsages::COPY_SRC;
        }
        if usage_copy_dst.unwrap_or(false) {
            usage |= wgpu::BufferUsages::COPY_DST;
        }

        // Default to Storage if none specified, for flexibility
        if usage.is_empty() {
            usage = wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC;
        }

        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buffer"),
            size,
            usage,
            mapped_at_creation: false,
        });

        Ok(Buffer {
            buffer: Some(Arc::new(buffer)),
            size,
        })
    }

    pub fn size(&self) -> f64 {
        self.size as f64
    }
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}
