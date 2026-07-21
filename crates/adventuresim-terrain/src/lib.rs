//! Bounded native-detail strategic terrain packs and deterministic routing.
//!
//! The pack keeps GLO-30 pixels in compressed 256×256 chunks on disk. Runtime
//! consumers decompress only a bounded LRU and never put the continental grid
//! in SpacetimeDB or in one allocation.

use flate2::read::DeflateDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BinaryHeap, HashMap},
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    ops::Range,
    path::Path,
    sync::{Arc, Mutex},
};

pub const SCHEMA: u32 = 1;
pub const CHUNK_SIDE: u16 = 256;
pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_PACK_BYTES: usize = 2 * 1024 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_DECODED_CHUNK_BYTES: usize = 256 * 256 * 4;
pub const CACHE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ROUTE_NODES: usize = 750_000;
pub const MAX_ROUTE_POINTS: usize = 8_192;

#[cfg(feature = "builder")]
pub mod builder;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("terrain I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("terrain manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[error("terrain validation: {0}")]
    Validation(String),
    #[error("no bounded route was found")]
    NoRoute,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Road,
    #[default]
    Open,
    SparseWoods,
    DeepWoods,
    Water,
}

impl Surface {
    pub fn speed_metres_per_hour(self) -> u32 {
        match self {
            Self::Road => 5_000,
            Self::Open => 1_250,
            Self::SparseWoods => 1_000,
            Self::DeepWoods => 750,
            Self::Water => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Cell {
    pub elevation_m: i16,
    pub surface: Surface,
    /// Roads over water are valid bridges/ferries/fords.
    pub crossing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub bounds: [f64; 4],
    pub source_resolution_m: u16,
    pub content_sha256: String,
    pub entries: Vec<Entry>,
    pub package_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub south: i16,
    pub west: i16,
    pub tile_width: u16,
    pub tile_height: u16,
    pub chunk_x: u16,
    pub chunk_y: u16,
    pub width: u16,
    pub height: u16,
    pub offset: u64,
    pub length: u32,
    pub decoded_sha256: String,
}

struct Cache {
    clock: u64,
    bytes: usize,
    chunks: HashMap<usize, (u64, Arc<[Cell]>)>,
}

pub struct TerrainPack {
    manifest: Manifest,
    file: Mutex<File>,
    ranges: Vec<Range<usize>>,
    index: HashMap<(i16, i16, u16, u16), usize>,
    tiles: HashMap<(i16, i16), (u16, u16)>,
    cache: Mutex<Cache>,
}

impl TerrainPack {
    pub fn load(manifest_path: &Path, pack_path: &Path) -> Result<Self> {
        let manifest_length = fs::metadata(manifest_path)?.len();
        if manifest_length == 0 || manifest_length > MAX_MANIFEST_BYTES {
            return Err(Error::Validation(
                "terrain manifest exceeds its byte bound".into(),
            ));
        }
        let json = fs::read(manifest_path)?;
        let manifest: Manifest = serde_json::from_slice(&json)?;
        validate_manifest(&manifest)?;
        let mut unsigned = manifest.clone();
        unsigned.package_sha256 = "0".repeat(64);
        if hex_sha(&serde_json::to_vec(&unsigned)?) != manifest.package_sha256 {
            return Err(Error::Validation("manifest digest mismatch".into()));
        }
        let pack_length = fs::metadata(pack_path)?.len();
        if pack_length == 0 || pack_length > MAX_PACK_BYTES as u64 {
            return Err(Error::Validation("pack exceeds 2 GiB bound".into()));
        }
        let mut file = File::open(pack_path)?;
        if hex_sha_reader(BufReader::new(&mut file))? != manifest.content_sha256 {
            return Err(Error::Validation("pack digest mismatch".into()));
        }
        file.seek(SeekFrom::Start(0))?;
        let mut ranges = Vec::with_capacity(manifest.entries.len());
        let mut previous_end = 0;
        for entry in &manifest.entries {
            let start = usize::try_from(entry.offset)
                .map_err(|_| Error::Validation("chunk offset overflow".into()))?;
            let end = start
                .checked_add(entry.length as usize)
                .ok_or_else(|| Error::Validation("chunk range overflow".into()))?;
            if start < previous_end || end as u64 > pack_length {
                return Err(Error::Validation(
                    "chunk ranges overlap or exceed pack".into(),
                ));
            }
            previous_end = end;
            ranges.push(start..end);
        }
        let mut index = HashMap::with_capacity(manifest.entries.len());
        let mut tiles = HashMap::new();
        for (position, entry) in manifest.entries.iter().enumerate() {
            if index
                .insert(
                    (entry.south, entry.west, entry.chunk_x, entry.chunk_y),
                    position,
                )
                .is_some()
            {
                return Err(Error::Validation("duplicate terrain chunk".into()));
            }
            if tiles
                .insert(
                    (entry.south, entry.west),
                    (entry.tile_width, entry.tile_height),
                )
                .is_some_and(|dimensions| dimensions != (entry.tile_width, entry.tile_height))
            {
                return Err(Error::Validation(
                    "inconsistent source tile dimensions".into(),
                ));
            }
        }
        Ok(Self {
            manifest,
            file: Mutex::new(file),
            ranges,
            index,
            tiles,
            cache: Mutex::new(Cache {
                clock: 0,
                bytes: 0,
                chunks: HashMap::new(),
            }),
        })
    }

    pub fn digest(&self) -> &str {
        &self.manifest.package_sha256
    }
    pub fn bounds(&self) -> [f64; 4] {
        self.manifest.bounds
    }

    pub fn cell(&self, latitude: f64, longitude: f64) -> Result<Option<Cell>> {
        if !latitude.is_finite() || !longitude.is_finite() {
            return Ok(None);
        }
        let south = latitude.floor() as i16;
        let west = longitude.floor() as i16;
        let Some(&(tile_width, tile_height)) = self.tiles.get(&(south, west)) else {
            return Ok(None);
        };
        let x = ((longitude - f64::from(west)) * f64::from(tile_width))
            .floor()
            .clamp(0.0, f64::from(tile_width - 1)) as u16;
        let y = ((f64::from(south + 1) - latitude) * f64::from(tile_height))
            .floor()
            .clamp(0.0, f64::from(tile_height - 1)) as u16;
        let chunk_x = x / CHUNK_SIDE;
        let chunk_y = y / CHUNK_SIDE;
        let &index = self
            .index
            .get(&(south, west, chunk_x, chunk_y))
            .ok_or_else(|| Error::Validation("manifest has a hole in a source tile".into()))?;
        let entry = &self.manifest.entries[index];
        let cells = self.chunk(index)?;
        let local_x = x - chunk_x * CHUNK_SIDE;
        let local_y = y - chunk_y * CHUNK_SIDE;
        Ok(cells
            .get(local_y as usize * entry.width as usize + local_x as usize)
            .copied())
    }

    fn chunk(&self, index: usize) -> Result<Arc<[Cell]>> {
        {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| Error::Validation("terrain cache poisoned".into()))?;
            cache.clock = cache
                .clock
                .checked_add(1)
                .ok_or_else(|| Error::Validation("cache clock overflow".into()))?;
            let clock = cache.clock;
            if let Some((used, cells)) = cache.chunks.get_mut(&index) {
                *used = clock;
                return Ok(cells.clone());
            }
        }
        let entry = &self.manifest.entries[index];
        let expected = entry.width as usize * entry.height as usize * 4;
        if expected > MAX_DECODED_CHUNK_BYTES {
            return Err(Error::Validation("decoded chunk exceeds bound".into()));
        }
        let range = self.ranges[index].clone();
        let mut compressed = vec![0_u8; range.len()];
        {
            let mut file = self
                .file
                .lock()
                .map_err(|_| Error::Validation("terrain pack file lock poisoned".into()))?;
            file.seek(SeekFrom::Start(range.start as u64))?;
            file.read_exact(&mut compressed)?;
        }
        let mut decoded = Vec::with_capacity(expected);
        DeflateDecoder::new(compressed.as_slice())
            .take((MAX_DECODED_CHUNK_BYTES + 1) as u64)
            .read_to_end(&mut decoded)?;
        if decoded.len() != expected || hex_sha(&decoded) != entry.decoded_sha256 {
            return Err(Error::Validation("chunk is corrupt or truncated".into()));
        }
        let cells: Arc<[Cell]> = decoded
            .chunks_exact(4)
            .map(|bytes| Cell {
                elevation_m: i16::from_le_bytes([bytes[0], bytes[1]]),
                surface: match bytes[2] {
                    0 => Surface::Road,
                    1 => Surface::Open,
                    2 => Surface::SparseWoods,
                    3 => Surface::DeepWoods,
                    _ => Surface::Water,
                },
                crossing: bytes[3] != 0,
            })
            .collect::<Vec<_>>()
            .into();
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| Error::Validation("terrain cache poisoned".into()))?;
        cache.clock = cache
            .clock
            .checked_add(1)
            .ok_or_else(|| Error::Validation("cache clock overflow".into()))?;
        let clock = cache.clock;
        if let Some((used, existing)) = cache.chunks.get_mut(&index) {
            *used = clock;
            return Ok(existing.clone());
        }
        while cache.bytes + decoded.len() > CACHE_BYTES {
            let victim = cache
                .chunks
                .iter()
                .min_by_key(|(key, (used, _))| (*used, **key))
                .map(|(key, _)| *key)
                .ok_or_else(|| Error::Validation("cache cannot admit chunk".into()))?;
            let removed = cache.chunks.remove(&victim).expect("cache victim exists");
            cache.bytes -= removed.1.len() * 4;
        }
        cache.bytes += decoded.len();
        cache.chunks.insert(index, (clock, cells.clone()));
        Ok(cells)
    }

    /// Plan fastest travel through a bounded geographic window. The window
    /// starts at native 30 m spacing and deterministically coarsens only when
    /// the hard node cap would otherwise be exceeded.
    pub fn plan(&self, start: (f64, f64), goal: (f64, f64)) -> Result<RoutePlan> {
        let [pack_west, pack_south, pack_east, pack_north] = self.bounds();
        if ![start.0, start.1, goal.0, goal.1]
            .into_iter()
            .all(f64::is_finite)
        {
            return Err(Error::NoRoute);
        }
        let span_lon = (start.1 - goal.1).abs();
        let span_lat = (start.0 - goal.0).abs();
        let padding = (span_lon.max(span_lat) * 0.25).max(0.01);
        let west = (start.1.min(goal.1) - padding).max(pack_west);
        let east = (start.1.max(goal.1) + padding).min(pack_east);
        let south = (start.0.min(goal.0) - padding).max(pack_south);
        let north = (start.0.max(goal.0) + padding).min(pack_north);
        if west >= east || south >= north {
            return Err(Error::NoRoute);
        }
        let latitude = (south + north) * 0.5;
        let mut step_lat = 30.0 / 111_320.0;
        let mut step_lon = 30.0 / (111_320.0 * latitude.to_radians().cos().abs().max(0.2));
        loop {
            let width = ((east - west) / step_lon).ceil() as usize + 1;
            let height = ((north - south) / step_lat).ceil() as usize + 1;
            if width
                .checked_mul(height)
                .is_some_and(|count| count <= MAX_ROUTE_NODES)
            {
                break;
            }
            step_lat *= 1.25;
            step_lon *= 1.25;
        }
        let width = ((east - west) / step_lon).ceil() as u32 + 1;
        let height = ((north - south) / step_lat).ceil() as u32 + 1;
        let mut cells = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            let lat = north - f64::from(y) * step_lat;
            for x in 0..width {
                let lon = west + f64::from(x) * step_lon;
                cells.push(self.cell(lat, lon)?.unwrap_or(Cell {
                    elevation_m: 0,
                    surface: Surface::Water,
                    crossing: false,
                }));
            }
        }
        let grid = WindowGrid {
            width,
            height,
            cells,
            metres: (step_lat * 111_320.0).round().max(1.0) as u32,
        };
        let coordinate = |value: (f64, f64)| {
            (
                ((value.1 - west) / step_lon)
                    .round()
                    .clamp(0.0, f64::from(width - 1)) as u32,
                ((north - value.0) / step_lat)
                    .round()
                    .clamp(0.0, f64::from(height - 1)) as u32,
            )
        };
        let start_node = snap(&grid, coordinate(start), 8).ok_or(Error::NoRoute)?;
        let goal_node = snap(&grid, coordinate(goal), 8).ok_or(Error::NoRoute)?;
        let mut plan = astar(&grid, start_node, goal_node)?;
        for point in &mut plan.points {
            let x = point.longitude;
            let y = point.latitude;
            point.longitude = west + x * step_lon;
            point.latitude = north - y * step_lat;
        }
        simplify_route(&mut plan.points, 512);
        Ok(plan)
    }
}

struct WindowGrid {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
    metres: u32,
}
impl RoutingGrid for WindowGrid {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn cell(&self, x: u32, y: u32) -> Option<Cell> {
        self.cells.get((y * self.width + x) as usize).copied()
    }
    fn metres_per_cell(&self) -> u32 {
        self.metres
    }
}
fn snap<G: RoutingGrid>(grid: &G, origin: (u32, u32), radius: i32) -> Option<(u32, u32)> {
    for distance in 0..=radius {
        for dy in -distance..=distance {
            for dx in -distance..=distance {
                if dx.abs().max(dy.abs()) != distance {
                    continue;
                }
                let x = origin.0 as i32 + dx;
                let y = origin.1 as i32 + dy;
                if x >= 0
                    && y >= 0
                    && x < grid.width() as i32
                    && y < grid.height() as i32
                    && grid
                        .cell(x as u32, y as u32)
                        .is_some_and(|c| c.surface != Surface::Water || c.crossing)
                {
                    return Some((x as u32, y as u32));
                }
            }
        }
    }
    None
}
fn simplify_route(points: &mut Vec<RoutePoint>, cap: usize) {
    if points.len() <= cap {
        return;
    }
    let stride = points.len().div_ceil(cap - 1);
    let mut simplified = points.iter().step_by(stride).cloned().collect::<Vec<_>>();
    if simplified.last() != points.last() {
        simplified.push(points.last().expect("route is nonempty").clone())
    }
    *points = simplified;
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.schema != SCHEMA
        || manifest.source_resolution_m != 30
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_ENTRIES
    {
        return Err(Error::Validation(
            "unsupported or unbounded manifest".into(),
        ));
    }
    let [west, south, east, north] = manifest.bounds;
    if ![west, south, east, north].into_iter().all(f64::is_finite) || west >= east || south >= north
    {
        return Err(Error::Validation("invalid geographic bounds".into()));
    }
    if !valid_digest(&manifest.content_sha256) || !valid_digest(&manifest.package_sha256) {
        return Err(Error::Validation("invalid digest".into()));
    }
    for entry in &manifest.entries {
        if ![1_800, 2_400, 3_600].contains(&entry.tile_width)
            || entry.tile_height != 3_600
            || entry.width == 0
            || entry.height == 0
            || entry.width > CHUNK_SIDE
            || entry.height > CHUNK_SIDE
            || !valid_digest(&entry.decoded_sha256)
        {
            return Err(Error::Validation(
                "invalid native GLO-30 chunk metadata".into(),
            ));
        }
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hex_sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex_sha_reader(mut reader: impl Read) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutePoint {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSpan {
    pub surface: Surface,
    pub start_minute: u64,
    pub duration_minutes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoutePlan {
    pub points: Vec<RoutePoint>,
    pub spans: Vec<TerrainSpan>,
    pub distance_m: u64,
    pub minutes: u64,
}

/// Small deterministic A* kernel used by the pack planner and exhaustive fixtures.
pub trait RoutingGrid {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn cell(&self, x: u32, y: u32) -> Option<Cell>;
    fn metres_per_cell(&self) -> u32;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Queue {
    f: u64,
    g: u64,
    index: u32,
}
impl Ord for Queue {
    fn cmp(&self, other: &Self) -> Ordering {
        self.f
            .cmp(&other.f)
            .then(self.g.cmp(&other.g))
            .then(self.index.cmp(&other.index))
    }
}
impl PartialOrd for Queue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn astar<G: RoutingGrid>(grid: &G, start: (u32, u32), goal: (u32, u32)) -> Result<RoutePlan> {
    if start.0 >= grid.width()
        || start.1 >= grid.height()
        || goal.0 >= grid.width()
        || goal.1 >= grid.height()
    {
        return Err(Error::NoRoute);
    }
    let index = |p: (u32, u32)| p.1 * grid.width() + p.0;
    let point = |i: u32| (i % grid.width(), i / grid.width());
    let start_i = index(start);
    let goal_i = index(goal);
    let mut open = BinaryHeap::new();
    let mut best = HashMap::new();
    let mut parent = HashMap::new();
    best.insert(start_i, 0_u64);
    open.push(Reverse(Queue {
        f: heuristic(start, goal, grid.metres_per_cell()),
        g: 0,
        index: start_i,
    }));
    let mut visited = 0;
    while let Some(Reverse(current)) = open.pop() {
        if best.get(&current.index) != Some(&current.g) {
            continue;
        }
        if current.index == goal_i {
            return reconstruct(grid, &parent, start_i, goal_i);
        }
        visited += 1;
        if visited > MAX_ROUTE_NODES {
            return Err(Error::NoRoute);
        }
        let (x, y) = point(current.index);
        for dy in -1_i32..=1 {
            for dx in -1_i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 {
                    continue;
                }
                let next = (nx as u32, ny as u32);
                if next.0 >= grid.width() || next.1 >= grid.height() {
                    continue;
                }
                let Some(from) = grid.cell(x, y) else {
                    continue;
                };
                let Some(to) = grid.cell(next.0, next.1) else {
                    continue;
                };
                if to.surface == Surface::Water && !to.crossing {
                    continue;
                }
                let step = step_minutes(from, to, grid.metres_per_cell(), dx != 0 && dy != 0);
                let next_g = current.g.saturating_add(step);
                let next_i = index(next);
                if best.get(&next_i).is_none_or(|known| next_g < *known) {
                    best.insert(next_i, next_g);
                    parent.insert(next_i, current.index);
                    open.push(Reverse(Queue {
                        f: next_g.saturating_add(heuristic(next, goal, grid.metres_per_cell())),
                        g: next_g,
                        index: next_i,
                    }));
                }
            }
        }
    }
    Err(Error::NoRoute)
}

fn heuristic(a: (u32, u32), b: (u32, u32), metres: u32) -> u64 {
    let dx = a.0.abs_diff(b.0);
    let dy = a.1.abs_diff(b.1);
    let diagonal = dx.min(dy);
    let straight = dx.max(dy) - diagonal;
    ((u64::from(diagonal) * 1414 + u64::from(straight) * 1000) * u64::from(metres) * 60)
        / (1000 * 5000)
}
fn step_minutes(from: Cell, to: Cell, metres: u32, diagonal: bool) -> u64 {
    let distance = u64::from(metres) * if diagonal { 1414 } else { 1000 } / 1000;
    let base = (distance * 60).div_ceil(u64::from(to.surface.speed_metres_per_hour().max(1)));
    let rise = i32::from(to.elevation_m) - i32::from(from.elevation_m);
    let uphill = if rise > 0 {
        1000 + (u64::try_from(rise).unwrap_or(0) * 6).min(1500)
    } else {
        1000
    };
    (base * uphill).div_ceil(1000).max(1)
}

fn reconstruct<G: RoutingGrid>(
    grid: &G,
    parent: &HashMap<u32, u32>,
    start: u32,
    goal: u32,
) -> Result<RoutePlan> {
    let mut nodes = vec![goal];
    let mut current = goal;
    while current != start {
        current = *parent.get(&current).ok_or(Error::NoRoute)?;
        nodes.push(current);
        if nodes.len() > MAX_ROUTE_POINTS {
            return Err(Error::NoRoute);
        }
    }
    nodes.reverse();
    let mut distance = 0_u64;
    let mut minutes = 0_u64;
    let mut spans = Vec::new();
    for pair in nodes.windows(2) {
        let a = (pair[0] % grid.width(), pair[0] / grid.width());
        let b = (pair[1] % grid.width(), pair[1] / grid.width());
        let from = grid.cell(a.0, a.1).ok_or(Error::NoRoute)?;
        let to = grid.cell(b.0, b.1).ok_or(Error::NoRoute)?;
        let diagonal = a.0 != b.0 && a.1 != b.1;
        let step_distance =
            u64::from(grid.metres_per_cell()) * if diagonal { 1414 } else { 1000 } / 1000;
        let step = step_minutes(from, to, grid.metres_per_cell(), diagonal);
        distance += step_distance;
        minutes += step;
        if let Some(last) = spans
            .last_mut()
            .filter(|span: &&mut TerrainSpan| span.surface == to.surface)
        {
            last.duration_minutes += step
        } else {
            spans.push(TerrainSpan {
                surface: to.surface,
                start_minute: minutes - step,
                duration_minutes: step,
            })
        }
    }
    Ok(RoutePlan {
        points: nodes
            .into_iter()
            .map(|i| RoutePoint {
                longitude: f64::from(i % grid.width()),
                latitude: f64::from(i / grid.width()),
            })
            .collect(),
        spans,
        distance_m: distance,
        minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    struct Grid {
        w: u32,
        h: u32,
        cells: Vec<Cell>,
    }
    impl RoutingGrid for Grid {
        fn width(&self) -> u32 {
            self.w
        }
        fn height(&self) -> u32 {
            self.h
        }
        fn cell(&self, x: u32, y: u32) -> Option<Cell> {
            self.cells.get((y * self.w + x) as usize).copied()
        }
        fn metres_per_cell(&self) -> u32 {
            100
        }
    }
    fn grid(w: u32, h: u32) -> Grid {
        Grid {
            w,
            h,
            cells: vec![Cell::default(); (w * h) as usize],
        }
    }
    #[test]
    fn road_detour_beats_direct_open() {
        let mut g = grid(7, 3);
        for x in 0..7 {
            g.cells[(2 * 7 + x) as usize].surface = Surface::Road;
        }
        let p = astar(&g, (0, 1), (6, 1)).unwrap();
        assert!(p.spans.iter().any(|s| s.surface == Surface::Road));
    }
    #[test]
    fn woods_and_uphill_are_slower_directionally() {
        let mut g = grid(3, 1);
        g.cells[1].surface = Surface::SparseWoods;
        g.cells[2].surface = Surface::DeepWoods;
        g.cells[2].elevation_m = 100;
        let up = astar(&g, (0, 0), (2, 0)).unwrap();
        let down = astar(&g, (2, 0), (0, 0)).unwrap();
        assert!(up.minutes > down.minutes);
        assert_eq!(up.spans.len(), 2);
    }
    #[test]
    fn water_needs_crossing() {
        let mut g = grid(3, 1);
        g.cells[1].surface = Surface::Water;
        assert!(astar(&g, (0, 0), (2, 0)).is_err());
        g.cells[1].crossing = true;
        assert!(astar(&g, (0, 0), (2, 0)).is_ok());
    }
    #[test]
    fn ties_are_deterministic() {
        let g = grid(4, 4);
        assert_eq!(
            astar(&g, (0, 0), (3, 2)).unwrap(),
            astar(&g, (0, 0), (3, 2)).unwrap()
        );
    }

    #[test]
    fn oversized_manifest_is_rejected_before_reading_or_parsing() {
        let root =
            std::env::temp_dir().join(format!("adventuresim-terrain-bound-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let manifest = root.join("terrain.json");
        let pack = root.join("terrain.pack");
        File::create(&manifest)
            .unwrap()
            .set_len(MAX_MANIFEST_BYTES + 1)
            .unwrap();
        File::create(&pack).unwrap().write_all(b"x").unwrap();
        assert!(matches!(
            TerrainPack::load(&manifest, &pack),
            Err(Error::Validation(message)) if message.contains("manifest exceeds")
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
