//! Seamless forged-iron surface for straps, hinges, latches, and bars.
//!
//! Shape masks and fastener geometry belong to the consuming mesh. This recipe
//! represents only the material visible across those forms, with its U axis
//! following the long forging direction.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const IRONWORK_TEXTURE_SIZE: u32 = 512;
pub const IRONWORK_TILE_METRES: f32 = 0.64;
pub const IRONWORK_HEIGHT_RANGE_METRES: f32 = 0.0018;

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
    (a + (b - a) * tx) + ((c + (d - c) * tx) - (a + (b - a) * tx)) * ty
}

fn periodic_delta(value: f32) -> f32 {
    value - value.round()
}

fn hammer_facets(u: f32, v: f32) -> (f32, f32) {
    let mut relief = 0.0;
    let mut crown: f32 = 0.0;
    for mark in 0..38_u64 {
        let seed = splitmix64(0x5d77_a931 ^ mark.wrapping_mul(0x9e37_79b9));
        let dx = periodic_delta(u - inclusive_unit_f32(seed ^ 0x31c7));
        let dy = periodic_delta(v - inclusive_unit_f32(seed ^ 0xa579));
        let angle = if mark % 5 == 0 {
            std::f32::consts::FRAC_PI_2
        } else {
            0.0
        } + (inclusive_unit_f32(seed ^ 0x6d21) - 0.5) * 0.62;
        let (sin_angle, cos_angle) = angle.sin_cos();
        let along = dx * cos_angle + dy * sin_angle;
        let across = -dx * sin_angle + dy * cos_angle;
        let half_length = 0.052 + inclusive_unit_f32(seed ^ 0xf317) * 0.068;
        let half_width = 0.018 + inclusive_unit_f32(seed ^ 0x8ca9) * 0.028;
        let local_u = along / half_length;
        let local_v = across / half_width;
        let radius_squared = local_u * local_u + local_v * local_v;
        if radius_squared >= 1.0 {
            continue;
        }
        let envelope = smooth(1.0 - radius_squared);
        let tilt = local_u * (inclusive_unit_f32(seed ^ 0x42e1) - 0.5)
            + local_v * (inclusive_unit_f32(seed ^ 0xb731) - 0.5) * 0.55;
        relief += envelope * tilt * 0.075;
        crown = crown.max(envelope * (1.0 - tilt.abs() * 0.35));
    }
    (relief.clamp(-0.14, 0.14), crown)
}

fn field(u: f32, v: f32) -> (f32, f32, f32, f32) {
    let broad = periodic_noise(u, v, 4, 5, 0x10e4_91a7);
    let fine = periodic_noise(u, v, 41, 37, 0x4f83_d2a1);
    let (facets, crown) = hammer_facets(u, v);
    let draw =
        ((v * 7.0 + periodic_noise(u, v, 3, 7, 0x6b31_ef95) * 0.34) * std::f32::consts::TAU).sin();
    let scale = periodic_noise(u, v, 17, 19, 0x39de_b647) * 0.62
        + periodic_noise(u, v, 31, 23, 0x74a1_29dd) * 0.38;
    let scale_recess = smooth(((scale - 0.79) / 0.13).clamp(0.0, 1.0));
    let height = (0.50 + (broad - 0.5) * 0.045 + facets + draw * 0.0045 + (fine - 0.5) * 0.012
        - scale_recess * 0.035)
        .clamp(0.0, 1.0);
    (height, crown, scale, scale_recess)
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = IRONWORK_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_ironwork_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = IRONWORK_TEXTURE_SIZE;
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
    let slope_scale = IRONWORK_HEIGHT_RANGE_METRES / (2.0 * IRONWORK_TILE_METRES / size as f32);

    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) as usize;
            let (surface_height, crown, scale, scale_recess) = samples[index];
            let oxide = smooth(((scale - 0.48) / 0.34).clamp(0.0, 1.0));
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let contact_zone =
                smooth(((periodic_noise(u, v, 3, 2, 0xf712_30c5) - 0.62) / 0.30).clamp(0.0, 1.0));
            let polish = smooth(((crown - 0.72) / 0.24).clamp(0.0, 1.0)) * contact_zone;
            let draw = ((y as f32 + 0.5) / size as f32 * 7.0 * std::f32::consts::TAU).sin();
            let base = 43.0 + oxide * 4.0 + polish * 2.0 + draw * 1.2;
            albedo.extend_from_slice(&[
                (base + oxide * 4.0).round() as u8,
                (base - oxide).round() as u8,
                (base - oxide * 3.0).round() as u8,
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
            let ao = ((0.995 - scale_recess * 0.075) * 255.0).round() as u8;
            let roughness = (192.0 + oxide * 27.0 - polish * 22.0 + draw.abs() * 2.0)
                .clamp(145.0, 222.0)
                .round() as u8;
            arm.extend_from_slice(&[ao, roughness, 255, 255]);
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
        let textures = generate_ironwork_textures(&mut images);
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
        let stride = IRONWORK_TEXTURE_SIZE as usize * 4;
        let mut maximum = 0_i16;
        for coordinate in 0..IRONWORK_TEXTURE_SIZE as usize {
            let horizontal = (i16::from(height[coordinate * stride])
                - i16::from(height[coordinate * stride + stride - 4]))
            .abs();
            let vertical = (i16::from(height[coordinate * 4])
                - i16::from(
                    height[(IRONWORK_TEXTURE_SIZE as usize - 1) * stride + coordinate * 4],
                ))
            .abs();
            maximum = maximum.max(horizontal).max(vertical);
        }
        assert!(maximum <= 24, "seam height jump was {maximum}");
    }

    #[test]
    fn scale_and_surface_response_are_physically_bounded() {
        assert!((IRONWORK_TILE_METRES - 0.64).abs() < f32::EPSILON);
        assert!((0.001..=0.003).contains(&IRONWORK_HEIGHT_RANGE_METRES));
        let (images, textures) = generated();
        let arm = images.get(&textures.arm).unwrap().data.as_deref().unwrap();
        let base = &arm[..(IRONWORK_TEXTURE_SIZE.pow(2) * 4) as usize];
        assert!(base.iter().skip(2).step_by(4).all(|value| *value == 255));
        let roughness = base
            .iter()
            .skip(1)
            .step_by(4)
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(roughness.len() > 30);
        assert!(*roughness.first().unwrap() >= 128);
        assert!(*roughness.last().unwrap() <= 232);
        assert!(
            base.iter()
                .step_by(4)
                .filter(|value| **value >= 245)
                .count()
                * 100
                / (base.len() / 4)
                >= 97
        );
    }

    #[test]
    fn all_outputs_have_complete_mip_chains() {
        let (images, textures) = generated();
        let levels = IRONWORK_TEXTURE_SIZE.ilog2() + 1;
        let bytes = (0..levels)
            .map(|level| {
                let side = IRONWORK_TEXTURE_SIZE >> level;
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
    fn export_ironwork_visual_review() {
        use std::{fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        fn base(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(IRONWORK_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn separate(data: &[u8], channel: usize) -> Vec<u8> {
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
                    let diffuse = n.dot(Vec3::new(-0.36, -0.48, 0.80)).max(0.0);
                    let roughness = arm[1] as f32 / 255.0;
                    let metallic_glint = (1.0 - roughness) * diffuse.powf(9.0) * 115.0;
                    let shade = (0.40 + diffuse * 0.60) * (arm[0] as f32 / 255.0);
                    [
                        (color[0] as f32 * shade + metallic_glint).clamp(0.0, 255.0) as u8,
                        (color[1] as f32 * shade + metallic_glint).clamp(0.0, 255.0) as u8,
                        (color[2] as f32 * shade + metallic_glint).clamp(0.0, 255.0) as u8,
                        255,
                    ]
                })
                .collect()
        }

        fn save_scales(output: &Path, name: &str, data: Vec<u8>) {
            let full = ImageBuffer::<Rgba<u8>, _>::from_raw(
                IRONWORK_TEXTURE_SIZE,
                IRONWORK_TEXTURE_SIZE,
                data,
            )
            .unwrap();
            full.save(output.join(format!("ironwork-{name}-full.png")))
                .unwrap();
            let mut tiled = ImageBuffer::new(IRONWORK_TEXTURE_SIZE * 2, IRONWORK_TEXTURE_SIZE * 2);
            for y in 0..2 {
                for x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &full,
                        i64::from(x * IRONWORK_TEXTURE_SIZE),
                        i64::from(y * IRONWORK_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("ironwork-{name}-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&full, size, size, imageops::FilterType::Lanczos3)
                    .save(output.join(format!("ironwork-{name}-{size}.png")))
                    .unwrap();
            }
        }

        fn fnv1a64(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
            })
        }

        fn manifest(output: &Path, state: &str) {
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
                    "{{\n  \"schema\": 1,\n  \"recipe\": \"ironwork\",\n  \"state\": \"{state}\",\n  \"dimensions\": {{\"full\":[512,512],\"tile_2x2\":[1024,1024],\"reductions\":[128,64]}},\n  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n  \"files\": [\n{entries}\n  ]\n}}\n"
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
                ("height", height.clone()),
                ("arm", arm.clone()),
                ("ao-separated", separate(&arm, 0)),
                ("roughness-separated", separate(&arm, 1)),
                ("metallic-separated", separate(&arm, 2)),
                ("interpreted", preview),
            ] {
                save_scales(output, name, data);
            }
        }

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/procedural-texture-reviews/ironwork");
        let baseline = root.join("baseline-planned");
        let candidate = root.join("candidate-5");
        for path in [&baseline, &candidate] {
            if path.exists() {
                fs::remove_dir_all(path).unwrap();
            }
            fs::create_dir_all(path).unwrap();
        }
        let pixels = IRONWORK_TEXTURE_SIZE.pow(2) as usize;
        export_set(
            &baseline,
            [42_u8, 41, 39, 255]
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
            [255_u8, 205, 255, 255]
                .into_iter()
                .cycle()
                .take(pixels * 4)
                .collect(),
        );
        fs::write(
            baseline.join("provenance.txt"),
            "recipe=ironwork\nstate=planned baseline\ndescription=uniform dark metallic swatch; no forged surface identity\n",
        )
        .unwrap();
        manifest(&baseline, "planned-baseline");

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
        let head = String::from_utf8(head.stdout).unwrap();
        let status = String::from_utf8(status.stdout).unwrap();
        fs::write(
            candidate.join("provenance.txt"),
            format!(concat!(
                "recipe=ironwork\n",
                "candidate=5\n",
                "source=deterministic analytic recipe; no external imagery\n",
                "semantic_scope=generic tileable wrought-iron surface only; fitting silhouettes, edges, rivets and wear masks belong to consuming meshes\n",
                "historical_scope=forged straps, hinges, latches, bars and fittings in a German urban setting circa 1544\n",
                "forging_axis=U; meshes should align their long forged direction to texture U\n",
                "tile_metres=0.64 by 0.64\ntexture_size=512\nheight_range_metres=0.0018\n",
                "arm_packing=R ambient visibility, G perceptual roughness, B metallic\nmetallicity=1\n",
                "surface=mostly quiet plane with sparse overlapping tilted hammer facets, restrained dark oxide scale, crown-linked contact polish, subtle U-axis draw marks\n",
                "seamless=true\nreductions=128 and 64 preserve broad material identity\n",
                "deterministic_constants=facet_seed:5d77a931,facet_step:9e3779b9,broad:10e491a7,draw:6b31ef95,scale_a:39deb647,scale_b:74a129dd,fine:4f83d2a1\n",
                "repository_head={}\nrepository_dirty={}\nrepository_status_porcelain_v1_begin\n{}repository_status_porcelain_v1_end\n",
                "review_fixture=frozen orthographic sheet with grazing light plus raw and separated channels\n",
                "export_command=cargo test -p adventuresim-procedural-textures ironwork::tests::export_ironwork_visual_review -- --ignored --exact\n",
                "prior_candidates=candidate-1 cloudy wood-like relief; candidate-2 repeating diagonal stamps; candidate-3 cracked-leather cell edges and vertical polish streaks; candidate-4 rejected independently for residual Voronoi boundary network and cloudy masks\n",
                "candidate_status=awaiting independent visual acceptance; implementer has not accepted it\n"
                ),
                head.trim(),
                !status.is_empty(),
                status,
            ),
        )
        .unwrap();
        manifest(&candidate, "candidate-5-awaiting-independent-review");
    }
}
