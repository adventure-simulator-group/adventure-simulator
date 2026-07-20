use image::{ExtendedColorType, ImageEncoder, codecs::avif::AvifEncoder};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

use super::{Package, Point, TileEntry, TilePyramid};

const WIDTH: f64 = 1_200.0;
const HEIGHT: f64 = 800.0;
const TILE_GUTTER: u8 = 4;

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
    let render_size = tile_size + 2 * u32::from(gutter);
    let mut pixmap = Pixmap::new(render_size, render_size).ok_or("invalid tile dimensions")?;
    pixmap.fill(color(palette.land));
    let origin = (
        f64::from(tile_x) * f64::from(tile_size) - f64::from(gutter),
        f64::from(tile_y) * f64::from(tile_size) - f64::from(gutter),
    );
    let logical_bounds = [
        origin.0 / scale,
        origin.1 / scale,
        (origin.0 + f64::from(render_size)) / scale,
        (origin.1 + f64::from(render_size)) / scale,
    ];

    if scale < 32.0 {
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
    }
    for ring in &package.water {
        stroke_and_fill_source_path(
            &mut pixmap,
            ring,
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
        if scale < 32.0 {
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
            fill_forest_symbols(
                &mut pixmap,
                region.bounds,
                region.density,
                package.bounds,
                scale,
                origin,
                logical_bounds,
                shade,
            );
        }
    }
    for contour in &package.elevation.contours {
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
    Ok(pixmap)
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

fn fill_forest_symbols(
    pixmap: &mut Pixmap,
    [west, south, east, north]: [f64; 4],
    density: u8,
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    mut shade: [u8; 4],
) {
    let (left, top) = project(west, north, map_bounds);
    let (right, bottom) = project(east, south, map_bounds);
    if !intersects([left, top, right, bottom], tile_bounds) {
        return;
    }
    shade[3] = match density {
        1 => 105,
        2 => 145,
        _ => 180,
    };
    let positions = [
        (0.25, 0.30),
        (0.72, 0.68),
        (0.70, 0.24),
        (0.30, 0.76),
        (0.50, 0.50),
        (0.86, 0.82),
    ];
    let count = usize::from(density.clamp(1, 3)) * 2;
    for (relative_x, relative_y) in positions.into_iter().take(count) {
        let x = ((left + (right - left) * relative_x) * scale - origin.0) as f32;
        let y = ((top + (bottom - top) * relative_y) * scale - origin.1) as f32;
        let Some(path) = PathBuilder::from_circle(x, y, 2.4) else {
            continue;
        };
        pixmap.fill_path(
            &path,
            &paint(shade),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
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

fn stroke_and_fill_source_path(
    pixmap: &mut Pixmap,
    points: &[Point],
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    fill: Option<[u8; 4]>,
    stroke: Option<([u8; 4], f32)>,
) {
    let projected: Vec<_> = points
        .iter()
        .map(|point| project(point.0[0], point.0[1], map_bounds))
        .collect();
    if !path_bounds(&projected).is_some_and(|bounds| intersects(bounds, tile_bounds)) {
        return;
    }
    let Some(path) = tile_path(&projected, scale, origin, true) else {
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

fn color(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn mix(left: [u8; 4], right: [u8; 4]) -> [u8; 4] {
    [
        ((u16::from(left[0]) + u16::from(right[0])) / 2) as u8,
        ((u16::from(left[1]) + u16::from(right[1])) / 2) as u8,
        ((u16::from(left[2]) + u16::from(right[2])) / 2) as u8,
        ((u16::from(left[3]) + u16::from(right[3])) / 2) as u8,
    ]
}
