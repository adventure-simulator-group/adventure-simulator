//! Settlement residences: permanent three-tier offers, durable legal
//! holdings, one designated primary home, explicit occupancy, and
//! chronological recurring billing.

use std::collections::BTreeMap;

use adventuresim_core::courtship::{
    HOUSING_BILLING_PERIOD_MINUTES, HousingTier as CoreHousingTier, RESIDENCE_MORALE_SPEC,
    RefreshableMorale, refresh_bounded_leisure_morale, residence_leisure_bonus_milli,
    residence_period_charge,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::character;
use crate::condition::{MoraleEvent, morale_event};
use crate::relationship::{HouseholdRole, character_kinship, household_member};
use crate::strategic::{settlement, strategic_gateway_authority__view};
use crate::time::{character_time, character_time__view};

pub const RESIDENCE_BILLING_PERIOD_MINUTES: u64 = HOUSING_BILLING_PERIOD_MINUTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ResidenceTier {
    Cheap,
    Moderate,
    Fancy,
}

impl ResidenceTier {
    pub const ALL: [Self; 3] = [Self::Cheap, Self::Moderate, Self::Fancy];

    const fn core(self) -> CoreHousingTier {
        match self {
            Self::Cheap => CoreHousingTier::Cheap,
            Self::Moderate => CoreHousingTier::Moderate,
            Self::Fancy => CoreHousingTier::Fancy,
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Moderate => "moderate",
            Self::Fancy => "fancy",
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_residence_offer, public)]
pub struct SettlementResidenceOffer {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub tier: ResidenceTier,
    pub purchase_price: u32,
    pub rent_per_period: u32,
    pub owner_maintenance_per_period: u32,
    pub property_tax_per_period: u32,
    pub leisure_morale_basis_points: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ResidenceTenure {
    Renter,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ResidenceHoldingStatus {
    Active,
    Dormant,
    Relinquished,
}

/// Durable legal interest in one physical home. An owner may retain any
/// number of purchased holdings; a renter may have at most one active rental.
/// The stable ID includes the owner-local acquisition ordinal so buying the
/// same tier in the same settlement twice never overwrites history.
#[derive(Clone, Debug)]
#[table(accessor = residence_holding)]
pub struct ResidenceHolding {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    pub tier: ResidenceTier,
    pub tenure: ResidenceTenure,
    pub status: ResidenceHoldingStatus,
    pub acquired_ordinal: u64,
    pub acquired_minute: u64,
    pub last_billed_minute: u64,
    pub next_due_minute: u64,
    pub resolved_minute: Option<u64>,
}

/// Exactly one designated home per character, independent of how many legal
/// holdings they own.
#[derive(Clone, Debug)]
#[table(accessor = primary_residence)]
pub struct PrimaryResidence {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub holding_id: String,
    pub designated_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ResidenceChargeOutcome {
    Paid,
    Unpaid,
}

/// Immutable one-period ledger receipt. The components make property cost and
/// supported-household necessities auditable without exposing this private
/// authority table directly to clients.
#[derive(Clone, Debug)]
#[table(accessor = residence_charge)]
pub struct ResidenceCharge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub holding_id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub due_minute: u64,
    pub base_housing_amount: u64,
    pub adult_necessities_amount: u64,
    pub dependent_necessities_amount: u64,
    pub amount: u64,
    pub supported_adults: u32,
    pub supported_dependents: u32,
    pub outcome: ResidenceChargeOutcome,
    pub recorded_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = residence_occupant)]
pub struct ResidenceOccupant {
    /// A character can occupy at most one home at a time.
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub holding_id: String,
    pub admitted_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ResidenceTransitionKind {
    Acquired,
    Designated,
    Relinquished,
    Dormant,
    Recovered,
    OccupantAdmitted,
    OccupantRemoved,
}

#[derive(Clone, Debug)]
#[table(accessor = residence_transition)]
pub struct ResidenceTransition {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub holding_id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub affected_character_id: u64,
    pub kind: ResidenceTransitionKind,
    pub minute: u64,
}

/// Gateway-only summary of the home a character may currently use. Legal
/// holdings, billing ledgers, and the identities of unrelated occupants remain
/// private.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCharacterResidenceStatus {
    pub character_id: u64,
    pub holding_id: String,
    pub owner_character_id: u64,
    pub settlement_id: String,
    pub tier: ResidenceTier,
    pub tenure: ResidenceTenure,
    pub active: bool,
    pub primary: bool,
    pub occupied: bool,
    pub acquired_minute: u64,
    pub last_billed_minute: u64,
    pub next_due_minute: u64,
}

fn residence_view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

fn holding_active_at_view(ctx: &ViewContext, holding_id: &str, minute: u64) -> bool {
    let mut transitions = ctx
        .db
        .residence_transition()
        .holding_id()
        .filter(&holding_id.to_owned())
        .filter(|transition| transition.minute <= minute)
        .collect::<Vec<_>>();
    transitions.sort_by_key(|transition| {
        (
            transition.minute,
            residence_transition_precedence(transition.kind),
            transition.id.clone(),
        )
    });
    transitions
        .into_iter()
        .fold(false, |active, transition| match transition.kind {
            ResidenceTransitionKind::Acquired | ResidenceTransitionKind::Recovered => true,
            ResidenceTransitionKind::Dormant | ResidenceTransitionKind::Relinquished => false,
            ResidenceTransitionKind::Designated
            | ResidenceTransitionKind::OccupantAdmitted
            | ResidenceTransitionKind::OccupantRemoved => active,
        })
}

fn occupant_holding_id_at_view(
    ctx: &ViewContext,
    character_id: u64,
    minute: u64,
) -> Option<String> {
    let mut transitions = ctx
        .db
        .residence_transition()
        .affected_character_id()
        .filter(character_id)
        .filter(|transition| transition.minute <= minute)
        .filter(|transition| {
            matches!(
                transition.kind,
                ResidenceTransitionKind::OccupantAdmitted
                    | ResidenceTransitionKind::OccupantRemoved
            )
        })
        .collect::<Vec<_>>();
    transitions.sort_by_key(|transition| {
        (
            transition.minute,
            residence_transition_precedence(transition.kind),
            transition.id.clone(),
        )
    });
    transitions
        .into_iter()
        .fold(None, |holding_id, transition| match transition.kind {
            ResidenceTransitionKind::OccupantAdmitted => Some(transition.holding_id),
            ResidenceTransitionKind::OccupantRemoved
                if holding_id.as_deref() == Some(transition.holding_id.as_str()) =>
            {
                None
            }
            _ => holding_id,
        })
}

#[view(accessor = backend_character_residence_statuses, public)]
pub fn backend_character_residence_statuses(
    ctx: &ViewContext,
) -> Vec<BackendCharacterResidenceStatus> {
    if !residence_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .residence_holding()
        .owner_character_id()
        .filter(0u64..)
        .flat_map(|holding| {
            let holding_id = holding.id.clone();
            let mut character_ids = ctx
                .db
                .residence_occupant()
                .holding_id()
                .filter(&holding_id)
                .filter(|row| {
                    ctx.db
                        .character_time()
                        .character_id()
                        .find(row.character_id)
                        .is_some_and(|time| row.admitted_minute <= time.minutes)
                })
                .map(|row| row.character_id)
                .collect::<Vec<_>>();
            // Legal ownership must remain visible to the owner even after a
            // different holding becomes primary and moves their occupancy.
            // Otherwise an unoccupied property continues billing privately
            // but cannot be inspected or managed through the gateway.
            character_ids.push(holding.owner_character_id);
            character_ids.extend(
                ctx.db
                    .primary_residence()
                    .holding_id()
                    .filter(&holding_id)
                    .filter(|row| {
                        ctx.db
                            .character_time()
                            .character_id()
                            .find(row.character_id)
                            .is_some_and(|time| row.designated_minute <= time.minutes)
                    })
                    .map(|row| row.character_id),
            );
            character_ids.extend(
                ctx.db
                    .residence_transition()
                    .holding_id()
                    .filter(&holding_id)
                    .map(|transition| transition.affected_character_id),
            );
            character_ids.sort_unstable();
            character_ids.dedup();
            character_ids
                .into_iter()
                .map(move |character_id| (holding.clone(), character_id))
        })
        .filter_map(|(holding, character_id)| {
            let character_minute = ctx
                .db
                .character_time()
                .character_id()
                .find(character_id)
                .map(|time| time.minutes)?;
            if holding.acquired_minute > character_minute
                || holding
                    .resolved_minute
                    .is_some_and(|resolved| resolved <= character_minute)
            {
                return None;
            }
            let occupancy_holding_id =
                occupant_holding_id_at_view(ctx, character_id, character_minute);
            let primary = ctx.db.primary_residence().character_id().find(character_id);
            let owns_holding = holding.owner_character_id == character_id;
            Some(BackendCharacterResidenceStatus {
                character_id,
                holding_id: holding.id.clone(),
                owner_character_id: holding.owner_character_id,
                settlement_id: holding.settlement_id,
                tier: holding.tier,
                tenure: holding.tenure,
                active: holding_active_at_view(ctx, &holding.id, character_minute),
                primary: primary.as_ref().is_some_and(|row| {
                    row.holding_id == holding.id && row.designated_minute <= character_minute
                }),
                occupied: occupancy_holding_id.as_deref() == Some(holding.id.as_str()),
                acquired_minute: holding.acquired_minute,
                // Billing is the owner's private economic state. Household
                // occupants need the home and comfort facts, not timestamps
                // that may have been advanced beyond their personal date.
                last_billed_minute: owns_holding
                    .then_some(holding.last_billed_minute)
                    .unwrap_or(0),
                next_due_minute: owns_holding.then_some(holding.next_due_minute).unwrap_or(0),
            })
        })
        .collect()
}

pub fn offer_id(settlement_id: &str, tier: ResidenceTier) -> String {
    format!("residence:{settlement_id}:{}", tier.id())
}

fn holding_id(
    owner_character_id: u64,
    settlement_id: &str,
    tier: ResidenceTier,
    acquired_ordinal: u64,
) -> String {
    format!(
        "residence-holding:{owner_character_id}:{settlement_id}:{}:{acquired_ordinal}",
        tier.id()
    )
}

fn transition_id(
    holding_id: &str,
    affected_character_id: u64,
    minute: u64,
    kind: ResidenceTransitionKind,
) -> String {
    format!("residence-transition:{holding_id}:{affected_character_id}:{minute}:{kind:?}")
}

fn record_transition(
    ctx: &ReducerContext,
    holding: &ResidenceHolding,
    affected_character_id: u64,
    minute: u64,
    kind: ResidenceTransitionKind,
) {
    let id = transition_id(&holding.id, affected_character_id, minute, kind);
    if ctx.db.residence_transition().id().find(&id).is_none() {
        ctx.db.residence_transition().insert(ResidenceTransition {
            id,
            holding_id: holding.id.clone(),
            owner_character_id: holding.owner_character_id,
            affected_character_id,
            kind,
            minute,
        });
    }
}

/// Idempotently creates the universal three offers for a settlement.
pub fn ensure_settlement_residence_offers(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    if ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_owned())
        .is_none()
    {
        return Err("Settlement not found".into());
    }
    for tier in ResidenceTier::ALL {
        let id = offer_id(settlement_id, tier);
        if ctx.db.settlement_residence_offer().id().find(&id).is_some() {
            continue;
        }
        let economy = tier.core().economy();
        ctx.db
            .settlement_residence_offer()
            .insert(SettlementResidenceOffer {
                id,
                settlement_id: settlement_id.to_owned(),
                tier,
                purchase_price: economy.purchase_price,
                rent_per_period: economy.rent_per_30_days,
                owner_maintenance_per_period: economy.owner_maintenance_per_30_days,
                property_tax_per_period: economy.property_tax_per_30_days,
                leisure_morale_basis_points: economy.leisure_morale_basis_points,
            });
    }
    Ok(())
}

fn residence_now(ctx: &ReducerContext, character_id: u64) -> Result<u64, String> {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|row| row.minutes)
        .ok_or_else(|| "Character time record not found".to_string())
}

fn offer(
    ctx: &ReducerContext,
    settlement_id: &str,
    tier: ResidenceTier,
) -> Result<SettlementResidenceOffer, String> {
    ctx.db
        .settlement_residence_offer()
        .id()
        .find(&offer_id(settlement_id, tier))
        .ok_or_else(|| "Residence offer not found".to_string())
}

/// Private projection seam used by reducers and the authenticated gateway.
/// Legal holdings and their ledgers remain private.
pub fn primary_residence_holding(
    ctx: &ReducerContext,
    character_id: u64,
) -> Option<ResidenceHolding> {
    let minute = residence_now(ctx, character_id).ok()?;
    let primary = ctx
        .db
        .primary_residence()
        .character_id()
        .find(character_id)
        .filter(|row| row.designated_minute <= minute)?;
    ctx.db
        .residence_holding()
        .id()
        .find(&primary.holding_id)
        .filter(|row| holding_active_at(ctx, &row.id, minute))
}

pub fn active_primary_residence(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
) -> Option<ResidenceHolding> {
    primary_residence_holding(ctx, character_id)
        .filter(|holding| holding.settlement_id == settlement_id)
}

/// Resolve the active home an occupant is entitled to use through occupancy,
/// not through ownership.
pub fn active_residence_for_occupant(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
) -> Option<ResidenceHolding> {
    let minute = residence_now(ctx, character_id).ok()?;
    let holding_id = occupant_holding_id_at(ctx, character_id, minute)?;
    ctx.db
        .residence_holding()
        .id()
        .find(&holding_id)
        .filter(|row| holding_active_at(ctx, &row.id, minute) && row.settlement_id == settlement_id)
}

/// Whether a holding was usable at an effective personal minute. Current
/// status alone is insufficient because a later unpaid bill or recovery may
/// have changed the mutable row after the event being materialized.
pub(crate) fn holding_active_at(ctx: &ReducerContext, holding_id: &str, minute: u64) -> bool {
    let mut transitions = ctx
        .db
        .residence_transition()
        .holding_id()
        .filter(&holding_id.to_owned())
        .filter(|transition| transition.minute <= minute)
        .collect::<Vec<_>>();
    transitions.sort_by_key(|transition| {
        (
            transition.minute,
            residence_transition_precedence(transition.kind),
            transition.id.clone(),
        )
    });
    transitions
        .into_iter()
        .fold(false, |active, transition| match transition.kind {
            ResidenceTransitionKind::Acquired | ResidenceTransitionKind::Recovered => true,
            ResidenceTransitionKind::Dormant | ResidenceTransitionKind::Relinquished => false,
            ResidenceTransitionKind::Designated
            | ResidenceTransitionKind::OccupantAdmitted
            | ResidenceTransitionKind::OccupantRemoved => active,
        })
}

const fn residence_transition_precedence(kind: ResidenceTransitionKind) -> u8 {
    match kind {
        ResidenceTransitionKind::Acquired => 0,
        ResidenceTransitionKind::Designated => 1,
        ResidenceTransitionKind::OccupantRemoved => 2,
        ResidenceTransitionKind::OccupantAdmitted => 3,
        ResidenceTransitionKind::Dormant => 4,
        ResidenceTransitionKind::Recovered => 5,
        ResidenceTransitionKind::Relinquished => 6,
    }
}

pub(crate) fn occupant_holding_id_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Option<String> {
    let mut transitions = ctx
        .db
        .residence_transition()
        .affected_character_id()
        .filter(character_id)
        .filter(|transition| transition.minute <= minute)
        .filter(|transition| {
            matches!(
                transition.kind,
                ResidenceTransitionKind::OccupantAdmitted
                    | ResidenceTransitionKind::OccupantRemoved
            )
        })
        .collect::<Vec<_>>();
    transitions.sort_by_key(|transition| {
        (
            transition.minute,
            residence_transition_precedence(transition.kind),
            transition.id.clone(),
        )
    });
    transitions
        .into_iter()
        .fold(None, |holding_id, transition| match transition.kind {
            ResidenceTransitionKind::OccupantAdmitted => Some(transition.holding_id),
            ResidenceTransitionKind::OccupantRemoved
                if holding_id.as_deref() == Some(transition.holding_id.as_str()) =>
            {
                None
            }
            _ => holding_id,
        })
}

/// Apply an occupancy transition at its effective minute without overwriting
/// a newer current pointer. This is used by delayed shared lifecycle events.
pub(crate) fn move_residence_occupant_effective(
    ctx: &ReducerContext,
    holding_id: &str,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    let holding = ctx
        .db
        .residence_holding()
        .id()
        .find(holding_id.to_owned())
        .filter(|row| holding_active_at(ctx, &row.id, minute))
        .ok_or("Residence holding was not active at the effective minute")?;
    if occupant_holding_id_at(ctx, character_id, minute).as_deref() == Some(holding_id) {
        return Ok(());
    }
    if let Some(previous_id) = occupant_holding_id_at(ctx, character_id, minute)
        && let Some(previous) = ctx.db.residence_holding().id().find(&previous_id)
    {
        record_transition(
            ctx,
            &previous,
            character_id,
            minute,
            ResidenceTransitionKind::OccupantRemoved,
        );
    }
    record_transition(
        ctx,
        &holding,
        character_id,
        minute,
        ResidenceTransitionKind::OccupantAdmitted,
    );
    let has_later_transition = ctx
        .db
        .residence_transition()
        .affected_character_id()
        .filter(character_id)
        .any(|transition| transition.minute > minute);
    if !has_later_transition {
        let row = ResidenceOccupant {
            character_id,
            holding_id: holding.id,
            admitted_minute: minute,
        };
        if ctx
            .db
            .residence_occupant()
            .character_id()
            .find(character_id)
            .is_some()
        {
            ctx.db.residence_occupant().character_id().update(row);
        } else {
            ctx.db.residence_occupant().insert(row);
        }
    }
    Ok(())
}

/// End guest occupancy at an effective lifecycle minute without allowing a
/// delayed marriage resolution to overwrite a newer move. Owners retain their
/// own holding; only the historical non-owner occupancy receives a removal.
pub(crate) fn remove_nonowned_occupancy_effective(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) {
    let Some(holding_id) = occupant_holding_id_at(ctx, character_id, minute) else {
        return;
    };
    let Some(holding) = ctx.db.residence_holding().id().find(&holding_id) else {
        return;
    };
    if holding.owner_character_id == character_id {
        return;
    }
    record_transition(
        ctx,
        &holding,
        character_id,
        minute,
        ResidenceTransitionKind::OccupantRemoved,
    );
    let has_later_transition = ctx
        .db
        .residence_transition()
        .affected_character_id()
        .filter(character_id)
        .any(|transition| transition.minute > minute);
    if !has_later_transition
        && ctx
            .db
            .residence_occupant()
            .character_id()
            .find(character_id)
            .is_some_and(|occupant| occupant.holding_id == holding_id)
    {
        ctx.db
            .residence_occupant()
            .character_id()
            .delete(character_id);
    }
}

pub(crate) fn remove_occupant_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Option<ResidenceOccupant> {
    let occupant = ctx
        .db
        .residence_occupant()
        .character_id()
        .find(character_id)?;
    ctx.db
        .residence_occupant()
        .character_id()
        .delete(character_id);
    if let Some(holding) = ctx.db.residence_holding().id().find(&occupant.holding_id) {
        record_transition(
            ctx,
            &holding,
            character_id,
            minute,
            ResidenceTransitionKind::OccupantRemoved,
        );
    }
    Some(occupant)
}

/// Canonical atomic move used after wedding/birth authority is established.
pub(crate) fn move_residence_occupant_internal(
    ctx: &ReducerContext,
    holding_id: &str,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    let holding = ctx
        .db
        .residence_holding()
        .id()
        .find(holding_id.to_owned())
        .filter(|row| row.status == ResidenceHoldingStatus::Active)
        .ok_or("Active residence holding not found")?;
    if let Some(existing) = ctx
        .db
        .residence_occupant()
        .character_id()
        .find(character_id)
    {
        if existing.holding_id == holding.id {
            return Ok(());
        }
        remove_occupant_at(ctx, character_id, minute);
    }
    ctx.db.residence_occupant().insert(ResidenceOccupant {
        character_id,
        holding_id: holding.id.clone(),
        admitted_minute: minute,
    });
    record_transition(
        ctx,
        &holding,
        character_id,
        minute,
        ResidenceTransitionKind::OccupantAdmitted,
    );
    Ok(())
}

fn admission_relationship_authorized(
    ctx: &ReducerContext,
    owner_character_id: u64,
    occupant_id: u64,
    actor_minute: u64,
) -> bool {
    use crate::relationship::KinshipKind;

    if owner_character_id == occupant_id {
        return true;
    }
    let same_household = ctx
        .db
        .household_member()
        .character_id()
        .find(owner_character_id)
        .zip(ctx.db.household_member().character_id().find(occupant_id))
        .is_some_and(|(owner, occupant)| {
            owner.household_id == occupant.household_id
                && owner.joined_minute <= actor_minute
                && occupant.joined_minute <= actor_minute
        });
    let family = ctx.db.character_kinship().iter().any(|edge| {
        edge.subject_id == owner_character_id
            && edge.related_id == occupant_id
            && edge.established_minute <= actor_minute
            && matches!(
                edge.kind,
                KinshipKind::Spouse | KinshipKind::Parent | KinshipKind::Child
            )
    });
    same_household || family
}

fn validate_public_admission(
    ctx: &ReducerContext,
    holding: &ResidenceHolding,
    occupant_id: u64,
) -> Result<(), String> {
    use crate::relationship::KinshipKind;
    use adventuresim_core::courtship::ADULT_AGE_YEARS;

    let owner = crate::character::require_living_character(ctx, holding.owner_character_id)?;
    let occupant = crate::character::require_living_character(ctx, occupant_id)?;
    let actor_minute = crate::relationship::enforce_temporal_scope(
        ctx,
        holding.owner_character_id,
        Some(occupant_id),
        crate::relationship::TemporalScope::PairwiseSoft,
    )?;
    if owner.current_settlement_id.as_deref() != Some(&holding.settlement_id)
        || occupant.current_settlement_id.as_deref() != Some(&holding.settlement_id)
    {
        return Err("Residence admission requires both characters to be co-located".into());
    }
    let dependent = ctx.db.character_kinship().iter().any(|edge| {
        edge.subject_id == holding.owner_character_id
            && edge.related_id == occupant_id
            && edge.established_minute <= actor_minute
            && matches!(edge.kind, KinshipKind::Parent | KinshipKind::Child)
    });
    if occupant.age_years < ADULT_AGE_YEARS && !dependent {
        return Err("Only an adult or dependent child may occupy a residence".into());
    }
    if !admission_relationship_authorized(
        ctx,
        holding.owner_character_id,
        occupant_id,
        actor_minute,
    ) {
        return Err("Only an active household member or immediate family may be admitted".into());
    }
    if ctx
        .db
        .residence_occupant()
        .character_id()
        .find(occupant_id)
        .is_some_and(|existing| existing.holding_id != holding.id)
    {
        return Err("Character already occupies a different residence".into());
    }
    Ok(())
}

fn occupants_for_holding(ctx: &ReducerContext, holding_id: &str) -> Vec<u64> {
    ctx.db
        .residence_occupant()
        .holding_id()
        .filter(holding_id)
        .map(|row| row.character_id)
        .collect()
}

fn clear_primary_if_holding(ctx: &ReducerContext, character_id: u64, holding_id: &str) {
    if ctx
        .db
        .primary_residence()
        .character_id()
        .find(character_id)
        .is_some_and(|primary| primary.holding_id == holding_id)
    {
        ctx.db
            .primary_residence()
            .character_id()
            .delete(character_id);
    }
}

fn relinquish_holding_at(
    ctx: &ReducerContext,
    character_id: u64,
    holding_id: &str,
    minute: u64,
) -> Result<(), String> {
    let mut holding = ctx
        .db
        .residence_holding()
        .id()
        .find(holding_id.to_owned())
        .filter(|row| row.owner_character_id == character_id)
        .ok_or("Residence holding not found")?;
    if holding.status == ResidenceHoldingStatus::Relinquished {
        return Ok(());
    }
    for occupant_id in occupants_for_holding(ctx, holding_id) {
        remove_occupant_at(ctx, occupant_id, minute);
    }
    clear_primary_if_holding(ctx, character_id, holding_id);
    holding.status = ResidenceHoldingStatus::Relinquished;
    holding.resolved_minute = Some(minute);
    record_transition(
        ctx,
        &holding,
        character_id,
        minute,
        ResidenceTransitionKind::Relinquished,
    );
    ctx.db.residence_holding().id().update(holding);
    Ok(())
}

fn supported_occupant_counts_at(
    ctx: &ReducerContext,
    holding_id: &str,
    due_minute: u64,
) -> (u32, u32) {
    let mut latest: BTreeMap<u64, (u64, bool)> = BTreeMap::new();
    for transition in ctx
        .db
        .residence_transition()
        .holding_id()
        .filter(holding_id)
        .filter(|row| {
            row.minute <= due_minute
                && matches!(
                    row.kind,
                    ResidenceTransitionKind::OccupantAdmitted
                        | ResidenceTransitionKind::OccupantRemoved
                )
        })
    {
        let admitted = transition.kind == ResidenceTransitionKind::OccupantAdmitted;
        let candidate = (transition.minute, admitted);
        if latest
            .get(&transition.affected_character_id)
            .is_none_or(|current| candidate > *current)
        {
            latest.insert(transition.affected_character_id, candidate);
        }
    }
    let mut adults = 0_u32;
    let mut dependents = 0_u32;
    for (character_id, (_, admitted)) in latest {
        if !admitted {
            continue;
        }
        let effective_age = crate::relationship::effective_age_years(ctx, character_id, due_minute)
            .or_else(|| {
                ctx.db
                    .character()
                    .id()
                    .find(character_id)
                    .map(|character| character.age_years)
            });
        let underage =
            effective_age.is_some_and(|age| age < adventuresim_core::courtship::ADULT_AGE_YEARS);
        if underage {
            dependents = dependents.saturating_add(1);
        } else {
            adults = adults.saturating_add(1);
            if let Some(mut member) = ctx
                .db
                .household_member()
                .character_id()
                .find(character_id)
                .filter(|member| {
                    member.joined_minute <= due_minute && member.role == HouseholdRole::Dependent
                })
            {
                member.role = HouseholdRole::AdultChild;
                ctx.db.household_member().id().update(member);
            }
        }
    }
    (adults, dependents)
}

fn period_charge(
    ctx: &ReducerContext,
    holding: &ResidenceHolding,
    due_minute: u64,
) -> Result<
    (
        u32,
        u32,
        adventuresim_core::courtship::ResidencePeriodCharge,
    ),
    String,
> {
    let (adults, dependents) = supported_occupant_counts_at(ctx, &holding.id, due_minute);
    Ok((
        adults,
        dependents,
        residence_period_charge(
            holding.tier.core().economy(),
            holding.tenure == ResidenceTenure::Owner,
            adults,
            dependents,
        ),
    ))
}

fn settle_one_holding_period(
    ctx: &ReducerContext,
    mut holding: ResidenceHolding,
    available: &mut u64,
    total_spent: &mut u64,
) -> Result<(), String> {
    let due_minute = holding.next_due_minute;
    let (adults, dependents, charge) = period_charge(ctx, &holding, due_minute)?;
    let amount = charge.total();
    let paid = *available >= amount;
    let outcome = if paid {
        ResidenceChargeOutcome::Paid
    } else {
        ResidenceChargeOutcome::Unpaid
    };
    let id = format!(
        "residence-charge:{}:{due_minute}:{}",
        holding.id,
        if paid { "paid" } else { "unpaid" }
    );
    if ctx.db.residence_charge().id().find(&id).is_none() {
        ctx.db.residence_charge().insert(ResidenceCharge {
            id,
            holding_id: holding.id.clone(),
            owner_character_id: holding.owner_character_id,
            due_minute,
            base_housing_amount: charge.base_housing,
            adult_necessities_amount: charge.adult_necessities,
            dependent_necessities_amount: charge.dependent_necessities,
            amount,
            supported_adults: adults,
            supported_dependents: dependents,
            outcome,
            recorded_minute: due_minute,
        });
    }
    if !paid {
        holding.status = ResidenceHoldingStatus::Dormant;
        record_transition(
            ctx,
            &holding,
            holding.owner_character_id,
            due_minute,
            ResidenceTransitionKind::Dormant,
        );
        clear_primary_if_holding(ctx, holding.owner_character_id, &holding.id);
        if holding.tenure == ResidenceTenure::Renter {
            for occupant_id in occupants_for_holding(ctx, &holding.id) {
                remove_occupant_at(ctx, occupant_id, due_minute);
            }
        }
    } else {
        *available = available.saturating_sub(amount);
        *total_spent = total_spent.saturating_add(amount);
        holding.last_billed_minute = due_minute;
        holding.next_due_minute = due_minute.saturating_add(RESIDENCE_BILLING_PERIOD_MINUTES);
    }
    ctx.db.residence_holding().id().update(holding);
    Ok(())
}

/// Settle every active legal holding in `(due minute, holding ID)` order.
/// Nonprimary owned properties therefore continue to incur maintenance and
/// property tax. Each due period is indivisible; the first unaffordable bill
/// retains the remaining funds and its authoritative due frontier.
pub fn settle_residence_billing(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let now = residence_now(ctx, character_id)?;
    let mut available = crate::item::personal_currency_total(ctx, character_id);
    let mut total_spent = 0_u64;
    loop {
        let next = ctx
            .db
            .residence_holding()
            .owner_character_id()
            .filter(character_id)
            .filter(|holding| {
                holding.status == ResidenceHoldingStatus::Active && holding.next_due_minute <= now
            })
            .min_by(|left, right| {
                (left.next_due_minute, left.id.as_str())
                    .cmp(&(right.next_due_minute, right.id.as_str()))
            });
        let Some(holding) = next else {
            break;
        };
        settle_one_holding_period(ctx, holding, &mut available, &mut total_spent)?;
    }
    if total_spent > 0 {
        crate::item::consume_personal_currency(ctx, character_id, total_spent)?;
    }
    Ok(())
}

/// Residence comfort is one fixed-duration, refreshable, bounded source.
pub fn apply_residence_leisure_morale(
    ctx: &ReducerContext,
    character_id: u64,
    baseline_morale: f32,
    now: u64,
) -> Result<(), String> {
    if baseline_morale <= 0.0 || !baseline_morale.is_finite() {
        return Ok(());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(holding) = character
        .current_settlement_id
        .as_deref()
        .and_then(|settlement_id| active_residence_for_occupant(ctx, character_id, settlement_id))
    else {
        return Ok(());
    };
    let offer = offer(ctx, &holding.settlement_id, holding.tier)?;
    let earned_milli = residence_leisure_bonus_milli(
        (baseline_morale * 1_000.0).round().max(0.0) as u32,
        offer.leisure_morale_basis_points,
    );
    let source = format!("residence-leisure:{character_id}");
    let existing = ctx
        .db
        .morale_event()
        .character_id()
        .filter(character_id)
        .find(|event| event.source_id.as_deref() == Some(&source));
    let spouse = ctx
        .db
        .morale_event()
        .character_id()
        .filter(character_id)
        .find(|event| {
            event
                .source_id
                .as_deref()
                .is_some_and(|source| source.starts_with("spouse-leisure:"))
        })
        .map_or(RefreshableMorale::default(), |event| RefreshableMorale {
            milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
            expires_at_minute: event.expires_at_minute,
        });
    let refreshed = refresh_bounded_leisure_morale(
        existing
            .as_ref()
            .map_or(RefreshableMorale::default(), |event| RefreshableMorale {
                milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
                expires_at_minute: event.expires_at_minute,
            }),
        spouse,
        now,
        earned_milli,
        RESIDENCE_MORALE_SPEC,
    );
    if earned_milli == 0 {
        return Ok(());
    }
    let event = MoraleEvent {
        id: existing.as_ref().map_or(0, |event| event.id),
        character_id,
        kind: "residence_leisure".into(),
        magnitude: refreshed.milli_points as f32 / 1_000.0,
        occurred_at_minute: now,
        expires_at_minute: refreshed.expires_at_minute,
        source_id: Some(source),
    };
    if existing.is_some() {
        ctx.db.morale_event().id().update(event);
    } else {
        ctx.db.morale_event().insert(event);
    }
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

fn designate_holding_at(
    ctx: &ReducerContext,
    character_id: u64,
    holding_id: &str,
    minute: u64,
) -> Result<(), String> {
    let holding = ctx
        .db
        .residence_holding()
        .id()
        .find(holding_id.to_owned())
        .filter(|row| row.owner_character_id == character_id)
        .filter(|row| row.status == ResidenceHoldingStatus::Active)
        .ok_or("Active residence holding not found")?;
    let primary = PrimaryResidence {
        character_id,
        holding_id: holding.id.clone(),
        designated_minute: minute,
    };
    if ctx
        .db
        .primary_residence()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db.primary_residence().character_id().update(primary);
    } else {
        ctx.db.primary_residence().insert(primary);
    }
    move_residence_occupant_internal(ctx, &holding.id, character_id, minute)?;
    record_transition(
        ctx,
        &holding,
        character_id,
        minute,
        ResidenceTransitionKind::Designated,
    );
    Ok(())
}

fn acquire_residence_internal(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    tier: ResidenceTier,
    tenure: ResidenceTenure,
) -> Result<String, String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.as_deref() != Some(settlement_id) {
        return Err("You must be in a settlement to acquire a residence there".into());
    }
    let offer = offer(ctx, settlement_id, tier)?;
    let initial_charge = match tenure {
        ResidenceTenure::Renter => u64::from(offer.rent_per_period),
        ResidenceTenure::Owner => u64::from(offer.purchase_price),
    };
    crate::item::consume_personal_currency(ctx, character_id, initial_charge)?;
    let now = residence_now(ctx, character_id)?;
    if tenure == ResidenceTenure::Renter {
        let active_rentals: Vec<_> = ctx
            .db
            .residence_holding()
            .owner_character_id()
            .filter(character_id)
            .filter(|row| {
                row.tenure == ResidenceTenure::Renter
                    && row.status != ResidenceHoldingStatus::Relinquished
            })
            .map(|row| row.id)
            .collect();
        for active_rental in active_rentals {
            relinquish_holding_at(ctx, character_id, &active_rental, now)?;
        }
    }
    let acquired_ordinal = ctx
        .db
        .residence_holding()
        .owner_character_id()
        .filter(character_id)
        .count() as u64;
    let id = holding_id(character_id, settlement_id, tier, acquired_ordinal);
    let holding = ResidenceHolding {
        id: id.clone(),
        owner_character_id: character_id,
        settlement_id: settlement_id.to_owned(),
        tier,
        tenure,
        status: ResidenceHoldingStatus::Active,
        acquired_ordinal,
        acquired_minute: now,
        last_billed_minute: now,
        next_due_minute: now.saturating_add(RESIDENCE_BILLING_PERIOD_MINUTES),
        resolved_minute: None,
    };
    ctx.db.residence_holding().insert(holding.clone());
    record_transition(
        ctx,
        &holding,
        character_id,
        now,
        ResidenceTransitionKind::Acquired,
    );
    designate_holding_at(ctx, character_id, &id, now)?;
    Ok(id)
}

fn acquire_residence(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    tier: ResidenceTier,
    tenure: ResidenceTenure,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    acquire_residence_internal(ctx, character_id, settlement_id, tier, tenure).map(|_| ())
}

#[reducer]
pub fn rent_residence(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    tier: ResidenceTier,
) -> Result<(), String> {
    acquire_residence(
        ctx,
        character_id,
        &settlement_id,
        tier,
        ResidenceTenure::Renter,
    )
}

#[reducer]
pub fn buy_residence(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    tier: ResidenceTier,
) -> Result<(), String> {
    acquire_residence(
        ctx,
        character_id,
        &settlement_id,
        tier,
        ResidenceTenure::Owner,
    )
}

#[reducer]
pub fn relinquish_residence(
    ctx: &ReducerContext,
    character_id: u64,
    holding_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let now = residence_now(ctx, character_id)?;
    relinquish_holding_at(ctx, character_id, &holding_id, now)
}

#[reducer]
pub fn designate_residence(
    ctx: &ReducerContext,
    character_id: u64,
    holding_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let now = residence_now(ctx, character_id)?;
    designate_holding_at(ctx, character_id, &holding_id, now)
}

#[reducer]
pub fn recover_owned_residence(
    ctx: &ReducerContext,
    character_id: u64,
    holding_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    recover_owned_residence_internal(ctx, character_id, &holding_id)
}

fn recover_owned_residence_internal(
    ctx: &ReducerContext,
    character_id: u64,
    holding_id: &str,
) -> Result<(), String> {
    let now = residence_now(ctx, character_id)?;
    let mut holding = ctx
        .db
        .residence_holding()
        .id()
        .find(holding_id.to_owned())
        .filter(|row| row.owner_character_id == character_id)
        .ok_or("Residence holding not found")?;
    if holding.tenure != ResidenceTenure::Owner {
        return Err("Only an owned residence can be recovered".into());
    }
    if holding.status == ResidenceHoldingStatus::Relinquished {
        return Err("A relinquished property cannot be recovered".into());
    }
    if holding.status == ResidenceHoldingStatus::Active {
        return Ok(());
    }
    holding.status = ResidenceHoldingStatus::Active;
    ctx.db.residence_holding().id().update(holding.clone());
    settle_residence_billing(ctx, character_id)?;
    let recovered = ctx
        .db
        .residence_holding()
        .id()
        .find(holding_id.to_owned())
        .filter(|row| row.status == ResidenceHoldingStatus::Active)
        .ok_or("Not enough coin to recover the residence")?;
    record_transition(
        ctx,
        &recovered,
        character_id,
        now,
        ResidenceTransitionKind::Recovered,
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NpcResidenceOutcome {
    AlreadyOccupant,
    RecoveredOwner,
    Rented(ResidenceTier),
    NoAffordableOffer,
    NotAtHome,
}

/// Scheduler-only residence policy using the same holdings and billing rules.
pub(crate) fn settle_npc_residence(
    ctx: &ReducerContext,
    character_id: u64,
    home_settlement_id: &str,
) -> Result<NpcResidenceOutcome, String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.as_deref() != Some(home_settlement_id) {
        return Ok(NpcResidenceOutcome::NotAtHome);
    }
    if ctx
        .db
        .residence_occupant()
        .character_id()
        .find(character_id)
        .and_then(|occupancy| ctx.db.residence_holding().id().find(&occupancy.holding_id))
        .is_some_and(|holding| holding.status == ResidenceHoldingStatus::Active)
    {
        return Ok(NpcResidenceOutcome::AlreadyOccupant);
    }
    let owned = ctx
        .db
        .residence_holding()
        .owner_character_id()
        .filter(character_id)
        .filter(|holding| {
            holding.tenure == ResidenceTenure::Owner
                && holding.settlement_id == home_settlement_id
                && holding.status != ResidenceHoldingStatus::Relinquished
        })
        .min_by_key(|holding| (holding.acquired_ordinal, holding.id.clone()));
    if let Some(holding) = owned {
        if holding.status == ResidenceHoldingStatus::Dormant
            && recover_owned_residence_internal(ctx, character_id, &holding.id).is_err()
        {
            return Ok(NpcResidenceOutcome::NoAffordableOffer);
        }
        designate_holding_at(
            ctx,
            character_id,
            &holding.id,
            residence_now(ctx, character_id)?,
        )?;
        return Ok(if holding.status == ResidenceHoldingStatus::Dormant {
            NpcResidenceOutcome::RecoveredOwner
        } else {
            NpcResidenceOutcome::AlreadyOccupant
        });
    }
    ensure_settlement_residence_offers(ctx, home_settlement_id)?;
    let available = crate::item::personal_currency_total(ctx, character_id);
    let mut resolved = Vec::with_capacity(ResidenceTier::ALL.len());
    for tier in ResidenceTier::ALL {
        let row = offer(ctx, home_settlement_id, tier)?;
        resolved.push((
            tier,
            adventuresim_core::npc_policy::NpcHouseOffer {
                rank: match tier {
                    ResidenceTier::Cheap => 0,
                    ResidenceTier::Moderate => 1,
                    ResidenceTier::Fancy => 2,
                },
                initial_cost: u64::from(row.rent_per_period),
                recurring_cost: u64::from(row.rent_per_period),
            },
        ));
    }
    let Some(selected) = adventuresim_core::npc_policy::best_affordable_house(
        available,
        resolved.iter().map(|(_, offer)| *offer),
    ) else {
        return Ok(NpcResidenceOutcome::NoAffordableOffer);
    };
    let tier = resolved
        .into_iter()
        .find_map(|(tier, offer)| (offer == selected).then_some(tier))
        .ok_or("Selected residence offer disappeared")?;
    acquire_residence_internal(
        ctx,
        character_id,
        home_settlement_id,
        tier,
        ResidenceTenure::Renter,
    )?;
    Ok(NpcResidenceOutcome::Rented(tier))
}

#[reducer]
pub fn admit_household_occupant(
    ctx: &ReducerContext,
    owner_character_id: u64,
    holding_id: String,
    occupant_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, owner_character_id)?;
    let now = residence_now(ctx, owner_character_id)?;
    let holding = ctx
        .db
        .residence_holding()
        .id()
        .find(&holding_id)
        .filter(|row| row.owner_character_id == owner_character_id)
        .filter(|row| row.status == ResidenceHoldingStatus::Active)
        .ok_or("Active residence holding not found")?;
    validate_public_admission(ctx, &holding, occupant_id)?;
    move_residence_occupant_internal(ctx, &holding.id, occupant_id, now)
}

#[reducer]
pub fn remove_household_occupant(
    ctx: &ReducerContext,
    owner_character_id: u64,
    holding_id: String,
    occupant_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, owner_character_id)?;
    ctx.db
        .residence_holding()
        .id()
        .find(&holding_id)
        .filter(|row| row.owner_character_id == owner_character_id)
        .ok_or("Residence holding not found")?;
    let existing = ctx
        .db
        .residence_occupant()
        .character_id()
        .find(occupant_id)
        .filter(|row| row.holding_id == holding_id)
        .ok_or("Character does not occupy this residence")?;
    let now = residence_now(ctx, owner_character_id)?;
    remove_occupant_at(ctx, existing.character_id, now);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_and_holding_ids_are_stable() {
        assert_eq!(ResidenceTier::ALL.len(), 3);
        assert_ne!(
            offer_id("lubeck", ResidenceTier::Cheap),
            offer_id("lubeck", ResidenceTier::Fancy)
        );
        assert_ne!(
            holding_id(1, "lubeck", ResidenceTier::Cheap, 0),
            holding_id(1, "lubeck", ResidenceTier::Cheap, 1)
        );
    }

    #[test]
    fn schema_separates_many_holdings_from_one_primary_and_one_occupancy() {
        let source = include_str!("residence.rs");
        assert!(source.contains("#[table(accessor = residence_holding)]"));
        assert!(source.contains("pub struct PrimaryResidence"));
        assert!(source.contains("pub struct ResidenceOccupant"));
        assert!(source.contains("pub holding_id: String"));
        assert!(!source.contains("pub struct CharacterResidence"));
        assert!(!source.contains("#[table(accessor = residence_holding, public)]"));
    }

    #[test]
    fn acquisition_preserves_owned_property_and_replaces_only_a_rental() {
        let source = include_str!("residence.rs");
        let acquisition = source
            .split("fn acquire_residence_internal")
            .nth(1)
            .unwrap()
            .split("fn acquire_residence")
            .next()
            .unwrap();
        assert!(acquisition.contains("tenure == ResidenceTenure::Renter"));
        assert!(acquisition.contains("row.tenure == ResidenceTenure::Renter"));
        assert!(!acquisition.contains("ResidenceTenure::Owner\n"));
        assert!(acquisition.contains("designate_holding_at"));
    }

    #[test]
    fn billing_covers_all_holdings_and_audits_household_line_items() {
        let source = include_str!("residence.rs");
        let billing = source
            .split("pub fn settle_residence_billing")
            .nth(1)
            .unwrap()
            .split("pub fn apply_residence_leisure_morale")
            .next()
            .unwrap();
        assert!(billing.contains("owner_character_id()"));
        assert!(billing.contains(".min_by(|left, right|"));
        assert!(billing.contains("(left.next_due_minute, left.id.as_str())"));
        assert!(billing.contains("settle_one_holding_period"));
        assert!(source.contains("adult_necessities_amount"));
        assert!(source.contains("dependent_necessities_amount"));
        assert!(source.contains("supported_occupant_counts_at"));
        assert!(source.contains("next_due_minute = due_minute.saturating_add"));
    }

    #[test]
    fn necessities_use_effective_age_and_promote_adult_children() {
        let source = include_str!("residence.rs");
        let counts = source
            .split("fn supported_occupant_counts_at")
            .nth(1)
            .unwrap()
            .split("fn period_charge")
            .next()
            .unwrap();
        assert!(counts.contains("effective_age_years(ctx, character_id, due_minute)"));
        assert!(counts.contains("age < adventuresim_core::courtship::ADULT_AGE_YEARS"));
        assert!(counts.contains("member.role = HouseholdRole::AdultChild"));
    }

    #[test]
    fn gateway_projection_keeps_nonprimary_owned_holdings_manageable() {
        let source = include_str!("residence.rs");
        let projection = source
            .split("pub fn backend_character_residence_statuses")
            .nth(1)
            .unwrap()
            .split("pub fn offer_id")
            .next()
            .unwrap();
        assert!(projection.contains("character_ids.push(holding.owner_character_id)"));
        assert!(projection.contains("residence_occupant()"));
        assert!(projection.contains("primary_residence()"));
        assert!(
            projection.contains("let owns_holding = holding.owner_character_id == character_id")
        );
        assert!(projection.contains("owns_holding.then_some(holding.last_billed_minute)"));
        assert!(projection.contains("owns_holding.then_some(holding.next_due_minute)"));
    }

    #[test]
    fn effective_guest_removal_preserves_newer_current_occupancy() {
        let source = include_str!("residence.rs");
        let removal = source
            .split("pub(crate) fn remove_nonowned_occupancy_effective")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn remove_occupant_at")
            .next()
            .unwrap();
        assert!(removal.contains("occupant_holding_id_at(ctx, character_id, minute)"));
        assert!(removal.contains("holding.owner_character_id == character_id"));
        assert!(removal.contains("transition.minute > minute"));
        assert!(removal.contains("occupant.holding_id == holding_id"));
    }

    #[test]
    fn interleaved_holdings_allocate_partial_funds_in_global_due_order() {
        // Two periods for A straddle B's first period. With only ten coins,
        // chronological settlement pays A1 and B1, then records A2 unpaid.
        let mut bills = vec![(10_u64, "a", 6_u64), (30, "a", 6), (20, "b", 4)];
        bills.sort_by_key(|(due, holding, _)| (*due, *holding));
        let mut funds = 10_u64;
        let outcomes: Vec<_> = bills
            .into_iter()
            .map(|(due, holding, amount)| {
                let paid = funds >= amount;
                if paid {
                    funds -= amount;
                }
                (due, holding, paid)
            })
            .collect();
        assert_eq!(
            outcomes,
            vec![(10, "a", true), (20, "b", true), (30, "a", false)]
        );
        assert_eq!(funds, 0);

        let source = include_str!("residence.rs");
        let one_period = source
            .split("fn settle_one_holding_period")
            .nth(1)
            .unwrap()
            .split("pub fn settle_residence_billing")
            .next()
            .unwrap();
        assert!(!one_period.contains("while holding"));
    }

    #[test]
    fn specific_relinquishment_resolves_occupants_without_deleting_history() {
        let source = include_str!("residence.rs");
        let relinquish = source
            .split("fn relinquish_holding_at")
            .nth(1)
            .unwrap()
            .split("fn supported_occupant_counts_at")
            .next()
            .unwrap();
        assert!(relinquish.contains("occupants_for_holding"));
        assert!(relinquish.contains("ResidenceHoldingStatus::Relinquished"));
        assert!(!relinquish.contains("residence_holding().id().delete"));
    }
}
