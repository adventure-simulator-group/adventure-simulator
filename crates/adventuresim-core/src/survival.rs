//! Deterministic strategic exposure, wetness, and thermal strain.
//!
//! The pure reducer advances one integer minute at a time. Calling it for a
//! whole interval or for any partition of that interval therefore produces the
//! same result, provided the same minute snapshots are supplied.

use crate::weather::{Precipitation, WeatherSnapshot};
use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;

pub const MAX_WETNESS_BPS: u16 = BASIS_POINTS_PER_WHOLE;
pub const MAX_THERMAL_STRAIN: i32 = 10_000;
pub const LEATHER_WEATHERPROOF_RESISTANCE: f32 = 20.0;
pub const PADDING_INSULATION_BPS_PER_POINT: f32 = 80.0;
pub const MAX_CLOTHING_INSULATION_BPS: u16 = 8_000;
pub const COLD_STAGGER_STRAIN: i32 = -5_000;
pub const COLD_INCAPACITATION_STRAIN: i32 = -9_000;
pub const HEAT_STAGGER_STRAIN: i32 = 5_000;
pub const HEAT_INCAPACITATION_STRAIN: i32 = 9_000;
pub const FROSTBITE_STRAIN_THRESHOLD: i32 = -7_500;
pub const FROSTBITE_EXPOSURE_MINUTES_PER_DAMAGE: u32 = 360;
pub const FROSTBITE_DAMAGE_PER_THRESHOLD: f32 = 0.01;
pub const IMMERSION_WETNESS_BPS: u16 = 9_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurvivalState {
    pub wetness_bps: u16,
    /// Negative is cold strain; positive is heat strain.
    pub thermal_strain: i32,
    /// Sustained peripheral cold exposure not yet converted to injury.
    pub frostbite_progress_minutes: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClothingExposure {
    pub insulation_bps: u16,
    /// Fraction of the stable body regions protected by an outer
    /// leather-equivalent shell. Resistance above leather is capped.
    pub weatherproofing_bps: u16,
    /// Arms then legs (left, right), used for deterministic frostbite
    /// targeting. Each value combines the outer layer's coverage and capped
    /// resistance.
    pub peripheral_protection_bps: [u16; 4],
}

/// Shelter available at an outdoor field camp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(rename_all = "snake_case")]
pub enum FieldShelter {
    #[default]
    Bivouac,
    Tent,
}

/// The environment governing weather exposure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExposureShelter {
    Field(FieldShelter),
    /// A settlement building protects occupants from ambient thermal loading
    /// as well as precipitation and wind.
    Indoor,
}

impl Default for ExposureShelter {
    fn default() -> Self {
        Self::Field(FieldShelter::default())
    }
}

impl ExposureShelter {
    pub const fn blocks_rain(self) -> bool {
        matches!(self, Self::Field(FieldShelter::Tent) | Self::Indoor)
    }

    pub const fn wind_multiplier_bps(self) -> u16 {
        match self {
            Self::Field(FieldShelter::Bivouac) => BASIS_POINTS_PER_WHOLE,
            Self::Field(FieldShelter::Tent) => 2_500,
            Self::Indoor => 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExposureOutcome {
    pub state: SurvivalState,
    /// Zero-based minute offsets at which peripheral damage thresholds cross.
    /// Callers can replay these at canonical absolute times, independent of
    /// how the enclosing strategic interval was partitioned.
    pub frostbite_event_offsets: Vec<u64>,
}

pub fn insulation_from_layers(layers: impl IntoIterator<Item = (f32, f32)>) -> u16 {
    let insulation = layers
        .into_iter()
        .map(|(padding, coverage)| padding.max(0.0) * coverage.clamp(0.0, 1.0))
        .sum::<f32>()
        * PADDING_INSULATION_BPS_PER_POINT;
    insulation.clamp(0.0, f32::from(MAX_CLOTHING_INSULATION_BPS)) as u16
}

pub fn is_weatherproof_outer_layer(resistance: f32) -> bool {
    resistance.is_finite() && resistance >= LEATHER_WEATHERPROOF_RESISTANCE
}

pub fn weatherproofing_from_outer_layer(resistance: f32, coverage: f32) -> u16 {
    if !resistance.is_finite() || !coverage.is_finite() {
        return 0;
    }
    (resistance.max(0.0) / LEATHER_WEATHERPROOF_RESISTANCE)
        .min(1.0)
        .mul_add(coverage.clamp(0.0, 1.0), 0.0)
        .mul_add(f32::from(BASIS_POINTS_PER_WHOLE), 0.0)
        .round() as u16
}

pub fn apply_immersion(mut state: SurvivalState) -> SurvivalState {
    state.wetness_bps = state.wetness_bps.max(IMMERSION_WETNESS_BPS);
    state
}

/// Advance exposure over weather snapshots for consecutive absolute minutes.
pub fn advance_exposure(
    mut state: SurvivalState,
    weather: impl IntoIterator<Item = WeatherSnapshot>,
    clothing: ClothingExposure,
    shelter: ExposureShelter,
) -> ExposureOutcome {
    let mut frostbite_event_offsets = Vec::new();
    for (offset, snapshot) in weather.into_iter().enumerate() {
        state.wetness_bps = next_wetness(state.wetness_bps, snapshot, clothing, shelter);
        state.thermal_strain = next_thermal_strain(
            state.thermal_strain,
            state.wetness_bps,
            snapshot,
            clothing,
            shelter,
        );
        if state.thermal_strain <= FROSTBITE_STRAIN_THRESHOLD {
            state.frostbite_progress_minutes = state.frostbite_progress_minutes.saturating_add(1);
            while state.frostbite_progress_minutes >= FROSTBITE_EXPOSURE_MINUTES_PER_DAMAGE {
                state.frostbite_progress_minutes -= FROSTBITE_EXPOSURE_MINUTES_PER_DAMAGE;
                frostbite_event_offsets.push(offset as u64);
            }
        } else {
            state.frostbite_progress_minutes = state.frostbite_progress_minutes.saturating_sub(1);
        }
    }
    ExposureOutcome {
        state,
        frostbite_event_offsets,
    }
}

fn next_wetness(
    wetness: u16,
    weather: WeatherSnapshot,
    clothing: ClothingExposure,
    shelter: ExposureShelter,
) -> u16 {
    let rain = matches!(weather.precipitation, Precipitation::Rain) && !shelter.blocks_rain();
    let gain = if rain {
        let protection = u32::from(clothing.weatherproofing_bps) * 9 / 10;
        u32::from(weather.intensity_bps)
            * (u32::from(BASIS_POINTS_PER_WHOLE).saturating_sub(protection))
            / 500_000
    } else {
        0
    };
    // Padding traps moisture. Warmth and wind otherwise accelerate evaporation.
    let evaporation = if rain {
        0
    } else {
        let warmth = weather.temperature_deci_c.saturating_add(100).max(10) as u32;
        let wind = u32::from(weather.wind_speed_bps) * u32::from(shelter.wind_multiplier_bps())
            / u32::from(BASIS_POINTS_PER_WHOLE);
        let trapped = u32::from(BASIS_POINTS_PER_WHOLE)
            .saturating_sub(u32::from(clothing.insulation_bps) * 3 / 4);
        (warmth + wind / 250) * trapped / 100_000
    };
    u32::from(wetness)
        .saturating_add(gain.max(u32::from(rain)))
        .saturating_sub(evaporation.max(u32::from(!rain)))
        .min(u32::from(MAX_WETNESS_BPS)) as u16
}

fn next_thermal_strain(
    strain: i32,
    wetness: u16,
    weather: WeatherSnapshot,
    clothing: ClothingExposure,
    shelter: ExposureShelter,
) -> i32 {
    // Settlement downtime happens inside a building rather than in a tent
    // pitched outdoors. Ignore the exterior temperature while indoors and
    // let any existing strain recover at the same bounded neutral rate used
    // for comfortable weather.
    if shelter == ExposureShelter::Indoor {
        return strain - strain.signum();
    }
    const COMFORT_DECI_C: i32 = 180;
    let wind = i32::from(weather.wind_speed_bps) * i32::from(shelter.wind_multiplier_bps())
        / i32::from(BASIS_POINTS_PER_WHOLE);
    let wind_chill_deci_c = wind / 180;
    let wet_chill_deci_c = i32::from(wetness) / 160;
    let insulation = i32::from(clothing.insulation_bps);
    let cold_delta =
        (COMFORT_DECI_C - weather.temperature_deci_c + wind_chill_deci_c + wet_chill_deci_c).max(0);
    let heat_delta = (weather.temperature_deci_c - 240).max(0);
    let whole_bps = i32::from(BASIS_POINTS_PER_WHOLE);
    let cold_rate = cold_delta * (whole_bps - insulation) / 50_000;
    // Insulation reduces sweat evaporation and therefore worsens heat.
    let heat_rate = heat_delta * (whole_bps + insulation / 2) / 50_000;
    let next = match (cold_rate, heat_rate) {
        (cold, 0) if cold > 0 => strain.saturating_sub(cold.max(1)),
        (0, heat) if heat > 0 => strain.saturating_add(heat.max(1)),
        _ => strain - strain.signum(),
    };
    next.clamp(-MAX_THERMAL_STRAIN, MAX_THERMAL_STRAIN)
}

pub fn thermal_incapacitation(strain: i32) -> f32 {
    let magnitude = strain.unsigned_abs().min(MAX_THERMAL_STRAIN as u32) as f32;
    let warning_threshold = if strain < 0 {
        COLD_STAGGER_STRAIN.unsigned_abs()
    } else {
        HEAT_STAGGER_STRAIN as u32
    } as f32;
    let incapacitating_threshold = if strain < 0 {
        COLD_INCAPACITATION_STRAIN.unsigned_abs()
    } else {
        HEAT_INCAPACITATION_STRAIN as u32
    } as f32;
    ((magnitude - warning_threshold) / (incapacitating_threshold - warning_threshold))
        .clamp(0.0, 1.0)
}

/// Choose one least-protected peripheral at a canonical event minute. Ties
/// rotate deterministically so repeated exposure does not always punish the
/// same side, while interval partitioning cannot alter the result.
pub fn frostbite_peripheral_index(protection_bps: [u16; 4], absolute_event_minute: u64) -> usize {
    let minimum = protection_bps.into_iter().min().unwrap_or(0);
    let tied = protection_bps
        .into_iter()
        .enumerate()
        .filter_map(|(index, protection)| (protection == minimum).then_some(index))
        .collect::<Vec<_>>();
    tied[(absolute_event_minute as usize) % tied.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather(temp: i32, wind: u16, rain: bool) -> WeatherSnapshot {
        WeatherSnapshot {
            rules_version: crate::weather::WEATHER_RULES_VERSION,
            interval_start_minute: 0,
            cell_latitude: 0,
            cell_longitude: 0,
            temperature_deci_c: temp,
            wind_speed_bps: wind,
            precipitation: if rain {
                Precipitation::Rain
            } else {
                Precipitation::Clear
            },
            intensity_bps: if rain { 8_000 } else { 0 },
            ground_moisture_bps: 0,
            snow_cover_bps: 0,
            atmosphere: Default::default(),
        }
    }

    #[test]
    fn interval_partition_does_not_change_state_or_frostbite() {
        let samples = vec![weather(-100, 8_000, true); 1_000];
        let whole = advance_exposure(
            SurvivalState::default(),
            samples.iter().copied(),
            ClothingExposure::default(),
            ExposureShelter::Field(FieldShelter::Bivouac),
        );
        let first = advance_exposure(
            SurvivalState::default(),
            samples[..333].iter().copied(),
            ClothingExposure::default(),
            ExposureShelter::Field(FieldShelter::Bivouac),
        );
        let second = advance_exposure(
            first.state,
            samples[333..].iter().copied(),
            ClothingExposure::default(),
            ExposureShelter::Field(FieldShelter::Bivouac),
        );
        assert_eq!(whole.state, second.state);
        assert_eq!(
            whole.frostbite_event_offsets,
            first
                .frostbite_event_offsets
                .iter()
                .copied()
                .chain(
                    second
                        .frostbite_event_offsets
                        .iter()
                        .map(|offset| offset + 333)
                )
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn wetness_worsens_cold_and_tent_blocks_rain_without_adding_heat() {
        let cold_rain = weather(-20, 8_000, true);
        let exposed = advance_exposure(
            SurvivalState::default(),
            [cold_rain; 60],
            ClothingExposure::default(),
            ExposureShelter::Field(FieldShelter::Bivouac),
        );
        let tented = advance_exposure(
            SurvivalState::default(),
            [cold_rain; 60],
            ClothingExposure::default(),
            ExposureShelter::Field(FieldShelter::Tent),
        );
        assert!(exposed.state.wetness_bps > tented.state.wetness_bps);
        assert!(exposed.state.thermal_strain < tented.state.thermal_strain);
        assert_eq!(
            advance_exposure(
                SurvivalState::default(),
                [weather(180, 0, false); 60],
                ClothingExposure::default(),
                ExposureShelter::Field(FieldShelter::Tent),
            )
            .state
            .thermal_strain,
            0
        );
    }

    #[test]
    fn indoor_rest_does_not_create_extreme_thermal_exposure() {
        for weather in [weather(-800, 10_000, true), weather(500, 0, false)] {
            let outcome = advance_exposure(
                SurvivalState::default(),
                [weather; 1_440],
                ClothingExposure::default(),
                ExposureShelter::Indoor,
            );
            assert_eq!(outcome.state.thermal_strain, 0);
            assert_eq!(thermal_incapacitation(outcome.state.thermal_strain), 0.0);
            assert!(outcome.frostbite_event_offsets.is_empty());
        }

        let recovering = advance_exposure(
            SurvivalState {
                wetness_bps: MAX_WETNESS_BPS,
                thermal_strain: COLD_STAGGER_STRAIN,
                frostbite_progress_minutes: 10,
            },
            [weather(-800, 10_000, true); 60],
            ClothingExposure::default(),
            ExposureShelter::Indoor,
        );
        assert!(recovering.state.wetness_bps < MAX_WETNESS_BPS);
        assert!(recovering.state.thermal_strain > COLD_STAGGER_STRAIN);
        assert_eq!(recovering.state.frostbite_progress_minutes, 0);
    }

    #[test]
    fn immersion_and_thermal_incapacitation_are_bounded() {
        assert_eq!(
            apply_immersion(SurvivalState::default()).wetness_bps,
            IMMERSION_WETNESS_BPS
        );
        assert_eq!(thermal_incapacitation(0), 0.0);
        assert_eq!(thermal_incapacitation(MAX_THERMAL_STRAIN), 1.0);
        assert_eq!(thermal_incapacitation(COLD_STAGGER_STRAIN), 0.0);
        assert_eq!(thermal_incapacitation(HEAT_STAGGER_STRAIN), 0.0);
        assert!(thermal_incapacitation(COLD_STAGGER_STRAIN - 1) > 0.0);
        assert!(thermal_incapacitation(HEAT_STAGGER_STRAIN + 1) > 0.0);
        assert_eq!(thermal_incapacitation(COLD_INCAPACITATION_STRAIN), 1.0);
    }

    #[test]
    fn padding_and_leather_threshold_are_the_only_clothing_inputs() {
        assert_eq!(insulation_from_layers([(10.0, 1.0)]), 800);
        assert_eq!(insulation_from_layers([(10.0, 0.5)]), 400);
        assert!(!is_weatherproof_outer_layer(
            LEATHER_WEATHERPROOF_RESISTANCE - 0.01
        ));
        assert!(is_weatherproof_outer_layer(LEATHER_WEATHERPROOF_RESISTANCE));
        assert!(is_weatherproof_outer_layer(
            LEATHER_WEATHERPROOF_RESISTANCE * 10.0
        ));
        assert_eq!(
            weatherproofing_from_outer_layer(LEATHER_WEATHERPROOF_RESISTANCE * 10.0, 0.5),
            5_000
        );
        assert_eq!(
            weatherproofing_from_outer_layer(LEATHER_WEATHERPROOF_RESISTANCE / 2.0, 1.0),
            5_000
        );
    }

    #[test]
    fn frostbite_targets_only_a_least_protected_peripheral_deterministically() {
        let protection = [9_000, 2_000, 2_000, 8_000];
        assert_eq!(frostbite_peripheral_index(protection, 0), 1);
        assert_eq!(frostbite_peripheral_index(protection, 1), 2);
        assert_eq!(frostbite_peripheral_index(protection, 2), 1);
    }
}
