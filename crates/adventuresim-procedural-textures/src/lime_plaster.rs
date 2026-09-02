use super::*;

pub const LIME_PLASTER_TEXTURE_SIZE: u32 = 1024;
pub const LIME_PLASTER_TILE_METRES: f32 = 1.0;
pub const LIME_PLASTER_HEIGHT_RANGE_METRES: f32 = 0.004;
pub const LIME_PLASTER_REFERENCE_SRGB: [f32; 3] = [0.745, 0.710, 0.630];

const PLASTER_BASE: Vec3 = Vec3::from_array(LIME_PLASTER_REFERENCE_SRGB);
const PLASTER_WARM_FLECK: Vec3 = Vec3::new(0.752, 0.714, 0.634);
const PLASTER_COOL_FLECK: Vec3 = Vec3::new(0.738, 0.706, 0.626);
const PLASTER_ALBEDO_CELLS_PER_METRE: i32 = 256;
const PLASTER_FLECK_FRACTION: f32 = 0.03;
const PLASTER_HEIGHT_HALF_RANGE_METRES: f32 = LIME_PLASTER_HEIGHT_RANGE_METRES * 0.5;
const TROWEL_BODY_HEIGHT_METRES: f32 = 0.002;
const TROWEL_EDGE_HEIGHT_METRES: f32 = 0.000_28;
const SAND_FLOAT_HEIGHT_METRES: f32 = 0.000_40;
const FINE_AGGREGATE_HEIGHT_METRES: f32 = 0.000_28;
const OBLIQUE_TOOL_MARK_HEIGHT_METRES: f32 = 0.000_32;
const EXPOSED_AGGREGATE_HEIGHT_METRES: f32 = 0.000_20;
const RARE_PULL_DEPTH_METRES: f32 = 0.001;

#[derive(Clone, Copy, Debug)]
pub(super) struct LimePlasterSample {
    pub height: f32,
    pub ao: f32,
    pub roughness: f32,
    pub albedo: Vec3,
    #[cfg(test)]
    pub cavity: f32,
}

fn hash_grid(x: i32, y: i32, cells: i32, salt: u64) -> f32 {
    let x = x.rem_euclid(cells) as u64;
    let y = y.rem_euclid(cells) as u64;
    unit_hash(splitmix64(x | (y << 16) | salt.rotate_left(33)))
}

fn quintic(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

fn periodic_noise(u: f32, v: f32, cells: i32, salt: u64) -> f32 {
    let x = u * cells as f32;
    let y = v * cells as f32;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = quintic(x - x.floor());
    let ty = quintic(y - y.floor());
    let lower = hash_grid(ix, iy, cells, salt).lerp(hash_grid(ix + 1, iy, cells, salt), tx);
    let upper = hash_grid(ix, iy + 1, cells, salt).lerp(hash_grid(ix + 1, iy + 1, cells, salt), tx);
    lower.lerp(upper, ty) * 2.0 - 1.0
}

fn wrapped_offset(value: f32) -> f32 {
    (value + 0.5).rem_euclid(1.0) - 0.5
}

fn cellular_feature(u: f32, v: f32, cells: i32, salt: u64, enabled_threshold: f32) -> (f32, f32) {
    let scaled_x = u * cells as f32;
    let scaled_y = v * cells as f32;
    let cell_x = scaled_x.floor() as i32;
    let cell_y = scaled_y.floor() as i32;
    let mut nearest = f32::INFINITY;
    let mut identity = 0.0;
    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let candidate_x = cell_x + offset_x;
            let candidate_y = cell_y + offset_y;
            let enabled = hash_grid(candidate_x, candidate_y, cells, salt ^ 0x61d3);
            if enabled < enabled_threshold {
                continue;
            }
            let site_x = candidate_x as f32
                + 0.16
                + hash_grid(candidate_x, candidate_y, cells, salt ^ 0x8a4f) * 0.68;
            let site_y = candidate_y as f32
                + 0.16
                + hash_grid(candidate_x, candidate_y, cells, salt ^ 0xc279) * 0.68;
            let dx = wrapped_offset((scaled_x - site_x) / cells as f32) * cells as f32;
            let dy = wrapped_offset((scaled_y - site_y) / cells as f32) * cells as f32;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance < nearest {
                nearest = distance;
                identity = hash_grid(candidate_x, candidate_y, cells, salt ^ 0x3e95);
            }
        }
    }
    (nearest, identity)
}

fn oblique_micro_variation(u: f32, v: f32) -> f32 {
    let warp_x = periodic_noise(u, v, 5, 0x47b9) * 0.024;
    let warp_y = periodic_noise(u, v, 7, 0xd263) * 0.024;
    let diagonal = periodic_noise(u + v + warp_x, v - u + warp_y, 19, 0x8e31);
    let cross_diagonal = periodic_noise(u * 2.0 + v + warp_y, u - v * 2.0 + warp_x, 31, 0x35ad);
    diagonal * 0.62 + cross_diagonal * 0.38
}

fn slope_adjusted_roughness(base: f32, physical_slope: f32) -> f32 {
    (base + smoothstep(0.035, 0.28, physical_slope) * 0.045).clamp(0.74, 0.94)
}

fn plaster_albedo(u: f32, v: f32) -> Vec3 {
    let cell_x = (u * PLASTER_ALBEDO_CELLS_PER_METRE as f32).floor() as i32;
    let cell_y = (v * PLASTER_ALBEDO_CELLS_PER_METRE as f32).floor() as i32;
    let mineral = hash_grid(cell_x, cell_y, PLASTER_ALBEDO_CELLS_PER_METRE, 0x2f49);
    if mineral < PLASTER_FLECK_FRACTION * 0.5 {
        PLASTER_COOL_FLECK
    } else if mineral > 1.0 - PLASTER_FLECK_FRACTION * 0.5 {
        PLASTER_WARM_FLECK
    } else {
        PLASTER_BASE
    }
}

pub(super) fn lime_plaster_sample(u: f32, v: f32) -> LimePlasterSample {
    // Lime plaster is built from overlapping, slightly oblique trowel passes.
    // Keep the application gesture in relief rather than turning metre-scale
    // value noise into baked clouds in the base colour.
    let pass_warp = periodic_noise(u, v, 9, 0x9c31) * 0.035;
    let pass_phase = u * 7.0 + v + pass_warp;
    let pass = (core::f32::consts::TAU * pass_phase).sin();
    let pass_edge = (core::f32::consts::TAU * (pass_phase * 2.0 + 0.17)).sin();
    let trowel =
        pass * 0.17 + pass_edge * 0.055 + periodic_noise(u * 2.0 + v, v, 23, 0x537b) * 0.21;
    // A plasterer's float leaves shallow blade-edge tracks inside the broader
    // sweep. These must remain physical relief: at roughly 25-40 mm spacing,
    // a sub-0.2 mm edge survives a close tactical view without becoming a
    // painted stripe or roughcast pebble.
    let edge_warp = periodic_noise(u, v, 11, 0x49e3) * 0.045;
    let edge_phase = u * 29.0 + v * 7.0 + edge_warp;
    let trowel_edge = (core::f32::consts::TAU * edge_phase).sin();
    let sand = periodic_noise(u, v, 73, 0xa8d5);
    let fine_aggregate = periodic_noise(u, v, 181, 0xb74d);

    let (aggregate_distance, aggregate_identity) = cellular_feature(u, v, 128, 0x63af, 0.82);
    let aggregate_radius = 0.10 + aggregate_identity * 0.10;
    let aggregate = 1.0
        - smoothstep(
            aggregate_radius,
            aggregate_radius + 0.085,
            aggregate_distance,
        );

    let (cavity_distance, cavity_identity) = cellular_feature(u, v, 96, 0x91c7, 0.965);
    let cavity_radius = 0.025 + cavity_identity * 0.035;
    let cavity = 1.0 - smoothstep(cavity_radius, cavity_radius + 0.035, cavity_distance);

    let micro_variation = oblique_micro_variation(u, v);
    let height_metres = trowel * TROWEL_BODY_HEIGHT_METRES
        + trowel_edge * TROWEL_EDGE_HEIGHT_METRES
        + sand * SAND_FLOAT_HEIGHT_METRES
        + fine_aggregate * FINE_AGGREGATE_HEIGHT_METRES
        + micro_variation * OBLIQUE_TOOL_MARK_HEIGHT_METRES
        + aggregate * EXPOSED_AGGREGATE_HEIGHT_METRES
        - cavity * RARE_PULL_DEPTH_METRES;
    let height = (height_metres / PLASTER_HEIGHT_HALF_RANGE_METRES).clamp(-1.0, 1.0);
    // A nearly uniform lime matrix carries sparse mineral flecks at a physical
    // scale below four millimetres. The old quantized smooth noise formed
    // high-contrast, rounded 3-10 cm islands that read as stones. Trowel work,
    // aggregate, and cavities remain exclusively in relief and AO.
    let albedo = plaster_albedo(u, v);

    let occlusion = (cavity * 0.14 + (-height).max(0.0) * 0.025).clamp(0.0, 0.18);
    let ao = (1.0 - occlusion).clamp(0.82, 1.0);
    let roughness = (0.805
        + (micro_variation + 1.0) * 0.018
        + aggregate * 0.050
        + cavity * 0.030
        + fine_aggregate.max(0.0) * 0.018
        - trowel.max(0.0) * 0.018)
        .clamp(0.74, 0.94);
    LimePlasterSample {
        height,
        ao,
        roughness,
        albedo,
        #[cfg(test)]
        cavity,
    }
}

fn encode_unit(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn generate_lime_plaster_textures(images: &mut Assets<Image>) -> SurfaceTextureSet {
    let size = LIME_PLASTER_TEXTURE_SIZE;
    let pixel_count = size.pow(2) as usize;
    let mut albedo = Vec::with_capacity(pixel_count * 4);
    let mut normal = Vec::with_capacity(pixel_count * 4);
    let mut height = Vec::with_capacity(pixel_count * 4);
    let mut arm = Vec::with_capacity(pixel_count * 4);
    let texel_metres = LIME_PLASTER_TILE_METRES / size as f32;
    let uv_step = 1.0 / size as f32;

    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            let sample = lime_plaster_sample(u, v);
            let left = lime_plaster_sample(u - uv_step, v).height;
            let right = lime_plaster_sample(u + uv_step, v).height;
            let down = lime_plaster_sample(u, v - uv_step).height;
            let up = lime_plaster_sample(u, v + uv_step).height;
            let dh_dx =
                (right - left) * LIME_PLASTER_HEIGHT_RANGE_METRES * 0.5 / (2.0 * texel_metres);
            let dh_dy = (up - down) * LIME_PLASTER_HEIGHT_RANGE_METRES * 0.5 / (2.0 * texel_metres);
            let normal_vector = Vec3::new(-dh_dx, -dh_dy, 1.0).normalize();
            let roughness = slope_adjusted_roughness(sample.roughness, dh_dx.hypot(dh_dy));

            albedo.extend_from_slice(&[
                encode_unit(sample.albedo.x),
                encode_unit(sample.albedo.y),
                encode_unit(sample.albedo.z),
                255,
            ]);
            normal.extend_from_slice(&[
                encode_unit(normal_vector.x * 0.5 + 0.5),
                encode_unit(normal_vector.y * 0.5 + 0.5),
                encode_unit(normal_vector.z * 0.5 + 0.5),
                255,
            ]);
            let encoded_height = encode_unit(sample.height * 0.5 + 0.5);
            height.extend_from_slice(&[encoded_height, encoded_height, encoded_height, 255]);
            arm.extend_from_slice(&[encode_unit(sample.ao), encode_unit(roughness), 0, 255]);
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

    fn mip_rgba(image: &Image, level: u32) -> (u32, &[u8]) {
        let size = LIME_PLASTER_TEXTURE_SIZE >> level;
        let offset = (0..level)
            .map(|prior| (LIME_PLASTER_TEXTURE_SIZE >> prior).pow(2) as usize * 4)
            .sum::<usize>();
        let byte_count = size.pow(2) as usize * 4;
        let data = image.data.as_deref().expect("packed lime-plaster mips");
        (size, &data[offset..offset + byte_count])
    }

    fn normal_rms_degrees(bytes: &[u8]) -> f32 {
        let mean_square = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                let normal = Vec3::new(
                    f32::from(pixel[0]) / 255.0 * 2.0 - 1.0,
                    f32::from(pixel[1]) / 255.0 * 2.0 - 1.0,
                    f32::from(pixel[2]) / 255.0 * 2.0 - 1.0,
                )
                .normalize_or_zero();
                normal.z.clamp(-1.0, 1.0).acos().to_degrees().powi(2)
            })
            .sum::<f32>()
            / bytes.as_chunks::<4>().0.len() as f32;
        mean_square.sqrt()
    }

    fn normal_angle_degrees(pixel: &[u8; 4]) -> f32 {
        let normal = Vec3::new(
            f32::from(pixel[0]) / 255.0 * 2.0 - 1.0,
            f32::from(pixel[1]) / 255.0 * 2.0 - 1.0,
            f32::from(pixel[2]) / 255.0 * 2.0 - 1.0,
        )
        .normalize_or_zero();
        normal.z.clamp(-1.0, 1.0).acos().to_degrees()
    }

    #[test]
    fn sampling_is_deterministic_and_periodic() {
        for (u, v) in [(0.0, 0.13), (0.07, 0.61), (0.48, 0.94), (0.91, 0.22)] {
            let sample = lime_plaster_sample(u, v);
            let repeated = lime_plaster_sample(u + 1.0, v - 1.0);
            assert_eq!(
                sample.height.to_bits(),
                lime_plaster_sample(u, v).height.to_bits()
            );
            assert!((sample.height - repeated.height).abs() < 1.0e-4);
            assert!((sample.ao - repeated.ao).abs() < 1.0e-4);
            assert!((sample.roughness - repeated.roughness).abs() < 1.0e-4);
            assert!(sample.albedo.distance(repeated.albedo) < 1.0e-4);
        }
    }

    #[test]
    fn roughness_is_oblique_periodic_and_increases_with_relief_slope() {
        for (u, v) in [(0.12, 0.37), (0.58, 0.81), (0.93, 0.04)] {
            let variation = oblique_micro_variation(u, v);
            assert!((variation - oblique_micro_variation(u + 1.0, v - 1.0)).abs() < 1.0e-4);
        }
        let base = 0.81;
        assert!(slope_adjusted_roughness(base, 0.18) > slope_adjusted_roughness(base, 0.0));
        assert!(slope_adjusted_roughness(base, 0.70) <= 0.94);
    }

    #[test]
    fn physical_scale_preserves_worked_relief_and_sparse_cavities() {
        assert_eq!(LIME_PLASTER_TILE_METRES, 1.0);
        assert!((0.003..=0.005).contains(&LIME_PLASTER_HEIGHT_RANGE_METRES));
        assert!(LIME_PLASTER_TILE_METRES / LIME_PLASTER_TEXTURE_SIZE as f32 <= 0.001);
        let mut cavities = 0_usize;
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        let mut physical_heights = Vec::with_capacity(256 * 256);
        for y in 0..256 {
            for x in 0..256 {
                let sample =
                    lime_plaster_sample((x as f32 + 0.5) / 256.0, (y as f32 + 0.5) / 256.0);
                cavities += usize::from(sample.cavity > 0.5);
                minimum = minimum.min(sample.height);
                maximum = maximum.max(sample.height);
                physical_heights.push(sample.height.abs() * LIME_PLASTER_HEIGHT_RANGE_METRES * 0.5);
            }
        }
        physical_heights.sort_by(f32::total_cmp);
        let [median, p90, p99, maximum_physical] = [0.50, 0.90, 0.99, 1.0].map(|quantile| {
            let index = ((physical_heights.len() - 1) as f32 * quantile) as usize;
            physical_heights[index]
        });
        println!(
            "lime-plaster absolute height metres at median/P90/P99/max: {median:.7}/{p90:.7}/{p99:.7}/{maximum_physical:.7}"
        );
        assert!(median <= 0.000_4, "median relief: {median} m");
        assert!(p90 <= 0.000_8, "P90 relief: {p90} m");
        assert!(p99 <= 0.001_1, "P99 relief: {p99} m");
        assert!(
            maximum_physical <= PLASTER_HEIGHT_HALF_RANGE_METRES,
            "maximum relief: {maximum_physical} m"
        );
        assert!((8..=220).contains(&cavities), "cavity texels: {cavities}");
        assert!(minimum < -0.25, "minimum height: {minimum}");
        assert!(maximum > 0.18, "maximum height: {maximum}");
    }

    #[test]
    fn albedo_is_a_bounded_palette_without_relief_shading_or_black_pores() {
        let mut palette = BTreeSet::new();
        let mut heights_by_color = std::collections::BTreeMap::<[u32; 3], (f32, f32)>::new();
        for y in 0..128 {
            for x in 0..128 {
                let sample =
                    lime_plaster_sample((x as f32 + 0.5) / 128.0, (y as f32 + 0.5) / 128.0);
                assert!(
                    sample.albedo.min_element() > 0.60,
                    "black plaster pore: {sample:?}"
                );
                let key = sample.albedo.to_array().map(f32::to_bits);
                palette.insert(key);
                let range = heights_by_color
                    .entry(key)
                    .or_insert((f32::INFINITY, f32::NEG_INFINITY));
                range.0 = range.0.min(sample.height);
                range.1 = range.1.max(sample.height);
            }
        }
        assert_eq!(palette.len(), 3);
        assert!(
            palette.iter().all(|color| {
                Vec3::from_array(color.map(f32::from_bits)).distance(PLASTER_BASE) < 0.012
            }),
            "plaster palette is too contrasty: {palette:?}"
        );
        assert!(
            heights_by_color
                .values()
                .all(|(minimum, maximum)| maximum - minimum > 0.30),
            "albedo categories must not encode relief height: {heights_by_color:?}"
        );
    }

    #[test]
    fn albedo_accents_are_sparse_millimetre_flecks_not_connected_cells() {
        let side = PLASTER_ALBEDO_CELLS_PER_METRE as usize;
        let classes = (0..side * side)
            .map(|index| {
                let x = index % side;
                let y = index / side;
                let albedo = plaster_albedo(
                    (x as f32 + 0.5) / side as f32,
                    (y as f32 + 0.5) / side as f32,
                );
                if albedo == PLASTER_BASE {
                    0_i8
                } else if albedo == PLASTER_COOL_FLECK {
                    -1
                } else {
                    1
                }
            })
            .collect::<Vec<_>>();
        let accent_fraction =
            classes.iter().filter(|class| **class != 0).count() as f32 / classes.len() as f32;
        assert!(
            (0.025..=0.035).contains(&accent_fraction),
            "accent fraction: {accent_fraction}"
        );

        let mut visited = vec![false; classes.len()];
        let mut largest_component = 0;
        for start in 0..classes.len() {
            if classes[start] == 0 || visited[start] {
                continue;
            }
            let class = classes[start];
            let mut pending = std::collections::VecDeque::from([start]);
            visited[start] = true;
            let mut component = 0;
            while let Some(index) = pending.pop_front() {
                component += 1;
                let x = index % side;
                let y = index / side;
                for neighbour in [
                    ((x + side - 1) % side, y),
                    ((x + 1) % side, y),
                    (x, (y + side - 1) % side),
                    (x, (y + 1) % side),
                ] {
                    let neighbour = neighbour.1 * side + neighbour.0;
                    if !visited[neighbour] && classes[neighbour] == class {
                        visited[neighbour] = true;
                        pending.push_back(neighbour);
                    }
                }
            }
            largest_component = largest_component.max(component);
        }
        let fleck_metres = LIME_PLASTER_TILE_METRES / side as f32;
        assert!((0.003..=0.005).contains(&fleck_metres));
        assert!(
            largest_component <= 4,
            "accent connected component spans {largest_component} flecks"
        );
    }

    #[test]
    fn albedo_has_no_low_frequency_coloured_region() {
        let block_cells = 16;
        let blocks = PLASTER_ALBEDO_CELLS_PER_METRE as usize / block_cells;
        let mut maximum_block_deviation = 0.0_f32;
        for block_y in 0..blocks {
            for block_x in 0..blocks {
                let mut mean = Vec3::ZERO;
                for y in 0..block_cells {
                    for x in 0..block_cells {
                        mean += plaster_albedo(
                            ((block_x * block_cells + x) as f32 + 0.5)
                                / PLASTER_ALBEDO_CELLS_PER_METRE as f32,
                            ((block_y * block_cells + y) as f32 + 0.5)
                                / PLASTER_ALBEDO_CELLS_PER_METRE as f32,
                        );
                    }
                }
                mean /= (block_cells * block_cells) as f32;
                maximum_block_deviation = maximum_block_deviation.max(mean.distance(PLASTER_BASE));
            }
        }
        assert!(
            maximum_block_deviation < 0.001,
            "six-centimetre block deviates by {maximum_block_deviation}"
        );
    }

    #[test]
    fn relief_has_no_dominant_metre_scale_cloud() {
        let block_side = 128;
        let blocks = 2;
        let mut block_means = Vec::new();
        for block_y in 0..blocks {
            for block_x in 0..blocks {
                let mut total = 0.0;
                for y in 0..block_side {
                    for x in 0..block_side {
                        let u = (block_x * block_side + x) as f32 / (blocks * block_side) as f32;
                        let v = (block_y * block_side + y) as f32 / (blocks * block_side) as f32;
                        total += lime_plaster_sample(u, v).height;
                    }
                }
                block_means.push(total / (block_side * block_side) as f32);
            }
        }
        let spread = block_means
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
            - block_means.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(spread < 0.05, "low-frequency block-mean spread: {spread}");
    }

    #[test]
    fn generated_channels_are_coherent_and_have_complete_mips() {
        let mut images = Assets::<Image>::default();
        let textures = generate_lime_plaster_textures(&mut images);
        assert_eq!(images.len(), 4);
        let expected_mips = LIME_PLASTER_TEXTURE_SIZE.ilog2() + 1;
        let mip_texels = (0..expected_mips)
            .map(|level| (LIME_PLASTER_TEXTURE_SIZE >> level).pow(2))
            .sum::<u32>() as usize;
        for handle in [
            &textures.albedo,
            &textures.normal_gl,
            &textures.height,
            &textures.arm,
        ] {
            let image = images.get(handle).unwrap();
            assert_eq!((image.width(), image.height()), (1024, 1024));
            assert_eq!(image.texture_descriptor.mip_level_count, expected_mips);
            assert_eq!(image.data.as_ref().unwrap().len(), mip_texels * 4);
        }
        assert_eq!(
            images
                .get(&textures.albedo)
                .unwrap()
                .texture_descriptor
                .format,
            TextureFormat::Rgba8UnormSrgb
        );
        let arm = images.get(&textures.arm).unwrap().data.as_ref().unwrap();
        assert!(
            arm.as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[2] == 0 && pixel[3] == 255)
        );
        let albedo = images.get(&textures.albedo).unwrap().data.as_ref().unwrap();
        let palette = albedo[..LIME_PLASTER_TEXTURE_SIZE.pow(2) as usize * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect::<BTreeSet<_>>();
        assert_eq!(palette.len(), 3, "albedo colors: {}", palette.len());
        assert!(
            palette
                .iter()
                .all(|pixel| pixel.iter().all(|channel| *channel > 150)),
            "albedo contains a black pinhole: {palette:?}"
        );

        let normal = images
            .get(&textures.normal_gl)
            .expect("generated lime-plaster normal");
        let normal_rms = [0, 2, 4].map(|level| normal_rms_degrees(mip_rgba(normal, level).1));
        println!("lime-plaster normal RMS degrees at mips 0/2/4: {normal_rms:?}");
        assert!(
            normal_rms[0] >= 4.8,
            "base normal RMS is too flat: {normal_rms:?}"
        );
        assert!(
            normal_rms[1] >= 4.0,
            "four-texel mip loses close-view relief: {normal_rms:?}"
        );
        assert!(
            (2.0..=2.8).contains(&normal_rms[2]),
            "sixteen-texel mip must retain shallow relief then converge: {normal_rms:?}"
        );

        let (base_size, base_normal) = mip_rgba(normal, 0);
        let pixels = base_normal.as_chunks::<4>().0;
        for quadrant_y in 0..2 {
            for quadrant_x in 0..2 {
                let mut active = 0_usize;
                let mut total = 0_usize;
                for y in
                    quadrant_y * base_size as usize / 2..(quadrant_y + 1) * base_size as usize / 2
                {
                    for x in quadrant_x * base_size as usize / 2
                        ..(quadrant_x + 1) * base_size as usize / 2
                    {
                        total += 1;
                        active += usize::from(
                            normal_angle_degrees(&pixels[y * base_size as usize + x]) >= 1.5,
                        );
                    }
                }
                let active_fraction = active as f32 / total as f32;
                assert!(
                    active_fraction >= 0.25,
                    "half-metre quadrant ({quadrant_x}, {quadrant_y}) has only {active_fraction:.3} molded response"
                );
            }
        }
    }
}
