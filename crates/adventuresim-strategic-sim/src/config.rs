use crate::FORMAT_VERSION;
use adventuresim_core::strategic_time::DAYS_PER_YEAR;
use serde::{Deserialize, Serialize};

pub const MAX_POPULATION: u32 = 10_000;
const MAX_SIMULATION_YEARS: u32 = 100;
pub const DEFAULT_SIMULATION_DAYS: u32 = DAYS_PER_YEAR as u32 * 3;
pub const DEFAULT_MATCHED_DAYS: u32 = DAYS_PER_YEAR as u32;
pub const DEFAULT_CORE_LOOP_DURATION_DAYS: u32 = DAYS_PER_YEAR as u32;
pub const MAX_DAYS: u32 = DAYS_PER_YEAR as u32 * MAX_SIMULATION_YEARS;
pub const MAX_DECISIONS: u64 = 100_000_000;
pub const MAX_TRACE_EVENTS: u32 = 100_000;
pub const MAX_SNAPSHOTS: u32 = 100_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationConfig {
    pub version: u32,
    pub seed: u64,
    pub population: u32,
    pub days: u32,
    pub max_decisions: u64,
    pub max_trace_events: u32,
    pub snapshot_interval_days: u32,
    pub max_snapshots: u32,
    pub population_scale: f32,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            seed: 1,
            population: 100,
            days: DEFAULT_SIMULATION_DAYS,
            max_decisions: 1_000_000,
            max_trace_events: 10_000,
            snapshot_interval_days: 30,
            max_snapshots: 10_000,
            population_scale: 2.0,
        }
    }
}

impl SimulationConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != FORMAT_VERSION {
            return Err(format!("unsupported config version {}", self.version));
        }
        if !(1..=MAX_POPULATION).contains(&self.population) {
            return Err(format!("population must be 1..={MAX_POPULATION}"));
        }
        if !(1..=MAX_DAYS).contains(&self.days) {
            return Err(format!("days must be 1..={MAX_DAYS}"));
        }
        if !(1..=MAX_DECISIONS).contains(&self.max_decisions) {
            return Err(format!("max_decisions must be 1..={MAX_DECISIONS}"));
        }
        if self.max_trace_events > MAX_TRACE_EVENTS {
            return Err(format!("max_trace_events must be <= {MAX_TRACE_EVENTS}"));
        }
        if !(1..=MAX_DAYS).contains(&self.snapshot_interval_days) {
            return Err("snapshot_interval_days must be nonzero and bounded".into());
        }
        if self.max_snapshots > MAX_SNAPSHOTS {
            return Err(format!("max_snapshots must be <= {MAX_SNAPSHOTS}"));
        }
        if !self.population_scale.is_finite() || !(0.5..=4.0).contains(&self.population_scale) {
            return Err("population_scale must be finite and in 0.5..=4".into());
        }
        let decisions = u64::from(self.population)
            .checked_mul(u64::from(self.days))
            .ok_or("decision count overflow")?;
        if decisions > self.max_decisions {
            return Err("population * days exceeds max_decisions".into());
        }
        Ok(())
    }
}
