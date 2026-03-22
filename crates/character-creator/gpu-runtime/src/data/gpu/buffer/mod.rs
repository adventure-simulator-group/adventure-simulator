use anyhow::{Result, anyhow};
use std::sync::Arc;
use crate::globals::WgpuContext;

#[derive(Clone, Debug, Default)]
pub struct Buffer {
    pub buffer: Option<Arc<wgpu::Buffer>>,
    pub size: u64,
}

impl Buffer {
    pub fn new(
        context: &WgpuContext,
        bytes: u64,
        usage_uniform: Option<bool>,
        usage_storage: Option<bool>,
        usage_vertex: Option<bool>,
        usage_index: Option<bool>,
        usage_copy_src: Option<bool>,
        usage_copy_dst: Option<bool>,
    ) -> Result<Buffer> {
        if bytes == 0 {
            return Err(anyhow!("Buffer size must be greater than 0"));
        }

        let mut usage = wgpu::BufferUsages::empty();
        if usage_uniform.unwrap_or(false) {
            usage |= wgpu::BufferUsages::UNIFORM;
        }
        if usage_storage.unwrap_or(false) {
            usage |= wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST;
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

        // Universal default if nothing is specified (safety fallback)
        if usage.is_empty() {
            usage = wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC;
        }

        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Buffer"),
            size: bytes,
            usage,
            mapped_at_creation: false,
        });

        Ok(Buffer {
            buffer: Some(Arc::new(buffer)),
            size: bytes,
        })
    }

    pub fn size(&self) -> f64 {
        self.size as f64
    }

    pub async fn read_f32(&self, context: &WgpuContext) -> Result<Vec<f32>> {
        let buffer = self
            .buffer
            .as_ref()
            .ok_or_else(|| anyhow!("Buffer not initialized"))?;
        let size = self.size;

        // 1. Create staging buffer
        let staging_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 2. Copy data to staging buffer
        let mut encoder = context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(buffer, 0, &staging_buffer, 0, size);
        context.queue.submit(Some(encoder.finish()));

        let (tx, rx) = futures_channel::oneshot::channel();

        // 1. Start mapping in a block to ensure the BufferSlice is dropped before await
        {
            let slice = staging_buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        }

        // 2. Poll if on native
        #[cfg(not(target_arch = "wasm32"))]
        let _ = context.device.poll(wgpu::PollType::Wait);

        // 3. Await result. No non-Send types are live across this await.
        rx.await
            .map_err(|_| anyhow!("Mapping channel closed"))?
            .map_err(|_| anyhow!("GPU Mapping error"))?;

        // 4. Get data and unmap
        let slice = staging_buffer.slice(..);
        let data = slice.get_mapped_range();
        let result = bytemuck::cast_slice::<u8, f32>(&data).to_vec();
        drop(data);
        staging_buffer.unmap();

        Ok(result)
    }
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}
impl PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        (match (&self.buffer, &other.buffer) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }) && self.size == other.size
    }
}
