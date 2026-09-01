//! Overlapping beaver-tail clay roof tiles, oriented with texture V down-slope.
//!
//! This recipe represents the plain, flat `Biberschwanz` covering used in
//! German-speaking regions rather than mixing it with pantile or monk-and-nun
//! profiles. Each visible course overlaps the course below it; the rounded
//! tails and recessed contacts remain legible without turning every tile into
//! a modern, uniformly bevelled extrusion.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const CLAY_ROOF_TILE_TEXTURE_SIZE: u32 = 512;
pub const CLAY_ROOF_TILE_TILE_METRES: f32 = 2.4;
pub const CLAY_ROOF_TILE_HEIGHT_RANGE_METRES: f32 = 0.018;

const COURSES: i32 = 16;
const TILES_PER_COURSE: i32 = 15;
const TAIL_START: f32 = 0.66;

#[derive(Clone, Copy)]
struct TileSample {
    height: f32,
    tile_id: u64,
    contact: f32,
    firing: f32,
    edge_wear: f32,
}

fn hash_unit(value: u64) -> f32 {
    inclusive_unit_f32(splitmix64(value))
}

fn tile_id(row: i32, column: i32) -> u64 {
    splitmix64(
        0x6a17_49d3
            ^ ((row.rem_euclid(COURSES) as u64) << 32)
            ^ column.rem_euclid(TILES_PER_COURSE) as u64,
    )
}

fn tile_coordinates(u: f32, row: i32) -> (i32, f32) {
    let offset = if row.rem_euclid(2) == 0 { 0.0 } else { 0.5 };
    let scaled = u.rem_euclid(1.0) * TILES_PER_COURSE as f32 - offset;
    let column = scaled.floor() as i32;
    (column, scaled - (column as f32 + 0.5))
}

fn tail_side_width(course_phase: f32, id: u64, salt: u64) -> f32 {
    let hand_width = 0.89 + hash_unit(id ^ salt) * 0.095;
    let tail_start = TAIL_START + (hash_unit(id ^ salt.rotate_left(13)) - 0.5) * 0.070;
    if course_phase <= tail_start {
        return 0.5 * hand_width;
    }
    let tail = ((course_phase - tail_start) / (1.0 - tail_start)).clamp(0.0, 1.0);
    let roundness = 1.65 + hash_unit(id ^ salt.rotate_left(29)) * 0.75;
    let rounded = (1.0 - tail.powf(roundness)).max(0.0).sqrt();
    0.5 * hand_width * rounded
}

fn face_variation(local_x: f32, course_phase: f32, id: u64) -> f32 {
    let phase = hash_unit(id ^ 0x83b1) * std::f32::consts::TAU;
    let broad = (local_x * 2.1 + course_phase * 0.8 + phase).sin();
    let fine = (local_x * 7.0 - course_phase * 3.0 + phase * 1.7).sin();
    broad * 0.72 + fine * 0.28
}

fn sample_tiles(u: f32, v: f32) -> TileSample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let scaled_v = v * COURSES as f32;
    let row = scaled_v.floor() as i32;
    let raw_course_phase = scaled_v - row as f32;
    let (column, mut local_x) = tile_coordinates(u, row);
    let id = tile_id(row, column);
    let vertical_offset = (hash_unit(id ^ 0x24d9) - 0.5) * 0.070;
    let course_phase = (raw_course_phase - vertical_offset).clamp(0.0, 1.0);
    let yaw = (hash_unit(id ^ 0xc239) - 0.5) * 0.070;
    let tail_asymmetry = (hash_unit(id ^ 0x37a1) - 0.5) * 0.070;
    let side_warp =
        (course_phase * std::f32::consts::PI + hash_unit(id ^ 0xb547) * 2.0).sin() * 0.007;
    local_x += yaw * (course_phase - 0.5) + side_warp + tail_asymmetry * course_phase.powi(3);
    let left_width = tail_side_width(course_phase, id, 0xa4d7);
    let right_width = tail_side_width(course_phase, id, 0x6e31);
    let edge_distance = (local_x + left_width).min(right_width - local_x);
    let antialias = 0.75 * TILES_PER_COURSE as f32 / CLAY_ROOF_TILE_TEXTURE_SIZE as f32;
    let coverage = ((edge_distance + antialias) / (2.0 * antialias)).clamp(0.0, 1.0);

    // The lower course seen between rounded tails is recessed. V increases in
    // the gravity direction, while height rises gently toward each lower lip.
    let variation = face_variation(local_x, course_phase, id);
    let thickness = (hash_unit(id ^ 0x47ad) - 0.5) * 0.024;
    let cup = (local_x * local_x - 0.16) * ((hash_unit(id ^ 0x7193) - 0.35) * 0.026);
    let twist = local_x * (course_phase - 0.5) * (hash_unit(id ^ 0xe45b) - 0.5) * 0.030;
    let lip = ((course_phase - 0.54) / 0.19).clamp(0.0, 1.0);
    let lip = lip * lip * (3.0 - 2.0 * lip) * 0.050;
    let face_height =
        0.60 + raw_course_phase * 0.038 + lip + thickness + cup + twist + variation * 0.004;
    let under_id = tile_id(row + 1, tile_coordinates(u, row + 1).0);
    let under_variation = face_variation(local_x, 0.12, under_id);
    let under_height = 0.555 + under_variation * 0.004;
    let height = under_height + (face_height - under_height) * coverage;
    let edge_proximity = (1.0 - edge_distance.abs() / 0.085).clamp(0.0, 1.0);
    let lower_lip_contact = ((course_phase - 0.58) / 0.12).clamp(0.0, 1.0)
        * ((0.94 - course_phase) / 0.16).clamp(0.0, 1.0);
    let narrow_side_joint = usize::from(course_phase < TAIL_START) as f32 * edge_proximity;
    let contact = (1.0 - coverage) * edge_proximity * 0.24
        + coverage * (lower_lip_contact * 0.20 + narrow_side_joint * 0.05);
    let wear_segment = (course_phase * 7.0).floor() as u64;
    let edge_wear = edge_proximity
        * coverage
        * ((hash_unit(id ^ wear_segment.wrapping_mul(0x91a7)) - 0.78) / 0.22).clamp(0.0, 1.0);
    let firing_cluster = ((u * 2.0 + v).fract() * std::f32::consts::TAU).sin() * 0.50
        + ((u - v * 2.0).fract() * std::f32::consts::TAU).sin() * 0.28;
    let firing = firing_cluster + (hash_unit(id ^ 0xf613) - 0.5) * 0.38;
    TileSample {
        height,
        tile_id: if coverage >= 0.5 { id } else { under_id },
        contact,
        firing,
        edge_wear,
    }
}

fn color_and_roughness(sample: TileSample) -> ([u8; 3], u8) {
    let mineral = hash_unit(sample.tile_id ^ 0x1db5) - 0.5;
    let kiln_shift = (sample.firing * 11.0 + mineral * 4.0 - sample.contact * 3.0).round() as i16;
    let wear = (sample.edge_wear * 12.0).round() as i16;
    let mut color = [139_i16, 58, 41];
    color[0] += kiln_shift + wear;
    color[1] += kiln_shift / 2 + wear;
    color[2] += kiln_shift / 3 + wear / 2;
    let color = [
        color[0].clamp(0, 255) as u8,
        color[1].clamp(0, 255) as u8,
        color[2].clamp(0, 255) as u8,
    ];
    let roughness = (210.0 - sample.firing * 7.0 + mineral * 4.0 + sample.edge_wear * 11.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    (color, roughness)
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = CLAY_ROOF_TILE_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_clay_roof_tile_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = CLAY_ROOF_TILE_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_tiles(
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
    let metres_per_texel = CLAY_ROOF_TILE_TILE_METRES / size as f32;
    let slope_scale = CLAY_ROOF_TILE_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

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
            let ao = ((1.0 - sample.contact * 0.32) * 255.0)
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
        let textures = generate_clay_roof_tile_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_deterministic_and_periodic() {
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
        for index in 0..256 {
            let coordinate = (index as f32 + 0.5) / 256.0;
            let sample = sample_tiles(coordinate, coordinate * 0.73);
            assert!(
                (sample.height - sample_tiles(coordinate + 1.0, coordinate * 0.73).height).abs()
                    < 1.0e-5
            );
            let repeat_error =
                (sample.height - sample_tiles(coordinate, coordinate * 0.73 + 1.0).height).abs();
            assert!(
                repeat_error < 0.001,
                "vertical repeat error: {repeat_error}"
            );
        }
    }

    #[test]
    fn scale_and_course_direction_describe_overlapping_plain_tiles() {
        let visible_width = CLAY_ROOF_TILE_TILE_METRES / TILES_PER_COURSE as f32;
        let course_exposure = CLAY_ROOF_TILE_TILE_METRES / COURSES as f32;
        assert!((0.15..=0.18).contains(&visible_width));
        assert!((0.14..=0.17).contains(&course_exposure));
        assert!((0.014..=0.022).contains(&CLAY_ROOF_TILE_HEIGHT_RANGE_METRES));
        let tile_center = 0.5 / TILES_PER_COURSE as f32;
        let upper = sample_tiles(tile_center, 0.011).height;
        let lower_lip = sample_tiles(tile_center, 0.038).height;
        assert!(
            lower_lip > upper,
            "course must rise toward its down-slope lip"
        );
    }

    #[test]
    fn rounded_tails_reveal_recessed_under_course_without_floating_gaps() {
        let mut exposed = 0;
        let mut recessed = 0;
        for y in 0..256 {
            for x in 0..256 {
                let sample = sample_tiles((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                exposed += usize::from(sample.height >= 0.56);
                recessed += usize::from(sample.height < 0.56);
                assert!(sample.height >= 0.49);
            }
        }
        assert!(exposed > 45_000, "exposed tile samples: {exposed}");
        assert!(recessed > 4_000, "recessed lap samples: {recessed}");
    }

    #[test]
    fn channels_are_nonmetallic_varied_and_mipped() {
        let (images, textures) = generated();
        let expected_levels = CLAY_ROOF_TILE_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let level_size = CLAY_ROOF_TILE_TEXTURE_SIZE >> level;
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
        let base = &arm[..(CLAY_ROOF_TILE_TEXTURE_SIZE.pow(2) * 4) as usize];
        assert!(base.iter().skip(2).step_by(4).all(|value| *value == 0));
        assert!(
            base.iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 12
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
    fn export_clay_roof_tile_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(CLAY_ROOF_TILE_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn save_rgba(path: &Path, data: &[u8]) {
            image::save_buffer_with_format(
                path,
                data,
                CLAY_ROOF_TILE_TEXTURE_SIZE,
                CLAY_ROOF_TILE_TEXTURE_SIZE,
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
        let output = output_root.join("procedural-texture-reviews/clay-roof-tile/candidate-2");
        let before = output.parent().unwrap().join("before");
        fs::create_dir_all(&output).unwrap();
        fs::create_dir_all(&before).unwrap();
        let baseline = (0..CLAY_ROOF_TILE_TEXTURE_SIZE)
            .flat_map(|y| {
                (0..CLAY_ROOF_TILE_TEXTURE_SIZE).flat_map(move |x| {
                    if (x / 64 + y / 64) % 2 == 0 {
                        [104, 45, 37, 255]
                    } else {
                        [139, 62, 43, 255]
                    }
                })
            })
            .collect::<Vec<_>>();
        save_rgba(
            &before.join("clay-roof-tile-planned-baseline.png"),
            &baseline,
        );

        let (images, textures) = generated();
        let channels = [
            ("albedo", &textures.albedo),
            ("normal", &textures.normal_gl),
            ("height", &textures.height),
            ("arm", &textures.arm),
        ];
        for (name, handle) in channels {
            let data = base_rgba(&images, handle);
            let full_name = format!("clay-roof-tile-{name}.png");
            save_rgba(&output.join(&full_name), &data);
            let base = ImageBuffer::<Rgba<u8>, _>::from_raw(
                CLAY_ROOF_TILE_TEXTURE_SIZE,
                CLAY_ROOF_TILE_TEXTURE_SIZE,
                data.clone(),
            )
            .unwrap();
            let mut tiled = ImageBuffer::new(
                CLAY_ROOF_TILE_TEXTURE_SIZE * 2,
                CLAY_ROOF_TILE_TEXTURE_SIZE * 2,
            );
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * CLAY_ROOF_TILE_TEXTURE_SIZE),
                        i64::from(tile_y * CLAY_ROOF_TILE_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("clay-roof-tile-{name}-tile-2x2.png")))
                .unwrap();
            for preview_size in [128, 64] {
                imageops::resize(
                    &base,
                    preview_size,
                    preview_size,
                    imageops::FilterType::Lanczos3,
                )
                .save(output.join(format!("clay-roof-tile-{name}-{preview_size}.png")))
                .unwrap();
            }
        }

        let albedo = base_rgba(&images, &textures.albedo);
        let normal = base_rgba(&images, &textures.normal_gl);
        let arm = base_rgba(&images, &textures.arm);
        let interpreted = albedo
            .as_chunks::<4>()
            .0
            .iter()
            .zip(normal.chunks_exact(4))
            .zip(arm.chunks_exact(4))
            .flat_map(|((color, normal), arm)| {
                let nx = normal[0] as f32 / 127.5 - 1.0;
                let ny = normal[1] as f32 / 127.5 - 1.0;
                let nz = normal[2] as f32 / 127.5 - 1.0;
                let light = (nx * -0.35 + ny * -0.45 + nz * 0.82).clamp(0.0, 1.0);
                let shade = (0.30 + light * 0.70) * (arm[0] as f32 / 255.0);
                [
                    (color[0] as f32 * shade).round() as u8,
                    (color[1] as f32 * shade).round() as u8,
                    (color[2] as f32 * shade).round() as u8,
                    255,
                ]
            })
            .collect::<Vec<_>>();
        save_rgba(&output.join("clay-roof-tile-interpreted.png"), &interpreted);
        let interpreted_base = ImageBuffer::<Rgba<u8>, _>::from_raw(
            CLAY_ROOF_TILE_TEXTURE_SIZE,
            CLAY_ROOF_TILE_TEXTURE_SIZE,
            interpreted,
        )
        .unwrap();
        let mut interpreted_tiled = ImageBuffer::new(
            CLAY_ROOF_TILE_TEXTURE_SIZE * 2,
            CLAY_ROOF_TILE_TEXTURE_SIZE * 2,
        );
        for tile_y in 0..2 {
            for tile_x in 0..2 {
                imageops::replace(
                    &mut interpreted_tiled,
                    &interpreted_base,
                    i64::from(tile_x * CLAY_ROOF_TILE_TEXTURE_SIZE),
                    i64::from(tile_y * CLAY_ROOF_TILE_TEXTURE_SIZE),
                );
            }
        }
        interpreted_tiled
            .save(output.join("clay-roof-tile-interpreted-tile-2x2.png"))
            .unwrap();
        for preview_size in [128, 64] {
            imageops::resize(
                &interpreted_base,
                preview_size,
                preview_size,
                imageops::FilterType::Lanczos3,
            )
            .save(output.join(format!("clay-roof-tile-interpreted-{preview_size}.png")))
            .unwrap();
        }

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
                &output.join(format!("clay-roof-tile-{name}.png")),
                &separated,
            );
        }

        fs::write(
            output.join("provenance.txt"),
            concat!(
                "recipe=clay-roof-tile\n",
                "candidate=2\n",
                "source=deterministic analytic Biberschwanz plain-tile courses; no external imagery\n",
                "period_and_region=German-speaking Central Europe; late medieval to early modern plain clay covering\n",
                "orientation=texture V increases down the roof slope with gravity\n",
                "physical_tile_metres=2.4\n",
                "visible_tile_width_metres=0.16\n",
                "visible_course_exposure_metres=0.15\n",
                "height_range_metres=0.018\n",
            ),
        )
        .unwrap();
        let revision = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|result| result.status.success())
            .map(|result| String::from_utf8_lossy(&result.stdout).trim().to_owned())
            .unwrap_or_else(|| "unavailable".to_owned());
        let mut evidence = fs::read_dir(&output)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() != "manifest.json")
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        evidence.sort();
        let manifest_entries = evidence
            .iter()
            .map(|path| {
                let bytes = fs::read(path).unwrap();
                format!(
                    "    {{\"file\":\"{}\",\"fnv1a64\":\"{:016x}\"}}",
                    path.file_name().unwrap().to_string_lossy(),
                    fnv1a64(&bytes)
                )
            })
            .collect::<Vec<_>>();
        fs::write(
            output.join("manifest.json"),
            format!(
                "{{\n  \"recipe\": \"clay-roof-tile\",\n  \"candidate\": 2,\n  \"revision\": \"{revision}\",\n  \"tool\": \"procedural-texture-lab plus ignored evidence exporter\",\n  \"export_command\": \"cargo test -p adventuresim-procedural-textures export_clay_roof_tile_visual_review -- --ignored --nocapture\",\n  \"physical_tile_metres\": 2.4,\n  \"visible_tile_width_metres\": 0.16,\n  \"visible_course_exposure_metres\": 0.15,\n  \"height_range_metres\": 0.018,\n  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n  \"files\": [\n{}\n  ]\n}}\n",
                manifest_entries.join(",\n")
            ),
        )
        .unwrap();
    }
}
