//! HYDE 3.5 historical land-use sampling.
//!
//! HYDE supplies 5-arcminute land-use *areas*.  The supplied 1544 world year
//! falls between its 1500 and 1600 snapshots, so we linearly interpolate the
//! source areas and divide them by HYDE's matching cell-area grid.

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    BASIS_POINTS_PER_WHOLE, LandUseFraction, LandUseProfile, SourceProvenance,
};
use netcdf_reader::{NcFile, NcFormat, NcSliceInfo, NcSliceInfoElem, NcType};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::{
    Error, Result,
    draft::{
        ElevatedSettlementDraft, LandUseEvidence, LandUseSettlementDraft, WorldDraft,
        push_source_note,
    },
};

const HYDE_COLUMNS: usize = 4_320;
const HYDE_ROWS: usize = 2_160;
const HYDE_CELL_SIZE: f64 = 1.0 / 12.0;
const MAX_GRID_CELLS: usize = 10_000_000;
const MAX_NORMALIZABLE_OVERLAP: f64 = 1.05;
const AXIS_EPSILON: f64 = 1e-6;

#[derive(Clone, Copy, Debug)]
pub struct HydeCropCell {
    pub row: i16,
    pub column: i16,
    pub bounds: [f64; 4],
    pub crop_km2: f64,
}

/// Read every HYDE source cell intersecting the playable bounds. This exposes
/// raw interpolated cropland kmÂ² to the map allocator rather than reusing the
/// rounded settlement profile.
pub fn crop_cells(
    directory: &Path,
    year: i32,
    bounds: [f64; 4],
) -> Result<(Vec<HydeCropCell>, String)> {
    let [west, south, east, north] = bounds;
    if !bounds.into_iter().all(f64::is_finite) || west >= east || south >= north {
        return Err(Error::Validation("invalid HYDE crop bounds".into()));
    }
    let mut coordinates = Vec::new();
    let mut identities = Vec::new();
    for row in 0..HYDE_ROWS {
        let latitude = 90.0 - (row as f64 + 0.5) * HYDE_CELL_SIZE;
        if latitude + HYDE_CELL_SIZE / 2.0 <= south || latitude - HYDE_CELL_SIZE / 2.0 >= north {
            continue;
        }
        for column in 0..HYDE_COLUMNS {
            let longitude = -180.0 + (column as f64 + 0.5) * HYDE_CELL_SIZE;
            if longitude + HYDE_CELL_SIZE / 2.0 <= west || longitude - HYDE_CELL_SIZE / 2.0 >= east
            {
                continue;
            }
            coordinates.push((latitude, longitude));
            identities.push((row as i16, column as i16));
        }
    }
    if coordinates.is_empty() || coordinates.len() > MAX_GRID_CELLS {
        return Err(Error::Validation(
            "HYDE crop selection is empty or unbounded".into(),
        ));
    }
    let grid = HydeGrid::open(directory, year, &coordinates)?;
    let cells = coordinates
        .into_iter()
        .zip(identities)
        .zip(grid.values)
        .map(|(((latitude, longitude), (row, column)), values)| {
            let crop_start = values.crop_start.ok_or_else(|| {
                Error::Validation("HYDE crop cell has missing start value".into())
            })?;
            let crop_end = values
                .crop_end
                .ok_or_else(|| Error::Validation("HYDE crop cell has missing end value".into()))?;
            let crop_km2 = interpolate(crop_start, crop_end, grid.interpolation);
            if !crop_km2.is_finite() || crop_km2 < 0.0 {
                return Err(Error::Validation("HYDE crop area is invalid".into()));
            }
            Ok(HydeCropCell {
                row,
                column,
                bounds: [
                    longitude - HYDE_CELL_SIZE / 2.0,
                    latitude - HYDE_CELL_SIZE / 2.0,
                    longitude + HYDE_CELL_SIZE / 2.0,
                    latitude + HYDE_CELL_SIZE / 2.0,
                ],
                crop_km2,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut hasher = Sha256::new();
    hasher.update(b"hyde-3.5-c9-cropland-1544-grid-v1");
    for name in ["cropland.nc", "general_files.zip"] {
        let path = require(directory, name)?;
        let mut reader = BufReader::new(File::open(path)?);
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
    }
    Ok((cells, format!("{:x}", hasher.finalize())))
}

const COMPONENTS: [HydeComponent; 3] = [
    HydeComponent::new("cropland.nc", "cropland"),
    HydeComponent::new("grazing_land.nc", "grazing_land"),
    HydeComponent::new("urban_area.nc", "urban_area"),
];

#[derive(Clone, Copy)]
struct HydeComponent {
    filename: &'static str,
    variable: &'static str,
}

impl HydeComponent {
    const fn new(filename: &'static str, variable: &'static str) -> Self {
        Self { filename, variable }
    }
}

pub(crate) fn enrich(
    mut draft: WorldDraft<ElevatedSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<LandUseSettlementDraft>> {
    let coordinates: Vec<_> = draft
        .settlements
        .iter()
        .map(|settlement| {
            (
                settlement.settlement.latitude,
                settlement.settlement.longitude,
            )
        })
        .collect();
    let grid = HydeGrid::open(directory, draft.year, &coordinates)?;
    let mut fallbacks = 0;
    let mut normalized = 0;
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(grid.values)
        .map(|(mut elevated, values)| {
            let (land_use, evidence, note) = match values.profile(grid.interpolation)? {
                Some((profile, was_normalized)) => {
                    normalized += usize::from(was_normalized);
                    (
                        profile,
                        LandUseEvidence::Hyde35Sampled { normalized: was_normalized },
                        if was_normalized {
                            "**[HYDE 3.5](https://landuse.sites.uu.nl/hyde-project/):** 5-arcminute land-use areas were linearly interpolated to this world year and normalized after a small source-area overlap."
                        } else {
                            "**[HYDE 3.5](https://landuse.sites.uu.nl/hyde-project/):** 5-arcminute land-use areas were linearly interpolated to this world year. This is a regional reconstruction, not an exact settlement observation."
                        },
                    )
                }
                None => {
                    fallbacks += 1;
                    (
                        fallback_profile(&elevated),
                        LandUseEvidence::DeterministicFallback,
                        "**HYDE 3.5 land-use fallback:** The source cell had no usable land-use profile, so cropland and grazing are deterministically seeded by the Viabundus node, built-up land by settlement population level, and the remainder is natural land.",
                    )
                }
            };
            push_source_note(&mut elevated, note);
            Ok(LandUseSettlementDraft { elevated, land_use, evidence })
        })
        .collect::<Result<Vec<_>>>()?;

    draft.sources.push(source_provenance());
    draft.report.land_use_rasters_read = COMPONENTS.len() + 1;
    draft.report.land_use_samples = settlements.len();
    draft.report.land_use_fallback_samples = fallbacks;
    draft.report.land_use_normalized_samples = normalized;
    Ok(WorldDraft {
        year: draft.year,
        spatial_grid: draft.spatial_grid,
        sources: draft.sources,
        road_types: draft.road_types,
        nodes: draft.nodes,
        edges: draft.edges,
        settlement_aliases: draft.settlement_aliases,
        settlement_descriptions: draft.settlement_descriptions,
        settlements,
        report: draft.report,
    })
}

fn source_provenance() -> SourceProvenance {
    crate::manifest::hyde35()
}

#[derive(Clone, Copy, Default)]
struct LandUseSourceValues {
    cell_area: Option<f64>,
    crop_start: Option<f64>,
    crop_end: Option<f64>,
    grazing_start: Option<f64>,
    grazing_end: Option<f64>,
    urban_start: Option<f64>,
    urban_end: Option<f64>,
}

impl LandUseSourceValues {
    fn profile(self, interpolation: f64) -> Result<Option<(LandUseProfile, bool)>> {
        let Some(area) = self.cell_area.filter(|area| *area > 0.0) else {
            if [
                self.crop_start,
                self.crop_end,
                self.grazing_start,
                self.grazing_end,
                self.urban_start,
                self.urban_end,
            ]
            .iter()
            .any(Option::is_some)
            {
                return Err(Error::Validation(
                    "HYDE 3.5 cell-area and land-use nodata do not agree".into(),
                ));
            }
            return Ok(None);
        };
        let values = [
            self.crop_start,
            self.crop_end,
            self.grazing_start,
            self.grazing_end,
            self.urban_start,
            self.urban_end,
        ];
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let [
            Some(crop_start),
            Some(crop_end),
            Some(grazing_start),
            Some(grazing_end),
            Some(urban_start),
            Some(urban_end),
        ] = values
        else {
            return Err(Error::Validation(
                "HYDE 3.5 source cell has partial nodata across area components or time bounds"
                    .into(),
            ));
        };
        profile_from_fractions(
            interpolate(crop_start, crop_end, interpolation) / area,
            interpolate(grazing_start, grazing_end, interpolation) / area,
            interpolate(urban_start, urban_end, interpolation) / area,
        )
        .map(Some)
    }
}

fn interpolate(start: f64, end: f64, amount: f64) -> f64 {
    start + (end - start) * amount
}

fn profile_from_fractions(
    cropland: f64,
    grazing: f64,
    built_up: f64,
) -> Result<(LandUseProfile, bool)> {
    let mut fractions = [cropland, grazing, built_up];
    if !fractions
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
    {
        return Err(Error::Validation(
            "HYDE 3.5 derived land-use fractions are negative or non-finite".into(),
        ));
    }
    let total = fractions.iter().sum::<f64>();
    if !total.is_finite() || total > MAX_NORMALIZABLE_OVERLAP {
        return Err(Error::Validation(format!(
            "HYDE 3.5 land-use areas exceed cell area by more than {:.0}%",
            (MAX_NORMALIZABLE_OVERLAP - 1.0) * 100.0
        )));
    }
    let normalized = total > 1.0;
    if normalized {
        for fraction in &mut fractions {
            *fraction /= total;
        }
    }
    let mut basis_points =
        fractions.map(|fraction| (fraction * f64::from(BASIS_POINTS_PER_WHOLE)).round() as u16);
    let managed = basis_points
        .iter()
        .map(|value| u32::from(*value))
        .sum::<u32>();
    if managed > u32::from(BASIS_POINTS_PER_WHOLE) {
        let excess = (managed - u32::from(BASIS_POINTS_PER_WHOLE)) as u16;
        let largest = basis_points
            .iter()
            .enumerate()
            .max_by_key(|(_, value)| *value)
            .map(|(index, _)| index)
            .expect("three managed components");
        basis_points[largest] -= excess;
    }
    let natural = BASIS_POINTS_PER_WHOLE - basis_points.iter().sum::<u16>();
    Ok((
        LandUseProfile::new(
            LandUseFraction::new(basis_points[0]).expect("bounded cropland"),
            LandUseFraction::new(basis_points[1]).expect("bounded grazing"),
            LandUseFraction::new(basis_points[2]).expect("bounded urban"),
            LandUseFraction::new(natural).expect("bounded natural"),
        )
        .expect("exhaustive HYDE profile"),
        normalized,
    ))
}

fn fallback_profile(settlement: &ElevatedSettlementDraft) -> LandUseProfile {
    let seed = settlement.settlement.source_node_id;
    let cropland = 1_500 + (seed % 1_501) as u16;
    let grazing = 1_000 + ((seed / 7) % 1_501) as u16;
    let built_up = (settlement.settlement.population_level.max(1) as u16) * 20;
    let natural = BASIS_POINTS_PER_WHOLE - cropland - grazing - built_up;
    LandUseProfile::new(
        LandUseFraction::new(cropland).unwrap(),
        LandUseFraction::new(grazing).unwrap(),
        LandUseFraction::new(built_up).unwrap(),
        LandUseFraction::new(natural).unwrap(),
    )
    .unwrap()
}

struct HydeGrid {
    interpolation: f64,
    values: Vec<LandUseSourceValues>,
}

impl HydeGrid {
    fn open(directory: &Path, year: i32, coordinates: &[(f64, f64)]) -> Result<Self> {
        let first_path = require(directory, COMPONENTS[0].filename)?;
        let reference = Schema::read(&first_path, COMPONENTS[0])?;
        let selection = TimeSelection::for_year(&first_path, &reference.times, year)?;
        let cells = coordinates
            .iter()
            .map(|&(latitude, longitude)| {
                Ok(CellIndex {
                    latitude: nearest_axis_index(&reference.latitudes, latitude).ok_or_else(
                        || {
                            Error::Validation(format!(
                                "latitude {latitude} is outside the HYDE 3.5 grid"
                            ))
                        },
                    )?,
                    longitude: nearest_axis_index(&reference.longitudes, longitude).ok_or_else(
                        || {
                            Error::Validation(format!(
                                "longitude {longitude} is outside the HYDE 3.5 grid"
                            ))
                        },
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut values = sample_cell_areas(directory, &cells)?
            .into_iter()
            .map(|cell_area| LandUseSourceValues {
                cell_area,
                ..Default::default()
            })
            .collect::<Vec<_>>();

        for (component_index, component) in COMPONENTS.into_iter().enumerate() {
            let path = require(directory, component.filename)?;
            let schema = if component_index == 0 {
                reference.clone()
            } else {
                let schema = Schema::read(&path, component)?;
                reference.require_matches(&path, &schema)?;
                schema
            };
            let samples = schema.samples(&path, component, selection, &cells)?;
            for (target, (start, end)) in values.iter_mut().zip(samples) {
                match component_index {
                    0 => {
                        target.crop_start = start;
                        target.crop_end = end;
                    }
                    1 => {
                        target.grazing_start = start;
                        target.grazing_end = end;
                    }
                    2 => {
                        target.urban_start = start;
                        target.urban_end = end;
                    }
                    _ => unreachable!("three HYDE components"),
                }
            }
        }
        Ok(Self {
            interpolation: selection.amount,
            values,
        })
    }
}

#[derive(Clone)]
struct Schema {
    longitudes: Vec<f64>,
    latitudes: Vec<f64>,
    times: Vec<f64>,
    fill: Option<f64>,
}

impl Schema {
    fn read(path: &Path, component: HydeComponent) -> Result<Self> {
        let file = nc(path, NcFile::open(path))?;
        if !matches!(file.format(), NcFormat::Nc4 | NcFormat::Nc4Classic) {
            return Err(invalid(
                path,
                "format",
                format!("{:?}", file.format()),
                "expected a NetCDF-4 HYDE file",
            ));
        }
        let variable = nc(path, file.variable(component.variable))?;
        if !matches!(variable.dtype, NcType::Float | NcType::Double) {
            return Err(invalid(
                path,
                component.variable,
                format!("{:?}", variable.dtype),
                "expected a floating-point HYDE area variable",
            ));
        }
        let dimensions: Vec<_> = variable
            .dimensions
            .iter()
            .map(|dimension| dimension.name.as_str())
            .collect();
        if dimensions != ["time", "lat", "lon"] {
            return Err(invalid(
                path,
                component.variable,
                format!("{dimensions:?}"),
                "expected time, lat, lon dimensions in that order",
            ));
        }
        let longitudes = read_axis(&file, path, "lon")?;
        let latitudes = read_axis(&file, path, "lat")?;
        let times = read_axis(&file, path, "time")?;
        if longitudes.len() != HYDE_COLUMNS || latitudes.len() != HYDE_ROWS {
            return Err(invalid(
                path,
                component.variable,
                format!("{} x {}", longitudes.len(), latitudes.len()),
                "expected the global HYDE 3.5 5-arcminute grid",
            ));
        }
        checked_grid_cells(longitudes.len(), latitudes.len())?;
        if variable.dimensions[0].size != times.len() as u64
            || variable.dimensions[1].size != latitudes.len() as u64
            || variable.dimensions[2].size != longitudes.len() as u64
        {
            return Err(invalid(
                path,
                component.variable,
                format!("{dimensions:?}"),
                "variable dimensions do not match coordinate axes",
            ));
        }
        let time = nc(path, file.variable("time"))?;
        require_hyde_time_axis(path, time, &times)?;
        Ok(Self {
            longitudes,
            latitudes,
            times,
            fill: variable
                .attribute("_FillValue")
                .and_then(|attribute| attribute.value.as_f64()),
        })
    }

    fn require_matches(&self, path: &Path, actual: &Self) -> Result<()> {
        require_same_axis(path, "longitude", &self.longitudes, &actual.longitudes)?;
        require_same_axis(path, "latitude", &self.latitudes, &actual.latitudes)?;
        require_same_axis(path, "time", &self.times, &actual.times)
    }

    fn samples(
        &self,
        path: &Path,
        component: HydeComponent,
        selection: TimeSelection,
        cells: &[CellIndex],
    ) -> Result<Vec<(Option<f64>, Option<f64>)>> {
        let file = nc(path, NcFile::open(path))?;
        let read = |time| {
            let raw = nc(
                path,
                file.read_variable_slice_as_f64(
                    component.variable,
                    &NcSliceInfo {
                        selections: vec![
                            NcSliceInfoElem::Index(time as u64),
                            NcSliceInfoElem::Slice {
                                start: 0,
                                end: self.latitudes.len() as u64,
                                step: 1,
                            },
                            NcSliceInfoElem::Slice {
                                start: 0,
                                end: self.longitudes.len() as u64,
                                step: 1,
                            },
                        ],
                    },
                ),
            )?;
            if raw.shape() != [self.latitudes.len(), self.longitudes.len()] {
                return Err(invalid(
                    path,
                    component.variable,
                    format!("{:?}", raw.shape()),
                    "unexpected HYDE time-slice shape",
                ));
            }
            Ok(raw)
        };
        let lower = read(selection.start)?;
        let upper = (selection.end != selection.start)
            .then(|| read(selection.end))
            .transpose()?;
        cells
            .iter()
            .map(|cell| {
                let start = source_value(
                    path,
                    component,
                    lower[[cell.latitude, cell.longitude]],
                    self.fill,
                )?;
                let end = upper.as_ref().map_or(Ok(start), |upper| {
                    source_value(
                        path,
                        component,
                        upper[[cell.latitude, cell.longitude]],
                        self.fill,
                    )
                })?;
                Ok((start, end))
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
struct CellIndex {
    latitude: usize,
    longitude: usize,
}

#[derive(Clone, Copy)]
struct TimeSelection {
    start: usize,
    end: usize,
    amount: f64,
}

impl TimeSelection {
    fn for_year(path: &Path, times: &[f64], year: i32) -> Result<Self> {
        let years = times
            .iter()
            .map(|time| hyde_calendar_year(path, *time))
            .collect::<Result<Vec<_>>>()?;
        if let Some(index) = years.iter().position(|candidate| *candidate == year) {
            return Ok(Self {
                start: index,
                end: index,
                amount: 0.0,
            });
        }
        let end = years
            .iter()
            .position(|candidate| *candidate > year)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "{} does not cover HYDE year {year}",
                    path.display()
                ))
            })?;
        let start = end.checked_sub(1).ok_or_else(|| {
            Error::Validation(format!(
                "{} does not cover HYDE year {year}",
                path.display()
            ))
        })?;
        let first = years[start];
        let last = years[end];
        Ok(Self {
            start,
            end,
            amount: f64::from(year - first) / f64::from(last - first),
        })
    }
}

fn require_hyde_time_axis(
    path: &Path,
    time: &netcdf_reader::NcVariable,
    values: &[f64],
) -> Result<()> {
    const HYDE_CALENDAR: &str = "365_day";

    let units = time
        .attribute("units")
        .and_then(|attribute| attribute.value.as_string());
    let calendar = time
        .attribute("calendar")
        .and_then(|attribute| attribute.value.as_string());
    if units.as_deref() != Some("days since 1-5-1 00:00:00")
        || calendar.as_deref() != Some(HYDE_CALENDAR)
    {
        return Err(invalid(
            path,
            "time",
            format!("units={units:?}, calendar={calendar:?}"),
            "expected HYDE 3.5's 365-day time axis",
        ));
    }
    for value in values {
        hyde_calendar_year(path, *value)?;
    }
    Ok(())
}

fn hyde_calendar_year(path: &Path, days: f64) -> Result<i32> {
    const HYDE_DAYS_PER_YEAR: f64 = 365.0;

    let years = days / HYDE_DAYS_PER_YEAR;
    let rounded = years.round();
    if !days.is_finite() || (years - rounded).abs() > AXIS_EPSILON {
        return Err(invalid(
            path,
            "time",
            days.to_string(),
            "expected a whole HYDE 365-day year",
        ));
    }
    i32::try_from(rounded as i64 + 1).map_err(|_| {
        invalid(
            path,
            "time",
            days.to_string(),
            "HYDE calendar year is out of range",
        )
    })
}

fn source_value(
    path: &Path,
    component: HydeComponent,
    value: f64,
    fill: Option<f64>,
) -> Result<Option<f64>> {
    if value.is_nan() || fill.is_some_and(|marker| value == marker) {
        return Ok(None);
    }
    if !value.is_finite() || value < 0.0 {
        return Err(invalid(
            path,
            component.variable,
            value.to_string(),
            "expected a non-negative finite HYDE area or nodata",
        ));
    }
    Ok(Some(value))
}

fn sample_cell_areas(directory: &Path, cells: &[CellIndex]) -> Result<Vec<Option<f64>>> {
    let path = require(directory, "general_files.zip")?;
    let mut archive = ZipArchive::new(File::open(&path)?).map_err(|error| {
        Error::Validation(format!(
            "{} is not a readable HYDE general-files archive: {error}",
            path.display()
        ))
    })?;
    let entry = archive
        .by_name("general_files/garea_cr.asc")
        .map_err(|error| {
            Error::Validation(format!(
                "{} does not contain general_files/garea_cr.asc: {error}",
                path.display()
            ))
        })?;
    AsciiGrid::sample_hyde(&path, BufReader::new(entry), cells)
}

struct AsciiGrid;

impl AsciiGrid {
    fn sample_hyde(
        path: &Path,
        mut reader: impl BufRead,
        cells: &[CellIndex],
    ) -> Result<Vec<Option<f64>>> {
        let header = GridHeader::parse(path, &mut reader)?;
        header.require_hyde(path)?;
        let mut requested: HashMap<usize, Vec<usize>> = HashMap::new();
        for (target, cell) in cells.iter().enumerate() {
            requested
                .entry(cell.latitude * header.columns + cell.longitude)
                .or_default()
                .push(target);
        }
        let mut samples = vec![None; cells.len()];
        let mut index = 0usize;
        for line in reader.lines() {
            for token in line?.split_whitespace() {
                if index >= header.cell_count()? {
                    return Err(Error::Validation(format!(
                        "{} has too many raster cells",
                        path.display()
                    )));
                }
                let value = token.parse::<f64>().map_err(|error| Error::InvalidField {
                    path: path.into(),
                    field: "cell area",
                    value: token.into(),
                    message: error.to_string(),
                })?;
                let value = if value == header.nodata {
                    None
                } else if !value.is_finite() || value < 0.0 {
                    return Err(invalid(
                        path,
                        "cell area",
                        value.to_string(),
                        "expected a non-negative finite HYDE cell area or nodata",
                    ));
                } else {
                    Some(value)
                };
                if let Some(targets) = requested.get(&index) {
                    for target in targets {
                        samples[*target] = value;
                    }
                }
                index += 1;
            }
        }
        if index != header.cell_count()? {
            return Err(Error::Validation(format!(
                "{} contains {index} raster cells, expected {}",
                path.display(),
                header.cell_count()?
            )));
        }
        Ok(samples)
    }
}

struct GridHeader {
    columns: usize,
    rows: usize,
    west: f64,
    south: f64,
    cell_size: f64,
    nodata: f64,
}

impl GridHeader {
    fn parse(path: &Path, reader: &mut impl BufRead) -> Result<Self> {
        let mut fields = HashMap::new();
        for _ in 0..6 {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Err(Error::Validation(format!(
                    "{} has an incomplete ESRI ASCII header",
                    path.display()
                )));
            }
            let mut parts = line.split_whitespace();
            let key = parts.next().unwrap_or_default().to_ascii_lowercase();
            let value = parts.next().ok_or_else(|| {
                Error::Validation(format!(
                    "{} has a malformed ESRI ASCII header",
                    path.display()
                ))
            })?;
            if parts.next().is_some() || fields.insert(key, value.to_string()).is_some() {
                return Err(Error::Validation(format!(
                    "{} has a malformed or duplicate ESRI ASCII header",
                    path.display()
                )));
            }
        }
        let integer = |name: &'static str| {
            fields
                .get(name)
                .ok_or_else(|| Error::Validation(format!("{} is missing {name}", path.display())))?
                .parse::<usize>()
                .map_err(|error| {
                    invalid(path, name, fields[name].clone(), error.to_string().as_str())
                })
        };
        let number = |name: &'static str| {
            fields
                .get(name)
                .ok_or_else(|| Error::Validation(format!("{} is missing {name}", path.display())))?
                .parse::<f64>()
                .map_err(|error| {
                    invalid(path, name, fields[name].clone(), error.to_string().as_str())
                })
        };
        Ok(Self {
            columns: integer("ncols")?,
            rows: integer("nrows")?,
            west: number("xllcorner")?,
            south: number("yllcorner")?,
            cell_size: number("cellsize")?,
            nodata: number("nodata_value")?,
        })
    }

    fn cell_count(&self) -> Result<usize> {
        self.columns.checked_mul(self.rows).ok_or_else(|| {
            Error::Validation("ESRI ASCII grid dimensions overflow the address space".into())
        })
    }

    fn require_hyde(&self, path: &Path) -> Result<()> {
        if self.columns != HYDE_COLUMNS
            || self.rows != HYDE_ROWS
            || (self.west + 180.0).abs() > AXIS_EPSILON
            || (self.south + 90.0).abs() > AXIS_EPSILON
            || (self.cell_size - HYDE_CELL_SIZE).abs() > AXIS_EPSILON
            || !self.nodata.is_finite()
        {
            return Err(Error::Validation(format!(
                "{} is not a global 5-arcminute HYDE 3.5 area grid",
                path.display()
            )));
        }
        Ok(())
    }
}

fn read_axis(file: &NcFile, path: &Path, name: &'static str) -> Result<Vec<f64>> {
    let variable = nc(path, file.variable(name))?;
    if variable.dimensions.len() != 1
        || variable.dimensions[0].name != name
        || !matches!(variable.dtype, NcType::Float | NcType::Double)
    {
        return Err(invalid(
            path,
            name,
            format!("{:?}", variable.dtype),
            "expected a one-dimensional floating-point coordinate variable",
        ));
    }
    let values: Vec<_> = nc(path, file.read_variable_as_f64(name))?
        .iter()
        .copied()
        .collect();
    if values.len() < 2
        || values.iter().any(|value| !value.is_finite())
        || !strictly_monotonic(&values)
    {
        return Err(invalid(
            path,
            name,
            format!("{} values", values.len()),
            "expected finite, strictly monotonic coordinates",
        ));
    }
    Ok(values)
}

fn require_same_axis(
    path: &Path,
    label: &'static str,
    expected: &[f64],
    actual: &[f64],
) -> Result<()> {
    if expected.len() != actual.len()
        || expected
            .iter()
            .zip(actual)
            .any(|(left, right)| (left - right).abs() > AXIS_EPSILON)
    {
        return Err(invalid(
            path,
            label,
            format!("{} values", actual.len()),
            "coordinate axis differs from the other HYDE component files",
        ));
    }
    Ok(())
}

fn checked_grid_cells(longitudes: usize, latitudes: usize) -> Result<()> {
    let cells = longitudes.checked_mul(latitudes).ok_or_else(|| {
        Error::Validation("HYDE grid dimensions overflow the address space".into())
    })?;
    if cells > MAX_GRID_CELLS {
        return Err(Error::Validation(format!(
            "HYDE grid has {cells} cells, exceeding the {MAX_GRID_CELLS}-cell safety limit"
        )));
    }
    Ok(())
}

fn strictly_monotonic(values: &[f64]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
        || values.windows(2).all(|pair| pair[0] > pair[1])
}

fn nearest_axis_index(axis: &[f64], value: f64) -> Option<usize> {
    value.is_finite().then_some(())?;
    let index = axis
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| (*left - value).abs().total_cmp(&(*right - value).abs()))?
        .0;
    let half_step = match index {
        0 => (axis[1] - axis[0]).abs() / 2.0,
        index if index + 1 == axis.len() => (axis[index] - axis[index - 1]).abs() / 2.0,
        index => {
            (axis[index] - axis[index - 1])
                .abs()
                .min((axis[index + 1] - axis[index]).abs())
                / 2.0
        }
    };
    ((axis[index] - value).abs() <= half_step + AXIS_EPSILON).then_some(index)
}

fn require(directory: &Path, filename: &str) -> Result<PathBuf> {
    let path = directory.join(filename);
    path.is_file()
        .then_some(path.clone())
        .ok_or(Error::MissingSource(path))
}

fn nc<T>(path: &Path, result: netcdf_reader::Result<T>) -> Result<T> {
    result.map_err(|source| Error::Netcdf {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

fn invalid(path: &Path, field: &'static str, value: String, message: &str) -> Error {
    Error::InvalidField {
        path: path.to_path_buf(),
        field,
        value,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{LandUseSourceValues, TimeSelection, hyde_calendar_year, profile_from_fractions};

    #[test]
    fn hyde_time_axis_interpolates_1544_between_1500_and_1600() {
        let selection = TimeSelection::for_year(
            std::path::Path::new("hyde.nc"),
            &[547_135.0, 583_635.0],
            1544,
        )
        .unwrap();
        assert_eq!(selection.start, 0);
        assert_eq!(selection.end, 1);
        assert!((selection.amount - 0.44).abs() < f64::EPSILON);
        assert_eq!(
            hyde_calendar_year(std::path::Path::new("hyde.nc"), 547_135.0).unwrap(),
            1500
        );
    }

    #[test]
    fn source_areas_interpolate_into_an_exhaustive_profile() {
        let values = LandUseSourceValues {
            cell_area: Some(100.0),
            crop_start: Some(10.0),
            crop_end: Some(30.0),
            grazing_start: Some(20.0),
            grazing_end: Some(20.0),
            urban_start: Some(1.0),
            urban_end: Some(1.0),
        };
        let (profile, normalized) = values.profile(0.44).unwrap().unwrap();
        assert!(!normalized);
        assert_eq!(profile.cropland().basis_points(), 1_880);
        assert_eq!(profile.grazing().basis_points(), 2_000);
        assert_eq!(profile.built_up().basis_points(), 100);
        assert_eq!(profile.natural().basis_points(), 6_020);
    }

    #[test]
    fn small_source_overlap_is_normalized() {
        let (profile, normalized) = profile_from_fractions(0.7, 0.3, 0.01).unwrap();
        assert!(normalized);
        assert_eq!(profile.natural().basis_points(), 0);
    }

    #[test]
    fn partial_nodata_fails_closed() {
        let values = LandUseSourceValues {
            cell_area: Some(100.0),
            crop_start: Some(1.0),
            ..Default::default()
        };
        assert!(values.profile(0.44).is_err());
    }
}
