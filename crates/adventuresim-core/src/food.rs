//! Pure food-lot, spoilage, meal, and cooking rules.

use serde::{Deserialize, Serialize};

pub const MAX_MEAL_FULLNESS_KCAL: f32 = 3_000.0;
pub const MIN_INITIAL_CONTAMINATION: f32 = 1.0e-8;
pub const MAX_INITIAL_CONTAMINATION: f32 = 1.0e-5;
pub const RECONTAMINATION_FLOOR: f32 = 1.0e-9;
pub const MAX_CONTAMINATION: f32 = 1.0e9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoodClass {
    Ration,
    Grain,
    Bread,
    Fruit,
    Berries,
    Vegetable,
    Nuts,
    Herb,
    Mushroom,
    RawMeat,
    CookedMeat,
    MixedMeal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CookingMethod {
    PanFry,
    Stew,
    Roast,
    Bake,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub class: FoodClass,
    pub kcal_per_unit: f32,
    pub mass_kg_per_unit: f32,
    pub value_per_unit: f32,
    /// Exponential growth exponent per hour at the deferred ambient baseline.
    pub growth_per_hour: f32,
    /// Minutes required for the ingredient to become safe/done.
    pub cooking_minutes: u32,
}

pub const FOOD_CATALOG: &[FoodDefinition] = &[
    FoodDefinition {
        id: "travel_ration",
        name: "Travel ration",
        class: FoodClass::Ration,
        kcal_per_unit: 2_500.0,
        mass_kg_per_unit: 0.65,
        value_per_unit: 3.0,
        growth_per_hour: 0.008,
        cooking_minutes: 0,
    },
    FoodDefinition {
        id: "oat_grain",
        name: "Oat grain",
        class: FoodClass::Grain,
        kcal_per_unit: 950.0,
        mass_kg_per_unit: 0.25,
        value_per_unit: 1.0,
        growth_per_hour: 0.010,
        cooking_minutes: 25,
    },
    FoodDefinition {
        id: "rye_bread",
        name: "Rye bread",
        class: FoodClass::Bread,
        kcal_per_unit: 1_250.0,
        mass_kg_per_unit: 0.50,
        value_per_unit: 2.0,
        growth_per_hour: 0.018,
        cooking_minutes: 4,
    },
    FoodDefinition {
        id: "apple",
        name: "Apple",
        class: FoodClass::Fruit,
        kcal_per_unit: 95.0,
        mass_kg_per_unit: 0.18,
        value_per_unit: 0.3,
        growth_per_hour: 0.012,
        cooking_minutes: 5,
    },
    FoodDefinition {
        id: "wild_berries",
        name: "Wild berries",
        class: FoodClass::Berries,
        kcal_per_unit: 140.0,
        mass_kg_per_unit: 0.25,
        value_per_unit: 0.6,
        growth_per_hour: 0.020,
        cooking_minutes: 4,
    },
    FoodDefinition {
        id: "root_vegetables",
        name: "Root vegetables",
        class: FoodClass::Vegetable,
        kcal_per_unit: 190.0,
        mass_kg_per_unit: 0.35,
        value_per_unit: 0.5,
        growth_per_hour: 0.010,
        cooking_minutes: 20,
    },
    FoodDefinition {
        id: "hazelnuts",
        name: "Hazelnuts",
        class: FoodClass::Nuts,
        kcal_per_unit: 630.0,
        mass_kg_per_unit: 0.10,
        value_per_unit: 1.0,
        growth_per_hour: 0.004,
        cooking_minutes: 5,
    },
    FoodDefinition {
        id: "garlic",
        name: "Garlic",
        class: FoodClass::Herb,
        kcal_per_unit: 45.0,
        mass_kg_per_unit: 0.10,
        value_per_unit: 1.0,
        growth_per_hour: 0.009,
        cooking_minutes: 3,
    },
    FoodDefinition {
        id: "sage",
        name: "Sage",
        class: FoodClass::Herb,
        kcal_per_unit: 8.0,
        mass_kg_per_unit: 0.05,
        value_per_unit: 2.0,
        growth_per_hour: 0.008,
        cooking_minutes: 2,
    },
    FoodDefinition {
        id: "watercress",
        name: "Watercress",
        class: FoodClass::Herb,
        kcal_per_unit: 28.0,
        mass_kg_per_unit: 0.20,
        value_per_unit: 0.7,
        growth_per_hour: 0.025,
        cooking_minutes: 2,
    },
    FoodDefinition {
        id: "seaweed",
        name: "Seaweed",
        class: FoodClass::Vegetable,
        kcal_per_unit: 45.0,
        mass_kg_per_unit: 0.25,
        value_per_unit: 0.6,
        growth_per_hour: 0.022,
        cooking_minutes: 5,
    },
    FoodDefinition {
        id: "wild_mushrooms",
        name: "Wild mushrooms",
        class: FoodClass::Mushroom,
        kcal_per_unit: 55.0,
        mass_kg_per_unit: 0.25,
        value_per_unit: 1.0,
        growth_per_hour: 0.030,
        cooking_minutes: 12,
    },
    FoodDefinition {
        id: "raw_venison",
        name: "Raw venison",
        class: FoodClass::RawMeat,
        kcal_per_unit: 790.0,
        mass_kg_per_unit: 0.50,
        value_per_unit: 2.0,
        growth_per_hour: 0.090,
        cooking_minutes: 18,
    },
    FoodDefinition {
        id: "raw_fowl",
        name: "Raw fowl",
        class: FoodClass::RawMeat,
        kcal_per_unit: 720.0,
        mass_kg_per_unit: 0.50,
        value_per_unit: 2.0,
        growth_per_hour: 0.105,
        cooking_minutes: 22,
    },
    FoodDefinition {
        id: "cooked_meal",
        name: "Cooked meal",
        class: FoodClass::MixedMeal,
        kcal_per_unit: 3_000.0,
        mass_kg_per_unit: 0.65,
        value_per_unit: 1.0,
        growth_per_hour: 0.025,
        cooking_minutes: 0,
    },
];

pub fn definition(id: &str) -> Option<&'static FoodDefinition> {
    FOOD_CATALOG.iter().find(|food| food.id == id)
}

pub fn deterministic_initial_contamination(seed: u64) -> f32 {
    let mixed = seed.wrapping_mul(0x9E3779B97F4A7C15).rotate_left(27) ^ 0xD1B54A32D192ED03;
    let unit = (mixed >> 11) as f64 / ((1_u64 << 53) as f64);
    let log_min = (MIN_INITIAL_CONTAMINATION as f64).ln();
    let log_max = (MAX_INITIAL_CONTAMINATION as f64).ln();
    (log_min + unit * (log_max - log_min)).exp() as f32
}

pub fn contamination_at(anchor: f32, growth_per_hour: f32, elapsed_minutes: u64) -> f32 {
    if !anchor.is_finite() || !growth_per_hour.is_finite() {
        return MAX_CONTAMINATION;
    }
    let exponent = (growth_per_hour.max(0.0) as f64 * elapsed_minutes as f64 / 60.0).min(80.0);
    ((anchor.max(0.0) as f64 * exponent.exp()).min(MAX_CONTAMINATION as f64)) as f32
}

pub fn cooked_contamination(current: f32, method: CookingMethod) -> f32 {
    let kill = match method {
        CookingMethod::PanFry => 1.0e-5,
        CookingMethod::Stew => 2.0e-6,
        CookingMethod::Roast => 1.0e-5,
        CookingMethod::Bake => 4.0e-6,
    };
    (current.max(0.0) * kill)
        .max(RECONTAMINATION_FLOOR)
        .min(MAX_CONTAMINATION)
}

pub fn cooked_growth_per_hour(input_growth: &[f32], method: CookingMethod) -> f32 {
    let slowest = input_growth
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(0.0_f32, f32::max);
    let factor = match method {
        CookingMethod::PanFry => 0.42,
        CookingMethod::Stew => 0.35,
        CookingMethod::Roast => 0.45,
        CookingMethod::Bake => 0.32,
    };
    (slowest * factor).clamp(0.003, 0.045)
}

pub fn cooking_duration_minutes(
    method: CookingMethod,
    safety_minutes: &[u32],
    total_mass_kg: f32,
) -> Option<u32> {
    if safety_minutes.is_empty()
        || !total_mass_kg.is_finite()
        || total_mass_kg <= 0.0
        || total_mass_kg > 25.0
    {
        return None;
    }
    let setup: u32 = match method {
        CookingMethod::PanFry => 5,
        CookingMethod::Stew => 12,
        CookingMethod::Roast => 7,
        CookingMethod::Bake => 15,
    };
    let slowest = safety_minutes.iter().copied().max()?;
    let batch = ((total_mass_kg - 0.5).max(0.0).sqrt() * 8.0).ceil() as u32;
    setup.checked_add(slowest)?.checked_add(batch)
}

/// Expertise shortens setup and batch handling, never the ingredient safety
/// interval. Rank five removes at most thirty percent of overhead.
pub fn cooking_duration_minutes_for_check(
    method: CookingMethod,
    safety_minutes: &[u32],
    total_mass_kg: f32,
    cooking_check: f32,
) -> Option<u32> {
    let baseline = cooking_duration_minutes(method, safety_minutes, total_mass_kg)?;
    let safety = safety_minutes.iter().copied().max()?;
    let overhead = baseline.saturating_sub(safety);
    let check = if cooking_check.is_finite() {
        cooking_check.clamp(0.0, 5.0)
    } else {
        0.0
    };
    safety.checked_add((overhead as f32 * (1.0 - 0.06 * check)).ceil() as u32)
}

pub fn cooked_nutrition_retention(cooking_check: f32) -> f32 {
    let check = if cooking_check.is_finite() {
        cooking_check.clamp(0.0, 5.0)
    } else {
        0.0
    };
    0.95 + 0.008 * check
}

pub fn cooked_quality_multiplier(cooking_check: f32) -> f32 {
    let check = if cooking_check.is_finite() {
        cooking_check.clamp(0.0, 5.0)
    } else {
        0.0
    };
    0.95 + 0.03 * check
}

/// Cooked output is terminal preparation state. Allowing it back into the
/// ingredient pipeline would repeatedly multiply retained value and nutrition.
pub fn is_cookable_ingredient(item_id: &str) -> bool {
    item_id != "cooked_meal"
}

pub fn travel_consumption(deficit_kcal: f32, available_kcal: f32) -> f32 {
    if !deficit_kcal.is_finite() || !available_kcal.is_finite() {
        return 0.0;
    }
    (-deficit_kcal.min(0.0)).min(available_kcal.max(0.0))
}

pub fn explicit_meal_consumption(balance_kcal: f32, available_kcal: f32) -> f32 {
    if !balance_kcal.is_finite() || !available_kcal.is_finite() {
        return 0.0;
    }
    (MAX_MEAL_FULLNESS_KCAL - balance_kcal)
        .max(0.0)
        .min(available_kcal.max(0.0))
}

/// Scale a non-negative lot component while keeping malformed state from
/// creating mass, nutrition, value, or provenance.
pub fn retained_component(value: f32, retained_fraction: f32) -> f32 {
    if !value.is_finite() || !retained_fraction.is_finite() {
        return 0.0;
    }
    value.max(0.0) * retained_fraction.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starting_load_is_deterministic_log_bounded() {
        for seed in 0..10_000 {
            let x = deterministic_initial_contamination(seed);
            assert!((MIN_INITIAL_CONTAMINATION..=MAX_INITIAL_CONTAMINATION).contains(&x));
            assert_eq!(x, deterministic_initial_contamination(seed));
        }
    }
    #[test]
    fn exponential_growth_is_bounded_and_realistically_ordered() {
        let start = 1e-6;
        let fruit = contamination_at(start, definition("apple").unwrap().growth_per_hour, 24 * 60);
        let meat = contamination_at(
            start,
            definition("raw_fowl").unwrap().growth_per_hour,
            24 * 60,
        );
        assert!(meat > fruit);
        assert_eq!(
            contamination_at(f32::MAX, 100.0, u64::MAX),
            MAX_CONTAMINATION
        );
    }
    #[test]
    fn cooking_kills_and_slows_contamination() {
        let raw = definition("raw_venison").unwrap();
        assert!(cooked_contamination(1.0, CookingMethod::Stew) < 1e-4);
        assert!(
            cooked_growth_per_hour(&[raw.growth_per_hour], CookingMethod::Stew)
                < raw.growth_per_hour
        );
    }
    #[test]
    fn duration_uses_slowest_and_batch_size() {
        assert_eq!(
            cooking_duration_minutes(CookingMethod::Roast, &[5, 22], 0.5),
            Some(29)
        );
        assert!(
            cooking_duration_minutes(CookingMethod::Roast, &[5], 4.0)
                > cooking_duration_minutes(CookingMethod::Roast, &[5], 0.5)
        );
    }
    #[test]
    fn cooking_skill_only_reduces_overhead_and_bounds_quality() {
        let safety = [22];
        let novice =
            cooking_duration_minutes_for_check(CookingMethod::Roast, &safety, 4.0, 0.0).unwrap();
        let master =
            cooking_duration_minutes_for_check(CookingMethod::Roast, &safety, 4.0, 5.0).unwrap();
        assert!(master < novice);
        assert!(master >= 22);
        assert_eq!(cooked_nutrition_retention(-1.0), 0.95);
        assert!((cooked_nutrition_retention(5.0) - 0.99).abs() < f32::EPSILON);
        assert_eq!(cooked_quality_multiplier(f32::NAN), 0.95);
        assert!((cooked_quality_multiplier(5.0) - 1.10).abs() < f32::EPSILON);
    }

    #[test]
    fn cooked_output_cannot_reenter_the_value_multiplier() {
        assert!(is_cookable_ingredient("hazelnuts"));
        assert!(!is_cookable_ingredient("cooked_meal"));
        let ingredient_value = 100.0;
        let once = ingredient_value * cooked_quality_multiplier(5.0);
        assert_eq!(once, 110.0);
        assert!(!is_cookable_ingredient("cooked_meal"));
    }
    #[test]
    fn travel_never_creates_surplus_but_meals_can() {
        assert_eq!(travel_consumption(-100.0, 500.0), 100.0);
        assert_eq!(explicit_meal_consumption(-100.0, 500.0), 500.0);
        assert_eq!(explicit_meal_consumption(-1_000.0, 8_000.0), 4_000.0);
        assert_eq!(explicit_meal_consumption(f32::NAN, 500.0), 0.0);
    }

    #[test]
    fn standard_cooked_meal_is_positive_and_unique() {
        let meal = definition("cooked_meal").expect("standard cooked meal");
        assert_eq!(meal.class, FoodClass::MixedMeal);
        assert!(meal.kcal_per_unit > 0.0);
        assert!(meal.mass_kg_per_unit > 0.0);
        assert!(meal.value_per_unit > 0.0);
        assert!(meal.growth_per_hour > 0.0);
        assert_eq!(
            FOOD_CATALOG
                .iter()
                .filter(|food| food.id == "cooked_meal")
                .count(),
            1
        );
    }

    #[test]
    fn partial_lot_components_are_conserved() {
        for value in [0.25, 3.0, 2_500.0, 25.0] {
            let remaining = retained_component(value, 0.37);
            let consumed = retained_component(value, 0.63);
            assert!((remaining + consumed - value).abs() < 0.001);
        }
    }
}
