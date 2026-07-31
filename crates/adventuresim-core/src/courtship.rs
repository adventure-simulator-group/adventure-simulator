//! Pure rules for strategic relationships, housing, and family growth.
//!
//! Persistence deliberately lives in the strategic module.  Keeping the
//! thresholds and clock arithmetic here makes reducers deterministic and lets
//! callers advance a long interval in exactly the same way as smaller chunks.

use crate::strategic_time::{MINUTES_PER_DAY, MINUTES_PER_YEAR};

pub const ADULT_AGE_YEARS: u16 = 16;
pub const FORMAL_COURTSHIP_AFFINITY: f32 = 45.0;
pub const FORMAL_FATHER_APPROVAL_AFFINITY: f32 = 35.0;
pub const INFORMAL_COURTSHIP_AFFINITY: f32 = 75.0;
pub const AMOROUS_INFORMAL_MODIFIER: f32 = -10.0;
pub const PROPER_INFORMAL_MODIFIER: f32 = 10.0;
pub const WEDDING_NOTICE_MINUTES: u64 = MINUTES_PER_YEAR;
pub const GESTATION_MINUTES: u64 = 280 * MINUTES_PER_DAY;
pub const SOCIALIZING_QUANTUM_MINUTES: u16 = 15;

/// Named rather than magic-number housing economy.  All amounts are paid in
/// the settlement's ordinary inventory currency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HousingEconomy {
    pub purchase_price: u32,
    pub rent_per_30_days: u32,
    pub owner_maintenance_per_30_days: u32,
    pub property_tax_per_30_days: u32,
    pub leisure_morale_basis_points: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HousingTier {
    Cheap,
    Moderate,
    Fancy,
}

impl HousingTier {
    pub const ALL: [Self; 3] = [Self::Cheap, Self::Moderate, Self::Fancy];

    pub const fn economy(self) -> HousingEconomy {
        match self {
            Self::Cheap => HousingEconomy {
                purchase_price: 120,
                rent_per_30_days: 8,
                owner_maintenance_per_30_days: 2,
                property_tax_per_30_days: 2,
                leisure_morale_basis_points: 11_000,
            },
            Self::Moderate => HousingEconomy {
                purchase_price: 500,
                rent_per_30_days: 24,
                owner_maintenance_per_30_days: 6,
                property_tax_per_30_days: 6,
                leisure_morale_basis_points: 13_000,
            },
            Self::Fancy => HousingEconomy {
                purchase_price: 1_800,
                rent_per_30_days: 70,
                owner_maintenance_per_30_days: 16,
                property_tax_per_30_days: 18,
                leisure_morale_basis_points: 16_000,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtshipDisposition {
    Amorous,
    Neutral,
    Proper,
}

pub const fn informal_affinity_threshold(disposition: CourtshipDisposition) -> f32 {
    match disposition {
        CourtshipDisposition::Amorous => INFORMAL_COURTSHIP_AFFINITY + AMOROUS_INFORMAL_MODIFIER,
        CourtshipDisposition::Neutral => INFORMAL_COURTSHIP_AFFINITY,
        CourtshipDisposition::Proper => INFORMAL_COURTSHIP_AFFINITY + PROPER_INFORMAL_MODIFIER,
    }
}

/// A stable Bernoulli trial keyed by a whole calendar day.  `numerator` is in
/// ten-thousandths; callers choose a hash-derived value in the same range.
pub const fn succeeds_daily_trial(day_entropy: u16, numerator: u16) -> bool {
    day_entropy < numerator
}

/// Count daily event boundaries crossed by an interval.  The event's state is
/// keyed by that day, so callers can process each returned day exactly once.
pub fn crossed_days(start_minute: u64, end_minute: u64) -> impl Iterator<Item = u64> {
    let first = start_minute / MINUTES_PER_DAY;
    let last = end_minute / MINUTES_PER_DAY;
    (first + 1)..=last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn courtship_trait_thresholds_are_ordered() {
        assert!(informal_affinity_threshold(CourtshipDisposition::Amorous)
            < informal_affinity_threshold(CourtshipDisposition::Neutral));
        assert!(informal_affinity_threshold(CourtshipDisposition::Neutral)
            < informal_affinity_threshold(CourtshipDisposition::Proper));
    }

    #[test]
    fn day_boundaries_are_chunk_invariant() {
        let whole: Vec<_> = crossed_days(20, 3 * MINUTES_PER_DAY + 20).collect();
        let mut split = crossed_days(20, MINUTES_PER_DAY + 20).collect::<Vec<_>>();
        split.extend(crossed_days(MINUTES_PER_DAY + 20, 3 * MINUTES_PER_DAY + 20));
        assert_eq!(whole, split);
    }
}
