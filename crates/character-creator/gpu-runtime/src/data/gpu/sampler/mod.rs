mod address_mode;
mod filter_mode;

pub use address_mode::*;
pub use filter_mode::*;

use crate::globals::WgpuContext;

use gpu_runtime_base::Result;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Sampler {
    pub sampler: Option<Arc<wgpu::Sampler>>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self { sampler: None }
    }
}

unsafe impl Send for Sampler {}
unsafe impl Sync for Sampler {}

impl Sampler {
    pub fn new(
        context: &WgpuContext,
        address_mode_u: Option<SamplerAddressMode>,
        address_mode_v: Option<SamplerAddressMode>,
        address_mode_w: Option<SamplerAddressMode>,
        mag_filter: Option<SamplerFilterMode>,
        min_filter: Option<SamplerFilterMode>,
    ) -> Result<Sampler> {
        let address_mode_u = address_mode_u.unwrap_or_default();
        let address_mode_v = address_mode_v.unwrap_or_default();
        let address_mode_w = address_mode_w.unwrap_or_default();
        let mag_filter = mag_filter.unwrap_or_default();
        let min_filter = min_filter.unwrap_or_default();

        let sampler = context.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sampler"),
            address_mode_u: address_mode_u.into(),
            address_mode_v: address_mode_v.into(),
            address_mode_w: address_mode_w.into(),
            mag_filter: mag_filter.into(),
            min_filter: min_filter.into(),
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let sampler_value = Sampler {
            sampler: Some(Arc::new(sampler)),
        };

        Ok(sampler_value)
    }
}
