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
pub struct ReflectionData {
    pub uniform_members: Vec<UniformMember>,
    pub uniform_buffer_size: u32,
    pub uniform_binding: Option<u32>,
    pub texture_bindings: Vec<TextureBinding>,
    pub sampler_bindings: Vec<(String, u32)>, // name, binding index
    pub fragment_entry_point: String,
    pub vertex_entry_point: String,
}
