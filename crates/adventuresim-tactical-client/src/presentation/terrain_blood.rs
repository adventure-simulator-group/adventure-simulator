use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::Image,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

const TERRAIN_BLOOD_MASK_SIZE: u32 = 1024;

pub(super) fn empty_terrain_blood_mask() -> Image {
    let pixels = vec![0; (TERRAIN_BLOOD_MASK_SIZE * TERRAIN_BLOOD_MASK_SIZE) as usize];
    let mut image = Image::new(
        Extent3d {
            width: TERRAIN_BLOOD_MASK_SIZE,
            height: TERRAIN_BLOOD_MASK_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::R8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });
    image
}
