use serde::{Deserialize, Serialize};

/// Yellow wheel extensions show recent deterioration projected over this
/// horizon. These durations affect presentation, never combat outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncapacitationForecastConfig {
    pub horizon_seconds: f32,
    pub trend_response_seconds: f32,
}

impl Default for IncapacitationForecastConfig {
    fn default() -> Self {
        Self {
            horizon_seconds: 2.0,
            trend_response_seconds: 1.0,
        }
    }
}

impl IncapacitationForecastConfig {
    pub fn validate(self) -> Result<(), &'static str> {
        if [self.horizon_seconds, self.trend_response_seconds]
            .into_iter()
            .all(|seconds| seconds.is_finite() && seconds > 0.0)
        {
            Ok(())
        } else {
            Err("incapacitation forecast durations must be finite and positive")
        }
    }
}
