//! Irregular, gravity-laid local fieldstone masonry with recessed lime mortar.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const RUBBLE_MASONRY_TEXTURE_SIZE: u32 = 1024;
pub const RUBBLE_MASONRY_TILE_METRES: f32 = 4.8;
pub const RUBBLE_MASONRY_HEIGHT_RANGE_METRES: f32 = 0.032;

const ROWS: i32 = 22;
const MAX_STONES_PER_ROW: usize = 22;

#[derive(Clone, Copy, Debug)]
struct MasonrySample {
    height: f32,
    stone_coverage: f32,
    stone_id: u64,
    mineral: f32,
    edge_distance: f32,
}

fn hash_unit(value: u64) -> f32 {
    inclusive_unit_f32(splitmix64(value))
}

fn row_id(row: i32) -> u64 {
    splitmix64(0x794b_d25e ^ row.rem_euclid(ROWS) as u64)
}

fn stone_count(row: i32) -> usize {
    15 + (row_id(row) as usize % 7)
}

fn stone_id(row: i32, stone: usize) -> u64 {
    splitmix64(row_id(row) ^ (stone as u64).wrapping_mul(0x9e37_79b9))
}

fn row_offset(row: i32) -> f32 {
    hash_unit(row_id(row) ^ 0x9b13) * 0.83
}

fn row_at(v: f32) -> (i32, f32, f32) {
    let mut weights = [0.0; ROWS as usize];
    let mut total = 0.0;
    for row in 0..ROWS {
        let weight = 0.84 + hash_unit(row_id(row) ^ 0x5c2b) * 0.32;
        weights[row as usize] = weight;
        total += weight;
    }
    let position = v.rem_euclid(1.0) * total;
    let mut start = 0.0;
    for row in 0..ROWS {
        let weight = weights[row as usize];
        if position < start + weight || row + 1 == ROWS {
            return (row, (position - start) / weight, weight / total);
        }
        start += weight;
    }
    unreachable!()
}

fn row_pitch(row: i32) -> f32 {
    let total = (0..ROWS)
        .map(|candidate| 0.84 + hash_unit(row_id(candidate) ^ 0x5c2b) * 0.32)
        .sum::<f32>();
    (0.84 + hash_unit(row_id(row) ^ 0x5c2b) * 0.32) / total
}

fn interval_weights(row: i32, count: usize) -> ([f32; MAX_STONES_PER_ROW], f32) {
    let mut weights = [0.0; MAX_STONES_PER_ROW];
    let mut total = 0.0;
    for (stone, weight) in weights.iter_mut().take(count).enumerate() {
        let id = stone_id(row, stone);
        *weight = 0.72 + hash_unit(id ^ 0xb529) * 0.83;
        total += *weight;
    }
    (weights, total)
}

fn stone_at(row: i32, u: f32) -> (usize, f32, f32) {
    let count = stone_count(row);
    let (weights, total) = interval_weights(row, count);
    let shifted = (u + row_offset(row)).rem_euclid(1.0) * total;
    let mut start = 0.0;
    for (stone, weight) in weights.iter().copied().take(count).enumerate() {
        let end = start + weight;
        if shifted < end || stone + 1 == count {
            return (stone, (shifted - start) / weight, weight / total);
        }
        start = end;
    }
    unreachable!()
}

fn edge_wobble(coordinate: f32, id: u64, salt: u64) -> f32 {
    let phase = hash_unit(id ^ salt) * std::f32::consts::TAU;
    let secondary = hash_unit(id ^ salt.rotate_left(19)) * std::f32::consts::TAU;
    (coordinate * std::f32::consts::TAU + phase).sin() * 0.115
        + (coordinate * std::f32::consts::TAU * 2.0 + secondary).sin() * 0.038
}

fn sample_masonry(u: f32, v: f32) -> MasonrySample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let (mut row, mut local_y, mut row_pitch) = row_at(v);
    let (mut stone, mut local_x, mut width) = stone_at(row, u);
    let previous_row = row - 1;
    let (previous_stone, previous_x, previous_width) = stone_at(previous_row, u);
    let previous_id = stone_id(previous_row, previous_stone);
    let previous_pitch = self::row_pitch(previous_row);
    let previous_interlocks =
        hash_unit(previous_id ^ 0x17d9) > 0.91 && (previous_x * 2.0 - 1.0).abs() < 0.72;
    if previous_interlocks && local_y * row_pitch < previous_pitch * 0.18 {
        row = previous_row;
        stone = previous_stone;
        local_x = previous_x;
        width = previous_width;
        local_y = 1.0 + local_y * row_pitch / previous_pitch;
        row_pitch = previous_pitch;
    }
    let id = stone_id(row, stone);
    let interlocks = hash_unit(id ^ 0x17d9) > 0.91;
    let centered_x = local_x * 2.0 - 1.0;
    let centered_y = local_y * 2.0 - 1.0;

    let joint_metres = 0.006 + hash_unit(id ^ 0x315d) * 0.010;
    let contact_variation_x = 0.68
        + 0.32
            * (local_y * std::f32::consts::TAU + hash_unit(id ^ 0x1187) * 6.0)
                .sin()
                .abs();
    let contact_variation_y = 0.68
        + 0.32
            * (local_x * std::f32::consts::TAU + hash_unit(id ^ 0x8a25) * 6.0)
                .sin()
                .abs();
    let mortar_u = joint_metres / RUBBLE_MASONRY_TILE_METRES / width * contact_variation_x;
    let mortar_v = joint_metres / RUBBLE_MASONRY_TILE_METRES / row_pitch * contact_variation_y;
    let left = -1.0 + mortar_u * 2.0 + edge_wobble(local_y, id, 0x63a1);
    let right = 1.0 - mortar_u * 2.0 + edge_wobble(local_y, id, 0xe527);
    let bottom = -1.0 + mortar_v * 2.0 + edge_wobble(local_x, id, 0xa94d);
    let top_extent = if interlocks { 1.38 } else { 1.0 };
    let top = top_extent - mortar_v * 2.0 + edge_wobble(local_x, id, 0x2f17);
    let corner_cut = 1.16 + hash_unit(id ^ 0x4b6f) * 0.36;
    let corner_bias = (hash_unit(id ^ 0x907d) - 0.5) * 0.22;
    let diagonal_a = (centered_x + centered_y + corner_bias).abs() - corner_cut;
    let diagonal_b = (centered_x - centered_y - corner_bias).abs() - corner_cut;
    let edge_distance = (left - centered_x)
        .max(centered_x - right)
        .max(bottom - centered_y)
        .max(centered_y - top)
        .max(diagonal_a)
        .max(diagonal_b);
    let minimum_extent = width.min(row_pitch);
    let antialias = 0.8 / RUBBLE_MASONRY_TEXTURE_SIZE as f32 / minimum_extent;
    let stone_coverage = ((antialias - edge_distance) / (antialias * 2.0)).clamp(0.0, 1.0);

    let broad_phase = hash_unit(id ^ 0xc38b) * std::f32::consts::TAU;
    let face_variation = (centered_x * 1.35 + centered_y * 0.77 + broad_phase).sin() * 0.006
        + (centered_x * 2.7 - centered_y * 1.9 + broad_phase * 0.63).sin() * 0.003;
    let planar_tilt = centered_x * (hash_unit(id ^ 0x191f) - 0.5) * 0.025
        + centered_y * (hash_unit(id ^ 0x8d31) - 0.5) * 0.018;
    let face_height = 0.70 + (hash_unit(id ^ 0x751d) - 0.5) * 0.10 + planar_tilt + face_variation;
    let mortar_variation = ((u * 11.0 + v * 7.0) * std::f32::consts::TAU).sin() * 0.014;
    let mortar_height = 0.17 + mortar_variation + (hash_unit(id ^ 0x5fb3) - 0.5) * 0.035;

    MasonrySample {
        height: mortar_height + (face_height - mortar_height) * stone_coverage,
        stone_coverage,
        stone_id: id,
        mineral: hash_unit(id ^ 0xd651),
        edge_distance,
    }
}

fn stone_color(sample: MasonrySample) -> ([u8; 3], u8) {
    if sample.stone_coverage < 0.5 {
        let shade = if sample.mineral > 0.55 { 140 } else { 132 };
        return ([shade + 5, shade + 2, shade - 7], 238);
    }
    let palette = [
        [99, 98, 89],
        [119, 108, 88],
        [88, 91, 86],
        [128, 116, 94],
        [105, 99, 82],
        [113, 104, 86],
        [93, 96, 91],
        [123, 111, 91],
    ];
    let mut color = palette[sample.stone_id as usize % palette.len()];
    let face_shift = ((sample.height - 0.70) * 25.0).round() as i16;
    for channel in &mut color {
        *channel = (*channel as i16 + face_shift).clamp(0, 255) as u8;
    }
    let near_joint = (1.0 - (-sample.edge_distance * 18.0).clamp(0.0, 1.0)) * 7.0;
    let roughness = (221.0 + near_joint + sample.mineral * 7.0).round() as u8;
    (color, roughness)
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = RUBBLE_MASONRY_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

fn ambient_visibility(heights: &[f32], x: i32, y: i32) -> f32 {
    let center = height_at(heights, x, y);
    let mut obstruction = 0.0;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        for step in [1, 3, 7, 15] {
            obstruction += ((height_at(heights, x + dx * step, y + dy * step) - center)
                / step as f32)
                .max(0.0);
        }
    }
    (1.0 - obstruction * 2.7).clamp(0.46, 1.0)
}

pub fn generate_rubble_masonry_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = RUBBLE_MASONRY_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_masonry(
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                )
            })
        })
        .collect::<Vec<_>>();
    let heights = samples
        .iter()
        .map(|sample| sample.height)
        .collect::<Vec<_>>();
    let mut albedo = Vec::with_capacity((size * size * 4) as usize);
    let mut normal = Vec::with_capacity(albedo.capacity());
    let mut height = Vec::with_capacity(albedo.capacity());
    let mut arm = Vec::with_capacity(albedo.capacity());
    let metres_per_texel = RUBBLE_MASONRY_TILE_METRES / size as f32;
    let slope_scale = RUBBLE_MASONRY_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

    for y in 0..size {
        for x in 0..size {
            let sample = samples[(y * size + x) as usize];
            let (color, roughness) = stone_color(sample);
            albedo.extend_from_slice(&[color[0], color[1], color[2], 255]);
            let dx = height_at(&heights, x as i32 + 1, y as i32)
                - height_at(&heights, x as i32 - 1, y as i32);
            let dy = height_at(&heights, x as i32, y as i32 + 1)
                - height_at(&heights, x as i32, y as i32 - 1);
            let n = Vec3::new(-dx * slope_scale, -dy * slope_scale, 1.0).normalize();
            let encoded = ((n + Vec3::ONE) * 127.5)
                .round()
                .clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]);
            let encoded_height = (sample.height * 255.0).round() as u8;
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            let ao = (ambient_visibility(&heights, x as i32, y as i32) * 255.0).round() as u8;
            arm.extend_from_slice(&[ao, roughness, 0, 255]);
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

    fn generated() -> (Assets<Image>, SurfaceTextureSet) {
        let mut images = Assets::default();
        let textures = generate_rubble_masonry_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_deterministic() {
        let (a_images, a) = generated();
        let (b_images, b) = generated();
        for (ah, bh) in [
            (&a.albedo, &b.albedo),
            (&a.normal_gl, &b.normal_gl),
            (&a.height, &b.height),
            (&a.arm, &b.arm),
        ] {
            assert_eq!(
                a_images.get(ah).unwrap().data,
                b_images.get(bh).unwrap().data
            );
        }
    }

    #[test]
    fn analytic_field_tiles_continuously() {
        let mut maximum_error = 0.0_f32;
        for index in 0..512 {
            let c = (index as f32 + 0.5) / 512.0;
            maximum_error = maximum_error
                .max((sample_masonry(c, c).height - sample_masonry(c + 1.0, c).height).abs())
                .max((sample_masonry(c, c).height - sample_masonry(c, c + 1.0).height).abs());
        }
        assert!(
            maximum_error < f32::EPSILON,
            "periodic height error: {maximum_error}"
        );
    }

    #[test]
    fn stones_have_period_appropriate_scale_and_variety() {
        let widths = (0..ROWS)
            .flat_map(|row| {
                let count = stone_count(row);
                let (weights, total) = interval_weights(row, count);
                weights
                    .into_iter()
                    .take(count)
                    .map(move |weight| weight / total * RUBBLE_MASONRY_TILE_METRES)
            })
            .collect::<Vec<_>>();
        assert!(widths.iter().copied().fold(f32::INFINITY, f32::min) < 0.16);
        assert!(widths.iter().copied().fold(0.0_f32, f32::max) > 0.34);
        assert!(widths.iter().all(|width| (0.06..=0.55).contains(width)));
        let row_heights = (0..ROWS)
            .map(|row| row_at((row as f32 + 0.5) / ROWS as f32).2 * RUBBLE_MASONRY_TILE_METRES)
            .collect::<Vec<_>>();
        assert!(
            row_heights
                .iter()
                .all(|height| (0.12..=0.30).contains(height))
        );
    }

    #[test]
    fn broad_stones_bear_across_multiple_stones_in_the_lift_below() {
        let mut broad = 0;
        let mut multiply_supported = 0;
        for row in 0..ROWS {
            let count = stone_count(row);
            let (weights, total) = interval_weights(row, count);
            let mut start = 0.0;
            for (stone, weight) in weights.into_iter().take(count).enumerate() {
                let width = weight / total;
                let center = (start + weight * 0.5) / total - row_offset(row);
                start += weight;
                if width * RUBBLE_MASONRY_TILE_METRES < 0.23 {
                    continue;
                }
                broad += 1;
                let supports = [-0.38, 0.0, 0.38]
                    .map(|offset| stone_at(row - 1, center + offset * width).0)
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                if supports.len() >= 2 {
                    multiply_supported += 1;
                }
                assert!(stone < MAX_STONES_PER_ROW);
            }
        }
        assert!(broad > 100);
        assert!(multiply_supported as f32 / broad as f32 > 0.72);
    }

    #[test]
    fn joints_are_recessed_and_stones_are_not_inflated_pillows() {
        let mut mortar = Vec::new();
        let mut interiors = Vec::new();
        for y in 0..256 {
            for x in 0..256 {
                let sample = sample_masonry((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                if sample.stone_coverage < 0.1 {
                    mortar.push(sample.height);
                }
                if sample.stone_coverage > 0.99 && sample.edge_distance < -0.25 {
                    interiors.push(sample.height);
                }
            }
        }
        let mortar_max = mortar.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let interior_min = interiors.iter().copied().fold(f32::INFINITY, f32::min);
        let interior_span =
            interiors.iter().copied().fold(f32::NEG_INFINITY, f32::max) - interior_min;
        assert!(interior_min - mortar_max > 0.35);
        assert!(
            interior_span < 0.16,
            "interior relief span: {interior_span}"
        );
    }

    #[test]
    fn channels_are_nonmetallic_varied_and_mipped() {
        let (images, textures) = generated();
        let expected_levels = RUBBLE_MASONRY_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let n = RUBBLE_MASONRY_TEXTURE_SIZE >> level;
                (n * n * 4) as usize
            })
            .sum::<usize>();
        for handle in [
            &textures.albedo,
            &textures.normal_gl,
            &textures.height,
            &textures.arm,
        ] {
            let image = images.get(handle).unwrap();
            assert_eq!(image.texture_descriptor.mip_level_count, expected_levels);
            assert_eq!(image.data.as_ref().unwrap().len(), expected_bytes);
        }
        let arm = images.get(&textures.arm).unwrap().data.as_deref().unwrap();
        let base = &arm[..(RUBBLE_MASONRY_TEXTURE_SIZE.pow(2) * 4) as usize];
        assert!(base.iter().skip(2).step_by(4).all(|value| *value == 0));
        assert!(
            base.iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 24
        );
        assert!(
            base.iter()
                .skip(1)
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 8
        );
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target"]
    fn export_rubble_masonry_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(RUBBLE_MASONRY_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn save_rgba(path: &Path, data: &[u8]) {
            image::save_buffer_with_format(
                path,
                data,
                RUBBLE_MASONRY_TEXTURE_SIZE,
                RUBBLE_MASONRY_TEXTURE_SIZE,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
        }

        fn fnv1a64(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
            })
        }

        let output_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let output = output_root.join("procedural-texture-reviews/rubble-masonry/candidate-3");
        let before = output.parent().unwrap().join("before");
        fs::create_dir_all(&output).unwrap();
        fs::create_dir_all(&before).unwrap();
        let baseline = (0..RUBBLE_MASONRY_TEXTURE_SIZE)
            .flat_map(|y| {
                (0..RUBBLE_MASONRY_TEXTURE_SIZE).flat_map(move |x| {
                    if (x / 64 + y / 64) % 2 == 0 {
                        [104, 100, 88, 255]
                    } else {
                        [132, 126, 108, 255]
                    }
                })
            })
            .collect::<Vec<_>>();
        save_rgba(
            &before.join("rubble-masonry-planned-baseline.png"),
            &baseline,
        );

        let (images, textures) = generated();
        let channels = [
            ("albedo", &textures.albedo),
            ("normal", &textures.normal_gl),
            ("height", &textures.height),
            ("arm", &textures.arm),
        ];
        let mut manifest_entries = Vec::new();
        for (name, handle) in channels {
            let data = base_rgba(&images, handle);
            let full_name = format!("rubble-masonry-{name}.png");
            save_rgba(&output.join(&full_name), &data);
            let base = ImageBuffer::<Rgba<u8>, _>::from_raw(
                RUBBLE_MASONRY_TEXTURE_SIZE,
                RUBBLE_MASONRY_TEXTURE_SIZE,
                data.clone(),
            )
            .unwrap();
            let mut tiled = ImageBuffer::new(
                RUBBLE_MASONRY_TEXTURE_SIZE * 2,
                RUBBLE_MASONRY_TEXTURE_SIZE * 2,
            );
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * RUBBLE_MASONRY_TEXTURE_SIZE),
                        i64::from(tile_y * RUBBLE_MASONRY_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("rubble-masonry-{name}-tile-2x2.png")))
                .unwrap();
            for preview_size in [128, 64] {
                imageops::resize(
                    &base,
                    preview_size,
                    preview_size,
                    imageops::FilterType::Lanczos3,
                )
                .save(output.join(format!("rubble-masonry-{name}-{preview_size}.png")))
                .unwrap();
            }
            manifest_entries.push(format!(
                "    {{\"file\":\"{full_name}\",\"fnv1a64\":\"{:016x}\"}}",
                fnv1a64(&data)
            ));
        }

        let arm = base_rgba(&images, &textures.arm);
        for (name, channel) in [("ao", 0), ("roughness", 1), ("metallic", 2)] {
            let separated = arm
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| {
                    let value = pixel[channel];
                    [value, value, value, 255]
                })
                .collect::<Vec<_>>();
            save_rgba(
                &output.join(format!("rubble-masonry-{name}.png")),
                &separated,
            );
        }

        fs::write(
            output.join("provenance.txt"),
            concat!(
                "recipe=rubble-masonry\n",
                "candidate=3\n",
                "source=deterministic analytic fieldstone courses; no external imagery\n",
                "physical_tile_metres=2.4\n",
                "height_range_metres=0.032\n",
                "intended_use=fortification, foundation, vernacular stone wall\n",
            ),
        )
        .unwrap();
        fs::write(
            output.join("manifest.json"),
            format!(
                "{{\n  \"recipe\": \"rubble-masonry\",\n  \"candidate\": 3,\n  \"hash_algorithm\": \"fnv1a64-base-rgba\",\n  \"files\": [\n{}\n  ]\n}}\n",
                manifest_entries.join(",\n")
            ),
        )
        .unwrap();
    }
}
