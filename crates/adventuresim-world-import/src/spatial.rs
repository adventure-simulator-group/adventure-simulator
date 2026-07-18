//! Canonical spatial-grid projection and cell assignment for importer stages.
//!
//! This module deliberately contains no raster/vector readers. A source stage
//! must resolve nodata explicitly: distinguish outside source coverage from a
//! covered cell whose observations are nodata, record the chosen fallback in
//! provenance, and never silently treat either state as a numeric zero.

use adventuresim_world_schema::SpatialGridSpec;
use proj4rs::{proj::Proj, transform::transform};

use crate::{Error, Result};

const MILLIMETERS_PER_METER: i64 = 1_000;
const I64_LOWER_INCLUSIVE: f64 = -9_223_372_036_854_775_808.0;
const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;

/// Finite EPSG:3035 coordinate quantized to the nearest millimetre.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectedCoordinate {
    easting_mm: i64,
    northing_mm: i64,
}

impl ProjectedCoordinate {
    pub fn from_meters(easting_m: f64, northing_m: f64) -> Result<Self> {
        let quantize = |meters: f64| {
            let millimeters = (meters * MILLIMETERS_PER_METER as f64).round();
            (millimeters.is_finite()
                && (I64_LOWER_INCLUSIVE..I64_UPPER_EXCLUSIVE).contains(&millimeters))
            .then_some(millimeters as i64)
        };
        let Some(easting_mm) = quantize(easting_m) else {
            return Err(invalid_projected_coordinate(easting_m, northing_m));
        };
        let Some(northing_mm) = quantize(northing_m) else {
            return Err(invalid_projected_coordinate(easting_m, northing_m));
        };
        Ok(Self {
            easting_mm,
            northing_mm,
        })
    }

    pub const fn easting_millimeters(self) -> i64 {
        self.easting_mm
    }
    pub const fn northing_millimeters(self) -> i64 {
        self.northing_mm
    }
    pub fn easting_meters(self) -> f64 {
        self.easting_mm as f64 / MILLIMETERS_PER_METER as f64
    }
    pub fn northing_meters(self) -> f64 {
        self.northing_mm as f64 / MILLIMETERS_PER_METER as f64
    }

    pub fn cell(self, grid: SpatialGridSpec) -> SpatialCellId {
        let cell_mm = i64::from(grid.cell_size_meters().get()) * MILLIMETERS_PER_METER;
        SpatialCellId {
            column: self.easting_mm.div_euclid(cell_mm),
            row: self.northing_mm.div_euclid(cell_mm),
        }
    }
}

fn invalid_projected_coordinate(easting_m: f64, northing_m: f64) -> Error {
    Error::Validation(format!(
        "invalid EPSG:3035 coordinate ({easting_m}, {northing_m})"
    ))
}

/// Stable column/row address in a [`SpatialGridSpec`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SpatialCellId {
    column: i64,
    row: i64,
}

impl SpatialCellId {
    pub const fn column(self) -> i64 {
        self.column
    }
    pub const fn row(self) -> i64 {
        self.row
    }
}

/// Deterministic WGS84 longitude/latitude to EPSG:3035 projector.
pub struct SpatialProjection {
    geographic: Proj,
    projected: Proj,
}

impl SpatialProjection {
    pub fn new() -> Result<Self> {
        Ok(Self {
            geographic: Proj::from_proj_string(
                "+proj=longlat +datum=WGS84 +ellps=WGS84 +no_defs +type=crs",
            )?,
            projected: Proj::from_proj_string(
                "+proj=laea +lat_0=52 +lon_0=10 +x_0=4321000 +y_0=3210000 +ellps=GRS80 +units=m +no_defs +type=crs",
            )?,
        })
    }

    pub fn project(&self, latitude: f64, longitude: f64) -> Result<ProjectedCoordinate> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(Error::Validation(format!(
                "invalid WGS84 coordinate ({latitude}, {longitude})"
            )));
        }
        let mut coordinate = (longitude.to_radians(), latitude.to_radians(), 0.0);
        transform(&self.geographic, &self.projected, &mut coordinate)?;
        ProjectedCoordinate::from_meters(coordinate.0, coordinate.1)
    }

    /// Inverse of [`Self::project`], used when a source raster is geographic
    /// but route interpolation is required on the canonical projected grid.
    pub fn unproject(&self, coordinate: ProjectedCoordinate) -> Result<(f64, f64)> {
        let mut raw = (
            coordinate.easting_meters(),
            coordinate.northing_meters(),
            0.0,
        );
        transform(&self.projected, &self.geographic, &mut raw)?;
        let latitude = raw.1.to_degrees();
        let longitude = raw.0.to_degrees();
        if !latitude.is_finite() || !longitude.is_finite() {
            return Err(Error::Validation(
                "inverse EPSG:3035 projection was non-finite".into(),
            ));
        }
        Ok((latitude, longitude))
    }
}

#[cfg(test)]
mod tests {
    use adventuresim_world_schema::{GridCellSizeMeters, SpatialGridSpec};

    use super::{ProjectedCoordinate, SpatialProjection};

    #[test]
    fn cell_assignment_uses_euclidean_division_at_boundaries() {
        let grid = SpatialGridSpec::new(GridCellSizeMeters::new(1_000).unwrap());
        for (x, expected) in [
            (-1000.0, -1),
            (-999.999, -1),
            (-0.001, -1),
            (0.0, 0),
            (999.999, 0),
            (1000.0, 1),
        ] {
            assert_eq!(
                ProjectedCoordinate::from_meters(x, x)
                    .unwrap()
                    .cell(grid)
                    .column(),
                expected
            );
        }
    }

    #[test]
    fn projection_rejects_nonfinite_and_out_of_range_coordinates() {
        let projection = SpatialProjection::new().unwrap();
        assert!(projection.project(f64::NAN, 10.0).is_err());
        assert!(projection.project(91.0, 10.0).is_err());
        assert!(projection.project(52.0, 181.0).is_err());
        assert!(ProjectedCoordinate::from_meters(f64::INFINITY, 0.0).is_err());
    }

    #[test]
    fn millimeter_quantization_rejects_positive_i64_overflow_boundary() {
        let upper_exclusive_mm = 9_223_372_036_854_775_808.0;
        assert!(ProjectedCoordinate::from_meters(upper_exclusive_mm / 1_000.0, 0.0).is_err());
    }

    #[test]
    fn millimeter_quantization_accepts_nearest_representable_boundaries() {
        let nearest_below_upper_mm = f64::from_bits(9_223_372_036_854_775_808.0_f64.to_bits() - 1);
        let lower_inclusive_mm = -9_223_372_036_854_775_808.0;

        let nearest_valid_meters = nearest_below_upper_mm / 1_000.0;
        let expected_mm = (nearest_valid_meters * 1_000.0).round() as i64;
        let positive =
            ProjectedCoordinate::from_meters(nearest_valid_meters, lower_inclusive_mm / 1_000.0)
                .unwrap();
        assert_eq!(positive.easting_millimeters(), expected_mm);
        assert!(positive.easting_millimeters() < i64::MAX);
        assert_eq!(positive.northing_millimeters(), i64::MIN);
    }

    #[test]
    fn wgs84_fixture_projects_to_a_stable_epsg3035_cell() {
        let coordinate = SpatialProjection::new()
            .unwrap()
            .project(52.0, 10.0)
            .unwrap();
        assert_eq!(coordinate.easting_millimeters(), 4_321_000_000);
        assert_eq!(coordinate.northing_millimeters(), 3_210_000_000);
        let cell = coordinate.cell(SpatialGridSpec::default());
        assert_eq!((cell.column(), cell.row()), (4_321, 3_210));
    }
}
