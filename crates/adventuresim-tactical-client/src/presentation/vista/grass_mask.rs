use super::*;

const VISTA_GRASS_MASK_TEXEL_METRES: f32 = 1.0;

#[allow(clippy::too_many_arguments)]
pub(super) fn stitched_vista_grass_coverage(
    lod: &VistaLod,
    playable_half_extent: Vec2,
    playable_ground: &SceneGround,
    playable_mask: &[u8],
    playable_width: u32,
    playable_depth: u32,
    point: Vec2,
    urban_ground: UrbanGround<'_>,
) -> f32 {
    if urban_ground.suppresses_grass(point) {
        return 0.0;
    }
    let boundary = point.clamp(-playable_half_extent, playable_half_extent);
    let playable_coverage = sample_playable_grass_mask(
        playable_mask,
        playable_width,
        playable_depth,
        playable_ground,
        boundary,
    );
    let outside = (point.abs() - playable_half_extent)
        .max(Vec2::ZERO)
        .max_element();
    if outside <= 0.0 {
        return playable_coverage;
    }
    let vista_coverage = sample_vista_environment(lod, point)
        .map(vista_sward_coverage)
        .unwrap_or(0.0);
    let blend = smoothstep01(outside / VISTA_GRASS_BOUNDARY_STITCH_METRES);
    playable_coverage.lerp(vista_coverage, blend)
}

pub(super) fn vista_grass_cover_mask_image(
    lod: &VistaLod,
    playable_half_extent: Vec2,
    playable_ground: &SceneGround,
    seed: u64,
    outer_collar: f32,
    urban_ground: UrbanGround<'_>,
) -> (Image, Vec4) {
    let (playable_width, playable_depth, playable_mask) =
        grass_cover_mask_pixels(playable_ground, seed);
    let requested_outer = playable_half_extent + Vec2::splat(outer_collar);
    let width =
        ((requested_outer.x * 2.0 / VISTA_GRASS_MASK_TEXEL_METRES).ceil() as u32 + 1).max(2);
    let depth =
        ((requested_outer.y * 2.0 / VISTA_GRASS_MASK_TEXEL_METRES).ceil() as u32 + 1).max(2);
    let span = Vec2::new(
        (width - 1) as f32 * VISTA_GRASS_MASK_TEXEL_METRES,
        (depth - 1) as f32 * VISTA_GRASS_MASK_TEXEL_METRES,
    );
    let outer = span * 0.5;
    let mut pixels = Vec::with_capacity(width as usize * depth as usize);
    for z in 0..depth {
        for x in 0..width {
            let point = Vec2::new(
                x as f32 * VISTA_GRASS_MASK_TEXEL_METRES - outer.x,
                z as f32 * VISTA_GRASS_MASK_TEXEL_METRES - outer.y,
            );
            let coverage = stitched_vista_grass_coverage(
                lod,
                playable_half_extent,
                playable_ground,
                &playable_mask,
                playable_width,
                playable_depth,
                point,
                urban_ground,
            );
            pixels.push((coverage.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width,
            height: depth,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    (image, Vec4::new(1.0 / span.x, 1.0 / span.y, 0.5, 0.5))
}

pub(super) fn sample_playable_grass_mask(
    mask: &[u8],
    width: u32,
    depth: u32,
    ground: &SceneGround,
    point: Vec2,
) -> f32 {
    let maximum_coordinate = Vec2::new((width - 1) as f32, (depth - 1) as f32);
    let coordinate = (Vec2::new(
        point.x / ground.width().max(f32::EPSILON) + 0.5,
        point.y / ground.depth().max(f32::EPSILON) + 0.5,
    ) * maximum_coordinate)
        .clamp(Vec2::ZERO, maximum_coordinate);
    let minimum = coordinate.floor().max(Vec2::ZERO);
    let maximum = (minimum + Vec2::ONE).min(maximum_coordinate);
    let fraction = coordinate - minimum;
    let sample = |x: f32, z: f32| f32::from(mask[z as usize * width as usize + x as usize]) / 255.0;
    let bottom = sample(minimum.x, minimum.y).lerp(sample(maximum.x, minimum.y), fraction.x);
    let top = sample(minimum.x, maximum.y).lerp(sample(maximum.x, maximum.y), fraction.x);
    bottom.lerp(top, fraction.y)
}
