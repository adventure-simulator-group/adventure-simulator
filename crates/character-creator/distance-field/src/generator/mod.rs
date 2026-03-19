use gpu_runtime::prelude::*;

use crate::{BoneIndexField, BoneWeightField, DistanceField, Field};

mod pipeline;

pub struct Generator;

impl Generator {
    pub async fn generate() -> Result<(DistanceField, BoneIndexField, BoneWeightField)> {
        let wgpu_context = WgpuContext::new()
            .await
            .expect("Failed to create WgpuContext");

        let (sdf_tex, idx_tex, weight_tex, _image) = pipeline::generate(&wgpu_context)?;

        let width = sdf_tex.size.0 as usize;
        let height = sdf_tex.size.1 as usize;
        let depth = sdf_tex.size.2 as usize;
        let voxel_size = 2.0 / (width as f32);

        let mut encoder =
            wgpu_context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Readback Encoder"),
                });

        let (sdf_buffers, sdf_row_bytes) =
            Self::create_readback_buffers(&wgpu_context, &sdf_tex, &mut encoder, 4);
        let (idx_buffers, idx_row_bytes) =
            Self::create_readback_buffers(&wgpu_context, &idx_tex, &mut encoder, 4);
        let (weight_buffers, weight_row_bytes) =
            Self::create_readback_buffers(&wgpu_context, &weight_tex, &mut encoder, 16);

        wgpu_context.queue.submit(Some(encoder.finish()));

        let mut rx_sdfs = Vec::new();
        for buf in &sdf_buffers {
            rx_sdfs.push(Self::map_buffer(buf));
        }
        let mut rx_idxs = Vec::new();
        for buf in &idx_buffers {
            rx_idxs.push(Self::map_buffer(buf));
        }
        let mut rx_weights = Vec::new();
        for buf in &weight_buffers {
            rx_weights.push(Self::map_buffer(buf));
        }

        wgpu_context
            .device
            .poll(wgpu::PollType::Wait)
            .expect("Failed to poll device");

        for rx in rx_sdfs {
            rx.await.unwrap().unwrap();
        }
        for rx in rx_idxs {
            rx.await.unwrap().unwrap();
        }
        for rx in rx_weights {
            rx.await.unwrap().unwrap();
        }

        let mut sdf_field = Field::new(width, height, depth, voxel_size, 0.0f32);
        let mut idx_field = Field::new(width, height, depth, voxel_size, [0u8; 4]);
        let mut weight_field = Field::new(width, height, depth, voxel_size, [0.0f32; 4]);

        {
            let sdf_views: Vec<_> = sdf_buffers
                .iter()
                .map(|b| b.slice(..).get_mapped_range())
                .collect();
            let idx_views: Vec<_> = idx_buffers
                .iter()
                .map(|b| b.slice(..).get_mapped_range())
                .collect();
            let weight_views: Vec<_> = weight_buffers
                .iter()
                .map(|b| b.slice(..).get_mapped_range())
                .collect();

            let max_bytes_per_buffer = 512 * 1024 * 1024;

            let sdf_slices_per_buffer = (max_bytes_per_buffer / (sdf_row_bytes * height)).max(1);
            let idx_slices_per_buffer = (max_bytes_per_buffer / (idx_row_bytes * height)).max(1);
            let weight_slices_per_buffer =
                (max_bytes_per_buffer / (weight_row_bytes * height)).max(1);

            for z in 0..depth {
                let sdf_buf_idx = z / sdf_slices_per_buffer;
                let sdf_z_local = z % sdf_slices_per_buffer;

                let idx_buf_idx = z / idx_slices_per_buffer;
                let idx_z_local = z % idx_slices_per_buffer;

                let weight_buf_idx = z / weight_slices_per_buffer;
                let weight_z_local = z % weight_slices_per_buffer;

                let sdf_data = &sdf_views[sdf_buf_idx];
                let idx_data = &idx_views[idx_buf_idx];
                let weight_data = &weight_views[weight_buf_idx];

                for y in 0..height {
                    for x in 0..width {
                        let sdf_offset = (sdf_z_local * height + y) * sdf_row_bytes + x * 4;
                        let val = f32::from_ne_bytes(
                            sdf_data[sdf_offset..sdf_offset + 4].try_into().unwrap(),
                        );
                        sdf_field.set(x, y, z, val);

                        let idx_offset = (idx_z_local * height + y) * idx_row_bytes + x * 4;
                        let idx: [u8; 4] = idx_data[idx_offset..idx_offset + 4].try_into().unwrap();
                        idx_field.set(x, y, z, idx);

                        let weight_offset =
                            (weight_z_local * height + y) * weight_row_bytes + x * 16;
                        let w0 = f32::from_ne_bytes(
                            weight_data[weight_offset..weight_offset + 4]
                                .try_into()
                                .unwrap(),
                        );
                        let w1 = f32::from_ne_bytes(
                            weight_data[weight_offset + 4..weight_offset + 8]
                                .try_into()
                                .unwrap(),
                        );
                        let w2 = f32::from_ne_bytes(
                            weight_data[weight_offset + 8..weight_offset + 12]
                                .try_into()
                                .unwrap(),
                        );
                        let w3 = f32::from_ne_bytes(
                            weight_data[weight_offset + 12..weight_offset + 16]
                                .try_into()
                                .unwrap(),
                        );
                        weight_field.set(x, y, z, [w0, w1, w2, w3]);
                    }
                }
            }
        }

        for b in sdf_buffers {
            b.unmap();
        }
        for b in idx_buffers {
            b.unmap();
        }
        for b in weight_buffers {
            b.unmap();
        }

        Ok((sdf_field, idx_field, weight_field))
    }

    fn create_readback_buffers(
        wgpu_context: &WgpuContext,
        texture: &Texture3D,
        encoder: &mut wgpu::CommandEncoder,
        bytes_per_pixel: usize,
    ) -> (Vec<wgpu::Buffer>, usize) {
        let width = texture.size.0 as usize;
        let height = texture.size.1 as usize;
        let depth = texture.size.2 as usize;

        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;

        let bytes_per_slice = padded_bytes_per_row * height;
        let max_bytes_per_buffer = 512 * 1024 * 1024; // 512 MB chunk size
        let mut slices_per_buffer = max_bytes_per_buffer / bytes_per_slice;
        if slices_per_buffer == 0 {
            slices_per_buffer = 1;
        }

        let mut buffers = Vec::new();
        let mut z_start = 0;
        while z_start < depth {
            let z_end = (z_start + slices_per_buffer).min(depth);
            let current_depth = z_end - z_start;
            let buffer_size = (current_depth * bytes_per_slice) as wgpu::BufferAddress;

            let buffer = wgpu_context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Readback Buffer"),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: texture.texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: z_start as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_bytes_per_row as u32),
                        rows_per_image: Some(height as u32),
                    },
                },
                wgpu::Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: current_depth as u32,
                },
            );

            buffers.push(buffer);
            z_start = z_end;
        }

        (buffers, padded_bytes_per_row)
    }

    fn map_buffer(
        buffer: &wgpu::Buffer,
    ) -> tokio::sync::oneshot::Receiver<Result<(), wgpu::BufferAsyncError>> {
        let buffer_slice = buffer.slice(..);
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate() {
        let df_result = Generator::generate().await;
        match df_result {
            Ok((df, _, _)) => {
                let (w, h, d) = df.dimensions();
                println!(
                    "Successfully generated distance field of size {}x{}x{}",
                    w, h, d
                );

                let center_val = df.get(w / 2, h / 2, d / 2);
                println!("Center value: {}", center_val);

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
