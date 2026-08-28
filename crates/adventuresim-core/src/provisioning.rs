//! Shared strategic travel-provision calculations.

pub const STRATEGIC_TRAVEL_KCAL_PER_DAY: f32 = 6_000.0;
pub const STRATEGIC_TRAVEL_WATER_ML_PER_DAY: f32 = 4_000.0;

pub fn shared_then_personal_units(requested: u32, shared: u32, personal: u32) -> (u32, u32) {
    let shared_used = requested.min(shared);
    (
        shared_used,
        requested.saturating_sub(shared_used).min(personal),
    )
}

/// Aggregate party supplies used by the travel preview and provisioning draft.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartyProvisioningInputs {
    pub planning_minutes: u64,
    pub target_surplus_days: f32,
    pub living_members: u32,
    pub food_reserve_kcal: f32,
    /// Useful calories across every carried/shared food lot.
    pub food_lot_kcal: f32,
    pub water_reserve_ml: f32,
    pub waterskin_count: u32,
    pub ration_kcal: f32,
    pub waterskin_capacity_ml: u32,
    pub emergency_alcohol_hydration_ml: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartyProvisioningForecast {
    pub food_days: f32,
    pub water_days: f32,
    pub ordinary_water_days: f32,
    pub emergency_alcohol_days: f32,
    pub journey_days: f32,
    pub rations_to_buy: u32,
    pub waterskins_to_buy: u32,
}

impl PartyProvisioningInputs {
    pub fn forecast(self) -> PartyProvisioningForecast {
        let members = self.living_members.max(1) as f32;
        let food_per_day = members * STRATEGIC_TRAVEL_KCAL_PER_DAY;
        let water_per_day = members * STRATEGIC_TRAVEL_WATER_ML_PER_DAY;
        let food = (self.food_reserve_kcal + self.food_lot_kcal).max(0.0);
        let ordinary_water = (self.water_reserve_ml
            + self.waterskin_count as f32 * self.waterskin_capacity_ml as f32)
            .max(0.0);
        let water = ordinary_water + self.emergency_alcohol_hydration_ml as f32;
        let journey_days = self.planning_minutes as f32 / (24.0 * 60.0);
        let target_days = (journey_days + self.target_surplus_days).max(0.0);
        PartyProvisioningForecast {
            food_days: food / food_per_day,
            water_days: water / water_per_day,
            ordinary_water_days: ordinary_water / water_per_day,
            emergency_alcohol_days: self.emergency_alcohol_hydration_ml as f32 / water_per_day,
            journey_days,
            rations_to_buy: if self.ration_kcal > 0.0 {
                ((target_days * food_per_day - food).max(0.0) / self.ration_kcal).ceil() as u32
            } else {
                0
            },
            waterskins_to_buy: if self.waterskin_capacity_ml > 0 {
                ((target_days * water_per_day - water).max(0.0) / self.waterskin_capacity_ml as f32)
                    .ceil() as u32
            } else {
                0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_target_math_allows_negative_days_and_ceil_units() {
        let base = PartyProvisioningInputs {
            planning_minutes: 2 * 24 * 60,
            living_members: 2,
            food_reserve_kcal: 6_001.0,
            water_reserve_ml: 4_001.0,
            ration_kcal: 6_000.0,
            waterskin_capacity_ml: 4_000,
            ..Default::default()
        };
        assert_eq!(base.forecast().rations_to_buy, 3);
        assert_eq!(base.forecast().waterskins_to_buy, 3);
        let negative = PartyProvisioningInputs {
            target_surplus_days: -1.0,
            ..base
        }
        .forecast();
        assert_eq!(negative.rations_to_buy, 1);
        assert_eq!(negative.waterskins_to_buy, 1);
    }

    #[test]
    fn communal_supplies_are_consumed_before_personal_supplies() {
        assert_eq!(shared_then_personal_units(3, 2, 4), (2, 1));
    }

    #[test]
    fn food_lot_calories_are_the_authoritative_carried_food_total() {
        let forecast = PartyProvisioningInputs {
            planning_minutes: 24 * 60,
            living_members: 1,
            food_lot_kcal: 4_000.0,
            ration_kcal: 3_000.0,
            ..Default::default()
        }
        .forecast();
        assert_eq!(forecast.rations_to_buy, 1);
        assert!((forecast.food_days - (4_000.0 / STRATEGIC_TRAVEL_KCAL_PER_DAY)).abs() < 0.001);
    }
}
