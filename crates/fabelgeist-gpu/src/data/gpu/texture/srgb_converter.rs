use crate::data::gpu::texture::Texture2d;
use crate::globals::WgpuContext;
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct TextureSrgbConverter {
    pipeline: Arc<wgpu::ComputePipeline>,
    bind_group_layout: Arc<wgpu::BindGroupLayout>,
}

impl TextureSrgbConverter {
    pub fn new(context: &WgpuContext) -> Result<Self> {
        let device = &context.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sRGB Conversion Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                r#"
                @group(0) @binding(0) var src_tex: texture_2d<f32>;
                @group(0) @binding(1) var dst_tex: texture_storage_2d<rgba8unorm, write>;

                fn linear_to_srgb(linear: vec3<f32>) -> vec3<f32> {
                    let a = 12.92 * linear;
                    let b = 1.055 * pow(linear, vec3<f32>(1.0 / 2.4)) - 0.055;
                    return select(b, a, linear <= vec3<f32>(0.0031308));
                }

                @compute @workgroup_size(16, 16, 1)
                fn main(@builtin(global_invocation_id) id: vec3<u32>) {
                    let size = textureDimensions(src_tex);
                    if (id.x >= size.x || id.y >= size.y) {
                        return;
                    }
                    let pos = id.xy;
                    let color = textureLoad(src_tex, pos, 0);
                    let srgb_rgb = linear_to_srgb(color.rgb);
                    textureStore(dst_tex, pos, vec4<f32>(srgb_rgb, color.a));
                }
                "#,
            )),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sRGB Conversion Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sRGB Conversion Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sRGB Conversion Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(Self {
            pipeline: Arc::new(pipeline),
            bind_group_layout: Arc::new(bind_group_layout),
        })
    }

    pub fn convert(&self, context: &WgpuContext, src: &Texture2d, dst: &Texture2d) -> Result<()> {
        let device = &context.device;
        let src_view = src
            .view
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Source texture view missing"))?;
        let dst_view = dst
            .view
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Destination texture view missing"))?;

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sRGB Conversion Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dst_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sRGB Conversion Encoder"),
        });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("sRGB Conversion Pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);

            // 16x16 workgroup size
            let workgroups_x = src.size.0.div_ceil(16);
            let workgroups_y = src.size.1.div_ceil(16);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        context.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}
