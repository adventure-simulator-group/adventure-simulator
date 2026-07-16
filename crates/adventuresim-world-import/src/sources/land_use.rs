//! HYDE 3.2 historical land-use sampling for the 1544 world.

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use adventuresim_world_schema::{
    CompiledWorld, LandUseFraction, LandUseProfile, SettlementImport, SourceProvenance,
    WORLD_SCHEMA_VERSION, WorldMetadata,
};

use crate::{
    Error, Result,
    draft::{ElevatedSettlementDraft, WorldDraft},
};

const SOURCE_NAME: &str = "History Database of the Global Environment 3.2";
const SOURCE_URL: &str = "https://doi.org/10.17026/dans-znk-cfy3";
const SOURCE_LICENSE: &str = "CC0-1.0";
const START_YEAR: i32 = 1500;
const END_YEAR: i32 = 1600;
const HYDE_COLUMNS: usize = 4_320;
const HYDE_ROWS: usize = 2_160;
const HYDE_CELL_SIZE: f64 = 1.0 / 12.0;

const RASTERS: [(&str, &str, LandUseComponent); 7] = [
    ("garea_cr.asc", "cell area", LandUseComponent::CellArea),
    (
        "cropland1500AD.asc",
        "1500 cropland",
        LandUseComponent::CroplandStart,
    ),
    (
        "cropland1600AD.asc",
        "1600 cropland",
        LandUseComponent::CroplandEnd,
    ),
    (
        "grazing1500AD.asc",
        "1500 grazing",
        LandUseComponent::GrazingStart,
    ),
    (
        "grazing1600AD.asc",
        "1600 grazing",
        LandUseComponent::GrazingEnd,
    ),
    (
        "urban1500AD.asc",
        "1500 built-up",
        LandUseComponent::BuiltUpStart,
    ),
    (
        "urban1600AD.asc",
        "1600 built-up",
        LandUseComponent::BuiltUpEnd,
    ),
];

pub(crate) fn enrich(
    mut draft: WorldDraft<ElevatedSettlementDraft>,
    directory: &Path,
) -> Result<CompiledWorld> {
    if !(START_YEAR..=END_YEAR).contains(&draft.year) {
        return Err(Error::Validation(format!(
            "HYDE interpolation supports {START_YEAR}..={END_YEAR}, not {}",
            draft.year
        )));
    }
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
    let mut values = vec![LandUseSourceValues::default(); coordinates.len()];
    for (filename, label, component) in RASTERS {
        let path = require(directory, filename)?;
        let samples = AsciiGrid::sample_hyde(&path, &coordinates)?;
        for (target, sample) in values.iter_mut().zip(samples) {
            target.set(component, sample, label)?;
        }
    }

    let interpolation = f64::from(draft.year - START_YEAR) / f64::from(END_YEAR - START_YEAR);
    let mut fallback_samples = 0;
    let settlements = draft
        .settlements
        .into_iter()
        .zip(values)
        .map(|(elevated, values)| {
            let land_use = match values.profile(interpolation) {
                Some(profile) => profile,
                None => {
                    fallback_samples += 1;
                    fallback_profile(&elevated)
                }
            };
            let settlement = elevated.settlement;
            SettlementImport {
                id: settlement.id,
                source_node_id: settlement.source_node_id,
                name: settlement.name,
                longitude: settlement.longitude,
                latitude: settlement.latitude,
                population_level: settlement.population_level,
                population_estimate: settlement.population_estimate,
                elevation: elevated.elevation,
                land_use,
                scene_key: settlement.scene_key,
                religion_id: settlement.religion_id,
            }
        })
        .collect::<Vec<_>>();
    draft.sources.push(SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        license: SOURCE_LICENSE.into(),
    });
    draft.report.land_use_rasters_read = RASTERS.len();
    draft.report.land_use_samples = settlements.len();
    draft.report.land_use_fallback_samples = fallback_samples;
    Ok(CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            world_year: draft.year,
            sources: draft.sources,
            road_types: draft.road_types,
        },
        nodes: draft.nodes,
        edges: draft.edges,
        settlements,
        report: draft.report,
    })
}

#[derive(Clone, Copy)]
enum LandUseComponent {
    CellArea,
    CroplandStart,
    CroplandEnd,
    GrazingStart,
    GrazingEnd,
    BuiltUpStart,
    BuiltUpEnd,
}

#[derive(Clone, Copy, Default)]
struct LandUseSourceValues {
    cell_area: Option<f64>,
    cropland_start: Option<f64>,
    cropland_end: Option<f64>,
    grazing_start: Option<f64>,
    grazing_end: Option<f64>,
    built_up_start: Option<f64>,
    built_up_end: Option<f64>,
}

impl LandUseSourceValues {
    fn set(
        &mut self,
        component: LandUseComponent,
        value: Option<f64>,
        label: &'static str,
    ) -> Result<()> {
        if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(Error::Validation(format!(
                "HYDE {label} sample is negative or non-finite"
            )));
        }
        match component {
            LandUseComponent::CellArea => self.cell_area = value,
            LandUseComponent::CroplandStart => self.cropland_start = value,
            LandUseComponent::CroplandEnd => self.cropland_end = value,
            LandUseComponent::GrazingStart => self.grazing_start = value,
            LandUseComponent::GrazingEnd => self.grazing_end = value,
            LandUseComponent::BuiltUpStart => self.built_up_start = value,
            LandUseComponent::BuiltUpEnd => self.built_up_end = value,
        }
        Ok(())
    }

    fn profile(self, interpolation: f64) -> Option<LandUseProfile> {
        let area = self.cell_area.filter(|area| *area > 0.0)?;
        let crop = interpolate(self.cropland_start?, self.cropland_end?, interpolation) / area;
        let grazing = interpolate(self.grazing_start?, self.grazing_end?, interpolation) / area;
        let built = interpolate(self.built_up_start?, self.built_up_end?, interpolation) / area;
        Some(profile_from_fractions(crop, grazing, built))
    }
}

fn interpolate(start: f64, end: f64, amount: f64) -> f64 {
    start + (end - start) * amount
}

fn profile_from_fractions(cropland: f64, grazing: f64, built_up: f64) -> LandUseProfile {
    let mut fractions = [cropland.max(0.0), grazing.max(0.0), built_up.max(0.0)];
    let total = fractions.iter().sum::<f64>();
    if total > 1.0 {
        for fraction in &mut fractions {
            *fraction /= total;
        }
    }
    let mut basis_points = fractions.map(|fraction| (fraction * 10_000.0).round() as u16);
    let managed = basis_points
        .iter()
        .map(|value| u32::from(*value))
        .sum::<u32>();
    if managed > 10_000 {
        let excess = (managed - 10_000) as u16;
        let largest = basis_points
            .iter()
            .enumerate()
            .max_by_key(|(_, value)| *value)
            .map(|(index, _)| index)
            .unwrap();
        basis_points[largest] -= excess;
    }
    let natural = 10_000 - basis_points.iter().sum::<u16>();
    LandUseProfile::new(
        LandUseFraction::new(basis_points[0]).unwrap(),
        LandUseFraction::new(basis_points[1]).unwrap(),
        LandUseFraction::new(basis_points[2]).unwrap(),
        LandUseFraction::new(natural).unwrap(),
    )
    .unwrap()
}

fn fallback_profile(settlement: &ElevatedSettlementDraft) -> LandUseProfile {
    let seed = settlement.settlement.source_node_id;
    let cropland = 1_500 + (seed % 1_501) as u16;
    let grazing = 1_000 + ((seed / 7) % 1_501) as u16;
    let built_up = (settlement.settlement.population_level.max(1) as u16) * 20;
    let natural = 10_000 - cropland - grazing - built_up;
    LandUseProfile::new(
        LandUseFraction::new(cropland).unwrap(),
        LandUseFraction::new(grazing).unwrap(),
        LandUseFraction::new(built_up).unwrap(),
        LandUseFraction::new(natural).unwrap(),
    )
    .unwrap()
}

fn require(directory: &Path, filename: &str) -> Result<PathBuf> {
    let path = directory.join(filename);
    path.is_file()
        .then_some(path.clone())
        .ok_or(Error::MissingSource(path))
}

struct AsciiGrid;

impl AsciiGrid {
    fn sample_hyde(path: &Path, coordinates: &[(f64, f64)]) -> Result<Vec<Option<f64>>> {
        Self::sample(path, coordinates, true)
    }

    fn sample(
        path: &Path,
        coordinates: &[(f64, f64)],
        require_hyde_grid: bool,
    ) -> Result<Vec<Option<f64>>> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let header = GridHeader::parse(path, &mut reader)?;
        if require_hyde_grid {
            header.require_hyde(path)?;
        }
        let mut requested: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, &(latitude, longitude)) in coordinates.iter().enumerate() {
            let cell = header.cell(path, latitude, longitude)?;
            requested.entry(cell).or_default().push(index);
        }
        let mut samples = vec![None; coordinates.len()];
        let mut token_index = 0usize;
        for line in reader.lines() {
            let line = line?;
            for token in line.split_whitespace() {
                if token_index >= header.cell_count()? {
                    return Err(Error::Validation(format!(
                        "{} has too many raster cells",
                        path.display()
                    )));
                }
                let parsed = token.parse::<f64>().map_err(|error| Error::InvalidField {
                    path: path.into(),
                    field: "raster cell",
                    value: token.into(),
                    message: error.to_string(),
                })?;
                let value = if parsed == header.nodata {
                    None
                } else if !parsed.is_finite() || parsed < 0.0 {
                    return Err(Error::InvalidField {
                        path: path.into(),
                        field: "raster cell",
                        value: token.into(),
                        message: "HYDE cells must be non-negative finite areas or nodata".into(),
                    });
                } else {
                    Some(parsed)
                };
                if let Some(targets) = requested.get(&token_index) {
                    for &target in targets {
                        samples[target] = value;
                    }
                }
                token_index += 1;
            }
        }
        if token_index != header.cell_count()? {
            return Err(Error::Validation(format!(
                "{} contains {token_index} raster cells, expected {}",
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
        let parse_float = |key: &'static str| -> Result<f64> {
            fields
                .get(key)
                .ok_or_else(|| Error::Validation(format!("{} is missing {key}", path.display())))?
                .parse()
                .map_err(|error: std::num::ParseFloatError| Error::InvalidField {
                    path: path.into(),
                    field: key,
                    value: fields[key].clone(),
                    message: error.to_string(),
                })
        };
        let parse_usize = |key: &'static str| -> Result<usize> {
            fields
                .get(key)
                .ok_or_else(|| Error::Validation(format!("{} is missing {key}", path.display())))?
                .parse()
                .map_err(|error: std::num::ParseIntError| Error::InvalidField {
                    path: path.into(),
                    field: key,
                    value: fields[key].clone(),
                    message: error.to_string(),
                })
        };
        let columns = parse_usize("ncols")?;
        let rows = parse_usize("nrows")?;
        let west = parse_float("xllcorner")?;
        let south = parse_float("yllcorner")?;
        let cell_size = parse_float("cellsize")?;
        let nodata = parse_float("nodata_value")?;
        if columns == 0
            || rows == 0
            || cell_size <= 0.0
            || ![west, south, cell_size, nodata]
                .iter()
                .all(|value| value.is_finite())
        {
            return Err(Error::Validation(format!(
                "{} has invalid ESRI ASCII dimensions or coordinates",
                path.display()
            )));
        }
        Ok(Self {
            columns,
            rows,
            west,
            south,
            cell_size,
            nodata,
        })
    }

    fn cell_count(&self) -> Result<usize> {
        self.columns.checked_mul(self.rows).ok_or_else(|| {
            Error::Validation("ESRI ASCII grid dimensions overflow the address space".into())
        })
    }

    fn require_hyde(&self, path: &Path) -> Result<()> {
        let epsilon = 1e-9;
        if self.columns != HYDE_COLUMNS
            || self.rows != HYDE_ROWS
            || (self.west + 180.0).abs() > epsilon
            || (self.south + 90.0).abs() > epsilon
            || (self.cell_size - HYDE_CELL_SIZE).abs() > epsilon
        {
            return Err(Error::Validation(format!(
                "{} is not a global 5-arcminute HYDE 3.2 grid",
                path.display()
            )));
        }
        Ok(())
    }

    fn cell(&self, path: &Path, latitude: f64, longitude: f64) -> Result<usize> {
        let column = ((longitude - self.west) / self.cell_size).floor();
        let north = self.south + self.cell_size * self.rows as f64;
        let row = ((north - latitude) / self.cell_size).floor();
        if !latitude.is_finite()
            || !longitude.is_finite()
            || column < 0.0
            || row < 0.0
            || column >= self.columns as f64
            || row >= self.rows as f64
        {
            return Err(Error::Validation(format!(
                "coordinate ({latitude}, {longitude}) is outside {}",
                path.display()
            )));
        }
        Ok(row as usize * self.columns + column as usize)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{AsciiGrid, GridHeader, LandUseSourceValues, profile_from_fractions};

    #[test]
    fn ascii_grid_samples_north_to_south_and_preserves_nodata() {
        let path = std::env::temp_dir().join(format!(
            "adventuresim-hyde-{}.asc",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            "ncols 2\nnrows 2\nxllcorner 0\nyllcorner 0\ncellsize 1\nNODATA_value -9999\n1 2\n3 -9999\n",
        )
        .unwrap();
        let samples = AsciiGrid::sample(&path, &[(1.5, 0.5), (0.5, 1.5)], false).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(samples, vec![Some(1.0), None]);
    }

    #[test]
    fn source_areas_interpolate_into_an_exhaustive_profile() {
        let values = LandUseSourceValues {
            cell_area: Some(100.0),
            cropland_start: Some(10.0),
            cropland_end: Some(30.0),
            grazing_start: Some(20.0),
            grazing_end: Some(20.0),
            built_up_start: Some(1.0),
            built_up_end: Some(1.0),
        };
        let profile = values.profile(0.44).unwrap();
        assert_eq!(profile.cropland().basis_points(), 1_880);
        assert_eq!(profile.grazing().basis_points(), 2_000);
        assert_eq!(profile.built_up().basis_points(), 100);
        assert_eq!(profile.natural().basis_points(), 6_020);
    }

    #[test]
    fn source_boundary_requires_the_hyde_global_five_arcminute_grid() {
        let hyde = GridHeader {
            columns: 4_320,
            rows: 2_160,
            west: -180.0,
            south: -90.0,
            cell_size: 1.0 / 12.0,
            nodata: -9_999.0,
        };
        assert!(hyde.require_hyde(std::path::Path::new("hyde.asc")).is_ok());
        let wrong_resolution = GridHeader {
            columns: 360,
            rows: 180,
            cell_size: 1.0,
            ..hyde
        };
        assert!(
            wrong_resolution
                .require_hyde(std::path::Path::new("renamed.asc"))
                .is_err()
        );
    }

    #[test]
    fn overlapping_source_areas_are_normalized_without_invalid_totals() {
        let profile = profile_from_fractions(0.8, 0.6, 0.1);
        let total = profile.cropland().basis_points()
            + profile.grazing().basis_points()
            + profile.built_up().basis_points()
            + profile.natural().basis_points();
        assert_eq!(total, 10_000);
        assert_eq!(profile.natural().basis_points(), 0);
    }
}
