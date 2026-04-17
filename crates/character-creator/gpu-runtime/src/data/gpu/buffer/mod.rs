use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Buffer {
    pub buffer: Arc<wgpu::Buffer>,
    pub size: u64,
    pub usage: wgpu::BufferUsages,
}

#[derive(Clone, Debug)]
pub struct BufferDefinition {
    pub label: Option<String>,
    pub uniform: bool,
    pub storage: bool,
    pub vertex: bool,
    pub index: bool,
    pub indirect: bool,
    pub copy_src: bool,
    pub copy_dst: bool,
    pub map_write: bool,
    pub map_read: bool,
}

impl Default for BufferDefinition {
    fn default() -> Self {
        Self::all()
    }
}

impl BufferDefinition {
    pub fn new() -> Self {
        Self {
            label: None,
            uniform: false,
            storage: false,
            vertex: false,
            index: false,
            indirect: false,
            copy_src: false,
            copy_dst: false,
            map_write: false,
            map_read: false,
        }
    }

    pub fn all() -> Self {
        Self {
            label: None,
            uniform: true,
            storage: true,
            vertex: true,
            index: true,
            indirect: true,
            copy_src: true,
            copy_dst: true,
            map_write: false,
            map_read: false,
        }
    }

    pub fn with_label(mut self, label: impl ToString) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn uniform() -> Self {
        Self::new().with_uniform()
    }

    pub fn storage() -> Self {
        Self::new().with_storage()
    }

    pub fn vertex() -> Self {
        Self::new().with_vertex()
    }

    pub fn index() -> Self {
        Self::new().with_index()
    }

    pub fn indirect() -> Self {
        Self::new().with_indirect()
    }

    pub fn map_write() -> Self {
        Self::new().with_map_write()
    }

    pub fn map_read() -> Self {
        Self::new().with_map_read()
    }

    pub fn copy_src() -> Self {
        Self::new().with_copy_src()
    }

    pub fn copy_dst() -> Self {
        Self::new().with_copy_dst()
    }

    pub fn with_uniform(mut self) -> Self {
        self.uniform = true;
        self
    }

    pub fn with_storage(mut self) -> Self {
        self.storage = true;
        self
    }

    pub fn with_vertex(mut self) -> Self {
        self.vertex = true;
        self
    }

    pub fn with_index(mut self) -> Self {
        self.index = true;
        self
    }

    pub fn with_indirect(mut self) -> Self {
        self.indirect = true;
        self
    }

    pub fn with_copy_src(mut self) -> Self {
        self.copy_src = true;
        self
    }

    pub fn with_copy_dst(mut self) -> Self {
        self.copy_dst = true;
        self
    }

    pub fn with_map_write(mut self) -> Self {
        self.map_write = true;
        self
    }

    pub fn with_map_read(mut self) -> Self {
        self.map_read = true;
        self
    }
}

impl Buffer {
    pub fn new(context: &WgpuContext, bytes: u64, definition: BufferDefinition) -> Result<Buffer> {
        if bytes == 0 {
            return Err(anyhow!("Buffer size must be greater than 0"));
        }

        let label = definition.label;

        let mut usage = wgpu::BufferUsages::empty();
        if definition.uniform {
            usage |= wgpu::BufferUsages::UNIFORM;
        }
        if definition.storage {
            usage |= wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST;
        }
        if definition.vertex {
            usage |= wgpu::BufferUsages::VERTEX;
        }
        if definition.index {
            usage |= wgpu::BufferUsages::INDEX;
        }
        if definition.indirect {
            usage |= wgpu::BufferUsages::INDIRECT;
        }
        if definition.copy_src {
            usage |= wgpu::BufferUsages::COPY_SRC;
        }
        if definition.copy_dst {
            usage |= wgpu::BufferUsages::COPY_DST;
        }
        if definition.map_write {
            usage |= wgpu::BufferUsages::MAP_WRITE;
        }
        if definition.map_read {
            usage |= wgpu::BufferUsages::MAP_READ;
        }

        // Universal default if nothing is specified (safety fallback)
        if usage.is_empty() {
            usage = wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
        }

        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: label.as_deref(),
            size: bytes,
            usage,
            mapped_at_creation: false,
        });

        Ok(Buffer {
            buffer: Arc::new(buffer),
            size: bytes,
            usage,
        })
    }

    pub fn size(&self) -> f64 {
        self.size as f64
    }

    pub async fn read<T: bytemuck::AnyBitPattern>(&self, context: &WgpuContext) -> Result<Vec<T>> {
        let size = self.size;
        let is_mappable = self.usage.contains(wgpu::BufferUsages::MAP_READ);

        let (target_buffer, needs_unmap) = if is_mappable {
            (self.buffer.clone(), false)
        } else {
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
            encoder.copy_buffer_to_buffer(&self.buffer, 0, &staging_buffer, 0, size);
            context.queue.submit(Some(encoder.finish()));
            (Arc::new(staging_buffer), true)
        };

        let (tx, rx) = futures_channel::oneshot::channel();

        // 1. Start mapping
        {
            let slice = target_buffer.slice(..);
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        }

        // 2. Poll if on native
        #[cfg(not(target_arch = "wasm32"))]
        let _ = context.device.poll(wgpu::PollType::Wait);

        // 3. Await result
        rx.await
            .map_err(|_| anyhow!("Mapping channel closed"))?
            .map_err(|_| anyhow!("GPU Mapping error"))?;

        // 4. Get data and unmap
        let slice = target_buffer.slice(..);
        let data = slice.get_mapped_range();
        let result = bytemuck::cast_slice::<u8, T>(&data).to_vec();
        drop(data);

        if needs_unmap {
            target_buffer.unmap();
        } else {
            // If it's the original buffer, we still need to unmap it so it can be used by the GPU again
            target_buffer.unmap();
        }

        Ok(result)
    }

    pub fn write<T: bytemuck::NoUninit>(&self, context: &WgpuContext, data: &[T]) -> Result<()> {
        let bytes = bytemuck::cast_slice(data);
        context.queue.write_buffer(&self.buffer, 0, bytes);
        Ok(())
    }

    pub fn from_slice<T: bytemuck::NoUninit>(
        context: &WgpuContext,
        data: &[T],
        definition: BufferDefinition,
    ) -> Result<Buffer> {
        let bytes = bytemuck::cast_slice(data);
        Self::from_bytes(context, bytes, definition)
    }

    pub fn from_bytes(
        context: &WgpuContext,
        bytes: &[u8],
        definition: BufferDefinition,
    ) -> Result<Buffer> {
        let buffer = Self::new(context, bytes.len() as u64, definition)?;
        context.queue.write_buffer(&buffer.buffer, 0, bytes);
        Ok(buffer)
    }
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.buffer, &other.buffer) && self.size == other.size
    }
}
