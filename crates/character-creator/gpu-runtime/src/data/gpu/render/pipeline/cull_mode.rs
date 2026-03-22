use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    strum::EnumIter,
    strum::AsRefStr,
    strum::EnumString,
)]
pub enum CullMode {
    #[default]
    None,
    Front,
    Back,
}

impl From<CullMode> for Option<wgpu::Face> {
    fn from(mode: CullMode) -> Self {
        match mode {
            CullMode::None => None,
            CullMode::Front => Some(wgpu::Face::Front),
            CullMode::Back => Some(wgpu::Face::Back),
        }
    }
}
