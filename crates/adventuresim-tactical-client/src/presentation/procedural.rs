use super::*;

pub(super) fn obstacle_seed(position: Vec3) -> u64 {
    splitmix64(u64::from(position.x.to_bits()) << 32 ^ u64::from(position.z.to_bits()))
}

pub(super) fn stable_text_seed(value: &str) -> u64 {
    value.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

pub(super) fn digest_unit(value: &str) -> f32 {
    unit_hash(stable_text_seed(value))
}

pub(super) fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(super) fn unit_hash(value: u64) -> f32 {
    (value >> 40) as f32 / ((1_u32 << 24) - 1) as f32
}

pub(super) fn bps(value: u16) -> f32 {
    f32::from(value) / 10_000.0
}

pub(super) fn color_vec4(color: Color) -> Vec4 {
    Vec4::from_array(color.to_linear().to_f32_array())
}
