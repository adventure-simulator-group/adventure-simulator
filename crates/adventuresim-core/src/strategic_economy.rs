//! Authoritative strategic merchant quotes shared by reducers and simulators.

pub const MERCHANT_MARGIN: f32 = 1.25;
pub const SALES_TAX: f32 = 0.10;
pub const NPC_HERBALIST_EXAM_FEE: u32 = 25;
pub const INN_FULL_BOARD_GOLD_PER_DAY: u32 = 2;

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

/// Charges each complete inn day once and rounds any partial day up once.
pub fn inn_full_board_cost(requested_minutes: u64) -> Option<u64> {
    let minutes_per_day = crate::strategic_time::MINUTES_PER_DAY;
    let complete_days = requested_minutes / minutes_per_day;
    let partial_day = u64::from(requested_minutes % minutes_per_day != 0);
    complete_days
        .checked_add(partial_day)?
        .checked_mul(u64::from(INN_FULL_BOARD_GOLD_PER_DAY))
}

pub fn merchant_sell_price(base_value: u32) -> u32 {
    (base_value as f32 / MERCHANT_MARGIN).floor().max(1.0) as u32
}

pub fn language_adjusted_buy_price(price: u32, shared_language: f32) -> u32 {
    let coefficient = if shared_language.is_finite() {
        shared_language.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (price as f32 * (1.0 + (1.0 - coefficient) * 0.25)).ceil() as u32
}

pub fn language_adjusted_sell_price(price: u32, shared_language: f32) -> u32 {
    if price == 0 {
        return 0;
    }
    let coefficient = if shared_language.is_finite() {
        shared_language.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (price as f32 * (1.0 - (1.0 - coefficient) * 0.20))
        .floor()
        .max(1.0) as u32
}

/// Food lots can have fractional remaining value after a meal. Unlike a whole
/// item quote, a sub-coin remainder may sell for zero and must never be rounded
/// back up into a fresh unit of value.
pub fn merchant_sell_food_lot_value(total_value: f32) -> Option<u64> {
    if !total_value.is_finite() || total_value < 0.0 {
        return None;
    }
    Some((total_value / MERCHANT_MARGIN).floor() as u64)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_preserve_a_positive_spread() {
        assert!(merchant_buy_price(100) > merchant_sell_price(100));
    }

    #[test]
    fn language_penalty_hurts_both_sides_of_a_trade() {
        assert!(language_adjusted_buy_price(100, 0.0) > language_adjusted_buy_price(100, 1.0));
        assert!(language_adjusted_sell_price(100, 0.0) < language_adjusted_sell_price(100, 1.0));
        assert_eq!(language_adjusted_sell_price(0, 0.0), 0);
        assert_eq!(language_adjusted_sell_price(1, 0.0), 1);
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
    fn partial_food_sale_never_recreates_consumed_value() {
        assert_eq!(merchant_sell_food_lot_value(3.0), Some(2));
        assert_eq!(merchant_sell_food_lot_value(0.75), Some(0));
        assert_eq!(merchant_sell_food_lot_value(f32::NAN), None);
    }
}
