use image::{ExtendedColorType, ImageEncoder, codecs::avif::AvifEncoder};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
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
const NATIVE_DETAIL_BOUNDS: [f64; 4] = [5.0, 50.0, 16.0, 56.0];
const FOREST_CANOPY_THRESHOLD_PERCENT: f64 = 20.0;
const RELIEF_STEP_DEGREES: f64 = 0.01;
const MOUNTAIN_RELIEF_RADIUS_M: f64 = 7_000.0;
const MOUNTAIN_RELIEF_THRESHOLD_M: i32 = 300;
const MOUNTAIN_LOCAL_RELIEF_THRESHOLD_M: i32 = 150;
const MOUNTAIN_STEEP_FRACTION_PERCENT: u8 = 15;
const MIN_MOUNTAIN_COMPONENT_CELLS: usize = 10;
const MOUNTAIN_MARKER_BLOCK: usize = 4;

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

#[derive(Clone, Copy, Debug, Default)]
struct ReliefCell {
    elevation_m: Option<i16>,
    local_relief_m: u16,
    hilly_fraction_percent: u8,
    mountain: bool,
    relief_7km_m: u16,
}

#[derive(Clone, Copy, Debug)]
struct MountainMarker {
    longitude: f64,
    latitude: f64,
    relief_m: u16,
}

#[derive(Debug)]
struct ReliefField {
    west: f64,
    north: f64,
    columns: usize,
    rows: usize,
    cells: Vec<ReliefCell>,
    markers: Vec<MountainMarker>,
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
                let mut elevations = Vec::with_capacity(9);
                let mut hilly = 0_u8;
                let mut samples = 0_u8;
                for offset_y in sample_offsets {
                    for offset_x in sample_offsets {
                        let sample_latitude = latitude - offset_y * RELIEF_STEP_DEGREES;
                        let sample_longitude = longitude + offset_x * RELIEF_STEP_DEGREES;
                        if let Some(cell) = terrain.cell(sample_latitude, sample_longitude)? {
                            elevations.push(cell.elevation_m);
                            hilly += u8::from(cell.hilly);
                            samples += 1;
                        }
                    }
                }
                if elevations.is_empty() {
                    continue;
                }
                elevations.sort_unstable();
                let local_relief =
                    i32::from(*elevations.last().expect("nonempty")) - i32::from(elevations[0]);
                cells[row * columns + column] = ReliefCell {
                    elevation_m: Some(elevations[elevations.len() / 2]),
                    local_relief_m: local_relief.clamp(0, i32::from(u16::MAX)) as u16,
                    hilly_fraction_percent: u8::try_from(
                        u16::from(hilly) * 100 / u16::from(samples),
                    )
                    .unwrap_or(100),
                    ..ReliefCell::default()
                };
            }
        }
        classify_mountains(&mut cells, columns, rows, north);
        let markers = mountain_markers(&cells, columns, rows, west, north);
        Ok(Self {
            west,
            north,
            columns,
            rows,
            cells,
            markers,
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

fn classify_mountains(cells: &mut [ReliefCell], columns: usize, rows: usize, north: f64) {
    for row in 0..rows {
        let latitude = north - (row as f64 + 0.5) * RELIEF_STEP_DEGREES;
        let north_radius =
            (MOUNTAIN_RELIEF_RADIUS_M / (111_320.0 * RELIEF_STEP_DEGREES)).ceil() as isize;
        let east_metres = 111_320.0 * latitude.to_radians().cos() * RELIEF_STEP_DEGREES;
        let east_radius = (MOUNTAIN_RELIEF_RADIUS_M / east_metres.max(1.0)).ceil() as isize;
        for column in 0..columns {
            let index = row * columns + column;
            if cells[index].elevation_m.is_none() {
                continue;
            }
            let mut minimum = i16::MAX;
            let mut maximum = i16::MIN;
            for offset_y in -north_radius..=north_radius {
                let neighbour_row = row as isize + offset_y;
                if neighbour_row < 0 || neighbour_row >= rows as isize {
                    continue;
                }
                for offset_x in -east_radius..=east_radius {
                    let neighbour_column = column as isize + offset_x;
                    if neighbour_column < 0 || neighbour_column >= columns as isize {
                        continue;
                    }
                    let distance = ((offset_y as f64 * 111_320.0 * RELIEF_STEP_DEGREES).powi(2)
                        + (offset_x as f64 * east_metres).powi(2))
                    .sqrt();
                    if distance > MOUNTAIN_RELIEF_RADIUS_M {
                        continue;
                    }
                    if let Some(elevation) = cells
                        [neighbour_row as usize * columns + neighbour_column as usize]
                        .elevation_m
                    {
                        minimum = minimum.min(elevation);
                        maximum = maximum.max(elevation);
                    }
                }
            }
            if minimum == i16::MAX {
                continue;
            }
            let relief = (i32::from(maximum) - i32::from(minimum)).max(0);
            cells[index].relief_7km_m = relief.min(i32::from(u16::MAX)) as u16;
            cells[index].mountain = relief >= MOUNTAIN_RELIEF_THRESHOLD_M
                && (i32::from(cells[index].local_relief_m) >= MOUNTAIN_LOCAL_RELIEF_THRESHOLD_M
                    || cells[index].hilly_fraction_percent >= MOUNTAIN_STEEP_FRACTION_PERCENT);
        }
    }
    remove_small_mountain_components(cells, columns, rows);

    // Fill small interior gaps so a ridge remains a coherent area rather than
    // a stippled collection of independently qualifying kilometre cells.
    let original = cells.iter().map(|cell| cell.mountain).collect::<Vec<_>>();
    for row in 1..rows.saturating_sub(1) {
        for column in 1..columns.saturating_sub(1) {
            let index = row * columns + column;
            if original[index] || cells[index].elevation_m.is_none() {
                continue;
            }
            let neighbours = (-1_isize..=1)
                .flat_map(|dy| (-1_isize..=1).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| *dx != 0 || *dy != 0)
                .filter(|(dx, dy)| {
                    original
                        [(row as isize + dy) as usize * columns + (column as isize + dx) as usize]
                })
                .count();
            if neighbours >= 6 {
                cells[index].mountain = true;
            }
        }
    }
}

fn remove_small_mountain_components(cells: &mut [ReliefCell], columns: usize, rows: usize) {
    let mut visited = vec![false; cells.len()];
    for start in 0..cells.len() {
        if visited[start] || !cells[start].mountain {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut component = Vec::new();
        visited[start] = true;
        while let Some(index) = queue.pop_front() {
            component.push(index);
            let row = index / columns;
            let column = index % columns;
            for offset_y in -1_isize..=1 {
                for offset_x in -1_isize..=1 {
                    if offset_x == 0 && offset_y == 0 {
                        continue;
                    }
                    let next_row = row as isize + offset_y;
                    let next_column = column as isize + offset_x;
                    if next_row < 0
                        || next_column < 0
                        || next_row >= rows as isize
                        || next_column >= columns as isize
                    {
                        continue;
                    }
                    let next = next_row as usize * columns + next_column as usize;
                    if !visited[next] && cells[next].mountain {
                        visited[next] = true;
                        queue.push_back(next);
                    }
                }
            }
        }
        if component.len() < MIN_MOUNTAIN_COMPONENT_CELLS {
            for index in component {
                cells[index].mountain = false;
            }
        }
    }
}

fn mountain_markers(
    cells: &[ReliefCell],
    columns: usize,
    rows: usize,
    west: f64,
    north: f64,
) -> Vec<MountainMarker> {
    let mut markers = Vec::new();
    for block_row in (0..rows).step_by(MOUNTAIN_MARKER_BLOCK) {
        for block_column in (0..columns).step_by(MOUNTAIN_MARKER_BLOCK) {
            let mut mountain_count = 0;
            let mut strongest = None;
            for row in block_row..(block_row + MOUNTAIN_MARKER_BLOCK).min(rows) {
                for column in block_column..(block_column + MOUNTAIN_MARKER_BLOCK).min(columns) {
                    let cell = cells[row * columns + column];
                    if !cell.mountain {
                        continue;
                    }
                    mountain_count += 1;
                    if strongest.is_none_or(|(_, _, relief)| cell.relief_7km_m > relief) {
                        strongest = Some((row, column, cell.relief_7km_m));
                    }
                }
            }
            if mountain_count < 4 {
                continue;
            }
            if let Some((row, column, relief_m)) = strongest {
                markers.push(MountainMarker {
                    longitude: west + (column as f64 + 0.5) * RELIEF_STEP_DEGREES,
                    latitude: north - (row as f64 + 0.5) * RELIEF_STEP_DEGREES,
                    relief_m,
                });
            }
        }
    }
    markers
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
            max_zoom: 7,
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
    road: [u8; 4],
    ferry: [u8; 4],
    forest_sparse: [u8; 4],
    forest_deep: [u8; 4],
    terrain_ink: [u8; 4],
    terrain_hatch: [u8; 4],
    elevation: [[u8; 4]; 7],
}

const PAPER: Palette = Palette {
    land: [230, 225, 203, 255],
    paper_fiber: [111, 91, 66, 10],
    paper_fleck: [151, 125, 86, 7],
    water: [184, 201, 197, 255],
    water_edge: [103, 119, 116, 190],
    road: [92, 79, 57, 230],
    ferry: [91, 88, 78, 180],
    forest_sparse: [105, 139, 91, 120],
    forest_deep: [49, 94, 55, 155],
    terrain_ink: [94, 81, 66, 205],
    terrain_hatch: [112, 96, 77, 155],
    elevation: [
        [221, 213, 186, 255],
        [212, 201, 170, 255],
        [201, 185, 153, 120],
        [186, 166, 134, 255],
        [170, 146, 119, 255],
        [152, 127, 108, 255],
        [130, 107, 96, 255],
    ],
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
        relief_field,
        package.bounds,
        scale,
        origin,
        palette,
    )?;
    if let Some(relief_field) = relief_field {
        draw_mountain_ranges(
            &mut pixmap,
            relief_field,
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
    for road in &package.roads {
        let Some((shade, width)) =
            road_style(road.importance, scale, road.kind == "ferry", palette)
        else {
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

fn road_style(importance: u8, scale: f64, ferry: bool, palette: Palette) -> Option<([u8; 4], f32)> {
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
    if ferry {
        return Some((with_alpha(palette.ferry, 165), 0.95));
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
    for pixel_y in 0..height {
        let logical_y = (origin.1 + pixel_y as f64 + 0.5) / scale;
        for pixel_x in 0..width {
            let logical_x = (origin.0 + pixel_x as f64 + 0.5) / scale;
            let mut forest_samples = 0_u8;
            let mut hilly_samples = 0_u8;
            let mut combined_samples = 0_u8;
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
                    let forest = native.map_or_else(
                        || {
                            forest_field.is_some_and(|field| {
                                field.density_at(sample_x, sample_y)
                                    >= FOREST_CANOPY_THRESHOLD_PERCENT
                            })
                        },
                        |cell| f64::from(cell.canopy_percent) >= FOREST_CANOPY_THRESHOLD_PERCENT,
                    );
                    let hilly = native.map_or_else(
                        || relief_field.is_some_and(|field| field.hilly_at(latitude, longitude)),
                        |cell| cell.hilly,
                    );
                    forest_samples += u8::from(forest && !hilly);
                    hilly_samples += u8::from(hilly && !forest);
                    combined_samples += u8::from(hilly && forest);
                }
            }
            if forest_samples == 0 && hilly_samples == 0 && combined_samples == 0 {
                continue;
            }
            let offset = (pixel_y * width + pixel_x) * 4;
            blend_opaque_pixel(
                &mut data[offset..offset + 4],
                palette.elevation[2],
                f64::from(hilly_samples) * 0.25,
            );
            blend_opaque_pixel(
                &mut data[offset..offset + 4],
                palette.forest_sparse,
                f64::from(forest_samples) * 0.25,
            );
            blend_opaque_pixel(
                &mut data[offset..offset + 4],
                palette.forest_deep,
                f64::from(combined_samples) * 0.25,
            );
        }
    }
    Ok(())
}

fn draw_mountain_ranges(
    pixmap: &mut Pixmap,
    field: &ReliefField,
    map_bounds: [f64; 4],
    scale: f64,
    origin: (f64, f64),
    tile_bounds: [f64; 4],
    palette: Palette,
) {
    if scale < 16.0 {
        return;
    }
    let margin = 24.0 / scale;
    for marker in &field.markers {
        let (x, y) = project(marker.longitude, marker.latitude, map_bounds);
        if !intersects(
            [x - margin, y - margin, x + margin, y + margin],
            tile_bounds,
        ) {
            continue;
        }
        let relief_factor = (f64::from(marker.relief_m) / 600.0).clamp(0.75, 1.45);
        draw_hill_stamp(
            pixmap,
            (x * scale - origin.0) as f32,
            (y * scale - origin.1) as f32,
            (18.0 * relief_factor) as f32,
            4,
            scale,
            palette.elevation[3],
            palette.terrain_ink,
            palette.terrain_hatch,
        );
    }
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
    fn road_hierarchy_filters_and_weights_minor_routes() {
        assert!(road_style(4, 8.0, false, PAPER).is_none());
        let major = road_style(0, 64.0, false, PAPER).unwrap();
        let minor = road_style(4, 64.0, false, PAPER).unwrap();
        assert!(major.1 > minor.1);
        assert!(major.0[3] > minor.0[3]);
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
    fn mountain_classifier_rejects_high_plains() {
        let mut cells = vec![
            ReliefCell {
                elevation_m: Some(1_500),
                ..ReliefCell::default()
            };
            30 * 30
        ];
        classify_mountains(&mut cells, 30, 30, 53.0);
        assert!(cells.iter().all(|cell| !cell.mountain));
    }

    #[test]
    fn mountain_classifier_accepts_sustained_low_elevation_relief() {
        let mut cells = (0..30)
            .flat_map(|_| {
                (0..30).map(|column| ReliefCell {
                    elevation_m: Some((column * 40) as i16),
                    local_relief_m: 180,
                    hilly_fraction_percent: 45,
                    ..ReliefCell::default()
                })
            })
            .collect::<Vec<_>>();
        classify_mountains(&mut cells, 30, 30, 53.0);
        assert!(cells.iter().filter(|cell| cell.mountain).count() > 50);
    }

    #[test]
    fn mountain_classifier_removes_an_isolated_spike() {
        let mut cells = vec![
            ReliefCell {
                elevation_m: Some(0),
                ..ReliefCell::default()
            };
            30 * 30
        ];
        cells[15 * 30 + 15] = ReliefCell {
            elevation_m: Some(500),
            local_relief_m: 500,
            hilly_fraction_percent: 100,
            ..ReliefCell::default()
        };
        classify_mountains(&mut cells, 30, 30, 53.0);
        assert!(cells.iter().all(|cell| !cell.mountain));
    }
}
