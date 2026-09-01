use std::collections::BTreeSet;

use super::*;

fn rgba_palette(image: &Image) -> BTreeSet<[u8; 4]> {
    image
        .data
        .as_deref()
        .expect("generated image data")
        .as_chunks::<4>()
        .0
        .iter()
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
    let textures = generate_rock_textures(&mut images);
    let albedo = images.get(&textures.albedo).unwrap();
    let base_length = (ROCK_TEXTURE_SIZE * ROCK_TEXTURE_SIZE * 4) as usize;
    let albedo_palette = albedo.data.as_ref().unwrap()[..base_length]
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert!(albedo_palette.len() <= 4);
    let arm = images.get(&textures.arm).unwrap().data.as_ref().unwrap()[..base_length]
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert!(
        arm.iter()
            .map(|pixel| pixel[1])
            .collect::<BTreeSet<_>>()
            .len()
            <= 4
    );
    let normal = images.get(&textures.normal_gl).unwrap();
    let height = images.get(&textures.height).unwrap();
    assert!(
        normal.data.as_ref().unwrap()[..base_length]
            .as_chunks::<4>()
            .0
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            > 64
    );
    assert!(
        height.data.as_ref().unwrap()[..base_length]
            .as_chunks::<4>()
            .0
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            > 32
    );
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
fn forest_ground_uses_packed_surface_and_normal_textures_with_complete_mip_chains() {
    let mut images = Assets::<Image>::default();
    let textures = generate_forest_soil_texture(&mut images);
    assert_eq!(images.len(), 3);
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

    let litter = images.get(&textures.litter_surface).unwrap();
    assert_eq!((litter.width(), litter.height()), (1024, 1024));
    assert_eq!(litter.texture_descriptor.format, TextureFormat::Rgba8Unorm);
    assert_eq!(litter.texture_descriptor.mip_level_count, 11);
    assert_eq!(
        litter.data.as_ref().unwrap().len(),
        (mip_texels * 4) as usize
    );
    let litter_normal = images.get(&textures.litter_normal).unwrap();
    assert_eq!(
        (litter_normal.width(), litter_normal.height()),
        (1024, 1024)
    );
    assert_eq!(
        litter_normal.texture_descriptor.format,
        TextureFormat::Rg8Unorm
    );
    assert_eq!(litter_normal.texture_descriptor.mip_level_count, 11);
    assert_eq!(
        litter_normal.data.as_ref().unwrap().len(),
        (mip_texels * 2) as usize
    );
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
        .flat_map(|y| (0..128).map(move |x| forest_soil_height(x as f32 / 128.0, y as f32 / 128.0)))
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
fn forest_litter_is_periodic_dense_and_retains_soil_gaps() {
    let mut covered = 0_usize;
    let mut exposed = 0_usize;
    let mut minimum_ao = 1.0_f32;
    let mut maximum_repeat_error = 0.0_f32;
    for y in 0..128 {
        for x in 0..128 {
            let u = (x as f32 + 0.5) / 128.0;
            let v = (y as f32 + 0.5) / 128.0;
            let sample = forest_litter_sample(u, v);
            let repeated = forest_litter_sample(u + 1.0, v - 1.0);
            maximum_repeat_error = maximum_repeat_error
                .max((sample.coverage - repeated.coverage).abs())
                .max((sample.height - repeated.height).abs());
            assert!((0.47..=0.94).contains(&sample.height));
            minimum_ao = minimum_ao.min(sample.ao);
            covered += usize::from(sample.coverage >= 0.5);
            exposed += usize::from(sample.coverage <= 0.1);
        }
    }
    let samples = 128 * 128;
    assert!(
        maximum_repeat_error < 0.01,
        "maximum periodic repeat error: {maximum_repeat_error}"
    );
    assert!(covered * 100 / samples >= 68, "covered texels: {covered}");
    assert!(exposed * 100 / samples >= 3, "exposed texels: {exposed}");
    assert!(minimum_ao <= 0.82, "minimum litter AO: {minimum_ao}");
    assert_eq!(FOREST_LITTER_TILE_METRES, 4.0);
    assert!((0.014..=0.020).contains(&FOREST_LITTER_HEIGHT_RANGE_METRES));
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
            (modulation - bark_segment_modulation(point + Vec2::ONE, first, second)).abs() < 1.0e-5
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
