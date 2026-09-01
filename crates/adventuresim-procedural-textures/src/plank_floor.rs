//! Broad, hand-sawn oak floorboards for an early-modern urban interior.
//!
//! The recipe assumes unfinished or lightly waxed local oak laid over joists.
//! Boards run along texture V; butt joints and the few visible forged nails are
//! constrained to an implicit 0.6 m joist spacing.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const PLANK_FLOOR_TEXTURE_SIZE: u32 = 1024;
pub const PLANK_FLOOR_TILE_METRES: f32 = 7.2;
pub const PLANK_FLOOR_HEIGHT_RANGE_METRES: f32 = 0.010;

const BOARD_COUNT: i32 = 22;
const JOIST_STATIONS: i32 = 12;
const EDGE_GAP_METRES: f32 = 0.006;
const END_GAP_METRES: f32 = 0.005;

#[derive(Clone, Copy, Debug)]
struct PlankSample {
    height: f32,
    tone: f32,
    roughness: f32,
    ao: f32,
    joint: f32,
    nail: f32,
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

fn board_weight(board: i32) -> f32 {
    0.82 + hash_unit(0x8a31_f64d ^ board.rem_euclid(BOARD_COUNT) as u64) * 0.36
}

fn board_boundary(boundary: i32) -> f32 {
    let boundary = boundary.clamp(0, BOARD_COUNT);
    let total = (0..BOARD_COUNT).map(board_weight).sum::<f32>();
    (0..boundary).map(board_weight).sum::<f32>() / total
}

fn edge_warp(boundary: i32, v: f32) -> f32 {
    let id = boundary.rem_euclid(BOARD_COUNT) as u64;
    let first_phase = hash_unit(0x2b7d_91a3 ^ id) * std::f32::consts::TAU;
    let second_phase = hash_unit(0x913c_44e7 ^ id) * std::f32::consts::TAU;
    let amplitude_metres = 0.0018 + hash_unit(0xaf67_205b ^ id) * 0.0024;
    let amplitude = amplitude_metres / PLANK_FLOOR_TILE_METRES;
    amplitude
        * ((std::f32::consts::TAU * v + first_phase).sin()
            + 0.45 * (std::f32::consts::TAU * 3.0 * v + second_phase).sin())
}

fn butt_joint_station(board: i32) -> i32 {
    let jitter = (splitmix64(0xe473_b51f ^ board as u64) % 3) as i32;
    (board * 5 + jitter).rem_euclid(JOIST_STATIONS)
}

fn growth_field(local_u: f32, v: f32, board_id: u64) -> (f32, f32) {
    let phase_offset = hash_unit(0x514c_a93b ^ board_id);
    let slow_warp = value_noise_1d(v, 3 + (board_id % 3) as i32, 0x643a_11e9 ^ board_id) - 0.5;
    let mode = splitmix64(0x31a8_f75d ^ board_id) % 5;
    let (coordinate, contrast, latewood_power) = match mode {
        0 | 1 => {
            // Rift-sawn faces: a few long, gently wandering growth bands.
            (
                local_u * (1.8 + hash_unit(0x7d91_38af ^ board_id) * 1.8)
                    + slow_warp * (0.18 + hash_unit(0xf26c_509d ^ board_id) * 0.20),
                0.72,
                4.0,
            )
        }
        2 | 3 => {
            // Flat-sawn faces: broad parabolic/cathedral sweeps rather than
            // a repeated stack of straight pinstripes.
            let center = 0.34 + hash_unit(0x5c48_b73f ^ board_id) * 0.32;
            let along = periodic_delta(v - hash_unit(0x8651_d24b ^ board_id));
            let across = (local_u - center) * 1.45;
            let radial = (across * across + (along * 0.34).powi(2)).sqrt();
            (
                radial * (4.5 + hash_unit(0x43fb_216e ^ board_id) * 2.5) + slow_warp * 0.13,
                0.88,
                4.5,
            )
        }
        _ => {
            // A deliberately quiet face with only broad, oblique structure.
            (
                local_u * (0.8 + hash_unit(0x238b_7f51 ^ board_id) * 0.8)
                    + v * (0.08 + hash_unit(0xa371_09cd ^ board_id) * 0.10)
                    + slow_warp * 0.10,
                0.30,
                3.0,
            )
        }
    };
    let phase = std::f32::consts::TAU * (coordinate + phase_offset);
    let broad = phase.sin();
    let latewood = ((phase.sin() + 1.0) * 0.5).powf(latewood_power);
    let secondary = (phase * 0.47 + slow_warp * 2.2).sin();
    let tone = (broad * 0.24 - latewood * 0.30 + secondary * 0.07) * contrast;
    let relief = (broad * 0.005 - latewood * 0.009) * contrast;
    (tone, relief)
}

fn finite_surface_features(u: f32, v: f32, board: i32, left: f32, right: f32) -> (f32, f32, f32) {
    let id = board as u64;
    let board_width = right - left;
    let local_u = ((u - left) / board_width).clamp(0.0, 1.0);
    let mut check = 0.0_f32;
    if hash_unit(0xa813_5f4d ^ id) > 0.70 {
        let center_u = 0.18 + hash_unit(0x749b_c2e1 ^ id) * 0.64;
        let center_v = hash_unit(0x19e4_8b73 ^ id);
        let dx = (local_u - center_u + periodic_delta(v - center_v) * 0.035).abs();
        let dy = periodic_delta(v - center_v).abs();
        let width = 0.006 + hash_unit(0xd457_2ca1 ^ id) * 0.006;
        let half_length = (0.06 + hash_unit(0x61af_839d ^ id) * 0.09) / PLANK_FLOOR_TILE_METRES;
        check = (1.0 - smooth((dx / width).clamp(0.0, 1.0)))
            * (1.0 - smooth((dy / half_length).clamp(0.0, 1.0)));
    }

    let mut hand_mark = 0.0_f32;
    if hash_unit(0x93c1_6e5b ^ id) > 0.62 {
        for mark in 0..2_u64 {
            let center_u = 0.14 + hash_unit(0x2f75_8c19 ^ id ^ mark) * 0.72;
            let center_v = hash_unit(0xc486_31ad ^ id ^ mark);
            let dx = (local_u - center_u) / (0.10 + hash_unit(0xa59d_17e3 ^ id ^ mark) * 0.08);
            let dy = periodic_delta(v - center_v)
                / ((0.10 + hash_unit(0x7db2_e451 ^ id ^ mark) * 0.16) / PLANK_FLOOR_TILE_METRES);
            let radius = dx * dx + dy * dy;
            if radius < 1.0 {
                let facet = (1.0 - smooth(radius))
                    * (dx * 0.45 + dy * 0.08)
                    * (if mark == 0 { 1.0 } else { -1.0 });
                hand_mark += facet * 0.018;
            }
        }
    }

    // Tiny open pores occur on a minority of boards and remain subordinate to
    // the broad growth field.
    let pore_phase =
        std::f32::consts::TAU * (local_u * (31.0 + (id % 5) as f32) + v * (7.0 + (id % 3) as f32));
    let pore = if hash_unit(0xb4d7_52a9 ^ id) > 0.48 {
        ((pore_phase.sin() - 0.94) / 0.06).clamp(0.0, 1.0)
            * ((std::f32::consts::TAU * (v * 19.0 + hash_unit(id))).sin() - 0.72).clamp(0.0, 0.28)
            / 0.28
    } else {
        0.0
    };
    (check, hand_mark, pore)
}

fn board_at(u: f32) -> (i32, f32, f32) {
    let u = u.rem_euclid(1.0);
    for board in 0..BOARD_COUNT {
        let left = board_boundary(board);
        let right = board_boundary(board + 1);
        if u < right {
            return (board, left, right);
        }
    }
    (BOARD_COUNT - 1, board_boundary(BOARD_COUNT - 1), 1.0)
}

fn value_noise_1d(value: f32, cells: i32, salt: u64) -> f32 {
    let scaled = value.rem_euclid(1.0) * cells as f32;
    let first = scaled.floor() as i32;
    let blend = smooth(scaled.fract());
    let sample = |index: i32| hash_unit(salt ^ index.rem_euclid(cells) as u64);
    sample(first) + (sample(first + 1) - sample(first)) * blend
}

fn joint_profile(distance_metres: f32, half_width_metres: f32) -> f32 {
    let feather = PLANK_FLOOR_TILE_METRES / PLANK_FLOOR_TEXTURE_SIZE as f32 * 0.85;
    1.0 - smooth(((distance_metres - half_width_metres) / feather).clamp(0.0, 1.0))
}

fn nail_profile(u: f32, v: f32, board: i32, left: f32, right: f32, station: i32) -> f32 {
    let board_width = right - left;
    let joint_v = station as f32 / JOIST_STATIONS as f32;
    let mut nail = 0.0_f32;
    let id = board.rem_euclid(BOARD_COUNT) as u64;
    if hash_unit(0x9a72_4cd1 ^ id) > 0.38 {
        for side in [0.12_f32, 0.88] {
            let center_u = left + board_width * side;
            let dx = periodic_delta(u - center_u) * PLANK_FLOOR_TILE_METRES;
            let dy = periodic_delta(v - joint_v) * PLANK_FLOOR_TILE_METRES;
            let radius = (dx * dx + dy * dy).sqrt();
            nail = nail.max(1.0 - smooth((radius / 0.009).clamp(0.0, 1.0)));
        }
    }

    // Occasional surviving face nails at another supporting joist. These are
    // construction-related, not a decorative grid across every board.
    if hash_unit(0xc173_4d2f ^ id) > 0.84 {
        let support = (station + 3) % JOIST_STATIONS;
        let side = if hash_unit(0x48d2_71a5 ^ id) > 0.5 {
            0.14
        } else {
            0.86
        };
        let center_u = left + board_width * side;
        let center_v = support as f32 / JOIST_STATIONS as f32;
        let dx = periodic_delta(u - center_u) * PLANK_FLOOR_TILE_METRES;
        let dy = periodic_delta(v - center_v) * PLANK_FLOOR_TILE_METRES;
        let radius = (dx * dx + dy * dy).sqrt();
        nail = nail.max(1.0 - smooth((radius / 0.005).clamp(0.0, 1.0)));
    }
    nail
}

fn sample_plank_floor(u: f32, v: f32) -> PlankSample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let (board, left, right) = board_at(u);
    let board_id = board as u64;
    let board_width = right - left;
    let local_u = ((u - left) / board_width).clamp(0.0, 1.0);

    let left_edge = left + edge_warp(board, v);
    let right_edge = right + edge_warp(board + 1, v);
    let edge_distance = periodic_delta(u - left_edge)
        .abs()
        .min(periodic_delta(u - right_edge).abs())
        * PLANK_FLOOR_TILE_METRES;
    let edge_joint = joint_profile(edge_distance, EDGE_GAP_METRES * 0.5);

    let station = butt_joint_station(board);
    let cut_skew_metres = (hash_unit(0x273b_91f5 ^ board_id) - 0.5) * 0.010;
    let cut_offset = (local_u - 0.5) * cut_skew_metres / PLANK_FLOOR_TILE_METRES;
    let end_distance = periodic_delta(v - station as f32 / JOIST_STATIONS as f32 - cut_offset)
        .abs()
        * PLANK_FLOOR_TILE_METRES;
    let end_joint = joint_profile(end_distance, END_GAP_METRES * 0.5);
    let end_inside_board = smooth((local_u / 0.035).clamp(0.0, 1.0))
        * smooth(((1.0 - local_u) / 0.035).clamp(0.0, 1.0));
    let end_joint = end_joint * end_inside_board;
    let joint = edge_joint.max(end_joint);
    let nail = nail_profile(u, v, board, left, right, station);

    let (grain_tone, grain_relief) = growth_field(local_u, v, board_id);
    let (check, hand_mark, pore) = finite_surface_features(u, v, board, left, right);
    let broad_length = value_noise_1d(v, 4, 0x159a_e271 ^ board_id) - 0.5;
    let board_tone = hash_unit(0x11bd_7a35 ^ board_id) - 0.5;

    let cup_direction = if hash_unit(0x7531_ac49 ^ board_id) > 0.5 {
        1.0
    } else {
        -1.0
    };
    let cup = ((local_u - 0.5).powi(2) * 4.0 - 0.34)
        * cup_direction
        * (0.010 + hash_unit(0x43e9_65b1 ^ board_id) * 0.010);
    let traffic_center = 0.56
        + 0.055 * (std::f32::consts::TAU * v).sin()
        + 0.025 * (std::f32::consts::TAU * 3.0 * v + 1.4).sin();
    let traffic_distance = periodic_delta(u - traffic_center).abs();
    let wear = smooth((1.0 - traffic_distance / 0.17).clamp(0.0, 1.0))
        * (0.68 + value_noise_1d(v, 5, 0xb582_08cd) * 0.32);

    let height = (0.66 + cup + grain_relief + broad_length * 0.018 + hand_mark
        - check * 0.16
        - pore * 0.035
        - joint * 0.53
        - nail * 0.16)
        .clamp(0.0, 1.0);
    let tone = (board_tone * 0.42 + grain_tone * 0.34 + broad_length * 0.18
        - check * 0.26
        - pore * 0.16
        - joint * 0.60
        - nail * 0.34)
        .clamp(-1.0, 1.0);
    let roughness = (0.77 + grain_tone.abs() * 0.018 + joint * 0.12 + check * 0.08 + pore * 0.05
        - wear * 0.15)
        .clamp(0.54, 0.94);
    let ao =
        (1.0 - joint * (0.46 + (1.0 - wear) * 0.05) - nail * 0.24 - check * 0.08).clamp(0.48, 1.0);

    PlankSample {
        height,
        tone,
        roughness,
        ao,
        joint,
        nail,
    }
}

fn plank_color(sample: PlankSample) -> [u8; 3] {
    let base = [104.0_f32, 75.0, 45.0];
    let shift = sample.tone * 40.0;
    let embedded_dirt = sample.joint * 16.0 + sample.nail * 10.0;
    [
        (base[0] + shift - embedded_dirt).clamp(0.0, 255.0) as u8,
        (base[1] + shift * 0.72 - embedded_dirt).clamp(0.0, 255.0) as u8,
        (base[2] + shift * 0.42 - embedded_dirt * 0.72).clamp(0.0, 255.0) as u8,
    ]
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = PLANK_FLOOR_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_plank_floor_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = PLANK_FLOOR_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_plank_floor(
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
    let metres_per_texel = PLANK_FLOOR_TILE_METRES / size as f32;
    let slope_scale = PLANK_FLOOR_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) as usize;
            let sample = samples[index];
            let color = plank_color(sample);
            albedo.extend_from_slice(&[color[0], color[1], color[2], 255]);
            let dx = height_at(&heights, x as i32 + 1, y as i32)
                - height_at(&heights, x as i32 - 1, y as i32);
            let dy = height_at(&heights, x as i32, y as i32 + 1)
                - height_at(&heights, x as i32, y as i32 - 1);
            let surface_normal = Vec3::new(-dx * slope_scale, -dy * slope_scale, 1.0).normalize();
            let encoded = ((surface_normal + Vec3::ONE) * 127.5)
                .round()
                .clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]);
            let encoded_height = (sample.height * 255.0).round() as u8;
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            arm.extend_from_slice(&[
                (sample.ao * 255.0).round() as u8,
                (sample.roughness * 255.0).round() as u8,
                0,
                255,
            ]);
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
        let textures = generate_plank_floor_textures(&mut images);
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
    fn field_is_periodic_in_both_directions() {
        for (u, v) in [(0.031, 0.17), (0.28, 0.51), (0.63, 0.87), (0.94, 0.39)] {
            let sample = sample_plank_floor(u, v);
            assert!((sample.height - sample_plank_floor(u + 1.0, v).height).abs() < 1.0e-6);
            assert!((sample.height - sample_plank_floor(u, v - 1.0).height).abs() < 1.0e-6);
            assert!((sample.tone - sample_plank_floor(u + 1.0, v + 1.0).tone).abs() < 1.0e-5);
        }
    }

    #[test]
    fn boards_are_broad_and_joints_follow_joists() {
        let widths = (0..BOARD_COUNT)
            .map(|board| {
                (board_boundary(board + 1) - board_boundary(board)) * PLANK_FLOOR_TILE_METRES
            })
            .collect::<Vec<_>>();
        assert!(
            widths.iter().all(|width| (0.25..=0.40).contains(width)),
            "widths: {widths:?}"
        );
        for board in 0..BOARD_COUNT {
            let station = butt_joint_station(board);
            let metres = station as f32 * PLANK_FLOOR_TILE_METRES / JOIST_STATIONS as f32;
            assert!((metres / 0.6 - (metres / 0.6).round()).abs() < 1.0e-5);
            let next = butt_joint_station((board + 1).rem_euclid(BOARD_COUNT));
            assert_ne!(
                station,
                next,
                "adjacent boards {board} and {} cluster",
                board + 1
            );
        }
    }

    #[test]
    fn gaps_and_fasteners_are_sparse_and_physically_scaled() {
        assert!((0.004..=0.008).contains(&EDGE_GAP_METRES));
        assert!((0.003..=0.007).contains(&END_GAP_METRES));
        let mut joint = 0_usize;
        let mut nail = 0_usize;
        for y in 0..256 {
            for x in 0..256 {
                let sample = sample_plank_floor((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                joint += usize::from(sample.joint > 0.5);
                nail += usize::from(sample.nail > 0.5);
            }
        }
        let texels = 256 * 256;
        assert!((joint * 100 / texels) < 8, "joint texels: {joint}");
        assert!((nail * 10_000 / texels) < 20, "nail texels: {nail}");
    }

    #[test]
    fn channels_are_detailed_complete_and_nonmetallic() {
        let (images, textures) = generated();
        let expected_levels = PLANK_FLOOR_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let level_size = PLANK_FLOOR_TEXTURE_SIZE >> level;
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
        let base = &arm[..(PLANK_FLOOR_TEXTURE_SIZE.pow(2) * 4) as usize];
        assert!(base.iter().skip(2).step_by(4).all(|value| *value == 0));
        assert!(
            base.iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 20
        );
        assert!(
            base.iter()
                .skip(1)
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 18
        );
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target"]
    fn export_plank_floor_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(PLANK_FLOOR_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn save_scales(output: &Path, name: &str, data: Vec<u8>) {
            let base = ImageBuffer::<Rgba<u8>, _>::from_raw(
                PLANK_FLOOR_TEXTURE_SIZE,
                PLANK_FLOOR_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            base.save(output.join(format!("plank-floor-{name}-full.png")))
                .unwrap();
            let mut tiled =
                ImageBuffer::new(PLANK_FLOOR_TEXTURE_SIZE * 2, PLANK_FLOOR_TEXTURE_SIZE * 2);
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * PLANK_FLOOR_TEXTURE_SIZE),
                        i64::from(tile_y * PLANK_FLOOR_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("plank-floor-{name}-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&base, size, size, imageops::FilterType::Lanczos3)
                    .save(output.join(format!("plank-floor-{name}-{size}.png")))
                    .unwrap();
            }
        }

        fn separate(data: &[u8], channel: usize) -> Vec<u8> {
            data.chunks_exact(4)
                .flat_map(|pixel| {
                    let value = pixel[channel];
                    [value, value, value, 255]
                })
                .collect()
        }

        fn interpreted(albedo: &[u8], normal: &[u8], arm: &[u8]) -> Vec<u8> {
            albedo
                .chunks_exact(4)
                .zip(normal.chunks_exact(4))
                .zip(arm.chunks_exact(4))
                .flat_map(|((color, normal), arm)| {
                    let nx = normal[0] as f32 / 127.5 - 1.0;
                    let ny = normal[1] as f32 / 127.5 - 1.0;
                    let nz = normal[2] as f32 / 127.5 - 1.0;
                    let light = (nx * -0.36 + ny * -0.48 + nz * 0.80).clamp(0.0, 1.0);
                    let shade = (0.34 + light * 0.66) * (arm[0] as f32 / 255.0);
                    [
                        (color[0] as f32 * shade).round() as u8,
                        (color[1] as f32 * shade).round() as u8,
                        (color[2] as f32 * shade).round() as u8,
                        255,
                    ]
                })
                .collect()
        }

        fn fnv1a64(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
            })
        }

        fn write_manifest(output: &Path, state: &str) {
            let mut files = fs::read_dir(output)
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
                        "  \"recipe\": \"plank-floor\",\n",
                        "  \"state\": \"{}\",\n",
                        "  \"dimensions\": {{\"full\": [1024,1024], \"tile_2x2\": [2048,2048], \"reductions\": [128,64]}},\n",
                        "  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n",
                        "  \"files\": [\n{}\n  ]\n",
                        "}}\n"
                    ),
                    state, entries
                ),
            )
            .unwrap();
        }

        let review_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/procedural-texture-reviews/plank-floor");
        let baseline = review_root.join("baseline-planned");
        let candidate = review_root.join("candidate-3");
        if baseline.exists() {
            fs::remove_dir_all(&baseline).unwrap();
        }
        if candidate.exists() {
            fs::remove_dir_all(&candidate).unwrap();
        }
        fs::create_dir_all(&baseline).unwrap();
        fs::create_dir_all(&candidate).unwrap();

        let pixels = PLANK_FLOOR_TEXTURE_SIZE.pow(2) as usize;
        let baseline_albedo = [91_u8, 70, 49, 255]
            .into_iter()
            .cycle()
            .take(pixels * 4)
            .collect::<Vec<_>>();
        let baseline_normal = [128_u8, 128, 255, 255]
            .into_iter()
            .cycle()
            .take(pixels * 4)
            .collect::<Vec<_>>();
        let baseline_height = [128_u8, 128, 128, 255]
            .into_iter()
            .cycle()
            .take(pixels * 4)
            .collect::<Vec<_>>();
        let baseline_arm = [255_u8, 210, 0, 255]
            .into_iter()
            .cycle()
            .take(pixels * 4)
            .collect::<Vec<_>>();
        for (name, data) in [
            ("albedo", baseline_albedo.clone()),
            ("normal", baseline_normal.clone()),
            ("height", baseline_height.clone()),
            ("arm", baseline_arm.clone()),
            ("ao-separated", separate(&baseline_arm, 0)),
            ("roughness-separated", separate(&baseline_arm, 1)),
            ("metallic-separated", separate(&baseline_arm, 2)),
            (
                "interpreted",
                interpreted(&baseline_albedo, &baseline_normal, &baseline_arm),
            ),
        ] {
            save_scales(&baseline, name, data);
        }
        fs::write(
            baseline.join("provenance.txt"),
            "recipe=plank-floor\nstate=planned baseline\ndescription=uniform brown swatch; no procedural flooring identity\n",
        )
        .unwrap();
        write_manifest(&baseline, "planned-baseline");

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
            ("ao-separated", separate(&arm, 0)),
            ("roughness-separated", separate(&arm, 1)),
            ("metallic-separated", separate(&arm, 2)),
            ("interpreted", interpreted(&albedo, &normal, &arm)),
        ] {
            save_scales(&candidate, name, data);
        }
        let revision = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        let revision = String::from_utf8(revision.stdout).unwrap();
        let dirty_state = std::process::Command::new("git")
            .args(["status", "--short"])
            .output()
            .unwrap();
        let dirty_state = String::from_utf8(dirty_state.stdout).unwrap();
        fs::write(
            candidate.join("provenance.txt"),
            format!(
                concat!(
                "recipe=plank-floor\n",
                "candidate=3\n",
                "source=deterministic analytic recipe; no external imagery\n",
                "historical_scope=broad local oak boards in a prosperous German urban interior circa 1544\n",
                "finish=unfinished to lightly waxed, darkened and locally polished by use\n",
                "construction=twenty-two hand-sawn boards; V-axis grain; non-clustering butt joints and selective forged face nails constrained to 0.6 m joists\n",
                "tile_metres=7.2 by 7.2\n",
                "texture_size=1024\n",
                "height_range_metres=0.010\n",
                "arm_packing=R ambient visibility, G perceptual roughness, B metallic\n",
                "metallicity=0\n",
                "seed_contract=splitmix64/inclusive_unit_f32 with fixed hexadecimal salts in plank_floor.rs; no runtime entropy\n",
                "review_fixture=frozen orthographic sheet with upper-left grazing light plus raw separated channels\n",
                "export_command=cargo test -p adventuresim-procedural-textures plank_floor::tests::export_plank_floor_visual_review -- --ignored --exact\n",
                "review_history=candidate 1 self-rejected before independent review because grain, hand-working, wear, and edge irregularity disappeared at full resolution\n",
                "review_history_candidate_2=independent REJECT: 3.6 m repeat, universal sinusoidal pinstripes, modern barcode identity, weak handmade variation, clustered square butt cuts and illegible nails\n",
                "candidate_status=awaiting independent visual acceptance; implementer has not accepted it\n",
                "git_head={}dirty_state_begin\n{}dirty_state_end\n",
                ),
                revision,
                dirty_state,
            ),
        )
        .unwrap();
        write_manifest(&candidate, "candidate-3-awaiting-independent-review");
    }
}
