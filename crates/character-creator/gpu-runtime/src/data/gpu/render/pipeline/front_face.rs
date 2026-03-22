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
pub enum FrontFace {
    #[default]
    Ccw,
    Cw,
}

impl From<FrontFace> for wgpu::FrontFace {
    fn from(face: FrontFace) -> Self {
        match face {
            FrontFace::Ccw => wgpu::FrontFace::Ccw,
            FrontFace::Cw => wgpu::FrontFace::Cw,
        }
    }
}
