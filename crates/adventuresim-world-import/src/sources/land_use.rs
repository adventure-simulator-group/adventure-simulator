//! LUH1 annual historical land-use sampling.
//!
//! LUH1 is a half-degree, modelled global reconstruction.  We deliberately do
//! not assume a particular production grid shape here: the five state files
//! must instead prove that they use the same coordinates and annual time axis.

use std::path::{Path, PathBuf};

use adventuresim_world_schema::{LandUseFraction, LandUseProfile, SourceProvenance};
use netcdf_reader::{NcFile, NcFormat, NcSliceInfo, NcSliceInfoElem, NcType};

use crate::{
    Error, Result,
    draft::{ElevatedSettlementDraft, LandUseSettlementDraft, WorldDraft, push_source_note},
};

const SOURCE_NAME: &str = "LUH1: Harmonized Global Land Use for Years 1500-2100, V1";
const SOURCE_URL: &str = "https://doi.org/10.3334/ORNLDAAC/1248";
const MAX_NORMALIZABLE_OVERFILL: f64 = 1.000_001;
const AXIS_EPSILON: f64 = 1e-9;

const STATES: [StateFile; 5] = [
    StateFile::new("gcrop", "cropland"),
    StateFile::new("gpast", "pasture"),
    StateFile::new("gurbn", "urban"),
    StateFile::new("gothr", "primary land"),
    StateFile::new("gsecd", "secondary land"),
];

#[derive(Clone, Copy)]
struct StateFile {
    variable: &'static str,
    label: &'static str,
}

impl StateFile {
    const fn new(variable: &'static str, label: &'static str) -> Self {
        Self { variable, label }
    }

    fn filename(self) -> String {
        format!("LUHa_u2.v1_{}.nc4", self.variable)
    }
}

pub(crate) fn enrich(
    mut draft: WorldDraft<ElevatedSettlementDraft>,
    directory: &Path,
) -> Result<WorldDraft<LandUseSettlementDraft>> {
    let grid = LuhGrid::open(directory, draft.year)?;
    let mut fallbacks = 0;
    let mut normalized = 0;
    let settlements = std::mem::take(&mut draft.settlements)
        .into_iter()
        .map(|mut elevated| {
            let sample = grid.sample(
                elevated.settlement.latitude,
                elevated.settlement.longitude,
            )?;
            let (land_use, note) = match sample.profile()? {
                Some((profile, was_normalized)) => {
                    normalized += usize::from(was_normalized);
                    (
                        profile,
                        if was_normalized {
                            "**[LUH1](https://doi.org/10.3334/ORNLDAAC/1248):** A modelled 0.5 degree annual land-use reconstruction was sampled directly for this world year. Tiny floating-point state overfill was deterministically normalized into an exhaustive profile."
                        } else {
                            "**[LUH1](https://doi.org/10.3334/ORNLDAAC/1248):** A modelled 0.5 degree annual land-use reconstruction was sampled directly for this world year. It is a regional grid estimate, not an exact settlement observation."
                        },
                    )
                }
                None => {
                    fallbacks += 1;
                    (
                        fallback_profile(&elevated),
                        "**LUH1 land-use fallback:** The otherwise valid LUH1 state cell was nodata or contained no terrestrial state. Cropland and pasture are deterministically seeded by the Viabundus node, built-up land by settlement population level, and the remainder is natural land.",
                    )
                }
            };
            push_source_note(&mut elevated, note);
            Ok(LandUseSettlementDraft { elevated, land_use })
        })
        .collect::<Result<Vec<_>>>()?;

    draft.sources.push(source_provenance());
    draft.report.land_use_rasters_read = STATES.len();
    draft.report.land_use_samples = settlements.len();
    draft.report.land_use_fallback_samples = fallbacks;
    draft.report.land_use_normalized_samples = normalized;
    Ok(WorldDraft {
        year: draft.year,
        world_bounds: draft.world_bounds,
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
    SourceProvenance {
        name: SOURCE_NAME.into(),
        url: SOURCE_URL.into(),
        // The official LUH1 catalogue asks users to cite the source, but we
        // have not verified a redistributable licence statement for the files.
        license: "Licence not stated by the LUH1 catalogue; citation required".into(),
    }
}

#[derive(Clone, Copy, Debug)]
struct LandUseStates {
    crop: Option<f64>,
    pasture: Option<f64>,
    urban: Option<f64>,
    primary: Option<f64>,
    secondary: Option<f64>,
}

impl LandUseStates {
    fn profile(self) -> Result<Option<(LandUseProfile, bool)>> {
        let Some([crop, pasture, urban, primary, secondary]) = self.values() else {
            return Ok(None);
        };
        let values = [crop, pasture, urban, primary, secondary];
        if values.iter().all(|value| *value == 0.0) {
            return Ok(None);
        }
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 1.0)
        {
            return Err(Error::Validation(
                "LUH1 state sample must be a finite fraction between zero and one".into(),
            ));
        }
        let total = values.iter().sum::<f64>();
        if total > MAX_NORMALIZABLE_OVERFILL {
            return Err(Error::Validation(format!(
                "LUH1 state fractions sum to {total}, which exceeds the small floating-point tolerance"
            )));
        }
        if total < 1.0 - AXIS_EPSILON {
            return Err(Error::Validation(format!(
                "LUH1 terrestrial state fractions sum to {total}, not one"
            )));
        }
        let normalized = total > 1.0;
        let scale = if normalized { total } else { 1.0 };
        profile_from_components(
            crop / scale,
            pasture / scale,
            urban / scale,
            (primary + secondary) / scale,
        )
        .map(|profile| Some((profile, normalized)))
    }

    fn values(self) -> Option<[f64; 5]> {
        Some([
            self.crop?,
            self.pasture?,
            self.urban?,
            self.primary?,
            self.secondary?,
        ])
    }
}

fn profile_from_components(
    crop: f64,
    pasture: f64,
    urban: f64,
    natural: f64,
) -> Result<LandUseProfile> {
    let mut basis_points =
        [crop, pasture, urban].map(|fraction| (fraction * 10_000.0).round() as u16);
    let natural_points = (natural * 10_000.0).round() as u16;
    let total = basis_points
        .iter()
        .map(|value| u32::from(*value))
        .sum::<u32>()
        + u32::from(natural_points);
    if total != 10_000 {
        // The source contract already established that this is at most normal
        // rounding error. Put its deterministic remainder in natural land.
        let managed = basis_points
            .iter()
            .map(|value| u32::from(*value))
            .sum::<u32>();
        if managed > 10_000 {
            let largest = basis_points
                .iter()
                .enumerate()
                .max_by_key(|(_, value)| *value)
                .map(|(index, _)| index)
                .expect("three managed components");
            basis_points[largest] -= (managed - 10_000) as u16;
        }
    }
    let managed = basis_points
        .iter()
        .map(|value| u32::from(*value))
        .sum::<u32>();
    let natural = 10_000u16.checked_sub(managed as u16).ok_or_else(|| {
        Error::Validation("LUH1 rounded land-use fractions exceed an exhaustive profile".into())
    })?;
    LandUseProfile::new(
        LandUseFraction::new(basis_points[0]).expect("bounded basis points"),
        LandUseFraction::new(basis_points[1]).expect("bounded basis points"),
        LandUseFraction::new(basis_points[2]).expect("bounded basis points"),
        LandUseFraction::new(natural).expect("bounded basis points"),
    )
    .ok_or_else(|| {
        Error::Validation("LUH1 states do not form an exhaustive land-use profile".into())
    })
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

struct LuhGrid {
    longitudes: Vec<f64>,
    latitudes: Vec<f64>,
    states: Vec<LandUseStates>,
}

impl LuhGrid {
    fn open(directory: &Path, year: i32) -> Result<Self> {
        let mut reference: Option<(Vec<f64>, Vec<f64>, Vec<f64>)> = None;
        let mut component_values: Vec<Vec<Option<f64>>> = Vec::with_capacity(STATES.len());
        for state in STATES {
            let path = require(directory, &state.filename())?;
            let component = read_component(&path, state, year)?;
            if let Some((longitudes, latitudes, times)) = &reference {
                require_same_axis(&path, "longitude", longitudes, &component.longitudes)?;
                require_same_axis(&path, "latitude", latitudes, &component.latitudes)?;
                require_same_axis(&path, "time", times, &component.times)?;
            } else {
                reference = Some((
                    component.longitudes.clone(),
                    component.latitudes.clone(),
                    component.times.clone(),
                ));
            }
            component_values.push(component.values);
        }
        let (longitudes, latitudes, _) = reference.expect("five required LUH1 states");
        let cells = longitudes
            .len()
            .checked_mul(latitudes.len())
            .ok_or_else(|| {
                Error::Validation("LUH1 grid dimensions overflow the address space".into())
            })?;
        if component_values.iter().any(|values| values.len() != cells) {
            return Err(Error::Validation(
                "LUH1 component slice dimensions do not match the coordinate grid".into(),
            ));
        }
        let states = (0..cells)
            .map(|index| LandUseStates {
                crop: component_values[0][index],
                pasture: component_values[1][index],
                urban: component_values[2][index],
                primary: component_values[3][index],
                secondary: component_values[4][index],
            })
            .collect();
        Ok(Self {
            longitudes,
            latitudes,
            states,
        })
    }

    fn sample(&self, latitude: f64, longitude: f64) -> Result<LandUseStates> {
        let longitude = nearest_axis_index(&self.longitudes, longitude).ok_or_else(|| {
            Error::Validation(format!("longitude {longitude} is outside the LUH1 grid"))
        })?;
        let latitude = nearest_axis_index(&self.latitudes, latitude).ok_or_else(|| {
            Error::Validation(format!("latitude {latitude} is outside the LUH1 grid"))
        })?;
        Ok(self.states[longitude * self.latitudes.len() + latitude])
    }
}

struct Component {
    longitudes: Vec<f64>,
    latitudes: Vec<f64>,
    times: Vec<f64>,
    values: Vec<Option<f64>>,
}

fn read_component(path: &Path, state: StateFile, year: i32) -> Result<Component> {
    let file = nc(path, NcFile::open(path))?;
    if !matches!(file.format(), NcFormat::Nc4 | NcFormat::Nc4Classic) {
        return Err(invalid(
            path,
            "format",
            format!("{:?}", file.format()),
            "expected a NetCDF-4 LUH1 state file",
        ));
    }
    let variable = nc(path, file.variable(state.variable))?;
    if !matches!(variable.dtype, NcType::Float | NcType::Double) {
        return Err(invalid(
            path,
            state.variable,
            format!("{:?}", variable.dtype),
            "expected a floating-point LUH1 state variable",
        ));
    }
    let dimensions: Vec<_> = variable
        .dimensions
        .iter()
        .map(|dimension| dimension.name.as_str())
        .collect();
    if dimensions.len() != 3
        || !dimensions.contains(&"time")
        || !dimensions.contains(&"lat")
        || !dimensions.contains(&"lon")
    {
        return Err(invalid(
            path,
            state.variable,
            format!("{dimensions:?}"),
            "expected dimensions containing time, lat, and lon exactly once",
        ));
    }
    let longitudes = read_axis(&file, path, "lon")?;
    let latitudes = read_axis(&file, path, "lat")?;
    let times = read_axis(&file, path, "time")?;
    let time_index = times
        .iter()
        .position(|value| *value == f64::from(year))
        .ok_or_else(|| {
            Error::Validation(format!(
                "{} does not contain requested annual LUH1 year {year}",
                path.display()
            ))
        })?;
    let dimension_index = |name: &str| {
        dimensions
            .iter()
            .position(|dimension| *dimension == name)
            .expect("validated dimension")
    };
    let mut selections = vec![
        NcSliceInfoElem::Slice {
            start: 0,
            end: 0,
            step: 1
        };
        3
    ];
    for (name, length) in [
        ("time", times.len()),
        ("lat", latitudes.len()),
        ("lon", longitudes.len()),
    ] {
        selections[dimension_index(name)] = if name == "time" {
            NcSliceInfoElem::Index(time_index as u64)
        } else {
            NcSliceInfoElem::Slice {
                start: 0,
                end: length as u64,
                step: 1,
            }
        };
    }
    let raw = nc(
        path,
        file.read_variable_slice_as_f64(state.variable, &NcSliceInfo { selections }),
    )?;
    let expected_shape: Vec<_> = dimensions
        .iter()
        .filter(|name| **name != "time")
        .map(|name| {
            if *name == "lon" {
                longitudes.len()
            } else {
                latitudes.len()
            }
        })
        .collect();
    if raw.shape() != expected_shape {
        return Err(invalid(
            path,
            state.variable,
            format!("{:?}", raw.shape()),
            "unexpected LUH1 annual slice shape",
        ));
    }
    let fill = variable
        .attribute("_FillValue")
        .and_then(|attribute| attribute.value.as_f64());
    let missing = variable
        .attribute("missing_value")
        .and_then(|attribute| attribute.value.as_f64());
    let mut values = vec![None; longitudes.len() * latitudes.len()];
    let spatial_dimensions: Vec<_> = dimensions
        .iter()
        .copied()
        .filter(|name| *name != "time")
        .collect();
    for longitude in 0..longitudes.len() {
        for latitude in 0..latitudes.len() {
            let raw_index = if spatial_dimensions == ["lon", "lat"] {
                [longitude, latitude]
            } else {
                [latitude, longitude]
            };
            let value = raw[raw_index];
            let is_missing = value.is_nan()
                || fill.is_some_and(|marker| value == marker)
                || missing.is_some_and(|marker| value == marker);
            if !is_missing && (!value.is_finite() || !(0.0..=1.0).contains(&value)) {
                return Err(invalid(
                    path,
                    state.variable,
                    value.to_string(),
                    "expected a finite LUH1 fraction between zero and one or nodata",
                ));
            }
            values[longitude * latitudes.len() + latitude] = (!is_missing).then_some(value);
        }
    }
    let usable = values.iter().filter_map(|value| *value).count();
    if usable == 0 {
        return Err(Error::Validation(format!(
            "{} has no numeric {} values for year {year}",
            path.display(),
            state.label
        )));
    }
    Ok(Component {
        longitudes,
        latitudes,
        times,
        values,
    })
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
    if values.is_empty()
        || (name != "time" && values.len() < 2)
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
            "coordinate axis differs from the other LUH1 state files",
        ));
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
            ((axis[index] - axis[index - 1])
                .abs()
                .min((axis[index + 1] - axis[index]).abs()))
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
    use std::path::Path;

    use super::{LandUseStates, LuhGrid, STATES, nearest_axis_index, require_same_axis};

    #[test]
    fn annual_luh_states_map_directly_into_an_exhaustive_profile() {
        let (profile, normalized) = LandUseStates {
            crop: Some(0.2),
            pasture: Some(0.3),
            urban: Some(0.01),
            primary: Some(0.4),
            secondary: Some(0.09),
        }
        .profile()
        .unwrap()
        .unwrap();
        assert!(!normalized);
        assert_eq!(profile.cropland().basis_points(), 2_000);
        assert_eq!(profile.grazing().basis_points(), 3_000);
        assert_eq!(profile.built_up().basis_points(), 100);
        assert_eq!(profile.natural().basis_points(), 4_900);
    }

    #[test]
    fn nodata_or_all_zero_terrestrial_states_use_the_documented_fallback() {
        assert!(
            LandUseStates {
                crop: None,
                pasture: Some(0.0),
                urban: Some(0.0),
                primary: Some(0.0),
                secondary: Some(0.0)
            }
            .profile()
            .unwrap()
            .is_none()
        );
        assert!(
            LandUseStates {
                crop: Some(0.0),
                pasture: Some(0.0),
                urban: Some(0.0),
                primary: Some(0.0),
                secondary: Some(0.0)
            }
            .profile()
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn malformed_or_materially_non_exhaustive_states_are_rejected() {
        assert!(
            LandUseStates {
                crop: Some(0.5),
                pasture: Some(0.5),
                urban: Some(0.2),
                primary: Some(0.0),
                secondary: Some(0.0)
            }
            .profile()
            .is_err()
        );
        assert!(
            LandUseStates {
                crop: Some(0.5),
                pasture: Some(0.0),
                urban: Some(0.0),
                primary: Some(0.0),
                secondary: Some(0.0)
            }
            .profile()
            .is_err()
        );
    }

    #[test]
    fn tiny_floating_point_overfill_is_normalized_but_material_overfill_is_not() {
        let (profile, normalized) = LandUseStates {
            crop: Some(0.2),
            pasture: Some(0.3),
            urban: Some(0.1),
            primary: Some(0.3),
            secondary: Some(0.100_000_000_1),
        }
        .profile()
        .unwrap()
        .unwrap();
        assert!(normalized);
        assert_eq!(
            profile.cropland().basis_points()
                + profile.grazing().basis_points()
                + profile.built_up().basis_points()
                + profile.natural().basis_points(),
            10_000
        );
    }

    #[test]
    fn sampling_uses_coordinate_cell_footprints_without_a_fixed_grid_contract() {
        assert_eq!(nearest_axis_index(&[0.25, 0.75, 1.25], 0.5), Some(0));
        assert_eq!(nearest_axis_index(&[0.25, 0.75, 1.25], 1.5), Some(2));
        assert_eq!(nearest_axis_index(&[0.25, 0.75, 1.25], -0.01), None);
    }

    #[test]
    fn source_contract_requires_the_five_urban_inclusive_state_files() {
        assert_eq!(
            STATES.map(|state| state.filename()),
            [
                "LUHa_u2.v1_gcrop.nc4",
                "LUHa_u2.v1_gpast.nc4",
                "LUHa_u2.v1_gurbn.nc4",
                "LUHa_u2.v1_gothr.nc4",
                "LUHa_u2.v1_gsecd.nc4",
            ]
        );
        assert!(LuhGrid::open(Path::new("definitely-missing-luh1"), 1544).is_err());
    }

    #[test]
    fn components_with_different_coordinate_axes_are_rejected() {
        assert!(
            require_same_axis(
                Path::new("gsecd.nc4"),
                "latitude",
                &[0.25, 0.75],
                &[0.25, 1.25],
            )
            .is_err()
        );
    }
}
