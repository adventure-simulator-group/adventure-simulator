use super::*;

const TEXTURE_SIZE: u32 = 256;

#[derive(Clone, Debug)]
pub(super) struct LeafTextureSet {
    pub(super) opacity: Handle<Image>,
    pub(super) front_albedo: Handle<Image>,
    pub(super) back_albedo: Handle<Image>,
    pub(super) front_normal: Handle<Image>,
    pub(super) back_normal: Handle<Image>,
    #[allow(dead_code)]
    pub(super) height: Handle<Image>,
    pub(super) arm: Handle<Image>,
}

#[derive(Clone, Debug)]
pub(super) struct SurfaceTextureSet {
    pub(super) albedo: Handle<Image>,
    pub(super) normal_gl: Handle<Image>,
    #[allow(dead_code)]
    pub(super) height: Handle<Image>,
    pub(super) arm: Handle<Image>,
}

#[derive(Resource, Clone, Debug)]
pub(crate) struct ProceduralEnvironmentAssets {
    pub(super) oak_leaf: LeafTextureSet,
    pub(super) dry_oak_leaf: LeafTextureSet,
    pub(super) hazel_leaf: LeafTextureSet,
    pub(super) rock: SurfaceTextureSet,
}

#[derive(Clone, Copy, Debug)]
struct LeafRecipe {
    widest_point: f32,
    base_power: f32,
    tip_power: f32,
    lobe_count: f32,
    lobe_depth: f32,
    tooth_count: f32,
    tooth_depth: f32,
    vein_pairs: u32,
    bend: f32,
    blade: [u8; 3],
    vein: [u8; 3],
    back_blade: [u8; 3],
    roughness: u8,
}

impl LeafRecipe {
    const WHITE_OAK: Self = Self {
        widest_point: 0.48,
        base_power: 0.72,
        tip_power: 0.58,
        lobe_count: 4.5,
        lobe_depth: 0.25,
        tooth_count: 0.0,
        tooth_depth: 0.0,
        vein_pairs: 7,
        bend: 0.035,
        blade: [76, 111, 48],
        vein: [139, 157, 76],
        back_blade: [91, 116, 65],
        roughness: 219,
    };

    const DRY_WHITE_OAK: Self = Self {
        blade: [157, 105, 49],
        vein: [190, 139, 69],
        back_blade: [126, 82, 43],
        roughness: 232,
        ..Self::WHITE_OAK
    };

    const HAZEL: Self = Self {
        widest_point: 0.43,
        base_power: 0.58,
        tip_power: 0.72,
        lobe_count: 0.0,
        lobe_depth: 0.0,
        tooth_count: 13.0,
        tooth_depth: 0.075,
        vein_pairs: 9,
        bend: -0.025,
        blade: [66, 112, 48],
        vein: [129, 154, 75],
        back_blade: [82, 119, 61],
        roughness: 216,
    };
}

#[derive(Clone, Copy, Debug)]
enum SurfaceRecipe {
    OakBark,
    Rock,
}

pub(super) fn setup_procedural_environment_assets(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(generate_procedural_environment_assets(&mut images));
}

pub(super) fn generate_procedural_environment_assets(
    images: &mut Assets<Image>,
) -> ProceduralEnvironmentAssets {
    ProceduralEnvironmentAssets {
        oak_leaf: generate_leaf_textures(images, LeafRecipe::WHITE_OAK),
        dry_oak_leaf: generate_leaf_textures(images, LeafRecipe::DRY_WHITE_OAK),
        hazel_leaf: generate_leaf_textures(images, LeafRecipe::HAZEL),
        rock: generate_surface_textures(images, SurfaceRecipe::Rock),
    }
}

fn image_rgba(data: Vec<u8>, srgb: bool, repeat: bool, linear_filter: bool) -> Image {
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

fn leaf_width(recipe: LeafRecipe, t: f32) -> f32 {
    let base = if t < recipe.widest_point {
        (t / recipe.widest_point)
            .clamp(0.0, 1.0)
            .powf(recipe.base_power)
    } else {
        ((1.0 - t) / (1.0 - recipe.widest_point))
            .clamp(0.0, 1.0)
            .powf(recipe.tip_power)
    };
    let lobes = if recipe.lobe_count > 0.0 {
        1.0 - recipe.lobe_depth
            * (0.5 + 0.5 * (t * recipe.lobe_count * core::f32::consts::TAU).cos())
            * (t * core::f32::consts::PI).sin().powi(2)
    } else {
        1.0
    };
    let teeth = if recipe.tooth_count > 0.0 {
        1.0 - recipe.tooth_depth
            * (0.5 + 0.5 * (t * recipe.tooth_count * core::f32::consts::TAU).cos())
    } else {
        1.0
    };
    base * lobes * teeth
}

fn leaf_sample(recipe: LeafRecipe, u: f32, v: f32) -> (bool, bool, f32) {
    // A small petiole margin leaves room for the same parameter model to own
    // both blade and skeleton, as in the organization leaf generator.
    let t = ((1.0 - v) - 0.08) / 0.84;
    let axis = recipe.bend * (t - 0.5).powi(2);
    let x = (u - 0.5) * 2.15 - axis;
    let petiole = (0.0..0.09).contains(&(1.0 - v)) && x.abs() < 0.018;
    if !(0.0..=1.0).contains(&t) {
        return (petiole, petiole, if petiole { 0.08 } else { 0.0 });
    }
    let width = leaf_width(recipe, t) * 0.43;
    let inside = x.abs() <= width;
    let midrib = x.abs() < 0.012;
    let mut vein = midrib;
    for index in 0..recipe.vein_pairs {
        let origin_t = 0.13 + index as f32 / recipe.vein_pairs as f32 * 0.72;
        let reach = leaf_width(recipe, origin_t) * 0.37;
        let dy = t - origin_t;
        if (0.0..0.16).contains(&dy) {
            let target = reach * (dy / 0.16);
            vein |= (x.abs() - target).abs() < 0.009;
        }
    }
    let dome = if inside {
        (1.0 - (x / width.max(0.001)).powi(2)).max(0.0) * 0.32 + if vein { 0.16 } else { 0.0 }
    } else if petiole {
        0.12
    } else {
        0.0
    };
    (inside || petiole, vein || petiole, dome)
}

fn generate_leaf_textures(images: &mut Assets<Image>, recipe: LeafRecipe) -> LeafTextureSet {
    let pixel_count = (TEXTURE_SIZE * TEXTURE_SIZE) as usize;
    let mut opacity = Vec::with_capacity(pixel_count * 4);
    let mut front = Vec::with_capacity(pixel_count * 4);
    let mut back = Vec::with_capacity(pixel_count * 4);
    let mut normal_front = Vec::with_capacity(pixel_count * 4);
    let mut normal_back = Vec::with_capacity(pixel_count * 4);
    let mut height_map = Vec::with_capacity(pixel_count * 4);
    let mut arm = Vec::with_capacity(pixel_count * 4);
    let texel = 1.0 / TEXTURE_SIZE as f32;
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let u = (x as f32 + 0.5) * texel;
            let v = (y as f32 + 0.5) * texel;
            let (inside, vein, height) = leaf_sample(recipe, u, v);
            let alpha = if inside { 255 } else { 0 };
            opacity.extend_from_slice(&[alpha, alpha, alpha, alpha]);
            let front_color = if vein { recipe.vein } else { recipe.blade };
            let back_color = if vein { recipe.vein } else { recipe.back_blade };
            if inside {
                front.extend_from_slice(&[front_color[0], front_color[1], front_color[2], alpha]);
                back.extend_from_slice(&[back_color[0], back_color[1], back_color[2], alpha]);
            } else {
                front.extend_from_slice(&[0, 0, 0, 0]);
                back.extend_from_slice(&[0, 0, 0, 0]);
            }
            let hx = leaf_sample(recipe, (u + texel).min(1.0), v).2
                - leaf_sample(recipe, (u - texel).max(0.0), v).2;
            let hy = leaf_sample(recipe, u, (v + texel).min(1.0)).2
                - leaf_sample(recipe, u, (v - texel).max(0.0)).2;
            let normal = Vec3::new(-hx * 9.0, hy * 9.0, 1.0).normalize();
            let encoded = ((normal + Vec3::ONE) * 127.5).clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal_front.extend_from_slice(&[
                encoded.x as u8,
                encoded.y as u8,
                encoded.z as u8,
                255,
            ]);
            normal_back.extend_from_slice(&[
                encoded.x as u8,
                (255.0 - encoded.y) as u8,
                encoded.z as u8,
                255,
            ]);
            let ao = if inside {
                (214.0 + height * 41.0).min(255.0) as u8
            } else {
                255
            };
            let encoded_height = (height * 255.0).clamp(0.0, 255.0) as u8;
            height_map.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            arm.extend_from_slice(&[ao, recipe.roughness, 0, 255]);
        }
    }
    LeafTextureSet {
        opacity: images.add(image_rgba(opacity, false, false, false)),
        front_albedo: images.add(image_rgba(front, true, false, false)),
        back_albedo: images.add(image_rgba(back, true, false, false)),
        front_normal: images.add(image_rgba(normal_front, false, false, true)),
        back_normal: images.add(image_rgba(normal_back, false, false, true)),
        height: images.add(image_rgba(height_map, false, false, true)),
        arm: images.add(image_rgba(arm, false, false, false)),
    }
}

fn periodic_height(recipe: SurfaceRecipe, u: f32, v: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    match recipe {
        SurfaceRecipe::OakBark => {
            let furrow = (u * tau * 5.0 + (v * tau * 2.0).sin() * 0.55).sin().abs();
            let plates = (u * tau * 11.0).sin() * (v * tau * 7.0).cos();
            furrow * 0.72 + plates * 0.10
        }
        SurfaceRecipe::Rock => {
            (u * tau * 4.0).sin() * (v * tau * 5.0).cos() * 0.24
                + ((u * 1.7 - v) * tau * 13.0).sin() * 0.07
        }
    }
}

fn surface_palette(recipe: SurfaceRecipe, height: f32, _u: f32, _v: f32) -> ([u8; 3], u8) {
    match recipe {
        SurfaceRecipe::OakBark => ([70, 50, 30], 241),
        SurfaceRecipe::Rock => {
            if height > 0.04 {
                ([139, 136, 128], 221)
            } else {
                ([119, 117, 112], 221)
            }
        }
    }
}

fn generate_surface_textures(
    images: &mut Assets<Image>,
    recipe: SurfaceRecipe,
) -> SurfaceTextureSet {
    let pixel_count = (TEXTURE_SIZE * TEXTURE_SIZE) as usize;
    let mut albedo = Vec::with_capacity(pixel_count * 4);
    let mut normal = Vec::with_capacity(pixel_count * 4);
    let mut height_map = Vec::with_capacity(pixel_count * 4);
    let mut arm = Vec::with_capacity(pixel_count * 4);
    let texel = 1.0 / TEXTURE_SIZE as f32;
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let u = (x as f32 + 0.5) * texel;
            let v = (y as f32 + 0.5) * texel;
            let height = periodic_height(recipe, u, v);
            let (color, roughness) = surface_palette(recipe, height, u, v);
            albedo.extend_from_slice(&[color[0], color[1], color[2], 255]);
            let wrap = |value: f32| value.rem_euclid(1.0);
            let hx = periodic_height(recipe, wrap(u + texel), v)
                - periodic_height(recipe, wrap(u - texel), v);
            let hy = periodic_height(recipe, u, wrap(v + texel))
                - periodic_height(recipe, u, wrap(v - texel));
            let n = Vec3::new(-hx * 11.0, -hy * 11.0, 1.0).normalize();
            let encoded = ((n + Vec3::ONE) * 127.5).clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]);
            let ao = (218.0 + height.clamp(-0.5, 0.5) * 58.0).clamp(176.0, 255.0) as u8;
            let encoded_height = ((height + 0.5) * 255.0).clamp(0.0, 255.0) as u8;
            height_map.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            arm.extend_from_slice(&[ao, roughness, 0, 255]);
        }
    }
    SurfaceTextureSet {
        albedo: images.add(image_rgba(albedo, true, true, false)),
        normal_gl: images.add(image_rgba(normal, false, true, true)),
        height: images.add(image_rgba(height_map, false, true, true)),
        arm: images.add(image_rgba(arm, false, true, false)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn rgba_palette(image: &Image) -> BTreeSet<[u8; 4]> {
        image
            .data
            .as_deref()
            .expect("generated image data")
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect()
    }

    #[test]
    fn leaf_presets_are_binary_and_use_small_solid_palettes() {
        for recipe in [
            LeafRecipe::WHITE_OAK,
            LeafRecipe::DRY_WHITE_OAK,
            LeafRecipe::HAZEL,
        ] {
            let mut images = Assets::<Image>::default();
            let textures = generate_leaf_textures(&mut images, recipe);
            let opacity = images.get(&textures.opacity).unwrap();
            let opacity_values = rgba_palette(opacity);
            assert!(opacity_values.len() <= 2);
            assert!(
                opacity_values
                    .iter()
                    .all(|pixel| pixel[0] == 0 || pixel[0] == 255)
            );
            assert!(rgba_palette(images.get(&textures.front_albedo).unwrap()).len() <= 3);
            assert!(rgba_palette(images.get(&textures.back_albedo).unwrap()).len() <= 3);
            assert!(rgba_palette(images.get(&textures.height).unwrap()).len() > 16);
            let arm = rgba_palette(images.get(&textures.arm).unwrap());
            assert_eq!(
                arm.iter()
                    .map(|pixel| pixel[1])
                    .collect::<BTreeSet<_>>()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn surface_albedo_and_roughness_are_palette_constrained_but_normals_are_detailed() {
        for recipe in [SurfaceRecipe::OakBark, SurfaceRecipe::Rock] {
            let mut images = Assets::<Image>::default();
            let textures = generate_surface_textures(&mut images, recipe);
            assert!(rgba_palette(images.get(&textures.albedo).unwrap()).len() <= 2);
            let arm = rgba_palette(images.get(&textures.arm).unwrap());
            assert!(
                arm.iter()
                    .map(|pixel| pixel[1])
                    .collect::<BTreeSet<_>>()
                    .len()
                    <= 2
            );
            assert!(rgba_palette(images.get(&textures.normal_gl).unwrap()).len() > 64);
            assert!(rgba_palette(images.get(&textures.height).unwrap()).len() > 32);
            assert!(
                arm.iter()
                    .map(|pixel| pixel[0])
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 16
            );
        }
    }

    #[test]
    fn oak_bark_uses_one_uniform_molded_color() {
        let mut images = Assets::<Image>::default();
        let textures = generate_surface_textures(&mut images, SurfaceRecipe::OakBark);
        assert_eq!(
            rgba_palette(images.get(&textures.albedo).unwrap()),
            BTreeSet::from([[70, 50, 30, 255]])
        );
        assert_eq!(
            rgba_palette(images.get(&textures.arm).unwrap())
                .iter()
                .map(|pixel| pixel[1])
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([241])
        );
    }
}
