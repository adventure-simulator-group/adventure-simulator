//! Existing building-texture baselines.
//!
//! Each function is intentionally independent so a texture iteration can
//! replace one recipe without changing tactical material construction.

use bevy::{
    asset::RenderAssetUsages,
    image::{Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

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
    let size = 128_u32;
    let bay = 64_i32;
    let timber_half_width = 4_i32;
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
                palette.timber[((x / 8 + y / 8) % 2) as usize]
            } else if palette.finish == FacadeFinish::BrickInfill {
                brick_pixel(palette.infill, x, y)
            } else {
                palette.infill[((x / 8 + y / 8) % 2) as usize]
            };
            pixels.extend_from_slice(&color);
        }
    }
    image_with_sampler(size, size, pixels, ImageAddressMode::Repeat)
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
