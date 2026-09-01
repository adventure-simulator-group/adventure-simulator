#[cfg(not(target_family = "wasm"))]
use std::path::Path;

use bevy::prelude::Resource;
use serde::Deserialize;

const MAX_AUDIO_CONFIG_BYTES: u64 = 32 * 1024;

#[derive(Debug, Clone, Deserialize, Resource)]
#[serde(deny_unknown_fields)]
pub(crate) struct TacticalAudioConfig {
    pub(crate) movement: MovementAudioConfig,
    pub(crate) combat: CombatAudioConfig,
    pub(crate) weather: WeatherAudioConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MovementAudioConfig {
    pub(crate) footstep_relative_volume: f32,
    pub(crate) body_impact_relative_volume: f32,
    pub(crate) tall_grass_rustle_relative_volume: f32,
    pub(crate) tall_grass_rustle_pitch: f32,
    pub(crate) bush_rustle_relative_volume: f32,
    pub(crate) bush_rustle_pitch: f32,
    pub(crate) bush_contact_radius_m: f32,
    pub(crate) pitch_randomization: [f32; 2],
    pub(crate) wind: WindAudioConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WindAudioConfig {
    pub(crate) minimum_audible_speed_bps: u16,
    pub(crate) maximum_relative_volume: f32,
    pub(crate) response_exponent: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatAudioConfig {
    pub(crate) impact_relative_volume: f32,
    pub(crate) impact_pitch_randomization: [f32; 2],
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WeatherAudioConfig {
    pub(crate) rain_surface_maximum_relative_volume: f32,
    pub(crate) rain_storm_maximum_relative_volume: f32,
    pub(crate) heavy_rain_start_fraction: f32,
    pub(crate) volume_response_per_second: f32,
    pub(crate) surface_intensity_exponent: f32,
    pub(crate) storm_surface_ducking: f32,
}

impl TacticalAudioConfig {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let config: Self = serde_saphyr::from_str(text)
            .map_err(|error| format!("audio configuration is not valid YAML: {error}"))?;
        config.validate()?;
        Ok(config)
    }

    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let length = std::fs::metadata(path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?
            .len();
        if length == 0 || length > MAX_AUDIO_CONFIG_BYTES {
            return Err("audio config must contain between 1 byte and 32 KiB".into());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    fn validate(&self) -> Result<(), String> {
        let movement = &self.movement;
        for (name, volume) in [
            (
                "movement.footstep_relative_volume",
                movement.footstep_relative_volume,
            ),
            (
                "movement.body_impact_relative_volume",
                movement.body_impact_relative_volume,
            ),
            (
                "movement.tall_grass_rustle_relative_volume",
                movement.tall_grass_rustle_relative_volume,
            ),
            (
                "movement.bush_rustle_relative_volume",
                movement.bush_rustle_relative_volume,
            ),
            (
                "movement.wind.maximum_relative_volume",
                movement.wind.maximum_relative_volume,
            ),
            (
                "combat.impact_relative_volume",
                self.combat.impact_relative_volume,
            ),
            (
                "weather.rain_surface_maximum_relative_volume",
                self.weather.rain_surface_maximum_relative_volume,
            ),
            (
                "weather.rain_storm_maximum_relative_volume",
                self.weather.rain_storm_maximum_relative_volume,
            ),
        ] {
            unit_interval(name, volume)?;
        }
        positive(
            "movement.tall_grass_rustle_pitch",
            movement.tall_grass_rustle_pitch,
        )?;
        positive("movement.bush_rustle_pitch", movement.bush_rustle_pitch)?;
        positive(
            "movement.bush_contact_radius_m",
            movement.bush_contact_radius_m,
        )?;
        pitch_range("movement.pitch_randomization", movement.pitch_randomization)?;
        if movement.wind.minimum_audible_speed_bps
            >= adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
        {
            return Err("movement.wind.minimum_audible_speed_bps must be below one whole".into());
        }
        positive(
            "movement.wind.response_exponent",
            movement.wind.response_exponent,
        )?;
        pitch_range(
            "combat.impact_pitch_randomization",
            self.combat.impact_pitch_randomization,
        )?;
        unit_interval(
            "weather.heavy_rain_start_fraction",
            self.weather.heavy_rain_start_fraction,
        )?;
        if self.weather.heavy_rain_start_fraction >= 1.0 {
            return Err("weather.heavy_rain_start_fraction must be below 1".into());
        }
        positive(
            "weather.volume_response_per_second",
            self.weather.volume_response_per_second,
        )?;
        positive(
            "weather.surface_intensity_exponent",
            self.weather.surface_intensity_exponent,
        )?;
        unit_interval(
            "weather.storm_surface_ducking",
            self.weather.storm_surface_ducking,
        )
    }
}

fn positive(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{name} must be finite and positive"))
    }
}

fn unit_interval(name: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 0 and 1"))
    }
}

fn pitch_range(name: &str, range: [f32; 2]) -> Result<(), String> {
    if range[0].is_finite() && range[0] > 0.0 && range[1].is_finite() && range[1] >= range[0] {
        Ok(())
    } else {
        Err(format!(
            "{name} must contain an ordered pair of positive values"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_configuration_is_valid() {
        TacticalAudioConfig::parse(include_str!("../../../assets/config/tactical-audio.yaml"))
            .unwrap();
    }

    #[test]
    fn invalid_pitch_range_fails_closed() {
        let text = include_str!("../../../assets/config/tactical-audio.yaml").replace(
            "pitch_randomization: [0.94, 1.06]",
            "pitch_randomization: [1.1, 0.9]",
        );
        assert!(TacticalAudioConfig::parse(&text).is_err());
    }
}
