use bevy::{asset::Assets, image::Image, math::Vec3};

use crate::{
    LeafRecipe, LeafTextureSet, TEXTURE_SIZE,
    foliage::{LeafMipSemantic, leaf_mipped_image},
};

const EDGE_SAMPLES: u32 = 4;

#[derive(Clone, Copy, Debug)]
enum BeechSide {
    Left,
    Right,
}

impl BeechSide {
    const fn sign(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct BeechVein {
    origin: f32,
    margin: f32,
    reach: f32,
    crenation: f32,
}

const LEFT_VEINS: [BeechVein; 8] = [
    BeechVein {
        origin: 0.075,
        margin: 0.180,
        reach: 0.84,
        crenation: 0.002,
    },
    BeechVein {
        origin: 0.165,
        margin: 0.285,
        reach: 0.88,
        crenation: 0.005,
    },
    BeechVein {
        origin: 0.260,
        margin: 0.390,
        reach: 0.91,
        crenation: 0.008,
    },
    BeechVein {
        origin: 0.360,
        margin: 0.500,
        reach: 0.92,
        crenation: 0.010,
    },
    BeechVein {
        origin: 0.465,
        margin: 0.610,
        reach: 0.91,
        crenation: 0.009,
    },
    BeechVein {
        origin: 0.570,
        margin: 0.715,
        reach: 0.89,
        crenation: 0.008,
    },
    BeechVein {
        origin: 0.675,
        margin: 0.810,
        reach: 0.86,
        crenation: 0.006,
    },
    BeechVein {
        origin: 0.770,
        margin: 0.890,
        reach: 0.80,
        crenation: 0.004,
    },
];

const RIGHT_VEINS: [BeechVein; 8] = [
    BeechVein {
        origin: 0.082,
        margin: 0.188,
        reach: 0.83,
        crenation: 0.002,
    },
    BeechVein {
        origin: 0.174,
        margin: 0.296,
        reach: 0.88,
        crenation: 0.005,
    },
    BeechVein {
        origin: 0.270,
        margin: 0.402,
        reach: 0.90,
        crenation: 0.007,
    },
    BeechVein {
        origin: 0.372,
        margin: 0.512,
        reach: 0.92,
        crenation: 0.009,
    },
    BeechVein {
        origin: 0.478,
        margin: 0.620,
        reach: 0.91,
        crenation: 0.010,
    },
    BeechVein {
        origin: 0.582,
        margin: 0.724,
        reach: 0.88,
        crenation: 0.007,
    },
    BeechVein {
        origin: 0.686,
        margin: 0.820,
        reach: 0.85,
        crenation: 0.006,
    },
    BeechVein {
        origin: 0.780,
        margin: 0.898,
        reach: 0.79,
        crenation: 0.003,
    },
];

#[derive(Clone, Copy)]
struct BeechSample {
    inside: bool,
    vein: bool,
    petiole: bool,
    height: f32,
    back_height: f32,
}

fn veins(side: BeechSide) -> &'static [BeechVein; 8] {
    match side {
        BeechSide::Left => &LEFT_VEINS,
        BeechSide::Right => &RIGHT_VEINS,
    }
}

fn axis(t: f32) -> f32 {
    0.010 * (t - 0.43).powi(2) - 0.003
}

fn rounded_pulse(t: f32, center: f32, radius: f32) -> f32 {
    let distance = ((t - center) / radius).abs();
    if distance >= 1.0 {
        0.0
    } else {
        let smooth = 1.0 - distance * distance;
        smooth * smooth
    }
}

fn side_width(t: f32, side: BeechSide) -> f32 {
    if !(0.0..=1.0).contains(&t) {
        return 0.0;
    }
    // The smooth elliptic lamina carries the silhouette. Vein-linked pulses
    // add mature beech's quiet blunt undulation without periodic sawteeth.
    let envelope =
        0.355 * (t * core::f32::consts::PI).sin().max(0.0).powf(0.64) * (1.08 - 0.16 * t);
    let side_scale = match side {
        BeechSide::Left => 0.993,
        BeechSide::Right => 1.007,
    };
    let margin = veins(side)
        .iter()
        .map(|vein| rounded_pulse(t, vein.margin, 0.043) * vein.crenation)
        .sum::<f32>();
    envelope * side_scale + margin
}

fn rounded_base_minimum_t(x: f32) -> f32 {
    let normalized = (x.abs() / 0.15).clamp(0.0, 1.0);
    0.034 * normalized * normalized
}

fn vein_target_x(side: BeechSide, vein: BeechVein, progress: f32) -> f32 {
    let terminal = side_width(vein.margin, side) * vein.reach;
    let bowed = (progress * core::f32::consts::PI).sin() * 0.010;
    side.sign() * (terminal * progress.powf(0.86) - bowed)
}

fn tissue_relief(u: f32, v: f32) -> f32 {
    let broad = (u * 8.0 + v * 7.0 + 0.8).sin();
    let cross = (u * 6.0 - v * 9.0 + 1.6).cos();
    (broad * 0.62 + cross * 0.38) * 0.0025
}

fn sample(u: f32, v: f32) -> BeechSample {
    let longitudinal = 1.0 - v;
    let t = (longitudinal - 0.075) / 0.85;
    let x = (u - 0.5) - axis(t);
    let petiole = (0.018..0.090).contains(&longitudinal) && x.abs() < 0.0085;
    if !(0.0..=1.0).contains(&t) {
        let height = if petiole { 0.085 } else { 0.0 };
        return BeechSample {
            inside: petiole,
            vein: petiole,
            petiole,
            height,
            back_height: height,
        };
    }

    let side = if x < 0.0 {
        BeechSide::Left
    } else {
        BeechSide::Right
    };
    let width = side_width(t, side);
    let inside_blade = x.abs() <= width && t >= rounded_base_minimum_t(x);
    if !inside_blade && !petiole {
        return BeechSample {
            inside: false,
            vein: false,
            petiole: false,
            height: 0.0,
            back_height: 0.0,
        };
    }

    let midrib_width = 0.0065 - t * 0.0025;
    let mut vein_distance = x.abs();
    let mut vein = x.abs() <= midrib_width;
    let mut corrugation = 0.0;
    for (index, secondary) in veins(side).iter().enumerate() {
        let progress =
            ((t - secondary.origin) / (secondary.margin - secondary.origin)).clamp(0.0, 1.0);
        if progress > 0.0 && progress < 1.0 {
            let target_x = vein_target_x(side, *secondary, progress);
            let distance = (x - target_x).abs();
            vein_distance = vein_distance.min(distance);
            vein |= distance < 0.0032;
            let fold = if index % 2 == 0 { 1.0 } else { -1.0 };
            corrugation += fold
                * (1.0 - distance / 0.050).clamp(0.0, 1.0)
                * (progress * core::f32::consts::PI).sin()
                * 0.008;
            let link_progress = ((progress - 0.38) / 0.45).clamp(0.0, 1.0);
            if link_progress > 0.0 && link_progress < 1.0 {
                let link_x = target_x - side.sign() * 0.018 * link_progress;
                vein_distance = vein_distance.min((x - link_x).abs());
            }
        }
    }

    let transverse = (x / width.max(0.001)).clamp(-1.0, 1.0);
    let blade_dome = (1.0 - transverse * transverse).powf(0.76) * 0.125;
    let longitudinal_dome = (t * core::f32::consts::PI).sin().max(0.0).sqrt();
    let vein_ridge = (1.0 - vein_distance / 0.012).clamp(0.0, 1.0).powi(2) * 0.090;
    let height = if petiole && !inside_blade {
        0.085
    } else {
        (blade_dome * longitudinal_dome + vein_ridge + corrugation + tissue_relief(u, v))
            .clamp(0.010, 0.275)
    };
    let underside_vein_relief = (1.0 - vein_distance / 0.017).clamp(0.0, 1.0).powi(2) * 0.014;
    BeechSample {
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
    let normal = Vec3::new(-(right - left) * 7.0, (up - down) * 7.0, 1.0).normalize();
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
                236
            } else if leaf.vein {
                (233.0 + height * 32.0).min(248.0) as u8
            } else {
                (222.0 + height * 38.0).clamp(222.0, 244.0) as u8
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
    fn silhouette_and_venation_match_mature_european_beech() {
        let width = side_width(0.44, BeechSide::Left) + side_width(0.44, BeechSide::Right);
        assert!((0.62..=0.72).contains(&width), "plate width ratio: {width}");
        assert!(side_width(0.88, BeechSide::Left) > side_width(0.98, BeechSide::Left) * 2.0);
        assert!(sample(0.5, 0.96).petiole);
        assert!(LEFT_VEINS[0].crenation < LEFT_VEINS[3].crenation * 0.3);
        for side in [BeechSide::Left, BeechSide::Right] {
            assert_eq!(veins(side).len(), 8);
            for secondary in veins(side) {
                let progress = 0.90;
                let t = secondary.origin + (secondary.margin - secondary.origin) * progress;
                let u = 0.5 + axis(t) + vein_target_x(side, *secondary, progress);
                let v = 1.0 - (0.075 + t * 0.85);
                assert!(sample(u, v).vein, "secondary at t={t} remains coherent");
            }
        }
        assert_ne!(LEFT_VEINS[3].margin, RIGHT_VEINS[3].margin);
    }

    #[test]
    fn channels_are_palette_bounded_alpha_matched_and_mip_complete() {
        let mut images = Assets::<Image>::default();
        let textures = generate(&mut images, LeafRecipe::BEECH);
        for handle in [
            &textures.opacity,
            &textures.front_albedo,
            &textures.back_albedo,
            &textures.front_normal,
            &textures.back_normal,
            &textures.height,
            &textures.arm,
        ] {
            let image = images.get(handle).unwrap();
            assert_eq!(image.texture_descriptor.mip_level_count, 9);
            let texels = (0..9)
                .map(|level| (TEXTURE_SIZE >> level).pow(2))
                .sum::<u32>();
            assert_eq!(image.data.as_ref().unwrap().len(), (texels * 4) as usize);
        }
        let opacity = images.get(&textures.opacity).unwrap();
        let front = images.get(&textures.front_albedo).unwrap();
        let back = images.get(&textures.back_albedo).unwrap();
        let opacity_pixels = base_bytes(opacity).as_chunks::<4>().0;
        let front_pixels = base_bytes(front).as_chunks::<4>().0;
        let back_pixels = base_bytes(back).as_chunks::<4>().0;
        assert!(
            opacity_pixels
                .iter()
                .all(|pixel| pixel[0] == 0 || pixel[0] == 255)
        );
        assert!(
            opacity_pixels
                .iter()
                .zip(front_pixels)
                .zip(back_pixels)
                .all(|((opacity, front), back)| opacity[0] == front[3] && opacity[0] == back[3])
        );
        for pixels in [front_pixels, back_pixels] {
            let colors = pixels
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect::<BTreeSet<_>>();
            assert_eq!(colors.len(), 2);
        }
        assert_eq!(opacity.data.as_ref().unwrap().last(), Some(&255));
    }

    #[test]
    fn relief_normals_and_generation_are_valid_and_repeatable() {
        let mut first_images = Assets::<Image>::default();
        let first = generate(&mut first_images, LeafRecipe::BEECH);
        let height = base_bytes(first_images.get(&first.height).unwrap());
        assert!(
            height
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| pixel[0])
                .collect::<BTreeSet<_>>()
                .len()
                > 40
        );
        for handle in [&first.front_normal, &first.back_normal] {
            for pixel in base_bytes(first_images.get(handle).unwrap())
                .as_chunks::<4>()
                .0
            {
                let normal = Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32) / 127.5
                    - Vec3::ONE;
                assert!((0.97..=1.03).contains(&normal.length()));
            }
        }
        let mut second_images = Assets::<Image>::default();
        let second = generate(&mut second_images, LeafRecipe::BEECH);
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
