//! Hand-worked small-pane window glass for early-modern buildings.
//!
//! The generated maps describe imperfections in the glass itself. Lead cames,
//! wooden frames, latches, and bars remain geometry or separate masks: baking
//! them here would make the material repeat structural members within a pane.

use bevy::{asset::Assets, image::Image, math::Vec3, render::render_resource::TextureFormat};

use super::{GlassTextureSet, image_rg_mipped, image_rgba_mipped};

pub const WINDOW_GLASS_TEXTURE_SIZE: u32 = 512;
pub const WINDOW_GLASS_TILE_METRES: f32 = 2.4;
pub const WINDOW_GLASS_NOMINAL_THICKNESS_METRES: f32 = 0.0032;
pub const WINDOW_GLASS_THICKNESS_VARIATION_METRES: f32 = 0.0012;

/// Renderer-independent physical intent for the generated maps.
///
/// A renderer should use specular transmission/refraction where available.
/// The packed thickness channel describes desirable future per-texel optical
/// thickness, but the current Bevy `StandardMaterial` cannot consume it.
/// Fabelgeist therefore renders the nominal 3.2 mm scalar thickness today;
/// the varying thickness remains generated and reviewed future-shader data.
/// `fallback_alpha` is only for a renderer that cannot transmit a scene color;
/// it is not an opacity map and must not replace the transmittance texture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGlassMaterialContract {
    pub index_of_refraction: f32,
    pub specular_transmission: f32,
    pub diffuse_transmission: f32,
    /// Scalar runtime thickness used until the renderer accepts a thickness map.
    pub nominal_thickness_metres: f32,
    pub attenuation_color_linear: [f32; 3],
    pub attenuation_distance_metres: f32,
    pub base_perceptual_roughness: f32,
    pub double_sided: bool,
    pub fallback_alpha: f32,
}

pub const WINDOW_GLASS_MATERIAL_CONTRACT: WindowGlassMaterialContract =
    WindowGlassMaterialContract {
        index_of_refraction: 1.52,
        specular_transmission: 0.96,
        diffuse_transmission: 0.0,
        nominal_thickness_metres: WINDOW_GLASS_NOMINAL_THICKNESS_METRES,
        attenuation_color_linear: [0.76, 0.88, 0.79],
        attenuation_distance_metres: 0.42,
        base_perceptual_roughness: 0.13,
        double_sided: true,
        fallback_alpha: 0.24,
    };

#[derive(Clone, Copy, Debug)]
struct GlassSample {
    optical_height: f32,
    thickness: f32,
    roughness: f32,
    transmittance: [u8; 3],
}

fn periodic_noise(u: f32, v: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (tau * (u + 0.11 * (tau * v).sin())).sin() * 0.42
        + (tau * (v - 0.16 * (tau * u).sin())).sin() * 0.31
        + (tau * (2.0 * u + v)).sin() * 0.12
        + (tau * (u - 2.0 * v)).sin() * 0.08
}

fn toroidal_delta(value: f32, center: f32) -> f32 {
    let delta = (value - center).abs();
    delta.min(1.0 - delta)
}

fn bubble_lens(u: f32, v: f32) -> f32 {
    // Sparse, stretched inclusions associated with hand-blown cylinder glass.
    // Their low amplitude keeps them from reading as dense bubble noise.
    const BUBBLES: [(f32, f32, f32, f32); 6] = [
        (0.127, 0.223, 0.010, 0.019),
        (0.714, 0.091, 0.006, 0.014),
        (0.421, 0.632, 0.008, 0.023),
        (0.882, 0.741, 0.005, 0.010),
        (0.266, 0.891, 0.004, 0.012),
        (0.603, 0.382, 0.004, 0.008),
    ];
    BUBBLES
        .into_iter()
        .map(|(cx, cy, radius_x, radius_y)| {
            let x = toroidal_delta(u, cx) / radius_x;
            let y = toroidal_delta(v, cy) / radius_y;
            let distance = (x * x + y * y).sqrt();
            (1.0 - distance).max(0.0).powi(2)
        })
        .sum::<f32>()
        .min(1.0)
}

fn localized_striation(u: f32, v: f32) -> f32 {
    // A few finite draw marks interrupt broad quiet areas. Unlike candidate 1,
    // no high-frequency stripe traverses the whole repeat.
    const PATCHES: [(f32, f32, f32, f32, f32, f32); 4] = [
        (0.16, 0.29, 0.23, 0.10, 2.4, 0.013),
        (0.71, 0.18, 0.18, 0.08, 3.1, 0.010),
        (0.48, 0.77, 0.27, 0.09, 2.1, 0.012),
        (0.89, 0.61, 0.14, 0.07, 2.8, 0.008),
    ];
    let tau = std::f32::consts::TAU;
    PATCHES
        .into_iter()
        .map(|(cx, cy, radius_x, radius_y, cycles, amplitude)| {
            let dx = toroidal_delta(u, cx);
            let dy = toroidal_delta(v, cy);
            let normalized = ((dx / radius_x).powi(2) + (dy / radius_y).powi(2)).sqrt();
            let envelope = (1.0 - normalized).clamp(0.0, 1.0).powi(3);
            let phase = cycles * dx / radius_x + 0.27 * (tau * dy / radius_y).sin();
            (tau * phase).sin() * envelope * amplitude
        })
        .sum()
}

fn sample_glass(u: f32, v: f32) -> GlassSample {
    let broad = periodic_noise(u, v);
    let draw_striation = localized_striation(u, v);
    let bubble = bubble_lens(u, v);
    let optical_height = broad * 0.15 + draw_striation + bubble * 0.035;
    let thickness = (0.50 + broad * 0.17 + draw_striation * 0.60 + bubble * 0.22).clamp(0.0, 1.0);
    let roughness = (0.11 + broad.abs() * 0.035 + bubble * 0.055).clamp(0.08, 0.24);

    // This is transmitted-light tint, not opaque surface color. Variation is
    // deliberately restrained so a pane does not become blue or milky.
    let absorption = (broad * 3.0 - bubble * 4.0).round() as i16;
    let transmittance = [
        (214_i16 + absorption).clamp(0, 255) as u8,
        (226_i16 + absorption).clamp(0, 255) as u8,
        (217_i16 + absorption).clamp(0, 255) as u8,
    ];
    GlassSample {
        optical_height,
        thickness,
        roughness,
        transmittance,
    }
}

fn height_at(heights: &[f32], x: i32, y: i32) -> f32 {
    let size = WINDOW_GLASS_TEXTURE_SIZE as i32;
    heights[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize]
}

pub fn generate_window_glass_textures(images: &mut Assets<Image>) -> GlassTextureSet {
    let size = WINDOW_GLASS_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_glass(
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                )
            })
        })
        .collect::<Vec<_>>();
    let heights = samples
        .iter()
        .map(|sample| sample.optical_height)
        .collect::<Vec<_>>();
    let mut transmittance = Vec::with_capacity((size * size * 4) as usize);
    let mut normal = Vec::with_capacity(transmittance.capacity());
    let mut thickness_roughness = Vec::with_capacity((size * size * 2) as usize);

    for y in 0..size {
        for x in 0..size {
            let sample = samples[(y * size + x) as usize];
            transmittance.extend_from_slice(&[
                sample.transmittance[0],
                sample.transmittance[1],
                sample.transmittance[2],
                255,
            ]);
            let dx = height_at(&heights, x as i32 + 1, y as i32)
                - height_at(&heights, x as i32 - 1, y as i32);
            let dy = height_at(&heights, x as i32, y as i32 + 1)
                - height_at(&heights, x as i32, y as i32 - 1);
            let surface_normal = Vec3::new(-dx * 14.0, -dy * 14.0, 1.0).normalize();
            let encoded = ((surface_normal + Vec3::ONE) * 127.5)
                .round()
                .clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]);
            thickness_roughness.extend_from_slice(&[
                (sample.thickness * 255.0).round() as u8,
                (sample.roughness * 255.0).round() as u8,
            ]);
        }
    }

    let mut transmittance_image = image_rgba_mipped(transmittance, size, true);
    transmittance_image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    GlassTextureSet {
        transmittance: images.add(transmittance_image),
        optical_normal_gl: images.add(image_rgba_mipped(normal, size, true)),
        thickness_roughness: images.add(image_rg_mipped(thickness_roughness, size, true)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn generated() -> (Assets<Image>, GlassTextureSet) {
        let mut images = Assets::default();
        let textures = generate_window_glass_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn generation_is_deterministic() {
        let (first_images, first) = generated();
        let (second_images, second) = generated();
        for (a, b) in [
            (&first.transmittance, &second.transmittance),
            (&first.optical_normal_gl, &second.optical_normal_gl),
            (&first.thickness_roughness, &second.thickness_roughness),
        ] {
            assert_eq!(
                first_images.get(a).unwrap().data,
                second_images.get(b).unwrap().data
            );
        }
    }

    #[test]
    fn analytic_optical_field_tiles_continuously() {
        let epsilon = 0.2 / WINDOW_GLASS_TEXTURE_SIZE as f32;
        let mut maximum_error = 0.0_f32;
        for index in 0..512 {
            let coordinate = (index as f32 + 0.5) / 512.0;
            maximum_error = maximum_error
                .max(
                    (sample_glass(epsilon, coordinate).optical_height
                        - sample_glass(1.0 - epsilon, coordinate).optical_height)
                        .abs(),
                )
                .max(
                    (sample_glass(coordinate, epsilon).optical_height
                        - sample_glass(coordinate, 1.0 - epsilon).optical_height)
                        .abs(),
                );
        }
        assert!(maximum_error < 0.012, "seam error: {maximum_error}");
    }

    #[test]
    fn tint_is_restrained_and_not_opaque_blue() {
        let (images, textures) = generated();
        let data = images
            .get(&textures.transmittance)
            .unwrap()
            .data
            .as_deref()
            .unwrap();
        let base = &data[..(WINDOW_GLASS_TEXTURE_SIZE.pow(2) * 4) as usize];
        for pixel in base.as_chunks::<4>().0 {
            assert!(pixel[0] >= 204 && pixel[1] >= 216 && pixel[2] >= 207);
            assert!(pixel[1].saturating_sub(pixel[0]) <= 13);
            assert_eq!(pixel[3], 255, "transmittance map is not an alpha fallback");
        }
    }

    #[test]
    fn thickness_and_roughness_are_materially_varied_but_restrained() {
        let (images, textures) = generated();
        let data = images
            .get(&textures.thickness_roughness)
            .unwrap()
            .data
            .as_deref()
            .unwrap();
        let base = &data[..(WINDOW_GLASS_TEXTURE_SIZE.pow(2) * 2) as usize];
        let thickness = base.iter().step_by(2).copied().collect::<BTreeSet<_>>();
        let roughness = base
            .iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(thickness.len() > 80);
        assert!(roughness.len() >= 15);
        assert!(base.iter().skip(1).step_by(2).all(|value| *value <= 62));
    }

    #[test]
    fn every_output_has_a_complete_mip_chain() {
        let (images, textures) = generated();
        let levels = WINDOW_GLASS_TEXTURE_SIZE.ilog2() + 1;
        for (handle, bytes_per_pixel) in [
            (textures.transmittance, 4),
            (textures.optical_normal_gl, 4),
            (textures.thickness_roughness, 2),
        ] {
            let image = images.get(&handle).unwrap();
            let expected = (0..levels)
                .map(|level| {
                    let mip_size = WINDOW_GLASS_TEXTURE_SIZE >> level;
                    (mip_size * mip_size * bytes_per_pixel) as usize
                })
                .sum::<usize>();
            assert_eq!(image.texture_descriptor.mip_level_count, levels);
            assert_eq!(image.data.as_ref().unwrap().len(), expected);
        }
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target"]
    fn export_window_glass_visual_review() {
        use std::{fmt::Write as _, fs, path::Path};

        use image::{ImageBuffer, Rgba, imageops};

        type ReviewImage = ImageBuffer<Rgba<u8>, Vec<u8>>;

        fn rgba_base(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(WINDOW_GLASS_TEXTURE_SIZE.pow(2) * 4) as usize]
                .to_vec()
        }

        fn rg_base(images: &Assets<Image>, handle: &bevy::prelude::Handle<Image>) -> Vec<u8> {
            images.get(handle).unwrap().data.as_ref().unwrap()
                [..(WINDOW_GLASS_TEXTURE_SIZE.pow(2) * 2) as usize]
                .to_vec()
        }

        fn separate_rg(data: &[u8], channel: usize) -> Vec<u8> {
            data.as_chunks::<2>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[channel], pixel[channel], pixel[channel], 255])
                .collect()
        }

        fn save_scales(output: &Path, stem: &str, data: Vec<u8>) {
            let base =
                ReviewImage::from_raw(WINDOW_GLASS_TEXTURE_SIZE, WINDOW_GLASS_TEXTURE_SIZE, data)
                    .unwrap();
            base.save(output.join(format!("window-glass-{stem}-full.png")))
                .unwrap();
            let mut tiled =
                ReviewImage::new(WINDOW_GLASS_TEXTURE_SIZE * 2, WINDOW_GLASS_TEXTURE_SIZE * 2);
            for tile_y in 0..2 {
                for tile_x in 0..2 {
                    imageops::replace(
                        &mut tiled,
                        &base,
                        i64::from(tile_x * WINDOW_GLASS_TEXTURE_SIZE),
                        i64::from(tile_y * WINDOW_GLASS_TEXTURE_SIZE),
                    );
                }
            }
            tiled
                .save(output.join(format!("window-glass-{stem}-2x2.png")))
                .unwrap();
            for size in [128, 64] {
                imageops::resize(&base, size, size, imageops::FilterType::Lanczos3)
                    .save(output.join(format!("window-glass-{stem}-{size}.png")))
                    .unwrap();
            }
        }

        fn background(x: i32, y: i32) -> [u8; 3] {
            let x = x.rem_euclid(WINDOW_GLASS_TEXTURE_SIZE as i32);
            let y = y.rem_euclid(WINDOW_GLASS_TEXTURE_SIZE as i32);
            let panel = ((x / 48) + (y / 48)) & 1;
            let mut color = if panel == 0 {
                [188_u8, 169, 129]
            } else {
                [119_u8, 61, 42]
            };
            if x % 96 < 3 || y % 96 < 3 {
                color = [31, 27, 23];
            }
            if (x + 2 * y).rem_euclid(137) < 4 {
                color = [218, 205, 162];
            }
            color
        }

        fn transmission_fixture(transmittance: &[u8], normal: &[u8]) -> ReviewImage {
            let size = WINDOW_GLASS_TEXTURE_SIZE;
            let mut fixture = ReviewImage::new(size * 2, size);
            for y in 0..size {
                for x in 0..size {
                    let reference = background(x as i32, y as i32);
                    fixture.put_pixel(x, y, Rgba([reference[0], reference[1], reference[2], 255]));
                    let index = ((y * size + x) * 4) as usize;
                    let nx = normal[index] as f32 / 127.5 - 1.0;
                    let ny = normal[index + 1] as f32 / 127.5 - 1.0;
                    let displaced = background(
                        x as i32 + (nx * 22.0).round() as i32,
                        y as i32 + (ny * 22.0).round() as i32,
                    );
                    let highlight = ((nx * -0.55 + ny * -0.35).max(0.0) * 23.0) as u8;
                    fixture.put_pixel(
                        size + x,
                        y,
                        Rgba([
                            ((u16::from(displaced[0]) * u16::from(transmittance[index])) / 255)
                                .saturating_add(u16::from(highlight))
                                .min(255) as u8,
                            ((u16::from(displaced[1]) * u16::from(transmittance[index + 1])) / 255)
                                .saturating_add(u16::from(highlight))
                                .min(255) as u8,
                            ((u16::from(displaced[2]) * u16::from(transmittance[index + 2])) / 255)
                                .saturating_add(u16::from(highlight))
                                .min(255) as u8,
                            255,
                        ]),
                    );
                }
            }
            // Divider: left is unglazed reference, right applies the maps.
            for y in 0..size {
                for x in size.saturating_sub(2)..=size + 1 {
                    fixture.put_pixel(x, y, Rgba([244, 238, 213, 255]));
                }
            }
            fixture
        }

        fn fnv1a64(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
            })
        }

        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/procedural-texture-reviews/window-glass/candidate-2");
        if output.exists() {
            fs::remove_dir_all(&output).unwrap();
        }
        fs::create_dir_all(&output).unwrap();

        let (images, textures) = generated();
        let transmittance = rgba_base(&images, &textures.transmittance);
        let normal = rgba_base(&images, &textures.optical_normal_gl);
        let packed = rg_base(&images, &textures.thickness_roughness);
        for (name, data) in [
            ("transmittance", transmittance.clone()),
            ("optical-normal", normal.clone()),
            ("thickness", separate_rg(&packed, 0)),
            ("roughness", separate_rg(&packed, 1)),
        ] {
            save_scales(&output, name, data);
        }
        let fixture = transmission_fixture(&transmittance, &normal);
        fixture
            .save(output.join("window-glass-transmission-fixture-full.png"))
            .unwrap();
        for (width, height, suffix) in [(256, 128, "128"), (128, 64, "64")] {
            imageops::resize(&fixture, width, height, imageops::FilterType::Lanczos3)
                .save(output.join(format!("window-glass-transmission-fixture-{suffix}.png")))
                .unwrap();
        }
        fs::write(
            output.join("actual-standard-material-capture-UNASSESSABLE.txt"),
            concat!(
                "status=UNASSESSABLE\n",
                "reason=the deterministic procedural-texture harness exports CPU maps and a declared distortion proxy; it does not render Bevy StandardMaterial scene-color transmission\n",
                "required_future_evidence=a frozen GPU capture with opaque high-contrast geometry behind the pane, identical camera/light/exposure, and a no-glass reference\n",
                "proxy_limitation=the PNG transmission fixture demonstrates map direction and distortion legibility, not the exact StandardMaterial refraction result\n",
            ),
        )
        .unwrap();

        let revision = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        let dirty = std::process::Command::new("git")
            .args(["status", "--short"])
            .output()
            .unwrap();
        fs::write(
            output.join("provenance.txt"),
            format!(
                concat!(
                    "recipe=window-glass\n",
                    "candidate=2\n",
                    "status=awaiting independent visual acceptance; implementer has not accepted it\n",
                    "source=deterministic analytic recipe; no external imagery\n",
                    "historical_scope=restrained hand-blown and flattened small-pane cylinder/crown glass appropriate to German buildings circa 1544\n",
                    "features=lower-frequency crown/cylinder waviness, four finite localized draw-mark patches, large quiet areas, six sparse elongated inclusions\n",
                    "excluded=lead cames, wood frames, latches, bars, baked reflections, alpha opacity\n",
                    "texture_outputs=RGB transmittance tint; OpenGL optical normal; RG thickness and perceptual roughness\n",
                    "renderer_contract=IOR 1.52; specular transmission 0.96; diffuse transmission 0; 3.2 mm nominal runtime thickness; 0.42 m attenuation distance; double sided\n",
                    "thickness_contract=packed R is generated evidence and future custom-shader per-texel thickness data; Bevy StandardMaterial does not consume it; runtime uses only nominal scalar 3.2 mm thickness today; packed G is runtime roughness\n",
                    "fallback_contract=alpha 0.24 only if scene-color transmission is unavailable\n",
                    "tile_metres=2.4 by 2.4\n",
                    "texture_size=512\n",
                    "fixture=left unglazed reference; right deterministic texture-driven tint and screen-space distortion proxy; divider is evidence-only\n",
                    "actual_standard_material_capture=UNASSESSABLE in current CPU evidence harness; see actual-standard-material-capture-UNASSESSABLE.txt\n",
                    "review_history_candidate_1=independent REJECT: universal 11/17-cycle vertical corrugation, salient 2x2 landmarks, and insufficiently explicit runtime thickness limitation\n",
                    "export_command=cargo test -p adventuresim-procedural-textures window_glass::tests::export_window_glass_visual_review --lib -- --ignored --exact\n",
                    "git_head={}dirty_state_begin\n{}dirty_state_end\n"
                ),
                String::from_utf8(revision.stdout).unwrap(),
                String::from_utf8(dirty.stdout).unwrap(),
            ),
        )
        .unwrap();

        let mut files = fs::read_dir(&output)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file() && path.file_name().unwrap() != "manifest.json")
            .collect::<Vec<_>>();
        files.sort();
        let mut entries = String::new();
        for (index, path) in files.iter().enumerate() {
            let bytes = fs::read(path).unwrap();
            if index > 0 {
                entries.push_str(",\n");
            }
            write!(
                entries,
                "    {{\"file\":\"{}\",\"bytes\":{},\"fnv1a64\":\"{:016x}\"}}",
                path.file_name().unwrap().to_string_lossy(),
                bytes.len(),
                fnv1a64(&bytes)
            )
            .unwrap();
        }
        fs::write(
            output.join("manifest.json"),
            format!(
                concat!(
                    "{{\n",
                    "  \"schema\": 1,\n",
                    "  \"recipe\": \"window-glass\",\n",
                    "  \"candidate\": 2,\n",
                    "  \"state\": \"awaiting-independent-review\",\n",
                    "  \"dimensions\": {{\"maps_full\":[512,512],\"maps_2x2\":[1024,1024],\"map_reductions\":[128,64],\"transmission_fixture\":[1024,512]}},\n",
                    "  \"hash_algorithm\": \"fnv1a64-file-bytes\",\n",
                    "  \"files\": [\n{}\n  ]\n",
                    "}}\n"
                ),
                entries
            ),
        )
        .unwrap();
    }
}
