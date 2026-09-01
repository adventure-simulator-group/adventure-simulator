//! Thin hand-split slate laid in rising, double-lapped scale courses.
//!
//! Texture V increases down-slope. Each visible course therefore overlaps the
//! next course toward +V. The recipe models a restrained old German scale
//! covering: gently rising courses of irregularly dressed scales with clipped heels,
//! narrow recessed side joints, and shallow cleft planes rather than clay-tile
//! curvature or modern machine-cut regularity.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const SLATE_ROOF_TEXTURE_SIZE: u32 = 512;
pub const SLATE_ROOF_TILE_METRES: f32 = 4.8;
pub const SLATE_ROOF_HEIGHT_RANGE_METRES: f32 = 0.012;

const COURSES: i32 = 28;
const PIECES_PER_COURSE: i32 = 28;
const COURSE_RISE_PER_REPEAT: i32 = 2;
const COURSE_FACE_END: f32 = 0.88;

#[derive(Clone, Copy, Debug)]
struct SlateSample {
    height: f32,
    piece_id: u64,
    contact: f32,
    edge_wear: f32,
    cleavage: f32,
}

fn hash_unit(value: u64) -> f32 {
    inclusive_unit_f32(splitmix64(value))
}

fn piece_id(canonical_row: i32, column: i32) -> u64 {
    splitmix64(
        0x51a7_3e29
            ^ ((canonical_row.rem_euclid(COURSES) as u64) << 32)
            ^ column.rem_euclid(PIECES_PER_COURSE) as u64,
    )
}

fn piece_coordinates(u: f32, row: i32) -> (i32, i32, f32) {
    let base_column = (u * PIECES_PER_COURSE as f32).floor() as i32;
    let base_wrap = base_column.div_euclid(PIECES_PER_COURSE);
    let row_key = (row + COURSE_RISE_PER_REPEAT * base_wrap).rem_euclid(COURSES) as u64;
    let stagger = hash_unit(0x718d_295b ^ row_key) * 0.82;
    let scaled = u * PIECES_PER_COURSE as f32 - stagger;
    let column = scaled.floor() as i32;
    let wrap = column.div_euclid(PIECES_PER_COURSE);
    let canonical_row = row + COURSE_RISE_PER_REPEAT * wrap;
    (canonical_row, column, scaled - column as f32 - 0.5)
}

fn cleft_relief(local_x: f32, course_phase: f32, id: u64) -> f32 {
    let phase = hash_unit(id ^ 0x6ca1) * std::f32::consts::TAU;
    let rake = (hash_unit(id ^ 0x347b) - 0.5) * 0.75;
    let plane_a = (local_x * 2.4 + course_phase * rake + phase).sin();
    let plane_b = (local_x * 4.6 - course_phase * 1.2 + phase * 0.63).sin();
    let plane_c = (local_x * 8.0 + course_phase * 2.1 + phase * 1.41).sin();
    plane_a * 0.58 + plane_b * 0.29 + plane_c * 0.13
}

fn lower_edge(local_x: f32, id: u64) -> f32 {
    let heel_bias = (hash_unit(id ^ 0x728d) - 0.5) * 0.080;
    let left_clip = ((-local_x - 0.25) / 0.20).clamp(0.0, 1.0);
    let right_clip = ((local_x - 0.29) / 0.16).clamp(0.0, 1.0);
    let asymmetry = left_clip * (0.045 + hash_unit(id ^ 0x941f) * 0.050)
        + right_clip * (0.025 + hash_unit(id ^ 0x2e57) * 0.045);
    let chip_segment = ((local_x + 0.5) * 7.0).floor() as u64;
    let chip =
        ((hash_unit(id ^ chip_segment.wrapping_mul(0x85eb)) - 0.86) / 0.14).clamp(0.0, 1.0) * 0.024;
    COURSE_FACE_END + heel_bias - asymmetry - chip
}

fn sample_slate(u: f32, v: f32) -> SlateSample {
    let scaled_v = v * COURSES as f32 - u * COURSE_RISE_PER_REPEAT as f32;
    let row = scaled_v.floor() as i32;
    let phase = scaled_v - row as f32;
    let (canonical_row, column, local_x) = piece_coordinates(u, row);
    let id = piece_id(canonical_row, column);

    let left_boundary = -0.5 + (hash_unit(id ^ 0x14ad) - 0.5) * 0.150;
    let right_boundary =
        0.5 + (hash_unit(piece_id(canonical_row, column + 1) ^ 0x14ad) - 0.5) * 0.150;
    let side_distance = (local_x - left_boundary).min(right_boundary - local_x);
    let side_joint = (1.0 - side_distance / 0.040).clamp(0.0, 1.0);
    let face_end = lower_edge(local_x, id);
    let lip_distance = (phase - face_end).abs();
    let front = phase <= face_end;

    let active_id = if front {
        id
    } else {
        let (under_row, under_column, _) = piece_coordinates(u, row + 1);
        piece_id(under_row, under_column)
    };
    let active_local_x = if front {
        local_x
    } else {
        piece_coordinates(u, row + 1).2
    };
    let active_phase = if front {
        phase
    } else {
        (phase - face_end) * 0.48
    };
    let cleavage = cleft_relief(active_local_x, active_phase, active_id);
    let piece_thickness = (hash_unit(active_id ^ 0xa673) - 0.5) * 0.012;
    let plane_tilt = active_local_x * (hash_unit(active_id ^ 0x1f39) - 0.5) * 0.015
        + (active_phase - 0.5) * (hash_unit(active_id ^ 0xdb42) - 0.5) * 0.012;
    let lip = ((phase - (face_end - 0.10)) / 0.10).clamp(0.0, 1.0);
    let lip = lip * lip * (3.0 - 2.0 * lip) * 0.038;
    let base = if front { 0.604 } else { 0.548 };
    let mut height = base + piece_thickness + plane_tilt + cleavage * 0.006;
    if front {
        height += lip;
    }
    height -= side_joint * if front { 0.050 } else { 0.030 };

    let lip_contact = (1.0 - lip_distance / 0.060).clamp(0.0, 1.0);
    let contact = (side_joint * 0.70 + lip_contact * 0.82).clamp(0.0, 1.0);
    let edge_band = (1.0 - lip_distance / 0.040)
        .clamp(0.0, 1.0)
        .max((1.0 - side_distance / 0.030).clamp(0.0, 1.0));
    let wear_cell = ((local_x + 0.5) * 9.0).floor() as u64;
    let edge_wear = edge_band
        * ((hash_unit(id ^ wear_cell.wrapping_mul(0xb529)) - 0.72) / 0.28).clamp(0.0, 1.0);

    SlateSample {
        height,
        piece_id: active_id,
        contact,
        edge_wear,
        cleavage,
    }
}

fn color_and_roughness(sample: SlateSample) -> ([u8; 3], u8) {
    let mineral = hash_unit(sample.piece_id ^ 0x45c7) - 0.5;
    let cool_shift = (hash_unit(sample.piece_id ^ 0x8a13) - 0.5) * 5.0;
    let color = [
        (52.0 + mineral * 6.0).round(),
        (59.0 + mineral * 7.0).round(),
        (66.0 + mineral * 8.0 + cool_shift).round(),
    ];
    let roughness = (218.0 + mineral * 10.0 + sample.edge_wear * 9.0 - sample.contact * 5.0
        + sample.cleavage.abs() * 2.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    (
        [
            color[0].clamp(0.0, 255.0) as u8,
            color[1].clamp(0.0, 255.0) as u8,
            color[2].clamp(0.0, 255.0) as u8,
        ],
        roughness,
    )
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = SLATE_ROOF_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_slate_roof_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = SLATE_ROOF_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_slate(
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
    let metres_per_texel = SLATE_ROOF_TILE_METRES / size as f32;
    let slope_scale = SLATE_ROOF_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

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
            let ao = ((1.0 - sample.contact * 0.34) * 255.0)
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
        let textures = generate_slate_roof_textures(&mut images);
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
            let sample = sample_slate(coordinate, coordinate * 0.71);
            assert!(
                (sample.height - sample_slate(coordinate + 1.0, coordinate * 0.71).height).abs()
                    < 1.0e-5
            );
            assert!(
                (sample.height - sample_slate(coordinate, coordinate * 0.71 + 1.0).height).abs()
                    < 1.0e-5
            );
        }
    }

    #[test]
    fn scale_and_direction_describe_thin_overlapping_slate() {
        let visible_width = SLATE_ROOF_TILE_METRES / PIECES_PER_COURSE as f32;
        let course_exposure = SLATE_ROOF_TILE_METRES / COURSES as f32;
        assert!((0.15..=0.19).contains(&visible_width));
        assert!((0.15..=0.19).contains(&course_exposure));
        assert!((0.008..=0.014).contains(&SLATE_ROOF_HEIGHT_RANGE_METRES));
        let u = 0.25;
        let (canonical_row, column, local_x) = piece_coordinates(u, 0);
        let face_end = lower_edge(local_x, piece_id(canonical_row, column));
        let upper = sample_slate(
            u,
            (u * COURSE_RISE_PER_REPEAT as f32 + 0.10) / COURSES as f32,
        )
        .height;
        let lower_lip = sample_slate(
            u,
            (u * COURSE_RISE_PER_REPEAT as f32 + face_end - 0.01) / COURSES as f32,
        )
        .height;
        assert!(
            lower_lip > upper,
            "visible course must rise toward its down-slope lip"
        );
    }

    #[test]
    fn recessed_laps_and_narrow_joints_remain_present() {
        let values = (0..256)
            .flat_map(|y| {
                (0..256)
                    .map(move |x| sample_slate((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0))
            })
            .collect::<Vec<_>>();
        let recessed = values.iter().filter(|sample| sample.height < 0.57).count();
        let contacts = values.iter().filter(|sample| sample.contact > 0.55).count();
        assert!(recessed > 3_000, "recessed lap texels: {recessed}");
        assert!(contacts > 3_000, "contact texels: {contacts}");
    }

    #[test]
    fn channels_are_nonmetallic_varied_and_mipped() {
        let (images, textures) = generated();
        let expected_levels = SLATE_ROOF_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let level_size = SLATE_ROOF_TEXTURE_SIZE >> level;
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
        let base = &arm[..(SLATE_ROOF_TEXTURE_SIZE.pow(2) * 4) as usize];
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
    fn export_slate_roof_visual_review() {
        use std::{fmt::Write as _, fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base_rgba(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(SLATE_ROOF_TEXTURE_SIZE.pow(2) * 4) as usize]
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

        fn hash(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf_29ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
            })
        }

        fn variants(output: &Path, stem: &str, data: Vec<u8>) {
            save_rgba(
                &output.join(format!("{stem}.png")),
                &data,
                SLATE_ROOF_TEXTURE_SIZE,
                SLATE_ROOF_TEXTURE_SIZE,
            );
            let base = ImageBuffer::<Rgba<u8>, _>::from_raw(
                SLATE_ROOF_TEXTURE_SIZE,
                SLATE_ROOF_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            let mut tiled =
                ImageBuffer::new(SLATE_ROOF_TEXTURE_SIZE * 2, SLATE_ROOF_TEXTURE_SIZE * 2);
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * SLATE_ROOF_TEXTURE_SIZE),
                        i64::from(tile_y * SLATE_ROOF_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("{stem}-tile-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&base, size, size, imageops::FilterType::Lanczos3)
                    .save(output.join(format!("{stem}-{size}.png")))
                    .unwrap();
            }
        }

        let output_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let output = output_root.join("procedural-texture-reviews/slate-roof/candidate-4");
        let before = output.parent().unwrap().join("before");
        fs::create_dir_all(&output).unwrap();
        fs::create_dir_all(&before).unwrap();
        let baseline = (0..SLATE_ROOF_TEXTURE_SIZE.pow(2))
            .flat_map(|_| [55, 62, 69, 255])
            .collect::<Vec<_>>();
        save_rgba(
            &before.join("slate-roof-planned-baseline.png"),
            &baseline,
            SLATE_ROOF_TEXTURE_SIZE,
            SLATE_ROOF_TEXTURE_SIZE,
        );

        let (images, textures) = generated();
        let channels = [
            ("slate-roof-albedo", &textures.albedo),
            ("slate-roof-normal", &textures.normal_gl),
            ("slate-roof-height", &textures.height),
            ("slate-roof-arm", &textures.arm),
        ];
        let mut manifest = String::from(
            "recipe=slate-roof\ncandidate=4\nlayout=rising double-lapped old German scale courses\noverlap=texture +V (down-slope)\ncourse_rise_metres_per_repeat=0.3429\nnominal_piece_width_metres=0.1714\nnominal_course_exposure_metres=0.1714\nheight_range_metres=0.012\ntexture_dimensions=512x512\npattern_period_metres=4.8\nseed=recipe constants and fabelgeist splitmix64\ngenerator=adventuresim-procedural-textures 0.1.0\nexport_command=cargo test -p adventuresim-procedural-textures export_slate_roof_visual_review --lib -- --ignored --nocapture\nrevision=workspace-uncommitted-texture-iteration\ndirty_tree=true\nhash_algorithm=FNV-1a-64 over file bytes unless decoded_rgba is stated\n",
        );
        let mut channel_data = Vec::new();
        for (stem, handle) in channels {
            let data = base_rgba(&images, handle);
            writeln!(manifest, "fnv1a64_{stem}={:016x}", hash(&data)).unwrap();
            variants(&output, stem, data.clone());
            channel_data.push(data);
        }

        let interpreted = channel_data[0]
            .chunks_exact(4)
            .zip(channel_data[1].chunks_exact(4))
            .zip(channel_data[3].chunks_exact(4))
            .flat_map(|((color, normal), arm)| {
                let nx = normal[0] as f32 / 127.5 - 1.0;
                let ny = normal[1] as f32 / 127.5 - 1.0;
                let nz = normal[2] as f32 / 127.5 - 1.0;
                let light = (nx * -0.35 + ny * -0.45 + nz * 0.82).clamp(0.0, 1.0);
                let shade = (0.34 + light * 0.66) * (arm[0] as f32 / 255.0);
                [
                    (color[0] as f32 * shade).round() as u8,
                    (color[1] as f32 * shade).round() as u8,
                    (color[2] as f32 * shade).round() as u8,
                    255,
                ]
            })
            .collect::<Vec<_>>();
        writeln!(
            manifest,
            "fnv1a64_slate-roof-interpreted={:016x}",
            hash(&interpreted)
        )
        .unwrap();
        variants(&output, "slate-roof-interpreted", interpreted);

        let mut sheet = ImageBuffer::new(SLATE_ROOF_TEXTURE_SIZE * 2, SLATE_ROOF_TEXTURE_SIZE * 2);
        for (index, data) in channel_data.into_iter().enumerate() {
            let panel = ImageBuffer::<Rgba<u8>, _>::from_raw(
                SLATE_ROOF_TEXTURE_SIZE,
                SLATE_ROOF_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            imageops::replace(
                &mut sheet,
                &panel,
                i64::from((index as u32 % 2) * SLATE_ROOF_TEXTURE_SIZE),
                i64::from((index as u32 / 2) * SLATE_ROOF_TEXTURE_SIZE),
            );
        }
        sheet
            .save(output.join("slate-roof-separated-contact-sheet.png"))
            .unwrap();
        let provenance = "recipe=slate-roof\ncandidate=4\nsource=deterministic analytic split-slate courses; no external imagery\nhistorical_scope=restrained old German scale covering for slate-producing regions and high-status structures, circa 1544\norientation=texture V increases down-slope\nlayout=rising double-lapped courses with irregular clipped heels\nseed=recipe constants and fabelgeist splitmix64\n";
        fs::write(output.join("provenance.txt"), provenance).unwrap();
        let baseline_provenance = "recipe=slate-roof\nstate=planned baseline\nsource=flat authored comparison swatch generated by the evidence exporter\noutputs=single albedo-like reference only; no implemented PBR recipe existed\n";
        fs::write(before.join("provenance.txt"), baseline_provenance).unwrap();
        let baseline_bytes = fs::read(before.join("slate-roof-planned-baseline.png")).unwrap();
        writeln!(
            manifest,
            "baseline=slate-roof-planned-baseline.png bytes={} fnv1a64={:016x}",
            baseline_bytes.len(),
            hash(&baseline_bytes)
        )
        .unwrap();
        let mut evidence_files = fs::read_dir(&output)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.file_name().is_some_and(|name| name != "manifest.txt"))
            .collect::<Vec<_>>();
        evidence_files.sort();
        manifest.push_str("evidence_files:\n");
        for path in evidence_files {
            let bytes = fs::read(&path).unwrap();
            writeln!(
                manifest,
                "{} bytes={} fnv1a64={:016x}",
                path.file_name().unwrap().to_string_lossy(),
                bytes.len(),
                hash(&bytes)
            )
            .unwrap();
        }
        fs::write(output.join("manifest.txt"), manifest).unwrap();
    }
}
