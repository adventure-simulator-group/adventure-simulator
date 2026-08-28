//! Strong coordinate units shared across strategic and tactical systems.

const E7_UNITS_PER_COORDINATE_UNIT: i32 = 10_000_000;
const MILLIONTHS_PER_COORDINATE_UNIT: i32 = 1_000_000;

/// An E7-scaled coordinate component without WGS84 axis bounds.
///
/// This is reserved for strategic locations whose coordinate system is
/// explicitly abstract. Geographic positions must use [`LatitudeE7`],
/// [`LongitudeE7`], or [`Wgs84CoordinateE7`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UnboundedCoordinateE7(i32);

impl UnboundedCoordinateE7 {
    pub const UNITS_PER_COORDINATE_UNIT: i32 = E7_UNITS_PER_COORDINATE_UNIT;

    pub const fn from_raw(value: i32) -> Self {
        Self(value)
    }

    pub fn from_coordinate_units(value: f64) -> Option<Self> {
        let scaled = value * f64::from(Self::UNITS_PER_COORDINATE_UNIT);
        if !scaled.is_finite() || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&scaled) {
            return None;
        }
        Some(Self(scaled.round() as i32))
    }

    pub const fn raw(self) -> i32 {
        self.0
    }

    pub fn coordinate_units(self) -> f64 {
        f64::from(self.0) / f64::from(Self::UNITS_PER_COORDINATE_UNIT)
    }

    pub const fn millionths_of_coordinate_unit(self) -> i32 {
        self.0 / (Self::UNITS_PER_COORDINATE_UNIT / MILLIONTHS_PER_COORDINATE_UNIT)
    }
}

/// A validated WGS84 latitude stored in ten-millionths of a degree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LatitudeE7(i32);

impl LatitudeE7 {
    pub const UNITS_PER_DEGREE: i32 = E7_UNITS_PER_COORDINATE_UNIT;
    pub const MIN: Self = Self(-90 * Self::UNITS_PER_DEGREE);
    pub const MAX: Self = Self(90 * Self::UNITS_PER_DEGREE);
    pub const ZERO: Self = Self(0);

    pub const fn new(value: i32) -> Option<Self> {
        if value >= Self::MIN.0 && value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn from_degrees(degrees: f64) -> Option<Self> {
        if !degrees.is_finite() || !(-90.0..=90.0).contains(&degrees) {
            return None;
        }
        Self::new((degrees * f64::from(Self::UNITS_PER_DEGREE)).round() as i32)
    }

    pub const fn get(self) -> i32 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        f64::from(self.0) / f64::from(Self::UNITS_PER_DEGREE)
    }

    pub const fn to_microdegrees(self) -> LatitudeMicrodegrees {
        LatitudeMicrodegrees(
            self.0 / (Self::UNITS_PER_DEGREE / LatitudeMicrodegrees::UNITS_PER_DEGREE),
        )
    }
}

/// A validated WGS84 longitude stored in ten-millionths of a degree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LongitudeE7(i32);

impl LongitudeE7 {
    pub const UNITS_PER_DEGREE: i32 = E7_UNITS_PER_COORDINATE_UNIT;
    pub const MIN: Self = Self(-180 * Self::UNITS_PER_DEGREE);
    pub const MAX: Self = Self(180 * Self::UNITS_PER_DEGREE);
    pub const ZERO: Self = Self(0);

    pub const fn new(value: i32) -> Option<Self> {
        if value >= Self::MIN.0 && value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn from_degrees(degrees: f64) -> Option<Self> {
        if !degrees.is_finite() || !(-180.0..=180.0).contains(&degrees) {
            return None;
        }
        Self::new((degrees * f64::from(Self::UNITS_PER_DEGREE)).round() as i32)
    }

    pub const fn get(self) -> i32 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        f64::from(self.0) / f64::from(Self::UNITS_PER_DEGREE)
    }

    pub const fn to_microdegrees(self) -> LongitudeMicrodegrees {
        LongitudeMicrodegrees(
            self.0 / (Self::UNITS_PER_DEGREE / LongitudeMicrodegrees::UNITS_PER_DEGREE),
        )
    }
}

/// A validated WGS84 coordinate stored in ten-millionths of a degree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Wgs84CoordinateE7 {
    latitude: LatitudeE7,
    longitude: LongitudeE7,
}

impl Wgs84CoordinateE7 {
    pub const fn new(latitude_e7: i32, longitude_e7: i32) -> Option<Self> {
        let Some(latitude) = LatitudeE7::new(latitude_e7) else {
            return None;
        };
        let Some(longitude) = LongitudeE7::new(longitude_e7) else {
            return None;
        };
        Some(Self {
            latitude,
            longitude,
        })
    }

    /// Converts a longitude/latitude pair in degrees into its validated E7
    /// representation.
    pub fn from_longitude_latitude_degrees(longitude: f64, latitude: f64) -> Option<Self> {
        let latitude = LatitudeE7::from_degrees(latitude)?;
        let longitude = LongitudeE7::from_degrees(longitude)?;
        Some(Self {
            latitude,
            longitude,
        })
    }

    pub const fn latitude(self) -> LatitudeE7 {
        self.latitude
    }

    pub const fn longitude(self) -> LongitudeE7 {
        self.longitude
    }

    /// Returns the pair in the longitude/latitude order used by strategic
    /// positions and geographic distance functions.
    pub fn longitude_latitude_degrees(self) -> (f64, f64) {
        (self.longitude.degrees(), self.latitude.degrees())
    }

    /// Returns the pair in the latitude/longitude order used by terrain route
    /// requests.
    pub fn latitude_longitude_degrees(self) -> (f64, f64) {
        (self.latitude.degrees(), self.longitude.degrees())
    }
}

/// A validated WGS84 latitude stored in millionths of a degree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LatitudeMicrodegrees(i32);

impl LatitudeMicrodegrees {
    pub const UNITS_PER_DEGREE: i32 = 1_000_000;
    pub const MIN: Self = Self(-90 * Self::UNITS_PER_DEGREE);
    pub const MAX: Self = Self(90 * Self::UNITS_PER_DEGREE);
    pub const ZERO: Self = Self(0);

    pub const fn new(value: i32) -> Option<Self> {
        if value >= Self::MIN.0 && value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn from_degrees(degrees: f64) -> Option<Self> {
        if !degrees.is_finite() || !(-90.0..=90.0).contains(&degrees) {
            return None;
        }
        Self::new((degrees * f64::from(Self::UNITS_PER_DEGREE)).round() as i32)
    }

    pub const fn get(self) -> i32 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        f64::from(self.0) / f64::from(Self::UNITS_PER_DEGREE)
    }

    pub const fn to_e7(self) -> LatitudeE7 {
        LatitudeE7(self.0 * (LatitudeE7::UNITS_PER_DEGREE / Self::UNITS_PER_DEGREE))
    }
}

/// A validated WGS84 longitude stored in millionths of a degree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LongitudeMicrodegrees(i32);

impl LongitudeMicrodegrees {
    pub const UNITS_PER_DEGREE: i32 = 1_000_000;
    pub const MIN: Self = Self(-180 * Self::UNITS_PER_DEGREE);
    pub const MAX: Self = Self(180 * Self::UNITS_PER_DEGREE);
    pub const ZERO: Self = Self(0);

    pub const fn new(value: i32) -> Option<Self> {
        if value >= Self::MIN.0 && value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn from_degrees(degrees: f64) -> Option<Self> {
        if !degrees.is_finite() || !(-180.0..=180.0).contains(&degrees) {
            return None;
        }
        Self::new((degrees * f64::from(Self::UNITS_PER_DEGREE)).round() as i32)
    }

    pub const fn get(self) -> i32 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        f64::from(self.0) / f64::from(Self::UNITS_PER_DEGREE)
    }

    pub const fn to_e7(self) -> LongitudeE7 {
        LongitudeE7(self.0 * (LongitudeE7::UNITS_PER_DEGREE / Self::UNITS_PER_DEGREE))
    }
}

/// A validated WGS84 coordinate stored in millionths of a degree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Wgs84CoordinateMicrodegrees {
    latitude: LatitudeMicrodegrees,
    longitude: LongitudeMicrodegrees,
}

impl Wgs84CoordinateMicrodegrees {
    pub const fn new(latitude: i32, longitude: i32) -> Option<Self> {
        let Some(latitude) = LatitudeMicrodegrees::new(latitude) else {
            return None;
        };
        let Some(longitude) = LongitudeMicrodegrees::new(longitude) else {
            return None;
        };
        Some(Self {
            latitude,
            longitude,
        })
    }

    pub fn from_longitude_latitude_degrees(longitude: f64, latitude: f64) -> Option<Self> {
        Some(Self {
            latitude: LatitudeMicrodegrees::from_degrees(latitude)?,
            longitude: LongitudeMicrodegrees::from_degrees(longitude)?,
        })
    }

    pub const fn from_e7(coordinate: Wgs84CoordinateE7) -> Self {
        Self {
            latitude: coordinate.latitude().to_microdegrees(),
            longitude: coordinate.longitude().to_microdegrees(),
        }
    }

    pub const fn latitude(self) -> LatitudeMicrodegrees {
        self.latitude
    }

    pub const fn longitude(self) -> LongitudeMicrodegrees {
        self.longitude
    }

    pub const fn to_e7(self) -> Wgs84CoordinateE7 {
        Wgs84CoordinateE7 {
            latitude: self.latitude.to_e7(),
            longitude: self.longitude.to_e7(),
        }
    }

    pub fn longitude_latitude_degrees(self) -> (f64, f64) {
        (self.longitude.degrees(), self.latitude.degrees())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latitude_units_enforce_wgs84_bounds() {
        assert_eq!(
            LatitudeE7::new(LatitudeE7::MIN.get()),
            Some(LatitudeE7::MIN)
        );
        assert_eq!(
            LatitudeE7::new(LatitudeE7::MAX.get()),
            Some(LatitudeE7::MAX)
        );
        assert!(LatitudeE7::new(LatitudeE7::MIN.get() - 1).is_none());
        assert!(LatitudeE7::new(LatitudeE7::MAX.get() + 1).is_none());

        assert_eq!(
            LatitudeMicrodegrees::new(LatitudeMicrodegrees::MIN.get()),
            Some(LatitudeMicrodegrees::MIN)
        );
        assert_eq!(
            LatitudeMicrodegrees::new(LatitudeMicrodegrees::MAX.get()),
            Some(LatitudeMicrodegrees::MAX)
        );
        assert!(LatitudeMicrodegrees::new(LatitudeMicrodegrees::MIN.get() - 1).is_none());
        assert!(LatitudeMicrodegrees::new(LatitudeMicrodegrees::MAX.get() + 1).is_none());
    }

    #[test]
    fn unbounded_coordinate_preserves_abstract_coordinate_units() {
        let coordinate = UnboundedCoordinateE7::from_coordinate_units(150.125).unwrap();

        assert_eq!(coordinate.raw(), 1_501_250_000);
        assert_eq!(coordinate.coordinate_units(), 150.125);
        assert_eq!(coordinate.millionths_of_coordinate_unit(), 150_125_000);
    }

    #[test]
    fn unbounded_coordinate_rejects_non_finite_and_unrepresentable_units() {
        let scale = f64::from(UnboundedCoordinateE7::UNITS_PER_COORDINATE_UNIT);

        assert!(UnboundedCoordinateE7::from_coordinate_units(f64::NAN).is_none());
        assert!(UnboundedCoordinateE7::from_coordinate_units(f64::INFINITY).is_none());
        assert!(
            UnboundedCoordinateE7::from_coordinate_units(f64::from(i32::MAX) / scale + 1.0)
                .is_none()
        );
        assert!(
            UnboundedCoordinateE7::from_coordinate_units(f64::from(i32::MIN) / scale - 1.0)
                .is_none()
        );
    }

    #[test]
    fn coordinate_pair_rejects_an_invalid_component() {
        assert!(Wgs84CoordinateE7::new(505_000_000, 105_000_000).is_some());
        assert!(Wgs84CoordinateE7::new(LatitudeE7::MAX.get() + 1, 0).is_none());
        assert!(Wgs84CoordinateE7::new(0, LongitudeE7::MAX.get() + 1).is_none());
    }

    #[test]
    fn coordinate_pair_converts_degrees_without_order_ambiguity() {
        let coordinate = Wgs84CoordinateE7::from_longitude_latitude_degrees(10.5, 53.25).unwrap();

        assert_eq!(coordinate.longitude().get(), 105_000_000);
        assert_eq!(coordinate.latitude().get(), 532_500_000);
        assert_eq!(coordinate.longitude_latitude_degrees(), (10.5, 53.25));
        assert_eq!(coordinate.latitude_longitude_degrees(), (53.25, 10.5));
    }

    #[test]
    fn longitude_units_enforce_wgs84_bounds() {
        assert_eq!(
            LongitudeE7::new(LongitudeE7::MIN.get()),
            Some(LongitudeE7::MIN)
        );
        assert_eq!(
            LongitudeE7::new(LongitudeE7::MAX.get()),
            Some(LongitudeE7::MAX)
        );
        assert!(LongitudeE7::new(LongitudeE7::MIN.get() - 1).is_none());
        assert!(LongitudeE7::new(LongitudeE7::MAX.get() + 1).is_none());

        assert_eq!(
            LongitudeMicrodegrees::new(LongitudeMicrodegrees::MIN.get()),
            Some(LongitudeMicrodegrees::MIN)
        );
        assert_eq!(
            LongitudeMicrodegrees::new(LongitudeMicrodegrees::MAX.get()),
            Some(LongitudeMicrodegrees::MAX)
        );
        assert!(LongitudeMicrodegrees::new(LongitudeMicrodegrees::MIN.get() - 1).is_none());
        assert!(LongitudeMicrodegrees::new(LongitudeMicrodegrees::MAX.get() + 1).is_none());
    }

    #[test]
    fn degree_construction_preserves_rounding() {
        let latitude = LatitudeE7::from_degrees(53.500_000_05).unwrap();
        let longitude = LongitudeMicrodegrees::from_degrees(10.000_000_5).unwrap();

        assert_eq!(latitude.get(), 535_000_001);
        assert_eq!(longitude.get(), 10_000_001);
        assert_eq!(latitude.degrees(), 53.500_000_1);
        assert_eq!(longitude.degrees(), 10.000_001);
        assert!(LatitudeE7::from_degrees(f64::NAN).is_none());
        assert!(LongitudeMicrodegrees::from_degrees(180.000_001).is_none());
    }

    #[test]
    fn e7_to_microdegree_conversion_preserves_integer_truncation() {
        let positive = LatitudeE7::new(535_000_009).unwrap();
        let negative = LongitudeE7::new(-100_000_009).unwrap();

        assert_eq!(positive.to_microdegrees().get(), 53_500_000);
        assert_eq!(negative.to_microdegrees().get(), -10_000_000);
        assert_eq!(positive.to_microdegrees().to_e7().get(), 535_000_000);
        assert_eq!(negative.to_microdegrees().to_e7().get(), -100_000_000);
    }

    #[test]
    fn coordinate_pairs_round_trip_between_e7_and_microdegrees() {
        let e7 = Wgs84CoordinateE7::new(535_000_009, 100_000_009).unwrap();
        let microdegrees = Wgs84CoordinateMicrodegrees::from_e7(e7);

        assert_eq!(microdegrees.latitude().get(), 53_500_000);
        assert_eq!(microdegrees.longitude().get(), 10_000_000);
        assert_eq!(microdegrees.to_e7().latitude().get(), 535_000_000);
        assert_eq!(microdegrees.to_e7().longitude().get(), 100_000_000);
        assert!(Wgs84CoordinateMicrodegrees::new(90_000_001, 0).is_none());
    }

    #[test]
    fn travel_geometry_boundary_keeps_primitive_e7_wire_shape() {
        let point = crate::TravelGeometryPoint::new(10.000_000_05, 53.500_000_05).unwrap();

        assert_eq!(
            serde_json::to_value(point).unwrap(),
            serde_json::json!({
                "longitude_e7": 100_000_001,
                "latitude_e7": 535_000_001,
            })
        );
    }
}
