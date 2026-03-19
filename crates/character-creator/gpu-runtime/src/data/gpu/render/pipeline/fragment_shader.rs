use crate::data::gpu::shader::parse_naga;
use crate::data::shader;
use crate::globals::WgpuContext;
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct FragmentShader {
    pub code: String,
    pub module: Option<Arc<wgpu::ShaderModule>>,
    pub error: Arc<Mutex<Option<String>>>,
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

impl FragmentShader {
    pub const DEFAULT_CODE: &'static str = include_str!("default.wgsl");

    pub fn new(context: &WgpuContext, code: String) -> Result<FragmentShader> {
        // 1. Naga Parse & Deep Validation
        let is_glsl = shader::detect_from_code(&code) == "glsl";
        let naga_res = parse_naga(&code, wgpu::naga::ShaderStage::Fragment)
            .map_err(|e| anyhow::anyhow!("Fragment Shader Parse Error: {}", e))?;

        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );

        let info = match validator.validate(&naga_res) {
            Ok(info) => info,
            Err(e) => {
                let message = e.emit_to_string(&code);
                return Err(anyhow::anyhow!(
                    "Fragment Shader Validation Error: {}",
                    message
                ));
            }
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

        context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);

        let sm = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("FragmentShader"),
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
            return Err(anyhow!("Fragment Shader Validation Error: {}", e));
        }

        let definition = FragmentShader {
            code: code.clone(),
            module,
            error,
        };

        Ok(definition)
    }
}
