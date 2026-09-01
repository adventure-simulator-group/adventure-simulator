//! Weathered earth-and-lime daub with rare, localized exposed wattle.

use bevy::{
    asset::Assets,
    image::Image,
    math::{FloatExt, Vec3},
    render::render_resource::TextureFormat,
};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};

use super::{SurfaceTextureSet, image_rgba_mipped};

pub const WATTLE_AND_DAUB_TEXTURE_SIZE: u32 = 1024;
pub const WATTLE_AND_DAUB_TILE_METRES: f32 = 1.5;
pub const WATTLE_AND_DAUB_HEIGHT_RANGE_METRES: f32 = 0.012;

const DAUB_COOL: Vec3 = Vec3::new(0.53, 0.48, 0.38);
const DAUB_WARM: Vec3 = Vec3::new(0.66, 0.57, 0.42);
const AGGREGATE_COLOR: Vec3 = Vec3::new(0.42, 0.38, 0.30);
const FIBRE_COLOR: Vec3 = Vec3::new(0.47, 0.40, 0.29);
const WATTLE_COLOR: Vec3 = Vec3::new(0.33, 0.23, 0.13);

#[derive(Clone, Copy, Debug)]
struct DaubSample {
    height: f32,
    albedo: Vec3,
    roughness: f32,
    #[cfg(test)]
    aggregate: f32,
    #[cfg(test)]
    fibre: f32,
    crack: f32,
    #[cfg(test)]
    wattle: f32,
    exposed_cavity: f32,
}

fn hash_unit(x: i32, y: i32, salt: u64) -> f32 {
    let packed = x.rem_euclid(65_536) as u64 | ((y.rem_euclid(65_536) as u64) << 16);
    inclusive_unit_f32(splitmix64(packed ^ salt))
}

fn periodic_hash(x: i32, y: i32, cells_x: i32, cells_y: i32, salt: u64) -> f32 {
    hash_unit(x.rem_euclid(cells_x), y.rem_euclid(cells_y), salt)
}

fn quintic(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn smoothstep(lower: f32, upper: f32, value: f32) -> f32 {
    let t = ((value - lower) / (upper - lower)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn periodic_noise(u: f32, v: f32, cells_x: i32, cells_y: i32, salt: u64) -> f32 {
    let x = u.rem_euclid(1.0) * cells_x as f32;
    let y = v.rem_euclid(1.0) * cells_y as f32;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = quintic(x.fract());
    let ty = quintic(y.fract());
    let lower = periodic_hash(ix, iy, cells_x, cells_y, salt)
        .lerp(periodic_hash(ix + 1, iy, cells_x, cells_y, salt), tx);
    let upper = periodic_hash(ix, iy + 1, cells_x, cells_y, salt)
        .lerp(periodic_hash(ix + 1, iy + 1, cells_x, cells_y, salt), tx);
    lower.lerp(upper, ty) * 2.0 - 1.0
}

fn toroidal_delta(value: f32) -> f32 {
    (value + 0.5).rem_euclid(1.0) - 0.5
}

fn capsule_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    let segment = (end.0 - start.0, end.1 - start.1);
    let relative = (point.0 - start.0, point.1 - start.1);
    let length_squared = segment.0 * segment.0 + segment.1 * segment.1;
    let along =
        ((relative.0 * segment.0 + relative.1 * segment.1) / length_squared).clamp(0.0, 1.0);
    let dx = relative.0 - segment.0 * along;
    let dy = relative.1 - segment.1 * along;
    (dx * dx + dy * dy).sqrt()
}

fn sparse_capsules(
    u: f32,
    v: f32,
    cells: i32,
    salt: u64,
    enabled_threshold: f32,
    half_length_range: (f32, f32),
    radius: f32,
) -> f32 {
    let scaled_x = u.rem_euclid(1.0) * cells as f32;
    let scaled_y = v.rem_euclid(1.0) * cells as f32;
    let base_x = scaled_x.floor() as i32;
    let base_y = scaled_y.floor() as i32;
    let mut coverage = 0.0_f32;
    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let cell_x = base_x + offset_x;
            let cell_y = base_y + offset_y;
            if periodic_hash(cell_x, cell_y, cells, cells, salt ^ 0x91e5) < enabled_threshold {
                continue;
            }
            let center = (
                cell_x as f32
                    + 0.15
                    + periodic_hash(cell_x, cell_y, cells, cells, salt ^ 0x3b71) * 0.70,
                cell_y as f32
                    + 0.15
                    + periodic_hash(cell_x, cell_y, cells, cells, salt ^ 0xc54d) * 0.70,
            );
            let angle =
                periodic_hash(cell_x, cell_y, cells, cells, salt ^ 0x7ad3) * std::f32::consts::TAU;
            let half_length = half_length_range.0
                + periodic_hash(cell_x, cell_y, cells, cells, salt ^ 0xe217)
                    * (half_length_range.1 - half_length_range.0);
            let direction = (angle.cos() * half_length, angle.sin() * half_length);
            let point = (scaled_x, scaled_y);
            let distance = capsule_distance(
                point,
                (center.0 - direction.0, center.1 - direction.1),
                (center.0 + direction.0, center.1 + direction.1),
            );
            coverage = coverage.max(1.0 - smoothstep(radius, radius * 1.7, distance));
        }
    }
    coverage
}

fn aggregate_coverage(u: f32, v: f32) -> f32 {
    sparse_capsules(u, v, 96, 0x7361, 0.94, (0.0, 0.020), 0.070)
}

fn fibre_coverage(u: f32, v: f32) -> f32 {
    sparse_capsules(u, v, 38, 0x19d7, 0.965, (0.12, 0.32), 0.028)
}

fn shrink_crack(u: f32, v: f32) -> f32 {
    sparse_capsules(u, v, 7, 0x52b9, 0.90, (0.040, 0.105), 0.0035)
}

fn exposed_wattle(u: f32, v: f32) -> (f32, f32) {
    let dx = toroidal_delta(u - 0.22);
    let dy = toroidal_delta(v - 0.31);
    let angle = 0.16_f32;
    let cosine = angle.cos();
    let sine = angle.sin();
    let local_x = dx * cosine + dy * sine;
    let local_y = -dx * sine + dy * cosine;
    let theta = (local_y / 0.0075).atan2(local_x / 0.0105);
    let irregular_radius =
        1.0 + (theta * 3.0 + 0.7).sin() * 0.15 + (theta * 5.0 - 0.4).sin() * 0.07;
    let edge_noise = periodic_noise(u, v, 17, 13, 0xc183) * 0.06;
    let elliptical = ((local_x / 0.0105).powi(2) + (local_y / 0.0075).powi(2)).sqrt();
    let cavity = 1.0
        - smoothstep(
            irregular_radius * 0.77 + edge_noise,
            irregular_radius + edge_noise,
            elliptical,
        );

    let rod_distance = capsule_distance((local_x, local_y), (-0.0065, -0.0025), (0.0045, 0.0040));
    let woody_fragment = 1.0 - smoothstep(0.0012, 0.0025, rod_distance);
    let chipped_occlusion = smoothstep(-0.006, 0.003, local_x + local_y * 0.35);
    (cavity, cavity * woody_fragment * chipped_occlusion * 0.72)
}

fn sample_daub(u: f32, v: f32) -> DaubSample {
    let broad = periodic_noise(u, v, 4, 5, 0x39a7);
    let medium = periodic_noise(u, v, 11, 9, 0x7c31);
    let fine = periodic_noise(u, v, 61, 53, 0xe257);
    let warp = periodic_noise(u, v, 3, 4, 0x64d9) * 0.028;
    let smear = periodic_noise(u + warp, v - warp * 0.35, 5, 9, 0x2ab5);
    let trowel_wave_a = (std::f32::consts::TAU
        * (u * 3.0 + v * 1.0 + periodic_noise(u, v, 3, 3, 0xb49d) * 0.18))
        .sin();
    let trowel_wave_b =
        (std::f32::consts::TAU * (u - v * 2.0 + periodic_noise(u, v, 2, 4, 0x8e63) * 0.13)).sin();
    let trowel_mass = trowel_wave_a * 0.68 + trowel_wave_b * 0.32;
    let aggregate = aggregate_coverage(u, v);
    let fibre = fibre_coverage(u, v);
    let crack = shrink_crack(u, v);
    let (exposed_cavity, wattle) = exposed_wattle(u, v);

    let surface = 0.61
        + broad * 0.055
        + medium * 0.025
        + fine * 0.006
        + smear * 0.030
        + trowel_mass * 0.030
        + aggregate * 0.018
        + fibre * 0.012
        - crack * 0.075;
    let height = surface * (1.0 - exposed_cavity) + (0.25 + wattle * 0.16) * exposed_cavity;

    let warm_mix = smoothstep(-0.75, 0.80, broad * 0.68 + medium * 0.32);
    let mut albedo = DAUB_COOL.lerp(DAUB_WARM, warm_mix);
    albedo *= 0.94 + medium * 0.028 + smear * 0.024 + trowel_mass * 0.026;
    albedo = albedo.lerp(AGGREGATE_COLOR, aggregate * 0.55);
    albedo = albedo.lerp(FIBRE_COLOR, fibre * 0.62);
    albedo *= 1.0 - crack * 0.12;
    let cavity_color = Vec3::new(0.39, 0.33, 0.25).lerp(WATTLE_COLOR, wattle * 0.65);
    albedo = albedo.lerp(cavity_color, exposed_cavity * 0.86);

    DaubSample {
        height: height.clamp(0.0, 1.0),
        albedo: albedo.clamp(Vec3::ZERO, Vec3::ONE),
        roughness: (0.87
            + broad * 0.008
            + smear.abs() * 0.012
            + trowel_mass.abs() * 0.010
            + aggregate * 0.040
            + fibre * 0.028
            + crack * 0.018)
            .clamp(0.80, 0.96),
        #[cfg(test)]
        aggregate,
        #[cfg(test)]
        fibre,
        crack,
        #[cfg(test)]
        wattle,
        exposed_cavity,
    }
}

fn height_at(samples: &[DaubSample], x: i32, y: i32) -> f32 {
    let size = WATTLE_AND_DAUB_TEXTURE_SIZE as i32;
    samples[(y.rem_euclid(size) * size + x.rem_euclid(size)) as usize].height
}

fn ambient_visibility(samples: &[DaubSample], x: i32, y: i32) -> f32 {
    let center = height_at(samples, x, y);
    let mut obstruction = 0.0_f32;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        for step in [1, 4, 12, 32] {
            obstruction += ((height_at(samples, x + dx * step, y + dy * step) - center)
                / step as f32)
                .max(0.0);
        }
    }
    (1.0 - obstruction * 2.2).clamp(0.42, 1.0)
}

fn encode_unit(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn generate_wattle_and_daub_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = WATTLE_AND_DAUB_TEXTURE_SIZE;
    let samples = (0..size)
        .flat_map(|y| {
            (0..size).map(move |x| {
                sample_daub(
                    (x as f32 + 0.5) / size as f32,
                    (y as f32 + 0.5) / size as f32,
                )
            })
        })
        .collect::<Vec<_>>();
    let mut albedo = Vec::with_capacity((size * size * 4) as usize);
    let mut normal = Vec::with_capacity(albedo.capacity());
    let mut height = Vec::with_capacity(albedo.capacity());
    let mut arm = Vec::with_capacity(albedo.capacity());
    let metres_per_texel = WATTLE_AND_DAUB_TILE_METRES / size as f32;
    let slope_scale = WATTLE_AND_DAUB_HEIGHT_RANGE_METRES / (2.0 * metres_per_texel);

    for y in 0..size {
        for x in 0..size {
            let sample = samples[(y * size + x) as usize];
            albedo.extend_from_slice(&[
                encode_unit(sample.albedo.x),
                encode_unit(sample.albedo.y),
                encode_unit(sample.albedo.z),
                255,
            ]);
            let dx = height_at(&samples, x as i32 + 1, y as i32)
                - height_at(&samples, x as i32 - 1, y as i32);
            let dy = height_at(&samples, x as i32, y as i32 + 1)
                - height_at(&samples, x as i32, y as i32 - 1);
            let surface_normal = Vec3::new(-dx * slope_scale, -dy * slope_scale, 1.0).normalize();
            normal.extend_from_slice(&[
                encode_unit(surface_normal.x * 0.5 + 0.5),
                encode_unit(surface_normal.y * 0.5 + 0.5),
                encode_unit(surface_normal.z * 0.5 + 0.5),
                255,
            ]);
            let encoded_height = encode_unit(sample.height);
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            let ao = ambient_visibility(&samples, x as i32, y as i32)
                * (1.0 - sample.exposed_cavity * 0.16 - sample.crack * 0.05);
            arm.extend_from_slice(&[encode_unit(ao), encode_unit(sample.roughness), 0, 255]);
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
        let textures = generate_wattle_and_daub_textures(&mut images);
        (images, textures)
    }

    #[test]
    fn sampling_is_deterministic_and_periodic() {
        for (u, v) in [(0.01, 0.17), (0.31, 0.66), (0.74, 0.93), (0.97, 0.04)] {
            let first = sample_daub(u, v);
            let repeated = sample_daub(u + 1.0, v - 1.0);
            assert_eq!(first.height.to_bits(), sample_daub(u, v).height.to_bits());
            assert!((first.height - repeated.height).abs() < 1.0e-4);
            assert!(first.albedo.distance(repeated.albedo) < 1.0e-4);
            assert!((first.roughness - repeated.roughness).abs() < 1.0e-4);
        }
    }

    #[test]
    fn tile_edges_match_in_value_and_first_derivative() {
        let epsilon = 0.25 / WATTLE_AND_DAUB_TEXTURE_SIZE as f32;
        let mut maximum_value_error = 0.0_f32;
        let mut maximum_slope_error = 0.0_f32;
        for index in 0..512 {
            let coordinate = (index as f32 + 0.5) / 512.0;
            for horizontal in [true, false] {
                let sample = |edge: f32| {
                    if horizontal {
                        sample_daub(edge, coordinate)
                    } else {
                        sample_daub(coordinate, edge)
                    }
                };
                let center = sample(0.0);
                let inside = sample(epsilon);
                let wrapped = sample(1.0 - epsilon);
                maximum_value_error = maximum_value_error
                    .max((inside.height - wrapped.height).abs())
                    .max(inside.albedo.distance(wrapped.albedo))
                    .max((inside.roughness - wrapped.roughness).abs());
                let inward_slope = inside.height - center.height;
                let wrapped_slope = center.height - wrapped.height;
                maximum_slope_error = maximum_slope_error.max((inward_slope - wrapped_slope).abs());
            }
        }
        assert!(
            maximum_value_error < 0.012,
            "edge value error: {maximum_value_error}"
        );
        assert!(
            maximum_slope_error < 0.006,
            "edge slope error: {maximum_slope_error}"
        );
    }

    #[test]
    fn physical_scale_and_feature_coverage_are_restrained() {
        assert_eq!(WATTLE_AND_DAUB_TILE_METRES, 1.5);
        assert!((0.008..=0.014).contains(&WATTLE_AND_DAUB_HEIGHT_RANGE_METRES));
        let mut aggregate = 0_usize;
        let mut fibre = 0_usize;
        let mut cracks = 0_usize;
        let mut cavities = 0_usize;
        let mut wattle = 0_usize;
        let sample_count = 512_usize.pow(2);
        for y in 0..512 {
            for x in 0..512 {
                let sample = sample_daub((x as f32 + 0.5) / 512.0, (y as f32 + 0.5) / 512.0);
                aggregate += usize::from(sample.aggregate > 0.5);
                fibre += usize::from(sample.fibre > 0.5);
                cracks += usize::from(sample.crack > 0.5);
                cavities += usize::from(sample.exposed_cavity > 0.5);
                wattle += usize::from(sample.wattle > 0.5);
            }
        }
        let fraction = |count| count as f32 / sample_count as f32;
        assert!(
            (0.0015..=0.035).contains(&fraction(aggregate)),
            "aggregate: {}",
            fraction(aggregate)
        );
        assert!(
            (0.001..=0.020).contains(&fraction(fibre)),
            "fibre: {}",
            fraction(fibre)
        );
        assert!(fraction(cracks) < 0.012, "cracks: {}", fraction(cracks));
        assert!(
            (0.0001..=0.004).contains(&fraction(cavities)),
            "cavities: {}",
            fraction(cavities)
        );
        assert!(fraction(wattle) < fraction(cavities) * 0.45);
    }

    #[test]
    fn generated_channels_are_coherent_nonmetallic_and_mipped() {
        let (images, textures) = generated();
        let expected_levels = WATTLE_AND_DAUB_TEXTURE_SIZE.ilog2() + 1;
        let expected_bytes = (0..expected_levels)
            .map(|level| {
                let level_size = WATTLE_AND_DAUB_TEXTURE_SIZE >> level;
                (level_size * level_size * 4) as usize
            })
            .sum::<usize>();
        for handle in [
            &textures.albedo,
            &textures.normal_gl,
            &textures.height,
            &textures.arm,
        ] {
            let image = images.get(handle).unwrap();
            assert_eq!((image.width(), image.height()), (1024, 1024));
            assert_eq!(image.texture_descriptor.mip_level_count, expected_levels);
            assert_eq!(image.data.as_ref().unwrap().len(), expected_bytes);
        }
        assert_eq!(
            images
                .get(&textures.albedo)
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        let base_bytes = (WATTLE_AND_DAUB_TEXTURE_SIZE.pow(2) * 4) as usize;
        let arm = &images.get(&textures.arm).unwrap().data.as_ref().unwrap()[..base_bytes];
        assert!(arm.iter().skip(2).step_by(4).all(|metallic| *metallic == 0));
        assert!(
            arm.iter()
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 24
        );
        assert!(
            arm.iter()
                .skip(1)
                .step_by(4)
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                > 12
        );
    }
}
