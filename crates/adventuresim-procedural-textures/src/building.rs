//! Existing building-texture baselines.
//!
//! Each function is intentionally independent so a texture iteration can
//! replace one recipe without changing tactical material construction.

use bevy::{
    asset::RenderAssetUsages,
    image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::image_rgba_mipped;

pub type Rgba = [u8; 4];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FacadeFinish {
    PlasterInfill,
    BrickInfill,
    FullyRendered,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildingSurfacePalette {
    pub finish: FacadeFinish,
    pub infill: [Rgba; 2],
    pub timber: [Rgba; 2],
    pub tile: [Rgba; 2],
}

impl BuildingSurfacePalette {
    pub const fn new(
        finish: FacadeFinish,
        infill: [Rgba; 2],
        timber: [Rgba; 2],
        tile: [Rgba; 2],
    ) -> Self {
        Self {
            finish,
            infill,
            timber,
            tile,
        }
    }
}

pub fn checker_texture(first: Rgba, second: Rgba) -> Image {
    let size = 64_u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            pixels.extend_from_slice(if (x / 8 + y / 8) % 2 == 0 {
                &first
            } else {
                &second
            });
        }
    }
    image_with_sampler(size, size, pixels, ImageAddressMode::Repeat)
}

pub fn brick_texture(colors: [Rgba; 2]) -> Image {
    let size = 64_u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            pixels.extend_from_slice(&brick_pixel(colors, x, y));
        }
    }
    image_with_sampler(size, size, pixels, ImageAddressMode::Repeat)
}

fn brick_pixel(colors: [Rgba; 2], x: u32, y: u32) -> Rgba {
    let course = y / 8;
    let offset = if course.is_multiple_of(2) { 0 } else { 8 };
    let local_x = (x + offset) % 16;
    if y.is_multiple_of(8) || local_x == 0 {
        [151, 139, 119, 255]
    } else {
        colors[((x / 8 + y / 8) % 2) as usize]
    }
}

pub fn fachwerk_baked_texture(palette: BuildingSurfacePalette) -> Image {
    let size = 512_u32;
    let bay = 256_i32;
    let timber_half_width = 14_i32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let bay_x = x as i32 % bay;
            let bay_y = y as i32 % bay;
            let on_post = bay_x <= timber_half_width || bay_x >= bay - timber_half_width;
            let on_rail = bay_y <= timber_half_width || bay_y >= bay - timber_half_width;
            let on_brace = (bay_x - bay_y).abs() <= timber_half_width
                || (bay_x + bay_y - bay).abs() <= timber_half_width;
            let color = if on_post || on_rail || on_brace {
                let grain = 0.5
                    + 0.26 * ((y as f32 / size as f32) * std::f32::consts::TAU * 41.0).sin()
                    + 0.14
                        * (((x + y * 3) as f32 / size as f32) * std::f32::consts::TAU * 17.0).sin();
                blend_rgba(palette.timber[0], palette.timber[1], grain.clamp(0.0, 1.0))
            } else if palette.finish == FacadeFinish::BrickInfill {
                baked_brick_pixel(palette.infill, x, y, size)
            } else {
                let u = x as f32 / size as f32;
                let v = y as f32 / size as f32;
                let broad = 0.5
                    + 0.20 * (std::f32::consts::TAU * (u * 5.0 + v * 3.0)).sin()
                    + 0.12 * (std::f32::consts::TAU * (u * 13.0 - v * 11.0)).sin();
                blend_rgba(palette.infill[0], palette.infill[1], broad.clamp(0.0, 1.0))
            };
            pixels.extend_from_slice(&color);
        }
    }
    let mut image = image_rgba_mipped(pixels, size, true);
    image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    image
}

fn blend_rgba(first: Rgba, second: Rgba, amount: f32) -> Rgba {
    let blend = |channel: usize| {
        (first[channel] as f32 + (second[channel] as f32 - first[channel] as f32) * amount).round()
            as u8
    };
    [blend(0), blend(1), blend(2), blend(3)]
}

fn baked_brick_pixel(colors: [Rgba; 2], x: u32, y: u32, size: u32) -> Rgba {
    const COURSES: u32 = 20;
    const BRICKS_PER_COURSE: u32 = 8;
    let scaled_y = y * COURSES;
    let course = scaled_y / size;
    let local_y = scaled_y % size;
    let half_brick_offset = if course.is_multiple_of(2) {
        0
    } else {
        size / (BRICKS_PER_COURSE * 2)
    };
    let scaled_x = ((x + half_brick_offset) % size) * BRICKS_PER_COURSE;
    let column = scaled_x / size;
    let local_x = scaled_x % size;
    let mortar = local_y < size / 80 || local_x < size / 96;
    if mortar {
        [151, 139, 119, 255]
    } else {
        let identity = course.wrapping_mul(1_103_515_245) ^ column.wrapping_mul(12_345);
        let tone = 0.30 + (identity & 0xff) as f32 / 255.0 * 0.50;
        blend_rgba(colors[0], colors[1], tone)
    }
}

pub fn facade_atlas() -> Image {
    let width = 256_u32;
    let height = 64_u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let color = match x {
                0..=63 if (x + y / 2) % 13 < 3 => [45, 24, 13, 255],
                0..=63 => [91, 50, 25, 255],
                64..=95 => [53, 102, 123, 255],
                96..=127 => [94, 48, 23, 255],
                128..=159 => [70, 38, 22, 255],
                160..=191 => [30, 28, 24, 255],
                192..=223 => [24, 22, 20, 255],
                _ => [64, 50, 35, 255],
            };
            pixels.extend_from_slice(&color);
        }
    }
    image_with_sampler(width, height, pixels, ImageAddressMode::ClampToEdge)
}

fn image_with_sampler(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    address_mode: ImageAddressMode,
) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        address_mode_w: address_mode,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn baked_fachwerk_is_high_resolution_mipped_and_not_a_checker() {
        let image = fachwerk_baked_texture(BuildingSurfacePalette::new(
            FacadeFinish::PlasterInfill,
            [[214, 204, 177, 255], [192, 181, 153, 255]],
            [[84, 48, 27, 255], [57, 31, 19, 255]],
            [[112, 49, 34, 255], [77, 32, 25, 255]],
        ));
        assert_eq!((image.width(), image.height()), (512, 512));
        assert_eq!(image.texture_descriptor.mip_level_count, 10);
        let base_len = (image.width() * image.height() * 4) as usize;
        let colors = image.data.as_ref().unwrap()[..base_len]
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(colors.len() > 64, "baked facade colors: {}", colors.len());
    }
}
