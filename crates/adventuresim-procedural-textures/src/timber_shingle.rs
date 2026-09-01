//! Hand-split timber roof shingles, with texture V increasing down-slope.
//!
//! The recipe models a restrained, weathered softwood covering in staggered
//! courses. Longitudinal split fibres follow the roof pitch; shared irregular
//! edges and sparse tail checks avoid both modern sawn uniformity and a field
//! of inflated individual tiles.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const TIMBER_SHINGLE_TEXTURE_SIZE: u32 = 512;
pub const TIMBER_SHINGLE_TILE_METRES: f32 = 3.2;
pub const TIMBER_SHINGLE_HEIGHT_RANGE_METRES: f32 = 0.014;

const COURSES: i32 = 16;
const SHINGLES_PER_COURSE: i32 = 18;
const JOINT_HALF_WIDTH: f32 = 0.022;

#[derive(Clone, Copy, Debug)]
struct ShingleSample {
    height: f32,
    shingle_id: u64,
    contact: f32,
    fibre: f32,
    weathering: f32,
    checking: f32,
}

fn hash_unit(value: u64) -> f32 {
    inclusive_unit_f32(splitmix64(value))
}

fn shingle_id(row: i32, column: i32) -> u64 {
    splitmix64(
        0xb361_5e9d
            ^ ((row.rem_euclid(COURSES) as u64) << 32)
            ^ column.rem_euclid(SHINGLES_PER_COURSE) as u64,
    )
}

fn boundary_jitter(row: i32, boundary: i32) -> f32 {
    let id = splitmix64(
        0x713d_c5a9
            ^ ((row.rem_euclid(COURSES) as u64) << 32)
            ^ boundary.rem_euclid(SHINGLES_PER_COURSE) as u64,
    );
    (hash_unit(id) - 0.5) * 0.16
}

fn course_offset(row: i32) -> f32 {
    let row = row.rem_euclid(COURSES) as u64;
    0.10 + hash_unit(splitmix64(0x3d71_b52f ^ row)) * 0.80
}

fn edge_position(row: i32, boundary: i32, phase: f32) -> f32 {
    let base = boundary as f32 + course_offset(row) + boundary_jitter(row, boundary);
    let id = shingle_id(row, boundary);
    let lean = (hash_unit(id ^ 0x3b91) - 0.5) * 0.075 * (phase - 0.35);
    let split_wander = (phase * std::f32::consts::TAU * 1.5 + hash_unit(id ^ 0x89d7) * 6.0).sin()
        * 0.012
        * phase.powi(2);
    base + lean + split_wander
}

fn locate_shingle(u: f32, row: i32, phase: f32) -> (i32, f32, f32) {
    let scaled = u.rem_euclid(1.0) * SHINGLES_PER_COURSE as f32;
    let estimate = (scaled - course_offset(row)).floor() as i32;
    for column in (estimate - 2)..=(estimate + 2) {
        let left = edge_position(row, column, phase);
        let right = edge_position(row, column + 1, phase);
        let shifted = scaled
            + ((left - scaled) / SHINGLES_PER_COURSE as f32).round() * SHINGLES_PER_COURSE as f32;
        if shifted >= left && shifted < right {
            let width = (right - left).max(0.65);
            let local = ((shifted - left) / width).clamp(0.0, 1.0);
            return (column, local, width);
        }
    }
    (estimate, 0.5, 1.0)
}

fn split_fibres(local_x: f32, phase: f32, id: u64) -> f32 {
    let selection = hash_unit(id ^ 0xca53);
    if selection < 0.62 {
        return 0.0;
    }
    let count = 1 + usize::from(selection > 0.86) + usize::from(selection > 0.96);
    let mut relief = 0.0_f32;
    for fibre_index in 0..count {
        let salt = 0x714d_u64.wrapping_mul(fibre_index as u64 + 1);
        let center = 0.12 + hash_unit(id ^ salt) * 0.76;
        let start = 0.05 + hash_unit(id ^ salt.rotate_left(11)) * 0.48;
        let length = 0.18 + hash_unit(id ^ salt.rotate_left(23)) * 0.50;
        let end = (start + length).min(0.97);
        let along =
            ((phase - start) / 0.055).clamp(0.0, 1.0) * ((end - phase) / 0.055).clamp(0.0, 1.0);
        let curvature = (phase * std::f32::consts::TAU
            + hash_unit(id ^ salt.rotate_left(37)) * std::f32::consts::TAU)
            .sin()
            * (0.004 + hash_unit(id ^ salt.rotate_left(43)) * 0.012);
        let width = 0.005 + hash_unit(id ^ salt.rotate_left(17)) * 0.010;
        let profile = ((width - (local_x - center - curvature).abs()) / width).clamp(0.0, 1.0);
        let polarity = if hash_unit(id ^ salt.rotate_left(29)) > 0.42 {
            1.0
        } else {
            -0.65
        };
        relief += profile * profile * along * polarity;
    }
    relief.clamp(-1.0, 1.0)
}

fn tail_shape(local_x: f32, phase: f32, id: u64) -> (f32, f32) {
    let progress = ((phase - 0.70) / 0.27).clamp(0.0, 1.0);
    let class = hash_unit(id ^ 0x2f91);
    if class < 0.74 {
        return (1.0, 0.0);
    }
    if class < 0.91 {
        let inset = progress * (0.025 + hash_unit(id ^ 0xb357) * 0.040);
        let edge = (local_x - inset).min(1.0 - inset - local_x);
        let coverage = (edge / 0.025).clamp(0.0, 1.0);
        return (coverage, 1.0 - coverage);
    }
    let center = 0.42 + hash_unit(id ^ 0x83d1) * 0.16;
    let width = 0.028 + hash_unit(id ^ 0x5c47) * 0.030;
    let notch = ((width - (local_x - center).abs()) / width).clamp(0.0, 1.0) * progress;
    (1.0 - notch, notch)
}

fn check_field(local_x: f32, phase: f32, id: u64) -> f32 {
    if hash_unit(id ^ 0xe147) < 0.54 {
        return 0.0;
    }
    let anchor = 0.19 + hash_unit(id ^ 0x471b) * 0.62;
    let length = 0.18 + hash_unit(id ^ 0x921d) * 0.28;
    let tail_distance = 1.0 - phase;
    let along = (1.0 - tail_distance / length).clamp(0.0, 1.0);
    let wander = (tail_distance * 19.0 + hash_unit(id ^ 0x51c7) * 6.0).sin() * 0.010;
    let across = (local_x - anchor - wander).abs();
    let half_width = 0.011 + tail_distance * 0.025;
    ((half_width - across) / half_width.max(1.0e-5)).clamp(0.0, 1.0) * along
}

fn sample_shingles(u: f32, v: f32) -> ShingleSample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let scaled_v = v * COURSES as f32;
    let row = scaled_v.floor() as i32;
    let raw_phase = scaled_v - row as f32;
    let row_id = splitmix64(row.rem_euclid(COURSES) as u64 ^ 0xa11c);
    let course_shift = (hash_unit(row_id) - 0.5) * 0.055;
    let (provisional_column, _, _) = locate_shingle(u, row, raw_phase);
    let provisional_id = shingle_id(row, provisional_column);
    let tail_variation = (hash_unit(provisional_id ^ 0x62e5) - 0.5) * 0.105;
    let phase = (raw_phase - course_shift - tail_variation).rem_euclid(1.0);
    let (column, local_x, width) = locate_shingle(u, row, phase);
    let id = shingle_id(row, column);

    let edge_distance = local_x.min(1.0 - local_x) * width;
    let joint = ((JOINT_HALF_WIDTH - edge_distance) / JOINT_HALF_WIDTH).clamp(0.0, 1.0);
    let head_contact = ((0.105 - phase) / 0.105).clamp(0.0, 1.0);
    let tail_lip = ((phase - 0.78) / 0.18).clamp(0.0, 1.0);
    let tail_lip = tail_lip * tail_lip * (3.0 - 2.0 * tail_lip);

    let fibre = split_fibres(local_x, phase, id);
    let checking = check_field(local_x, phase, id);
    let cup = ((local_x - 0.5).powi(2) - 0.085) * ((hash_unit(id ^ 0x27bf) - 0.5) * 0.060);
    let twist = (local_x - 0.5) * (phase - 0.5) * (hash_unit(id ^ 0x74a1) - 0.5) * 0.050;
    let thickness = (hash_unit(id ^ 0x9dc3) - 0.5) * 0.030;
    let face = 0.58 + phase * 0.022 + tail_lip * 0.070 + cup + twist + thickness;
    let recessed = 0.47 + fibre * 0.006;
    let (tail_coverage, tail_contact) = tail_shape(local_x, phase, id);
    let coverage = (1.0 - joint).powi(2) * tail_coverage;
    let height = recessed + (face + fibre * 0.011 - checking * 0.075 - recessed) * coverage;

    let exposure =
        (phase * 0.70 + (1.0 - edge_distance * 1.8).clamp(0.0, 1.0) * 0.16).clamp(0.0, 1.0);
    let weathering =
        (exposure + (hash_unit(id ^ 0xf24d) - 0.5) * 0.30 + fibre.max(0.0) * 0.08).clamp(0.0, 1.0);
    let contact = (joint * 0.76
        + head_contact * (1.0 - joint) * 0.30
        + checking * 0.28
        + tail_contact * 0.34)
        .clamp(0.0, 1.0);

    ShingleSample {
        height,
        shingle_id: id,
        contact,
        fibre,
        weathering,
        checking,
    }
}

fn color_and_roughness(sample: ShingleSample) -> ([u8; 3], u8) {
    let piece = hash_unit(sample.shingle_id ^ 0xd517) - 0.5;
    let fibre_shift = sample.fibre * 5.2;
    let sun_grey = sample.weathering * 17.0;
    let check_darkening = sample.checking * 24.0;
    let base = [105.0, 82.0, 57.0];
    let color = [
        base[0] + piece * 10.0 + fibre_shift - sun_grey - check_darkening,
        base[1] + piece * 8.0 + fibre_shift * 0.75 - sun_grey * 0.78 - check_darkening,
        base[2] + piece * 6.0 + fibre_shift * 0.55 - sun_grey * 0.50 - check_darkening,
    ];
    let roughness = (211.0 + sample.weathering * 23.0 + sample.checking * 8.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    (
        [
            color[0].round().clamp(0.0, 255.0) as u8,
            color[1].round().clamp(0.0, 255.0) as u8,
            color[2].round().clamp(0.0, 255.0) as u8,
        ],
        roughness,
    )
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = TIMBER_SHINGLE_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_timber_shingle_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = TIMBER_SHINGLE_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_shingles(
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
    let metres_per_texel = TIMBER_SHINGLE_TILE_METRES / size as f32;
    let slope_scale = TIMBER_SHINGLE_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

    for y in 0..size {
        for x in 0..size {
            let sample = samples[(y * size + x) as usize];
            let (color, roughness) = color_and_roughness(sample);
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
            let h = (sample.height * 255.0).round().clamp(0.0, 255.0) as u8;
            height.extend_from_slice(&[h, h, h, 255]);
            let ao = ((1.0 - sample.contact * 0.47) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
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
        let textures = generate_timber_shingle_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_deterministic_and_analytic_field_is_periodic() {
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
        for index in 0..512 {
            let coordinate = (index as f32 + 0.5) / 512.0;
            let sample = sample_shingles(coordinate, coordinate * 0.73);
            assert_eq!(
                sample.height.to_bits(),
                sample_shingles(coordinate + 1.0, coordinate * 0.73)
                    .height
                    .to_bits()
            );
            assert!(
                (sample.height - sample_shingles(coordinate, coordinate * 0.73 + 1.0).height).abs()
                    < 1.0e-5
            );
        }
    }

    #[test]
    fn physical_scale_and_direction_match_split_shingle_covering() {
        let nominal_width = TIMBER_SHINGLE_TILE_METRES / SHINGLES_PER_COURSE as f32;
        let course_exposure = TIMBER_SHINGLE_TILE_METRES / COURSES as f32;
        assert!((0.15..=0.20).contains(&nominal_width));
        assert!((0.18..=0.23).contains(&course_exposure));
        assert!((0.010..=0.018).contains(&TIMBER_SHINGLE_HEIGHT_RANGE_METRES));
        let upper = sample_shingles(0.43, 0.015).height;
        let lower = sample_shingles(0.43, 0.078).height;
        assert!(
            lower > upper,
            "visible course should rise toward its lower lip"
        );
    }

    #[test]
    fn courses_are_staggered_and_width_variation_is_restrained() {
        let positions = (0..4)
            .map(|row| edge_position(row, 0, 0.5).rem_euclid(1.0).to_bits())
            .collect::<BTreeSet<_>>();
        assert_eq!(positions.len(), 4);
        for row in 0..COURSES {
            for column in 0..SHINGLES_PER_COURSE {
                let width = edge_position(row, column + 1, 0.5) - edge_position(row, column, 0.5);
                assert!((0.72..=1.28).contains(&width), "shingle width: {width}");
            }
        }
        assert!(
            (COURSES * SHINGLES_PER_COURSE) >= 256,
            "repeat sequence must contain at least 256 distinct shingles"
        );
    }

    #[test]
    fn split_fibres_are_sparse_finite_and_leave_quiet_faces() {
        let mut quiet = 0;
        let mut active = 0;
        for row in 0..COURSES {
            for column in 0..SHINGLES_PER_COURSE {
                let id = shingle_id(row, column);
                let maximum = (0..64)
                    .flat_map(|y| {
                        (0..24).map(move |x| {
                            split_fibres((x as f32 + 0.5) / 24.0, (y as f32 + 0.5) / 64.0, id).abs()
                        })
                    })
                    .fold(0.0_f32, f32::max);
                if maximum < 0.01 {
                    quiet += 1;
                } else {
                    active += 1;
                    assert_eq!(split_fibres(0.5, 0.0, id), 0.0);
                    assert_eq!(split_fibres(0.5, 1.0, id), 0.0);
                }
            }
        }
        assert!(quiet > active, "quiet={quiet}, active={active}");
        assert!(active > 70, "active fibre shingles: {active}");
    }

    #[test]
    fn tail_classes_are_restrained_but_include_tapers_and_notches() {
        let mut square = 0;
        let mut altered = 0;
        for row in 0..COURSES {
            for column in 0..SHINGLES_PER_COURSE {
                let id = shingle_id(row, column);
                let minimum_coverage = (0..32)
                    .map(|sample| tail_shape((sample as f32 + 0.5) / 32.0, 0.98, id).0)
                    .fold(1.0_f32, f32::min);
                if minimum_coverage >= 0.99 {
                    square += 1;
                } else {
                    altered += 1;
                }
            }
        }
        assert!(square > 190, "square tails: {square}");
        assert!((45..=95).contains(&altered), "altered tails: {altered}");
    }

    #[test]
    fn checks_are_localized_and_contacts_are_recessed_without_open_holes() {
        let mut checked = 0;
        let mut deep_contacts = 0;
        let mut minimum_height = f32::INFINITY;
        for y in 0..256 {
            for x in 0..256 {
                let sample = sample_shingles((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                checked += usize::from(sample.checking > 0.20);
                deep_contacts += usize::from(sample.contact > 0.55);
                minimum_height = minimum_height.min(sample.height);
            }
        }
        assert!(
            (100..=4_500).contains(&checked),
            "checked samples: {checked}"
        );
        assert!(deep_contacts > 500, "deep contact samples: {deep_contacts}");
        assert!(minimum_height > 0.35, "minimum height: {minimum_height}");
    }

    #[test]
    fn channels_are_nonmetallic_varied_and_have_complete_mips() {
        let (images, textures) = generated();
        let expected_levels = TIMBER_SHINGLE_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let level_size = TIMBER_SHINGLE_TEXTURE_SIZE >> level;
                (level_size * level_size * 4) as usize
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
        let base = &arm[..(TIMBER_SHINGLE_TEXTURE_SIZE.pow(2) * 4) as usize];
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
                > 12
        );
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target"]
    fn export_timber_shingle_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(TIMBER_SHINGLE_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn save_rgba(path: &Path, data: &[u8], width: u32, height: u32) {
            image::save_buffer_with_format(
                path,
                data,
                width,
                height,
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

        fn save_scales(output: &Path, name: &str, data: Vec<u8>) {
            let base = ImageBuffer::<Rgba<u8>, _>::from_raw(
                TIMBER_SHINGLE_TEXTURE_SIZE,
                TIMBER_SHINGLE_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            base.save(output.join(format!("timber-shingle-{name}.png")))
                .unwrap();
            let mut tiled = ImageBuffer::new(
                TIMBER_SHINGLE_TEXTURE_SIZE * 2,
                TIMBER_SHINGLE_TEXTURE_SIZE * 2,
            );
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * TIMBER_SHINGLE_TEXTURE_SIZE),
                        i64::from(tile_y * TIMBER_SHINGLE_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("timber-shingle-{name}-tile-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&base, size, size, imageops::FilterType::Lanczos3)
                    .save(output.join(format!("timber-shingle-{name}-{size}.png")))
                    .unwrap();
            }
        }

        let review_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/procedural-texture-reviews/timber-shingle");
        let output = review_root.join("candidate-3");
        let before = review_root.join("before");
        fs::create_dir_all(&output).unwrap();
        fs::create_dir_all(&before).unwrap();
        let baseline = vec![92_u8, 72, 51, 255]
            .into_iter()
            .cycle()
            .take((TIMBER_SHINGLE_TEXTURE_SIZE.pow(2) * 4) as usize)
            .collect::<Vec<_>>();
        save_rgba(
            &before.join("timber-shingle-planned-baseline.png"),
            &baseline,
            TIMBER_SHINGLE_TEXTURE_SIZE,
            TIMBER_SHINGLE_TEXTURE_SIZE,
        );
        fs::write(
            before.join("provenance.txt"),
            concat!(
                "recipe=timber-shingle\n",
                "state=planned baseline\n",
                "description=uniform brown material swatch; no procedural shingle recipe\n",
            ),
        )
        .unwrap();
        let baseline_bytes = fs::read(before.join("timber-shingle-planned-baseline.png")).unwrap();
        fs::write(
            before.join("manifest.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"schema\": 1,\n",
                    "  \"recipe\": \"timber-shingle\",\n",
                    "  \"state\": \"planned-baseline\",\n",
                    "  \"files\": [{{\"file\":\"timber-shingle-planned-baseline.png\",\"bytes\":{},\"fnv1a64\":\"{:016x}\"}}]\n",
                    "}}\n"
                ),
                baseline_bytes.len(),
                fnv1a64(&baseline_bytes)
            ),
        )
        .unwrap();

        let (images, textures) = generated();
        let albedo = base_rgba(&images, &textures.albedo);
        let normal = base_rgba(&images, &textures.normal_gl);
        let height = base_rgba(&images, &textures.height);
        let arm = base_rgba(&images, &textures.arm);
        for (name, data) in [
            ("albedo", albedo.clone()),
            ("normal", normal.clone()),
            ("height", height.clone()),
            ("arm", arm.clone()),
        ] {
            save_scales(&output, name, data);
        }

        for (name, channel) in [("ao", 0), ("roughness", 1), ("metallic", 2)] {
            let separated = arm
                .chunks_exact(4)
                .flat_map(|pixel| {
                    let value = pixel[channel];
                    [value, value, value, 255]
                })
                .collect::<Vec<_>>();
            save_scales(&output, name, separated);
        }

        let interpreted = albedo
            .chunks_exact(4)
            .zip(normal.chunks_exact(4))
            .zip(arm.chunks_exact(4))
            .flat_map(|((color, normal), arm)| {
                let nx = normal[0] as f32 / 127.5 - 1.0;
                let ny = normal[1] as f32 / 127.5 - 1.0;
                let nz = normal[2] as f32 / 127.5 - 1.0;
                let light = (nx * -0.38 + ny * -0.47 + nz * 0.80).clamp(0.0, 1.0);
                let shade = (0.30 + light * 0.70) * (arm[0] as f32 / 255.0);
                [
                    (color[0] as f32 * shade).round() as u8,
                    (color[1] as f32 * shade).round() as u8,
                    (color[2] as f32 * shade).round() as u8,
                    255,
                ]
            })
            .collect::<Vec<_>>();
        save_scales(&output, "interpreted", interpreted);

        fs::write(
            output.join("provenance.txt"),
            concat!(
                "recipe=timber-shingle\n",
                "candidate=3\n",
                "source=deterministic analytic hand-split shingle courses; no external imagery\n",
                "historical_scope=weathered softwood roof or secondary-structure covering, circa 1544 Germany\n",
                "orientation=texture V increases down-slope\n",
                "tile_metres=3.2\n",
                "texture_size=512\n",
                "course_count=16\n",
                "shingles_per_course=18\n",
                "height_range_metres=0.014\n",
                "arm_packing=R ambient visibility, G perceptual roughness, B metallic\n",
                "metallicity=0\n",
                "fixture=frozen orthographic material sheet with upper-left grazing light\n",
                "seed=recipe constants and fabelgeist splitmix64\n",
                "review_history=candidate 2 received one ACCEPT and one contradictory material-defect REJECT; candidate 2 is not accepted\n",
                "candidate_3_status=awaiting one manager-assigned independent reviewer; implementer has not accepted it\n",
            ),
        )
        .unwrap();
        fs::write(
            output.join("review-disagreement.txt"),
            concat!(
                "candidate-2 disposition: NOT ACCEPTED\n",
                "reviewer A: ACCEPT; found overlap, scale, channel coherence and mip identity satisfactory\n",
                "reviewer B: REJECT; found universal full-length evenly spaced ribs, modern uniform tails, and a short 14x12 repeat\n",
                "manager disposition: reviewer B defects are material and candidate 3 must receive one manager-assigned independent review\n",
                "candidate-3 implementer disposition: no self-acceptance\n",
            ),
        )
        .unwrap();

        let mut files = fs::read_dir(&output)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file() && path.file_name().unwrap() != "manifest.json")
            .collect::<Vec<_>>();
        files.sort();
        let entries = files
            .iter()
            .map(|path| {
                let bytes = fs::read(path).unwrap();
                format!(
                    "    {{\"file\":\"{}\",\"bytes\":{},\"fnv1a64\":\"{:016x}\"}}",
                    path.file_name().unwrap().to_string_lossy(),
                    bytes.len(),
                    fnv1a64(&bytes)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        fs::write(
            output.join("manifest.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"schema\": 1,\n",
                    "  \"recipe\": \"timber-shingle\",\n",
                    "  \"candidate\": \"candidate-3\",\n",
                    "  \"revision\": \"544b7a50 with disclosed dirty worktree\",\n",
                    "  \"command\": \"cargo test -p adventuresim-procedural-textures timber_shingle::tests::export_timber_shingle_visual_review -- --ignored --exact\",\n",
                    "  \"dimensions\": {{\"full\": [512,512], \"tile_2x2\": [1024,1024], \"reductions\": [128,64]}},\n",
                    "  \"files\": [\n{}\n  ]\n",
                    "}}\n"
                ),
                entries
            ),
        )
        .unwrap();
    }
}
