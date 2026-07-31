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
pub const HOUSING_BILLING_PERIOD_MINUTES: u64 = 30 * MINUTES_PER_DAY;
pub const CONCEPTION_QUANTUM_MINUTES: u64 = 60;
pub const CONCEPTION_CHANCE_PER_TEN_THOUSAND: u16 = 40;

/// Residence Leisure is one refreshable source which lasts for one week.
pub const RESIDENCE_MORALE_CAP_MILLI: u32 = 8_000;
pub const RESIDENCE_MORALE_DURATION_MINUTES: u64 = 7 * MINUTES_PER_DAY;
/// Time with a spouse has a stronger, longer-lived refreshable benefit.
pub const SPOUSE_LEISURE_MORALE_CAP_MILLI: u32 = 12_000;
pub const SPOUSE_LEISURE_MORALE_DURATION_MINUTES: u64 = 30 * MINUTES_PER_DAY;
/// Both refreshable sources may coexist, but cannot exceed this total.
pub const LEISURE_MORALE_STACK_CAP_MILLI: u32 = 16_000;
pub const SPOUSE_MORALE_MILLI_PER_JOINT_MINUTE: u32 = 2;

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
pub enum HousingCatalogError {
    WrongOfferCount,
    WrongTierOrder,
    ValuesNotStrictlyOrdered,
    OwnershipNotCheaperThanRent,
}

/// Validate the authored invariant that every settlement exposes exactly one
/// offer in each tier, with every price and benefit strictly increasing.
pub fn validate_housing_catalog(
    offers: &[(HousingTier, HousingEconomy)],
) -> Result<(), HousingCatalogError> {
    if offers.len() != HousingTier::ALL.len() {
        return Err(HousingCatalogError::WrongOfferCount);
    }
    if !offers
        .iter()
        .zip(HousingTier::ALL)
        .all(|((actual, _), expected)| *actual == expected)
    {
        return Err(HousingCatalogError::WrongTierOrder);
    }
    if offers.iter().any(|(_, economy)| {
        economy
            .owner_maintenance_per_30_days
            .saturating_add(economy.property_tax_per_30_days)
            >= economy.rent_per_30_days
    }) {
        return Err(HousingCatalogError::OwnershipNotCheaperThanRent);
    }
    let ordered = offers.windows(2).all(|pair| {
        let low = pair[0].1;
        let high = pair[1].1;
        low.purchase_price < high.purchase_price
            && low.rent_per_30_days < high.rent_per_30_days
            && low.owner_maintenance_per_30_days < high.owner_maintenance_per_30_days
            && low.property_tax_per_30_days < high.property_tax_per_30_days
            && low.leisure_morale_basis_points < high.leisure_morale_basis_points
    });
    if !ordered {
        return Err(HousingCatalogError::ValuesNotStrictlyOrdered);
    }
    Ok(())
}

pub fn authored_housing_catalog() -> [(HousingTier, HousingEconomy); 3] {
    HousingTier::ALL.map(|tier| (tier, tier.economy()))
}

/// Chronological due dates beginning at `next_due_minute`, inclusive of the
/// requested frontier. Overflow terminates the finite authoritative timeline.
#[derive(Clone, Copy, Debug)]
pub struct DuePeriods {
    next: Option<u64>,
    through_minute: u64,
}

impl Iterator for DuePeriods {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        let due = self.next?;
        if due > self.through_minute {
            self.next = None;
            return None;
        }
        self.next = due.checked_add(HOUSING_BILLING_PERIOD_MINUTES);
        Some(due)
    }
}

pub const fn due_periods(next_due_minute: u64, through_minute: u64) -> DuePeriods {
    DuePeriods {
        next: Some(next_due_minute),
        through_minute,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DueSettlementPlan {
    pub periods_due: u64,
    pub periods_paid: u64,
    pub amount_spent: u64,
    pub funds_remaining: u64,
    /// The next date to persist after all successful payments. On failure this
    /// remains the first unpaid date, which makes retry and chunk behavior
    /// unambiguous.
    pub next_due_minute: u64,
    pub first_unpaid_due_minute: Option<u64>,
}

/// Settle recurring bills one period at a time. A bill is indivisible: funds
/// below one complete charge are retained and the first unpaid date is
/// reported. Calling this through an intermediate frontier and then the final
/// frontier produces the same plan as one call when the returned funds and
/// next due date are carried forward.
pub fn plan_due_period_settlement(
    next_due_minute: u64,
    through_minute: u64,
    available_funds: u64,
    charge_per_period: u64,
) -> DueSettlementPlan {
    let periods_due = due_periods(next_due_minute, through_minute).count() as u64;
    let affordable = if charge_per_period == 0 {
        periods_due
    } else {
        available_funds / charge_per_period
    };
    let periods_paid = periods_due.min(affordable);
    let amount_spent = periods_paid.saturating_mul(charge_per_period);
    let first_unpaid_due_minute = (periods_paid < periods_due).then(|| {
        next_due_minute.saturating_add(periods_paid.saturating_mul(HOUSING_BILLING_PERIOD_MINUTES))
    });
    DueSettlementPlan {
        periods_due,
        periods_paid,
        amount_spent,
        funds_remaining: available_funds.saturating_sub(amount_spent),
        next_due_minute: next_due_minute
            .saturating_add(periods_paid.saturating_mul(HOUSING_BILLING_PERIOD_MINUTES)),
        first_unpaid_due_minute,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshableMorale {
    pub milli_points: u32,
    pub expires_at_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefreshableMoraleSpec {
    pub cap_milli: u32,
    pub duration_minutes: u64,
}

pub const RESIDENCE_MORALE_SPEC: RefreshableMoraleSpec = RefreshableMoraleSpec {
    cap_milli: RESIDENCE_MORALE_CAP_MILLI,
    duration_minutes: RESIDENCE_MORALE_DURATION_MINUTES,
};
pub const SPOUSE_LEISURE_MORALE_SPEC: RefreshableMoraleSpec = RefreshableMoraleSpec {
    cap_milli: SPOUSE_LEISURE_MORALE_CAP_MILLI,
    duration_minutes: SPOUSE_LEISURE_MORALE_DURATION_MINUTES,
};

const fn min_u32(left: u32, right: u32) -> u32 {
    if left < right { left } else { right }
}

const fn min_u64(left: u64, right: u64) -> u64 {
    if left < right { left } else { right }
}

/// Add realized morale to one durable refreshable source. Expired value never
/// contributes, zero gain does not prolong it, and refreshes cannot stack
/// beyond the source cap.
pub const fn refresh_morale(
    current: RefreshableMorale,
    now_minute: u64,
    earned_milli: u32,
    spec: RefreshableMoraleSpec,
) -> RefreshableMorale {
    if earned_milli == 0 {
        return current;
    }
    let live = if current.expires_at_minute > now_minute {
        current.milli_points
    } else {
        0
    };
    RefreshableMorale {
        milli_points: min_u32(live.saturating_add(earned_milli), spec.cap_milli),
        expires_at_minute: now_minute.saturating_add(spec.duration_minutes),
    }
}

pub const fn bounded_leisure_morale_total(
    residence: RefreshableMorale,
    spouse: RefreshableMorale,
    at_minute: u64,
) -> u32 {
    let residence = if residence.expires_at_minute > at_minute {
        min_u32(residence.milli_points, RESIDENCE_MORALE_CAP_MILLI)
    } else {
        0
    };
    let spouse = if spouse.expires_at_minute > at_minute {
        min_u32(spouse.milli_points, SPOUSE_LEISURE_MORALE_CAP_MILLI)
    } else {
        0
    };
    min_u32(
        residence.saturating_add(spouse),
        LEISURE_MORALE_STACK_CAP_MILLI,
    )
}

/// Refresh one Leisure source while reserving room for the other source's
/// still-live value. Calling this from either source updater gives the same
/// combined cap and ignores expired counterpart value.
pub const fn refresh_bounded_leisure_morale(
    current: RefreshableMorale,
    other: RefreshableMorale,
    now_minute: u64,
    earned_milli: u32,
    spec: RefreshableMoraleSpec,
) -> RefreshableMorale {
    let refreshed = refresh_morale(current, now_minute, earned_milli, spec);
    if earned_milli == 0 {
        return refreshed;
    }
    let other_live = if other.expires_at_minute > now_minute {
        other.milli_points
    } else {
        0
    };
    RefreshableMorale {
        milli_points: min_u32(
            refreshed.milli_points,
            LEISURE_MORALE_STACK_CAP_MILLI.saturating_sub(other_live),
        ),
        expires_at_minute: refreshed.expires_at_minute,
    }
}

/// Residence comfort is the tier's premium over baseline realized Leisure.
/// Integer milli-morale avoids checkpoint-dependent float rounding.
pub const fn residence_leisure_bonus_milli(
    realized_baseline_milli: u32,
    leisure_morale_basis_points: u16,
) -> u32 {
    let premium_basis_points = leisure_morale_basis_points.saturating_sub(10_000);
    min_u64(
        realized_baseline_milli as u64 * premium_basis_points as u64 / 10_000,
        u32::MAX as u64,
    ) as u32
}

pub const fn spouse_leisure_earned_milli(joint_leisure_minutes: u64) -> u32 {
    min_u64(
        joint_leisure_minutes.saturating_mul(SPOUSE_MORALE_MILLI_PER_JOINT_MINUTE as u64),
        u32::MAX as u64,
    ) as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeisureInterval<'a> {
    pub start_minute: u64,
    pub end_minute: u64,
    pub location_id: &'a str,
}

/// Intersect two realized Leisure spans. Invalid/reversed spans accrue zero,
/// as do spans at different locations.
pub fn joint_leisure_minutes(left: LeisureInterval<'_>, right: LeisureInterval<'_>) -> u64 {
    if left.location_id != right.location_id {
        return 0;
    }
    left.end_minute
        .min(right.end_minute)
        .saturating_sub(left.start_minute.max(right.start_minute))
}

/// Accrue a joint span inside an arbitrary checkpoint interval. Integer
/// intersection makes adjacent checkpoints telescope without a remainder.
pub fn joint_leisure_minutes_in(
    left: LeisureInterval<'_>,
    right: LeisureInterval<'_>,
    checkpoint_start: u64,
    checkpoint_end: u64,
) -> u64 {
    joint_leisure_minutes(
        LeisureInterval {
            start_minute: left.start_minute.max(checkpoint_start),
            end_minute: left.end_minute.min(checkpoint_end),
            location_id: left.location_id,
        },
        LeisureInterval {
            start_minute: right.start_minute.max(checkpoint_start),
            end_minute: right.end_minute.min(checkpoint_end),
            location_id: right.location_id,
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinuteSpan {
    pub start_minute: u64,
    pub end_minute: u64,
}

/// Return candidate coverage not already represented by existing spans.
/// Existing spans may be unordered, overlapping, or duplicated; the result is
/// sorted and disjoint so retries cannot create additional Leisure.
pub fn uncovered_minute_spans(
    candidate: MinuteSpan,
    existing: impl IntoIterator<Item = MinuteSpan>,
) -> Vec<MinuteSpan> {
    if candidate.end_minute <= candidate.start_minute {
        return Vec::new();
    }
    let mut covered: Vec<_> = existing
        .into_iter()
        .filter_map(|span| {
            let start = candidate.start_minute.max(span.start_minute);
            let end = candidate.end_minute.min(span.end_minute);
            (end > start).then_some(MinuteSpan {
                start_minute: start,
                end_minute: end,
            })
        })
        .collect();
    covered.sort_by_key(|span| (span.start_minute, span.end_minute));
    let mut result = Vec::new();
    let mut cursor = candidate.start_minute;
    for span in covered {
        if span.start_minute > cursor {
            result.push(MinuteSpan {
                start_minute: cursor,
                end_minute: span.start_minute,
            });
        }
        cursor = cursor.max(span.end_minute);
        if cursor >= candidate.end_minute {
            break;
        }
    }
    if cursor < candidate.end_minute {
        result.push(MinuteSpan {
            start_minute: cursor,
            end_minute: candidate.end_minute,
        });
    }
    result
}

/// Stable FNV-1a domain-separated hash. Unlike `DefaultHasher`, this result is
/// part of gameplay and is stable across processes and compiler releases.
pub fn stable_lifecycle_hash(domain: &str, parts: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in domain.as_bytes().iter().copied().chain([0xff]).chain(
        parts
            .iter()
            .flat_map(|part| part.as_bytes().iter().copied().chain([0xff])),
    ) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn daily_location_target_score(
    actor_id: &str,
    location_id: &str,
    calendar_day: u64,
    target_id: &str,
) -> u64 {
    let day = calendar_day.to_string();
    stable_lifecycle_hash(
        "daily-location-target",
        &[actor_id, location_id, &day, target_id],
    )
}

pub fn select_stable_target_by_score<'a>(
    candidates: impl IntoIterator<Item = &'a str>,
    score: impl Fn(&str) -> u64,
) -> Option<&'a str> {
    candidates
        .into_iter()
        .min_by_key(|candidate| (score(candidate), *candidate))
}

/// Pick the lowest deterministic score. Character ID is the stable final tie
/// break, so storage or iteration order cannot affect an ambiguous choice.
pub fn select_daily_location_target<'a>(
    actor_id: &str,
    location_id: &str,
    calendar_day: u64,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    select_stable_target_by_score(candidates, |candidate| {
        daily_location_target_score(actor_id, location_id, calendar_day, candidate)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConceptionQuantumState {
    pub conserved_joint_minutes: u8,
    pub next_trial_ordinal: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConceptionTrial {
    pub ordinal: u64,
    /// One-based minute offset from the start of this accrual interval.
    pub crossing_offset_minutes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptionQuantumPlan {
    pub trials: Vec<ConceptionTrial>,
    pub state: ConceptionQuantumState,
}

/// Cross one deterministic conception trial per conserved joint Leisure hour.
/// Persisting the returned remainder and ordinal makes arbitrary checkpoint
/// partitioning equivalent to one whole interval.
pub fn conception_quantum_plan(
    state: ConceptionQuantumState,
    additional_joint_minutes: u64,
) -> ConceptionQuantumPlan {
    let carried = u64::from(state.conserved_joint_minutes) % CONCEPTION_QUANTUM_MINUTES;
    let first_crossing = CONCEPTION_QUANTUM_MINUTES - carried;
    let crossing_count = if additional_joint_minutes < first_crossing {
        0
    } else {
        1 + (additional_joint_minutes - first_crossing) / CONCEPTION_QUANTUM_MINUTES
    };
    let trials = (0..crossing_count)
        .map(|index| ConceptionTrial {
            ordinal: state.next_trial_ordinal.saturating_add(index),
            crossing_offset_minutes: first_crossing
                .saturating_add(index.saturating_mul(CONCEPTION_QUANTUM_MINUTES)),
        })
        .collect();
    ConceptionQuantumPlan {
        trials,
        state: ConceptionQuantumState {
            conserved_joint_minutes: ((carried
                + additional_joint_minutes % CONCEPTION_QUANTUM_MINUTES)
                % CONCEPTION_QUANTUM_MINUTES) as u8,
            next_trial_ordinal: state.next_trial_ordinal.saturating_add(crossing_count),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChildSeeds {
    pub identity: u64,
    pub name: u64,
    pub female: bool,
    pub home: u64,
}

/// Domain-separated child seeds make identity, naming, sex, and home placement
/// stable without coupling any result to table insertion order.
pub fn deterministic_child_seeds(
    first_parent_id: &str,
    second_parent_id: &str,
    pregnancy_ordinal: u64,
    birth_minute: u64,
    home_location_id: &str,
) -> ChildSeeds {
    let (left, right) = if first_parent_id <= second_parent_id {
        (first_parent_id, second_parent_id)
    } else {
        (second_parent_id, first_parent_id)
    };
    let pregnancy = pregnancy_ordinal.to_string();
    let birth = birth_minute.to_string();
    let base = [left, right, &pregnancy, &birth];
    ChildSeeds {
        identity: stable_lifecycle_hash("child-identity", &base),
        name: stable_lifecycle_hash("child-name", &base),
        female: stable_lifecycle_hash("child-sex", &base) & 1 == 0,
        home: stable_lifecycle_hash(
            "child-home",
            &[left, right, &pregnancy, &birth, home_location_id],
        ),
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
        assert!(
            informal_affinity_threshold(CourtshipDisposition::Amorous)
                < informal_affinity_threshold(CourtshipDisposition::Neutral)
        );
        assert!(
            informal_affinity_threshold(CourtshipDisposition::Neutral)
                < informal_affinity_threshold(CourtshipDisposition::Proper)
        );
    }

    #[test]
    fn day_boundaries_are_chunk_invariant() {
        let whole: Vec<_> = crossed_days(20, 3 * MINUTES_PER_DAY + 20).collect();
        let mut split = crossed_days(20, MINUTES_PER_DAY + 20).collect::<Vec<_>>();
        split.extend(crossed_days(MINUTES_PER_DAY + 20, 3 * MINUTES_PER_DAY + 20));
        assert_eq!(whole, split);
    }

    #[test]
    fn authored_housing_is_exactly_three_strictly_ordered_tiers() {
        let catalog = authored_housing_catalog();
        assert_eq!(validate_housing_catalog(&catalog), Ok(()));

        assert_eq!(
            validate_housing_catalog(&catalog[..2]),
            Err(HousingCatalogError::WrongOfferCount)
        );
        let mut swapped = catalog;
        swapped.swap(0, 1);
        assert_eq!(
            validate_housing_catalog(&swapped),
            Err(HousingCatalogError::WrongTierOrder)
        );
        let mut flat = catalog;
        flat[1].1.purchase_price = flat[0].1.purchase_price;
        assert_eq!(
            validate_housing_catalog(&flat),
            Err(HousingCatalogError::ValuesNotStrictlyOrdered)
        );
        let mut costly_owner = catalog;
        costly_owner[0].1.owner_maintenance_per_30_days = costly_owner[0].1.rent_per_30_days;
        assert_eq!(
            validate_housing_catalog(&costly_owner),
            Err(HousingCatalogError::OwnershipNotCheaperThanRent)
        );
    }

    #[test]
    fn due_periods_are_chronological_and_chunk_invariant() {
        let first = 100;
        let through = first + 4 * HOUSING_BILLING_PERIOD_MINUTES + 12;
        let whole: Vec<_> = due_periods(first, through).collect();
        let checkpoint = first + 2 * HOUSING_BILLING_PERIOD_MINUTES - 1;
        let mut chunks: Vec<_> = due_periods(first, checkpoint).collect();
        let next = chunks
            .last()
            .map_or(first, |paid| paid + HOUSING_BILLING_PERIOD_MINUTES);
        chunks.extend(due_periods(next, through));
        assert_eq!(whole, chunks);
        assert_eq!(whole.len(), 5);
        assert!(whole.windows(2).all(|pair| pair[1] > pair[0]));
    }

    #[test]
    fn due_plan_retains_partial_funds_and_stops_at_first_unpaid_period() {
        let first = HOUSING_BILLING_PERIOD_MINUTES;
        let through = 4 * HOUSING_BILLING_PERIOD_MINUTES;
        let plan = plan_due_period_settlement(first, through, 25, 8);
        assert_eq!(
            plan,
            DueSettlementPlan {
                periods_due: 4,
                periods_paid: 3,
                amount_spent: 24,
                funds_remaining: 1,
                next_due_minute: 4 * HOUSING_BILLING_PERIOD_MINUTES,
                first_unpaid_due_minute: Some(4 * HOUSING_BILLING_PERIOD_MINUTES),
            }
        );

        let whole = plan_due_period_settlement(first, through, 40, 8);
        let early = plan_due_period_settlement(first, 2 * HOUSING_BILLING_PERIOD_MINUTES, 40, 8);
        let late =
            plan_due_period_settlement(early.next_due_minute, through, early.funds_remaining, 8);
        assert_eq!(whole.periods_paid, early.periods_paid + late.periods_paid);
        assert_eq!(whole.amount_spent, early.amount_spent + late.amount_spent);
        assert_eq!(whole.funds_remaining, late.funds_remaining);
        assert_eq!(whole.next_due_minute, late.next_due_minute);
        assert_eq!(whole.first_unpaid_due_minute, late.first_unpaid_due_minute);
    }

    #[test]
    fn refreshable_morale_caps_expires_and_has_a_stack_bound() {
        let residence = refresh_morale(
            RefreshableMorale::default(),
            100,
            RESIDENCE_MORALE_CAP_MILLI + 500,
            RESIDENCE_MORALE_SPEC,
        );
        let spouse = refresh_morale(
            RefreshableMorale::default(),
            100,
            SPOUSE_LEISURE_MORALE_CAP_MILLI + 500,
            SPOUSE_LEISURE_MORALE_SPEC,
        );
        assert_eq!(residence.milli_points, RESIDENCE_MORALE_CAP_MILLI);
        assert_eq!(spouse.milli_points, SPOUSE_LEISURE_MORALE_CAP_MILLI);
        assert_eq!(
            bounded_leisure_morale_total(residence, spouse, 100),
            LEISURE_MORALE_STACK_CAP_MILLI
        );
        assert_eq!(
            bounded_leisure_morale_total(
                residence,
                spouse,
                100 + RESIDENCE_MORALE_DURATION_MINUTES
            ),
            SPOUSE_LEISURE_MORALE_CAP_MILLI
        );
        assert_eq!(
            refresh_morale(residence, 200, 0, RESIDENCE_MORALE_SPEC),
            residence
        );
        assert_eq!(residence_leisure_bonus_milli(4_000, 11_000), 400);
        assert_eq!(residence_leisure_bonus_milli(4_000, 10_000), 0);
        assert_eq!(spouse_leisure_earned_milli(60), 120);
        assert_eq!(
            spouse_leisure_earned_milli(17) + spouse_leisure_earned_milli(43),
            spouse_leisure_earned_milli(60)
        );

        let residence_first = refresh_bounded_leisure_morale(
            RefreshableMorale::default(),
            RefreshableMorale::default(),
            100,
            8_000,
            RESIDENCE_MORALE_SPEC,
        );
        let spouse_second = refresh_bounded_leisure_morale(
            RefreshableMorale::default(),
            residence_first,
            100,
            12_000,
            SPOUSE_LEISURE_MORALE_SPEC,
        );
        let spouse_first = refresh_bounded_leisure_morale(
            RefreshableMorale::default(),
            RefreshableMorale::default(),
            100,
            12_000,
            SPOUSE_LEISURE_MORALE_SPEC,
        );
        let residence_second = refresh_bounded_leisure_morale(
            RefreshableMorale::default(),
            spouse_first,
            100,
            8_000,
            RESIDENCE_MORALE_SPEC,
        );
        assert_eq!(
            residence_first.milli_points + spouse_second.milli_points,
            LEISURE_MORALE_STACK_CAP_MILLI
        );
        assert_eq!(
            spouse_first.milli_points + residence_second.milli_points,
            LEISURE_MORALE_STACK_CAP_MILLI
        );
        let after_spouse_expiry = refresh_bounded_leisure_morale(
            residence_second,
            spouse_first,
            spouse_first.expires_at_minute,
            8_000,
            RESIDENCE_MORALE_SPEC,
        );
        assert_eq!(after_spouse_expiry.milli_points, RESIDENCE_MORALE_CAP_MILLI);
    }

    #[test]
    fn deterministic_target_selection_is_order_independent_and_ties_by_id() {
        let forward =
            select_daily_location_target("actor", "wittenberg", 42, ["c", "a", "b"]).unwrap();
        let reverse =
            select_daily_location_target("actor", "wittenberg", 42, ["b", "a", "c"]).unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(
            select_stable_target_by_score(["zeta", "alpha"], |_| 7),
            Some("alpha")
        );
        assert_ne!(
            daily_location_target_score("actor", "wittenberg", 42, "a"),
            daily_location_target_score("actor", "wittenberg", 43, "a")
        );
    }

    #[test]
    fn joint_leisure_requires_overlap_and_identical_location() {
        let first = LeisureInterval {
            start_minute: 100,
            end_minute: 220,
            location_id: "town",
        };
        let overlapping = LeisureInterval {
            start_minute: 160,
            end_minute: 280,
            location_id: "town",
        };
        assert_eq!(joint_leisure_minutes(first, overlapping), 60);
        assert_eq!(
            joint_leisure_minutes(
                first,
                LeisureInterval {
                    location_id: "road",
                    ..overlapping
                }
            ),
            0
        );
        assert_eq!(
            joint_leisure_minutes(
                first,
                LeisureInterval {
                    start_minute: 220,
                    end_minute: 300,
                    location_id: "town",
                }
            ),
            0
        );
    }

    #[test]
    fn joint_leisure_checkpoint_accrual_telescopes() {
        let first = LeisureInterval {
            start_minute: 10,
            end_minute: 310,
            location_id: "town",
        };
        let second = LeisureInterval {
            start_minute: 80,
            end_minute: 260,
            location_id: "town",
        };
        let whole = joint_leisure_minutes_in(first, second, 0, 400);
        let chunked = joint_leisure_minutes_in(first, second, 0, 137)
            + joint_leisure_minutes_in(first, second, 137, 400);
        assert_eq!(whole, 180);
        assert_eq!(whole, chunked);
    }

    #[test]
    fn retries_and_alternative_slices_only_return_uncovered_time() {
        let candidate = MinuteSpan {
            start_minute: 100,
            end_minute: 300,
        };
        let existing = [
            MinuteSpan {
                start_minute: 180,
                end_minute: 240,
            },
            MinuteSpan {
                start_minute: 120,
                end_minute: 200,
            },
            MinuteSpan {
                start_minute: 120,
                end_minute: 200,
            },
        ];
        assert_eq!(
            uncovered_minute_spans(candidate, existing),
            vec![
                MinuteSpan {
                    start_minute: 100,
                    end_minute: 120,
                },
                MinuteSpan {
                    start_minute: 240,
                    end_minute: 300,
                },
            ]
        );
        assert!(uncovered_minute_spans(candidate, [candidate]).is_empty());
    }

    #[test]
    fn conception_quantum_preserves_remainder_ordinal_and_exact_offsets() {
        let initial = ConceptionQuantumState {
            conserved_joint_minutes: 20,
            next_trial_ordinal: 7,
        };
        let whole = conception_quantum_plan(initial, 151);
        assert_eq!(
            whole.trials,
            vec![
                ConceptionTrial {
                    ordinal: 7,
                    crossing_offset_minutes: 40,
                },
                ConceptionTrial {
                    ordinal: 8,
                    crossing_offset_minutes: 100,
                },
            ]
        );
        assert_eq!(
            whole.state,
            ConceptionQuantumState {
                conserved_joint_minutes: 51,
                next_trial_ordinal: 9,
            }
        );

        let early = conception_quantum_plan(initial, 55);
        let late = conception_quantum_plan(early.state, 96);
        let mut chunked = early.trials;
        chunked.extend(late.trials.into_iter().map(|trial| ConceptionTrial {
            crossing_offset_minutes: trial.crossing_offset_minutes + 55,
            ..trial
        }));
        assert_eq!(whole.trials, chunked);
        assert_eq!(whole.state, late.state);
        assert!(succeeds_daily_trial(39, CONCEPTION_CHANCE_PER_TEN_THOUSAND));
        assert!(!succeeds_daily_trial(
            40,
            CONCEPTION_CHANCE_PER_TEN_THOUSAND
        ));
    }

    #[test]
    fn child_seeds_are_parent_order_independent_and_domain_separated() {
        let first = deterministic_child_seeds("anna", "beatrice", 3, 900, "wittenberg");
        let reversed = deterministic_child_seeds("beatrice", "anna", 3, 900, "wittenberg");
        assert_eq!(first, reversed);
        assert_ne!(first.identity, first.name);
        assert_ne!(first.identity, first.home);
        assert_ne!(
            first,
            deterministic_child_seeds("anna", "beatrice", 4, 900, "wittenberg")
        );
    }
}
