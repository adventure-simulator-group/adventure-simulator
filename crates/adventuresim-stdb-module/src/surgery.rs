//! Durable strategic wounds and the manual, limb-at-a-time treatment reducers.
//!
//! This intentionally records injury categories rather than internal anatomy.
//! Tactical positions and ticks remain transient; only committed hit outcomes
//! cross into these rows.

use adventuresim_core::prelude::*;
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
#[cfg(test)]
use adventuresim_core::surgery::untreated_cut_progress;
pub use adventuresim_core::surgery::{
    UNTREATED_CUT_BLOOD_LOSS_PER_DAY, UNTREATED_CUT_DETERIORATION_PER_DAY,
};
use adventuresim_core::surgery::{simulate_blood_interval, standing_infection_multiplier};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::character::character;
use crate::{
    CharacterLimbs, character_attributes, character_condition, character_equip, character_limbs,
    character_skills, character_stats, character_time, infection_episode, inventory_item,
};

pub const BRUISE_HEALING_PER_DAY: f32 = 0.035;
pub const FRACTURE_HEALING_PER_DAY: f32 = 0.0125;
pub const STITCH_HEALING_BONUS_PER_LEVEL: f32 = 0.006;
pub const RETAINED_PROJECTILE_HEALING_MULTIPLIER: f32 = 0.60;
pub const FRACTURE_SINGLE_HIT_THRESHOLD: f32 = 0.18;
pub const STANDING_INFECTION_CHECK_EXPOSURE: f32 = 0.05;
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum LimbRegion {
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
}

impl LimbRegion {
    pub const ALL: [Self; 7] = [
        Self::LeftArm,
        Self::RightArm,
        Self::LeftLeg,
        Self::RightLeg,
        Self::Chest,
        Self::Stomach,
        Self::Head,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "left-arm" => Self::LeftArm,
            "right-arm" => Self::RightArm,
            "left-leg" => Self::LeftLeg,
            "right-leg" => Self::RightLeg,
            "chest" => Self::Chest,
            "stomach" => Self::Stomach,
            "head" => Self::Head,
            _ => return None,
        })
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::LeftArm => "left-arm",
            Self::RightArm => "right-arm",
            Self::LeftLeg => "left-leg",
            Self::RightLeg => "right-leg",
            Self::Chest => "chest",
            Self::Stomach => "stomach",
            Self::Head => "head",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ProjectileKind {
    Arrowhead,
    Ball,
}

#[derive(Clone, Debug)]
#[table(accessor = limb_injury, public)]
pub struct LimbInjury {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub limb: LimbRegion,
    pub cut_damage: f32,
    pub bruise_damage: f32,
    /// Fracture severity is a condition within blunt trauma, not additional
    /// health damage. It therefore never contributes again to the projection.
    pub fracture_damage: f32,
    pub bandaged: bool,
    pub stitched: bool,
    pub stitch_quality: f32,
    /// Owner to whom the reusable splint is returned after removal/healing.
    pub splint_owner_id: Option<u64>,
    /// The exact inventory row moved into the applied/equipped state.
    pub splint_inventory_item_id: Option<u64>,
    /// Continuous deterministic wound exposure carried across time chunks.
    pub infection_exposure: f32,
    pub infection_checks: u32,
    pub infection_origin_minute: Option<u64>,
}

#[derive(Clone, Debug)]
#[table(accessor = retained_projectile, public)]
pub struct RetainedProjectile {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub limb: LimbRegion,
    pub kind: ProjectileKind,
    /// Extraction is deliberately uncapped. Future ammunition definitions can
    /// multiply this value (for example, a barbed-arrow modifier).
    pub extraction_dc: f32,
    pub source_damage: f32,
}

fn injury_id(character_id: u64, limb: LimbRegion) -> String {
    format!("{character_id}:{}", limb.slug())
}

fn blank_injury(character_id: u64, limb: LimbRegion) -> LimbInjury {
    LimbInjury {
        id: injury_id(character_id, limb),
        character_id,
        limb,
        cut_damage: 0.0,
        bruise_damage: 0.0,
        fracture_damage: 0.0,
        bandaged: false,
        stitched: false,
        stitch_quality: 0.0,
        splint_owner_id: None,
        splint_inventory_item_id: None,
        infection_exposure: 0.0,
        infection_checks: 0,
        infection_origin_minute: None,
    }
}

fn projected_damage(injury: &LimbInjury) -> f32 {
    (injury.cut_damage + injury.bruise_damage.max(injury.fracture_damage)).clamp(0.0, 1.0)
}

pub fn injury_for(ctx: &ReducerContext, character_id: u64, limb: LimbRegion) -> LimbInjury {
    let key = injury_id(character_id, limb);
    ctx.db
        .limb_injury()
        .id()
        .find(key)
        .filter(|row| row.character_id == character_id && row.limb == limb)
        .unwrap_or_else(|| blank_injury(character_id, limb))
}

fn store_injury(ctx: &ReducerContext, injury: LimbInjury) {
    if ctx.db.limb_injury().id().find(injury.id.clone()).is_some() {
        ctx.db.limb_injury().id().update(injury);
    } else {
        ctx.db.limb_injury().insert(injury);
    }
}

fn health_mut(limbs: &mut CharacterLimbs, limb: LimbRegion) -> &mut f32 {
    match limb {
        LimbRegion::LeftArm => &mut limbs.left_arm_health,
        LimbRegion::RightArm => &mut limbs.right_arm_health,
        LimbRegion::LeftLeg => &mut limbs.left_leg_health,
        LimbRegion::RightLeg => &mut limbs.right_leg_health,
        LimbRegion::Chest => &mut limbs.chest_health,
        LimbRegion::Stomach => &mut limbs.stomach_health,
        LimbRegion::Head => &mut limbs.head_health,
    }
}

fn limb_health(limbs: &CharacterLimbs, limb: LimbRegion) -> f32 {
    match limb {
        LimbRegion::LeftArm => limbs.left_arm_health,
        LimbRegion::RightArm => limbs.right_arm_health,
        LimbRegion::LeftLeg => limbs.left_leg_health,
        LimbRegion::RightLeg => limbs.right_leg_health,
        LimbRegion::Chest => limbs.chest_health,
        LimbRegion::Stomach => limbs.stomach_health,
        LimbRegion::Head => limbs.head_health,
    }
}

/// Idempotently adopts legacy limb-health deficits before injury rows become
/// authoritative. Any unclassified deficit is conservatively recorded as
/// bruising; an incoming hit can therefore never heal old damage.
pub fn backfill_character_injuries(ctx: &ReducerContext, character_id: u64) {
    let Some(limbs) = ctx.db.character_limbs().character_id().find(character_id) else {
        return;
    };
    for limb in LimbRegion::ALL {
        let mut injury = injury_for(ctx, character_id, limb);
        let legacy_deficit = (1.0 - limb_health(&limbs, limb)).clamp(0.0, 1.0);
        let classified = projected_damage(&injury);
        if legacy_deficit > classified {
            injury.bruise_damage += legacy_deficit - classified;
        }
        if injury.splint_owner_id.is_some() && injury.splint_inventory_item_id.is_none() {
            injury.splint_inventory_item_id =
                crate::add_inventory_item(ctx, character_id, "splint", 1);
        }
        store_injury(ctx, injury);
    }
}

#[reducer]
pub fn upgrade_manual_surgery(ctx: &ReducerContext) {
    crate::item::upsert_surgery_items(ctx);
    let character_ids: Vec<_> = ctx
        .db
        .character_limbs()
        .iter()
        .map(|row| row.character_id)
        .collect();
    for character_id in character_ids {
        backfill_character_injuries(ctx, character_id);
    }
}

fn refresh_limb_projection(ctx: &ReducerContext, character_id: u64, limb: LimbRegion) {
    let Some(mut limbs) = ctx.db.character_limbs().character_id().find(character_id) else {
        return;
    };
    let injury = injury_for(ctx, character_id, limb);
    *health_mut(&mut limbs, limb) = (1.0 - projected_damage(&injury)).clamp(0.0, 1.0);
    ctx.db.character_limbs().character_id().update(limbs);
}

fn refresh_all_limb_projections(ctx: &ReducerContext, character_id: u64) {
    let Some(mut limbs) = ctx.db.character_limbs().character_id().find(character_id) else {
        return;
    };
    for limb in LimbRegion::ALL {
        let injury = injury_for(ctx, character_id, limb);
        *health_mut(&mut limbs, limb) = (1.0 - projected_damage(&injury)).clamp(0.0, 1.0);
    }
    ctx.db.character_limbs().character_id().update(limbs);
}

/// Commit one strategic hit. The fracture threshold is based on this hit, not
/// accumulated bruising. Projectile depth/DC is correlated with hit damage but
/// retains a seeded random component and has no artificial upper ceiling.
pub fn commit_hit_injury(
    ctx: &ReducerContext,
    character_id: u64,
    limb: LimbRegion,
    cut_damage: f32,
    blunt_damage: f32,
    projectile: Option<ProjectileKind>,
) -> Result<(), String> {
    backfill_character_injuries(ctx, character_id);
    let mut injury = injury_for(ctx, character_id, limb);
    injury.cut_damage += cut_damage.max(0.0);
    injury.bruise_damage += blunt_damage.max(0.0);
    injury.fracture_damage =
        (injury.fracture_damage + fracture_from_single_hit(blunt_damage)).min(injury.bruise_damage);
    if cut_damage > 0.0 {
        injury.infection_origin_minute.get_or_insert_with(|| {
            ctx.db
                .character_time()
                .character_id()
                .find(character_id)
                .map_or(0, |row| row.minutes)
        });
        injury.bandaged = false;
        injury.stitched = false;
        injury.stitch_quality = 0.0;
        crate::filth::deposit_now(
            ctx,
            character_id,
            crate::filth::FilthSubstance::Blood,
            Some(character_id),
            (cut_damage * 50.0).ceil().clamp(1.0, 20.0) as u16,
        )?;
    }
    store_injury(ctx, injury);
    if let Some(kind) = projectile.filter(|_| cut_damage + blunt_damage > 0.0) {
        let random_depth = (ctx.random::<u64>() % 151) as f32 / 100.0;
        let total_damage = cut_damage.max(0.0) + blunt_damage.max(0.0);
        ctx.db.retained_projectile().insert(RetainedProjectile {
            id: 0,
            character_id,
            limb,
            kind,
            extraction_dc: adventuresim_core::surgery::projectile_extraction_dc(
                total_damage,
                random_depth,
            ),
            source_damage: total_damage,
        });
    }
    refresh_limb_projection(ctx, character_id, limb);
    Ok(())
}

pub fn fracture_from_single_hit(blunt_damage: f32) -> f32 {
    (blunt_damage.max(0.0) - FRACTURE_SINGLE_HIT_THRESHOLD).max(0.0) * 0.65
}

pub fn projectile_extraction_dc(hit_damage: f32, random_depth: f32) -> f32 {
    adventuresim_core::surgery::projectile_extraction_dc(hit_damage, random_depth)
}

fn has_projectile(ctx: &ReducerContext, character_id: u64, limb: LimbRegion) -> bool {
    ctx.db
        .retained_projectile()
        .character_id()
        .filter(character_id)
        .any(|projectile| projectile.limb == limb)
}

#[derive(Clone, Copy, Debug)]
pub struct InjurySettlement {
    pub elapsed: u64,
    pub alive: bool,
}

pub fn preview_elapsed_for_injuries(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_recovery: bool,
) -> Result<u64, String> {
    preview_elapsed_for_injuries_with_rest_minutes(
        ctx,
        character_id,
        requested,
        if allow_recovery { requested } else { 0 },
    )
}

pub fn preview_elapsed_for_injuries_with_rest_minutes(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    recovery_minutes: u64,
) -> Result<u64, String> {
    crate::condition::apply_blood_loss(ctx, character_id, 0.0)?;
    let blood = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .map_or(1.0, |row| {
            if row.maximum_blood_ml > 0.0 {
                row.current_blood_ml / row.maximum_blood_ml
            } else {
                1.0
            }
        });
    let cuts = LimbRegion::ALL
        .into_iter()
        .map(|limb| {
            let injury = injury_for(ctx, character_id, limb);
            if injury.bandaged {
                0.0
            } else {
                injury.cut_damage
            }
        })
        .collect::<Vec<_>>();
    Ok(simulate_blood_interval(
        blood,
        &cuts,
        requested,
        crate::condition::BLOOD_RECOVERY_FRACTION_PER_DAY * recovery_minutes.min(requested) as f32
            / requested.max(1) as f32,
    )
    .elapsed)
}

/// Advance authoritative wounds for one personal-clock interval. This is the
/// sole writer of injury-backed limb health and the sole rest-time blood
/// recovery path, so projections cannot drift from durable wound state.
pub fn settle_injuries(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed: u64,
    allow_healing: bool,
) -> Result<InjurySettlement, String> {
    settle_injuries_with_rest_minutes(
        ctx,
        character_id,
        elapsed,
        if allow_healing { elapsed } else { 0 },
    )
}

pub fn settle_injuries_with_rest_minutes(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed: u64,
    healing_minutes: u64,
) -> Result<InjurySettlement, String> {
    if elapsed == 0 {
        return Ok(InjurySettlement {
            elapsed: 0,
            alive: true,
        });
    }
    backfill_character_injuries(ctx, character_id);
    crate::condition::apply_blood_loss(ctx, character_id, 0.0)?;
    let starting_blood = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .map_or(1.0, |row| {
            if row.maximum_blood_ml > 0.0 {
                row.current_blood_ml / row.maximum_blood_ml
            } else {
                1.0
            }
        });
    let mut injuries = LimbRegion::ALL.map(|limb| injury_for(ctx, character_id, limb));
    let open_cuts = injuries
        .iter()
        .map(|injury| {
            if injury.bandaged {
                0.0
            } else {
                injury.cut_damage
            }
        })
        .collect::<Vec<_>>();
    let interval = simulate_blood_interval(
        starting_blood,
        &open_cuts,
        elapsed,
        crate::condition::BLOOD_RECOVERY_FRACTION_PER_DAY * healing_minutes.min(elapsed) as f32
            / elapsed.max(1) as f32,
    );
    let elapsed_days = interval.elapsed as f32 / MINUTES_PER_DAY as f32;
    let healing_days = healing_minutes.min(interval.elapsed) as f32 / MINUTES_PER_DAY as f32;
    let physiology = if healing_days > 0.0 {
        crate::time::party_physiology_check(ctx, character_id)?
    } else {
        0.0
    };
    let natural = if healing_days > 0.0 {
        crate::time::health_recovered_per_day(physiology)
    } else {
        0.0
    };
    for (index, limb) in LimbRegion::ALL.into_iter().enumerate() {
        let injury = &mut injuries[index];
        let starting_cut = injury.cut_damage;
        let mut exposure = interval.cut_days[index];
        if !injury.bandaged {
            injury.cut_damage = interval.open_cuts[index];
        }
        let projectile_term = if has_projectile(ctx, character_id, limb) {
            RETAINED_PROJECTILE_HEALING_MULTIPLIER
        } else {
            1.0
        };
        if healing_days > 0.0 {
            let stitch_bonus = if injury.stitched {
                injury.stitch_quality.max(0.0) * STITCH_HEALING_BONUS_PER_LEVEL
            } else {
                0.0
            };
            if injury.bandaged {
                injury.cut_damage = (injury.cut_damage
                    - (natural + 0.01 + stitch_bonus) * projectile_term * healing_days)
                    .max(0.0);
                exposure += (starting_cut + injury.cut_damage) * 0.5 * elapsed_days;
            }
            injury.bruise_damage = (injury.bruise_damage
                - (natural + BRUISE_HEALING_PER_DAY) * projectile_term * healing_days)
                .max(0.0);
            if injury.splint_inventory_item_id.is_some() {
                injury.fracture_damage = (injury.fracture_damage
                    - (natural + FRACTURE_HEALING_PER_DAY) * projectile_term * healing_days)
                    .max(0.0);
                if injury.fracture_damage == 0.0 {
                    return_splint(ctx, injury)?;
                }
            }
        } else if injury.bandaged {
            exposure += injury.cut_damage * elapsed_days;
        }
        if injury.cut_damage > 0.0 || starting_cut > 0.0 {
            let protection = standing_infection_multiplier(
                injury.bandaged,
                injury.stitched,
                injury.stitch_quality,
            );
            let dirt = adventuresim_core::filth::dirt_wound_multiplier(crate::filth::dirt_total(
                ctx,
                character_id,
            ));
            accrue_standing_infection(ctx, injury, exposure * protection * dirt)?;
        }
        store_injury(ctx, injury.clone());
    }
    refresh_all_limb_projections(ctx, character_id);
    if interval.terminal {
        if let Some(mut time) = ctx.db.character_time().character_id().find(character_id) {
            time.minutes = time.minutes.saturating_add(interval.elapsed);
            ctx.db.character_time().character_id().update(time);
        }
    }
    crate::condition::set_blood_fraction(ctx, character_id, interval.blood_fraction)?;
    let alive = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|row| row.alive);
    Ok(InjurySettlement {
        elapsed: interval.elapsed,
        alive,
    })
}

pub fn convalescence_minutes(
    ctx: &ReducerContext,
    character_id: u64,
    physiology_check: f32,
) -> u64 {
    let natural = crate::time::health_recovered_per_day(physiology_check);
    let mut days = 0.0_f32;
    for limb in LimbRegion::ALL {
        let injury = injury_for(ctx, character_id, limb);
        let projectile = if has_projectile(ctx, character_id, limb) {
            RETAINED_PROJECTILE_HEALING_MULTIPLIER
        } else {
            1.0
        };
        if injury.bandaged && injury.cut_damage > 0.0 {
            let stitch = if injury.stitched {
                injury.stitch_quality.max(0.0) * STITCH_HEALING_BONUS_PER_LEVEL
            } else {
                0.0
            };
            days = days.max(injury.cut_damage / ((natural + 0.01 + stitch) * projectile));
        }
        if injury.bruise_damage > 0.0 {
            days =
                days.max(injury.bruise_damage / ((natural + BRUISE_HEALING_PER_DAY) * projectile));
        }
        if injury.splint_inventory_item_id.is_some() && injury.fracture_damage > 0.0 {
            days = days
                .max(injury.fracture_damage / ((natural + FRACTURE_HEALING_PER_DAY) * projectile));
        }
    }
    (days * MINUTES_PER_DAY as f32).ceil() as u64
}

fn item_quantity(ctx: &ReducerContext, character_id: u64, item_id: &str) -> u32 {
    ctx.db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|item| item.item_id == item_id)
        .map(|item| item.quantity)
        .sum()
}

fn splint_is_equipped(ctx: &ReducerContext, inventory_item_id: u64) -> bool {
    ctx.db
        .limb_injury()
        .iter()
        .any(|injury| injury.splint_inventory_item_id == Some(inventory_item_id))
}

fn available_splints(ctx: &ReducerContext, character_id: u64) -> u32 {
    ctx.db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|item| item.item_id == "splint" && !splint_is_equipped(ctx, item.id))
        .map(|item| item.quantity)
        .sum()
}

fn equip_splint(ctx: &ReducerContext, owner_id: u64, patient_id: u64) -> Result<u64, String> {
    let mut stack = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(owner_id)
        .filter(|item| {
            item.item_id == "splint" && item.quantity > 0 && !splint_is_equipped(ctx, item.id)
        })
        .min_by_key(|item| item.id)
        .ok_or("No splint available")?;
    if stack.quantity == 1 {
        stack.character_id = patient_id;
        let id = stack.id;
        ctx.db.inventory_item().id().update(stack);
        Ok(id)
    } else {
        stack.quantity -= 1;
        ctx.db.inventory_item().id().update(stack);
        Ok(ctx
            .db
            .inventory_item()
            .insert(crate::InventoryItem {
                id: 0,
                character_id: patient_id,
                item_id: "splint".into(),
                quantity: 1,
            })
            .id)
    }
}

fn return_splint(ctx: &ReducerContext, injury: &mut LimbInjury) -> Result<(), String> {
    let owner = injury
        .splint_owner_id
        .take()
        .ok_or("Applied splint has no owner")?;
    let inventory_id = injury
        .splint_inventory_item_id
        .take()
        .ok_or("Applied splint has no item")?;
    let mut item = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_id)
        .ok_or("Applied splint item is missing")?;
    if item.item_id != "splint" || item.quantity != 1 {
        return Err("Applied splint provenance is invalid".into());
    }
    item.character_id = owner;
    ctx.db.inventory_item().id().update(item);
    Ok(())
}

fn accrue_standing_infection(
    ctx: &ReducerContext,
    injury: &mut LimbInjury,
    exposure: f32,
) -> Result<(), String> {
    injury.infection_exposure += exposure.max(0.0);
    let completed = (injury.infection_exposure / STANDING_INFECTION_CHECK_EXPOSURE).floor() as u32;
    while injury.infection_checks < completed {
        injury.infection_checks += 1;
        crate::disease::record_standing_cut_exposure(
            ctx,
            injury.character_id,
            STANDING_INFECTION_CHECK_EXPOSURE,
            if injury.stitched {
                injury.stitch_quality
            } else {
                0.0
            },
            &format!("{}:{}", injury.id, injury.infection_checks),
            injury.infection_origin_minute.unwrap_or(0) + u64::from(injury.infection_checks),
        )?;
    }
    Ok(())
}

fn consume_one(ctx: &ReducerContext, character_id: u64, item_id: &str) -> Result<(), String> {
    let mut stack = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|item| item.item_id == item_id && item.quantity > 0)
        .min_by_key(|item| item.id)
        .ok_or_else(|| format!("No {item_id} available"))?;
    stack.quantity -= 1;
    if stack.quantity == 0 {
        ctx.db.inventory_item().id().delete(stack.id);
    } else {
        ctx.db.inventory_item().id().update(stack);
    }
    Ok(())
}

fn procedure_check(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    procedure: &str,
) -> Result<f32, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(actor_id)
        .ok_or("Character attributes not found")?;
    let attributes = crate::disease::effective_attributes(ctx, actor_id, attributes)?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(actor_id)
        .ok_or("Character skills not found")?;
    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(actor_id)
        .ok_or("Character equipment not found")?;
    let body = ctx
        .db
        .character_limbs()
        .character_id()
        .find(actor_id)
        .ok_or("Character limbs not found")?;
    let essentials = ctx
        .db
        .character_stats()
        .character_id()
        .find(actor_id)
        .ok_or("Character stats not found")?;
    let equipment = crate::capability::StrategicEquipment::load(ctx, actor_id, &equip);
    let check = |skill| {
        skills.skill_check_by_parts(
            skill,
            &attributes,
            &body,
            &essentials,
            &equipment,
            LimbWeights::both_arms(),
        )
    };
    Ok(adventuresim_core::surgery::procedure_skill(
        procedure,
        check(Skill::Surgery),
        actor_id == patient_id,
    ))
}

fn infection_control_check(ctx: &ReducerContext, actor_id: u64, surgical_skill: f32) -> f32 {
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .map_or(0, |time| time.minutes);
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(actor_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let diseased_penalty = if ctx
        .db
        .infection_episode()
        .character_id()
        .filter(actor_id)
        .any(|row| {
            crate::disease::parse_id(&row.disease_id).is_ok_and(|disease_id| {
                !matches!(
                    adventuresim_core::disease::evaluate(
                        adventuresim_core::disease::InfectionEpisode {
                            id: row.id,
                            character_id: row.character_id,
                            disease_id,
                            contracted_at: row.contracted_at,
                            ruleset_version: row.ruleset_version,
                            phenotype_key_version: row.phenotype_key_version,
                        },
                        now,
                        immunity,
                    )
                    .stage,
                    adventuresim_core::disease::DiseaseStage::Resolved
                )
            })
        }) {
        1.0
    } else {
        0.0
    };
    (surgical_skill - diseased_penalty).max(0.0)
}

fn require_together(ctx: &ReducerContext, actor_id: u64, patient_id: u64) -> Result<(), String> {
    let actor = crate::require_living_character(ctx, actor_id)?;
    let patient = crate::require_living_character(ctx, patient_id)?;
    let actor_site = crate::investigation::character_case_site_id(ctx, actor.id);
    let patient_site = crate::investigation::character_case_site_id(ctx, patient.id);
    let same_place = actor.current_settlement_id.is_some()
        && actor.current_settlement_id == patient.current_settlement_id
        || actor_site.is_some() && actor_site == patient_site;
    if !same_place {
        return Err("Surgeon and patient must be together".into());
    }
    if actor_id != patient_id && (actor.party_id.is_none() || actor.party_id != patient.party_id) {
        return Err("A surgeon may treat only themselves or a member of their party".into());
    }
    Ok(())
}

fn align_and_advance(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    duration: u64,
) -> Result<bool, String> {
    let actor_time = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .ok_or("Surgeon has no time record")?
        .minutes;
    let patient_time = ctx
        .db
        .character_time()
        .character_id()
        .find(patient_id)
        .ok_or("Patient has no time record")?
        .minutes;
    let aligned = actor_time.max(patient_time);
    for (id, time) in [(actor_id, actor_time), (patient_id, patient_time)] {
        if id == patient_id && actor_id == patient_id {
            continue;
        }
        let catchup = aligned.saturating_sub(time);
        if catchup > 0 && !crate::time::advance_character_wait_time(ctx, id, catchup)? {
            return Ok(false);
        }
    }
    require_together(ctx, actor_id, patient_id)?;
    let participants = if actor_id == patient_id {
        vec![actor_id]
    } else {
        vec![actor_id, patient_id]
    };
    let safe_duration = participants.iter().try_fold(duration, |limit, id| {
        let disease = crate::disease::preview_elapsed_for_disease(ctx, *id, limit, true)?;
        let injury = preview_elapsed_for_injuries(ctx, *id, limit, true)?;
        Ok::<u64, String>(limit.min(disease).min(injury))
    })?;
    let mut completed = safe_duration == duration;
    for id in participants {
        completed &= crate::time::advance_character_wait_time(ctx, id, safe_duration)?;
    }
    Ok(completed)
}

fn duration_minutes(procedure: &str, skill: f32, dc: f32) -> u64 {
    adventuresim_core::surgery::procedure_duration_minutes(procedure, skill, dc)
}

/// Manual surgery. The UI supplies a procedure and optional projectile id;
/// reducers remain authoritative over requirements, clocks, consumption, and
/// hidden infection rolls.
#[reducer]
pub fn treat_limb(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    limb_slug: String,
    procedure: String,
    projectile_id: Option<u64>,
    use_soap: bool,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    crate::item::upsert_surgery_items(ctx);
    require_together(ctx, actor_id, patient_id)?;
    let limb = LimbRegion::parse(&limb_slug).ok_or("Unknown limb")?;
    let skill = procedure_check(ctx, actor_id, patient_id, &procedure)?;
    let mut injury = injury_for(ctx, patient_id, limb);
    let projectile = projectile_id.and_then(|id| ctx.db.retained_projectile().id().find(id));
    let dc = match procedure.as_str() {
        "bandage" if injury.cut_damage > 0.0 && !injury.bandaged => 0.0,
        "stitch" if injury.cut_damage > 0.0 && !injury.stitched => 2.0,
        "splint" if injury.fracture_damage > 0.0 && injury.splint_inventory_item_id.is_none() => {
            1.0
        }
        "remove-splint" if injury.splint_inventory_item_id.is_some() => 0.0,
        "extract"
            if projectile
                .as_ref()
                .is_some_and(|p| p.character_id == patient_id && p.limb == limb) =>
        {
            projectile.as_ref().unwrap().extraction_dc
        }
        "bandage" => return Err("This limb does not need bandaging".into()),
        "stitch" => return Err("This wound cannot be stitched".into()),
        "splint" => return Err("This limb has no unsplinted fracture".into()),
        "remove-splint" => return Err("This limb is not splinted".into()),
        "extract" => return Err("Projectile not found in this limb".into()),
        _ => return Err("Unknown procedure".into()),
    };
    if skill < dc {
        return Err(format!(
            "Insufficient procedure skill: this procedure requires {dc:.1}"
        ));
    }
    if procedure == "stitch" && item_quantity(ctx, actor_id, "surgery_kit") == 0 {
        return Err("Stitching requires a surgery kit".into());
    }
    if procedure == "extract"
        && adventuresim_core::surgery::extraction_requires_surgery_kit(dc)
        && item_quantity(ctx, actor_id, "surgery_kit") == 0
    {
        return Err("Extracting a projectile above DC 1 requires a surgery kit".into());
    }
    if procedure == "bandage" && item_quantity(ctx, actor_id, "bandage") == 0 {
        return Err("Bandaging requires one bandage".into());
    }
    if procedure == "splint" && available_splints(ctx, actor_id) == 0 {
        return Err("Applying a splint requires one splint".into());
    }
    let soap_applicable = matches!(procedure.as_str(), "bandage" | "stitch" | "extract");
    let soap_available = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((actor_id, crate::filth::SOAP_ITEM_ID))
        .any(|row| {
            crate::inventory_amount::personal_amount(ctx, row.id).unwrap_or(0)
                >= crate::filth::SOAP_MILLIUNITS_PER_CLEANSING_POINT
        });
    if use_soap && (!soap_applicable || !soap_available) {
        return Err("The selected procedure cannot use an available unit of soap".into());
    }
    let duration = duration_minutes(&procedure, skill, dc);
    if !align_and_advance(ctx, actor_id, patient_id, duration)? {
        return Ok(());
    }
    require_together(ctx, actor_id, patient_id)?;
    injury = injury_for(ctx, patient_id, limb);
    let selected_alcohol = soap_applicable
        .then(|| crate::alcohol::best_disinfectant(ctx, actor_id))
        .flatten();
    if use_soap {
        let soap = ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((actor_id, crate::filth::SOAP_ITEM_ID))
            .find(|row| {
                crate::inventory_amount::personal_amount(ctx, row.id).unwrap_or(0)
                    >= crate::filth::SOAP_MILLIUNITS_PER_CLEANSING_POINT
            })
            .ok_or("Selected soap is no longer available")?;
        crate::filth::consume_personal_soap_points(ctx, soap.id, 1)?;
    }
    if let Some((inventory_id, _, _)) = selected_alcohol.as_ref() {
        crate::alcohol::consume_inventory_row(ctx, *inventory_id)?;
    }
    let clean_check = infection_control_check(ctx, actor_id, skill)
        + adventuresim_core::alcohol::surgery_control_bonus(
            use_soap,
            selected_alcohol
                .as_ref()
                .map(|(_, _, effectiveness)| *effectiveness),
        );
    match procedure.as_str() {
        "bandage" => {
            consume_one(ctx, actor_id, "bandage")?;
            injury.bandaged = true;
            // The risk is intentionally hidden. Low skill and an untreated
            // disease reduce the check passed to the existing wound-exposure model.
            crate::disease::record_committed_cut(
                ctx,
                patient_id,
                injury.cut_damage * 0.25,
                clean_check,
            )?;
        }
        "stitch" => {
            injury.stitched = true;
            injury.stitch_quality = skill;
            crate::disease::record_committed_cut(
                ctx,
                patient_id,
                injury.cut_damage * 0.12,
                clean_check,
            )?;
        }
        "splint" => {
            injury.splint_owner_id = Some(actor_id);
            injury.splint_inventory_item_id = Some(equip_splint(ctx, actor_id, patient_id)?);
        }
        "remove-splint" => {
            return_splint(ctx, &mut injury)?;
        }
        "extract" => {
            let projectile = projectile.unwrap();
            let trauma = 0.015;
            injury.cut_damage = (injury.cut_damage + trauma).min(1.0);
            injury.infection_origin_minute.get_or_insert_with(|| {
                ctx.db
                    .character_time()
                    .character_id()
                    .find(patient_id)
                    .map_or(0, |row| row.minutes)
            });
            injury.bandaged = false;
            injury.stitched = false;
            injury.stitch_quality = 0.0;
            crate::condition::apply_blood_loss(ctx, patient_id, trauma * 0.15)?;
            crate::disease::record_committed_cut(ctx, patient_id, trauma, clean_check)?;
            ctx.db.retained_projectile().id().delete(projectile.id);
        }
        _ => unreachable!(),
    }
    let exposure =
        adventuresim_core::surgery::procedure_blood_exposure(&procedure, actor_id != patient_id);
    if exposure > 0 {
        crate::filth::deposit_now(
            ctx,
            actor_id,
            crate::filth::FilthSubstance::Blood,
            Some(patient_id),
            exposure,
        )?;
    }
    store_injury(ctx, injury);
    refresh_limb_projection(ctx, patient_id, limb);
    if !ctx
        .db
        .character()
        .id()
        .find(patient_id)
        .is_some_and(|character| character.alive)
    {
        return Ok(());
    }
    crate::capability::refresh_character_capability(ctx, patient_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fracture_uses_single_hit_threshold() {
        assert_eq!(fracture_from_single_hit(0.18), 0.0);
        assert!((fracture_from_single_hit(0.38) - 0.13).abs() < 0.0001);
    }

    #[test]
    fn projectile_dc_is_correlated_but_uncapped() {
        assert_eq!(projectile_extraction_dc(0.02, 0.0), 0.0);
        assert!(projectile_extraction_dc(0.30, 0.0) > projectile_extraction_dc(0.10, 1.0));
        assert!(projectile_extraction_dc(0.80, 1.0) > 5.0);
    }

    #[test]
    fn self_treatment_makes_two_preferable_to_four() {
        assert!(
            adventuresim_core::surgery::effective_skill(2.0, false)
                > adventuresim_core::surgery::effective_skill(4.0, true)
        );
    }

    #[test]
    fn untreated_cut_math_is_chunk_invariant() {
        let (whole_cut, whole_exposure) = untreated_cut_progress(0.25, 12.0);
        let (half_cut, first_exposure) = untreated_cut_progress(0.25, 5.0);
        let (chunked_cut, second_exposure) = untreated_cut_progress(half_cut, 7.0);
        assert!((whole_cut - chunked_cut).abs() < 0.000_001);
        assert!((whole_exposure - first_exposure - second_exposure).abs() < 0.000_001);
    }

    #[test]
    fn limb_keys_do_not_wrap_or_alias() {
        assert_ne!(
            injury_id(0, LimbRegion::LeftArm),
            injury_id(1_u64 << 61, LimbRegion::LeftArm)
        );
        assert_ne!(
            injury_id(42, LimbRegion::LeftArm),
            injury_id(42, LimbRegion::RightArm)
        );
    }

    #[test]
    fn fracture_severity_does_not_duplicate_hit_damage() {
        let mut injury = blank_injury(1, LimbRegion::LeftLeg);
        injury.cut_damage = 0.12;
        injury.bruise_damage = 0.38;
        injury.fracture_damage = fracture_from_single_hit(0.38);
        assert!((projected_damage(&injury) - 0.50).abs() < 0.000_001);
    }

    #[test]
    fn blood_recovery_and_bleeding_are_chunk_invariant() {
        let whole = simulate_blood_interval(0.82, &[0.10], 3_000, 0.01);
        let first = simulate_blood_interval(0.82, &[0.10], 1_000, 0.01);
        let second = simulate_blood_interval(first.blood_fraction, &first.open_cuts, 2_000, 0.01);
        assert!((whole.blood_fraction - second.blood_fraction).abs() < 0.000_001);
        assert!((whole.open_cuts[0] - second.open_cuts[0]).abs() < 0.000_001);
        assert!((whole.cut_days[0] - first.cut_days[0] - second.cut_days[0]).abs() < 0.000_001);
    }

    #[test]
    fn bleeding_terminal_boundary_is_stable_across_chunks() {
        let whole = simulate_blood_interval(0.13, &[0.45], MINUTES_PER_DAY, 0.0);
        assert!(whole.terminal);
        let split = whole.elapsed / 2;
        let first = simulate_blood_interval(0.13, &[0.45], split, 0.0);
        let second = simulate_blood_interval(
            first.blood_fraction,
            &first.open_cuts,
            MINUTES_PER_DAY - split,
            0.0,
        );
        assert_eq!(whole.elapsed, first.elapsed + second.elapsed);
        assert_eq!(whole.terminal, second.terminal);
    }

    #[test]
    fn bandages_and_stitches_reduce_standing_exposure() {
        let open = standing_infection_multiplier(false, false, 0.0);
        let bandaged = standing_infection_multiplier(true, false, 0.0);
        let stitched = standing_infection_multiplier(true, true, 4.0);
        assert!(open > bandaged);
        assert!(bandaged > stitched);
        let whole_checks = ((0.13_f32) / STANDING_INFECTION_CHECK_EXPOSURE).floor() as u32;
        let chunked_checks =
            (((0.04_f32 + 0.09_f32) / STANDING_INFECTION_CHECK_EXPOSURE).floor()) as u32;
        assert_eq!(whole_checks, chunked_checks);
    }
}
