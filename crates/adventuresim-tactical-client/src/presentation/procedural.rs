use super::*;
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME: u64 = 0x100_0000_01b3;

pub(super) fn obstacle_seed(position: Vec3) -> u64 {
    splitmix64(u64::from(position.x.to_bits()) << 32 ^ u64::from(position.z.to_bits()))
}

pub(super) fn stable_text_seed(value: &str) -> u64 {
    value.bytes().fold(FNV1A_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV1A_PRIME)
    })
}

pub(super) fn unit_hash(value: u64) -> f32 {
    inclusive_unit_f32(value)
}

pub(super) fn bps(value: u16) -> f32 {
    f32::from(value) / 10_000.0
}

pub(super) fn color_vec4(color: Color) -> Vec4 {
    Vec4::from_array(color.to_linear().to_f32_array())
}
