//! Framework-independent arithmetic for measured inventory definitions.
//!
//! The persistent schema described in `docs/MEASURED_INVENTORY.md` is not
//! implemented yet. This module is the small arithmetic boundary reducers can
//! adopt when that schema lands.

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
    pub kind: MeasurementKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredInventoryRow {
    /// Number of full unopened units, or exactly one for measured state.
    pub quantity: u32,
    /// `None` is a full unopened stack. `Some` is a quantity-one measured row.
    pub remaining_amount: Option<u64>,
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
    AmountExceedsCapacity,
    Overflow,
}

impl MeasurementProfile {
    pub fn validate(self) -> Result<Self, MeasurementError> {
        if self.capacity == 0 {
            return Err(MeasurementError::ZeroCapacity);
        }
        if matches!(
            self.kind,
            MeasurementKind::Depletable | MeasurementKind::BulkLot
        ) && (self.tare_mass != 0 || self.tare_value != 0)
        {
            return Err(MeasurementError::NonContainerHasTare);
        }
        self.tare_mass
            .checked_add(self.full_contents_mass)
            .ok_or(MeasurementError::Overflow)?;
        self.tare_value
            .checked_add(self.full_contents_value)
            .ok_or(MeasurementError::Overflow)?;
        Ok(self)
    }

    pub fn effective_totals(
        self,
        row: MeasuredInventoryRow,
    ) -> Result<EffectiveTotals, MeasurementError> {
        let profile = self.validate()?;
        if row.quantity == 0 {
            return Err(MeasurementError::ZeroQuantity);
        }

        match row.remaining_amount {
            None => {
                let unit_mass = profile
                    .tare_mass
                    .checked_add(profile.full_contents_mass)
                    .ok_or(MeasurementError::Overflow)?;
                let unit_value = profile
                    .tare_value
                    .checked_add(profile.full_contents_value)
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
                if amount > profile.capacity {
                    return Err(MeasurementError::AmountExceedsCapacity);
                }
                let contents_mass =
                    prorated_floor(profile.full_contents_mass, amount, profile.capacity)?;
                let contents_value =
                    prorated_floor(profile.full_contents_value, amount, profile.capacity)?;
                Ok(EffectiveTotals {
                    mass: profile
                        .tare_mass
                        .checked_add(contents_mass)
                        .ok_or(MeasurementError::Overflow)?,
                    value: profile
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

#[cfg(test)]
mod tests {
    use super::*;

    fn soap() -> MeasurementProfile {
        MeasurementProfile {
            capacity: 100,
            full_contents_mass: 250,
            full_contents_value: 17,
            tare_mass: 0,
            tare_value: 0,
            kind: MeasurementKind::Depletable,
        }
    }

    fn bottle() -> MeasurementProfile {
        MeasurementProfile {
            capacity: 750,
            full_contents_mass: 750,
            full_contents_value: 11,
            tare_mass: 400,
            tare_value: 3,
            kind: MeasurementKind::Containerized,
        }
    }

    #[test]
    fn depletable_partial_scales_mass_and_value() {
        let totals = soap()
            .effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(40),
            })
            .unwrap();
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
        let empty = bottle()
            .effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(0),
            })
            .unwrap();
        let partial = bottle()
            .effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(375),
            })
            .unwrap();
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
        let totals = bottle()
            .effective_totals(MeasuredInventoryRow {
                quantity: 4,
                remaining_amount: None,
            })
            .unwrap();
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
        let unopened = bottle()
            .effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: None,
            })
            .unwrap();
        let opened = bottle()
            .effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(bottle().capacity),
            })
            .unwrap();
        assert_eq!(opened, unopened);
    }

    #[test]
    fn invalid_profiles_and_rows_are_rejected() {
        assert_eq!(
            MeasurementProfile {
                capacity: 0,
                ..soap()
            }
            .validate(),
            Err(MeasurementError::ZeroCapacity)
        );
        assert_eq!(
            MeasurementProfile {
                tare_mass: 1,
                ..soap()
            }
            .validate(),
            Err(MeasurementError::NonContainerHasTare)
        );
        assert_eq!(
            soap().effective_totals(MeasuredInventoryRow {
                quantity: 2,
                remaining_amount: Some(50)
            }),
            Err(MeasurementError::MeasuredRowIsNotSingleton)
        );
        assert_eq!(
            soap().effective_totals(MeasuredInventoryRow {
                quantity: 1,
                remaining_amount: Some(101)
            }),
            Err(MeasurementError::AmountExceedsCapacity)
        );
        assert_eq!(
            soap().effective_totals(MeasuredInventoryRow {
                quantity: 0,
                remaining_amount: None
            }),
            Err(MeasurementError::ZeroQuantity)
        );
        assert_eq!(prorated_floor(1, 0, 0), Err(MeasurementError::ZeroCapacity));
    }

    #[test]
    fn overflow_is_reported_instead_of_saturating() {
        let profile = MeasurementProfile {
            capacity: 1,
            full_contents_mass: u64::MAX,
            full_contents_value: 1,
            tare_mass: 0,
            tare_value: 0,
            kind: MeasurementKind::Containerized,
        };
        assert_eq!(
            profile.effective_totals(MeasuredInventoryRow {
                quantity: 2,
                remaining_amount: None
            }),
            Err(MeasurementError::Overflow)
        );
        assert_eq!(
            MeasurementProfile {
                tare_mass: 1,
                ..profile
            }
            .validate(),
            Err(MeasurementError::Overflow)
        );
    }

    #[test]
    fn mass_and_value_are_monotonic() {
        let mut previous = EffectiveTotals { mass: 0, value: 0 };
        for amount in 0..=bottle().capacity {
            let current = bottle()
                .effective_totals(MeasuredInventoryRow {
                    quantity: 1,
                    remaining_amount: Some(amount),
                })
                .unwrap();
            assert!(current.mass >= previous.mass);
            assert!(current.value >= previous.value);
            previous = current;
        }
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
}
