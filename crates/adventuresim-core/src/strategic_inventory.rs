//! Pure planning and validation rules for strategic inventory commerce.
//!
//! Persistent identifiers and reducer arguments remain wire-compatible raw values. Reducers
//! parse them into these types before applying authoritative database mutations.

use std::{fmt, num::NonZeroU64};

use crate::settlement_economy::Storefront;

/// A non-zero amount of coin in a validated payment plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoinAmount(NonZeroU64);

impl CoinAmount {
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// The caller's maximum authorized contribution from each funding source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorefrontPaymentAuthorization {
    maximum_personal: u64,
    maximum_stake: u64,
}

impl StorefrontPaymentAuthorization {
    pub fn new(maximum_personal: u64, maximum_stake: u64) -> Self {
        Self {
            maximum_personal,
            maximum_stake,
        }
    }

    /// Plans a payment using personal coin first, without inventing zero-valued sources.
    pub fn plan(self, total: u64) -> Result<StorefrontPaymentPlan, PaymentPlanError> {
        let total = NonZeroU64::new(total)
            .ok_or(PaymentPlanError::ZeroPurchaseTotal)?
            .get();
        let personal = total.min(self.maximum_personal);
        let stake = total - personal;
        if stake > self.maximum_stake {
            return Err(PaymentPlanError::AuthorizedFundsInsufficient);
        }

        match (NonZeroU64::new(personal), NonZeroU64::new(stake)) {
            (Some(personal), None) => Ok(StorefrontPaymentPlan::PersonalOnly {
                personal: CoinAmount(personal),
            }),
            (None, Some(stake)) => Ok(StorefrontPaymentPlan::StakeOnly {
                stake: CoinAmount(stake),
            }),
            (Some(personal), Some(stake)) => Ok(StorefrontPaymentPlan::PersonalAndStake {
                personal: CoinAmount(personal),
                stake: CoinAmount(stake),
            }),
            (None, None) => unreachable!("a non-zero purchase has a non-zero funding source"),
        }
    }
}

/// A valid funding plan. Variants make an absent source distinct from a zero payment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorefrontPaymentPlan {
    PersonalOnly {
        personal: CoinAmount,
    },
    StakeOnly {
        stake: CoinAmount,
    },
    PersonalAndStake {
        personal: CoinAmount,
        stake: CoinAmount,
    },
}

impl StorefrontPaymentPlan {
    pub fn personal_amount(self) -> u64 {
        match self {
            Self::PersonalOnly { personal } | Self::PersonalAndStake { personal, .. } => {
                personal.get()
            }
            Self::StakeOnly { .. } => 0,
        }
    }

    pub fn stake_amount(self) -> u64 {
        match self {
            Self::StakeOnly { stake } | Self::PersonalAndStake { stake, .. } => stake.get(),
            Self::PersonalOnly { .. } => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentPlanError {
    ZeroPurchaseTotal,
    AuthorizedFundsInsufficient,
}

impl fmt::Display for PaymentPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroPurchaseTotal => f.write_str("Storefront purchase total must be positive"),
            Self::AuthorizedFundsInsufficient => {
                f.write_str("Current storefront payment exceeds the authorized stake maximum")
            }
        }
    }
}

impl std::error::Error for PaymentPlanError {}

/// A validated persistent resident-character identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MerchantProviderId(NonZeroU64);

impl MerchantProviderId {
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl TryFrom<u64> for MerchantProviderId {
    type Error = MerchantProviderError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(MerchantProviderError::InvalidIdentifier)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MerchantProviderError {
    NotFound,
    Ambiguous,
    InvalidIdentifier,
}

impl fmt::Display for MerchantProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("Merchant service provider not found"),
            Self::Ambiguous => f.write_str("Merchant service provider is ambiguous"),
            Self::InvalidIdentifier => {
                f.write_str("Merchant service provider has an invalid identifier")
            }
        }
    }
}

impl std::error::Error for MerchantProviderError {}

/// Selects exactly one provider and rejects both missing and corrupt identifiers.
pub fn unique_merchant_provider(
    providers: impl IntoIterator<Item = u64>,
) -> Result<MerchantProviderId, MerchantProviderError> {
    let mut providers = providers.into_iter();
    let provider = providers.next().ok_or(MerchantProviderError::NotFound)?;
    if providers.next().is_some() {
        return Err(MerchantProviderError::Ambiguous);
    }
    MerchantProviderId::try_from(provider)
}

/// Parsed route from a durable merchant service key to its storefront and location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MerchantStorefrontRoute {
    storefront: Storefront,
    location_id: &'static str,
}

impl MerchantStorefrontRoute {
    pub fn storefront(self) -> Storefront {
        self.storefront
    }

    pub fn location_id(self) -> &'static str {
        self.location_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownMerchantService;

impl fmt::Display for UnknownMerchantService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Unknown merchant storefront")
    }
}

impl std::error::Error for UnknownMerchantService {}

impl TryFrom<&str> for MerchantStorefrontRoute {
    type Error = UnknownMerchantService;

    fn try_from(service_id: &str) -> Result<Self, Self::Error> {
        let (storefront, location_id) = match service_id {
            "merchants" => (Storefront::General, "market"),
            "weapons" => (Storefront::Weapons, "forge"),
            "armor" => (Storefront::Armor, "armoury"),
            "clothing" => (Storefront::Clothing, "tailor"),
            "inn" => (Storefront::Inn, "inn"),
            "books" => (Storefront::Books, "bookstore"),
            _ => return Err(UnknownMerchantService),
        };
        Ok(Self {
            storefront,
            location_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_plan_uses_personal_coin_before_stake() {
        let plan = StorefrontPaymentAuthorization::new(10, 2).plan(12).unwrap();
        assert!(matches!(
            plan,
            StorefrontPaymentPlan::PersonalAndStake { .. }
        ));
        assert_eq!(plan.personal_amount(), 10);
        assert_eq!(plan.stake_amount(), 2);
    }

    #[test]
    fn payment_plan_represents_single_source_payments_as_single_variants() {
        let personal = StorefrontPaymentAuthorization::new(10, 0).plan(8).unwrap();
        assert!(matches!(
            personal,
            StorefrontPaymentPlan::PersonalOnly { .. }
        ));

        let stake = StorefrontPaymentAuthorization::new(0, 10).plan(8).unwrap();
        assert!(matches!(stake, StorefrontPaymentPlan::StakeOnly { .. }));
    }

    #[test]
    fn payment_plan_rejects_an_unauthorized_shortfall() {
        assert_eq!(
            StorefrontPaymentAuthorization::new(10, 1).plan(12),
            Err(PaymentPlanError::AuthorizedFundsInsufficient)
        );
        assert_eq!(
            StorefrontPaymentAuthorization::new(10, 1).plan(0),
            Err(PaymentPlanError::ZeroPurchaseTotal)
        );
    }

    #[test]
    fn provider_selection_requires_one_nonzero_identifier() {
        assert_eq!(unique_merchant_provider([41]).unwrap().get(), 41);
        assert_eq!(
            unique_merchant_provider(Vec::<u64>::new()),
            Err(MerchantProviderError::NotFound)
        );
        assert_eq!(
            unique_merchant_provider([41, 42]),
            Err(MerchantProviderError::Ambiguous)
        );
        assert_eq!(
            unique_merchant_provider([0]),
            Err(MerchantProviderError::InvalidIdentifier)
        );
    }

    #[test]
    fn storefront_routes_parse_only_canonical_service_keys() {
        assert_eq!(
            MerchantStorefrontRoute::try_from("merchants")
                .unwrap()
                .storefront(),
            Storefront::General
        );
        assert_eq!(
            MerchantStorefrontRoute::try_from("merchants")
                .unwrap()
                .location_id(),
            "market"
        );
        assert_eq!(
            MerchantStorefrontRoute::try_from("inn")
                .unwrap()
                .location_id(),
            "inn"
        );
        assert!(MerchantStorefrontRoute::try_from("herbalist").is_err());
        assert!(MerchantStorefrontRoute::try_from("../inn").is_err());
    }
}
