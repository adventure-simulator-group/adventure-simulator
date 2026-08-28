//! Deterministic, low-cost celestial presentation calculations.
//!
//! These are intentionally analytical rather than a high-precision ephemeris.
//! They preserve the important relationships for an outdoor scene: seasonal
//! solar altitude, longitude-adjusted solar time, lunar phase, and the Moon's
//! phase-relative rise and set time.

use crate::strategic_time::{DAYS_PER_YEAR, MINUTES_PER_DAY, lunar_illumination, lunar_phase};
use adventuresim_world_schema::coordinates::{LatitudeMicrodegrees, LongitudeMicrodegrees};

const AXIAL_TILT_RADIANS: f64 = 23.44_f64.to_radians();

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialDirections {
    /// Unit vector from the observer toward the Sun in east/up/north axes.
    pub sun: [f32; 3],
    /// Unit vector from the observer toward the Moon in east/up/north axes.
    pub moon: [f32; 3],
    /// Canonical cycle fraction: new=0, first quarter=.25, full=.5.
    pub lunar_phase: f32,
    pub lunar_illumination: f32,
}

/// Resolve Sun and Moon directions for the deterministic 365-day game
/// calendar. Longitude is interpreted east-positive and the authoritative
/// clock is treated as UTC for presentation.
pub fn celestial_directions(
    absolute_minute: u64,
    latitude: LatitudeMicrodegrees,
    longitude: LongitudeMicrodegrees,
) -> CelestialDirections {
    celestial_directions_with_phase(absolute_minute, absolute_minute, latitude, longitude)
}

/// Resolve celestial directions while allowing journey-local time of day to
/// advance independently from the canonical lunar phase.
pub fn celestial_directions_with_phase(
    absolute_minute: u64,
    lunar_phase_minute: u64,
    latitude: LatitudeMicrodegrees,
    longitude: LongitudeMicrodegrees,
) -> CelestialDirections {
    let latitude = latitude.degrees().to_radians();
    let longitude_degrees = longitude.degrees();
    // The strategic clock is already a minute offset into its canonical
    // 365-day year (WORLD_START_MINUTE is August 20), so no epoch offset is
    // applied a second time here.
    let day_of_year =
        (absolute_minute as f64 / MINUTES_PER_DAY as f64).rem_euclid(DAYS_PER_YEAR as f64);
    let local_minutes = (absolute_minute % MINUTES_PER_DAY) as f64 + longitude_degrees * 4.0;
    let solar_hour_angle = ((local_minutes / 60.0 - 12.0) * 15.0).to_radians();

    // Standard low-cost solar declination approximation. Accuracy is more
    // than sufficient for lighting and the apparent day length of a scene.
    let seasonal_angle = std::f64::consts::TAU * (day_of_year + 284.0) / DAYS_PER_YEAR as f64;
    let solar_declination = AXIAL_TILT_RADIANS
        .sin()
        .mul_add(seasonal_angle.sin(), 0.0)
        .asin();
    let sun = equatorial_to_horizon(latitude, solar_declination, solar_hour_angle);

    let phase = lunar_phase(lunar_phase_minute);
    let lunar_hour_angle = solar_hour_angle + std::f64::consts::TAU * phase;
    // Shifting the seasonal ecliptic angle by phase keeps new moons near the
    // Sun and full moons opposite it without the cost of a full ephemeris.
    let lunar_declination =
        (AXIAL_TILT_RADIANS.sin() * (seasonal_angle + std::f64::consts::TAU * phase).sin()).asin();
    let moon = equatorial_to_horizon(latitude, lunar_declination, lunar_hour_angle);

    CelestialDirections {
        sun,
        moon,
        lunar_phase: phase as f32,
        lunar_illumination: lunar_illumination(phase) as f32,
    }
}

fn equatorial_to_horizon(latitude: f64, declination: f64, hour_angle: f64) -> [f32; 3] {
    let east = -declination.cos() * hour_angle.sin();
    let up =
        latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * hour_angle.cos();
    let north =
        latitude.cos() * declination.sin() - latitude.sin() * declination.cos() * hour_angle.cos();
    let length = east.hypot(up).hypot(north);
    [
        (east / length) as f32,
        (up / length) as f32,
        (north / length) as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn altitude(direction: [f32; 3]) -> f32 {
        direction[1].asin().to_degrees()
    }

    #[test]
    fn northern_summer_noon_is_higher_than_winter_noon() {
        let longitude = LongitudeMicrodegrees::ZERO;
        let latitude = LatitudeMicrodegrees::from_degrees(53.5).unwrap();
        let june_noon =
            celestial_directions((172 * MINUTES_PER_DAY) + 12 * 60, latitude, longitude);
        let december_noon =
            celestial_directions((355 * MINUTES_PER_DAY) + 12 * 60, latitude, longitude);
        assert!(altitude(june_noon.sun) > altitude(december_noon.sun));
    }

    #[test]
    fn full_moon_is_opposite_the_sun_and_illuminated() {
        let minute = crate::strategic_time::LUNAR_CYCLE_MINUTES / 2;
        let sky = celestial_directions(
            minute,
            LatitudeMicrodegrees::from_degrees(53.5).unwrap(),
            LongitudeMicrodegrees::from_degrees(10.0).unwrap(),
        );
        let dot = sky.sun[0] * sky.moon[0] + sky.sun[1] * sky.moon[1] + sky.sun[2] * sky.moon[2];
        assert!(dot < -0.9, "full moon should oppose the Sun: {dot}");
        assert!((sky.lunar_illumination - 1.0).abs() < 0.001);
    }

    #[test]
    fn longitude_shifts_apparent_solar_time() {
        let utc_noon = celestial_directions(
            12 * 60,
            LatitudeMicrodegrees::ZERO,
            LongitudeMicrodegrees::ZERO,
        );
        let east_noon = celestial_directions(
            12 * 60,
            LatitudeMicrodegrees::ZERO,
            LongitudeMicrodegrees::from_degrees(30.0).unwrap(),
        );
        assert!(utc_noon.sun[1] > east_noon.sun[1]);
        assert!(east_noon.sun[0] < 0.0);
    }

    #[test]
    fn journey_time_can_move_the_sky_without_advancing_the_lunar_phase() {
        let phase_anchor = 12_345;
        let latitude = LatitudeMicrodegrees::from_degrees(53.5).unwrap();
        let longitude = LongitudeMicrodegrees::from_degrees(10.0).unwrap();
        let first = celestial_directions_with_phase(720, phase_anchor, latitude, longitude);
        let later = celestial_directions_with_phase(1_080, phase_anchor, latitude, longitude);
        assert_eq!(first.lunar_phase, later.lunar_phase);
        assert_eq!(first.lunar_illumination, later.lunar_illumination);
        assert_ne!(first.sun, later.sun);
        assert_ne!(first.moon, later.moon);
    }
}
