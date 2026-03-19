#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentOps<T: Clone + Copy + Default> {
    pub load: wgpu::LoadOp<T>,
    pub store: wgpu::StoreOp,
}

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
pub enum LoadOp {
    Load,
    #[default]
    Clear,
}

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
pub enum StoreOp {
    #[default]
    Store,
    Discard,
}
