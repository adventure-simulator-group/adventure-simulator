use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct UniformMember {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone)]
pub struct TextureBinding {
    pub name: String,
    pub binding: u32,
    pub format: Option<wgpu::TextureFormat>,
    pub dimension: wgpu::TextureViewDimension,
}

#[derive(Debug, Clone)]
pub struct BufferBinding {
    pub name: String,
    pub binding: u32,
    pub ty: wgpu::BufferBindingType,
}

#[derive(Debug, Clone, Default)]
pub struct BindGroupReflection {
    pub index: u32,
    pub uniform_members: Vec<UniformMember>,
    pub uniform_buffer_size: u32,
    pub uniform_binding: Option<u32>,
    pub texture_bindings: Vec<TextureBinding>,
    pub sampler_bindings: Vec<(String, u32)>, // name, binding index
    pub buffer_bindings: Vec<BufferBinding>,
}

#[derive(Debug, Clone)]
pub struct ReflectionData {
    pub bind_groups: Vec<BindGroupReflection>,
    pub fragment_entry_point: String,
    pub vertex_entry_point: String,
}

pub fn detect_from_code(code: &str) -> String {
    if code.contains("#version") {
        return "glsl".to_string();
    }
    "wgsl".to_string()
}

pub fn parse_naga(
    code: &str,
    stage: wgpu::naga::ShaderStage,
) -> anyhow::Result<wgpu::naga::Module> {
    let lang = detect_from_code(code);

    if lang == "glsl" {
        let mut frontend = wgpu::naga::front::glsl::Frontend::default();
        frontend
            .parse(
                &wgpu::naga::front::glsl::Options {
                    stage,
                    defines: Default::default(),
                },
                code,
            )
            .map_err(|e| anyhow!("GLSL Parse Error: {:?}", e))
    } else {
        wgpu::naga::front::wgsl::parse_str(code).map_err(|e| {
            let message = e.emit_to_string(code);
            anyhow!("WGSL Parse Error: {}", message)
        })
    }
}
