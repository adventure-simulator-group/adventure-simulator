//! Authoritative strategic merchant quotes shared by reducers and simulators.

pub const MERCHANT_MARGIN: f32 = 1.25;
pub const SALES_TAX: f32 = 0.10;

pub fn merchant_buy_price(base_value: u32) -> u32 {
    (base_value as f32 * MERCHANT_MARGIN * (1.0 + SALES_TAX)).ceil() as u32
}

pub fn merchant_sell_price(base_value: u32) -> u32 {
    (base_value as f32 / MERCHANT_MARGIN).floor().max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_preserve_a_positive_spread() {
        assert!(merchant_buy_price(100) > merchant_sell_price(100));
    }
}
