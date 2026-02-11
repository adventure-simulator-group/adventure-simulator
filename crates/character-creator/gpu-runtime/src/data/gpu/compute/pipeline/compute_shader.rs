use crate::globals::WgpuContext;
use anyhow::anyhow;
use gpu_runtime_base::Result;
use naga::front::wgsl;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct ComputeShader {
    pub code: String,
    pub module: Option<Arc<wgpu::ShaderModule>>,
    pub error: Arc<Mutex<Option<String>>>,
}

unsafe impl Send for ComputeShader {}
unsafe impl Sync for ComputeShader {}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;


impl ComputeShader {
    pub fn new(context: &WgpuContext, code: String) -> Result<ComputeShader> {
        // 1. Naga Parse & Deep Validation
        let naga_module = match wgsl::parse_str(&code) {
            Ok(m) => m,
            Err(e) => {
                let message = e.emit_to_string(&code);
                return Err(anyhow!("Compute Shader Parse Error: {}", message));
            }
        };

        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );

        if let Err(e) = validator.validate(&naga_module) {
            let message = e.emit_to_string(&code);
            return Err(anyhow!("Compute Shader Validation Error: {}", message));
        }

        // 2. WGPU Validation & Creation
        context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);

        let sm = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ComputeShader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&code)),
            });
        let module = Some(Arc::new(sm));

        let error = Arc::new(Mutex::new(None));

        #[cfg(target_arch = "wasm32")]
        {
            let device = context.device.clone();
            let error = error.clone();
            spawn_local(async move {
                if let Some(e) = device.pop_error_scope().await {
                    if let Ok(mut guard) = error.lock() {
                        *guard = Some(e.to_string());
                    }
                }
            });
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(e) = pollster::block_on(context.device.pop_error_scope()) {
            return Err(anyhow!("Compute Shader Validation Error: {}", e));
        }

        let definition = ComputeShader {
            code: code.clone(),
            module,
            error,
        };

        Ok(definition)
    }
}
