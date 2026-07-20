//! Authoritative strategic merchant quotes shared by reducers and simulators.

use crate::disease::MedicationRecipe;

pub const MERCHANT_MARGIN: f32 = 1.25;
pub const SALES_TAX: f32 = 0.10;
pub const NPC_HERBALIST_EXAM_FEE: u32 = 25;
pub const HERBALIST_MEDICATION_PREMIUM: f32 = 1.50;

/// Canonical ingredient values used by both item seeding and herbalist quotes.
pub const MEDICINAL_INGREDIENT_VALUES: [(&str, u32); 11] = [
    ("honey", 2),
    ("sage", 2),
    ("dried_mint", 2),
    ("charcoal", 1),
    ("willow_bark", 2),
    ("vinegar", 2),
    ("poppy", 4),
    ("comfrey", 2),
    ("garlic", 1),
    ("oatmeal", 1),
    ("rosewater", 3),
];

pub fn merchant_buy_price(base_value: u32) -> u32 {
    (base_value as f32 * MERCHANT_MARGIN * (1.0 + SALES_TAX)).ceil() as u32
}

pub fn merchant_sell_price(base_value: u32) -> u32 {
    (base_value as f32 / MERCHANT_MARGIN).floor().max(1.0) as u32
}

/// Checked line extension used by every authoritative merchant total. A quote
/// that cannot be represented is rejected instead of being silently capped.
pub fn checked_merchant_line_total(unit_price: u32, quantity: u32) -> Option<u64> {
    u64::from(unit_price).checked_mul(u64::from(quantity))
}

pub fn checked_add_merchant_total(total: u64, line: u64) -> Option<u64> {
    total.checked_add(line)
}

/// Split a party purchase between shared coin and the acting character's coin.
/// Shared funds are spent first so personal funds only cover the shortfall.
pub fn split_party_purchase_payment(
    party_coins: u64,
    personal_coins: u64,
    amount: u64,
) -> Option<(u64, u64)> {
    if party_coins.saturating_add(personal_coins) < amount {
        return None;
    }
    let pooled = amount.min(party_coins);
    Some((pooled, amount - pooled))
}

pub fn medicinal_ingredient_value(item_id: &str) -> Option<u32> {
    MEDICINAL_INGREDIENT_VALUES
        .iter()
        .find_map(|(id, value)| (*id == item_id).then_some(*value))
}

pub fn medication_ingredient_merchant_cost(recipe: &MedicationRecipe) -> u32 {
    recipe
        .ingredients
        .iter()
        .map(|ingredient| {
            merchant_buy_price(
                medicinal_ingredient_value(ingredient.item_id)
                    .expect("every medication ingredient has an authoritative value"),
            ) * ingredient.quantity
        })
        .sum()
}

/// Herbalists charge for their preparation and guaranteed stock as well as the
/// ingredients. The one-coin floor keeps the premium strict after rounding.
pub fn herbalist_medication_price(recipe: &MedicationRecipe) -> u32 {
    let ingredients = medication_ingredient_merchant_cost(recipe);
    ((ingredients as f32 * HERBALIST_MEDICATION_PREMIUM).ceil() as u32)
        .max(ingredients.saturating_add(1))
}

/// Stable settlement service skill. Herbalists are useful generalists, never
/// master (Medicine 5) apothecaries.
pub fn settlement_herbalist_medicine_skill(settlement_id: &str) -> u8 {
    let mut hash = 0xcbf29ce484222325_u64 ^ 0x4845_5242_414c_4953;
    for byte in settlement_id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    2 + (hash % 3) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_preserve_a_positive_spread() {
        assert!(merchant_buy_price(100) > merchant_sell_price(100));
    }

    #[test]
    fn party_purchases_use_pooled_coin_then_personal_coin() {
        assert_eq!(split_party_purchase_payment(8, 20, 15), Some((8, 7)));
        assert_eq!(split_party_purchase_payment(20, 8, 15), Some((15, 0)));
        assert_eq!(split_party_purchase_payment(4, 5, 10), None);
    }

    #[test]
    fn merchant_totals_are_checked_in_u64_space() {
        assert_eq!(
            checked_merchant_line_total(u32::MAX, u32::MAX),
            Some(u64::from(u32::MAX) * u64::from(u32::MAX))
        );
        assert_eq!(checked_add_merchant_total(u64::MAX, 1), None);
    }

    #[test]
    fn every_prepared_course_costs_more_than_its_merchant_ingredients() {
        for recipe in crate::disease::MEDICATION_RECIPES {
            assert!(
                herbalist_medication_price(&recipe) > medication_ingredient_merchant_cost(&recipe),
                "{} needs a strict NPC preparation premium",
                recipe.name
            );
        }
    }

    #[test]
    fn herbalist_skill_is_deterministic_and_never_master_rank() {
        let first = settlement_herbalist_medicine_skill("riverdale");
        assert_eq!(first, settlement_herbalist_medicine_skill("riverdale"));
        for id in [
            "riverdale",
            "ironforge",
            "willowmere",
            "Lubeck",
            "St. John's",
        ] {
            assert!((2..=4).contains(&settlement_herbalist_medicine_skill(id)));
        }
    }
}
