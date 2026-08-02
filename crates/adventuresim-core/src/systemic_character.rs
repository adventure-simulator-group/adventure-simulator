//! Framework-neutral rules for contextual surrender, custody, recruitment, and property.
//!
//! These rules deliberately contain no quest or persistence concepts. Owning systems
//! validate authority, persist revisions/receipts, and emit trusted outcome facts.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextDisposition {
    Neutral,
    Hostile,
    OfferPending,
    DemandPending,
    Refused,
    Surrendered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurrenderAction {
    Offer,
    Demand,
    Accept,
    Refuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObligationKind {
    Disarm,
    LeaveSite,
    PayRansom,
    EnterCustody,
    Testify,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredObligation {
    pub kind: ObligationKind,
    pub beneficiary_id: String,
    pub amount_minor: u64,
}

/// Private authoritative inputs. Values are bounded integers so persistence
/// adapters cannot smuggle NaNs or client-selected probabilities into a rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurrenderInputs {
    pub morale: i16,
    pub fear: u16,
    pub incapacitation_bps: u16,
    pub affinity: i16,
    pub familiarity_bps: u16,
    pub leverage: i16,
    pub mutual_awareness: bool,
    pub obligations: Vec<AuthoredObligation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Refuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    OutOfRange,
    NotAware,
    InvalidTransition,
}

impl SurrenderInputs {
    pub fn validate(&self) -> Result<(), RuleError> {
        if !(-100..=100).contains(&self.morale)
            || self.fear > 100
            || self.incapacitation_bps > 10_000
            || !(-100..=100).contains(&self.affinity)
            || self.familiarity_bps > 10_000
            || !(-100..=100).contains(&self.leverage)
            || self.obligations.len() > 16
        {
            return Err(RuleError::OutOfRange);
        }
        if !self.mutual_awareness {
            return Err(RuleError::NotAware);
        }
        Ok(())
    }
}

/// Deterministic authored decision matrix. Demand is harder than an offer;
/// fear, incapacity, familiarity and leverage favor acceptance, while morale
/// and onerous obligations resist it. No random roll or browser input exists.
pub fn decide_surrender(
    action: SurrenderAction,
    input: &SurrenderInputs,
) -> Result<Decision, RuleError> {
    input.validate()?;
    let pressure = i32::from(input.fear) * 2
        + i32::from(input.incapacitation_bps / 100)
        + i32::from(input.affinity)
        + i32::from(input.familiarity_bps / 200)
        + i32::from(input.leverage) * 2
        - i32::from(input.morale) * 2
        - i32::try_from(input.obligations.len()).unwrap_or(16) * 12;
    let threshold = match action {
        SurrenderAction::Offer => -20,
        SurrenderAction::Demand => 35,
        _ => return Err(RuleError::InvalidTransition),
    };
    Ok(if pressure >= threshold {
        Decision::Accept
    } else {
        Decision::Refuse
    })
}

pub fn next_disposition(
    current: ContextDisposition,
    action: SurrenderAction,
) -> Result<ContextDisposition, RuleError> {
    use ContextDisposition as D;
    use SurrenderAction as A;
    match (current, action) {
        (D::Hostile | D::Refused, A::Offer) => Ok(D::OfferPending),
        (D::Hostile | D::Refused, A::Demand) => Ok(D::DemandPending),
        (D::OfferPending | D::DemandPending, A::Accept) => Ok(D::Surrendered),
        (D::OfferPending | D::DemandPending, A::Refuse) => Ok(D::Refused),
        _ => Err(RuleError::InvalidTransition),
    }
}

/// Shared precombat/tactical transition. A trusted tactical adapter may attest
/// that an active hostile yielded; browser-driven acceptance still requires a
/// pending authored offer or demand.
pub fn resolve_surrender_transition(
    current: ContextDisposition,
    action: SurrenderAction,
    tactical_yield: bool,
) -> Result<ContextDisposition, RuleError> {
    if tactical_yield {
        return match (current, action) {
            (
                ContextDisposition::Hostile | ContextDisposition::Refused,
                SurrenderAction::Accept,
            ) => Ok(ContextDisposition::Surrendered),
            _ => Err(RuleError::InvalidTransition),
        };
    }
    next_disposition(current, action)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustodyStatus {
    Captive,
    Released,
    Escaped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Custodian {
    None,
    Party(String),
    Character(u64),
    Faction(String),
    Site(String),
}

impl Custodian {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyAction {
    Capture,
    Handoff,
    Release,
    Escape,
    RansomRelease,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustodyTransition {
    pub current: Option<(CustodyStatus, Custodian)>,
    pub action: CustodyAction,
    pub destination: Custodian,
    pub target_surrendered_or_incapacitated: bool,
    pub actor_controls_current: bool,
    pub actor_controls_destination: bool,
    pub actor_is_captive: bool,
    pub co_located: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyError {
    InvalidState,
    InvalidCustodian,
    Unauthorized,
    NotCoLocated,
    TargetDidNotYield,
}

/// Canonical custody state machine. Persistence adapters validate that typed
/// custodians exist before passing `actor_controls_destination`.
pub fn validate_custody_transition(
    input: &CustodyTransition,
) -> Result<CustodyStatus, CustodyError> {
    if !input.co_located && !matches!(input.action, CustodyAction::Escape) {
        return Err(CustodyError::NotCoLocated);
    }
    match input.action {
        CustodyAction::Capture => {
            if input.destination.is_none() {
                return Err(CustodyError::InvalidCustodian);
            }
            if !input.target_surrendered_or_incapacitated {
                return Err(CustodyError::TargetDidNotYield);
            }
            if !input.actor_controls_destination {
                return Err(CustodyError::Unauthorized);
            }
            if input
                .current
                .as_ref()
                .is_some_and(|(status, _)| *status == CustodyStatus::Captive)
            {
                return Err(CustodyError::InvalidState);
            }
            Ok(CustodyStatus::Captive)
        }
        CustodyAction::Handoff => {
            if input.destination.is_none() {
                return Err(CustodyError::InvalidCustodian);
            }
            if !matches!(input.current, Some((CustodyStatus::Captive, _))) {
                return Err(CustodyError::InvalidState);
            }
            if !input.actor_controls_current || !input.actor_controls_destination {
                return Err(CustodyError::Unauthorized);
            }
            Ok(CustodyStatus::Captive)
        }
        CustodyAction::Release | CustodyAction::RansomRelease => {
            if !input.destination.is_none() {
                return Err(CustodyError::InvalidCustodian);
            }
            if !matches!(input.current, Some((CustodyStatus::Captive, _))) {
                return Err(CustodyError::InvalidState);
            }
            if input.action == CustodyAction::Release && !input.actor_controls_current {
                return Err(CustodyError::Unauthorized);
            }
            Ok(CustodyStatus::Released)
        }
        CustodyAction::Escape => {
            if !input.destination.is_none() {
                return Err(CustodyError::InvalidCustodian);
            }
            if !matches!(input.current, Some((CustodyStatus::Captive, _))) {
                return Err(CustodyError::InvalidState);
            }
            if !input.actor_is_captive {
                return Err(CustodyError::Unauthorized);
            }
            Ok(CustodyStatus::Escaped)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecruitmentPreflight {
    pub destination_exists: bool,
    pub actor_leads_destination: bool,
    pub active_contact: bool,
    pub mutual_awareness: bool,
    pub co_located: bool,
    pub disposition: ContextDisposition,
    pub expected_revision: u32,
    pub actual_revision: u32,
    pub captive: bool,
    pub existing_control_grant: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecruitmentError {
    MissingDestination,
    UnauthorizedDestination,
    MissingContact,
    NotAware,
    NotCoLocated,
    NoConsent,
    StaleDisposition,
    Captive,
    AlreadyControlled,
}

pub fn validate_recruitment(input: &RecruitmentPreflight) -> Result<(), RecruitmentError> {
    if !input.destination_exists {
        return Err(RecruitmentError::MissingDestination);
    }
    if !input.actor_leads_destination {
        return Err(RecruitmentError::UnauthorizedDestination);
    }
    if !input.active_contact {
        return Err(RecruitmentError::MissingContact);
    }
    if !input.mutual_awareness {
        return Err(RecruitmentError::NotAware);
    }
    if !input.co_located {
        return Err(RecruitmentError::NotCoLocated);
    }
    if input.expected_revision != input.actual_revision {
        return Err(RecruitmentError::StaleDisposition);
    }
    if input.disposition != ContextDisposition::Surrendered {
        return Err(RecruitmentError::NoConsent);
    }
    if input.captive {
        return Err(RecruitmentError::Captive);
    }
    if input.existing_control_grant {
        return Err(RecruitmentError::AlreadyControlled);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessCandidate {
    pub character_id: u64,
    pub co_present: bool,
    pub aware: bool,
}

pub fn derive_theft_witnesses(actor_id: u64, candidates: &[WitnessCandidate]) -> Vec<u64> {
    let mut witnesses: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            candidate.character_id != actor_id && candidate.co_present && candidate.aware
        })
        .map(|candidate| candidate.character_id)
        .collect();
    witnesses.sort_unstable();
    witnesses.dedup();
    witnesses
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObligationEffect {
    Disarm {
        beneficiary_id: String,
    },
    LeaveSite,
    PayRansom {
        beneficiary_id: String,
        amount_minor: u64,
    },
    EnterCustody {
        custodian_id: String,
    },
    Testify {
        beneficiary_id: String,
    },
}

pub fn obligation_effects(
    obligations: &[AuthoredObligation],
) -> Result<Vec<ObligationEffect>, RuleError> {
    if obligations.len() > 16 {
        return Err(RuleError::OutOfRange);
    }
    let mut entered_custody = false;
    obligations
        .iter()
        .map(|obligation| {
            if obligation.beneficiary_id.is_empty()
                && !matches!(obligation.kind, ObligationKind::LeaveSite)
            {
                return Err(RuleError::OutOfRange);
            }
            Ok(match obligation.kind {
                ObligationKind::Disarm => ObligationEffect::Disarm {
                    beneficiary_id: obligation.beneficiary_id.clone(),
                },
                ObligationKind::LeaveSite => ObligationEffect::LeaveSite,
                ObligationKind::PayRansom => {
                    if obligation.amount_minor == 0 || !entered_custody {
                        return Err(RuleError::OutOfRange);
                    }
                    ObligationEffect::PayRansom {
                        beneficiary_id: obligation.beneficiary_id.clone(),
                        amount_minor: obligation.amount_minor,
                    }
                }
                ObligationKind::EnterCustody => {
                    entered_custody = true;
                    ObligationEffect::EnterCustody {
                        custodian_id: obligation.beneficiary_id.clone(),
                    }
                }
                ObligationKind::Testify => ObligationEffect::Testify {
                    beneficiary_id: obligation.beneficiary_id.clone(),
                },
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyOwnerKind {
    Personal,
    Party,
    Faction,
    Abandoned,
    Corpse,
}

pub fn transfer_is_theft(owner: PropertyOwnerKind, authorized: bool) -> bool {
    !authorized
        && matches!(
            owner,
            PropertyOwnerKind::Personal | PropertyOwnerKind::Party | PropertyOwnerKind::Faction
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    ZeroAmount,
    StaleVersion,
    Insufficient,
    Overflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferRequest {
    pub source_id: String,
    pub property_id: String,
    pub amount: u64,
    pub expected_version: u32,
    pub from_owner: String,
    pub to_owner: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferReceipt {
    pub request: TransferRequest,
    pub source_after: u64,
    pub destination_after: u64,
    pub resulting_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyTransferError {
    Balance(TransferError),
    ConflictingSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactRetryError {
    ConflictingSource,
}

/// Persistence-neutral exact-source contract shared conceptually by every
/// reducer receipt: identical retry is a no-op; changed coordinates conflict.
pub fn record_exact_request<T: Clone + PartialEq>(
    receipts: &mut BTreeMap<String, T>,
    source_id: &str,
    request: T,
) -> Result<bool, ExactRetryError> {
    if let Some(existing) = receipts.get(source_id) {
        return if existing == &request {
            Ok(false)
        } else {
            Err(ExactRetryError::ConflictingSource)
        };
    }
    receipts.insert(source_id.to_owned(), request);
    Ok(true)
}

pub fn apply_transfer(
    receipts: &mut BTreeMap<String, TransferReceipt>,
    request: TransferRequest,
    source: u64,
    destination: u64,
    actual_version: u32,
) -> Result<(TransferReceipt, bool), ApplyTransferError> {
    if let Some(receipt) = receipts.get(&request.source_id) {
        return if receipt.request == request {
            Ok((receipt.clone(), false))
        } else {
            Err(ApplyTransferError::ConflictingSource)
        };
    }
    let (source_after, destination_after) = transfer_balances(
        source,
        destination,
        request.amount,
        request.expected_version,
        actual_version,
    )
    .map_err(ApplyTransferError::Balance)?;
    let receipt = TransferReceipt {
        request: request.clone(),
        source_after,
        destination_after,
        resulting_version: actual_version
            .checked_add(1)
            .ok_or(ApplyTransferError::Balance(TransferError::Overflow))?,
    };
    receipts.insert(request.source_id.clone(), receipt.clone());
    Ok((receipt, true))
}

/// Checked conservation preflight shared by item and currency adapters.
pub fn transfer_balances(
    source: u64,
    destination: u64,
    amount: u64,
    expected_version: u32,
    actual_version: u32,
) -> Result<(u64, u64), TransferError> {
    if amount == 0 {
        return Err(TransferError::ZeroAmount);
    }
    if expected_version != actual_version {
        return Err(TransferError::StaleVersion);
    }
    let source = source
        .checked_sub(amount)
        .ok_or(TransferError::Insufficient)?;
    let destination = destination
        .checked_add(amount)
        .ok_or(TransferError::Overflow)?;
    Ok((source, destination))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalLot {
    pub property_id: String,
    pub binding_id: String,
    pub quantity: u64,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LotTransferRequest {
    pub source_id: String,
    pub source_property_id: String,
    pub expected_binding_id: String,
    pub destination_property_id: String,
    pub destination_binding_id: String,
    pub quantity: u64,
    pub expected_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LotTransferReceipt {
    pub request: LotTransferRequest,
    pub source_after: PhysicalLot,
    pub destination_after: PhysicalLot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LotTransferError {
    MissingSource,
    MissingDestination,
    BindingMismatch,
    Balance(TransferError),
    ConflictingSource,
}

/// Pure durable-lot ledger used by persistence adapters before mutating their
/// physical inventory rows. Destination lots remain valid sources for later
/// hops; no property ID can substitute for its exact physical binding.
pub fn apply_lot_transfer(
    lots: &mut BTreeMap<String, PhysicalLot>,
    receipts: &mut BTreeMap<String, LotTransferReceipt>,
    request: LotTransferRequest,
) -> Result<(LotTransferReceipt, bool), LotTransferError> {
    if let Some(receipt) = receipts.get(&request.source_id) {
        return if receipt.request == request {
            Ok((receipt.clone(), false))
        } else {
            Err(LotTransferError::ConflictingSource)
        };
    }
    let mut source = lots
        .get(&request.source_property_id)
        .cloned()
        .ok_or(LotTransferError::MissingSource)?;
    if source.binding_id != request.expected_binding_id {
        return Err(LotTransferError::BindingMismatch);
    }
    let mut destination = lots
        .get(&request.destination_property_id)
        .cloned()
        .unwrap_or(PhysicalLot {
            property_id: request.destination_property_id.clone(),
            binding_id: request.destination_binding_id.clone(),
            quantity: 0,
            version: 0,
        });
    if destination.binding_id != request.destination_binding_id {
        return Err(LotTransferError::BindingMismatch);
    }
    let (source_after, destination_after) = transfer_balances(
        source.quantity,
        destination.quantity,
        request.quantity,
        request.expected_version,
        source.version,
    )
    .map_err(LotTransferError::Balance)?;
    source.quantity = source_after;
    source.version = source
        .version
        .checked_add(1)
        .ok_or(LotTransferError::Balance(TransferError::Overflow))?;
    destination.quantity = destination_after;
    destination.version = destination
        .version
        .checked_add(1)
        .ok_or(LotTransferError::Balance(TransferError::Overflow))?;
    lots.insert(source.property_id.clone(), source.clone());
    lots.insert(destination.property_id.clone(), destination.clone());
    let receipt = LotTransferReceipt {
        request: request.clone(),
        source_after: source,
        destination_after: destination,
    };
    receipts.insert(request.source_id.clone(), receipt.clone());
    Ok((receipt, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inputs() -> SurrenderInputs {
        SurrenderInputs {
            morale: 5,
            fear: 60,
            incapacitation_bps: 5_000,
            affinity: 10,
            familiarity_bps: 2_000,
            leverage: 20,
            mutual_awareness: true,
            obligations: vec![],
        }
    }
    #[test]
    fn surrender_matrix_covers_offer_and_demand() {
        assert_eq!(
            decide_surrender(SurrenderAction::Offer, &inputs()),
            Ok(Decision::Accept)
        );
        assert_eq!(
            decide_surrender(SurrenderAction::Demand, &inputs()),
            Ok(Decision::Accept)
        );
    }
    #[test]
    fn refusal_is_durable_and_can_be_renegotiated() {
        assert_eq!(
            next_disposition(ContextDisposition::DemandPending, SurrenderAction::Refuse),
            Ok(ContextDisposition::Refused)
        );
        assert_eq!(
            next_disposition(ContextDisposition::Refused, SurrenderAction::Offer),
            Ok(ContextDisposition::OfferPending)
        );
    }
    #[test]
    fn pending_offer_accepts_and_pending_demand_refuses() {
        assert_eq!(
            next_disposition(ContextDisposition::OfferPending, SurrenderAction::Accept),
            Ok(ContextDisposition::Surrendered)
        );
        assert_eq!(
            next_disposition(ContextDisposition::DemandPending, SurrenderAction::Refuse),
            Ok(ContextDisposition::Refused)
        );
    }
    #[test]
    fn awareness_and_ranges_are_authoritative() {
        let mut i = inputs();
        i.mutual_awareness = false;
        assert_eq!(
            decide_surrender(SurrenderAction::Offer, &i),
            Err(RuleError::NotAware)
        );
        i.mutual_awareness = true;
        i.fear = 101;
        assert_eq!(i.validate(), Err(RuleError::OutOfRange));
    }
    #[test]
    fn transfer_conserves_and_checks_versions() {
        assert_eq!(transfer_balances(10, 4, 3, 2, 2), Ok((7, 7)));
        assert_eq!(
            transfer_balances(10, 4, 3, 1, 2),
            Err(TransferError::StaleVersion)
        );
    }
    #[test]
    fn abandoned_and_corpse_loot_are_not_living_theft() {
        assert!(!transfer_is_theft(PropertyOwnerKind::Abandoned, false));
        assert!(!transfer_is_theft(PropertyOwnerKind::Corpse, false));
        assert!(transfer_is_theft(PropertyOwnerKind::Personal, false));
    }

    #[test]
    fn exact_item_and_currency_retries_conserve_and_conflicts_fail() {
        for property in ["item:sword", "currency:mark"] {
            let mut receipts = BTreeMap::new();
            let request = TransferRequest {
                source_id: format!("transfer:{property}"),
                property_id: property.into(),
                amount: 4,
                expected_version: 2,
                from_owner: "party:a".into(),
                to_owner: "party:b".into(),
            };
            let (first, applied) =
                apply_transfer(&mut receipts, request.clone(), 10, 3, 2).unwrap();
            assert!(applied);
            assert_eq!(first.source_after + first.destination_after, 13);
            let (retry, applied) =
                apply_transfer(&mut receipts, request.clone(), 999, 999, 99).unwrap();
            assert!(!applied);
            assert_eq!(retry, first);
            let mut conflict = request;
            conflict.amount = 5;
            assert_eq!(
                apply_transfer(&mut receipts, conflict, 10, 3, 2),
                Err(ApplyTransferError::ConflictingSource)
            );
        }
    }

    #[test]
    fn surrender_contract_covers_precombat_and_tactical_acceptance() {
        assert_eq!(
            next_disposition(ContextDisposition::Hostile, SurrenderAction::Offer),
            Ok(ContextDisposition::OfferPending)
        );
        assert_eq!(
            next_disposition(ContextDisposition::OfferPending, SurrenderAction::Accept),
            Ok(ContextDisposition::Surrendered)
        );
        assert_eq!(
            next_disposition(ContextDisposition::DemandPending, SurrenderAction::Accept),
            Ok(ContextDisposition::Surrendered)
        );
        assert_eq!(
            resolve_surrender_transition(
                ContextDisposition::Hostile,
                SurrenderAction::Accept,
                true
            ),
            Ok(ContextDisposition::Surrendered)
        );
        assert_eq!(
            resolve_surrender_transition(
                ContextDisposition::Hostile,
                SurrenderAction::Accept,
                false
            ),
            Err(RuleError::InvalidTransition)
        );
    }

    #[test]
    fn failed_recruitment_preflight_is_atomic_for_the_calling_adapter() {
        let mut membership = "hostile-party".to_owned();
        let mut control_grant = None::<String>;
        let result = validate_recruitment(&RecruitmentPreflight {
            destination_exists: true,
            actor_leads_destination: true,
            active_contact: true,
            mutual_awareness: true,
            co_located: true,
            disposition: ContextDisposition::Surrendered,
            expected_revision: 4,
            actual_revision: 5,
            captive: false,
            existing_control_grant: false,
        });
        if result.is_ok() {
            membership = "player-party".into();
            control_grant = Some("owner".into());
        }
        assert_eq!(result, Err(RecruitmentError::StaleDisposition));
        assert_eq!(membership, "hostile-party");
        assert_eq!(control_grant, None);
    }

    #[test]
    fn custody_handoff_release_and_escape_enforce_authority() {
        let captive = Some((CustodyStatus::Captive, Custodian::Party("wardens".into())));
        let handoff = CustodyTransition {
            current: captive.clone(),
            action: CustodyAction::Handoff,
            destination: Custodian::Party("sheriffs".into()),
            target_surrendered_or_incapacitated: true,
            actor_controls_current: true,
            actor_controls_destination: true,
            actor_is_captive: false,
            co_located: true,
        };
        assert_eq!(
            validate_custody_transition(&handoff),
            Ok(CustodyStatus::Captive)
        );
        assert_eq!(
            validate_custody_transition(&CustodyTransition {
                actor_controls_destination: false,
                ..handoff.clone()
            }),
            Err(CustodyError::Unauthorized)
        );
        let escape = CustodyTransition {
            current: captive,
            action: CustodyAction::Escape,
            destination: Custodian::None,
            target_surrendered_or_incapacitated: false,
            actor_controls_current: false,
            actor_controls_destination: false,
            actor_is_captive: true,
            co_located: false,
        };
        assert_eq!(
            validate_custody_transition(&escape),
            Ok(CustodyStatus::Escaped)
        );
        assert_eq!(
            validate_custody_transition(&CustodyTransition {
                actor_is_captive: false,
                ..escape
            }),
            Err(CustodyError::Unauthorized)
        );
        let capture = CustodyTransition {
            current: None,
            action: CustodyAction::Capture,
            destination: Custodian::Party("wardens".into()),
            target_surrendered_or_incapacitated: true,
            actor_controls_current: false,
            actor_controls_destination: true,
            actor_is_captive: false,
            co_located: true,
        };
        assert_eq!(
            validate_custody_transition(&capture),
            Ok(CustodyStatus::Captive)
        );
        assert_eq!(
            validate_custody_transition(&CustodyTransition {
                destination: Custodian::None,
                ..capture.clone()
            }),
            Err(CustodyError::InvalidCustodian)
        );
        assert_eq!(
            validate_custody_transition(&CustodyTransition {
                current: Some((CustodyStatus::Captive, Custodian::Party("wardens".into()))),
                ..capture
            }),
            Err(CustodyError::InvalidState)
        );
    }

    #[test]
    fn theft_witnesses_require_presence_and_awareness_in_each_location_mode() {
        for candidates in [
            vec![
                WitnessCandidate {
                    character_id: 2,
                    co_present: true,
                    aware: true,
                },
                WitnessCandidate {
                    character_id: 3,
                    co_present: true,
                    aware: false,
                },
            ],
            vec![
                WitnessCandidate {
                    character_id: 4,
                    co_present: true,
                    aware: true,
                },
                WitnessCandidate {
                    character_id: 5,
                    co_present: false,
                    aware: true,
                },
            ],
        ] {
            assert_eq!(derive_theft_witnesses(1, &candidates).len(), 1);
        }
    }

    #[test]
    fn authored_obligations_are_typed_and_executable() {
        let effects = obligation_effects(&[
            AuthoredObligation {
                kind: ObligationKind::Disarm,
                beneficiary_id: "party:wardens".into(),
                amount_minor: 0,
            },
            AuthoredObligation {
                kind: ObligationKind::LeaveSite,
                beneficiary_id: String::new(),
                amount_minor: 0,
            },
            AuthoredObligation {
                kind: ObligationKind::EnterCustody,
                beneficiary_id: "party:wardens".into(),
                amount_minor: 0,
            },
            AuthoredObligation {
                kind: ObligationKind::PayRansom,
                beneficiary_id: "party:wardens".into(),
                amount_minor: 25,
            },
            AuthoredObligation {
                kind: ObligationKind::Testify,
                beneficiary_id: "case:road".into(),
                amount_minor: 0,
            },
        ])
        .unwrap();
        assert_eq!(effects.len(), 5);
        assert!(matches!(
            effects[3],
            ObligationEffect::PayRansom {
                amount_minor: 25,
                ..
            }
        ));
    }

    #[test]
    fn multi_hop_item_and_currency_lots_conserve_and_bind_physical_sources() {
        for kind in ["item", "currency"] {
            let mut lots = BTreeMap::from([(
                "a".into(),
                PhysicalLot {
                    property_id: "a".into(),
                    binding_id: format!("{kind}:row:1"),
                    quantity: 10,
                    version: 0,
                },
            )]);
            let mut receipts = BTreeMap::new();
            let first = LotTransferRequest {
                source_id: format!("{kind}:hop:1"),
                source_property_id: "a".into(),
                expected_binding_id: format!("{kind}:row:1"),
                destination_property_id: "b".into(),
                destination_binding_id: format!("{kind}:row:2"),
                quantity: 6,
                expected_version: 0,
            };
            assert!(
                apply_lot_transfer(&mut lots, &mut receipts, first.clone())
                    .unwrap()
                    .1
            );
            assert!(
                !apply_lot_transfer(&mut lots, &mut receipts, first.clone())
                    .unwrap()
                    .1
            );
            let second = LotTransferRequest {
                source_id: format!("{kind}:hop:2"),
                source_property_id: "b".into(),
                expected_binding_id: format!("{kind}:row:2"),
                destination_property_id: "c".into(),
                destination_binding_id: format!("{kind}:row:3"),
                quantity: 4,
                expected_version: 1,
            };
            apply_lot_transfer(&mut lots, &mut receipts, second).unwrap();
            let full_b = LotTransferRequest {
                source_id: format!("{kind}:hop:3"),
                source_property_id: "b".into(),
                expected_binding_id: format!("{kind}:row:2"),
                destination_property_id: "c".into(),
                destination_binding_id: format!("{kind}:row:3"),
                quantity: 2,
                expected_version: 2,
            };
            apply_lot_transfer(&mut lots, &mut receipts, full_b).unwrap();
            let full_a = LotTransferRequest {
                source_id: format!("{kind}:hop:4"),
                source_property_id: "a".into(),
                expected_binding_id: format!("{kind}:row:1"),
                destination_property_id: "c".into(),
                destination_binding_id: format!("{kind}:row:3"),
                quantity: 4,
                expected_version: 1,
            };
            apply_lot_transfer(&mut lots, &mut receipts, full_a).unwrap();
            assert_eq!(lots.values().map(|lot| lot.quantity).sum::<u64>(), 10);
            let mut conflict = first;
            conflict.quantity = 5;
            assert_eq!(
                apply_lot_transfer(&mut lots, &mut receipts, conflict),
                Err(LotTransferError::ConflictingSource)
            );
            let bypass = LotTransferRequest {
                source_id: format!("{kind}:bypass"),
                source_property_id: "c".into(),
                expected_binding_id: "forged".into(),
                destination_property_id: "d".into(),
                destination_binding_id: "row:4".into(),
                quantity: 1,
                expected_version: 1,
            };
            assert_eq!(
                apply_lot_transfer(&mut lots, &mut receipts, bypass),
                Err(LotTransferError::BindingMismatch)
            );
        }
    }

    #[test]
    fn custody_recruitment_and_ransom_receipts_have_exact_retry_semantics() {
        for operation in ["custody:handoff", "recruit:context", "ransom:payment"] {
            let mut receipts = BTreeMap::new();
            let coordinates = (operation.to_owned(), 7_u64, 3_u32);
            assert_eq!(
                record_exact_request(&mut receipts, "request:1", coordinates.clone()),
                Ok(true)
            );
            assert_eq!(
                record_exact_request(&mut receipts, "request:1", coordinates),
                Ok(false)
            );
            assert_eq!(
                record_exact_request(&mut receipts, "request:1", (operation.to_owned(), 7, 4)),
                Err(ExactRetryError::ConflictingSource)
            );
        }
    }
}
