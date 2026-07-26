//! Framework-independent arithmetic for measured inventory definitions.
//!
//! The persistent schema described in `docs/MEASURED_INVENTORY.md` is not
//! implemented yet. This module is the small arithmetic boundary reducers can
//! adopt when that schema lands.

/// Canonical fixed-point magnitude of one full consumable unit.
pub const FULL_AMOUNT_MILLIUNITS: u32 = 1_000_000;

pub fn amount_for_fraction(numerator: u64, denominator: u64) -> Result<u32, MeasurementError> {
    if denominator == 0 {
        return Err(MeasurementError::ZeroCapacity);
    }
    let amount = u128::from(FULL_AMOUNT_MILLIUNITS)
        .checked_mul(u128::from(numerator))
        .ok_or(MeasurementError::Overflow)?
        .div_ceil(u128::from(denominator));
    u32::try_from(amount.min(u128::from(FULL_AMOUNT_MILLIUNITS)))
        .map_err(|_| MeasurementError::Overflow)
}

pub fn scaled_by_amount(full: u64, amount_milliunits: u32) -> u64 {
    ((u128::from(full) * u128::from(amount_milliunits)) / u128::from(FULL_AMOUNT_MILLIUNITS)) as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementKind {
    /// The item itself is consumed. An empty row has no remaining object.
    Depletable,
    /// A fungible quantity-one lot with no container tare.
    BulkLot,
    /// Contents are consumed while the recoverable container remains.
    Containerized,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasurementProfile {
    /// For a measured row, adapters supply the measured object's immutable
    /// kind snapshot here rather than re-reading the current item definition.
    pub kind: MeasurementKind,
    /// Standard immutable basis copied to an object when ordinary stock opens.
    pub standard_basis: MeasurementBasis,
}

/// Immutable integer basis for one measured object or derived lot.
///
/// Recipe outputs may use a basis different from the item definition's
/// standard basis. Remaining amount is mutable state and is not stored here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasurementBasis {
    /// Full contents in the definition's fixed-point unit.
    pub capacity: u64,
    /// Mass of full contents in the game's fixed mass subunit.
    pub full_contents_mass: u64,
    /// Intrinsic value of full contents in the game's currency subunit.
    pub full_contents_value: u64,
    /// Mass retained at zero contents. Must be zero for depletable items.
    pub tare_mass: u64,
    /// Intrinsic value retained at zero contents. Must be zero for depletable items.
    pub tare_value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredInventoryRow {
    /// Number of full unopened units, or exactly one for measured state.
    pub quantity: u32,
    /// `None` is a full unopened stack. `Some` is a quantity-one measured row.
    pub remaining_amount: Option<u64>,
    /// Immutable per-object basis. Required whenever measured state exists.
    pub instance_basis: Option<MeasurementBasis>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveTotals {
    pub mass: u64,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementError {
    ZeroCapacity,
    NonContainerHasTare,
    ZeroQuantity,
    MeasuredRowIsNotSingleton,
    BulkLotMustBeMeasuredSingleton,
    MissingInstanceBasis,
    UnexpectedInstanceBasis,
    AmountExceedsCapacity,
    InvalidPricingFactor,
    Overflow,
}

impl MeasurementProfile {
    pub fn validate(self) -> Result<Self, MeasurementError> {
        self.validate_basis(self.standard_basis)?;
        Ok(self)
    }

    fn validate_basis(self, basis: MeasurementBasis) -> Result<(), MeasurementError> {
        if basis.capacity == 0 {
            return Err(MeasurementError::ZeroCapacity);
        }
        if matches!(
            self.kind,
            MeasurementKind::Depletable | MeasurementKind::BulkLot
        ) && (basis.tare_mass != 0 || basis.tare_value != 0)
        {
            return Err(MeasurementError::NonContainerHasTare);
        }
        basis
            .tare_mass
            .checked_add(basis.full_contents_mass)
            .ok_or(MeasurementError::Overflow)?;
        basis
            .tare_value
            .checked_add(basis.full_contents_value)
            .ok_or(MeasurementError::Overflow)?;
        Ok(())
    }

    pub fn effective_totals(
        self,
        row: MeasuredInventoryRow,
    ) -> Result<EffectiveTotals, MeasurementError> {
        let profile = self.validate()?;
        if row.quantity == 0 {
            return Err(MeasurementError::ZeroQuantity);
        }
        if profile.kind == MeasurementKind::BulkLot
            && (row.quantity != 1 || row.remaining_amount.is_none())
        {
            return Err(MeasurementError::BulkLotMustBeMeasuredSingleton);
        }

        match row.remaining_amount {
            None => {
                if row.instance_basis.is_some() {
                    return Err(MeasurementError::UnexpectedInstanceBasis);
                }
                let basis = profile.standard_basis;
                let unit_mass = basis
                    .tare_mass
                    .checked_add(basis.full_contents_mass)
                    .ok_or(MeasurementError::Overflow)?;
                let unit_value = basis
                    .tare_value
                    .checked_add(basis.full_contents_value)
                    .ok_or(MeasurementError::Overflow)?;
                Ok(EffectiveTotals {
                    mass: unit_mass
                        .checked_mul(u64::from(row.quantity))
                        .ok_or(MeasurementError::Overflow)?,
                    value: unit_value
                        .checked_mul(u64::from(row.quantity))
                        .ok_or(MeasurementError::Overflow)?,
                })
            }
            Some(amount) => {
                if row.quantity != 1 {
                    return Err(MeasurementError::MeasuredRowIsNotSingleton);
                }
                let basis = row
                    .instance_basis
                    .ok_or(MeasurementError::MissingInstanceBasis)?;
                profile.validate_basis(basis)?;
                if amount > basis.capacity {
                    return Err(MeasurementError::AmountExceedsCapacity);
                }
                let contents_mass =
                    prorated_floor(basis.full_contents_mass, amount, basis.capacity)?;
                let contents_value =
                    prorated_floor(basis.full_contents_value, amount, basis.capacity)?;
                Ok(EffectiveTotals {
                    mass: basis
                        .tare_mass
                        .checked_add(contents_mass)
                        .ok_or(MeasurementError::Overflow)?,
                    value: basis
                        .tare_value
                        .checked_add(contents_value)
                        .ok_or(MeasurementError::Overflow)?,
                })
            }
        }
    }
}

/// Prorate once in widened integer arithmetic, rounding toward zero.
///
/// Applying this to partitions cannot make their summed result exceed the
/// unsplit result. Authoritative trade still prices complete rows so repeated
/// partitioning cannot exploit independently rounded merchant quotes.
pub fn prorated_floor(full: u64, amount: u64, capacity: u64) -> Result<u64, MeasurementError> {
    if capacity == 0 {
        return Err(MeasurementError::ZeroCapacity);
    }
    if amount > capacity {
        return Err(MeasurementError::AmountExceedsCapacity);
    }
    Ok(((u128::from(full) * u128::from(amount)) / u128::from(capacity)) as u64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PricingFactor {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceRounding {
    /// Merchant purchases an inventory line from the player.
    Floor,
    /// Merchant sells an inventory line to the player.
    Ceil,
}

/// Price one complete line after aggregating intrinsic row values.
///
/// Factors are reduced across each other before multiplication. The composed
/// ratio is applied once and rounded once in the requested direction.
pub fn checked_aggregate_price(
    row_intrinsic_values: &[u64],
    factors: &[PricingFactor],
    rounding: PriceRounding,
) -> Result<u64, MeasurementError> {
    let line_value = row_intrinsic_values
        .iter()
        .try_fold(0_u64, |total, value| {
            total.checked_add(*value).ok_or(MeasurementError::Overflow)
        })?;
    let mut numerator = 1_u128;
    let mut denominator = 1_u128;
    for factor in factors {
        if factor.numerator == 0 || factor.denominator == 0 {
            return Err(MeasurementError::InvalidPricingFactor);
        }
        let mut next_numerator = u128::from(factor.numerator);
        let mut next_denominator = u128::from(factor.denominator);

        let cancel_numerator = greatest_common_divisor(next_numerator, denominator);
        next_numerator /= cancel_numerator;
        denominator /= cancel_numerator;

        let cancel_denominator = greatest_common_divisor(next_denominator, numerator);
        next_denominator /= cancel_denominator;
        numerator /= cancel_denominator;

        numerator = numerator
            .checked_mul(next_numerator)
            .ok_or(MeasurementError::Overflow)?;
        denominator = denominator
            .checked_mul(next_denominator)
            .ok_or(MeasurementError::Overflow)?;
    }

    let mut reduced_line = u128::from(line_value);
    let cancel_line = greatest_common_divisor(reduced_line, denominator);
    reduced_line /= cancel_line;
    denominator /= cancel_line;
    let scaled = reduced_line
        .checked_mul(numerator)
        .ok_or(MeasurementError::Overflow)?;
    let quotient = scaled / denominator;
    let remainder = scaled % denominator;
    let rounded = match rounding {
        PriceRounding::Floor => quotient,
        PriceRounding::Ceil if remainder == 0 => quotient,
        PriceRounding::Ceil => quotient.checked_add(1).ok_or(MeasurementError::Overflow)?,
    };
    u64::try_from(rounded).map_err(|_| MeasurementError::Overflow)
}

const fn greatest_common_divisor(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soap() -> MeasurementProfile {
        MeasurementProfile {
            kind: MeasurementKind::Depletable,
            standard_basis: MeasurementBasis {
                capacity: 100,
                full_contents_mass: 250,
                full_contents_value: 17,
                tare_mass: 0,
                tare_value: 0,
            },
        }
    }

    #[test]
    fn fixed_point_fraction_rounds_up_and_caps_at_one_unit() {
        assert_eq!(amount_for_fraction(1, 25), Ok(40_000));
        assert_eq!(amount_for_fraction(25, 100), Ok(250_000));
        assert_eq!(amount_for_fraction(2, 1), Ok(FULL_AMOUNT_MILLIUNITS));
        assert_eq!(
            amount_for_fraction(1, 0),
            Err(MeasurementError::ZeroCapacity)
        );
    }

    #[test]
    fn fixed_point_scaling_rounds_down() {
        assert_eq!(scaled_by_amount(425, 500_000), 212);
        assert_eq!(scaled_by_amount(30, FULL_AMOUNT_MILLIUNITS), 30);
    }

    fn bottle() -> MeasurementProfile {
        MeasurementProfile {
            kind: MeasurementKind::Containerized,
            standard_basis: MeasurementBasis {
                capacity: 750,
                full_contents_mass: 750,
                full_contents_value: 11,
                tare_mass: 400,
                tare_value: 3,
            },
        }
    }

    fn unopened(quantity: u32) -> MeasuredInventoryRow {
        MeasuredInventoryRow {
            quantity,
            remaining_amount: None,
            instance_basis: None,
        }
    }

    fn measured(profile: MeasurementProfile, amount: u64) -> MeasuredInventoryRow {
        MeasuredInventoryRow {
            quantity: 1,
            remaining_amount: Some(amount),
            instance_basis: Some(profile.standard_basis),
        }
    }

    #[test]
    fn depletable_partial_scales_mass_and_value() {
        let totals = soap().effective_totals(measured(soap(), 40)).unwrap();
        assert_eq!(
            totals,
            EffectiveTotals {
                mass: 100,
                value: 6
            }
        );
    }

    #[test]
    fn containerized_empty_and_partial_retain_tare() {
        let empty = bottle().effective_totals(measured(bottle(), 0)).unwrap();
        let partial = bottle().effective_totals(measured(bottle(), 375)).unwrap();
        assert_eq!(
            empty,
            EffectiveTotals {
                mass: 400,
                value: 3
            }
        );
        assert_eq!(
            partial,
            EffectiveTotals {
                mass: 775,
                value: 8
            }
        );
    }

    #[test]
    fn unopened_stacks_multiply_full_unit_totals() {
        let totals = bottle().effective_totals(unopened(4)).unwrap();
        assert_eq!(
            totals,
            EffectiveTotals {
                mass: 4_600,
                value: 56
            }
        );
    }

    #[test]
    fn opened_full_matches_one_unopened_unit() {
        let unopened = bottle().effective_totals(unopened(1)).unwrap();
        let opened = bottle()
            .effective_totals(measured(bottle(), bottle().standard_basis.capacity))
            .unwrap();
        assert_eq!(opened, unopened);
    }

    #[test]
    fn invalid_profiles_and_rows_are_rejected() {
        assert_eq!(
            MeasurementProfile {
                standard_basis: MeasurementBasis {
                    capacity: 0,
                    ..soap().standard_basis
                },
                ..soap()
            }
            .validate(),
            Err(MeasurementError::ZeroCapacity)
        );
        assert_eq!(
            MeasurementProfile {
                standard_basis: MeasurementBasis {
                    tare_mass: 1,
                    ..soap().standard_basis
                },
                ..soap()
            }
            .validate(),
            Err(MeasurementError::NonContainerHasTare)
        );
        assert_eq!(
            soap().effective_totals(MeasuredInventoryRow {
                quantity: 2,
                remaining_amount: Some(50),
                instance_basis: Some(soap().standard_basis),
            }),
            Err(MeasurementError::MeasuredRowIsNotSingleton)
        );
        assert_eq!(
            soap().effective_totals(measured(soap(), 101)),
            Err(MeasurementError::AmountExceedsCapacity)
        );
        assert_eq!(
            soap().effective_totals(unopened(0)),
            Err(MeasurementError::ZeroQuantity)
        );
        assert_eq!(prorated_floor(1, 0, 0), Err(MeasurementError::ZeroCapacity));
    }

    #[test]
    fn overflow_is_reported_instead_of_saturating() {
        let profile = MeasurementProfile {
            kind: MeasurementKind::Containerized,
            standard_basis: MeasurementBasis {
                capacity: 1,
                full_contents_mass: u64::MAX,
                full_contents_value: 1,
                tare_mass: 0,
                tare_value: 0,
            },
        };
        assert_eq!(
            profile.effective_totals(unopened(2)),
            Err(MeasurementError::Overflow)
        );
        assert_eq!(
            MeasurementProfile {
                standard_basis: MeasurementBasis {
                    tare_mass: 1,
                    ..profile.standard_basis
                },
                ..profile
            }
            .validate(),
            Err(MeasurementError::Overflow)
        );
    }

    #[test]
    fn mass_and_value_are_monotonic() {
        let mut previous = EffectiveTotals { mass: 0, value: 0 };
        for amount in 0..=bottle().standard_basis.capacity {
            let current = bottle()
                .effective_totals(measured(bottle(), amount))
                .unwrap();
            assert!(current.mass >= previous.mass);
            assert!(current.value >= previous.value);
            previous = current;
        }
    }

    #[test]
    fn measured_rows_use_their_immutable_instance_basis() {
        let derived_basis = MeasurementBasis {
            capacity: 600,
            full_contents_mass: 900,
            full_contents_value: 20,
            tare_mass: 0,
            tare_value: 0,
        };
        let totals = soap()
            .effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(300),
                instance_basis: Some(derived_basis),
            })
            .unwrap();
        assert_eq!(
            totals,
            EffectiveTotals {
                mass: 450,
                value: 10
            }
        );
        assert_eq!(
            soap().effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(50),
                instance_basis: None,
            }),
            Err(MeasurementError::MissingInstanceBasis)
        );
    }

    #[test]
    fn bulk_lots_are_always_measured_singletons() {
        let profile = MeasurementProfile {
            kind: MeasurementKind::BulkLot,
            standard_basis: soap().standard_basis,
        };
        assert_eq!(
            profile.effective_totals(unopened(1)),
            Err(MeasurementError::BulkLotMustBeMeasuredSingleton)
        );
        assert_eq!(
            profile.effective_totals(unopened(3)),
            Err(MeasurementError::BulkLotMustBeMeasuredSingleton)
        );
        assert!(
            profile
                .effective_totals(measured(profile, profile.standard_basis.capacity))
                .is_ok()
        );
    }

    #[test]
    fn partition_rounding_cannot_create_contents_value() {
        let whole = prorated_floor(11, 750, 750).unwrap();
        let parts = [1_u64, 100, 249, 400]
            .into_iter()
            .map(|amount| prorated_floor(11, amount, 750).unwrap())
            .sum::<u64>();
        assert_eq!(1 + 100 + 249 + 400, 750);
        assert!(parts <= whole);

        for split in 0..=750 {
            let left = prorated_floor(11, split, 750).unwrap();
            let right = prorated_floor(11, 750 - split, 750).unwrap();
            assert!(left + right <= whole);
        }
    }

    #[test]
    fn aggregate_pricing_rounds_once_in_the_required_direction() {
        let factors = [
            PricingFactor {
                numerator: 3,
                denominator: 2,
            },
            PricingFactor {
                numerator: 5,
                denominator: 3,
            },
        ];
        assert_eq!(
            checked_aggregate_price(&[1], &factors, PriceRounding::Floor),
            Ok(2)
        );
        assert_eq!(
            checked_aggregate_price(&[1], &factors, PriceRounding::Ceil),
            Ok(3)
        );
        assert_eq!(
            checked_aggregate_price(&[2, 3], &factors, PriceRounding::Floor),
            Ok(12)
        );
        assert_eq!(
            checked_aggregate_price(&[2, 3], &factors, PriceRounding::Ceil),
            Ok(13)
        );
        assert_eq!(
            checked_aggregate_price(
                &[7],
                &[
                    PricingFactor {
                        numerator: u64::MAX,
                        denominator: u64::MAX,
                    },
                    PricingFactor {
                        numerator: u64::MAX,
                        denominator: u64::MAX,
                    },
                ],
                PriceRounding::Floor
            ),
            Ok(7)
        );
    }

    #[test]
    fn split_sale_cannot_create_player_proceeds() {
        let factors = [PricingFactor {
            numerator: 3,
            denominator: 2,
        }];
        let aggregate = checked_aggregate_price(&[1, 1], &factors, PriceRounding::Floor).unwrap();
        let split = checked_aggregate_price(&[1], &factors, PriceRounding::Floor).unwrap()
            + checked_aggregate_price(&[1], &factors, PriceRounding::Floor).unwrap();
        assert_eq!(aggregate, 3);
        assert_eq!(split, 2);
        assert!(split <= aggregate);

        let aggregate_sale =
            checked_aggregate_price(&[1, 1], &factors, PriceRounding::Ceil).unwrap();
        let split_sale = checked_aggregate_price(&[1], &factors, PriceRounding::Ceil).unwrap()
            + checked_aggregate_price(&[1], &factors, PriceRounding::Ceil).unwrap();
        assert_eq!(aggregate_sale, 3);
        assert_eq!(split_sale, 4);
    }

    #[test]
    fn pricing_rejects_invalid_factors_and_overflow() {
        assert_eq!(
            checked_aggregate_price(
                &[1],
                &[PricingFactor {
                    numerator: 1,
                    denominator: 0,
                }],
                PriceRounding::Floor
            ),
            Err(MeasurementError::InvalidPricingFactor)
        );
        assert_eq!(
            checked_aggregate_price(
                &[1],
                &[PricingFactor {
                    numerator: 0,
                    denominator: 1,
                }],
                PriceRounding::Floor
            ),
            Err(MeasurementError::InvalidPricingFactor)
        );
        assert_eq!(
            checked_aggregate_price(&[u64::MAX, 1], &[], PriceRounding::Floor),
            Err(MeasurementError::Overflow)
        );
        assert_eq!(
            checked_aggregate_price(
                &[u64::MAX],
                &[PricingFactor {
                    numerator: u64::MAX,
                    denominator: 1,
                }],
                PriceRounding::Ceil
            ),
            Err(MeasurementError::Overflow)
        );
        assert_eq!(
            checked_aggregate_price(
                &[1],
                &[
                    PricingFactor {
                        numerator: u64::MAX,
                        denominator: 1,
                    },
                    PricingFactor {
                        numerator: u64::MAX,
                        denominator: 1,
                    },
                    PricingFactor {
                        numerator: u64::MAX,
                        denominator: 1,
                    },
                ],
                PriceRounding::Floor
            ),
            Err(MeasurementError::Overflow)
        );
    }
}
