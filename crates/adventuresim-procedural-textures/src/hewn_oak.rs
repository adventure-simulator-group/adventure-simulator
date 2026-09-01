//! Weathered, hand-hewn structural oak with longitudinal grain and restrained adze marks.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const HEWN_OAK_TEXTURE_SIZE: u32 = 512;
pub const HEWN_OAK_TILE_METRES: f32 = 2.0;
pub const HEWN_OAK_HEIGHT_RANGE_METRES: f32 = 0.009;

const ADZE_COLUMNS: i32 = 7;
const ADZE_ROWS: i32 = 10;
const CHECK_COLUMNS: i32 = 4;
const CHECK_ROWS: i32 = 5;

#[derive(Clone, Copy, Debug)]
struct HewnOakSample {
    height: f32,
    tone: f32,
    roughness: f32,
    check: f32,
    knot: f32,
    tool_recess: f32,
}

fn hash_unit(value: u64) -> f32 {
    inclusive_unit_f32(splitmix64(value))
}

fn smooth(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn periodic_delta(value: f32) -> f32 {
    value - value.round()
}

fn grid_hash(x: i32, y: i32, cells_x: i32, cells_y: i32, salt: u64) -> f32 {
    let x = x.rem_euclid(cells_x) as u64;
    let y = y.rem_euclid(cells_y) as u64;
    hash_unit(salt ^ (x << 32) ^ y)
}

fn value_noise(u: f32, v: f32, cells_x: i32, cells_y: i32, salt: u64) -> f32 {
    let x = u.rem_euclid(1.0) * cells_x as f32;
    let y = v.rem_euclid(1.0) * cells_y as f32;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = smooth(x.fract());
    let ty = smooth(y.fract());
    let bottom = grid_hash(x0, y0, cells_x, cells_y, salt)
        + (grid_hash(x0 + 1, y0, cells_x, cells_y, salt)
            - grid_hash(x0, y0, cells_x, cells_y, salt))
            * tx;
    let top = grid_hash(x0, y0 + 1, cells_x, cells_y, salt)
        + (grid_hash(x0 + 1, y0 + 1, cells_x, cells_y, salt)
            - grid_hash(x0, y0 + 1, cells_x, cells_y, salt))
            * tx;
    bottom + (top - bottom) * ty
}

fn adze_relief(u: f32, v: f32) -> (f32, f32) {
    let column = (u * ADZE_COLUMNS as f32).floor() as i32;
    let row = (v * ADZE_ROWS as f32).floor() as i32;
    let mut closest = (f32::INFINITY, 0.0_f32);
    let mut second = (f32::INFINITY, 0.0_f32);
    for cell_y in (row - 1)..=(row + 1) {
        for cell_x in (column - 1)..=(column + 1) {
            let id = splitmix64(
                0x9b71_d453
                    ^ ((cell_x.rem_euclid(ADZE_COLUMNS) as u64) << 32)
                    ^ cell_y.rem_euclid(ADZE_ROWS) as u64,
            );
            let center_u =
                (cell_x as f32 + 0.16 + hash_unit(id ^ 0xb591) * 0.68) / ADZE_COLUMNS as f32;
            let center_v =
                (cell_y as f32 + 0.13 + hash_unit(id ^ 0x40fd) * 0.74) / ADZE_ROWS as f32;
            let angle = (hash_unit(id ^ 0x8851) - 0.5) * 0.95;
            let (sin, cos) = angle.sin_cos();
            let dx = periodic_delta(u - center_u);
            let dy = periodic_delta(v - center_v);
            let local_x = dx * cos + dy * sin;
            let local_y = -dx * sin + dy * cos;
            let cell_width = 1.0 / ADZE_COLUMNS as f32;
            let cell_length = 1.0 / ADZE_ROWS as f32;
            let distance = (local_x / cell_width).powi(2) * (0.72 + hash_unit(id ^ 0x23ab) * 0.36)
                + (local_y / cell_length).powi(2) * (0.76 + hash_unit(id ^ 0xf127) * 0.32);
            let slope_x = (hash_unit(id ^ 0x6c89) - 0.5) * 0.10;
            let slope_y = (hash_unit(id ^ 0xe459) - 0.5) * 0.14;
            let offset = (hash_unit(id ^ 0x917d) - 0.5) * 0.030;
            let plane = offset + local_x / cell_width * slope_x + local_y / cell_length * slope_y;
            if distance < closest.0 {
                second = closest;
                closest = (distance, plane);
            } else if distance < second.0 {
                second = (distance, plane);
            }
        }
    }
    let separation = (second.0.sqrt() - closest.0.sqrt()).max(0.0);
    let closest_weight = 0.5 + smooth((separation / 0.24).clamp(0.0, 1.0)) * 0.5;
    let facet = second.1 + (closest.1 - second.1) * closest_weight;
    let tool_edge = (1.0 - smooth((separation / 0.14).clamp(0.0, 1.0))) * 0.010;
    (facet, tool_edge)
}

fn check_field(u: f32, v: f32) -> f32 {
    let column = (u * CHECK_COLUMNS as f32).floor() as i32;
    let row = (v * CHECK_ROWS as f32).floor() as i32;
    let mut check = 0.0_f32;
    for cell_y in (row - 1)..=(row + 1) {
        for cell_x in (column - 1)..=(column + 1) {
            let id = splitmix64(
                0x4c2e_78a1
                    ^ ((cell_x.rem_euclid(CHECK_COLUMNS) as u64) << 32)
                    ^ cell_y.rem_euclid(CHECK_ROWS) as u64,
            );
            if hash_unit(id ^ 0x2ab7) < 0.73 {
                continue;
            }
            let center_u =
                (cell_x as f32 + 0.2 + hash_unit(id ^ 0xa447) * 0.6) / CHECK_COLUMNS as f32;
            let center_v = (cell_y as f32 + 0.2 + hash_unit(id ^ 0x77d3) * 0.6) / CHECK_ROWS as f32;
            let dx = periodic_delta(u - center_u);
            let dy = periodic_delta(v - center_v);
            let bend = (dy * 13.0 + hash_unit(id ^ 0xcb31) * 6.0).sin() * 0.0035;
            let half_width = 0.0015 + hash_unit(id ^ 0x5167) * 0.0015;
            let half_length = 0.028 + hash_unit(id ^ 0xea45) * 0.050;
            let across = ((dx - bend) / half_width).abs();
            let along = (dy / half_length).abs();
            let profile =
                smooth((1.0 - across).clamp(0.0, 1.0)) * smooth((1.0 - along).clamp(0.0, 1.0));
            check = check.max(profile);
        }
    }
    check
}

fn knot_field(u: f32, v: f32) -> f32 {
    let center_u = 0.713;
    let center_v = 0.367;
    let dx = periodic_delta(u - center_u);
    let dy = periodic_delta(v - center_v);
    let warp = value_noise(u, v, 8, 8, 0x5e91) - 0.5;
    let radius = ((dx / 0.032).powi(2) + ((dy + warp * 0.006) / 0.060).powi(2)).sqrt();
    smooth((1.0 - radius).clamp(0.0, 1.0))
}

fn sample_hewn_oak(u: f32, v: f32) -> HewnOakSample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let warp = (value_noise(u, v, 4, 7, 0x7d13) - 0.5) * 0.40;
    let grain_phase = std::f32::consts::TAU * (u * 25.0 + warp);
    let fine_phase =
        std::f32::consts::TAU * (u * 61.0 + (value_noise(u, v, 7, 13, 0x328b) - 0.5) * 0.32);
    let grain = grain_phase.sin() * 0.58 + fine_phase.sin() * 0.24;
    let broad_growth =
        (std::f32::consts::TAU * (u * 8.0 + (value_noise(u, v, 3, 5, 0x81c7) - 0.5) * 0.2)).sin();
    let timber_variation = value_noise(u, v, 6, 5, 0xd24f) - 0.5;
    let (adze_cut, adze_shoulder) = adze_relief(u, v);
    let check = check_field(u, v) * 0.18;
    let knot = knot_field(u, v) * 0.10;
    let tool_recess = ((-adze_cut - 0.038) / 0.075).clamp(0.0, 1.0);
    let height = (0.57
        + broad_growth * 0.030
        + grain * 0.018
        + timber_variation * 0.045
        + adze_cut
        + adze_shoulder
        - check * 0.28
        - knot * 0.055)
        .clamp(0.0, 1.0);
    let tone = (timber_variation * 0.62
        + broad_growth * 0.22
        + grain * 0.08
        + adze_cut * 4.5
        + adze_shoulder * 3.0
        - check * 0.85
        - knot * 0.48)
        .clamp(-1.0, 1.0);
    let roughness = (0.79 + grain.abs() * 0.035 + adze_cut.abs() * 0.50 + check * 0.10
        - knot * 0.035)
        .clamp(0.68, 0.94);
    HewnOakSample {
        height,
        tone,
        roughness,
        check,
        knot,
        tool_recess,
    }
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = HEWN_OAK_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

fn hewn_oak_color(sample: HewnOakSample) -> [u8; 3] {
    let base = [73.0_f32, 48.0, 29.0];
    let warm_shift = sample.tone * 24.0;
    let weathering = (sample.check * 12.0 + sample.knot * 8.0).min(14.0);
    [
        (base[0] + warm_shift - weathering)
            .round()
            .clamp(0.0, 255.0) as u8,
        (base[1] + warm_shift * 0.70 - weathering)
            .round()
            .clamp(0.0, 255.0) as u8,
        (base[2] + warm_shift * 0.38 - weathering * 0.65)
            .round()
            .clamp(0.0, 255.0) as u8,
    ]
}

pub fn generate_hewn_oak_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = HEWN_OAK_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_hewn_oak(
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
    let capacity = (size * size * 4) as usize;
    let mut albedo = Vec::with_capacity(capacity);
    let mut normal = Vec::with_capacity(capacity);
    let mut height = Vec::with_capacity(capacity);
    let mut arm = Vec::with_capacity(capacity);
    let metres_per_texel = HEWN_OAK_TILE_METRES / size as f32;
    let slope_scale = HEWN_OAK_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

    for y in 0..size {
        for x in 0..size {
            let sample = samples[(y * size + x) as usize];
            let color = hewn_oak_color(sample);
            albedo.extend_from_slice(&[color[0], color[1], color[2], 255]);
            let dx = height_at(&heights, x as i32 + 1, y as i32)
                - height_at(&heights, x as i32 - 1, y as i32);
            let dy = height_at(&heights, x as i32, y as i32 + 1)
                - height_at(&heights, x as i32, y as i32 - 1);
            let surface_normal = Vec3::new(-dx * slope_scale, -dy * slope_scale, 1.0).normalize();
            let encoded_normal = ((surface_normal + Vec3::ONE) * 127.5)
                .round()
                .clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[
                encoded_normal.x as u8,
                encoded_normal.y as u8,
                encoded_normal.z as u8,
                255,
            ]);
            let encoded_height = (sample.height * 255.0).round() as u8;
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            let ambient_visibility =
                (1.0 - sample.check * 0.20 - sample.knot * 0.08 - sample.tool_recess * 0.035)
                    .clamp(0.72, 1.0);
            let ao = (ambient_visibility * 255.0).round() as u8;
            let roughness = (sample.roughness * 255.0).round() as u8;
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

    use bevy::render::render_resource::TextureFormat;

    use super::*;

    fn generated() -> (Assets<Image>, SurfaceTextureSet) {
        let mut images = Assets::default();
        let textures = generate_hewn_oak_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_deterministic() {
        let (first_images, first) = generated();
        let (second_images, second) = generated();
        for (first_handle, second_handle) in [
            (&first.albedo, &second.albedo),
            (&first.normal_gl, &second.normal_gl),
            (&first.height, &second.height),
            (&first.arm, &second.arm),
        ] {
            assert_eq!(
                first_images.get(first_handle).unwrap().data,
                second_images.get(second_handle).unwrap().data
            );
        }
    }

    #[test]
    fn analytic_field_tiles_continuously() {
        let epsilon = 0.1 / HEWN_OAK_TEXTURE_SIZE as f32;
        let mut maximum_error = 0.0_f32;
        for index in 0..1_024 {
            let coordinate = (index as f32 + 0.5) / 1_024.0;
            maximum_error = maximum_error
                .max(
                    (sample_hewn_oak(epsilon, coordinate).height
                        - sample_hewn_oak(1.0 - epsilon, coordinate).height)
                        .abs(),
                )
                .max(
                    (sample_hewn_oak(coordinate, epsilon).height
                        - sample_hewn_oak(coordinate, 1.0 - epsilon).height)
                        .abs(),
                );
        }
        assert!(maximum_error < 0.025, "seam height error: {maximum_error}");
    }

    #[test]
    fn marks_have_plausible_physical_scale_and_restrained_frequency() {
        let adze_width_metres = HEWN_OAK_TILE_METRES / ADZE_COLUMNS as f32;
        let adze_length_metres = HEWN_OAK_TILE_METRES / ADZE_ROWS as f32;
        assert!((0.27..=0.30).contains(&adze_width_metres));
        assert!((0.19..=0.21).contains(&adze_length_metres));

        let mut checks = 0;
        let mut knots = 0;
        for y in 0..256 {
            for x in 0..256 {
                let sample = sample_hewn_oak((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                checks += usize::from(sample.check > 0.10);
                knots += usize::from(sample.knot > 0.055);
            }
        }
        assert!((10..=900).contains(&checks), "check pixels: {checks}");
        assert!((20..=500).contains(&knots), "knot pixels: {knots}");
    }

    #[test]
    fn channels_are_coherent_weathered_and_nonmetallic() {
        let (images, textures) = generated();
        assert_eq!(
            images
                .get(&textures.albedo)
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        let base_len = (HEWN_OAK_TEXTURE_SIZE.pow(2) * 4) as usize;
        let albedo = &images
            .get(&textures.albedo)
            .unwrap()
            .data
            .as_deref()
            .unwrap()[..base_len];
        let arm = &images.get(&textures.arm).unwrap().data.as_deref().unwrap()[..base_len];
        assert!(
            albedo
                .iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 24
        );
        assert!(
            arm.iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 6
        );
        assert!(
            arm.iter()
                .skip(1)
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 8
        );
        assert!(arm.iter().skip(2).step_by(4).all(|metallic| *metallic == 0));
    }

    #[test]
    fn every_output_has_a_complete_mip_chain() {
        let (images, textures) = generated();
        let expected_levels = HEWN_OAK_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let level_size = HEWN_OAK_TEXTURE_SIZE >> level;
                (level_size * level_size * 4) as usize
            })
            .sum::<usize>();
        for handle in [
            textures.albedo,
            textures.normal_gl,
            textures.height,
            textures.arm,
        ] {
            let image = images.get(&handle).unwrap();
            assert_eq!(image.texture_descriptor.mip_level_count, expected_levels);
            assert_eq!(image.data.as_ref().unwrap().len(), expected_bytes);
        }
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target"]
    fn export_hewn_oak_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            let image = images.get(handle).unwrap();
            image.data.as_ref().unwrap()[..(HEWN_OAK_TEXTURE_SIZE.pow(2) * 4) as usize].to_vec()
        }

        fn save_rgba(path: &Path, width: u32, height: u32, data: Vec<u8>) {
            image::save_buffer_with_format(
                path,
                &data,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
        }

        let output_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let output = output_root.join("procedural-texture-reviews/hewn-oak/candidate-6");
        let before = output.parent().unwrap().join("before");
        fs::create_dir_all(&output).unwrap();
        fs::create_dir_all(&before).unwrap();
        let (images, textures) = generated();
        let channels = [
            ("albedo", &textures.albedo),
            ("normal", &textures.normal_gl),
            ("height", &textures.height),
            ("arm", &textures.arm),
        ];
        for (name, handle) in channels {
            let data = base_rgba(&images, handle);
            save_rgba(
                &output.join(format!("hewn-oak-{name}.png")),
                HEWN_OAK_TEXTURE_SIZE,
                HEWN_OAK_TEXTURE_SIZE,
                data.clone(),
            );
            let base = ImageBuffer::<Rgba<u8>, _>::from_raw(
                HEWN_OAK_TEXTURE_SIZE,
                HEWN_OAK_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            let mut tiled = ImageBuffer::new(HEWN_OAK_TEXTURE_SIZE * 2, HEWN_OAK_TEXTURE_SIZE * 2);
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * HEWN_OAK_TEXTURE_SIZE),
                        i64::from(tile_y * HEWN_OAK_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("hewn-oak-{name}-tile-2x2.png")))
                .unwrap();
            for preview_size in [128, 64] {
                imageops::resize(
                    &base,
                    preview_size,
                    preview_size,
                    imageops::FilterType::Lanczos3,
                )
                .save(output.join(format!("hewn-oak-{name}-{preview_size}.png")))
                .unwrap();
            }
        }

        let arm = base_rgba(&images, &textures.arm);
        for (name, channel) in [("ao", 0), ("roughness", 1), ("metallic", 2)] {
            let separated = arm
                .chunks_exact(4)
                .flat_map(|pixel| {
                    let value = pixel[channel];
                    [value, value, value, 255]
                })
                .collect::<Vec<_>>();
            save_rgba(
                &output.join(format!("hewn-oak-{name}.png")),
                HEWN_OAK_TEXTURE_SIZE,
                HEWN_OAK_TEXTURE_SIZE,
                separated,
            );
        }

        let baseline = (0..HEWN_OAK_TEXTURE_SIZE)
            .flat_map(|y| {
                (0..HEWN_OAK_TEXTURE_SIZE).flat_map(move |x| {
                    if (x / 64 + y / 64) % 2 == 0 {
                        [45, 24, 13, 255]
                    } else {
                        [91, 50, 25, 255]
                    }
                })
            })
            .collect::<Vec<_>>();
        save_rgba(
            &before.join("hewn-oak-albedo.png"),
            HEWN_OAK_TEXTURE_SIZE,
            HEWN_OAK_TEXTURE_SIZE,
            baseline,
        );
    }
}
