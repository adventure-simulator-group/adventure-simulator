use anyhow::{anyhow, Context, Result};
use gpu_runtime::globals::WgpuContext;

use crate::DistanceField;

mod pipeline;

pub struct Generator;

impl Generator {
    pub async fn generate() -> Result<DistanceField> {
        let wgpu_context = WgpuContext::new()
            .await
            .expect("Failed to create WgpuContext");

        let texture = pipeline::generate(&wgpu_context)?;

        // 6. Read Texture Data back to DistanceField
        // (Adapted from original code)
        let texture_arc = texture
            .texture
            .as_ref()
            .ok_or_else(|| anyhow!("Texture3D has no internal wgpu::Texture"))?;

        let width = texture.size.0 as usize;
        let height = texture.size.1 as usize;
        let depth = texture.size.2 as usize;

        // Create a buffer to read the texture
        // Format is R32Float (4 bytes per pixel)
        let unpadded_bytes_per_row = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;

        let buffer_size = (padded_bytes_per_row * height * depth) as wgpu::BufferAddress;

        let output_buffer = wgpu_context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SDF Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder =
            wgpu_context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("SDF Readback Encoder"),
                });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: texture_arc,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: Some(height as u32),
                },
            },
            wgpu::Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: depth as u32,
            },
        );

        let _index = wgpu_context.queue.submit(Some(encoder.finish()));

        // Map the buffer
        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            tx.send(res).unwrap();
        });

        // Poll the device to ensure the callback is called
        wgpu_context
            .device
            .poll(wgpu::PollType::Wait)
            .expect("Failed to poll device");

        rx.await
            .context("Failed to receive map_async result")?
            .context("Buffer mapping failed")?;

        let data = buffer_slice.get_mapped_range();

        // Convert to DistanceField
        // domain is [-1, 1] so size is 2.0.
        let voxel_size = 2.0 / (width as f32);
        let mut df = DistanceField::new_distance_field(width, height, depth, voxel_size);

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    // Calculate offset in buffer
                    let row_offset = (z * height + y) * padded_bytes_per_row;
                    let pixel_offset = row_offset + x * 4;

                    let bytes: [u8; 4] = data[pixel_offset..pixel_offset + 4].try_into().unwrap();
                    let val = f32::from_ne_bytes(bytes);

                    df.set(x, y, z, val);
                }
            }
        }

        drop(data);
        output_buffer.unmap();

        Ok(df)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate() {
        let df_result = Generator::generate().await;
        match df_result {
            Ok(df) => {
                let (w, h, d) = df.dimensions();
                println!(
                    "Successfully generated distance field of size {}x{}x{}",
                    w, h, d
                );

                // Inspect center value (should be negative for a sphere, or close to surface)
                // The shader generates a sphere.
                let center_val = df.get(w / 2, h / 2, d / 2);
                println!("Center value: {}", center_val);

                // Inspect corner value (should be positive)
                let corner_val = df.get(0, 0, 0);
                println!("Corner value: {}", corner_val);

                assert!(
                    center_val < corner_val,
                    "Center should be closer/inside compared to corner"
                );
                assert!(
                    *center_val != f32::INFINITY,
                    "Center value should not be infinite"
                );
            }
            Err(e) => {
                panic!("Generation failed: {:?}", e);
            }
        }
    }
}
