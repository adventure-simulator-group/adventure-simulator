//! Settlement residences: permanent three-tier offers, one designated home,
//! explicit occupancy, and chronological recurring billing.

use adventuresim_core::courtship::{
    HOUSING_BILLING_PERIOD_MINUTES, HousingTier as CoreHousingTier, RESIDENCE_MORALE_SPEC,
    RefreshableMorale, plan_due_period_settlement, refresh_morale, residence_leisure_bonus_milli,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::character::character;
use crate::condition::{MoraleEvent, morale_event};
use crate::strategic::settlement;
use crate::time::character_time;

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

/// Current designated residence. Replacing or relinquishing it first resolves
/// all occupancy and writes an immutable transition receipt.
#[derive(Clone, Debug)]
#[table(accessor = character_residence, public)]
pub struct CharacterResidence {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    pub tier: ResidenceTier,
    pub tenure: ResidenceTenure,
    pub active: bool,
    pub last_billed_minute: u64,
    pub next_due_minute: u64,
    pub acquired_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ResidenceChargeOutcome {
    Paid,
    Unpaid,
}

/// Immutable one-period charge receipt. Paid and unpaid have distinct IDs so
/// a dormant owner can later recover and settle the same due period.
#[derive(Clone, Debug)]
#[table(accessor = residence_charge)]
pub struct ResidenceCharge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub residence_character_id: u64,
    pub due_minute: u64,
    pub amount: u64,
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
    pub residence_character_id: u64,
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
    pub residence_character_id: u64,
    pub affected_character_id: u64,
    pub kind: ResidenceTransitionKind,
    pub minute: u64,
}

pub fn offer_id(settlement_id: &str, tier: ResidenceTier) -> String {
    format!("residence:{settlement_id}:{}", tier.id())
}

fn transition_id(
    residence_character_id: u64,
    affected_character_id: u64,
    minute: u64,
    kind: ResidenceTransitionKind,
) -> String {
    format!(
        "residence-transition:{residence_character_id}:{affected_character_id}:{minute}:{kind:?}"
    )
}

fn record_transition(
    ctx: &ReducerContext,
    residence_character_id: u64,
    affected_character_id: u64,
    minute: u64,
    kind: ResidenceTransitionKind,
) {
    let id = transition_id(residence_character_id, affected_character_id, minute, kind);
    if ctx.db.residence_transition().id().find(&id).is_none() {
        ctx.db.residence_transition().insert(ResidenceTransition {
            id,
            residence_character_id,
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

fn charge_per_period(residence: &CharacterResidence, offer: &SettlementResidenceOffer) -> u64 {
    u64::from(match residence.tenure {
        ResidenceTenure::Renter => offer.rent_per_period,
        ResidenceTenure::Owner => offer
            .owner_maintenance_per_period
            .saturating_add(offer.property_tax_per_period),
    })
}

pub fn active_primary_residence(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
) -> Option<CharacterResidence> {
    ctx.db
        .character_residence()
        .character_id()
        .find(character_id)
        .filter(|row| row.active && row.settlement_id == settlement_id)
}

fn remove_occupant_at(
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
    record_transition(
        ctx,
        occupant.residence_character_id,
        character_id,
        minute,
        ResidenceTransitionKind::OccupantRemoved,
    );
    Some(occupant)
}

pub fn admit_residence_occupant(
    ctx: &ReducerContext,
    residence_character_id: u64,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    let residence = ctx
        .db
        .character_residence()
        .character_id()
        .find(residence_character_id)
        .filter(|row| row.active)
        .ok_or("Active residence not found")?;
    if let Some(existing) = ctx
        .db
        .residence_occupant()
        .character_id()
        .find(character_id)
    {
        if existing.residence_character_id == residence.character_id {
            return Ok(());
        }
        remove_occupant_at(ctx, character_id, minute);
    }
    ctx.db.residence_occupant().insert(ResidenceOccupant {
        character_id,
        residence_character_id,
        admitted_minute: minute,
    });
    record_transition(
        ctx,
        residence_character_id,
        character_id,
        minute,
        ResidenceTransitionKind::OccupantAdmitted,
    );
    Ok(())
}

fn relinquish_current_residence_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    let Some(residence) = ctx
        .db
        .character_residence()
        .character_id()
        .find(character_id)
    else {
        return Ok(());
    };
    let occupants: Vec<_> = ctx
        .db
        .residence_occupant()
        .residence_character_id()
        .filter(character_id)
        .map(|row| row.character_id)
        .collect();
    for occupant_id in occupants {
        remove_occupant_at(ctx, occupant_id, minute);
    }
    record_transition(
        ctx,
        character_id,
        character_id,
        minute,
        ResidenceTransitionKind::Relinquished,
    );
    ctx.db
        .character_residence()
        .character_id()
        .delete(residence.character_id);
    Ok(())
}

/// Settle bills in chronological one-period units. Successful receipts and
/// `next_due_minute` are identical whether time arrives in one chunk or many.
pub fn settle_residence_billing(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let Some(mut residence) = ctx
        .db
        .character_residence()
        .character_id()
        .find(character_id)
    else {
        return Ok(());
    };
    if !residence.active {
        return Ok(());
    }
    let now = residence_now(ctx, character_id)?;
    let offer = offer(ctx, &residence.settlement_id, residence.tier)?;
    let each = charge_per_period(&residence, &offer);
    let available = crate::item::personal_currency_total(ctx, character_id);
    let plan = plan_due_period_settlement(residence.next_due_minute, now, available, each);
    if plan.amount_spent > 0 {
        crate::item::consume_personal_currency(ctx, character_id, plan.amount_spent)?;
    }
    for period in 0..plan.periods_paid {
        let due_minute = residence
            .next_due_minute
            .saturating_add(period.saturating_mul(RESIDENCE_BILLING_PERIOD_MINUTES));
        let id = format!("residence-charge:{character_id}:{due_minute}:paid");
        if ctx.db.residence_charge().id().find(&id).is_none() {
            ctx.db.residence_charge().insert(ResidenceCharge {
                id,
                residence_character_id: character_id,
                due_minute,
                amount: each,
                outcome: ResidenceChargeOutcome::Paid,
                recorded_minute: due_minute,
            });
        }
        residence.last_billed_minute = due_minute;
    }
    residence.next_due_minute = plan.next_due_minute;
    if let Some(unpaid_due) = plan.first_unpaid_due_minute {
        let id = format!("residence-charge:{character_id}:{unpaid_due}:unpaid");
        if ctx.db.residence_charge().id().find(&id).is_none() {
            ctx.db.residence_charge().insert(ResidenceCharge {
                id,
                residence_character_id: character_id,
                due_minute: unpaid_due,
                amount: each,
                outcome: ResidenceChargeOutcome::Unpaid,
                recorded_minute: unpaid_due,
            });
        }
        residence.active = false;
        record_transition(
            ctx,
            character_id,
            character_id,
            unpaid_due,
            ResidenceTransitionKind::Dormant,
        );
        if residence.tenure == ResidenceTenure::Renter {
            let occupants: Vec<_> = ctx
                .db
                .residence_occupant()
                .residence_character_id()
                .filter(character_id)
                .map(|row| row.character_id)
                .collect();
            for occupant_id in occupants {
                remove_occupant_at(ctx, occupant_id, unpaid_due);
            }
        }
    }
    ctx.db
        .character_residence()
        .character_id()
        .update(residence);
    Ok(())
}

/// Residence comfort is one fixed-duration, refreshable, bounded morale
/// source. The condition helper's personality duration is intentionally not
/// used here because housing has a fixed seven-day rules duration.
pub fn apply_residence_leisure_morale(
    ctx: &ReducerContext,
    character_id: u64,
    baseline_morale: f32,
    now: u64,
) -> Result<(), String> {
    if baseline_morale <= 0.0 || !baseline_morale.is_finite() {
        return Ok(());
    }
    let Some(residence) = ctx
        .db
        .character_residence()
        .character_id()
        .find(character_id)
        .filter(|row| row.active)
    else {
        return Ok(());
    };
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.current_settlement_id.as_deref() != Some(&residence.settlement_id) {
        return Ok(());
    }
    let offer = offer(ctx, &residence.settlement_id, residence.tier)?;
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
    let refreshed = refresh_morale(
        existing
            .as_ref()
            .map_or(RefreshableMorale::default(), |event| RefreshableMorale {
                milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
                expires_at_minute: event.expires_at_minute,
            }),
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

fn acquire_residence(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    tier: ResidenceTier,
    tenure: ResidenceTenure,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
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
    relinquish_current_residence_at(ctx, character_id, now)?;
    ctx.db.character_residence().insert(CharacterResidence {
        character_id,
        settlement_id: settlement_id.to_owned(),
        tier,
        tenure,
        active: true,
        last_billed_minute: now,
        next_due_minute: now.saturating_add(RESIDENCE_BILLING_PERIOD_MINUTES),
        acquired_minute: now,
    });
    record_transition(
        ctx,
        character_id,
        character_id,
        now,
        ResidenceTransitionKind::Acquired,
    );
    admit_residence_occupant(ctx, character_id, character_id, now)?;
    Ok(())
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
pub fn relinquish_residence(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let now = residence_now(ctx, character_id)?;
    relinquish_current_residence_at(ctx, character_id, now)
}

#[reducer]
pub fn designate_residence(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let now = residence_now(ctx, character_id)?;
    let mut residence = ctx
        .db
        .character_residence()
        .character_id()
        .find(character_id)
        .ok_or("Residence not found")?;
    if !residence.active {
        return Err("A dormant residence must be recovered before designation".into());
    }
    residence.active = true;
    ctx.db
        .character_residence()
        .character_id()
        .update(residence);
    record_transition(
        ctx,
        character_id,
        character_id,
        now,
        ResidenceTransitionKind::Designated,
    );
    Ok(())
}

#[reducer]
pub fn recover_owned_residence(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let now = residence_now(ctx, character_id)?;
    let mut residence = ctx
        .db
        .character_residence()
        .character_id()
        .find(character_id)
        .ok_or("Residence not found")?;
    if residence.tenure != ResidenceTenure::Owner {
        return Err("Only an owned residence can be recovered".into());
    }
    if residence.active {
        return Ok(());
    }
    let offer = offer(ctx, &residence.settlement_id, residence.tier)?;
    let each = charge_per_period(&residence, &offer);
    if crate::item::personal_currency_total(ctx, character_id) < each {
        return Err("Not enough coin to recover the residence".into());
    }
    residence.active = true;
    ctx.db
        .character_residence()
        .character_id()
        .update(residence);
    settle_residence_billing(ctx, character_id)?;
    if ctx
        .db
        .character_residence()
        .character_id()
        .find(character_id)
        .is_some_and(|row| row.active)
    {
        admit_residence_occupant(ctx, character_id, character_id, now)?;
        record_transition(
            ctx,
            character_id,
            character_id,
            now,
            ResidenceTransitionKind::Recovered,
        );
    }
    Ok(())
}

#[reducer]
pub fn admit_household_occupant(
    ctx: &ReducerContext,
    residence_character_id: u64,
    occupant_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, residence_character_id)?;
    let now = residence_now(ctx, residence_character_id)?;
    admit_residence_occupant(ctx, residence_character_id, occupant_id, now)
}

#[reducer]
pub fn remove_household_occupant(
    ctx: &ReducerContext,
    residence_character_id: u64,
    occupant_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, residence_character_id)?;
    let existing = ctx
        .db
        .residence_occupant()
        .character_id()
        .find(occupant_id)
        .filter(|row| row.residence_character_id == residence_character_id)
        .ok_or("Character does not occupy this residence")?;
    let now = residence_now(ctx, residence_character_id)?;
    remove_occupant_at(ctx, existing.character_id, now);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_are_stable_and_three_tiered() {
        assert_eq!(ResidenceTier::ALL.len(), 3);
        assert_ne!(
            offer_id("lubeck", ResidenceTier::Cheap),
            offer_id("lubeck", ResidenceTier::Fancy)
        );
    }

    #[test]
    fn billing_contract_uses_next_due_and_one_period_receipts() {
        let source = include_str!("residence.rs");
        assert!(source.contains("plan_due_period_settlement"));
        assert!(source.contains("pub next_due_minute: u64"));
        assert!(source.contains("pub struct ResidenceCharge"));
        assert!(source.contains("for period in 0..plan.periods_paid"));
        assert!(!source.contains("charge_for_periods"));
        let billing = source
            .split("pub fn settle_residence_billing")
            .nth(1)
            .unwrap()
            .split("pub fn apply_residence_leisure_morale")
            .next()
            .unwrap();
        assert!(billing.contains("recorded_minute: due_minute"));
        assert!(billing.contains("recorded_minute: unpaid_due"));
        assert!(billing.contains("ResidenceTransitionKind::Dormant"));
        assert!(billing.contains("remove_occupant_at(ctx, occupant_id, unpaid_due)"));
    }

    #[test]
    fn replacement_resolves_occupancy_before_inserting_new_home() {
        let source = include_str!("residence.rs");
        let acquisition = source
            .split("fn acquire_residence")
            .nth(1)
            .unwrap()
            .split("#[reducer]")
            .next()
            .unwrap();
        assert!(
            acquisition.find("relinquish_current_residence_at").unwrap()
                < acquisition.find("CharacterResidence {").unwrap()
        );
    }
}
