use image::{ExtendedColorType, ImageEncoder, codecs::avif::AvifEncoder};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use tiny_skia::{
    Color, FillRule, IntRect, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke,
    Transform,
};

use super::{Package, Point, TileEntry, TilePyramid};

const WIDTH: f64 = 1_200.0;
const HEIGHT: f64 = 800.0;
const TILE_GUTTER: u8 = 4;
const RENDER_MARGIN: u32 = 12;

#[derive(Clone, Copy, Debug)]
pub(super) struct TileConfig {
    pub tile_size: u32,
    pub max_zoom: u8,
}

impl Default for TileConfig {
    fn default() -> Self {
        Self {
            tile_size: 512,
            max_zoom: 6,
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    land: [u8; 4],
    water: [u8; 4],
    water_edge: [u8; 4],
    road: [u8; 4],
    ferry: [u8; 4],
    contour: [u8; 4],
    forest: [u8; 4],
    conifer: [u8; 4],
    terrain_ink: [u8; 4],
    terrain_hatch: [u8; 4],
    elevation: [[u8; 4]; 7],
}

const PAPER: Palette = Palette {
    land: [230, 225, 203, 255],
    water: [184, 201, 197, 255],
    water_edge: [112, 124, 119, 255],
    road: [110, 101, 79, 255],
    ferry: [98, 95, 88, 255],
    contour: [102, 91, 73, 135],
    forest: [115, 128, 106, 120],
    conifer: [88, 106, 93, 145],
    terrain_ink: [94, 81, 66, 205],
    terrain_hatch: [112, 96, 77, 155],
    elevation: [
        [221, 213, 186, 255],
        [212, 201, 170, 255],
        [201, 185, 153, 255],
        [186, 166, 134, 255],
        [170, 146, 119, 255],
        [152, 127, 108, 255],
        [130, 107, 96, 255],
    ],
};

pub(super) fn build(
    package: &Package,
    config: TileConfig,
) -> Result<(TilePyramid, Vec<u8>), Box<dyn std::error::Error>> {
    if config.tile_size < 64 || !config.tile_size.is_power_of_two() || config.max_zoom > 8 {
        return Err("strategic map tile configuration is outside its bound".into());
    }
    let mut bytes = Vec::new();
    let mut entries = Vec::new();
    for zoom in 0..=config.max_zoom {
        let scale = f64::from(1_u32 << zoom);
        let span = f64::from(config.tile_size) / scale;
        let columns = (WIDTH / span).ceil() as u16;
        let rows = (HEIGHT / span).ceil() as u16;
        let coordinates: Vec<_> = (0..rows)
            .flat_map(|y| (0..columns).map(move |x| (x, y)))
            .collect();
        let quality = if zoom == config.max_zoom { 95 } else { 82 };
        let encoded_tiles: Result<Vec<Vec<u8>>, String> = coordinates
            .par_iter()
            .map(|&(x, y)| {
                let tile = render(package, config.tile_size, TILE_GUTTER, scale, x, y, PAPER)
                    .map_err(|error| error.to_string())?;
                encode(&tile, quality).map_err(|error| error.to_string())
            })
            .collect();
        for ((x, y), encoded) in coordinates.into_iter().zip(encoded_tiles?) {
            let offset = u64::try_from(bytes.len())?;
            let length = u32::try_from(encoded.len())?;
            bytes.extend_from_slice(&encoded);
            entries.push(TileEntry {
                theme: "paper",
                zoom,
                x,
                y,
                offset,
                length,
            });
        }
    }
    let content_sha256 = format!("{:x}", Sha256::digest(&bytes));
    Ok((
        TilePyramid {
            format: "avif",
            tile_size: config.tile_size,
            gutter: TILE_GUTTER,
            max_zoom: config.max_zoom,
            content_sha256,
            entries,
        },
        bytes,
    ))
}

fn encode(pixmap: &Pixmap, quality: u8) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Vec::new();
    AvifEncoder::new_with_speed_quality(&mut bytes, 10, quality).write_image(
        pixmap.data(),
        pixmap.width(),
        pixmap.height(),
        ExtendedColorType::Rgba8,
    )?;
    Ok(bytes)
}

fn render(
    package: &Package,
    tile_size: u32,
    gutter: u8,
    scale: f64,
    tile_x: u16,
    tile_y: u16,
    palette: Palette,
) -> Result<Pixmap, Box<dyn std::error::Error>> {
    let output_size = tile_size + 2 * u32::from(gutter);
    let render_size = output_size + 2 * RENDER_MARGIN;
    let mut pixmap = Pixmap::new(render_size, render_size).ok_or("invalid tile dimensions")?;
    pixmap.fill(color(palette.land));
    let origin = (
        f64::from(tile_x) * f64::from(tile_size) - f64::from(gutter) - f64::from(RENDER_MARGIN),
        f64::from(tile_y) * f64::from(tile_size) - f64::from(gutter) - f64::from(RENDER_MARGIN),
    );
    let logical_bounds = [
        origin.0 / scale,
        origin.1 / scale,
        (origin.0 + f64::from(render_size)) / scale,
        (origin.1 + f64::from(render_size)) / scale,
    ];

    if scale < 16.0 {
        for cell in &package.elevation.cells {
            let index = [50, 100, 250, 500, 1_000, 1_500, 2_000]
                .iter()
                .position(|value| *value == cell.band_m)
                .unwrap_or_default();
            fill_source_bounds(
                &mut pixmap,
                cell.bounds,
                package.bounds,
                scale,
                origin,
                logical_bounds,
                palette.elevation[index],
            );
        }
    } else {
        draw_elevation_stamps(
            &mut pixmap,
            &package.elevation.cells,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            palette,
        );
    }
    for polygon in &package.water {
        stroke_and_fill_source_polygon(
            &mut pixmap,
            polygon,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            Some(palette.water),
            Some((palette.water_edge, 1.1)),
        );
    }
    for region in &package.forest.regions {
        let mut shade = match region.kind.as_str() {
            "conifer" => palette.conifer,
            "mixed" => mix(palette.forest, palette.conifer),
            _ => palette.forest,
        };
        shade[3] = match region.density {
            1 => (u16::from(shade[3]) * 2 / 5) as u8,
            2 => (u16::from(shade[3]) * 2 / 3) as u8,
            _ => shade[3],
        };
        if scale < 16.0 {
            fill_source_bounds(
                &mut pixmap,
                region.bounds,
                package.bounds,
                scale,
                origin,
                logical_bounds,
                shade,
            );
        } else {
            draw_forest_grove(
                &mut pixmap,
                region.bounds,
                region.density,
                &region.kind,
                package.bounds,
                scale,
                origin,
                logical_bounds,
                shade,
                palette.terrain_ink,
            );
        }
    }
    for (index, contour) in package.elevation.contours.iter().enumerate() {
        let show_contour =
            scale < 16.0 || (scale < 32.0 && contour.elevation_m >= 500 && index % 2 == 0);
        if !show_contour {
            continue;
        }
        stroke_raw_path(
            &mut pixmap,
            &contour.points,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            palette.contour,
            if contour.elevation_m >= 500 { 1.3 } else { 1.1 },
        );
    }
    for road in &package.roads {
        stroke_source_path(
            &mut pixmap,
            &road.points,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            if road.kind == "ferry" {
                palette.ferry
            } else {
                palette.road
            },
            1.35,
        );
    }
    let output_rect = IntRect::from_xywh(
        RENDER_MARGIN as i32,
        RENDER_MARGIN as i32,
        output_size,
        output_size,
    )
    .ok_or("invalid tile crop")?;
    pixmap
        .clone_rect(output_rect)
        .ok_or("invalid tile crop".into())
}

fn fill_source_bounds(
    pixmap: &mut Pixmap,
    [west, south, east, north]: [f64; 4],
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    shade: [u8; 4],
) {
    let (left, top) = project(west, north, map_bounds);
    let (right, bottom) = project(east, south, map_bounds);
    if !intersects([left, top, right, bottom], tile_bounds) {
        return;
    }
    let Some(rect) = Rect::from_xywh(
        (left * scale - origin.0) as f32,
        (top * scale - origin.1) as f32,
        ((right - left) * scale) as f32,
        ((bottom - top) * scale) as f32,
    ) else {
        return;
    };
    pixmap.fill_rect(rect, &paint(shade), Transform::identity(), None);
}

fn draw_forest_grove(
    pixmap: &mut Pixmap,
    [west, south, east, north]: [f64; 4],
    density: u8,
    kind: &str,
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    mut shade: [u8; 4],
    ink: [u8; 4],
) {
    let (left, top) = project(west, north, map_bounds);
    let (right, bottom) = project(east, south, map_bounds);
    let symbol_margin = 8.0 / scale;
    if !intersects(
        [
            left - symbol_margin,
            top - symbol_margin,
            right + symbol_margin,
            bottom + symbol_margin,
        ],
        tile_bounds,
    ) {
        return;
    }
    shade[3] = match density {
        1 => 105,
        2 => 145,
        _ => 180,
    };
    let zoom_multiplier = if scale >= 64.0 {
        3
    } else if scale >= 32.0 {
        2
    } else {
        1
    };
    let count = usize::from(density.clamp(1, 3)) * zoom_multiplier + 1;
    let mut random = feature_seed([west, south, east, north], density, kind);
    for index in 0..count {
        let relative_x = 0.10 + 0.80 * next_unit(&mut random);
        let relative_y = 0.12 + 0.76 * next_unit(&mut random);
        let size_jitter = 0.84 + 0.28 * next_unit(&mut random);
        let x = ((left + (right - left) * relative_x) * scale - origin.0) as f32;
        let y = ((top + (bottom - top) * relative_y) * scale - origin.1) as f32;
        let height = (if scale >= 64.0 {
            8.0
        } else if scale >= 32.0 {
            6.5
        } else {
            5.0
        } * size_jitter) as f32;
        let conifer = match kind {
            "conifer" => true,
            "mixed" => (next_random(&mut random) ^ index as u64) & 1 == 0,
            _ => false,
        };
        if conifer {
            draw_conifer(pixmap, x, y, height, shade, ink);
        } else {
            draw_deciduous_tree(pixmap, x, y, height, shade, ink);
        }
    }
}

fn draw_deciduous_tree(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    height: f32,
    shade: [u8; 4],
    ink: [u8; 4],
) {
    let half = height * 0.42;
    let crown_y = y - height * 0.20;
    let mut crown = PathBuilder::new();
    crown.move_to(x - half, crown_y + height * 0.08);
    crown.cubic_to(
        x - half * 1.08,
        crown_y - height * 0.18,
        x - half * 0.55,
        crown_y - height * 0.45,
        x - half * 0.18,
        crown_y - height * 0.34,
    );
    crown.cubic_to(
        x + half * 0.05,
        crown_y - height * 0.58,
        x + half * 0.72,
        crown_y - height * 0.42,
        x + half * 0.65,
        crown_y - height * 0.16,
    );
    crown.cubic_to(
        x + half * 1.08,
        crown_y - height * 0.02,
        x + half * 0.68,
        crown_y + height * 0.29,
        x + half * 0.22,
        crown_y + height * 0.22,
    );
    crown.cubic_to(
        x - half * 0.15,
        crown_y + height * 0.38,
        x - half * 0.72,
        crown_y + height * 0.30,
        x - half,
        crown_y + height * 0.08,
    );
    crown.close();
    if let Some(crown) = crown.finish() {
        pixmap.fill_path(
            &crown,
            &symbol_paint(shade),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        stroke_pixmap_path(pixmap, &crown, ink, 0.75);
    }
    let mut trunk = PathBuilder::new();
    trunk.move_to(x, crown_y + height * 0.12);
    trunk.line_to(x - height * 0.03, y + height * 0.46);
    trunk.move_to(x, crown_y + height * 0.12);
    trunk.line_to(x + height * 0.12, crown_y - height * 0.02);
    if let Some(trunk) = trunk.finish() {
        stroke_pixmap_path(pixmap, &trunk, ink, 0.8);
    }
}

fn draw_conifer(pixmap: &mut Pixmap, x: f32, y: f32, height: f32, shade: [u8; 4], ink: [u8; 4]) {
    let width = height * 0.60;
    let mut crown = PathBuilder::new();
    crown.move_to(x, y - height * 0.72);
    crown.line_to(x - width * 0.30, y - height * 0.34);
    crown.line_to(x - width * 0.13, y - height * 0.36);
    crown.line_to(x - width * 0.49, y - height * 0.02);
    crown.line_to(x - width * 0.22, y - height * 0.08);
    crown.line_to(x - width * 0.60, y + height * 0.24);
    crown.line_to(x + width * 0.60, y + height * 0.24);
    crown.line_to(x + width * 0.22, y - height * 0.08);
    crown.line_to(x + width * 0.49, y - height * 0.02);
    crown.line_to(x + width * 0.13, y - height * 0.36);
    crown.line_to(x + width * 0.30, y - height * 0.34);
    crown.close();
    if let Some(crown) = crown.finish() {
        pixmap.fill_path(
            &crown,
            &symbol_paint(shade),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        stroke_pixmap_path(pixmap, &crown, ink, 0.8);
    }
    let mut trunk = PathBuilder::new();
    trunk.move_to(x, y + height * 0.10);
    trunk.line_to(x, y + height * 0.48);
    if let Some(trunk) = trunk.finish() {
        stroke_pixmap_path(pixmap, &trunk, ink, 0.9);
    }
}

fn draw_elevation_stamps(
    pixmap: &mut Pixmap,
    cells: &[super::raster::ElevationCell],
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    palette: Palette,
) {
    let minimum_band = if scale >= 64.0 {
        100
    } else if scale >= 32.0 {
        250
    } else {
        500
    };
    for cell in cells.iter().filter(|cell| cell.band_m >= minimum_band) {
        let [west, south, east, north] = cell.bounds;
        let (left, top) = project(west, north, map_bounds);
        let (right, bottom) = project(east, south, map_bounds);
        let stamp_margin = 12.0 / scale;
        if !intersects(
            [
                left - stamp_margin,
                top - stamp_margin,
                right + stamp_margin,
                bottom + stamp_margin,
            ],
            tile_bounds,
        ) {
            continue;
        }
        let mut random = feature_seed(cell.bounds, cell.band_m.clamp(0, 255) as u8, "hill");
        let width = if scale >= 64.0 {
            18.0
        } else if scale >= 32.0 {
            14.0
        } else {
            10.0
        };
        let band_index = [50, 100, 250, 500, 1_000, 1_500, 2_000]
            .iter()
            .position(|value| *value == cell.band_m)
            .unwrap_or_default();
        let high_ridge = band_index >= 4;
        let stamp_count = if scale >= 64.0 {
            8 + usize::from(high_ridge) * 2
        } else if scale >= 32.0 {
            4 + usize::from(high_ridge)
        } else {
            2 + usize::from(high_ridge)
        };
        for index in 0..stamp_count {
            let progress = (index as f64 + 0.5) / stamp_count as f64;
            let jitter_x = (next_unit(&mut random) - 0.5) * 0.14;
            let jitter_y = (next_unit(&mut random) - 0.5) * 0.28;
            let center_x = left + (right - left) * (0.10 + 0.80 * progress + jitter_x);
            let center_y = top
                + (bottom - top) * (0.50 + jitter_y + if index % 2 == 0 { -0.07 } else { 0.07 });
            let width_jitter = 0.88 + 0.24 * next_unit(&mut random);
            draw_hill_stamp(
                pixmap,
                (center_x * scale - origin.0) as f32,
                (center_y * scale - origin.1) as f32,
                (width * width_jitter) as f32,
                band_index,
                scale,
                palette.elevation[band_index],
                palette.terrain_ink,
                palette.terrain_hatch,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hill_stamp(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    band_index: usize,
    scale: f64,
    fill: [u8; 4],
    ink: [u8; 4],
    hatch: [u8; 4],
) {
    let height = width * (0.48 + band_index as f32 * 0.035);
    let left = x - width * 0.5;
    let right = x + width * 0.5;
    let ridge = band_index >= 4;
    let mut profile = PathBuilder::new();
    profile.move_to(left, y + height * 0.30);
    if ridge {
        profile.line_to(x - width * 0.18, y - height * 0.30);
        profile.line_to(x - width * 0.04, y - height * 0.10);
        profile.line_to(x + width * 0.14, y - height * 0.52);
    } else {
        profile.line_to(x - width * 0.08, y - height * 0.48);
    }
    profile.line_to(right, y + height * 0.30);
    profile.close();
    if let Some(profile) = profile.finish() {
        pixmap.fill_path(
            &profile,
            &symbol_paint(with_alpha(fill, 170)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        stroke_pixmap_path(pixmap, &profile, ink, if scale >= 32.0 { 1.0 } else { 0.8 });
    }

    let hatch_count = if scale >= 64.0 {
        4 + usize::from(band_index >= 4)
    } else if scale >= 32.0 {
        3
    } else {
        1
    };
    for index in 0..hatch_count {
        let offset = (index as f32 + 1.0) / (hatch_count as f32 + 1.0);
        let start_x = x + width * (0.05 + offset * 0.36);
        let start_y = y - height * (0.28 - offset * 0.16);
        let mut line = PathBuilder::new();
        line.move_to(start_x, start_y);
        line.line_to(start_x - width * 0.12, y + height * (0.05 + offset * 0.20));
        if let Some(line) = line.finish() {
            stroke_pixmap_path(pixmap, &line, hatch, 0.65);
        }
    }
}

fn stroke_pixmap_path(pixmap: &mut Pixmap, path: &Path, shade: [u8; 4], width: f32) {
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(
        path,
        &symbol_paint(shade),
        &stroke,
        Transform::identity(),
        None,
    );
}

fn feature_seed(bounds: [f64; 4], density: u8, kind: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bounds
        .into_iter()
        .flat_map(|value| value.to_bits().to_le_bytes())
        .chain([density])
        .chain(kind.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn next_unit(state: &mut u64) -> f64 {
    (next_random(state) >> 11) as f64 / (1_u64 << 53) as f64
}

fn stroke_source_path(
    pixmap: &mut Pixmap,
    points: &[Point],
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    shade: [u8; 4],
    width: f32,
) {
    let raw: Vec<_> = points.iter().map(|point| point.0).collect();
    stroke_raw_path(
        pixmap,
        &raw,
        map_bounds,
        scale,
        origin,
        tile_bounds,
        shade,
        width,
    );
}

fn stroke_raw_path(
    pixmap: &mut Pixmap,
    points: &[[f64; 2]],
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    shade: [u8; 4],
    width: f32,
) {
    let projected: Vec<_> = points
        .iter()
        .map(|point| project(point[0], point[1], map_bounds))
        .collect();
    if !path_bounds(&projected).is_some_and(|bounds| intersects(bounds, tile_bounds)) {
        return;
    }
    let Some(path) = tile_path(&projected, scale, origin, false) else {
        return;
    };
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint(shade), &stroke, Transform::identity(), None);
}

fn stroke_and_fill_source_polygon(
    pixmap: &mut Pixmap,
    polygon: &super::WaterPolygon,
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    fill: Option<[u8; 4]>,
    stroke: Option<([u8; 4], f32)>,
) {
    let projected: Vec<Vec<_>> = polygon
        .rings
        .iter()
        .map(|ring| {
            ring.iter()
                .map(|point| project(point.0[0], point.0[1], map_bounds))
                .collect()
        })
        .collect();
    let bounds = projected
        .iter()
        .filter_map(|ring| path_bounds(ring))
        .reduce(|left, right| {
            [
                left[0].min(right[0]),
                left[1].min(right[1]),
                left[2].max(right[2]),
                left[3].max(right[3]),
            ]
        });
    if !bounds.is_some_and(|bounds| intersects(bounds, tile_bounds)) {
        return;
    }
    let mut builder = PathBuilder::new();
    for ring in &projected {
        for (index, point) in ring.iter().enumerate() {
            let x = (point.0 * scale - origin.0) as f32;
            let y = (point.1 * scale - origin.1) as f32;
            if index == 0 {
                builder.move_to(x, y);
            } else {
                builder.line_to(x, y);
            }
        }
        builder.close();
    }
    let Some(path) = builder.finish() else {
        return;
    };
    if let Some(shade) = fill {
        pixmap.fill_path(
            &path,
            &paint(shade),
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
    }
    if let Some((shade, width)) = stroke {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint(shade), &stroke, Transform::identity(), None);
    }
}

fn tile_path(points: &[(f64, f64)], scale: f64, origin: (f64, f64), close: bool) -> Option<Path> {
    let mut builder = PathBuilder::new();
    for (index, point) in points.iter().enumerate() {
        let x = (point.0 * scale - origin.0) as f32;
        let y = (point.1 * scale - origin.1) as f32;
        if index == 0 {
            builder.move_to(x, y);
        } else {
            builder.line_to(x, y);
        }
    }
    if close {
        builder.close();
    }
    builder.finish()
}

fn path_bounds(points: &[(f64, f64)]) -> Option<[f64; 4]> {
    let first = *points.first()?;
    Some(
        points
            .iter()
            .skip(1)
            .fold([first.0, first.1, first.0, first.1], |mut bounds, point| {
                bounds[0] = bounds[0].min(point.0);
                bounds[1] = bounds[1].min(point.1);
                bounds[2] = bounds[2].max(point.0);
                bounds[3] = bounds[3].max(point.1);
                bounds
            }),
    )
}

fn intersects(a: [f64; 4], b: [f64; 4]) -> bool {
    a[2] >= b[0] && a[0] <= b[2] && a[3] >= b[1] && a[1] <= b[3]
}

fn project(longitude: f64, latitude: f64, [west, south, east, north]: [f64; 4]) -> (f64, f64) {
    (
        ((longitude - west) / (east - west) * WIDTH).clamp(0.0, WIDTH),
        ((north - latitude) / (north - south) * HEIGHT).clamp(0.0, HEIGHT),
    )
}

fn paint(rgba: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
    // Some source coastlines cross many tile widths; tiny-skia's hairline AA
    // path panics on those clipped extremes. The high-density pyramid and
    // AVIF sampling provide smooth display without enabling scanline AA here.
    paint.anti_alias = false;
    paint
}

fn symbol_paint(rgba: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
    paint.anti_alias = true;
    paint
}

fn color(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn with_alpha(mut rgba: [u8; 4], alpha: u8) -> [u8; 4] {
    rgba[3] = alpha;
    rgba
}

fn mix(left: [u8; 4], right: [u8; 4]) -> [u8; 4] {
    [
        ((u16::from(left[0]) + u16::from(right[0])) / 2) as u8,
        ((u16::from(left[1]) + u16::from(right[1])) / 2) as u8,
        ((u16::from(left[2]) + u16::from(right[2])) / 2) as u8,
        ((u16::from(left[3]) + u16::from(right[3])) / 2) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{ElevationCell, ElevationLayer, ForestLayer, ForestRegion, LayerSource};
    use std::collections::BTreeMap;

    fn layer_source() -> LayerSource {
        LayerSource {
            name: "fixture".into(),
            version: "1".into(),
            url: "https://example.invalid".into(),
            license: "test-only".into(),
            file_count: 1,
            files_sha256: BTreeMap::new(),
            verification_status: "fixture".into(),
        }
    }

    fn paper_fixture(kind: &str, include_forest: bool) -> Package {
        Package {
            schema: 2,
            year: 1544,
            bounds: [0.0, 0.0, WIDTH, HEIGHT],
            source: super::super::Source {
                name: "fixture",
                version: "1".into(),
                url: "https://example.invalid",
                license: "test-only",
                files_sha256: BTreeMap::new(),
                verification_status: "fixture",
            },
            roads: Vec::new(),
            water: Vec::new(),
            elevation: ElevationLayer {
                source: layer_source(),
                cells: vec![ElevationCell {
                    bounds: [1.35, 797.5, 2.65, 799.4],
                    band_m: 1_000,
                }],
                contours: Vec::new(),
            },
            forest: ForestLayer {
                source: layer_source(),
                coverage: Vec::new(),
                regions: include_forest
                    .then(|| ForestRegion {
                        bounds: [1.65, 797.6, 2.35, 798.8],
                        density: 3,
                        kind: kind.into(),
                    })
                    .into_iter()
                    .collect(),
            },
            tiles: TilePyramid {
                format: "avif",
                tile_size: 64,
                gutter: TILE_GUTTER,
                max_zoom: 6,
                content_sha256: String::new(),
                entries: Vec::new(),
            },
        }
    }

    fn donut_fixture() -> Package {
        let mut package = paper_fixture("broadleaf", false);
        package.elevation.cells.clear();
        package.water = vec![super::super::WaterPolygon {
            rings: vec![
                vec![
                    Point([10.0, 750.0]),
                    Point([50.0, 750.0]),
                    Point([50.0, 790.0]),
                    Point([10.0, 790.0]),
                    Point([10.0, 750.0]),
                ],
                vec![
                    Point([25.0, 765.0]),
                    Point([35.0, 765.0]),
                    Point([35.0, 775.0]),
                    Point([25.0, 775.0]),
                    Point([25.0, 765.0]),
                ],
            ],
        }];
        package
    }

    fn pixel(tile: &Pixmap, x: usize, y: usize) -> [u8; 4] {
        let offset = (y * tile.width() as usize + x) * 4;
        tile.data()[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn close_paper_symbols_are_deterministic_and_kind_specific() {
        let conifer = paper_fixture("conifer", true);
        let first = render(&conifer, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        let second = render(&conifer, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        assert_eq!(first.data(), second.data());

        let broadleaf = paper_fixture("broadleaf", true);
        let broadleaf = render(&broadleaf, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        assert_ne!(first.data(), broadleaf.data());
    }

    #[test]
    fn close_paper_symbols_are_stable_across_tile_gutters() {
        let package = paper_fixture("mixed", true);
        let left = render(&package, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        let right = render(&package, 64, TILE_GUTTER, 32.0, 1, 0, PAPER).unwrap();
        let stride = left.width() as usize * 4;
        for y in 0..left.height() as usize {
            for global_x in 60..68 {
                let left_x = global_x + usize::from(TILE_GUTTER);
                let right_x = global_x - 60;
                let left_pixel = &left.data()[y * stride + left_x * 4..][..4];
                let right_pixel = &right.data()[y * stride + right_x * 4..][..4];
                let maximum_delta = left_pixel
                    .iter()
                    .zip(right_pixel)
                    .map(|(left, right)| left.abs_diff(*right))
                    .max()
                    .unwrap_or_default();
                assert!(
                    maximum_delta <= 1,
                    "seam mismatch at ({global_x}, {y}): {left_pixel:?} != {right_pixel:?}"
                );
            }
        }
    }

    #[test]
    fn close_elevation_uses_sparse_profile_stamp_instead_of_square_fill() {
        let package = paper_fixture("broadleaf", false);
        let tile = render(&package, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        let land = PAPER.land;
        let changed = tile
            .data()
            .chunks_exact(4)
            .filter(|pixel| *pixel != land)
            .count();
        assert!(changed > 20, "profile stamp did not render");
        assert!(changed < 1_800, "close elevation reverted to a filled cell");
    }

    #[test]
    fn representative_paper_tile_has_deterministic_png_preview_hook() {
        let package = paper_fixture("mixed", true);
        let tile = render(&package, 256, TILE_GUTTER, 64.0, 0, 0, PAPER).unwrap();
        let first = tile.encode_png().unwrap();
        let second = tile.encode_png().unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with(b"\x89PNG\r\n\x1a\n"));
        if let Some(path) = std::env::var_os("STRATEGIC_MAP_PREVIEW_PNG") {
            std::fs::write(path, first).unwrap();
        }
    }

    #[test]
    fn compound_water_polygon_keeps_donut_hole_as_land() {
        let tile = render(&donut_fixture(), 64, TILE_GUTTER, 1.0, 0, 0, PAPER).unwrap();
        assert_eq!(pixel(&tile, 24, 24), PAPER.water);
        assert_eq!(pixel(&tile, 34, 34), PAPER.land);
    }
}
