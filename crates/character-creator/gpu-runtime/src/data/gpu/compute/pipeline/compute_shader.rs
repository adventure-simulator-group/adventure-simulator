use crate::data::{gpu::shader::parse_naga, shader};
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct ComputeShader {
    pub code: String,
    pub module: Option<Arc<wgpu::ShaderModule>>,
    pub error: Arc<Mutex<Option<String>>>,
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

impl ComputeShader {
    pub fn new(context: &WgpuContext, code: String) -> Result<ComputeShader> {
        // 1. Naga Parse & Deep Validation
        let is_glsl = shader::detect_from_code(&code) == "glsl";
        let naga_res = parse_naga(&code, wgpu::naga::ShaderStage::Compute)
            .map_err(|e| anyhow::anyhow!("Compute Shader Parse Error: {}", e))?;

        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );

        let info = if let Ok(info) = validator.validate(&naga_res) {
            info
        } else {
            let e = validator.validate(&naga_res).unwrap_err();
            let message = e.emit_to_string(&code);
            return Err(anyhow::anyhow!(
                "Compute Shader Validation Error: {}",
                message
            ));
        };

        // 2. Convert to WGSL for WGPU compatibility (if it was GLSL)
        let wgsl_code = if is_glsl {
            match wgpu::naga::back::wgsl::write_string(
                &naga_res,
                &info,
                wgpu::naga::back::wgsl::WriterFlags::empty(),
            ) {
                Ok(s) => s,
                Err(e) => return Err(anyhow!("Failed to convert GLSL to WGSL: {}", e)),
            }
        } else {
            code.clone()
        };

        // 3. WGPU Validation & Creation
        context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);

        let sm = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ComputeShader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(&wgsl_code)),
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
impl PartialEq for ComputeShader {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}
