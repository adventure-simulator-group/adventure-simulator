//! Pure food-lot, spoilage, meal, and cooking rules.

use serde::{Deserialize, Serialize};

pub const MAX_MEAL_FULLNESS_KCAL: f32 = 3_000.0;
pub const MIN_INITIAL_CONTAMINATION: f32 = 1.0e-8;
pub const MAX_INITIAL_CONTAMINATION: f32 = 1.0e-5;
pub const RECONTAMINATION_FLOOR: f32 = 1.0e-9;
pub const MAX_CONTAMINATION: f32 = 1.0e9;
pub const PAN_FRY_MIN_FAT_MASS_RATIO: f32 = 0.02;

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

/// Flavor potency in mass-equivalent kilograms. A value of 0.1 means enough
/// of that flavor to season 0.1 kg of food at the shared objective target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlavorProfile {
    pub salty: f32,
    pub spicy: f32,
    pub sweet: f32,
    pub sour: f32,
    pub savory: f32,
}

impl FlavorProfile {
    pub const fn new(salty: f32, spicy: f32, sweet: f32, sour: f32, savory: f32) -> Self {
        Self {
            salty,
            spicy,
            sweet,
            sour,
            savory,
        }
    }

    pub fn scaled(self, factor: f32) -> Self {
        let factor = if factor.is_finite() {
            factor.max(0.0)
        } else {
            0.0
        };
        Self::new(
            self.salty * factor,
            self.spicy * factor,
            self.sweet * factor,
            self.sour * factor,
            self.savory * factor,
        )
    }

    pub fn add_assign(&mut self, other: Self) {
        self.salty += other.salty;
        self.spicy += other.spicy;
        self.sweet += other.sweet;
        self.sour += other.sour;
        self.savory += other.savory;
    }

    pub fn valid(self) -> bool {
        [self.salty, self.spicy, self.sweet, self.sour, self.savory]
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
    }
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
    pub flavors_per_unit: FlavorProfile,
    pub culinary_fat: bool,
    /// Quality of catalog stock and newly acquired raw lots.
    pub default_quality: u8,
}

pub static FOOD_CATALOG: std::sync::LazyLock<Vec<FoodDefinition>> =
    std::sync::LazyLock::new(|| {
        crate::item_catalog::catalog()
            .iter()
            .filter_map(|item| {
                let food = item.capabilities.food.as_ref()?;
                let class = match food.class.as_str() {
                    "ration" => FoodClass::Ration,
                    "grain" => FoodClass::Grain,
                    "bread" => FoodClass::Bread,
                    "fruit" => FoodClass::Fruit,
                    "berries" => FoodClass::Berries,
                    "vegetable" => FoodClass::Vegetable,
                    "nuts" => FoodClass::Nuts,
                    "herb" => FoodClass::Herb,
                    "mushroom" => FoodClass::Mushroom,
                    "raw_meat" => FoodClass::RawMeat,
                    "cooked_meat" => FoodClass::CookedMeat,
                    "mixed_meal" => FoodClass::MixedMeal,
                    _ => unreachable!("validated food class"),
                };
                Some(FoodDefinition {
                    id: item.id.as_str(),
                    name: item.display_name.as_str(),
                    class,
                    kcal_per_unit: food.nutrition_kcal,
                    mass_kg_per_unit: item.weight_kg,
                    value_per_unit: food.value_per_unit,
                    growth_per_hour: food.growth_per_hour,
                    cooking_minutes: food.cooking_minutes,
                    flavors_per_unit: FlavorProfile::new(
                        food.flavors_kg.salty,
                        food.flavors_kg.spicy,
                        food.flavors_kg.sweet,
                        food.flavors_kg.sour,
                        food.flavors_kg.savory,
                    ),
                    culinary_fat: food.culinary_fat,
                    default_quality: food.quality,
                })
            })
            .collect()
    });

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
        CookingMethod::Bake => 30,
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

/// Method-specific retention composed with generic skill-based retention.
pub fn method_nutrition_retention(method: CookingMethod) -> f32 {
    match method {
        CookingMethod::Roast => 0.85,
        _ => 1.0,
    }
}

pub fn quality_value_multiplier(quality: u8) -> f32 {
    match quality.clamp(1, 5) {
        1 => 0.80,
        2 => 0.90,
        3 => 1.00,
        4 => 1.15,
        _ => 1.35,
    }
}

pub fn pan_fry_has_enough_fat(culinary_fat_kg: f32, ingredient_mass_kg: f32) -> bool {
    culinary_fat_kg.is_finite()
        && ingredient_mass_kg.is_finite()
        && ingredient_mass_kg > 0.0
        && culinary_fat_kg >= ingredient_mass_kg * PAN_FRY_MIN_FAT_MASS_RATIO
}

fn flavor_score(actual: f32, target: f32) -> f32 {
    if !actual.is_finite() || !target.is_finite() || actual < 0.0 || target <= 0.0 {
        return 0.0;
    }
    let ratio = actual / target;
    if ratio <= 1.0 {
        5.0 * ratio
    } else {
        5.0 / (ratio * ratio)
    }
}

/// Shared objective flavor score. Each method has fixed required targets,
/// including a zero score when a required flavor is absent. Baking
/// deterministically chooses sweet or savory, whichever has greater potency.
/// Every required flavor is weighted equally and targets potency equal to mass.
pub fn aggregate_flavor_quality(
    method: CookingMethod,
    flavors: FlavorProfile,
    mass_kg: f32,
) -> f32 {
    if !flavors.valid() || !mass_kg.is_finite() || mass_kg <= 0.0 {
        return 0.0;
    }
    let mut active = Vec::new();
    let push = |scores: &mut Vec<f32>, value: f32| {
        scores.push(flavor_score(value, mass_kg));
    };
    match method {
        CookingMethod::Bake if flavors.sweet >= flavors.savory => {
            push(&mut active, flavors.salty);
            push(&mut active, flavors.spicy);
            push(&mut active, flavors.sweet);
        }
        CookingMethod::Bake => {
            push(&mut active, flavors.salty);
            push(&mut active, flavors.spicy);
            push(&mut active, flavors.savory);
        }
        CookingMethod::Stew => {
            push(&mut active, flavors.salty);
            push(&mut active, flavors.spicy);
            push(&mut active, flavors.sour);
            push(&mut active, flavors.savory);
        }
        CookingMethod::PanFry | CookingMethod::Roast => {
            push(&mut active, flavors.salty);
            push(&mut active, flavors.spicy);
            push(&mut active, flavors.savory);
        }
    }
    active.iter().sum::<f32>() / active.len() as f32
}

/// Checks below one occupy novice tier 1 in the five-tier item system.
pub fn chef_quality_tier(cooking_check: f32) -> u8 {
    if cooking_check.is_finite() {
        (cooking_check.floor() as i32).clamp(1, 5) as u8
    } else {
        1
    }
}

/// Flavor is floored before the discrete chef cap. Tier 1 is the system floor.
pub fn cooked_quality(chef_tier: u8, flavor_quality: f32, fatless_pan_fry: bool) -> u8 {
    let flavor = if flavor_quality.is_finite() {
        flavor_quality.clamp(0.0, 5.0)
    } else {
        0.0
    };
    let mut tier = i32::from(chef_tier.clamp(1, 5))
        .min(flavor.floor() as i32)
        .max(1);
    if fatless_pan_fry {
        tier -= 1;
    }
    tier.clamp(1, 5) as u8
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
    fn cooking_skill_only_reduces_overhead_and_bounds_retention() {
        let safety = [22];
        let novice =
            cooking_duration_minutes_for_check(CookingMethod::Roast, &safety, 4.0, 0.0).unwrap();
        let master =
            cooking_duration_minutes_for_check(CookingMethod::Roast, &safety, 4.0, 5.0).unwrap();
        assert!(master < novice);
        assert!(master >= 22);
        assert_eq!(cooked_nutrition_retention(-1.0), 0.95);
        assert!((cooked_nutrition_retention(5.0) - 0.99).abs() < f32::EPSILON);
        assert_eq!(method_nutrition_retention(CookingMethod::Roast), 0.85);
        assert_eq!(method_nutrition_retention(CookingMethod::Bake), 1.0);
    }

    #[test]
    fn cooked_output_cannot_reenter_the_value_multiplier() {
        assert!(is_cookable_ingredient("hazelnuts"));
        assert!(!is_cookable_ingredient("cooked_meal"));
        let ingredient_value = 100.0;
        let once = ingredient_value * quality_value_multiplier(5);
        assert_eq!(once, 135.0);
        assert!(!is_cookable_ingredient("cooked_meal"));
    }

    #[test]
    fn flavor_scoring_is_linear_below_and_quadratic_above_target() {
        let exact = FlavorProfile::new(1.0, 1.0, 0.0, 0.0, 1.0);
        let low = FlavorProfile::new(0.5, 0.5, 0.0, 0.0, 0.5);
        let high = FlavorProfile::new(2.0, 2.0, 0.0, 0.0, 2.0);
        assert_eq!(
            aggregate_flavor_quality(CookingMethod::Roast, exact, 1.0),
            5.0
        );
        assert_eq!(
            aggregate_flavor_quality(CookingMethod::Roast, low, 1.0),
            2.5
        );
        assert_eq!(
            aggregate_flavor_quality(CookingMethod::Roast, high, 1.0),
            1.25
        );
    }

    #[test]
    fn quality_obeys_chef_and_flavor_caps_and_fatless_penalty() {
        assert_eq!(chef_quality_tier(0.2), 1);
        assert_eq!(chef_quality_tier(4.8), 4);
        assert_eq!(cooked_quality(4, 3.9, false), 3);
        assert_eq!(cooked_quality(4, 3.9, true), 2);
        assert_eq!(cooked_quality(1, 5.0, true), 1);
    }

    #[test]
    fn required_flavors_do_not_disappear_when_omitted() {
        let omitted = FlavorProfile::new(0.0, 1.0, 0.0, 0.0, 1.0);
        let undersalted = FlavorProfile::new(0.5, 1.0, 0.0, 0.0, 1.0);
        assert!(
            aggregate_flavor_quality(CookingMethod::Roast, omitted, 1.0)
                < aggregate_flavor_quality(CookingMethod::Roast, undersalted, 1.0)
        );
    }

    #[test]
    fn pan_fry_fat_threshold_is_inclusive_and_validated() {
        assert!(!pan_fry_has_enough_fat(0.019, 1.0));
        assert!(pan_fry_has_enough_fat(0.02, 1.0));
        assert!(!pan_fry_has_enough_fat(f32::NAN, 1.0));
        assert!(!pan_fry_has_enough_fat(1.0, 0.0));
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
    fn forage_meats_have_complete_raw_food_definitions() {
        for id in ["raw_venison", "raw_fowl", "raw_fish", "raw_beast_meat"] {
            let food = definition(id).unwrap();
            assert_eq!(food.class, FoodClass::RawMeat);
            assert!(food.kcal_per_unit > 0.0);
            assert!(food.mass_kg_per_unit > 0.0);
            assert!(food.value_per_unit > 0.0);
            assert!(food.growth_per_hour > 0.0);
            assert!(food.cooking_minutes > 0);
            assert!(food.flavors_per_unit.valid());
            assert!((1..=5).contains(&food.default_quality));
        }
    }

    #[test]
    fn catalog_flavor_metadata_is_valid_and_salt_uses_exact_calibration() {
        for item in FOOD_CATALOG.iter() {
            assert!(item.flavors_per_unit.valid(), "{}", item.id);
            assert!((1..=5).contains(&item.default_quality), "{}", item.id);
        }
        assert_eq!(definition("salt").unwrap().flavors_per_unit.salty, 1.0);
        assert_eq!(definition("salt").unwrap().mass_kg_per_unit, 0.01);
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
