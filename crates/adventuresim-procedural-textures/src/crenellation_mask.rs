//! Coverage-preserving opacity for distant crenellated crown silhouettes.
//!
//! This recipe is render-only. One U repeat is one architectural crown pitch
//! (merlon plus embrasure), while V spans the complete breastwork and merlon
//! height. Close, collidable crowns remain geometry owned by the building
//! generator; this mask is only a stable silhouette substitute for shell LODs.

use bevy::{
    asset::{Assets, RenderAssetUsages},
    image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::Handle,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub const CRENELLATION_MASK_TEXTURE_SIZE: u32 = 256;
/// Share of one crown pitch occupied by a merlon.
pub const CRENELLATION_MERLON_DUTY_CYCLE: f32 = 0.60;
/// Share of total strip height occupied by the continuous breastwork.
pub const CRENELLATION_BREASTWORK_HEIGHT_RATIO: f32 = 5.0 / 9.0;
pub const CRENELLATION_ALPHA_CUTOFF: f32 = 0.5;

const EDGE_SUPERSAMPLES: u32 = 4;
const MASONRY_RGB: [u8; 3] = [121, 122, 111];

fn texel_coverage(x: u32, y: u32, size: u32) -> u8 {
    let x = x % size;
    let mut covered = 0_u32;
    for sub_y in 0..EDGE_SUPERSAMPLES {
        for sub_x in 0..EDGE_SUPERSAMPLES {
            let u = (x as f32 + (sub_x as f32 + 0.5) / EDGE_SUPERSAMPLES as f32) / size as f32;
            let v = (y as f32 + (sub_y as f32 + 0.5) / EDGE_SUPERSAMPLES as f32) / size as f32;
            if v >= 1.0 - CRENELLATION_BREASTWORK_HEIGHT_RATIO || u < CRENELLATION_MERLON_DUTY_CYCLE
            {
                covered += 1;
            }
        }
    }
    ((covered * 255 + EDGE_SUPERSAMPLES.pow(2) / 2) / EDGE_SUPERSAMPLES.pow(2)) as u8
}

fn base_level(size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let alpha = texel_coverage(x, y, size);
            pixels.extend_from_slice(&[MASONRY_RGB[0], MASONRY_RGB[1], MASONRY_RGB[2], alpha]);
        }
    }
    pixels
}

fn downsample_coverage(previous: &[u8], previous_size: u32) -> Vec<u8> {
    let next_size = previous_size / 2;
    let mut next = Vec::with_capacity((next_size * next_size * 4) as usize);
    for y in 0..next_size {
        for x in 0..next_size {
            let mut alpha_sum = 0_u32;
            for offset_y in 0..2 {
                for offset_x in 0..2 {
                    let source_x = x * 2 + offset_x;
                    let source_y = y * 2 + offset_y;
                    let index = ((source_y * previous_size + source_x) * 4) as usize;
                    alpha_sum += u32::from(previous[index + 3]);
                }
            }
            let alpha = ((alpha_sum + 2) / 4) as u8;
            next.extend_from_slice(&[MASONRY_RGB[0], MASONRY_RGB[1], MASONRY_RGB[2], alpha]);
        }
    }
    next
}

pub fn generate_crenellation_mask(images: &mut Assets<Image>) -> Handle<Image> {
    let base = base_level(CRENELLATION_MASK_TEXTURE_SIZE);
    let mut packed_mips = base.clone();
    let mut previous = base.clone();
    let mut previous_size = CRENELLATION_MASK_TEXTURE_SIZE;
    while previous_size > 1 {
        let next = downsample_coverage(&previous, previous_size);
        packed_mips.extend_from_slice(&next);
        previous = next;
        previous_size /= 2;
    }

    let mut image = Image::new(
        Extent3d {
            width: CRENELLATION_MASK_TEXTURE_SIZE,
            height: CRENELLATION_MASK_TEXTURE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        base,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(packed_mips);
    image.texture_descriptor.mip_level_count = CRENELLATION_MASK_TEXTURE_SIZE.ilog2() + 1;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    images.add(image)
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Write as _,
        fs,
        path::{Path, PathBuf},
    };

    use bevy::asset::Assets;
    use image::{ImageBuffer, Rgba, RgbaImage, imageops};

    use super::*;

    fn generated() -> (Assets<Image>, Handle<Image>) {
        let mut images = Assets::default();
        let handle = generate_crenellation_mask(&mut images);
        (images, handle)
    }

    fn mip<'a>(data: &'a [u8], level: u32) -> (u32, &'a [u8]) {
        let mut offset = 0_usize;
        let mut size = CRENELLATION_MASK_TEXTURE_SIZE;
        for _ in 0..level {
            offset += (size * size * 4) as usize;
            size /= 2;
        }
        let length = (size * size * 4) as usize;
        (size, &data[offset..offset + length])
    }

    fn mean_alpha(bytes: &[u8]) -> f32 {
        bytes
            .iter()
            .skip(3)
            .step_by(4)
            .map(|v| f32::from(*v))
            .sum::<f32>()
            / (bytes.len() / 4) as f32
            / 255.0
    }

    fn threshold_coverage(bytes: &[u8]) -> f32 {
        let cutoff = (CRENELLATION_ALPHA_CUTOFF * 255.0).round() as u8;
        bytes
            .iter()
            .skip(3)
            .step_by(4)
            .filter(|alpha| **alpha >= cutoff)
            .count() as f32
            / (bytes.len() / 4) as f32
    }

    #[test]
    fn base_duty_cycle_and_breastwork_ratio_match_the_declared_architecture() {
        let pixels = base_level(CRENELLATION_MASK_TEXTURE_SIZE);
        let top_row = &pixels[..(CRENELLATION_MASK_TEXTURE_SIZE * 4) as usize];
        let top_coverage = threshold_coverage(top_row);
        assert!((top_coverage - CRENELLATION_MERLON_DUTY_CYCLE).abs() <= 0.005);

        let opaque_share = threshold_coverage(&pixels);
        let expected = CRENELLATION_BREASTWORK_HEIGHT_RATIO
            + (1.0 - CRENELLATION_BREASTWORK_HEIGHT_RATIO) * CRENELLATION_MERLON_DUTY_CYCLE;
        assert!((opaque_share - expected).abs() <= 0.01);
    }

    #[test]
    fn u_repeat_is_periodic_and_v_is_clamped() {
        for y in 0..CRENELLATION_MASK_TEXTURE_SIZE {
            assert_eq!(
                texel_coverage(0, y, CRENELLATION_MASK_TEXTURE_SIZE),
                texel_coverage(
                    CRENELLATION_MASK_TEXTURE_SIZE,
                    y,
                    CRENELLATION_MASK_TEXTURE_SIZE
                ),
            );
        }
        let (images, handle) = generated();
        let image = images.get(&handle).unwrap();
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("mask needs an explicit sampler")
        };
        assert_eq!(sampler.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(sampler.address_mode_v, ImageAddressMode::ClampToEdge);
    }

    #[test]
    fn alpha_coverage_is_stable_through_visible_mips() {
        let (images, handle) = generated();
        let image = images.get(&handle).unwrap();
        let data = image.data.as_deref().unwrap();
        let base_mean = mean_alpha(mip(data, 0).1);
        for level in 1..=5 {
            let (size, bytes) = mip(data, level);
            assert!(
                (mean_alpha(bytes) - base_mean).abs() <= 0.006,
                "mip {level} ({size}px) changed integrated skyline coverage"
            );
        }
        for level in 0..=3 {
            let (size, bytes) = mip(data, level);
            assert!(
                (threshold_coverage(bytes) - threshold_coverage(mip(data, 0).1)).abs() <= 0.02,
                "mip {level} ({size}px) changed alpha-test occupancy"
            );
        }
    }

    #[test]
    fn generation_is_deterministic_and_mip_complete() {
        let (first_images, first) = generated();
        let (second_images, second) = generated();
        let first = first_images.get(&first).unwrap();
        let second = second_images.get(&second).unwrap();
        assert_eq!(first.data, second.data);
        assert_eq!(first.texture_descriptor.mip_level_count, 9);
        let expected_texels = (0..9)
            .map(|level| (CRENELLATION_MASK_TEXTURE_SIZE >> level).pow(2))
            .sum::<u32>();
        assert_eq!(
            first.data.as_ref().unwrap().len(),
            (expected_texels * 4) as usize
        );
    }

    fn rgba(bytes: &[u8], size: u32) -> RgbaImage {
        ImageBuffer::from_raw(size, size, bytes.to_vec()).unwrap()
    }

    fn composite(bytes: &[u8], size: u32) -> RgbaImage {
        let mut result = RgbaImage::new(size, size);
        for y in 0..size {
            for x in 0..size {
                let index = ((y * size + x) * 4) as usize;
                let alpha = f32::from(bytes[index + 3]) / 255.0;
                let sky = [92.0, 132.0, 171.0];
                let stone = [121.0, 122.0, 111.0];
                result.put_pixel(
                    x,
                    y,
                    Rgba([
                        (sky[0] + (stone[0] - sky[0]) * alpha).round() as u8,
                        (sky[1] + (stone[1] - sky[1]) * alpha).round() as u8,
                        (sky[2] + (stone[2] - sky[2]) * alpha).round() as u8,
                        255,
                    ]),
                );
            }
        }
        result
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
    }

    fn save_png(path: PathBuf, image: &RgbaImage) {
        image.save(path).unwrap();
    }

    #[test]
    #[ignore = "writes deterministic visual-review evidence under target/"]
    fn export_crenellation_mask_visual_review() {
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/procedural-texture-reviews/crenellation-mask/candidate-1");
        if output.exists() {
            fs::remove_dir_all(&output).unwrap();
        }
        fs::create_dir_all(&output).unwrap();

        let (images, handle) = generated();
        let image = images.get(&handle).unwrap();
        let data = image.data.as_deref().unwrap();
        let (full_size, full_bytes) = mip(data, 0);
        let full = rgba(full_bytes, full_size);
        save_png(output.join("crenellation-mask-opacity-full.png"), &full);
        let mut tiled = RgbaImage::new(full_size * 2, full_size * 2);
        for tile_y in 0..2 {
            for tile_x in 0..2 {
                imageops::replace(
                    &mut tiled,
                    &full,
                    i64::from(tile_x * full_size),
                    i64::from(tile_y * full_size),
                );
            }
        }
        save_png(output.join("crenellation-mask-opacity-2x2.png"), &tiled);
        let full_silhouette = composite(full_bytes, full_size);
        let mut tiled_silhouette = RgbaImage::new(full_size * 2, full_size * 2);
        for tile_y in 0..2 {
            for tile_x in 0..2 {
                imageops::replace(
                    &mut tiled_silhouette,
                    &full_silhouette,
                    i64::from(tile_x * full_size),
                    i64::from(tile_y * full_size),
                );
            }
        }
        save_png(
            output.join("crenellation-mask-silhouette-2x2.png"),
            &tiled_silhouette,
        );

        for (level, label) in [(1, "128"), (2, "64")] {
            let (size, bytes) = mip(data, level);
            save_png(
                output.join(format!("crenellation-mask-opacity-{label}.png")),
                &rgba(bytes, size),
            );
            save_png(
                output.join(format!("crenellation-mask-silhouette-{label}.png")),
                &composite(bytes, size),
            );
        }
        save_png(
            output.join("crenellation-mask-silhouette-full.png"),
            &full_silhouette,
        );

        let mut diagnostics = String::from("level,size,mean_alpha,threshold_coverage\n");
        for level in 0..image.texture_descriptor.mip_level_count {
            let (size, bytes) = mip(data, level);
            writeln!(
                diagnostics,
                "{level},{size},{:.6},{:.6}",
                mean_alpha(bytes),
                threshold_coverage(bytes)
            )
            .unwrap();
        }
        fs::write(output.join("coverage-mip-diagnostics.csv"), diagnostics).unwrap();

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
                    "recipe=crenellation-mask\n",
                    "candidate=1\n",
                    "status=awaiting independent visual acceptance; implementer has not accepted it\n",
                    "semantic_owner=distant shell-LOD crenellation silhouette only; never LOD0 geometry or collision\n",
                    "source=deterministic analytic architectural mask; no external imagery\n",
                    "historical_scope=plain rectilinear masonry merlons and embrasures suitable for defensive crowns; not ornamental rounded battlements\n",
                    "uv_contract=one U repeat equals one authored merlon-plus-embrasure pitch; V spans the complete breastwork-plus-merlon strip\n",
                    "physical_scale=the building crown profile owns metres; this normalized mask inherits that exact pitch and height through mesh UVs\n",
                    "duty_cycle=0.60 merlon; 0.40 embrasure\n",
                    "breastwork_height_ratio=0.555556\n",
                    "alpha_contract=4x4 supersampled base coverage; box-integrated coverage mip chain; alpha cutoff 0.5\n",
                    "sampler=repeat U; clamp V; linear minification/magnification; anisotropy 8\n",
                    "excluded=random gaps, rounded teeth, decorative notches, alpha blend fringes, close collision, LOD0 use\n",
                    "texture_size=256; mip_levels=9\n",
                    "fixture=opacity maps and stone-on-sky silhouette composites at full, 2x2, 128, and 64\n",
                    "export_command=cargo test -p adventuresim-procedural-textures crenellation_mask::tests::export_crenellation_mask_visual_review --lib -- --ignored --exact\n",
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
                    "  \"recipe\": \"crenellation-mask\",\n",
                    "  \"candidate\": 1,\n",
                    "  \"state\": \"awaiting-independent-review\",\n",
                    "  \"dimensions\": {{\"full\":[256,256],\"2x2\":[512,512],\"reductions\":[128,64]}},\n",
                    "  \"files\": [\n{}\n  ]\n",
                    "}}\n"
                ),
                entries
            ),
        )
        .unwrap();
    }
}
