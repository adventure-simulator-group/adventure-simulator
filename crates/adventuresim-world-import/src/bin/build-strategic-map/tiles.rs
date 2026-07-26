use image::{ExtendedColorType, ImageEncoder, codecs::avif::AvifEncoder};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tiny_skia::{
    Color, FillRule, IntRect, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke,
    Transform,
};

use super::{Package, Point, TileEntry, TilePyramid};

const WIDTH: f64 = 1_200.0;
const HEIGHT: f64 = 800.0;
const TILE_GUTTER: u8 = 4;
const MIN_TILE_SIZE: u32 = 64;
const MAX_TILE_SIZE: u32 = 2_048;
const MAX_TILE_COUNT: usize = 100_000;
const RENDER_MARGIN: u32 = 12;
const NATIVE_DETAIL_BOUNDS: [f64; 4] = adventuresim_world_schema::PLAYABLE_BOUNDS;
const FOREST_CANOPY_THRESHOLD_PERCENT: f64 = 20.0;
const CANOPY_CELLS_PER_DEGREE: usize = 1_000;
const RELIEF_STEP_DEGREES: f64 = 0.01;

#[derive(Debug)]
struct ForestField {
    cells: HashMap<(i64, i64), f64>,
    origin: (f64, f64),
    step: (f64, f64),
}

impl ForestField {
    fn from_regions(regions: &[crate::raster::ForestRegion], map_bounds: [f64; 4]) -> Option<Self> {
        let mut samples = Vec::with_capacity(regions.len());
        let mut widths = Vec::with_capacity(regions.len());
        let mut heights = Vec::with_capacity(regions.len());
        for region in regions {
            let [west, south, east, north] = region.bounds;
            let (left, top) = project(west, north, map_bounds);
            let (right, bottom) = project(east, south, map_bounds);
            let width = right - left;
            let height = bottom - top;
            if width <= f64::EPSILON || height <= f64::EPSILON {
                continue;
            }
            samples.push(((left + right) * 0.5, (top + bottom) * 0.5, region.density));
            widths.push(width);
            heights.push(height);
        }
        if samples.is_empty() {
            return None;
        }

        widths.sort_by(f64::total_cmp);
        heights.sort_by(f64::total_cmp);
        let step = (widths[widths.len() / 2], heights[heights.len() / 2]);
        let origin = samples.iter().fold(
            (f64::INFINITY, f64::INFINITY),
            |(minimum_x, minimum_y), (x, y, _)| (minimum_x.min(*x), minimum_y.min(*y)),
        );
        let mut cells: HashMap<(i64, i64), f64> = HashMap::with_capacity(samples.len());
        for (x, y, density) in samples {
            let column = ((x - origin.0) / step.0).round() as i64;
            let row = ((y - origin.1) / step.1).round() as i64;
            cells
                .entry((column, row))
                .and_modify(|stored| *stored = stored.max(f64::from(density)))
                .or_insert(f64::from(density));
        }
        Some(Self {
            cells,
            origin,
            step,
        })
    }

    fn density_at(&self, logical_x: f64, logical_y: f64) -> f64 {
        let base_x = (logical_x - self.origin.0) / self.step.0;
        let base_y = (logical_y - self.origin.1) / self.step.1;
        let warp_x = fractal_noise(base_x * 0.23, base_y * 0.23, 0x0f67_47f4_6c9a_31b5) * 0.40
            + fractal_noise(base_x * 1.19, base_y * 1.19, 0x8b72_46dc_a913_5ef0) * 0.14;
        let warp_y = fractal_noise(base_x * 0.23, base_y * 0.23, 0xca62_d13b_98f4_07e1) * 0.40
            + fractal_noise(base_x * 1.19, base_y * 1.19, 0x34d9_81a7_f5c0_62be) * 0.14;
        let x = base_x + warp_x;
        let y = base_y + warp_y;
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let tx = smoothstep(x - x.floor());
        let ty = smoothstep(y - y.floor());
        let sample = |column, row| self.cells.get(&(column, row)).copied().unwrap_or_default();
        let top = lerp(sample(x0, y0), sample(x0 + 1, y0), tx);
        let bottom = lerp(sample(x0, y0 + 1), sample(x0 + 1, y0 + 1), tx);
        let interpolated = lerp(top, bottom, ty);
        let boundary_detail = fractal_noise(x * 1.71, y * 1.71, 0x5e41_bdf0_216d_893c) * 5.4
            + fractal_noise(x * 4.83, y * 4.83, 0x12f7_8a4c_d963_b05e) * 1.65;
        interpolated + boundary_detail
    }
}

#[derive(Debug)]
struct CanopyLevel {
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

#[derive(Debug)]
struct CanopyPyramid {
    bounds: [f64; 4],
    levels: Vec<CanopyLevel>,
}

impl CanopyPyramid {
    fn from_terrain(
        terrain: &adventuresim_terrain::TerrainPack,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let bounds = terrain.bounds();
        let width = ((bounds[2] - bounds[0]) * CANOPY_CELLS_PER_DEGREE as f64).round() as usize;
        let height = ((bounds[3] - bounds[1]) * CANOPY_CELLS_PER_DEGREE as f64).round() as usize;
        if width == 0 || height == 0 {
            return Err("terrain bounds cannot produce a canopy pyramid".into());
        }
        let mut coverage = vec![0_u8; width * height];
        let longitude_step = (bounds[2] - bounds[0]) / width as f64;
        let latitude_step = (bounds[3] - bounds[1]) / height as f64;
        for y in 0..height {
            let latitude = bounds[3] - (y as f64 + 0.5) * latitude_step;
            for x in 0..width {
                let longitude = bounds[0] + (x as f64 + 0.5) * longitude_step;
                let forest = terrain.cell(latitude, longitude)?.is_some_and(|cell| {
                    f64::from(cell.canopy_percent) >= FOREST_CANOPY_THRESHOLD_PERCENT
                });
                coverage[y * width + x] = if forest { u8::MAX } else { 0 };
            }
        }
        Ok(Self::from_base(bounds, width, height, coverage))
    }

    fn from_base(bounds: [f64; 4], width: usize, height: usize, coverage: Vec<u8>) -> Self {
        debug_assert_eq!(coverage.len(), width * height);
        let mut levels = vec![CanopyLevel {
            width,
            height,
            coverage,
        }];
        while levels
            .last()
            .is_some_and(|level| level.width > 1 || level.height > 1)
        {
            let previous = levels.last().expect("canopy base level exists");
            let next_width = previous.width.div_ceil(2);
            let next_height = previous.height.div_ceil(2);
            let mut next = Vec::with_capacity(next_width * next_height);
            for y in 0..next_height {
                for x in 0..next_width {
                    let mut sum = 0_u16;
                    let mut count = 0_u16;
                    for child_y in y * 2..(y * 2 + 2).min(previous.height) {
                        for child_x in x * 2..(x * 2 + 2).min(previous.width) {
                            sum += u16::from(previous.coverage[child_y * previous.width + child_x]);
                            count += 1;
                        }
                    }
                    next.push(((sum + count / 2) / count) as u8);
                }
            }
            levels.push(CanopyLevel {
                width: next_width,
                height: next_height,
                coverage: next,
            });
        }
        Self { bounds, levels }
    }

    fn coverage_at(
        &self,
        latitude: f64,
        longitude: f64,
        longitude_per_pixel: f64,
        latitude_per_pixel: f64,
    ) -> f64 {
        let [west, south, east, north] = self.bounds;
        if longitude < west || longitude >= east || latitude <= south || latitude > north {
            return 0.0;
        }
        let source_footprint = (longitude_per_pixel.abs() * CANOPY_CELLS_PER_DEGREE as f64)
            .max(latitude_per_pixel.abs() * CANOPY_CELLS_PER_DEGREE as f64)
            .max(1.0);
        let lod = source_footprint.log2();
        let lower = (lod.floor() as usize).min(self.levels.len() - 1);
        let upper = (lower + 1).min(self.levels.len() - 1);
        let blend = if lower == upper { 0.0 } else { lod.fract() };
        let sample = |level_index: usize| {
            let level = &self.levels[level_index];
            let divisor = (1_u64 << level_index) as f64;
            let x = (longitude - west) * CANOPY_CELLS_PER_DEGREE as f64 / divisor - 0.5;
            let y = (north - latitude) * CANOPY_CELLS_PER_DEGREE as f64 / divisor - 0.5;
            bilinear_coverage(level, x, y)
        };
        lerp(sample(lower), sample(upper), blend) / 255.0
    }
}

fn bilinear_coverage(level: &CanopyLevel, x: f64, y: f64) -> f64 {
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let tx = x - x.floor();
    let ty = y - y.floor();
    let sample = |column: isize, row: isize| {
        if column < 0 || row < 0 || column >= level.width as isize || row >= level.height as isize {
            0.0
        } else {
            f64::from(level.coverage[row as usize * level.width + column as usize])
        }
    };
    let top = lerp(sample(x0, y0), sample(x0 + 1, y0), tx);
    let bottom = lerp(sample(x0, y0 + 1), sample(x0 + 1, y0 + 1), tx);
    lerp(top, bottom, ty)
}

#[derive(Clone, Copy, Debug, Default)]
struct ReliefCell {
    hilly_fraction_percent: u8,
}

#[derive(Debug)]
struct ReliefField {
    west: f64,
    north: f64,
    columns: usize,
    rows: usize,
    cells: Vec<ReliefCell>,
}

impl ReliefField {
    fn from_terrain(
        terrain: &adventuresim_terrain::TerrainPack,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let [west, south, east, north] = terrain.bounds();
        let columns = ((east - west) / RELIEF_STEP_DEGREES).ceil() as usize;
        let rows = ((north - south) / RELIEF_STEP_DEGREES).ceil() as usize;
        let mut cells = vec![ReliefCell::default(); columns * rows];
        let sample_offsets = [-1.0 / 3.0, 0.0, 1.0 / 3.0];
        for row in 0..rows {
            let latitude = north - (row as f64 + 0.5) * RELIEF_STEP_DEGREES;
            for column in 0..columns {
                let longitude = west + (column as f64 + 0.5) * RELIEF_STEP_DEGREES;
                let mut hilly = 0_u8;
                let mut samples = 0_u8;
                for offset_y in sample_offsets {
                    for offset_x in sample_offsets {
                        let sample_latitude = latitude - offset_y * RELIEF_STEP_DEGREES;
                        let sample_longitude = longitude + offset_x * RELIEF_STEP_DEGREES;
                        if let Some(cell) = terrain.cell(sample_latitude, sample_longitude)? {
                            hilly += u8::from(cell.hilly_fraction_percent >= 50);
                            samples += 1;
                        }
                    }
                }
                if samples == 0 {
                    continue;
                }
                cells[row * columns + column] = ReliefCell {
                    hilly_fraction_percent: u8::try_from(
                        u16::from(hilly) * 100 / u16::from(samples),
                    )
                    .unwrap_or(100),
                };
            }
        }
        Ok(Self {
            west,
            north,
            columns,
            rows,
            cells,
        })
    }

    fn hilly_at(&self, latitude: f64, longitude: f64) -> bool {
        let x = (longitude - self.west) / RELIEF_STEP_DEGREES - 0.5;
        let y = (self.north - latitude) / RELIEF_STEP_DEGREES - 0.5;
        if x < -1.0 || y < -1.0 || x > self.columns as f64 || y > self.rows as f64 {
            return false;
        }
        let warp_x = fractal_noise(x * 0.07, y * 0.07, 0xd145_74c1_a6b2_91e5) * 0.46;
        let warp_y = fractal_noise(x * 0.07, y * 0.07, 0x7d2e_89f3_5cab_4011) * 0.46;
        let x = x + warp_x;
        let y = y + warp_y;
        let x0 = x.floor() as isize;
        let y0 = y.floor() as isize;
        let sample = |column: isize, row: isize| -> f64 {
            if column < 0 || row < 0 || column >= self.columns as isize || row >= self.rows as isize
            {
                0.0
            } else {
                f64::from(
                    self.cells[row as usize * self.columns + column as usize]
                        .hilly_fraction_percent,
                )
            }
        };
        let tx = smoothstep(x - x.floor());
        let ty = smoothstep(y - y.floor());
        let score = lerp(
            lerp(sample(x0, y0), sample(x0 + 1, y0), tx),
            lerp(sample(x0, y0 + 1), sample(x0 + 1, y0 + 1), tx),
            ty,
        ) + fractal_noise(x * 0.83, y * 0.83, 0x62ec_192f_b761_0a4d) * 5.0;
        score >= 10.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TileConfig {
    pub tile_size: u32,
    pub max_zoom: u8,
}

impl Default for TileConfig {
    fn default() -> Self {
        Self {
            tile_size: 512,
            // The bounded map reaches approximately 25 m/pixel at z3. The
            // former continental map needed z7 for equivalent native detail.
            max_zoom: 3,
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    land: [u8; 4],
    paper_fiber: [u8; 4],
    paper_fleck: [u8; 4],
    water: [u8; 4],
    water_edge: [u8; 4],
    wetland: [u8; 4],
    wetland_edge: [u8; 4],
    road: [u8; 4],
    ferry: [u8; 4],
    forest_sparse: [u8; 4],
    forest_deep: [u8; 4],
    inferred_road: [u8; 4],
    hilly_open: [u8; 4],
    cultivated: [u8; 4],
    cultivated_edge: [u8; 4],
}

const PAPER: Palette = Palette {
    land: [230, 225, 203, 255],
    paper_fiber: [111, 91, 66, 10],
    paper_fleck: [151, 125, 86, 7],
    water: [184, 201, 197, 255],
    water_edge: [103, 119, 116, 190],
    wetland: [112, 139, 102, 105],
    wetland_edge: [74, 104, 72, 120],
    road: [92, 79, 57, 230],
    ferry: [91, 88, 78, 180],
    forest_sparse: [105, 139, 91, 120],
    forest_deep: [49, 94, 55, 155],
    inferred_road: [118, 91, 61, 145],
    hilly_open: [191, 159, 115, 180],
    cultivated: [184, 159, 91, 72],
    cultivated_edge: [116, 91, 48, 118],
};

pub(super) fn build(
    package: &Package,
    native_terrain: Option<&adventuresim_terrain::TerrainPack>,
    config: TileConfig,
) -> Result<(TilePyramid, Vec<u8>), Box<dyn std::error::Error>> {
    if !(MIN_TILE_SIZE..=MAX_TILE_SIZE).contains(&config.tile_size)
        || !config.tile_size.is_power_of_two()
        || config.max_zoom > 8
        || pyramid_tile_count(config.tile_size, config.max_zoom) > MAX_TILE_COUNT
    {
        return Err("strategic map tile configuration is outside its bound".into());
    }
    let forest_field = ForestField::from_regions(&package.forest.regions, package.bounds);
    let canopy_pyramid = native_terrain
        .map(CanopyPyramid::from_terrain)
        .transpose()?;
    let relief_field = native_terrain.map(ReliefField::from_terrain).transpose()?;
    let mut bytes = Vec::new();
    let mut entries = Vec::new();
    for zoom in 0..=config.max_zoom {
        let scale = f64::from(1_u32 << zoom);
        let span = f64::from(config.tile_size) / scale;
        let columns = (WIDTH / span).ceil() as u16;
        let rows = (HEIGHT / span).ceil() as u16;
        let coordinates = tile_grid(columns, rows)
            .into_iter()
            .filter(|&(x, y)| {
                config.max_zoom < 7
                    || zoom < config.max_zoom
                    || tile_intersects_geographic_detail(
                        package.bounds,
                        span,
                        u32::from(x),
                        u32::from(y),
                    )
            })
            .collect::<Vec<_>>();
        let quality = if zoom == config.max_zoom { 95 } else { 82 };
        let encoded_tiles: Result<Vec<Vec<u8>>, String> = coordinates
            .par_iter()
            .map(|&(x, y)| {
                let tile = render_with_forest_field(
                    package,
                    native_terrain,
                    forest_field.as_ref(),
                    canopy_pyramid.as_ref(),
                    relief_field.as_ref(),
                    config.tile_size,
                    TILE_GUTTER,
                    scale,
                    x,
                    y,
                    PAPER,
                )
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

fn tile_grid(columns: u16, rows: u16) -> Vec<(u16, u16)> {
    (0..rows)
        .flat_map(|y| (0..columns).map(move |x| (x, y)))
        .collect()
}

fn tile_intersects_geographic_detail(map_bounds: [f64; 4], span: f64, x: u32, y: u32) -> bool {
    let [west, south, east, north] = NATIVE_DETAIL_BOUNDS;
    let (left, top) = project(west, north, map_bounds);
    let (right, bottom) = project(east, south, map_bounds);
    intersects(
        [
            f64::from(x) * span,
            f64::from(y) * span,
            f64::from(x + 1) * span,
            f64::from(y + 1) * span,
        ],
        [left, top, right, bottom],
    )
}

fn pyramid_tile_count(tile_size: u32, max_zoom: u8) -> usize {
    (0..=max_zoom)
        .map(|zoom| {
            let scale = 1_u32 << zoom;
            let columns = (WIDTH as u32 * scale).div_ceil(tile_size) as usize;
            let rows = (HEIGHT as u32 * scale).div_ceil(tile_size) as usize;
            columns * rows
        })
        .sum()
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

#[cfg(test)]
fn render(
    package: &Package,
    tile_size: u32,
    gutter: u8,
    scale: f64,
    tile_x: u16,
    tile_y: u16,
    palette: Palette,
) -> Result<Pixmap, Box<dyn std::error::Error>> {
    let forest_field = ForestField::from_regions(&package.forest.regions, package.bounds);
    render_with_forest_field(
        package,
        None,
        forest_field.as_ref(),
        None,
        None,
        tile_size,
        gutter,
        scale,
        tile_x,
        tile_y,
        palette,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_with_forest_field(
    package: &Package,
    native_terrain: Option<&adventuresim_terrain::TerrainPack>,
    forest_field: Option<&ForestField>,
    canopy_pyramid: Option<&CanopyPyramid>,
    relief_field: Option<&ReliefField>,
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
    draw_parchment_texture(&mut pixmap, scale, origin, palette);

    draw_land_cover(
        &mut pixmap,
        native_terrain,
        forest_field,
        canopy_pyramid,
        relief_field,
        package.bounds,
        scale,
        origin,
        palette,
    )?;
    // Cultivation is painted from the same exact final-pack squares used by
    // legality. Wetlands, water, and roads remain visually above it.
    for polygon in &package.cultivated {
        stroke_and_fill_source_polygon(
            &mut pixmap,
            polygon,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            Some(palette.cultivated),
            Some((palette.cultivated_edge, 0.42)),
            false,
        );
    }
    for polygon in &package.wetlands {
        stroke_and_fill_source_polygon(
            &mut pixmap,
            polygon,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            Some(palette.wetland),
            Some((palette.wetland_edge, 0.65)),
            true,
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
            false,
        );
    }
    for road in &package.roads {
        let Some((shade, width)) = road_style(road.importance, scale, &road.kind, palette) else {
            continue;
        };
        stroke_source_path(
            &mut pixmap,
            &road.points,
            package.bounds,
            scale,
            origin,
            logical_bounds,
            shade,
            width,
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

fn draw_parchment_texture(pixmap: &mut Pixmap, scale: f64, origin: (f64, f64), palette: Palette) {
    let cell_size = 112.0;
    let zoom = scale.log2().round().clamp(0.0, 8.0) as u8;
    let right = origin.0 + f64::from(pixmap.width());
    let bottom = origin.1 + f64::from(pixmap.height());
    let first_x = (origin.0 / cell_size).floor() as i64 - 1;
    let last_x = (right / cell_size).ceil() as i64 + 1;
    let first_y = (origin.1 / cell_size).floor() as i64 - 1;
    let last_y = (bottom / cell_size).ceil() as i64 + 1;

    for cell_y in first_y..=last_y {
        for cell_x in first_x..=last_x {
            let mut random = grid_seed(cell_x, cell_y, zoom, "parchment");
            let x = cell_x as f64 * cell_size + 8.0 + next_unit(&mut random) * 96.0 - origin.0;
            let y = cell_y as f64 * cell_size + 8.0 + next_unit(&mut random) * 96.0 - origin.1;
            let length = 3.0 + next_unit(&mut random) * 8.0;
            let slope = (next_unit(&mut random) - 0.5) * 1.8;
            let mut fiber = PathBuilder::new();
            fiber.move_to(x as f32, y as f32);
            fiber.line_to((x + length) as f32, (y + slope) as f32);
            if let Some(fiber) = fiber.finish() {
                stroke_pixmap_path(pixmap, &fiber, palette.paper_fiber, 0.55);
            }

            if next_random(&mut random) % 5 == 0
                && let Some(fleck) = Rect::from_xywh(
                    (x + next_unit(&mut random) * 13.0) as f32,
                    (y + 3.0 + next_unit(&mut random) * 10.0) as f32,
                    0.9,
                    0.9,
                )
            {
                pixmap.fill_rect(
                    fleck,
                    &symbol_paint(palette.paper_fleck),
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

fn road_style(importance: u8, scale: f64, kind: &str, palette: Palette) -> Option<([u8; 4], f32)> {
    let maximum_importance = if scale < 2.0 {
        0
    } else if scale < 4.0 {
        1
    } else if scale < 8.0 {
        2
    } else if scale < 16.0 {
        3
    } else {
        4
    };
    if importance > maximum_importance {
        return None;
    }
    if kind == "ferry" {
        return Some((with_alpha(palette.ferry, 165), 0.95));
    }
    if kind == "inferred" {
        return (scale >= 8.0).then_some((palette.inferred_road, 0.82));
    }
    let base_width = if scale >= 16.0 { 1.75 } else { 1.45 };
    let width = (base_width - f32::from(importance) * 0.16).max(0.85);
    let alpha = 225_u8.saturating_sub(importance.saturating_mul(18));
    Some((with_alpha(palette.road, alpha), width))
}

fn draw_land_cover(
    pixmap: &mut Pixmap,
    native_terrain: Option<&adventuresim_terrain::TerrainPack>,
    forest_field: Option<&ForestField>,
    canopy_pyramid: Option<&CanopyPyramid>,
    relief_field: Option<&ReliefField>,
    [west, south, east, north]: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    palette: Palette,
) -> Result<(), Box<dyn std::error::Error>> {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let data = pixmap.data_mut();
    let sample_offset = 0.25 / scale;
    let longitude_per_pixel = (east - west) / (WIDTH * scale);
    let latitude_per_pixel = (north - south) / (HEIGHT * scale);
    for pixel_y in 0..height {
        let logical_y = (origin.1 + pixel_y as f64 + 0.5) / scale;
        for pixel_x in 0..width {
            let logical_x = (origin.0 + pixel_x as f64 + 0.5) / scale;
            let mut forest_samples = 0.0;
            let mut hilly_samples = 0.0;
            let mut combined_samples = 0.0;
            for offset_y in [-sample_offset, sample_offset] {
                for offset_x in [-sample_offset, sample_offset] {
                    let sample_x = logical_x + offset_x;
                    let sample_y = logical_y + offset_y;
                    let longitude = west + sample_x / WIDTH * (east - west);
                    let latitude = north - sample_y / HEIGHT * (north - south);
                    let native = if scale >= 64.0 {
                        native_terrain
                            .map(|terrain| terrain.cell(latitude, longitude))
                            .transpose()?
                            .flatten()
                    } else {
                        None
                    };
                    let forest_coverage = canopy_pyramid.map_or_else(
                        || {
                            f64::from(u8::from(forest_field.is_some_and(|field| {
                                field.density_at(sample_x, sample_y)
                                    >= FOREST_CANOPY_THRESHOLD_PERCENT
                            })))
                        },
                        |pyramid| {
                            pyramid.coverage_at(
                                latitude,
                                longitude,
                                longitude_per_pixel,
                                latitude_per_pixel,
                            )
                        },
                    );
                    let hilly = native.map_or_else(
                        || relief_field.is_some_and(|field| field.hilly_at(latitude, longitude)),
                        |cell| cell.hilly_fraction_percent >= 50,
                    );
                    if hilly {
                        hilly_samples += 1.0 - forest_coverage;
                        combined_samples += forest_coverage;
                    } else {
                        forest_samples += forest_coverage;
                    }
                }
            }
            if forest_samples == 0.0 && hilly_samples == 0.0 && combined_samples == 0.0 {
                continue;
            }
            let offset = (pixel_y * width + pixel_x) * 4;
            blend_opaque_pixel(
                &mut data[offset..offset + 4],
                palette.hilly_open,
                hilly_samples * 0.25,
            );
            blend_opaque_pixel(
                &mut data[offset..offset + 4],
                palette.forest_sparse,
                forest_samples * 0.25,
            );
            blend_opaque_pixel(
                &mut data[offset..offset + 4],
                palette.forest_deep,
                combined_samples * 0.25,
            );
        }
    }
    Ok(())
}

fn blend_opaque_pixel(destination: &mut [u8], source: [u8; 4], coverage: f64) {
    let alpha = (f64::from(source[3]) * coverage).round().clamp(0.0, 255.0) as u32;
    if alpha == 0 {
        return;
    }
    for channel in 0..3 {
        destination[channel] = ((u32::from(source[channel]) * alpha
            + u32::from(destination[channel]) * (255 - alpha)
            + 127)
            / 255) as u8;
    }
    destination[3] = 255;
}

fn fractal_noise(mut x: f64, mut y: f64, seed: u64) -> f64 {
    let mut amplitude = 0.58;
    let mut total = 0.0;
    let mut weight = 0.0;
    for octave in 0_u64..4 {
        total += value_noise(x, y, seed.wrapping_add(octave * 0x9e37_79b9)) * amplitude;
        weight += amplitude;
        x = x * 2.03 + 17.7;
        y = y * 2.03 - 11.3;
        amplitude *= 0.5;
    }
    total / weight
}

fn value_noise(x: f64, y: f64, seed: u64) -> f64 {
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let tx = smoothstep(x - x.floor());
    let ty = smoothstep(y - y.floor());
    let top = lerp(
        lattice_noise(x0, y0, seed),
        lattice_noise(x0 + 1, y0, seed),
        tx,
    );
    let bottom = lerp(
        lattice_noise(x0, y0 + 1, seed),
        lattice_noise(x0 + 1, y0 + 1, seed),
        tx,
    );
    lerp(top, bottom, ty)
}

fn lattice_noise(x: i64, y: i64, seed: u64) -> f64 {
    let mut value = seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
}

fn smoothstep(value: f64) -> f64 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(left: f64, right: f64, amount: f64) -> f64 {
    left + (right - left) * amount
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

fn grid_seed(x: i64, y: i64, level: u8, kind: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in x
        .to_le_bytes()
        .into_iter()
        .chain(y.to_le_bytes())
        .chain([level])
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
    smooth_boundary: bool,
) {
    let projected: Vec<Vec<_>> = polygon
        .rings
        .iter()
        .map(|ring| {
            let projected = ring
                .iter()
                .map(|point| project(point.0[0], point.0[1], map_bounds))
                .collect::<Vec<_>>();
            if smooth_boundary {
                organic_closed_ring(&projected)
            } else {
                projected
            }
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

/// Gently warp source-cell vertices, then repeatedly corner-cut the closed
/// contour. This makes wetlands read as natural regions instead of rounded
/// raster cells without changing the exact terrain geometry used by routing.
fn organic_closed_ring(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut vertices = points;
    if points.len() > 1 && points.first() == points.last() {
        vertices = &points[..points.len() - 1];
    }
    if vertices.len() < 3 {
        return points.to_vec();
    }

    let mut contour = Vec::with_capacity(vertices.len());
    for index in 0..vertices.len() {
        let previous = vertices[(index + vertices.len() - 1) % vertices.len()];
        let current = vertices[index];
        let next = vertices[(index + 1) % vertices.len()];
        let chord = (next.0 - previous.0, next.1 - previous.1);
        let chord_length = chord.0.hypot(chord.1);
        let local_scale = (current.0 - previous.0)
            .hypot(current.1 - previous.1)
            .min((next.0 - current.0).hypot(next.1 - current.1));
        let noise = organic_vertex_noise(current);
        let displacement = (noise - 0.5) * local_scale * 0.22;
        if chord_length > f64::EPSILON {
            contour.push((
                current.0 - chord.1 / chord_length * displacement,
                current.1 + chord.0 / chord_length * displacement,
            ));
        } else {
            contour.push(current);
        }
    }

    for _ in 0..3 {
        let mut smoothed = Vec::with_capacity(contour.len() * 2);
        for index in 0..contour.len() {
            let current = contour[index];
            let next = contour[(index + 1) % contour.len()];
            smoothed.push((
                current.0 * 0.75 + next.0 * 0.25,
                current.1 * 0.75 + next.1 * 0.25,
            ));
            smoothed.push((
                current.0 * 0.25 + next.0 * 0.75,
                current.1 * 0.25 + next.1 * 0.75,
            ));
        }
        contour = smoothed;
    }
    contour.push(contour[0]);
    contour
}

fn organic_vertex_noise(point: (f64, f64)) -> f64 {
    let x = (point.0 * 16.0).round() as i64 as u64;
    let y = (point.1 * 16.0).round() as i64 as u64;
    let mut value = x.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ y.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value >> 11) as f64 / ((1_u64 << 53) - 1) as f64
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
            routing_roads: Vec::new(),
            water: Vec::new(),
            wetlands: Vec::new(),
            cultivated: Vec::new(),
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
                        density: 60,
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

    fn flat_fixture() -> Package {
        let mut package = paper_fixture("broadleaf", false);
        package.elevation.cells.clear();
        package
    }

    fn continuous_forest_fixture() -> Package {
        let mut package = flat_fixture();
        package.forest.regions = (1..=6)
            .flat_map(|row| {
                (1..=6).map(move |column| ForestRegion {
                    bounds: [
                        f64::from(column),
                        HEIGHT - f64::from(row + 1),
                        f64::from(column + 1),
                        HEIGHT - f64::from(row),
                    ],
                    density: 60,
                    kind: "mixed".into(),
                })
            })
            .collect();
        package
    }

    fn pixel(tile: &Pixmap, x: usize, y: usize) -> [u8; 4] {
        let offset = (y * tile.width() as usize + x) * 4;
        tile.data()[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn forest_overlay_is_deterministic_and_leaf_type_neutral() {
        let conifer = paper_fixture("conifer", true);
        let first = render(&conifer, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        let second = render(&conifer, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        assert_eq!(first.data(), second.data());

        let broadleaf = paper_fixture("broadleaf", true);
        let broadleaf = render(&broadleaf, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        assert_eq!(first.data(), broadleaf.data());
    }

    #[test]
    fn forest_overlay_starts_at_twenty_percent_canopy() {
        let mut below = paper_fixture("mixed", true);
        below.forest.regions[0].density = 10;
        let below = render(&below, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        let plain = render(&flat_fixture(), 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        assert_eq!(below.data(), plain.data());

        let mut forest = paper_fixture("mixed", true);
        forest.forest.regions[0].density = 20;
        let forest = render(&forest, 64, TILE_GUTTER, 32.0, 0, 0, PAPER).unwrap();
        assert_ne!(forest.data(), plain.data());
    }

    #[test]
    fn parchment_texture_is_deterministic_and_breaks_up_flat_land() {
        let package = flat_fixture();
        let first = render(&package, 128, TILE_GUTTER, 64.0, 0, 0, PAPER).unwrap();
        let second = render(&package, 128, TILE_GUTTER, 64.0, 0, 0, PAPER).unwrap();
        assert_eq!(first.data(), second.data());
        let textured = first
            .data()
            .chunks_exact(4)
            .filter(|pixel| *pixel != PAPER.land)
            .count();
        assert!(textured > 10, "parchment texture did not render");
    }

    #[test]
    fn close_forest_overlay_forms_a_substantial_organic_mass() {
        let mut forest = paper_fixture("mixed", true);
        forest.elevation.cells.clear();
        let plain = flat_fixture();
        let forest = render(&forest, 256, TILE_GUTTER, 64.0, 0, 0, PAPER).unwrap();
        let plain = render(&plain, 256, TILE_GUTTER, 64.0, 0, 0, PAPER).unwrap();
        let changed = forest
            .data()
            .chunks_exact(4)
            .zip(plain.data().chunks_exact(4))
            .filter(|(forest, plain)| forest != plain)
            .count();
        assert!(
            changed > 600,
            "forest overlay is still too sparse: {changed}"
        );
    }

    #[test]
    fn continuous_forest_does_not_reveal_cell_center_holes() {
        let forest = render(
            &continuous_forest_fixture(),
            512,
            TILE_GUTTER,
            64.0,
            0,
            0,
            PAPER,
        )
        .unwrap();
        let plain = render(&flat_fixture(), 512, TILE_GUTTER, 64.0, 0, 0, PAPER).unwrap();
        for logical_y in 2..6 {
            for logical_x in 2..6 {
                let pixel_x = logical_x * 64 + usize::from(TILE_GUTTER);
                let pixel_y = logical_y * 64 + usize::from(TILE_GUTTER);
                assert_ne!(
                    pixel(&forest, pixel_x, pixel_y),
                    pixel(&plain, pixel_x, pixel_y),
                    "forest field left a periodic hole at ({logical_x}, {logical_y})"
                );
            }
        }
    }

    #[test]
    fn canopy_parent_is_the_area_average_of_its_children() {
        let pyramid =
            CanopyPyramid::from_base([0.0, 0.0, 0.002, 0.002], 2, 2, vec![u8::MAX, 0, 0, 0]);
        assert_eq!(pyramid.levels.len(), 2);
        assert_eq!(pyramid.levels[1].coverage, vec![64]);
        let parent = pyramid.coverage_at(0.001, 0.001, 0.002, 0.002);
        assert!((parent - 64.0 / 255.0).abs() < f64::EPSILON);
    }

    #[test]
    fn canopy_finest_lod_preserves_source_cells() {
        let pyramid =
            CanopyPyramid::from_base([0.0, 0.0, 0.002, 0.002], 2, 2, vec![u8::MAX, 0, 0, 0]);
        assert_eq!(pyramid.coverage_at(0.0015, 0.0005, 0.001, 0.001), 1.0);
        assert_eq!(pyramid.coverage_at(0.0015, 0.0015, 0.001, 0.001), 0.0);
    }

    #[test]
    fn wetland_boundary_smoothing_is_closed_organic_and_deterministic() {
        let square = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)];
        let smoothed = organic_closed_ring(&square);
        assert_eq!(smoothed.first(), smoothed.last());
        assert_eq!(smoothed.len(), 33);
        assert!(!smoothed.contains(&(0.0, 0.0)));
        assert_eq!(smoothed, organic_closed_ring(&square));
        assert_ne!(
            organic_vertex_noise((0.0, 0.0)),
            organic_vertex_noise((4.0, 0.0))
        );
    }

    #[test]
    fn road_hierarchy_filters_and_weights_minor_routes() {
        assert!(road_style(4, 8.0, "land", PAPER).is_none());
        let major = road_style(0, 64.0, "land", PAPER).unwrap();
        let minor = road_style(4, 64.0, "land", PAPER).unwrap();
        assert!(major.1 > minor.1);
        assert!(major.0[3] > minor.0[3]);
        let inferred = road_style(4, 64.0, "inferred", PAPER).unwrap();
        assert_ne!(inferred.0, minor.0);
        assert!(inferred.1 < minor.1);
    }

    #[test]
    fn wetland_map_fill_is_distinct_from_plain_and_water() {
        let mut package = flat_fixture();
        package.wetlands = vec![super::super::WaterPolygon {
            rings: vec![vec![
                Point([10.0, 750.0]),
                Point([50.0, 750.0]),
                Point([50.0, 790.0]),
                Point([10.0, 790.0]),
                Point([10.0, 750.0]),
            ]],
        }];
        let tile = render(&package, 64, TILE_GUTTER, 1.0, 0, 0, PAPER).unwrap();
        let wet = pixel(&tile, 24, 24);
        assert_ne!(wet, PAPER.land);
        assert_ne!(wet, PAPER.water);
    }

    #[test]
    fn forest_overlay_is_stable_across_tile_gutters() {
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

    #[test]
    fn selected_cultivation_square_changes_the_rendered_map_layer() {
        let plain = flat_fixture();
        let plain_tile = render(&plain, 64, TILE_GUTTER, 1.0, 0, 0, PAPER).unwrap();
        let mut cultivated = plain;
        cultivated.cultivated = vec![super::super::WaterPolygon {
            rings: vec![vec![
                Point([10.0, 750.0]),
                Point([20.0, 750.0]),
                Point([20.0, 760.0]),
                Point([10.0, 760.0]),
                Point([10.0, 750.0]),
            ]],
        }];
        let cultivated_tile = render(&cultivated, 64, TILE_GUTTER, 1.0, 0, 0, PAPER).unwrap();
        assert_ne!(pixel(&plain_tile, 14, 44), pixel(&cultivated_tile, 14, 44));
    }

    #[test]
    fn excessive_tile_pyramid_is_rejected_before_rendering() {
        let result = build(
            &paper_fixture("broadleaf", false),
            None,
            TileConfig {
                tile_size: MIN_TILE_SIZE,
                max_zoom: 8,
            },
        );

        assert!(result.is_err());
        assert!(pyramid_tile_count(MIN_TILE_SIZE, 8) > MAX_TILE_COUNT);
    }

    #[test]
    fn maximum_zoom_grid_is_sparse_but_every_tile_has_a_parent_fallback() {
        let zoom = 7;
        let span = 512.0 / f64::from(1_u32 << zoom);
        let columns = (WIDTH / span).ceil() as u16;
        let rows = (HEIGHT / span).ceil() as u16;
        let coordinates = tile_grid(columns, rows)
            .into_iter()
            .filter(|&(x, y)| {
                tile_intersects_geographic_detail(
                    [-11.0, 43.0, 31.0, 70.0],
                    span,
                    u32::from(x),
                    u32::from(y),
                )
            })
            .collect::<Vec<_>>();
        assert!(!coordinates.is_empty());
        assert!(coordinates.len() < usize::from(columns) * usize::from(rows));
        assert!(coordinates.iter().all(|&(x, y)| {
            let parent_x = x / 2;
            let parent_y = y / 2;
            parent_x < columns.div_ceil(2) && parent_y < rows.div_ceil(2)
        }));
    }

    #[test]
    fn open_hill_tint_is_visibly_brown_against_the_parchment() {
        let mut tinted = PAPER.land;
        blend_opaque_pixel(&mut tinted, PAPER.hilly_open, 1.0);
        assert!(PAPER.land[0].abs_diff(tinted[0]) >= 25);
        assert!(PAPER.land[1].abs_diff(tinted[1]) >= 35);
        assert!(PAPER.land[2].abs_diff(tinted[2]) >= 50);
        assert!(tinted[0] > tinted[1] && tinted[1] > tinted[2]);
    }
}
