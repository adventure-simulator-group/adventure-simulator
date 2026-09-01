//! Seamless historic lead-sheet surface for flashing, gutters, roof valleys,
//! and roofs or architectural coverings that were actually clad in lead.
//!
//! Panel edges, standing/folded seams, laps, nails, gutter profiles, and edge
//! wear are geometry- or placement-mask responsibilities. This generic tile is
//! only the quiet oxidized material within a sheet and must not be substituted
//! for forged iron.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const LEAD_SHEET_TEXTURE_SIZE: u32 = 512;
pub const LEAD_SHEET_TILE_METRES: f32 = 1.6;
pub const LEAD_SHEET_HEIGHT_RANGE_METRES: f32 = 0.0014;

fn hash(x: i32, y: i32, period_x: i32, period_y: i32, salt: u64) -> f32 {
    let x = x.rem_euclid(period_x) as u64;
    let y = y.rem_euclid(period_y) as u64;
    inclusive_unit_f32(splitmix64(salt ^ (x << 32) ^ y))
}

fn smooth(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn periodic_noise(u: f32, v: f32, cells_x: i32, cells_y: i32, salt: u64) -> f32 {
    let x = u.rem_euclid(1.0) * cells_x as f32;
    let y = v.rem_euclid(1.0) * cells_y as f32;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = smooth(x.fract());
    let ty = smooth(y.fract());
    let a = hash(ix, iy, cells_x, cells_y, salt);
    let b = hash(ix + 1, iy, cells_x, cells_y, salt);
    let c = hash(ix, iy + 1, cells_x, cells_y, salt);
    let d = hash(ix + 1, iy + 1, cells_x, cells_y, salt);
    let upper = a + (b - a) * tx;
    let lower = c + (d - c) * tx;
    upper + (lower - upper) * ty
}

fn field(u: f32, v: f32) -> (f32, f32, f32, f32) {
    let broad = periodic_noise(u, v, 3, 5, 0x17ec_5b92);
    let medium = periodic_noise(u, v, 13, 17, 0x4d2a_9c31);
    let fine = periodic_noise(u, v, 43, 47, 0x8b17_63de);
    let long_warp = (u * 2.0 * std::f32::consts::TAU).sin() * 0.065
        + (u * 5.0 * std::f32::consts::TAU).sin() * 0.018;
    let roll_primary = ((v * 3.0 + long_warp) * std::f32::consts::TAU).sin();
    let roll_secondary = ((v * 7.0 - long_warp * 0.55 + u) * std::f32::consts::TAU).sin();
    let rolling = roll_primary * 0.72 + roll_secondary * 0.28;
    // Patina is directionally dragged by the same working direction instead
    // of appearing as independent cloudy stains.
    let patina = (0.50
        + roll_primary * 0.11
        + roll_secondary * 0.035
        + (periodic_noise(u, v, 5, 11, 0x72a3_d805) - 0.5) * 0.16)
        .clamp(0.0, 1.0);
    let height = (0.50
        + (broad - 0.5) * 0.010
        + (medium - 0.5) * 0.010
        + (fine - 0.5) * 0.005
        + roll_primary * 0.018
        + roll_secondary * 0.006)
        .clamp(0.0, 1.0);
    (height, patina, rolling, roll_primary)
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = LEAD_SHEET_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_lead_sheet_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = LEAD_SHEET_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                field(
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                )
            })
        })
        .collect::<Vec<_>>();
    let heights = samples.iter().map(|sample| sample.0).collect::<Vec<_>>();
    let capacity = (size * size * 4) as usize;
    let mut albedo = Vec::with_capacity(capacity);
    let mut normal = Vec::with_capacity(capacity);
    let mut height = Vec::with_capacity(capacity);
    let mut arm = Vec::with_capacity(capacity);
    let slope_scale = LEAD_SHEET_HEIGHT_RANGE_METRES / (2.0 * LEAD_SHEET_TILE_METRES / size as f32);

    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) as usize;
            let (surface_height, patina, rolling, roll_primary) = samples[index];
            let base = 91.0 + (patina - 0.5) * 8.0 + rolling * 1.3;
            albedo.extend_from_slice(&[
                (base - 7.0).clamp(65.0, 118.0).round() as u8,
                (base - 2.0).clamp(70.0, 124.0).round() as u8,
                (base + 3.0).clamp(76.0, 132.0).round() as u8,
                255,
            ]);

            let dx = height_at(&heights, x as i32 + 1, y as i32)
                - height_at(&heights, x as i32 - 1, y as i32);
            let dy = height_at(&heights, x as i32, y as i32 + 1)
                - height_at(&heights, x as i32, y as i32 - 1);
            let n = Vec3::new(-dx * slope_scale, -dy * slope_scale, 1.0).normalize();
            let encoded = ((n + Vec3::ONE) * 127.5)
                .round()
                .clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]);
            let h = (surface_height * 255.0).round() as u8;
            height.extend_from_slice(&[h, h, h, 255]);

            // Oxide dulls but does not turn the material into orange corrosion
            // or erase the metallic substrate entirely.
            let roughness = (178.0 + patina * 56.0 + roll_primary * 2.0)
                .clamp(180.0, 224.0)
                .round() as u8;
            let metallic = (240.0 - patina * 8.0).clamp(228.0, 244.0).round() as u8;
            arm.extend_from_slice(&[253, roughness, metallic, 255]);
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
        let textures = generate_lead_sheet_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_repeatable() {
        let (images_a, textures_a) = generated();
        let (images_b, textures_b) = generated();
        for (a, b) in [
            (&textures_a.albedo, &textures_b.albedo),
            (&textures_a.normal_gl, &textures_b.normal_gl),
            (&textures_a.height, &textures_b.height),
            (&textures_a.arm, &textures_b.arm),
        ] {
            assert_eq!(images_a.get(a).unwrap().data, images_b.get(b).unwrap().data);
        }
    }

    #[test]
    fn edges_tile_without_a_value_jump() {
        let (images, textures) = generated();
        let height = images
            .get(&textures.height)
            .unwrap()
            .data
            .as_deref()
            .unwrap();
        let stride = LEAD_SHEET_TEXTURE_SIZE as usize * 4;
        let mut maximum = 0_i16;
        for coordinate in 0..LEAD_SHEET_TEXTURE_SIZE as usize {
            let horizontal = (i16::from(height[coordinate * stride])
                - i16::from(height[coordinate * stride + stride - 4]))
            .abs();
            let vertical = (i16::from(height[coordinate * 4])
                - i16::from(
                    height[(LEAD_SHEET_TEXTURE_SIZE as usize - 1) * stride + coordinate * 4],
                ))
            .abs();
            maximum = maximum.max(horizontal).max(vertical);
        }
        assert!(maximum <= 16, "seam height jump was {maximum}");
    }

    #[test]
    fn scale_and_surface_response_are_physically_bounded() {
        assert!((LEAD_SHEET_TILE_METRES - 1.6).abs() < f32::EPSILON);
        assert!((0.0008..=0.002).contains(&LEAD_SHEET_HEIGHT_RANGE_METRES));
        let (images, textures) = generated();
        let arm = images.get(&textures.arm).unwrap().data.as_deref().unwrap();
        let base = &arm[..(LEAD_SHEET_TEXTURE_SIZE.pow(2) * 4) as usize];
        assert!(base.iter().step_by(4).all(|value| *value >= 250));
        let roughness = base
            .iter()
            .skip(1)
            .step_by(4)
            .copied()
            .collect::<BTreeSet<_>>();
        let metallic = base
            .iter()
            .skip(2)
            .step_by(4)
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(roughness.len() > 15);
        assert!(*roughness.first().unwrap() >= 150);
        assert!(*roughness.last().unwrap() <= 230);
        assert!(*metallic.first().unwrap() >= 215);
        assert!(*metallic.last().unwrap() >= 235);
    }

    #[test]
    fn all_outputs_have_complete_mip_chains() {
        let (images, textures) = generated();
        let levels = LEAD_SHEET_TEXTURE_SIZE.ilog2() + 1;
        let bytes = (0..levels)
            .map(|level| {
                let side = LEAD_SHEET_TEXTURE_SIZE >> level;
                (side * side * 4) as usize
            })
            .sum::<usize>();
        for handle in [
            &textures.albedo,
            &textures.normal_gl,
            &textures.height,
            &textures.arm,
        ] {
            let image = images.get(handle).unwrap();
            assert_eq!(image.texture_descriptor.mip_level_count, levels);
            assert_eq!(image.data.as_ref().unwrap().len(), bytes);
        }
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target"]
    fn export_lead_sheet_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(LEAD_SHEET_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn separated(data: &[u8], channel: usize) -> Vec<u8> {
            data.chunks_exact(4)
                .flat_map(|pixel| [pixel[channel], pixel[channel], pixel[channel], 255])
                .collect()
        }

        fn interpreted(albedo: &[u8], normal: &[u8], arm: &[u8]) -> Vec<u8> {
            albedo
                .chunks_exact(4)
                .zip(normal.chunks_exact(4))
                .zip(arm.chunks_exact(4))
                .flat_map(|((color, normal), arm)| {
                    let n = Vec3::new(
                        normal[0] as f32 / 127.5 - 1.0,
                        normal[1] as f32 / 127.5 - 1.0,
                        normal[2] as f32 / 127.5 - 1.0,
                    );
                    let light = n.dot(Vec3::new(-0.34, -0.46, 0.82)).max(0.0);
                    let roughness = arm[1] as f32 / 255.0;
                    let metallic = arm[2] as f32 / 255.0;
                    let glint = metallic * (1.0 - roughness) * light.powf(12.0) * 100.0;
                    let shade = (0.44 + light * 0.56) * (arm[0] as f32 / 255.0);
                    [
                        (color[0] as f32 * shade + glint).clamp(0.0, 255.0) as u8,
                        (color[1] as f32 * shade + glint).clamp(0.0, 255.0) as u8,
                        (color[2] as f32 * shade + glint).clamp(0.0, 255.0) as u8,
                        255,
                    ]
                })
                .collect()
        }

        fn save_scales(output: &Path, name: &str, data: Vec<u8>) {
            let full = ImageBuffer::<Rgba<u8>, _>::from_raw(
                LEAD_SHEET_TEXTURE_SIZE,
                LEAD_SHEET_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            full.save(output.join(format!("lead-sheet-{name}-full.png")))
                .unwrap();
            let mut tiled =
                ImageBuffer::new(LEAD_SHEET_TEXTURE_SIZE * 2, LEAD_SHEET_TEXTURE_SIZE * 2);
            for y in 0..2 {
                for x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &full,
                        i64::from(x * LEAD_SHEET_TEXTURE_SIZE),
                        i64::from(y * LEAD_SHEET_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("lead-sheet-{name}-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&full, size, size, imageops::FilterType::Lanczos3)
                    .save(output.join(format!("lead-sheet-{name}-{size}.png")))
                    .unwrap();
            }
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
                        "{{\n  \"schema\": 1,\n  \"recipe\": \"lead-sheet\",\n",
                        "  \"state\": \"{state}\",\n",
                        "  \"usage\": \"historic lead flashing, gutters, valleys, and actual lead coverings; not generic iron\",\n",
                        "  \"tile_semantics\": \"seamless 1.6 metre substrate; seams, laps, fasteners, profiles, and edges are geometry or placement masks\",\n",
                        "  \"dimensions\": {{\"full\":[512,512],\"tile_2x2\":[1024,1024],\"reductions\":[128,64]}},\n",
                        "  \"channels\": {{\"arm\":[\"ambient_occlusion\",\"roughness\",\"metallic\",\"unused\"]}},\n",
                        "  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n  \"files\": [\n{entries}\n  ]\n}}\n"
                    ),
                    state = state,
                    entries = entries,
                ),
            )
            .unwrap();
        }

        fn export_set(
            output: &Path,
            albedo: Vec<u8>,
            normal: Vec<u8>,
            height: Vec<u8>,
            arm: Vec<u8>,
        ) {
            let preview = interpreted(&albedo, &normal, &arm);
            for (name, data) in [
                ("albedo", albedo),
                ("normal", normal),
                ("height", height),
                ("arm", arm.clone()),
                ("ao-separated", separated(&arm, 0)),
                ("roughness-separated", separated(&arm, 1)),
                ("metallic-separated", separated(&arm, 2)),
                ("interpreted", preview),
            ] {
                save_scales(output, name, data);
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/procedural-texture-reviews/lead-sheet");
        let baseline = root.join("baseline-planned");
        let candidate = root.join("candidate-3");
        for path in [&baseline, &candidate] {
            if path.exists() {
                fs::remove_dir_all(path).unwrap();
            }
            fs::create_dir_all(path).unwrap();
        }
        let pixels = LEAD_SHEET_TEXTURE_SIZE.pow(2) as usize;
        export_set(
            &baseline,
            [86_u8, 91, 97, 255]
                .into_iter()
                .cycle()
                .take(pixels * 4)
                .collect(),
            [128_u8, 128, 255, 255]
                .into_iter()
                .cycle()
                .take(pixels * 4)
                .collect(),
            [128_u8, 128, 128, 255]
                .into_iter()
                .cycle()
                .take(pixels * 4)
                .collect(),
            [253_u8, 205, 235, 255]
                .into_iter()
                .cycle()
                .take(pixels * 4)
                .collect(),
        );
        fs::write(
            baseline.join("provenance.txt"),
            "recipe=lead-sheet\nstate=planned baseline\ndescription=uniform blue-gray lead swatch; no sheet character\n",
        )
        .unwrap();
        write_manifest(&baseline, "planned-baseline");

        let (images, textures) = generated();
        export_set(
            &candidate,
            base(&images, &textures.albedo),
            base(&images, &textures.normal_gl),
            base(&images, &textures.height),
            base(&images, &textures.arm),
        );
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let head = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repository_root)
            .output()
            .unwrap();
        let status = std::process::Command::new("git")
            .args(["status", "--porcelain=v1", "--untracked-files=all"])
            .current_dir(&repository_root)
            .output()
            .unwrap();
        fs::write(
            candidate.join("provenance.txt"),
            format!(
                concat!(
                    "recipe=lead-sheet\nstate=candidate-3\nhead={}\n",
                    "working_tree_status_begin\n{}working_tree_status_end\n",
                    "generator=crates/adventuresim-procedural-textures/src/lead_sheet.rs\n",
                    "tile_metres={}\nheight_range_metres={}\n",
                    "usage=historic lead flashing, gutters, valleys, or lead coverings; never generic iron\n",
                    "geometry_responsibilities=panel edges, standing or folded seams, laps, fasteners, gutter profiles\n",
                    "mask_responsibilities=localized seam contact, edge wear, runoff streaks tied to actual geometry\n"
                ),
                String::from_utf8(head.stdout).unwrap().trim(),
                String::from_utf8(status.stdout).unwrap(),
                LEAD_SHEET_TILE_METRES,
                LEAD_SHEET_HEIGHT_RANGE_METRES,
            ),
        )
        .unwrap();
        write_manifest(&candidate, "candidate-3-awaiting-independent-review");
    }
}
