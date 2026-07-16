//! Shared strategic travel-provision calculations.

pub const STRATEGIC_TRAVEL_KCAL_PER_DAY: f32 = 6_000.0;
pub const STRATEGIC_TRAVEL_WATER_ML_PER_DAY: f32 = 4_000.0;
pub const STRATEGIC_PROVISION_BUFFER_PERCENT: u64 = 30;
pub const STANDARD_TRAVEL_RATION_ID: &str = "travel_ration";
pub const STANDARD_WATERSKIN_ID: &str = "waterskin";

/// Authoritative inputs for calculating the containers and consumables needed
/// for a planned journey.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProvisioningInputs {
    pub planning_minutes: u64,
    pub buffer_percent: u64,
    pub food_balance_kcal: f32,
    pub water_balance_ml: f32,
    pub travel_kcal_per_day: f32,
    pub travel_water_ml_per_day: f32,
    pub ration_kcal: f32,
    pub waterskin_capacity_ml: u32,
}

/// Whole provision units required to cover a journey after physiological
/// reserves have been taken into account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProvisionUnits {
    pub rations: u32,
    pub waterskins: u32,
}

impl ProvisioningInputs {
    pub fn required_units(self) -> ProvisionUnits {
        let buffered_minutes = self
            .planning_minutes
            .saturating_mul(100 + self.buffer_percent)
            .div_ceil(100);
        let travel_days = buffered_minutes as f32 / (24.0 * 60.0);
        let food_needed =
            (travel_days * self.travel_kcal_per_day - self.food_balance_kcal.max(0.0)).max(0.0);
        let water_needed =
            (travel_days * self.travel_water_ml_per_day - self.water_balance_ml.max(0.0)).max(0.0);
        ProvisionUnits {
            rations: if self.ration_kcal > 0.0 {
                (food_needed / self.ration_kcal).ceil() as u32
            } else {
                0
            },
            waterskins: if self.waterskin_capacity_ml > 0 {
                (water_needed / self.waterskin_capacity_ml as f32).ceil() as u32
            } else {
                0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_inputs(planning_minutes: u64) -> ProvisioningInputs {
        ProvisioningInputs {
            planning_minutes,
            buffer_percent: 30,
            food_balance_kcal: 6_000.0,
            water_balance_ml: 4_000.0,
            travel_kcal_per_day: 6_000.0,
            travel_water_ml_per_day: 4_000.0,
            ration_kcal: 6_000.0,
            waterskin_capacity_ml: 4_000,
        }
    }

    #[test]
    fn includes_return_and_thirty_percent_reserve() {
        assert_eq!(
            standard_inputs(2 * 24 * 60).required_units(),
            ProvisionUnits {
                rations: 2,
                waterskins: 2,
            }
        );
    }

    #[test]
    fn short_trips_fit_within_physiological_reserves() {
        assert_eq!(
            standard_inputs(12 * 60).required_units(),
            ProvisionUnits::default()
        );
    }
}
