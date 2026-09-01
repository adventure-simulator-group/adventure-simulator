//! Regular early-modern ashlar with recessed lime joints and restrained hand dressing.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const DRESSED_STONE_TEXTURE_SIZE: u32 = 1024;
pub const DRESSED_STONE_TILE_METRES: f32 = 7.2;
pub const DRESSED_STONE_HEIGHT_RANGE_METRES: f32 = 0.024;

const COURSES: i32 = 22;
const MAX_BLOCKS_PER_COURSE: usize = 16;
const MIN_BLOCKS_PER_COURSE: usize = 11;

#[derive(Clone, Copy, Debug)]
struct StoneSample {
    height: f32,
    stone_coverage: f32,
    stone_id: u64,
    mineral: f32,
    tool_strength: f32,
    edge_distance: f32,
}

fn hash_unit(value: u64) -> f32 {
    inclusive_unit_f32(splitmix64(value))
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn course_id(course: i32) -> u64 {
    splitmix64(0x453a_91d7 ^ course.rem_euclid(COURSES) as u64)
}

fn block_count(course: i32) -> usize {
    MIN_BLOCKS_PER_COURSE + course_id(course) as usize % 5
}

fn block_id(course: i32, block: usize) -> u64 {
    splitmix64(course_id(course) ^ (block as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

fn course_weights() -> ([f32; COURSES as usize], f32) {
    let mut weights = [0.0; COURSES as usize];
    let mut total = 0.0;
    for course in 0..COURSES {
        let weight = 0.91 + hash_unit(course_id(course) ^ 0x157d) * 0.18;
        weights[course as usize] = weight;
        total += weight;
    }
    (weights, total)
}

fn course_at(v: f32) -> (i32, f32, f32) {
    let (weights, total) = course_weights();
    let position = v.rem_euclid(1.0) * total;
    let mut start = 0.0;
    for course in 0..COURSES {
        let weight = weights[course as usize];
        if position < start + weight || course + 1 == COURSES {
            return (course, (position - start) / weight, weight / total);
        }
        start += weight;
    }
    unreachable!()
}

fn block_weights(course: i32) -> ([f32; MAX_BLOCKS_PER_COURSE], f32) {
    let mut weights = [0.0; MAX_BLOCKS_PER_COURSE];
    let mut total = 0.0;
    for (block, weight) in weights.iter_mut().take(block_count(course)).enumerate() {
        let id = block_id(course, block);
        *weight = 0.79 + hash_unit(id ^ 0x6c81) * 0.42;
        total += *weight;
    }
    (weights, total)
}

fn course_offset(course: i32) -> f32 {
    // A broad deterministic offset gives a convincing bond without a machine-perfect half bond.
    let alternating = if course.rem_euclid(2) == 0 { 0.0 } else { 0.47 };
    (alternating / block_count(course) as f32
        + (hash_unit(course_id(course) ^ 0xc319) - 0.5) * 0.035)
        .rem_euclid(1.0)
}

fn block_at(course: i32, u: f32) -> (usize, f32, f32) {
    let count = block_count(course);
    let (weights, total) = block_weights(course);
    let position = (u + course_offset(course)).rem_euclid(1.0) * total;
    let mut start = 0.0;
    for (block, weight) in weights.into_iter().take(count).enumerate() {
        if position < start + weight || block + 1 == count {
            return (block, (position - start) / weight, weight / total);
        }
        start += weight;
    }
    unreachable!()
}

fn periodic_wave(coordinate: f32, id: u64, salt: u64) -> f32 {
    let phase = hash_unit(id ^ salt) * std::f32::consts::TAU;
    let phase_two = hash_unit(id ^ salt.rotate_left(17)) * std::f32::consts::TAU;
    (coordinate * std::f32::consts::TAU + phase).sin() * 0.54
        + (coordinate * std::f32::consts::TAU * 2.0 + phase_two).sin() * 0.18
}

fn tool_marks(local_x: f32, local_y: f32, id: u64) -> f32 {
    if hash_unit(id ^ 0x49b5) < 0.56 {
        return 0.0;
    }
    let mark_count = 2 + (splitmix64(id ^ 0x2b8f) % 4) as usize;
    let base_angle = -0.30 + hash_unit(id ^ 0x861d) * 1.90;
    let mut relief = 0.0_f32;
    for mark in 0..mark_count {
        let mark_id = splitmix64(id ^ (mark as u64).wrapping_mul(0x9e37_79b9));
        let center_x = hash_unit(mark_id ^ 0x13c7).mul_add(1.30, -0.65);
        let center_y = hash_unit(mark_id ^ 0xb15d).mul_add(1.30, -0.65);
        let angle = base_angle + (hash_unit(mark_id ^ 0xd371) - 0.5) * 0.54;
        let dx = local_x - center_x;
        let dy = local_y - center_y;
        let along = dx * angle.cos() + dy * angle.sin();
        let mut across = -dx * angle.sin() + dy * angle.cos();
        let half_length = 0.11 + hash_unit(mark_id ^ 0xa275) * 0.30;
        across += (along * along - half_length * half_length * 0.33)
            * (hash_unit(mark_id ^ 0xe695) - 0.5)
            * 0.62;
        let width = 0.026 + hash_unit(mark_id ^ 0x7f29) * 0.036;
        let taper = smoothstep(0.0, 0.28, 1.0 - along.abs() / half_length);
        let gap_center = (hash_unit(mark_id ^ 0x5d91) - 0.5) * half_length;
        let gap_radius = half_length * (0.05 + hash_unit(mark_id ^ 0xc583) * 0.10);
        let interruption = smoothstep(
            0.0,
            gap_radius,
            (along - gap_center).abs() - gap_radius * 0.35,
        );
        let groove = (1.0 - across.abs() / width).max(0.0);
        relief -=
            groove * groove * taper * interruption * (0.004 + hash_unit(mark_id ^ 0x4cb7) * 0.005);
    }
    relief
}

fn localized_edge_wear(local_x: f32, local_y: f32, id: u64) -> f32 {
    if hash_unit(id ^ 0x4ad1) < 0.82 {
        return 0.0;
    }
    let edge = splitmix64(id ^ 0x7c39) % 4;
    let along = if edge < 2 { local_y } else { local_x };
    let across = match edge {
        0 => local_x + 0.88,
        1 => 0.88 - local_x,
        2 => local_y + 0.88,
        _ => 0.88 - local_y,
    };
    let center = hash_unit(id ^ 0xa8e3).mul_add(1.30, -0.65);
    let along_radius = 0.07 + hash_unit(id ^ 0xd457) * 0.11;
    let across_radius = 0.045 + hash_unit(id ^ 0x319b) * 0.055;
    let ellipse = ((along - center) / along_radius).powi(2) + (across / across_radius).powi(2);
    let profile = (1.0 - ellipse).max(0.0);
    -profile * profile * (0.015 + hash_unit(id ^ 0xf1a7) * 0.020)
}

fn sample_stonework(u: f32, v: f32) -> StoneSample {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    let (course, local_y, course_height) = course_at(v);
    let (block, local_x, block_width) = block_at(course, u);
    let id = block_id(course, block);
    let x = local_x * 2.0 - 1.0;
    let y = local_y * 2.0 - 1.0;

    let bed_joint = (0.010 + hash_unit(course_id(course) ^ 0x9751) * 0.004)
        / DRESSED_STONE_TILE_METRES
        / course_height
        * 2.0;
    let head_joint =
        (0.008 + hash_unit(id ^ 0x61df) * 0.005) / DRESSED_STONE_TILE_METRES / block_width * 2.0;
    let horizontal_wobble = periodic_wave(local_x, id, 0xb62d) * 0.008;
    let vertical_wobble = periodic_wave(local_y, id, 0x297b) * 0.010;
    let left = -1.0 + head_joint + vertical_wobble;
    let right = 1.0 - head_joint + vertical_wobble;
    let bottom = -1.0 + bed_joint + horizontal_wobble;
    let top = 1.0 - bed_joint + horizontal_wobble;
    let edge_distance = (left - x).max(x - right).max(bottom - y).max(y - top);
    let antialias = 0.8 / DRESSED_STONE_TEXTURE_SIZE as f32 / block_width.min(course_height);
    let stone_coverage = ((antialias - edge_distance) / (antialias * 2.0)).clamp(0.0, 1.0);

    let planar_tilt =
        x * (hash_unit(id ^ 0x158d) - 0.5) * 0.030 + y * (hash_unit(id ^ 0xb4e7) - 0.5) * 0.022;
    let broad = periodic_wave(x * 0.5 + 0.5, id, 0xc7a9) * 0.005;
    let tools = tool_marks(x, y, id);
    let face_height = 0.71
        + (hash_unit(id ^ 0x53f1) - 0.5) * 0.09
        + planar_tilt
        + broad
        + tools
        + localized_edge_wear(x, y, id);
    let mortar = 0.18
        + ((u * 7.0 + v * 11.0) * std::f32::consts::TAU).sin() * 0.009
        + ((u * 13.0 - v * 5.0) * std::f32::consts::TAU).sin() * 0.004;

    StoneSample {
        height: mortar + (face_height - mortar) * stone_coverage,
        stone_coverage,
        stone_id: id,
        mineral: hash_unit(id ^ 0x98c3),
        tool_strength: tools.abs(),
        edge_distance,
    }
}

fn stone_color(sample: StoneSample) -> ([u8; 3], u8) {
    if sample.stone_coverage < 0.5 {
        return if sample.mineral > 0.5 {
            ([148, 143, 128], 238)
        } else {
            ([137, 134, 121], 241)
        };
    }
    let palette = [
        [128, 126, 113],
        [130, 127, 112],
        [126, 124, 111],
        [132, 129, 115],
        [125, 123, 110],
        [129, 126, 114],
    ];
    let mut color = palette[sample.stone_id as usize % palette.len()];
    let face_shift = ((sample.height - 0.71) * 22.0).round() as i16;
    for channel in &mut color {
        *channel = (*channel as i16 + face_shift).clamp(0, 255) as u8;
    }
    let joint_roughness = (1.0 - (-sample.edge_distance * 25.0).clamp(0.0, 1.0)) * 5.0;
    let roughness = (218.0 + sample.mineral * 8.0 + joint_roughness + sample.tool_strength * 420.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    (color, roughness)
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = DRESSED_STONE_TEXTURE_SIZE as i32;
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
    (1.0 - obstruction * 2.8).clamp(0.48, 1.0)
}

pub fn generate_dressed_stone_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = DRESSED_STONE_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_stonework(
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
    let metres_per_texel = DRESSED_STONE_TILE_METRES / size as f32;
    let slope_scale = DRESSED_STONE_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

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
            let encoded_height = (sample.height * 255.0).round().clamp(0.0, 255.0) as u8;
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            let ao = (ambient_visibility(&heights, x as i32, y as i32) * 255.0)
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
        let textures = generate_dressed_stone_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_deterministic() {
        let (a_images, a) = generated();
        let (b_images, b) = generated();
        for (a_handle, b_handle) in [
            (&a.albedo, &b.albedo),
            (&a.normal_gl, &b.normal_gl),
            (&a.height, &b.height),
            (&a.arm, &b.arm),
        ] {
            assert_eq!(
                a_images.get(a_handle).unwrap().data,
                b_images.get(b_handle).unwrap().data
            );
        }
    }

    #[test]
    fn analytic_field_tiles_continuously() {
        let mut maximum_error = 0.0_f32;
        for index in 0..512 {
            let coordinate = (index as f32 + 0.5) / 512.0;
            let sample = sample_stonework(coordinate, 0.37).height;
            maximum_error = maximum_error
                .max((sample - sample_stonework(coordinate + 1.0, 0.37).height).abs())
                .max((sample - sample_stonework(coordinate, 1.37).height).abs());
        }
        assert!(
            maximum_error < f32::EPSILON,
            "periodic height error: {maximum_error}"
        );
    }

    #[test]
    fn ashlar_has_declared_period_appropriate_scale() {
        assert_eq!(DRESSED_STONE_TILE_METRES, 7.2);
        assert!((0.020..=0.028).contains(&DRESSED_STONE_HEIGHT_RANGE_METRES));
        let (courses, total_height) = course_weights();
        for weight in courses {
            let metres = weight / total_height * DRESSED_STONE_TILE_METRES;
            assert!((0.27..=0.37).contains(&metres), "course height: {metres}");
        }
        for course in 0..COURSES {
            let (weights, total) = block_weights(course);
            for weight in weights.into_iter().take(block_count(course)) {
                let metres = weight / total * DRESSED_STONE_TILE_METRES;
                assert!((0.32..=0.90).contains(&metres), "block width: {metres}");
            }
        }
    }

    #[test]
    fn joints_are_recessed_while_faces_remain_planar() {
        let mut joints = Vec::new();
        let mut faces = Vec::new();
        for y in 0..256 {
            for x in 0..256 {
                let sample = sample_stonework((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                if sample.stone_coverage < 0.1 {
                    joints.push(sample.height);
                } else if sample.stone_coverage > 0.99 && sample.edge_distance < -0.30 {
                    faces.push(sample.height);
                }
            }
        }
        let joint_max = joints.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let face_min = faces.iter().copied().fold(f32::INFINITY, f32::min);
        let face_span = faces.iter().copied().fold(f32::NEG_INFINITY, f32::max) - face_min;
        assert!(face_min - joint_max > 0.35);
        assert!(face_span < 0.17, "planar face span: {face_span}");
    }

    #[test]
    fn channels_are_complete_nonmetallic_and_mipped() {
        let (images, textures) = generated();
        let expected_levels = DRESSED_STONE_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let size = DRESSED_STONE_TEXTURE_SIZE >> level;
                (size * size * 4) as usize
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
        assert_eq!(
            images
                .get(&textures.albedo)
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        let arm = images.get(&textures.arm).unwrap().data.as_deref().unwrap();
        let base = &arm[..(DRESSED_STONE_TEXTURE_SIZE.pow(2) * 4) as usize];
        assert!(base.iter().skip(2).step_by(4).all(|value| *value == 0));
        assert!(
            base.iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 16
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
    fn export_dressed_stone_visual_review() {
        use std::{fs, path::Path, process::Command};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(DRESSED_STONE_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn save_rgba(path: &Path, data: &[u8], size: u32) {
            image::save_buffer_with_format(
                path,
                data,
                size,
                size,
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

        fn command_output(program: &str, arguments: &[&str]) -> String {
            Command::new(program)
                .args(arguments)
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
                .unwrap_or_else(|| "unavailable".to_owned())
        }

        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let review_root = target.join("procedural-texture-reviews/dressed-stone");
        let baseline = review_root.join("baseline-planned");
        let candidate = review_root.join("candidate-3");
        fs::create_dir_all(&baseline).unwrap();
        fs::create_dir_all(&candidate).unwrap();

        let flat_baseline = (0..DRESSED_STONE_TEXTURE_SIZE.pow(2))
            .flat_map(|_| [126, 124, 112, 255])
            .collect::<Vec<_>>();
        save_rgba(
            &baseline.join("dressed-stone-planned-baseline.png"),
            &flat_baseline,
            DRESSED_STONE_TEXTURE_SIZE,
        );
        let baseline_image = ImageBuffer::<Rgba<u8>, _>::from_raw(
            DRESSED_STONE_TEXTURE_SIZE,
            DRESSED_STONE_TEXTURE_SIZE,
            flat_baseline,
        )
        .unwrap();
        let mut baseline_tiled = ImageBuffer::new(
            DRESSED_STONE_TEXTURE_SIZE * 2,
            DRESSED_STONE_TEXTURE_SIZE * 2,
        );
        for tile_y in 0..2 {
            for tile_x in 0..2 {
                imageops::replace(
                    &mut baseline_tiled,
                    &baseline_image,
                    i64::from(tile_x * DRESSED_STONE_TEXTURE_SIZE),
                    i64::from(tile_y * DRESSED_STONE_TEXTURE_SIZE),
                );
            }
        }
        baseline_tiled
            .save(baseline.join("dressed-stone-planned-baseline-2x2.png"))
            .unwrap();
        for size in [128, 64] {
            imageops::resize(&baseline_image, size, size, imageops::FilterType::Lanczos3)
                .save(baseline.join(format!("dressed-stone-planned-baseline-{size}.png")))
                .unwrap();
        }
        let mut baseline_files = fs::read_dir(&baseline)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
            .collect::<Vec<_>>();
        baseline_files.sort();
        let baseline_entries = baseline_files
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
            .collect::<Vec<_>>();
        fs::write(
            baseline.join("manifest.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"recipe\": \"dressed-stone\",\n",
                    "  \"status\": \"planned-no-generator\",\n",
                    "  \"disclosure\": \"The baseline had no texture generator. These flat neutral swatches visualize absence and are not runtime outputs.\",\n",
                    "  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n",
                    "  \"files\": [\n{}\n  ]\n",
                    "}}\n",
                ),
                baseline_entries.join(",\n")
            ),
        )
        .unwrap();
        fs::write(
            baseline.join("provenance.txt"),
            concat!(
                "recipe=dressed-stone\n",
                "status=planned-no-generator\n",
                "source=flat neutral review swatch generated only to disclose the absent baseline\n",
                "runtime_asset=false\n",
            ),
        )
        .unwrap();

        let (images, textures) = generated();
        for (name, handle) in [
            ("albedo", &textures.albedo),
            ("normal", &textures.normal_gl),
            ("height", &textures.height),
            ("arm", &textures.arm),
        ] {
            let data = base_rgba(&images, handle);
            save_rgba(
                &candidate.join(format!("dressed-stone-{name}.png")),
                &data,
                DRESSED_STONE_TEXTURE_SIZE,
            );
            let image = ImageBuffer::<Rgba<u8>, _>::from_raw(
                DRESSED_STONE_TEXTURE_SIZE,
                DRESSED_STONE_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            let mut tiled = ImageBuffer::new(
                DRESSED_STONE_TEXTURE_SIZE * 2,
                DRESSED_STONE_TEXTURE_SIZE * 2,
            );
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &image,
                        i64::from(tile_x * DRESSED_STONE_TEXTURE_SIZE),
                        i64::from(tile_y * DRESSED_STONE_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(candidate.join(format!("dressed-stone-{name}-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&image, size, size, imageops::FilterType::Lanczos3)
                    .save(candidate.join(format!("dressed-stone-{name}-{size}.png")))
                    .unwrap();
            }
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
                &candidate.join(format!("dressed-stone-{name}.png")),
                &separated,
                DRESSED_STONE_TEXTURE_SIZE,
            );
        }

        let git_revision = command_output("git", &["rev-parse", "HEAD"]);
        let git_status = command_output("git", &["status", "--short"]);
        let rustc = command_output("rustc", &["--version"]);
        fs::write(
            candidate.join("provenance.txt"),
            format!(
                concat!(
                    "recipe=dressed-stone\n",
                    "candidate=3\n",
                    "generator_version=3\n",
                    "source=deterministic analytic ashlar; no external imagery\n",
                    "source_file=crates/adventuresim-procedural-textures/src/dressed_stone.rs\n",
                    "physical_tile_metres={}\n",
                    "height_range_metres={}\n",
                    "texture_size={}\n",
                    "courses={}\n",
                    "block_count_range={}-{}\n",
                    "intended_use=fortification, castle, church, and civic ashlar\n",
                    "git_revision={}\n",
                    "git_dirty={}\n",
                    "rustc={}\n",
                    "capture=ignored deterministic unit-test exporter\n",
                ),
                DRESSED_STONE_TILE_METRES,
                DRESSED_STONE_HEIGHT_RANGE_METRES,
                DRESSED_STONE_TEXTURE_SIZE,
                COURSES,
                MIN_BLOCKS_PER_COURSE,
                MAX_BLOCKS_PER_COURSE,
                git_revision,
                !git_status.is_empty(),
                rustc,
            ),
        )
        .unwrap();
        fs::write(
            candidate.join("commands.txt"),
            concat!(
                "cargo test -p adventuresim-procedural-textures dressed_stone --no-fail-fast\n",
                "cargo test -p adventuresim-procedural-textures export_dressed_stone_visual_review -- --ignored --exact\n",
                "cargo run -p adventuresim-procedural-textures --bin procedural-texture-lab -- export dressed-stone\n",
            ),
        )
        .unwrap();

        let mut files = fs::read_dir(&candidate)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
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
            .collect::<Vec<_>>();
        fs::write(
            candidate.join("manifest.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"recipe\": \"dressed-stone\",\n",
                    "  \"candidate\": 3,\n",
                    "  \"dimensions\": {{\"full\": 1024, \"tile_2x2\": 2048, \"previews\": [128, 64]}},\n",
                    "  \"physical_tile_metres\": 7.2,\n",
                    "  \"height_range_metres\": 0.024,\n",
                    "  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n",
                    "  \"files\": [\n{}\n  ]\n",
                    "}}\n",
                ),
                entries.join(",\n")
            ),
        )
        .unwrap();
    }
}
