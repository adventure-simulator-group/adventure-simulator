//! Versioned, deterministic strategic weather.
//!
//! Weather is evaluated from authoritative absolute time and position.  It is
//! intentionally calculation-only: callers snapshot the result when a stable
//! journey or incident needs to outlive later rules changes.

use serde::{Deserialize, Serialize};

pub const WEATHER_RULES_VERSION: u16 = 3;
/// One domain seed shared by every authoritative and player-visible weather
/// query in the Fabelgeist world.
pub const WORLD_WEATHER_SEED: u64 = 0x4144_5645_4e54_5552;
pub const WEATHER_INTERVAL_MINUTES: u64 = 360;
pub const WEATHER_CELL_MICRODEGREES: i32 = 250_000;
const HISTORY_INTERVALS: u64 = 16;
const FIELD_FRACTION: i64 = 65_536;
const SYNOPTIC_SPATIAL_CELLS: i64 = 8;
const SYNOPTIC_TIME_INTERVALS: i64 = 4;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precipitation {
    #[default]
    Clear,
    Rain,
    Snow,
}

/// Observable cloud form diagnosed from the atmospheric state.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudForm {
    Cirrus,
    Cirrocumulus,
    Cirrostratus,
    Altocumulus,
    Altostratus,
    Nimbostratus,
    Stratocumulus,
    #[default]
    Stratus,
    Cumulus,
    CumulusCongestus,
    Cumulonimbus,
}

/// One diagnosed cloud deck. Heights are metres above local ground level.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CloudLayerSnapshot {
    pub form: CloudForm,
    pub coverage_bps: u16,
    pub optical_density_bps: u16,
    pub base_metres: u16,
    pub top_metres: u16,
}

/// Atmospheric conditions needed to explain the visible weather.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct AtmosphericSnapshot {
    pub relative_humidity_bps: u16,
    pub dew_point_deci_c: i32,
    /// Sea-level pressure in tenths of a hectopascal.
    pub sea_level_pressure_deci_hpa: u16,
    /// Direction the near-surface wind travels toward, clockwise from north.
    pub wind_direction_degrees: u16,
    pub wind_shear_bps: u16,
    pub instability_bps: u16,
    /// Broad ascent or subsidence, on a -10,000..=10,000 scale.
    pub lift_bps: i16,
    pub low_cloud: Option<CloudLayerSnapshot>,
    pub middle_cloud: Option<CloudLayerSnapshot>,
    pub high_cloud: Option<CloudLayerSnapshot>,
}

impl Default for AtmosphericSnapshot {
    fn default() -> Self {
        Self {
            relative_humidity_bps: 5_000,
            dew_point_deci_c: 0,
            sea_level_pressure_deci_hpa: 10_132,
            wind_direction_degrees: 0,
            wind_shear_bps: 0,
            instability_bps: 0,
            lift_bps: 0,
            low_cloud: None,
            middle_cloud: None,
            high_cloud: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WeatherSnapshot {
    pub rules_version: u16,
    pub interval_start_minute: u64,
    pub cell_latitude: i32,
    pub cell_longitude: i32,
    /// Ambient air temperature in tenths of a degree Celsius.
    pub temperature_deci_c: i32,
    /// Deterministic wind speed on a 0..=10,000 strategic scale.
    pub wind_speed_bps: u16,
    pub precipitation: Precipitation,
    /// Current precipitation intensity, in basis points.
    pub intensity_bps: u16,
    /// Antecedent liquid ground moisture, in basis points.
    pub ground_moisture_bps: u16,
    /// Established snow cover, in basis points.
    pub snow_cover_bps: u16,
    pub atmosphere: AtmosphericSnapshot,
}

impl Default for WeatherSnapshot {
    fn default() -> Self {
        Self {
            rules_version: WEATHER_RULES_VERSION,
            interval_start_minute: 0,
            cell_latitude: 0,
            cell_longitude: 0,
            temperature_deci_c: 100,
            wind_speed_bps: 0,
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
            ground_moisture_bps: 0,
            snow_cover_bps: 0,
            atmosphere: AtmosphericSnapshot::default(),
        }
    }
}

impl WeatherSnapshot {
    pub const fn is_precipitating(self) -> bool {
        !matches!(self.precipitation, Precipitation::Clear)
    }

    pub const fn qualitative_ground(self) -> &'static str {
        if self.snow_cover_bps >= 6_000 {
            "Deep snow"
        } else if self.snow_cover_bps >= 1_500 {
            "Snow-covered"
        } else if self.ground_moisture_bps >= 7_000 {
            "Waterlogged"
        } else if self.ground_moisture_bps >= 3_000 {
            "Muddy"
        } else if self.ground_moisture_bps >= 800 {
            "Damp"
        } else {
            "Dry"
        }
    }

    pub fn cloud_layers(self) -> impl Iterator<Item = CloudLayerSnapshot> {
        [
            self.atmosphere.low_cloud,
            self.atmosphere.middle_cloud,
            self.atmosphere.high_cloud,
        ]
        .into_iter()
        .flatten()
    }
}

/// Evaluate the strategic weather authority.
///
/// Coordinates are signed decimal microdegrees and elevation is metres above
/// sea level. `world_seed` and the rules version domain-separate worlds and
/// later algorithms without storing per-cell simulation rows.
pub fn weather_at(
    world_seed: u64,
    absolute_minute: u64,
    latitude_microdegrees: i32,
    longitude_microdegrees: i32,
    elevation_m: i16,
) -> WeatherSnapshot {
    let cell_latitude = latitude_microdegrees.div_euclid(WEATHER_CELL_MICRODEGREES);
    let cell_longitude = longitude_microdegrees.div_euclid(WEATHER_CELL_MICRODEGREES);
    let interval = absolute_minute / WEATHER_INTERVAL_MINUTES;
    let current = interval_weather(
        world_seed,
        interval,
        cell_latitude,
        cell_longitude,
        elevation_m,
    );
    let mut moisture = 0u32;
    let mut snow = 0u32;
    let available_history = HISTORY_INTERVALS.min(interval.saturating_add(1));
    for age in (0..available_history).rev() {
        let sample = interval_weather(
            world_seed,
            interval.saturating_sub(age),
            cell_latitude,
            cell_longitude,
            elevation_m,
        );
        (moisture, snow) = advance_ground(moisture, snow, sample, sample.temperature_deci_c);
    }
    WeatherSnapshot {
        rules_version: WEATHER_RULES_VERSION,
        interval_start_minute: interval * WEATHER_INTERVAL_MINUTES,
        cell_latitude,
        cell_longitude,
        temperature_deci_c: current.temperature_deci_c,
        wind_speed_bps: current.wind_speed_bps,
        precipitation: current.precipitation,
        intensity_bps: current.intensity_bps,
        ground_moisture_bps: moisture.min(10_000) as u16,
        snow_cover_bps: snow.min(10_000) as u16,
        atmosphere: current.atmosphere,
    }
}

fn advance_ground(
    mut moisture: u32,
    mut snow: u32,
    sample: IntervalWeather,
    temperature_deci_c: i32,
) -> (u32, u32) {
    // Liquid drains faster than established snow melts.
    moisture = moisture.saturating_mul(3) / 4;
    snow = snow.saturating_mul(7) / 8;
    match sample.precipitation {
        Precipitation::Rain => {
            moisture = moisture.saturating_add(u32::from(sample.intensity_bps) / 3);
            // Warm rain accelerates thaw.
            snow = snow.saturating_mul(3) / 4;
        }
        Precipitation::Snow => {
            snow = snow.saturating_add(u32::from(sample.intensity_bps) / 3);
        }
        Precipitation::Clear => {
            if temperature_deci_c > 20 {
                snow = snow.saturating_mul(7) / 8;
            }
        }
    }
    (moisture, snow)
}

#[derive(Clone, Copy)]
struct IntervalWeather {
    temperature_deci_c: i32,
    precipitation: Precipitation,
    intensity_bps: u16,
    wind_speed_bps: u16,
    atmosphere: AtmosphericSnapshot,
}

fn interval_weather(
    world_seed: u64,
    interval: u64,
    cell_latitude: i32,
    cell_longitude: i32,
    elevation_m: i16,
) -> IntervalWeather {
    let pressure_field = correlated_field(
        world_seed,
        0x5052_4553,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let moisture_field = correlated_field(
        world_seed,
        0x4d4f_4953,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let middle_moisture = correlated_field(
        world_seed,
        0x4d49_444d,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let upper_moisture = correlated_field(
        world_seed,
        0x4849_4748,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let convective_field = correlated_field(
        world_seed,
        0x434f_4e56,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let occurrence_field = correlated_field(
        world_seed,
        0x5241_494e,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let temperature_deci_c = temperature_deci_c(
        world_seed,
        interval,
        cell_latitude,
        cell_longitude,
        elevation_m,
    );
    let low_pressure_bps = 10_000u16.saturating_sub(pressure_field);
    let relative_humidity_bps = (3_200u32
        + u32::from(moisture_field) * 5_000 / 10_000
        + u32::from(low_pressure_bps) * 1_800 / 10_000)
        .min(10_000) as u16;
    let dew_point_deci_c =
        temperature_deci_c - i32::from(10_000u16.saturating_sub(relative_humidity_bps)) / 50;
    let sea_level_pressure_deci_hpa =
        (10_132 + (i32::from(pressure_field) - 5_000) * 250 / 5_000) as u16;

    let pressure_west = correlated_field(
        world_seed,
        0x5052_4553,
        interval,
        cell_latitude,
        cell_longitude - 1,
    );
    let pressure_east = correlated_field(
        world_seed,
        0x5052_4553,
        interval,
        cell_latitude,
        cell_longitude + 1,
    );
    let pressure_south = correlated_field(
        world_seed,
        0x5052_4553,
        interval,
        cell_latitude - 1,
        cell_longitude,
    );
    let pressure_north = correlated_field(
        world_seed,
        0x5052_4553,
        interval,
        cell_latitude + 1,
        cell_longitude,
    );
    let east_gradient = i32::from(pressure_east) - i32::from(pressure_west);
    let north_gradient = i32::from(pressure_north) - i32::from(pressure_south);
    let pressure_gradient =
        (east_gradient.unsigned_abs() + north_gradient.unsigned_abs()).min(10_000);
    let wind_direction_degrees = wind_direction(east_gradient, north_gradient);
    let wind_speed_bps =
        (700u32 + pressure_gradient * 5 + u32::from(low_pressure_bps) / 5).min(10_000) as u16;
    let shear_field = correlated_field(
        world_seed,
        0x5348_4541,
        interval,
        cell_latitude,
        cell_longitude,
    );
    let wind_shear_bps = ((u32::from(shear_field) + pressure_gradient * 3) / 4).min(10_000) as u16;

    let hour = interval * WEATHER_INTERVAL_MINUTES / 60 % 24;
    let daylight_heating = triangle_wave_bps((hour + 21) % 24, 24).max(0) as u32;
    let instability_bps = (u32::from(convective_field) * 5 / 10
        + u32::from(relative_humidity_bps) * 2 / 10
        + daylight_heating * 3 / 10)
        .min(10_000) as u16;
    let frontal_lift = (pressure_gradient as i32 * 3).min(10_000);
    let lift_bps =
        ((i32::from(low_pressure_bps) - 4_700) * 2 + frontal_lift).clamp(-10_000, 10_000) as i16;
    let lcl_metres =
        ((temperature_deci_c - dew_point_deci_c).max(0) * 25 / 2).clamp(120, 2_500) as u16;

    let mut low_cloud =
        diagnose_low_cloud(relative_humidity_bps, instability_bps, lift_bps, lcl_metres);
    let mut middle_cloud = diagnose_middle_cloud(
        middle_moisture,
        relative_humidity_bps,
        instability_bps,
        lift_bps,
    );
    let high_cloud = diagnose_high_cloud(upper_moisture, wind_shear_bps, lift_bps);
    let precipitating_cloud = low_cloud.is_some_and(|layer| {
        matches!(layer.form, CloudForm::Cumulonimbus) && layer.coverage_bps >= 2_000
    }) || middle_cloud
        .is_some_and(|layer| matches!(layer.form, CloudForm::Nimbostratus));
    let cloud_water = low_cloud
        .into_iter()
        .chain(middle_cloud)
        .map(|layer| u32::from(layer.coverage_bps) + u32::from(layer.optical_density_bps))
        .max()
        .unwrap_or(0);
    let precipitation_signal = u32::from(relative_humidity_bps)
        + u32::from(lift_bps.max(0) as u16) / 2
        + cloud_water / 2
        + u32::from(occurrence_field) / 3;
    let is_precipitating = precipitating_cloud && precipitation_signal >= 15_000;
    let intensity_bps = if is_precipitating {
        ((precipitation_signal - 14_000) * 2).clamp(1_000, 10_000) as u16
    } else {
        0
    };
    let precipitation = if intensity_bps == 0 {
        Precipitation::Clear
    } else if temperature_deci_c <= 0 {
        Precipitation::Snow
    } else {
        Precipitation::Rain
    };
    if intensity_bps > 0 {
        if low_cloud.is_none() {
            low_cloud = Some(cloud_layer(
                CloudForm::Stratus,
                7_500,
                7_000,
                lcl_metres,
                lcl_metres.saturating_add(750),
            ));
        }
        if !low_cloud.is_some_and(|layer| matches!(layer.form, CloudForm::Cumulonimbus)) {
            middle_cloud = Some(cloud_layer(
                CloudForm::Nimbostratus,
                8_000u16.saturating_add(intensity_bps / 5).min(10_000),
                7_500u16.saturating_add(intensity_bps / 4).min(10_000),
                1_800,
                5_800,
            ));
        }
    }

    IntervalWeather {
        temperature_deci_c,
        precipitation,
        intensity_bps,
        wind_speed_bps,
        atmosphere: AtmosphericSnapshot {
            relative_humidity_bps,
            dew_point_deci_c,
            sea_level_pressure_deci_hpa,
            wind_direction_degrees,
            wind_shear_bps,
            instability_bps,
            lift_bps,
            low_cloud,
            middle_cloud,
            high_cloud,
        },
    }
}

fn diagnose_low_cloud(
    humidity: u16,
    instability: u16,
    lift: i16,
    base_metres: u16,
) -> Option<CloudLayerSnapshot> {
    let signal =
        u32::from(humidity) + u32::from(lift.max(0) as u16) / 2 + u32::from(instability) / 4;
    let coverage = coverage_from_signal(signal, 7_200, 6_500);
    if coverage == 0 {
        return None;
    }
    let (form, top_metres, density) = if instability >= 7_700 && lift >= 1_500 {
        (
            CloudForm::Cumulonimbus,
            base_metres.saturating_add(9_000).min(12_000),
            8_500,
        )
    } else if instability >= 6_000 && lift >= 500 {
        (
            CloudForm::CumulusCongestus,
            base_metres.saturating_add(4_500),
            6_500,
        )
    } else if humidity >= 8_500 && instability < 4_500 {
        (CloudForm::Stratus, base_metres.saturating_add(650), 5_500)
    } else if coverage >= 5_500 {
        (
            CloudForm::Stratocumulus,
            base_metres.saturating_add(1_200),
            5_000,
        )
    } else {
        (
            CloudForm::Cumulus,
            base_metres.saturating_add(1_600 + instability / 3),
            4_500,
        )
    };
    Some(cloud_layer(
        form,
        coverage,
        density,
        base_metres,
        top_metres,
    ))
}

fn diagnose_middle_cloud(
    middle_moisture: u16,
    surface_humidity: u16,
    instability: u16,
    lift: i16,
) -> Option<CloudLayerSnapshot> {
    let signal = u32::from(middle_moisture)
        + u32::from(lift.max(0) as u16) / 3
        + u32::from(surface_humidity) / 6;
    let coverage = coverage_from_signal(signal, 6_400, 5_500);
    if coverage == 0 {
        return None;
    }
    let form = if lift >= 3_000 && surface_humidity >= 8_000 && coverage >= 6_500 {
        CloudForm::Nimbostratus
    } else if instability >= 5_400 {
        CloudForm::Altocumulus
    } else {
        CloudForm::Altostratus
    };
    let density = if matches!(form, CloudForm::Nimbostratus) {
        8_000
    } else {
        3_500 + coverage / 3
    };
    Some(cloud_layer(form, coverage, density, 2_600, 5_400))
}

fn diagnose_high_cloud(
    upper_moisture: u16,
    wind_shear: u16,
    lift: i16,
) -> Option<CloudLayerSnapshot> {
    let signal = u32::from(upper_moisture) + u32::from(lift.max(0) as u16) / 5;
    let coverage = coverage_from_signal(signal, 5_700, 5_000);
    if coverage == 0 {
        return None;
    }
    let form = if coverage >= 7_000 {
        CloudForm::Cirrostratus
    } else if wind_shear >= 5_000 {
        CloudForm::Cirrus
    } else {
        CloudForm::Cirrocumulus
    };
    Some(cloud_layer(
        form,
        coverage,
        2_000 + coverage / 4,
        6_200,
        10_500,
    ))
}

const fn cloud_layer(
    form: CloudForm,
    coverage_bps: u16,
    optical_density_bps: u16,
    base_metres: u16,
    top_metres: u16,
) -> CloudLayerSnapshot {
    CloudLayerSnapshot {
        form,
        coverage_bps,
        optical_density_bps,
        base_metres,
        top_metres,
    }
}

fn coverage_from_signal(signal: u32, onset: u32, span: u32) -> u16 {
    if signal <= onset {
        0
    } else {
        ((signal - onset) * 10_000 / span).min(10_000) as u16
    }
}

fn temperature_deci_c(
    world_seed: u64,
    interval: u64,
    cell_latitude: i32,
    cell_longitude: i32,
    elevation_m: i16,
) -> i32 {
    let day = (interval * WEATHER_INTERVAL_MINUTES / 1_440) % 365;
    let hour = (interval * WEATHER_INTERVAL_MINUTES / 60) % 24;
    let latitude_degrees = cell_latitude * WEATHER_CELL_MICRODEGREES / 1_000_000;
    let mean = 95 - (latitude_degrees.abs() - 53).abs() * 4;
    let seasonal = triangle_wave_bps((day + 343) % 365, 365) * 105 / 10_000;
    let diurnal = triangle_wave_bps((hour + 21) % 24, 24) * 25 / 10_000;
    let synoptic = (i32::from(correlated_field(
        world_seed,
        0x5445_4d50,
        interval,
        cell_latitude,
        cell_longitude,
    )) - 5_000)
        * 45
        / 5_000;
    let elevation_penalty = i32::from(elevation_m.max(0)) * 65 / 1_000;
    mean + seasonal + diurnal + synoptic - elevation_penalty
}

fn triangle_wave_bps(phase: u64, period: u64) -> i32 {
    let doubled = phase % period * 20_000 / period;
    if doubled <= 10_000 {
        doubled as i32 * 2 - 10_000
    } else {
        30_000 - doubled as i32 * 2
    }
}

fn wind_direction(east_gradient: i32, north_gradient: i32) -> u16 {
    if east_gradient == 0 && north_gradient == 0 {
        return 0;
    }
    let radians = (-(east_gradient as f64)).atan2(north_gradient as f64);
    radians.to_degrees().rem_euclid(360.0).round() as u16 % 360
}

/// Smooth deterministic field with synoptic-scale spatial correlation,
/// eastward advection, and day-scale evolution.
fn correlated_field(seed: u64, domain: u64, interval: u64, lat: i32, lon: i32) -> u16 {
    let interval = interval.min(i64::MAX as u64 / (4 * FIELD_FRACTION as u64)) as i64;
    let x = i64::from(lon) * FIELD_FRACTION - interval * 4 * FIELD_FRACTION;
    let y = i64::from(lat) * FIELD_FRACTION + interval * FIELD_FRACTION;
    let t = interval * FIELD_FRACTION / SYNOPTIC_TIME_INTERVALS;
    let spatial_scale = SYNOPTIC_SPATIAL_CELLS * FIELD_FRACTION;
    let x0 = x.div_euclid(spatial_scale);
    let y0 = y.div_euclid(spatial_scale);
    let t0 = t.div_euclid(FIELD_FRACTION);
    let xf = fade_fraction(x.rem_euclid(spatial_scale), spatial_scale);
    let yf = fade_fraction(y.rem_euclid(spatial_scale), spatial_scale);
    let tf = fade_fraction(t.rem_euclid(FIELD_FRACTION), FIELD_FRACTION);
    let sample = |dx: i64, dy: i64, dt: i64| {
        (weather_hash(seed ^ domain, t0 + dt, y0 + dy, x0 + dx) % 10_001) as u16
    };
    let lower = lerp_bps(
        lerp_bps(sample(0, 0, 0), sample(1, 0, 0), xf),
        lerp_bps(sample(0, 1, 0), sample(1, 1, 0), xf),
        yf,
    );
    let upper = lerp_bps(
        lerp_bps(sample(0, 0, 1), sample(1, 0, 1), xf),
        lerp_bps(sample(0, 1, 1), sample(1, 1, 1), xf),
        yf,
    );
    lerp_bps(lower, upper, tf)
}

fn fade_fraction(remainder: i64, scale: i64) -> u32 {
    let fraction = (remainder as u64 * 65_535 / scale as u64) as u32;
    let f = u64::from(fraction);
    ((f * f * (3 * 65_535 - 2 * f)) / (65_535 * 65_535)) as u32
}

fn lerp_bps(a: u16, b: u16, fraction: u32) -> u16 {
    let a = i64::from(a);
    let delta = i64::from(b) - a;
    (a + delta * i64::from(fraction) / 65_535).clamp(0, 10_000) as u16
}

fn weather_hash(seed: u64, interval: i64, lat: i64, lon: i64) -> u64 {
    let mut value = seed
        ^ (u64::from(WEATHER_RULES_VERSION) << 48)
        ^ (interval as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (lat as u64).rotate_left(17)
        ^ (lon as u64).rotate_left(39);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Supplement an underlying terrain check with the Snow overlay.
pub fn snow_overlay_check(underlying_bps: u16, snow_bps: u16, cover_bps: u16) -> u16 {
    let cover = u32::from(cover_bps.min(10_000));
    (((u32::from(underlying_bps) * (10_000 - cover)) + (u32::from(snow_bps) * cover)) / 10_000)
        as u16
}

/// Split travel practice without replacing the underlying biome exposure.
pub fn snow_training_exposure(
    travel_minutes: u32,
    snow_cover_bps: u16,
    road_discount_bps: u16,
) -> (u32, u32) {
    let discounted = u64::from(travel_minutes)
        * u64::from(10_000u16.saturating_sub(road_discount_bps.min(10_000)))
        / 10_000;
    let snow_minutes = discounted * u64::from(snow_cover_bps.min(10_000)) / 10_000;
    ((discounted - snow_minutes) as u32, snow_minutes as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_is_stable_within_an_interval_and_cells_are_coarse() {
        let a = weather_at(42, 123_456, 53_551_000, 9_993_000, 10);
        assert_eq!(a, weather_at(42, 123_456, 53_551_000, 9_993_000, 10));
        assert_eq!(a, weather_at(42, 123_457, 53_599_999, 9_999_999, 10));
        assert_eq!(a.rules_version, WEATHER_RULES_VERSION);
        assert!((-800..=500).contains(&a.temperature_deci_c));
        assert!(a.wind_speed_bps <= 10_000);
    }

    #[test]
    fn epoch_start_samples_interval_zero_once() {
        let snapshot = weather_at(42, 0, 53_000_000, 10_000_000, 0);
        let sample = interval_weather(42, 0, snapshot.cell_latitude, snapshot.cell_longitude, 0);
        let (moisture, snow) = advance_ground(0, 0, sample, sample.temperature_deci_c);
        assert_eq!(snapshot.ground_moisture_bps, moisture.min(10_000) as u16);
        assert_eq!(snapshot.snow_cover_bps, snow.min(10_000) as u16);
    }

    #[test]
    fn elevation_and_season_control_rain_versus_snow() {
        let winter_interval = 30 * 4;
        let summer_interval = 180 * 4;
        assert!(
            temperature_deci_c(7, winter_interval, 212, 40, 0)
                < temperature_deci_c(7, summer_interval, 212, 40, 0)
        );
        assert!(
            temperature_deci_c(7, summer_interval, 212, 40, 2_000)
                < temperature_deci_c(7, summer_interval, 212, 40, 0)
        );
        let mut found = false;
        for interval in 0..1_460 {
            let low = interval_weather(7, interval, 212, 40, 0);
            let high = interval_weather(7, interval, 212, 40, 2_000);
            if low.precipitation == Precipitation::Rain && high.precipitation == Precipitation::Snow
            {
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn initialized_world_uses_late_summer_temperature() {
        let interval = crate::strategic_time::WORLD_START_MINUTE / WEATHER_INTERVAL_MINUTES;
        let day = interval * WEATHER_INTERVAL_MINUTES / 1_440 % 365;
        assert_eq!(day, 231);
        assert!(temperature_deci_c(7, interval, 214, 40, 0) > 100);
    }

    #[test]
    fn moisture_and_snow_are_bounded_and_have_memory() {
        for minute in (0..525_600).step_by(360) {
            let sample = weather_at(19, minute, 53_500_000, 10_000_000, 80);
            assert!(sample.ground_moisture_bps <= 10_000);
            assert!(sample.snow_cover_bps <= 10_000);
        }
    }

    #[test]
    fn synoptic_fields_are_spatially_and_temporally_coherent() {
        let interval = 500;
        let center = interval_weather(19, interval, 214, 40, 80);
        let neighbor = interval_weather(19, interval, 214, 41, 80);
        let next = interval_weather(19, interval + 1, 214, 40, 80);
        assert!(
            center
                .atmosphere
                .sea_level_pressure_deci_hpa
                .abs_diff(neighbor.atmosphere.sea_level_pressure_deci_hpa)
                < 150
        );
        assert!(
            center
                .atmosphere
                .sea_level_pressure_deci_hpa
                .abs_diff(next.atmosphere.sea_level_pressure_deci_hpa)
                < 200
        );
    }

    #[test]
    fn precipitation_is_backed_by_saturation_lift_and_a_precipitating_cloud() {
        let mut found = 0;
        for interval in 0..4_000 {
            let sample = interval_weather(23, interval, 214, 40, 20);
            if sample.precipitation == Precipitation::Clear {
                continue;
            }
            found += 1;
            assert!(sample.atmosphere.relative_humidity_bps >= 7_000);
            assert!(sample.atmosphere.lift_bps > 0);
            assert!(
                sample.atmosphere.low_cloud.is_some() || sample.atmosphere.middle_cloud.is_some()
            );
        }
        assert!(found > 0);
    }

    #[test]
    fn diagnostic_clouds_cover_low_middle_and_high_families() {
        let mut low = false;
        let mut middle = false;
        let mut high = false;
        for interval in 0..2_000 {
            let sample = interval_weather(29, interval, 214, 40, 20);
            low |= sample.atmosphere.low_cloud.is_some();
            middle |= sample.atmosphere.middle_cloud.is_some();
            high |= sample.atmosphere.high_cloud.is_some();
        }
        assert!(low && middle && high);
    }

    #[test]
    fn rain_drains_and_snow_accumulates_then_melts() {
        let rain = IntervalWeather {
            temperature_deci_c: 50,
            precipitation: Precipitation::Rain,
            intensity_bps: 9_000,
            wind_speed_bps: 0,
            atmosphere: AtmosphericSnapshot::default(),
        };
        let clear = IntervalWeather {
            temperature_deci_c: 50,
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
            wind_speed_bps: 0,
            atmosphere: AtmosphericSnapshot::default(),
        };
        let snowfall = IntervalWeather {
            temperature_deci_c: -20,
            precipitation: Precipitation::Snow,
            intensity_bps: 9_000,
            wind_speed_bps: 0,
            atmosphere: AtmosphericSnapshot::default(),
        };
        let (wet, _) = advance_ground(0, 0, rain, 50);
        let (drained, _) = advance_ground(wet, 0, clear, 50);
        assert!(wet > 0 && drained < wet);
        let (_, first_snow) = advance_ground(0, 0, snowfall, -20);
        let (_, accumulated) = advance_ground(0, first_snow, snowfall, -20);
        let (_, melted) = advance_ground(0, accumulated, clear, 50);
        assert!(accumulated > first_snow);
        assert!(melted < accumulated);
    }

    #[test]
    fn snow_overlay_and_training_are_monotonic_and_keep_underlying_exposure() {
        assert_eq!(snow_overlay_check(3_000, 5_000, 0), 3_000);
        assert!(snow_overlay_check(3_000, 5_000, 8_000) > 3_000);
        let (underlying, snow) = snow_training_exposure(600, 8_000, 2_500);
        assert_eq!(underlying, 90);
        assert_eq!(snow, 360);
        let chunks = (0..6)
            .map(|_| snow_training_exposure(100, 8_000, 2_500))
            .fold((0, 0), |sum, value| (sum.0 + value.0, sum.1 + value.1));
        assert_eq!((underlying, snow), chunks);
    }
}
