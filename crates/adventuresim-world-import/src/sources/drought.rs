//! NOAA Old World Drought Atlas (OWDA v1.0) summer PDSI sampling.

use std::path::Path;

use adventuresim_world_schema::{
    DroughtHistory, DroughtProfile, PalmerDroughtSeverityIndex, SourceProvenance,
};
use netcdf_reader::{NcFile, NcFormat, NcSliceInfo, NcSliceInfoElem, NcType};

use crate::{
    Error, Result,
    draft::{DroughtSettlementDraft, ReligionSettlementDraft, WorldDraft, push_source_note},
};

const SOURCE_NAME: &str = "NOAA Old World Drought Atlas v1.0";
const SOURCE_URL: &str = "https://www.ncei.noaa.gov/access/paleo-search/study/19419";
const SOURCE_LICENSE: &str = "NOAA/NCEI public-access dataset; citation requested";
const LONGITUDES: usize = 114;
const LATITUDES: usize = 88;
const YEARS: usize = 2_013;
const GRID_STEP: f64 = 0.5;
const FIRST_LONGITUDE: f64 = -11.75;
const FIRST_LATITUDE: f64 = 27.25;
const MAX_NEIGHBOR_DISTANCE_DEGREES: f64 = 1.5;

pub(crate) fn enrich(
    mut draft: WorldDraft<ReligionSettlementDraft>,
    netcdf_path: &Path,
) -> Result<WorldDraft<DroughtSettlementDraft>> {
    let grid = OwdaGrid::open(netcdf_path, draft.year)?;
    if draft.settlements.is_empty() {
        return finish(draft, Vec::new(), grid.valid_cells.len(), 0, 0);
    }
    let mut neighbor_samples = 0;
    let mut fallbacks = 0;
    let profiles = draft
        .settlements
        .iter_mut()
        .map(|religious| {
            let settlement = &religious
                .geologic
                .soil
                .trees
                .vegetated
                .forest
                .land
                .elevated
                .settlement;
            match grid.sample(settlement.latitude, settlement.longitude) {
                Some(sample) => {
                    neighbor_samples += usize::from(sample.used_neighbor);
                    push_source_note(
                        religious,
                        if sample.used_neighbor {
                            "**[NOAA Old World Drought Atlas](https://doi.org/10.25921/rjm6-mq74):** The containing cell lacked a complete reconstruction, so the 1525–1544 profile uses the nearest complete OWDA grid point within 1.5 degrees."
                        } else {
                            "**[NOAA Old World Drought Atlas](https://doi.org/10.25921/rjm6-mq74):** The 1544 summer PDSI and 1525–1544 summary are reconstructed directly from the containing OWDA grid point."
                        },
                    );
                    DroughtProfile::Reconstructed(sample.history)
                }
                None => {
                    fallbacks += 1;
                    push_source_note(
                        religious,
                        "**OWDA drought fallback:** No complete reconstruction was available within 1.5 degrees, so the canonical twenty-year profile is the documented neutral deterministic fallback.",
                    );
                    DroughtProfile::Inferred(neutral_history())
                }
            }
        })
        .collect();
    finish(
        draft,
        profiles,
        grid.valid_cells.len(),
        neighbor_samples,
        fallbacks,
    )
}

fn finish(
    mut draft: WorldDraft<ReligionSettlementDraft>,
    profiles: Vec<DroughtProfile>,
    cells_read: usize,
    neighbor_samples: usize,
    fallbacks: usize,
) -> Result<WorldDraft<DroughtSettlementDraft>> {
    if profiles.len() != draft.settlements.len() {
        return Err(Error::Validation(
            "drought profiles do not match settlements".into(),
        ));
    }
    let settlements: Vec<DroughtSettlementDraft> = std::mem::take(&mut draft.settlements)
        .into_iter()
        .zip(profiles)
        .map(|(religious, drought)| DroughtSettlementDraft { religious, drought })
        .collect();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.drought_grid_cells_read = cells_read;
    draft.report.drought_samples = settlements.len();
    draft.report.drought_neighbor_samples = neighbor_samples;
    draft.report.drought_fallback_samples = fallbacks;
    Ok(WorldDraft {
        year: draft.year,
        sources: draft.sources,
        road_types: draft.road_types,
        nodes: draft.nodes,
        edges: draft.edges,
        settlements,
        report: draft.report,
    })
}

fn neutral_history() -> DroughtHistory {
    let normal = PalmerDroughtSeverityIndex::new(0).unwrap();
    DroughtHistory::new(normal, normal, 0, 0).unwrap()
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    history: DroughtHistory,
    used_neighbor: bool,
}

struct OwdaGrid {
    longitudes: Vec<f64>,
    latitudes: Vec<f64>,
    cells: Vec<Option<DroughtHistory>>,
    valid_cells: Vec<(usize, usize)>,
}

impl OwdaGrid {
    fn open(path: &Path, year: i32) -> Result<Self> {
        if !path.is_file() {
            return Err(Error::MissingSource(path.to_path_buf()));
        }
        let file = nc(path, NcFile::open(path))?;
        if file.format() != NcFormat::Nc4Classic {
            return Err(invalid(
                path,
                "format",
                format!("{:?}", file.format()),
                "expected NetCDF-4 classic model",
            ));
        }
        for (name, size) in [("lon", LONGITUDES), ("lat", LATITUDES), ("time", YEARS)] {
            let dimension = nc(path, file.dimension(name))?;
            if dimension.size != size as u64 || dimension.is_unlimited {
                return Err(invalid(
                    path,
                    "dimensions",
                    format!("{}={}", dimension.name, dimension.size),
                    "unexpected OWDA dimension",
                ));
            }
        }
        let pdsi = nc(path, file.variable("pdsi"))?;
        let dimension_names: Vec<_> = pdsi
            .dimensions
            .iter()
            .map(|dim| dim.name.as_str())
            .collect();
        if dimension_names != ["lon", "lat", "time"] || pdsi.dtype != NcType::Double {
            return Err(invalid(
                path,
                "pdsi",
                format!("{:?} {dimension_names:?}", pdsi.dtype),
                "expected f64 PDSI with dimensions (lon, lat, time)",
            ));
        }
        require_attribute(path, pdsi, "longname", "Palmer Drought Severity Index")?;
        require_attribute(path, pdsi, "units", "unitless")?;

        let longitudes = read_axis(&file, path, "lon", LONGITUDES, FIRST_LONGITUDE)?;
        let latitudes = read_axis(&file, path, "lat", LATITUDES, FIRST_LATITUDE)?;
        let times = read_axis(&file, path, "time", YEARS, 0.0)?;
        let year_index = times
            .iter()
            .position(|value| *value == f64::from(year))
            .ok_or_else(|| Error::Validation(format!("OWDA does not contain year {year}")))?;
        let window = usize::from(DroughtHistory::WINDOW_YEARS);
        if year_index + 1 < window {
            return Err(Error::Validation(format!(
                "OWDA year {year} lacks a complete {window}-year history"
            )));
        }
        let start = year_index + 1 - window;
        let selection = NcSliceInfo {
            selections: vec![
                NcSliceInfoElem::Slice {
                    start: 0,
                    end: LONGITUDES as u64,
                    step: 1,
                },
                NcSliceInfoElem::Slice {
                    start: 0,
                    end: LATITUDES as u64,
                    step: 1,
                },
                NcSliceInfoElem::Slice {
                    start: start as u64,
                    end: (year_index + 1) as u64,
                    step: 1,
                },
            ],
        };
        let values = nc(path, file.read_variable_slice_as_f64("pdsi", &selection))?;
        if values.shape() != [LONGITUDES, LATITUDES, window] {
            return Err(invalid(
                path,
                "pdsi",
                format!("{:?}", values.shape()),
                "unexpected OWDA slice shape",
            ));
        }

        let mut cells = Vec::with_capacity(LONGITUDES * LATITUDES);
        let mut valid_cells = Vec::new();
        for longitude in 0..LONGITUDES {
            for latitude in 0..LATITUDES {
                let series: Vec<_> = (0..window)
                    .map(|offset| values[[longitude, latitude, offset]])
                    .collect();
                let finite = series.iter().filter(|value| value.is_finite()).count();
                if finite == 0 && series.iter().all(|value| value.is_nan()) {
                    cells.push(None);
                    continue;
                }
                if finite != window {
                    return Err(invalid(
                        path,
                        "pdsi",
                        format!(
                            "cell ({longitude}, {latitude}) has {finite}/{window} finite values"
                        ),
                        "each grid cell must have a complete history or only NaN missing values",
                    ));
                }
                let parsed: Vec<_> = series
                    .into_iter()
                    .map(|value| parse_pdsi(path, value))
                    .collect::<Result<_>>()?;
                let history = history(&parsed)?;
                cells.push(Some(history));
                valid_cells.push((longitude, latitude));
            }
        }
        if valid_cells.is_empty() {
            return Err(Error::Validation(format!(
                "{} contains no usable OWDA grid cells",
                path.display()
            )));
        }
        Ok(Self {
            longitudes,
            latitudes,
            cells,
            valid_cells,
        })
    }

    fn sample(&self, latitude: f64, longitude: f64) -> Option<Sample> {
        if !latitude.is_finite() || !longitude.is_finite() {
            return None;
        }
        if let (Some(longitude_index), Some(latitude_index)) = (
            nearest_index(&self.longitudes, longitude),
            nearest_index(&self.latitudes, latitude),
        ) && let Some(history) = self.cell(longitude_index, latitude_index)
        {
            return Some(Sample {
                history,
                used_neighbor: false,
            });
        }
        let longitude_scale = latitude.to_radians().cos();
        let nearest = self.valid_cells.iter().copied().min_by(|left, right| {
            distance_squared(
                self.longitudes[left.0],
                self.latitudes[left.1],
                longitude,
                latitude,
                longitude_scale,
            )
            .total_cmp(&distance_squared(
                self.longitudes[right.0],
                self.latitudes[right.1],
                longitude,
                latitude,
                longitude_scale,
            ))
        })?;
        let distance = distance_squared(
            self.longitudes[nearest.0],
            self.latitudes[nearest.1],
            longitude,
            latitude,
            longitude_scale,
        );
        (distance <= MAX_NEIGHBOR_DISTANCE_DEGREES.powi(2)).then(|| Sample {
            history: self.cell(nearest.0, nearest.1).expect("valid-cell index"),
            used_neighbor: true,
        })
    }

    fn cell(&self, longitude: usize, latitude: usize) -> Option<DroughtHistory> {
        self.cells[longitude * LATITUDES + latitude]
    }
}

fn read_axis(
    file: &NcFile,
    path: &Path,
    name: &'static str,
    length: usize,
    first: f64,
) -> Result<Vec<f64>> {
    let variable = nc(path, file.variable(name))?;
    if variable.dtype != NcType::Double
        || variable.dimensions.len() != 1
        || variable.dimensions[0].name != name
    {
        return Err(invalid(
            path,
            name,
            format!("{:?}", variable.dtype),
            "unexpected coordinate variable",
        ));
    }
    let values = nc(path, file.read_variable_as_f64(name))?;
    let values: Vec<_> = values.iter().copied().collect();
    if values.len() != length
        || values.iter().enumerate().any(|(index, value)| {
            *value != first + index as f64 * if name == "time" { 1.0 } else { GRID_STEP }
        })
    {
        return Err(invalid(
            path,
            name,
            format!("{} values", values.len()),
            "unexpected OWDA coordinate axis",
        ));
    }
    Ok(values)
}

fn require_attribute(
    path: &Path,
    variable: &netcdf_reader::NcVariable,
    name: &'static str,
    expected: &str,
) -> Result<()> {
    let actual = variable
        .attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .and_then(|attribute| attribute.value.as_string());
    if actual.as_deref() != Some(expected) {
        return Err(invalid(
            path,
            name,
            format!("{actual:?}"),
            "unexpected OWDA attribute",
        ));
    }
    Ok(())
}

fn parse_pdsi(path: &Path, value: f64) -> Result<PalmerDroughtSeverityIndex> {
    if !value.is_finite() {
        return Err(invalid(
            path,
            "pdsi",
            value.to_string(),
            "expected a finite reconstructed value",
        ));
    }
    let milli = value * 1_000.0;
    let rounded = milli.round();
    if (milli - rounded).abs() > 1e-6
        || rounded < f64::from(i16::MIN)
        || rounded > f64::from(i16::MAX)
    {
        return Err(invalid(
            path,
            "pdsi",
            value.to_string(),
            "expected a three-decimal bounded value",
        ));
    }
    PalmerDroughtSeverityIndex::new(rounded as i16).ok_or_else(|| {
        invalid(
            path,
            "pdsi",
            value.to_string(),
            "value exceeds canonical PDSI bounds",
        )
    })
}

fn history(series: &[PalmerDroughtSeverityIndex]) -> Result<DroughtHistory> {
    if series.len() != usize::from(DroughtHistory::WINDOW_YEARS) {
        return Err(Error::Validation(
            "OWDA history does not contain twenty summers".into(),
        ));
    }
    let sum: i32 = series
        .iter()
        .map(|value| i32::from(value.milli_units()))
        .sum();
    let mean = (f64::from(sum) / f64::from(DroughtHistory::WINDOW_YEARS)).round() as i16;
    let mean = PalmerDroughtSeverityIndex::new(mean).expect("mean remains within input bounds");
    let drought_summers = series
        .iter()
        .filter(|value| value.milli_units() <= -2_000)
        .count() as u8;
    let wet_summers = series
        .iter()
        .filter(|value| value.milli_units() >= 2_000)
        .count() as u8;
    Ok(
        DroughtHistory::new(*series.last().unwrap(), mean, drought_summers, wet_summers)
            .expect("classified summers are disjoint and bounded by the window"),
    )
}

fn nearest_index(axis: &[f64], value: f64) -> Option<usize> {
    let first = *axis.first()?;
    let last = *axis.last()?;
    if value < first - GRID_STEP / 2.0 || value > last + GRID_STEP / 2.0 {
        return None;
    }
    axis.iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| (*left - value).abs().total_cmp(&(*right - value).abs()))
        .map(|(index, _)| index)
}

fn distance_squared(
    cell_longitude: f64,
    cell_latitude: f64,
    longitude: f64,
    latitude: f64,
    longitude_scale: f64,
) -> f64 {
    ((cell_longitude - longitude) * longitude_scale).powi(2) + (cell_latitude - latitude).powi(2)
}

fn nc<T>(path: &Path, result: netcdf_reader::Result<T>) -> Result<T> {
    result.map_err(|source| Error::Netcdf {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

fn invalid(path: &Path, field: &'static str, value: String, message: &'static str) -> Error {
    Error::InvalidField {
        path: path.to_path_buf(),
        field,
        value,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdsi_history_preserves_current_mean_and_extreme_counts() {
        let path = Path::new("owda.nc");
        let mut series = vec![parse_pdsi(path, 0.0).unwrap(); 20];
        series[0] = parse_pdsi(path, -3.0).unwrap();
        series[1] = parse_pdsi(path, 2.0).unwrap();
        series[19] = parse_pdsi(path, -4.0).unwrap();
        let history = history(&series).unwrap();
        assert_eq!(history.current_summer().milli_units(), -4_000);
        assert_eq!(history.twenty_year_mean().milli_units(), -250);
        assert_eq!(history.drought_summers(), 2);
        assert_eq!(history.wet_summers(), 1);
    }

    #[test]
    fn source_values_must_be_finite_three_decimal_and_bounded() {
        let path = Path::new("owda.nc");
        assert_eq!(parse_pdsi(path, -11.996).unwrap().milli_units(), -11_996);
        assert!(parse_pdsi(path, f64::NAN).is_err());
        assert!(parse_pdsi(path, 1.2345).is_err());
        assert!(parse_pdsi(path, 15.001).is_err());
    }

    #[test]
    fn grid_cell_selection_uses_half_degree_cell_footprints() {
        let axis = [0.25, 0.75, 1.25];
        assert_eq!(nearest_index(&axis, 0.0), Some(0));
        assert_eq!(nearest_index(&axis, 0.5), Some(0));
        assert_eq!(nearest_index(&axis, 1.5), Some(2));
        assert_eq!(nearest_index(&axis, 1.51), None);
    }

    #[test]
    fn just_outside_grid_footprint_can_use_a_nearby_reconstruction() {
        let history = neutral_history();
        let grid = OwdaGrid {
            longitudes: vec![0.25],
            latitudes: vec![0.25],
            cells: vec![Some(history)],
            valid_cells: vec![(0, 0)],
        };
        let sample = grid.sample(0.25, -0.1).unwrap();
        assert!(sample.used_neighbor);
        assert_eq!(sample.history, history);
    }

    #[test]
    fn empty_world_still_requires_the_source() {
        let draft = WorldDraft {
            year: 1544,
            sources: Vec::new(),
            road_types: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            settlements: Vec::new(),
            report: Default::default(),
        };
        let missing = Path::new("missing-owda.nc");
        assert!(
            matches!(enrich(draft, missing), Err(Error::MissingSource(path)) if path == missing)
        );
    }

    #[test]
    #[ignore = "requires the manually downloaded 228 MB NOAA OWDA NetCDF file"]
    fn reads_downloaded_owda_source() {
        let path = std::env::var_os("OWDA_NETCDF").expect("set OWDA_NETCDF");
        let grid = OwdaGrid::open(Path::new(&path), 1544).unwrap();
        assert_eq!(grid.valid_cells.len(), 5_414);
        let sample = grid.sample(53.5, 10.0).unwrap();
        assert_eq!(sample.history.current_summer().milli_units(), -837);
    }

    #[test]
    #[ignore = "requires the manually downloaded OWDA file and Viabundus source directory"]
    fn samples_every_real_viabundus_settlement() {
        let owda = std::env::var_os("OWDA_NETCDF").expect("set OWDA_NETCDF");
        let viabundus = std::env::var_os("VIABUNDUS_DIR").expect("set VIABUNDUS_DIR");
        let grid = OwdaGrid::open(Path::new(&owda), 1544).unwrap();
        let draft = crate::sources::viabundus::compile(Path::new(&viabundus), 1544).unwrap();
        let mut direct = 0;
        let mut neighbors = 0;
        let mut fallbacks = 0;
        for settlement in draft.settlements {
            match grid.sample(settlement.latitude, settlement.longitude) {
                Some(Sample {
                    used_neighbor: true,
                    ..
                }) => neighbors += 1,
                Some(_) => direct += 1,
                None => fallbacks += 1,
            }
        }
        assert_eq!((direct, neighbors, fallbacks), (5_458, 583, 0));
    }
}
