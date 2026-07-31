//! Settlement residences: permanent three-tier offers, a single primary home,
//! and lazy, deterministic recurring billing.

use adventuresim_core::courtship::HousingTier as CoreHousingTier;
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::strategic::settlement;
use crate::time::character_time;

pub const RESIDENCE_BILLING_PERIOD_MINUTES: u64 = 30 * MINUTES_PER_DAY;

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
        match self { Self::Cheap => "cheap", Self::Moderate => "moderate", Self::Fancy => "fancy" }
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

/// A character may designate exactly one primary residence. `active` makes a
/// missed payment reversible without deleting an owned home or its history.
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
}

pub fn offer_id(settlement_id: &str, tier: ResidenceTier) -> String {
    format!("residence:{settlement_id}:{}", tier.id())
}

/// Idempotently creates the universal three offers for a settlement.  Offers
/// are intentionally nonexclusive; character ownership is represented by
/// `CharacterResidence`, not a depleted shared stock row.
pub fn ensure_settlement_residence_offers(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    if ctx.db.settlement().id().find(settlement_id.to_owned()).is_none() {
        return Err("Settlement not found".into());
    }
    for tier in ResidenceTier::ALL {
        let id = offer_id(settlement_id, tier);
        if ctx.db.settlement_residence_offer().id().find(&id).is_some() { continue; }
        let economy = tier.core().economy();
        ctx.db.settlement_residence_offer().insert(SettlementResidenceOffer {
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
    ctx.db.character_time().character_id().find(character_id)
        .map(|row| row.minutes)
        .ok_or_else(|| "Character time record not found".to_string())
}

fn offer(ctx: &ReducerContext, settlement_id: &str, tier: ResidenceTier) -> Result<SettlementResidenceOffer, String> {
    ctx.db.settlement_residence_offer().id().find(&offer_id(settlement_id, tier))
        .ok_or_else(|| "Residence offer not found".to_string())
}

fn charge_for_periods(residence: &CharacterResidence, offer: &SettlementResidenceOffer, periods: u64) -> u64 {
    let each = match residence.tenure {
        ResidenceTenure::Renter => offer.rent_per_period,
        ResidenceTenure::Owner => offer.owner_maintenance_per_period.saturating_add(offer.property_tax_per_period),
    };
    u64::from(each).saturating_mul(periods)
}

/// Settle every complete billing period exactly once.  The timestamp is only
/// moved after payment, making a failed rent bill lapse tenancy while an owned
/// home stays dormant until the owner can pay.
pub fn settle_residence_billing(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let Some(mut residence) = ctx.db.character_residence().character_id().find(character_id) else { return Ok(()); };
    let now = residence_now(ctx, character_id)?;
    let periods = now.saturating_sub(residence.last_billed_minute) / RESIDENCE_BILLING_PERIOD_MINUTES;
    if periods == 0 || !residence.active { return Ok(()); }
    let offer = offer(ctx, &residence.settlement_id, residence.tier)?;
    let amount = charge_for_periods(&residence, &offer, periods);
    if crate::item::consume_personal_currency(ctx, character_id, amount).is_err() {
        residence.active = false;
        ctx.db.character_residence().character_id().update(residence);
        return Ok(());
    }
    residence.last_billed_minute = residence.last_billed_minute.saturating_add(periods.saturating_mul(RESIDENCE_BILLING_PERIOD_MINUTES));
    ctx.db.character_residence().character_id().update(residence);
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
    let initial_charge = match tenure { ResidenceTenure::Renter => u64::from(offer.rent_per_period), ResidenceTenure::Owner => u64::from(offer.purchase_price) };
    crate::item::consume_personal_currency(ctx, character_id, initial_charge)?;
    let now = residence_now(ctx, character_id)?;
    let row = CharacterResidence { character_id, settlement_id: settlement_id.to_owned(), tier, tenure, active: true, last_billed_minute: now };
    if ctx.db.character_residence().character_id().find(character_id).is_some() {
        ctx.db.character_residence().character_id().update(row);
    } else { ctx.db.character_residence().insert(row); }
    Ok(())
}

#[reducer]
pub fn rent_residence(ctx: &ReducerContext, character_id: u64, settlement_id: String, tier: ResidenceTier) -> Result<(), String> {
    acquire_residence(ctx, character_id, &settlement_id, tier, ResidenceTenure::Renter)
}

#[reducer]
pub fn buy_residence(ctx: &ReducerContext, character_id: u64, settlement_id: String, tier: ResidenceTier) -> Result<(), String> {
    acquire_residence(ctx, character_id, &settlement_id, tier, ResidenceTenure::Owner)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offers_are_stable_and_three_tiered() {
        assert_eq!(ResidenceTier::ALL.len(), 3);
        assert_ne!(offer_id("lubeck", ResidenceTier::Cheap), offer_id("lubeck", ResidenceTier::Fancy));
    }
}
