use super::*;

const TEXTURE_SIZE: u32 = 256;
const OAK_BARK_TEXTURE_SIZE: u32 = 1024;
const OAK_BARK_AO_SIZE: u32 = 512;
const OAK_BARK_AO_DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const OAK_BARK_AO_STEPS: [i32; 4] = [1, 4, 12, 32];
pub(super) const FOREST_SOIL_TEXTURE_SIZE: u32 = 1024;
const FOREST_SOIL_AO_SIZE: u32 = 512;
const FOREST_SOIL_AO_DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const FOREST_SOIL_AO_STEPS: [i32; 4] = [1, 4, 12, 32];
pub(super) const FOREST_SOIL_TILE_METRES: f32 = 2.0;
pub(super) const FOREST_SOIL_HEIGHT_RANGE_METRES: f32 = 0.028;

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

#[derive(Clone, Debug)]
pub(super) struct BarkTextureSet {
    pub(super) height_ao: Handle<Image>,
}

#[derive(Clone, Debug)]
pub(super) struct GroundTextureSet {
    pub(super) height_ao: Handle<Image>,
}

#[derive(Resource, Clone, Debug)]
pub(crate) struct ProceduralEnvironmentAssets {
    pub(super) oak_leaf: LeafTextureSet,
    pub(super) dry_oak_leaf: LeafTextureSet,
    pub(super) hazel_leaf: LeafTextureSet,
    pub(super) blackthorn_leaf: LeafTextureSet,
    pub(super) hawthorn_leaf: LeafTextureSet,
    pub(super) beech_leaf: LeafTextureSet,
    pub(super) oak_bark: BarkTextureSet,
    pub(super) forest_soil: GroundTextureSet,
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

    const BLACKTHORN: Self = Self {
        widest_point: 0.46,
        base_power: 0.82,
        tip_power: 0.76,
        lobe_count: 0.0,
        lobe_depth: 0.0,
        tooth_count: 11.0,
        tooth_depth: 0.028,
        vein_pairs: 6,
        bend: 0.018,
        blade: [61, 103, 42],
        vein: [117, 139, 66],
        back_blade: [77, 111, 56],
        roughness: 220,
    };

    const HAWTHORN: Self = Self {
        widest_point: 0.44,
        base_power: 0.72,
        tip_power: 0.68,
        lobe_count: 3.5,
        lobe_depth: 0.14,
        tooth_count: 9.0,
        tooth_depth: 0.035,
        vein_pairs: 6,
        bend: -0.012,
        blade: [72, 113, 44],
        vein: [132, 151, 68],
        back_blade: [84, 119, 58],
        roughness: 217,
    };

    const BEECH: Self = Self {
        widest_point: 0.47,
        base_power: 0.72,
        tip_power: 0.76,
        lobe_count: 0.0,
        lobe_depth: 0.0,
        tooth_count: 8.0,
        tooth_depth: 0.018,
        vein_pairs: 8,
        bend: 0.022,
        blade: [67, 108, 46],
        vein: [126, 147, 72],
        back_blade: [81, 117, 59],
        roughness: 214,
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
    let started = std::time::Instant::now();
    info!("Generating procedural environment texture assets");
    commands.insert_resource(generate_procedural_environment_assets(&mut images));
    info!(
        elapsed_ms = started.elapsed().as_millis(),
        "Generated procedural environment texture assets"
    );
}

pub(super) fn generate_procedural_environment_assets(
    images: &mut Assets<Image>,
) -> ProceduralEnvironmentAssets {
    ProceduralEnvironmentAssets {
        oak_leaf: generate_leaf_textures(images, LeafRecipe::WHITE_OAK),
        dry_oak_leaf: generate_leaf_textures(images, LeafRecipe::DRY_WHITE_OAK),
        hazel_leaf: generate_leaf_textures(images, LeafRecipe::HAZEL),
        blackthorn_leaf: generate_leaf_textures(images, LeafRecipe::BLACKTHORN),
        hawthorn_leaf: generate_leaf_textures(images, LeafRecipe::HAWTHORN),
        beech_leaf: generate_leaf_textures(images, LeafRecipe::BEECH),
        oak_bark: generate_oak_bark_texture(images),
        forest_soil: generate_forest_soil_texture(images),
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

fn image_rg_mipped(data: Vec<u8>, size: u32, repeat: bool) -> Image {
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
        SurfaceRecipe::OakBark => oak_bark_height(u, v),
        SurfaceRecipe::Rock => {
            (u * tau * 4.0).sin() * (v * tau * 5.0).cos() * 0.24
                + ((u * 1.7 - v) * tau * 13.0).sin() * 0.07
        }
    }
}

const OAK_BARK_TILE_METRES: f32 = 0.5;
const OAK_BARK_HEIGHT_RANGE_METRES: f32 = 0.032;
const OAK_BARK_COLUMNS: i32 = 10;
const OAK_BARK_ROWS: i32 = 6;
const OAK_BARK_FISSURE_WIDTH_MIN: f32 = 0.007;
const OAK_BARK_FISSURE_WIDTH_SPAN: f32 = 0.003;
const OAK_BARK_VALLEY_WIDTH_MIN: f32 = 0.014;
const OAK_BARK_VALLEY_WIDTH_SPAN: f32 = 0.010;
const OAK_BARK_CROWN_HEIGHT_MIN: f32 = 0.10;
const OAK_BARK_CROWN_HEIGHT_SPAN: f32 = 0.12;

fn bark_hash(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn bark_random(cell_x: i32, cell_y: i32, salt: u64) -> f32 {
    let hash = bark_hash(bark_cell_id(cell_x, cell_y) | salt.rotate_left(21));
    ((hash >> 40) as u32) as f32 / 16_777_215.0
}

fn bark_cell_id(cell_x: i32, cell_y: i32) -> u64 {
    let wrapped_x = cell_x.rem_euclid(OAK_BARK_COLUMNS) as u64;
    let wrapped_y = cell_y.rem_euclid(OAK_BARK_ROWS) as u64;
    wrapped_x | (wrapped_y << 8)
}

fn bark_edge_random(first: (i32, i32), second: (i32, i32), salt: u64) -> f32 {
    let first = bark_cell_id(first.0, first.1);
    let second = bark_cell_id(second.0, second.1);
    let (lower, upper) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    let hash = bark_hash(lower | (upper << 16) | salt.rotate_left(37));
    ((hash >> 40) as u32) as f32 / 16_777_215.0
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn bark_segment_modulation(point: Vec2, first: (i32, i32), second: (i32, i32)) -> f32 {
    let tau = core::f32::consts::TAU;
    let frequency = 2.0 + (bark_edge_random(first, second, 0x8d31) * 3.0).floor();
    let phase = bark_edge_random(first, second, 0xc4b7);
    let meander = 0.13 * (tau * (point.x * 3.0 + bark_edge_random(first, second, 0x724d))).sin();
    let wave = (tau * (point.y * frequency + phase + meander)).sin();
    0.48 + 0.52 * smoothstep(-0.55, 0.30, wave)
}

fn oak_bark_major_profile(
    edge_distance: f32,
    core_width: f32,
    valley_width: f32,
    run_strength: f32,
    crown_height: f32,
    shoulder_height: f32,
) -> f32 {
    let core = (-0.5 * (edge_distance / core_width).powi(2)).exp();
    let valley = (-0.5 * (edge_distance / valley_width).powi(2)).exp();
    let shoulder_distance = (edge_distance - valley_width * 1.15) / (valley_width * 0.38);
    let shoulder = (-0.5 * shoulder_distance.powi(2)).exp();
    crown_height + shoulder_height * run_strength * shoulder
        - 0.24 * run_strength * valley
        - (0.035 + 0.24 * run_strength) * core
}

fn oak_bark_crack_x(crack: i32, v: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    let phase = bark_random(crack, 0, 0xd32f);
    let secondary_phase = bark_random(crack, 0, 0x82b5);
    let offset = (bark_random(crack, 0, 0x4c19) - 0.5) * 0.16 / OAK_BARK_COLUMNS as f32;
    crack as f32 / OAK_BARK_COLUMNS as f32
        + offset
        + 0.0065 * (tau * (v * 2.0 + phase)).sin()
        + 0.0028 * (tau * (v * 5.0 + secondary_phase)).sin()
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let axis = end - start;
    let along = ((point - start).dot(axis) / axis.length_squared().max(1.0e-6)).clamp(0.0, 1.0);
    point.distance(start + axis * along)
}

/// Periodic oak relief built from meandering longitudinal crack lines. The
/// intervening strips receive staggered, interrupted transverse closures so
/// they read as raised bark masses rather than a complete Voronoi mosaic.
fn oak_bark_height(u: f32, v: f32) -> f32 {
    let tau = core::f32::consts::TAU;
    let point = Vec2::new(u, v);
    let warp = Vec2::new(
        0.022 * (tau * (v * 2.0 + 0.17)).sin() + 0.009 * (tau * (u * 2.0 - v * 3.0 + 0.41)).sin(),
        0.008 * (tau * (u * 3.0 + 0.63)).sin() + 0.004 * (tau * (u * 5.0 + v * 2.0)).sin(),
    );
    let sample = point + warp;
    let approximate_crack = (sample.x * OAK_BARK_COLUMNS as f32).round() as i32;
    let mut nearest_crack = approximate_crack;
    let mut signed_edge_distance = f32::INFINITY;
    for crack in (approximate_crack - 2)..=(approximate_crack + 2) {
        let signed_distance = sample.x - oak_bark_crack_x(crack, sample.y);
        if signed_distance.abs() < signed_edge_distance.abs() {
            signed_edge_distance = signed_distance;
            nearest_crack = crack;
        }
    }
    let edge_distance = signed_edge_distance.abs();
    let nearest_cell = (nearest_crack - 1, 0);
    let second_cell = (nearest_crack, 0);
    let primary_run = 0.72 + 0.28 * bark_segment_modulation(point, nearest_cell, second_cell);
    let core_width = OAK_BARK_FISSURE_WIDTH_MIN
        + OAK_BARK_FISSURE_WIDTH_SPAN * bark_edge_random(nearest_cell, second_cell, 0x1337);
    let valley_width = OAK_BARK_VALLEY_WIDTH_MIN
        + OAK_BARK_VALLEY_WIDTH_SPAN * bark_edge_random(nearest_cell, second_cell, 0x4f29);
    let column = (sample.x * OAK_BARK_COLUMNS as f32).floor() as i32;
    let row_offset = bark_random(column, 0, 0x8bd1);
    let row_coordinate = sample.y * OAK_BARK_ROWS as f32
        + row_offset
        + 0.13 * (tau * (sample.x * 3.0 + row_offset)).sin();
    let row = row_coordinate.floor() as i32;
    let within_row = row_coordinate - row_coordinate.floor();
    let transverse_distance = within_row.min(1.0 - within_row) / OAK_BARK_ROWS as f32;
    let plate_variation = bark_random(column, row, 0x2d91) - 0.5;
    let crown_height =
        OAK_BARK_CROWN_HEIGHT_MIN + OAK_BARK_CROWN_HEIGHT_SPAN * bark_random(column, row, 0x61e3);
    let shoulder_bias = 0.72
        + 0.56
            * bark_random(
                nearest_crack,
                row + i32::from(signed_edge_distance >= 0.0),
                0xa91f,
            );
    let shoulder_height =
        (0.025 + 0.055 * bark_edge_random(nearest_cell, second_cell, 0xa91f)) * shoulder_bias;
    let macro_relief = oak_bark_major_profile(
        edge_distance,
        core_width,
        valley_width,
        primary_run,
        crown_height,
        shoulder_height,
    );
    let crown = smoothstep(core_width * 0.8, valley_width * 1.6, edge_distance);
    let transverse_width = 0.012 + 0.008 * bark_random(column, row, 0xc713);
    let transverse_core = (-0.5 * (transverse_distance / transverse_width).powi(2)).exp();
    let closure_gate = 0.42
        + 0.58
            * smoothstep(
                -0.58,
                0.20,
                (tau * (sample.x * 7.0 + bark_random(column, row, 0x5a71))).sin(),
            );
    let transverse_relief =
        -(0.12 + 0.10 * bark_random(column, row, 0x731c)) * transverse_core * closure_gate * crown;
    let column_coordinate = sample.x * OAK_BARK_COLUMNS as f32;
    let within_column = column_coordinate - column_coordinate.floor();
    let plate_tilt =
        ((within_column - 0.5) * 0.15 + (within_row - 0.5) * 0.055) * plate_variation * crown;
    let vertical_bulge = (1.0 - ((within_row - 0.5) * 2.0).powi(2)).max(0.0);
    let plate_bulge = (0.035 + 0.055 * bark_random(column, row, 0x19d7)) * vertical_bulge * crown;
    let chip_x = 0.16 + 0.68 * bark_random(column, row, 0xe417);
    let chip_y = 0.10 + 0.80 * bark_random(column, row, 0xb529);
    let chip_distance = Vec2::new(
        (within_column - chip_x) / 0.13,
        (within_row - chip_y) / 0.10,
    )
    .length_squared();
    let chipped_face =
        -(0.050 + 0.080 * bark_random(column, row, 0xf81d)) * (-0.5 * chip_distance).exp() * crown;
    let branch_roll = bark_random(column, row, 0x64ab);
    let branch_side = bark_random(column, row, 0x917d) >= 0.5;
    let branch_start_y = 0.18 + 0.64 * bark_random(column, row, 0x2f43);
    let branch_end_y =
        (branch_start_y + bark_random(column, row, 0xd815) * 0.54 - 0.27).clamp(0.08, 0.92);
    let branch_start_x = if branch_side { 0.98 } else { 0.02 };
    let branch_end_x = if branch_side {
        0.42 + 0.20 * bark_random(column, row, 0x3e29)
    } else {
        0.38 - 0.20 * bark_random(column, row, 0x3e29)
    };
    let plate_point = Vec2::new(
        within_column / OAK_BARK_COLUMNS as f32,
        within_row / OAK_BARK_ROWS as f32,
    );
    let branch_start = Vec2::new(
        branch_start_x / OAK_BARK_COLUMNS as f32,
        branch_start_y / OAK_BARK_ROWS as f32,
    );
    let branch_end = Vec2::new(
        branch_end_x / OAK_BARK_COLUMNS as f32,
        branch_end_y / OAK_BARK_ROWS as f32,
    );
    let branch_distance = distance_to_segment(plate_point, branch_start, branch_end);
    let branch_width = 0.005 + 0.003 * bark_random(column, row, 0xa53f);
    let branch_enabled = smoothstep(0.54, 0.68, branch_roll);
    let terminating_branch = -(0.070 + 0.090 * bark_random(column, row, 0x781b))
        * (-0.5 * (branch_distance / branch_width).powi(2)).exp()
        * branch_enabled
        * crown;
    let notch_y = bark_random(nearest_crack, row, 0x48c1);
    let notch_distance = Vec2::new(
        (edge_distance - valley_width * 1.08) / (valley_width * 0.34),
        (within_row - notch_y) / 0.11,
    )
    .length_squared();
    let fractured_notch =
        -(0.035 + 0.055 * bark_random(nearest_crack, row, 0xbb27)) * (-0.5 * notch_distance).exp();
    let secondary_phase = sample.x * 31.0
        + 0.82 * (tau * (sample.y * 3.0 + 0.17)).sin()
        + 0.21 * (tau * (sample.x * 2.0 - sample.y * 4.0 + 0.31)).sin();
    let secondary_distance = (core::f32::consts::PI * secondary_phase).sin().abs();
    let secondary_fissure = (-0.5 * (secondary_distance / 0.14).powi(2)).exp();
    let secondary_gate = smoothstep(
        -0.24,
        0.46,
        (tau * (sample.y * 5.0 + 0.16 * (tau * sample.x * 3.0).sin())).sin(),
    );
    let secondary_relief = -0.032 * secondary_fissure * secondary_gate * crown;
    let plate_grain = (0.018
        * (tau * (sample.x * 19.0 + 0.24 * (tau * sample.y * 4.0).sin())).sin()
        + 0.008 * (tau * (sample.x * 43.0 - sample.y * 11.0 + 0.37)).sin())
        * crown;
    (macro_relief
        + transverse_relief
        + chipped_face
        + terminating_branch
        + fractured_notch
        + plate_bulge
        + 0.045 * plate_variation * crown
        + plate_tilt
        + secondary_relief
        + plate_grain)
        .clamp(-0.5, 0.38)
}

fn surface_height_range_metres(recipe: SurfaceRecipe) -> f32 {
    match recipe {
        SurfaceRecipe::OakBark => OAK_BARK_HEIGHT_RANGE_METRES,
        SurfaceRecipe::Rock => 0.04,
    }
}

fn surface_tile_metres(recipe: SurfaceRecipe) -> f32 {
    match recipe {
        SurfaceRecipe::OakBark => OAK_BARK_TILE_METRES,
        SurfaceRecipe::Rock => 2.0,
    }
}

fn periodic_sample(field: &[f32], size: u32, x: i32, y: i32) -> f32 {
    let size = size as i32;
    let wrapped_x = x.rem_euclid(size) as usize;
    let wrapped_y = y.rem_euclid(size) as usize;
    field[wrapped_y * size as usize + wrapped_x]
}

fn height_field_ao(recipe: SurfaceRecipe, field: &[f32], size: u32, x: i32, y: i32) -> f32 {
    if matches!(recipe, SurfaceRecipe::Rock) {
        return ((218.0 + periodic_sample(field, size, x, y).clamp(-0.5, 0.5) * 58.0) / 255.0)
            .clamp(0.0, 1.0);
    }
    const DIRECTIONS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    const BASE_STEPS: [i32; 6] = [1, 2, 4, 8, 16, 32];
    let centre = periodic_sample(field, size, x, y) * surface_height_range_metres(recipe);
    let texel_metres = surface_tile_metres(recipe) / size as f32;
    let resolution_scale = size as i32 / TEXTURE_SIZE as i32;
    let mut visibility = 0.0;
    for (direction_x, direction_y) in DIRECTIONS {
        let direction_length =
            ((direction_x * direction_x + direction_y * direction_y) as f32).sqrt();
        let mut maximum_slope = 0.0_f32;
        for base_step in BASE_STEPS {
            let step = base_step * resolution_scale;
            let neighbor =
                periodic_sample(field, size, x + direction_x * step, y + direction_y * step)
                    * surface_height_range_metres(recipe);
            let run = step as f32 * direction_length * texel_metres;
            maximum_slope = maximum_slope.max(((neighbor - centre) / run).max(0.0));
        }
        visibility += 1.0 / (1.0 + maximum_slope * maximum_slope).sqrt();
    }
    (visibility / DIRECTIONS.len() as f32).clamp(0.36, 1.0)
}

fn oak_bark_horizon_ao(field: &[f32], x: i32, y: i32) -> f32 {
    debug_assert_eq!(field.len(), (OAK_BARK_TEXTURE_SIZE.pow(2)) as usize);
    let source_scale = (OAK_BARK_TEXTURE_SIZE / OAK_BARK_AO_SIZE) as i32;
    let source_x = x * source_scale + source_scale / 2;
    let source_y = y * source_scale + source_scale / 2;
    let centre = periodic_sample(field, OAK_BARK_TEXTURE_SIZE, source_x, source_y)
        * surface_height_range_metres(SurfaceRecipe::OakBark);
    let ao_texel_metres = surface_tile_metres(SurfaceRecipe::OakBark) / OAK_BARK_AO_SIZE as f32;
    let mut visibility = 0.0;
    for (direction_x, direction_y) in OAK_BARK_AO_DIRECTIONS {
        let mut maximum_slope = 0.0_f32;
        for ao_step in OAK_BARK_AO_STEPS {
            let source_step = ao_step * source_scale;
            let neighbor = periodic_sample(
                field,
                OAK_BARK_TEXTURE_SIZE,
                source_x + direction_x * source_step,
                source_y + direction_y * source_step,
            ) * surface_height_range_metres(SurfaceRecipe::OakBark);
            let run = ao_step as f32 * ao_texel_metres;
            maximum_slope = maximum_slope.max(((neighbor - centre) / run).max(0.0));
        }
        visibility += 1.0 / (1.0 + maximum_slope * maximum_slope).sqrt();
    }
    (visibility / OAK_BARK_AO_DIRECTIONS.len() as f32).clamp(0.36, 1.0)
}

fn periodic_bilinear_sample(field: &[f32], size: u32, u: f32, v: f32) -> f32 {
    let x = u * size as f32 - 0.5;
    let y = v * size as f32 - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let blend_x = x - x.floor();
    let blend_y = y - y.floor();
    let lower = periodic_sample(field, size, x0, y0)
        .lerp(periodic_sample(field, size, x0 + 1, y0), blend_x);
    let upper = periodic_sample(field, size, x0, y0 + 1)
        .lerp(periodic_sample(field, size, x0 + 1, y0 + 1), blend_x);
    lower.lerp(upper, blend_y)
}

fn oak_bark_local_cavity(field: &[f32], x: i32, y: i32) -> f32 {
    let centre = periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x, y);
    let neighbors = periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x - 1, y)
        + periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x + 1, y)
        + periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x, y - 1)
        + periodic_sample(field, OAK_BARK_TEXTURE_SIZE, x, y + 1);
    let cavity = (neighbors * 0.25 - centre).max(0.0);
    (1.0 - cavity * 1.5).clamp(0.72, 1.0)
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
    debug_assert!(matches!(recipe, SurfaceRecipe::Rock));
    let pixel_count = (TEXTURE_SIZE * TEXTURE_SIZE) as usize;
    let texel = 1.0 / TEXTURE_SIZE as f32;
    let heights = (0..TEXTURE_SIZE)
        .flat_map(|y| {
            (0..TEXTURE_SIZE).map(move |x| {
                periodic_height(recipe, (x as f32 + 0.5) * texel, (y as f32 + 0.5) * texel)
            })
        })
        .collect::<Vec<_>>();
    let mut albedo = Vec::with_capacity(pixel_count * 4);
    let mut normal = Vec::with_capacity(pixel_count * 4);
    let mut height_map = Vec::with_capacity(pixel_count * 4);
    let mut arm = Vec::with_capacity(pixel_count * 4);
    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let u = (x as f32 + 0.5) * texel;
            let v = (y as f32 + 0.5) * texel;
            let height = periodic_sample(&heights, TEXTURE_SIZE, x as i32, y as i32);
            let (color, roughness) = surface_palette(recipe, height, u, v);
            albedo.extend_from_slice(&[color[0], color[1], color[2], 255]);
            let hx = periodic_sample(&heights, TEXTURE_SIZE, x as i32 + 1, y as i32)
                - periodic_sample(&heights, TEXTURE_SIZE, x as i32 - 1, y as i32);
            let hy = periodic_sample(&heights, TEXTURE_SIZE, x as i32, y as i32 + 1)
                - periodic_sample(&heights, TEXTURE_SIZE, x as i32, y as i32 - 1);
            let slope_scale =
                surface_height_range_metres(recipe) / (2.0 * texel * surface_tile_metres(recipe));
            let n = Vec3::new(-hx * slope_scale, -hy * slope_scale, 1.0).normalize();
            let encoded = ((n + Vec3::ONE) * 127.5).clamp(Vec3::ZERO, Vec3::splat(255.0));
            normal.extend_from_slice(&[encoded.x as u8, encoded.y as u8, encoded.z as u8, 255]);
            let ao = (height_field_ao(recipe, &heights, TEXTURE_SIZE, x as i32, y as i32) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
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

fn generate_oak_bark_texture(images: &mut Assets<Image>) -> BarkTextureSet {
    let size = OAK_BARK_TEXTURE_SIZE;
    let pixel_count = (size * size) as usize;
    let texel = 1.0 / size as f32;
    let heights = (0..size)
        .flat_map(|y| {
            (0..size)
                .map(move |x| oak_bark_height((x as f32 + 0.5) * texel, (y as f32 + 0.5) * texel))
        })
        .collect::<Vec<_>>();
    let horizon_ao = (0..OAK_BARK_AO_SIZE)
        .flat_map(|y| {
            let heights = &heights;
            (0..OAK_BARK_AO_SIZE).map(move |x| oak_bark_horizon_ao(heights, x as i32, y as i32))
        })
        .collect::<Vec<_>>();
    let mut height_ao = Vec::with_capacity(pixel_count * 2);
    for y in 0..size {
        for x in 0..size {
            let height = periodic_sample(&heights, size, x as i32, y as i32);
            let encoded_height = ((height + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let broad_visibility = periodic_bilinear_sample(&horizon_ao, OAK_BARK_AO_SIZE, u, v);
            let local_visibility = oak_bark_local_cavity(&heights, x as i32, y as i32);
            let ao = (broad_visibility * local_visibility * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            height_ao.extend_from_slice(&[encoded_height, ao]);
        }
    }
    BarkTextureSet {
        height_ao: images.add(image_rg_mipped(height_ao, size, true)),
    }
}

fn soil_random(cell_x: i32, cell_y: i32, period: i32, salt: u64) -> f32 {
    let wrapped_x = cell_x.rem_euclid(period) as u64;
    let wrapped_y = cell_y.rem_euclid(period) as u64;
    let hash = bark_hash(wrapped_x | (wrapped_y << 16) | salt.rotate_left(33));
    ((hash >> 40) as u32) as f32 / 16_777_215.0
}

fn soil_value_noise(point: Vec2, frequency: i32, salt: u64) -> f32 {
    let scaled = point * frequency as f32;
    let cell = scaled.floor().as_ivec2();
    let local = scaled - cell.as_vec2();
    let blend = local * local * (Vec2::splat(3.0) - local * 2.0);
    let sample = |x: i32, y: i32| soil_random(cell.x + x, cell.y + y, frequency, salt);
    let lower = sample(0, 0).lerp(sample(1, 0), blend.x);
    let upper = sample(0, 1).lerp(sample(1, 1), blend.x);
    lower.lerp(upper, blend.y) * 2.0 - 1.0
}

fn soil_feature_field(
    point: Vec2,
    grid: i32,
    salt: u64,
    density: f32,
    minimum_radius: f32,
    radius_span: f32,
) -> f32 {
    let scaled = point * grid as f32;
    let base_cell = scaled.floor().as_ivec2();
    let mut field = 0.0_f32;
    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let cell = base_cell + IVec2::new(offset_x, offset_y);
            let enabled = soil_random(cell.x, cell.y, grid, salt ^ 0x5de3);
            if enabled > density {
                continue;
            }
            let centre = cell.as_vec2()
                + Vec2::new(
                    0.16 + soil_random(cell.x, cell.y, grid, salt ^ 0x13a7) * 0.68,
                    0.16 + soil_random(cell.x, cell.y, grid, salt ^ 0x91cb) * 0.68,
                );
            let angle = soil_random(cell.x, cell.y, grid, salt ^ 0xc72d) * core::f32::consts::TAU;
            let axis = Vec2::new(angle.cos(), angle.sin());
            let delta = scaled - centre;
            let local = Vec2::new(delta.dot(axis), delta.perp_dot(axis));
            let radius =
                minimum_radius + radius_span * soil_random(cell.x, cell.y, grid, salt ^ 0x27f1);
            let aspect = 0.58 + soil_random(cell.x, cell.y, grid, salt ^ 0xe419) * 0.68;
            let elliptical = Vec2::new(local.x / radius, local.y / (radius * aspect));
            let angle = elliptical.y.atan2(elliptical.x);
            let lobes = 2.0 + (soil_random(cell.x, cell.y, grid, salt ^ 0x6bd3) * 3.0).floor();
            let phase = soil_random(cell.x, cell.y, grid, salt ^ 0x41af) * core::f32::consts::TAU;
            let edge_warp = 1.0 + 0.14 * (angle * lobes + phase).sin();
            let distance = elliptical.length() * edge_warp;
            let mound = (1.0 - smoothstep(0.0, 1.0, distance)).powf(1.35);
            field = field.max(mound);
        }
    }
    field
}

/// Exactly periodic two-metre forest-floor relief. Broad compaction controls
/// where distinct clods survive; smaller aggregate remains sparse enough that
/// the surface reads as earth rather than isotropic multi-octave noise.
fn forest_soil_height(u: f32, v: f32) -> f32 {
    let point = Vec2::new(u, v);
    let warp = Vec2::new(
        soil_value_noise(point, 5, 0x8ae1),
        soil_value_noise(point + Vec2::new(0.37, 0.61), 5, 0x42d7),
    ) * 0.022;
    let sample = point + warp;
    let compaction = smoothstep(-0.34, 0.42, soil_value_noise(sample, 3, 0x1d93));
    let broad =
        soil_value_noise(sample, 7, 0x7c31) * 0.050 + soil_value_noise(sample, 13, 0xb527) * 0.028;
    let hollows = soil_feature_field(sample, 11, 0xd1a9, 0.44, 0.40, 0.36);
    let clods = soil_feature_field(sample, 18, 0x39e7, 0.57, 0.28, 0.29);
    let aggregate = soil_feature_field(sample, 48, 0xa613, 0.22, 0.16, 0.20);
    let granular = soil_value_noise(sample, 79, 0xf28b) * 0.015;
    let loose_soil = 1.0 - compaction * 0.72;
    (broad - hollows * 0.14
        + clods * 0.25 * loose_soil
        + aggregate * 0.075 * (0.55 + loose_soil * 0.45)
        + granular * (0.38 + loose_soil * 0.62))
        .clamp(-0.42, 0.46)
}

fn forest_soil_horizon_ao(field: &[f32], x: i32, y: i32) -> f32 {
    let source_scale = (FOREST_SOIL_TEXTURE_SIZE / FOREST_SOIL_AO_SIZE) as i32;
    let source_x = x * source_scale + source_scale / 2;
    let source_y = y * source_scale + source_scale / 2;
    let centre = periodic_sample(field, FOREST_SOIL_TEXTURE_SIZE, source_x, source_y)
        * FOREST_SOIL_HEIGHT_RANGE_METRES;
    let ao_texel_metres = FOREST_SOIL_TILE_METRES / FOREST_SOIL_AO_SIZE as f32;
    let mut visibility = 0.0;
    for (direction_x, direction_y) in FOREST_SOIL_AO_DIRECTIONS {
        let mut maximum_slope = 0.0_f32;
        for ao_step in FOREST_SOIL_AO_STEPS {
            let source_step = ao_step * source_scale;
            let neighbor = periodic_sample(
                field,
                FOREST_SOIL_TEXTURE_SIZE,
                source_x + direction_x * source_step,
                source_y + direction_y * source_step,
            ) * FOREST_SOIL_HEIGHT_RANGE_METRES;
            let run = ao_step as f32 * ao_texel_metres;
            maximum_slope = maximum_slope.max(((neighbor - centre) / run).max(0.0));
        }
        visibility += 1.0 / (1.0 + maximum_slope * maximum_slope).sqrt();
    }
    (visibility / FOREST_SOIL_AO_DIRECTIONS.len() as f32).clamp(0.55, 1.0)
}

fn forest_soil_local_cavity(field: &[f32], x: i32, y: i32) -> f32 {
    let centre = periodic_sample(field, FOREST_SOIL_TEXTURE_SIZE, x, y);
    let neighbors = periodic_sample(field, FOREST_SOIL_TEXTURE_SIZE, x - 1, y)
        + periodic_sample(field, FOREST_SOIL_TEXTURE_SIZE, x + 1, y)
        + periodic_sample(field, FOREST_SOIL_TEXTURE_SIZE, x, y - 1)
        + periodic_sample(field, FOREST_SOIL_TEXTURE_SIZE, x, y + 1);
    let cavity = (neighbors * 0.25 - centre).max(0.0);
    (1.0 - cavity * 2.2).clamp(0.78, 1.0)
}

fn generate_forest_soil_texture(images: &mut Assets<Image>) -> GroundTextureSet {
    let size = FOREST_SOIL_TEXTURE_SIZE;
    let heights = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                forest_soil_height(
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                )
            })
        })
        .collect::<Vec<_>>();
    let horizon_ao = (0..FOREST_SOIL_AO_SIZE)
        .flat_map(|y| {
            let heights = &heights;
            (0..FOREST_SOIL_AO_SIZE)
                .map(move |x| forest_soil_horizon_ao(heights, x as i32, y as i32))
        })
        .collect::<Vec<_>>();
    let mut height_ao = Vec::with_capacity((size * size * 2) as usize);
    for y in 0..size {
        for x in 0..size {
            let height = periodic_sample(&heights, size, x as i32, y as i32);
            let encoded_height = ((height + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let broad_visibility = periodic_bilinear_sample(&horizon_ao, FOREST_SOIL_AO_SIZE, u, v);
            let local_visibility = forest_soil_local_cavity(&heights, x as i32, y as i32);
            let ao = (broad_visibility * local_visibility * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            height_ao.extend_from_slice(&[encoded_height, ao]);
        }
    }
    GroundTextureSet {
        height_ao: images.add(image_rg_mipped(height_ao, size, true)),
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
            LeafRecipe::BLACKTHORN,
            LeafRecipe::HAWTHORN,
            LeafRecipe::BEECH,
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
        let mut images = Assets::<Image>::default();
        let textures = generate_surface_textures(&mut images, SurfaceRecipe::Rock);
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

    #[test]
    fn oak_bark_is_one_specialized_1024_texture_with_a_complete_mip_chain() {
        let mut images = Assets::<Image>::default();
        let textures = generate_oak_bark_texture(&mut images);
        assert_eq!(images.len(), 1);
        let image = images.get(&textures.height_ao).unwrap();
        assert_eq!(image.width(), OAK_BARK_TEXTURE_SIZE);
        assert_eq!(image.height(), OAK_BARK_TEXTURE_SIZE);
        assert_eq!(
            image.texture_descriptor.mip_level_count,
            OAK_BARK_TEXTURE_SIZE.ilog2() + 1
        );
        let mip_texels = (0..image.texture_descriptor.mip_level_count)
            .map(|level| (OAK_BARK_TEXTURE_SIZE >> level).pow(2))
            .sum::<u32>();
        assert_eq!(
            image.data.as_ref().unwrap().len(),
            (mip_texels * 2) as usize
        );
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rg8Unorm);
        assert_eq!(OAK_BARK_AO_SIZE, OAK_BARK_TEXTURE_SIZE / 2);
        let old_horizon_samples = OAK_BARK_TEXTURE_SIZE.pow(2) * 8 * 6;
        let reduced_horizon_samples = OAK_BARK_AO_SIZE.pow(2)
            * OAK_BARK_AO_DIRECTIONS.len() as u32
            * OAK_BARK_AO_STEPS.len() as u32;
        assert_eq!(old_horizon_samples / reduced_horizon_samples, 12);
        let data = image.data.as_ref().unwrap();
        let first_mip_offset = (OAK_BARK_TEXTURE_SIZE * OAK_BARK_TEXTURE_SIZE * 2) as usize;
        for channel in 0..2_usize {
            let source = [
                data[channel],
                data[2 + channel],
                data[(OAK_BARK_TEXTURE_SIZE * 2) as usize + channel],
                data[(OAK_BARK_TEXTURE_SIZE * 2 + 2) as usize + channel],
            ];
            let expected = ((source.iter().map(|value| *value as u32).sum::<u32>() + 2) / 4) as u8;
            assert_eq!(data[first_mip_offset + channel], expected);
        }
    }

    #[test]
    fn forest_soil_is_one_packed_1024_texture_with_a_complete_mip_chain() {
        let mut images = Assets::<Image>::default();
        let textures = generate_forest_soil_texture(&mut images);
        assert_eq!(images.len(), 1);
        let image = images.get(&textures.height_ao).unwrap();
        assert_eq!((image.width(), image.height()), (1024, 1024));
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rg8Unorm);
        assert_eq!(image.texture_descriptor.mip_level_count, 11);
        let mip_texels = (0..11)
            .map(|level| (FOREST_SOIL_TEXTURE_SIZE >> level).pow(2))
            .sum::<u32>();
        assert_eq!(
            image.data.as_ref().unwrap().len(),
            (mip_texels * 2) as usize
        );
        assert_eq!(FOREST_SOIL_AO_SIZE, FOREST_SOIL_TEXTURE_SIZE / 2);
    }

    #[test]
    fn forest_soil_height_is_periodic_deterministic_and_physically_scaled() {
        for (u, v) in [(0.0, 0.13), (0.07, 0.61), (0.48, 0.94), (0.91, 0.22)] {
            let height = forest_soil_height(u, v);
            assert_eq!(height.to_bits(), forest_soil_height(u, v).to_bits());
            assert!((height - forest_soil_height(u + 1.0, v)).abs() < 1.0e-5);
            assert!((height - forest_soil_height(u, v + 1.0)).abs() < 1.0e-5);
        }
        let values = (0..128)
            .flat_map(|y| {
                (0..128).map(move |x| forest_soil_height(x as f32 / 128.0, y as f32 / 128.0))
            })
            .collect::<Vec<_>>();
        let minimum = values.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(minimum < -0.08, "minimum forest-soil height: {minimum}");
        assert!(maximum > 0.16, "maximum forest-soil height: {maximum}");
        assert!((1.5..=2.5).contains(&FOREST_SOIL_TILE_METRES));
        assert!((0.020..=0.035).contains(&FOREST_SOIL_HEIGHT_RANGE_METRES));
        assert!(FOREST_SOIL_TILE_METRES / (FOREST_SOIL_TEXTURE_SIZE as f32) < 0.0021);
    }

    #[test]
    fn forest_soil_ao_combines_half_resolution_horizons_with_local_cavities() {
        let flat = vec![0.0; FOREST_SOIL_TEXTURE_SIZE.pow(2) as usize];
        assert_eq!(forest_soil_horizon_ao(&flat, 17, 29), 1.0);
        assert_eq!(forest_soil_local_cavity(&flat, 17, 29), 1.0);
        let mut sharp_cavity = flat;
        sharp_cavity[(29 * FOREST_SOIL_TEXTURE_SIZE + 17) as usize] = -0.5;
        assert_eq!(forest_soil_local_cavity(&sharp_cavity, 17, 29), 0.78);
        let horizon_samples = FOREST_SOIL_AO_SIZE.pow(2)
            * FOREST_SOIL_AO_DIRECTIONS.len() as u32
            * FOREST_SOIL_AO_STEPS.len() as u32;
        assert_eq!(horizon_samples, 4_194_304);
    }

    #[test]
    fn oak_bark_height_is_periodic_deterministic_and_deeply_fissured() {
        let samples = [(0.0, 0.13), (0.07, 0.61), (0.48, 0.94), (0.91, 0.22)];
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for (u, v) in samples {
            let height = oak_bark_height(u, v);
            assert_eq!(height.to_bits(), oak_bark_height(u, v).to_bits());
            assert!((height - oak_bark_height(u + 1.0, v)).abs() < 1.0e-5);
            assert!((height - oak_bark_height(u, v + 1.0)).abs() < 1.0e-5);
        }
        for y in 0..96 {
            for x in 0..96 {
                let height = oak_bark_height(x as f32 / 96.0, y as f32 / 96.0);
                minimum = minimum.min(height);
                maximum = maximum.max(height);
            }
        }
        assert!(minimum < -0.39, "minimum oak bark height: {minimum}");
        assert!(maximum > 0.025, "maximum oak bark height: {maximum}");
        assert!(maximum - minimum > 0.5);
    }

    #[test]
    fn oak_bark_plate_and_fissure_scale_stays_short_and_narrow() {
        let nominal_width = OAK_BARK_TILE_METRES / OAK_BARK_COLUMNS as f32;
        let nominal_height = OAK_BARK_TILE_METRES / OAK_BARK_ROWS as f32;
        let minimum_fissure = OAK_BARK_TILE_METRES * OAK_BARK_FISSURE_WIDTH_MIN;
        let maximum_fissure =
            OAK_BARK_TILE_METRES * (OAK_BARK_FISSURE_WIDTH_MIN + OAK_BARK_FISSURE_WIDTH_SPAN);
        assert!((0.04..=0.06).contains(&nominal_width));
        assert!((0.06..=0.10).contains(&nominal_height));
        assert!((0.003..=0.005).contains(&minimum_fissure));
        assert!((0.003..=0.005).contains(&maximum_fissure));
    }

    #[test]
    fn oak_bark_major_profile_has_a_broad_valley_and_a_raised_crown() {
        let core_width = OAK_BARK_FISSURE_WIDTH_MIN;
        let valley_width = OAK_BARK_VALLEY_WIDTH_MIN;
        let edge = oak_bark_major_profile(0.0, core_width, valley_width, 1.0, 0.14, 0.05);
        let shoulder = oak_bark_major_profile(
            valley_width * 1.15,
            core_width,
            valley_width,
            1.0,
            0.14,
            0.05,
        );
        let crown = oak_bark_major_profile(
            valley_width * 2.5,
            core_width,
            valley_width,
            1.0,
            0.14,
            0.05,
        );

        assert!(edge < -0.35, "major fissure floor: {edge}");
        assert!(shoulder > edge + 0.25, "major fissure shoulder: {shoulder}");
        assert!(crown > 0.10, "raised plate crown: {crown}");
        let physical_valley_width = OAK_BARK_TILE_METRES * valley_width;
        assert!((0.007..=0.012).contains(&physical_valley_width));
    }

    #[test]
    fn oak_bark_primary_cracks_meander_periodically_without_crossing_columns() {
        for crack in 0..OAK_BARK_COLUMNS {
            let mut minimum = f32::INFINITY;
            let mut maximum = f32::NEG_INFINITY;
            for sample in 0..128 {
                let v = sample as f32 / 128.0;
                let x = oak_bark_crack_x(crack, v);
                assert!((x - oak_bark_crack_x(crack, v + 1.0)).abs() < 1.0e-5);
                minimum = minimum.min(x);
                maximum = maximum.max(x);
            }
            assert!(maximum - minimum > 0.006);
            let next = oak_bark_crack_x(crack + 1, 0.37);
            assert!(next - oak_bark_crack_x(crack, 0.37) > 0.06);
        }
    }

    #[test]
    fn oak_bark_terminating_cracks_are_sparse_finite_segments() {
        assert_eq!(distance_to_segment(Vec2::ZERO, Vec2::ZERO, Vec2::X), 0.0);
        assert!(
            (distance_to_segment(Vec2::new(2.0, 1.0), Vec2::ZERO, Vec2::X) - 2.0_f32.sqrt()).abs()
                < 1.0e-6
        );

        let enabled = (0..OAK_BARK_COLUMNS)
            .flat_map(|column| (0..OAK_BARK_ROWS).map(move |row| (column, row)))
            .filter(|(column, row)| bark_random(*column, *row, 0x64ab) > 0.54)
            .count();
        assert!(
            (12..=38).contains(&enabled),
            "enabled terminating cracks: {enabled}"
        );
    }

    #[test]
    fn oak_bark_primary_fissure_depth_varies_without_breaking_edge_continuity() {
        let first = (2, 1);
        let second = (3, 1);
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for index in 0..256 {
            let point = Vec2::new(0.17, index as f32 / 256.0);
            let modulation = bark_segment_modulation(point, first, second);
            assert_eq!(
                modulation.to_bits(),
                bark_segment_modulation(point, second, first).to_bits()
            );
            assert!(
                (modulation - bark_segment_modulation(point + Vec2::ONE, first, second)).abs()
                    < 1.0e-5
            );
            minimum = minimum.min(modulation);
            maximum = maximum.max(modulation);
        }
        assert!((0.47..=0.50).contains(&minimum));
        assert!(maximum > 0.99);
    }

    #[test]
    fn oak_bark_ao_uses_reduced_neighboring_horizons_and_full_resolution_cavities() {
        let heights = (0..OAK_BARK_TEXTURE_SIZE)
            .flat_map(|y| {
                (0..OAK_BARK_TEXTURE_SIZE).map(move |x| {
                    oak_bark_height(
                        (x as f32 + 0.5) / OAK_BARK_TEXTURE_SIZE as f32,
                        (y as f32 + 0.5) / OAK_BARK_TEXTURE_SIZE as f32,
                    )
                })
            })
            .collect::<Vec<_>>();
        let visibility = (0..OAK_BARK_AO_SIZE as i32)
            .step_by(16)
            .flat_map(|y| {
                let heights = &heights;
                (0..OAK_BARK_AO_SIZE as i32)
                    .step_by(16)
                    .map(move |x| oak_bark_horizon_ao(heights, x, y))
            })
            .collect::<Vec<_>>();
        let minimum = visibility.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = visibility.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(minimum < 0.72, "minimum horizon visibility: {minimum}");
        assert!(maximum > 0.92, "maximum horizon visibility: {maximum}");

        let flat = vec![0.0; OAK_BARK_TEXTURE_SIZE.pow(2) as usize];
        assert_eq!(oak_bark_local_cavity(&flat, 17, 29), 1.0);
        let mut sharp_cavity = flat;
        sharp_cavity[(29 * OAK_BARK_TEXTURE_SIZE + 17) as usize] = -0.5;
        assert_eq!(oak_bark_local_cavity(&sharp_cavity, 17, 29), 0.72);
    }
}
