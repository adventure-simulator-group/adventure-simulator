use bevy::{asset::Assets, image::Image, math::Vec3};

use crate::{
    LeafRecipe, LeafTextureSet, TEXTURE_SIZE,
    foliage::{LeafMipSemantic, leaf_mipped_image},
};

const EDGE_SAMPLES: u32 = 4;

#[derive(Clone, Copy, Debug)]
enum HawthornSide {
    Left,
    Right,
}

impl HawthornSide {
    const fn sign(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct WidthLandmark {
    t: f32,
    width: f32,
}

const LEFT_WIDTHS: [WidthLandmark; 8] = [
    WidthLandmark {
        t: 0.00,
        width: 0.050,
    },
    WidthLandmark {
        t: 0.13,
        width: 0.154,
    },
    WidthLandmark {
        t: 0.32,
        width: 0.330,
    },
    WidthLandmark {
        t: 0.47,
        width: 0.116,
    },
    WidthLandmark {
        t: 0.60,
        width: 0.286,
    },
    WidthLandmark {
        t: 0.72,
        width: 0.108,
    },
    WidthLandmark {
        t: 0.84,
        width: 0.196,
    },
    WidthLandmark {
        t: 1.00,
        width: 0.000,
    },
];

const RIGHT_WIDTHS: [WidthLandmark; 8] = [
    WidthLandmark {
        t: 0.00,
        width: 0.052,
    },
    WidthLandmark {
        t: 0.14,
        width: 0.160,
    },
    WidthLandmark {
        t: 0.34,
        width: 0.338,
    },
    WidthLandmark {
        t: 0.48,
        width: 0.120,
    },
    WidthLandmark {
        t: 0.62,
        width: 0.278,
    },
    WidthLandmark {
        t: 0.73,
        width: 0.110,
    },
    WidthLandmark {
        t: 0.85,
        width: 0.190,
    },
    WidthLandmark {
        t: 1.00,
        width: 0.000,
    },
];

#[derive(Clone, Copy)]
struct HawthornVein {
    origin_t: f32,
    target_t: f32,
    target_fraction: f32,
}

const LEFT_VEINS: [HawthornVein; 3] = [
    HawthornVein {
        origin_t: 0.13,
        target_t: 0.32,
        target_fraction: 0.88,
    },
    HawthornVein {
        origin_t: 0.39,
        target_t: 0.60,
        target_fraction: 0.87,
    },
    HawthornVein {
        origin_t: 0.66,
        target_t: 0.84,
        target_fraction: 0.70,
    },
];

const RIGHT_VEINS: [HawthornVein; 3] = [
    HawthornVein {
        origin_t: 0.14,
        target_t: 0.34,
        target_fraction: 0.88,
    },
    HawthornVein {
        origin_t: 0.40,
        target_t: 0.62,
        target_fraction: 0.87,
    },
    HawthornVein {
        origin_t: 0.67,
        target_t: 0.85,
        target_fraction: 0.70,
    },
];

#[derive(Clone, Copy)]
struct HawthornSample {
    inside: bool,
    vein: bool,
    petiole: bool,
    height: f32,
    back_height: f32,
}

fn landmarks(side: HawthornSide) -> &'static [WidthLandmark; 8] {
    match side {
        HawthornSide::Left => &LEFT_WIDTHS,
        HawthornSide::Right => &RIGHT_WIDTHS,
    }
}

fn veins(side: HawthornSide) -> &'static [HawthornVein; 3] {
    match side {
        HawthornSide::Left => &LEFT_VEINS,
        HawthornSide::Right => &RIGHT_VEINS,
    }
}

fn axis(t: f32) -> f32 {
    -0.010 * (t - 0.36).powi(2) + 0.003
}

fn triangular_pulse(t: f32, center: f32, radius: f32) -> f32 {
    (1.0 - (t - center).abs() / radius).clamp(0.0, 1.0)
}

fn tooth_extension(t: f32, side: HawthornSide) -> f32 {
    let shift = match side {
        HawthornSide::Left => -0.003,
        HawthornSide::Right => 0.004,
    };
    [
        (0.265, 0.010),
        (0.375, 0.008),
        (0.555, 0.009),
        (0.655, 0.007),
        (0.825, 0.006),
        (0.895, 0.005),
    ]
    .into_iter()
    .map(|(center, amount)| triangular_pulse(t, center + shift, 0.018) * amount)
    .sum()
}

fn side_width(t: f32, side: HawthornSide) -> f32 {
    if !(0.0..=1.0).contains(&t) {
        return 0.0;
    }
    let points = landmarks(side);
    let segment = points
        .windows(2)
        .find(|pair| (pair[0].t..=pair[1].t).contains(&t))
        .unwrap_or(&points[points.len() - 2..]);
    let linear_progress = (t - segment[0].t) / (segment[1].t - segment[0].t);
    let progress = linear_progress * linear_progress * (3.0 - 2.0 * linear_progress);
    let width = segment[0].width + (segment[1].width - segment[0].width) * progress;
    width + tooth_extension(t, side)
}

fn distance_to_segment(point: Vec3, start: Vec3, end: Vec3) -> f32 {
    let direction = end - start;
    let progress = ((point - start).dot(direction) / direction.length_squared()).clamp(0.0, 1.0);
    point.distance(start + direction * progress)
}

fn tissue_relief(u: f32, v: f32) -> f32 {
    let broad = (u * 8.0 + v * 6.0 + 0.8).sin();
    let cross = (u * 5.0 - v * 9.0 + 1.4).cos();
    (broad * 0.62 + cross * 0.38) * 0.0025
}

fn sample(u: f32, v: f32) -> HawthornSample {
    let longitudinal = 1.0 - v;
    let t = (longitudinal - 0.095) / 0.82;
    let x = (u - 0.5) - axis(t);
    let petiole = (0.020..0.110).contains(&longitudinal) && x.abs() < 0.008;
    if !(0.0..=1.0).contains(&t) {
        let height = if petiole { 0.09 } else { 0.0 };
        return HawthornSample {
            inside: petiole,
            vein: petiole,
            petiole,
            height,
            back_height: height,
        };
    }

    let side = if x < 0.0 {
        HawthornSide::Left
    } else {
        HawthornSide::Right
    };
    let width = side_width(t, side);
    let inside_blade = x.abs() <= width;
    if !inside_blade && !petiole {
        return HawthornSample {
            inside: false,
            vein: false,
            petiole: false,
            height: 0.0,
            back_height: 0.0,
        };
    }

    let point = Vec3::new(x, t, 0.0);
    let midrib_width = 0.0065 - t * 0.0025;
    let mut vein_distance = x.abs();
    let mut vein = x.abs() <= midrib_width;
    let mut corrugation = 0.0;
    for (index, secondary) in veins(side).iter().enumerate() {
        let start = Vec3::new(0.0, secondary.origin_t, 0.0);
        let target = Vec3::new(
            side.sign() * side_width(secondary.target_t, side) * secondary.target_fraction,
            secondary.target_t,
            0.0,
        );
        let distance = distance_to_segment(point, start, target);
        vein_distance = vein_distance.min(distance);
        vein |= distance < 0.0035;
        let direction = target - start;
        let progress =
            ((point - start).dot(direction) / direction.length_squared()).clamp(0.0, 1.0);
        if progress > 0.0 && progress < 1.0 {
            let fold = if index % 2 == 0 { 1.0 } else { -1.0 };
            corrugation += fold
                * (1.0 - distance / 0.050).clamp(0.0, 1.0)
                * (progress * core::f32::consts::PI).sin()
                * 0.010;
        }
    }

    let transverse = (x / width.max(0.001)).clamp(-1.0, 1.0);
    let blade_dome = (1.0 - transverse * transverse).powf(0.68) * 0.125;
    let longitudinal_dome = (t * core::f32::consts::PI).sin().max(0.0).sqrt();
    let central_fold = (1.0 - x.abs() / 0.055).clamp(0.0, 1.0).powi(2) * 0.028;
    let vein_ridge = (1.0 - vein_distance / 0.014).clamp(0.0, 1.0).powi(2) * 0.085;
    let height = if petiole && !inside_blade {
        0.09
    } else {
        (blade_dome * longitudinal_dome
            + central_fold
            + vein_ridge
            + corrugation
            + tissue_relief(u, v))
        .clamp(0.01, 0.30)
    };
    let underside_vein_relief = (1.0 - vein_distance / 0.019).clamp(0.0, 1.0).powi(2) * 0.009;
    HawthornSample {
        inside: true,
        vein: vein || petiole,
        petiole,
        height,
        back_height: height + underside_vein_relief,
    }
}

fn coverage(u: f32, v: f32, texel: f32) -> bool {
    let mut covered = 0;
    for sample_y in 0..EDGE_SAMPLES {
        for sample_x in 0..EDGE_SAMPLES {
            let offset_x = (sample_x as f32 + 0.5) / EDGE_SAMPLES as f32 - 0.5;
            let offset_y = (sample_y as f32 + 0.5) / EDGE_SAMPLES as f32 - 0.5;
            covered += u32::from(sample(u + offset_x * texel, v + offset_y * texel).inside);
        }
    }
    covered * 2 >= EDGE_SAMPLES.pow(2)
}

fn encoded_normal(left: f32, right: f32, down: f32, up: f32) -> [u8; 3] {
    let normal = Vec3::new(-(right - left) * 7.4, (up - down) * 7.4, 1.0).normalize();
    let encoded = ((normal + Vec3::ONE) * 127.5).clamp(Vec3::ZERO, Vec3::splat(255.0));
    [encoded.x as u8, encoded.y as u8, encoded.z as u8]
}

pub(super) fn generate(images: &mut Assets<Image>, recipe: LeafRecipe) -> LeafTextureSet {
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
            let leaf = sample(u, v);
            let inside = coverage(u, v, texel);
            let alpha = if inside { 255 } else { 0 };
            opacity.extend_from_slice(&[alpha; 4]);
            let front_color = if leaf.vein { recipe.vein } else { recipe.blade };
            let back_color = if leaf.vein {
                recipe.vein
            } else {
                recipe.back_blade
            };
            if inside {
                front.extend_from_slice(&[front_color[0], front_color[1], front_color[2], alpha]);
                back.extend_from_slice(&[back_color[0], back_color[1], back_color[2], alpha]);
            } else {
                front.extend_from_slice(&[0; 4]);
                back.extend_from_slice(&[0; 4]);
            }

            let left = sample((u - texel).max(0.0), v);
            let right = sample((u + texel).min(1.0), v);
            let down = sample(u, (v - texel).max(0.0));
            let up = sample(u, (v + texel).min(1.0));
            let front_encoded = encoded_normal(left.height, right.height, down.height, up.height);
            normal_front.extend_from_slice(&[
                front_encoded[0],
                front_encoded[1],
                front_encoded[2],
                255,
            ]);
            let back_encoded = encoded_normal(
                left.back_height,
                right.back_height,
                down.back_height,
                up.back_height,
            );
            normal_back.extend_from_slice(&[
                back_encoded[0],
                255 - back_encoded[1],
                back_encoded[2],
                255,
            ]);

            let height = if inside { leaf.height } else { 0.0 };
            let ao = if !inside {
                255
            } else if leaf.petiole {
                234
            } else if leaf.vein {
                (230.0 + height * 36.0).min(248.0) as u8
            } else {
                (220.0 + height * 40.0).clamp(220.0, 244.0) as u8
            };
            let encoded_height = (height * 255.0).clamp(0.0, 255.0) as u8;
            height_map.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            arm.extend_from_slice(&[ao, recipe.roughness, 0, 255]);
        }
    }

    LeafTextureSet {
        opacity: images.add(leaf_mipped_image(opacity, false, LeafMipSemantic::Coverage)),
        front_albedo: images.add(leaf_mipped_image(
            front,
            true,
            LeafMipSemantic::ColorCoverage,
        )),
        back_albedo: images.add(leaf_mipped_image(
            back,
            true,
            LeafMipSemantic::ColorCoverage,
        )),
        front_normal: images.add(leaf_mipped_image(
            normal_front,
            false,
            LeafMipSemantic::Normal,
        )),
        back_normal: images.add(leaf_mipped_image(
            normal_back,
            false,
            LeafMipSemantic::Normal,
        )),
        height: images.add(leaf_mipped_image(
            height_map,
            false,
            LeafMipSemantic::Scalar,
        )),
        arm: images.add(leaf_mipped_image(arm, false, LeafMipSemantic::Scalar)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn base_bytes(image: &Image) -> &[u8] {
        &image.data.as_ref().unwrap()[..(TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize]
    }

    #[test]
    fn silhouette_has_two_lateral_lobe_pairs_and_deep_open_sinuses() {
        for side in [HawthornSide::Left, HawthornSide::Right] {
            let points = landmarks(side);
            assert!(points[2].width > points[3].width * 2.7);
            assert!(points[4].width > points[5].width * 2.5);
            assert!(points[3].width < points[2].width * 0.5);
            assert!(points[5].width < points[4].width * 0.5);
        }
        assert!(sample(0.5, 0.95).petiole);
    }

    #[test]
    fn sparse_teeth_stay_on_distal_lobe_margins() {
        for side in [HawthornSide::Left, HawthornSide::Right] {
            assert_eq!(tooth_extension(0.47, side), 0.0);
            assert_eq!(tooth_extension(0.72, side), 0.0);
            assert!(tooth_extension(0.265, side) > 0.004);
            assert!(tooth_extension(0.555, side) > 0.004);
        }
    }

    #[test]
    fn each_major_lobe_has_a_directed_secondary_vein() {
        for side in [HawthornSide::Left, HawthornSide::Right] {
            for secondary in veins(side) {
                let start = Vec3::new(0.0, secondary.origin_t, 0.0);
                let target = Vec3::new(
                    side.sign() * side_width(secondary.target_t, side) * secondary.target_fraction,
                    secondary.target_t,
                    0.0,
                );
                let midpoint = start.lerp(target, 0.55);
                let longitudinal = 0.095 + midpoint.y * 0.82;
                let u = 0.5 + axis(midpoint.y) + midpoint.x;
                let v = 1.0 - longitudinal;
                assert!(sample(u, v).vein);
            }
        }
    }

    #[test]
    fn outputs_are_palette_bounded_alpha_matched_normalized_and_mipped() {
        let mut images = Assets::<Image>::default();
        let textures = generate(&mut images, LeafRecipe::HAWTHORN);
        let opacity = base_bytes(images.get(&textures.opacity).unwrap());
        for handle in [&textures.front_albedo, &textures.back_albedo] {
            let image = images.get(handle).unwrap();
            assert_eq!(
                image.texture_descriptor.mip_level_count,
                TEXTURE_SIZE.ilog2() + 1
            );
            let colors = base_bytes(image)
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect::<BTreeSet<_>>();
            assert!(colors.len() <= 2);
            for (index, pixel) in base_bytes(image).as_chunks::<4>().0.iter().enumerate() {
                assert_eq!(pixel[3], opacity[index * 4]);
            }
        }
        for handle in [
            &textures.opacity,
            &textures.front_normal,
            &textures.back_normal,
            &textures.height,
            &textures.arm,
        ] {
            assert_eq!(
                images
                    .get(handle)
                    .unwrap()
                    .texture_descriptor
                    .mip_level_count,
                TEXTURE_SIZE.ilog2() + 1
            );
        }
        for handle in [&textures.front_normal, &textures.back_normal] {
            for pixel in base_bytes(images.get(handle).unwrap()).as_chunks::<4>().0 {
                let normal = Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32) / 127.5
                    - Vec3::ONE;
                assert!((0.97..=1.03).contains(&normal.length()));
            }
        }
    }

    #[test]
    fn generation_is_repeatable() {
        let mut first_images = Assets::<Image>::default();
        let first = generate(&mut first_images, LeafRecipe::HAWTHORN);
        let mut second_images = Assets::<Image>::default();
        let second = generate(&mut second_images, LeafRecipe::HAWTHORN);
        for (first_handle, second_handle) in [
            (&first.opacity, &second.opacity),
            (&first.front_albedo, &second.front_albedo),
            (&first.back_albedo, &second.back_albedo),
            (&first.front_normal, &second.front_normal),
            (&first.back_normal, &second.back_normal),
            (&first.height, &second.height),
            (&first.arm, &second.arm),
        ] {
            assert_eq!(
                first_images.get(first_handle).unwrap().data,
                second_images.get(second_handle).unwrap().data
            );
        }
    }
}
