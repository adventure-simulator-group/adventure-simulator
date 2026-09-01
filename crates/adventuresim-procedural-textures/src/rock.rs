use super::*;

pub const ROCK_TEXTURE_SIZE: u32 = 256;
pub const ROCK_TILE_METRES: f32 = 2.0;
pub const ROCK_HEIGHT_RANGE_METRES: f32 = 0.032;

const ROCK_DOMAIN_COLUMNS: i32 = 8;
const ROCK_DOMAIN_ROWS: i32 = 8;
const HORIZON_DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const HORIZON_STEPS: [i32; 5] = [1, 3, 7, 15, 31];
const ROCK_PALETTE: [[u8; 3]; 4] = [
    [112, 111, 107],
    [124, 122, 116],
    [136, 133, 125],
    [147, 144, 135],
];
const ROCK_ROUGHNESS: [u8; 4] = [218, 226, 234, 222];

#[derive(Clone, Copy)]
struct RockFieldSample {
    height: f32,
    palette_index: usize,
}

fn rock_random(cell_x: i32, cell_y: i32, salt: u64) -> f32 {
    unit_hash(splitmix64(
        rock_cell_id(cell_x, cell_y) | salt.rotate_left(23),
    ))
}

fn rock_cell_id(cell_x: i32, cell_y: i32) -> u64 {
    let wrapped_x = cell_x.rem_euclid(ROCK_DOMAIN_COLUMNS) as u64;
    let wrapped_y = cell_y.rem_euclid(ROCK_DOMAIN_ROWS) as u64;
    wrapped_x | (wrapped_y << 8)
}

fn rock_edge_random(first: (i32, i32), second: (i32, i32), salt: u64) -> f32 {
    let first = rock_cell_id(first.0, first.1);
    let second = rock_cell_id(second.0, second.1);
    let (lower, upper) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    unit_hash(splitmix64(lower | (upper << 16) | salt.rotate_left(37)))
}

fn cell_site(cell_x: i32, cell_y: i32) -> Vec2 {
    Vec2::new(
        cell_x as f32 + 0.18 + 0.64 * rock_random(cell_x, cell_y, 0x4b31),
        cell_y as f32 + 0.18 + 0.64 * rock_random(cell_x, cell_y, 0xa927),
    )
}

fn periodic_value_field(u: f32, v: f32, columns: i32, rows: i32, salt: u64) -> f32 {
    let x = u.rem_euclid(1.0) * columns as f32;
    let y = v.rem_euclid(1.0) * rows as f32;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let blend_x = smoothstep(0.0, 1.0, x - x.floor());
    let blend_y = smoothstep(0.0, 1.0, y - y.floor());
    let value = |cell_x: i32, cell_y: i32| {
        let wrapped_x = cell_x.rem_euclid(columns) as u64;
        let wrapped_y = cell_y.rem_euclid(rows) as u64;
        unit_hash(splitmix64(
            wrapped_x | (wrapped_y << 16) | salt.rotate_left(29),
        )) * 2.0
            - 1.0
    };
    let lower = value(x0, y0).lerp(value(x0 + 1, y0), blend_x);
    let upper = value(x0, y0 + 1).lerp(value(x0 + 1, y0 + 1), blend_x);
    lower.lerp(upper, blend_y)
}

fn rock_field(u: f32, v: f32) -> RockFieldSample {
    let point = Vec2::new(
        u.rem_euclid(1.0) * ROCK_DOMAIN_COLUMNS as f32,
        v.rem_euclid(1.0) * ROCK_DOMAIN_ROWS as f32,
    );
    let origin_x = point.x.floor() as i32;
    let origin_y = point.y.floor() as i32;
    let mut nearest = [(f32::INFINITY, 0_i32, 0_i32); 2];
    let mut weighted_structure = 0.0;
    let mut weight_sum = 0.0;
    for cell_y in (origin_y - 1)..=(origin_y + 1) {
        for cell_x in (origin_x - 1)..=(origin_x + 1) {
            let distance = point.distance(cell_site(cell_x, cell_y));
            let weight = (-(distance / 0.72).powi(4)).exp() + 1.0e-5;
            weighted_structure += weight * (rock_random(cell_x, cell_y, 0xd513) * 2.0 - 1.0);
            weight_sum += weight;
            if distance < nearest[0].0 {
                nearest[1] = nearest[0];
                nearest[0] = (distance, cell_x, cell_y);
            } else if distance < nearest[1].0 {
                nearest[1] = (distance, cell_x, cell_y);
            }
        }
    }

    let edge_distance = nearest[1].0 - nearest[0].0;
    let structure = weighted_structure / weight_sum;
    let tau = core::f32::consts::TAU;
    let first_cell = (nearest[0].1, nearest[0].2);
    let second_cell = (nearest[1].1, nearest[1].2);
    let edge_phase = rock_edge_random(first_cell, second_cell, 0x832d);
    let edge_enabled = rock_edge_random(first_cell, second_cell, 0x191f) > 0.68;
    let interruption = smoothstep(-0.48, 0.18, (tau * (u * 3.0 + v * 5.0 + edge_phase)).sin());
    let fracture = if edge_enabled {
        (1.0 - smoothstep(0.025, 0.095, edge_distance)) * interruption
    } else {
        0.0
    };
    let broad = periodic_value_field(u, v, 3, 3, 0x4ad3);
    let aggregate = periodic_value_field(u, v, 23, 23, 0xb175);
    let height =
        (0.14 * broad + 0.28 * structure - 0.31 * fracture + 0.05 * aggregate).clamp(-0.5, 0.5);
    let palette_index = if structure < -0.28 {
        0
    } else if structure < -0.02 {
        1
    } else if structure < 0.27 {
        2
    } else {
        3
    };
    RockFieldSample {
        height,
        palette_index: palette_index.min(ROCK_PALETTE.len() - 1),
    }
}

fn rock_horizon_ao(heights: &[f32], x: i32, y: i32) -> f32 {
    let centre = periodic_sample(heights, ROCK_TEXTURE_SIZE, x, y) * ROCK_HEIGHT_RANGE_METRES;
    let texel_metres = ROCK_TILE_METRES / ROCK_TEXTURE_SIZE as f32;
    let mut visibility = 0.0;
    for (direction_x, direction_y) in HORIZON_DIRECTIONS {
        let direction_length =
            ((direction_x * direction_x + direction_y * direction_y) as f32).sqrt();
        let mut maximum_slope = 0.0_f32;
        for step in HORIZON_STEPS {
            let neighbor = periodic_sample(
                heights,
                ROCK_TEXTURE_SIZE,
                x + direction_x * step,
                y + direction_y * step,
            ) * ROCK_HEIGHT_RANGE_METRES;
            let run = step as f32 * direction_length * texel_metres;
            maximum_slope = maximum_slope.max(((neighbor - centre) / run).max(0.0));
        }
        visibility += 1.0 / (1.0 + maximum_slope * maximum_slope).sqrt();
    }
    (visibility / HORIZON_DIRECTIONS.len() as f32).clamp(0.55, 1.0)
}

fn encode_normal(normal: Vec3) -> [u8; 4] {
    let encoded = ((normal + Vec3::ONE) * 127.5).clamp(Vec3::ZERO, Vec3::splat(255.0));
    [
        encoded.x.round() as u8,
        encoded.y.round() as u8,
        encoded.z.round() as u8,
        255,
    ]
}

fn base_levels() -> [Vec<u8>; 4] {
    let pixel_count = (ROCK_TEXTURE_SIZE * ROCK_TEXTURE_SIZE) as usize;
    let texel = 1.0 / ROCK_TEXTURE_SIZE as f32;
    let samples = (0..ROCK_TEXTURE_SIZE)
        .flat_map(|y| {
            (0..ROCK_TEXTURE_SIZE)
                .map(move |x| rock_field((x as f32 + 0.5) * texel, (y as f32 + 0.5) * texel))
        })
        .collect::<Vec<_>>();
    let heights = samples
        .iter()
        .map(|sample| sample.height)
        .collect::<Vec<_>>();
    let mut albedo = Vec::with_capacity(pixel_count * 4);
    let mut normal = Vec::with_capacity(pixel_count * 4);
    let mut height = Vec::with_capacity(pixel_count * 4);
    let mut arm = Vec::with_capacity(pixel_count * 4);
    for y in 0..ROCK_TEXTURE_SIZE {
        for x in 0..ROCK_TEXTURE_SIZE {
            let index = (y * ROCK_TEXTURE_SIZE + x) as usize;
            let sample = samples[index];
            albedo.extend_from_slice(&[
                ROCK_PALETTE[sample.palette_index][0],
                ROCK_PALETTE[sample.palette_index][1],
                ROCK_PALETTE[sample.palette_index][2],
                255,
            ]);
            let height_x = periodic_sample(&heights, ROCK_TEXTURE_SIZE, x as i32 + 1, y as i32)
                - periodic_sample(&heights, ROCK_TEXTURE_SIZE, x as i32 - 1, y as i32);
            let height_y = periodic_sample(&heights, ROCK_TEXTURE_SIZE, x as i32, y as i32 + 1)
                - periodic_sample(&heights, ROCK_TEXTURE_SIZE, x as i32, y as i32 - 1);
            let slope_scale = ROCK_HEIGHT_RANGE_METRES / (2.0 * texel * ROCK_TILE_METRES);
            normal.extend_from_slice(&encode_normal(
                Vec3::new(-height_x * slope_scale, -height_y * slope_scale, 1.0).normalize(),
            ));
            let encoded_height = ((sample.height + 0.5) * 255.0).round() as u8;
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            let ao = (rock_horizon_ao(&heights, x as i32, y as i32) * 255.0).round() as u8;
            arm.extend_from_slice(&[ao, ROCK_ROUGHNESS[sample.palette_index], 0, 255]);
        }
    }
    [albedo, normal, height, arm]
}

fn decode_normal(pixel: &[u8]) -> Vec3 {
    Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32) / 127.5 - Vec3::ONE
}

fn srgb_to_linear(value: u8) -> f32 {
    (value as f32 / 255.0).powf(2.2)
}

fn linear_to_srgb(value: f32) -> u8 {
    (value.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8
}

fn downsample_levels(previous: [&[u8]; 4], previous_size: u32) -> [Vec<u8>; 4] {
    let next_size = previous_size / 2;
    let mut next =
        core::array::from_fn(|_| Vec::with_capacity((next_size * next_size * 4) as usize));
    for y in 0..next_size {
        for x in 0..next_size {
            let indices = [
                ((y * 2 * previous_size + x * 2) * 4) as usize,
                ((y * 2 * previous_size + x * 2 + 1) * 4) as usize,
                ((((y * 2 + 1) * previous_size) + x * 2) * 4) as usize,
                ((((y * 2 + 1) * previous_size) + x * 2 + 1) * 4) as usize,
            ];
            let mut linear_color = Vec3::ZERO;
            let mut normal_sum = Vec3::ZERO;
            let mut ao = 0.0;
            let mut roughness_squared = 0.0;
            let mut height = 0_u32;
            for index in indices {
                linear_color += Vec3::new(
                    srgb_to_linear(previous[0][index]),
                    srgb_to_linear(previous[0][index + 1]),
                    srgb_to_linear(previous[0][index + 2]),
                );
                normal_sum += decode_normal(&previous[1][index..index + 4]);
                height += previous[2][index] as u32;
                ao += previous[3][index] as f32 / 255.0;
                let roughness = previous[3][index + 1] as f32 / 255.0;
                roughness_squared += roughness * roughness;
            }
            linear_color *= 0.25;
            next[0].extend_from_slice(&[
                linear_to_srgb(linear_color.x),
                linear_to_srgb(linear_color.y),
                linear_to_srgb(linear_color.z),
                255,
            ]);
            let average_normal = normal_sum * 0.25;
            let normal_variance = (1.0 - average_normal.length()).max(0.0);
            next[1].extend_from_slice(&encode_normal(average_normal.normalize_or(Vec3::Z)));
            let encoded_height = ((height + 2) / 4) as u8;
            next[2].extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            let average_ao = ao * 0.25;
            let filtered_ao = average_ao + (1.0 - average_ao) * normal_variance.min(1.0);
            let filtered_roughness = (roughness_squared * 0.25 + normal_variance * 0.35)
                .sqrt()
                .clamp(0.0, 1.0);
            next[3].extend_from_slice(&[
                (filtered_ao * 255.0).round() as u8,
                (filtered_roughness * 255.0).round() as u8,
                0,
                255,
            ]);
        }
    }
    next
}

fn complete_mips(base: [Vec<u8>; 4]) -> [Vec<u8>; 4] {
    let mut complete = base.clone();
    let mut previous = base;
    let mut previous_size = ROCK_TEXTURE_SIZE;
    while previous_size > 1 {
        let next = downsample_levels(
            [&previous[0], &previous[1], &previous[2], &previous[3]],
            previous_size,
        );
        for channel in 0..4 {
            complete[channel].extend_from_slice(&next[channel]);
        }
        previous = next;
        previous_size /= 2;
    }
    complete
}

fn rock_image(data: Vec<u8>, srgb: bool) -> Image {
    let base_level_length = (ROCK_TEXTURE_SIZE * ROCK_TEXTURE_SIZE * 4) as usize;
    let mut image = Image::new(
        Extent3d {
            width: ROCK_TEXTURE_SIZE,
            height: ROCK_TEXTURE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data[..base_level_length].to_vec(),
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = ROCK_TEXTURE_SIZE.ilog2() + 1;
    use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

pub(super) fn generate_rock_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let [albedo, normal, height, arm] = complete_mips(base_levels());
    SurfaceTextureSet {
        albedo: images.add(rock_image(albedo, true)),
        normal_gl: images.add(rock_image(normal, false)),
        height: images.add(rock_image(height, false)),
        arm: images.add(rock_image(arm, false)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn field_is_periodic_deterministic_and_physically_scaled() {
        for (u, v) in [(0.0, 0.17), (0.23, 0.51), (0.61, 0.97), (0.91, 0.08)] {
            let sample = rock_field(u, v);
            assert_eq!(sample.height.to_bits(), rock_field(u, v).height.to_bits());
            assert!((sample.height - rock_field(u + 1.0, v).height).abs() < 1.0e-5);
            assert!((sample.height - rock_field(u, v + 1.0).height).abs() < 1.0e-5);
        }
        assert_eq!(ROCK_TILE_METRES / ROCK_DOMAIN_COLUMNS as f32, 0.25);
        assert!((0.024..=0.040).contains(&ROCK_HEIGHT_RANGE_METRES));
        assert!(ROCK_TILE_METRES / ROCK_TEXTURE_SIZE as f32 <= 0.008);
    }

    #[test]
    fn outputs_are_deterministic_mipped_and_channel_correct() {
        let mut first_images = Assets::<Image>::default();
        let first = generate_rock_textures(&mut first_images);
        let mut second_images = Assets::<Image>::default();
        let second = generate_rock_textures(&mut second_images);
        for (first_handle, second_handle) in [
            (&first.albedo, &second.albedo),
            (&first.normal_gl, &second.normal_gl),
            (&first.height, &second.height),
            (&first.arm, &second.arm),
        ] {
            let first_image = first_images.get(first_handle).unwrap();
            let second_image = second_images.get(second_handle).unwrap();
            assert_eq!(first_image.data, second_image.data);
            assert_eq!(first_image.texture_descriptor.mip_level_count, 9);
            let mip_texels = (0..9)
                .map(|level| (ROCK_TEXTURE_SIZE >> level).pow(2))
                .sum::<u32>();
            assert_eq!(
                first_image.data.as_ref().unwrap().len(),
                (mip_texels * 4) as usize
            );
        }
        let arm = first_images.get(&first.arm).unwrap().data.as_ref().unwrap();
        assert!(
            arm.chunks_exact(4)
                .all(|pixel| pixel[2] == 0 && pixel[3] == 255)
        );
    }

    #[test]
    fn base_color_and_roughness_use_restrained_solid_regions() {
        let [albedo, _, _, arm] = base_levels();
        let colors = albedo
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect::<BTreeSet<_>>();
        let roughness = arm
            .chunks_exact(4)
            .map(|pixel| pixel[1])
            .collect::<BTreeSet<_>>();
        assert_eq!(colors, ROCK_PALETTE.into_iter().collect());
        assert_eq!(roughness, ROCK_ROUGHNESS.into_iter().collect());
    }
}
