use super::*;

pub(super) fn image_rgba(data: Vec<u8>, srgb: bool, repeat: bool, linear_filter: bool) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = if repeat {
        use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
        ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            anisotropy_clamp: if linear_filter { 8 } else { 1 },
            ..if linear_filter {
                ImageSamplerDescriptor::linear()
            } else {
                ImageSamplerDescriptor::nearest()
            }
        })
    } else if linear_filter {
        ImageSampler::linear()
    } else {
        ImageSampler::nearest()
    };
    image
}

pub(super) fn image_rg_mipped(data: Vec<u8>, size: u32, repeat: bool) -> Image {
    assert!(size.is_power_of_two());
    assert_eq!(data.len(), (size * size * 2) as usize);
    let base_level = data.clone();
    let mut mip_data = data;
    let mut previous = base_level.clone();
    let mut previous_size = size;
    while previous_size > 1 {
        let next_size = previous_size / 2;
        let mut next = Vec::with_capacity((next_size * next_size * 2) as usize);
        for y in 0..next_size {
            for x in 0..next_size {
                for channel in 0..2 {
                    let mut sum = 0_u32;
                    for offset_y in 0..2 {
                        for offset_x in 0..2 {
                            let source_x = x * 2 + offset_x;
                            let source_y = y * 2 + offset_y;
                            let index =
                                ((source_y * previous_size + source_x) * 2 + channel) as usize;
                            sum += previous[index] as u32;
                        }
                    }
                    next.push(((sum + 2) / 4) as u8);
                }
            }
        }
        mip_data.extend_from_slice(&next);
        previous = next;
        previous_size = next_size;
    }

    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        base_level,
        TextureFormat::Rg8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(mip_data);
    image.texture_descriptor.mip_level_count = size.ilog2() + 1;
    use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        address_mode_v: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

pub(super) fn image_rgba_mipped(data: Vec<u8>, size: u32, repeat: bool) -> Image {
    assert!(size.is_power_of_two());
    assert_eq!(data.len(), (size * size * 4) as usize);
    let base_level = data.clone();
    let mut mip_data = data;
    let mut previous = base_level.clone();
    let mut previous_size = size;
    while previous_size > 1 {
        let next_size = previous_size / 2;
        let mut next = Vec::with_capacity((next_size * next_size * 4) as usize);
        for y in 0..next_size {
            for x in 0..next_size {
                for channel in 0..4 {
                    let mut sum = 0_u32;
                    for offset_y in 0..2 {
                        for offset_x in 0..2 {
                            let source_x = x * 2 + offset_x;
                            let source_y = y * 2 + offset_y;
                            let index =
                                ((source_y * previous_size + source_x) * 4 + channel) as usize;
                            sum += previous[index] as u32;
                        }
                    }
                    next.push(((sum + 2) / 4) as u8);
                }
            }
        }
        mip_data.extend_from_slice(&next);
        previous = next;
        previous_size = next_size;
    }

    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        base_level,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(mip_data);
    image.texture_descriptor.mip_level_count = size.ilog2() + 1;
    use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        address_mode_v: if repeat {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    image
}
