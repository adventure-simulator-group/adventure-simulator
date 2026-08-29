//! Durable strategic wounds and the manual, limb-at-a-time treatment reducers.
//!
//! This intentionally records injury categories rather than internal anatomy.
//! Tactical positions and ticks remain transient; only committed hit outcomes
//! cross into these rows.

use adventuresim_core::physiology::BodyRegion;
use adventuresim_core::prelude::*;
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
#[cfg(test)]
use adventuresim_core::surgery::untreated_cut_progress;
use adventuresim_core::surgery::{
    SurgeryProcedure, simulate_blood_interval, standing_infection_multiplier,
};
pub use adventuresim_core::surgery::{
    UNTREATED_CUT_BLOOD_LOSS_PER_DAY, UNTREATED_CUT_DETERIORATION_PER_DAY,
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::character::character;
use crate::{
    CharacterLimbs, character_attributes, character_condition, character_limbs, character_skills,
    character_stats, character_time, infection_episode, inventory_item,
};

pub const BRUISE_HEALING_PER_DAY: f32 = 0.035;
pub const FRACTURE_HEALING_PER_DAY: f32 = 0.0125;
pub const STITCH_HEALING_BONUS_PER_LEVEL: f32 = 0.006;
pub const RETAINED_PROJECTILE_HEALING_MULTIPLIER: f32 = 0.60;
pub const FRACTURE_SINGLE_HIT_THRESHOLD: f32 = 0.18;
pub const STANDING_INFECTION_CHECK_EXPOSURE: f32 = 0.05;
#[derive(Clone, Debug)]
#[table(accessor = treatment_action_receipt)]
pub struct TreatmentActionReceipt {
    #[primary_key]
    pub id: String,
    pub action_id: String,
    pub actor_id: u64,
    pub patient_id: u64,
    pub limb: BodyRegion,
    pub procedure: SurgeryProcedure,
    pub projectile_id: Option<u64>,
    pub use_soap: bool,
    pub context_ref: Option<String>,
    pub expected_membership_revision: Option<u32>,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TreatmentReceiptDisposition {
    New,
    ExactReplay,
    Collision,
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn treatment_receipt_disposition(
    existing: Option<&TreatmentActionReceipt>,
    action_id: &str,
    actor_id: u64,
    patient_id: u64,
    limb: BodyRegion,
    procedure: SurgeryProcedure,
    projectile_id: Option<u64>,
    use_soap: bool,
    context_ref: Option<&str>,
    expected_membership_revision: Option<u32>,
) -> TreatmentReceiptDisposition {
    let Some(existing) = existing else {
        return TreatmentReceiptDisposition::New;
    };
    if existing.action_id == action_id
        && existing.actor_id == actor_id
        && existing.patient_id == patient_id
        && existing.limb == limb
        && existing.procedure == procedure
        && existing.projectile_id == projectile_id
        && existing.use_soap == use_soap
        && existing.context_ref.as_deref() == context_ref
        && existing.expected_membership_revision == expected_membership_revision
        && existing.completed
    {
        TreatmentReceiptDisposition::ExactReplay
    } else {
        TreatmentReceiptDisposition::Collision
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
    pub limb: BodyRegion,
    pub cut_damage: f32,
    pub bruise_damage: f32,
    /// Cold injury is durable tissue damage, independent of impact trauma.
    pub frostbite_damage: f32,
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
    pub limb: BodyRegion,
    pub kind: ProjectileKind,
    /// Extraction is deliberately uncapped. Future ammunition definitions can
    /// multiply this value (for example, a barbed-arrow modifier).
    pub extraction_dc: f32,
    pub source_damage: f32,
}

fn injury_id(character_id: u64, limb: BodyRegion) -> String {
    format!("{character_id}:{}", limb.slug())
}

fn blank_injury(character_id: u64, limb: BodyRegion) -> LimbInjury {
    LimbInjury {
        id: injury_id(character_id, limb),
        character_id,
        limb,
        cut_damage: 0.0,
        bruise_damage: 0.0,
        frostbite_damage: 0.0,
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

pub(crate) fn initialize_character_injuries(ctx: &ReducerContext, character_id: u64) {
    for limb in BodyRegion::ALL {
        ctx.db
            .limb_injury()
            .insert(blank_injury(character_id, limb));
    }
}

pub(crate) fn reset_character_injuries(ctx: &ReducerContext, character_id: u64) {
    for limb in BodyRegion::ALL {
        ctx.db
            .limb_injury()
            .id()
            .update(blank_injury(character_id, limb));
    }
}

fn projected_damage(injury: &LimbInjury) -> f32 {
    (injury.cut_damage + injury.bruise_damage.max(injury.fracture_damage) + injury.frostbite_damage)
        .clamp(0.0, 1.0)
}

pub fn injury_for(ctx: &ReducerContext, character_id: u64, limb: BodyRegion) -> LimbInjury {
    let key = injury_id(character_id, limb);
    ctx.db
        .limb_injury()
        .id()
        .find(key)
        .filter(|row| row.character_id == character_id && row.limb == limb)
        .expect("character injury rows must be initialized at character creation")
}

fn store_injury(ctx: &ReducerContext, injury: LimbInjury) {
    ctx.db.limb_injury().id().update(injury);
}

/// Idempotently seed a real untreated cut for an authored field patient.
pub(crate) fn seed_field_cut(
    ctx: &ReducerContext,
    character_id: u64,
    limb: BodyRegion,
    damage: f32,
    origin_minute: u64,
) {
    let mut injury = injury_for(ctx, character_id, limb);
    if injury.cut_damage <= 0.0 {
        injury.cut_damage = damage.clamp(0.01, 1.0);
        injury.infection_origin_minute = Some(origin_minute);
        store_injury(ctx, injury);
    }
}

fn health_mut(limbs: &mut CharacterLimbs, limb: BodyRegion) -> &mut f32 {
    match limb {
        BodyRegion::LeftArm => &mut limbs.left_arm_health,
        BodyRegion::RightArm => &mut limbs.right_arm_health,
        BodyRegion::LeftLeg => &mut limbs.left_leg_health,
        BodyRegion::RightLeg => &mut limbs.right_leg_health,
        BodyRegion::Chest => &mut limbs.chest_health,
        BodyRegion::Abdomen => &mut limbs.stomach_health,
        BodyRegion::Head => &mut limbs.head_health,
    }
}

fn refresh_limb_projection(ctx: &ReducerContext, character_id: u64, limb: BodyRegion) {
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
    for limb in BodyRegion::ALL {
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
    limb: BodyRegion,
    cut_damage: f32,
    blunt_damage: f32,
    projectile: Option<ProjectileKind>,
) -> Result<(), String> {
    commit_aggregated_hit_injury(
        ctx,
        character_id,
        limb,
        cut_damage,
        blunt_damage,
        blunt_damage,
        projectile,
    )
}

/// Commit bounded aggregate damage while preserving the largest blunt hit as
/// the only fracture-driving value.
pub(crate) fn commit_aggregated_hit_injury(
    ctx: &ReducerContext,
    character_id: u64,
    limb: BodyRegion,
    cut_damage: f32,
    blunt_damage: f32,
    max_single_hit_blunt_damage: f32,
    projectile: Option<ProjectileKind>,
) -> Result<(), String> {
    let mut injury = injury_for(ctx, character_id, limb);
    injury.cut_damage += cut_damage.max(0.0);
    injury.bruise_damage += blunt_damage.max(0.0);
    injury.fracture_damage = (injury.fracture_damage
        + fracture_from_single_hit(max_single_hit_blunt_damage))
    .min(injury.bruise_damage);
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
            adventuresim_core::filth::FilthSubstance::Blood,
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

/// Commit sustained cold injury through the same durable limb authority used
/// by combat and surgery. Frostbite is not a cut, bruise, or fracture and
/// therefore cannot create blood deposits or projectile state.
pub fn commit_frostbite_injury(
    ctx: &ReducerContext,
    character_id: u64,
    limb: BodyRegion,
    damage: f32,
) -> Result<(), String> {
    if !matches!(
        limb,
        BodyRegion::LeftArm | BodyRegion::RightArm | BodyRegion::LeftLeg | BodyRegion::RightLeg
    ) {
        return Err("Frostbite must target a peripheral limb".to_string());
    }
    let mut injury = injury_for(ctx, character_id, limb);
    injury.frostbite_damage = (injury.frostbite_damage + damage.max(0.0)).clamp(0.0, 1.0);
    store_injury(ctx, injury);
    refresh_limb_projection(ctx, character_id, limb);
    Ok(())
}

pub fn fracture_from_single_hit(blunt_damage: f32) -> f32 {
    (blunt_damage.max(0.0) - FRACTURE_SINGLE_HIT_THRESHOLD).max(0.0) * 0.65
}

pub fn projectile_extraction_dc(hit_damage: f32, random_depth: f32) -> f32 {
    adventuresim_core::surgery::projectile_extraction_dc(hit_damage, random_depth)
}

fn has_projectile(ctx: &ReducerContext, character_id: u64, limb: BodyRegion) -> bool {
    ctx.db
        .retained_projectile()
        .character_id()
        .filter(character_id)
        .any(|projectile| projectile.limb == limb)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InjuryRecoveryMinutes(u64);

impl InjuryRecoveryMinutes {
    pub(crate) const NONE: Self = Self(0);

    pub(crate) const fn new(minutes: u64) -> Self {
        Self(minutes)
    }

    const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InjuryPreview {
    pub elapsed: u64,
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InjurySettlement {
    pub elapsed: u64,
    pub alive: bool,
}

/// Side-effect-free preview of the first injury terminal boundary in an
/// interval. Recovery is explicit because only restorative minutes offset
/// blood loss and heal wounds.
pub(crate) fn preview_injury_boundary(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    recovery: InjuryRecoveryMinutes,
) -> Result<InjuryPreview, String> {
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
    let cuts = BodyRegion::ALL
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
    let interval = simulate_blood_interval(
        blood,
        &cuts,
        requested,
        adventuresim_core::morale::BLOOD_RECOVERY_FRACTION_PER_DAY
            * recovery.get().min(requested) as f32
            / requested.max(1) as f32,
    );
    Ok(InjuryPreview {
        elapsed: interval.elapsed,
        terminal: interval.terminal,
    })
}

/// Advance authoritative wounds for one personal-clock interval. This is the
/// sole writer of injury-backed limb health and the sole rest-time blood
/// recovery path, so projections cannot drift from durable wound state.
pub(crate) fn settle_injuries(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed: u64,
    recovery: InjuryRecoveryMinutes,
) -> Result<InjurySettlement, String> {
    if elapsed == 0 {
        return Ok(InjurySettlement {
            elapsed: 0,
            alive: true,
        });
    }
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
    let mut injuries = BodyRegion::ALL.map(|limb| injury_for(ctx, character_id, limb));
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
        adventuresim_core::morale::BLOOD_RECOVERY_FRACTION_PER_DAY
            * recovery.get().min(elapsed) as f32
            / elapsed.max(1) as f32,
    );
    let elapsed_days = interval.elapsed as f32 / MINUTES_PER_DAY as f32;
    let healing_days = recovery.get().min(interval.elapsed) as f32 / MINUTES_PER_DAY as f32;
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
    for (index, limb) in BodyRegion::ALL.into_iter().enumerate() {
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
    // The strategic time owner commits the elapsed frontier exactly once.
    // Surgery settles injury state only; writing CharacterTime here used to
    // double-advance terminal intervals when the caller committed the same
    // elapsed span.
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
    for limb in BodyRegion::ALL {
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
    procedure: SurgeryProcedure,
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
    let equipment = crate::capability::StrategicEquipment::load(ctx, actor_id);
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
    let same_place = actor.current_settlement_id.is_some()
        && actor.current_settlement_id == patient.current_settlement_id
        || crate::world_actor::characters_are_contextually_present(ctx, actor_id, patient_id);
    if !same_place {
        return Err("Surgeon and patient must be together".into());
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
    let disease_plan =
        crate::disease::plan_party_disease_interval(ctx, &participants, duration, true)?;
    let safe_duration = participants.iter().try_fold(duration, |limit, id| {
        let disease = crate::disease::preview_elapsed_for_disease_in_plan(
            ctx,
            *id,
            limit,
            true,
            &disease_plan,
        )?;
        let injury = preview_injury_boundary(ctx, *id, limit, InjuryRecoveryMinutes::new(limit))?;
        Ok::<u64, String>(limit.min(disease).min(injury.elapsed))
    })?;
    let mut completed = safe_duration == duration;
    for id in participants {
        completed &= crate::time::advance_character_wait_time_in_plan(
            ctx,
            id,
            safe_duration,
            &disease_plan,
        )?;
    }
    Ok(completed)
}

fn duration_minutes(procedure: SurgeryProcedure, skill: f32, dc: f32) -> u64 {
    adventuresim_core::surgery::procedure_duration_minutes(procedure, skill, dc)
}

/// Manual surgery. The UI supplies a procedure and optional projectile id;
/// reducers remain authoritative over requirements, clocks, consumption, and
/// hidden infection rolls.
#[reducer]
#[expect(
    clippy::too_many_arguments,
    reason = "the reducer ABI exposes each independently validated treatment field"
)]
pub fn treat_limb(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    limb_slug: String,
    procedure: SurgeryProcedure,
    projectile_id: Option<u64>,
    use_soap: bool,
    action_id: String,
    context_ref: Option<String>,
    expected_membership_revision: Option<u32>,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Treatment action ID is invalid".into());
    }
    let limb = BodyRegion::parse_slug(&limb_slug).ok_or("Unknown limb")?;
    let contextual_claim = match (&context_ref, expected_membership_revision) {
        (None, None) => None,
        (Some(contact_ref), Some(expected_membership_revision))
            if !contact_ref.is_empty() && contact_ref.len() <= 256 =>
        {
            Some(crate::world_actor::ContextualTreatmentClaim {
                contact_ref: contact_ref.clone(),
                expected_membership_revision,
            })
        }
        _ => return Err("Treatment context claim is malformed".into()),
    };
    let receipt_id = format!("treatment:{actor_id}:{action_id}");
    match treatment_receipt_disposition(
        ctx.db
            .treatment_action_receipt()
            .id()
            .find(&receipt_id)
            .as_ref(),
        &action_id,
        actor_id,
        patient_id,
        limb,
        procedure,
        projectile_id,
        use_soap,
        context_ref.as_deref(),
        expected_membership_revision,
    ) {
        TreatmentReceiptDisposition::New => {}
        TreatmentReceiptDisposition::ExactReplay => return Ok(()),
        TreatmentReceiptDisposition::Collision => {
            return Err("Conflicting treatment retry".into());
        }
    }
    require_together(ctx, actor_id, patient_id)?;
    let initial_decision = crate::world_actor::contextual_treatment_decision(
        ctx,
        actor_id,
        patient_id,
        limb,
        procedure,
        contextual_claim.as_ref(),
    );
    match initial_decision {
        adventuresim_core::strategic_action::ContextualActionDecision::Allowed(_) => {}
        adventuresim_core::strategic_action::ContextualActionDecision::Refused => {
            return Err("Treatment was refused".into());
        }
        adventuresim_core::strategic_action::ContextualActionDecision::Unavailable => {
            return Err("Treatment is unavailable".into());
        }
    }
    crate::item::upsert_surgery_items(ctx);
    let skill = procedure_check(ctx, actor_id, patient_id, procedure)?;
    let mut injury = injury_for(ctx, patient_id, limb);
    let projectile = projectile_id.and_then(|id| ctx.db.retained_projectile().id().find(id));
    let dc = match procedure {
        SurgeryProcedure::Bandage if injury.cut_damage > 0.0 && !injury.bandaged => 0.0,
        SurgeryProcedure::Stitch if injury.cut_damage > 0.0 && !injury.stitched => 2.0,
        SurgeryProcedure::Splint
            if injury.fracture_damage > 0.0 && injury.splint_inventory_item_id.is_none() =>
        {
            1.0
        }
        SurgeryProcedure::RemoveSplint if injury.splint_inventory_item_id.is_some() => 0.0,
        SurgeryProcedure::Extract
            if projectile
                .as_ref()
                .is_some_and(|p| p.character_id == patient_id && p.limb == limb) =>
        {
            projectile.as_ref().unwrap().extraction_dc
        }
        SurgeryProcedure::Bandage => return Err("This limb does not need bandaging".into()),
        SurgeryProcedure::Stitch => return Err("This wound cannot be stitched".into()),
        SurgeryProcedure::Splint => {
            return Err("This limb has no unsplinted fracture".into());
        }
        SurgeryProcedure::RemoveSplint => return Err("This limb is not splinted".into()),
        SurgeryProcedure::Extract => return Err("Projectile not found in this limb".into()),
        SurgeryProcedure::OpenBody => {
            return Err("This procedure is available only for dead subjects".into());
        }
    };
    if skill < dc {
        return Err(format!(
            "Insufficient procedure skill: this procedure requires {dc:.1}"
        ));
    }
    if procedure == SurgeryProcedure::Stitch && item_quantity(ctx, actor_id, "surgery_kit") == 0 {
        return Err("Stitching requires a surgery kit".into());
    }
    if procedure == SurgeryProcedure::Extract
        && adventuresim_core::surgery::extraction_requires_surgery_kit(dc)
        && item_quantity(ctx, actor_id, "surgery_kit") == 0
    {
        return Err("Extracting a projectile above DC 1 requires a surgery kit".into());
    }
    if procedure == SurgeryProcedure::Bandage && item_quantity(ctx, actor_id, "bandage") == 0 {
        return Err("Bandaging requires one bandage".into());
    }
    if procedure == SurgeryProcedure::Splint && available_splints(ctx, actor_id) == 0 {
        return Err("Applying a splint requires one splint".into());
    }
    let soap_applicable = matches!(
        procedure,
        SurgeryProcedure::Bandage | SurgeryProcedure::Stitch | SurgeryProcedure::Extract
    );
    let soap_available = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((actor_id, adventuresim_core::item_references::SOFT_SOAP_ID))
        .any(|row| {
            crate::inventory_amount::personal_fraction(ctx, row.id).unwrap_or_default()
                >= crate::filth::SOAP_FRACTION_PER_CLEANSING_POINT
        });
    if use_soap && (!soap_applicable || !soap_available) {
        return Err("The selected procedure cannot use an available unit of soap".into());
    }
    let duration = duration_minutes(procedure, skill, dc);
    if !align_and_advance(ctx, actor_id, patient_id, duration)? {
        return Ok(());
    }
    require_together(ctx, actor_id, patient_id)?;
    match crate::world_actor::contextual_treatment_decision(
        ctx,
        actor_id,
        patient_id,
        limb,
        procedure,
        contextual_claim.as_ref(),
    ) {
        adventuresim_core::strategic_action::ContextualActionDecision::Allowed(_) => {}
        adventuresim_core::strategic_action::ContextualActionDecision::Refused => {
            return Err("Treatment was refused before it could be committed".into());
        }
        adventuresim_core::strategic_action::ContextualActionDecision::Unavailable => {
            return Err("Treatment became unavailable before it could be committed".into());
        }
    }
    injury = injury_for(ctx, patient_id, limb);
    let selected_alcohol = soap_applicable
        .then(|| crate::alcohol::best_disinfectant(ctx, actor_id))
        .flatten();
    if use_soap {
        let soap = ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((actor_id, adventuresim_core::item_references::SOFT_SOAP_ID))
            .find(|row| {
                crate::inventory_amount::personal_fraction(ctx, row.id).unwrap_or_default()
                    >= crate::filth::SOAP_FRACTION_PER_CLEANSING_POINT
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
    match procedure {
        SurgeryProcedure::Bandage => {
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
        SurgeryProcedure::Stitch => {
            injury.stitched = true;
            injury.stitch_quality = skill;
            crate::disease::record_committed_cut(
                ctx,
                patient_id,
                injury.cut_damage * 0.12,
                clean_check,
            )?;
        }
        SurgeryProcedure::Splint => {
            injury.splint_owner_id = Some(actor_id);
            injury.splint_inventory_item_id = Some(equip_splint(ctx, actor_id, patient_id)?);
        }
        SurgeryProcedure::RemoveSplint => {
            return_splint(ctx, &mut injury)?;
        }
        SurgeryProcedure::Extract => {
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
        SurgeryProcedure::OpenBody => unreachable!("live-patient procedure was rejected"),
    }
    let exposure =
        adventuresim_core::surgery::procedure_blood_exposure(procedure, actor_id != patient_id);
    if exposure > 0 {
        crate::filth::deposit_now(
            ctx,
            actor_id,
            adventuresim_core::filth::FilthSubstance::Blood,
            Some(patient_id),
            exposure,
        )?;
    }
    store_injury(ctx, injury);
    refresh_limb_projection(ctx, patient_id, limb);
    let patient_survived = ctx
        .db
        .character()
        .id()
        .find(patient_id)
        .is_some_and(|character| character.alive);
    if patient_survived {
        crate::capability::refresh_character_capability(ctx, patient_id)?;
    }
    ctx.db
        .treatment_action_receipt()
        .insert(TreatmentActionReceipt {
            id: receipt_id,
            action_id,
            actor_id,
            patient_id,
            limb,
            procedure,
            projectile_id,
            use_soap,
            context_ref,
            expected_membership_revision,
            completed: true,
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treatment_authorization_precedes_mutation_and_is_revalidated() {
        let source = crate::production_source(include_str!("surgery.rs"));
        let reducer = source
            .split("pub fn treat_limb")
            .nth(1)
            .expect("treatment reducer");
        let first_decision = reducer
            .find("contextual_treatment_decision")
            .expect("initial treatment decision");
        assert!(first_decision < reducer.find("upsert_surgery_items").expect("item upsert"));
        assert!(reducer[first_decision + 1..].contains("contextual_treatment_decision"));
        assert!(reducer.contains("Treatment was refused"));
        assert!(reducer.contains("Treatment is unavailable"));
    }

    #[test]
    fn treatment_receipts_bind_exact_requests_and_retries() {
        let source = crate::production_source(include_str!("surgery.rs"));
        let binding = source
            .split("fn treatment_receipt_disposition")
            .nth(1)
            .and_then(|tail| tail.split("impl BodyRegion").next())
            .expect("treatment receipt binding");
        let reducer = source
            .split("pub fn treat_limb")
            .nth(1)
            .expect("treatment reducer");
        for coordinate in [
            "existing.actor_id == actor_id",
            "existing.patient_id == patient_id",
            "existing.limb == limb",
            "existing.procedure == procedure",
            "existing.projectile_id == projectile_id",
            "existing.use_soap == use_soap",
        ] {
            assert!(binding.contains(coordinate), "missing {coordinate}");
        }
        assert!(reducer.contains("Conflicting treatment retry"));
        assert!(reducer.contains("treatment_action_receipt()"));
        assert!(reducer.contains(".insert(TreatmentActionReceipt"));
        let interrupted = reducer
            .split("if !align_and_advance")
            .nth(1)
            .and_then(|tail| tail.split("require_together").next())
            .expect("interrupted treatment branch");
        assert!(!interrupted.contains("treatment_action_receipt"));
    }

    #[test]
    fn receipt_state_machine_distinguishes_replay_collision_and_new_attempt() {
        let receipt = TreatmentActionReceipt {
            id: "treatment:7:token-a".into(),
            action_id: "token-a".into(),
            actor_id: 7,
            patient_id: 8,
            limb: BodyRegion::RightLeg,
            procedure: SurgeryProcedure::Bandage,
            projectile_id: None,
            use_soap: true,
            context_ref: Some("road:1".into()),
            expected_membership_revision: Some(4),
            completed: true,
        };
        let disposition = |existing, token: &str, soap| {
            treatment_receipt_disposition(
                existing,
                token,
                7,
                8,
                BodyRegion::RightLeg,
                SurgeryProcedure::Bandage,
                None,
                soap,
                Some("road:1"),
                Some(4),
            )
        };
        assert_eq!(
            disposition(Some(&receipt), "token-a", true),
            TreatmentReceiptDisposition::ExactReplay
        );
        assert_eq!(
            disposition(Some(&receipt), "token-a", false),
            TreatmentReceiptDisposition::Collision
        );
        assert_eq!(
            disposition(None, "token-b", true),
            TreatmentReceiptDisposition::New
        );

        let mut incomplete = receipt.clone();
        incomplete.completed = false;
        assert_eq!(
            disposition(Some(&incomplete), "token-a", true),
            TreatmentReceiptDisposition::Collision
        );
    }

    #[test]
    fn fracture_uses_single_hit_threshold() {
        assert_eq!(fracture_from_single_hit(0.18), 0.0);
        assert!((fracture_from_single_hit(0.38) - 0.13).abs() < 0.0001);
    }

    #[test]
    fn frostbite_is_distinct_additive_tissue_damage() {
        let mut injury = blank_injury(7, BodyRegion::LeftArm);
        injury.bruise_damage = 0.1;
        injury.frostbite_damage = 0.2;
        assert!((projected_damage(&injury) - 0.3).abs() < 0.0001);
        assert_eq!(injury.cut_damage, 0.0);
        assert_eq!(injury.fracture_damage, 0.0);
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
            injury_id(0, BodyRegion::LeftArm),
            injury_id(1_u64 << 61, BodyRegion::LeftArm)
        );
        assert_ne!(
            injury_id(42, BodyRegion::LeftArm),
            injury_id(42, BodyRegion::RightArm)
        );
    }

    #[test]
    fn fracture_severity_does_not_duplicate_hit_damage() {
        let mut injury = blank_injury(1, BodyRegion::LeftLeg);
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
    fn injury_terminal_preview_is_side_effect_free() {
        let source = crate::production_source(include_str!("surgery.rs"));
        let preview = source
            .split("pub(crate) fn preview_injury_boundary")
            .nth(1)
            .and_then(|tail| tail.split("/// Advance authoritative wounds").next())
            .expect("injury terminal preview");
        for forbidden in [
            "apply_blood_loss",
            "initialize_character_condition",
            "store_injury",
            ".insert(",
            ".update(",
            ".delete(",
            "refresh",
            "death",
        ] {
            assert!(!preview.contains(forbidden), "preview contains {forbidden}");
        }
    }

    #[test]
    fn injury_interval_api_has_one_explicit_recovery_path() {
        let source = crate::production_source(include_str!("surgery.rs"));
        let preview = source
            .split("pub(crate) fn preview_injury_boundary")
            .nth(1)
            .and_then(|tail| tail.split("/// Advance authoritative wounds").next())
            .expect("canonical injury preview");
        assert!(preview.contains("recovery: InjuryRecoveryMinutes"));
        assert!(preview.contains("Result<InjuryPreview, String>"));

        let settlement = source
            .split("pub(crate) fn settle_injuries")
            .nth(1)
            .and_then(|tail| tail.split("pub fn convalescence_minutes").next())
            .expect("canonical injury settlement");
        assert!(settlement.contains("recovery: InjuryRecoveryMinutes"));
        assert!(!settlement.contains("allow_healing"));

        let preparation = crate::food::FOOD_SOURCE
            .split("fn preparation_terminal_minute")
            .nth(1)
            .and_then(|tail| tail.split("fn next_preparation_attempt_generation").next())
            .expect("preparation terminal preview");
        assert!(preparation.contains("preview_injury_boundary"));
        assert!(preparation.contains("InjuryRecoveryMinutes::new(duration)"));
    }

    #[test]
    fn current_injury_rows_are_created_once_then_updated() {
        let source = crate::production_source(include_str!("surgery.rs"));
        let initializer = source
            .split("pub(crate) fn initialize_character_injuries")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn reset_character_injuries").next())
            .expect("injury row initializer");
        assert!(initializer.contains(".insert(blank_injury(character_id, limb))"));

        let lookup_and_store = source
            .split("pub fn injury_for")
            .nth(1)
            .and_then(|tail| tail.split("/// Idempotently seed").next())
            .expect("injury lookup and store");
        assert!(lookup_and_store.contains("initialized at character creation"));
        assert!(!lookup_and_store.contains("unwrap_or_else"));
        assert!(!lookup_and_store.contains(".insert("));
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
