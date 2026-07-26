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
    time::Instant,
};

pub const SCHEMA: u32 = 5;
pub const CHUNK_SIDE: u16 = 256;
pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_PACK_BYTES: usize = 2 * 1024 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const CELL_BYTES: usize = 5;
pub const MAX_DECODED_CHUNK_BYTES: usize = 256 * 256 * CELL_BYTES;
pub const CACHE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ROUTE_NODES: usize = 750_000;
pub const MAX_ROUTE_POINTS: usize = 512;
pub const MAX_TERRAIN_SPANS: usize = 256;

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
    #[error("terrain route planning deadline elapsed")]
    Deadline,
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
    Wetland,
}

impl Surface {
    pub fn speed_metres_per_hour(self) -> u32 {
        match self {
            Self::Road => 5_000,
            Self::Open => 1_250,
            Self::SparseWoods => 1_000,
            Self::DeepWoods => 750,
            Self::Water => 0,
            Self::Wetland => 500,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Cell {
    pub elevation_m: i16,
    pub surface: Surface,
    /// Roads over water are valid bridges/ferries/fords.
    pub crossing: bool,
    /// Whether the canonical EPSG:3035 1 km square is cultivated.
    /// This uses a spare bit in the existing flags byte.
    pub cultivated: bool,
    /// Copernicus TCD canopy cover, retained independently of routing classes.
    pub canopy_percent: u8,
    /// Area share whose native neighbours form slopes steeper than 15 degrees.
    /// Native pack bits decode to 0 or 100; coarser cells retain the average.
    pub hilly_fraction_percent: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainWeights {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub urban: u16,
}

impl TerrainWeights {
    pub const TOTAL: u16 = 1_000;
    pub fn from_cover(canopy_percent: u8, hilly_fraction_percent: u8) -> Self {
        let forest = u16::from(canopy_percent.min(100)) * 10;
        let hills = (u32::from(Self::TOTAL - forest) * u32::from(hilly_fraction_percent.min(100))
            / 100) as u16;
        Self {
            plains: Self::TOTAL - forest - hills,
            forest,
            hills,
            urban: 0,
        }
    }
    pub const fn is_normalized(self) -> bool {
        self.plains <= Self::TOTAL
            && self.forest <= Self::TOTAL
            && self.hills <= Self::TOTAL
            && self.urban <= Self::TOTAL
            && self.plains + self.forest + self.hills + self.urban == Self::TOTAL
    }
    pub fn dot(self, profile: TerrainSkillProfile) -> u16 {
        debug_assert!(self.is_normalized());
        ((u32::from(self.plains) * u32::from(profile.plains)
            + u32::from(self.forest) * u32::from(profile.forest)
            + u32::from(self.hills) * u32::from(profile.hills)
            + u32::from(self.urban) * u32::from(profile.urban))
            / u32::from(Self::TOTAL)) as u16
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSkillProfile {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub urban: u16,
}

impl TerrainSkillProfile {
    pub const MAX: u16 = 5_000;
    pub const fn is_valid(self) -> bool {
        self.plains <= Self::MAX
            && self.forest <= Self::MAX
            && self.hills <= Self::MAX
            && self.urban <= Self::MAX
    }
}

impl Cell {
    pub fn terrain_weights(self) -> TerrainWeights {
        TerrainWeights::from_cover(self.canopy_percent, self.hilly_fraction_percent)
    }
    fn underlying_surface(self) -> Surface {
        match self.canopy_percent {
            45..=u8::MAX => Surface::DeepWoods,
            10..=44 => Surface::SparseWoods,
            _ => Surface::Open,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub purpose: TerrainPurpose,
    pub bounds: [f64; 4],
    pub source_resolution_m: u16,
    pub content_sha256: String,
    pub road_geometry_sha256: String,
    pub wetland_source_sha256: String,
    pub wetland_cells: u64,
    pub cultivation_grid_crs: String,
    pub cultivation_grid_resolution_m: u16,
    pub cultivation_rules_version: u16,
    pub cultivation_source_sha256: String,
    pub cultivated_square_count: u64,
    pub cultivated_native_cells: u64,
    pub entries: Vec<Entry>,
    pub package_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerrainPurpose {
    DocumentedBase,
    Final,
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

    pub const fn purpose(&self) -> TerrainPurpose {
        self.manifest.purpose
    }
    pub fn package_sha256(&self) -> &str {
        &self.manifest.package_sha256
    }
    pub fn road_geometry_sha256(&self) -> &str {
        &self.manifest.road_geometry_sha256
    }
    pub fn wetland_source_sha256(&self) -> &str {
        &self.manifest.wetland_source_sha256
    }
    pub const fn source_resolution_m(&self) -> u16 {
        self.manifest.source_resolution_m
    }
    pub const fn wetland_cells(&self) -> u64 {
        self.manifest.wetland_cells
    }
    pub fn cultivation_source_sha256(&self) -> &str {
        &self.manifest.cultivation_source_sha256
    }
    pub const fn cultivated_square_count(&self) -> u64 {
        self.manifest.cultivated_square_count
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
        let [west_bound, south_bound, east_bound, north_bound] = self.bounds();
        if longitude < west_bound
            || longitude > east_bound
            || latitude < south_bound
            || latitude > north_bound
        {
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
        let expected = entry.width as usize * entry.height as usize * CELL_BYTES;
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
        if decoded
            .chunks_exact(CELL_BYTES)
            .any(|bytes| bytes[2] > 5 || bytes[3] & !0b111 != 0)
        {
            return Err(Error::Validation(
                "chunk contains an unknown surface or flag discriminant".into(),
            ));
        }
        let cells: Arc<[Cell]> = decoded
            .chunks_exact(CELL_BYTES)
            .map(|bytes| Cell {
                elevation_m: i16::from_le_bytes([bytes[0], bytes[1]]),
                surface: match bytes[2] {
                    0 => Surface::Road,
                    1 => Surface::Open,
                    2 => Surface::SparseWoods,
                    3 => Surface::DeepWoods,
                    4 => Surface::Water,
                    5 => Surface::Wetland,
                    _ => unreachable!("surface discriminants were validated"),
                },
                crossing: bytes[3] & 1 != 0,
                hilly_fraction_percent: if bytes[3] & 2 != 0 { 100 } else { 0 },
                cultivated: bytes[3] & 4 != 0,
                canopy_percent: bytes[4],
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
            cache.bytes -= removed.1.len() * CELL_BYTES;
        }
        cache.bytes += decoded.len();
        cache.chunks.insert(index, (clock, cells.clone()));
        Ok(cells)
    }

    /// Plan fastest travel through a bounded geographic window. The window
    /// starts at native 30 m spacing and deterministically coarsens only when
    /// the hard node cap would otherwise be exceeded.
    pub fn plan(&self, start: (f64, f64), goal: (f64, f64)) -> Result<RoutePlan> {
        self.plan_with_profile(start, goal, TerrainSkillProfile::default())
    }

    pub fn plan_with_profile(
        &self,
        start: (f64, f64),
        goal: (f64, f64),
        profile: TerrainSkillProfile,
    ) -> Result<RoutePlan> {
        self.plan_with_deadline(start, goal, profile, None)
    }

    /// Plan with a cooperative deadline. This is intended for request-serving
    /// callers so a timed-out blocking task stops consuming its worker slot.
    pub fn plan_until(
        &self,
        start: (f64, f64),
        goal: (f64, f64),
        deadline: Instant,
    ) -> Result<RoutePlan> {
        self.plan_until_with_profile(start, goal, TerrainSkillProfile::default(), deadline)
    }

    pub fn plan_until_with_profile(
        &self,
        start: (f64, f64),
        goal: (f64, f64),
        profile: TerrainSkillProfile,
        deadline: Instant,
    ) -> Result<RoutePlan> {
        self.plan_with_deadline(start, goal, profile, Some(deadline))
    }

    fn plan_with_deadline(
        &self,
        start: (f64, f64),
        goal: (f64, f64),
        profile: TerrainSkillProfile,
        deadline: Option<Instant>,
    ) -> Result<RoutePlan> {
        if !profile.is_valid() {
            return Err(Error::Validation(
                "terrain skill profile exceeds rank five".into(),
            ));
        }
        if ![start.0, start.1, goal.0, goal.1]
            .into_iter()
            .all(f64::is_finite)
        {
            return Err(Error::NoRoute);
        }
        let [west, south, east, north] = self.bounds();
        let inside = |(latitude, longitude): (f64, f64)| {
            longitude >= west && longitude <= east && latitude >= south && latitude <= north
        };
        if !inside(start) || !inside(goal) {
            return Err(Error::NoRoute);
        }
        for window in expanding_windows(start, goal, self.bounds()) {
            check_deadline(deadline)?;
            match self.plan_window(start, goal, window, profile, deadline) {
                Ok(plan) => return Ok(plan),
                Err(Error::NoRoute) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Error::NoRoute)
    }

    fn plan_window(
        &self,
        start: (f64, f64),
        goal: (f64, f64),
        [west, south, east, north]: [f64; 4],
        profile: TerrainSkillProfile,
        deadline: Option<Instant>,
    ) -> Result<RoutePlan> {
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
            if y % 16 == 0 {
                check_deadline(deadline)?;
            }
            let lat = north - f64::from(y) * step_lat;
            for x in 0..width {
                let lon = west + f64::from(x) * step_lon;
                cells.push(self.sample_coarsened(lat, lon, step_lat, step_lon)?);
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
        let mut plan =
            astar_with_profile_and_deadline(&grid, start_node, goal_node, profile, deadline)?;
        for point in &mut plan.points {
            let x = point.longitude;
            let y = point.latitude;
            point.longitude = west + x * step_lon;
            point.latitude = north - y * step_lat;
        }
        simplify_route(&mut plan.points, MAX_ROUTE_POINTS);
        Ok(plan)
    }

    fn sample_coarsened(
        &self,
        latitude: f64,
        longitude: f64,
        step_lat: f64,
        step_lon: f64,
    ) -> Result<Cell> {
        let native_lat = 30.0 / 111_320.0;
        let samples = (step_lat / native_lat).ceil().clamp(1.0, 9.0) as usize;
        let mut cells = Vec::with_capacity(samples * samples);
        for row in 0..samples {
            for column in 0..samples {
                let x = (column as f64 + 0.5) / samples as f64 - 0.5;
                let y = (row as f64 + 0.5) / samples as f64 - 0.5;
                cells.push(
                    self.cell(latitude - y * step_lat, longitude + x * step_lon)?
                        .unwrap_or(Cell {
                            elevation_m: 0,
                            surface: Surface::Water,
                            crossing: false,
                            cultivated: false,
                            canopy_percent: 0,
                            hilly_fraction_percent: 0,
                        }),
                );
            }
        }
        Ok(aggregate_cells(&cells))
    }
}

fn check_deadline(deadline: Option<Instant>) -> Result<()> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        Err(Error::Deadline)
    } else {
        Ok(())
    }
}

fn expanding_windows(start: (f64, f64), goal: (f64, f64), bounds: [f64; 4]) -> Vec<[f64; 4]> {
    let [pack_west, pack_south, pack_east, pack_north] = bounds;
    let span = (start.1 - goal.1).abs().max((start.0 - goal.0).abs());
    let mut windows = Vec::new();
    for factor in [0.25, 0.5, 1.0, 2.0] {
        let padding = (span * factor).max(0.01);
        let window = [
            (start.1.min(goal.1) - padding).max(pack_west),
            (start.0.min(goal.0) - padding).max(pack_south),
            (start.1.max(goal.1) + padding).min(pack_east),
            (start.0.max(goal.0) + padding).min(pack_north),
        ];
        if windows.last() != Some(&window) {
            windows.push(window);
        }
    }
    if windows.last() != Some(&bounds) {
        windows.push(bounds);
    }
    windows
}

fn aggregate_cells(cells: &[Cell]) -> Cell {
    if let Some(road) = cells.iter().find(|cell| cell.surface == Surface::Road) {
        return Cell {
            elevation_m: road.elevation_m,
            surface: Surface::Road,
            crossing: cells.iter().any(|cell| cell.crossing),
            cultivated: cells.iter().any(|cell| cell.cultivated),
            canopy_percent: cells
                .iter()
                .map(|cell| u64::from(cell.canopy_percent))
                .sum::<u64>()
                .checked_div(cells.len() as u64)
                .unwrap_or_default() as u8,
            hilly_fraction_percent: (cells
                .iter()
                .map(|cell| u64::from(cell.hilly_fraction_percent))
                .sum::<u64>()
                / cells.len() as u64) as u8,
        };
    }
    let passable = cells
        .iter()
        .filter(|cell| passable(**cell))
        .collect::<Vec<_>>();
    if passable.is_empty() {
        return Cell {
            elevation_m: 0,
            surface: Surface::Water,
            crossing: false,
            cultivated: false,
            canopy_percent: 0,
            hilly_fraction_percent: 0,
        };
    }
    let mut counts = [0_usize; 4];
    for cell in &passable {
        match cell.surface {
            Surface::Open => counts[0] += 1,
            Surface::SparseWoods => counts[1] += 1,
            Surface::DeepWoods => counts[2] += 1,
            Surface::Wetland => counts[3] += 1,
            _ => {}
        }
    }
    let surface = [
        Surface::Open,
        Surface::SparseWoods,
        Surface::DeepWoods,
        Surface::Wetland,
    ]
    .into_iter()
    .enumerate()
    .max_by_key(|(index, _)| (counts[*index], *index))
    .map(|(_, surface)| surface)
    .unwrap_or(Surface::Open);
    Cell {
        elevation_m: (passable
            .iter()
            .map(|cell| i64::from(cell.elevation_m))
            .sum::<i64>()
            / passable.len() as i64) as i16,
        surface,
        crossing: passable.iter().any(|cell| cell.crossing),
        cultivated: passable.iter().any(|cell| cell.cultivated),
        canopy_percent: (passable
            .iter()
            .map(|cell| u64::from(cell.canopy_percent))
            .sum::<u64>()
            / passable.len() as u64) as u8,
        hilly_fraction_percent: (passable
            .iter()
            .map(|cell| u64::from(cell.hilly_fraction_percent))
            .sum::<u64>()
            / passable.len() as u64) as u8,
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
    if !valid_digest(&manifest.road_geometry_sha256)
        || !valid_digest(&manifest.wetland_source_sha256)
        || !valid_digest(&manifest.cultivation_source_sha256)
        || manifest.wetland_cells > 100_000
        || manifest.cultivation_grid_crs != "EPSG:3035"
        || manifest.cultivation_grid_resolution_m != 1_000
        || manifest.cultivation_rules_version == 0
        || manifest.cultivated_square_count > 5_000_000
        || manifest.cultivated_native_cells > 1_000_000_000
    {
        return Err(Error::Validation("invalid terrain feature identity".into()));
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
    // Keep the streaming buffer off the small Windows process stack.
    let mut buffer = vec![0_u8; 1024 * 1024];
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
    pub terrain: TerrainWeights,
    pub training_multiplier_permille: u16,
    pub check_millirank: u16,
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
    astar_with_profile(grid, start, goal, TerrainSkillProfile::default())
}

pub fn astar_with_profile<G: RoutingGrid>(
    grid: &G,
    start: (u32, u32),
    goal: (u32, u32),
    profile: TerrainSkillProfile,
) -> Result<RoutePlan> {
    if !profile.is_valid() {
        return Err(Error::Validation(
            "terrain skill profile exceeds rank five".into(),
        ));
    }
    astar_with_profile_and_deadline(grid, start, goal, profile, None)
}

fn astar_with_profile_and_deadline<G: RoutingGrid>(
    grid: &G,
    start: (u32, u32),
    goal: (u32, u32),
    profile: TerrainSkillProfile,
    deadline: Option<Instant>,
) -> Result<RoutePlan> {
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
            return reconstruct(grid, &parent, start_i, goal_i, profile);
        }
        visited += 1;
        if visited % 256 == 0 {
            check_deadline(deadline)?;
        }
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
                if dx != 0
                    && dy != 0
                    && (!grid.cell(next.0, y).is_some_and(passable)
                        || !grid.cell(x, next.1).is_some_and(passable))
                {
                    continue;
                }
                let step = step_seconds(
                    from,
                    to,
                    grid.metres_per_cell(),
                    dx != 0 && dy != 0,
                    profile,
                );
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
    ((u64::from(diagonal) * 1414 + u64::from(straight) * 1000) * u64::from(metres) * 3_600)
        / (1000 * 7500)
}
fn passable(cell: Cell) -> bool {
    cell.surface != Surface::Water || cell.crossing
}
fn step_seconds(
    from: Cell,
    to: Cell,
    metres: u32,
    diagonal: bool,
    profile: TerrainSkillProfile,
) -> u64 {
    let distance = u64::from(metres) * if diagonal { 1414 } else { 1000 } / 1000;
    let base = (distance * 3_600).div_ceil(u64::from(from.surface.speed_metres_per_hour().max(1)));
    let rise = i32::from(to.elevation_m) - i32::from(from.elevation_m);
    let uphill = if rise > 0 {
        1000 + (u64::try_from(rise).unwrap_or(0) * 6).min(1500)
    } else {
        1000
    };
    let check = u64::from(from.terrain_weights().dot(profile));
    // check is milli-rank: 1 + rank/10 => (10_000 + check) / 10_000.
    (base * uphill * 10_000)
        .div_ceil(1000 * (10_000 + check))
        .max(1)
}

fn reconstruct<G: RoutingGrid>(
    grid: &G,
    parent: &HashMap<u32, u32>,
    start: u32,
    goal: u32,
    profile: TerrainSkillProfile,
) -> Result<RoutePlan> {
    let mut nodes = vec![goal];
    let mut current = goal;
    while current != start {
        current = *parent.get(&current).ok_or(Error::NoRoute)?;
        nodes.push(current);
        if nodes.len() > MAX_ROUTE_NODES {
            return Err(Error::NoRoute);
        }
    }
    nodes.reverse();
    let mut distance = 0_u64;
    let mut seconds = 0_u64;
    let mut second_spans: Vec<(Surface, TerrainWeights, u16, u16, u64)> = Vec::new();
    for pair in nodes.windows(2) {
        let a = (pair[0] % grid.width(), pair[0] / grid.width());
        let b = (pair[1] % grid.width(), pair[1] / grid.width());
        let from = grid.cell(a.0, a.1).ok_or(Error::NoRoute)?;
        let to = grid.cell(b.0, b.1).ok_or(Error::NoRoute)?;
        let diagonal = a.0 != b.0 && a.1 != b.1;
        let step_distance =
            u64::from(grid.metres_per_cell()) * if diagonal { 1414 } else { 1000 } / 1000;
        let step = step_seconds(from, to, grid.metres_per_cell(), diagonal, profile);
        distance += step_distance;
        seconds += step;
        let terrain = from.terrain_weights();
        let training_multiplier_permille = if from.surface == Surface::Road {
            (from.underlying_surface().speed_metres_per_hour() / 5) as u16
        } else {
            1_000
        };
        let check_millirank = terrain.dot(profile);
        if let Some((_, _, _, _, duration)) =
            second_spans
                .last_mut()
                .filter(|(surface, weights, multiplier, check, _)| {
                    *surface == from.surface
                        && *weights == terrain
                        && *multiplier == training_multiplier_permille
                        && *check == check_millirank
                })
        {
            *duration += step
        } else {
            second_spans.push((
                from.surface,
                terrain,
                training_multiplier_permille,
                check_millirank,
                step,
            ));
        }
    }
    let mut cursor_seconds = 0_u64;
    let mut cursor_minutes = 0_u64;
    let mut spans: Vec<TerrainSpan> = Vec::new();
    for (surface, terrain, training_multiplier_permille, check_millirank, duration_seconds) in
        second_spans
    {
        cursor_seconds = cursor_seconds.saturating_add(duration_seconds);
        let end_minutes = cursor_seconds.div_ceil(60);
        let duration_minutes = end_minutes.saturating_sub(cursor_minutes);
        if duration_minutes == 0 {
            continue;
        }
        if let Some(last) = spans.last_mut().filter(|span| {
            span.surface == surface
                && span.terrain == terrain
                && span.training_multiplier_permille == training_multiplier_permille
                && span.check_millirank == check_millirank
        }) {
            last.duration_minutes += duration_minutes;
        } else {
            spans.push(TerrainSpan {
                surface,
                terrain,
                training_multiplier_permille,
                check_millirank,
                start_minute: cursor_minutes,
                duration_minutes,
            });
        }
        cursor_minutes = end_minutes;
    }
    let mut points = nodes
        .into_iter()
        .map(|i| RoutePoint {
            longitude: f64::from(i % grid.width()),
            latitude: f64::from(i / grid.width()),
        })
        .collect::<Vec<_>>();
    retain_grid_turns(&mut points);
    simplify_route(&mut points, MAX_ROUTE_POINTS);
    compact_spans(&mut spans, seconds.div_ceil(60), MAX_TERRAIN_SPANS);
    Ok(RoutePlan {
        points,
        spans,
        distance_m: distance,
        minutes: seconds.div_ceil(60),
    })
}

fn compact_spans(spans: &mut Vec<TerrainSpan>, total_minutes: u64, cap: usize) {
    if spans.len() <= cap || cap == 0 || total_minutes == 0 {
        return;
    }
    let source = std::mem::take(spans);
    let mut compacted: Vec<TerrainSpan> = Vec::with_capacity(cap);
    for bucket in 0..cap {
        let start = total_minutes * bucket as u64 / cap as u64;
        let end = total_minutes * (bucket as u64 + 1) / cap as u64;
        if start == end {
            continue;
        }
        let mut durations = [0_u64; 6];
        let mut training_mass = 0_u128;
        let mut check_mass = 0_u128;
        let mut terrain_training_mass = [0_u128; 4];
        for span in &source {
            let span_end = span.start_minute + span.duration_minutes;
            let overlap = end
                .min(span_end)
                .saturating_sub(start.max(span.start_minute));
            durations[surface_index(span.surface)] += overlap;
            let overlap = u128::from(overlap);
            let training = u128::from(span.training_multiplier_permille);
            training_mass += overlap * training;
            check_mass += overlap * u128::from(span.check_millirank);
            for (mass, weight) in terrain_training_mass.iter_mut().zip([
                span.terrain.plains,
                span.terrain.forest,
                span.terrain.hills,
                span.terrain.urban,
            ]) {
                *mass += overlap * training * u128::from(weight);
            }
        }
        let surface = [
            Surface::Road,
            Surface::Open,
            Surface::SparseWoods,
            Surface::DeepWoods,
            Surface::Water,
            Surface::Wetland,
        ]
        .into_iter()
        .enumerate()
        .max_by_key(|(index, _)| (durations[*index], Reverse(*index)))
        .map(|(_, surface)| surface)
        .unwrap_or(Surface::Open);
        let duration = u128::from(end - start);
        let training_multiplier_permille = ((training_mass + duration / 2) / duration) as u16;
        let check_millirank = ((check_mass + duration / 2) / duration) as u16;
        let weights = normalized_weighted_parts(terrain_training_mass, training_mass);
        let terrain = TerrainWeights {
            plains: weights[0],
            forest: weights[1],
            hills: weights[2],
            urban: weights[3],
        };
        if let Some(previous) = compacted.last_mut().filter(|previous| {
            previous.surface == surface
                && previous.terrain == terrain
                && previous.training_multiplier_permille == training_multiplier_permille
                && previous.check_millirank == check_millirank
        }) {
            previous.duration_minutes += end - start;
        } else {
            compacted.push(TerrainSpan {
                surface,
                terrain,
                training_multiplier_permille,
                check_millirank,
                start_minute: start,
                duration_minutes: end - start,
            });
        }
    }
    *spans = compacted;
}

fn normalized_weighted_parts(numerators: [u128; 4], denominator: u128) -> [u16; 4] {
    if denominator == 0 {
        return [TerrainWeights::TOTAL, 0, 0, 0];
    }
    let mut result = [0_u16; 4];
    let mut remainders = [0_u128; 4];
    for index in 0..4 {
        result[index] = (numerators[index] / denominator) as u16;
        remainders[index] = numerators[index] % denominator;
    }
    let mut missing = TerrainWeights::TOTAL - result.iter().sum::<u16>();
    let mut awarded = [false; 4];
    while missing > 0 {
        let index = (0..4)
            .filter(|index| !awarded[*index])
            .max_by_key(|index| (remainders[*index], Reverse(*index)))
            .expect("normalization deficit is at most one unit per terrain");
        result[index] += 1;
        awarded[index] = true;
        missing -= 1;
    }
    result
}

fn surface_index(surface: Surface) -> usize {
    match surface {
        Surface::Road => 0,
        Surface::Open => 1,
        Surface::SparseWoods => 2,
        Surface::DeepWoods => 3,
        Surface::Water => 4,
        Surface::Wetland => 5,
    }
}

fn retain_grid_turns(points: &mut Vec<RoutePoint>) {
    if points.len() <= 2 {
        return;
    }
    let mut retained = Vec::with_capacity(points.len());
    retained.push(points[0].clone());
    for window in points.windows(3) {
        let before = (
            (window[1].longitude - window[0].longitude).signum() as i8,
            (window[1].latitude - window[0].latitude).signum() as i8,
        );
        let after = (
            (window[2].longitude - window[1].longitude).signum() as i8,
            (window[2].latitude - window[1].latitude).signum() as i8,
        );
        if before != after {
            retained.push(window[1].clone());
        }
    }
    retained.push(points.last().expect("route is nonempty").clone());
    *points = retained;
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
    fn surface_speeds_are_applied_without_per_cell_minute_rounding() {
        let expected = [
            (Surface::Road, 2),
            (Surface::Open, 5),
            (Surface::SparseWoods, 6),
            (Surface::DeepWoods, 8),
        ];
        for (surface, minutes) in expected {
            let mut g = grid(2, 1);
            g.cells[0].surface = surface;
            assert_eq!(astar(&g, (0, 0), (1, 0)).unwrap().minutes, minutes);
        }
    }

    #[test]
    fn first_span_describes_the_surface_being_departed() {
        let mut g = grid(3, 1);
        g.cells[0].surface = Surface::Road;
        let route = astar(&g, (0, 0), (2, 0)).unwrap();
        assert_eq!(route.spans[0].surface, Surface::Road);
        assert_eq!(route.spans[0].start_minute, 0);
    }

    #[test]
    fn coarsening_preserves_a_road_cell() {
        let mut cells = vec![Cell::default(); 25];
        cells[12] = Cell {
            surface: Surface::Road,
            elevation_m: 17,
            crossing: true,
            ..Cell::default()
        };
        assert_eq!(
            aggregate_cells(&cells),
            Cell {
                surface: Surface::Road,
                elevation_m: 17,
                crossing: true,
                ..Cell::default()
            }
        );
    }

    #[test]
    fn terrain_weights_are_normalized_monotonic_and_keep_mixed_hills() {
        let open_hills = TerrainWeights::from_cover(0, 100);
        let mixed = TerrainWeights::from_cover(40, 50);
        let dense = TerrainWeights::from_cover(80, 50);
        assert!(open_hills.is_normalized() && mixed.is_normalized() && dense.is_normalized());
        assert_eq!(
            open_hills,
            TerrainWeights {
                plains: 0,
                forest: 0,
                hills: 1_000,
                urban: 0
            }
        );
        assert_eq!(
            mixed,
            TerrainWeights {
                plains: 300,
                forest: 400,
                hills: 300,
                urban: 0
            }
        );
        assert!(dense.forest > mixed.forest && dense.hills < mixed.hills);
    }

    #[test]
    fn coarse_hill_fraction_is_area_average_not_any() {
        let cells = [
            Cell {
                hilly_fraction_percent: 100,
                ..Cell::default()
            },
            Cell::default(),
            Cell::default(),
            Cell::default(),
        ];
        assert_eq!(aggregate_cells(&cells).hilly_fraction_percent, 25);
    }

    #[test]
    fn roads_train_underlying_terrain_at_exact_inverse_speed_discount() {
        for (canopy, expected) in [(0, 250), (20, 200), (60, 150)] {
            let mut g = grid(2, 1);
            g.cells[0].surface = Surface::Road;
            g.cells[0].canopy_percent = canopy;
            let span = astar(&g, (0, 0), (1, 0)).unwrap().spans[0].clone();
            assert_eq!(span.training_multiplier_permille, expected);
            assert_eq!(span.terrain, TerrainWeights::from_cover(canopy, 0));
        }
    }

    #[test]
    fn matching_skill_shortens_route_and_is_stored_on_span() {
        let mut g = grid(2, 1);
        g.cells[0].canopy_percent = 100;
        let novice = astar(&g, (0, 0), (1, 0)).unwrap();
        let expert = astar_with_profile(
            &g,
            (0, 0),
            (1, 0),
            TerrainSkillProfile {
                forest: 5_000,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(expert.minutes < novice.minutes);
        assert_eq!(expert.spans[0].check_millirank, 5_000);
    }

    #[test]
    fn skill_profile_can_change_the_fastest_path() {
        let mut g = grid(7, 3);
        for x in 0..7 {
            g.cells[(7 + x) as usize].surface = Surface::DeepWoods;
            g.cells[(7 + x) as usize].canopy_percent = 100;
            g.cells[(14 + x) as usize].surface = Surface::Water;
        }
        let novice = astar(&g, (0, 1), (6, 1)).unwrap();
        let expert = astar_with_profile(
            &g,
            (0, 1),
            (6, 1),
            TerrainSkillProfile {
                forest: 5_000,
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(novice.points, expert.points);
        assert!(expert.spans.iter().all(|span| span.terrain.forest == 1_000));
    }

    #[test]
    fn expanding_search_finishes_with_full_pack_bounds() {
        let bounds = [5.0, 50.0, 15.0, 55.0];
        let windows = expanding_windows((53.5, 9.8), (53.7, 10.2), bounds);
        assert!(windows.len() > 1);
        assert_eq!(windows.last(), Some(&bounds));
        assert_ne!(windows.first(), windows.last());
    }

    #[test]
    fn terrain_spans_are_deterministically_bounded() {
        let mut g = grid(700, 1);
        for (index, cell) in g.cells.iter_mut().enumerate() {
            cell.surface = if index % 2 == 0 {
                Surface::Road
            } else {
                Surface::DeepWoods
            };
        }
        let first = astar(&g, (0, 0), (699, 0)).unwrap();
        let second = astar(&g, (0, 0), (699, 0)).unwrap();
        assert_eq!(first, second);
        assert!(first.spans.len() <= MAX_TERRAIN_SPANS);
        assert_eq!(
            first
                .spans
                .iter()
                .map(|span| span.duration_minutes)
                .sum::<u64>(),
            first.minutes
        );
        assert!(first
            .spans
            .windows(2)
            .all(|pair| pair[0].start_minute + pair[0].duration_minutes
                == pair[1].start_minute));
    }

    #[test]
    fn span_compaction_conserves_weighted_skill_exposure_with_road_discounts() {
        let terrains = [
            TerrainWeights {
                plains: 1_000,
                forest: 0,
                hills: 0,
                urban: 0,
            },
            TerrainWeights {
                plains: 0,
                forest: 1_000,
                hills: 0,
                urban: 0,
            },
            TerrainWeights {
                plains: 0,
                forest: 0,
                hills: 1_000,
                urban: 0,
            },
            TerrainWeights {
                plains: 400,
                forest: 300,
                hills: 300,
                urban: 0,
            },
        ];
        let mut spans = (0..300_u64)
            .map(|minute| TerrainSpan {
                surface: if minute % 3 == 0 {
                    Surface::Road
                } else {
                    Surface::Open
                },
                terrain: terrains[minute as usize % terrains.len()],
                training_multiplier_permille: if minute % 3 == 0 {
                    [150, 200, 250][minute as usize / 3 % 3]
                } else {
                    1_000
                },
                check_millirank: (minute % 6) as u16 * 1_000,
                start_minute: minute,
                duration_minutes: 1,
            })
            .collect::<Vec<_>>();
        let exposure = |values: &[TerrainSpan]| {
            let mut result = [0.0_f64; 4];
            for span in values {
                for (total, weight) in result.iter_mut().zip([
                    span.terrain.plains,
                    span.terrain.forest,
                    span.terrain.hills,
                    span.terrain.urban,
                ]) {
                    *total += span.duration_minutes as f64
                        * f64::from(span.training_multiplier_permille)
                        / 1_000.0
                        * f64::from(weight)
                        / 1_000.0;
                }
            }
            result
        };
        let before = exposure(&spans);
        compact_spans(&mut spans, 300, 8);
        let after = exposure(&spans);
        assert!(spans.len() <= 8);
        assert!(spans.iter().all(|span| span.terrain.is_normalized()));
        for (before, after) in before.into_iter().zip(after) {
            assert!(
                (before - after).abs() <= before.max(1.0) * 0.005,
                "{before} != {after}"
            );
        }
    }

    #[test]
    fn expired_deadline_stops_search_cooperatively() {
        let g = grid(100, 100);
        assert!(matches!(
            astar_with_profile_and_deadline(
                &g,
                (0, 0),
                (99, 99),
                TerrainSkillProfile::default(),
                Some(Instant::now())
            ),
            Err(Error::Deadline)
        ));
    }

    #[test]
    fn long_native_routes_are_compacted_after_search() {
        let g = grid(10_000, 1);
        let route = astar(&g, (0, 0), (9_999, 0)).unwrap();
        assert_eq!(route.distance_m, 999_900);
        assert_eq!(route.points.len(), 2);
        assert_eq!(
            route
                .spans
                .iter()
                .map(|span| span.duration_minutes)
                .sum::<u64>(),
            route.minutes
        );
    }

    #[test]
    fn endpoint_snapping_and_blocked_diagonal_are_deterministic() {
        let mut g = grid(5, 5);
        g.cells[(2 * 5 + 2) as usize].surface = Surface::Water;
        assert_eq!(snap(&g, (2, 2), 1), Some((1, 1)));

        let mut corner = grid(2, 2);
        corner.cells[1].surface = Surface::Water;
        corner.cells[2].surface = Surface::Water;
        assert!(astar(&corner, (0, 0), (1, 1)).is_err());
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

    #[test]
    fn self_consistent_pack_with_reserved_surface_is_rejected_on_decode() {
        use flate2::{Compression, write::DeflateEncoder};
        let root = std::env::temp_dir().join(format!(
            "adventuresim-terrain-surface-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let decoded = [0, 0, 6, 0, 0];
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&decoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut manifest = Manifest {
            schema: SCHEMA,
            purpose: TerrainPurpose::Final,
            bounds: [10.0, 50.0, 11.0, 51.0],
            source_resolution_m: 30,
            content_sha256: hex_sha(&compressed),
            road_geometry_sha256: hex_sha(b"roads"),
            wetland_source_sha256: hex_sha(b"wetlands"),
            wetland_cells: 0,
            cultivation_grid_crs: "EPSG:3035".into(),
            cultivation_grid_resolution_m: 1_000,
            cultivation_rules_version: 1,
            cultivation_source_sha256: hex_sha(b"cultivation"),
            cultivated_square_count: 0,
            cultivated_native_cells: 0,
            entries: vec![Entry {
                south: 50,
                west: 10,
                tile_width: 1_800,
                tile_height: 3_600,
                chunk_x: 0,
                chunk_y: 0,
                width: 1,
                height: 1,
                offset: 0,
                length: compressed.len() as u32,
                decoded_sha256: hex_sha(&decoded),
            }],
            package_sha256: "0".repeat(64),
        };
        manifest.package_sha256 = hex_sha(&serde_json::to_vec(&manifest).unwrap());
        let manifest_path = root.join("terrain.json");
        let pack_path = root.join("terrain.pack");
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        std::fs::write(&pack_path, compressed).unwrap();
        let pack = TerrainPack::load(&manifest_path, &pack_path).unwrap();
        assert!(
            matches!(pack.cell(50.9999,10.0001),Err(Error::Validation(message)) if message.contains("unknown surface"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
