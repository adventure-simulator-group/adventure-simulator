use super::*;

pub const LIME_PLASTER_TEXTURE_SIZE: u32 = 1024;
pub const LIME_PLASTER_TILE_METRES: f32 = 1.0;
pub const LIME_PLASTER_HEIGHT_RANGE_METRES: f32 = 0.004;
pub const LIME_PLASTER_REFERENCE_SRGB: [f32; 3] = [0.745, 0.710, 0.630];

const PLASTER_BASE: Vec3 = Vec3::from_array(LIME_PLASTER_REFERENCE_SRGB);
const PLASTER_WARM: Vec3 = Vec3::new(0.790, 0.744, 0.646);
const PLASTER_COOL: Vec3 = Vec3::new(0.690, 0.676, 0.622);

#[derive(Clone, Copy, Debug)]
pub(super) struct LimePlasterSample {
    pub height: f32,
    pub ao: f32,
    pub roughness: f32,
    pub albedo: Vec3,
    #[cfg(test)]
    pub pinhole: f32,
}

fn hash_grid(x: i32, y: i32, cells: i32, salt: u64) -> f32 {
    let x = x.rem_euclid(cells) as u64;
    let y = y.rem_euclid(cells) as u64;
    unit_hash(splitmix64(x | (y << 16) | salt.rotate_left(33)))
}

fn quintic(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn periodic_noise(u: f32, v: f32, cells: i32, salt: u64) -> f32 {
    let x = u * cells as f32;
    let y = v * cells as f32;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = quintic(x - x.floor());
    let ty = quintic(y - y.floor());
    let lower = hash_grid(ix, iy, cells, salt).lerp(hash_grid(ix + 1, iy, cells, salt), tx);
    let upper = hash_grid(ix, iy + 1, cells, salt).lerp(hash_grid(ix + 1, iy + 1, cells, salt), tx);
    lower.lerp(upper, ty) * 2.0 - 1.0
}

fn wrapped_offset(value: f32) -> f32 {
    (value + 0.5).rem_euclid(1.0) - 0.5
}

fn cellular_feature(u: f32, v: f32, cells: i32, salt: u64, enabled_threshold: f32) -> (f32, f32) {
    let scaled_x = u * cells as f32;
    let scaled_y = v * cells as f32;
    let cell_x = scaled_x.floor() as i32;
    let cell_y = scaled_y.floor() as i32;
    let mut nearest = f32::INFINITY;
    let mut identity = 0.0;
    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let candidate_x = cell_x + offset_x;
            let candidate_y = cell_y + offset_y;
            let enabled = hash_grid(candidate_x, candidate_y, cells, salt ^ 0x61d3);
            if enabled < enabled_threshold {
                continue;
            }
            let site_x = candidate_x as f32
                + 0.16
                + hash_grid(candidate_x, candidate_y, cells, salt ^ 0x8a4f) * 0.68;
            let site_y = candidate_y as f32
                + 0.16
                + hash_grid(candidate_x, candidate_y, cells, salt ^ 0xc279) * 0.68;
            let dx = wrapped_offset((scaled_x - site_x) / cells as f32) * cells as f32;
            let dy = wrapped_offset((scaled_y - site_y) / cells as f32) * cells as f32;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance < nearest {
                nearest = distance;
                identity = hash_grid(candidate_x, candidate_y, cells, salt ^ 0x3e95);
            }
        }
    }
    (nearest, identity)
}

fn oblique_micro_variation(u: f32, v: f32) -> f32 {
    let warp_x = periodic_noise(u, v, 5, 0x47b9) * 0.024;
    let warp_y = periodic_noise(u, v, 7, 0xd263) * 0.024;
    let diagonal = periodic_noise(u + v + warp_x, v - u + warp_y, 19, 0x8e31);
    let cross_diagonal = periodic_noise(u * 2.0 + v + warp_y, u - v * 2.0 + warp_x, 31, 0x35ad);
    diagonal * 0.62 + cross_diagonal * 0.38
}

fn slope_adjusted_roughness(base: f32, physical_slope: f32) -> f32 {
    (base + smoothstep(0.035, 0.28, physical_slope) * 0.045).clamp(0.74, 0.94)
}

pub(super) fn lime_plaster_sample(u: f32, v: f32) -> LimePlasterSample {
    let broad = periodic_noise(u, v, 4, 0x9c31);
    let trowel = periodic_noise(u, v, 17, 0x537b);
    let sand = periodic_noise(u, v, 73, 0xa8d5);
    let fine_aggregate = periodic_noise(u, v, 181, 0xb74d);
    let mineral = periodic_noise(u, v, 13, 0x2f49);
    let sweep_phase = u * 2.0 + v + periodic_noise(u, v, 3, 0xd731) * 0.22;
    let sweep = (core::f32::consts::TAU * sweep_phase).sin() * 0.070;

    let (aggregate_distance, aggregate_identity) = cellular_feature(u, v, 128, 0x63af, 0.82);
    let aggregate_radius = 0.10 + aggregate_identity * 0.10;
    let aggregate = 1.0
        - smoothstep(
            aggregate_radius,
            aggregate_radius + 0.085,
            aggregate_distance,
        );

    let (pinhole_distance, pinhole_identity) = cellular_feature(u, v, 32, 0x91c7, 0.84);
    let pinhole_radius = 0.035 + pinhole_identity * 0.050;
    let pinhole = 1.0 - smoothstep(pinhole_radius, pinhole_radius + 0.040, pinhole_distance);

    let micro_variation = oblique_micro_variation(u, v);
    let height = (broad * 0.18
        + trowel * 0.27
        + sweep
        + sand * 0.075
        + fine_aggregate * 0.055
        + micro_variation * 0.070
        + aggregate * 0.12
        - pinhole * 0.42)
        .clamp(-1.0, 1.0);
    let mineral_mix = ((mineral + 1.0) * 0.5 * 8.0).round() / 8.0;
    let grain_mix = ((fine_aggregate + 1.0) * 0.5 * 12.0).round() / 12.0;
    let warm_mix = smoothstep(-0.65, 0.75, broad * 0.22 + mineral * 0.78);
    let mut albedo = PLASTER_COOL
        .lerp(PLASTER_WARM, warm_mix)
        .lerp(PLASTER_BASE, 0.56);
    albedo *= 0.965
        + mineral_mix * 0.040
        + grain_mix * 0.035
        + trowel * 0.012
        + micro_variation * 0.010
        + aggregate * 0.025;
    albedo *= 1.0 - pinhole * 0.17;

    let cavity = (pinhole * 0.15 + (-height).max(0.0) * 0.035).clamp(0.0, 0.22);
    let ao = (1.0 - cavity).clamp(0.76, 1.0);
    let roughness = (0.805
        + (micro_variation + 1.0) * 0.018
        + aggregate * 0.050
        + pinhole * 0.035
        + fine_aggregate.max(0.0) * 0.018
        - trowel.max(0.0) * 0.018)
        .clamp(0.74, 0.94);
    LimePlasterSample {
        height,
        ao,
        roughness,
        albedo: albedo.clamp(Vec3::splat(0.0), Vec3::splat(1.0)),
        #[cfg(test)]
        pinhole,
    }
}

fn encode_unit(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn generate_lime_plaster_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = LIME_PLASTER_TEXTURE_SIZE;
    let pixel_count = size.pow(2) as usize;
    let mut albedo = Vec::with_capacity(pixel_count * 4);
    let mut normal = Vec::with_capacity(pixel_count * 4);
    let mut height = Vec::with_capacity(pixel_count * 4);
    let mut arm = Vec::with_capacity(pixel_count * 4);
    let texel_metres = LIME_PLASTER_TILE_METRES / size as f32;
    let uv_step = 1.0 / size as f32;

    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let sample = lime_plaster_sample(u, v);
            let left = lime_plaster_sample(u - uv_step, v).height;
            let right = lime_plaster_sample(u + uv_step, v).height;
            let down = lime_plaster_sample(u, v - uv_step).height;
            let up = lime_plaster_sample(u, v + uv_step).height;
            let dh_dx =
                (right - left) * LIME_PLASTER_HEIGHT_RANGE_METRES * 0.5 / (2.0 * texel_metres);
            let dh_dy = (up - down) * LIME_PLASTER_HEIGHT_RANGE_METRES * 0.5 / (2.0 * texel_metres);
            let normal_vector = Vec3::new(-dh_dx, -dh_dy, 1.0).normalize();
            let roughness = slope_adjusted_roughness(sample.roughness, dh_dx.hypot(dh_dy));

            albedo.extend_from_slice(&[
                encode_unit(sample.albedo.x),
                encode_unit(sample.albedo.y),
                encode_unit(sample.albedo.z),
                255,
            ]);
            normal.extend_from_slice(&[
                encode_unit(normal_vector.x * 0.5 + 0.5),
                encode_unit(normal_vector.y * 0.5 + 0.5),
                encode_unit(normal_vector.z * 0.5 + 0.5),
                255,
            ]);
            let encoded_height = encode_unit(sample.height * 0.5 + 0.5);
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            arm.extend_from_slice(&[encode_unit(sample.ao), encode_unit(roughness), 0, 255]);
        }
    }

    let mut albedo_image = image_rgba_mipped(albedo, size, true);
    albedo_image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    SurfaceTextureSet {
        albedo: images.add(albedo_image),
        normal_gl: images.add(image_rgba_mipped(normal, size, true)),
        height: images.add(image_rgba_mipped(height, size, true)),
        arm: images.add(image_rgba_mipped(arm, size, true)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn sampling_is_deterministic_and_periodic() {
        for (u, v) in [(0.0, 0.13), (0.07, 0.61), (0.48, 0.94), (0.91, 0.22)] {
            let sample = lime_plaster_sample(u, v);
            let repeated = lime_plaster_sample(u + 1.0, v - 1.0);
            assert_eq!(
                sample.height.to_bits(),
                lime_plaster_sample(u, v).height.to_bits()
            );
            assert!((sample.height - repeated.height).abs() < 1.0e-4);
            assert!((sample.ao - repeated.ao).abs() < 1.0e-4);
            assert!((sample.roughness - repeated.roughness).abs() < 1.0e-4);
            assert!(sample.albedo.distance(repeated.albedo) < 1.0e-4);
        }
    }

    #[test]
    fn roughness_is_oblique_periodic_and_increases_with_relief_slope() {
        for (u, v) in [(0.12, 0.37), (0.58, 0.81), (0.93, 0.04)] {
            let variation = oblique_micro_variation(u, v);
            assert!((variation - oblique_micro_variation(u + 1.0, v - 1.0)).abs() < 1.0e-4);
        }
        let base = 0.81;
        assert!(slope_adjusted_roughness(base, 0.18) > slope_adjusted_roughness(base, 0.0));
        assert!(slope_adjusted_roughness(base, 0.70) <= 0.94);
    }

    #[test]
    fn physical_scale_preserves_plaster_relief_and_sparse_pinholes() {
        assert_eq!(LIME_PLASTER_TILE_METRES, 1.0);
        assert!((0.003..=0.005).contains(&LIME_PLASTER_HEIGHT_RANGE_METRES));
        assert!(LIME_PLASTER_TILE_METRES / LIME_PLASTER_TEXTURE_SIZE as f32 <= 0.001);
        let mut pinholes = 0_usize;
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for y in 0..256 {
            for x in 0..256 {
                let sample =
                    lime_plaster_sample((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                pinholes += usize::from(sample.pinhole > 0.5);
                minimum = minimum.min(sample.height);
                maximum = maximum.max(sample.height);
            }
        }
        assert!((15..=260).contains(&pinholes), "pinhole texels: {pinholes}");
        assert!(minimum < -0.35, "minimum height: {minimum}");
        assert!(maximum > 0.25, "maximum height: {maximum}");
    }

    #[test]
    fn fine_aggregate_remains_visible_below_a_centimetre() {
        let mut total_difference = 0.0;
        let samples = 256;
        let offset = 0.004 / LIME_PLASTER_TILE_METRES;
        for index in 0..samples {
            let u = (index as f32 * 0.618_034).fract();
            let v = (index as f32 * 0.414_214).fract();
            let first = lime_plaster_sample(u, v).albedo;
            let adjacent = lime_plaster_sample(u + offset, v).albedo;
            total_difference += first.distance(adjacent);
        }
        let mean_difference = total_difference / samples as f32;
        assert!(
            mean_difference > 0.004,
            "four-millimetre albedo difference: {mean_difference}"
        );
    }

    #[test]
    fn generated_channels_are_coherent_and_have_complete_mips() {
        let mut images = Assets::<Image>::default();
        let textures = generate_lime_plaster_textures(&mut images);
        assert_eq!(images.len(), 4);
        let expected_mips = LIME_PLASTER_TEXTURE_SIZE.ilog2() + 1;
        let mip_texels = (0..expected_mips)
            .map(|level| (LIME_PLASTER_TEXTURE_SIZE >> level).pow(2))
            .sum::<u32>() as usize;
        for handle in [
            &textures.albedo,
            &textures.normal_gl,
            &textures.height,
            &textures.arm,
        ] {
            let image = images.get(handle).unwrap();
            assert_eq!((image.width(), image.height()), (1024, 1024));
            assert_eq!(image.texture_descriptor.mip_level_count, expected_mips);
            assert_eq!(image.data.as_ref().unwrap().len(), mip_texels * 4);
        }
        assert_eq!(
            images
                .get(&textures.albedo)
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        let arm = images.get(&textures.arm).unwrap().data.as_ref().unwrap();
        assert!(
            arm.as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[2] == 0 && pixel[3] == 255)
        );
        let albedo = images.get(&textures.albedo).unwrap().data.as_ref().unwrap();
        let palette = albedo[..LIME_PLASTER_TEXTURE_SIZE.pow(2) as usize * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect::<BTreeSet<_>>();
        assert!(
            (24..=1024).contains(&palette.len()),
            "albedo colors: {}",
            palette.len()
        );
    }
}
